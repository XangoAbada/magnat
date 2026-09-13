//! Greedy meshing z voxelowym AO (M1 §5.3, WP-V2).
//!
//! Sześć przebiegów (3 osie × 2 zwroty). Dla każdego z 32 przekrojów budujemy maskę
//! widocznych ścian i scalamy je w prostokąty — ale **tylko przy identycznej czwórce**
//! `(materiał, AO w czterech rogach, światło słoneczne, światło punktowe)`. To ograniczenie
//! fragmentuje quady na zboczach i jest świadomym kosztem (M1 §5.3): AO liczone per róg
//! jest jedynym powodem, dla którego voxelowy teren nie wygląda jak plastikowe klocki.
//!
//! **Sąsiedztwo na granicy chunka** bierze się z otoczki `ChunkBuilder` (`pad = 1`), czyli
//! z tego samego `ColumnSource`, a nie z sąsiedniego chunka. Dzięki temu mesh chunka nie
//! zależy od kolejności ładowania sąsiadów i nie wymaga przeliczania, gdy sąsiad się pojawi.

use crate::chunk::{ChunkBuilder, CHUNK_DIM};
use crate::material::{MaterialId, MaterialRegistry};

/// Wierzchołek w formacie 8-bajtowym (M1 §5.3).
///
/// ```text
/// lo: u32   x:6 | y:6 | z:6 | ao:2 | normal:3 | _:9
/// hi: u32   material:16 | sun:4 | block:4 | _:8
/// ```
///
/// Pozycja bazowa chunka i skala LOD idą w per-chunk SSBO, nie w wierzchołku — inaczej
/// każdy wierzchołek nosiłby 8 bajtów tej samej liczby.
/// `Pod` i `repr(C)`, bo ten typ trafia **bez konwersji** do bufora GPU. Gdyby układ pól
/// zależał od kompilatora, format wierzchołka z §5.3 przestałby być kontraktem z shaderem.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PackedVertex {
    pub lo: u32,
    pub hi: u32,
}

impl PackedVertex {
    // Osiem argumentów, bo tyle pól ma format z §5.3. Opakowanie ich w strukturę
    // pośrednią dodałoby typ istniejący wyłącznie po to, żeby zaraz się rozpakować.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn new(
        x: u32,
        y: u32,
        z: u32,
        ao: u8,
        normal: u8,
        material: MaterialId,
        sun: u8,
        block: u8,
    ) -> PackedVertex {
        let lo = (x & 0x3F)
            | ((y & 0x3F) << 6)
            | ((z & 0x3F) << 12)
            | (((ao as u32) & 0x3) << 18)
            | (((normal as u32) & 0x7) << 20);
        let hi =
            (material.0 as u32) | (((sun as u32) & 0xF) << 16) | (((block as u32) & 0xF) << 20);
        PackedVertex { lo, hi }
    }

    #[must_use]
    pub const fn position(self) -> (u32, u32, u32) {
        (
            self.lo & 0x3F,
            (self.lo >> 6) & 0x3F,
            (self.lo >> 12) & 0x3F,
        )
    }

    #[must_use]
    pub const fn ao(self) -> u8 {
        ((self.lo >> 18) & 0x3) as u8
    }

    #[must_use]
    pub const fn normal(self) -> u8 {
        ((self.lo >> 20) & 0x7) as u8
    }

    #[must_use]
    pub const fn material(self) -> MaterialId {
        MaterialId((self.hi & 0xFFFF) as u16)
    }
}

/// Sześć kierunków ścian. Kolejność jest kontraktem shadera: indeks trafia do wierzchołka
/// jako `normal` i po nim pipeline odtwarza wektor normalny bez trzymania go w danych.
pub const NORMALS: [(i32, i32, i32); 6] = [
    (1, 0, 0),
    (-1, 0, 0),
    (0, 1, 0),
    (0, -1, 0),
    (0, 0, 1),
    (0, 0, -1),
];

/// Gotowy mesh chunka.
///
/// Indeksy są **posegregowane**: najpierw wszystkie ściany nieprzezroczyste, potem
/// przezroczyste. Renderer rysuje je dwoma passami (§5.8: woda ma własny przebieg
/// z mieszaniem i bez zapisu głębi), a segregacja po stronie meshingu oszczędza mu
/// przeglądania geometrii co klatkę — materiał ściany nie zmienia się między klatkami.
#[derive(Clone, Default, Debug)]
pub struct ChunkMesh {
    pub vertices: Vec<PackedVertex>,
    pub indices: Vec<u32>,
    /// Ile pierwszych indeksów należy do geometrii nieprzezroczystej.
    pub opaque_indices: u32,
}

