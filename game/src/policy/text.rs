//! Postać tekstowa polityki: zapis i odczyt (M9d §5.6 „Gramatyka postaci tekstowej", WP8).
//!
//! # Po co tekst, skoro kanoniczne jest drzewo
//!
//! Do trzech rzeczy i żadna z nich nie jest wykonaniem: **pokazania** graczowi, co ta
//! polityka robi, **podzielenia się** nią z kimś innym i **zapisania** jej w `data/`.
//! Parser nie wykonuje się w pętli — w pętli wykonuje się AST.
//!
//! # Słowa kluczowe są lokalizacją, a nie literałem
//!
//! Tekst polityki widzi gracz, więc podlega tej samej regule co każdy inny napis
//! (CLAUDE.md): każde słowo gramatyki ma klucz `ui.policy.*` w `pl.ron` **i** `en.ron`.
//! Zapis idzie w języku, który gracz ma ustawiony; odczyt przyjmuje **oba**, bo polityka
//! przysłana przez kogoś innego nie ma obowiązku być w tym samym języku.
//!
//! Wyjątki od lokalizacji są trzy i wszystkie są identyfikatorami, a nie słowami:
//! klucz towaru (`"food_bread"` — kontrakt zapisu gry, 00 §5), numer roli i receptury
//! (`role#4`, `recipe#7`) oraz numer komunikatu polityki (`msg#1`).
//!
//! ponytail: sufit nazwany — role i receptury jadą numerem, bo dziedziny `Hr`
//! i `Production` nie mają dziś wykonawcy (`DomainNotAvailable`), więc nikt tych
//! polityk nie przypnie. Klucze tekstowe wejdą tu razem z wykonawcą, a nie przed nim.
//!
//! # Granica parsera
//!
//! Parser czyta to, co pisze serializator: płaski łańcuch porównań, sloty o głębokości
//! ≤ 3, jeden spójnik na regułę. To jest **ten sam** zakres, który umie pokazać
//! formularz ([`super::slot`]) — bo tekst jest rzutem formularza, a nie drugim
//! sposobem opisania języka. Tekst spoza tego zakresu wraca [`TextError`], a nie
//! połową polityki.

use magnat_core::{ActionKind, Entity, GoodId, PriceBasis};
use magnat_policy::{
    Bp, Cadence, CmpOp, Metric, Policy, PolicyDomain, PolicyScope, Severity, Value,
};
use magnat_ui::{Catalog, Locale};
use std::collections::BTreeMap;

use super::slot::{ActionDraft, Base, Clause, Join, RuleDraft, Slot};

/// Nazwy towarów w obie strony. Klucz tekstowy, nie indeks — `GoodId` nadaje się
/// przy ładowaniu katalogu i przesuwa przy każdym nowym towarze (00 §5).
#[derive(Clone, Debug, Default)]
pub struct GoodKeys {
    po_id: BTreeMap<u16, String>,
    po_kluczu: BTreeMap<String, GoodId>,
}

impl GoodKeys {
    #[must_use]
    pub fn new(pary: impl IntoIterator<Item = (GoodId, String)>) -> GoodKeys {
        let mut k = GoodKeys::default();
        for (id, key) in pary {
            k.po_id.insert(id.get(), key.clone());
            k.po_kluczu.insert(key, id);
        }
        k
    }

    fn key(&self, g: GoodId) -> Option<&str> {
        self.po_id.get(&g.get()).map(String::as_str)
    }

    fn id(&self, key: &str) -> Option<GoodId> {
        self.po_kluczu.get(key).copied()
    }
}

/// Czego parser nie zrozumiał.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TextError {
    /// Spodziewano się słowa, jest co innego (albo koniec tekstu).
    Expected { what: &'static str, at: usize },
    /// Słowo, którego nie ma w gramatyce żadnego z języków.
    Unknown { word: String, at: usize },
    /// Towar o tym kluczu nie istnieje w katalogu.
    UnknownGood(String),
    /// Brutto zmieszane z netto (`K-7`). Import jest jedyną drogą, którą taka polityka
    /// może w ogóle powstać — formularz jej nie złoży.
    PriceBasisMismatch { lhs: PriceBasis, rhs: PriceBasis },
    /// Liczba poza zakresem albo bez jednostki, której wymaga to miejsce.
    BadNumber { at: usize },
}

