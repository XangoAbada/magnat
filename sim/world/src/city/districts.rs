//! WP9 — dzielnice i ich tożsamość (M2 §5.5, PRD §4.3).
//!
//! Zalążki (centrum, bramy, klastry przemysłowe, próbkowanie Poissona) → Voronoi po
//! **kwartałach** z metryką po sieci dróg i karami za różnicę epoki i strefy → doginanie
//! granic do arterii i rzek → tożsamość: rodzaj, styl, reputacja, przestępczość,
//! przedział dochodowy i nazwa.
//!
//! Hierarchia jest tablicowa (M2 §5.5): po przypisaniu kwartały są **przenumerowane**
//! tak, żeby kwartały jednej dzielnicy leżały w ciągłym zakresie. Stąd kolejność
//! pakietów: WP9 idzie przed WP8, bo przenumerowanie po powstaniu parcel wymagałoby
//! przestawienia dwóch tablic zamiast jednej (korekta C8).

use super::blocks::{Block, BlockId, BlockSet};
use super::gates::GateKind;
use super::poly;
use super::road::{PolyArena, PolyRef, RoadClass, RoadFlags, RoadNetwork, SegmentId};
use super::zoning::{CityFields, EpochId, StyleId, ZoneKind, ZoneResult};
use super::CityPlan;
use crate::assets::data_path;
use crate::query::TerrainQuery;
use magnat_core::{rng, DistrictId, Money, Q, StreamId, Tick};
use magnat_spatial::Vec2;
use serde::{Deserialize, Serialize};
use std::ops::Range;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum DistrictKind {
    OldTown,
    InnerCity,
    BlockEstate,
    Suburb,
    IndustrialBelt,
    PortQuarter,
    Village,
    Campus,
    GreenBelt,
}

impl DistrictKind {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            DistrictKind::OldTown => "old_town",
            DistrictKind::InnerCity => "inner_city",
            DistrictKind::BlockEstate => "block_estate",
            DistrictKind::Suburb => "suburb",
            DistrictKind::IndustrialBelt => "industrial_belt",
            DistrictKind::PortQuarter => "port_quarter",
            DistrictKind::Village => "village",
            DistrictKind::Campus => "campus",
            DistrictKind::GreenBelt => "green_belt",
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct District {
    /// Nazwa proceduralna z `data/names/` — **nie jest** lokalizacją UI (CLAUDE.md).
    pub name: String,
    pub kind: DistrictKind,
    pub founded_epoch: EpochId,
    pub style: StyleId,
    pub boundary: PolyRef,
    pub blocks: Range<u32>,
    pub centroid: Vec2,
    pub reputation: Q,
    pub crime: Q,
    /// 0..=4 — M3 mapuje na klasy społeczne (M2 §9.1/5).
    pub income_tier: u8,
    /// Za m². Liczone po WP15 (M2e); tu zero.
    pub avg_land_value: Money,
    pub pop_capacity: u32,
}

/// Kara metryki Voronoi za różnicę pierścienia epoki, w metrach ekwiwalentnych.
const EPOCH_PENALTY_M: f32 = 70.0;
/// Kara za różnicę klasy strefy, w metrach ekwiwalentnych.
const ZONE_PENALTY_M: f32 = 100.0;
/// Klaster przemysłowy od tej powierzchni dostaje własny zalążek (M2 §5.5 krok 1).
const CLUSTER_MIN_M2: f64 = 300_000.0;
/// Docelowa liczba mieszkańców na dzielnicę (M2 §5.5 krok 1).
const POP_PER_DISTRICT: u32 = 12_000;
/// Kwartałów na „osiedle".
const BLOCKS_PER_NEIGHBORHOOD: u32 = 8;

/// Ludzi na hektar według gęstości zabudowy — proxy `pop_capacity` do czasu, aż M2d
/// policzy lokale (korekta C9).
fn pop_per_ha(z: ZoneKind) -> f32 {
    use super::zoning::ResDensity as D;
    match z {
        ZoneKind::Residential(D::R1) => 30.0,
        ZoneKind::Residential(D::R2) => 65.0,
        ZoneKind::Residential(D::R3) => 160.0,
        ZoneKind::Residential(D::R4) => 250.0,
        ZoneKind::Residential(D::R5) => 400.0,
        _ => 0.0,
    }
}

// ── Nazwy ────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Toponym {
    River,
    Lake,
    Hill,
    Forest,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct DistrictNames {
    pub schema_version: u32,
    pub by_kind: Vec<(DistrictKind, Vec<String>)>,
    pub toponyms: Vec<(Toponym, Vec<String>)>,
    pub adjectives: Vec<String>,
    pub cores: Vec<String>,
    /// Sufiksy kierunkowe wymuszające unikalność.
    pub directions: Vec<String>,
}

pub const NAMES_SCHEMA_VERSION: u32 = 1;

impl DistrictNames {
    pub fn load() -> Result<DistrictNames, super::zoning::EpochError> {
        let txt = std::fs::read_to_string(data_path("names/districts_pl.ron"))
            .map_err(super::zoning::EpochError::Io)?;
        let n: DistrictNames = ron::from_str(&txt).map_err(super::zoning::EpochError::Ron)?;
        if n.schema_version != NAMES_SCHEMA_VERSION {
            return Err(super::zoning::EpochError::Schema {
                found: n.schema_version,
            });
        }
        if n.cores.is_empty() || n.adjectives.is_empty() {
            return Err(super::zoning::EpochError::Empty);
        }
        Ok(n)
    }

