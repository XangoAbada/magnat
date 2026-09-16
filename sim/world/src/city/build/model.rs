//! Typy wyjściowe Etapu 6 (kontrakt M2 §6) i katalog ról z `data/jobs/`.
//!
//! Wydzielone z `city/build.rs` w R-WP8 bez zmiany zachowania.

use super::*;

// ── Typy wyjściowe (kontrakt M2 §6) ──────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EntranceKind {
    Main,
    Service,
    /// Rampa przeładunkowa — wymagana dla `Logistics` i `IndustryHeavy` (test T4).
    Ramp,
    Garage,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Entrance {
    pub pos: glam::Vec3,
    pub seg: SegmentId,
    pub t: f32,
    pub kind: EntranceKind,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnitKind {
    Dwelling { rooms: u8 },
    Retail,
    Office,
    Workshop,
    Storage,
    Common,
}

impl UnitKind {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            UnitKind::Dwelling { .. } => "mieszkanie",
            UnitKind::Retail => "handel",
            UnitKind::Office => "biuro",
            UnitKind::Workshop => "warsztat",
            UnitKind::Storage => "magazyn",
            UnitKind::Common => "część wspólna",
        }
    }

    #[must_use]
    pub const fn is_dwelling(self) -> bool {
        matches!(self, UnitKind::Dwelling { .. })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnitOccupant {
    Vacant,
    Household(magnat_core::HouseholdId),
    Site(SiteId),
}

/// Indeks w globalnej tablicy lokali (korekta D8). Nie `Entity`: lokali jest ~190 tys.,
/// nie są odpytywane przekrojowo po archetypach i żyją w ciągłym zakresie `Building.units`
/// — ta sama logika co `K-16` dla partii i ofert.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct UnitIdx(pub u32);

#[derive(Clone, PartialEq, Debug)]
pub struct Unit {
    pub building: BuildingId,
    /// Ujemne = kondygnacja podziemna.
    pub floor: i8,
    pub kind: UnitKind,
    pub area_m2: u16,
    /// W M2 zawsze `Vacant`; `Site(..)` dopisze M2e (Etap 7).
    pub occupant: UnitOccupant,
    /// Miesięcznie; podpowiedź startowa dla M5, nie cena.
    pub rent_hint: Money,
    pub workplaces: Range<u32>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShiftId {
    I,
    II,
    III,
    Flexible,
}

/// Widełki, nie pensja. Pensje emergentne to M7 — M2 daje przedział startowy, żeby M3
/// (Etap 8) mógł dopasować dochody rodzin do wartości mieszkań. Grosze, miesięcznie, brutto.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct WageBand {
    pub min: Money,
    pub median: Money,
    pub max: Money,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Workplace {
    pub unit: UnitIdx,
    /// `None` do czasu Etapu 7 (M2e, WP13) — stanowisko istnieje, ale nie wie jeszcze,
    /// czyim jest zakładem. `Option` zamiast sentinela, bo `SiteId` jest `Entity`
    /// i nisza `Option` nic nie kosztuje.
    pub site: Option<SiteId>,
    pub role: JobRoleId,
    pub shift: ShiftId,
    pub wage_band: WageBand,
    /// W M2 zawsze `None`.
    pub occupant: Option<CitizenId>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Building {
    pub parcel: magnat_core::ParcelId,
    pub grammar: GrammarId,
    pub epoch: EpochId,
    pub footprint: PolyRef,
    /// Nadziemne.
    pub floors: u8,
    pub basements: u8,
    /// Wysokość każdej kondygnacji — M11 tnie widok po stropach, nie po stałej wysokości.
    pub floor_heights_dm: SmallVec<[u16; 8]>,
    pub height_dm: u16,
    pub gross_area_m2: u32,
    pub units: Range<u32>,
    /// Stan techniczny: f(epoka, wartość gruntu, szum).
    pub condition: magnat_core::Q,
    pub aabb: Aabb3,
    pub entrances: SmallVec<[Entrance; 4]>,
}

/// Wynik Etapu 6.
#[derive(Clone, Debug)]
pub struct BuildingSet {
    pub buildings: Vec<Building>,
    pub units: Vec<Unit>,
    pub workplaces: Vec<Workplace>,
    /// „Budynki w prostokącie" — selekcja do kadru dla M11 i karta inspekcji.
    pub index: CsrGrid<BuildingId>,
    pub report: BuildReport,
}

#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct BuildReport {
    pub buildings: u32,
    pub units: u32,
    pub dwellings: u32,
    pub workplaces: u32,
    /// Parcele, na których po cofnięciach nie zmieściła się żadna bryła.
    pub too_small: u32,
    /// Parcele świadomie niezabudowane — `applies.coverage` gramatyki.
    pub left_vacant: u32,
    /// Użycia gramatyki awaryjnej. Kryterium akceptacyjne §5.6: < 2%.
    pub fallback: u32,
    /// Fallback w rozbiciu na strefy (`ZoneKind::index`) — bez tego „6 % awaryjnych"
    /// nie mówi, której gramatyki brakuje, a właśnie o to pytanie chodzi.
    pub fallback_zone: [u32; 16],
    /// Trafienia dopiero po rozluźnieniu filtru `applies` — luka w katalogu gramatyk,
    /// nie błąd generacji, ale ma być widoczna.
    pub relaxed: u32,
    /// To samo w rozbiciu na filtr, który trzeba było pominąć. Rozróżnienie ma znaczenie
    /// operacyjne: brak epoki albo stylu łata się **plikiem** w `data/grammar/`, a brak
    /// przedziału wartości gruntu — **liczbą** w istniejącym pliku. Suma nie musi być
    /// równa `relaxed`: jeden dobór pomija filtry narastająco (M2f, WP19).
    pub relaxed_epoch: u32,
    pub relaxed_style: u32,
    pub relaxed_value: u32,
    /// Derywacje przerwane limitem węzłów albo głębokości (M2 §5.6: liczone, nie ciche).
    pub truncated: u32,
    /// Wysunięcia (`Protrude`): postawione, przycięte do granicy działki i odrzucone
    /// jako płytsze niż voxel. Bez tych trzech liczb nie widać, czy balkonów nie ma,
    /// bo gramatyka ich nie chce, czy dlatego, że nigdzie się nie mieszczą (M2f §5.6c).
    pub protrusions: u32,
    pub protrusions_clipped: u32,
    pub protrusions_dropped: u32,
    /// Budynki w strefie `Logistics`/`IndustryHeavy` bez drogi bez `NO_HEAVY` w zasięgu
    /// (korekta D7) — rampa nie powstała i ma to być widać.
    pub ramp_missing: u32,
    pub edit_commands: u32,

    // ── WP20: różnorodność ──────────────────────────────────────────────────────
    /// Ile budynków dostała każda gramatyka (indeksy jak `GrammarId`).
    pub grammar_hist: Vec<u32>,
    /// Pary budynków w promieniu 60 m i ile z nich ma **identyczną** sygnaturę.
    /// Dwie liczby zamiast udziału, bo `BuildReport` jest `Eq` i nie trzyma floatów.
    pub signature_pairs: u32,
    pub signature_repeats: u32,
    /// Najgorsze sąsiedztwo: udział powtórzeń w promilach i jego współrzędne w metrach —
    /// żeby dało się tam polecieć kamerą, zamiast czytać, że test upadł.
    pub worst_neighbourhood_permille: u32,
    pub worst_neighbourhood_at: (i32, i32),
    /// Najniższa entropia Shannona rozkładu **sygnatur** w dzielnicy mieszkaniowej
    /// o co najmniej [`MIN_BUDYNKOW_DZIELNICY`] budynkach, w milibitach, i numer tej
    /// dzielnicy. Zero znaczy „nie było czego mierzyć".
    ///
    /// Po sygnaturach, a nie po gramatykach — bo dzielnica zbudowana w całości z chałup,
    /// w której każda ma inny materiał, inny dach i inną wysokość, **jest** różnorodna,
    /// a entropia gramatyk pokazywała dla niej 0,29 bita. Sygnatura mierzy to, co gracz
    /// widzi z ulicy; identyfikator gramatyki mierzy, jak zorganizowany jest katalog.
    pub min_district_entropy_mbits: u32,
    pub min_district_entropy_at: u16,
    pub districts_measured: u32,
    /// Dzielnica o najniższej entropii w trzech liczbach: **która gramatyka** ją zdominowała,
    /// ile w niej ma budynków i ile budynków ma cała dzielnica. Bez tego sama entropia mówi,
    /// że jest źle, ale nie mówi, czy to wina katalogu, czy charakteru dzielnicy.
    pub min_district_top: (u16, u32, u32),
    /// Entropia rozkładu sygnatur w **całym mieście**, w milibitach. Dzielnica ma prawo być
    /// monotonna — osiedle płytowe takie jest i tak ma wyglądać — ale miasto nie ma.
    /// To jest liczba, która odpowiada na pytanie „czy to miasto jest różnorodne".
    pub city_entropy_mbits: u32,
}

/// Promień sąsiedztwa z testu T13 — mniej więcej to, co widać z chodnika po obu
/// stronach ulicy.
pub const PROMIEN_SASIEDZTWA_M: f32 = 60.0;

/// Poniżej tylu budynków entropia dzielnicy jest szumem, nie miarą.
pub const MIN_BUDYNKOW_DZIELNICY: u32 = 100;

/// Cztery znaczniki, po których dwa budynki tej samej gramatyki są albo nie są tym samym
/// budynkiem: gramatyka, liczba kondygnacji, materiał ściany i kształt dachu. Dokładnie to,
/// co gracz rozróżnia z chodnika po drugiej stronie ulicy — i nic więcej, bo detal poniżej
/// tego progu i tak ginie w rastrze (M2f §5.6c).
///
/// Liczona w generacji i **nietrzymana w `Building`**: 14,5 tys. budynków × 4 B to pamięć
/// za miarę jakości, a nie za stan gry. Do raportu trafiają agregaty, nie tablica sygnatur.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct BuildingSignature(pub u32);

impl BuildingSignature {
    /// Pakowanie: 6 bitów gramatyki, 5 kondygnacji, 10 materiału, 2 dachu — 23 z 32.
    /// Przekroczenie zakresu **nasyca**, nie zawija: budynek 40-kondygnacyjny ma być
    /// nieodróżnialny od 31-kondygnacyjnego, a nie od 8-kondygnacyjnego.
    #[must_use]
    pub fn new(
        grammar: GrammarId,
        floors: u8,
        wall: magnat_voxel::MaterialId,
        roof: RoofKind,
    ) -> BuildingSignature {
        let g = u32::from(grammar.0).min(63);
        let f = u32::from(floors).min(31);
        let m = u32::from(wall.0).min(1023);
        BuildingSignature(g | (f << 6) | (m << 11) | (roof.index() << 21))
    }
}

#[must_use]
pub fn building_id(i: u32) -> BuildingId {
    BuildingId(magnat_core::Entity::new(
        i,
        std::num::NonZeroU32::new(1).expect("1 != 0"),
    ))
}

// ── Katalog ról (`data/jobs/`) ───────────────────────────────────────────────────────

#[derive(Clone, Debug, serde::Deserialize)]
pub struct JobRole {
    pub key: String,
    pub unit: UnitClass,
    /// Powierzchnia lokalu na jedno stanowisko.
    pub m2_per_workplace: u16,
    pub shift: ShiftKey,
    /// Widełki bazowe w epoce współczesnej, grosze miesięcznie brutto.
    pub wage_base: (i64, i64, i64),
    /// Prestiż zawodu 0..=100 — składnik „zawód" funkcji statusu (M3c §5.8).
    /// Etap 8 przepisuje go do `CityFacts.job_prestige` (M3d, korekta E-14).
    #[serde(default = "prestiz_neutralny")]
    pub prestige: u8,
}

/// Rola bez wpisanego prestiżu jest zawodem przeciętnym, nie zawodem bez znaczenia.
fn prestiz_neutralny() -> u8 {
    50
}

/// Zmiana w danych. Osobny typ od [`ShiftId`], bo ten drugi jest kontraktem dla M3
/// i nie ma powodu, żeby wisiał na `serde`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Deserialize)]
pub enum ShiftKey {
    I,
    II,
    III,
    Flexible,
}

impl From<ShiftKey> for ShiftId {
    fn from(s: ShiftKey) -> ShiftId {
        match s {
            ShiftKey::I => ShiftId::I,
            ShiftKey::II => ShiftId::II,
            ShiftKey::III => ShiftId::III,
            ShiftKey::Flexible => ShiftId::Flexible,
        }
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct JobTable {
    pub schema_version: u32,
    pub epoch_wage_mult: Vec<(String, f32)>,
    pub roles: Vec<JobRole>,
}

/// Wersja schematu `data/jobs/roles.ron`.
///
/// Podniesiona z 1 do 2 w M7a: rekord roli dostał `weights` (wagi produktywności
/// per zawód, `D16` rozstrzygnięte jako rozszerzenie schematu). M2 tego pola nie
/// czyta — czyta je `magnat_firms::RoleTable`, drugi widok na ten sam plik.
/// Wersję trzymają obaj czytelnicy i **musi** się zgadzać u obu, inaczej dopisanie
/// pola po jednej stronie przeszłoby niezauważone po drugiej.
pub const JOBS_SCHEMA_VERSION: u32 = 2;

impl JobTable {
    pub fn load() -> Result<JobTable, crate::city::zoning::EpochError> {
        use crate::city::zoning::EpochError;
        let path = crate::assets::data_path("jobs/roles.ron");
        let txt = std::fs::read_to_string(&path).map_err(EpochError::Io)?;
        let t: JobTable = ron::from_str(&txt).map_err(EpochError::Ron)?;
        if t.schema_version != JOBS_SCHEMA_VERSION {
            return Err(EpochError::Schema {
                found: t.schema_version,
            });
        }
        Ok(t)
    }

    /// Rola obsadzająca dany rodzaj lokalu; `None` dla mieszkań i części wspólnych.
    #[must_use]
    pub fn role_for(&self, k: UnitKind) -> Option<(JobRoleId, &JobRole)> {
        let szukany = match k {
            UnitKind::Retail => UnitClass::Retail,
            UnitKind::Office => UnitClass::Office,
            UnitKind::Workshop => UnitClass::Workshop,
            UnitKind::Storage => UnitClass::Storage,
            UnitKind::Dwelling { .. } | UnitKind::Common => return None,
        };
        self.roles
            .iter()
            .position(|r| r.unit == szukany)
            .map(|i| (JobRoleId(i as u16), &self.roles[i]))
    }

    #[must_use]
    pub fn epoch_mult(&self, epoch_key: &str) -> f32 {
        self.epoch_wage_mult
            .iter()
            .find(|(k, _)| k == epoch_key)
            .map_or(1.0, |(_, v)| *v)
    }
}

/// `WageBand` = widełki roli × epoka × zamożność dzielnicy (M2 §6, kontrakt dla M3).
#[must_use]
pub fn wage_band(role: &JobRole, epoch_mult: f32, income_tier: u8) -> WageBand {
    // Dzielnica zamożna płaci więcej za tę samą rolę: 0,88 przy tier 0, 1,18 przy 4.
    let tier = 0.88 + 0.075 * f32::from(income_tier.min(4));
    let m = f64::from(epoch_mult * tier);
    let skala = |v: i64| Money((v as f64 * m).round() as i64);
    WageBand {
        min: skala(role.wage_base.0),
        median: skala(role.wage_base.1),
        max: skala(role.wage_base.2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widelki_rosna_z_epoka_i_zamoznoscia() {
        let jobs = JobTable::load().expect("data/jobs/roles.ron");
        let role = &jobs.roles[0];
        let biedna = wage_band(role, jobs.epoch_mult("postwar"), 0);
        let bogata = wage_band(role, jobs.epoch_mult("contemporary"), 4);
        assert!(bogata.median.0 > biedna.median.0);
        assert!(biedna.min.0 < biedna.median.0 && biedna.median.0 < biedna.max.0);
    }
}
