//! Łańcuchy referencyjne A i B — kryterium ukończenia WP13 i WP14 (M6 §7.1, §7.2).
//!
//! **Po co to istnieje.** Do M6d każdy kawałek łańcucha miał własny test i każdy
//! przechodził: magazyn oddzielnie, zakład oddzielnie, rynek oddzielnie. Czego żaden
//! z nich nie sprawdzał, to czy **złożone razem** dowożą bochenek chleba na półkę
//! i litr diesla do baku — a to jest obietnica §1 dokumentu fazy, nie sumy jego
//! pakietów.
//!
//! Dwa łańcuchy, dwa kryteria:
//! - chleb: pole → elewator → młyn → piekarnia → sklep, ślad **≥ 5 etapów**
//!   z czasem, masą, jakością i kosztem narastającym;
//! - diesel: złoże → szyb → rafineria → terminal → stacja, ślad **≥ 6 etapów**
//!   kończących się `DepositId`.
//!
//! Do tego bilans masy każdego towaru, który przez łańcuch przeszedł, **co do grama**,
//! i bilans pieniądza magazynu co do grosza.
//!
//! **Czego tu nie ma i dlaczego.** Rynek B2B, kaskada niedoboru i wybór dostawcy mają
//! własne testy (`tests/b2b.rs`, `tests/shortage.rs`). Tutaj towar jedzie zleceniem
//! wystawionym wprost, bo pytanie brzmi „czy masa i koszt przeżywają całą drogę",
//! a nie „czy rynek wybiera dobrze". Zmieszanie obu dałoby test, który pęka z sześciu
//! powodów naraz i nie mówi, z którego.

use magnat_core::{
    DepositId, Energy, Entity, FirmId, GoodId, HashState, Mass, Money, OpenHours, SimMinute,
    SiteId, StateHasher, UtilityService, Volume, Q,
};
use magnat_supply::batch::TraceKind;
use magnat_supply::catalog::load_default;
use magnat_supply::plant::{Dock, PlantSite, ProductionCtx};
use magnat_supply::store::{BatchDraft, MassIn, WarehouseRole};
use magnat_supply::transport::{TransportRequest, VehicleRequirements};
use magnat_supply::{
    advance_production, BatchFlags, BatchId, BatchOrigin, Catalog, Deposits, FlatRateFreight,
    MiningSite, ProductionLine, SlotId, StorageClass, Store, Transport, Tuning,
};
use std::cell::Cell;
use std::num::NonZeroU32;

fn encja(i: u32) -> Entity {
    Entity::new(i, NonZeroU32::new(1).expect("generacja"))
}

/// Złoże ropy z §7.2: 2,4 mln ton przy 82 % koncentracji.
struct Zloze {
    reserves: Mass,
    wydobyte: Cell<i64>,
}

impl HashState for Zloze {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_i64(self.wydobyte.get());
    }
}

impl Deposits for Zloze {
    fn remaining(&self, _id: DepositId) -> Mass {
        Mass(self.reserves.0 - self.wydobyte.get())
    }
    fn initial(&self, _id: DepositId) -> Mass {
        self.reserves
    }
    fn extract(&self, id: DepositId, want: Mass) -> Mass {
        let got = want.0.clamp(0, self.remaining(id).0);
        self.wydobyte.set(self.wydobyte.get() + got);
        Mass(got)
    }
}

/// Miasto złożone z kilku zakładów i jednego magazynu — tyle, ile trzeba, żeby towar
/// miał gdzie stanąć i czym pojechać.
struct Lancuch {
    cat: Catalog,
    tuning: Tuning,
    store: Store,
    plant: magnat_supply::Plant,
    transport: Transport,
    oracle: FlatRateFreight,
    nastepny_site: u32,
}

impl Lancuch {
    fn nowy() -> Lancuch {
        let cat = load_default("contemporary").expect("katalog z data/");
        let tuning = Tuning::load_default().expect("data/tuning/supply.ron");
        let goods = cat.goods.len();
        Lancuch {
            oracle: FlatRateFreight {
                km: 12,
                tuning: tuning.transport,
                blocked: Vec::new(),
            },
            cat,
            tuning,
            store: Store::new(goods),
            plant: magnat_supply::Plant::new(),
            transport: Transport::new(),
            nastepny_site: 1,
        }
    }

