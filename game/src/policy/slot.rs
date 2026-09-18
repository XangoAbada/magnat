//! Model formularza edytora: slot wyrażenia, klauzula i szkic akcji (M9d WP8).
//!
//! # Po co osobny model, skoro jest AST
//!
//! AST jest drzewem, a formularz jest listą pól — i to jest cała różnica, ale jest
//! zasadnicza. Gracz nie składa `Bin { lhs, op, rhs }`; gracz wybiera z listy „cena
//! najtańszego konkurenta", dopisuje „× 98 %" i zaznacza „brutto". Trzy pola, jeden
//! wiersz, żadnej składni — i **żadnej możliwości napisania błędu**, bo lista nie
//! zawiera pozycji, która by go tworzyła.
//!
//! [`Slot`] jest dokładnie tym wierszem i odwzorowuje się na AST w jedną stronę
//! jednoznacznie, a w drugą częściowo: drzewo, którego formularz nie umie pokazać,
//! wraca `None`. To nie jest luka, tylko granica — edytor obsługuje **to, co da się
//! wyklikać**, a nie wszystko, co da się wyrazić. Dzisiaj pokrywa to komplet sześciu
//! polityk przykładowych z §5.6 i komplet presetów z `data/policies/`.
//!
//! # Głębokość
//!
//! Slot z konwersją i skalowaniem to `Convert(Bin(base, Pct, Lit))`, czyli dokładnie
//! trzy poziomy — tyle, ile wynosi `MAX_DEPTH` języka. Czwartego poziomu formularz
//! nie umie złożyć i to jest zamierzone: limit z §5.6 przestaje być regułą walidatora,
//! a staje się kształtem formularza.

use magnat_core::{ActionKind, JobRoleId, PriceBasis, RecipeId};
use magnat_policy::{
    Action, ArithOp, Bp, CmpOp, ConditionExpr, Expr, GoodRef, Metric, OrderSource, Rule, Severity,
    Unit, Value,
};

/// Podstawa slotu: metryka świata albo liczba wpisana przez gracza.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Base {
    Metric(Metric),
    Lit(Value),
}

impl Base {
    #[must_use]
    pub const fn unit(self) -> Unit {
        match self {
            Base::Metric(m) => m.unit(),
            Base::Lit(v) => v.unit(),
        }
    }

    #[must_use]
    pub const fn basis(self) -> Option<PriceBasis> {
        match self {
            Base::Metric(m) => m.basis(),
            Base::Lit(_) => None,
        }
    }
}

/// Jeden wiersz formularza: co, razy ile procent, w jakiej podstawie.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Slot {
    pub base: Base,
    /// Skalowanie „× N %". `None` znaczy sto procent, a nie brak mnożenia — różnica
    /// jest wyłącznie w zapisie tekstowym.
    pub scale: Option<Bp>,
    /// Jawna konwersja podstawy ceny (`brutto(x)` / `netto(x)`, `K-7`).
    pub convert: Option<PriceBasis>,
}

impl Slot {
    #[must_use]
    pub const fn metric(m: Metric) -> Slot {
        Slot {
            base: Base::Metric(m),
            scale: None,
            convert: None,
        }
    }

    #[must_use]
    pub const fn lit(v: Value) -> Slot {
        Slot {
            base: Base::Lit(v),
            scale: None,
            convert: None,
        }
    }

    #[must_use]
    pub const fn scaled(mut self, bp: Bp) -> Slot {
        self.scale = Some(bp);
        self
    }

    #[must_use]
    pub const fn converted(mut self, to: PriceBasis) -> Slot {
        self.convert = Some(to);
        self
    }

    /// Jednostka wyniku. Konwersja czyni z wyrażenia pieniądz — i to jest jej
    /// jedyny skutek poza podstawą.
    #[must_use]
    pub const fn unit(self) -> Unit {
        match self.convert {
            Some(_) => Unit::Money,
            None => self.base.unit(),
        }
    }

    /// Podstawa ceny, jeśli slot jakąś niesie. `None` znaczy „nie dotyczy",
    /// a nie „domyślnie brutto" — na tej różnicy stoi `PriceBasisMismatch`.
    #[must_use]
    pub const fn basis(self) -> Option<PriceBasis> {
        match self.convert {
            Some(b) => Some(b),
            None => self.base.basis(),
        }
    }

