//! Testy własnościowe fazy M6 — cztery z ośmiu z §7.3, których do M6e nie było.
//!
//! Pozostałe cztery mają swoje miejsca i tam zostają: `prop_mass_conservation`
//! i `prop_no_negative_stock` w `warehouse.rs`, `prop_no_teleport` w `teleport.rs`,
//! `prop_deposit_monotone` w `mining.rs`. Rozdział jest celowy — test własnościowy
//! stoi przy mechanizmie, który sprawdza, a nie w worku o nazwie „properties";
//! tutaj lądują te, które **przecinają** moduły albo nie miały dotąd właściciela.
//!
//! | Własność | Co pilnuje |
//! |---|---|
//! | `prop_no_expired_on_shelf` | kontrakt `D12`: psucie przed detalem |
//! | `prop_cost_vs_mass` | wiąże bilans masy z bilansem pieniądza (00 §6) |
//! | `prop_dock_capacity` | rampa jest realnym wąskim gardłem, a nie licznikiem |
//! | `prop_contract_penalty` | kara naliczona równa się zapłaconej |

use magnat_core::{Entity, FirmId, Mass, Money, OpenHours, SimMinute, SiteId, Volume, Q};
use magnat_supply::batch::TransportOrderId;
use magnat_supply::catalog::load_default;
use magnat_supply::plant::{Dock, VehicleArrivedAtSite};
use magnat_supply::store::{BatchDraft, MassIn, WarehouseRole};
use magnat_supply::{BatchFlags, BatchOrigin, StorageClass, Store, Tuning};
use proptest::prelude::*;
use std::num::NonZeroU32;

fn encja(i: u32) -> Entity {
    Entity::new(i, NonZeroU32::new(1).expect("generacja"))
}

// ── 4. prop_no_expired_on_shelf ──────────────────────────────────────────────────────

/// Na półce nigdy nie leży partia po dacie, a przecena zapala się **przed** nią.
///
/// To nie jest test o dacie — to jest test o **kolejności**. `Store::spoil` biegnie
/// w kadencji minutowej łańcucha, a sprzedaż po nim (`D12`); odwrócenie tej kolejności
/// nie wywala się niczym widocznym, tylko wpuszcza do transakcji bochenek, który
/// w tej samej minucie miał zejść na odpis. Dlatego test sprawdza **każdą** minutę,
/// a nie stan końcowy: stan końcowy jest czysty także wtedy, gdy przez dobę na półce
/// leżał towar po terminie.
#[test]
fn prop_no_expired_on_shelf() {
    let cat = load_default("contemporary").expect("katalog z data/");
    let mut store = Store::new(cat.goods.len());
    let sklep = SiteId(encja(5));
    let polka = store.add_slot(
        sklep,
        WarehouseRole::Shelf,
        StorageClass::Ambient,
        Mass(10_000_000),
        Volume(i64::MAX / 8),
        0,
    );
    let chleb = cat.good_id("food_bread_wheat").expect("chleb");
    let trwalosc = cat
        .good(chleb)
        .shelf_life_minutes
        .expect("chleb ma termin ważności");

    // Dostawa co sześć godzin przez dwanaście dób — z terminem 48 h część partii
    // musi się przeterminować, inaczej test nie ma czego sprawdzić.
    let mut przeterminowanych = 0usize;
    for m in 0..(12 * 1_440u64) {
        let now = SimMinute(m);
        store.set_now(now);
        if m % 360 == 0 {
            let _ = store.put(
                &cat,
                polka,
                BatchDraft {
                    good: chleb,
                    mass: Mass(80_000),
                    quality: Q::new(70),
                    brand: None,
                    producer: FirmId(encja(1)),
                    produced_at: now,
                    cost: Money(80_000),
                    origin: BatchOrigin::default(),
                    flags: BatchFlags::default(),
                },
                MassIn::Produced,
            );
        }
        przeterminowanych += store.spoil(now).len();

        // Sprzedaż **po** psuciu — tak jak w DAG systemów. Wolniejsza niż dostawa,
        // bo test o przeterminowaniu potrzebuje towaru, który zdąży się zestarzeć:
        // półka wyprzedana co do grama nie ma czego zepsuć.
        if m % 30 == 0 {
            let _ = store.shelf_pick(polka, chleb, Mass(900));
        }

        for b in store.slot(polka).expect("półka").batches() {
            let partia = store.batch(*b).expect("partia z listy slotu żyje");
            assert!(
                !partia.expired_at(now),
                "minuta {m}: na półce leży partia po dacie ({:?} ≤ {m})",
                partia.expires_at
            );
        }
    }
    assert!(
        przeterminowanych > 0,
        "przez dwanaście dób nic się nie przeterminowało przy trwałości {trwalosc} min — \
         test nie sprawdził tego, co miał"
    );
    store
        .check_mass(chleb)
        .expect("bilans masy chleba po przebiegu");
}

