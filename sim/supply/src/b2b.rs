//! Rynek B2B (M6c, WP7–WP9).
//!
//! Do M6b zakład, któremu zabrakło wsadu, schodził kolejnymi szczeblami kaskady niedoboru
//! i **nic z tego nie wynikało**: `shortage::review` zwracał listę [`ShortageAction`],
//! której nikt nie czytał, więc drabina wchodziła wyżej aż do postoju. Zapotrzebowanie
//! po prostu znikało. Ta podfaza dokłada stronę, która na nie odpowiada.
//!
//! ## Kierunek zależności
//!
//! Rynek mieszka w `sim/supply`, a nie w `sim/economy`, i to nie jest wybór estetyczny.
//! Wycena oferty potrzebuje magazynu ([`Store`]), zakładu ([`Plant`]), trasy
//! ([`FreightOracle`]) i katalogu — wszystkiego, co jest tutaj. `sim/economy` zależy od
//! `sim/supply`, nigdy odwrotnie, więc rynek po tamtej stronie sięgałby po cudze wnętrze
//! przy każdej ofercie. Pieniądz wychodzi stąd tak samo jak z [`Plant::bill_utilities`]:
//! listą faktów do zaksięgowania ([`Settlement`]), bo `sim/supply` księgi nie widzi.
//!
//! **`Quote` jest własnym typem, nie widokiem na `Offer` M5** — wbrew literze §5.8,
//! zgodnie z jej duchem (`K-36`, `AH-1`). `Offer` jest stojącą ceną półkową indeksowaną
//! przestrzennie po `StockCat` dla mieszkańca; ruda żelaza nie ma `StockCat`, w którym
//! mogłaby stanąć. `Quote` jest odpowiedzią na **jedno** zapytanie, z masą, terminem
//! gotowości i datą ważności. Substancja `K-7` zostaje bez zmian: cały rynek B2B liczy
//! netto, a rozróżnienie niesie jawna deklaracja, nie domysł z kontekstu.

pub mod contract;
pub mod relation;
pub mod rfq;
pub mod trade;

use std::collections::BTreeMap;

use magnat_core::{
    ContractId, DecisionReason, FirmId, GoodId, HashState, Mass, Money, SimMinute, SiteId,
    StateHasher,
};

use crate::batch::{SlotId, TransportOrderId};
use crate::catalog::Catalog;
use crate::plant::Plant;
use crate::shortage::{RfqId, ShortageAction, ShortageStage};
use crate::store::Store;
use crate::transport::FreightOracle;
use crate::tuning::Tuning;

pub use contract::{
    ContractDelivery, ContractError, ContractPricing, DeliverySchedule, Penalty, SupplyContract,
    SupplyContractDraft,
};
pub use relation::{DeliveryOutcome, RelationTuning, Relations, SupplierRelation};
pub use rfq::{Quote, QuoteId, Rfq, RfqDraft, RfqOutcome, SellerIndex, WhoTransports};
pub use trade::{
    gate_allows, ImportQuote, PendingImport, TariffClass, TariffError, TariffTable, TradeError,
    TradeGood, TradeNode, TradeNodeId, TARIFFS_SCHEMA_VERSION,
};

/// Kto sprzedał. Import kupuje się od świata zewnętrznego, a ten nie jest firmą —
/// i to rozróżnienie musi przeżyć drogę do księgi, bo `sim/economy` ma po tamtej
/// stronie `SupplierRef` z dokładnie tym samym podziałem.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SellerRef {
    Firm(FirmId),
    External(TradeNodeId),
}

