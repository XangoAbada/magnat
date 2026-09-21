//! Powody decyzji mieszkańca i gospodarstwa (`K-58`, `R2-WP20`).
//!
//! Mieszkaniec decyduje o swojej dobie, swoich zakupach i swoim domu. Tu
//! trafia też to, czego **doświadcza** od innych — odmowa sklepu należy do
//! tego, kto stoi przed pustą półką, a nie do sklepu, który jej nie napełnił.
//!
//! **Numery są wieczne i nie zmieniły się przy podziale** — wchodzą do hasha stanu
//! i do kronik, więc przenumerowanie przepisałoby cudzą historię. Trzy enumy dzielą
//! jedną przestrzeń numerów, a nie każdy własną; pilnuje tego test
//! `dyskryminanty_sa_wieczne`.
//!
//! **Numer stoi w [`discriminant`](CitizenReason::discriminant), a nie przy wariancie**,
//! i to jest cena podziału zapłacona świadomie. Jawna dyskryminanta przy wariancie
//! wymaga `#[repr(u16)]`, a `#[repr(u16)]` **wyłącza optymalizację niszy** — suma
//! `DecisionReason` urosła przez to z 24 B na 32 B, czyli o jedną trzecią na każdy
//! powód zapisany w dzienniku przecen, w kronice i w pierścieniu decyzji. Numery
//! nie zniknęły: przeniosły się o czterdzieści linii niżej, do jednego miejsca,
//! które ma test strażniczy — a przedtem były w **dwóch**, bo `discriminant()`
//! i tak wypisywał je po raz drugi.
//!
//! Reguła `K-12` obowiązuje tutaj **osobno**: bez `#[non_exhaustive]`, bez ramienia
//! `_` w renderze. To jest cała treść podziału — dopisanie powodu przez fazę
//! dotykającą firm nie powiększa pliku, który obsługuje mieszkańców.

use super::*;

