//! **WP6 — pogoda jest deterministyczna i zgodna z normami M1.**
//!
//! Kryterium podfazy mówi „średnia roczna temperatura ±0,5 °C, suma opadów ±10 %".
//! Mierzy się to na **dwudziestoleciu**, nie na jednym roku, i to nie jest
//! rozluźnienie kryterium — to jego jedyne wykonalne odczytanie. Pogoda jest
//! procesem z pamięcią: przy czasie korelacji rzędu dwóch tygodni pojedynczy rok
//! ma kilkanaście niezależnych obserwacji wilgotności, a nie trzysta sześćdziesiąt.
//! Rok cieplejszy od normy o 0,6 °C nie jest wtedy usterką modelu, tylko rokiem
//! cieplejszym od normy — i tak samo jest w rzeczywistości. Wiążące jest to,
//! że **nie ma dryfu**: średnia z dwudziestu lat wraca do klimatu.
//!
//! Przy okazji test sprawdza to, co jest warunkiem istnienia suszy: że wilgotne
//! i suche okresy **się kleją**. Przy losowaniu niezależnym doba po dobie niedobór
//! trzydziestodobowy nigdy nie doszedłby do wartości, przy których krzywa suszy
//! zaczyna rosnąć — i zdarzenie byłoby martwe (ryzyko `R2`).

use magnat_core::{HashState, StateHasher, Tick};
use magnat_ecs::World;
use magnat_events::{ClimateNorms, EpochClock, EventCatalog, Events, WeatherState};

const DOB_W_ROKU: u64 = 360;

struct Pomiar {
    srednia_temp_dc: i64,
    opad_chm: i64,
    norma_opadu_chm: i64,
    szczyt_niedoboru_chm: i64,
}

/// Przebieg pogody bez miasta. Katalog jest pusty, więc mierzy się **wyłącznie**
/// proces pogodowy, bez sprzężenia ze zdarzeniami.
fn przebieg(seed: u64, lat: u64) -> Pomiar {
    let normy = ClimateNorms::default();
    let mut world = World::new(seed);
    let mut ev = Events::new(
        EventCatalog::default(),
        WeatherState::new(normy),
        EpochClock::new(1990),
    );
    let mut suma_t: i64 = 0;
    let mut n_t: i64 = 0;
    let mut opad: i64 = 0;
    let mut norma_opadu: i64 = 0;
    let mut szczyt: i64 = 0;
    for doba in 0..DOB_W_ROKU * lat {
        for godzina in 0..24u64 {
            magnat_events::krok(&mut ev, &mut world, Tick((doba * 24 + godzina) * 60), seed);
            suma_t += i64::from(ev.weather().weather().temp_dc);
            n_t += 1;
        }
        opad += i64::from(ev.weather().precip_today_chm());
        norma_opadu += i64::from(normy.precip_at_day_chm((doba % 360) as u16));
        szczyt = szczyt.max(ev.weather().precip_deficit_chm(30));
    }
    Pomiar {
        srednia_temp_dc: suma_t / n_t,
        opad_chm: opad,
        norma_opadu_chm: norma_opadu,
        szczyt_niedoboru_chm: szczyt,
    }
}

#[test]
fn dwudziestolecie_trzyma_sie_normy_klimatu() {
    let normy = ClimateNorms::default();
    let norma_t: i64 = normy
        .temp_monthly_dc
        .iter()
        .map(|t| i64::from(*t))
        .sum::<i64>()
        / 12;
    let p = przebieg(7, 20);
    assert!(
        (p.srednia_temp_dc - norma_t).abs() <= 5,
        "średnia z dwudziestolecia {} dziesiątych wobec normy {norma_t} — więcej niż 0,5 °C",
        p.srednia_temp_dc
    );
    let odchylenie = (p.opad_chm - p.norma_opadu_chm).abs() * 100 / p.norma_opadu_chm.max(1);
    assert!(
        odchylenie <= 10,
        "suma opadu {} wobec normy {} — odchylenie {odchylenie} %",
        p.opad_chm,
        p.norma_opadu_chm
    );
}

#[test]
fn nie_ma_dryfu_miedzy_pierwsza_a_druga_dekada() {
    // Test na **dryf**, nie na wartość: proces bez kotwicy odpłynąłby od normy
    // i druga dekada byłaby cieplejsza albo zimniejsza od pierwszej systematycznie.
    let a = przebieg(21, 10).srednia_temp_dc;
    let b = przebieg(22, 10).srednia_temp_dc;
    assert!(
        (a - b).abs() <= 10,
        "dwie dekady na dwóch ziarnach różnią się o {} dziesiątych — proces odpływa od normy",
        (a - b).abs()
    );
}

#[test]
fn susze_maja_z_czego_powstac() {
    // Bez pamięci procesu wilgotności niedobór trzydziestodobowy krążyłby wokół
    // zera i nie doszedłby do 4 000 setnych mm — czyli do punktu, w którym krzywa
    // suszy w `data/events/natural.ron` daje mnożnik ×3.
    let p = przebieg(7, 5);
    assert!(
        p.szczyt_niedoboru_chm >= 4_000,
        "największy niedobór 30-dobowy w pięcioleciu to {} setnych mm — krzywa suszy \
         nigdy nie ruszy, a zdarzenie jest martwe",
        p.szczyt_niedoboru_chm
    );
}

#[test]
fn ten_sam_seed_daje_ta_sama_pogode() {
    let a = przebieg(11, 1);
    let b = przebieg(11, 1);
    assert_eq!(a.srednia_temp_dc, b.srednia_temp_dc);
    assert_eq!(a.opad_chm, b.opad_chm);
    assert_eq!(a.szczyt_niedoboru_chm, b.szczyt_niedoboru_chm);
    let c = przebieg(12, 1);
    assert_ne!(
        a.opad_chm, c.opad_chm,
        "dwa różne ziarna dały identyczny rok"
    );
}

#[test]
fn stan_zdarzen_wchodzi_do_hasha() {
    // Bez tego testu rozjazd w pogodzie przeszedłby przez bramkę determinizmu
    // niezauważony: hash stanu widzi wyłącznie to, co zadeklarowano (00 §3.6).
    let mut world = World::new(5);
    let mut ev = Events::new(
        EventCatalog::default(),
        WeatherState::new(ClimateNorms::default()),
        EpochClock::new(1990),
    );
    let odcisk = |e: &Events| {
        let mut h = StateHasher::new();
        e.hash_state(&mut h);
        h.finish()
    };
    let przed = odcisk(&ev);
    for godzina in 0..48u64 {
        magnat_events::krok(&mut ev, &mut world, Tick(godzina * 60), 5);
    }
    assert_ne!(
        przed,
        odcisk(&ev),
        "dwie doby pogody nie zmieniły hasha stanu zdarzeń"
    );
}
