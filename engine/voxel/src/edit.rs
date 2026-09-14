//! Kolejka edycji voxeli — model dwufazowy (M1 §5.4a).
//!
//! **M2 i M6 nie piszą do chunków. Kolejkują edycje.** To nie jest wygoda `engine/voxel`,
//! tylko zastosowanie kontraktu 00 §3.4: voxele są stanem trwałym symulacji (kopalnia je drąży,
//! zapis gry je przechowuje), więc podlegają tej samej regule co encje — mutacja strukturalna
//! wyłącznie przez bufor komend, aplikowana w punkcie synchronizacji, w ustalonym porządku.
//!
//! ## Korekta planu: czym jest „ustalony porządek"
//!
//! M1 §5.4a zakłada sortowanie po `(source, seq)`, gdzie `seq` nadaje się przy `push`.
//! To **nie jest deterministyczne**, gdy `push` wolno wołać z `&self` z wielu jobów naraz:
//! numer dostaje ten worker, który pierwszy dobiegł do licznika, a to zależy od harmonogramu.
//! Kolejność kanoniczna jest więc wyprowadzana **z treści komendy** (źródło, zasięg, rodzaj
//! operacji), a `EditSeq` zostaje wyłącznie jako identyfikator do raportu i do diagnostyki.
//! Wynik: ten sam zbiór komend daje ten sam stan voxeli przy 1 i przy 16 wątkach,
//! niezależnie od kolejności kolejkowania — czego wymaga test `edits_order_invariant` (§7.1).

use crate::chunk::ChunkCoord;
use crate::material::MaterialId;
use magnat_core::{IAabb3, IVec3};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// Identyfikator systemu zgłaszającego edycję. Odpowiednik `SystemId` z `engine/ecs` —
/// tutaj jako liczba, żeby `engine/voxel` nie musiał zależeć od ECS dla jednego pola.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct EditSource(pub u16);

/// Globalny numer porządkowy edycji. **Nie** jest kluczem sortowania — patrz nagłówek modułu.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct EditSeq(pub u64);

/// Obrót wielokrotności 90° wokół osi pionowej — jedyny, jaki ma sens dla prefabrykatu
/// stawianego na siatce voxeli.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub enum Rot90 {
    #[default]
    Deg0,
    Deg90,
    Deg180,
    Deg270,
}

/// Prostopadłościan **zorientowany**, w voxelach, obrócony wokół osi pionowej.
///
/// Dodany w M2d. Budynek stoi równolegle do swojej ulicy, a ulica biegnie pod dowolnym
/// kątem: `IAabb3` wymusiłby albo miasto w układzie Manhattan, albo rozbicie każdej bryły
/// na kilkadziesiąt komend-wierszy — przy 50 tys. budynków miliony komend zamiast tysięcy.
/// Obrót wyłącznie wokół pionu, bo tylko taki ma sens dla budynku i dla jezdni.
///
/// Zmiennoprzecinkowo i to jest dozwolone: 00 §2 („float wolno w geometrii") oraz §K-6
/// (`+ − × ÷`, `sqrt` w IEEE-754 są powtarzalne międzyplatformowo). Test przynależności
/// to same porównania, więc przypisanie voxela do bryły jest deterministyczne.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Obb3 {
    /// Środek bryły w voxelach.
    pub center: [f32; 3],
    /// Kierunek osi „u" w poziomie (jednostkowy); oś „v" to obrót o 90°.
    pub axis_u: [f32; 2],
    /// Połowy wymiarów wzdłuż (u, v, z), w voxelach.
    pub half: [f32; 3],
}

impl Obb3 {
    #[must_use]
    pub fn new(center: [f32; 3], axis_u: [f32; 2], half: [f32; 3]) -> Obb3 {
        let len = (axis_u[0] * axis_u[0] + axis_u[1] * axis_u[1]).sqrt();
        let a = if len > 1e-6 {
            [axis_u[0] / len, axis_u[1] / len]
        } else {
            [1.0, 0.0]
        };
        Obb3 {
            center,
            axis_u: a,
            half,
        }
    }

