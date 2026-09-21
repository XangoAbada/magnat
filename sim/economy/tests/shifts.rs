//! Zmiany robocze idą za rodzajem zakładu, a nie za stałą w kodzie (`R2-WP37`).
//!
//! Test padał przed naprawą: `post_offers` wpisywało do każdej oferty
//! `ShiftKind::Day`, a `hire` do każdego etatu `Employment::WEEKDAYS`, więc każda
//! zmiana pracy spłaszczała mieszkańca do godzin 8–16 od poniedziałku do piątku.
//! Po jednym pokoleniu całe miasto wychodziło z domu o tej samej godzinie — choć
//! generator miasta rozdał zmiany poprawnie.

use magnat_agents::{ShiftKind, ShiftProfile};
use magnat_firms::SiteTypeCategory;

/// Huta pracuje w nocy, biuro nie, a sklep ma obsadę weekendową. Trzy zdania
/// o mieście, które przed naprawą były nieprawdziwe po pierwszej zmianie pracy.
#[test]
fn profil_zakladu_rozdaje_rozne_zmiany() {
    let obsada = |profil: ShiftProfile, n: u32| -> Vec<(ShiftKind, u8)> {
        (0..n).map(|i| ShiftKind::schedule(profil, i)).collect()
    };

    let ciagly = obsada(ShiftProfile::Continuous, 8);
    assert!(
        ciagly.iter().any(|(s, _)| *s == ShiftKind::Night),
        "ruch ciągły bez zmiany nocnej: {ciagly:?}"
    );
    assert_eq!(
        ciagly
            .iter()
            .map(|(s, _)| *s as u8)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        4,
        "cztery brygady mają cztery różne pory"
    );

    let biuro = obsada(ShiftProfile::Office, 8);
    assert!(
        biuro.iter().all(|(s, d)| *s == ShiftKind::Day && *d == 0b001_1111),
        "biuro przestało być biurem: {biuro:?}"
    );

    // Sobota: bit 5. Handel musi mieć kogoś, kto w nią pracuje.
    let sklep = obsada(ShiftProfile::Shop, 6);
    assert!(
        sklep.iter().any(|(_, d)| d & (1 << 5) != 0),
        "sklep zamknięty w sobotę: {sklep:?}"
    );
    assert!(
        sklep.iter().any(|(_, d)| d & (1 << 6) != 0),
        "sklep zamknięty w niedzielę: {sklep:?}"
    );
}

/// Branża rodzaju zakładu wybiera profil. Bez tego odwzorowania rynek pracy nie ma
/// czym odróżnić huty od biura rachunkowego.
#[test]
fn branza_wybiera_profil() {
    assert_eq!(
        SiteTypeCategory::Retail.shift_profile(),
        ShiftProfile::Shop,
        "handel bez obsady weekendowej"
    );
    assert_eq!(
        SiteTypeCategory::Processing.shift_profile(),
        ShiftProfile::Continuous,
        "przeróbka bez ruchu ciągłego"
    );
    assert_eq!(
        SiteTypeCategory::Finance.shift_profile(),
        ShiftProfile::Office
    );
    // Każda branża ma profil — brak ramienia łamie kompilację, ale brak pokrycia
    // w teście nie, więc pokrycie sprawdza się tutaj.
    assert_eq!(SiteTypeCategory::ALL.len(), 10);
    let profile: std::collections::BTreeSet<u8> = SiteTypeCategory::ALL
        .iter()
        .map(|c| c.shift_profile() as u8)
        .collect();
    assert_eq!(profile.len(), 4, "któryś profil nie ma ani jednej branży");
}
