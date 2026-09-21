//! Krok makro — jedna doba (M7f WP13; kontrakt M10a §5.7).
//!
//! Osiem faz opisuje kontrakt M10 i **od M10a są komplet**: M7f zbudował fazy 2–6
//! (rynek pracy, produkcja, rynek dóbr, ceny, finanse), M10a dokłada 1, 7 i 8
//! (demografia, relacje dostawców, zdarzenia). Trzy nowe są dla horyzontu
//! kwartalnego prawie niewidoczne — demografia rusza raz na rok gry, a wstrząs
//! wypada raz na kilka lat — i dokładnie dlatego M7 mógł ich nie mieć.
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

pub mod demography;
mod events;
mod finance;
mod goods;
mod labor;
mod price;
mod produce;
mod relations;

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
    /// Ciśnienie na odejście dobrowolne, w dziesięciotysięcznych obsady na dobę —
    /// `hr.quit_base_per_10k` z `data/tuning/labor.ron`.
    ///
    /// **Ta sama liczba, którą rotacja liczy w mezo** (`K-50`): bez niej makro nie
    /// ma ani jednego źródła bezrobocia poza redukcją etatów, więc firmy zatrudniają
    /// wszystkich i bezrobocie schodzi do zera. Dokładnie to zmierzyła bramka 4
    /// Etapu 10 po `GE-12` — 0 ‰ wobec pasma 30–150 (`GF-7`).
    pub quit_per_10k_day: u16,
    /// Nominalna przepustowość jednego etatu na godzinę, w jednostkach natywnych
    /// towaru. Mnożnik skali, nie twierdzenie fizyczne — patrz [`produce`].
    pub nominal_per_slot_hour: i64,
    /// Płaca odniesienia, gdy firma nie ma jeszcze ani jednego pracownika.
    pub default_wage_month: magnat_core::Money,
    /// Agresja licytacyjna z osobowości dyrektora, 0..=100. Makro uśrednia,
    /// bo `MacroFirm` nie niesie osobowości — to jest ta sama zaślepka, którą
    /// M7b zostawił w `AGGRESSION` (`AV-6`).
    pub aggression: u8,

    // ── M10a: fazy 1, 7 i 8 ──────────────────────────────────────────────────
    /// Ziarno świata — wejście `rng()` w fazie 8. Fazy 1–7 nie losują.
    pub seed: u64,
    /// Ile promili najstarszej kohorty odchodzi w ciągu roku gry. Populacja jest
    /// zamknięta (`step::demography`), więc ta sama liczba wraca jako urodzenia.
    pub elder_mortality_permille: u16,
    /// Ile promili mieszkańców komórki przeprowadza się w ciągu roku, gdy różnica
    /// warunków przekracza [`MacroParams::migration_gap`].
    pub migration_permille: u16,
    /// Próg, od którego różnica atrakcyjności dwóch komórek uruchamia przeprowadzki.
    /// W jednostkach porządkowych `atrakcyjnosc()` — nie w pieniądzu.
    pub migration_gap: u16,
    /// O ile tańszy musi być nowy dostawca, żeby zerwać istniejącą współpracę (bp).
    pub supplier_switch_bp: i32,
    /// Szansa na wstrząs w skali miasta w ciągu doby, w promilach.
    pub shock_hazard_permille: u16,
    /// Siła wstrząsu w punktach bazowych popytu albo przerobu.
    pub shock_magnitude_bp: i32,
    /// Najkrótszy wstrząs, w dobach.
    pub shock_min_days: u16,
    /// Rozpiętość losowania długości wstrząsu ponad minimum, w dobach.
    pub shock_span_days: u16,
    /// Co ile dób firmy przeceniają — środek przedziału czujności 1–7 dób
    /// z `data/economy/shop.ron`. Makro nie ma osobowości firm, więc ma jedną
    /// kadencję dla wszystkich (`step::price`).
    pub reprice_every_days: u8,
    /// Ile ofert na towar ocenia komórka — `k_max` z `data/economy/choice.ron`.
    /// Mieszkaniec w mezo też nie ocenia wszystkich (`step::goods`).
    pub candidates_per_good: u8,
    /// Ile dób **reprezentuje** jeden krok (§5.7: krok zmienny w historii „na sucho").
    ///
    /// Nie jest to przyspieszenie przez pomijanie: każda faza mnoży przez tę liczbę
    /// swój przepływ — lista płac, odsetki, przerób i budżet zakupowy. Osiemdziesiąt
    /// lat liczone dobą po dobie to 28 800 kroków, czyli trzy minuty na ekranie
    /// ładowania; krok sześciodobowy w latach wczesnych i jednodobowy w ostatnich
    /// pięciu daje 6 300 kroków i te same wielkości końcowe.
    ///
    /// Sufit jest nazwany: przy kroku wielodobowym zapas może zejść do zera **w środku**
    /// kroku i nikt tego nie zobaczy — dlatego ostatnie lata przed startem partii
    /// liczy się dobą po dobie, bo to one ustawiają stan początkowy.
    pub days_per_step: u8,
    /// Sufit dźwigni, powyżej którego firma nie dostaje już kredytu obrotowego,
    /// w promilach aktywów. Bez sufitu firma trwale nierentowna pożycza codziennie
    /// i dług rośnie bez końca — makro nie ma postępowania upadłościowego (`K-10`
    /// przyznaje je M7), więc **ogranicznik jest tu jedyną drogą**, żeby zamiast
    /// bankructwa dostać firmę, która przestaje płacić.
    pub max_leverage_permille: i64,
}

