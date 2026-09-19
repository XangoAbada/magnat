//! Historia „na sucho" — etapy D0–D5 (M10a §5.7, WP10.3).
//!
//! PRD §4.2 Etap 9 obiecuje świat, który **zaczyna się zużyty**: ceny wyjściowe
//! z rynku, a nie z tabeli, zapasy po latach obrotu, zadłużenie konkretnych firm,
//! majątki rodzin rozwarstwione historią i sieć stałych dostawców. Ten moduł jest
//! całą drogą do tego stanu i składa się z pięciu etapów:
//!
//! | Etap | Co robi |
//! |---|---|
//! | **D0** | Zasiew: [`crate::lift`] ze stojącego świata. |
//! | **D1** | Bieg: `years` lat kroku makro, krok zmienny (§5.7). |
//! | **D3** | Kronika: zdarzenia o skali większej niż próg → [`ChronicleEvent`]. |
//! | **D4** | Rozwinięcie: [`crate::lower`] z powrotem na świat. |
//! | **D5** | Weryfikacja i naprawa: bramki Etapu 10 i do trzech rund `rebalance`. |
//!
//! # Czego tu nie ma i dlaczego
//!
//! **Etapu D2 (epoki) nie ma.** Plan wymieniał „etapy D0–D5", ale tabela w §5.7
//! opisywała cztery z sześciu — D1 i D2 nie miały wiersza. D1 jest tu jako bieg;
//! D2 to postęp technologiczny epoki, czyli PRD §11.3, czyli **WP10.9** i podfaza
//! M10c. Wpisanie go tutaj znaczyłoby drugi mechanizm epok obok tamtego.
//!
//! **Miasto nie rośnie w trakcie historii.** `MacroState` startuje z populacją
//! stojącego świata i tę populację ma na końcu — uzasadnienie w `step::demography`.
//! Zmienia się wszystko poza liczbą ludzi: struktura wieku, rozmieszczenie między
//! dzielnicami, majątek, ceny, zapasy, zadłużenie i sieć dostawców.
//!
//! # `SeedWorld` nie powstaje
//!
//! Kontrakt M10 §6 zapowiadał `dry_run(cfg, seed_world: &SeedWorld)`. Takiego typu
//! nie ma i nie będzie: świat **jest** zasiewem. `lower()` nanosi stan na istniejące
//! encje (patrz [`crate::lower`]), więc dry-run dostaje `&mut World` po Etapie 8
//! generatora i oddaje ten sam świat z osiemdziesięcioletnią historią w liczbach.
//! Osobny typ zasiewu byłby drugim opisem tego, co `World` już niesie.

use magnat_core::{DistrictId, GoodId, Money, Tick};
use magnat_economy::kernel::{self, PriceInput};
use magnat_ecs::World;

use crate::lower::{lower, lower_cell, LowerReport};
use crate::state::{MacroState, ShockKind};
use crate::step::{step, MacroParams};

/// Rok gry ma 360 dób (`K-1`).
const DOB_W_ROKU: u32 = 360;

/// Okno, w którym mierzy się nierównowagę rynku (§5.7, D5): średnia z ostatnich
/// trzydziestu dób, nie z jednej. Pojedyncza doba jest zaszumiona i dawałaby
/// fałszywe alarmy.
const OKNO_POMIARU: u32 = 30;

/// Jak długo przed startem partii krok schodzi do jednej doby.
const LAT_NA_KONCU: u16 = 5;

/// Nastawy historii „na sucho".
#[derive(Clone, Debug)]
pub struct DryRunConfig {
    pub seed: u64,
    /// Ile lat historii. 30–100 (§5.7).
    pub years: u16,
    /// Rok kalendarzowy, w którym historia się zaczyna — wyłącznie do kroniki.
    pub start_year: i32,
    /// Ile dób obejmuje krok w latach wczesnych.
    pub step_days_early: u8,
    /// Ile dób obejmuje krok w ostatnich [`LAT_NA_KONCU`] latach.
    pub step_days_late: u8,
    /// Ile rund naprawczych wolno zużyć na domknięcie Etapu 10.
    pub max_rebalance_rounds: u8,
    /// Czy zbierać kronikę. Przebieg bez niej jest o kilka procent szybszy
    /// i używa go test spójności LOD, któremu narracja nie jest potrzebna.
    pub chronicle: bool,
    /// Koszyk podstawowy do bramki 8 — pary (towar, ilość na miesiąc).
    /// Buduje go wołający z `data/economy/cpi.ron`, bo to on ma katalog towarów.
    /// Pusty koszyk znaczy „bramki 8 nie mierzono" i raport mówi to wprost.
    pub basket: Vec<(GoodId, i64)>,
}

