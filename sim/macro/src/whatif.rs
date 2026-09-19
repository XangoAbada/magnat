//! „Co jeśli" i ranking wariantów (M7f WP13, §5.10).
//!
//! # To jest granica, za którą liczba z makra nie wychodzi
//!
//! Model ma nieusuwalne odchylenie 3–12 % dla pojedynczej firmy — wariancja rozkładu
//! wielomianowego przy ~200 klientach, nie kwestia kalibracji. Dlatego
//! [`MacroOutcome`] jest w [`RankedVariants`] **prywatny**: gdyby wyciekł do reguł
//! albo do UI, ktoś prędzej czy później porównałby go z progiem albo pokazał jako
//! kwotę, a fałszywa precyzja jest gorsza od braku prognozy (`R14`, `R15`).
//!
//! Na zewnątrz wychodzą dokładnie trzy rzeczy: **który wariant wygrał** (i tylko
//! wtedy, gdy wygrał ponad margines), **w którą stronę** idzie wynik i **jak szeroki
//! jest margines**. Nie ma czwartej.
//!
//! | Zakazane | Dozwolone |
//! |---|---|
//! | „wejdź, jeśli prognozowany zysk > 200 tys." | „wybierz najlepszy, jeśli przewaga > margines" |
//! | „zamknij, bo makro przewiduje stratę 40 tys." | „zamknij, bo wariant *bez* wypada lepiej niż *z*" |
//! | próg absolutny z `what_if()` | próg absolutny z **własnych ksiąg** (`SitePnlMonth`) |

use magnat_core::{FirmId, Money, SiteId, Trend};
use smallvec::SmallVec;

use crate::state::MacroState;
use crate::step::{step, MacroParams};

/// Wariant strategii do przeliczenia. Opisuje **zmianę względem stanu bieżącego**,
/// a nie stan docelowy — dzięki temu `KeepCourse` jest pustym wariantem i zawsze
/// istnieje.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scenario {
    /// Nic nie rób. **Zawsze w zestawie** i domyślnie wygrywa, gdy model nie
    /// rozstrzyga — niepewność modelu ma się przekładać na bezwładność firmy,
    /// a nie na losowe miotanie się (§5.10).
    ///
    /// Niesie firmę, choć nic w niej nie zmienia, i to nie jest nadmiar: wariant
    /// jest **punktem odniesienia**, a punkt odniesienia musi wiedzieć, co mierzy.
    /// Bez tego pola porównanie „zamknąć czy nie" zestawiałoby wynik firmy z zerem
    /// i wygrywało zawsze — co jest dokładnie tym rodzajem cichej pomyłki, przed
    /// którą `decisive_winner` ma bronić.
    KeepCourse { firm: FirmId },
    /// Nowy zakład w dzielnicy: tyle etatów, tyle kapitału na start.
    OpenSite {
        firm: FirmId,
        district: u16,
        slots: u32,
        capex: Money,
    },
    /// Zamknięcie zakładu: tyle etatów znika razem z nim.
    CloseSite {
        firm: FirmId,
        site: SiteId,
        slots: u32,
    },
    /// Zmiana marży docelowej firmy o `delta_bp`.
    Reprice { firm: FirmId, delta_bp: i32 },
}

impl Scenario {
    /// Firma, której wariant dotyczy. `KeepCourse` nie dotyczy żadnej.
    #[must_use]
    pub fn firm(&self) -> Option<FirmId> {
        match self {
            Scenario::KeepCourse { firm }
            | Scenario::OpenSite { firm, .. }
            | Scenario::CloseSite { firm, .. }
            | Scenario::Reprice { firm, .. } => Some(*firm),
        }
    }
}

/// Wynik jednego przebiegu „co jeśli".
///
/// Pola są publiczne w obrębie crate'u i **nie są reeksportowane jako liczby**:
/// jedyną drogą na zewnątrz jest [`RankedVariants`], która ich nie oddaje.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MacroOutcome {
    /// Kapitał firmy po horyzoncie — miara porównawcza, nie prognoza kwoty.
    pub(crate) equity: Money,
    /// Zatrudnienie firmy po horyzoncie.
    pub(crate) employees: u32,
    /// Deklarowany błąd **tego** wyniku. Konsument go nie zgaduje i nie nadpisuje
    /// (M10 §6): margines jest obietnicą modelu, a nie parametrem wołającego.
    pub error_margin_bp: u32,
}

