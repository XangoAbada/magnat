//! WP10 — język gramatyki architektury voxelowej (M2 §5.6).
//!
//! Gramatyka kształtowa nad **drzewem zakresów**: zakres to zorientowany prostopadłościan,
//! reguła przekształca go w listę zakresów potomnych, a terminale kolejkują edycje voxeli
//! (korekta D4 — nie ma zapisu wprost do chunków).
//!
//! Ten moduł zawiera wyłącznie **język**: typy danych, ładowanie katalogu i walidator.
//! Derywacja jest w [`super::derive`], sadzenie budynków na parcelach w [`super::build`].
//!
//! Zbiór operatorów jest **zamknięty** (ryzyko R4): nie ma zmiennych użytkownika, nie ma
//! pętli poza `Repeat`, a `If` czyta wyłącznie parametry wejściowe budynku. Modding gramatyk
//! należy do M12; dopóki go nie ma, gramatyka ma być czytelna jak tabela, nie jak program.

use super::zoning::ZoneKind;
use magnat_voxel::{MaterialId, MaterialRegistry};
use serde::Deserialize;
use std::path::Path;

/// Wersja schematu plików `data/grammar/*.ron`.
pub const GRAMMAR_SCHEMA_VERSION: u32 = 1;

/// Twarde limity z M2 §5.6 — egzekwowane przez walidator (statycznie) i przez derywację
/// (dynamicznie, bo `Repeat` rozwija się dopiero przy znanych wymiarach zakresu).
pub const MAX_DEPTH: u32 = 12;
pub const MAX_NODES: u32 = 4096;

/// Indeks gramatyki w katalogu `data/grammar/`, nadawany przy ładowaniu w kolejności
/// alfabetycznej klucza — tak jak `GoodId` (00 §5, korekta D8).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct GrammarId(pub u16);

// ── Język ────────────────────────────────────────────────────────────────────────────

/// Oś zakresu: `U` wzdłuż frontu, `V` w głąb działki, `W` w pionie.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub enum Axis {
    U,
    V,
    W,
}

/// Ściana zakresu — argument `Comp`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub enum Face {
    Front,
    Back,
    Side,
    Top,
    Bottom,
}

/// Wielkość części w `Split`.
#[derive(Clone, Copy, PartialEq, Debug, Deserialize)]
pub enum Size {
    /// Bezwzględna, w metrach.
    Abs(f32),
    /// Względna — udział w tym, co zostaje po częściach bezwzględnych.
    Rel(f32),
}

/// Warunek `If`. Czyta **wyłącznie parametry wejściowe** budynku (R4): wymiary zakresu,
/// numer kondygnacji, wartość gruntu. Nie ma dostępu do stanu derywacji.
#[derive(Clone, PartialEq, Debug, Deserialize)]
pub enum Cond {
    /// Wartość gruntu w groszach za m².
    LandValueAtLeast(i64),
    FloorAtLeast(u8),
    WidthAtLeast(f32),
    DepthAtLeast(f32),
    HeightAtLeast(f32),
}

/// Kształt dachu.
#[derive(Clone, Copy, PartialEq, Debug, Deserialize)]
pub enum RoofShape {
    Flat,
    /// Dwuspadowy — kalenica wzdłuż osi `U` (czyli równolegle do ulicy).
    Gable {
        pitch_deg: u8,
    },
    /// Czterospadowy.
    Hip {
        pitch_deg: u8,
    },
}