impl ChunkMesh {
    #[must_use]
    pub fn quad_count(&self) -> usize {
        self.vertices.len() / 4
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    #[must_use]
    pub fn bytes(&self) -> usize {
        self.vertices.len() * std::mem::size_of::<PackedVertex>()
            + self.indices.len() * std::mem::size_of::<u32>()
    }
}

/// Klucz scalania: dwie ściany łączą się w jeden prostokąt tylko przy pełnej zgodności.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
struct Face {
    material: MaterialId,
    ao: [u8; 4],
    sun: u8,
    block: u8,
    visible: bool,
}

/// Buduje mesh z zawartości buildera. Builder musi mieć otoczkę (`pad >= 1`), inaczej
/// ściany na granicy chunka policzą się tak, jakby za nią była pustka.
#[must_use]
pub fn build_mesh(b: &ChunkBuilder, reg: &MaterialRegistry) -> ChunkMesh {
    assert!(
        b.pad() >= 1,
        "meshing wymaga otoczki: bez niej ściany na granicy chunka wiszą w powietrzu"
    );
    let dim = CHUNK_DIM as i32;
    let mut mesh = ChunkMesh::default();

    // Czy voxel w ogóle daje geometrię. Powietrze nie daje; woda **daje** —
    // bez tego jeziora i rzeki są dziurami, przez które widać niebo, i wygląda to
    // dokładnie tak, jak brzmi.
    let renders = |x: i32, y: i32, z: i32| -> MaterialId { b.material_at(x, y, z) };
    // Czy sąsiad zasłania ścianę. Zasłania materiał nieprzezroczysty oraz **ten sam
    // materiał** — dzięki temu tafla wody nie produkuje ściany na każdej granicy voxela,
    // a dno pod wodą nadal istnieje i przegrywa z nią na buforze głębi.
    let blocks = |wlasny: MaterialId, x: i32, y: i32, z: i32| -> bool {
        let n = b.material_at(x, y, z);
        n == wlasny || (!n.is_air() && reg.get(n).is_opaque())
    };
    // Zasłanianie na potrzeby AO: liczy się to, co blokuje światło, czyli nieprzezroczyste.
    let solid = |x: i32, y: i32, z: i32| -> bool {
        let m = b.material_at(x, y, z);
        !m.is_air() && reg.get(m).is_opaque()
    };

    // Indeksy przezroczyste zbierane osobno i doklejane na końcu — patrz `ChunkMesh`.
    let mut przezroczyste: Vec<u32> = Vec::new();

    for (n, (nx, ny, nz)) in NORMALS.iter().enumerate() {
        // Osie płaszczyzny przekroju: `u` i `v` to dwie osie prostopadłe do normalnej.
        let axis = if *nx != 0 {
            0
        } else if *ny != 0 {
            1
        } else {
            2
        };
        // Osie płaszczyzny przekroju muszą tworzyć z normalną układ **prawoskrętny**,
        // inaczej quad wychodzi nawinięty odwrotnie i wypada przy odrzucaniu tylnych ścianek.
        // Dla osi Y prawoskrętna para to (Z, X), nie (X, Z) — bo Y = Z × X. Pierwsza wersja
        // brała (X, Z) i skutek był taki, że **wszystkie** ściany zwrócone na północ i południe
        // znikały: teren pokrywał się siatką dziur wzdłuż poziomic, przez które widać niebo.
        let (ua, va) = match axis {
            0 => (1usize, 2usize), // X = Y × Z
            1 => (2, 0),           // Y = Z × X
            _ => (0, 1),           // Z = X × Y
        };

        let mut mask = vec![Face::default(); (dim * dim) as usize];
        for slice in 0..dim {
            // ── Maska widocznych ścian w przekroju ───────────────────────────────────
            for v in 0..dim {
                for u in 0..dim {
                    let mut p = [0i32; 3];
                    p[axis] = slice;
                    p[ua] = u;
                    p[va] = v;

                    let material = renders(p[0], p[1], p[2]);
                    let widoczna =
                        !material.is_air() && !blocks(material, p[0] + nx, p[1] + ny, p[2] + nz);
                    let idx = (v * dim + u) as usize;
                    if !widoczna {
                        mask[idx] = Face::default();
                        continue;
                    }
                    mask[idx] = Face {
                        material,
                        ao: corner_ao(&solid, p, (*nx, *ny, *nz), ua, va),
                        // Oświetlenie: M1 nie ma jeszcze propagacji światła, więc każda
                        // ściana dostaje pełne słońce i zero światła punktowego. Format
                        // wierzchołka niesie oba pola od początku, żeby M11 nie musiał
                        // przebudowywać ani wierzchołka, ani areny GPU.
                        sun: 15,
                        block: 0,
                        visible: true,
                    };
                }
            }

            // ── Scalanie zachłanne ───────────────────────────────────────────────────
            for v in 0..dim {
                let mut u = 0;
                while u < dim {
                    let start = mask[(v * dim + u) as usize];
                    if !start.visible {
                        u += 1;
                        continue;
                    }
                    // Szerokość: dokąd sięga identyczna czwórka.
                    let mut w = 1;
                    while u + w < dim && mask[(v * dim + u + w) as usize] == start {
                        w += 1;
                    }
                    // Wysokość: ile pełnych wierszy o tej samej szerokości.
                    let mut h = 1;
                    'wzwyz: while v + h < dim {
                        for k in 0..w {
                            if mask[((v + h) * dim + u + k) as usize] != start {
                                break 'wzwyz;
                            }
                        }
                        h += 1;
                    }

                    let cel = if reg.get(start.material).is_opaque() {
                        None
                    } else {
                        Some(&mut przezroczyste)
                    };
                    emit_quad(
                        &mut mesh, cel, axis, ua, va, slice, u, v, w, h, n as u8, start,
                    );

                    for dv in 0..h {
                        for du in 0..w {
                            mask[((v + dv) * dim + u + du) as usize] = Face::default();
                        }
                    }
                    u += w;
                }
            }
        }
    }
    mesh.opaque_indices = mesh.indices.len() as u32;
    mesh.indices.extend_from_slice(&przezroczyste);
    mesh
}

