//! Łańcuch dostaw: graf dostawców i odbiorców, umowy i ryzyka.
//!
//! **Jeden dostawca to czerwony węzeł** — i to jest cała treść tego panelu.
//! Gracz ma zobaczyć, gdzie jego łańcuch ma jedno ogniwo, zanim to ogniwo pęknie.
//!
//! Komenda panelu: **cel zapasu**. Zapas jest jedyną rzeczą, którą właściciel
//! naprawdę steruje w łańcuchu — reszta jest wynikiem umów i tras.

use magnat_core::{GoodId, SiteId, Subject};
use magnat_ui::{DataSource, GraphEdge, GraphNode, GraphView};

use super::widgets::{action, picker, row, section};
use super::{PanelAction, PanelCtx, PanelDesc, PanelId, PanelModel, PanelView};
use crate::PlayerCommand;

pub(super) const DESC: PanelDesc = PanelDesc {
    id: PanelId::Supply,
    title: "ui.panel.supply",
    deps: &[DataSource::Clock, DataSource::Market, DataSource::Locale],
    build,
    render,
    min_tier: Some(crate::CareerTier::FirstBusiness),
};

pub struct Model {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    /// Umowy dostaw, których stroną jest zakład gracza: towar, dokąd, do kiedy.
    pub contracts: Vec<(GoodId, SiteId, u64)>,
    /// Towary wybranego zakładu — wejście celu zapasu.
    pub goods: Vec<(GoodId, String)>,
    pub site: Option<SiteId>,
    pub names: Vec<String>,
    pub sites: Vec<SiteId>,
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
    let site = sites.get(ctx.sel.min(sites.len().saturating_sub(1))).copied();
    let Some(m) = s.market.as_ref() else {
        return PanelModel::Supply(Model {
            nodes: Vec::new(),
            edges: Vec::new(),
            contracts: Vec::new(),
            goods: Vec::new(),
            site,
            names,
            sites,
        });
    };
    let goods = site.map_or_else(Vec::new, |x| {
        m.goods_of(x)
            .into_iter()
            .map(|g| (g, m.good_key(g).unwrap_or_else(|| g.0.to_string())))
            .collect()
    });

    // Graf: węzły to zakłady, krawędzie to umowy. Buduje się z rejestru umów,
    // a nie z tras — trasa jest wykonaniem, umowa jest topologią.
    let chain = m.chain();
    let c = chain.lock();
    let mut wezly: Vec<SiteId> = Vec::new();
    let mut krawedzie: Vec<(SiteId, SiteId)> = Vec::new();
    let mut contracts = Vec::new();
    for k in c.b2b.contracts() {
        let nasz = sites.contains(&k.deliver_to) || sites.contains(&k.deliver_from);
        if !nasz {
            continue;
        }
        for x in [k.deliver_from, k.deliver_to] {
            if !wezly.contains(&x) {
                wezly.push(x);
            }
        }
        krawedzie.push((k.deliver_from, k.deliver_to));
        contracts.push((k.good, k.deliver_to, k.valid_to.0));
    }
    drop(c);

    // Ryzyko: zakład, do którego prowadzi dokładnie jedna krawędź wejściowa,
    // stoi na jednym dostawcy.
    let nodes = wezly
        .iter()
        .map(|x| {
            let wejscia = krawedzie.iter().filter(|(_, b)| b == x).count();
            GraphNode {
                label: x.entity().index().to_string(),
                subject: Some(Subject::Site(*x)),
                risk: wejscia == 1,
            }
        })
        .collect();
    let edges = krawedzie
        .iter()
        .filter_map(|(a, b)| {
            Some(GraphEdge {
                from: u32::try_from(wezly.iter().position(|x| x == a)?).ok()?,
                to: u32::try_from(wezly.iter().position(|x| x == b)?).ok()?,
                weight_permille: 500,
            })
        })
        .collect();
    PanelModel::Supply(Model {
        nodes,
        edges,
        contracts,
        goods,
        site,
        names,
        sites,
    })
}

fn render(
    ui: &mut egui::Ui,
    ctx: &PanelCtx<'_>,
    model: &PanelModel,
    view: &mut PanelView,
) -> PanelAction {
    let PanelModel::Supply(m) = model else {
        return PanelAction::None;
    };
    let th = ctx.theme;
    if m.sites.is_empty() {
        ui.label(ctx.text("ui.panel.supply.no_sites"));
        return PanelAction::None;
    }
    picker(ui, th, &m.names, &mut view.sel);
    let mut akcja = PanelAction::None;

    section(ui, th, &ctx.text("ui.panel.supply.graph"));
    if m.nodes.is_empty() {
        ui.label(ctx.text("ui.panel.supply.no_contracts"));
    } else {
        // Widok grafu trzyma układ między klatkami, ale jest **stanem widoku**,
        // nie modelu: przewinięcie nie ma przeliczać warstw.
        let mut g = GraphView::default();
        g.set(m.nodes.clone(), m.edges.clone());
        if let Some(s) = g.show(ui, th, th.gap(30)) {
            akcja = PanelAction::Show(s);
        }
        let ryzykowne = m.nodes.iter().filter(|n| n.risk).count();
        if ryzykowne > 0 {
            ui.label(ctx.fmt(
                "ui.panel.supply.single_source",
                &[("ile", &ryzykowne.to_string())],
            ));
        }
    }

    section(ui, th, &ctx.text("ui.panel.supply.contracts"));
    if m.contracts.is_empty() {
        ui.label(ctx.text("ui.panel.supply.no_contracts"));
    }
    for (good, do_, valid_to) in m.contracts.iter().take(10) {
        let dni = valid_to.saturating_sub(ctx.session.tick().get())
            / magnat_core::time::MINUTES_PER_DAY;
        row(
            ui,
            th,
            &ctx.fmt(
                "ui.panel.supply.contract_row",
                &[
                    ("towar", &good.0.to_string()),
                    ("gdzie", &do_.entity().index().to_string()),
                ],
            ),
            &ctx.fmt("ui.panel.supply.days_left", &[("dni", &dni.to_string())]),
        );
    }

    let (Some(site), Some((_, klucz))) = (
        m.site,
        m.goods.get(view.tab.min(m.goods.len().saturating_sub(1))),
    ) else {
        return akcja;
    };
    section(ui, th, &ctx.text("ui.panel.supply.restock"));
    let etykiety: Vec<String> = m.goods.iter().map(|(_, k)| k.clone()).collect();
    // Własny identyfikator listy: panel ma **dwie** listy rozwijane i wspólne id
    // sprawiłoby, że rozwijają się razem.
    super::widgets::picker_id(ui, th, &etykiety, &mut view.tab, "supply.goods");
    ui.horizontal(|ui| {
        for dni in [3u16, 7, 14, 30] {
            let cmd = PlayerCommand::SetRestockTarget {
                site,
                good: klucz.clone(),
                days: dni,
            };
            let blokada = crate::precheck(&ctx.session.view(), &cmd)
                .err()
                .map(|e| e.text(ctx.c, ctx.l));
            if action(
                ui,
                ctx,
                &ctx.fmt("ui.panel.shop.restock_days", &[("dni", &dni.to_string())]),
                blokada,
            ) {
                akcja = PanelAction::cmd(cmd);
            }
        }
    });
    akcja
}