impl std::fmt::Display for TextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TextError::Expected { what, at } => write!(f, "słowo {at}: oczekiwano {what}"),
            TextError::Unknown { word, at } => write!(f, "słowo {at}: nieznane „{word}”"),
            TextError::UnknownGood(k) => write!(f, "nie ma towaru o kluczu {k}"),
            TextError::PriceBasisMismatch { lhs, rhs } => {
                write!(f, "podstawy ceny: {lhs:?} wobec {rhs:?}")
            }
            TextError::BadNumber { at } => write!(f, "słowo {at}: zła liczba"),
        }
    }
}

impl std::error::Error for TextError {}

// ── słownik gramatyki ────────────────────────────────────────────────────────────

/// Wszystkie klucze gramatyki. Jedna lista, bo zapis i odczyt muszą używać tej samej —
/// dwie rozjechałyby się przy pierwszym nowym słowie.
const KW: &[&str] = &[
    "policy",
    "domain",
    "for",
    "every",
    "cooldown",
    "when",
    "then",
    "also",
    "otherwise",
    "and",
    "or",
    "always",
    "this_good",
    "day",
    "hour",
    "gross",
    "net",
    "in",
    "to",
    "at",
    "note",
    "disabled",
];

const SCOPE: &[&str] = &["firm", "site", "group", "product", "category"];

const UNIT: &[&str] = &["money", "qty", "days", "metres"];

/// Nazwy metryk w kolejności [`super::editor::metryki`] — indeks jest tu wyłącznie
/// wygodą, kontraktem jest nazwa klucza.
const METRIC: &[&str] = &[
    "Price",
    "UnitCost",
    "Margin",
    "CheapestCompetitorPrice",
    "AvgCompetitorPrice",
    "CompetitorCount",
    "Stock",
    "StockDays",
    "Turnover7d",
    "Sales7d",
    "DaysToExpiry",
    "ShelfGap",
    "MachineUtilization",
    "OpenPositions",
    "StaffTurnover12m",
    "MedianMarketWage",
    "StaffMood",
    "ManagerSkill",
    "CashBalance",
    "Receivables",
    "Season",
    "DayOfWeek",
    "DayOfMonth",
    "HourOfDay",
    "DaysSinceLastChange",
];

const SEASON: &[&str] = &["Winter", "Spring", "Summer", "Autumn"];

fn kw(c: &Catalog, l: Locale, name: &str) -> String {
    c.fmt_key(l, &format!("ui.policy.kw.{name}"), &[])
}

fn scope_word(c: &Catalog, l: Locale, name: &str) -> String {
    c.fmt_key(l, &format!("ui.policy.scope.{name}"), &[])
}

fn unit_word(c: &Catalog, l: Locale, name: &str) -> String {
    c.fmt_key(l, &format!("ui.policy.unit.{name}"), &[])
}

fn metric_word(c: &Catalog, l: Locale, name: &str) -> String {
    c.fmt_key(l, &format!("ui.policy.metric.{name}"), &[])
}

fn act_word(c: &Catalog, l: Locale, k: ActionKind) -> String {
    c.fmt_key(l, &format!("ui.policy.act.{}", k.name()), &[])
}

/// Słownik odwrotny: słowo → rola, ze **wszystkich** języków naraz.
struct Slownik {
    kw: BTreeMap<String, &'static str>,
    scope: BTreeMap<String, &'static str>,
    unit: BTreeMap<String, &'static str>,
    metric: BTreeMap<String, &'static str>,
    act: BTreeMap<String, ActionKind>,
    season: BTreeMap<String, u8>,
    dow: BTreeMap<String, u8>,
    sev: BTreeMap<String, Severity>,
    domain: BTreeMap<String, PolicyDomain>,
    src: BTreeMap<String, magnat_policy::OrderSource>,
}

