//! Walidator polityki (M9d §5.6 „Walidacja", M7c WP6b).
//!
//! Edytor M9 nie pozwoli złożyć większości tych błędów — sloty oferują wyłącznie
//! wyrażenia zgodne jednostkowo. Walidator jest mimo to potrzebny w dwóch miejscach,
//! i oba są realne: **import z tekstu** (dzielenie się politykami, `data/policies/`)
//! oraz **polityka z zapisu gry sprzed zmiany danych**.
//!
//! Rozróżnienie błąd/ostrzeżenie jest z M9d i nie jest kosmetyczne: sprzedaż poniżej
//! kosztu bywa **świadomą** wojną cenową, więc jest ostrzeżeniem; zmieszanie ceny
//! brutto z netto nie bywa niczym świadomym, więc jest błędem (`K-7`).

use crate::ast::{
    Action, ArithOp, ConditionExpr, Expr, Metric, Policy, PolicyDomain, Unit, MAX_COMPETITIVE,
    MAX_DEPTH, MAX_RADIUS_M, MAX_RULES,
};
use magnat_core::PriceBasis;

/// Gdzie w drzewie siedzi problem: numer reguły i numer akcji w niej.
///
/// `rule == usize::MAX` znaczy „w akcji zapasowej", bo to nie jest reguła numer nic.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct NodePath {
    pub rule: usize,
    pub action: Option<usize>,
}

impl NodePath {
    #[must_use]
    pub const fn rule(rule: usize) -> NodePath {
        NodePath { rule, action: None }
    }

    #[must_use]
    pub const fn action(rule: usize, action: usize) -> NodePath {
        NodePath {
            rule,
            action: Some(action),
        }
    }
}

/// Uwaga walidatora.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Diagnostic {
    /// Błąd: porównanie albo działanie na niezgodnych jednostkach.
    UnitMismatch {
        at: NodePath,
        expected: Unit,
        got: Unit,
    },
    /// Błąd (`K-7`): brutto zmieszane z netto w jednym porównaniu lub w jednej akcji.
    PriceBasisMismatch {
        at: NodePath,
        lhs: PriceBasis,
        rhs: PriceBasis,
    },
    /// Błąd: akcja spoza dziedziny polityki — nie miałaby kto jej wykonać.
    ActionOutOfDomain {
        at: NodePath,
        expected: PolicyDomain,
        got: PolicyDomain,
    },
    /// Błąd: dziedzina, której w tej fazie nikt jeszcze nie wykonuje.
    ///
    /// Jawna odmowa zamiast cichego zera. Polityka kadrowa przypięta do zakładu
    /// wyglądałaby jak działająca i nie robiłaby nic — a to jest gorsze niż komunikat,
    /// bo gracz zbudowałby na niej plan.
    DomainNotAvailable { domain: PolicyDomain },
    /// Błąd: wyrażenie głębsze niż [`MAX_DEPTH`].
    TooDeep { at: NodePath, depth: u8 },
    /// Błąd: więcej reguł niż [`MAX_RULES`], promień ponad [`MAX_RADIUS_M`]
    /// albo więcej metryk konkurencyjnych niż [`MAX_COMPETITIVE`].
    BudgetExceeded { what: &'static str, limit: u32 },
    /// Ostrzeżenie: reguła nieosiągalna, bo wcześniejsza pokrywa ją w całości.
    UnreachableRule { rule: usize, shadowed_by: usize },
    /// Ostrzeżenie: reguła rusza metryką, którą sama czyta, i nie ma ani ogranicznika,
    /// ani martwej strefy — to jest przepis na oscylację.
    PossibleOscillation { rule: usize },
}

impl Diagnostic {
    /// Czy uwaga blokuje przypięcie polityki.
    #[must_use]
    pub const fn is_error(&self) -> bool {
        !matches!(
            self,
            Diagnostic::UnreachableRule { .. } | Diagnostic::PossibleOscillation { .. }
        )
    }
}

/// Zbiór uwag. Pusty znaczy „polityka jest poprawna", nie „nie sprawdzono".
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct PolicyError(pub Vec<Diagnostic>);

impl PolicyError {
    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.0.iter().filter(|d| d.is_error())
    }

    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.0.iter().any(Diagnostic::is_error)
    }
}

impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "polityka ma {} uwag walidatora", self.0.len())
    }
}

impl std::error::Error for PolicyError {}

