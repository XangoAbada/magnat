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

use crate::types::Q;
use crate::vocab::{DeprivationEffect, NeedKind};
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
    // Pełna lista wariantów fazy jest w M3b §5.4; numeracja jest przypisana tam raz
    // i zamrożona (M3 §6.4), więc podfaza dopisuje **swój** wariant pod swoim numerem,
    // a nie kolejny wolny. Numery wolne w tym bloku, zajęte przez M3b i M3c:
    //   100 Commitment, 102 StockBelowThreshold, 103 FreeTimePreference,
    //   104 NoTimeWindow, 105 SlotBudgetExhausted, 107 ChosenOnRoute,
    //   109 PlaceClosed, 110 Arrived, 111 Replanned.
    /// Potrzeba zeszła poniżej progu krytycznego i wymusiła slot w planie (M3b faza 2).
    NeedCritical { need: NeedKind, level: Q } = 101,
    /// Wybrano najbliższe znane miejsce; `runner_up_min` mówi, o ile było gorsze drugie —
    /// bez tego karta inspekcji pokazuje wybór bez alternatywy, a PRD §5.5 chce obu.
    ChosenNearest { travel_min: u16, runner_up_min: u16 } = 106,
    /// Mieszkaniec nie zna żadnego miejsca zaspokajającego tę potrzebę (§5.7).
    /// `known_count` to liczba miejsc, które w ogóle zna — 0 czyta się inaczej niż 12.
    PlaceUnknown { need: NeedKind, known_count: u8 } = 108,
    /// Skutek utrzymującej się deprywacji potrzeby (M3a §5.5).
    Deprivation {
        need: NeedKind,
        effect: DeprivationEffect,
    } = 112,
    /// W M3 jedynym środkiem transportu jest chodzenie — wybór nie jest wyborem.
    /// M4 dokłada `ModeChosen` (200) i ten wariant przestaje się pojawiać w świecie,
    /// ale zostaje w enumie na zawsze, bo siedzi w starych zapisach (zasada 3).
    ModeWalkOnly { minutes: u16 } = 113,
    /// Wizyta zaspokoiła potrzebę o `gain` punktów (`PlaceProvider::fulfil`).
    NeedSatisfied { need: NeedKind, gain: Q } = 114,
    // ── M4 — ruch: 200..=299 ─────────────────────────────────────────────────────
    // ModeChosen { mode: TransportMode, minutes: u16 } = 200,
    // ── M5 — gospodarka detaliczna: 300..=399 ────────────────────────────────────
    // ShopChosen { shop: FirmId, dominant: UtilityKind, margin_permille: i16 } = 300,
    // ... kolejne fazy dopisują własne bloki na końcu pliku
}

impl DecisionReason {
    /// Dyskryminanta — to ona, a nie nazwa wariantu, wchodzi do snapshotu i kronik.
    ///
    /// Jawny `match`, a nie `self as u16`: rzutowanie działa wyłącznie dla enuma bez
    /// ładunku, a ten ładunek ma od M3. Dopisanie wariantu bez wpisu tutaj łamie
    /// kompilację — czyli dokładnie ten sam mechanizm, który wymusza ramię w karcie
    /// inspekcji.
    #[inline]
    #[must_use]
    pub const fn discriminant(self) -> u16 {
        match self {
            DecisionReason::Unspecified => 0,
            DecisionReason::NeedCritical { .. } => 101,
            DecisionReason::ChosenNearest { .. } => 106,
            DecisionReason::PlaceUnknown { .. } => 108,
            DecisionReason::Deprivation { .. } => 112,
            DecisionReason::ModeWalkOnly { .. } => 113,
            DecisionReason::NeedSatisfied { .. } => 114,
        }
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
        assert_eq!(
            DecisionReason::NeedCritical {
                need: NeedKind::Sleep,
                level: Q::new(12)
            }
            .discriminant(),
            101
        );
        assert_eq!(
            DecisionReason::ChosenNearest {
                travel_min: 8,
                runner_up_min: 14
            }
            .discriminant(),
            106
        );
        assert_eq!(
            DecisionReason::Deprivation {
                need: NeedKind::Hunger,
                effect: DeprivationEffect::EnergyLoss
            }
            .discriminant(),
            112
        );
        assert_eq!(
            DecisionReason::ModeWalkOnly { minutes: 3 }.discriminant(),
            113
        );
    }

    #[test]
    fn dyskryminanta_nie_zalezy_od_ladunku() {
        // Ładunek jest treścią wyjaśnienia, nie tożsamością powodu: dwa wywołania
        // tego samego wariantu muszą trafić w tę samą liczbę w zapisie gry.
        let a = DecisionReason::ModeWalkOnly { minutes: 1 };
        let b = DecisionReason::ModeWalkOnly { minutes: 999 };
        assert_eq!(a.discriminant(), b.discriminant());
        assert_ne!(a, b);
    }
}
