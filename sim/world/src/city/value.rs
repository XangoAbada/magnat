//! Statyczna wycena gruntu (M2 §5.7). **W M2d realizowany jest `pass_1`** (WP15a);
//! `pass_2` dokłada M2e (WP15b).
//!
//! Trzy przebiegi o ustalonym zakresie, bez iteracji do zbieżności — to jest mitygacja
//! ryzyka R3: cykl „wycena ↔ strefowanie ↔ budynki ↔ firmy" rozcina się kolejnością,
//! nie punktem stałym, bo pętla zbieżnościowa nie ma deterministycznego czasu trwania.
//!
//! | Przebieg | Kiedy | Gdzie | Odbiorca |
//! |---|---|---|---|
//! | `pass_0` | przed Etapem 4 | M2c, jako punktacja stref | przydział stref |
//! | `pass_1` | po Etapie 5 | **tutaj** | dobór gramatyki, liczba kondygnacji, `rent_hint` |
//! | `pass_2` | po Etapie 7 | M2e | UI, karta inspekcji, wejście dla M5/M8/M10 |
//!
//! Arytmetyka: czynniki w `f64` (00 §2 — funkcje użyteczności wolno), **sumowane
//! w stałej kolejności enumeracji** [`LandValueFactor`], jedno zaokrąglenie na końcu
//! przez `div_round_half_up`. Nigdy nie kumulujemy `Money` z floata w pętli.

use super::blocks::BlockSet;
use super::districts::{DistrictKind, DistrictSet};
use super::parcels::ParcelSet;
use super::poly;
use super::road::{PolyArena, RoadClass, RoadNetwork};
use super::zoning::{CityFields, ResDensity, ZoneKind};
use magnat_core::Money;
use magnat_spatial::{GridSpec, ScalarField, Vec2};

/// Czynnik wyceny — wymóg wyjaśnialności (00 §7). Kolejność wariantów jest kolejnością
/// mnożenia i **nie wolno jej zmieniać**: zmiana przestawia zaokrąglenia w każdym świecie.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum LandValueFactor {
    ZoneBase,
    JobAccess,
    RetailAccess,
    TransitAccess,
    Amenity,
    Noise,
    Pollution,
    FloodRisk,
    DistrictPrestige,
    Epoch,
    Frontage,
    Shape,
}

impl LandValueFactor {
    pub const ALL: [LandValueFactor; 12] = [
        LandValueFactor::ZoneBase,
        LandValueFactor::JobAccess,
        LandValueFactor::RetailAccess,
        LandValueFactor::TransitAccess,
        LandValueFactor::Amenity,
        LandValueFactor::Noise,
        LandValueFactor::Pollution,
        LandValueFactor::FloodRisk,
        LandValueFactor::DistrictPrestige,
        LandValueFactor::Epoch,
        LandValueFactor::Frontage,
        LandValueFactor::Shape,
    ];

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            LandValueFactor::ZoneBase => "baza strefy",
            LandValueFactor::JobAccess => "dostęp do pracy",
            LandValueFactor::RetailAccess => "dostęp do handlu",
            LandValueFactor::TransitAccess => "dostęp komunikacyjny",
            LandValueFactor::Amenity => "otoczenie",
            LandValueFactor::Noise => "hałas",
            LandValueFactor::Pollution => "zanieczyszczenie",
            LandValueFactor::FloodRisk => "ryzyko powodzi",
            LandValueFactor::DistrictPrestige => "prestiż dzielnicy",
            LandValueFactor::Epoch => "wiek zabudowy",
            LandValueFactor::Frontage => "front działki",
            LandValueFactor::Shape => "kształt i wielkość",
        }
    }
}

/// Rozbicie wyceny na czynniki. `factors` niesie wpływ **w punktach procentowych**
/// względem bazy, żeby kartę inspekcji dało się posortować po wielkości wpływu.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct LandValueBreakdown {
    pub base: Money,
    pub factors: [(LandValueFactor, i16); 12],
    pub result: Money,
}

impl Default for LandValueBreakdown {
    fn default() -> LandValueBreakdown {
        LandValueBreakdown {
            base: Money(0),
            factors: LandValueFactor::ALL.map(|f| (f, 0)),
            result: Money(0),
        }
    }
}