    /// Czy **środek** voxela leży w bryle. Środek, nie narożnik: inaczej dwie bryły
    /// stykające się ścianą albo zostawiają szczelinę, albo nachodzą na siebie o voxel.
    #[must_use]
    pub fn contains_voxel(&self, x: i32, y: i32, z: i32) -> bool {
        let dx = x as f32 + 0.5 - self.center[0];
        let dy = y as f32 + 0.5 - self.center[1];
        let dz = z as f32 + 0.5 - self.center[2];
        let du = dx * self.axis_u[0] + dy * self.axis_u[1];
        let dv = -dx * self.axis_u[1] + dy * self.axis_u[0];
        du.abs() <= self.half[0] && dv.abs() <= self.half[1] && dz.abs() <= self.half[2]
    }

    /// Prostopadłościan opisany — zasięg komendy dla fazy B.
    #[must_use]
    pub fn aabb(&self) -> IAabb3 {
        let ex = (self.axis_u[0] * self.half[0]).abs() + (self.axis_u[1] * self.half[1]).abs();
        let ey = (self.axis_u[1] * self.half[0]).abs() + (self.axis_u[0] * self.half[1]).abs();
        IAabb3::new(
            IVec3::new(
                (self.center[0] - ex).floor() as i32,
                (self.center[1] - ey).floor() as i32,
                (self.center[2] - self.half[2]).floor() as i32,
            ),
            IVec3::new(
                (self.center[0] + ex).ceil() as i32 + 1,
                (self.center[1] + ey).ceil() as i32 + 1,
                (self.center[2] + self.half[2]).ceil() as i32 + 1,
            ),
        )
    }

    /// Odcisk treści — składnik klucza porządku kanonicznego (patrz [`VoxelEditCmd::key`]).
    fn bits(&self) -> [u32; 8] {
        [
            self.center[0].to_bits(),
            self.center[1].to_bits(),
            self.center[2].to_bits(),
            self.axis_u[0].to_bits(),
            self.axis_u[1].to_bits(),
            self.half[0].to_bits(),
            self.half[1].to_bits(),
            self.half[2].to_bits(),
        ]
    }
}

/// Kształt wykopu.
#[derive(Clone, PartialEq, Debug)]
pub enum CarveShape {
    /// Wykop prostopadłościenny — fundament, wyrobisko odkrywkowe.
    Box(IAabb3),
    /// Szyb albo sztolnia: odcinek o zadanym promieniu.
    Tunnel { from: IVec3, to: IVec3, radius: u32 },
    /// Otwór zorientowany — okno, przejazd bramny, prześwit pod mostem (M2d).
    Prism(Obb3),
}

impl CarveShape {
    #[must_use]
    pub fn aabb(&self) -> IAabb3 {
        match self {
            CarveShape::Box(a) => *a,
            CarveShape::Prism(o) => o.aabb(),
            CarveShape::Tunnel { from, to, radius } => {
                let r = *radius as i32;
                IAabb3::new(
                    IVec3::new(
                        from.x.min(to.x) - r,
                        from.y.min(to.y) - r,
                        from.z.min(to.z) - r,
                    ),
                    IVec3::new(
                        from.x.max(to.x) + r + 1,
                        from.y.max(to.y) + r + 1,
                        from.z.max(to.z) + r + 1,
                    ),
                )
            }
        }
    }
}

/// Operacja edycji.
#[derive(Clone, PartialEq, Debug)]
pub enum EditOp {
    /// Wypełnienie prostopadłościanu jednym materiałem.
    Fill { aabb: IAabb3, material: MaterialId },
    /// Wypełnienie bryły zorientowanej — budynek przy ulicy pod kątem, korpus jezdni (M2d).
    Prism { obb: Obb3, material: MaterialId },
    /// Wykop — zastępuje materiał powietrzem.
    Carve { shape: CarveShape },
    /// Wyrównanie terenu do zadanego poziomu: nasyp albo wykop drogowy (M2).
    Terrace {
        aabb: IAabb3,
        target_z: i32,
        material: MaterialId,
    },
}

impl EditOp {
    /// Zasięg operacji. Liczony **przy kolejkowaniu**, żeby faza B mogła rozdzielić komendy
    /// po chunkach bez interpretowania rodzaju operacji.
    #[must_use]
    pub fn aabb(&self) -> IAabb3 {
        match self {
            EditOp::Fill { aabb, .. } | EditOp::Terrace { aabb, .. } => *aabb,
            EditOp::Prism { obb, .. } => obb.aabb(),
            EditOp::Carve { shape } => shape.aabb(),
        }
    }

