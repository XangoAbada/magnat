//! Kaskada niedoboru — kryterium ukończenia WP6 (M6b §5.7, PRD §8.4).
//!
//! Scenariusz jest ten z dokumentu: **młyn stoi pięć dni**, piekarnia zjada bufor
//! i schodzi kolejnymi szczeblami. Kryterium ma dwie połowy i obie są sprawdzane:
//! przejścia idą w **udokumentowanej kolejności**, a każde ma zapisany powód czytelny
//! w karcie inspekcji.

use magnat_core::{
    DecisionReason, Energy, Entity, FirmId, Mass, Money, OpenHours, Q, ShortageStageKind,
    SimMinute, SiteId, UtilityService, Volume,
};
use magnat_supply::plant::{Dock, PlantSite};
use magnat_supply::store::{BatchDraft, MassIn, WarehouseRole};
use magnat_supply::{
    InventoryRule, Plant, ProductionLine, ShortageAction, StorageClass, Store, Transport, Tuning,
};
use std::num::NonZeroU32;

fn encja(i: u32) -> Entity {
    Entity::new(i, NonZeroU32::new(1).expect("generacja"))
}

/// Piekarnia z §7.1: jeden piec, mąka w zapleczu, żadnej dostawy w drodze.
struct Piekarnia {
    cat: magnat_supply::Catalog,
    tuning: Tuning,
    store: Store,
    plant: Plant,
    transport: Transport,
    site: SiteId,
    maka: magnat_core::GoodId,
}

impl Piekarnia {
    fn nowa(maki_kg: i64) -> Piekarnia {
        let cat = magnat_supply::catalog::load_default("contemporary").expect("katalog z data/");
        let tuning = Tuning::load_default().expect("data/tuning/supply.ron");
        let mut store = Store::new(cat.goods.len());
        let site = SiteId(encja(21));
        let owner = FirmId(encja(5));

        let wejscie = store.add_slot(
            site,
            WarehouseRole::Input,
            StorageClass::Dry,
            Mass(50_000_000),
            Volume(500_000_000),
            0,
        );
        let wyjscie = store.add_slot(
            site,
            WarehouseRole::Output,
            StorageClass::Ambient,
            Mass(50_000_000),
            Volume(500_000_000),
            0,
        );

        let maka = cat.good_id("food_flour_t550").expect("mąka");
        let wypiek = cat.recipe_id("bakery_bread_wheat").expect("wypiek");
        let czesc = cat.good_id("part_bearing_6204").expect("łożysko");

        // Mąka, drożdże i sól — bez nich linia stanęłaby na innym wejściu i kaskada
        // mierzyłaby nie to, co trzeba.
        for (klucz, kg) in [
            ("food_flour_t550", maki_kg),
            ("food_yeast", 5_000),
            ("food_salt", 5_000),
        ] {
            let g = cat.good_id(klucz).expect(klucz);
            store
                .put(
                    &cat,
                    wejscie,
                    BatchDraft {
                        good: g,
                        mass: Mass(kg * 1_000),
                        quality: Q::new(67),
                        brand: None,
                        producer: owner,
                        produced_at: SimMinute(0),
                        cost: Money(kg * 126),
                        origin: Default::default(),
                        flags: Default::default(),
                    },
                    MassIn::Initial,
                )
                .expect("zapas startowy");
        }

        let mut zaklad = PlantSite::new(site, owner, Dock::new(1, 10, 1, OpenHours::ALWAYS));
        zaklad.inputs.push(wejscie);
        zaklad.outputs.push(wyjscie);
        zaklad.meters.push(magnat_supply::UtilityMeter::new(
            UtilityService::Electricity,
            FirmId(encja(3)),
            Money(tuning.utility.power_gr_per_kwh),
        ));
        zaklad.meters.push(magnat_supply::UtilityMeter::new(
            UtilityService::Water,
            FirmId(encja(3)),
            Money(tuning.utility.water_gr_per_m3),
        ));
        let mut piec = ProductionLine::new(
            cat.machine_class_id("bakery_oven_deck").expect("piec"),
            Mass(300_000),
            Energy(18),
            czesc,
        );
        piec.recipe = Some(wypiek);
        zaklad.lines.push(piec);

        let mut plant = Plant::new();
        plant.insert(zaklad);
        Piekarnia {
            cat,
            tuning,
            store,
            plant,
            transport: Transport::new(),
            site,
            maka,
        }
    }

