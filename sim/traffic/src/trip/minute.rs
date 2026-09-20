//! Krok minutowy sieci: wpuszczenie podróży, przejazd krawędzi po krawędzi,
//! tankowanie i zejście z sieci (M4b/WP4).
//!
//! Wydzielone z `trip.rs` w R-WP6 bez zmiany zachowania. Kolejność wywołań jest
//! tu kontraktem hasha — plik czyta się od `step_minute` w dół, tak jak minuta biegnie.

use super::*;

impl TrafficNetwork {
    /// Krok minutowy warstwy mezo. Zwraca zdarzenia do zastosowania w świecie.
    ///
    /// Kolejność jest w całości wyznaczona przez klucz kopca i przez indeks krawędzi —
    /// nigdzie nie pytamy o zegar, wątek ani kolejność ukończenia jobów (§5.9).
    pub fn step_minute(
        &mut self,
        now: u32,
        road: &RoadGraph,
        vdf: &VdfTable,
        cat: &VehicleCatalog,
        pending: Vec<PendingTrip>,
        out: &mut Vec<TrafficEvent>,
    ) {
        self.mezo.freeze_minute(road, vdf);

        self.unjam(now);

        for p in pending {
            self.dispatch(now, road, cat, p, out);
        }

        // Zdarzenia tej minuty w porządku chronologicznym. Prędkości są zamrożone
        // (`freeze_minute`), więc kolejność nie wpływa na czas przejazdu nikogo —
        // wpływa wyłącznie na to, kto pierwszy wjedzie na krawędź bliską zapełnienia,
        // a ten porządek jest totalny i wynika z klucza kopca.
        let limit = (u64::from(now) + 1) * CS_PER_MINUTE;
        let mut kroki = 0u64;
        while let Some(Reverse((cs, _, slot))) = self.heap.peek().copied() {
            if cs >= limit {
                break;
            }
            self.heap.pop();
            self.advance(slot, now, road, cat, out);
            kroki += 1;
            if kroki >= MAX_EDGE_STEPS_PER_MINUTE {
                break;
            }
        }
    }

    fn dispatch(
        &mut self,
        now: u32,
        road: &RoadGraph,
        cat: &VehicleCatalog,
        p: PendingTrip,
        out: &mut Vec<TrafficEvent>,
    ) {
        let mut edges: Vec<EdgeId> = Vec::new();
        let mut refuel_after = u32::MAX;
        for (i, leg) in p.legs.iter().enumerate() {
            for l in &leg.legs {
                edges.extend_from_slice(&l.edges);
            }
            if i == 0 && p.station.is_some() {
                refuel_after = edges.len().saturating_sub(1) as u32;
            }
        }
        // Podsłuch krawędzi (M10b WP10.6): trasa jest znana **teraz** i tylko teraz —
        // `ActiveTrip` niesie ją dalej, ale tożsamość podróżnego i pełny przebieg
        // widać naraz wyłącznie przy wpuszczaniu. Pusty zbiór obserwowanych krawędzi
        // kosztuje jedno porównanie.
        self.watch.note(p.traveller, &edges);

        if edges.is_empty() {
            // Trasa pusta znaczy „origin == cel": mieszkaniec jest już na miejscu.
            out.push(TrafficEvent::Arrived {
                trip: p.trip,
                traveller: p.traveller,
                vehicle: p.vehicle,
                slot: p.slot,
                dest: p.dest,
                at: SimMinute(u64::from(now) + 1),
                planned_minutes: p.planned_minutes,
                tank_level_ul: p.tank_level_ul,
                ledger: TripLedger {
                    trip: p.trip,
                    depart: p.depart,
                    arrive: SimMinute(u64::from(now) + 1),
                    mode: TransportMode::Car as u8,
                    reason: p.reason,
                    ..TripLedger::default()
                },
            });
            self.stats.dispatched += 1;
            self.stats.arrived += 1;
            return;
        }

        let first = edges[0];
        let mut t = ActiveTrip {
            trip: p.trip,
            traveller: p.traveller,
            vehicle: p.vehicle,
            household: p.household,
            class: p.class,
            slot: p.slot,
            dest: p.dest,
            station: p.station,
            edges,
            refuel_after,
            pos: 0,
            entry_cs: u64::from(now) * CS_PER_MINUTE,
            exit_cs: 0,
            depart: p.depart,
            planned_minutes: p.planned_minutes,
            load: p.load,
            fuel_ul: 0,
            distance_cm: 0,
            tank_level_ul: p.tank_level_ul,
            tank_capacity_ul: p.tank_capacity_ul,
            bought_ul: 0,
            money: Money::ZERO,
            stops: 0,
            blocked_since: u32::MAX,
            reason: p.reason,
            station_reason: None,
            wait_next: crate::mezo::NO_WAIT,
            pending_node_cs: 0,
            pending_blocked_cs: 0,
            entries: Vec::new(),
            traced: p.traced,
        };

        self.mezo.enter(first);
        self.stats.entries += 1;
        self.stats.dispatched += 1;
        // Zimny start liczy się raz na podróż, na pierwszej krawędzi (§5.4).
        self.settle_current(&mut t, road, cat, 0, true);

        let slot = match self.free.pop() {
            Some(i) => {
                self.trips[i as usize] = Some(t);
                i
            }
            None => {
                self.trips.push(Some(t));
                (self.trips.len() - 1) as u32
            }
        };
        let (cs, veh) = {
            let t = self.trips[slot as usize].as_ref().expect("świeża podróż");
            (t.exit_cs, t.vehicle)
        };
        self.heap.push(Reverse((cs, veh, slot)));
    }

