//! Scenariusze, cele i łatki świata (M9e WP12, §5.11, PRD §13.3).
//!
//! # Cel jest zapytaniem o świat, a nie licznikiem obok niego
//!
//! [`Goal`] nie trzyma stanu poza jedną liczbą: ile dób z rzędu warunek jest
//! spełniony. Wszystko inne liczy się ze świata w chwili pytania, raz na dobę.
//! Dzięki temu cel nie może się rozjechać z tym, co gracz widzi w panelu, a zapis
//! gry nie musi go wieźć.
//!
//! # Czego w celach nie ma i dlaczego
//!
//! §5.11 wymienia siedem wariantów. Dwa nie powstają:
//!
//! - **`ProductLaunched`** — własna marka i receptury należą do M10. Cel, którego
//!   nie da się osiągnąć, wygląda w panelu tak samo jak osiągalny (`K-67`).
//! - **`Custom(ConditionExpr)`** — z tego samego powodu, dla którego wyrażeniem
//!   przestał być filtr encji (`DG-4`) i warunek zatrzymania: metryki języka reguł
//!   opisują **jeden zakład**, a cel scenariusza pyta o całą grę.
//!
//! # Łatki świata
//!
//! [`WorldPatch`] wykonuje się **w ticku 0, po generacji, przed pierwszym systemem**,
//! więc hash pozostaje funkcją `(ziarno, lista łatek)` — tak, jak zapowiadała
//! decyzja otwarta nr 9 dokumentu fazy. Łatki są dwie i obie mają skutek widoczny
//! w pierwszej dobie; trzeciej nie ma, bo nie było jej po co dodawać.

use std::collections::BTreeMap;

use magnat_core::Money;
use serde::Deserialize;

use crate::career::Holdings;
use crate::Session;

/// Wersja schematu pliku scenariuszy.
pub const SCENARIOS_SCHEMA_VERSION: u32 = 1;

/// Numer celu w scenariuszu.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Deserialize)]
pub struct ObjectiveId(pub u8);

/// Co trzeba osiągnąć.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
pub enum Goal {
    /// Udział w obrocie towarem w punktach bazowych. Towar **kluczem tekstowym**
    /// (00 §5) — indeks przesuwa się przy każdym nowym towarze w katalogu.
    MarketShare { good: String, min_bp: u16 },
    /// Ile zakładów prowadzi gracz.
    SiteCount { min: u32 },
    /// Ilu ludzi zatrudnia; `rank` = miejsce wśród pracodawców miasta (1 = pierwszy).
    Employment {
        min_headcount: u32,
        rank: Option<u8>,
    },
    /// Zakład wypłacalny nieprzerwanie przez `for_days` dób.
    SiteSolvent { ordinal: u32, for_days: u32 },
    /// Majątek: gotówka gospodarstwa plus kapitał własny zakładów.
    NetWorth { min: i64 },
}

impl Goal {
    /// Klucz tekstu: `ui.goal.<key>`.
    #[must_use]
    pub const fn key(&self) -> &'static str {
        match self {
            Goal::MarketShare { .. } => "market_share",
            Goal::SiteCount { .. } => "site_count",
            Goal::Employment { .. } => "employment",
            Goal::SiteSolvent { .. } => "site_solvent",
            Goal::NetWorth { .. } => "net_worth",
        }
    }

    /// Postęp w punktach bazowych, `0..=10000`. To jest liczba w pasku postępu.
    #[must_use]
    pub fn progress_bp(&self, session: &Session, h: &Holdings) -> u16 {
        let ulamek = |ile: i64, cel: i64| -> u16 {
            if cel <= 0 {
                return 10_000;
            }
            u16::try_from((ile.max(0).saturating_mul(10_000) / cel).min(10_000)).unwrap_or(10_000)
        };
        match self {
            Goal::MarketShare { good, min_bp } => {
                let Some(m) = session.market.as_ref() else {
                    return 0;
                };
                let Some(g) = m.good_of_key(good) else {
                    return 0;
                };
                let udzial = m.turnover_share_bp(g, None, &h.sites).unwrap_or(0);
                ulamek(i64::from(udzial), i64::from(*min_bp))
            }
            Goal::SiteCount { min } => {
                ulamek(i64::try_from(h.sites.len()).unwrap_or(0), i64::from(*min))
            }
            Goal::Employment {
                min_headcount,
                rank,
            } => {
                let po_liczbie = ulamek(i64::from(h.employees), i64::from(*min_headcount));
                match rank {
                    None => po_liczbie,
                    // Miejsce w rankingu jest warunkiem **dodatkowym**: postęp pokazuje
                    // ten z dwóch, który jest dalej od celu — inaczej pasek dobiłby
                    // do setki, a cel dalej by nie zaliczał.
                    Some(r) => po_liczbie.min(if miejsce(session, h) <= u32::from(*r) {
                        10_000
                    } else {
                        po_liczbie / 2
                    }),
                }
            }
            // Postęp celu z licznikiem dób liczy **`ScenarioState`**, bo tylko on zna
            // serię; sam cel odpowiada wtedy „albo trzyma, albo nie" i to jest
            // poprawna odpowiedź na pytanie zadane bez historii.
            Goal::SiteSolvent { ordinal, .. } => {
                if wyplacalny(session, h, *ordinal) {
                    10_000
                } else {
                    0
                }
            }
            Goal::NetWorth { min } => ulamek(majatek(session, h), *min),
        }
    }

    /// Czy cel jest **w tej chwili** spełniony. Dla celu z licznikiem dób odpowiada
    /// wyłącznie za jedną dobę — ciąg liczy [`ScenarioState`].
    #[must_use]
    pub fn holds(&self, session: &Session, h: &Holdings) -> bool {
        match self {
            Goal::SiteSolvent { ordinal, .. } => wyplacalny(session, h, *ordinal),
            _ => self.progress_bp(session, h) >= 10_000,
        }
    }

    /// Ile dób z rzędu warunek musi się trzymać. Zero = liczy się chwila.
    #[must_use]
    pub const fn streak_days(&self) -> u32 {
        match self {
            Goal::SiteSolvent { for_days, .. } => *for_days,
            _ => 0,
        }
    }
}