/// AO per róg z trzech sąsiadów rogu: dwa boczne i narożny (M1 §5.3).
///
/// Reguła jest klasyczna i nieprzypadkowa: gdy oba boczne są pełne, róg jest w całości
/// zasłonięty (0) niezależnie od narożnego — bo światło i tak nie ma którędy wejść.
fn corner_ao(
    solid: &impl Fn(i32, i32, i32) -> bool,
    p: [i32; 3],
    n: (i32, i32, i32),
    ua: usize,
    va: usize,
) -> [u8; 4] {
    // Cztery rogi ściany w kolejności (0,0), (1,0), (1,1), (0,1) w osiach (u, v).
    const ROGI: [(i32, i32); 4] = [(-1, -1), (1, -1), (1, 1), (-1, 1)];
    let mut out = [3u8; 4];
    for (i, (du, dv)) in ROGI.iter().enumerate() {
        let przesun = |du: i32, dv: i32| {
            let mut q = [p[0] + n.0, p[1] + n.1, p[2] + n.2];
            q[ua] += du;
            q[va] += dv;
            q
        };
        let a = przesun(*du, 0);
        let b = przesun(0, *dv);
        let c = przesun(*du, *dv);
        let side1 = u8::from(solid(a[0], a[1], a[2]));
        let side2 = u8::from(solid(b[0], b[1], b[2]));
        let corner = u8::from(solid(c[0], c[1], c[2]));
        out[i] = if side1 == 1 && side2 == 1 {
            0
        } else {
            3 - side1 - side2 - corner
        };
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn emit_quad(
    mesh: &mut ChunkMesh,
    // `Some` dla ścian przezroczystych: ich indeksy idą do osobnej listy, doklejanej
    // na końcu. Wierzchołki zostają w jednej tablicy — dzielenie ich na dwie tylko po to,
    // żeby rozdzielić passy, podwoiłoby liczbę alokacji w arenie GPU.
    osobno: Option<&mut Vec<u32>>,
    axis: usize,
    ua: usize,
    va: usize,
    slice: i32,
    u: i32,
    v: i32,
    w: i32,
    h: i32,
    normal: u8,
    face: Face,
) {
    // Ściana dodatnia leży o jeden voxel dalej wzdłuż osi niż sam voxel.
    let base_slice = if normal.is_multiple_of(2) {
        slice + 1
    } else {
        slice
    };
    let mut rogi = [[0i32; 3]; 4];
    let offsets = [(0, 0), (w, 0), (w, h), (0, h)];
    for (i, (du, dv)) in offsets.iter().enumerate() {
        rogi[i][axis] = base_slice;
        rogi[i][ua] = u + du;
        rogi[i][va] = v + dv;
    }

    let first = mesh.vertices.len() as u32;
    for (i, r) in rogi.iter().enumerate() {
        mesh.vertices.push(PackedVertex::new(
            r[0] as u32,
            r[1] as u32,
            r[2] as u32,
            face.ao[i],
            normal,
            face.material,
            face.sun,
            face.block,
        ));
    }

    // Przekątna quada wybierana po AO. Bez tego na narożnikach powstaje artefakt
    // „anizotropii": interpolacja po przekątnej rozjeżdża cieniowanie w jedną stronę
    // i płaska ściana wygląda jak zagięta.
    let flip = i32::from(face.ao[0]) + i32::from(face.ao[2])
        < i32::from(face.ao[1]) + i32::from(face.ao[3]);
    let kolejnosc: [u32; 6] = if flip {
        [1, 2, 3, 1, 3, 0]
    } else {
        [0, 1, 2, 0, 2, 3]
    };
    // Zwrot nawijania zależy od kierunku ściany — inaczej połowa ścian byłaby odrzucona
    // przez culling tylnych ścianek.
    let odwroc = !normal.is_multiple_of(2);
    let docelowe = osobno.unwrap_or(&mut mesh.indices);
    for k in 0..2 {
        let t = [kolejnosc[k * 3], kolejnosc[k * 3 + 1], kolejnosc[k * 3 + 2]];
        if odwroc {
            docelowe.extend_from_slice(&[first + t[0], first + t[2], first + t[1]]);
        } else {
            docelowe.extend_from_slice(&[first + t[0], first + t[1], first + t[2]]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::ChunkCoord;

    fn rejestr() -> MaterialRegistry {
        MaterialRegistry::load_dir(std::path::Path::new("../../data/materials"))
            .expect("data/materials")
    }

    /// Chunk z płaskim terenem o zadanej wysokości.
    fn plaski(h: i32, m: MaterialId) -> ChunkBuilder {
        let mut b = ChunkBuilder::new(ChunkCoord::new(0, 0, 0), 0, 1);
        let r = b.range();
        for y in r.clone() {
            for x in r.clone() {
                b.fill_column(x, y, -1, h, m);
            }
        }
        b
    }

    #[test]
    fn plaski_teren_scala_sie_do_jednego_quada_od_gory() {
        let reg = rejestr();
        let kamien = reg.expect_id("granite");
        let b = plaski(10, kamien);
        let mesh = build_mesh(&b, &reg);

        // Ściany boczne nie powstają (otoczka jest pełna), spód też nie (otoczka niżej).
        // Zostaje jedna ściana górna — i cały sens greedy meshingu polega na tym,
        // że jest **jednym** quadem, a nie 1024.
        assert_eq!(
            mesh.quad_count(),
            1,
            "płaski teren dał {} quadów",
            mesh.quad_count()
        );
        assert_eq!(mesh.indices.len(), 6);
        let (_, _, z) = mesh.vertices[0].position();
        assert_eq!(z, 10, "ściana górna nie leży na wysokości terenu");
        assert_eq!(mesh.vertices[0].material(), kamien);
        assert_eq!(mesh.vertices[0].normal(), 4, "normalna nie wskazuje w górę");
        for v in &mesh.vertices {
            assert_eq!(v.ao(), 3, "płaska ściana bez zasłonięcia ma mieć pełne AO");
        }
    }

    #[test]
    fn format_wierzcholka_pakuje_sie_bez_strat() {
        for (x, y, z) in [(0u32, 0u32, 0u32), (32, 32, 32), (17, 3, 31)] {
            for ao in 0..4u8 {
                for n in 0..6u8 {
                    let v = PackedVertex::new(x, y, z, ao, n, MaterialId(65535), 15, 15);
                    assert_eq!(v.position(), (x, y, z));
                    assert_eq!(v.ao(), ao);
                    assert_eq!(v.normal(), n);
                    assert_eq!(v.material(), MaterialId(65535));
                }
            }
        }
        assert_eq!(
            std::mem::size_of::<PackedVertex>(),
            8,
            "wierzchołek ma mieć 8 B"
        );
    }

    /// Znak orientacji trójkąta rzutowanego na płaszczyznę prostopadłą do normalnej.
    /// Dodatni = nawinięcie przeciwne do ruchu wskazówek zegara patrząc z kierunku normalnej,
    /// czyli ściana przednia przy `front_face: Ccw`.
    fn orientacja(mesh: &ChunkMesh, tri: usize) -> f32 {
        let idx = [
            mesh.indices[tri * 3] as usize,
            mesh.indices[tri * 3 + 1] as usize,
            mesh.indices[tri * 3 + 2] as usize,
        ];
        let p: Vec<[f32; 3]> = idx
            .iter()
            .map(|i| {
                let (x, y, z) = mesh.vertices[*i].position();
                [x as f32, y as f32, z as f32]
            })
            .collect();
        let n = NORMALS[mesh.vertices[idx[0]].normal() as usize];
        let (u, v) = (
            [p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]],
            [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]],
        );
        let cross = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        cross[0] * n.0 as f32 + cross[1] * n.1 as f32 + cross[2] * n.2 as f32
    }

    #[test]
    fn kazda_sciana_jest_nawinieta_przodem_do_swojej_normalnej() {
        // To jest test, którego brak kosztował najwięcej: przy złej orientacji ściany
        // **znikają** przy odrzucaniu tylnych ścianek, a w kodzie meshingu wszystko wygląda
        // poprawnie — błąd widać dopiero na ekranie, jako dziury w terenie.
        let reg = rejestr();
        let kamien = reg.expect_id("granite");
        let mut b = ChunkBuilder::new(ChunkCoord::new(0, 0, 0), 0, 1);
        // Schodki we wszystkich czterech kierunkach poziomych plus góra i dół.
        for y in 0..CHUNK_DIM as i32 {
            for x in 0..CHUNK_DIM as i32 {
                let h = 8 + (x / 4) - (y / 4);
                b.fill_column(x, y, 0, h.max(1), kamien);
            }
        }
        let mesh = build_mesh(&b, &reg);
        assert!(
            mesh.quad_count() > 50,
            "za mało ścian, test nic nie sprawdza"
        );

        let mut widziane = [0u32; 6];
        for tri in 0..mesh.indices.len() / 3 {
            let o = orientacja(&mesh, tri);
            let n = mesh.vertices[mesh.indices[tri * 3] as usize].normal();
            widziane[n as usize] += 1;
            assert!(
                o > 0.0,
                "trójkąt {tri} ściany o normalnej {n} jest nawinięty tyłem (orientacja {o})"
            );
        }
        // Wszystkie sześć kierunków muszą wystąpić — inaczej test przechodzi, bo brakującej
        // ściany nie ma czym sprawdzić.
        for (n, ile) in widziane.iter().enumerate() {
            assert!(*ile > 0, "brak ścian o normalnej {n}");
        }
    }

    #[test]
    fn pojedynczy_voxel_daje_szesc_scian() {
        let reg = rejestr();
        let kamien = reg.expect_id("granite");
        let mut b = ChunkBuilder::new(ChunkCoord::new(0, 0, 0), 0, 1);
        b.set(5, 5, 5, kamien);
        let mesh = build_mesh(&b, &reg);
        assert_eq!(mesh.quad_count(), 6, "sześcian ma sześć ścian");
        // Każda normalna dokładnie raz.
        let mut widziane = [0u32; 6];
        for q in 0..6 {
            widziane[mesh.vertices[q * 4].normal() as usize] += 1;
        }
        assert_eq!(widziane, [1; 6]);
    }

    #[test]
    fn ao_ciemnieje_przy_scianie() {
        let reg = rejestr();
        let kamien = reg.expect_id("granite");
        let mut b = plaski(10, kamien);
        // Słupek obok: rogi ściany górnej stykające się z nim mają być ciemniejsze.
        b.fill_column(16, 16, 10, 14, kamien);
        let mesh = build_mesh(&b, &reg);

        let mut min_ao = 3u8;
        for v in &mesh.vertices {
            if v.normal() == 4 {
                min_ao = min_ao.min(v.ao());
            }
        }
        assert!(
            min_ao < 3,
            "słupek nie rzucił zasłonięcia na sąsiednie rogi"
        );
    }

    #[test]
    fn mesh_nie_przecieka_miedzy_chunkami() {
        // M1 §4 WP-V2: brak dziur na styku. Chunk wypełniony po brzegi, z otoczką
        // również pełną, nie może wyprodukować ani jednej ściany bocznej.
        let reg = rejestr();
        let kamien = reg.expect_id("granite");
        let mut b = ChunkBuilder::new(ChunkCoord::new(0, 0, 0), 0, 1);
        let r = b.range();
        for y in r.clone() {
            for x in r.clone() {
                for z in r.clone() {
                    b.set(x, y, z, kamien);
                }
            }
        }
        let mesh = build_mesh(&b, &reg);
        assert!(
            mesh.is_empty(),
            "chunk w litej skale wyprodukował {} quadów",
            mesh.quad_count()
        );
    }

    #[test]
    fn woda_ma_swoja_tafle_i_nie_zaslania_dna() {
        let reg = rejestr();
        let kamien = reg.expect_id("granite");
        let woda = reg.expect_id("water");
        let mut b = plaski(10, kamien);
        let r = b.range();
        for y in r.clone() {
            for x in r.clone() {
                b.fill_column(x, y, 10, 13, woda);
            }
        }
        let mesh = build_mesh(&b, &reg);
        // Ściana górna skały pozostaje widoczna mimo wody nad nią — inaczej dno rzeki
        // znikałoby, gdy tylko ją zalejemy.
        assert!(
            mesh.vertices
                .iter()
                .any(|v| v.material() == kamien && v.normal() == 4),
            "woda zasłoniła dno"
        );
        // I sama tafla istnieje. To nie jest asercja symetryczna dla ozdoby: pierwsza
        // wersja testu sprawdzała wyłącznie dno, więc przeszła również wtedy, gdy woda
        // nie dawała **żadnej** ściany — a na ekranie jezioro było dziurą do nieba.
        assert!(
            mesh.vertices
                .iter()
                .any(|v| v.material() == woda && v.normal() == 4),
            "tafla wody nie powstała"
        );
        // Wnętrze wody się nie mesh-uje: tafla to jeden quad od góry, nie trzy warstwy.
        let tafla = mesh
            .vertices
            .iter()
            .filter(|v| v.material() == woda && v.normal() == 4)
            .count();
        assert_eq!(tafla, 4, "tafla rozbita na warstwy zamiast jednego quada");

        // Indeksy wody leżą **za** granicą `opaque_indices`: renderer rysuje je osobnym
        // passem z mieszaniem, więc pomieszanie obu list oznaczałoby wodę rysowaną
        // nieprzezroczyście albo skałę rysowaną z mieszaniem.
        let nieprzezroczyste = &mesh.indices[..mesh.opaque_indices as usize];
        assert!(
            nieprzezroczyste
                .iter()
                .all(|i| mesh.vertices[*i as usize].material() != woda),
            "woda trafiła do geometrii nieprzezroczystej"
        );
        let reszta = &mesh.indices[mesh.opaque_indices as usize..];
        assert!(!reszta.is_empty(), "brak indeksów przezroczystych");
        assert!(
            reszta
                .iter()
                .all(|i| mesh.vertices[*i as usize].material() == woda),
            "do listy przezroczystej trafiło coś poza wodą"
        );
    }

    #[test]
    fn gladkie_zbocze_scala_sie_w_pasy() {
        // Zbocze o stałym spadku: greedy meshing ma z niego zrobić pasy, nie siatkę
        // pojedynczych ścianek. Liczba quadów rzędu liczby stopni, a nie liczby voxeli.
        let reg = rejestr();
        let skala = reg.expect_id("granite");
        let mut b = ChunkBuilder::new(ChunkCoord::new(0, 0, 0), 0, 1);
        let r = b.range();
        for y in r.clone() {
            for x in r.clone() {
                b.fill_column(x, y, -1, 8 + x / 4, skala);
            }
        }
        let mesh = build_mesh(&b, &reg);
        // 9 stopni w poziomie: 9 pasów ściany górnej + 8 ścianek pionowych między nimi,
        // z zapasem na fragmentację AO na krawędziach.
        assert!(
            mesh.quad_count() < 200,
            "gładkie zbocze dało {} quadów zamiast kilkudziesięciu",
            mesh.quad_count()
        );
        assert!(
            mesh.quad_count() >= 17,
            "zbocze ma mieć stopnie, nie być płaskie"
        );
    }
}
