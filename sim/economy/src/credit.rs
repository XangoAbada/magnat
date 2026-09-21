//! Kredyt: harmonogram, ocena zdolności, kreacja i destrukcja pieniądza (M5d §5.10).
//!
//! Cały mechanizm inflacji kredytowej mieści się w trzech zdaniach i nie ma w nim
//! ani jednego parametru „inflacja":
//!
//! 1. Uruchomienie kredytu **tworzy** depozyt (`Books::create_credit`), więc podaż
//!    pieniądza rośnie o kwotę kapitału.
//! 2. Spłata kapitału **niszczy** pieniądz (`Books::destroy_credit`).
//! 3. Odsetki są zwykłym przelewem do banku i podaży **nie ruszają**.
//!
//! Reszta wychodzi sama: więcej pieniądza → większe koperty gospodarstw → więcej
//! zakupów powyżej progu → szybciej schodzący zapas → dodatni człon `adj_stock`
//! w `reprice` → wyższe ceny ofert → wyższe CPI (`crate::cpi`).
//!
//! **Kalendarz.** Miesiąc odsetkowy to zawsze 30 dób (`K-1`), więc stopa miesięczna
//! to `rate_bp_annual / 12` — bez wyjątków lutowych i bez konwencji ACT/365. Terminy
//! rat to `first_due + k · 30 dni`. Ta sama siatka obowiązuje okresy księgowe i okno
//! CPI: „30 dni" i „miesiąc" są w M5 jednym pojęciem, nigdy dwoma.

use magnat_core::{
    rng, CitizenReason, DecisionReason, HashState, LoanKind, Money, RejectCredit, StateHasher,
    StreamId, Tick,
};

use crate::books::{AccountOwner, LoanId};
use crate::data::{BankParams, LoanProduct};
use crate::kernel::{annuity_payment, monthly_interest, BP};

/// Ticków w miesiącu odsetkowym — 30 dób kalendarza 360-dniowego (`K-1`).
pub const TICKS_PER_MONTH: u64 = magnat_core::time::MINUTES_PER_MONTH;

/// Jedna rata harmonogramu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Installment {
    pub due: Tick,
    pub principal: Money,
    pub interest: Money,
}

/// Kredyt. `schedule` jest zbudowany raz przy uruchomieniu i się nie zmienia —
/// zaległość przesuwa `arrears_months`, a nie terminy.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Loan {
    pub id: LoanId,
    pub borrower: AccountOwner,
    pub lender: magnat_core::FirmId,
    pub kind: LoanKind,
    pub principal: Money,
    pub outstanding: Money,
    pub rate_bp_annual: i32,
    pub term_months: u16,
    pub paid_months: u16,
    /// Annuitet. `Σ principal == principal` **co do grosza** (własność P6).
    pub schedule: Vec<Installment>,
    pub arrears_months: u8,
}

impl Loan {
    /// Rata przypadająca na najbliższy niezapłacony miesiąc, albo `None` po spłacie.
    #[must_use]
    pub fn next_installment(&self) -> Option<Installment> {
        self.schedule.get(self.paid_months as usize).copied()
    }

    /// Miesięczna obsługa długu — to, czym kredyt obciąża budżet.
    #[must_use]
    pub fn monthly_service(&self) -> Money {
        self.next_installment().map_or(Money::ZERO, |i| {
            Money(i.principal.get().saturating_add(i.interest.get()))
        })
    }

    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.paid_months as usize >= self.schedule.len()
    }

    pub(crate) fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.id.0);
        h.write_u8(self.kind.as_index() as u8);
        self.principal.hash_state(h);
        self.outstanding.hash_state(h);
        h.write_u32(self.rate_bp_annual as u32);
        h.write_u16(self.term_months);
        h.write_u16(self.paid_months);
        h.write_u8(self.arrears_months);
    }
}

