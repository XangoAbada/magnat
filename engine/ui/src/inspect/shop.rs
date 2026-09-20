//! Karta panelu sklepu (M5e/WP12, PRD §14.3, §5.12 dokumentu podfazy).
//!
//! **Karta nie liczy niczego sama** — tak samo jak karta podróży. Dostaje
//! [`ShopPanelSnapshot`], czyli jedyne wejście interfejsu do gospodarki (`sim/economy`
//! §5.12), i zamienia je w tekst. Jedyna arytmetyka tutaj to formatowanie i porównanie
//! ceny konkurenta z własną — i to drugie liczy `ShelfRow::margin_of`, czyli ta sama
//! funkcja, którą migawka policzyła marżę. Gdyby karta liczyła cokolwiek u siebie,
//! gracz widziałby drugi rachunek obok tego, na którym stoi symulacja (00 §7).
//!
//! **Powody renderuje wyłącznie [`crate::describe`]** (`K-12`): „dlaczego wczoraj
//! potaniało" i „dlaczego Anna nie kupiła u mnie" mają zdania w `reason`, a nie tutaj.
//!
//! **Wejście przychodzi spoza świata.** `ShopPanelSnapshot` jest oderwaną od stanu
//! strukturą, którą wolno trzymać przez klatkę — dlatego nie ma tu `InspectorPanel`:
//! panel sklepu nie dostaje `&World` i dostać go nie może.

use crate::inspect::reason;
use crate::loc::{Catalog, Locale};
use crate::time::TimeControlsWidget;
use magnat_core::{GoodId, PlaceKind, Qty, RejectCause, Tick};
use magnat_economy::{
    CompetitorRef, CompetitorRow, LostSaleTracking, PricePolicy, ShelfRow, ShopPanelSnapshot,
};

/// Ile ostatnich utraconych sprzedaży pokazuje zakładka Klienci.
///
/// Pierścień ma 256 wpisów, ale panel jest odpowiedzią na „dlaczego Anna nie kupiła
/// u mnie **dziś**", a nie dziennikiem — dziesięć najnowszych mieści się na ekranie
/// i nie zasłania histogramu, który mówi o całym tygodniu.
const RECENT_LOST_SALES: usize = 10;

/// Wszystko, czego karta sklepu potrzebuje, a czego nie ma w migawce.
///
/// Rodzaj zakładu i początek okresu rachunku wyników są stanem świata, nie rynku —
/// migawka ich nie niesie, a karta bez nich nie umie napisać nagłówka.
pub struct ShopView<'a> {
    pub snapshot: &'a ShopPanelSnapshot,
    /// Rodzaj zakładu — nagłówek („Sklep spożywczy").
    pub kind: PlaceKind,
    /// Początek okresu rachunku wyników (zwykle początek miesiąca gry).
    pub period_from: Tick,
}

/// Zakładka panelu (§5.12: Półki / Klienci / Konkurencja).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ShopTab {
    #[default]
    Shelves,
    Customers,
    Competition,
}

impl ShopTab {
    pub const ALL: [ShopTab; 3] = [ShopTab::Shelves, ShopTab::Customers, ShopTab::Competition];

    #[must_use]
    pub fn label(self, c: &Catalog, l: Locale) -> String {
        let klucz = match self {
            ShopTab::Shelves => "ui.shop.tab.shelves",
            ShopTab::Customers => "ui.shop.tab.customers",
            ShopTab::Competition => "ui.shop.tab.competition",
        };
        c.fmt_key(l, klucz, &[])
    }
}

/// Karta sklepu — model, z którego powstaje i wydruk tekstowy, i widget.
///
/// Wiersze są gotowymi zdaniami w języku gracza, bo **każde** z nich i tak powstaje
/// z katalogu tekstów: struktura pośrednia niosłaby te same napisy w polach o innych
/// nazwach. Nagłówki sekcji dokłada dopiero render — dzięki temu widget rysuje to samo,
/// co wypisuje złoty test.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ShopCard {
    /// Rodzaj zakładu w języku gracza.
    kind: String,
    site: u32,
    firm: u32,
    /// Chwila migawki, poziom śledzenia i finanse — wspólne dla wszystkich zakładek.
    header: Vec<String>,
    shelves: Vec<String>,
    customers: Vec<String>,
    competition: Vec<String>,
}

