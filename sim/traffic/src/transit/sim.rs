//! Minuta sieci komunikacji: odjazdy, przejazd kursów, przesiadki i rezygnacje (M4c/WP7).
//!
//! Wydzielone z `transit.rs` w R-WP10 bez zmiany zachowania. Kolejność wywołań jest
//! tu kontraktem hasha: `dispatch` → `advance` → `give_up`, a plik czyta się od
//! `step_minute` w dół, tak jak minuta biegnie.

use super::*;

impl TransitNetwork {
    /// Wstawia pasażera do kolejki przystanku. Wołane z `begin_trip`.
    pub fn enqueue(
        &mut self,
        j: &TransitJourney,
        citizen: u32,
        slot: u8,
        now: u32,
        walk_fallback_min: u16,
    ) {
        let Some(l) = self.lines.iter_mut().find(|l| l.id == j.line) else {
            return;
        };
        let Some(s) = l.stops.get_mut(j.board_stop as usize) else {
            return;
        };
        let w = Waiting {
            since_min: now,
            citizen,
            alight_stop: j.alight_stop,
            egress_min: j.egress_min,
            slot,
            next: j.transfer_to,
            walk_fallback_min,
            passed: 0,
        };
        // Kolejka jest posortowana po kluczu totalnym, więc wstawienie idzie na
        // właściwe miejsce, a nie na koniec: dwie osoby z tej samej minuty mają
        // wsiadać w kolejności indeksu encji niezależnie od kolejności zgłoszeń.
        let i = s.waiting.partition_point(|x| *x < w);
        s.waiting.insert(i, w);
    }

    /// Krok minutowy: odjazdy z rozkładu, przejazd kursów, wsiadanie i wysiadanie.
    ///
    /// `driver_ready` mówi, czy wskazany mieszkaniec jest w pracy — sieć nie ma
    /// dostępu do świata, więc pyta o to wołającego.
    #[allow(clippy::too_many_arguments)]
    pub fn step_minute(
        &mut self,
        now: u32,
        dow: DayOfWeek,
        road: &RoadGraph,
        mezo: &MezoState,
        cat: &VehicleCatalog,
        drivers: &[u32],
        driver_ready: &mut dyn FnMut(u32) -> bool,
        out: &mut Vec<TransitEvent>,
    ) {
        self.dispatch(now, dow, drivers, driver_ready, out);
        self.advance(now, road, mezo, cat, out);
        self.give_up(now, out);
    }

    /// Wypuszcza kursy, których odjazd wypada w tej minucie.
    fn dispatch(
        &mut self,
        now: u32,
        dow: DayOfWeek,
        drivers: &[u32],
        driver_ready: &mut dyn FnMut(u32) -> bool,
        out: &mut Vec<TransitEvent>,
    ) {
        let minuta_doby = (now % 1440) as u16;
        for li in 0..self.lines.len() {
            let (id, wypada, pojazdy, pojemnosc) = {
                let l = &self.lines[li];
                (
                    l.id,
                    l.timetable.runs_on(dow) && l.timetable.departs_at(minuta_doby),
                    l.fleet.clone(),
                    l.capacity,
                )
            };
            if !wypada || pojazdy.is_empty() {
                continue;
            }
            // Pojazd wolny = taki, którego nie prowadzi żaden kurs w toku. Kolejność
            // jest kolejnością taboru linii, więc ten sam rozkład daje ten sam pojazd.
            let Some(vehicle) = pojazdy
                .iter()
                .copied()
                .find(|v| !self.runs.iter().any(|r| r.vehicle == *v))
            else {
                self.stats.runs_cancelled += 1;
                out.push(TransitEvent::RunCancelled {
                    line: id,
                    at: SimMinute(u64::from(now)),
                });
                continue;
            };
            // Kierowca: pierwszy z listy linii, który jest dziś w pracy. Brak kierowcy
            // odwołuje kurs — to jest cały model absencji w tej podfazie (§5.6).
            let Some(driver) = drivers.iter().copied().find(|c| driver_ready(*c)) else {
                self.stats.runs_cancelled += 1;
                out.push(TransitEvent::RunCancelled {
                    line: id,
                    at: SimMinute(u64::from(now)),
                });
                continue;
            };
            self.runs.push(TransitRun {
                line: id,
                vehicle,
                driver,
                stop_index: 0,
                depart_min: now,
                arrive_min: now,
                occupancy: 0,
                capacity: pojemnosc,
                delay_minutes: 0,
                onboard: Vec::new(),
                fuel_ul: 0,
            });
            self.stats.runs_started += 1;
        }
    }