/// Stopa miesięczna z rocznej. Dzielenie całkowite jest **zamierzone**: reszta
/// z dzielenia przez 12 to mniej niż 1 bp miesięcznie, a ułamek stopy w stanie
/// trwałym byłby floatem w ścieżce pieniężnej (00 §2).
#[must_use]
pub fn monthly_rate_bp(rate_bp_annual: i32) -> i32 {
    rate_bp_annual / 12
}

/// Harmonogram annuitetowy. **Ostatnia rata pochłania resztę z zaokrągleń**, więc
/// suma kapitałów równa się kwocie kredytu co do grosza — to jest własność P6
/// i jedyny powód, dla którego ta pętla nie jest dzieleniem przez `n`.
#[must_use]
pub fn build_schedule(
    principal: Money,
    rate_bp_annual: i32,
    months: u16,
    first_due: Tick,
) -> Vec<Installment> {
    let n = months.max(1);
    let rate_m = monthly_rate_bp(rate_bp_annual);
    let rata = annuity_payment(principal, rate_m, n);
    let mut out = Vec::with_capacity(n as usize);
    let mut saldo = principal;
    for k in 0..n {
        let due = Tick(first_due.get() + u64::from(k) * TICKS_PER_MONTH);
        let odsetki = monthly_interest(saldo, rate_m);
        let kapital = if k + 1 == n {
            // Ostatnia rata zmiata resztę. Bez tej gałęzi `Σ principal` różni się
            // od kapitału o kilka groszy i P6 pęka po pierwszym tysiącu kredytów.
            saldo
        } else {
            let k_rata = Money(rata.get().saturating_sub(odsetki.get()));
            // Rata mniejsza od odsetek znaczyłaby ujemną amortyzację — przy naszych
            // widełkach nie wystąpi, ale saldo nie ma prawa rosnąć niezależnie od danych.
            Money(k_rata.get().clamp(0, saldo.get()))
        };
        saldo = Money(saldo.get() - kapital.get());
        out.push(Installment {
            due,
            principal: kapital,
            interest: odsetki,
        });
    }
    out
}

// ── ocena zdolności ──────────────────────────────────────────────────────────────

/// Wniosek kredytowy **rozwiązany na liczby**.
///
/// Encji tu nie ma z tego samego powodu co w `kernel`: rozwiązanie gospodarstwa
/// i zakładu na przepływy robi wołający, a ocena jest czystą funkcją. Plan §5.10
/// zapowiadał sygnaturę `assess_credit(&LoanApplication, &Books, &CreditHistory,
/// BaseRate)`; `Books` i historia wchodzą tu jako `existing_service`,
/// `arrears_months` i `ebitda_12m`, bo tylko te trzy liczby z nich czytamy.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LoanApplication {
    pub kind: LoanKind,
    /// O ile prosi wnioskodawca; bank może przyznać mniej.
    pub amount: Money,
    /// Gospodarstwo: dochód miesięczny netto. Zakład: zero.
    pub income_monthly: Money,
    /// Suma rat już obsługiwanych kredytów.
    pub existing_service: Money,
    /// Zakład: wynik operacyjny z ostatnich 12 miesięcy (przybliżenie EBITDA).
    pub ebitda_12m: Money,
    /// Zakład: obsługa długu w najbliższych 12 miesiącach, razem z nowym kredytem.
    pub debt_service_12m: Money,
    pub months_in_business: u16,
    /// Miesiące zaległości w ostatnich 24 — to jest cała `CreditHistory` w M5.
    pub arrears_months: u8,
    /// Klucz strumienia rozrzutu scoringu: indeks gospodarstwa albo zakładu.
    pub key: u32,
}

/// Rozstrzygnięcie wniosku. Oba warianty niosą `DecisionReason`, bo karta inspekcji
/// ma pokazać **czemu** — to jest wymóg 00 §7, nie ozdoba.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CreditDecision {
    Approved {
        limit: Money,
        rate_bp: i32,
        reason: DecisionReason,
    },
    Rejected {
        cause: RejectCredit,
        reason: DecisionReason,
    },
}

