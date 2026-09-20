//! Kalibracja relacji, zmów i związków zawodowych — `data/tuning/relations.ron`
//! (M10e, `K-88`).
//!
//! Jeden plik na całą podfazę, trzy sekcje, tak samo jak `insurance.ron` z M10d
//! niesie naraz ubezpieczenia i giełdę. Powód jest ten sam: te trzy mechaniki
//! stroi się **razem**, bo stoją na jednej liczbie — na tym, ile firma zarabia
//! ponad koszt. Zbyt niska marża i nie ma po co się zmawiać ani o co strajkować;
//! zbyt wysoka i jedno i drugie dzieje się wszędzie.
//!
//! Granica wobec `data/economy/` jest ta sama, którą `K-56` postawił między
//! `data/city/` a `data/tuning/city.ron`: tam siedzą liczby zmieniające kształt
//! modelu (widełki marży sklepu, koszty stałe), tutaj progi, przy których
//! mechanika się odpala.

use std::path::Path;

use magnat_supply::RelationTuning;
use serde::Deserialize;

/// Wersja schematu pliku. Podnosi się przy każdej zmianie zestawu pól.
pub const RELATIONS_SCHEMA_VERSION: u32 = 1;

/// Kalibracja zmowy cenowej.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub struct CartelParams {
    /// Ilu sprzedawców tego samego towaru w dzielnicy musi być, żeby zmowa
    /// w ogóle miała sens. Poniżej trzech to nie jest kartel, tylko duopol.
    pub min_members: u8,
    /// Sufit liczebności. Powyżej niego tajemnicy nie da się utrzymać, a `SmallVec`
    /// zmowy ma stałą pojemność.
    pub max_members: u8,
    /// Próg skłonności do ryzyka (0..=100), od którego firma wchodzi w zmowę.
    pub risk_threshold: u8,
    /// O ile procent (w punktach bazowych) ponad medianę rynkową idzie cena
    /// minimalna kartelu. To jest cała jego treść: podnieść cenę i utrzymać ją.
    pub floor_markup_bp: u16,
    /// Hazard bazowy wykrycia, w częściach na milion na miesiąc.
    pub base_hazard_ppm: u32,
    /// Ile hazardu dokłada każdy członek ponad `min_members`.
    pub member_hazard_ppm: u32,
    /// Ile hazardu dokłada każde 10 % odchylenia ceny od benchmarku.
    pub deviation_hazard_ppm: u32,
    /// Ile hazardu dokłada każdy niezadowolony wtajemniczony.
    pub insider_hazard_ppm: u32,
    /// Karencja po wykryciu, w miesiącach gry (`K-1`: miesiąc = 30 dób).
    pub cooldown_months: u16,
    /// O ile punktów spada sympatia do marki członka po ujawnieniu zmowy.
    pub brand_drop: u8,
    /// Siła dowodów przekazywanych urzędowi antymonopolowemu przy wykryciu.
    pub evidence: u8,
}

/// Kalibracja związków zawodowych i strajków.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub struct UnionParams {
    /// Poziom żalu, powyżej którego załoga zaczyna się organizować.
    pub grievance_threshold: u8,
    /// Ile kolejnych dób żal musi się utrzymać.
    pub grievance_days: u16,
    /// Najmniejsza spójna składowa grafu relacji w załodze.
    pub min_component: u16,
    /// …albo tyle procent załogi, jeśli to więcej (w punktach bazowych).
    pub component_share_bp: u16,
    /// Najmniejsza gęstość potencjalnego członkostwa, w punktach bazowych.
    pub density_bp: u16,
    /// Wagi członów żalu: zysk na pracownika, luka płacowa, stres. Sumują się
    /// do 100 i walidator tego pilnuje — inaczej skala 0..100 przestaje być skalą.
    pub w_profit: u8,
    pub w_wage: u8,
    pub w_stress: u8,
    /// Ile dób trwa jedna runda negocjacji.
    pub round_days: u16,
    /// Po ilu rundach bez ugody zaczyna się strajk.
    pub max_rounds: u8,
    /// Sufit żądania płacowego w punktach bazowych.
    pub demand_cap_bp: u16,
    /// Sufit ustępstwa firmy w punktach bazowych.
    pub concession_cap_bp: u16,
    /// Jaka część załogi wychodzi przy pełnej wojowniczości, w punktach bazowych.
    pub participation_bp: u16,
    /// Twardy sufit długości strajku w dobach — bezpiecznik, nie mechanizm.
    /// Rozstrzygać ma fundusz; ten próg łapie wyłącznie przypadek, w którym
    /// załoga nie ma nic do stracenia, bo nie ma nic.
    pub max_strike_days: u16,
    /// Ile miesięcy po ugodzie związek nie wraca z nowym żądaniem.
    pub truce_months: u16,
}