/// Powód decyzji mieszkańca i gospodarstwa.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum CitizenReason {
    // ── M3 — agenci: 100..=199 ───────────────────────────────────────────────────
    // Pełna lista wariantów fazy jest w M3b §5.4; numeracja jest przypisana tam raz
    // i zamrożona (M3 §6.4), więc podfaza dopisuje **swój** wariant pod swoim numerem,
    // a nie kolejny wolny. Blok M3 jest po M3b kompletny: 100–114 zajęte, 115–199 wolne.
    /// Zobowiązanie stałe planu dnia — praca, szkoła, odwożenie dzieci, dojazd
    /// do jednego z nich (M3b faza 1). Slotu z tym powodem nie usuwa żadna
    /// późniejsza faza planera ani przeplanowanie przyrostowe.
    Commitment { kind: CommitmentKind },
    /// Potrzeba zeszła poniżej progu krytycznego i wymusiła slot w planie (M3b faza 2).
    NeedCritical { need: NeedKind, level: Q },
    /// Zapas gospodarstwa w tej kategorii starcza na `days_left` dni — stąd zakupy
    /// (M3b faza 3). W M3 zapas jest abstrakcyjny; M5 wstawi tu realny towar.
    StockBelowThreshold { cat: StockCat, days_left: u8 },
    /// Czas wolny wypełniony wg cechy osobowości; `weight` to waga, którą ta cecha
    /// dała wybranemu zajęciu (0 = brak preferencji, slot wypełniający).
    FreeTimePreference { trait_id: TraitId, weight: u8 },
    /// Zadanie pominięte, bo w dobie nie było dość długiej luki. `longest_gap_min`
    /// mówi, ile brakowało — bez tego karta inspekcji nie odróżnia „nie zdążył"
    /// od „nie chciał".
    NoTimeWindow {
        need: NeedKind,
        needed_min: u16,
        longest_gap_min: u16,
    },
    /// Zadanie pominięte, bo skończył się twardy limit 24 slotów planu (M3b §5.4).
    SlotBudgetExhausted { dropped: NeedKind },
    /// Wybrano najbliższe znane miejsce; `runner_up_min` mówi, o ile było gorsze drugie —
    /// bez tego karta inspekcji pokazuje wybór bez alternatywy, a PRD §5.5 chce obu.
    ChosenNearest { travel_min: u16, runner_up_min: u16 },
    /// Wybrano miejsce leżące na trasie już zaplanowanego dojazdu: nadłożenie
    /// `detour_min` minut zamiast osobnej wyprawy na `direct_min` minut.
    ChosenOnRoute { detour_min: u16, direct_min: u16 },
    /// Mieszkaniec nie zna żadnego miejsca zaspokajającego tę potrzebę (§5.7).
    /// `known_count` to liczba miejsc, które w ogóle zna — 0 czyta się inaczej niż 12.
    PlaceUnknown { need: NeedKind, known_count: u8 },
    /// Miejsce było w tej porze zamknięte; `opens_at` to najbliższe otwarcie.
    PlaceClosed {
        place: PlaceRef,
        opens_at: MinuteOfDay,
    },
    /// Mieszkaniec dotarł na miejsce — plan wobec realizacji (§14.4).
    Arrived {
        planned: MinuteOfDay,
        actual: MinuteOfDay,
    },
    /// Dzień przeplanowany. `cause_tag` to `ReplanCause::tag()` z `sim/agents`:
    /// ładunek centralnego enuma nie może pochodzić z crate'u zależnego od `core`
    /// (korekta B-1), a sam `ReplanCause` niesie `PlaceRef` i przekroczyłby limit 24 B.
    Replanned { cause_tag: u8, slots_changed: u8 },
    /// Skutek utrzymującej się deprywacji potrzeby (M3a §5.5).
    Deprivation {
        need: NeedKind,
        effect: DeprivationEffect,
    },
    /// W M3 jedynym środkiem transportu jest chodzenie — wybór nie jest wyborem.
    /// M4 dokłada `ModeChosen` (200) i ten wariant przestaje się pojawiać w świecie,
    /// ale zostaje w enumie na zawsze, bo siedzi w starych zapisach (zasada 3).
    ModeWalkOnly { minutes: u16 },
    /// Wizyta zaspokoiła potrzebę o `gain` punktów (`PlaceProvider::fulfil`).
    NeedSatisfied { need: NeedKind, gain: Q },
    /// Dobór partnera (M3c §5.6): `compatibility` to zgodność statusu i wieku
    /// w skali Q, `candidates` — ilu kandydatów było w grafie relacji. Zero kandydatów
    /// nie produkuje tego powodu; produkuje brak decyzji.
    PartnerChosen { compatibility: Q, candidates: u8 },
    /// Rozstanie (M3c §5.6). `stress` to stres bardziej zestresowanego z pary,
    /// `years_together` — długość związku w latach gry (360 dni), nasycone na 255.
    SeparationFiled { stress: Q, years_together: u8 },
    /// Gospodarstwo przyjechało albo wyjechało (M3c §5.7). `months_jobless` mówi,
    /// ile miesięcy żaden dorosły nie miał pracy — przy `Arrived` zawsze 0.
    MigrationDecision {
        kind: MigrationKind,
        months_jobless: u8,
    },
    /// Udział w spadku po zmarłym (M3c §5.6). `permille` sumuje się do 1000 między
    /// wszystkimi `heirs`; reszta z dzielenia idzie do pierwszego wg `entity_index`.
    Inheritance { permille: u16, heirs: u8 },
    /// Zdarzenie cyklu życia, które nie jest wyborem mieszkańca (M3c §5.6):
    /// narodziny, poczęcie, emerytura, choroba, wyzdrowienie, zgon.
    LifeEvent { kind: LifeEventKind },
    /// Dziecko wymagało odprowadzenia do szkoły i go nie dostało (`R2-WP3`).
    ///
    /// Dwa powody, jeden wariant: gospodarstwo bez dorosłego oraz piąte i dalsze
    /// dziecko ponad `MAX_ESCORTED`. Rozróżnienie ich kosztowałoby drugi wariant
    /// i nie zmieniłoby ani jednej decyzji — dla gracza oba znaczą „nie miał kto".
    EscortUnavailable { count: u8 },
    /// Gospodarstwo dostało opiekuna prawnego spoza składu (`R2-WP4`).
    ///
    /// `wards` — ilu podopiecznych (osieroconych dzieci albo niedołężnych seniorów),
    /// `weight` — waga relacji, która zdecydowała, `kin` — czy wybrany jest krewnym.
    /// Trzy liczby, bo pytanie gracza brzmi „dlaczego **on**", a nie „czy ktoś jest".
    GuardianAppointed { wards: u8, weight: u8, kin: bool },
    // ── M4 — ruch: 200..=299 ─────────────────────────────────────────────────────
    // M4b zajmuje 200–204. Wybór środka z pełnym kosztem uogólnionym (M4c/WP6)
    // dopisze `ModeCompared` pod kolejnym numerem — `ModeChosen` zostaje i niesie
    // to, co wiadomo zawsze: który środek i ile minut.
    /// Wybrany środek transportu i przewidywany czas przejazdu (M4b §5.2).
    ModeChosen { mode: TransportMode, minutes: u16 },
    /// Dla tego środka nie ma trasy między końcami podróży — 3,5 % węzłów sieci M2
    /// to pułapki jednokierunkowe (M4b `Y-1`). Rozstrzygnięcie zapada **przy
    /// planowaniu**: mieszkaniec dostaje inny środek, a nie porażkę przejazdu.
    NoRouteForMode {
        mode: TransportMode,
        fallback: TransportMode,
    },
    /// Poziom paliwa spadł poniżej progu i tankowanie weszło do planu dnia
    /// (M4b §5.7). `level_permille` to stan baku w promilach pojemności.
    RefuelNeeded { level_permille: u16 },
    /// Wybrana stacja paliw: nadłożenie trasy i cena, którą płaci kierowca
    /// (M4b §5.7). Remisy rozstrzyga indeks stacji, nie kolejność iteracji.
    StationChosen {
        detour_min: u16,
        price_gr_per_l: u16,
    },
    /// Przejazd trwał dłużej, niż zakładał plan — korek, spillback albo kolejka
    /// na skrzyżowaniu (M4b §5.2). To jest druga przyczyna `ReplanCause::Late`
    /// obok tej, którą M3 znał (marsz zwalniający razem z energią).
    TripDelayed { planned_min: u16, actual_min: u16 },
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
    },
    /// Opcja „samochód" odpadła, bo u celu nie ma wolnego miejsca postojowego
    /// (M4c §5.5). To jest wprost uzasadnienie z PRD §14.1: *„dlaczego Anna nie
    /// kupiła u mnie?" → „brak parkingu"*. `lots_searched` mówi, ilu parkingów
    /// szukano w promieniu dojścia.
    NoParkingAtDestination { lots_searched: u16 },
    /// Pasażer nie zmieścił się do pojazdu komunikacji i czeka na następny kurs
    /// (M4c §5.6). `waited_min` to czas spędzony na przystanku do tej chwili.
    LeftBehind { line: u16, waited_min: u16 },
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
    },
    /// Oferta odpadła (M5b §5.4). `detail` czyta się zależnie od `cause`: minuty
    /// nadmiarowego dojazdu dla `TooFar`, punkty bazowe różnicy ceny dla `PriceTooHigh`,
    /// zero dla pozostałych. To jest sztandarowy przykład karty inspekcji z PRD §14.1:
    /// *„dlaczego Anna nie kupiła u mnie"*.
    OfferRejected {
        site: SiteId,
        cause: RejectCause,
        detail: i16,
    },
    /// Zakup odłożony: najlepsza użyteczność nie przekroczyła progu (M5b §5.4).
    /// `gap_permille` to `(U_best − U_threshold) × 1000`, czyli jak bardzo zabrakło.
    PurchaseDeferred {
        need: NeedKind,
        cause: RejectCause,
        gap_permille: i16,
    },
    /// Bank udzielił kredytu (M5d §5.10). `rate_bp` to oprocentowanie roczne
    /// w punktach bazowych, `load_bp` — obciążenie, którym decyzja stanęła:
    /// DSTI dla gospodarstwa, DSCR × 100 dla zakładu. Obie liczby są tym, co karta
    /// inspekcji ma pokazać obok słowa „przyznano".
    CreditApproved {
        kind: LoanKind,
        rate_bp: i16,
        load_bp: i16,
    },
    /// Bank odmówił kredytu (M5d §5.10). `margin_bp` mówi, **o ile** zabrakło:
    /// ile punktów bazowych ponad limit wyszło DSTI, ile poniżej progu DSCR.
    /// Zero dla przyczyn, które nie są liczbą (`NoIncome`, `NoLender`).
    CreditRejected {
        kind: LoanKind,
        cause: RejectCredit,
        margin_bp: i16,
    },
    /// Gospodarstwu zabrakło na koszt stały i powstała zaległość (M5d §5.9).
    /// `gap_permille` to nieopłacona część pozycji w tysięcznych — 1000 znaczy
    /// „nie zapłacono nic". To jest ogniwo, bez którego ścieżka „debet → wniosek
    /// → odmowa → zaległość" nie daje się wyjaśnić graczowi do końca.
    BudgetShortfall { cost: FixedCost, gap_permille: i16 },
    /// Mieszkaniec zagłosował (M8e WP10, PRD §10.2, §14.1).
    ///
    /// Powód wyborcy, nie powód komisji: `driver` niesie **największy** składnik
    /// użyteczności kandydata, `margin_bp` — o ile wyprzedził drugiego w rankingu
    /// tego wyborcy. Głos oddany z przewagą 20 bp i z przewagą 4000 bp to dwie
    /// różne odpowiedzi na pytanie, czy kampania miała sens.
    VoteCast {
        candidate: u8,
        driver: VoteDriver,
        margin_bp: u16,
    },
    // 623–699 zarezerwowane dla M8.

    // ── M10: 800..=899 — głębia (`K-12`) ────────────────────────────────────────
    // 700–799 zostaje w całości wolne (blok M9 — panele niczego nie decydują, `K-71`).
    /// Mieszkaniec dowiedział się o marce (M10b WP10.5, PRD §7.6, §5.7).
    ///
    /// To jest powód, dla którego marka nie jest liczbą: karta inspekcji mówi
    /// **skąd** mieszkaniec ją zna i czego się po niej spodziewa. `source` niesie
    /// źródło (reklama, plotka, media, własne doświadczenie), `channel` — kanał,
    /// jeśli źródłem była kampania; dla plotki i doświadczenia jest `None`.
    BrandLearned {
        brand: BrandId,
        source: TouchSource,
        channel: Option<AdChannelKind>,
        awareness: Q,
    },
    /// Zakup zmienił stosunek mieszkańca do marki (M10b WP10.5, PRD §7.6).
    ///
    /// Asymetria z PRD §7.6 jest tu widoczna wprost: `expected` przeciw `actual`
    /// i wynikowa zmiana afinitetu. Rozczarowanie o 20 punktów kosztuje trzy razy
    /// więcej, niż daje zachwyt o 20 — a gracz, który przereklamował produkt,
    /// widzi w tym miejscu, że sam podniósł sobie `expected`.
    BrandExperience {
        brand: BrandId,
        expected: Q,
        actual: Q,
        delta: i16,
    },
}

