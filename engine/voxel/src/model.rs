//! Format `.mvox` — model voxelowy encji i propów (M11a §5.1, WP1).
//!
//! Jeden format dla wszystkiego, co **nie jest chunkiem terenu**: postać, pojazd, prop
//! wnętrza, szyld, maszyna. Siatka 0,25 m, katalog `data/models/`.
//!
//! ### Dwie decyzje, z których wynika reszta
//!
//! **1. Model nie zawiera koloru, tylko indeks slotu palety 0..15.** Barwa podstawia się
//! przy rysowaniu, z wpisu w `PaletteTable` wskazanego przez instancję. To jest mechanizm,
//! dzięki któremu „auto należy do konkretnego gospodarstwa i ma swój kolor" (PRD §16.3)
//! nie wymaga duplikowania siatki: 48 modeli × 64 lakiery to 3072 różne auta z 48 siatek,
//! a nie 3072 pliki.
//!
//! **2. Model jest zbiorem części sztywnych z hierarchią i pivotami — bez skinningu.**
//! Voxelowa figurka nie ma się jak deformować, ma się obracać w stawach. Skinning
//! wierzchołkowy wymagałby wag per wierzchołek (a greedy meshing produkuje wierzchołki,
//! które nie mają tożsamości), macierzy kości na instancję i ponownego meshingu przy
//! każdej pozie. Części sztywne dają tę samą jakość wizualną za jeden bajt `part`
//! w atrybucie wierzchołka.
//!
//! ### Jednostki
//!
//! Voxel modelu ma 0,25 m. `pivot` i pozycje wierzchołków są w **ćwiartkach voxela**
//! (0,0625 m) — precyzja stawu, przy której kolano nie wypada z nogi, a zasięg `i16`
//! (±2047 m) jest dwa rzędy większy niż największy model.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

/// Wersja formatu. Plik z inną wersją jest błędem, nie ostrzeżeniem — cicha zgodność
/// wsteczna z danymi kończy się modelem wczytanym z połowy pól.
pub const MVOX_VERSION: u16 = 1;

/// Ile poziomów detalu ma każdy model: L0 pełny, L1 uproszczony, L2 impostor.
pub const LOD_COUNT: u8 = 3;

/// Ile slotów palety ma model. Szesnaście, bo tyle mieści wpis `PaletteTable`
/// (16 × RGBA8 = 64 B) i tyle wchodzi w cztery bity indeksu w danych voxela.
pub const MAX_SLOTS: usize = 16;

/// Indeks modelu w katalogu `data/models/`, nadawany po **posortowanym kluczu
/// tekstowym** (00 §5). W zapisie gry trzymamy klucz, nie indeks.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct ModelId(pub u16);

/// Do czego model służy. Kolejność wariantów jest kontraktem pliku — siedzi w nagłówku.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[repr(u8)]
pub enum ModelKind {
    #[default]
    Character = 0,
    Vehicle = 1,
    Prop = 2,
    Sign = 3,
    Machine = 4,
}

impl ModelKind {
    fn from_u8(v: u8) -> Result<ModelKind, ModelError> {
        Ok(match v {
            0 => ModelKind::Character,
            1 => ModelKind::Vehicle,
            2 => ModelKind::Prop,
            3 => ModelKind::Sign,
            4 => ModelKind::Machine,
            other => return Err(ModelError::UnknownKind(other)),
        })
    }
}

/// Rola slotu palety — **co** ten fragment modelu jest, a nie jakiego jest koloru.
/// Kolejność jest kontraktem danych: `data/palettes/` przypisuje zestaw barw po roli,
/// a `as_index()` indeksuje tablicę zestawów.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[repr(u8)]
pub enum SlotRole {
    #[default]
    Skin = 0,
    Hair = 1,
    OutfitMain = 2,
    OutfitTrim = 3,
    Accent = 4,
    Metal = 5,
    Glass = 6,
    Paint = 7,
    Livery = 8,
    Sign = 9,
    Rubber = 10,
    Emissive = 11,
}

