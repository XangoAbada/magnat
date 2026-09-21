//! **Złoty test karty podróży i karty pojazdu** — kryterium WP11 (M4d).
//!
//! Scena jest zdaniem testowym fazy M4 z §1: *„Anna wyjeżdża o 07:20…"*. Anna jedzie
//! do pracy własnym autem, bo komunikacja kosztuje ją więcej po zmonetyzowaniu czasu,
//! a rower odpada przy deszczu. Karta ma to powiedzieć **liczbami w groszach**, nie
//! wagami — dokładnie tak, jak żąda PRD §14.1 („dlaczego Anna nie kupiła u mnie?").
//!
//! Test biegnie bez GPU i bez świata: karta dostaje decyzję i rejestr przejazdu jako
//! dane, bo `TripLedger` nie jest komponentem ECS i po zakończeniu podróży nikt go
//! nie przechowuje.

use magnat_agents::Identity;
use magnat_core::{
    BuildingId, CitizenReason, DecisionReason, Entity, Money, PlaceRef, SimMinute, SiteId, Weather,
};
use magnat_nav::EdgeId;
use magnat_traffic::{
    evaluate_modes, FuelTank, Infeasible, LedgerEntry, ModeChoiceParams, ModeContext, ModeDecision,
    OptionOffer, TravelOption, TripId, TripLedger, TripPurpose, VehicleCatalog, VehicleClass,
    VehicleCondition, VehicleLocation, VehicleOwner,
};
use magnat_ui::{Catalog, Locale, TripCard, TripView, VehicleCard, VehicleView};
use std::num::NonZeroU32;

fn encja(i: u32) -> Entity {
    Entity::new(i, NonZeroU32::new(1).unwrap())
}

const DOM: u32 = 1;
const PRACA: u32 = 2;

/// Anna z puli imion. Indeksy wyszukiwane po napisie, bo kolejność w `data/names/`
/// jest kontraktem zapisu gry — test ma się wywalić na zmianie **danych**, a nie
/// cicho pokazać kogoś innego.
fn anna() -> Identity {
    let c = magnat_agents::name_catalog();
    let imie = (0..c.first_len() as u16)
        .find(|i| c.first_name(*i) == "Anna")
        .expect("brak imienia Anna w data/names/first_names_pl.ron");
    let nazwisko = (0..c.surname_len() as u16)
        .find(|i| c.surname(*i, false) == "Kowalska")
        .expect("brak nazwiska Kowalska w data/names/surnames_pl.ron");
    Identity {
        first_name: imie,
        last_name: nazwisko,
        birth_day: -360 * 34,
        flags: Identity::FLAG_ALIVE,
        household: 100,
        ..Identity::default()
    }
}

/// Wybór środka na prawdziwych parametrach z `data/roads/mode_choice.ron`.
/// Sztuczna byłaby tu tylko lista ofert — rachunek jest ten sam, co w symulacji.
fn decyzja() -> ModeDecision {
    let p = ModeChoiceParams::load_default().expect("data/roads/mode_choice.ron");
    let ctx = ModeContext {
        citizen: 77,
        age_years: 34,
        status_percentile: 58,
        hourly_net_income_gr: 2_350,
        purpose: TripPurpose::Work,
        // Deszcz ze stopniem: to on wypycha Annę z roweru.
        weather: Weather {
            temp_dc: 60,
            precip_permille: 700,
            ..Weather::default()
        },
        luggage_kg: 3,
        habit: Some(TravelOption::CarOwn),
    };
    evaluate_modes(
        &p,
        &ctx,
        &[
            (
                TravelOption::Walk,
                Err(Infeasible::DistanceOverPersonalLimit { minutes: 108 }),
            ),
            (
                TravelOption::Bike,
                Ok(OptionOffer {
                    minutes: 41,
                    exposed_minutes: 41,
                    ..OptionOffer::default()
                }),
            ),
            (
                TravelOption::Transit,
                Ok(OptionOffer {
                    minutes: 46,
                    money: Money(240),
                    exposed_minutes: 11,
                    crowding_permille: 620,
                    transfers: 1,
                    walk_access_min: 9,
                    ..OptionOffer::default()
                }),
            ),
            (
                TravelOption::CarOwn,
                Ok(OptionOffer {
                    minutes: 28,
                    money: Money(742),
                    walk_access_min: 4,
                    ..OptionOffer::default()
                }),
            ),
            (TravelOption::CarHousehold, Err(Infeasible::CarInUseBy(81))),
            (
                TravelOption::Taxi,
                Err(Infeasible::BeyondBudget { fare_gr: 3_180 }),
            ),
        ],
    )
    .expect("auto jest wykonalne")
}