/// Dolna i górna granica deklarowanego marginesu, w punktach bazowych.
///
/// Odchylenie rośnie z horyzontem i maleje z liczbą klientów — te dwie liczby
/// są końcami przedziału 3–12 % z rozstrzygnięcia M10 i nie są kalibracją:
/// przestawienie ich byłoby zmianą **obietnicy**, a test uczciwości marginesu
/// natychmiast by to pokazał.
/// macro-guard: deklarowany błąd modelu, nie stawka ekonomiczna
const MARGIN_MIN_BP: u32 = 300;
const MARGIN_MAX_BP: u32 = 1_200;

/// Przebieg jednego scenariusza na **klonie** stanu. Świat nie drga.
#[must_use]
pub fn what_if(base: &MacroState, sc: &Scenario, horizon_days: u16) -> MacroOutcome {
    let mut st = base.clone();
    let mut params = MacroParams::default();
    zastosuj(&mut st, sc, &mut params);
    for _ in 0..horizon_days {
        step(&mut st, &params);
    }
    let firma = sc
        .firm()
        .and_then(|id| st.firm_index(id))
        .map(|i| &st.firms[i]);
    MacroOutcome {
        equity: firma.map_or(Money::ZERO, crate::state::MacroFirm::equity),
        employees: firma.map_or(0, |f| f.employees),
        error_margin_bp: margines(base, sc, horizon_days),
    }
}

/// Deklarowany margines: rośnie z horyzontem, maleje z liczbą klientów firmy.
///
/// „Liczba klientów" to populacja dzielnicy, w której firma stoi, podzielona przez
/// liczbę firm konkurujących w niej o ten sam grosz. To jest ta sama wielkość,
/// z której bierze się 3–12 % w rozstrzygnięciu M10 — wariancja rozkładu
/// wielomianowego jest funkcją `n`, a nie jakości modelu.
fn margines(base: &MacroState, sc: &Scenario, horizon_days: u16) -> u32 {
    let Some(i) = sc.firm().and_then(|id| base.firm_index(id)) else {
        return MARGIN_MAX_BP;
    };
    let d = base.firms[i].district.0;
    let klienci: u32 = base
        .cells
        .iter()
        .filter(|c| c.key.0 .0 == d)
        .map(crate::state::MacroCell::population)
        .fold(0, u32::saturating_add);
    let rywale = base
        .firms
        .iter()
        .filter(|f| f.district.0 == d)
        .count()
        .max(1) as u32;
    let n = (klienci / rywale).max(1);
    // Odchylenie ~ 1/sqrt(n), przeskalowane tak, żeby n = 200 dało środek pasma.
    let bazowy = (MARGIN_MAX_BP * 14) / (isqrt(u64::from(n)) as u32).max(1);
    let z_horyzontem = bazowy.saturating_add(u32::from(horizon_days) / 2);
    z_horyzontem.clamp(MARGIN_MIN_BP, MARGIN_MAX_BP)
}

