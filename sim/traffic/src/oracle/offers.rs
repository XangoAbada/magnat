//! Wybór środka transportu: pięć ofert i porównanie kosztu uogólnionego (M4c/WP6).
//!
//! Wydzielone z `oracle.rs` w R-WP6 bez zmiany zachowania. Pięć funkcji `offer_*`
//! to pięć wariantów jednego kontraktu — leżą obok siebie, żeby różnica między nimi
//! była widoczna, i **nie są** sprowadzone do wspólnej abstrakcji.

use super::*;
use magnat_core::CitizenReason;

impl TrafficOracle {
    pub(super) fn policz_niewykonalne(&self, d: &ModeDecision) {
        for c in d.candidates.as_slice() {
            let i = match c.infeasible {
                Some(Infeasible::NoCarInHousehold) => 0,
                Some(Infeasible::CarInUseBy(_)) => 1,
                Some(Infeasible::NoParkingWithinRadius { .. }) => 2,
                Some(Infeasible::InsufficientFuelRange { .. }) => 3,
                Some(Infeasible::NoTransitConnection) => 4,
                Some(Infeasible::NoRoute) => 5,
                Some(Infeasible::DistanceOverPersonalLimit { .. }) => 6,
                Some(
                    Infeasible::BelowMinimumAge
                    | Infeasible::VehicleBroken
                    | Infeasible::BeyondBudget { .. },
                ) => 7,
                None => continue,
            };
            self.infeasible_counts[i].fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Wybór środka transportu — **wspólny dla `estimate` i `begin_trip`** (WP6, §5.3).
    ///
    /// To jest punkt, w którym M4c wymienia regułę zastępczą M4b (`M-1`): zamiast
    /// „kto ma auto i jedzie dalej niż 800 m, ten jedzie", jest pełne porównanie
    /// kosztu uogólnionego wszystkich wykonalnych opcji.
    ///
    /// **Jedna funkcja dla planu i dla przejazdu** — plan i podróż muszą mówić o tej
    /// samej liczbie, inaczej slot `Commute` w planie M3 nie zgadza się z podróżą
    /// i mieszkaniec przeplanowuje dobę bez przerwy. Różnicę robi wyłącznie `commit`:
    ///
    /// - `commit == false` (szacunek planera): parking jest **sprawdzany**, nie
    ///   rezerwowany, a nawyk nie jest zapisywany. Planer woła to ~4 razy na dobę na
    ///   mieszkańca i nie ma prawa zająć miejsca postojowego na podróż, której może
    ///   nigdy nie być.
    /// - `commit == true` (faktyczne wyruszenie): miejsce jest rezerwowane, pasażer
    ///   wchodzi do kolejki przystanku, a wybór zapisuje się jako nawyk.
    pub(super) fn plan(
        &self,
        from: PlaceRef,
        to: PlaceRef,
        who: &CitizenView<'_>,
        purpose: TripPurpose,
        depart: MinuteOfDay,
        commit: bool,
    ) -> ModeDecision {
        let a = self.coord_of(from);
        let b = self.coord_of(to);
        let citizen = who.id.entity().index();
        let ctx = ModeContext {
            citizen,
            age_years: who.identity.age_years(who.today).max(0) as u16,
            status_percentile: who.vitals.status,
            hourly_net_income_gr: self.income_of(who.identity.household),
            purpose,
            weather: self.weather(),
            // Zakupy wraca się z siatkami, do pracy z teczką. Jedna liczba na cel
            // zamiast modelu koszyka — koszyk jest w M5 i wtedy wejdzie tu wprost.
            luggage_kg: match purpose {
                TripPurpose::Shopping => 6,
                _ => 0,
            },
            habit: self.habit_of(citizen),
        };

        // Sloty pojazdów, dla których wycena zarezerwowała miejsce postojowe. Przegrane
        // opcje oddają je zaraz po wyborze — patrz niżej.
        let mut zarezerwowane: Vec<(TravelOption, u32)> = Vec::new();
        let mut oferty: Vec<(TravelOption, Result<OptionOffer, Infeasible>)> =
            Vec::with_capacity(TravelOption::ALL.len());
        oferty.push((TravelOption::Walk, self.offer_walk(from, to, who)));
        oferty.push((TravelOption::Bike, self.offer_bike(a, b, &ctx)));
        oferty.push((
            TravelOption::Transit,
            self.offer_transit(a, b, depart, who.today),
        ));

        // Auto własne i auto rodzinne różnią się **wyłącznie** dostępnością pojazdu —
        // trasa, paliwo i parking są te same, więc liczą się raz.
        let wlasne = self.driver_of(citizen);
        let rodzinne = self
            .household_car(who.identity.household, citizen)
            .filter(|_| wlasne.is_none());
        for (opcja, kierowca) in [
            (TravelOption::CarOwn, wlasne),
            (TravelOption::CarHousehold, rodzinne),
        ] {
            let oferta = self.offer_car(from, to, a, b, depart, &ctx, kierowca, commit);
            if let (Ok(o), Some(k)) = (&oferta, kierowca) {
                if o.parking.is_some() {
                    zarezerwowane.push((opcja, k.vehicle));
                }
            }
            oferty.push((opcja, oferta));
        }
        oferty.push((
            TravelOption::Taxi,
            self.offer_taxi(from, to, a, b, depart, &ctx),
        ));

        match evaluate_modes(&self.params, &ctx, &oferty) {
            Some(d) => {
                if commit {
                    self.remember_habit(citizen, d.chosen);
                    // Rezerwacja powstaje przy **wycenie** auta, bo bez niej nie
                    // wiadomo ani czy opcja jest wykonalna, ani ile kosztuje dojście
                    // od parkingu. Gdy wygra co innego, miejsce wraca do puli w tej
                    // samej minucie — inaczej każda rozważona i odrzucona jazda
                    // zabierałaby centrum jedno miejsce na zawsze.
                    for (opcja, vehicle) in &zarezerwowane {
                        if *opcja != d.chosen {
                            self.with_parking(|p| p.release(*vehicle));
                        }
                    }
                }
                d
            }
            // **Żadna opcja nie jest wykonalna.** Nie wolno tego zostawić bez podróży:
            // mieszkaniec czeka na `Arrive` i bez niego stoi do końca gry (`M-4`).
            // Idzie więc pieszo, choćby długo — marsz jest jedyną opcją, która nie
            // może zawieść, bo nie potrzebuje ani pojazdu, ani miejsca, ani rozkładu.
            None => {
                for (_, vehicle) in &zarezerwowane {
                    self.with_parking(|p| p.release(*vehicle));
                }
                let minuty = self.network_walk_minutes(from, to, speed_pct(who));
                ModeDecision {
                    chosen: TravelOption::Walk,
                    minutes: minuty.max(1),
                    money: Money::ZERO,
                    parking: None,
                    candidates: ArrayVec::new(),
                    reason: DecisionReason::Citizen(CitizenReason::ModeWalkOnly {
                        minutes: minuty,
                    }),
                }
            }
        }
    }

    fn offer_walk(
        &self,
        from: PlaceRef,
        to: PlaceRef,
        who: &CitizenView<'_>,
    ) -> Result<OptionOffer, Infeasible> {
        // Odległość po chodnikach, nie w linii prostej. Miasto M2 potrafi mieć
        // 2,5-krotne nadłożenie wzdłuż rzeki albo torów, a mnożnik 1,25 z M3 zgadywał
        // tam o połowę za mało — i każde takie dojście kończyło się spóźnieniem.
        let minutes = self.network_walk_minutes(from, to, speed_pct(who));
        if minutes > self.params.max_walk_min {
            return Err(Infeasible::DistanceOverPersonalLimit { minutes });
        }
        Ok(OptionOffer {
            minutes,
            exposed_minutes: minutes,
            ..OptionOffer::default()
        })
    }

    /// Rower: formuła, nie router.
    ///
    /// `ponytail:` sufit nazwany i zmierzony w budżecie: `estimate` woła się ponad
    /// milion razy na dobę metropolii, a zapytanie do warstwy rowerowej kosztuje tyle
    /// samo co pieszej. Rower jest przy tym opcją marginalną (widełki 0,5–12 %), więc
    /// dokładność jego czasu przesuwa rozkład udziałów mniej niż szerokość tych widełek.
    /// Ścieżka wyjścia: ta sama co dla marszu — zapytanie do routera z `RouteCache`,
    /// gdy udział roweru zacznie mieć znaczenie dla bilansu (epoki po 2010).
    fn offer_bike(
        &self,
        a: WorldCoord,
        b: WorldCoord,
        ctx: &ModeContext,
    ) -> Result<OptionOffer, Infeasible> {
        if ctx.age_years < self.params.min_bike_age {
            return Err(Infeasible::BelowMinimumAge);
        }
        // Rower jest **własnością, nie prawem**. Losowanie idzie raz per mieszkaniec
        // (tick 0), więc jest stałe przez całą sesję i niezależne od liczby wątków
        // (00 §3.1) — nie trzeba go nigdzie trzymać ani haszować.
        if !magnat_core::rng(
            self.seed,
            magnat_core::StreamId::ModeChoice,
            ctx.citizen,
            magnat_core::Tick(0),
        )
        .gen_bool_permille(self.params.bike_ownership_permille)
        {
            return Err(Infeasible::VehicleBroken);
        }
        let dist = manhattan_cm(a, b) * WALK_DETOUR_NUM / DETOUR_DEN;
        let minutes = (dist / BIKE_SPEED_CM_PER_MIN).clamp(1, i64::from(u16::MAX)) as u16;
        if minutes > self.params.max_bike_min {
            return Err(Infeasible::DistanceOverPersonalLimit { minutes });
        }
        Ok(OptionOffer {
            minutes,
            exposed_minutes: minutes,
            ..OptionOffer::default()
        })
    }

    fn offer_transit(
        &self,
        a: WorldCoord,
        b: WorldCoord,
        depart: MinuteOfDay,
        today: i32,
    ) -> Result<OptionOffer, Infeasible> {
        let dow = DayOfWeek::from_day_index(today.max(0) as u64);
        let t = self.transit.lock().expect("transit");
        let Some(j) = t.plan_journey(a, b, depart.get(), dow) else {
            return Err(Infeasible::NoTransitConnection);
        };
        // Obłożenie linii bierze się z kursów w toku: pasażer stojący na przystanku
        // widzi, jak pełne są autobusy, które właśnie przejechały.
        let tloczno = t
            .runs()
            .iter()
            .filter(|r| r.line == j.line && r.capacity > 0)
            .map(|r| u32::from(r.occupancy) * 1_000 / u32::from(r.capacity))
            .max()
            .unwrap_or(0)
            .min(1_000) as u16;
        Ok(OptionOffer {
            minutes: j.total_minutes(),
            money: j.fare,
            exposed_minutes: j.access_min + j.wait_min + j.egress_min,
            crowding_permille: tloczno,
            transfers: j.transfers,
            walk_access_min: j.access_min + j.egress_min,
            parking: None,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn offer_car(
        &self,
        from: PlaceRef,
        to: PlaceRef,
        a: WorldCoord,
        b: WorldCoord,
        depart: MinuteOfDay,
        ctx: &ModeContext,
        kierowca: Option<DriverEntry>,
        commit: bool,
    ) -> Result<OptionOffer, Infeasible> {
        let Some(d) = kierowca else {
            return Err(Infeasible::NoCarInHousehold);
        };
        if ctx.age_years < self.params.min_drive_age {
            return Err(Infeasible::BelowMinimumAge);
        }
        let dist_cm = manhattan_cm(a, b);
        if dist_cm < i64::from(self.params.min_car_distance_m) * 100 {
            // Poniżej tego dystansu nikt nie wyprowadza auta: manewry, wyjazd
            // i parkowanie zjadają zysk, zanim jeszcze ruszy.
            return Err(Infeasible::DistanceOverPersonalLimit { minutes: 0 });
        }
        if self.busy.lock().expect("busy").get(d.vehicle as usize) == Some(&true) {
            return Err(Infeasible::CarInUseBy(d.citizen));
        }
        // Trasa z routera, nie formuła: plan i przejazd mają mówić o tej samej podróży.
        // Zapytanie idzie przez `RouteCache`, a para dom–praca powtarza się co dobę,
        // więc to jest prawie zawsze trafienie.
        let Some(r) = self.car_route(from, to, depart) else {
            // Trasy samochodem nie ma — 2,8 % par metropolii jej nie ma i to jest
            // prawda o sieci M2, nie błąd routera (`Y-1`).
            return Err(Infeasible::NoRoute);
        };
        let minuty_jazdy =
            (i64::from(r.planned_minutes) * PLANNING_MARGIN_PERMILLE / 1_000).max(1) as u16;

        // Parking u celu — **warunek wykonalności opcji, sprawdzany przy planowaniu**
        // (PRD §9.4, `M-4`). Brak miejsca odbiera opcję, a nie podróż w trakcie.
        let vot = self
            .params
            .vot_gr_per_min(ctx.hourly_net_income_gr, ctx.purpose);
        let do_kiedy = SimMinute(u64::from(depart.get()) + u64::from(minuty_jazdy) + 1);
        let miejsce = self.with_parking(|p| {
            if commit {
                p.find_and_reserve(b, PARKING_SEARCH_RADIUS_M, d.vehicle, vot, do_kiedy)
                    .map(Some)
            } else if p.any_free_within(b, PARKING_SEARCH_RADIUS_M) {
                Ok(None)
            } else {
                Err(crate::parking::ParkingDenied::AllFull { searched: 0 })
            }
        });
        let miejsce = match miejsce {
            Ok(m) => m,
            Err(crate::parking::ParkingDenied::NoLotInRadius) => {
                return Err(Infeasible::NoParkingWithinRadius { lots_searched: 0 })
            }
            Err(crate::parking::ParkingDenied::AllFull { searched }) => {
                return Err(Infeasible::NoParkingWithinRadius {
                    lots_searched: searched,
                })
            }
        };
        let dojscie = miejsce.map_or(0, |s| s.walk_minutes);

        // Pieniądz: paliwo z katalogu klasy plus amortyzacja (`D3` — do decyzji, nie
        // do budżetu) plus opłata parkingowa. Wszystko całkowite (00 §2).
        let spec = self.catalog.spec(d.class);
        let km = dist_cm / 100_000;
        let paliwo_ul = i64::from(spec.base_ml_per_100km) * dist_cm / 10_000;
        let paliwo = self.catalog.fuel_cost(spec.fuel, paliwo_ul);
        let zuzycie = spec.wear_gr_per_100km * km / 100;
        let oplata = miejsce.map_or(0, |s| s.price_gr_per_hour.0 * PARKING_ASSUMED_HOURS);
        Ok(OptionOffer {
            minutes: minuty_jazdy + dojscie,
            money: Money(paliwo.0 + zuzycie + oplata),
            exposed_minutes: dojscie,
            crowding_permille: 0,
            transfers: 0,
            walk_access_min: dojscie,
            parking: miejsce,
        })
    }

    /// Taksówka (`D5`): opcja z ceną z danych, bez encji firmy i bez floty.
    ///
    /// `ponytail:` sufit nazwany — taksówka **nie wjeżdża na sieć**, więc nie tworzy
    /// korka i nie pali paliwa; jej opłata idzie do rejestru przewoźnika, żeby bilans
    /// pieniądza się domykał. Ścieżka wyjścia: M7 stawia operatora z flotą i wtedy
    /// kurs staje się zwykłym `PendingTrip` z pojazdem.
    fn offer_taxi(
        &self,
        from: PlaceRef,
        to: PlaceRef,
        a: WorldCoord,
        b: WorldCoord,
        depart: MinuteOfDay,
        ctx: &ModeContext,
    ) -> Result<OptionOffer, Infeasible> {
        let km = manhattan_cm(a, b) / 100_000;
        let taryfa = self.params.taxi_base_gr + self.params.taxi_gr_per_km * km;
        // **Sama cena taksówki jej nie hamuje.** Dla mieszkańca bez auta i bez zasięgu
        // komunikacji jest ona jedyną szybką opcją, więc koszt uogólniony wybiera ją
        // nawet wtedy, gdy kurs kosztuje dniówkę. Próg budżetowy jest tu tym, czym
        // parking dla samochodu: warunkiem wykonalności, a nie składnikiem kosztu.
        let dzienny = ctx.hourly_net_income_gr * 8;
        if taryfa * 1_000 > dzienny * self.params.taxi_max_fare_permille_of_daily {
            return Err(Infeasible::BeyondBudget {
                fare_gr: taryfa.clamp(0, i64::from(u32::MAX)) as u32,
            });
        }
        let Some(r) = self.car_route(from, to, depart) else {
            return Err(Infeasible::NoRoute);
        };
        let minuty =
            (i64::from(r.planned_minutes) * PLANNING_MARGIN_PERMILLE / 1_000).max(1) as u16;
        Ok(OptionOffer {
            minutes: minuty + self.params.taxi_wait_min,
            money: Money(taryfa),
            exposed_minutes: 0,
            crowding_permille: 0,
            transfers: 0,
            walk_access_min: 0,
            parking: None,
        })
    }
}