/// Reguła gramatyki. Operatory kształtowe i trzy reguły domenowe (`Foundation`, `Floors`,
/// `Roof`), które są cukrem na `Split`, ale niosą parametry czytane poza derywacją:
/// wysokości kondygnacji trafiają do `Building.floor_heights_dm`, bo M11 tnie po stropach.
#[derive(Clone, PartialEq, Debug, Deserialize)]
pub enum Rule {
    /// Kolejno, na tym samym zakresie.
    Seq(Vec<Rule>),
    Split {
        axis: Axis,
        parts: Vec<(Size, Rule)>,
    },
    /// Podział na równe kawałki o zadanym kroku; `pad_m` to odstęp po obu stronach kawałka.
    Repeat {
        axis: Axis,
        step_m: f32,
        pad_m: f32,
        rule: Box<Rule>,
    },
    /// Zwężenie zakresu w poziomie o `m` z każdej strony.
    Inset {
        m: f32,
        rule: Box<Rule>,
    },
    /// Poszerzenie zakresu w poziomie o `m` z każdej strony.
    Offset {
        m: f32,
        rule: Box<Rule>,
    },
    /// Ustawienie wysokości zakresu na `m`, licząc od jego spodu.
    Extrude {
        m: f32,
        rule: Box<Rule>,
    },
    /// Płyta o grubości `thickness_m` przy wskazanych ścianach zakresu.
    Comp {
        thickness_m: f32,
        faces: Vec<(Face, Rule)>,
    },
    Choice(Vec<(u16, Rule)>),
    If {
        cond: Cond,
        then: Box<Rule>,
        otherwise: Box<Rule>,
    },
    Ref(String),

    // ── reguły domenowe ─────────────────────────────────────────────────────────────
    /// Fundament i kondygnacje podziemne — poniżej rzędnej posadowienia.
    Foundation {
        depth_m: f32,
        material: String,
        basements: u8,
    },
    /// Podział bryły na kondygnacje. Liczba wynika z `count` i wartości gruntu
    /// (M2 §5.6, kanał „wartość gruntu"), wysokość parteru bywa inna niż pięter.
    Floors {
        count: (u8, u8),
        height_m: f32,
        ground_height_m: f32,
        ground: Box<Rule>,
        typical: Box<Rule>,
        top: Box<Rule>,
    },
    Roof {
        shape: RoofShape,
        material: String,
    },

    // ── terminale ───────────────────────────────────────────────────────────────────
    /// Wypełnienie zakresu materiałem.
    Fill(String),
    /// Otwór — okno, przejazd bramny. Kolejkuje `Carve`, nie `Fill`.
    Void,
    /// Nic. Istnieje, żeby `If` i `Choice` miały czytelną gałąź pustą zamiast `Seq([])`.
    Nothing,
}

/// Warunki, przy których gramatyka w ogóle kandyduje dla parceli (M2 §5.6).
#[derive(Clone, PartialEq, Debug, Deserialize)]
pub struct Applies {
    /// Klucze `ZoneKind` (`r3`, `commercial`, …). Puste = dowolna strefa.
    #[serde(default)]
    pub zones: Vec<String>,
    /// Klucze epok z `data/epochs/`. Puste = dowolna epoka.
    #[serde(default)]
    pub epochs: Vec<String>,
    /// Klucze `DistrictKind` (`old_town`, `suburb`, …). Puste = dowolny styl.
    ///
    /// `District.style` to `kind × 32 + founded_epoch`, więc filtr po rodzaju dzielnicy
    /// jest filtrem po górnej połowie `StyleId` — i jest czytelny w danych, czego
    /// liczba 417 nie byłaby.
    #[serde(default)]
    pub styles: Vec<String>,
    /// Wartość gruntu w groszach za m², przedział domknięty.
    pub land_value: (i64, i64),
    pub frontage_m: (f32, f32),
    pub depth_m: (f32, f32),
    pub weight: u16,
    /// Udział pasujących parcel, które ta gramatyka faktycznie zabudowuje (0..=1).
    ///
    /// Bez tego każda działka w strefie zieleni dostawałaby pawilon, bo filtr `applies`
    /// mówi tylko „czy wolno", nie „jak często". Pokrycie < 1 zostawia resztę `Vacant` —
    /// i to jest jedyny sposób, w jaki M2 tworzy pustą działkę w środku miasta.
    #[serde(default = "pelne_pokrycie")]
    pub coverage: f32,
}

fn pelne_pokrycie() -> f32 {
    1.0
}

