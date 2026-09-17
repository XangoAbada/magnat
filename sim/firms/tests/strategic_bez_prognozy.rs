//! Tier strategiczny nie widzi prognozy jako liczby (M7 §7.7 pkt 4).
//!
//! §7.7 pkt 4 żąda „testu negatywnego na regresję projektową": żadna ścieżka
//! w `ai::strategic` nie porównuje wartości z `MacroOutcome` ze stałą progową.
//! Plan zapowiadał na to lint. Lint nie jest potrzebny, bo obowiązuje coś
//! mocniejszego — **graf crate'ów**: `sim/firms` nie zależy od `sim/macro`, więc
//! `MacroOutcome` jest tutaj typem nie do wyrażenia i kodu, który by go porównał
//! z progiem, po prostu nie da się napisać.
//!
//! Ten test pilnuje tamtej zależności, a nie kodu, który by z niej wynikał. To ta
//! sama droga, którą `BD-1` zamknął asymetrię informacji: gwarancji pilnuje Cargo,
//! a nie przegląd.

use std::path::Path;

#[test]
fn firms_nie_zalezy_od_makra() {
    let manifest =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
            .expect("sim/firms/Cargo.toml");
    assert!(
        !manifest.contains("magnat-macro"),
        "sim/firms zależy od sim/macro — `MacroOutcome` stał się wyrażalny w tierze \
         strategicznym, a razem z nim próg absolutny na prognozie (M7 §7.7 pkt 4, R14)"
    );
}

#[test]
fn outlook_nie_niesie_kwoty() {
    // Drugie zabezpieczenie, tym razem po stronie kształtu: to, co firma widzi
    // z prognozy, ma być rankingiem, kierunkiem i marginesem. Gdyby ktoś dopisał
    // do `Outlook` pole pieniężne, ten test padnie — bo `Money` ma osiem bajtów,
    // a `Outlook` ich nie ma gdzie schować.
    let o = magnat_firms::Outlook {
        variants: smallvec::smallvec![magnat_firms::StrAction::KeepCourse],
        winner: None,
        trend: magnat_core::Trend::Flat,
        error_margin_bp: 600,
        at: magnat_core::Tick(0),
    };
    // Powtórzenie pól w teście jest celowe: dopisanie pola do `Outlook` **musi**
    // złamać kompilację tego pliku, żeby autor zmiany przeczytał komentarz wyżej.
    let magnat_firms::Outlook {
        variants,
        winner,
        trend,
        error_margin_bp,
        at,
    } = o;
    assert_eq!(variants.len(), 1);
    assert!(winner.is_none());
    assert_eq!(trend, magnat_core::Trend::Flat);
    assert_eq!(error_margin_bp, 600);
    assert_eq!(at.get(), 0);
}