    /// Zakład z jedną linią na podanej recepturze, magazynem wejściowym i wyjściowym.
    fn zaklad(&mut self, recipe: &str, przepustowosc_g_h: i64) -> (SiteId, SlotId, SlotId) {
        let site = SiteId(encja(self.nastepny_site));
        self.nastepny_site += 1;
        let owner = FirmId(encja(1_000 + site.entity().index()));
        let rid = self.cat.recipe_id(recipe).expect(recipe);
        let rec = self.cat.recipe(rid).clone();

        let we = self.store.add_slot(
            site,
            WarehouseRole::Input,
            StorageClass::Dry,
            Mass(i64::MAX / 8),
            Volume(i64::MAX / 8),
            0xFF,
        );
        let wy = self.store.add_slot(
            site,
            WarehouseRole::Output,
            StorageClass::Dry,
            Mass(i64::MAX / 8),
            Volume(i64::MAX / 8),
            0xFF,
        );

        let mut z = PlantSite::new(site, owner, Dock::new(4, 10, 1, OpenHours::ALWAYS));
        z.inputs.push(we);
        z.outputs.push(wy);
        z.meters.push(magnat_supply::UtilityMeter::new(
            UtilityService::Electricity,
            FirmId(encja(9_999)),
            Money(self.tuning.utility.power_gr_per_kwh),
        ));
        z.meters.push(magnat_supply::UtilityMeter::new(
            UtilityService::Water,
            FirmId(encja(9_999)),
            Money(self.tuning.utility.water_gr_per_m3),
        ));
        // Trzy zmiany, bo łańcuch referencyjny ma jechać dobę po dobie bez przerw —
        // pytanie testu dotyczy masy i kosztu, nie grafiku zmianowego.
        for i in 0..3 {
            z.schedule.shifts[i] = Some(magnat_supply::plant::Shift {
                from: (i as u16) * 480,
                to: ((i as u16) + 1) * 480 % 1440,
                headcount: 8,
                wage_multiplier_pct: 100,
                skill: Q::new(60),
            });
        }
        let klasa = self
            .cat
            .machine_class_of(&rec)
            .expect("klasa maszyny z receptury");
        let czesc = self
            .cat
            .good_id("part_bearing_6204")
            .expect("part_bearing_6204");
        let mut l = ProductionLine::new(
            klasa,
            Mass(przepustowosc_g_h),
            Energy(rec.energy.0.max(0)),
            czesc,
        );
        l.recipe = Some(rid);
        l.next_maintenance = SimMinute(u64::MAX);
        z.lines.push(l);
        self.plant.insert(z);
        (site, we, wy)
    }

    /// Magazyn bez produkcji: elewator, terminal, sklep.
    fn magazyn(&mut self, role: WarehouseRole) -> (SiteId, SlotId) {
        let site = SiteId(encja(self.nastepny_site));
        self.nastepny_site += 1;
        let owner = FirmId(encja(1_000 + site.entity().index()));
        let slot = self.store.add_slot(
            site,
            role,
            StorageClass::Dry,
            Mass(i64::MAX / 8),
            Volume(i64::MAX / 8),
            0xFF,
        );
        let mut z = PlantSite::new(site, owner, Dock::new(4, 10, 1, OpenHours::ALWAYS));
        z.inputs.push(slot);
        z.outputs.push(slot);
        self.plant.insert(z);
        (site, slot)
    }

    fn wsad(&mut self, slot: SlotId, key: &str, kg: i64, q: u8, traced: bool) -> GoodId {
        let g = self.cat.good_id(key).expect(key);
        let cena = self
            .cat
            .good(g)
            .external_base_price
            .map_or(100, magnat_core::Money::get);
        let mut flags = BatchFlags::default();
        if traced {
            flags.set(BatchFlags::TRACED);
        }
        self.store
            .put(
                &self.cat,
                slot,
                BatchDraft {
                    good: g,
                    mass: Mass(kg * 1_000),
                    quality: Q::new(q),
                    brand: None,
                    producer: FirmId(encja(1)),
                    produced_at: SimMinute(0),
                    cost: Money(cena * kg),
                    origin: BatchOrigin::default(),
                    flags,
                },
                MassIn::Initial,
            )
            .expect("wsad startowy");
        g
    }