    /// Przesuwa kursy, które dotarły na przystanek: wysiadka, wsiadka, odjazd dalej.
    fn advance(
        &mut self,
        now: u32,
        road: &RoadGraph,
        mezo: &MezoState,
        cat: &VehicleCatalog,
        out: &mut Vec<TransitEvent>,
    ) {
        let at = SimMinute(u64::from(now));
        let mut zakonczone: Vec<usize> = Vec::new();
        for ri in 0..self.runs.len() {
            if self.runs[ri].arrive_min > now {
                continue;
            }
            let li = {
                let line = self.runs[ri].line;
                match self.lines.iter().position(|l| l.id == line) {
                    Some(i) => i,
                    None => continue,
                }
            };
            let stop = self.runs[ri].stop_index;

            // 1. Wysiadka — kto jedzie do tego przystanku, wysiada tutaj. Pasażer
            //    z przesiadką nie kończy tu podróży, tylko wraca do kolejki drugiej
            //    linii: gdyby dostał `Arrive`, przyjechałby do celu z przystanku
            //    przesiadkowego, czyli teleportował się przez drugą nogę.
            let mut wysiadlo = 0u32;
            let mut przesiadki: Vec<Waiting> = Vec::new();
            {
                let r = &mut self.runs[ri];
                r.onboard.retain(|w| {
                    if w.alight_stop != stop {
                        return true;
                    }
                    wysiadlo += 1;
                    if w.next.0 != LineId(0) {
                        przesiadki.push(*w);
                    } else {
                        out.push(TransitEvent::Alighted {
                            citizen: w.citizen,
                            slot: w.slot,
                            egress_min: w.egress_min,
                            at,
                        });
                    }
                    false
                });
                r.occupancy = r
                    .occupancy
                    .saturating_sub(wysiadlo.min(u32::from(u16::MAX)) as u16);
            }
            self.stats.alightings += u64::from(wysiadlo);
            for w in przesiadki {
                self.przesiadka(&w, self.lines[li].stops[stop as usize].node, now, out);
            }

            // 2. Wsiadka — FIFO, aż do pojemności. Reszta zostaje na przystanku.
            let ostatni = self.lines[li].stops.len() as u16 - 1;
            let mut wsiadlo = 0u32;
            if stop < ostatni {
                let (fare, id) = (self.lines[li].fare, self.lines[li].id);
                let wolne = self.runs[ri]
                    .capacity
                    .saturating_sub(self.runs[ri].occupancy);
                let kolejka = &mut self.lines[li].stops[stop as usize].waiting;
                let mut zostaja: Vec<Waiting> = Vec::new();
                for w in std::mem::take(kolejka) {
                    if w.alight_stop <= stop {
                        // Kurs minął cel pasażera — poczeka na następny, który zaczyna
                        // od pierwszego przystanku.
                        zostaja.push(w);
                        continue;
                    }
                    if wsiadlo < u32::from(wolne) {
                        let czekal =
                            now.saturating_sub(w.since_min).min(u32::from(u16::MAX)) as u16;
                        self.stats.wait_minutes += u64::from(czekal);
                        out.push(TransitEvent::Boarded {
                            citizen: w.citizen,
                            line: id,
                            fare,
                            wait_min: czekal,
                            at,
                        });
                        self.runs[ri].onboard.push(w);
                        wsiadlo += 1;
                    } else {
                        out.push(TransitEvent::LeftBehind {
                            citizen: w.citizen,
                            line: id,
                            at,
                        });
                        let mut w = w;
                        if w.passed == 0 {
                            self.stats.left_behind += 1;
                        }
                        w.passed = w.passed.saturating_add(1);
                        self.stats.boarding_refusals += 1;
                        zostaja.push(w);
                    }
                }
                self.lines[li].stops[stop as usize].waiting = zostaja;
                let r = &mut self.runs[ri];
                r.occupancy = r
                    .occupancy
                    .saturating_add(wsiadlo.min(u32::from(u16::MAX)) as u16);
                self.stats.boardings += u64::from(wsiadlo);
                self.stats.max_occupancy = self.stats.max_occupancy.max(r.occupancy);
                self.stats.fare_revenue = Money(
                    self.stats.fare_revenue.0
                        + fare.0 * i64::from(wsiadlo.min(u32::from(u16::MAX))),
                );
            }

            // 3. Postój: baza plus czas wymiany pasażerów. To on zamienia przepełnienie
            //    w spóźnienie — im więcej chętnych, tym dłużej autobus stoi.
            let dwell_s = u32::from(self.lines[li].stops[stop as usize].dwell_base_s)
                + wsiadlo * BOARD_S
                + wysiadlo * ALIGHT_S;

            if stop >= ostatni {
                zakonczone.push(ri);
                continue;
            }

            // 4. Przejazd do następnego przystanku — **tym samym `settle_edge`**, którym
            //    jadą samochody. Autobus dzieli korek z resztą miasta; tramwaj i metro
            //    mają wydzielone torowisko, więc jadą swobodnie.
            let (czas_cs, paliwo_ul, koszt) =
                self.hop_cost(li, stop as usize, road, mezo, cat, now);
            let r = &mut self.runs[ri];
            r.fuel_ul += paliwo_ul;
            r.stop_index = stop + 1;
            r.depart_min = now;
            let minut = (u64::from(dwell_s) * 100 + czas_cs)
                .div_ceil(CS_PER_MINUTE)
                .max(1);
            r.arrive_min = now + minut.min(u64::from(u32::MAX)) as u32;

            // Spóźnienie narastające: różnica między tym, ile przejazd trwał naprawdę,
            // a ile trwałby przy prędkości swobodnej.
            let swobodnie = u64::from(self.free_hop_min[li][stop as usize]);
            let opoznienie = minut.saturating_sub(swobodnie).min(i16::MAX as u64) as i16;
            r.delay_minutes = r.delay_minutes.saturating_add(opoznienie);
            self.stats.delay_minutes += i64::from(opoznienie);
            self.stats.max_delay_minutes = self.stats.max_delay_minutes.max(r.delay_minutes);
            if paliwo_ul > 0 {
                let (id, vehicle) = (r.line, r.vehicle);
                self.stats.fuel_ul += paliwo_ul;
                self.stats.fuel_cost = Money(self.stats.fuel_cost.0 + koszt.0);
                out.push(TransitEvent::RunFuelled {
                    line: id,
                    vehicle,
                    units_ul: paliwo_ul,
                    cost: koszt,
                    at,
                });
            }
        }

        // Kursy na pętli: wszyscy jeszcze na pokładzie wysiadają, kurs znika.
        for ri in zakonczone.into_iter().rev() {
            let r = self.runs.swap_remove(ri);
            for w in r.onboard {
                self.stats.alightings += 1;
                out.push(TransitEvent::Alighted {
                    citizen: w.citizen,
                    slot: w.slot,
                    egress_min: w.egress_min,
                    at,
                });
            }
            self.stats.runs_finished += 1;
        }
    }

