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
    world.register_resource_hash::<TrafficNetwork>();
    world.register_resource_hash::<FuelLedger>();
}

/// Krok minutowy warstwy mezo: wysłanie zleceń, przejazd, rozliczenie.
pub struct TrafficSystem {
    desc: SystemDesc,
    events: Vec<TrafficEvent>,
}

impl TrafficSystem {
    #[must_use]
    pub fn new(_world: &World) -> TrafficSystem {
        TrafficSystem {
            desc: SystemDesc::new("traffic.Mezo", Cadence::EveryMinute).exclusive(),
            events: Vec::new(),
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
        let (oracle, vdf, fleet) = {
            let s = world.resource::<TrafficServices>();
            (s.oracle.clone(), s.vdf, s.fleet.clone())
        };
        let catalog = oracle.catalog().clone();

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
            zastosuj(world, &fleet, ev, satysfakcja);
        }
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
            zaparkuj(world, fleet, vehicle, dest, tank_level_ul, ledger.distance_cm);
            {
                let l = world.resource_mut::<FuelLedger>();
                l.burned_ul += ledger.total_fuel_ul;
            }
            przybycie(world, traveller, slot, at, satysfakcja);
            let spoznienie = at.0.saturating_sub(ledger.depart.0 + u64::from(planned_minutes));
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
                c.wear = c.wear.saturating_add(VehicleWearSystem::WEAR_PER_SERVICE).min(100);
                c.next_service_cm = c
                    .next_service_cm
                    .saturating_add(VehicleCondition::SERVICE_INTERVAL_CM);
            }
        }
    }
}