    /// Ranga rodzaju operacji w porządku kanonicznym. Wartości są wieczne — zmiana
    /// przestawia kolejność stosowania edycji w każdym zapisanym świecie.
    #[must_use]
    const fn rank(&self) -> u8 {
        match self {
            EditOp::Fill { .. } => 0,
            EditOp::Carve { .. } => 1,
            EditOp::Terrace { .. } => 2,
            // Bryła zorientowana też kładzie materiał, ale **po** `Fill`: to ona jest
            // dokładniejsza i ma prawo nadpisać zgrubny prostopadłościan, nie odwrotnie.
            EditOp::Prism { .. } => 3,
        }
    }

    /// Odcisk treści operacji. Domyka porządek kanoniczny: dwie komendy o tym samym
    /// źródle, zasięgu i randze (dwa okna w jednej ścianie, dwa `Fill` o tym samym
    /// pudełku i różnym materiale) miały dotąd **równe klucze**, a `sort_by_key` jest
    /// stabilny — o kolejności decydowała wtedy kolejność `push`, czyli harmonogram
    /// wątków. Dokładnie to, czego zakazuje nagłówek modułu.
    fn content_hash(&self) -> u64 {
        let mut h = magnat_core::StateHasher::new();
        h.write_u8(self.rank());
        match self {
            EditOp::Fill { material, .. } => h.write_u16(material.0),
            EditOp::Terrace {
                target_z, material, ..
            } => {
                h.write_i64(i64::from(*target_z));
                h.write_u16(material.0);
            }
            EditOp::Prism { obb, material } => {
                for b in obb.bits() {
                    h.write_u32(b);
                }
                h.write_u16(material.0);
            }
            EditOp::Carve { shape } => match shape {
                CarveShape::Box(_) => h.write_u8(0),
                CarveShape::Tunnel { radius, .. } => {
                    h.write_u8(1);
                    h.write_u32(*radius);
                }
                CarveShape::Prism(o) => {
                    h.write_u8(2);
                    for b in o.bits() {
                        h.write_u32(b);
                    }
                }
            },
        }
        h.finish().0 as u64
    }
}

/// Komenda edycji w kolejce.
#[derive(Clone, PartialEq, Debug)]
pub struct VoxelEditCmd {
    pub seq: EditSeq,
    pub source: EditSource,
    pub op: EditOp,
    pub aabb: IAabb3,
}

impl VoxelEditCmd {
    /// Klucz porządku kanonicznego: wyprowadzony **z treści**, nie z kolejności kolejkowania.
    fn key(&self) -> (u16, i32, i32, i32, i32, i32, i32, u8, u64) {
        (
            self.source.0,
            self.aabb.min.z,
            self.aabb.min.y,
            self.aabb.min.x,
            self.aabb.max.z,
            self.aabb.max.y,
            self.aabb.max.x,
            self.op.rank(),
            self.op.content_hash(),
        )
    }
}

/// Faza A — kolejkowanie. Wolno wołać z dowolnego joba, także dla budynku przecinającego
/// granicę chunka: podziałem na chunki zajmuje się faza B.
pub struct EditQueue {
    /// Jeden zamek na cały bufor, nie bufor per worker.
    ///
    /// ponytail: `Mutex<Vec>` zamiast buforów thread-local z M1 §5.4a. Kolejkowanie zdarza się
    /// rzędy wielkości rzadziej niż odczyt voxeli (budynek to jedna komenda, nie tysiąc),
    /// a porządek i tak wyprowadza się z treści, więc bufory per worker dałyby tylko mniej
    /// rywalizacji o zamek, której nie ma. Ścieżka wyjścia, gdyby profil pokazał inaczej:
    /// `Vec` per indeks workera i scalenie w fazie B — kontrakt `push(&self)` się nie zmienia.
    cmds: Mutex<Vec<VoxelEditCmd>>,
    next_seq: AtomicU64,
}

impl Default for EditQueue {
    fn default() -> Self {
        EditQueue::new()
    }
}

impl EditQueue {
    #[must_use]
    pub fn new() -> EditQueue {
        EditQueue {
            cmds: Mutex::new(Vec::new()),
            next_seq: AtomicU64::new(1),
        }
    }

    /// `&self`, nie `&mut self` — kolejkowanie jest równoległe z założenia.
    pub fn push(&self, source: EditSource, op: EditOp) -> EditSeq {
        let seq = EditSeq(self.next_seq.fetch_add(1, Ordering::Relaxed));
        let aabb = op.aabb();
        self.cmds.lock().push(VoxelEditCmd {
            seq,
            source,
            op,
            aabb,
        });
        seq
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.cmds.lock().len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cmds.lock().is_empty()
    }