/// Obrys bryły na działce (M2 §5.6, `massing`).
///
/// Jedna struktura zamiast enuma `Perimeter | Freestanding | Hall`: warianty różniły się
/// wyłącznie wartościami cofnięć i progiem dziedzińca, a trzy warianty o identycznych
/// polach to abstrakcja bez drugiego konsumenta (00, „Dobre praktyki": YAGNI przed SOLID).
#[derive(Clone, Copy, PartialEq, Debug, Deserialize)]
pub struct Massing {
    pub setback_front_m: f32,
    pub setback_side_m: f32,
    pub setback_back_m: f32,
    /// Powierzchnia, powyżej której w środku bryły powstaje dziedziniec. 0 = bryła pełna.
    #[serde(default)]
    pub courtyard_min_m2: f32,
    /// Maksymalna głębokość traktu; nadmiar idzie na dziedziniec albo podwórze.
    #[serde(default = "domyslny_trakt")]
    pub max_depth_m: f32,
}

fn domyslny_trakt() -> f32 {
    18.0
}

/// Rodzaj lokalu — odpowiednik `UnitKind` po stronie danych.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub enum UnitClass {
    Dwelling,
    Retail,
    Office,
    Workshop,
    Storage,
}

/// Podział kondygnacji na lokale.
#[derive(Clone, Copy, PartialEq, Debug, Deserialize)]
pub struct UnitSpec {
    pub kind: UnitClass,
    /// Powierzchnia lokalu w m², przedział.
    pub area_m2: (u16, u16),
}

#[derive(Clone, Copy, PartialEq, Debug, Deserialize)]
pub struct Interior {
    pub ground: UnitSpec,
    pub typical: UnitSpec,
    pub top: UnitSpec,
    /// Klatki i korytarze — odejmowane od powierzchni brutto kondygnacji.
    pub circulation_share: f32,
}

/// Jedna gramatyka — jeden plik `data/grammar/*.ron`.
#[derive(Clone, PartialEq, Debug, Deserialize)]
pub struct BuildingGrammar {
    pub schema_version: u32,
    pub id: String,
    pub applies: Applies,
    pub massing: Massing,
    /// Reguły stosowane po kolei do zakresu bryły.
    pub rules: Vec<Rule>,
    /// Cele operatora `Ref`. Lista par, nie mapa: kolejność w danych ma być kolejnością
    /// w pamięci, a po `HashMap` nie wolno iterować (00 §3.2).
    #[serde(default)]
    pub refs: Vec<(String, Rule)>,
    pub interior: Interior,
}

impl BuildingGrammar {
    #[must_use]
    pub fn rule_ref(&self, id: &str) -> Option<&Rule> {
        self.refs.iter().find(|(k, _)| k == id).map(|(_, r)| r)
    }
}

// ── Katalog ──────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum GrammarError {
    Io(std::io::Error),
    Parse { file: String, msg: String },
    SchemaVersion { file: String, got: u32 },
    UnknownMaterial { id: String, key: String },
    UnknownZone { id: String, key: String },
    UnknownRef { id: String, key: String },
    RefCycle { id: String, key: String },
    TooManyNodes { id: String, nodes: u32 },
    TooDeep { id: String, depth: u32 },
    BadRange { id: String, what: &'static str },
    DuplicateId(String),
    NoFallback(String),
}

impl std::fmt::Display for GrammarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GrammarError::Io(e) => write!(f, "data/grammar: {e}"),
            GrammarError::Parse { file, msg } => write!(f, "{file}: {msg}"),
            GrammarError::SchemaVersion { file, got } => write!(
                f,
                "{file}: schema_version {got}, oczekiwano {GRAMMAR_SCHEMA_VERSION}"
            ),
            GrammarError::UnknownMaterial { id, key } => {
                write!(f, "gramatyka {id}: nieznany materiał `{key}`")
            }
            GrammarError::UnknownZone { id, key } => {
                write!(f, "gramatyka {id}: nieznana strefa `{key}`")
            }
            GrammarError::UnknownRef { id, key } => {
                write!(f, "gramatyka {id}: Ref(\"{key}\") bez celu")
            }
            GrammarError::RefCycle { id, key } => {
                write!(f, "gramatyka {id}: cykl Ref przez `{key}`")
            }
            GrammarError::TooManyNodes { id, nodes } => write!(
                f,
                "gramatyka {id}: {nodes} węzłów, limit {MAX_NODES} (M2 §5.6)"
            ),
            GrammarError::TooDeep { id, depth } => write!(
                f,
                "gramatyka {id}: głębokość {depth}, limit {MAX_DEPTH} (M2 §5.6)"
            ),
            GrammarError::BadRange { id, what } => {
                write!(f, "gramatyka {id}: przedział `{what}` jest pusty albo ujemny")
            }
            GrammarError::DuplicateId(id) => write!(f, "gramatyka {id} zdefiniowana dwa razy"),
            GrammarError::NoFallback(z) => write!(
                f,
                "brak gramatyki awaryjnej `_fallback_{z}` — parcela bez kandydata nie miałaby co postawić"
            ),
        }
    }
}

