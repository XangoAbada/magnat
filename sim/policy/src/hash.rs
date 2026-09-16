//! Polityka w funkcji haszującej stan świata (00 §3.6).
//!
//! # Dlaczego całe drzewo, a nie sam numer
//!
//! Pierwsza wersja haszowała `policy.id` i liczbę reguł — i było to za mało w sposób,
//! który objawiłby się dopiero jako desync. Gracz **zmienia politykę w trakcie gry**:
//! wyłącza regułę (`enabled`), przestawia martwą strefę (`cooldown_h`), poprawia próg
//! w warunku. Żadna z tych zmian nie rusza ani numeru, ani długości listy, więc dwa
//! rozjechane światy dawały identyczny hash — czyli dokładnie ten przypadek, po który
//! hash w ogóle istnieje.
//!
//! Koszt jest znikomy i policzalny: hash stanu liczy się co 1000 ticków, polityka ma
//! najwyżej osiem reguł o głębokości wyrażenia ≤ 3, a zdelegowanych zakładów są tysiące,
//! nie miliony. To jest obchód kilkudziesięciu węzłów na zakład raz na dobę gry.
//!
//! **Notatka gracza (`Rule::note`) do hasha nie wchodzi** i to jest jedyne pominięcie:
//! nie wpływa na wykonanie, a wchodzi do zapisu gry osobno, razem z resztą tekstu.

use magnat_core::hash::{HashState, StateHasher};

use crate::ast::{
    Action, ArithOp, Bp, Cadence, CmpOp, ConditionExpr, Expr, GoodRef, Metric, Policy,
    PolicyDomain, Rule, Severity, Unit, Value,
};

impl HashState for Bp {
    #[inline]
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.0 as u32);
    }
}

impl HashState for Unit {
    #[inline]
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(*self as u8);
    }
}

impl HashState for PolicyDomain {
    #[inline]
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(*self as u8);
    }
}

impl HashState for Cadence {
    #[inline]
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(*self as u8);
    }
}

impl HashState for CmpOp {
    #[inline]
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(*self as u8);
    }
}

impl HashState for ArithOp {
    #[inline]
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(*self as u8);
    }
}

impl HashState for Severity {
    #[inline]
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(*self as u8);
    }
}

impl HashState for GoodRef {
    fn hash_state(&self, h: &mut StateHasher) {
        match self {
            GoodRef::This => h.write_u8(0),
            GoodRef::Id(g) => {
                h.write_u8(1);
                g.hash_state(h);
            }
        }
    }
}

impl HashState for Value {
    fn hash_state(&self, h: &mut StateHasher) {
        // Znacznik jednostki **przed** liczbą: `Days(1)` i `Count(1)` to dwie różne
        // wartości i mają dać dwa różne hashe.
        self.unit().hash_state(h);
        h.write_u64(self.raw() as u64);
    }
}

impl HashState for Metric {
    fn hash_state(&self, h: &mut StateHasher) {
        // Dyskryminanta przez `std::mem::discriminant` nie da się zapisać stabilnie
        // między wersjami kompilatora, więc numerujemy jawnie — tak samo jak
        // `DecisionReason::discriminant` w `core`.
        let (tag, good, radius, basis, role) = rozbierz(*self);
        h.write_u8(tag);
        good.hash_state(h);
        h.write_u32(radius);
        h.write_u8(basis);
        h.write_u16(role);
    }
}

/// Metryka rozłożona na pięć liczb — jedno miejsce, w którym stoi jej numeracja.
fn rozbierz(m: Metric) -> (u8, GoodRef, u32, u8, u16) {
    let brak = GoodRef::This;
    let b = |x: magnat_core::PriceBasis| x as u8 + 1;
    match m {
        Metric::Price { good, basis } => (0, good, 0, b(basis), 0),
        Metric::UnitCost(g) => (1, g, 0, 0, 0),
        Metric::Margin(g) => (2, g, 0, 0, 0),
        Metric::CheapestCompetitorPrice {
            good,
            radius_m,
            basis,
        } => (3, good, radius_m, b(basis), 0),
        Metric::AvgCompetitorPrice {
            good,
            radius_m,
            basis,
        } => (4, good, radius_m, b(basis), 0),
        Metric::CompetitorCount { radius_m } => (5, brak, radius_m, 0, 0),
        Metric::Stock(g) => (6, g, 0, 0, 0),
        Metric::StockDays(g) => (7, g, 0, 0, 0),
        Metric::Turnover7d(g) => (8, g, 0, 0, 0),
        Metric::Sales7d(g) => (9, g, 0, 0, 0),
        Metric::DaysToExpiry(g) => (10, g, 0, 0, 0),
        Metric::ShelfGap(g) => (11, g, 0, 0, 0),
        Metric::MachineUtilization => (12, brak, 0, 0, 0),
        Metric::OpenPositions(r) => (13, brak, 0, 0, r.get()),
        Metric::StaffTurnover12m => (14, brak, 0, 0, 0),
        Metric::MedianMarketWage(r) => (15, brak, 0, 0, r.get()),
        Metric::StaffMood => (16, brak, 0, 0, 0),
        Metric::ManagerSkill => (17, brak, 0, 0, 0),
        Metric::CashBalance => (18, brak, 0, 0, 0),
        Metric::Receivables => (19, brak, 0, 0, 0),
        Metric::Season => (20, brak, 0, 0, 0),
        Metric::DayOfWeek => (21, brak, 0, 0, 0),
        Metric::DayOfMonth => (22, brak, 0, 0, 0),
        Metric::HourOfDay => (23, brak, 0, 0, 0),
        Metric::DaysSinceLastChange(g) => (24, g, 0, 0, 0),
    }
}

