//! Kronika: historia sprzed partii i filtr pochodzenia (M10f WP10.15).
//!
//! Test jest jednostkowy i taki ma być: `ingest_dry_run` bierze gotową listę
//! wpisów z `sim/macro`, więc postawienie miasta niczego by tu nie sprawdziło —
//! sprawdzałoby generator. Czy historia w ogóle powstaje na prawdziwym mieście,
//! pilnuje `tools/headless/tests/m10_domkniecie.rs`.

use magnat_core::{DistrictId, SimMinute};
use magnat_game::chronicle::{
    Chronicle, ChronicleKind, ChroniclePayload, Provenance, Query, PROG_DECYMACJI,
};
use magnat_macro::{ChronicleEvent, ChronicleKind as MacroKind};

fn wpis(day: u32, year: i32, magnitude: i32, district: Option<u16>) -> ChronicleEvent {
    ChronicleEvent {
        day,
        year,
        kind: MacroKind::DistrictDecline,
        district: district.map(DistrictId),
        magnitude,
    }
}

#[test]
fn historia_wchodzi_do_kroniki_z_wlasnym_rokiem() {
    let mut k = Chronicle::default();
    k.ingest_dry_run(&[
        wpis(1_080, 1963, -220, Some(2)),
        wpis(9_000, 1985, 310, Some(5)),
    ]);

    let wszystko = k.query(&Query::default());
    assert_eq!(wszystko.len(), 2, "oba wpisy weszły");
    for e in &wszystko {
        assert_eq!(e.provenance, Provenance::DryRun);
        assert_eq!(e.kind, ChronicleKind::History);
        // Minuta świata zaczyna się od zera i nie umie liczyć wstecz, więc rok
        // jedzie w ładunku. Gdyby jechał w `at`, historia udawałaby rozgrywkę.
        assert_eq!(e.at, SimMinute(0));
    }
    let lata: Vec<i32> = wszystko
        .iter()
        .filter_map(|e| match e.payload {
            ChroniclePayload::History { year, .. } => Some(year),
            _ => None,
        })
        .collect();
    assert!(lata.contains(&1963) && lata.contains(&1985));
}

#[test]
fn filtr_pochodzenia_oddziela_historie_od_rozgrywki() {
    let mut k = Chronicle::default();
    k.ingest_dry_run(&[wpis(1_080, 1963, -220, Some(2))]);

    let tylko_gra = k.query(&Query {
        provenance: Some(Provenance::Live),
        ..Query::default()
    });
    assert!(
        tylko_gra.is_empty(),
        "w świeżej sesji nie ma jeszcze ani jednego wpisu z rozgrywki"
    );

    let tylko_historia = k.query(&Query {
        provenance: Some(Provenance::DryRun),
        ..Query::default()
    });
    assert_eq!(tylko_historia.len(), 1);
}

#[test]
fn decymacja_nie_rusza_historii() {
    let mut k = Chronicle::default();
    // Skala 50 ‰ daje ważność 5, czyli głęboko poniżej progu decymacji —
    // wpis z rozgrywki o tej wadze zniknąłby w piątym roku.
    k.ingest_dry_run(&[wpis(1_080, 1963, 50, Some(2))]);
    let waga = k.entries()[0].importance;
    assert!(
        waga < PROG_DECYMACJI,
        "test mierzy wyjątek, więc wpis musi być pod progiem (jest {waga})"
    );

    // Dziesięć lat gry: dwa razy więcej, niż wynosi okno decymacji.
    k.decimate(SimMinute(10 * 360 * magnat_core::time::MINUTES_PER_DAY));
    assert_eq!(
        k.len(),
        1,
        "historia ma `at = 0`, więc kryterium wieku skasowałoby ją co do jednego wpisu"
    );
}

#[test]
fn skala_zdarzenia_staje_sie_waznoscia() {
    let mut k = Chronicle::default();
    k.ingest_dry_run(&[
        wpis(1, 1960, -900, None),
        wpis(2, 1961, 40, None),
        wpis(3, 1962, 100_000, None),
    ]);
    let w: Vec<u8> = k.entries().iter().map(|e| e.importance).collect();
    assert_eq!(w[0], 90, "ubytek 900 ‰ waży 90");
    assert_eq!(w[1], 4, "przyrost 40 ‰ waży 4");
    assert_eq!(w[2], 100, "skala poza skalą przycina się do stu");
}
