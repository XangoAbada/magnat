//! Wykonanie polityki zdelegowanego zakładu (M7c WP7, M9d WP9).
//!
//! # Dlaczego wykonawca stoi tutaj, a nie w `sim/policy`
//!
//! Crate reguł nie wie, czym jest sklep, i wiedzieć nie może: zależność idzie
//! `economy → firms → policy` i odwrócenie zamknęłoby cykl, którego Cargo nie zbuduje.
//! `sim/policy` mówi **co zrobić**; kto ma półkę i zaplecze, ten to robi — tak samo
//! jak rynek pracy stoi w `sim/economy`, choć jego autorem jest M7 (`D2`).
//!
//! Z tego samego powodu stoi tu **jakość wykonania menedżera** ([`crate::ManagerExecution`]),
//! choć M9 §6 wpisywał ją do `game::policy`: politykę wykonuje system ECS, a `game/`
//! jest nad symulacją, nie w niej. W `game/` zostaje warstwa gracza — edytor, dry-run
//! i diagnostyka.
//!
//! # Dwie strony jednego kroku
//!
//! **Odczyt** ([`facts`]) buduje się z tego, co firma o sobie wie. **Zapis** idzie przez
//! mechanizmy, które już istnieją, a nie obok nich: `SetPrice` przestawia sterownik na
//! [`crate::PricePolicy::Fixed`], `SetMargin` na `Markup`, a zamówienie rusza
//! `ReorderPolicy`. Druga droga do ceny rozjechałaby się z pierwszą przy pierwszej zmianie.
//!
//! ponytail: sufit nazwany — `mod.rs` ma ~360 linii **własnego** kodu i zostaje przy
//! nich. Jest w nim jedna rzecz: doba polityk i to, co z niej wynika. Odczyty
//! przeprowadziły się do `market/readout.rs`, dry-run do `dry.rs`, fakty do
//! `facts.rs`, rozstrzyganie do `decide.rs` — dalszy podział rozciąłby pętlę.
//!
//! # Ślad doby
//!
//! Zakład **śledzony** zapisuje co dobę fakty, na których polityka pracowała
//! ([`facts::PolicyTrace`]). To jest cały materiał dry-runu: „co by ustawiła ta reguła
//! przez ostatnie 30 dni" liczy się z zapisanej przeszłości, a nie z drugiego modelu.

mod decide;
mod dry;
pub mod facts;

use magnat_core::{DecisionReason, GoodId, Money, PolicyId, SimCalendar, SiteId, Tick};
use magnat_firms::Firms;
use magnat_policy::{Cadence, PolicyDomain, Severity};

use crate::manager_exec::{ManagerCurve, ManagerExecution};
use crate::market::{Market, MarketInner};

pub use decide::PolicyOutcome;
pub use dry::{DryDay, DryRun};
pub use facts::{GoodFacts, PolicyTrace, TraceDay, MAX_COVER, TRACE_DAYS};

use decide::{decyduj, z_powodu, zastosuj, RunCtx, Wynik};

/// Ile wpisów trzyma skrzynka eskalacji. Sześćdziesiąt cztery, bo to jest lista
/// do przeczytania przez człowieka, a nie dziennik — starszy wpis ustępuje nowszemu.
pub const POLICY_INBOX: usize = 64;

/// Ile dziennik przecen zakładu trzyma wpisów polityki. Ten sam limit co przy przecenie.
const REPRICE_LOG: usize = 64;

/// Podsumowanie doby polityk — metryki balansatora i panelu.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct PolicyDay {
    /// Ile zakładów w ogóle miało dziś politykę do wykonania.
    pub sites: u32,
    /// Ile wykonań wypadło w martwej strefie (`cooldown_h`) i zostało pominiętych.
    pub on_cooldown: u32,
    /// Ile cykli menedżer przepuścił — rzut `StreamId::PolicyExecution`, nie martwa strefa.
    pub skipped: u32,
    /// Ile akcji wykonano.
    pub applied: u32,
    /// Ile akcji wróciło bez skutku, bo metryki nie dało się odczytać.
    pub blind: u32,
    /// Ile alertów i pytań do gracza polityka wystawiła. Nie zmieniają świata.
    pub alerts: u32,
}

