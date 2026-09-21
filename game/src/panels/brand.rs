//! Marka i kampanie: lejek, rozbicie na dzielnice, obietnica wobec towaru.
//!
//! # Po co ten panel jest
//!
//! Przez trzy podfazy mechanika reklamy działała i nikt nie mógł jej dotknąć: firmy
//! AI kupowały kampanie raz na miesiąc, a gracz nie miał jak kupić billboardu
//! (`FF-1`). Panel domyka drugą stronę tego samego mechanizmu — **tą samą drogą**,
//! którą idzie AI (`Campaigns::open`), a nie własną.
//!
//! # Trzy liczby lejka i skąd każda pochodzi
//!
//! - **znajomość** — `CampaignMetrics::first_contacts`: ekspozycje, które trafiły
//!   do kogoś, kto marki nie znał. Liczy je kampania, bo tylko ona wie, czy dotarła.
//! - **próba** — ilu mieszkańców ma wpis o tej marce ze źródłem `Experience`, czyli
//!   **kupiło**. Tego kampania nie wie i wiedzieć nie może: zakup dzieje się dobę
//!   później i gdzie indziej.
//! - **afinitet** — średnia sympatia wśród znających.
//!
//! Dwie ostatnie liczy jeden przebieg po mieszkańcach, raz na godzinę gry, i to jest
//! jawny sufit: pamięć marki nie ma indeksu „kto mnie zna" i mieć go nie ma
//! (`brand_strength` w `sim/agents` nazywa ten sam powód — pole byłoby drugą prawdą
//! o tej samej liczbie). `ponytail:` droga wyjścia, gdyby to zaczęło kosztować:
//! licznik per marka podbijany w `touch`, wpięty do hasha jak każdy inny stan.

use magnat_core::{BrandId, DistrictId, Money, SiteId, Subject};
use magnat_media::{AdChannel, Campaigns, Outlets};
use magnat_ui::DataSource;

use super::widgets::{action, picker, row, section};
use super::{PanelAction, PanelCtx, PanelDesc, PanelId, PanelModel, PanelView};
use crate::PlayerCommand;

pub(super) const DESC: PanelDesc = PanelDesc {
    id: PanelId::Brand,
    title: "ui.panel.brand",
    deps: &[DataSource::Clock, DataSource::World, DataSource::Locale],
    build,
    render,
    min_tier: Some(crate::CareerTier::FirstBusiness),
};

/// Ile najruchliwszych ulic pokazać pod billboard.
const ULIC: usize = 6;

/// Domyślna długość kampanii gracza w dobach — miesiąc kalendarza `K-1`.
const DNI: u16 = 30;

/// Szansa, że przejeżdżający zauważy tablicę (M10b §5.2). Ta sama liczba, którą
/// niesie kanał billboardu przy kampanii firmy AI.
const ZAUWAZA_BPS: u16 = 1_200;

/// Jeden kanał, który gracz może kupić **teraz**: indeks słownika, cel i etykieta.
pub struct Wybor {
    pub channel: u8,
    pub target: u32,
    /// Klucz tekstu opisującego cel: ulica, tytuł, zdarzenie albo nic.
    pub label: String,
    /// Cena tysiąca ekspozycji — jedyna liczba, po której da się kanały porównać.
    pub cpm: Money,
}

/// Wiersz tabeli kampanii.
pub struct Kampania {
    pub channel: u8,
    pub budget: Money,
    pub spent: Money,
    pub exposures: u64,
    pub first_contacts: u64,
    pub districts: Vec<(DistrictId, u32)>,
    pub live: bool,
}

pub struct Model {
    pub sites: Vec<SiteId>,
    pub names: Vec<String>,
    pub brand: Option<BrandId>,
    pub campaigns: Vec<Kampania>,
    pub oferta: Vec<Wybor>,
    /// Ilu zna markę, ilu jej spróbowało, jaka jest średnia sympatia.
    pub lejek: (u32, u32, i8),
    /// Obiecane wobec dostarczonego: średnia oczekiwana jakość w pamięci miasta
    /// i średnia jakość tego, co stoi dziś na półkach gracza.
    pub jakosc: (u8, u8),
    /// Budżet, który gracz może przeznaczyć — saldo rachunku wybranego zakładu.
    pub saldo: Money,
}

