//! Prowadzenie podróży: szacunek dla planera, wyruszenie na sieć, wejście do mikro
//! i pamięć ostatniej decyzji dla karty inspekcji (M4b/M4c).
//!
//! Wydzielone z `oracle.rs` w R-WP6 bez zmiany zachowania. Nazwa `journey`, a nie
//! `trip`: `crate::trip` jest w tej skrzyni zajęte przez sieć podróży w toku, więc
//! `crate::oracle::trip` obok niego czytałoby się jak pomyłka.

use super::*;

impl TrafficOracle {
    #[must_use]
    pub fn is_watched(&self, citizen: u32) -> bool {
        self.watched.lock().expect("watched").contains(&citizen)
    }

    /// Ostatni wybór środka transportu śledzonego mieszkańca, jeśli jakiś był.
    ///
    /// Razem z decyzją wraca **miejsce startu**: ledger go nie niesie (rejestruje
    /// krawędzie, nie adresy), a karta ma napisać „z domu do zakładu", a nie „donikąd".
    #[must_use]
    pub fn last_decision(&self, citizen: u32) -> Option<(PlaceRef, PlaceRef, ModeDecision)> {
        self.decisions
            .lock()
            .expect("decisions")
            .iter()
            .rev()
            .find(|(c, _, _, _)| *c == citizen)
            .map(|(_, from, to, d)| (*from, *to, d.clone()))
    }

    fn zapamietaj_decyzje(&self, citizen: u32, from: PlaceRef, to: PlaceRef, d: &ModeDecision) {
        let mut v = self.decisions.lock().expect("decisions");
        v.retain(|(c, _, _, _)| *c != citizen);
        v.push((citizen, from, to, d.clone()));
        // Śledzonych jest najwyżej ośmiu (decyzja 9.16), więc bufor nie ma jak urosnąć;
        // limit jest tu po to, żeby nie urósł, gdyby ktoś podniósł tamten.
        if v.len() > 16 {
            v.remove(0);
        }
    }

    pub(super) fn estimate_inner(
        &self,
        from: PlaceRef,
        to: PlaceRef,
        depart: MinuteOfDay,
        who: &CitizenView<'_>,
    ) -> TravelEstimate {
        let d = self.plan(from, to, who, TripPurpose::Work, depart, false);
        TravelEstimate {
            minutes: d.minutes,
            // Od M4c koszt jest realny: bilet, taryfa, paliwo i amortyzacja policzone
            // w `GeneralizedCost.money_cost`. Planer M3 widzi więc, ile podróż kosztuje,
            // zanim ją zaplanuje — a M5 dostanie tę liczbę do budżetu gospodarstwa.
            cost: d.money,
            mode: d.chosen.mode(),
            reason: d.reason,
        }
    }