const DOW: &[&str] = &[
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

const SEV: &[(&str, Severity)] = &[
    ("Info", Severity::Info),
    ("Warning", Severity::Warning),
    ("Critical", Severity::Critical),
];

const DOMAIN: &[(&str, PolicyDomain)] = &[
    ("Pricing", PolicyDomain::Pricing),
    ("Stock", PolicyDomain::Stock),
    ("Hr", PolicyDomain::Hr),
    ("Production", PolicyDomain::Production),
    ("Logistics", PolicyDomain::Logistics),
];

impl Slownik {
    fn new(c: &Catalog) -> Slownik {
        let mut s = Slownik {
            kw: BTreeMap::new(),
            scope: BTreeMap::new(),
            unit: BTreeMap::new(),
            metric: BTreeMap::new(),
            act: BTreeMap::new(),
            season: BTreeMap::new(),
            dow: BTreeMap::new(),
            sev: BTreeMap::new(),
            domain: BTreeMap::new(),
            src: BTreeMap::new(),
        };
        for l in Locale::ALL {
            for k in KW {
                s.kw.insert(norm(&kw(c, l, k)), k);
            }
            for k in SCOPE {
                s.scope.insert(norm(&scope_word(c, l, k)), k);
            }
            for k in UNIT {
                s.unit.insert(norm(&unit_word(c, l, k)), k);
            }
            for k in METRIC {
                s.metric.insert(norm(&metric_word(c, l, k)), k);
            }
            for k in ActionKind::ALL {
                s.act.insert(norm(&act_word(c, l, *k)), *k);
            }
            for (i, k) in SEASON.iter().enumerate() {
                s.season.insert(
                    norm(&c.fmt_key(l, &format!("ui.policy.season.{k}"), &[])),
                    i as u8,
                );
            }
            for (i, k) in DOW.iter().enumerate() {
                s.dow
                    .insert(norm(&c.fmt_key(l, &format!("ui.dow.{k}"), &[])), i as u8);
            }
            for (k, v) in SEV {
                s.sev
                    .insert(norm(&c.fmt_key(l, &format!("ui.policy.sev.{k}"), &[])), *v);
            }
            for (k, v) in [
                ("Market", magnat_policy::OrderSource::Market),
                ("Preferred", magnat_policy::OrderSource::Preferred),
            ] {
                s.src
                    .insert(norm(&c.fmt_key(l, &format!("ui.policy.src.{k}"), &[])), v);
            }
            for (k, v) in DOMAIN {
                s.domain.insert(
                    norm(&c.fmt_key(l, &format!("ui.policy.domain.{k}"), &[])),
                    *v,
                );
            }
        }
        s
    }
}

/// Normalizacja słowa: małe litery, bo „GDY" i „gdy" to jedno słowo, a gracz przysyła
/// tekst przepisany ręcznie równie często jak skopiowany.
fn norm(s: &str) -> String {
    s.to_lowercase()
}

/// Słowa, które w jednym języku znaczą co innego niż w drugim.
///
/// Parser przyjmuje **oba** języki naraz, więc słowo powtórzone w dwóch rolach nie
/// jest niejednoznacznością do rozstrzygnięcia w locie — jest błędem danych.
/// Pierwszy przypadek wyszedł od razu: polskie „TO" (wtedy) i angielskie „to"
/// (do widełek) to jedno słowo po normalizacji, a od tego, które wygra, zależało,
/// czy reguła ma akcje. Pilnuje tego test `zaden_jezyk_nie_nadpisuje_slowa_drugiego`.
#[must_use]
#[cfg(test)]
fn kolizje(c: &Catalog) -> Vec<String> {
    let mut out = Vec::new();
    for (nazwa, lista) in [
        ("kw", KW),
        ("scope", SCOPE),
        ("unit", UNIT),
        ("metric", METRIC),
    ] {
        let mut widziane: BTreeMap<String, &str> = BTreeMap::new();
        for l in Locale::ALL {
            for k in lista {
                let w = norm(&match nazwa {
                    "kw" => kw(c, l, k),
                    "scope" => scope_word(c, l, k),
                    "unit" => unit_word(c, l, k),
                    _ => metric_word(c, l, k),
                });
                if let Some(inny) = widziane.insert(w.clone(), k) {
                    if inny != *k {
                        out.push(format!("{nazwa}: „{w}” to i {inny}, i {k}"));
                    }
                }
            }
        }
    }
    out
}

// ── zapis ────────────────────────────────────────────────────────────────────────

/// Polityka jako tekst w zadanym języku.
#[must_use]
pub fn write(p: &Policy, scope: &PolicyScope, goods: &GoodKeys, c: &Catalog, l: Locale) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{} \"{}\" {} {} {} {} {} {}",
        kw(c, l, "policy"),
        // Cudzysłów w nazwie zamyka napis, więc zamienia się go na apostrof. Nazwa
        // jest tekstem gracza, nie kluczem — i to jest jedyne miejsce, w którym
        // postać tekstowa nie jest wierna co do znaku.
        p.name.replace('"', "'"),
        kw(c, l, "domain"),
        c.fmt_key(l, &format!("ui.policy.domain.{}", domena(p.domain)), &[]),
        kw(c, l, "for"),
        zakres(scope, goods, c, l),
        kw(c, l, "every"),
        match p.cadence {
            Cadence::Daily => kw(c, l, "day"),
            Cadence::Hourly => kw(c, l, "hour"),
        }
    ));
    if p.cooldown_h > 0 {
        out.push_str(&format!(" {} {}", kw(c, l, "cooldown"), p.cooldown_h));
    }
    out.push('\n');
    for r in &p.rules {
        // Reguła, której formularz nie umie pokazać, nie ma jak trafić do tekstu —
        // a cicha strata reguły przy eksporcie byłaby najgorszym rodzajem błędu:
        // polityka wygląda na przesłaną i robi mniej. Takiej reguły dziś nie da się
        // zbudować (edytor jej nie złoży, presety jej nie mają), więc `debug_assert`
        // wystarcza za bramkę — pęknie w testach, zanim ktokolwiek zobaczy skutek.
        //
        // ponytail: sufit nazwany — gdyby kiedyś powstało źródło polityk spoza
        // formularza (mod, import z cudzego zapisu), `write` zmienia się w `Result`.
        let Some(d) = RuleDraft::from_rule(r) else {
            debug_assert!(false, "reguła spoza formularza w zapisie tekstowym");
            continue;
        };
        if !d.enabled {
            out.push_str(&format!("  {}\n", kw(c, l, "disabled")));
        }
        out.push_str(&format!(
            "  {} {}\n",
            kw(c, l, "when"),
            warunek(&d, goods, c, l, Styl::Zapis)
        ));
        for (i, a) in d.actions.iter().enumerate() {
            let slowo = if i == 0 {
                kw(c, l, "then")
            } else {
                kw(c, l, "also")
            };
            out.push_str(&format!(
                "    {} {}\n",
                slowo,
                akcja(a, goods, c, l, Styl::Zapis)
            ));
        }
        if !d.note.is_empty() {
            out.push_str(&format!("    {} \"{}\"\n", kw(c, l, "note"), d.note));
        }
    }
    if let Some(a) = &p.fallback {
        debug_assert!(
            ActionDraft::from_action(a).is_some(),
            "akcja zapasowa spoza formularza w zapisie tekstowym"
        );
        if let Some(d) = ActionDraft::from_action(a) {
            out.push_str(&format!(
                "  {} {}\n",
                kw(c, l, "otherwise"),
                akcja(&d, goods, c, l, Styl::Zapis)
            ));
        }
    }
    out
}

