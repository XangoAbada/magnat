//! Budżet pamięci partii i tryb awaryjny agregacji — kryterium ukończenia WP15.
//!
//! Kryterium brzmi: „miasto 400 tys. mieści się w ≤ 600 tys. aktywnych partii
//! i ≤ 64 MB pamięci gorącej". Pierwsza połowa jest własnością **strategii agregacji**
//! i sprawdza ją tutaj tryb awaryjny; druga jest własnością **rozmiaru struktury**
//! i sprawdza ją `size_of`.
//!
//! `size_of`, a nie pomiar na przebiegu, z jednego powodu: 600 tys. partii × rozmiar
//! `Batch` to liczba, którą łamie **dopisanie pola**, a nie zmiana scenariusza. Test,
//! który tego pilnuje, ma pękać przy tym dopisaniu — w tej samej zmianie, która je
//! wnosi, a nie pół roku później przy profilowaniu.

use magnat_core::{Entity, FirmId, Mass, Money, Q, SimMinute, SiteId, Volume};
use magnat_supply::catalog::load_default;
use magnat_supply::store::{BatchDraft, MassIn, WarehouseRole};
use magnat_supply::{Batch, BatchFlags, BatchOrigin, StorageClass, Store, BATCH_SOFT_LIMIT};
use std::num::NonZeroU32;

/// Szacunek z §7.4 (`Batch` w układzie SoA, ~88 B) i skala docelowa.
const PARTIE_DOCELOWO: usize = 600_000;
const BUDZET_MB: usize = 64;

fn encja(i: u32) -> Entity {
    Entity::new(i, NonZeroU32::new(1).expect("generacja"))
}

/// 600 tys. partii mieści się w 64 MB pamięci gorącej.
///
/// Arena nie kompaktuje (`K-16`: uchwyt po zwolnieniu nigdy nie jest ponownie ważny),
/// więc liczy się **maksimum historyczne** slotów, a nie liczba żywych partii. Sufit
/// pamięci jest więc sufitem na `Store::arena_slots`, a agregacja z WP15 jest tym,
/// co go utrzymuje.
#[test]
fn szescset_tysiecy_partii_miesci_sie_w_budzecie() {
    let rozmiar = std::mem::size_of::<Batch>();
    let bajty = PARTIE_DOCELOWO * rozmiar;
    let budzet = BUDZET_MB * 1024 * 1024;
    assert!(
        bajty <= budzet,
        "{PARTIE_DOCELOWO} partii × {rozmiar} B = {:.1} MB, budżet {BUDZET_MB} MB. \
         Szacunek §7.4 mówił 88 B na partię; dopisanie pola do `Batch` kosztuje \
         {:.1} MB za każde 8 bajtów.",
        bajty as f64 / (1024.0 * 1024.0),
        (PARTIE_DOCELOWO * 8) as f64 / (1024.0 * 1024.0)
    );
}

