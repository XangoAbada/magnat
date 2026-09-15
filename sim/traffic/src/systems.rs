//! Systemy ECS fazy i jedyne miejsce, w którym ruch zapisuje cokolwiek do świata
//! (M4b, §5.8).
//!
//! `TrafficSystem` jest **wyłączny** (`K-21`). Deklaracja dostępu przez komponenty
//! nic by tu nie kupiła: krok minutowy dotyka zbiornika, stanu technicznego,
//! położenia pojazdu, gotówki mieszkańca, jego potrzeb i kolejki zdarzeń — czyli
//! praktycznie wszystkiego, co o ruchu wiadomo. Dałaby za to **fałszywą** deklarację,
//! gdyby ktoś czegoś nie wypisał. Cena jest jawna i zapisana w §7.3: ten system nie
//! korzysta ze zrównoleglenia.
//!
//! **To nie unieważnia kontraktu z §5.4.** System mikro z M4d jest osobnym systemem
//! i to on ma zadeklarować `Read<LinkState> + Write<VehicleState>`; test
//! `micro_writes_nothing` patrzy na niego, nie na ten.

use crate::oracle::TrafficOracle;
use crate::spec::{VdfTable, VehicleCatalog};
use crate::transit::TransitEvent;
use crate::trip::{PendingTrip, TrafficEvent, TrafficNetwork, TripFailure};
use crate::vehicle::{FuelTank, VehicleClass, VehicleCondition, VehicleLocation, VehicleOwner};
use magnat_agents::{
    citizen_by_index, DayStats, EventKind, EventQueue, NeedTable, Needs, SimEvent, Wealth,
};
use magnat_core::{Cadence, HashState, Money, NeedKind, PlaceRef, SimMinute, StateHasher};
use magnat_ecs::{Entity, System, SystemCtx, SystemDesc, World};
use std::sync::Arc;

/// Usługi ruchu: oracle (router, flota, stacje) i tabela diagramu podstawowego.
///
/// **Nie wchodzi do hasha stanu** i to jest świadome: graf, katalog pojazdów
/// i tabela VDF są danymi wejściowymi miasta, tak samo jak katalog miejsc w M3.
/// Stanem jest `TrafficNetwork` i komponenty pojazdów — te w hashu są.
pub struct TrafficServices {
    pub oracle: Arc<TrafficOracle>,
    pub vdf: VdfTable,
    /// Encje pojazdów, indeksowane slotem floty (`DriverEntry.vehicle`).
    pub fleet: Vec<Entity>,
    /// Mieszkańcy, którzy prowadzą kursy komunikacji — indeksy encji, posortowane.
    /// Kurs bierze pierwszego, który jest dziś w pracy (§5.6).
    pub transit_drivers: Vec<u32>,
}

impl HashState for TrafficServices {
    /// **Zmiana wobec M4b: usługi ruchu wchodzą do hasha** — nie w całości, tylko
    /// tym, co jest w nich stanem.
    ///
    /// Do M4b zasób był traktowany jak dane wejściowe miasta i pomijany. Już wtedy
    /// trzymał jednak `busy` — informację o tym, które auto jest w podróży — a ta
    /// wpływa na plan mieszkańca. M4c dokłada parkingi, komunikację, nawyk i macierz
    /// czasów przejazdu (`M-2`), więc luka przestała być teoretyczna. Graf, katalog
    /// pojazdów i tabela VDF **nadal** zostają poza hashem: są wejściem, nie wynikiem.
    fn hash_state(&self, h: &mut StateHasher) {
        self.oracle.hash_state(h);
    }
}

