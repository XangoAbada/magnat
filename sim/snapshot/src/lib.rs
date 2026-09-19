//! `magnat-sim-snapshot` — jedyny kanał z symulacji do renderu (00 §1, M11 §6.3).
//!
//! Crate zawiera **wyłącznie typy POD i funkcje czyste**: zero zależności od `sim/*`,
//! zero `&mut World`, zero metod zmieniających świat. To nie jest higiena, tylko
//! egzekucja zasady z 00 §4 — „render czyta snapshot i go nie mutuje" — grafem
//! zależności cargo zamiast regulaminem. `engine/render` i `engine/audio` mają w drzewie
//! ten jeden crate i żadnego innego `sim/*`; pilnuje tego `cargo tree` w CI.
//!
//! ### Co się zmieniło w M11a i dlaczego
//!
//! Do M11a `RenderSnapshot` był `Copy` i mieścił się na stosie, bo niósł cztery pola
//! i tablicę świateł. Kontrakt M11 §5.2 dokłada do niego **1,30 MB rekordów encji**,
//! a `Copy` na takiej strukturze znaczy kopiowanie megabajta przy każdym przekazaniu
//! i przepełnienie stosu przy pierwszej konstrukcji. Wymaganiem nigdy nie było `Copy` —
//! wymaganiem było **zero alokacji na publikację**, a to spełnia bufor o **stałej
//! pojemności alokowanej raz** ([`CapVec`]). Publikacja jest zamianą dwóch gotowych
//! buforów ([`SnapshotPair`]), nie budową nowego.
//!
//! Konsekwencja dla `Z-2` z dokumentu fazy („piesi nie idą przez `RenderSnapshot`"):
//! przesłanka tamtego zapisu odpadła razem z `Copy`, więc mieszkaniec wraca do snapshotu
//! jako [`CitizenRenderRec`] z capem 24 576. [`PedestrianRecord`] zostaje — to jest
//! **rekord ruchu**, który zna pozycję i nie zna zawodu; złączenie go z wyglądem robi
//! wypełniacz po stronie gry.
//!
//! ### Czego tu nie ma
//!
//! `fill_render_snapshot(&World, …)`. Sygnatura z §6.1 dokumentu fazy stawiała ją tutaj,
//! ale `World` mieszka w `engine/ecs`, a rekordy wypełniają **trzy różne crate'y**
//! (`sim/agents` — mieszkańcy, `sim/traffic` — pojazdy, `sim/economy` — zakłady), więc
//! jedna funkcja nad `&World` musiałaby zależeć od wszystkich i wciągnąć je do drzewa
//! renderu. Tutaj są typy, capy, [`ViewQuery`] i deterministyczny selektor
//! ([`select_top_k`]); wypełnianie stoi w `magnat_game::view`, czyli tam, gdzie `K-68`
//! postawił stawianie świata.

#![forbid(unsafe_code)]

pub mod appearance;
pub mod records;
pub mod select;

pub use appearance::{Appearance, AppearanceFields, AgeBand, Carry, OutfitTier};
pub use records::{
    CitizenRenderRec, CrowdDensityRec, LightRecord, PedestrianRecord, PlayerViewRec, PowerRec,
    SiteRenderRec, VehicleRecord, VehicleRenderRec, WeatherState, CITIZEN_FLAG_HIGHLIGHTED,
    CITIZEN_FLAG_IN_BUILDING, CITIZEN_FLAG_IN_VEHICLE, CITIZEN_FLAG_PLAYER_OWNED, MAX_DISTRICTS,
    SITE_FAULT, VEHICLE_FLAG_BLINKER, VEHICLE_FLAG_ENGINE_ON, VEHICLE_FLAG_HIGHLIGHTED,
    VEHICLE_FLAG_LIGHTS, VEHICLE_FLAG_PLAYER_OWNED,
};
pub use select::{dist2_mm, select_top_k, Aabb, Candidate, SnapshotCaps, ViewQuery};