/// Jeden cel scenariusza.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
pub struct Objective {
    pub id: ObjectiveId,
    /// Klucz tytułu w `data/locale/`.
    pub title: String,
    pub goal: Goal,
    #[serde(default)]
    pub optional: bool,
    /// Termin w dobach gry. `None` = bez terminu.
    #[serde(default)]
    pub deadline_days: Option<u32>,
    /// Cel ujawnia się dopiero po domknięciu tamtego.
    #[serde(default)]
    pub reveal_after: Option<ObjectiveId>,
}

/// Deterministyczna zmiana świata, stosowana w ticku 0.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub enum WorldPatch {
    /// Zamyka w `ordinal`-tej dzielnicy sklepy **jednego rodzaju** — tak powstaje
    /// nisza samouczka („dzielnica bez sklepu spożywczego", §5.12 pkt 2).
    ///
    /// Trzy rzeczy w tej jednej łatce są decyzjami, nie szczegółami.
    ///
    /// **Rodzaj, a nie cała dzielnica.** Nisza to brakujący rodzaj sklepu, a nie
    /// martwa dzielnica. Wersja zamykająca wszystko wygasiła w mieście testowym
    /// osiemdziesiąt trzy sklepy z osiemdziesięciu trzech — gracz nie miał wtedy
    /// gdzie otworzyć własnego, bo firma wchodzi do **istniejącego** lokalu (`DI-5`).
    ///
    /// **Dzielnica porządkowo, a nie numerem.** Scenariusz kładzie się na dowolnym
    /// wygenerowanym mieście, a numery dzielnic zależą od ziarna.
    ///
    /// **Rodzaj najrzadszy.** Deterministycznie (remis po `PlaceKind::as_index()`)
    /// i najmniej inwazyjnie: dziura jest prawdziwa, a miasto zostaje miastem.
    ClearShopKind { district_ordinal: u32 },
    /// Wyprowadza gotówkę z rachunku `ordinal`-tego zakładu miasta — tak powstaje
    /// „upadająca huta". Kolejność zakładów jest kolejnością kluczy, więc łatka
    /// trafia w ten sam zakład przy każdym przebiegu tego samego ziarna.
    IndebtSite { ordinal: u32, amount: i64 },
}

/// Scenariusz: świat, cele i łatki.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
pub struct Scenario {
    /// Klucz tekstowy — **on**, a nie numer, jest kontraktem zapisu gry.
    pub key: String,
    pub title: String,
    pub brief: String,
    #[serde(default)]
    pub patches: Vec<WorldPatch>,
    #[serde(default)]
    pub objectives: Vec<Objective>,
    /// Limit czasu w dobach gry.
    #[serde(default)]
    pub time_limit_days: Option<u32>,
    /// Czy to scenariusz samouczka — wtedy powłoka prowadzi gracza krok po kroku.
    #[serde(default)]
    pub tutorial: bool,
}

