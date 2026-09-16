//! Kryterium zamknięcia M7a WP2: walidator katalogu typów zakładów, na **prawdziwych**
//! danych z `data/`, a nie na atrapie.
//!
//! Reguła krzyżowa („każdy typ zakładu ma archetyp w `data/buildings/`, a jego receptury
//! istnieją w katalogu towarów") ma swój test tam, gdzie widać oba katalogi naraz —
//! w `tools/headless`, bo to on stawia miasto z jednego i drugiego. Tu jest wszystko,
//! co da się sprawdzić bez generatora miasta.

use magnat_firms::catalog::{CatalogError, SiteTypeCatalog, SITE_TYPES_SCHEMA_VERSION};
use magnat_firms::hr::roles::RoleTable;
use magnat_supply::catalog::load_default;
use magnat_supply::Catalog;
use std::collections::BTreeMap;

fn katalogi() -> (RoleTable, Catalog) {
    (
        RoleTable::load_default().expect("data/jobs/roles.ron"),
        load_default("contemporary").expect("katalog towarów z data/"),
    )
}

fn site_types() -> SiteTypeCatalog {
    let (roles, goods) = katalogi();
    // Mapa pusta = bez reguł krzyżowych; te sprawdza test w `tools/headless`.
    SiteTypeCatalog::load_default(&roles, &goods, &BTreeMap::new())
        .expect("data/site_types/ ładuje się i przechodzi walidację")
}

#[test]
fn katalog_typow_zakladow_sie_laduje() {
    let c = site_types();
    assert!(
        c.len() >= 90,
        "katalog ma {} typów, a miasto ma 96 archetypów firmowych",
        c.len()
    );
}

#[test]
fn kazdy_typ_ma_stanowisko_kierownicze() {
    // Reguła jest w walidatorze, więc `load` już by się nie powiodło — ale ten test
    // mówi **dlaczego** ona istnieje i pęknie, gdyby ktoś ją z walidatora wyjął.
    // Zakład bez kierownika nie ma komu delegować (M7c) ani kogo zwolnić (M7d).
    for (_, t) in site_types().iter() {
        assert!(
            t.staffing.iter().any(|s| s.managerial),
            "typ zakładu „{}” nie ma stanowiska kierowniczego",
            t.key
        );
    }
}

#[test]
fn kazdy_typ_kogos_zatrudnia() {
    for (_, t) in site_types().iter() {
        let zatrudnia = t
            .staffing
            .iter()
            .any(|s| s.per_10000m2 > 0 || s.per_site > 0 || s.min > 0);
        assert!(zatrudnia, "typ zakładu „{}” nie zatrudnia nikogo", t.key);
    }
}

#[test]
fn obsada_typowego_zakladu_jest_w_rozsadnym_rzedzie_wielkosci() {
    // Zakład o 2000 m² nie ma mieć ani jednego pracownika, ani pięciuset.
    // To nie jest test kalibracji — to jest siatka na zero i na przecinek
    // przestawiony o trzy miejsca.
    for (_, t) in site_types().iter() {
        let etaty: u32 = t.staffing.iter().map(|s| u32::from(s.slots(2_000))).sum();
        assert!(
            (1..=500).contains(&etaty),
            "typ zakładu „{}” na 2000 m² obsadza {etaty} etatów",
            t.key
        );
    }
}

#[test]
fn koszt_staly_i_naklad_rosna_z_powierzchnia() {
    for (_, t) in site_types().iter() {
        let maly = t.fixed_cost_month(500);
        let duzy = t.fixed_cost_month(5_000);
        assert!(
            duzy.get() > maly.get(),
            "typ zakładu „{}”: czynsz nie zależy od powierzchni",
            t.key
        );
        assert!(
            t.capex(5_000).get() >= t.capex(500).get(),
            "typ zakładu „{}”: nakład maleje z powierzchnią",
            t.key
        );
        assert!(maly.get() > 0, "typ zakładu „{}” nic nie kosztuje", t.key);
    }
}

#[test]
fn nieznana_rola_jest_bledem_a_nie_ostrzezeniem() {
    let (roles, goods) = katalogi();
    let dir = std::env::temp_dir().join("magnat_m7a_zla_rola");
    std::fs::create_dir_all(&dir).expect("katalog tymczasowy");
    std::fs::write(
        dir.join("x.ron"),
        format!(
            r#"(
                schema_version: {SITE_TYPES_SCHEMA_VERSION},
                site_types: [(
                    key: "supermarket",
                    category: Retail,
                    staffing: [( role: "zaklinacz_deszczu", per_site: 1, managerial: true )],
                    capex: ( build: 1, equip_per_100m2: 1 ),
                    fixed_cost: ( rent_per_m2: 1, admin: 1 ),
                )],
            )"#
        ),
    )
    .expect("zapis pliku");
    let wynik = SiteTypeCatalog::load(&dir, &roles, &goods, &BTreeMap::new());
    std::fs::remove_dir_all(&dir).ok();
    assert!(matches!(wynik, Err(CatalogError::UnknownRole { .. })));
}

#[test]
fn typ_bez_kierownika_jest_bledem() {
    let (roles, goods) = katalogi();
    let dir = std::env::temp_dir().join("magnat_m7a_bez_kierownika");
    std::fs::create_dir_all(&dir).expect("katalog tymczasowy");
    std::fs::write(
        dir.join("x.ron"),
        format!(
            r#"(
                schema_version: {SITE_TYPES_SCHEMA_VERSION},
                site_types: [(
                    key: "supermarket",
                    category: Retail,
                    staffing: [( role: "cashier", per_10000m2: 90 )],
                    capex: ( build: 1, equip_per_100m2: 1 ),
                    fixed_cost: ( rent_per_m2: 1, admin: 1 ),
                )],
            )"#
        ),
    )
    .expect("zapis pliku");
    let wynik = SiteTypeCatalog::load(&dir, &roles, &goods, &BTreeMap::new());
    std::fs::remove_dir_all(&dir).ok();
    assert!(matches!(wynik, Err(CatalogError::NoManager(_))));
}

#[test]
fn identyfikator_typu_nie_zalezy_od_pliku() {
    // Kolejność jest alfabetyczna po kluczu, więc przeniesienie rekordu między
    // plikami branżowymi nie przenumerowuje niczego — a `SiteTypeId` siedzi
    // w zakładzie, czyli w zapisie gry.
    let c = site_types();
    let klucze: Vec<&str> = c.iter().map(|(_, t)| t.key.as_str()).collect();
    let mut posortowane = klucze.clone();
    posortowane.sort_unstable();
    assert_eq!(klucze, posortowane);
}

#[test]
fn wagi_produktywnosci_pokrywaja_caly_katalog_rol() {
    // `RoleTable::load` odrzuca wagi niesumujące się do 1000, więc samo załadowanie
    // jest dowodem — ale bez tego testu nikt by nie zauważył, że plik przestał
    // być ładowany, gdyby ktoś usunął jego jedynego czytelnika.
    let roles = RoleTable::load_default().expect("data/jobs/roles.ron");
    assert!(roles.len() >= 40, "ról jest {}", roles.len());
    for i in 0..roles.len() {
        let w = roles.weights(magnat_core::JobRoleId(i as u16));
        assert_eq!(
            w.sum(),
            1000,
            "rola „{}”",
            roles.key(magnat_core::JobRoleId(i as u16))
        );
    }
}