/// Cały plik.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RelationsTuning {
    pub supplier: RelationTuning,
    pub cartel: CartelParams,
    pub union: UnionParams,
}

#[derive(Deserialize)]
struct Plik {
    schema_version: u32,
    supplier: RelationTuning,
    cartel: CartelParams,
    union: UnionParams,
}

/// Co może pójść nie tak przy wczytywaniu.
#[derive(Debug)]
pub enum RelationsError {
    Io(std::io::Error),
    Ron(String),
    Schema { found: u32, want: u32 },
    Zero(&'static str),
    Weights(u32),
}

impl std::fmt::Display for RelationsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RelationsError::Io(e) => write!(f, "relations.ron: {e}"),
            RelationsError::Ron(e) => write!(f, "relations.ron: {e}"),
            RelationsError::Schema { found, want } => {
                write!(
                    f,
                    "relations.ron: schema_version {found}, oczekiwano {want}"
                )
            }
            RelationsError::Zero(p) => write!(f, "relations.ron: pole `{p}` nie może być zerem"),
            RelationsError::Weights(s) => write!(
                f,
                "relations.ron: wagi żalu sumują się do {s}, a mają do 100 — \
                 inaczej skala 0..100 przestaje być skalą"
            ),
        }
    }
}

impl std::error::Error for RelationsError {}

impl RelationsTuning {
    /// Wczytuje i waliduje plik.
    pub fn load(path: &Path) -> Result<RelationsTuning, RelationsError> {
        let txt = std::fs::read_to_string(path).map_err(RelationsError::Io)?;
        let p: Plik = ron::from_str(&txt).map_err(|e| RelationsError::Ron(e.to_string()))?;
        if p.schema_version != RELATIONS_SCHEMA_VERSION {
            return Err(RelationsError::Schema {
                found: p.schema_version,
                want: RELATIONS_SCHEMA_VERSION,
            });
        }
        if p.supplier.trust_window == 0 {
            return Err(RelationsError::Zero("supplier.trust_window"));
        }
        if p.supplier.forget_days == 0 {
            return Err(RelationsError::Zero("supplier.forget_days"));
        }
        if p.cartel.min_members < 2 {
            return Err(RelationsError::Zero("cartel.min_members"));
        }
        if p.cartel.cooldown_months == 0 {
            return Err(RelationsError::Zero("cartel.cooldown_months"));
        }
        if p.union.round_days == 0 {
            return Err(RelationsError::Zero("union.round_days"));
        }
        if p.union.max_strike_days == 0 {
            return Err(RelationsError::Zero("union.max_strike_days"));
        }
        let suma =
            u32::from(p.union.w_profit) + u32::from(p.union.w_wage) + u32::from(p.union.w_stress);
        if suma != 100 {
            return Err(RelationsError::Weights(suma));
        }
        Ok(RelationsTuning {
            supplier: p.supplier,
            cartel: p.cartel,
            union: p.union,
        })
    }

    /// Plik z katalogu danych gry.
    pub fn load_default() -> Result<RelationsTuning, RelationsError> {
        RelationsTuning::load(&magnat_core::assets::data_path("tuning/relations.ron"))
    }
}

impl Default for RelationsTuning {
    /// Wartości domyślne **tożsame z plikiem w repozytorium** — pilnuje tego test
    /// niżej. Istnieją po to, żeby test jednostkowy nie musiał czytać dysku,
    /// a nie po to, żeby plik był opcjonalny.
    fn default() -> RelationsTuning {
        RelationsTuning {
            supplier: RelationTuning::default(),
            cartel: CartelParams {
                min_members: 3,
                max_members: 8,
                risk_threshold: 55,
                floor_markup_bp: 1_500,
                base_hazard_ppm: 5_000,
                member_hazard_ppm: 3_000,
                deviation_hazard_ppm: 20_000,
                insider_hazard_ppm: 2_000,
                cooldown_months: 24,
                brand_drop: 20,
                evidence: 60,
            },
            union: UnionParams {
                grievance_threshold: 55,
                grievance_days: 60,
                min_component: 8,
                component_share_bp: 2_500,
                density_bp: 3_000,
                w_profit: 40,
                w_wage: 40,
                w_stress: 20,
                round_days: 7,
                max_rounds: 4,
                demand_cap_bp: 2_500,
                concession_cap_bp: 1_200,
                participation_bp: 9_000,
                max_strike_days: 90,
                truce_months: 12,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plik_gry_wczytuje_sie_i_zgadza_sie_z_domyslnymi() {
        let t = RelationsTuning::load_default().expect("data/tuning/relations.ron");
        assert_eq!(
            t,
            RelationsTuning::default(),
            "plik w repozytorium rozjechał się z wartościami domyślnymi — \
             test jednostkowy mierzyłby wtedy inną grę niż ta, która się uruchamia"
        );
    }
}
