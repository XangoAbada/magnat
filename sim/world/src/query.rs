//! `TerrainQuery` — **jedyny** interfejs do terenu dla wszystkich faz (00 §K-13, M1 §6.1).
//!
//! Trait, a nie struktura, z konkretnego powodu: ten interfejs będzie rósł po zamknięciu M1.
//! Kolejne fazy zgłaszają braki (M2 zgłosiła pięć, wszystkie przyjęte), a metoda z domyślną
//! implementacją nie łamie niczego u istniejących konsumentów. Kontrakt K-13 mówi, że faza,
//! której czegoś brakuje, **zgłasza to tutaj**, a nie liczy sobie z surowych danych —
//! bo pięć niezależnych modeli ryzyka powodziowego to pięć różnych odpowiedzi na to samo
//! pytanie.
//!
//! Jednostki są kontraktem i nie podlegają lokalnej interpretacji:
//! **współrzędne w metrach**, **wysokość w jednostkach 0,5 m**, **nachylenie 0..=255**.

use crate::climate::ClimateCell;
use crate::data::{RiverNetwork, WaterClass};
use crate::deposit::{Deposit, DepositId};
use crate::geology::ColumnStack;
use magnat_core::{Biome, IRect, IVec2, Q};
use magnat_voxel::MaterialId;

/// Bok kafla wsadowego w metrach — jednostka, w której M2 pobiera wysokości dla L-systemu.
pub const TILE_M: usize = 256;

/// Współrzędna kafla 256 m.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct TileCoord {
    pub x: i32,
    pub y: i32,
}

impl TileCoord {
    #[must_use]
    pub const fn new(x: i32, y: i32) -> TileCoord {
        TileCoord { x, y }
    }

    /// Narożnik kafla w metrach.
    #[must_use]
    pub const fn origin_m(self) -> IVec2 {
        IVec2::new(self.x * TILE_M as i32, self.y * TILE_M as i32)
    }
}

/// Stan wody w punkcie.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct WaterCell {
    pub depth_dm: u16,
    pub class: WaterClass,
    /// Indeks sąsiada D8, do którego spływa woda.
    pub flow_dir: u8,
    pub strahler: u8,
}

/// Klasa spławności — odpowiedź na pytanie „czy tędy przepłynie jednostka o danym zanurzeniu".
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NavigableClass {
    Sea,
    Canal,
    River { min_draft_dm: u16 },
}

/// Przydatność punktu pod zabudowę. Wszystko, czego M2 potrzebuje, żeby policzyć koszt —
/// ale **bez** samego kosztu: cena robót ziemnych to własność epoki i technologii,
/// a nie terenu (M1 §6.1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Buildability {
    pub slope_ok: bool,
    /// Ryzyko powodziowe 0..=100. **Jedno źródło prawdy** — M8 nakłada na nie odchylenie
    /// pogodowe, nie liczy własnego modelu (M1 §6.1).
    pub flood_risk: Q,
    pub bearing_capacity: u8,
    /// Mnożnik kosztu robót ziemnych; M2 przelicza na `Money`.
    pub earthwork_cost_index: u16,
}

/// Sposób pokonania przeszkody między dwoma punktami.
///
/// Zwraca **geometrię i objętość robót**, nie `Money` — przeliczenie na koszt należy do M2,
/// bo zależy od epoki i technologii, a nie od terenu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Crossing {
    Flat,
    Embankment { fill_m3: i64 },
    Bridge { span_m: u32, clearance_m: u16 },
    Tunnel { len_m: u32, rock: MaterialId },
}

/// Maska przeszkód: 1 bit na metr kwadratowy. Generowana **na żądanie dla prostokąta**,
/// nigdy trzymana w całości (M1 §9 D4) — pełna maska mapy 16 km to 32 MB, których nikt
/// nie potrzebuje naraz.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ObstacleBitset {
    pub rect: IRect,
    bits: Vec<u64>,
}

impl ObstacleBitset {
    #[must_use]
    pub fn new(rect: IRect) -> ObstacleBitset {
        let n = (rect.width().max(0) as usize) * (rect.height().max(0) as usize);
        ObstacleBitset {
            rect,
            bits: vec![0; n.div_ceil(64)],
        }
    }

    #[inline]
    fn index(&self, p: IVec2) -> Option<usize> {
        if !self.rect.contains(p) {
            return None;
        }
        let w = self.rect.width() as usize;
        Some((p.y - self.rect.min.y) as usize * w + (p.x - self.rect.min.x) as usize)
    }

    pub fn set(&mut self, p: IVec2, blocked: bool) {
        if let Some(i) = self.index(p) {
            if blocked {
                self.bits[i / 64] |= 1 << (i % 64);
            } else {
                self.bits[i / 64] &= !(1 << (i % 64));
            }
        }
    }

    #[must_use]
    pub fn get(&self, p: IVec2) -> bool {
        self.index(p)
            .is_some_and(|i| self.bits[i / 64] & (1 << (i % 64)) != 0)
    }