impl std::error::Error for GrammarError {}

impl From<std::io::Error> for GrammarError {
    fn from(e: std::io::Error) -> GrammarError {
        GrammarError::Io(e)
    }
}

/// Katalog gramatyk. `GrammarId` to indeks w `items`, a kolejność jest alfabetyczna
/// po `id` — ten sam zestaw danych daje te same identyfikatory na każdej maszynie.
#[derive(Clone, Debug)]
pub struct GrammarSet {
    items: Vec<BuildingGrammar>,
    /// Gramatyka awaryjna per strefa, w indeksach `ZoneKind::index()`.
    fallback: [Option<GrammarId>; 16],
}

impl GrammarSet {
    /// Ładuje i **waliduje** wszystkie `*.ron` z katalogu. Błąd danych jest błędem,
    /// nie ostrzeżeniem: gramatyka odrzucona po cichu daje miasto z dziurami,
    /// a znaleźć to można dopiero wzrokiem.
    pub fn load_dir(dir: &Path, mats: &MaterialRegistry) -> Result<GrammarSet, GrammarError> {
        let mut files: Vec<_> = std::fs::read_dir(dir)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "ron"))
            .collect();
        files.sort();

        let mut items: Vec<BuildingGrammar> = Vec::new();
        for path in files {
            let name = path.display().to_string();
            let text = std::fs::read_to_string(&path)?;
            let g: BuildingGrammar = ron::from_str(&text).map_err(|e| GrammarError::Parse {
                file: name.clone(),
                msg: e.to_string(),
            })?;
            if g.schema_version != GRAMMAR_SCHEMA_VERSION {
                return Err(GrammarError::SchemaVersion {
                    file: name,
                    got: g.schema_version,
                });
            }
            items.push(g);
        }
        GrammarSet::from_items(items, mats)
    }

    fn from_items(
        mut items: Vec<BuildingGrammar>,
        mats: &MaterialRegistry,
    ) -> Result<GrammarSet, GrammarError> {
        items.sort_by(|a, b| a.id.cmp(&b.id));
        for para in items.windows(2) {
            if para[0].id == para[1].id {
                return Err(GrammarError::DuplicateId(para[0].id.clone()));
            }
        }
        for g in &items {
            validate(g, mats)?;
        }

        // Gramatyka awaryjna: `_fallback_<strefa>`, a gdy jej nie ma — `_fallback_default`.
        // Jeden plik na strefę byłby dwunastoma prawie identycznymi plikami; zestaw
        // domyślny plus wyjątki tam, gdzie naprawdę się różnią, mówi to samo krócej.
        let domyslna = items
            .iter()
            .position(|g| g.id == "_fallback_default")
            .map(|i| GrammarId(i as u16))
            .ok_or_else(|| GrammarError::NoFallback("default".to_string()))?;
        let mut fallback = [Some(domyslna); 16];
        // Strefy, w których M2d **nie stawia nic**: park z budynkiem awaryjnym na każdej
        // działce przestaje być parkiem, a kopalnia i gospodarstwo to `SiteSeed` Etapu 7
        // (M2e §5.8 pkt 5), nie bryła z gramatyki.
        for z in [
            ZoneKind::Water,
            ZoneKind::Undevelopable,
            ZoneKind::Green,
            ZoneKind::Extraction,
        ] {
            fallback[z.index()] = None;
        }
        for (i, g) in items.iter().enumerate() {
            let Some(z) = g.id.strip_prefix("_fallback_") else {
                continue;
            };
            if z == "default" {
                continue;
            }
            let Some(zone) = zone_z_klucza(z) else {
                return Err(GrammarError::UnknownZone {
                    id: g.id.clone(),
                    key: z.to_string(),
                });
            };
            fallback[zone.index()] = Some(GrammarId(i as u16));
        }
        Ok(GrammarSet { items, fallback })
    }

    #[must_use]
    pub fn get(&self, id: GrammarId) -> &BuildingGrammar {
        &self.items[id.0 as usize]
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[must_use]
    pub fn all(&self) -> &[BuildingGrammar] {
        &self.items
    }

    #[must_use]
    pub fn fallback_for(&self, z: ZoneKind) -> Option<GrammarId> {
        self.fallback[z.index()]
    }

    #[must_use]
    pub fn id_of(&self, key: &str) -> Option<GrammarId> {
        self.items
            .iter()
            .position(|g| g.id == key)
            .map(|i| GrammarId(i as u16))
    }
}

