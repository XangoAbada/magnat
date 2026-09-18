//! Karta panelu łańcucha dostaw (WP13, PRD §14.3 i §14.4).
//!
//! **Karta nie liczy niczego sama** — tak samo jak karta sklepu i karta podróży.
//! Dostaje [`SupplyGraphView`] i [`BatchTrace`], czyli gotowe liczby z `sim/supply`,
//! i zamienia je w tekst. Gdyby liczyła cokolwiek u siebie, gracz widziałby drugi
//! rachunek obok tego, na którym stoi symulacja (00 §7).
//!
//! Trzy zakładki, trzy pytania z PRD:
//! - **Dostawcy** (§14.3): kto do mnie wozi i czym, z oznaczeniem ryzyka jednego
//!   dostawcy — czerwona krawędź z dokumentu fazy;
//! - **Pokrycie** (§14.3): na ile godzin starczy zapasu i na którym szczeblu drabiny
//!   niedoboru stoi zakład;
//! - **Ślad** (§14.4): „od pola do półki" — oś czasu partii z masą, jakością
//!   i kosztem narastającym.
//!
//! **Czego ta karta nigdy nie zrobi.** Ślad urwany mówi „urwany" i mówi dlaczego
//! ([`TraceOrigin`]). Import kończy się na granicy, partia odtworzona z agregatu
//! makro kończy się na dzielnicy, a partia nieśledzona nie ma historii w ogóle —
//! i to są trzy różne zdania, nie jedno. Gracz, który raz przyłapie panel na
//! zmyślaniu, przestanie mu wierzyć także tam, gdzie panel ma rację (M6 §6.4.2).

use crate::loc::{Catalog, Locale};
use crate::time::TimeControlsWidget;
use magnat_core::Tick;
use magnat_supply::batch::TraceKind;
use magnat_supply::{BatchTrace, Catalog as GoodCatalog, SupplyGraphView, TraceOrigin};

/// Ile etapów śladu pokazuje zakładka. Twardy limit głębokości to szesnaście
/// (`BatchOrigin::MAX_DEPTH`), ale etapów jest więcej niż przetworzeń — każdy przewóz
/// dokłada załadunek i rozładunek — więc panel pokazuje ostatnie trzydzieści dwa
/// i mówi, ile obciął.
const MAX_ETAPOW: usize = 32;

/// Zakładka panelu.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SupplyTab {
    #[default]
    Suppliers,
    Coverage,
    Trace,
}

impl SupplyTab {
    pub const ALL: [SupplyTab; 3] = [SupplyTab::Suppliers, SupplyTab::Coverage, SupplyTab::Trace];

    #[must_use]
    pub fn label(self, c: &Catalog, l: Locale) -> String {
        let klucz = match self {
            SupplyTab::Suppliers => "ui.supply.tab.suppliers",
            SupplyTab::Coverage => "ui.supply.tab.coverage",
            SupplyTab::Trace => "ui.supply.tab.trace",
        };
        c.fmt_key(l, klucz, &[])
    }
}

/// Wszystko, czego karta potrzebuje, a czego nie ma w widoku grafu.
pub struct SupplyView<'a> {
    pub graph: &'a SupplyGraphView,
    /// Ślad partii wskazanej w trybie „śledź partię". `None` = gracz niczego nie wskazał.
    pub trace: Option<&'a BatchTrace>,
    /// Katalog towarów — panel mówi kluczami, a nazwy biorą się z `ui.good.<klucz>`
    /// (`AD-3`: `Good::name` nie powstaje, bo `sim/*` nie zależy od UI).
    pub goods: &'a GoodCatalog,
    pub at: Tick,
}

/// Karta łańcucha — model, z którego powstaje i wydruk tekstowy, i widget.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SupplyCard {
    firm: u32,
    sites: usize,
    header: Vec<String>,
    suppliers: Vec<String>,
    coverage: Vec<String>,
    trace: Vec<String>,
}

impl SupplyCard {
    #[must_use]
    pub fn build(c: &Catalog, l: Locale, v: &SupplyView<'_>) -> SupplyCard {
        SupplyCard {
            firm: v.graph.firm.entity().index(),
            sites: v.graph.sites.len(),
            header: naglowek(c, l, v),
            suppliers: dostawcy(c, l, v),
            coverage: pokrycie(c, l, v),
            trace: slad(c, l, v),
        }
    }

    #[must_use]
    pub fn render_header(&self, c: &Catalog, l: Locale) -> crate::Rich {
        use std::fmt::Write;
        let mut s = c.fmt_key(
            l,
            "ui.supply.header",
            &[
                ("firma", &self.firm.to_string()),
                ("zaklady", &self.sites.to_string()),
            ],
        );
        s.push('\n');
        for w in &self.header {
            let _ = writeln!(s, "  {w}");
        }
        crate::rich::lines_titled(&s)
    }

