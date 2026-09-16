//! Planowanie podróży komunikacją: dojście, przejazd, jedna przesiadka (M4c/WP7).
//!
//! Wydzielone z `transit.rs` w R-WP10 bez zmiany zachowania. Tu nie zmienia się nic
//! w stanie sieci — wszystkie metody biorą `&self` i zwracają wycenę. Remisy
//! rozstrzyga klucz `(dojście, linia, przystanek)`, nigdy układ indeksu przestrzennego.

use super::*;

impl TransitNetwork {
    /// Przystanki w promieniu dojścia od punktu, jako `(linia, przystanek, minuty)`.
    fn stops_near(&self, at: WorldCoord, radius_m: u16, out: &mut Vec<(u16, u16, u16)>) {
        out.clear();
        let Some(index) = self.index.as_ref() else {
            return;
        };
        let c = Vec2::new(at.x as f32 / 100.0, at.y as f32 / 100.0);
        index.for_each_in_radius(c, f32::from(radius_m), |_, i| {
            let (li, si) = self.stops_flat[i as usize];
            let s = &self.lines[li as usize].stops[si as usize];
            out.push((li, si, crate::parking::walk_minutes(s.at, at)));
        });
        // Klucz totalny: najpierw dojście, potem linia, potem przystanek. Bez tego
        // wynik zależałby od układu komórek indeksu przestrzennego.
        out.sort_unstable_by_key(|(li, si, m)| (*m, *li, *si));
    }

    /// Najlepsza podróż komunikacją między dwoma punktami albo `None`, gdy jej nie ma.
    ///
    /// Jedna linia albo jedna przesiadka. Kryterium jest sumaryczny czas, a remisy
    /// rozstrzyga `(linia, przystanek)` — deterministycznie, bez losowania.
    #[must_use]
    pub fn plan_journey(
        &self,
        from: WorldCoord,
        to: WorldCoord,
        depart: u16,
        dow: DayOfWeek,
    ) -> Option<TransitJourney> {
        if self.lines.is_empty() {
            return None;
        }
        let mut skad = Vec::new();
        let mut dokad = Vec::new();
        self.stops_near(from, MAX_ACCESS_M, &mut skad);
        self.stops_near(to, MAX_ACCESS_M, &mut dokad);
        if skad.is_empty() || dokad.is_empty() {
            return None;
        }
        let mut best: Option<(u16, TransitJourney)> = None;
        for (li, si, access) in &skad {
            let l = &self.lines[*li as usize];
            if !l.timetable.runs_on(dow) {
                continue;
            }
            let Some(odjazd) = l.timetable.next_departure(depart.saturating_add(*access)) else {
                continue;
            };
            let wait = odjazd.saturating_sub(depart.saturating_add(*access));
            for (lj, sj, egress) in &dokad {
                if lj != li || sj <= si {
                    continue;
                }
                let ride = self.ride_min(*li, *si, *sj);
                let j = TransitJourney {
                    line: l.id,
                    board_stop: *si,
                    alight_stop: *sj,
                    access_min: *access,
                    wait_min: wait,
                    ride_min: ride,
                    egress_min: *egress,
                    transfers: 0,
                    fare: l.fare,
                    transfer_to: (LineId(0), 0),
                };
                let suma = j.total_minutes();
                if best.as_ref().is_none_or(|(b, _)| suma < *b) {
                    best = Some((suma, j));
                }
            }
        }
        if best.is_none() {
            best = self.plan_with_transfer(&skad, &dokad, depart, dow);
        }
        best.map(|(_, j)| j)
    }

    /// Czas przejazdu między przystankami przy prędkości swobodnej, w minutach.
    fn ride_min(&self, line: u16, from: u16, to: u16) -> u16 {
        self.free_hop_min[line as usize][from as usize..to as usize]
            .iter()
            .map(|m| u32::from(*m))
            .sum::<u32>()
            .min(u32::from(u16::MAX)) as u16
    }

