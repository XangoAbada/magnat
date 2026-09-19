//! Most makro ↔ mezo na **prawdziwym mieście** (M10a, WP10.1 pkt 3 i WP10.4).
//!
//! Testy jednostkowe rdzenia rozwinięcia stoją w `sim/macro/tests/lower.rs` i tam
//! jest ich miejsce: `lower_cell` jest funkcją czystą i nie potrzebuje miasta.
//! Tutaj jest to, czego tamte sprawdzić nie mogą — czy zdjęcie świata, krok makro
//! i naniesienie wyniku z powrotem domykają się na świecie, który ktoś wygenerował.
//!
//! Cztery pytania:
//!
//! 1. `lift(lower(s)) == s` na polach pieniężnych — **tolerancja zero** (`K4`);
//! 2. `lower()` nie tworzy ani nie niszczy grosza w skali całego świata — czyli
//!    `Books::check_conservation` przechodzi po nim tak samo jak przed;
//! 3. dwa dry-runy tego samego ziarna dają identyczny świat (`K4`, determinizm);
//! 4. historia „na sucho" domyka Etap 10 i zostawia kronikę (WP10.3).
//!
//! `#[ignore]` z tego samego powodu co cała rodzina testów generujących świat:
//! najmniejsze miasto to kilkadziesiąt tysięcy mieszkańców. CI uruchamia je jawnie
//! przez `--include-ignored`.

use magnat_agents::{bootstrap_day, register_day};
use magnat_economy::Books;
use magnat_ecs::World;
use magnat_headless::full;
use magnat_headless::population::{swiat_agentow, zaludnij, zbuduj_miasto};
use magnat_jobs::JobPool;
use magnat_macro::{dry_run, lift, lower, state_hash, DryRunConfig, MacroParams, MacroState};

const ZIARNO: u64 = 11;

/// Miasto z gospodarką, firmami i stroną publiczną — ta sama droga, którą składa
/// je `full::setup`, żeby test nie budował piątego wariantu świata.
fn miasto(citizens: u32) -> World {
    let pool = JobPool::new(0);
    let city = zbuduj_miasto(ZIARNO, "4km", "lowland", "1990", "mixed", &pool).expect("miasto");
    let mut world = swiat_agentow(ZIARNO).expect("świat");
    register_day(&mut world);
    let zaludnione = zaludnij(&mut world, &city, citizens, 40_000).expect("Etap 8");
    full::setup(
        &mut world,
        &city,
        zaludnione.places.clone(),
        zaludnione.travel_oracle(),
        &zaludnione.traffic,
        ZIARNO,
        &pool,
    )
    .expect("gospodarka");
    bootstrap_day(&mut world, 0);
    world
}

/// Suma pieniądza świata: komponenty gospodarstw **plus** salda ksiąg. Jedno bez
/// drugiego nie jest sumą — gospodarstwo trzyma gotówkę poza `Books` (M5d).
fn pieniadz_swiata(world: &World) -> i64 {
    magnat_agents::total_money(world)
        + world
            .get_resource::<Books>()
            .map_or(0, |b| b.total_balance().get())
}

/// Dziesięć towarów o najszerszej dostępności, po trzydzieści sztuk na miesiąc.
fn koszyk_miasta(world: &World, ile: usize) -> Vec<(magnat_core::GoodId, i64)> {
    let st = lift(world);
    let mut licznik: std::collections::BTreeMap<u16, u32> = std::collections::BTreeMap::new();
    for f in &st.firms {
        for (g, cena) in f.price.iter() {
            if cena.get() > 0 {
                *licznik.entry(g.0).or_default() += 1;
            }
        }
    }
    let mut v: Vec<(u32, u16)> = licznik.into_iter().map(|(g, n)| (n, g)).collect();
    v.sort_unstable_by(|a, b| b.cmp(a));
    v.into_iter()
        .take(ile)
        .map(|(_, g)| (magnat_core::GoodId(g), 30))
        .collect()
}

fn sumy_pieniezne(st: &MacroState) -> (i64, i64, i64, i64) {
    (
        st.cells.iter().map(|c| c.cash.get()).sum(),
        st.cells.iter().map(|c| c.deposits.get()).sum(),
        st.cells.iter().map(|c| c.debt.get()).sum(),
        st.firms.iter().map(|f| f.capital.get()).sum(),
    )
}

