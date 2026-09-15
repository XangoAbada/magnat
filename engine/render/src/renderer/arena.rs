//! Arena geometrii: suballokator wspólnej pary buforów i chunki rezydentne na GPU.
//!
//! Wydzielone z `renderer.rs` w R-WP3 bez zmiany zachowania.

use super::Renderer;
use magnat_voxel::{ChunkCoord, ChunkMesh, CHUNK_DIM};

/// Blok w arenie: przesunięcie i długość w elementach.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct Block {
    pub(super) offset: u32,
    pub(super) len: u32,
}

/// Suballokator listy wolnych bloków.
///
/// Pierwszy pasujący blok, nie najlepszy: mesh chunka ma rozmiar rzędu kilku kilobajtów
/// i rotuje przy każdej zmianie LOD, więc szukanie najlepszego dopasowania kosztowałoby
/// więcej niż fragmentacja, której ma zapobiec.
pub(super) struct Arena {
    capacity: u32,
    free: Vec<Block>,
    used: u32,
}

impl Arena {
    pub(super) fn new(capacity: u32) -> Arena {
        Arena {
            capacity,
            free: vec![Block {
                offset: 0,
                len: capacity,
            }],
            used: 0,
        }
    }

    fn alloc(&mut self, len: u32) -> Option<Block> {
        if len == 0 {
            return Some(Block { offset: 0, len: 0 });
        }
        let i = self.free.iter().position(|b| b.len >= len)?;
        let b = self.free[i];
        if b.len == len {
            self.free.remove(i);
        } else {
            self.free[i] = Block {
                offset: b.offset + len,
                len: b.len - len,
            };
        }
        self.used += len;
        Some(Block {
            offset: b.offset,
            len,
        })
    }

    fn free_block(&mut self, b: Block) {
        if b.len == 0 {
            return;
        }
        self.used -= b.len;
        // Scalanie z sąsiadami przy zwalnianiu — bez tego arena rozsypuje się na tysiąc
        // dziur po godzinie latania kamerą i przestaje mieścić cokolwiek, mimo że jest pusta.
        self.free.push(b);
        self.free.sort_by_key(|x| x.offset);
        let mut scalone: Vec<Block> = Vec::with_capacity(self.free.len());
        for blok in self.free.drain(..) {
            match scalone.last_mut() {
                Some(p) if p.offset + p.len == blok.offset => p.len += blok.len,
                _ => scalone.push(blok),
            }
        }
        self.free = scalone;
    }

    pub(super) fn bytes_used(&self, stride: usize) -> usize {
        self.used as usize * stride
    }

    pub(super) fn capacity_bytes(&self, stride: usize) -> usize {
        self.capacity as usize * stride
    }
}

/// Wpis chunka rezydentnego na GPU.
pub(super) struct GpuChunk {
    pub(super) vertices: Block,
    pub(super) indices: Block,
    /// Ile pierwszych indeksów bloku jest nieprzezroczystych; reszta to woda (§5.8).
    pub(super) opaque_indices: u32,
    pub(super) lod: u8,
    /// Środek chunka w metrach — do cullingu.
    pub(super) center_m: [f32; 3],
    pub(super) radius_m: f32,
    /// Rewizja, dla której zbudowano ten mesh; rozjazd wymusza przebudowę.
    revision: u32,
}

/// Docelowa pojemność areny w elementach: 8 B na wierzchołek, 4 B na indeks — razem 384 MB,
/// czyli połowa budżetu 768 MB z §5.9, z zapasem na kaskady cieni i bufory pośrednie.
///
/// **Wartość docelowa, nie ostateczna.** `wgpu` z limitami `downlevel_defaults` dopuszcza
/// bufor do 256 MB, a na słabszym sprzęcie mniej — dlatego faktyczna pojemność jest
/// przycinana do `max_buffer_size` urządzenia przy starcie. Przekroczenie limitu nie jest
/// ostrzeżeniem, tylko błędem walidacji i natychmiastową paniką, więc przycięcie musi być
/// w kodzie, a nie w komentarzu.
pub(super) const VERTEX_CAPACITY: u64 = 24 * 1024 * 1024;
pub(super) const INDEX_CAPACITY: u64 = 48 * 1024 * 1024;

/// Przycina pojemność areny do limitu bufora urządzenia.
pub(super) fn pojemnosc(limit_bajtow: u64, docelowa: u64, stride: u64) -> u32 {
    let mieszczaca_sie = limit_bajtow / stride;
    docelowa.min(mieszczaca_sie).min(u64::from(u32::MAX)) as u32
}

impl Renderer {
    #[must_use]
    pub fn has_chunk(&self, coord: ChunkCoord, lod: u8, revision: u32) -> bool {
        self.chunks
            .get(&(lod, coord))
            .is_some_and(|c| c.revision == revision)
    }

