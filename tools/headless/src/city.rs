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

// ── M8d: usługi publiczne, urzędy i egzekucja ────────────────────────────────────

/// Co most postawił po stronie usług — liczby do raportu scenariusza.
pub struct ServicesSetup {
    pub services: usize,
    pub offices: usize,
    pub staff: u32,
    /// Ile rodzajów usługi ma w tym mieście choć jedną placówkę. Mniej niż osiem
    /// znaczy, że któryś rodzaj jest martwy — i to jest informacja do raportu,
    /// a nie stan do przemilczenia (`R2`).
    pub live_kinds: usize,
    pub emission_limit_g_per_min: i64,
}

/// Odwzorowanie archetypu budynku publicznego na rodzaj usługi.
///
/// Tabela jest **tutaj**, a nie w danych, z tego samego powodu co odwzorowanie
/// usługi na kierunek wydatku: przychodnia zapisana jako szkoła nie byłaby innym
/// balansem, tylko innym modelem. `None` znaczy „to nie jest usługa publiczna",
/// i tak jest dla działki ogrodniczej — budynek istnieje, usługi nie ma.
#[must_use]
fn rodzaj_uslugi(key: &str) -> Option<magnat_core::ServiceKind> {
    use magnat_core::ServiceKind as S;
    Some(match key {
        "school" | "kindergarten" | "library" => S::School,
        "clinic" => S::Clinic,
        "hospital" => S::Hospital,
        "police_station" => S::Police,
        "fire_station" => S::Fire,
        "town_hall" | "public_office" => S::Office,
        "museum" | "park_service" | "sports_ground" => S::Park,
        _ => return None,
    })
}

/// Dokłada do stojącego miasta usługi publiczne, urzędy i egzekucję (M8d).
///
/// Osobne wywołanie od [`setup`], a nie jego część, bo to jest **inna warstwa
/// i inny warunek wejścia**: podatki potrzebują rynku, a usługi — populacji
/// (obsada placówek to komponent `Employment` mieszkańców) i kalibracji
/// z `data/tuning/city.ron`.
///
/// # Errors
/// Zwraca błąd, gdy nie da się wczytać `data/tuning/city.ron` albo
/// `data/tuning/supply.ron` (próg emisji ma jedno źródło, `K-35`).
pub fn setup_services(
    world: &mut World,
    city_data: &CityData,
) -> Result<ServicesSetup, Box<dyn Error>> {
    let tuning = magnat_city::CityTuning::load_default()?;
    let supply = magnat_supply::Tuning::load_default()?;
    let limit =
        supply.emission_ref_pm_g_per_min * i64::from(tuning.agencies.emission_permille) / 1_000;

    let dzielnic = city_data.districts.districts.len().max(1);
    let mut placowki: Vec<magnat_city::PublicService> = Vec::new();
    for (i, s) in city_data.sites.sites.iter().enumerate() {
        let arch = city_data.site_catalog.get(s.archetype);
        let Some(kind) = rodzaj_uslugi(arch.key()) else {
            continue;
        };
        let etaty = s.workplaces.end.saturating_sub(s.workplaces.start);
        placowki.push(magnat_city::PublicService {
            kind,
            site: crate::plants::site_id(i),
            district: magnat_core::DistrictId(dzielnica_budynku(city_data, s.building.0.index())),
            // Pojemność bierze się z normatywu „jedna placówka na tylu mieszkańców"
            // z `data/buildings/public.ron` — czyli z liczby, którą generator już
            // miał, żeby wiedzieć, ile ich postawić. Druga liczba obok tamtej
            // rozjechałaby się przy pierwszej zmianie normatywu.
            capacity: arch.spec.per_pop.unwrap_or(2_000),
            staff_target: etaty,
            staff: 0,
            funding_per_month: Money::ZERO,
            condition: magnat_core::Q::new(80),
            quality: magnat_core::Q::MIN,
            utilization_bps: 0,
            served: 0,
        });
    }
    placowki.sort_by_key(|p| p.site.0.index());

    let urzedy: Vec<magnat_city::PermitOffice> = placowki
        .iter()
        .filter(|p| p.kind == magnat_core::ServiceKind::Office)
        .map(|p| magnat_city::PermitOffice::new(p.site, p.district))
        .collect();

    let mut zywe = [false; magnat_core::SERVICE_KIND_COUNT];
    for p in &placowki {
        zywe[p.kind.as_index()] = true;
    }
    let raport = ServicesSetup {
        services: placowki.len(),
        offices: urzedy.len(),
        staff: placowki.iter().map(|p| p.staff_target).sum(),
        live_kinds: zywe.iter().filter(|b| **b).count(),
        emission_limit_g_per_min: limit,
    };

    let Some(miasto) = world.get_resource_mut::<City>() else {
        return Err("świat bez miasta — `city::setup` musi stać przed usługami".into());
    };
    miasto.services = magnat_city::PublicServices::new(placowki, dzielnic);
    miasto.permits = magnat_city::PermitRegistry::new(urzedy);
    // Inspektorów przepisuje co miesiąc krok miasta z obsady urzędów; jeden na start,
    // żeby pierwsza doba nie była dobą bez żadnego urzędu.
    miasto.enforcement = magnat_city::Enforcement::new(1, limit);
    miasto.tuning = Arc::new(magnat_city::TuningRef(Some(tuning)));
    magnat_city::step::register_coverage(world, dzielnic);
    Ok(raport)
}

/// Dzielnica budynku — przez parcelę, bo indeksu przestrzennego dzielnic nie ma.
fn dzielnica_budynku(city: &CityData, building: u32) -> u16 {
    city.buildings
        .buildings
        .get(building as usize)
        .and_then(|b| city.parcels.parcels.get(b.parcel.0.index() as usize))
        .map_or(0, |p| p.district.0)
}
