//! Estymator dojścia pieszo — implementacja tymczasowa `TravelOracle` (K-2).
//!
//! **Moduł jest `pub(crate)` i taki zostaje.** Właścicielem grafu nawigacyjnego jest
//! M4 (`engine/nav`); M3 ma tu wyłącznie prywatny estymator za wspólnym interfejsem.
//! Zero publicznych typów grafu, zero funkcji `route`/`path`/`graph` w API crate'a —
//! kryterium akceptacyjne nr 8 fazy sprawdza to testem architektonicznym. Gdy M4
//! dostarczy swoją implementację, ten moduł **znika w całości**; nie ma tu nic
//! do zmigrowania i nic, co trzeba by utrzymywać równolegle.
//!
//! W M3a odległość jest **manhattanowa z korektą 1,25×** — fallback opisany wprost
//! w ryzyku R7 dokumentu fazy. M3b (WP6) zastępuje ją odległością sieciową po
//! centroliniach ulic z M2, nie zmieniając ani sygnatury, ani niczego u wywołujących.
//!
//! Arytmetyka jest całkowitoliczbowa w centymetrach. Nie dlatego, że float byłby
//! niedeterministyczny (00 §K-6 mówi, że nie byłby), tylko dlatego, że czas przybycia
//! jest stanem trwałym i wchodzi do hasha — a liczba całkowita nie ma wariantów
//! zaokrąglenia, o które można się spierać przy porównaniu mikro z mezo.

use crate::des::{EventKind, EventQueue, SimEvent};
use crate::places::{
    CitizenView, PlaceTable, TravelEstimate, TravelOracle, TripHandle, TripRequest, MAX_ON_ROUTE,
};
use crate::ArrayVec;
use magnat_core::{DecisionReason, MinuteOfDay, Money, PlaceRef, TransportMode, WorldCoord};
use std::sync::Arc;

/// Prędkość marszu: 1,35 m/s = 81 m/min = 8100 cm/min (§5.10).
pub(crate) const BASE_SPEED_CM_PER_MIN: i64 = 8_100;

/// Ta sama prędkość w metrach na minutę — do przeliczenia zasięgu osobistego
/// na promień zapytania przestrzennego.
pub const BASE_SPEED_M_PER_MIN: f32 = 81.0;

/// Korekta odległości manhattanowej do sieciowej (R7). M3b zastępuje ją pomiarem
/// po centroliniach ulic i ta stała znika.
const DETOUR_NUM: i64 = 125;
const DETOUR_DEN: i64 = 100;

/// Szerokość korytarza, w którym miejsce liczy się jako „widoczne z trasy" (§5.7).
const ROUTE_CORRIDOR_CM: i64 = 6_000; // 60 m

/// Czas dojścia w minutach, zawsze ≥ 1: podróż zerominutowa wywróciłaby kolejkę
/// zdarzeń (dwa zdarzenia tego samego aktora w tej samej minucie, §5.2).
#[must_use]
pub(crate) fn walk_minutes(from: WorldCoord, to: WorldCoord, speed_pct: u32) -> u16 {
    let dx = i64::from(from.x - to.x).abs();
    let dy = i64::from(from.y - to.y).abs();
    let dystans_cm = (dx + dy) * DETOUR_NUM / DETOUR_DEN;
    let predkosc = (BASE_SPEED_CM_PER_MIN * i64::from(speed_pct) / 100).max(1);
    ((dystans_cm + predkosc - 1) / predkosc).clamp(1, i64::from(u16::MAX)) as u16
}

/// Prędkość marszu mieszkańca jako procent bazowej (§5.10: wiek, zdrowie, energia).
///
/// Progi, nie krzywa: różnica między 96 % a 97 % prędkości nie jest widoczna ani
/// w histogramie dojazdów, ani w karcie inspekcji, a krzywa wymagałaby kalibracji,
/// której nie ma na czym oprzeć przed M3d.
#[must_use]
pub(crate) fn speed_pct(who: &CitizenView<'_>) -> u32 {
    let lata = who.identity.age_years(who.today);
    let mut pct: i32 = match lata {
        ..=5 => 60,
        6..=13 => 85,
        14..=64 => 100,
        65..=79 => 85,
        _ => 65,
    };
    if who.vitals.health < 40 {
        pct -= 15;
    }
    if who.vitals.energy < 30 {
        pct -= 10;
    }
    pct.clamp(45, 110) as u32
}

/// Estymator dojścia. Trzyma katalog miejsc i licznik podróży — nic więcej,
/// bo nic więcej nie jest mu potrzebne do policzenia czasu i zaplanowania `Arrive`.
pub struct WalkOracle {
    places: Arc<PlaceTable>,
    next_trip: u32,
}