    /// Slot na wyrażenie AST.
    #[must_use]
    pub fn to_expr(self) -> Expr {
        let mut e = match self.base {
            Base::Metric(m) => Expr::Metric(m),
            Base::Lit(v) => Expr::Lit(v),
        };
        if let Some(bp) = self.scale {
            e = Expr::Bin {
                lhs: Box::new(e),
                op: ArithOp::Pct,
                rhs: Box::new(Expr::Lit(Value::Bp(bp))),
            };
        }
        if let Some(to) = self.convert {
            e = Expr::Convert {
                to,
                of: Box::new(e),
            };
        }
        e
    }

    /// Wyrażenie AST na slot. `None` dla drzewa, którego formularz nie umie pokazać.
    #[must_use]
    pub fn from_expr(e: &Expr) -> Option<Slot> {
        match e {
            Expr::Lit(v) => Some(Slot::lit(*v)),
            Expr::Metric(m) => Some(Slot::metric(*m)),
            Expr::Bin {
                lhs,
                op: ArithOp::Pct,
                rhs,
            } => {
                let Expr::Lit(Value::Bp(bp)) = **rhs else {
                    return None;
                };
                let s = Slot::from_expr(lhs)?;
                if s.scale.is_some() || s.convert.is_some() {
                    return None;
                }
                Some(s.scaled(bp))
            }
            Expr::Convert { to, of } => {
                let s = Slot::from_expr(of)?;
                if s.convert.is_some() {
                    return None;
                }
                Some(s.converted(*to))
            }
            Expr::Bin { .. } => None,
        }
    }
}

/// Jedno porównanie w warunku reguły.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Clause {
    pub lhs: Slot,
    pub op: CmpOp,
    pub rhs: Slot,
}

/// Czym spięte są klauzule reguły. Jeden spójnik na regułę, nie po jednym na parę —
/// mieszanie „I" z „LUB" bez nawiasów jest pułapką, a nawiasów formularz nie ma.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Join {
    #[default]
    And,
    Or,
}

/// Szkic reguły: lista klauzul, lista akcji i notatka gracza.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct RuleDraft {
    pub clauses: Vec<Clause>,
    pub join: Join,
    pub actions: Vec<ActionDraft>,
    pub enabled: bool,
    pub note: String,
}

impl RuleDraft {
    #[must_use]
    pub fn new() -> RuleDraft {
        RuleDraft {
            clauses: Vec::new(),
            join: Join::And,
            actions: Vec::new(),
            enabled: true,
            note: String::new(),
        }
    }

    /// Warunek reguły. Pusta lista klauzul to `ZAWSZE` — bo reguła bez warunku
    /// jest regułą bezwarunkową, a nie regułą niedokończoną.
    #[must_use]
    pub fn condition(&self) -> ConditionExpr {
        let mut it = self.clauses.iter().map(|c| ConditionExpr::Cmp {
            lhs: c.lhs.to_expr(),
            op: c.op,
            rhs: c.rhs.to_expr(),
        });
        let Some(pierwsza) = it.next() else {
            return ConditionExpr::Always;
        };
        it.fold(pierwsza, |acc, c| match self.join {
            Join::And => ConditionExpr::And(Box::new(acc), Box::new(c)),
            Join::Or => ConditionExpr::Or(Box::new(acc), Box::new(c)),
        })
    }

    #[must_use]
    pub fn to_rule(&self) -> Rule {
        Rule {
            when: self.condition(),
            then: self.actions.iter().map(ActionDraft::to_action).collect(),
            enabled: self.enabled,
            note: self.note.clone(),
        }
    }

    /// Reguła AST na szkic. `None`, gdy warunek nie jest płaskim łańcuchem porównań
    /// albo któraś akcja nie mieści się w formularzu.
    #[must_use]
    pub fn from_rule(r: &Rule) -> Option<RuleDraft> {
        let mut clauses = Vec::new();
        let join = splasz(&r.when, &mut clauses)?;
        let actions = r
            .then
            .iter()
            .map(ActionDraft::from_action)
            .collect::<Option<Vec<_>>>()?;
        Some(RuleDraft {
            clauses,
            join: join.unwrap_or_default(),
            actions,
            enabled: r.enabled,
            note: r.note.clone(),
        })
    }
}