    fn pick(v: &[String], r: &mut magnat_core::Rng) -> Option<String> {
        if v.is_empty() {
            return None;
        }
        Some(v[r.gen_range_u32(v.len() as u32) as usize].clone())
    }
}

// ── Wynik ────────────────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq, Debug)]
pub struct DistrictSet {
    pub districts: Vec<District>,
    /// Permutacja kwartałów: `order[nowy] = stary`. Tablice indeksowane po kwartale
    /// (strefy, pierścienie epok) muszą przejść tym samym przestawieniem.
    pub order: Vec<u32>,
    pub seeds: u32,
    pub snapped: u32,
}

/// WP9 w całości. **Przenumerowuje kwartały** w `blocks` i przestawia `zones`.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn build_districts(
    plan: &CityPlan,
    t: &dyn TerrainQuery,
    net: &mut RoadNetwork,
    geom: &mut PolyArena,
    blocks: &mut BlockSet,
    zones: &mut ZoneResult,
    adj: &(Vec<u32>, Vec<u32>),
    fields: &CityFields,
    center: Vec2,
    names: &DistrictNames,
) -> DistrictSet {
    let n = blocks.blocks.len();
    if n == 0 {
        return DistrictSet {
            districts: Vec::new(),
            order: Vec::new(),
            seeds: 0,
            snapped: 0,
        };
    }
    let centroid: Vec<Vec2> = blocks
        .blocks
        .iter()
        .map(|b| poly::centroid(geom.get(b.poly)))
        .collect();

    // ── 1. Zalążki ──────────────────────────────────────────────────────────────────
    let cel = (plan.target_pop / POP_PER_DISTRICT).clamp(10, 40) as usize;
    let najblizszy = |p: Vec2| -> u32 {
        (0..n)
            .min_by_key(|&i| ((centroid[i] - p).length() * 1000.0) as u64)
            .unwrap_or(0) as u32
    };
    let mut seeds: Vec<u32> = vec![najblizszy(center)];
    for g in &net.gates {
        seeds.push(najblizszy(g.pos));
    }
    for c in klastry_przemyslowe(blocks, zones, adj, &centroid) {
        seeds.push(najblizszy(c));
    }
    seeds.sort_unstable();
    seeds.dedup();

    // Uzupełnienie próbkowaniem Poissona o d_min = sqrt(pole / liczba dzielnic).
    let urban: f64 = blocks.blocks.iter().map(|b| f64::from(b.area_m2)).sum();
    let d_min = magnat_core::det_math::sqrt(urban / cel.max(1) as f64) as f32;
    let mut r = rng(plan.seed, StreamId::Districts, 0, Tick(0));
    let mut kandydaci: Vec<u32> = (0..n as u32).collect();
    r.shuffle(&mut kandydaci);
    for k in kandydaci {
        if seeds.len() >= cel {
            break;
        }
        if seeds
            .iter()
            .all(|&s| (centroid[s as usize] - centroid[k as usize]).length() >= d_min)
        {
            seeds.push(k);
        }
    }
    seeds.sort_unstable();
    seeds.dedup();

    // ── 2. Voronoi po kwartałach ────────────────────────────────────────────────────
    let mut owner = voronoi(&seeds, adj, &centroid, zones, &blocks.blocks);

    // ── 3. Doginanie do barier ──────────────────────────────────────────────────────
    let snapped = dognij(net, blocks, adj, &mut owner);

    // ── 4. Przenumerowanie: kwartały jednej dzielnicy w ciągłym zakresie ────────────
    let mut order: Vec<u32> = (0..n as u32).collect();
    order.sort_by_key(|&i| (owner[i as usize], i));
    let mut nowy_indeks = vec![0u32; n];
    for (nowy, &stary) in order.iter().enumerate() {
        nowy_indeks[stary as usize] = nowy as u32;
    }
    przestaw(blocks, zones, &order, &mut owner);
    let centroid: Vec<Vec2> = order.iter().map(|&i| centroid[i as usize]).collect();

    // ── 5. Tożsamość ────────────────────────────────────────────────────────────────
    let mut districts: Vec<District> = Vec::new();
    let mut i = 0usize;
    let mut uzyte: Vec<String> = Vec::new();
    while i < n {
        let d = owner[i];
        let mut j = i;
        while j < n && owner[j] == d {
            j += 1;
        }
        let id = DistrictId(districts.len() as u16);
        for b in &mut blocks.blocks[i..j] {
            b.district = id;
        }
        for (k, b) in blocks.blocks[i..j].iter_mut().enumerate() {
            b.neighborhood = (k as u32 / BLOCKS_PER_NEIGHBORHOOD) as u16;
        }
        let zakres = i as u32..j as u32;
        let dist = tozsamosc(
            plan,
            t,
            net,
            geom,
            blocks,
            zones,
            fields,
            &centroid,
            zakres.clone(),
            names,
            &mut uzyte,
            center,
        );
        districts.push(dist);
        i = j;
    }

    // Przedział dochodowy: kwantyle punktacji zastępczej (wartość gruntu powstaje
    // dopiero w WP15 — korekta C10).
    let mut ranking: Vec<(usize, f32)> = districts
        .iter()
        .enumerate()
        .map(|(i, d)| (i, proxy_value(fields, d.centroid)))
        .collect();
    ranking.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    let ile = ranking.len().max(1);
    for (poz, (idx, _)) in ranking.into_iter().enumerate() {
        let tier = (poz * 5 / ile).min(4) as u8;
        districts[idx].income_tier = tier;
        let zielen = zielen_udzial(blocks, zones, districts[idx].blocks.clone());
        let halas = fields.noise.sample(districts[idx].centroid);
        districts[idx].reputation = Q::new(
            ((f32::from(tier) / 4.0 * 60.0 + zielen * 25.0 + (1.0 - halas) * 15.0) as i32)
                .clamp(0, 100) as u8,
        );
        let gestosc = fields.d_center.sample(districts[idx].centroid);
        districts[idx].crime = Q::new(
            (((4 - tier) as f32 / 4.0 * 55.0 + (1.0 - gestosc) * 20.0) as i32).clamp(0, 100) as u8,
        );
    }

    DistrictSet {
        districts,
        order,
        seeds: seeds.len() as u32,
        snapped,
    }
}

