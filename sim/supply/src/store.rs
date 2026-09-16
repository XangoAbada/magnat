//! Magazyn: sloty, FEFO, wycena po koszcie, psucie (M6a §5.2–5.3, WP2).
//!
//! Podział pliku: tutaj **stan** — sloty, arena, księgi i odcisk; [`ops`] trzyma operacje
//! magazynowe (`put`, `reserve`, `take`, `spoil`, `merge_in_slot`), [`invariants`] —
//! niezmienniki, które te operacje mają zachowywać. Trzy tematy, trzy pliki.
//!
//! **Arena partii mieszka tutaj, wewnątrz zasobu, a nie osobno.** `K-29` dopuszcza to
//! wprost, pod warunkiem że arena wchodzi do hasha przez `Arena::hash_state` (kolejność
//! indeksów, sloty i generacje), a zasób jest wpięty przez `register_resource_hash`.
//! Powód jest po stronie ECS, nie wygody: `World::resource_mut` pożycza **cały** świat,
//! więc komponentu i zasobu nie da się zmutować naraz — a każda operacja magazynowa
//! dotyka i slotu, i partii. Sekcja hasha zmienia się z „areny wg `ArenaKind`" na
//! „zasoby wg nazwy typu" i to jest cała różnica. `ArenaKind::Batches` zostaje
//! zarezerwowane i niezmienne.
//!
//! Dwa niezmienniki, których pilnują testy własnościowe:
//!
//! - **masa**: `produced + imported + initial == consumed + exported + Σ losses + stock`,
//!   tolerancja 0 g, a każda strata ma kategorię [`LossKind`] — masa znikająca bez
//!   kategorii jest błędem testu, nie zaokrągleniem;
//! - **pieniądz**: `Σ cost_total żywych partii == paid_in − cogs − write_offs`,
//!   tolerancja 0 gr.

use magnat_core::{
    Arena, FirmId, GoodId, HashState, Mass, Money, SimMinute, SiteId, StateHasher, Volume,
    LOSS_KIND_COUNT, Q,
};

use crate::batch::{Batch, BatchFlags, BatchId, BatchLedger, BatchOrigin, BrandId, SlotId};
use crate::catalog::{Catalog, StorageClass};

mod aging;
mod invariants;
mod ops;
mod retail;
pub use aging::{Spoiled, BATCH_HARD_LIMIT, BATCH_SOFT_LIMIT};
pub use retail::ShelfState;
#[cfg(test)]
mod tests_support;
mod transit;

/// Rola magazynu w zakładzie.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum WarehouseRole {
    /// Magazyn wejściowy — surowce przed linią.
    Input,
    /// Magazyn wyjściowy — wyrób gotowy przed wysyłką.
    Output,
    /// Zaplecze sklepu.
    Backroom,
    /// Półka. **Wyłączona** z `stock_fill` zakładu (M6 §6.4.3) — ma własny widok.
    Shelf,
    /// Centrum dystrybucyjne: towar, który tu stoi, nie jest ani wsadem, ani wyrobem —
    /// jest w drodze i czeka na przeładunek (WP12).
    ///
    /// Dopisane **na końcu** enuma z rozmysłu (`AG-3`): kolejność wariantów jest
    /// kontraktem zapisu gry, bo `role` siedzi w slocie magazynowym, a slot wchodzi
    /// do hasha stanu. Wstawienie `Distribution` w środku przenumerowałoby każdy
    /// magazyn w każdym zapisanym świecie.
    ///
    /// `Tank` z §5.5 nie powstaje i to jest właściwe: zbiornik jest **klasą
    /// przechowywania** ([`crate::StorageClass::Tank`], istnieje), a nie rolą magazynu —
    /// rola odpowiada na pytanie „po co ten magazyn stoi", a nie „co w nim trzymamy".
    Distribution,
}

