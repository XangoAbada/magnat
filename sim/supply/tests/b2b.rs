//! Rynek B2B — kryteria ukończenia WP7, WP8 i WP9 (M6c §5.8, §5.9).
//!
//! Scenariusz jest ten z łańcucha referencyjnego §7.1: **trzy młyny i piekarnia**.
//! Młyny różnią się kosztem wytworzenia mąki i odległością, więc jest co rozstrzygać,
//! a odpowiedź da się sprawdzić ręcznie.

use magnat_core::{
    ContractId, Energy, Entity, FirmId, GateKind, GoodId, Mass, Money, OpenHours, SimMinute,
    SiteId, Volume, Q,
};
use magnat_supply::b2b::{
    contract::{ContractPricing, DeliverySchedule, Penalty, SupplyContractDraft},
    rfq::{RfqDraft, RfqOutcome, WhoTransports},
    trade::{TariffTable, TradeGood, TradeNode, TradeNodeId},
    B2b, SellerRef,
};
use magnat_supply::plant::{Dock, PlantSite};
use magnat_supply::store::{BatchDraft, MassIn, WarehouseRole};
use magnat_supply::transport::FlatRateFreight;
use magnat_supply::{
    Catalog, Plant, ProductionLine, ShortageAction, ShortageStage, StorageClass, Store, Transport,
    Tuning,
};
use std::num::NonZeroU32;

const SEED: u64 = 7;

fn encja(i: u32) -> Entity {
    Entity::new(i, NonZeroU32::new(1).expect("generacja"))
}

/// Miasto testowe: trzy młyny, piekarnia i węzeł kolejowy.
struct Miasto {
    cat: Catalog,
    tuning: Tuning,
    store: Store,
    plant: Plant,
    transport: Transport,
    b2b: B2b,
    oracle: FlatRateFreight,
    piekarnia: SiteId,
    mlyny: Vec<SiteId>,
    maka: GoodId,
    zboze: GoodId,
    wejscie_piekarni: magnat_supply::SlotId,
}

impl Miasto {
    /// `koszty_gr_za_tone` — koszt wytworzenia mąki w kolejnych młynach. Różne z rozmysłu:
    /// bez różnicy rozstrzygnięcie sprowadzałoby się do tie-breaku i test nie sprawdzałby
    /// funkcji celu, tylko porządek indeksów.
    fn nowe(koszty_gr_za_tone: &[i64], maki_w_mlynie_kg: i64) -> Miasto {
        let cat = magnat_supply::catalog::load_default("contemporary").expect("katalog z data/");
        let tuning = Tuning::load_default().expect("data/tuning/supply.ron");
        let mut store = Store::new(cat.goods.len());
        let maka = cat.good_id("food_flour_t550").expect("mąka");
        let zboze = cat.good_id("raw_wheat").expect("zboże");
        let przemial = cat.recipe_id("milling_wheat_t550").expect("przemiał");
        let wypiek = cat.recipe_id("bakery_bread_wheat").expect("wypiek");
        let czesc = cat.good_id("part_bearing_6204").expect("łożysko");

        let mut plant = Plant::new();
        let mut mlyny = Vec::new();
        for (i, koszt) in koszty_gr_za_tone.iter().enumerate() {
            let site = SiteId(encja(100 + i as u32));
            let owner = FirmId(encja(200 + i as u32));
            let wejscie = store.add_slot(
                site,
                WarehouseRole::Input,
                StorageClass::Silo,
                Mass(500_000_000),
                Volume(2_000_000_000),
                0,
            );
            let wyjscie = store.add_slot(
                site,
                WarehouseRole::Output,
                StorageClass::Dry,
                Mass(500_000_000),
                Volume(2_000_000_000),
                0,
            );
            store
                .put(
                    &cat,
                    wyjscie,
                    BatchDraft {
                        good: maka,
                        mass: Mass(maki_w_mlynie_kg * 1_000),
                        quality: Q::new(67),
                        brand: None,
                        producer: owner,
                        produced_at: SimMinute(0),
                        cost: Money(maki_w_mlynie_kg * koszt / 1_000),
                        origin: Default::default(),
                        flags: Default::default(),
                    },
                    MassIn::Initial,
                )
                .expect("mąka w młynie");
            let mut zaklad = PlantSite::new(site, owner, Dock::new(2, 25, 1, OpenHours::ALWAYS));
            zaklad.inputs.push(wejscie);
            zaklad.outputs.push(wyjscie);
            let mut walcowy = ProductionLine::new(
                cat.machine_class_id("mill_roller").expect("młyn"),
                Mass(3_000_000),
                Energy(55),
                czesc,
            );
            walcowy.recipe = Some(przemial);
            zaklad.lines.push(walcowy);
            plant.insert(zaklad);
            mlyny.push(site);
        }

        let piekarnia = SiteId(encja(21));
        let wlasciciel = FirmId(encja(5));
        let wejscie_piekarni = store.add_slot(
            piekarnia,
            WarehouseRole::Input,
            StorageClass::Dry,
            Mass(50_000_000),
            Volume(500_000_000),
            0,
        );
        let wyjscie_piekarni = store.add_slot(
            piekarnia,
            WarehouseRole::Output,
            StorageClass::Ambient,
            Mass(50_000_000),
            Volume(500_000_000),
            0,
        );
        let mut zaklad = PlantSite::new(
            piekarnia,
            wlasciciel,
            Dock::new(1, 10, 1, OpenHours::ALWAYS),
        );
        zaklad.inputs.push(wejscie_piekarni);
        zaklad.outputs.push(wyjscie_piekarni);
        let mut piec = ProductionLine::new(
            cat.machine_class_id("bakery_oven_deck").expect("piec"),
            Mass(300_000),
            Energy(18),
            czesc,
        );
        piec.recipe = Some(wypiek);
        zaklad.lines.push(piec);
        plant.insert(zaklad);

        let tariffs = TariffTable::load_default().expect("data/trade/tariffs.ron");
        let mut b2b = B2b::new(cat.goods.len(), tariffs, SEED);
        b2b.reindex(&cat, &plant);

        Miasto {
            cat,
            tuning,
            store,
            plant,
            transport: Transport::new(),
            b2b,
            oracle: FlatRateFreight {
                km: 22,
                tuning: Tuning::load_default().expect("strojenie").transport,
                blocked: Vec::new(),
            },
            piekarnia,
            mlyny,
            maka,
            zboze,
            wejscie_piekarni,
        }
    }