    /// Przewóz towaru między zakładami — jedyna legalna droga zmiany lokacji
    /// (`prop_no_teleport`).
    #[allow(clippy::too_many_arguments)]
    fn przewiez(
        &mut self,
        from: SiteId,
        from_slot: SlotId,
        to: SiteId,
        to_slot: SlotId,
        good: GoodId,
        mass: Mass,
        now: SimMinute,
    ) -> bool {
        if self.store.available(from_slot, good, Q::MIN).0 < mass.0 || mass.0 <= 0 {
            return false;
        }
        let id = self.transport.order(
            &self.oracle,
            TransportRequest {
                from,
                to,
                from_slot,
                to_slot,
                good,
                mass,
                requires: VehicleRequirements::for_good(&self.cat, good, mass),
                ready_at: now,
                due_at: SimMinute(now.0 + 600),
            },
            magnat_core::DecisionReason::Unspecified,
        );
        self.transport
            .dispatch(
                &self.oracle,
                &mut self.store,
                id,
                magnat_supply::Carrier::Unassigned,
                now,
            )
            .is_ok()
    }

    /// Doprowadza wszystkie zlecenia, które dojechały.
    fn rozladuj(&mut self, now: SimMinute) {
        let gotowe: Vec<_> = self
            .transport
            .iter()
            .filter(|o| {
                matches!(o.state, magnat_supply::TransportOrderState::EnRoute { eta } if eta.0 <= now.0)
            })
            .map(|o| o.id)
            .collect();
        for id in gotowe {
            let _ = self.transport.deliver(&self.cat, &mut self.store, id, now);
        }
    }

    fn minuta(&mut self, now: SimMinute, deposits: &dyn Deposits) {
        self.store.set_now(now);
        let ctx = ProductionCtx {
            cat: &self.cat,
            tuning: &self.tuning,
            world_seed: 7,
            deposits,
        };
        let sites: Vec<SiteId> = self.plant.sites().collect();
        for s in &sites {
            advance_production(&ctx, &mut self.store, &mut self.plant, *s, now, 1);
        }
        self.rozladuj(now);
    }

    /// Pierwsza partia śledzona w slocie — punkt wejścia panelu „śledź partię".
    fn sledzona_w(&self, slot: SlotId, good: GoodId) -> Option<BatchId> {
        self.store.slot(slot)?.batches().iter().copied().find(|b| {
            self.store
                .batch(*b)
                .is_some_and(|x| x.good == good && x.flags.has(BatchFlags::TRACED))
        })
    }

    fn bilans(&self, klucze: &[&str]) {
        for k in klucze {
            let g = self.cat.good_id(k).expect(k);
            if let Err((lewa, prawa)) = self.store.check_mass(g) {
                panic!("bilans masy {k}: wejścia {lewa} g ≠ wyjścia {prawa} g");
            }
        }
        if let Err((w_partiach, oczekiwane)) = self.store.check_cost() {
            panic!(
                "bilans pieniądza: w partiach {} gr, oczekiwane {} gr",
                w_partiach.get(),
                oczekiwane.get()
            );
        }
        self.store.check_no_negative().expect("stany nieujemne");
    }
}

/// Etapy śladu niosą **komplet** danych z §14.4: czas, masę, jakość i koszt narastający.
fn etapy_sa_pelne(t: &magnat_supply::BatchTrace) {
    assert!(!t.stages.is_empty(), "ślad pusty");
    for (i, s) in t.stages.iter().enumerate() {
        assert!(s.mass.0 > 0, "etap {i}: masa {} ≤ 0", s.mass.0);
        assert!(
            s.cost_cumulative.get() >= 0,
            "etap {i}: ujemny koszt narastający"
        );
        assert!(s.quality.get() <= 100, "etap {i}: jakość poza skalą");
    }
    let czasy: Vec<u64> = t.stages.iter().map(|s| s.at.0).collect();
    let mut posortowane = czasy.clone();
    posortowane.sort_unstable();
    assert_eq!(czasy, posortowane, "etapy nie są w kolejności czasu");
}

