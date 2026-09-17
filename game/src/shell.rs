//! Powłoka sesji — nowa gra, generacja z postępem, podgląd świata, ustawienia
//! (M9a §5.13, PRD §14.7).
//!
//! Tu mieszka **logika** ekranów poza rozgrywką; ich postać graficzna to WP14
//! w `M9b`. Podział przebiega dokładnie tam, gdzie zwykle: wszystko poniżej da się
//! uruchomić i przetestować bez GPU, więc narzędzie bezgłowe zakłada nową grę tą
//! samą drogą co klient graficzny — a nie drugą, równoległą.
//!
//! **Po co to istnieje.** Do M8e jedyną drogą do świata było
//! `magnat --seed … --size … --region …`. To jest droga dla nas, nie dla gracza,
//! i nie jest to kwestia wygody: dopóki parametry żyją wyłącznie w `clap`, żaden
//! ekran nie ma czego pokazać, a zapis gry nie ma czego odtworzyć poza ziarnem.

use magnat_core::{Mass, ResourceKind};
use magnat_ui::Locale;
use magnat_world::{WorldGenParams, WorldGenReport};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;

use crate::world::{population, BuiltCity, SessionOpts};

/// Ekran poza rozgrywką. Pauza jest tutaj, a nie w [`crate::session::GameState`],
/// bo **sesja zostaje w pamięci**: powrót do gry nie wczytuje niczego. Sam zegar
/// stoi przez `SimSpeed::Paused` — jeden mechanizm, nie dwa.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ShellScreen {
    MainMenu,
    /// Kreator; `draft` przeżywa wejście w podgląd i powrót.
    NewGame {
        draft: NewGameParams,
    },
    Load {
        slots: Vec<u8>,
        selected: Option<u8>,
    },
    Settings {
        tab: SettingsTab,
    },
    Pause,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SettingsTab {
    Game,
    Graphics,
    Audio,
    Controls,
}

/// Scenariusz rozgrywki. Tryb otwarty to **też** scenariusz — inaczej „bez celu"
/// byłoby gałęzią w kodzie zamiast wpisem w danych (PRD §13.3).
///
/// Katalog `data/scenarios/*.ron` i cele projektuje WP12 (`M9e`); tu jest sam
/// identyfikator, bo niesie go koperta `StartGame` i musi być w niej od początku.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ScenarioId(pub u16);

impl ScenarioId {
    /// Tryb otwarty — bez końca i bez celów.
    pub const SANDBOX: ScenarioId = ScenarioId(0);
}

/// Wariant startu (PRD §13.1).
///
/// Wartość jedzie w kopercie `StartGame` od M9a, choć **skutek** (kapitał, praca,
/// dom, spadek) dokłada dopiero WP4 w `M9c`. Powód jest formatowy, nie ozdobny:
/// dopisanie pola do koperty po nagraniu pierwszych dzienników unieważniłoby je.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum StartVariant {
    /// Absolwent bez kapitału — pożyczka od rodziny.
    Graduate,
    /// Doświadczony pracownik z oszczędnościami.
    #[default]
    Worker,
    /// Spadkobierca małej firmy.
    Heir,
    /// Inwestor z zewnątrz: kapitał, brak sieci znajomości.
    Investor,
    /// Piaskownica — dowolny kapitał.
    Sandbox,
}

/// Wejście do założenia nowej gry. Serializowalne — narzędzie bezgłowe bierze je
/// z pliku RON, klient z kreatora, test z literału. Trzy drogi, jedna struktura.
///
/// `WorldGenParams` **nie jest kopiowany ani opakowywany** — to ten sam typ, który
/// generator dostaje z wiersza poleceń i który już jest w zapisie gry (M1 §5.5).
/// Kreator jest edytorem tej struktury, nic więcej; `params.validate()` jest tą
/// samą funkcją, która broni CLI.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct NewGameParams {
    pub world: WorldGenParams,
    pub scenario: ScenarioId,
    pub variant: StartVariant,
    /// Nastawy przebiegu (liczba mieszkańców, gospodarka, warstwa Mikro).
    /// W grze domyślne; scenariusz i test je zmieniają.
    #[serde(default)]
    pub opts: SessionOpts,
}

impl Default for NewGameParams {
    fn default() -> NewGameParams {
        NewGameParams {
            world: WorldGenParams::default(),
            scenario: ScenarioId::SANDBOX,
            variant: StartVariant::default(),
            opts: SessionOpts::default(),
        }
    }
}

/// Postęp generacji, czytany przez UI co klatkę, pisany przez wątek generacji.
#[derive(Debug)]
pub struct GenProgress {
    done: AtomicU8,
    total: u8,
    pass: AtomicU8,
}

impl GenProgress {
    #[must_use]
    pub fn new() -> GenProgress {
        GenProgress {
            done: AtomicU8::new(0),
            total: u8::try_from(population::kroki_generacji()).unwrap_or(u8::MAX),
            pass: AtomicU8::new(0),
        }
    }