/// Rejestr opłat przewozowych: bilety, taryfy i parkingi (WP7, WP10).
///
/// Wchodzi do hasha z tego samego powodu co [`FuelLedger`]: to **pieniądz**, a test
/// własnościowy „wydatki mieszkańców == przychody operatorów" wymaga tolerancji 0
/// (00 §6). M4b zamknął parę „kierowca ↔ stacja"; M4c dokłada do niej pary
/// „pasażer ↔ przewoźnik" i „kierowca ↔ parking" (`M-6`) — nie drugi licznik,
/// tylko drugi wiersz w tym samym.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct FareLedger {
    /// Bilety komunikacji miejskiej.
    pub transit_revenue: Money,
    pub transit_tickets: u64,
    /// Taryfy taksówkowe (`D5` — przewoźnik bez encji do czasu M7).
    pub taxi_revenue: Money,
    pub taxi_rides: u64,
    /// Opłaty parkingowe.
    pub parking_revenue: Money,
    pub parking_stays: u64,
    /// Paliwo spalone przez tabor komunikacji — obciąża operatora i jest drugą
    /// stroną obrotu stacji, tak samo jak paliwo kierowcy.
    pub transit_fuel_cost: Money,
}

impl HashState for FareLedger {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.transit_revenue.0 as u64);
        h.write_u64(self.transit_tickets);
        h.write_u64(self.taxi_revenue.0 as u64);
        h.write_u64(self.taxi_rides);
        h.write_u64(self.parking_revenue.0 as u64);
        h.write_u64(self.parking_stays);
        h.write_u64(self.transit_fuel_cost.0 as u64);
    }
}

/// Rejestr obrotu stacji paliw — druga strona każdego grosza wydanego na paliwo.
///
/// Wchodzi do hasha, bo to jest **pieniądz**: bez tej sumy test własnościowy
/// „wydatki kierowców == przychody stacji" nie miałby z czym porównywać, a 00 §6
/// wymaga tolerancji 0. M6 podepnie tu zbiornik, M5 — obrót jako przychód firmy;
/// kontrakt zdarzenia `FuelPurchased` się przez to nie zmieni.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct FuelLedger {
    pub revenue: Money,
    pub purchases: u64,
    /// Zatankowane w mikrolitrach.
    pub volume_ul: i64,
    /// Spalone w mikrolitrach — licznik zamykający bilans paliwa.
    pub burned_ul: i64,
}

impl HashState for FuelLedger {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.revenue.0 as u64);
        h.write_u64(self.purchases);
        h.write_u64(self.volume_ul as u64);
        h.write_u64(self.burned_ul as u64);
    }
}

/// Zakończona podróż zapamiętana dla karty inspekcji (WP11).
///
/// **Bufor inspekcji, nie stan świata** — tak samo jak `Trace` w M3 (decyzja 9.16)
/// i z tego samego powodu: gdyby wchodził do hasha, wskazanie mieszkańca myszą
/// zmieniałoby hash świata i `camera_does_not_change_world` (A5) by to złapało.
///
/// Zapisują się wyłącznie podróże **śledzonych** mieszkańców — i tylko one mają
/// wypełnione `TripLedger.entries`, bo rejestr krawędź po krawędzi zbiera się właśnie
/// dla nich (`ActiveTrip.traced`).
#[derive(Clone, Debug)]
pub struct TripRecord {
    pub traveller: u32,
    /// Skąd wyruszył. Pochodzi z bufora decyzji oracle'a, bo ledger notuje krawędzie,
    /// a nie adresy; `None` w tamtym buforze znaczy „podróż zaczęła się przed
    /// włączeniem śledzenia" i wtedy start jest nieznany.
    pub origin: Option<PlaceRef>,
    pub dest: PlaceRef,
    pub planned_minutes: u16,
    pub ledger: crate::trip::TripLedger,
    /// Pełne porównanie środków transportu, jeśli oracle je zapamiętał.
    pub decision: Option<crate::mode::ModeDecision>,
}

/// Pierścień ostatnich podróży śledzonych mieszkańców.
#[derive(Default, Debug)]
pub struct TripLog {
    records: Vec<TripRecord>,
}

impl TripLog {
    /// Ilu podróży wstecz sięga karta. Ośmiu śledzonych × cztery podróże na dobę —
    /// tyle, ile mieści się w jednym ekranie, i ani rekordu więcej.
    pub const CAP: usize = 32;