    /// Wgrywa mesh chunka do areny. Stary blok jest zwalniany — bez tego arena wypełnia się
    /// po kilku minutach latania kamerą, a objawem jest zniknięcie odległych chunków.
    pub fn upload_chunk(&mut self, coord: ChunkCoord, lod: u8, revision: u32, mesh: &ChunkMesh) {
        self.remove_chunk(coord, lod);
        if mesh.is_empty() {
            return;
        }
        let Some(vb) = self.vertex_arena.alloc(mesh.vertices.len() as u32) else {
            log::warn!("arena wierzchołków pełna — chunk {coord:?} pominięty");
            return;
        };
        let Some(ib) = self.index_arena.alloc(mesh.indices.len() as u32) else {
            self.vertex_arena.free_block(vb);
            log::warn!("arena indeksów pełna — chunk {coord:?} pominięty");
            return;
        };

        self.gpu.queue.write_buffer(
            &self.vertex_buffer,
            u64::from(vb.offset) * 8,
            bytemuck::cast_slice(&mesh.vertices),
        );
        // Indeksy są lokalne dla mesha, a rysujemy z `base_vertex` — więc do bufora idą
        // bez przesunięcia. To jest powód, dla którego `base_vertex` w ogóle istnieje.
        self.gpu.queue.write_buffer(
            &self.index_buffer,
            u64::from(ib.offset) * 4,
            bytemuck::cast_slice(&mesh.indices),
        );

        let span = f32::from(CHUNK_DIM as u16) * f32::from(1u16 << lod);
        let d = CHUNK_DIM as i32;
        let skala = 1i32 << lod;
        let center_m = [
            (coord.x * d * skala) as f32 + span * 0.5,
            (coord.y * d * skala) as f32 + span * 0.5,
            (i32::from(coord.z) * d * skala) as f32 * 0.5 + span * 0.25,
        ];
        self.chunks.insert(
            (lod, coord),
            GpuChunk {
                vertices: vb,
                indices: ib,
                opaque_indices: mesh.opaque_indices,
                lod,
                center_m,
                // Promień sfery otaczającej chunk: przekątna prostopadłościanu.
                radius_m: span * 0.75,
                revision,
            },
        );
    }

    pub fn remove_chunk(&mut self, coord: ChunkCoord, lod: u8) {
        if let Some(c) = self.chunks.remove(&(lod, coord)) {
            self.vertex_arena.free_block(c.vertices);
            self.index_arena.free_block(c.indices);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arena_scala_zwolnione_bloki() {
        // Bez scalania arena rozsypuje się na dziury i przestaje mieścić cokolwiek,
        // mimo że jest pusta — objawem jest znikanie odległych chunków po kilku minutach.
        let mut a = Arena::new(100);
        let b1 = a.alloc(30).unwrap();
        let b2 = a.alloc(30).unwrap();
        let b3 = a.alloc(30).unwrap();
        a.free_block(b1);
        a.free_block(b2);
        a.free_block(b3);
        assert_eq!(
            a.free.len(),
            1,
            "wolne bloki nie zostały scalone: {:?}",
            a.free
        );
        assert_eq!(
            a.free[0],
            Block {
                offset: 0,
                len: 100
            }
        );
        assert_eq!(a.used, 0);
        // Po scaleniu znów mieści blok większy niż pojedynczy zwolniony.
        assert!(a.alloc(90).is_some());
    }

    #[test]
    fn arena_odmawia_gdy_brakuje_miejsca() {
        let mut a = Arena::new(10);
        assert!(a.alloc(8).is_some());
        assert!(a.alloc(8).is_none(), "arena przydzieliła ponad pojemność");
        assert!(a.alloc(2).is_some());
    }

    #[test]
    fn arena_uzywa_ponownie_zwolnionego_bloku_i_nie_naklada_blokow() {
        // Trzy rzeczy, które suballokator może zepsuć po cichu: przydzielić dwa razy
        // ten sam offset, zgubić pojemność przy zwolnieniu bloku ze środka, albo policzyć
        // `used` inaczej niż suma żywych bloków. Żadnej z nich nie widać na ekranie od
        // razu — objawem jest chunk rysujący cudzą geometrię kilka minut później.
        let mut a = Arena::new(64);
        let b1 = a.alloc(10).unwrap();
        let b2 = a.alloc(20).unwrap();
        let b3 = a.alloc(30).unwrap();
        assert_eq!((b1.offset, b2.offset, b3.offset), (0, 10, 30));
        assert_eq!(a.used, 60);

        // Zwolnienie bloku ze środka ma oddać dokładnie jego dziurę, nie więcej.
        a.free_block(b2);
        assert_eq!(a.used, 40);
        let b4 = a.alloc(20).unwrap();
        assert_eq!(
            b4.offset, b2.offset,
            "dziura po środkowym bloku nie została użyta"
        );

        // Bloki żywe jednocześnie nie mogą na siebie zachodzić.
        let b5 = a.alloc(4).unwrap();
        for (x, y) in [(b1, b3), (b1, b4), (b1, b5), (b3, b4), (b3, b5), (b4, b5)] {
            let (p, q) = if x.offset <= y.offset { (x, y) } else { (y, x) };
            assert!(
                p.offset + p.len <= q.offset,
                "bloki {p:?} i {q:?} nakładają się"
            );
        }

        // Pusty przydział jest neutralny: nie rusza licznika i nie zajmuje miejsca.
        let zero = a.alloc(0).unwrap();
        assert_eq!(zero.len, 0);
        assert_eq!(a.used, 64);
        a.free_block(zero);
        assert_eq!(a.used, 64);
    }
}
