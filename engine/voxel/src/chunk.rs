//! Chunk voxelowy, paleta lokalna i trzy warianty składowania (M1 §5.2).
//!
//! Świat 16 km ma 4,2 mln slotów chunków; gęsto to 134 GB, więc voxele **nie są składowane
//! w całości** — materializują się na żądanie z `ColumnSource` (M1 §5.1). Ten moduł
//! odpowiada wyłącznie za to, jak wygląda pojedynczy zmaterializowany chunk.

use crate::material::{LocalIdx, MaterialId};
use magnat_core::IVec3;
use smallvec::SmallVec;

pub const CHUNK_DIM: u32 = 32;
pub const CHUNK_DIM_USIZE: usize = CHUNK_DIM as usize;
pub const CHUNK_VOXELS: usize = CHUNK_DIM_USIZE * CHUNK_DIM_USIZE * CHUNK_DIM_USIZE;

/// Bok chunka w metrach poziomo.
pub const CHUNK_SPAN_M: i32 = 32;
/// Wysokość chunka w metrach (32 voxele × 0,5 m). Chunk jest niesymetryczny w metrach —
/// to celowe (M1 §5.1).
pub const CHUNK_HEIGHT_M: i32 = 16;
/// Wysokość voxela w decymetrach — rozdzielczość pionowa 0,5 m (PRD §4.2).
pub const VOXEL_HEIGHT_DM: i32 = 5;

/// Indeksowanie liniowe: `Z` jest najszybszym indeksem.
///
/// Powód jest mierzalny, nie estetyczny: w terenie najdłuższe ciągi identycznych voxeli
/// są **pionowe** (kolumna powietrza, kolumna skały), więc RLE po Z daje kilkadziesiąt
/// runów na kolumnę zamiast kilkuset przy układzie XY-major.
#[inline]
#[must_use]
pub const fn lin(x: u32, y: u32, z: u32) -> usize {
    ((y * CHUNK_DIM + x) * CHUNK_DIM + z) as usize
}

/// Odwrotność [`lin`].
#[inline]
#[must_use]
pub const fn unlin(i: usize) -> (u32, u32, u32) {
    let z = i % CHUNK_DIM_USIZE;
    let x = (i / CHUNK_DIM_USIZE) % CHUNK_DIM_USIZE;
    let y = i / (CHUNK_DIM_USIZE * CHUNK_DIM_USIZE);
    (x as u32, y as u32, z as u32)
}

/// Współrzędna chunka w jednostkach chunków. `z` jako `i16`, bo świat ma 16 warstw
/// (M1 §5.1) i nigdy nie będzie ich więcej niż kilkadziesiąt.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct ChunkCoord {
    pub x: i32,
    pub y: i32,
    pub z: i16,
}

impl ChunkCoord {
    #[must_use]
    pub const fn new(x: i32, y: i32, z: i16) -> ChunkCoord {
        ChunkCoord { x, y, z }
    }

    /// Narożnik chunka w voxelach świata, dla zadanego LOD.
    /// Przy LOD n chunk pokrywa `2^n` razy większy obszar.
    #[must_use]
    pub fn origin_voxels(self, lod: u8) -> IVec3 {
        let s = (CHUNK_DIM as i32) << lod;
        IVec3::new(self.x * s, self.y * s, i32::from(self.z) * s)
    }
}

/// Faza życia chunka w puli rezydencji (M1 §5.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ChunkState {
    #[default]
    Unloaded,
    Queued,
    Generating,
    Ready,
    Meshing,
    Resident,
}

/// Paleta lokalna chunka. Terenowy chunk ma typowo 2–6 pozycji, stąd `SmallVec` —
/// alokacja na stercie zdarza się tylko dla chunków edytowanych.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Palette {
    entries: SmallVec<[MaterialId; 8]>,
}

impl Default for Palette {
    fn default() -> Self {
        Palette::new()
    }
}

impl Palette {
    /// Paleta z powietrzem na pozycji 0 — niezmiennik całego modułu
    /// (patrz [`crate::material::MaterialId::AIR`]).
    #[must_use]
    pub fn new() -> Palette {
        let mut entries = SmallVec::new();
        entries.push(MaterialId::AIR);
        Palette { entries }
    }