    /// Węzeł kolejowy z magazynem, skupujący i sprzedający zboże.
    fn z_wezlem(mut self, przepustowosc_t: i64, cena_gr_za_tone: i64) -> Miasto {
        let site = SiteId(encja(900));
        let slot = self.store.add_slot(
            site,
            WarehouseRole::Output,
            StorageClass::Silo,
            Mass(900_000_000),
            Volume(4_000_000_000),
            0,
        );
        let klasa = self
            .b2b
            .tariffs()
            .class_of(self.cat.good(self.zboze).key.as_ref())
            .expect("klasa taryfowa zboża");
        self.b2b.add_node(TradeNode {
            id: TradeNodeId(0),
            kind: GateKind::RailFreight,
            site,
            slot,
            capacity_per_day: Mass(przepustowosc_t * 1_000_000),
            used_today: Mass::ZERO,
            backlog: Mass::ZERO,
            base_lead_minutes: 480,
            goods: vec![TradeGood {
                good: self.zboze,
                base_price: Money(cena_gr_za_tone),
                elasticity_permille: self.tuning.trade.elasticity_permille,
                window_reference: Mass(przepustowosc_t * 1_000_000 * 30),
                bought_window: Mass::ZERO,
                export_spread_pct: self.tuning.trade.export_spread_pct,
                tariff_class: klasa,
            }],
        });
        self
    }

    fn zapytanie(&mut self, masa_kg: i64, now: u64) -> magnat_supply::RfqId {
        self.b2b.open_rfq(
            RfqDraft {
                buyer: FirmId(encja(5)),
                deliver_to: self.piekarnia,
                to_slot: self.wejscie_piekarni,
                good: self.maka,
                mass: Mass(masa_kg * 1_000),
                min_quality: Q::new(55),
                needed_by: SimMinute(now + 240),
                incoterm: WhoTransports::Seller,
            },
            &self.cat,
            &self.store,
            &self.plant,
            &self.oracle,
            &self.tuning,
            SimMinute(now),
        )
    }

    fn rozstrzygnij(&mut self, now: u64) -> Vec<magnat_supply::Settlement> {
        self.b2b.resolve_due(
            &self.cat,
            &mut self.store,
            &mut self.transport,
            &self.oracle,
            &self.tuning,
            SimMinute(now),
        )
    }
}

// ── WP7: rynek spot ─────────────────────────────────────────────────────────────

/// Kryterium WP7, pierwsza połowa: **ten sam seed daje tę samą wybraną ofertę.**
///
/// Dwa niezależne przebiegi, identyczna konfiguracja — ten sam zwycięzca, ta sama cena
/// i ta sama liczba ofert. Gdyby rozstrzygnięcie zależało od kolejności iteracji po
/// mapie albo od kolejności wpisu do kolekcji, ten test padłby losowo, a nie zawsze.
#[test]
fn ten_sam_seed_daje_te_sama_wybrana_oferte() {
    let wybierz = || {
        let mut m = Miasto::nowe(&[105_600, 98_400, 112_000], 40_000);
        let id = m.zapytanie(5_000, 0);
        let r = m.rozstrzygnij(61);
        assert_eq!(r.len(), 1, "jedno zapytanie, jedno rozstrzygnięcie");
        let rfq = m.b2b.rfq(id).expect("zapytanie").clone();
        (rfq.outcome, r[0].net, rfq.quotes.len())
    };
    let a = wybierz();
    let b = wybierz();
    assert_eq!(a, b, "rozstrzygnięcie RFQ musi być powtarzalne");
    assert_eq!(a.2, 3, "wszystkie trzy młyny stanęły w przetargu");
    assert!(
        matches!(a.0, RfqOutcome::Awarded { .. }),
        "zapytanie z ofertami kończy się wyborem, nie ciszą"
    );
}