/// Katalog scenariuszy z `data/scenarios/scenarios.ron`.
///
/// Jeden plik, a nie `*.ron` po jednym na scenariusz: katalog `data/scenarios/`
/// należy od M6 do zapasu startowego (`initial_stock.ron`), a ładowarka po masce
/// próbowałaby go czytać jako scenariusz.
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
pub struct ScenarioCatalog {
    pub schema_version: u32,
    pub scenarios: Vec<Scenario>,
}

/// Co poszło nie tak przy ładowaniu katalogu.
#[derive(Debug)]
pub enum ScenarioError {
    Io(std::io::Error),
    Ron(String),
    Schema {
        found: u32,
    },
    /// Dwa scenariusze o tym samym kluczu — klucz jest kontraktem zapisu.
    DuplicateKey(String),
}

impl std::fmt::Display for ScenarioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScenarioError::Io(e) => write!(f, "nie da się wczytać scenariuszy: {e}"),
            ScenarioError::Ron(e) => write!(f, "błąd składni scenariuszy: {e}"),
            ScenarioError::Schema { found } => write!(
                f,
                "scenariusze w wersji {found}, a gra czyta {SCENARIOS_SCHEMA_VERSION}"
            ),
            ScenarioError::DuplicateKey(k) => write!(f, "dwa scenariusze o kluczu {k}"),
        }
    }
}

impl std::error::Error for ScenarioError {}

impl ScenarioCatalog {
    /// Wczytuje katalog.
    ///
    /// # Errors
    /// [`ScenarioError`] — brak pliku, błąd składni, zła wersja schematu albo
    /// powtórzony klucz.
    pub fn load() -> Result<ScenarioCatalog, ScenarioError> {
        let p = magnat_core::data_path("scenarios").join("scenarios.ron");
        let tekst = std::fs::read_to_string(p).map_err(ScenarioError::Io)?;
        let k: ScenarioCatalog =
            ron::from_str(&tekst).map_err(|e| ScenarioError::Ron(e.to_string()))?;
        if k.schema_version != SCENARIOS_SCHEMA_VERSION {
            return Err(ScenarioError::Schema {
                found: k.schema_version,
            });
        }
        let mut widziane = Vec::new();
        for s in &k.scenarios {
            if widziane.contains(&s.key) {
                return Err(ScenarioError::DuplicateKey(s.key.clone()));
            }
            widziane.push(s.key.clone());
        }
        Ok(k)
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Scenario> {
        self.scenarios.iter().find(|s| s.key == key)
    }

    /// Scenariusz po numerze z koperty `StartGame`. Numer jest **pozycją w pliku**,
    /// a klucz kontraktem — dlatego zapis niesie numer, a plik może rosnąć na końcu.
    #[must_use]
    pub fn by_id(&self, id: crate::ScenarioId) -> Option<&Scenario> {
        self.scenarios.get(id.0 as usize)
    }
}

/// Postęp celów w trwającej grze.
#[derive(Default)]
pub struct ScenarioState {
    /// Ile dób z rzędu cel się trzyma.
    pub(crate) streak: BTreeMap<u8, u32>,
    /// Cele już domknięte — raz osiągnięty zostaje osiągnięty.
    done: Vec<ObjectiveId>,
    /// Cele przegrane po terminie.
    failed: Vec<ObjectiveId>,
    last_day: Option<u64>,
}

/// Jak stoi scenariusz.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScenarioOutcome {
    /// Gra trwa.
    Running,
    /// Wszystkie obowiązkowe cele domknięte.
    Won,
    /// Minął limit czasu albo termin celu obowiązkowego.
    Lost,
}

impl ScenarioState {
    /// Przelicza postęp, jeśli minęła doba. Zwraca cele domknięte **w tej dobie**.
    pub fn step(&mut self, session: &Session, sc: &Scenario) -> Vec<ObjectiveId> {
        let doba = session.tick().get() / magnat_core::time::MINUTES_PER_DAY;
        if self.last_day == Some(doba) {
            return Vec::new();
        }
        self.last_day = Some(doba);
        let h = Holdings::of(session);
        let mut swieze = Vec::new();
        for o in &sc.objectives {
            if self.done.contains(&o.id) || self.failed.contains(&o.id) {
                continue;
            }
            if o.reveal_after.is_some_and(|r| !self.done.contains(&r)) {
                continue;
            }
            if o.deadline_days.is_some_and(|d| doba > u64::from(d)) {
                self.failed.push(o.id);
                continue;
            }
            let trzyma = o.goal.holds(session, &h);
            let licznik = self.streak.entry(o.id.0).or_insert(0);
            *licznik = if trzyma { *licznik + 1 } else { 0 };
            if trzyma && *licznik >= o.goal.streak_days().max(1) {
                self.done.push(o.id);
                swieze.push(o.id);
            }
        }
        swieze
    }

