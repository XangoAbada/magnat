//! Jakość wykonania polityki przez menedżera (M9d §5.6 „Wykonanie i menedżer", WP9).
//!
//! # Dlaczego to stoi w `sim/economy`, a nie w `game/`
//!
//! Dokument M9 §6 wpisywał `ManagerExecution` do `game::policy`. To jest niewykonalne
//! i wychodzi przy pierwszej próbie: politykę wykonuje [`crate::Market::run_policies`]
//! wołane z systemu ECS, a `sim/economy` **nie zależy i nie może zależeć** od `game/` —
//! zależność idzie w drugą stronę. Jakość wykonania jest odchyleniem **wewnątrz** tego
//! kroku, więc mieszka tam, gdzie krok. W `game/` zostaje to, co naprawdę jest warstwą
//! gracza: edytor reguł, dry-run i diagnostyka.
//!
//! Odwzorowanie umiejętność → jakość wykonania jest przy tym **daną**, nie kodem
//! (decyzja otwarta nr 5 fazy M9, przyjęta wg propozycji domyślnej): krzywa siedzi
//! w `data/tuning/policy.ron` i stroi ją balansator. To jest ta sama reguła, którą
//! `K-35` zastosował do progów kaskady niedoboru — liczby, które wolno przestawić
//! bez zmiany znaczenia modelu, nie należą do kodu.
//!
//! # Cztery wejścia i tylko dwa losowania
//!
//! Menedżer psuje wykonanie na cztery sposoby, ale losowe są z nich dwa:
//!
//! | wejście | skąd się bierze | losuje? |
//! |---|---|---|
//! | `info_lag_days` | wiek obrazu konkurencji ([`crate::CompetitorSnapshot::delay_days`]) | nie |
//! | `reaction_delay_h` | wydłużenie martwej strefy polityki | nie |
//! | `exec_error_bp` | rzut `StreamId::PolicyExecution` | **tak** |
//! | `skip_chance_bp` | rzut `StreamId::PolicyExecution` | **tak** |
//!
//! Opóźnienie informacji **nie dostaje własnego mechanizmu**: obraz konkurencji
//! z opóźnieniem 1–7 dni istnieje od M5c i to on jest tą liczbą. Drugi licznik wieku
//! obok pierwszego rozjechałby się przy pierwszej zmianie — to ta sama zasada, dla
//! której wykonawca akcji liczy wyrażenia ewaluatorem, a nie własną arytmetyką.

use magnat_core::{rng, Money, StreamId, Tick, Q};
use serde::Deserialize;
use std::path::Path;

/// Wersja schematu `data/tuning/policy.ron`.
pub const POLICY_TUNING_SCHEMA_VERSION: u32 = 1;

/// Krzywa „umiejętność → jakość wykonania". Dwa krańce na każde wejście; między
/// nimi interpolacja liniowa całkowitoliczbowa.
///
/// Krańce są nazwane `worst` (umiejętność 0) i `best` (umiejętność 100), a nie
/// `min`/`max`, bo trzy z czterech wejść maleją wraz z umiejętnością i para
/// „min/max" kazałaby przy każdym czytaniu sprawdzać, w którą stronę.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct ManagerCurve {
    /// Umiejętność zakładu, którym nikt nie zarządza — „bez nadzoru" z §5.6.
    ///
    /// Zakład zdelegowany bez menedżera (zastępstwo po odejściu) i zakład prowadzony
    /// przez gracza, który nie stoi w nim na zmianie, dostają tę samą podłogę. To jest
    /// mechaniczna motywacja do zatrudnienia menedżera — koszt, a nie bramka.
    pub unsupervised_skill: u8,
    pub info_lag_days_worst: u8,
    pub info_lag_days_best: u8,
    pub reaction_delay_h_worst: u8,
    pub reaction_delay_h_best: u8,
    pub exec_error_bp_worst: i32,
    pub exec_error_bp_best: i32,
    pub skip_chance_bp_worst: i32,
    pub skip_chance_bp_best: i32,
}

/// Kalibracja wykonania polityk. Zasób świata; stawia go `game::world`.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct PolicyTuning {
    pub schema_version: u32,
    pub manager: ManagerCurve,
}