/// **Łańcuch A — chleb.** Pole → elewator → młyn → piekarnia → sklep.
///
/// Kryterium WP13 dla chleba: `trace_batch` na bochenku z półki zwraca ≥ 5 etapów
/// z czasem, masą, jakością i kosztem. Kryterium WP14: bilans masy każdego towaru
/// domyka się co do grama.
#[test]
fn e2e_bread() {
    let mut l = Lancuch::nowy();

    // Pole: 1 t zboża na dobę przy skali bazowej (§7.1 etap 1).
    let (pole, pole_we, pole_wy) = l.zaklad("wheat_farming", 1_000_000 / 24);
    l.wsad(pole_we, "agri_fertilizer", 4_000, 70, false);
    // **Ziarno siewne jest śledzone** — i to ono zasiewa ślad całego łańcucha:
    // flaga dziedziczy się w dół przy każdym przetworzeniu (`AP-8`), więc bochenek
    // na półce jest prawnukiem tej partii.
    l.wsad(pole_we, "agri_seed_wheat", 4_000, 70, true);

    let (elewator, elewator_slot) = l.magazyn(WarehouseRole::Distribution);
    let (mlyn, mlyn_we, mlyn_wy) = l.zaklad("milling_wheat_t550", 3_000_000);
    let (piekarnia, piek_we, piek_wy) = l.zaklad("bakery_bread_wheat", 60_000);
    l.wsad(piek_we, "food_yeast", 40, 60, false);
    l.wsad(piek_we, "food_salt", 40, 60, false);
    let (sklep, polka) = l.magazyn(WarehouseRole::Shelf);

    let zboze = l.cat.good_id("raw_wheat").expect("raw_wheat");
    let maka = l.cat.good_id("food_flour_t550").expect("mąka");
    let chleb = l.cat.good_id("food_bread_wheat").expect("chleb");

    // Dwadzieścia dób: pole potrzebuje doby na szarżę, wypiek 190 minut, a towar
    // musi jeszcze przejechać trzy odcinki.
    for m in 0..(20 * 1_440u64) {
        let now = SimMinute(m);
        l.minuta(now, &magnat_supply::NoDeposits);
        if m % 240 != 0 {
            continue;
        }
        let ile = l.store.available(pole_wy, zboze, Q::MIN);
        l.przewiez(pole, pole_wy, elewator, elewator_slot, zboze, ile, now);
        let ile = l.store.available(elewator_slot, zboze, Q::MIN);
        l.przewiez(elewator, elewator_slot, mlyn, mlyn_we, zboze, ile, now);
        let ile = l.store.available(mlyn_wy, maka, Q::MIN);
        l.przewiez(mlyn, mlyn_wy, piekarnia, piek_we, maka, ile, now);
        let ile = l.store.available(piek_wy, chleb, Q::MIN);
        l.przewiez(piekarnia, piek_wy, sklep, polka, chleb, ile, now);
    }

    let na_polce = l.store.available(polka, chleb, Q::MIN);
    assert!(
        na_polce.0 > 0,
        "po dwudziestu dobach na półce nie ma ani grama chleba"
    );

    let bochenek = l
        .sledzona_w(polka, chleb)
        .expect("bochenek na półce ma być śledzony — ślad dziedziczy się w dół łańcucha");
    let t = magnat_supply::trace_batch(&l.store, bochenek);
    etapy_sa_pelne(&t);
    assert!(
        t.stages.len() >= 5,
        "kryterium WP13: ślad bochenka ma ≥ 5 etapów, jest {} ({:?})",
        t.stages.len(),
        t.stages.iter().map(|s| s.kind).collect::<Vec<_>>()
    );
    assert!(
        t.stages.iter().any(|s| s.kind == TraceKind::Produced),
        "ślad bez ani jednego etapu wytworzenia nie jest łańcuchem, tylko przewozem"
    );
    assert!(
        t.depth >= 3,
        "chleb jest o trzy przetworzenia od nawozu, a `depth` mówi {}",
        t.depth
    );

    l.bilans(&[
        "raw_wheat",
        "food_flour_t550",
        "food_bread_wheat",
        "feed_bran",
        "agri_fertilizer",
        "agri_seed_wheat",
        "waste_grain_screenings",
    ]);
}

