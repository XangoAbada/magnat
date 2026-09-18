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
use crate::time::MinuteOfDay;
use crate::types::{DistrictId, EventId, GoodId, JobRoleId, Money, PolicyId, Q};
use crate::vocab::{
    AbateReason, ActionKind, AgencyKind, BankruptcyTrigger, ClaimPriority, CommitmentKind,
    DeprivationEffect, EventCategory, FirmStrategy, FixedCost, LeaveCause, LifeEventKind,
    LineStopCause, LoanKind, MigrationKind, NeedKind, PermitKind, PlaceRef, PolicyKind,
    PriceDriver, ReactionKind, RejectCause, RejectCredit, RemedyKind, ServiceKind,
    ShortageStageKind, SpendCategory, StockCat, TaxKind, TenderKind, TraitId, TransportMode, Trend,
    UtilityKind, UtilityService, VoteDriver, WageCause,
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
    NoRouteForMode {
        mode: TransportMode,
        fallback: TransportMode,
    } = 201,
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
    /// Sklep zmienił cenę oferty (M5c §5.6). `driver` mówi, który człon korekty
    /// przeważył, `delta_bp` — o ile zmieniła się cena względem poprzedniej.
    /// To jest odpowiedź na pytanie gracza „czemu u konkurenta potaniało".
    Repricing {
        site: SiteId,
        good: GoodId,
        driver: PriceDriver,
        delta_bp: i16,
    } = 303,
    /// Bank udzielił kredytu (M5d §5.10). `rate_bp` to oprocentowanie roczne
    /// w punktach bazowych, `load_bp` — obciążenie, którym decyzja stanęła:
    /// DSTI dla gospodarstwa, DSCR × 100 dla zakładu. Obie liczby są tym, co karta
    /// inspekcji ma pokazać obok słowa „przyznano".
    CreditApproved {
        kind: LoanKind,
        rate_bp: i16,
        load_bp: i16,
    } = 304,
    /// Bank odmówił kredytu (M5d §5.10). `margin_bp` mówi, **o ile** zabrakło:
    /// ile punktów bazowych ponad limit wyszło DSTI, ile poniżej progu DSCR.
    /// Zero dla przyczyn, które nie są liczbą (`NoIncome`, `NoLender`).
    CreditRejected {
        kind: LoanKind,
        cause: RejectCredit,
        margin_bp: i16,
    } = 305,
    /// Gospodarstwu zabrakło na koszt stały i powstała zaległość (M5d §5.9).
    /// `gap_permille` to nieopłacona część pozycji w tysięcznych — 1000 znaczy
    /// „nie zapłacono nic". To jest ogniwo, bez którego ścieżka „debet → wniosek
    /// → odmowa → zaległość" nie daje się wyjaśnić graczowi do końca.
    BudgetShortfall { cost: FixedCost, gap_permille: i16 } = 306,
    // 307–399 zarezerwowane dla M5.

    // ── M6: 400..=499 ────────────────────────────────────────────────────────────
    /// Zakład wszedł na kolejny stopień kaskady niedoboru (M6b §5.7, PRD §8.4).
    /// `coverage_minutes` to pokrycie zapasu w minutach w chwili przejścia — liczba,
    /// którą gracz widzi w panelu łańcucha jako „zostało ci 3 h mąki".
    /// Zapisywane na **każdym** przejściu, także w dół: powrót do `Ok` też jest
    /// odpowiedzią na pytanie „co się stało z moją piekarnią".
    Shortage {
        good: GoodId,
        from: ShortageStageKind,
        to: ShortageStageKind,
        coverage_minutes: u32,
    } = 400,
    /// Linia stanęła (M6b §5.5). `line` to indeks linii w zakładzie, nie `Entity` —
    /// linia nie jest encją ECS, a karta inspekcji i tak pokazuje ją jako „linia 2".
    ProductionHalted {
        site: SiteId,
        line: u16,
        cause: LineStopCause,
    } = 401,
    /// Zakład użył substytutu zamiast brakującego wejścia (M6b §5.7, PRD §8.4).
    /// `quality_loss` to spadek jakości wyjścia w punktach skali `Q` — cena substytucji,
    /// przez którą stoi ona **przedostatnia** w kaskadzie, tuż przed postojem.
    SubstituteUsed {
        good: GoodId,
        alt: GoodId,
        quality_loss: u8,
    } = 402,
    /// Rozstrzygnięcie zapytania ofertowego na rynku spot (M6c §5.8).
    /// `saving_bp` to przewaga zwycięzcy nad drugą ofertą w punktach bazowych funkcji
    /// celu — zero znaczy „jedyna oferta", a nie „remis". Gracz pytający „dlaczego
    /// kupiłeś u nich" dostaje odpowiedź w postaci, w której da się ją sprawdzić:
    /// ilu było chętnych i o ile ten był lepszy.
    SupplierChosen {
        good: GoodId,
        seller: FirmId,
        quotes: u16,
        saving_bp: u16,
    } = 403,
    /// Podpisanie kontraktu terminowego (M6c §5.8). `months` to okres obowiązywania
    /// w miesiącach 30-dniowych (`K-1`), `indexed` odróżnia cenę stałą od takiej,
    /// która chodzi za indeksem — bo to jest różnica, o którą gracz pyta najpierw.
    ContractSigned {
        good: GoodId,
        seller: FirmId,
        months: u16,
        indexed: bool,
    } = 404,
    /// Producent wybrał eksport zamiast sprzedaży lokalnej (M6c §5.9).
    /// `premium_bp` to przewaga ceny eksportowej **po odjęciu transportu do węzła**
    /// nad najlepszą ceną lokalną. Drenaż podaży jest emergentny, więc powód musi
    /// nieść liczbę, z której wynikł — inaczej wzrost cen w mieście wygląda na błąd.
    ExportChosen {
        good: GoodId,
        premium_bp: u16,
        mass_kg: u32,
    } = 405,
    // 406–499 zarezerwowane dla M6.

    // ── M7: 500..=599 ────────────────────────────────────────────────────────────
    /// Firma wybrała kandydata (M7b §5.5, PRD §6.6). `score` to wynik scoringu
    /// zatrudnionego, `runner_up` — drugiego w kolejce; `i32::MIN` znaczy „nie było
    /// drugiego", a nie „drugi był fatalny". Dwie liczby zamiast jednej, bo pytanie
    /// gracza brzmi „dlaczego **on**", a nie „czy był dobry": różnica między pierwszym
    /// a drugim jest całą odpowiedzią i bez niej powód byłby oceną bez skali.
    Hired {
        role: JobRoleId,
        score: i32,
        runner_up: i32,
    } = 500,
    /// Firma ruszyła stawkę w wiszącej ofercie (M7b §5.5, PRD §6.6).
    /// `delta_bp` to przyrost wobec stawki poprzedniej w punktach bazowych,
    /// `days_open` — ile dni oferta wisiała bez akceptowalnego kandydata.
    ///
    /// **`delta_bp == 0` przy `cause: Ceiling` jest wpisem pełnoprawnym**: znaczy
    /// „dalej nie licytuję, bo przy wyższej stawce ten etat przestaje się opłacać".
    /// Nieobsadzony wakat jest poprawnym wynikiem (M7 §7.1 pkt 4) i musi mieć zdanie,
    /// którym da się go graczowi wytłumaczyć.
    WageRaise {
        role: JobRoleId,
        delta_bp: u16,
        days_open: u16,
        cause: WageCause,
    } = 501,
    /// Pracownik przestał pracować w tym zakładzie (M7b WP6). Powód jest zapisywany
    /// **po stronie odchodzącego** — także wtedy, gdy odejście jest zwolnieniem.
    /// `tenure_days` to staż w dobach: rotacja tygodniowa i rotacja po pięciu latach
    /// to dwie różne diagnozy tego samego zdarzenia.
    JobLeft {
        role: JobRoleId,
        cause: LeaveCause,
        tenure_days: u16,
    } = 502,
    /// Reguła polityki wyzwoliła się i firma wykonała jej akcję (M7c WP6b, `K-11`).
    ///
    /// **Ten sam wariant dla gracza i dla AI** — to jest cała treść `sim/policy`:
    /// reguła z edytora M9 i reguła wygenerowana przez tier taktyczny M7e wykonują się
    /// tym samym kodem i zapisują ten sam powód. Gdyby były dwa warianty, różnica
    /// wróciłaby tylnymi drzwiami w karcie inspekcji.
    ///
    /// `rule` to indeks reguły w polityce (0..=7 — twardy limit ośmiu reguł z M9d §5.6),
    /// `action` — rodzaj wykonanej akcji. Pełnych wejść reguły tu nie ma i być nie może:
    /// `Metric` mieszka w `sim/policy`, a `core` od niego nie zależy. Odtwarza je na
    /// żądanie `magnat_policy::inputs` przy otwarciu karty — dlatego ta funkcja istnieje.
    ///
    /// **Dwa pola dokłada M9d WP9 i one wejść nie zastępują.** `lag_days` to wiek obrazu
    /// konkurencji, na którym menedżer pracował, `deviation_bp` — o ile punktów bazowych
    /// spudłował wobec wartości, którą reguła wyliczyła. Jedno i drugie da się poznać
    /// **wyłącznie w chwili wykonania**: dzień później obraz konkurencji jest już inny,
    /// a rzut menedżera nie zostawia po sobie śladu nigdzie indziej. Bez nich zdanie
    /// „cel 6,38 zł, menedżer ustawił 6,44 zł" z §5.6 nie miałoby z czego powstać.
    /// Zero w obu znaczy „wykonano dokładnie i na świeżych danych".
    PolicyApplied {
        policy: PolicyId,
        rule: u8,
        action: ActionKind,
        lag_days: u8,
        deviation_bp: i16,
    } = 503,
    /// Zakład dostał menedżera albo go stracił (M7c WP7, PRD §7.5).
    ///
    /// `prev` to jakość zarządzania **sprzed** zmiany, `skill_mgmt` — umiejętność
    /// przychodzącego menedżera. Odejście menedżera zapisuje się tym samym wariantem
    /// z `skill_mgmt` zastępstwa, bo dla gracza „przyszedł nowy" i „został po nim
    /// zastępca" to odpowiedź na to samo pytanie: dlaczego zakład nagle produkuje inaczej.
    ManagerAssigned {
        site: SiteId,
        skill_mgmt: Q,
        prev: u8,
    } = 504,
    /// Firma uruchomiła instrument dłużny (M7d WP8, PRD §7.8).
    ///
    /// Jeden wariant na kredyt obrotowy i inwestycyjny, bo `LoanKind` już je rozróżnia
    /// — drugi wariant powielałby słownik, który po to powstał. `rate_bp` to stopa
    /// roczna z decyzji banku, `term_months` — długość harmonogramu; razem odpowiadają
    /// na pytanie „ile mnie to kosztuje i jak długo", czyli na to, które gracz zadaje.
    LoanTaken {
        kind: LoanKind,
        rate_bp: i16,
        term_months: u16,
    } = 505,
    /// Firma podpisała leasing (M7d WP8). `months` to okres do wykupu.
    ///
    /// Osobno od [`DecisionReason::LoanTaken`], bo leasing **nie tworzy pieniądza**
    /// i nie daje aktywa: rzecz należy do leasingodawcy aż do wykupu i w upadłości
    /// do masy nie wchodzi. To jest różnica, którą karta inspekcji ma pokazać, zanim
    /// gracz policzy na nią majątek firmy.
    LeaseSigned { site: SiteId, months: u16 } = 506,
    /// Firma sprzedała należności z dyskontem (M7d WP8, faktoring).
    ///
    /// `count` to liczba sprzedanych pozycji, `discount_bp` — marża faktora.
    /// Dla obserwatora jest to typowy sygnał kłopotów z płynnością i dlatego ma
    /// własny wariant: „wzięli kredyt" i „sprzedali należności" to dwie różne
    /// diagnozy tej samej firmy.
    ReceivablesFactored { count: u16, discount_bp: u16 } = 507,
    /// Firma wypuściła obligacje (M7d WP8). `coupon_bp` to kupon roczny,
    /// `months` — czas do wykupu. Nabywcami są mieszkańcy z oszczędnościami
    /// i inne firmy; rynek wtórny należy do M10.
    BondIssued { coupon_bp: u16, months: u16 } = 508,
    /// Otwarto postępowanie upadłościowe (M7d WP9, `K-10`).
    ///
    /// `days` ma znaczenie tylko przy `BankruptcyTrigger::Illiquid` (ile dób firma
    /// nie płaciła wymagalnych zobowiązań) i przy pozostałych wyzwalaczach jest zerem
    /// — liczba stoi obok słownika, a nie w nim, żeby histogram przyczyn upadłości
    /// liczył przyczyny, a nie pary (przyczyna, długość).
    BankruptcyOpened {
        trigger: BankruptcyTrigger,
        days: u16,
    } = 509,
    /// Syndyk zaspokoił roszczenia jednego priorytetu (M7d WP9).
    ///
    /// `ratio_bp` to stopień zaspokojenia w punktach bazowych — 10 000 znaczy
    /// „w całości", 0 „nie starczyło na nic". To jest liczba, której szuka
    /// i pracownik, i bank, i dostawca, więc powód niesie ją zamiast kwoty:
    /// kwota jest indywidualna, stopień zaspokojenia dotyczy całego priorytetu.
    ClaimSettled {
        priority: ClaimPriority,
        ratio_bp: u16,
    } = 510,
    /// Tier operacyjny ustawił cel marży na towarze (M7e WP11, PRD §12.3).
    ///
    /// Nie mylić z `Repricing` (M5c): tamten mówi, **która korekta przeważyła**
    /// przy składaniu dzisiejszej ceny, ten — że firma zmieniła cel, wokół którego
    /// cena się składa. Pierwsze zdarza się codziennie, drugie raz na kilka tygodni,
    /// i gracz pyta o nie osobno („czemu dziś taniej" vs. „czemu on zszedł z marży").
    MarginTargetSet {
        good: GoodId,
        margin_bp: i32,
        prev_bp: i32,
    } = 511,
    /// Tier operacyjny ustawił cel zapasu na towarze, w dobach sprzedaży (M7e WP11).
    ///
    /// `days` to pokrycie, do którego firma chce zamawiać; `prev` — poprzednie.
    /// Zapas jest drugą dźwignią tieru operacyjnego obok ceny i musi mieć własny
    /// powód, bo „stoi pusty" i „stoi pełny" to dwa różne błędy tej samej firmy.
    RestockTargetSet { good: GoodId, days: u16, prev: u16 } = 512,
    /// Tier taktyczny zamknął zakład (M7e WP12, PRD §12.3).
    ///
    /// `months` to długość nieprzerwanej straty, `margin_bp` — marża ostatniego
    /// miesiąca (ujemna). Dwie liczby zamiast jednej z tego samego powodu co przy
    /// `Hired`: pytanie gracza brzmi „dlaczego **ten**", a odpowiedź „bo od trzech
    /// miesięcy traci 8%" niesie i skalę, i czas.
    SiteClosed { months: u8, margin_bp: i32 } = 513,
    /// Tier taktyczny zmienił kurs firmy i przypiął do niego preset polityki
    /// (M7e WP12, PRD §12.1).
    ///
    /// **To jest źródło polityk firm AI.** Do M7e menedżer dostawał delegację
    /// z polityką pustą, bo zestaw reguł miał generować tier taktyczny, a tieru
    /// nie było (`AZ-1`). Reguła gracza i reguła stąd wykonują się tym samym
    /// ewaluatorem — różnica jest w tym, kto ją napisał (`K-11`).
    StrategySet {
        strategy: FirmStrategy,
        prev: FirmStrategy,
    } = 514,
    /// Firma odpowiedziała na utratę udziału w rynku (M7e WP14, PRD §12.2).
    ///
    /// `target` to **zakład** rywala, nie jego firma — także wtedy, gdy rywalem jest
    /// gracz. Zakład, bo to on jest widoczny z ulicy i to jego cena stoi w tablicy
    /// publicznej; firmę znajdzie się z niego jednym odczytem rejestru, a w drugą
    /// stronę nie da się wcale (`K-46`: `FirmId` sklepu pochodzi z generatora miasta,
    /// `FirmKey` z rejestru firm, i nie są tą samą liczbą).
    ///
    /// `depth_bp` znaczy co innego w każdym wariancie `kind` i to jest zamierzone:
    /// przy wojnie cenowej to zejście z ceny, przy wyłączności premia dla dostawcy,
    /// przy przeciąganiu ludzi — nadpłata ponad stawkę rywala. Jedna liczba, bo we
    /// wszystkich trzech odpowiada na to samo pytanie gracza: „ile go to kosztuje".
    CompetitiveResponse {
        kind: ReactionKind,
        target: SiteId,
        depth_bp: u16,
    } = 515,
    /// Tier strategiczny otwiera zakład (M7f WP13, PRD §12.3).
    ///
    /// **Nie ma tu kwoty i nie będzie.** Decyzja stoi na porównaniu wariantów
    /// w modelu makro, a ten deklaruje własny błąd 3–12 % — kwota z niego byłaby
    /// fałszywą precyzją, w którą gracz uwierzy i na której zbuduje plan (`R15`).
    /// Dlatego powód niesie **to, z czego konkurent wybierał**: ile wariantów
    /// porównał, jak szeroki był margines i w którą stronę szedł wynik. Gracz widzi,
    /// że rywal wybrał A nad B i o ile pewnie, a nie że „wyliczył 240 tys.".
    SiteOpened {
        district: DistrictId,
        variants: u8,
        margin_bp: u16,
        trend: Trend,
    } = 516,
    /// Właściciel zamyka firmę dobrowolnie (M7f WP15, PRD §12.4).
    ///
    /// Osobny powód od `BankruptcyOpened`: to nie jest upadłość, tylko wyjście
    /// przed nią. Firma wyprzedaje majątek bez syndyka, spłaca zobowiązania i wraca
    /// na rynek pracy — tańsze dla niej i dla symulacji. `months` mówi, jak długo
    /// trwała strata, `cash` — ile zostało w kasie, kiedy właściciel się poddał.
    VoluntaryClosure { months: u8, cash: Money } = 517,
    /// Mieszkaniec zakłada firmę (M7f WP15, PRD §5.6).
    ///
    /// `score` to wynik `founding_score` w setnych, `capital` — kapitał, który
    /// wniósł. Obie liczby są **z jego własnych oszczędności i zdolności kredytowej**,
    /// a nie z prognozy: nisza jest wykryta w okolicy, którą zna (§5.7), a nie
    /// przez wyrocznię globalną.
    FirmFounded { score: u16, capital: Money } = 518,
    /// Sieć zewnętrzna wchodzi do miasta (M7f WP15, PRD §12.4).
    ///
    /// `capital` jest **zarejestrowanym punktem emisji pieniądza** — przechodzi
    /// przez `Books::inject_external_capital`, inaczej globalny test zachowania
    /// pieniądza pękłby i nikt nie wiedziałby dlaczego (`D10`).
    ChainEntered { capital: Money, sites: u8 } = 519,
    // 520–599 zarezerwowane dla M7.

    // ── M8: miasto jako aktor (600–699) ──────────────────────────────────────────
    /// Miasto naliczyło daninę (M8a WP2, PRD §6.8).
    ///
    /// Powód niesie **stawkę użytą w chwili naliczenia**, a nie aktualną: stawka
    /// zmienia się uchwałą i nigdy wstecz (`effective_from`), więc karta inspekcji
    /// pokazana pół roku później ma tłumaczyć kwotę, która wtedy powstała. Bez tego
    /// pola „dlaczego tyle" nie da się odpowiedzieć inaczej niż przeliczeniem, które
    /// da inny wynik.
    TaxAssessed {
        kind: TaxKind,
        rate_bp: u16,
        amount: Money,
    } = 600,
    /// Należność zapłacona — pieniądz przeszedł od płatnika do budżetu miasta.
    TaxSettled { kind: TaxKind, amount: Money } = 601,
    /// Termin minął, a należność stoi. `days` liczy doby od terminu płatności,
    /// `amount` to kwota główna bez odsetek — odsetki rosną co dobę i mają własny
    /// wiersz w rejestrze, więc powtarzanie ich tutaj dałoby dwie prawdy o jednej
    /// liczbie.
    TaxOverdue {
        kind: TaxKind,
        days: u16,
        amount: Money,
    } = 602,
    /// Należność umorzona: przestała być wymagalna, choć nikt jej nie zapłacił.
    /// To jest czwarty stan cyklu życia i wchodzi do domknięcia `Assessed =
    /// Settled + Overdue + Abated` (test T1) — pominięcie go znaczyłoby, że
    /// upadłość firmy gubi budżetowi pieniądze bez śladu.
    TaxAbated {
        kind: TaxKind,
        why: AbateReason,
        amount: Money,
    } = 603,
    /// Miasto wydało pieniądze (M8a WP1).
    PublicSpend {
        category: SpendCategory,
        amount: Money,
    } = 604,
    /// Miasto wyemitowało obligację, bo deficytu nie dało się zamknąć cięciem.
    MunicipalBondIssued { coupon_bp: u16, principal: Money } = 605,
    /// Domknięcie deficytu cięciem wydatków: `gap` to luka, `cut_bp` — o ile
    /// promili przycięto plan wydatków bieżących.
    BudgetDeficitClosed { gap: Money, cut_bp: u16 } = 606,
    /// Zrzut obciążenia w sieci przesyłowej (M8b §5.4 krok 3): wyspa nie domykała
    /// bilansu, więc odbiorcy od najniższego priorytetu poszli w ciemność.
    ///
    /// `priority` to **ostatni odłączony** próg, a nie każdy po kolei: gracz pyta
    /// „dokąd sięgnęło", a nie „ilu było". `shortfall_w` niesie moc, której
    /// zabrakło **przed** zrzutem — po zrzucie jest z definicji zero, więc powód
    /// zapisany po fakcie mówiłby, że nic się nie stało.
    LoadShed {
        service: UtilityService,
        priority: u8,
        shortfall_w: u32,
    } = 607,
    /// Zabezpieczenie krawędzi zadziałało: przepływ przekroczył przepustowość
    /// i linia wypadła z sieci (M8b §5.4 krok 5). To jest wejście do kaskady —
    /// wypadnięcie linii zmienia topologię, a zmiana topologii jest następną rundą.
    ///
    /// `repair_minutes` jest **wylosowanym** czasem brygady (`StreamId::GridFault`),
    /// a nie stałą: to jedyna rzecz, którą sieć w tej fazie losuje.
    GridTripped {
        service: UtilityService,
        repair_minutes: u16,
    } = 608,
    /// Zdarzenie świata zaczęło się (M8c §5.5, PRD §11.1).
    ///
    /// Powodem **nie jest** losowanie: rzut rozstrzygnął tylko „czy dziś", a szansę
    /// wyliczyły sondy stanu świata. Dlatego karta zdarzenia pokazuje obok tego powodu
    /// rozbicie hazardu na czynniki (`HazardFactor`) — „awaria, bo blok ma 32 lata
    /// i 90 dób zaległej konserwacji", a nie „awaria, bo wypadła szóstka".
    ///
    /// `severity_bps` jest siłą **wylosowaną w widełkach definicji** i to ona skaluje
    /// każdy efekt zdarzenia: to samo zdarzenie o sile 2000 i 9000 bps zmienia parametr
    /// inaczej, bo susza bywa dokuczliwa i bywa katastrofą.
    EventStarted {
        event: EventId,
        category: EventCategory,
        severity_bps: u16,
    } = 609,
    /// Zdarzenie świata się skończyło (M8c §5.5).
    ///
    /// `days` to długość, która faktycznie wyszła, a nie ta zapowiedziana: zdarzenie
    /// `UntilResolved` kończy się wtedy, gdy stan świata przestaje je podtrzymywać,
    /// więc „ile trwało" jest wynikiem symulacji, nie parametrem definicji.
    EventEnded {
        event: EventId,
        category: EventCategory,
        days: u16,
    } = 610,
    /// Jakość placówki publicznej po miesięcznym przeliczeniu (M8d WP7, PRD §10.3).
    ///
    /// Trzy liczby obok wyniku, bo „szkoła ma 41 punktów" nie jest odpowiedzią na
    /// pytanie gracza „dlaczego moje dziecko nie umie czytać". `funding_bp` mówi,
    /// ile miasto daje na ucznia wobec normy, `staff_bp` — jaka część etatów jest
    /// obsadzona, `load_bp` — ilu uczniów przypada na miejsce. Placówka niedofinansowana
    /// i placówka przepełniona schodzą do tej samej jakości z dwóch różnych powodów,
    /// a naprawia się je dwoma różnymi decyzjami.
    ServiceQuality {
        kind: ServiceKind,
        district: DistrictId,
        quality: Q,
        funding_bp: u16,
        staff_bp: u16,
        load_bp: u16,
    } = 611,
    /// Urząd wydał pozwolenie (M8d WP7, M8e §5.2).
    ///
    /// `waited_days` jest **wynikiem**, a nie parametrem: czas oczekiwania bierze się
    /// z obsady urzędu, długości kolejki i dni wolnych (`K-15`). To jest cała treść
    /// tego powodu — pozwolenie wydane w trzy doby i w sześćdziesiąt jest tą samą
    /// decyzją urzędu i różni się wyłącznie tym, ile kosztowało czasu.
    PermitIssued { kind: PermitKind, waited_days: u16 } = 612,
    /// Urząd otworzył sprawę przeciwko zakładowi (M8d WP8, PRD §10.4).
    ///
    /// `evidence` to materiał dowodowy w chwili otwarcia, nie w chwili rozstrzygnięcia:
    /// sprawa rośnie w czasie i to jest jej istota. Otwarcie sprawy samo w sobie nie
    /// jest karą i nie musi się nią skończyć.
    CaseOpened { agency: AgencyKind, evidence: Q } = 613,
    /// Urząd nałożył środek zaradczy (M8d WP8).
    ///
    /// `amount` jest kwotą tam, gdzie środek ma kwotę (grzywna, domiar), i zerem tam,
    /// gdzie jej nie ma (zamknięcie, cofnięcie koncesji, przymusowy podział) — bo
    /// wtedy dolegliwością jest czas albo majątek, a nie pieniądz, i udawanie kwoty
    /// zafałszowałoby histogram kar.
    RemedyImposed {
        agency: AgencyKind,
        remedy: RemedyKind,
        amount: Money,
    } = 614,
    /// Zakład zmienił udział obrotu poza deklaracją (M8d WP8, PRD §10.4).
    ///
    /// Szara strefa nie jest cechą charakteru, tylko **odpowiedzią na przyciśnięcie**:
    /// zakład pod kreską ukrywa więcej, zakład z marżą wraca do deklarowania. Dlatego
    /// powód niesie obie liczby — nowy udział i wynik miesiąca, który go wywołał.
    ShadowShareSet { share_bp: u16, last_result: Money } = 615,
    /// Rada uchwaliła regulację (M8e WP9, PRD §10.1).
    ///
    /// `for_bp` to poparcie w radzie w punktach bazowych, a `delay_days` — vacatio
    /// legis, czyli ile dób minie od uchwalenia do wejścia w życie. Obie liczby są
    /// treścią, a nie ozdobą: uchwała przegłosowana 5100 do 4900 i uchwała
    /// jednomyślna to dwie różne sytuacje polityczne, a regulacja wchodząca
    /// jutro i za kwartał to dwie różne sytuacje gospodarcze.
    PolicyEnacted {
        kind: PolicyKind,
        for_bp: u16,
        delay_days: u16,
    } = 616,
    /// Burmistrz ruszył stawkę daniny (M8e WP9, PRD §10.1, §6.8).
    ///
    /// Osobny powód od [`DecisionReason::PolicyEnacted`], mimo że stawka jest
    /// uchwałą jak każda inna, bo niesie **kierunek i odchylenie od celu**, czyli
    /// to, czego pilnuje test T5. `gap_bp` jest odchyleniem salda budżetu od celu
    /// w chwili decyzji — z dodatnim znakiem, gdy miasto ma nadwyżkę.
    TaxRateChanged {
        kind: TaxKind,
        from_bp: u16,
        to_bp: u16,
        gap_bp: i16,
    } = 617,
    /// Miasto ogłosiło przetarg (M8e WP9, PRD §10.3).
    ///
    /// `subject_id` niesie dzielnicę albo linię — słownik jest płaski, a identyfikator
    /// idzie osobnym polem, ta sama korekta co przy `RemedyKind` (`K-64`).
    TenderPublished {
        subject: TenderKind,
        subject_id: u16,
        budget: Money,
    } = 618,
    /// Przetarg rozstrzygnięty (M8e WP9).
    ///
    /// `score_bp` jest punktacją zwycięzcy, a `runner_up_bp` — drugiego w kolejności.
    /// Przetarg wygrany o włos i wygrany bezkonkurencyjnie to dwie różne odpowiedzi
    /// na pytanie „dlaczego nie ja", a to jest pytanie, które gracz zada (PRD §14.1).
    /// `bids` równe zero znaczy przetarg nierozstrzygnięty — miasto robi wtedy usługę
    /// samo i płaci za nią plan, a nie ofertę.
    TenderAwarded {
        subject: TenderKind,
        price: Money,
        score_bp: u16,
        runner_up_bp: u16,
        bids: u8,
    } = 619,
    /// Wybory rozstrzygnięte (M8e WP10, PRD §10.2).
    ///
    /// `turnout_bp` to frekwencja, `winner_bp` — wynik zwycięzcy, `incumbent` mówi,
    /// czy wygrał urzędujący burmistrz. Trzecia liczba jest tu dlatego, że cały
    /// mechanizm z §1 dokumentu fazy („spadek poparcia → przegrana w tym obwodzie")
    /// jest nieczytelny bez odpowiedzi, czy władza się w ogóle zmieniła.
    ElectionHeld {
        turnout_bp: u16,
        winner_bp: u16,
        incumbent: bool,
    } = 620,
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
    } = 621,
    /// Ktoś dołożył się do kampanii kandydata (M8e WP10, PRD §10.2).
    ///
    /// `illegal` rozstrzyga, czy to darowizna, czy łapówka — i to jedno pole niesie
    /// całe ryzyko: wsparcie nielegalne podnosi sondę hazardu skandalu, a ujawnienie
    /// uderza w kandydata **i** we wspierającego. Fundator jedzie osobno, bo jest
    /// podmiotem, a nie słownikiem (`K-62`).
    CampaignBacked {
        candidate: u8,
        amount: Money,
        illegal: bool,
    } = 622,
    // 623–699 zarezerwowane dla M8.
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
            DecisionReason::Repricing { .. } => 303,
            DecisionReason::CreditApproved { .. } => 304,
            DecisionReason::CreditRejected { .. } => 305,
            DecisionReason::BudgetShortfall { .. } => 306,
            DecisionReason::Shortage { .. } => 400,
            DecisionReason::ProductionHalted { .. } => 401,
            DecisionReason::SubstituteUsed { .. } => 402,
            DecisionReason::SupplierChosen { .. } => 403,
            DecisionReason::ContractSigned { .. } => 404,
            DecisionReason::ExportChosen { .. } => 405,
            DecisionReason::Hired { .. } => 500,
            DecisionReason::WageRaise { .. } => 501,
            DecisionReason::JobLeft { .. } => 502,
            DecisionReason::PolicyApplied { .. } => 503,
            DecisionReason::ManagerAssigned { .. } => 504,
            DecisionReason::LoanTaken { .. } => 505,
            DecisionReason::LeaseSigned { .. } => 506,
            DecisionReason::ReceivablesFactored { .. } => 507,
            DecisionReason::BondIssued { .. } => 508,
            DecisionReason::BankruptcyOpened { .. } => 509,
            DecisionReason::ClaimSettled { .. } => 510,
            DecisionReason::MarginTargetSet { .. } => 511,
            DecisionReason::RestockTargetSet { .. } => 512,
            DecisionReason::SiteClosed { .. } => 513,
            DecisionReason::StrategySet { .. } => 514,
            DecisionReason::CompetitiveResponse { .. } => 515,
            DecisionReason::SiteOpened { .. } => 516,
            DecisionReason::VoluntaryClosure { .. } => 517,
            DecisionReason::FirmFounded { .. } => 518,
            DecisionReason::ChainEntered { .. } => 519,
            DecisionReason::TaxAssessed { .. } => 600,
            DecisionReason::TaxSettled { .. } => 601,
            DecisionReason::TaxOverdue { .. } => 602,
            DecisionReason::TaxAbated { .. } => 603,
            DecisionReason::PublicSpend { .. } => 604,
            DecisionReason::MunicipalBondIssued { .. } => 605,
            DecisionReason::BudgetDeficitClosed { .. } => 606,
            DecisionReason::LoadShed { .. } => 607,
            DecisionReason::GridTripped { .. } => 608,
            DecisionReason::EventStarted { .. } => 609,
            DecisionReason::EventEnded { .. } => 610,
            DecisionReason::ServiceQuality { .. } => 611,
            DecisionReason::PermitIssued { .. } => 612,
            DecisionReason::CaseOpened { .. } => 613,
            DecisionReason::RemedyImposed { .. } => 614,
            DecisionReason::ShadowShareSet { .. } => 615,
            DecisionReason::PolicyEnacted { .. } => 616,
            DecisionReason::TaxRateChanged { .. } => 617,
            DecisionReason::TenderPublished { .. } => 618,
            DecisionReason::TenderAwarded { .. } => 619,
            DecisionReason::ElectionHeld { .. } => 620,
            DecisionReason::VoteCast { .. } => 621,
            DecisionReason::CampaignBacked { .. } => 622,
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
        // Blok M8 (600–699) — otwarty w M8a, wartości wieczne.
        assert_eq!(
            DecisionReason::TaxAssessed {
                kind: TaxKind::Vat,
                rate_bp: 2300,
                amount: Money(1)
            }
            .discriminant(),
            600
        );
        assert_eq!(
            DecisionReason::BudgetDeficitClosed {
                gap: Money(1),
                cut_bp: 100
            }
            .discriminant(),
            606
        );
        assert_eq!(
            DecisionReason::GridTripped {
                service: UtilityService::Electricity,
                repair_minutes: 180
            }
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
