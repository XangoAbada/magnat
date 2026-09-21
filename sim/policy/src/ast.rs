//! Drzewo składniowe języka reguł (M9d §5.6 — **specyfikacja wiążąca dla M7**).
//!
//! Języka nie projektujemy tutaj. Projektuje go M9 (`K-11`) i to jego dokument mówi,
//! jakie są warunki, akcje, metryki i jednostki; M7 buduje ewaluator i rejestr metryk.
//! Odstępstwa od tamtej listy są wypisane w tabeli korekt `M7c-polityki-i-menedzerowie.md`
//! i wpisane z powrotem do `M9d-jezyk-regul.md` (`K-18`) — nie ma ich więcej niż tam.
//!
//! ## Reprezentacją kanoniczną jest AST, nie tekst
//!
//! Edytor M9 manipuluje drzewem przez sloty z listami rozwijanymi, więc gracz nie
//! może napisać błędu składniowego. Postać tekstowa istnieje do wyświetlania i do
//! `data/policies/`; **parser tekstu nie wykonuje się w pętli**.
//!
//! ## Arytmetyka
//!
//! W całości całkowitoliczbowa (00 §2). Pieniądz to `Money(i64)` w groszach, procenty
//! to [`Bp`] w punktach bazowych (10 000 = 100 %), mnożenie `Money × Bp` idzie przez
//! `i128` i `div_round_half_up`. To nie jest preferencja: polityka cenowa wykonana
//! przez menedżera zmienia stan trwały.

use magnat_core::{ActionKind, GoodId, JobRoleId, Money, PolicyId, PriceBasis, RecipeId};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// Punkty bazowe: 10 000 = 100 %. Osobny typ od `i32`, żeby „2 %" i „2 gr" nie dały
/// się porównać przez przeoczenie — walidator jednostek stoi właśnie na tym.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize,
)]
#[repr(transparent)]
pub struct Bp(pub i32);

impl Bp {
    pub const ONE: Bp = Bp(10_000);

    #[inline]
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// `kwota × bp`, zaokrąglone od zera. Jedyna droga mnożenia pieniądza przez procent.
    #[inline]
    #[must_use]
    pub fn of(self, m: Money) -> Money {
        m.mul_ratio(i64::from(self.0), 10_000)
    }
}

/// Dziedzina polityki — po niej dobiera się wykonawca akcji.
///
/// Wykonawcę ma dziś `Pricing` i `Stock` (sklep, M7c); `Hr`, `Production` i `Logistics`
/// są w słowniku, bo tak stanowi M9d, ale walidator je odrzuca do czasu, aż ktoś je
/// wykona — cicha polityka bez wykonawcy byłaby gorsza od odmowy, bo gracz widziałby
/// regułę, która „działa" i nic nie robi.
///
/// **Sprawdzone w R2e (poz. 55 wykazu, `R2-WP22`): odmowa działa i ma test**
/// (`Diagnostic::DomainNotAvailable`, `validate.rs`). Cichej polityki nie ma i nigdy
/// nie było — przegląd przed R2 zgłaszał ryzyko, które walidator już zamykał.
///
/// Przy okazji wyszło co innego i to zostaje nazwane, a nie naprawione:
/// **`Logistics` nie ma ani jednej akcji**, podczas gdy `Hr` ma dwie (`Hire`,
/// `RaiseWage`), a `Production` jedną (`PlanProduction`). Jest więc wariantem, do
/// którego nie da się dojść nawet po wpisaniu go na listę dostępnych. Nie znika tu,
/// bo kolejność wariantów jedzie w `Policy.domain`, czyli w zapisie gry, a usunięcie
/// wariantu jest zmianą formatu — należy do fazy, która transport zleceń w ogóle
/// zbuduje, albo do M12b razem z migracją. Wpisane do wykazu `R2` jako pozycja 81.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum PolicyDomain {
    Pricing,
    Stock,
    Hr,
    Production,
    Logistics,
}

/// Jak często polityka się wykonuje.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize,
)]
pub enum Cadence {
    #[default]
    Daily,
    /// Droższe — dla towarów nietrwałych.
    Hourly,
}

