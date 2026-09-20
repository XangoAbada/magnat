//! Strojenie marki: `data/tuning/brand.ron` i jego walidacja (M10b §5.1–5.3, `K-35`).
//!
//! Wydzielone z [`super`] przy domykaniu M10b — plik pamięci marki przekroczył próg
//! ostrzegawczy przeglądu strukturalnego i miał **dwa tematy**: jak marka działa
//! i skąd biorą się jej liczby. To jest ten drugi. Przeniesienie jest mechaniczne:
//! ani jednej zmiany zachowania, ani jednej zmiany nazwy.
//!
//! **Co tu NIE stoi:** asymetria „rozczarowanie boli trzy razy mocniej" jest kształtem
//! modelu z PRD §7.6, a nie kalibracją — dlatego `k_down > k_up` jest niezmiennikiem
//! pilnowanym przy ładowaniu, a nie liczbą do przestawienia w dowolną stronę.

use magnat_core::{
    AdChannelKind, EditorialBias, EventCategory, MediaKind, Money, AD_CHANNEL_KIND_COUNT,
    MEDIA_KIND_COUNT,
};
use serde::Deserialize;
use std::path::Path;

use super::DECAY_BUCKETS;

/// Wersja schematu `data/tuning/brand.ron` (00 §5).
pub const BRAND_SCHEMA_VERSION: u32 = 1;

/// Liczba kategorii zdarzeń — szerokość macierzy wag redakcyjnych.
pub const EVENT_CATEGORY_COUNT: usize = EventCategory::ALL.len();
/// Liczba linii redakcyjnych — wysokość tej samej macierzy.
pub const EDITORIAL_BIAS_COUNT: usize = EditorialBias::ALL.len();

/// Parametry pamięci marki z `data/tuning/brand.ron` (`K-35`).
///
/// **Dane, nie kod**: asymetria „rozczarowanie boli trzy razy mocniej niż zachwyt cieszy"
/// jest kształtem modelu i zostaje w tej strukturze, ale jej **liczby** stroi balansator.
/// Stała współczynnika jest ułamkiem w 1/256 — arytmetyka jest całkowita, bo wynik
/// wchodzi do stanu świata i do hasha (00 §2).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct BrandTuning {
    /// Jak mocno reklama przesuwa oczekiwaną jakość ku obietnicy (1/256).
    pub ad_learn: u16,
    /// Jak mocno doświadczenie przesuwa oczekiwaną jakość ku faktycznej (1/256).
    pub exp_learn: u16,
    /// Przyrost afinitetu za punkt zachwytu (1/256).
    pub k_up: u16,
    /// Ubytek afinitetu za punkt rozczarowania (1/256) — większy od `k_up`, i to jest
    /// cała asymetria z PRD §7.6.
    pub k_down: u16,
    /// Ile wizyt w sklepie przypina jego markę w pamięci (ryzyko `R4`).
    pub pin_visits: u8,
    /// Jak mocno plotka zbliża opinię słuchacza do opinii opowiadającego (1/256).
    pub rumor_pull: u16,
    /// Przyrost znajomości za jedną ekspozycję, per kanał.
    #[serde(skip)]
    pub awareness_gain: [u8; AD_CHANNEL_KIND_COUNT],
    /// Mnożnik afinitetu (1/256) po n pełnych miesiącach bez kontaktu.
    #[serde(skip)]
    pub decay_affinity: [u16; DECAY_BUCKETS],
    /// Mnożnik znajomości (1/256) po n pełnych miesiącach bez kontaktu.
    #[serde(skip)]
    pub decay_awareness: [u16; DECAY_BUCKETS],
}

