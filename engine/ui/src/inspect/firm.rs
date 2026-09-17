//! Karta firmy i panel ludzi (M7f WP16, PRD §14.3).
//!
//! **Karta nie liczy niczego sama** — tak samo jak karta sklepu. Dostaje
//! [`FirmPanelSnapshot`], czyli jedyne wejście interfejsu do warstwy zarządczej,
//! i zamienia je w tekst. Powody renderuje wyłącznie [`crate::describe`] (`K-12`).
//!
//! # Zakaz fałszywej precyzji
//!
//! Zakładka „Kurs" pokazuje wynik ostatniego „co jeśli" i **nie ma prawa pokazać
//! kwoty** (§5.10, `R15`). Nie jest to samodyscyplina: migawka niesie `OutlookRow`,
//! a `OutlookRow` kwoty nie ma. Zdanie „prognozowany zysk: 240 000 zł" jest tu
//! niewyrażalne, bo nie ma z czego go złożyć. Widać: ile wariantów porównano,
//! który wygrał, o ile pewnie (margines) i w którą stronę (`Trend`).
//!
//! Kwoty do grosza w tej karcie pochodzą **wyłącznie** z ksiąg firmy: saldo
//! rachunku, koszt miesięczny, wynik ostatniego domkniętego miesiąca.

use crate::inspect::reason;
use crate::loc::{Catalog, Locale};
use magnat_city::{ChargeState, FiscalPeriod, TaxCharge};
use magnat_firms::panel::{EmployeeRow, FirmPanelSnapshot, OutlookRow, SiteRow};
use magnat_firms::StrAction;

/// Ile ostatnich decyzji pokazuje oś czasu karty.
///
/// Dziennik firmy ma trzydzieści dwa wpisy, ale karta jest odpowiedzią na
/// „co ta firma ostatnio zrobiła", a nie kroniką — dwanaście mieści się na ekranie
/// i nie spycha tabeli zakładów poniżej krawędzi.
const RECENT_DECISIONS: usize = 12;

/// Zakładka panelu firmy.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FirmTab {
    /// Zakłady: gdzie, ile osób, ile kosztują, jak stoją.
    #[default]
    Sites,
    /// Ludzie: kto gdzie pracuje i za ile — razem z drzewem organizacyjnym.
    People,
    /// Kurs: strategia, kampania i wynik ostatniego „co jeśli".
    Course,
    /// Decyzje: oś ostatnich powodów w języku gracza.
    Decisions,
    /// Daniny: co miasto naliczyło, od czego i czy zapłacone (M8a WP2).
    Taxes,
}

impl FirmTab {
    pub const ALL: [FirmTab; 5] = [
        FirmTab::Sites,
        FirmTab::People,
        FirmTab::Course,
        FirmTab::Decisions,
        FirmTab::Taxes,
    ];

    #[must_use]
    pub fn label(self, c: &Catalog, l: Locale) -> String {
        let klucz = match self {
            FirmTab::Sites => "ui.firm.tab.sites",
            FirmTab::People => "ui.firm.tab.people",
            FirmTab::Course => "ui.firm.tab.course",
            FirmTab::Decisions => "ui.firm.tab.decisions",
            FirmTab::Taxes => "ui.firm.tab.taxes",
        };
        c.fmt_key(l, klucz, &[])
    }
}

/// Karta firmy złożona z migawki — gotowe wiersze, bez dalszej arytmetyki.
pub struct FirmCard {
    name: String,
    header: Vec<String>,
    sites: Vec<String>,
    people: Vec<String>,
    course: Vec<String>,
    decisions: Vec<String>,
    taxes: Vec<String>,
}

impl FirmCard {
    #[must_use]
    pub fn build(c: &Catalog, l: Locale, s: &FirmPanelSnapshot) -> FirmCard {
        FirmCard {
            name: s.name.clone(),
            header: naglowek(c, l, s),
            sites: zaklady(c, l, s),
            people: ludzie(c, l, s),
            course: kurs(c, l, s),
            decisions: decyzje(c, l, s),
            taxes: Vec::new(),
        }
    }