    #[must_use]
    pub fn new() -> TripLog {
        TripLog::default()
    }

    pub fn push(&mut self, r: TripRecord) {
        if self.records.len() >= TripLog::CAP {
            self.records.remove(0);
        }
        self.records.push(r);
    }

    #[must_use]
    pub fn records(&self) -> &[TripRecord] {
        &self.records
    }

    /// Ostatnia podróż danego mieszkańca — to jest to, co pokazuje karta.
    #[must_use]
    pub fn last_of(&self, citizen: u32) -> Option<&TripRecord> {
        self.records.iter().rev().find(|r| r.traveller == citizen)
    }
}

/// Rejestruje komponenty, zasoby i haki hasha fazy M4.
///
/// Komponent niezarejestrowany nie wchodzi do hasha stanu (00 §3.6) i rozjazd
/// przeszedłby przez CI niezauważony — to samo uzasadnienie, co przy `K-16`.
pub fn register_traffic(world: &mut World, services: TrafficServices, network: TrafficNetwork) {
    world.register_component::<VehicleOwner>();
    world.register_component::<VehicleCondition>();
    world.register_component::<FuelTank>();
    world.register_component::<VehicleLocation>();
    world.register_component::<VehicleClass>();
    world.insert_resource(services);
    world.insert_resource(network);
    world.insert_resource(FuelLedger::default());
    // Nakładki są **pomiarem, nie stanem** — tak samo jak `TrafficStats`. Gdyby weszły
    // do hasha, przełączenie nakładki przez gracza zmieniałoby hash świata, czyli
    // dokładnie to, przed czym broni §5.4. Dlatego zasób bez `register_resource_hash`.
    world.insert_resource(crate::overlay::TrafficOverlay::new());
    // Dziennik podróży, tak samo jak nakładki, jest pomiarem — bez haka hasha.
    world.insert_resource(TripLog::new());
    world.insert_resource(FareLedger::default());
    world.register_resource_hash::<TrafficNetwork>();
    world.register_resource_hash::<FuelLedger>();
    world.register_resource_hash::<FareLedger>();
    world.register_resource_hash::<TrafficServices>();
}

/// Krok minutowy warstwy mezo: wysłanie zleceń, przejazd, rozliczenie.
pub struct TrafficSystem {
    desc: SystemDesc,
    events: Vec<TrafficEvent>,
    transit: Vec<TransitEvent>,
}

impl TrafficSystem {
    #[must_use]
    pub fn new(_world: &World) -> TrafficSystem {
        TrafficSystem {
            desc: SystemDesc::new("traffic.Mezo", Cadence::EveryMinute).exclusive(),
            events: Vec::new(),
            transit: Vec::new(),
        }
    }
}

impl System for TrafficSystem {
    fn desc(&self) -> &SystemDesc {
        &self.desc
    }

    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let now = ctx.tick.0 as u32;
        let world = ctx.world_mut();
        if world.get_resource::<TrafficServices>().is_none() {
            return;
        }
        let (oracle, vdf, fleet, kierowcy) = {
            let s = world.resource::<TrafficServices>();
            (
                s.oracle.clone(),
                s.vdf,
                s.fleet.clone(),
                s.transit_drivers.clone(),
            )
        };
        let catalog = oracle.catalog().clone();
        // Doba świata — pogoda przelicza się tylko przy jej zmianie (`D4`).
        oracle.set_day(u64::from(now) / 1440);
        // Blokady parkingowe, które wygasły: podróż, która nigdy nie wyruszyła, nie
        // ma prawa trzymać miejsca (zawór, nie ścieżka główna — patrz `parking.rs`).
        oracle.with_parking(|p| p.expire(SimMinute(u64::from(now))));

        // 1. Zlecenia zebrane przez `begin_trip` w poprzedniej minucie. Dopiero tutaj
        //    wiadomo, ile jest paliwa w baku — bo tutaj jest świat.
        let (pending, odrzucone) =
            przygotuj(world, &oracle, &catalog, &fleet, oracle.take_pending());