impl ShopCard {
    #[must_use]
    pub fn build(c: &Catalog, l: Locale, v: &ShopView<'_>) -> ShopCard {
        let s = v.snapshot;
        ShopCard {
            kind: c.fmt_key(l, &format!("ui.place_kind.{}", v.kind.name()), &[]),
            site: s.site.entity().index(),
            firm: s.firm.entity().index(),
            header: naglowek(c, l, v),
            shelves: polki(c, l, s),
            customers: klienci(c, l, s),
            competition: konkurencja(c, l, s),
        }
    }

    /// Nagłówek wspólny dla zakładek: co to za zakład, z kiedy są dane i jak stoją
    /// finanse. Osobno od zakładek, bo widget rysuje go nad nimi, a nie w każdej.
    #[must_use]
    pub fn render_header(&self, c: &Catalog, l: Locale) -> crate::Rich {
        use std::fmt::Write;
        let mut s = c.fmt_key(
            l,
            "ui.shop.header",
            &[
                ("rodzaj", &self.kind),
                ("nr", &self.site.to_string()),
                ("firma", &self.firm.to_string()),
            ],
        );
        s.push('\n');
        for w in &self.header {
            let _ = writeln!(s, "  {w}");
        }
        crate::rich::lines_titled(&s)
    }

    /// Cała treść panelu jako tekst — wszystkie trzy zakładki pod sobą.
    /// To jest forma testowalna w CI bez GPU i to ona jest złotym wydrukiem.
    #[must_use]
    pub fn render_text(&self, c: &Catalog, l: Locale) -> String {
        use crate::RichExt;
        let mut s = self.render_header(c, l).to_plain();
        for t in ShopTab::ALL {
            s.push_str(&self.render_tab(c, l, t).to_plain());
        }
        s
    }

    /// Jedna zakładka — tego używa widget.
    #[must_use]
    pub fn render_tab(&self, c: &Catalog, l: Locale, tab: ShopTab) -> crate::Rich {
        use std::fmt::Write;
        let wiersze = match tab {
            ShopTab::Shelves => &self.shelves,
            ShopTab::Customers => &self.customers,
            ShopTab::Competition => &self.competition,
        };
        let mut s = format!("{}:\n", tab.label(c, l));
        if wiersze.is_empty() {
            let _ = writeln!(s, "  {}", c.fmt_key(l, "ui.shop.empty", &[]));
        }
        for w in wiersze {
            let _ = writeln!(s, "  {w}");
        }
        crate::rich::lines_titled(&s)
    }
}

// ── budowa wierszy ──────────────────────────────────────────────────────────────

