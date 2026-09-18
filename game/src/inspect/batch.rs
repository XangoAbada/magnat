//! Karta partii: od pola do półki, z czasem i kosztem każdego etapu (WP11, §14.4).
//!
//! # Skąd ten łańcuch
//!
//! `magnat_supply::trace_batch` chodzi po krawędziach pokrewieństwa partii i składa
//! etapy **przodków przed własnymi**, bo „od pola do półki" zaczyna się na polu —
//! czyli w partii, której ten bochenek jest wnukiem. Karta niczego nie liczy sama:
//! gdyby liczyła, miałaby drugą prawdę o tym samym koszcie.
//!
//! # Koszt jest narastający i to jest jego treść
//!
//! `TraceStage::cost_cumulative` mówi, ile ta masa kosztowała **w chwili tego etapu**,
//! więc różnica między dwoma wierszami jest tym, co dołożył krok między nimi. Gracz
//! widzi wprost, gdzie jego towar podrożał — na polu, w transporcie czy w piecu.
//!
//! # Czego karta nie udaje
//!
//! Ślad, którego nie ma, jest **brakiem danych**, a nie brakiem pochodzenia:
//! `TraceOrigin::NotTraced` znaczy „ta partia nie była śledzona", a partia śledzona
//! kończąca się na granicy ma `Imported`. Karta rozróżnia te trzy odpowiedzi, bo
//! pomylenie ich zamieniłoby lukę w pomiarze w zdanie o świecie.

use magnat_core::{ArenaRef, Subject};
use magnat_supply::{TraceKind, TraceOrigin};
use magnat_ui::{CardTabKind, InspectionCard, Rich};

use super::CardCtx;

/// Ile etapów pokazać. Łańcuch pszenica → mąka → chleb ma ich kilkanaście;
/// przy dwudziestu karta przestaje być kartą, a zaczyna być dziennikiem.
const ETAPOW: usize = 20;

/// Karta partii towaru.
pub fn card(ctx: &CardCtx<'_>, handle: ArenaRef) -> InspectionCard {
    let subject = Subject::Batch(handle);
    let nazwa = ctx.subject_name(subject);
    let naglowek = magnat_ui::lines_titled(&format!("{nazwa}\n"));
    let Some(slad) = slad(ctx, handle) else {
        let mut body: Rich = Vec::new();
        ctx.line(&mut body, "ui.batch.no_trace", &[]);
        return InspectionCard::new(subject, naglowek).tab(CardTabKind::Detail, body);
    };

    let mut stan: Rich = Vec::new();
    let towar = ctx
        .session
        .market
        .as_ref()
        .and_then(|m| m.good_key(slad.good))
        .unwrap_or_else(|| slad.good.0.to_string());
    ctx.line(&mut stan, "ui.batch.good", &[("towar", &towar)]);
    ctx.line(
        &mut stan,
        "ui.batch.depth",
        &[("ile", &slad.depth.to_string())],
    );
    let klucz_zrodla = match slad.origin {
        TraceOrigin::Deposit(_) => "ui.batch.origin.deposit",
        TraceOrigin::Imported => "ui.batch.origin.imported",
        TraceOrigin::InitialStock => "ui.batch.origin.initial",
        TraceOrigin::NotTraced => "ui.batch.origin.unknown",
    };
    ctx.line(&mut stan, klucz_zrodla, &[]);

    // Historia: etap, kiedy, gdzie, ile i ile kosztowała do tej chwili. Zakład jest
    // **odnośnikiem** — to jest ta sama reguła, którą trzyma każda inna karta (`Z-2`).
    let mut historia: Rich = Vec::new();
    // Okno ostatnich etapów, ale **punkt odniesienia bierze się sprzed okna**:
    // inaczej pierwszy widoczny wiersz pokazywałby jako przyrost cały koszt
    // narastający, czyli towar „drożałby o wszystko" w losowym kroku transportu.
    let od = slad.stages.len().saturating_sub(ETAPOW);
    let poprzedni = &mut od
        .checked_sub(1)
        .and_then(|i| slad.stages.get(i))
        .map_or(0, |x| x.cost_cumulative.get());
    for s in slad.stages.iter().skip(od) {
        let etap = ctx.text(&format!("ui.batch.stage.{}", kind_key(s.kind)));
        let kiedy = magnat_ui::CalendarFmt::axis_day(magnat_core::SimCalendar::from_minute(s.at));
        let przyrost = s.cost_cumulative.get() - *poprzedni;
        *poprzedni = s.cost_cumulative.get();
        let wiersz = ctx.fmt(
            "ui.batch.stage_row",
            &[
                ("etap", &etap),
                ("kiedy", &kiedy),
                ("masa", &magnat_ui::fmt::integer(ctx.l, s.mass.0 / 1000)),
                ("koszt", &ctx.money_str(s.cost_cumulative)),
                ("przyrost", &ctx.money_str(magnat_core::Money(przyrost))),
            ],
        );
        historia.push(magnat_ui::Span::plain(format!("{wiersz} ")));
        match s.site {
            Some(site) => {
                historia.push(ctx.subject_span(Subject::Site(site)));
                historia.push(magnat_ui::Span::plain("\n".to_string()));
            }
            None => historia.push(magnat_ui::Span::plain("\n".to_string())),
        }
    }

    InspectionCard::new(subject, naglowek)
        .tab(CardTabKind::State, stan)
        .tab(CardTabKind::History, historia)
}

/// Ślad partii z magazynu. `None`, gdy świat nie ma gospodarki albo uchwyt
/// wskazuje partię, której już nie ma — jedno i drugie jest normalnym stanem.
fn slad(ctx: &CardCtx<'_>, handle: ArenaRef) -> Option<magnat_supply::BatchTrace> {
    let m = ctx.session.market.as_ref()?;
    let chain = m.chain();
    let c = chain.lock();
    // `to_handle` wolno wołać **wyłącznie** tam, gdzie wiadomo, o którą arenę chodzi
    // (`K-69`) — a tutaj wiadomo: to jest karta partii.
    let id = handle.to_handle::<magnat_supply::Batch>();
    let t = magnat_supply::trace_batch(&c.store, id);
    (!t.stages.is_empty()).then_some(t)
}

/// Klucz tekstu etapu: `ui.batch.stage.<key>`.
const fn kind_key(k: TraceKind) -> &'static str {
    match k {
        TraceKind::Produced => "produced",
        TraceKind::Stored => "stored",
        TraceKind::Loaded => "loaded",
        TraceKind::Departed => "departed",
        TraceKind::Arrived => "arrived",
        TraceKind::Unloaded => "unloaded",
        TraceKind::Shelved => "shelved",
        TraceKind::Consumed => "consumed",
        TraceKind::Sold => "sold",
        TraceKind::Lost(_) => "lost",
        TraceKind::Split => "split",
        TraceKind::Merged => "merged",
    }
}