    /// Rozlicza przejazd bieżącej krawędzi i ustawia minutę wyjazdu.
    fn settle_current(
        &mut self,
        t: &mut ActiveTrip,
        road: &RoadGraph,
        cat: &VehicleCatalog,
        stops: u8,
        cold_start: bool,
    ) {
        let edge = t.edges[t.pos as usize];
        let e = &road.edges[edge.0 as usize];
        let link = &self.mezo.links[edge.0 as usize];
        let veh = VehicleSpecRef {
            cat,
            class: t.class,
        };
        let mut entry = settle_edge(e, link, &veh, t.load, t.entry_cs, stops, cold_start);
        entry.edge = edge;
        entry.node_delay_cs = std::mem::take(&mut t.pending_node_cs);
        entry.blocked_cs = std::mem::take(&mut t.pending_blocked_cs);
        t.exit_cs = t.entry_cs
            + u64::from(entry.travel_cs)
            + u64::from(stops) * u64::from(crate::mezo::HEADWAY_CS);
        t.fuel_ul += entry.fuel_ul;
        t.distance_cm += u64::from(e.length_cm);
        t.money = Money(t.money.0 + entry.money.0);
        t.stops += u32::from(stops);
        if t.traced {
            t.entries.push(entry);
        }
        self.stats.edge_settles += 1;
        self.stats.travel_cs_total += u64::from(entry.travel_cs);
        self.stats.fuel_burned_ul += entry.fuel_ul;
    }

