//! Drzewo technologii: `data/tech/tech.ron`, jego indeksy i walidacja (M10c §5.4).
//!
//! **Rok „światowy" przelicza się na dobę świata przy ładowaniu.** To jest jedyne
//! miejsce w kodzie R&D, w którym pada słowo „rok kalendarzowy": `TechTree::load`
//! bierze rok startowy partii argumentem, tak samo jak `Catalog::load_default` bierze
//! klucz epoki, i od tej chwili symulacja porównuje **doby**. Bez tego trzeba by
//! trzymać rok startowy drugi raz obok `EpochClock` z M8 — a dwa źródła jednej liczby
//! to dokładnie ten błąd, który `K-39` i `K-60` już raz naprawiały.

use magnat_core::{GoodId, RecipeId, TechId};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

/// Wersja schematu `data/tech/tech.ron` (00 §5).
pub const TECH_SCHEMA_VERSION: u32 = 1;

/// Doba roku kalendarzowego przy kalendarzu `K-1`.
pub const DAYS_PER_YEAR: u64 = 360;

/// Gałąź drzewa — szuflada w panelu, nie mechanika.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub struct BranchId(pub u8);

/// Co zmienia odkrycie węzła — **zmiany nieciągłe**, czyli rzeczy, których przed
/// odkryciem nie było wcale.
///
/// Wpływ ciągły (jakość, produktywność) idzie osobnym kanałem: `TechNode::gain`
/// podnosi `Site.tech`, które model jakości M6 już czyta przez `w_tech`. Drugie
/// wejście obok niego byłoby drugą prawdą o tej samej rzeczy.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TechEffect {
    /// Towar wchodzi do obiegu: na półki i do koszyka potrzeby.
    NewGood(GoodId),
    /// Zużycie wsadu tej receptury spada o `bps` punktów bazowych.
    CostReduction { recipe: RecipeId, bps: u16 },
    /// Linie tej klasy maszyn w zakładach firmy dostają przepustowość i MTBF.
    MachineUpgrade {
        machine_class: Box<str>,
        throughput_bps: u16,
        failure_bps: i16,
    },
}

/// Węzeł drzewa.
#[derive(Clone, Debug)]
pub struct TechNode {
    pub id: TechId,
    pub key: Box<str>,
    pub branch: BranchId,
    /// Warunki wstępne, **rosnąco po `TechId`** — kolejność jest danymi, nie
    /// przypadkiem kolejności w pliku (00 §3.2).
    pub prereqs: Vec<TechId>,
    /// Koszt pełny, w punktach badawczych.
    pub cost_rp: u32,
    /// Rok kalendarzowy, od którego technologia jest w obiegu. Pole **do pokazania
    /// graczowi**; logika chodzi po [`TechNode::world_day`].
    pub world_year: i32,
    /// Doba świata, od której technologia jest wiedzą powszechną. `0` znaczy
    /// „już przed startem partii".
    pub world_day: u64,
    /// O ile odkrycie podnosi poziom wyposażenia zakładów firmy (skala `Q`).
    pub gain: u8,
    pub effects: Vec<TechEffect>,
}

impl TechNode {
    /// Koszt, jaki firma faktycznie musi zebrać, w punktach badawczych.
    ///
    /// Po roku „światowym" wiedza jest w obiegu i da się ją podpatrzeć — stąd zniżka.
    /// To jest cała różnica między trybem 1 a trybem 2 z §5.4.
    #[must_use]
    pub fn effective_cost_rp(&self, day: u64, discount_bp: u16) -> u32 {
        if day < self.world_day {
            return self.cost_rp;
        }
        let zostaje = 10_000u64.saturating_sub(u64::from(discount_bp));
        u32::try_from(u64::from(self.cost_rp) * zostaje / 10_000).unwrap_or(self.cost_rp)
    }

    /// Czy odkrycie w tej dobie daje patent — czyli czy wyprzedza świat.
    #[must_use]
    pub fn patentable(&self, day: u64) -> bool {
        day < self.world_day
    }
}

