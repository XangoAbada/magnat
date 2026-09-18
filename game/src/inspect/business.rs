//! Karty firmy, zakładu i pojazdu (M9c §5.7).
//!
//! Zakład handlowy **nie dostaje drugiej karty**: półki, klientów i konkurencję
//! składa `magnat_ui::ShopCard` z migawki `ShopPanelSnapshot` od M5e, a ten moduł
//! dokłada nagłówek z odnośnikiem do firmy i zakładkę „Dlaczego" z powodami przecen.
//! Druga arytmetyka tych samych liczb rozjechałaby się z pierwszą (`Y-1`).

use super::CardCtx;
use magnat_core::{FirmId, SiteId, Subject, Tick, VehicleId};
use magnat_firms::{FirmKey, Firms};
use magnat_ui::{CardTabKind, InspectionCard, Rich, ShopCard, ShopTab, ShopView, Span};

/// `FirmId` niesie indeks encji, a `Firms` kluczuje po `FirmKey` — przejście jest
/// odwrotnością `magnat_firms::firm_id` i mieszka **w jednym miejscu**, tak samo jak
/// konwersja klucza zakładu z `K-46`.
#[must_use]
fn key_of(f: FirmId) -> FirmKey {
    FirmKey(u64::from(f.entity().index()))
}

fn firms(session: &crate::Session) -> Option<&Firms> {
    session.app.world.get_resource::<Firms>()
}

#[must_use]
pub fn firm_exists(session: &crate::Session, f: FirmId) -> bool {
    firms(session).is_some_and(|r| r.get(key_of(f)).is_some())
}

#[must_use]
pub fn site_exists(session: &crate::Session, s: SiteId) -> bool {
    firms(session).is_some_and(|r| r.site(s).is_some())
        || session
            .market
            .as_ref()
            .is_some_and(|m| m.account_of(s).is_some())
}

#[must_use]
pub fn vehicle_exists(session: &crate::Session, v: VehicleId) -> bool {
    session
        .app
        .world
        .get::<magnat_traffic::VehicleOwner>(v.entity())
        .is_some()
}

/// Nazwa firmy z `data/names/` — proceduralna, więc bez klucza lokalizacji.
#[must_use]
pub fn firm_name(ctx: &CardCtx<'_>, f: FirmId) -> String {
    firms(ctx.session)
        .and_then(|r| r.get(key_of(f)).map(|x| x.name.clone()))
        .unwrap_or_else(|| {
            ctx.fmt(
                "ui.subject.firm",
                &[("nr", &f.entity().index().to_string())],
            )
        })
}

/// Nazwa zakładu: nazwa firmy, która go prowadzi, plus rodzaj. Własnych nazw zakłady
/// nie mają — `ponytail:` sufit nazwany, bo nazwa punktu („Dobry Koszyk nr 3") jest
/// kosmetyką, a odnośnik działa bez niej.
#[must_use]
pub fn site_name(ctx: &CardCtx<'_>, s: SiteId) -> String {
    let firma = firms(ctx.session)
        .and_then(|r| r.site(s).map(|x| x.firm))
        .and_then(|k| firms(ctx.session).and_then(|r| r.get(k).map(|f| f.name.clone())));
    match firma {
        Some(n) => n,
        None => ctx.fmt(
            "ui.subject.site",
            &[("nr", &s.entity().index().to_string())],
        ),
    }
}

#[must_use]
pub fn vehicle_name(ctx: &CardCtx<'_>, v: VehicleId) -> String {
    ctx.fmt(
        "ui.subject.vehicle",
        &[("nr", &v.entity().index().to_string())],
    )
}

/// Karta firmy: właściciele, zakłady i powody ostatnich decyzji.
///
/// Pełny pulpit firmy (przepływy, KPI, alerty) jest panelem biznesowym i należy
/// do `M9e` — tutaj jest **karta**, czyli tożsamość i odnośniki.
#[must_use]
pub fn firm_card(ctx: &CardCtx<'_>, f: FirmId) -> InspectionCard {
    let subject = Subject::Firm(f);
    let nazwa = firm_name(ctx, f);
    let naglowek = magnat_ui::lines_titled(&format!("{nazwa}\n"));
    let Some(firm) = firms(ctx.session).and_then(|r| r.get(key_of(f))) else {
        return InspectionCard::new(subject, naglowek).tab(
            CardTabKind::State,
            vec![
                magnat_ui::gone_span(ctx.c, ctx.l, &nazwa),
                Span::plain("\n".to_string()),
            ],
        );
    };

    let mut stan: Rich = Vec::new();
    ctx.line(
        &mut stan,
        "ui.firm.status",
        &[("stan", &format!("{:?}", firm.status))],
    );
    ctx.line(
        &mut stan,
        "ui.firm.strategy",
        &[(
            "kurs",
            &ctx.text(&format!("ui.strategy.{}", firm.strategy.name())),
        )],
    );
    if let Some(d) = firm.director {
        ctx.link_line(&mut stan, "ui.firm.director", Subject::Citizen(d));
    }
    ctx.link_line(
        &mut stan,
        "ui.card.district",
        Subject::District(firm.hq_district),
    );
    for s in &firm.sites {
        ctx.link_line(&mut stan, "ui.firm.site", Subject::Site(*s));
    }

    let mut dlaczego: Rich = Vec::new();
    for d in firm.log.iter() {
        dlaczego.push(Span::plain(format!(
            "{}  {}\n",
            magnat_ui::zegar((d.tick.get() % 1440) as u16),
            magnat_ui::describe(ctx.c, ctx.l, d.reason)
        )));
    }

    InspectionCard::new(subject, naglowek)
        .tab(CardTabKind::State, stan)
        .tab(CardTabKind::Why, dlaczego)
}

