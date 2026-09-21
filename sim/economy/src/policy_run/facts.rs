//! Fakty o zakładzie, widok reguły na nie i ślad doby (M7c WP7, M9d WP8).
//!
//! Osobny plik, bo to jest **inny temat** niż wykonanie: tutaj jest odczyt świata
//! i jego zamrożenie, tam decyzja i zapis. Ślad doby ([`PolicyTrace`]) mieszka tu,
//! a nie przy dry-runie, z tego samego powodu: jest ciągiem tych samych faktów,
//! tylko wczorajszych.

use magnat_core::{GoodId, Money, PriceBasis, Qty, SimCalendar, Tick};
use magnat_policy::{Bp, GoodRef, Metric, MetricCtx, PolicyView, Value};

use crate::kernel::BP;
use crate::market::MarketInner;

/// Pokrycie zapasu, którego reguła nie odróżnia od nieskończonego.
///
/// 999 dób to blisko trzy lata gry — żaden towar w katalogu tyle nie leży, a liczba
/// mieści się w `i32` bez zapasu na przepełnienie przy mnożeniu przez procent.
pub const MAX_COVER: i32 = 999;

/// Ile dób trzyma ślad zakładu. Trzydzieści, bo tyle obiecuje dry-run („przetestuj
/// na ostatnich 30 dniach", M9d §5.6 pkt 8).
pub const TRACE_DAYS: usize = 30;

/// Wszystko, co polityka wie o jednym towarze na półce.
///
/// Liczby są **te same**, którymi liczy się przecena (`reprice_all`) — i to jest
/// warunek, a nie zbieg okoliczności: gdyby reguła czytała inny koszt własny niż
/// sterownik ceny, „ogranicz cenę do 105 % kosztu" znaczyłoby co innego w regule
/// niż w wyniku.
///
/// Publiczne od M9d, bo to jest zarazem **próbka dnia** w śladzie zakładu: dry-run
/// odtwarza politykę na zapisanych faktach, a nie na drugiej, podobnej strukturze.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GoodFacts {
    pub good: GoodId,
    /// Cena półkowa, brutto (`K-7`).
    pub price_gross: Money,
    /// Koszt własny za jednostkę ceny, netto.
    pub unit_cost_net: Money,
    /// Cena netto — do marży, liczona tym samym silnikiem podatkowym co przecena.
    pub price_net: Money,
    pub stock: Qty,
    /// Pokrycie zapasu w dobach wg obrotu tygodniowego.
    ///
    /// Trzy przypadki i każdy znaczy co innego: przy sprzedaży to zapas przez dzienny
    /// obrót; przy **pustej półce** to zero, bo pusta półka nie ma pokrycia niezależnie
    /// od tego, czy coś się sprzedawało; przy zapasie bez sprzedaży to [`MAX_COVER`],
    /// bo towar leżący bez ruchu ma pokrycie **nieskończone**, a nie nieznane —
    /// i dokładnie tak powinna go czytać reguła „martwy zapas".
    pub stock_days: i32,
    pub turnover_7d: Qty,
    pub days_to_expiry: Option<i32>,
    /// Ile miejsc na półce stoi pustych.
    pub shelf_gap: i32,
    pub days_since_change: i32,
    pub cheapest_competitor: Option<Money>,
    pub avg_competitor: Option<Money>,
    pub competitors: i32,
    /// Obraz zawężony do promieni, o które pyta polityka (`R2-WP22`). Pusty slot ma
    /// `radius_m == 0`; slot o zerowej liczbie ofert znaczy „w tym promieniu nikogo
    /// nie widać", a nie „najtańszy kosztuje zero".
    pub near: [crate::pricing::NearRadius; crate::pricing::MAX_NEAR_RADII],
}

impl GoodFacts {
    /// Najtańszy konkurent w **zadanym** promieniu.
    ///
    /// `None` znaczy „nie wiem": albo obserwacji nie ma, albo w tym promieniu nie ma
    /// nikogo. Reguła ma wtedy nie podjąć decyzji, a nie podjąć ją na zerze.
    ///
    /// Promień, o który nikt nie pytał w chwili obserwacji, wraca do obrazu bazowego —
    /// bo obserwacja zna promienie z **poprzedniej** doby, a reguła dopisana dziś
    /// zaczyna być widoczna jutro. To jest to samo opóźnienie, które ma cały obraz
    /// konkurencji (1–7 dób), a nie nowa klasa błędu.
    #[must_use]
    pub fn cheapest_within(&self, radius_m: u32) -> Option<Money> {
        match self.near.iter().find(|n| n.radius_m == radius_m) {
            Some(n) if n.offers > 0 => Some(n.cheapest),
            Some(_) => None,
            None => self.cheapest_competitor,
        }
    }

    /// Mediana ceny konkurencji w zadanym promieniu. Jak [`GoodFacts::cheapest_within`].
    #[must_use]
    pub fn avg_within(&self, radius_m: u32) -> Option<Money> {
        match self.near.iter().find(|n| n.radius_m == radius_m) {
            Some(n) if n.offers > 0 => Some(n.median),
            Some(_) => None,
            None => self.avg_competitor,
        }
    }

    /// Ilu konkurentów w zadanym promieniu.
    #[must_use]
    pub fn competitors_within(&self, radius_m: u32) -> i32 {
        match self.near.iter().find(|n| n.radius_m == radius_m) {
            Some(n) => i32::try_from(n.offers).unwrap_or(i32::MAX),
            None => self.competitors,
        }
    }
}

