//! Trwały stan świata — to, czego **nie** da się odtworzyć taniej niż zapisać.
//!
//! Reszta terenu regeneruje się z `WorldGenParams` (M1 §6.1), więc do zapisu gry idzie
//! tylko to, co jest tutaj, plus rzadka nakładka edycji gracza. Budżet z M1 §5.9 dla
//! mapy 16 km: ≤ 60 MB w RAM, ≤ 20 MB po kompresji.
//!
//! Układ pól jest podporządkowany temu budżetowi i dlatego bywa nieoczywisty:
//! klasa wody i kierunek spływu siedzą **w jednym bajcie**, a głębokość jeziora i dane
//! koryta w rzadkich tablicach bocznych. Powód: siatka 4096² ma 16,8 mln komórek, więc
//! każdy dołożony bajt na komórkę to 16,8 MB — cztery bajty „dla wygody" wysadzają budżet.

use crate::grid::Grid2;
use crate::params::WorldGenParams;
use magnat_core::hash::StateHasher;
use serde::{Deserialize, Serialize};

/// Wysokość terenu w **decymetrach** nad poziomem morza.
///
/// Nie w jednostkach 0,5 m (te są kontraktem `TerrainQuery`, 00 §K-13), bo nachylenie
/// liczone z siatki 4 m przy kroku 0,5 m miałoby ziarnistość 12,5 % — za grubo dla
/// klasyfikacji zboczy i dla upsamplingu. Konwersja na jednostki kontraktu jest
/// w `TerrainQuery::height_at`, w jednym miejscu.
pub type HeightDm = i16;

/// Poziom morza. Stała, nie parametr: przesuwanie go niczego nie dodaje, a każdy
/// przelicznik głębokości musiałby go wtedy nosić.
pub const SEA_LEVEL_DM: HeightDm = 0;

/// Klasa wody w komórce.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize,
)]
#[repr(u8)]
pub enum WaterClass {
    #[default]
    Dry = 0,
    Sea = 1,
    Lake = 2,
    River = 3,
}

impl WaterClass {
    #[must_use]
    pub const fn from_bits(b: u8) -> WaterClass {
        match b & 0b11 {
            1 => WaterClass::Sea,
            2 => WaterClass::Lake,
            3 => WaterClass::River,
            _ => WaterClass::Dry,
        }
    }

    #[must_use]
    pub const fn is_water(self) -> bool {
        !matches!(self, WaterClass::Dry)
    }
}

/// Klasa wody i kierunek spływu w jednym bajcie (patrz nagłówek modułu).
/// bity 0–1: klasa, bity 2–4: indeks sąsiada D8 (0–7), bit 5: komórka bez odbiornika (ujście).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct WaterBits(pub u8);

const SINK_BIT: u8 = 1 << 5;

impl WaterBits {
    #[must_use]
    pub const fn new(class: WaterClass, flow_dir: u8, sink: bool) -> WaterBits {
        WaterBits((class as u8) | ((flow_dir & 0b111) << 2) | if sink { SINK_BIT } else { 0 })
    }

    #[must_use]
    pub const fn class(self) -> WaterClass {
        WaterClass::from_bits(self.0)
    }

    /// Indeks sąsiada w [`crate::grid::NEIGHBORS_8`], do którego spływa komórka.
    #[must_use]
    pub const fn flow_dir(self) -> u8 {
        (self.0 >> 2) & 0b111
    }

    /// Komórka bez odbiornika — morze albo krawędź mapy.
    #[must_use]
    pub const fn is_sink(self) -> bool {
        self.0 & SINK_BIT != 0
    }
}

/// Dane koryta w komórce rzecznej. Rzadkie: rzeki to ułamek procenta komórek mapy,
/// więc trzymanie tego jako pełnej siatki kosztowałoby 100 MB za 99,7 % zer.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RiverCell {
    /// Indeks komórki na siatce roboczej — klucz do wyszukiwania binarnego.
    pub cell: u32,
    pub strahler: u8,
    pub depth_dm: u16,
    pub width_dm: u16,
    /// Rzędna zwierciadła wody w korycie.
    pub surface_dm: HeightDm,
}