/// Punktacja zastępcza wartości gruntu: blisko centrum, cicho, ładnie.
fn proxy_value(f: &CityFields, p: Vec2) -> f32 {
    (1.0 - f.d_center.sample(p)) * 1.0 + f.amenity.sample(p) * 0.6 - f.noise.sample(p) * 0.8
        + (1.0 - f.access_road.sample(p)) * 0.4
}

fn zielen_udzial(blocks: &BlockSet, zones: &ZoneResult, r: Range<u32>) -> f32 {
    let mut zielen = 0.0f64;
    let mut calosc = 0.0f64;
    for i in r.start as usize..r.end as usize {
        let a = f64::from(blocks.blocks[i].area_m2);
        calosc += a;
        if matches!(zones.zone[i], ZoneKind::Green | ZoneKind::Agriculture) {
            zielen += a;
        }
    }
    (zielen / calosc.max(1.0)) as f32
}

/// Klastry przemysłowe ≥ 30 ha — spójne grupy kwartałów o strefie produkcyjnej.
fn klastry_przemyslowe(
    blocks: &BlockSet,
    zones: &ZoneResult,
    adj: &(Vec<u32>, Vec<u32>),
    centroid: &[Vec2],
) -> Vec<Vec2> {
    let przemysl = |z: ZoneKind| {
        matches!(
            z,
            ZoneKind::IndustryHeavy | ZoneKind::IndustryLight | ZoneKind::Logistics
        )
    };
    let n = blocks.blocks.len();
    let (start, items) = adj;
    let mut seen = vec![false; n];
    let mut out = Vec::new();
    for s in 0..n {
        if seen[s] || !przemysl(zones.zone[s]) {
            continue;
        }
        let mut stos = vec![s];
        seen[s] = true;
        let mut grupa = Vec::new();
        while let Some(v) = stos.pop() {
            grupa.push(v);
            for k in start[v]..start[v + 1] {
                let j = items[k as usize] as usize;
                if !seen[j] && przemysl(zones.zone[j]) {
                    seen[j] = true;
                    stos.push(j);
                }
            }
        }
        let pole: f64 = grupa.iter().map(|&i| f64::from(blocks.blocks[i].area_m2)).sum();
        if pole >= CLUSTER_MIN_M2 {
            let suma = grupa.iter().fold(Vec2::ZERO, |a, &i| a + centroid[i]);
            out.push(suma / grupa.len() as f32);
        }
    }
    out
}