    #[must_use]
    pub fn is_done(&self, id: ObjectiveId) -> bool {
        self.done.contains(&id)
    }

    #[must_use]
    pub fn is_failed(&self, id: ObjectiveId) -> bool {
        self.failed.contains(&id)
    }

    #[must_use]
    pub fn done_count(&self) -> usize {
        self.done.len()
    }

    /// Jak stoi cały scenariusz.
    #[must_use]
    pub fn outcome(&self, session: &Session, sc: &Scenario) -> ScenarioOutcome {
        let doba = session.tick().get() / magnat_core::time::MINUTES_PER_DAY;
        if sc.time_limit_days.is_some_and(|d| doba > u64::from(d)) {
            return ScenarioOutcome::Lost;
        }
        let obowiazkowe: Vec<&Objective> = sc.objectives.iter().filter(|o| !o.optional).collect();
        if obowiazkowe.is_empty() {
            return ScenarioOutcome::Running;
        }
        if obowiazkowe.iter().any(|o| self.failed.contains(&o.id)) {
            return ScenarioOutcome::Lost;
        }
        if obowiazkowe.iter().all(|o| self.done.contains(&o.id)) {
            return ScenarioOutcome::Won;
        }
        ScenarioOutcome::Running
    }
}

/// Stosuje łatki scenariusza. Wołane **raz**, przed pierwszym tickiem.
pub fn apply_patches(session: &mut Session, patches: &[WorldPatch]) {
    let Some(m) = session.market.clone() else {
        return;
    };
    for p in patches {
        match *p {
            WorldPatch::ClearShopKind { district_ordinal } => {
                // Dzielnicę podaje **rynek**, a nie rejestr firm: sklepy stawia
                // generator miasta i w rejestrze firm w chwili łatki jeszcze ich
                // nie ma. Pytanie „gdzie stoi ten sklep" ma jedną odpowiedź i jest
                // nią `ShopSeed`.
                let nasiona = m.shop_seeds();
                let mut dzielnice: Vec<u16> = nasiona.iter().map(|x| x.district).collect();
                dzielnice.sort_unstable();
                dzielnice.dedup();
                let Some(d) = dzielnice.get(district_ordinal as usize).copied() else {
                    continue;
                };
                let w_dzielnicy: Vec<&magnat_economy::ShopSeed> =
                    nasiona.iter().filter(|x| x.district == d).collect();
                // Ile sklepów każdego rodzaju — po `as_index()`, żeby remis rozstrzygał
                // się kolejnością słownika, a nie kolejnością wektora.
                let mut ile: BTreeMap<usize, usize> = BTreeMap::new();
                for x in &w_dzielnicy {
                    *ile.entry(x.kind.as_index()).or_insert(0) += 1;
                }
                // Jeden rodzaj w dzielnicy: zamknięcie go zostawiłoby ją bez sklepów
                // i bez lokalu, do którego gracz mógłby wejść. Wtedy niszy nie ma
                // i **to jest poprawny wynik** — łatka opisuje świat, a nie życzenie.
                if ile.len() < 2 {
                    continue;
                }
                let Some((rodzaj, _)) = ile.iter().min_by_key(|(k, n)| (**n, **k)) else {
                    continue;
                };
                for seed in w_dzielnicy.iter().filter(|x| x.kind.as_index() == *rodzaj) {
                    m.close_shop(seed.site);
                }
            }
            WorldPatch::IndebtSite { ordinal, amount } => {
                let sites = m.sites();
                let Some(site) = sites.get(ordinal as usize).copied() else {
                    continue;
                };
                let (Some(konto), Some(books)) = (
                    m.shop_account(site),
                    session
                        .app
                        .world
                        .get_resource_mut::<magnat_economy::Books>(),
                ) else {
                    continue;
                };
                let saldo = books.balance(konto).unwrap_or(Money::ZERO).get();
                let kwota = Money(amount.min(saldo).max(0));
                if kwota.get() > 0 {
                    let rest = m.rest_of_world();
                    let memo = magnat_economy::TxMemo::new(
                        magnat_economy::TxKind::Endowment,
                        magnat_core::DecisionReason::Unspecified,
                    );
                    let _ = books.transfer(konto, rest, kwota, memo, magnat_core::Tick(0));
                }
            }
        }
    }
}

