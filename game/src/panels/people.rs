//! Ludzie: załoga, wakaty, kandydaci i to, ile wolno menedżerowi.
//!
//! Trzy komendy, bo trzy różne pytania gracza: **kogo zatrudnić** (`HireCandidate`),
//! **ile wolno menedżerowi** (`SetDelegationAutonomy`) i — gdy gracz jest jeszcze
//! pracownikiem — **gdzie się zgłosić** (`ApplyForJob`).
//!
//! Menedżera panel nie przypisuje i nie da się tego zrobić: menedżerem zostaje
//! najlepszy człowiek na stanowisku kierowniczym i wybiera go rynek pracy codziennie.
//! Gracz stawia menedżera, **zatrudniając go** na takie stanowisko.
//!
//! # Czwarta komenda: odpowiedź załodze (`FF-23`, M10g)
//!
//! Spór zbiorowy jest **stanem zakładu**, więc jego miejscem jest ten panel, a nie
//! nowy. Gracz odpowiada jedną liczbą — ile podwyżki daje w tej rundzie — i trzy
//! przyciski są trzema jej wartościami: żądanie w całości, połowa, zero. Próg
//! akceptacji załogi, ugoda i wyjście do strajku liczą się dalej tym samym kodem,
//! bo `talks::rundy` czyta gotową ofertę, a nie to, skąd pochodzi. To jest ta sama
//! reguła, którą `PricePolicy::Fixed` stosuje do ceny (M5c): podstawienie, a nie
//! drugi silnik.
//!
//! `grievance` stoi w tym panelu **jako ostrzeżenie wyprzedzające** (M10 §6 pkt 5):
//! rośnie miesiącami, zanim związek w ogóle powstanie, więc gracz, który go widzi,
//! ma czas podnieść płace zamiast negocjować pod strajkiem.

use magnat_core::{CitizenId, JobRoleId, Money, SiteId, Subject};
use magnat_ui::DataSource;

use super::widgets::{action, picker, row, section};
use super::{PanelAction, PanelCtx, PanelDesc, PanelId, PanelModel, PanelView};
use crate::{Autonomy, PlayerCommand};

pub(super) const DESC: PanelDesc = PanelDesc {
    id: PanelId::People,
    title: "ui.panel.people",
    deps: &[DataSource::Clock, DataSource::World, DataSource::Locale],
    build,
    render,
    min_tier: Some(crate::CareerTier::FirstBusiness),
};

/// Ilu kandydatów pokazać. Panel pokazuje kandydatów, a nie spis bezrobotnych.
const KANDYDATOW: usize = 12;