/// Druga połowa kryterium: wygrywa **najniższy koszt całkowity**, a przy równym
/// transporcie to znaczy najtańszy młyn. Nie najbliższy, nie pierwszy w indeksie.
#[test]
fn wygrywa_najtanszy_wsad_gdy_transport_rowny() {
    let mut m = Miasto::nowe(&[105_600, 98_400, 112_000], 40_000);
    let id = m.zapytanie(5_000, 0);
    m.rozstrzygnij(61);
    let rfq = m.b2b.rfq(id).expect("zapytanie");
    let RfqOutcome::Awarded { quote, .. } = rfq.outcome else {
        panic!("brak rozstrzygnięcia");
    };
    let zwyciezca = rfq.quotes.iter().find(|q| q.id == quote).expect("oferta");
    let najtanszy = rfq
        .quotes
        .iter()
        .min_by_key(|q| q.price.0)
        .expect("jakaś oferta");
    assert_eq!(zwyciezca.id, najtanszy.id);
    // Drugi młyn ma koszt 98 400 gr/t; marża 18 % daje 116 112, plus szum do ±2 %.
    let oczekiwana = 98_400 * 11_800 / 10_000;
    let odchylenie = (zwyciezca.price.0 - oczekiwana).abs();
    assert!(
        odchylenie * 10_000 / oczekiwana <= 210,
        "cena {} odbiega od kosztu z marżą {oczekiwana} o więcej niż szum",
        zwyciezca.price.0
    );
}

/// Trasa niewykonalna wyklucza dostawcę **na etapie planowania** (§6.2), a nie w połowie
/// przejazdu. Zablokowany młyn nie startuje w przetargu, więc piekarnia od razu wie,
/// że musi szukać gdzie indziej.
#[test]
fn zablokowana_trasa_wyklucza_dostawce_z_przetargu() {
    let mut m = Miasto::nowe(&[105_600, 98_400, 112_000], 40_000);
    // Najtańszy młyn jest odcięty mostem o zbyt niskim tonażu.
    m.oracle
        .blocked
        .push((m.mlyny[1].entity().index(), m.piekarnia.entity().index()));
    let id = m.zapytanie(5_000, 0);
    m.rozstrzygnij(61);
    let rfq = m.b2b.rfq(id).expect("zapytanie");
    assert_eq!(rfq.quotes.len(), 2, "odcięty młyn nie składa oferty");
    let RfqOutcome::Awarded { quote, .. } = rfq.outcome else {
        panic!("brak rozstrzygnięcia");
    };
    let zwyciezca = rfq.quotes.iter().find(|q| q.id == quote).expect("oferta");
    assert_ne!(zwyciezca.from_site, m.mlyny[1]);
}

/// Towar naprawdę jedzie: po rozstrzygnięciu masa zeszła ze slotu sprzedawcy i wisi
/// w zleceniu transportowym. To jest ta sama reguła, której pilnuje `prop_no_teleport` —
/// rynek nie jest wyjątkiem od niej.
#[test]
fn rozstrzygniecie_wysyla_towar_zleceniem_a_nie_teleportem() {
    let mut m = Miasto::nowe(&[105_600, 98_400, 112_000], 40_000);
    let przed = m.store.total_stock(m.maka);
    let na_polkach: Vec<i64> = m
        .mlyny
        .iter()
        .map(|s| {
            let slot = m.plant.get(*s).expect("młyn").outputs[0];
            m.store.stock_of(slot, m.maka).0
        })
        .collect();
    m.zapytanie(5_000, 0);
    let rozliczenia = m.rozstrzygnij(61);
    assert_eq!(rozliczenia.len(), 1);
    let s = rozliczenia[0];
    assert!(s.order.is_some(), "bez zlecenia towar by się teleportował");
    assert!(matches!(s.seller, SellerRef::Firm(_)));
    assert_eq!(s.mass, Mass(5_000_000));
    // Masa zeszła z **półki sprzedawcy**, ale nie ze świata: partia w drodze dalej
    // siedzi w arenie i dlatego `total_stock` się nie rusza. To jest różnica, którą
    // łatwo pomylić, bo obie liczby nazywają się „zapas".
    let zwyciezca = rozliczenia[0].order.expect("zlecenie");
    let (skad, slot) = {
        let o = m.transport.get(zwyciezca).expect("zlecenie w rejestrze");
        (o.from, o.from_slot)
    };
    let i = m
        .mlyny
        .iter()
        .position(|s| *s == skad)
        .expect("zwycięski młyn");
    assert_eq!(m.store.stock_of(slot, m.maka).0, na_polkach[i] - 5_000_000);
    assert_eq!(m.store.total_stock(m.maka), przed, "masa nie wyparowała");
    m.store.check_mass(m.maka).expect("bilans masy mąki");
}

/// Zapytanie bez ani jednej oferty kończy się `NoQuotes`, a nie ciszą. To jest sygnał,
/// na którym kaskada wchodzi szczebel wyżej, do importu — bez niego drabina stałaby
/// na spocie w nieskończoność.
#[test]
fn brak_ofert_konczy_sie_jawna_odmowa() {
    let mut m = Miasto::nowe(&[105_600], 0);
    let id = m.zapytanie(5_000, 0);
    m.rozstrzygnij(61);
    assert_eq!(
        m.b2b.rfq(id).expect("zapytanie").outcome,
        RfqOutcome::NoQuotes
    );
    assert_eq!(m.b2b.open_rfq_count(), 0);
}

// ── WP8: kontrakty ──────────────────────────────────────────────────────────────

fn kara() -> Penalty {
    Penalty {
        per_tonne_missed: Money(12_000),
        cap_pct: 20,
        grace_minutes: 120,
    }
}

