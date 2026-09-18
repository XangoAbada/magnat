//! Karty budynku, parceli, dzielnicy i rady miasta (M9c §5.7).
//!
//! Wszystkie cztery czytają `CityData` — i to jest powód, dla którego karta składa się
//! w `game/`, a nie w `engine/ui`: interfejs nie zależy od `sim/world`.
//!
//! `sim/world` ma już `parcel_card`, która zwraca **wiersze tekstu** pod kliknięcie
//! w teren. Nie podmieniamy jej: tamta odpowiada na „co jest w tym punkcie", ta na
//! „co to jest za parcela" i musi nieść odnośniki. Pokrywają się w trzech wierszach
//! i to jest cena, której nie warto płacić przebudową cudzego modułu.

use super::CardCtx;
use magnat_core::{BuildingId, DistrictId, ParcelId, Subject};
use magnat_ui::{CardTabKind, InspectionCard, Rich, Span};
use magnat_world::city::build::UnitOccupant;
use magnat_world::CityData;

fn city(session: &crate::Session) -> &CityData {
    &session.built.city
}

#[must_use]
pub fn building_exists(session: &crate::Session, b: BuildingId) -> bool {
    (b.entity().index() as usize) < city(session).buildings.buildings.len()
}

#[must_use]
pub fn parcel_exists(session: &crate::Session, p: ParcelId) -> bool {
    (p.entity().index() as usize) < city(session).parcels.parcels.len()
}

#[must_use]
pub fn district_exists(session: &crate::Session, d: DistrictId) -> bool {
    (d.get() as usize) < city(session).districts.districts.len()
}

#[must_use]
pub fn building_name(ctx: &CardCtx<'_>, b: BuildingId) -> String {
    ctx.fmt(
        "ui.subject.building",
        &[("nr", &b.entity().index().to_string())],
    )
}

/// Nazwa dzielnicy jest proceduralna (`data/names/districts_*.ron`), więc nie ma klucza.
#[must_use]
pub fn district_name(ctx: &CardCtx<'_>, d: DistrictId) -> String {
    city(ctx.session)
        .districts
        .districts
        .get(d.get() as usize)
        .map_or_else(
            || ctx.fmt("ui.subject.district", &[("nr", &d.get().to_string())]),
            |x| x.name.clone(),
        )
}

/// Karta budynku: kondygnacje, lokatorzy i najemcy, stan techniczny.
#[must_use]
pub fn building_card(ctx: &CardCtx<'_>, b: BuildingId) -> InspectionCard {
    let subject = Subject::Building(b);
    let nazwa = building_name(ctx, b);
    let naglowek = magnat_ui::lines_titled(&format!("{nazwa}\n"));
    let c = city(ctx.session);
    let Some(bud) = c.buildings.buildings.get(b.entity().index() as usize) else {
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
        "ui.building.floors",
        &[
            ("nad", &bud.floors.to_string()),
            ("pod", &bud.basements.to_string()),
        ],
    );
    ctx.line(
        &mut stan,
        "ui.building.area",
        &[("m2", &bud.gross_area_m2.to_string())],
    );
    ctx.line(
        &mut stan,
        "ui.building.condition",
        &[("ile", &bud.condition.get().to_string())],
    );
    ctx.link_line(&mut stan, "ui.card.parcel", Subject::Parcel(bud.parcel));
    if let Some(p) = c.parcels.parcels.get(bud.parcel.entity().index() as usize) {
        ctx.link_line(&mut stan, "ui.card.district", Subject::District(p.district));
    }

    // Lokatorzy i najemcy w jednej zakładce, bo gracz pyta „kto tu jest", a nie
    // „kto tu mieszka" osobno od „kto tu handluje".
    let mut lokatorzy: Rich = Vec::new();
    let zakres = bud.units.start as usize..bud.units.end as usize;
    for u in c.buildings.units.get(zakres).unwrap_or_default() {
        match u.occupant {
            UnitOccupant::Household(h) => {
                ctx.link_line(&mut lokatorzy, "ui.building.tenant", Subject::Household(h));
            }
            UnitOccupant::Site(s) => {
                ctx.link_line(&mut lokatorzy, "ui.building.lessee", Subject::Site(s));
            }
            UnitOccupant::Vacant => {}
        }
    }
    if lokatorzy.is_empty() {
        ctx.line(&mut lokatorzy, "ui.building.empty", &[]);
    }

    InspectionCard::new(subject, naglowek)
        .tab(CardTabKind::State, stan)
        .tab(CardTabKind::Family, lokatorzy)
}