/// Fakt do zaksięgowania. `sim/supply` nie zna księgi (kierunek zależności jest odwrotny),
/// więc oddaje listę — ten sam wzorzec co [`Plant::bill_utilities`].
///
/// Kwoty są **netto** (`K-7`). `duty` jest osobną pozycją, a nie składnikiem `net`:
/// M8 podmieni stub taryfy na politykę miasta i wtedy cło ma być widoczne jako obciążenie,
/// a nie jako towar, który nagle podrożał.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Settlement {
    pub buyer: FirmId,
    /// Zakład, do którego towar jedzie. Firma może mieć wiele zakładów, a księga
    /// i rachunek bieżący są **per zakład** (M5 §5.8), więc sam `FirmId` nie mówi,
    /// z którego konta zapłacić. Dopisane w WP11, kiedy rozliczenie dostało pierwszego
    /// czytelnika po stronie księgi.
    pub deliver_to: SiteId,
    pub seller: SellerRef,
    /// Zakład, **z którego** towar wyjechał; `None` przy imporcie zza granicy.
    ///
    /// Dopisane w `R2-WP7`. Sam `FirmId` nie wystarczał do niczego, co nazywa
    /// wynik: firma z trzema zakładami dostawała jedną kwotę i nie było jej jak
    /// rozdzielić na linie, więc `Firms::post_revenue` nie miało kogo obciążyć
    /// utargiem i `SitePnlMonth.revenue` zakładu produkcyjnego zostawał zerem.
    /// Rozstrzygnięcie zna zakład sprzedawcy w chwili konstrukcji — pole tylko
    /// przenosi to, co już było wiadomo.
    pub seller_site: Option<SiteId>,
    /// Koszt własny sprzedawcy dla tej wysyłki — to, ile partia kosztowała **jego**,
    /// zanim zmieniła właściciela. Liczy ją `Store::resell`, czyli to samo miejsce,
    /// które dopisuje ją do księgi kontrolnej magazynu: marża zakładu jest wtedy
    /// liczona z partii, a nie z miesięcznej średniej.
    pub seller_cogs: Money,
    pub good: GoodId,
    pub mass: Mass,
    pub net: Money,
    pub duty: Money,
    pub order: Option<TransportOrderId>,
    pub reason: DecisionReason,
}

/// Siedmiodobowe okno cen spot jednego towaru — podstawa indeksacji kontraktów (§5.8).
///
/// Tablica, nie kolejka: siedem `Money` to 56 bajtów i zero alokacji per towar,
/// a kolejka kosztowałaby wskaźnik i pojemność dla tej samej zawartości.
#[derive(Clone, Copy, Debug, Default)]
struct SpotWindow {
    dni: [Money; 7],
    suma_masy: [i64; 7],
    dzis: usize,
    wypelnione: u8,
}

impl SpotWindow {
    /// Zapisuje transakcję w dzisiejszym kubełku. Średnia **ważona masą**, a nie liczbą
    /// transakcji: dwie dostawy po tonie i jedna po stu nie mogą liczyć się tyle samo.
    fn record(&mut self, unit_price: Money, mass: Mass) {
        let i = self.dzis;
        self.dni[i] = Money(self.dni[i].0 + unit_price.0 * mass.0 / 1_000_000);
        self.suma_masy[i] += mass.0;
    }

    fn roll_day(&mut self) {
        self.dzis = (self.dzis + 1) % 7;
        self.dni[self.dzis] = Money::ZERO;
        self.suma_masy[self.dzis] = 0;
        self.wypelnione = self.wypelnione.saturating_add(1).min(7);
    }

    /// Średnia cena za tonę z okna. `None`, jeśli w oknie nic się nie sprzedało —
    /// zero byłoby ceną, a brak obrotu ceną nie jest.
    fn average(&self) -> Option<Money> {
        let wartosc: i128 = self.dni.iter().map(|m| i128::from(m.0)).sum();
        let masa: i128 = self.suma_masy.iter().map(|m| i128::from(*m)).sum();
        if masa <= 0 {
            return None;
        }
        Some(Money((wartosc * 1_000_000 / masa) as i64))
    }
}

impl HashState for SpotWindow {
    fn hash_state(&self, h: &mut StateHasher) {
        for i in 0..7 {
            self.dni[i].hash_state(h);
            h.write_i64(self.suma_masy[i]);
        }
        h.write_u32(self.dzis as u32);
        h.write_u8(self.wypelnione);
    }
}