/// Dobowy sufit zmiany płacy, w punktach bazowych (M10a §5.7: „zmiana ograniczona
/// do ±2 %/dzień"). Ogranicznik jest po stronie makra, nie jądra, bo jądro liczy
/// **krok licytacji**, a nie tempo, w jakim rynek go wchłania.
/// macro-guard: tempo wchłaniania podwyżki, nie jej wysokość
pub const WAGE_STEP_CAP_BP: i32 = 200;

impl Default for MacroParams {
    // macro-guard: kalibracja kroku — wartości zapasowe dla świata bez `data/`
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
            quit_per_10k_day: LaborTuning::load_default().map_or(2, |t| t.hr.quit_base_per_10k),
            nominal_per_slot_hour: 12,
            default_wage_month: magnat_core::Money(400_000),
            aggression: 50,
            seed: 0,
            // Umieralność najstarszej kohorty: 85+ to średnio ośmioletnie dalsze
            // trwanie życia, czyli ~120 ‰ rocznie. Liczba trafia tu, a nie
            // do `data/demography/`, bo tamta tabela opisuje hazard **osoby**
            // w danym wieku, a to jest hazard **kohorty otwartej** — dwie różne
            // wielkości, których nie wolno mylić jedną nazwą.
            elder_mortality_permille: 120,
            migration_permille: 20,
            migration_gap: 150,
            supplier_switch_bp: 500,
            shock_hazard_permille: 2,
            shock_magnitude_bp: 1_500,
            shock_min_days: 120,
            shock_span_days: 360,
            reprice_every_days: 4,
            candidates_per_good: 8,
            days_per_step: 1,
            max_leverage_permille: 800,
        }
    }
}

/// Jedna doba modelu: osiem faz w kolejności z kontraktu.
///
/// Kolejność jest kontraktem, nie wygodą: ceny ustawia się **po** rynku dóbr, bo
/// sterownik ceny reaguje na zapas, który został po sprzedaży — tak samo jak
/// w mezo, gdzie `reprice_all` biegnie po dobie zakupów, a nie przed nią.
///
/// Demografia stoi **pierwsza** i to też jest kontrakt: przeprowadzka zmienia
/// liczebność komórki, a rynek pracy i popyt liczą się z niej. Odwrotna kolejność
/// znaczyłaby, że w dniu przeprowadzki ludzie kupują w komórce, której już nie ma.
/// Zdarzenia stoją **ostatnie**, bo wstrząs działa od doby następnej — zdarzenie
/// zmieniające popyt w tej samej dobie, w której wypadło, byłoby nie do odróżnienia
/// od zwykłego wahania.
pub fn step(state: &mut MacroState, params: &MacroParams) {
    demography::phase(state, params);
    labor::phase(state, params);
    produce::phase(state, params);
    goods::phase(state, params);
    price::phase(state, params);
    finance::phase(state, params);
    relations::phase(state, params);
    events::phase(state, params);
    let dni = u32::from(params.days_per_step.max(1));
    state.day = state.day.saturating_add(dni);
    state.tick = Tick(state.tick.get() + u64::from(dni) * magnat_core::time::MINUTES_PER_DAY);
}

/// Czy krok o długości `dni` rozpoczęty w dobie `day` przekracza granicę okresu.
///
/// Zastępuje `day % okres == 0` wszędzie tam, gdzie faza chodzi rzadziej niż co dobę.
/// Przy kroku wielodobowym reszta z dzielenia **mija granicę bez trafienia w nią**:
/// krok sześciodobowy z doby 28 ląduje w 34 i nigdy nie zobaczy trzydziestki, więc
/// przegląd dostawców nie odbyłby się ani razu przez osiemdziesiąt lat — a faza
/// wyglądałaby na działającą, bo test z krokiem jednodobowym przechodzi.
#[must_use]
pub fn przekroczono(day: u32, dni: u32, okres: u32) -> bool {
    if okres == 0 {
        return false;
    }
    day / okres != (day + dni.max(1)) / okres
}