/// Komórki jeziorne w układzie „struktura tablic": osobno indeksy, osobno rzędne.
///
/// Wyglądałoby to naturalniej jako `Vec<(u32, i16)>`, ale `u32` wymusza wyrównanie do 4 B,
/// więc para zajmuje 8 B zamiast 6 — dwa bajty wyrzucone na każdą komórkę jeziora.
/// Przy 1,5 mln komórek na mapie 16 km to 2,9 MB, czyli różnica między zmieszczeniem się
/// w budżecie §5.9 a jego przekroczeniem. Tam, gdzie liczba elementów idzie w miliony,
/// wyrównanie przestaje być szczegółem.
#[derive(Clone, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct LakeCells {
    /// Indeksy komórek, rosnąco — klucz wyszukiwania binarnego.
    pub cell: Vec<u32>,
    /// Rzędna zwierciadła, równolegle do `cell`.
    pub surface_dm: Vec<HeightDm>,
}

impl LakeCells {
    pub fn push(&mut self, cell: u32, surface_dm: HeightDm) {
        self.cell.push(cell);
        self.surface_dm.push(surface_dm);
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.cell.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cell.is_empty()
    }

    #[must_use]
    pub fn surface_of(&self, cell: usize) -> Option<HeightDm> {
        self.cell
            .binary_search(&(cell as u32))
            .ok()
            .map(|i| self.surface_dm[i])
    }

    #[must_use]
    pub fn bytes(&self) -> usize {
        self.cell.len() * 4 + self.surface_dm.len() * 2
    }
}

/// Odcinek wektorowej sieci koryt. To **wektor jest źródłem prawdy** o rzece, nie raster:
/// przy materializacji 1 m koryto wcina się z tej polilinii, dzięki czemu zachowuje
/// ciągłość mimo że erozja liczona jest na 4 m (M1 §R2).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RiverSegment {
    pub id: u32,
    /// Punkty w metrach, od źródła do ujścia.
    pub points: Vec<(i32, i32)>,
    pub strahler: u8,
    /// Odcinek, do którego uchodzi; `None` = ujście do morza lub jeziora bezodpływowego.
    pub downstream: Option<u32>,
    pub width_dm: u16,
    pub depth_dm: u16,
}

/// Wektorowa sieć koryt.
#[derive(Clone, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RiverNetwork {
    pub segments: Vec<RiverSegment>,
}

impl RiverNetwork {
    /// Łączna długość koryt w metrach — pozycja raportu generacji.
    #[must_use]
    pub fn total_length_m(&self) -> u64 {
        let mut sum = 0u64;
        for s in &self.segments {
            for w in s.points.windows(2) {
                let dx = i64::from(w[1].0 - w[0].0);
                let dy = i64::from(w[1].1 - w[0].1);
                // Odległość całkowitoliczbowa: sqrt jest dokładnie zaokrąglane (00 §K-6).
                sum += magnat_core::det_math::sqrt((dx * dx + dy * dy) as f64) as u64;
            }
        }
        sum
    }
}

/// Trwały stan świata po generacji.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorldData {
    pub params: WorldGenParams,
    /// Wysokość terenu po erozji, siatka robocza 4 m.
    pub height: Grid2<HeightDm>,
    /// Klasa wody + kierunek spływu, siatka robocza 4 m.
    pub water: Grid2<WaterBits>,
    /// Komórki rzeczne, posortowane po `cell` — wyszukiwanie binarne.
    pub river_cells: Vec<RiverCell>,
    /// Komórki jeziorne, posortowane po indeksie.
    pub lake_cells: LakeCells,
    pub rivers: RiverNetwork,
    pub deposits: Vec<crate::deposit::Deposit>,
    /// Siatka klimatu 256 m.
    pub climate: Grid2<crate::climate::ClimateCell>,
}

impl WorldData {
    /// Świat pusty, o właściwych wymiarach — punkt wyjścia potoku.
    #[must_use]
    pub fn empty(params: WorldGenParams) -> WorldData {
        let n = params.work_dim();
        let c = params.climate_dim();
        WorldData {
            params,
            height: Grid2::filled(n, SEA_LEVEL_DM),
            water: Grid2::filled(n, WaterBits::default()),
            river_cells: Vec::new(),
            lake_cells: LakeCells::default(),
            rivers: RiverNetwork::default(),
            deposits: Vec::new(),
            climate: Grid2::filled(c, crate::climate::ClimateCell::default()),
        }
    }