    /// Dodaje materiał, jeśli go nie ma. `None`, gdy paleta jest pełna (256 pozycji,
    /// PRD §16.3) — wywołujący ma wtedy przejść na `Dense` z paletą globalną albo
    /// podzielić chunk; ciche nadpisanie byłoby zmianą terenu bez śladu.
    pub fn intern(&mut self, m: MaterialId) -> Option<LocalIdx> {
        if let Some(i) = self.entries.iter().position(|e| *e == m) {
            return Some(LocalIdx(i as u8));
        }
        if self.entries.len() >= 256 {
            return None;
        }
        self.entries.push(m);
        Some(LocalIdx((self.entries.len() - 1) as u8))
    }

    #[inline]
    #[must_use]
    pub fn resolve(&self, i: LocalIdx) -> MaterialId {
        self.entries[i.0 as usize]
    }

    /// Odwrotność [`Palette::resolve`]. `None`, gdy materiału nie ma w palecie.
    #[inline]
    #[must_use]
    pub fn local_of(&self, m: MaterialId) -> Option<LocalIdx> {
        self.entries
            .iter()
            .position(|e| *e == m)
            .map(|i| LocalIdx(i as u8))
    }

    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub fn entries(&self) -> &[MaterialId] {
        &self.entries
    }
}

/// Run w kodowaniu RLE. `len` mieści się w `u16`, bo najdłuższy możliwy run to 32768.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Run {
    pub len: u16,
    pub idx: LocalIdx,
}

/// Składowanie zawartości chunka.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ChunkStorage {
    /// Cały chunk jednym materiałem — powietrze, lita skała, głębia wody.
    /// Około 95 % slotów świata; koszt 2 B.
    Uniform(MaterialId),
    /// Powierzchnia terenu. Typowo 1500–4000 runów.
    Rle(Box<[Run]>),
    /// Chunk edytowany (wykop kopalni M6, fundament M2). 32 KB.
    Dense(Box<[LocalIdx; CHUNK_VOXELS]>),
}

impl ChunkStorage {
    /// Zużycie pamięci w bajtach — wejście do budżetu puli chunków (M1 §5.9).
    #[must_use]
    pub fn bytes(&self) -> usize {
        match self {
            ChunkStorage::Uniform(_) => std::mem::size_of::<MaterialId>(),
            ChunkStorage::Rle(r) => r.len() * std::mem::size_of::<Run>(),
            ChunkStorage::Dense(_) => CHUNK_VOXELS,
        }
    }

    /// Indeks palety w danym miejscu. Dla `Rle` koszt jest liniowy względem liczby runów —
    /// do przejścia po całym chunku służy [`ChunkStorage::decompress_into`], nie pętla po `get`.
    ///
    /// `palette` jest potrzebna wyłącznie dla `Uniform`, które niesie `MaterialId`,
    /// a nie indeks lokalny.
    #[must_use]
    pub fn get_local(&self, i: usize, palette: &Palette) -> LocalIdx {
        match self {
            ChunkStorage::Uniform(m) => palette.local_of(*m).unwrap_or(LocalIdx::AIR),
            ChunkStorage::Dense(d) => d[i],
            ChunkStorage::Rle(runs) => {
                let mut at = 0usize;
                for r in runs.iter() {
                    at += r.len as usize;
                    if i < at {
                        return r.idx;
                    }
                }
                LocalIdx::AIR
            }
        }
    }

    /// Rozpakowanie do bufora roboczego — jedyna droga do iteracji po całym chunku.
    pub fn decompress_into(&self, palette: &Palette, out: &mut [LocalIdx; CHUNK_VOXELS]) {
        match self {
            ChunkStorage::Uniform(m) => {
                out.fill(palette.local_of(*m).unwrap_or(LocalIdx::AIR));
            }
            ChunkStorage::Dense(d) => out.copy_from_slice(&d[..]),
            ChunkStorage::Rle(runs) => {
                let mut at = 0usize;
                for r in runs.iter() {
                    let end = at + r.len as usize;
                    out[at..end].fill(r.idx);
                    at = end;
                }
                debug_assert_eq!(at, CHUNK_VOXELS, "RLE nie pokrywa całego chunka");
            }
        }
    }
}

/// Chunk w puli rezydencji.
#[derive(Clone, Debug)]
pub struct Chunk {
    pub coord: ChunkCoord,
    /// 0 = 1 m, 1 = 2 m, 2 = 4 m, 3 = 8 m.
    pub lod: u8,
    pub palette: Palette,
    pub storage: ChunkStorage,
    /// Rośnie przy każdej edycji — unieważnia mesh i agregaty LOD rodziców (M1 §5.4a).
    pub revision: u32,
    pub state: ChunkState,
}

