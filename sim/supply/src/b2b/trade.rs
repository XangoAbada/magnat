//! Import, eksport i węzły graniczne (M6c §5.9, WP9).
//!
//! Import jest w tej fazie jedynym źródłem masy poza produkcją i zapasem startowym, więc
//! kusi, żeby zrobić z niego zawór bezpieczeństwa: brakuje — dowieziemy. Cała ta sekcja
//! jest po to, żeby **nie był**. Trzy mechanizmy naraz, wszystkie z §5.9:
//!
//! 1. **Przepustowość dobowa węzła.** Nadwyżka ponad `capacity_per_day` idzie w zaległość,
//!    a zaległość wydłuża czas dostawy **wszystkim**, nie tylko temu, kto ją wywołał.
//! 2. **Cena rosnąca z wolumenem.** Zakup dziesięciokrotności wolumenu odniesienia przy
//!    elastyczności 300 ‰ podnosi cenę o 300 %.
//! 3. **Cło poza ceną.** `ImportQuote` niesie cenę netto i osobno rozpisane obciążenie
//!    (`K-7`), więc podwyżka cła jest widoczna jako pozycja, a nie jako drożejący towar.
//!
//! Eksport jest tą samą mechaniką w drugą stronę i **nie ma własnej reguły „drenażu"**.
//! Producent porównuje cenę skupu za granicą z najlepszą ceną lokalną, i jeśli tamta
//! wygrywa, powstaje zwykłe zlecenie transportowe **do** węzła — zajmuje ciężarówkę,
//! rampę i zabiera masę z lokalnej podaży. Wzrost cen w mieście wychodzi z tego sam.

use magnat_core::{
    rng, DecisionReason, FirmId, FirmReason, GateKind, GoodId, HashState, Mass, Money, SimMinute,
    SiteId, StateHasher, StreamId, TariffClassId, Tick,
};
use serde::Deserialize;

use crate::batch::SlotId;
use crate::catalog::Catalog;
use crate::tuning::TradeTuning;

pub const TARIFFS_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct TradeNodeId(pub u32);

/// Klasa taryfowa i jej stawka.
#[derive(Clone, Debug, Deserialize)]
pub struct TariffClass {
    pub key: String,
    /// Domeny kluczy towarów (`AD-12`), które wpadają w tę klasę. Przypisanie **przez
    /// domenę, a nie przez tabelę czterystu wierszy**: domen jest dwanaście i lista jest
    /// zamknięta, więc każdy towar ma klasę z definicji, a nowy towar nie może jej
    /// zgubić przez przeoczenie w danych.
    pub domains: Vec<String>,
    /// Stawka ad valorem w punktach bazowych. **Stub** — stawka jest polityką miasta
    /// i epoki, a ta należy do M8 (`ChargeRegistry`). M6 podaje liczbę, żeby import
    /// nie był identyczny z cłem i bez, a M8 podmienia całą tabelę, nie jej kształt.
    pub duty_bp: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TariffTable {
    pub schema_version: u32,
    pub classes: Vec<TariffClass>,
}

#[derive(Debug)]
pub enum TariffError {
    Io(String),
    Parse(String),
    Schema {
        found: u32,
        want: u32,
    },
    /// Domena z klucza towaru nie ma klasy — walidator ma to złapać w CI, a nie gracz.
    DomainWithoutClass(String),
}

impl std::fmt::Display for TariffError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TariffError::Io(e) | TariffError::Parse(e) => write!(f, "data/trade/tariffs.ron: {e}"),
            TariffError::Schema { found, want } => write!(
                f,
                "data/trade/tariffs.ron: schema_version {found}, oczekiwano {want}"
            ),
            TariffError::DomainWithoutClass(d) => {
                write!(
                    f,
                    "data/trade/tariffs.ron: domena `{d}` bez klasy taryfowej"
                )
            }
        }
    }
}

impl std::error::Error for TariffError {}

impl TariffTable {
    pub fn load(path: &std::path::Path) -> Result<TariffTable, TariffError> {
        let tekst = std::fs::read_to_string(path).map_err(|e| TariffError::Io(e.to_string()))?;
        let t: TariffTable =
            ron::from_str(&tekst).map_err(|e| TariffError::Parse(e.to_string()))?;
        if t.schema_version != TARIFFS_SCHEMA_VERSION {
            return Err(TariffError::Schema {
                found: t.schema_version,
                want: TARIFFS_SCHEMA_VERSION,
            });
        }
        Ok(t)
    }

