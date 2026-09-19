//! Impostory encji — wypalone sylwetki zastępujące model w oddali (M11b §5.6, WP4).
//!
//! Od ok. 120 m postać ma na ekranie kilkanaście pikseli, a kosztuje tyle samo
//! wierzchołków co pod nosem. Impostor zamienia ją w **jeden kafel tekstury**: dwa
//! trójkąty zamiast czterystu, jedno wywołanie rysowania na cały tłum.
//!
//! ### Kafel niesie rolę slotu, nie kolor
//!
//! To jest ta sama decyzja, która sprawia, że model nie zna barwy (`E-7`, `F-1`):
//! kanał `R` trzyma **rolę slotu palety**, a barwę podstawia fragment shader tą samą
//! funkcją `pick(wariant, rola)`, którą liczy ją dla pełnej geometrii. Dzięki temu tłum
//! na trzystu metrach ma nadal zróżnicowane ubrania, a nie jednolitą szarość — i dzięki
//! temu przejście L1 → L2 nie zmienia koloru ani jednej postaci.
//!
//! **Konsekwencja, której plan nie przewidywał:** §5.6 liczył kafle mieszkańca jako
//! „32 `outfit_class` × 8 kierunków × 4 fazy = 1024" i wychodziło 6,3 MB. Klasa ubrania
//! **nie zmienia kształtu sylwetki** — zmienia barwę, a barwy w kaflu nie ma. Wymiar
//! odpada, zostaje 8 × 4 = 32 kafle na model. Atlas obu modeli waży poniżej megabajta
//! zamiast trzynastu.
//!
//! ### Wypalanie jest rasteryzacją po stronie procesora
//!
//! Bez GPU, bo atlas encji nie zależy od świata — zależy wyłącznie od modelu i klipu,
//! więc da się go policzyć raz przy starcie i sprawdzić testem w CI, który karty
//! graficznej nie ma. Rasteryzacja jest barycentryczna z buforem głębi: kilkaset
//! trójkątów na kafel, kilkadziesiąt kafli, czyli milisekundy przy starcie.

use crate::anim::{AnimationClip, ClipLibrary, PoseTexel, POSE_TRANS_SCALE};
use crate::model::{ModelId, ModelKind, ModelLibrary};
use crate::model_mesh::{build_model_mesh, ModelMesh, FACE_NORMALS};
use crate::PoseAtlas;

/// Szerokość i wysokość kafla w pikselach. **Jeden rozmiar dla wszystkich**, bo warstwy
/// `Texture2DArray` muszą mieć ten sam wymiar; §5.6 podawał 32×48 dla postaci i 64×32
/// dla pojazdu, więc kafel jest sumą obu potrzeb, a nieużyta część zostaje przezroczysta.
pub const TILE_W: u32 = 64;
pub const TILE_H: u32 = 48;

/// Ile azymutów wypala się dla postaci i dla pojazdu (§5.6).
pub const CHARACTER_DIRS: u8 = 8;
pub const VEHICLE_DIRS: u8 = 16;
/// Ile faz chodu niesie impostor postaci.
pub const CHARACTER_FRAMES: u8 = 4;

/// Kanał `R`: rola slotu **przesunięta o jeden**, bo zero znaczy pustkę.
/// Kanał `G`: numer ściany voxela przesunięty o jeden — normalną odtwarza shader.
/// Kanał `B`: zapas (głębokość dla miękkiego przecięcia — M11e, jeśli pomiar ją uzasadni).
/// Kanał `A`: 255 tam, gdzie sylwetka jest, 0 poza nią.
pub const CHANNELS: usize = 4;

