//! Parkingi (M4c/WP7, §5.5).
//!
//! Jedna reguła tłumaczy cały moduł: **każdy pojazd, który nie jest na sieci, zajmuje
//! miejsce postojowe**. Nie ma pojazdów widm (ryzyko R6) i nie ma parkowania „gdzieś
//! pod domem" — dlatego niezmiennik testu `parking_no_ghosts` da się sprawdzić jednym
//! porównaniem sum, a nie przeglądem świata.
//!
//! Z tej reguły wynika druga, ta, o którą naprawdę chodzi w PRD §9.4: *brak parkingu
//! przy sklepie zmniejsza jego zasięg dla kierowców*. Rezerwacja miejsca u celu jest
//! **warunkiem wykonalności opcji „samochód"** (`Infeasible::NoParkingWithinRadius`),
//! sprawdzanym **przy planowaniu**, nie w trakcie przejazdu (`M-4`). Podróż, która nie
//! ma jak się skończyć, nigdy nie wyrusza.
//!
//! ## Czego tu nie ma
//!
//! - **Rezerwacji na przyszłe okno czasowe.** Miejsce trzyma się od chwili decyzji do
//!   chwili wyjazdu, bo długość postoju nie jest znana przy planowaniu — plan dnia
//!   mówi, dokąd mieszkaniec jedzie, a nie na jak długo zostanie. `until` w
//!   [`ParkingRegistry::try_reserve`] jest **terminem ważności blokady**, nie końcem
//!   postoju: chroni przed wyciekiem miejsca, gdy podróż nigdy nie dojedzie.
//! - **Taryf miejskich i stref płatnego parkowania** — to polityka, czyli M8. Tutaj
//!   cena jest liczbą na parkingu.
//!
//! Parking przyuliczny jest `ParkingLot` jak każdy inny (§5.5): ta sama ścieżka kodu,
//! ta sama rezerwacja, ten sam licznik. Różni się wyłącznie tym, skąd bierze pojemność
//! — z `RoadEdge.curb_parking`, które policzył `nav_build` z klasy i długości (`M-8`).

use magnat_core::{BuildingId, HashState, Money, SimMinute, StateHasher, WorldCoord};
use magnat_nav::{NodeId, RoadGraph};
use magnat_spatial::{Aabb2, CsrGrid, GridSpec, Vec2};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Pojazd nie stoi na żadnym parkingu — jest na sieci albo jeszcze nie istnieje.
pub const NO_LOT: u32 = u32::MAX;

/// Rodzaj parkingu. Dyskryminanty wchodzą do hasha stanu, więc kolejność jest
/// kontraktem tak samo jak w katalogu klas pojazdów.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum ParkingKind {
    Surface = 0,
    Underground = 1,
    /// Postój przyuliczny — pojemność krawędzi drogowej.
    Curb = 2,
    /// Parking zamknięty przy zakładzie albo biurze. Właściciela (`FirmId`) dopisze
    /// M7; do tego czasu prywatność znaczy tyle, że parking nie jest ogólnodostępny.
    Private = 3,
}

/// Parking: pojemność, cena i punkt dojścia.
///
/// `occupied` liczy **razem** stojące pojazdy i wydane blokady, bo z punktu widzenia
/// kolejnego kierowcy nie ma między nimi różnicy: miejsce jest zajęte albo nie.
#[derive(Clone, Debug)]
pub struct ParkingLot {
    /// Budynek, przy którym stoi parking; `None` = parking samodzielny albo przyuliczny.
    pub building: Option<BuildingId>,
    pub at: WorldCoord,
    pub access_node: NodeId,
    pub capacity: u16,
    pub occupied: u16,
    /// 0 = parking darmowy. Taryfy miejskie dopiero M8.
    pub price_gr_per_hour: Money,
    /// Ile dojścia mieszkaniec zaakceptuje od tego parkingu do celu.
    pub walk_radius_m: u16,
    pub kind: ParkingKind,
}

impl ParkingLot {
    #[must_use]
    pub fn free(&self) -> u16 {
        self.capacity.saturating_sub(self.occupied)
    }