/// Drzewo technologii — katalog, nie stan.
///
/// `TechId` jest pozycją **posortowanego klucza tekstowego**, tak samo jak `GoodId`
/// (00 §5), więc dopisanie węzła nie przenumerowuje zapisów gry.
///
/// **Indeksu „węzły tej gałęzi" tu nie ma.** Gałąź jest szufladą w panelu, a panelu
/// nie ma — indeks budowany przy każdym ładowaniu dla nikogo byłby dokładnie tym,
/// przed czym broni `K-67`. Panel R&D (M10f) dołoży go razem z pierwszym czytelnikiem;
/// to jest pętla po dwunastu węzłach, nie mechanizm.
#[derive(Clone, Debug, Default)]
pub struct TechTree {
    pub nodes: Vec<TechNode>,
    pub branches: Vec<Box<str>>,
}

impl TechTree {
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    #[must_use]
    pub fn node(&self, t: TechId) -> &TechNode {
        &self.nodes[t.0 as usize]
    }

    #[must_use]
    pub fn get(&self, t: TechId) -> Option<&TechNode> {
        self.nodes.get(t.0 as usize)
    }

    #[must_use]
    pub fn id(&self, key: &str) -> Option<TechId> {
        self.nodes
            .binary_search_by(|n| (*n.key).cmp(key))
            .ok()
            .map(|i| TechId(i as u16))
    }

    /// Drzewo z gotowych części — dla atrap testowych. **Nie waliduje**: walidacja
    /// jest w [`TechTree::load`], bo tam błąd danych ma zatrzymać uruchomienie.
    #[must_use]
    pub fn from_parts(nodes: Vec<TechNode>, branches: Vec<Box<str>>) -> TechTree {
        TechTree { nodes, branches }
    }
}

// ── plik ─────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TechFile {
    schema_version: u32,
    branches: Vec<BranchSpec>,
    nodes: Vec<NodeSpec>,
}

#[derive(Deserialize)]
struct BranchSpec {
    key: String,
}

#[derive(Deserialize)]
struct NodeSpec {
    key: String,
    branch: String,
    #[serde(default)]
    prereqs: Vec<String>,
    cost_rp: u32,
    world_year: i32,
    #[serde(default)]
    gain: u8,
    #[serde(default)]
    effects: Vec<EffectSpec>,
}

#[derive(Deserialize)]
enum EffectSpec {
    NewGood(String),
    CostReduction {
        recipe: String,
        bps: u16,
    },
    MachineUpgrade {
        machine_class: String,
        throughput_bps: u16,
        failure_bps: i16,
    },
}

/// Błąd wczytania albo walidacji `data/tech/tech.ron`.
#[derive(Debug)]
pub enum TechTreeError {
    Io(std::io::Error),
    Ron(ron::error::SpannedError),
    Schema {
        found: u32,
        want: u32,
    },
    DuplicateKey(String),
    UnknownBranch {
        node: String,
        branch: String,
    },
    UnknownPrereq {
        node: String,
        prereq: String,
    },
    /// Węzeł, który jest swoim własnym warunkiem — bezpośrednio albo przez łańcuch.
    /// Bez tej reguły firma wybierałaby projekt, którego nie da się zacząć nigdy,
    /// a panel pokazywałby drzewo z pętlą.
    Cycle(Vec<String>),
    UnknownGood {
        node: String,
        good: String,
    },
    UnknownRecipe {
        node: String,
        recipe: String,
    },
    UnknownMachineClass {
        node: String,
        machine_class: String,
    },
    /// Koszt zerowy: technologia odkrywana w pierwszej dobie, zanim ktokolwiek
    /// zatrudni badacza.
    ZeroCost(String),
}

impl std::fmt::Display for TechTreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TechTreeError::Io(e) => write!(f, "tech.ron: {e}"),
            TechTreeError::Ron(e) => write!(f, "tech.ron: {e}"),
            TechTreeError::Schema { found, want } => {
                write!(f, "tech.ron: schema_version {found}, oczekiwano {want}")
            }
            TechTreeError::DuplicateKey(k) => write!(f, "tech.ron: klucz `{k}` dwa razy"),
            TechTreeError::UnknownBranch { node, branch } => {
                write!(f, "tech.ron: `{node}` wskazuje nieznaną gałąź `{branch}`")
            }
            TechTreeError::UnknownPrereq { node, prereq } => {
                write!(f, "tech.ron: `{node}` wymaga nieznanego węzła `{prereq}`")
            }
            TechTreeError::Cycle(v) => {
                write!(f, "tech.ron: cykl warunków wstępnych: {}", v.join(" → "))
            }
            TechTreeError::UnknownGood { node, good } => write!(
                f,
                "tech.ron: `{node}` odblokowuje towar `{good}`, którego nie ma w data/goods/"
            ),
            TechTreeError::UnknownRecipe { node, recipe } => write!(
                f,
                "tech.ron: `{node}` obniża koszt receptury `{recipe}`, której nie ma w data/recipes/"
            ),
            TechTreeError::UnknownMachineClass {
                node,
                machine_class,
            } => write!(
                f,
                "tech.ron: `{node}` ulepsza klasę maszyn `{machine_class}`, której nie używa żadna receptura"
            ),
            TechTreeError::ZeroCost(k) => {
                write!(f, "tech.ron: `{k}` ma koszt zero — odkryłby się sam")
            }
        }
    }
}

