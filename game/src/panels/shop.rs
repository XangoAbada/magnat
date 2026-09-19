//! Sklep: półka, utracone wizyty i konkurencja w zasięgu.
//!
//! # Wszystkie ceny brutto (`K-7`)
//!
//! Cena detaliczna to kwota, którą płaci kupujący, więc panel pokazuje brutto
//! i porównuje brutto do brutto. Marża jest liczona przez migawkę, nie tutaj —
//! panel nie ma prawa policzyć własnej, bo rozjechałaby się z tą, na której stoi
//! przecena.
//!
//! # Czym ten panel różni się od karty zakładu
//!
//! Karta inspekcji (dok prawy) odpowiada na „co się tu dzieje" i jest tekstem.
//! Panel (dok lewy) odpowiada na „co z tym zrobić" i ma przyciski: cena, cel zapasu,
//! asortyment. Drugiej karty nie rysuje (`DG-14`) — wysyła do inspekcji
//! [`PanelAction::Show`].

use magnat_core::{Money, SiteId, Subject};
use magnat_economy::{ShopPanelSnapshot, TRACE_DAYS};
use magnat_ui::{ColorToken, DataSource, TextRole};

use super::widgets::{action, alert, picker, row, section, Sev};
use super::{PanelAction, PanelCtx, PanelDesc, PanelId, PanelModel, PanelView};
use crate::PlayerCommand;

pub(super) const DESC: PanelDesc = PanelDesc {
    id: PanelId::Shop,
    title: "ui.panel.shop",
    deps: &[DataSource::Clock, DataSource::Market, DataSource::Locale],
    build,
    render,
    min_tier: None,
};

/// O ile zmienia cenę jedno kliknięcie: pięć procent. Krok, a nie pole tekstowe,
/// bo pierwsza decyzja cenowa gracza ma być **jednym kliknięciem** (§5.12 pkt 3).
const KROK_CENY_BP: i64 = 500;

pub struct Model {
    /// Zakłady handlowe gracza i ich nazwy.
    pub sites: Vec<SiteId>,
    pub names: Vec<String>,
    /// Migawka **wybranego** zakładu. Jedna, nie dwieście: model unieważnia się
    /// razem z wyborem gracza (`PanelCtx::sel`), więc przełączenie sklepu przelicza
    /// tę jedną, a nie wszystkie.
    pub snapshot: Option<Box<ShopPanelSnapshot>>,
}