/// Jedna doba śladu: fakty, na których polityka pracowała tego dnia.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TraceDay {
    pub day: Tick,
    pub facts: Vec<GoodFacts>,
}

/// Ostatnie [`TRACE_DAYS`] dób zakładu — materiał dry-runu (M9d §5.6 pkt 8).
///
/// Prowadzą go **wyłącznie zakłady śledzone** ([`crate::LostSaleTracking`] różne
/// od `None`), czyli te, o które gracz pytał albo które są jego. Nie wchodzi do hasha
/// stanu z tego samego powodu co pierścień utraconych sprzedaży i dziennik przecen:
/// oznaczenie zakładu nie jest stanem świata (`U-22`), więc zapis idący za nim też
/// nie może nim być.
///
/// ponytail: sufit nazwany — trzydzieści dób po czterdzieści towarów to ~100 kB
/// na zakład i dlatego trzyma się to przy zakładach oznaczonych, a nie przy wszystkich.
/// Ścieżka wyjścia, gdyby gracz oznaczał setki zakładów naraz: próbkowanie co dekadę
/// zamiast co dobę, bo piramida mip wykresu i tak schodzi do dekad (`M9b` §5.8).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct PolicyTrace {
    days: Vec<TraceDay>,
}

impl PolicyTrace {
    /// Dokłada dobę, wypychając najstarszą.
    pub fn push(&mut self, day: TraceDay) {
        if self.days.len() >= TRACE_DAYS {
            self.days.remove(0);
        }
        self.days.push(day);
    }

    #[must_use]
    pub fn days(&self) -> &[TraceDay] {
        &self.days
    }
}

/// Widok zakładu handlowego dla reguły.
///
/// Buduje się z tego, co firma o sobie wie: własnej półki, własnego kosztu i obrazu
/// konkurencji z opóźnieniem 1–7 dni. To jest granica asymetrii informacji z M7 §5.8 —
/// reguła widzi ceny półkowe sąsiadów, bo te są publiczne, i nie widzi ich kosztów,
/// bo widok ich nie ma.
pub(crate) struct ShopView<'a> {
    pub goods: &'a [GoodFacts],
    pub cash: Option<Money>,
    pub manager_skill: Option<i32>,
    pub staff_mood: Option<i32>,
    pub open_positions: i32,
    pub cal: SimCalendar,
    /// Stawka VAT w punktach bazowych — przelicznik jawnej konwersji `brutto`/`netto`.
    /// Do M8 zawsze zero (`NoTax`), ale czytana z silnika podatkowego, a nie wpisana.
    pub vat_bp: i32,
}

impl ShopView<'_> {
    pub(crate) fn fakt(&self, r: GoodRef) -> Option<&GoodFacts> {
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
            // Promień **wchodzi do odczytu** od R2e (`R2-WP22`). Do tej chwili był
            // sprawdzany przez walidator (limit 10 km) i ignorowany przy liczeniu:
            // reguła z promieniem 3 km i reguła z 5 km dostawały tę samą liczbę, bo
            // obraz konkurencji powstawał jednym promieniem obserwacji. Gracz widział
            // dwie różne reguły i jeden wynik, a PRD §6.3 cytuje „w promieniu 3 km"
            // jako treść reguły.
            //
            // Zawężenia liczy `observe_competitors`, bo to on ma odległości; tutaj
            // jest wyłącznie wybór slotu. Koszt ogranicza limit dwóch metryk
            // konkurencyjnych na politykę, więc promieni na sklep jest najwyżej trzy.
            Metric::CheapestCompetitorPrice { good, radius_m, .. } => {
                self.fakt(good)?.cheapest_within(radius_m).map(Value::Money)
            }
            Metric::AvgCompetitorPrice { good, radius_m, .. } => {
                self.fakt(good)?.avg_within(radius_m).map(Value::Money)
            }
            Metric::CompetitorCount { radius_m } => {
                // Liczba konkurentów jest cechą sklepu, nie towaru — bierzemy
                // największą obserwację, bo tyle sąsiadów sklep w ogóle widzi.
                let n = self
                    .goods
                    .iter()
                    .map(|f| f.competitors_within(radius_m))
                    .max()?;
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

/// Zbiera fakty o wszystkich towarach na półce jednego sklepu.
///
/// Liczby są te same, którymi liczy `reprice_all`; różnica jest taka, że tam wchodzą
/// do składania ceny, a tu do warunku reguły.
/// Silnik podatkowy jedzie **argumentem, nie przez `m.tax`**, i to jest poprawka,
/// nie porządek: `run_policies` wyjmuje silnik ze struktury na czas pętli (bo `m.shops`
/// jest w niej pożyczane mutowalnie), więc odczyt z `m.tax` trafiał w zaślepkę `NoTax`
/// dokładnie wtedy, gdy liczył cenę netto do marży. Dziś stawka wynosi zero i różnicy
/// nie widać; przy pierwszym VAT-cie M8 reguła „marża > 20 %" czytałaby marżę liczoną
/// bez podatku, a ogranicznik z `K-7` — z podatkiem.
pub(crate) fn zbierz_fakty(
    m: &MarketInner,
    ch: &magnat_supply::Chain,
    tax: &dyn crate::tax::TaxEngine,
    i: usize,
    t: Tick,
) -> Vec<GoodFacts> {
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
                price_net: tax.net_from_gross(good, cena),
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
                near: obs.map(|o| o.near).unwrap_or_default(),
            }
        })
        .collect()
}