use magnat_core::{SimMinute, Tick};

/// Globalny limit świateł w klatce (M1 §5.8). Cap jest **stały**, nie dynamiczny:
/// przepełnienie obsługuje producent snapshotu, obcinając po malejącym dystansie,
/// a nie pass, który musiałby wtedy alokować w środku klatki.
pub const MAX_LIGHTS: usize = 4096;

/// Tablica o stałej pojemności — odpowiednik `SoaSlice` z planu M1 §6.1.
///
/// Pojemność jest ustalona przy konstrukcji i **nigdy nie rośnie**: [`CapVec::push`]
/// przy komplecie odmawia zamiast realokować. Stąd bierze się gwarancja, na której stoi
/// cały kontrakt — publikacja snapshotu nie alokuje ani razu, bo nie ma z czego.
///
/// Osobny typ zamiast gołego `Vec`, bo `Vec` ma `push`, który realokuje po cichu, a licznik
/// trzymany obok tablicy rozjeżdża się z zawartością dokładnie wtedy, gdy ktoś doda drugie
/// miejsce zapisu.
#[derive(Clone, Debug)]
pub struct CapVec<T> {
    items: Vec<T>,
    cap: usize,
}

impl<T> CapVec<T> {
    /// Bufor o zadanej pojemności. **Jedyna alokacja w życiu tej struktury.**
    #[must_use]
    pub fn with_capacity(cap: usize) -> CapVec<T> {
        CapVec {
            items: Vec::with_capacity(cap),
            cap,
        }
    }

    /// Dopisuje element. Zwraca `false` przy przepełnieniu — **bez paniki i bez cichego
    /// gubienia**: producent ma wtedy obciąć listę po priorytecie, co jest jego decyzją,
    /// nie naszą (M1 §5.8).
    pub fn push(&mut self, v: T) -> bool {
        if self.items.len() >= self.cap {
            return false;
        }
        self.items.push(v);
        true
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.items
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.cap
    }

    /// Ile bajtów zajmuje bufor **rezydentnie**, niezależnie od zapełnienia. To jest
    /// liczba, której pilnuje `snapshot_size_is_constant` (§7.1): rozmiar snapshotu
    /// nie może zależeć od wielkości miasta.
    #[must_use]
    pub fn resident_bytes(&self) -> usize {
        self.cap * core::mem::size_of::<T>()
    }
}

/// Ładunek publikowany z symulacji do renderu.
///
/// Rozmiar **rezydentny** jest stały i niezależny od wielkości miasta: miasto 40 tys.
/// i miasto 400 tys. dają ten sam ≈1,30 MB. To jest ta właściwość, która pozwala M12
/// zamknąć metropolię w budżecie pamięci bez dotykania renderu.
#[derive(Clone, Debug)]
pub struct RenderSnapshot {
    pub tick: Tick,
    /// Pozycja słońca i cykl dobowy wyprowadzają się z tej jednej liczby.
    pub sim_minute: SimMinute,
    /// Cel śledzenia kamery; `None` = kamera swobodna.
    pub camera_hint: Option<[f64; 3]>,
    /// Rośnie przy każdej edycji voxeli — unieważnia cache chunków po stronie renderu.
    pub terrain_revision: u32,
    pub lights: CapVec<LightRecord>,
    pub citizens: CapVec<CitizenRenderRec>,
    pub vehicles: CapVec<VehicleRenderRec>,
    pub sites: CapVec<SiteRenderRec>,
    /// Tłum **poza** capem mieszkańców — gęstość per krawędź grafu pieszego.
    pub crowd: CapVec<CrowdDensityRec>,
    pub weather: WeatherState,
    pub power: [PowerRec; MAX_DISTRICTS],
    /// `DistrictPaletteId` per dzielnica — który zestaw barw obowiązuje w tej dzielnicy
    /// i w epoce, w której powstała.
    ///
    /// Tablicy nie było w §5.2 i musiała dojść: `CitizenRenderRec.district` niesie
    /// `DistrictId`, bo tego potrzebuje `AmbientZone` (M11d), a shader instancji
    /// potrzebuje **palety**. Odwzorowanie jednego na drugie jest wiedzą o mieście
    /// (rodzaj dzielnicy × epoka), której render nie ma i mieć nie może. Osiem bajtów
    /// na dzielnicę zamiast drugiego pola w rekordzie liczonym na dwadzieścia cztery
    /// tysiące sztuk.
    pub district_palette: [u16; MAX_DISTRICTS],
    pub player: PlayerViewRec,
}