fn naglowek(c: &Catalog, l: Locale, v: &ShopView<'_>) -> Vec<String> {
    let s = v.snapshot;
    let f = &s.finance;
    let mut out = vec![
        c.fmt_key(
            l,
            "ui.shop.at",
            &[
                ("data", &data(c, l, s.at)),
                ("godzina", &godzina(c, l, s.at)),
            ],
        ),
        c.fmt_key(
            l,
            "ui.shop.tracking",
            &[("poziom", &sledzenie(c, l, s.tracking))],
        ),
        c.fmt_key(
            l,
            "ui.shop.fin.period",
            &[("od", &data(c, l, v.period_from))],
        ),
        c.fmt_key(
            l,
            "ui.shop.fin.result",
            &[
                ("przychod", &crate::zlotowki(f.statement.revenue)),
                ("koszt", &crate::zlotowki(f.statement.cogs)),
                ("marza", &crate::zlotowki(f.statement.gross_margin())),
            ],
        ),
        c.fmt_key(
            l,
            "ui.shop.fin.bottom",
            &[
                ("odpisy", &crate::zlotowki(f.statement.write_off)),
                // Reklama i składka są w wyniku od M10d, więc muszą być też
                // w wierszu: liczba, która nie zgadza się z pozycjami nad nią,
                // wygląda dla gracza jak błąd programu.
                ("reklama", &crate::zlotowki(f.statement.marketing)),
                ("ubezpieczenie", &crate::zlotowki(f.statement.insurance)),
                ("wynik", &crate::zlotowki(f.statement.net_result())),
            ],
        ),
        c.fmt_key(
            l,
            "ui.shop.fin.balance",
            &[
                ("suma", &crate::zlotowki(f.balance.assets)),
                ("domkniecie", &crate::zlotowki(f.balance.imbalance())),
            ],
        ),
        c.fmt_key(
            l,
            "ui.shop.fin.cash",
            &[
                ("operacyjne", &crate::zlotowki(f.cash.operating)),
                ("inwestycyjne", &crate::zlotowki(f.cash.investing)),
                ("finansowe", &crate::zlotowki(f.cash.financing)),
                ("netto", &crate::zlotowki(f.cash.net)),
            ],
        ),
    ];
    // Liczba obcięta oknem dziennika wygląda jak liczba prawdziwa — panel **musi**
    // to powiedzieć wprost (korekta planu M5e, `CashFlow::complete`).
    if !f.cash.complete {
        out.push(c.fmt_key(l, "ui.shop.fin.cash_partial", &[]));
    }
    out.push(c.fmt_key(
        l,
        "ui.shop.fin.inventory",
        &[("zapas", &crate::zlotowki(f.inventory_value))],
    ));
    if let Some(loan) = f.loan {
        out.push(c.fmt_key(l, "ui.shop.fin.loan", &[("nr", &loan.0.to_string())]));
    }
    out
}

fn polki(c: &Catalog, l: Locale, s: &ShopPanelSnapshot) -> Vec<String> {
    let mut out = Vec::with_capacity(s.shelves.len() * 2 + s.reprices.len() + 1);
    for r in &s.shelves {
        out.push(c.fmt_key(
            l,
            "ui.shop.shelf",
            &[
                ("towar", &towar(c, l, s, r.good)),
                ("cena", &crate::zlotowki(r.price)),
                ("koszt", &crate::zlotowki(r.unit_cost)),
                ("marza", &reason::procent_bp(r.margin_bp)),
                ("polka", &sztuki(r.on_shelf)),
                ("zaplecze", &sztuki(r.backroom)),
                ("pokrycie", &pokrycie(c, l, r.days_of_cover)),
                ("obrot", &sztuki(r.turnover_7d)),
            ],
        ));
        let mut druga = c.fmt_key(
            l,
            "ui.shop.shelf.policy",
            &[
                ("polityka", &polityka(c, l, r.policy)),
                (
                    "tryb",
                    &c.fmt_key(
                        l,
                        if r.delegated {
                            "ui.shop.delegated"
                        } else {
                            "ui.shop.manual"
                        },
                        &[],
                    ),
                ),
            ],
        );
        if let Some(e) = r.expires_at {
            // `expires_at` jest chwilą, o której towar traci termin — karta pokazuje,
            // ile zostało licząc od chwili migawki, bo tak to czyta gracz.
            let dni = e.get().saturating_sub(s.at.get()) / magnat_core::time::MINUTES_PER_DAY;
            druga.push_str(" · ");
            druga.push_str(&c.fmt_key(
                l,
                "ui.shop.shelf.expires",
                &[("termin", &reason::days(c, l, dni as u32))],
            ));
        }
        out.push(format!("  {druga}"));
    }
    if !s.reprices.is_empty() {
        out.push(c.fmt_key(l, "ui.shop.reprices", &[]));
        for r in &s.reprices {
            out.push(format!("  {}", reason::describe(c, l, *r)));
        }
    }
    out
}

