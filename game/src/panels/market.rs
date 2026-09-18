//! Rynek: rozkład cen towaru w mieście, udziały i moje oferty.
//!
//! # Nie ma „ceny mleka w mieście" (PRD §6.1)
//!
//! Jest rozkład cen po ofertach i tak go panel pokazuje: minimum, mediana,
//! percentyle, maksimum i liczba ofert. Jedna liczba nazwana „ceną rynkową"
//! byłaby zmienną, której w tej gospodarce nie ma.
//!
//! # Detal i hurt nigdy w jednym szeregu (`K-7`)
//!
//! Rozkład liczy się z ofert detalicznych, czyli **brutto**, i panel to podpisuje.
//! Oferta hurtowa jest netto i mieszkanie ich w jednej kolumnie znaczyłoby
//! porównywanie kwot o dwóch różnych podstawach.

use magnat_core::{GoodId, Money, SiteId};
use magnat_economy::PriceDist;
use magnat_ui::DataSource;

use super::widgets::{action, picker, row, section};
use super::{PanelAction, PanelCtx, PanelDesc, PanelId, PanelModel, PanelView};
use crate::PlayerCommand;

pub(super) const DESC: PanelDesc = PanelDesc {
    id: PanelId::Market,
    title: "ui.panel.market",
    deps: &[DataSource::Clock, DataSource::Market, DataSource::Locale],
    build,
    render,
    min_tier: Some(crate::CareerTier::FirstBusiness),
};

pub struct Model {
    /// Towary, którymi handluje gracz — o te pyta, bo o cudze nie ma jak zapytać
    /// inaczej niż przez ofertę, którą i tak widzi w karcie konkurenta.
    /// Towar i jego **klucz tekstowy** — ten sam, który jedzie w komendzie.
    /// Etykieta i klucz to jedna rzecz, bo etykietą towaru jest jego klucz; gdyby
    /// kiedyś się rozeszły, komenda ma dostać klucz, a nie napis.
    pub goods: Vec<(GoodId, String)>,
    /// Rozkład cen wybranego towaru po wszystkich ofertach miasta.
    pub dist: Option<PriceDist>,
    /// Udział gracza w obrocie tym towarem, w punktach bazowych.
    pub share_bp: Option<u16>,
    /// Moje oferty tego towaru: zakład i cena brutto.
    pub mine: Vec<(SiteId, Money)>,
}

fn build(ctx: &PanelCtx<'_>) -> PanelModel {
    let s = ctx.session;
    let Some(m) = s.market.as_ref() else {
        return PanelModel::Market(Model {
            goods: Vec::new(),
            dist: None,
            share_bp: None,
            mine: Vec::new(),
        });
    };
    let mut goods: Vec<(GoodId, String)> = Vec::new();
    for site in &ctx.holdings.sites {
        for g in m.goods_of(*site) {
            // Towar bez klucza w katalogu **nie wchodzi na listę**: komenda i tak
            // odrzuciłaby numer, a wiersz, po którego kliknięciu nic się nie dzieje,
            // jest gorszy od jego braku.
            if !goods.iter().any(|(x, _)| *x == g) {
                if let Some(k) = m.good_key(g) {
                    goods.push((g, k));
                }
            }
        }
    }
    goods.sort_unstable_by_key(|(g, _)| g.0);
    let wybrany = goods
        .get(ctx.sel.min(goods.len().saturating_sub(1)))
        .map(|(g, _)| *g);
    let (dist, share_bp, mine) = wybrany.map_or((None, None, Vec::new()), |g| {
        let rozklad = m
            .balance_sample()
            .prices
            .into_iter()
            .find(|d| d.good == g);
        let moje = ctx
            .holdings
            .sites
            .iter()
            .filter_map(|s| m.price_at(*s, g).map(|c| (*s, c)))
            .collect();
        (
            rozklad,
            m.turnover_share_bp(g, None, &ctx.holdings.sites),
            moje,
        )
    });
    PanelModel::Market(Model {
        goods,
        dist,
        share_bp,
        mine,
    })
}

fn render(
    ui: &mut egui::Ui,
    ctx: &PanelCtx<'_>,
    model: &PanelModel,
    view: &mut PanelView,
) -> PanelAction {
    let PanelModel::Market(m) = model else {
        return PanelAction::None;
    };
    let th = ctx.theme;
    if m.goods.is_empty() {
        ui.label(ctx.text("ui.market.no_goods"));
        return PanelAction::None;
    }
    let etykiety: Vec<String> = m.goods.iter().map(|(_, k)| k.clone()).collect();
    picker(ui, th, &etykiety, &mut view.sel);
    let klucz = m.goods[view.sel.min(m.goods.len() - 1)].1.clone();

    section(ui, th, &ctx.text("ui.market.dist_gross"));
    match &m.dist {
        None => {
            ui.label(ctx.text("ui.market.no_offers"));
        }
        Some(d) => {
            row(ui, th, &ctx.text("ui.market.min"), &ctx.money(d.min));
            row(ui, th, &ctx.text("ui.market.p10"), &ctx.money(d.p10));
            row(ui, th, &ctx.text("ui.market.median"), &ctx.money(d.p50));
            row(ui, th, &ctx.text("ui.market.p90"), &ctx.money(d.p90));
            row(ui, th, &ctx.text("ui.market.max"), &ctx.money(d.max));
            row(
                ui,
                th,
                &ctx.text("ui.market.offers"),
                &ctx.int(i64::from(d.offers)),
            );
        }
    }

    section(ui, th, &ctx.text("ui.market.share"));
    row(
        ui,
        th,
        &ctx.text("ui.market.my_share"),
        &m.share_bp.map_or_else(
            || ctx.text("ui.market.no_turnover"),
            |bp| super::widgets::percent_bp(i32::from(bp)),
        ),
    );

    section(ui, th, &ctx.text("ui.market.my_offers"));
    let mut akcja = PanelAction::None;
    for (site, cena) in &m.mine {
        ui.horizontal(|ui| {
            ui.label(ctx.fmt(
                "ui.market.offer_row",
                &[
                    ("gdzie", &site.entity().index().to_string()),
                    ("cena", &ctx.money(*cena)),
                ],
            ));
            // Wyrównanie do mediany: jedno kliknięcie, które gracz rozumie bez
            // liczenia. Mediana, a nie minimum — pod minimum schodzi wojna cenowa,
            // a tę wypowiada się świadomie.
            let Some(d) = m.dist.as_ref() else { return };
            let cmd = PlayerCommand::SetPrice {
                site: *site,
                good: klucz.clone(),
                price: d.p50,
            };
            let blokada = crate::precheck(&ctx.session.view(), &cmd)
                .err()
                .map(|e| e.text(ctx.c, ctx.l));
            if action(ui, ctx, &ctx.text("ui.market.match_median"), blokada) {
                akcja = PanelAction::cmd(cmd);
            }
        });
    }
    akcja
}
