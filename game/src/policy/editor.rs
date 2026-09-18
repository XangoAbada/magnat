//! Edytor reguł: formularz, który nie pozwala złożyć błędu (M9d §5.6, WP8).
//!
//! # Czego tu nie ma i dlaczego
//!
//! **Pisania.** Gracz nie wpisuje wyrażeń i nie ma jak: każda zmiana idzie przez
//! [`Edit`], a każdy `Edit` niosący wyrażenie jest **odrzucany**, jeśli nie zgadza się
//! jednostką albo podstawą ceny. To jest cała treść zdania „gracz nie pisze i nie może
//! napisać składniowego błędu" z §5.6 — walidator `sim/policy` zostaje drugą linią
//! dla polityk **z importu** i z zapisu sprzed zmiany danych, a nie pierwszą.
//!
//! **Parsera.** Reprezentacją kanoniczną jest drzewo, a tekst jest jego rzutem
//! ([`super::text`]).
//!
//! # Jedna diagnoza, której `sim/policy` postawić nie mógł
//!
//! `BelowCost` wypadło z walidatora w M7c i wraca tutaj, bo tutaj jest komplet danych:
//! sprowadzenie ceny półkowej i kosztu własnego do jednej podstawy wymaga **stawki
//! VAT**, a tej walidator języka nie widzi.
//!
//! **`ScopeConflict` nie wraca i to jest rozstrzygnięcie, nie przeoczenie.** Konflikt
//! zakresów wymaga dwóch polityk o tej samej szczegółowości na tym samym celu, a dziś
//! polityka przypina się **wyłącznie do zakładu** i zakład ma najwyżej jedną
//! (`SiteDelegation`). Diagnoza, której nie da się wywołać, przechodzi każdy test
//! i wygląda tak samo jak działająca (`K-67`). Wraca razem z przypinaniem do grupy
//! i do firmy, czyli wtedy, gdy dwa zakresy mogą się wreszcie spotkać.

use magnat_core::{ActionKind, PolicyId, PriceBasis};
use magnat_policy::{
    diagnose, Bp, Cadence, Diagnostic, GoodRef, Metric, Policy, PolicyDomain, PolicyScope, Unit,
    MAX_RULES,
};

use super::slot::{ActionDraft, Base, Clause, Join, RuleDraft, Slot};

/// Uwaga edytora: to, co widzi walidator języka, plus to, co widzi tylko edytor.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Note {
    /// Uwaga walidatora `sim/policy`.
    Language(Diagnostic),
    /// Ostrzeżenie: reguła schodzi z ceną pod koszt własny. **Nie błąd** — wojna
    /// cenowa bywa świadoma, a polityka, która nie pozwala jej wypowiedzieć, jest
    /// gorsza od ostrzeżenia, którego gracz nie przeczyta.
    BelowCost { rule: usize },
}

impl Note {
    #[must_use]
    pub fn is_error(&self) -> bool {
        match self {
            Note::Language(d) => d.is_error(),
            Note::BelowCost { .. } => false,
        }
    }
}

/// Czego edytor nie przyjął.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum EditError {
    NoRule(usize),
    NoClause(usize, usize),
    NoAction(usize, usize),
    /// Więcej reguł niż [`MAX_RULES`].
    TooManyRules,
    /// Obie strony porównania muszą mierzyć to samo.
    UnitMismatch { expected: Unit, got: Unit },
    /// Brutto do brutto, netto do netto (`K-7`). Konwersja istnieje, ale jawna.
    PriceBasisMismatch { lhs: PriceBasis, rhs: PriceBasis },
    /// Akcja, której w tej dziedzinie nikt nie wykona.
    ActionOutOfDomain { expected: PolicyDomain },
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditError::NoRule(i) => write!(f, "nie ma reguły {i}"),
            EditError::NoClause(r, i) => write!(f, "reguła {r} nie ma warunku {i}"),
            EditError::NoAction(r, i) => write!(f, "reguła {r} nie ma akcji {i}"),
            EditError::TooManyRules => write!(f, "polityka ma już {MAX_RULES} reguł"),
            EditError::UnitMismatch { expected, got } => {
                write!(f, "oczekiwano {expected:?}, jest {got:?}")
            }
            EditError::PriceBasisMismatch { lhs, rhs } => {
                write!(f, "{lhs:?} wobec {rhs:?}")
            }
            EditError::ActionOutOfDomain { expected } => {
                write!(f, "akcja spoza dziedziny {expected:?}")
            }
        }
    }
}

impl std::error::Error for EditError {}

