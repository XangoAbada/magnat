//! Produktywność pracownika i zakładu (M7a WP3, M7 §5.3, PRD §6.6).
//!
//! **Jednostka.** `Qty` jest tu **milietatem**: 1000 znaczy jeden pełny etat pracujący
//! ze sprawnością odniesienia. Zakład o dwudziestu ludziach w dobrej formie da około
//! 20 000; ten sam zakład z chorą i zestresowaną załogą wyraźnie mniej. M6 mnoży przez
//! to przepustowość linii i **nie zagląda do środka** — kontrakt to jedna funkcja
//! (M7 §8, ryzyko `R8`).
//!
//! Wszystko w liczbach całkowitych: wynik wpływa na stan trwały, więc floata nie ma
//! (dokument 00 §2). Mnożniki są w tysięcznych, dzielenie następuje po każdym z nich
//! osobno — kolejność działań jest częścią wyniku i nie wolno jej „uprościć".

use magnat_agents::{DeprivationPressure, Vitals};
use magnat_core::{Qty, Q};

use super::roles::RoleWeights;

/// Jakość zarządzania zakładem, 0..=100. Właścicielem mechaniki jest M7c
/// (menedżerowie, delegowanie); tu jest wejściem, którego M7a nie wylicza —
/// zakład bez menedżera dostaje [`ManagementQuality::NEUTRAL`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct ManagementQuality(pub u8);

impl ManagementQuality {
    /// Zakład bez menedżera nie jest zakładem źle zarządzanym — jest zarządzanym
    /// przeciętnie. Mnożnik wychodzi wtedy równo 1000.
    pub const NEUTRAL: ManagementQuality = ManagementQuality(50);
}

impl Default for ManagementQuality {
    fn default() -> ManagementQuality {
        ManagementQuality::NEUTRAL
    }
}

/// Jeden milietat — pełny etat o sprawności odniesienia.
pub const FULL_TIME: i64 = 1000;

/// Mnożnik wyposażenia zakładu, w tysięcznych: 700 przy `tech = 0`, **1000 przy 50**,
/// 1300 przy 100.
///
/// Zakres jest szeroki z rozmysłu: różnica między warsztatem a nowoczesną linią
/// ma być czymś, co gracz widzi w wyniku, a nie ozdobą (PRD §6.6).
///
/// **Przesunięte o 100 w dół wobec 800..1400 z M7 §5.3, przy zachowanej rozpiętości
/// 600.** Powód znalazł test: `Q::new(50)` jest wartością domyślną `PlantSite::tech`
/// w M6 i `Site::tech` tutaj, czyli znaczy „wyposażenie przeciętne" — a w skali
/// 800..1400 przeciętne wyposażenie dawało ciche +10% do przepustowości każdego
/// zakładu w mieście. Mnożnik bez punktu neutralnego nie jest mnożnikiem, tylko
/// przesunięciem skali ukrytym w kalibracji.
#[must_use]
pub const fn tech_mult(tech: Q) -> i32 {
    700 + (tech.get() as i32) * 6
}

/// Mnożnik zarządzania, w tysięcznych: 850 przy 0, 1000 przy 50, 1150 przy 100.
///
/// Zakres ≥ ±15% jest wymaganiem, nie kalibracją (M7 §8, ryzyko `R6`): menedżer
/// o mnożniku 0,98–1,02 byłby kosmetyką, a §7.5 PRD obiecuje realny wpływ.
#[must_use]
pub const fn mgmt_mult(mgmt: ManagementQuality) -> i32 {
    850 + (mgmt.0 as i32) * 3
}

/// Nastrój przeskalowany z −100..=100 do 0..=100.
///
/// Jawnie, a nie przez `max(0)`: zły nastrój ma obniżać pracę, a nie jej nie wytwarzać.
/// Człowiek w fatalnym humorze nadal przychodzi i coś robi — po prostu mniej.
#[must_use]
pub const fn mood01(mood: i8) -> Q {
    Q::new(((mood as i32 + 100) / 2) as u8)
}

