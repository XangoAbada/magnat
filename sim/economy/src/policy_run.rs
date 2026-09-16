//! Wykonanie polityki zdelegowanego zakładu (M7c WP7).
//!
//! # Dlaczego wykonawca stoi tutaj, a nie w `sim/policy`
//!
//! Crate reguł nie wie, czym jest sklep, i wiedzieć nie może: zależność idzie
//! `economy → firms → policy` i odwrócenie zamknęłoby cykl, którego Cargo nie zbuduje.
//! `sim/policy` mówi **co zrobić**; kto ma półkę i zaplecze, ten to robi — tak samo
//! jak rynek pracy stoi w `sim/economy`, choć jego autorem jest M7 (`D2`).
//!
//! # Dwie strony jednego kroku
//!
//! **Odczyt** ([`ShopView`]) buduje się z tego, co firma o sobie wie: własnej półki,
//! własnego kosztu i **obrazu konkurencji z opóźnieniem 1–7 dni** (`CompetitorSnapshot`,
//! M5c §6.3). To jest granica asymetrii informacji z M7 §5.8: reguła widzi ceny
//! półkowe sąsiadów, bo te są publiczne, i nie widzi ich kosztów, bo widok ich nie ma.
//!
//! **Zapis** idzie przez mechanizmy, które już istnieją, a nie obok nich: `SetPrice`
//! przestawia sterownik na [`PricePolicy::Fixed`], `SetMargin` na `Markup`, a zamówienie
//! rusza `ReorderPolicy`. Druga droga do ceny rozjechałaby się z pierwszą przy pierwszej
//! zmianie — to ta sama zasada, dla której `set_price` z M5b pilnuje, żeby oferta
//! i sterownik nie rozeszły się nawet na jedną dobę.
//!
//! # Czego tu nie ma
//!
//! **Jakości wykonania menedżera** (`ManagerExecution` — opóźnienie reakcji, błąd
//! wykonania, pominięty cykl). M9d przypisuje ją wprost do WP9 fazy M9 i tam zostaje;
//! M7c dostarcza jakość zarządzania, która wchodzi w produktywność, rotację i straty.
//! Dopisanie jej tutaj znaczyłoby, że M9 najpierw musi ją stąd usunąć.

use magnat_core::{
    DecisionReason, GoodId, Money, PolicyId, PriceBasis, Qty, SimCalendar, SiteId, Tick,
};
use magnat_firms::Firms;
use magnat_policy::{
    evaluate, Action, Bp, Cadence, Expr, GoodRef, Metric, MetricCtx, PolicyDomain, PolicyView,
    Value,
};

use crate::kernel::BP;
use crate::market::{Market, MarketInner};
use crate::pricing::PricePolicy;
use crate::shop::{ReorderPolicy, Shop};

/// Pokrycie zapasu, którego reguła nie odróżnia od nieskończonego.
///
/// 999 dób to blisko trzy lata gry — żaden towar w katalogu tyle nie leży, a liczba
/// mieści się w `i32` bez zapasu na przepełnienie przy mnożeniu przez procent.
pub const MAX_COVER: i32 = 999;

/// Podsumowanie doby polityk — metryki balansatora i panelu.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct PolicyDay {
    /// Ile zakładów w ogóle miało dziś politykę do wykonania.
    pub sites: u32,
    /// Ile wykonań wypadło w martwej strefie (`cooldown_h`) i zostało pominiętych.
    pub on_cooldown: u32,
    /// Ile akcji wykonano.
    pub applied: u32,
    /// Ile akcji wróciło bez skutku, bo metryki nie dało się odczytać.
    pub blind: u32,
    /// Ile alertów i pytań do gracza polityka wystawiła. Nie zmieniają świata —
    /// czeka na nie panel M9.
    pub alerts: u32,
}