impl WalkOracle {
    #[must_use]
    pub fn new(places: Arc<PlaceTable>) -> WalkOracle {
        WalkOracle {
            places,
            next_trip: 0,
        }
    }

    fn coord(&self, p: PlaceRef) -> WorldCoord {
        match self.places.coord_of(p) {
            Some(c) => c,
            None => {
                // Miejsce spoza katalogu to błąd wywołującego: kandydaci pochodzą
                // z `PlaceProvider`, a dom z `Residence`. W debug pada test, w release
                // podróż trwa minutę — symulacja nie ma się zatrzymywać przez jeden
                // wpis w danych.
                debug_assert!(false, "miejsce {p:?} spoza katalogu");
                WorldCoord::ORIGIN
            }
        }
    }
}

impl TravelOracle for WalkOracle {
    fn estimate(
        &self,
        from: PlaceRef,
        to: PlaceRef,
        _depart: MinuteOfDay,
        who: &CitizenView<'_>,
    ) -> TravelEstimate {
        let minutes = walk_minutes(self.coord(from), self.coord(to), speed_pct(who));
        TravelEstimate {
            minutes,
            cost: Money::ZERO,
            mode: TransportMode::Walk,
            reason: DecisionReason::ModeWalkOnly { minutes },
        }
    }

    fn begin_trip(
        &mut self,
        trip: TripRequest,
        who: &CitizenView<'_>,
        q: &mut EventQueue,
    ) -> TripHandle {
        let minutes = walk_minutes(self.coord(trip.from), self.coord(trip.to), speed_pct(who));
        let arrive_at = q.now() + u32::from(minutes);
        q.schedule(SimEvent::new(
            arrive_at,
            trip.traveller.entity().index(),
            EventKind::Arrive,
            trip.slot,
        ));
        let id = self.next_trip;
        self.next_trip = self.next_trip.wrapping_add(1);
        TripHandle {
            id,
            from: trip.from,
            to: trip.to,
            arrive_at,
            minutes,
            mode: TransportMode::Walk,
        }
    }

    fn places_on_route(&self, trip: &TripHandle, out: &mut ArrayVec<PlaceRef, MAX_ON_ROUTE>) {
        out.clear();
        let a = self.coord(trip.from);
        let b = self.coord(trip.to);
        let srodek = WorldCoord::new((a.x + b.x) / 2, (a.y + b.y) / 2, 0);
        let polowa_m = (((a.distance_sq_xy(b) as f64).sqrt() / 2.0) / 100.0) as f32;
        let promien = polowa_m + (ROUTE_CORRIDOR_CM / 100) as f32;

        // Zbieramy kandydatów ze wszystkich rodzajów miejsc: „widoczne z trasy" nie
        // zależy od tego, czego mieszkaniec akurat szuka (§5.7 — to jest źródło wiedzy,
        // nie wybór celu).
        let mut znalezione: Vec<(i64, PlaceRef)> = Vec::new();
        for kind in magnat_core::PlaceKind::ALL {
            self.places.for_each_near(*kind, srodek, promien, |e| {
                if e.place == trip.from || e.place == trip.to {
                    return;
                }
                if let Some(d) = odleglosc_od_odcinka(a, b, e.at) {
                    if d <= ROUTE_CORRIDOR_CM {
                        znalezione.push((d, e.place));
                    }
                }
            });
        }
        // Remis rozstrzyga `PlaceRef`, nie kolejność komórek indeksu.
        znalezione.sort_unstable();
        for (_, p) in znalezione.into_iter().take(MAX_ON_ROUTE) {
            out.push(p);
        }
    }
}

