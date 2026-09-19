//! Siatka modelu voxelowego (M11a §5.1, WP1).
//!
//! Zamienia [`VoxModel`] na trójkąty dla jednego poziomu detalu. Meshing jest **naiwny —
//! jeden kwadrat na odsłoniętą ścianę voxela** — i to jest decyzja, nie zaniedbanie:
//! model postaci ma 200–400 voxeli, a `build_mesh` chunka (greedy, 32³) płaci za siebie
//! dopiero przy tysiącach. Tutaj scalanie kwadratów oszczędziłoby kilkadziesiąt
//! wierzchołków na model, a kosztowało drugi algorytm do utrzymania.
//!
//! Widoczność ściany liczy się **wewnątrz części**, nigdy między częściami. To wynika
//! wprost z tego, czym są części: obracają się niezależnie, więc ściana zasłonięta
//! w pozie spoczynkowej odsłania się przy pierwszym ruchu ramienia. Model z zaszytą
//! widocznością między częściami miałby dziury w ruchu — i byłaby to najgorsza klasa
//! błędu, bo statyczny zrzut ekranu wyglądałby poprawnie.
//!
//! ### Pozycje są w ćwiartkach voxela i **w pozie spoczynkowej**
//!
//! Wierzchołek niesie pozycję w 1/4 voxela (0,0625 m), z **wliczonym** przesunięciem
//! części względem korzenia (łańcuch pivotów). Dzięki temu M11a rysuje model jednym
//! wywołaniem, bez tablicy transformacji na GPU. Numer części jedzie mimo to w atrybucie
//! wierzchołka, a [`ModelMesh::part_rest_qv`] niesie te same przesunięcia po stronie CPU —
//! M11b, dokładając pozy, odejmuje je w shaderze zamiast liczyć od nowa.

use crate::model::{Part, VoxModel};

/// Wierzchołek modelu — 12 B.
///
/// `slot` zamiast koloru: barwę podstawia paleta instancji (§5.1). To jest ta sama
/// decyzja, przez którą 48 modeli pojazdów wystarcza na 3072 różne auta.
///
/// Układ jest **dobrany pod atrybuty wierzchołka `wgpu`**, a nie pod czytelność:
/// `Sint16x4` na przesunięciu 0 i `Uint8x4` na 8. Stąd wypełniacz po pozycji —
/// trzy `i16` nie mają odpowiadającego formatu atrybutu, a czwarty bajt i tak
/// przepadłby na wyrównaniu.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct ModelVertex {
    /// Pozycja w ćwiartkach voxela (0,0625 m), poza spoczynkowa.
    pub pos_qv: [i16; 3],
    pub _pad: i16,
    /// Numer ściany 0..5 w kolejności −X, +X, −Y, +Y, −Z, +Z.
    pub normal: u8,
    /// Indeks slotu palety 1..15 — tożsamość fragmentu w modelu.
    pub slot: u8,
    /// Numer części w tablicy modelu — dla animacji M11b.
    pub part: u8,
    /// Rola slotu ([`crate::SlotRole::as_index`]) **wypieczona w siatce**.
    ///
    /// Odwzorowanie slot → rola jest stałe dla modelu, więc trzymanie go osobno
    /// znaczyłoby drugie pośrednictwo w shaderze (`model, slot → rola`) przy zerowym
    /// zysku. Shader instancji potrzebuje roli i wariantu, żeby wybrać barwę
    /// z zestawu palety dzielnicy (`palette::pick`), i dostaje jedno i drugie wprost.
    pub role: u8,
}

/// Normalne ścian w kolejności zgodnej z `ModelVertex::normal`.
pub const FACE_NORMALS: [[i8; 3]; 6] = [
    [-1, 0, 0],
    [1, 0, 0],
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
];

/// Gotowa siatka jednego poziomu detalu.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ModelMesh {
    pub vertices: Vec<ModelVertex>,
    pub indices: Vec<u32>,
    /// Przesunięcie każdej części względem korzenia w pozie spoczynkowej,
    /// w ćwiartkach voxela. Indeks = numer części w [`VoxModel::parts`].
    pub part_rest_qv: Vec<[i16; 3]>,
    /// Bryła otaczająca w ćwiartkach voxela — do odcięcia stożkiem i do doboru LOD.
    pub bounds_qv: ([i16; 3], [i16; 3]),
}

impl ModelMesh {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Promień kuli otaczającej w metrach — wejście do testu stożka widzenia.
    #[must_use]
    pub fn bounding_radius_m(&self) -> f32 {
        let (min, max) = self.bounds_qv;
        let mut r2 = 0.0f32;
        for i in 0..3 {
            let d = f32::from(max[i] - min[i]) * 0.5;
            r2 += d * d;
        }
        // Ćwiartka voxela to 0,0625 m.
        r2.sqrt() * 0.0625
    }
}

