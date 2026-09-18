//! Finanse: rachunek wyniku, bilans, przepływy i kredyty.
//!
//! # VAT nie jest przychodem (`K-7`)
//!
//! Przychód netto i VAT należny to **dwa osobne wiersze i nigdy się nie sumują**.
//! VAT jest zobowiązaniem wobec miasta, więc stoi po stronie pasywów bilansu,
//! a nie w rachunku wyniku. Panel, który by je dodał, pokazałby graczowi obrót
//! brutto pod nazwą przychodu — i gracz podjąłby na tym decyzję cenową.
//!
//! Komenda panelu: **kredyt obrotowy**. Ta sama ocena i ten sam bank, do którego
//! sklep puka sam przy pustej kasie — różni się wyłącznie tym, kto nacisnął.

use magnat_core::SiteId;
use magnat_economy::{BalanceSheet, CashFlow, IncomeStatement};
use magnat_ui::DataSource;

use super::widgets::{action, picker, row, section, Sev};
use super::{PanelAction, PanelCtx, PanelDesc, PanelId, PanelModel, PanelView};
use crate::{CareerTier, PlayerCommand};

pub(super) const DESC: PanelDesc = PanelDesc {
    id: PanelId::Finance,
    title: "ui.panel.finance",
    deps: &[DataSource::Clock, DataSource::Market, DataSource::Locale],
    build,
    render,
    // Panel Finanse z pełną księgowością otwarty w piątej minucie zabija metrykę
    // onboardingu (§5.12 pkt 5) — dlatego przypina się dopiero z drugim zakładem.
    min_tier: Some(CareerTier::Company),
};

/// Ile dób wstecz obejmuje rachunek wyniku panelu: miesiąc gry (`K-1`).
const OKNO_DOB: u64 = 30;

pub struct Model {
    pub sites: Vec<SiteId>,
    pub names: Vec<String>,
    pub pnl: Option<IncomeStatement>,
    pub balance: Option<BalanceSheet>,
    pub cash: Option<CashFlow>,
    /// Czy zakład ma już kredyt — drugi wniosek bank odrzuci.
    pub has_loan: bool,
}

fn build(ctx: &PanelCtx<'_>) -> PanelModel {
    let s = ctx.session;
    let Some(m) = s.market.as_ref() else {
        return PanelModel::Finance(Model {
            sites: Vec::new(),
            names: Vec::new(),
            pnl: None,
            balance: None,
            cash: None,
            has_loan: false,
        });
    };
    let znane = m.sites();
    let sites: Vec<SiteId> = ctx
        .holdings
        .sites
        .iter()
        .copied()
        .filter(|x| znane.contains(x))
        .collect();
    let names = sites
        .iter()
        .map(|x| {
            ctx.fmt(
                "ui.subject.site",
                &[("nr", &x.entity().index().to_string())],
            )
        })
        .collect();
    let wybrany = sites.get(ctx.sel.min(sites.len().saturating_sub(1))).copied();
    let do_ = s.tick();
    let od = magnat_core::Tick(
        do_.get()
            .saturating_sub(OKNO_DOB * magnat_core::time::MINUTES_PER_DAY),
    );
    let (pnl, balance, cash, has_loan) = wybrany.map_or((None, None, None, false), |site| {
        (
            m.income_statement(site, od, do_),
            m.balance_sheet(site, do_),
            m.cash_flow(site, od, do_),
            m.shop_panel(site, od, do_)
                .is_some_and(|p| p.finance.loan.is_some()),
        )
    });
    PanelModel::Finance(Model {
        sites,
        names,
        pnl,
        balance,
        cash,
        has_loan,
    })
}