/// Wartość literalna wraz z jednostką. Jednostka **jest** typem: walidator porównuje
/// jednostki, a nie liczby, więc „< 2 dni" i „< 2 zł" to dwa różne warunki i jeden
/// z nich jest błędem.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Value {
    Money(Money),
    /// Sztuki w milisztukach (`Qty`).
    Qty(i64),
    Days(i32),
    Bp(Bp),
    Count(i32),
    /// Wariant słownika (sezon, dzień tygodnia) jako indeks.
    Enum(u8),
}

impl Value {
    /// Jednostka wartości — podstawa sprawdzenia zgodności w walidatorze.
    #[must_use]
    pub const fn unit(self) -> Unit {
        match self {
            Value::Money(_) => Unit::Money,
            Value::Qty(_) => Unit::Qty,
            Value::Days(_) => Unit::Days,
            Value::Bp(_) => Unit::Bp,
            Value::Count(_) => Unit::Count,
            Value::Enum(_) => Unit::Enum,
        }
    }

    /// Liczba, na której liczy arytmetyka. Pieniądz w groszach, reszta wprost.
    #[must_use]
    pub const fn raw(self) -> i64 {
        match self {
            Value::Money(m) => m.get(),
            Value::Qty(q) => q,
            Value::Days(d) | Value::Count(d) => d as i64,
            Value::Bp(b) => b.get() as i64,
            Value::Enum(e) => e as i64,
        }
    }

    /// Wartość tej samej jednostki z nową liczbą — wynik działania arytmetycznego.
    #[must_use]
    pub const fn with_raw(self, v: i64) -> Value {
        match self {
            Value::Money(_) => Value::Money(Money(v)),
            Value::Qty(_) => Value::Qty(v),
            Value::Days(_) => Value::Days(v as i32),
            Value::Bp(_) => Value::Bp(Bp(v as i32)),
            Value::Count(_) => Value::Count(v as i32),
            Value::Enum(_) => Value::Enum(v as u8),
        }
    }
}

/// Jednostka wyrażenia.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum Unit {
    Money,
    Qty,
    Days,
    Bp,
    Count,
    Enum,
}

/// Do którego towaru odnosi się metryka.
///
/// **Dodane wobec listy z M9d** (`AX-1`): tamten AST ma w metrykach gołe `GoodId`,
/// a wszystkie przykładowe polityki w tym samym paragrafie piszą `TEN_TOWAR` —
/// polityki o zakresie kategorii albo grupy stosują się do każdego towaru osobno
/// i nie mogą znać jego identyfikatora w chwili zapisu. Bez tego wariantu preset
/// w `data/policies/` nie da się zapisać inaczej niż jako lista czterystu kopii.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum GoodRef {
    /// Towar, dla którego polityka jest właśnie wykonywana (`TEN_TOWAR`).
    This,
    Id(GoodId),
}

/// Metryka — **jedyne** wejście reguły do świata.
///
/// Odczyt idzie wyłącznie przez [`crate::PolicyView`], czyli przez to, co firma widzi.
/// To jest ta sama asymetria informacji co w `FirmView` (M7 §5.8): reguła nie może
/// przeczytać kosztu konkurenta, bo widok jej tego nie poda.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum Metric {
    // ── cena i koszt — każda metryka cenowa NIESIE podstawę (`K-7`) ──
    Price {
        good: GoodRef,
        basis: PriceBasis,
    },
    UnitCost(GoodRef),
    Margin(GoodRef),
    CheapestCompetitorPrice {
        good: GoodRef,
        radius_m: u32,
        basis: PriceBasis,
    },
    AvgCompetitorPrice {
        good: GoodRef,
        radius_m: u32,
        basis: PriceBasis,
    },
    CompetitorCount {
        radius_m: u32,
    },
    // ── zapas — okna kroczące, więc niezależne od podziału na tygodnie ──
    Stock(GoodRef),
    StockDays(GoodRef),
    Turnover7d(GoodRef),
    Sales7d(GoodRef),
    DaysToExpiry(GoodRef),
    ShelfGap(GoodRef),
    // ── produkcja i ludzie ──
    MachineUtilization,
    OpenPositions(JobRoleId),
    StaffTurnover12m,
    MedianMarketWage(JobRoleId),
    StaffMood,
    ManagerSkill,
    // ── finanse i kontekst ──
    CashBalance,
    Receivables,
    Season,
    DayOfWeek,
    DayOfMonth,
    HourOfDay,
    DaysSinceLastChange(GoodRef),
}