        // Zlecenie, którego sieć nie przyjmie, **nie może zniknąć**: mieszkaniec
        // czeka na `Arrive` i bez niego stanąłby w miejscu do końca gry. Dostaje je
        // w czasie, który zakładał plan — to samo, co dostałby, gdyby poszedł pieszo.
        for (traveller, slot, minuty, vehicle) in odrzucone {
            oracle.release_vehicle(vehicle);
            przybycie(
                world,
                traveller,
                slot,
                SimMinute(u64::from(now) + u64::from(minuty.max(1))),
                0,
            );
        }

        // 2. Przejazd. Sieć nie widzi świata; oddaje zdarzenia.
        let mut net = std::mem::take(world.resource_mut::<TrafficNetwork>());
        self.events.clear();
        oracle.with_road(|road| {
            net.step_minute(now, road, &vdf, &catalog, pending, &mut self.events);
        });
        *world.resource_mut::<TrafficNetwork>() = net;

        // 3. Skutki w świecie — jedyne miejsce, w którym ruch cokolwiek zapisuje.
        let satysfakcja = world
            .resource::<NeedTable>()
            .spec(NeedKind::Mobility)
            .satisfaction;
        for ev in std::mem::take(&mut self.events) {
            match &ev {
                TrafficEvent::Arrived { vehicle, .. } | TrafficEvent::Failed { vehicle, .. } => {
                    oracle.release_vehicle(*vehicle);
                }
                TrafficEvent::Refuelled { .. } => {}
            }
            if let TrafficEvent::Arrived {
                traveller,
                dest,
                planned_minutes,
                ledger,
                ..
            } = &ev
            {
                if oracle.is_watched(*traveller) {
                    let ostatnia = oracle.last_decision(*traveller);
                    let origin = ostatnia.as_ref().map(|(f, _, _)| *f);
                    let decision = ostatnia.map(|(_, _, d)| d);
                    world.resource_mut::<TripLog>().push(TripRecord {
                        traveller: *traveller,
                        origin,
                        dest: *dest,
                        planned_minutes: *planned_minutes,
                        ledger: ledger.clone(),
                        decision,
                    });
                }
            }
            zastosuj(world, &fleet, ev, satysfakcja);
        }

        // 4. Komunikacja miejska: odjazdy z rozkładu, przejazd kursów, wsiadanie
        //    i wysiadanie. Idzie **po** kroku mezo, bo czas przejazdu autobusu liczy
        //    się z `LinkState` tej minuty — z tego samego obłożenia, które właśnie
        //    zatrzymało samochody (§5.6).
        self.transit.clear();
        let dow = magnat_core::DayOfWeek::from_day_index(u64::from(now) / 1440);
        let mut gotowy = |c: u32| kierowca_w_pracy(world, c, dow);
        oracle.with_road(|road| {
            let net = world.resource::<TrafficNetwork>();
            oracle.with_transit(|t| {
                t.step_minute(
                    now,
                    dow,
                    road,
                    &net.mezo,
                    &catalog,
                    &kierowcy,
                    &mut gotowy,
                    &mut self.transit,
                );
            });
        });
        for ev in std::mem::take(&mut self.transit) {
            zastosuj_transit(world, ev, satysfakcja);
        }

