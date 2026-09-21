//! Pojazd jako encja ECS (M4b §5.2, WP5).
//!
//! Pojazd jest encją, a nie wpisem w arenie — i to jest różnica wobec `K-16`
//! (partie i oferty). Powód jest ten sam, tylko odwrócony: pojazdów jest rzędu
//! dziesiątek tysięcy, nie setek, ich rotacja jest **znikoma** (auto żyje latami),
//! a przekrojowe zapytania po archetypie są dokładnie tym, czego potrzebują fazy
//! następne: M7 liczy majątek gospodarstw, M8 nalicza podatek od pojazdów.
//!
//! **`VehicleLocation` jest jedynym źródłem prawdy o tym, gdzie auto jest** (ryzyko R6).
//! Przejście stanów idzie przez jedną funkcję, żeby „pojazd widmo" — zaparkowany
//! i jadący naraz — był niemożliwy strukturalnie, a nie przez dyscyplinę.

use crate::spec::{FuelKind, VehicleClassId, VehicleClassSpec};
use magnat_core::{HashState, Money, PlaceRef, SimMinute, StateHasher, Volume, Q};
use magnat_ecs::Component;

/// Właściciel pojazdu.
///
/// `kind` jest spakowany do pary (znacznik, indeks encji), bo `OwnerKind` z §5.2
/// byłby enumem o trzech wariantach z których **dwa nie mają dziś właściciela**:
/// firm nie ma do M7, linii komunikacyjnych do M4c. Znacznik rozstrzyga, który
/// to przypadek, a indeks jest ten sam we wszystkich trzech.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct VehicleOwner {
    /// [`OwnerKind`] jako `u8`.
    pub kind: u8,
    pub _pad: [u8; 3],
    /// Indeks encji właściciela (gospodarstwo, firma, linia).
    pub owner: u32,
    /// Indeks encji kierowcy głównego; [`VehicleOwner::NO_DRIVER`] = brak.
    pub primary_driver: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum OwnerKind {
    Household = 0,
    Firm = 1,
    Transit = 2,
}

impl VehicleOwner {
    pub const NO_DRIVER: u32 = u32::MAX;

    #[must_use]
    pub fn household(owner: u32, driver: u32) -> VehicleOwner {
        VehicleOwner {
            kind: OwnerKind::Household as u8,
            _pad: [0; 3],
            owner,
            primary_driver: driver,
        }
    }

    #[must_use]
    pub const fn has_driver(&self) -> bool {
        self.primary_driver != VehicleOwner::NO_DRIVER
    }
}

/// Stan techniczny i przebieg.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct VehicleCondition {
    /// Zużycie 0..=100; 100 = wrak.
    pub wear: u8,
    pub _pad: [u8; 3],
    /// `u32::MAX` = pojazd sprawny. Minuta świata, do której stoi unieruchomiony.
    pub broken_until: u32,
    pub odometer_cm: u64,
    pub next_service_cm: u64,
}

impl VehicleCondition {
    pub const HEALTHY: u32 = u32::MAX;
    /// Przegląd co 15 tys. km.
    pub const SERVICE_INTERVAL_CM: u64 = 150_000_000;

    #[must_use]
    pub fn new() -> VehicleCondition {
        VehicleCondition {
            wear: 0,
            _pad: [0; 3],
            broken_until: VehicleCondition::HEALTHY,
            odometer_cm: 0,
            next_service_cm: VehicleCondition::SERVICE_INTERVAL_CM,
        }
    }

    #[must_use]
    pub fn wear_q(&self) -> Q {
        Q::new(self.wear)
    }

    #[must_use]
    pub fn is_broken(&self, now: SimMinute) -> bool {
        self.broken_until != VehicleCondition::HEALTHY && (now.0 as u32) < self.broken_until
    }
}

impl Default for VehicleCondition {
    fn default() -> VehicleCondition {
        VehicleCondition::new()
    }
}