    /// Cała treść panelu jako tekst — to jest forma testowalna w CI bez GPU
    /// i to ona jest złotym wydrukiem.
    #[must_use]
    pub fn render_text(&self, c: &Catalog, l: Locale) -> String {
        use crate::RichExt;
        let mut s = self.render_header(c, l).to_plain();
        for t in SupplyTab::ALL {
            s.push_str(&self.render_tab(c, l, t).to_plain());
        }
        s
    }

    #[must_use]
    pub fn render_tab(&self, c: &Catalog, l: Locale, tab: SupplyTab) -> crate::Rich {
        use std::fmt::Write;
        let wiersze = match tab {
            SupplyTab::Suppliers => &self.suppliers,
            SupplyTab::Coverage => &self.coverage,
            SupplyTab::Trace => &self.trace,
        };
        let mut s = format!("{}:\n", tab.label(c, l));
        if wiersze.is_empty() {
            let _ = writeln!(s, "  {}", c.fmt_key(l, "ui.supply.empty", &[]));
        }
        for w in wiersze {
            let _ = writeln!(s, "  {w}");
        }
        crate::rich::lines_titled(&s)
    }
}

// ── budowa wierszy ──────────────────────────────────────────────────────────────

fn naglowek(c: &Catalog, l: Locale, v: &SupplyView<'_>) -> Vec<String> {
    let (kupno, sprzedaz) = v.graph.contracts;
    let ryzykownych = v.graph.inbound.iter().filter(|e| e.sole_source).count();
    let mut out = vec![
        c.fmt_key(
            l,
            "ui.supply.at",
            &[
                ("data", &data(c, l, v.at)),
                ("godzina", &godzina(c, l, v.at)),
            ],
        ),
        c.fmt_key(
            l,
            "ui.supply.contracts",
            &[
                ("kupno", &kupno.to_string()),
                ("sprzedaz", &sprzedaz.to_string()),
            ],
        ),
    ];
    if ryzykownych > 0 {
        out.push(c.fmt_key(
            l,
            "ui.supply.sole_source_warning",
            &[("ile", &ryzykownych.to_string())],
        ));
    }
    out
}

fn dostawcy(c: &Catalog, l: Locale, v: &SupplyView<'_>) -> Vec<String> {
    let mut out = Vec::new();
    for e in &v.graph.inbound {
        let wiersz = c.fmt_key(
            l,
            "ui.supply.inbound",
            &[
                ("zaklad", &e.from.entity().index().to_string()),
                ("towar", &towar(c, l, v.goods, e.good)),
                ("masa", &kilogramy(e.mass)),
            ],
        );
        // Ryzyko jest **zdaniem**, a nie kolorem: klient graficzny może dołożyć czerwień,
        // ale wydruk tekstowy i złoty test muszą je widzieć tak samo.
        out.push(if e.sole_source {
            format!("{wiersz} — {}", c.fmt_key(l, "ui.supply.sole_source", &[]))
        } else {
            wiersz
        });
    }
    for e in &v.graph.outbound {
        out.push(c.fmt_key(
            l,
            "ui.supply.outbound",
            &[
                ("zaklad", &e.to.entity().index().to_string()),
                ("towar", &towar(c, l, v.goods, e.good)),
                ("masa", &kilogramy(e.mass)),
            ],
        ));
    }
    out
}

fn pokrycie(c: &Catalog, l: Locale, v: &SupplyView<'_>) -> Vec<String> {
    v.graph
        .coverage
        .iter()
        .map(|r| {
            let godziny = match r.hours {
                Some(h) => c.fmt_key(l, "ui.supply.hours", &[("ile", &h.to_string())]),
                // Zakład, który tego nie zużywa, nie ma pokrycia „na zawsze" —
                // on po prostu nie odpowiada na to pytanie.
                None => c.fmt_key(l, "ui.supply.hours_unknown", &[]),
            };
            c.fmt_key(
                l,
                "ui.supply.coverage",
                &[
                    ("zaklad", &r.site.entity().index().to_string()),
                    ("towar", &towar(c, l, v.goods, r.good)),
                    ("zapas", &kilogramy(r.stock)),
                    ("pokrycie", &godziny),
                    ("szczebel", &szczebel(c, l, r.stage)),
                ],
            )
        })
        .collect()
}