/// **Łańcuch B — paliwo.** Złoże → szyb → rafineria → terminal → stacja.
///
/// Kryterium WP13 dla diesla: ślad ma ≥ 6 etapów i kończy się **konkretnym złożem**,
/// a nie liczbą, która przypadkiem wygląda jak identyfikator (`AL-3`, `K-39`).
#[test]
fn e2e_fuel() {
    let mut l = Lancuch::nowy();
    let zloze = Zloze {
        reserves: Mass(2_400_000 * 1_000_000),
        wydobyte: Cell::new(0),
    };

    // Szyb: 320 t/dobę z §7.2, czyli ~13,3 t/h.
    let (szyb, _szyb_we, szyb_wy) = l.zaklad("oil_well", 320_000_000 / 24);
    if let Some(p) = l.plant.get_mut(szyb) {
        p.mining = Some(MiningSite {
            deposit: DepositId(0),
            concentration_pct: 82,
            depth_m: 1_800,
        });
    }
    let (rafineria, raf_we, raf_wy) = l.zaklad("refinery_crude_fractionation", 12_000_000);
    let (terminal, terminal_slot) = l.magazyn(WarehouseRole::Distribution);
    let (stacja, zbiornik) = l.magazyn(WarehouseRole::Shelf);

    let ropa = l.cat.good_id("raw_crude_oil").expect("ropa");
    let diesel = l.cat.good_id("fuel_diesel_b7").expect("diesel");

    for m in 0..(12 * 1_440u64) {
        let now = SimMinute(m);
        l.minuta(now, &zloze);
        if m % 120 != 0 {
            continue;
        }
        // Gracz wskazuje partię ropy w panelu — stąd bierze się flaga `TRACED`.
        // Wydobycie nie ma wejść, więc nie ma po czym jej odziedziczyć, i to jest
        // właściwe: ślad zaczyna się tam, gdzie ktoś na niego patrzy (§6.4.2).
        if l.sledzona_w(szyb_wy, ropa).is_none() {
            if let Some(b) = l
                .store
                .slot(szyb_wy)
                .and_then(|s| s.batches().first().copied())
            {
                l.store.mark_traced(b);
            }
        }
        let ile = l.store.available(szyb_wy, ropa, Q::MIN);
        l.przewiez(szyb, szyb_wy, rafineria, raf_we, ropa, ile, now);
        let ile = l.store.available(raf_wy, diesel, Q::MIN);
        l.przewiez(rafineria, raf_wy, terminal, terminal_slot, diesel, ile, now);
        let ile = l.store.available(terminal_slot, diesel, Q::MIN);
        l.przewiez(terminal, terminal_slot, stacja, zbiornik, diesel, ile, now);
    }

    assert!(
        zloze.wydobyte.get() > 0,
        "szyb nie wydobył ani grama — kopalnia bez złoża stoi na `Starved`"
    );
    let w_zbiorniku = l.store.available(zbiornik, diesel, Q::MIN);
    assert!(w_zbiorniku.0 > 0, "w zbiorniku stacji nie ma diesla");

    let litr = l
        .sledzona_w(zbiornik, diesel)
        .expect("w zbiorniku nie ma partii śledzonej");
    let t = magnat_supply::trace_batch(&l.store, litr);
    etapy_sa_pelne(&t);
    assert!(
        t.stages.len() >= 6,
        "kryterium WP13: ślad litra diesla ma ≥ 6 etapów, jest {} ({:?})",
        t.stages.len(),
        t.stages.iter().map(|s| s.kind).collect::<Vec<_>>()
    );
    assert!(
        t.reaches_deposit(),
        "ślad paliwa ma się kończyć złożem, a kończy się {:?}",
        t.origin
    );

    l.bilans(&[
        "raw_crude_oil",
        "fuel_diesel_b7",
        "fuel_petrol_95",
        "fuel_lpg",
        "fuel_heating_oil",
        "mat_bitumen",
        "chem_lubricant_base",
        "waste_refinery_residue",
    ]);
}