/// Gdzie w atlasie leżą kafle jednego modelu i jak duży jest jego billboard.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct ImpostorEntry {
    /// Pierwsza warstwa tekstury; kafle idą w kolejności `kierunek × faza`.
    pub base_layer: u32,
    pub dirs: u8,
    pub frames: u8,
    /// O ile bitów przesunąć fazę klipu, żeby trafić w kafel.
    ///
    /// Kafle są wypalane z **co n-tej** klatki klipu (16 klatek chodu na 4 fazy), więc
    /// faza z rekordu tyka cztery razy szybciej, niż zmienia się sylwetka. Bez tego
    /// przesunięcia tłum na impostorach przebiera nogami czterokrotnie szybciej niż
    /// ten sam tłum metr bliżej — czyli dokładnie przeskok, którego zabrania kryterium.
    pub frame_shift: u8,
    /// Szerokość i wysokość billboardu w metrach — **te same, co bryła modelu**.
    /// Stąd bierze się brak przeskoku wielkości przy przejściu L1 → L2.
    ///
    /// Dolnej krawędzi nie ma w tej strukturze, bo modele stoją na gruncie: `min.z`
    /// siatki jest zerem i pilnuje tego test w `mvoxc`. Pole, które zawsze ma jedną
    /// wartość, jest miejscem, w którym ktoś kiedyś wpisze drugą i nikt tego nie sprawdzi.
    pub size_m: [f32; 2],
}

impl ImpostorEntry {
    #[must_use]
    pub const fn layers(&self) -> u32 {
        self.dirs as u32 * self.frames as u32
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.dirs == 0 || self.frames == 0
    }
}

/// Atlas sylwetek wszystkich modeli. Wypalany raz, nigdy nie zmieniany.
#[derive(Clone, Debug, Default)]
pub struct ImpostorAtlas {
    /// RGBA8, warstwa po warstwie, `TILE_W × TILE_H` każda.
    pixels: Vec<u8>,
    /// Wpis per `ModelId`.
    index: Vec<ImpostorEntry>,
}

impl ImpostorAtlas {
    /// Wypala kafle dla każdego modelu w katalogu.
    ///
    /// Postać dostaje cztery fazy klipu chodu — inaczej tłum na dwustu metrach stałby
    /// w miejscu, choć się przesuwa, co widać z daleka bardziej niż brak palców u rąk.
    /// Pojazd ma jedną fazę i dwa razy więcej azymutów, bo jest dłuższy niż szerszy
    /// i sylwetka zmienia się z kątem szybciej.
    #[must_use]
    pub fn bake(models: &ModelLibrary, clips: &ClipLibrary, poses: &PoseAtlas) -> ImpostorAtlas {
        let mut atlas = ImpostorAtlas {
            pixels: Vec::new(),
            index: vec![ImpostorEntry::default(); models.len()],
        };
        for (id, model) in models.iter() {
            let (dirs, frames, clip) = match model.kind {
                ModelKind::Character => (
                    CHARACTER_DIRS,
                    CHARACTER_FRAMES,
                    Some(clips.id_of(crate::ClipKind::Walk)),
                ),
                ModelKind::Vehicle => (VEHICLE_DIRS, 1, None),
                // Propy, szyldy i maszyny nie mają poziomu impostora: prop wnętrza
                // znika razem z wnętrzem, a szyld czytany z trzystu metrów i tak
                // jest plamą. Wpis zostaje pusty i `lod_for` nigdy ich tam nie wyśle.
                _ => continue,
            };
            // Siatka L1 (uproszczona) zamiast L0: impostor ma oddać sylwetkę, a nie
            // szczegóły, których w kaflu 64×48 i tak nie widać.
            let mesh = build_model_mesh(model, 1);
            if mesh.is_empty() {
                continue;
            }
            let (size_m, base_z_m) = bryla(&mesh);
            let klip = clip.and_then(|c| clips.get(c).map(|k| (c, k)));
            // Klip 16-klatkowy na 4 kafle znaczy przesunięcie o 2 bity. Liczba klatek
            // klipu i liczba kafli są potęgami dwójki (pilnuje tego test biblioteki),
            // więc iloraz też nią jest i mieści się w przesunięciu.
            let frame_shift = klip.map_or(0, |(_, k)| {
                (u32::from(k.frames) / u32::from(frames)).trailing_zeros()
            }) as u8;
            let entry = ImpostorEntry {
                base_layer: (atlas.pixels.len() / (TILE_W as usize * TILE_H as usize * CHANNELS))
                    as u32,
                dirs,
                frames,
                frame_shift,
                size_m,
            };
            for d in 0..dirs {
                for f in 0..frames {
                    let azymut = f32::from(d) / f32::from(dirs) * std::f32::consts::TAU;
                    let poza = klip.map(|(cid, k): (crate::ClipId, &AnimationClip)| {
                        // Cztery fazy rozłożone równo po klipie — nie cztery pierwsze
                        // klatki, bo te różnią się od siebie o kilka stopni.
                        let klatka = u32::from(f) * u32::from(k.frames) / u32::from(frames);
                        (poses, id, cid, klatka as u8)
                    });
                    atlas.wypal(&mesh, azymut, size_m, base_z_m, poza);
                }
            }
            atlas.index[id.0 as usize] = entry;
        }
        atlas
    }

    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    #[must_use]
    pub fn layers(&self) -> u32 {
        (self.pixels.len() / (TILE_W as usize * TILE_H as usize * CHANNELS)) as u32
    }