/// Zbiornik.
///
/// **Jednostką jest mikrolitr** (dla baterii: miliwatogodzina) — ta sama, w której
/// liczy paliwo `settle_edge`, i z tego samego powodu: krawędź metropolii ma ~40 m,
/// a zużycie na niej to 2,5 ml, więc mililitr jest za grubą działką, żeby zamknąć
/// bilans (patrz nagłówek modułu `mezo`). Katalog `data/vehicles/` podaje pojemność
/// w mililitrach, bo tak ją czyta człowiek; przeliczenie jest w [`FuelTank::full`]
/// i jest jedynym miejscem, w którym te dwie jednostki się spotykają.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct FuelTank {
    /// [`FuelKind`] jako `u8`.
    pub kind: u8,
    pub _pad: [u8; 7],
    pub capacity: i64,
    pub level: i64,
    /// Poniżej tej wartości podróż zaczyna się od tankowania.
    pub refuel_threshold: i64,
}

impl FuelTank {
    #[must_use]
    pub fn full(spec: &VehicleClassSpec) -> FuelTank {
        FuelTank {
            kind: spec.fuel as u8,
            _pad: [0; 7],
            capacity: spec.tank_ml * crate::mezo::UL_PER_ML,
            level: spec.tank_ml * crate::mezo::UL_PER_ML,
            refuel_threshold: spec.tank_ml
                * crate::mezo::UL_PER_ML
                * i64::from(spec.refuel_threshold_permille)
                / 1000,
        }
    }

    #[must_use]
    pub fn fuel_kind(&self) -> FuelKind {
        match self.kind {
            1 => FuelKind::Diesel,
            2 => FuelKind::Lpg,
            3 => FuelKind::Electric,
            _ => FuelKind::Petrol,
        }
    }

    /// Stan baku w promilach pojemności — ładunek `DecisionReason::Citizen(CitizenReason::RefuelNeeded)`.
    #[must_use]
    pub fn level_permille(&self) -> u16 {
        if self.capacity <= 0 {
            return 0;
        }
        ((self.level.max(0) * 1000) / self.capacity).min(1000) as u16
    }

    #[must_use]
    pub fn needs_refuel(&self) -> bool {
        self.level < self.refuel_threshold
    }

    /// Poziom jako `Volume`, czyli w mililitrach (00 §2) — wyjście na zewnątrz
    /// dla paliw ciekłych. Zaokrąglenie jest jawne, bo to granica jednostek.
    #[must_use]
    pub fn volume(&self) -> Volume {
        Volume(
            Money(self.level)
                .div_round_half_up(crate::mezo::UL_PER_ML)
                .0,
        )
    }
}

/// Gdzie pojazd jest. **Jedyne** źródło prawdy (R6).
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct VehicleLocation {
    /// [`LocationKind`] jako `u8`.
    pub kind: u8,
    pub _pad: [u8; 3],
    /// `OnEdge`: `EdgeId`. `Depot`: indeks zakładu. W pozostałych przypadkach 0.
    pub edge: u32,
    /// `Parked`: miejsce postoju. W ruchu: miejsce, z którego pojazd wyruszył.
    pub at: PlaceRef,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum LocationKind {
    Parked = 0,
    OnEdge = 1,
    Depot = 2,
}

impl VehicleLocation {
    #[must_use]
    pub fn parked(at: PlaceRef) -> VehicleLocation {
        VehicleLocation {
            kind: LocationKind::Parked as u8,
            _pad: [0; 3],
            edge: 0,
            at,
        }
    }

    #[must_use]
    pub const fn is_parked(&self) -> bool {
        self.kind == LocationKind::Parked as u8
    }

    #[must_use]
    pub const fn is_on_edge(&self) -> bool {
        self.kind == LocationKind::OnEdge as u8
    }

    /// Jedyne przejście „zaparkowany → w ruchu". Osobna funkcja, bo to ona, a nie
    /// zbiór miejsc przypisania pola, ma być tym, czego szuka się przy podejrzeniu
    /// pojazdu widma.
    pub fn depart(&mut self, edge: u32) {
        debug_assert!(
            self.is_parked(),
            "pojazd wyrusza, choć nie jest zaparkowany"
        );
        self.kind = LocationKind::OnEdge as u8;
        self.edge = edge;
    }

    pub fn advance(&mut self, edge: u32) {
        debug_assert!(self.is_on_edge(), "pojazd zmienia krawędź, choć nie jedzie");
        self.edge = edge;
    }

    /// Jedyne przejście „w ruchu → zaparkowany".
    pub fn arrive(&mut self, at: PlaceRef) {
        self.kind = LocationKind::Parked as u8;
        self.edge = 0;
        self.at = at;
    }
}

impl Default for VehicleLocation {
    fn default() -> VehicleLocation {
        VehicleLocation::parked(PlaceRef::default())
    }
}

/// Klasa pojazdu jako komponent — indeks do katalogu `data/vehicles/`.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct VehicleClass {
    pub id: VehicleClassId,
    pub _pad: u16,
}