impl CreditDecision {
    #[must_use]
    pub fn reason(&self) -> DecisionReason {
        match self {
            CreditDecision::Approved { reason, .. } | CreditDecision::Rejected { reason, .. } => {
                *reason
            }
        }
    }
}

fn odmowa(kind: LoanKind, cause: RejectCredit, margin_bp: i32) -> CreditDecision {
    CreditDecision::Rejected {
        cause,
        reason: DecisionReason::Citizen(CitizenReason::CreditRejected {
            kind,
            cause,
            margin_bp: margin_bp.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
        }),
    }
}

/// Premia za ryzyko: liniowo od zera zaległości do progu blokady, plus rozrzut
/// ze strumienia `CreditScoringJitter`. Dwa identyczne wnioski u dwóch urzędników
/// nie kończą się identycznie — i to jest celowe, bo inaczej rynek kredytowy jest
/// progiem, a nie rozkładem.
fn risk_premium_bp(app: &LoanApplication, p: &BankParams, seed: u64, t: Tick) -> i32 {
    let prog = i32::from(p.scoring.arrears_block_months.max(1));
    let zakres = p.scoring.risk_premium_bp;
    let udzial = i32::from(app.arrears_months).min(prog);
    let baza = zakres.min + (zakres.max - zakres.min) * udzial / prog;
    if p.scoring.jitter_bp <= 0 {
        return baza;
    }
    let mut r = rng(seed, StreamId::CreditScoringJitter, app.key, t);
    let szerokosc = u32::try_from(p.scoring.jitter_bp * 2 + 1).unwrap_or(1);
    baza + r.gen_range_u32(szerokosc) as i32 - p.scoring.jitter_bp
}

/// Ocena zdolności (§6.5: „historia, zabezpieczenie, przepływy").
///
/// Kolejność sprawdzeń jest kolejnością **wyjaśnienia**: najpierw to, co przekreśla
/// wniosek bez liczenia (brak dochodu, zaległości), potem właściwa miara obciążenia.
/// Odwrotna kolejność dałaby graczowi „raty ponad limit" tam, gdzie prawdziwym
/// powodem jest brak dochodu.
#[must_use]
pub fn assess_credit(
    app: &LoanApplication,
    p: &BankParams,
    base: BaseRate,
    seed: u64,
    t: Tick,
) -> CreditDecision {
    let product: LoanProduct = p.products.get(app.kind);

    if app.arrears_months >= p.scoring.arrears_block_months {
        return odmowa(
            app.kind,
            RejectCredit::Arrears,
            i32::from(app.arrears_months),
        );
    }

    let (limit, load_bp) = match app.kind {
        LoanKind::Consumer => {
            if app.income_monthly.get() <= 0 {
                return odmowa(app.kind, RejectCredit::NoIncome, 0);
            }
            let sufit = app
                .income_monthly
                .mul_ratio(product.max_multiple_bp, BP)
                .get();
            let limit = Money(app.amount.get().min(sufit));
            if limit.get() <= 0 {
                return odmowa(app.kind, RejectCredit::NoIncome, 0);
            }
            let rata = annuity_payment(
                limit,
                monthly_rate_bp(base.bp + product.spread_bp),
                product.term_months,
            );
            let obciazenie = app.existing_service.get().saturating_add(rata.get());
            let dsti_bp = i32::try_from(obciazenie * BP / app.income_monthly.get().max(1))
                .unwrap_or(i32::MAX);
            if dsti_bp > p.scoring.dsti_limit_bp {
                return odmowa(
                    app.kind,
                    RejectCredit::DstiTooHigh,
                    dsti_bp - p.scoring.dsti_limit_bp,
                );
            }
            (limit, dsti_bp)
        }
        // Obrotowy i inwestycyjny idą **tą samą ścieżką**: obie są kredytem firmy
        // i obie ocenia się pokryciem obsługi długu przepływami. Różnica siedzi
        // w produkcie (`spread_bp`, `term_months`), a nie w regule — osobna gałąź
        // powtarzałaby ten sam kod, żeby na końcu zwrócić tę samą parę liczb.
        LoanKind::WorkingCapital | LoanKind::Investment => {
            if app.months_in_business < p.scoring.min_months_in_business {
                return odmowa(
                    app.kind,
                    RejectCredit::DscrTooLow,
                    -i32::from(p.scoring.min_months_in_business - app.months_in_business),
                );
            }
            if app.debt_service_12m.get() <= 0 {
                return odmowa(app.kind, RejectCredit::DscrTooLow, 0);
            }
            let dscr_bp =
                i32::try_from(app.ebitda_12m.get() * BP / app.debt_service_12m.get().max(1))
                    .unwrap_or(i32::MAX);
            if dscr_bp < p.scoring.dscr_min_bp {
                return odmowa(
                    app.kind,
                    RejectCredit::DscrTooLow,
                    dscr_bp - p.scoring.dscr_min_bp,
                );
            }
            (app.amount, dscr_bp)
        }
    };

    let rate_bp = base.bp + risk_premium_bp(app, p, seed, t) + product.spread_bp;
    CreditDecision::Approved {
        limit,
        rate_bp,
        reason: DecisionReason::Citizen(CitizenReason::CreditApproved {
            kind: app.kind,
            rate_bp: rate_bp.clamp(0, i32::from(i16::MAX)) as i16,
            load_bp: load_bp.clamp(0, i32::from(i16::MAX)) as i16,
        }),
    }
}

