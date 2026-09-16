//! Zakład fizyczny — kryterium ukończenia WP4 (M6b).
//!
//! Cztery zdania z kryterium, cztery testy: linia bez prądu stoi, awaria wymaga części
//! z magazynu, przezbrojenie kosztuje czas i masę, a spójność LOD mikro↔mezo ma
//! tolerancję 0. Piąty test pilnuje, że bilans masy i pieniądza domyka się po produkcji,
//! bo to jest niezmiennik, którego żaden z tamtych czterech sam nie sprawdzi.
//!
//! Katalog jest **prawdziwy** (`data/`), a nie atrapą: młyn z §7.1 dokumentu fazy jest
//! jedynym miejscem, w którym liczby z planu spotykają się z kodem.

use magnat_core::{
    Energy, Entity, FirmId, GoodId, LossKind, Mass, Money, OpenHours, Q, SimMinute, SiteId,
    UtilityService, Volume,
};
use magnat_supply::catalog::load_default;
use magnat_supply::plant::{advance_production, Dock, PlantSite, ProductionCtx};
use magnat_supply::store::{BatchDraft, MassIn, WarehouseRole};
use magnat_supply::{
    BreakCause, Catalog, LineState, Plant, ProductionLine, SlotId, StorageClass, Store, Tuning,
};
use std::num::NonZeroU32;

const ZAKLAD: u32 = 11;

fn encja(i: u32) -> Entity {
    Entity::new(i, NonZeroU32::new(1).expect("generacja"))
}

/// Młyn z §7.1: jedna linia `mill_roller` o przepustowości 3 t/h, jedna zmiana dzienna,
/// licznik prądu i wody, magazyn wejściowy pełen zboża.
struct Mlyn {
    cat: Catalog,
    tuning: Tuning,
    store: Store,
    plant: Plant,
    site: SiteId,
    wejscie: SlotId,
    zboze: GoodId,
    maka: GoodId,
}

impl Mlyn {
    fn nowy(zboza_kg: i64) -> Mlyn {
        let cat = load_default("contemporary").expect("katalog z data/");
        let tuning = Tuning::load_default().expect("data/tuning/supply.ron");
        let mut store = Store::new(cat.goods.len());
        let site = SiteId(encja(ZAKLAD));
        let owner = FirmId(encja(2));

        let wejscie = store.add_slot(
            site,
            WarehouseRole::Input,
            StorageClass::Silo,
            Mass(400_000_000),
            Volume(4_000_000_000),
            0,
        );
        let wyjscie = store.add_slot(
            site,
            WarehouseRole::Output,
            StorageClass::Dry,
            Mass(400_000_000),
            Volume(4_000_000_000),
            0,
        );

        let zboze = cat.good_id("raw_wheat").expect("raw_wheat");
        let maka = cat.good_id("food_flour_t550").expect("food_flour_t550");
        let przemial = cat.recipe_id("milling_wheat_t550").expect("milling_wheat_t550");
        let czesc = cat.good_id("part_bearing_6204").expect("part_bearing_6204");

        store
            .put(
                &cat,
                wejscie,
                BatchDraft {
                    good: zboze,
                    mass: Mass(zboza_kg * 1_000),
                    quality: Q::new(64),
                    brand: None,
                    producer: owner,
                    produced_at: SimMinute(0),
                    cost: Money(zboza_kg * 78),
                    origin: Default::default(),
                    flags: Default::default(),
                },
                MassIn::Initial,
            )
            .expect("zboże do silosu");

        let mut zaklad = PlantSite::new(site, owner, Dock::new(2, 25, 1, OpenHours::ALWAYS));
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
        let mut linia = ProductionLine::new(
            cat.machine_class_id("mill_roller").expect("klasa maszyny"),
            Mass(3_000_000),
            Energy(55),
            czesc,
        );
        linia.recipe = Some(przemial);
        zaklad.lines.push(linia);

        let mut plant = Plant::new();
        plant.insert(zaklad);
        Mlyn {
            cat,
            tuning,
            store,
            plant,
            site,
            wejscie,
            zboze,
            maka,
        }
    }