    /// Podróż z **jedną** przesiadką: linia A do przystanku wspólnego, linia B dalej.
    ///
    /// Przystanek wspólny to ten sam **węzeł grafu** na obu liniach — nie „blisko",
    /// tylko ten sam, bo przesiadka między przystankami oddalonymi o przecznicę jest
    /// dojściem, a dojście ma już swój składnik kosztu i nie należy go liczyć dwa razy.
    ///
    /// `ponytail:` jedna przesiadka, przeszukiwanie parami linii. Sufit nazwany
    /// i policzony: przy `L` liniach po `S` przystanków kosztuje to `O(L² · S)`,
    /// czyli przy 24 liniach po 20 przystanków ~11 tys. porównań na zapytanie —
    /// do przyjęcia, bo wołający ma jedno zapytanie na podróż, nie na krawędź.
    /// Ścieżka wyjścia: RAPTOR na tabeli połączeń, gdy linii będzie więcej niż
    /// kilkanaście na dzielnicę.
    fn plan_with_transfer(
        &self,
        skad: &[(u16, u16, u16)],
        dokad: &[(u16, u16, u16)],
        depart: u16,
        dow: DayOfWeek,
    ) -> Option<(u16, TransitJourney)> {
        let mut best: Option<(u16, TransitJourney)> = None;
        for (li, si, access) in skad {
            let a = &self.lines[*li as usize];
            if !a.timetable.runs_on(dow) {
                continue;
            }
            let Some(odjazd) = a.timetable.next_departure(depart.saturating_add(*access)) else {
                continue;
            };
            let wait = odjazd.saturating_sub(depart.saturating_add(*access));
            for (lj, sj, egress) in dokad {
                if lj == li {
                    continue;
                }
                let b = &self.lines[*lj as usize];
                if !b.timetable.runs_on(dow) {
                    continue;
                }
                // Przystanek wspólny: po `si` na linii A i przed `sj` na linii B.
                for (pa, sa) in a.stops.iter().enumerate().skip(*si as usize + 1) {
                    let Some(pb) = b.stops[..*sj as usize]
                        .iter()
                        .position(|s| s.node == sa.node)
                    else {
                        continue;
                    };
                    let pa = pa as u16;
                    let pb = pb as u16;
                    let jazda_a = self.ride_min(*li, *si, pa);
                    let jazda_b = self.ride_min(*lj, pb, *sj);
                    // Czekanie na drugą linię: połowa odstępu, bo rozkłady nie są
                    // zsynchronizowane. Wartość oczekiwana przy przyjeździe losowym.
                    let przesiadka = b.timetable.headway_base_min / 2;
                    let j = TransitJourney {
                        line: a.id,
                        board_stop: *si,
                        alight_stop: pa,
                        access_min: *access,
                        wait_min: wait,
                        ride_min: jazda_a.saturating_add(przesiadka).saturating_add(jazda_b),
                        egress_min: *egress,
                        transfers: 1,
                        fare: Money(a.fare.0 + b.fare.0),
                        transfer_to: (b.id, *sj),
                    };
                    let suma = j.total_minutes();
                    if best.as_ref().is_none_or(|(x, _)| suma < *x) {
                        best = Some((suma, j));
                    }
                }
            }
        }
        best
    }

    pub(super) fn hop_cost(
        &self,
        li: usize,
        stop: usize,
        road: &RoadGraph,
        mezo: &MezoState,
        cat: &VehicleCatalog,
        now: u32,
    ) -> (u64, i64, Money) {
        let l = &self.lines[li];
        if !l.mode.shares_road() {
            // Wydzielone torowisko: czas swobodny, bez wpływu ruchu drogowego (§5.6).
            return (
                u64::from(self.free_hop_min[li][stop]) * CS_PER_MINUTE,
                0,
                Money::ZERO,
            );
        }
        let veh = VehicleSpecRef {
            cat,
            class: l.class,
        };
        let mut cs = 0u64;
        let mut paliwo = 0i64;
        let mut entry_cs = u64::from(now) * CS_PER_MINUTE;
        for e in &l.hops[stop] {
            let edge = &road.edges[e.0 as usize];
            let link = &mezo.links[e.0 as usize];
            // Przystanek na początku odcinka to jedno zatrzymanie; zimny start
            // wyłącznie na pierwszej krawędzi kursu.
            let wpis = settle_edge(
                edge,
                link,
                &veh,
                Mass::ZERO,
                entry_cs,
                u8::from(e == &l.hops[stop][0]),
                stop == 0 && e == &l.hops[stop][0],
            );
            cs += u64::from(wpis.travel_cs);
            paliwo += wpis.fuel_ul;
            entry_cs += u64::from(wpis.travel_cs);
        }
        let koszt = cat.fuel_cost(cat.spec(l.class).fuel, paliwo);
        (cs, paliwo, koszt)
    }
}
