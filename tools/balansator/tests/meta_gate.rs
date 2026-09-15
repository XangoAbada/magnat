//! Meta-test bramki G3 (kryterium WP13): **bramka, która nigdy nie świeci na
//! czerwono, nie jest bramką.**
//!
//! Korekta ★ po M5c mówi, gdzie dokładnie siedzi „sztucznie wprowadzony błąd":
//! podłoga ceny to **jedna linia** w `magnat_economy::kernel::next_price_full`
//!
//! ```text
//! let floor = mul_bp(cost, BP + i64::from(i.min_margin_bp) + i64::from(adj_spoil));
//! ```
//!
//! Dlatego test jest **testem rdzenia z podmienionym wejściem**, a nie przebiegiem
//! z pokiereszowanym kodem produkcyjnym: obie serie marż powstają z tej samej pętli,
//! różnią się wyłącznie tym, czy cena przechodzi przez `clamp` do podłogi, i obie
//! idą do tej samej funkcji `gates::g3_series`, którą CI wywołuje na prawdziwych
//! danych. Gdyby bramka była wpleciona w przebieg, tego testu nie dałoby się napisać
//! bez budowania miasta — i dlatego logika bramki mieszka osobno od zbierania metryk.

use magnat_balansator::gates::g3_series;
use magnat_core::Money;
use magnat_economy::{kernel, EconomyData, PriceInput, BP};

/// Koszt własny jednostki ceny — dowolny, byle stały: bramka mierzy marżę, czyli
/// stosunek, a nie kwotę.
const KOSZT: i64 = 1_000;
/// Ile dób trwa symulowane zaleganie zapasu.
const DOB: usize = 200;

/// Wejście jednej doby: zapas rośnie liniowo od celu do dziesięciokrotności celu.
///
/// To jest dokładnie ta sytuacja, przed którą broni podłoga (§5.6, ryzyko R1):
/// sklep, któremu towar zalega, przecenia go co dobę, a bez ogranicznika przecena
/// nie ma dna.
fn wejscie(dzien: usize, min_margin_bp: i32) -> PriceInput {
    PriceInput {
        unit_cost: Money(KOSZT),
        // Dolna krawędź widełek z `data/economy/shop.ron` — sklep o najskromniejszej
        // marży docelowej. To on pierwszy dochodzi do podłogi, więc to na nim
        // widać, czy podłoga w ogóle działa.
        target_margin_bp: 2_400,
        min_margin_bp,
        max_margin_bp: 12_000,
        // 10 000 = dokładnie cel, 100 000 = dziesięć razy tyle.
        stock_bp_of_target: 10_000 + (dzien as i32) * 450,
        k_stock: 5_000,
        competitor_net: None,
        k_comp: 3_000,
        adj_elast_bp: 0,
        // Towar, który się nie psuje — inaczej `adj_spoil` obniżałby także podłogę
        // i test mierzyłby przecenę terminową zamiast spirali.
        adj_spoil_bp: 0,
    }
}

/// Marża ceny nad kosztem własnym, w punktach bazowych — to samo, co `Market`
/// zapisuje w `BalanceSample::margin_median_bp`.
fn marza_bp(cena: i64) -> i32 {
    i32::try_from((cena - KOSZT) * BP / KOSZT).unwrap_or(i32::MAX)
}

/// Cena **bez podłogi**: to samo składanie co w `next_price_full`, z pominięciem
/// przycięcia do dolnego ogranicznika. Górny ogranicznik zostaje — usuwamy jedną
/// linię, a nie pół funkcji.
fn cena_bez_podlogi(i: PriceInput) -> i64 {
    let b = kernel::next_price_full(i);
    let suma = i64::from(b.adj_stock_bp)
        + i64::from(b.adj_comp_bp)
        + i64::from(b.adj_elast_bp)
        + i64::from(b.adj_spoil_bp);
    b.base.mul_ratio(BP + suma, BP).get().min(b.ceil.get())
}

#[test]
fn usuniecie_podlogi_ceny_czerwieni_bramke_deflacji() {
    let min_margin_bp = EconomyData::load_default()
        .expect("data/economy/shop.ron")
        .pricing
        .min_margin_bp
        .min;

    let mut z_podloga = Vec::with_capacity(DOB);
    let mut bez_podlogi = Vec::with_capacity(DOB);
    for d in 0..DOB {
        let i = wejscie(d, min_margin_bp);
        let cena = kernel::next_price_full(i).price.get();
        // Niezmiennik samej podłogi, niezależny od bramki: cena nigdy nie schodzi
        // poniżej `unit_cost · (1 + min_margin_bp/10 000)`.
        assert!(
            cena >= Money(KOSZT)
                .mul_ratio(BP + i64::from(min_margin_bp), BP)
                .get(),
            "doba {d}: cena {cena} przebiła podłogę"
        );
        z_podloga.push(marza_bp(cena));
        bez_podlogi.push(marza_bp(cena_bez_podlogi(i)));
    }

    // Seria CPI jest w obu przypadkach ta sama i płaska — mierzymy wyłącznie ramię
    // marżowe bramki, żeby czerwień nie mogła pochodzić skądinąd.
    let cpi = [10_000i32; 24];

    assert!(
        g3_series(&cpi, &z_podloga, min_margin_bp),
        "z podłogą bramka G3 musi być zielona; marże {:?}…",
        &z_podloga[..5]
    );
    assert!(
        !g3_series(&cpi, &bez_podlogi, min_margin_bp),
        "bez podłogi bramka G3 musi być czerwona; marże {:?}…{:?}",
        &bez_podlogi[..5],
        &bez_podlogi[DOB - 5..]
    );
    // I dla pewności, że różnica jest tam, gdzie mówi korekta ★: ostatnia doba
    // bez podłogi stoi poniżej minimalnej marży, a z podłogą dokładnie na niej.
    assert!(bez_podlogi[DOB - 1] < min_margin_bp);
    assert_eq!(z_podloga[DOB - 1], min_margin_bp);
}