        // 5. Nakładki danych (WP11): bufor tylny przepisuje się tutaj, a renderer czyta
        //    przedni — dzięki temu klatka nigdy nie czeka na krok minutowy i odwrotnie.
        {
            let godzina = ((now / 60) % 24) as u8;
            let origin = world.resource::<crate::overlay::TrafficOverlay>().origin();
            // Izochrona odczytuje się **przed** wejściem w `with_road`, a nie w środku:
            // obie metody biorą ten sam `Mutex` routera, a `std::sync::Mutex` nie jest
            // wznawialny — zagnieżdżenie zakleszcza wątek na amen (ostrzeżenie przy
            // `with_road` z M4b). Dzielnic jest kilkadziesiąt, więc tabela jest tańsza
            // niż jedno zapytanie CCH.
            let izochrona: Vec<u16> = oracle.with_matrix(|m| {
                (0..m.n_districts())
                    .map(|d| {
                        m.lookup(
                            magnat_core::DistrictId(origin),
                            magnat_core::DistrictId(d),
                            godzina,
                            magnat_core::TransportMode::Car,
                        )
                        .unwrap_or(0)
                    })
                    .collect()
            });
            oracle.with_road(|road| {
                let mezo = &world.resource::<TrafficNetwork>().mezo;
                oracle.with_parking(|parking| {
                    oracle.with_transit(|transit| {
                        world.resource::<crate::overlay::TrafficOverlay>().rebuild(
                            &crate::overlay::OverlayInputs {
                                minute: now,
                                road,
                                mezo,
                                parking,
                                transit,
                                origin_district: origin,
                            },
                            |d| izochrona.get(usize::from(d)).copied().unwrap_or(0),
                        );
                    });
                });
            });
        }

        // 6. Zasilenie warstwy Mikro (WP8). Idzie **na końcu minuty**, po tym jak mezo
        //    policzyło wszystko — warstwa dostaje stan, a nie wpływa na niego. Przy
        //    zamkniętym kadrze `begin_vehicle_feed` zwraca `false` i headless nie płaci
        //    nic poza jednym sprawdzeniem atomika.
        if oracle.micro().begin_vehicle_feed() {
            oracle.with_road(|road| {
                world
                    .resource::<TrafficNetwork>()
                    .feed_micro(oracle.micro(), road, &catalog);
                oracle.with_transit(|t| t.feed_micro(oracle.micro(), road, &catalog));
            });
            oracle.micro().end_vehicle_feed();
        }

        // 7. Taryfy taksówkowe: oracle je zebrał przy wyruszeniu, tu trafiają do
        //    rejestru, żeby bilans pieniądza miał drugą stronę (`M-6`).
        let taryfy = oracle.taxi_fares();
        let l = world.resource_mut::<FareLedger>();
        if taryfy.0 > l.taxi_revenue.0 {
            l.taxi_rides += 1;
            l.taxi_revenue = taryfy;
        }
    }
}

/// Czy mieszkaniec jest dziś w pracy i zdolny poprowadzić kurs.
///
/// Brak kierowcy odwołuje kurs — to jest cały model absencji w tej podfazie (§5.6).
/// Choroba i wolne przenoszą się wprost z `Employment`, bo kierowca **jest**
/// mieszkańcem i ma tę pracę w planie dnia, a nie obok niego.
fn kierowca_w_pracy(world: &World, citizen: u32, dow: magnat_core::DayOfWeek) -> bool {
    let Some(c) = citizen_by_index(world, citizen) else {
        return false;
    };
    let Some(e) = world.get::<magnat_agents::Employment>(c).copied() else {
        return false;
    };
    e.works_on(dow) && e.flags & magnat_agents::Employment::FLAG_SICK_LEAVE == 0
}

