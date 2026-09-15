//! Katalogi danych fazy: klasy pojazdów i diagram podstawowy ruchu (M4b §5.2, §5.7).
//!
//! Obie tabele są **danymi, nie kodem** (00 §5, zasada „rozszerzanie przez dane"):
//! `balansator calibrate-vdf` z M4d przepisuje `data/roads/vdf.ron` bez dotykania
//! ani jednej linii Rusta, a nowa klasa pojazdu to wiersz w `data/vehicles/classes.ron`.
//!
//! Wszystkie mnożniki są całkowite i wyrażone w promilach. To nie jest ozdoba:
//! ich iloczyn wchodzi do `settle_edge`, czyli do zużycia paliwa, czyli do pieniądza,
//! a tam float jest zakazany (00 §2).

use magnat_core::{data_path, Money, RoadClass};
use serde::Deserialize;
use std::path::Path;

pub const VEHICLES_SCHEMA_VERSION: u32 = 1;
pub const VDF_SCHEMA_VERSION: u32 = 1;

/// Rodzaj energii napędowej.
///
/// `Electric` istnieje jako wariant, ale **żadna klasa w `data/` go dziś nie używa**:
/// epoki, które M2 generuje, kończą się na 2020, a flota jest spalinowa. Zbiornik
/// trzyma wtedy watogodziny zamiast mililitrów — jedna ścieżka kodu, inna jednostka.
///
/// `ponytail:` sufit nazwany — jednostka baku zależy od `kind`, zamiast być w typie
/// (`D6` wariant (b), `enum EnergyStore`). Ścieżka wyjścia: gdy M8 podłączy ładowarki
/// do sieci energetycznej i `Energy` zacznie wychodzić na zewnątrz, `FuelTank`
/// rozdziela się na dwa warianty; dziś drugi wariant nie miałby ani jednej instancji.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Deserialize)]
#[repr(u8)]
pub enum FuelKind {
    Petrol,
    Diesel,
    Lpg,
    Electric,
}

impl FuelKind {
    pub const ALL: &'static [FuelKind] = &[
        FuelKind::Petrol,
        FuelKind::Diesel,
        FuelKind::Lpg,
        FuelKind::Electric,
    ];

    #[must_use]
    pub const fn as_index(self) -> usize {
        self as usize
    }

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            FuelKind::Petrol => "petrol",
            FuelKind::Diesel => "diesel",
            FuelKind::Lpg => "lpg",
            FuelKind::Electric => "electric",
        }
    }

    /// Czy jednostką zbiornika są watogodziny zamiast mililitrów.
    #[must_use]
    pub const fn is_electric(self) -> bool {
        matches!(self, FuelKind::Electric)
    }
}

/// Indeks klasy pojazdu w katalogu. Kolejność w `data/vehicles/classes.ron`
/// jest kontraktem — ten indeks siedzi w komponencie pojazdu, a komponent w zapisie.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[repr(transparent)]
pub struct VehicleClassId(pub u16);

/// Parametry jednej klasy pojazdu.
#[derive(Clone, Debug, Deserialize)]
pub struct VehicleClassSpec {
    pub key: String,
    pub fuel: FuelKind,
    /// Masa własna w gramach — mianownik mnożnika obciążenia.
    pub kerb_mass_g: i64,
    /// Długość pojazdu; wchodzi do pojemności postojowej krawędzi w M4d.
    pub length_cm: u16,
    pub seats: u8,
    /// Pojemność zbiornika: mililitry albo watogodziny wg `fuel`.
    pub tank_ml: i64,
    /// Poziom, poniżej którego podróż zaczyna się od tankowania (promile pojemności).
    pub refuel_threshold_permille: u16,
    /// Zużycie odniesienia: pojazd pusty, teren płaski, kubełek prędkości optymalny.
    pub base_ml_per_100km: u32,
    pub idle_ml_per_stop: u32,
    pub cold_start_ml: u32,
    /// Amortyzacja w groszach na 100 km. **Wchodzi tylko do kosztu uogólnionego
    /// decyzji** (`D3`): realne obciążenie budżetu gospodarstwa nalicza M5 z przebiegu,
    /// więc liczenie jej tu drugi raz byłoby podwójnym kosztem.
    pub wear_gr_per_100km: i64,
    pub top_speed_dkmh: u16,
}