    #[must_use]
    pub fn is_full(&self) -> bool {
        self.occupied >= self.capacity
    }

    /// Obłożenie w promilach — wejście nakładki i raportu.
    #[must_use]
    pub fn occupancy_permille(&self) -> u16 {
        if self.capacity == 0 {
            return 1_000;
        }
        (u32::from(self.occupied) * 1_000 / u32::from(self.capacity)).min(1_000) as u16
    }
}

/// Miejsce przypisane pojazdowi. Uchwyt, nie kopia: zwolnienie idzie przez rejestr.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ParkingSlotRef {
    pub lot: u32,
    /// Dojście od parkingu do celu w minutach — wchodzi do kosztu uogólnionego (§5.3).
    pub walk_minutes: u16,
    pub price_gr_per_hour: Money,
}

/// Dlaczego miejsca nie ma.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParkingDenied {
    /// W promieniu dojścia nie ma żadnego parkingu.
    NoLotInRadius,
    /// Parkingi są, ale wszystkie pełne. `searched` to ilu sprawdzono.
    AllFull { searched: u16 },
}

/// Ile dojścia kierowca akceptuje od parkingu przyulicznego do celu.
///
/// `ponytail:` parkingi biorą się z **krawędzi, nie z budynków**. Sufit nazwany: sklep
/// z własnym placem i sklep bez niego mają dziś ten sam zasięg dla kierowców, o ile
/// stoją przy podobnej ulicy. Ścieżka wyjścia: `BuildingSpec` z M2 dostaje pole
/// `parking_spaces`, a `traffic_build` stawia wtedy `ParkingKind::Surface` obok
/// przyulicznego — rejestr i wyszukiwanie nie drgną, bo dla nich to kolejny wpis.
pub const CURB_WALK_RADIUS_M: u16 = 300;

/// Rozmiar komórki indeksu parkingów w metrach.
const LOT_CELL_M: u16 = 100;

/// Ile najbliższych parkingów sprawdza wyszukiwanie, zanim odda `AllFull`.
///
/// Limit twardy, nie orientacyjny: w szczycie wyszukiwanie woła się ~1 000 razy na
/// minutę gry i to ono, a nie sama jazda, weszłoby w budżet 1,0 ms z §7.3.
pub const MAX_LOTS_SEARCHED: usize = 16;

/// Prędkość dojścia z parkingu do celu w centymetrach na minutę — ta sama, co bazowa
/// prędkość marszu w [`crate::oracle`]. Jedna liczba, bo to ten sam ruch.
const WALK_CM_PER_MIN: i64 = 8_100;

/// Ile godzin postoju zakłada się przy porównaniu opłat między parkingami.
///
/// `ponytail:` stała zamiast długości czynności z planu dnia. Sufit nazwany: parking
/// przy kinie i parking przy biurze są wyceniane tym samym postojem. Ścieżka wyjścia:
/// `TripPurpose` niesie typową długość pobytu i wchodzi tu zamiast stałej — potrzebne
/// dopiero, gdy M8 wprowadzi strefy płatnego parkowania z realną taryfą godzinową.
const ASSUMED_STAY_HOURS: i64 = 2;

/// Rejestr parkingów miasta plus to, gdzie stoi każdy pojazd.
///
/// **To jest stan symulacji, nie dane wejściowe** — obłożenie decyduje o wykonalności
/// podróży autem, a ta o tym, co mieszkaniec zrobi. Wchodzi więc do hasha (00 §3.6)
/// przez [`HashState`].
#[derive(Clone, Debug, Default)]
pub struct ParkingRegistry {
    lots: Vec<ParkingLot>,
    index: Option<CsrGrid<u32>>,
    /// Slot floty → indeks parkingu albo [`NO_LOT`]. Jedyne źródło prawdy o tym,
    /// gdzie stoi pojazd — `VehicleLocation` mówi „przy którym miejscu", ten wektor
    /// „na którym parkingu", i to on domyka bilans.
    of_vehicle: Vec<u32>,
    /// Blokady z terminem ważności: `(minuta wygaśnięcia, slot pojazdu)`.
    holds: BinaryHeap<Reverse<(u64, u32)>>,
    pub searches: u64,
    pub denials: u64,
    pub expired: u64,
}