const fn domena(d: PolicyDomain) -> &'static str {
    match d {
        PolicyDomain::Pricing => "Pricing",
        PolicyDomain::Stock => "Stock",
        PolicyDomain::Hr => "Hr",
        PolicyDomain::Production => "Production",
        PolicyDomain::Logistics => "Logistics",
    }
}

/// Wiersze jednej reguły: warunek, potem po jednym wierszu na akcję.
///
/// Edytor pokazuje regułę tak samo, jak zapisuje ją tekst — bo to jest **ta sama**
/// reguła. Drugi sposób opisania warunku w formularzu rozjechałby się z zapisem
/// przy pierwszej nowej metryce, a gracz porównuje jedno z drugim.
#[must_use]
pub fn rule_lines(d: &RuleDraft, goods: &GoodKeys, c: &Catalog, l: Locale) -> Vec<String> {
    let mut out = vec![format!(
        "{} {}",
        kw(c, l, "when"),
        warunek(d, goods, c, l, Styl::Ekran)
    )];
    for (i, a) in d.actions.iter().enumerate() {
        let slowo = if i == 0 {
            kw(c, l, "then")
        } else {
            kw(c, l, "also")
        };
        out.push(format!("  {slowo} {}", akcja(a, goods, c, l, Styl::Ekran)));
    }
    if !d.note.is_empty() {
        out.push(format!("  {} \"{}\"", kw(c, l, "note"), d.note));
    }
    out
}

