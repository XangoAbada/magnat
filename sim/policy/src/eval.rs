//! Ewaluator (M7c WP6b, `K-11`).
//!
//! **Ten sam kod wykonuje regułę gracza i politykę firmy AI.** To jest cały powód,
//! dla którego `sim/policy` istnieje jako osobny crate: PRD §6.3 obiecuje graczowi
//! „ten sam zestaw narzędzi co AI", a dwa interpretery tej samej reguły rozjechałyby
//! się przy pierwszym przypadku brzegowym — i to w miejscu, w którym nikt by tego
//! nie zauważył, bo obie strony „działają".
//!
//! Różnica między graczem a AI leży w **źródle** reguł (edytor M9 albo tier taktyczny
//! M7e) i w **jakości wykonania** (menedżer — `ManagerExecution`, M9d WP9), nigdy
//! w ewaluatorze.
//!
//! # Determinizm i brak alokacji
//!
//! Ewaluacja jest funkcją czystą `(polityka, kontekst, widok) → akcje`: nie losuje,
//! nie pyta o czas rzeczywisty i nie iteruje po mapie (00 §3). Na ścieżce gorącej
//! nie alokuje — wynik idzie do `SmallVec` o pojemności `MAX_RULES`, a drzewo warunku
//! jest już zbudowane i wyłącznie czytane.

use magnat_core::{DecisionReason, Money, PolicyId, PriceBasis};
use smallvec::SmallVec;

use crate::ast::{
    Action, ArithOp, ConditionExpr, Expr, GoodRef, Metric, Policy, Unit, Value, MAX_RULES,
};
use crate::view::{MetricCtx, PolicyView};

/// Akcja wraz z powodem. **Nie istnieje konstruktor bez powodu** — to jest ten sam
/// mechanizm co `apply_decision` z M7 §5.11: decyzji bez wyjaśnienia nie da się
/// wyprodukować, więc test równości liczników (§7.8) nie ma jak pęknąć po cichu.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Decided<T> {
    action: T,
    reason: DecisionReason,
}

impl<T> Decided<T> {
    #[must_use]
    pub fn new(action: T, reason: DecisionReason) -> Decided<T> {
        Decided { action, reason }
    }

    #[must_use]
    pub fn action(&self) -> &T {
        &self.action
    }

    #[must_use]
    pub fn reason(&self) -> DecisionReason {
        self.reason
    }

    /// Rozkłada na parę — dla wykonawcy, który zapisuje powód i wykonuje akcję
    /// w jednym kroku.
    #[must_use]
    pub fn split(self) -> (T, DecisionReason) {
        (self.action, self.reason)
    }
}

/// Wynik ewaluacji: akcje pierwszej pasującej reguły albo akcja zapasowa.
///
/// Akcje wracają **przez referencję do polityki**, a nie kopią. Kopia byłaby alokacją
/// na ścieżce gorącej, bo `Action` niesie `Expr` za wskaźnikiem — a kryterium WP6b
/// mówi wprost „bez alokacji", i test z licznikiem alokacji sprawdza to od drugiej
/// strony. Wykonawca, który potrzebuje własnej kopii, robi ją u siebie i płaci
/// za nią raz na zakład, a nie raz na towar.
pub type Decisions<'a> = SmallVec<[Decided<&'a Action>; MAX_RULES]>;

/// Wykonuje politykę na jednym celu.
///
/// **Pierwsza pasująca reguła wygrywa** (M9d §5.6) — kolejność reguł jest priorytetem,
/// a nie sugestią. Reguła wyłączona jest pomijana bez oceny warunku. Gdy nie pasuje
/// żadna, wykonuje się `fallback`, jeśli polityka go ma.
///
/// Powód każdej akcji wskazuje **regułę**, która się wyzwoliła; akcja zapasowa dostaje
/// numer `u8::MAX`, bo „to jest ta reguła, której nie było" jest innym zdaniem niż
/// „to jest reguła numer zero".
pub fn evaluate<'a>(rules: &'a Policy, ctx: &MetricCtx, v: &impl PolicyView) -> Decisions<'a> {
    let mut out = Decisions::new();
    for (i, r) in rules.rules.iter().enumerate().take(MAX_RULES) {
        if !r.enabled {
            continue;
        }
        if !condition(&r.when, ctx, v) {
            continue;
        }
        for a in &r.then {
            out.push(decyzja(rules.id, i as u8, a));
        }
        return out;
    }
    if let Some(a) = &rules.fallback {
        out.push(decyzja(rules.id, FALLBACK_RULE, a));
    }
    out
}

/// Numer reguły zapisywany dla akcji zapasowej (`INACZEJ` z gramatyki M9d).
pub const FALLBACK_RULE: u8 = u8::MAX;