/// Ile pracy najwyżej zabiera deprywacja, w promilach (`R2-WP16`).
///
/// Sufit jest jawny i to jest jego treść: człowiek głodny, niewyspany i chory naraz
/// pracuje **gorzej**, ale przychodzi i coś robi. Bez sufitu wystarczyłoby dosypać
/// skutków w danych, żeby zakład stanął, nie tracąc ani jednego pracownika — a stanie
/// zakładu ma mieć przyczynę, którą widać w obsadzie, nie w tabeli potrzeb.
pub const MAX_DEPRIVATION_LOSS_PERMILLE: u32 = 700;

/// Mnożnik deprywacji, w tysięcznych: 1000 bez deprywacji, 300 przy pełnym sufycie.
///
/// To jest **drugi** kanał, nie ten sam, którym deprywacja już działa: głód zjada
/// energię przez `EnergyLoss` i to wchodzi do `body`, a `ProductivityLoss` mówi, o ile
/// gorzej pracuje człowiek myślący o jedzeniu. Dane deklarują oba osobno od M3a.
#[must_use]
pub const fn deprivation_mult(productivity_loss_permille: u32) -> i32 {
    let strata = if productivity_loss_permille > MAX_DEPRIVATION_LOSS_PERMILLE {
        MAX_DEPRIVATION_LOSS_PERMILLE
    } else {
        productivity_loss_permille
    };
    1000 - strata as i32
}

/// Produktywność jednego pracownika w milietatach (M7 §5.3).
///
/// Składniki idą w kolejności pól [`RoleWeights`], mnożniki po kolei i każdy z własnym
/// dzieleniem — dwa dzielenia dają inny wynik niż jedno przez iloczyn i to **to**
/// jest wynik kontraktowy. Deprywacja wchodzi **po** wyposażeniu i zarządzaniu, bo
/// dotyczy człowieka, a nie zakładu.
#[must_use]
pub fn effective_labor(
    body: &Vitals,
    skill: Q,
    tech: Q,
    mgmt: ManagementQuality,
    w: &RoleWeights,
    dep: DeprivationPressure,
) -> Qty {
    let base = i64::from(w.base(skill, body.energy_q(), mood01(body.mood), body.health_q()));
    // base ∈ 0..=100 → milietaty przed mnożnikami.
    let mut out = base * 10;
    out = out * i64::from(tech_mult(tech)) / 1000;
    out = out * i64::from(mgmt_mult(mgmt)) / 1000;
    out = out * i64::from(deprivation_mult(dep.productivity_loss_permille)) / 1000;
    Qty(out)
}