impl HashState for Expr {
    fn hash_state(&self, h: &mut StateHasher) {
        match self {
            Expr::Lit(v) => {
                h.write_u8(0);
                v.hash_state(h);
            }
            Expr::Metric(m) => {
                h.write_u8(1);
                m.hash_state(h);
            }
            Expr::Bin { lhs, op, rhs } => {
                h.write_u8(2);
                op.hash_state(h);
                lhs.hash_state(h);
                rhs.hash_state(h);
            }
            Expr::Convert { to, of } => {
                h.write_u8(3);
                h.write_u8(*to as u8);
                of.hash_state(h);
            }
        }
    }
}

impl HashState for ConditionExpr {
    fn hash_state(&self, h: &mut StateHasher) {
        match self {
            ConditionExpr::Always => h.write_u8(0),
            ConditionExpr::Not(a) => {
                h.write_u8(1);
                a.hash_state(h);
            }
            ConditionExpr::And(a, b) => {
                h.write_u8(2);
                a.hash_state(h);
                b.hash_state(h);
            }
            ConditionExpr::Or(a, b) => {
                h.write_u8(3);
                a.hash_state(h);
                b.hash_state(h);
            }
            ConditionExpr::Cmp { lhs, op, rhs } => {
                h.write_u8(4);
                op.hash_state(h);
                lhs.hash_state(h);
                rhs.hash_state(h);
            }
        }
    }
}

impl HashState for Action {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(self.kind() as u8);
        match self {
            Action::SetPrice { good, to } | Action::AdjustPrice { good, by: to } => {
                good.hash_state(h);
                to.hash_state(h);
            }
            Action::SetMargin { good, bp } | Action::Markdown { good, bp } => {
                good.hash_state(h);
                bp.hash_state(h);
            }
            Action::ClampPrice { good, min, max } => {
                good.hash_state(h);
                min.hash_state(h);
                max.hash_state(h);
            }
            Action::OrderUpTo { good, days } => {
                good.hash_state(h);
                days.hash_state(h);
            }
            Action::OrderQty { good, qty, source } => {
                good.hash_state(h);
                qty.hash_state(h);
                h.write_u8(*source as u8);
            }
            Action::RemoveFromShelf(good) => good.hash_state(h),
            Action::Hire { role, count, wage } => {
                role.hash_state(h);
                h.write_u16(*count);
                wage.hash_state(h);
            }
            Action::RaiseWage { role, by, cap } => {
                role.hash_state(h);
                by.hash_state(h);
                match cap {
                    None => h.write_u8(0),
                    Some(e) => {
                        h.write_u8(1);
                        e.hash_state(h);
                    }
                }
            }
            Action::PlanProduction { recipe, qty } => {
                recipe.hash_state(h);
                qty.hash_state(h);
            }
            Action::Alert { msg, severity } => {
                h.write_u16(*msg);
                severity.hash_state(h);
            }
            Action::AskPlayer { msg } => h.write_u16(*msg),
        }
    }
}

impl HashState for Rule {
    fn hash_state(&self, h: &mut StateHasher) {
        // `enabled` **przed** warunkiem: wyłączenie reguły nie zmienia niczego innego,
        // a zmienia wszystko w wykonaniu.
        h.write_u8(u8::from(self.enabled));
        self.when.hash_state(h);
        h.write_u32(self.then.len() as u32);
        for a in &self.then {
            a.hash_state(h);
        }
        // `note` świadomie poza hashem: notatka gracza nie wpływa na wykonanie.
    }
}

