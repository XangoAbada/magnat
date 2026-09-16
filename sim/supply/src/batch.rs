//! Partia — jednostka masy z tożsamością (M6a §5.2, WP2).
//!
//! **K-16: partie żyją w dedykowanej arenie, nie w ECS.** Uzasadnienie jest skalą
//! i rotacją: partii jest rzędu 600 tys., powstają i giną w każdym ticku, a żadna nie
//! jest nigdy odpytywana przekrojowo po archetypach — do partii dociera się zawsze przez
//! magazyn, pojazd albo półkę. Płacenie za nie mutacją strukturalną ECS to koszt bez
//! korzyści. `Arena<T>` dostarcza M0; M6 nie pisze własnej.
//!
//! Cztery warunki nienegocjowalne z `K-16`:
//!
//! 1. Arena wchodzi do funkcji haszującej stan, **w kolejności indeksów** — robi to
//!    `Arena::hash_state`, wołane przez [`crate::Store`].
//! 2. Uchwyt po zwolnieniu nigdy nie staje się ponownie ważny — pilnuje tego generacja
//!    w `ArenaHandle`.
//! 3. Snapshot traktuje arenę jak sekcję ECS.
//! 4. Kompaktowanie areny nie zmienia indeksów żywych partii w trakcie ticku — `Arena`
//!    z M0 **nigdy** nie kompaktuje, więc warunek jest spełniony konstrukcyjnie.

use magnat_core::{
    ArenaHandle, DepositId, FirmId, GoodId, HashState, LossKind, Mass, Money, RecipeId,
    SimMinute, SiteId, StateHasher, Volume, Q,
};

/// Uchwyt partii. `{ index: u32, generation: NonZeroU32 }`, 8 bajtów.
pub type BatchId = ArenaHandle<Batch>;

/// Klucz agregacji partii (WP15): `(towar, kubełek jakości, marka, producent, doba
/// przydatności)`. Krotka, nie struktura, bo jedyne, co się z nią robi, to porównanie.
pub type CoalesceKey = (u16, u8, u16, u32, u32);

/// Marka. Właścicielem semantyki jest M10 — M6 wyłącznie przenosi wartość w partii
/// i nie interpretuje jej. Typ mieszka tutaj, bo do M10 nikt inny go nie potrzebuje;
/// kiedy M10 powstanie, przenosi go do siebie razem z pamięcią marki u agentów.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct BrandId(pub u16);


/// Linia produkcyjna (M6b). Tutaj wyłącznie jako miejsce, w którym partia może stać.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct LineId(pub u32);

/// Zlecenie transportowe (M6b). Jak wyżej.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct TransportOrderId(pub u32);

/// Slot magazynowy — indeks w arenie slotów [`crate::Store`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SlotId(pub u32);

/// Gdzie partia jest. Stan, nie adnotacja: `prop_no_teleport` (M6b) sprawdza, że zmiana
/// tego pola między zakładami zawsze ma zlecenie transportowe.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BatchLocation {
    Slot(SlotId),
    InTransit(TransportOrderId),
    OnLine(LineId),
}

/// Pochodzenie — poziom lekki, ten, który ma **każda** partia. 16 bajtów.
///
/// Odpowiada na „skąd to jest" na poziomie zakładu i złoża, ale nie na „którędy szło";
/// na to odpowiada [`BatchLedger`], i tylko dla partii z flagą [`BatchFlags::TRACED`].
/// Powód jest arytmetyczny: pełne drzewo dla 600 tys. partii to setki MB rosnące przez
/// sto lat gry. Koszt śledzenia płaci się tam, gdzie ktoś patrzy.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct BatchOrigin {
    pub site: Option<SiteId>,
    pub recipe: Option<RecipeId>,
    /// Ile etapów od surowca. Twardy limit 16 — patrz [`BatchOrigin::MAX_DEPTH`].
    pub depth: u8,
    pub deposit: Option<DepositId>,
}

impl BatchOrigin {
    /// Twardy limit głębokości łańcucha (M6 §6.4.2). Łańcuch dłuższy niż szesnaście
    /// etapów nie powstaje w danych, a gdyby powstał, panel „od pola do półki" i tak
    /// nie miałby go jak pokazać.
    pub const MAX_DEPTH: u8 = 16;