/// Rynek B2B miasta: zapytania, kontrakty, węzły graniczne i indeks dostawców.
///
/// Jeden zasób, nie cztery, z tego samego powodu, dla którego partie i sloty poszły do
/// jednego [`Store`] (`AD-7`): rozstrzygnięcie zapytania podpisuje kontrakt i wystawia
/// zlecenie transportowe w jednym kroku, a `World::resource_mut` pożycza cały świat.
/// Wyłączności dostawców — legalny kontrakt, nie zmowa (M7e WP14, PRD §7.9).
///
/// Firma, która traci udział w rynku, może **zabezpieczyć sobie dostawcę**: przez
/// umówiony okres ten zakład sprzedaje jej towar wyłącznie jej, a ona płaci za to
/// premię ponad cenę rynkową. Rywal nie dostaje odmowy z powietrza — po prostu
/// przestaje widzieć tego dostawcę w zapytaniu ofertowym, tak jak nie widzi zakładu,
/// który akurat nic nie ma na wystawce.
///
/// **To jest wyłączność, nie kartel.** Kartel to zmowa **między** sprzedawcami co do
/// ceny i należy do M8 razem z UOKiK (M7 §2, „nie wchodzi"). Tutaj strony są dwie
/// i obie na tym coś tracą: kupujący płaci premię, sprzedawca traci resztę rynku.
///
/// `BTreeMap`, bo po tej kolejności idzie hash stanu (00 §3.2).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Exclusives {
    locks: BTreeMap<(u32, u16), Lock>,
}

/// Jedna wyłączność: kto ją ma, do kiedy i za ile.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Lock {
    pub holder: FirmId,
    pub until: SimMinute,
    /// Premia ponad cenę rynkową, w punktach bazowych — cena wyłączności.
    pub premium_bp: u16,
}

impl Exclusives {
    /// Zawiera wyłączność na parę (zakład dostawcy, towar). Nadpisuje poprzednią:
    /// druga umowa na tę samą wystawkę znaczy, że pierwsza została odkupiona.
    pub fn lock(&mut self, seller: SiteId, good: GoodId, l: Lock) {
        self.locks.insert((seller.entity().index(), good.get()), l);
    }

    /// Kto ma wyłączność na tę wystawkę w tej chwili. `None` znaczy „wolny rynek",
    /// także dla umowy, której termin minął — wygasła wyłączność nikogo nie wiąże.
    #[must_use]
    pub fn holder(&self, seller: SiteId, good: GoodId, now: SimMinute) -> Option<Lock> {
        self.locks
            .get(&(seller.entity().index(), good.get()))
            .copied()
            .filter(|l| now.0 < l.until.0)
    }

    /// Sprząta umowy, którym minął termin. Woła się raz na dobę — bez tego mapa
    /// rośnie przez całą grę o każdą kampanię, która kiedykolwiek się odbyła.
    pub fn expire(&mut self, now: SimMinute) {
        self.locks.retain(|_, l| now.0 < l.until.0);
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.locks.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.locks.is_empty()
    }
}

impl HashState for Exclusives {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.locks.len() as u32);
        for ((s, g), l) in &self.locks {
            h.write_u32(*s);
            h.write_u16(*g);
            l.holder.0.hash_state(h);
            h.write_u64(l.until.0);
            h.write_u16(l.premium_bp);
        }
    }
}

pub struct B2b {
    rfqs: BTreeMap<u32, Rfq>,
    contracts: BTreeMap<u32, SupplyContract>,
    nodes: Vec<TradeNode>,
    pending: Vec<PendingImport>,
    sellers: SellerIndex,
    spot: Vec<SpotWindow>,
    /// Mnożnik ceny świata zewnętrznego per towar, w punktach bazowych.
    /// Wejście bramki **G4** — patrz [`B2b::set_supply_shock`].
    shock_bp: Vec<i32>,
    tariffs: TariffTable,
    next_rfq: u32,
    next_quote: u32,
    next_contract: u32,
    world_seed: u64,
    /// Wyłączności dostawców (M7e WP14).
    exclusives: Exclusives,
    /// Relacje z dostawcami: zaufanie, preferencja i priorytet (M10e WP10.13).
    relations: Relations,
}

impl B2b {
    #[must_use]
    pub fn new(goods: usize, tariffs: TariffTable, world_seed: u64) -> B2b {
        B2b {
            rfqs: BTreeMap::new(),
            contracts: BTreeMap::new(),
            nodes: Vec::new(),
            pending: Vec::new(),
            sellers: SellerIndex::new(goods),
            spot: vec![SpotWindow::default(); goods],
            shock_bp: vec![10_000; goods],
            tariffs,
            exclusives: Exclusives::default(),
            relations: Relations::default(),
            next_rfq: 1,
            next_quote: 1,
            next_contract: 1,
            world_seed,
        }
    }