impl PartialEq for ParkingRegistry {
    /// Kopiec blokad jest wyprowadzalny z przypisań, więc porównanie stanu go pomija —
    /// tak samo jak `TrafficNetwork` pomija kopiec odjazdów.
    fn eq(&self, other: &ParkingRegistry) -> bool {
        self.of_vehicle == other.of_vehicle
            && self.lots.len() == other.lots.len()
            && self
                .lots
                .iter()
                .zip(&other.lots)
                .all(|(a, b)| a.occupied == b.occupied)
    }
}

impl Eq for ParkingRegistry {}

impl ParkingRegistry {
    /// Buduje rejestr: parkingi podane jawnie plus postój przyuliczny wyprowadzony
    /// z krawędzi warstwy drogowej.
    ///
    /// Krawędź, której `nav_build` nie przyznał ani jednego miejsca (arteria,
    /// odcinek bezkolizyjny), nie tworzy parkingu — a nie parking o pojemności 0,
    /// bo pusty wpis kosztowałby pamięć i czas wyszukiwania, nie dając niczego.
    #[must_use]
    pub fn build(road: &RoadGraph, mut lots: Vec<ParkingLot>, fleet: usize) -> ParkingRegistry {
        for (i, e) in road.edges.iter().enumerate() {
            if e.curb_parking == 0 {
                continue;
            }
            // Krawędzie jezdni dwukierunkowej mają ten sam krawężnik policzony dwa
            // razy (`nav_build` daje obu kierunkom tę samą liczbę). Bierzemy jedną
            // z pary — tę o mniejszym indeksie — żeby miasto nie miało dwa razy
            // więcej miejsc, niż ma krawężnika.
            if e.twin.is_some_and(|t| (t.0 as usize) < i) {
                continue;
            }
            let n = &road.nodes[e.to.0 as usize];
            lots.push(ParkingLot {
                building: None,
                at: WorldCoord::new(n.pos_cm.x, n.pos_cm.y, n.z_cm),
                access_node: e.to,
                capacity: e.curb_parking,
                occupied: 0,
                price_gr_per_hour: Money::ZERO,
                walk_radius_m: CURB_WALK_RADIUS_M,
                kind: ParkingKind::Curb,
            });
        }
        let index = build_index(&lots);
        ParkingRegistry {
            lots,
            index,
            of_vehicle: vec![NO_LOT; fleet],
            ..ParkingRegistry::default()
        }
    }

    /// Dokłada parking prywatny w podanym punkcie — podjazd przy domu, plac przy
    /// zakładzie. Wołane przy budowie świata; indeks przestrzenny przelicza się od nowa.
    ///
    /// `ponytail:` przebudowa indeksu na każdy dokładany parking. Sufit nazwany
    /// i policzony: przy `n` parkingach i `m` dokładanych kosztuje to `O(n · m)`,
    /// czyli dla metropolii ~15 tys. × ~4 tys. — sekundy **raz, przy generacji świata**,
    /// nigdy w pętli doby. Ścieżka wyjścia: zebrać wszystkie prywatne przed budową
    /// indeksu, gdy to zacznie boleć.
    pub fn add_private(&mut self, at: WorldCoord, capacity: u16) {
        self.lots.push(ParkingLot {
            building: None,
            at,
            access_node: NodeId(0),
            capacity,
            occupied: 0,
            price_gr_per_hour: Money::ZERO,
            walk_radius_m: CURB_WALK_RADIUS_M,
            // Podjazd jest **ogólnodostępny dla właściciela** i tylko dla niego, ale
            // rejestr nie zna właścicieli. `Surface` zamiast `Private`, bo `Private`
            // jest pomijany w wyszukiwaniu i miejsce byłoby nieosiągalne także dla tego,
            // dla kogo powstało.
            kind: ParkingKind::Surface,
        });
        self.index = build_index(&self.lots);
    }

