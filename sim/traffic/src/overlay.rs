//! Nakładki danych warstwy ruchu (M4d §5.4 i PRD §14.2, WP11).
//!
//! Pięć nakładek z §14.2, które produkuje ta faza: natężenie, korki, izochrony czasu
//! dojazdu, obłożenie parkingów i obciążenie linii komunikacji.
//!
//! ## Dlaczego podwójny bufor, a nie odczyt zasobu wprost
//!
//! Renderer rysuje w swoim rytmie, a symulacja liczy w swoim. Gdyby klient czytał
//! `TrafficNetwork` bezpośrednio, każda klatka zakładałaby zamek na zasobie, który
//! w tej samej chwili przepisuje krok minutowy — i albo renderer czekałby na symulację,
//! albo odwrotnie. Dlatego: **pisarz trzyma bufor tylny, czytelnik przedni**, a zamiana
//! to podmiana jednego indeksu. Oba bufory mają własny zamek, więc te dwie strony
//! nigdy nie stoją o to samo.
//!
//! Nakładka **nie jest stanem symulacji** i nie wchodzi do hasha (00 §3.6) — to pomiar,
//! tak samo jak `TrafficStats`. Gdyby weszła, przełączenie nakładki przez gracza
//! zmieniałoby hash świata, czyli dokładnie to, przed czym broni §5.4.
//!
//! `ponytail:` izochrona ma ziarnistość **dzielnicy**, nie węzła. Sufit nazwany:
//! `TravelTimeMatrix` jest tabelą dzielnica × godzina i to jest jej cała rozdzielczość,
//! a jednorazowa Dijkstra jeden-do-wszystkich dałaby obraz ładniejszy i nieporównywalny
//! z tym, czym liczy rynek pracy (M7) i zasięg sklepu (M5). Ścieżka wyjścia: gdy ktoś
//! zechce izochrony z **wybranego punktu**, a nie z dzielnicy, dokłada zapytanie
//! jeden-do-wszystkich w `engine/nav` i drugie pole tutaj.

use crate::mezo::MezoState;
use crate::parking::ParkingRegistry;
use crate::transit::TransitNetwork;
use magnat_core::WorldCoord;
use magnat_nav::RoadGraph;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// Którą wielkość rysujemy. Klucze zgadzają się z wpisami w `data/ui/overlays.ron`
/// (`K-19`) — to one niosą paletę, progi i jednostkę, bo nakładka jest daną, nie kodem.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrafficField {
    /// Pojazdy na godzinę wjeżdżające na krawędź.
    Flow,
    /// Prędkość jako promile prędkości swobodnej. **Mniej znaczy gorzej.**
    Congestion,
    /// Minuty dojazdu do dzielnicy odniesienia.
    Isochrone,
    /// Zapełnienie parkingu w promilach.
    Parking,
    /// Obłożenie kursu w promilach pojemności.
    TransitLoad,
}

impl TrafficField {
    pub const ALL: [TrafficField; 5] = [
        TrafficField::Flow,
        TrafficField::Congestion,
        TrafficField::Isochrone,
        TrafficField::Parking,
        TrafficField::TransitLoad,
    ];

    /// Klucz wpisu w `data/ui/overlays.ron`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            TrafficField::Flow => "traffic_flow",
            TrafficField::Congestion => "congestion",
            TrafficField::Isochrone => "isochrone",
            TrafficField::Parking => "parking_occupancy",
            TrafficField::TransitLoad => "transit_load",
        }
    }

    /// Czy nakładka rysuje się po krawędziach sieci (`true`), czy po punktach (`false`).
    #[must_use]
    pub const fn is_edge_field(self) -> bool {
        !matches!(self, TrafficField::Parking)
    }
}