fn podpisz(m: &mut Miasto, mlyn: usize, masa_kg: i64, dob: u64) -> ContractId {
    let slot = m.plant.get(m.mlyny[mlyn]).expect("młyn").outputs[0];
    m.b2b
        .sign_contract(SupplyContractDraft {
            buyer: FirmId(encja(5)),
            seller: m.plant.get(m.mlyny[mlyn]).expect("młyn").owner,
            good: m.maka,
            min_quality: Q::new(55),
            deliver_from: m.mlyny[mlyn],
            from_slot: slot,
            deliver_to: m.piekarnia,
            to_slot: m.wejscie_piekarni,
            incoterm: WhoTransports::Seller,
            schedule: DeliverySchedule {
                every_minutes: 1_440,
                mass: Mass(masa_kg * 1_000),
                window: OpenHours::ALWAYS,
            },
            pricing: ContractPricing::Fixed {
                price: Money(124_600),
            },
            penalty: kara(),
            valid_from: SimMinute(0),
            valid_to: SimMinute(dob * 1_440),
            notice_minutes: 2_880,
        })
        .expect("kontrakt")
}

/// Kryterium WP8, pierwsza połowa — `prop_contract_penalty`:
/// **suma kar naliczonych równa sumie zapłaconych**, a `fulfilled + missed` równa się
/// masie wynikającej z harmonogramu **co do grama**.
#[test]
fn kary_umowne_naliczone_rownaja_sie_zaplaconym() {
    // Młyn ma mąkę na trzy dostawy, kontrakt żąda dziesięciu — siedem pójdzie w karę.
    let mut m = Miasto::nowe(&[105_600], 15_000);
    let id = podpisz(&mut m, 0, 5_000, 10);
    for d in 1..=10u64 {
        m.b2b.run_contracts(
            &m.cat,
            &mut m.store,
            &mut m.transport,
            &m.oracle,
            SimMinute(d * 1_440),
        );
        m.b2b.roll_day(&m.tuning, SimMinute(0));
    }

    let c = m.b2b.contract(id).expect("kontrakt");
    assert_eq!(
        c.fulfilled_mass.0 + c.missed_mass.0,
        c.scheduled_mass.0,
        "zrealizowane plus niedostarczone musi się zgadzać z harmonogramem"
    );
    // Dziewięć, nie dziesięć: `valid_to` jest **wyłączne**, więc dostawa wypadająca
    // dokładnie w minucie wygaśnięcia już się nie odbywa. Liczba jest tu wpisana wprost,
    // bo to jest ten rodzaj warunku brzegowego, który cicho przesuwa harmonogram o jedną
    // dostawę i widać go dopiero w rozliczeniu kary.
    assert_eq!(c.scheduled_mass.0, 9 * 5_000_000);
    assert!(c.missed_mass.0 > 0, "brakującej mąki nie da się dowieźć");
    let naliczone = c.penalty_accrued;
    assert!(naliczone.0 > 0, "niedostarczenie kosztuje");

    // Kara zapłacona w dwóch ratach — suma musi się zejść z naliczoną.
    let polowa = Money(naliczone.0 / 2);
    m.b2b.pay_penalty(id, polowa).expect("pierwsza rata");
    m.b2b
        .pay_penalty(id, Money(naliczone.0 - polowa.0))
        .expect("druga rata");
    let (a, p) = m.b2b.penalty_totals();
    assert_eq!(a, p, "suma kar naliczonych równa sumie zapłaconych");
    // Nadpłata nie przechodzi: dług zszedł do zera i nie da się zapłacić więcej.
    assert_eq!(m.b2b.pay_penalty(id, Money(1_000_000)), Ok(Money::ZERO));
}

/// Sufit `cap_pct` jest sufitem **całego kontraktu**, nie jednej dostawy — inaczej
/// kontrakt na rok byłby dwunastokrotnie droższy do zerwania niż kontrakt na miesiąc
/// o tych samych warunkach.
#[test]
fn kara_nie_przekracza_sufitu_kontraktu() {
    let mut m = Miasto::nowe(&[105_600], 0);
    let id = podpisz(&mut m, 0, 5_000, 30);
    for d in 1..=30u64 {
        m.b2b.run_contracts(
            &m.cat,
            &mut m.store,
            &mut m.transport,
            &m.oracle,
            SimMinute(d * 1_440),
        );
    }
    let c = m.b2b.contract(id).expect("kontrakt");
    let wartosc = c.total_value(Money::ZERO);
    assert!(
        c.penalty_accrued.0 <= wartosc.0 * 20 / 100,
        "kara {} ponad 20 % wartości kontraktu {}",
        c.penalty_accrued.0,
        wartosc.0
    );
}

/// Cennik indeksowany chodzi za **średnią ceną spot z siedmiu dób** i przelicza się
/// całkowitoliczbowo. Bez obrotu nie ma indeksu — i wtedy obowiązuje cena bazowa,
/// a nie zero.
#[test]
fn indeksacja_bierze_srednia_spot_a_bez_obrotu_stoi_na_bazie() {
    let mut m = Miasto::nowe(&[105_600, 98_400], 40_000);
    assert_eq!(m.b2b.spot_index(m.maka), None, "bez obrotu nie ma indeksu");

    m.zapytanie(5_000, 0);
    m.rozstrzygnij(61);
    let indeks = m.b2b.spot_index(m.maka).expect("po transakcji indeks jest");
    assert!(indeks.0 > 0);

    let cennik = ContractPricing::Indexed {
        base: Money(124_600),
        index: m.maka,
        ref_price: Money(116_000),
        pass_through_pct: 85,
    };
    let cena = cennik.price_at(indeks);
    let recznie = 124_600 + (indeks.0 - 116_000) * 85 / 100;
    assert_eq!(cena.0, recznie, "indeksacja bez dryfu zmiennoprzecinkowego");
}