impl HashState for Policy {
    fn hash_state(&self, h: &mut StateHasher) {
        self.id.hash_state(h);
        self.domain.hash_state(h);
        self.cadence.hash_state(h);
        h.write_u8(self.cooldown_h);
        h.write_u32(self.rules.len() as u32);
        for r in &self.rules {
            r.hash_state(h);
        }
        match &self.fallback {
            None => h.write_u8(0),
            Some(a) => {
                h.write_u8(1);
                a.hash_state(h);
            }
        }
        // Nazwa polityki jest tekstem gracza — tak samo jak nazwa firmy, która
        // do hasha **wchodzi** (`Firm::hash_state`). Tu wchodzi z tego samego powodu:
        // przemianowanie polityki widać w zapisie gry i ma być odtwarzalne.
        h.write_u32(self.name.len() as u32);
        h.write(self.name.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{PolicyDomain, Rule};
    use magnat_core::{hash::StateHasher, GoodId, Money, PolicyId, PriceBasis};

    fn hash(p: &Policy) -> magnat_core::StateHash {
        let mut h = StateHasher::new();
        p.hash_state(&mut h);
        h.finish()
    }

    fn polityka() -> Policy {
        let mut p = Policy::empty(PolicyId(1), "test", PolicyDomain::Pricing);
        p.cooldown_h = 12;
        p.rules.push(Rule {
            when: ConditionExpr::Cmp {
                lhs: Expr::Metric(Metric::Price {
                    good: GoodRef::This,
                    basis: PriceBasis::GrossRetail,
                }),
                op: CmpOp::Gt,
                rhs: Expr::Lit(Value::Money(Money(700))),
            },
            then: smallvec::smallvec![Action::Markdown {
                good: GoodRef::This,
                bp: Bp(500)
            }],
            enabled: true,
            note: String::new(),
        });
        p
    }

    #[test]
    fn kazda_zmiana_polityki_rusza_hash() {
        let baza = hash(&polityka());

        // Wyłączenie reguły **nie zmienia** ani numeru, ani długości listy — i to jest
        // dokładnie przypadek, który poprzednia wersja hasha przepuszczała.
        let mut p = polityka();
        p.rules[0].enabled = false;
        assert_ne!(baza, hash(&p), "wyłączenie reguły nie ruszyło hasha");

        let mut p = polityka();
        p.cooldown_h = 6;
        assert_ne!(baza, hash(&p), "martwa strefa nie ruszyła hasha");

        let mut p = polityka();
        p.cadence = Cadence::Hourly;
        assert_ne!(baza, hash(&p), "kadencja nie ruszyła hasha");

        // Próg w warunku: ta sama struktura, inna liczba.
        let mut p = polityka();
        p.rules[0].when = ConditionExpr::Cmp {
            lhs: Expr::Metric(Metric::Price {
                good: GoodRef::This,
                basis: PriceBasis::GrossRetail,
            }),
            op: CmpOp::Gt,
            rhs: Expr::Lit(Value::Money(Money(701))),
        };
        assert_ne!(baza, hash(&p), "próg w warunku nie ruszył hasha");

        // Podstawa ceny: ta sama liczba, inne znaczenie (`K-7`).
        let mut p = polityka();
        p.rules[0].when = ConditionExpr::Cmp {
            lhs: Expr::Metric(Metric::Price {
                good: GoodRef::This,
                basis: PriceBasis::NetB2B,
            }),
            op: CmpOp::Gt,
            rhs: Expr::Lit(Value::Money(Money(700))),
        };
        assert_ne!(baza, hash(&p), "podstawa ceny nie ruszyła hasha");

        // Akcja zapasowa.
        let mut p = polityka();
        p.fallback = Some(Action::SetMargin {
            good: GoodRef::This,
            bp: Bp(2_500),
        });
        assert_ne!(baza, hash(&p), "akcja zapasowa nie ruszyła hasha");

        // Ten sam stan — ten sam hash.
        assert_eq!(baza, hash(&polityka()));
    }

    #[test]
    fn notatka_gracza_nie_rusza_hasha() {
        // Jedyne pominięcie i jedyne, które ma być pominięciem: notatka nie wpływa
        // na wykonanie, więc dwa światy różniące się wyłącznie nią są tym samym światem.
        let mut p = polityka();
        p.rules[0].note = "przypomnieć sobie, czemu to tu jest".to_owned();
        assert_eq!(hash(&polityka()), hash(&p));
    }

    #[test]
    fn jednostka_jest_czescia_wartosci() {
        // `Days(1)` i `Count(1)` to ta sama liczba i dwa różne warunki.
        let mut a = StateHasher::new();
        Value::Days(1).hash_state(&mut a);
        let mut b = StateHasher::new();
        Value::Count(1).hash_state(&mut b);
        assert_ne!(a.finish(), b.finish());
    }

    #[test]
    fn metryki_nie_zlewaja_sie_w_jedna() {
        // Numeracja w `rozbierz` jest kontraktem hasha: dwie metryki o tym samym
        // ładunku muszą dać dwa różne odciski, inaczej podmiana jednej na drugą
        // przechodzi bez śladu.
        let wszystkie = [
            Metric::Stock(GoodRef::Id(GoodId(1))),
            Metric::StockDays(GoodRef::Id(GoodId(1))),
            Metric::Turnover7d(GoodRef::Id(GoodId(1))),
            Metric::Sales7d(GoodRef::Id(GoodId(1))),
            Metric::DaysToExpiry(GoodRef::Id(GoodId(1))),
            Metric::ShelfGap(GoodRef::Id(GoodId(1))),
            Metric::DaysSinceLastChange(GoodRef::Id(GoodId(1))),
        ];
        let mut odciski: Vec<magnat_core::StateHash> = wszystkie
            .iter()
            .map(|m| {
                let mut h = StateHasher::new();
                m.hash_state(&mut h);
                h.finish()
            })
            .collect();
        odciski.sort_unstable();
        let ile = odciski.len();
        odciski.dedup();
        assert_eq!(odciski.len(), ile, "dwie metryki mają ten sam odcisk");
    }
}