    /// Mnożnik ceny świata zewnętrznego dla towaru, w punktach bazowych
    /// (10 000 = bez zmian, 18 000 = +80 %).
    ///
    /// **To jest jedyne zdarzenie zewnętrzne, jakie gospodarka ma**, i jedyne wejście
    /// bramki **G4** balansatora („reaktywność szoku podaży: widoczny w 2–7 dni,
    /// wygaszony w 14–56"). Do M6c wisiało na `ExternalSupplier` i podmiana dostawcy
    /// zabrałaby bramce czym szokować (`AC-1`) — dlatego przenosi się tam, gdzie od
    /// WP11 naprawdę ustala się cena hurtowa: na węzeł graniczny. Szok jest **stanem**,
    /// a nie parametrem zapytania, bo dokładnie tak zachowuje się prawdziwy: cena
    /// skacze wszystkim naraz i zostaje, dopóki ktoś jej nie cofnie.
    pub fn set_supply_shock(&mut self, good: GoodId, factor_bp: i32) {
        if (good.0 as usize) < self.shock_bp.len() {
            self.shock_bp[good.0 as usize] = factor_bp;
        }
    }

    /// Stawka celna klasy i jej podmiana — polityka celna miasta (M8).
    #[must_use]
    pub fn duty_bp(&self, class: magnat_core::TariffClassId) -> i64 {
        self.tariffs.duty_bp(class)
    }

    /// Klasa taryfowa po własnym kluczu — wejście dla katalogu zdarzeń M8c.
    #[must_use]
    pub fn tariff_class_of(&self, key: &str) -> Option<magnat_core::TariffClassId> {
        self.tariffs.class_by_key(key)
    }

    /// Liczba klas taryfowych w tabeli.
    #[must_use]
    pub fn tariff_class_count(&self) -> usize {
        self.tariffs.class_count()
    }

    /// Podmienia stawkę celną klasy; zwraca poprzednią.
    pub fn set_duty_bp(&mut self, class: magnat_core::TariffClassId, bp: i64) -> i64 {
        self.tariffs.set_duty_bp(class, bp)
    }

    /// Mnożnik szoku dla towaru; `10_000` znaczy „bez zmian".
    #[must_use]
    pub fn supply_shock(&self, good: GoodId) -> i32 {
        self.shock_bp
            .get(good.0 as usize)
            .copied()
            .filter(|f| *f > 0)
            .unwrap_or(10_000)
    }

    #[must_use]
    pub fn tariffs(&self) -> &TariffTable {
        &self.tariffs
    }

    #[must_use]
    pub fn rfq(&self, id: RfqId) -> Option<&Rfq> {
        self.rfqs.get(&id.0)
    }

    #[must_use]
    pub fn contract(&self, id: ContractId) -> Option<&SupplyContract> {
        self.contracts.get(&id.entity().index())
    }

    /// Nadaje `ContractId` umowie, której `sim/supply` nie prowadzi.
    ///
    /// Istnieje, bo licencja patentowa z M10c **jest** `ContractId` (M10c §5.4:
    /// „zero nowego mechanizmu umów"), a nie jest umową dostawy: nie ma towaru,
    /// harmonogramu ani rampy, więc nie zmieści się w [`SupplyContract`]. Numer
    /// musi jednak pochodzić **stąd**, bo to jest jedyny licznik identyfikatorów
    /// umów w grze — dwa liczniki dałyby dwie umowy o tym samym numerze
    /// i `Subject::Contract` prowadziłby raz tu, raz tam.
    ///
    /// Numer wydany tą drogą **nie ma wpisu** w `self.contracts` i to jest
    /// poprawny stan: rejestr umowy prowadzi ten, kto ją zawarł.
    pub fn mint_contract_id(&mut self) -> ContractId {
        let idx = self.next_contract;
        self.next_contract += 1;
        ContractId(magnat_core::Entity::new(idx, std::num::NonZeroU32::MIN))
    }

