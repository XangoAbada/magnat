//! `prop_no_teleport` — kryterium ukończenia WP5 (M6 §7.3 pkt 3).
//!
//! **Partia nigdy nie zmienia lokacji bez zlecenia.** Ruch wewnątrz zakładu (magazyn →
//! linia → magazyn wyjściowy, zaplecze → półka) jest dozwolony; wszystko inne wymaga
//! ciężarówki, pociągu albo rurociągu.
//!
//! Test prowadzi **własną** księgę lokacji zamiast czytać dziennik z kodu produkcyjnego.
//! To jest różnica z zamysłem: dziennik audytu w `Store` byłby stanem, który trzeba
//! haszować, opróżniać i utrzymywać przez sto lat gry, a pytanie „czy partia się
//! teleportowała" zadaje wyłącznie ten test. Księga po stronie testu nie może też
//! skłamać razem z kodem, który sprawdza.

use magnat_core::{Entity, FirmId, GoodId, Mass, Money, SimMinute, SiteId, Volume, Q};
use magnat_supply::batch::{BatchId, BatchLocation, TransportOrderId};
use magnat_supply::store::{BatchDraft, MassIn, WarehouseRole};
use magnat_supply::transport::{
    FlatRateFreight, FreightOracle, TransportRequest, VehicleRequirements,
};
use magnat_supply::{Catalog, SlotId, StorageClass, Store, Transport, Tuning};
use proptest::prelude::*;
use std::collections::BTreeMap;
use std::num::NonZeroU32;

fn encja(i: u32) -> Entity {
    Entity::new(i, NonZeroU32::new(1).expect("generacja"))
}

/// Gdzie partia jest **z punktu widzenia zakładu**: w którym zakładzie albo w drodze.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Gdzie {
    Zaklad(u32),
    WDrodze(u32),
}

fn gdzie(store: &Store, b: BatchId) -> Option<Gdzie> {
    let batch = store.batch(b)?;
    Some(match batch.location {
        BatchLocation::Slot(s) => Gdzie::Zaklad(store.slot(s)?.site.entity().index()),
        BatchLocation::InTransit(o) => Gdzie::WDrodze(o.0),
        BatchLocation::OnLine(_) => Gdzie::Zaklad(u32::MAX),
    })
}

struct Swiat {
    cat: Catalog,
    store: Store,
    transport: Transport,
    sloty: Vec<(SiteId, SlotId)>,
    towar: GoodId,
}