/// Jeden wiersz akcji — nagłówek reguły zapasowej w edytorze.
#[must_use]
pub fn action_line(a: &ActionDraft, goods: &GoodKeys, c: &Catalog, l: Locale) -> String {
    akcja(a, goods, c, l, Styl::Ekran)
}

fn zakres(s: &PolicyScope, goods: &GoodKeys, c: &Catalog, l: Locale) -> String {
    match s {
        PolicyScope::Firm(f) => format!("{} {}", scope_word(c, l, "firm"), encja(f.0)),
        PolicyScope::Site(x) => format!("{} {}", scope_word(c, l, "site"), encja(x.0)),
        PolicyScope::Group(t) => format!("{} {}", scope_word(c, l, "group"), t.0),
        PolicyScope::Product { inner, good } => format!(
            "{} {} {} {}",
            scope_word(c, l, "product"),
            towar_klucz(*good, goods),
            kw(c, l, "in"),
            zakres(inner, goods, c, l)
        ),
        PolicyScope::Category { inner, cat } => format!(
            "{} {} {} {}",
            scope_word(c, l, "category"),
            cat.0,
            kw(c, l, "in"),
            zakres(inner, goods, c, l)
        ),
    }
}

fn encja(e: Entity) -> String {
    format!("{}:{}", e.index(), e.generation())
}

fn towar_klucz(g: GoodId, goods: &GoodKeys) -> String {
    goods
        .key(g)
        .map_or_else(|| format!("\"good#{}\"", g.get()), |k| format!("\"{k}\""))
}

fn towar(r: magnat_policy::GoodRef, goods: &GoodKeys, c: &Catalog, l: Locale) -> String {
    match r {
        magnat_policy::GoodRef::This => kw(c, l, "this_good"),
        magnat_policy::GoodRef::Id(g) => towar_klucz(g, goods),
    }
}

fn warunek(d: &RuleDraft, goods: &GoodKeys, c: &Catalog, l: Locale, styl: Styl) -> String {
    if d.clauses.is_empty() {
        return kw(c, l, "always");
    }
    let spojnik = match d.join {
        Join::And => kw(c, l, "and"),
        Join::Or => kw(c, l, "or"),
    };
    d.clauses
        .iter()
        .map(|k| klauzula(k, goods, c, l, styl))
        .collect::<Vec<_>>()
        .join(&format!(" {spojnik} "))
}

fn klauzula(k: &Clause, goods: &GoodKeys, c: &Catalog, l: Locale, styl: Styl) -> String {
    let ctx = match k.lhs.base {
        Base::Metric(m) => Some(m),
        Base::Lit(_) => None,
    };
    format!(
        "{} {} {}",
        slot(&k.lhs, ctx, goods, c, l, styl),
        op_tekst(k.op),
        slot(&k.rhs, ctx, goods, c, l, styl)
    )
}

const fn op_tekst(o: CmpOp) -> &'static str {
    match o {
        CmpOp::Lt => "<",
        CmpOp::Le => "<=",
        CmpOp::Eq => "=",
        CmpOp::Ne => "!=",
        CmpOp::Ge => ">=",
        CmpOp::Gt => ">",
    }
}

fn slot(
    s: &Slot,
    ctx: Option<Metric>,
    goods: &GoodKeys,
    c: &Catalog,
    l: Locale,
    styl: Styl,
) -> String {
    let rdzen = match s.base {
        Base::Metric(m) => metryka(m, goods, c, l),
        Base::Lit(v) => wartosc(v, ctx, c, l, styl),
    };
    let ze_skala = match s.scale {
        Some(bp) => format!("{rdzen} * {}", procent(bp, styl, l)),
        None => rdzen,
    };
    match s.convert {
        Some(PriceBasis::GrossRetail) => format!("{}( {ze_skala} )", kw(c, l, "gross")),
        Some(PriceBasis::NetB2B) => format!("{}( {ze_skala} )", kw(c, l, "net")),
        None => ze_skala,
    }
}