#[derive(Deserialize)]
struct FuelPriceRow {
    kind: FuelKind,
    price: u32,
}

#[derive(Deserialize)]
struct VehiclesFile {
    schema_version: u32,
    speed_factor_permille: Vec<u32>,
    load_slope_permille: u32,
    grade_slope_permille: u32,
    grade_floor_permille: u32,
    fuel_price_gr: Vec<FuelPriceRow>,
    classes: Vec<VehicleClassSpec>,
}

/// Katalog klas pojazdów wraz z mnożnikami wzoru paliwowego.
#[derive(Clone, Debug)]
pub struct VehicleCatalog {
    classes: Vec<VehicleClassSpec>,
    speed_factor_permille: Vec<u32>,
    load_slope_permille: u32,
    grade_slope_permille: u32,
    grade_floor_permille: u32,
    /// Cena w groszach za litr (albo za kWh dla `Electric`), indeksowana `FuelKind`.
    price_gr: [u32; 4],
}

impl VehicleCatalog {
    /// Szerokość kubełka prędkości w tabeli `speed_factor_permille`, w dekakm/h.
    pub const SPEED_BUCKET_DKMH: u16 = 100;

    pub fn load_default() -> Result<VehicleCatalog, DataError> {
        VehicleCatalog::load(&data_path("vehicles/classes.ron"))
    }

    pub fn load(path: &Path) -> Result<VehicleCatalog, DataError> {
        let txt = std::fs::read_to_string(path)?;
        let f: VehiclesFile = ron::from_str(&txt).map_err(|e| DataError::Ron(e.to_string()))?;
        if f.schema_version != VEHICLES_SCHEMA_VERSION {
            return Err(DataError::Schema {
                found: f.schema_version,
                want: VEHICLES_SCHEMA_VERSION,
            });
        }
        if f.classes.is_empty() {
            return Err(DataError::Empty("vehicles/classes.ron: classes"));
        }
        if f.speed_factor_permille.is_empty() {
            return Err(DataError::Empty(
                "vehicles/classes.ron: speed_factor_permille",
            ));
        }
        let mut price_gr = [0u32; 4];
        for row in &f.fuel_price_gr {
            price_gr[row.kind.as_index()] = row.price;
        }
        for k in FuelKind::ALL {
            if price_gr[k.as_index()] == 0 {
                return Err(DataError::MissingFuelPrice(k.key()));
            }
        }
        for c in &f.classes {
            if c.kerb_mass_g <= 0 || c.tank_ml <= 0 {
                return Err(DataError::BadClass(c.key.clone()));
            }
        }
        Ok(VehicleCatalog {
            classes: f.classes,
            speed_factor_permille: f.speed_factor_permille,
            load_slope_permille: f.load_slope_permille,
            grade_slope_permille: f.grade_slope_permille,
            grade_floor_permille: f.grade_floor_permille,
            price_gr,
        })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.classes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.classes.is_empty()
    }

    /// Specyfikacja klasy. Indeks spoza katalogu jest błędem wołającego, nie danych —
    /// `VehicleClassId` powstaje wyłącznie tutaj.
    #[must_use]
    pub fn spec(&self, id: VehicleClassId) -> &VehicleClassSpec {
        &self.classes[id.0 as usize]
    }

    #[must_use]
    pub fn by_key(&self, key: &str) -> Option<VehicleClassId> {
        self.classes
            .iter()
            .position(|c| c.key == key)
            .map(|i| VehicleClassId(i as u16))
    }

    /// Mnożnik zużycia dla skwantowanej prędkości średniej. Ostatni kubełek
    /// obowiązuje dla wszystkiego powyżej tabeli.
    #[must_use]
    pub fn speed_factor(&self, mean_speed_dkmh: u16) -> u32 {
        let i = (mean_speed_dkmh / VehicleCatalog::SPEED_BUCKET_DKMH) as usize;
        let last = self.speed_factor_permille.len() - 1;
        self.speed_factor_permille[i.min(last)]
    }

