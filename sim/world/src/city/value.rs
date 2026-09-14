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
use magnat_spatial::Vec2;

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

/// Wejście wyceny — wszystko, co `pass_1` czyta, w jednym miejscu.
pub struct ValueCtx<'a> {
    pub fields: &'a CityFields,
    pub roads: &'a RoadNetwork,
    pub geom: &'a PolyArena,
    pub blocks: &'a BlockSet,
    pub districts: &'a DistrictSet,
    /// Liczba pierścieni epok — do normalizacji `epoch_ring`.
    pub rings: u8,
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

/// Średnia wartość gruntu per dzielnica — wejście `District.avg_land_value`
/// i testu „starówka droższa od przedmieścia".
#[must_use]
pub fn district_average(districts: &DistrictSet, parcels: &ParcelSet) -> Vec<Money> {
    let mut suma = vec![0i64; districts.districts.len()];
    let mut ile = vec![0i64; districts.districts.len()];
    for p in &parcels.parcels {
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
    let srednie = district_average(districts, parcels);
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