/// Do czego powstaje ten tekst: do zapisu czy na ekran.
///
/// Rozróżnienie jest konieczne, bo te dwa mają **sprzeczne** wymagania. Postać
/// tekstowa musi wrócić z parsera co do znaku, więc nie ma języka i mieć go nie
/// może — polityka zapisana po polsku wczytuje się po angielsku. Ekran ma za to
/// pokazywać liczby tak, jak pisze je gracz w swoim języku (`M9b`).
///
/// Do R2e obie drogi były jedną drogą i wygrywał zapis, więc polski gracz czytał
/// „18.55 %" i kwotę w groszach. Rozdziela je ten enum, a nie drugi zestaw
/// funkcji: reguła składania zdania jest jedna i ma zostać jedna — różni się
/// w niej wyłącznie to, skąd bierze się separator i jak drukuje się kwota.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Styl {
    /// Format wymiany: kropka dziesiętna, kwota w groszach, bez separatora tysięcy.
    Zapis,
    /// Interfejs: separator języka, kwota z walutą przez `fmt::money`.
    Ekran,
}

/// Punkty bazowe jako procent.
fn procent(bp: Bp, styl: Styl, l: Locale) -> String {
    let v = bp.get();
    let a = v.unsigned_abs();
    if styl == Styl::Ekran {
        // Minus typograficzny, tak samo jak w `fmt::integer` — ekran jest ekranem.
        let znak = if v < 0 { "\u{2212}" } else { "" };
        if a.is_multiple_of(100) {
            return format!("{znak}{}%", a / 100);
        }
        return format!("{znak}{}%", magnat_ui::fmt::decimal(l, i64::from(a), 2));
    }
    let znak = if v < 0 { "-" } else { "" };
    if a.is_multiple_of(100) {
        format!("{znak}{}%", a / 100)
    } else {
        format!("{znak}{}.{:02}%", a / 100, a % 100)
    }
}

fn wartosc(v: Value, ctx: Option<Metric>, c: &Catalog, l: Locale, styl: Styl) -> String {
    match v {
        Value::Money(m) => format!("{} {}", m.get(), unit_word(c, l, "money")),
        Value::Qty(q) => format!("{q} {}", unit_word(c, l, "qty")),
        Value::Days(d) => format!("{d} {}", unit_word(c, l, "days")),
        Value::Bp(bp) => procent(bp, styl, l),
        Value::Count(n) => n.to_string(),
        Value::Enum(e) => match ctx {
            Some(Metric::Season) => c.fmt_key(
                l,
                &format!(
                    "ui.policy.season.{}",
                    SEASON[usize::from(e).min(SEASON.len() - 1)]
                ),
                &[],
            ),
            Some(Metric::DayOfWeek) => c.fmt_key(
                l,
                &format!("ui.dow.{}", DOW[usize::from(e).min(DOW.len() - 1)]),
                &[],
            ),
            _ => format!("enum#{e}"),
        },
    }
}