    /// Dokłada miejsca w tablicy pojazdów, gdy flota urosła po zbudowaniu rejestru.
    pub fn resize_fleet(&mut self, fleet: usize) {
        if self.of_vehicle.len() < fleet {
            self.of_vehicle.resize(fleet, NO_LOT);
        }
    }

    #[must_use]
    pub fn lots(&self) -> &[ParkingLot] {
        &self.lots
    }

    /// Parkingi do poprawienia **przy budowie świata** — cennik i promienie nadaje
    /// `sim/world`, bo to ono zna klasę ulicy i politykę miasta. Pojemności i obłożenia
    /// nie wolno tędy ruszać: to stan, a nie dane, i pilnuje go niezmiennik bilansu.
    pub fn lots_mut(&mut self) -> &mut [ParkingLot] {
        &mut self.lots
    }

    #[must_use]
    pub fn fleet_len(&self) -> usize {
        self.of_vehicle.len()
    }

    /// Na którym parkingu stoi pojazd; [`NO_LOT`] = na sieci.
    #[must_use]
    pub fn lot_of(&self, vehicle: u32) -> u32 {
        self.of_vehicle
            .get(vehicle as usize)
            .copied()
            .unwrap_or(NO_LOT)
    }

    /// Ile pojazdów stoi na parkingach — lewa strona niezmiennika `parking_no_ghosts`.
    #[must_use]
    pub fn parked(&self) -> u64 {
        self.of_vehicle.iter().filter(|l| **l != NO_LOT).count() as u64
    }

    /// Suma obłożeń wszystkich parkingów. Musi być równa [`Self::parked`] — jeśli nie
    /// jest, gdzieś powstało miejsce z niczego albo pojazd stoi w dwóch naraz.
    #[must_use]
    pub fn occupied_total(&self) -> u64 {
        self.lots.iter().map(|l| u64::from(l.occupied)).sum()
    }

    #[must_use]
    pub fn free_total(&self) -> u64 {
        self.lots.iter().map(|l| u64::from(l.free())).sum()
    }

    /// Rezerwuje wskazany parking dla pojazdu (`try_reserve` z M4 §6). `until` to
    /// **termin ważności blokady**, nie koniec postoju: po nim miejsce wraca do puli,
    /// jeśli pojazd nigdy nie dojechał.
    ///
    /// Pojazd, który już gdzieś stoi, najpierw zwalnia poprzednie miejsce — inaczej
    /// jedno auto zajmowałoby dwa i bilans przestałby się domykać.
    pub fn try_reserve(
        &mut self,
        lot: u32,
        vehicle: u32,
        until: SimMinute,
    ) -> Result<ParkingSlotRef, ParkingDenied> {
        let Some(l) = self.lots.get_mut(lot as usize) else {
            return Err(ParkingDenied::NoLotInRadius);
        };
        if l.is_full() {
            return Err(ParkingDenied::AllFull { searched: 1 });
        }
        let cena = l.price_gr_per_hour;
        l.occupied += 1;
        self.release(vehicle);
        if let Some(v) = self.of_vehicle.get_mut(vehicle as usize) {
            *v = lot;
        }
        self.holds.push(Reverse((until.0, vehicle)));
        Ok(ParkingSlotRef {
            lot,
            walk_minutes: 0,
            price_gr_per_hour: cena,
        })
    }

