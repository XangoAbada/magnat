//! Katalog zdarzeń jako **dane** (WP5, PRD §11.2).
//!
//! Definicja zdarzenia mówi trzy rzeczy: kiedy w ogóle wolno je rozważać (`gate`),
//! jak bardzo stan świata podnosi jego szansę (`factors`) i co zmienia, gdy zajdzie
//! (`effects`). Nie mówi **nigdy**, jaka ma być cena — zakaz jest wpisany w typ
//! [`crate::param::Effect`], a nie w regulamin, i pilnuje go test T7.
//!
//! Klucz tekstowy (`natural/drought`) jest kontraktem zapisu gry: aktywne zdarzenie
//! przeżywa zapis pod kluczem, a nie pod indeksem w pliku, więc przestawienie
//! wierszy w `data/events/` niczego nie unieważnia. To odwrotnie niż przy klasach
//! pojazdów (`K-24`) i rolach (`K-43`) — i celowo, bo katalog zdarzeń ma rosnąć
//! przy każdej fazie, a katalog pojazdów nie.

use crate::param::Effect;
use crate::probe::Probe;
use magnat_core::{EventCategory, Season};
use serde::Deserialize;
use std::path::Path;

pub const EVENTS_SCHEMA_VERSION: u32 = 2;

/// Zakres zdarzenia: co jest jego „instancją".
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub enum EventScope {
    /// Całe miasto. Jedna instancja.
    World,
    /// Dzielnica. Instancji tyle, ile dzielnic.
    District,
    /// Zakład. Instancji tyle, ile zakładów przechodzących filtr definicji.
    Site,
    /// Firma. Instancji tyle, ile firm w rejestrze.
    Firm,
    /// Sieć przesyłowa. Instancji tyle, ile sieci ma miasto.
    Network,
}

/// Twardy warunek wstępny. Nie podnosi szansy — **rozstrzyga, czy w ogóle pytamy**.
///
/// Bramka jest tania z rozmysłu (ryzyko `R6`): odsiewa, zanim policzy się choćby
/// jedna sonda, więc zakres `Site` ocenia się dla kilku procent zakładów, a nie
/// dla wszystkich.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
pub enum Precondition {
    /// Wyłącznie w tych porach roku.
    Season(Vec<Season>),
    /// Temperatura co najmniej / co najwyżej, w dziesiątych °C.
    MinAirTempDc(i32),
    MaxAirTempDc(i32),
    /// Dzielnica (albo miasto) ma zakłady rolne — bez nich susza nie ma co zniszczyć.
    HasFarms,
    /// Sieć ma czynne źródło. Blok, który już stoi, nie psuje się drugi raz.
    SourceOnline,
    /// Wyłącznie te media. Bez tego warunku „przeciek w sieci ciepłowniczej"
    /// oceniałby się także dla wodociągu i elektrowni, bo zakres `Network`
    /// wylicza wszystkie sieci miasta.
    Service(Vec<magnat_core::UtilityService>),
    /// Firma zatrudnia co najmniej tylu ludzi.
    MinFirmHeadcount(u32),
    /// Firma istnieje co najmniej tyle dób.
    MinFirmAgeDays(u32),
    /// Zakład ma co najmniej tyle linii produkcyjnych.
    MinSiteLines(u32),
}

/// Łamana „wartość sondy → mnożnik hazardu w punktach bazowych".
///
/// Interpolacja liniowa na `i64`, poza końcami wartość skrajna. Punkty muszą być
/// posortowane rosnąco po `x` — pilnuje tego walidator, bo łamana nieposortowana
/// daje mnożnik zależny od kolejności wpisów, czyli od niczego.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
pub struct Curve(pub Vec<(i64, i64)>);