/// Zrzut nakładek na jedną minutę świata. Cztery pola na krawędź plus lista punktów
/// parkingowych — dokładnie to, co budżet §7.3 wycenił na 0,8 ms przy 80 tys. krawędzi.
#[derive(Clone, Default, Debug)]
pub struct TrafficOverlaySnapshot {
    /// Minuta świata, z której pochodzi zrzut. Renderer może pokazać, że nakładka
    /// jest o klatkę stara — i to jest uczciwsze niż udawanie, że nie jest.
    pub minute: u32,
    /// Pojazdy na godzinę, per krawędź.
    pub flow_vph: Vec<u16>,
    /// Promile prędkości swobodnej, per krawędź. 1000 = jedzie się swobodnie.
    pub speed_permille: Vec<u16>,
    /// Zapełnienie kolejki krawędzi w promilach pojemności postojowej.
    pub queue_permille: Vec<u16>,
    /// Minuty dojazdu z dzielnicy odniesienia do dzielnicy tej krawędzi.
    pub isochrone_min: Vec<u16>,
    /// Obłożenie kursów przypisane krawędziom linii, w promilach pojemności.
    pub transit_permille: Vec<u16>,
    /// Parkingi: `(pozycja, zapełnienie w promilach)`.
    pub lots: Vec<(WorldCoord, u16)>,
}

impl TrafficOverlaySnapshot {
    /// Wartość pola dla krawędzi — w jednostce, w której `data/ui/overlays.ron`
    /// podaje swoje progi.
    #[must_use]
    pub fn edge_value(&self, field: TrafficField, edge: usize) -> i64 {
        let get = |v: &Vec<u16>| i64::from(v.get(edge).copied().unwrap_or(0));
        match field {
            TrafficField::Flow => get(&self.flow_vph),
            TrafficField::Congestion => get(&self.speed_permille),
            TrafficField::Isochrone => get(&self.isochrone_min),
            TrafficField::TransitLoad => get(&self.transit_permille),
            // Parking nie jest polem krawędziowym — rysuje się z `lots`.
            TrafficField::Parking => get(&self.queue_permille),
        }
    }
}

/// Podwójnie buforowany zrzut nakładek. Zasób świata, **poza hashem stanu**.
#[derive(Debug)]
pub struct TrafficOverlay {
    bufs: [Mutex<TrafficOverlaySnapshot>; 2],
    front: AtomicUsize,
    /// Dzielnica odniesienia izochrony — punkt, „z którego" gracz mierzy dojazd.
    /// Wybór kamery, nie stan świata, więc atomik i brak miejsca w hashu.
    origin: AtomicUsize,
}

impl Default for TrafficOverlay {
    fn default() -> TrafficOverlay {
        TrafficOverlay {
            bufs: [
                Mutex::new(TrafficOverlaySnapshot::default()),
                Mutex::new(TrafficOverlaySnapshot::default()),
            ],
            front: AtomicUsize::new(0),
            origin: AtomicUsize::new(0),
        }
    }
}

/// Wszystko, z czego buduje się zrzut nakładek. Struktura zamiast ośmiu argumentów,
/// bo przy ośmiu pierwsza pomyłka w kolejności `parking`/`transit` skompilowałaby się
/// bez słowa — oba są referencjami do czegoś innego, ale wołający ich nie rozróżnia.
pub struct OverlayInputs<'a> {
    pub minute: u32,
    pub road: &'a RoadGraph,
    pub mezo: &'a MezoState,
    pub parking: &'a ParkingRegistry,
    pub transit: &'a TransitNetwork,
    /// Dzielnica, z której mierzy się izochronę.
    pub origin_district: u16,
}

impl TrafficOverlay {
    #[must_use]
    pub fn new() -> TrafficOverlay {
        TrafficOverlay::default()
    }

