//! Snapshot ECS — zapis i odczyt stanu świata (PRD §18.1, M0 §5.9, WP-10).
//!
//! **Snapshot jest listą sekcji, nie płaskim buforem.** Bez tego migracje schematu
//! i zapis przyrostowy w M12 są niewykonalne: żeby zmienić układ jednego komponentu,
//! trzeba by przepisać cały plik, a żeby zapisać przyrostowo — wiedzieć, co się zmieniło.
//! Nagłówek i katalog sekcji są **nieskompresowane**, więc da się je przeczytać bez
//! dotykania ładunku (kilkaset kB zamiast 300 MB).
//!
//! Czego tu nie ma: migracji schematu i zapisu w tle. To M12 — M0 daje im kształt,
//! na którym da się je zbudować, i twardy błąd przy niezgodnej wersji formatu.

use crate::hash::world_state_hash;
use magnat_core::{ComponentSchemaId, Entity, StateHash, Tick};
use magnat_ecs::{ComponentId, ComponentRegistry, World};
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom, Write};
use std::ops::Range;
use std::path::Path;

pub const SNAPSHOT_MAGIC: [u8; 8] = *b"MAGNAT\0\x01";
pub const FORMAT_VERSION: u16 = 1;
const ZSTD_LEVEL: i32 = 3;

/// Konfiguracja bincode: **stała długość liczb**. Zmienna długość oszczędziłaby
/// kilka procent, ale katalog sekcji musi mieć rozmiar niezależny od wartości
/// offsetów, które sam zawiera — inaczej zapis wymagałby iteracji do punktu stałego.
fn bincode_config() -> impl bincode::config::Config {
    bincode::config::standard().with_fixed_int_encoding()
}

#[derive(Debug)]
pub enum IoError {
    Io(std::io::Error),
    BadMagic,
    /// Zgodność wstecz zaczyna się w M12. Do tego czasu niezgodna wersja to czytelny
    /// błąd, nie próba odgadnięcia układu pól (ryzyko R-8).
    UnsupportedVersion {
        found: u16,
        expected: u16,
    },
    UnknownComponent {
        schema: ComponentSchemaId,
    },
    /// Hash z nagłówka nie zgadza się z policzonym po wczytaniu.
    HashMismatch {
        expected: StateHash,
        actual: StateHash,
    },
    Corrupt(String),
}

impl std::fmt::Display for IoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IoError::Io(e) => write!(f, "błąd wejścia/wyjścia: {e}"),
            IoError::BadMagic => write!(f, "to nie jest plik zapisu Magnata"),
            IoError::UnsupportedVersion { found, expected } => write!(
                f,
                "zapis w wersji formatu {found}, ten silnik czyta {expected} \
                 (zgodność wstecz dopiero od M12)"
            ),
            IoError::UnknownComponent { schema } => {
                write!(f, "zapis zawiera nieznany komponent {schema}")
            }
            IoError::HashMismatch { expected, actual } => write!(
                f,
                "hash stanu po wczytaniu ({actual}) różny od zapisanego ({expected})"
            ),
            IoError::Corrupt(s) => write!(f, "plik zapisu uszkodzony: {s}"),
        }
    }
}

impl std::error::Error for IoError {}