    /// Szuka miejsca w promieniu dojścia od celu i od razu je rezerwuje.
    ///
    /// Kandydaci są sortowani po `(koszt dojścia + opłata, indeks parkingu)` — klucz
    /// totalny, więc wynik nie zależy od układu komórek indeksu przestrzennego (§5.5).
    /// `vot_gr_per_min` to wartość czasu mieszkańca: bez niej „bliżej" i „taniej" nie
    /// dałyby się porównać w jednej liczbie, a porównywanie ich wagami bez jednostki
    /// jest dokładnie tym, czego §5.3 zakazuje.
    pub fn find_and_reserve(
        &mut self,
        dest: WorldCoord,
        radius_m: u16,
        vehicle: u32,
        vot_gr_per_min: i64,
        until: SimMinute,
    ) -> Result<ParkingSlotRef, ParkingDenied> {
        self.searches += 1;
        let Some(index) = self.index.as_ref() else {
            self.denials += 1;
            return Err(ParkingDenied::NoLotInRadius);
        };
        let cel = Vec2::new(dest.x as f32 / 100.0, dest.y as f32 / 100.0);
        let mut kandydaci: Vec<(i64, u32)> = Vec::with_capacity(MAX_LOTS_SEARCHED);
        index.for_each_in_radius(cel, f32::from(radius_m), |_, i| {
            let l = &self.lots[i as usize];
            if l.is_full() || l.kind == ParkingKind::Private {
                return;
            }
            // Parking deklaruje **własny** promień dojścia i on jest wiążący: dalej
            // niż on nikt z niego nie pójdzie, choćby geometrycznie mieścił się
            // w promieniu zapytania. Podziemny garaż w centrum i krawężnik na
            // przedmieściu mają inną cierpliwość.
            if walk_cm(l.at, dest) > i64::from(l.walk_radius_m) * 100 {
                return;
            }
            let dojscie = walk_minutes(l.at, dest);
            let koszt =
                i64::from(dojscie) * vot_gr_per_min + l.price_gr_per_hour.0 * ASSUMED_STAY_HOURS;
            kandydaci.push((koszt, i));
        });
        if kandydaci.is_empty() {
            self.denials += 1;
            return Err(if self.lots.is_empty() {
                ParkingDenied::NoLotInRadius
            } else {
                ParkingDenied::AllFull { searched: 0 }
            });
        }
        kandydaci.sort_unstable();
        kandydaci.truncate(MAX_LOTS_SEARCHED);
        let (_, lot) = kandydaci[0];
        let dojscie = walk_minutes(self.lots[lot as usize].at, dest);
        match self.try_reserve(lot, vehicle, until) {
            Ok(mut s) => {
                s.walk_minutes = dojscie;
                Ok(s)
            }
            Err(_) => {
                self.denials += 1;
                Err(ParkingDenied::AllFull {
                    searched: kandydaci.len().min(u16::MAX as usize) as u16,
                })
            }
        }
    }

    /// Czy w promieniu jest wolne miejsce — pytanie bez rezerwacji, dla wyceny opcji.
    #[must_use]
    pub fn any_free_within(&self, dest: WorldCoord, radius_m: u16) -> bool {
        let Some(index) = self.index.as_ref() else {
            return false;
        };
        let cel = Vec2::new(dest.x as f32 / 100.0, dest.y as f32 / 100.0);
        let mut wolne = false;
        index.for_each_in_radius(cel, f32::from(radius_m), |_, i| {
            let l = &self.lots[i as usize];
            wolne |= !l.is_full() && l.kind != ParkingKind::Private;
        });
        wolne
    }

    /// Zwalnia miejsce pojazdu. Wołane przy wyruszeniu w podróż — pojazd na sieci
    /// nie zajmuje parkingu.
    pub fn release(&mut self, vehicle: u32) {
        let Some(v) = self.of_vehicle.get_mut(vehicle as usize) else {
            return;
        };
        let lot = std::mem::replace(v, NO_LOT);
        if lot == NO_LOT {
            return;
        }
        if let Some(l) = self.lots.get_mut(lot as usize) {
            l.occupied = l.occupied.saturating_sub(1);
        }
    }

    /// Zwalnia blokady, których termin minął, a pojazd nigdy nie dojechał.
    ///
    /// Zawór, nie ścieżka główna: `TrafficSystem` zwalnia miejsce przy każdym
    /// `Arrived` i `Failed`, więc tu trafia tylko to, co przepadło inaczej. Licznik
    /// `expired` ma krzyknąć, gdyby ścieżka główna przestała działać.
    pub fn expire(&mut self, now: SimMinute) {
        while let Some(Reverse((t, v))) = self.holds.peek().copied() {
            if t > now.0 {
                break;
            }
            self.holds.pop();
            // Blokada wygasa tylko wtedy, gdy pojazd nadal stoi tam, gdzie ją wydano.
            // Późniejsza rezerwacja tego samego pojazdu wstawiła własny wpis.
            if self.lot_of(v) != NO_LOT && self.holds.iter().all(|Reverse((_, w))| *w != v) {
                self.release(v);
                self.expired += 1;
            }
        }
    }
}