/// Voronoi po grafie kwartałów: jedna Dijkstra wielu źródeł z propagacją właściciela.
fn voronoi(
    seeds: &[u32],
    adj: &(Vec<u32>, Vec<u32>),
    centroid: &[Vec2],
    zones: &ZoneResult,
    blocks: &[Block],
) -> Vec<u16> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    let n = blocks.len();
    let (start, items) = adj;
    let mut dist = vec![u32::MAX; n];
    let mut owner = vec![u16::MAX; n];
    let mut heap: BinaryHeap<Reverse<(u32, u32, u16)>> = BinaryHeap::new();
    for (k, &s) in seeds.iter().enumerate() {
        dist[s as usize] = 0;
        owner[s as usize] = k as u16;
        heap.push(Reverse((0, s, k as u16)));
    }
    while let Some(Reverse((d, v, o))) = heap.pop() {
        let v = v as usize;
        if d > dist[v] {
            continue;
        }
        for k in start[v]..start[v + 1] {
            let j = items[k as usize] as usize;
            let mut w = (centroid[j] - centroid[v]).length();
            w += (i32::from(zones.epoch_ring[j]) - i32::from(zones.epoch_ring[v])).unsigned_abs()
                as f32
                * EPOCH_PENALTY_M;
            if zones.zone[j].index() != zones.zone[v].index() {
                w += ZONE_PENALTY_M;
            }
            let nd = d.saturating_add(w as u32 + 1);
            // Remis rozstrzyga niższy numer zalążka — bez tego wynik zależałby
            // od kolejności wyjmowania z kopca przy równych odległościach.
            if nd < dist[j] || (nd == dist[j] && o < owner[j]) {
                dist[j] = nd;
                owner[j] = o;
                heap.push(Reverse((nd, j as u32, o)));
            }
        }
    }
    // Kwartały odcięte od każdego zalążka trafiają do dzielnicy 0 — lepsze niż dziura.
    for o in &mut owner {
        if *o == u16::MAX {
            *o = 0;
        }
    }
    owner
}

/// Doginanie granic do barier: kwartał, którego większość sąsiadów **przez ulicę
/// lokalną** (czyli nie przez arterię, kolej ani rzekę) siedzi w innej dzielnicy,
/// przechodzi do nich. Efekt: granica biegnie arterią, nie po przekątnej kwartału.
fn dognij(
    net: &RoadNetwork,
    blocks: &BlockSet,
    adj: &(Vec<u32>, Vec<u32>),
    owner: &mut [u16],
) -> u32 {
    let n = blocks.blocks.len();
    let (start, items) = adj;
    let bariera = |a: usize, b: usize| -> bool {
        // Wspólny segment o randze ≥ Collector albo kolejowy jest barierą.
        let sa = &blocks.bounding_items
            [blocks.blocks[a].bounding.start as usize..blocks.blocks[a].bounding.end as usize];
        let sb = &blocks.bounding_items
            [blocks.blocks[b].bounding.start as usize..blocks.blocks[b].bounding.end as usize];
        sa.iter().any(|s| {
            sb.contains(s) && {
                let seg = &net.segments[s.0 as usize];
                seg.class.rank() >= RoadClass::Collector.rank()
                    || seg.flags.contains(RoadFlags::RAIL)
            }
        })
    };
    let mut zmian = 0u32;
    for _ in 0..2 {
        let mut zmiany: Vec<(usize, u16)> = Vec::new();
        for i in 0..n {
            let mut licznik: Vec<(u16, u32)> = Vec::new();
            let mut miekkich = 0u32;
            for k in start[i]..start[i + 1] {
                let j = items[k as usize] as usize;
                if bariera(i, j) {
                    continue;
                }
                miekkich += 1;
                match licznik.iter_mut().find(|(d, _)| *d == owner[j]) {
                    Some((_, c)) => *c += 1,
                    None => licznik.push((owner[j], 1)),
                }
            }
            if miekkich < 3 {
                continue;
            }
            licznik.sort_by_key(|(d, c)| (std::cmp::Reverse(*c), *d));
            if let Some(&(d, c)) = licznik.first() {
                if d != owner[i] && c * 5 >= miekkich * 3 {
                    zmiany.push((i, d));
                }
            }
        }
        zmian += zmiany.len() as u32;
        for (i, d) in zmiany {
            owner[i] = d;
        }
    }
    zmian
}