impl SlotRole {
    pub const ALL: [SlotRole; 12] = [
        SlotRole::Skin,
        SlotRole::Hair,
        SlotRole::OutfitMain,
        SlotRole::OutfitTrim,
        SlotRole::Accent,
        SlotRole::Metal,
        SlotRole::Glass,
        SlotRole::Paint,
        SlotRole::Livery,
        SlotRole::Sign,
        SlotRole::Rubber,
        SlotRole::Emissive,
    ];

    #[must_use]
    pub const fn as_index(self) -> usize {
        self as usize
    }

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            SlotRole::Skin => "skin",
            SlotRole::Hair => "hair",
            SlotRole::OutfitMain => "outfit_main",
            SlotRole::OutfitTrim => "outfit_trim",
            SlotRole::Accent => "accent",
            SlotRole::Metal => "metal",
            SlotRole::Glass => "glass",
            SlotRole::Paint => "paint",
            SlotRole::Livery => "livery",
            SlotRole::Sign => "sign",
            SlotRole::Rubber => "rubber",
            SlotRole::Emissive => "emissive",
        }
    }

    #[must_use]
    pub fn from_key(k: &str) -> Option<SlotRole> {
        SlotRole::ALL.into_iter().find(|r| r.key() == k)
    }

    fn from_u8(v: u8) -> Result<SlotRole, ModelError> {
        SlotRole::ALL
            .get(v as usize)
            .copied()
            .ok_or(ModelError::UnknownRole(v))
    }
}

/// Nazwa części w hierarchii. Newtype nad `u16`, a nie enum, bo katalog części rośnie
/// z modelami (dźwig ma wysięgnik, wózek widłowy maszt), a animacja M11b szuka części
/// **po nazwie** — nieznana nazwa ma być częścią, której nikt nie animuje, a nie błędem
/// wczytania.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct PartName(pub u16);

impl PartName {
    pub const ROOT: PartName = PartName(0);
    pub const HEAD: PartName = PartName(1);
    pub const TORSO: PartName = PartName(2);
    pub const ARM_L: PartName = PartName(3);
    pub const ARM_R: PartName = PartName(4);
    pub const FOREARM_L: PartName = PartName(5);
    pub const FOREARM_R: PartName = PartName(6);
    pub const THIGH_L: PartName = PartName(7);
    pub const THIGH_R: PartName = PartName(8);
    pub const SHIN_L: PartName = PartName(9);
    pub const SHIN_R: PartName = PartName(10);
    pub const CARRIED: PartName = PartName(11);
    pub const HEADGEAR: PartName = PartName(12);
    /// Uproszczenie L1: oba ramiona jedną bryłą.
    pub const ARMS: PartName = PartName(13);
    /// Uproszczenie L1: obie nogi jedną bryłą.
    pub const LEGS: PartName = PartName(14);
    pub const BODY: PartName = PartName(32);
    pub const CAB: PartName = PartName(33);
    pub const WHEEL_FL: PartName = PartName(34);
    pub const WHEEL_FR: PartName = PartName(35);
    pub const WHEEL_RL: PartName = PartName(36);
    pub const WHEEL_RR: PartName = PartName(37);
    pub const CARGO: PartName = PartName(38);
    pub const LAMPS: PartName = PartName(39);
    pub const MAST: PartName = PartName(40);

    /// Nazwy znane z imienia, w kolejności numerów — do odczytu specyfikacji importu.
    pub const KNOWN: [PartName; 24] = [
        PartName::ROOT,
        PartName::HEAD,
        PartName::TORSO,
        PartName::ARM_L,
        PartName::ARM_R,
        PartName::FOREARM_L,
        PartName::FOREARM_R,
        PartName::THIGH_L,
        PartName::THIGH_R,
        PartName::SHIN_L,
        PartName::SHIN_R,
        PartName::CARRIED,
        PartName::HEADGEAR,
        PartName::ARMS,
        PartName::LEGS,
        PartName::BODY,
        PartName::CAB,
        PartName::WHEEL_FL,
        PartName::WHEEL_FR,
        PartName::WHEEL_RL,
        PartName::WHEEL_RR,
        PartName::CARGO,
        PartName::LAMPS,
        PartName::MAST,
    ];