fn decyzja(policy: PolicyId, rule: u8, action: &Action) -> Decided<&Action> {
    let kind = action.kind();
    Decided::new(
        action,
        DecisionReason::PolicyApplied {
            policy,
            rule,
            action: kind,
        },
    )
}

/// Ocena warunku. Nieznana metryka daje `false`, nie panikę i nie zero — patrz
/// `PolicyView::metric`.
#[must_use]
pub fn condition(c: &ConditionExpr, ctx: &MetricCtx, v: &impl PolicyView) -> bool {
    match c {
        ConditionExpr::Always => true,
        ConditionExpr::Not(inner) => !condition(inner, ctx, v),
        ConditionExpr::And(a, b) => condition(a, ctx, v) && condition(b, ctx, v),
        ConditionExpr::Or(a, b) => condition(a, ctx, v) || condition(b, ctx, v),
        ConditionExpr::Cmp { lhs, op, rhs } => {
            let (Some(l), Some(r)) = (eval(lhs, ctx, v), eval(rhs, ctx, v)) else {
                return false;
            };
            // Jednostki po obu stronach sprawdza walidator; w wykonaniu porównujemy
            // liczby, bo polityka bez przejścia przez walidator nie ma prawa się tu
            // znaleźć. `debug_assert`, a nie `assert`: w wydaniu produkcyjnym
            // niezgodność jednostek ma dać złą decyzję jednego sklepu, a nie panikę
            // całej symulacji.
            debug_assert_eq!(
                l.unit(),
                r.unit(),
                "niezgodne jednostki w porównaniu — walidator tego nie złapał"
            );
            op.holds(l.raw().cmp(&r.raw()))
        }
    }
}

/// Ocena wyrażenia. `None` = którejś metryki nie dało się odczytać.
#[must_use]
pub fn eval(e: &Expr, ctx: &MetricCtx, v: &impl PolicyView) -> Option<Value> {
    match e {
        Expr::Lit(val) => Some(*val),
        Expr::Metric(m) => {
            let out = v.metric(resolve_good(*m, ctx), ctx)?;
            debug_assert_eq!(
                out.unit(),
                m.unit(),
                "widok zwrócił metrykę w innej jednostce, niż deklaruje jej typ"
            );
            Some(out)
        }
        Expr::Bin { lhs, op, rhs } => {
            let l = eval(lhs, ctx, v)?;
            let r = eval(rhs, ctx, v)?;
            arytmetyka(l, *op, r)
        }
        Expr::Convert { to, of } => {
            let Value::Money(m) = eval(of, ctx, v)? else {
                // Konwersja podstawy dotyczy wyłącznie kwot. „Brutto z trzech dni"
                // nie znaczy nic i ma się nie wykonać, a nie wykonać się tożsamościowo.
                return None;
            };
            let vat = v.vat_bp(ctx).max(0);
            Some(Value::Money(match to {
                PriceBasis::GrossRetail => m.mul_ratio(i64::from(10_000 + vat), 10_000),
                PriceBasis::NetB2B => m.mul_ratio(10_000, i64::from(10_000 + vat)),
            }))
        }
    }
}

/// Podstawia `TEN_TOWAR` pod metrykę. Metryka bez towaru wraca niezmieniona.
pub(crate) fn resolve_good(m: Metric, ctx: &MetricCtx) -> Metric {
    let Some(g) = ctx.good else {
        return m;
    };
    let p = |r: GoodRef| match r {
        GoodRef::This => GoodRef::Id(g),
        inne => inne,
    };
    match m {
        Metric::Price { good, basis } => Metric::Price {
            good: p(good),
            basis,
        },
        Metric::UnitCost(good) => Metric::UnitCost(p(good)),
        Metric::Margin(good) => Metric::Margin(p(good)),
        Metric::CheapestCompetitorPrice {
            good,
            radius_m,
            basis,
        } => Metric::CheapestCompetitorPrice {
            good: p(good),
            radius_m,
            basis,
        },
        Metric::AvgCompetitorPrice {
            good,
            radius_m,
            basis,
        } => Metric::AvgCompetitorPrice {
            good: p(good),
            radius_m,
            basis,
        },
        Metric::Stock(good) => Metric::Stock(p(good)),
        Metric::StockDays(good) => Metric::StockDays(p(good)),
        Metric::Turnover7d(good) => Metric::Turnover7d(p(good)),
        Metric::Sales7d(good) => Metric::Sales7d(p(good)),
        Metric::DaysToExpiry(good) => Metric::DaysToExpiry(p(good)),
        Metric::ShelfGap(good) => Metric::ShelfGap(p(good)),
        Metric::DaysSinceLastChange(good) => Metric::DaysSinceLastChange(p(good)),
        inne => inne,
    }
}

