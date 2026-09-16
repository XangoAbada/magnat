//! Reguła krzyżowa katalogów M7a WP2: `data/site_types/` (M7) i `data/buildings/` (M2)
//! opisują **ten sam** zakład tym samym kluczem tekstowym.
//!
//! Test mieszka tutaj, bo `tools/headless` jest jedynym miejscem, z którego widać oba
//! katalogi naraz — `sim/firms` nie zależy od `sim/world` i zależeć nie będzie.
//! Bez tego testu rozjazd byłby **cichy**: archetyp zmieniłby nazwę, zakład przestałby
//! mieć typ, obsada wyszłaby pusta, a objawem byłby zakład bez ani jednego pracownika
//! zauważony kiedyś w panelu. To jest cały koszt trzymania tych opisów osobno
//! i to jest jego spłata.

use magnat_firms::catalog::{CatalogError, SiteTypeCatalog};
use magnat_firms::hr::roles::RoleTable;
use magnat_world::city::grammar::GrammarSet;
use magnat_world::city::sites::SiteCatalog;
use std::collections::{BTreeMap, BTreeSet};

/// Sektory, których zakłady **nie są firmami**: szkoła, szpital, ratusz, park.
/// Instytucje miejskie prowadzi M8 i to on dopisze im typy zakładów.
const NIE_FIRMY: &[&str] = &["Public", "Green"];

struct Katalogi {
    site_catalog: SiteCatalog,
    roles: RoleTable,
    goods: magnat_supply::Catalog,
}

fn katalogi() -> Katalogi {
    let mats =
        magnat_voxel::MaterialRegistry::load_dir(&magnat_core::assets::data_path("materials"))
            .expect("data/materials/");
    let grammars =
        GrammarSet::load_dir(&magnat_core::assets::data_path("grammar"), &mats).expect("gramatyki");
    let goods = magnat_supply::catalog::load_default("contemporary").expect("katalog towarów");
    let site_catalog = SiteCatalog::load_default(&goods, &grammars).expect("archetypy z M2");
    Katalogi {
        site_catalog,
        roles: RoleTable::load_default().expect("data/jobs/roles.ron"),
        goods,
    }
}

/// Archetypy wraz z kluczami ich receptur — argument, którego oczekuje walidator M7.
fn archetypy(k: &Katalogi) -> BTreeMap<String, Vec<String>> {
    k.site_catalog
        .archetypes
        .iter()
        .map(|a| {
            let recepty = a
                .recipes
                .iter()
                .map(|r| k.goods.recipe(*r).key.to_string())
                .collect();
            (a.key().to_owned(), recepty)
        })
        .collect()
}

#[test]
fn kazdy_typ_zakladu_ma_archetyp_w_ktorym_moze_stanac() {
    let k = katalogi();
    // Pełna walidacja z regułami krzyżowymi: typ bez archetypu i receptura spoza
    // katalogu towarów są **błędami**, nie ostrzeżeniami.
    SiteTypeCatalog::load_default(&k.roles, &k.goods, &archetypy(&k))
        .expect("data/site_types/ zgadza się z data/buildings/");
}

#[test]
fn kazdy_archetyp_firmowy_ma_typ_zakladu() {
    // Druga strona tej samej reguły. Walidator jej nie egzekwuje z rozmysłu —
    // archetyp bez typu zakładu jest **poprawnym** stanem dla instytucji miejskiej,
    // której firmą nie jest. Dla reszty jest dziurą i ta lista ma być pusta.
    let k = katalogi();
    let typy = SiteTypeCatalog::load_default(&k.roles, &k.goods, &archetypy(&k)).expect("katalog");
    let znane: BTreeSet<&str> = typy.iter().map(|(_, t)| t.key.as_str()).collect();

    let brakujace: Vec<&str> = k
        .site_catalog
        .archetypes
        .iter()
        .filter(|a| !NIE_FIRMY.contains(&format!("{:?}", a.spec.sector).as_str()))
        .map(magnat_world::city::sites::Archetype::key)
        .filter(|key| !znane.contains(key))
        .collect();

    assert!(
        brakujace.is_empty(),
        "archetypy firmowe bez typu zakładu — staną w mieście bez ani jednego etatu: {brakujace:?}"
    );
}

#[test]
fn typ_zakladu_bez_archetypu_jest_bledem() {
    // Test testu: gdyby reguła krzyżowa przestała działać, poprzednie dwa testy
    // przechodziłyby z błędnego powodu — bo niczego by nie sprawdzały.
    let k = katalogi();
    let dir = std::env::temp_dir().join("magnat_m7a_bez_archetypu");
    std::fs::create_dir_all(&dir).expect("katalog tymczasowy");
    std::fs::write(
        dir.join("x.ron"),
        r#"(
            schema_version: 1,
            site_types: [(
                key: "fabryka_jednorozcow",
                category: Manufacturing,
                staffing: [( role: "plant_manager", per_site: 1, managerial: true )],
                capex: ( build: 1, equip_per_100m2: 1 ),
                fixed_cost: ( rent_per_m2: 1, admin: 1 ),
            )],
        )"#,
    )
    .expect("zapis pliku");
    let wynik = SiteTypeCatalog::load(&dir, &k.roles, &k.goods, &archetypy(&k));
    std::fs::remove_dir_all(&dir).ok();
    assert!(matches!(wynik, Err(CatalogError::NoArchetype(_))));
}