    /// Dokłada zakładkę danin.
    ///
    /// Osobnym wywołaniem, a nie polem `FirmPanelSnapshot`, bo migawkę składa
    /// `sim/firms`, a ten crate **nie widzi miasta** i widzieć nie może —
    /// zależność idzie `city → economy → firms` i odwrócenie jej zamknęłoby cykl.
    /// Kartę składa więc ten, kto widzi obie strony, czyli warstwa prezentacji.
    ///
    /// Zakład bez ani jednego obciążenia zostawia zakładkę pustą, a pusta zakładka
    /// mówi „brak danych" — i to jest właściwa odpowiedź, bo firma, której miasto
    /// jeszcze nic nie naliczyło, różni się od firmy, która wszystko zapłaciła.
    #[must_use]
    pub fn with_taxes(mut self, c: &Catalog, l: Locale, charges: &[TaxCharge]) -> FirmCard {
        self.taxes = daniny(c, l, charges);
        self
    }

    /// Nagłówek wspólny dla zakładek.
    #[must_use]
    pub fn render_header(&self, c: &Catalog, l: Locale) -> String {
        use std::fmt::Write;
        let mut s = c.fmt_key(l, "ui.firm.header", &[("firma", &self.name)]);
        s.push('\n');
        for w in &self.header {
            let _ = writeln!(s, "  {w}");
        }
        s
    }

    /// Cała treść panelu jako tekst — forma testowalna w CI bez GPU.
    #[must_use]
    pub fn render_text(&self, c: &Catalog, l: Locale) -> String {
        let mut s = self.render_header(c, l);
        for t in FirmTab::ALL {
            s.push_str(&self.render_tab(c, l, t));
        }
        s
    }

    /// Jedna zakładka — tego używa widget.
    #[must_use]
    pub fn render_tab(&self, c: &Catalog, l: Locale, tab: FirmTab) -> String {
        use std::fmt::Write;
        let wiersze = match tab {
            FirmTab::Sites => &self.sites,
            FirmTab::People => &self.people,
            FirmTab::Course => &self.course,
            FirmTab::Decisions => &self.decisions,
            FirmTab::Taxes => &self.taxes,
        };
        let mut s = format!("{}:\n", tab.label(c, l));
        if wiersze.is_empty() {
            let _ = writeln!(s, "  {}", c.fmt_key(l, "ui.firm.empty", &[]));
        }
        for w in wiersze {
            let _ = writeln!(s, "  {w}");
        }
        s
    }
}

fn naglowek(c: &Catalog, l: Locale, s: &FirmPanelSnapshot) -> Vec<String> {
    let mut out = vec![
        c.fmt_key(
            l,
            "ui.firm.summary",
            &[
                ("zaklady", &s.sites.len().to_string()),
                ("ludzie", &s.headcount().to_string()),
                ("wakaty", &s.vacancies().to_string()),
            ],
        ),
        c.fmt_key(
            l,
            "ui.firm.money",
            &[
                ("kasa", &crate::zlotowki(s.cash)),
                ("koszt", &crate::zlotowki(s.monthly_cost())),
            ],
        ),
        c.fmt_key(
            l,
            "ui.firm.course",
            &[("kurs", &reason::firm_strategy(c, l, s.strategy))],
        ),
    ];
    // Wynik **z ksiąg**, a nie z prognozy. `None` znaczy „żaden zakład nie ma
    // jeszcze rachunku" i to jest inne zdanie niż „wyszli na zero" — panel musi
    // je rozróżniać, bo zakład produkcyjny nie ma przychodu do dziś (`BC-8`).
    out.push(match s.last_result() {
        Some(m) => c.fmt_key(l, "ui.firm.result", &[("wynik", &crate::zlotowki(m))]),
        None => c.fmt_key(l, "ui.firm.result_unknown", &[]),
    });
    out
}

fn zaklady(c: &Catalog, l: Locale, s: &FirmPanelSnapshot) -> Vec<String> {
    s.sites.iter().map(|r| wiersz_zakladu(c, l, r)).collect()
}

fn wiersz_zakladu(c: &Catalog, l: Locale, r: &SiteRow) -> String {
    let marza = r.last_month.and_then(|m| m.margin_bp()).map_or_else(
        || c.fmt_key(l, "ui.firm.no_measure", &[]),
        reason::procent_bp,
    );
    let mut s = c.fmt_key(
        l,
        "ui.firm.site_row",
        &[
            ("typ", &r.site_type),
            ("dzielnica", &r.district.0.to_string()),
            ("ludzie", &r.headcount.to_string()),
            ("wakaty", &r.vacancies.to_string()),
            ("koszt", &crate::zlotowki(r.fixed_cost)),
            ("marza", &marza),
        ],
    );
    if r.months_in_loss > 0 {
        s.push(' ');
        s.push_str(&c.fmt_key(
            l,
            "ui.firm.site_loss",
            &[("okres", &reason::months(c, l, u32::from(r.months_in_loss)))],
        ));
    }
    s
}