/// Skutki zdarzeń komunikacji w świecie.
fn zastosuj_transit(world: &mut World, ev: TransitEvent, satysfakcja: u8) {
    match ev {
        TransitEvent::Boarded { citizen, fare, .. } => {
            {
                let l = world.resource_mut::<FareLedger>();
                l.transit_revenue = Money(l.transit_revenue.0 + fare.0);
                l.transit_tickets += 1;
            }
            // Pasażer płaci dokładnie tyle, ile dostaje przewoźnik — obie strony
            // biorą tę samą liczbę, więc bilans domyka się z konstrukcji.
            if let Some(c) = citizen_by_index(world, citizen) {
                if let Some(w) = world.get_mut::<Wealth>(c) {
                    w.cash = Money(w.cash.0 - fare.0);
                }
            }
        }
        TransitEvent::Alighted {
            citizen,
            slot,
            egress_min,
            at,
        } => {
            // Dojście z przystanku do celu dolicza się tutaj: pasażer wysiadł, ale
            // jeszcze nie dotarł, a plan dnia mierzy drzwi–drzwi.
            przybycie(
                world,
                citizen,
                slot,
                SimMinute(at.0 + u64::from(egress_min)),
                satysfakcja,
            );
        }
        // Pozostawiony na przystanku **nie dostaje** `Arrive`: czeka na następny kurs,
        // a jeśli się nie doczeka, `GaveUp` zamknie podróż marszem.
        TransitEvent::LeftBehind { .. } => {}
        TransitEvent::GaveUp {
            citizen,
            slot,
            walk_min,
            at,
        } => {
            // Zawór z `M-4`: podróż bez zakończenia zawiesza mieszkańca do końca gry.
            // Idzie pieszo i **liczy się mu to uczciwie** — przybycie w następnej
            // minucie byłoby teleportacją w nagrodę za czterdzieści pięć minut
            // czekania na przystanku.
            przybycie(
                world,
                citizen,
                slot,
                SimMinute(at.0 + u64::from(walk_min.max(1))),
                satysfakcja,
            );
        }
        TransitEvent::RunFuelled { units_ul, cost, .. } => {
            let l = world.resource_mut::<FuelLedger>();
            l.revenue = Money(l.revenue.0 + cost.0);
            l.purchases += 1;
            l.volume_ul += units_ul;
            l.burned_ul += units_ul;
            let f = world.resource_mut::<FareLedger>();
            f.transit_fuel_cost = Money(f.transit_fuel_cost.0 + cost.0);
        }
        TransitEvent::RunCancelled { .. } => {}
    }
}

/// Dopina do zlecenia stan zbiornika i — gdy trzeba — postój na stacji.
///
/// Tankowanie jest **w podróży**, nie osobnym zadaniem planera. Plan §5.7 zakładał
/// zadanie w planie dnia; okazało się to niewykonalne bez otwarcia planera M3, którego
/// kryterium akceptacji tej podfazy zabrania ruszać — a zdanie testowe fazy M4 §1
/// brzmi „tankuje **po drodze**", więc objazd na stację jest bliższy temu, co faza
/// obiecuje, niż osobna wyprawa.
type Odrzucone = Vec<(u32, u8, u16, u32)>;

fn przygotuj(
    world: &mut World,
    oracle: &TrafficOracle,
    catalog: &VehicleCatalog,
    fleet: &[Entity],
    zlecenia: Vec<PendingTrip>,
) -> (Vec<PendingTrip>, Odrzucone) {
    let mut out = Vec::with_capacity(zlecenia.len());
    let mut odrzucone: Odrzucone = Vec::new();
    for mut p in zlecenia {
        let odrzuc = |o: &mut Odrzucone, p: &PendingTrip| {
            o.push((p.traveller, p.slot, p.planned_minutes, p.vehicle));
        };
        let Some(veh) = fleet.get(p.vehicle as usize).copied() else {
            odrzuc(&mut odrzucone, &p);
            continue;
        };
        let Some(tank) = world.get::<FuelTank>(veh).copied() else {
            odrzuc(&mut odrzucone, &p);
            continue;
        };
        if world
            .get::<VehicleCondition>(veh)
            .is_some_and(|c| c.is_broken(p.depart))
        {
            odrzuc(&mut odrzucone, &p);
            continue;
        }
        p.tank_level_ul = tank.level;
        p.tank_capacity_ul = tank.capacity;

        // Zapas zasięgu liczony z odległości w linii prostej i spalania klasy —
        // dokładność nie ma tu znaczenia, bo to jest próg bezpieczeństwa, nie wycena.
        let a = oracle.coord_of(p.origin);
        let b = oracle.coord_of(p.dest);
        let dist_cm = i64::from((b.x - a.x).abs()) + i64::from((b.y - a.y).abs());
        let spec = catalog.spec(p.class);
        let szacunek = i64::from(spec.base_ml_per_100km) * dist_cm / 10_000;
        if tank.level < tank.refuel_threshold
            || tank.level < szacunek * crate::oracle::FUEL_RESERVE_FACTOR
        {
            // **Auto, którego nie ma czym zatankować, nie jest opcją transportową**
            // (kryterium WP5). Gdy w korytarzu trasy nie ma osiągalnej stacji,
            // zlecenie wraca do wołającego i mieszkaniec idzie pieszo — zamiast
            // wjechać na sieć z zapasem, który nie wystarczy do celu.
            if wstaw_stacje(oracle, &mut p, a, b) {
                p.reason = magnat_core::DecisionReason::RefuelNeeded {
                    level_permille: tank.level_permille(),
                };
            } else if tank.level < szacunek {
                odrzuc(&mut odrzucone, &p);
                continue;
            }
        }
        if let Some(loc) = world.get_mut::<VehicleLocation>(veh) {
            if loc.is_parked() {
                loc.depart(0);
            }
        }
        out.push(p);
    }
    (out, odrzucone)
}