/// Dziedziny, które mają w tej fazie wykonawcę.
///
/// `Hr` czeka na M7e (decyzje kadrowe firmy AI), `Production` i `Logistics` na M7e
/// i M9. Lista jest tutaj, a nie rozsypana po wykonawcach, bo to ona jest odpowiedzią
/// na pytanie „co ta polityka umie dziś zrobić".
const AVAILABLE: &[PolicyDomain] = &[PolicyDomain::Pricing, PolicyDomain::Stock];

/// Sprawdza politykę. Błąd blokuje przypięcie; same ostrzeżenia nie.
///
/// Pełną listę uwag, razem z ostrzeżeniami, daje [`diagnose`] — to ona zasila edytor
/// M9. `validate` odpowiada na węższe pytanie: **czy wolno**.
pub fn validate(rules: &Policy) -> Result<(), PolicyError> {
    let e = diagnose(rules);
    if e.has_errors() {
        Err(e)
    } else {
        Ok(())
    }
}

/// Wszystkie uwagi walidatora. Kolejność jest deterministyczna — idzie po regułach
/// i po akcjach w nich, więc dwa przebiegi dają tę samą listę.
#[must_use]
pub fn diagnose(rules: &Policy) -> PolicyError {
    let mut d: Vec<Diagnostic> = Vec::new();

    if !AVAILABLE.contains(&rules.domain) {
        d.push(Diagnostic::DomainNotAvailable {
            domain: rules.domain,
        });
    }
    if rules.rules.len() > MAX_RULES {
        d.push(Diagnostic::BudgetExceeded {
            what: "rules",
            limit: MAX_RULES as u32,
        });
    }

    let mut konkurencyjne = 0usize;
    for (i, r) in rules.rules.iter().enumerate() {
        warunek(&r.when, i, &mut d, &mut konkurencyjne);
        for (j, a) in r.then.iter().enumerate() {
            akcja(
                a,
                rules.domain,
                NodePath::action(i, j),
                &mut d,
                &mut konkurencyjne,
            );
        }
        if oscyluje(r, rules.cooldown_h) {
            d.push(Diagnostic::PossibleOscillation { rule: i });
        }
        // Reguła nieosiągalna: wcześniejsza ma warunek `ZAWSZE`. Tanio i wystarcza —
        // M9d mówi wprost „podstawienie przedziałów, nie SMT", a warunek zawsze
        // prawdziwy jest jedynym przypadkiem, który daje się rozstrzygnąć bez
        // znajomości świata.
        if let Some(kto) = rules.rules[..i]
            .iter()
            .position(|w| w.enabled && matches!(w.when, ConditionExpr::Always))
        {
            d.push(Diagnostic::UnreachableRule {
                rule: i,
                shadowed_by: kto,
            });
        }
    }
    if let Some(a) = &rules.fallback {
        akcja(
            a,
            rules.domain,
            NodePath::rule(usize::MAX),
            &mut d,
            &mut konkurencyjne,
        );
    }
    if konkurencyjne > MAX_COMPETITIVE {
        d.push(Diagnostic::BudgetExceeded {
            what: "competitive_metrics",
            limit: MAX_COMPETITIVE as u32,
        });
    }
    PolicyError(d)
}

fn warunek(c: &ConditionExpr, rule: usize, d: &mut Vec<Diagnostic>, konk: &mut usize) {
    match c {
        ConditionExpr::Always => {}
        ConditionExpr::Not(inner) => warunek(inner, rule, d, konk),
        ConditionExpr::And(a, b) | ConditionExpr::Or(a, b) => {
            warunek(a, rule, d, konk);
            warunek(b, rule, d, konk);
        }
        ConditionExpr::Cmp { lhs, op: _, rhs } => {
            let at = NodePath::rule(rule);
            let l = wyrazenie(lhs, at, d, konk);
            let r = wyrazenie(rhs, at, d, konk);
            if let (Some(lu), Some(ru)) = (l.unit, r.unit) {
                if lu != ru {
                    d.push(Diagnostic::UnitMismatch {
                        at,
                        expected: lu,
                        got: ru,
                    });
                }
            }
            sprawdz_podstawe(l.basis, r.basis, at, d);
        }
    }
}

/// Co walidator wie o wyrażeniu: jednostkę wyniku i podstawę ceny, jeśli jakąś niesie.
#[derive(Clone, Copy, Default)]
struct Info {
    unit: Option<Unit>,
    basis: Option<PriceBasis>,
}