/// Ludzie i drzewo organizacyjne. Drzewo jest płaskie z rozmysłu: firma ma dwa
/// poziomy — właściciel i menedżerowie zakładów — a rysowanie gałęzi tam, gdzie
/// jest jedno rozgałęzienie, kosztuje czytelność i nic nie kupuje.
fn ludzie(c: &Catalog, l: Locale, s: &FirmPanelSnapshot) -> Vec<String> {
    let mut out = Vec::new();
    for r in &s.sites {
        out.push(match &r.manager {
            Some(m) if m.citizen.is_some() => c.fmt_key(
                l,
                "ui.firm.manager",
                &[
                    ("typ", &r.site_type),
                    ("styl", &styl(c, l, m.style)),
                    ("umiejetnosc", &m.skill_mgmt.get().to_string()),
                    ("zaklady", &m.span.to_string()),
                ],
            ),
            // Zastępstwo: polityka została, człowiek odszedł (§5.4). To jest stan,
            // o którym gracz musi wiedzieć — zakład działa gorzej i nikt tego nie
            // zgłosi, jeśli panel tego nie pokaże.
            Some(_) => c.fmt_key(l, "ui.firm.manager_acting", &[("typ", &r.site_type)]),
            None => c.fmt_key(l, "ui.firm.manager_owner", &[("typ", &r.site_type)]),
        });
        for e in s.staff.iter().filter(|e| e.site == r.site) {
            out.push(format!("  {}", wiersz_pracownika(c, l, e)));
        }
    }
    out
}

fn wiersz_pracownika(c: &Catalog, l: Locale, e: &EmployeeRow) -> String {
    c.fmt_key(
        l,
        "ui.firm.employee_row",
        &[
            ("rola", &e.role.0.to_string()),
            ("stawka", &crate::zlotowki(e.wage_month)),
            ("ocena", &e.perf.to_string()),
            ("ostrzezenia", &e.warnings.to_string()),
        ],
    )
}

/// Kurs firmy: strategia, trwająca kampania i **ranking wariantów**, nigdy kwota.
fn kurs(c: &Catalog, l: Locale, s: &FirmPanelSnapshot) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(k) = s.campaign {
        out.push(c.fmt_key(
            l,
            "ui.firm.campaign",
            &[
                ("odpowiedz", &reason::reaction_kind(c, l, k.kind)),
                ("koszt", &reason::procent_bp(i32::from(k.depth_bp))),
            ],
        ));
    }
    match &s.outlook {
        None => out.push(c.fmt_key(l, "ui.firm.no_outlook", &[])),
        Some(o) => out.extend(wiersze_prognozy(c, l, o)),
    }
    out
}

fn wiersze_prognozy(c: &Catalog, l: Locale, o: &OutlookRow) -> Vec<String> {
    let mut out = vec![c.fmt_key(
        l,
        "ui.firm.outlook",
        &[
            ("warianty", &o.variants.len().to_string()),
            (
                "margines",
                &reason::procent_bp(i32::try_from(o.error_margin_bp).unwrap_or(i32::MAX)),
            ),
        ],
    )];
    out.push(
        match o.winner.and_then(|i| o.variants.get(usize::from(i))) {
            Some(a) => c.fmt_key(
                l,
                "ui.firm.outlook_winner",
                &[
                    ("wariant", &wariant(c, l, *a)),
                    ("kierunek", &reason::trend(c, l, o.trend)),
                ],
            ),
            // `None` znaczy „model nie rozstrzygnął", a nie „nic się nie stanie".
            // Różnica jest dla gracza istotna: firma stoi, bo prognoza jest niepewna,
            // a nie dlatego, że wszystko jest w porządku.
            None => c.fmt_key(l, "ui.firm.outlook_undecided", &[]),
        },
    );
    out
}

fn wariant(c: &Catalog, l: Locale, a: StrAction) -> String {
    let klucz = match a {
        StrAction::KeepCourse => "ui.firm.variant.keep",
        StrAction::OpenSite { .. } => "ui.firm.variant.open",
        StrAction::CloseSite { .. } => "ui.firm.variant.close",
        StrAction::RequestVoluntaryClosure => "ui.firm.variant.wind_down",
    };
    c.fmt_key(l, klucz, &[])
}