impl CitizenReason {
    /// Dyskryminanta w **globalnej** przestrzeni numerów `DecisionReason`.
    ///
    /// Jawny `match`, a nie `self as u16`: numery nie zaczynają się od zera i nie są
    /// ciągłe, bo należą do bloków faz (`K-12`). Dopisanie wariantu bez wpisu tutaj
    /// łamie kompilację — ten sam mechanizm, który wymusza ramię w karcie inspekcji.
    #[inline]
    #[must_use]
    pub const fn discriminant(self) -> u16 {
        match self {
            CitizenReason::Commitment { .. } => 100,
            CitizenReason::NeedCritical { .. } => 101,
            CitizenReason::StockBelowThreshold { .. } => 102,
            CitizenReason::FreeTimePreference { .. } => 103,
            CitizenReason::NoTimeWindow { .. } => 104,
            CitizenReason::SlotBudgetExhausted { .. } => 105,
            CitizenReason::ChosenNearest { .. } => 106,
            CitizenReason::ChosenOnRoute { .. } => 107,
            CitizenReason::PlaceUnknown { .. } => 108,
            CitizenReason::PlaceClosed { .. } => 109,
            CitizenReason::Arrived { .. } => 110,
            CitizenReason::Replanned { .. } => 111,
            CitizenReason::Deprivation { .. } => 112,
            CitizenReason::ModeWalkOnly { .. } => 113,
            CitizenReason::NeedSatisfied { .. } => 114,
            CitizenReason::PartnerChosen { .. } => 115,
            CitizenReason::SeparationFiled { .. } => 116,
            CitizenReason::MigrationDecision { .. } => 117,
            CitizenReason::Inheritance { .. } => 118,
            CitizenReason::LifeEvent { .. } => 119,
            CitizenReason::EscortUnavailable { .. } => 120,
            CitizenReason::GuardianAppointed { .. } => 121,
            CitizenReason::ModeChosen { .. } => 200,
            CitizenReason::NoRouteForMode { .. } => 201,
            CitizenReason::RefuelNeeded { .. } => 202,
            CitizenReason::StationChosen { .. } => 203,
            CitizenReason::TripDelayed { .. } => 204,
            CitizenReason::ModeCompared { .. } => 205,
            CitizenReason::NoParkingAtDestination { .. } => 206,
            CitizenReason::LeftBehind { .. } => 207,
            CitizenReason::ShopChosen { .. } => 300,
            CitizenReason::OfferRejected { .. } => 301,
            CitizenReason::PurchaseDeferred { .. } => 302,
            CitizenReason::CreditApproved { .. } => 304,
            CitizenReason::CreditRejected { .. } => 305,
            CitizenReason::BudgetShortfall { .. } => 306,
            CitizenReason::VoteCast { .. } => 621,
            CitizenReason::BrandLearned { .. } => 800,
            CitizenReason::BrandExperience { .. } => 801,
        }
    }
}

impl From<CitizenReason> for DecisionReason {
    fn from(r: CitizenReason) -> DecisionReason {
        DecisionReason::Citizen(r)
    }
}