/// Jedno kliknięcie w formularzu.
///
/// Wszystko, co gracz może zrobić z polityką, jest wariantem tego enuma — i to jest
/// warunek testu „sześć polityk zbudowanych wyłącznie klikaniem" (§7). Test, który
/// sięgałby po `Policy` bezpośrednio, sprawdzałby budowanie drzewa, a nie edytor.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Edit {
    Name(String),
    Domain(PolicyDomain),
    Cadence(Cadence),
    Cooldown(u8),
    Scope(PolicyScope),
    AddRule,
    DelRule(usize),
    MoveRule { from: usize, to: usize },
    Enable(usize, bool),
    Note(usize, String),
    Join(usize, Join),
    AddClause(usize, Clause),
    SetClause(usize, usize, Clause),
    DelClause(usize, usize),
    AddAction(usize, ActionDraft),
    SetAction(usize, usize, ActionDraft),
    DelAction(usize, usize),
    Fallback(Option<ActionDraft>),
}

/// Edytor jednej polityki.
pub struct RuleEditor {
    id: PolicyId,
    name: String,
    domain: PolicyDomain,
    cadence: Cadence,
    cooldown_h: u8,
    scope: PolicyScope,
    rules: Vec<RuleDraft>,
    fallback: Option<ActionDraft>,
    /// Stawka VAT towaru w punktach bazowych — wejście `BelowCost`. Do M8 zero.
    vat_bp: i32,
}

impl RuleEditor {
    #[must_use]
    pub fn new(id: PolicyId, name: &str, domain: PolicyDomain, scope: PolicyScope) -> RuleEditor {
        RuleEditor {
            id,
            name: name.to_owned(),
            domain,
            cadence: Cadence::Daily,
            cooldown_h: 0,
            scope,
            rules: Vec::new(),
            fallback: None,
            vat_bp: 0,
        }
    }

    /// Otwiera istniejącą politykę. `None`, gdy któraś reguła nie mieści się
    /// w formularzu — wtedy polityka jest do obejrzenia tekstem, a nie do edycji,
    /// i lepiej to powiedzieć wprost niż pokazać połowę.
    #[must_use]
    pub fn from_policy(p: &Policy, scope: PolicyScope) -> Option<RuleEditor> {
        let rules = p
            .rules
            .iter()
            .map(RuleDraft::from_rule)
            .collect::<Option<Vec<_>>>()?;
        let fallback = match &p.fallback {
            Some(a) => Some(ActionDraft::from_action(a)?),
            None => None,
        };
        Some(RuleEditor {
            id: p.id,
            name: p.name.clone(),
            domain: p.domain,
            cadence: p.cadence,
            cooldown_h: p.cooldown_h,
            scope,
            rules,
            fallback,
            vat_bp: 0,
        })
    }

    /// Stawka VAT, po której `BelowCost` sprowadza cenę do netto.
    #[must_use]
    pub fn with_vat(mut self, vat_bp: i32) -> RuleEditor {
        self.vat_bp = vat_bp;
        self
    }

    #[must_use]
    pub fn scope(&self) -> &PolicyScope {
        &self.scope
    }

    #[must_use]
    pub fn rules(&self) -> &[RuleDraft] {
        &self.rules
    }

    #[must_use]
    pub fn fallback(&self) -> Option<&ActionDraft> {
        self.fallback.as_ref()
    }