/// Klucz tekstowy → `ZoneKind`. Odwrotność `ZoneKind::key()`, w jednym miejscu.
#[must_use]
pub fn zone_z_klucza(k: &str) -> Option<ZoneKind> {
    ZoneKind::ALL.into_iter().find(|z| z.key() == k)
}

// ── Walidator ────────────────────────────────────────────────────────────────────────

/// Sprawdza jedną gramatykę. Kryterium WP10: odrzuca brak `schema_version` (to robi loader),
/// nieznany materiał, cykl `Ref` i przekroczenie budżetu węzłów.
pub fn validate(g: &BuildingGrammar, mats: &MaterialRegistry) -> Result<(), GrammarError> {
    let id = &g.id;
    for z in &g.applies.zones {
        if zone_z_klucza(z).is_none() {
            return Err(GrammarError::UnknownZone {
                id: id.clone(),
                key: z.clone(),
            });
        }
    }
    if g.applies.land_value.0 > g.applies.land_value.1 {
        return Err(GrammarError::BadRange {
            id: id.clone(),
            what: "land_value",
        });
    }
    if g.applies.frontage_m.0 > g.applies.frontage_m.1 || g.applies.frontage_m.0 < 0.0 {
        return Err(GrammarError::BadRange {
            id: id.clone(),
            what: "frontage_m",
        });
    }
    if g.applies.depth_m.0 > g.applies.depth_m.1 || g.applies.depth_m.0 < 0.0 {
        return Err(GrammarError::BadRange {
            id: id.clone(),
            what: "depth_m",
        });
    }
    for spec in [&g.interior.ground, &g.interior.typical, &g.interior.top] {
        if spec.area_m2.0 == 0 || spec.area_m2.0 > spec.area_m2.1 {
            return Err(GrammarError::BadRange {
                id: id.clone(),
                what: "interior.area_m2",
            });
        }
    }

    // Statyczna liczba węzłów i głębokość, ze śledzeniem `Ref` — stąd stos `w_trakcie`,
    // który jednocześnie wykrywa cykl. `Repeat` liczy się jak jeden węzeł; jego faktyczne
    // rozwinięcie zależy od wymiarów zakresu i limit egzekwuje derywacja.
    let mut nodes = 0u32;
    let mut stos: Vec<&str> = Vec::new();
    for r in &g.rules {
        zlicz(g, r, 1, &mut nodes, &mut stos)?;
    }
    for (_, r) in &g.refs {
        zlicz(g, r, 1, &mut nodes, &mut stos)?;
    }
    if nodes > MAX_NODES {
        return Err(GrammarError::TooManyNodes {
            id: id.clone(),
            nodes,
        });
    }

    let mut materialy = Vec::new();
    for r in &g.rules {
        zbierz_materialy(r, &mut materialy);
    }
    for (_, r) in &g.refs {
        zbierz_materialy(r, &mut materialy);
    }
    for m in materialy {
        if mats.id_of(&m).is_none() {
            return Err(GrammarError::UnknownMaterial {
                id: id.clone(),
                key: m,
            });
        }
    }
    Ok(())
}

