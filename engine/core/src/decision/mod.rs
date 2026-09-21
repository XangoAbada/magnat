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

use crate::ids::{FirmId, SiteId};
use crate::subject::Subject;
use crate::time::MinuteOfDay;
use crate::types::{BrandId, DistrictId, EventId, GoodId, JobRoleId, Money, PolicyId, TechId, Q};
use crate::vocab::{
    AbateReason, ActionKind, AdChannelKind, AgencyKind, BankruptcyTrigger, ClaimPriority,
    CommitmentKind, DeprivationEffect, EditorialBias, EventCategory, FirmStrategy, FixedCost,
    LeaveCause, LifeEventKind, LineStopCause, LoanKind, MigrationKind, NeedKind, PerilKind,
    PermitKind, PlaceRef, PolicyKind, PriceDriver, ReactionKind, RejectCause, RejectCredit,
    RemedyKind, ServiceKind, ShortageStageKind, SpendCategory, StockCat, TaxKind, TenderKind,
    TouchSource, TraitId, TransportMode, Trend, UtilityKind, UtilityService, VoteDriver, WageCause,
};
use serde::{Deserialize, Serialize};

mod citizen;
mod city;
mod firm;

pub use citizen::CitizenReason;
pub use city::CityReason;
pub use firm::FirmReason;

/// JEDEN enum dla całej gry — od R2e **suma trzech enumów po aktorze** (`K-58`).
///
/// Bez `#[non_exhaustive]`. Celowo, i ta decyzja się nie zmieniła: brak ramienia
/// renderującego ma łamać kompilację `game/`, a nie po cichu wyświetlać pustą kartę.
/// Zmienił się **nośnik** tej gwarancji, a nie sama gwarancja.
///
/// Powód jest w cenie, którą płaciła jedna funkcja. `describe` w `engine/ui` miała
/// 319 linii przed M6b, 566 po M7c i 1290 przy progu błędu kontroli strukturalnej
/// wynoszącym 250 — bo rosła liniowo z liczbą faz, a każda faza dokłada warianty
/// wszystkich trzech aktorów naraz. Po podziale dopisanie powodu przez fazę
/// dotykającą firm nie powiększa pliku, który obsługuje mieszkańców.
///
/// Podział idzie po **aktorze**, a nie po fazie (`D-N15`), i to jest istota: faza
/// jest własnością planu i się zmienia, aktor jest własnością domeny i nie zmienia
/// się nigdy. Powód „sklep odmówił, bo brak towaru" powstał w M5, a należy do
/// mieszkańca, który stoi przed pustą półką — i tam zostanie na zawsze.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum DecisionReason {
    /// Tylko w testach silnika i w świecie syntetycznym headless.
    /// Użycie w `sim/*` to błąd przeglądu, nie wartość domyślna.
    ///
    /// Zostaje na sumie, a nie w którymś z trzech enumów, bo nie jest niczyją
    /// decyzją — nazywa jej **brak**.
    Unspecified,
    Citizen(CitizenReason),
    Firm(FirmReason),
    City(CityReason),
}

