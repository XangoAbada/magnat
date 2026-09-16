//! Stan firm wchodzi do hasha świata (dokument 00 §3.6, bramka 2 fazy, M7 §7.5).
//!
//! Test nie sprawdza konkretnej wartości — złoty odcisk fazy powstaje razem z jej
//! domknięciem. Sprawdza to, co jest tu bezwartościowe, gdy przestaje być prawdą:
//! że ten sam stan daje ten sam hash, i że **każda** rzecz, którą M7a zmienia,
//! ten hash rusza. Komponent poza hashem to dwa przebiegi jednego ziarna, które
//! rozjeżdżają się bez śladu.

use magnat_core::{DecisionReason, DistrictId, SimCalendar, SimMinute, Tick};
use magnat_ecs::World;
use magnat_firms::firm::{Firm, FirmStatus, Owner};
use magnat_firms::systems::register_firms;
use magnat_firms::{FirmKey, Firms, Tier};
use magnat_io::world_state_hash;

fn swiat(firm: u32) -> World {
    let mut w = World::new(7);
    let mut f = Firms::new();
    for i in 0..firm {
        f.insert(|key| {
            Firm::sole_owner(
                key,
                format!("Firma {i}"),
                SimMinute(0),
                DistrictId((i % 24) as u16),
                Owner::Player,
            )
        });
    }
    register_firms(&mut w, f);
    w
}

fn zmien(f: impl FnOnce(&mut Firms)) -> magnat_io::StateHash {
    let mut w = swiat(50);
    f(w.get_resource_mut::<Firms>().expect("rejestr firm"));
    world_state_hash(&w)
}

#[test]
fn ten_sam_stan_daje_ten_sam_hash() {
    assert_eq!(world_state_hash(&swiat(50)), world_state_hash(&swiat(50)));
}

#[test]
fn zalozenie_firmy_rusza_hash() {
    assert_ne!(world_state_hash(&swiat(50)), world_state_hash(&swiat(51)));
}

#[test]
fn kazda_zmiana_stanu_firmy_rusza_hash() {
    let baza = world_state_hash(&swiat(50));

    /// Jedna zmiana stanu rejestru, opisana nazwą do komunikatu asercji.
    type Przypadek = (&'static str, Box<dyn FnOnce(&mut Firms)>);

    let przypadki: Vec<Przypadek> = vec![
        (
            "zamknięcie firmy",
            Box::new(|f: &mut Firms| {
                f.get_mut(FirmKey(1)).expect("firma").status = FirmStatus::Closed;
            }),
        ),
        (
            "wpis do dziennika decyzji",
            Box::new(|f: &mut Firms| {
                f.log(FirmKey(1), Tick(5), DecisionReason::Unspecified);
            }),
        ),
        (
            "zmiana dyrektora",
            Box::new(|f: &mut Firms| {
                f.get_mut(FirmKey(2)).expect("firma").director = None;
                f.get_mut(FirmKey(2)).expect("firma").hq_district = DistrictId(99);
            }),
        ),
    ];

    for (nazwa, zmiana) in przypadki {
        assert_ne!(baza, zmien(zmiana), "{nazwa} nie ruszyła hasha");
    }
}

#[test]
fn dziennik_decyzji_rozroznia_powod() {
    // Dwa wpisy w tym samym ticku, różne powody — wyjaśnialność jest stanem,
    // a nie ozdobą karty inspekcji (dokument 00 §7).
    let a = zmien(|f| f.log(FirmKey(1), Tick(5), DecisionReason::Unspecified));
    let b = zmien(|f| {
        f.log(
            FirmKey(1),
            Tick(5),
            DecisionReason::NeedCritical {
                need: magnat_core::NeedKind::Hunger,
                level: magnat_core::Q::new(10),
            },
        );
    });
    assert_ne!(a, b);
}

#[test]
fn kubelki_slotow_nie_wchodza_do_hasha() {
    // Indeks pochodny **nie może** wchodzić: jest funkcją zbioru kluczy i odtwarza
    // się z niego w całości. Gdyby wchodził, jego przebudowa zmieniałaby hash stanu
    // bez zmiany stanu — czyli replay pękałby po każdej reorganizacji indeksu.
    //
    // Dowód pośredni, bo indeks jest prywatny: przydział slotów bez zdjęcia decyzji
    // z kolejki nie zmienia niczego, co jest stanem.
    let mut a = swiat(50);
    let przed = world_state_hash(&a);
    {
        let f = a.get_resource_mut::<Firms>().expect("rejestr");
        // Minuta, w której żadna z 50 firm nie ma slotu operacyjnego — kolejka
        // zostaje pusta, więc stan się nie zmienia.
        let pusta = (0..1440u64)
            .find(|m| {
                let cal = SimCalendar::new(Tick(*m));
                f.iter().all(|(k, _)| {
                    !magnat_firms::due(magnat_firms::slots(k), Tier::Operational, cal)
                })
            })
            .expect("jakaś minuta bez decyzji przy 50 firmach");
        f.schedule(SimCalendar::new(Tick(pusta)));
    }
    assert_eq!(przed, world_state_hash(&a));
}

#[test]
fn kolejka_przepelnienia_jest_stanem_i_rusza_hash() {
    // Przesunięcie decyzji na następny tick jest stanem, nie szczegółem wykonania
    // (M7 §8, ryzyko `R4`): gdyby nie wchodziło do hasha, dwa przebiegi jednego
    // ziarna mogłyby podjąć te same decyzje w **innych** minutach i nikt by tego
    // nie zobaczył.
    //
    // Przepełnienie trzeba wywołać **naprawdę**, a nie założyć. Poziom operacyjny
    // się nie przepełnia — 10 tys. firm na 1440 minut daje pik równy sufitowi 32 —
    // więc bierze się poziom taktyczny: te same firmy mieszczą się w 30 dniach
    // po 24 godziny, czyli kilkanaście na godzinę przy sufycie 4.
    let mut w = swiat(6_000);
    let przed = world_state_hash(&w);
    let f = w.get_resource_mut::<Firms>().expect("rejestr");

    let mut minuta = 0u64;
    while f.backlog(Tier::Tactical) == 0 {
        f.schedule(SimCalendar::new(Tick(minuta)));
        minuta += 60;
        assert!(
            minuta < 30 * 1440,
            "kolejka taktyczna nigdy się nie przepełniła"
        );
    }
    assert_ne!(przed, world_state_hash(&w), "kolejka nie ruszyła hasha");
}