impl Default for DryRunConfig {
    fn default() -> DryRunConfig {
        DryRunConfig {
            seed: 0,
            years: 30,
            start_year: 1960,
            step_days_early: 6,
            step_days_late: 1,
            max_rebalance_rounds: 3,
            chronicle: true,
            basket: Vec::new(),
        }
    }
}

/// Rodzaj wpisu kronikarskiego z historii „na sucho".
///
/// Kolejność wariantów jest kontraktem, bo `as_index()` indeksuje klucz tekstu
/// w interfejsie (M9 rysuje kronikę, M10 dokłada do niej wpisy — `DK-1`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChronicleKind {
    /// Zaczął się wstrząs w skali miasta.
    ShockBegan(ShockKind),
    /// Wstrząs się skończył.
    ShockEnded(ShockKind),
    /// Dzielnica straciła w ciągu roku więcej niż próg swojej ludności.
    DistrictDecline,
    /// Dzielnica urosła o więcej niż próg.
    DistrictBoom,
    /// Firma zeszła poniżej zera i nie wróciła.
    FirmInsolvent,
    /// Runda naprawcza Etapu 10 — restrukturyzacja zadłużenia albo dosypanie zapasu.
    Rebalanced,
}

impl ChronicleKind {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            ChronicleKind::ShockBegan(_) => "chronicle.shock_began",
            ChronicleKind::ShockEnded(_) => "chronicle.shock_ended",
            ChronicleKind::DistrictDecline => "chronicle.district_decline",
            ChronicleKind::DistrictBoom => "chronicle.district_boom",
            ChronicleKind::FirmInsolvent => "chronicle.firm_insolvent",
            ChronicleKind::Rebalanced => "chronicle.rebalanced",
        }
    }
}

/// Wpis kronikarski. **To nie jest `game::chronicle::ChronicleEntry`** i nie ma nim
/// być (`DK-2`): tamten opisuje grę, która się toczy, ten — osiemdziesiąt lat,
/// których nikt nie rozegrał. Most między nimi jest jednokierunkowy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChronicleEvent {
    pub day: u32,
    pub year: i32,
    pub kind: ChronicleKind,
    pub district: Option<DistrictId>,
    /// Skala zdarzenia w promilach — tyle ludności straciła dzielnica, tyle
    /// punktów bazowych zabrał popyt. Liczba porządkowa, nie pieniężna.
    pub magnitude: i32,
}

/// Jedna bramka Etapu 10.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateCheck {
    /// Numer z tabeli §5.7 (D5).
    pub id: u8,
    pub key: &'static str,
    /// Zmierzona wartość w jednostce bramki (promile albo permille udziału).
    pub value: i64,
    pub lo: i64,
    pub hi: i64,
    pub pass: bool,
    /// `false` znaczy „nie było czym zmierzyć" — i taka bramka **nie przechodzi**,
    /// bo bramka niezmierzona jest deklaracją, a nie bramką (`R2`).
    pub measured: bool,
}

/// Wynik weryfikacji Etapu 10.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VerificationReport {
    pub checks: Vec<GateCheck>,
}

impl VerificationReport {
    #[must_use]
    pub fn passed(&self) -> bool {
        !self.checks.is_empty() && self.checks.iter().all(|c| c.pass)
    }

    pub fn failed(&self) -> impl Iterator<Item = &GateCheck> {
        self.checks.iter().filter(|c| !c.pass)
    }
}

/// Wynik historii „na sucho".
#[derive(Clone, Debug)]
pub struct DryRunResult {
    pub state: MacroState,
    pub chronicle: Vec<ChronicleEvent>,
    pub verification: VerificationReport,
    pub rebalance_rounds: u8,
    /// Raport rozwinięcia (D4). `None`, gdy świat nie miał na czym go przyjąć.
    pub lower: Option<LowerReport>,
    /// Ile kroków makro wykonano — miara kosztu, nie długości historii.
    pub steps: u32,
}

