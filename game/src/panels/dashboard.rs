//! Pulpit firmy: przepływy, alerty i cztery liczby, które mówią, jak idzie.
//!
//! Alertów pulpit **nie produkuje**, tylko pokazuje. Eskalacje polityk prowadzi
//! `sim/economy` (`DH-1`, `Market::policy_inbox`), bo akcje `Alert` i `AskPlayer`
//! wykonują się w symulacji tysiące razy na dobę — także wtedy, gdy żadnego okna
//! nie ma. Pusta półka i zakład pod kreską też są stanem świata, nie panelu.
//!
//! Komenda pulpitu: **załóż firmę** albo **otwórz punkt**. To jest miejsce, w którym
//! gracz bez firmy przestaje nim być — i dlatego panel jest przypięty od pierwszej
//! minuty (`min_tier: None`).

use magnat_core::{Money, Subject};
use magnat_ui::DataSource;

use super::widgets::{action, alert, kpi, row, section, Sev};
use super::{PanelAction, PanelCtx, PanelDesc, PanelId, PanelModel, PanelView};
use crate::metrics::MetricId;
use crate::PlayerCommand;

pub(super) const DESC: PanelDesc = PanelDesc {
    id: PanelId::Dashboard,
    title: "ui.panel.dashboard",
    deps: &[DataSource::Clock, DataSource::Market, DataSource::Locale],
    build,
    render,
    min_tier: None,
};

/// Jeden alert pulpitu: waga, zdanie i podmiot, którego kartę otwiera kliknięcie.
pub struct Alert {
    pub sev: Sev,
    pub text: String,
    pub subject: Option<Subject>,
}

pub struct Model {
    pub tier: crate::CareerTier,
    pub sites: usize,
    pub employees: u32,
    pub managers: u32,
    pub cash: Money,
    /// Rachunek ostatniego domkniętego miesiąca, zsumowany po zakładach gracza.
    pub revenue_net: Money,
    pub result: Money,
    pub margin_bp: Option<i32>,
    pub alerts: Vec<Alert>,
    /// Dzielnice, w których da się wejść do lokalu — wybór dla `FoundFirm`/`OpenSite`.
    pub districts: Vec<u16>,
    pub has_firm: bool,
    /// Ile zaproponować jako kapitał startowy: cała gotówka gospodarstwa.
    pub capital: Money,
    /// Saldo rachunku firmy — **stąd** idzie nakład na nowy punkt, a nie z portfela
    /// domowego. Dwa różne konta i dwie różne decyzje.
    pub firm_cash: Money,
}

fn build(ctx: &PanelCtx<'_>) -> PanelModel {
    let h = ctx.holdings;
    let s = ctx.session;
    let cash = crate::metrics::gotowka_gracza(s);
    let (mut revenue, mut result) = (0i64, 0i64);
    if let Some(firms) = s.app.world.get_resource::<magnat_firms::Firms>() {
        for id in &h.sites {
            if let Some(m) = firms.site(*id).and_then(|z| z.pnl.last()) {
                revenue = revenue.saturating_add(m.revenue.get());
                result = result.saturating_add(m.result().get());
            }
        }
    }
    // `as i32` na tym ilorazie zawija się przy sklepie o utargu rzędu złotówek
    // i dużym odpisie — strata pokazywałaby się jako dodatnia marża. Przycięcie
    // jest jawne i mówi prawdę: „poniżej skali", a nie „na plusie".
    let margin_bp = (revenue > 0).then(|| {
        i32::try_from((result.saturating_mul(10_000) / revenue).clamp(-1_000_000, 1_000_000))
            .unwrap_or(0)
    });
    let districts: Vec<u16> = s.market.as_ref().map_or_else(Vec::new, |m| {
        magnat_economy::firmlife::districts_with_seed(m)
            .into_iter()
            .map(|(d, _)| d)
            .collect()
    });
    PanelModel::Dashboard(Model {
        tier: h.tier(s),
        sites: h.sites.len(),
        employees: h.employees,
        managers: h.managers,
        cash,
        revenue_net: Money(revenue),
        result: Money(result),
        margin_bp,
        alerts: alerty(ctx),
        districts,
        has_firm: !h.firms.is_empty(),
        capital: cash,
        firm_cash: saldo_firmy(ctx),
    })
}