/// Skąd masa weszła do świata. Bez tej kategorii bilans masy nie da się domknąć,
/// bo `put` nie wie, czy partia właśnie powstała, czy przyjechała zza granicy.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MassIn {
    Produced,
    Imported,
    /// Zapas startowy świata. **Nie jest źródłem w grafie produktów** (walidator tego
    /// pilnuje) — jest wyłącznie pozycją otwarcia bilansu.
    Initial,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StoreError {
    /// Slot nie przyjmuje tej klasy niebezpieczeństwa.
    HazardNotAllowed,
    /// Nie mieści się — masa albo objętość.
    OutOfCapacity,
    UnknownSlot,
    /// Uchwyt partii z rezerwacji przestał być ważny między `reserve` a `take`.
    StaleReservation,
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::HazardNotAllowed => write!(f, "slot nie przyjmuje tej klasy ładunku"),
            StoreError::OutOfCapacity => write!(f, "slot nie ma miejsca"),
            StoreError::UnknownSlot => write!(f, "nieznany slot"),
            StoreError::StaleReservation => write!(f, "rezerwacja odwołuje się do martwej partii"),
        }
    }
}

impl std::error::Error for StoreError {}

/// Slot magazynowy.
///
/// Lista partii jest trzymana **w porządku FEFO** (`first expired, first out`): rosnąco
/// po dacie przydatności, a przy remisie po indeksie w arenie. Nietrwałe stoją przed
/// trwałymi. Porządek jest niezmiennikiem wstawiania, nie wynikiem sortowania przy
/// wydaniu — dzięki temu `take` jest liniowy po tym, co faktycznie bierze.
#[derive(Clone, Debug)]
pub struct StorageSlot {
    pub site: SiteId,
    pub role: WarehouseRole,
    pub class: StorageClass,
    /// Maska dozwolonych klas niebezpieczeństwa — bity z [`HazardClass::bit`].
    pub hazard_mask: u8,
    pub cap_mass: Mass,
    pub cap_volume: Volume,
    used_mass: Mass,
    used_volume: Volume,
    batches: Vec<BatchId>,
}

impl StorageSlot {
    #[must_use]
    pub const fn used_mass(&self) -> Mass {
        self.used_mass
    }

    #[must_use]
    pub const fn used_volume(&self) -> Volume {
        self.used_volume
    }

    #[must_use]
    pub fn batches(&self) -> &[BatchId] {
        &self.batches
    }
}

/// Plan wydania: które partie i ile z każdej. Powstaje w [`Store::reserve`],
/// zużywa go [`Store::take`].
#[derive(Clone, Debug)]
pub struct Reservation {
    pub slot: SlotId,
    pub good: GoodId,
    pub mass: Mass,
    items: Vec<(BatchId, Mass)>,
}

/// Po co magazyn wydaje towar — rozstrzyga etap śladu i pokrewieństwo partii.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TakeKind {
    /// Wsad linii produkcyjnej: wydana partia zostaje rodzicem wyrobu.
    Consume,
    /// Sprzedaż z półki.
    Sell,
    /// Odpis z kategorią straty.
    Loss(magnat_core::LossKind),
    /// Przełożenie w obrębie zakładu albo wyjazd poza miasto.
    Move,
}

impl TakeKind {
    pub(crate) fn trace_kind(self) -> crate::batch::TraceKind {
        match self {
            TakeKind::Consume => crate::batch::TraceKind::Consumed,
            TakeKind::Sell => crate::batch::TraceKind::Sold,
            TakeKind::Loss(k) => crate::batch::TraceKind::Lost(k),
            TakeKind::Move => crate::batch::TraceKind::Loaded,
        }
    }
}