impl RenderSnapshot {
    /// Snapshot o capach z `caps`. Alokuje pięć buforów i więcej nie alokuje nigdy.
    #[must_use]
    pub fn new(caps: SnapshotCaps) -> RenderSnapshot {
        RenderSnapshot {
            tick: Tick(0),
            sim_minute: SimMinute(0),
            camera_hint: None,
            terrain_revision: 0,
            lights: CapVec::with_capacity(MAX_LIGHTS),
            citizens: CapVec::with_capacity(caps.citizens as usize),
            vehicles: CapVec::with_capacity(caps.vehicles as usize),
            sites: CapVec::with_capacity(caps.sites as usize),
            crowd: CapVec::with_capacity(caps.crowd as usize),
            weather: WeatherState::default(),
            power: [PowerRec::default(); MAX_DISTRICTS],
            district_palette: [0; MAX_DISTRICTS],
            player: PlayerViewRec::default(),
        }
    }

    /// Zeruje zawartość, **zostawiając pojemności**. To jest pierwszy krok wypełniania.
    ///
    /// Zeruje też pola o stałym rozmiarze — pogodę, zasilanie, paletę dzielnic i gracza.
    /// Przy podwójnym buforowaniu tylny bufor niesie stan **sprzed dwóch klatek**, więc
    /// pole zostawione bez ruszenia nie jest puste, tylko nieświeże: wypełniacz, który
    /// o nim zapomni, opublikuje pogodę z poprzedniej publikacji i nic tego nie pokaże.
    pub fn clear(&mut self) {
        self.lights.clear();
        self.citizens.clear();
        self.vehicles.clear();
        self.sites.clear();
        self.crowd.clear();
        self.weather = WeatherState::default();
        self.power = [PowerRec::default(); MAX_DISTRICTS];
        self.district_palette = [0; MAX_DISTRICTS];
        self.player = PlayerViewRec::default();
    }

    /// Rozmiar rezydentny w bajtach — suma pojemności buforów i pól stałych.
    /// Nie zależy od zapełnienia ani od wielkości miasta (§7.1).
    #[must_use]
    pub fn resident_bytes(&self) -> usize {
        self.lights.resident_bytes()
            + self.citizens.resident_bytes()
            + self.vehicles.resident_bytes()
            + self.sites.resident_bytes()
            + self.crowd.resident_bytes()
            + core::mem::size_of::<WeatherState>()
            + core::mem::size_of::<[PowerRec; MAX_DISTRICTS]>()
            + core::mem::size_of::<[u16; MAX_DISTRICTS]>()
            + core::mem::size_of::<PlayerViewRec>()
    }
}

impl Default for RenderSnapshot {
    fn default() -> Self {
        RenderSnapshot::new(SnapshotCaps::DEFAULT)
    }
}

/// Podwójne buforowanie: jeden bufor wypełnia symulacja, drugi czyta render.
///
/// Zamiana jest [`SnapshotPair::publish`] i nie kopiuje ani bajta — przestawia indeks.
/// Obie strony dostają referencję o rozłącznym czasie życia, więc „render czyta to,
/// co sim właśnie pisze" nie jest stanem, który trzeba wykluczyć dyscypliną.
#[derive(Debug)]
pub struct SnapshotPair {
    bufs: [RenderSnapshot; 2],
    front: usize,
}