/// Przeprowadza historię „na sucho" i nanosi jej wynik na świat.
///
/// Świat wchodzi po Etapie 8 generatora (ludzie, firmy, gospodarka bazowa),
/// a wychodzi z majątkami, cenami, zapasami i zadłużeniem po `cfg.years` latach.
pub fn dry_run(cfg: &DryRunConfig, world: &mut World, params: &MacroParams) -> DryRunResult {
    // ── D0: zasiew ───────────────────────────────────────────────────────────
    let mut st = crate::lift(world);
    st.day = 0;
    st.tick = Tick(0);

    let dob_razem = u32::from(cfg.years) * DOB_W_ROKU;
    let prog_kroku_dobowego =
        dob_razem.saturating_sub(u32::from(LAT_NA_KONCU.min(cfg.years)) * DOB_W_ROKU);

    let mut kronika: Vec<ChronicleEvent> = Vec::new();
    let mut kroki = 0u32;
    let mut poprzedni_wstrzas: Option<ShockKind> = None;
    let mut ludnosc_dzielnic = ludnosc_per_dzielnica(&st);
    let mut okno: Option<Okno> = None;

    // ── D1: bieg ─────────────────────────────────────────────────────────────
    while st.day < dob_razem {
        let dni = if st.day < prog_kroku_dobowego {
            cfg.step_days_early.max(1)
        } else {
            cfg.step_days_late.max(1)
        };
        let p = MacroParams {
            seed: cfg.seed,
            days_per_step: dni,
            ..params.clone()
        };
        if okno.is_none() && st.day + u32::from(dni) >= dob_razem.saturating_sub(OKNO_POMIARU) {
            okno = Some(Okno::otwórz(&st));
        }
        step(&mut st, &p);
        kroki += 1;

        // ── D3: kronika ──────────────────────────────────────────────────────
        if cfg.chronicle {
            zapisz_wstrzas(&mut kronika, &st, cfg, &mut poprzedni_wstrzas);
            if crate::step::przekroczono(st.day - u32::from(dni), u32::from(dni), DOB_W_ROKU) {
                zapisz_dzielnice(&mut kronika, &st, cfg, &mut ludnosc_dzielnic);
            }
        }
    }

    // ── D5: weryfikacja i naprawa ────────────────────────────────────────────
    let mut okno = okno.unwrap_or_else(|| Okno::otwórz(&st));
    let mut rundy = 0u8;
    let mut raport = zweryfikuj(&st, &okno, cfg);
    while !raport.passed() && rundy < cfg.max_rebalance_rounds {
        napraw(&mut st, &raport, params, cfg, &mut kronika, &mut okno);
        rundy += 1;
        raport = zweryfikuj(&st, &okno, cfg);
    }

    // ── D4: rozwinięcie ──────────────────────────────────────────────────────
    // Po naprawie, nie przed nią: świat ma dostać stan, który przeszedł bramki,
    // a nie ten, który dopiero ma je przejść.
    let raport_lower = lower(&st, world, cfg.seed).ok();

    DryRunResult {
        state: st,
        chronicle: kronika,
        verification: raport,
        rebalance_rounds: rundy,
        lower: raport_lower,
        steps: kroki,
    }
}

// ── D3: kronika ─────────────────────────────────────────────────────────────────

/// Ile promili ludności musi stracić albo zyskać dzielnica w ciągu roku, żeby
/// trafić do kroniki. Próg jest **wyższy niż dla zdarzenia w partii** (`R9`):
/// osiemdziesiąt lat × wszystkie zdarzenia to kronika, której nikt nie przeczyta.
const PROG_ZMIANY_DZIELNICY: i32 = 40;

fn ludnosc_per_dzielnica(st: &MacroState) -> Vec<(u16, u32)> {
    let mut out: Vec<(u16, u32)> = Vec::new();
    for c in &st.cells {
        match out.iter_mut().find(|(d, _)| *d == c.key.0 .0) {
            Some((_, n)) => *n = n.saturating_add(c.population()),
            None => out.push((c.key.0 .0, c.population())),
        }
    }
    out.sort_unstable();
    out
}

fn rok(cfg: &DryRunConfig, day: u32) -> i32 {
    cfg.start_year + i32::try_from(day / DOB_W_ROKU).unwrap_or(0)
}