    pub fn load_default() -> Result<TariffTable, TariffError> {
        TariffTable::load(&magnat_core::data_path("trade/tariffs.ron"))
    }

    /// Klasa taryfowa towaru, wyprowadzona z domeny jego klucza (`AD-12`).
    #[must_use]
    pub fn class_of(&self, key: &str) -> Option<TariffClassId> {
        let domena = key.split_once('_').map_or(key, |(d, _)| d);
        self.classes
            .iter()
            .position(|c| c.domains.iter().any(|d| d == domena))
            .map(|i| TariffClassId(i as u16))
    }

    #[must_use]
    pub fn duty_bp(&self, class: TariffClassId) -> i64 {
        self.classes.get(class.0 as usize).map_or(0, |c| c.duty_bp)
    }

    /// Klasa taryfowa po **własnym kluczu** (`raw`, `food`, `fuel`, …).
    ///
    /// Osobno od [`TariffTable::class_of`], które mapuje **klucz towaru** na klasę
    /// przez domenę. Dwie różne rzeczy pod jedną nazwą rozjechałyby się przy
    /// pierwszym towarze, którego klucz przypadkiem wygląda jak nazwa klasy.
    #[must_use]
    pub fn class_by_key(&self, key: &str) -> Option<TariffClassId> {
        self.classes
            .iter()
            .position(|c| c.key == key)
            .and_then(|i| u16::try_from(i).ok())
            .map(TariffClassId)
    }

    /// Liczba klas taryfowych — zakres, po którym generator zdarzeń M8c wylicza
    /// instancje zakresu `TariffClass`.
    #[must_use]
    pub fn class_count(&self) -> usize {
        self.classes.len()
    }

    /// Podmienia stawkę klasy. Zwraca poprzednią.
    ///
    /// Wejście dla polityki celnej M8: uchwała rady (M8e) i zdarzenie polityczne
    /// (M8c) zmieniają **stawkę**, nigdy przypisania towaru do klasy — klasa jest
    /// etykietą towaru i należy do katalogu M6 (`K-38`).
    pub fn set_duty_bp(&mut self, class: TariffClassId, bp: i64) -> i64 {
        match self.classes.get_mut(class.0 as usize) {
            Some(c) => std::mem::replace(&mut c.duty_bp, bp.max(0)),
            None => 0,
        }
    }

    /// Każdy towar katalogu ma klasę. Wołane przy ładowaniu, nie w pętli handlu.
    pub fn check_covers(&self, cat: &Catalog) -> Result<(), TariffError> {
        for g in &cat.goods {
            if self.class_of(&g.key).is_none() {
                let domena = g.key.split_once('_').map_or(&*g.key, |(d, _)| d);
                return Err(TariffError::DomainWithoutClass(domena.to_string()));
            }
        }
        Ok(())
    }
}

/// Towar w obrocie przez konkretny węzeł.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TradeGood {
    pub good: GoodId,
    /// Grosze za tonę (albo za 1000 sztuk), netto, loco węzeł.
    pub base_price: Money,
    pub elasticity_permille: u32,
    /// Wolumen odniesienia w oknie `TradeTuning::window_minutes`.
    pub window_reference: Mass,
    pub bought_window: Mass,
    pub export_spread_pct: u8,
    pub tariff_class: TariffClassId,
}

impl TradeGood {
    /// Cena importowa przy aktualnym zakupie w oknie (§5.9).
    ///
    /// `p = base * (1000 + elasticity * bought / reference) / 1000`
    #[must_use]
    pub fn import_price(&self) -> Money {
        if self.window_reference.0 <= 0 {
            return self.base_price;
        }
        let narzut = i128::from(self.elasticity_permille) * i128::from(self.bought_window.0)
            / i128::from(self.window_reference.0);
        let mnoznik = (1_000 + narzut).clamp(1_000, 1_000_000);
        Money((i128::from(self.base_price.0) * mnoznik / 1_000) as i64)
    }