    fn ctx(&self) -> ProductionCtx<'_> {
        ProductionCtx {
            cat: &self.cat,
            tuning: &self.tuning,
            world_seed: 7,
            deposits: &magnat_supply::NoDeposits,
        }
    }

    fn biegnij(&mut self, od: u64, minut: u32) -> magnat_supply::ProductionReport {
        let ctx = ProductionCtx {
            cat: &self.cat,
            tuning: &self.tuning,
            world_seed: 7,
            deposits: &magnat_supply::NoDeposits,
        };
        advance_production(
            &ctx,
            &mut self.store,
            &mut self.plant,
            self.site,
            SimMinute(od),
            minut,
        )
    }

    fn stan_linii(&self) -> LineState {
        self.plant.get(self.site).expect("zakład").lines[0].state
    }
}

/// Kryterium WP4, zdanie pierwsze: **linia bez prądu stoi.** Bez tego PRD §9.6 jest
/// tylko zdaniem w dokumencie.
#[test]
fn linia_bez_pradu_stoi() {
    let mut m = Mlyn::nowy(50_000);
    // Doba 0, 7:00 — zmiana dzienna trwa, młyn mieli.
    m.biegnij(7 * 60, 45);
    assert!(
        matches!(m.stan_linii(), LineState::Running { .. } | LineState::Idle),
        "{:?}",
        m.stan_linii()
    );
    let z_pradem = m.store.total_stock(m.maka);
    assert!(z_pradem.0 > 0, "przy prądzie mąka powstaje");

    // Odcięcie licznika.
    m.plant
        .get_mut(m.site)
        .expect("zakład")
        .meter_mut(UtilityService::Electricity)
        .expect("licznik prądu")
        .cut_off = true;

    m.biegnij(8 * 60, 240);
    assert!(
        matches!(
            m.stan_linii(),
            LineState::Broken {
                cause: BreakCause::NoPower,
                ..
            }
        ),
        "{:?}",
        m.stan_linii()
    );
    assert!(
        m.plant.get(m.site).expect("zakład").is_fault(),
        "odcięty licznik zapala SITE_FAULT, a nie samą ciszę"
    );

    // Prąd wraca — i młyn wraca do pracy sam, bez naprawy i bez części.
    m.plant
        .get_mut(m.site)
        .expect("zakład")
        .meter_mut(UtilityService::Electricity)
        .expect("licznik prądu")
        .cut_off = false;
    m.biegnij(13 * 60, 60);
    assert!(m.store.total_stock(m.maka).0 > z_pradem.0, "po powrocie prądu mieli dalej");
}

/// Kryterium WP4, zdanie drugie: **awaria wymaga części z magazynu.** To jest cały powód,
/// dla którego linia w ogóle ma pole `spare_part` — bez części awaria byłaby licznikiem
/// czasu, a nie problemem zaopatrzeniowym.
#[test]
fn awaria_wymaga_czesci_z_magazynu() {
    let mut m = Mlyn::nowy(50_000);
    let czesc = m.cat.good_id("part_bearing_6204").expect("łożysko");
    let site = m.site;

    // Awaria mechaniczna, magazyn bez części.
    m.plant.get_mut(site).expect("zakład").lines[0].state = LineState::Broken {
        since: SimMinute(0),
        cause: BreakCause::Wear,
    };
    m.biegnij(7 * 60, 600);
    assert!(
        matches!(m.stan_linii(), LineState::Broken { .. }),
        "bez części linia stoi, choćby doba minęła: {:?}",
        m.stan_linii()
    );

    // Część przyjeżdża na zaplecze.
    let masa_sztuki = m.cat.mass_of(czesc, magnat_core::Qty(1_000));
    m.store
        .put(
            &m.cat,
            m.wejscie,
            BatchDraft {
                good: czesc,
                mass: Mass(masa_sztuki.0 * 4),
                quality: Q::new(70),
                brand: None,
                producer: FirmId(encja(2)),
                produced_at: SimMinute(0),
                cost: Money(4_000),
                origin: Default::default(),
                flags: Default::default(),
            },
            MassIn::Imported,
        )
        .expect("część do magazynu");

    m.biegnij(17 * 60, 1);
    assert!(
        matches!(m.stan_linii(), LineState::Maintenance { .. }),
        "z częścią naprawa rusza: {:?}",
        m.stan_linii()
    );
    assert_eq!(
        m.store.total_stock(czesc),
        Mass(masa_sztuki.0 * 3),
        "naprawa zjadła dokładnie jedną sztukę"
    );
    m.store.check_mass(czesc).expect("bilans części");
}

