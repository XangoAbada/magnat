//! Raport generacji miasta i jego wydruk tekstowy (M2 §1, artefakt 1).
//!
//! Wydzielone z `city/mod.rs` w R-WP9 bez zmiany zachowania.

use super::*;

/// Raport generacji miasta (M2 §1, artefakt 1). M2b wypełnia część transportową;
/// kolejne podfazy dokładają swoje sekcje do tej samej struktury.
#[derive(Clone, PartialEq, Debug)]
pub struct GenerationReport {
    pub center: Vec2,
    pub gates: Vec<(GateKind, Vec2)>,
    pub missing_gates: Vec<GateKind>,
    /// Bramy postawione, ale bez połączenia z siecią w tej podfazie: kolejowe czekają
    /// na tory z M2c. Wypisywane, bo brama, o której raport milczy, znika bez śladu.
    pub deferred_gates: Vec<(GateKind, Vec2)>,
    pub nodes: u32,
    pub segments: u32,
    pub road_km: f64,
    pub bridges: u32,
    pub tunnels: u32,
    pub embankments: u32,
    pub blocks: u32,
    pub dropped_faces: u32,
    pub swallowed_faces: u32,
    pub faces_debug: String,
    pub urban_area_km2: f64,
    pub road_area_km2: f64,
    pub stats: lsystem::LStats,
    // ── M2c ──────────────────────────────────────────────────────────────────────────
    /// Zrealizowany i docelowy udział powierzchniowy stref, w indeksach `ZoneKind::ALL`.
    pub zone_share: [f32; 16],
    pub zone_target: [f32; 16],
    /// Największe odchylenie udziału od kwoty, w punktach procentowych (test T9: ≤ 3).
    pub zone_dev_pp: f32,
    /// Kwartały pozamiejskie — poza kwotami (patrz `zoning::MAX_URBAN_BLOCK_M2`).
    pub rural_blocks: u32,
    pub rural_area_km2: f64,
    /// Dolna granica `zone_dev_pp` wynikająca z ziarnistości kwartałów.
    pub zone_dev_floor_pp: f32,
    pub zone_count: [u32; 16],
    pub zone_candidates: [u32; 16],
    /// Liczba kwartałów w każdym pierścieniu epoki, w kolejności od najstarszego.
    pub epoch_rings: Vec<(String, u32)>,
    pub districts: u32,
    pub district_names: Vec<String>,
    pub district_seeds: u32,
    pub district_snapped: u32,
    pub parcels: u32,
    pub parcels_without_frontage: u32,
    pub parcel_slivers: u32,
    pub local_streets: u32,
    pub segment_splits: u32,
    pub rail: rail::RailReport,
    // ── M2d ──────────────────────────────────────────────────────────────────────────
    /// Etap 6: budynki, lokale, stanowiska, gramatyka awaryjna.
    pub build: build::BuildReport,
    /// Warstwa transportowa w voxelach (§5.6b).
    pub road_voxels: voxels::RoadVoxelReport,
    /// Wartość gruntu po `pass_1` (WP15a): mediana i skrajne dzielnice.
    pub land_value_median: magnat_core::Money,
    /// Średnia w dzielnicach rdzenia (starówka + śródmieście) i obrzeża (przedmieście
    /// + wieś) — para z kryterium WP15a.
    pub land_value_core: magnat_core::Money,
    pub land_value_fringe: magnat_core::Money,
    // ── M2e ──────────────────────────────────────────────────────────────────────────
    /// Etap 7: firmy, zakłady, normatywy.
    pub sites: sites::SiteReport,
    /// Domknięcie łańcuchów produktowych (WP14).
    pub closure: sites::ClosureReport,
    /// Wartość gruntu po `pass_2` — te same trzy liczby co dla `pass_1`, żeby dało się
    /// zobaczyć, co zmienił Etap 7.
    pub land_value_median_2: magnat_core::Money,
    pub land_value_core_2: magnat_core::Money,
    pub land_value_fringe_2: magnat_core::Money,
    /// Odcisk całej warstwy M2 — `city_hash` plus Etap 7 (D1/D5 z §7 fazy).
    pub world_hash_m2: StateHash,
    /// Zastosowane współczynniki kalibracji T10: zagęszczenie mieszkań i przelicznik
    /// stanowisk. 1,0 znaczy „nie było czego kalibrować"; wartość na granicy widełek
    /// znaczy, że teren nie pozwolił dojść do `target_pop` i T10 może nie przejść.
    pub dwelling_scale: f32,
    pub stage_millis: Vec<(&'static str, f64)>,
    pub road_hash: StateHash,
    /// Odcisk warstwy M2c: strefy, dzielnice, parcele. Wchodzi do `world_hash_m2` (M2e).
    pub city_hash: StateHash,
    /// Ostrzeżenia — R1 fazy: generator, który odrzuca ponad 40% propozycji,
    /// buduje co innego, niż planowano, i ma o tym powiedzieć.
    pub warnings: Vec<String>,
}

impl GenerationReport {
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        let mut v = vec![
            format!("centrum: {:.0}, {:.0}", self.center.x, self.center.y),
            format!(
                "bramy: {}",
                self.gates
                    .iter()
                    .map(|(k, p)| format!("{}@{:.0},{:.0}", k.key(), p.x, p.y))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            format!(
                "sieć: {} węzłów, {} segmentów, {:.1} km",
                self.nodes, self.segments, self.road_km
            ),
            format!(
                "struktury: {} mostów, {} tuneli, {} nasypów",
                self.bridges, self.tunnels, self.embankments
            ),
            format!(
                "kwartały: {} · obszar {:.2} km² · pas drogowy {:.2} km²",
                self.blocks, self.urban_area_km2, self.road_area_km2
            ),
            format!(
                "propozycje: {} · przyjęte {} · odrzucone {}% (nachylenie {}, przeprawa {}, kąt {}, strefa {})",
                self.stats.proposals,
                self.stats.accepted,
                self.stats.rejection_pct(),
                self.stats.rejected_slope,
                self.stats.rejected_crossing,
                self.stats.rejected_angle,
                self.stats.rejected_zone,
            ),
            format!(
                "scalenia {} · podziały {} · przycięte {} · kwartały odrzucone {} / pochłonięte {}",
                self.stats.snapped, self.stats.split, self.stats.pruned,
                self.dropped_faces, self.swallowed_faces,
            ),
            self.faces_debug.clone(),
            format!("hash sieci: {:032x}", self.road_hash.0),
        ];
        if self.parcels > 0 || self.districts > 0 {
            self.lines_m2(&mut v);
        }
        for (n, ms) in &self.stage_millis {
            v.push(format!("  {n}: {ms:.1} ms"));
        }
        if !self.deferred_gates.is_empty() {
            v.push(format!(
                "bramy odłożone do M2c (tory): {}",
                self.deferred_gates
                    .iter()
                    .map(|(k, p)| format!("{}@{:.0},{:.0}", k.key(), p.x, p.y))
                    .collect::<Vec<_>>()
                    .join(" ")
            ));
        }
        if !self.missing_gates.is_empty() {
            v.push(format!(
                "BRAK BRAM WYMAGANYCH: {}",
                self.missing_gates
                    .iter()
                    .map(|k| k.key())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        for w in &self.warnings {
            v.push(format!("OSTRZEŻENIE: {w}"));
        }
        v
    }

    /// Sekcje M2c, M2d i M2e wydruku — wszystko, co powstaje dopiero wtedy,
    /// gdy są parcele albo dzielnice. Wydzielone z `lines` w R-WP9: kolejność
    /// linii jest kontraktem z czytelnikiem i nie zmienia się ani o jedną.
    fn lines_m2(&self, v: &mut Vec<String>) {
        v.push(format!(
            "strefy: odchylenie max {:.1} pp (próg ziarnistości {:.1}) · pozamiejskich {} ({:.2} km²) · {}",
            self.zone_dev_pp,
            self.zone_dev_floor_pp,
            self.rural_blocks,
            self.rural_area_km2,
            ZoneKind::ALL
                .iter()
                .filter(|z| {
                self.zone_share[z.index()] > 0.0005
                    || self.zone_target[z.index()] > 0.0005
                    || self.zone_count[z.index()] > 0
            })
                .map(|z| format!(
                    "{} {}k{}×{:.1}/{:.1}%",
                    z.key(),
                    self.zone_candidates[z.index()],
                    self.zone_count[z.index()],
                    self.zone_share[z.index()] * 100.0,
                    self.zone_target[z.index()] * 100.0
                ))
                .collect::<Vec<_>>()
                .join(" ")
        ));
        v.push(format!(
            "pierścienie epok: {}",
            self.epoch_rings
                .iter()
                .map(|(k, n)| format!("{k} {n}"))
                .collect::<Vec<_>>()
                .join(" · ")
        ));
        v.push(format!(
            "dzielnice: {} (zalążków {}, dogięć {}) — {}",
            self.districts,
            self.district_seeds,
            self.district_snapped,
            self.district_names.join(", ")
        ));
        v.push(format!(
            "parcele: {} · bez frontu {} · odpad {} · ulice lokalne {} · podziały segmentów {}",
            self.parcels,
            self.parcels_without_frontage,
            self.parcel_slivers,
            self.local_streets,
            self.segment_splits
        ));
        v.push(format!(
            "kolej: {:.1} km · bocznic {} · rozjazdów {} · w zasięgu istniejących {} · bez połączenia {} · max nachylenie {:.2}%",
            self.rail.track_km,
            self.rail.sidings,
            self.rail.junctions,
            self.rail.covered,
            self.rail.unreachable,
            self.rail.max_grade_pct
        ));
        v.push(format!(
            "zabudowa: {} budynków · {} lokali ({} mieszkań) · {} stanowisk · fallback {} ({:.1}%) · rozluźnień {} · za małe {} · puste z zamiaru {} · bez rampy {}",
            self.build.buildings,
            self.build.units,
            self.build.dwellings,
            self.build.workplaces,
            self.build.fallback,
            if self.build.buildings > 0 {
                f64::from(self.build.fallback) * 100.0 / f64::from(self.build.buildings)
            } else {
                0.0
            },
            self.build.relaxed,
            self.build.too_small,
            self.build.left_vacant,
            self.build.ramp_missing
        ));
        if self.build.relaxed > 0 {
            v.push(format!(
                "  rozluźnienia: epoka {} · styl {} · wartość gruntu {} — pierwsze dwa łata się plikiem w data/grammar/, trzecie liczbą w istniejącym",
                self.build.relaxed_epoch, self.build.relaxed_style, self.build.relaxed_value
            ));
        }
        v.push(format!(
            "  detal bryły: {} wysunięć ({} przyciętych do działki, {} odrzuconych jako płytsze niż voxel)",
            self.build.protrusions, self.build.protrusions_clipped, self.build.protrusions_dropped
        ));
        v.push(format!(
            "  różnorodność: powtórki sygnatury w promieniu 60 m {:.1}% ({} z {} par) · najgorsze {:.1}% w {},{} · entropia gramatyk: miasto {:.2} bita, dzielnica {} min {:.2} ({} mierzonych, dominuje gramatyka #{} z {} na {} budynków)",
            if self.build.signature_pairs > 0 {
                f64::from(self.build.signature_repeats) * 100.0 / f64::from(self.build.signature_pairs)
            } else {
                0.0
            },
            self.build.signature_repeats,
            self.build.signature_pairs,
            f64::from(self.build.worst_neighbourhood_permille) / 10.0,
            self.build.worst_neighbourhood_at.0,
            self.build.worst_neighbourhood_at.1,
            f64::from(self.build.city_entropy_mbits) / 1000.0,
            self.build.min_district_entropy_at,
            f64::from(self.build.min_district_entropy_mbits) / 1000.0,
            self.build.districts_measured,
            self.build.min_district_top.0,
            self.build.min_district_top.1,
            self.build.min_district_top.2
        ));
        if !self.build.grammar_hist.is_empty() {
            let mut h: Vec<(usize, u32)> = self
                .build
                .grammar_hist
                .iter()
                .copied()
                .enumerate()
                .filter(|(_, n)| *n > 0)
                .collect();
            h.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            v.push(format!(
                "  gramatyki wg liczby budynków: {}",
                h.iter()
                    .map(|(i, n)| format!("#{i} {n}"))
                    .collect::<Vec<_>>()
                    .join(" · ")
            ));
        }
        if self.build.fallback > 0 {
            v.push(format!(
                "  awaryjne wg stref: {}",
                ZoneKind::ALL
                    .iter()
                    .filter(|z| self.build.fallback_zone[z.index()] > 0)
                    .map(|z| format!("{} {}", z.key(), self.build.fallback_zone[z.index()]))
                    .collect::<Vec<_>>()
                    .join(" · ")
            ));
        }
        v.push(format!(
            "voxele: {} komend zabudowy + {} komend drogowych na {} segmentach ({} schodkowanych, {} mostów, {} tuneli)",
            self.build.edit_commands,
            self.road_voxels.commands,
            self.road_voxels.segments,
            self.road_voxels.stepped,
            self.road_voxels.bridges,
            self.road_voxels.tunnels
        ));
        v.push(format!(
            "wartość gruntu (pass_1): mediana {:.2} zł/m² · rdzeń {:.2} · obrzeże {:.2}",
            self.land_value_median.0 as f64 / 100.0,
            self.land_value_core.0 as f64 / 100.0,
            self.land_value_fringe.0 as f64 / 100.0
        ));
        v.push(format!(
            "wartość gruntu (pass_2): mediana {:.2} zł/m² · rdzeń {:.2} · obrzeże {:.2}",
            self.land_value_median_2.0 as f64 / 100.0,
            self.land_value_core_2.0 as f64 / 100.0,
            self.land_value_fringe_2.0 as f64 / 100.0
        ));
        v.push(format!(
            "etap 7: {} firm · {} zakładów · normatywy {}/{} · z łańcuchów {} · budynki dostawione {} (nieudane {}) · bez zakładu {}",
            self.sites.firms,
            self.sites.sites,
            self.sites.norm_placed,
            self.sites.norm_target,
            self.sites.from_chain,
            self.sites.buildings_added,
            self.sites.buildings_failed,
            self.sites.parcels_without_site
        ));
        if !self.sites.unplaced.is_empty() {
            v.push(format!(
                "  bez działki: {}",
                self.sites
                    .unplaced
                    .iter()
                    .map(|(k, n)| format!("{k} ×{n}"))
                    .collect::<Vec<_>>()
                    .join(" · ")
            ));
        }
        v.push(format!(
            "  sektory: {}",
            sites::SectorId::ALL
                .iter()
                .filter(|s| self.sites.by_sector[**s as usize] > 0)
                .map(|s| format!("{} {}", s.key(), self.sites.by_sector[*s as usize]))
                .collect::<Vec<_>>()
                .join(" · ")
        ));
        v.push(format!(
            "domknięcie łańcuchów: brakujące {} · import {} towarów · najgorszy stosunek {} {:.2} · nadwyżki uboczne {}",
            if self.closure.missing.is_empty() {
                "brak".to_string()
            } else {
                self.closure.missing.join(", ")
            },
            self.closure.imported.len(),
            self.closure.worst.0,
            self.closure.worst.1,
            self.closure.byproduct_surplus.len()
        ));
        if !self.closure.imported.is_empty() {
            v.push(format!(
                "  import (t/dobę): {}",
                self.closure
                    .imported
                    .iter()
                    .map(|(k, t)| format!("{k} {}", t / 1000))
                    .collect::<Vec<_>>()
                    .join(" · ")
            ));
        }
        for e in &self.closure.errors {
            v.push(format!("  BŁĄD DOMKNIĘCIA: {e}"));
        }
        v.push(format!("hash miasta: {:032x}", self.city_hash.0));
        v.push(format!("hash M2: {:032x}", self.world_hash_m2.0));
    }
}