    /// Pochodzenie partii wytworzonej z podanych rodziców: głębokość o jeden większa
    /// od najgłębszego wejścia, złoże propagowane, jeśli wszystkie wejścia zgodne.
    #[must_use]
    pub fn from_inputs(site: SiteId, recipe: RecipeId, parents: &[BatchOrigin]) -> BatchOrigin {
        let depth = parents
            .iter()
            .map(|p| p.depth)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
            .min(BatchOrigin::MAX_DEPTH);
        // Złoże propaguje się tylko wtedy, gdy **wszystkie** wejścia niosą to samo.
        // Inaczej ślad do konkretnego złoża byłby zmyślony, a zmyślony ślad jest gorszy
        // od przyznania się do braku danych (M6 §6.4.2).
        let mut deposit = parents.first().and_then(|p| p.deposit);
        for p in parents {
            if p.deposit != deposit {
                deposit = None;
                break;
            }
        }
        BatchOrigin {
            site: Some(site),
            recipe: Some(recipe),
            depth,
            deposit,
        }
    }

    /// Pochodzenie partii przywiezionej zza granicy (M6c §5.9).
    ///
    /// Ślad kończy się tutaj i **ma się tutaj kończyć**: świata zewnętrznego nie
    /// symulujemy, więc panel „od pola do półki" ma powiedzieć „import", a nie domalować
    /// wiarygodny łańcuch do złoża, którego nie ma. To ta sama zasada, którą M6 i M10
    /// uzgodniły dla partii odtworzonej z agregatu (§6.4.2): przyznanie się do braku
    /// danych jest tańsze i uczciwsze od fabrykowania.
    #[must_use]
    pub const fn imported() -> BatchOrigin {
        BatchOrigin {
            site: None,
            recipe: None,
            depth: 0,
            deposit: None,
        }
    }
}

/// Flagi partii.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct BatchFlags(pub u8);

impl BatchFlags {
    /// Pełny ślad w [`BatchLedger`]. Partia `TRACED` **nigdy nie jest scalana** (WP15).
    pub const TRACED: BatchFlags = BatchFlags(1);
    /// Wstrzymana — afera, kontrola, podejrzenie. Nie wolno jej wydać.
    pub const QUARANTINED: BatchFlags = BatchFlags(2);
    /// Objęta rezerwacją.
    pub const RESERVED: BatchFlags = BatchFlags(4);
    /// Przeceniona — krótko przed datą przydatności.
    pub const MARKDOWN: BatchFlags = BatchFlags(8);

    #[must_use]
    pub const fn has(self, f: BatchFlags) -> bool {
        self.0 & f.0 != 0
    }

    pub fn set(&mut self, f: BatchFlags) {
        self.0 |= f.0;
    }

    pub fn clear(&mut self, f: BatchFlags) {
        self.0 &= !f.0;
    }
}

/// Partia.
///
/// **`mass` jest polem wiodącym bilansu.** `qty` i `volume` są jego funkcjami przez
/// katalog i istnieją dla czytelności oraz dla ograniczeń pojemnościowych — bilans
/// własnościowy liczy się wyłącznie w gramach, co usuwa całą klasę błędów zaokrągleń.
#[derive(Clone, Debug)]
pub struct Batch {
    pub good: GoodId,
    /// Gramy — **pole wiodące**.
    pub mass: Mass,
    /// Milisztuki; zero dla postaci sypkich.
    pub qty: magnat_core::Qty,
    /// Mililitry brutto — do ograniczeń pojemności.
    pub volume: Volume,
    pub quality: Q,
    pub brand: Option<BrandId>,
    pub producer: FirmId,
    pub produced_at: SimMinute,
    pub expires_at: Option<SimMinute>,
    /// Koszt **całej** partii, nie jednostkowy — patrz [`Batch::unit_cost`].
    pub cost_total: Money,
    pub origin: BatchOrigin,
    pub location: BatchLocation,
    pub flags: BatchFlags,
}

impl Batch {
    /// Koszt jednostkowy do UI i księgowości: grosze za tonę (postaci sypkie) albo
    /// za 1000 sztuk (sztukowe) — ta sama skala co `Good::external_base_price`.
    ///
    /// **Getter, nie pole.** Przechowywanie kosztu na gram zaokrągla do zera dla towarów
    /// tanich i gubi grosze przy każdym podziale; koszt całej partii dzieli się
    /// proporcjonalnie do masy z regułą reszty z dok. 00 §2. Dzięki temu suma
    /// `cost_total` po wszystkich partiach jest **dokładnie** równa sumie zapłaconych
    /// kwot pomniejszonej o rozliczony COGS — i da się to sprawdzić testem.
    #[must_use]
    pub fn unit_cost(&self) -> Money {
        if self.mass.0 == 0 {
            return Money::ZERO;
        }
        Money((i128::from(self.cost_total.0) * 1_000_000 / i128::from(self.mass.0)) as i64)
    }