fn klienci(c: &Catalog, l: Locale, s: &ShopPanelSnapshot) -> Vec<String> {
    // Zakład nieśledzony nie ma **żadnych** danych o klientach — pusta tabela udawałaby
    // zero klientów, a to co innego niż „nie wiemy" (§5.4, `W-7`).
    if s.tracking == LostSaleTracking::None {
        return vec![c.fmt_key(l, "ui.shop.untracked", &[])];
    }
    let mut out = Vec::new();
    let ciag: Vec<String> = s.customers.daily.iter().map(u32::to_string).collect();
    out.push(c.fmt_key(
        l,
        "ui.shop.daily",
        &[
            ("ciag", &ciag.join(" · ")),
            ("razem", &s.customers.total.to_string()),
        ],
    ));

    out.push(c.fmt_key(l, "ui.shop.by_district", &[]));
    for (d, n) in &s.customers.by_district {
        out.push(format!(
            "  {}",
            c.fmt_key(
                l,
                "ui.shop.count",
                &[
                    (
                        "co",
                        &c.fmt_key(l, "ui.shop.district", &[("nr", &d.0.to_string())])
                    ),
                    ("ile", &n.to_string()),
                ],
            )
        ));
    }
    out.push(c.fmt_key(l, "ui.shop.by_class", &[]));
    for (k, n) in &s.customers.by_class {
        out.push(format!(
            "  {}",
            policz(c, l, &reason::social_class(c, l, *k), *n)
        ));
    }
    out.push(c.fmt_key(l, "ui.shop.by_driver", &[]));
    for (u, n) in &s.customers.by_driver {
        out.push(format!(
            "  {}",
            policz(c, l, &reason::utility_term(c, l, *u), *n)
        ));
    }

    out.push(c.fmt_key(l, "ui.shop.lost", &[]));
    for cause in RejectCause::ALL {
        let n = s.lost_sales.histogram.by_cause[cause.as_index()];
        out.push(format!(
            "  {}",
            policz(c, l, &reason::reject_cause(c, l, *cause), n)
        ));
    }
    if !s.lost_sales.recent.is_empty() {
        out.push(c.fmt_key(l, "ui.shop.lost.recent", &[]));
        // Pierścień idzie od najstarszego do najnowszego — panel pokazuje odwrotnie.
        for e in s.lost_sales.recent.iter().rev().take(RECENT_LOST_SALES) {
            let mut w = c.fmt_key(
                l,
                "ui.shop.lost.row",
                &[
                    ("towar", &towar(c, l, s, e.good)),
                    ("powod", &reason::reject_cause(c, l, e.cause)),
                ],
            );
            if let Some(gdzie) = e.went_to {
                w.push_str(" · ");
                w.push_str(&c.fmt_key(
                    l,
                    "ui.shop.lost.went_to",
                    &[("nr", &gdzie.entity().index().to_string())],
                ));
            }
            out.push(format!("  {w}"));
        }
    }
    out
}

fn konkurencja(c: &Catalog, l: Locale, s: &ShopPanelSnapshot) -> Vec<String> {
    let mut out = Vec::new();
    for k in &s.competition {
        out.push(c.fmt_key(
            l,
            "ui.shop.competitor",
            &[
                ("nr", &k.site.entity().index().to_string()),
                ("metry", &k.distance_m.to_string()),
                // Wiek obrazu **wprost**: gracz ma widzieć, że patrzy na dane sprzed
                // N dni, tak samo jak AI (§5.12, §6.3).
                ("wiek", &reason::days(c, l, u32::from(k.observed_age_days))),
            ],
        ));
        for w in ceny_konkurenta(c, l, s, k) {
            out.push(format!("  {w}"));
        }
    }
    out
}