impl Chunk {
    #[must_use]
    pub fn uniform(coord: ChunkCoord, lod: u8, m: MaterialId) -> Chunk {
        let mut palette = Palette::new();
        palette.intern(m);
        Chunk {
            coord,
            lod,
            palette,
            storage: ChunkStorage::Uniform(m),
            revision: 0,
            state: ChunkState::Ready,
        }
    }

    #[inline]
    #[must_use]
    pub fn material_at(&self, x: u32, y: u32, z: u32) -> MaterialId {
        match &self.storage {
            ChunkStorage::Uniform(m) => *m,
            other => self
                .palette
                .resolve(other.get_local(lin(x, y, z), &self.palette)),
        }
    }

    /// Czy chunk jest w całości powietrzem — pytanie zadawane przed każdym meshingiem.
    #[must_use]
    pub fn is_empty_air(&self) -> bool {
        matches!(&self.storage, ChunkStorage::Uniform(m) if m.is_air())
    }

    #[must_use]
    pub fn bytes(&self) -> usize {
        self.storage.bytes() + self.palette.len() * std::mem::size_of::<MaterialId>()
    }
}

/// Bufor, do którego `ColumnSource` wpisuje zawartość chunka (M1 §5.2).
///
/// `pad` to szerokość otoczki: meshing potrzebuje jednego voxela sąsiedztwa dookoła,
/// żeby ściany na granicy chunka nie zależały od tego, czy sąsiad jest rezydentny
/// (M1 §5.3). Otoczka jest materializowana z **tego samego** źródła, więc wynik nie
/// zależy od kolejności ładowania chunków.
pub struct ChunkBuilder {
    coord: ChunkCoord,
    lod: u8,
    pad: u32,
    dim: u32,
    palette: Palette,
    cells: Vec<LocalIdx>,
    /// Przekroczenie 256 materiałów w palecie. Nie panikujemy w środku generacji —
    /// zgłaszamy w `finish`, żeby wywołujący mógł zdecydować.
    overflow: bool,
}

impl ChunkBuilder {
    #[must_use]
    pub fn new(coord: ChunkCoord, lod: u8, pad: u32) -> ChunkBuilder {
        let dim = CHUNK_DIM + 2 * pad;
        ChunkBuilder {
            coord,
            lod,
            pad,
            dim,
            palette: Palette::new(),
            cells: vec![LocalIdx::AIR; (dim * dim * dim) as usize],
            overflow: false,
        }
    }

    #[must_use]
    pub const fn coord(&self) -> ChunkCoord {
        self.coord
    }

    #[must_use]
    pub const fn lod(&self) -> u8 {
        self.lod
    }

    #[must_use]
    pub const fn pad(&self) -> u32 {
        self.pad
    }

    /// Zasięg współrzędnych lokalnych, które przyjmuje builder: `-pad ..= 31 + pad`.
    #[must_use]
    pub const fn range(&self) -> std::ops::RangeInclusive<i32> {
        -(self.pad as i32)..=(CHUNK_DIM as i32 - 1 + self.pad as i32)
    }

    #[inline]
    fn index(&self, x: i32, y: i32, z: i32) -> Option<usize> {
        let p = self.pad as i32;
        let d = self.dim as i32;
        let (ix, iy, iz) = (x + p, y + p, z + p);
        if ix < 0 || iy < 0 || iz < 0 || ix >= d || iy >= d || iz >= d {
            return None;
        }
        Some(((iy * d + ix) * d + iz) as usize)
    }

    /// Wpis pojedynczego voxela. Współrzędne poza zasięgiem otoczki są **ignorowane**,
    /// nie są błędem: `ColumnSource` wypełnia kolumnę bez wiedzy o tym, czy builder
    /// ma otoczkę, a przycinanie po jego stronie byłoby duplikacją tej samej reguły.
    pub fn set(&mut self, x: i32, y: i32, z: i32, m: MaterialId) {
        let Some(i) = self.index(x, y, z) else { return };
        match self.palette.intern(m) {
            Some(l) => self.cells[i] = l,
            None => self.overflow = true,
        }
    }