/// Odległość punktu od odcinka w centymetrach; `None`, gdy rzut pada poza odcinek —
/// miejsce „za plecami" nie jest widoczne z trasy, choć leży blisko jej końca.
fn odleglosc_od_odcinka(a: WorldCoord, b: WorldCoord, p: WorldCoord) -> Option<i64> {
    let (abx, aby) = (i64::from(b.x - a.x), i64::from(b.y - a.y));
    let (apx, apy) = (i64::from(p.x - a.x), i64::from(p.y - a.y));
    let dlugosc_sq = abx * abx + aby * aby;
    if dlugosc_sq == 0 {
        return Some(((apx * apx + apy * apy) as f64).sqrt() as i64);
    }
    let t = apx * abx + apy * aby;
    if t < 0 || t > dlugosc_sq {
        return None;
    }
    // |AP × AB| / |AB| — bez dzielenia przez zero, bo długość sprawdzona wyżej.
    let cross = (apx * aby - apy * abx).abs();
    Some((cross as f64 / (dlugosc_sq as f64).sqrt()) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Identity, Needs, Personality, Residence, Vitals};
    use magnat_core::{CitizenId, Entity, PlaceKind};

    fn widok<'a>(
        i: &'a Identity,
        v: &'a Vitals,
        n: &'a Needs,
        p: &'a Personality,
        r: &'a Residence,
    ) -> CitizenView<'a> {
        CitizenView {
            id: CitizenId(Entity::new(1, std::num::NonZeroU32::new(1).unwrap())),
            identity: i,
            vitals: v,
            needs: n,
            personality: p,
            residence: r,
            today: 0,
        }
    }

    #[test]
    fn kilometr_w_linii_prostej_to_kwadrans_marszu() {
        // 1 km manhattanowo × 1,25 = 1250 m przy 81 m/min → 16 minut.
        let m = walk_minutes(
            WorldCoord::new(0, 0, 0),
            WorldCoord::new(100_000, 0, 0),
            100,
        );
        assert_eq!(m, 16);
        // Podróż do sąsiedniego wejścia nigdy nie trwa zero minut.
        assert_eq!(
            walk_minutes(WorldCoord::ORIGIN, WorldCoord::new(10, 0, 0), 100),
            1
        );
    }

    #[test]
    fn dziecko_i_chory_ida_wolniej_niz_dorosly() {
        let dorosly = Identity {
            birth_day: -360 * 30,
            ..Identity::default()
        };
        let dziecko = Identity {
            birth_day: -360 * 8,
            ..Identity::default()
        };
        let zdrowy = Vitals {
            health: 90,
            energy: 90,
            ..Vitals::default()
        };
        let chory = Vitals {
            health: 20,
            energy: 20,
            ..Vitals::default()
        };
        let (n, p, r) = (
            Needs::default(),
            Personality::default(),
            Residence::default(),
        );

        assert_eq!(speed_pct(&widok(&dorosly, &zdrowy, &n, &p, &r)), 100);
        assert_eq!(speed_pct(&widok(&dziecko, &zdrowy, &n, &p, &r)), 85);
        assert_eq!(speed_pct(&widok(&dorosly, &chory, &n, &p, &r)), 75);
    }

    #[test]
    fn podroz_harmonogramuje_przybycie_na_wlasciwa_minute() {
        use crate::places::{PlaceEntry, PlaceTable};
        use magnat_core::BuildingId;

        let bud = |i: u32| {
            PlaceRef::Building(BuildingId(Entity::new(
                i,
                std::num::NonZeroU32::new(1).unwrap(),
            )))
        };
        let table = Arc::new(PlaceTable::build(vec![
            PlaceEntry {
                place: bud(1),
                kind: PlaceKind::Home,
                at: WorldCoord::new(0, 0, 0),
            },
            PlaceEntry {
                place: bud(2),
                kind: PlaceKind::Workplace,
                at: WorldCoord::new(100_000, 0, 0),
            },
            // Sklep w korytarzu trasy — powinien być widoczny z drogi do pracy.
            PlaceEntry {
                place: bud(3),
                kind: PlaceKind::Grocery,
                at: WorldCoord::new(50_000, 3_000, 0),
            },
            // Sklep 300 m w bok — nie jest.
            PlaceEntry {
                place: bud(4),
                kind: PlaceKind::Grocery,
                at: WorldCoord::new(50_000, 30_000, 0),
            },
        ]));
        let mut oracle = WalkOracle::new(table);
        let (i, v, n, p, r) = (
            Identity {
                birth_day: -360 * 30,
                ..Identity::default()
            },
            Vitals {
                health: 90,
                energy: 90,
                ..Vitals::default()
            },
            Needs::default(),
            Personality::default(),
            Residence::default(),
        );
        let who = widok(&i, &v, &n, &p, &r);

        let mut q = EventQueue::new();
        let handle = oracle.begin_trip(
            TripRequest {
                traveller: who.id,
                from: bud(1),
                to: bud(2),
                depart: MinuteOfDay::new(8 * 60),
                slot: 3,
            },
            &who,
            &mut q,
        );
        assert_eq!(handle.minutes, 16);
        assert_eq!(handle.arrive_at, 16);
        assert_eq!(q.len(), 1);

        let mut out = Vec::new();
        for _ in 0..17 {
            q.drain_minute(&mut out);
        }
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].event_kind(), Some(EventKind::Arrive));
        assert_eq!(out[0].slot, 3);

        let mut widoczne = ArrayVec::new();
        oracle.places_on_route(&handle, &mut widoczne);
        assert_eq!(
            widoczne.as_slice(),
            &[bud(3)],
            "korytarz trasy wpuścił sklep 300 m w bok albo zgubił ten przy drodze"
        );
    }
}