impl SnapshotPair {
    #[must_use]
    pub fn new(caps: SnapshotCaps) -> SnapshotPair {
        SnapshotPair {
            bufs: [RenderSnapshot::new(caps), RenderSnapshot::new(caps)],
            front: 0,
        }
    }

    /// Bufor do wypełnienia przez symulację (tylny).
    pub fn back_mut(&mut self) -> &mut RenderSnapshot {
        &mut self.bufs[1 - self.front]
    }

    /// Bufor do odczytu przez render (przedni).
    #[must_use]
    pub fn front(&self) -> &RenderSnapshot {
        &self.bufs[self.front]
    }

    /// Udostępnia właśnie wypełniony bufor renderowi. Zero kopiowania.
    pub fn publish(&mut self) {
        self.front = 1 - self.front;
    }
}

impl Default for SnapshotPair {
    fn default() -> Self {
        SnapshotPair::new(SnapshotCaps::DEFAULT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_nie_panikuje_tylko_odmawia() {
        let mut s: CapVec<LightRecord> = CapVec::with_capacity(4);
        for _ in 0..4 {
            assert!(s.push(LightRecord::default()));
        }
        assert!(!s.push(LightRecord::default()), "przepełnienie przeszło");
        assert_eq!(s.len(), 4);
        assert_eq!(s.capacity(), 4);
        s.clear();
        assert!(s.is_empty());
    }

    /// Cała gwarancja bezalokacyjności stoi na tym, że `push` nie realokuje.
    /// Adres bufora sprzed i po zapełnieniu musi być ten sam.
    #[test]
    fn zapelnienie_nie_realokuje() {
        let mut s: CapVec<u32> = CapVec::with_capacity(64);
        let adres = s.as_slice().as_ptr();
        for i in 0..64 {
            assert!(s.push(i));
        }
        assert!(!s.push(0));
        assert_eq!(s.as_slice().as_ptr(), adres, "bufor się przeprowadził");
    }

    /// `snapshot_size_is_constant` z §7.1: rozmiar rezydentny nie zależy od zapełnienia,
    /// czyli od wielkości miasta. Miasto 40 tys. i 400 tys. dają tę samą liczbę.
    #[test]
    fn rozmiar_rezydentny_nie_zalezy_od_zapelnienia() {
        let mut s = RenderSnapshot::default();
        let pusty = s.resident_bytes();
        for i in 0..SnapshotCaps::DEFAULT.citizens {
            assert!(s.citizens.push(CitizenRenderRec {
                entity_lo: i,
                ..Default::default()
            }));
        }
        for i in 0..SnapshotCaps::DEFAULT.vehicles {
            assert!(s.vehicles.push(VehicleRenderRec {
                entity_lo: i,
                ..Default::default()
            }));
        }
        assert_eq!(s.resident_bytes(), pusty, "rozmiar zmienił się z zawartością");
        // Budżet z §5.2: ≈1,30 MB na bufor, twardy sufit 1,35 MB z §7.1 pkt 7.
        assert!(
            pusty <= 1_350_000,
            "snapshot ma {pusty} B, sufit to 1 350 000"
        );
        assert!(pusty > 1_200_000, "snapshot skurczył się do {pusty} B");
    }

    #[test]
    fn publikacja_zamienia_bufory_bez_kopiowania() {
        let mut p = SnapshotPair::default();
        p.back_mut().tick = Tick(7);
        assert_eq!(p.front().tick, Tick(0), "render widzi bufor przed publikacją");
        p.publish();
        assert_eq!(p.front().tick, Tick(7));
        // Tylny jest teraz tym, który render czytał w poprzedniej klatce.
        assert_eq!(p.back_mut().tick, Tick(0));
    }
}