    /// Cena skupu eksportowego. Liczona od **ceny bazowej**, nie od importowej: świat
    /// zewnętrzny nie płaci więcej za to, że miasto akurat dużo kupuje.
    #[must_use]
    pub fn export_price(&self) -> Money {
        self.base_price
            .mul_ratio(i64::from(self.export_spread_pct), 100)
    }
}

/// Węzeł graniczny: port, bocznica, zjazd z autostrady.
///
/// **Rodzaj węzła to `GateKind` z `core`, nie własny enum.** §5.9 rysował
/// `TradeNodeKind { Port | Rail | Highway | PipelineHead | Airport }`, ale `K-33`
/// przeniósł `GateKind` do `core` **właśnie po to**, żeby `Good::import_via` i węzeł
/// graniczny mówiły o tym samym. Drugi enum rozjechałby się przy pierwszej zmianie
/// i unieważnił pole `import_via` w całym katalogu fali A (`AH-3`). Głowica rurociągu
/// z §5.9 nie powstaje: w fali A żaden rurociąg nie przechodzi granicy.
///
/// **Sprostowanie wpisane w R2e (`D-N5`):** ten akapit powoływał się na
/// `Carrier::Pipeline`, który „wozi ropę wewnątrz miasta" — taki wariant istniał
/// w typie, ale **nie miał ani jednej ścieżki wykonania**: nikt go nie konstruował,
/// `haul_cost` liczył wyłącznie z `cost_gr_per_tonne_km`, a `pipeline_gr_per_tonne`
/// w `data/tuning/supply.ron` nie miało czytelnika. Wariant znikł z `Carrier`, bo
/// `M8b` §5.4 buduje rurociągi jako **sieci przesyłowe z taryfą i fakturą**, co jest
/// innym mechanizmem; drugi, martwy, był kosztem bez konsumenta.
#[derive(Clone, Debug)]
pub struct TradeNode {
    pub id: TradeNodeId,
    pub kind: GateKind,
    /// Fizyczne miejsce w mieście — węzeł ma rampę i magazyn jak każdy zakład.
    pub site: SiteId,
    pub slot: SlotId,
    pub capacity_per_day: Mass,
    pub used_today: Mass,
    pub backlog: Mass,
    pub base_lead_minutes: u32,
    pub goods: Vec<TradeGood>,
}

impl TradeNode {
    #[must_use]
    pub fn good(&self, good: GoodId) -> Option<&TradeGood> {
        self.goods.iter().find(|g| g.good == good)
    }

    pub fn good_mut(&mut self, good: GoodId) -> Option<&mut TradeGood> {
        self.goods.iter_mut().find(|g| g.good == good)
    }

    /// Czas dostawy z zaległością: `lead = base * (1 + backlog / capacity)` (§5.9).
    /// Przeciążony węzeł wydłuża kolejkę **wszystkim** — to jest cała różnica między
    /// kolejką a listą oczekujących.
    #[must_use]
    pub fn lead_minutes(&self, t: &TradeTuning, world_seed: u64, now: SimMinute) -> u32 {
        let baza = if self.capacity_per_day.0 <= 0 {
            u64::from(self.base_lead_minutes)
        } else {
            let mnoznik = 1 + self.backlog.0 / self.capacity_per_day.0;
            u64::from(self.base_lead_minutes) * mnoznik.max(1) as u64
        };
        if t.lead_jitter_bp <= 0 {
            return baza.min(u64::from(u32::MAX)) as u32;
        }
        let mut r = rng(
            world_seed,
            StreamId::SupplyImportLead,
            self.id.0,
            Tick(now.0),
        );
        let rozpietosc = (2 * t.lead_jitter_bp + 1) as u32;
        let szum = i64::from(r.gen_range_u32(rozpietosc)) - t.lead_jitter_bp;
        let z_szumem = (baza as i128 * i128::from(10_000 + szum) / 10_000).max(1);
        z_szumem.min(i128::from(u32::MAX)) as u32
    }