impl std::error::Error for TechTreeError {}

impl TechTree {
    /// Wczytuje i **waliduje w całości** drzewo technologii.
    ///
    /// `start_year` to rok kalendarzowy, w którym zaczyna się partia — ten sam,
    /// który dostaje `EpochClock` w M8. Z niego i z `world_year` liczy się
    /// [`TechNode::world_day`]; węzeł, którego rok minął przed startem, dostaje
    /// dobę zero i patentu nie da już nikomu.
    ///
    /// `goods` służy wyłącznie walidacji odnośników — drzewo nie trzyma katalogu.
    pub fn load(
        path: &Path,
        start_year: i32,
        goods: &magnat_supply::Catalog,
    ) -> Result<TechTree, TechTreeError> {
        let tekst = std::fs::read_to_string(path).map_err(TechTreeError::Io)?;
        let f: TechFile = ron::from_str(&tekst).map_err(TechTreeError::Ron)?;
        if f.schema_version != TECH_SCHEMA_VERSION {
            return Err(TechTreeError::Schema {
                found: f.schema_version,
                want: TECH_SCHEMA_VERSION,
            });
        }
        let branches: Vec<Box<str>> = f.branches.into_iter().map(|b| b.key.into()).collect();

        // Kolejność alfabetyczna klucza: `TechId` ma nie zależeć od tego, w którym
        // miejscu pliku węzeł stoi (00 §5).
        let mut specs = f.nodes;
        specs.sort_by(|a, b| a.key.cmp(&b.key));
        let mut index: BTreeMap<&str, TechId> = BTreeMap::new();
        for (i, s) in specs.iter().enumerate() {
            if index.insert(&s.key, TechId(i as u16)).is_some() {
                return Err(TechTreeError::DuplicateKey(s.key.clone()));
            }
        }

        let mut nodes = Vec::with_capacity(specs.len());
        for (i, s) in specs.iter().enumerate() {
            if s.cost_rp == 0 {
                return Err(TechTreeError::ZeroCost(s.key.clone()));
            }
            let branch = branches
                .iter()
                .position(|b| **b == *s.branch)
                .map(|p| BranchId(p as u8))
                .ok_or_else(|| TechTreeError::UnknownBranch {
                    node: s.key.clone(),
                    branch: s.branch.clone(),
                })?;
            let mut prereqs = Vec::with_capacity(s.prereqs.len());
            for p in &s.prereqs {
                let id = *index
                    .get(p.as_str())
                    .ok_or_else(|| TechTreeError::UnknownPrereq {
                        node: s.key.clone(),
                        prereq: p.clone(),
                    })?;
                prereqs.push(id);
            }
            prereqs.sort_unstable();
            prereqs.dedup();

            let mut effects = Vec::with_capacity(s.effects.len());
            for e in &s.effects {
                effects.push(match e {
                    EffectSpec::NewGood(g) => {
                        TechEffect::NewGood(goods.good_id(g).ok_or_else(|| {
                            TechTreeError::UnknownGood {
                                node: s.key.clone(),
                                good: g.clone(),
                            }
                        })?)
                    }
                    EffectSpec::CostReduction { recipe, bps } => TechEffect::CostReduction {
                        recipe: goods.recipe_id(recipe).ok_or_else(|| {
                            TechTreeError::UnknownRecipe {
                                node: s.key.clone(),
                                recipe: recipe.clone(),
                            }
                        })?,
                        bps: *bps,
                    },
                    EffectSpec::MachineUpgrade {
                        machine_class,
                        throughput_bps,
                        failure_bps,
                    } => {
                        if goods.machine_class_id(machine_class).is_none() {
                            return Err(TechTreeError::UnknownMachineClass {
                                node: s.key.clone(),
                                machine_class: machine_class.clone(),
                            });
                        }
                        TechEffect::MachineUpgrade {
                            machine_class: machine_class.clone().into_boxed_str(),
                            throughput_bps: *throughput_bps,
                            failure_bps: *failure_bps,
                        }
                    }
                });
            }

            let world_day = u64::try_from(s.world_year - start_year)
                .unwrap_or(0)
                .saturating_mul(DAYS_PER_YEAR);
            nodes.push(TechNode {
                id: TechId(i as u16),
                key: s.key.clone().into_boxed_str(),
                branch,
                prereqs,
                cost_rp: s.cost_rp,
                world_year: s.world_year,
                world_day,
                gain: s.gain,
                effects,
            });
        }

        let tree = TechTree::from_parts(nodes, branches);
        tree.validate_acyclic()?;
        Ok(tree)
    }