    /// Zabiera zawartość i zwraca ją w **porządku kanonicznym**.
    pub fn drain_sorted(&self) -> Vec<VoxelEditCmd> {
        let mut v = std::mem::take(&mut *self.cmds.lock());
        v.sort_by_key(VoxelEditCmd::key);
        v
    }
}

/// Rasteryzacja jednej komendy do chunka `(origin, lod)`.
///
/// `set` dostaje **lokalne** współrzędne w siatce tego chunka i materiał. Jedno miejsce
/// dla wszystkich konsumentów: nakładki edycji ([`crate::EditOverlay`]) i indeksu
/// leniwego ([`EditIndex`]) — inaczej znaczenie `Terrace` zaczęłoby się różnić
/// między ścieżką zapisu gry a ścieżką podglądu.
///
/// Przy `lod > 0` wiele voxeli świata wpada w jedną komórkę; wygrywa ostatni zapis,
/// a kolejność jest kolejnością pętli, czyli deterministyczna.
pub fn rasterize(
    cmd: &VoxelEditCmd,
    origin: IVec3,
    lod: u8,
    dim: i32,
    set: &mut impl FnMut(i32, i32, i32, MaterialId),
) {
    let span = dim << lod;
    let hi = IVec3::new(origin.x + span, origin.y + span, origin.z + span);
    let a = cmd.aabb;
    let (x0, x1) = (a.min.x.max(origin.x), a.max.x.min(hi.x));
    let (y0, y1) = (a.min.y.max(origin.y), a.max.y.min(hi.y));
    let (z0, z1) = (a.min.z.max(origin.z), a.max.z.min(hi.z));
    if x0 >= x1 || y0 >= y1 || z0 >= z1 {
        return;
    }
    let mut zapisz = |x: i32, y: i32, z: i32, m: MaterialId| {
        set(
            (x - origin.x) >> lod,
            (y - origin.y) >> lod,
            (z - origin.z) >> lod,
            m,
        );
    };

    match &cmd.op {
        EditOp::Fill { material, .. } => {
            for z in z0..z1 {
                for y in y0..y1 {
                    for x in x0..x1 {
                        zapisz(x, y, z, *material);
                    }
                }
            }
        }
        EditOp::Prism { obb, material } => {
            for z in z0..z1 {
                for y in y0..y1 {
                    for x in x0..x1 {
                        if obb.contains_voxel(x, y, z) {
                            zapisz(x, y, z, *material);
                        }
                    }
                }
            }
        }
        EditOp::Carve { shape } => {
            for z in z0..z1 {
                for y in y0..y1 {
                    for x in x0..x1 {
                        let w = match shape {
                            CarveShape::Box(_) => true,
                            CarveShape::Prism(o) => o.contains_voxel(x, y, z),
                            CarveShape::Tunnel { from, to, radius } => {
                                let r = i64::from(*radius);
                                dist_sq_point_segment(IVec3::new(x, y, z), *from, *to) <= r * r
                            }
                        };
                        if w {
                            zapisz(x, y, z, MaterialId::AIR);
                        }
                    }
                }
            }
        }
        EditOp::Terrace {
            target_z, material, ..
        } => {
            // Wyrównanie: poniżej poziomu docelowego materiał nasypu, powyżej powietrze.
            for z in z0..z1 {
                let m = if z <= *target_z {
                    *material
                } else {
                    MaterialId::AIR
                };
                for y in y0..y1 {
                    for x in x0..x1 {
                        zapisz(x, y, z, m);
                    }
                }
            }
        }
    }
}

fn dist_sq_point_segment(p: IVec3, a: IVec3, b: IVec3) -> i64 {
    let ab = b - a;
    let ap = p - a;
    let len_sq = i64::from(ab.x) * i64::from(ab.x)
        + i64::from(ab.y) * i64::from(ab.y)
        + i64::from(ab.z) * i64::from(ab.z);
    if len_sq == 0 {
        return ap.distance_sq(IVec3::ZERO);
    }
    let dot = (i64::from(ap.x) * i64::from(ab.x)
        + i64::from(ap.y) * i64::from(ab.y)
        + i64::from(ap.z) * i64::from(ab.z))
    .clamp(0, len_sq);
    let cx = i64::from(ap.x) - i64::from(ab.x) * dot / len_sq;
    let cy = i64::from(ap.y) - i64::from(ab.y) * dot / len_sq;
    let cz = i64::from(ap.z) - i64::from(ab.z) * dot / len_sq;
    cx * cx + cy * cy + cz * cz
}