/// Arytmetyka całkowitoliczbowa z jednostkami (00 §2).
///
/// Zasady, wszystkie trzy z M9d §5.6:
/// 1. `+` i `−` wymagają tej samej jednostki i ją zachowują;
/// 2. `×` i `÷` przez [`Unit::Count`] zachowują jednostkę lewej strony;
/// 3. `% z` mnoży przez punkty bazowe — pieniądz przez `i128` i zaokrąglenie od zera.
///
/// Wszystko inne jest błędem jednostek i daje `None`. Walidator odrzuca takie
/// wyrażenia wcześniej; tutaj jest druga linia, bo polityka wczytana z `data/` albo
/// z zapisu gry mogła powstać przy innej wersji danych.
fn arytmetyka(l: Value, op: ArithOp, r: Value) -> Option<Value> {
    match op {
        ArithOp::Pct => {
            let Value::Bp(bp) = r else { return None };
            Some(match l {
                Value::Money(m) => Value::Money(bp.of(m)),
                inne => inne.with_raw(mnoz_bp(inne.raw(), bp.get())),
            })
        }
        ArithOp::Mul | ArithOp::Div => {
            // Skalowanie: prawa strona jest liczbą bez jednostki albo procentem.
            let k = match r.unit() {
                Unit::Count => r.raw(),
                Unit::Bp if op == ArithOp::Mul => return arytmetyka(l, ArithOp::Pct, r),
                _ => return None,
            };
            if op == ArithOp::Mul {
                Some(l.with_raw(l.raw().saturating_mul(k)))
            } else {
                // `checked_div` łapie i zero, i jedyny drugi przypadek paniki
                // w dzieleniu całkowitym: `i64::MIN / -1`. Obie odpowiedzi brzmią
                // „nie wiem", bo obie są błędem wyrażenia, a nie stanem świata.
                l.raw().checked_div(k).map(|v| l.with_raw(v))
            }
        }
        ArithOp::Add | ArithOp::Sub => {
            if l.unit() != r.unit() {
                return None;
            }
            let v = if op == ArithOp::Add {
                l.raw().saturating_add(r.raw())
            } else {
                l.raw().saturating_sub(r.raw())
            };
            Some(l.with_raw(v))
        }
    }
}