/// Przestawienie kwartałów i tablic z nimi sprzężonych zgodnie z permutacją.
fn przestaw(blocks: &mut BlockSet, zones: &mut ZoneResult, order: &[u32], owner: &mut Vec<u16>) {
    let mut nowe: Vec<Block> = Vec::with_capacity(order.len());
    let mut items: Vec<SegmentId> = Vec::with_capacity(blocks.bounding_items.len());
    let mut zone = Vec::with_capacity(order.len());
    let mut ring = Vec::with_capacity(order.len());
    let mut own = Vec::with_capacity(order.len());
    for (nowy, &stary) in order.iter().enumerate() {
        let b = &blocks.blocks[stary as usize];
        let s = items.len() as u32;
        items.extend_from_slice(
            &blocks.bounding_items[b.bounding.start as usize..b.bounding.end as usize],
        );
        let mut nb = b.clone();
        nb.id = BlockId(nowy as u32);
        nb.bounding = s..items.len() as u32;
        nowe.push(nb);
        zone.push(zones.zone[stary as usize]);
        ring.push(zones.epoch_ring[stary as usize]);
        own.push(owner[stary as usize]);
    }
    blocks.blocks = nowe;
    blocks.bounding_items = items;
    zones.zone = zone;
    zones.epoch_ring = ring;
    *owner = own;
}

/// Tożsamość dzielnicy: rodzaj z dominującej pary (pierścień, strefa), styl, nazwa.
#[allow(clippy::too_many_arguments)]
fn tozsamosc(
    plan: &CityPlan,
    t: &dyn TerrainQuery,
    net: &RoadNetwork,
    geom: &mut PolyArena,
    blocks: &BlockSet,
    zones: &ZoneResult,
    _fields: &CityFields,
    centroid: &[Vec2],
    zakres: Range<u32>,
    names: &DistrictNames,
    uzyte: &mut Vec<String>,
    center: Vec2,
) -> District {
    let idx = zakres.start as usize..zakres.end as usize;
    let mut pole = [0.0f64; 16];
    let mut ring_pole: Vec<f64> = vec![0.0; 32];
    let mut calosc = 0.0f64;
    let mut pop = 0.0f32;
    for i in idx.clone() {
        let a = f64::from(blocks.blocks[i].area_m2);
        calosc += a;
        pole[zones.zone[i].index()] += a;
        ring_pole[usize::from(zones.epoch_ring[i]).min(31)] += a;
        pop += (a as f32 / 10_000.0) * pop_per_ha(zones.zone[i]);
    }
    let dominujaca = (0..16)
        .max_by(|a, b| pole[*a].total_cmp(&pole[*b]).then(b.cmp(a)))
        .unwrap_or(0);
    let ring = ring_pole
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1).then(b.0.cmp(&a.0)))
        .map_or(0u8, |(i, _)| i as u8);
    let srodek = idx
        .clone()
        .fold(Vec2::ZERO, |s, i| s + centroid[i])
        / idx.len().max(1) as f32;

    let mieszkaniowa = (0..5).map(|i| pole[i]).sum::<f64>() / calosc.max(1.0);
    let port_blisko = net
        .gates
        .iter()
        .any(|g| g.kind == GateKind::Port && (g.pos - srodek).length() < 1200.0);
    let z = ZoneKind::ALL[dominujaca];
    let kind = if port_blisko {
        DistrictKind::PortQuarter
    } else if matches!(
        z,
        ZoneKind::IndustryHeavy | ZoneKind::IndustryLight | ZoneKind::Logistics
    ) {
        DistrictKind::IndustrialBelt
    } else if z == ZoneKind::Institutional {
        DistrictKind::Campus
    } else if z == ZoneKind::Agriculture {
        if mieszkaniowa > 0.2 {
            DistrictKind::Village
        } else {
            DistrictKind::GreenBelt
        }
    } else if z == ZoneKind::Green {
        DistrictKind::GreenBelt
    } else if ring == 0 {
        DistrictKind::OldTown
    } else if matches!(
        z,
        ZoneKind::Residential(super::zoning::ResDensity::R4)
            | ZoneKind::Residential(super::zoning::ResDensity::R5)
    ) {
        DistrictKind::BlockEstate
    } else if ring <= 1 || (srodek - center).length() < plan.urban_radius_m() * 0.35 {
        DistrictKind::InnerCity
    } else {
        DistrictKind::Suburb
    };

    let name = nazwa(plan, t, names, kind, srodek, uzyte, zakres.start);
    uzyte.push(name.clone());

    District {
        name,
        kind,
        founded_epoch: EpochId(ring),
        // Styl: para (rodzaj, epoka założenia) — M2d czyta z tego zestaw gramatyk.
        style: StyleId(u16::from(kind as u8) * 32 + u16::from(ring)),
        boundary: obrys(net, blocks, geom, zakres.clone()),
        blocks: zakres,
        centroid: srodek,
        reputation: Q::new(50),
        crime: Q::new(30),
        income_tier: 2,
        avg_land_value: Money(0),
        pop_capacity: pop as u32,
    }
}