fn decyzje(c: &Catalog, l: Locale, s: &FirmPanelSnapshot) -> Vec<String> {
    s.decisions
        .iter()
        .rev()
        .take(RECENT_DECISIONS)
        .map(|(t, r)| {
            c.fmt_key(
                l,
                "ui.firm.decision_row",
                &[
                    (
                        "doba",
                        &(t.get() / magnat_core::time::MINUTES_PER_DAY).to_string(),
                    ),
                    ("powod", &reason::describe(c, l, *r)),
                ],
            )
        })
        .collect()
}

fn styl(c: &Catalog, l: Locale, s: magnat_firms::ManagerStyle) -> String {
    let klucz = match s {
        magnat_firms::ManagerStyle::Taskmaster => "ui.manager.Taskmaster",
        magnat_firms::ManagerStyle::Coach => "ui.manager.Coach",
        magnat_firms::ManagerStyle::Bureaucrat => "ui.manager.Bureaucrat",
        magnat_firms::ManagerStyle::Dealmaker => "ui.manager.Dealmaker",
    };
    c.fmt_key(l, klucz, &[])
}

/// Ile obciążeń pokazuje karta. Najnowsze, bo rejestr trzyma wszystko od początku
/// świata, a gracz pyta „co mi właśnie naliczyli", a nie „co mi naliczyli trzy lata
/// temu" — na to drugie odpowiada Kronika (M9).
const RECENT_CHARGES: usize = 12;

/// Wiersze zakładki danin: nazwa, okres, podstawa, stawka, kwota i stan.
///
/// **Podstawa jest w wierszu obowiązkowo** i to jest cała różnica między kartą
/// a paragonem: „VAT 620 zł" nie odpowiada na pytanie gracza, a „VAT od obrotu
/// 12 400 zł po 5 % to 620 zł" odpowiada. Stawka idzie z **migawki należności**,
/// nie z aktualnego kodeksu — obciążenie sprzed pół roku ma tłumaczyć kwotę,
/// która wtedy powstała, a nie kwotę, która powstałaby dziś.
fn daniny(c: &Catalog, l: Locale, charges: &[TaxCharge]) -> Vec<String> {
    let mut wiersze: Vec<&TaxCharge> = charges.iter().collect();
    // Malejąco po chwili naliczenia, remis po identyfikatorze — ten sam rejestr
    // daje ten sam porządek w obu przebiegach (00 §3.2).
    wiersze.sort_by_key(|t| (std::cmp::Reverse(t.assessed_at.get()), t.id.0));
    wiersze
        .into_iter()
        .take(RECENT_CHARGES)
        .map(|t| {
            c.fmt_key(
                l,
                "ui.firm.tax_row",
                &[
                    ("danina", &reason::tax_kind(c, l, t.kind)),
                    ("okres", &okres(c, l, t.period)),
                    ("podstawa", &crate::zlotowki(t.base)),
                    ("stawka", &reason::procent(t.rate_snapshot)),
                    ("kwota", &crate::zlotowki(t.amount)),
                    ("stan", &stan(c, l, t.state)),
                ],
            )
        })
        .collect()
}

fn okres(c: &Catalog, l: Locale, p: FiscalPeriod) -> String {
    match p {
        FiscalPeriod::Day(d) => c.fmt_key(l, "ui.tax.period.day", &[("doba", &d.to_string())]),
        FiscalPeriod::Month(y, m) => c.fmt_key(
            l,
            "ui.tax.period.month",
            &[("rok", &y.to_string()), ("miesiac", &m.to_string())],
        ),
        FiscalPeriod::Year(y) => c.fmt_key(l, "ui.tax.period.year", &[("rok", &y.to_string())]),
    }
}

fn stan(c: &Catalog, l: Locale, s: ChargeState) -> String {
    match s {
        ChargeState::Assessed => c.fmt_key(l, "ui.tax.state.assessed", &[]),
        ChargeState::Settled { .. } => c.fmt_key(l, "ui.tax.state.settled", &[]),
        ChargeState::Overdue { interest, .. } => c.fmt_key(
            l,
            "ui.tax.state.overdue",
            &[("odsetki", &crate::zlotowki(interest))],
        ),
        ChargeState::Abated { why, .. } => c.fmt_key(
            l,
            "ui.tax.state.abated",
            &[("powod", &reason::abate_reason(c, l, why))],
        ),
    }
}
