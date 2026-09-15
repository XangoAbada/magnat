//! Dane gospodarki detalicznej — `data/economy/` (M5b §5.3, §5.4).
//!
//! Cztery pliki, czterech różnych właścicieli zmiany:
//! - `weights.ron` — wagi bazowe funkcji użyteczności per potrzeba; zmienia je projektant,
//! - `choice.ron` — temperatura, szum, progi, promień; zmienia je **balansator** (M5e),
//! - `retail.ron` — kategoria, trwałość, czas dostawy i narzut per towar; zmienia je modder,
//! - `shop.ron` — osobowość cenowa firmy i koszty stałe zakładu (M5c); stroi je
//!   **balansator**, bo to `min_margin_bp` stąd jest dolnym ogranicznikiem, którego
//!   pilnuje bramka G3 (brak spirali deflacji).
//!
//! Żaden z nich nie powtarza `data/goods/`: cena hurtowa, gęstość i masa sztuki
//! zostają w katalogu towarów, bo to jedno źródło prawdy o towarze (00 §5).
//!
//! Kształt loadera jest ten sam co w `data/needs/` i `data/vehicles/`: stała
//! `*_SCHEMA_VERSION`, prywatny `*File` z wersją, jawny `enum` błędu i walidacja
//! **kompletności przed użyciem** — brak wpisu jest błędem ładowania, nie cichym zerem.

use std::path::Path;

use magnat_core::{Money, NeedKind, PlaceKind, Rng, StockCat, NEED_COUNT, STOCK_CAT_COUNT};
use serde::Deserialize;

pub const ECONOMY_SCHEMA_VERSION: u32 = 1;

// ── błędy ────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum EconomyDataError {
    Io(std::io::Error),
    Ron { file: &'static str, msg: String },
    Schema {
        file: &'static str,
        found: u32,
        want: u32,
    },
    /// Klucz tekstowy nie odpowiada żadnemu wariantowi słownika w `core`.
    UnknownKey { file: &'static str, key: String },
    Duplicate { file: &'static str, key: String },
    /// Słownik jest kompletny z definicji, a w pliku brakuje wariantu.
    Missing { file: &'static str, key: &'static str },
}

impl std::fmt::Display for EconomyDataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EconomyDataError::Io(e) => write!(f, "data/economy: {e}"),
            EconomyDataError::Ron { file, msg } => write!(f, "{file}: {msg}"),
            EconomyDataError::Schema { file, found, want } => {
                write!(f, "{file}: schema_version {found}, oczekiwano {want}")
            }
            EconomyDataError::UnknownKey { file, key } => write!(f, "{file}: nieznany klucz {key}"),
            EconomyDataError::Duplicate { file, key } => write!(f, "{file}: powtórzony klucz {key}"),
            EconomyDataError::Missing { file, key } => write!(f, "{file}: brakuje wpisu {key}"),
        }
    }
}

impl std::error::Error for EconomyDataError {}

impl From<std::io::Error> for EconomyDataError {
    fn from(e: std::io::Error) -> EconomyDataError {
        EconomyDataError::Io(e)
    }
}

fn read<T: serde::de::DeserializeOwned>(
    path: &Path,
    file: &'static str,
) -> Result<T, EconomyDataError> {
    let txt = std::fs::read_to_string(path)?;
    ron::from_str(&txt).map_err(|e| EconomyDataError::Ron {
        file,
        msg: e.to_string(),
    })
}

/// Indeks wariantu słownika po kluczu tekstowym — bez rozróżniania wielkości liter,
/// tak samo jak w `data/needs/`.
fn need_index(file: &'static str, key: &str) -> Result<usize, EconomyDataError> {
    NeedKind::ALL
        .iter()
        .position(|n| n.name().eq_ignore_ascii_case(key))
        .ok_or_else(|| EconomyDataError::UnknownKey {
            file,
            key: key.to_string(),
        })
}

// ── weights.ron ──────────────────────────────────────────────────────────────────

