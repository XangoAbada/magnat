//! `R2-WP16`, pozycja 43 wykazu `R2`: **nastrój mieszkańca tylko spadał.**
//!
//! `DeprivationEffect::MoodLoss` był jedynym pisarzem `Vitals.mood` w całym
//! repozytorium i pisał wyłącznie `saturating_sub`. Po roku gry cała populacja siedziała
//! na −100, a przebieg 400-dobowy `m8miasto` mierzył średnią −99. Nastrój wchodzi
//! do produktywności pracownika (`mood01` w `effective_labor`) i do decyzji zakupowej,
//! więc miasto bez wyjścia z tego dołka mierzyło stałą, a nie stan.

use magnat_agents::{
    components::{AgentState, Needs, Vitals},
    DeprivationEffectsSystem, NeedTable,
};
use magnat_core::{NeedKind, Q};
use magnat_ecs::{App, ScheduleBuilder, World};

/// Świat z jednym mieszkańcem o zadanych potrzebach i zadanym nastroju.
fn swiat(needs: Needs, mood: i8) -> World {
    let mut w = World::new(1);
    magnat_agents::register(&mut w, NeedTable::load_default().expect("data/needs"));
    let _ = w
        .spawn()
        .with(needs)
        .with(Vitals {
            health: 100,
            energy: 100,
            mood,
            stress: 0,
            edu_level: 0,
            edu_field: 0,
            status: 50,
            _pad: 0,
        })
        .with(AgentState::default());
    w
}

fn godziny(world: World, ile: u64) -> World {
    let schedule = {
        let mut b = ScheduleBuilder::new();
        b.add(DeprivationEffectsSystem::new(&world));
        b.build().expect("harmonogram")
    };
    let mut app = App::new(world, schedule, 1);
    app.run_ticks(ile * 60);
    app.world
}

fn nastroj(world: &World) -> i8 {
    world
        .archetypes()
        .iter()
        .flat_map(|a| a.chunks())
        .flat_map(|c| c.entities().to_vec())
        .filter_map(|e| world.get::<Vitals>(e).map(|v| v.mood))
        .next()
        .expect("mieszkaniec")
}

/// Mieszkaniec z zaspokojonymi potrzebami wraca z dołka do zera.
///
/// Przed naprawą zostaje na −100 na zawsze: żaden kod w repozytorium nie podnosił
/// `Vitals.mood` ani o punkt.
#[test]
fn zaspokojony_mieszkaniec_wychodzi_z_dolka_nastroju() {
    let w = godziny(swiat(Needs::default(), -100), 200);
    let po = nastroj(&w);
    assert!(
        po > -100,
        "nastrój nie drgnął przy zaspokojonych potrzebach: {po}"
    );
    assert_eq!(po, 0, "odbudowa ma dojść do zera i tam stanąć: {po}");
}

/// Odbudowa nie jest darmowym dodatnim nastrojem: sufit to zero.
///
/// Test **strażnik**, nie odtwarzający: przed naprawą też przechodził, bo nastrój mógł
/// wyłącznie spadać. Pilnuje granicy, której naprawa nie ma prawa przekroczyć.
#[test]
fn odbudowa_nie_przebija_zera() {
    let w = godziny(swiat(Needs::default(), 0), 200);
    assert_eq!(
        nastroj(&w),
        0,
        "nastrój wyszedł ponad neutralny sam z siebie"
    );
}

/// Odbudowa wygrywa z jedną deprywacją nastrojową, a przegrywa z dwiema.
///
/// To jest właściwe kryterium, a nie samo „nastrój wraca": naprawa, która wygrywa
/// zawsze, zamieniłaby jedną stałą na drugą — zamiast „wszyscy na −100" byłoby
/// „wszyscy na zero" i nastrój znowu przestałby cokolwiek różnicować. Pierwsza połowa
/// **pada przed naprawą** (jedna zdeprywowana potrzeba spycha do −100), druga pilnuje,
/// żeby naprawa nie poszła za daleko.
#[test]
fn odbudowa_wygrywa_z_jedna_deprywacja_a_przegrywa_z_dwiema() {
    let mut jedna = Needs::default();
    jedna.set(NeedKind::Leisure, Q::new(0));
    let po_jednej = nastroj(&godziny(swiat(jedna, 0), 200));
    // Nie dokładnie zero: odbudowa 1,5 pkt/h i kara 1,0 pkt/h są dawkowane
    // całkowitoliczbowo po numerze godziny, więc nastrój drga między 0 a −1.
    // Przed naprawą było tam −100.
    assert!(
        po_jednej >= -2,
        "jedna zdeprywowana potrzeba nastrojowa zjadła całą odbudowę: {po_jednej}"
    );

    let mut dwie = Needs::default();
    dwie.set(NeedKind::Leisure, Q::new(0));
    dwie.set(NeedKind::Social, Q::new(0));
    let po_dwoch = nastroj(&godziny(swiat(dwie, 0), 200));
    assert!(
        po_dwoch < 0,
        "dwie zdeprywowane potrzeby nie ruszyły nastroju: {po_dwoch}"
    );
}