/// Przesunięcia części względem korzenia: suma pivotów wzdłuż łańcucha rodziców.
///
/// Hierarchia jest uporządkowana od korzenia (pilnuje tego [`VoxModel::validate`]),
/// więc wystarczy jeden przebieg w przód — rodzic ma już policzone przesunięcie,
/// zanim dojdziemy do dziecka. Bez tego niezmiennika trzeba by stosu i wykrywania cykli.
#[must_use]
pub fn rest_offsets(model: &VoxModel) -> Vec<[i16; 3]> {
    let mut out: Vec<[i16; 3]> = Vec::with_capacity(model.parts.len());
    for p in &model.parts {
        let baza = if p.parent == Part::NO_PARENT {
            [0i16; 3]
        } else {
            out[p.parent as usize]
        };
        out.push([
            baza[0].saturating_add(p.pivot[0]),
            baza[1].saturating_add(p.pivot[1]),
            baza[2].saturating_add(p.pivot[2]),
        ]);
    }
    out
}

/// Buduje siatkę modelu dla zadanego poziomu detalu.
///
/// L2 jest impostorem i zwykle nie ma żadnej części — wynik jest wtedy pustą siatką
/// i to jest poprawna odpowiedź, nie błąd: impostor rysuje `ImpostorAtlas` (M11b §5.6),
/// a nie ten mesher.
#[must_use]
pub fn build_model_mesh(model: &VoxModel, lod: u8) -> ModelMesh {
    let rest = rest_offsets(model);
    let mut m = ModelMesh {
        part_rest_qv: rest.clone(),
        bounds_qv: ([i16::MAX; 3], [i16::MIN; 3]),
        ..Default::default()
    };

    for (pi, p) in model.parts_in_lod(lod) {
        let off = rest[pi];
        let (dx, dy, dz) = (
            i32::from(p.dims[0]),
            i32::from(p.dims[1]),
            i32::from(p.dims[2]),
        );
        for z in 0..dz {
            for y in 0..dy {
                for x in 0..dx {
                    let slot = p.at(x, y, z);
                    if slot == 0 {
                        continue;
                    }
                    for (face, n) in FACE_NORMALS.iter().enumerate() {
                        let (nx, ny, nz) = (
                            x + i32::from(n[0]),
                            y + i32::from(n[1]),
                            z + i32::from(n[2]),
                        );
                        if p.at(nx, ny, nz) != 0 {
                            continue;
                        }
                        let rola = model
                            .role_of(slot)
                            .map_or(0, |r| r.as_index() as u8);
                        push_face(
                            &mut m,
                            off,
                            [x, y, z],
                            face as u8,
                            (slot, rola),
                            pi as u8,
                        );
                    }
                }
            }
        }
    }

    if m.vertices.is_empty() {
        m.bounds_qv = ([0; 3], [0; 3]);
    }
    m
}