#[derive(Debug)]
pub enum PolicyTuningError {
    Io(String),
    Parse(String),
    Schema {
        found: u32,
        want: u32,
    },
    /// Kraniec „najlepszy" gorszy od krańca „najgorszego". Plik da się sparsować,
    /// a symulacja byłaby taka, w której lepszy menedżer myli się częściej — czyli
    /// dokładnie ten błąd, którego szuka test monotoniczności, tylko znaleziony
    /// przy ładowaniu zamiast po pięciu latach przebiegu.
    NotMonotonic(&'static str),
    /// Liczba poza sensownym rzędem wielkości. Sprawdzane przy ładowaniu, bo krzywa
    /// jest **daną**, a dana z literówką („400000" zamiast „400") przechodzi parser
    /// i dopiero w interpolacji przepełnia `i32`. Sufit jest hojny — chodzi o rząd
    /// wielkości, nie o kalibrację, którą stroi balansator.
    OutOfRange(&'static str),
}

impl std::fmt::Display for PolicyTuningError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PolicyTuningError::Io(e) | PolicyTuningError::Parse(e) => {
                write!(f, "data/tuning/policy.ron: {e}")
            }
            PolicyTuningError::Schema { found, want } => write!(
                f,
                "data/tuning/policy.ron: schema_version {found}, oczekiwano {want}"
            ),
            PolicyTuningError::NotMonotonic(co) => {
                write!(f, "data/tuning/policy.ron: „{co}” rośnie z umiejętnością")
            }
            PolicyTuningError::OutOfRange(co) => {
                write!(f, "data/tuning/policy.ron: „{co}” poza zakresem")
            }
        }
    }
}

impl std::error::Error for PolicyTuningError {}

impl PolicyTuning {
    /// # Errors
    /// Brak pliku, zły RON, niezgodna wersja schematu albo krzywa niemonotoniczna.
    pub fn load_default() -> Result<PolicyTuning, PolicyTuningError> {
        PolicyTuning::load(&magnat_core::data_path("tuning/policy.ron"))
    }

    /// # Errors
    /// Jak [`PolicyTuning::load_default`].
    pub fn load(path: &Path) -> Result<PolicyTuning, PolicyTuningError> {
        let txt =
            std::fs::read_to_string(path).map_err(|e| PolicyTuningError::Io(e.to_string()))?;
        PolicyTuning::parse(&txt)
    }

    /// # Errors
    /// Jak [`PolicyTuning::load_default`].
    pub fn parse(txt: &str) -> Result<PolicyTuning, PolicyTuningError> {
        let t: PolicyTuning =
            ron::from_str(txt).map_err(|e| PolicyTuningError::Parse(e.to_string()))?;
        if t.schema_version != POLICY_TUNING_SCHEMA_VERSION {
            return Err(PolicyTuningError::Schema {
                found: t.schema_version,
                want: POLICY_TUNING_SCHEMA_VERSION,
            });
        }
        let c = &t.manager;
        if c.info_lag_days_best > c.info_lag_days_worst {
            return Err(PolicyTuningError::NotMonotonic("info_lag_days"));
        }
        if c.reaction_delay_h_best > c.reaction_delay_h_worst {
            return Err(PolicyTuningError::NotMonotonic("reaction_delay_h"));
        }
        if c.exec_error_bp_best > c.exec_error_bp_worst {
            return Err(PolicyTuningError::NotMonotonic("exec_error_bp"));
        }
        if c.skip_chance_bp_best > c.skip_chance_bp_worst {
            return Err(PolicyTuningError::NotMonotonic("skip_chance_bp"));
        }
        // Rzędy wielkości. Dziesięć tysięcy punktów bazowych to sto procent: menedżer
        // mylący się o więcej niż całą wartość albo przepuszczający więcej niż każdy
        // cykl nie jest kalibracją, tylko literówką.
        if c.unsupervised_skill > 100 {
            return Err(PolicyTuningError::OutOfRange("unsupervised_skill"));
        }
        if c.info_lag_days_worst > 7 || c.info_lag_days_best == 0 {
            return Err(PolicyTuningError::OutOfRange("info_lag_days"));
        }
        if c.exec_error_bp_worst > 10_000 || c.exec_error_bp_best < 0 {
            return Err(PolicyTuningError::OutOfRange("exec_error_bp"));
        }
        if c.skip_chance_bp_worst > 10_000 || c.skip_chance_bp_best < 0 {
            return Err(PolicyTuningError::OutOfRange("skip_chance_bp"));
        }
        Ok(t)
    }
}

