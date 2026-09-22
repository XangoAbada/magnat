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

// ── R2-WP24: bramka bezrobocia naprawdę mierzy bezrobocie ────────────────────────
//
// Ta sama zasada co wyżej, zastosowana do G11. Test jest **odtwarzający**: na
// kodzie sprzed R2f syntetyczny przebieg z bezrobociem 2 ‰ nie dawał werdyktu
// „czerwony", tylko nie dawał żadnego — filtr „wakatów nie więcej niż ludzi
// w sile roboczej" wykluczał go z oceny, a w profilu `ci` bramka znikała
// z raportu całkiem. Świat z bezrobociem 0,2 % przy 12 032 pustych etatach
// (poz. 6 wykazu `R2`) przechodził więc przez bieg nocny bez jednej czerwonej
// lampki przez całe M7.
//
// Liczby są wzięte z tamtego pomiaru, a nie wymyślone.

use magnat_balansator::gates::{evaluate, Profile, Verdict};
use magnat_balansator::metrics::{DayMetrics, RunFile, SCHEMA_VERSION};

/// Bezrobocie zmierzone w mieście z poz. 6 wykazu: 2 promile siły roboczej.
const BEZROBOCIE_PERMILLE: u16 = 2;
/// Puste etaty z tego samego pomiaru — **więcej niż ludzi w sile roboczej**,
/// i to właśnie ta nierówność wyłączała bramkę.
const WAKATY: u32 = 12_032;
const SILA_ROBOCZA: u32 = 9_000;

fn doba(day: u32) -> DayMetrics {
    DayMetrics {
        day,
        prices: Vec::new(),
        cpi_index_bp: 10_000,
        cpi_mom_bp: Some(0),
        cpi_yoy_bp: Some(0),
        base_rate_bp: 500,
        margin_median_bp: 3_000,
        shops: 50,
        insolvent: 0,
        hhi_median: 1_000,
        hhi_pairs: 10,
        live_categories: 8,
        stockout_permille: 0,
        stockout_refusal_permille: 0,
        deferral_permille: 0,
        purchases: 100,
        revenue_gr: 10_000,
        reprices: 0,
        write_offs: 0,
        write_off_gr: 0,
        expired_qty: 0,
        loans: 0,
        credit_outstanding_gr: 0,
        money_supply_gr: 1_000_000,
        firms: 200,
        firm_sites: 202,
        unemployment_permille: BEZROBOCIE_PERMILLE,
        labour_force: SILA_ROBOCZA,
        vacancies: WAKATY,
        firms_founded: 0,
        firms_gone: 0,
    }
}

/// Przebieg dość długi, żeby ogon G11 istniał (`G11_MIN_DAYS` = 90).
fn przebieg(days: u16) -> RunFile {
    RunFile {
        schema_version: SCHEMA_VERSION,
        scenario: "base".to_string(),
        seed: 1,
        days,
        citizens: 24_800,
        shops: 50,
        hashes: Vec::new(),
        days_data: (0..=u32::from(days)).map(doba).collect(),
        shock_good: None,
        conservation_ok: true,
        decisions_without_reason: 0,
        decisions_sampled: 1_000,
        lod: Vec::new(),
    }
}

fn bramka<'a>(
    w: &'a [magnat_balansator::gates::GateOutcome],
    id: &str,
) -> &'a magnat_balansator::gates::GateOutcome {
    w.iter()
        .find(|g| g.gate == id)
        .unwrap_or_else(|| panic!("bramki {id} nie ma w raporcie"))
}

#[test]
fn bezrobocie_dwa_promile_czerwieni_bramke_g11() {
    let werdykty = evaluate(&[przebieg(120)], Profile::Nightly, 2_400);
    let g11 = bramka(&werdykty, "G11");
    assert_eq!(
        g11.verdict,
        Verdict::Red,
        "2 ‰ bezrobocia jest poza pasmem 3–12 % niezależnie od tego, ile jest wakatów; \
         zmierzono: {}",
        g11.value
    );
    // Gęstość etatów zostaje w opisie, bo to ona jest przyczyną i ma adresata (R3).
    assert!(
        g11.value.contains(&format!("{WAKATY}/{SILA_ROBOCZA}")),
        "opis ma nieść wakaty i siłę roboczą: {}",
        g11.value
    );
}