    /// Liczba zablokowanych metrów kwadratowych — do szybkiej oceny działki przez M2.
    #[must_use]
    pub fn count_blocked(&self) -> u32 {
        self.bits.iter().map(|w| w.count_ones()).sum()
    }
}

/// Jedyny interfejs do terenu dla wszystkich faz. Czysty, bezalokacyjny poza jawnie
/// alokującymi metodami, wołalny z wielu wątków.
pub trait TerrainQuery: Send + Sync {
    // ── 1. Mapa wysokości ────────────────────────────────────────────────────────────
    /// Wysokość terenu **naturalnego** w jednostkach 0,5 m.
    ///
    /// Naturalnego, czyli deterministycznego z ziarna i zawsze odtwarzalnego (M1 §9 D5).
    /// Nasypy i wykopy M2 żyją w nakładce edycji i mają własne zapytanie — inaczej teren
    /// przestaje być regenerowalny, a zapis gry rośnie o rzędy wielkości.
    fn height_at(&self, x: i32, y: i32) -> i32;

    /// Wysokości całego kafla 256 × 256 m naraz — dla L-systemu dróg M2, który pyta
    /// o dziesiątki tysięcy punktów i nie powinien płacić za wywołanie wirtualne przy każdym.
    fn height_tile(&self, tile: TileCoord, out: &mut [i32; TILE_M * TILE_M]);

    /// Nachylenie 0..=255 ≈ tan(α) × 64.
    fn slope_at(&self, x: i32, y: i32) -> u8;

    fn surface_material_at(&self, x: i32, y: i32) -> MaterialId;

    fn column_at(&self, x: i32, y: i32) -> ColumnStack;

    // ── 2. Spławność i woda ──────────────────────────────────────────────────────────
    fn water_at(&self, x: i32, y: i32) -> WaterCell;

    fn river_network(&self) -> &RiverNetwork;

    fn navigable(&self, x: i32, y: i32) -> Option<NavigableClass>;

    // ── 3. Przeszkody i przydatność pod zabudowę ─────────────────────────────────────
    fn buildability_at(&self, x: i32, y: i32) -> Buildability;

    fn obstacle_mask(&self, rect: IRect) -> ObstacleBitset;

    fn crossing_cost(&self, a: IVec2, b: IVec2) -> Crossing;

    // ── 4. Złoża ─────────────────────────────────────────────────────────────────────
    fn deposits_in(&self, rect: IRect) -> Vec<DepositId>;

    fn deposit(&self, id: DepositId) -> &Deposit;

    fn deposits_at_column(&self, x: i32, y: i32) -> Vec<DepositId>;

    // ── 5. Klimat i biomy ────────────────────────────────────────────────────────────
    fn climate_at(&self, x: i32, y: i32) -> &ClimateCell;

    fn biome_at(&self, x: i32, y: i32) -> Biome;

    fn soil_fertility_at(&self, x: i32, y: i32) -> Q;

    // ── 6. Skróty zgłoszone przez M2 (K-13) ──────────────────────────────────────────
    // Cztery z pięciu to jednolinijkowe akcesory nad danymi, które już istnieją — dlatego
    // mają **domyślne implementacje**: konsument dostaje je za darmo, a implementacja
    // nie musi ich przepisywać.

    /// Głębokość wody w decymetrach. To `water_at(x, y).depth_dm`.
    fn water_depth_at(&self, x: i32, y: i32) -> u16 {
        self.water_at(x, y).depth_dm
    }

    /// Najpłytsze złoże przecinające kolumnę — do strefowania wydobywczego (M2, etap 4).
    /// Pełna lista nadal przez [`TerrainQuery::deposits_at_column`].
    fn deposit_at(&self, x: i32, y: i32) -> Option<DepositId> {
        self.deposits_at_column(x, y)
            .into_iter()
            .min_by_key(|id| self.deposit(*id).depth_top_m)
    }

    /// **Alias `soil_fertility_at`, i tylko dla rolnictwa.**
    ///
    /// „Jakość gleby" znaczy dwie różne rzeczy i M2 musi wiedzieć którą bierze: do uprawy
    /// to żyzność (ta metoda), do fundamentu to [`Buildability::bearing_capacity`], zupełnie
    /// inna wielkość. Nigdy tego samego pola do obu (M1 §6.1).
    fn soil_quality_at(&self, x: i32, y: i32) -> Q {
        self.soil_fertility_at(x, y)
    }

    /// Kierunek wiatru dominującego w stopniach. Kryterium urbanistyczne: przemysł
    /// z podwiatru od mieszkaniówki.
    fn prevailing_wind(&self, x: i32, y: i32) -> u16 {
        self.climate_at(x, y).prevailing_wind_deg
    }

    /// Ryzyko powodziowe — **ta sama wartość** co [`Buildability::flood_risk`].
    /// Jedna liczba, jedno źródło, dwie drogi dostępu.
    fn flood_risk_at(&self, x: i32, y: i32) -> Q {
        self.buildability_at(x, y).flood_risk
    }
}