/// Wpis skrzynki eskalacji: polityka zgłasza właścicielowi coś, czego sama nie rozstrzyga.
///
/// „Automatyzacja, której nie da się przerwać, jest gorsza niż jej brak" (M9d §5.6) —
/// `ask` odróżnia „wiedz o tym" od „zdecyduj", a przy `ask` polityka **zatrzymuje się**
/// na tym towarze do najbliższego wykonania.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PolicyAlert {
    pub tick: Tick,
    pub site: SiteId,
    pub policy: PolicyId,
    pub rule: u8,
    /// Indeks komunikatu polityki. Odwzorowanie na klucz lokalizacji należy do UI
    /// (`AX-2`): `sim/policy` nie zna `LocKey` i nie będzie znał.
    pub msg: u16,
    pub severity: Severity,
    pub ask: bool,
}

impl Market {
    /// Doba polityk zdelegowanych zakładów.
    ///
    /// Wołana **przed** [`Market::reprice_all`] i to jest kontrakt kolejności: polityka
    /// ustawia sterownik ceny, a przecena go wykonuje razem z ogranicznikiem marży.
    /// Odwrotnie polityka nadpisywałaby świeżo policzoną cenę i sklep pracowałby
    /// na wyniku sprzed doby.
    ///
    /// `curve` to kalibracja menedżera z `data/tuning/policy.ron`. `None` znaczy świat
    /// **bez modelu menedżera** — polityka wykonuje się wtedy dokładnie i natychmiast.
    /// Tak stoją scenariusze, które stawiają wycinek gospodarki bez gry; klient i przebieg
    /// pełny ładują plik w `game::world` i błąd pliku zatrzymuje start, a nie wykonanie.
    pub fn run_policies(
        &self,
        firms: &mut Firms,
        cash: &std::collections::BTreeMap<SiteId, Money>,
        curve: Option<&ManagerCurve>,
        t: Tick,
    ) -> PolicyDay {
        let mut d = PolicyDay::default();
        let cal = SimCalendar::new(t);
        let mut m = self.lock();
        let seed = m.seed;
        let chain = m.chain.clone();
        let ch = chain.lock();
        // Silnik podatkowy wyjęty przed pętlę: `m.shops` jest w niej pożyczane
        // mutowalnie, a sprowadzenie ceny brutto do netto musi iść **tym samym**
        // silnikiem, którym liczy przecena (`K-7`).
        let tax = std::mem::replace(&mut m.tax, Box::new(crate::tax::NoTax));
        // Stawka VAT jest dziś jedna dla wszystkich towarów (`NoTax` z M5, właścicielem
        // podatków jest M8), więc czyta się ją raz. Kiedy M8 da stawki per towar,
        // zmienia się to w odczyt per towar w `zbierz_fakty`, a nie kształt widoku.
        let vat = {
            let brutto = tax.gross_from_net(GoodId(0), Money(10_000));
            i32::try_from(brutto.get() - 10_000).unwrap_or(0)
        };

        // Zakłady zdelegowane, w kolejności `BTreeMap` rejestru firm — nigdy
        // w kolejności sklepów w wektorze rynku (00 §3.2).
        let zlecenia: Vec<(SiteId, usize)> = firms
            .sites()
            .filter(|(_, s)| s.delegation.is_some())
            .filter_map(|(id, _)| m.by_site.get(&id).map(|i| (id, *i as usize)))
            .collect();

        for (site, i) in zlecenia {
            let Some(zaklad) = firms.site(site) else {
                continue;
            };
            let Some(deleg) = zaklad.delegation.as_ref() else {
                continue;
            };
            // Kadencja polityki (M9d §5.6). Polityka dobowa wykonuje się na granicy
            // doby, godzinowa — co godzinę; martwa strefa `cooldown_h` dopiero nad tym
            // stoi. Bez tego rozróżnienia `Cadence::Hourly` byłaby polem bez czytelnika,
            // a każde `cooldown_h` poniżej 24 h nie znaczyłoby nic, bo `ready` nikt by
            // nie pytał częściej niż raz na dobę.
            if deleg.policy.cadence == Cadence::Daily && !cal.is_day_boundary() {
                continue;
            }

            // Menedżer: jego umiejętność albo podłoga „bez nadzoru". Zakład, którym
            // nikt nie zarządza, prowadzi politykę gorzej — to jest koszt, a nie bramka.
            let skill_mgr = firms.manager_of_site(site).map(|mgr| mgr.skill_mgmt);
            let exec = match curve {
                None => ManagerExecution::flawless(),
                Some(c) => ManagerExecution::from_skill(
                    skill_mgr.unwrap_or(magnat_core::Q::new(c.unsupervised_skill)),
                    c,
                ),
            };
            let klucz = deleg
                .manager
                .map_or_else(|| site.entity().index(), |c| c.entity().index());

            d.sites += 1;
            // Zwłoka reakcji wydłuża martwą strefę polityki.
            //
            // ponytail: sufit nazwany — słaby menedżer reaguje **rzadziej**, a nie
            // „z kolejki odroczonych akcji". §5.6 opisuje to drugie; kolejka kosztuje
            // bufor akcji na każdy zdelegowany zakład, a różnicy dla gracza nie ma,
            // dopóki panel nie pokazuje „menedżer zauważy jutro". Wtedy — i dopiero
            // wtedy — kolejka ma czytelnika.
            if !deleg.ready_after(t, exec.reaction_delay_h) {
                d.on_cooldown += 1;
                continue;
            }
            if exec.skips(seed, klucz, t) {
                d.skipped += 1;
                continue;
            }
            if !deleg.autonomy.covers(deleg.policy.domain) {
                // Menedżer nie ma tu prawa głosu. Nie jest to błąd polityki, tylko
                // zakres pełnomocnictwa — właściciel oddał ceny, a nie zapas.
                continue;
            }
            let polityka = deleg.policy.clone();
            let mgmt = zaklad.mgmt;
            let wakaty = i32::try_from(zaklad.vacancies()).unwrap_or(i32::MAX);
            let saldo = cash.get(&site).copied();

            // Opóźnienie informacji **nie dostaje własnego licznika**: obraz konkurencji
            // z opóźnieniem 1..=7 dób istnieje od M5c i to on jest tą liczbą. Menedżer
            // ustawia w nim zakład — drugi wiek obok pierwszego rozjechałby się przy
            // pierwszej zmianie.
            m.shops[i].observed.delay_days = exec.info_lag_days;
            let lag = i32::try_from(
                t.get().saturating_sub(m.shops[i].observed.refreshed.get())
                    / magnat_core::time::MINUTES_PER_DAY,
            )
            .unwrap_or(7);

            let mut fakty = facts::zbierz_fakty(&m, &ch, &*tax, i, t);
            let ctx = RunCtx {
                cal,
                vat_bp: vat,
                cash: saldo,
                manager_skill: skill_mgr.map(|q| i32::from(q.get())),
                staff_mood: Some(i32::from(mgmt.0)),
                open_positions: wakaty,
                exec: ManagerExecution {
                    info_lag_days: u8::try_from(lag.clamp(0, 7)).unwrap_or(7),
                    ..exec
                },
                error_bp: exec.error_bp(seed, klucz, t),
            };
            // Promienie, o które pyta ta polityka — obserwacja następnej doby zawęzi
            // do nich obraz konkurencji (`R2-WP22`). Zapisywane przy każdym przebiegu,
            // więc odpięcie reguły zeruje je w tej samej dobie.
            m.shops[i].asked_radii = promienie_polityki(&polityka);

            let decyzje = decyduj(&polityka, &mut fakty, &ctx, &*tax);

            m.zapisz_decyzje(i, site, firms, &decyzje, t, &mut d);
            if let Some(s) = firms.site_mut(site) {
                if let Some(dd) = s.delegation.as_mut() {
                    dd.last_run = t;
                }
            }
        }
        m.tax = tax;
        d
    }