pub struct Model {
    pub sites: Vec<SiteId>,
    pub names: Vec<String>,
    /// Obsada wybranego zakładu: kto, w jakim zawodzie, za ile.
    pub crew: Vec<(CitizenId, JobRoleId, Money)>,
    /// Wolne etaty wybranego zakładu.
    pub vacancies: Vec<(JobRoleId, u16)>,
    pub candidates: Vec<(CitizenId, Money)>,
    /// Autonomia menedżera wybranego zakładu — `None`, gdy zakład nie jest zdelegowany.
    pub autonomy: Option<Autonomy>,
    /// Oferty pracy dla postaci gracza: bity uchwytu, zakład, zawód, stawka.
    pub jobs: Vec<(u64, SiteId, JobRoleId, Money)>,
    /// Żal załogi wybranego zakładu: poziom 0..100 i ile dób ponad progiem.
    pub grievance: (u8, u16),
    /// Spór, jeśli trwa: klucz stanu, żądanie w bp, fundusz strajkowy, numer rundy.
    pub spor: Option<(&'static str, u16, Money, u8)>,
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
    let wybrany = sites
        .get(ctx.sel.min(sites.len().saturating_sub(1)))
        .copied();
    let mut crew = Vec::new();
    let mut vacancies = Vec::new();
    let mut autonomy = None;
    if let (Some(site), Some(firms)) = (wybrany, s.app.world.get_resource::<magnat_firms::Firms>())
    {
        if let Some(z) = firms.site(site) {
            for p in &z.positions {
                for e in &p.filled {
                    crew.push((e.citizen, p.role, e.wage_month));
                }
                let wolne = p
                    .slots
                    .saturating_sub(u16::try_from(p.filled.len()).unwrap_or(u16::MAX));
                if wolne > 0 {
                    vacancies.push((p.role, wolne));
                }
            }
            autonomy = z
                .delegation
                .as_ref()
                .map(|d| Autonomy::from_firms(d.autonomy));
        }
    }
    // Kandydat, którego nie da się zatrudnić, byłby przyciskiem, który zawsze
    // odmawia. Lista pyta więc rejestr firm, czy ten człowiek już gdzieś pracuje —
    // szukający pracy **na etacie** jest poprawnym stanem rynku i złym wierszem
    // panelu (`DI-4`).
    let zatrudniony = |c: magnat_core::CitizenId| {
        s.app
            .world
            .get_resource::<magnat_firms::Firms>()
            .is_some_and(|f| f.employer_of(c).is_some())
    };
    let candidates = s
        .app
        .world
        .get_resource::<magnat_economy::LaborHandle>()
        .and_then(magnat_economy::LaborHandle::get)
        .map_or_else(Vec::new, |m| {
            m.seekers_list(KANDYDATOW * 4)
                .into_iter()
                .filter(|(c, _)| !zatrudniony(*c))
                .take(KANDYDATOW)
                .map(|(c, sk)| (c, sk.last_wage))
                .collect()
        });
    let jobs = s.player().map_or_else(Vec::new, |p| {
        magnat_economy::owner_ops::open_jobs_for(&s.app.world, p.citizen)
            .into_iter()
            .take(KANDYDATOW)
            .collect()
    });
    let (grievance, spor) = wybrany.map_or(((0, 0), None), |site| {
        let Some(u) = s.app.world.get_resource::<magnat_economy::Unions>() else {
            return ((0, 0), None);
        };
        let g = u.grievance(site);
        let z = u.get(site).and_then(|z| {
            let (stan, runda) = match z.state {
                magnat_economy::UnionState::Dormant { .. } => return None,
                magnat_economy::UnionState::Demand { .. } => ("demand", 0),
                magnat_economy::UnionState::Talks { round, .. } => ("talks", round),
                magnat_economy::UnionState::Strike { .. } => ("strike", 0),
            };
            Some((
                stan,
                z.demand.map_or(0, |d| d.raise_bp),
                z.strike_fund,
                runda,
            ))
        });
        ((g.level, g.days_above), z)
    });
    PanelModel::People(Model {
        sites,
        names,
        crew,
        vacancies,
        candidates,
        autonomy,
        jobs,
        grievance,
        spor,
    })
}

fn render(
    ui: &mut egui::Ui,
    ctx: &PanelCtx<'_>,
    model: &PanelModel,
    view: &mut PanelView,
) -> PanelAction {
    let PanelModel::People(m) = model else {
        return PanelAction::None;
    };
    let th = ctx.theme;
    let mut akcja = PanelAction::None;

    // Gracz bez firmy nadal jest pracownikiem — i to jest jego panel Ludzie.
    if m.sites.is_empty() {
        section(ui, th, &ctx.text("ui.people.job_search"));
        if m.jobs.is_empty() {
            ui.label(ctx.text("ui.people.no_jobs"));
        }
        for (bits, site, role, wage) in m.jobs.iter().take(8) {
            ui.horizontal(|ui| {
                ui.label(ctx.fmt(
                    "ui.people.job_row",
                    &[
                        ("gdzie", &site.entity().index().to_string()),
                        ("rola", &role.0.to_string()),
                        ("stawka", &ctx.money(*wage)),
                    ],
                ));
                let cmd = PlayerCommand::ApplyForJob { offer: *bits };
                let blokada = crate::precheck(&ctx.session.view(), &cmd)
                    .err()
                    .map(|e| e.text(ctx.c, ctx.l));
                if action(ui, ctx, &ctx.text("ui.people.apply"), blokada) {
                    akcja = PanelAction::cmd(cmd);
                }
            });
        }
        return akcja;
    }

    picker(ui, th, &m.names, &mut view.sel);
    let site = m.sites[view.sel.min(m.sites.len() - 1)];

    section(ui, th, &ctx.text("ui.people.crew"));
    if m.crew.is_empty() {
        ui.label(ctx.text("ui.people.no_crew"));
    }
    for (c, role, wage) in m.crew.iter().take(20) {
        let etykieta = ctx.fmt(
            "ui.people.crew_row",
            &[("rola", &role.0.to_string()), ("stawka", &ctx.money(*wage))],
        );
        if super::widgets::alert(ui, th, super::widgets::Sev::Normal, &etykieta) {
            akcja = PanelAction::Show(Subject::Citizen(*c));
        }
    }

    section(ui, th, &ctx.text("ui.people.vacancies"));
    if m.vacancies.is_empty() {
        ui.label(ctx.text("ui.people.no_vacancies"));
    }
    for (role, ile) in &m.vacancies {
        row(
            ui,
            th,
            &ctx.fmt("ui.people.role", &[("rola", &role.0.to_string())]),
            &ctx.int(i64::from(*ile)),
        );
    }

    if let (Some((role, _)), false) = (m.vacancies.first(), m.candidates.is_empty()) {
        section(ui, th, &ctx.text("ui.people.candidates"));
        for (c, ostatnia) in m.candidates.iter().take(6) {
            ui.horizontal(|ui| {
                ui.label(ctx.fmt(
                    "ui.people.candidate_row",
                    &[("stawka", &ctx.money(*ostatnia))],
                ));
                let cmd = PlayerCommand::HireCandidate {
                    site,
                    citizen: *c,
                    role: role.0,
                };
                let blokada = crate::precheck(&ctx.session.view(), &cmd)
                    .err()
                    .map(|e| e.text(ctx.c, ctx.l));
                if action(ui, ctx, &ctx.text("ui.people.hire"), blokada) {
                    akcja = PanelAction::cmd(cmd);
                }
            });
        }
    }

    // Spór zbiorowy przed autonomią: to jest ta sekcja, przez którą gracz otwiera
    // panel, kiedy załoga wyszła do bramy.
    section(ui, th, &ctx.text("ui.people.dispute"));
    let (poziom, dni) = m.grievance;
    super::widgets::kpi(
        ui,
        th,
        &ctx.text("ui.people.grievance"),
        &ctx.fmt(
            "ui.people.grievance_row",
            &[("poziom", &poziom.to_string()), ("dni", &dni.to_string())],
        ),
        if poziom >= 55 {
            super::widgets::Sev::Bad
        } else if poziom >= 35 {
            super::widgets::Sev::Warn
        } else {
            super::widgets::Sev::Normal
        },
    );
    match m.spor {
        None => {
            ui.label(ctx.text("ui.people.no_dispute"));
        }
        Some((stan, zadanie, fundusz, runda)) => {
            row(
                ui,
                th,
                &ctx.text(&format!("ui.union_state.{stan}")),
                &ctx.fmt(
                    "ui.people.dispute_row",
                    &[("bp", &zadanie.to_string()), ("runda", &runda.to_string())],
                ),
            );
            row(
                ui,
                th,
                &ctx.text("ui.people.strike_fund"),
                &ctx.money(fundusz),
            );
            // Trzy przyciski, jedna komenda: przyjmij żądanie, kontruj połową,
            // przeczekaj. Liczba jest **odpowiedzią na tę rundę**, a nie polityką
            // na całe negocjacje — dlatego zużywa się przy najbliższej.
            for (key, bp) in [
                ("ui.people.accept", zadanie),
                ("ui.people.counter", zadanie / 2),
                ("ui.people.wait", 0),
            ] {
                let cmd = PlayerCommand::AnswerUnion { site, offer_bp: bp };
                let blokada = crate::precheck(&ctx.session.view(), &cmd)
                    .err()
                    .map(|e| e.text(ctx.c, ctx.l));
                if action(ui, ctx, &ctx.fmt(key, &[("bp", &bp.to_string())]), blokada) {
                    akcja = PanelAction::cmd(cmd);
                }
            }
        }
    }

    section(ui, th, &ctx.text("ui.people.autonomy"));
    match m.autonomy {
        None => {
            ui.label(ctx.text("ui.people.no_delegation"));
        }
        Some(teraz) => {
            for a in Autonomy::ALL {
                let cmd = PlayerCommand::SetDelegationAutonomy { site, autonomy: a };
                let etykieta = ctx.text(&format!("ui.autonomy.{}", a.key()));
                let blokada = if a == teraz {
                    Some(ctx.text("ui.people.autonomy_current"))
                } else {
                    crate::precheck(&ctx.session.view(), &cmd)
                        .err()
                        .map(|e| e.text(ctx.c, ctx.l))
                };
                if action(ui, ctx, &etykieta, blokada) {
                    akcja = PanelAction::cmd(cmd);
                }
            }
        }
    }
    akcja
}