/// Kryterium WP4, zdanie trzecie: **przezbrojenie kosztuje czas i masę.** Masa schodzi
/// jako `LossKind::Setup` — z kategorią, bo masa znikająca bez kategorii jest błędem
/// testu, nie zaokrągleniem (§7.3 pkt 1).
///
/// Para receptur jest prawdziwa i wzięta z katalogu fali A: paszarnia i przemiał
/// pszenicy chodzą na tej samej klasie maszyny (`mill_roller`), a przezbrojenie na
/// przemiał kosztuje 50 minut i 30 kg — dokładnie liczby z §7.1 dokumentu fazy.
#[test]
fn przezbrojenie_kosztuje_czas_i_mase() {
    let mut m = Mlyn::nowy(50_000);
    let site = m.site;
    let pasza = m.cat.recipe_id("feed_mill").expect("feed_mill");
    let przemial = m
        .cat
        .recipe_id("milling_wheat_t550")
        .expect("milling_wheat_t550");
    let zboze_przed = m.store.total_stock(m.zboze);

    // Linia stoi przezbrojona na paszę; plan każe przemielić pszenicę.
    m.plant.get_mut(site).expect("zakład").lines[0].recipe = Some(pasza);
    m.plant
        .get_mut(site)
        .expect("zakład")
        .schedule
        .plan
        .push_back(magnat_supply::PlannedRun {
            recipe: przemial,
            remaining: Mass(1_000_000),
            earliest_start: SimMinute(0),
            priority: 0,
        });

    m.biegnij(7 * 60, 1);
    let LineState::Setup { until, to_recipe } = m.stan_linii() else {
        panic!("linia powinna wejść w przezbrojenie: {:?}", m.stan_linii());
    };
    assert_eq!(to_recipe, przemial);
    assert_eq!(until, SimMinute(7 * 60 + 50), "50 minut z §7.1");

    let stracone = m.store.losses(m.zboze, LossKind::Setup);
    assert_eq!(stracone, Mass(30_000), "złom przezbrojenia z §7.1: 30 kg");
    assert_eq!(
        m.store.total_stock(m.zboze).0,
        zboze_przed.0 - 30_000,
        "złom schodzi z magazynu, a nie powstaje z niczego"
    );
    assert_eq!(
        m.plant.get(site).expect("zakład").losses[LossKind::Setup.as_index()],
        30_000,
        "histogram strat zakładu widzi to samo co magazyn"
    );
    m.store.check_mass(m.zboze).expect("bilans zboża");
    m.store.check_cost().expect("bilans pieniądza");

    // Przezbrojenie **kosztuje czas**: przez te pięćdziesiąt minut nic nie powstaje.
    m.biegnij(7 * 60 + 1, 49);
    assert_eq!(m.store.total_stock(m.maka), Mass::ZERO);
    m.biegnij(7 * 60 + 50, 45);
    assert!(
        m.store.total_stock(m.maka).0 > 0,
        "po przezbrojeniu mąka wreszcie powstaje"
    );
}