    /// Rzędna zwierciadła wody w komórce, jeśli w niej jest woda.
    #[must_use]
    pub fn water_surface_dm(&self, cell: usize) -> Option<HeightDm> {
        match self.water[cell].class() {
            WaterClass::Dry => None,
            WaterClass::Sea => Some(SEA_LEVEL_DM),
            WaterClass::Lake => self.lake_cells.surface_of(cell),
            WaterClass::River => self
                .river_cells
                .binary_search_by_key(&(cell as u32), |r| r.cell)
                .ok()
                .map(|i| self.river_cells[i].surface_dm),
        }
    }

    /// Głębokość wody w decymetrach; 0 dla lądu. Nigdy ujemna — patrz test `no_negative_water`.
    #[must_use]
    pub fn water_depth_dm(&self, cell: usize) -> u16 {
        match self.water_surface_dm(cell) {
            Some(s) => {
                (i32::from(s) - i32::from(self.height[cell])).clamp(0, i32::from(u16::MAX)) as u16
            }
            None => 0,
        }
    }

    #[must_use]
    pub fn river_cell(&self, cell: usize) -> Option<&RiverCell> {
        self.river_cells
            .binary_search_by_key(&(cell as u32), |r| r.cell)
            .ok()
            .map(|i| &self.river_cells[i])
    }

    /// Hash stanu świata — test kontraktowy determinizmu (00 §3.6, M1 §7.1).
    /// Kolejność haszowania jest kolejnością indeksów, nigdy kolejnością iteracji mapy.
    pub fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.params.seed);
        h.write_u32(self.params.size.meters());
        for v in self.height.as_slice() {
            h.write_u16(*v as u16);
        }
        for v in self.water.as_slice() {
            h.write_u8(v.0);
        }
        for r in &self.river_cells {
            h.write_u32(r.cell);
            h.write_u8(r.strahler);
            h.write_u16(r.depth_dm);
            h.write_u16(r.width_dm);
            h.write_u16(r.surface_dm as u16);
        }
        for (c, s) in self.lake_cells.cell.iter().zip(&self.lake_cells.surface_dm) {
            h.write_u32(*c);
            h.write_u16(*s as u16);
        }
        for d in &self.deposits {
            d.hash_state(h);
        }
        for c in self.climate.as_slice() {
            c.hash_state(h);
        }
    }

    /// Rozmiar stanu trwałego w bajtach — asercja budżetu z M1 §5.9.
    #[must_use]
    pub fn persistent_bytes(&self) -> usize {
        use std::mem::size_of;
        self.height.len() * size_of::<HeightDm>()
            + self.water.len() * size_of::<WaterBits>()
            + self.river_cells.len() * size_of::<RiverCell>()
            + self.lake_cells.bytes()
            + self
                .rivers
                .segments
                .iter()
                .map(|s| s.points.len() * size_of::<(i32, i32)>() + size_of::<RiverSegment>())
                .sum::<usize>()
            + self.deposits.len() * size_of::<crate::deposit::Deposit>()
            + self.climate.len() * size_of::<crate::climate::ClimateCell>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bity_wody_pakuja_sie_bez_strat() {
        for dir in 0..8u8 {
            for &class in &[
                WaterClass::Dry,
                WaterClass::Sea,
                WaterClass::Lake,
                WaterClass::River,
            ] {
                for sink in [false, true] {
                    let b = WaterBits::new(class, dir, sink);
                    assert_eq!(b.class(), class);
                    assert_eq!(b.flow_dir(), dir);
                    assert_eq!(b.is_sink(), sink);
                }
            }
        }
    }

    #[test]
    fn glebokosc_morza_liczy_sie_od_poziomu_zero() {
        let mut w = WorldData::empty(WorldGenParams::default());
        w.height.set(0, 0, -35);
        w.water.set(0, 0, WaterBits::new(WaterClass::Sea, 0, true));
        assert_eq!(w.water_depth_dm(0), 35);
        // Ląd nad poziomem morza nigdy nie ma ujemnej głębokości.
        w.height.set(1, 0, 120);
        assert_eq!(w.water_depth_dm(1), 0);
    }
}