fn render(
    ui: &mut egui::Ui,
    ctx: &PanelCtx<'_>,
    model: &PanelModel,
    view: &mut PanelView,
) -> PanelAction {
    let PanelModel::Finance(m) = model else {
        return PanelAction::None;
    };
    let th = ctx.theme;
    if m.sites.is_empty() {
        ui.label(ctx.text("ui.finance.no_sites"));
        return PanelAction::None;
    }
    picker(ui, th, &m.names, &mut view.sel);
    let site = m.sites[view.sel.min(m.sites.len() - 1)];

    if let Some(p) = &m.pnl {
        section(ui, th, &ctx.text("ui.finance.pnl"));
        // Przychód **netto** — to jest ta liczba, która nie zawiera VAT-u.
        row(ui, th, &ctx.text("ui.finance.revenue_net"), &ctx.money(p.revenue));
        row(ui, th, &ctx.text("ui.finance.cogs"), &ctx.money(p.cogs));
        row(ui, th, &ctx.text("ui.finance.wages"), &ctx.money(p.wages));
        row(ui, th, &ctx.text("ui.finance.rent"), &ctx.money(p.rent));
        row(ui, th, &ctx.text("ui.finance.utilities"), &ctx.money(p.utilities));
        row(ui, th, &ctx.text("ui.finance.depreciation"), &ctx.money(p.depreciation));
        row(ui, th, &ctx.text("ui.finance.interest"), &ctx.money(p.interest));
        row(ui, th, &ctx.text("ui.finance.write_off"), &ctx.money(p.write_off));
        let wynik = p.net_result();
        ui.horizontal(|ui| {
            super::widgets::kpi(
                ui,
                th,
                &ctx.text("ui.finance.result"),
                &ctx.money(wynik),
                Sev::of_money(wynik),
            );
        });
    }

    if let Some(b) = &m.balance {
        section(ui, th, &ctx.text("ui.finance.balance"));
        row(ui, th, &ctx.text("ui.finance.bank"), &ctx.money(b.bank));
        row(ui, th, &ctx.text("ui.finance.inventory"), &ctx.money(b.inventory));
        row(ui, th, &ctx.text("ui.finance.fixed"), &ctx.money(b.fixed_net));
        row(ui, th, &ctx.text("ui.finance.assets"), &ctx.money(b.assets));
        section(ui, th, &ctx.text("ui.finance.liabilities"));
        row(ui, th, &ctx.text("ui.finance.trade_payable"), &ctx.money(b.trade_payable));
        // VAT stoi **tutaj**, po stronie zobowiązań, i nigdzie indziej (`K-7`).
        row(ui, th, &ctx.text("ui.finance.tax_payable"), &ctx.money(b.tax_payable));
        row(ui, th, &ctx.text("ui.finance.wage_payable"), &ctx.money(b.wage_payable));
        row(ui, th, &ctx.text("ui.finance.loans"), &ctx.money(b.loans));
        row(ui, th, &ctx.text("ui.finance.equity"), &ctx.money(b.equity_total));
    }

    if let Some(c) = &m.cash {
        section(ui, th, &ctx.text("ui.finance.cash_flow"));
        row(ui, th, &ctx.text("ui.finance.operating"), &ctx.money(c.operating));
        row(ui, th, &ctx.text("ui.finance.investing"), &ctx.money(c.investing));
        row(ui, th, &ctx.text("ui.finance.financing"), &ctx.money(c.financing));
        if !c.complete {
            // Liczba obcięta oknem dziennika wygląda jak prawdziwa — panel **musi**
            // to powiedzieć, bo inaczej gracz porówna ją z pełnym miesiącem.
            ui.label(ctx.text("ui.finance.window_partial"));
        }
    }

    section(ui, th, &ctx.text("ui.finance.credit"));
    if m.has_loan {
        ui.label(ctx.text("ui.finance.has_loan"));
    }
    let cmd = PlayerCommand::TakeLoan { site };
    let blokada = crate::precheck(&ctx.session.view(), &cmd)
        .err()
        .map(|e| e.text(ctx.c, ctx.l))
        .or_else(|| m.has_loan.then(|| ctx.text("ui.finance.has_loan")));
    if action(ui, ctx, &ctx.text("ui.finance.take_loan"), blokada) {
        return PanelAction::cmd(cmd);
    }
    PanelAction::None
}