/// Wszystko, co polityka wie o jednym towarze na półce.
///
/// Liczby są **te same**, którymi liczy się przecena (`reprice_all`) — i to jest
/// warunek, a nie zbieg okoliczności: gdyby reguła czytała inny koszt własny niż
/// sterownik ceny, „ogranicz cenę do 105 % kosztu" znaczyłoby co innego w regule
/// niż w wyniku.
#[derive(Clone, Copy, Debug)]
struct GoodFacts {
    good: GoodId,
    /// Cena półkowa, brutto (`K-7`).
    price_gross: Money,
    /// Koszt własny za jednostkę ceny, netto.
    unit_cost_net: Money,
    /// Cena netto — do marży, liczona tym samym silnikiem podatkowym co przecena.
    price_net: Money,
    stock: Qty,
    /// Pokrycie zapasu w dobach wg obrotu tygodniowego.
    ///
    /// Trzy przypadki i każdy znaczy co innego: przy sprzedaży to zapas przez dzienny
    /// obrót; przy **pustej półce** to zero, bo pusta półka nie ma pokrycia niezależnie
    /// od tego, czy coś się sprzedawało; przy zapasie bez sprzedaży to [`MAX_COVER`],
    /// bo towar leżący bez ruchu ma pokrycie **nieskończone**, a nie nieznane —
    /// i dokładnie tak powinna go czytać reguła „martwy zapas".
    stock_days: i32,
    turnover_7d: Qty,
    days_to_expiry: Option<i32>,
    /// Ile miejsc na półce stoi pustych.
    shelf_gap: i32,
    days_since_change: i32,
    cheapest_competitor: Option<Money>,
    avg_competitor: Option<Money>,
    competitors: i32,
}

/// Widok zakładu handlowego dla reguły.
struct ShopView<'a> {
    goods: &'a [GoodFacts],
    cash: Option<Money>,
    manager_skill: Option<i32>,
    staff_mood: Option<i32>,
    open_positions: i32,
    cal: SimCalendar,
    /// Stawka VAT w punktach bazowych — przelicznik jawnej konwersji `brutto`/`netto`.
    /// Do M8 zawsze zero (`NoTax`), ale czytana z silnika podatkowego, a nie wpisana.
    vat_bp: i32,
}

impl ShopView<'_> {
    fn fakt(&self, r: GoodRef) -> Option<&GoodFacts> {
        let GoodRef::Id(g) = r else {
            // `TEN_TOWAR` jest podstawiany przez ewaluator, zanim widok cokolwiek
            // zobaczy. Gołe `This` tutaj znaczy politykę wykonywaną bez kontekstu
            // towaru — i wtedy metryka towarowa nie ma odpowiedzi, a nie ma zera.
            return None;
        };
        self.goods
            .binary_search_by_key(&g, |f| f.good)
            .ok()
            .map(|i| &self.goods[i])
    }
}