    /// Czy partia jest przeterminowana w danej minucie.
    #[must_use]
    pub fn expired_at(&self, now: SimMinute) -> bool {
        self.expires_at.is_some_and(|e| e.0 <= now.0)
    }

    /// Klucz agregacji (WP15): partie o tym samym kluczu wolno scalić.
    ///
    /// Jakość kubełkowana co 5 punktów, data przydatności co dobę — bo scalanie ma
    /// zbić 5 000 partii do 300, a nie zachować każdą różnicę jednego punktu jakości.
    /// Partia `TRACED` nie ma klucza i nie jest scalana nigdy.
    #[must_use]
    pub fn coalesce_key(&self) -> Option<CoalesceKey> {
        if self.flags.has(BatchFlags::TRACED) {
            return None;
        }
        Some((
            self.good.0,
            self.quality.get() / 5,
            self.brand.map_or(u16::MAX, |b| b.0),
            self.producer.entity().index(),
            self.expires_at.map_or(u32::MAX, |e| (e.0 / 1440) as u32),
        ))
    }

    /// Klucz agregacji **trybu awaryjnego** (WP15, §7.4).
    ///
    /// Traci markę, producenta i tygodniową precyzję daty, zyskuje rząd wielkości:
    /// `(good, quality / 20, expires_at / 10080)`. Włącza się progiem na liczbie
    /// partii, więc przełączenie jest deterministyczne i odwracalne — a nie decyzją
    /// „gdy zrobi się ciasno".
    ///
    /// Partia `TRACED` nie ma klucza także tutaj: panel „od pola do półki" gubiłby
    /// wtedy ślad dokładnie wtedy, gdy świat jest duży, czyli gdy gracz najbardziej
    /// go potrzebuje.
    #[must_use]
    pub fn coalesce_key_coarse(&self) -> Option<CoalesceKey> {
        if self.flags.has(BatchFlags::TRACED) {
            return None;
        }
        Some((
            self.good.0,
            self.quality.get() / 20,
            u16::MAX,
            u32::MAX,
            self.expires_at.map_or(u32::MAX, |e| (e.0 / 10_080) as u32),
        ))
    }
}

impl HashState for Batch {
    fn hash_state(&self, h: &mut StateHasher) {
        self.good.hash_state(h);
        self.mass.hash_state(h);
        self.qty.hash_state(h);
        self.volume.hash_state(h);
        self.quality.hash_state(h);
        h.write_u32(self.brand.map_or(u32::MAX, |b| u32::from(b.0)));
        self.producer.entity().hash_state(h);
        self.produced_at.hash_state(h);
        h.write_u64(self.expires_at.map_or(u64::MAX, |e| e.0));
        self.cost_total.hash_state(h);
        h.write_u32(self.origin.site.map_or(u32::MAX, |s| s.entity().index()));
        h.write_u16(self.origin.recipe.map_or(u16::MAX, |r| r.0));
        h.write_u8(self.origin.depth);
        h.write_u32(self.origin.deposit.map_or(u32::MAX, |d| d.0));
        match self.location {
            BatchLocation::Slot(s) => {
                h.write_u8(0);
                h.write_u32(s.0);
            }
            BatchLocation::InTransit(t) => {
                h.write_u8(1);
                h.write_u32(t.0);
            }
            BatchLocation::OnLine(l) => {
                h.write_u8(2);
                h.write_u32(l.0);
            }
        }
        h.write_u8(self.flags.0);
    }
}

// ── Ślad partii ──────────────────────────────────────────────────────────────────────

/// Etap w historii partii.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TraceKind {
    Produced,
    Stored,
    Loaded,
    Departed,
    Arrived,
    Unloaded,
    Shelved,
    Consumed,
    Sold,
    Lost(LossKind),
    Split,
    Merged,
}

/// Wpis w historii partii `TRACED`.
#[derive(Clone, Copy, Debug)]
pub struct BatchEvent {
    pub batch: BatchId,
    pub at: SimMinute,
    pub site: Option<SiteId>,
    pub kind: TraceKind,
    pub mass: Mass,
    pub cost_cumulative: Money,
    pub quality: Q,
}