/// Mnożenie liczby przez punkty bazowe z zaokrągleniem od zera — ta sama reguła
/// co `Money::mul_ratio`, żeby „105 % z 7 dni" i „105 % z 7 zł" zaokrągliły tak samo.
fn mnoz_bp(v: i64, bp: i32) -> i64 {
    Money(v).mul_ratio(i64::from(bp), 10_000).get()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Bp, CmpOp, PolicyDomain, Rule, Severity};
    use crate::view::BlindView;
    use magnat_core::{ActionKind, GoodId, PriceBasis};

    /// Widok, który zna dwie liczby — tyle wystarcza, żeby sprawdzić ewaluator.
    struct Stub {
        cena: i64,
        konkurent: Option<i64>,
    }

    impl PolicyView for Stub {
        fn metric(&self, m: Metric, _ctx: &MetricCtx) -> Option<Value> {
            match m {
                Metric::Price { .. } => Some(Value::Money(Money(self.cena))),
                Metric::CheapestCompetitorPrice { .. } => {
                    self.konkurent.map(|c| Value::Money(Money(c)))
                }
                _ => None,
            }
        }
    }

    fn cena() -> Expr {
        Expr::Metric(Metric::Price {
            good: GoodRef::This,
            basis: PriceBasis::GrossRetail,
        })
    }

    fn konkurent() -> Expr {
        Expr::Metric(Metric::CheapestCompetitorPrice {
            good: GoodRef::This,
            radius_m: 3_000,
            basis: PriceBasis::GrossRetail,
        })
    }

    fn polityka(rules: Vec<Rule>, fallback: Option<Action>) -> Policy {
        let mut p = Policy::empty(PolicyId(1), "test", PolicyDomain::Pricing);
        p.rules = rules;
        p.fallback = fallback;
        p
    }

    fn regula(when: ConditionExpr, then: Action) -> Rule {
        Rule {
            when,
            then: smallvec::smallvec![then],
            enabled: true,
            note: String::new(),
        }
    }

    fn ustaw(to: Expr) -> Action {
        Action::SetPrice {
            good: GoodRef::This,
            to,
        }
    }

    #[test]
    fn pierwsza_pasujaca_regula_wygrywa() {
        let p = polityka(
            vec![
                regula(
                    ConditionExpr::Always,
                    ustaw(Expr::Lit(Value::Money(Money(1)))),
                ),
                regula(
                    ConditionExpr::Always,
                    ustaw(Expr::Lit(Value::Money(Money(2)))),
                ),
            ],
            None,
        );
        let d = evaluate(&p, &MetricCtx::default(), &BlindView);
        assert_eq!(d.len(), 1);
        assert_eq!(
            d[0].reason(),
            DecisionReason::PolicyApplied {
                policy: PolicyId(1),
                rule: 0,
                action: ActionKind::SetPrice
            }
        );
    }

    #[test]
    fn regula_wylaczona_nie_liczy_sie_wcale() {
        let mut r = regula(
            ConditionExpr::Always,
            ustaw(Expr::Lit(Value::Money(Money(1)))),
        );
        r.enabled = false;
        let p = polityka(
            vec![r],
            Some(Action::Alert {
                msg: 0,
                severity: Severity::Info,
            }),
        );
        let d = evaluate(&p, &MetricCtx::default(), &BlindView);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].action().kind(), ActionKind::Alert);
        assert_eq!(
            d[0].reason(),
            DecisionReason::PolicyApplied {
                policy: PolicyId(1),
                rule: FALLBACK_RULE,
                action: ActionKind::Alert
            }
        );
    }

    #[test]
    fn nieznana_metryka_nie_wyzwala_reguly() {
        // Sklep, który nie zna ceny konkurenta, **nie** przecenia do zera.
        let p = polityka(
            vec![regula(
                ConditionExpr::Cmp {
                    lhs: cena(),
                    op: CmpOp::Gt,
                    rhs: konkurent(),
                },
                ustaw(konkurent()),
            )],
            None,
        );
        let widok = Stub {
            cena: 700,
            konkurent: None,
        };
        assert!(evaluate(&p, &MetricCtx::for_good(GoodId(1)), &widok).is_empty());
        // Ta sama reguła, gdy konkurent jest znany i tańszy, wyzwala się.
        let widok = Stub {
            cena: 700,
            konkurent: Some(651),
        };
        assert_eq!(
            evaluate(&p, &MetricCtx::for_good(GoodId(1)), &widok).len(),
            1
        );
    }

    #[test]
    fn dwa_procent_ponizej_najtanszego_liczy_sie_w_groszach() {
        // Reguła z PRD §6.3 wprost: „−2 % względem najtańszego konkurenta w 3 km".
        let wyrazenie = Expr::Bin {
            lhs: Box::new(konkurent()),
            op: ArithOp::Pct,
            rhs: Box::new(Expr::Lit(Value::Bp(Bp(9_800)))),
        };
        let widok = Stub {
            cena: 700,
            konkurent: Some(651),
        };
        let v = eval(&wyrazenie, &MetricCtx::for_good(GoodId(1)), &widok);
        // 651 gr × 98 % = 637,98 gr → 638 gr, zaokrąglone od zera.
        assert_eq!(v, Some(Value::Money(Money(638))));
    }

    #[test]
    fn jednostki_nie_dodaja_sie_przez_przeoczenie() {
        let dni_plus_zlote = arytmetyka(Value::Days(3), ArithOp::Add, Value::Money(Money(5)));
        assert_eq!(dni_plus_zlote, None);
        // Ta sama jednostka — wolno.
        assert_eq!(
            arytmetyka(Value::Days(3), ArithOp::Add, Value::Days(4)),
            Some(Value::Days(7))
        );
        // Skalowanie liczbą bez jednostki zachowuje jednostkę lewej strony.
        assert_eq!(
            arytmetyka(Value::Money(Money(100)), ArithOp::Mul, Value::Count(3)),
            Some(Value::Money(Money(300)))
        );
        // Dzielenie przez zero nie panikuje — daje „nie wiem".
        assert_eq!(
            arytmetyka(Value::Money(Money(100)), ArithOp::Div, Value::Count(0)),
            None
        );
    }

    #[test]
    fn ewaluacja_jest_powtarzalna_co_do_bitu() {
        let p = polityka(
            vec![regula(
                ConditionExpr::And(
                    Box::new(ConditionExpr::Cmp {
                        lhs: cena(),
                        op: CmpOp::Gt,
                        rhs: konkurent(),
                    }),
                    Box::new(ConditionExpr::Not(Box::new(ConditionExpr::Cmp {
                        lhs: cena(),
                        op: CmpOp::Lt,
                        rhs: Expr::Lit(Value::Money(Money(100))),
                    }))),
                ),
                ustaw(konkurent()),
            )],
            None,
        );
        let widok = Stub {
            cena: 700,
            konkurent: Some(651),
        };
        let a = evaluate(&p, &MetricCtx::for_good(GoodId(1)), &widok);
        let b = evaluate(&p, &MetricCtx::for_good(GoodId(1)), &widok);
        assert_eq!(a, b);
    }
}
