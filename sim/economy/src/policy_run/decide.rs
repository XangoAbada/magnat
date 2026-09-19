//! Co reguła **znaczy** dla zakładu i co menedżer z tego robi (M7c WP7, M9d WP9).
//!
//! # Trzy kroki, nie dwa
//!
//! 1. **Warunek** liczy się na faktach z początku kroku — doba polityki ocenia sklep
//!    takim, jakim był rano, a nie takim, jakim uczyniła go poprzednia reguła.
//! 2. **Akcja** liczy się na faktach bieżących, bo druga akcja tej samej reguły ma
//!    widzieć, co zrobiła pierwsza: „ustaw cenę ORAZ ogranicz do widełek" bez tego
//!    cofa własną pierwszą akcję.
//! 3. **Menedżer** przesuwa wynik o swój błąd. To jest krok trzeci, a nie poprawka
//!    wewnątrz drugiego: reguła ma cel, a menedżer w niego celuje — i różnica między
//!    jednym a drugim jest tym, co karta inspekcji pokazuje graczowi.
//!
//! Kroki 1–3 są tą samą funkcją dla wykonania i dla dry-runu ([`decyduj`]). Druga
//! arytmetyka po stronie podglądu rozjechałaby się z wykonaniem przy pierwszej zmianie
//! wzoru — a dry-run istnieje właśnie po to, żeby powiedzieć, co się stanie.

use magnat_core::{DecisionReason, GoodId, Money, PolicyId, Qty, SimCalendar, Tick};
use magnat_policy::{
    evaluate, Action, Bp, Expr, GoodRef, MetricCtx, Policy, PolicyView, Severity, Value,
};

use super::facts::{GoodFacts, ShopView};
use crate::kernel::BP;
use crate::manager_exec::ManagerExecution;
use crate::pricing::PricePolicy;
use crate::shop::{ReorderPolicy, Shop};

/// Co wyszło z wykonania jednej akcji — licznik doby, nie decyzja.
pub(crate) enum Wynik {
    Zrobione,
    Alert,
    /// Wyrażenia nie dało się policzyć — któraś metryka nie miała wartości.
    Slepa,
}

/// Co akcja **znaczy** dla sklepu, po policzeniu wszystkich wyrażeń.
///
/// Rozdzielenie „policz" od „zastosuj" jest tu warunkiem poprawności, a nie
/// porządkiem: liczenie idzie przez [`magnat_policy::eval`] nad widokiem, który
/// pożycza tablicę faktów, a zastosowanie tę tablicę **aktualizuje** — bo druga
/// akcja tej samej reguły musi widzieć cenę, którą ustawiła pierwsza. Bez tego
/// „ustaw cenę ORAZ ogranicz do widełek" cofa własną pierwszą akcję i sztandarowa
/// polityka z PRD §6.3 nie robi nic.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PolicyOutcome {
    /// Cena półkowa brutto, po odchyleniu menedżera.
    Price(GoodId, Money),
    /// Marża docelowa w punktach bazowych.
    Margin(GoodId, i32),
    /// Cel zapasu w sztukach.
    Order(GoodId, Qty),
    /// Alert albo pytanie do gracza. `ask` odróżnia „wiedz o tym" od „zdecyduj".
    Alert {
        msg: u16,
        severity: Severity,
        ask: bool,
    },
    /// Wyrażenia nie dało się policzyć.
    Blind,
}

/// Kontekst jednego wykonania polityki — wszystko, co nie jest towarem.
pub(crate) struct RunCtx {
    pub cal: SimCalendar,
    pub vat_bp: i32,
    pub cash: Option<Money>,
    pub manager_skill: Option<i32>,
    pub staff_mood: Option<i32>,
    pub open_positions: i32,
    pub exec: ManagerExecution,
    /// Rzut menedżera na tę dobę, w punktach bazowych. Jeden na zakład i dobę,
    /// a nie jeden na towar: niedokładność jest cechą ręki, a nie półki.
    pub error_bp: i32,
}

impl RunCtx {
    fn widok<'a>(&self, goods: &'a [GoodFacts]) -> ShopView<'a> {
        ShopView {
            goods,
            cash: self.cash,
            manager_skill: self.manager_skill,
            staff_mood: self.staff_mood,
            open_positions: self.open_positions,
            cal: self.cal,
            vat_bp: self.vat_bp,
        }
    }
}

