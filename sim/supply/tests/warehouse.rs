//! Testy własnościowe magazynu — kryterium ukończenia WP2 (M6a).
//!
//! Metodologia jest ta sama, co przy pieniądzu w M5 (`sim/economy/tests/money_conservation.rs`)
//! i nie jest kwestią gustu: **strumień jest jeden, długi i deterministyczny**, bo szuka się
//! dryfu, który kumuluje się przez historię, a historii o długości 1 nie ma jak skumulować.
//! `proptest` pilnuje obok tego rzeczy, do której nadaje się lepiej — skracania kontrprzykładu
//! przy losowym przeplocie operacji.
//!
//! Scenariusz to **sam magazyn**: bez zakładu, bez transportu, bez rynku. Taki jest zakres
//! M6a i taka jest granica, na której te niezmienniki mają się domykać już teraz.

use magnat_core::{
    Entity, GoodId, LossKind, Mass, Money, NeedCategoryId, SimMinute, SiteId, Volume, Q,
};
use magnat_supply::catalog::{GoodForm, HazardClass, NeedCategory, StorageClass};
use magnat_supply::{
    BatchDraft, BatchFlags, BatchOrigin, Catalog, MassIn, SlotId, Store, WarehouseRole,
};
use proptest::prelude::*;
use std::num::NonZeroU32;

const CHLEB: GoodId = GoodId(0);
const PIASEK: GoodId = GoodId(1);
const TOWARY: [GoodId; 2] = [CHLEB, PIASEK];

fn encja(i: u32) -> Entity {
    Entity::new(i, NonZeroU32::new(1).expect("generacja"))
}

/// Chleb psuje się po dobie, piasek nie psuje się nigdy. Dwa towary wystarczą: bilans
/// liczy się **per towar**, więc trzeci nie dokłada ani jednej ścieżki kodu.
fn katalog() -> Catalog {
    let towar = |i: u16, key: &str, zycie: Option<u32>| magnat_supply::Good {
        key: key.into(),
        id: GoodId(i),
        category: NeedCategoryId(0),
        form: GoodForm::Bulk,
        density_g_per_l: 500,
        unit_mass: Mass::ZERO,
        unit_volume: Volume::ZERO,
        shelf_life_minutes: zycie,
        storage: StorageClass::Ambient,
        hazard: HazardClass::None,
        has_quality: true,
        substitutes: Vec::new(),
        external_base_price: None,
        import_via: Vec::new(),
        disposal_cost: Money::ZERO,
    };
    Catalog::from_parts(
        vec![
            towar(0, "food_bread_wheat", Some(1440)),
            towar(1, "raw_sand", None),
        ],
        Vec::new(),
        vec![NeedCategory {
            key: "test".to_string(),
            stock_cat: None,
        }],
        Vec::new(),
    )
}

fn magazyn(cat: &Catalog) -> (Store, Vec<SlotId>) {
    let mut s = Store::new(cat.goods.len());
    let sloty = (0..3)
        .map(|i| {
            s.add_slot(
                SiteId(encja(i)),
                WarehouseRole::Backroom,
                StorageClass::Ambient,
                Mass(50_000_000),
                Volume(500_000_000),
                0,
            )
        })
        .collect();
    (s, sloty)
}

fn draft(good: GoodId, masa: i64, koszt: i64, minuta: u64) -> BatchDraft {
    BatchDraft {
        good,
        mass: Mass(masa),
        quality: Q::new(50 + (masa % 40) as u8),
        brand: None,
        producer: magnat_core::FirmId(encja(0)),
        produced_at: SimMinute(minuta),
        cost: Money(koszt),
        origin: BatchOrigin::default(),
        flags: BatchFlags::default(),
    }
}