/// Jakość wykonania jednego menedżera.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ManagerExecution {
    pub skill: Q,
    /// Ile dni ma obraz konkurencji, na którym menedżer pracuje (1..=7).
    pub info_lag_days: u8,
    /// O ile godzin wydłuża się martwa strefa polityki.
    pub reaction_delay_h: u8,
    /// Rozrzut wykonania w punktach bazowych: wynik ląduje w `±exec_error_bp`.
    pub exec_error_bp: i32,
    /// Szansa pominięcia całego cyklu, w punktach bazowych.
    pub skip_chance_bp: i32,
}

impl ManagerExecution {
    /// Odwzorowanie umiejętności na jakość wykonania — **jedno, deterministyczne,
    /// bez floatów** (M9d §5.6).
    ///
    /// Sygnatura z dokumentu brzmi `from_skill(skill: Q) -> Self`; krzywa jest daną,
    /// więc trzeba ją podać. To jest cała różnica i jest ona konsekwencją decyzji
    /// otwartej nr 5, a nie zmianą modelu.
    #[must_use]
    pub fn from_skill(skill: Q, c: &ManagerCurve) -> ManagerExecution {
        let s = i32::from(skill.get().min(100));
        let lerp = |worst: i32, best: i32| worst + (best - worst) * s / 100;
        ManagerExecution {
            skill,
            info_lag_days: lerp(
                i32::from(c.info_lag_days_worst),
                i32::from(c.info_lag_days_best),
            )
            .clamp(1, 7) as u8,
            reaction_delay_h: lerp(
                i32::from(c.reaction_delay_h_worst),
                i32::from(c.reaction_delay_h_best),
            )
            .clamp(0, 255) as u8,
            exec_error_bp: lerp(c.exec_error_bp_worst, c.exec_error_bp_best).max(0),
            skip_chance_bp: lerp(c.skip_chance_bp_worst, c.skip_chance_bp_best).max(0),
        }
    }

    /// Wykonanie bez odchylenia — świat bez kalibracji menedżera i punkt odniesienia
    /// dry-runu (§7: „dry-run zgodny z wykonaniem przy `skill = 100`, tolerancja 0").
    #[must_use]
    pub const fn flawless() -> ManagerExecution {
        ManagerExecution {
            skill: Q::new(100),
            info_lag_days: 1,
            reaction_delay_h: 0,
            exec_error_bp: 0,
            skip_chance_bp: 0,
        }
    }

    /// Czy menedżer pominął ten cykl.
    ///
    /// Losuje `StreamId::PolicyExecution` po kluczu `(indeks menedżera, tick)` —
    /// nigdy po czasie rzeczywistym i nigdy po kolejności zakładów w pętli, bo
    /// wtedy dopisanie sklepu zmieniałoby decyzje wszystkich pozostałych.
    #[must_use]
    pub fn skips(&self, world_seed: u64, key: u32, t: Tick) -> bool {
        if self.skip_chance_bp <= 0 {
            return false;
        }
        let mut r = rng(world_seed, StreamId::PolicyExecution, key, t);
        i64::from(r.gen_range_u32(10_000)) < i64::from(self.skip_chance_bp)
    }

    /// Odchylenie wykonania w punktach bazowych, z przedziału `[-e, +e]`.
    ///
    /// Drugi rzut z tego samego strumienia i tego samego klucza — pierwszy poszedł
    /// na pominięcie cyklu. Kolejność rzutów jest kontraktem: zamiana miejscami
    /// zmieniłaby każdą cenę w każdym zdelegowanym zakładzie każdego świata.
    #[must_use]
    pub fn error_bp(&self, world_seed: u64, key: u32, t: Tick) -> i32 {
        if self.exec_error_bp <= 0 {
            return 0;
        }
        let mut r = rng(world_seed, StreamId::PolicyExecution, key, t);
        let _pominiecie = r.gen_range_u32(10_000);
        let rozpietosc = self.exec_error_bp.unsigned_abs() * 2 + 1;
        r.gen_range_u32(rozpietosc) as i32 - self.exec_error_bp
    }