impl Default for BrandTuning {
    /// Wartości z M10b §5.1 — te, na których stoją testy asymetrii z WP10.5.
    ///
    /// Default jest tu **kalibracją awaryjną, nie źródłem prawdy**: świat produkcyjny
    /// czyta `data/tuning/brand.ron`. Trzyma się jednak tych samych liczb, żeby test
    /// jednostkowy bez wczytanych danych mierzył ten sam model.
    fn default() -> BrandTuning {
        BrandTuning {
            ad_learn: 24,
            exp_learn: 96,
            k_up: 64,
            k_down: 192,
            awareness_gain: [6, 9, 7, 12, 8, 10, 5, 4],
            // Znajomość gaśnie szybciej niż sympatia: „nie pamiętam takiej firmy"
            // przychodzi wcześniej niż „nie lubiłem jej".
            decay_affinity: [
                256, 250, 244, 238, 232, 226, 220, 214, 208, 202, 196, 190, 184,
            ],
            decay_awareness: [
                256, 240, 226, 212, 199, 187, 176, 165, 155, 146, 137, 129, 121,
            ],
            pin_visits: 8,
            rumor_pull: 48,
        }
    }
}

/// Koszt i skuteczność kanałów reklamowych (§5.2).
///
/// Czyta to `magnat_media`, nie `sim/agents` — a mieszka tutaj, bo plik jest jeden
/// i ma jeden `schema_version`. Drugi plik musiałby powtórzyć kolejność kanałów,
/// a dwie listy o wymuszonej wspólnej kolejności rozjeżdżają się przy pierwszej
/// zmianie (to samo uzasadnienie, które `K-43` dało wagom produktywności w `roles.ron`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ChannelTuning {
    /// Domyślna szansa zauważenia billboardu przez przejeżdżającego, w punktach bazowych.
    pub billboard_notice_bps: u16,
    /// Koszt tysiąca ekspozycji, per kanał, w groszach.
    pub cpm_gr: [i64; AD_CHANNEL_KIND_COUNT],
    /// Jaki udział mieszkańców w promieniu dostaje ulotkę, w punktach bazowych.
    pub leaflet_hit_bps: u16,
    /// Domyślny promień ulotkowania w metrach.
    pub leaflet_radius_m: u16,
}

impl ChannelTuning {
    /// Koszt tysiąca ekspozycji tym kanałem.
    #[must_use]
    pub fn cpm(&self, c: AdChannelKind) -> Money {
        Money(self.cpm_gr[c.as_index()])
    }
}

/// Parametry tytułów medialnych (§5.3). Czytelnik ten sam co przy [`ChannelTuning`].
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct OutletTuning {
    /// Startowa wiarygodność tytułu w skali `Q`.
    pub base_credibility: u8,
    /// Ile zdarzeń redakcja bierze na wydanie — dolna granica.
    pub stories_min: u8,
    /// Górna granica tego samego.
    pub stories_max: u8,
    /// Waga kategorii zdarzenia per linia redakcyjna, w punktach bazowych.
    ///
    /// Wiersz to [`EditorialBias`], kolumna [`EventCategory`] — obie kolejności
    /// są kontraktem tej tablicy.
    pub bias_weights: [[u16; EVENT_CATEGORY_COUNT]; EDITORIAL_BIAS_COUNT],
    /// Ile slotów reklamowych tytuł sprzedaje na dobę, per [`MediaKind`].
    pub inventory_daily: [u16; MEDIA_KIND_COUNT],
    /// Cena slotu reklamowego w groszach, per [`MediaKind`].
    pub slot_price_gr: [i64; MEDIA_KIND_COUNT],
}

impl OutletTuning {
    /// Wartość informacyjna zdarzenia dla tytułu o tej linii redakcyjnej.
    #[must_use]
    pub fn news_value(&self, bias: EditorialBias, cat: EventCategory, severity_bps: u16) -> u32 {
        let waga = u32::from(self.bias_weights[bias.as_index()][cat.as_index()]);
        waga * u32::from(severity_bps) / 10_000
    }

    /// Cena jednego slotu reklamowego w tym rodzaju tytułu.
    #[must_use]
    pub fn slot_price(&self, kind: MediaKind) -> Money {
        Money(self.slot_price_gr[kind.as_index()])
    }
}

/// Cały plik `data/tuning/brand.ron` — jeden zasób świata, jeden `schema_version`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrandData {
    pub memory: BrandTuning,
    pub channels: ChannelTuning,
    pub outlets: OutletTuning,
}

#[derive(Deserialize)]
struct BrandFile {
    schema_version: u32,
    memory: BrandTuning,
    awareness_gain: [u8; AD_CHANNEL_KIND_COUNT],
    decay_affinity: [u16; DECAY_BUCKETS],
    decay_awareness: [u16; DECAY_BUCKETS],
    channels: ChannelTuning,
    outlets: OutletTuning,
}