/// Wypowiedzenie kosztuje i **nie kończy kontraktu natychmiast**: dostawy lecą do końca
/// okresu wypowiedzenia. Darmowe wyjście czyniłoby z kontraktu deklarację intencji.
#[test]
fn wypowiedzenie_kosztuje_i_nie_ucina_kontraktu_od_razu() {
    let mut m = Miasto::nowe(&[105_600], 40_000);
    let id = podpisz(&mut m, 0, 5_000, 30);
    let kupujacy = FirmId(encja(5));
    let oplata = m
        .b2b
        .terminate_contract(id, kupujacy, SimMinute(5_000))
        .expect("wypowiedzenie");
    assert!(oplata.0 > 0, "wypowiedzenie ma cenę");
    let c = m.b2b.contract(id).expect("kontrakt");
    assert_eq!(c.terminated_at, Some(SimMinute(5_000 + 2_880)));
    assert!(
        c.is_active(SimMinute(6_000)),
        "w okresie wypowiedzenia trwa"
    );
    assert!(!c.is_active(SimMinute(8_000)), "po okresie już nie");

    // Obcy nie wypowiada cudzego kontraktu.
    assert_eq!(
        m.b2b
            .terminate_contract(id, FirmId(encja(999)), SimMinute(5_100)),
        Err(magnat_supply::ContractError::NotAParty)
    );
}

// ── WP9: import i eksport ───────────────────────────────────────────────────────

/// Kryterium WP9, pierwsza połowa: **import nie jest darmowym zaworem.**
///
/// Zakup dziesięciokrotności dobowej przepustowości węzła kończy się kolejką i wzrostem
/// ceny, a nie natychmiastową dostawą. Sprawdzane są obie połowy naraz, bo każda z osobna
/// dałaby się spełnić po taniości: sama kolejka bez ceny znaczyłaby „poczekaj i bierz",
/// sama cena bez kolejki — „zapłać i masz od ręki".
#[test]
fn import_dziesieciu_przepustowosci_daje_kolejke_i_wzrost_ceny() {
    let mut m = Miasto::nowe(&[105_600], 0).z_wezlem(100, 78_000);
    let kupujacy = FirmId(encja(5));

    let pierwsza = m
        .b2b
        .import_quote(&m.cat, m.zboze, Mass(1_000_000), &m.tuning, SimMinute(0))
        .expect("węzeł skupuje zboże");
    assert_eq!(
        pierwsza.unit_price,
        Money(78_000),
        "pusty węzeł, cena bazowa"
    );

    // Dziesięciokrotność dobowej przepustowości: 1000 t przy 100 t/dobę.
    let duza = m
        .b2b
        .import_quote(
            &m.cat,
            m.zboze,
            Mass(1_000_000_000),
            &m.tuning,
            SimMinute(0),
        )
        .expect("wycena dużego zakupu");
    m.b2b
        .place_import(duza, kupujacy, m.piekarnia, m.wejscie_piekarni)
        .expect("zamówienie");

    let n = m.b2b.node(TradeNodeId(0)).expect("węzeł");
    assert_eq!(
        n.backlog,
        Mass(900_000_000),
        "dziewięćset ton ponad przepustowość idzie w zaległość"
    );
    let dluzszy = n.lead_minutes(&m.tuning.trade, SEED, SimMinute(0));
    assert!(
        dluzszy > 480 * 5,
        "zaległość musi realnie wydłużyć kolejkę, a nie tylko ją odnotować (było {dluzszy})"
    );

    let druga = m
        .b2b
        .import_quote(&m.cat, m.zboze, Mass(1_000_000), &m.tuning, SimMinute(60))
        .expect("wycena po dużym zakupie");
    assert!(
        druga.unit_price.0 > pierwsza.unit_price.0,
        "cena importowa rośnie z wolumenem: {} nie jest większe od {}",
        druga.unit_price.0,
        pierwsza.unit_price.0
    );
    assert!(
        m.b2b.pending_imports().len() == 1,
        "towar jeszcze nie dojechał"
    );
}

/// Cło jest **osobną pozycją**, nie składnikiem ceny (`K-7`). Podwyżka cła ma być
/// w panelu widoczna jako obciążenie, a nie jako towar, który nagle podrożał.
#[test]
fn clo_stoi_obok_ceny_a_nie_w_niej() {
    let m = Miasto::nowe(&[105_600], 0).z_wezlem(100, 78_000);
    let q = m
        .b2b
        .import_quote(&m.cat, m.zboze, Mass(10_000_000), &m.tuning, SimMinute(0))
        .expect("wycena");
    assert_eq!(q.net, Money(78_000 * 10), "netto to sama wartość towaru");
    let stawka = m
        .b2b
        .tariffs()
        .duty_bp(m.b2b.tariffs().class_of("raw_wheat").expect("klasa"));
    assert_eq!(q.duty, Money(q.net.0 * stawka / 10_000));
    assert_eq!(q.payable(), Money(q.net.0 + q.duty.0));
    assert!(q.duty.0 > 0, "stub taryfy ma być odróżnialny od zera");
}