    #[must_use]
    pub fn done(&self) -> u8 {
        self.done.load(Ordering::Relaxed)
    }

    #[must_use]
    pub const fn total(&self) -> u8 {
        self.total
    }

    /// Nazwa etapu, na którym stoi generacja. Pochodzi z `PASSES`, a nie z listy
    /// przepisanej tutaj — pasek ma mówić prawdę o tym, co się dzieje (`Z-4`).
    #[must_use]
    pub fn pass_name(&self) -> &'static str {
        let i = self.pass.load(Ordering::Relaxed) as usize;
        magnat_world::PASSES
            .get(i)
            .map_or("miasto", |p: &magnat_world::GenPass| p.name)
    }
}

impl Default for GenProgress {
    fn default() -> GenProgress {
        GenProgress::new()
    }
}

/// Para „postęp + anulowanie" przekazywana do generatora.
///
/// [`GenWatch::none`] jest wersją bez obu, dla wołających, którzy nie mają ekranu
/// ładowania (scenariusze, testy) — wtedy generacja nie da się anulować i to jest
/// jedyna różnica.
#[derive(Clone, Default)]
pub struct GenWatch {
    progress: Option<Arc<GenProgress>>,
    cancel: Option<Arc<AtomicBool>>,
}

impl GenWatch {
    #[must_use]
    pub fn none() -> GenWatch {
        GenWatch::default()
    }

    #[must_use]
    pub fn new(progress: Arc<GenProgress>, cancel: Arc<AtomicBool>) -> GenWatch {
        GenWatch {
            progress: Some(progress),
            cancel: Some(cancel),
        }
    }

    /// Zgłasza ukończony etap i odpowiada, czy **kontynuować**.
    pub fn pass_done(&self, i: usize, _name: &str) -> bool {
        if let Some(p) = &self.progress {
            let n = u8::try_from(i + 1).unwrap_or(u8::MAX);
            p.done.store(n.min(p.total), Ordering::Relaxed);
            p.pass
                .store(u8::try_from(i + 1).unwrap_or(u8::MAX), Ordering::Relaxed);
        }
        !self.cancelled()
    }

    #[must_use]
    pub fn cancelled(&self) -> bool {
        self.cancel
            .as_ref()
            .is_some_and(|c| c.load(Ordering::Relaxed))
    }
}

/// Generacja świata w wątku w tle, z postępem i anulowaniem.
///
/// `ponytail:` zwykły `std::thread`, a nie `engine/jobs` — plan zapowiadał tamto,
/// ale `JobPool` nie ma dziś uchwytu do zadania w tle (tylko `scope`, blokujące),
/// a generacja **sama bierze pulę** do zrównoleglenia passów. Zagnieżdżenie puli
/// w pulę kupiłoby zakleszczenie zamiast oszczędności. Ścieżka wyjścia, gdyby
/// kiedyś była potrzebna: `JobPool::spawn` zwracające uchwyt (M0 jest właścicielem).
pub struct WorldGenJob {
    handle: Option<std::thread::JoinHandle<Result<Option<BuiltCity>, String>>>,
    progress: Arc<GenProgress>,
    cancel: Arc<AtomicBool>,
}

impl WorldGenJob {
    /// Startuje etap A zakładania gry: teren (M1) i miasto (M2).
    #[must_use]
    pub fn start(params: WorldGenParams, threads: usize) -> WorldGenJob {
        let progress = Arc::new(GenProgress::new());
        let cancel = Arc::new(AtomicBool::new(false));
        let watch = GenWatch::new(progress.clone(), cancel.clone());
        let handle = std::thread::spawn(move || {
            let pool = magnat_jobs::JobPool::new(threads);
            population::zbuduj_z_params(params, &pool, &watch).map_err(|e| e.to_string())
        });
        WorldGenJob {
            handle: Some(handle),
            progress,
            cancel,
        }
    }

    #[must_use]
    pub fn progress(&self) -> &Arc<GenProgress> {
        &self.progress
    }

    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.handle.as_ref().is_some_and(|h| h.is_finished())
    }

    /// Prosi o przerwanie. Wątek kończy się po bieżącym passie.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Czeka na koniec. `Ok(None)` znaczy „anulowano" — nie „nie udało się".
    ///
    /// # Errors
    /// Błąd generacji (parametry, dane) albo panika wątku, opisana tekstem.
    pub fn join(mut self) -> Result<Option<BuiltCity>, String> {
        match self.handle.take() {
            Some(h) => h
                .join()
                .unwrap_or_else(|_| Err("wątek generacji świata zakończył się paniką".to_string())),
            None => Ok(None),
        }
    }
}

impl Drop for WorldGenJob {
    /// Anulowanie w połowie generacji nie ma prawa zostawić wątku za sobą —
    /// pięćdziesiąt anulowań w pętli ma dawać stałe zużycie pamięci (M9 §7).
    fn drop(&mut self) {
        if let Some(h) = self.handle.take() {
            self.cancel.store(true, Ordering::Relaxed);
            let _ = h.join();
        }
    }
}