fn ceny_konkurenta(
    c: &Catalog,
    l: Locale,
    s: &ShopPanelSnapshot,
    k: &CompetitorRow,
) -> Vec<String> {
    k.prices
        .iter()
        .map(|(g, cena)| {
            let nasza = s.shelves.iter().find(|r| r.good == *g).map(|r| r.price);
            match nasza {
                // Porównanie liczy **ta sama** funkcja, którą migawka policzyła marżę:
                // `(ich − nasza) / nasza` w punktach bazowych. Drugi wzór tutaj
                // rozjechałby się z pierwszym przy pierwszej zmianie zaokrąglenia.
                Some(n) if n.get() > 0 => c.fmt_key(
                    l,
                    "ui.shop.comp_price",
                    &[
                        ("towar", &towar(c, l, s, *g)),
                        ("cena", &crate::zlotowki(*cena)),
                        (
                            "roznica",
                            &reason::procent_bp(ShelfRow::margin_of(*cena, n)),
                        ),
                    ],
                ),
                _ => c.fmt_key(
                    l,
                    "ui.shop.comp_price_only",
                    &[
                        ("towar", &towar(c, l, s, *g)),
                        ("cena", &crate::zlotowki(*cena)),
                    ],
                ),
            }
        })
        .collect()
}

// ── pomocnicze ──────────────────────────────────────────────────────────────────

fn policz(c: &Catalog, l: Locale, co: &str, ile: u32) -> String {
    c.fmt_key(l, "ui.shop.count", &[("co", co), ("ile", &ile.to_string())])
}

/// Nazwa towaru. Klucze towarów pochodzą z `data/economy/retail.ron`, więc towar
/// dołożony moddem nie ma wpisu w katalogu tekstów — wtedy lepiej pokazać jego surowy
/// klucz niż wywrócić interfejs (ta sama zasada co przy klasie pojazdu). Pusty klucz
/// znaczy „katalog towarów go nie zna", i wtedy zostaje sam identyfikator.
fn towar(c: &Catalog, l: Locale, s: &ShopPanelSnapshot, g: GoodId) -> String {
    let key = s.good_key(g);
    if key.is_empty() {
        return format!("#{}", g.0);
    }
    c.key(&format!("ui.good.{key}"))
        .map_or_else(|| key.to_string(), |k| c.text(l, k).to_string())
}

/// Milisztuki jako sztuki. Karta nie udaje dokładności tysięcznej części chleba.
fn sztuki(q: Qty) -> String {
    (q.get() / 1_000).to_string()
}

/// Dni pokrycia. `u16::MAX` znaczy „nic się nie sprzedaje", czyli **brak odpowiedzi** —
/// wypisanie 65535 dni byłoby liczbą udającą pomiar.
fn pokrycie(c: &Catalog, l: Locale, dni: u16) -> String {
    if dni == u16::MAX {
        c.fmt_key(l, "ui.shop.no_data", &[])
    } else {
        reason::days(c, l, u32::from(dni))
    }
}

fn sledzenie(c: &Catalog, l: Locale, t: LostSaleTracking) -> String {
    let klucz = match t {
        LostSaleTracking::None => "ui.shop.tracking.none",
        LostSaleTracking::Histogram => "ui.shop.tracking.histogram",
        LostSaleTracking::Full => "ui.shop.tracking.full",
    };
    c.fmt_key(l, klucz, &[])
}

/// Polityka cenowa w języku gracza. Wyczerpujący `match` bez ramienia `_`, z tego
/// samego powodu co przy [`crate::infeasible`]: wariant dopisany w `sim/economy`
/// nie skompiluje interfejsu, dopóki nikt nie napisze, jak go pokazać graczowi.
fn polityka(c: &Catalog, l: Locale, p: PricePolicy) -> String {
    match p {
        PricePolicy::Fixed { price } => c.fmt_key(
            l,
            "ui.shop.policy.Fixed",
            &[("cena", &crate::zlotowki(price))],
        ),
        PricePolicy::Markup { target_margin_bp } => c.fmt_key(
            l,
            "ui.shop.policy.Markup",
            &[("marza", &reason::procent_bp(target_margin_bp))],
        ),
        PricePolicy::MatchCompetitor {
            delta_bp,
            radius_m,
            reference,
        } => c.fmt_key(
            l,
            "ui.shop.policy.MatchCompetitor",
            &[
                ("roznica", &reason::procent_bp(delta_bp)),
                ("promien", &radius_m.to_string()),
                ("wzorzec", &wzorzec(c, l, reference)),
            ],
        ),
        PricePolicy::Dynamic {
            target_margin_bp,
            floor_margin_bp,
            ceil_margin_bp,
        } => c.fmt_key(
            l,
            "ui.shop.policy.Dynamic",
            &[
                ("cel", &reason::procent_bp(target_margin_bp)),
                ("dol", &reason::procent_bp(floor_margin_bp)),
                ("gora", &reason::procent_bp(ceil_margin_bp)),
            ],
        ),
    }
}