    #[must_use]
    pub const fn load_slope_permille(&self) -> u32 {
        self.load_slope_permille
    }

    #[must_use]
    pub const fn grade_slope_permille(&self) -> u32 {
        self.grade_slope_permille
    }

    #[must_use]
    pub const fn grade_floor_permille(&self) -> u32 {
        self.grade_floor_permille
    }

    /// Cena jednostki paliwa w groszach (litr albo kWh).
    #[must_use]
    pub fn price_gr(&self, kind: FuelKind) -> i64 {
        i64::from(self.price_gr[kind.as_index()])
    }

    /// Koszt paliwa w **mikrolitrach** (`K-25`) — bo taka jest jednostka wewnętrzna
    /// ruchu, a cena z katalogu jest za litr. Zaokrąglenie jest jawne i idzie przez
    /// `div_round_half_up` (00 §2), nigdy przez obcięcie.
    ///
    /// Dzielnik był `1_000` do M4c: funkcja powstała w M4b przed rozstrzygnięciem
    /// `K-25` i **nie miała wtedy żadnego wołającego** — jedyne tankowanie liczyło
    /// się inline w `trip::refuel`, już w mikrolitrach. Pierwszy konsument (tabor
    /// komunikacji) pokazał błąd natychmiast: kurs płacił tysiąckrotność.
    #[must_use]
    pub fn fuel_cost(&self, kind: FuelKind, units_ul: i64) -> Money {
        Money(units_ul.saturating_mul(self.price_gr(kind))).div_round_half_up(1_000_000)
    }
}

// ── diagram podstawowy ──────────────────────────────────────────────────────────

/// Parametry diagramu podstawowego dla jednej klasy drogi.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct VdfClass {
    pub capacity_vph_per_lane: u16,
    pub crit_density_permille: u16,
    pub min_speed_dkmh: u16,
}

#[derive(Deserialize)]
struct VdfRow {
    class: String,
    capacity_vph_per_lane: u16,
    crit_density_permille: u16,
    min_speed_dkmh: u16,
}

#[derive(Deserialize)]
struct VdfFile {
    schema_version: u32,
    jam_spacing_cm: u32,
    classes: Vec<VdfRow>,
}

/// Tabela diagramu podstawowego, indeksowana `RoadClass`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct VdfTable {
    jam_spacing_cm: u32,
    classes: [VdfClass; 8],
}

impl VdfTable {
    pub fn load_default() -> Result<VdfTable, DataError> {
        VdfTable::load(&data_path("roads/vdf.ron"))
    }

    pub fn load(path: &Path) -> Result<VdfTable, DataError> {
        let txt = std::fs::read_to_string(path)?;
        let f: VdfFile = ron::from_str(&txt).map_err(|e| DataError::Ron(e.to_string()))?;
        if f.schema_version != VDF_SCHEMA_VERSION {
            return Err(DataError::Schema {
                found: f.schema_version,
                want: VDF_SCHEMA_VERSION,
            });
        }
        if f.jam_spacing_cm == 0 {
            return Err(DataError::Empty("roads/vdf.ron: jam_spacing_cm"));
        }
        // Brak wiersza nie może dawać cichej wartości domyślnej: klasa bez wpisu
        // dostałaby prędkość 0 i cała sieć na niej stanęłaby bez komunikatu.
        let mut classes = [None; 8];
        for row in &f.classes {
            let idx = RoadClass::ALL
                .iter()
                .position(|c| c.key() == row.class)
                .ok_or_else(|| DataError::UnknownClass(row.class.clone()))?;
            if classes[idx].is_some() {
                return Err(DataError::DuplicateClass(row.class.clone()));
            }
            classes[idx] = Some(VdfClass {
                capacity_vph_per_lane: row.capacity_vph_per_lane,
                crit_density_permille: row.crit_density_permille,
                min_speed_dkmh: row.min_speed_dkmh,
            });
        }
        let mut out = [VdfClass {
            capacity_vph_per_lane: 0,
            crit_density_permille: 0,
            min_speed_dkmh: 0,
        }; 8];
        for (i, c) in classes.iter().enumerate() {
            out[i] = c.ok_or_else(|| DataError::MissingClass(RoadClass::ALL[i].key()))?;
        }
        Ok(VdfTable {
            jam_spacing_cm: f.jam_spacing_cm,
            classes: out,
        })
    }