/// Import dojeżdża: masa wchodzi do świata w magazynie węzła jako `MassIn::Imported`,
/// bilans domyka się, a towar jedzie do kupującego zwykłym zleceniem.
#[test]
fn import_dojezdza_i_bilans_masy_sie_domyka() {
    let mut m = Miasto::nowe(&[105_600], 0).z_wezlem(100, 78_000);
    let kupujacy = FirmId(encja(5));
    let q = m
        .b2b
        .import_quote(&m.cat, m.zboze, Mass(20_000_000), &m.tuning, SimMinute(0))
        .expect("wycena");
    let przyjazd = q.arrives_at;
    m.b2b
        .place_import(q, kupujacy, m.piekarnia, m.wejscie_piekarni)
        .expect("zamówienie");

    // Przed czasem nic nie przyjeżdża — to jest cała różnica między kolejką a życzeniem.
    let wczesniej = m.b2b.poll_imports(
        &m.cat,
        &mut m.store,
        &mut m.transport,
        &m.oracle,
        SimMinute(przyjazd.0 - 1),
    );
    assert!(wczesniej.is_empty());
    assert_eq!(m.store.total_stock(m.zboze), Mass::ZERO);

    let teraz = m
        .b2b
        .poll_imports(&m.cat, &mut m.store, &mut m.transport, &m.oracle, przyjazd);
    assert_eq!(teraz.len(), 1);
    assert!(matches!(teraz[0].seller, SellerRef::External(_)));
    assert!(teraz[0].duty.0 > 0, "cło jedzie razem z rozliczeniem");
    m.store.check_mass(m.zboze).expect("bilans masy zboża");
}

/// Kryterium WP9, druga połowa: **eksport zabiera masę z lokalnej podaży.**
///
/// Bez zaprogramowanej reguły drenażu — producent po prostu sprzedał drożej za granicę.
/// Eksport **jedzie ciężarówką** (`D6`): masa schodzi z bilansu dopiero po rozładunku
/// w węźle, a nie w chwili decyzji. Bilans domyka się, bo wyjście jest zaksięgowane
/// jako `exported`, a nie zgubione.
#[test]
fn eksport_zabiera_mase_z_lokalnej_podazy() {
    let mut m = Miasto::nowe(&[105_600], 0).z_wezlem(1_000, 200_000);
    let mlyn = m.mlyny[0];
    let slot = m.plant.get(mlyn).expect("młyn").inputs[0];
    let wlasciciel = m.plant.get(mlyn).expect("młyn").owner;
    m.store
        .put(
            &m.cat,
            slot,
            BatchDraft {
                good: m.zboze,
                mass: Mass(100_000_000),
                quality: Q::new(64),
                brand: None,
                producer: wlasciciel,
                produced_at: SimMinute(0),
                cost: Money(78_000 * 100),
                origin: Default::default(),
                flags: Default::default(),
            },
            MassIn::Initial,
        )
        .expect("zboże w młynie");
    let przed = m.store.total_stock(m.zboze);

    // Cena skupu 200 000 × 82 % = 164 000 gr/t bije lokalne 90 000.
    let s = m
        .b2b
        .try_export(
            &m.cat,
            &mut m.store,
            &mut m.transport,
            m.zboze,
            mlyn,
            slot,
            Mass(60_000_000),
            wlasciciel,
            Money(90_000),
            &m.oracle,
            SimMinute(0),
        )
        .expect("eksport wygrywa z ceną lokalną");
    assert_eq!(s.mass, Mass(60_000_000));
    let zlecenie = s
        .order
        .expect("eksport zajmuje ciężarówkę, a nie tylko księgę");
    assert!(matches!(
        s.reason,
        magnat_core::DecisionReason::ExportChosen { .. }
    ));
    // Przed rozładunkiem w porcie towar jest **w drodze**, a nie za granicą: nie zszedł
    // jeszcze z bilansu, bo przejazd może się nie udać.
    assert_eq!(
        m.store.total_stock(m.zboze),
        przed,
        "w drodze, nie za granicą"
    );
    assert!(m.b2b.absorb_exports(&mut m.store).is_empty());

    m.transport
        .deliver(&m.cat, &mut m.store, zlecenie, SimMinute(60))
        .expect("rozładunek w porcie");
    let wypuszczone = m.b2b.absorb_exports(&mut m.store);
    assert_eq!(wypuszczone.len(), 1);
    assert_eq!(wypuszczone[0].2, Mass(60_000_000));
    assert_eq!(
        m.store.total_stock(m.zboze).0,
        przed.0 - 60_000_000,
        "wyeksportowane zboże znika z lokalnej podaży"
    );
    m.store
        .check_mass(m.zboze)
        .expect("bilans masy po eksporcie");
}