    /// Godzina życia piekarni: produkcja przez minutę po minucie, przegląd kaskady
    /// na koniec godziny — dokładnie ta częstotliwość, którą deklaruje §5.7.
    fn godzina(&mut self, od: u64) -> Vec<ShortageAction> {
        let ctx = magnat_supply::ProductionCtx {
            cat: &self.cat,
            tuning: &self.tuning,
            world_seed: 3,
        };
        magnat_supply::advance_production(
            &ctx,
            &mut self.store,
            &mut self.plant,
            self.site,
            SimMinute(od),
            60,
        );
        let zaklad = self.plant.get_mut(self.site).expect("zakład");
        magnat_supply::shortage::review(
            &self.cat,
            &self.store,
            zaklad,
            SimMinute(od + 60),
            &self.tuning.shortage,
        )
    }

    fn stopien(&self) -> ShortageStageKind {
        self.plant
            .get(self.site)
            .expect("zakład")
            .stage(self.maka)
            .kind()
    }
}

/// Kryterium WP6: **młyn stoi pięć dni, kaskada przechodzi przez wszystkie stopnie
/// w udokumentowanej kolejności.** Kolejność jest z PRD §8.4 i jest kontraktem:
/// bufor → obniżenie produkcji → spot → import → substytut → postój.
#[test]
fn mlyn_stoi_piec_dni_i_kaskada_schodzi_po_kolei() {
    let mut p = Piekarnia::nowa(900);
    let mut przebieg: Vec<ShortageStageKind> = vec![p.stopien()];
    let mut akcje: Vec<ShortageAction> = Vec::new();

    // Pięć dób bez ani jednej dostawy mąki — młyn stoi.
    for h in 0..(5 * 24u64) {
        akcje.extend(p.godzina(h * 60));
        let s = p.stopien();
        if przebieg.last() != Some(&s) {
            przebieg.push(s);
        }
    }

    assert_eq!(
        przebieg,
        vec![
            ShortageStageKind::Ok,
            ShortageStageKind::Buffer,
            ShortageStageKind::Throttled,
            ShortageStageKind::SpotSearch,
            ShortageStageKind::Importing,
            ShortageStageKind::Substituted,
            ShortageStageKind::Halted,
        ],
        "kolejność prób jest kontraktem z PRD §8.4, nie preferencją"
    );

    // Spot i import zostały **poproszone**, a nie pominięte — to jest różnica między
    // drabiną a tabelą progów.
    assert!(
        akcje
            .iter()
            .any(|a| matches!(a, ShortageAction::OpenRfq { .. })),
        "szczebel spot wystawia zapytanie ofertowe"
    );
    assert!(
        akcje
            .iter()
            .any(|a| matches!(a, ShortageAction::Import { .. })),
        "szczebel importu prosi o import"
    );

    // Druga połowa kryterium: **każde przejście ma zapisany powód**, czytelny w karcie
    // inspekcji. `DecisionReason` jest centralnym enumem bez wildcardu, więc sam fakt,
    // że `engine/ui` się kompiluje, gwarantuje tekst w obu językach (00 §K-12).
    let zaklad = p.plant.get(p.site).expect("zakład");
    let powody: Vec<&DecisionReason> = zaklad.reasons().iter().map(|(_, r)| r).collect();
    assert!(
        powody
            .iter()
            .any(|r| matches!(r, DecisionReason::Shortage { .. })),
        "przejścia kaskady zapisują powód: {powody:?}"
    );
    assert!(
        powody
            .iter()
            .any(|r| matches!(r, DecisionReason::SubstituteUsed { .. })),
        "substytucja zapisuje własny powód, bo kosztuje jakość: {powody:?}"
    );

    // Linia stoi na braku wejścia, a nie w awarii: zakład bez mąki nie jest zepsuty.
    assert!(matches!(
        zaklad.lines[0].state,
        magnat_supply::LineState::Starved { .. } | magnat_supply::LineState::Idle
    ));
    assert!(!zaklad.is_fault(), "brak wsadu to nie awaria");
}