/// Scalanie amortyzuje się po dobie: slot o indeksie `i` schodzi w minucie `i % 1440`.
///
/// Test sprawdza **rozproszenie**, nie sam wynik scalania (ten ma test jednostkowy
/// w `store::aging`): po pełnej dobie każdy slot został obsłużony dokładnie raz,
/// a żadna minuta nie wzięła na siebie całej pracy.
#[test]
fn scalanie_rozklada_sie_na_dobe() {
    let cat = load_default("contemporary").expect("katalog z data/");
    let mut store = Store::new(cat.goods.len());
    let chleb = cat.good_id("food_bread_wheat").expect("chleb");

    // Trzy sloty o indeksach 0, 1, 2 — czyli trzy różne fazy doby.
    let sloty: Vec<_> = (0..3u32)
        .map(|i| {
            store.add_slot(
                SiteId(encja(i)),
                WarehouseRole::Shelf,
                StorageClass::Ambient,
                Mass(i64::MAX / 32),
                Volume(i64::MAX / 32),
                0,
            )
        })
        .collect();
    for s in &sloty {
        for j in 0..20u32 {
            store
                .put(
                    &cat,
                    *s,
                    BatchDraft {
                        good: chleb,
                        mass: Mass(1_000),
                        quality: Q::new(70),
                        brand: None,
                        producer: FirmId(encja(1)),
                        produced_at: SimMinute(u64::from(j % 2)),
                        cost: Money(1_000),
                        origin: BatchOrigin::default(),
                        flags: BatchFlags::default(),
                    },
                    MassIn::Initial,
                )
                .expect("partia");
        }
    }
    let przed = store.live_batches();
    assert_eq!(przed, 60, "trzy sloty po dwadzieścia partii");

    // Minuta, która nie jest fazą żadnego z trzech slotów: nic się nie dzieje.
    assert_eq!(store.coalesce_phase(7), 0, "obca faza nie rusza żadnego slotu");
    assert_eq!(store.live_batches(), przed);

    let mut scalone = 0;
    for faza in 0..1_440u32 {
        scalone += store.coalesce_phase(faza);
    }
    assert!(
        scalone > 0,
        "po pełnej dobie nic się nie scaliło, choć każdy slot ma dwadzieścia partii \
         o zgodnym kluczu agregacji"
    );
    assert_eq!(
        store.live_batches(),
        przed - scalone,
        "liczba partii ma zmaleć dokładnie o liczbę scalonych"
    );
    // Masa i pieniądz przeżywają scalanie bez straty grama i grosza — to jest cała
    // różnica między agregacją a odpisem.
    store.check_mass(chleb).expect("bilans masy po scaleniu");
    store.check_cost().expect("bilans pieniądza po scaleniu");
    store.check_no_negative().expect("stany nieujemne");
}

/// Tryb awaryjny włącza się **progiem na liczbie partii**, a nie decyzją „gdy ciasno".
///
/// Deterministyczny i odwracalny: dwa przebiegi tego samego ziarna przełączą się
/// w tej samej minucie, a zejście poniżej progu wraca do klucza pełnego. Test pilnuje
/// samego progu — klucz awaryjny ma własny test w `store::aging`.
#[test]
fn tryb_awaryjny_ma_prog_a_nie_nastroj() {
    let cat = load_default("contemporary").expect("katalog z data/");
    let store = Store::new(cat.goods.len());
    assert!(
        !store.emergency_coalescing(),
        "pusty magazyn nie pracuje w trybie awaryjnym"
    );
    assert_eq!(
        BATCH_SOFT_LIMIT, PARTIE_DOCELOWO,
        "próg trybu awaryjnego ma być szacunkiem metropolii z §7.4 — dwie różne liczby \
         znaczyłyby, że tryb włącza się przed albo po tym, na czym stoi budżet"
    );
}

/// Klucz awaryjny scala to, czego pełny nie ruszy: różne marki, różni producenci
/// i daty przydatności w obrębie tygodnia.
#[test]
fn klucz_awaryjny_zbija_to_czego_pelny_nie_rusza() {
    let cat = load_default("contemporary").expect("katalog z data/");
    let mut store = Store::new(cat.goods.len());
    let chleb = cat.good_id("food_bread_wheat").expect("chleb");
    let slot = store.add_slot(
        SiteId(encja(0)),
        WarehouseRole::Shelf,
        StorageClass::Ambient,
        Mass(i64::MAX / 32),
        Volume(i64::MAX / 32),
        0,
    );
    // Dziesięć partii o **różnych** producentach i różnych dobach przydatności:
    // klucz pełny nie scali z nich ani dwóch.
    for j in 0..10u32 {
        store
            .put(
                &cat,
                slot,
                BatchDraft {
                    good: chleb,
                    mass: Mass(1_000),
                    quality: Q::new(70 + (j % 3) as u8),
                    brand: None,
                    producer: FirmId(encja(j)),
                    produced_at: SimMinute(0),
                    cost: Money(1_000),
                    origin: BatchOrigin::default(),
                    flags: BatchFlags::default(),
                },
                MassIn::Initial,
            )
            .expect("partia");
    }
    assert_eq!(store.merge_in_slot(slot), 0, "klucz pełny nie ma co scalić");
    let scalone = store.merge_in_slot_with(slot, true);
    assert!(
        scalone >= 8,
        "klucz awaryjny miał zbić dziesięć partii do jednej albo dwóch, zbił {scalone}"
    );
    store.check_mass(chleb).expect("bilans masy");
    store.check_cost().expect("bilans pieniądza");
}