impl Metric {
    /// Jednostka wyniku. Stała cecha metryki, nie zależy od świata — dlatego walidator
    /// sprawdza zgodność jednostek **bez** uruchamiania polityki.
    #[must_use]
    pub const fn unit(self) -> Unit {
        match self {
            Metric::Price { .. }
            | Metric::UnitCost(_)
            | Metric::CheapestCompetitorPrice { .. }
            | Metric::AvgCompetitorPrice { .. }
            | Metric::MedianMarketWage(_)
            | Metric::CashBalance
            | Metric::Receivables => Unit::Money,
            Metric::Margin(_) | Metric::MachineUtilization | Metric::StaffTurnover12m => Unit::Bp,
            Metric::Stock(_) | Metric::Turnover7d(_) | Metric::Sales7d(_) => Unit::Qty,
            Metric::StockDays(_) | Metric::DaysToExpiry(_) | Metric::DaysSinceLastChange(_) => {
                Unit::Days
            }
            Metric::CompetitorCount { .. }
            | Metric::ShelfGap(_)
            | Metric::OpenPositions(_)
            | Metric::StaffMood
            | Metric::ManagerSkill
            | Metric::DayOfMonth
            | Metric::HourOfDay => Unit::Count,
            Metric::Season | Metric::DayOfWeek => Unit::Enum,
        }
    }

    /// Podstawa ceny, jeśli metryka jest cenowa. `None` znaczy „nie dotyczy",
    /// a nie „domyślnie brutto" — walidator `PriceBasisMismatch` stoi na tej różnicy.
    #[must_use]
    pub const fn basis(self) -> Option<PriceBasis> {
        match self {
            Metric::Price { basis, .. }
            | Metric::CheapestCompetitorPrice { basis, .. }
            | Metric::AvgCompetitorPrice { basis, .. } => Some(basis),
            // Koszt własny i mediana płacy są z definicji netto — cena, którą firma
            // płaci, nie ma w sobie VAT-u przelotowego (`K-7`).
            Metric::UnitCost(_) | Metric::MedianMarketWage(_) => Some(PriceBasis::NetB2B),
            _ => None,
        }
    }

    /// Czy metryka pyta o konkurencję — te są drogie i jest ich limit na politykę.
    #[must_use]
    pub const fn is_competitive(self) -> bool {
        matches!(
            self,
            Metric::CheapestCompetitorPrice { .. }
                | Metric::AvgCompetitorPrice { .. }
                | Metric::CompetitorCount { .. }
        )
    }
}

/// Operator porównania.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum CmpOp {
    Lt,
    Le,
    Eq,
    Ne,
    Ge,
    Gt,
}

impl CmpOp {
    #[must_use]
    pub const fn holds(self, ord: std::cmp::Ordering) -> bool {
        use std::cmp::Ordering::{Equal, Greater, Less};
        matches!(
            (self, ord),
            (CmpOp::Lt, Less)
                | (CmpOp::Le, Less | Equal)
                | (CmpOp::Eq, Equal)
                | (CmpOp::Ne, Less | Greater)
                | (CmpOp::Ge, Greater | Equal)
                | (CmpOp::Gt, Greater)
        )
    }
}

/// Operator arytmetyczny. `Pct` to „procent z" z gramatyki M9d — mnożenie przez
/// punkty bazowe, które **zachowuje jednostkę lewej strony**.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum ArithOp {
    Add,
    Sub,
    Mul,
    Div,
    Pct,
}

