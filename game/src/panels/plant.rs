//! Zakład: linie produkcyjne jako Gantt, zapas i decyzja o zamknięciu.
//!
//! Gantt odpowiada na „co i kiedy": każda linia to wiersz, każda szarża to pasek.
//! Linia stojąca dostaje pasek w kolorze i **z ramką** — powód postoju jest obok
//! słowami, bo kolor nigdy nie niesie znaczenia sam (`ui-design.md` §7).
//!
//! Komenda panelu: **zamknij zakład**. To jest ta sama droga, którą zamyka zakład
//! decyzja tieru taktycznego firmy AI — załoga schodzi z etatów, półka znika,
//! a odprawy wypłaca firma, która nadal istnieje.

use magnat_core::{LineStopCause, SiteId, Subject};
use magnat_ui::{BarState, DataSource, GanttBar, GanttView};

use super::widgets::{action, picker, row, section};
use super::{PanelAction, PanelCtx, PanelDesc, PanelId, PanelModel, PanelView};
use crate::PlayerCommand;

pub(super) const DESC: PanelDesc = PanelDesc {
    id: PanelId::Plant,
    title: "ui.panel.plant",
    deps: &[DataSource::Clock, DataSource::Market, DataSource::Locale],
    build,
    render,
    min_tier: Some(crate::CareerTier::Company),
};

/// Ile dób pokazuje oś Gantta: dekada (`K-15` — dekada dzieli miesiąc bez reszty).
const OKNO_DOB: u64 = 10;

pub struct Model {
    pub sites: Vec<SiteId>,
    pub names: Vec<String>,
    pub bars: Vec<GanttBar>,
    /// Postoje wybranego zakładu: numer linii i powód.
    pub stops: Vec<(u16, LineStopCause)>,
    /// Zapas w slotach magazynowych zakładu: rola slotu i masa w gramach.
    pub storage: Vec<(magnat_supply::WarehouseRole, i64)>,
}

fn build(ctx: &PanelCtx<'_>) -> PanelModel {
    let s = ctx.session;
    let sites = ctx.holdings.sites.clone();
    let names = sites
        .iter()
        .map(|x| {
            ctx.fmt(
                "ui.subject.site",
                &[("nr", &x.entity().index().to_string())],
            )
        })
        .collect();
    let site = sites
        .get(ctx.sel.min(sites.len().saturating_sub(1)))
        .copied();
    let (mut bars, mut stops, mut storage) = (Vec::new(), Vec::new(), Vec::new());
    if let (Some(site), Some(m)) = (site, s.market.as_ref()) {
        let chain = m.chain();
        let c = chain.lock();
        if let Some(z) = c.plant.get(site) {
            for (i, l) in z.lines.iter().enumerate() {
                let wiersz = u16::try_from(i).unwrap_or(u16::MAX);
                if let Some(powod) = powod_postoju(l.state) {
                    stops.push((wiersz, powod));
                }
                // Pasek szarży: od jej startu do przewidywanego końca. Linia, która
                // nie mieli, nie dostaje paska — pusty wiersz mówi „nic tu nie idzie"
                // lepiej niż pasek o zerowej szerokości.
                if let magnat_supply::LineState::Running {
                    recipe,
                    started,
                    ends,
                    ..
                } = l.state
                {
                    bars.push(GanttBar {
                        row: wiersz,
                        label: recipe.0.to_string(),
                        from: started.0,
                        to: ends.0,
                        state: BarState::Running,
                    });
                }
                if let magnat_supply::LineState::Maintenance { until } = l.state {
                    bars.push(GanttBar {
                        row: wiersz,
                        label: String::new(),
                        from: l.next_maintenance.0.min(until.0),
                        to: until.0,
                        state: BarState::Late,
                    });
                }
            }
            for slot in c.store.slots_of(site) {
                storage.push((slot.role, slot.used_mass().get()));
            }
        }
    }
    PanelModel::Plant(Model {
        sites,
        names,
        bars,
        stops,
        storage,
    })
}