/// Komendy edycji z indeksem chunkowym — **stosowane leniwie, przy materializacji**.
///
/// Alternatywa dla [`crate::EditOverlay`] tam, gdzie edycji jest dużo i są trwałe:
/// nakładka trzyma **każdy zmieniony voxel**, a miasto to ~50 tys. budynków po kilkanaście
/// tysięcy voxeli każdy — setki milionów wpisów, czyli dziesiątki gigabajtów. Indeks trzyma
/// **komendy** (kilkaset tysięcy) i przelicza je dla chunka w chwili, gdy chunk powstaje.
/// Ta sama rasteryzacja, ten sam wynik, pamięć rzędu wielkości mniejsza.
///
/// Nakładka zostaje dla edycji **runtime'owych** (M6 drąży kopalnię w trakcie gry):
/// tam liczy się, że zmiana jest mała i musi przeżyć w zapisie gry bez powtarzania
/// całej generacji.
#[derive(Clone, Default, Debug)]
pub struct EditIndex {
    cmds: Vec<VoxelEditCmd>,
    /// Chunk LOD0 → indeksy komend, które go dotykają. `BTreeMap` (00 §3.2).
    buckets: std::collections::BTreeMap<ChunkCoord, Vec<u32>>,
}

impl EditIndex {
    /// Zabiera kolejkę i buduje indeks. Komendy zostają w porządku kanonicznym,
    /// więc kolejność stosowania nie zależy od kolejności kolejkowania.
    #[must_use]
    pub fn build(q: &EditQueue) -> EditIndex {
        EditIndex::from_commands(q.drain_sorted())
    }

    #[must_use]
    pub fn from_commands(cmds: Vec<VoxelEditCmd>) -> EditIndex {
        let mut buckets: std::collections::BTreeMap<ChunkCoord, Vec<u32>> =
            std::collections::BTreeMap::new();
        for (coord, lista) in bucket_by_chunk(&cmds) {
            buckets.insert(coord, lista.into_iter().map(|i| i as u32).collect());
        }
        EditIndex { cmds, buckets }
    }

    #[must_use]
    pub fn commands(&self) -> &[VoxelEditCmd] {
        &self.cmds
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cmds.is_empty()
    }

    /// Liczba par (chunk, komenda) — miara kosztu pamięciowego indeksu.
    #[must_use]
    pub fn entries(&self) -> usize {
        self.buckets.values().map(Vec::len).sum()
    }

    /// Czy chunk `(coord, lod)` jest w ogóle dotknięty przez którąkolwiek komendę.
    #[must_use]
    pub fn touches(&self, coord: ChunkCoord, lod: u8) -> bool {
        !self.indices_for(coord, lod).is_empty()
    }

    /// Nakłada edycje na świeżo zmaterializowany chunk. Woła się **po** `ColumnSource`.
    pub fn apply_to(&self, coord: ChunkCoord, lod: u8, b: &mut crate::ChunkBuilder) {
        let dim = crate::chunk::CHUNK_DIM as i32;
        let origin = coord.origin_voxels(lod);
        for i in self.indices_for(coord, lod) {
            rasterize(
                &self.cmds[i as usize],
                origin,
                lod,
                dim,
                &mut |x, y, z, m| {
                    b.set(x, y, z, m);
                },
            );
        }
    }