/// Majątek gracza: gotówka gospodarstwa plus kapitał własny zakładów.
fn majatek(session: &Session, h: &Holdings) -> i64 {
    let gotowka = crate::metrics::gotowka_gracza(session).get();
    let kapital = session.market.as_ref().map_or(0, |m| {
        h.sites
            .iter()
            .map(|s| m.equity_of(*s).get())
            .fold(0i64, i64::saturating_add)
    });
    gotowka.saturating_add(kapital)
}

/// Miejsce gracza wśród pracodawców miasta; `u32::MAX`, gdy nie zatrudnia nikogo.
fn miejsce(session: &Session, h: &Holdings) -> u32 {
    if h.employees == 0 {
        return u32::MAX;
    }
    let Some(firms) = session.app.world.get_resource::<magnat_firms::Firms>() else {
        return u32::MAX;
    };
    let wiekszych = firms
        .iter()
        .filter(|(_, f)| {
            !f.owners
                .iter()
                .any(|o| o.owner == magnat_firms::Owner::Player)
        })
        .filter(|(_, f)| zatrudnienie(firms, f) > h.employees)
        .count();
    u32::try_from(wiekszych + 1).unwrap_or(u32::MAX)
}

fn zatrudnienie(firms: &magnat_firms::Firms, f: &magnat_firms::Firm) -> u32 {
    f.sites
        .iter()
        .filter_map(|s| firms.site(*s))
        .map(|z| {
            z.positions
                .iter()
                .map(|p| u32::try_from(p.filled.len()).unwrap_or(0))
                .sum::<u32>()
        })
        .sum()
}

/// Czy `ordinal`-ty zakład miasta ma dodatni kapitał własny.
fn wyplacalny(session: &Session, _h: &Holdings, ordinal: u32) -> bool {
    let Some(m) = session.market.as_ref() else {
        return false;
    };
    m.sites()
        .get(ordinal as usize)
        .is_some_and(|s| m.equity_of(*s).get() > 0)
}

/// Postęp celu z licznikiem dób, w punktach bazowych ciągu.
///
/// Osobno od [`Goal::progress_bp`], bo tylko stan scenariusza zna serię: cel
/// „wypłacalny przez rok" po stu dobach jest w 27 %, a nie w zerze albo w setce.
#[must_use]
pub fn streak_progress_bp(state: &ScenarioState, o: &Objective) -> u16 {
    let cel = o.goal.streak_days();
    if cel == 0 {
        return 0;
    }
    let ile = state.streak.get(&o.id.0).copied().unwrap_or(0);
    u16::try_from((u64::from(ile) * 10_000 / u64::from(cel)).min(10_000)).unwrap_or(10_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn katalog_wczytuje_sie_i_ma_szesc_pozycji() {
        let k = ScenarioCatalog::load().expect("katalog scenariuszy");
        assert_eq!(
            k.scenarios.len(),
            6,
            "pięć scenariuszy z §13.3 plus tryb otwarty"
        );
        assert!(k.get("sandbox").is_some(), "tryb otwarty musi być pierwszy");
        assert_eq!(
            k.by_id(crate::ScenarioId::SANDBOX).map(|s| s.key.as_str()),
            Some("sandbox"),
            "numer z koperty StartGame wskazuje pozycję w pliku"
        );
    }

    #[test]
    fn samouczek_ma_late_i_cel_w_jednym_kroku() {
        let k = ScenarioCatalog::load().expect("katalog scenariuszy");
        let s = k.get("first_shop").expect("samouczek");
        assert!(s.tutorial, "samouczek ma być oznaczony");
        assert!(!s.patches.is_empty(), "nisza powstaje łatką, nie tekstem");
        // Pierwszy cel musi być osiągalny jedną komendą — to jest cała treść
        // budżetu dwunastu interakcji (§5.12 pkt 3).
        assert!(matches!(s.objectives[0].goal, Goal::SiteCount { min: 1 }));
    }

    #[test]
    fn kazdy_cel_ma_klucz_tekstu_a_nie_napis() {
        let k = ScenarioCatalog::load().expect("katalog scenariuszy");
        for s in &k.scenarios {
            assert!(
                s.title.starts_with("ui."),
                "tytuł {} nie jest kluczem",
                s.key
            );
            assert!(
                s.brief.starts_with("ui."),
                "opis {} nie jest kluczem",
                s.key
            );
            for o in &s.objectives {
                assert!(
                    o.title.starts_with("ui."),
                    "cel w {} nie ma klucza tekstu",
                    s.key
                );
            }
        }
    }
}