    /// Doba się skończyła: przepustowość wraca, zaległość maleje o to, co węzeł zdążył
    /// przerobić. Wolumen w oknie zjeżdża proporcjonalnie — 30 dób po 1/30 na dobę,
    /// zamiast pełnej historii zakupów.
    pub fn roll_day(&mut self, t: &TradeTuning) {
        let przerob = self.capacity_per_day.0;
        self.backlog = Mass((self.backlog.0 - przerob).max(0));
        self.used_today = Mass::ZERO;
        let dob = (t.window_minutes / 1_440).max(1);
        for g in &mut self.goods {
            g.bought_window = Mass(g.bought_window.0 - g.bought_window.0 / i64::from(dob));
        }
    }
}

impl HashState for TradeNode {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.id.0);
        h.write_u8(self.kind as u8);
        self.site.entity().hash_state(h);
        h.write_u32(self.slot.0);
        self.capacity_per_day.hash_state(h);
        self.used_today.hash_state(h);
        self.backlog.hash_state(h);
        h.write_u32(self.base_lead_minutes);
        h.write_u32(self.goods.len() as u32);
        for g in &self.goods {
            h.write_u16(g.good.0);
            g.base_price.hash_state(h);
            h.write_u32(g.elasticity_permille);
            g.window_reference.hash_state(h);
            g.bought_window.hash_state(h);
            h.write_u8(g.export_spread_pct);
            h.write_u16(g.tariff_class.0);
        }
    }
}

/// Wycena importu. **Cena i obciążenia osobno** (`K-7`): cło nigdy nie siedzi w środku
/// ceny, bo wtedy podwyżka cła wyglądałaby jak podrożenie towaru.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ImportQuote {
    pub node: TradeNodeId,
    pub good: GoodId,
    pub mass: Mass,
    /// Grosze za tonę, netto, loco węzeł.
    pub unit_price: Money,
    /// Wartość towaru netto.
    pub net: Money,
    /// Cło — pozycja osobna, nie składnik ceny.
    pub duty: Money,
    pub lead_minutes: u32,
    pub arrives_at: SimMinute,
}

