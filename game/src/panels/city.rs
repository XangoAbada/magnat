//! Miasto: podatki, budżet, przetargi i wybory.
//!
//! Panel czyta `sim/city` i nie liczy niczego sam — stawki są polityką miasta,
//! a budżet jego księgą. Jedyna droga, którą gracz może na to wpłynąć, to wybory:
//! **wpłata na kampanię** (`K-66`). Pieniądz wychodzi z gospodarstwa i jedzie
//! kanałem `TxKind::CampaignDonation`, więc poparcie jest kosztem, a nie deklaracją.

use magnat_core::{Money, Subject, TaxKind};
use magnat_ui::DataSource;

use super::widgets::{action, picker, row, section};
use super::{PanelAction, PanelCtx, PanelDesc, PanelId, PanelModel, PanelView};
use crate::PlayerCommand;

pub(super) const DESC: PanelDesc = PanelDesc {
    id: PanelId::City,
    title: "ui.panel.city",
    deps: &[DataSource::Clock, DataSource::World, DataSource::Locale],
    build,
    render,
    min_tier: Some(crate::CareerTier::Company),
};

/// Ile wpłacić na kampanię jednym kliknięciem: dziesiąta część gotówki. Ułamek,
/// a nie kwota stała — wpłata ma znaczyć tyle samo dla kioskarza i dla magnata.
const CZESC_NA_KAMPANIE: i64 = 10;

pub struct Model {
    /// Stawki danin, które **mają jedną liczbę**: CIT, podatek od nieruchomości
    /// i odsetki za zwłokę. VAT ma klasę per towar, a PIT progi — jedna stawka
    /// byłaby dla nich fałszem, więc panel ich tu nie udaje.
    pub rates: Vec<(TaxKind, i64)>,
    pub revenue_ytd: Money,
    pub spend_ytd: Money,
    pub debt: Money,
    /// Kandydaci: numer, czy urzędujący, i zebrane środki. **Sondażu tu nie ma**:
    /// wynik wyborów liczy się z głosów mieszkańców w dniu głosowania, a liczba
    /// pokazana wcześniej jako „poparcie" byłaby prognozą, której nikt nie liczy.
    pub candidates: Vec<(u8, u16, Money)>,
    pub tenders: usize,
    pub capital: Money,
    /// Ile urzędów przyjmuje wnioski. Zero znaczy „nie ma gdzie złożyć".
    pub offices: usize,
}

fn build(ctx: &PanelCtx<'_>) -> PanelModel {
    let s = ctx.session;
    let Some(city) = s.app.world.get_resource::<magnat_city::City>() else {
        return PanelModel::City(Model {
            rates: Vec::new(),
            revenue_ytd: Money::ZERO,
            spend_ytd: Money::ZERO,
            debt: Money::ZERO,
            candidates: Vec::new(),
            tenders: 0,
            capital: Money::ZERO,
            offices: 0,
        });
    };
    let rates = vec![
        (TaxKind::Cit, i64::from(city.code.cit_bp)),
        (TaxKind::Property, i64::from(city.code.property_bp_per_year)),
    ];
    let candidates = city.election.as_ref().map_or_else(Vec::new, |e| {
        e.candidates
            .iter()
            .enumerate()
            .map(|(i, c)| {
                (
                    u8::try_from(i).unwrap_or(u8::MAX),
                    u16::from(c.incumbent),
                    c.funding,
                )
            })
            .collect()
    });
    PanelModel::City(Model {
        rates,
        revenue_ytd: city.budget.revenue_total(),
        spend_ytd: city.budget.spend_total(),
        debt: city.budget.debt_outstanding(),
        candidates,
        tenders: city.tenders.len(),
        capital: crate::metrics::gotowka_gracza(s),
        offices: city.permits.offices().len(),
    })
}

fn render(
    ui: &mut egui::Ui,
    ctx: &PanelCtx<'_>,
    model: &PanelModel,
    view: &mut PanelView,
) -> PanelAction {
    let PanelModel::City(m) = model else {
        return PanelAction::None;
    };
    let th = ctx.theme;
    if m.rates.is_empty() {
        ui.label(ctx.text("ui.city.no_city"));
        return PanelAction::None;
    }
    let mut akcja = PanelAction::None;

    section(ui, th, &ctx.text("ui.city.taxes"));
    for (k, bp) in &m.rates {
        row(
            ui,
            th,
            &ctx.text(&format!("ui.tax.{}", k.name())),
            &format!("{},{:02}%", bp / 100, (bp % 100).abs()),
        );
    }

    section(ui, th, &ctx.text("ui.city.budget"));
    row(
        ui,
        th,
        &ctx.text("ui.city.revenue"),
        &ctx.money(m.revenue_ytd),
    );
    row(ui, th, &ctx.text("ui.city.spend"), &ctx.money(m.spend_ytd));
    row(ui, th, &ctx.text("ui.city.debt"), &ctx.money(m.debt));
    row(
        ui,
        th,
        &ctx.text("ui.city.tenders"),
        &ctx.int(i64::try_from(m.tenders).unwrap_or(0)),
    );
    if super::widgets::action(ui, ctx, &ctx.text("ui.city.show_council"), None) {
        akcja = PanelAction::Show(Subject::Government);
    }

    // Wniosek o pozwolenie: jedyna rzecz, którą gracz może zrobić z miastem
    // **poza kampanią**. Wybory są raz na kadencję, a urząd stoi zawsze —
    // bez tego panel Miasto byłby operacyjny przez trzy tygodnie na cztery lata.
    section(ui, th, &ctx.text("ui.city.permits"));
    if m.offices == 0 {
        ui.label(ctx.text("ui.city.no_office"));
    } else {
        let rodzaje: Vec<String> = magnat_core::PermitKind::ALL
            .iter()
            .map(|k| ctx.text(&format!("ui.permit.{}", k.name())))
            .collect();
        picker(ui, th, &rodzaje, &mut view.tab);
        let cmd = PlayerCommand::ApplyForPermit {
            kind: u8::try_from(view.tab).unwrap_or(0),
        };
        let blokada = crate::precheck(&ctx.session.view(), &cmd)
            .err()
            .map(|e| e.text(ctx.c, ctx.l));
        if action(ui, ctx, &ctx.text("ui.city.file_permit"), blokada) {
            akcja = PanelAction::cmd(cmd);
        }
    }

    section(ui, th, &ctx.text("ui.city.election"));
    if m.candidates.is_empty() {
        ui.label(ctx.text("ui.city.no_election"));
        return akcja;
    }
    let etykiety: Vec<String> = m
        .candidates
        .iter()
        .map(|(nr, urzedujacy, srodki)| {
            ctx.fmt(
                if *urzedujacy == 1 {
                    "ui.city.candidate_incumbent"
                } else {
                    "ui.city.candidate"
                },
                &[("nr", &nr.to_string()), ("srodki", &ctx.money(*srodki))],
            )
        })
        .collect();
    picker(ui, th, &etykiety, &mut view.sel);
    let (nr, _, _) = m.candidates[view.sel.min(m.candidates.len() - 1)];
    let kwota = Money(m.capital.get() / CZESC_NA_KAMPANIE);
    let cmd = PlayerCommand::BackCandidate {
        candidate: nr,
        amount: kwota,
    };
    let blokada = crate::precheck(&ctx.session.view(), &cmd)
        .err()
        .map(|e| e.text(ctx.c, ctx.l));
    if action(
        ui,
        ctx,
        &ctx.fmt("ui.city.back", &[("kwota", &ctx.money(kwota))]),
        blokada,
    ) {
        akcja = PanelAction::cmd(cmd);
    }
    akcja
}