/// Strumień historii partii — **append-only**, poza ECS.
///
/// Trafiają tu wyłącznie partie z flagą [`BatchFlags::TRACED`]. Retencja (decyzja
/// otwarta `D8` fazy: pełna historia do 2 lat gry, potem kompaktowanie do pięciu etapów
/// kluczowych) należy do M9/M12 razem z zapisem w tle — M6a trzyma strumień w pamięci
/// i nie udaje, że umie go zarchiwizować.
#[derive(Clone, Debug, Default)]
pub struct BatchLedger {
    events: Vec<BatchEvent>,
    /// Kto z kogo powstał: `(dziecko, rodzic)`. Osobno od zdarzeń, bo to jest
    /// **krawędź**, a nie etap — i bez niej ślad urywa się na pierwszym przetworzeniu:
    /// bochenek chleba ma własną historię od wyjęcia z pieca, ale „od pola do półki"
    /// zaczyna się na polu, czyli w partii, której ten bochenek jest wnukiem.
    parents: Vec<(BatchId, BatchId)>,
}

impl BatchLedger {
    pub fn push(&mut self, e: BatchEvent) {
        self.events.push(e);
    }

    /// Zapisuje, że `child` powstało z `parent`.
    pub fn link(&mut self, child: BatchId, parent: BatchId) {
        if child != parent {
            self.parents.push((child, parent));
        }
    }

    /// Historia jednej partii, w kolejności zapisu.
    ///
    /// `ponytail:` przeszukanie liniowe. Sufit nazwany: do dziennika trafiają wyłącznie
    /// partie z flagą `TRACED`, czyli te, na które ktoś patrzy — rzędu dziesiątek, nie
    /// sześciuset tysięcy. Droga wyjścia to indeks `BatchId → zakres`, i wejdzie wtedy,
    /// gdy `bench_trace_batch_depth12` pokaże, że jest potrzebny.
    #[must_use]
    pub fn trace(&self, b: BatchId) -> Vec<BatchEvent> {
        self.events
            .iter()
            .copied()
            .filter(|e| e.batch == b)
            .collect()
    }

    /// Rodzice partii, w kolejności zapisania.
    #[must_use]
    pub fn parents_of(&self, b: BatchId) -> Vec<BatchId> {
        self.parents
            .iter()
            .filter(|(c, _)| *c == b)
            .map(|(_, p)| *p)
            .collect()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::Entity;
    use std::num::NonZeroU32;

    fn encja(i: u32) -> Entity {
        Entity::new(i, NonZeroU32::new(1).expect("generacja 1"))
    }

    fn firma() -> FirmId {
        FirmId(encja(0))
    }

    fn site(i: u32) -> SiteId {
        SiteId(encja(i))
    }

    #[test]
    fn glebokosc_rosnie_i_zatrzymuje_sie_na_suficie() {
        let zloze = BatchOrigin {
            deposit: Some(DepositId(3)),
            ..BatchOrigin::default()
        };
        let o = BatchOrigin::from_inputs(site(1), RecipeId(0), &[zloze]);
        assert_eq!(o.depth, 1);
        assert_eq!(o.deposit, Some(DepositId(3)));

        let gleboka = BatchOrigin {
            depth: BatchOrigin::MAX_DEPTH,
            ..BatchOrigin::default()
        };
        let o = BatchOrigin::from_inputs(site(1), RecipeId(0), &[gleboka]);
        assert_eq!(o.depth, BatchOrigin::MAX_DEPTH);
    }

    /// Ślad do złoża propaguje się tylko wtedy, gdy **wszystkie** wejścia niosą to samo.
    /// Zmyślony ślad jest gorszy od przyznania się do braku danych: gracz, który raz
    /// przyłapie panel na zmyślaniu, przestanie mu wierzyć także tam, gdzie panel ma rację.
    #[test]
    fn dwa_zloza_daja_brak_zloza_a_nie_pierwsze_lepsze() {
        let a = BatchOrigin {
            deposit: Some(DepositId(1)),
            ..BatchOrigin::default()
        };
        let b = BatchOrigin {
            deposit: Some(DepositId(2)),
            ..BatchOrigin::default()
        };
        assert_eq!(
            BatchOrigin::from_inputs(site(1), RecipeId(0), &[a, b]).deposit,
            None
        );
    }

    #[test]
    fn partia_sledzona_nie_ma_klucza_agregacji() {
        let mut b = Batch {
            good: GoodId(1),
            mass: Mass(1000),
            qty: magnat_core::Qty::ZERO,
            volume: Volume(2000),
            quality: Q::new(60),
            brand: None,
            producer: firma(),
            produced_at: SimMinute(0),
            expires_at: None,
            cost_total: Money(500),
            origin: BatchOrigin::default(),
            location: BatchLocation::Slot(SlotId(0)),
            flags: BatchFlags::default(),
        };
        assert!(b.coalesce_key().is_some());
        b.flags.set(BatchFlags::TRACED);
        assert!(b.coalesce_key().is_none());
    }
}