impl ImportQuote {
    /// Kwota, którą kupujący faktycznie wyda: towar plus obciążenia.
    #[must_use]
    pub fn payable(&self) -> Money {
        Money(self.net.0 + self.duty.0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TradeError {
    NoNode,
    NotImportable,
    /// Węzeł nie obsługuje tej bramy dla tego towaru (`Good::import_via`).
    GateNotAllowed,
    BadMass,
}

/// Zamówienie importowe w drodze. Towar **nie istnieje** w mieście, dopóki nie dojedzie
/// do węzła — dlatego to nie jest partia, tylko zapowiedź partii.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PendingImport {
    pub node: TradeNodeId,
    pub buyer: FirmId,
    pub good: GoodId,
    pub mass: Mass,
    pub paid: Money,
    pub duty: Money,
    pub arrives_at: SimMinute,
    /// Zakład, do którego towar ma pojechać z węzła.
    pub deliver_to: SiteId,
    pub to_slot: SlotId,
}

/// Czy ten węzeł w ogóle wpuszcza ten towar (`Good::import_via`, `K-33`).
#[must_use]
pub fn gate_allows(cat: &Catalog, good: GoodId, kind: GateKind) -> bool {
    let g = cat.good(good);
    // Pusta lista znaczy „wszystkie bramy towarowe", a nie „żadna" — tak stoi w §5.1
    // i tak jest wypełniony katalog fali A.
    g.import_via.is_empty() || g.import_via.contains(&kind)
}

/// Powód decyzji o eksporcie (00 §7). `premium_bp` jest liczone **po** odjęciu transportu
/// do węzła — inaczej powód mówiłby, że eksport był o 14 % lepszy, a księga pokazywałaby
/// stratę, i gracz miałby rację, nie ufając panelowi.
#[must_use]
pub fn export_reason(good: GoodId, premium_bp: u16, mass: Mass) -> DecisionReason {
    DecisionReason::Firm(FirmReason::ExportChosen {
        good,
        premium_bp,
        mass_kg: (mass.0 / 1_000).clamp(0, i64::from(u32::MAX)) as u32,
    })
}

// ── Handel zagraniczny po stronie zasobu ────────────────────────────────────────
//
// Jak w `rfq.rs`: blok `impl B2b` stoi przy swoim module.

use crate::b2b::{B2b, SellerRef, Settlement};
use crate::store::{BatchDraft, MassIn, Store};
use crate::transport::{FreightOracle, Transport, TransportRequest, VehicleRequirements};
use crate::tuning::Tuning;

/// `ponytail:` blok jest cztery linie ponad progiem ostrzegawczym kontroli
/// strukturalnej (304 wobec 300; próg błędu to 500) i zostaje w całości świadomie.
/// Sufit nazwany: to jest **jeden temat** — obrót przez granicę — i sześć metod,
/// które dzielą stan węzła. Rozcięcie go rozdzieliłoby `import_quote` od
/// `place_import`, czyli wycenę od zamówienia, które tę wycenę konsumuje; para,
/// której nie wolno zmieniać osobno, ma stać obok siebie. Droga wyjścia, gdyby
/// dobiło do progu błędu: osobny plik na eksport (`try_export`, `absorb_exports`,
/// `export_price`), bo to jest drugi temat — ale dopiero wtedy, gdy eksport dostanie
/// własną logikę decyzyjną firmy, czyli w M7.
impl B2b {
    // ── WP9: import i eksport ───────────────────────────────────────────────────

    /// Wycena importu przez najtańszy węzeł wpuszczający ten towar (§5.9).
    #[must_use]
    pub fn import_quote(
        &self,
        cat: &Catalog,
        good: GoodId,
        mass: Mass,
        t: &Tuning,
        now: SimMinute,
    ) -> Option<ImportQuote> {
        if mass.0 <= 0 {
            return None;
        }
        let klasa = self.tariffs.class_of(cat.good(good).key.as_ref())?;
        let mut najlepsza: Option<ImportQuote> = None;
        for n in &self.nodes {
            if !gate_allows(cat, good, n.kind) {
                continue;
            }
            let Some(tg) = n.good(good) else { continue };
            // Szok podaży mnoży cenę świata zewnętrznego — i tylko ją. Elastyczność
            // wolumenowa i cło liczą się **od** niej, bo tak samo zachowuje się
            // prawdziwy: drożeje towar, a nie stawka celna.
            let cena = tg
                .import_price()
                .mul_ratio(i64::from(self.supply_shock(good)), 10_000);
            let netto = Money((i128::from(cena.0) * i128::from(mass.0) / 1_000_000) as i64);
            let lead = n.lead_minutes(&t.trade, self.world_seed, now);
            let q = ImportQuote {
                node: n.id,
                good,
                mass,
                unit_price: cena,
                net: netto,
                duty: netto.mul_ratio(self.tariffs.duty_bp(klasa), 10_000),
                lead_minutes: lead,
                arrives_at: SimMinute(now.0 + u64::from(lead)),
            };
            // Najtańszy po kwocie do zapłaty, remis po identyfikatorze węzła —
            // nigdy po kolejności w wektorze, choć akurat tu są tym samym.
            if najlepsza.is_none_or(|b| (q.payable().0, q.node.0) < (b.payable().0, b.node.0)) {
                najlepsza = Some(q);
            }
        }
        najlepsza
    }

    /// Składa zamówienie importowe. Towar **nie powstaje teraz** — powstaje w węźle,
    /// kiedy dojedzie ([`B2b::poll_imports`]).
    ///
    /// To tutaj import przestaje być darmowy: masa ponad dobową przepustowość węzła
    /// idzie w zaległość, a zaległość wydłuża czas dostawy wszystkim następnym.
    pub fn place_import(
        &mut self,
        q: ImportQuote,
        buyer: FirmId,
        deliver_to: SiteId,
        to_slot: SlotId,
    ) -> Result<(), TradeError> {
        let n = self
            .nodes
            .get_mut(q.node.0 as usize)
            .ok_or(TradeError::NoNode)?;
        let wolne = Mass((n.capacity_per_day.0 - n.used_today.0).max(0));
        let dzis = Mass(q.mass.0.min(wolne.0));
        n.used_today = Mass(n.used_today.0 + dzis.0);
        n.backlog = Mass(n.backlog.0 + q.mass.0 - dzis.0);
        if let Some(tg) = n.good_mut(q.good) {
            tg.bought_window = Mass(tg.bought_window.0 + q.mass.0);
        }
        self.pending.push(PendingImport {
            node: q.node,
            buyer,
            good: q.good,
            mass: q.mass,
            paid: q.net,
            duty: q.duty,
            arrives_at: q.arrives_at,
            deliver_to,
            to_slot,
        });
        Ok(())
    }

    /// Przyjmuje import, który dojechał: masa wchodzi do świata w magazynie węzła
    /// jako [`MassIn::Imported`], a stamtąd jedzie zwykłym zleceniem do kupującego.
    #[allow(clippy::too_many_arguments)]
    pub fn poll_imports(
        &mut self,
        cat: &Catalog,
        store: &mut Store,
        transport: &mut Transport,
        oracle: &dyn FreightOracle,
        now: SimMinute,
    ) -> Vec<Settlement> {
        let mut wynik = Vec::new();
        let mut zostaje = Vec::new();
        for p in std::mem::take(&mut self.pending) {
            if p.arrives_at.0 > now.0 {
                zostaje.push(p);
                continue;
            }
            let Some(n) = self.nodes.get(p.node.0 as usize) else {
                continue;
            };
            let (slot_wezla, site_wezla) = (n.slot, n.site);
            let draft = BatchDraft {
                good: p.good,
                mass: p.mass,
                // Jakość importu jest **stała i średnia**. `ponytail:` sufit nazwany:
                // świata zewnętrznego nie symulujemy, więc nie ma skąd wziąć jakości
                // partii. Droga wyjścia to pole w `TradeGood`, ale dopóki nikt nie może
                // go wypełnić czymś innym niż zgadywaniem, jedna stała jest uczciwsza.
                quality: magnat_core::Q::new(60),
                // Import **nie ma marki** i to nie jest przeoczenie: producent siedzi
                // poza miastem, a `p.buyer` jest importerem, nie wytwórcą. Wpisanie tu
                // marki importera znaczyłoby, że hurtownia buduje sobie renomę cudzym
                // towarem — czyli dokładnie to, czego PRD §7.6 zabrania.
                brand: None,
                producer: p.buyer,
                produced_at: now,
                // **Koszt nabycia obejmuje cło** — i to nie jest sprzeczne z `K-7`.
                // `K-7` mówi, że cło nie modyfikuje **ceny w ofercie**: sprzedawca
                // podaje netto, a obciążenie jest rozpisane osobno i to zostaje bez
                // zmian. Ale kupujący, który zapłacił netto **i** cło, ma towar
                // o takiej wartości i tyle wchodzi mu na stan; rozdzielenie tych dwóch
                // liczb po stronie zapasu rozjeżdżałoby `InventoryGoods` z wyceną
                // magazynu o sumę ceł (zmierzone w `po_roku_bilans_zamyka_sie_co_do_grosza`).
                cost: Money(p.paid.0 + p.duty.0),
                origin: crate::batch::BatchOrigin::imported(),
                flags: crate::batch::BatchFlags::default(),
            };
            if store.put(cat, slot_wezla, draft, MassIn::Imported).is_err() {
                // Węzeł pełny — towar czeka na zewnątrz, zamiast wyparować.
                zostaje.push(p);
                continue;
            }
            let powod = DecisionReason::Firm(FirmReason::Shortage {
                good: p.good,
                from: magnat_core::ShortageStageKind::Importing,
                to: magnat_core::ShortageStageKind::Ok,
                coverage_minutes: 0,
            });
            let id = transport.order(
                oracle,
                TransportRequest {
                    from: site_wezla,
                    to: p.deliver_to,
                    from_slot: slot_wezla,
                    to_slot: p.to_slot,
                    good: p.good,
                    mass: p.mass,
                    requires: VehicleRequirements::for_good(cat, p.good, p.mass),
                    ready_at: now,
                    due_at: SimMinute(now.0 + 1_440),
                },
                powod,
            );
            let _ = transport.dispatch(
                oracle,
                store,
                id,
                crate::transport::Carrier::Unassigned,
                now,
            );
            wynik.push(Settlement {
                buyer: p.buyer,
                deliver_to: p.deliver_to,
                // Import: sprzedawcą jest zagranica, więc zakładu sprzedającego nie ma.
                seller: SellerRef::External(p.node),
                seller_site: None,
                seller_cogs: Money::ZERO,
                good: p.good,
                mass: p.mass,
                net: p.paid,
                duty: p.duty,
                order: Some(id),
                reason: powod,
            });
        }
        self.pending = zostaje;
        wynik
    }

    #[must_use]
    pub fn pending_imports(&self) -> &[PendingImport] {
        &self.pending
    }

    /// Cena skupu eksportowego, po odjęciu kosztu dowozu do węzła. `None`, jeśli żaden
    /// węzeł tego towaru nie skupuje albo nie da się tam dojechać.
    #[must_use]
    pub fn export_price(
        &self,
        cat: &Catalog,
        good: GoodId,
        from: SiteId,
        mass: Mass,
        oracle: &dyn FreightOracle,
    ) -> Option<(TradeNodeId, Money)> {
        let wymagania = VehicleRequirements::for_good(cat, good, mass);
        let mut najlepszy: Option<(TradeNodeId, Money)> = None;
        for n in &self.nodes {
            let Some(tg) = n.good(good) else { continue };
            let Some(przewoz) = oracle.quote(from, n.site, mass, &wymagania) else {
                continue;
            };
            let brutto = i128::from(tg.export_price().0) * i128::from(mass.0) / 1_000_000;
            let netto = Money((brutto - i128::from(przewoz.cost.0)) as i64);
            if najlepszy.is_none_or(|(id, m)| (netto.0, n.id.0) > (m.0, id.0)) {
                najlepszy = Some((n.id, netto));
            }
        }
        najlepszy
    }

    /// Eksport masy z magazynu zakładu. Zwraca rozliczenie, jeśli cena zagraniczna bije
    /// `best_local` — **bez zaprogramowanej reguły drenażu**: masa znika z lokalnej podaży
    /// dlatego, że producent ją sprzedał, a nie dlatego, że tak każe tabela.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    pub fn try_export(
        &mut self,
        cat: &Catalog,
        store: &mut Store,
        transport: &mut Transport,
        good: GoodId,
        from: SiteId,
        from_slot: SlotId,
        mass: Mass,
        seller: FirmId,
        best_local: Money,
        oracle: &dyn FreightOracle,
        now: SimMinute,
    ) -> Option<Settlement> {
        let (node, netto) = self.export_price(cat, good, from, mass, oracle)?;
        let lokalnie = Money((i128::from(best_local.0) * i128::from(mass.0) / 1_000_000) as i64);
        if netto.0 <= lokalnie.0 {
            return None;
        }
        let premia = if lokalnie.0 > 0 {
            (i128::from(netto.0 - lokalnie.0) * 10_000 / i128::from(lokalnie.0))
                .clamp(0, i128::from(u16::MAX)) as u16
        } else {
            u16::MAX
        };
        let powod = export_reason(good, premia, mass);
        let (site_wezla, slot_wezla) = {
            let n = self.nodes.get(node.0 as usize)?;
            (n.site, n.slot)
        };
        // **Eksport jedzie ciężarówką** (`D6`). Odpisanie masy wprost z magazynu byłoby
        // o czterdzieści linii krótsze i czyniłoby z drenażu podaży fikcję księgową:
        // towar znikałby bez zajęcia pojazdu, kierowcy i rampy, więc eksport nie
        // konkurowałby z dostawami o tę samą flotę. Masa schodzi z bilansu dopiero
        // w [`B2b::absorb_exports`], po rozładunku w węźle.
        let id = transport.order(
            oracle,
            TransportRequest {
                from,
                to: site_wezla,
                from_slot,
                to_slot: slot_wezla,
                good,
                mass,
                requires: VehicleRequirements::for_good(cat, good, mass),
                ready_at: now,
                due_at: SimMinute(now.0 + 1_440),
            },
            powod,
        );
        transport
            .dispatch(
                oracle,
                store,
                id,
                crate::transport::Carrier::Unassigned,
                now,
            )
            .ok()?;
        // Towar zmienił właściciela w chwili załadunku — tak samo jak przy sprzedaży
        // hurtowej (`rfq.rs`, `AP-7`). Ładunek zlecenia jest **jedynym** miejscem,
        // w którym widać, które partie pojechały, a `resell` przepisuje ich koszt
        // własny na cenę i oddaje wołającemu to, co zszło z bilansu sprzedawcy.
        //
        // `R2-WP36`: do R2 stało tu `seller_site: None` z uzasadnieniem „masa schodzi
        // z bilansu dopiero po rozładunku w węźle, więc nie ma czym zmierzyć kosztu".
        // Uzasadnienie było nieprawdziwe: `dispatch` zdejmuje partie ze slotu
        // sprzedawcy **od razu** (`Store::load`), a z bilansu masy schodzą później —
        // to dwie różne rzeczy. Skutek był ten sam co przed `R2-WP7`: zakład
        // produkujący wyłącznie na eksport nie miał utargu, więc `margin_bp()`
        // zwracało „nie wiem" i tier taktyczny nie umiał go zamknąć.
        let koszt_sprzedawcy = match transport.get(id) {
            Some(o) => {
                let cargo = o.cargo.clone();
                store.resell(&cargo, netto)
            }
            None => Money::ZERO,
        };
        let n = self.nodes.get_mut(node.0 as usize)?;
        n.used_today = Mass(n.used_today.0 + mass.0);
        Some(Settlement {
            buyer: FirmId(site_wezla.entity()),
            deliver_to: site_wezla,
            seller: SellerRef::Firm(seller),
            seller_site: Some(from),
            seller_cogs: koszt_sprzedawcy,
            good,
            mass,
            net: netto,
            duty: Money::ZERO,
            order: Some(id),
            reason: powod,
        })
    }

    /// Wypuszcza poza miasto to, co dojechało do magazynów węzłów granicznych.
    ///
    /// To jest **jedyne** miejsce, w którym masa schodzi z bilansu jako `exported`.
    /// Osobny krok od [`B2b::try_export`], bo między decyzją a wyjazdem stoi przejazd,
    /// który może się nie udać — a towar, który nie dojechał do portu, nie został
    /// wyeksportowany i ma wrócić do podaży, a nie zniknąć.
    pub fn absorb_exports(&mut self, store: &mut Store) -> Vec<(TradeNodeId, GoodId, Mass)> {
        let mut wynik = Vec::new();
        for n in &self.nodes {
            // Kolejność po `GoodId`, czyli po indeksie katalogu — jawna i niezależna
            // od tego, w jakiej kolejności ciężarówki przyjechały (00 §3.2).
            let mut towary: Vec<GoodId> = n.goods.iter().map(|g| g.good).collect();
            towary.sort_unstable();
            for good in towary {
                let jest = store.available(n.slot, good, magnat_core::Q::MIN);
                if jest.0 <= 0 {
                    continue;
                }
                let (wyszlo, _koszt) = store.export(n.slot, good, jest);
                if wyszlo.0 > 0 {
                    wynik.push((n.id, good, wyszlo));
                }
            }
        }
        wynik
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn towar() -> TradeGood {
        TradeGood {
            good: GoodId(0),
            base_price: Money(78_000),
            elasticity_permille: 300,
            window_reference: Mass(1_000_000_000), // 1000 t
            bought_window: Mass::ZERO,
            export_spread_pct: 82,
            tariff_class: TariffClassId(0),
        }
    }

    /// Liczba z §5.9: kupno dziesięciokrotności wolumenu odniesienia przy elastyczności
    /// 300 ‰ podnosi cenę o 300 %, czyli do czterokrotności bazy.
    #[test]
    fn dziesieciokrotny_zakup_poczwarza_cene() {
        let mut g = towar();
        assert_eq!(g.import_price(), Money(78_000));
        g.bought_window = g.window_reference;
        assert_eq!(g.import_price(), Money(78_000 * 13 / 10));
        g.bought_window = Mass(g.window_reference.0 * 10);
        assert_eq!(g.import_price(), Money(78_000 * 4));
    }

    /// Cena skupu jest niższa od importowej **zawsze**, także wtedy, gdy miasto dużo
    /// kupuje — inaczej dałoby się zarabiać na obrocie w kółko przez samą granicę.
    #[test]
    fn skup_zawsze_ponizej_importu() {
        let mut g = towar();
        for kupione in [0, 500, 5_000, 50_000] {
            g.bought_window = Mass(kupione * 1_000_000);
            assert!(
                g.export_price().0 < g.import_price().0,
                "skup {} >= import {} przy {kupione} t",
                g.export_price().0,
                g.import_price().0
            );
        }
    }
}