/// Symetrycznie: przy dobrej cenie lokalnej eksport **nie zachodzi**. Gdyby zachodził
/// zawsze, „drenaż" nie byłby decyzją, tylko regułą — a to jest dokładnie to, czego §5.9
/// zabrania.
#[test]
fn przy_dobrej_cenie_lokalnej_eksport_nie_zachodzi() {
    let mut m = Miasto::nowe(&[105_600], 0).z_wezlem(1_000, 78_000);
    let mlyn = m.mlyny[0];
    let slot = m.plant.get(mlyn).expect("młyn").inputs[0];
    let wlasciciel = m.plant.get(mlyn).expect("młyn").owner;
    m.store
        .put(
            &m.cat,
            slot,
            BatchDraft {
                good: m.zboze,
                mass: Mass(100_000_000),
                quality: Q::new(64),
                brand: None,
                producer: wlasciciel,
                produced_at: SimMinute(0),
                cost: Money(78_000 * 100),
                origin: Default::default(),
                flags: Default::default(),
            },
            MassIn::Initial,
        )
        .expect("zboże");
    let przed = m.store.total_stock(m.zboze);
    let brak = m.b2b.try_export(
        &m.cat,
        &mut m.store,
        &mut m.transport,
        m.zboze,
        mlyn,
        slot,
        Mass(60_000_000),
        wlasciciel,
        // Lokalnie 120 000 gr/t bije skup 78 000 × 82 % = 63 960.
        Money(120_000),
        &m.oracle,
        SimMinute(0),
    );
    assert!(brak.is_none());
    assert_eq!(m.store.total_stock(m.zboze), przed);
}

// ── spięcie z kaskadą niedoboru ─────────────────────────────────────────────────

/// `AG-3` i `AG-4` w jednym teście: kaskada wypuszcza akcje, rynek je konsumuje, a stopnie
/// przestają nieść zaślepki. `SpotSearch` dostaje **prawdziwy** `RfqId`, a `Importing` —
/// czas dostawy z węzła, zamiast stałej ośmiu godzin wpisanej w M6b.
#[test]
fn rynek_wypelnia_zaslepki_kaskady_prawdziwymi_uchwytami() {
    let mut m = Miasto::nowe(&[105_600], 40_000).z_wezlem(100, 78_000);
    let piekarnia = m.piekarnia;
    let maka = m.maka;

    let akcje = vec![
        ShortageAction::OpenRfq {
            site: piekarnia,
            good: maka,
            mass: Mass(5_000_000),
        },
        ShortageAction::Import {
            site: piekarnia,
            good: m.zboze,
            mass: Mass(3_000_000),
        },
    ];
    // Kaskada musi mieć wpis, żeby było co nadpisać — `set_stage` nie tworzy szczebla
    // bez historii.
    {
        let z = m.plant.get_mut(piekarnia).expect("piekarnia");
        for g in [maka, m.zboze] {
            z.shortage.push(magnat_supply::shortage::ShortageState {
                good: g,
                stage: ShortageStage::Throttled { pct: 50 },
                since: SimMinute(0),
                coverage_minutes: 90,
                throttle_pct: 50,
            });
        }
    }

    let (cat, store, tuning, oracle) = (&m.cat, &m.store, &m.tuning, &m.oracle);
    let mut plant = std::mem::take(&mut m.plant);
    m.b2b.serve(
        &akcje,
        cat,
        store,
        &mut plant,
        oracle,
        tuning,
        SimMinute(600),
    );
    m.plant = plant;

    let z = m.plant.get(piekarnia).expect("piekarnia");
    match z.stage(maka) {
        ShortageStage::SpotSearch { rfq } => {
            assert_ne!(
                rfq,
                magnat_supply::RfqId::default(),
                "uchwyt nie jest zaślepką"
            );
            assert!(m.b2b.rfq(rfq).is_some(), "zapytanie naprawdę powstało");
        }
        inny => panic!("spodziewano się SpotSearch, jest {inny:?}"),
    }
    match z.stage(m.zboze) {
        ShortageStage::Importing { eta } => {
            assert!(eta.0 > 600, "czas dostawy musi leżeć w przyszłości");
            assert_ne!(
                eta.0,
                600 + 8 * 60,
                "stała ośmiu godzin z M6b miała zniknąć razem z węzłem granicznym"
            );
        }
        inny => panic!("spodziewano się Importing, jest {inny:?}"),
    }
}

/// Każdy towar katalogu ma klasę taryfową. Reguła pokrycia jest w danych (`AD-12`:
/// dwanaście domen, lista zamknięta), więc nowy towar nie może zgubić klasy przez
/// przeoczenie — a gdyby zgubił, `import_quote` zwracałoby `None` i import po cichu
/// przestałby istnieć dla tego towaru.
#[test]
fn kazda_domena_ma_klase_taryfowa() {
    let cat = magnat_supply::catalog::load_default("contemporary").expect("katalog");
    let t = TariffTable::load_default().expect("taryfy");
    t.check_covers(&cat).expect("pokrycie domen jest pełne");
}

// ── M10e WP10.13: zaufanie do dostawcy ──────────────────────────────────────────