fn wzorzec(c: &Catalog, l: Locale, r: CompetitorRef) -> String {
    match r {
        CompetitorRef::Cheapest => c.fmt_key(l, "ui.shop.ref.cheapest", &[]),
        CompetitorRef::Median => c.fmt_key(l, "ui.shop.ref.median", &[]),
        CompetitorRef::Named(s) => c.fmt_key(
            l,
            "ui.shop.ref.named",
            &[("nr", &s.entity().index().to_string())],
        ),
    }
}

/// Data i godzina migawki. Kalendarz jest ten sam, co w pasku czasu (`K-1`) —
/// druga implementacja rozjechałaby się z nim przy pierwszej zmianie formatu.
fn data(c: &Catalog, l: Locale, t: Tick) -> String {
    TimeControlsWidget::new(t).date_label(c, l)
}

fn godzina(c: &Catalog, l: Locale, t: Tick) -> String {
    TimeControlsWidget::new(t).clock_label(c, l)
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::Money;

    #[test]
    fn sztuki_licza_sie_z_milisztuk() {
        assert_eq!(sztuki(Qty(12_000)), "12");
        assert_eq!(sztuki(Qty(999)), "0");
    }

    #[test]
    fn brak_sprzedazy_nie_udaje_pokrycia() {
        let c = Catalog::load().expect("data/locale/");
        assert_eq!(pokrycie(&c, Locale::Pl, u16::MAX), "—");
        assert_eq!(pokrycie(&c, Locale::Pl, 3), "3 dni");
    }

    #[test]
    fn kazda_polityka_cenowa_ma_tekst_w_obu_jezykach() {
        let c = Catalog::load().expect("data/locale/");
        let site = magnat_core::SiteId(magnat_core::Entity::new(7, std::num::NonZeroU32::MIN));
        let wszystkie = [
            PricePolicy::Fixed { price: Money(349) },
            PricePolicy::Markup {
                target_margin_bp: 2_800,
            },
            PricePolicy::MatchCompetitor {
                delta_bp: -200,
                radius_m: 3_000,
                reference: CompetitorRef::Cheapest,
            },
            PricePolicy::MatchCompetitor {
                delta_bp: 0,
                radius_m: 1_200,
                reference: CompetitorRef::Median,
            },
            PricePolicy::MatchCompetitor {
                delta_bp: 100,
                radius_m: 800,
                reference: CompetitorRef::Named(site),
            },
            PricePolicy::Dynamic {
                target_margin_bp: 2_800,
                floor_margin_bp: 800,
                ceil_margin_bp: 6_000,
            },
        ];
        for p in wszystkie {
            for l in Locale::ALL {
                let t = polityka(&c, l, p);
                assert!(!t.is_empty(), "{p:?} w {} jest puste", l.code());
                assert!(!t.contains('{'), "{p:?} w {}: `{t}`", l.code());
            }
        }
    }

    #[test]
    fn kazdy_rodzaj_miejsca_ma_nazwe_w_obu_jezykach() {
        let c = Catalog::load().expect("data/locale/");
        for k in PlaceKind::ALL {
            for l in Locale::ALL {
                assert!(
                    !c.fmt_key(l, &format!("ui.place_kind.{}", k.name()), &[])
                        .is_empty(),
                    "{k:?}"
                );
            }
        }
    }
}
