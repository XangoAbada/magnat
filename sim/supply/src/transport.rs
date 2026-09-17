//! Zlecenia transportowe (M6b §5.6, WP5).
//!
//! Jedyna droga, którą partia przechodzi między zakładami. Cały pakiet jest po to,
//! żeby `prop_no_teleport` dało się udowodnić: `Store::load` i `Store::unload` wymagają
//! `TransportOrderId`, a `TransportOrderId` nadaje wyłącznie [`Transport::order`].
//!
//! **Czego tu nie ma i dlaczego.** Konsolidacja milk-run (Clarke–Wright) jest w opisie
//! §5.6, ale należy do **WP12**, czyli do M6d — i to nie jest przeoczenie, tylko
//! kolejność: konsolidacja opłaca się dopiero razem z centrum dystrybucyjnym, a `DC`
//! to `WarehouseRole::Distribution`, którego M6b nie ma. Zlecenie wozi jeden ładunek
//! na jednej trasie; scalanie zleceń dokłada się nad tym, nie zamiast tego.
//!
//! **Czego nie ma po stronie M4.** Kontrakt z §6.2 dokumentu fazy zakłada
//! `traffic::dispatch(order) -> VehicleArrivedAtSite`, ciężarówki w `data/vehicles/`
//! i uprawnienia `Licence::ADR`. Nic z tego jeszcze nie istnieje — M4 dowiózł ruch
//! pasażerski. Dlatego trasa wchodzi tu przez wąski trait [`FreightOracle`], tak samo
//! jak podróż mieszkańca wchodzi do M3 przez `TravelOracle`: M6b definiuje gniazdo
//! i atrapę, a wtyczkę z prawdziwym routerem wkłada ten, kto pierwszy będzie miał
//! ciężarówkę. Szczegóły w tabeli korekt dokumentu podfazy.

use magnat_core::{
    DecisionReason, GoodId, HashState, Mass, Money, SimMinute, SiteId, StateHasher, Volume, Q,
};
use std::collections::BTreeMap;

use crate::batch::SlotId;
use crate::batch::{BatchId, TransportOrderId};
use crate::catalog::{Catalog, StorageClass};
use crate::tuning::TransportTuning;
use crate::Store;

pub mod consolidate;
pub use consolidate::{consolidate, ConsolidationLimits, MilkRun};

/// Rurociąg — środek transportu bez pojazdu, kierowcy i rampy.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct PipelineId(pub u32);

/// Nadwozie — słownik mieszka od M6d w `engine/core` (`K-40`), bo deklaruje je także
/// katalog pojazdów M4 (`data/vehicles/classes.ron`). Tu zostaje re-eksport, więc nazwy
/// z §5.6 nie drgnęły.
pub use magnat_core::BodyType;

/// Nadwozie wymagane przez klasę przechowywania. Chłodnia w drodze nie jest
/// preferencją — towar `Chilled` przewieziony skrzynią psuje się ośmiokrotnie
/// szybciej (`R9`), więc to jest wymaganie, a nie sugestia.
///
/// Funkcja wolna, a nie metoda: `BodyType` należy teraz do `core`, a `StorageClass`
/// do `sim/supply`, więc `impl` po tej stronie łamałby regułę sieroty. Ten sam zabieg
/// co przy `RoadClass` i `spec()` w M4 (`K-23`).
#[must_use]
pub const fn body_for_storage(c: StorageClass) -> BodyType {
    match c {
        StorageClass::Chilled | StorageClass::Frozen => BodyType::Reefer,
        StorageClass::Tank => BodyType::Tanker,
        StorageClass::Silo => BodyType::Tipper,
        _ => BodyType::Box,
    }
}

/// Czego zlecenie wymaga od pojazdu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct VehicleRequirements {
    pub body: BodyType,
    pub min_payload: Mass,
    pub min_volume: Volume,
    /// Wymaga kierowcy z uprawnieniami ADR. Do czasu, aż M4 wprowadzi uprawnienia,
    /// jest to **deklaracja**, nie ograniczenie — i lepiej, żeby była zapisana już
    /// teraz, niż żeby cysterna z paliwem jeździła bez niej przez trzy fazy.
    pub adr: bool,
    pub storage_class: StorageClass,
}