    /// Indeksy komend dotykających chunka. Przy `lod > 0` chunk pokrywa `2^lod` chunków
    /// LOD0 w każdej osi, więc kubełki trzeba zebrać i **posortować z odsianiem duplikatów** —
    /// komenda przecinająca granicę leży w kilku z nich, a zastosowana dwa razy dałaby
    /// ten sam wynik tylko przy operacjach idempotentnych.
    fn indices_for(&self, coord: ChunkCoord, lod: u8) -> Vec<u32> {
        if lod == 0 {
            return self.buckets.get(&coord).cloned().unwrap_or_default();
        }
        let n = 1i32 << lod;
        let (x0, y0, z0) = (coord.x * n, coord.y * n, i32::from(coord.z) * n);
        let lo = ChunkCoord::new(x0, i32::MIN, i16::MIN);
        let hi = ChunkCoord::new(x0 + n - 1, i32::MAX, i16::MAX);
        let mut out: Vec<u32> = Vec::new();
        for (c, lista) in self.buckets.range(lo..=hi) {
            if c.y < y0 || c.y >= y0 + n || i32::from(c.z) < z0 || i32::from(c.z) >= z0 + n {
                continue;
            }
            out.extend_from_slice(lista);
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}

/// Nakładanie się dwóch edycji na ten sam obszar.
///
/// M2 używa tego do wykrycia kolidujących budynków. `apply_edits` **zawsze stosuje wszystko**
/// i tylko raportuje nakładki: tylko M2 wie, czy nakładka to błąd (dwa budynki na tej samej
/// działce) czy zamiar (budynek na nasypie), więc rozstrzyganie należy do niego (M1 §9 D11).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Overlap {
    pub first: EditSeq,
    pub second: EditSeq,
    pub aabb: IAabb3,
}

/// Wynik punktu synchronizacji.
#[derive(Clone, Default, Debug)]
pub struct EditReport {
    pub commands: usize,
    pub chunks_touched: usize,
    pub voxels_written: u64,
    pub overlaps: Vec<Overlap>,
}

/// Rozdziela komendy po chunkach. Komenda przecinająca granicę trafia na listę **każdego**
/// chunka, którego dotyka — i to jest dokładnie to miejsce, w którym znika przypadek graniczny
/// M2: faza wywołująca nie musi nic wiedzieć o granicach chunków.
#[must_use]
pub fn bucket_by_chunk(cmds: &[VoxelEditCmd]) -> Vec<(ChunkCoord, Vec<usize>)> {
    use std::collections::BTreeMap;
    let dim = crate::chunk::CHUNK_DIM as i32;
    // `BTreeMap`, nie `HashMap`: kolejność iteracji wchodzi do kolejności aplikacji (00 §3.2).
    let mut kubelki: BTreeMap<ChunkCoord, Vec<usize>> = BTreeMap::new();

    for (i, c) in cmds.iter().enumerate() {
        if c.aabb.is_empty() {
            continue;
        }
        let min = ChunkCoord::new(
            c.aabb.min.x.div_euclid(dim),
            c.aabb.min.y.div_euclid(dim),
            c.aabb.min.z.div_euclid(dim) as i16,
        );
        let max = ChunkCoord::new(
            (c.aabb.max.x - 1).div_euclid(dim),
            (c.aabb.max.y - 1).div_euclid(dim),
            (c.aabb.max.z - 1).div_euclid(dim) as i16,
        );
        for z in min.z..=max.z {
            for y in min.y..=max.y {
                for x in min.x..=max.x {
                    kubelki.entry(ChunkCoord::new(x, y, z)).or_default().push(i);
                }
            }
        }
    }
    kubelki.into_iter().collect()
}

/// Wykrywa nakładające się zasięgi w posortowanej liście komend.
#[must_use]
pub fn find_overlaps(cmds: &[VoxelEditCmd]) -> Vec<Overlap> {
    let mut out = Vec::new();
    // Kwadratowo po liczbie komend w punkcie synchronizacji.
    //
    // ponytail: O(n²) na zbiorze, który w M2 liczy setki komend na tick (budynki dzielnicy),
    // czyli dziesiątki tysięcy porównań AABB — poniżej milisekundy. Ścieżka wyjścia, gdy
    // M6 zacznie kolejkować tysiące wykopów naraz: zamiatanie po osi X z listą aktywnych.
    for i in 0..cmds.len() {
        for j in (i + 1)..cmds.len() {
            if cmds[i].aabb.intersects(cmds[j].aabb) {
                out.push(Overlap {
                    first: cmds[i].seq,
                    second: cmds[j].seq,
                    aabb: przeciecie(cmds[i].aabb, cmds[j].aabb),
                });
            }
        }
    }
    out
}

fn przeciecie(a: IAabb3, b: IAabb3) -> IAabb3 {
    IAabb3::new(
        IVec3::new(
            a.min.x.max(b.min.x),
            a.min.y.max(b.min.y),
            a.min.z.max(b.min.z),
        ),
        IVec3::new(
            a.max.x.min(b.max.x),
            a.max.y.min(b.max.y),
            a.max.z.min(b.max.z),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill(x: i32, y: i32, z: i32, m: u16) -> EditOp {
        EditOp::Fill {
            aabb: IAabb3::new(IVec3::new(x, y, z), IVec3::new(x + 4, y + 4, z + 4)),
            material: MaterialId(m),
        }
    }

    #[test]
    fn porzadek_kanoniczny_nie_zalezy_od_kolejnosci_kolejkowania() {
        // To jest sedno korekty planu: `seq` różni się między przebiegami, a kolejność
        // stosowania — nie.
        let a = EditQueue::new();
        a.push(EditSource(1), fill(0, 0, 0, 5));
        a.push(EditSource(1), fill(64, 0, 0, 6));
        a.push(EditSource(1), fill(32, 0, 0, 7));

        let b = EditQueue::new();
        b.push(EditSource(1), fill(32, 0, 0, 7));
        b.push(EditSource(1), fill(0, 0, 0, 5));
        b.push(EditSource(1), fill(64, 0, 0, 6));

        let (sa, sb) = (a.drain_sorted(), b.drain_sorted());
        let ops: Vec<&EditOp> = sa.iter().map(|c| &c.op).collect();
        let ops_b: Vec<&EditOp> = sb.iter().map(|c| &c.op).collect();
        assert_eq!(ops, ops_b, "porządek zależy od kolejności kolejkowania");
        // `seq` natomiast **ma prawo** się różnić — i właśnie dlatego nie jest kluczem.
        assert_ne!(sa[0].seq, sb[0].seq);
    }

    #[test]
    fn dwie_komendy_o_tym_samym_pudelku_maja_rozny_klucz() {
        // Bez odcisku treści obie miały klucz identyczny, a `sort_by_key` jest stabilny:
        // o kolejności decydowałaby kolejność `push`, czyli harmonogram wątków.
        let a = EditQueue::new();
        a.push(EditSource(1), fill(0, 0, 0, 5));
        a.push(EditSource(1), fill(0, 0, 0, 9));
        let b = EditQueue::new();
        b.push(EditSource(1), fill(0, 0, 0, 9));
        b.push(EditSource(1), fill(0, 0, 0, 5));
        let (sa, sb) = (a.drain_sorted(), b.drain_sorted());
        assert_eq!(
            sa.iter().map(|c| &c.op).collect::<Vec<_>>(),
            sb.iter().map(|c| &c.op).collect::<Vec<_>>(),
            "remis w kluczu rozstrzyga kolejność kolejkowania"
        );
    }

    #[test]
    fn bryla_obrocona_lezy_w_swoim_pudelku_i_ma_zadane_pole() {
        // Kwadrat 10×10 obrócony o 45° ma przekątną 14,14, więc AABB rośnie do ~15,
        // a liczba trafionych voxeli zostaje bliska 100.
        let o = Obb3::new([50.0, 50.0, 0.5], [1.0, 1.0], [5.0, 5.0, 0.5]);
        let a = o.aabb();
        assert!(
            a.min.x <= 42 && a.max.x >= 58,
            "pudełko nie objęło obrotu: {a:?}"
        );
        let mut n = 0;
        for y in a.min.y..a.max.y {
            for x in a.min.x..a.max.x {
                if o.contains_voxel(x, y, 0) {
                    n += 1;
                }
            }
        }
        // Rasteryzacja po środkach voxeli: kwadrat o polu 100 obrócony o 45° trafia
        // w 112 komórek, bo przekątna łapie brzegowe. Tolerancja jest na to, a nie na błąd.
        assert!(
            (85..=120).contains(&n),
            "pole obróconej bryły to {n} voxeli"
        );
    }

    #[test]
    fn indeks_daje_ten_sam_wynik_przy_lod0_i_lod1() {
        // Sedno WP12b: budynek ma być widoczny także poza pierścieniem LOD0 (192 m).
        // Chunk LOD1 pokrywa osiem chunków LOD0, więc komenda musi się w nim znaleźć
        // dokładnie raz i trafić w komórkę o współrzędnej dwa razy mniejszej.
        let q = EditQueue::new();
        q.push(
            EditSource(1),
            EditOp::Fill {
                aabb: IAabb3::new(IVec3::new(30, 30, 2), IVec3::new(40, 40, 6)),
                material: MaterialId(3),
            },
        );
        let idx = EditIndex::build(&q);
        assert!(idx.touches(ChunkCoord::new(0, 0, 0), 0));
        assert!(idx.touches(ChunkCoord::new(1, 1, 0), 0));
        assert!(
            idx.touches(ChunkCoord::new(0, 0, 0), 1),
            "LOD1 nie widzi komendy"
        );

        let mut b0 = crate::ChunkBuilder::new(ChunkCoord::new(1, 1, 0), 0, 1);
        idx.apply_to(ChunkCoord::new(1, 1, 0), 0, &mut b0);
        let mut b1 = crate::ChunkBuilder::new(ChunkCoord::new(0, 0, 0), 1, 1);
        idx.apply_to(ChunkCoord::new(0, 0, 0), 1, &mut b1);
        // Voxel świata (34, 34, 4) → LOD0 chunk (1,1,0) lokalnie (2,2,4);
        //                          → LOD1 chunk (0,0,0) lokalnie (17,17,2).
        assert_eq!(b0.material_at(2, 2, 4), MaterialId(3));
        assert_eq!(b1.material_at(17, 17, 2), MaterialId(3));
    }

    #[test]
    fn zrodlo_rozdziela_edycje_przed_zasiegiem() {
        let q = EditQueue::new();
        q.push(EditSource(7), fill(100, 0, 0, 1));
        q.push(EditSource(2), fill(900, 0, 0, 2));
        let s = q.drain_sorted();
        assert_eq!(
            s[0].source,
            EditSource(2),
            "źródło nie jest pierwszym kluczem"
        );
    }

    #[test]
    fn komenda_na_granicy_trafia_do_obu_chunkow() {
        // Przypadek graniczny M2: budynek przecinający granicę chunka. Musi wylądować
        // na liście każdego dotykanego chunka, inaczej połowa budynku nie powstanie.
        let q = EditQueue::new();
        q.push(
            EditSource(1),
            EditOp::Fill {
                aabb: IAabb3::new(IVec3::new(30, 30, 0), IVec3::new(36, 36, 4)),
                material: MaterialId(3),
            },
        );
        let kubelki = bucket_by_chunk(&q.drain_sorted());
        assert_eq!(
            kubelki.len(),
            4,
            "komenda 6×6 na rogu dotyka czterech chunków"
        );
        for (coord, lista) in &kubelki {
            assert_eq!(lista.len(), 1, "chunk {coord:?} dostał złą liczbę komend");
        }
    }

    #[test]
    fn kubelki_ida_w_kolejnosci_wspolrzednych() {
        let q = EditQueue::new();
        q.push(EditSource(1), fill(200, 0, 0, 1));
        q.push(EditSource(1), fill(0, 0, 0, 2));
        q.push(EditSource(1), fill(100, 0, 0, 3));
        let kubelki = bucket_by_chunk(&q.drain_sorted());
        let coords: Vec<ChunkCoord> = kubelki.iter().map(|(c, _)| *c).collect();
        let mut posortowane = coords.clone();
        posortowane.sort();
        assert_eq!(
            coords, posortowane,
            "kolejność chunków nie jest deterministyczna"
        );
    }

    #[test]
    fn nakladki_sa_raportowane_a_nie_odrzucane() {
        let q = EditQueue::new();
        q.push(EditSource(1), fill(0, 0, 0, 1));
        q.push(EditSource(1), fill(2, 2, 2, 2)); // zachodzi na pierwszą
        q.push(EditSource(1), fill(100, 0, 0, 3)); // osobno
        let cmds = q.drain_sorted();
        let n = find_overlaps(&cmds);
        assert_eq!(n.len(), 1, "wykryto {} nakładek zamiast jednej", n.len());
        assert_eq!(n[0].aabb.volume(), 2 * 2 * 2, "złe przecięcie zasięgów");
        assert_eq!(
            cmds.len(),
            3,
            "komenda została odrzucona zamiast zaraportowana"
        );
    }

    #[test]
    fn zasieg_tunelu_obejmuje_promien() {
        let op = EditOp::Carve {
            shape: CarveShape::Tunnel {
                from: IVec3::new(0, 0, 0),
                to: IVec3::new(10, 0, 0),
                radius: 2,
            },
        };
        let a = op.aabb();
        assert_eq!(a.min, IVec3::new(-2, -2, -2));
        assert_eq!(a.max, IVec3::new(13, 3, 3));
    }
}