// ── stopa bazowa ─────────────────────────────────────────────────────────────────

/// Stopa bazowa banku centralnego. Nie jest firmą i nie ma konta — ustawia jedną
/// liczbę raz na miesiąc i tyle go w mieście widać.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BaseRate {
    pub bp: i32,
    pub set_at: Tick,
}

impl BaseRate {
    #[must_use]
    pub fn start(p: &BankParams) -> BaseRate {
        BaseRate {
            bp: p.base_rate.start_bp,
            set_at: Tick(0),
        }
    }

    pub(crate) fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.bp as u32);
        h.write_u64(self.set_at.get());
    }
}

/// Reguła typu Taylora w arytmetyce całkowitej:
/// `base = clamp(neutral + a · (cpi_yoy − target) / 10 000, floor, ceil)`,
/// ze zmianą ograniczoną do ±`max_step_bp` na miesiąc.
///
/// Reakcja jest **celowo powolna**. Bank centralny ma stabilizować, nie sterować;
/// sterowanie zabiłoby emergencję, którą balansator ma mierzyć (M5e).
#[must_use]
pub fn update_base_rate(cur: BaseRate, cpi_yoy_bp: i32, p: &BankParams, t: Tick) -> BaseRate {
    let r = &p.base_rate;
    let cel =
        i64::from(r.neutral_bp) + i64::from(r.a_bp) * i64::from(cpi_yoy_bp - r.target_bp) / BP;
    let cel = cel.clamp(i64::from(r.floor_bp), i64::from(r.ceil_bp)) as i32;
    let krok = (cel - cur.bp).clamp(-r.max_step_bp, r.max_step_bp);
    BaseRate {
        bp: (cur.bp + krok).clamp(r.floor_bp, r.ceil_bp),
        set_at: t,
    }
}

// ── rejestr kredytów ─────────────────────────────────────────────────────────────

/// Wszystkie kredyty miasta. `LoanId` to indeks w tym wektorze i **nigdy nie jest
/// zwalniany**: spłacony kredyt zostaje w rejestrze, bo historia kredytowa
/// i kronika M9 pytają o kredyty, których już nie ma.
#[derive(Default, Clone, PartialEq, Eq, Debug)]
pub struct LoanBook {
    loans: Vec<Loan>,
}

impl LoanBook {
    #[must_use]
    pub fn new() -> LoanBook {
        LoanBook::default()
    }