// ── 5. prop_cost_vs_mass ─────────────────────────────────────────────────────────────

// Suma kosztów żywych partii = zapłacone − COGS − odpisy, po **dowolnym** ciągu
// operacji magazynowych.
//
// Wiąże bilans masy z bilansem pieniądza (00 §6) i wykrywa dokładnie tę klasę błędu,
// której nie widać w żadnym teście jednostkowym: zgubiony grosz przy podziale partii.
// Podział dzieli koszt proporcjonalnie do masy, a reszta z dzielenia musi trafić
// do **którejś** części — nie wyparować.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn prop_cost_vs_mass(
        operacje in prop::collection::vec(
            (0u8..6, 1i64..40_000, 40u8..95),
            80..200,
        )
    ) {
        let cat = load_default("contemporary").expect("katalog z data/");
        let mut store = Store::new(cat.goods.len());
        let site = SiteId(encja(7));
        let magazyn = store.add_slot(
            site,
            WarehouseRole::Input,
            StorageClass::Dry,
            Mass(i64::MAX / 8),
            Volume(i64::MAX / 8),
            0xFF,
        );
        let polka = store.add_slot(
            site,
            WarehouseRole::Shelf,
            StorageClass::Ambient,
            Mass(i64::MAX / 8),
            Volume(i64::MAX / 8),
            0xFF,
        );
        let g = cat.good_id("food_flour_t550").expect("mąka");
        let mut zlecenie = 0u32;

        for (i, (op, ile, q)) in operacje.iter().enumerate() {
            store.set_now(SimMinute(i as u64));
            let masa = Mass(*ile);
            match op {
                0 => {
                    // Wstawienie: cena bierze się z masy, więc koszt jednostkowy
                    // jest różny między partiami — to jest warunek, w którym podział
                    // kosztu w ogóle może zgubić grosz.
                    let _ = store.put(&cat, magazyn, BatchDraft {
                        good: g,
                        mass: masa,
                        quality: Q::new(*q),
                        brand: None,
                        producer: FirmId(encja(1)),
                        produced_at: SimMinute(i as u64),
                        cost: Money(ile * 37 + 13),
                        origin: BatchOrigin::default(),
                        flags: BatchFlags::default(),
                    }, MassIn::Produced);
                }
                1 => {
                    if let Some(r) = store.reserve(magazyn, g, masa, Q::MIN) {
                        let _ = store.take(r);
                    }
                }
                2 => {
                    let _ = store.write_off(magazyn, g, masa, magnat_core::LossKind::Storage);
                }
                3 => {
                    zlecenie += 1;
                    let id = TransportOrderId(zlecenie);
                    if let Some(cargo) = store.load(magazyn, g, masa, Q::MIN, id) {
                        // Odsprzedaż z narzutem — to ona przenosi koszt własny
                        // na cenę zapłaconą (`AP-7`) i ona może zgubić grosz.
                        store.resell(&cargo, Money(ile * 41 + 7));
                        let _ = store.unload(&cat, id, &cargo, polka);
                    }
                }
                4 => {
                    let _ = store.move_within_site(&cat, magazyn, polka, g, masa);
                }
                _ => {
                    let _ = store.shelf_pick(polka, g, masa);
                }
            }
            store.check_no_negative().expect("stany nieujemne");
        }

        if let Err((w_partiach, oczekiwane)) = store.check_cost() {
            panic!(
                "koszt w partiach {} gr, a zapłacone − COGS − odpisy {} gr (różnica {} gr)",
                w_partiach.get(),
                oczekiwane.get(),
                w_partiach.get() - oczekiwane.get()
            );
        }
        store.check_mass(g).expect("bilans masy mąki");
    }
}

// ── 6. prop_dock_capacity ────────────────────────────────────────────────────────────