/// Wagi bazowe członów funkcji użyteczności (§5.4). Przed użyciem modulowane
/// osobowością i statusem, potem normalizowane do `Σ|w| = 1`.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct UtilityWeights {
    pub price: f64,
    pub quality: f64,
    pub brand: f64,
    pub dist: f64,
    pub loyalty: f64,
    pub status: f64,
    pub novelty: f64,
}

impl UtilityWeights {
    /// Normalizacja `Σ|w| = 1` — to ona daje progowi `thr0` wspólną skalę
    /// dla wszystkich potrzeb. Suma zerowa zostawia wagi bez zmian: waga zerowa
    /// dla każdego członu znaczy „ta potrzeba nie jest zakupem", a nie dzielenie
    /// przez zero.
    #[must_use]
    pub fn normalized(self) -> UtilityWeights {
        let s = self.price.abs()
            + self.quality.abs()
            + self.brand.abs()
            + self.dist.abs()
            + self.loyalty.abs()
            + self.status.abs()
            + self.novelty.abs();
        if s <= 0.0 {
            return self;
        }
        UtilityWeights {
            price: self.price / s,
            quality: self.quality / s,
            brand: self.brand / s,
            dist: self.dist / s,
            loyalty: self.loyalty / s,
            status: self.status / s,
            novelty: self.novelty / s,
        }
    }
}

#[derive(Deserialize)]
struct WeightRow {
    key: String,
    price: f64,
    quality: f64,
    brand: f64,
    dist: f64,
    loyalty: f64,
    status: f64,
    novelty: f64,
}

#[derive(Deserialize)]
struct WeightsFile {
    schema_version: u32,
    needs: Vec<WeightRow>,
}

// ── choice.ron ───────────────────────────────────────────────────────────────────

/// Próg odłożenia zakupu per potrzeba (§5.4).
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct ThresholdSpec {
    pub thr0: f64,
    pub k_urgency: f64,
    pub k_envelope: f64,
}

#[derive(Deserialize)]
struct ThresholdRow {
    key: String,
    thr0: f64,
    k_urgency: f64,
    k_envelope: f64,
}

#[derive(Deserialize)]
struct BudgetRow {
    cat: StockCat,
    gr: i64,
}

#[derive(Deserialize)]
struct ChoiceFile {
    schema_version: u32,
    temperature: f64,
    noise_sigma: f64,
    k_min: usize,
    k_max: usize,
    radius_m: u32,
    price_slippage_bp: i32,
    vot_base_gr_per_min: i64,
    purchase_days: u8,
    home_stock_min_days: u8,
    budget_ref_gr: Vec<BudgetRow>,
    thresholds: Vec<ThresholdRow>,
}

// ── retail.ron ───────────────────────────────────────────────────────────────────

#[derive(Clone, Deserialize)]
pub struct RetailGood {
    pub key: String,
    pub cat: StockCat,
    pub quality: u8,
    /// `0` = towar się nie psuje.
    pub shelf_life_days: u16,
    pub lead_time_days: u8,
    pub markup_bp: Option<i32>,
}

#[derive(Clone, Deserialize)]
pub struct ShopKindRow {
    pub kind: PlaceKind,
    pub cats: Vec<StockCat>,
}

#[derive(Deserialize)]
struct RetailFile {
    schema_version: u32,
    default_markup_bp: i32,
    shop_kinds: Vec<ShopKindRow>,
    goods: Vec<RetailGood>,
}

/// Parametry detaliczne towarów i asortymentu sklepów.
///
/// Kolejność `goods` w obrębie kategorii jest **rangą substytutu** (§5.4 krok 1)
/// i jest kontraktem danych — przestawienie zmienia to, co mieszkaniec kupuje,
/// kiedy pierwszy wybór nie przeszedł progu.
#[derive(Clone)]
pub struct RetailTable {
    pub default_markup_bp: i32,
    pub goods: Vec<RetailGood>,
    shop_kinds: Vec<ShopKindRow>,
}

impl RetailTable {
    /// Kategorie, którymi handluje sklep tego rodzaju. Pusty wynik = archetyp,
    /// którego M5 nie obsadza ofertą (np. stacja paliw — jej obrót wnosi M4/T-2).
    #[must_use]
    pub fn cats_for(&self, kind: PlaceKind) -> &[StockCat] {
        self.shop_kinds
            .iter()
            .find(|r| r.kind == kind)
            .map_or(&[][..], |r| &r.cats)
    }