    /// Wpisuje kredyt i nadaje mu identyfikator.
    ///
    /// Osiem argumentów zamiast struktury wejściowej z rozmysłu: siedem z nich to
    /// **wynik decyzji kredytowej**, a nie konfiguracja, i każde ma tu jedyne
    /// wywołanie. Struktura `LoanTerms` byłaby typem z jednym producentem
    /// i jednym konsumentem — czyli nazwą dla listy argumentów.
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        &mut self,
        borrower: AccountOwner,
        lender: magnat_core::FirmId,
        kind: LoanKind,
        principal: Money,
        rate_bp_annual: i32,
        term_months: u16,
        first_due: Tick,
    ) -> LoanId {
        let id = LoanId(u32::try_from(self.loans.len()).unwrap_or(u32::MAX));
        let schedule = build_schedule(principal, rate_bp_annual, term_months, first_due);
        self.loans.push(Loan {
            id,
            borrower,
            lender,
            kind,
            principal,
            outstanding: principal,
            rate_bp_annual,
            term_months,
            paid_months: 0,
            schedule,
            arrears_months: 0,
        });
        id
    }

    #[must_use]
    pub fn get(&self, id: LoanId) -> Option<&Loan> {
        self.loans.get(id.0 as usize)
    }

    pub fn get_mut(&mut self, id: LoanId) -> Option<&mut Loan> {
        self.loans.get_mut(id.0 as usize)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.loans.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.loans.is_empty()
    }

    /// Wszystkie kredyty w kolejności nadania. Potrzebne postępowaniu upadłościowemu
    /// (M7d): niespłacony kapitał kredytu jest roszczeniem banku, choć nie jest
    /// jeszcze zaległością — rata przyszłego miesiąca nigdy nie zapadnie, bo firmy
    /// wtedy nie będzie.
    pub fn iter(&self) -> impl Iterator<Item = &Loan> {
        self.loans.iter()
    }

    /// Suma niespłaconego kapitału — tyle pieniądza kredytowego krąży po mieście.
    #[must_use]
    pub fn outstanding_total(&self) -> Money {
        Money(self.loans.iter().map(|l| l.outstanding.get()).sum())
    }

    pub(crate) fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.loans.len() as u64);
        for l in &self.loans {
            l.hash_state(h);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> BankParams {
        crate::data::EconomyData::load_default()
            .expect("data/economy/")
            .bank
    }

    #[test]
    fn harmonogram_sumuje_sie_do_kapitalu_co_do_grosza() {
        // P6 z §7.1. Dziesięć tysięcy losowych trójek (kwota, stopa, okres) —
        // gałąź „zmiatania reszty" w ostatniej racie jest tym, co ten test chroni.
        let mut r = magnat_core::Rng::from_state([0x9E37_79B9_7F4A_7C15, 3, 5, 7]);
        for _ in 0..10_000 {
            let kwota = Money(i64::from(r.gen_range_u32(50_000_000)) + 1);
            let stopa = r.gen_range_u32(4_000) as i32;
            let miesiecy = (r.gen_range_u32(360) + 1) as u16;
            let h = build_schedule(kwota, stopa, miesiecy, Tick(0));
            let suma: i64 = h.iter().map(|i| i.principal.get()).sum();
            assert_eq!(
                suma,
                kwota.get(),
                "kwota {kwota:?}, stopa {stopa}, {miesiecy} mies."
            );
            assert_eq!(h.len(), miesiecy as usize);
            assert!(h.iter().all(|i| i.principal.get() >= 0));
        }
    }

    #[test]
    fn terminy_rat_ida_co_trzydziesci_dob() {
        let h = build_schedule(Money(1_200_000), 1_200, 3, Tick(TICKS_PER_MONTH));
        let d: Vec<u64> = h.iter().map(|i| i.due.get()).collect();
        assert_eq!(
            d,
            vec![TICKS_PER_MONTH, 2 * TICKS_PER_MONTH, 3 * TICKS_PER_MONTH]
        );
    }

    #[test]
    fn odsetki_maleja_razem_z_saldem() {
        let h = build_schedule(Money(2_400_000), 2_400, 12, Tick(0));
        assert!(h[0].interest.get() > h[11].interest.get());
        // Annuitet: rata stała, więc kapitał rośnie dokładnie o tyle, o ile spadają odsetki.
        let r0 = h[0].principal.get() + h[0].interest.get();
        let r5 = h[5].principal.get() + h[5].interest.get();
        assert!((r0 - r5).abs() <= 2, "rata skacze: {r0} vs {r5}");
    }

    #[test]
    fn brak_dochodu_i_zaleglosci_odmawiaja_przed_liczeniem_dsti() {
        let p = params();
        let base = BaseRate::start(&p);
        let mut app = LoanApplication {
            kind: LoanKind::Consumer,
            amount: Money(500_000),
            income_monthly: Money::ZERO,
            existing_service: Money::ZERO,
            ebitda_12m: Money::ZERO,
            debt_service_12m: Money::ZERO,
            months_in_business: 0,
            arrears_months: 0,
            key: 1,
        };
        match assess_credit(&app, &p, base, 7, Tick(0)) {
            CreditDecision::Rejected { cause, .. } => assert_eq!(cause, RejectCredit::NoIncome),
            d => panic!("{d:?}"),
        }
        app.income_monthly = Money(400_000);
        app.arrears_months = 3;
        match assess_credit(&app, &p, base, 7, Tick(0)) {
            CreditDecision::Rejected { cause, .. } => assert_eq!(cause, RejectCredit::Arrears),
            d => panic!("{d:?}"),
        }
    }

    #[test]
    fn zbyt_wysokie_dsti_konczy_sie_odmowa_z_miara_braku() {
        let p = params();
        let base = BaseRate::start(&p);
        let app = LoanApplication {
            kind: LoanKind::Consumer,
            amount: Money(1_200_000),
            income_monthly: Money(300_000),
            // Raty już obsługiwane zjadają cały limit.
            existing_service: Money(130_000),
            ebitda_12m: Money::ZERO,
            debt_service_12m: Money::ZERO,
            months_in_business: 0,
            arrears_months: 0,
            key: 2,
        };
        match assess_credit(&app, &p, base, 7, Tick(0)) {
            CreditDecision::Rejected { cause, reason } => {
                assert_eq!(cause, RejectCredit::DstiTooHigh);
                match reason {
                    DecisionReason::Citizen(CitizenReason::CreditRejected {
                        margin_bp, ..
                    }) => assert!(margin_bp > 0),
                    r => panic!("{r:?}"),
                }
            }
            d => panic!("{d:?}"),
        }
    }

    #[test]
    fn stopa_bazowa_rusza_sie_powoli_i_w_strone_inflacji() {
        let p = params();
        let mut r = BaseRate::start(&p);
        let start = r.bp;
        // CPI r/r 10 % wobec celu 2,5 % — reguła chce w górę, ale krokiem.
        r = update_base_rate(r, 1_000, &p, Tick(1));
        assert!(r.bp > start);
        assert!(r.bp - start <= p.base_rate.max_step_bp);
        // Krok jest ograniczony do `max_step_bp`, więc dojście do celu reguły
        // zajmuje ponad dwadzieścia miesięcy — i o to w tej powolności chodzi.
        for k in 2..40 {
            r = update_base_rate(r, 1_000, &p, Tick(k));
        }
        let cel =
            p.base_rate.neutral_bp + p.base_rate.a_bp * (1_000 - p.base_rate.target_bp) / 10_000;
        assert_eq!(r.bp, cel.clamp(p.base_rate.floor_bp, p.base_rate.ceil_bp));
        // Deflacja zawraca; podłoga trzyma.
        for k in 14..120 {
            r = update_base_rate(r, -500, &p, Tick(k));
        }
        assert_eq!(r.bp, p.base_rate.floor_bp);
    }
}