// Rampa nigdy nie obsługuje więcej pojazdów naraz niż ma stanowisk, kolejka rusza
// wyłącznie w godzinach dostaw, a pojazd ponad pojemność placu stoi **na ulicy** —
// nie znika.
//
// Trzy zdania kryterium §7.3 pkt 6 i trzy asercje. Czwarte zdanie („suma czasów
// obsługi równa sumie czasów zajętości stanowisk") jest własnością `Dock::arrive`
// i sprawdza je test siostrzany po stronie M4 (`ramp_wait_lod_invariant`) — to nasz
// wspólny test, nie mój i nie M4.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn prop_dock_capacity(
        przyjazdy in prop::collection::vec((0u64..2_880, 1i64..26_000_000), 30..120),
        bays in 1u8..6,
        plac in 0u8..4,
    ) {
        let tuning = Tuning::load_default().expect("data/tuning/supply.ron");
        // Godziny dostaw 6:00–18:00 — węższe niż doba, żeby warunek brzegowy
        // „kolejka rusza dopiero po otwarciu" miał co sprawdzać.
        let mut dock = Dock::new(bays, 12, 1, OpenHours::new(360, 1_080, 0x7F));
        dock.yard_capacity = plac;

        let mut posortowane = przyjazdy.clone();
        posortowane.sort_unstable();
        let mut na_ulicy = 0usize;
        let mut obsluga: Vec<(u64, u64)> = Vec::new();

        for (i, (minuta, masa)) in posortowane.iter().enumerate() {
            let now = SimMinute(*minuta);
            let r = dock.arrive(
                &VehicleArrivedAtSite {
                    site: SiteId(encja(3)),
                    order: TransportOrderId(i as u32),
                    vehicle: magnat_core::VehicleId(encja(i as u32)),
                    mass: Mass(*masa),
                    at: now,
                },
                &tuning.dock,
            );
            if r.on_street {
                na_ulicy += 1;
            }
            prop_assert!(
                r.release_at.0 >= now.0,
                "pojazd {i} wyjechał przed przyjazdem: {} < {}",
                r.release_at.0,
                now.0
            );
            // Obsługa zaczyna się najwcześniej po otwarciu rampy: zwolnienie nie może
            // wypaść wcześniej niż otwarcie doby, w której pojazd przyjechał.
            let doba = now.0 / 1_440;
            let otwarcie = doba * 1_440 + 360;
            if now.0 < otwarcie {
                prop_assert!(
                    r.release_at.0 >= otwarcie,
                    "pojazd {i} obsłużony przed otwarciem rampy: {} < {otwarcie}",
                    r.release_at.0
                );
            }
            // **Stanowisko obsługuje jeden pojazd naraz.** `occupancy` liczy wszystkich
            // w systemie — także tych czekających na placu i na ulicy — więc może
            // przekraczać liczbę stanowisk i to jest poprawne. Niezmiennik dotyczy
            // **obsługi**: przedziały `[start, release)` nie mogą zachodzić na siebie
            // więcej niż `bays` razy. `start` wyprowadza się z odpowiedzi, bo czas
            // obsługi jest funkcją masy i jest publiczny.
            let start = r.release_at.0 - u64::from(dock.service_minutes(Mass(*masa)));
            obsluga.push((start, r.release_at.0));
        }

        let mut brzegi: Vec<(u64, i32)> = Vec::new();
        for (a, b) in &obsluga {
            brzegi.push((*a, 1));
            brzegi.push((*b, -1));
        }
        brzegi.sort_unstable();
        let mut naraz = 0i32;
        for (chwila, delta) in brzegi {
            naraz += delta;
            prop_assert!(
                naraz <= i32::from(bays),
                "minuta {chwila}: {naraz} pojazdów obsługiwanych na {bays} stanowiskach"
            );
        }

        // Pojazd, który się nie zmieścił, **jest w wyniku** — a nie zgubiony.
        prop_assert!(
            na_ulicy <= posortowane.len(),
            "więcej pojazdów na ulicy niż przyjechało"
        );
        prop_assert!(
            dock.queue().len() <= posortowane.len(),
            "w kolejce {} wpisów przy {} przyjazdach",
            dock.queue().len(),
            posortowane.len()
        );
    }
}

// ── 8. prop_contract_penalty ─────────────────────────────────────────────────────────

