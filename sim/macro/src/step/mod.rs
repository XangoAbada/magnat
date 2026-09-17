//! Krok makro — jedna doba (M7f WP13; kontrakt M10a §5.7).
//!
//! Osiem faz opisuje kontrakt M10; **M7 implementuje 2–6**: rynek pracy, produkcję,
//! rynek dóbr, ceny i finanse. Fazy 1, 7 i 8 (demografia, relacje, zdarzenia) należą
//! do M10 i dla horyzontu kwartalnego są nieistotne — kwartał to 90 dób, a demografia
//! rusza agregatem w skali lat.
//!
//! # Czego tu nie ma i dlaczego to jest ważne
//!
//! Nie ma **ani jednej reguły ekonomicznej**. Cena wychodzi z `kernel::next_price`,
//! stawka z `kernel::wage_bid`, przerób z `kernel::throughput`, ocena oferty
//! z `kernel::purchase_score`, a podział popytu z `kernel::softmax_shares`. Tutaj
//! zostaje agregacja, alokacja i pętla czasu (M10a §5.1). Gdyby kiedyś pojawił się
//! tu mnożnik cenowy albo stawka wpisana z palca, byłby to drugi model gospodarki
//! obok mezo — czyli dokładnie to, przed czym broni `K-5`.
//!
//! # Jeden krok jest jednowątkowy
//!
//! 240 komórek × kilkanaście towarów + kilka tysięcy firm to rząd 10⁵ operacji
//! na krok. Zrównoleglenie byłoby tu wyłącznie źródłem niedeterminizmu bez zysku
//! (M10a §5.7).

use magnat_core::Tick;
use magnat_economy::data::UtilityWeights;
use magnat_firms::hr::tuning::{LaborTuning, WageTuning};

use crate::state::MacroState;

mod finance;
mod goods;
mod labor;
mod price;
mod produce;

/// Parametry kroku makro. Wszystko, co tu stoi, pochodzi z tych samych plików
/// `data/`, które stroi balansator — model makro nie ma własnej kalibracji i mieć
/// jej nie może, bo wtedy dwa poziomy odpowiadałyby na inne pytania.
#[derive(Clone, Debug)]
pub struct MacroParams {
    /// Parametry licytacji o pracownika — `data/tuning/labor.ron`.
    pub wage: WageTuning,
    /// Wagi funkcji użyteczności zakupu — `data/economy/weights.ron`, znormalizowane.
    pub weights: UtilityWeights,
    /// Temperatura softmaxu — `data/economy/choice.ron`.
    pub temperature: f64,
    /// Ile gotówki komórka wydaje na dobę, w punktach bazowych.
    pub spend_rate_bp: i32,
    /// Widełki marży netto (min, max) w bp — `data/economy/shop.ron`.
    pub margin_bp: (i32, i32),
    /// Marża docelowa w bp — środek widełek, o ile firma nie ma własnej polityki.
    pub target_margin_bp: i32,
    /// Czułość ceny na zapas i na konkurencję (`PriceInput::k_stock`, `k_comp`).
    pub k_stock: i32,
    pub k_comp: i32,
    /// Docelowe pokrycie zapasu w dobach.
    pub target_cover_days: i32,
    /// Miesięczna stopa długu w bp.
    pub debt_rate_bp_month: i32,
    /// Ile wakatów firma obsadza na dobę, w promilach zamówionych etatów.
    /// Tarcie wyszukiwania w skali agregatu — bez niego zatrudnienie skakałoby
    /// do pełna w pierwszej dobie i rynek pracy przestałby cokolwiek znaczyć.
    pub hire_speed_permille: u16,
    /// Nominalna przepustowość jednego etatu na godzinę, w jednostkach natywnych
    /// towaru. Mnożnik skali, nie twierdzenie fizyczne — patrz [`produce`].
    pub nominal_per_slot_hour: i64,
    /// Płaca odniesienia, gdy firma nie ma jeszcze ani jednego pracownika.
    pub default_wage_month: magnat_core::Money,
    /// Agresja licytacyjna z osobowości dyrektora, 0..=100. Makro uśrednia,
    /// bo `MacroFirm` nie niesie osobowości — to jest ta sama zaślepka, którą
    /// M7b zostawił w `AGGRESSION` (`AV-6`).
    pub aggression: u8,
}

/// Dobowy sufit zmiany płacy, w punktach bazowych (M10a §5.7: „zmiana ograniczona
/// do ±2 %/dzień"). Ogranicznik jest po stronie makra, nie jądra, bo jądro liczy
/// **krok licytacji**, a nie tempo, w jakim rynek go wchłania.
pub const WAGE_STEP_CAP_BP: i64 = 200;

impl Default for MacroParams {
    fn default() -> MacroParams {
        MacroParams {
            wage: LaborTuning::load_default().map_or(
                WageTuning {
                    reservation_start_bp: 10_000,
                    reservation_newcomer_bp: 9_000,
                    reservation_floor_bp: 7_000,
                    reservation_decay_bp_per_day: 20,
                    base_step_bp: 150,
                    headhunt_shortage: 500,
                    headhunt_premium_bp: 1_200,
                    switch_threshold_bp: 1_000,
                },
                |t| t.wage,
            ),
            weights: UtilityWeights {
                price: 0.45,
                quality: 0.2,
                brand: 0.0,
                dist: 0.2,
                loyalty: 0.05,
                status: 0.05,
                novelty: 0.05,
            }
            .normalized(),
            temperature: 0.35,
            spend_rate_bp: 300,
            margin_bp: (500, 4_000),
            target_margin_bp: 1_800,
            k_stock: 2_000,
            k_comp: 3_000,
            target_cover_days: 7,
            debt_rate_bp_month: 80,
            hire_speed_permille: 120,
            nominal_per_slot_hour: 12,
            default_wage_month: magnat_core::Money(400_000),
            aggression: 50,
        }
    }
}

/// Jedna doba modelu: fazy 2–6 w kolejności z kontraktu.
///
/// Kolejność jest kontraktem, nie wygodą: ceny ustawia się **po** rynku dóbr, bo
/// sterownik ceny reaguje na zapas, który został po sprzedaży — tak samo jak
/// w mezo, gdzie `reprice_all` biegnie po dobie zakupów, a nie przed nią.
pub fn step(state: &mut MacroState, params: &MacroParams) {
    labor::phase(state, params);
    produce::phase(state, params);
    goods::phase(state, params);
    price::phase(state, params);
    finance::phase(state, params);
    state.day = state.day.saturating_add(1);
    state.tick = Tick(state.tick.get() + magnat_core::time::MINUTES_PER_DAY);
}