/// Kryterium WP4, zdanie czwarte i najważniejsze: **spójność LOD z tolerancją 0.**
///
/// Mikro i mezo różnią się **wyłącznie ziarnistością wywołania** (§7.6). Sześćdziesiąt
/// kroków po minucie musi dać co do grama i co do grosza to samo, co jeden krok po
/// godzinie — razem z ciągiem losowań awarii, bo strumień RNG jest kluczowany absolutną
/// minutą, a nie licznikiem wywołań.
#[test]
fn spojnosc_lod_ma_tolerancje_zero() {
    let doba = 1_440u32;
    let mut mikro = Mlyn::nowy(200_000);
    let mut mezo = Mlyn::nowy(200_000);

    let mut r_mikro = magnat_supply::ProductionReport::default();
    for m in 0..doba {
        let cz = mikro.biegnij(u64::from(m), 1);
        r_mikro.batches += cz.batches;
        r_mikro.produced = Mass(r_mikro.produced.0 + cz.produced.0);
        r_mikro.cost = Money(r_mikro.cost.0 + cz.cost.0);
        r_mikro.breakdowns += cz.breakdowns;
    }
    let mut r_mezo = magnat_supply::ProductionReport::default();
    for h in 0..24u64 {
        let cz = mezo.biegnij(h * 60, 60);
        r_mezo.batches += cz.batches;
        r_mezo.produced = Mass(r_mezo.produced.0 + cz.produced.0);
        r_mezo.cost = Money(r_mezo.cost.0 + cz.cost.0);
        r_mezo.breakdowns += cz.breakdowns;
    }

    assert!(r_mikro.batches > 0, "doba młyna to więcej niż zero szarż");
    assert_eq!(r_mikro.batches, r_mezo.batches, "liczba szarż");
    assert_eq!(r_mikro.produced, r_mezo.produced, "masa co do grama");
    assert_eq!(r_mikro.cost, r_mezo.cost, "koszt co do grosza");
    assert_eq!(r_mikro.breakdowns, r_mezo.breakdowns, "ciąg awarii");
    assert_eq!(
        mikro.store.total_stock(mikro.maka),
        mezo.store.total_stock(mezo.maka),
        "zapas mąki"
    );
    assert_eq!(
        mikro
            .plant
            .get(mikro.site)
            .expect("zakład")
            .meter(UtilityService::Electricity)
            .expect("licznik")
            .consumed,
        mezo.plant
            .get(mezo.site)
            .expect("zakład")
            .meter(UtilityService::Electricity)
            .expect("licznik")
            .consumed,
        "licznik prądu co do watominuty"
    );
}

/// Produkcja nie tworzy ani nie niszczy masy bez kategorii, a koszt wytworzenia
/// domyka się z kosztem partii. To jest niezmiennik z 00 §6, którego cztery testy
/// kryterium same nie sprawdzą.
#[test]
fn produkcja_domyka_bilans_masy_i_pieniadza() {
    let mut m = Mlyn::nowy(100_000);
    m.biegnij(6 * 60, 480);
    let otreby = m.cat.good_id("feed_bran").expect("feed_bran");
    let odpad = m
        .cat
        .good_id("waste_grain_screenings")
        .expect("waste_grain_screenings");

    for g in [m.zboze, m.maka, otreby, odpad] {
        m.store
            .check_mass(g)
            .unwrap_or_else(|(l, p)| panic!("bilans {}: {l} wobec {p}", m.cat.key_of(g)));
    }
    m.store.check_cost().expect("bilans pieniądza");
    m.store.check_no_negative().expect("niezmienniki slotów");

    // Mąka, otręby i odpad wychodzą w proporcji 760 : 220 : 20 — to jest bilans masy
    // receptury odtworzony w biegu, a nie sprawdzony statycznie przy ładowaniu.
    let (f, b, w) = (
        m.store.total_stock(m.maka).0,
        m.store.total_stock(otreby).0,
        m.store.total_stock(odpad).0,
    );
    assert!(f > 0 && b > 0 && w > 0, "{f} / {b} / {w}");
    assert_eq!(f * 220, b * 760, "mąka do otrąb");
    assert_eq!(f * 20, w * 760, "mąka do odsiewu");

    // Koszt wyrobu jest **wyższy** od kosztu wsadu: doszła praca, prąd i narzut.
    let ctx = m.ctx();
    assert!(ctx.tuning.plant.overhead_gr_per_batch > 0);
}