impl HashState for ParkingRegistry {
    /// Obłożenie w kolejności indeksów parkingów i przypisania w kolejności slotów
    /// floty — oba porządki są totalne i niezależne od historii (00 §3.2).
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.lots.len() as u32);
        for l in &self.lots {
            h.write_u16(l.occupied);
            h.write_u16(l.capacity);
        }
        for v in &self.of_vehicle {
            h.write_u32(*v);
        }
    }
}

/// Droga z parkingu do celu w centymetrach. Manhattan, nie linia prosta: od parkingu
/// do drzwi idzie się chodnikiem wzdłuż kwartału, a nie przez budynki.
#[inline]
#[must_use]
fn walk_cm(from: WorldCoord, to: WorldCoord) -> i64 {
    i64::from((to.x - from.x).abs()) + i64::from((to.y - from.y).abs())
}

/// Czas dojścia z parkingu do celu, zaokrąglony w górę — dojście trwające 3,7 minuty
/// kończy się w czwartej. Promień parkingu porównuje się jednak z **odległością**,
/// nie z tą liczbą: inaczej zaokrąglenie odcinałoby ostatnie kilkadziesiąt metrów
/// każdego promienia.
#[must_use]
pub fn walk_minutes(from: WorldCoord, to: WorldCoord) -> u16 {
    let d = walk_cm(from, to);
    ((d + WALK_CM_PER_MIN - 1) / WALK_CM_PER_MIN).clamp(0, i64::from(u16::MAX)) as u16
}