fn zapisz_wstrzas(
    kronika: &mut Vec<ChronicleEvent>,
    st: &MacroState,
    cfg: &DryRunConfig,
    poprzedni: &mut Option<ShockKind>,
) {
    let teraz = st.shock.map(|s| s.kind);
    if teraz == *poprzedni {
        return;
    }
    if let Some(stary) = *poprzedni {
        kronika.push(ChronicleEvent {
            day: st.day,
            year: rok(cfg, st.day),
            kind: ChronicleKind::ShockEnded(stary),
            district: None,
            magnitude: 0,
        });
    }
    if let Some(nowy) = teraz {
        kronika.push(ChronicleEvent {
            day: st.day,
            year: rok(cfg, st.day),
            kind: ChronicleKind::ShockBegan(nowy),
            district: None,
            magnitude: st.shock.map_or(0, |s| s.magnitude_bp),
        });
    }
    *poprzedni = teraz;
}

fn zapisz_dzielnice(
    kronika: &mut Vec<ChronicleEvent>,
    st: &MacroState,
    cfg: &DryRunConfig,
    poprzednia: &mut Vec<(u16, u32)>,
) {
    let teraz = ludnosc_per_dzielnica(st);
    for (d, n) in &teraz {
        let Some((_, przed)) = poprzednia.iter().find(|(dd, _)| dd == d) else {
            continue;
        };
        if *przed == 0 {
            continue;
        }
        let zmiana = (i64::from(*n) - i64::from(*przed)) * 1_000 / i64::from(*przed);
        let zmiana = i32::try_from(zmiana).unwrap_or(0);
        if zmiana.abs() < PROG_ZMIANY_DZIELNICY {
            continue;
        }
        kronika.push(ChronicleEvent {
            day: st.day,
            year: rok(cfg, st.day),
            kind: if zmiana < 0 {
                ChronicleKind::DistrictDecline
            } else {
                ChronicleKind::DistrictBoom
            },
            district: Some(DistrictId(*d)),
            magnitude: zmiana,
        });
    }
    *poprzednia = teraz;
}

// ── D5: bramki Etapu 10 ─────────────────────────────────────────────────────────

/// Zdjęcie zapasów i popytu sprzed okna pomiaru. Nierównowagę liczy się z różnicy
/// dwóch zdjęć, bo żadne z nich osobno nie mówi, ile towaru **przepłynęło**.
struct Okno {
    stock: Vec<(u16, i64)>,
    demand: Vec<(u16, i64)>,
}

impl Okno {
    fn otwórz(st: &MacroState) -> Okno {
        Okno {
            stock: suma_zapasow(st),
            demand: suma_popytu(st),
        }
    }
}

fn suma_zapasow(st: &MacroState) -> Vec<(u16, i64)> {
    let mut out: Vec<(u16, i64)> = Vec::new();
    for f in &st.firms {
        for (g, v) in f.stock.iter() {
            dodaj(&mut out, g.0, v);
        }
    }
    out.sort_unstable();
    out
}

fn suma_popytu(st: &MacroState) -> Vec<(u16, i64)> {
    let mut out: Vec<(u16, i64)> = Vec::new();
    for c in &st.cells {
        for (g, v) in c.demand.iter() {
            dodaj(&mut out, g.0, v);
        }
    }
    out.sort_unstable();
    out
}

fn dodaj(v: &mut Vec<(u16, i64)>, k: u16, delta: i64) {
    match v.iter_mut().find(|(kk, _)| *kk == k) {
        Some((_, x)) => *x = x.saturating_add(delta),
        None => v.push((k, delta)),
    }
}

fn wartosc(v: &[(u16, i64)], k: u16) -> i64 {
    v.iter().find(|(kk, _)| *kk == k).map_or(0, |(_, x)| *x)
}