    #[must_use]
    pub fn domain(&self) -> PolicyDomain {
        self.domain
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Polityka w postaci, w jakiej pojedzie do zakładu.
    #[must_use]
    pub fn policy(&self) -> Policy {
        Policy {
            id: self.id,
            name: self.name.clone(),
            domain: self.domain,
            rules: self.rules.iter().map(RuleDraft::to_rule).collect(),
            fallback: self.fallback.as_ref().map(ActionDraft::to_action),
            cadence: self.cadence,
            cooldown_h: self.cooldown_h,
        }
    }

    /// Jedno kliknięcie.
    ///
    /// # Errors
    /// [`EditError`], gdy zmiana nie trzyma się jednostek, podstawy ceny, dziedziny
    /// albo limitu reguł. Stan edytora zostaje wtedy **nietknięty** — odrzucona zmiana
    /// nie zostawia po sobie połowy.
    pub fn apply(&mut self, e: Edit) -> Result<(), EditError> {
        match e {
            Edit::Name(n) => self.name = n,
            Edit::Domain(d) => self.domain = d,
            Edit::Cadence(c) => self.cadence = c,
            Edit::Cooldown(h) => self.cooldown_h = h,
            Edit::Scope(s) => self.scope = s,
            Edit::AddRule => {
                if self.rules.len() >= MAX_RULES {
                    return Err(EditError::TooManyRules);
                }
                self.rules.push(RuleDraft::new());
            }
            Edit::DelRule(i) => {
                self.rule(i)?;
                self.rules.remove(i);
            }
            Edit::MoveRule { from, to } => {
                self.rule(from)?;
                self.rule(to)?;
                let r = self.rules.remove(from);
                self.rules.insert(to, r);
            }
            Edit::Enable(i, v) => self.rules.get_mut(i).ok_or(EditError::NoRule(i))?.enabled = v,
            Edit::Note(i, n) => self.rules.get_mut(i).ok_or(EditError::NoRule(i))?.note = n,
            Edit::Join(i, j) => self.rules.get_mut(i).ok_or(EditError::NoRule(i))?.join = j,
            Edit::AddClause(i, c) => {
                self.rule(i)?;
                sprawdz_klauzule(&c)?;
                self.rules[i].clauses.push(c);
            }
            Edit::SetClause(i, j, c) => {
                self.klauzula(i, j)?;
                sprawdz_klauzule(&c)?;
                self.rules[i].clauses[j] = c;
            }
            Edit::DelClause(i, j) => {
                self.klauzula(i, j)?;
                self.rules[i].clauses.remove(j);
            }
            Edit::AddAction(i, a) => {
                self.rule(i)?;
                sprawdz_akcje(&a, self.domain)?;
                self.rules[i].actions.push(a);
            }
            Edit::SetAction(i, j, a) => {
                self.akcja(i, j)?;
                sprawdz_akcje(&a, self.domain)?;
                self.rules[i].actions[j] = a;
            }
            Edit::DelAction(i, j) => {
                self.akcja(i, j)?;
                self.rules[i].actions.remove(j);
            }
            Edit::Fallback(a) => {
                if let Some(a) = &a {
                    sprawdz_akcje(a, self.domain)?;
                }
                self.fallback = a;
            }
        }
        Ok(())
    }

    fn rule(&self, i: usize) -> Result<(), EditError> {
        if i < self.rules.len() {
            Ok(())
        } else {
            Err(EditError::NoRule(i))
        }
    }

    fn klauzula(&self, i: usize, j: usize) -> Result<(), EditError> {
        self.rule(i)?;
        if j < self.rules[i].clauses.len() {
            Ok(())
        } else {
            Err(EditError::NoClause(i, j))
        }
    }

    fn akcja(&self, i: usize, j: usize) -> Result<(), EditError> {
        self.rule(i)?;
        if j < self.rules[i].actions.len() {
            Ok(())
        } else {
            Err(EditError::NoAction(i, j))
        }
    }

    /// Pełna diagnostyka: język plus to, co widzi tylko edytor.
    ///
    /// Kolejność jest deterministyczna — najpierw walidator języka w swojej kolejności,
    /// potem uwagi edytora po numerach reguł.
    #[must_use]
    pub fn notes(&self) -> Vec<Note> {
        let p = self.policy();
        let mut out: Vec<Note> = diagnose(&p).0.into_iter().map(Note::Language).collect();
        for (i, r) in self.rules.iter().enumerate() {
            if r.actions.iter().any(|a| self.pod_kosztem(a)) {
                out.push(Note::BelowCost { rule: i });
            }
        }
        out
    }

    /// Czy ta akcja schodzi z ceną pod koszt własny.
    ///
    /// Sprowadzenie do jednej podstawy jest tu istotą, a nie szczegółem: koszt własny
    /// jest netto, cena półkowa brutto, więc porównanie bez VAT-u byłoby fałszywe
    /// dokładnie o stawkę podatku. Do M8 stawka wynosi zero i różnicy nie widać —
    /// ale ten kod ma jej nie zgubić, kiedy przestanie.
    fn pod_kosztem(&self, a: &ActionDraft) -> bool {
        // Marża niedodatnia znaczy sprzedaż po koszcie albo poniżej — wprost.
        if a.kind == ActionKind::SetMargin && a.bp.get() <= 0 {
            return true;
        }
        // Cena wyprowadzona z kosztu ze współczynnikiem poniżej stu procent —
        // po sprowadzeniu obu stron do netto, czyli po zdjęciu VAT-u z brutta.
        let ponizej = |s: &Slot| {
            if !matches!(s.base, Base::Metric(Metric::UnitCost(_))) {
                return false;
            }
            let skala = i64::from(s.scale.unwrap_or(Bp::ONE).get());
            let prog = if s.convert == Some(PriceBasis::GrossRetail) {
                10_000 + i64::from(self.vat_bp.max(0))
            } else {
                10_000
            };
            skala < prog
        };
        match a.kind {
            ActionKind::SetPrice => ponizej(&a.a),
            // Przy widełkach liczy się **górny** kraniec: dolny wolno postawić nisko,
            // bo cena i tak w niego nie zjedzie, jeśli górny trzyma ją wyżej.
            ActionKind::ClampPrice => ponizej(&a.b),
            _ => false,
        }
    }

    /// Metryki, które wolno postawić w slocie po tej stronie porównania.
    ///
    /// To jest miejsce, w którym „edytor oferuje wyłącznie wyrażenia zgodne" przestaje
    /// być zdaniem w dokumencie: lista slotu jest **wynikiem tej funkcji**, a nie
    /// kompletem metryk z filtrem po stronie rysowania.
    #[must_use]
    pub fn options_for(&self, wobec: Option<&Slot>) -> Vec<Base> {
        let wszystkie = metryki(self.domain);
        wszystkie
            .into_iter()
            .map(Base::Metric)
            .filter(|b| match wobec {
                None => true,
                Some(l) => {
                    let s = Slot {
                        base: *b,
                        scale: None,
                        convert: None,
                    };
                    zgodne(l, &s).is_ok()
                }
            })
            .collect()
    }
}

/// Metryki, o które wolno pytać w polityce tej dziedziny.
///
/// Dziedzina nie ogranicza **odczytu** — reguła cenowa wolno pyta o zapas i odwrotnie,
/// bo „przeceń, gdy zalega" jest polityką cenową czytającą zapas. Ogranicza za to
/// akcje, i tam jest właściwe miejsce na to pytanie.
#[must_use]
pub fn metryki(_domain: PolicyDomain) -> Vec<Metric> {
    let t = GoodRef::This;
    vec![
        Metric::Price {
            good: t,
            basis: PriceBasis::GrossRetail,
        },
        Metric::Price {
            good: t,
            basis: PriceBasis::NetB2B,
        },
        Metric::UnitCost(t),
        Metric::Margin(t),
        Metric::CheapestCompetitorPrice {
            good: t,
            radius_m: 3_000,
            basis: PriceBasis::GrossRetail,
        },
        Metric::AvgCompetitorPrice {
            good: t,
            radius_m: 3_000,
            basis: PriceBasis::GrossRetail,
        },
        Metric::CompetitorCount { radius_m: 3_000 },
        Metric::Stock(t),
        Metric::StockDays(t),
        Metric::Turnover7d(t),
        Metric::Sales7d(t),
        Metric::DaysToExpiry(t),
        Metric::ShelfGap(t),
        Metric::MachineUtilization,
        Metric::OpenPositions(magnat_core::JobRoleId(0)),
        Metric::StaffTurnover12m,
        Metric::MedianMarketWage(magnat_core::JobRoleId(0)),
        Metric::StaffMood,
        Metric::ManagerSkill,
        Metric::CashBalance,
        Metric::Receivables,
        Metric::Season,
        Metric::DayOfWeek,
        Metric::DayOfMonth,
        Metric::HourOfDay,
        Metric::DaysSinceLastChange(t),
    ]
}

/// Czy dwa sloty wolno porównać.
fn zgodne(l: &Slot, r: &Slot) -> Result<(), EditError> {
    if l.unit() != r.unit() {
        return Err(EditError::UnitMismatch {
            expected: l.unit(),
            got: r.unit(),
        });
    }
    if let (Some(a), Some(b)) = (l.basis(), r.basis()) {
        if a != b {
            return Err(EditError::PriceBasisMismatch { lhs: a, rhs: b });
        }
    }
    Ok(())
}

fn sprawdz_klauzule(c: &Clause) -> Result<(), EditError> {
    zgodne(&c.lhs, &c.rhs)
}

fn sprawdz_akcje(a: &ActionDraft, domain: PolicyDomain) -> Result<(), EditError> {
    if let Some(d) = a.to_action().domain() {
        if d != domain {
            return Err(EditError::ActionOutOfDomain { expected: domain });
        }
    }
    let sprawdz = |s: &Slot| -> Result<(), EditError> {
        if let Some(u) = ActionDraft::unit_a(a.kind) {
            if s.unit() != u {
                return Err(EditError::UnitMismatch {
                    expected: u,
                    got: s.unit(),
                });
            }
        }
        if let (Some(oczekiwana), Some(ma)) = (ActionDraft::basis_of(a.kind), s.basis()) {
            if oczekiwana != ma {
                return Err(EditError::PriceBasisMismatch {
                    lhs: oczekiwana,
                    rhs: ma,
                });
            }
        }
        Ok(())
    };
    // Akcje bez wyrażenia (marża, przecena, wycofanie, alert) nie mają czego sprawdzać.
    if ActionDraft::unit_a(a.kind).is_none() && ActionDraft::basis_of(a.kind).is_none() {
        return Ok(());
    }
    sprawdz(&a.a)?;
    if a.has_b {
        sprawdz(&a.b)?;
    }
    Ok(())
}