    /// Zapisuje dobowy ślad zakładów śledzonych — materiał dry-runu (M9d WP8).
    ///
    /// Wołane **po** [`Market::observe_competitors`] i **przed** [`Market::run_policies`],
    /// czyli dokładnie tam, gdzie polityka zobaczy te same liczby. Zapis w innym miejscu
    /// doby dawałby dry-run, który mówi o świecie sprzed albo po wykonaniu — i wtedy
    /// zgodność z §7 („tolerancja 0") byłaby przypadkiem.
    pub fn record_policy_trace(&self, t: Tick) {
        let mut m = self.lock();
        let chain = m.chain.clone();
        let ch = chain.lock();
        let sledzone: Vec<usize> = (0..m.shops.len())
            .filter(|i| m.shops[*i].tracking != crate::shop::LostSaleTracking::None)
            .collect();
        // Silnik wyjmuje się tak samo jak w `run_policies` i **z tego samego powodu**:
        // `m.shops` jest niżej pożyczane mutowalnie. Gdyby ślad liczył cenę netto innym
        // silnikiem niż wykonanie, „dry-run zgodny co do grosza" (§7) przestałby być
        // prawdą przy pierwszym VAT-cie i nie złapałby tego żaden test.
        let tax = std::mem::replace(&mut m.tax, Box::new(crate::tax::NoTax));
        for i in &sledzone {
            let facts = facts::zbierz_fakty(&m, &ch, &*tax, *i, t);
            m.shops[*i].trace.push(TraceDay { day: t, facts });
        }
        m.tax = tax;
    }