#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn rozwiniecie_i_zdjecie_domykaja_sie_co_do_grosza() {
    let mut world = miasto(3_000);
    let st = lift(&world);
    assert!(!st.cells.is_empty(), "zdjęcie bez ani jednej komórki");

    let przed = pieniadz_swiata(&world);
    let raport = lower(&st, &mut world, ZIARNO).expect("rozwinięcie");
    let po = pieniadz_swiata(&world);
    assert_eq!(
        przed,
        po,
        "rozwinięcie zmieniło sumę pieniądza świata o {} gr",
        po - przed
    );
    assert_eq!(
        raport.unsettled,
        magnat_core::Money::ZERO,
        "kwota, której nie dało się nanieść: {:?}",
        raport.unsettled
    );
    assert!(
        raport.households > 0,
        "żadne gospodarstwo nie dostało stanu"
    );
    assert_eq!(
        raport.missing_citizens, 0,
        "makro zna ludzi, których nie ma w świecie — populacja przestała być zamknięta"
    );
    world
        .get_resource::<Books>()
        .expect("księgi")
        .check_conservation()
        .expect("P1 po rozwinięciu");

    // Zdjęcie po rozwinięciu ma dać te same sumy pieniężne co przed nim.
    let znowu = lift(&world);
    assert_eq!(
        sumy_pieniezne(&znowu),
        sumy_pieniezne(&st),
        "lift(lower(s)) rozjechało się z s na polach pieniężnych"
    );
}

#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn rozwiniecie_dwa_razy_daje_ten_sam_swiat() {
    // `K4`: `lower()` wykonany dwukrotnie z tego samego stanu → identyczny świat.
    // Nie „podobny" — identyczny co do hasha stanu ECS.
    let mut world = miasto(3_000);
    let st = lift(&world);
    lower(&st, &mut world, ZIARNO).expect("pierwsze rozwinięcie");
    let hash_a = magnat_io::world_state_hash(&world);
    lower(&st, &mut world, ZIARNO).expect("drugie rozwinięcie");
    let hash_b = magnat_io::world_state_hash(&world);
    assert_eq!(hash_a, hash_b, "drugie rozwinięcie dało inny świat");
}

#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn historia_na_sucho_zuzywa_swiat_i_domyka_etap_10() {
    // WP10.3: trzydzieści lat historii na prawdziwym mieście. Świat ma z niej
    // wyjść **inny** — inaczej dry-run byłby drogą kosztowną i bezcelową — i ma
    // przejść bramki Etapu 10 po co najwyżej trzech rundach naprawczych.
    // Miasto zaludnione **z pojemności**, nie garstką ludzi: bramka 3 Etapu 10 pyta,
    // czy każda firma ma obsadę, a miasto 4 km ma stołki dla czterdziestu tysięcy.
    // Trzy tysiące mieszkańców to nie jest małe miasto, tylko **wyludnione** — test
    // mierzyłby wtedy własny dobór danych, a nie świat, który generator stawia.
    let mut world = miasto(0);
    let przed_pieniadz = pieniadz_swiata(&world);
    let przed_hash = magnat_io::world_state_hash(&world);

    // Koszyk do bramki 8 buduje wołający — to on ma katalog towarów. Tu bierze
    // dziesięć najczęściej wystawianych towarów miasta po jednej sztuce dziennie;
    // pełny koszyk `data/economy/cpi.ron` składa scenariusz gry, nie test.
    let koszyk = koszyk_miasta(&world, 10);
    assert!(!koszyk.is_empty(), "miasto bez ani jednego towaru na półce");
    let cfg = DryRunConfig {
        seed: ZIARNO,
        years: 30,
        basket: koszyk,
        ..DryRunConfig::default()
    };
    let wynik = dry_run(&cfg, &mut world, &MacroParams::default());

    assert!(wynik.steps > 0, "historia bez ani jednego kroku");
    assert_ne!(
        magnat_io::world_state_hash(&world),
        przed_hash,
        "trzydzieści lat historii nie zmieniło świata"
    );
    assert_eq!(
        pieniadz_swiata(&world),
        przed_pieniadz,
        "historia na sucho stworzyła albo zniszczyła pieniądz"
    );
    world
        .get_resource::<Books>()
        .expect("księgi")
        .check_conservation()
        .expect("P1 po historii");

    for c in &wynik.verification.checks {
        println!(
            "bramka {} {:<28} = {:>8} [{}..{}] {}",
            c.id,
            c.key,
            c.value,
            c.lo,
            c.hi,
            if c.pass { "ok" } else { "CZERWONA" }
        );
    }
    // Raport ma mieć zdanie o **każdej** bramce, także o tej, której nie zmierzył.
    assert_eq!(
        wynik.verification.checks.len(),
        8,
        "brakuje bramki w raporcie"
    );
    for c in &wynik.verification.checks {
        assert!(
            c.measured,
            "bramka {} ({}) nie została zmierzona — bramka bez pomiaru jest deklaracją",
            c.id, c.key
        );
    }
    assert!(
        wynik.rebalance_rounds <= cfg.max_rebalance_rounds,
        "więcej rund naprawczych niż wolno"
    );
    assert!(
        !wynik.chronicle.is_empty(),
        "trzydzieści lat historii bez ani jednego wpisu kronikarskiego"
    );
    // Czego ten test **nie** twierdzi: że wszystkie bramki są zielone. Po M10a
    // cztery z ośmiu świecą na czerwono na świeżo wygenerowanym mieście 4 km
    // i przyczyna jest jedna, nazwana w `step::labor`: płaska macierz dojazdów
    // zamyka rekrutację w granicach dzielnicy, więc trzecia część firm nigdy nie
    // dostaje obsady, nic nie produkuje i rynek zjada zapas z Etapu 7. Rozstrzyga
    // to wypełnienie `CommuteMatrix` przez M4 (kontrakt M10 §6), a nie zmiana
    // progu w bramce — próg jest tu jedyną rzeczą, która mówi prawdę.
}