impl PolicyView for ShopView<'_> {
    fn vat_bp(&self, _ctx: &MetricCtx) -> i32 {
        self.vat_bp
    }

    fn metric(&self, m: Metric, _ctx: &MetricCtx) -> Option<Value> {
        let pieniadz = |m: Money| Some(Value::Money(m));
        match m {
            Metric::Price { good, basis } => {
                let f = self.fakt(good)?;
                pieniadz(match basis {
                    PriceBasis::GrossRetail => f.price_gross,
                    PriceBasis::NetB2B => f.price_net,
                })
            }
            Metric::UnitCost(good) => pieniadz(self.fakt(good)?.unit_cost_net),
            Metric::Margin(good) => {
                let f = self.fakt(good)?;
                let cena = f.price_net.get();
                if cena <= 0 {
                    return None;
                }
                let marza = (cena - f.unit_cost_net.get()).saturating_mul(BP) / cena;
                Some(Value::Bp(Bp(
                    marza.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
                )))
            }
            // Promień jest tu **ignorowany** i to jest świadome. Obraz konkurencji
            // sklepu (`CompetitorSnapshot`, M5c) jest budowany jednym promieniem
            // obserwacji dla całego sklepu, bo pełny cennik każdego sąsiada to 1,2 mln
            // wpisów na metropolię (`W-2`). Reguła z promieniem 3 km i reguła z 5 km
            // dostają więc dziś tę samą liczbę.
            //
            // ponytail: sufit nazwany — dopóki polityka ma limit dwóch metryk
            // konkurencyjnych, różnica między promieniami jest różnicą w zapisie,
            // a nie w wyniku. Ścieżka wyjścia: `observe_competitors` dostaje drugi
            // promień, kiedy panel M9 pokaże, że gracz na tej różnicy buduje decyzję.
            Metric::CheapestCompetitorPrice { good, .. } => {
                self.fakt(good)?.cheapest_competitor.map(Value::Money)
            }
            Metric::AvgCompetitorPrice { good, .. } => {
                self.fakt(good)?.avg_competitor.map(Value::Money)
            }
            Metric::CompetitorCount { .. } => {
                // Liczba konkurentów jest cechą sklepu, nie towaru — bierzemy
                // największą obserwację, bo tyle sąsiadów sklep w ogóle widzi.
                let n = self.goods.iter().map(|f| f.competitors).max()?;
                Some(Value::Count(n))
            }
            Metric::Stock(good) => Some(Value::Qty(self.fakt(good)?.stock.get())),
            Metric::StockDays(good) => Some(Value::Days(self.fakt(good)?.stock_days)),
            Metric::Turnover7d(good) => Some(Value::Qty(self.fakt(good)?.turnover_7d.get())),
            Metric::Sales7d(good) => Some(Value::Qty(self.fakt(good)?.turnover_7d.get())),
            Metric::DaysToExpiry(good) => self.fakt(good)?.days_to_expiry.map(Value::Days),
            Metric::ShelfGap(good) => Some(Value::Count(self.fakt(good)?.shelf_gap)),
            Metric::DaysSinceLastChange(good) => {
                Some(Value::Days(self.fakt(good)?.days_since_change))
            }
            Metric::CashBalance => self.cash.map(Value::Money),
            Metric::ManagerSkill => self.manager_skill.map(Value::Count),
            Metric::StaffMood => self.staff_mood.map(Value::Count),
            Metric::OpenPositions(_) => Some(Value::Count(self.open_positions)),
            Metric::DayOfMonth => Some(Value::Count(i32::from(self.cal.day_of_month()))),
            Metric::HourOfDay => Some(Value::Count(i32::from(self.cal.minute_of_day() / 60))),
            Metric::DayOfWeek => Some(Value::Enum(self.cal.day_of_week() as u8)),
            Metric::Season => Some(Value::Enum((self.cal.month_of_year() - 1) / 3)),
            // Reszta metryk nie ma jeszcze skąd wziąć liczby. `None` znaczy „nie wiem"
            // i reguła się na nich nie wyzwala — to jest właściwe zachowanie, a nie
            // dziura: `MachineUtilization` należy do zakładu produkcyjnego (M7e),
            // `StaffTurnover12m` i `MedianMarketWage` do panelu rynku pracy (M7f),
            // `Receivables` do finansów firmy (M7d).
            Metric::MachineUtilization
            | Metric::StaffTurnover12m
            | Metric::MedianMarketWage(_)
            | Metric::Receivables => None,
        }
    }
}

impl Market {
    /// Doba polityk zdelegowanych zakładów.
    ///
    /// Wołana **przed** [`Market::reprice_all`] i to jest kontrakt kolejności: polityka
    /// ustawia sterownik ceny, a przecena go wykonuje razem z ogranicznikiem marży.
    /// Odwrotnie polityka nadpisywałaby świeżo policzoną cenę i sklep pracowałby
    /// na wyniku sprzed doby.
    pub fn run_policies(
        &self,
        firms: &mut Firms,
        cash: &std::collections::BTreeMap<SiteId, Money>,
        t: Tick,
    ) -> PolicyDay {
        let mut d = PolicyDay::default();
        let cal = SimCalendar::new(t);
        let mut m = self.lock();
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
            d.sites += 1;
            if !deleg.ready(t) {
                d.on_cooldown += 1;
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
            let skill = firms
                .manager_of_site(site)
                .map(|mgr| i32::from(mgr.skill_mgmt.get()));
            let saldo = cash.get(&site).copied();
            let mut fakty = zbierz_fakty(&m, &ch, i, t);
            // Domknięcie zamiast wolnej funkcji nie przejdzie: zwracany widok
            // pożycza argument, a zapis `|g: &[GoodFacts]| ShopView { .. }` nie
            // umie tego związać. Funkcja wolna wiąże życie wprost.
            fn widok<'a>(
                goods: &'a [GoodFacts],
                saldo: Option<Money>,
                skill: Option<i32>,
                mgmt: u8,
                wakaty: i32,
                cal: SimCalendar,
                vat_bp: i32,
            ) -> ShopView<'a> {
                ShopView {
                    goods,
                    cash: saldo,
                    manager_skill: skill,
                    staff_mood: Some(i32::from(mgmt)),
                    open_positions: wakaty,
                    cal,
                    vat_bp,
                }
            }