/// Saldo rachunku pierwszej firmy gracza. Zero, gdy firmy nie ma albo nie ma ksiąg.
fn saldo_firmy(ctx: &PanelCtx<'_>) -> Money {
    let s = ctx.session;
    let (Some(m), Some(key)) = (s.market.as_ref(), ctx.holdings.firms.first().copied()) else {
        return Money::ZERO;
    };
    let konto = m.account_of_firm(magnat_firms::firm_id(key));
    let books = s.app.world.get_resource::<magnat_economy::Books>();
    match (konto, books) {
        (Some(k), Some(b)) => b.balance(k).unwrap_or(Money::ZERO),
        _ => Money::ZERO,
    }
}

/// Trzy źródła alertów, w kolejności pilności: eskalacja polityki (bo zatrzymała
/// automat), pusta półka (bo sklep nie sprzedaje) i zakład pod kreską (bo pali gotówkę).
fn alerty(ctx: &PanelCtx<'_>) -> Vec<Alert> {
    let mut out = Vec::new();
    let s = ctx.session;
    let Some(m) = s.market.as_ref() else {
        return out;
    };
    for a in m.policy_inbox() {
        // Numer komunikatu pochodzi z reguły gracza i nie jest ograniczony
        // walidatorem, więc katalog może go nie znać — wtedy pokazujemy numer,
        // a nie panikujemy w środku rysowania pulpitu.
        let tresc = ctx.fmt_or(
            &format!("ui.policy.msg.{}", a.msg),
            "ui.policy.msg.unknown",
            &[("rule", &a.rule.to_string()), ("nr", &a.msg.to_string())],
        );
        let klucz = if a.ask {
            "ui.dashboard.alert.policy_ask"
        } else {
            "ui.dashboard.alert.policy"
        };
        out.push(Alert {
            sev: if a.ask { Sev::Bad } else { Sev::Warn },
            text: ctx.fmt(klucz, &[("co", &tresc)]),
            subject: Some(Subject::Site(a.site)),
        });
    }
    for site in &ctx.holdings.sites {
        let puste = m
            .goods_of(*site)
            .into_iter()
            .filter(|g| m.shelf_qty(*site, *g).is_some_and(|q| q.get() <= 0))
            .count();
        if puste > 0 {
            out.push(Alert {
                sev: Sev::Warn,
                text: ctx.fmt(
                    "ui.dashboard.alert.empty_shelf",
                    &[("ile", &puste.to_string())],
                ),
                subject: Some(Subject::Site(*site)),
            });
        }
    }
    if let Some(firms) = s.app.world.get_resource::<magnat_firms::Firms>() {
        for site in &ctx.holdings.sites {
            let strata = firms
                .site(*site)
                .and_then(|z| z.pnl.last())
                .is_some_and(|m| m.result().get() < 0);
            if strata {
                out.push(Alert {
                    sev: Sev::Bad,
                    text: ctx.text("ui.dashboard.alert.loss"),
                    subject: Some(Subject::Site(*site)),
                });
            }
        }
    }
    out
}

