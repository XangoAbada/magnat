//! Parametry generacji świata (PRD §4.1, M1 §5.5).
//!
//! Cały ten zestaw wchodzi do zapisu gry i jest **jedynym** wejściem generatora:
//! z `WorldGenParams` da się odtworzyć teren bit w bit, więc terenu nie zapisujemy
//! (M1 §6.1, kontrakt dla M12).

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Rozmiar mapy. Wiąże się z celem populacyjnym z PRD §4.1
/// (małe 20–40 tys. → 4 km; metropolia 400 tys.+ → 16 km).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum WorldSize {
    Small4km,
    Medium8km,
    Large12km,
    Metropolis16km,
}

/// Bok siatki roboczej generatora w metrach (M1 §5.1). Hydrologia i erozja liczone tu,
/// 1 m jest deterministyczną pochodną.
pub const WORK_CELL_M: u32 = 4;
/// Bok komórki klimatu w metrach (M1 §5.1). Klimat nie ma struktury poniżej tej skali.
pub const CLIMATE_CELL_M: u32 = 256;

impl WorldSize {
    /// Bok mapy w metrach.
    #[must_use]
    pub const fn meters(self) -> u32 {
        match self {
            WorldSize::Small4km => 4_096,
            WorldSize::Medium8km => 8_192,
            WorldSize::Large12km => 12_288,
            WorldSize::Metropolis16km => 16_384,
        }
    }

    /// Bok siatki roboczej 4 m w komórkach.
    #[must_use]
    pub const fn work_dim(self) -> u32 {
        self.meters() / WORK_CELL_M
    }

    /// Bok siatki klimatu 256 m w komórkach.
    #[must_use]
    pub const fn climate_dim(self) -> u32 {
        self.meters() / CLIMATE_CELL_M
    }

    /// Liczba iteracji erozji (M1 §5.7): 40 dla map ≤ 8 km, 80 dla większych.
    #[must_use]
    pub const fn erosion_iterations(self) -> u32 {
        match self {
            WorldSize::Small4km | WorldSize::Medium8km => 40,
            WorldSize::Large12km | WorldSize::Metropolis16km => 80,
        }
    }

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            WorldSize::Small4km => "4km",
            WorldSize::Medium8km => "8km",
            WorldSize::Large12km => "12km",
            WorldSize::Metropolis16km => "16km",
        }
    }

    pub const ALL: &'static [WorldSize] = &[
        WorldSize::Small4km,
        WorldSize::Medium8km,
        WorldSize::Large12km,
        WorldSize::Metropolis16km,
    ];
}

/// Epoka startowa (PRD §4.1). W M1 przesuwa wagi typów złóż i stopień ich historycznego
/// wyeksploatowania; reszta skutków należy do faz ekonomicznych.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum Epoch {
    Y1950,
    Y1970,
    Y1990,
    Y2010,
    Y2020,
}

impl Epoch {
    #[must_use]
    pub const fn year(self) -> u16 {
        match self {
            Epoch::Y1950 => 1950,
            Epoch::Y1970 => 1970,
            Epoch::Y1990 => 1990,
            Epoch::Y2010 => 2010,
            Epoch::Y2020 => 2020,
        }
    }

    /// Klucz tekstowy epoki — ten sam, którym parsuje ją wiersz poleceń,
    /// i człon klucza lokalizacji `ui.epoch.<key>`. Dopisane w M9b: kreator świata
    /// (WP14) wypisuje warianty z `ALL`, a bez klucza musiałby je nazywać po swojemu.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Epoch::Y1950 => "1950",
            Epoch::Y1970 => "1970",
            Epoch::Y1990 => "1990",
            Epoch::Y2010 => "2010",
            Epoch::Y2020 => "2020",
        }
    }

    pub const ALL: &'static [Epoch] = &[
        Epoch::Y1950,
        Epoch::Y1970,
        Epoch::Y1990,
        Epoch::Y2010,
        Epoch::Y2020,
    ];
}

/// Profil gospodarczy miasta (PRD §4.1).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum EconomyProfile {
    Industrial,
    Port,
    University,
    Tourist,
    Agricultural,
    Mixed,
}

impl EconomyProfile {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            EconomyProfile::Industrial => "industrial",
            EconomyProfile::Port => "port",
            EconomyProfile::University => "university",
            EconomyProfile::Tourist => "tourist",
            EconomyProfile::Agricultural => "agricultural",
            EconomyProfile::Mixed => "mixed",
        }
    }

    pub const ALL: &'static [EconomyProfile] = &[
        EconomyProfile::Industrial,
        EconomyProfile::Port,
        EconomyProfile::University,
        EconomyProfile::Tourist,
        EconomyProfile::Agricultural,
        EconomyProfile::Mixed,
    ];
}