fn swiat(zakladow: usize) -> Swiat {
    let cat = magnat_supply::catalog::load_default("contemporary").expect("katalog z data/");
    let mut store = Store::new(cat.goods.len());
    let mut sloty = Vec::new();
    for i in 0..zakladow {
        let site = SiteId(encja(100 + i as u32));
        let slot = store.add_slot(
            site,
            WarehouseRole::Input,
            StorageClass::Silo,
            Mass(500_000_000),
            Volume(5_000_000_000),
            0,
        );
        sloty.push((site, slot));
    }
    let towar = cat.good_id("raw_wheat").expect("raw_wheat");
    Swiat {
        cat,
        store,
        transport: Transport::new(),
        sloty,
        towar,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// Strumień losowych przewozów między czterema zakładami. Po każdej operacji każda
    /// żywa partia musi stać tam, gdzie pozwala jej na to zlecenie — a każda zmiana
    /// zakładu musi mieć zlecenie, przez które przeszła.
    #[test]
    fn prop_no_teleport(
        kroki in prop::collection::vec((0usize..4, 0usize..4, 1i64..40), 1..60)
    ) {
        let mut w = swiat(4);
        let tuning = Tuning::load_default().expect("data/tuning/supply.ron");
        let oracle = FlatRateFreight { km: 18, tuning: tuning.transport, blocked: Vec::new() };

        // Każdy zakład zaczyna z pełnym silosem.
        for (site, slot) in w.sloty.clone() {
            w.store.put(&w.cat, slot, BatchDraft {
                good: w.towar,
                mass: Mass(60_000_000),
                quality: Q::new(64),
                brand: None,
                producer: FirmId(encja(1)),
                produced_at: SimMinute(0),
                cost: Money(4_680_000),
                origin: Default::default(),
                flags: Default::default(),
            }, MassIn::Initial).expect("zapas startowy");
            prop_assert!(w.store.stock_of(slot, w.towar).0 > 0, "{site:?}");
        }

        // Księga testu: gdzie każda partia była ostatnio i którym zleceniem tam trafiła.
        let mut ksiega: BTreeMap<u64, Gdzie> = BTreeMap::new();
        let zapisz = |store: &Store, ksiega: &mut BTreeMap<u64, Gdzie>| {
            for b in store.all_handles() {
                if let Some(g) = gdzie(store, b) {
                    ksiega.insert(b.to_bits(), g);
                }
            }
        };
        zapisz(&w.store, &mut ksiega);

        for (i, (a, b, tony)) in kroki.into_iter().enumerate() {
            if a == b {
                continue;
            }
            let (nadawca, slot_a) = w.sloty[a];
            let (odbiorca, slot_b) = w.sloty[b];
            let masa = Mass(tony * 1_000_000);
            if w.store.available(slot_a, w.towar, Q::MIN).0 < masa.0 {
                continue;
            }
            let req = TransportRequest {
                from: nadawca, to: odbiorca,
                from_slot: slot_a, to_slot: slot_b,
                good: w.towar,
                mass: masa,
                requires: VehicleRequirements::for_good(&w.cat, w.towar, masa),
                ready_at: SimMinute(i as u64 * 60),
                due_at: SimMinute(i as u64 * 60 + 600),
            };
            let id = w.transport.order(&oracle, req, magnat_core::DecisionReason::Unspecified);

            // ── załadunek ───────────────────────────────────────────────────────
            let przed = ksiega.clone();
            w.transport.dispatch(
                &oracle, &mut w.store, id,
                magnat_supply::Carrier::OwnFleet(FirmId(encja(1))),
                SimMinute(i as u64 * 60),
            ).expect("załadunek");
            sprawdz(&w.store, &przed, Some(id))?;
            zapisz(&w.store, &mut ksiega);

            // ── rozładunek ──────────────────────────────────────────────────────
            let przed = ksiega.clone();
            let nieprzyjete = w.transport
                .deliver(&w.cat, &mut w.store, id, SimMinute(i as u64 * 60 + 40))
                .expect("rozładunek");
            prop_assert_eq!(nieprzyjete, Mass::ZERO, "silos ma miejsce");
            sprawdz(&w.store, &przed, Some(id))?;
            zapisz(&w.store, &mut ksiega);

            w.store.check_mass(w.towar).map_err(|(l, p)| {
                TestCaseError::fail(format!("bilans masy: {l} wobec {p}"))
            })?;
            w.store.check_no_negative().map_err(TestCaseError::fail)?;
        }
    }
}

/// Każda zmiana zakładu między dwiema migawkami musi prowadzić przez `zlecenie`.
/// Partia, która zniknęła (podział, wydanie), nie jest teleportacją — jest zużyciem,
/// i tego pilnuje osobno bilans masy.
fn sprawdz(
    store: &Store,
    przed: &BTreeMap<u64, Gdzie>,
    zlecenie: Option<TransportOrderId>,
) -> Result<(), TestCaseError> {
    for b in store.all_handles() {
        let Some(teraz) = gdzie(store, b) else {
            continue;
        };
        let Some(wczesniej) = przed.get(&b.to_bits()).copied() else {
            // Nowa partia (podział ładunku) — musi się pojawić w drodze albo w slocie,
            // ale nie w innym zakładzie niż ten, który ją wypuścił.
            continue;
        };
        if wczesniej == teraz {
            continue;
        }
        let legalne = match (wczesniej, teraz) {
            (Gdzie::Zaklad(_), Gdzie::WDrodze(o)) | (Gdzie::WDrodze(o), Gdzie::Zaklad(_)) => {
                zlecenie.is_some_and(|z| z.0 == o)
            }
            // Zakład → zakład bez przystanku „w drodze" to jest dokładnie teleportacja.
            (Gdzie::Zaklad(_), Gdzie::Zaklad(_)) => false,
            (Gdzie::WDrodze(a), Gdzie::WDrodze(c)) => a == c,
        };
        if !legalne {
            return Err(TestCaseError::fail(format!(
                "partia {} przeszła {wczesniej:?} → {teraz:?} bez zlecenia",
                b.index()
            )));
        }
    }
    Ok(())
}

/// Trasa niewykonalna jest rozstrzygana **przy planowaniu**, a nie w połowie przejazdu
/// (§6.2). Dzięki temu uzupełnianie zapasu wie od razu, że musi szukać innego dostawcy,
/// zamiast wysyłać ciężarówkę w ślepy zaułek.
#[test]
fn trasa_niewykonalna_konczy_sie_przy_planowaniu() {
    let mut w = swiat(2);
    let tuning = Tuning::load_default().expect("data/tuning/supply.ron");
    let (a, slot_a) = w.sloty[0];
    let (b, slot_b) = w.sloty[1];
    let oracle = FlatRateFreight {
        km: 18,
        tuning: tuning.transport,
        blocked: vec![(a.entity().index(), b.entity().index())],
    };
    assert!(oracle
        .quote(
            a,
            b,
            Mass(1_000_000),
            &VehicleRequirements::for_good(&w.cat, w.towar, Mass(1))
        )
        .is_none());

    let id = w.transport.order(
        &oracle,
        TransportRequest {
            from: a,
            to: b,
            from_slot: slot_a,
            to_slot: slot_b,
            good: w.towar,
            mass: Mass(1_000_000),
            requires: VehicleRequirements::for_good(&w.cat, w.towar, Mass(1_000_000)),
            ready_at: SimMinute(0),
            due_at: SimMinute(600),
        },
        magnat_core::DecisionReason::Unspecified,
    );
    assert_eq!(
        w.transport.get(id).expect("zlecenie").state,
        magnat_supply::TransportOrderState::Failed(magnat_supply::FailReason::RouteBlocked)
    );
    // Nic nie zostało załadowane, więc nic nie wisi w drodze.
    assert!(w.transport.get(id).expect("zlecenie").cargo.is_empty());
}

/// Test odwrotny: gdyby partia **naprawdę** przeskoczyła między zakładami, `sprawdz`
/// ma to złapać. Bez tego `prop_no_teleport` przechodziłby także wtedy, gdyby sprawdzał
/// pustkę — a kryterium spełnione tożsamościowo nie jest kryterium.
#[test]
fn wykrywacz_lapie_teleportacje() {
    let mut w = swiat(2);
    let (_, slot_a) = w.sloty[0];
    let (_, slot_b) = w.sloty[1];
    let id = w
        .store
        .put(
            &w.cat,
            slot_a,
            BatchDraft {
                good: w.towar,
                mass: Mass(2_000_000),
                quality: Q::new(64),
                brand: None,
                producer: FirmId(encja(1)),
                produced_at: SimMinute(0),
                cost: Money(156_000),
                origin: Default::default(),
                flags: Default::default(),
            },
            MassIn::Initial,
        )
        .expect("zapas");

    let mut przed = BTreeMap::new();
    przed.insert(id.to_bits(), gdzie(&w.store, id).expect("lokacja"));

    // Przeniesienie „na skróty": ładunek zlecenia 42, rozładowany jako zlecenie 42,
    // ale bez żadnego zlecenia w księdze — czyli dokładnie to, czego test ma pilnować.
    let ladunek = w
        .store
        .load(
            slot_a,
            w.towar,
            Mass(2_000_000),
            Q::MIN,
            TransportOrderId(42),
        )
        .expect("załadunek");
    w.store
        .unload(&w.cat, TransportOrderId(42), &ladunek, slot_b)
        .expect("rozładunek");

    assert_eq!(
        gdzie(&w.store, ladunek[0]),
        Some(Gdzie::Zaklad(w.sloty[1].0.entity().index()))
    );
    assert!(
        sprawdz(&w.store, &przed, None).is_err(),
        "zmiana zakładu bez zlecenia musi być błędem"
    );
    assert!(
        sprawdz(&w.store, &przed, Some(TransportOrderId(42))).is_err(),
        "przeskok zakład → zakład jest teleportacją nawet z numerem w papierach"
    );
}