fn sprawdz(s: &Store, krok: usize) {
    for g in TOWARY {
        if let Err((lewa, prawa)) = s.check_mass(g) {
            panic!("krok {krok}: bilans masy {g:?} rozjechany: {lewa} g wobec {prawa} g");
        }
    }
    if let Err((w_partiach, oczekiwane)) = s.check_cost() {
        panic!("krok {krok}: bilans pieniądza {w_partiach:?} wobec {oczekiwane:?}");
    }
    if let Err(e) = s.check_no_negative() {
        panic!("krok {krok}: {e}");
    }
}

/// **`prop_mass_conservation`** — `produced + imported + initial` równa się
/// `consumed + exported + Σ losses + stock`, tolerancja **0 g**, przez sto tysięcy
/// operacji. Razem z nim leci niezmiennik pieniądza: koszt nie wyparowuje przy podziale
/// partii, bo to ten sam podział, który dzieli masę.
#[test]
fn prop_mass_conservation() {
    let cat = katalog();
    let (mut s, sloty) = magazyn(&cat);
    let mut minuta = 0u64;

    for krok in 0..100_000usize {
        let good = TOWARY[krok % 2];
        let slot = sloty[krok % sloty.len()];
        minuta += 7;

        match krok % 5 {
            // Wstawienie — masa wchodzi do świata z kategorią, bo inaczej bilans nie ma
            // pozycji otwarcia.
            0 | 1 => {
                let masa = 1_000 + (krok as i64 % 9_973);
                let koszt = 17 + (krok as i64 % 1_237);
                let zrodlo = if krok % 7 == 0 {
                    MassIn::Imported
                } else {
                    MassIn::Produced
                };
                let _ = s.put(&cat, slot, draft(good, masa, koszt, minuta), zrodlo);
            }
            // Wydanie — zawsze w porządku FEFO i zawsze z podziałem kosztu.
            2 | 3 => {
                let chce = Mass(500 + (krok as i64 % 4_001));
                let dostepne = s.stock_of(slot, good);
                let bierz = Mass(chce.0.min(dostepne.0));
                if bierz.0 > 0 {
                    let r = s.reserve(slot, good, bierz, Q::MIN).expect("rezerwacja");
                    s.take(r).expect("wydanie");
                }
            }
            // Psucie i scalanie — jedno zdejmuje masę z kategorią, drugie nie rusza
            // bilansu wcale i właśnie tego pilnujemy.
            _ => {
                s.spoil(SimMinute(minuta));
                s.merge_in_slot(slot);
            }
        }

        // Pełne sprawdzenie co tysiąc kroków: niezmiennik ma się domykać **zawsze**,
        // ale sto tysięcy pełnych przebiegów po arenie kosztowałoby minuty zegarowe.
        if krok % 1_000 == 0 {
            sprawdz(&s, krok);
        }
    }
    sprawdz(&s, 100_000);

    // Strumień ma faktycznie coś zrobić — test, w którym nic się nie stało, przechodzi
    // zawsze i nie znaczy nic.
    assert!(
        s.losses(CHLEB, LossKind::Expired).0 > 0,
        "chleb nie zdążył się zepsuć — scenariusz nie dotknął ścieżki psucia"
    );
    assert!(s.total_stock(PIASEK).0 > 0, "piasek zniknął z magazynu");
}