/// Wyrażenie. Głębokość ≤ 3 (limit z M9d) — pilnuje jej walidator, więc rekurencja
/// w ewaluatorze nie ma jak uciec.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Expr {
    Lit(Value),
    Metric(Metric),
    Bin {
        lhs: Box<Expr>,
        op: ArithOp,
        rhs: Box<Expr>,
    },
    /// Jawna konwersja podstawy ceny — `brutto(x)` i `netto(x)` z gramatyki M9d.
    ///
    /// Bez niej `K-7` zamyka język: koszt własny jest netto, cena półkowa brutto,
    /// a walidator odrzuca porównanie mieszające podstawy. Sztandarowa polityka
    /// z PRD §6.3 ogranicza cenę do widełek **liczonych z kosztu** i bez tego węzła
    /// nie da się jej zapisać — ogranicznik odniesiony do własnej ceny niczego nie
    /// ogranicza, bo przycina wartość do przedziału wyprowadzonego z niej samej.
    ///
    /// Przelicznikiem jest stawka VAT towaru, którą podaje widok
    /// ([`crate::PolicyView::vat_bp`]). Do M8 wynosi zero i konwersja jest tożsamością —
    /// ale **typ** się zmienia i to jest cała jej treść przed M8.
    Convert {
        to: PriceBasis,
        of: Box<Expr>,
    },
}

/// Warunek.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ConditionExpr {
    And(Box<ConditionExpr>, Box<ConditionExpr>),
    Or(Box<ConditionExpr>, Box<ConditionExpr>),
    Not(Box<ConditionExpr>),
    Cmp { lhs: Expr, op: CmpOp, rhs: Expr },
    Always,
}

/// Skąd bierze się towar w zamówieniu.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum OrderSource {
    /// Najtańsza oferta na rynku B2B.
    Market,
    /// Stały dostawca, jeśli firma go ma.
    Preferred,
}

/// Waga alertu.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

/// Akcja. Kolejność wariantów jest ta sama co w [`ActionKind`] z `core` — to ona
/// indeksuje histogram akcji w panelu i wchodzi do `DecisionReason::Firm(FirmReason::PolicyApplied)`.
///
/// **Tekstu tu nie ma.** `Alert` i `AskPlayer` z M9d niosą `LocKey`, czyli klucz
/// lokalizacji — a `LocKey` mieszka w `engine/ui`, od którego symulacja nie zależy
/// i zależeć nie będzie. Klucz jest tu indeksem komunikatu polityki (`msg`), który
/// warstwa UI odwzorowuje na `ui.policy.msg.<n>`; korekta `AX-2`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Action {
    SetPrice {
        good: GoodRef,
        to: Expr,
    },
    AdjustPrice {
        good: GoodRef,
        by: Expr,
    },
    SetMargin {
        good: GoodRef,
        bp: Bp,
    },
    ClampPrice {
        good: GoodRef,
        min: Expr,
        max: Expr,
    },
    OrderUpTo {
        good: GoodRef,
        days: Expr,
    },
    OrderQty {
        good: GoodRef,
        qty: Expr,
        source: OrderSource,
    },
    Markdown {
        good: GoodRef,
        bp: Bp,
    },
    RemoveFromShelf(GoodRef),
    Hire {
        role: JobRoleId,
        count: u16,
        wage: Expr,
    },
    RaiseWage {
        role: JobRoleId,
        by: Bp,
        cap: Option<Expr>,
    },
    PlanProduction {
        recipe: RecipeId,
        qty: Expr,
    },
    Alert {
        msg: u16,
        severity: Severity,
    },
    /// Eskalacja: polityka się zatrzymuje i pyta gracza.
    AskPlayer {
        msg: u16,
    },
}

impl Action {
    /// Rodzaj akcji — to, co wchodzi do powodu decyzji.
    #[must_use]
    pub const fn kind(&self) -> ActionKind {
        match self {
            Action::SetPrice { .. } => ActionKind::SetPrice,
            Action::AdjustPrice { .. } => ActionKind::AdjustPrice,
            Action::SetMargin { .. } => ActionKind::SetMargin,
            Action::ClampPrice { .. } => ActionKind::ClampPrice,
            Action::OrderUpTo { .. } => ActionKind::OrderUpTo,
            Action::OrderQty { .. } => ActionKind::OrderQty,
            Action::Markdown { .. } => ActionKind::Markdown,
            Action::RemoveFromShelf(_) => ActionKind::RemoveFromShelf,
            Action::Hire { .. } => ActionKind::Hire,
            Action::RaiseWage { .. } => ActionKind::RaiseWage,
            Action::PlanProduction { .. } => ActionKind::PlanProduction,
            Action::Alert { .. } => ActionKind::Alert,
            Action::AskPlayer { .. } => ActionKind::AskPlayer,
        }
    }