#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn ta_sama_historia_dwa_razy_daje_ten_sam_swiat() {
    // Determinizm całej drogi: zasiew, trzydzieści lat, naprawa i rozwinięcie.
    // To jest test, który pęka, gdy ktoś wprowadzi do kroku makro zależność
    // od kolejności iteracji albo od czasu rzeczywistego.
    let cfg = DryRunConfig {
        seed: ZIARNO,
        years: 10,
        chronicle: true,
        ..DryRunConfig::default()
    };
    let params = MacroParams::default();

    let mut a = miasto(2_000);
    let wa = dry_run(&cfg, &mut a, &params);
    let mut b = miasto(2_000);
    let wb = dry_run(&cfg, &mut b, &params);

    assert_eq!(state_hash(&wa.state), state_hash(&wb.state), "stan makro");
    assert_eq!(wa.chronicle, wb.chronicle, "kronika");
    assert_eq!(wa.steps, wb.steps, "liczba kroków");
    assert_eq!(
        magnat_io::world_state_hash(&a),
        magnat_io::world_state_hash(&b),
        "świat po rozwinięciu"
    );
}

#[test]
#[ignore = "generacja świata — CI uruchamia jawnie przez --include-ignored"]
fn krok_zmienny_daje_ten_sam_rzad_wielkosci_co_dobowy() {
    // WP10.4 / kryterium K1: krok wielodobowy jest **przyspieszeniem**, nie innym
    // modelem. Suma pieniądza ma się zgadzać co do grosza w obu przebiegach, a stan
    // końcowy ma być tego samego rzędu — nie identyczny, bo krok sześciodobowy nie
    // widzi dób pośrednich i to jest jego jawna cena (§5.7).
    let params = MacroParams::default();
    let mut world = miasto(2_000);
    let baza = lift(&world);
    let pieniadz = baza.money();

    let mut dobowy = DryRunConfig {
        seed: ZIARNO,
        years: 20,
        step_days_early: 1,
        chronicle: false,
        ..DryRunConfig::default()
    };
    let a = dry_run(&dobowy, &mut world, &params);
    assert_eq!(
        a.state.money(),
        pieniadz,
        "krok dobowy ruszył sumę pieniądza"
    );

    let mut swiat_b = miasto(2_000);
    dobowy.step_days_early = 6;
    let b = dry_run(&dobowy, &mut swiat_b, &params);
    assert_eq!(
        b.state.money(),
        pieniadz,
        "krok sześciodobowy ruszył sumę pieniądza"
    );
    // Oszczędność jest **ograniczona ogonem**: ostatnie pięć lat liczy się dobą
    // po dobie zawsze, bo to one ustawiają stan początkowy partii. Przy dwudziestu
    // latach daje to 2700 kroków wobec 7200, czyli 2,7× — zapowiadane w §5.7 „3×"
    // dotyczy ośmiudziesięciu lat, gdzie ogon jest szesnastą częścią historii,
    // a nie czwartą.
    assert!(
        b.steps * 3 < a.steps * 2,
        "krok sześciodobowy nie oszczędził kroków: {} wobec {}",
        b.steps,
        a.steps
    );

    // Zatrudnienie jest agregatem o liczebności grubo powyżej progu `MIN_CELL_POP`,
    // więc oba przebiegi mają je opisać podobnie. Pasmo jest szerokie z rozmysłu:
    // to jest test na **brak rozjazdu rzędu wielkości**, a nie na dokładność, której
    // kontrakt LOD dla makra nie obiecuje (00 §4, `K-5`).
    let (ea, eb) = (i64::from(a.state.employed()), i64::from(b.state.employed()));
    let odchylenie = (ea - eb).abs() * 1_000 / ea.max(eb).max(1);
    assert!(
        odchylenie < 300,
        "krok zmienny rozjechał zatrudnienie o {odchylenie} ‰ ({ea} wobec {eb})"
    );
}