fn wyrazenie(e: &Expr, at: NodePath, d: &mut Vec<Diagnostic>, konk: &mut usize) -> Info {
    fn idz(
        e: &Expr,
        at: NodePath,
        d: &mut Vec<Diagnostic>,
        konk: &mut usize,
        glebokosc: u8,
    ) -> Info {
        if glebokosc > MAX_DEPTH {
            d.push(Diagnostic::TooDeep {
                at,
                depth: glebokosc,
            });
            return Info::default();
        }
        match e {
            Expr::Lit(v) => Info {
                unit: Some(v.unit()),
                basis: None,
            },
            Expr::Metric(m) => {
                if m.is_competitive() {
                    *konk += 1;
                    if promien(*m) > MAX_RADIUS_M {
                        d.push(Diagnostic::BudgetExceeded {
                            what: "radius_m",
                            limit: MAX_RADIUS_M,
                        });
                    }
                }
                Info {
                    unit: Some(m.unit()),
                    basis: m.basis(),
                }
            }
            // Jawna konwersja **zmienia podstawę** i to jest jej cały sens: po niej
            // wyrażenie wolno porównać z drugą stroną w tej podstawie. Jednostka
            // zostaje pieniądzem; wyrażenie o innej jednostce jest błędem.
            Expr::Convert { to, of } => {
                let i = idz(of, at, d, konk, glebokosc + 1);
                if i.unit.is_some_and(|u| u != Unit::Money) {
                    d.push(Diagnostic::UnitMismatch {
                        at,
                        expected: Unit::Money,
                        got: i.unit.unwrap_or(Unit::Money),
                    });
                }
                Info {
                    unit: Some(Unit::Money),
                    basis: Some(*to),
                }
            }
            Expr::Bin { lhs, op, rhs } => {
                let l = idz(lhs, at, d, konk, glebokosc + 1);
                let r = idz(rhs, at, d, konk, glebokosc + 1);
                match op {
                    ArithOp::Add | ArithOp::Sub => {
                        if let (Some(lu), Some(ru)) = (l.unit, r.unit) {
                            if lu != ru {
                                d.push(Diagnostic::UnitMismatch {
                                    at,
                                    expected: lu,
                                    got: ru,
                                });
                            }
                        }
                        sprawdz_podstawe(l.basis, r.basis, at, d);
                        Info {
                            unit: l.unit,
                            // Suma dwóch cen o tej samej podstawie ma tę podstawę;
                            // po sprawdzeniu wyżej obie są równe albo jedna nieznana.
                            basis: l.basis.or(r.basis),
                        }
                    }
                    // Skalowanie nie zmienia ani jednostki, ani podstawy lewej strony.
                    ArithOp::Mul | ArithOp::Div | ArithOp::Pct => l,
                }
            }
        }
    }
    idz(e, at, d, konk, 1)
}

fn promien(m: Metric) -> u32 {
    match m {
        Metric::CheapestCompetitorPrice { radius_m, .. }
        | Metric::AvgCompetitorPrice { radius_m, .. }
        | Metric::CompetitorCount { radius_m } => radius_m,
        _ => 0,
    }
}

/// `K-7`: brutto do brutto, netto do netto. Konwersja istnieje, ale musi być jawna.
fn sprawdz_podstawe(
    l: Option<PriceBasis>,
    r: Option<PriceBasis>,
    at: NodePath,
    d: &mut Vec<Diagnostic>,
) {
    if let (Some(lb), Some(rb)) = (l, r) {
        if lb != rb {
            d.push(Diagnostic::PriceBasisMismatch {
                at,
                lhs: lb,
                rhs: rb,
            });
        }
    }
}