    /// Czy ten rodzaj miejsca jest w ogóle sklepem w rozumieniu M5.
    #[must_use]
    pub fn is_shop(&self, kind: PlaceKind) -> bool {
        !self.cats_for(kind).is_empty()
    }
}

// ── shop.ron ─────────────────────────────────────────────────────────────────────

/// Widełki parametru losowanego raz na firmę (M5c §5.6).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub struct Range {
    pub min: i32,
    pub max: i32,
}

impl Range {
    /// Losuje z zakresu domkniętego. Pusty albo odwrócony zakres daje `min` —
    /// dane wadliwe nie mają prawa panikować w środku generacji świata.
    pub fn pick(self, r: &mut Rng) -> i32 {
        if self.max <= self.min {
            return self.min;
        }
        let szerokosc = u32::try_from(i64::from(self.max) - i64::from(self.min) + 1).unwrap_or(1);
        self.min + r.gen_range_u32(szerokosc) as i32
    }
}

/// Próg przeceny psującego się towaru: „zostało ≤ `days_left` dni → korekta `adj_bp`".
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub struct SpoilageStep {
    pub days_left: u16,
    pub adj_bp: i32,
}

/// Parametry polityki cenowej — wejście [`crate::pricing::FirmPricing::draw`]
/// i [`crate::pricing::reprice`].
#[derive(Clone, Debug, Deserialize)]
pub struct PricingParams {
    pub target_margin_bp: Range,
    pub min_margin_bp: Range,
    pub max_margin_bp: Range,
    pub k_stock: Range,
    pub k_comp: Range,
    pub risk: Range,
    pub experiment_risk_min: i32,
    pub experiment_cooldown_days: u16,
    pub experiment_bp: Range,
    pub experiment_len_days: u8,
    pub experiment_noise_permille: i32,
    /// Czytane od góry: pierwszy próg, w którym się mieścimy, wygrywa. Loader
    /// **sortuje rosnąco po `days_left`**, więc kolejność w pliku nie jest kontraktem.
    pub spoilage: Vec<SpoilageStep>,
    pub observe_delay_days: Range,
    pub observe_radius_m: u32,
}

/// Koszty stałe zakładu, miesięcznie (kalendarz 360-dniowy, `K-1`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub struct ShopCosts {
    pub rent_gr_per_slot: i64,
    pub utilities_gr_per_slot: i64,
    pub wages_gr_per_slot: i64,
    pub equipment_gr_per_slot: i64,
    pub depreciation_months: u32,
}

impl ShopCosts {
    /// Suma kosztów stałych sklepu o tylu miejscach na półce.
    #[must_use]
    pub fn monthly(&self, slots: u16) -> (Money, Money, Money) {
        let n = i64::from(slots.max(1));
        (
            Money(self.rent_gr_per_slot * n),
            Money(self.utilities_gr_per_slot * n),
            Money(self.wages_gr_per_slot * n),
        )
    }

    /// Wartość początkowa wyposażenia i miesięczny odpis liniowy.
    #[must_use]
    pub fn equipment(&self, slots: u16) -> (Money, Money) {
        let wartosc = Money(self.equipment_gr_per_slot * i64::from(slots.max(1)));
        let odpis = wartosc.div_round_half_up(i64::from(self.depreciation_months.max(1)));
        (wartosc, odpis)
    }
}

#[derive(Deserialize)]
struct ShopFile {
    schema_version: u32,
    pricing: PricingParams,
    costs: ShopCosts,
}

// ── złożone dane fazy ────────────────────────────────────────────────────────────

/// Wszystko, co decyzja zakupowa czyta z `data/economy/`.
pub struct EconomyData {
    weights: [UtilityWeights; NEED_COUNT],
    thresholds: [ThresholdSpec; NEED_COUNT],
    budget_ref: [Money; STOCK_CAT_COUNT],
    pub temperature: f64,
    pub noise_sigma: f64,
    pub k_min: usize,
    pub k_max: usize,
    pub radius_m: u32,
    pub price_slippage_bp: i32,
    pub vot_base_gr_per_min: i64,
    pub purchase_days: u8,
    pub home_stock_min_days: u8,
    pub retail: RetailTable,
    pub pricing: PricingParams,
    pub costs: ShopCosts,
}

