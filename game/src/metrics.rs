//! Historia metryk gracza — `Series` z `M9b` dostaje wreszcie pisarza (`DF-4`).
//!
//! # Co tu jest, a czego nie ma
//!
//! `magnat_ui::Series` powstał w `M9b`: cztery poziomy piramidy (doba, dekada,
//! miesiąc, kwartał), sumy bezstratne, wykres ≤ 2000 odcinków niezależnie od
//! zakresu. Brakowało **czegoś, co do niego pisze** — i to jest ten plik.
//!
//! Zapis idzie raz na dobę gry, z kroku sesji, i **nie dotyka świata**: metryki są
//! stroną widoku, więc nie wchodzą do hasha stanu (00 §3.6) i nie zmieniają wyniku
//! symulacji. Przebieg bezgłowy prowadzi je tak samo jak klient — inaczej wykres
//! w oknie pokazywałby inną historię niż ta, którą mierzy balansator.
//!
//! # Czego dry-run tu nie szuka
//!
//! Próba polityki liczy się ze **śladu doby zakładu** (`Market::dry_run` po
//! `PolicyTrace`), a nie z tych serii (`DH-5`). Ślad niesie dokładnie to, co widzi
//! reguła; seria niesie to, co widzi gracz. To są dwie różne prawdy o tym samym
//! dniu i mieszanie ich dałoby próbę, która zgadza się z wykresem i rozjeżdża
//! z wykonaniem.

use magnat_core::Money;
use magnat_ui::{Series, SeriesKey};

use crate::career::Holdings;
use crate::Session;

/// Która metryka. Kolejność jest kontraktem, bo `as_index()` indeksuje tablicę serii.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum MetricId {
    /// Gotówka gospodarstwa gracza.
    Cash,
    /// Utarg netto zakładów gracza w dobie (`K-7` — bez VAT-u).
    RevenueNet,
    /// Wynik zakładów gracza w dobie.
    Profit,
    /// Obsadzone etaty u gracza.
    Headcount,
    /// Zakłady gracza.
    Sites,
    /// Utracone wizyty w zakładach gracza w dobie.
    LostSales,
    /// Indeks cen konsumenckich miasta w punktach bazowych.
    Cpi,
    /// Kurs spółki gracza — **cena jednego punktu bazowego** (`K-85`), w groszach.
    ///
    /// Historia kursu jest stroną widoku, a nie stanem świata: `Listing` trzyma
    /// ostatni i poprzedni fixing i niczego więcej trzymać nie ma, bo pierścień
    /// notowań wchodziłby do hasha i do zapisu za wykres. Zero znaczy „spółka
    /// nienotowana", a nie „kurs zerowy" — i tak to czyta panel giełdy.
    StockPrice,
}

impl MetricId {
    pub const ALL: [MetricId; 8] = [
        MetricId::Cash,
        MetricId::RevenueNet,
        MetricId::Profit,
        MetricId::Headcount,
        MetricId::Sites,
        MetricId::LostSales,
        MetricId::Cpi,
        MetricId::StockPrice,
    ];

    #[must_use]
    pub const fn as_index(self) -> usize {
        self as usize
    }

    /// Klucz tekstu: `ui.metric.<key>`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            MetricId::Cash => "cash",
            MetricId::RevenueNet => "revenue_net",
            MetricId::Profit => "profit",
            MetricId::Headcount => "headcount",
            MetricId::Sites => "sites",
            MetricId::LostSales => "lost_sales",
            MetricId::Cpi => "cpi",
            MetricId::StockPrice => "stock_price",
        }
    }

    /// Czy metryka jest kwotą — od tego zależy formatowanie osi i wartości.
    #[must_use]
    pub const fn is_money(self) -> bool {
        matches!(
            self,
            MetricId::Cash | MetricId::RevenueNet | MetricId::Profit | MetricId::StockPrice
        )
    }
}

/// Historia metryk gracza, doba po dobie.
pub struct MetricsRecorder {
    series: Vec<Series>,
    /// Ostatnia zapisana doba. `None` przed pierwszym zapisem — pierwsza doba gry
    /// ma dostać próbkę, a nie zostać pominięta jako „ta sama co poprzednia".
    last_day: Option<u64>,
    /// Utarg i wynik narastają w ciągu doby i zerują się przy zapisie.
    day_revenue: i64,
    day_profit: i64,
    day_lost: i64,
}