impl From<std::io::Error> for IoError {
    fn from(e: std::io::Error) -> Self {
        IoError::Io(e)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchemaEntry {
    pub schema_id: ComponentSchemaId,
    pub name: String,
    pub row_bytes: u32,
}

/// Archetyp opisany schematami komponentów — `ComponentId` nie jest stabilny
/// między uruchomieniami, więc w pliku nie występuje.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArchetypeEntry {
    pub archetype: u32,
    pub components: Vec<ComponentSchemaId>,
    pub chunks: u32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ArenaPart {
    Values,
    Generations,
    Occupancy,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SectionKind {
    EntityTable,
    /// Tablica encji jednego chunka — mówi, który wiersz należy do której encji.
    ChunkEntities {
        archetype: u32,
        chunk: u32,
    },
    /// Jedna kolumna jednego chunka. Ziarno zapisu przyrostowego = ziarno
    /// copy-on-write (§5.5.1).
    ComponentColumn {
        archetype: u32,
        chunk: u32,
        component: ComponentSchemaId,
    },
    /// Arena traktowana dokładnie jak sekcja ECS (00 §K-16). Instancje dokładają
    /// M5 (oferty) i M6 (partie) — M0 zna wyłącznie kształt sekcji.
    Arena {
        kind: u16,
        part: ArenaPart,
    },
    Resource {
        type_name: String,
    },
    /// Sekcja nierozpoznana przez tę wersję silnika — zachowywana bez zmian przy
    /// przepisaniu pliku. To ona pozwala M12 dokładać dane bez łamania starych odczytów.
    Opaque {
        tag: String,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Compression {
    None,
    Zstd { level: i32 },
}

/// Jedna samoopisująca się sekcja: adresowalna, weryfikowalna i wymienialna
/// niezależnie od pozostałych.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotSection {
    pub kind: SectionKind,
    /// Zakres bajtów **w pliku** — pozwala odczytać samą tę sekcję (seek + read).
    pub byte_range: Range<u64>,
    pub compressed_len: u64,
    pub uncompressed_len: u64,
    /// XXH3-64 ładunku **po dekompresji**.
    pub checksum: u64,
    pub compression: Compression,
    /// Generacja chunka w chwili zapisu (§5.5.1) — M12 porówna ją, żeby pominąć
    /// sekcje, które się nie zmieniły od poprzedniego zapisu.
    pub source_generation: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotHeader {
    pub magic: [u8; 8],
    pub format_version: u16,
    pub engine_build: String,
    pub world_seed: u64,
    pub tick: u64,
    pub entity_count: u64,
    pub state_hash: u128,
    /// Tablica schematu: to ona, a nie `ComponentId`, jest kluczem kompatybilności.
    pub schema: Vec<SchemaEntry>,
    pub archetypes: Vec<ArchetypeEntry>,
    /// Rejestr encji: `(generacja, czy żywa)` po indeksie oraz lista wolnych indeksów
    /// **w kolejności zwalniania** (bez niej wznowienie rozjeżdża się przy pierwszym
    /// spawnie — T-D4).
    pub entity_capacity: u64,
}

/// Snapshot jako lista sekcji — to, co widać bez dekompresji ładunków.
#[derive(Clone, Debug)]
pub struct RawSnapshot {
    pub header: SnapshotHeader,
    pub sections: Vec<SnapshotSection>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaveReport {
    pub bytes: u64,
    pub sections: usize,
    pub hash: StateHash,
}

#[derive(Serialize, Deserialize)]
struct EntityTablePayload {
    entries: Vec<(u32, bool)>,
    free: Vec<u32>,
}

fn encode<T: Serialize>(value: &T) -> Vec<u8> {
    bincode::serde::encode_to_vec(value, bincode_config()).expect("serializacja nagłówka")
}

fn decode<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, IoError> {
    bincode::serde::decode_from_slice(bytes, bincode_config())
        .map(|(v, _)| v)
        .map_err(|e| IoError::Corrupt(e.to_string()))
}

/// Zapis: nagłówek + katalog sekcji (nieskompresowane), potem ładunki, każdy
/// skompresowany osobno. Zapis do `<path>.tmp`, potem `rename` — przerwany zapis
/// nie niszczy poprzedniego pliku.
pub fn save_world(world: &World, path: &Path) -> Result<SaveReport, IoError> {
    let hash = world_state_hash(world);
    let reg = world.components();

    // 1. Ładunki sekcji (jeszcze bez offsetów).
    let mut payloads: Vec<(SectionKind, Option<u32>, Vec<u8>)> = Vec::new();

    let (entries, free) = world.entities().snapshot();
    payloads.push((
        SectionKind::EntityTable,
        None,
        encode(&EntityTablePayload { entries, free }),
    ));

    let mut archetypes = Vec::new();
    for arch in world.archetypes().iter() {
        if arch.is_empty() {
            continue;
        }
        archetypes.push(ArchetypeEntry {
            archetype: arch.id().get(),
            components: arch
                .components()
                .iter()
                .map(|c| reg.info(*c).schema_id())
                .collect(),
            chunks: arch.chunk_count() as u32,
        });
        for (chunk_index, chunk) in arch.chunks().iter().enumerate() {
            let entities: Vec<u64> = chunk.entities().iter().map(|e| e.to_bits()).collect();
            payloads.push((
                SectionKind::ChunkEntities {
                    archetype: arch.id().get(),
                    chunk: chunk_index as u32,
                },
                Some(chunk.generation()),
                encode(&entities),
            ));
            for cid in arch.components() {
                let bytes = arch
                    .column_bytes(chunk_index, *cid)
                    .expect("kolumna archetypu")
                    .to_vec();
                payloads.push((
                    SectionKind::ComponentColumn {
                        archetype: arch.id().get(),
                        chunk: chunk_index as u32,
                        component: reg.info(*cid).schema_id(),
                    },
                    Some(chunk.generation()),
                    bytes,
                ));
            }
        }
    }

    // 2. Kompresja per sekcja.
    /// Sekcja gotowa do zapisu: rodzaj, generacja chunka, ładunek po kompresji,
    /// długość przed kompresją i suma kontrolna.
    struct GotowaSekcja {
        kind: SectionKind,
        generation: Option<u32>,
        packed: Vec<u8>,
        uncompressed_len: u64,
        checksum: u64,
    }

    let mut compressed: Vec<GotowaSekcja> = Vec::new();
    for (kind, generation, raw) in payloads {
        let checksum = xxhash_rust::xxh3::xxh3_64(&raw);
        let packed = zstd::encode_all(raw.as_slice(), ZSTD_LEVEL)?;
        compressed.push(GotowaSekcja {
            kind,
            generation,
            uncompressed_len: raw.len() as u64,
            checksum,
            packed,
        });
    }

    let header = SnapshotHeader {
        magic: SNAPSHOT_MAGIC,
        format_version: FORMAT_VERSION,
        engine_build: env!("CARGO_PKG_VERSION").to_string(),
        world_seed: world.seed,
        tick: world.tick.0,
        entity_count: u64::from(world.entity_count()),
        state_hash: hash.0,
        schema: reg
            .iter()
            .map(|i| SchemaEntry {
                schema_id: i.schema_id(),
                name: i.name().to_string(),
                row_bytes: i.size() as u32,
            })
            .collect(),
        archetypes,
        entity_capacity: world.entities().capacity() as u64,
    };

    // 3. Offsety: przy stałej długości liczb rozmiar katalogu nie zależy od wartości
    //    offsetów, więc wystarczy serializacja próbna zamiast iteracji.
    let make_sections = |base: u64| -> Vec<SnapshotSection> {
        let mut offset = base;
        compressed
            .iter()
            .map(|s| {
                let start = offset;
                offset += s.packed.len() as u64;
                SnapshotSection {
                    kind: s.kind.clone(),
                    byte_range: start..offset,
                    compressed_len: s.packed.len() as u64,
                    uncompressed_len: s.uncompressed_len,
                    checksum: s.checksum,
                    compression: Compression::Zstd { level: ZSTD_LEVEL },
                    source_generation: s.generation,
                }
            })
            .collect()
    };

    let probe = encode(&(header.clone(), make_sections(0)));
    let directory_len = probe.len() as u64;
    let base = SNAPSHOT_MAGIC.len() as u64 + 8 + directory_len;
    let directory = encode(&(header.clone(), make_sections(base)));
    assert_eq!(
        directory.len() as u64,
        directory_len,
        "katalog zmienił długość po wstawieniu offsetów — konfiguracja bincode \
         musi mieć stałą długość liczb"
    );

    // 4. Zapis do pliku tymczasowego i podmiana.
    let tmp = path.with_extension("tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&SNAPSHOT_MAGIC)?;
        f.write_all(&directory_len.to_le_bytes())?;
        f.write_all(&directory)?;
        for sekcja in &compressed {
            f.write_all(&sekcja.packed)?;
        }
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;

    Ok(SaveReport {
        bytes: base
            + compressed
                .iter()
                .map(|c| c.packed.len() as u64)
                .sum::<u64>(),
        sections: compressed.len(),
        hash,
    })
}

/// Odczyt **samego katalogu**, bez dekompresji ładunków. Konsument: M12 (migracje,
/// zapis przyrostowy), devtools (podgląd zapisu), CI (test sekcyjny T-D13).
pub fn read_directory(path: &Path) -> Result<RawSnapshot, IoError> {
    let mut f = std::fs::File::open(path)?;
    let mut magic = [0u8; 8];
    f.read_exact(&mut magic)?;
    if magic != SNAPSHOT_MAGIC {
        return Err(IoError::BadMagic);
    }
    let mut len_bytes = [0u8; 8];
    f.read_exact(&mut len_bytes)?;
    let directory_len = u64::from_le_bytes(len_bytes);
    let mut directory = vec![0u8; usize::try_from(directory_len).expect("katalog > usize")];
    f.read_exact(&mut directory)?;
    let (header, sections): (SnapshotHeader, Vec<SnapshotSection>) = decode(&directory)?;
    if header.format_version != FORMAT_VERSION {
        return Err(IoError::UnsupportedVersion {
            found: header.format_version,
            expected: FORMAT_VERSION,
        });
    }
    Ok(RawSnapshot { header, sections })
}

/// Odczyt ładunku jednej sekcji — bez dotykania pozostałych.
pub fn read_section(path: &Path, section: &SnapshotSection) -> Result<Vec<u8>, IoError> {
    let mut f = std::fs::File::open(path)?;
    f.seek(SeekFrom::Start(section.byte_range.start))?;
    let mut packed = vec![0u8; usize::try_from(section.compressed_len).expect("sekcja > usize")];
    f.read_exact(&mut packed)?;
    let raw = match section.compression {
        Compression::None => packed,
        Compression::Zstd { .. } => zstd::decode_all(packed.as_slice())?,
    };
    if raw.len() as u64 != section.uncompressed_len {
        return Err(IoError::Corrupt(format!(
            "sekcja {:?} ma {} B po dekompresji zamiast {}",
            section.kind,
            raw.len(),
            section.uncompressed_len
        )));
    }
    let checksum = xxhash_rust::xxh3::xxh3_64(&raw);
    if checksum != section.checksum {
        return Err(IoError::Corrupt(format!(
            "suma kontrolna sekcji {:?} się nie zgadza",
            section.kind
        )));
    }
    Ok(raw)
}

/// Przepisuje plik z podmienionym ładunkiem wskazanych sekcji. Sekcje niewymienione
/// (w tym `Opaque`, nierozpoznane przez tę wersję silnika) przechodzą **bajt w bajt**.
///
/// To jest operacja, na której M12 oprze migracje schematu i zapis przyrostowy;
/// jeśli nie działa w M0, nie da się jej dorobić bez zmiany formatu (T-D13).
pub fn rewrite_sections(
    source: &Path,
    target: &Path,
    replacements: &[(usize, Vec<u8>)],
) -> Result<(), IoError> {
    let snapshot = read_directory(source)?;
    let mut payloads: Vec<Vec<u8>> = Vec::with_capacity(snapshot.sections.len());
    let mut sections = snapshot.sections.clone();

    for (i, section) in snapshot.sections.iter().enumerate() {
        if let Some((_, new_raw)) = replacements.iter().find(|(idx, _)| *idx == i) {
            let checksum = xxhash_rust::xxh3::xxh3_64(new_raw);
            let packed = zstd::encode_all(new_raw.as_slice(), ZSTD_LEVEL)?;
            sections[i].uncompressed_len = new_raw.len() as u64;
            sections[i].compressed_len = packed.len() as u64;
            sections[i].checksum = checksum;
            payloads.push(packed);
        } else {
            // Surowe, nietknięte bajty — bez dekompresji i rekompresji.
            let mut f = std::fs::File::open(source)?;
            f.seek(SeekFrom::Start(section.byte_range.start))?;
            let mut packed =
                vec![0u8; usize::try_from(section.compressed_len).expect("sekcja > usize")];
            f.read_exact(&mut packed)?;
            payloads.push(packed);
        }
    }

    let recompute = |base: u64, sections: &[SnapshotSection]| -> Vec<SnapshotSection> {
        let mut offset = base;
        sections
            .iter()
            .map(|s| {
                let start = offset;
                offset += s.compressed_len;
                SnapshotSection {
                    byte_range: start..offset,
                    ..s.clone()
                }
            })
            .collect()
    };

    let probe = encode(&(snapshot.header.clone(), recompute(0, &sections)));
    let directory_len = probe.len() as u64;
    let base = SNAPSHOT_MAGIC.len() as u64 + 8 + directory_len;
    let directory = encode(&(snapshot.header.clone(), recompute(base, &sections)));

    let tmp = target.with_extension("tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&SNAPSHOT_MAGIC)?;
        f.write_all(&directory_len.to_le_bytes())?;
        f.write_all(&directory)?;
        for packed in &payloads {
            f.write_all(packed)?;
        }
        f.sync_all()?;
    }
    std::fs::rename(&tmp, target)?;
    Ok(())
}

/// Odczyt świata. Remapowanie po `ComponentSchemaId`; nieznany komponent albo
/// niezgodna `format_version` → twardy, opisowy błąd.
pub fn load_world(path: &Path, registry: &ComponentRegistry) -> Result<World, IoError> {
    let snapshot = read_directory(path)?;
    let header = &snapshot.header;

    // Mapowanie schemat → ComponentId w TYM uruchomieniu.
    let mut by_schema: Vec<(ComponentSchemaId, ComponentId)> = Vec::new();
    for entry in &header.schema {
        let id = registry
            .iter()
            .find(|i| i.schema_id() == entry.schema_id)
            .map(|i| i.id())
            .ok_or(IoError::UnknownComponent {
                schema: entry.schema_id,
            })?;
        by_schema.push((entry.schema_id, id));
    }
    let resolve = |schema: ComponentSchemaId| -> Result<ComponentId, IoError> {
        by_schema
            .iter()
            .find(|(s, _)| *s == schema)
            .map(|(_, id)| *id)
            .ok_or(IoError::UnknownComponent { schema })
    };

    let mut world = World::with_registry(header.world_seed, registry.clone());
    world.tick = Tick(header.tick);

    // Rejestr encji: generacje i lista wolnych indeksów.
    let table = snapshot
        .sections
        .iter()
        .find(|s| s.kind == SectionKind::EntityTable)
        .ok_or_else(|| IoError::Corrupt("brak sekcji EntityTable".into()))?;
    let payload: EntityTablePayload = decode(&read_section(path, table)?)?;
    world
        .entities_mut()
        .restore(&payload.entries, &payload.free);

    // Wiersze, archetyp po archetypie, chunk po chunku.
    for arch in &header.archetypes {
        let components: Vec<ComponentId> = arch
            .components
            .iter()
            .map(|s| resolve(*s))
            .collect::<Result<_, _>>()?;
        for chunk in 0..arch.chunks {
            let entities_section = snapshot
                .sections
                .iter()
                .find(|s| {
                    s.kind
                        == SectionKind::ChunkEntities {
                            archetype: arch.archetype,
                            chunk,
                        }
                })
                .ok_or_else(|| {
                    IoError::Corrupt(format!("brak encji chunka {}/{chunk}", arch.archetype))
                })?;
            let bits: Vec<u64> = decode(&read_section(path, entities_section)?)?;
            let entities: Vec<Entity> = bits
                .iter()
                .map(|b| {
                    Entity::from_bits(*b)
                        .ok_or_else(|| IoError::Corrupt("encja o zerowej generacji".into()))
                })
                .collect::<Result<_, _>>()?;

            let mut column_bytes: Vec<(ComponentId, Vec<u8>)> = Vec::new();
            for schema in &arch.components {
                let section = snapshot
                    .sections
                    .iter()
                    .find(|s| {
                        s.kind
                            == SectionKind::ComponentColumn {
                                archetype: arch.archetype,
                                chunk,
                                component: *schema,
                            }
                    })
                    .ok_or_else(|| {
                        IoError::Corrupt(format!("brak kolumny {schema} w chunku {chunk}"))
                    })?;
                column_bytes.push((resolve(*schema)?, read_section(path, section)?));
            }
            let columns: Vec<(ComponentId, &[u8])> = column_bytes
                .iter()
                .map(|(c, b)| (*c, b.as_slice()))
                .collect();
            world
                .restore_rows_checked(&components, &entities, &columns)
                .map_err(|e| IoError::Corrupt(e.to_string()))?;
        }
    }

    // Weryfikacja: hash z nagłówka musi się zgadzać z policzonym po wczytaniu.
    let actual = world_state_hash(&world);
    if actual.0 != header.state_hash {
        return Err(IoError::HashMismatch {
            expected: StateHash(header.state_hash),
            actual,
        });
    }
    Ok(world)
}