/// Sprawdza bramki Etapu 10. Wszystkie wartości w **promilach**, żeby jedna
/// jednostka opisywała i udziały, i stopy — raport ma się czytać bez tabeli
/// przeliczeniowej.
fn zweryfikuj(st: &MacroState, okno: &Okno, cfg: &DryRunConfig) -> VerificationReport {
    let mut checks = Vec::new();
    let stock_teraz = suma_zapasow(st);
    let popyt_teraz = suma_popytu(st);

    // 1. Nierównowaga każdego towaru ≤ 300 ‰.
    let mut najgorsza = 0i64;
    let mut zmierzona = false;
    for (g, _) in &popyt_teraz {
        let sprzedano = wartosc(&popyt_teraz, *g) - wartosc(&okno.demand, *g);
        if sprzedano <= 0 {
            continue;
        }
        let delta_zapasu = wartosc(&stock_teraz, *g) - wartosc(&okno.stock, *g);
        let wyprodukowano = sprzedano + delta_zapasu;
        let mianownik = sprzedano.max(wyprodukowano).max(1);
        let niezrownowazenie = (wyprodukowano - sprzedano).abs() * 1_000 / mianownik;
        najgorsza = najgorsza.max(niezrownowazenie);
        zmierzona = true;
    }
    checks.push(bramka(1, "imbalance", najgorsza, 0, 300, zmierzona));

    // 2. Pokrycie zapasem każdego konsumowanego towaru ≥ 3 doby.
    let mut najmniejsze = i64::MAX;
    let mut mierzone = false;
    for (g, _) in &popyt_teraz {
        let sprzedano = wartosc(&popyt_teraz, *g) - wartosc(&okno.demand, *g);
        if sprzedano <= 0 {
            continue;
        }
        let dziennie = (sprzedano / i64::from(OKNO_POMIARU)).max(1);
        najmniejsze = najmniejsze.min(wartosc(&stock_teraz, *g) / dziennie);
        mierzone = true;
    }
    let pokrycie = if mierzone { najmniejsze } else { 0 };
    checks.push(bramka(
        2,
        "stock_cover_days",
        pokrycie,
        3,
        i64::MAX,
        mierzone,
    ));

    // 3. Każda firma ma ludzi. Udział firm bez obsady wśród tych, które jej chcą.
    let chcace: Vec<&crate::state::MacroFirm> = st
        .firms
        .iter()
        .filter(|f| f.capacity_daily.get() > 0)
        .collect();
    let bez_ludzi = chcace.iter().filter(|f| f.employees == 0).count();
    let udzial = if chcace.is_empty() {
        0
    } else {
        (bez_ludzi as i64) * 1_000 / chcace.len() as i64
    };
    checks.push(bramka(
        3,
        "firms_without_staff",
        udzial,
        0,
        0,
        !chcace.is_empty(),
    ));

    // 4. Bezrobocie 30–150 ‰.
    let bezrobocie = i64::from(st.unemployment_permille());
    checks.push(bramka(4, "unemployment", bezrobocie, 30, 150, true));

    // 5. Mediana dług/aktywa firm 100–600 ‰.
    let mut dzwignie: Vec<i64> = st
        .firms
        .iter()
        .map(|f| {
            let aktywa = f.capital.get().max(0) + f.debt.get().max(0);
            if aktywa == 0 {
                0
            } else {
                f.debt.get().max(0) * 1_000 / aktywa
            }
        })
        .collect();
    dzwignie.sort_unstable();
    let mediana = dzwignie.get(dzwignie.len() / 2).copied().unwrap_or(0);
    checks.push(bramka(
        5,
        "median_leverage",
        mediana,
        100,
        600,
        !dzwignie.is_empty(),
    ));

    // 6. Odsetek firm niewypłacalnych w najgorszej dzielnicy < 400 ‰.
    checks.push(bramka(
        6,
        "insolvent_share_worst_district",
        najgorsza_dzielnica(st),
        0,
        399,
        !st.firms.is_empty(),
    ));

    // 7. Gini majątku gospodarstw 250–450 ‰.
    let gini = gini_majatku(st, cfg.seed);
    checks.push(bramka(
        7,
        "wealth_gini",
        gini,
        250,
        450,
        st.population() > 0,
    ));

    // 8. Koszyk podstawowy / mediana dochodu 250–550 ‰.
    let (koszyk, mierzalny) = koszyk_do_dochodu(st, &cfg.basket);
    checks.push(bramka(8, "basket_to_income", koszyk, 250, 550, mierzalny));

    VerificationReport { checks }
}

fn bramka(id: u8, key: &'static str, value: i64, lo: i64, hi: i64, measured: bool) -> GateCheck {
    GateCheck {
        id,
        key,
        value,
        lo,
        hi,
        pass: measured && value >= lo && value <= hi,
        measured,
    }
}

fn najgorsza_dzielnica(st: &MacroState) -> i64 {
    let mut per: Vec<(u16, u32, u32)> = Vec::new();
    for f in &st.firms {
        let wpis = match per.iter_mut().find(|(d, _, _)| *d == f.district.0) {
            Some(w) => w,
            None => {
                per.push((f.district.0, 0, 0));
                per.last_mut().expect("właśnie dopisane")
            }
        };
        wpis.1 += 1;
        if f.capital.get() < 0 {
            wpis.2 += 1;
        }
    }
    per.iter()
        .map(|(_, wszystkie, zle)| i64::from(*zle) * 1_000 / i64::from((*wszystkie).max(1)))
        .max()
        .unwrap_or(0)
}

