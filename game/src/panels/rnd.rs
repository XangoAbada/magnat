//! Badania i rozwój: drzewo, projekt w toku, patenty.
//!
//! # Czym ten panel różni się od reguły AI
//!
//! Firma AI bierze **najtańszy osiągalny węzeł** i nie ma w tym wyboru. Gracz
//! wskazuje węzeł sam — i to jest cała różnica (`FF-9`). Wszystko poza wyborem
//! liczy się tym samym kodem: koszt zamrożony w chwili startu, zniżka za rok
//! „światowy", patent na dwadzieścia lat, blokada cudzym patentem.
//!
//! # Tempo bierze się z pomiaru, nie z drugiego wzoru
//!
//! „Ile jeszcze miesięcy" wymaga tempa punktów badawczych na dobę, a to liczy
//! `rnd::mrp_per_day` z obsady i budżetu materiałowego. Panel **nie powtarza tego
//! rachunku** — dzieli zebrane punkty przez liczbę dób od startu projektu.
//! Druga kopia wzoru rozjechałaby się z pierwszą przy pierwszej zmianie kalibracji,
//! a ta liczba jest przy okazji uczciwsza: mówi, jak firmie idzie **naprawdę**,
//! a nie ile powinno jej iść.

use magnat_core::{SiteId, Subject, TechId};
use magnat_ui::DataSource;

use super::widgets::{action, picker, row, section};
use super::{PanelAction, PanelCtx, PanelDesc, PanelId, PanelModel, PanelView};
use crate::PlayerCommand;

pub(super) const DESC: PanelDesc = PanelDesc {
    id: PanelId::Rnd,
    title: "ui.panel.rnd",
    deps: &[DataSource::Clock, DataSource::World, DataSource::Locale],
    build,
    render,
    min_tier: Some(crate::CareerTier::FirstBusiness),
};

/// Ile węzłów pokazać na liście „do zbadania".
const WEZLOW: usize = 10;

/// Węzeł drzewa w postaci, w jakiej czyta go panel.
pub struct Wezel {
    pub key: String,
    pub name: String,
    pub branch: String,
    pub cost_rp: u32,
    pub world_year: i32,
    /// Czy firma może go **zacząć**: nie zna, ma warunki, nikt go nie blokuje.
    pub osiagalny: bool,
    /// Czy odkrycie da patent (przed rokiem „światowym").
    pub patentowy: bool,
}

/// Projekt w toku.
pub struct Projekt {
    pub name: String,
    pub done_permille: u32,
    /// `None` = za wcześnie, żeby zmierzyć tempo (pierwsza doba projektu).
    pub months_left: Option<u16>,
    pub breakthroughs: u8,
}

pub struct Model {
    pub sites: Vec<SiteId>,
    pub names: Vec<String>,
    pub known: usize,
    pub projekt: Option<Projekt>,
    pub wezly: Vec<Wezel>,
    /// Patenty firmy: nazwa i ile życia im zostało, w promilach.
    pub patenty: Vec<(String, u32)>,
    /// Licencje, które firma płaci: nazwa węzła i stawka w bp.
    pub licencje: Vec<(String, u16)>,
}

/// Nazwa węzła w języku gracza. Klucz tekstowy jest w drzewie, tłumaczenie
/// w `data/locale/` — węzeł bez wpisu pokazuje swój klucz, a nie pusty napis.
pub(crate) fn nazwa_technologii(
    ctx: &PanelCtx<'_>,
    tree: &magnat_firms::TechTree,
    t: TechId,
) -> String {
    tree.get(t).map_or_else(
        || format!("#{}", t.0),
        |n| ctx.fmt_or(&format!("ui.tech.{}", n.key), "ui.tech.unknown", &[]),
    )
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
    let pusty = |sites: Vec<SiteId>, names: Vec<String>| {
        PanelModel::Rnd(Model {
            sites,
            names,
            known: 0,
            projekt: None,
            wezly: Vec::new(),
            patenty: Vec::new(),
            licencje: Vec::new(),
        })
    };
    let world = &s.app.world;
    let (Some(site), Some(data), Some(firms)) = (
        sites
            .get(ctx.sel.min(sites.len().saturating_sub(1)))
            .copied(),
        world.get_resource::<magnat_firms::RndData>(),
        world.get_resource::<magnat_firms::Firms>(),
    ) else {
        return pusty(sites, names);
    };
    let Some(key) = firms.site(site).map(|z| z.firm) else {
        return pusty(sites, names);
    };
    let st = firms.rnd();
    let now = magnat_core::SimMinute(s.tick().get());
    let dzien = s.tick().get() / magnat_core::time::MINUTES_PER_DAY;

    let projekt = st.projects.get(&key).map(|p| {
        let dni = (now.get().saturating_sub(p.started.get())) / magnat_core::time::MINUTES_PER_DAY;
        // Tempo zmierzone, nie wyliczone — patrz nagłówek modułu.
        let tempo = p.done_mrp.checked_div(dni).unwrap_or(0);
        Projekt {
            name: nazwa_technologii(ctx, &data.tree, p.tech),
            done_permille: (p.done_mrp * 1_000)
                .checked_div(p.cost_mrp)
                .map_or(0, |v| u32::try_from(v).unwrap_or(1_000)),
            months_left: (tempo > 0).then(|| p.months_left(tempo)),
            breakthroughs: p.breakthroughs,
        }
    });

    let mut wezly: Vec<Wezel> = data
        .tree
        .nodes
        .iter()
        .filter(|n| !st.knows(key, n.id))
        .map(|n| Wezel {
            key: n.key.to_string(),
            name: nazwa_technologii(ctx, &data.tree, n.id),
            branch: data
                .tree
                .branches
                .get(n.branch.0 as usize)
                .map_or_else(String::new, |b| {
                    ctx.fmt_or(&format!("ui.tech_branch.{b}"), "ui.tech.unknown", &[])
                }),
            cost_rp: n.effective_cost_rp(dzien, data.tuning.world_known_discount_bp),
            world_year: n.world_year,
            osiagalny: n.prereqs.iter().all(|p| st.knows(key, *p))
                && st.blocked_by(key, n.id, now).is_none(),
            patentowy: n.patentable(dzien),
        })
        .collect();
    // Osiągalne najpierw, w obrębie grupy tańsze najpierw: tak wygląda lista, z której
    // da się wybrać, a nie spis treści katalogu.
    wezly.sort_unstable_by(|a, b| {
        b.osiagalny
            .cmp(&a.osiagalny)
            .then(a.cost_rp.cmp(&b.cost_rp))
            .then(a.key.cmp(&b.key))
    });
    wezly.truncate(WEZLOW);

    let patenty = st
        .patents
        .values()
        .filter(|p| p.owner == key && p.active(now))
        .map(|p| {
            (
                nazwa_technologii(ctx, &data.tree, p.tech),
                p.life_left_permille(now),
            )
        })
        .collect();
    let licencje = st
        .licenses
        .values()
        .filter(|l| l.licensee == key)
        .map(|l| (nazwa_technologii(ctx, &data.tree, l.tech), l.royalty_bp))
        .collect();

    PanelModel::Rnd(Model {
        sites,
        names,
        known: st.known.get(&key).map_or(0, Vec::len),
        projekt,
        wezly,
        patenty,
        licencje,
    })
}