    /// Nazwa z klucza tekstowego. `part_<n>` odczytuje część spoza listy — importer
    /// ma umieć wczytać z powrotem to, co sam zapisał.
    #[must_use]
    pub fn from_key(k: &str) -> Option<PartName> {
        if let Some(n) = k.strip_prefix("part_") {
            return n.parse().ok().map(PartName);
        }
        PartName::KNOWN.into_iter().find(|p| p.key() == k)
    }

    /// Nazwa czytelna dla człowieka — do komunikatów walidatora i do `mvoxc check`.
    /// Część spoza listy nazywa się swoim numerem; to nie jest błąd (patrz wyżej).
    #[must_use]
    pub fn key(self) -> String {
        let s = match self {
            PartName::ROOT => "root",
            PartName::HEAD => "head",
            PartName::TORSO => "torso",
            PartName::ARM_L => "arm_l",
            PartName::ARM_R => "arm_r",
            PartName::FOREARM_L => "forearm_l",
            PartName::FOREARM_R => "forearm_r",
            PartName::THIGH_L => "thigh_l",
            PartName::THIGH_R => "thigh_r",
            PartName::SHIN_L => "shin_l",
            PartName::SHIN_R => "shin_r",
            PartName::CARRIED => "carried",
            PartName::HEADGEAR => "headgear",
            PartName::ARMS => "arms",
            PartName::LEGS => "legs",
            PartName::BODY => "body",
            PartName::CAB => "cab",
            PartName::WHEEL_FL => "wheel_fl",
            PartName::WHEEL_FR => "wheel_fr",
            PartName::WHEEL_RL => "wheel_rl",
            PartName::WHEEL_RR => "wheel_rr",
            PartName::CARGO => "cargo",
            PartName::LAMPS => "lamps",
            PartName::MAST => "mast",
            PartName(n) => return format!("part_{n}"),
        };
        s.to_string()
    }
}

/// Flagi modelu. Znaczenie konsumuje render i animacja, nie format.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ModelFlags(pub u8);

impl ModelFlags {
    pub const HAS_DOORS: ModelFlags = ModelFlags(1 << 0);
    pub const HAS_WHEELS: ModelFlags = ModelFlags(1 << 1);
    pub const EMISSIVE: ModelFlags = ModelFlags(1 << 2);
    pub const TWO_SIDED: ModelFlags = ModelFlags(1 << 3);

    #[must_use]
    pub const fn union(self, f: ModelFlags) -> ModelFlags {
        ModelFlags(self.0 | f.0)
    }
}

/// Jedna część sztywna: pudełko voxeli z pivotem i rodzicem.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Part {
    pub name: PartName,
    /// Indeks rodzica w tablicy części; `NO_PARENT` = korzeń.
    pub parent: u8,
    /// W których poziomach detalu ta część istnieje (bit `i` = LOD `i`).
    pub lod_mask: u8,
    /// Staw, wokół którego część się obraca — w **ćwiartkach voxela**, względem
    /// początku układu części.
    pub pivot: [i16; 3],
    /// Wymiary pudełka w voxelach.
    pub dims: [u8; 3],
    /// Voxele w kolejności X najszybciej, potem Y, potem Z. Wartość to indeks slotu
    /// palety; **0 znaczy pustkę**, nie „slot zerowy".
    pub voxels: Vec<u8>,
}

impl Part {
    pub const NO_PARENT: u8 = 0xFF;

    #[must_use]
    pub fn voxel_count(&self) -> usize {
        self.dims[0] as usize * self.dims[1] as usize * self.dims[2] as usize
    }

    #[must_use]
    pub fn in_lod(&self, lod: u8) -> bool {
        self.lod_mask & (1 << lod) != 0
    }

    /// Indeks voxela w tablicy albo `None` poza pudełkiem.
    #[must_use]
    pub fn index(&self, x: i32, y: i32, z: i32) -> Option<usize> {
        if x < 0 || y < 0 || z < 0 {
            return None;
        }
        let (dx, dy, dz) = (
            i32::from(self.dims[0]),
            i32::from(self.dims[1]),
            i32::from(self.dims[2]),
        );
        if x >= dx || y >= dy || z >= dz {
            return None;
        }
        Some((x + dx * (y + dy * z)) as usize)
    }