fn zlicz<'a>(
    g: &'a BuildingGrammar,
    r: &'a Rule,
    depth: u32,
    nodes: &mut u32,
    stos: &mut Vec<&'a str>,
) -> Result<(), GrammarError> {
    *nodes += 1;
    if depth > MAX_DEPTH {
        return Err(GrammarError::TooDeep {
            id: g.id.clone(),
            depth,
        });
    }
    if *nodes > MAX_NODES {
        return Err(GrammarError::TooManyNodes {
            id: g.id.clone(),
            nodes: *nodes,
        });
    }
    let zejdz =
        |r: &'a Rule, nodes: &mut u32, stos: &mut Vec<&'a str>| zlicz(g, r, depth + 1, nodes, stos);
    match r {
        Rule::Seq(v) => {
            for x in v {
                zejdz(x, nodes, stos)?;
            }
        }
        Rule::Split { parts, .. } => {
            for (_, x) in parts {
                zejdz(x, nodes, stos)?;
            }
        }
        Rule::Choice(v) => {
            for (_, x) in v {
                zejdz(x, nodes, stos)?;
            }
        }
        Rule::Comp { faces, .. } => {
            for (_, x) in faces {
                zejdz(x, nodes, stos)?;
            }
        }
        Rule::Repeat { rule, .. }
        | Rule::Inset { rule, .. }
        | Rule::Offset { rule, .. }
        | Rule::Extrude { rule, .. } => zejdz(rule, nodes, stos)?,
        Rule::If {
            then, otherwise, ..
        } => {
            zejdz(then, nodes, stos)?;
            zejdz(otherwise, nodes, stos)?;
        }
        Rule::Floors {
            ground,
            typical,
            top,
            ..
        } => {
            zejdz(ground, nodes, stos)?;
            zejdz(typical, nodes, stos)?;
            zejdz(top, nodes, stos)?;
        }
        Rule::Ref(k) => {
            if stos.contains(&k.as_str()) {
                return Err(GrammarError::RefCycle {
                    id: g.id.clone(),
                    key: k.clone(),
                });
            }
            let Some(target) = g.rule_ref(k) else {
                return Err(GrammarError::UnknownRef {
                    id: g.id.clone(),
                    key: k.clone(),
                });
            };
            stos.push(k);
            let r = zlicz(g, target, depth + 1, nodes, stos);
            stos.pop();
            r?;
        }
        Rule::Foundation { .. }
        | Rule::Roof { .. }
        | Rule::Fill(_)
        | Rule::Void
        | Rule::Nothing => {}
    }
    Ok(())
}

fn zbierz_materialy(r: &Rule, out: &mut Vec<String>) {
    match r {
        Rule::Fill(m) => out.push(m.clone()),
        Rule::Foundation { material, .. } | Rule::Roof { material, .. } => {
            out.push(material.clone())
        }
        Rule::Seq(v) => v.iter().for_each(|x| zbierz_materialy(x, out)),
        Rule::Split { parts, .. } => parts.iter().for_each(|(_, x)| zbierz_materialy(x, out)),
        Rule::Choice(v) => v.iter().for_each(|(_, x)| zbierz_materialy(x, out)),
        Rule::Comp { faces, .. } => faces.iter().for_each(|(_, x)| zbierz_materialy(x, out)),
        Rule::Repeat { rule, .. }
        | Rule::Inset { rule, .. }
        | Rule::Offset { rule, .. }
        | Rule::Extrude { rule, .. } => zbierz_materialy(rule, out),
        Rule::If {
            then, otherwise, ..
        } => {
            zbierz_materialy(then, out);
            zbierz_materialy(otherwise, out);
        }
        Rule::Floors {
            ground,
            typical,
            top,
            ..
        } => {
            zbierz_materialy(ground, out);
            zbierz_materialy(typical, out);
            zbierz_materialy(top, out);
        }
        Rule::Ref(_) | Rule::Void | Rule::Nothing => {}
    }
}