impl Curve {
    /// Mnożnik dla wartości sondy, w punktach bazowych.
    #[must_use]
    pub fn at(&self, x: i64) -> i64 {
        let p = &self.0;
        if p.is_empty() {
            return 10_000;
        }
        if x <= p[0].0 {
            return p[0].1;
        }
        let ostatni = p.len() - 1;
        if x >= p[ostatni].0 {
            return p[ostatni].1;
        }
        for w in p.windows(2) {
            let (x0, y0) = w[0];
            let (x1, y1) = w[1];
            if x <= x1 {
                let d = x1 - x0;
                if d <= 0 {
                    return y1;
                }
                return y0 + (y1 - y0) * (x - x0) / d;
            }
        }
        p[ostatni].1
    }

    fn rosnace_x(&self) -> bool {
        self.0.windows(2).all(|w| w[0].0 < w[1].0)
    }
}

/// Sonda i jej krzywa — jeden czynnik hazardu.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
pub struct HazardFactor {
    pub probe: Probe,
    pub curve: Curve,
}

/// Wyzwalacz: szansa bazowa, bramka i czynniki.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
pub struct EventTrigger {
    /// Szansa bazowa na milion, **na jedną ocenę, na jedną instancję zakresu**.
    /// Zdarzenie oceniane co godzinę dostaje 24 rzuty na dobę i tak trzeba tę
    /// liczbę czytać — inaczej katalog kłamie o rzędzie wielkości.
    pub base_ppm: u32,
    #[serde(default)]
    pub gate: Vec<Precondition>,
    #[serde(default)]
    pub factors: Vec<HazardFactor>,
}

/// Widełki siły zdarzenia.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
pub struct SeveritySpec {
    pub min_bps: u16,
    pub max_bps: u16,
    /// Skąd bierze się położenie w widełkach. `None` znaczy „z losowania" —
    /// wtedy zdarzenie ma siłę przypadkową. `Some` znaczy „ze stanu świata":
    /// susza przy deficycie 80 mm jest silniejsza niż przy 40 i nie jest to
    /// kwestia szczęścia. Rzut zostaje wtedy jako ±10 % rozrzutu.
    #[serde(default)]
    pub from: Option<(Probe, i64, i64)>,
}

/// Jak długo trwa.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
pub enum DurationSpec {
    /// Stała liczba dób.
    FixedDays(u32),
    /// Losowana z przedziału — czas brygady remontowej, nie parametr fizyki.
    RangeDays(u32, u32),
    /// Kończy się, gdy sonda spadnie poniżej progu. Sprawdzane raz na dobę,
    /// z gwarantowanym minimum, żeby zdarzenie nie umierało w tej samej dobie,
    /// w której powstało.
    UntilBelow {
        probe: Probe,
        value: i64,
        min_days: u32,
    },
}

/// Definicja zdarzenia — jeden wiersz katalogu.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
pub struct EventDef {
    pub key: String,
    pub category: EventCategory,
    pub scope: EventScope,
    /// Co ile ocenia się hazard. `false` znaczy „raz na dobę" (zdarzenia wolne:
    /// naturalne, społeczne, polityczne), `true` — „co godzinę" (awarie).
    #[serde(default)]
    pub hourly: bool,
    pub trigger: EventTrigger,
    pub severity: SeveritySpec,
    pub duration: DurationSpec,
    pub effects: Vec<Effect>,
    /// Rodzaj ryzyka ubezpieczeniowego, jeśli to zdarzenie **niszczy majątek**
    /// (M10d WP10.12, `GD-4`).
    ///
    /// Osobne pole, a nie siódmy wariant [`Effect`], i to jest rozstrzygnięcie:
    /// efekty są **odwracalne** — `ParamOverlay` pamięta, co zastał, i przywraca
    /// to przy wygaśnięciu. Szkoda majątkowa jest jednorazowa i nieodwracalna,
    /// więc w tamtym mechanizmie nie mieści się z definicji.
    ///
    /// `None` znaczy „to zdarzenie niczego nie niszczy" i tak jest dla większości
    /// katalogu: susza zabiera plon przez `SiteOutput`, a nie przez zniszczenie
    /// zapasu, który już leży w magazynie.
    ///
    /// Nazwanie ryzyka **nie jest parametrem ryzyka** w rozumieniu kryterium
    /// WP10.12: mówi, czym jest to zdarzenie, a nie jak groźna jest dzielnica.
    /// Tego drugiego nie ma nigdzie w danych i ma nie być.
    #[serde(default)]
    pub peril: Option<magnat_core::PerilKind>,
    pub cooldown_days: u32,
    pub max_concurrent: u8,
    /// Klucz lokalizacji wpisu do kroniki (`data/locale/`). Sam tekst nigdy
    /// nie stoi w katalogu zdarzeń — CLAUDE.md, „Język i lokalizacja".
    pub chronicle: String,
}