    #[must_use]
    pub fn at(&self, x: i32, y: i32, z: i32) -> u8 {
        self.index(x, y, z).map_or(0, |i| self.voxels[i])
    }
}

/// Deklaracja slotu: który indeks 1..15 jaką rolę pełni.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PaletteSlot {
    pub slot: u8,
    pub role: SlotRole,
}

/// Model voxelowy w pamięci.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct VoxModel {
    /// Klucz tekstowy = nazwa pliku bez rozszerzenia. To on jest tożsamością modelu
    /// w zapisie gry; `ModelId` jest wyłącznie indeksem w obrębie wersji danych.
    pub key: String,
    pub kind: ModelKind,
    pub flags: ModelFlags,
    /// Wymiary bryły otaczającej w voxelach.
    pub bbox: [u8; 3],
    pub parts: Vec<Part>,
    pub slots: Vec<PaletteSlot>,
}

impl VoxModel {
    /// Rola slotu o danym indeksie albo `None`, jeśli model go nie zadeklarował.
    #[must_use]
    pub fn role_of(&self, slot: u8) -> Option<SlotRole> {
        self.slots.iter().find(|s| s.slot == slot).map(|s| s.role)
    }

    /// Maska ról, których model używa. Walidator palety sprawdza, że paleta pokrywa
    /// każdą z nich (kryterium WP11).
    #[must_use]
    pub fn roles_used(&self) -> Vec<SlotRole> {
        let mut v: Vec<SlotRole> = self.slots.iter().map(|s| s.role).collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Części należące do poziomu detalu, w kolejności z pliku.
    pub fn parts_in_lod(&self, lod: u8) -> impl Iterator<Item = (usize, &Part)> {
        self.parts
            .iter()
            .enumerate()
            .filter(move |(_, p)| p.in_lod(lod))
    }

    /// Sprawdza to, czego nie sprawdza parser: spójność hierarchii, pokrycie slotów
    /// i obecność każdego poziomu detalu.
    ///
    /// Model bez części w L1 nie jest błędem parsera — jest modelem, który z odległości
    /// znika. Dlatego to jest osobny krok i dlatego `mvoxc check` istnieje.
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.parts.is_empty() {
            return Err(ModelError::NoParts(self.key.clone()));
        }
        if self.parts.len() > u8::MAX as usize {
            return Err(ModelError::TooManyParts(self.parts.len()));
        }
        for (i, p) in self.parts.iter().enumerate() {
            if p.voxels.len() != p.voxel_count() {
                return Err(ModelError::PartSize {
                    part: p.name.key(),
                    got: p.voxels.len(),
                    want: p.voxel_count(),
                });
            }
            if p.parent != Part::NO_PARENT {
                if p.parent as usize >= self.parts.len() {
                    return Err(ModelError::BadParent {
                        part: p.name.key(),
                        parent: p.parent,
                    });
                }
                // Rodzic **przed** dzieckiem w tablicy: hierarchia jest wtedy z definicji
                // acykliczna, a składanie póz idzie jednym przebiegiem bez stosu.
                if p.parent as usize >= i {
                    return Err(ModelError::ForwardParent {
                        part: p.name.key(),
                        parent: p.parent,
                    });
                }
            }
            for &v in &p.voxels {
                if v as usize >= MAX_SLOTS {
                    return Err(ModelError::SlotOutOfRange {
                        part: p.name.key(),
                        slot: v,
                    });
                }
                if v != 0 && self.role_of(v).is_none() {
                    return Err(ModelError::UndeclaredSlot {
                        part: p.name.key(),
                        slot: v,
                    });
                }
            }
        }
        let mut widziane = [false; MAX_SLOTS];
        for s in &self.slots {
            if s.slot as usize >= MAX_SLOTS {
                return Err(ModelError::SlotOutOfRange {
                    part: self.key.clone(),
                    slot: s.slot,
                });
            }
            if widziane[s.slot as usize] {
                return Err(ModelError::DuplicateSlot(s.slot));
            }
            widziane[s.slot as usize] = true;
        }
        // L2 jest impostorem i wolno mu nie mieć geometrii; L0 i L1 muszą coś rysować,
        // inaczej encja znika przy pierwszym progu i wygląda to jak błąd odcięcia.
        for lod in 0..2u8 {
            if !self.parts.iter().any(|p| p.in_lod(lod)) {
                return Err(ModelError::EmptyLod {
                    key: self.key.clone(),
                    lod,
                });
            }
        }
        Ok(())
    }