/// Przegląd planowy zdarza się **raz na okres**, a nie w każdej minucie po jego końcu.
///
/// Termin przeglądu jest stanem linii, nie harmonogramu zakładu, i ten test pilnuje
/// dlaczego: przy terminie wspólnym dla zakładu linia, która kończyła przegląd, wracała
/// do niego w tej samej minucie — o ile jakakolwiek inna linia akurat produkowała,
/// bo to ona blokowała przesunięcie terminu. Zakład stał wtedy w konserwacji na zawsze.
#[test]
fn przeglad_zdarza_sie_raz_na_okres_a_nie_w_kolko() {
    let mut m = Mlyn::nowy(200_000);
    let site = m.site;
    {
        let z = m.plant.get_mut(site).expect("zakład");
        z.schedule.maintenance_interval_hours = 4;
        z.lines[0].next_maintenance = SimMinute(7 * 60);
        z.lines[0].condition = Q::new(40);
        // Druga linia, która produkuje bez przerwy — to ona wywracała poprzedni model.
        let mut druga = z.lines[0];
        druga.condition = Q::MAX;
        druga.next_maintenance = SimMinute(u64::MAX);
        z.lines.push(druga);
    }

    // Liczymy **wejścia** w przegląd, nie minuty w nim: drugi przegląd zaczyna się
    // przed końcem okna i minuty by go ucięły.
    let mut wejsc = 0;
    let mut poprzedni = m.stan_linii();
    for minuta in 7 * 60..(7 * 60 + 600u64) {
        m.biegnij(minuta, 1);
        let teraz = m.stan_linii();
        if matches!(teraz, LineState::Maintenance { .. })
            && !matches!(poprzedni, LineState::Maintenance { .. })
        {
            wejsc += 1;
        }
        poprzedni = teraz;
    }

    let tuning = &m.tuning.plant;
    // Dokładnie dwa: jeden o 7:00, drugi cztery godziny po jego **zakończeniu**.
    // Okres liczy się od końca przeglądu, a nie od jego początku — inaczej długi
    // przegląd zjadałby własny okres i maszyna wracałaby na warsztat od razu.
    assert_eq!(wejsc, 2, "dwa przeglądy, a nie ciągły postój");
    assert_eq!(
        m.plant.get(site).expect("zakład").lines[0].condition,
        Q::new(tuning.condition_after_service),
        "maszyna po przeglądzie nie jest nowa, ale jest sprawna"
    );
}

/// Kryterium M7a WP3, zdanie pierwsze: **zakład z załogą wytwarza wynik proporcjonalny
/// do pracy**, którą w niego włożono.
///
/// Do M7a pole `labor_pct` nie istniało i młyn mielił tyle samo z pełną obsadą, z połową
/// i bez nikogo — praca nie była wejściem produkcji, tylko kosztem obok niej. Teraz jest
/// drugim ogranicznikiem szarży, obok wsadu.
///
/// Wartość liczy `magnat_firms::Site::labor_pct` z formy i umiejętności konkretnych
/// ludzi; M6 dostaje gotową liczbę i nie zagląda do środka (M7 §8, ryzyko `R8`).
#[test]
fn obsada_zakladu_jest_drugim_ogranicznikiem_szarzy() {
    let zmiel = |labor: u16| -> i64 {
        let mut m = Mlyn::nowy(50_000);
        m.plant
            .get_mut(m.site)
            .expect("zakład")
            .labor_pct = labor;
        // Osiem godzin zmiany dziennej, ta sama doba, ten sam wsad.
        m.biegnij(8 * 60, 480).produced.0
    };

    let pelna = zmiel(PlantSite::FULL_LABOR);
    let polowa = zmiel(500);
    let pusty = zmiel(0);

    assert!(pelna > 0, "młyn z pełną obsadą ma mleć");
    assert_eq!(pusty, 0, "młyn bez ludzi ma stać, a nie mleć sam");
    // Proporcjonalność z dokładnością do ziarnistości szarży: linia mieli całymi
    // szarżami, więc połowa pracy nie daje co do grama połowy mąki.
    let stosunek = polowa * 1000 / pelna;
    assert!(
        (440..=560).contains(&stosunek),
        "połowa obsady dała {stosunek}‰ produkcji zamiast około 500‰"
    );
}