impl Default for MetricsRecorder {
    fn default() -> MetricsRecorder {
        MetricsRecorder {
            series: MetricId::ALL
                .iter()
                .map(|m| Series::new(SeriesKey(u16::try_from(m.as_index()).unwrap_or(0))))
                .collect(),
            last_day: None,
            day_revenue: 0,
            day_profit: 0,
            day_lost: 0,
        }
    }
}

impl MetricsRecorder {
    #[must_use]
    pub fn series(&self, m: MetricId) -> &Series {
        &self.series[m.as_index()]
    }

    /// Ile dób historii ma zapis.
    #[must_use]
    pub fn days(&self) -> u32 {
        self.series[0].days()
    }

    /// Zapisuje dobę, jeśli właśnie minęła. Wołane z kroku sesji co tick — sprawdzenie
    /// numeru doby jest tańsze niż osobny harmonogram, a `MINUTES_PER_DAY` jest
    /// jedyną rzeczą, którą trzeba o czasie wiedzieć.
    pub fn maybe_record(&mut self, session: &Session) {
        let doba = session.tick().get() / magnat_core::time::MINUTES_PER_DAY;
        if self.last_day == Some(doba) {
            return;
        }
        self.last_day = Some(doba);
        let h = Holdings::of(session);
        self.zbierz_dobe(session, &h);
        self.record(session, &h);
    }

    /// Zbiera dobowe liczby zakładów gracza tuż przed zapisem.
    ///
    /// **Zbieramy, zamiast przyjmować zgłoszenia**, z tego samego powodu co kronika
    /// (`DI-4`): liczby już są po stronie `sim/*`, a wołający z symulacji nie ma jak
    /// sięgnąć do `game/`. Utarg i wynik idą z rachunku zakładu, utracone wizyty
    /// z dobowego histogramu sklepu — obie liczby są **dobowe**, więc pytanie raz
    /// na dobę daje dokładnie jedną próbkę.
    fn zbierz_dobe(&mut self, session: &Session, h: &Holdings) {
        let Some(m) = session.market.as_ref() else {
            return;
        };
        let do_ = session.tick();
        let od = magnat_core::Tick(do_.get().saturating_sub(magnat_core::time::MINUTES_PER_DAY));
        for site in &h.sites {
            if let Some(r) = m.income_statement(*site, od, do_) {
                self.day_revenue = self.day_revenue.saturating_add(r.revenue.get());
                self.day_profit = self.day_profit.saturating_add(r.net_result().get());
            }
            if let Some(hist) = m.lost_histogram(*site) {
                // Suma po powodach: histogram nie ma pola „razem" i dobrze, bo
                // wtedy trzeba by pilnować, żeby zgadzało się z tablicą.
                let razem: i64 = hist.by_cause.iter().map(|n| i64::from(*n)).sum();
                self.day_lost = self.day_lost.saturating_add(razem);
            }
        }
    }

    fn record(&mut self, session: &Session, h: &Holdings) {
        let gotowka = gotowka_gracza(session).get();
        let cpi = session
            .market
            .as_ref()
            .map_or(10_000, |m| i64::from(m.cpi_index_bp()));
        // Kurs spółki gracza. Firm gracz ma najwyżej jedną, więc „pierwsza" jest
        // jedyną — a spółka nienotowana daje zero, czyli przerwę w wykresie.
        let kurs = h.firms.first().map_or(0, |k| {
            session
                .app
                .world
                .get_resource::<magnat_economy::equity::Equity>()
                .and_then(|e| e.listing(*k))
                .map_or(0, |l| l.last_fixing.get())
        });
        let wartosci = [
            gotowka,
            self.day_revenue,
            self.day_profit,
            i64::from(h.employees),
            i64::try_from(h.sites.len()).unwrap_or(0),
            self.day_lost,
            cpi,
            kurs,
        ];
        for (i, v) in wartosci.iter().enumerate() {
            self.series[i].push_day(*v);
        }
        self.day_revenue = 0;
        self.day_profit = 0;
        self.day_lost = 0;
    }
}

/// Gotówka gospodarstwa gracza. Zero w trybie przeglądu — gracz bez postaci nie ma
/// konta, a nie ma konta pustego.
#[must_use]
pub fn gotowka_gracza(session: &Session) -> Money {
    let Some(p) = session.player() else {
        return Money::ZERO;
    };
    session
        .app
        .world
        .get::<magnat_agents::Household>(p.household.0)
        .map_or(Money::ZERO, |h| h.bank)
}
