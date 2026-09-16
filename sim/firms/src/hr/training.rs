//! Szkolenia, premie i świadczenia pozapłacowe (M7b WP6, PRD §7.5).
//!
//! Wszystko w liczbach całkowitych i wszystko jako funkcje czyste: decyzję „komu
//! i kiedy" podejmuje system doby, tutaj są same reguły i one muszą dać się sprawdzić
//! na kartce.
//!
//! **Świadczenie jest realnym zaspokojeniem potrzeby, a nie liczbą dodaną do nastroju**
//! — to jest wprost kryterium WP6. Opieka medyczna podnosi pracownikowi potrzebę
//! `Health`, posiłki `Hunger`, auto służbowe `Mobility`; firma płaci za to co miesiąc
//! z tego samego rachunku, z którego płaci pensje.

use magnat_core::{Money, NeedKind, Q};

use super::employment::BenefitSet;
use super::tuning::{BenefitTuning, HrTuning};

/// Sufit umiejętności, którego pracownik nie przeskoczy szkoleniem.
///
/// Talentu nie mamy jako osobnego pola i **nie dokładamy go**: rolę granicy pełni
/// wykształcenie, które M3 już prowadzi. To jest przybliżenie i ma nim zostać — punkt
/// wyjścia dla M10 (R&D i uczenie się organizacji) jest tutaj, a nie w nowym polu
/// komponentu mieszkańca.
#[must_use]
pub fn skill_ceiling(education: u8, t: &HrTuning) -> Q {
    let sufit = u32::from(t.talent_base) + u32::from(education) * u32::from(t.talent_step);
    Q::new(sufit.min(100) as u8)
}

/// Umiejętność po odbytym szkoleniu — przyrost z sufitem od talentu.
#[must_use]
pub fn trained_skill(skill: Q, education: u8, t: &HrTuning) -> Q {
    let sufit = skill_ceiling(education, t).get();
    if skill.get() >= sufit {
        return skill;
    }
    Q::new(skill.get().saturating_add(t.training_gain).min(sufit))
}

/// Koszt jednego szkolenia dla firmy: część stawki miesięcznej szkolonego.
#[must_use]
pub fn training_cost(wage_month: Money, t: &HrTuning) -> Money {
    Money(
        wage_month
            .get()
            .saturating_mul(i64::from(t.training_cost_bp))
            / 10_000,
    )
}

/// Premia miesięczna — funkcja oceny wyniku, rosnąca liniowo od progu.
///
/// Zero poniżej progu, `bonus_max_bp` stawki przy ocenie doskonałej. Wynik zakładu
/// wejdzie tu jako drugi mnożnik w M7c, kiedy premia stanie się polityką, a nie
/// tabelą — dziś nikt tej polityki nie zapisuje (`AR-6`).
#[must_use]
pub fn bonus(perf_ema: u16, wage_month: Money, t: &HrTuning) -> Money {
    if perf_ema <= t.bonus_perf_threshold {
        return Money::ZERO;
    }
    let ponad = i64::from(perf_ema - t.bonus_perf_threshold);
    let zakres = i64::from(1_000 - t.bonus_perf_threshold).max(1);
    let bp = i64::from(t.bonus_max_bp) * ponad / zakres;
    Money(wage_month.get().saturating_mul(bp) / 10_000)
}

/// Miesięczny koszt świadczeń pracownika dla firmy.
#[must_use]
pub fn benefit_cost(b: BenefitSet, wage_month: Money, t: &BenefitTuning) -> Money {
    let mut bp = 0i64;
    if b.has(BenefitSet::HEALTH) {
        bp += i64::from(t.health_cost_bp);
    }
    if b.has(BenefitSet::MEALS) {
        bp += i64::from(t.meals_cost_bp);
    }
    if b.has(BenefitSet::COMPANY_CAR) {
        bp += i64::from(t.car_cost_bp);
    }
    if b.has(BenefitSet::TRAINING) {
        bp += i64::from(t.training_cost_bp);
    }
    Money(wage_month.get().saturating_mul(bp) / 10_000)
}

/// Które potrzeby i o ile podnosi zestaw świadczeń — **na dobę**.
///
/// Zwraca listę zamiast pojedynczej liczby, bo świadczenia są rozłączne i każde
/// dotyka innej potrzeby; wołający dopisuje je mieszkańcowi w ustalonej kolejności.
#[must_use]
pub fn benefit_gains(b: BenefitSet, t: &BenefitTuning) -> [(NeedKind, u8); 3] {
    [
        (
            NeedKind::Health,
            if b.has(BenefitSet::HEALTH) {
                t.health_gain
            } else {
                0
            },
        ),
        (
            NeedKind::Hunger,
            if b.has(BenefitSet::MEALS) {
                t.meals_gain
            } else {
                0
            },
        ),
        (
            NeedKind::Mobility,
            if b.has(BenefitSet::COMPANY_CAR) {
                t.car_gain
            } else {
                0
            },
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hr::tuning::LaborTuning;

    fn tuning() -> LaborTuning {
        LaborTuning::load_default().expect("data/tuning/labor.ron")
    }

    #[test]
    fn szkolenie_konczy_sie_na_talencie() {
        let t = tuning().hr;
        let sufit = skill_ceiling(1, &t);
        let mut s = Q::new(10);
        for _ in 0..100 {
            s = trained_skill(s, 1, &t);
        }
        assert_eq!(s, sufit, "szkolenie przebiło sufit talentu");
        // Wyższe wykształcenie podnosi sufit, a nie tempo.
        assert!(skill_ceiling(3, &t).get() > sufit.get());
    }

    #[test]
    fn premia_zaczyna_sie_dopiero_nad_progiem() {
        let t = tuning().hr;
        assert_eq!(
            bonus(t.bonus_perf_threshold, Money(400_000), &t),
            Money::ZERO
        );
        let dobra = bonus(t.bonus_perf_threshold + 100, Money(400_000), &t);
        let doskonala = bonus(1_000, Money(400_000), &t);
        assert!(dobra.get() > 0);
        assert!(doskonala.get() > dobra.get());
        // Sufit premii jest sufitem, nie sugestią.
        assert!(doskonala.get() <= 400_000 * i64::from(t.bonus_max_bp) / 10_000);
    }

    #[test]
    fn swiadczenie_dotyka_potrzeby_a_nie_nastroju() {
        let t = tuning().benefits;
        let bez = benefit_gains(BenefitSet::NONE, &t);
        assert!(bez.iter().all(|(_, g)| *g == 0));
        let z_opieka = benefit_gains(BenefitSet(BenefitSet::HEALTH), &t);
        let zdrowie = z_opieka
            .iter()
            .find(|(n, _)| *n == NeedKind::Health)
            .expect("opieka medyczna dotyka zdrowia");
        assert!(zdrowie.1 > 0);
        assert!(benefit_cost(BenefitSet(BenefitSet::HEALTH), Money(400_000), &t).get() > 0);
    }
}