/// Materiał po kluczu, z paniką przy braku. Wolno, bo walidator przeszedł przed derywacją:
/// brak materiału w tym miejscu to błąd programu, nie danych.
#[must_use]
pub fn material(mats: &MaterialRegistry, key: &str) -> MaterialId {
    mats.expect_id(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rejestr() -> MaterialRegistry {
        MaterialRegistry::load_dir(&crate::assets::data_path("materials")).expect("materiały")
    }

    fn szkielet(rules: Vec<Rule>, refs: Vec<(String, Rule)>) -> BuildingGrammar {
        BuildingGrammar {
            schema_version: GRAMMAR_SCHEMA_VERSION,
            id: "test".to_string(),
            applies: Applies {
                zones: vec!["r3".to_string()],
                epochs: Vec::new(),
                styles: Vec::new(),
                land_value: (0, 1_000_000),
                frontage_m: (5.0, 50.0),
                depth_m: (5.0, 50.0),
                weight: 10,
                coverage: 1.0,
            },
            massing: Massing {
                setback_front_m: 0.0,
                setback_side_m: 0.0,
                setback_back_m: 0.0,
                courtyard_min_m2: 0.0,
                max_depth_m: 18.0,
            },
            rules,
            refs,
            interior: Interior {
                ground: UnitSpec {
                    kind: UnitClass::Retail,
                    area_m2: (40, 100),
                },
                typical: UnitSpec {
                    kind: UnitClass::Dwelling,
                    area_m2: (40, 90),
                },
                top: UnitSpec {
                    kind: UnitClass::Dwelling,
                    area_m2: (30, 60),
                },
                circulation_share: 0.14,
            },
        }
    }

    #[test]
    fn nieznany_material_lamie_walidacje() {
        let m = rejestr();
        let g = szkielet(vec![Rule::Fill("nie_ma_takiego".to_string())], Vec::new());
        assert!(matches!(
            validate(&g, &m),
            Err(GrammarError::UnknownMaterial { .. })
        ));
        let ok = szkielet(vec![Rule::Fill("brick_red".to_string())], Vec::new());
        assert!(validate(&ok, &m).is_ok());
    }

    #[test]
    fn cykl_ref_jest_wykrywany_a_nie_zapetla() {
        let m = rejestr();
        let g = szkielet(
            vec![Rule::Ref("a".to_string())],
            vec![
                ("a".to_string(), Rule::Ref("b".to_string())),
                ("b".to_string(), Rule::Ref("a".to_string())),
            ],
        );
        assert!(matches!(
            validate(&g, &m),
            Err(GrammarError::RefCycle { .. })
        ));
    }

    #[test]
    fn ref_bez_celu_jest_bledem() {
        let m = rejestr();
        let g = szkielet(vec![Rule::Ref("nigdzie".to_string())], Vec::new());
        assert!(matches!(
            validate(&g, &m),
            Err(GrammarError::UnknownRef { .. })
        ));
    }

    #[test]
    fn budzet_wezlow_jest_egzekwowany() {
        let m = rejestr();
        // Drzewo szersze niż limit: 5000 terminali w jednym `Seq`.
        let g = szkielet(
            vec![Rule::Seq(
                (0..5000).map(|_| Rule::Nothing).collect::<Vec<_>>(),
            )],
            Vec::new(),
        );
        assert!(matches!(
            validate(&g, &m),
            Err(GrammarError::TooManyNodes { .. })
        ));
    }

    #[test]
    fn glebokosc_ponad_limit_jest_bledem() {
        let m = rejestr();
        let mut r = Rule::Nothing;
        for _ in 0..MAX_DEPTH + 2 {
            r = Rule::Inset {
                m: 0.1,
                rule: Box::new(r),
            };
        }
        let g = szkielet(vec![r], Vec::new());
        assert!(matches!(
            validate(&g, &m),
            Err(GrammarError::TooDeep { .. })
        ));
    }

    #[test]
    fn katalog_z_repozytorium_przechodzi_walidacje() {
        // Kryterium WP10: 100% plików z repo przechodzi.
        let m = rejestr();
        let set = GrammarSet::load_dir(&crate::assets::data_path("grammar"), &m)
            .expect("katalog gramatyk");
        assert!(set.len() >= 8, "katalog ma tylko {} gramatyk", set.len());
        for z in ZoneKind::ALL {
            if matches!(
                z,
                ZoneKind::Water | ZoneKind::Undevelopable | ZoneKind::Green | ZoneKind::Extraction
            ) {
                continue;
            }
            assert!(
                set.fallback_for(z).is_some(),
                "strefa {} bez gramatyki awaryjnej",
                z.key()
            );
        }
    }
}