/// Cały katalog, sklejony z sześciu plików kategorii.
#[derive(Clone, Debug, Default)]
pub struct EventCatalog {
    pub defs: Vec<EventDef>,
}

#[derive(Debug, Deserialize)]
struct EventFile {
    schema_version: u32,
    events: Vec<EventDef>,
}

#[derive(Debug)]
pub enum CatalogError {
    Io(String),
    Parse(String),
    Schema { found: u32, want: u32 },
    DuplicateKey(String),
    BadKey(String),
    CurveNotSorted { key: String },
    EmptyCurve { key: String },
    NoEffects { key: String },
    BadSeverity { key: String },
    ZeroConcurrent { key: String },
    CategoryMismatch { key: String },
    EmptyCategory(&'static str),
    ScopeMismatch { key: String, why: &'static str },
}

impl std::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CatalogError::Io(e) | CatalogError::Parse(e) => write!(f, "data/events: {e}"),
            CatalogError::Schema { found, want } => {
                write!(f, "data/events: schema_version {found}, oczekiwano {want}")
            }
            CatalogError::DuplicateKey(k) => write!(f, "data/events: klucz {k} dwa razy"),
            CatalogError::BadKey(k) => {
                write!(f, "data/events: klucz {k} nie ma postaci kategoria/nazwa")
            }
            CatalogError::CurveNotSorted { key } => write!(
                f,
                "data/events: {key} ma krzywą nieposortowaną rosnąco po x"
            ),
            CatalogError::EmptyCurve { key } => {
                write!(f, "data/events: {key} ma krzywą bez ani jednego punktu")
            }
            CatalogError::NoEffects { key } => write!(
                f,
                "data/events: {key} nie zmienia ani jednego parametru — zdarzenie \
                 bez skutku przechodzi każdy test i wygląda jak działające"
            ),
            CatalogError::BadSeverity { key } => write!(
                f,
                "data/events: {key} ma widełki siły spoza 1..=10000 albo odwrócone"
            ),
            CatalogError::ZeroConcurrent { key } => write!(
                f,
                "data/events: {key} ma max_concurrent 0 — nigdy by nie zaszło"
            ),
            CatalogError::CategoryMismatch { key } => write!(
                f,
                "data/events: {key} leży w pliku innej kategorii, niż deklaruje"
            ),
            CatalogError::EmptyCategory(c) => write!(
                f,
                "data/events/{c}.ron: kategoria bez ani jednej definicji — \
                 kryterium WP5 wymaga zdarzeń ze wszystkich sześciu"
            ),
            CatalogError::ScopeMismatch { key, why } => {
                write!(f, "data/events: {key} — {why}")
            }
        }
    }
}

impl std::error::Error for CatalogError {}

/// Sześć plików kategorii. Lista jest **zamknięta**: kategoria bez pliku byłaby
/// kategorią bez zdarzeń, a `EventCategory` jest słownikiem `core` i rośnie
/// wyłącznie przez wpis `K-n`.
pub const CATEGORY_FILES: [(&str, EventCategory); 6] = [
    ("natural", EventCategory::Natural),
    ("infrastructure", EventCategory::Infrastructure),
    ("external", EventCategory::External),
    ("social", EventCategory::Social),
    ("firm", EventCategory::Firm),
    ("political", EventCategory::Political),
];

