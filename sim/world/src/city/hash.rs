//! Odciski warstwy M2 — `city_hash` i `world_hash_m2` — oraz dwie liczby do raportu.
//!
//! Wydzielone z `city/mod.rs` w R-WP9 bez zmiany zachowania. Ciała obu funkcji
//! haszujących przeniesione **dosłownie**: pominięte pole nie jest błędem, który
//! kompilator złapie, tylko takim, który widać wyłącznie w porównaniu hashy.

use super::*;

/// Mediana wartości gruntu po działkach zabudowywalnych — jedna liczba do raportu.
pub(super) fn mediana_wartosci(parcels: &ParcelSet) -> magnat_core::Money {
    let mut v: Vec<i64> = parcels
        .parcels
        .iter()
        .filter(|p| p.zone.parcelled())
        .map(|p| p.land_value_per_m2.0)
        .collect();
    if v.is_empty() {
        return magnat_core::Money(0);
    }
    v.sort_unstable();
    magnat_core::Money(v[v.len() / 2])
}

/// Odcisk warstwy M2c. Kolejność jest kolejnością tablic, a te powstają deterministycznie
/// (00 §3.6) — dwa przebiegi tego samego ziarna muszą dać ten sam hash.
#[must_use]
pub fn city_hash(
    blocks: &BlockSet,
    zones: &ZoneResult,
    districts: &DistrictSet,
    parcels: &ParcelSet,
    buildings: &build::BuildingSet,
) -> StateHash {
    let mut h = StateHasher::new();
    h.write_u32(blocks.blocks.len() as u32);
    for (i, b) in blocks.blocks.iter().enumerate() {
        h.write_u32(b.area_m2);
        h.write_u16(b.district.0);
        h.write_u16(b.neighborhood);
        h.write_u8(zones.zone[i].index() as u8);
        h.write_u8(zones.epoch_ring[i]);
        h.write_u32(b.parcels.start);
        h.write_u32(b.parcels.end);
    }
    h.write_u32(districts.districts.len() as u32);
    for d in &districts.districts {
        for b in d.name.as_bytes() {
            h.write_u8(*b);
        }
        h.write_u8(d.kind as u8);
        h.write_u8(d.founded_epoch.0);
        h.write_u16(d.style.0);
        h.write_u8(d.income_tier);
        h.write_u8(d.reputation.get());
        h.write_u8(d.crime.get());
        h.write_u32(d.pop_capacity);
    }
    h.write_u32(parcels.parcels.len() as u32);
    for p in &parcels.parcels {
        h.write_u32(p.block.0);
        h.write_u16(p.district.0);
        h.write_u32(p.area_m2);
        h.write_u8(p.zone.index() as u8);
        h.write_u32(p.frontage.seg.0);
        h.write_u32(p.frontage.t0.to_bits());
        h.write_u32(p.frontage.t1.to_bits());
        h.write_i64(p.land_value_per_m2.0);
    }
    // ── M2d: zabudowa i wnętrza ─────────────────────────────────────────────────────
    h.write_u32(buildings.buildings.len() as u32);
    for b in &buildings.buildings {
        h.write_u32(b.parcel.0.index());
        h.write_u16(b.grammar.0);
        h.write_u8(b.epoch.0);
        h.write_u8(b.floors);
        h.write_u8(b.basements);
        h.write_u16(b.height_dm);
        h.write_u32(b.gross_area_m2);
        h.write_u8(b.condition.get());
        for fh in &b.floor_heights_dm {
            h.write_u16(*fh);
        }
        h.write_u32(b.entrances.len() as u32);
        for e in &b.entrances {
            h.write_u32(e.seg.0);
            h.write_u32(e.t.to_bits());
            h.write_u8(e.kind as u8);
        }
    }
    h.write_u32(buildings.units.len() as u32);
    for u in &buildings.units {
        h.write_u32(u.building.0.index());
        h.write_i64(i64::from(u.floor));
        h.write_u16(u.area_m2);
        h.write_i64(u.rent_hint.0);
        h.write_u8(match u.kind {
            UnitKind::Dwelling { rooms } => 16 + rooms,
            UnitKind::Retail => 1,
            UnitKind::Office => 2,
            UnitKind::Workshop => 3,
            UnitKind::Storage => 4,
            UnitKind::Common => 5,
        });
    }
    h.write_u32(buildings.workplaces.len() as u32);
    for w in &buildings.workplaces {
        h.write_u32(w.unit.0);
        h.write_u16(w.role.0);
        h.write_u8(w.shift as u8);
        h.write_i64(w.wage_band.median.0);
    }
    h.finish()
}

/// Odcisk całej warstwy M2 (D1 i D5 z §7 fazy, dok. 00 §3.6).
///
/// Buduje się na `city_hash`, a nie obok niego (korekta F5): tamten obejmuje już strefy,
/// dzielnice, parcele, budynki, lokale, stanowiska i `land_value_per_m2`. Tu dochodzi
/// Etap 7 — firmy, zakłady i wynik domknięcia łańcuchów.
#[must_use]
pub fn world_hash_m2(city: &StateHash, sites: &sites::SiteSet) -> StateHash {
    let mut h = StateHasher::new();
    h.write_u64(city.0 as u64);
    h.write_u64((city.0 >> 64) as u64);
    h.write_u32(sites.firms.len() as u32);
    for f in &sites.firms {
        for b in f.name.as_bytes() {
            h.write_u8(*b);
        }
        h.write_u8(f.sector as u8);
        h.write_u32(f.sites.len() as u32);
    }
    h.write_u32(sites.sites.len() as u32);
    for s in &sites.sites {
        h.write_u32(s.firm.0.index());
        h.write_u32(s.building.0.index());
        h.write_u32(s.parcel.0.index());
        h.write_u16(s.archetype.0);
        h.write_u16(s.capacity_scale);
        h.write_u32(s.units.start);
        h.write_u32(s.units.end);
        h.write_u32(s.workplaces.start);
        h.write_u32(s.workplaces.end);
        for r in &s.recipes {
            h.write_u16(r.0);
        }
    }
    // Wynik domknięcia wchodzi do odcisku, bo zmienia `capacity_scale` — a to jest stan
    // trwały, z którego M6 wyprowadzi zdolności produkcyjne.
    for (k, t) in &sites.closure.imported {
        for b in k.as_bytes() {
            h.write_u8(*b);
        }
        h.write_i64(*t);
    }
    h.finish()
}

/// Liczba segmentów spełniających warunek — raport mówi o **gotowej** sieci,
/// a nie o tym, co powstało przed przycięciem wiszących końców.
pub(super) fn policz(net: &RoadNetwork, f: impl Fn(&RoadSegment) -> bool) -> u32 {
    net.segments.iter().filter(|s| f(s)).count() as u32
}