    #[must_use]
    pub fn bytes(&self) -> usize {
        self.pixels.len()
    }

    #[must_use]
    pub fn entry(&self, model: ModelId) -> ImpostorEntry {
        self.index
            .get(model.0 as usize)
            .copied()
            .unwrap_or_default()
    }

    /// Wpisy w kolejności `ModelId` — do bufora, który czyta shader.
    #[must_use]
    pub fn index(&self) -> &[ImpostorEntry] {
        &self.index
    }

    /// Piksel kafla — do testów i do podglądu w devtools.
    #[must_use]
    pub fn texel(&self, layer: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((layer * TILE_H + y) * TILE_W + x) as usize * CHANNELS;
        self.pixels
            .get(i..i + 4)
            .map_or([0; 4], |s| [s[0], s[1], s[2], s[3]])
    }

    fn wypal(
        &mut self,
        mesh: &ModelMesh,
        azymut: f32,
        size_m: [f32; 2],
        base_z_m: f32,
        poza: Option<(&PoseAtlas, ModelId, crate::ClipId, u8)>,
    ) {
        let start = self.pixels.len();
        self.pixels
            .resize(start + TILE_W as usize * TILE_H as usize * CHANNELS, 0);
        let mut depth = vec![f32::INFINITY; (TILE_W * TILE_H) as usize];

        // Układ kafla: kamera patrzy w kierunku `patrz` (czyli stoi po stronie `−patrz`),
        // oś pionowa w górę, a **prawa strona ekranu** to `patrz × góra`. Znak ma tu treść:
        // przy odwrotnym kafel wychodzi lustrzany i postać z daleka przebiera nogami
        // w drugą stronę, niż idzie.
        let (sa, ca) = azymut.sin_cos();
        let prawo = [sa, -ca, 0.0f32];
        let patrz = [ca, sa, 0.0f32];

        let mut trojkat = [[0.0f32; 3]; 3];
        let mut atrybuty = [(0u8, 0u8); 3];
        for (i, idx) in mesh.indices.iter().enumerate() {
            let v = mesh.vertices[*idx as usize];
            let mut p = [
                f32::from(v.pos_qv[0]) * QV_M,
                f32::from(v.pos_qv[1]) * QV_M,
                f32::from(v.pos_qv[2]) * QV_M,
            ];
            let mut normalna = v.normal;
            if let Some((atlas, id, clip, frame)) = poza {
                if let Some(t) = atlas.pose(id, clip, frame, v.part) {
                    p = zastosuj_poze(&t, p);
                    normalna = obroc_normalna(&t, normalna);
                }
            }
            let s = p[0] * prawo[0] + p[1] * prawo[1];
            let d = p[0] * patrz[0] + p[1] * patrz[1];
            // Piksel: `s` w [−w/2, w/2] → [0, W], `z` w [base, base + h] → [H, 0].
            trojkat[i % 3] = [
                (s / size_m[0] + 0.5) * TILE_W as f32,
                (1.0 - (p[2] - base_z_m) / size_m[1]) * TILE_H as f32,
                d,
            ];
            atrybuty[i % 3] = (v.role, normalna);
            if i % 3 == 2 {
                rasteryzuj(&mut self.pixels[start..], &mut depth, &trojkat, atrybuty[0]);
            }
        }
    }
}

/// Ćwiartka voxela w metrach.
const QV_M: f32 = 0.0625;

