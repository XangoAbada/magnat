//! Bramki Etapu 10 — ocena świata, który wyszedł z historii „na sucho".
//!
//! # Dlaczego osobno od `dryrun`
//!
//! Bo to są dwa tematy, nie jeden długi. `dryrun` **przeprowadza** historię:
//! zasiew, bieg, kronika, rozwinięcie, naprawa. Ten moduł **ocenia** stan, który
//! z niej wyszedł, i nie wie nic o tym, jak powstał — bierze `MacroState`, okno
//! pomiaru i konfigurację, oddaje dziewięć liczb z werdyktem.
//!
//! Granica jest ostra i widać ją po wejściach: żadna funkcja stąd nie bierze
//! `&mut`, bo bramka niczego nie naprawia. Naprawa jest po tamtej stronie.
//!
//! # Wszystko w promilach
//!
//! Jedna jednostka opisuje i udziały, i stopy, i współczynniki — raport ma się
//! czytać bez tabeli przeliczeniowej. Zero `f64` w całym module (00 §2):
//! werdykt bramki nie ma prawa zależeć od kolejności działań.

use magnat_core::{GoodId, Money};

use crate::dryrun::{
    suma_popytu, suma_zapasow, wartosc, DryRunConfig, GateCheck, Okno, VerificationReport,
    OKNO_POMIARU,
};
use crate::lower::lower_cell;
use crate::state::MacroState;

/// Sprawdza bramki Etapu 10. Wszystkie wartości w **promilach**, żeby jedna
/// jednostka opisywała i udziały, i stopy — raport ma się czytać bez tabeli
/// przeliczeniowej.
pub(crate) fn zweryfikuj(st: &MacroState, okno: &Okno, cfg: &DryRunConfig) -> VerificationReport {
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

    // 7. Gini **majątku** gospodarstw 550–850 ‰.
    //
    // Pasmo zmieniło się z 250–450 przy rozstrzygnięciu `D9` fazy M10 i to nie
    // jest poszerzenie progu, tylko naprawa pomiaru: 250–450 ‰ to pasmo typowe
    // dla Giniego **dochodu**, a ta bramka mierzy **majątek**, dla którego realny
    // współczynnik bywa rzędu 700–800 ‰. Jedna bramka na dwie różne wielkości
    // ukrywała obie — świeciła na czerwono od pierwszego pomiaru i nikt nie
    // wiedział, czy to wada generatora, czy wada progu. Dochód mierzy bramka 9.
    let gini = gini_majatku(st, cfg.seed);
    checks.push(bramka(
        7,
        "wealth_gini",
        gini,
        550,
        850,
        st.population() > 0,
    ));

    // 8. Koszyk podstawowy / mediana dochodu 250–550 ‰.
    let (koszyk, mierzalny) = koszyk_do_dochodu(st, &cfg.basket);
    checks.push(bramka(8, "basket_to_income", koszyk, 250, 550, mierzalny));

    // 10. Gini **dochodu** pracujących 250–450 ‰ (`D9`).
    //
    //     Numer dziesiąty, a nie dziewiąty: dziewiątkę zajmuje w tabeli §5.7
    //     bramka strukturalna („każdy mieszkaniec ma dom, grafy dróg spójne"),
    //     która pochodzi z Etapów 4–6 generatora, a nie z makra, i dlatego jej
    //     tutaj nie ma. Numer zajęty w planie zostaje zajęty.
    let (gini_doch, mierzony) = gini_dochodu(st);
    checks.push(bramka(10, "income_gini", gini_doch, 250, 450, mierzony));

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
    wspolczynnik_giniego(majatki)
}

/// Współczynnik Giniego próbki, w promilach. Jedna implementacja na dwie bramki
/// (`D9`): majątek i dochód różnią się wyłącznie tym, co się do niej wsypie.
fn wspolczynnik_giniego(mut proba: Vec<i64>) -> i64 {
    if proba.len() < 2 {
        return 0;
    }
    proba.sort_unstable();
    let n = proba.len() as i128;
    let suma: i128 = proba.iter().map(|m| i128::from(*m)).sum();
    if suma <= 0 {
        return 0;
    }
    let wazona: i128 = proba
        .iter()
        .enumerate()
        .map(|(i, m)| (i as i128 + 1) * i128::from(*m))
        .sum();
    // G = (2·Σ i·x_i)/(n·Σx) − (n+1)/n, w promilach.
    let g = (2 * wazona * 1_000) / (n * suma) - ((n + 1) * 1_000) / n;
    i64::try_from(g.clamp(0, 1_000)).unwrap_or(0)
}

/// Współczynnik Giniego **dochodu pracujących**, w promilach (bramka 9, `D9`).
///
/// Każdy zatrudniony dostaje przeciętne wynagrodzenie swojego pracodawcy —
/// `wage_bill / employees`. Dwa sufity trzeba nazwać, bo obrona progu bez nich
/// byłaby obroną liczby, która mierzy co innego, niż deklaruje:
///
/// 1. **Rozrzut wewnątrz firmy jest zerowy**, bo makro zna listę płac, a nie
///    pojedyncze pensje. Wynik jest więc **dolnym oszacowaniem** prawdziwego
///    Giniego dochodu: mierzy różnice między pracodawcami i między branżami,
///    a nie między stanowiskami u jednego.
/// 2. **Bezrobotnych nie liczymy.** Ich dochód jest w tej grze egzogeniczny
///    (decyzja nr 2 fazy M5 — płaci go pracodawca spoza miasta), więc wsypanie
///    ich z zerem albo z tamtą kwotą mierzyłoby tamtą decyzję, a nie rynek pracy,
///    o który ta bramka pyta.
fn gini_dochodu(st: &MacroState) -> (i64, bool) {
    let mut place: Vec<i64> = Vec::new();
    for f in &st.firms {
        if f.employees == 0 {
            continue;
        }
        let na_osobe = f.wage_bill.get() / i64::from(f.employees);
        if na_osobe <= 0 {
            continue;
        }
        place.extend(std::iter::repeat_n(na_osobe, f.employees as usize));
    }
    if place.len() < 2 {
        return (0, false);
    }
    (wspolczynnik_giniego(place), true)
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