/// Wynik wydania: masa, jakość ważona masą, koszt własny i marka najstarszej partii.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BatchSlice {
    /// Kto to wytworzył — pierwsza partia wydania. Niesione, bo transakcja detaliczna
    /// pokazuje producenta w karcie, a zwrot nieudanej sprzedaży musi oddać partię
    /// z tym samym producentem, a nie z zaślepką.
    pub producer: magnat_core::FirmId,
    pub good: GoodId,
    pub mass: Mass,
    pub quality: Q,
    pub cost_total: Money,
    pub brand: Option<BrandId>,
    /// Najwcześniejsza data przydatności w wydanej porcji.
    pub expires_at: Option<SimMinute>,
    /// Pochodzenie wydanej porcji: głębokość jako **maksimum**, zakład, receptura
    /// i złoże tylko wtedy, gdy **wszystkie** wydane partie niosą to samo. Dodane
    /// w M6b, bo bez tego linia produkcyjna nie ma z czego zbudować `BatchOrigin`
    /// wyrobu i ślad „od pola do półki" urywałby się na pierwszym przetworzeniu.
    pub origin: BatchOrigin,
}

/// Wiersz bilansu masy jednego towaru.
#[derive(Clone, Copy, Debug, Default)]
struct MassRow {
    produced: i64,
    imported: i64,
    initial: i64,
    consumed: i64,
    exported: i64,
    losses: [i64; LOSS_KIND_COUNT],
}

/// Magazyn świata: arena partii, sloty i obie księgi kontrolne.
///
/// Bez `Clone` i bez `Debug` z rozmysłu: `Arena<T>` z M0 nie ma ani jednego, ani drugiego,
/// a magazyn metropolii to 600 tys. partii — klon byłby pułapką wydajnościową, a `Debug`
/// wypisałby pięćdziesiąt megabajtów w komunikacie testu.
#[derive(Default)]
pub struct Store {
    batches: Arena<Batch>,
    slots: Vec<StorageSlot>,
    ledger: BatchLedger,
    /// Minuta symulacji, którą magazyn stempluje zdarzenia śladu.
    ///
    /// Pole, a nie argument dwunastu metod: `load`, `unload`, `split` i `shelf_pick`
    /// nie mają powodu znać zegara — poza jednym, czysto sprawozdawczym. Ustawia je
    /// `Chain::step_minute` raz na minutę, więc wartość jest zawsze zegarem bieżącego
    /// ticku, a kod symulacji i tak jej nie czyta (00 §3.5 dotyczy czasu **realnego**).
    now: SimMinute,
    /// Partie `TRACED` pobrane z magazynu zakładu i jeszcze nieprzypisane do wyrobu.
    ///
    /// `ponytail:` klucz to sam zakład, nie linia. Sufit nazwany: dwie linie tego
    /// samego zakładu przerabiające **śledzone** partie w zachodzących na siebie
    /// szarżach podzieliłyby się rodzicami. Flagę `TRACED` nadaje się ręcznie
    /// kilkunastu partiom, więc przypadek jest osiągalny wyłącznie umyślnie; droga
    /// wyjścia to klucz `(zakład, linia)`, kiedy `Charge` zacznie nieść numer linii.
    ///
    /// Minuta w kluczu **nie jest ozdobą**: receptura ma kilka wyjść (rafineria ma
    /// osiem), a wpis skasowany przy pierwszym z nich zostawiłby pozostałe siedem bez
    /// rodziców — i bez flagi śladu, bo ona dziedziczy się tą samą drogą. Ślad diesla
    /// urywał się wtedy na benzynie, czyli na wyjściu, które akurat stało pierwsze
    /// w pliku. Wpisy nie znikają po użyciu; zastępuje je następna szarża tego zakładu.
    pending_parents: Vec<(magnat_core::SiteId, SimMinute, BatchId)>,
    mass: Vec<MassRow>,
    paid_in: Money,
    cogs: Money,
    write_offs: Money,
}

impl Store {
    #[must_use]
    pub fn new(goods: usize) -> Store {
        Store {
            mass: vec![MassRow::default(); goods],
            ..Store::default()
        }
    }

    #[must_use]
    pub fn batch(&self, b: BatchId) -> Option<&Batch> {
        self.batches.get(b)
    }