/// Karta zakładu. Sklep dostaje trzy zakładki z migawki M5e; zakład bez półki —
/// tożsamość i odnośniki.
#[must_use]
pub fn site_card(ctx: &CardCtx<'_>, s: SiteId) -> InspectionCard {
    let subject = Subject::Site(s);
    let nazwa = site_name(ctx, s);
    let t = ctx.session.tick();
    let od = Tick(t.get() - t.get() % magnat_core::time::MINUTES_PER_MONTH);
    let migawka = ctx
        .session
        .market
        .as_ref()
        .and_then(|m| m.shop_panel(s, od, t));

    if let Some(snap) = migawka {
        let card = ShopCard::build(
            ctx.c,
            ctx.l,
            &ShopView {
                snapshot: &snap,
                kind: snap.kind,
                period_from: od,
            },
        );
        let mut naglowek: Rich = vec![Span::emphasis(format!("{nazwa}\n"))];
        naglowek.extend(card.render_header(ctx.c, ctx.l));
        naglowek.push(Span::plain(format!("{}: ", ctx.text("ui.firm.owner"))));
        naglowek.push(ctx.subject_span(Subject::Firm(snap.firm)));
        naglowek.push(Span::plain("\n".to_string()));

        let mut dlaczego: Rich = Vec::new();
        for r in &snap.reprices {
            dlaczego.push(Span::plain(format!(
                "{}\n",
                magnat_ui::describe(ctx.c, ctx.l, *r)
            )));
        }
        return InspectionCard::new(subject, naglowek)
            .tab(
                CardTabKind::Shelves,
                card.render_tab(ctx.c, ctx.l, ShopTab::Shelves),
            )
            .tab(
                CardTabKind::Customers,
                card.render_tab(ctx.c, ctx.l, ShopTab::Customers),
            )
            .tab(
                CardTabKind::Competition,
                card.render_tab(ctx.c, ctx.l, ShopTab::Competition),
            )
            .tab(CardTabKind::Why, dlaczego);
    }

    let naglowek = magnat_ui::lines_titled(&format!("{nazwa}\n"));
    let mut stan: Rich = Vec::new();
    match firms(ctx.session).and_then(|r| r.site(s)) {
        Some(site) => {
            ctx.link_line(
                &mut stan,
                "ui.firm.owner",
                Subject::Firm(owner_id(site.firm)),
            );
            ctx.link_line(
                &mut stan,
                "ui.card.building",
                Subject::Building(site.building),
            );
            ctx.link_line(
                &mut stan,
                "ui.card.district",
                Subject::District(site.district),
            );
            ctx.line(
                &mut stan,
                "ui.site.staff",
                &[("ile", &site.positions.len().to_string())],
            );
        }
        None => stan.push(magnat_ui::gone_span(ctx.c, ctx.l, &nazwa)),
    }
    InspectionCard::new(subject, naglowek).tab(CardTabKind::State, stan)
}

fn owner_id(k: FirmKey) -> FirmId {
    magnat_firms::firm_id(k)
}

/// Karta pojazdu: stan techniczny, paliwo i właściciel.
#[must_use]
pub fn vehicle_card(ctx: &CardCtx<'_>, v: VehicleId) -> InspectionCard {
    let subject = Subject::Vehicle(v);
    let nazwa = vehicle_name(ctx, v);
    let naglowek = magnat_ui::lines_titled(&format!("{nazwa}\n"));
    let world = &ctx.session.app.world;
    let mut stan: Rich = Vec::new();

    let Some(owner) = world
        .get::<magnat_traffic::VehicleOwner>(v.entity())
        .copied()
    else {
        return InspectionCard::new(subject, naglowek).tab(
            CardTabKind::State,
            vec![
                magnat_ui::gone_span(ctx.c, ctx.l, &nazwa),
                Span::plain("\n".to_string()),
            ],
        );
    };

    if let Some(cond) = world.get::<magnat_traffic::VehicleCondition>(v.entity()) {
        ctx.line(
            &mut stan,
            "ui.vehicle.wear",
            &[("ile", &cond.wear.to_string())],
        );
    }
    if let Some(tank) = world.get::<magnat_traffic::FuelTank>(v.entity()) {
        // Poziom paliwa jest w mikrolitrach (`K-25`); gracz czyta litry.
        ctx.line(
            &mut stan,
            "ui.vehicle.fuel",
            &[("litry", &format!("{:.1}", tank.level as f64 / 1_000_000.0))],
        );
    }
    if owner.primary_driver != magnat_traffic::VehicleOwner::NO_DRIVER {
        if let Some(e) = magnat_agents::citizen_by_index(world, owner.primary_driver) {
            ctx.link_line(
                &mut stan,
                "ui.vehicle.driver",
                Subject::Citizen(magnat_core::CitizenId(e)),
            );
        }
    }
    if owner.kind == magnat_traffic::OwnerKind::Household as u8 {
        if let Some(e) = magnat_agents::household_by_index(world, owner.owner) {
            ctx.link_line(
                &mut stan,
                "ui.card.household",
                Subject::Household(magnat_core::HouseholdId(e)),
            );
        }
    }
    InspectionCard::new(subject, naglowek).tab(CardTabKind::State, stan)
}