/// `true`, gdy postój na stacji udało się wstawić w trasę.
fn wstaw_stacje(
    oracle: &TrafficOracle,
    p: &mut PendingTrip,
    a: magnat_core::WorldCoord,
    b: magnat_core::WorldCoord,
) -> bool {
    let Some((station, _detour)) = oracle.station_on_route(a, b) else {
        return false;
    };
    let (Some(od), Some(do_)) = (oracle.nearest_node(a), oracle.nearest_node(b)) else {
        return false;
    };
    let godzina = magnat_core::MinuteOfDay::new((p.depart.0 % 1440) as u16);
    let (Some(l0), Some(l1)) = (
        oracle.route_nodes(od, station.node, godzina),
        oracle.route_nodes(station.node, do_, godzina),
    ) else {
        return false;
    };
    p.legs = vec![l0, l1];
    p.station = Some(station.place);
    true
}

fn zastosuj(world: &mut World, fleet: &[Entity], ev: TrafficEvent, satysfakcja: u8) {
    match ev {
        // To jest zdarzenie `FuelPurchased { station, volume, unit_price }` z kontraktu
        // M4 §6, tylko z wolumenem w mikrolitrach (`K-25`). M6 podepnie pod nie zbiornik
        // stacji, M5 — obrót jako przychód firmy; rozbicie na stacje należy do nich,
        // bo to one mają firmy. Tutaj zostaje suma, bo tylko ona zamyka bilans pieniądza.
        TrafficEvent::Refuelled {
            traveller,
            units,
            cost,
            ..
        } => {
            {
                let l = world.resource_mut::<FuelLedger>();
                l.revenue = Money(l.revenue.0 + cost.0);
                l.purchases += 1;
                l.volume_ul += units;
            }
            // Kierowca płaci dokładnie tyle, ile stacja dostaje — obie strony
            // biorą tę samą liczbę, więc tolerancja bilansu jest 0 z konstrukcji.
            if let Some(c) = citizen_by_index(world, traveller) {
                if let Some(w) = world.get_mut::<Wealth>(c) {
                    w.cash = Money(w.cash.0 - cost.0);
                }
            }
        }
        TrafficEvent::Arrived {
            trip: _,
            traveller,
            vehicle,
            slot,
            dest,
            at,
            planned_minutes,
            tank_level_ul,
            ledger,
        } => {
            zaparkuj(
                world,
                fleet,
                vehicle,
                dest,
                tank_level_ul,
                ledger.distance_cm,
            );
            {
                let l = world.resource_mut::<FuelLedger>();
                l.burned_ul += ledger.total_fuel_ul;
            }
            przybycie(world, traveller, slot, at, satysfakcja);
            let spoznienie =
                at.0.saturating_sub(ledger.depart.0 + u64::from(planned_minutes));
            let s = world.resource_mut::<DayStats>();
            s.trips += 1;
            let _ = spoznienie;
        }
        TrafficEvent::Failed {
            trip: _,
            traveller,
            vehicle,
            slot,
            dest,
            at,
            reason,
            tank_level_ul,
            distance_cm,
        } => {
            // Pojazd zostaje tam, dokąd zmierzał: podróż przerwana w połowie sieci
            // nie ma lepszego miejsca na postój, a pojazd bez położenia byłby widmem
            // (ryzyko R6). To jest uproszczenie i takie zostaje do WP7 (parkingi).
            zaparkuj(world, fleet, vehicle, dest, tank_level_ul, distance_cm);
            przybycie(world, traveller, slot, at, 0);
            debug_assert!(
                reason != TripFailure::NoFuel,
                "pojazd stanął bez paliwa — wybór środka wpuścił podróż poza zasięg"
            );
        }
    }
}