    #[must_use]
    pub fn slot(&self, s: SlotId) -> Option<&StorageSlot> {
        self.slots.get(s.0 as usize)
    }

    #[must_use]
    pub fn ledger(&self) -> &BatchLedger {
        &self.ledger
    }

    /// Ustawia zegar stempla śladu. Woła to kadencja łańcucha raz na minutę.
    pub fn set_now(&mut self, now: SimMinute) {
        self.now = now;
    }

    #[must_use]
    pub fn now(&self) -> SimMinute {
        self.now
    }

    /// Ile slotów areny partii zajęto od startu świata — **maksimum historyczne**,
    /// bo `Arena` z M0 nigdy nie kompaktuje (`K-16`: uchwyt po zwolnieniu ma nigdy
    /// nie być ponownie ważny, więc slot wraca do obiegu razem z generacją, a tablica
    /// nie maleje). Miara budżetu pamięci WP15: to ona, a nie
    /// [`Store::live_batches`], mnoży się przez rozmiar partii.
    /// Obejmuje partię śladem — **to jest punkt wejścia gracza** z §6.4.2: „partie
    /// wskazane ręcznie w UI".
    ///
    /// Ślad zaczyna się w chwili wskazania, a nie wstecz, i to jest uczciwe: historii,
    /// której dziennik nie zapisał, nie da się odtworzyć, a dopisanie jej byłoby
    /// zmyślaniem. Etap `Stored` zapisany tutaj mówi prawdę — „w tej minucie partia
    /// leżała w tym slocie" — i od niego ślad rośnie w przód, razem z flagą, która
    /// dziedziczy się na wszystko, co z tej partii powstanie.
    pub fn mark_traced(&mut self, id: BatchId) -> bool {
        let Some(b) = self.batches.get_mut(id) else {
            return false;
        };
        if b.flags.has(BatchFlags::TRACED) {
            return true;
        }
        b.flags.set(BatchFlags::TRACED);
        let site = match self.batches.get(id).map(|b| b.location) {
            Some(crate::batch::BatchLocation::Slot(s)) => self.site_of(s),
            _ => None,
        };
        self.zapisz(id, crate::batch::TraceKind::Stored, site);
        true
    }

    /// Sloty magazynowe jednego zakładu, w kolejności indeksów.
    pub fn slots_of(&self, site: SiteId) -> impl Iterator<Item = &StorageSlot> {
        self.slots.iter().filter(move |s| s.site == site)
    }

    #[must_use]
    pub fn arena_slots(&self) -> usize {
        self.batches.slot_count()
    }