    /// Wczytuje z katalogu danych gry.
    pub fn load_default(
        start_year: i32,
        goods: &magnat_supply::Catalog,
    ) -> Result<TechTree, TechTreeError> {
        TechTree::load(&magnat_core::data_path("tech/tech.ron"), start_year, goods)
    }

    /// Brak cykli w warunkach wstępnych — sortowanie topologiczne po `TechId`.
    ///
    /// Osobna metoda publiczna, bo pyta o nią także walidator w teście CI,
    /// a nie tylko ładowarka.
    pub fn validate_acyclic(&self) -> Result<(), TechTreeError> {
        let n = self.nodes.len();
        let mut brakow: Vec<u32> = self.nodes.iter().map(|x| x.prereqs.len() as u32).collect();
        let mut kolejka: Vec<TechId> = (0..n)
            .filter(|i| brakow[*i] == 0)
            .map(|i| TechId(i as u16))
            .collect();
        let mut gotowe = 0usize;
        while let Some(t) = kolejka.pop() {
            gotowe += 1;
            for (i, x) in self.nodes.iter().enumerate() {
                if x.prereqs.contains(&t) {
                    brakow[i] -= 1;
                    if brakow[i] == 0 {
                        kolejka.push(TechId(i as u16));
                    }
                }
            }
        }
        if gotowe == n {
            return Ok(());
        }
        let w_cyklu: Vec<String> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(i, _)| brakow[*i] > 0)
            .map(|(_, x)| x.key.to_string())
            .collect();
        Err(TechTreeError::Cycle(w_cyklu))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wezel(id: u16, key: &str, prereqs: Vec<u16>, cost: u32, world_day: u64) -> TechNode {
        TechNode {
            id: TechId(id),
            key: key.into(),
            branch: BranchId(0),
            prereqs: prereqs.into_iter().map(TechId).collect(),
            cost_rp: cost,
            world_year: 1970,
            world_day,
            gain: 5,
            effects: Vec::new(),
        }
    }

    #[test]
    fn rok_swiatowy_dzieli_koszt_i_patent() {
        let n = wezel(0, "a", vec![], 1_000, 3_600);
        // Przed rokiem światowym: pełny koszt, patent możliwy.
        assert_eq!(n.effective_cost_rp(0, 6_000), 1_000);
        assert!(n.patentable(0));
        // Po nim: −60 % i patentu już nie ma.
        assert_eq!(n.effective_cost_rp(3_600, 6_000), 400);
        assert!(!n.patentable(3_600));
    }

    #[test]
    fn cykl_warunkow_jest_bledem() {
        let t = TechTree::from_parts(
            vec![
                wezel(0, "a", vec![1], 100, 0),
                wezel(1, "b", vec![0], 100, 0),
            ],
            vec!["x".into()],
        );
        let Err(TechTreeError::Cycle(v)) = t.validate_acyclic() else {
            panic!("cykl nie został wykryty");
        };
        assert_eq!(v, vec!["a".to_owned(), "b".to_owned()]);
    }

    #[test]
    fn drzewo_bez_cyklu_przechodzi() {
        let t = TechTree::from_parts(
            vec![
                wezel(0, "a", vec![], 100, 0),
                wezel(1, "b", vec![0], 100, 0),
            ],
            vec!["x".into()],
        );
        assert!(t.validate_acyclic().is_ok());
    }
}