    // ── Odczyt i zapis ──────────────────────────────────────────────────────────

    /// Wczytuje model z bajtów `.mvox`. `key` pochodzi z nazwy pliku, nie z treści —
    /// plik i jego tożsamość to dwie różne rzeczy i druga kopia klucza w środku
    /// rozjechałaby się przy pierwszej zmianie nazwy.
    pub fn read(key: &str, buf: &[u8]) -> Result<VoxModel, ModelError> {
        let mut c = Cursor::new(key, buf);
        let magic = c.take(4)?;
        if magic != b"MVOX" {
            return Err(ModelError::BadMagic(key.to_string()));
        }
        let version = c.u16()?;
        if version != MVOX_VERSION {
            return Err(ModelError::Version {
                key: key.to_string(),
                got: version,
            });
        }
        let kind = ModelKind::from_u8(c.u8()?)?;
        let part_count = c.u8()?;
        let lod_count = c.u8()?;
        if lod_count != LOD_COUNT {
            return Err(ModelError::LodCount {
                key: key.to_string(),
                got: lod_count,
            });
        }
        let flags = ModelFlags(c.u8()?);
        let bbox = [c.u8()?, c.u8()?, c.u8()?];
        let slot_count = c.u8()?;
        let _ = c.take(2)?; // wyrównanie nagłówka do 16 B

        let mut parts = Vec::with_capacity(part_count as usize);
        for _ in 0..part_count {
            let name = PartName(c.u16()?);
            let parent = c.u8()?;
            let lod_mask = c.u8()?;
            let pivot = [c.i16()?, c.i16()?, c.i16()?];
            let dims = [c.u8()?, c.u8()?, c.u8()?];
            let _ = c.u8()?;
            let rle_len = c.u32()? as usize;
            let rle = c.take(rle_len)?;
            let total = dims[0] as usize * dims[1] as usize * dims[2] as usize;
            let voxels = rle_decode(key, rle, total)?;
            parts.push(Part {
                name,
                parent,
                lod_mask,
                pivot,
                dims,
                voxels,
            });
        }

        let mut slots = Vec::with_capacity(slot_count as usize);
        for _ in 0..slot_count {
            let slot = c.u8()?;
            let role = SlotRole::from_u8(c.u8()?)?;
            slots.push(PaletteSlot { slot, role });
        }

        let m = VoxModel {
            key: key.to_string(),
            kind,
            flags,
            bbox,
            parts,
            slots,
        };
        m.validate()?;
        Ok(m)
    }

    /// Zapisuje model do bajtów `.mvox`.
    #[must_use]
    pub fn write(&self) -> Vec<u8> {
        let mut o = Vec::with_capacity(1024);
        o.extend_from_slice(b"MVOX");
        o.extend_from_slice(&MVOX_VERSION.to_le_bytes());
        o.push(self.kind as u8);
        o.push(self.parts.len() as u8);
        o.push(LOD_COUNT);
        o.push(self.flags.0);
        o.extend_from_slice(&self.bbox);
        o.push(self.slots.len() as u8);
        o.extend_from_slice(&[0, 0]);
        debug_assert_eq!(o.len(), 16, "nagłówek MVOX ma mieć 16 B");

        for p in &self.parts {
            o.extend_from_slice(&p.name.0.to_le_bytes());
            o.push(p.parent);
            o.push(p.lod_mask);
            for v in p.pivot {
                o.extend_from_slice(&v.to_le_bytes());
            }
            o.extend_from_slice(&p.dims);
            o.push(0);
            let rle = rle_encode(&p.voxels);
            o.extend_from_slice(&(rle.len() as u32).to_le_bytes());
            o.extend_from_slice(&rle);
        }
        for s in &self.slots {
            o.push(s.slot);
            o.push(s.role as u8);
        }
        o
    }
}

