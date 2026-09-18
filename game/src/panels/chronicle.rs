//! Kronika: dziennik świata i gracza, przeszukiwalny.
//!
//! # To jest panel **bez komendy** i mówi to o sobie
//!
//! `PanelId::is_operational` zwraca dla niego `false`, a kryterium WP10 obejmuje
//! panele operacyjne (`DH-3`). Dziennik nie ma czego zmienić w świecie: przycisk
//! „zrób coś" w kronice byłby przyciskiem bez skutku, czyli dokładnie tym, przed
//! czym broni `K-67`. Kronika **prowadzi** do podmiotu — kliknięcie wpisu otwiera
//! jego kartę — i to jest cała jej interakcja.
//!
//! # Osiągnięcia to zapytania, nie system
//!
//! „Twoja firma przetrwała trzy recesje" jest pytaniem do tego magazynu
//! (`Chronicle::count_event`), a nie osobną mechaniką z własnym stanem (§5.11).

use magnat_ui::DataSource;

use super::widgets::{alert, picker, section, Sev};
use super::{PanelAction, PanelCtx, PanelDesc, PanelId, PanelModel, PanelView};
use crate::chronicle::{ChronicleKind, Query};

pub(super) const DESC: PanelDesc = PanelDesc {
    id: PanelId::Chronicle,
    title: "ui.panel.chronicle",
    deps: &[DataSource::Clock, DataSource::World, DataSource::Locale],
    build,
    render,
    min_tier: Some(crate::CareerTier::FirstBusiness),
};

/// Ile wpisów pokazać naraz. Kronika stu lat ma setki tysięcy wierszy, a ekran
/// czterdzieści — filtr jest tu narzędziem, nie ozdobą.
const NA_EKRANIE: usize = 40;

pub struct Model {
    /// Gotowe wiersze: data, tekst i podmiot do kliknięcia.
    pub rows: Vec<(String, String, Option<magnat_core::Subject>, Sev)>,
    pub total: usize,
    /// Osiągnięcia emergentne: klucz zdania i liczba, która je domknęła.
    ///
    /// **To są zapytania do kroniki, a nie osobny system** (§5.11). Lista jest
    /// krótka i ma nią zostać: osiągnięcie, którego nie da się zadać jako pytanie
    /// o wpisy, należy do mechaniki, a nie tutaj.
    pub feats: Vec<(String, usize)>,
}

fn build(ctx: &PanelCtx<'_>) -> PanelModel {
    let s = ctx.session;
    let k = s.chronicle();
    let filtr = ChronicleKind::ALL.get(ctx.sel.saturating_sub(1)).copied();
    let q = Query {
        kind: if ctx.sel == 0 { None } else { filtr },
        ..Query::default()
    };
    let rows = k
        .query(&q)
        .into_iter()
        .take(NA_EKRANIE)
        .map(|e| {
            let waga = match e.kind {
                ChronicleKind::PlayerAction => Sev::Good,
                ChronicleKind::EventStarted if e.importance >= 60 => Sev::Bad,
                ChronicleKind::EventStarted => Sev::Warn,
                _ => Sev::Normal,
            };
            (
                magnat_ui::CalendarFmt::axis_day(magnat_core::SimCalendar::from_minute(
                    magnat_core::SimMinute(e.at.0),
                )),
                crate::chronicle::text(e, s, ctx.c, ctx.l),
                e.actor,
                waga,
            )
        })
        .collect();
    PanelModel::Chronicle(Model {
        rows,
        total: k.len(),
        feats: osiagniecia(ctx),
    })
}

/// Osiągnięcia jako zapytania do kroniki.
///
/// Liczymy zdarzenia po definicji z katalogu, bo to definicja niesie klucz
/// (`EventDef::key`) — „przetrwałeś trzy recesje" jest pytaniem o liczbę wpisów
/// o definicji `recesja`, a nie o licznik prowadzony gdzieś obok.
fn osiagniecia(ctx: &PanelCtx<'_>) -> Vec<(String, usize)> {
    let k = ctx.session.chronicle();
    let Some(ev) = ctx
        .session
        .app
        .world
        .get_resource::<magnat_events::Events>()
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, d) in ev.catalog().defs.iter().enumerate() {
        let n = k.count_event(u16::try_from(i).unwrap_or(u16::MAX));
        if n >= PROG_OSIAGNIECIA {
            out.push((d.chronicle.clone(), n));
        }
    }
    out.truncate(POKAZ_OSIAGNIEC);
    out
}

/// Ile razy zdarzenie musi zajść, żeby przetrwanie go było osiągnięciem.
const PROG_OSIAGNIECIA: usize = 3;
/// Ile osiągnięć pokazać naraz.
const POKAZ_OSIAGNIEC: usize = 5;

fn render(
    ui: &mut egui::Ui,
    ctx: &PanelCtx<'_>,
    model: &PanelModel,
    view: &mut PanelView,
) -> PanelAction {
    let PanelModel::Chronicle(m) = model else {
        return PanelAction::None;
    };
    let th = ctx.theme;
    let mut etykiety = vec![ctx.text("ui.chronicle.all")];
    for k in ChronicleKind::ALL {
        etykiety.push(ctx.text(&format!("ui.chronicle.kind.{}", k.key())));
    }
    picker(ui, th, &etykiety, &mut view.sel);

    section(
        ui,
        th,
        &ctx.fmt("ui.chronicle.count", &[("ile", &m.total.to_string())]),
    );
    if !m.feats.is_empty() {
        section(ui, th, &ctx.text("ui.chronicle.feats"));
        for (klucz, ile) in &m.feats {
            ui.label(ctx.fmt(
                "ui.chronicle.feat_row",
                &[("co", &ctx.text(klucz)), ("ile", &ile.to_string())],
            ));
        }
    }

    let mut akcja = PanelAction::None;
    if m.rows.is_empty() {
        ui.label(ctx.text("ui.chronicle.empty"));
    }
    for (data, tekst, podmiot, waga) in &m.rows {
        if alert(ui, th, *waga, &format!("{data}  {tekst}")) {
            if let Some(s) = podmiot {
                akcja = PanelAction::Show(*s);
            }
        }
    }
    akcja
}