impl VehicleRequirements {
    /// Wymagania wynikające z samego towaru — to one są domyślne, a nie puste.
    #[must_use]
    pub fn for_good(cat: &Catalog, good: GoodId, mass: Mass) -> VehicleRequirements {
        let g = cat.good(good);
        VehicleRequirements {
            body: body_for_storage(g.storage),
            min_payload: mass,
            min_volume: g.volume_of(mass),
            adr: g.hazard.needs_permit(),
            storage_class: g.storage,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Carrier {
    OwnFleet(magnat_core::FirmId),
    Hired {
        firm: magnat_core::FirmId,
        quote: Money,
    },
    Pipeline(PipelineId),
    Unassigned,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FailReason {
    NoCarrier,
    NoVehicle,
    NoDriver,
    Expired,
    Refused,
    RouteBlocked,
    Accident,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TransportOrderState {
    Draft,
    Tendered { closes: SimMinute },
    Assigned { pickup_eta: SimMinute },
    LoadingQueue,
    Loading { until: SimMinute },
    EnRoute { eta: SimMinute },
    UnloadingQueue,
    Unloading { until: SimMinute },
    Done,
    Failed(FailReason),
}

impl TransportOrderState {
    #[must_use]
    pub const fn is_final(self) -> bool {
        matches!(
            self,
            TransportOrderState::Done | TransportOrderState::Failed(_)
        )
    }

    const fn tag(self) -> u8 {
        match self {
            TransportOrderState::Draft => 0,
            TransportOrderState::Tendered { .. } => 1,
            TransportOrderState::Assigned { .. } => 2,
            TransportOrderState::LoadingQueue => 3,
            TransportOrderState::Loading { .. } => 4,
            TransportOrderState::EnRoute { .. } => 5,
            TransportOrderState::UnloadingQueue => 6,
            TransportOrderState::Unloading { .. } => 7,
            TransportOrderState::Done => 8,
            TransportOrderState::Failed(_) => 9,
        }
    }
}

/// Zlecenie transportowe.
#[derive(Clone, Debug)]
pub struct TransportOrder {
    pub id: TransportOrderId,
    pub from: SiteId,
    pub to: SiteId,
    pub from_slot: SlotId,
    pub to_slot: SlotId,
    pub good: GoodId,
    pub mass: Mass,
    pub requires: VehicleRequirements,
    pub ready_at: SimMinute,
    /// Kiedy najpóźniej.
    pub due_at: SimMinute,
    pub carrier: Carrier,
    pub price: Money,
    pub state: TransportOrderState,
    pub reason: DecisionReason,
    /// Ładunek — uchwyty do partii w [`Store`], nie partie. `AE-1`.
    pub cargo: Vec<BatchId>,
}

impl HashState for TransportOrder {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.id.0);
        self.from.entity().hash_state(h);
        self.to.entity().hash_state(h);
        h.write_u32(self.from_slot.0);
        h.write_u32(self.to_slot.0);
        h.write_u16(self.good.0);
        self.mass.hash_state(h);
        h.write_u8(self.requires.body as u8);
        h.write_u8(u8::from(self.requires.adr));
        h.write_u8(self.requires.storage_class as u8);
        self.ready_at.hash_state(h);
        self.due_at.hash_state(h);
        match self.carrier {
            Carrier::OwnFleet(f) => {
                h.write_u8(0);
                f.entity().hash_state(h);
            }
            Carrier::Hired { firm, quote } => {
                h.write_u8(1);
                firm.entity().hash_state(h);
                quote.hash_state(h);
            }
            Carrier::Pipeline(p) => {
                h.write_u8(2);
                h.write_u32(p.0);
            }
            Carrier::Unassigned => h.write_u8(3),
        }
        self.price.hash_state(h);
        h.write_u8(self.state.tag());
        match self.state {
            TransportOrderState::Tendered { closes } => closes.hash_state(h),
            TransportOrderState::Assigned { pickup_eta } => pickup_eta.hash_state(h),
            TransportOrderState::Loading { until } | TransportOrderState::Unloading { until } => {
                until.hash_state(h)
            }
            TransportOrderState::EnRoute { eta } => eta.hash_state(h),
            TransportOrderState::Failed(r) => h.write_u8(r as u8),
            _ => {}
        }
        h.write_u16(self.reason.discriminant());
        h.write_u32(self.cargo.len() as u32);
        for b in &self.cargo {
            b.hash_state(h);
        }
    }
}

/// Wycena przejazdu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FreightQuote {
    pub minutes: u32,
    pub cost: Money,
    pub distance_m: u32,
    /// Część kosztu płacona **raz za pojazd**, nie za kilometr: podstawienie, dokumenty,
    /// czas kierowcy przy załadunku.
    ///
    /// Wyodrębniona, bo bez tego konsolidacja dostaw nie ma czego oszczędzić —
    /// milk-run po dziesięciu sklepach płaciłby dziesięć razy za podstawienie tej samej
    /// ciężarówki, czyli dokładnie tyle, ile dziesięć osobnych kursów (`AL-2`).
    pub call_out: Money,
}

/// Gniazdo dla routera M4 — **punkt podmiany**, nie abstrakcja na zapas.
///
/// Dwie implementacje istnieją od pierwszego dnia: [`FlatRateFreight`] (testowa
/// i scenariuszowa) oraz ta, która przyjdzie z M4, gdy ciężarówki wejdą do
/// `data/vehicles/classes.ron`. Ten sam wzorzec, którym `sim/agents` bierze podróże
/// z `sim/traffic`, i z tego samego powodu: `sim/supply` jest crate'em liściastym
/// i ma nim zostać — zależność od `magnat-traffic` przeciągnęłaby cały ruch do
/// `sim/economy`, czyli odwróciłaby dokładnie ten argument, dla którego M6a wyprowadził
/// katalog z `sim/world`.
pub trait FreightOracle: Send + Sync {
    /// `None`, jeśli przejazd jest **niewykonalny** — tonaż mostu, zakaz ruchu ciężkiego,
    /// brak drogi. Rozstrzygane przy **planowaniu**, nie w połowie trasy (§6.2): dzięki
    /// temu uzupełnianie zapasu wie od razu, że musi szukać innego dostawcy, zamiast
    /// wysyłać ciężarówkę w ślepy zaułek.
    fn quote(
        &self,
        from: SiteId,
        to: SiteId,
        mass: Mass,
        req: &VehicleRequirements,
    ) -> Option<FreightQuote>;
}

/// Atrapa: stała odległość i stawka tonokilometrowa z `data/tuning/supply.ron`.
///
/// `ponytail:` jedna odległość dla całego miasta. Sufit nazwany i jest nim brak
/// ciężarówek po stronie M4 — atrapa ma pozwolić przetestować **maszynę stanów
/// zlecenia**, a nie udawać sieć drogową. Prawdziwe odległości przychodzą razem
/// z routerem.
pub struct FlatRateFreight {
    pub km: u32,
    pub tuning: TransportTuning,
    /// Pary zakładów, między którymi przejazd jest niewykonalny — do testu ścieżki
    /// „trasa zwraca `None` już na etapie planowania".
    pub blocked: Vec<(u32, u32)>,
}

impl FreightOracle for FlatRateFreight {
    fn quote(
        &self,
        from: SiteId,
        to: SiteId,
        mass: Mass,
        _req: &VehicleRequirements,
    ) -> Option<FreightQuote> {
        let para = (from.entity().index(), to.entity().index());
        if self.blocked.contains(&para) {
            return None;
        }
        Some(FreightQuote {
            minutes: self.km * self.tuning.minutes_per_km + self.tuning.load_fixed_minutes,
            cost: self.tuning.haul_cost(mass, self.km),
            distance_m: self.km * 1_000,
            // Atrapa nie różnicuje podstawienia od kilometrów — ma jedną stawkę
            // tonokilometrową i nic w niej nie jest stałe.
            call_out: Money::ZERO,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TransportError {
    UnknownOrder,
    AlreadyFinal,
    NoRoute,
    NotEnoughStock,
}

/// Wszystkie zlecenia miasta.
///
/// `BTreeMap`, nie `Vec` z dziurami: zlecenia zamknięte trzeba usuwać (12 tys. nowych
/// na dobę gry), a usuwanie z `Vec` przenumerowałoby uchwyty, których partie w drodze
/// się trzymają. Iteracja po `BTreeMap` jest deterministyczna, po `HashMap` nie byłaby
/// (00 §3.2).
#[derive(Default)]
pub struct Transport {
    orders: BTreeMap<u32, TransportOrder>,
    next_id: u32,
}

impl Transport {
    #[must_use]
    pub fn new() -> Transport {
        Transport::default()
    }

    #[must_use]
    pub fn get(&self, id: TransportOrderId) -> Option<&TransportOrder> {
        self.orders.get(&id.0)
    }

    pub fn get_mut(&mut self, id: TransportOrderId) -> Option<&mut TransportOrder> {
        self.orders.get_mut(&id.0)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.orders.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.orders.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &TransportOrder> {
        self.orders.values()
    }

    /// Nowe zlecenie. Wycena przez [`FreightOracle`]; `None` z wyceny znaczy
    /// **niewykonalne** i zlecenie powstaje od razu jako `Failed(RouteBlocked)` —
    /// tak, żeby uzupełnianie zapasu zobaczyło porażkę natychmiast, a nie po dobie.
    pub fn order(
        &mut self,
        oracle: &dyn FreightOracle,
        req: TransportRequest,
        reason: DecisionReason,
    ) -> TransportOrderId {
        let id = TransportOrderId(self.next_id);
        self.next_id += 1;
        let wycena = oracle.quote(req.from, req.to, req.mass, &req.requires);
        let (state, price) = match wycena {
            Some(q) => (TransportOrderState::Draft, q.cost),
            None => (
                TransportOrderState::Failed(FailReason::RouteBlocked),
                Money::ZERO,
            ),
        };
        self.orders.insert(
            id.0,
            TransportOrder {
                id,
                from: req.from,
                to: req.to,
                from_slot: req.from_slot,
                to_slot: req.to_slot,
                good: req.good,
                mass: req.mass,
                requires: req.requires,
                ready_at: req.ready_at,
                due_at: req.due_at,
                carrier: Carrier::Unassigned,
                price,
                state,
                reason,
                cargo: Vec::new(),
            },
        );
        id
    }

    /// Ładuje towar i wysyła zlecenie w drogę. To **jedyne** miejsce, w którym partia
    /// opuszcza zakład nadawcy.
    pub fn dispatch(
        &mut self,
        oracle: &dyn FreightOracle,
        store: &mut Store,
        id: TransportOrderId,
        carrier: Carrier,
        now: SimMinute,
    ) -> Result<SimMinute, TransportError> {
        let o = self
            .orders
            .get_mut(&id.0)
            .ok_or(TransportError::UnknownOrder)?;
        if o.state.is_final() {
            return Err(TransportError::AlreadyFinal);
        }
        let q = oracle
            .quote(o.from, o.to, o.mass, &o.requires)
            .ok_or(TransportError::NoRoute)?;
        let Some(cargo) = store.load(o.from_slot, o.good, o.mass, Q::MIN, id) else {
            o.state = TransportOrderState::Failed(FailReason::Refused);
            return Err(TransportError::NotEnoughStock);
        };
        let eta = SimMinute(now.0 + u64::from(q.minutes));
        o.cargo = cargo;
        o.carrier = carrier;
        o.price = q.cost;
        o.state = TransportOrderState::EnRoute { eta };
        Ok(eta)
    }

    /// Wysyła w drogę całą trasę objazdową: jeden pojazd, kilka punktów rozładunku.
    ///
    /// Różnica wobec [`Transport::dispatch`] jest w cenie i w czasie, nie w ładowaniu:
    /// koszt trasy dzieli się między zlecenia **proporcjonalnie do masy** (00 §2 —
    /// podział sumuje się do oryginału co do grosza), a `eta` każdego przystanku jest
    /// czasem narastającym, bo ostatni sklep czeka dłużej niż pierwszy. To jest cała
    /// cena, którą się płaci za tańszą dostawę, i ma być widoczna.
    pub fn dispatch_run(
        &mut self,
        store: &mut Store,
        run: &MilkRun,
        carrier: Carrier,
        now: SimMinute,
    ) -> Result<(), TransportError> {
        let wagi: Vec<u64> = run
            .stops
            .iter()
            .map(|id| self.orders.get(&id.0).map_or(0, |o| o.mass.0.max(0) as u64))
            .collect();
        if wagi.is_empty() {
            return Err(TransportError::UnknownOrder);
        }
        let udzialy = magnat_core::money::split_proportional(run.cost, &wagi);
        let na_przystanek = run.minutes / (run.stops.len() as u32).max(1);

        for (i, id) in run.stops.iter().enumerate() {
            let o = self
                .orders
                .get_mut(&id.0)
                .ok_or(TransportError::UnknownOrder)?;
            if o.state.is_final() {
                return Err(TransportError::AlreadyFinal);
            }
            let Some(cargo) = store.load(o.from_slot, o.good, o.mass, Q::MIN, *id) else {
                o.state = TransportOrderState::Failed(FailReason::Refused);
                return Err(TransportError::NotEnoughStock);
            };
            o.cargo = cargo;
            o.carrier = carrier;
            o.price = udzialy[i];
            o.state = TransportOrderState::EnRoute {
                eta: SimMinute(now.0 + u64::from(na_przystanek * (i as u32 + 1))),
            };
        }
        Ok(())
    }

    /// Rozładunek u odbiorcy. Zwraca masę, która **nie** zmieściła się w magazynie —
    /// zero znaczy dostawę kompletną.
    pub fn deliver(
        &mut self,
        cat: &Catalog,
        store: &mut Store,
        id: TransportOrderId,
        _now: SimMinute,
    ) -> Result<Mass, TransportError> {
        let o = self
            .orders
            .get_mut(&id.0)
            .ok_or(TransportError::UnknownOrder)?;
        if o.state.is_final() {
            return Err(TransportError::AlreadyFinal);
        }
        let odrzucone = store
            .unload(cat, id, &o.cargo, o.to_slot)
            .map_err(|_| TransportError::UnknownOrder)?;
        let nieprzyjete = store.in_transit_mass(&odrzucone);
        o.cargo.retain(|b| odrzucone.contains(b));
        o.state = if nieprzyjete.0 == 0 {
            TransportOrderState::Done
        } else {
            TransportOrderState::Failed(FailReason::Refused)
        };
        Ok(nieprzyjete)
    }

    /// Usuwa zlecenia zamknięte **bez ładunku**. Zlecenie, którego towar nie został
    /// przyjęty, zostaje — inaczej partie w drodze straciłyby jedyny uchwyt, którym
    /// da się je rozładować, i towar zawisłby w arenie na zawsze.
    pub fn prune(&mut self) -> usize {
        let przed = self.orders.len();
        self.orders
            .retain(|_, o| !o.state.is_final() || !o.cargo.is_empty());
        przed - self.orders.len()
    }
}

impl HashState for Transport {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.next_id);
        h.write_u32(self.orders.len() as u32);
        for (k, o) in &self.orders {
            h.write_u32(*k);
            o.hash_state(h);
        }
    }
}

/// Zamówienie przewozu — to, co wie wołający, zanim powstanie zlecenie.
#[derive(Clone, Copy, Debug)]
pub struct TransportRequest {
    pub from: SiteId,
    pub to: SiteId,
    pub from_slot: SlotId,
    pub to_slot: SlotId,
    pub good: GoodId,
    pub mass: Mass,
    pub requires: VehicleRequirements,
    pub ready_at: SimMinute,
    pub due_at: SimMinute,
}