/// Bazowa wartość gruntu w strefie, w groszach za m². Skala epoki startowej 1990;
/// wartość bezwzględna jest kalibrowana dopiero w M5 (balansator), tutaj liczą się
/// **proporcje** między strefami, bo z nich wynika dobór gramatyki.
#[must_use]
pub const fn zone_base(z: ZoneKind) -> Money {
    Money(match z {
        ZoneKind::Residential(ResDensity::R1) => 12_000,
        ZoneKind::Residential(ResDensity::R2) => 16_000,
        ZoneKind::Residential(ResDensity::R3) => 28_000,
        ZoneKind::Residential(ResDensity::R4) => 19_000,
        ZoneKind::Residential(ResDensity::R5) => 34_000,
        ZoneKind::Commercial => 46_000,
        ZoneKind::Office => 54_000,
        ZoneKind::IndustryLight => 9_000,
        ZoneKind::IndustryHeavy => 6_000,
        ZoneKind::Logistics => 7_000,
        ZoneKind::Agriculture => 900,
        ZoneKind::Institutional => 20_000,
        ZoneKind::Green => 2_000,
        ZoneKind::Extraction => 3_000,
        // Nie da się tego kupić, ale zero łamałoby kryterium „brak wartości ≤ 0".
        ZoneKind::Water | ZoneKind::Undevelopable => 100,
    })
}

/// Mnożnik za klasę drogi frontowej. Adres przy arterii jest wart więcej niż przy sięgaczu
/// — ale przy autostradzie już mniej, bo to hałas bez dostępu (zjazd jest gdzie indziej).
fn frontage_class_mult(c: RoadClass) -> f64 {
    match c {
        RoadClass::Highway => 0.85,
        RoadClass::Arterial => 1.12,
        RoadClass::Collector => 1.08,
        RoadClass::Local => 1.0,
        RoadClass::Service => 0.94,
        RoadClass::Pedestrian => 1.06,
        RoadClass::RailFreight | RoadClass::RailPassenger => 0.8,
    }
}

/// Wejście wyceny — wszystko, co przebiegi wyceny czytają, w jednym miejscu.
///
/// `access` jest `None` w `pass_1` i `Some` w `pass_2`: cztery czynniki dostępności
/// nie istnieją przed Etapem 7, bo nie ma jeszcze ani miejsc pracy, ani sklepów.
/// Jedno pole zamiast dwóch struktur kontekstu, bo poza nim `pass_2` czyta dokładnie
/// to samo co `pass_1` (korekta F4).
pub struct ValueCtx<'a> {
    pub fields: &'a CityFields,
    pub roads: &'a RoadNetwork,
    pub geom: &'a PolyArena,
    pub blocks: &'a BlockSet,
    pub districts: &'a DistrictSet,
    /// Liczba pierścieni epok — do normalizacji `epoch_ring`.
    pub rings: u8,
    /// Pola dostępności po Etapie 7 (WP15b). `None` = przebieg pierwszy.
    pub access: Option<&'a AccessFields>,
}

