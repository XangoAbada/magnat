//! Kontrakt punktów rozszerzenia dla M4 i M5 (M3a WP4; M3 §7.6 kryteria 7 i 8).
//!
//! Ten plik istnieje po to, żeby kryterium „wymiana `InfinitePlaces` na atrapę nie
//! wymaga zmiany ani jednej linii u wołającego" dało się **sprawdzić**, a nie tylko
//! zadeklarować. Wszystkie testy wołają tę samą funkcję `zadanie_po_drodze`, która
//! zna wyłącznie `&dyn PlaceProvider` — gdyby przeciekł do niej typ implementacji,
//! podstawienie atrapy przestałoby się kompilować.

use magnat_agents::{
    choose_place, ArrayVec, EmptyPlaces, FlakyPlaces, FulfilOutcome, FulfilRequest, InfinitePlaces,
    Knowledge, KnowledgeKind, KnowledgeView, NeedTable, PanickingPlaces, PlaceCandidate,
    PlaceEntry, PlaceProvider, PlaceTable, MAX_CANDIDATES,
};
use magnat_core::{
    BuildingId, CitizenId, DecisionReason, Entity, HouseholdId, Money, NeedKind, PlaceKind,
    PlaceRef, SimMinute, WorldCoord,
};
use std::num::NonZeroU32;
use std::sync::Arc;

fn encja(i: u32) -> Entity {
    Entity::new(i, NonZeroU32::new(1).unwrap())
}

fn budynek(i: u32) -> PlaceRef {
    PlaceRef::Building(BuildingId(encja(i)))
}

/// Dom, dwa sklepy spożywcze (bliski i dalszy) i przychodnia.
fn katalog() -> Arc<PlaceTable> {
    Arc::new(PlaceTable::build(vec![
        PlaceEntry {
            place: budynek(1),
            kind: PlaceKind::Home,
            at: WorldCoord::new(0, 0, 0),
        },
        PlaceEntry {
            place: budynek(2),
            kind: PlaceKind::Grocery,
            at: WorldCoord::new(20_000, 0, 0), // 200 m
        },
        PlaceEntry {
            place: budynek(3),
            kind: PlaceKind::Grocery,
            at: WorldCoord::new(60_000, 0, 0), // 600 m
        },
        PlaceEntry {
            place: budynek(4),
            kind: PlaceKind::Doctor,
            at: WorldCoord::new(30_000, 0, 0),
        },
    ]))
}

fn wiedza(cele: &[u32]) -> Vec<Knowledge> {
    cele.iter()
        .map(|i| Knowledge {
            target: *i,
            day: 0,
            score: 60,
            kind: KnowledgeKind::Visited as u8,
        })
        .collect()
}

/// Wycinek fazy 3 planera: „gdzie po chleb". **Jedyne, co ta funkcja wie o świecie
/// gospodarczym, to że istnieje `PlaceProvider`.** Nie ma tu ceny, towaru, magazynu
/// ani trasy — i to jest treść kryterium akceptacyjnego nr 7.
fn zadanie_po_drodze(
    places: &dyn PlaceProvider,
    need: NeedKind,
    skad: PlaceRef,
    znane: &KnowledgeView<'_>,
) -> Result<PlaceCandidate, DecisionReason> {
    let mut kandydaci: ArrayVec<PlaceCandidate, MAX_CANDIDATES> = ArrayVec::new();
    choose_place(places, need, skad, 60, znane, &mut kandydaci)
}

#[test]
fn wybor_miejsca_nie_zna_implementacji() {
    let places = katalog();
    let needs = Arc::new(NeedTable::load_default().expect("data/needs/needs.ron"));
    let nieskonczone = InfinitePlaces::new(places.clone(), needs.clone());
    let wpisy = wiedza(&[2, 3]);
    let znane = KnowledgeView::new(&wpisy);

    let wybor = zadanie_po_drodze(&nieskonczone, NeedKind::Hunger, budynek(1), &znane)
        .expect("dwa znane sklepy, a kandydatów zero");
    assert_eq!(wybor.place, budynek(2), "wybrano dalszy sklep");
    // Uzasadnienie niesie alternatywę — bez niej karta inspekcji pokazuje wybór
    // bez kontekstu, a PRD §5.5 chce obu (00 §7).
    match wybor.reason {
        DecisionReason::ChosenNearest {
            travel_min,
            runner_up_min,
        } => {
            assert_eq!(travel_min, wybor.travel_min);
            assert!(
                runner_up_min > travel_min,
                "druga opcja ({runner_up_min}) nie jest gorsza od wybranej ({travel_min})"
            );
        }
        inny => panic!("nieoczekiwany powód: {inny:?}"),
    }

    // Ta sama funkcja, inna implementacja — zero zmian po stronie wołającego.
    let pusto = zadanie_po_drodze(&EmptyPlaces, NeedKind::Hunger, budynek(1), &znane);
    assert_eq!(
        pusto,
        Err(DecisionReason::PlaceUnknown {
            need: NeedKind::Hunger,
            known_count: 2
        })
    );
}