    /// Kwota po odchyleniu menedżera. Mnożenie przez `i128` i zaokrąglenie od zera —
    /// ta sama droga, którą liczy cała reszta polityki (00 §2).
    #[must_use]
    pub fn distort(&self, v: Money, err_bp: i32) -> Money {
        if err_bp == 0 {
            return v;
        }
        v.mul_ratio(i64::from(10_000 + err_bp), 10_000)
    }

    /// Liczba całkowita po odchyleniu — marża w punktach bazowych i liczba dób
    /// pokrycia w zamówieniu.
    #[must_use]
    pub fn distort_i32(&self, v: i32, err_bp: i32) -> i32 {
        if err_bp == 0 {
            return v;
        }
        // Nasycenie, nie zawinięcie: marża w punktach bazowych po odchyleniu może
        // wyjść poza `i32`, jeśli polityka podała absurdalną wartość, a cicha zmiana
        // znaku byłaby ceną ujemną przebraną za dodatnią.
        i32::try_from(
            Money(i64::from(v))
                .mul_ratio(i64::from(10_000 + err_bp), 10_000)
                .get(),
        )
        .unwrap_or(if v >= 0 { i32::MAX } else { i32::MIN })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn krzywa() -> ManagerCurve {
        PolicyTuning::load_default()
            .expect("data/tuning/policy.ron")
            .manager
    }

    #[test]
    fn plik_z_repozytorium_sie_laduje_i_jest_monotoniczny() {
        let c = krzywa();
        assert!(c.info_lag_days_worst <= 7 && c.info_lag_days_best >= 1);
        // Krzywa ma być monotoniczna na całej długości, nie tylko na krańcach.
        let mut poprzedni = ManagerExecution::from_skill(Q::new(0), &c);
        for s in 1..=100u8 {
            let e = ManagerExecution::from_skill(Q::new(s), &c);
            assert!(e.info_lag_days <= poprzedni.info_lag_days, "skill {s}");
            assert!(
                e.reaction_delay_h <= poprzedni.reaction_delay_h,
                "skill {s}"
            );
            assert!(e.exec_error_bp <= poprzedni.exec_error_bp, "skill {s}");
            assert!(e.skip_chance_bp <= poprzedni.skip_chance_bp, "skill {s}");
            poprzedni = e;
        }
    }

    #[test]
    fn menedzer_doskonaly_nie_odchyla_niczego() {
        let e = ManagerExecution::from_skill(Q::new(100), &krzywa());
        assert_eq!(e.exec_error_bp, 0);
        assert_eq!(e.skip_chance_bp, 0);
        assert_eq!(e.reaction_delay_h, 0);
        assert_eq!(e.error_bp(1, 7, Tick(100)), 0);
        assert!(!e.skips(1, 7, Tick(100)));
        assert_eq!(e.distort(Money(638), 0), Money(638));
    }

    #[test]
    fn odchylenie_jest_powtarzalne_i_miesci_sie_w_widelkach() {
        let e = ManagerExecution::from_skill(Q::new(20), &krzywa());
        assert!(e.exec_error_bp > 0, "słaby menedżer ma się mylić");
        for t in 1..200u64 {
            let a = e.error_bp(7, 42, Tick(t));
            let b = e.error_bp(7, 42, Tick(t));
            assert_eq!(a, b, "dwa odczyty tego samego rzutu");
            assert!(a.abs() <= e.exec_error_bp, "tick {t}: {a} bp");
        }
    }

    #[test]
    fn pominiecia_sa_rzadsze_u_lepszego_menedzera() {
        let c = krzywa();
        let ile = |skill: u8| {
            let e = ManagerExecution::from_skill(Q::new(skill), &c);
            (0..2_000u64).filter(|t| e.skips(3, 11, Tick(*t))).count()
        };
        let slaby = ile(10);
        let dobry = ile(80);
        assert!(slaby > dobry, "słaby {slaby}, dobry {dobry}");
    }

    #[test]
    fn niemonotoniczna_krzywa_nie_wchodzi() {
        let zly = std::fs::read_to_string(magnat_core::data_path("tuning/policy.ron"))
            .expect("data/tuning/policy.ron")
            .replace("exec_error_bp_best: 0", "exec_error_bp_best: 9000");
        assert!(matches!(
            PolicyTuning::parse(&zly),
            Err(PolicyTuningError::NotMonotonic("exec_error_bp"))
        ));
    }
}