fn slad(c: &Catalog, l: Locale, v: &SupplyView<'_>) -> Vec<String> {
    let Some(t) = v.trace else {
        return Vec::new();
    };
    let mut out = vec![c.fmt_key(
        l,
        "ui.supply.trace.head",
        &[
            ("towar", &towar(c, l, v.goods, t.good)),
            ("etapy", &t.stages.len().to_string()),
            ("glebokosc", &t.depth.to_string()),
        ],
    )];
    out.push(pochodzenie(c, l, t.origin));
    let obciete = t.stages.len().saturating_sub(MAX_ETAPOW);
    if obciete > 0 {
        out.push(c.fmt_key(
            l,
            "ui.supply.trace.truncated",
            &[("ile", &obciete.to_string())],
        ));
    }
    for s in t.stages.iter().skip(obciete) {
        out.push(c.fmt_key(
            l,
            "ui.supply.trace.stage",
            &[
                ("data", &data(c, l, Tick(s.at.0))),
                ("godzina", &godzina(c, l, Tick(s.at.0))),
                ("etap", &etap(c, l, s.kind)),
                (
                    "miejsce",
                    &s.site.map_or_else(
                        || c.fmt_key(l, "ui.supply.trace.nowhere", &[]),
                        |x| x.entity().index().to_string(),
                    ),
                ),
                ("masa", &kilogramy(s.mass)),
                ("jakosc", &s.quality.get().to_string()),
                ("koszt", &crate::zlotowki(s.cost_cumulative)),
            ],
        ));
    }
    out
}

/// Skąd ta partia jest — i **czy w ogóle wiadomo**.
fn pochodzenie(c: &Catalog, l: Locale, o: TraceOrigin) -> String {
    match o {
        TraceOrigin::Deposit(d) => {
            c.fmt_key(l, "ui.supply.origin.deposit", &[("nr", &d.0.to_string())])
        }
        TraceOrigin::Imported => c.fmt_key(l, "ui.supply.origin.imported", &[]),
        TraceOrigin::InitialStock => c.fmt_key(l, "ui.supply.origin.initial", &[]),
        TraceOrigin::NotTraced => c.fmt_key(l, "ui.supply.origin.not_traced", &[]),
    }
}

/// Nazwa etapu. `match` bez ramienia `_` z rozmysłu — dołożenie wariantu
/// [`TraceKind`] ma łamać kompilację, a nie po cichu pokazywać pustą oś czasu
/// (ta sama zasada co przy `DecisionReason`, `K-12`).
fn etap(c: &Catalog, l: Locale, k: TraceKind) -> String {
    let klucz = match k {
        TraceKind::Produced => "ui.supply.stage.produced",
        TraceKind::Stored => "ui.supply.stage.stored",
        TraceKind::Loaded => "ui.supply.stage.loaded",
        TraceKind::Departed => "ui.supply.stage.departed",
        TraceKind::Arrived => "ui.supply.stage.arrived",
        TraceKind::Unloaded => "ui.supply.stage.unloaded",
        TraceKind::Shelved => "ui.supply.stage.shelved",
        TraceKind::Consumed => "ui.supply.stage.consumed",
        TraceKind::Sold => "ui.supply.stage.sold",
        TraceKind::Split => "ui.supply.stage.split",
        TraceKind::Merged => "ui.supply.stage.merged",
        TraceKind::Lost(kind) => {
            return c.fmt_key(
                l,
                "ui.supply.stage.lost",
                &[(
                    "powod",
                    &c.fmt_key(l, &format!("ui.loss.{}", kind.name()), &[]),
                )],
            )
        }
    };
    c.fmt_key(l, klucz, &[])
}

fn szczebel(c: &Catalog, l: Locale, s: magnat_supply::ShortageStage) -> String {
    c.fmt_key(l, &format!("ui.shortage.{}", s.kind().name()), &[])
}

/// Nazwa towaru w języku gracza. Klucz, nie pole — `AD-3`.
fn towar(c: &Catalog, l: Locale, goods: &GoodCatalog, g: magnat_core::GoodId) -> String {
    let klucz = goods.key_of(g);
    // **Miękkie szukanie, a nie `fmt_key`.** Katalog M6 ma ~400 towarów, a nazwę
    // w `data/locale/` ma dziś kilkanaście — reszta to surowce i półprodukty, których
    // gracz nie widzi w sklepie. `fmt_key` panikuje przy braku klucza i ma panikować
    // (brak napisu jest błędem danych), ale **nie tutaj**: panel łańcucha pokazuje
    // rudę żelaza i olej smarowy, więc panika oznaczałaby, że nie da się go otworzyć.
    // Klucz techniczny jest gorszy od nazwy i lepszy od pustki: z pustki gracz nie
    // dowie się, czego brakuje, a z klucza owszem — i wie o tym też tłumacz.
    match c.key(&format!("ui.good.{klucz}")) {
        Some(k) => c.text(l, k).to_string(),
        None => klucz.to_string(),
    }
}

/// Masa w kilogramach z dwoma miejscami — gramy są jednostką bilansu, nie panelu.
fn kilogramy(m: magnat_core::Mass) -> String {
    let g = m.0;
    let znak = if g < 0 { "-" } else { "" };
    let a = g.unsigned_abs();
    format!("{znak}{}.{:02}", a / 1_000, (a % 1_000) / 10)
}

fn data(c: &Catalog, l: Locale, t: Tick) -> String {
    TimeControlsWidget::new(t).date_label(c, l)
}

fn godzina(c: &Catalog, l: Locale, t: Tick) -> String {
    TimeControlsWidget::new(t).clock_label(c, l)
}