    /// Skrzynka eskalacji zakładów śledzonych. Kopia, nie referencja — czyta ją panel
    /// spoza zamka rynku.
    #[must_use]
    pub fn policy_inbox(&self) -> Vec<PolicyAlert> {
        self.lock().inbox.clone()
    }

    /// Ślad doby zakładu. Pusty dla zakładu nieśledzonego.
    #[must_use]
    pub fn policy_trace(&self, site: SiteId) -> PolicyTrace {
        let m = self.lock();
        m.by_site
            .get(&site)
            .map_or_else(PolicyTrace::default, |i| m.shops[*i as usize].trace.clone())
    }
}

impl MarketInner {
    /// Zapisuje decyzje doby: skutek do sklepu, powód do dziennika, eskalację
    /// do skrzynki.
    ///
    /// Osobno od pętli po zakładach, bo to jest **inny krok**: tam rozstrzyga się,
    /// co polityka chce zrobić, tutaj — co z tego wychodzi. Rozdzielenie jest też
    /// odpowiedzią na próg strukturalny (CLAUDE.md).
    fn zapisz_decyzje(
        &mut self,
        shop: usize,
        site: SiteId,
        firms: &mut Firms,
        decyzje: &[(PolicyOutcome, DecisionReason)],
        t: Tick,
        d: &mut PolicyDay,
    ) {
        for (skutek, powod) in decyzje {
            // Wycofanie z półki dotyka areny ofert i indeksu przestrzennego, a nie
            // samego sklepu — więc wykonuje się tutaj, a nie w `zastosuj`, który widzi
            // sam `Shop`. Towar, którego na półce nie było, daje akcję **ślepą**:
            // „zdjąłem coś, czego nie miałem" byłoby dla gracza nieprawdą.
            let wynik = match *skutek {
                PolicyOutcome::Withdraw(g) => {
                    if self.zdejmij_linie(shop, g) {
                        Wynik::Zrobione
                    } else {
                        Wynik::Slepa
                    }
                }
                inny => zastosuj(&mut self.shops[shop], inny, t),
            };
            match wynik {
                Wynik::Zrobione => d.applied += 1,
                Wynik::Alert => d.alerts += 1,
                Wynik::Slepa => d.blind += 1,
            }
            if let PolicyOutcome::Alert { msg, severity, ask } = *skutek {
                let (policy, rule) = z_powodu(*powod);
                self.push_policy_alert(
                    shop,
                    PolicyAlert {
                        tick: t,
                        site,
                        policy,
                        rule,
                        msg,
                        severity,
                        ask,
                    },
                );
            }
            self.log_policy(shop, *powod);
            zapisz(firms, site, t, *powod);
        }
    }

    /// Dopisuje wpis do skrzynki, ale **tylko dla zakładu śledzonego**.
    ///
    /// Eskalacja jest wiadomością do gracza, więc zakład, o który gracz nie pytał
    /// i którego nie ma, nie ma komu jej wysłać. Ten sam próg co przy pierścieniu
    /// utraconych sprzedaży i z tego samego powodu — a przy okazji zero kosztu
    /// dla tysięcy zakładów AI.
    fn push_policy_alert(&mut self, shop: usize, a: PolicyAlert) {
        if self.shops[shop].tracking == crate::shop::LostSaleTracking::None {
            return;
        }
        if self.inbox.len() >= POLICY_INBOX {
            self.inbox.remove(0);
        }
        self.inbox.push(a);
    }