/// Spłaszcza warunek do listy klauzul spiętych jednym spójnikiem.
///
/// `Ok(None)` znaczy „jeden warunek albo `ZAWSZE`", czyli spójnika nie ma i wolno
/// przyjąć domyślny. Mieszanka „I" z „LUB" i zaprzeczenie zwracają `None` — formularz
/// ich nie pokaże, więc nie udaje, że umie.
fn splasz(c: &ConditionExpr, out: &mut Vec<Clause>) -> Option<Option<Join>> {
    match c {
        ConditionExpr::Always => Some(None),
        ConditionExpr::Cmp { lhs, op, rhs } => {
            out.push(Clause {
                lhs: Slot::from_expr(lhs)?,
                op: *op,
                rhs: Slot::from_expr(rhs)?,
            });
            Some(None)
        }
        ConditionExpr::And(a, b) => spiete(a, b, Join::And, out),
        ConditionExpr::Or(a, b) => spiete(a, b, Join::Or, out),
        ConditionExpr::Not(_) => None,
    }
}

fn spiete(
    a: &ConditionExpr,
    b: &ConditionExpr,
    j: Join,
    out: &mut Vec<Clause>,
) -> Option<Option<Join>> {
    let la = splasz(a, out)?;
    let lb = splasz(b, out)?;
    if la.is_some_and(|x| x != j) || lb.is_some_and(|x| x != j) {
        return None;
    }
    Some(Some(j))
}

/// Szkic akcji: jeden worek pól na wszystkie trzynaście akcji.
///
/// Trzynaście struktur po jednej na akcję byłoby wierniejsze AST i **gorsze dla
/// formularza**: gracz zmieniający „ustaw cenę" na „przeceń o" nie ma zaczynać od
/// pustego wiersza. Worek trzyma to, co wpisał, i pokazuje pola, które akcja czyta;
/// reszta czeka nietknięta. To jest ta sama decyzja co przy `NewGameParams` w M9a —
/// szkic przeżywa zmianę zdania.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ActionDraft {
    pub kind: ActionKind,
    pub good: GoodRef,
    /// Pierwsze wyrażenie: cena, przyrost, dolne widełki, dni pokrycia, ilość, płaca.
    pub a: Slot,
    /// Drugie wyrażenie: górne widełki albo sufit podwyżki.
    pub b: Slot,
    /// Czy drugie wyrażenie jest ustawione — `RaiseWage` ma sufit opcjonalny.
    pub has_b: bool,
    pub bp: Bp,
    pub role: JobRoleId,
    pub count: u16,
    pub recipe: RecipeId,
    pub msg: u16,
    pub severity: Severity,
    pub source: OrderSource,
}

impl Default for ActionDraft {
    fn default() -> ActionDraft {
        ActionDraft {
            kind: ActionKind::SetPrice,
            good: GoodRef::This,
            a: Slot::lit(Value::Money(magnat_core::Money::ZERO)),
            b: Slot::lit(Value::Money(magnat_core::Money::ZERO)),
            has_b: false,
            bp: Bp::ONE,
            role: JobRoleId(0),
            count: 1,
            recipe: RecipeId(0),
            msg: 0,
            severity: Severity::Info,
            source: OrderSource::Market,
        }
    }
}

impl ActionDraft {
    #[must_use]
    pub fn set_price(good: GoodRef, to: Slot) -> ActionDraft {
        ActionDraft {
            kind: ActionKind::SetPrice,
            good,
            a: to,
            ..ActionDraft::default()
        }
    }

    #[must_use]
    pub fn margin(good: GoodRef, bp: Bp) -> ActionDraft {
        ActionDraft {
            kind: ActionKind::SetMargin,
            good,
            bp,
            ..ActionDraft::default()
        }
    }

    #[must_use]
    pub fn markdown(good: GoodRef, bp: Bp) -> ActionDraft {
        ActionDraft {
            kind: ActionKind::Markdown,
            good,
            bp,
            ..ActionDraft::default()
        }
    }

    #[must_use]
    pub fn clamp(good: GoodRef, min: Slot, max: Slot) -> ActionDraft {
        ActionDraft {
            kind: ActionKind::ClampPrice,
            good,
            a: min,
            b: max,
            has_b: true,
            ..ActionDraft::default()
        }
    }