/// Szerokość i wysokość billboardu oraz jego dolna krawędź — z bryły otaczającej.
///
/// Szerokość bierze **przekątną** rzutu poziomego, a nie bok: model obrócony o 45°
/// jest szerszy niż na wprost, a kafel o stałej skali musi go zmieścić w każdym
/// azymucie, inaczej postać traciłaby ramię przy obrocie kamery.
fn bryla(mesh: &ModelMesh) -> ([f32; 2], f32) {
    let (min, max) = mesh.bounds_qv;
    let dx = f32::from(max[0] - min[0]) * QV_M;
    let dy = f32::from(max[1] - min[1]) * QV_M;
    let dz = f32::from(max[2] - min[2]) * QV_M;
    // Zapas 25 % na wychylenie kończyn w pozie — bryła liczy się w spoczynku.
    let szerokosc = (dx * dx + dy * dy).sqrt().max(0.25) * 1.25;
    let wysokosc = (dz * 1.05).max(0.25);
    ([szerokosc, wysokosc], f32::from(min[2]) * QV_M)
}

fn zastosuj_poze(t: &PoseTexel, p: [f32; 3]) -> [f32; 3] {
    let q = [
        f32::from(t.rot[0]) / 32767.0,
        f32::from(t.rot[1]) / 32767.0,
        f32::from(t.rot[2]) / 32767.0,
        f32::from(t.rot[3]) / 32767.0,
    ];
    let tr = [
        f32::from(t.trans[0]) * QV_M / POSE_TRANS_SCALE,
        f32::from(t.trans[1]) * QV_M / POSE_TRANS_SCALE,
        f32::from(t.trans[2]) * QV_M / POSE_TRANS_SCALE,
    ];
    let r = obroc(q, p);
    [r[0] + tr[0], r[1] + tr[1], r[2] + tr[2]]
}

fn obroc_normalna(t: &PoseTexel, face: u8) -> u8 {
    let n = FACE_NORMALS[face as usize % 6];
    let q = [
        f32::from(t.rot[0]) / 32767.0,
        f32::from(t.rot[1]) / 32767.0,
        f32::from(t.rot[2]) / 32767.0,
        f32::from(t.rot[3]) / 32767.0,
    ];
    let r = obroc(q, [f32::from(n[0]), f32::from(n[1]), f32::from(n[2])]);
    // Najbliższa oś — kafel niesie numer ściany, nie wektor, bo normalne voxelowe
    // są osiowe i trzy bity wystarczają.
    let mut najlepsza = 0u8;
    let mut najlepszy = f32::NEG_INFINITY;
    for (i, f) in FACE_NORMALS.iter().enumerate() {
        let d = r[0] * f32::from(f[0]) + r[1] * f32::from(f[1]) + r[2] * f32::from(f[2]);
        if d > najlepszy {
            najlepszy = d;
            najlepsza = i as u8;
        }
    }
    najlepsza
}

fn obroc(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let t = [
        2.0 * (q[1] * v[2] - q[2] * v[1]),
        2.0 * (q[2] * v[0] - q[0] * v[2]),
        2.0 * (q[0] * v[1] - q[1] * v[0]),
    ];
    [
        v[0] + q[3] * t[0] + q[1] * t[2] - q[2] * t[1],
        v[1] + q[3] * t[1] + q[2] * t[0] - q[0] * t[2],
        v[2] + q[3] * t[2] + q[0] * t[1] - q[1] * t[0],
    ]
}