/// Nazwa dzielnicy: toponim od cechy terenu, inaczej pula rodzaju, inaczej
/// „{przymiotnik} {rdzeń}". Unikalność wymuszana sufiksem kierunkowym, a w ostateczności
/// numerem — nazwa powtórzona psuje immersję natychmiast (ryzyko R10).
fn nazwa(
    plan: &CityPlan,
    t: &dyn TerrainQuery,
    names: &DistrictNames,
    kind: DistrictKind,
    p: Vec2,
    uzyte: &[String],
    seed_idx: u32,
) -> String {
    let mut r = rng(plan.seed, StreamId::Naming, seed_idx, Tick(0));
    let (x, y) = (p.x as i32, p.y as i32);
    let cecha = if t.water_depth_at(x, y) > 0 || blisko_wody(t, p) {
        match t.navigable(x, y) {
            Some(crate::query::NavigableClass::River { .. }) => Some(Toponym::River),
            Some(_) => Some(Toponym::Lake),
            None => Some(Toponym::River),
        }
    } else if t.slope_at(x, y) > 12 {
        Some(Toponym::Hill)
    } else if t.biome_at(x, y).is_forest() {
        Some(Toponym::Forest)
    } else {
        None
    };

    let baza = cecha
        .and_then(|c| names.toponyms.iter().find(|(k, _)| *k == c))
        .and_then(|(_, v)| DistrictNames::pick(v, &mut r))
        .or_else(|| {
            names
                .by_kind
                .iter()
                .find(|(k, _)| *k == kind)
                .and_then(|(_, v)| DistrictNames::pick(v, &mut r))
        })
        .unwrap_or_else(|| {
            format!(
                "{} {}",
                DistrictNames::pick(&names.adjectives, &mut r).unwrap_or_default(),
                DistrictNames::pick(&names.cores, &mut r).unwrap_or_default()
            )
        });

    if !uzyte.contains(&baza) {
        return baza;
    }
    for d in &names.directions {
        let k = format!("{baza} {d}");
        if !uzyte.contains(&k) {
            return k;
        }
    }
    let mut i = 2;
    loop {
        let k = format!("{baza} {i}");
        if !uzyte.contains(&k) {
            return k;
        }
        i += 1;
    }
}

fn blisko_wody(t: &dyn TerrainQuery, p: Vec2) -> bool {
    for (dx, dy) in [(-120, 0), (120, 0), (0, -120), (0, 120)] {
        if t.water_depth_at(p.x as i32 + dx, p.y as i32 + dy) > 0 {
            return true;
        }
    }
    false
}