    /// Przebudowuje bufor tylny i podmienia go z przednim. Woła krok minutowy ruchu.
    pub fn rebuild(&self, i: &OverlayInputs<'_>, travel_min: impl Fn(u16) -> u16) {
        let OverlayInputs {
            minute,
            road,
            mezo,
            parking,
            transit,
            origin_district,
        } = *i;
        let tyl = 1 - self.front.load(Ordering::Acquire);
        {
            let mut s = self.bufs[tyl].lock().expect("overlay");
            let n = road.edge_count();
            s.minute = minute;
            s.flow_vph.clear();
            s.speed_permille.clear();
            s.queue_permille.clear();
            s.isochrone_min.clear();
            s.transit_permille.clear();
            s.lots.clear();
            s.flow_vph.reserve(n);
            s.speed_permille.reserve(n);
            s.queue_permille.reserve(n);
            s.isochrone_min.reserve(n);
            s.transit_permille.resize(n, 0);

            for i in 0..n {
                let link = mezo.links[i];
                let q = mezo.queues[i];
                s.flow_vph.push(link.inflow_last_min.saturating_mul(60));
                let swobodna = link.free_flow_dkmh.max(1);
                s.speed_permille
                    .push((u32::from(link.mean_speed_dkmh) * 1000 / u32::from(swobodna)).min(1000)
                        as u16);
                let cap = u32::from(q.storage_capacity.max(1));
                s.queue_permille
                    .push((u32::from(q.occupancy) * 1000 / cap).min(1000) as u16);
                let d = road.edge(magnat_nav::EdgeId(i as u32)).district;
                s.isochrone_min.push(if d.0 == origin_district {
                    0
                } else {
                    travel_min(d.0)
                });
            }

            // Obciążenie linii: kurs obciąża **te krawędzie, po których właśnie jedzie**.
            // Maksimum, nie średnia — gracz szuka odcinka, na którym autobus pęka w szwach,
            // a średnia po całej linii ten odcinek rozcieńcza.
            for run in transit.runs() {
                if run.stop_index == 0 {
                    continue;
                }
                let Some(line) = transit.lines().iter().find(|l| l.id == run.line) else {
                    continue;
                };
                let Some(hop) = line.hops.get(usize::from(run.stop_index) - 1) else {
                    continue;
                };
                let obciazenie =
                    (u32::from(run.occupancy) * 1000 / u32::from(run.capacity.max(1))).min(1000)
                        as u16;
                for e in hop {
                    let slot = &mut s.transit_permille[e.0 as usize];
                    *slot = (*slot).max(obciazenie);
                }
            }

            s.lots.reserve(parking.lots().len());
            for lot in parking.lots() {
                s.lots.push((lot.at, lot.occupancy_permille()));
            }
        }
        self.front.store(tyl, Ordering::Release);
    }

    /// Czyta bufor przedni. Nie blokuje pisarza: ten trzyma drugi zamek.
    pub fn with_front<R>(&self, f: impl FnOnce(&TrafficOverlaySnapshot) -> R) -> R {
        let i = self.front.load(Ordering::Acquire);
        let s = self.bufs[i].lock().expect("overlay");
        f(&s)
    }

    /// Dzielnica, z której liczy się izochronę.
    #[must_use]
    pub fn origin(&self) -> u16 {
        self.origin.load(Ordering::Relaxed) as u16
    }

    pub fn set_origin(&self, district: u16) {
        self.origin.store(usize::from(district), Ordering::Relaxed);
    }

    /// Minuta, z której pochodzi bufor przedni.
    #[must_use]
    pub fn minute(&self) -> u32 {
        self.with_front(|s| s.minute)
    }
}

/// Rastruje pole krawędziowe na siatkę komórek — wejście `TerrainOverlay` z renderera.
///
/// Komórka dostaje **maksimum** z krawędzi, które przez nią przechodzą: nakładka ma
/// pokazać, gdzie stoi korek, a uśrednienie z pustą ulicą obok właśnie ten korek gasi.
/// Krawędź jest odcinkiem prostym (graf M4a nie ma dziś geometrii pośredniej), więc
/// stempluje się ją algorytmem kroczącym po dłuższej osi.
#[must_use]
pub fn rasterize_edges(
    road: &RoadGraph,
    values: &[u16],
    dim: u32,
    cell_m: u32,
    width_cells: u32,
) -> Vec<u16> {
    let mut out = vec![0u16; (dim as usize) * (dim as usize)];
    let cell_cm = (cell_m * 100).max(1) as i64;
    let promien = width_cells as i64;
    for (i, v) in values.iter().enumerate() {
        if *v == 0 {
            continue;
        }
        let e = road.edge(magnat_nav::EdgeId(i as u32));
        let a = road.nodes[e.from.0 as usize].pos_cm;
        let b = road.nodes[e.to.0 as usize].pos_cm;
        let (x0, y0) = (i64::from(a.x) / cell_cm, i64::from(a.y) / cell_cm);
        let (x1, y1) = (i64::from(b.x) / cell_cm, i64::from(b.y) / cell_cm);
        let krokow = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
        for k in 0..=krokow {
            let cx = x0 + (x1 - x0) * k / krokow;
            let cy = y0 + (y1 - y0) * k / krokow;
            for dy in -promien..=promien {
                for dx in -promien..=promien {
                    let (px, py) = (cx + dx, cy + dy);
                    if px < 0 || py < 0 || px >= i64::from(dim) || py >= i64::from(dim) {
                        continue;
                    }
                    let idx = py as usize * dim as usize + px as usize;
                    out[idx] = out[idx].max(*v);
                }
            }
        }
    }
    out
}