    /// Uchwyty wszystkich żywych partii, w kolejności indeksów w arenie.
    ///
    /// Dla testów własnościowych i dla migawki — kod symulacji dociera do partii
    /// przez magazyn, pojazd albo półkę, nigdy przekrojowo (`K-16`).
    pub fn all_handles(&self) -> impl Iterator<Item = BatchId> + '_ {
        self.batches.iter().map(|(h, _)| h)
    }

    #[must_use]
    pub fn live_batches(&self) -> usize {
        self.batches.len()
    }

    /// Nowy slot. Zwraca uchwyt; sloty nie znikają, więc indeks jest stabilny.
    pub fn add_slot(
        &mut self,
        site: SiteId,
        role: WarehouseRole,
        class: StorageClass,
        cap_mass: Mass,
        cap_volume: Volume,
        hazard_mask: u8,
    ) -> SlotId {
        let id = SlotId(self.slots.len() as u32);
        self.slots.push(StorageSlot {
            site,
            role,
            class,
            hazard_mask,
            cap_mass,
            cap_volume,
            used_mass: Mass::ZERO,
            used_volume: Volume::ZERO,
            batches: Vec::new(),
        });
        id
    }

    /// Wolna pojemność slotu: (masa, objętość).
    #[must_use]
    pub fn free_capacity(&self, s: SlotId) -> (Mass, Volume) {
        self.slot(s).map_or((Mass::ZERO, Volume::ZERO), |sl| {
            (
                Mass((sl.cap_mass.0 - sl.used_mass.0).max(0)),
                Volume((sl.cap_volume.0 - sl.used_volume.0).max(0)),
            )
        })
    }

    /// Masa towaru w slocie.
    #[must_use]
    pub fn stock_of(&self, s: SlotId, good: GoodId) -> Mass {
        let Some(sl) = self.slot(s) else {
            return Mass::ZERO;
        };
        Mass(
            sl.batches
                .iter()
                .filter_map(|b| self.batches.get(*b))
                .filter(|b| b.good == good)
                .map(|b| b.mass.0)
                .sum(),
        )
    }

    /// Masa towaru we wszystkich slotach — lewa strona niezmiennika bilansu.
    #[must_use]
    pub fn total_stock(&self, good: GoodId) -> Mass {
        Mass(
            self.batches
                .iter()
                .filter(|(_, b)| b.good == good)
                .map(|(_, b)| b.mass.0)
                .sum(),
        )
    }

    /// Wstawia nową partię do slotu.
    ///
    /// `source` mówi, **skąd masa wzięła się w świecie** — bez tego bilans nie ma pozycji
    /// otwarcia. `cost` to kwota faktycznie zapłacona (albo koszt wytworzenia): wchodzi
    fn przelicz_objetosc(&mut self, slot: SlotId) {
        let suma: i64 = self.slots[slot.0 as usize]
            .batches
            .iter()
            .filter_map(|b| self.batches.get(*b))
            .map(|b| b.volume.0)
            .sum();
        self.slots[slot.0 as usize].used_volume = Volume(suma);
    }

    /// Zapisuje etap śladu — **tylko** dla partii z flagą `TRACED`.
    ///
    /// `site` jest miejscem, w którym etap się wydarzył, a nie zakładem pochodzenia:
    /// do M6d dziennik stemplował wszystko `origin.site`, więc oś czasu bochenka
    /// pokazywała piekarnię także przy rozładunku w sklepie. Panel „od pola do półki"
    /// pyta „gdzie **wtedy** był", nie „skąd pochodzi".
    fn zapisz(&mut self, id: BatchId, kind: crate::batch::TraceKind, site: Option<SiteId>) {
        let masa = self.batches.get(id).map(|b| b.mass);
        self.zapisz_masa(id, kind, site, masa);
    }

    /// Etap śladu o **jawnej** masie — dla wydania, w którym z partii schodzi część.
    ///
    /// Bez tego etap `Consumed` opisywał resztę zostającą w magazynie, a nie porcję,
    /// która poszła na linię: partia zjedzona w całości zostawiała w śladzie zero
    /// gramów. Oś czasu w panelu pokazywałaby wtedy „zużyto 0 kg mąki", czyli
    /// dokładnie odwrotność tego, co się stało.
    fn zapisz_masa(
        &mut self,
        id: BatchId,
        kind: crate::batch::TraceKind,
        site: Option<SiteId>,
        mass: Option<Mass>,
    ) {
        let Some(b) = self.batches.get(id) else {
            return;
        };
        if !b.flags.has(BatchFlags::TRACED) {
            return;
        }
        let e = crate::batch::BatchEvent {
            batch: id,
            at: self.now,
            site: site.or(b.origin.site),
            kind,
            mass: mass.unwrap_or(b.mass),
            cost_cumulative: b.cost_total,
            quality: b.quality,
        };
        self.ledger.push(e);
    }

    /// Zakład, do którego należy slot.
    fn site_of(&self, slot: SlotId) -> Option<SiteId> {
        self.slots.get(slot.0 as usize).map(|s| s.site)
    }

    /// Etap „przeterminowane" — ostatni w śladzie partii, która nie doszła do klienta.
    fn zapisz_odpis(&mut self, id: BatchId, slot: SlotId) {
        let site = self.site_of(slot);
        self.zapisz(
            id,
            crate::batch::TraceKind::Lost(magnat_core::LossKind::Expired),
            site,
        );
    }
}