#[test]
fn mieszkaniec_bez_wiedzy_nie_teleportuje_sie_do_sklepu() {
    // `plan_unknown_place` z §7.3: brak wiedzy → zadanie pominięte z powodem,
    // nigdy „najbliższy sklep w mieście".
    let places = katalog();
    let needs = Arc::new(NeedTable::load_default().expect("data/needs/needs.ron"));
    let nieskonczone = InfinitePlaces::new(places, needs);
    let brak: Vec<Knowledge> = Vec::new();
    let znane = KnowledgeView::new(&brak);

    let wynik = zadanie_po_drodze(&nieskonczone, NeedKind::Hunger, budynek(1), &znane);
    assert_eq!(
        wynik,
        Err(DecisionReason::PlaceUnknown {
            need: NeedKind::Hunger,
            known_count: 0
        })
    );
}

#[test]
#[should_panic(expected = "PanickingPlaces::candidates")]
fn atrapa_panikujaca_dowodzi_ze_wolanie_idzie_przez_trait() {
    // Gdyby `zadanie_po_drodze` sięgało do `InfinitePlaces` po swojemu, ta atrapa
    // nigdy by nie wystrzeliła — a kryterium WP4 byłoby spełnione tożsamościowo.
    let brak: Vec<Knowledge> = Vec::new();
    let _ = zadanie_po_drodze(
        &PanickingPlaces,
        NeedKind::Hunger,
        budynek(1),
        &KnowledgeView::new(&brak),
    );
}

#[test]
fn sciezka_odmowy_dziala_zanim_m5_bedzie_mial_czym_odmawiac() {
    // `plan_refusal_path` z §7.3: `FlakyPlaces` odmawia co trzeciej wizycie,
    // więc ścieżka `ReplanCause::PlaceRefused` jest przetestowana już w M3.
    let places = katalog();
    let needs = Arc::new(NeedTable::load_default().expect("data/needs/needs.ron"));
    let mut flaky = FlakyPlaces::new(InfinitePlaces::new(places, needs));

    let req = FulfilRequest {
        citizen: CitizenId(encja(10)),
        household: HouseholdId(encja(11)),
        need: NeedKind::Hunger,
        place: budynek(2),
        at: SimMinute(480),
        budget_hint: Money::ZERO,
    };

    let wyniki: Vec<bool> = (0..6)
        .map(|_| matches!(flaky.fulfil(&req), FulfilOutcome::Refused(_)))
        .collect();
    assert_eq!(wyniki, vec![false, false, true, false, false, true]);

    // Wizyta udana zaspokaja tyle, ile mówią dane — i mówi, dlaczego (00 §7).
    match flaky.fulfil(&req) {
        FulfilOutcome::Done {
            satisfaction,
            spent,
            duration_min,
            reason,
        } => {
            assert_eq!(
                spent,
                Money::ZERO,
                "M3 nie rusza sald — to robi M5 (decyzja 9.8)"
            );
            assert!(satisfaction.get() > 0 && duration_min > 0);
            assert!(matches!(
                reason,
                DecisionReason::NeedSatisfied {
                    need: NeedKind::Hunger,
                    ..
                }
            ));
        }
        FulfilOutcome::Refused(r) => panic!("czwarta wizyta odmówiona: {r:?}"),
    }
}

#[test]
fn walk_nie_wycieka_do_publicznego_api() {
    // Kryterium akceptacyjne nr 8 fazy (K-2): M4 ma móc zbudować `engine/nav`
    // bez rozbierania M3. Test czyta `lib.rs`, bo to jedyne miejsce, w którym
    // ten przeciek może powstać — i powstanie cicho, jednym `pub use`.
    let lib = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"))
        .expect("src/lib.rs");
    assert!(
        lib.contains("pub(crate) mod walk;"),
        "moduł `walk` przestał być prywatny — graf pieszy należy do M4 (K-2)"
    );
    let reeksporty: Vec<&str> = lib
        .lines()
        .filter(|l| l.trim_start().starts_with("pub use walk::"))
        .collect();
    assert_eq!(
        reeksporty,
        vec!["pub use walk::WalkOracle;"],
        "z modułu `walk` wychodzi coś poza implementacją traitu"
    );
}

#[test]
fn kontrakt_nie_wspomina_o_gospodarce_ani_o_grafie() {
    // `plan_no_economy_types` z §7.3, przeniesiony na moduł kontraktu: to tutaj
    // przeciek by powstał, bo to tu M5 i M4 będą kusić, żeby „na chwilę" wstawić
    // cenę do wyboru miejsca (ryzyko R1).
    const ZAKAZANE: [&str; 6] = [
        "GoodId",
        "RecipeId",
        "Offer",
        "VehicleId",
        "RoadGraph",
        "Batch",
    ];
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/places.rs"))
        .expect("src/places.rs");
    for (nr, linia) in src.lines().enumerate() {
        let kod = linia.trim_start();
        if kod.starts_with("//") {
            continue; // komentarz może o nich mówić — i mówi, właśnie po to
        }
        for z in ZAKAZANE {
            assert!(
                !kod.contains(z),
                "places.rs:{}: typ `{z}` w kontrakcie — to jest ryzyko R1",
                nr + 1
            );
        }
    }
}