    pub fn contracts(&self) -> impl Iterator<Item = &SupplyContract> {
        self.contracts.values()
    }

    #[must_use]
    pub fn open_rfq_count(&self) -> usize {
        self.rfqs
            .values()
            .filter(|r| r.outcome == RfqOutcome::Open)
            .count()
    }

    pub fn nodes(&self) -> impl Iterator<Item = &TradeNode> {
        self.nodes.iter()
    }

    #[must_use]
    pub fn node(&self, id: TradeNodeId) -> Option<&TradeNode> {
        self.nodes.get(id.0 as usize)
    }

    /// Dokłada węzeł graniczny. Identyfikator to indeks w wektorze — węzłów jest kilka
    /// na miasto i żaden nie znika, więc dziury i generacje byłyby kosztem bez powodu.
    pub fn add_node(&mut self, mut n: TradeNode) -> TradeNodeId {
        let id = TradeNodeId(self.nodes.len() as u32);
        n.id = id;
        self.nodes.push(n);
        id
    }

    pub fn node_mut(&mut self, id: TradeNodeId) -> Option<&mut TradeNode> {
        self.nodes.get_mut(id.0 as usize)
    }

    /// Przebudowa indeksu dostawców. Wołane `EveryDay`.
    pub fn reindex(&mut self, cat: &Catalog, plant: &Plant) {
        self.sellers.rebuild(cat, plant);
    }

    #[must_use]
    pub fn sellers_of(&self, good: GoodId) -> &[SiteId] {
        self.sellers.sellers_of(good)
    }

    /// Rejestr wyłączności — odczyt dla panelu i dla testów.
    #[must_use]
    pub fn exclusives(&self) -> &Exclusives {
        &self.exclusives
    }

    /// Rejestr wyłączności do zapisu. Jedynym pisarzem jest reakcja firmy na wejście
    /// rywala (M7e WP14); M8 dołoży drugiego, kiedy UOKiK zacznie je unieważniać.
    pub fn exclusives_mut(&mut self) -> &mut Exclusives {
        &mut self.exclusives
    }

    /// Relacje z dostawcami — odczyt dla panelu, dla testów i dla `sim/economy`.
    #[must_use]
    pub fn relations(&self) -> &Relations {
        &self.relations
    }

    /// Relacje do zapisu. Pisarzy jest dwóch i obaj są w tym crate'cie: rozliczenie
    /// dostawy kontraktowej i rozliczenie zakupu spotowego. Z zewnątrz sięga tu
    /// wyłącznie kalibracja — patrz [`B2b::set_relation_tuning`].
    pub fn relations_mut(&mut self) -> &mut Relations {
        &mut self.relations
    }

    /// Kalibracja relacji z `data/tuning/relations.ron` (właściciel M10, `K-88`).
    /// Świat, w którym nikt jej nie wstawi, liczy przetarg na wartościach domyślnych
    /// — czyli tak samo, jak liczył go M6c.
    pub fn set_relation_tuning(&mut self, t: RelationTuning) {
        self.relations.set_tuning(t);
    }

    /// Średnia cena spot z ostatnich siedmiu dób — `index_now` dla cennika indeksowanego.
    #[must_use]
    pub fn spot_index(&self, good: GoodId) -> Option<Money> {
        self.spot.get(good.0 as usize).and_then(SpotWindow::average)
    }

    /// Koniec doby: okno cen przesuwa się, węzły odzyskują przepustowość.
    pub fn roll_day(&mut self, t: &Tuning, now: SimMinute) {
        for w in &mut self.spot {
            w.roll_day();
        }
        self.exclusives.expire(now);
        self.relations.forget(now);
        for n in &mut self.nodes {
            n.roll_day(&t.trade);
        }
    }

    // ── spięcie z kaskadą niedoboru ─────────────────────────────────────────────