/// Katalog modeli. `ModelId` jest **pozycją posortowanego klucza**, więc ten sam zestaw
/// plików daje te same identyfikatory na każdej maszynie (00 §5).
#[derive(Clone, Debug, Default)]
pub struct ModelLibrary {
    models: Vec<VoxModel>,
    by_key: BTreeMap<String, ModelId>,
}

impl ModelLibrary {
    /// Wczytuje wszystkie `*.mvox` z katalogu.
    pub fn load_dir(dir: &Path) -> Result<ModelLibrary, ModelError> {
        let mut pliki: Vec<_> = std::fs::read_dir(dir)
            .map_err(ModelError::Io)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "mvox"))
            .collect();
        pliki.sort();

        let mut modele = Vec::new();
        for p in pliki {
            let key = p
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let buf = std::fs::read(&p).map_err(ModelError::Io)?;
            modele.push(VoxModel::read(&key, &buf)?);
        }
        ModelLibrary::from_models(modele)
    }

    pub fn from_models(mut models: Vec<VoxModel>) -> Result<ModelLibrary, ModelError> {
        models.sort_by(|a, b| a.key.cmp(&b.key));
        let mut by_key = BTreeMap::new();
        for (i, m) in models.iter().enumerate() {
            if by_key.insert(m.key.clone(), ModelId(i as u16)).is_some() {
                return Err(ModelError::DuplicateKey(m.key.clone()));
            }
        }
        Ok(ModelLibrary { models, by_key })
    }

    #[must_use]
    pub fn get(&self, id: ModelId) -> Option<&VoxModel> {
        self.models.get(id.0 as usize)
    }

    #[must_use]
    pub fn id_of(&self, key: &str) -> Option<ModelId> {
        self.by_key.get(key).copied()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.models.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (ModelId, &VoxModel)> {
        self.models
            .iter()
            .enumerate()
            .map(|(i, m)| (ModelId(i as u16), m))
    }
}

// ── RLE ─────────────────────────────────────────────────────────────────────────

/// Kodowanie par `(slot, długość)`. Długość 0 nie występuje, więc odczyt zera
/// w tej pozycji jest uszkodzonym plikiem, nie pustym odcinkiem.
fn rle_encode(voxels: &[u8]) -> Vec<u8> {
    let mut o = Vec::with_capacity(voxels.len() / 4 + 2);
    let mut i = 0;
    while i < voxels.len() {
        let v = voxels[i];
        let mut n = 1usize;
        while i + n < voxels.len() && voxels[i + n] == v && n < 255 {
            n += 1;
        }
        o.push(v);
        o.push(n as u8);
        i += n;
    }
    o
}

fn rle_decode(key: &str, rle: &[u8], total: usize) -> Result<Vec<u8>, ModelError> {
    if !rle.len().is_multiple_of(2) {
        return Err(ModelError::Truncated(key.to_string()));
    }
    let mut o = Vec::with_capacity(total);
    for para in rle.chunks_exact(2) {
        let (v, n) = (para[0], para[1]);
        if n == 0 {
            return Err(ModelError::Truncated(key.to_string()));
        }
        if o.len() + n as usize > total {
            return Err(ModelError::RleOverrun {
                key: key.to_string(),
            });
        }
        o.resize(o.len() + n as usize, v);
    }
    if o.len() != total {
        return Err(ModelError::RleOverrun {
            key: key.to_string(),
        });
    }
    Ok(o)
}

// ── Kursor bajtowy ──────────────────────────────────────────────────────────────