/// Mnożnik strat magazynowych i produkcyjnych zakładu, w tysięcznych (kontrakt do M6,
/// M7 §6, decyzja otwarta `D13` fazy).
///
/// Dobry menedżer **obniża** straty, zły je podnosi — kierunek jest odwrotny niż
/// przy produktywności i to jest cała treść tej funkcji. 1150 przy zarządzaniu zerowym,
/// 850 przy doskonałym.
#[must_use]
pub const fn loss_multiplier(mgmt: ManagementQuality) -> i32 {
    1150 - (mgmt.0 as i32) * 3
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wagi() -> RoleWeights {
        RoleWeights {
            skill: 400,
            energy: 300,
            mood: 100,
            health: 200,
        }
    }

    fn zdrowy() -> Vitals {
        Vitals {
            health: 100,
            energy: 100,
            mood: 100,
            stress: 0,
            edu_level: 0,
            edu_field: 0,
            status: 50,
            _pad: 0,
        }
    }

    #[test]
    fn wzorcowy_pracownik_daje_dokladnie_pelny_etat() {
        // Umiejętność 100, forma 100, wyposażenie przeciętne, zarządzanie przeciętne
        // → dokładnie jeden etat, co do jednostki. To jest punkt odniesienia całej
        // skali i dlatego jest tu `assert_eq!`, a nie tolerancja: gdyby któryś
        // z mnożników przestał mieć punkt neutralny, cała gospodarka dostałaby
        // cichą korektę przepustowości, której nikt by nie zauważył.
        let q = effective_labor(
            &zdrowy(),
            Q::new(100),
            Q::new(50),
            ManagementQuality::NEUTRAL,
            &wagi(),
            DeprivationPressure::default(),
        );
        assert_eq!(q.0, FULL_TIME);
        assert_eq!(deprivation_mult(0), 1000);
        assert_eq!(tech_mult(Q::new(50)), 1000);
        assert_eq!(mgmt_mult(ManagementQuality::NEUTRAL), 1000);
    }

    #[test]
    fn zly_nastroj_nie_wytwarza_pracy_ujemnej() {
        let mut v = zdrowy();
        v.mood = -100;
        let q = effective_labor(
            &v,
            Q::new(100),
            Q::new(50),
            ManagementQuality::NEUTRAL,
            &wagi(),
            DeprivationPressure::default(),
        );
        assert!(q.0 > 0, "q = {}", q.0);
    }

    #[test]
    fn deprywacja_obniza_prace_ale_jej_nie_gasi() {
        let pelna = effective_labor(
            &zdrowy(),
            Q::new(100),
            Q::new(50),
            ManagementQuality::NEUTRAL,
            &wagi(),
            DeprivationPressure::default(),
        );
        let glodny = effective_labor(
            &zdrowy(),
            Q::new(100),
            Q::new(50),
            ManagementQuality::NEUTRAL,
            &wagi(),
            DeprivationPressure {
                productivity_loss_permille: 300,
                ambition_gain_permille: 0,
            },
        );
        assert_eq!(glodny.0, pelna.0 * 700 / 1000);
        // Sufit: dosypanie skutków w danych nie zgasi pracownika do zera. Teza jest
        // o **pracy**, nie o mnożniku, więc mierzymy pracę — mnożnik sprawdzony obok
        // powtarzałby definicję zapisaną trzydzieści linii wyżej.
        let skrajny = DeprivationPressure {
            productivity_loss_permille: 5_000,
            ambition_gain_permille: 0,
        };
        let na_dnie = effective_labor(
            &zdrowy(),
            Q::new(100),
            Q::new(50),
            ManagementQuality::NEUTRAL,
            &wagi(),
            skrajny,
        );
        assert!(na_dnie.0 > 0, "skrajna deprywacja zgasiła pracę do zera");
        assert!(na_dnie.0 < glodny.0, "sufit nie jest sufitem");
        assert_eq!(na_dnie.0, pelna.0 * 300 / 1000);
    }

    #[test]
    fn menedzer_dziala_monotonicznie() {
        // §7.4 fazy: lepszy menedżer nie może dać gorszego wyniku ani wyższych strat.
        let mut poprzedni = i64::MIN;
        let mut poprzednie_straty = i32::MAX;
        for m in 0..=100u8 {
            let mq = ManagementQuality(m);
            let q = effective_labor(
                &zdrowy(),
                Q::new(60),
                Q::new(50),
                mq,
                &wagi(),
                DeprivationPressure::default(),
            );
            assert!(q.0 >= poprzedni, "produktywność spadła przy mgmt = {m}");
            let s = loss_multiplier(mq);
            assert!(s <= poprzednie_straty, "straty wzrosły przy mgmt = {m}");
            poprzedni = q.0;
            poprzednie_straty = s;
        }
        // Zakres ≥ ±15% (ryzyko R6): menedżer ma być czuć.
        assert_eq!(mgmt_mult(ManagementQuality(0)), 850);
        assert_eq!(mgmt_mult(ManagementQuality(100)), 1150);
        // Wyposażenie też ma punkt neutralny i symetryczny zakres wokół niego.
        assert_eq!(tech_mult(Q::new(0)), 700);
        assert_eq!(tech_mult(Q::new(100)), 1300);
    }

    #[test]
    fn skala_nastroju_jest_liniowa() {
        assert_eq!(mood01(-100).get(), 0);
        assert_eq!(mood01(0).get(), 50);
        assert_eq!(mood01(100).get(), 100);
    }
}