fn render(
    ui: &mut egui::Ui,
    ctx: &PanelCtx<'_>,
    model: &PanelModel,
    view: &mut PanelView,
) -> PanelAction {
    let PanelModel::Rnd(m) = model else {
        return PanelAction::None;
    };
    let th = ctx.theme;
    if m.sites.is_empty() {
        ui.label(ctx.text("ui.panel.rnd.no_sites"));
        return PanelAction::None;
    }
    picker(ui, th, &m.names, &mut view.sel);
    let site = m.sites[view.sel.min(m.sites.len() - 1)];
    let mut akcja = PanelAction::None;

    section(ui, th, &ctx.text("ui.rnd.project"));
    match &m.projekt {
        None => {
            ui.label(ctx.text("ui.rnd.no_project"));
        }
        Some(p) => {
            row(
                ui,
                th,
                &p.name,
                &ctx.fmt(
                    "ui.rnd.progress",
                    &[("procent", &(p.done_permille / 10).to_string())],
                ),
            );
            let zostalo = match p.months_left {
                Some(u16::MAX) | None => ctx.text("ui.rnd.eta_unknown"),
                Some(n) => ctx.fmt("ui.rnd.eta", &[("miesiecy", &n.to_string())]),
            };
            row(ui, th, &ctx.text("ui.rnd.left"), &zostalo);
            if p.breakthroughs > 0 {
                row(
                    ui,
                    th,
                    &ctx.text("ui.rnd.breakthroughs"),
                    &ctx.int(i64::from(p.breakthroughs)),
                );
            }
        }
    }
    row(ui, th, &ctx.text("ui.rnd.known"), &ctx.int(m.known as i64));

    section(ui, th, &ctx.text("ui.rnd.tree"));
    for w in &m.wezly {
        let etykieta = ctx.fmt(
            "ui.rnd.node",
            &[
                ("nazwa", &w.name),
                ("galaz", &w.branch),
                ("rp", &w.cost_rp.to_string()),
                ("rok", &w.world_year.to_string()),
            ],
        );
        let cmd = PlayerCommand::StartResearch {
            site,
            tech: w.key.clone(),
        };
        let blokada = crate::precheck(&ctx.session.view(), &cmd)
            .err()
            .map(|e| e.text(ctx.c, ctx.l));
        // Patent jest połową powodu, dla którego warto wybrać **ten** węzeł,
        // a nie najtańszy — więc stoi przy przycisku, nie w dymku.
        let pelna = if w.patentowy {
            format!("{etykieta} {}", ctx.text("ui.rnd.patentable"))
        } else {
            etykieta
        };
        if action(ui, ctx, &pelna, blokada) {
            akcja = PanelAction::cmd(cmd);
        }
    }

    if !m.patenty.is_empty() {
        section(ui, th, &ctx.text("ui.rnd.patents"));
        for (nazwa, zycie) in &m.patenty {
            row(
                ui,
                th,
                nazwa,
                &ctx.fmt(
                    "ui.rnd.patent_left",
                    &[("procent", &(zycie / 10).to_string())],
                ),
            );
        }
    }
    if !m.licencje.is_empty() {
        section(ui, th, &ctx.text("ui.rnd.licenses"));
        for (nazwa, bp) in &m.licencje {
            row(
                ui,
                th,
                nazwa,
                &ctx.fmt("ui.rnd.royalty", &[("bp", &bp.to_string())]),
            );
        }
    }

    if action(ui, ctx, &ctx.text("ui.panel.rnd.show_card"), None) {
        akcja = PanelAction::Show(Subject::Site(site));
    }
    akcja
}