/// Odczyt bajt po bajcie z jawnym little-endianem, a nie rzutowanie na `#[repr(C)]`.
/// Plik jest formatem wymiany i ma się czytać tak samo na każdej platformie — rzutowanie
/// związałoby go z wyrównaniem i kolejnością bajtów maszyny, która go zapisała.
struct Cursor<'a> {
    key: &'a str,
    buf: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(key: &'a str, buf: &'a [u8]) -> Cursor<'a> {
        Cursor { key, buf, at: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], ModelError> {
        if self.at + n > self.buf.len() {
            return Err(ModelError::Truncated(self.key.to_string()));
        }
        let s = &self.buf[self.at..self.at + n];
        self.at += n;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u8, ModelError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, ModelError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn i16(&mut self) -> Result<i16, ModelError> {
        Ok(self.u16()? as i16)
    }

    fn u32(&mut self) -> Result<u32, ModelError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

// ── Błędy ───────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum ModelError {
    Io(std::io::Error),
    BadMagic(String),
    Version {
        key: String,
        got: u16,
    },
    LodCount {
        key: String,
        got: u8,
    },
    UnknownKind(u8),
    UnknownRole(u8),
    Truncated(String),
    RleOverrun {
        key: String,
    },
    NoParts(String),
    TooManyParts(usize),
    PartSize {
        part: String,
        got: usize,
        want: usize,
    },
    BadParent {
        part: String,
        parent: u8,
    },
    ForwardParent {
        part: String,
        parent: u8,
    },
    SlotOutOfRange {
        part: String,
        slot: u8,
    },
    UndeclaredSlot {
        part: String,
        slot: u8,
    },
    DuplicateSlot(u8),
    EmptyLod {
        key: String,
        lod: u8,
    },
    DuplicateKey(String),
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelError::Io(e) => write!(f, "odczyt modelu: {e}"),
            ModelError::BadMagic(k) => write!(f, "{k}: to nie jest plik MVOX"),
            ModelError::Version { key, got } => {
                write!(f, "{key}: wersja formatu {got}, oczekiwano {MVOX_VERSION}")
            }
            ModelError::LodCount { key, got } => {
                write!(f, "{key}: {got} poziomów detalu, format wymaga {LOD_COUNT}")
            }
            ModelError::UnknownKind(v) => write!(f, "nieznany rodzaj modelu: {v}"),
            ModelError::UnknownRole(v) => write!(f, "nieznana rola slotu: {v}"),
            ModelError::Truncated(k) => write!(f, "{k}: plik urwany"),
            ModelError::RleOverrun { key } => {
                write!(f, "{key}: RLE nie zgadza się z wymiarami części")
            }
            ModelError::NoParts(k) => write!(f, "{k}: model bez części"),
            ModelError::TooManyParts(n) => write!(f, "{n} części, limit to 255"),
            ModelError::PartSize { part, got, want } => {
                write!(f, "część `{part}`: {got} voxeli, wymiary mówią {want}")
            }
            ModelError::BadParent { part, parent } => {
                write!(f, "część `{part}`: rodzic {parent} nie istnieje")
            }
            ModelError::ForwardParent { part, parent } => write!(
                f,
                "część `{part}`: rodzic {parent} stoi za dzieckiem — hierarchia musi być \
                 uporządkowana od korzenia"
            ),
            ModelError::SlotOutOfRange { part, slot } => {
                write!(f, "`{part}`: slot {slot} poza zakresem 0..{MAX_SLOTS}")
            }
            ModelError::UndeclaredSlot { part, slot } => {
                write!(
                    f,
                    "część `{part}` używa slotu {slot}, którego model nie zadeklarował"
                )
            }
            ModelError::DuplicateSlot(s) => write!(f, "slot {s} zadeklarowany dwukrotnie"),
            ModelError::EmptyLod { key, lod } => {
                write!(f, "{key}: poziom detalu L{lod} nie ma ani jednej części")
            }
            ModelError::DuplicateKey(k) => write!(f, "model `{k}` wczytany dwukrotnie"),
        }
    }
}

impl std::error::Error for ModelError {}