/// Obrys dzielnicy: łańcuch segmentów granicznych (tych, po których drugiej stronie
/// leży inna dzielnica albo nic). Gdy łańcuch się nie domyka — otoczka wypukła
/// centroidów, bo pusty obrys byłby gorszy od przybliżonego.
fn obrys(net: &RoadNetwork, blocks: &BlockSet, geom: &mut PolyArena, zakres: Range<u32>) -> PolyRef {
    let mut punkty: Vec<Vec2> = Vec::new();
    for i in zakres.start as usize..zakres.end as usize {
        let b = &blocks.blocks[i];
        for &s in &blocks.bounding_items[b.bounding.start as usize..b.bounding.end as usize] {
            let seg = &net.segments[s.0 as usize];
            punkty.push(net.nodes[seg.a.0 as usize].pos);
            punkty.push(net.nodes[seg.b.0 as usize].pos);
        }
    }
    let hull = poly::convex_hull(&punkty);
    geom.push(if hull.len() >= 3 { &hull } else { &punkty })
}

/// Dzielnica segmentu: bierze ją od przylegającego kwartału o najniższym `BlockId`.
/// Wołane **po** dołożeniu torów i ulic lokalnych, żeby i one dostały dzielnicę.
/// Segment na granicy dwóch dzielnic należy do tej pierwszej — arbitralnie, ale
/// deterministycznie, a alternatywą byłoby pole `Option` i gałąź u każdego konsumenta.
pub fn assign_road_districts(net: &mut RoadNetwork, blocks: &BlockSet) {
    for b in &blocks.blocks {
        for &s in &blocks.bounding_items[b.bounding.start as usize..b.bounding.end as usize] {
            let seg = &mut net.segments[s.0 as usize];
            if seg.district == super::road::UNASSIGNED_DISTRICT {
                seg.district = b.district;
            }
        }
    }
    // Ulice lokalne i tory: po sąsiedzie z przypisaną dzielnicą, wzdłuż wspólnego węzła.
    // Do punktu stałego, nie dwa przebiegi: bocznica kolejowa bywa łańcuchem kilkunastu
    // segmentów, z których dopiero ostatni dotyka kwartału (zmierzone: 44 segmenty
    // bez dzielnicy po dwóch przebiegach).
    for _ in 0..64 {
        let przed = net
            .segments
            .iter()
            .filter(|s| s.district == super::road::UNASSIGNED_DISTRICT)
            .count();
        if przed == 0 {
            break;
        }
        let przypisane: Vec<DistrictId> = net.segments.iter().map(|s| s.district).collect();
        for i in 0..net.segments.len() {
            if przypisane[i] != super::road::UNASSIGNED_DISTRICT {
                continue;
            }
            let (a, b) = (net.segments[i].a, net.segments[i].b);
            let d = [a, b]
                .iter()
                .flat_map(|n| net.segments_at(*n))
                .map(|s| przypisane[s.0 as usize])
                .find(|d| *d != super::road::UNASSIGNED_DISTRICT);
            if let Some(d) = d {
                net.segments[i].district = d;
            }
        }
        let po = net
            .segments
            .iter()
            .filter(|s| s.district == super::road::UNASSIGNED_DISTRICT)
            .count();
        if po == przed {
            break; // segment odcięty od wszystkiego — dalsze przebiegi nic nie dadzą
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plik_nazw_ma_pule_dla_kazdego_rodzaju() {
        let n = DistrictNames::load().expect("data/names/districts_pl.ron");
        for k in [
            DistrictKind::OldTown,
            DistrictKind::InnerCity,
            DistrictKind::BlockEstate,
            DistrictKind::Suburb,
            DistrictKind::IndustrialBelt,
            DistrictKind::PortQuarter,
            DistrictKind::Village,
            DistrictKind::Campus,
            DistrictKind::GreenBelt,
        ] {
            let p = n.by_kind.iter().find(|(x, _)| *x == k);
            assert!(p.is_some_and(|(_, v)| !v.is_empty()), "brak nazw dla {k:?}");
        }
        // Pula musi wystarczyć na 40 dzielnic bez zjeżdżania na numerację.
        let suma: usize = n.by_kind.iter().map(|(_, v)| v.len()).sum::<usize>()
            + n.toponyms.iter().map(|(_, v)| v.len()).sum::<usize>();
        assert!(suma >= 40, "pula nazw ma {suma} pozycji");
    }
}
