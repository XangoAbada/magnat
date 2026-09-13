//! Zahardkodowany chunk 32³ i naiwna triangulacja (M0 §5.12).
//!
//! **To jest rusztowanie do wyrzucenia.** Generator terenu, greedy meshing, LOD
//! i streaming należą do M1 — stąd wzór proceduralny na dwóch mnożeniach zamiast
//! czegokolwiek, co przypomina teren. M1 przejmuje z tego pliku dokładnie nic.

use bytemuck::{Pod, Zeroable};

pub const CHUNK_SIZE: usize = 32;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    /// Normalna w postaci znormalizowanej całkowitoliczbowej — tak, jak będzie
    /// wyglądał wierzchołek w M1, żeby format bufora nie zmienił się przy przejęciu.
    pub normal: [i8; 4],
    pub palette_index: u32,
}

impl Vertex {
    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: size_of::<Vertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x3,
            },
            wgpu::VertexAttribute {
                offset: 12,
                shader_location: 1,
                format: wgpu::VertexFormat::Snorm8x4,
            },
            wgpu::VertexAttribute {
                offset: 16,
                shader_location: 2,
                format: wgpu::VertexFormat::Uint32,
            },
        ],
    };
}

/// Materiał voxela: 0 = pustka, 1..=4 = materiał z palety shadera.
#[must_use]
pub fn voxel(x: usize, y: usize, z: usize) -> u8 {
    // Wzór, nie teren: kopiec z kamiennym rdzeniem i piaskową obwódką.
    let (cx, cz) = (CHUNK_SIZE as i32 / 2, CHUNK_SIZE as i32 / 2);
    let (dx, dz) = (x as i32 - cx, z as i32 - cz);
    let promien = dx * dx + dz * dz;
    let wysokosc = (CHUNK_SIZE as i32 - 4) - promien / 12;
    let y = y as i32;
    if y > wysokosc {
        return 0;
    }
    if y == wysokosc {
        if promien > 180 {
            4
        } else {
            2
        }
    } else if y > wysokosc - 3 {
        1
    } else {
        3
    }
}

fn pelny(x: i32, y: i32, z: i32) -> bool {
    let n = CHUNK_SIZE as i32;
    if x < 0 || y < 0 || z < 0 || x >= n || y >= n || z >= n {
        return false;
    }
    voxel(x as usize, y as usize, z as usize) != 0
}

/// Naiwna triangulacja: sześć ścian na voxel, odrzucane tylko te stykające się
/// z pełnym sąsiadem. Bez greedy meshingu — to M1.
#[must_use]
pub fn build() -> (Vec<Vertex>, Vec<u32>) {
    const KIERUNKI: [([i32; 3], [i8; 4]); 6] = [
        ([1, 0, 0], [127, 0, 0, 0]),
        ([-1, 0, 0], [-127, 0, 0, 0]),
        ([0, 1, 0], [0, 127, 0, 0]),
        ([0, -1, 0], [0, -127, 0, 0]),
        ([0, 0, 1], [0, 0, 127, 0]),
        ([0, 0, -1], [0, 0, -127, 0]),
    ];
    // Narożniki ściany dla każdego kierunku, w kolejności przeciwnej do ruchu
    // wskazówek zegara patrząc z zewnątrz.
    const SCIANY: [[[f32; 3]; 4]; 6] = [
        [
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 1.0, 1.0],
            [1.0, 0.0, 1.0],
        ],
        [
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 1.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0],
        ],
        [
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 1.0],
            [1.0, 1.0, 1.0],
            [1.0, 1.0, 0.0],
        ],
        [
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 1.0],
        ],
        [
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
        ],
        [
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
        ],
    ];

    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for x in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let material = voxel(x, y, z);
                if material == 0 {
                    continue;
                }
                for (i, (kierunek, normal)) in KIERUNKI.iter().enumerate() {
                    if pelny(
                        x as i32 + kierunek[0],
                        y as i32 + kierunek[1],
                        z as i32 + kierunek[2],
                    ) {
                        continue;
                    }
                    let baza = vertices.len() as u32;
                    for naroznik in SCIANY[i] {
                        vertices.push(Vertex {
                            position: [
                                x as f32 + naroznik[0],
                                y as f32 + naroznik[1],
                                z as f32 + naroznik[2],
                            ],
                            normal: *normal,
                            palette_index: u32::from(material - 1),
                        });
                    }
                    indices.extend_from_slice(&[
                        baza,
                        baza + 1,
                        baza + 2,
                        baza,
                        baza + 2,
                        baza + 3,
                    ]);
                }
            }
        }
    }
    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn siatka_nie_jest_pusta_i_ma_spojne_indeksy() {
        let (v, i) = build();
        assert!(!v.is_empty(), "zahardkodowany chunk nie wygenerował ścian");
        assert_eq!(i.len() % 3, 0, "indeksy nie składają się w trójkąty");
        assert!(i.iter().all(|idx| (*idx as usize) < v.len()));
        // Ściany wewnętrzne muszą odpaść — inaczej siatka byłaby rzędu 6 × 32³.
        assert!(v.len() < 6 * 4 * CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE / 4);
    }

    #[test]
    fn wierzcholek_ma_rozmiar_zgodny_z_layoutem() {
        assert_eq!(size_of::<Vertex>(), 20);
        assert_eq!(Vertex::LAYOUT.array_stride, 20);
    }
}