fn akcja(
    a: &Action,
    domain: PolicyDomain,
    at: NodePath,
    d: &mut Vec<Diagnostic>,
    konk: &mut usize,
) {
    if let Some(wlasna) = a.domain() {
        if wlasna != domain {
            d.push(Diagnostic::ActionOutOfDomain {
                at,
                expected: domain,
                got: wlasna,
            });
        }
    }
    let mut sprawdz = |e: &Expr, oczekiwana: Unit, podstawa: Option<PriceBasis>| {
        let i = wyrazenie(e, at, d, konk);
        if let Some(u) = i.unit {
            if u != oczekiwana {
                d.push(Diagnostic::UnitMismatch {
                    at,
                    expected: oczekiwana,
                    got: u,
                });
            }
        }
        sprawdz_podstawe(podstawa, i.basis, at, d);
    };
    match a {
        // Cena półkowa jest brutto (`K-7`), więc wyrażenie ją ustawiające też musi być.
        Action::SetPrice { to, .. } => sprawdz(to, Unit::Money, Some(PriceBasis::GrossRetail)),
        Action::AdjustPrice { by, .. } => sprawdz(by, Unit::Money, Some(PriceBasis::GrossRetail)),
        Action::ClampPrice { min, max, .. } => {
            sprawdz(min, Unit::Money, Some(PriceBasis::GrossRetail));
            sprawdz(max, Unit::Money, Some(PriceBasis::GrossRetail));
        }
        Action::OrderUpTo { days, .. } => sprawdz(days, Unit::Days, None),
        Action::OrderQty { qty, .. } | Action::PlanProduction { qty, .. } => {
            sprawdz(qty, Unit::Qty, None);
        }
        // Płaca jest netto dla firmy — to koszt, nie cena półkowa.
        Action::Hire { wage, .. } => sprawdz(wage, Unit::Money, Some(PriceBasis::NetB2B)),
        Action::RaiseWage { cap: Some(c), .. } => sprawdz(c, Unit::Money, Some(PriceBasis::NetB2B)),
        Action::SetMargin { .. }
        | Action::Markdown { .. }
        | Action::RemoveFromShelf(_)
        | Action::RaiseWage { cap: None, .. }
        | Action::Alert { .. }
        | Action::AskPlayer { .. } => {}
    }
}

/// Czy reguła może oscylować: rusza ceną, czyta cenę i nie ma ani ogranicznika,
/// ani martwej strefy.
fn oscyluje(r: &crate::ast::Rule, cooldown_h: u8) -> bool {
    if cooldown_h > 0 {
        return false;
    }
    let ma_ogranicznik = r
        .then
        .iter()
        .any(|a| matches!(a, Action::ClampPrice { .. }));
    if ma_ogranicznik {
        return false;
    }
    let rusza_cena = r.then.iter().any(|a| {
        matches!(
            a,
            Action::SetPrice { .. } | Action::AdjustPrice { .. } | Action::Markdown { .. }
        )
    });
    rusza_cena && czyta_cene(&r.when)
}

fn czyta_cene(c: &ConditionExpr) -> bool {
    match c {
        ConditionExpr::Always => false,
        ConditionExpr::Not(i) => czyta_cene(i),
        ConditionExpr::And(a, b) | ConditionExpr::Or(a, b) => czyta_cene(a) || czyta_cene(b),
        ConditionExpr::Cmp { lhs, rhs, .. } => cenowe(lhs) || cenowe(rhs),
    }
}

