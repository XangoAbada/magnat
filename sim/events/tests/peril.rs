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