    /// Wypełnienie pionowego odcinka kolumny `[z0, z1)` jednym materiałem — główna droga
    /// wejścia danych z generatora, bo teren jest opisany kolumnami warstw.
    pub fn fill_column(&mut self, x: i32, y: i32, z0: i32, z1: i32, m: MaterialId) {
        if z1 <= z0 || m.is_air() {
            return;
        }
        let Some(l) = self.palette.intern(m) else {
            self.overflow = true;
            return;
        };
        let p = self.pad as i32;
        let lo = z0.max(-p);
        let hi = z1.min(CHUNK_DIM as i32 + p);
        for z in lo..hi {
            if let Some(i) = self.index(x, y, z) {
                self.cells[i] = l;
            }
        }
    }

    /// Odczyt z otoczki włącznie — używa tego meshing przy badaniu widoczności ścian.
    #[inline]
    #[must_use]
    pub fn local_at(&self, x: i32, y: i32, z: i32) -> LocalIdx {
        self.index(x, y, z).map_or(LocalIdx::AIR, |i| self.cells[i])
    }

    #[inline]
    #[must_use]
    pub fn material_at(&self, x: i32, y: i32, z: i32) -> MaterialId {
        self.palette.resolve(self.local_at(x, y, z))
    }

    #[must_use]
    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    #[must_use]
    pub const fn overflowed(&self) -> bool {
        self.overflow
    }

    /// Zamyka builder: wybiera najtańsze składowanie dla właściwych 32³ voxeli.
    /// Otoczka **nie** wchodzi do wyniku — służy wyłącznie meshingowi.
    #[must_use]
    pub fn finish(self) -> (Palette, ChunkStorage) {
        let mut core = Vec::with_capacity(CHUNK_VOXELS);
        for y in 0..CHUNK_DIM as i32 {
            for x in 0..CHUNK_DIM as i32 {
                for z in 0..CHUNK_DIM as i32 {
                    core.push(self.local_at(x, y, z));
                }
            }
        }
        let storage = pack(&core, &self.palette);
        (self.palette, storage)
    }
}