    #[must_use]
    pub fn order_up_to(good: GoodRef, days: Slot) -> ActionDraft {
        ActionDraft {
            kind: ActionKind::OrderUpTo,
            good,
            a: days,
            ..ActionDraft::default()
        }
    }

    #[must_use]
    pub fn alert(msg: u16, severity: Severity) -> ActionDraft {
        ActionDraft {
            kind: ActionKind::Alert,
            msg,
            severity,
            ..ActionDraft::default()
        }
    }

    #[must_use]
    pub fn ask(msg: u16) -> ActionDraft {
        ActionDraft {
            kind: ActionKind::AskPlayer,
            msg,
            ..ActionDraft::default()
        }
    }

    /// Której jednostki oczekuje pierwsze wyrażenie tej akcji — po tym formularz
    /// filtruje listę slotu.
    #[must_use]
    pub const fn unit_a(kind: ActionKind) -> Option<Unit> {
        match kind {
            ActionKind::SetPrice
            | ActionKind::AdjustPrice
            | ActionKind::ClampPrice
            | ActionKind::Hire => Some(Unit::Money),
            ActionKind::OrderUpTo => Some(Unit::Days),
            ActionKind::OrderQty | ActionKind::PlanProduction => Some(Unit::Qty),
            _ => None,
        }
    }

    /// Której podstawy ceny oczekuje wyrażenie tej akcji (`K-7`).
    #[must_use]
    pub const fn basis_of(kind: ActionKind) -> Option<PriceBasis> {
        match kind {
            // Cena półkowa jest brutto — to jest kwota, którą płaci klient.
            ActionKind::SetPrice | ActionKind::AdjustPrice | ActionKind::ClampPrice => {
                Some(PriceBasis::GrossRetail)
            }
            // Płaca jest kosztem firmy, a koszt jest netto.
            ActionKind::Hire | ActionKind::RaiseWage => Some(PriceBasis::NetB2B),
            _ => None,
        }
    }

    #[must_use]
    pub fn to_action(&self) -> Action {
        match self.kind {
            ActionKind::SetPrice => Action::SetPrice {
                good: self.good,
                to: self.a.to_expr(),
            },
            ActionKind::AdjustPrice => Action::AdjustPrice {
                good: self.good,
                by: self.a.to_expr(),
            },
            ActionKind::SetMargin => Action::SetMargin {
                good: self.good,
                bp: self.bp,
            },
            ActionKind::ClampPrice => Action::ClampPrice {
                good: self.good,
                min: self.a.to_expr(),
                max: self.b.to_expr(),
            },
            ActionKind::OrderUpTo => Action::OrderUpTo {
                good: self.good,
                days: self.a.to_expr(),
            },
            ActionKind::OrderQty => Action::OrderQty {
                good: self.good,
                qty: self.a.to_expr(),
                source: self.source,
            },
            ActionKind::Markdown => Action::Markdown {
                good: self.good,
                bp: self.bp,
            },
            ActionKind::RemoveFromShelf => Action::RemoveFromShelf(self.good),
            ActionKind::Hire => Action::Hire {
                role: self.role,
                count: self.count,
                wage: self.a.to_expr(),
            },
            ActionKind::RaiseWage => Action::RaiseWage {
                role: self.role,
                by: self.bp,
                cap: self.has_b.then(|| self.b.to_expr()),
            },
            ActionKind::PlanProduction => Action::PlanProduction {
                recipe: self.recipe,
                qty: self.a.to_expr(),
            },
            ActionKind::Alert => Action::Alert {
                msg: self.msg,
                severity: self.severity,
            },
            ActionKind::AskPlayer => Action::AskPlayer { msg: self.msg },
        }
    }