/// Cztery wierzchołki i sześć indeksów jednej ściany voxela.
fn push_face(
    m: &mut ModelMesh,
    off: [i16; 3],
    v: [i32; 3],
    face: u8,
    (slot, role): (u8, u8),
    part: u8,
) {
    // Róg voxela w ćwiartkach voxela: voxel zajmuje [4v, 4v+4].
    let baza = [
        off[0].saturating_add((v[0] * 4) as i16),
        off[1].saturating_add((v[1] * 4) as i16),
        off[2].saturating_add((v[2] * 4) as i16),
    ];
    // Kolejność rogów jest przeciwna do ruchu wskazówek zegara patrząc **z zewnątrz**
    // ściany, żeby odrzucanie tylnych ścian działało bez zgadywania.
    const ROGI: [[[u8; 3]; 4]; 6] = [
        [[0, 0, 0], [0, 0, 1], [0, 1, 1], [0, 1, 0]], // −X
        [[1, 0, 0], [1, 1, 0], [1, 1, 1], [1, 0, 1]], // +X
        [[0, 0, 0], [1, 0, 0], [1, 0, 1], [0, 0, 1]], // −Y
        [[0, 1, 0], [0, 1, 1], [1, 1, 1], [1, 1, 0]], // +Y
        [[0, 0, 0], [0, 1, 0], [1, 1, 0], [1, 0, 0]], // −Z
        [[0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]], // +Z
    ];
    let start = m.vertices.len() as u32;
    for rog in ROGI[face as usize] {
        let p = [
            baza[0] + i16::from(rog[0]) * 4,
            baza[1] + i16::from(rog[1]) * 4,
            baza[2] + i16::from(rog[2]) * 4,
        ];
        for (i, v) in p.iter().enumerate() {
            m.bounds_qv.0[i] = m.bounds_qv.0[i].min(*v);
            m.bounds_qv.1[i] = m.bounds_qv.1[i].max(*v);
        }
        m.vertices.push(ModelVertex {
            pos_qv: p,
            _pad: 0,
            normal: face,
            slot,
            part,
            role,
        });
    }
    m.indices
        .extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ModelFlags, ModelKind, PaletteSlot, PartName, SlotRole, VoxModel};

    /// Układ wierzchołka jest kontraktem z deklaracją atrybutów w `engine/render`:
    /// `Sint16x4` na 0, `Uint8x4` na 8. Zmiana rozmiaru przesuwa drugi atrybut
    /// i shader zaczyna czytać pozycję jako normalną — bez błędu walidacji.
    #[test]
    fn uklad_wierzcholka_jest_kontraktem_atrybutow() {
        assert_eq!(core::mem::size_of::<ModelVertex>(), 12);
        assert_eq!(core::mem::align_of::<ModelVertex>(), 2);
    }

    fn szescian(dims: [u8; 3], slot: u8) -> VoxModel {
        let n = dims[0] as usize * dims[1] as usize * dims[2] as usize;
        VoxModel {
            key: "kostka".into(),
            kind: ModelKind::Prop,
            flags: ModelFlags::default(),
            bbox: dims,
            parts: vec![crate::model::Part {
                name: PartName::BODY,
                parent: crate::model::Part::NO_PARENT,
                lod_mask: 0b111,
                pivot: [0; 3],
                dims,
                voxels: vec![slot; n],
            }],
            slots: vec![PaletteSlot {
                slot,
                role: SlotRole::Paint,
            }],
        }
    }

    /// Pełne pudełko ma odsłonięte wyłącznie ściany zewnętrzne: 2·(xy + xz + yz)
    /// kwadratów. Gdyby mesher nie sprawdzał sąsiada, wyszłoby 6·n.
    #[test]
    fn pelne_pudelko_daje_tylko_sciany_zewnetrzne() {
        let m = build_model_mesh(&szescian([4, 3, 2], 1), 0);
        let kwadratow = m.indices.len() / 6;
        assert_eq!(kwadratow, 2 * (4 * 3 + 4 * 2 + 3 * 2));
        assert_eq!(m.vertices.len(), kwadratow * 4);
    }

    #[test]
    fn pusty_model_nie_produkuje_trojkatow() {
        let m = build_model_mesh(&szescian([3, 3, 3], 0), 0);
        assert!(m.is_empty());
        assert_eq!(m.bounds_qv, ([0; 3], [0; 3]));
    }

    /// Poziom detalu wybiera części po masce, a nie po kolejności.
    #[test]
    fn lod_wybiera_czesci_po_masce() {
        let mut model = crate::model::tests_support::dwie_czesci();
        model.parts[1].lod_mask = 0b001; // głowa tylko w L0
        let l0 = build_model_mesh(&model, 0);
        let l1 = build_model_mesh(&model, 1);
        assert!(l0.indices.len() > l1.indices.len());
        assert!(!l1.is_empty(), "L1 stracił wszystko");
    }

    /// Przesunięcie części to suma pivotów wzdłuż łańcucha rodziców. Głowa siedzi
    /// nad torsem, a nie w jego początku układu.
    #[test]
    fn przesuniecia_sumuja_sie_wzdluz_hierarchii() {
        let model = crate::model::tests_support::dwie_czesci();
        let off = rest_offsets(&model);
        assert_eq!(off[0], model.parts[0].pivot);
        assert_eq!(
            off[1],
            [
                model.parts[0].pivot[0] + model.parts[1].pivot[0],
                model.parts[0].pivot[1] + model.parts[1].pivot[1],
                model.parts[0].pivot[2] + model.parts[1].pivot[2],
            ]
        );
    }

    /// Widoczność liczy się wewnątrz części. Dwie części stykające się ścianami
    /// **nie** kasują sobie nawzajem geometrii — inaczej ruch ramienia odsłaniałby dziurę.
    #[test]
    fn czesci_nie_kasuja_sobie_scian() {
        let mut model = szescian([2, 2, 2], 1);
        let mut druga = model.parts[0].clone();
        druga.name = PartName::HEAD;
        druga.parent = 0;
        // Dokładnie nad pierwszą: styk ścianą +Z / −Z.
        druga.pivot = [0, 0, 8];
        model.parts.push(druga);
        let m = build_model_mesh(&model, 0);
        // Dwa niezależne pudełka 2×2×2: po 24 kwadraty każde.
        assert_eq!(m.indices.len() / 6, 48);
    }
}