/// Wycena jednej parceli, przebieg 1. Zwraca wartość **i** rozbicie — rozbicie nie jest
/// przechowywane (42 tys. parcel × 40 B), tylko przeliczane na żądanie w karcie inspekcji.
#[must_use]
pub fn pass_1_parcel(
    ctx: &ValueCtx,
    parcels: &ParcelSet,
    idx: usize,
) -> (Money, LandValueBreakdown) {
    let p = &parcels.parcels[idx];
    let base = zone_base(p.zone);
    let obrys = ctx.geom.get(p.poly);
    let srodek = poly::centroid(obrys);

    let mut mnozniki = [1.0f64; 12];
    let m = |f: LandValueFactor| f as usize;

    // ── dostęp komunikacyjny: odległość od centrum po drogach + bliskość jakiejkolwiek
    //    drogi + klasa drogi frontowej. `d_center` jest znormalizowane: 1 = najdalej.
    let d_center = f64::from(ctx.fields.d_center.sample(srodek)).clamp(0.0, 1.0);
    let access = f64::from(ctx.fields.access_road.sample(srodek)).clamp(0.0, 1.0);
    let klasa = if p.frontage.is_none() {
        // Podwórko bez frontu: dostęp wyłącznie przez sąsiada.
        0.7
    } else {
        frontage_class_mult(ctx.roads.segments[p.frontage.seg.0 as usize].class)
    };
    mnozniki[m(LandValueFactor::TransitAccess)] =
        (1.9 - 1.2 * d_center) * (1.0 - 0.25 * access) * klasa;

    // ── otoczenie i uciążliwości
    mnozniki[m(LandValueFactor::Amenity)] =
        1.0 + 0.45 * f64::from(ctx.fields.amenity.sample(srodek));
    mnozniki[m(LandValueFactor::Noise)] =
        1.0 / (1.0 + 0.55 * f64::from(ctx.fields.noise.sample(srodek)));

    // ── ryzyko powodzi: nachylenie jest tu jedynym polem dostępnym bez terenu;
    //    właściwe `flood_risk` wchodzi razem z `pass_2` (M2e).
    let slope = f64::from(ctx.fields.slope.sample(srodek)).clamp(0.0, 1.0);
    mnozniki[m(LandValueFactor::FloodRisk)] = 1.0 / (1.0 + 1.6 * slope);

    // ── wiek zabudowy: pierścień najstarszy i najnowszy są warte więcej niż środek
    //    (starówka ma prestiż, przedmieście nowość, blokowisko ani jedno, ani drugie).
    let ring = f64::from(ctx.blocks.blocks[p.block.0 as usize].epoch_ring);
    let n = f64::from(ctx.rings.max(1));
    let t = if n > 1.0 { ring / (n - 1.0) } else { 0.5 };
    mnozniki[m(LandValueFactor::Epoch)] = 0.92 + 0.55 * (t - 0.5) * (t - 0.5) * 4.0 * 0.5;

    // ── front: długość pierzei w metrach, znormalizowana wobec typowej działki strefy.
    let front_m = if p.frontage.is_none() {
        0.0
    } else {
        let seg = &ctx.roads.segments[p.frontage.seg.0 as usize];
        f64::from((p.frontage.t1 - p.frontage.t0).abs()) * f64::from(seg.length_dm) / 10.0
    };
    let typowy = f64::from(super::parcels::zone_spec(p.zone).front.1);
    mnozniki[m(LandValueFactor::Frontage)] = if typowy > 0.0 {
        (0.82 + 0.30 * (front_m / typowy).min(2.0)).clamp(0.7, 1.35)
    } else {
        1.0
    };

    // ── kształt: smukła działka jest mniej warta od zwartej o tym samym polu,
    //    bo na trójkącie o proporcji 1:6 nie da się nic postawić.
    mnozniki[m(LandValueFactor::Shape)] = zwartosc(obrys, f64::from(p.area_m2));

    // ── czynniki `pass_2` — istnieją dopiero po Etapie 7 (WP15b) ────────────────────
    if let Some(a) = ctx.access {
        mnozniki[m(LandValueFactor::JobAccess)] = 1.0 + 0.55 * f64::from(a.jobs.sample(srodek));
        mnozniki[m(LandValueFactor::RetailAccess)] =
            1.0 + 0.30 * f64::from(a.retail.sample(srodek));
        // Usługi (szkoła, przychodnia, urząd, park z obsługą) wchodzą do `Amenity`,
        // a nie dostają trzynastego wariantu: kolejność wariantów `LandValueFactor`
        // jest kolejnością mnożenia i nie wolno jej zmieniać (korekta F4, I-5).
        mnozniki[m(LandValueFactor::Amenity)] *= 1.0 + 0.22 * f64::from(a.services.sample(srodek));
        mnozniki[m(LandValueFactor::Pollution)] =
            1.0 / (1.0 + 1.30 * f64::from(a.pollution.sample(srodek)));
        let d = ctx.districts.districts.get(p.district.0 as usize);
        mnozniki[m(LandValueFactor::DistrictPrestige)] = d.map_or(1.0, |d| {
            f64::from(d.kind.prestige())
                * (0.86
                    + 0.24 * f64::from(d.reputation.get()) / 100.0
                    + 0.05 * f64::from(d.income_tier.min(4))
                    - 0.18 * f64::from(d.crime.get()) / 100.0)
        });
    }

    // Mnożenie w **stałej kolejności enumeracji** — inaczej zaokrąglenie zależałoby
    // od kolejności iteracji (00 §2).
    let mut mult = 1.0f64;
    for f in LandValueFactor::ALL {
        mult *= mnozniki[f as usize];
    }
    // Jedno zaokrąglenie na końcu: mnożymy licznik w ustalonej skali i dzielimy.
    const SKALA: i64 = 4096;
    let num = (mult * SKALA as f64).round().clamp(1.0, 1.0e12) as i64;
    let result = Money(base.0.saturating_mul(num)).div_round_half_up(SKALA);
    // Zero jest błędem kryterium WP15a („każda parcela ma wartość > 0"), a nie wynikiem.
    let result = Money(result.0.max(1));

    let mut factors = LandValueFactor::ALL.map(|f| (f, 0i16));
    for (i, f) in LandValueFactor::ALL.into_iter().enumerate() {
        factors[i] = (f, ((mnozniki[f as usize] - 1.0) * 100.0).round() as i16);
    }
    (
        result,
        LandValueBreakdown {
            base,
            factors,
            result,
        },
    )
}

