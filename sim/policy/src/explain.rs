//! „Dlaczego ta reguła się wyzwoliła" — drzewo wyjaśnienia dla M9 (M7 §6).
//!
//! Osobno od [`crate::eval`] i z rozmysłu: ewaluator jest na ścieżce gorącej i **nie
//! alokuje**, a wyjaśnienie z natury alokuje, bo jest drzewem. Wołane jest raz —
//! kiedy gracz otwiera kartę albo uruchamia dry-run — a nie dziesięć tysięcy razy
//! na dobę gry.
//!
//! Wyjaśnienie niesie **odczytane wartości**, nie samą strukturę. „Cena > cena
//! najtańszego konkurenta" nie jest odpowiedzią na nic; „6,38 zł > 6,51 zł: nie"
//! jest.

use crate::ast::{ConditionExpr, Expr, Value};
use crate::view::{MetricCtx, PolicyView};

/// Węzeł wyjaśnienia. Tekst powstaje w warstwie UI — tutaj są liczby i struktura.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ExplainTree {
    Always,
    Not(Box<ExplainTree>, bool),
    And(Box<ExplainTree>, Box<ExplainTree>, bool),
    Or(Box<ExplainTree>, Box<ExplainTree>, bool),
    /// Porównanie wraz z tym, co stało po obu stronach. `None` znaczy „nie wiem" —
    /// i to jest najczęstsza odpowiedź na pytanie „czemu nic się nie stało".
    Cmp {
        lhs: Option<Value>,
        op: crate::ast::CmpOp,
        rhs: Option<Value>,
        holds: bool,
    },
}

impl ExplainTree {
    /// Czy ten węzeł wyszedł prawdą.
    #[must_use]
    pub const fn holds(&self) -> bool {
        match self {
            ExplainTree::Always => true,
            ExplainTree::Not(_, v) | ExplainTree::And(_, _, v) | ExplainTree::Or(_, _, v) => *v,
            ExplainTree::Cmp { holds, .. } => *holds,
        }
    }
}

/// Buduje drzewo wyjaśnienia dla warunku reguły.
#[must_use]
pub fn explain(c: &ConditionExpr, ctx: &MetricCtx, v: &impl PolicyView) -> ExplainTree {
    match c {
        ConditionExpr::Always => ExplainTree::Always,
        ConditionExpr::Not(inner) => {
            let t = explain(inner, ctx, v);
            let w = !t.holds();
            ExplainTree::Not(Box::new(t), w)
        }
        ConditionExpr::And(a, b) => {
            let (ta, tb) = (explain(a, ctx, v), explain(b, ctx, v));
            let w = crate::eval::condition(c, ctx, v);
            ExplainTree::And(Box::new(ta), Box::new(tb), w)
        }
        ConditionExpr::Or(a, b) => {
            let (ta, tb) = (explain(a, ctx, v), explain(b, ctx, v));
            let w = crate::eval::condition(c, ctx, v);
            ExplainTree::Or(Box::new(ta), Box::new(tb), w)
        }
        ConditionExpr::Cmp { lhs, op, rhs } => {
            let l = crate::eval::eval(lhs, ctx, v);
            let r = crate::eval::eval(rhs, ctx, v);
            let holds = match (l, r) {
                (Some(a), Some(b)) => op.holds(a.raw().cmp(&b.raw())),
                _ => false,
            };
            ExplainTree::Cmp {
                lhs: l,
                op: *op,
                rhs: r,
                holds,
            }
        }
    }
}

/// Wartości metryk, które wchodziły do decyzji — wejście karty inspekcji i dry-runu.
///
/// Zwracane osobno od drzewa, bo UI pokazuje je jako listę („najtańszy konkurent
/// w 3 km = 6,51 zł, dane sprzed 4 dni"), a nie jako drzewo.
#[must_use]
pub fn inputs(
    c: &ConditionExpr,
    ctx: &MetricCtx,
    v: &impl PolicyView,
) -> Vec<(crate::ast::Metric, Option<Value>)> {
    let mut out = Vec::new();
    zbierz_warunek(c, ctx, v, &mut out);
    out
}

fn zbierz_warunek(
    c: &ConditionExpr,
    ctx: &MetricCtx,
    v: &impl PolicyView,
    out: &mut Vec<(crate::ast::Metric, Option<Value>)>,
) {
    match c {
        ConditionExpr::Always => {}
        ConditionExpr::Not(i) => zbierz_warunek(i, ctx, v, out),
        ConditionExpr::And(a, b) | ConditionExpr::Or(a, b) => {
            zbierz_warunek(a, ctx, v, out);
            zbierz_warunek(b, ctx, v, out);
        }
        ConditionExpr::Cmp { lhs, rhs, .. } => {
            zbierz_wyrazenie(lhs, ctx, v, out);
            zbierz_wyrazenie(rhs, ctx, v, out);
        }
    }
}

fn zbierz_wyrazenie(
    e: &Expr,
    ctx: &MetricCtx,
    v: &impl PolicyView,
    out: &mut Vec<(crate::ast::Metric, Option<Value>)>,
) {
    match e {
        Expr::Lit(_) => {}
        Expr::Metric(m) => {
            let m = crate::eval::resolve_good(*m, ctx);
            out.push((m, v.metric(m, ctx)));
        }
        Expr::Bin { lhs, rhs, .. } => {
            zbierz_wyrazenie(lhs, ctx, v, out);
            zbierz_wyrazenie(rhs, ctx, v, out);
        }
        Expr::Convert { of, .. } => zbierz_wyrazenie(of, ctx, v, out),
    }
}