impl EconomyData {
    #[must_use]
    pub fn base_weights(&self, need: NeedKind) -> UtilityWeights {
        self.weights[need.as_index()]
    }

    #[must_use]
    pub fn threshold(&self, need: NeedKind) -> ThresholdSpec {
        self.thresholds[need.as_index()]
    }

    /// Mianownik członu ceny (§5.4). M5d podmieni to na kopertę gospodarstwa.
    #[must_use]
    pub fn budget_ref(&self, cat: StockCat) -> Money {
        self.budget_ref[cat.as_index()]
    }

    /// Ładowanie z katalogu `data/economy/`.
    pub fn load(dir: &Path) -> Result<EconomyData, EconomyDataError> {
        let wf: WeightsFile = read(&dir.join("weights.ron"), "economy/weights.ron")?;
        if wf.schema_version != ECONOMY_SCHEMA_VERSION {
            return Err(EconomyDataError::Schema {
                file: "economy/weights.ron",
                found: wf.schema_version,
                want: ECONOMY_SCHEMA_VERSION,
            });
        }
        let mut weights = [UtilityWeights::default(); NEED_COUNT];
        let mut seen = [false; NEED_COUNT];
        for row in &wf.needs {
            let i = need_index("economy/weights.ron", &row.key)?;
            if seen[i] {
                return Err(EconomyDataError::Duplicate {
                    file: "economy/weights.ron",
                    key: row.key.clone(),
                });
            }
            seen[i] = true;
            weights[i] = UtilityWeights {
                price: row.price,
                quality: row.quality,
                brand: row.brand,
                dist: row.dist,
                loyalty: row.loyalty,
                status: row.status,
                novelty: row.novelty,
            };
        }
        if let Some(i) = seen.iter().position(|s| !s) {
            return Err(EconomyDataError::Missing {
                file: "economy/weights.ron",
                key: NeedKind::ALL[i].name(),
            });
        }

        let cf: ChoiceFile = read(&dir.join("choice.ron"), "economy/choice.ron")?;
        if cf.schema_version != ECONOMY_SCHEMA_VERSION {
            return Err(EconomyDataError::Schema {
                file: "economy/choice.ron",
                found: cf.schema_version,
                want: ECONOMY_SCHEMA_VERSION,
            });
        }
        let mut thresholds = [ThresholdSpec::default(); NEED_COUNT];
        let mut seen = [false; NEED_COUNT];
        for row in &cf.thresholds {
            let i = need_index("economy/choice.ron", &row.key)?;
            if seen[i] {
                return Err(EconomyDataError::Duplicate {
                    file: "economy/choice.ron",
                    key: row.key.clone(),
                });
            }
            seen[i] = true;
            thresholds[i] = ThresholdSpec {
                thr0: row.thr0,
                k_urgency: row.k_urgency,
                k_envelope: row.k_envelope,
            };
        }
        if let Some(i) = seen.iter().position(|s| !s) {
            return Err(EconomyDataError::Missing {
                file: "economy/choice.ron",
                key: NeedKind::ALL[i].name(),
            });
        }
        let mut budget_ref = [Money::ZERO; STOCK_CAT_COUNT];
        let mut seen_cat = [false; STOCK_CAT_COUNT];
        for row in &cf.budget_ref_gr {
            let i = row.cat.as_index();
            if seen_cat[i] {
                return Err(EconomyDataError::Duplicate {
                    file: "economy/choice.ron",
                    key: row.cat.name().to_string(),
                });
            }
            seen_cat[i] = true;
            budget_ref[i] = Money(row.gr);
        }
        if let Some(i) = seen_cat.iter().position(|s| !s) {
            return Err(EconomyDataError::Missing {
                file: "economy/choice.ron",
                key: StockCat::ALL[i].name(),
            });
        }

        let rf: RetailFile = read(&dir.join("retail.ron"), "economy/retail.ron")?;
        if rf.schema_version != ECONOMY_SCHEMA_VERSION {
            return Err(EconomyDataError::Schema {
                file: "economy/retail.ron",
                found: rf.schema_version,
                want: ECONOMY_SCHEMA_VERSION,
            });
        }
        for (i, g) in rf.goods.iter().enumerate() {
            if rf.goods[..i].iter().any(|h| h.key == g.key) {
                return Err(EconomyDataError::Duplicate {
                    file: "economy/retail.ron",
                    key: g.key.clone(),
                });
            }
        }

        let mut sf: ShopFile = read(&dir.join("shop.ron"), "economy/shop.ron")?;
        if sf.schema_version != ECONOMY_SCHEMA_VERSION {
            return Err(EconomyDataError::Schema {
                file: "economy/shop.ron",
                found: sf.schema_version,
                want: ECONOMY_SCHEMA_VERSION,
            });
        }
        if sf.pricing.spoilage.is_empty() {
            return Err(EconomyDataError::Missing {
                file: "economy/shop.ron",
                key: "pricing.spoilage",
            });
        }
        // Progi przeceny czyta się „pierwszy pasujący wygrywa", więc muszą iść od
        // najkrótszego terminu. Sortowanie tutaj znaczy, że kolejność w pliku nie
        // jest kontraktem i modder nie zepsuje przeceny przestawieniem wierszy.
        sf.pricing.spoilage.sort_by_key(|s| s.days_left);

        Ok(EconomyData {
            weights,
            thresholds,
            budget_ref,
            temperature: cf.temperature,
            noise_sigma: cf.noise_sigma,
            k_min: cf.k_min,
            k_max: cf.k_max.min(magnat_agents::MAX_CANDIDATES),
            radius_m: cf.radius_m,
            price_slippage_bp: cf.price_slippage_bp,
            vot_base_gr_per_min: cf.vot_base_gr_per_min,
            purchase_days: cf.purchase_days.max(1),
            home_stock_min_days: cf.home_stock_min_days,
            retail: RetailTable {
                default_markup_bp: rf.default_markup_bp,
                goods: rf.goods,
                shop_kinds: rf.shop_kinds,
            },
            pricing: sf.pricing,
            costs: sf.costs,
        })
    }