/// Rejestr przejazdu: 28 minut planu, 34 faktyczne. Wpisy krawędzi powstają tylko
/// dla śledzonych mieszkańców — Anna jest śledzona, więc rozbiór czasu jest pełny.
fn rejestr() -> TripLedger {
    #[allow(clippy::too_many_arguments)]
    let wpis = |edge: u32,
                entry: u64,
                exit: u64,
                travel_cs: u32,
                stops: u8,
                fuel_ul: i64,
                node_delay_cs: u32,
                blocked_cs: u32| {
        LedgerEntry {
            node_delay_cs,
            blocked_cs,
            edge: EdgeId(edge),
            entry: SimMinute(entry),
            exit: SimMinute(exit),
            travel_cs,
            mean_speed_dkmh: 380,
            stops,
            fuel_ul,
            money: Money::ZERO,
        }
    };
    // Doba 3 świata, wyjazd 07:20.
    let d = 3 * 1440;
    TripLedger {
        trip: TripId(4_201),
        entries: vec![
            wpis(1_204, d + 440, d + 447, 41_500, 0, 91_000, 0, 0),
            wpis(1_205, d + 447, d + 455, 47_200, 2, 104_000, 1_800, 0),
            // Wołoska: sześć minut w kolejce przed przewężeniem — to jest ten korek
            // ze zdania testowego fazy, i ma się pokazać w **swoim** kubełku.
            wpis(1_318, d + 458, d + 468, 58_800, 1, 121_000, 900, 36_000),
            wpis(1_319, d + 470, d + 474, 22_600, 3, 51_000, 2_400, 0),
        ],
        total_fuel_ul: 367_000,
        total_money: Money(742),
        edges: 4,
        stops: 6,
        distance_cm: 921_400,
        depart: SimMinute(d + 440),
        arrive: SimMinute(d + 474),
        mode: magnat_core::TransportMode::Car as u8,
        reason: DecisionReason::Citizen(CitizenReason::TripDelayed {
            planned_min: 28,
            actual_min: 34,
        }),
    }
}

fn karta_pojazdu(c: &Catalog, l: Locale) -> VehicleCard {
    let cat = VehicleCatalog::load_default().expect("data/vehicles/classes.ron");
    let id = cat.by_key("car_small").expect("car_small");
    let mut tank = FuelTank::full(cat.spec(id));
    tank.level = tank.capacity * 61 / 100;
    let cond = VehicleCondition {
        wear: 23,
        odometer_cm: 8_400_000_000,
        next_service_cm: 8_550_000_000,
        ..VehicleCondition::new()
    };
    let kierowca = anna();
    VehicleCard::build(
        c,
        l,
        &VehicleView {
            owner: &VehicleOwner::household(100, 77),
            driver: Some(&kierowca),
            class: VehicleClass::new(id),
            catalog: &cat,
            condition: &cond,
            tank: &tank,
            location: &VehicleLocation::parked(PlaceRef::Site(SiteId(encja(PRACA)))),
        },
    )
}

fn wydruk(l: Locale) -> String {
    let c = Catalog::load().expect("data/locale/");
    let osoba = anna();
    let d = decyzja();
    let r = rejestr();
    let karta = TripCard::build(
        &c,
        l,
        &TripView {
            traveller: &osoba,
            origin: PlaceRef::Building(BuildingId(encja(DOM))),
            dest: PlaceRef::Site(SiteId(encja(PRACA))),
            planned_minutes: 28,
            decision: &d,
            ledger: &r,
        },
    );
    format!(
        "{}{}",
        karta.render_text(&c, l),
        karta_pojazdu(&c, l).render_text(&c, l)
    )
}

fn sprawdz_wzorzec(l: Locale, plik: &str, wzorzec: &str) {
    let w = wydruk(l);
    if std::env::var("ZAPISZ_GOLDEN").is_ok() {
        std::fs::write(
            format!("{}/tests/golden/{plik}", env!("CARGO_MANIFEST_DIR")),
            &w,
        )
        .expect("zapis wzorca");
    }
    assert_eq!(
        w, wzorzec,
        "karta podróży się zmieniła.\n--- otrzymano ---\n{w}"
    );
}

#[test]
fn zloty_wydruk_podrozy_po_polsku() {
    sprawdz_wzorzec(
        Locale::Pl,
        "trip_pl.txt",
        include_str!("golden/trip_pl.txt"),
    );
}

#[test]
fn zloty_wydruk_podrozy_po_angielsku() {
    sprawdz_wzorzec(
        Locale::En,
        "trip_en.txt",
        include_str!("golden/trip_en.txt"),
    );
}