fn build(ctx: &PanelCtx<'_>) -> PanelModel {
    let s = ctx.session;
    let Some(m) = s.market.as_ref() else {
        return PanelModel::Shop(Model {
            sites: Vec::new(),
            names: Vec::new(),
            snapshot: None,
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
    let snapshot = sites
        .get(ctx.sel.min(sites.len().saturating_sub(1)))
        .and_then(|site| {
            let do_ = s.tick();
            let od = magnat_core::Tick(
                do_.get()
                    .saturating_sub(TRACE_DAYS as u64 * magnat_core::time::MINUTES_PER_DAY),
            );
            m.shop_panel(*site, od, do_).map(Box::new)
        });
    PanelModel::Shop(Model {
        sites,
        names,
        snapshot,
    })
}

/// `ponytail:` jedna funkcja na cały panel. Sufit: przy dwóch kolejnych sekcjach
/// przekroczy 250. Ścieżka wyjścia jest oczywista i tania — każda sekcja
/// (`półka`, `cena`, `zapas`, `utracone`, `konkurencja`) jest już osobnym blokiem
/// i wychodzi do własnej funkcji bez zmiany kolejności rysowania. Dziś podział
/// rozbiłby jeden ekran na pięć plików i nic by nie kupił.
fn render(
    ui: &mut egui::Ui,
    ctx: &PanelCtx<'_>,
    model: &PanelModel,
    view: &mut PanelView,
) -> PanelAction {
    let PanelModel::Shop(m) = model else {
        return PanelAction::None;
    };
    let th = ctx.theme;
    if m.sites.is_empty() {
        ui.label(ctx.text("ui.panel.shop.no_sites"));
        return PanelAction::None;
    }
    picker(ui, th, &m.names, &mut view.sel);
    let site = m.sites[view.sel.min(m.sites.len() - 1)];
    let Some(snap) = m.snapshot.as_deref() else {
        ui.label(ctx.text("ui.panel.shop.no_data"));
        return PanelAction::None;
    };
    let mut akcja = PanelAction::None;
    if action(ui, ctx, &ctx.text("ui.panel.shop.show_card"), None) {
        akcja = PanelAction::Show(Subject::Site(site));
    }

    section(ui, th, &ctx.text("ui.panel.shop.shelves"));
    if snap.shelves.is_empty() {
        ui.label(ctx.text("ui.panel.shop.empty_shelf"));
    }
    for (i, w) in snap.shelves.iter().enumerate() {
        let nazwa = snap.good_key(w.good).to_string();
        ui.horizontal(|ui| {
            if ui
                .selectable_label(
                    i == view.tab,
                    egui::RichText::new(&nazwa).font(th.font(TextRole::Body)),
                )
                .clicked()
            {
                view.tab = i;
            }
            ui.label(
                egui::RichText::new(ctx.money(w.price))
                    .font(th.font(TextRole::Body))
                    .color(th.color(ColorToken::TextPrimary)),
            );
            ui.label(
                egui::RichText::new(super::widgets::percent_bp(w.margin_bp))
                    .font(th.font(TextRole::Micro))
                    .color(th.color(if w.margin_bp < 0 {
                        ColorToken::Danger
                    } else {
                        ColorToken::TextSecondary
                    })),
            );
            ui.label(
                egui::RichText::new(ctx.fmt(
                    "ui.panel.shop.cover",
                    &[("dni", &w.days_of_cover.to_string())],
                ))
                .font(th.font(TextRole::Micro))
                .color(th.color(ColorToken::TextSecondary)),
            );
        });
    }

    let Some(w) = snap
        .shelves
        .get(view.tab.min(snap.shelves.len().saturating_sub(1)))
    else {
        return akcja;
    };
    let klucz = snap.good_key(w.good).to_string();

    section(ui, th, &ctx.text("ui.panel.shop.price"));
    ui.horizontal(|ui| {
        for (etykieta, znak) in [
            ("ui.panel.shop.price_down", -1i64),
            ("ui.panel.shop.price_up", 1),
        ] {
            let nowa = Money(w.price.get() + w.price.get() * KROK_CENY_BP * znak / 10_000);
            let cmd = PlayerCommand::SetPrice {
                site,
                good: klucz.clone(),
                price: nowa,
            };
            if przycisk(ui, ctx, etykieta, &cmd) {
                akcja = PanelAction::cmd(cmd);
            }
        }
    });

    section(ui, th, &ctx.text("ui.panel.shop.restock"));
    // Co jest ustawione **teraz** (`DI-38`). Bez tego gracz naciskał „7 dni",
    // nie wiedząc, czy to zmiana, czy potwierdzenie tego, co już stoi — a odczyt
    // (`Market::restock_days`) istniał od M9e i nie miał ani jednego czytelnika.
    let biezacy = ctx
        .session
        .market
        .as_ref()
        .and_then(|m| m.restock_days(site, w.good));
    ui.label(
        egui::RichText::new(match biezacy {
            Some(d) => ctx.fmt("ui.panel.shop.restock_now", &[("dni", &d.to_string())]),
            None => ctx.text("ui.panel.shop.restock_auto"),
        })
        .font(th.font(TextRole::Micro))
        .color(th.color(ColorToken::TextSecondary)),
    );
    ui.horizontal(|ui| {
        for dni in [3u16, 7, 14] {
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

    // Zdjęcie towaru z półki: asortyment jedzie **całą listą**, bo tym jest asortyment.
    section(ui, th, &ctx.text("ui.panel.shop.assortment"));
    let bez_tego: Vec<String> = snap
        .shelves
        .iter()
        .filter(|x| x.good != w.good)
        .map(|x| snap.good_key(x.good).to_string())
        .collect();
    let cmd = PlayerCommand::SetShelfAssortment {
        site,
        goods: bez_tego,
    };
    if przycisk(ui, ctx, "ui.panel.shop.remove_good", &cmd) {
        akcja = PanelAction::cmd(cmd);
    }

    section(ui, th, &ctx.text("ui.panel.shop.lost"));
    if snap.lost_sales.recent.is_empty() {
        ui.label(ctx.text("ui.panel.shop.no_lost"));
    }
    for l in snap.lost_sales.recent.iter().rev().take(6) {
        let powod = magnat_ui::inspect::reason::reject_cause(ctx.c, ctx.l, l.cause);
        let tekst = l.went_to.map_or_else(
            || ctx.fmt("ui.panel.shop.lost_row", &[("powod", &powod)]),
            |s| {
                ctx.fmt(
                    "ui.panel.shop.lost_row_to",
                    &[
                        ("powod", &powod),
                        ("gdzie", &s.entity().index().to_string()),
                    ],
                )
            },
        );
        if alert(ui, th, Sev::Warn, &tekst) {
            if let Some(s) = l.went_to {
                akcja = PanelAction::Show(Subject::Site(s));
            }
        }
    }

    section(ui, th, &ctx.text("ui.panel.shop.competition"));
    if snap.competition.is_empty() {
        ui.label(ctx.text("ui.panel.shop.no_competition"));
    }
    for k in snap.competition.iter().take(5) {
        row(
            ui,
            th,
            &ctx.fmt(
                "ui.panel.shop.competitor",
                &[("nr", &k.site.entity().index().to_string())],
            ),
            &ctx.fmt(
                "ui.panel.shop.competitor_age",
                &[
                    ("m", &k.distance_m.to_string()),
                    ("dni", &k.observed_age_days.to_string()),
                ],
            ),
        );
    }
    akcja
}

/// Przycisk komendy: wygaszony z powodem, jeśli `precheck` odmawia.
fn przycisk(ui: &mut egui::Ui, ctx: &PanelCtx<'_>, key: &str, cmd: &PlayerCommand) -> bool {
    let blokada = crate::precheck(&ctx.session.view(), cmd)
        .err()
        .map(|e| e.text(ctx.c, ctx.l));
    action(ui, ctx, &ctx.text(key), blokada)
}