fn cenowe(e: &Expr) -> bool {
    match e {
        Expr::Lit(_) => false,
        Expr::Metric(m) => matches!(m, Metric::Price { .. } | Metric::Margin(_)),
        Expr::Bin { lhs, rhs, .. } => cenowe(lhs) || cenowe(rhs),
        Expr::Convert { of, .. } => cenowe(of),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Bp, CmpOp, GoodRef, Rule, Value};
    use magnat_core::{GoodId, Money, PolicyId};

    fn cena(basis: PriceBasis) -> Expr {
        Expr::Metric(Metric::Price {
            good: GoodRef::This,
            basis,
        })
    }

    fn koszt() -> Expr {
        Expr::Metric(Metric::UnitCost(GoodRef::This))
    }

    fn pol(rules: Vec<Rule>) -> Policy {
        let mut p = Policy::empty(PolicyId(1), "test", PolicyDomain::Pricing);
        p.rules = rules;
        p.cooldown_h = 12;
        p
    }

    fn r(when: ConditionExpr, then: Vec<Action>) -> Rule {
        Rule {
            when,
            then: then.into_iter().collect(),
            enabled: true,
            note: String::new(),
        }
    }

    #[test]
    fn mieszanie_brutto_z_netto_jest_bledem_a_nie_ostrzezeniem() {
        // „ustaw cenę brutto = koszt netto" — dokładnie ten przypadek z M9d.
        let p = pol(vec![r(
            ConditionExpr::Cmp {
                lhs: cena(PriceBasis::GrossRetail),
                op: CmpOp::Gt,
                rhs: koszt(),
            },
            vec![],
        )]);
        let e = validate(&p).expect_err("brutto do netto musi paść");
        assert!(e
            .0
            .iter()
            .any(|d| matches!(d, Diagnostic::PriceBasisMismatch { .. })));
    }

    #[test]
    fn to_samo_porownanie_w_jednej_podstawie_przechodzi() {
        let p = pol(vec![r(
            ConditionExpr::Cmp {
                lhs: cena(PriceBasis::GrossRetail),
                op: CmpOp::Gt,
                rhs: Expr::Metric(Metric::CheapestCompetitorPrice {
                    good: GoodRef::This,
                    radius_m: 3_000,
                    basis: PriceBasis::GrossRetail,
                }),
            },
            vec![Action::SetPrice {
                good: GoodRef::This,
                to: cena(PriceBasis::GrossRetail),
            }],
        )]);
        assert!(validate(&p).is_ok(), "{:?}", validate(&p));
    }

    #[test]
    fn niezgodne_jednostki_w_porownaniu_sa_bledem() {
        let p = pol(vec![r(
            ConditionExpr::Cmp {
                lhs: Expr::Metric(Metric::StockDays(GoodRef::This)),
                op: CmpOp::Lt,
                rhs: Expr::Lit(Value::Money(Money(300))),
            },
            vec![],
        )]);
        let e = validate(&p).expect_err("dni wobec złotówek");
        assert!(e
            .0
            .iter()
            .any(|d| matches!(d, Diagnostic::UnitMismatch { .. })));
    }

    #[test]
    fn dziedzina_bez_wykonawcy_jest_odmowa_a_nie_cisza() {
        let mut p = Policy::empty(PolicyId(2), "kadry", PolicyDomain::Hr);
        p.rules.push(r(
            ConditionExpr::Always,
            vec![Action::RaiseWage {
                role: magnat_core::JobRoleId(0),
                by: Bp(500),
                cap: None,
            }],
        ));
        let e = validate(&p).expect_err("polityka kadrowa nie ma dziś wykonawcy");
        assert!(e.0.iter().any(|d| matches!(
            d,
            Diagnostic::DomainNotAvailable {
                domain: PolicyDomain::Hr
            }
        )));
    }

    #[test]
    fn akcja_spoza_dziedziny_polityki_nie_przechodzi() {
        let p = pol(vec![r(
            ConditionExpr::Always,
            vec![Action::OrderUpTo {
                good: GoodRef::This,
                days: Expr::Lit(Value::Days(10)),
            }],
        )]);
        let e = validate(&p).expect_err("zamówienie w polityce cenowej");
        assert!(e
            .0
            .iter()
            .any(|d| matches!(d, Diagnostic::ActionOutOfDomain { .. })));
    }

    #[test]
    fn oscylacja_jest_ostrzezeniem_i_gasi_ja_martwa_strefa() {
        let regula = r(
            ConditionExpr::Cmp {
                lhs: cena(PriceBasis::GrossRetail),
                op: CmpOp::Gt,
                rhs: Expr::Lit(Value::Money(Money(100))),
            },
            vec![Action::Markdown {
                good: GoodRef::This,
                bp: Bp(500),
            }],
        );
        let mut p = Policy::empty(PolicyId(1), "test", PolicyDomain::Pricing);
        p.rules = vec![regula];
        // Bez martwej strefy: ostrzeżenie, ale polityka przechodzi.
        assert!(validate(&p).is_ok());
        assert!(diagnose(&p)
            .0
            .iter()
            .any(|d| matches!(d, Diagnostic::PossibleOscillation { .. })));
        p.cooldown_h = 12;
        assert!(diagnose(&p).0.is_empty());
    }

    #[test]
    fn regula_po_zawsze_jest_nieosiagalna() {
        let mut p = Policy::empty(PolicyId(1), "test", PolicyDomain::Pricing);
        p.cooldown_h = 12;
        p.rules = vec![
            r(ConditionExpr::Always, vec![]),
            r(
                ConditionExpr::Cmp {
                    lhs: cena(PriceBasis::GrossRetail),
                    op: CmpOp::Gt,
                    rhs: Expr::Lit(Value::Money(Money(1))),
                },
                vec![],
            ),
        ];
        assert!(diagnose(&p).0.iter().any(|d| matches!(
            d,
            Diagnostic::UnreachableRule {
                rule: 1,
                shadowed_by: 0
            }
        )));
    }

    #[test]
    fn promien_ponad_dziesiec_kilometrow_nie_przechodzi() {
        let p = pol(vec![r(
            ConditionExpr::Cmp {
                lhs: cena(PriceBasis::GrossRetail),
                op: CmpOp::Gt,
                rhs: Expr::Metric(Metric::CheapestCompetitorPrice {
                    good: GoodRef::Id(GoodId(1)),
                    radius_m: 20_000,
                    basis: PriceBasis::GrossRetail,
                }),
            },
            vec![],
        )]);
        let e = validate(&p).expect_err("promień ponad limit");
        assert!(e.0.iter().any(|d| matches!(
            d,
            Diagnostic::BudgetExceeded {
                what: "radius_m",
                ..
            }
        )));
    }
}