/// Wybór składowania: jednorodne → `Uniform`, inaczej tańsze z `Rle` i `Dense`.
#[must_use]
pub fn pack(cells: &[LocalIdx], palette: &Palette) -> ChunkStorage {
    debug_assert_eq!(cells.len(), CHUNK_VOXELS);
    let first = cells[0];
    if cells.iter().all(|c| *c == first) {
        return ChunkStorage::Uniform(palette.resolve(first));
    }

    let mut runs: Vec<Run> = Vec::new();
    let mut cur = cells[0];
    let mut len: u32 = 0;
    for c in cells {
        if *c == cur && len < u16::MAX as u32 {
            len += 1;
        } else {
            runs.push(Run {
                len: len as u16,
                idx: cur,
            });
            cur = *c;
            len = 1;
        }
    }
    runs.push(Run {
        len: len as u16,
        idx: cur,
    });

    if runs.len() * std::mem::size_of::<Run>() < CHUNK_VOXELS {
        ChunkStorage::Rle(runs.into_boxed_slice())
    } else {
        let mut dense = Box::new([LocalIdx::AIR; CHUNK_VOXELS]);
        dense.copy_from_slice(cells);
        ChunkStorage::Dense(dense)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indeksowanie_jest_odwracalne_i_z_najszybsze() {
        assert_eq!(lin(0, 0, 1), 1, "Z jest najszybszym indeksem");
        assert_eq!(lin(1, 0, 0), 32);
        assert_eq!(lin(0, 1, 0), 1024);
        for i in [0usize, 1, 31, 32, 1023, 1024, CHUNK_VOXELS - 1] {
            let (x, y, z) = unlin(i);
            assert_eq!(lin(x, y, z), i, "i = {i}");
        }
    }

    #[test]
    fn paleta_zaczyna_od_powietrza_i_zglasza_przepelnienie() {
        let mut p = Palette::new();
        assert_eq!(p.resolve(LocalIdx::AIR), MaterialId::AIR);
        assert_eq!(
            p.intern(MaterialId::AIR),
            Some(LocalIdx(0)),
            "powietrze już jest"
        );
        assert_eq!(p.intern(MaterialId(7)), Some(LocalIdx(1)));
        assert_eq!(p.intern(MaterialId(7)), Some(LocalIdx(1)), "bez duplikatów");

        // 254 nowych materiałów, żaden nie koliduje z powietrzem ani z MaterialId(7).
        for m in 100..354u16 {
            assert!(p.intern(MaterialId(m)).is_some(), "m = {m}");
        }
        assert_eq!(p.len(), 256);
        assert_eq!(
            p.intern(MaterialId(9999)),
            None,
            "257. materiał nie wchodzi"
        );
    }

    /// Kryterium ukończenia WP-V1: round-trip bit w bit.
    #[test]
    fn round_trip_dense_rle_dense() {
        // Teren syntetyczny: kolumny o różnej wysokości — dokładnie ten kształt, dla którego
        // RLE po Z ma sens.
        let mut cells = vec![LocalIdx::AIR; CHUNK_VOXELS];
        for y in 0..CHUNK_DIM {
            for x in 0..CHUNK_DIM {
                // Kolumna: skała, nad nią gleba, nad nią powietrze — dokładnie ten
                // kształt, dla którego RLE po Z ma sens (3 runy na kolumnę).
                let h = 8 + (x * 3 + y * 5) % 17;
                for z in 0..h {
                    cells[lin(x, y, z)] = if z + 2 < h { LocalIdx(1) } else { LocalIdx(2) };
                }
            }
        }
        let mut pal = Palette::new();
        for m in 1..4u16 {
            pal.intern(MaterialId(m));
        }
        let packed = pack(&cells, &pal);
        assert!(
            matches!(packed, ChunkStorage::Rle(_)),
            "teren ma się mieścić w RLE"
        );

        let mut out = [LocalIdx::AIR; CHUNK_VOXELS];
        packed.decompress_into(&pal, &mut out);
        assert_eq!(&out[..], &cells[..], "round-trip zmienił zawartość");

        // Losowy dostęp zgodny z rozpakowaniem.
        for i in (0..CHUNK_VOXELS).step_by(97) {
            assert_eq!(packed.get_local(i, &pal), cells[i], "i = {i}");
        }
    }

    #[test]
    fn szum_nie_miesci_sie_w_rle_i_ladnie_spada_do_dense() {
        // Najgorszy przypadek: sąsiednie voxele zawsze różne → runów tyle, co voxeli.
        let cells: Vec<LocalIdx> = (0..CHUNK_VOXELS).map(|i| LocalIdx((i % 7) as u8)).collect();
        let mut pal = Palette::new();
        for m in 1..8u16 {
            pal.intern(MaterialId(m));
        }
        let packed = pack(&cells, &pal);
        assert!(matches!(packed, ChunkStorage::Dense(_)));
        assert_eq!(packed.bytes(), CHUNK_VOXELS);
        let mut out = [LocalIdx::AIR; CHUNK_VOXELS];
        packed.decompress_into(&pal, &mut out);
        assert_eq!(&out[..], &cells[..]);
    }

    #[test]
    fn uniform_jest_tani() {
        let c = Chunk::uniform(ChunkCoord::new(0, 0, 0), 0, MaterialId::AIR);
        assert!(c.is_empty_air());
        assert!(
            c.bytes() <= 32,
            "Uniform ma się mieścić w 32 B, ma {}",
            c.bytes()
        );
    }

    #[test]
    fn builder_materializuje_otoczke_ale_jej_nie_zapisuje() {
        let mut b = ChunkBuilder::new(ChunkCoord::new(1, 2, 0), 0, 1);
        b.fill_column(-1, -1, -1, 4, MaterialId(3)); // wyłącznie otoczka
        b.fill_column(0, 0, 0, 4, MaterialId(3));
        assert_eq!(
            b.material_at(-1, -1, 0),
            MaterialId(3),
            "otoczka widoczna dla meshingu"
        );

        let (pal, st) = b.finish();
        assert_eq!(pal.resolve(LocalIdx(1)), MaterialId(3));
        let mut out = [LocalIdx::AIR; CHUNK_VOXELS];
        st.decompress_into(&pal, &mut out);
        assert_eq!(out[lin(0, 0, 0)], LocalIdx(1));
        assert_eq!(
            out[lin(0, 0, 4)],
            LocalIdx::AIR,
            "kolumna kończy się na z = 4"
        );
        // W wyniku nie ma śladu po otoczce: liczba niepustych voxeli to dokładnie 4.
        assert_eq!(out.iter().filter(|c| **c != LocalIdx::AIR).count(), 4);
    }
}