/// Klucz tekstu roli magazynu: `ui.warehouse.<key>`.
const fn rola_key(r: magnat_supply::WarehouseRole) -> &'static str {
    use magnat_supply::WarehouseRole as W;
    match r {
        W::Input => "Input",
        W::Output => "Output",
        W::Backroom => "Backroom",
        W::Shelf => "Shelf",
        W::Distribution => "Distribution",
    }
}

/// Powód postoju linii — ładunek `DecisionReason::ProductionHalted`, ten sam,
/// który czyta karta zakładu. Linia w biegu, w przezbrojeniu i na przeglądzie
/// **nie stoi**: przezbrojenie i przegląd są pracą, a nie awarią.
const fn powod_postoju(s: magnat_supply::LineState) -> Option<LineStopCause> {
    match s {
        magnat_supply::LineState::Broken { .. } => Some(LineStopCause::Wear),
        magnat_supply::LineState::Starved { .. } => Some(LineStopCause::Starved),
        magnat_supply::LineState::Blocked { .. } => Some(LineStopCause::Blocked),
        _ => None,
    }
}

fn render(
    ui: &mut egui::Ui,
    ctx: &PanelCtx<'_>,
    model: &PanelModel,
    view: &mut PanelView,
) -> PanelAction {
    let PanelModel::Plant(m) = model else {
        return PanelAction::None;
    };
    let th = ctx.theme;
    if m.sites.is_empty() {
        ui.label(ctx.text("ui.plant.no_sites"));
        return PanelAction::None;
    }
    picker(ui, th, &m.names, &mut view.sel);
    let site = m.sites[view.sel.min(m.sites.len() - 1)];
    let mut akcja = PanelAction::None;

    section(ui, th, &ctx.text("ui.plant.schedule"));
    if m.bars.is_empty() {
        ui.label(ctx.text("ui.plant.idle"));
    } else {
        let teraz = ctx.session.tick().get();
        let okno = OKNO_DOB * magnat_core::time::MINUTES_PER_DAY;
        let mut g = GanttView::default();
        g.set(m.bars.clone());
        g.show(
            ui,
            th,
            teraz.saturating_sub(okno / 2),
            teraz + okno / 2,
            th.gap(20),
        );
    }

    if !m.stops.is_empty() {
        section(ui, th, &ctx.text("ui.plant.stops"));
        for (linia, powod) in &m.stops {
            row(
                ui,
                th,
                &ctx.fmt("ui.plant.line", &[("nr", &linia.to_string())]),
                &ctx.text(&format!("ui.line_stop.{}", powod.name())),
            );
        }
    }

    section(ui, th, &ctx.text("ui.plant.storage"));
    if m.storage.is_empty() {
        ui.label(ctx.text("ui.plant.no_storage"));
    }
    for (rola, masa) in m.storage.iter().take(8) {
        // Nazwa roli idzie przez katalog: `Debug` enuma jest tekstem dla dewelopera
        // i nie ma prawa trafić na ekran gracza (CLAUDE.md).
        let nazwa = ctx.text(&format!("ui.warehouse.{}", rola_key(*rola)));
        row(ui, th, &nazwa, &ctx.int(masa / 1000));
    }

    if action(ui, ctx, &ctx.text("ui.plant.show_card"), None) {
        akcja = PanelAction::Show(Subject::Site(site));
    }
    section(ui, th, &ctx.text("ui.plant.decision"));
    let cmd = PlayerCommand::CloseSite { site };
    let blokada = crate::precheck(&ctx.session.view(), &cmd)
        .err()
        .map(|e| e.text(ctx.c, ctx.l));
    if action(ui, ctx, &ctx.text("ui.plant.close"), blokada) {
        akcja = PanelAction::cmd(cmd);
    }
    akcja
}
