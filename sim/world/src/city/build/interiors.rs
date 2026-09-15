//! Wnętrza logiczne: stan techniczny, podział kondygnacji na lokale i czynsz
//! startowy (M2 §5.6).
//!
//! Wydzielone z `city/build.rs` w R-WP8 bez zmiany zachowania.

use super::*;

/// Stan techniczny: im starszy pierścień, tym gorszy, im droższy grunt, tym lepszy.
pub(super) fn stan_techniczny(
    input: &BuildInput,
    p: &Planned,
    parcels: &ParcelSet,
) -> magnat_core::Q {
    let parcel = &parcels.parcels[p.parcel as usize];
    let ring = input.blocks.blocks[parcel.block.0 as usize].epoch_ring;
    let n = input.zones.rings.len().max(1) as f32;
    let wiek = 1.0 - f32::from(ring) / n;
    let mut r = rng(
        input.plan.seed,
        StreamId::Interiors,
        p.parcel,
        Tick(u64::MAX),
    );
    let szum = (r.next_u32() % 21) as i32 - 10;
    let bogactwo = (parcel.land_value_per_m2.0 / 2000).clamp(0, 20) as i32;
    let v = (88.0 - 42.0 * wiek) as i32 + bogactwo + szum;
    magnat_core::Q::new(v.clamp(5, 100) as u8)
}

/// Wnętrza logiczne: podział kondygnacji na lokale i obsadzenie ich stanowiskami.
/// Zwraca `(pierwszy_lokal, mieszkań, stanowisk)`.
pub(super) fn interiors(
    input: &BuildInput,
    out: &mut BuildingSet,
    p: &Planned,
    id: BuildingId,
    parcels: &ParcelSet,
) -> (u32, u32, u32) {
    let g = input.grammars.get(p.grammar);
    let parcel = &parcels.parcels[p.parcel as usize];
    let block = &input.blocks.blocks[parcel.block.0 as usize];
    let epoch_key = input
        .zones
        .rings
        .get(usize::from(block.epoch_ring))
        .map_or("contemporary", |e| e.key.as_str());
    let epoch_mult = input.jobs.epoch_mult(epoch_key);
    let tier = input
        .districts
        .districts
        .get(parcel.district.0 as usize)
        .map_or(2, |d| d.income_tier);

    let od = out.units.len() as u32;
    let mut mieszkan = 0u32;
    let mut stanowisk = 0u32;
    let uzytkowa = p.base.area_m2() * (1.0 - g.interior.circulation_share.clamp(0.0, 0.6));
    let mut r = rng(input.plan.seed, StreamId::Interiors, p.parcel, Tick(0));

    // Piwnice: jeden lokal gospodarczy na kondygnację podziemną.
    for b in 0..p.derived.basements {
        out.units.push(Unit {
            building: id,
            floor: -(i32::from(b) + 1) as i8,
            kind: UnitKind::Storage,
            area_m2: uzytkowa.clamp(1.0, f32::from(u16::MAX)) as u16,
            occupant: UnitOccupant::Vacant,
            rent_hint: Money(0),
            workplaces: 0..0,
        });
    }

    for f in 0..p.derived.floors {
        let spec = if f == 0 {
            g.interior.ground
        } else if f + 1 == p.derived.floors {
            g.interior.top
        } else {
            g.interior.typical
        };
        let (lo, hi) = (f32::from(spec.area_m2.0.max(1)), f32::from(spec.area_m2.1));
        let cel = lo + (hi - lo) * ((r.next_u32() % 1000) as f32 / 1000.0);
        let n = (uzytkowa / cel.max(1.0)).round().max(1.0) as u32;
        let pole = (uzytkowa / n as f32).max(1.0);
        for _ in 0..n {
            let kind = rodzaj(spec.kind, pole, &mut r);
            let rent = czynsz(parcel.land_value_per_m2, pole, kind);
            let wp_od = out.workplaces.len() as u32;
            if let Some((role_id, role)) = input.jobs.role_for(kind) {
                let na_stanowisko = f32::from(role.m2_per_workplace.max(1));
                let ile = (pole / na_stanowisko).round().max(1.0) as u32;
                let band = wage_band(role, epoch_mult, tier);
                for _ in 0..ile {
                    out.workplaces.push(Workplace {
                        unit: UnitIdx(out.units.len() as u32),
                        site: None,
                        role: role_id,
                        shift: role.shift.into(),
                        wage_band: band,
                        occupant: None,
                    });
                }
                stanowisk += ile;
            }
            if kind.is_dwelling() {
                mieszkan += 1;
            }
            out.units.push(Unit {
                building: id,
                floor: f as i8,
                kind,
                area_m2: pole.clamp(1.0, f32::from(u16::MAX)) as u16,
                occupant: UnitOccupant::Vacant,
                rent_hint: rent,
                workplaces: wp_od..out.workplaces.len() as u32,
            });
        }
    }
    (od, mieszkan, stanowisk)
}

fn rodzaj(k: UnitClass, pole_m2: f32, r: &mut Rng) -> UnitKind {
    match k {
        UnitClass::Dwelling => {
            // Pokoje z metrażu, z jednym losowaniem na lokal: 28 m² na pokój plus kuchnia.
            let baza = (pole_m2 / 28.0).round().clamp(1.0, 7.0) as u8;
            let odchyl = u8::from(r.next_u32().is_multiple_of(4));
            UnitKind::Dwelling {
                rooms: (baza + odchyl).clamp(1, 8),
            }
        }
        UnitClass::Retail => UnitKind::Retail,
        UnitClass::Office => UnitKind::Office,
        UnitClass::Workshop => UnitKind::Workshop,
        UnitClass::Storage => UnitKind::Storage,
    }
}

/// `rent_hint`: podpowiedź startowa dla M5, nie cena. Proporcjonalna do wartości gruntu
/// i powierzchni; kalibracja bezwzględna należy do balansatora (M5), bo to on ma dane
/// o dochodach. Liczone w groszach, bez floata w akumulacji (00 §2).
fn czynsz(land_value_per_m2: Money, pole_m2: f32, kind: UnitKind) -> Money {
    let mnoznik = match kind {
        UnitKind::Retail => 14,
        UnitKind::Office => 11,
        UnitKind::Workshop => 5,
        UnitKind::Storage => 3,
        UnitKind::Dwelling { .. } => 8,
        UnitKind::Common => 0,
    };
    let pole = pole_m2.clamp(0.0, 1.0e6) as i64;
    Money(
        land_value_per_m2
            .0
            .saturating_mul(pole)
            .saturating_mul(mnoznik),
    )
    .div_round_half_up(1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn czynsz_rosnie_z_wartoscia_gruntu() {
        let tanio = czynsz(Money(10_000), 50.0, UnitKind::Dwelling { rooms: 2 });
        let drogo = czynsz(Money(40_000), 50.0, UnitKind::Dwelling { rooms: 2 });
        assert!(drogo.0 > tanio.0 && tanio.0 > 0);
    }
}