fn zaparkuj(
    world: &mut World,
    fleet: &[Entity],
    vehicle: u32,
    dest: PlaceRef,
    tank_level_ul: i64,
    distance_cm: u64,
) {
    let Some(veh) = fleet.get(vehicle as usize).copied() else {
        return;
    };
    if let Some(t) = world.get_mut::<FuelTank>(veh) {
        t.level = tank_level_ul;
    }
    if let Some(c) = world.get_mut::<VehicleCondition>(veh) {
        c.odometer_cm = c.odometer_cm.saturating_add(distance_cm);
    }
    if let Some(loc) = world.get_mut::<VehicleLocation>(veh) {
        loc.arrive(dest);
    }
}

/// Zdarzenie `Arrive` dla mieszkańca i zaspokojenie mobilności.
///
/// Mobilność jest jedyną potrzebą, której nie zaspokaja wizyta gdziekolwiek
/// (`data/needs/needs.ron`, `places: []`): zaspokaja ją **fakt dojechania**.
/// M4 wnosi ten mechanizm razem z tempem spadku (`Z-2`).
fn przybycie(world: &mut World, traveller: u32, slot: u8, at: SimMinute, satysfakcja: u8) {
    let Some(c) = citizen_by_index(world, traveller) else {
        return;
    };
    if satysfakcja > 0 {
        if let Some(n) = world.get_mut::<Needs>(c) {
            let i = NeedKind::Mobility.as_index();
            n.level[i] = n.level[i].saturating_add(satysfakcja).min(100);
        }
    }
    let q = world.resource_mut::<EventQueue>();
    let minuta = (at.0 as u32).max(q.now() + 1);
    q.schedule(SimEvent::new(minuta, traveller, EventKind::Arrive, slot));
}

/// Zużycie techniczne pojazdów — raz na dobę, z przebiegu.
pub struct VehicleWearSystem {
    desc: SystemDesc,
}

impl VehicleWearSystem {
    #[must_use]
    pub fn new(world: &World) -> VehicleWearSystem {
        VehicleWearSystem {
            desc: SystemDesc::new("traffic.VehicleWear", Cadence::EveryDay)
                .with_query::<&mut VehicleCondition, ()>(world),
        }
    }

    /// Zużycie rośnie o jeden punkt na każde 15 tys. km przebiegu — ta sama liczba,
    /// która wyznacza przegląd. Wartość jest w kodzie, a nie w danych, bo dopóki
    /// nie ma serwisów (M7) i awarii (M4d), nie ma czego stroić.
    const WEAR_PER_SERVICE: u8 = 1;
}

impl System for VehicleWearSystem {
    fn desc(&self) -> &SystemDesc {
        &self.desc
    }

    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        for c in ctx.query::<&mut VehicleCondition, ()>().iter() {
            if c.odometer_cm >= c.next_service_cm {
                c.wear = c
                    .wear
                    .saturating_add(VehicleWearSystem::WEAR_PER_SERVICE)
                    .min(100);
                c.next_service_cm = c
                    .next_service_cm
                    .saturating_add(VehicleCondition::SERVICE_INTERVAL_CM);
            }
        }
    }
}
