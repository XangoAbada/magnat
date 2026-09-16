//! Walidator grafu produktów jako test CI (dok. 00 §5, M6a WP1 reguły 1–5).
//!
//! Test negatywny odziedziczony po M2 (`zapas_startowy_nie_jest_zrodlem`) **nie wolno
//! osłabić**: klasa błędu, którą łapie, ujawnia się dopiero jako „miasto po dwóch
//! tygodniach przestaje produkować X", czyli długo po commicie, który ją wprowadził.

use magnat_core::{LossKind, RecipeId};
use magnat_supply::catalog::load_default;
use magnat_supply::{Catalog, CatalogError, OutputKind, RecipeSource};

fn katalog() -> Catalog {
    load_default("contemporary").expect("katalog z data/")
}

/// Domeny klucza w konwencji `<domena>_<nazwa>[_<wariant>]` (M6a §5.1). Lista jest
/// zamknięta z rozmysłu: nowa domena to decyzja, a nie literówka w kluczu.
const DOMENY: &[&str] = &[
    "raw_", "agri_", "food_", "feed_", "fuel_", "mat_", "chem_", "part_", "pack_", "util_",
    "waste_", "cons_",
];

/// **Reguła 1** — każdy towar w katalogu ma źródło osiągalne z wydobycia, rolnictwa
/// albo importu. Fala A ma dać oba łańcuchy referencyjne w pełni.
#[test]
fn katalog_danych_domyka_sie() {
    let c = katalog();
    assert!(c.goods.len() >= 60, "katalog ma {} towarów", c.goods.len());
    assert!(
        c.recipes.len() >= 45,
        "katalog ma {} receptur",
        c.recipes.len()
    );
    c.validate_reachability().expect("graf produktów");
}

/// Test negatywny wymagany kryterium WP14 fazy M2: towar osiągalny **wyłącznie**
/// z zapasu startowego musi oblać walidację.
///
/// Zapas startowy nie jest źródłem i w tym kodzie nie jest nim nawet z nazwy — walidator
/// zna trzy punkty wejścia: wydobycie, rolnictwo i import. Towar, który nie ma żadnego
/// z nich, a jest w katalogu, to dokładnie ten przypadek: magazyn pozwoliłby mu raz
/// wystartować i nigdy się nie odtworzyć.
///
/// Beton nadaje się na dowód, bo jako jedyny półprodukt nie ma ceny importowej — betonu
/// się nie sprowadza z drugiego końca kraju, on wiąże po drodze.
#[test]
fn zapas_startowy_nie_jest_zrodlem() {
    let mut c = katalog();
    let beton = c.good_id("mat_concrete").expect("beton w katalogu");
    assert!(
        !c.good(beton).has_external_price(),
        "test stoi na tym, że betonu nie da się sprowadzić"
    );
    let wezel = c
        .recipes
        .iter()
        .position(|r| r.outputs.iter().any(|o| o.good == beton))
        .expect("receptura betoniarni");
    c.recipes.remove(wezel);
    // `RecipeId` jest indeksem, więc po usunięciu trzeba przenumerować — inaczej
    // `recipe(id)` sięgałby obok. To nie jest obejście testu, tylko ten sam kontrakt,
    // który obowiązuje przy ładowaniu.
    for (i, r) in c.recipes.iter_mut().enumerate() {
        r.id = RecipeId(i as u16);
    }
    let err = c
        .validate_reachability()
        .expect_err("beton bez wytwórni i bez importu musi oblać walidację");
    match err {
        CatalogError::Unreachable(v) => assert!(
            v.iter().any(|k| k == "mat_concrete"),
            "brakujący beton nie zgłoszony: {v:?}"
        ),
        other => panic!("zły błąd: {other}"),
    }
}

/// **Reguła 2** — każdy towar ma odbiorcę: recepturę, koszyk, eksport albo utylizację.
#[test]
fn kazdy_towar_ma_odbiorce() {
    katalog()
        .validate_consumers()
        .expect("ujście każdego towaru");
}

/// **Reguła 3** — bilans masy receptury jest liczony, a nie deklarowany.
/// `yield` nie jest polem właśnie dlatego, że rozjechałby się z masami.
#[test]
fn wytwarzanie_domyka_bilans_masy() {
    let c = katalog();
    for r in &c.recipes {
        if r.source != RecipeSource::Manufacturing {
            continue;
        }
        assert_eq!(
            r.input_mass().0,
            r.output_mass().0 + r.process_loss.0,
            "receptura {}",
            r.key
        );
    }
}

/// **Reguła 4** — spójność jednostek. Sypkie mają gęstość, sztukowe masę i objętość sztuki.
#[test]
fn jednostki_sa_spojne_z_postacia() {
    let c = katalog();
    for g in &c.goods {
        if g.form.is_bulk() {
            assert!(g.density_g_per_l > 0, "{}: brak gęstości", g.key);
        } else {
            assert!(g.unit_mass.0 > 0, "{}: brak masy sztuki", g.key);
            assert!(g.unit_volume.0 > 0, "{}: brak objętości sztuki", g.key);
        }
    }
}

