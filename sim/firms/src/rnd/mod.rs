//! Badania i rozwój: drzewo technologii, patenty, licencje (M10c, WP10.8, PRD §7.7).
//!
//! ## Co gdzie mieszka i dlaczego akurat tam
//!
//! **Stan siedzi w [`crate::Firms`]** — wiedza firmy, projekt w toku, patenty
//! i licencje. Nie w osobnym zasobie, bo pisze go jeden krok i czyta go ten sam
//! rejestr, a doczepienie go do `Firms` daje hash stanu za darmo. Czyta go też
//! `sim/macro`: `MacroFirm.tech` wychodzi z `Site.tech`, które R&D podnosi —
//! i to jest cała naprawa `FC-2`.
//!
//! **Lista towarów w obiegu siedzi w `Market`**, nie tutaj: na półce stoi oferta,
//! a oferty ma rynek. R&D zgłasza odblokowanie, rynek je wykonuje i on też odsiewa
//! powtórzenie, kiedy ten sam węzeł odkryją dwie firmy.
//!
//! **Katalog i kalibracja siedzą w [`RndData`]** — drzewo z `data/tech/tech.ron`
//! i liczby z `data/tuning/rnd.ron`. To jest **wejście, nie stan**: nie zmienia się
//! w przebiegu i nie wchodzi do hasha, tak samo jak `Catalog` w `ChainHandle`.
//!
//! **Krok prowadzi `sim/economy`**, nie ten crate. Powód jest ten sam, dla którego
//! listę płac księguje tamta strona: badania kosztują pieniądz, a `sim/firms`
//! nie widzi `Books` i widzieć nie może — zależność idzie `economy → firms`.
//! Tutaj są reguły, tam ich wykonanie.
//!
//! ## Czego tu nie ma
//!
//! **Roku kalendarzowego.** `world_year` z pliku przelicza się na dobę świata przy
//! ładowaniu drzewa i od tej chwili wszystko chodzi po dobach. Drugie źródło roku
//! obok `EpochClock` z M8 byłoby dokładnie tym błędem, który `K-39` i `K-60`
//! już raz naprawiały.

pub mod progress;
pub mod state;
pub mod tree;
pub mod tuning;

pub use progress::{mrp_per_day, step_day, PlantEffect, PlantEffectKind, RndDay, RndOutcome};
pub use state::{ChargeKind, License, Patent, Project, RndCharge, RndState};
pub use tree::{BranchId, TechEffect, TechNode, TechTree, TechTreeError, TECH_SCHEMA_VERSION};
pub use tuning::{RndTuning, RndTuningError, RND_SCHEMA_VERSION};

use std::sync::Arc;

/// Drzewo i kalibracja jako zasób świata — **wejście, nie stan**.
///
/// Uchwyt `Arc`, żeby dało się go wyjąć ze świata na czas kroku i trzymać obok
/// `&mut World`: ten sam wzorzec co `ChainHandle` i `Market`, i z tego samego powodu
/// (`K-29`). Zasób nie wchodzi do hasha, bo jest tą samą liczbą w każdym przebiegu.
#[derive(Clone)]
pub struct RndData(Arc<Inner>);

pub struct Inner {
    pub tree: TechTree,
    pub tuning: RndTuning,
}

impl RndData {
    #[must_use]
    pub fn new(tree: TechTree, tuning: RndTuning) -> RndData {
        RndData(Arc::new(Inner { tree, tuning }))
    }

    /// Świat bez drzewa technologii. Zachowuje się **dokładnie** jak przed M10c:
    /// nic się nie odblokowuje, nic nie kosztuje, żaden zakład nie zmienia
    /// wyposażenia. Scenariusze-wycinki (`m5shop`, testy M6) stoją na tym.
    #[must_use]
    pub fn empty() -> RndData {
        RndData::new(TechTree::default(), RndTuning::default())
    }
}

impl Default for RndData {
    fn default() -> RndData {
        RndData::empty()
    }
}

impl std::ops::Deref for RndData {
    type Target = Inner;

    fn deref(&self) -> &Inner {
        &self.0
    }
}

/// Wczytuje drzewo i kalibrację z katalogu danych.
///
/// `start_year` to rok kalendarzowy startu partii — ten sam, który dostaje
/// `EpochClock` w M8 (`game::world`).
pub fn load_default(
    start_year: i32,
    goods: &magnat_supply::Catalog,
) -> Result<RndData, Box<dyn std::error::Error>> {
    let tree = TechTree::load_default(start_year, goods)?;
    let tuning = RndTuning::load_default()?;
    Ok(RndData::new(tree, tuning))
}

/// Towary, które któryś węzeł obiecuje odblokować — czyli te, które na starcie
/// partii **nie istnieją w mieście**.
///
/// Osobna funkcja, bo woła ją ten, kto stawia świat i zakłada blokadę na rynku:
/// drzewo wie, co się kiedyś pojawi, a rynek wie, czego dziś nie ma na półce.
#[must_use]
pub fn gated_goods(tree: &TechTree) -> Vec<magnat_core::GoodId> {
    let mut v: Vec<magnat_core::GoodId> = tree
        .nodes
        .iter()
        .flat_map(|n| n.effects.iter())
        .filter_map(|e| match e {
            TechEffect::NewGood(g) => Some(*g),
            _ => None,
        })
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}