    /// Konsumuje akcje kaskady (`AG-3`): zapytanie ofertowe i import dostają **prawdziwe**
    /// uchwyty, a `ShortageStage` przestaje nieść zaślepki.
    ///
    /// To jest miejsce, w którym znika `ponytail:` ze stałą ośmiu godzin z M6b (`AG-4`):
    /// czas dostawy importowej bierze się teraz z węzła, jego zaległości i losowania
    /// `StreamId::SupplyImportLead`. Węzeł, który tego towaru nie wpuszcza, nie produkuje
    /// żadnego ETA — i wtedy drabina wchodzi szczebel wyżej, tak jak ma.
    #[allow(clippy::too_many_arguments)]
    pub fn serve(
        &mut self,
        actions: &[ShortageAction],
        cat: &Catalog,
        store: &Store,
        plant: &mut Plant,
        oracle: &dyn FreightOracle,
        t: &Tuning,
        now: SimMinute,
    ) {
        for a in actions {
            match *a {
                ShortageAction::OpenRfq { site, good, mass } => {
                    let Some((buyer, slot)) = kupujacy(plant, site) else {
                        continue;
                    };
                    let id = self.open_rfq(
                        RfqDraft {
                            buyer,
                            deliver_to: site,
                            to_slot: slot,
                            good,
                            mass,
                            min_quality: magnat_core::Q::MIN,
                            needed_by: SimMinute(now.0 + 60),
                            incoterm: WhoTransports::Seller,
                        },
                        cat,
                        store,
                        plant,
                        oracle,
                        t,
                        now,
                    );
                    if let Some(p) = plant.get_mut(site) {
                        p.set_stage(good, ShortageStage::SpotSearch { rfq: id });
                    }
                }
                ShortageAction::Import { site, good, mass } => {
                    let Some((buyer, slot)) = kupujacy(plant, site) else {
                        continue;
                    };
                    let Some(q) = self.import_quote(cat, good, mass, t, now) else {
                        continue;
                    };
                    let eta = q.arrives_at;
                    if self.place_import(q, buyer, site, slot).is_ok() {
                        if let Some(p) = plant.get_mut(site) {
                            p.set_stage(good, ShortageStage::Importing { eta });
                        }
                    }
                }
                // Substytucję wykonuje linia produkcyjna, nie rynek — zamiennik jest
                // właściwością procesu (§5.7), a nie czymś, co się kupuje osobno.
                ShortageAction::Substitute { .. } => {}
            }
        }
    }
}

/// Firma i slot wejściowy zakładu — to, czego potrzebuje zapytanie, żeby miało dokąd
/// dowieźć. Zakład bez magazynu wejściowego nie kupuje; nie ma gdzie.
fn kupujacy(plant: &Plant, site: SiteId) -> Option<(FirmId, SlotId)> {
    let p = plant.get(site)?;
    Some((p.owner, *p.inputs.first()?))
}

impl HashState for B2b {
    fn hash_state(&self, h: &mut StateHasher) {
        for f in &self.shock_bp {
            h.write_i64(i64::from(*f));
        }
        h.write_u32(self.next_rfq);
        h.write_u32(self.next_quote);
        h.write_u32(self.next_contract);
        h.write_u32(self.rfqs.len() as u32);
        for (k, r) in &self.rfqs {
            h.write_u32(*k);
            r.hash_state(h);
        }
        h.write_u32(self.contracts.len() as u32);
        for (k, c) in &self.contracts {
            h.write_u32(*k);
            c.hash_state(h);
        }
        h.write_u32(self.nodes.len() as u32);
        for n in &self.nodes {
            n.hash_state(h);
        }
        h.write_u32(self.pending.len() as u32);
        for p in &self.pending {
            h.write_u32(p.node.0);
            p.buyer.entity().hash_state(h);
            h.write_u16(p.good.0);
            p.mass.hash_state(h);
            p.paid.hash_state(h);
            p.duty.hash_state(h);
            p.arrives_at.hash_state(h);
            p.deliver_to.entity().hash_state(h);
            h.write_u32(p.to_slot.0);
        }
        h.write_u32(self.spot.len() as u32);
        for w in &self.spot {
            w.hash_state(h);
        }
        // Wyłączność zmienia to, kto kogo widzi w przetargu — czyli stan gospodarki,
        // a nie szczegół wykonania (M7e WP14).
        self.exclusives.hash_state(h);
        // Zaufanie zmienia to, kto z kim handluje i po ile — czyli stan gospodarki.
        self.relations.hash_state(h);
    }
}