    pub fn load_default() -> Result<EconomyData, EconomyDataError> {
        EconomyData::load(&magnat_core::data_path("economy"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dane_sie_laduja_i_sa_kompletne() {
        let d = EconomyData::load_default().expect("data/economy/");
        // Kompletność jest wymuszona ładowaniem, więc test sprawdza, że wymuszenie
        // działa: każda potrzeba ma wagi i próg, każda kategoria ma budżet odniesienia.
        for n in NeedKind::ALL {
            assert!(d.threshold(*n).thr0 <= 0.0, "{}", n.name());
        }
        for c in StockCat::ALL {
            assert!(d.budget_ref(*c).get() > 0, "{}", c.name());
        }
        assert!(d.temperature > 0.0);
        assert!(d.retail.is_shop(PlaceKind::Grocery));
        assert!(!d.retail.is_shop(PlaceKind::Home));
    }

    #[test]
    fn normalizacja_daje_wspolna_skale() {
        let d = EconomyData::load_default().expect("data/economy/");
        for n in NeedKind::ALL {
            let w = d.base_weights(*n).normalized();
            let s = w.price + w.quality + w.brand + w.dist + w.loyalty + w.status + w.novelty;
            assert!((s - 1.0).abs() < 1e-9, "{}: Σ|w| = {s}", n.name());
        }
    }

    #[test]
    fn ranga_substytutu_stawia_chleb_przed_miesem() {
        // Kolejność w pliku jest kontraktem — test pilnuje, żeby nikt jej nie
        // „posprzątał" alfabetycznie.
        let d = EconomyData::load_default().expect("data/economy/");
        let poz = |k: &str| d.retail.goods.iter().position(|g| g.key == k).unwrap();
        assert!(poz("bread") < poz("meat"));
        assert!(poz("potato") < poz("cheese"));
    }
}