/// Zwartość działki: pole rzeczywiste do pola prostokąta opisanego na niej.
/// 1 = prostokąt, 0,4 = wrzeciono. Wchodzi jako mnożnik 0,75..=1,05.
fn zwartosc(obrys: &[Vec2], area_m2: f64) -> f64 {
    if obrys.len() < 3 || area_m2 <= 0.0 {
        return 0.75;
    }
    let obb = poly::min_area_obb(obrys);
    let pole_obb = f64::from(obb.area());
    if pole_obb <= 0.0 {
        return 0.75;
    }
    let r = (area_m2 / pole_obb).clamp(0.0, 1.0);
    (0.70 + 0.35 * r).clamp(0.70, 1.05)
}

// ── Pola dostępności (`pass_2`, WP15b) ───────────────────────────────────────────────

/// Cztery pola, których `pass_1` mieć nie mógł, bo powstają dopiero z Etapu 7.
///
/// Wzór z §5.7 mówi `A_job = 1 + k_job · Σ_b jobs_b · exp(−t(parcel, b) / 12 min)`.
/// Liczone dosłownie — suma po wszystkich budynkach dla każdej parceli — to 14 tys.
/// × 42 tys. działań na pole. Zamiast tego rasteryzujemy źródła na siatkę 64 m
/// i rozmywamy ją **wykładniczo, separowalnie**: ten sam kształt jądra, koszt liniowy
/// względem liczby komórek, a nie iloczynu. Odległość jest euklidesowa, nie po drogach —
/// i to jest świadomy skrót, bo `t(parcel, b)` po sieci wymagałoby Dijkstry z każdego
/// budynku. Poprawienie go należy do M4, który i tak buduje macierz czasów przejazdu.
#[derive(Clone, Debug)]
pub struct AccessFields {
    pub jobs: ScalarField,
    pub retail: ScalarField,
    pub services: ScalarField,
    /// Uciążliwość: przemysł ciężki i wydobycie (kara z kryterium WP15b).
    pub pollution: ScalarField,
}

/// Bok komórki pól dostępności. 64 m to kompromis: drobniej niż kwartał, grubiej niż
/// działka — a wycena i tak próbkuje pole dwuliniowo.
const ACCESS_CELL_M: u16 = 64;

/// Połowiczny zanik dostępności, w metrach. Praca dalej niż dwa kilometry przestaje
/// podnosić wartość działki, sklep — dalej niż kilometr.
const HL_JOBS_M: f32 = 700.0;
const HL_RETAIL_M: f32 = 400.0;
const HL_SERVICES_M: f32 = 550.0;
const HL_POLLUTION_M: f32 = 180.0;

/// Skale normalizujące: tyle stanowisk (odpowiednio m² uciążliwości) w promieniu
/// jednego okresu zaniku daje wartość 1 pola. Liczby są kalibracyjne, nie fizyczne.
const NORM_JOBS: f32 = 24_000.0;
const NORM_RETAIL: f32 = 3_000.0;
const NORM_SERVICES: f32 = 7_000.0;
const NORM_POLLUTION: f32 = 110_000.0;

