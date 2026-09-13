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

/// Kształt wykopu.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum CarveShape {
    /// Wykop prostopadłościenny — fundament, wyrobisko odkrywkowe.
    Box(IAabb3),
    /// Szyb albo sztolnia: odcinek o zadanym promieniu.
    Tunnel { from: IVec3, to: IVec3, radius: u32 },
}

impl CarveShape {
    #[must_use]
    pub fn aabb(&self) -> IAabb3 {
        match self {
            CarveShape::Box(a) => *a,
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
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum EditOp {
    /// Wypełnienie prostopadłościanu jednym materiałem.
    Fill { aabb: IAabb3, material: MaterialId },
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
        }
    }
}

/// Komenda edycji w kolejce.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct VoxelEditCmd {
    pub seq: EditSeq,
    pub source: EditSource,
    pub op: EditOp,
    pub aabb: IAabb3,
}

impl VoxelEditCmd {
    /// Klucz porządku kanonicznego: wyprowadzony **z treści**, nie z kolejności kolejkowania.
    fn key(&self) -> (u16, i32, i32, i32, i32, i32, i32, u8) {
        (
            self.source.0,
            self.aabb.min.z,
            self.aabb.min.y,
            self.aabb.min.x,
            self.aabb.max.z,
            self.aabb.max.y,
            self.aabb.max.x,
            self.op.rank(),
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
