//! Ekrany poza rozgrywką — rysowanie (`M9b` §5.14, WP14, PRD §14.7).
//!
//! # Gdzie to stoi i dlaczego tutaj
//!
//! Model ekranów powstał w M9a i siedzi w [`crate::shell`]; tutaj jest **wyłącznie
//! rysowanie**. Adres jest wymuszony grafem zależności (`DE-7`): `game` zależy od
//! `engine/ui`, nigdy odwrotnie, więc rysowanie `NewGameParams` w `engine/ui` wymagałoby
//! drugiego kompletu struktur widoku — a te rozjechałyby się przy pierwszym nowym polu
//! kreatora. `engine/ui` daje widgety (motyw, zakładki, formatowanie), `game` je składa.
//!
//! # Wszystko klawiaturą
//!
//! To jest kryterium WP14, nie ozdoba: od `magnat` bez argumentów do grającego świata
//! w ≤ 6 interakcjach, wyłącznie z klawiatury. Dlatego każdy ekran ma **jeden kursor**
//! ([`Focus`]) i jeden zestaw klawiszy: ↑↓ wybór, ←→ zmiana wartości, Enter zatwierdza,
//! Esc cofa. Myszy to nie odbiera — klik ustawia kursor i zatwierdza.
//!
//! # Bez migawki symulacji
//!
//! W menu głównym nie ma jeszcze świata, więc te ekrany jako jedyne w grze nie czytają
//! `&Snapshot` (§5.14). Granica z §5.2 nie jest naruszona, tylko nieużywana.

mod character;
mod controls;
pub mod ending;
mod menu;
mod newgame;
mod settings;
pub mod slots;

use magnat_ui::{Catalog, Locale, Theme};

use crate::save::{SaveError, SaveSlot};
use crate::shell::{NewGameParams, ScenarioId, Settings, ShellScreen};

/// Co powłoka każe zrobić pętli gry. Rysowanie niczego nie wykonuje samo — świat
/// stawia [`crate::session`], a wyjście z gry należy do pętli okna.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ShellAction {
    /// Zacznij generację świata z tymi parametrami.
    Generate(NewGameParams),
    /// Wczytaj slot — przewinięciem dziennika wejść (`DA-7`).
    LoadSlot(u8),
    /// Zapisz bieżącą sesję do slotu.
    SaveSlot(u8),
    /// Wróć do gry z menu pauzy.
    Resume,
    /// Porzuć sesję i wróć do menu głównego.
    ToMenu,
    /// Zamknij grę.
    Quit,
    /// Anuluj generację i wróć do kreatora.
    CancelGeneration,
    /// „Gram tutaj" na podglądzie świata.
    PlayHere,
    /// „Losuj inny świat" — nowe ziarno, te same pozostałe parametry.
    Reroll,
    /// „Zmień parametry" — powrót do kreatora z zachowanym szkicem.
    BackToWizard,
    /// Ustawienia się zmieniły: język albo skala — trzeba je nałożyć bez restartu.
    SettingsChanged,
    /// Gracz wybrał postać z listy kandydatów.
    PickCitizen(magnat_core::CitizenId),
    /// „Wylosuj postać" — wybór z ziarna świata, nie z zegara.
    PickRandomCitizen,
    /// Tryb przeglądu: wejście do świata **bez postaci**. Gracz ogląda i klika,
    /// ale nie jest niczyim mieszkańcem i nie ma czym wydawać komend.
    Observe,
}

/// Pozycja kursora na ekranie. Jedna liczba, bo każdy ekran powłoki jest listą wierszy.
pub type Focus = usize;

/// Stan powłoki między klatkami: co jest na ekranie, co gracz ustawił i co widać
/// na liście slotów.
pub struct Shell {
    pub screen: ShellScreen,
    pub settings: Settings,
    pub catalog: Catalog,
    pub theme: Theme,
    /// Kursor bieżącego ekranu. Zerowany przy każdej zmianie ekranu — wracając
    /// do menu, gracz ma stanąć na pierwszej pozycji, a nie tam, gdzie był kiedyś.
    pub focus: Focus,
    /// Nagłówki slotów; `None` = slot pusty, `Some(Err)` = zapis niezgodny wersją
    /// i **widoczny** (ukryty zapis czyta się jako utracona gra).
    pub slots: Vec<(u8, Option<Result<SaveSlot, SaveError>>)>,
    /// Czy sesja stoi w pamięci — od tego zależy „Kontynuuj" w menu i „Zapisz".
    pub has_session: bool,
    /// Ostatni szkic kreatora; przeżywa wejście w podgląd i powrót.
    pub draft: NewGameParams,
    /// Po co gracz wszedł na listę slotów. Tryb, a nie osobny ekran: pytanie
    /// „który slot" jest w obu przypadkach to samo.
    pub slot_mode: slots::Mode,
    /// Kandydaci na postać gracza — wypełnia je klient po postawieniu świata.
    /// Dane, nie referencja: ekran ma być rysowalny w teście bez sesji.
    pub candidates: Vec<crate::player::Candidate>,
    /// Scenariusze do wyboru w kreatorze: numer, klucz tytułu i klucz opisu.
    ///
    /// Numer jest **pozycją w `data/scenarios/scenarios.ron`** i to on jedzie
    /// w kopercie `StartGame` (`K-71`). Lista jest tu, a nie w ekranie, bo ekran
    /// ma się rysować w teście bez czytania dysku.
    pub scenarios: Vec<(ScenarioId, String, String)>,
}