/// Wzorcowy model dla testów całego crate'u. Osobny moduł, a nie funkcja w `tests`,
/// bo korzysta z niego także mesher — a dwie kopie wzorca rozjeżdżają się przy pierwszej
/// zmianie formatu i wtedy jeden z testów zaczyna sprawdzać nieistniejący plik.
#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;

    /// Najprostszy poprawny model: dwie części, dwa sloty, trzy poziomy detalu.
    pub fn dwie_czesci() -> VoxModel {
        let mut tors = vec![0u8; 2 * 2 * 4];
        tors.iter_mut().for_each(|v| *v = 1);
        VoxModel {
            key: "test".into(),
            kind: ModelKind::Character,
            flags: ModelFlags::default(),
            bbox: [2, 2, 6],
            parts: vec![
                Part {
                    name: PartName::TORSO,
                    parent: Part::NO_PARENT,
                    lod_mask: 0b011,
                    pivot: [4, 4, 0],
                    dims: [2, 2, 4],
                    voxels: tors,
                },
                Part {
                    name: PartName::HEAD,
                    parent: 0,
                    lod_mask: 0b011,
                    pivot: [4, 4, 0],
                    dims: [2, 2, 2],
                    voxels: vec![2; 8],
                },
            ],
            slots: vec![
                PaletteSlot {
                    slot: 1,
                    role: SlotRole::OutfitMain,
                },
                PaletteSlot {
                    slot: 2,
                    role: SlotRole::Skin,
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::dwie_czesci as wzorzec;
    use super::*;

    #[test]
    fn zapis_i_odczyt_sa_odwrotne() {
        let m = wzorzec();
        m.validate().expect("wzorzec ma być poprawny");
        let bajty = m.write();
        assert_eq!(&bajty[..4], b"MVOX");
        let wczytany = VoxModel::read("test", &bajty).expect("odczyt");
        assert_eq!(wczytany, m);
    }

    #[test]
    fn rle_koduje_dlugie_odcinki_i_wraca_bit_w_bit() {
        for dane in [
            vec![0u8; 1000],
            vec![7u8; 300],
            (0..500u32).map(|i| (i % 5) as u8).collect::<Vec<u8>>(),
        ] {
            let rle = rle_encode(&dane);
            assert_eq!(rle_decode("t", &rle, dane.len()).unwrap(), dane);
        }
        // 1000 zer to cztery pary po 255/255/255/235, czyli 8 bajtów zamiast 1000.
        assert_eq!(rle_encode(&vec![0u8; 1000]).len(), 8);
    }

    #[test]
    fn walidator_lapie_rodzica_za_dzieckiem() {
        let mut m = wzorzec();
        m.parts.swap(0, 1);
        m.parts[0].parent = 1;
        m.parts[1].parent = Part::NO_PARENT;
        assert!(matches!(
            m.validate(),
            Err(ModelError::ForwardParent { .. })
        ));
    }

    #[test]
    fn walidator_lapie_slot_bez_deklaracji() {
        let mut m = wzorzec();
        m.parts[0].voxels[0] = 9;
        assert!(matches!(
            m.validate(),
            Err(ModelError::UndeclaredSlot { slot: 9, .. })
        ));
    }

    #[test]
    fn walidator_lapie_pusty_poziom_detalu() {
        let mut m = wzorzec();
        for p in &mut m.parts {
            p.lod_mask = 0b001;
        }
        assert!(matches!(
            m.validate(),
            Err(ModelError::EmptyLod { lod: 1, .. })
        ));
    }

    #[test]
    fn urwany_plik_nie_panikuje() {
        let bajty = wzorzec().write();
        for n in 0..bajty.len() {
            // Każde obcięcie ma dać błąd albo poprawny model — nigdy paniki.
            let _ = VoxModel::read("test", &bajty[..n]);
        }
    }

    #[test]
    fn katalog_nadaje_identyfikatory_po_posortowanym_kluczu() {
        let mut a = wzorzec();
        a.key = "zebra".into();
        let mut b = wzorzec();
        b.key = "aardvark".into();
        let lib = ModelLibrary::from_models(vec![a, b]).unwrap();
        assert_eq!(lib.id_of("aardvark"), Some(ModelId(0)));
        assert_eq!(lib.id_of("zebra"), Some(ModelId(1)));
        assert_eq!(lib.get(ModelId(0)).unwrap().key, "aardvark");
    }
}