impl VehicleClass {
    #[must_use]
    pub const fn new(id: VehicleClassId) -> VehicleClass {
        VehicleClass { id, _pad: 0 }
    }
}

// ── hash stanu (00 §3.6) ────────────────────────────────────────────────────────
//
// Pola `_pad` są poza hashem: to bajty wyrównania, nie stan.

impl HashState for VehicleOwner {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(self.kind);
        h.write_u32(self.owner);
        h.write_u32(self.primary_driver);
    }
}

impl HashState for VehicleCondition {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(self.wear);
        h.write_u32(self.broken_until);
        h.write_u64(self.odometer_cm);
        h.write_u64(self.next_service_cm);
    }
}

impl HashState for FuelTank {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(self.kind);
        h.write_u64(self.capacity as u64);
        h.write_u64(self.level as u64);
        h.write_u64(self.refuel_threshold as u64);
    }
}

impl HashState for VehicleLocation {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(self.kind);
        h.write_u32(self.edge);
        crate::hash_place(self.at, h);
    }
}

impl HashState for VehicleClass {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u16(self.id.0);
    }
}

macro_rules! komponent {
    ($($t:ty => $name:literal),+ $(,)?) => {$(
        impl Component for $t {
            const NAME: &'static str = $name;
        }
    )+};
}

komponent! {
    VehicleOwner => "traffic.VehicleOwner",
    VehicleCondition => "traffic.VehicleCondition",
    FuelTank => "traffic.FuelTank",
    VehicleLocation => "traffic.VehicleLocation",
    VehicleClass => "traffic.VehicleClass",
}

/// Suma `size_of` komponentów pojazdu — rachunek pamięci floty (§7.3).
pub const VEHICLE_COMPONENT_BYTES: usize = size_of::<VehicleOwner>()
    + size_of::<VehicleCondition>()
    + size_of::<FuelTank>()
    + size_of::<VehicleLocation>()
    + size_of::<VehicleClass>();

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::VehicleCatalog;

    #[test]
    fn bak_zna_swoj_prog() {
        let cat = VehicleCatalog::load_default().expect("data/vehicles/");
        let id = cat.by_key("car_small").expect("car_small");
        let tank = FuelTank::full(cat.spec(id));
        assert_eq!(tank.level_permille(), 1000);
        // Katalog podaje litry w mililitrach, zbiornik trzyma mikrolitry.
        assert_eq!(tank.capacity, cat.spec(id).tank_ml * 1_000);
        assert_eq!(tank.volume().0, cat.spec(id).tank_ml);
        assert!(!tank.needs_refuel());
        let mut t = tank;
        t.level = t.refuel_threshold - 1;
        assert!(t.needs_refuel());
    }

    #[test]
    fn pojazd_nie_jest_w_dwoch_miejscach() {
        let mut loc = VehicleLocation::parked(PlaceRef::default());
        assert!(loc.is_parked() && !loc.is_on_edge());
        loc.depart(7);
        assert!(!loc.is_parked() && loc.is_on_edge());
        assert_eq!(loc.edge, 7);
        loc.arrive(PlaceRef::default());
        assert!(loc.is_parked() && !loc.is_on_edge());
    }
}