/// Kanał kampanii z indeksu słownika i celu.
///
/// **Jedno miejsce, które wie, co znaczy `target` w każdym kanale** — czyta je
/// i panel, składając komendę, i `precheck`, wygaszając przycisk. Dwie kopie tej
/// tabeli rozjechałyby się przy pierwszym nowym kanale, a rozjazd wyglądałby jak
/// przycisk, który wygląda na dozwolony i nie przechodzi.
///
/// `None` znaczy „ten kanał nie ma tu gdzie stanąć": nie ma takiej ulicy, tytułu
/// ani zdarzenia — albo indeks jest spoza słownika.
pub(crate) fn kanal(
    world: &magnat_ecs::World,
    site: SiteId,
    channel: u8,
    target: u32,
) -> Option<AdChannel> {
    let kind = magnat_core::AdChannelKind::from_index(channel as usize)?;
    Some(match kind {
        magnat_core::AdChannelKind::Billboard => {
            let ile = world
                .get_resource::<magnat_traffic::TrafficServices>()?
                .oracle
                .with_road(magnat_nav::RoadGraph::edge_count);
            if target as usize >= ile {
                return None;
            }
            AdChannel::Billboard {
                edge: magnat_nav::EdgeId(target),
                notice_rate_bps: ZAUWAZA_BPS,
            }
        }
        magnat_core::AdChannelKind::Press
        | magnat_core::AdChannelKind::Radio
        | magnat_core::AdChannelKind::Tv => {
            let outlets = world.get_resource::<Outlets>()?;
            let (s, o) = outlets.iter().find(|(s, _)| s.entity().index() == target)?;
            // Tytuł musi być tego rodzaju, którego kupuje kanał: ogłoszenie prasowe
            // w stacji radiowej byłoby zleceniem, którego nikt nie wykona.
            let pasuje = matches!(
                (kind, o.kind),
                (
                    magnat_core::AdChannelKind::Press,
                    magnat_core::MediaKind::Newspaper | magnat_core::MediaKind::Portal
                ) | (
                    magnat_core::AdChannelKind::Radio,
                    magnat_core::MediaKind::Radio
                ) | (magnat_core::AdChannelKind::Tv, magnat_core::MediaKind::Tv)
            );
            if !pasuje {
                return None;
            }
            match kind {
                magnat_core::AdChannelKind::Press => AdChannel::Press { outlet: *s },
                magnat_core::AdChannelKind::Radio => AdChannel::Radio { outlet: *s },
                _ => AdChannel::Tv { outlet: *s },
            }
        }
        magnat_core::AdChannelKind::Leaflet => AdChannel::Leaflet {
            origin: site,
            radius_m: world
                .get_resource::<magnat_agents::BrandData>()?
                .channels
                .leaflet_radius_m,
        },
        magnat_core::AdChannelKind::InStorePromo => AdChannel::InStorePromo { site },
        magnat_core::AdChannelKind::Sponsorship => {
            let ev = world.get_resource::<magnat_events::Events>()?;
            let id = magnat_core::EventId(target);
            if !ev.active().iter().any(|e| e.id == id) {
                return None;
            }
            AdChannel::Sponsorship { event: id }
        }
        magnat_core::AdChannelKind::Pr => AdChannel::Pr,
    })
}

