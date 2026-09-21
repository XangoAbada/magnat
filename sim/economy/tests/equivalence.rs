//! Skala ekwiwalentna gospodarstwa (`R2-WP11`, pozycja 23 wykazu).
//!
//! Przed naprawą konsumpcja i media liczyły się jako `na osobę × liczba osób`,
//! więc niemowlę jadło tyle co dorosły mężczyzna i zużywało tyle samo prądu.
//! Gospodarstwo z czwórką dzieci miało zapotrzebowanie **sześciu** dorosłych,
//! czyli koszyk o jedną trzecią za duży — i szło to prosto do kopert budżetowych,
//! do progu odłożenia zakupu i do CPI.

use magnat_agents::HouseholdKind;
use magnat_core::{FixedCost, Money, Qty, Tick, Q};
use magnat_economy::budget::{plan_budget, HouseholdBudget, HouseholdProfile};
use magnat_economy::wanted_qty;
use magnat_economy::EconomyData;

fn dane() -> EconomyData {
    EconomyData::load_default().expect("data/economy")
}

/// Dwoje dorosłych i czworo dzieci: `1,0 + 0,5 + 4 × 0,3 = 2,7`.
fn rodzina() -> HouseholdProfile {
    HouseholdProfile {
        kind: HouseholdKind::FamilyWithKids,
        size: 6,
        children: 4,
        thrift: Q::new(50),
        ambition: Q::new(50),
    }
}

#[test]
fn szescioosobowa_rodzina_to_dwie_i_siedem_dziesiatych_osoby() {
    let d = dane();
    assert_eq!(rodzina().equivalent_permille(&d.budget.equivalence), 2_700);
    // Samotny dorosły jest jednostką skali.
    let sam = HouseholdProfile {
        size: 1,
        children: 0,
        kind: HouseholdKind::Single,
        ..rodzina()
    };
    assert_eq!(sam.equivalent_permille(&d.budget.equivalence), 1_000);
}

/// Media liczą się od osób ekwiwalentnych, nie od głów (`D-N10`).
///
/// Przed naprawą `fixed[Utilities]` wynosiło `9 000 × 6 = 54 000 gr`.
#[test]
fn media_licza_sie_od_osob_ekwiwalentnych() {
    let d = dane();
    let p = rodzina();
    let mut b = HouseholdBudget::default();
    plan_budget(&mut b, &p, Money(600_000), Money::ZERO, &d, Tick(0));

    let na_osobe = d.budget.fixed.utilities_gr_per_person;
    assert_eq!(
        b.fixed[FixedCost::Utilities.as_index()],
        Money(na_osobe * 2_700 / 1_000),
        "media policzone od głów, a nie od osób ekwiwalentnych"
    );
    assert!(
        b.fixed[FixedCost::Utilities.as_index()] < Money(na_osobe * 6),
        "rodzina z czwórką dzieci płaci za media jak sześcioro dorosłych"
    );
}

/// Koszyk dobowy jest odpowiednio mniejszy — ten sam mnożnik co przy mediach.
#[test]
fn koszyk_dobowy_idzie_za_skala() {
    let dzienne = Qty(1_000);
    let dni = 4;
    assert_eq!(wanted_qty(dzienne, 2_700, dni), Qty(1_000 * 27 * 4 / 10));
    // Sześć głów bez skali to szósta część za dużo niż prawda o rodzinie.
    assert!(wanted_qty(dzienne, 2_700, dni) < wanted_qty(dzienne, 6_000, dni));
    // Jedna osoba ekwiwalentna to dokładnie zużycie dobowe razy liczba dób.
    assert_eq!(wanted_qty(dzienne, 1_000, dni), Qty(4_000));
}
