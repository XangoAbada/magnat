//! Warstwa gracza nad silnikiem reguł: edytor, diagnostyka, dry-run i tekst
//! (M9d WP8, `K-11`).
//!
//! # Podział z `sim/policy` i `sim/economy`
//!
//! | co | gdzie | dlaczego tam |
//! |---|---|---|
//! | AST, ewaluator, walidator języka | `sim/policy` (M7) | firmy AI wykonują ten sam kod |
//! | wykonawca akcji i jakość menedżera | `sim/economy` (M7c, M9d WP9) | ma półkę i stoi w systemie ECS |
//! | **formularz, diagnostyka edytora, tekst, dry-run** | tutaj | widzi gracza, katalog tekstów i listę polityk |
//!
//! To nie jest podział estetyczny, tylko wymuszony grafem: `game` zależy od `economy`,
//! `economy` od `firms`, `firms` od `policy` — i nigdy odwrotnie. Edytor nie mógłby
//! stać niżej, bo potrzebuje katalogu lokalizacji; wykonawca nie mógłby stać wyżej,
//! bo woła go system symulacji.
//!
//! # Czego edytor nie pozwala zrobić
//!
//! Złożyć błędu. Każda zmiana idzie przez [`Edit`] i każda niosąca wyrażenie jest
//! odrzucana, jeśli nie zgadza się jednostką albo podstawą ceny (`K-7`). Walidator
//! `sim/policy` zostaje drugą linią — dla polityk z **importu tekstowego** i z zapisu
//! gry sprzed zmiany danych.

pub mod editor;
pub mod slot;
pub mod text;
pub mod view;

pub use editor::{metryki, Edit, EditError, Note, RuleEditor};
pub use slot::{ActionDraft, Base, Clause, Join, RuleDraft, Slot};
pub use text::{parse, write, GoodKeys, TextError};
pub use view::{EditorAction, RuleEditorView, Tab};
// Typy języka są **częścią** publicznego wejścia edytora (`RuleEditor::policy`
// zwraca `Policy`, `from_policy` bierze `PolicyScope`), więc mają tu adres —
// klient nie musi zależeć od `sim/policy`, żeby otworzyć ekran. To ta sama droga,
// którą `K-50` nadaje adres funkcjom jądra ekonomii.
pub use magnat_policy::{Policy, PolicyCatalog, PolicyDomain, PolicyScope};

use magnat_core::{Money, SiteId};
use magnat_economy::{DryRun, Market, PolicyOutcome};

/// Podsumowanie dry-runu dla nagłówka edytora: „na ilu dobach, ile razy ruszyłaby
/// ceną, w jakich widełkach".
///
/// Liczby zamiast wykresu, bo pytanie brzmi „czy to w ogóle coś robi" — a na nie
/// odpowiada licznik, nie krzywa. Krzywą rysuje panel sklepu z [`DryRun::prices_of`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct DrySummary {
    pub days: u32,
    /// Ile razy polityka ruszyłaby ceną albo marżą.
    pub price_moves: u32,
    /// Ile razy zmieniłaby cel zapasu.
    pub orders: u32,
    /// Ile alertów i pytań by wystawiła.
    pub alerts: u32,
    /// Ile razy nie umiała policzyć wyrażenia — reguła oparta na metryce, której
    /// zakład nie zna, jest regułą martwą i to jest jedyny sposób, żeby gracz
    /// dowiedział się o tym **przed** przypięciem.
    pub blind: u32,
    pub min_price: Option<Money>,
    pub max_price: Option<Money>,
}

impl DrySummary {
    #[must_use]
    pub fn of(r: &DryRun) -> DrySummary {
        let mut s = DrySummary {
            days: u32::try_from(r.days.len()).unwrap_or(u32::MAX),
            ..DrySummary::default()
        };
        for d in &r.days {
            for o in &d.decisions {
                match o {
                    PolicyOutcome::Price(_, m) => {
                        s.price_moves += 1;
                        s.min_price = Some(s.min_price.map_or(*m, |x: Money| x.min(*m)));
                        s.max_price = Some(s.max_price.map_or(*m, |x: Money| x.max(*m)));
                    }
                    PolicyOutcome::Margin(..) => s.price_moves += 1,
                    PolicyOutcome::Order(..) => s.orders += 1,
                    PolicyOutcome::Alert { .. } => s.alerts += 1,
                    PolicyOutcome::Blind => s.blind += 1,
                }
            }
        }
        s
    }
}

/// Otwiera edytor dla zakładu: polityką, która na nim **już stoi**, albo presetem
/// „Kurs stały" z `data/policies/`.
///
/// Pusty formularz byłby uczciwy i bezużyteczny — gracz uczy się języka, patrząc
/// na regułę, która działa, a nie na listę pustych slotów. Ten sam preset dostaje
/// firma AI, więc „ten sam zestaw narzędzi" z PRD §6.3 zaczyna się od tej samej
/// kartki papieru.
///
/// `None` znaczy, że tej polityki formularz nie umie pokazać — wtedy zostaje zapis
/// tekstowy, a nie połowa ekranu.
#[must_use]
pub fn open_for(session: &crate::Session, site: SiteId) -> Option<(RuleEditor, Policy)> {
    let p = session
        .app
        .world
        .get_resource::<magnat_firms::Firms>()
        .and_then(|f| f.site(site))
        .and_then(|z| z.delegation.as_ref().map(|d| d.policy.clone()))
        .or_else(|| {
            let kat = session.app.world.get_resource::<PolicyCatalog>()?;
            magnat_economy::preset_for(kat, "retail_steady", PolicyDomain::Pricing)
        })?;
    // Stawka VAT wchodzi **tutaj**, a nie w konstruktorze: bez niej `BelowCost`
    // porównuje cenę brutto z kosztem netto i myli się dokładnie o podatek.
    let vat = session.market.as_ref().map_or(0, Market::vat_bp);
    let e = RuleEditor::from_policy(&p, PolicyScope::Site(site))?.with_vat(vat);
    Some((e, p))
}

/// Klucze towarów detalicznych — słownik postaci tekstowej polityki (00 §5).
#[must_use]
pub fn good_keys(market: &Market) -> GoodKeys {
    let Ok(dane) = magnat_economy::EconomyData::load_default() else {
        return GoodKeys::default();
    };
    GoodKeys::new(
        dane.retail
            .goods
            .iter()
            .filter_map(|g| Some((market.good_of_key(&g.key)?, g.key.clone()))),
    )
}

/// „Przetestuj na ostatnich 30 dniach" (M9d §5.6 pkt 8).
///
/// Cała arytmetyka siedzi w [`Market::dry_run`], czyli w tym samym kodzie, który
/// politykę wykonuje. Tutaj jest wyłącznie wywołanie — i to jest cel: druga arytmetyka
/// podglądu rozjechałaby się z wykonaniem przy pierwszej zmianie wzoru, a dry-run
/// istnieje właśnie po to, żeby powiedzieć, co się stanie.
#[must_use]
pub fn dry_run(market: &Market, site: SiteId, e: &RuleEditor) -> DryRun {
    market.dry_run(site, &e.policy())
}