/// Współczynnik Giniego majątku gospodarstw, w promilach.
///
/// Liczony z **rozwinięcia**, nie z komórek: to rozwinięcie dostanie świat,
/// a rozrzut wewnątrz komórki jest połową całej nierówności. Gini z samych
/// średnich komórkowych mierzyłby różnice między dzielnicami i milczał o tym,
/// co dzieje się w jednej.
fn gini_majatku(st: &MacroState, seed: u64) -> i64 {
    let mut majatki: Vec<i64> = Vec::new();
    for c in &st.cells {
        let e = lower_cell(c, seed, st.tick);
        majatki.extend(
            e.people
                .iter()
                .map(|p| p.cash.get().saturating_add(p.deposits.get()).max(0)),
        );
    }
    if majatki.len() < 2 {
        return 0;
    }
    majatki.sort_unstable();
    let n = majatki.len() as i128;
    let suma: i128 = majatki.iter().map(|m| i128::from(*m)).sum();
    if suma <= 0 {
        return 0;
    }
    let wazona: i128 = majatki
        .iter()
        .enumerate()
        .map(|(i, m)| (i as i128 + 1) * i128::from(*m))
        .sum();
    // G = (2·Σ i·x_i)/(n·Σx) − (n+1)/n, w promilach.
    let g = (2 * wazona * 1_000) / (n * suma) - ((n + 1) * 1_000) / n;
    i64::try_from(g.clamp(0, 1_000)).unwrap_or(0)
}

/// Koszt miesięcznego koszyka podzielony przez medianę miesięcznego dochodu
/// gospodarstwa, w promilach.
///
/// Dochodu makro nie prowadzi wprost, więc bierze go z listy płac firm:
/// suma `wage_bill` podzielona przez liczbę zatrudnionych to przeciętne
/// wynagrodzenie miesięczne — i jest to ta sama liczba, którą mezo płaci
/// w `pay_incomes`, tylko uśredniona.
fn koszyk_do_dochodu(st: &MacroState, basket: &[(GoodId, i64)]) -> (i64, bool) {
    if basket.is_empty() || st.firms.is_empty() {
        return (0, false);
    }
    let mut koszt: i64 = 0;
    let mut pokryte = 0usize;
    for (g, ile) in basket {
        let mut ceny: Vec<i64> = st
            .firms
            .iter()
            .filter_map(|f| f.price.get(*g))
            .map(Money::get)
            .filter(|c| *c > 0)
            .collect();
        if ceny.is_empty() {
            continue;
        }
        ceny.sort_unstable();
        koszt = koszt.saturating_add(ceny[ceny.len() / 2].saturating_mul(*ile));
        pokryte += 1;
    }
    if pokryte == 0 {
        return (0, false);
    }
    let zatrudnieni: i64 = st.firms.iter().map(|f| i64::from(f.employees)).sum();
    if zatrudnieni == 0 {
        return (0, false);
    }
    let placa: i64 = st
        .firms
        .iter()
        .map(|f| f.wage_bill.get())
        .fold(0i64, i64::saturating_add)
        / zatrudnieni;
    if placa <= 0 {
        return (0, false);
    }
    (koszt.saturating_mul(1_000) / placa, true)
}

// ── D5: naprawa ─────────────────────────────────────────────────────────────────

/// Minimalne pokrycie zapasem, do którego dosypuje runda naprawcza.
const MIN_POKRYCIE_DOB: i64 = 5;

/// Do ilu promili sprowadza się dźwignię przy restrukturyzacji.
const DOCELOWA_DZWIGNIA: i64 = 400;