/// Kryterium WP10.13, pierwsza połowa: **zaufanie realnie zmienia wybór dostawcy,
/// a firma płaci za nie z własnej kieszeni.**
///
/// Trzy młyny, z których najtańszy nie jest tym, z którym piekarnia handluje od
/// lat. Bez historii dostaw wygrywa cena; z historią wygrywa dostawca droższy
/// o dwa procent — i to jest różnica, o którą chodzi. Cena zapłacona **nie
/// spada**: rabat jest preferencją w funkcji celu, a nie obniżką, więc piekarnia
/// faktycznie przepłaca za rzetelność.
#[test]
fn stala_wspolpraca_bije_nizsza_cene_i_widac_to_w_powodzie() {
    // Młyn 0 jest o ~2 % droższy od młyna 1, więc bez relacji przegrywa.
    let koszty = [100_400, 98_400, 112_000];
    let piekarnia = FirmId(encja(5));
    let mlyn0 = FirmId(encja(200));

    let bez_relacji = {
        let mut m = Miasto::nowe(&koszty, 40_000);
        let id = m.zapytanie(5_000, 0);
        m.rozstrzygnij(61);
        let rfq = m.b2b.rfq(id).expect("zapytanie");
        let RfqOutcome::Awarded { seller, .. } = rfq.outcome else {
            panic!("brak rozstrzygnięcia");
        };
        (
            seller,
            rfq.quotes.iter().map(|q| q.price.0).collect::<Vec<_>>(),
        )
    };
    assert_ne!(
        bez_relacji.0, mlyn0,
        "bez historii dostaw wygrywa cena — inaczej test nie mierzy niczego"
    );

    let mut m = Miasto::nowe(&koszty, 40_000);
    // Sześćdziesiąt terminowych dostaw od młyna 0 — zaufanie dochodzi do setki,
    // czyli do trzech procent przewagi w ocenie.
    for d in 0..60 {
        m.b2b.relations_mut().record(
            magnat_supply::DeliveryOutcome {
                buyer: piekarnia,
                supplier: mlyn0,
                good: m.maka,
                delivered: Mass(5_000_000),
                missed: Mass::ZERO,
                late: false,
            },
            SimMinute(d * 1_440),
        );
    }
    let zaufanie = m
        .b2b
        .relations()
        .get(piekarnia, mlyn0, m.maka)
        .expect("relacja")
        .trust;
    assert!(
        zaufanie.get() >= 95,
        "zaufanie po sześćdziesięciu dostawach"
    );

    let id = m.zapytanie(5_000, 0);
    let rozliczenia = m.rozstrzygnij(61);
    let rfq = m.b2b.rfq(id).expect("zapytanie").clone();
    let RfqOutcome::Awarded { seller, quote } = rfq.outcome else {
        panic!("brak rozstrzygnięcia");
    };
    assert_eq!(
        seller, mlyn0,
        "stały dostawca ma wygrać mimo wyższej ceny (kryterium WP10.13)"
    );

    let zwyciezca = rfq.quotes.iter().find(|q| q.id == quote).expect("oferta");
    let najtanszy = rfq.quotes.iter().map(|q| q.price.0).min().expect("oferty");
    assert!(
        zwyciezca.price.0 > najtanszy,
        "piekarnia ma **przepłacić**: {} wobec najtańszych {najtanszy}",
        zwyciezca.price.0
    );
    // Sufit, a nie pasmo: ile dokładnie piekarnia przepłaci, zależy od szumu
    // wyceny (±2 %), ale **nigdy** więcej niż wynosi preferencja — powyżej trzech
    // procent zaufanie przestaje wystarczać i wraca cena.
    let nadplata_bp = (zwyciezca.price.0 - najtanszy) * 10_000 / najtanszy;
    assert!(
        nadplata_bp <= 300,
        "nadpłata {nadplata_bp} bp przekracza sufit preferencji"
    );
    assert_eq!(
        rozliczenia.len(),
        1,
        "rozstrzygnięcie ma wysłać towar, a nie samo zmienić zdanie"
    );
    assert!(
        matches!(
            rozliczenia[0].reason,
            magnat_core::DecisionReason::TrustedSupplier { supplier, .. } if supplier == mlyn0
        ),
        "powód ma nazywać zaufanie, a nie cenę: {:?}",
        rozliczenia[0].reason
    );
}

/// Druga połowa: **zawiedziony dostawca traci preferencję.** Relacja nie jest
/// odznaką na zawsze — seria niedostarczonych dostaw zbija zaufanie poniżej progu
/// obojętności i przetarg wraca do liczenia samej ceny.
#[test]
fn zawiedzione_zaufanie_oddaje_przetarg_cenie() {
    let koszty = [100_400, 98_400, 112_000];
    let piekarnia = FirmId(encja(5));
    let mlyn0 = FirmId(encja(200));
    let mut m = Miasto::nowe(&koszty, 40_000);
    for d in 0..60 {
        m.b2b.relations_mut().record(
            magnat_supply::DeliveryOutcome {
                buyer: piekarnia,
                supplier: mlyn0,
                good: m.maka,
                delivered: Mass(5_000_000),
                missed: Mass::ZERO,
                late: false,
            },
            SimMinute(d * 1_440),
        );
    }
    for d in 60..90 {
        m.b2b.relations_mut().record(
            magnat_supply::DeliveryOutcome {
                buyer: piekarnia,
                supplier: mlyn0,
                good: m.maka,
                delivered: Mass::ZERO,
                missed: Mass(5_000_000),
                late: true,
            },
            SimMinute(d * 1_440),
        );
    }
    assert_eq!(
        m.b2b.relations().discount_bp(piekarnia, mlyn0, m.maka),
        0,
        "po trzydziestu zawalonych dostawach nie ma żadnej preferencji"
    );
    let id = m.zapytanie(5_000, 0);
    m.rozstrzygnij(61);
    let rfq = m.b2b.rfq(id).expect("zapytanie");
    let RfqOutcome::Awarded { seller, .. } = rfq.outcome else {
        panic!("brak rozstrzygnięcia");
    };
    assert_ne!(seller, mlyn0, "zawiedziony dostawca przegrywa z ceną");
}