/// Karta parceli: teren, strefa, zabudowa, wartość gruntu z rozbiciem na czynniki.
#[must_use]
pub fn parcel_card(ctx: &CardCtx<'_>, p: ParcelId) -> InspectionCard {
    let subject = Subject::Parcel(p);
    let nazwa = ctx.subject_name(subject);
    let naglowek = magnat_ui::lines_titled(&format!("{nazwa}\n"));
    let c = city(ctx.session);
    let Some(par) = c.parcels.parcels.get(p.entity().index() as usize) else {
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
        "ui.parcel.zone",
        &[("strefa", &ctx.text(&format!("ui.zone.{}", par.zone.key())))],
    );
    ctx.line(
        &mut stan,
        "ui.parcel.area",
        &[("m2", &par.area_m2.to_string())],
    );
    ctx.line(
        &mut stan,
        "ui.parcel.value",
        &[("kwota", &magnat_ui::zlotowki(par.land_value_per_m2))],
    );
    ctx.link_line(
        &mut stan,
        "ui.card.district",
        Subject::District(par.district),
    );
    if let Some(b) = par.building {
        ctx.link_line(&mut stan, "ui.card.building", Subject::Building(b));
    } else {
        ctx.line(&mut stan, "ui.parcel.vacant", &[]);
    }

    InspectionCard::new(subject, naglowek).tab(CardTabKind::State, stan)
}

/// Karta dzielnicy: ludzie, reputacja, wartość gruntu.
#[must_use]
pub fn district_card(ctx: &CardCtx<'_>, d: DistrictId) -> InspectionCard {
    let subject = Subject::District(d);
    let nazwa = district_name(ctx, d);
    let naglowek = magnat_ui::lines_titled(&format!("{nazwa}\n"));
    let c = city(ctx.session);
    let Some(dz) = c.districts.districts.get(d.get() as usize) else {
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
        "ui.district.capacity",
        &[("ile", &dz.pop_capacity.to_string())],
    );
    ctx.line(
        &mut stan,
        "ui.district.income",
        &[("tier", &dz.income_tier.to_string())],
    );
    ctx.line(
        &mut stan,
        "ui.district.reputation",
        &[("ile", &dz.reputation.get().to_string())],
    );
    ctx.line(
        &mut stan,
        "ui.district.crime",
        &[("ile", &dz.crime.get().to_string())],
    );
    ctx.line(
        &mut stan,
        "ui.district.land_value",
        &[("kwota", &magnat_ui::zlotowki(dz.avg_land_value))],
    );

    // Pokrycie usługami publicznymi — tablica dzielnica × rodzaj, którą `sim/city`
    // publikuje do `engine/core` właśnie po to, żeby czytali ją inni (`K-64`).
    let mut uslugi: Rich = Vec::new();
    if let Some(cov) = ctx
        .session
        .app
        .world
        .get_resource::<magnat_core::ServiceCoverage>()
    {
        for k in magnat_core::ServiceKind::ALL {
            uslugi.push(Span::plain(format!(
                "  {:<16} {:>3}/100\n",
                ctx.text(&format!("ui.service.{}", k.name())),
                cov.at(d, *k).get()
            )));
        }
    }

    InspectionCard::new(subject, naglowek)
        .tab(CardTabKind::State, stan)
        .tab(CardTabKind::Detail, uslugi)
}

/// Karta rady miasta: skład, poparcie, kasa.
///
/// Pełny panel „Miasto" (polityka, budżet, przetargi, wybory) jest panelem biznesowym
/// i należy do `M9e` — tutaj jest tożsamość i to, co widać bez jego tabel.
#[must_use]
pub fn government_card(ctx: &CardCtx<'_>) -> InspectionCard {
    let subject = Subject::Government;
    let nazwa = ctx.text("ui.subject.government");
    let naglowek = magnat_ui::lines_titled(&format!("{nazwa}\n"));
    let mut stan: Rich = Vec::new();
    match ctx
        .session
        .app
        .world
        .get_resource::<magnat_city::Government>()
    {
        Some(g) => {
            let world = &ctx.session.app.world;
            match magnat_agents::citizen_by_index(world, g.mayor) {
                Some(e) => ctx.link_line(
                    &mut stan,
                    "ui.gov.mayor",
                    Subject::Citizen(magnat_core::CitizenId(e)),
                ),
                None => ctx.line(&mut stan, "ui.gov.vacancy", &[]),
            }
            ctx.line(
                &mut stan,
                "ui.gov.council",
                &[("ile", &g.council.len().to_string())],
            );
            for (i, c) in g.council.iter().enumerate() {
                if let Some(e) = magnat_agents::citizen_by_index(world, c.0) {
                    ctx.link_line(
                        &mut stan,
                        "ui.gov.councillor",
                        Subject::Citizen(magnat_core::CitizenId(e)),
                    );
                }
                let _ = i;
            }
        }
        None => ctx.line(&mut stan, "ui.gov.none", &[]),
    }
    InspectionCard::new(subject, naglowek).tab(CardTabKind::State, stan)
}