/// Naprawa, nie odrzucenie ziarna (§5.7). Odrzucenie 40 % ziaren byłoby porażką
/// generatora i wściekłością gracza, który wybrał ziarno.
///
/// Trzy z czterech napraw z planu są tutaj. Czwartej — „brakujący dostawca" —
/// **nie ma i nie będzie**: zatowarowanie w fazie 3 idzie z importu, więc każda
/// firma ma dostawcę z konstrukcji i naprawa opisywałaby stan, który nie zachodzi.
fn napraw(
    st: &mut MacroState,
    raport: &VerificationReport,
    params: &MacroParams,
    cfg: &DryRunConfig,
    kronika: &mut Vec<ChronicleEvent>,
    okno: &mut Okno,
) {
    let mut zrobione = false;
    for check in raport.failed() {
        match check.id {
            2 => {
                dosyp_zapasy(st, okno);
                zrobione = true;
            }
            1 => {
                zresetuj_ceny(st, params);
                zrobione = true;
            }
            5 | 6 => {
                zrestrukturyzuj(st);
                zrobione = true;
            }
            _ => {}
        }
    }
    if zrobione && cfg.chronicle {
        kronika.push(ChronicleEvent {
            day: st.day,
            year: rok(cfg, st.day),
            kind: ChronicleKind::Rebalanced,
            district: None,
            magnitude: 0,
        });
    }
}

/// Dosypanie zapasu **z długiem**, a nie z powietrza: firma zaciąga import
/// i zostaje z zobowiązaniem. Pieniądz się nie rusza, bo dług nim nie jest.
/// Dosypany zapas wchodzi **także do podstawy okna pomiaru**, bo nie jest produkcją
/// tylko importem zaciągniętym poza rynkiem. Bez tego bramka 1 mierzyłaby własną
/// naprawę: przyrost zapasu wyglądałby jak nadprodukcja i nierównowaga skakała
/// do stu procent po pierwszej rundzie, zawsze i niezależnie od świata.
fn dosyp_zapasy(st: &mut MacroState, okno: &mut Okno) {
    for f in &mut st.firms {
        let towary: Vec<(GoodId, i64)> = f.cost.iter().map(|(g, c)| (g, c.get())).collect();
        if towary.is_empty() {
            continue;
        }
        let na_towar = (f.capacity_daily.get() / towary.len().max(1) as i64).max(1);
        for (g, koszt) in towary {
            let cel = na_towar.saturating_mul(MIN_POKRYCIE_DOB);
            let brak = cel - f.stock.get(g);
            if brak <= 0 {
                continue;
            }
            f.stock.add(g, brak);
            dodaj(&mut okno.stock, g.0, brak);
            f.debt = Money(f.debt.get().saturating_add(koszt.saturating_mul(brak)));
        }
    }
}

/// Sprowadzenie ceny do punktu równowagi: koszt własny plus marża docelowa,
/// przy zapasie dokładnie w celu i bez oglądania się na konkurencję.
///
/// Jednym krokiem, nie iteracyjnie, i **przez jądro** — to jest ta sama funkcja,
/// którą liczy cenę faza 5 i sklep w mezo.
fn zresetuj_ceny(st: &mut MacroState, p: &MacroParams) {
    for f in &mut st.firms {
        let towary: Vec<GoodId> = f.price.iter().map(|(g, _)| g).collect();
        for g in towary {
            let koszt = f.cost.get(g).unwrap_or(Money::ZERO);
            if koszt.get() <= 0 {
                continue;
            }
            f.price.set(
                g,
                kernel::next_price(PriceInput {
                    unit_cost: koszt,
                    target_margin_bp: p.target_margin_bp,
                    min_margin_bp: p.margin_bp.0,
                    max_margin_bp: p.margin_bp.1,
                    stock_bp_of_target: 10_000,
                    k_stock: p.k_stock,
                    competitor_net: None,
                    k_comp: p.k_comp,
                    adj_elast_bp: 0,
                    adj_spoil_bp: 0,
                }),
            );
        }
    }
}

/// Restrukturyzacja: dźwignia powyżej progu schodzi do [`DOCELOWA_DZWIGNIA`].
/// Umorzony dług **nie staje się pieniądzem** — znika po stronie zobowiązań.
fn zrestrukturyzuj(st: &mut MacroState) {
    for f in &mut st.firms {
        let aktywa = f.capital.get().max(0) + f.debt.get().max(0);
        if aktywa == 0 {
            continue;
        }
        let dzwignia = f.debt.get().max(0) * 1_000 / aktywa;
        if dzwignia <= DOCELOWA_DZWIGNIA {
            continue;
        }
        let cel = f.capital.get().max(0) * DOCELOWA_DZWIGNIA / (1_000 - DOCELOWA_DZWIGNIA);
        f.debt = Money(cel.min(f.debt.get()));
    }
}
