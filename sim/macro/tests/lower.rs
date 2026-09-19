//! Kryteria WP10.1 dla rdzenia rozwinięcia (M10a §5.8, M10 §7.3 `K4`).
//!
//! Testy są tu, a nie przy świecie, bo `lower_cell` jest **funkcją czystą** i to
//! jest jej cała wartość: da się ją wywołać milion razy bez stawiania miasta.
//! Rozwinięcie na prawdziwym świecie sprawdza `tools/headless/tests/macro_lod.rs`.

use magnat_core::{DistrictId, Money, Tick, Q};
use magnat_macro::{
    lower_cell, CitizenSeed, ClassGrain, ClassId, MacroCell, MacroState, MacroStock,
};
use proptest::prelude::*;

fn komorka(n: usize, cash: i64, deposits: i64, debt: i64, q: [i64; 4]) -> MacroCell {
    let mut c = MacroCell::new((DistrictId(3), ClassId(1)), 2);
    c.citizens = (0..n)
        .map(|i| CitizenSeed {
            birth_index: 1_000 + i as u32,
            cell: 0,
        })
        .collect();
    c.cash = Money(cash);
    c.deposits = Money(deposits);
    c.debt = Money(debt);
    c.wealth_q = [Money(q[0]), Money(q[1]), Money(q[2]), Money(q[3])];
    c.need_sat = [Q::new(60); magnat_macro::N_NEEDS];
    c.demand = MacroStock::new();
    c
}

#[test]
fn rozwiniecie_pustej_komorki_nie_gubi_niczego() {
    // Komórka bez ludzi to poprawny stan świata (dzielnica przemysłowa bez
    // mieszkań), a nie błąd — i nie ma prawa panikować ani zgubić kwoty.
    let c = komorka(0, 0, 0, 0, [0; 4]);
    let e = lower_cell(&c, 7, Tick(0));
    assert!(e.people.is_empty());
    assert_eq!(e.cash(), Money::ZERO);
}

#[test]
fn rozwiniecie_jest_funkcja_czysta() {
    // Dwa wywołania z tą samą trójką `(komórka, ziarno, tick)` mają dać wynik
    // identyczny co do bitu. To jest wymaganie M12: tu replay i zapis mogą się
    // rozjechać, więc funkcja nie ma prawa mieć stanu ukrytego.
    let c = komorka(
        500,
        12_345_678,
        9_000_000,
        4_000_000,
        [100, 900, 5_000, 90_000],
    );
    let a = lower_cell(&c, 42, Tick(360));
    let b = lower_cell(&c, 42, Tick(360));
    assert_eq!(a, b);
    // Inne ziarno daje inny przydział, choć te same sumy — inaczej porządek
    // rangowy nie zależałby od ziarna i byłby ozdobą.
    let inne = lower_cell(&c, 43, Tick(360));
    assert_eq!(inne.cash(), a.cash());
    assert_ne!(inne, a);
}

#[test]
fn kolejnosc_w_wektorze_nie_zmienia_wyniku() {
    // Porządek rangowy jest funkcją tożsamości, nie pozycji — więc komórka
    // zapełniona w odwrotnej kolejności daje tym samym ludziom te same kwoty.
    let c = komorka(300, 7_777_777, 0, 0, [0, 1_000, 8_000, 40_000]);
    let mut odwrocona = c.clone();
    odwrocona.citizens.reverse();

    let a = lower_cell(&c, 11, Tick(90));
    let b = lower_cell(&odwrocona, 11, Tick(90));
    for osoba in &a.people {
        let odpowiednik = b
            .people
            .iter()
            .find(|p| p.birth_index == osoba.birth_index)
            .expect("ten sam zbiór tożsamości");
        assert_eq!(osoba.cash, odpowiednik.cash);
        assert_eq!(osoba.deposits, odpowiednik.deposits);
        assert_eq!(osoba.debt, odpowiednik.debt);
    }
}

#[test]
fn profil_kwantylowy_daje_rozwarstwienie() {
    // Rozkład ma mieć kształt: przy kwantylach rozciągniętych od 100 gr do 90 tys.
    // najbogatszy ma dostać wielokrotność tego, co najuboższy. Bez tego `lower()`
    // rozdawałby po równo i współczynnik Giniego świata startowego byłby zerem.
    let c = komorka(1_000, 500_000_000, 0, 0, [100, 900, 5_000, 90_000]);
    let e = lower_cell(&c, 5, Tick(0));
    let mut kwoty: Vec<i64> = e.people.iter().map(|p| p.cash.get()).collect();
    kwoty.sort_unstable();
    let najubozszy = kwoty[0].max(1);
    let najbogatszy = kwoty[kwoty.len() - 1];
    assert!(
        najbogatszy > najubozszy * 10,
        "rozwarstwienie zbyt płaskie: {najubozszy} vs {najbogatszy}"
    );
}