impl HashState for Store {
    /// Arena partii wchodzi **w kolejności indeksów**, ze slotami i generacjami —
    /// wymaganie 1 z `K-16`. Sloty magazynowe po indeksie, bo nie znikają.
    fn hash_state(&self, h: &mut StateHasher) {
        self.batches.hash_state(h);
        h.write_u32(self.slots.len() as u32);
        for s in &self.slots {
            s.site.entity().hash_state(h);
            h.write_u8(s.role as u8);
            h.write_u8(s.class as u8);
            h.write_u8(s.hazard_mask);
            s.cap_mass.hash_state(h);
            s.cap_volume.hash_state(h);
            s.used_mass.hash_state(h);
            s.used_volume.hash_state(h);
            h.write_u32(s.batches.len() as u32);
            for b in &s.batches {
                b.hash_state(h);
            }
        }
        self.paid_in.hash_state(h);
        self.cogs.hash_state(h);
        self.write_offs.hash_state(h);
        h.write_u32(self.mass.len() as u32);
        for r in &self.mass {
            h.write_i64(r.produced);
            h.write_i64(r.imported);
            h.write_i64(r.initial);
            h.write_i64(r.consumed);
            h.write_i64(r.exported);
            for l in &r.losses {
                h.write_i64(*l);
            }
        }
    }
}

/// Opis nowej partii — to, co wie wołający, zanim katalog dołoży objętość i datę.
#[derive(Clone, Copy, Debug)]
pub struct BatchDraft {
    pub good: GoodId,
    pub mass: Mass,
    pub quality: Q,
    pub brand: Option<BrandId>,
    pub producer: FirmId,
    pub produced_at: SimMinute,
    pub cost: Money,
    pub origin: BatchOrigin,
    pub flags: BatchFlags,
}

fn qty_of(cat: &Catalog, good: GoodId, mass: Mass) -> magnat_core::Qty {
    let g = cat.good(good);
    if g.form.is_bulk() || g.unit_mass.0 <= 0 {
        return magnat_core::Qty::ZERO;
    }
    magnat_core::Qty((i128::from(mass.0) * 1000 / i128::from(g.unit_mass.0)) as i64)
}

/// Klucz porządku FEFO: data przydatności, a przy remisie indeks w arenie. Partia bez
/// daty idzie na koniec — trwałe wydaje się dopiero wtedy, gdy nietrwałych już nie ma.
fn klucz_fefo(arena: &Arena<Batch>, b: BatchId) -> (u64, u32) {
    let e = arena
        .get(b)
        .and_then(|x| x.expires_at)
        .map_or(u64::MAX, |x| x.0);
    (e, b.index())
}

fn wstaw_fefo(lista: &mut Vec<BatchId>, arena: &Arena<Batch>, nowa: BatchId) {
    let k = klucz_fefo(arena, nowa);
    let poz = lista.partition_point(|b| klucz_fefo(arena, *b) < k);
    lista.insert(poz, nowa);
}

impl Store {
    /// Wartość zapasu w slocie — suma kosztów nabycia partii.
    ///
    /// Lewa strona niezmiennika P5 po stronie detalu (M5c): zapas w bilansie ma mieć
    /// pokrycie w tym, co za niego zapłacono. Do WP11 liczyło się ją z linii zapasu;
    /// teraz linii nie ma, a partie są.
    #[must_use]
    pub fn slot_value(&self, slot: SlotId) -> Money {
        let Some(sl) = self.slots.get(slot.0 as usize) else {
            return Money::ZERO;
        };
        Money(
            sl.batches
                .iter()
                .filter_map(|b| self.batches.get(*b))
                .map(|b| b.cost_total.0)
                .sum(),
        )
    }
}