/// Kanały, które gracz może kupić dla tego zakładu, z celem i ceną tysiąca ekspozycji.
fn oferta(ctx: &PanelCtx<'_>, site: SiteId) -> Vec<Wybor> {
    let world = &ctx.session.app.world;
    let mut out: Vec<Wybor> = Vec::new();
    let cpm = |k: magnat_core::AdChannelKind| {
        world
            .get_resource::<magnat_agents::BrandData>()
            .map_or(Money::ZERO, |b| b.channels.cpm(k))
    };

    // Billboard: najruchliwsze ulice miasta. Ruchliwość jest jedyną liczbą, po której
    // gracz może te tablice porównać — a jest już policzona po stronie ruchu.
    for (edge, _) in ruchliwe(ctx).into_iter().take(ULIC) {
        out.push(Wybor {
            channel: magnat_core::AdChannelKind::Billboard.as_index() as u8,
            target: edge,
            label: ctx.fmt("ui.brand.street", &[("nr", &edge.to_string())]),
            cpm: cpm(magnat_core::AdChannelKind::Billboard),
        });
    }
    if let Some(outlets) = world.get_resource::<Outlets>() {
        for (s, o) in outlets.iter() {
            let kind = match o.kind {
                magnat_core::MediaKind::Newspaper | magnat_core::MediaKind::Portal => {
                    magnat_core::AdChannelKind::Press
                }
                magnat_core::MediaKind::Radio => magnat_core::AdChannelKind::Radio,
                magnat_core::MediaKind::Tv => magnat_core::AdChannelKind::Tv,
            };
            out.push(Wybor {
                channel: kind.as_index() as u8,
                target: s.entity().index(),
                label: ctx.fmt(
                    "ui.brand.outlet",
                    &[
                        (
                            "rodzaj",
                            &ctx.text(&format!("ui.media_kind.{}", o.kind.name())),
                        ),
                        ("nr", &s.entity().index().to_string()),
                    ],
                ),
                cpm: cpm(kind),
            });
        }
    }
    for k in [
        magnat_core::AdChannelKind::Leaflet,
        magnat_core::AdChannelKind::InStorePromo,
        magnat_core::AdChannelKind::Pr,
    ] {
        if kanal(world, site, k.as_index() as u8, 0).is_some() {
            out.push(Wybor {
                channel: k.as_index() as u8,
                target: 0,
                label: String::new(),
                cpm: cpm(k),
            });
        }
    }
    out
}

/// Najruchliwsze krawędzie sieci drogowej — `(numer, potok)`, malejąco.
fn ruchliwe(ctx: &PanelCtx<'_>) -> Vec<(u32, i64)> {
    let world = &ctx.session.app.world;
    let Some(oracle) = world
        .get_resource::<magnat_traffic::TrafficServices>()
        .map(|s| s.oracle.clone())
    else {
        return Vec::new();
    };
    let Some(overlay) = world.get_resource::<magnat_traffic::TrafficOverlay>() else {
        return Vec::new();
    };
    let ile = oracle.with_road(magnat_nav::RoadGraph::edge_count);
    let mut v: Vec<(u32, i64)> = overlay.with_front(|snap| {
        (0..ile)
            .map(|i| {
                (
                    i as u32,
                    snap.edge_value(magnat_traffic::TrafficField::Flow, i),
                )
            })
            .collect()
    });
    // Remis rozstrzyga niższy numer krawędzi: lista ma być ta sama w dwóch
    // przebiegach, bo gracz wybiera z niej pozycję, a pozycja jedzie do dziennika.
    v.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v.truncate(ULIC);
    v
}