    /// Jedno przejście przez krawędź. **Dokładnie jedno** — po nim podróż wraca
    /// do kopca, żeby przeplot z innymi był chronologiczny, a nie „każdy swoją trasę
    /// do końca". Od tego zależy, czy na krawędzi da się spotkać dwa pojazdy naraz,
    /// a więc czy w ogóle istnieje korek.
    fn advance(
        &mut self,
        slot: u32,
        now: u32,
        road: &RoadGraph,
        cat: &VehicleCatalog,
        out: &mut Vec<TrafficEvent>,
    ) {
        let Some(t) = self.trips[slot as usize].as_ref() else {
            return;
        };

        // Zawór bezpieczeństwa: podróż, która przekroczyła sufit czasu, kończy się
        // porażką zamiast wisieć na sieci w nieskończoność (ryzyko R4).
        let sufit = u32::from(t.planned_minutes)
            .saturating_mul(TRIP_TIMEOUT_FACTOR)
            .saturating_add(TRIP_TIMEOUT_EXTRA_MIN);
        if now.saturating_sub(t.depart.0 as u32) > sufit {
            let edge = t.edges[t.pos as usize];
            let ev = TrafficEvent::Failed {
                trip: t.trip,
                traveller: t.traveller,
                vehicle: t.vehicle,
                slot: t.slot,
                dest: t.dest,
                at: SimMinute(u64::from(now)),
                reason: TripFailure::Gridlock,
                tank_level_ul: t.tank_level_ul - t.fuel_ul + t.bought_ul,
                distance_cm: t.distance_cm,
            };
            self.stats.max_edges_per_trip = self.stats.max_edges_per_trip.max(u64::from(t.pos));
            self.mezo.leave(edge);
            self.stats.exits += 1;
            self.stats.failed += 1;
            self.stats.failed_gridlock += 1;
            self.release(slot);
            out.push(ev);
            return;
        }

        if t.pos as usize + 1 == t.edges.len() {
            self.finish(slot, now, out);
            return;
        }

        let cur = t.edges[t.pos as usize];
        let next = t.edges[t.pos as usize + 1];

        // Spillback: krawędź docelowa jest pełna, więc pojazd **zostaje** tam, gdzie
        // jest. Nie znika i nie dubluje się — stąd bierze się zachowanie liczby
        // pojazdów w systemie, sprawdzane co minutę w teście korka.
        let zapchana = self.mezo.queues[next.0 as usize].is_full();
        let wymuszone =
            self.mezo.queues[next.0 as usize].stuck_minutes >= GRIDLOCK_RELEASE_MIN as u16;
        if zapchana && !wymuszone {
            self.mezo.reject(next);
            if let Some(r) = self.rejects_total.get_mut(next.0 as usize) {
                *r += 1;
            }
            {
                let t = self.trips[slot as usize].as_mut().expect("podróż w toku");
                if t.blocked_since == u32::MAX {
                    t.blocked_since = now;
                    self.stats.spillbacks += 1;
                }
                let czekal = u64::from(now.saturating_sub(t.blocked_since));
                if czekal > self.stats.max_blocked_minutes {
                    self.stats.max_blocked_minutes = czekal;
                }
            }
            // Pojazd **czeka na zdarzenie**, a nie ponawia próby co jakiś czas: budzi
            // go zwolnienie miejsca na krawędzi, na którą czeka. Ponawianie co minutę
            // obcinałoby przepustowość każdego przewężenia do `storage_capacity`
            // pojazdów na minutę, a ponawianie co odstęp zamieniałoby korek
            // w miliony operacji na kopcu.
            self.enqueue_wait(next, slot);
            return;
        }
        if zapchana && wymuszone {
            self.stats.gridlock_releases += 1;
        }

        // Węzeł: opóźnienie i zatrzymania na wjeździe w następną krawędź.
        let node = road.edges[cur.0 as usize].to;
        let control = road.controls[node.0 as usize];
        let (priority, zawracanie) = turn_priority(road, cur, next);
        if zawracanie {
            self.stats.u_turns += 1;
        }
        let node_state = self.mezo.nodes[node.0 as usize];
        let (entry_cs, stops) = settle_node(control, priority, &node_state, t.exit_cs);
        let opoznienie_wezla = entry_cs.saturating_sub(t.exit_cs);
        self.stats.node_delay_cs_total += opoznienie_wezla;
        // Zawracanie to pełne zatrzymanie plus manewr — jedno dodatkowe zatrzymanie
        // ponad to, co policzył model węzła.
        let stops = stops.saturating_add(u8::from(zawracanie));
        self.mezo.nodes[node.0 as usize].arrivals_this_min = self.mezo.nodes[node.0 as usize]
            .arrivals_this_min
            .saturating_add(1);

        self.mezo.leave(cur);
        self.mezo.enter(next);
        self.wake_one(cur, entry_cs);

        let refuel = {
            let t = self.trips[slot as usize].as_mut().expect("podróż");
            t.pos += 1;
            t.entry_cs = entry_cs;
            t.blocked_since = u32::MAX;
            // Opóźnienie węzła należy do wiersza krawędzi, na którą pojazd właśnie
            // wjeżdża — powstanie ono dopiero w `settle_current` niżej (`N-6`).
            t.pending_node_cs = t
                .pending_node_cs
                .saturating_add(opoznienie_wezla.min(u64::from(u32::MAX)) as u32);
            t.refuel_after != u32::MAX && t.pos == t.refuel_after + 1
        };
        if refuel {
            self.refuel(slot, cat, out);
        }
        let mut t = self.trips[slot as usize].take().expect("podróż");
        self.settle_current(&mut t, road, cat, stops, false);
        let (cs, veh) = (t.exit_cs, t.vehicle);
        self.trips[slot as usize] = Some(t);
        self.heap.push(Reverse((cs, veh, slot)));
    }