            // Polityka wykonuje się **raz na towar**: zakres kategorii albo grupy
            // dotyczy każdego towaru osobno i to jest cała treść `TEN_TOWAR`.
            // Warunki liczą się na stanie **z początku kroku** — to jest właściwe,
            // bo doba polityki ocenia sklep, jakim był rano, a nie taki, jakim
            // uczyniła go poprzednia reguła.
            let mut decyzje: Vec<(GoodId, Action, DecisionReason)> = Vec::new();
            {
                let v = widok(&fakty, saldo, skill, mgmt.0, wakaty, cal, vat);
                for f in &fakty {
                    for dec in evaluate(&polityka, &MetricCtx::for_good(f.good), &v) {
                        let (a, r) = dec.split();
                        // Kopia akcji **raz na wyzwoloną regułę**, a nie raz na
                        // ewaluację: ewaluator zwraca referencje do polityki właśnie
                        // po to, żeby nie alokować, a wykonawca potrzebuje własności,
                        // bo między ewaluacją a wykonaniem pożycza rynek na mutowalnie.
                        decyzje.push((f.good, a.clone(), r));
                    }
                }
            }

            for (good, a, powod) in decyzje {
                let ctx = MetricCtx::for_good(good);
                // **Akcje** liczą się na stanie bieżącym, bo druga akcja tej samej
                // reguły ma widzieć, co zrobiła pierwsza: „ustaw cenę ORAZ ogranicz
                // do widełek" bez tego cofa własną pierwszą akcję.
                let v = widok(&fakty, saldo, skill, mgmt.0, wakaty, cal, vat);
                let skutek = rozstrzygnij(&a, &ctx, &v, &fakty);
                match zastosuj(&mut m.shops[i], &mut fakty, tax.as_ref(), skutek, t) {
                    Wynik::Zrobione => d.applied += 1,
                    Wynik::Alert => d.alerts += 1,
                    Wynik::Slepa => d.blind += 1,
                }
                zapisz(firms, site, t, powod);
            }
            if let Some(s) = firms.site_mut(site) {
                if let Some(dd) = s.delegation.as_mut() {
                    dd.last_run = t;
                }
            }
        }
        m.tax = tax;
        d
    }
}