/// Jeden przebieg po mieszkańcach: ilu zna markę, ilu jej spróbowało, jaka średnia
/// sympatia i jakiej jakości się spodziewają.
fn pamiec(ctx: &PanelCtx<'_>, brand: BrandId) -> (u32, u32, i8, u8) {
    let world = &ctx.session.app.world;
    let Some(p) = world.get_resource::<magnat_agents::Population>() else {
        return (0, 0, 0, 0);
    };
    let dzis = ctx.session.tick().get() / magnat_core::time::MINUTES_PER_DAY;
    let (mut zna, mut probowalo, mut suma, mut oczekiwana) = (0u32, 0u32, 0i64, 0i64);
    for e in p.citizens() {
        let Some(s) = magnat_agents::slots_of(world, *e, dzis)
            .as_slice()
            .iter()
            .find(|s| s.brand == brand)
            .copied()
        else {
            continue;
        };
        zna += 1;
        suma += i64::from(s.affinity);
        oczekiwana += i64::from(s.expected_quality);
        if s.source() == magnat_core::TouchSource::Experience
            || s.source() == magnat_core::TouchSource::Owned
        {
            probowalo += 1;
        }
    }
    if zna == 0 {
        return (0, 0, 0, 0);
    }
    let n = i64::from(zna);
    (
        zna,
        probowalo,
        (suma / n) as i8,
        (oczekiwana / n).clamp(0, 100) as u8,
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
    let Some(site) = sites
        .get(ctx.sel.min(sites.len().saturating_sub(1)))
        .copied()
    else {
        return PanelModel::Brand(Model {
            sites,
            names,
            brand: None,
            campaigns: Vec::new(),
            oferta: Vec::new(),
            lejek: (0, 0, 0),
            jakosc: (0, 0),
            saldo: Money::ZERO,
        });
    };
    let world = &s.app.world;
    let brand = world
        .get_resource::<magnat_firms::Firms>()
        .and_then(|f| f.site(site))
        .and_then(|z| magnat_supply::brand_of(magnat_firms::firm_id(z.firm)));

    let campaigns = world
        .get_resource::<Campaigns>()
        .map_or_else(Vec::new, |c| {
            let now = magnat_core::SimMinute(s.tick().get());
            c.iter()
                .filter(|(_, k)| k.site == site)
                .map(|(_, k)| Kampania {
                    channel: k.channel.kind().as_index() as u8,
                    budget: k.budget,
                    spent: k.spent,
                    exposures: k.metrics.exposures_total,
                    first_contacts: k.metrics.first_contacts,
                    districts: k.metrics.top_districts(),
                    live: k.is_live(now),
                })
                .collect()
        });

    let (zna, probowalo, afinitet, oczekiwana) = brand.map_or((0, 0, 0, 0), |b| pamiec(ctx, b));
    // Dostarczona jakość: średnia z tego, co stoi dziś na półkach gracza. Pusta półka
    // nie zaniża średniej — zaniżałaby ocenę firmy, która akurat nie ma czego sprzedać.
    let dostarczona = s.market.as_ref().map_or(0u8, |m| {
        let t = s.tick();
        let od = magnat_core::Tick(t.get() - t.get() % magnat_core::time::MINUTES_PER_MONTH);
        m.shop_panel(site, od, t).map_or(0, |snap| {
            let n = snap.shelves.len() as i64;
            if n == 0 {
                return 0;
            }
            (snap
                .shelves
                .iter()
                .map(|w| i64::from(w.quality.get()))
                .sum::<i64>()
                / n)
                .clamp(0, 100) as u8
        })
    });
    let saldo = s.market.as_ref().map_or(Money::ZERO, |m| {
        m.account_of(site)
            .and_then(|a| world.get_resource::<magnat_economy::Books>()?.balance(a))
            .unwrap_or(Money::ZERO)
    });

    PanelModel::Brand(Model {
        sites,
        names,
        brand,
        campaigns,
        oferta: oferta(ctx, site),
        lejek: (zna, probowalo, afinitet),
        jakosc: (oczekiwana, dostarczona),
        saldo,
    })
}

/// `ponytail:` jedna funkcja na cztery sekcje panelu (lejek, obietnica, kampanie,
/// zakup). Sufit: przekroczy 150 linii przy piątej sekcji. Ścieżka wyjścia jest
/// mechaniczna — każda sekcja jest już osobnym blokiem i wychodzi do własnej funkcji
/// bez zmiany kolejności rysowania.
fn render(
    ui: &mut egui::Ui,
    ctx: &PanelCtx<'_>,
    model: &PanelModel,
    view: &mut PanelView,
) -> PanelAction {
    let PanelModel::Brand(m) = model else {
        return PanelAction::None;
    };
    let th = ctx.theme;
    if m.sites.is_empty() {
        ui.label(ctx.text("ui.panel.brand.no_sites"));
        return PanelAction::None;
    }
    picker(ui, th, &m.names, &mut view.sel);
    let site = m.sites[view.sel.min(m.sites.len() - 1)];
    let mut akcja = PanelAction::None;
    if m.brand.is_none() {
        ui.label(ctx.text("ui.panel.brand.no_brand"));
        return akcja;
    }

    // Lejek: znajomość → próba → afinitet.
    section(ui, th, &ctx.text("ui.brand.funnel"));
    let (zna, probowalo, afinitet) = m.lejek;
    row(
        ui,
        th,
        &ctx.text("ui.brand.known"),
        &ctx.int(i64::from(zna)),
    );
    row(
        ui,
        th,
        &ctx.text("ui.brand.tried"),
        &ctx.int(i64::from(probowalo)),
    );
    row(
        ui,
        th,
        &ctx.text("ui.brand.affinity"),
        &ctx.int(i64::from(afinitet)),
    );

    // Obietnica wobec towaru — jedyne miejsce, w którym widać przereklamowanie.
    section(ui, th, &ctx.text("ui.brand.promise"));
    let (obiecana, dostarczona) = m.jakosc;
    row(
        ui,
        th,
        &ctx.text("ui.brand.expected"),
        &ctx.int(i64::from(obiecana)),
    );
    row(
        ui,
        th,
        &ctx.text("ui.brand.delivered"),
        &ctx.int(i64::from(dostarczona)),
    );
    if obiecana > dostarczona.saturating_add(10) {
        super::widgets::alert(
            ui,
            th,
            super::widgets::Sev::Warn,
            &ctx.fmt(
                "ui.brand.overpromised",
                &[("ile", &i64::from(obiecana - dostarczona).to_string())],
            ),
        );
    }

    section(ui, th, &ctx.text("ui.brand.campaigns"));
    if m.campaigns.is_empty() {
        ui.label(ctx.text("ui.brand.no_campaigns"));
    }
    for k in &m.campaigns {
        let nazwa = ctx.text(&format!(
            "ui.ad_channel.{}",
            magnat_core::AdChannelKind::from_index(k.channel as usize)
                .map_or("Pr", magnat_core::AdChannelKind::name)
        ));
        row(
            ui,
            th,
            &nazwa,
            &ctx.fmt(
                "ui.brand.campaign_row",
                &[
                    ("eksp", &ctx.int(k.exposures as i64)),
                    ("nowi", &ctx.int(k.first_contacts as i64)),
                    ("wydano", &ctx.money(k.spent)),
                    ("budzet", &ctx.money(k.budget)),
                ],
            ),
        );
        if !k.live {
            continue;
        }
        // Z jakich dzielnic — to jest ta połowa kryterium, której metryka kampanii
        // nie miała do `M10g`.
        for (d, ile) in k.districts.iter().take(5) {
            row(
                ui,
                th,
                &ctx.fmt("ui.brand.district", &[("nr", &d.0.to_string())]),
                &ctx.int(i64::from(*ile)),
            );
        }
    }

    section(ui, th, &ctx.text("ui.brand.buy"));
    // Budżet: dziesiąta część salda zakładu. Jedno kliknięcie, nie pole tekstowe —
    // ta sama reguła, którą panel sklepu stosuje do ceny (`§5.12 pkt 3`).
    let budzet = Money(m.saldo.get() / 10);
    for w in &m.oferta {
        let kind = magnat_core::AdChannelKind::from_index(w.channel as usize);
        let nazwa = ctx.text(&format!(
            "ui.ad_channel.{}",
            kind.map_or("Pr", magnat_core::AdChannelKind::name)
        ));
        let etykieta = if w.label.is_empty() {
            ctx.fmt(
                "ui.brand.buy_row",
                &[("kanal", &nazwa), ("cpm", &ctx.money(w.cpm))],
            )
        } else {
            ctx.fmt(
                "ui.brand.buy_row_at",
                &[
                    ("kanal", &nazwa),
                    ("gdzie", &w.label),
                    ("cpm", &ctx.money(w.cpm)),
                ],
            )
        };
        let cmd = PlayerCommand::OpenCampaign {
            site,
            channel: w.channel,
            target: w.target,
            budget: budzet,
            days: DNI,
        };
        let blokada = crate::precheck(&ctx.session.view(), &cmd)
            .err()
            .map(|e| e.text(ctx.c, ctx.l));
        if action(ui, ctx, &etykieta, blokada) {
            akcja = PanelAction::cmd(cmd);
        }
    }

    if action(ui, ctx, &ctx.text("ui.panel.brand.show_card"), None) {
        akcja = PanelAction::Show(Subject::Site(site));
    }
    akcja
}