/// Wykonuje politykę na jednym zakładzie i zwraca decyzje wraz z powodami.
///
/// **Aktualizuje `fakty`** — to jest część kontraktu, a nie efekt uboczny: kolejna
/// akcja czyta z nich cenę przez widok. Wołający dostaje decyzje i sam rozstrzyga,
/// czy je zapisać (wykonanie), czy tylko pokazać (dry-run).
pub(crate) fn decyduj(
    polityka: &Policy,
    fakty: &mut [GoodFacts],
    ctx: &RunCtx,
    tax: &dyn crate::tax::TaxEngine,
) -> Vec<(PolicyOutcome, DecisionReason)> {
    let mut out = Vec::new();
    // Krok 1: warunki na stanie z początku kroku, wszystkie naraz.
    let mut zamierzone: Vec<(GoodId, Action, DecisionReason)> = Vec::new();
    {
        let v = ctx.widok(fakty);
        for f in fakty.iter() {
            for dec in evaluate(polityka, &MetricCtx::for_good(f.good), &v) {
                let (a, r) = dec.split();
                // Kopia akcji **raz na wyzwoloną regułę**, a nie raz na ewaluację:
                // ewaluator zwraca referencje do polityki właśnie po to, żeby nie
                // alokować, a wykonawca potrzebuje własności, bo między ewaluacją
                // a zapisem pożycza fakty na mutowalnie.
                zamierzone.push((f.good, a.clone(), r));
            }
        }
    }

    // Krok 2 i 3: wyrażenia akcji na stanie bieżącym, potem ręka menedżera.
    let mut zatrzymany: Option<GoodId> = None;
    for (good, a, powod) in zamierzone {
        // „Zapytaj gracza" zatrzymuje politykę **dla tego towaru** (M9d §5.6):
        // automatyzacja, której nie da się przerwać, jest gorsza niż jej brak.
        if zatrzymany == Some(good) {
            continue;
        }
        let kontekst = MetricCtx::for_good(good);
        let skutek = {
            let v = ctx.widok(fakty);
            rozstrzygnij(&a, &kontekst, &v, fakty)
        };
        let (skutek, odchylenie) = reka_menedzera(skutek, ctx);
        if let PolicyOutcome::Alert { ask: true, .. } = skutek {
            zatrzymany = Some(good);
        }
        rzutuj(fakty, skutek, tax);
        out.push((
            skutek,
            dopisz_menedzera(powod, ctx.exec.info_lag_days, odchylenie),
        ));
    }
    out
}

/// Przesuwa wynik reguły o błąd menedżera i mówi, o ile.
fn reka_menedzera(s: PolicyOutcome, ctx: &RunCtx) -> (PolicyOutcome, i16) {
    if ctx.error_bp == 0 {
        return (s, 0);
    }
    let bp = ctx.error_bp.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
    match s {
        PolicyOutcome::Price(g, m) => (
            PolicyOutcome::Price(g, ctx.exec.distort(m, ctx.error_bp)),
            bp,
        ),
        PolicyOutcome::Margin(g, v) => (
            PolicyOutcome::Margin(g, ctx.exec.distort_i32(v, ctx.error_bp)),
            bp,
        ),
        PolicyOutcome::Order(g, q) => (
            PolicyOutcome::Order(g, Qty(ctx.exec.distort(Money(q.get()), ctx.error_bp).get())),
            bp,
        ),
        // Alert i pytanie nie mają wartości, w którą można spudłować.
        inne => (inne, 0),
    }
}

/// Dopisuje do powodu to, czego ewaluator wiedzieć nie mógł: wiek danych i odchyłkę.
///
/// `sim/policy` buduje `PolicyApplied` z zerami, bo nie zna menedżera — zna go
/// wykonawca. Zamiast drugiego wariantu powodu („PolicyAppliedByManager") uzupełnia
/// się ten sam, bo dla gracza to jest jedno zdarzenie.
fn dopisz_menedzera(powod: DecisionReason, lag: u8, dev: i16) -> DecisionReason {
    match powod {
        DecisionReason::PolicyApplied {
            policy,
            rule,
            action,
            ..
        } => DecisionReason::PolicyApplied {
            policy,
            rule,
            action,
            lag_days: lag,
            deviation_bp: dev,
        },
        inne => inne,
    }
}

/// Towar, którego akcja dotyczy. `This` bierze towar z kontekstu wykonania,
/// `Id` — ten wskazany w regule.
///
/// Do poprawki po recenzji M7c każde ramię wykonawcy odrzucało to pole (`..`)
/// i stosowało akcję do towaru z pętli: `SetPrice { good: Id(7) }` ustawiał cenę
/// akurat iterowanego towaru, a nie siódmego.
fn cel(r: GoodRef, ctx: &MetricCtx) -> Option<GoodId> {
    match r {
        GoodRef::Id(g) => Some(g),
        GoodRef::This => ctx.good,
    }
}