    fn finish(&mut self, slot: u32, now: u32, out: &mut Vec<TrafficEvent>) {
        let t = self.trips[slot as usize].take().expect("podróż w toku");
        let edge = t.edges[t.pos as usize];
        self.mezo.leave(edge);
        self.wake_one(edge, t.exit_cs);
        self.stats.exits += 1;
        self.stats.arrived += 1;

        // Przybycie nie może wypaść w minucie już rozdanej przez pętlę doby —
        // zdarzenie na przeszłość nigdy nie zostałoby doręczone.
        let at = SimMinute(u64::from(now) + 1);
        let planned_arrive = t.depart.0 + u64::from(t.planned_minutes);
        self.stats.planned_minutes_total += u64::from(t.planned_minutes);
        self.stats.actual_minutes_total += at.0.saturating_sub(t.depart.0);
        if at.0 > planned_arrive {
            self.stats.late_arrivals += 1;
            self.stats.late_minutes += at.0 - planned_arrive;
        }

        self.stats.max_edges_per_trip = self.stats.max_edges_per_trip.max(t.edges.len() as u64);
        let ledger = TripLedger {
            trip: t.trip,
            entries: t.entries,
            total_fuel_ul: t.fuel_ul,
            total_money: t.money,
            edges: t.edges.len() as u32,
            stops: t.stops,
            distance_cm: t.distance_cm,
            depart: t.depart,
            arrive: at,
            mode: TransportMode::Car as u8,
            reason: t.station_reason.unwrap_or(if at.0 > planned_arrive {
                DecisionReason::TripDelayed {
                    planned_min: t.planned_minutes,
                    actual_min: at.0.saturating_sub(t.depart.0).min(u64::from(u16::MAX)) as u16,
                }
            } else {
                t.reason
            }),
        };
        debug_assert!(
            t.tank_level_ul - t.fuel_ul + t.bought_ul >= 0,
            "pojazd {} dojechał z ujemnym bakiem — wybór środka wpuścił podróż poza zasięg",
            t.vehicle
        );
        self.stats.reasons[match ledger.reason {
            DecisionReason::ModeChosen { .. } => 0,
            DecisionReason::ModeCompared { .. } => 1,
            DecisionReason::NoRouteForMode { .. } => 2,
            DecisionReason::NoParkingAtDestination { .. } => 3,
            DecisionReason::RefuelNeeded { .. } => 4,
            DecisionReason::StationChosen { .. } => 5,
            DecisionReason::TripDelayed { .. } => 6,
            _ => 7,
        }] += 1;
        out.push(TrafficEvent::Arrived {
            trip: t.trip,
            traveller: t.traveller,
            vehicle: t.vehicle,
            slot: t.slot,
            dest: t.dest,
            at,
            planned_minutes: t.planned_minutes,
            tank_level_ul: t.tank_level_ul - t.fuel_ul + t.bought_ul,
            ledger,
        });
        self.free.push(slot);
    }

    fn release(&mut self, slot: u32) {
        self.trips[slot as usize] = None;
        self.free.push(slot);
    }

    /// Tankowanie do pełna w chwili dojazdu na stację.
    ///
    /// Bilans jest domknięty w jednym miejscu: `bought = pojemność − (stan − spalone)`,
    /// więc suma zatankowanego minus suma spalonego równa się zmianie poziomów baków
    /// co do mikrolitra — i to jest cały dowód testu własnościowego paliwa.
    fn refuel(&mut self, slot: u32, cat: &VehicleCatalog, out: &mut Vec<TrafficEvent>) {
        let (trip, traveller, vehicle, household, station, units, cost, minute) = {
            let t = self.trips[slot as usize].as_mut().expect("podróż");
            let Some(station) = t.station else {
                return;
            };
            let w_baku = t.tank_level_ul - t.fuel_ul + t.bought_ul;
            let units = (t.tank_capacity_ul - w_baku).max(0);
            let kind = cat.spec(t.class).fuel;
            let cost = cat.fuel_cost(kind, units);
            t.bought_ul += units;
            t.money = Money(t.money.0 + cost.0);
            // Objazd liczony z tego, co już wiadomo: różnica między planem a chwilą
            // dojazdu na stację. Cena jest za litr, więc z mikrolitrów na litry.
            t.station_reason = Some(DecisionReason::StationChosen {
                detour_min: REFUEL_DWELL_MIN as u16,
                price_gr_per_l: cat.price_gr(kind).clamp(0, i64::from(u16::MAX)) as u16,
            });
            // Postój przy dystrybutorze przesuwa cały dalszy ciąg podróży.
            t.entry_cs += u64::from(REFUEL_DWELL_MIN) * CS_PER_MINUTE;
            (
                t.trip,
                t.traveller,
                t.vehicle,
                t.household,
                station,
                units,
                cost,
                SimMinute(t.entry_cs / CS_PER_MINUTE),
            )
        };
        self.stats.refuels += 1;
        self.stats.fuel_bought_ul += units;
        self.stats.fuel_spent = Money(self.stats.fuel_spent.0 + cost.0);
        out.push(TrafficEvent::Refuelled {
            trip,
            traveller,
            vehicle,
            household,
            station,
            units,
            cost,
            at: minute,
        });
    }
}