#[test]
fn stan_metropolii_miesci_sie_w_osmiu_megabajtach() {
    // Kryterium WP10.1: `MacroState` metropolii ≤ 8 MB, żeby dziesięć równoległych
    // „co jeśli" mieściło się w 80 MB. Liczymy **pojemności**, nie `size_of`:
    // stan jest w większości wektorami, a `size_of` widziałby same nagłówki.
    let dzielnice = 40u16;
    let klas = u32::from(ClassGrain::Classes6.classes());
    let komorek = u32::from(dzielnice) * klas;
    let mieszkancow = 400_000u32;
    let firm = 3_000u32;

    let na_komorke = std::mem::size_of::<MacroCell>() as u64
        + u64::from(mieszkancow / komorek) * std::mem::size_of::<CitizenSeed>() as u64
        // `labor` i `skill_sum`: 46 ról po 4 bajty, dwa wektory.
        + 46 * 4 * 2;
    let na_firme = std::mem::size_of::<magnat_macro::MacroFirm>() as u64
        // `price`, `cost` i `stock`: po kilkanaście towarów, para (u16, i64).
        + 3 * 16 * 16;
    let macierz = u64::from(dzielnice) * u64::from(dzielnice) * 2;

    let razem = u64::from(komorek) * na_komorke
        + u64::from(firm) * na_firme
        + macierz
        + std::mem::size_of::<MacroState>() as u64;
    assert!(
        razem <= 8 * 1024 * 1024,
        "MacroState metropolii: {razem} B wobec sufitu 8 MiB"
    );
}

#[test]
fn ziarno_agregacji_trzyma_prog_liczebnosci_na_kazdym_rozmiarze() {
    // §7.4: wszystkie siedem rozmiarów z PRD §4.1 ma dać ≥ 500 osób na komórkę.
    // Bez tego kryterium K2 przechodzi w CI na metropolii i cicho pada u gracza,
    // który wybrał małe miasto.
    let rozmiary: [(u32, u16); 7] = [
        (20_000, 10),
        (40_000, 15),
        (60_000, 20),
        (120_000, 25),
        (150_000, 30),
        (300_000, 35),
        (400_000, 40),
    ];
    for (pop, dzielnic) in rozmiary {
        let grain = magnat_macro::cell_grain(pop, dzielnic);
        let komorek = u32::from(dzielnic) * u32::from(grain.classes());
        let na_komorke = pop / komorek;
        assert!(
            na_komorke >= magnat_macro::MIN_CELL_POP,
            "{pop} osób w {dzielnic} dzielnicach: {na_komorke} na komórkę przy {grain:?}"
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1_000))]

    /// Kryterium WP10.1: rozwinięcie zachowuje każdą kwotę **co do grosza**.
    ///
    /// Tysiąc losowych komórek, trzy kwoty w każdej. Tolerancja zero — nie „mała",
    /// nie „w przybliżeniu". Gwarancji nie daje staranność, tylko `split_proportional`
    /// i jego reszta oddana pierwszej stronie (00 §2).
    #[test]
    fn rozwiniecie_zachowuje_kwote_co_do_grosza(
        n in 1usize..400,
        cash in -1_000_000_000i64..1_000_000_000,
        deposits in 0i64..5_000_000_000,
        debt in 0i64..2_000_000_000,
        q0 in 0i64..1_000,
        q1 in 0i64..50_000,
        q2 in 0i64..500_000,
        q3 in 0i64..5_000_000,
        seed in any::<u64>(),
        dzien in 0u64..40_000,
    ) {
        let c = komorka(n, cash, deposits, debt, [q0, q1, q2, q3]);
        let e = lower_cell(&c, seed, Tick(dzien));
        prop_assert_eq!(e.people.len(), n);
        prop_assert_eq!(e.cash(), Money(cash));
        prop_assert_eq!(e.deposits(), Money(deposits));
        prop_assert_eq!(e.debt(), Money(debt));
    }
}