/// Liczy, co akcja znaczy — **tym samym ewaluatorem**, którym policzył się warunek.
///
/// Druga implementacja arytmetyki po stronie wykonawcy byłaby dokładnie tym rozjazdem,
/// przed którym `sim/policy` ma chronić (`K-11`), tylko po stronie, która pisze
/// do świata. Zmierzone przed poprawką: `Money + Days` dawało wynik w akcji i `None`
/// w warunku, a `Money × Money` przechodziło wyłącznie w akcji.
fn rozstrzygnij(
    a: &Action,
    ctx: &MetricCtx,
    v: &impl PolicyView,
    fakty: &[GoodFacts],
) -> PolicyOutcome {
    let kwota = |e: &Expr| match magnat_policy::eval(e, ctx, v) {
        Some(Value::Money(m)) => Some(m),
        _ => None,
    };
    let fakt = |g: GoodId| fakty.iter().find(|f| f.good == g);
    // Cena poniżej grosza znaczy „oddaję towar za darmo" i jest błędem wyrażenia,
    // a nie decyzją. Przycięcie stoi tutaj, a nie przy zapisie, żeby dry-run pokazał
    // tę samą liczbę, którą zobaczy półka.
    let cena = |g: GoodId, m: Money| PolicyOutcome::Price(g, Money(m.get().max(1)));
    match a {
        Action::SetPrice { good, to } => match (cel(*good, ctx), kwota(to)) {
            (Some(g), Some(c)) => cena(g, c),
            _ => PolicyOutcome::Blind,
        },
        Action::AdjustPrice { good, by } => {
            let (Some(g), Some(delta)) = (cel(*good, ctx), kwota(by)) else {
                return PolicyOutcome::Blind;
            };
            let Some(f) = fakt(g) else {
                return PolicyOutcome::Blind;
            };
            cena(g, Money(f.price_gross.get().saturating_add(delta.get())))
        }
        Action::SetMargin { good, bp } => match cel(*good, ctx) {
            Some(g) => PolicyOutcome::Margin(g, bp.get()),
            None => PolicyOutcome::Blind,
        },
        Action::Markdown { good, bp } => {
            let (Some(g), Some(f)) = (cel(*good, ctx), cel(*good, ctx).and_then(fakt)) else {
                return PolicyOutcome::Blind;
            };
            cena(g, Bp(BP as i32 - bp.get()).of(f.price_gross))
        }
        Action::ClampPrice { good, min, max } => {
            let (Some(g), Some(lo), Some(hi)) = (cel(*good, ctx), kwota(min), kwota(max)) else {
                return PolicyOutcome::Blind;
            };
            let Some(f) = fakt(g) else {
                return PolicyOutcome::Blind;
            };
            // Widełki podane odwrotnie są błędem gracza, a nie powodem do paniki:
            // bierzemy je w tej kolejności, w której są przedziałem.
            let (lo, hi) = (lo.get().min(hi.get()), lo.get().max(hi.get()));
            cena(g, Money(f.price_gross.get().clamp(lo, hi)))
        }
        Action::OrderUpTo { good, days } => {
            let Some(g) = cel(*good, ctx) else {
                return PolicyOutcome::Blind;
            };
            let (Some(Value::Days(dni)), Some(f)) = (magnat_policy::eval(days, ctx, v), fakt(g))
            else {
                return PolicyOutcome::Blind;
            };
            let dziennie = (f.turnover_7d.get() / 7).max(1);
            PolicyOutcome::Order(g, Qty(dziennie.saturating_mul(i64::from(dni.max(0)))))
        }
        Action::OrderQty { good, qty, .. } => {
            let Some(g) = cel(*good, ctx) else {
                return PolicyOutcome::Blind;
            };
            match magnat_policy::eval(qty, ctx, v) {
                Some(Value::Qty(q)) => PolicyOutcome::Order(g, Qty(q.max(0))),
                _ => PolicyOutcome::Blind,
            }
        }
        Action::Alert { msg, severity } => PolicyOutcome::Alert {
            msg: *msg,
            severity: *severity,
            ask: false,
        },
        Action::AskPlayer { msg } => PolicyOutcome::Alert {
            msg: *msg,
            severity: Severity::Warning,
            ask: true,
        },
        // Wycofanie z półki wymaga zwolnienia oferty w arenie razem z linią półki
        // (M5b §5.3), a to jest ta sama ścieżka, którą zamyka zakład w M7d — tam
        // powstanie raz, a nie dwa razy.
        Action::RemoveFromShelf(_) => PolicyOutcome::Blind,
        Action::Hire { .. } | Action::RaiseWage { .. } | Action::PlanProduction { .. } => {
            // Walidator nie przepuszcza polityki w tych dziedzinach (`DomainNotAvailable`),
            // więc tutaj nie da się dojść inaczej niż polityką z zapisu gry sprzed
            // zmiany danych. Wtedy lepiej nie zrobić nic, niż zrobić coś innego.
            PolicyOutcome::Blind
        }
    }
}

