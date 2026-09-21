//! Zdarzenie jako **źródło szkody** (M10d WP10.12, `GD-4`, `K-86`).
//!
//! Do M10d żadne zdarzenie w tej grze niczego nie niszczyło: sześć wariantów
//! `Effect` to odwracalne mnożniki parametrów, a `ParamOverlay` przywraca wartość
//! zastaną przy wygaśnięciu. Szkoda majątkowa jest jednorazowa, więc idzie osobnym
//! polem `EventDef::peril` i osobną drogą — zgłoszeniem do `sim/economy`.
//!
//! Ten plik pilnuje trzech rzeczy, których żaden inny test nie widzi:
//! **katalog gry naprawdę ma zdarzenia niosące ryzyko**, odwzorowanie zakresu na
//! cel jest takie, jak opisano, a zdarzenie bez ryzyka **milczy**.

use magnat_events::catalog::{EventCatalog, EventScope};
use magnat_events::system::peril_of;
use magnat_events::ScopeInstance;

fn katalog() -> EventCatalog {
    EventCatalog::load_default().expect("data/events/")
}

fn def<'a>(c: &'a EventCatalog, key: &str) -> &'a magnat_events::catalog::EventDef {
    c.defs
        .iter()
        .find(|d| d.key == key)
        .unwrap_or_else(|| panic!("brak zdarzenia `{key}` w katalogu"))
}

/// Katalog gry **ma** zdarzenia niosące ryzyko ubezpieczeniowe.
///
/// Test jest o katalogu, nie o kodzie, i to jest jego istota: `PerilKind` bez ani
/// jednego zdarzenia byłby ryzykiem, które nigdy nie zachodzi — przeszedłby każdy
/// test jednostkowy i wyglądałby tak samo jak działający (`K-67`).
#[test]
fn kazde_ryzyko_ma_w_katalogu_swoje_zdarzenie() {
    let c = katalog();
    for p in magnat_core::PerilKind::ALL {
        assert!(
            c.defs.iter().any(|d| d.peril == Some(*p)),
            "ryzyko {} nie ma ani jednego zdarzenia w `data/events/`",
            p.name()
        );
    }
}

/// Powódź jest dzielnicowa i zgłasza swoją dzielnicę.
#[test]
fn powodz_zglasza_dzielnice() {
    let c = katalog();
    let d = def(&c, "natural/flood");
    assert_eq!(d.scope, EventScope::District);
    assert_eq!(d.peril, Some(magnat_core::PerilKind::Flood));

    let o = peril_of(d, ScopeInstance::District(4), 7_500, 123).expect("zgłoszenie");
    assert_eq!(o.peril, magnat_core::PerilKind::Flood);
    assert_eq!(o.district, Some(magnat_core::DistrictId(4)));
    assert_eq!(o.site, None);
    assert_eq!(o.severity_bps, 7_500);
    assert_eq!(o.day, 123);
}

/// Pożar magazynu jest zakładowy i zgłasza swój zakład.
///
/// Przy okazji pilnuje bramki: `MinSiteLines(1)` wykluczał z pożaru każdy sklep,
/// choć sklep ma zaplecze i to właśnie ono płonie (`K-86`).
#[test]
fn pozar_zglasza_zaklad_i_nie_omija_sklepow() {
    let c = katalog();
    let d = def(&c, "firm/warehouse_fire");
    assert_eq!(d.scope, EventScope::Site);
    assert_eq!(d.peril, Some(magnat_core::PerilKind::Fire));
    assert!(
        d.trigger.gate.is_empty(),
        "bramka pożaru wróciła i znów wyklucza sklepy: {:?}",
        d.trigger.gate
    );

    let site = magnat_core::SiteId(magnat_core::Entity::new(77, std::num::NonZeroU32::MIN));
    let o = peril_of(d, ScopeInstance::Site(site), 10_000, 5).expect("zgłoszenie");
    assert_eq!(o.peril, magnat_core::PerilKind::Fire);
    assert_eq!(o.site, Some(site));
    assert_eq!(o.district, None);
}

/// Zdarzenie bez ryzyka milczy — i to jest większość katalogu.
#[test]
fn zdarzenie_bez_ryzyka_nie_zglasza_niczego() {
    let c = katalog();
    let d = def(&c, "natural/drought");
    assert_eq!(d.peril, None, "susza nie niszczy majątku, tylko plon");
    assert!(peril_of(d, ScopeInstance::District(1), 10_000, 1).is_none());

    // Ile zdarzeń w ogóle niesie ryzyko — liczba jest w teście po to, żeby
    // dopisanie ryzyka do zdarzenia było **świadome**, a nie przypadkowe.
    let z_ryzykiem = c.defs.iter().filter(|d| d.peril.is_some()).count();
    assert_eq!(
        z_ryzykiem, 2,
        "zmieniła się liczba zdarzeń niszczących majątek — sprawdź, czy to było zamierzone"
    );
}

/// Zakres firmowy i sieciowy **nie degradują się do całego miasta**.
///
/// Domyślna odpowiedź „całe miasto" byłaby najszerszą możliwą szkodą, czyli najgorszą
/// z możliwych — znalezisko recenzji M10d.
#[test]
fn zakres_bez_ramienia_milczy_zamiast_uderzac_w_cale_miasto() {
    let c = katalog();
    let d = def(&c, "natural/flood");
    assert!(peril_of(d, ScopeInstance::World, 10_000, 1).is_none());
    assert!(peril_of(d, ScopeInstance::Network(0), 10_000, 1).is_none());
}