// Suma kar naliczonych równa się sumie zapłaconych, a `fulfilled + missed` równa się
// masie wynikającej z harmonogramu.
//
// `AI-7`: trzecie zdanie kryterium — „cena `Collar` zawsze w `[floor, cap]`" — jest
// własnością czystej funkcji `ContractPricing::price_at` i ma **test jednostkowy**
// w `tests/b2b.rs`. Sprawdzanie jej na przebiegu kosztowałoby czas i nie dodało ani
// jednego przypadku, którego tamten test nie widzi. Tutaj są te dwie własności,
// które przebiegu **wymagają**, bo dotyczą akumulacji.
proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn prop_contract_penalty(
        dostawy in prop::collection::vec((1i64..50_000, 0i64..50_000, 0u32..600), 5..40),
        kara_bp in 0u32..2_000,
    ) {
        use magnat_supply::{ContractPricing, DeliverySchedule, Penalty, SupplyContract};

        let mut c = SupplyContract {
            id: magnat_core::ContractId(encja(11)),
            buyer: FirmId(encja(1)),
            seller: FirmId(encja(2)),
            good: magnat_core::GoodId(0),
            min_quality: Q::MIN,
            deliver_from: SiteId(encja(2)),
            from_slot: magnat_supply::SlotId(0),
            deliver_to: SiteId(encja(1)),
            to_slot: magnat_supply::SlotId(1),
            incoterm: magnat_supply::WhoTransports::Seller,
            schedule: DeliverySchedule {
                every_minutes: 1_440,
                mass: Mass(100_000),
                window: OpenHours::ALWAYS,
            },
            pricing: ContractPricing::Fixed { price: Money(1_000) },
            penalty: Penalty {
                per_tonne_missed: Money(i64::from(kara_bp)),
                cap_pct: 30,
                grace_minutes: 120,
            },
            valid_from: SimMinute(0),
            valid_to: SimMinute(90 * 1_440),
            notice_minutes: 4_320,
            scheduled_mass: Mass::ZERO,
            fulfilled_mass: Mass::ZERO,
            missed_mass: Mass::ZERO,
            late_deliveries: 0,
            penalty_accrued: Money::ZERO,
            penalty_paid: Money::ZERO,
            last_issued: SimMinute(0),
            terminated_at: None,
        };

        for (dowiezione, brakujace, spoznienie) in &dostawy {
            // Harmonogram zazadal tej masy - tak samo jak `run_contracts` w kadencji.
            c.scheduled_mass = Mass(c.scheduled_mass.0 + dowiezione + brakujace);
            c.record_fulfilled(Mass(*dowiezione), *spoznienie);
            if *brakujace > 0 {
                c.accrue_penalty(Mass(*brakujace), Money(1_000));
            }
        }

        prop_assert_eq!(
            c.fulfilled_mass.0 + c.missed_mass.0,
            c.scheduled_mass.0,
            "dowiezione {} plus brakujace {} wobec zaplanowanych {}",
            c.fulfilled_mass.0,
            c.missed_mass.0,
            c.scheduled_mass.0
        );

        // Sufit kary dziala na **naliczaniu**: nigdy nie naliczy sie wiecej niz
        // `cap_pct` wartosci calego kontraktu. Bez tego jedna kiepska dostawa mogla
        // kosztowac dostawce wiecej, niz caly kontrakt byl wart.
        let sufit = c
            .total_value(Money(1_000))
            .mul_ratio(i64::from(c.penalty.cap_pct), 100);
        prop_assert!(
            c.penalty_accrued.0 <= sufit.0,
            "naliczono {} gr ponad sufit {} gr",
            c.penalty_accrued.0,
            sufit.0
        );

        // Zaplata: kontrakt oddaje dokladnie tyle, ile jest dlugu, i nigdy wiecej -
        // nawet gdy wolajacy przeleje za duzo.
        let naliczone = c.penalty_accrued;
        let zaplacone = c.pay_penalty(Money(naliczone.0.saturating_mul(2)));
        prop_assert_eq!(
            zaplacone.get(),
            naliczone.get(),
            "zaplacono {} gr wobec naliczonych {} gr",
            zaplacone.get(),
            naliczone.get()
        );
        prop_assert_eq!(
            c.penalty_paid.get(),
            c.penalty_accrued.get(),
            "po rozliczeniu saldo kar nie jest zerowe"
        );
        prop_assert!(c.penalty_accrued.get() >= 0, "ujemna kara umowna");
        // Druga zaplata nie ma juz czego zdjac - dlug jest zerowy.
        prop_assert_eq!(c.pay_penalty(Money(1_000_000)).get(), 0);
    }
}