/// Pierwiastek całkowitoliczbowy. `f64::sqrt` byłby legalny (`K-6` dopuszcza
/// podstawową arytmetykę), ale margines wchodzi do porównania i lepiej, żeby był
/// liczbą całkowitą od początku do końca.
fn isqrt(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// Nanosi wariant na klon stanu. Tu **nie ma** logiki ekonomicznej: wariant
/// przestawia liczbę wejściową (etaty, kapitał, marżę), a co z niej wyniknie,
/// rozstrzyga krok.
fn zastosuj(st: &mut MacroState, sc: &Scenario, params: &mut MacroParams) {
    match sc {
        Scenario::KeepCourse { .. } => {}
        Scenario::OpenSite {
            firm,
            district: _,
            slots,
            capex,
        } => {
            if let Some(i) = st.firm_index(*firm) {
                let f = &mut st.firms[i];
                f.capacity_daily = magnat_core::Qty(
                    f.capacity_daily.get()
                        + i64::from(*slots) * magnat_firms::hr::productivity::FULL_TIME,
                );
                f.capital = Money(f.capital.get() - capex.get());
            }
        }
        Scenario::CloseSite { firm, slots, .. } => {
            if let Some(i) = st.firm_index(*firm) {
                let f = &mut st.firms[i];
                f.capacity_daily = magnat_core::Qty(
                    (f.capacity_daily.get()
                        - i64::from(*slots) * magnat_firms::hr::productivity::FULL_TIME)
                        .max(0),
                );
            }
        }
        Scenario::Reprice { delta_bp, .. } => {
            params.target_margin_bp =
                (params.target_margin_bp + delta_bp).clamp(params.margin_bp.0, params.margin_bp.1);
        }
    }
}

/// Uporządkowane warianty. Jedyne wyjście modelu do AI i do UI.
#[derive(Clone, Debug)]
pub struct RankedVariants {
    /// Prywatne **celowo**: `MacroOutcome` nie ma prawa opuścić tej struktury.
    ranked: SmallVec<[(usize, MacroOutcome); 5]>,
    /// Deklarowany przez model margines błędu porównania.
    pub error_margin_bp: u32,
}

impl RankedVariants {
    /// Zwycięzca **tylko** wtedy, gdy jego przewaga nad drugim przekracza margines
    /// błędu. Inaczej `None`, czyli `KeepCourse`.
    #[must_use]
    pub fn decisive_winner(&self) -> Option<usize> {
        let (i, pierwszy) = self.ranked.first()?;
        let Some((_, drugi)) = self.ranked.get(1) else {
            // Jeden wariant to brak wyboru, a nie zwycięstwo.
            return None;
        };
        let odniesienie = pierwszy
            .equity
            .get()
            .abs()
            .max(drugi.equity.get().abs())
            .max(1);
        let przewaga =
            (pierwszy.equity.get() - drugi.equity.get()).saturating_mul(10_000) / odniesienie;
        if przewaga > i64::from(self.error_margin_bp) {
            Some(*i)
        } else {
            None
        }
    }

    /// Kierunek i znak zmiany wariantu względem `KeepCourse` — bez wielkości.
    #[must_use]
    pub fn direction(&self, i: usize) -> Trend {
        let Some((_, wariant)) = self.ranked.iter().find(|(k, _)| *k == i) else {
            return Trend::Flat;
        };
        let Some((_, baza)) = self.ranked.iter().find(|(k, _)| *k == 0) else {
            return Trend::Flat;
        };
        let odniesienie = baza.equity.get().abs().max(1);
        let delta = (wariant.equity.get() - baza.equity.get()).saturating_mul(10_000) / odniesienie;
        if delta > i64::from(self.error_margin_bp) {
            Trend::Up
        } else if delta < -i64::from(self.error_margin_bp) {
            Trend::Down
        } else {
            Trend::Flat
        }
    }

    /// Ile wariantów porównano. Liczba idzie do uzasadnienia decyzji
    /// (`DecisionReason`), żeby gracz widział, **z czego** konkurent wybierał.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ranked.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ranked.is_empty()
    }
}

/// Woła [`what_if`] raz na wariant, porządkuje malejąco i dokłada margines.
///
/// **Jedyne wejście AI do makra** (§5.10). Nie udostępnia dalej surowych wartości
/// `MacroOutcome` i nie ma metody, która by je oddała.
#[must_use]
pub fn rank_variants(
    base: &MacroState,
    variants: &[Scenario],
    horizon_days: u16,
) -> RankedVariants {
    let mut ranked: SmallVec<[(usize, MacroOutcome); 5]> = SmallVec::new();
    let mut margines = MARGIN_MIN_BP;
    for (i, sc) in variants.iter().enumerate() {
        let out = what_if(base, sc, horizon_days);
        // Margines zestawu to **najszerszy** z marginesów wariantów: porównanie
        // jest tak niepewne, jak jego najbardziej niepewna strona.
        margines = margines.max(out.error_margin_bp);
        ranked.push((i, out));
    }
    // Remis rozstrzyga indeks wariantu, a nie kolejność wstawiania — przy równym
    // wyniku wygrywa wariant wcześniejszy na liście, czyli `KeepCourse`.
    ranked.sort_by(|a, b| b.1.equity.get().cmp(&a.1.equity.get()).then(a.0.cmp(&b.0)));
    RankedVariants {
        ranked,
        error_margin_bp: margines,
    }
}
