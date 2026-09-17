//! Most „miasto jako aktor": budżet, kodeks podatkowy i kataster w świecie,
//! który ma już gospodarkę i firmy (M8a WP1, WP2).
//!
//! # Po co to jest osobno
//!
//! `full::setup` stawia gospodarkę i warstwę firm. Ten most dokłada **stronę
//! publiczną**, i dokłada ją osobnym wywołaniem z tego samego powodu, dla którego
//! `m7labor` stał obok `m5shop`, zanim M7f je zszył: włączenie siedmiu danin
//! zmienia świat, na którym skalibrowano bramki G1–G9, a kalibracja należy
//! do panelu balansatora, czyli do M8e.
//!
//! Do tego czasu miasto stawia scenariusz `m8miasto` i testy fazy, a `m7miasto`
//! chodzi dalej bez podatków i pokazuje dokładnie to, co pokazywało.
//!
//! # Czego tu nie ma
//!
//! Ani jednej reguły podatkowej. Wszystko, co liczy kwotę, mieszka w `sim/city`;
//! ten plik wyłącznie **wiąże** kodeks z katalogiem towarów, kataster z parcelami
//! i budżet z kontem w księgach.

use std::error::Error;
use std::sync::Arc;

use magnat_city::{BudgetPolicy, CadastreEntry, City, TaxCode};
use magnat_core::Money;
use magnat_economy::{AccountKind, AccountOwner, Books};
use magnat_ecs::World;
use magnat_firms::SiteTypeCatalog;
use magnat_world::CityData;

/// Co most postawił — liczby do raportu scenariusza.
pub struct CitySetup {
    /// Ile zakładów ma wpis w katastrze (czyli płaci podatek od nieruchomości).
    pub cadastre: usize,
    /// Suma wartości katastralnej — podstawa rocznego podatku od nieruchomości.
    pub cadastral_value: Money,
    /// Ile rodzajów zakładu jest w tym mieście objętych koncesją.
    pub licensed_types: usize,
    /// Ile towarów katalogu ma niezerową stawkę VAT i ile akcyzę.
    ///
    /// W raporcie, bo danina bez ani jednego obłożonego towaru przechodzi **każdy**
    /// test domknięcia i wygląda tak samo jak danina, której nikt nie zapłacił
    /// (`R2` — martwy hazard, ta sama klasa błędu).
    pub vat_goods: usize,
    pub excise_goods: usize,
    /// Ile mediów jest obłożonych akcyzą od energii (M8b). Zero znaczy, że ta
    /// ścieżka jest martwa — i to jest informacja do raportu, a nie stan
    /// do przemilczenia. Ten sam powód, dla którego obok stoi `excise_goods`.
    pub excise_services: usize,
}