#[must_use]
pub fn build_access(
    plan: &super::CityPlan,
    geom: &PolyArena,
    parcels: &ParcelSet,
    buildings: &super::build::BuildingSet,
    sites: &super::sites::SiteSet,
    sc: &super::sites::SiteCatalog,
) -> AccessFields {
    use super::sites::SectorId;
    let map = plan.map_size_m() as f32;
    let spec = GridSpec::covering(
        magnat_spatial::Aabb2::new(Vec2::ZERO, Vec2::new(map, map)),
        ACCESS_CELL_M,
    );
    let n = spec.cell_count();
    let (mut jobs, mut retail, mut services) = (vec![0.0f32; n], vec![0.0f32; n], vec![0.0f32; n]);
    let mut pollution = vec![0.0f32; n];

    for (i, b) in buildings.buildings.iter().enumerate() {
        let c = Vec2::new(
            (b.aabb.min.x + b.aabb.max.x) * 0.5,
            (b.aabb.min.y + b.aabb.max.y) * 0.5,
        );
        let cell = spec.cell_of(c).0 as usize;
        let wp: f32 = buildings.units[b.units.start as usize..b.units.end as usize]
            .iter()
            .map(|u| u.workplaces.len() as f32)
            .sum();
        if wp <= 0.0 {
            continue;
        }
        jobs[cell] += wp / NORM_JOBS;
        match sites
            .site_of_building(super::build::building_id(i as u32))
            .map(|s| sc.get(s.archetype).sector())
        {
            Some(SectorId::Retail) => retail[cell] += wp / NORM_RETAIL,
            Some(SectorId::Services) => {
                retail[cell] += wp * 0.5 / NORM_RETAIL;
                services[cell] += wp * 0.5 / NORM_SERVICES;
            }
            Some(SectorId::Public | SectorId::Green) => services[cell] += wp / NORM_SERVICES,
            _ => {}
        }
    }
    // Uciążliwość bierze się z **powierzchni** działki, nie z liczby etatów: huta
    // szkodzi sąsiadom tym, czym jest, a nie tym, ilu ludzi zatrudnia.
    for p in &parcels.parcels {
        if !matches!(p.zone, ZoneKind::IndustryHeavy | ZoneKind::Extraction) {
            continue;
        }
        let c = super::poly::centroid(geom.get(p.poly));
        // Dolna granica amplitudy: **kotłownia przy płocie uwiera tak samo jak huta**,
        // tylko na mniejszym obszarze. Bez niej działka przemysłowa o 4 tys. m² — a takie
        // daje profil rolniczy i uniwersytecki, gdzie `IndustryHeavy` ma 1 % powierzchni —
        // nie obniżała sąsiadom wartości ani o grosz (korekta I-12).
        pollution[spec.cell_of(c).0 as usize] += (p.area_m2 as f32 / NORM_POLLUTION).max(0.35);
    }

    let (cols, rows) = (usize::from(spec.cols), usize::from(spec.rows));
    for (v, hl) in [
        (&mut jobs, HL_JOBS_M),
        (&mut retail, HL_RETAIL_M),
        (&mut services, HL_SERVICES_M),
        (&mut pollution, HL_POLLUTION_M),
    ] {
        rozmyj(v, cols, rows, hl / f32::from(ACCESS_CELL_M));
    }

    let pole = |v: &[f32]| {
        ScalarField::from_data(
            spec,
            v.iter()
                .map(|x| (x.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16)
                .collect(),
            1.0,
        )
    };
    AccessFields {
        jobs: pole(&jobs),
        retail: pole(&retail),
        services: pole(&services),
        pollution: pole(&pollution),
    }
}

/// Separowalne rozmycie wykładnicze filtrem IIR pierwszego rzędu, w obie strony.
///
/// Jądro jest dokładnie takie samo jak w `ScalarField::decay_from` (zanik 2^(−d/hl)),
/// ale koszt to O(komórek), a nie O(źródła × zasięg). Przy 14 tys. budynków i zasięgu
/// sześciu okresów zaniku ta różnica to sekundy, nie procenty.
fn rozmyj(v: &mut [f32], cols: usize, rows: usize, hl_cells: f32) {
    if hl_cells <= 0.0 || cols == 0 || rows == 0 {
        return;
    }
    let a = magnat_core::det_math::exp2(-1.0 / f64::from(hl_cells)) as f32;
    // **Bez normalizacji jądrem.** Pole ma być sumą wpływów („ile pracy jest w zasięgu"),
    // a nie średnią ważoną („jaka jest tu gęstość") — tak samo jak `ScalarField::decay_from`,
    // którego kształt odtwarzamy. Dzielenie przez całkę jądra rozmazywało 900 etatów
    // po tysiącu komórek i `A_job` wychodził zerem w całym mieście.
    let norm = 1.0f32;
    let mut bufor = vec![0.0f32; cols.max(rows)];

    // Wiersze, potem kolumny. Jądro dwuwymiarowe jest iloczynem jednowymiarowych,
    // więc rozdzielenie nie zmienia wyniku, a zamienia koszt kwadratowy na liniowy.
    for y in 0..rows {
        let od = y * cols;
        linia(&mut v[od..od + cols], 1, cols, a, norm, &mut bufor);
    }
    for x in 0..cols {
        linia(&mut v[x..], cols, rows, a, norm, &mut bufor);
    }
}

/// Jeden przebieg filtru po linii o zadanym kroku: w przód, w tył, suma bez podwójnego
/// liczenia komórki środkowej.
fn linia(v: &mut [f32], krok: usize, n: usize, a: f32, norm: f32, bufor: &mut [f32]) {
    for i in 0..n {
        bufor[i] = v[i * krok];
    }
    let mut acc = 0.0f32;
    for i in 0..n {
        acc = bufor[i] + a * acc;
        v[i * krok] = acc;
    }
    let mut back = 0.0f32;
    for i in (0..n).rev() {
        back = bufor[i] + a * back;
        v[i * krok] = (v[i * krok] + back - bufor[i]) * norm;
    }
}

/// `pass_1` dla całego miasta — zapisuje `Parcel.land_value_per_m2` (WP15a).
///
/// Sekwencyjnie i to jest świadome: wycena jednej parceli to kilkanaście próbek pól
/// i jedno OBB, czyli rzędy wielkości mniej niż derywacja gramatyki. Zrównoleglenie
/// kosztowałoby więcej uwagi niż oszczędza czasu.
pub fn pass_1(ctx: &ValueCtx, parcels: &mut ParcelSet) {
    for i in 0..parcels.parcels.len() {
        let (v, _) = pass_1_parcel(ctx, parcels, i);
        parcels.parcels[i].land_value_per_m2 = v;
    }
}

/// `pass_2` (WP15b): wycena po Etapie 7, z dostępem do pracy, handlu, usług i kary
/// za sąsiedztwo uciążliwe. Nadpisuje `Parcel.land_value_per_m2` i wypełnia
/// `District.avg_land_value`.
///
/// Nadpisuje, a nie koryguje: `pass_1` był wejściem doboru gramatyki i swoje zrobił,
/// a mnożenie jednego wyniku przez drugi kumulowałoby zaokrąglenia dwa razy.
pub fn pass_2(ctx: &ValueCtx, parcels: &mut ParcelSet) {
    debug_assert!(
        ctx.access.is_some(),
        "pass_2 bez pól dostępności jest pass_1 pod inną nazwą"
    );
    for i in 0..parcels.parcels.len() {
        let (v, _) = pass_1_parcel(ctx, parcels, i);
        parcels.parcels[i].land_value_per_m2 = v;
    }
}

/// Wycena parceli z rozbiciem na czynniki — kontrakt §6 fazy, wejście karty inspekcji.
///
/// **Korekta I-7 wobec §5.7.** Sygnatura z dokumentu fazy brzmi
/// `land_value_at(w: &World, p: ParcelId)`, czyli zakłada, że warstwa miejska mieszka
/// w ECS. Nie mieszka: M2 trzyma miasto w [`CityData`] — tablicowo, bo parcel jest
/// 42 tys. i żadna z nich nie jest odpytywana przekrojowo po archetypach. To jest ta
/// sama decyzja co `K-16` dla partii i ofert, tylko podjęta o fazę wcześniej. Zmienia
/// się adres argumentu, nie kontrakt: wynik i rozbicie są dokładnie te z §5.7.
///
/// Rozbicie **nie jest przechowywane** (42 tys. × 40 B) — liczy się je na żądanie.
#[must_use]
pub fn land_value_at(
    city: &super::CityData,
    p: magnat_core::ParcelId,
) -> (Money, LandValueBreakdown) {
    let ctx = ValueCtx {
        fields: &city.fields,
        roads: &city.roads,
        geom: &city.roads.geom,
        blocks: &city.blocks,
        districts: &city.districts,
        rings: city.zones.rings.len() as u8,
        access: Some(&city.access),
    };
    pass_1_parcel(&ctx, &city.parcels, p.0.index() as usize)
}

/// Wpisuje średnią wartość gruntu do dzielnic. Osobno od [`pass_2`], bo kontekst wyceny
/// trzyma `&DistrictSet` (prestiż jest czynnikiem) — a nie da się jednocześnie czytać
/// dzielnicy i do niej pisać.
pub fn set_district_averages(districts: &mut DistrictSet, parcels: &ParcelSet) {
    let srednie = district_average(districts, parcels);
    for (d, v) in districts.districts.iter_mut().zip(srednie) {
        d.avg_land_value = v;
    }
}

/// Średnia wartość gruntu per dzielnica — wejście `District.avg_land_value`
/// i testu „starówka droższa od przedmieścia".
#[must_use]
pub fn district_average(districts: &DistrictSet, parcels: &ParcelSet) -> Vec<Money> {
    district_average_filtered(districts, parcels, |_| true)
}

/// Średnia per dzielnica z filtrem stref.
#[must_use]
pub fn district_average_filtered(
    districts: &DistrictSet,
    parcels: &ParcelSet,
    filtr: impl Fn(ZoneKind) -> bool,
) -> Vec<Money> {
    let mut suma = vec![0i64; districts.districts.len()];
    let mut ile = vec![0i64; districts.districts.len()];
    for p in parcels.parcels.iter().filter(|p| filtr(p.zone)) {
        let d = p.district.0 as usize;
        if d < suma.len() {
            suma[d] += p.land_value_per_m2.0;
            ile[d] += 1;
        }
    }
    suma.iter()
        .zip(&ile)
        .map(|(s, n)| if *n > 0 { Money(*s / *n) } else { Money(0) })
        .collect()
}

/// Dzielnice „rdzenia": starówka i śródmieście.
///
/// Kryterium WP15a mówi `avg(OldTown) > avg(Suburb)`, ale `DistrictKind::OldTown`
/// dostaje wyłącznie dzielnica, której **dominującym** pierścieniem jest najstarsza
/// epoka — a ten pierścień ma z `data/epochs/` 3% powierzchni, więc w mieście
/// czterotysięcznym bywa rozsiany po kilku dzielnicach i żadnej nie przeważa.
/// Porównanie po samym `OldTown` wypadałoby wtedy „0 vs 0", czyli byłoby spełnione
/// tożsamościowo — a tego zakazuje `K-18` pkt 3. Stąd grupa: rdzeń wobec obrzeża.
pub const CORE_KINDS: [DistrictKind; 2] = [DistrictKind::OldTown, DistrictKind::InnerCity];
/// Dzielnice obrzeża.
pub const FRINGE_KINDS: [DistrictKind; 2] = [DistrictKind::Suburb, DistrictKind::Village];

/// Średnia wartość gruntu w dzielnicach wskazanych rodzajów — kryterium WP15a.
/// `Money(0)` znaczy „nie ma takich dzielnic", a nie „warte zero".
#[must_use]
pub fn average_by_kind(
    districts: &DistrictSet,
    parcels: &ParcelSet,
    kinds: &[DistrictKind],
) -> Money {
    average_by_kind_filtered(districts, parcels, kinds, |_| true)
}

/// To samo, ale z filtrem stref. Porównanie „rdzeń wobec obrzeża" **po samej
/// mieszkaniówce** jest uczciwsze od porównania wszystkiego: dzielnica peryferyjna
/// z centrum handlowym przy obwodnicy ma średnią podbitą strefą `Commercial`, a nie
/// wartością ziemi mieszkaniowej, o którą w tym porównaniu chodzi (korekta I-16).
#[must_use]
pub fn average_by_kind_filtered(
    districts: &DistrictSet,
    parcels: &ParcelSet,
    kinds: &[DistrictKind],
    filtr: impl Fn(ZoneKind) -> bool,
) -> Money {
    let srednie = district_average_filtered(districts, parcels, filtr);
    let mut suma = 0i64;
    let mut n = 0i64;
    for (i, d) in districts.districts.iter().enumerate() {
        if kinds.contains(&d.kind) && srednie[i].0 > 0 {
            suma += srednie[i].0;
            n += 1;
        }
    }
    if n > 0 {
        Money(suma / n)
    } else {
        Money(0)
    }
}