    /// Wstawia pasażera do kolejki drugiej linii na przystanku przesiadkowym.
    ///
    /// Gdy druga linia nie obsługuje już tego węzła (zmiana trasy), podróż kończy się
    /// tutaj z dojściem pieszo — to jest ta sama reguła co `GaveUp`: zawsze musi być
    /// wyjście, które się kończy (`M-4`).
    fn przesiadka(&mut self, w: &Waiting, node: NodeId, now: u32, out: &mut Vec<TransitEvent>) {
        let (line, cel) = w.next;
        let Some(lj) = self.lines.iter().position(|l| l.id == line) else {
            out.push(TransitEvent::Alighted {
                citizen: w.citizen,
                slot: w.slot,
                egress_min: w.egress_min,
                at: SimMinute(u64::from(now)),
            });
            return;
        };
        let Some(board) = self.lines[lj].stop_at(node) else {
            out.push(TransitEvent::Alighted {
                citizen: w.citizen,
                slot: w.slot,
                egress_min: w.egress_min,
                at: SimMinute(u64::from(now)),
            });
            return;
        };
        let dalej = Waiting {
            since_min: now,
            citizen: w.citizen,
            alight_stop: cel,
            egress_min: w.egress_min,
            slot: w.slot,
            next: (LineId(0), 0),
            walk_fallback_min: w.walk_fallback_min,
            passed: 0,
        };
        let kolejka = &mut self.lines[lj].stops[board as usize].waiting;
        let i = kolejka.partition_point(|x| *x < dalej);
        kolejka.insert(i, dalej);
    }

    /// Pasażerowie, którzy czekają dłużej niż [`MAX_WAIT_MIN`], rezygnują.
    fn give_up(&mut self, now: u32, out: &mut Vec<TransitEvent>) {
        let at = SimMinute(u64::from(now));
        let mut poddanych = 0u64;
        for l in &mut self.lines {
            for s in &mut l.stops {
                s.waiting.retain(|w| {
                    if now.saturating_sub(w.since_min) < MAX_WAIT_MIN {
                        return true;
                    }
                    poddanych += 1;
                    out.push(TransitEvent::GaveUp {
                        citizen: w.citizen,
                        slot: w.slot,
                        walk_min: w.walk_fallback_min,
                        at,
                    });
                    false
                });
            }
        }
        self.stats.gave_up += poddanych;
    }
}