impl DecisionReason {
    /// Dyskryminanta — to ona, a nie nazwa wariantu, wchodzi do snapshotu i kronik.
    ///
    /// Trzy enumy dzielą **jedną** przestrzeń numerów, więc suma wyłącznie deleguje.
    /// Numery są wieczne i nie drgnęły przy podziale: `Shortage` jest 400 i przed
    /// R2e, i po niej.
    #[inline]
    #[must_use]
    pub const fn discriminant(self) -> u16 {
        match self {
            DecisionReason::Unspecified => 0,
            DecisionReason::Citizen(r) => r.discriminant(),
            DecisionReason::Firm(r) => r.discriminant(),
            DecisionReason::City(r) => r.discriminant(),
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
            DecisionReason::Citizen(CitizenReason::NeedCritical {
                need: NeedKind::Sleep,
                level: Q::new(12)
            })
            .discriminant(),
            101
        );
        assert_eq!(
            DecisionReason::Citizen(CitizenReason::ChosenNearest {
                travel_min: 8,
                runner_up_min: 14
            })
            .discriminant(),
            106
        );
        assert_eq!(
            DecisionReason::Citizen(CitizenReason::Deprivation {
                need: NeedKind::Hunger,
                effect: DeprivationEffect::EnergyLoss
            })
            .discriminant(),
            112
        );
        assert_eq!(
            DecisionReason::Citizen(CitizenReason::ModeWalkOnly { minutes: 3 }).discriminant(),
            113
        );

        // Blok M3 po M3c: 100..=119 bez dziur i bez przestawień. Lista jest
        // wypisana jawnie, bo to jej **liczby** są kontraktem (M3 §6.4), a nie
        // kolejność deklaracji w pliku.
        let wszystkie = [
            DecisionReason::Unspecified,
            DecisionReason::Citizen(CitizenReason::Commitment {
                kind: CommitmentKind::Work,
            }),
            DecisionReason::Citizen(CitizenReason::NeedCritical {
                need: NeedKind::Sleep,
                level: Q::MIN,
            }),
            DecisionReason::Citizen(CitizenReason::StockBelowThreshold {
                cat: StockCat::Food,
                days_left: 1,
            }),
            DecisionReason::Citizen(CitizenReason::FreeTimePreference {
                trait_id: TraitId::Sociability,
                weight: 7,
            }),
            DecisionReason::Citizen(CitizenReason::NoTimeWindow {
                need: NeedKind::Health,
                needed_min: 45,
                longest_gap_min: 20,
            }),
            DecisionReason::Citizen(CitizenReason::SlotBudgetExhausted {
                dropped: NeedKind::Clothing,
            }),
            DecisionReason::Citizen(CitizenReason::ChosenNearest {
                travel_min: 1,
                runner_up_min: 2,
            }),
            DecisionReason::Citizen(CitizenReason::ChosenOnRoute {
                detour_min: 3,
                direct_min: 9,
            }),
            DecisionReason::Citizen(CitizenReason::PlaceUnknown {
                need: NeedKind::Hunger,
                known_count: 0,
            }),
            DecisionReason::Citizen(CitizenReason::PlaceClosed {
                place: PlaceRef::District(crate::types::DistrictId(1)),
                opens_at: MinuteOfDay::new(7 * 60),
            }),
            DecisionReason::Citizen(CitizenReason::Arrived {
                planned: MinuteOfDay::new(480),
                actual: MinuteOfDay::new(482),
            }),
            DecisionReason::Citizen(CitizenReason::Replanned {
                cause_tag: 2,
                slots_changed: 3,
            }),
            DecisionReason::Citizen(CitizenReason::Deprivation {
                need: NeedKind::Sleep,
                effect: DeprivationEffect::AbsenceRisk,
            }),
            DecisionReason::Citizen(CitizenReason::ModeWalkOnly { minutes: 3 }),
            DecisionReason::Citizen(CitizenReason::NeedSatisfied {
                need: NeedKind::Hunger,
                gain: Q::new(40),
            }),
            DecisionReason::Citizen(CitizenReason::PartnerChosen {
                compatibility: Q::new(80),
                candidates: 4,
            }),
            DecisionReason::Citizen(CitizenReason::SeparationFiled {
                stress: Q::new(70),
                years_together: 12,
            }),
            DecisionReason::Citizen(CitizenReason::MigrationDecision {
                kind: MigrationKind::Arrived,
                months_jobless: 0,
            }),
            DecisionReason::Citizen(CitizenReason::Inheritance {
                permille: 500,
                heirs: 2,
            }),
            DecisionReason::Citizen(CitizenReason::LifeEvent {
                kind: LifeEventKind::Died,
            }),
            DecisionReason::Citizen(CitizenReason::EscortUnavailable { count: 1 }),
            DecisionReason::Citizen(CitizenReason::GuardianAppointed {
                wards: 2,
                weight: 90,
                kin: true,
            }),
        ];
        let numery: Vec<u16> = wszystkie.iter().map(|r| r.discriminant()).collect();
        assert_eq!(numery[0], 0);
        assert_eq!(numery[1..], (100..=121).collect::<Vec<u16>>()[..]);
        // Blok M8 (600–699) — otwarty w M8a, wartości wieczne.
        assert_eq!(
            DecisionReason::City(CityReason::TaxAssessed {
                kind: TaxKind::Vat,
                rate_bp: 2300,
                amount: Money(1)
            })
            .discriminant(),
            600
        );
        assert_eq!(
            DecisionReason::City(CityReason::BudgetDeficitClosed {
                gap: Money(1),
                cut_bp: 100
            })
            .discriminant(),
            606
        );
        assert_eq!(
            DecisionReason::City(CityReason::GridTripped {
                service: UtilityService::Electricity,
                repair_minutes: 180
            })
            .discriminant(),
            608
        );
    }

    #[test]
    fn skrot_powodu_miesci_sie_w_bajcie() {
        // `PlanSlot.reason` (M3a §5.1) pakuje powód jako `tag(u8) | param(u8) << 8`.
        // Pakowanie jest bezstratne dla bloków M0–M4, i **tylko dla nich** — blok M5
        // zaczyna się od 300, więc do slotu nie wchodzi (patrz komentarz przy `ShopChosen`).
        // Powody M5 opisują zakup, nie slot planu, więc nic z tego nie tracimy; faza,
        // która będzie chciała włożyć do slotu powód ≥ 256, musi zmienić zapis, nie obciąć.
        assert!(
            DecisionReason::Citizen(CitizenReason::NeedSatisfied {
                need: NeedKind::Hunger,
                gain: Q::MAX
            })
            .discriminant()
                <= 255
        );
    }

    #[test]
    fn dyskryminanta_nie_zalezy_od_ladunku() {
        // Ładunek jest treścią wyjaśnienia, nie tożsamością powodu: dwa wywołania
        // tego samego wariantu muszą trafić w tę samą liczbę w zapisie gry.
        let a = DecisionReason::Citizen(CitizenReason::ModeWalkOnly { minutes: 1 });
        let b = DecisionReason::Citizen(CitizenReason::ModeWalkOnly { minutes: 999 });
        assert_eq!(a.discriminant(), b.discriminant());
        assert_ne!(a, b);
    }
}