/// **`prop_no_negative_stock`** — zapełnienie slotu w granicach pojemności, suma partii
/// równa zapisanemu zapełnieniu, żadnej partii o masie zero i żadnego martwego uchwytu
/// na liście slotu.
#[test]
fn prop_no_negative_stock() {
    let cat = katalog();
    let (mut s, sloty) = magazyn(&cat);
    // Slot ciasny: pojemność ma być realnie dotykana, inaczej test sprawdza sufit,
    // do którego nikt nie sięga.
    let ciasny = s.add_slot(
        SiteId(encja(9)),
        WarehouseRole::Shelf,
        StorageClass::Ambient,
        Mass(20_000),
        Volume(60_000),
        0,
    );
    let mut odmowy = 0;
    for krok in 0..20_000usize {
        let good = TOWARY[krok % 2];
        let slot = if krok % 3 == 0 {
            ciasny
        } else {
            sloty[krok % 3]
        };
        if s.put(
            &cat,
            slot,
            draft(good, 1_000 + (krok as i64 % 3_001), 11, krok as u64 * 3),
            MassIn::Produced,
        )
        .is_err()
        {
            odmowy += 1;
        }
        if krok % 4 == 0 {
            let dostepne = s.stock_of(slot, good);
            if dostepne.0 > 0 {
                let r = s
                    .reserve(slot, good, Mass(dostepne.0 / 2 + 1), Q::MIN)
                    .expect("rezerwacja");
                s.take(r).expect("wydanie");
            }
        }
        s.spoil(SimMinute(krok as u64 * 3));
        s.check_no_negative()
            .unwrap_or_else(|e| panic!("krok {krok}: {e}"));
    }
    assert!(
        odmowy > 0,
        "ciasny slot nigdy nie odmówił — sufit nietknięty"
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    /// Losowy przeplot operacji nie łamie ani bilansu masy, ani bilansu pieniądza.
    /// `proptest` jest tu po to, po co się nadaje: po kontrprzykład skrócony do minimum.
    #[test]
    fn przeplot_operacji_nie_lamie_bilansu(
        ops in prop::collection::vec(
            (0u8..4, 0usize..2, 1i64..50_000, 0i64..9_999),
            1..300,
        ),
    ) {
        let cat = katalog();
        let (mut s, sloty) = magazyn(&cat);
        let mut minuta = 0u64;
        for (i, (op, g, masa, koszt)) in ops.iter().enumerate() {
            let good = TOWARY[*g];
            let slot = sloty[i % sloty.len()];
            minuta += 11;
            match op {
                0 | 1 => {
                    let _ = s.put(&cat, slot, draft(good, *masa, *koszt, minuta), MassIn::Produced);
                }
                2 => {
                    let dostepne = s.stock_of(slot, good);
                    let bierz = Mass((*masa).min(dostepne.0));
                    if bierz.0 > 0 {
                        let r = s.reserve(slot, good, bierz, Q::MIN).expect("rezerwacja");
                        s.take(r).expect("wydanie");
                    }
                }
                _ => {
                    s.spoil(SimMinute(minuta));
                    s.merge_in_slot(slot);
                }
            }
            for x in TOWARY {
                prop_assert!(s.check_mass(x).is_ok(), "bilans masy po kroku {}", i);
            }
            prop_assert!(s.check_cost().is_ok(), "bilans pieniądza po kroku {}", i);
            prop_assert!(s.check_no_negative().is_ok(), "stan slotów po kroku {}", i);
        }
    }
}

/// `K-16` wymaganie 1: arena wchodzi do hasha **w kolejności indeksów**, i to ma coś
/// znaczyć. Pominięcie areny dałoby dziurę, której żaden istniejący test by nie wykrył —
/// hash dalej byłby stabilny, tylko przestałby opisywać największą strukturę danych w grze.
///
/// Stąd test w obie strony: ten sam przebieg daje ten sam odcisk, a jeden gram różnicy
/// daje inny.
#[test]
fn arena_partii_wchodzi_do_hasha_i_reaguje_na_gram() {
    fn odcisk(dodatkowy_gram: i64) -> magnat_core::StateHash {
        let cat = katalog();
        let (mut s, sloty) = magazyn(&cat);
        for i in 0..50i64 {
            let g = TOWARY[(i % 2) as usize];
            s.put(
                &cat,
                sloty[(i % 3) as usize],
                draft(g, 1_000 + i * 37 + dodatkowy_gram, 100 + i, i as u64 * 13),
                MassIn::Produced,
            )
            .expect("wstawienie");
        }
        s.spoil(SimMinute(2_000));
        let mut h = magnat_core::StateHasher::default();
        magnat_core::HashState::hash_state(&s, &mut h);
        h.finish()
    }

    assert_eq!(odcisk(0), odcisk(0), "ten sam przebieg, inny odcisk");
    assert_ne!(odcisk(0), odcisk(1), "gram różnicy nie zmienił odcisku");
}