#[allow(clippy::needless_pass_by_value)]
fn render(
    ui: &mut egui::Ui,
    ctx: &PanelCtx<'_>,
    model: &PanelModel,
    view: &mut PanelView,
) -> PanelAction {
    let PanelModel::Dashboard(m) = model else {
        return PanelAction::None;
    };
    let th = ctx.theme;
    ui.horizontal_wrapped(|ui| {
        kpi(
            ui,
            th,
            &ctx.text("ui.kpi.cash"),
            &ctx.money(m.cash),
            Sev::of_money(m.cash),
        );
        kpi(
            ui,
            th,
            &ctx.text("ui.kpi.revenue_net"),
            &ctx.money(m.revenue_net),
            Sev::Normal,
        );
        kpi(
            ui,
            th,
            &ctx.text("ui.kpi.margin"),
            &m.margin_bp
                .map_or_else(|| ctx.text("ui.value.unknown"), super::widgets::percent_bp),
            m.margin_bp.map_or(Sev::Normal, |bp| {
                if bp < 0 {
                    Sev::Bad
                } else {
                    Sev::Good
                }
            }),
        );
        kpi(
            ui,
            th,
            &ctx.text("ui.kpi.headcount"),
            &ctx.int(i64::from(m.employees)),
            Sev::Normal,
        );
    });

    section(ui, th, &ctx.text("ui.dashboard.cash_flow"));
    // Zakres wykresu: miesiąc, rok, dziesięć lat. Trzy segmenty zamiast suwaka,
    // bo to są trzy pytania („jak idzie", „jaki był rok", „czy rosnę"), a nie
    // ciągła wielkość.
    let zakresy = [30u32, 360, 3_600];
    let etykiety: Vec<String> = zakresy
        .iter()
        .map(|d| ctx.fmt("ui.chart.days", &[("dni", &d.to_string())]))
        .collect();
    let mut i = zakresy.iter().position(|d| *d == view.days).unwrap_or(0);
    if super::widgets::picker_id(ui, th, &etykiety, &mut i, "dashboard.range") {
        view.days = zakresy[i];
    }
    let seria = ctx.session.metrics().series(MetricId::Cash);
    let dni = seria.days();
    if dni > 0 {
        let od = dni.saturating_sub(view.days.max(1));
        magnat_ui::chart::chart(ui, th, seria, od, dni, th.gap(20));
    } else {
        ui.label(ctx.text("ui.chart.no_data"));
    }

    section(ui, th, &ctx.text("ui.dashboard.alerts"));
    let mut akcja = PanelAction::None;
    if m.alerts.is_empty() {
        ui.label(ctx.text("ui.dashboard.no_alerts"));
    }
    for a in m.alerts.iter().take(8) {
        if alert(ui, th, a.sev, &a.text) {
            if let Some(s) = a.subject {
                akcja = PanelAction::Show(s);
            }
        }
    }

    section(ui, th, &ctx.text("ui.dashboard.holdings"));
    row(
        ui,
        th,
        &ctx.text("ui.dashboard.tier"),
        &ctx.text(&format!("ui.career.{}", m.tier.key())),
    );
    row(
        ui,
        th,
        &ctx.text("ui.dashboard.sites"),
        &ctx.int(i64::try_from(m.sites).unwrap_or(0)),
    );
    row(
        ui,
        th,
        &ctx.text("ui.dashboard.managers"),
        &ctx.int(i64::from(m.managers)),
    );

    if m.districts.is_empty() {
        return akcja;
    }
    section(ui, th, &ctx.text("ui.dashboard.expand"));
    let etykiety: Vec<String> = m
        .districts
        .iter()
        .map(|d| ctx.fmt("ui.district.nr", &[("nr", &d.to_string())]))
        .collect();
    super::widgets::picker(ui, th, &etykiety, &mut view.sel);
    let dzielnica = m.districts[view.sel.min(m.districts.len() - 1)];
    let (etykieta, cmd) = if m.has_firm {
        (
            "ui.dashboard.open_site",
            PlayerCommand::OpenSite {
                district: dzielnica,
                capex: Money(m.firm_cash.get().max(0) / 4),
            },
        )
    } else {
        (
            "ui.dashboard.found_firm",
            PlayerCommand::FoundFirm {
                district: dzielnica,
                capital: m.capital,
            },
        )
    };
    let blokada = crate::precheck(&ctx.session.view(), &cmd)
        .err()
        .map(|e| e.text(ctx.c, ctx.l));
    if action(ui, ctx, &ctx.text(etykieta), blokada) {
        akcja = PanelAction::cmd(cmd);
    }
    akcja
}