/// Stawia miasto jako aktora fiskalnego w gotowym świecie.
///
/// Kolejność jest wymuszona: **rynek musi już stać**, bo silnik podatkowy wchodzi
/// w jego miejsce na `NoTax`, a kataster wiąże zakłady kluczem przesuniętym
/// o `SITE_KEY_BASE` (`K-46`) — tym samym, który nadał most detaliczny.
///
/// # Errors
/// Zwraca błąd, gdy `data/city/tax.ron` albo `data/city/budget.ron` nie da się
/// wczytać, albo gdy któraś domena klucza towaru nie ma klasy VAT.
pub fn setup(world: &mut World, city_data: &CityData) -> Result<CitySetup, Box<dyn Error>> {
    let code = TaxCode::load_default()?;
    // Katalog towarów bierze się **z łańcucha dostaw stojącego w świecie**, a nie
    // z drugiego wczytania pliku. `GoodId` jest indeksem w katalogu **tej epoki**,
    // a nie liczbą globalną: miasto 1990 dostaje koszyk `transition`, miasto 2020
    // `contemporary`, i ta sama klasa VAT wypada w nich pod innym numerem. Drugie
    // wczytanie dałoby stawkę chleba nałożoną na ropę i nikt by tego nie zauważył,
    // bo obie liczby są stawkami i obie się sumują.
    let goods = world
        .get_resource::<magnat_supply::ChainHandle>()
        .map(|c| std::sync::Arc::clone(&c.cat))
        .ok_or("świat bez łańcucha dostaw — nie ma katalogu, do którego przypisać stawki")?;
    let rates = code.resolve(&goods)?;
    let policy = BudgetPolicy::load_default()?;

    // Konto miasta w księgach. Bez debetu: miasto, które wchodzi na minus bez
    // emisji obligacji, omijałoby cały mechanizm domykania deficytu.
    let konto = world
        .get_resource_mut::<Books>()
        .map(|b| b.open_account(AccountOwner::City, AccountKind::Current, None, Money::ZERO))
        .ok_or("świat bez ksiąg — miasto nie ma gdzie trzymać budżetu")?;

    let mut miasto = City::new(Arc::new(code), Arc::new(rates), policy, konto);

    miasto.cadastre = cadastre_from_city(city_data);
    let wartosc = Money(miasto.cadastre.iter().map(|c| c.value.get()).sum());
    if let Some(types) = world.get_resource::<SiteTypeCatalog>() {
        miasto.licenses = magnat_city::licenses_from_catalog(&miasto.code, types);
    }
    let raport = CitySetup {
        cadastre: miasto.cadastre.len(),
        cadastral_value: wartosc,
        licensed_types: miasto.licenses.len(),
        vat_goods: miasto.rates.vat_goods(),
        excise_goods: miasto.rates.excise_goods(),
        excise_services: miasto.code.excise_services(),
    };
    magnat_city::register_city(world, miasto);
    Ok(raport)
}

/// Kataster: zakład i wartość katastralna parceli, na której stoi.
///
/// **Płatnikiem jest zakład, nie właściciel gruntu** — i to jest świadome
/// uproszczenie M8a z nazwanym sufitem. Generator oddaje całą mieszkaniówkę
/// i wszystkie instytucje **miastu** (`ParcelOwner::City`), a miasto nie opodatkowuje
/// samo siebie; realnym właścicielem prywatnym jest w Etapie 7 wyłącznie firma
/// stojąca na parceli przemysłowej albo handlowej. Podatek od mieszkania czeka
/// więc na rynek nieruchomości, którego w tej fazie nie ma i który jej nie należy.
///
/// `ponytail:` sufit — podstawą jest **sama ziemia** (`land_value_per_m2 × area_m2`),
/// bez wartości budynku. Droga wyjścia: dołożyć wycenę bryły z `BuildingSet`,
/// kiedy M9 pokaże graczowi kartę nieruchomości i różnica zacznie być widoczna.
///
/// Funkcja stoi **tutaj, a nie w `sim/city`**, i to jest jedyne miejsce, w którym
/// może stać: kataster wiąże parcelę generatora (`sim/world`) z zakładem gospodarki,
/// czyli dwie strony, których żadna nie widzi drugiej. Most widzi obie — po to jest.
#[must_use]
pub fn cadastre_from_city(city: &CityData) -> Vec<CadastreEntry> {
    let mut out = Vec::new();
    for (i, s) in city.sites.sites.iter().enumerate() {
        let idx = s.parcel.0.index() as usize;
        let Some(p) = city.parcels.parcels.get(idx) else {
            continue;
        };
        let wartosc = p
            .land_value_per_m2
            .checked_mul_int(i64::from(p.area_m2))
            .unwrap_or(Money::ZERO);
        if wartosc.get() <= 0 {
            continue;
        }
        out.push(CadastreEntry {
            site: crate::plants::site_id(i),
            value: wartosc,
        });
    }
    // Po `SiteId` rosnąco — kolejność naliczania jest kolejnością identyfikatorów
    // i nie zależy od tego, w jakiej kolejności generator stawiał zakłady (00 §3.2).
    out.sort_by_key(|c| c.site.0.index());
    out
}