/// Rzutuje skutek na tablicę faktów. Wołane także w dry-runie — tam jest **jedynym**
/// zapisem, bo sklepu nikt nie dotyka.
fn rzutuj(fakty: &mut [GoodFacts], s: PolicyOutcome, tax: &dyn crate::tax::TaxEngine) {
    if let PolicyOutcome::Price(good, cena) = s {
        if let Some(f) = fakty.iter_mut().find(|f| f.good == good) {
            f.price_gross = cena;
            f.price_net = tax.net_from_gross(good, cena);
        }
    }
}

/// Zapisuje skutek do sklepu.
pub(crate) fn zastosuj(shop: &mut Shop, s: PolicyOutcome, t: Tick) -> Wynik {
    match s {
        PolicyOutcome::Blind => Wynik::Slepa,
        PolicyOutcome::Alert { .. } => Wynik::Alert,
        PolicyOutcome::Price(good, cena) => {
            let Some(pc) = shop.controllers.get_mut(&good) else {
                return Wynik::Slepa;
            };
            // Sterownik przechodzi na `Fixed`, bo to jest **znaczenie** akcji „ustaw
            // cenę": od tej chwili cena stoi tam, gdzie ją postawiono, a dobowa przecena
            // wyłącznie pilnuje ogranicznika marży. Bez tego `reprice_all` nadpisałby
            // wynik polityki tego samego dnia i reguła gracza nie robiłaby nic.
            pc.policy = PricePolicy::Fixed { price: cena };
            pc.current = cena;
            pc.last_change = t;
            Wynik::Zrobione
        }
        PolicyOutcome::Margin(good, bp) => match shop.controllers.get_mut(&good) {
            Some(pc) => {
                pc.policy = PricePolicy::Markup {
                    target_margin_bp: bp,
                };
                Wynik::Zrobione
            }
            None => Wynik::Slepa,
        },
        // **Uwaga na sprzężenie z inflacją emergentną.** `docelowy_zapas` w `restock.rs`
        // celowo **nie** wiąże celu zamówienia z obrotem dla sklepów sprzedających, bo
        // presja zapasu jest w M5 jedynym kanałem, którym pieniądz dochodzi do cen —
        // związanie celu z obrotem odwróciło tam znak bramek G1–G3. Tutaj wolno to
        // zrobić, bo dotyczy **wyłącznie zakładów zdelegowanych**, czyli takich, którym
        // ktoś tę politykę świadomie przypiął. Faza, która zdeleguje wszystkie sklepy
        // miasta naraz, musi przemierzyć bramki G1–G3.
        PolicyOutcome::Order(good, cel) => {
            let p = shop.inventory.reorder.entry(good).or_insert(ReorderPolicy {
                point: Qty::ZERO,
                target: Qty::ZERO,
                lead_time_days: 2,
            });
            p.target = cel;
            // Punkt zamówienia to trzy dziesiąte celu: zapas schodzący poniżej niego
            // wyzwala dostawę z zapasem na czas dojazdu. Bez punktu sklep zamawiałby
            // codziennie po jednej sztuce. `saturating_mul`, bo `cel` mógł już nasycić
            // się na `i64::MAX` przy absurdalnym wyrażeniu z polityki.
            p.point = Qty(cel.get().saturating_mul(3) / 10);
            Wynik::Zrobione
        }
    }
}

/// Numer polityki z powodu — po nim wpis w skrzynce wraca do reguły, która go wystawiła.
pub(crate) fn z_powodu(powod: DecisionReason) -> (PolicyId, u8) {
    match powod {
        DecisionReason::PolicyApplied { policy, rule, .. } => (policy, rule),
        _ => (PolicyId(0), 0),
    }
}