    /// Dziedzina, w której akcja ma wykonawcę. `None` znaczy „w każdej".
    ///
    /// Alert i pytanie do gracza **nie zmieniają świata**, więc nie należą do żadnej
    /// dziedziny i wolno ich użyć wszędzie. To nie jest furtka: to jest jedyna para
    /// akcji, która niczego nie wykonuje, a bez niej polityka zapasu nie umiałaby
    /// powiedzieć „masz martwy zapas" — musiałaby coś przecenić, żeby cokolwiek
    /// zakomunikować.
    #[must_use]
    pub const fn domain(&self) -> Option<PolicyDomain> {
        match self {
            Action::SetPrice { .. }
            | Action::AdjustPrice { .. }
            | Action::SetMargin { .. }
            | Action::ClampPrice { .. }
            | Action::Markdown { .. } => Some(PolicyDomain::Pricing),
            Action::OrderUpTo { .. } | Action::OrderQty { .. } | Action::RemoveFromShelf(_) => {
                Some(PolicyDomain::Stock)
            }
            Action::Hire { .. } | Action::RaiseWage { .. } => Some(PolicyDomain::Hr),
            Action::PlanProduction { .. } => Some(PolicyDomain::Production),
            Action::Alert { .. } | Action::AskPlayer { .. } => None,
        }
    }
}

/// Reguła: warunek, akcje i notatka gracza.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Rule {
    pub when: ConditionExpr,
    pub then: SmallVec<[Action; 2]>,
    #[serde(default = "wlaczona")]
    pub enabled: bool,
    /// Notatka gracza. **Nie wpływa na wykonanie** i nie wchodzi do hasha stanu.
    #[serde(default)]
    pub note: String,
}

fn wlaczona() -> bool {
    true
}

/// Polityka: uporządkowana lista reguł, pierwsza pasująca wygrywa.
///
/// M7 §6 nazywa ten typ `FirmPolicy` i pod tą nazwą jest re-eksportowany — w M9d
/// nazywa się `Policy`, bo tam sterują nią i firma, i gracz. To jest jeden typ
/// o dwóch nazwach w dwóch dokumentach, a nie dwa typy (korekta `AX-3`).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Policy {
    pub id: PolicyId,
    /// Nazwa nadana przez gracza albo wzięta z presetu. Nie jest lokalizacją UI —
    /// to jest tekst gracza, tak samo jak nazwa firmy.
    pub name: String,
    pub domain: PolicyDomain,
    /// **Pierwsza pasująca wygrywa** — kolejność jest priorytetem.
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub fallback: Option<Action>,
    #[serde(default)]
    pub cadence: Cadence,
    /// Martwa strefa czasowa w godzinach — antyoscylacja.
    #[serde(default)]
    pub cooldown_h: u8,
}

/// Maksymalna liczba reguł w polityce (M9d: „maksymalnie 8 reguł na politykę").
pub const MAX_RULES: usize = 8;
/// Maksymalna głębokość wyrażenia.
pub const MAX_DEPTH: u8 = 3;
/// Maksymalny promień metryki konkurencyjnej w metrach.
pub const MAX_RADIUS_M: u32 = 10_000;
/// Maksymalna liczba metryk konkurencyjnych na politykę.
pub const MAX_COMPETITIVE: usize = 2;

impl Policy {
    /// Pusta polityka o zadanej dziedzinie — punkt wyjścia edytora i testów.
    #[must_use]
    pub fn empty(id: PolicyId, name: &str, domain: PolicyDomain) -> Policy {
        Policy {
            id,
            name: name.to_owned(),
            domain,
            rules: Vec::new(),
            fallback: None,
            cadence: Cadence::Daily,
            cooldown_h: 0,
        }
    }
}

/// Nazwa kontraktowa z M7 §6.
pub type FirmPolicy = Policy;