    #[must_use]
    pub fn class(&self, c: RoadClass) -> VdfClass {
        self.classes[c.as_index()]
    }

    #[must_use]
    pub const fn jam_spacing_cm(&self) -> u32 {
        self.jam_spacing_cm
    }

    /// Najmniejsza pojemność postojowa, jaką dostaje krawędź.
    ///
    /// Sieć M2 ma kilkaset odcinków krótszych od 20 m — to kikuty przy skrzyżowaniach,
    /// nie ulice. Przy dosłownym `długość / odstęp` mieszczą jeden pojazd, a krawędź
    /// o pojemności jeden jest **kandydatem na zakleszczenie**: wystarczy, że dwa takie
    /// kikuty czekają na siebie nawzajem przy jednym skrzyżowaniu i obie strony stoją
    /// na zawsze. Cztery pojazdy to tyle, ile fizycznie ustawia się przed linią
    /// zatrzymania nawet na bardzo krótkim wlocie.
    pub const MIN_STORAGE: u16 = 4;

    /// Czy krawędź jest na tyle **długa**, żeby stanęła na niej realna kolejka.
    ///
    /// Kryterium liczy się **na pas**, nie na całą krawędź: piętnastometrowy odcinek
    /// arterii o dwóch pasach mieści cztery pojazdy, więc po zsumowaniu pasów trafiłby
    /// w próg — a kolejka na piętnastu metrach arterii to nadal fikcja geometryczna,
    /// bo fizycznie stoi ona na ulicy przed nią. Zmierzone: przy kryterium sumującym
    /// pasy najwęższym gardłem metropolii był właśnie taki kikut, z 4 299 odmowami
    /// wjazdu na dobę (dziennik M4b).
    #[must_use]
    pub fn holds_queue(&self, length_cm: u32, _lanes: u8) -> bool {
        length_cm / self.jam_spacing_cm.max(1) >= u32::from(VdfTable::MIN_STORAGE)
    }

    /// Ile pojazdów fizycznie mieści się na krawędzi. To ta liczba, a nie
    /// przepustowość, wyznacza próg spillbacku (§5.4 punkt 3).
    #[must_use]
    pub fn storage_capacity(&self, length_cm: u32, lanes: u8) -> u16 {
        let lanes = u32::from(lanes.max(1));
        let n = length_cm.saturating_mul(lanes) / self.jam_spacing_cm.max(1);
        n.clamp(u32::from(VdfTable::MIN_STORAGE), u32::from(u16::MAX)) as u16
    }

    /// Przepustowość krawędzi w pojazdach na minutę.
    #[must_use]
    pub fn capacity_vpm(&self, class: RoadClass, lanes: u8) -> u16 {
        let vph = u32::from(self.class(class).capacity_vph_per_lane) * u32::from(lanes.max(1));
        (vph / 60).min(u32::from(u16::MAX)) as u16
    }

    /// Prędkość średnia z obłożenia krawędzi — **jedyne** miejsce, w którym ona powstaje.
    ///
    /// Dwa reżimy: do gęstości krytycznej prędkość swobodna, powyżej spadek liniowy
    /// do `min_speed_dkmh` przy zapełnieniu. Wynik jest całkowity i przez to identyczny
    /// na każdej platformie i w każdym LOD z definicji, a nie z tolerancji (§5.4).
    #[must_use]
    pub fn speed_dkmh(
        &self,
        class: RoadClass,
        free_flow_dkmh: u16,
        occupancy: u16,
        storage_capacity: u16,
    ) -> u16 {
        let spec = self.class(class);
        let free = free_flow_dkmh.max(spec.min_speed_dkmh);
        let cap = u32::from(storage_capacity.max(1));
        let density = (u32::from(occupancy) * 1000 / cap).min(1000);
        let crit = u32::from(spec.crit_density_permille);
        if density <= crit {
            return free;
        }
        let span = (1000 - crit).max(1);
        let over = (density - crit).min(span);
        let drop = u32::from(free.saturating_sub(spec.min_speed_dkmh));
        let v = u32::from(free) - drop * over / span;
        (v as u16).max(spec.min_speed_dkmh).max(1)
    }
}