    /// Akcja AST na szkic. `None`, gdy któreś wyrażenie nie mieści się w slocie.
    #[must_use]
    pub fn from_action(a: &Action) -> Option<ActionDraft> {
        let d = ActionDraft {
            kind: a.kind(),
            ..ActionDraft::default()
        };
        Some(match a {
            Action::SetPrice { good, to } => ActionDraft {
                good: *good,
                a: Slot::from_expr(to)?,
                ..d
            },
            Action::AdjustPrice { good, by } => ActionDraft {
                good: *good,
                a: Slot::from_expr(by)?,
                ..d
            },
            Action::SetMargin { good, bp } => ActionDraft {
                good: *good,
                bp: *bp,
                ..d
            },
            Action::ClampPrice { good, min, max } => ActionDraft {
                good: *good,
                a: Slot::from_expr(min)?,
                b: Slot::from_expr(max)?,
                has_b: true,
                ..d
            },
            Action::OrderUpTo { good, days } => ActionDraft {
                good: *good,
                a: Slot::from_expr(days)?,
                ..d
            },
            Action::OrderQty { good, qty, source } => ActionDraft {
                good: *good,
                a: Slot::from_expr(qty)?,
                source: *source,
                ..d
            },
            Action::Markdown { good, bp } => ActionDraft {
                good: *good,
                bp: *bp,
                ..d
            },
            Action::RemoveFromShelf(good) => ActionDraft { good: *good, ..d },
            Action::Hire { role, count, wage } => ActionDraft {
                role: *role,
                count: *count,
                a: Slot::from_expr(wage)?,
                ..d
            },
            Action::RaiseWage { role, by, cap } => ActionDraft {
                role: *role,
                bp: *by,
                b: match cap {
                    Some(c) => Slot::from_expr(c)?,
                    None => d.b,
                },
                has_b: cap.is_some(),
                ..d
            },
            Action::PlanProduction { recipe, qty } => ActionDraft {
                recipe: *recipe,
                a: Slot::from_expr(qty)?,
                ..d
            },
            Action::Alert { msg, severity } => ActionDraft {
                msg: *msg,
                severity: *severity,
                ..d
            },
            Action::AskPlayer { msg } => ActionDraft { msg: *msg, ..d },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::Money;

    #[test]
    fn slot_z_konwersja_i_skala_wraca_ten_sam() {
        let s = Slot::metric(Metric::UnitCost(GoodRef::This))
            .scaled(Bp(10_500))
            .converted(PriceBasis::GrossRetail);
        assert_eq!(Slot::from_expr(&s.to_expr()), Some(s));
        assert_eq!(s.unit(), Unit::Money);
        assert_eq!(s.basis(), Some(PriceBasis::GrossRetail));
    }

    #[test]
    fn drzewo_spoza_formularza_nie_udaje_ze_sie_miesci() {
        // Dwie metryki mnożone przez siebie — język to wyraża, formularz nie.
        let e = Expr::Bin {
            lhs: Box::new(Expr::Metric(Metric::CashBalance)),
            op: ArithOp::Add,
            rhs: Box::new(Expr::Metric(Metric::Receivables)),
        };
        assert_eq!(Slot::from_expr(&e), None);
    }

    #[test]
    fn presety_z_repozytorium_daja_sie_wyklikac() {
        // To jest twardsze niż wygląda: presety `data/policies/` powstały w M7c
        // ręcznie w RON-ie, a formularz M9d jest od nich późniejszy. Gdyby któryś
        // nie dał się pokazać, znaczyłoby to, że gracz nie umie zbudować polityki,
        // którą firma AI dostaje z pudełka — czyli że „ten sam zestaw narzędzi"
        // z PRD §6.3 jest nieprawdą.
        let c = magnat_policy::PolicyCatalog::load_default().expect("data/policies");
        for p in c.iter() {
            for (i, r) in p.policy.rules.iter().enumerate() {
                let d = RuleDraft::from_rule(r)
                    .unwrap_or_else(|| panic!("preset {} reguła {i}", p.key));
                assert_eq!(d.to_rule(), *r, "preset {} reguła {i}", p.key);
            }
        }
    }

    #[test]
    fn regula_bez_klauzul_jest_bezwarunkowa() {
        let d = RuleDraft::new();
        assert_eq!(d.condition(), ConditionExpr::Always);
    }

    #[test]
    fn widelki_wracaja_z_obu_stron() {
        let a = ActionDraft::clamp(
            GoodRef::This,
            Slot::lit(Value::Money(Money(100))),
            Slot::lit(Value::Money(Money(200))),
        );
        assert_eq!(ActionDraft::from_action(&a.to_action()), Some(a));
    }
}