fn metryka(m: Metric, goods: &GoodKeys, c: &Catalog, l: Locale) -> String {
    let n = |name: &str| metric_word(c, l, name);
    let t = |g| towar(g, goods, c, l);
    let promien = |r: u32| format!("{r} {}", unit_word(c, l, "metres"));
    let b = |x: PriceBasis| match x {
        PriceBasis::GrossRetail => kw(c, l, "gross"),
        PriceBasis::NetB2B => kw(c, l, "net"),
    };
    match m {
        Metric::Price { good, basis } => format!("{}( {} , {} )", n("Price"), t(good), b(basis)),
        Metric::UnitCost(g) => format!("{}( {} )", n("UnitCost"), t(g)),
        Metric::Margin(g) => format!("{}( {} )", n("Margin"), t(g)),
        Metric::CheapestCompetitorPrice {
            good,
            radius_m,
            basis,
        } => format!(
            "{}( {} , {} , {} )",
            n("CheapestCompetitorPrice"),
            t(good),
            promien(radius_m),
            b(basis)
        ),
        Metric::AvgCompetitorPrice {
            good,
            radius_m,
            basis,
        } => format!(
            "{}( {} , {} , {} )",
            n("AvgCompetitorPrice"),
            t(good),
            promien(radius_m),
            b(basis)
        ),
        Metric::CompetitorCount { radius_m } => {
            format!("{}( {} )", n("CompetitorCount"), promien(radius_m))
        }
        Metric::Stock(g) => format!("{}( {} )", n("Stock"), t(g)),
        Metric::StockDays(g) => format!("{}( {} )", n("StockDays"), t(g)),
        Metric::Turnover7d(g) => format!("{}( {} )", n("Turnover7d"), t(g)),
        Metric::Sales7d(g) => format!("{}( {} )", n("Sales7d"), t(g)),
        Metric::DaysToExpiry(g) => format!("{}( {} )", n("DaysToExpiry"), t(g)),
        Metric::ShelfGap(g) => format!("{}( {} )", n("ShelfGap"), t(g)),
        Metric::DaysSinceLastChange(g) => format!("{}( {} )", n("DaysSinceLastChange"), t(g)),
        Metric::OpenPositions(r) => format!("{}( role#{} )", n("OpenPositions"), r.0),
        Metric::MedianMarketWage(r) => format!("{}( role#{} )", n("MedianMarketWage"), r.0),
        Metric::MachineUtilization => n("MachineUtilization"),
        Metric::StaffTurnover12m => n("StaffTurnover12m"),
        Metric::StaffMood => n("StaffMood"),
        Metric::ManagerSkill => n("ManagerSkill"),
        Metric::CashBalance => n("CashBalance"),
        Metric::Receivables => n("Receivables"),
        Metric::Season => n("Season"),
        Metric::DayOfWeek => n("DayOfWeek"),
        Metric::DayOfMonth => n("DayOfMonth"),
        Metric::HourOfDay => n("HourOfDay"),
    }
}

fn akcja(a: &ActionDraft, goods: &GoodKeys, c: &Catalog, l: Locale, styl: Styl) -> String {
    let v = act_word(c, l, a.kind);
    let t = towar(a.good, goods, c, l);
    let s = |x: &Slot| slot(x, None, goods, c, l, styl);
    match a.kind {
        ActionKind::SetPrice | ActionKind::AdjustPrice => format!("{v} {t} = {}", s(&a.a)),
        ActionKind::SetMargin | ActionKind::Markdown => {
            format!("{v} {t} = {}", procent(a.bp, styl, l))
        }
        ActionKind::ClampPrice => format!("{v} {t} {} {} .. {}", kw(c, l, "to"), s(&a.a), s(&a.b)),
        ActionKind::OrderUpTo => format!("{v} {t} {}", s(&a.a)),
        ActionKind::OrderQty => format!(
            "{v} {t} {} {}",
            s(&a.a),
            c.fmt_key(
                l,
                match a.source {
                    magnat_policy::OrderSource::Market => "ui.policy.src.Market",
                    magnat_policy::OrderSource::Preferred => "ui.policy.src.Preferred",
                },
                &[]
            )
        ),
        ActionKind::RemoveFromShelf => format!("{v} {t}"),
        ActionKind::Hire => format!(
            "{v} {} role#{} {} {}",
            a.count,
            a.role.0,
            kw(c, l, "at"),
            s(&a.a)
        ),
        ActionKind::RaiseWage => {
            let baza = format!("{v} role#{} = {}", a.role.0, procent(a.bp, styl, l));
            if a.has_b {
                format!("{baza} {} {}", kw(c, l, "to"), s(&a.b))
            } else {
                baza
            }
        }
        ActionKind::PlanProduction => format!("{v} recipe#{} {}", a.recipe.0, s(&a.a)),
        ActionKind::Alert => format!(
            "{v} msg#{} {}",
            a.msg,
            c.fmt_key(
                l,
                &format!(
                    "ui.policy.sev.{}",
                    SEV.iter()
                        .find(|(_, s)| *s == a.severity)
                        .map_or("Info", |(k, _)| k)
                ),
                &[]
            )
        ),
        ActionKind::AskPlayer => format!("{v} msg#{}", a.msg),
    }
}

// ── odczyt ───────────────────────────────────────────────────────────────────────

mod parser;

pub use parser::parse;

#[cfg(test)]
mod tests;