impl Shell {
    /// # Errors
    /// Brak katalogu tekstów albo motywu w `data/` — jedno i drugie jest błędem
    /// instalacji, wykrywanym przy starcie.
    pub fn new(settings: Settings) -> Result<Shell, Box<dyn std::error::Error>> {
        Ok(Shell {
            screen: ShellScreen::MainMenu,
            settings,
            catalog: Catalog::load()?,
            theme: Theme::load()?,
            focus: 0,
            slots: Vec::new(),
            has_session: false,
            draft: NewGameParams::default(),
            slot_mode: slots::Mode::default(),
            candidates: Vec::new(),
            scenarios: scenariusze(),
        })
    }

    /// Jak [`Shell::new`], ale z katalogiem w pseudo-lokalizacji — do testu układu.
    ///
    /// # Errors
    /// Jak [`Shell::new`].
    pub fn new_pseudo(settings: Settings) -> Result<Shell, Box<dyn std::error::Error>> {
        let mut s = Shell::new(settings)?;
        s.catalog = Catalog::load_pseudo()?;
        Ok(s)
    }

    #[must_use]
    pub fn locale(&self) -> Locale {
        self.settings.locale
    }

    /// Skala interfejsu jako mnożnik — `egui` bierze ją przez `zoom_factor`, więc
    /// przyciąganie do pikseli fizycznych dzieje się w jednym miejscu.
    #[must_use]
    pub fn zoom(&self) -> f32 {
        f32::from(self.settings.ui_scale) / 1000.0
    }

    pub fn go(&mut self, screen: ShellScreen) {
        self.screen = screen;
        self.focus = 0;
    }

    /// Odświeża listę slotów z katalogu zapisów.
    pub fn refresh_slots(&mut self, dir: &std::path::Path) {
        self.slots = crate::save::list_slots(dir);
    }

    #[must_use]
    pub fn text(&self, key: &str) -> String {
        self.catalog.fmt_key(self.settings.locale, key, &[])
    }

    #[must_use]
    pub fn fmt(&self, key: &str, args: &[(&str, &str)]) -> String {
        self.catalog.fmt_key(self.settings.locale, key, args)
    }

    /// Nakłada motyw i skalę na kontekst. Woła się **raz na klatkę, przed** budową
    /// interfejsu: styl `egui` jest stanem kontekstu, a nie parametrem widgetu.
    pub fn prepare(&self, ctx: &egui::Context) {
        self.theme.apply(ctx);
        ctx.set_zoom_factor(self.zoom());
    }

    fn tlo(&self) -> egui::Frame {
        egui::Frame::NONE
            .fill(self.theme.color(magnat_ui::ColorToken::BgWindow))
            .inner_margin(self.theme.gap(6) as i8)
    }

    /// Rysuje bieżący ekran powłoki.
    pub fn draw(&mut self, ui: &mut egui::Ui) -> Option<ShellAction> {
        let tlo = self.tlo();
        egui::CentralPanel::default()
            .frame(tlo)
            .show(ui, |ui| match self.screen.clone() {
                ShellScreen::MainMenu => menu::main_menu(self, ui),
                ShellScreen::Pause => menu::pause_menu(self, ui),
                ShellScreen::NewGame { .. } => newgame::wizard(self, ui),
                ShellScreen::Load { .. } => slots::list(self, ui),
                ShellScreen::Settings { tab } => settings::screen(self, ui, tab),
                ShellScreen::CharacterSelect => character::screen(self, ui),
            })
            .inner
    }

    /// Ekran generacji: pasek postępu z prawdziwą nazwą etapu i anulowanie (§5.14 pkt 1).
    pub fn draw_generating(
        &mut self,
        ui: &mut egui::Ui,
        progress: &crate::shell::GenProgress,
    ) -> Option<ShellAction> {
        let tlo = self.tlo();
        egui::CentralPanel::default()
            .frame(tlo)
            .show(ui, |ui| newgame::generating(self, ui, progress))
            .inner
    }

    /// Podgląd świata: „Gram tutaj" / „Losuj inny" / „Zmień parametry".
    pub fn draw_preview(
        &mut self,
        ui: &mut egui::Ui,
        preview: &crate::shell::WorldPreview,
        mapa: Option<&egui::TextureHandle>,
    ) -> Option<ShellAction> {
        let tlo = self.tlo();
        egui::CentralPanel::default()
            .frame(tlo)
            .show(ui, |ui| newgame::preview(self, ui, preview, mapa))
            .inner
    }
}

/// Scenariusze z katalogu, a przy jego braku sam tryb otwarty.
///
/// Brak katalogu **nie jest błędem startu gry**: scenariusz jest warstwą nad
/// światem, a świat stoi i bez niego. To ta sama odpowiedź, którą daje
/// `Session::load_scenario` — dwie różne byłyby menu obiecującym coś, czego
/// sesja potem nie wczyta.
fn scenariusze() -> Vec<(ScenarioId, String, String)> {
    let Ok(k) = crate::scenario::ScenarioCatalog::load() else {
        return vec![(
            ScenarioId::SANDBOX,
            "ui.scenario.sandbox.title".to_string(),
            "ui.scenario.sandbox.brief".to_string(),
        )];
    };
    k.scenarios
        .iter()
        .enumerate()
        .map(|(i, sc)| {
            (
                ScenarioId(u16::try_from(i).unwrap_or(0)),
                sc.title.clone(),
                sc.brief.clone(),
            )
        })
        .collect()
}

pub(crate) use controls::{header, menu_list, segment_row, Keys};