/// `FF-21`: **ile szkód na rok gry** daje katalog — i czy karencja tego nie zjada.
///
/// # Dlaczego to nie jest przebieg miasta
///
/// Bo przebieg miasta daje **jedną próbkę**, a pytanie brzmi „ile na rok,
/// uśrednione po ziarnach". Czterysta dób w jednym mieście przy oczekiwanych
/// 0,65 powodzi na rok to rzut monetą: zero szkód niczego nie dowodzi, jedna
/// też nie. Liczba, o którą chodzi, jest **arytmetyką katalogu** — hazard razy
/// liczba instancji zakresu razy długość sezonu — i to samo mnożenie robi
/// [`kazda_definicja_moze_zajsc_w_dwudziestoleciu`] w `catalog.rs`.
///
/// Ten test dokłada do tego jedno, czego tamten nie ma i co `FF-21` nazywa
/// pułapką: **`cooldown_days` jest przerwą definicji, nie podmiotu** (`CH-11`).
/// Jeden pożar magazynu ucisza **wszystkie** zakłady w mieście na sto osiemdziesiąt
/// dób, więc realizowana częstość ma twardy sufit `360 / cooldown_days` na rok gry
/// — niezależnie od tego, ile zakładów stoi w mieście. Sufit wiąże dopiero przy
/// dużym mieście i dlatego nie widać go w małym: to jest dokładnie ta klasa liczby,
/// którą pojedynczy przebieg 4 km przepuszcza.
#[test]
fn szkody_na_rok_gry_i_sufit_karencji() {
    let c = katalog();
    // Dwa miasta, żeby było widać, kiedy sufit zaczyna wiązać. Liczby instancji
    // zakresu są z tego samego rachunku co w `catalog.rs`, tylko rozpisane na
    // rozmiar: 4 km ma ~8 dzielnic i ~220 zakładów, metropolia ~24 i ~2 200.
    for (nazwa, dzielnic, zakladow) in [("4 km", 8.0, 220.0), ("metropolia", 24.0, 2_200.0)] {
        for d in c.defs.iter().filter(|d| d.peril.is_some()) {
            let instancji = match d.scope {
                EventScope::District => dzielnic,
                EventScope::Site => zakladow,
                _ => 1.0,
            };
            let sezonowa = d
                .trigger
                .gate
                .iter()
                .any(|g| matches!(g, magnat_events::Precondition::Season(_)));
            let dob_w_sezonie = if sezonowa { 270.0 } else { 360.0 };
            // Hazard przy **neutralnych** sondach: tyle wypada w roku przeciętnym.
            // Krzywe mnożą to w roku mokrym, ale bramka pyta o przeciętny.
            let z_hazardu = f64::from(d.trigger.base_ppm) / 1e6 * dob_w_sezonie * instancji;
            // Sufit karencji: definicja milczy `cooldown_days` po każdym wygaśnięciu.
            let sufit = 360.0 / f64::from(d.cooldown_days.max(1));
            let realna = z_hazardu.min(sufit);
            println!(
                "{nazwa}  {:<22} hazard {z_hazardu:.2}/rok, sufit karencji {sufit:.2}/rok \
                 → realnie {realna:.2}/rok",
                d.key
            );
            assert!(
                realna >= 0.2,
                "`{}` daje w mieście {nazwa} {realna:.2} szkody na rok gry — \
                 gracz nie zobaczy tego mechanizmu przez pięć lat",
                d.key
            );
            assert!(
                realna <= 12.0,
                "`{}` daje w mieście {nazwa} {realna:.1} szkód na rok gry — \
                 ubezpieczenie przestaje być zakładem, a staje się abonamentem",
                d.key
            );
        }
    }
}

/// Karencja **jest** ograniczeniem, a nie ozdobą — i test mówi, gdzie zaczyna wiązać.
///
/// Osobno od poprzedniego, bo mierzy co innego: tamten pyta „ile szkód",
/// ten „od jakiego miasta katalog przestaje skalować się z liczbą zakładów".
/// Odpowiedź jest treścią `FF-21`: powyżej tego progu dołożenie zakładów **nie**
/// zwiększa liczby pożarów, więc duże miasto ma ich na zakład mniej niż małe.
#[test]
fn karencja_wiaze_powyzej_progu_liczby_zakladow() {
    let c = katalog();
    let d = def(&c, "firm/warehouse_fire");
    assert_eq!(d.scope, EventScope::Site);
    let sufit = 360.0 / f64::from(d.cooldown_days.max(1));
    // Ile zakładów musi mieć miasto, żeby hazard dobił do sufitu karencji.
    let prog = sufit / (f64::from(d.trigger.base_ppm) / 1e6 * 360.0);
    println!(
        "firm/warehouse_fire: sufit {sufit:.2}/rok wiąże od {prog:.0} zakładów \
         (4 km ma ~220, metropolia ~2200)"
    );
    assert!(
        (100.0..5_000.0).contains(&prog),
        "próg {prog:.0} zakładów wypada poza zakresem rozmiarów miasta z §4.1 — \
         albo karencja nie wiąże nigdy, albo wiąże zawsze, i w obu przypadkach \
         jest parametrem bez pytania, na które odpowiada"
    );
}