// ── błędy ładowania ─────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum DataError {
    Io(std::io::Error),
    Ron(String),
    Schema { found: u32, want: u32 },
    Empty(&'static str),
    BadClass(String),
    UnknownClass(String),
    DuplicateClass(String),
    MissingClass(&'static str),
    MissingFuelPrice(&'static str),
}

impl std::fmt::Display for DataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataError::Io(e) => write!(f, "dane ruchu: {e}"),
            DataError::Ron(m) => write!(f, "dane ruchu: {m}"),
            DataError::Schema { found, want } => {
                write!(f, "dane ruchu: schema_version {found}, oczekiwano {want}")
            }
            DataError::Empty(k) => write!(f, "dane ruchu: pusta sekcja {k}"),
            DataError::BadClass(k) => write!(f, "dane ruchu: klasa {k:?} ma masę albo bak ≤ 0"),
            DataError::UnknownClass(k) => write!(f, "dane ruchu: nieznana klasa drogi {k:?}"),
            DataError::DuplicateClass(k) => write!(f, "dane ruchu: klasa drogi {k:?} dwa razy"),
            DataError::MissingClass(k) => write!(f, "dane ruchu: brak klasy drogi {k}"),
            DataError::MissingFuelPrice(k) => write!(f, "dane ruchu: brak ceny paliwa {k}"),
        }
    }
}

impl std::error::Error for DataError {}

impl From<std::io::Error> for DataError {
    fn from(e: std::io::Error) -> Self {
        DataError::Io(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn katalogi_z_repozytorium_sie_laduja() {
        let cat = VehicleCatalog::load_default().expect("data/vehicles/");
        assert!(!cat.is_empty());
        assert!(cat.by_key("car_small").is_some());
        assert!(cat.price_gr(FuelKind::Petrol) > 0);

        let vdf = VdfTable::load_default().expect("data/roads/");
        assert!(vdf.jam_spacing_cm() > 0);
    }

    #[test]
    fn predkosc_spada_dopiero_powyzej_gestosci_krytycznej() {
        let vdf = VdfTable::load_default().expect("data/roads/");
        let c = RoadClass::Local;
        let spec = vdf.class(c);
        let free = 300;
        // Pusta krawędź i krawędź poniżej progu: prędkość swobodna.
        assert_eq!(vdf.speed_dkmh(c, free, 0, 100), free);
        let ponizej = (u32::from(spec.crit_density_permille) * 100 / 1000) as u16;
        assert_eq!(vdf.speed_dkmh(c, free, ponizej, 100), free);
        // Zapełniona: prędkość minimalna klasy.
        assert_eq!(vdf.speed_dkmh(c, free, 100, 100), spec.min_speed_dkmh);
        // Monotoniczność — bez niej korek mógłby przyspieszać.
        let mut poprzednia = free;
        for occ in 0..=100u16 {
            let v = vdf.speed_dkmh(c, free, occ, 100);
            assert!(v <= poprzednia, "prędkość rośnie przy occ={occ}");
            poprzednia = v;
        }
    }

    #[test]
    fn mnoznik_predkosci_ma_ksztalt_litery_u() {
        let cat = VehicleCatalog::load_default().expect("data/vehicles/");
        // W korku i na autostradzie drożej niż w optimum — inaczej korek nie kosztowałby
        // paliwa, a to jest jedna z rzeczy, które faza ma pokazać.
        let korek = cat.speed_factor(50);
        let optimum = cat.speed_factor(550);
        let autostrada = cat.speed_factor(1_300);
        assert!(korek > optimum, "{korek} !> {optimum}");
        assert!(autostrada > optimum, "{autostrada} !> {optimum}");
    }
}