#[test]
fn karta_wypisuje_pelne_rozbicie_kosztu_kazdego_kandydata() {
    // Kryterium WP11: karta pokazuje **którą opcję wybrał i dlaczego**. „Dlaczego"
    // znaczy: koszt uogólniony każdego kandydata w groszach, z zaznaczeniem wybranego
    // i drugiego w kolejności — albo powód, dla którego kandydat w ogóle nie wchodził.
    let c = Catalog::load().expect("data/locale/");
    let osoba = anna();
    let d = decyzja();
    let r = rejestr();
    for l in Locale::ALL {
        let karta = TripCard::build(
            &c,
            l,
            &TripView {
                traveller: &osoba,
                origin: PlaceRef::Building(BuildingId(encja(DOM))),
                dest: PlaceRef::Site(SiteId(encja(PRACA))),
                planned_minutes: 28,
                decision: &d,
                ledger: &r,
            },
        );
        assert_eq!(
            karta.candidates.len(),
            TravelOption::ALL.len(),
            "karta gubi kandydatów"
        );
        assert_eq!(karta.candidates.iter().filter(|k| k.chosen).count(), 1);
        assert_eq!(karta.candidates.iter().filter(|k| k.runner_up).count(), 1);
        for k in &karta.candidates {
            match &k.cost {
                // Rachunek ma się zgadzać co do grosza: czas + pieniądz + dyskomfort.
                Some(g) => assert_eq!(
                    g.total.0,
                    g.time_cost.0 + g.money_cost.0 + g.discomfort.total().0,
                    "{} nie sumuje się",
                    k.label
                ),
                None => assert!(!k.note.is_empty(), "{} odpadł bez powodu", k.label),
            }
        }
        assert_eq!(karta.late_minutes, 6, "spóźnienie liczone z rejestru");
        let t = karta.split.expect("podróż śledzona ma rozbiór czasu");
        assert!(
            t.travel_cs > 0 && t.node_cs > 0 && t.queue_cs > 0,
            "rozbiór czasu jest pusty"
        );
        // Opóźnienie sygnalizacji **nie** ma lądować w kubełku kolejek (`N-6`):
        // to są dwie różne naprawy i miernik dryfu ma je rozróżniać.
        assert_eq!(t.queue_cs, 36_000, "czas kolejki policzony jako reszta");
        let wydruk = karta.render_text(&c, l);
        assert!(!wydruk.contains('{'), "nietrafione podstawienie:\n{wydruk}");
    }
}

#[test]
fn brak_parkingu_przeslania_porownanie_srodkow() {
    // PRD §14.1: „dlaczego Anna nie kupiła u mnie?" → „bo nie miała gdzie stanąć".
    // `R-7`: zdanie jest już w `describe`, karta ma je **pokazać**, a nie napisać.
    let c = Catalog::load().expect("data/locale/");
    let p = ModeChoiceParams::load_default().expect("data/roads/mode_choice.ron");
    let ctx = ModeContext {
        citizen: 77,
        age_years: 34,
        status_percentile: 58,
        hourly_net_income_gr: 2_350,
        purpose: TripPurpose::Shopping,
        weather: Weather {
            temp_dc: 180,
            precip_permille: 0,
            ..Weather::default()
        },
        luggage_kg: 0,
        habit: None,
    };
    let d = evaluate_modes(
        &p,
        &ctx,
        &[
            (
                TravelOption::Walk,
                Ok(OptionOffer {
                    minutes: 24,
                    ..OptionOffer::default()
                }),
            ),
            (
                TravelOption::CarOwn,
                Err(Infeasible::NoParkingWithinRadius { lots_searched: 11 }),
            ),
        ],
    )
    .expect("marsz zostaje");
    let osoba = anna();
    let karta = TripCard::build(
        &c,
        Locale::Pl,
        &TripView {
            traveller: &osoba,
            origin: PlaceRef::Building(BuildingId(encja(DOM))),
            dest: PlaceRef::Building(BuildingId(encja(9))),
            planned_minutes: 24,
            decision: &d,
            ledger: &TripLedger::default(),
        },
    );
    assert!(
        karta.choice_reason.contains("miejsca postojowego"),
        "karta nie mówi, dlaczego Anna nie przyjechała: {}",
        karta.choice_reason
    );
    let auto = karta
        .candidates
        .iter()
        .find(|k| !k.chosen && k.cost.is_none())
        .expect("auto jest na liście odrzuconych");
    assert!(
        auto.note.contains("11"),
        "karta nie mówi, ilu parkingów szukał"
    );
    assert!(
        karta.split.is_none(),
        "podróż bez wpisów nie ma prawa pokazać rozbioru czasu"
    );
}