/// Podgląd świata: „gram tutaj" albo „losuj ponownie" (M9a §5.13).
///
/// Pokazuje **pojemność** miasta z Etapów 6–7 (mieszkania, miejsca pracy, firmy),
/// a nie populację — ta powstaje dopiero w etapie B. Podawanie „118 400
/// mieszkańców", zanim ktokolwiek został wygenerowany, byłoby liczbą z sufitu.
pub struct WorldPreview {
    /// Miniatura RGBA [`PREVIEW_PX`] × [`PREVIEW_PX`].
    pub map: Vec<u8>,
    pub homes: u32,
    pub jobs: u32,
    pub firms: u32,
    pub city_area_km2: u32,
    pub districts: u16,
    pub deposits: Vec<(ResourceKind, Mass)>,
    /// Czasy etapów i hash terenu — do zgłoszeń błędów.
    pub report: WorldGenReport,
}

/// Bok miniatury podglądu w pikselach.
pub const PREVIEW_PX: usize = 512;

impl WorldPreview {
    /// Składa podgląd z postawionego terenu i miasta.
    #[must_use]
    pub fn of(built: &BuiltCity) -> WorldPreview {
        let r = &built.city.report;
        let mut deposits: Vec<(ResourceKind, Mass)> = Vec::new();
        for d in &built.terrain.data().deposits {
            match deposits.iter_mut().find(|(k, _)| *k == d.resource) {
                Some((_, m)) => *m = Mass(m.0.saturating_add(d.reserves.0)),
                None => deposits.push((d.resource, d.reserves)),
            }
        }
        deposits.sort_by_key(|(k, m)| (std::cmp::Reverse(m.0), *k as u8));
        WorldPreview {
            map: minimap(built),
            homes: r.build.dwellings,
            jobs: r.build.workplaces,
            firms: r.sites.firms,
            city_area_km2: r.urban_area_km2.round() as u32,
            districts: u16::try_from(r.districts).unwrap_or(u16::MAX),
            deposits,
            report: built.report.clone(),
        }
    }
}

/// Miniatura terenu: ląd cieniowany wysokością, woda na niebiesko.
///
/// `ponytail:` własne cieniowanie zamiast renderera pól z `headless preview` —
/// tamten rysuje dwanaście pól z wstawkami i siedzi w binarce narzędzia. Tu
/// chodzi o jeden obrazek na ekran ładowania; pełna nakładka danych to `M9c`.
fn minimap(built: &BuiltCity) -> Vec<u8> {
    let data = built.terrain.data();
    let n = data.height.dim();
    let mut px = vec![0u8; PREVIEW_PX * PREVIEW_PX * 4];
    let (mut lo, mut hi) = (i32::MAX, i32::MIN);
    for i in 0..n * n {
        let h = i32::from(data.height[i]);
        lo = lo.min(h);
        hi = hi.max(h);
    }
    let span = (hi - lo).max(1);
    for y in 0..PREVIEW_PX {
        for x in 0..PREVIEW_PX {
            let cell = (y * n / PREVIEW_PX) * n + (x * n / PREVIEW_PX);
            let o = (y * PREVIEW_PX + x) * 4;
            let (r, g, b) = if data.water_depth_dm(cell) > 0 {
                (30, 80, 150)
            } else {
                let t = ((i32::from(data.height[cell]) - lo) * 255 / span).clamp(0, 255) as u8;
                (60 + t / 2, 110 + t / 3, 60 + t / 4)
            };
            px[o] = r;
            px[o + 1] = g;
            px[o + 2] = b;
            px[o + 3] = 255;
        }
    }
    px
}

/// Profil gracza: język, skala UI, zapis dziennika widoku.
///
/// Siedzi **poza zapisem świata i poza hashem stanu** — zmiana języka w trakcie
/// gry nie ma prawa ruszyć symulacji ani o minutę.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub locale: Locale,
    /// Skala UI w promilach: 750 / 1000 / 1500 / 2000 / 3000.
    pub ui_scale: u16,
    /// Czy zapisywać strumień widoku (decyzja otwarta nr 7 fazy: domyślnie tak,
    /// wyłączalne; zawsze dołączany do zgłoszenia błędu).
    pub record_view: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            locale: Locale::Pl,
            ui_scale: 1000,
            record_view: true,
        }
    }
}

impl Settings {
    /// Wczytuje profil; brak pliku to **nie** błąd — pierwsze uruchomienie gry.
    ///
    /// # Errors
    /// Plik jest, ale nie daje się sparsować.
    pub fn load(path: &std::path::Path) -> Result<Settings, String> {
        match std::fs::read_to_string(path) {
            Ok(s) => ron::from_str(&s).map_err(|e| e.to_string()),
            Err(_) => Ok(Settings::default()),
        }
    }

    /// # Errors
    /// Błąd zapisu pliku.
    pub fn save(&self, path: &std::path::Path) -> Result<(), String> {
        let s = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(|e| e.to_string())?;
        std::fs::write(path, s).map_err(|e| e.to_string())
    }
}