impl EventCatalog {
    /// Ładuje wszystkie sześć plików z katalogu i waliduje całość.
    ///
    /// # Errors
    /// Zwraca błąd, gdy któregoś pliku nie ma, ma inną wersję schematu albo gdy
    /// definicja nie przechodzi walidacji z WP5.
    pub fn load_dir(dir: &Path) -> Result<EventCatalog, CatalogError> {
        let mut defs = Vec::new();
        for (nazwa, kategoria) in CATEGORY_FILES {
            let p = dir.join(format!("{nazwa}.ron"));
            let txt = std::fs::read_to_string(&p)
                .map_err(|e| CatalogError::Io(format!("{nazwa}: {e}")))?;
            let f: EventFile =
                ron::from_str(&txt).map_err(|e| CatalogError::Parse(format!("{nazwa}: {e}")))?;
            if f.schema_version != EVENTS_SCHEMA_VERSION {
                return Err(CatalogError::Schema {
                    found: f.schema_version,
                    want: EVENTS_SCHEMA_VERSION,
                });
            }
            if f.events.is_empty() {
                return Err(CatalogError::EmptyCategory(nazwa));
            }
            for d in &f.events {
                if d.category != kategoria {
                    return Err(CatalogError::CategoryMismatch { key: d.key.clone() });
                }
            }
            defs.extend(f.events);
        }
        let cat = EventCatalog { defs };
        cat.validate()?;
        Ok(cat)
    }

    /// # Errors
    /// Jak [`EventCatalog::load_dir`].
    pub fn load_default() -> Result<EventCatalog, CatalogError> {
        EventCatalog::load_dir(&magnat_core::data_path("events"))
    }

    /// Walidator z WP5. Każda reguła istnieje, bo jej złamanie daje definicję,
    /// która **wygląda** na działającą.
    fn validate(&self) -> Result<(), CatalogError> {
        let mut klucze: Vec<&str> = self.defs.iter().map(|d| d.key.as_str()).collect();
        klucze.sort_unstable();
        for w in klucze.windows(2) {
            if w[0] == w[1] {
                return Err(CatalogError::DuplicateKey(w[0].to_string()));
            }
        }
        for d in &self.defs {
            let Some((dom, reszta)) = d.key.split_once('/') else {
                return Err(CatalogError::BadKey(d.key.clone()));
            };
            if reszta.is_empty() || !CATEGORY_FILES.iter().any(|(n, _)| *n == dom) {
                return Err(CatalogError::BadKey(d.key.clone()));
            }
            if d.effects.is_empty() {
                return Err(CatalogError::NoEffects { key: d.key.clone() });
            }
            if d.severity.min_bps == 0
                || d.severity.max_bps > 10_000
                || d.severity.min_bps > d.severity.max_bps
            {
                return Err(CatalogError::BadSeverity { key: d.key.clone() });
            }
            if d.max_concurrent == 0 {
                return Err(CatalogError::ZeroConcurrent { key: d.key.clone() });
            }
            for f in &d.trigger.factors {
                if f.curve.0.is_empty() {
                    return Err(CatalogError::EmptyCurve { key: d.key.clone() });
                }
                if !f.curve.rosnace_x() {
                    return Err(CatalogError::CurveNotSorted { key: d.key.clone() });
                }
            }
            for e in &d.effects {
                if let Some(why) = e.scope_mismatch(d.scope) {
                    return Err(CatalogError::ScopeMismatch {
                        key: d.key.clone(),
                        why,
                    });
                }
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn index_of(&self, key: &str) -> Option<u16> {
        self.defs
            .iter()
            .position(|d| d.key == key)
            .and_then(|i| u16::try_from(i).ok())
    }
}