    /// Dopisuje powód do dziennika przecen zakładu — to z niego karta inspekcji
    /// buduje zakładkę „Dlaczego" (`DG-6`).
    fn log_policy(&mut self, shop: usize, powod: DecisionReason) {
        if self.shops[shop].tracking == crate::shop::LostSaleTracking::None {
            return;
        }
        let log = &mut self.shops[shop].reprice_log;
        if log.len() >= REPRICE_LOG {
            log.remove(0);
        }
        log.push(powod);
    }
}

/// Zapis powodu do dziennika firmy (00 §7). Osobna funkcja, bo woła się ją
/// po każdej akcji i **nie ma drogi obok**: `Decided` nie daje akcji bez powodu.
fn zapisz(firms: &mut Firms, site: SiteId, t: Tick, powod: DecisionReason) {
    if let Some(firma) = firms.site(site).map(|s| s.firm) {
        firms.log(firma, t, powod);
    }
}

/// Polityka o tej dziedzinie, którą most stawiający miasto przypina zakładowi.
///
/// Skrót dla wołających spoza tego modułu (testy, scenariusze, M7f): preset z katalogu
/// plus numer. Numer jest **pozycją w katalogu**, więc nie zależy od kolejności
/// przypinania — dwa światy z tego samego ziarna nadadzą te same numery.
#[must_use]
pub fn preset_for(
    catalog: &magnat_policy::PolicyCatalog,
    key: &str,
    domain: PolicyDomain,
) -> Option<magnat_policy::FirmPolicy> {
    let numer = catalog.iter().position(|p| p.key == key)?;
    let p = catalog.instantiate(key, PolicyId(u16::try_from(numer).unwrap_or(u16::MAX)))?;
    (p.domain == domain).then_some(p)
}

/// Promienie metryk konkurencyjnych tej polityki, rosnąco i bez powtórzeń.
///
/// Walidator języka dopuszcza najwyżej [`magnat_policy::MAX_COMPETITIVE`] takich metryk
/// w jednej polityce, więc lista jest z definicji krótka i mieści się w tablicy
/// [`crate::pricing::MAX_NEAR_RADII`]. Rosnąco i bez powtórzeń, żeby dwie reguły o tym
/// samym promieniu nie zajmowały dwóch slotów, a kolejność nie zależała od tego,
/// w którym miejscu polityki gracz dopisał regułę.
fn promienie_polityki(p: &magnat_policy::FirmPolicy) -> [u32; crate::pricing::MAX_NEAR_RADII] {
    let mut out = [0u32; crate::pricing::MAX_NEAR_RADII];
    let mut zebrane: Vec<u32> = Vec::new();
    let mut zbierz = |e: &magnat_policy::Expr| {
        zbierz_promienie(e, &mut zebrane);
    };
    for r in &p.rules {
        zbierz_z_warunku(&r.when, &mut zbierz);
    }
    zebrane.sort_unstable();
    zebrane.dedup();
    for (slot, r) in zebrane.into_iter().take(out.len()).enumerate() {
        out[slot] = r;
    }
    out
}

/// Obchodzi warunek i podaje każde wyrażenie do domknięcia.
fn zbierz_z_warunku(c: &magnat_policy::ConditionExpr, f: &mut impl FnMut(&magnat_policy::Expr)) {
    use magnat_policy::ConditionExpr as C;
    match c {
        C::Always => {}
        C::Cmp { lhs, rhs, .. } => {
            f(lhs);
            f(rhs);
        }
        C::And(a, b) | C::Or(a, b) => {
            zbierz_z_warunku(a, f);
            zbierz_z_warunku(b, f);
        }
        C::Not(x) => zbierz_z_warunku(x, f),
    }
}

/// Promienie metryk konkurencyjnych w jednym wyrażeniu.
fn zbierz_promienie(e: &magnat_policy::Expr, out: &mut Vec<u32>) {
    use magnat_policy::{Expr, Metric};
    match e {
        Expr::Metric(m) => match m {
            Metric::CheapestCompetitorPrice { radius_m, .. }
            | Metric::AvgCompetitorPrice { radius_m, .. }
            | Metric::CompetitorCount { radius_m } => out.push(*radius_m),
            _ => {}
        },
        Expr::Lit(_) => {}
        Expr::Bin { lhs, rhs, .. } => {
            zbierz_promienie(lhs, out);
            zbierz_promienie(rhs, out);
        }
        Expr::Convert { of, .. } => zbierz_promienie(of, out),
    }
}