/// **Reguła 5** — ostrzeżenia są deterministyczne. Raport walidatora ma się dać porównać
/// między przebiegami, inaczej nikt nie zauważy, że katalog stał się kruchszy.
#[test]
fn ostrzezenia_sa_stabilne() {
    let c = katalog();
    assert_eq!(c.warnings(), c.warnings());
    assert!(
        c.warnings().windows(2).all(|w| {
            (w[0].kind as u8, &w[0].subject, &w[0].context)
                <= (w[1].kind as u8, &w[1].subject, &w[1].context)
        }),
        "ostrzeżenia nieposortowane"
    );
}

#[test]
fn identyfikatory_sa_alfabetyczne_i_stabilne() {
    let c = katalog();
    assert!(c.goods.windows(2).all(|w| w[0].key < w[1].key));
    assert!(c.recipes.windows(2).all(|w| w[0].key < w[1].key));
    assert!(c.categories.windows(2).all(|w| w[0].key < w[1].key));
    for (i, g) in c.goods.iter().enumerate() {
        assert_eq!(g.id.0 as usize, i, "{}: id rozjechane z indeksem", g.key);
        assert_eq!(c.good_id(&g.key), Some(g.id));
        assert_eq!(c.key_of(g.id), &*g.key);
    }
}

/// Konwencja klucza jest kontraktem zapisu gry, więc pilnuje jej test, a nie zwyczaj.
#[test]
fn klucze_trzymaja_konwencje() {
    let c = katalog();
    for g in &c.goods {
        assert!(
            DOMENY.iter().any(|d| g.key.starts_with(d)),
            "`{}` nie zaczyna się od znanej domeny",
            g.key
        );
        assert!(
            g.key
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
            "`{}`: klucz poza [a-z0-9_]",
            g.key
        );
    }
}

/// Fala A ma dać **oba łańcuchy referencyjne w pełni** — to jest kryterium, przy którym
/// testy E2E fazy w ogóle mają czym biegać. Liczby z M6 §7.1 i §7.2, masy z tolerancją 0 g.
#[test]
fn lancuch_chleba_zgadza_sie_co_do_grama() {
    let c = katalog();

    let mlyn = c
        .recipe_id("milling_wheat_t550")
        .map(|r| c.recipe(r))
        .expect("receptura przemiału");
    assert_eq!(mlyn.input_mass().0, 1_000_000);
    assert_eq!(wyjscie(c.good_id("food_flour_t550"), mlyn), 760_000);
    assert_eq!(wyjscie(c.good_id("feed_bran"), mlyn), 220_000);
    assert_eq!(wyjscie(c.good_id("waste_grain_screenings"), mlyn), 20_000);
    assert_eq!(mlyn.output_mass().0, 1_000_000);
    assert!(mlyn.quality.cap_by_worst_input, "mąka bez sufitu wejścia");

    let piekarnia = c
        .recipe_id("bakery_bread_wheat")
        .map(|r| c.recipe(r))
        .expect("receptura wypieku");
    assert_eq!(piekarnia.input_mass().0, 165_800);
    assert_eq!(wyjscie(c.good_id("food_bread_wheat"), piekarnia), 158_000);
    assert_eq!(piekarnia.process_loss.0, 7_800);
    assert_eq!(piekarnia.loss_kind, LossKind::Evaporation);

    let chleb = c.good_id("food_bread_wheat").expect("chleb");
    assert_eq!(c.good(chleb).shelf_life_minutes, Some(2_880));
}

#[test]
fn lancuch_paliwa_zgadza_sie_co_do_grama() {
    let c = katalog();
    let r = c
        .recipe_id("refinery_crude_fractionation")
        .map(|r| c.recipe(r))
        .expect("receptura frakcjonowania");
    assert_eq!(r.input_mass().0, 1_000_000);
    for (klucz, masa) in [
        ("fuel_petrol_95", 260_000),
        ("fuel_diesel_b7", 340_000),
        ("fuel_lpg", 45_000),
        ("fuel_heating_oil", 160_000),
        ("mat_bitumen", 95_000),
        ("chem_lubricant_base", 40_000),
        ("fuel_refinery_gas", 42_000),
        ("waste_refinery_residue", 18_000),
    ] {
        assert_eq!(wyjscie(c.good_id(klucz), r), masa, "wyjście {klucz}");
    }
    assert_eq!(r.output_mass().0, 1_000_000);

    // Alokacja wg masy dałaby asfalt droższy od benzyny — absurd, który wywraca ceny
    // w całym łańcuchu w dół. Przy produkcji łącznej podział jest obowiązkowo wartościowy.
    assert_eq!(
        r.cost_allocation,
        magnat_supply::CostAllocation::ByMarketValue
    );
    let gaz = c.good_id("fuel_refinery_gas").expect("gaz opałowy");
    assert_eq!(
        r.outputs.iter().find(|o| o.good == gaz).map(|o| o.kind),
        Some(OutputKind::SelfConsumed),
        "gaz rafineryjny spala się na miejscu i nie tworzy partii"
    );

    // Ślad paliwa ma dojść do złoża: ropa jest wejściem z wydobycia, nie z importu.
    let szyb = c
        .recipes
        .iter()
        .find(|x| matches!(x.source, RecipeSource::Extraction(_)))
        .expect("choć jedna receptura wydobywcza");
    assert!(szyb.source.is_entry());
}

fn wyjscie(g: Option<magnat_core::GoodId>, r: &magnat_supply::Recipe) -> i64 {
    let g = g.expect("towar w katalogu");
    r.outputs
        .iter()
        .find(|o| o.good == g)
        .map_or(0, |o| o.mass.0)
}