/// Region geograficzny. Steruje wypiętrzeniem, poziomem morza, wilgotnością bazową
/// i wagami złóż (M1 §5.5).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum Region {
    Coastal,
    Mountain,
    Lowland,
    River,
    Desert,
}

impl Region {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Region::Coastal => "coastal",
            Region::Mountain => "mountain",
            Region::Lowland => "lowland",
            Region::River => "river",
            Region::Desert => "desert",
        }
    }

    /// Szerokość geograficzna środka mapy w dziesiątych częściach stopnia.
    /// Wchodzi do modelu temperatury (P10) i do pozycji słońca (R4) — stąd musi tu być,
    /// mimo że brzmi jak parametr renderu.
    #[must_use]
    pub const fn latitude_ddeg(self) -> i16 {
        match self {
            Region::Coastal => 542,  // 54,2° — wybrzeże bałtyckie
            Region::Mountain => 494, // 49,4° — pogórze
            Region::Lowland => 521,  // 52,1° — nizina środkowa
            Region::River => 512,    // 51,2° — dolina dużej rzeki
            Region::Desert => 315,   // 31,5° — strefa suchych podzwrotników
        }
    }

    /// Czy region ma morze w granicach mapy. Steruje P1 i klasyfikacją wód w P7.
    #[must_use]
    pub const fn has_sea(self) -> bool {
        matches!(self, Region::Coastal)
    }

    pub const ALL: &'static [Region] = &[
        Region::Coastal,
        Region::Mountain,
        Region::Lowland,
        Region::River,
        Region::Desert,
    ];
}

/// Poziom trudności. M1 wyłącznie przenosi go dalej (PRD §4.1, M1 §3).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum Difficulty {
    Easy,
    Normal,
    Hard,
    Brutal,
}

impl Difficulty {
    /// Klucz tekstowy — ten sam, którym parsuje go wiersz poleceń, i człon klucza
    /// lokalizacji `ui.difficulty.<key>`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Difficulty::Easy => "easy",
            Difficulty::Normal => "normal",
            Difficulty::Hard => "hard",
            Difficulty::Brutal => "brutal",
        }
    }

    pub const ALL: &'static [Difficulty] = &[
        Difficulty::Easy,
        Difficulty::Normal,
        Difficulty::Hard,
        Difficulty::Brutal,
    ];
}

/// Komplet parametrów generacji. W zapisie gry w całości (M1 §5.5).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct WorldGenParams {
    pub seed: u64,
    pub size: WorldSize,
    pub epoch: Epoch,
    pub profile: EconomyProfile,
    pub region: Region,
    pub difficulty: Difficulty,
}

impl Default for WorldGenParams {
    fn default() -> Self {
        WorldGenParams {
            seed: 1,
            size: WorldSize::Small4km,
            epoch: Epoch::Y1990,
            profile: EconomyProfile::Mixed,
            region: Region::Lowland,
            difficulty: Difficulty::Normal,
        }
    }
}

/// Błąd walidacji parametrów. Osobny typ, bo komunikat trafia do CLI i do UI (M9).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ParamError {
    /// Wartość spoza dopuszczalnego zbioru — przy parsowaniu z CLI albo z RON.
    Unknown { field: &'static str, value: String },
    /// Naruszenie niezmiennika geometrii świata. Wyłapuje pomyłkę przy dokładaniu
    /// wariantu `WorldSize` — nie sytuację, którą gracz może wywołać.
    Geometry(&'static str),
}

impl fmt::Display for ParamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParamError::Unknown { field, value } => {
                write!(f, "nieznana wartość `{value}` dla parametru `{field}`")
            }
            ParamError::Geometry(m) => write!(f, "niespójna geometria świata: {m}"),
        }
    }
}

impl std::error::Error for ParamError {}

impl WorldGenParams {
    /// Niezmienniki geometrii, na których stoi cały generator. Sprawdzane raz, przy starcie:
    /// gdyby ktoś dopisał `WorldSize` niepodzielny przez 256 m, klimat i siatka robocza
    /// rozjechałyby się o pół komórki, a objaw wyszedłby dopiero jako artefakt na brzegu mapy.
    pub fn validate(&self) -> Result<(), ParamError> {
        let m = self.size.meters();
        if !m.is_multiple_of(WORK_CELL_M) {
            return Err(ParamError::Geometry("bok mapy niepodzielny przez 4 m"));
        }
        if !m.is_multiple_of(CLIMATE_CELL_M) {
            return Err(ParamError::Geometry("bok mapy niepodzielny przez 256 m"));
        }
        // Chunk voxelowy ma 32 m boku (M1 §5.1); mapa musi się z nich składać bez reszty,
        // inaczej ostatni rząd chunków wystaje poza świat i `ColumnSource` czyta poza zakresem.
        if !m.is_multiple_of(32) {
            return Err(ParamError::Geometry("bok mapy niepodzielny przez 32 m"));
        }
        Ok(())
    }