/// Przebieg krótszy niż ogon nie ma czego zmierzyć — i **mówi to**, zamiast
/// znikać z raportu.
#[test]
fn przebieg_bez_ogona_pomija_g11_z_powodem() {
    let werdykty = evaluate(&[przebieg(30)], Profile::Nightly, 2_400);
    let g11 = bramka(&werdykty, "G11");
    assert_eq!(g11.verdict, Verdict::Skipped);
    assert!(
        g11.blokuje(Profile::Nightly),
        "`D-N17`: w biegu nocnym pominięcie jest błędem konfiguracji"
    );
}

/// Profil `ci` nadal nie liczy czterech bramek — ale one **zostają w raporcie**
/// jako pominięte. Do R2f raport profilu `ci` miał osiem wierszy i nikt nie
/// wiedział, że brakuje czterech.
#[test]
fn profil_ci_pomija_bramki_bez_usuwania_ich_z_raportu() {
    let werdykty = evaluate(&[przebieg(120)], Profile::Ci, 2_400);
    assert_eq!(werdykty.len(), 12, "dwanaście bramek, zawsze");
    for id in ["G4", "G6", "G11", "G12"] {
        let g = bramka(&werdykty, id);
        assert_eq!(g.verdict, Verdict::Skipped, "{id}");
        assert!(!g.blokuje(Profile::Ci), "{id} nie wywraca profilu ci");
    }
}

/// **G1 bez roku historii nie jest zielona** (N1.6, `M5#2`). Przebieg 120 dób ma cztery
/// miesiące, a G1 liczy inflację r/r od 12. — do E1 `all()` na pustym zbiorze dawało
/// zieleń, więc bramka pull requesta nie zmierzyła G1 ani razu i świeciła na zielono.
#[test]
fn g1_bez_roku_historii_jest_pominieta_a_nie_zielona() {
    let werdykty = evaluate(&[przebieg(120)], Profile::Nightly, 2_400);
    let g1 = bramka(&werdykty, "G1");
    assert_eq!(g1.verdict, Verdict::Skipped, "zmierzono: {}", g1.value);
    assert!(
        g1.blokuje(Profile::Nightly),
        "bieg nocny musi mieć dane dla G1"
    );
    let werdykty = evaluate(&[przebieg(120)], Profile::Ci, 2_400);
    assert!(
        !bramka(&werdykty, "G1").blokuje(Profile::Ci),
        "profil ci wymienia G1 z nazwy jako dopuszczalnie bez danych"
    );
}

/// G3 w połowie deflacyjnej potrzebuje siedmiu miesięcy (sześć spadków z rzędu).
/// Cztery miesiące nie mogą jej zaczerwienić, więc nie mogą też jej zazielenić.
#[test]
fn g3_bez_siedmiu_miesiecy_nie_udaje_pomiaru_deflacji() {
    let werdykty = evaluate(&[przebieg(120)], Profile::Nightly, 2_400);
    let g3 = bramka(&werdykty, "G3");
    assert_eq!(g3.verdict, Verdict::Skipped, "zmierzono: {}", g3.value);
    let werdykty = evaluate(&[przebieg(420)], Profile::Nightly, 2_400);
    assert_eq!(bramka(&werdykty, "G3").verdict, Verdict::Green);
    assert_eq!(
        bramka(&werdykty, "G1").verdict,
        Verdict::Green,
        "420 dób to 14 miesięcy"
    );
}

/// Profil `ci` dopuszcza pominięcie **tylko bramek wymienionych z nazwy** —
/// pominięta G2 (a mierzy się zawsze) wywraca także bramkę pull requesta.
#[test]
fn profil_ci_nie_przepuszcza_pominiecia_spoza_listy() {
    let mut g2 = bramka(&evaluate(&[przebieg(120)], Profile::Ci, 2_400), "G2").clone();
    g2.verdict = Verdict::Skipped;
    assert!(g2.blokuje(Profile::Ci));
}