/// Co wyszło z wykonania jednej akcji.
enum Wynik {
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
enum Skutek {
    Cena(GoodId, Money),
    Marza(GoodId, i32),
    Zamowienie(GoodId, Qty),
    Alert,
    Slepa,
}

/// Zapis powodu do dziennika firmy (00 §7). Osobna funkcja, bo woła się ją
/// po każdej akcji i **nie ma drogi obok**: `Decided` nie daje akcji bez powodu.
fn zapisz(firms: &mut Firms, site: SiteId, t: Tick, powod: DecisionReason) {
    if let Some(firma) = firms.site(site).map(|s| s.firm) {
        firms.log(firma, t, powod);
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
fn rozstrzygnij(a: &Action, ctx: &MetricCtx, v: &impl PolicyView, fakty: &[GoodFacts]) -> Skutek {
    let kwota = |e: &Expr| match magnat_policy::eval(e, ctx, v) {
        Some(Value::Money(m)) => Some(m),
        _ => None,
    };
    let fakt = |g: GoodId| fakty.iter().find(|f| f.good == g);
    match a {
        Action::SetPrice { good, to } => match (cel(*good, ctx), kwota(to)) {
            (Some(g), Some(c)) => Skutek::Cena(g, c),
            _ => Skutek::Slepa,
        },
        Action::AdjustPrice { good, by } => {
            let (Some(g), Some(delta)) = (cel(*good, ctx), kwota(by)) else {
                return Skutek::Slepa;
            };
            let Some(f) = fakt(g) else {
                return Skutek::Slepa;
            };
            Skutek::Cena(g, Money(f.price_gross.get().saturating_add(delta.get())))
        }
        Action::SetMargin { good, bp } => match cel(*good, ctx) {
            Some(g) => Skutek::Marza(g, bp.get()),
            None => Skutek::Slepa,
        },
        Action::Markdown { good, bp } => {
            let (Some(g), Some(f)) = (cel(*good, ctx), cel(*good, ctx).and_then(fakt)) else {
                return Skutek::Slepa;
            };
            Skutek::Cena(g, Bp(BP as i32 - bp.get()).of(f.price_gross))
        }
        Action::ClampPrice { good, min, max } => {
            let (Some(g), Some(lo), Some(hi)) = (cel(*good, ctx), kwota(min), kwota(max)) else {
                return Skutek::Slepa;
            };
            let Some(f) = fakt(g) else {
                return Skutek::Slepa;
            };
            // Widełki podane odwrotnie są błędem gracza, a nie powodem do paniki:
            // bierzemy je w tej kolejności, w której są przedziałem.
            let (lo, hi) = (lo.get().min(hi.get()), lo.get().max(hi.get()));
            Skutek::Cena(g, Money(f.price_gross.get().clamp(lo, hi)))
        }
        Action::OrderUpTo { good, days } => {
            let Some(g) = cel(*good, ctx) else {
                return Skutek::Slepa;
            };
            let (Some(Value::Days(dni)), Some(f)) = (magnat_policy::eval(days, ctx, v), fakt(g))
            else {
                return Skutek::Slepa;
            };
            let dziennie = (f.turnover_7d.get() / 7).max(1);
            Skutek::Zamowienie(g, Qty(dziennie.saturating_mul(i64::from(dni.max(0)))))
        }
        Action::OrderQty { good, qty, .. } => {
            let Some(g) = cel(*good, ctx) else {
                return Skutek::Slepa;
            };
            match magnat_policy::eval(qty, ctx, v) {
                Some(Value::Qty(q)) => Skutek::Zamowienie(g, Qty(q.max(0))),
                _ => Skutek::Slepa,
            }
        }
        // Wycofanie z półki, alert i pytanie do gracza czekają na swoich wykonawców.
        // `RemoveFromShelf` wymaga zwolnienia oferty w arenie razem z linią półki
        // (M5b §5.3), a to jest ta sama ścieżka, którą zamyka zakład w M7d — tam
        // powstanie raz, a nie dwa razy. Alert i pytanie nie zmieniają świata:
        // czeka na nie pulpit firmy z M9.
        Action::RemoveFromShelf(_) | Action::Alert { .. } | Action::AskPlayer { .. } => {
            Skutek::Alert
        }
        Action::Hire { .. } | Action::RaiseWage { .. } | Action::PlanProduction { .. } => {
            // Walidator nie przepuszcza polityki w tych dziedzinach (`DomainNotAvailable`),
            // więc tutaj nie da się dojść inaczej niż polityką z zapisu gry sprzed
            // zmiany danych. Wtedy lepiej nie zrobić nic, niż zrobić coś innego.
            Skutek::Slepa
        }
    }
}

/// Zapisuje skutek do sklepu **i do tablicy faktów**.
///
/// Aktualizacja faktów jest częścią zastosowania, nie dodatkiem: kolejna akcja tej
/// samej reguły czyta cenę z tej tablicy przez widok, więc tablica nieaktualna znaczy
/// akcję liczoną na świecie sprzed chwili.
fn zastosuj(
    shop: &mut Shop,
    fakty: &mut [GoodFacts],
    tax: &dyn crate::tax::TaxEngine,
    s: Skutek,
    t: Tick,
) -> Wynik {
    match s {
        Skutek::Slepa => Wynik::Slepa,
        Skutek::Alert => Wynik::Alert,
        Skutek::Cena(good, cena) => {
            let cena = Money(cena.get().max(1));
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
            if let Some(f) = fakty.iter_mut().find(|f| f.good == good) {
                f.price_gross = cena;
                f.price_net = tax.net_from_gross(good, cena);
            }
            Wynik::Zrobione
        }
        Skutek::Marza(good, bp) => match shop.controllers.get_mut(&good) {
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
        Skutek::Zamowienie(good, cel) => {
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

/// Zbiera fakty o wszystkich towarach na półce jednego sklepu.
///
/// Liczby są te same, którymi liczy `reprice_all`; różnica jest taka, że tam wchodzą
/// do składania ceny, a tu do warunku reguły.
fn zbierz_fakty(m: &MarketInner, ch: &magnat_supply::Chain, i: usize, t: Tick) -> Vec<GoodFacts> {
    let shop = &m.shops[i];
    let cat = &m.chain.cat;
    let doba = magnat_core::time::MINUTES_PER_DAY;
    shop.shelf
        .lines
        .iter()
        .map(|linia| {
            let good = linia.good;
            let zaplecze = ch.store.shelf_state(shop.backroom, good);
            let polka = ch.store.shelf_state(shop.shelf_slot, good);
            let ilosc = cat
                .good(good)
                .units_of_mass(magnat_core::Mass(zaplecze.mass.0 + polka.mass.0))
                .get();
            let koszt = zaplecze.cost_total.get() + polka.cost_total.get();
            let unit_cost = if ilosc > 0 && koszt > 0 {
                Money(koszt).mul_ratio(crate::supply::PRICE_UNIT, ilosc)
            } else {
                m.goods.spec(good).map_or(Money::ZERO, |s| s.wholesale_base)
            };
            let pc = shop.controllers.get(&good);
            let cena = pc.map_or(Money::ZERO, |p| p.current);
            let obrót = pc.map_or(Qty::ZERO, crate::pricing::PriceController::turnover_7d);
            let dziennie = obrót.get() / 7;
            let termin = match (zaplecze.expires_at, polka.expires_at) {
                (Some(a), Some(b)) => Some(a.get().min(b.get())),
                (a, b) => a.or(b).map(magnat_core::SimMinute::get),
            }
            .map(|e| i32::try_from(e.saturating_sub(t.get()) / doba).unwrap_or(i32::MAX));
            let obs = shop.observed.get(good);
            GoodFacts {
                good,
                price_gross: cena,
                unit_cost_net: unit_cost,
                price_net: m.tax.net_from_gross(good, cena),
                stock: Qty(ilosc),
                stock_days: if ilosc <= 0 {
                    0
                } else if dziennie > 0 {
                    i32::try_from(ilosc / dziennie)
                        .unwrap_or(MAX_COVER)
                        .min(MAX_COVER)
                } else {
                    MAX_COVER
                },
                turnover_7d: obrót,
                days_to_expiry: termin,
                shelf_gap: i32::from(shop.shelf.slots)
                    - i32::try_from(shop.shelf.lines.len()).unwrap_or(0),
                days_since_change: pc.map_or(0, |p| {
                    i32::try_from(t.get().saturating_sub(p.last_change.get()) / doba)
                        .unwrap_or(i32::MAX)
                }),
                cheapest_competitor: obs.map(|o| o.cheapest),
                avg_competitor: obs.map(|o| o.median),
                competitors: obs.map_or(0, |o| i32::try_from(o.offers).unwrap_or(i32::MAX)),
            }
        })
        .collect()
}

impl Market {
    /// Cel zamówienia towaru w sklepie — po nim widać, czy polityka zapasu zadziałała.
    #[must_use]
    pub fn reorder_target(&self, site: SiteId, good: GoodId) -> Option<Qty> {
        let m = self.lock();
        let s = &m.shops[*m.by_site.get(&site)? as usize];
        s.inventory.reorder.get(&good).map(|r| r.target)
    }

    /// Polityka cenowa sterownika — po niej widać, czy regułę wykonano ceną stałą,
    /// czy marżą. Odczyt dla testów i dla panelu M9.
    #[must_use]
    pub fn price_policy(&self, site: SiteId, good: GoodId) -> Option<PricePolicy> {
        let m = self.lock();
        let s = &m.shops[*m.by_site.get(&site)? as usize];
        s.controllers.get(&good).map(|pc| pc.policy)
    }

    /// Podgląd obrazu konkurencji — wyłącznie do testów i do panelu.
    #[must_use]
    pub fn competitor_entry(&self, site: SiteId, good: GoodId) -> Option<(Money, Money, u32)> {
        let m = self.lock();
        let s = &m.shops[*m.by_site.get(&site)? as usize];
        s.observed
            .get(good)
            .map(|e| (e.cheapest, e.median, e.offers))
    }

    /// Rachunki bieżące zakładów handlowych — wejście metryki `saldo` w regule.
    ///
    /// Osobno od [`Market::run_policies`], bo saldo prowadzi `Books`, a `Books`
    /// i rejestr firm nie dają się pożyczyć ze świata naraz. Wołający czyta jedno,
    /// potem drugie — i to jest cała treść tej funkcji.
    #[must_use]
    pub fn shop_accounts(&self) -> Vec<(SiteId, crate::books::AccountId)> {
        let m = self.lock();
        m.shops.iter().map(|s| (s.site, s.account)).collect()
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
