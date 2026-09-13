//! `DecisionReason` — wyjaśnialność egzekwowana przez kompilator (00 §K-12, §7).
//!
//! Dokument 00 §7 wymaga, żeby każda decyzja agenta i firmy zapisywała powód w formie
//! strukturalnej, a system bez wyjaśnialności nie spełniał Definition of Done swojej fazy.
//! Sam wymóg w dokumencie nie wystarczy — sprawdza go człowiek na przeglądzie i przegapia.
//! Dlatego enum jest **jeden, centralny i bez `#[non_exhaustive]`**: kod renderujący kartę
//! inspekcji robi `match` bez ramienia `_`, więc dopisanie wariantu przez M7 **łamie
//! kompilację `game/`**, dopóki ktoś nie napisze, jak ten powód pokazać graczowi.
//! To nie jest niedogodność — to jest cały mechanizm.
//!
//! ## Zasady dokładania wariantów (wiążące dla faz M1+)
//!
//! 1. **Blok dyskryminant per faza, po 100**, tak jak siatka `StreamId`: M0 0–19,
//!    M3 100–199, M4 200–299, M5 300–399, M6 400–499, M7 500–599, M8 600–699,
//!    M9 700–799, M10 800–899, M12 900–999. Jawne `= N` przy każdym wariancie,
//!    bo dyskryminanta wchodzi do zapisu gry.
//! 2. **Warianty dopisuje się na końcu swojego bloku, nigdy w środku cudzego.**
//! 3. **Nigdy nie usuwać i nie zmieniać numeracji istniejącego wariantu** — dyskryminanta
//!    jest w snapshocie i w kronikach (M9). Wariant wycofany zostaje z komentarzem.
//! 4. **Nazwa zawiera domenę** (`ShopChosen`, `ModeChosen`, `TaxRaised`) — przestrzeń
//!    nazw jest wspólna dla całej gry.
//! 5. **Ładunek: `Copy`, mały, typowane id — nigdy `String`.** Powód jest zapisywany
//!    milionami sztuk na dobę gry; tekst powstaje dopiero w warstwie prezentacji,
//!    z lokalizacją. Test pilnuje `size_of::<DecisionReason>() <= 24`.
//! 6. Wariant, którego nie da się pokazać graczowi jednym zdaniem, jest źle zaprojektowany.

use serde::{Deserialize, Serialize};

/// JEDEN enum dla całej gry. Bez `#[non_exhaustive]`. Celowo.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[repr(u16)]
pub enum DecisionReason {
    // ── M0: 0..=19 ───────────────────────────────────────────────────────────────
    /// Tylko w testach silnika i w świecie syntetycznym headless.
    /// Użycie w `sim/*` to błąd przeglądu, nie wartość domyślna.
    Unspecified = 0,
    // ── M3 — agenci: 100..=199 ───────────────────────────────────────────────────
    // NeedUrgent { need: NeedKind, level: Q } = 100,
    // ── M4 — ruch: 200..=299 ─────────────────────────────────────────────────────
    // ModeChosen { mode: TransportMode, minutes: u16 } = 200,
    // ── M5 — gospodarka detaliczna: 300..=399 ────────────────────────────────────
    // ShopChosen { shop: FirmId, dominant: UtilityKind, margin_permille: i16 } = 300,
    // ... kolejne fazy dopisują własne bloki na końcu pliku
}

impl DecisionReason {
    /// Dyskryminanta — to ona, a nie nazwa wariantu, wchodzi do snapshotu i kronik.
    #[inline]
    #[must_use]
    pub const fn discriminant(self) -> u16 {
        // Bezpieczne dla enuma z `#[repr(u16)]` bez ładunku wskaźnikowego;
        // gdy fazy dopiszą warianty z ładunkiem, `as u16` nadal zwraca dyskryminantę.
        self as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn powod_miesci_sie_w_budzecie() {
        // Limit z 00 §K-12: powód jest zapisywany milionami sztuk na dobę gry.
        // Większy ładunek chowa się za `Entity` albo indeksem do tablicy fazy.
        assert!(
            size_of::<DecisionReason>() <= 24,
            "DecisionReason urósł do {} B — patrz zasada 5 w nagłówku modułu",
            size_of::<DecisionReason>()
        );
    }

    #[test]
    fn dyskryminanty_sa_wieczne() {
        // Test strażniczy: dopisanie wariantu nie może zmienić istniejących wartości,
        // bo stare zapisy gry i kroniki M9 trzymają liczby, nie nazwy.
        assert_eq!(DecisionReason::Unspecified.discriminant(), 0);
    }
}