fn build_index(lots: &[ParkingLot]) -> Option<CsrGrid<u32>> {
    if lots.is_empty() {
        return None;
    }
    let mut min = Vec2::new(f32::MAX, f32::MAX);
    let mut max = Vec2::new(f32::MIN, f32::MIN);
    for l in lots {
        let v = Vec2::new(l.at.x as f32 / 100.0, l.at.y as f32 / 100.0);
        min = Vec2::new(min.x.min(v.x), min.y.min(v.y));
        max = Vec2::new(max.x.max(v.x), max.y.max(v.y));
    }
    let spec = GridSpec::covering(Aabb2::new(min, max), LOT_CELL_M);
    Some(CsrGrid::build(
        spec,
        lots.iter().enumerate().map(|(i, l)| {
            (
                Vec2::new(l.at.x as f32 / 100.0, l.at.y as f32 / 100.0),
                i as u32,
            )
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parking(at_m: i32, capacity: u16) -> ParkingLot {
        ParkingLot {
            building: None,
            at: WorldCoord::new(at_m * 100, 0, 0),
            access_node: NodeId(0),
            capacity,
            occupied: 0,
            price_gr_per_hour: Money::ZERO,
            walk_radius_m: CURB_WALK_RADIUS_M,
            kind: ParkingKind::Surface,
        }
    }

    fn rejestr(lots: Vec<ParkingLot>, fleet: usize) -> ParkingRegistry {
        let index = build_index(&lots);
        ParkingRegistry {
            lots,
            index,
            of_vehicle: vec![NO_LOT; fleet],
            ..ParkingRegistry::default()
        }
    }

    #[test]
    fn przepelniony_parking_odmawia() {
        let mut r = rejestr(vec![parking(0, 2)], 4);
        let cel = WorldCoord::new(0, 0, 0);
        for v in 0..2 {
            assert!(r.find_and_reserve(cel, 200, v, 10, SimMinute(100)).is_ok());
        }
        let odmowa = r.find_and_reserve(cel, 200, 2, 10, SimMinute(100));
        assert_eq!(odmowa, Err(ParkingDenied::AllFull { searched: 0 }));
        assert_eq!(r.parked(), 2);
        assert_eq!(
            r.occupied_total(),
            2,
            "obłożenie rozjechało się z przypisaniem"
        );
    }

    #[test]
    fn blizszy_parking_wygrywa_a_remis_rozstrzyga_indeks() {
        // Dwa parkingi: 400 m i 50 m od celu. Wartość czasu dodatnia, obie darmowe.
        let mut r = rejestr(vec![parking(400, 5), parking(50, 5)], 2);
        let cel = WorldCoord::new(0, 0, 0);
        let s = r
            .find_and_reserve(cel, 1_000, 0, 20, SimMinute(100))
            .expect("miejsce");
        assert_eq!(s.lot, 1, "wybrano dalszy parking");
        assert!(s.walk_minutes >= 1);

        // Ten sam koszt (oba w tym samym punkcie) → wygrywa mniejszy indeks.
        let mut r2 = rejestr(vec![parking(10, 5), parking(10, 5)], 2);
        let s2 = r2
            .find_and_reserve(cel, 1_000, 0, 20, SimMinute(100))
            .expect("miejsce");
        assert_eq!(s2.lot, 0, "remis rozstrzygnięty inaczej niż indeksem");
    }

    #[test]
    fn drozszy_parking_przegrywa_z_dalszym_gdy_czas_jest_tani() {
        let mut platny = parking(10, 5);
        platny.price_gr_per_hour = Money(1_000); // 10 zł/h × 2 h = 2000 gr
        let daleki = parking(300, 5); // ~4 min dojścia × 5 gr/min = 20 gr
        let mut r = rejestr(vec![platny, daleki], 2);
        let s = r
            .find_and_reserve(WorldCoord::new(0, 0, 0), 1_000, 0, 5, SimMinute(100))
            .expect("miejsce");
        assert_eq!(
            s.lot, 1,
            "kierowca zapłacił 20 zł, żeby oszczędzić cztery minuty"
        );
    }

    #[test]
    fn pojazd_nie_zajmuje_dwoch_miejsc() {
        let mut r = rejestr(vec![parking(0, 5), parking(1_000, 5)], 2);
        r.try_reserve(0, 0, SimMinute(100)).expect("pierwsze");
        r.try_reserve(1, 0, SimMinute(200)).expect("drugie");
        assert_eq!(r.parked(), 1);
        assert_eq!(r.occupied_total(), 1);
        assert_eq!(r.lot_of(0), 1);
        assert_eq!(
            r.lots()[0].occupied,
            0,
            "pierwsze miejsce nie wróciło do puli"
        );
    }

    #[test]
    fn blokada_bez_przyjazdu_wygasa() {
        let mut r = rejestr(vec![parking(0, 1)], 2);
        r.try_reserve(0, 0, SimMinute(30)).expect("blokada");
        r.expire(SimMinute(29));
        assert_eq!(r.occupied_total(), 1, "blokada wygasła przed terminem");
        r.expire(SimMinute(30));
        assert_eq!(r.occupied_total(), 0, "miejsce wyciekło");
        assert_eq!(r.expired, 1);
        assert!(r.try_reserve(0, 1, SimMinute(60)).is_ok());
    }

    #[test]
    fn przyulicznych_miejsc_jest_tyle_ile_krawezniku() {
        // Para krawędzi jednej jezdni ma ten sam krawężnik policzony dwa razy —
        // rejestr bierze go raz.
        use magnat_core::{DistrictId, IVec2, Mass, RoadClass};
        use magnat_nav::{EdgeSpec, GeomRef, Modality, RoadGraphBuilder};
        let mut b = RoadGraphBuilder::new(Modality::Road);
        b.add_node(IVec2::new(0, 0), 0);
        b.add_node(IVec2::new(10_000, 0), 0);
        b.add_edge_pair(EdgeSpec {
            from: NodeId(0),
            to: NodeId(1),
            geometry_ref: GeomRef(0),
            length_cm: 10_000,
            lanes: 1,
            class: RoadClass::Local,
            speed_limit_dkmh: 300,
            max_mass: Mass::ZERO,
            grade_permille: 0,
            bridge: None,
            curb_parking: 16,
            district: DistrictId(0),
        });
        let g = b.finish();
        let r = ParkingRegistry::build(&g, Vec::new(), 1);
        assert_eq!(r.lots().len(), 1, "krawężnik policzony dwa razy");
        assert_eq!(r.free_total(), 16);
    }
}