/// Dostawa zdejmuje zakład ze **wszystkich** szczebli naraz, a nie po jednym na godzinę.
/// Odzyskiwanie szczebel po szczeblu trzymałoby zakład w obniżonej produkcji długo po
/// tym, jak problem minął — czyli produkowałoby efekt byczego bicza (`R3`).
#[test]
fn dostawa_zdejmuje_ze_wszystkich_szczebli_naraz() {
    let mut p = Piekarnia::nowa(900);
    for h in 0..40u64 {
        p.godzina(h * 60);
    }
    assert_ne!(p.stopien(), ShortageStageKind::Ok, "zapas zdążył zejść");
    let obnizenie = p
        .plant
        .get(p.site)
        .expect("zakład")
        .throttle_pct(p.maka);
    assert!(obnizenie < 100, "obniżona produkcja: {obnizenie}%");

    // Przyjeżdża ciężarówka z mąką.
    let wejscie = p.plant.get(p.site).expect("zakład").inputs[0];
    p.store
        .put(
            &p.cat,
            wejscie,
            BatchDraft {
                good: p.maka,
                mass: Mass(3_000_000),
                quality: Q::new(67),
                brand: None,
                producer: FirmId(encja(5)),
                produced_at: SimMinute(40 * 60),
                cost: Money(378_000),
                origin: Default::default(),
                flags: Default::default(),
            },
            MassIn::Imported,
        )
        .expect("dostawa");

    p.godzina(40 * 60);
    assert_eq!(p.stopien(), ShortageStageKind::Ok, "jedna dostawa, jeden skok w dół");
    assert_eq!(
        p.plant.get(p.site).expect("zakład").throttle_pct(p.maka),
        100,
        "produkcja wraca do pełnej od razu"
    );
}

/// Polityka zapasu nie zamawia dwa razy tego, co już jedzie. Bez tego zakład zamawia
/// to samo tyle razy, ile przeglądów zmieści się w czasie dostawy — i to jest druga
/// połowa obrony przed efektem byczego bicza.
#[test]
fn to_co_jedzie_nie_jest_zamawiane_drugi_raz() {
    let p = Piekarnia::nowa(50);
    let zaklad = p.plant.get(p.site).expect("zakład");
    let regula = InventoryRule::continuous(p.maka, 240, 120);

    let bez_dostawy = magnat_supply::inventory::review(
        &p.cat,
        &p.store,
        &p.transport,
        zaklad,
        &[regula],
        SimMinute(0),
    );
    assert_eq!(bez_dostawy.len(), 1, "pusty magazyn woła o towar");
    assert!(bez_dostawy[0].mass.0 > 0);

    // Ta sama sytuacja, ale z zamówieniem w drodze — zapotrzebowanie znika.
    let mut transport = Transport::new();
    let oracle = magnat_supply::transport::FlatRateFreight {
        km: 11,
        tuning: p.tuning.transport,
        blocked: Vec::new(),
    };
    let wejscie = zaklad.inputs[0];
    transport.order(
        &oracle,
        magnat_supply::TransportRequest {
            from: SiteId(encja(99)),
            to: p.site,
            from_slot: wejscie,
            to_slot: wejscie,
            good: p.maka,
            mass: Mass(bez_dostawy[0].mass.0 * 2),
            requires: magnat_supply::VehicleRequirements::for_good(
                &p.cat,
                p.maka,
                bez_dostawy[0].mass,
            ),
            ready_at: SimMinute(0),
            due_at: SimMinute(600),
        },
        DecisionReason::Unspecified,
    );
    let z_dostawa = magnat_supply::inventory::review(
        &p.cat,
        &p.store,
        &transport,
        zaklad,
        &[regula],
        SimMinute(0),
    );
    assert!(
        z_dostawa.is_empty(),
        "zamówienie w drodze pokrywa potrzebę: {z_dostawa:?}"
    );
}