/// Błąd wczytania `data/tuning/brand.ron`.
#[derive(Debug)]
pub enum BrandDataError {
    Io(std::io::Error),
    Ron(ron::error::SpannedError),
    Schema {
        found: u32,
        want: u32,
    },
    /// Asymetria z PRD §7.6 nie jest kalibracją: `k_down` musi przewyższać `k_up`.
    /// Plik, który to odwraca, opisuje inny model, a nie inne strojenie.
    Symmetry {
        k_up: u16,
        k_down: u16,
    },
    /// Górna granica liczby tekstów na wydanie poniżej dolnej. Odejmowanie na `u8`
    /// zawinęłoby się i redakcja brałaby dwieście pięćdziesiąt zdarzeń na dobę.
    Range {
        min: u8,
        max: u8,
    },
}

impl std::fmt::Display for BrandDataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BrandDataError::Io(e) => write!(f, "brand.ron: {e}"),
            BrandDataError::Ron(e) => write!(f, "brand.ron: {e}"),
            BrandDataError::Schema { found, want } => {
                write!(f, "brand.ron: schema_version {found}, oczekiwano {want}")
            }
            BrandDataError::Symmetry { k_up, k_down } => write!(
                f,
                "brand.ron: k_down ({k_down}) nie przewyższa k_up ({k_up}) — to odwraca asymetrię PRD §7.6"
            ),
            BrandDataError::Range { min, max } => write!(
                f,
                "brand.ron: stories_max ({max}) poniżej stories_min ({min})"
            ),
        }
    }
}

impl std::error::Error for BrandDataError {}

impl BrandData {
    /// Wczytuje i waliduje plik strojenia marki.
    pub fn load(path: &Path) -> Result<BrandData, BrandDataError> {
        let tekst = std::fs::read_to_string(path).map_err(BrandDataError::Io)?;
        let f: BrandFile = ron::from_str(&tekst).map_err(BrandDataError::Ron)?;
        if f.schema_version != BRAND_SCHEMA_VERSION {
            return Err(BrandDataError::Schema {
                found: f.schema_version,
                want: BRAND_SCHEMA_VERSION,
            });
        }
        if f.memory.k_down <= f.memory.k_up {
            return Err(BrandDataError::Symmetry {
                k_up: f.memory.k_up,
                k_down: f.memory.k_down,
            });
        }
        if f.outlets.stories_max < f.outlets.stories_min {
            return Err(BrandDataError::Range {
                min: f.outlets.stories_min,
                max: f.outlets.stories_max,
            });
        }
        let mut memory = f.memory;
        memory.awareness_gain = f.awareness_gain;
        memory.decay_affinity = f.decay_affinity;
        memory.decay_awareness = f.decay_awareness;
        Ok(BrandData {
            memory,
            channels: f.channels,
            outlets: f.outlets,
        })
    }

    /// Wczytuje z katalogu danych gry.
    pub fn load_default() -> Result<BrandData, BrandDataError> {
        BrandData::load(&magnat_core::data_path("tuning/brand.ron"))
    }
}

impl Default for BrandData {
    fn default() -> BrandData {
        BrandData {
            memory: BrandTuning::default(),
            channels: ChannelTuning {
                billboard_notice_bps: 1200,
                cpm_gr: [18_000, 42_000, 26_000, 95_000, 12_000, 6_000, 55_000, 4_000],
                leaflet_hit_bps: 3_500,
                leaflet_radius_m: 800,
            },
            outlets: OutletTuning {
                base_credibility: 65,
                stories_min: 3,
                stories_max: 8,
                bias_weights: [
                    [6_000, 9_000, 12_000, 5_000, 14_000, 9_000],
                    [9_000, 10_000, 6_000, 15_000, 7_000, 12_000],
                    [14_000, 8_000, 9_000, 12_000, 9_000, 11_000],
                    [11_000, 13_000, 4_000, 12_000, 10_000, 10_000],
                ],
                inventory_daily: [12, 24, 8, 40],
                slot_price_gr: [240_000, 90_000, 1_800_000, 45_000],
            },
        }
    }
}
