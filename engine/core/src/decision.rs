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

use crate::ids::SiteId;
use crate::time::MinuteOfDay;
use crate::types::Q;
use crate::vocab::{
    CommitmentKind, DeprivationEffect, LifeEventKind, MigrationKind, NeedKind, PlaceRef,
    RejectCause, StockCat, TraitId, TransportMode, UtilityKind,
};
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
    // a nie kolejny wolny. Blok M3 jest po M3b kompletny: 100–114 zajęte, 115–199 wolne.
    /// Zobowiązanie stałe planu dnia — praca, szkoła, odwożenie dzieci, dojazd
    /// do jednego z nich (M3b faza 1). Slotu z tym powodem nie usuwa żadna
    /// późniejsza faza planera ani przeplanowanie przyrostowe.
    Commitment { kind: CommitmentKind } = 100,
    /// Potrzeba zeszła poniżej progu krytycznego i wymusiła slot w planie (M3b faza 2).
    NeedCritical { need: NeedKind, level: Q } = 101,
    /// Zapas gospodarstwa w tej kategorii starcza na `days_left` dni — stąd zakupy
    /// (M3b faza 3). W M3 zapas jest abstrakcyjny; M5 wstawi tu realny towar.
    StockBelowThreshold { cat: StockCat, days_left: u8 } = 102,
    /// Czas wolny wypełniony wg cechy osobowości; `weight` to waga, którą ta cecha
    /// dała wybranemu zajęciu (0 = brak preferencji, slot wypełniający).
    FreeTimePreference { trait_id: TraitId, weight: u8 } = 103,
    /// Zadanie pominięte, bo w dobie nie było dość długiej luki. `longest_gap_min`
    /// mówi, ile brakowało — bez tego karta inspekcji nie odróżnia „nie zdążył"
    /// od „nie chciał".
    NoTimeWindow {
        need: NeedKind,
        needed_min: u16,
        longest_gap_min: u16,
    } = 104,
    /// Zadanie pominięte, bo skończył się twardy limit 24 slotów planu (M3b §5.4).
    SlotBudgetExhausted { dropped: NeedKind } = 105,
    /// Wybrano najbliższe znane miejsce; `runner_up_min` mówi, o ile było gorsze drugie —
    /// bez tego karta inspekcji pokazuje wybór bez alternatywy, a PRD §5.5 chce obu.
    ChosenNearest { travel_min: u16, runner_up_min: u16 } = 106,
    /// Wybrano miejsce leżące na trasie już zaplanowanego dojazdu: nadłożenie
    /// `detour_min` minut zamiast osobnej wyprawy na `direct_min` minut.
    ChosenOnRoute { detour_min: u16, direct_min: u16 } = 107,
    /// Mieszkaniec nie zna żadnego miejsca zaspokajającego tę potrzebę (§5.7).
    /// `known_count` to liczba miejsc, które w ogóle zna — 0 czyta się inaczej niż 12.
    PlaceUnknown { need: NeedKind, known_count: u8 } = 108,
    /// Miejsce było w tej porze zamknięte; `opens_at` to najbliższe otwarcie.
    PlaceClosed {
        place: PlaceRef,
        opens_at: MinuteOfDay,
    } = 109,
    /// Mieszkaniec dotarł na miejsce — plan wobec realizacji (§14.4).
    Arrived {
        planned: MinuteOfDay,
        actual: MinuteOfDay,
    } = 110,
    /// Dzień przeplanowany. `cause_tag` to `ReplanCause::tag()` z `sim/agents`:
    /// ładunek centralnego enuma nie może pochodzić z crate'u zależnego od `core`
    /// (korekta B-1), a sam `ReplanCause` niesie `PlaceRef` i przekroczyłby limit 24 B.
    Replanned { cause_tag: u8, slots_changed: u8 } = 111,
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
    /// Dobór partnera (M3c §5.6): `compatibility` to zgodność statusu i wieku
    /// w skali Q, `candidates` — ilu kandydatów było w grafie relacji. Zero kandydatów
    /// nie produkuje tego powodu; produkuje brak decyzji.
    PartnerChosen { compatibility: Q, candidates: u8 } = 115,
    /// Rozstanie (M3c §5.6). `stress` to stres bardziej zestresowanego z pary,
    /// `years_together` — długość związku w latach gry (360 dni), nasycone na 255.
    SeparationFiled { stress: Q, years_together: u8 } = 116,
    /// Gospodarstwo przyjechało albo wyjechało (M3c §5.7). `months_jobless` mówi,
    /// ile miesięcy żaden dorosły nie miał pracy — przy `Arrived` zawsze 0.
    MigrationDecision {
        kind: MigrationKind,
        months_jobless: u8,
    } = 117,
    /// Udział w spadku po zmarłym (M3c §5.6). `permille` sumuje się do 1000 między
    /// wszystkimi `heirs`; reszta z dzielenia idzie do pierwszego wg `entity_index`.
    Inheritance { permille: u16, heirs: u8 } = 118,
    /// Zdarzenie cyklu życia, które nie jest wyborem mieszkańca (M3c §5.6):
    /// narodziny, poczęcie, emerytura, choroba, wyzdrowienie, zgon.
    LifeEvent { kind: LifeEventKind } = 119,
    // ── M4 — ruch: 200..=299 ─────────────────────────────────────────────────────
    // M4b zajmuje 200–204. Wybór środka z pełnym kosztem uogólnionym (M4c/WP6)
    // dopisze `ModeCompared` pod kolejnym numerem — `ModeChosen` zostaje i niesie
    // to, co wiadomo zawsze: który środek i ile minut.
    /// Wybrany środek transportu i przewidywany czas przejazdu (M4b §5.2).
    ModeChosen { mode: TransportMode, minutes: u16 } = 200,
    /// Dla tego środka nie ma trasy między końcami podróży — 3,5 % węzłów sieci M2
    /// to pułapki jednokierunkowe (M4b `Y-1`). Rozstrzygnięcie zapada **przy
    /// planowaniu**: mieszkaniec dostaje inny środek, a nie porażkę przejazdu.
    NoRouteForMode { mode: TransportMode, fallback: TransportMode } = 201,
    /// Poziom paliwa spadł poniżej progu i tankowanie weszło do planu dnia
    /// (M4b §5.7). `level_permille` to stan baku w promilach pojemności.
    RefuelNeeded { level_permille: u16 } = 202,
    /// Wybrana stacja paliw: nadłożenie trasy i cena, którą płaci kierowca
    /// (M4b §5.7). Remisy rozstrzyga indeks stacji, nie kolejność iteracji.
    StationChosen {
        detour_min: u16,
        price_gr_per_l: u16,
    } = 203,
    /// Przejazd trwał dłużej, niż zakładał plan — korek, spillback albo kolejka
    /// na skrzyżowaniu (M4b §5.2). To jest druga przyczyna `ReplanCause::Late`
    /// obok tej, którą M3 znał (marsz zwalniający razem z energią).
    TripDelayed { planned_min: u16, actual_min: u16 } = 204,
    /// Środek wybrany **przez porównanie kosztu uogólnionego** wszystkich wykonalnych
    /// opcji (M4c §5.3). `runner_up` to druga najtańsza opcja, a `delta_gr` — o ile
    /// groszy była droższa; ujemna różnica jest niemożliwa i znaczyłaby błąd argminu.
    ///
    /// Pełna lista kandydatów z rozbiciem kosztu żyje w `ModeDecision.candidates`
    /// i idzie do karty inspekcji; tutaj zostaje to, co mieści się w 24 bajtach
    /// i co wchodzi do ledgera każdej podróży (zasada 5 w nagłówku modułu).
    ModeCompared {
        chosen: TransportMode,
        runner_up: TransportMode,
        delta_gr: i32,
    } = 205,
    /// Opcja „samochód" odpadła, bo u celu nie ma wolnego miejsca postojowego
    /// (M4c §5.5). To jest wprost uzasadnienie z PRD §14.1: *„dlaczego Anna nie
    /// kupiła u mnie?" → „brak parkingu"*. `lots_searched` mówi, ilu parkingów
    /// szukano w promieniu dojścia.
    NoParkingAtDestination { lots_searched: u16 } = 206,
    /// Pasażer nie zmieścił się do pojazdu komunikacji i czeka na następny kurs
    /// (M4c §5.6). `waited_min` to czas spędzony na przystanku do tej chwili.
    LeftBehind { line: u16, waited_min: u16 } = 207,
    // ── M5 — gospodarka detaliczna: 300..=399 ────────────────────────────────────
    //
    // UWAGA (M5b): dyskryminanty tego bloku **nie mieszczą się w bajcie**, więc nie
    // wolno ich pakować do `PlanSlot.reason` (M3a §5.1). To nie jest przeoczenie —
    // powody M5 opisują **zakup**, a nie slot planu dnia: powstają w chwili wizyty
    // i mieszkają w dzienniku transakcji, w pierścieniu utraconych sprzedaży i w karcie
    // inspekcji. Plan dnia nadal niesie powody M3 (`StockBelowThreshold`,
    // `ChosenNearest`, `ChosenOnRoute`), bo to one tłumaczą, **czemu w ogóle wyjście**.
    /// Mieszkaniec wybrał tę ofertę spośród kandydatów (M5b §5.4, PRD §6.4).
    /// `dominant` mówi, który człon funkcji użyteczności przeważył, a `delta_bp`
    /// o ile procent (w punktach bazowych) wybrana cena różni się od drugiej w kolejce.
    /// Ujemna `delta_bp` = wybrana była tańsza. To jest odpowiedź na „dlaczego tam".
    ShopChosen {
        site: SiteId,
        dominant: UtilityKind,
        delta_bp: i16,
    } = 300,
    /// Oferta odpadła (M5b §5.4). `detail` czyta się zależnie od `cause`: minuty
    /// nadmiarowego dojazdu dla `TooFar`, punkty bazowe różnicy ceny dla `PriceTooHigh`,
    /// zero dla pozostałych. To jest sztandarowy przykład karty inspekcji z PRD §14.1:
    /// *„dlaczego Anna nie kupiła u mnie"*.
    OfferRejected {
        site: SiteId,
        cause: RejectCause,
        detail: i16,
    } = 301,
    /// Zakup odłożony: najlepsza użyteczność nie przekroczyła progu (M5b §5.4).
    /// `gap_permille` to `(U_best − U_threshold) × 1000`, czyli jak bardzo zabrakło.
    PurchaseDeferred {
        need: NeedKind,
        cause: RejectCause,
        gap_permille: i16,
    } = 302,
    // 303–399 zarezerwowane dla M5 (`Repricing` w M5c, `CreditDecision` w M5d).
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
            DecisionReason::Commitment { .. } => 100,
            DecisionReason::NeedCritical { .. } => 101,
            DecisionReason::StockBelowThreshold { .. } => 102,
            DecisionReason::FreeTimePreference { .. } => 103,
            DecisionReason::NoTimeWindow { .. } => 104,
            DecisionReason::SlotBudgetExhausted { .. } => 105,
            DecisionReason::ChosenNearest { .. } => 106,
            DecisionReason::ChosenOnRoute { .. } => 107,
            DecisionReason::PlaceUnknown { .. } => 108,
            DecisionReason::PlaceClosed { .. } => 109,
            DecisionReason::Arrived { .. } => 110,
            DecisionReason::Replanned { .. } => 111,
            DecisionReason::Deprivation { .. } => 112,
            DecisionReason::ModeWalkOnly { .. } => 113,
            DecisionReason::NeedSatisfied { .. } => 114,
            DecisionReason::PartnerChosen { .. } => 115,
            DecisionReason::SeparationFiled { .. } => 116,
            DecisionReason::MigrationDecision { .. } => 117,
            DecisionReason::Inheritance { .. } => 118,
            DecisionReason::LifeEvent { .. } => 119,
            DecisionReason::ModeChosen { .. } => 200,
            DecisionReason::NoRouteForMode { .. } => 201,
            DecisionReason::RefuelNeeded { .. } => 202,
            DecisionReason::StationChosen { .. } => 203,
            DecisionReason::TripDelayed { .. } => 204,
            DecisionReason::ModeCompared { .. } => 205,
            DecisionReason::NoParkingAtDestination { .. } => 206,
            DecisionReason::LeftBehind { .. } => 207,
            DecisionReason::ShopChosen { .. } => 300,
            DecisionReason::OfferRejected { .. } => 301,
            DecisionReason::PurchaseDeferred { .. } => 302,
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

        // Blok M3 po M3c: 100..=119 bez dziur i bez przestawień. Lista jest
        // wypisana jawnie, bo to jej **liczby** są kontraktem (M3 §6.4), a nie
        // kolejność deklaracji w pliku.
        let wszystkie = [
            DecisionReason::Unspecified,
            DecisionReason::Commitment {
                kind: CommitmentKind::Work,
            },
            DecisionReason::NeedCritical {
                need: NeedKind::Sleep,
                level: Q::MIN,
            },
            DecisionReason::StockBelowThreshold {
                cat: StockCat::Food,
                days_left: 1,
            },
            DecisionReason::FreeTimePreference {
                trait_id: TraitId::Sociability,
                weight: 7,
            },
            DecisionReason::NoTimeWindow {
                need: NeedKind::Health,
                needed_min: 45,
                longest_gap_min: 20,
            },
            DecisionReason::SlotBudgetExhausted {
                dropped: NeedKind::Clothing,
            },
            DecisionReason::ChosenNearest {
                travel_min: 1,
                runner_up_min: 2,
            },
            DecisionReason::ChosenOnRoute {
                detour_min: 3,
                direct_min: 9,
            },
            DecisionReason::PlaceUnknown {
                need: NeedKind::Hunger,
                known_count: 0,
            },
            DecisionReason::PlaceClosed {
                place: PlaceRef::District(crate::types::DistrictId(1)),
                opens_at: MinuteOfDay::new(7 * 60),
            },
            DecisionReason::Arrived {
                planned: MinuteOfDay::new(480),
                actual: MinuteOfDay::new(482),
            },
            DecisionReason::Replanned {
                cause_tag: 2,
                slots_changed: 3,
            },
            DecisionReason::Deprivation {
                need: NeedKind::Sleep,
                effect: DeprivationEffect::AbsenceRisk,
            },
            DecisionReason::ModeWalkOnly { minutes: 3 },
            DecisionReason::NeedSatisfied {
                need: NeedKind::Hunger,
                gain: Q::new(40),
            },
            DecisionReason::PartnerChosen {
                compatibility: Q::new(80),
                candidates: 4,
            },
            DecisionReason::SeparationFiled {
                stress: Q::new(70),
                years_together: 12,
            },
            DecisionReason::MigrationDecision {
                kind: MigrationKind::Arrived,
                months_jobless: 0,
            },
            DecisionReason::Inheritance {
                permille: 500,
                heirs: 2,
            },
            DecisionReason::LifeEvent {
                kind: LifeEventKind::Died,
            },
        ];
        let numery: Vec<u16> = wszystkie.iter().map(|r| r.discriminant()).collect();
        assert_eq!(numery[0], 0);
        assert_eq!(numery[1..], (100..=119).collect::<Vec<u16>>()[..]);
    }

    #[test]
    fn skrot_powodu_miesci_sie_w_bajcie() {
        // `PlanSlot.reason` (M3a §5.1) pakuje powód jako `tag(u8) | param(u8) << 8`.
        // Pakowanie jest bezstratne dla bloków M0–M4, i **tylko dla nich** — blok M5
        // zaczyna się od 300, więc do slotu nie wchodzi (patrz komentarz przy `ShopChosen`).
        // Powody M5 opisują zakup, nie slot planu, więc nic z tego nie tracimy; faza,
        // która będzie chciała włożyć do slotu powód ≥ 256, musi zmienić zapis, nie obciąć.
        assert!(
            DecisionReason::NeedSatisfied {
                need: NeedKind::Hunger,
                gain: Q::MAX
            }
            .discriminant()
                <= 255
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