    /// Skrót używany w całym generatorze — bok siatki roboczej jako `usize`.
    #[must_use]
    pub fn work_dim(&self) -> usize {
        self.size.work_dim() as usize
    }

    /// Bok siatki klimatu jako `usize`.
    #[must_use]
    pub fn climate_dim(&self) -> usize {
        self.size.climate_dim() as usize
    }
}

/// Parsowanie z CLI. Przyjmuje pisownię, którą człowiek wpisze z pamięci —
/// `16km`, `metropolis` — i odrzuca resztę z nazwą parametru w błędzie.
macro_rules! parse_enum {
    ($t:ty, $field:literal, $($($pat:literal)|+ => $val:expr),+ $(,)?) => {
        impl FromStr for $t {
            type Err = ParamError;
            fn from_str(s: &str) -> Result<Self, ParamError> {
                let k = s.trim().to_ascii_lowercase();
                $( if $( k == $pat )||+ { return Ok($val); } )+
                Err(ParamError::Unknown { field: $field, value: k })
            }
        }
    };
}

parse_enum!(WorldSize, "size",
    "4km" | "small" | "small4km" => WorldSize::Small4km,
    "8km" | "medium" | "medium8km" => WorldSize::Medium8km,
    "12km" | "large" | "large12km" => WorldSize::Large12km,
    "16km" | "metropolis" | "metropolis16km" => WorldSize::Metropolis16km,
);

parse_enum!(Epoch, "epoch",
    "1950" | "y1950" => Epoch::Y1950,
    "1970" | "y1970" => Epoch::Y1970,
    "1990" | "y1990" => Epoch::Y1990,
    "2010" | "y2010" => Epoch::Y2010,
    "2020" | "y2020" => Epoch::Y2020,
);

parse_enum!(EconomyProfile, "profile",
    "industrial" | "przemyslowe" => EconomyProfile::Industrial,
    "port" | "portowe" => EconomyProfile::Port,
    "university" | "uniwersyteckie" => EconomyProfile::University,
    "tourist" | "turystyczne" => EconomyProfile::Tourist,
    "agricultural" | "rolnicze" => EconomyProfile::Agricultural,
    "mixed" | "mieszane" => EconomyProfile::Mixed,
);

parse_enum!(Region, "region",
    "coastal" | "nadmorski" => Region::Coastal,
    "mountain" | "gorski" => Region::Mountain,
    "lowland" | "nizinny" => Region::Lowland,
    "river" | "rzeczny" => Region::River,
    "desert" | "pustynny" => Region::Desert,
);

parse_enum!(Difficulty, "difficulty",
    "easy" | "latwy" => Difficulty::Easy,
    "normal" | "normalny" => Difficulty::Normal,
    "hard" | "trudny" => Difficulty::Hard,
    "brutal" | "brutalny" => Difficulty::Brutal,
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometria_kazdego_rozmiaru_jest_spojna() {
        for &size in WorldSize::ALL {
            let p = WorldGenParams {
                size,
                ..WorldGenParams::default()
            };
            p.validate()
                .unwrap_or_else(|e| panic!("{}: {e}", size.key()));
            assert_eq!(p.size.work_dim() * WORK_CELL_M, p.size.meters());
            assert_eq!(p.size.climate_dim() * CLIMATE_CELL_M, p.size.meters());
        }
        // Liczby z M1 §5.1 — gdyby ktoś zmienił bok komórki, ten test to wyłapie.
        assert_eq!(WorldSize::Metropolis16km.work_dim(), 4096);
        assert_eq!(WorldSize::Metropolis16km.climate_dim(), 64);
    }

    #[test]
    fn parsowanie_przyjmuje_obie_pisownie() {
        assert_eq!(
            "16km".parse::<WorldSize>().unwrap(),
            WorldSize::Metropolis16km
        );
        assert_eq!(
            "Metropolis".parse::<WorldSize>().unwrap(),
            WorldSize::Metropolis16km
        );
        assert_eq!("rzeczny".parse::<Region>().unwrap(), Region::River);
        assert_eq!(
            "przemyslowe".parse::<EconomyProfile>().unwrap(),
            EconomyProfile::Industrial
        );
        assert_eq!("1990".parse::<Epoch>().unwrap(), Epoch::Y1990);

        let e = "20km".parse::<WorldSize>().unwrap_err();
        assert_eq!(
            e,
            ParamError::Unknown {
                field: "size",
                value: "20km".into()
            }
        );
    }
}