/// Rasteryzacja barycentryczna jednego trójkąta z buforem głębi.
///
/// Atrybuty są **płaskie** — cały trójkąt ma jedną rolę i jedną ścianę, bo w siatce
/// voxelowej kwadrat ściany ma jedno i drugie stałe. Stąd brak interpolacji czegokolwiek
/// poza głębią i stąd cała funkcja mieści się w kilkunastu liniach.
fn rasteryzuj(px: &mut [u8], depth: &mut [f32], v: &[[f32; 3]; 3], (role, face): (u8, u8)) {
    let min_x = v
        .iter()
        .map(|p| p[0])
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0) as i32;
    let max_x = (v
        .iter()
        .map(|p| p[0])
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil())
    .min(TILE_W as f32) as i32;
    let min_y = v
        .iter()
        .map(|p| p[1])
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0) as i32;
    let max_y = (v
        .iter()
        .map(|p| p[1])
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil())
    .min(TILE_H as f32) as i32;
    let pole =
        (v[1][0] - v[0][0]) * (v[2][1] - v[0][1]) - (v[2][0] - v[0][0]) * (v[1][1] - v[0][1]);
    if pole.abs() < 1.0e-6 {
        return;
    }
    for y in min_y..max_y {
        for x in min_x..max_x {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let w0 = ((v[1][0] - fx) * (v[2][1] - fy) - (v[2][0] - fx) * (v[1][1] - fy)) / pole;
            let w1 = ((v[2][0] - fx) * (v[0][1] - fy) - (v[0][0] - fx) * (v[2][1] - fy)) / pole;
            let w2 = 1.0 - w0 - w1;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }
            let d = w0 * v[0][2] + w1 * v[1][2] + w2 * v[2][2];
            let i = (y as u32 * TILE_W + x as u32) as usize;
            if d >= depth[i] {
                continue;
            }
            depth[i] = d;
            let o = i * CHANNELS;
            px[o] = role + 1;
            px[o + 1] = face + 1;
            px[o + 2] = 0;
            px[o + 3] = 255;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ClipLibrary;

    fn katalog() -> Option<ModelLibrary> {
        ModelLibrary::load_dir(&magnat_core::assets::data_path("models")).ok()
    }

    /// Sylwetka musi coś zawierać i musi mieć **tło przezroczyste**. Kafel pełny
    /// znaczy, że rzutowanie wyszło poza skalę, a kafel pusty — że nic nie trafiło
    /// w piksele; jedno i drugie wygląda na ekranie jak brak impostorów.
    #[test]
    fn kafel_ma_sylwetke_i_tlo() {
        let Some(modele) = katalog() else { return };
        let klipy = ClipLibrary::builtin();
        let pozy = PoseAtlas::bake(&modele, &klipy);
        let atlas = ImpostorAtlas::bake(&modele, &klipy, &pozy);
        let id = modele.id_of("citizen").expect("citizen");
        let e = atlas.entry(id);
        assert!(!e.is_empty(), "postać nie dostała impostora");
        for l in 0..e.layers() {
            let mut pelne = 0usize;
            for y in 0..TILE_H {
                for x in 0..TILE_W {
                    if atlas.texel(e.base_layer + l, x, y)[3] != 0 {
                        pelne += 1;
                    }
                }
            }
            let wszystkie = (TILE_W * TILE_H) as usize;
            assert!(
                pelne > wszystkie / 200,
                "warstwa {l}: sylwetka ma {pelne} pikseli z {wszystkie}"
            );
            assert!(
                pelne < wszystkie * 3 / 4,
                "warstwa {l}: sylwetka zajmuje {pelne} z {wszystkie} — rzut poza skalą"
            );
        }
    }

    /// Sylwetka musi **mieścić się w kaflu**. Rzut zakłada, że bryła jest wyśrodkowana
    /// na origin (pilnuje tego test w `mvoxc`); model przesunięty względem niego wychodzi
    /// poza krawędź i traci ramię albo pół maski — a wygląda to jak wada modelu, nie jak
    /// błąd skali. Skrajne kolumny i wiersz górny mają zostać puste.
    #[test]
    fn sylwetka_miesci_sie_w_kaflu() {
        let Some(modele) = katalog() else { return };
        let klipy = ClipLibrary::builtin();
        let pozy = PoseAtlas::bake(&modele, &klipy);
        let atlas = ImpostorAtlas::bake(&modele, &klipy, &pozy);
        for (id, model) in modele.iter() {
            let e = atlas.entry(id);
            if e.is_empty() {
                continue;
            }
            for l in 0..e.layers() {
                for y in 0..TILE_H {
                    for x in [0u32, TILE_W - 1] {
                        assert_eq!(
                            atlas.texel(e.base_layer + l, x, y)[3],
                            0,
                            "{}: warstwa {l} dotyka krawędzi w ({x}, {y})",
                            model.key
                        );
                    }
                }
                for x in 0..TILE_W {
                    assert_eq!(
                        atlas.texel(e.base_layer + l, x, 0)[3],
                        0,
                        "{}: warstwa {l} dotyka górnej krawędzi w kolumnie {x}",
                        model.key
                    );
                }
            }
        }
    }

    /// Kafel niesie **rolę**, nie kolor (`E-7`). Gdyby niósł kolor, tłum na trzystu
    /// metrach robiłby się jednolity, bo wariant nie miałby jak zadziałać.
    #[test]
    fn kafel_niesie_role_slotu() {
        let Some(modele) = katalog() else { return };
        let klipy = ClipLibrary::builtin();
        let pozy = PoseAtlas::bake(&modele, &klipy);
        let atlas = ImpostorAtlas::bake(&modele, &klipy, &pozy);
        let id = modele.id_of("citizen").expect("citizen");
        let e = atlas.entry(id);
        let mut role = std::collections::BTreeSet::new();
        for y in 0..TILE_H {
            for x in 0..TILE_W {
                let t = atlas.texel(e.base_layer, x, y);
                if t[3] != 0 {
                    role.insert(t[0]);
                    assert!(
                        t[1] >= 1 && t[1] <= 6,
                        "numer ściany {} poza zakresem",
                        t[1]
                    );
                }
            }
        }
        assert!(role.len() >= 2, "sylwetka ma jedną rolę: {role:?}");
        assert!(
            !role.contains(&0),
            "rola zero w miejscu, gdzie jest sylwetka"
        );
    }

    /// Cztery fazy chodu mają się **różnić** — inaczej tłum w oddali stoi w miejscu,
    /// choć się przesuwa, a to widać z daleka bardziej niż brak szczegółów.
    #[test]
    fn fazy_chodu_roznia_sie_od_siebie() {
        let Some(modele) = katalog() else { return };
        let klipy = ClipLibrary::builtin();
        let pozy = PoseAtlas::bake(&modele, &klipy);
        let atlas = ImpostorAtlas::bake(&modele, &klipy, &pozy);
        let e = atlas.entry(modele.id_of("citizen").expect("citizen"));
        let sylwetka = |l: u32| -> Vec<u8> {
            (0..TILE_H)
                .flat_map(|y| (0..TILE_W).map(move |x| (x, y)))
                .map(|(x, y)| atlas.texel(e.base_layer + l, x, y)[3])
                .collect()
        };
        let a = sylwetka(0);
        let b = sylwetka(1);
        let roznice = a.iter().zip(&b).filter(|(x, y)| x != y).count();
        assert!(roznice > 4, "fazy chodu różnią się o {roznice} pikseli");
    }

    /// Budżet: atlas encji ma zmieścić się w ułamku 192 MB przeznaczonych na impostory
    /// dzielnic. §5.6 liczył 13 MB przy wymiarze `outfit_class`, którego nie ma.
    #[test]
    fn atlas_encji_jest_maly() {
        let Some(modele) = katalog() else { return };
        let klipy = ClipLibrary::builtin();
        let pozy = PoseAtlas::bake(&modele, &klipy);
        let atlas = ImpostorAtlas::bake(&modele, &klipy, &pozy);
        assert!(atlas.bytes() > 0, "atlas pusty");
        assert!(
            atlas.bytes() <= 4 * 1024 * 1024,
            "atlas encji waży {} B",
            atlas.bytes()
        );
        // 8 kierunków × 4 fazy postaci + 16 kierunków pojazdu.
        assert_eq!(atlas.layers(), 32 + 16);
    }

    /// Billboard ma **tę samą wielkość co model**, inaczej postać przy przekroczeniu
    /// progu urosłaby albo zmalała — czyli dokładnie ten pop, którego zabrania
    /// kryterium WP4.
    #[test]
    fn billboard_ma_wielkosc_modelu() {
        let Some(modele) = katalog() else { return };
        let klipy = ClipLibrary::builtin();
        let pozy = PoseAtlas::bake(&modele, &klipy);
        let atlas = ImpostorAtlas::bake(&modele, &klipy, &pozy);
        let id = modele.id_of("citizen").expect("citizen");
        let e = atlas.entry(id);
        let mesh = build_model_mesh(modele.get(id).expect("model"), 1);
        let (min, max) = mesh.bounds_qv;
        let wysokosc = f32::from(max[2] - min[2]) * QV_M;
        assert!(
            (e.size_m[1] - wysokosc).abs() < 0.2,
            "billboard ma {} m, model {wysokosc} m",
            e.size_m[1]
        );
    }
}