    /// Wspólne ciało `begin_trip` — bierze `&self`, bo cała mutacja idzie przez
    /// zamki. Dzięki temu ten sam oracle da się trzymać w `Arc` i sięgać do niego
    /// z systemu ruchu, który potrzebuje routera do wstawienia postoju na stacji.
    pub fn start_trip(
        &self,
        trip: TripRequest,
        who: &CitizenView<'_>,
        q: &mut EventQueue,
    ) -> TripHandle {
        let now = q.now();
        let teraz = MinuteOfDay::new((now % 1440) as u16);
        let decision = self.plan(trip.from, trip.to, who, TripPurpose::Work, teraz, true);
        self.mode_counts[decision.chosen.as_index()].fetch_add(1, Ordering::Relaxed);
        self.policz_niewykonalne(&decision);
        let (minutes, reason) = (decision.minutes, decision.reason);
        let id = self.next_trip.fetch_add(1, Ordering::Relaxed);
        let citizen = trip.traveller.entity().index();
        if self.is_watched(citizen) {
            self.zapamietaj_decyzje(citizen, trip.from, trip.to, &decision);
        }

        // Komunikacja: pasażer wchodzi do kolejki przystanku, a `Arrive` wstawi
        // warstwa komunikacji w chwili, gdy wysiądzie (`TransitEvent::Alighted`).
        // Nie planujemy go tutaj, bo przepełniony autobus może go zostawić — i to
        // jest cała treść przepełnienia (§5.6).
        if decision.chosen == TravelOption::Transit {
            let a = self.coord_of(trip.from);
            let b = self.coord_of(trip.to);
            let dow = DayOfWeek::from_day_index(who.today.max(0) as u64);
            let podroz = self
                .transit
                .lock()
                .expect("transit")
                .plan_journey(a, b, teraz.get(), dow);
            if let Some(j) = podroz {
                // Ścieżka odwrotu liczona **przed** wejściem do kolejki: gdy pasażer
                // się podda, ma dojść pieszo, a nie znaleźć się na miejscu.
                let pieszo = self.network_walk_minutes(trip.from, trip.to, speed_pct(who));
                self.transit
                    .lock()
                    .expect("transit")
                    .enqueue(&j, citizen, trip.slot, now, pieszo);
                return TripHandle {
                    id,
                    from: trip.from,
                    to: trip.to,
                    arrive_at: now + u32::from(minutes),
                    minutes,
                    mode: TransportMode::Bus,
                };
            }
        }

        // Taksówka: pasażer dociera w czasie planowanym i płaci taryfę. Kurs nie
        // wjeżdża na sieć (`D5`), więc zdarzenie przybycia stawia się tutaj.
        if decision.chosen == TravelOption::Taxi {
            let arrive_at = now + u32::from(minutes);
            self.taxi_fares
                .fetch_add(decision.money.0.max(0) as u64, Ordering::Relaxed);
            q.schedule(SimEvent::new(
                arrive_at,
                citizen,
                EventKind::Arrive,
                trip.slot,
            ));
            return TripHandle {
                id,
                from: trip.from,
                to: trip.to,
                arrive_at,
                minutes,
                mode: TransportMode::Car,
            };
        }

        // Samochód: trasa idzie na sieć, a `Arrive` wstawi mezo w minucie faktycznego
        // przyjazdu. Brak trasy odbiera opcję **przy planowaniu** (`Y-1`) — mieszkaniec
        // idzie pieszo, a nie „podróż się nie udała".
        let mode = decision.chosen.mode();
        if decision.chosen.needs_parking() {
            let kierowca = match decision.chosen {
                TravelOption::CarOwn => self.driver_of(citizen),
                _ => self.household_car(who.identity.household, citizen),
            };
            if let Some(driver) = kierowca.filter(|d| self.take_vehicle(d.vehicle)) {
                if let Some(route) = self.car_route(trip.from, trip.to, teraz) {
                    let mut kolejka = self.pending.lock().expect("pending");
                    // Zlecenia odbiera **wyłącznie** `TrafficSystem`. Harmonogram bez
                    // niego nie kompiluje się inaczej ani nie pada — po prostu nikt
                    // nigdy nie dojeżdża, a auta zostają zajęte na zawsze. Rosnąca
                    // kolejka jest jedynym objawem, więc to ona ma krzyknąć.
                    debug_assert!(
                        kolejka.len() < 100_000,
                        "{} niewykonanych zleceń przejazdu — czy `TrafficSystem`                          jest w harmonogramie?",
                        kolejka.len()
                    );
                    kolejka.push(PendingTrip {
                        trip: TripId(id),
                        traveller: citizen,
                        vehicle: driver.vehicle,
                        household: who.identity.household,
                        class: driver.class,
                        slot: trip.slot,
                        origin: trip.from,
                        dest: trip.to,
                        depart: SimMinute(u64::from(now)),
                        planned_minutes: minutes,
                        purpose: TripPurpose::Work,
                        load: Mass::ZERO,
                        legs: vec![route],
                        station: None,
                        tank_level_ul: 0,
                        tank_capacity_ul: 0,
                        reason,
                        traced: self.is_watched(citizen),
                    });
                    drop(kolejka);
                    return TripHandle {
                        id,
                        from: trip.from,
                        to: trip.to,
                        arrive_at: now + u32::from(minutes),
                        minutes,
                        mode: TransportMode::Car,
                    };
                }
                // Trasy nie ma (`Y-1`) — auto wraca do puli i mieszkaniec idzie pieszo.
                self.release_vehicle(driver.vehicle);
            }
            // Auto odpadło po wyborze — zwolnione miejsce postojowe wraca do puli,
            // inaczej stałoby zajęte przez pojazd, który nigdzie nie pojechał.
            if let Some(p) = decision.parking {
                let _ = p;
                if let Some(k) = match decision.chosen {
                    TravelOption::CarOwn => self.driver_of(citizen),
                    _ => self.household_car(who.identity.household, citizen),
                } {
                    self.with_parking(|p| p.release(k.vehicle));
                }
            }
        }

        // Pieszo i rowerem: ścieżka M3 bez zmian. Ani pieszy, ani rowerzysta nie tworzy
        // korka i nie wchodzi na sieć drogową.
        let (mode, minutes) = if decision.chosen.needs_parking() || mode == TransportMode::Car {
            // Trasy samochodem nie ma albo auto jest w innej podróży — mieszkaniec
            // idzie pieszo i liczymy mu to uczciwie, po chodnikach.
            (
                TransportMode::Walk,
                self.network_walk_minutes(trip.from, trip.to, speed_pct(who)),
            )
        } else {
            (mode, minutes)
        };
        let arrive_at = now + u32::from(minutes);
        q.schedule(SimEvent::new(
            arrive_at,
            citizen,
            EventKind::Arrive,
            trip.slot,
        ));
        TripHandle {
            id,
            from: trip.from,
            to: trip.to,
            arrive_at,
            minutes,
            mode,
        }
    }

    pub(super) fn places_on_route_inner(
        &self,
        trip: &TripHandle,
        out: &mut ArrayVec<PlaceRef, MAX_ON_ROUTE>,
    ) {
        // Miejsca „po drodze" liczy się w korytarzu odcinka skąd–dokąd. Dla M4b
        // to ten sam model, co w M3: pełny korytarz po realnej trasie wymaga
        // geometrii krawędzi, a ta jest po stronie `sim/world` (`J-3`).
        out.clear();
        let a = self.coord_of(trip.from);
        let b = self.coord_of(trip.to);
        let srodek = WorldCoord::new((a.x + b.x) / 2, (a.y + b.y) / 2, (a.z + b.z) / 2);
        let promien = (euclid_cm(a, b) as f32 / 200.0).max(50.0);
        for kind in magnat_core::PlaceKind::ALL {
            if out.is_full() {
                break;
            }
            self.places.for_each_near(*kind, srodek, promien, |e| {
                if !out.is_full() {
                    let _ = out.push(e.place);
                }
            });
        }
    }

    pub(super) fn enter_micro_inner(&self, handle: &TripHandle, citizen: u32, depart: MinuteOfDay) {
        if handle.mode != TransportMode::Walk {
            return;
        }
        let route = [self.coord_of(handle.from), self.coord_of(handle.to)];
        self.micro.enter(
            citizen,
            &route,
            depart.get(),
            (u32::from(depart.get()) + u32::from(handle.minutes)).min(1_439) as u16,
        );
    }
}
