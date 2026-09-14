//! Karta inspekcji parceli (M2 §1 artefakt 2, WP16; wymóg wyjaśnialności — dok. 00 §7).
//!
//! Mieszka w `sim/world`, a nie w narzędziu, **z rozmysłu**: kryterium WP16 mówi, że
//! klient graficzny ma pokazać to samo, co wersja headless. Dwie kopie tej listy
//! rozjechałyby się przy pierwszym nowym polu, a wtedy zrzut z `headless preview
//! --inspect` przestałby być dowodem na to, co widzi gracz (korekta D10 mówi to samo
//! od strony rusztowania: karta z M2c ma zostać rozszerzona, nie napisana drugi raz).
//!
//! Karta zwraca **wiersze tekstu**, nie widżety: `engine/ui` powstaje w M3, a do tego
//! czasu obie ścieżki wypisują to samo na konsolę.

use super::value;
use super::CityData;
use magnat_spatial::Vec2;

/// Parcela pod punktem — `ParcelTree` daje kandydatów, wielokąt rozstrzyga.
#[must_use]
pub fn parcel_at(city: &CityData, p: Vec2) -> Option<magnat_core::ParcelId> {
    let geom = &city.roads.geom;
    city.parcels.tree.at_point(p, |id| {
        let i = id.0.index() as usize;
        super::poly::contains(geom.get(city.parcels.parcels[i].poly), p)
    })
}

/// Pełna karta: parcela, wycena z rozbiciem, budynek, zakład, dzielnica.
#[must_use]
pub fn parcel_card(city: &CityData, punkt: Vec2) -> Vec<String> {
    let Some(id) = parcel_at(city, punkt) else {
        return vec![format!("{:.0},{:.0}: brak parceli", punkt.x, punkt.y)];
    };
    let parcel = &city.parcels.parcels[id.0.index() as usize];
    let block = &city.blocks.blocks[parcel.block.0 as usize];
    let mut v = vec![
        format!(
            "parcela #{} · {} · {} m² · {:?}",
            id.0.index(),
            parcel.zone.key(),
            parcel.area_m2,
            parcel.status
        ),
        format!("właściciel: {:?}", parcel.owner),
    ];
    if parcel.frontage.is_none() {
        v.push("front: brak (podwórko)".to_string());
    } else {
        let seg = &city.roads.segments[parcel.frontage.seg.0 as usize];
        v.push(format!(
            "front: segment {} ({}) · t {:.3}..{:.3} · {:.1} m",
            parcel.frontage.seg.0,
            seg.class.key(),
            parcel.frontage.t0,
            parcel.frontage.t1,
            (parcel.frontage.t1 - parcel.frontage.t0) * f32::from(seg.length_dm as u16) / 10.0
        ));
    }
    v.push(format!(
        "kwartał {} · osiedle {} · pierścień epoki {}",
        block.id.0, block.neighborhood, block.epoch_ring
    ));

    // ── wycena z rozbiciem: kryterium WP16 („rozbicie na ≥ 8 czynników") ────────────
    let (wartosc, rozbicie) = value::land_value_at(city, id);
    v.push(format!(
        "wartość gruntu (pass_2): {:.2} zł/m² · łącznie {:.0} zł · baza strefy {:.2} zł/m²",
        super::overlay::zlote(wartosc),
        super::overlay::zlote(wartosc) * f64::from(parcel.area_m2),
        super::overlay::zlote(rozbicie.base)
    ));
    // Posortowane po **wielkości wpływu**, nie po kolejności enuma: karta odpowiada
    // na pytanie „dlaczego tyle", a nie „jakie są czynniki".
    let mut czynniki = rozbicie.factors;
    czynniki.sort_by_key(|(f, pp)| (-i32::from(pp.abs()), *f));
    for (f, pp) in czynniki {
        v.push(format!("    {:<22} {pp:+} %", f.key()));
    }

    match parcel.building {
        None => v.push("zabudowa: brak".to_string()),
        Some(bid) => {
            let b = &city.buildings.buildings[bid.0.index() as usize];
            let lokale = &city.buildings.units[b.units.start as usize..b.units.end as usize];
            let stanowisk: u32 = lokale.iter().map(|u| u.workplaces.len() as u32).sum();
            v.push(format!(
                "budynek #{}: gramatyka {} · {} kondygnacji (+{} podziemnych) · {:.1} m · {} m² brutto · stan {}/100",
                bid.0.index(),
                b.grammar.0,
                b.floors,
                b.basements,
                f64::from(b.height_dm) / 10.0,
                b.gross_area_m2,
                b.condition.get()
            ));
            v.push(format!(
                "lokale: {} ({} mieszkań) · stanowisk pracy {} · czynsz wywoławczy {:.0}–{:.0} zł/mies.",
                lokale.len(),
                lokale.iter().filter(|u| u.kind.is_dwelling()).count(),
                stanowisk,
                lokale.iter().map(|u| u.rent_hint.0).min().unwrap_or(0) as f64 / 100.0,
                lokale.iter().map(|u| u.rent_hint.0).max().unwrap_or(0) as f64 / 100.0
            ));
            v.push(format!(
                "wejścia: {}",
                b.entrances
                    .iter()
                    .map(|e| format!("{:?}@seg{}", e.kind, e.seg.0))
                    .collect::<Vec<_>>()
                    .join(" ")
            ));
            if let Some(site) = city.sites.site_of_building(bid) {
                let a = city.site_catalog.get(site.archetype);
                let firm = &city.sites.firms[site.firm.0.index() as usize];
                v.push(format!(
                    "zakład: {} ({}) · skala {:.2} × bazowa · stanowisk {} · firma {} ({} zakładów)",
                    a.key(),
                    a.sector().key(),
                    f64::from(site.capacity_scale) / 1000.0,
                    site.workplaces.len(),
                    firm.name,
                    firm.sites.len()
                ));
                if !site.recipes.is_empty() {
                    v.push(format!(
                        "  receptury: {}",
                        site.recipes
                            .iter()
                            .map(|r| city.catalog.recipe(*r).key().to_string())
                            .collect::<Vec<_>>()
                            .join(" · ")
                    ));
                }
            }
        }
    }

    if let Some(d) = city.districts.districts.get(parcel.district.0 as usize) {
        v.push(format!(
            "dzielnica {}: {} ({:?}, styl {}) · dochód {}/4 · reputacja {} · przestępczość {} · pojemność {} os. · średnia wartość gruntu {:.2} zł/m²",
            parcel.district.0,
            d.name,
            d.kind,
            d.style.0,
            d.income_tier,
            d.reputation.get(),
            d.crime.get(),
            d.pop_capacity,
            super::overlay::zlote(d.avg_land_value)
        ));
    }
    v
}