/// Rastruje punkty (parkingi) na siatkę — ta sama jednostka wyjściowa co wyżej.
#[must_use]
pub fn rasterize_points(
    lots: &[(WorldCoord, u16)],
    dim: u32,
    cell_m: u32,
    radius_cells: u32,
) -> Vec<u16> {
    let mut out = vec![0u16; (dim as usize) * (dim as usize)];
    let cell_cm = (cell_m * 100).max(1) as i64;
    let r = radius_cells as i64;
    for (at, v) in lots {
        let cx = i64::from(at.x) / cell_cm;
        let cy = i64::from(at.y) / cell_cm;
        for dy in -r..=r {
            for dx in -r..=r {
                let (px, py) = (cx + dx, cy + dy);
                if px < 0 || py < 0 || px >= i64::from(dim) || py >= i64::from(dim) {
                    continue;
                }
                let idx = py as usize * dim as usize + px as usize;
                out[idx] = out[idx].max(*v);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn podmiana_bufora_nie_gubi_poprzedniej_minuty() {
        let o = TrafficOverlay::new();
        assert_eq!(o.minute(), 0);
        let road = magnat_nav::synthetic_grid(4, 4, 10_000);
        let vdf = crate::spec::VdfTable::load_default().expect("data/roads/vdf.ron");
        let mezo = MezoState::new(&road, &vdf);
        let parking = ParkingRegistry::build(&road, Vec::new(), 0);
        let transit = TransitNetwork::new(Vec::new(), &road);

        let we = |m| OverlayInputs {
            minute: m,
            road: &road,
            mezo: &mezo,
            parking: &parking,
            transit: &transit,
            origin_district: 0,
        };
        o.rebuild(&we(7), |_| 12);
        assert_eq!(o.minute(), 7);
        o.with_front(|s| {
            assert_eq!(s.flow_vph.len(), road.edge_count());
            assert_eq!(s.speed_permille.len(), road.edge_count());
            // Pusta sieć jedzie swobodnie — promile prędkości mają być pełne.
            assert!(s.speed_permille.iter().all(|v| *v == 1000));
        });

        o.rebuild(&we(8), |_| 12);
        assert_eq!(o.minute(), 8, "podmiana bufora nie doszła do skutku");
    }

    #[test]
    fn raster_stempluje_krawedzie_i_nie_wychodzi_poza_siatke() {
        let road = magnat_nav::synthetic_grid(4, 4, 10_000);
        let mut values = vec![0u16; road.edge_count()];
        values[0] = 500;
        let r = rasterize_edges(&road, &values, 32, 16, 1);
        assert_eq!(r.len(), 32 * 32);
        assert!(r.contains(&500), "krawędź nie trafiła na raster");
        assert!(
            r.iter().filter(|v| **v > 0).count() < 32 * 32,
            "jedna krawędź zamalowała całą siatkę"
        );
    }

    #[test]
    fn raster_punktowy_ma_promien_a_nie_pojedynczy_piksel() {
        let lots = [(WorldCoord::new(1_600, 1_600, 0), 800)];
        let r = rasterize_points(&lots, 16, 16, 1);
        assert_eq!(r.iter().filter(|v| **v == 800).count(), 9, "3×3 wokół punktu");
    }
}
