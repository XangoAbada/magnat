//! Słowniki domenowe wspólne dla wielu faz (00 §K-8).
//!
//! **Dlaczego w `core`, a nie w fazie, która tego używa.** Gdyby `TransportMode` mieszkał
//! w `sim/traffic`, a `NeedKind` w `sim/agents`, to `sim/agents` (wybór środka transportu)
//! zależałby od `sim/traffic`, `sim/traffic` od `sim/economy` (koszt paliwa), a `sim/economy`
//! od `sim/agents` (potrzeby kupującego). To cykl w grafie crate'ów — Cargo takiego workspace
//! nie zbuduje. Zgłosiły to niezależnie M3, M5 i M8, więc nie jest to hipoteza.
//!
//! `core` dostaje **wyłącznie słownik i konwersje, zero logiki**: żadnego kosztu przejazdu,
//! żadnej funkcji użyteczności, żadnego zapotrzebowania — te żyją w fazach.

use crate::ids::{BuildingId, ParcelId, SiteId};
use crate::types::{DistrictId, Qty};
use serde::{Deserialize, Serialize};

/// Makro dla słownika bez ładunku: warianty, `ALL`, `from_u8`.
macro_rules! vocab_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(
            Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize,
        )]
        #[repr(u8)]
        pub enum $name { $($variant),+ }

        impl $name {
            /// Wszystkie warianty w kolejności deklaracji — deterministyczna podstawa
            /// iteracji (00 §3.2), np. przy budowie tablic indeksowanych wariantem.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            #[inline]
            #[must_use]
            pub const fn as_index(self) -> usize {
                self as usize
            }

            #[must_use]
            pub fn from_index(i: usize) -> Option<$name> {
                $name::ALL.get(i).copied()
            }

            #[must_use]
            pub const fn name(self) -> &'static str {
                match self { $($name::$variant => stringify!($variant)),+ }
            }
        }
    };
}

vocab_enum! {
    /// Wymiar, w którym agent ocenia opcję. Konsument: M5 (funkcja użyteczności zakupu,
    /// PRD §6.4), M7 (oceny ofert pracy), M9 (podsumowanie decyzji gracza).
    UtilityKind {
        Price, Quality, Distance, Time, Variety, Brand, Habit, Convenience, Risk,
    }
}

/// Liczba wymiarów oceny — rozmiar histogramu „dlaczego u mnie kupili" w panelu
/// sklepu (M5e §5.12). Kolejność wariantów jest kontraktem tej tablicy.
pub const UTILITY_KIND_COUNT: usize = UtilityKind::ALL.len();

vocab_enum! {
    /// Środek transportu. Konsument: M3 (plan dnia), M4 (ruch), M5 (koszt dojazdu
    /// po zakupy), M8 (polityka miejska).
    TransportMode {
        Walk, Bicycle, Car, Bus, Tram, Rail, Freight,
    }
}

vocab_enum! {
    /// Rodzaj potrzeby. Konsument: M3 (model potrzeb), M5 (co wyzwala zakup),
    /// M8 (usługi publiczne).
    ///
    /// **Dwanaście wariantów wniesionych przez M3** (M3a §5.5) — kolejność jest
    /// kontraktem: `Needs.level: [u8; 12]` indeksuje się `as_index()`, więc
    /// przestawienie wariantów przestawiłoby zapisane poziomy potrzeb wszystkim
    /// mieszkańcom. Wolno dopisywać na końcu, nie wolno przestawiać (M3 §6.4).
    NeedKind {
        Hunger, Sleep, Hygiene, Health, Safety, Housing,
        Mobility, Clothing, Leisure, Social, Status, Development,
    }
}

/// Liczba potrzeb — rozmiar tablicy `Needs.level` (M3a §5.1).
pub const NEED_COUNT: usize = NeedKind::ALL.len();

vocab_enum! {
    /// Rodzaj miejsca, w którym da się zaspokoić potrzebę. Odwzorowanie
    /// `NeedKind → PlaceKind` jest danymi (`data/needs/needs.ron`), nie kodem.
    ///
    /// W `core`, bo mówią nim trzy fazy: M3 (wybór celu w planie dnia), M5 (sklep
    /// jako miejsce z ofertą) i M8 (instytucje publiczne z pojemnością). Zero logiki —
    /// `core` nie wie, co się w takim miejscu dzieje.
    PlaceKind {
        Home, Grocery, Eatery, Clothing, Doctor, Pharmacy, Hospital,
        Leisure, Social, Education, Workplace,
    }
}

vocab_enum! {
    /// Cecha osobowości — indeks w `Personality([u8; 8])` (M3a §5.1).
    ///
    /// W `core`, bo cechy czyta więcej niż jedna faza: M3 (plan dnia, czas wolny),
    /// M5 (`PriceSensitivity`, `Loyalty` w funkcji użyteczności §6.4), M7 (`Ambition`
    /// przy zmianie pracy), M10 (`Openness` przy marce). Kolejność jest kontraktem
    /// tak samo jak przy `NeedKind`.
    TraitId {
        Ambition, Thrift, PriceSensitivity, Loyalty,
        Sociability, Openness, Risk, Conscientiousness,
    }
}

vocab_enum! {
    /// Skutek deprywacji potrzeby — ładunek `DecisionReason::Deprivation` (M3a §5.5).
    /// Nazwa mówi, **co** się pogarsza; o ile, mówią dane w `data/needs/needs.ron`.
    DeprivationEffect {
        EnergyLoss, HealthLoss, MoodLoss, StressGain,
        StatusLoss, ProductivityLoss, AbsenceRisk, AccidentRisk, AmbitionGain,
    }
}

vocab_enum! {
    /// Zobowiązanie stałe w planie dnia — ładunek `DecisionReason::Commitment` (M3b §5.4).
    ///
    /// W `core`, choć sam planer jest w `sim/agents`: ładunek centralnego enuma nie może
    /// pochodzić z crate'u, który od `core` zależy (ta sama reguła, która wypchnęła
    /// `ReplanCause` z `DecisionReason` — korekta B-1). M4 czyta go przy dojazdach,
    /// M7 przy grafikach zmianowych.
    CommitmentKind {
        Work, School, Childcare, Commute,
    }
}

vocab_enum! {
    /// Kategoria zapasu gospodarstwa domowego — indeks w `Household.stock`
    /// (M3c §5.6) i ładunek `DecisionReason::StockBelowThreshold` (M3b §5.4).
    ///
    /// Kolejność jest kontraktem tak samo jak przy `NeedKind`: to ona indeksuje tablicę
    /// dni zapasu. W M3 zapas jest abstrakcyjnymi „dniami"; M5 zastępuje go realnymi
    /// towarami i to on przypisuje `GoodId` do kategorii — `core` nadal nie wie,
    /// co w kategorii leży.
    ///
    /// **`Comms` dopisane na końcu w M10c** (`K-83`) i to jest jedyny dozwolony ruch
    /// w tej liście: kolejność indeksuje `Household.stock`, więc wstawienie wariantu
    /// w środku przenumerowałoby spiżarnię każdego gospodarstwa w każdym zapisie.
    /// Powód dopisania jest jeden i wynika z kryterium WP10.9: potrzeba `Social`
    /// nie miała **żadnej** kategorii zakupowej, więc „kontakt społeczny" był
    /// wyłącznie wizytą w miejscu i żaden nowy towar nie mógł go zaspokoić.
    StockCat {
        Food, Drink, Hygiene, Cleaning, Clothing, Medicine, Fuel, Other, Comms,
    }
}

/// Liczba kategorii zapasu — rozmiar tablicy `Household.stock`.
pub const STOCK_CAT_COUNT: usize = StockCat::ALL.len();

vocab_enum! {
    /// Który człon korekty przeważył przy zmianie ceny (M5c §5.6) — ładunek
    /// `DecisionReason::Repricing`.
    ///
    /// W `core` z tego samego powodu co `StockCat` i `RejectCause` (`K-20`): ładunek
    /// centralnego enuma nie może pochodzić z crate'u, który od `core` zależy.
    /// Kolejność jest kontraktem, bo `as_index()` indeksuje histogram powodów przecen
    /// w panelu sklepu (M5e).
    ///
    /// - `Cost` — ruszył się koszt własny; marża docelowa bez zmian.
    /// - `Stock` — zapas odbiegł od celu (nadmiar w dół, brak w górę).
    /// - `Competitor` — obserwowana cena konkurenta (z opóźnieniem 1–7 dni).
    /// - `Experiment` — trwający eksperyment cenowy albo zmierzona elastyczność.
    /// - `Spoilage` — przecena towaru przy kończącym się terminie ważności.
    /// - `Floor` / `Ceiling` — cena oparła się o dolny albo górny ogranicznik marży.
    /// - `Policy` — zmiana wynika wprost z polityki (ręczna cena, dopasowanie do konkurenta).
    PriceDriver {
        Cost, Stock, Competitor, Experiment, Spoilage, Floor, Ceiling, Policy,
    }
}

vocab_enum! {
    /// Dlaczego oferta odpadła albo zakup się nie odbył (M5b §5.4, PRD §14.1).
    ///
    /// W `core`, bo jest **ładunkiem centralnego enuma** `DecisionReason` — ta sama
    /// reguła, która wypchnęła tu `StockCat` i `CommitmentKind` (`K-20`): ładunek nie
    /// może pochodzić z crate'u, który od `core` zależy.
    ///
    /// Warianty są **bezładunkowe z rozmysłu**, choć plan M5 §5.4 pisał je z liczbami
    /// (`TooFar { extra_min }`, `PriceHigherBy { bp }`). Liczba mieszka teraz w polu
    /// `detail` powodu, bo `vocab_enum!` daje `ALL`/`as_index`/`from_index`, a to one
    /// robią z tego enuma indeks histogramu utraconych sprzedaży — wariant z ładunkiem
    /// nie byłby indeksem.
    ///
    /// Kolejność jest kontraktem: indeksuje `LostSaleHistogram.by_cause`.
    ///
    /// - `NotKnown` — mieszkaniec nie zna sklepu (§5.7); nie wchodzi do kandydatów.
    /// - `OutOfStock` — półka pusta; oferta zostaje widoczna, żeby było co pokazać.
    /// - `TooFar` — dojazd poza zasięgiem zadania (`detail` = minuty ponad limit).
    /// - `PriceTooHigh` — cena ponad to, co kupujący zaakceptował (`detail` = bp).
    /// - `QualityBelowStatus` — jakość nie pasuje do statusu (§5.4).
    /// - `BudgetExhausted` — brak środków w gospodarstwie.
    /// - `NoOffers` — w promieniu nie było ani jednej oferty kategorii.
    /// - `BelowThreshold` — najlepsza użyteczność poniżej progu; zakup odłożony.
    RejectCause {
        NotKnown,
        OutOfStock,
        TooFar,
        PriceTooHigh,
        QualityBelowStatus,
        BudgetExhausted,
        NoOffers,
        BelowThreshold,
    }
}

/// Liczba powodów odrzucenia — rozmiar histogramu utraconych sprzedaży (M5b §5.4).
pub const REJECT_CAUSE_COUNT: usize = RejectCause::ALL.len();

vocab_enum! {
    /// Rodzaj kredytu (M5d §5.10, PRD §6.5).
    ///
    /// W `core` z tego samego powodu co `PriceDriver` (`K-30`): jest ładunkiem
    /// `DecisionReason::Credit{Approved,Rejected}`, a ładunek centralnego enuma nie
    /// może pochodzić z crate'u, który od `core` zależy.
    ///
    /// M5 zna dwa produkty; M7 dokłada inwestycyjny, M10 hipoteczny — **na końcu**,
    /// bo kolejność wariantów jest kontraktem zapisu gry (`as_index()` indeksuje
    /// widełki marży kredytowej w `data/economy/bank.ron`).
    ///
    /// - `Consumer` — kredyt konsumpcyjny gospodarstwa domowego.
    /// - `WorkingCapital` — kredyt obrotowy zakładu pod zapasy i koszty stałe.
    /// - `Investment` — kredyt inwestycyjny firmy pod nakład na zakład (M7d §5.12).
    ///   Dopisany **na końcu**, bo indeks wybiera produkt w `data/economy/bank.ron`.
    LoanKind {
        Consumer,
        WorkingCapital,
        Investment,
    }
}

vocab_enum! {
    /// Co otwarło postępowanie upadłościowe (M7d §5.13, PRD §7.8).
    ///
    /// W `core` z tej samej reguły co [`WageCause`] (`K-45`): jest ładunkiem
    /// `DecisionReason::BankruptcyOpened`, a ładunek centralnego enuma nie może
    /// pochodzić z crate'u, który od `core` zależy. Drugi czytelnik znany z nazwy
    /// i numeru fazy: M8 (kara administracyjna jako `CourtOrder`), M9 (karta firmy).
    ///
    /// Plan fazy zapisywał wariant jako `IlliquidDays { n }`; liczba dób idzie
    /// osobnym polem powodu, bo `vocab_enum!` daje słownik, a nie enum z ładunkiem
    /// — i tak jest lepiej, bo histogram przyczyn upadłości liczy przyczyny,
    /// a nie pary (przyczyna, długość).
    ///
    /// - `Illiquid` — firma nie zapłaciła wymagalnych zobowiązań przez próg dób.
    /// - `NegativeEquity` — kapitał własny ujemny **i** kredyt w zaległości.
    /// - `CourtOrder` — postanowienie z zewnątrz (M8: egzekucja administracyjna).
    BankruptcyTrigger {
        Illiquid,
        NegativeEquity,
        CourtOrder,
    }
}

vocab_enum! {
    /// Kolejność zaspokojenia w upadłości (M7d §5.13, `K-10`).
    ///
    /// Ładunek `DecisionReason::ClaimSettled`, więc w `core` — ta sama reguła co przy
    /// [`BankruptcyTrigger`]. Drugi czytelnik: M8 zgłasza roszczenie miasta jako
    /// `Public` i musi je nazwać, nie mając własnej ścieżki egzekucji (`K-10`).
    ///
    /// **Kolejność wariantów JEST regułą podziału**, a nie tylko kontraktem indeksu:
    /// podział idzie po `as_index()` rosnąco i niższy priorytet nie dostaje ani
    /// grosza, dopóki wyższy nie jest zaspokojony w całości. Dzięki temu zmiana
    /// układu (decyzja właściciela produktu, `D7` fazy M7) jest zmianą tej listy,
    /// a nie zmianą logiki podziału.
    ///
    /// Układ przyjęty przez właściciela produktu: **pracownicy przed wierzycielem
    /// zabezpieczonym**. Skutek jest zamierzony — ryzyko przenosi się na bank, czyli
    /// na stronę, która je wycenia, a upadłość zostaje widoczna w dzielnicy, bo ludzie
    /// dostają wypłatę i wydają ją lokalnie.
    ///
    /// - `Wages` — zaległe wynagrodzenia.
    /// - `Severance` — odprawy.
    /// - `Secured` — wierzyciele zabezpieczeni, **do wartości zabezpieczenia**;
    ///   nadwyżka ponad nią spada do `Unsecured` jawnym roszczeniem, nie po cichu.
    /// - `Public` — miasto: podatki, składki, opłaty, kary administracyjne (M8).
    /// - `Unsecured` — dostawcy, obligatariusze, kary umowne.
    /// - `Owners` — właściciele; to, co zostanie, czyli zwykle nic.
    ClaimPriority {
        Wages,
        Severance,
        Secured,
        Public,
        Unsecured,
        Owners,
    }
}

/// Liczba priorytetów — rozmiar histogramu wypłat syndyka (M7d §5.13).
pub const CLAIM_PRIORITY_COUNT: usize = ClaimPriority::ALL.len();

vocab_enum! {
    /// Dlaczego bank odmówił kredytu (M5d §5.10).
    ///
    /// Osobny słownik od `RejectCause`, mimo podobnej roli, bo tamten indeksuje
    /// histogram **utraconych sprzedaży** i jego kolejność jest kontraktem panelu
    /// sklepu. Wspólny enum kosztowałby dwa martwe warianty po każdej stronie.
    ///
    /// - `NoIncome` — gospodarstwo bez dochodu; nie ma z czego liczyć zdolności.
    /// - `DstiTooHigh` — obciążenie ratami ponad limit dochodu (`dsti_limit_bp`).
    /// - `Arrears` — zaległości w historii kredytowej ostatnich 24 miesięcy.
    /// - `DscrTooLow` — przepływy zakładu nie pokrywają obsługi długu (`dscr_min`).
    /// - `NoLender` — w mieście nie ma banku, który mógłby udzielić kredytu.
    RejectCredit {
        NoIncome,
        DstiTooHigh,
        Arrears,
        DscrTooLow,
        NoLender,
    }
}

vocab_enum! {
    /// Pozycja kosztów stałych gospodarstwa domowego (M5d §5.9, PRD §5.2).
    ///
    /// Ładunek `DecisionReason::BudgetShortfall`, więc mieszka w `core` (`K-30`).
    /// Kolejność indeksuje `HouseholdBudget.fixed` — jest kontraktem zapisu gry.
    ///
    /// - `Housing` — czynsz albo rata mieszkaniowa. W M5 stała z danych; M7/M10 wnoszą
    ///   czynsz emergentny z rynku nieruchomości.
    /// - `Utilities` — media. M8 podmienia stałą na rachunek z sieci przesyłowych.
    /// - `Insurance` — ubezpieczenia. M10 wnosi realny produkt ubezpieczeniowy.
    /// - `LoanService` — raty kredytów z harmonogramów (`M5d` WP9).
    FixedCost {
        Housing,
        Utilities,
        Insurance,
        LoanService,
    }
}

/// Liczba pozycji kosztów stałych — rozmiar `HouseholdBudget.fixed` (M5d §5.9).
pub const FIXED_COST_COUNT: usize = FixedCost::ALL.len();

vocab_enum! {
    /// Klasa drogi w hierarchii ulicznej. **Kolejność jest kontraktem** — indeksuje
    /// tablicę `SPECS` generatora miasta w `sim/world`.
    ///
    /// W `core`, bo mówią nią dwie fazy i nie mogą jej sobie podać: M2 (`sim/world`)
    /// planuje sieć i nadaje klasę, M4 (`engine/nav`) buduje z niej graf przejezdny.
    /// Zależność `engine/nav → sim/world` jest niedopuszczalna, bo `sim/world` zależy
    /// od `sim/agents`, a `sim/agents` od M4b zależy od `engine/nav` (`Z-1`) — powstałby
    /// dokładnie ten cykl, przed którym broni `K-8`. Dalsi konsumenci: M8 (remonty
    /// i regulacje ruchu), M11 (render sieci wg klasy).
    ///
    /// `core` nie dostaje przy tym parametrów klasy: `ClassSpec` (szerokość pasa,
    /// długość odcinka, limity mostu i tunelu) zostaje w `sim/world`, bo to dane
    /// generacji miasta, a nie słownik.
    RoadClass {
        Highway, Arterial, Collector, Local, Service, Pedestrian, RailFreight, RailPassenger,
    }
}

impl RoadClass {
    /// Ranga: im wyżej w hierarchii ulicznej, tym większa. Tory mają rangę 0 —
    /// nie uczestniczą w hierarchii dróg kołowych i nigdy nie są celem snapowania.
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            RoadClass::Highway => 6,
            RoadClass::Arterial => 5,
            RoadClass::Collector => 4,
            RoadClass::Local => 3,
            RoadClass::Service => 2,
            RoadClass::Pedestrian => 1,
            RoadClass::RailFreight | RoadClass::RailPassenger => 0,
        }
    }

    /// Czy klasa niesie ruch kołowy (wchodzi do testu spójności T1 fazy M2 i do
    /// wyznaczania kwartałów).
    #[must_use]
    pub const fn is_driveable(self) -> bool {
        matches!(
            self,
            RoadClass::Highway
                | RoadClass::Arterial
                | RoadClass::Collector
                | RoadClass::Local
                | RoadClass::Service
        )
    }

    #[must_use]
    pub const fn is_rail(self) -> bool {
        matches!(self, RoadClass::RailFreight | RoadClass::RailPassenger)
    }

    /// Klucz tekstowy — stabilny identyfikator w `data/` i w zapisie gry.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            RoadClass::Highway => "highway",
            RoadClass::Arterial => "arterial",
            RoadClass::Collector => "collector",
            RoadClass::Local => "local",
            RoadClass::Service => "service",
            RoadClass::Pedestrian => "pedestrian",
            RoadClass::RailFreight => "rail_freight",
            RoadClass::RailPassenger => "rail_passenger",
        }
    }
}

impl StockCat {
    /// Potrzeba, którą uzupełnia zakup w tej kategorii. Odwzorowanie jest tutaj,
    /// a nie w danych, bo jest **definicją kategorii**, nie parametrem do strojenia:
    /// zapas żywności uzupełnia głód i nic innego.
    ///
    /// Kategoria wskazująca potrzebę bez miejsc w `data/needs/needs.ron` (paliwo,
    /// „inne" → `Housing`) nie produkuje w M3 zadania zakupowego. To granica fazy,
    /// nie luka: paliwo należy do M4, wyposażenie mieszkania do M5.
    #[must_use]
    pub const fn need(self) -> NeedKind {
        match self {
            StockCat::Food | StockCat::Drink => NeedKind::Hunger,
            StockCat::Hygiene | StockCat::Cleaning => NeedKind::Hygiene,
            StockCat::Clothing => NeedKind::Clothing,
            StockCat::Medicine => NeedKind::Health,
            StockCat::Fuel | StockCat::Other => NeedKind::Housing,
            // Telefon, abonament, znaczek pocztowy — rzeczy, które kupuje się po to,
            // żeby utrzymać kontakt (M10c WP10.9, PRD §11.3). Do M10c `Social` nie
            // miał żadnej kategorii zakupowej, więc `cats_of(Social)` zwracało pustkę
            // i potrzeba zaspokajała się wyłącznie wizytą w miejscu.
            StockCat::Comms => NeedKind::Social,
        }
    }
}

vocab_enum! {
    /// Kierunek i powód decyzji migracyjnej gospodarstwa — ładunek
    /// `DecisionReason::MigrationDecision` (M3c §5.7).
    ///
    /// W `core`, bo jest ładunkiem centralnego enuma (K-12): ładunek nie może pochodzić
    /// z crate'u, który od `core` zależy. Czyta go M8 (polityka mieszkaniowa miasta)
    /// i M10 (historia „na sucho").
    MigrationKind {
        Arrived, LeftJobless, LeftHousing, LeftUnsettled,
    }
}

vocab_enum! {
    /// Zdarzenie cyklu życia mieszkańca — ładunek `DecisionReason::LifeEvent` (M3c §5.6).
    ///
    /// Tu, a nie w `sim/agents`, z tego samego powodu co `MigrationKind`. M7 czyta
    /// `Retired` (zwolnienie etatu), M8 `Died` i `FellIll` (usługi publiczne).
    ///
    /// `LeftSchool` dopisany **na końcu** przez `R2-WP1` — kolejność wariantów jest
    /// kontraktem zapisu gry, więc nowy wariant wchodzi wyłącznie za ostatnim. Nazwa
    /// jest neutralna z rozmysłu: tym samym przejściem wychodzi ze szkoły absolwent
    /// w wieku `school_end` i piętnastolatek, który wziął etat. Ile lat faktycznie
    /// przechodził, mówi `edu_level`, a nie nazwa zdarzenia.
    LifeEventKind {
        Born, Conceived, Retired, FellIll, Recovered, Died, LeftSchool,
    }
}

vocab_enum! {
    /// Rodzaj czynności w planie dnia. Konsument: M3 (planer + DES), M4 (skąd dokąd),
    /// M9 (oś czasu w karcie inspekcji).
    ActivityKind {
        Sleep, Work, School, Shop, Eat, Leisure, Social, Errand, Commute, Idle,
    }
}

vocab_enum! {
    /// Nadwozie pojazdu dostawczego.
    ///
    /// Mieszka w `core` (`K-8`), bo ma **dwóch** konsumentów znanych z nazwy i numeru
    /// fazy: M4 deklaruje je przy klasie pojazdu w `data/vehicles/classes.ron`, a M6
    /// stawia jako **wymaganie** zlecenia transportowego — chłodnia w drodze nie jest
    /// preferencją, tylko warunkiem, bez którego towar psuje się ośmiokrotnie szybciej.
    /// Duplikat enuma po jednej ze stron rozjechałby się przy pierwszej zmianie,
    /// a kolejność wariantów jest kontraktem zapisu gry: siedzi w wymaganiach zlecenia,
    /// a zlecenia przeżywają zapis.
    BodyType {
        Box, Reefer, Tanker, Tipper, Container, Flatbed,
    }
}

vocab_enum! {
    /// Rodzaj mediów. M5 używa go w `TxKind::Utility`, zanim M8 zbuduje sieci przesyłowe —
    /// i to jest dokładnie powód, dla którego enum stoi tutaj, a nie w `sim/city`.
    UtilityService {
        Electricity, Water, Sewage, Gas, Heat, Waste, Internet,
    }
}

vocab_enum! {
    /// Kategoria zdarzenia świata (PRD §11.2, M8c §5.5).
    ///
    /// Tutaj, a nie w `sim/events`, bo jest **ładunkiem** `DecisionReason::EventStarted`,
    /// a ładunek centralnego enuma nie może pochodzić z crate'u, który od `core` zależy —
    /// ta sama reguła, która wypchnęła tu `PriceDriver` (`K-30`) i `WageCause` (`K-45`).
    /// Kolejność wariantów jest kontraktem: `as_index()` indeksuje histogram zdarzeń
    /// na rok w raporcie balansatora (ryzyko `R1` fazy) i licznik `max_concurrent`
    /// per kategoria.
    EventCategory {
        Natural, Infrastructure, External, Social, Firm, Political,
    }
}

vocab_enum! {
    /// Pora roku. Rok gry ma 360 dób (`K-1`), więc sezon to równe 90 dób i nie ma
    /// sporu o granicę. Czytają go: generator zdarzeń (bramka „susza tylko wiosną
    /// i latem"), rolnictwo M6 (kalendarz agrotechniczny) i UI.
    ///
    /// Kolejność zaczyna się od zimy, bo doba 0 roku gry wypada w styczniu.
    Season {
        Winter, Spring, Summer, Autumn,
    }
}

vocab_enum! {
    /// Rodzaj surowca w złożu. Tutaj, a nie w `sim/world`, bo słownika używają M5
    /// (ceny surowców), M6 (wydobycie i mapowanie na `GoodId`) i M8 (opłaty eksploatacyjne) —
    /// czyli więcej niż jedna faza, co jest kryterium z 00 §K-8.
    /// `core` nie wie nic o geometrii złoża ani o kosztach wydobycia — to jest w M1 i M6.
    ResourceKind {
        Coal, IronOre, Oil, Gas, Aggregate, Groundwater, ClayDeposit,
    }
}

vocab_enum! {
    /// Rodzaj bramy — punktu, którym towar i człowiek wchodzą do miasta.
    ///
    /// Tutaj, a nie w `sim/world`, od M6a: brama jest polem `Good::import_via` w katalogu
    /// towarów (`sim/supply`), a `sim/supply` nie może zależeć od `sim/world` — zależność
    /// idzie w drugą stronę, bo to generator miasta pyta katalog, a nie katalog generator.
    /// Kryterium jest to samo co przy `RoadClass` (`K-23`): dwa crate'y, jeden słownik.
    ///
    /// **Kolejność wariantów jest kolejnością przetwarzania** w generatorze M2 — brama
    /// drogowa powstaje przed kolejową, bo kolej domyka się do istniejącego układu.
    GateKind {
        Highway, RailFreight, RailPassenger, Port, Airport,
    }
}

impl GateKind {
    /// Klucz tekstowy — ten, który stoi w `data/` i w zapisie gry.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            GateKind::Highway => "highway",
            GateKind::RailFreight => "rail_freight",
            GateKind::RailPassenger => "rail_passenger",
            GateKind::Port => "port",
            GateKind::Airport => "airport",
        }
    }

    /// Czy brama jest kolejowa — tory powstają dopiero w M2c (Etap 4 daje im cel).
    #[must_use]
    pub const fn is_rail(self) -> bool {
        matches!(self, GateKind::RailFreight | GateKind::RailPassenger)
    }

    /// Przepustowość dobowa w [`Qty`] (milisztuki). Skala jest **wstępna**: prawdziwym
    /// konsumentem jest limit importu w M6 i to M6 ją skalibruje. M2 dostarczył pole,
    /// nie bilans handlowy.
    #[must_use]
    pub const fn capacity(self) -> Qty {
        Qty(match self {
            GateKind::Highway => 60_000_000,
            GateKind::RailFreight => 250_000_000,
            GateKind::RailPassenger => 0,
            GateKind::Port => 400_000_000,
            GateKind::Airport => 4_000_000,
        })
    }
}

vocab_enum! {
    /// Powód, dla którego masa zeszła z bilansu. Właścicielem słownika jest M6, ale mieszka
    /// w `core`, bo księguje go także M5 (odpis towaru przeterminowanego na półce)
    /// i M7 (odpis w rachunku wyniku).
    ///
    /// Reguła, dla której ten enum w ogóle istnieje: **masa znikająca bez kategorii jest
    /// błędem testu, nie zaokrągleniem** (M6 §7.3 pkt 1). Kolejność wariantów jest
    /// kontraktem, bo `as_index()` indeksuje histogram strat w panelu zakładu.
    LossKind {
        Drying, Evaporation, Spoilage, Expired, Spillage, ProcessWaste, Setup,
        TransportDamage, Theft, Storage,
    }
}

/// Liczba kategorii strat — rozmiar histogramu strat.
pub const LOSS_KIND_COUNT: usize = LossKind::ALL.len();

vocab_enum! {
    /// Dlaczego linia produkcyjna nie produkuje. Właścicielem jest M6, ale słownik mieszka
    /// w `core`, bo jest **ładunkiem** `DecisionReason::ProductionHalted` — a ładunek
    /// centralnego enuma nie może pochodzić z crate'u, który od `core` zależy (`K-20`).
    /// Czyta go karta inspekcji zakładu (M6e), pulpit firmy (M7) i alert miejski
    /// przy odcięciu mediów (M8).
    ///
    /// `Starved` i `Blocked` są tu obok awarii z rozmysłu: dla gracza „stoi, bo nie ma
    /// mąki" i „stoi, bo się zepsuło" to ta sama klasa pytania, a rozdzielenie ich na
    /// dwa enumy zmusiłoby kartę inspekcji do dwóch ścieżek dla jednego zdania.
    /// Kolejność wariantów jest kontraktem, bo `as_index()` indeksuje histogram postojów.
    LineStopCause {
        Wear, NoPower, NoWater, NoStaff, Strike, Starved, Blocked, Setup, Maintenance,
    }
}

vocab_enum! {
    /// Stopień kaskady niedoboru (PRD §8.4) **bez ładunku** — ładunek zostaje po stronie
    /// M6 w `supply::ShortageStage`, tutaj jest sam stopień, bo tyle niesie
    /// `DecisionReason::Shortage` i tyle pokazuje karta inspekcji.
    ///
    /// Kolejność jest kolejnością prób z PRD i **jest kontraktem**: bufor → obniżenie
    /// produkcji → spot → import → substytut → postój. Test kaskady sprawdza, że
    /// przejścia idą po `as_index()` w górę, więc przestawienie wariantów przestawiłoby
    /// znaczenie testu, a nie tylko liczby.
    ShortageStageKind {
        Ok, Buffer, Throttled, SpotSearch, Importing, Substituted, Halted,
    }
}

vocab_enum! {
    /// Dlaczego firma ruszyła stawkę w ofercie pracy (M7b §5.5, PRD §6.6).
    ///
    /// Ładunek `DecisionReason::WageRaise`, więc mieszka w `core` z tego samego powodu
    /// co `PriceDriver` i `LineStopCause` (`K-20`): ładunek centralnego enuma nie może
    /// pochodzić z crate'u, który od `core` zależy. Czyta go karta inspekcji firmy,
    /// panel ludzi (M7c) i związki zawodowe (M10, `K-9`) — bo „o ile i dlaczego
    /// podniesiono" jest wejściem żądania płacowego.
    ///
    /// **`Ceiling` nie jest powodem podwyżki, tylko powodem jej braku.** Krok
    /// przycięty do sufitu marży zapisuje się z tym powodem i z przyrostem, który
    /// realnie został — zero znaczy „oferta zamrożona, wakat zostaje pusty".
    /// To jest odpowiedź na pytanie gracza „dlaczego nikogo nie zatrudniłeś"
    /// i dlatego stoi tu obok podwyżek, a nie w osobnym słowniku (M7 §7.1 pkt 4).
    ///
    /// Kolejność wariantów jest kontraktem: `as_index()` indeksuje histogram przyczyn
    /// podwyżek w panelu rynku pracy.
    WageCause {
        NoCandidates, Shortage, Headhunt, Counteroffer, Ceiling,
    }
}

vocab_enum! {
    /// Dlaczego pracownik przestał pracować w tym zakładzie (M7b §5.5, WP6).
    ///
    /// Ładunek `DecisionReason::JobLeft`, ta sama reguła co przy [`WageCause`].
    /// Kryterium WP6 brzmi: **odejście zawsze ma powód po stronie odchodzącego** —
    /// więc słownik obejmuje i odejścia dobrowolne, i zwolnienia, i wyjście z rynku
    /// pracy, którego firma nie wywołała (emerytura, zgon, wyjazd z miasta; tamte
    /// prowadzi M3, a M7 tylko domyka po nich etat).
    ///
    /// `Dismissed` to zwolnienie **za wynik**, `Redundancy` — z powodu kosztów;
    /// dla gracza to dwa różne zdania i dwie różne konsekwencje reputacyjne.
    LeaveCause {
        BetterOffer, Mood, Stress, Dismissed, Redundancy, LeftLabourForce,
    }
}

vocab_enum! {
    /// Podstawa ceny: kwota brutto płacona w detalu czy netto w hurcie (`K-7`).
    ///
    /// **Przeniesione z `sim/economy::offer` do `core` przy starcie M7c** — wykonanie
    /// `K-8` dla podstawy ceny, ten sam ruch co `RoadClass` (`K-23`) i `GateKind`
    /// (`K-33`). Powód jest twardy: `Metric::Price` w języku reguł (`sim/policy`)
    /// **musi** nieść podstawę, bo walidator odrzuca mieszanie brutto z netto w jednym
    /// porównaniu (M9d §5.6) — a `sim/policy` nie może zależeć od `sim/economy`,
    /// bo zależność idzie `economy → firms → policy` i odwrócenie zamknęłoby cykl.
    /// Duplikat enuma odpada z tego samego powodu co zawsze: rozjechałby się przy
    /// pierwszej zmianie, a tu chodzi o liczbę, którą widzi gracz na półce.
    ///
    /// Nazwy wariantów zostają takie, jakie M5 nadał w `Offer.price_basis`
    /// (`GrossRetail`/`NetB2B`), a nie `Gross`/`Net` z przykładu w M9d: przenosiny
    /// nie są okazją do przemianowania pola, które siedzi w arenie 80 tys. ofert.
    PriceBasis {
        GrossRetail, NetB2B,
    }
}

vocab_enum! {
    /// Rodzaj akcji polityki — ładunek `DecisionReason::PolicyApplied` (M7c WP6b).
    ///
    /// **Rodzaj, a nie akcja.** Pełna `Action` z języka reguł niesie `Expr`, czyli
    /// drzewo za wskaźnikiem, i do 24-bajtowego powodu nie wejdzie (`K-12` zasada 5).
    /// Do dziennika idzie więc odpowiedź na pytanie „co polityka zrobiła" w rozdzielczości,
    /// w jakiej gracz je zadaje; pełne wejścia i wynik pokazuje dry-run M9.
    ///
    /// Kolejność wariantów jest kontraktem, bo `as_index()` indeksuje histogram akcji
    /// w panelu polityk. Kolejność jest ta sama co w `policy::Action`.
    ActionKind {
        SetPrice, AdjustPrice, SetMargin, ClampPrice, OrderUpTo, OrderQty, Markdown,
        RemoveFromShelf, Hire, RaiseWage, PlanProduction, Alert, AskPlayer,
    }
}

vocab_enum! {
    /// Strategia firmy — kurs, na którym stoi tier taktyczny (M7e §5.7, PRD §12.1).
    ///
    /// W `core`, bo jest **ładunkiem** `DecisionReason::StrategySet`, a ładunek
    /// centralnego enuma nie może pochodzić z crate'u, który od `core` zależy —
    /// ta sama reguła, która wypchnęła tu `WageCause` (`K-45`) i `ActionKind` (`K-47`).
    /// Drugi czytelnik znany z nazwy i numeru fazy: M10 (marka i R&D czytają kurs
    /// firmy, zanim zaproponują wydatek).
    ///
    /// Kolejność wariantów jest kontraktem: `as_index()` indeksuje zarówno histogram
    /// strategii w panelu, jak i tablicę presetów polityk w `data/policies/`.
    FirmStrategy {
        Cautious, Discount, NicheQuality, AggressiveExpansion, Innovative, Consolidator,
    }
}

vocab_enum! {
    /// Czym firma odpowiedziała na utratę udziału w rynku (M7e WP14, PRD §12.2).
    ///
    /// Ładunek `DecisionReason::CompetitiveResponse`, ta sama reguła co przy
    /// [`FirmStrategy`]. Trzy warianty, bo trzy są legalne: kartel i zmowa cenowa
    /// są przestępstwem i należą do M8, a nie do repertuaru firmy AI.
    ///
    /// - `PriceWar` — zejście z ceną poniżej własnej marży docelowej, płacone z gotówki.
    /// - `SupplierLock` — kontrakt na wyłączność z dostawcą, opłacony premią.
    /// - `Poach` — oferta bezpośrednia do pracowników rywala na rolach, których
    ///   mu najbardziej brakuje.
    ReactionKind {
        PriceWar, SupplierLock, Poach,
    }
}

vocab_enum! {
    /// Skąd mieszkaniec wie o marce — źródło wpisu w slocie marki (M10b §5.1).
    ///
    /// W `core`, bo jest **ładunkiem** `DecisionReason::BrandLearned`, a ładunek
    /// centralnego enuma nie może pochodzić z crate'u, który od `core` zależy —
    /// ta sama reguła, która wypchnęła tu `PriceDriver` (`K-30`) i `WageCause` (`K-45`).
    /// Dwóch czytelników znanych z nazwy i numeru fazy: M3/M10 (slot marki
    /// u mieszkańca) i M10 (kampanie, media, karta inspekcji).
    ///
    /// Kolejność wariantów jest kontraktem **podwójnie**: `as_index()` indeksuje
    /// histogram źródeł w panelu marketingu, a **rosnąca dyskryminanta to malejąca
    /// siła źródła** — dokładnie jak w `KnowledgeKind` (M3a). `Owned` jest pierwszy
    /// i najmocniejszy, bo slot przypięty (pracodawca, sklep odwiedzony ≥ 8 razy)
    /// nie podlega wypieraniu; `Experience` bije reklamę, bo „kupiłem i wiem" nie ma
    /// prawa zamienić się w „widziałem plakat" (M10 §8, ryzyko `R4`).
    TouchSource {
        Owned, Experience, Media, Rumor, Ad,
    }
}

pub const TOUCH_SOURCE_COUNT: usize = TouchSource::ALL.len();

vocab_enum! {
    /// Kanał, którym kampania dociera do mieszkańca (M10b §5.2, PRD §7.6).
    ///
    /// W `core` z tego samego powodu co [`TouchSource`]: jest ładunkiem
    /// `DecisionReason::{BrandLearned, AdCampaignStarted}`. Sam kanał **z parametrami**
    /// (krawędź billboardu, tytuł prasowy, promień ulotki) mieszka w `magnat_media`,
    /// bo parametr niesie uchwyty, których `core` nie zna — ta sama korekta, którą
    /// `K-48` zrobił przy `BankruptcyTrigger`, a `K-64` przy `RemedyKind`.
    ///
    /// Kolejność wariantów jest kontraktem: `as_index()` indeksuje tablicę przyrostu
    /// znajomości `awareness_gain` w `data/tuning/brand.ron` i histogram kanałów w panelu.
    AdChannelKind {
        Billboard, Press, Radio, Tv, Leaflet, InStorePromo, Sponsorship, Pr,
    }
}

pub const AD_CHANNEL_KIND_COUNT: usize = AdChannelKind::ALL.len();

vocab_enum! {
    /// Rodzaj tytułu medialnego (M10b §5.3, PRD §7.2).
    ///
    /// W `core`, bo jest ładunkiem `DecisionReason::StoryPublished`. Cztery warianty
    /// odpowiadają czterem rodzajom zakładu w `data/site_types/media.ron`; kolejność
    /// indeksuje tablicę `outlets` w `data/tuning/brand.ron`.
    MediaKind {
        Newspaper, Radio, Tv, Portal,
    }
}

pub const MEDIA_KIND_COUNT: usize = MediaKind::ALL.len();

vocab_enum! {
    /// Linia redakcyjna tytułu — czym redakcja waży wartość informacyjną zdarzenia.
    ///
    /// Ładunek `DecisionReason::StoryPublished` razem z [`MediaKind`]. Kolejność
    /// indeksuje tablicę wag kategorii zdarzeń w `data/tuning/brand.ron`.
    EditorialBias {
        Market, Social, Sensational, Local,
    }
}

vocab_enum! {
    /// Kierunek zmiany — **bez wielkości** (M7f §5.10, `K-52`).
    ///
    /// W `core`, bo ma trzech czytelników znanych z nazwy i numeru fazy: `sim/macro`
    /// (M10) go produkuje, `sim/firms` (M7) czyta go w tierze strategicznym, a M9
    /// rysuje z niego strzałkę w karcie firmy. Duplikat rozjechałby się przy pierwszej
    /// zmianie, a tu chodzi o jedyną rzecz, którą prognozie makro wolno powiedzieć
    /// graczowi: „w górę", „bez zmian", „w dół". Kwota z modelu makro nie trafia
    /// do UI **nigdy** (`R15`), więc `Trend` jest całym słownikiem tej odpowiedzi.
    ///
    /// Kolejność wariantów jest kontraktem, bo `as_index()` indeksuje ikonę strzałki.
    Trend {
        Up, Flat, Down,
    }
}

vocab_enum! {
    /// Rodzaj daniny publicznej (M8a §5.1, PRD §6.8). Siedem danin i ani jednej więcej.
    ///
    /// W `core`, bo jest **ładunkiem** `DecisionReason::Tax*`, a ładunek centralnego
    /// enuma nie może pochodzić z crate'u, który od `core` zależy — ta sama reguła,
    /// która wypchnęła tu `PriceDriver` (`K-30`) i `WageCause` (`K-45`). Drugi czytelnik
    /// znany z nazwy i numeru fazy: M5 (`ChargeKind` w dzienniku transakcji niesie
    /// ten indeks) i M7 (wierzyciel podatkowy w postępowaniu upadłościowym, `K-10`).
    ///
    /// **Słownik jest płaski.** Plan fazy zapisywał `Excise(ExciseClass)`,
    /// `Duty(TariffClass)` i `License(LicenseClass)`; klasa idzie **osobnym polem**
    /// należności, bo `vocab_enum!` daje słownik, a nie enum z ładunkiem — i tak jest
    /// lepiej, bo histogram wpływów ma liczyć daniny, a nie pary (danina, klasa).
    ///
    /// Kolejność wariantów jest kontraktem podwójnie: `as_index()` indeksuje
    /// `CityBudget.revenue_ytd`, a ten sam indeks jedzie do `TxKind::TaxPayment`.
    TaxKind {
        Cit, Pit, Vat, Property, Excise, Duty, License,
    }
}

/// Liczba danin — rozmiar tablicy wpływów budżetu miasta. Kolejność `TaxKind`
/// jest kontraktem tej tablicy.
pub const TAX_KIND_COUNT: usize = TaxKind::ALL.len();

vocab_enum! {
    /// Kierunek wydatku publicznego (M8a §5.1). Ładunek `DecisionReason::PublicSpend`
    /// i indeks `CityBudget.spend_ytd`; ten sam indeks jedzie do `ProgramId`
    /// w `TxKind::PublicSpend`, więc kolejność jest kontraktem zapisu gry.
    ///
    /// W `core` z tego samego powodu co [`TaxKind`]: ładunek centralnego enuma.
    /// Drugi czytelnik znany z nazwy i numeru fazy to M9 (panel „Miasto").
    SpendCategory {
        Education, Health, Police, Fire, Waste, Parks, Administration,
        TransitSubsidy, RoadMaintenance, CapitalInvestment, Subsidies, DebtService,
    }
}

/// Liczba kierunków wydatku — rozmiar tablicy wydatków budżetu miasta.
pub const SPEND_CATEGORY_COUNT: usize = SpendCategory::ALL.len();

vocab_enum! {
    /// Dlaczego należność podatkowa przestała być wymagalna bez zapłaty (M8a §5.1).
    ///
    /// Ładunek `DecisionReason::TaxAbated`, ta sama reguła co przy [`TaxKind`].
    /// `Bankruptcy` zamyka należność, której postępowanie upadłościowe M7 nie
    /// zaspokoiło w całości (`K-10`, `ClaimPriority::Public`); `Council` to uchwała
    /// rady (M8e); `TimeBarred` — przedawnienie po okresie z `TaxCode`;
    /// `PayerGone` — płatnik przestał istnieć, a nie było z czego egzekwować.
    AbateReason {
        Bankruptcy, Council, TimeBarred, PayerGone,
    }
}

vocab_enum! {
    /// Rodzaj usługi publicznej (M8d §5.3, PRD §10.3).
    ///
    /// W `core`, a nie w `sim/city`, bo czyta go **więcej niż jedna faza** (`K-8`)
    /// i to nie hipotetycznie: M3 przy rozwoju dziecka i długości choroby, M5 przy
    /// stratach inwentaryzacyjnych sklepu, M8 przy jakości placówki. Żadna z tych
    /// faz nie może zależeć od `sim/city` — zależność idzie w drugą stronę.
    ///
    /// Kolejność wariantów jest kontraktem, bo `as_index()` indeksuje wiersz
    /// [`ServiceCoverage`], czyli pokrycie obwodowe per dzielnica.
    ///
    /// `School` obejmuje przedszkole, szkołę i bibliotekę; `Clinic` przychodnię,
    /// `Hospital` szpital. Rozróżnienie przychodni od szpitala zostaje, bo jedna
    /// stoi w co drugiej dzielnicy, a drugi jeden na całe miasto — i to jest różnica
    /// w zasięgu, nie w nazwie.
    ServiceKind {
        School, Clinic, Hospital, Police, Fire, Waste, Park, Office,
    }
}

/// Liczba rodzajów usługi publicznej — szerokość wiersza [`ServiceCoverage`].
pub const SERVICE_KIND_COUNT: usize = ServiceKind::ALL.len();

vocab_enum! {
    /// Urząd kontrolny (M8d §5.3, PRD §10.4).
    ///
    /// Ładunek `DecisionReason::{CaseOpened, RemedyImposed}`, więc w `core` z tego
    /// samego powodu co [`TaxKind`]: ładunek centralnego enuma nie może pochodzić
    /// z crate'u, który od `core` zależy. Drugi czytelnik znany z nazwy i numeru
    /// fazy: M7 (ryzyko kontroli w decyzji firmy) i M9 (karta sprawy).
    ///
    /// Kolejność jest kontraktem, bo `as_index()` indeksuje histogram spraw
    /// per urząd w panelu miasta.
    AgencyKind {
        Antitrust, LaborInspection, Sanitary, Environment, TaxOffice,
    }
}

/// Liczba urzędów kontrolnych.
pub const AGENCY_KIND_COUNT: usize = AgencyKind::ALL.len();

vocab_enum! {
    /// Rodzaj środka zaradczego nałożonego przez urząd (M8d §5.3).
    ///
    /// **Słownik jest płaski, a kwota i termin idą osobnymi polami** — ta sama
    /// korekta co przy [`TaxKind`] i `BankruptcyTrigger` (`K-48`): `vocab_enum!`
    /// daje słownik, a nie enum z ładunkiem, a histogram środków ma liczyć środki,
    /// a nie pary (środek, kwota). Wariant z ładunkiem mieszka w `sim/city::Remedy`.
    ///
    /// **Cztery warianty, bo cztery ktoś nakłada.** Cofnięcie koncesji wyglądało na
    /// oczywisty piąty i nie ma go tu z rozmysłu: żaden urząd nie ma dziś przesłanki,
    /// która by je uzasadniała, a wariant, którego nic nie ustawia, przechodzi każdy
    /// test i w histogramie wygląda tak samo jak wariant działający (`R2`).
    RemedyKind {
        Fine, Closure, ForcedDivestiture, BackTax,
    }
}

vocab_enum! {
    /// Rodzaj pozwolenia wydawanego przez urząd (M8d WP7, M8e §5.2).
    ///
    /// Ładunek `DecisionReason::PermitIssued`. Kolejność wariantów jest kontraktem,
    /// bo `as_index()` indeksuje koszt wniosku w jednostkach przerobu urzędu.
    PermitKind {
        Build, ChangeOfUse, Demolition, EnvClearance, AlcoholLicense,
        FoodService, RoadAccess, OversizeTransport,
    }
}

vocab_enum! {
    /// Rodzaj uchwały rady miasta (M8e §5.2, PRD §10.1).
    ///
    /// Ładunek `DecisionReason::{PolicyEnacted, TaxRateChanged}`, więc w `core`
    /// z tego samego powodu co [`TaxKind`]: ładunek centralnego enuma nie może
    /// pochodzić z crate'u, który od `core` zależy. Sam `Policy` — z kwotami,
    /// progami i godzinami otwarcia — zostaje w `sim/city`, bo to nie jest słownik.
    /// Ta sama korekta co przy [`RemedyKind`] i `BankruptcyTrigger` (`K-48`).
    ///
    /// Kolejność wariantów jest kontraktem, bo `as_index()` indeksuje histogram
    /// uchwał w karcie rady.
    ///
    /// **Siedem wariantów, bo siedem ma dziś czytelnika**, a lista nieobecnych jest
    /// dłuższa i każda nieobecność ma powód: strefowania nie ma, bo strefa jest
    /// wejściem generatora miasta i po Etapie 7 nikt jej nie czyta; opłaty
    /// parkingowej — bo cennik siedzi za `&mut` w routerze podróży; ograniczenia
    /// ruchowego — bo maska krawędzi nie ma jak dojść do routera z tej samej
    /// przyczyny. Dotacja nie jest osobnym wariantem, tylko udziałem
    /// `SpendCategory::Subsidies` w planie wydatków, czyli `SpendShare`.
    /// Wariant, którego skutku nikt nie widzi, przechodzi każdy test i wygląda
    /// tak samo jak wariant działający (`R2`).
    PolicyKind {
        TaxRate, SpendShare, MinWage, EmissionLimit, TariffCap,
        AgencyStaffing, TradingHours,
    }
}

/// Liczba rodzajów uchwały — szerokość tablicy histerezy burmistrza.
pub const POLICY_KIND_COUNT: usize = PolicyKind::ALL.len();

vocab_enum! {
    /// Przedmiot przetargu miejskiego (M8e §5.2, PRD §10.3).
    ///
    /// Ładunek `DecisionReason::{TenderPublished, TenderAwarded}`; identyfikator
    /// przedmiotu (dzielnica, linia) idzie **osobnym polem** — ta sama korekta
    /// co przy [`RemedyKind`].
    TenderKind {
        WasteCollection, RoadMaintenance, TransitLine, Construction,
    }
}

vocab_enum! {
    /// Co przeważyło w głosie wyborcy (M8e §5.7, PRD §10.2).
    ///
    /// Ładunek `DecisionReason::VoteCast`. Wyjaśnialność z §7 dokumentu 00 dotyczy
    /// także wyborcy: „głosowała na Nowaka" bez powodu jest liczbą w tabeli, a nie
    /// odpowiedzią. Wariant niesie **największy** składnik użyteczności kandydata,
    /// a nie całą jej rozpiskę — rozpiska jest w panelu wyborów.
    ///
    /// Kolejność jest kontraktem, bo `as_index()` indeksuje histogram motywów
    /// w karcie rady.
    VoteDriver {
        Taxes, Services, Mood, Media, Ties, Habit,
    }
}

vocab_enum! {
    /// Biom. Konsument poza M1: M2 (strefowanie i zieleń), M5/M6 (rolnictwo i leśnictwo),
    /// M8 (zdarzenia pogodowe zależne od pokrycia terenu).
    Biome {
        Sea, Lake, River, Marsh, BroadleafForest, ConiferForest, MixedForest,
        Grassland, Cropland, Scrub, Rock, Sand, Snow,
    }
}

impl Biome {
    /// Czy biom jest powierzchnią wodną — pytanie zadawane w M1, M2 i M4 na tyle często,
    /// że lepiej mieć jedną odpowiedź niż trzy listy wariantów.
    #[inline]
    #[must_use]
    pub const fn is_water(self) -> bool {
        matches!(self, Biome::Sea | Biome::Lake | Biome::River)
    }

    /// Czy biom jest zalesiony.
    #[inline]
    #[must_use]
    pub const fn is_forest(self) -> bool {
        matches!(
            self,
            Biome::BroadleafForest | Biome::ConiferForest | Biome::MixedForest
        )
    }
}

/// Współrzędna świata w centymetrach — całkowitoliczbowa, więc deterministyczna
/// i porównywalna. `i32` w centymetrach daje ±21 tys. km, czyli zapas rzędu wielkości
/// ponad największą planowaną mapę.
///
/// `core` definiuje **tylko reprezentację**; interpretacja (układ odniesienia, granice
/// świata, wysokość terenu) należy do M1/M2 — źródłem prawdy o terenie jest `TerrainQuery`
/// (00 §K-13), nie ten typ.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize,
)]
pub struct WorldCoord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl WorldCoord {
    pub const ORIGIN: WorldCoord = WorldCoord { x: 0, y: 0, z: 0 };

    #[inline]
    #[must_use]
    pub const fn new(x: i32, y: i32, z: i32) -> WorldCoord {
        WorldCoord { x, y, z }
    }

    /// Kwadrat odległości poziomej w cm². Kwadrat, a nie pierwiastek: porównanie
    /// odległości nie wymaga `sqrt`, a dopóki go nie ma, nie ma też zaokrąglenia,
    /// o które można by się rozjechać.
    ///
    /// ponytail: arytmetyka nasycająca zamiast `i128`. Sufit: 3 037 000 km między
    /// punktami (√(2^63)/2 cm) — dwa rzędy wielkości ponad największą planowaną mapę.
    /// Przy przekroczeniu wynik przywiera do `i64::MAX` zamiast zawijać, więc
    /// porządek „dalej/bliżej" nigdy się nie odwraca. Jeśli M1 kiedyś zaadresuje
    /// świat większy niż kontynent, ta funkcja zmienia typ wyniku na `i128`.
    #[inline]
    #[must_use]
    pub const fn distance_sq_xy(self, other: WorldCoord) -> i64 {
        let dx = (self.x as i64) - (other.x as i64);
        let dy = (self.y as i64) - (other.y as i64);
        dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy))
    }
}

/// Odniesienie do miejsca w świecie — wspólna waluta między agentami, ruchem i gospodarką.
///
/// Świadomie **nie ma** wariantu `RoadNode`: węzeł grafu dróg należy do `engine/nav` (M4),
/// a wpisanie go tutaj odtworzyłoby cykl, przed którym ten moduł ma bronić.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
pub enum PlaceRef {
    Building(BuildingId),
    Parcel(ParcelId),
    Site(SiteId),
    District(DistrictId),
    Coord(WorldCoord),
}

impl Default for PlaceRef {
    /// Punkt (0, 0, 0) — wartość **wypełniająca**, nie „brak miejsca".
    ///
    /// Istnieje wyłącznie po to, żeby `PlaceRef` dało się trzymać w tablicy o stałej
    /// pojemności (`ArrayVec` w M3) bez `unsafe`. Kod, który chce powiedzieć „nie ma
    /// miejsca", mówi to przez `Option<PlaceRef>` — i tak ma to wyrazić, bo inaczej
    /// mieszkaniec poszedłby po zakupy do początku układu współrzędnych.
    fn default() -> PlaceRef {
        PlaceRef::Coord(WorldCoord::ORIGIN)
    }
}

impl Season {
    /// Sezon doby roku (0..=359). Cztery równe kwartały po 90 dób — patrz komentarz
    /// przy `SimCalendar` w `core::time`.
    #[must_use]
    pub const fn of_day(day_of_year: u16) -> Season {
        match day_of_year / 90 {
            0 => Season::Winter,
            1 => Season::Spring,
            2 => Season::Summer,
            _ => Season::Autumn,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slownik_ma_stabilne_indeksy() {
        assert_eq!(UtilityKind::Price.as_index(), 0);
        assert_eq!(UtilityKind::from_index(0), Some(UtilityKind::Price));
        assert_eq!(TransportMode::ALL.len(), 7);
        assert_eq!(ActivityKind::Idle.name(), "Idle");
        assert_eq!(
            UtilityService::ALL.first(),
            Some(&UtilityService::Electricity)
        );
    }

    #[test]
    fn wspolrzedna_liczy_odleglosc_bez_zmiennoprzecinkowych() {
        let a = WorldCoord::new(300, 400, 0);
        assert_eq!(a.distance_sq_xy(WorldCoord::ORIGIN), 250_000);
        // Skraj zakresu nasyca się zamiast zawinąć — porządek odległości zostaje.
        let daleko = WorldCoord::new(i32::MAX, i32::MAX, 0);
        assert_eq!(
            daleko.distance_sq_xy(WorldCoord::new(i32::MIN, i32::MIN, 0)),
            i64::MAX
        );
        // W realnym zakresie mapy (100 km) nasycenia nie ma.
        let sto_km = WorldCoord::new(10_000_000, 0, 0);
        assert_eq!(
            sto_km.distance_sq_xy(WorldCoord::ORIGIN),
            100_000_000_000_000
        );
    }

    #[test]
    fn kolejnosc_potrzeb_i_cech_jest_kontraktem() {
        // M3 §6.4: przestawienie wariantu przestawia zapisane poziomy potrzeb
        // wszystkim mieszkańcom, bo `Needs.level` indeksuje się `as_index()`.
        assert_eq!(NEED_COUNT, 12);
        assert_eq!(NeedKind::Hunger.as_index(), 0);
        assert_eq!(NeedKind::Sleep.as_index(), 1);
        assert_eq!(NeedKind::Development.as_index(), 11);
        assert_eq!(TraitId::ALL.len(), 8);
        assert_eq!(TraitId::Ambition.as_index(), 0);
        assert_eq!(TraitId::Conscientiousness.as_index(), 7);
        // M8a: `as_index()` indeksuje `CityBudget.revenue_ytd` i `spend_ytd`, a ten sam
        // indeks jedzie do `ChargeKind`/`ProgramId` w dzienniku transakcji M5 — czyli
        // do zapisu gry. Przestawienie wariantu przepisuje cudzą historię budżetu.
        assert_eq!(TAX_KIND_COUNT, 7);
        assert_eq!(TaxKind::Cit.as_index(), 0);
        assert_eq!(TaxKind::Vat.as_index(), 2);
        assert_eq!(TaxKind::License.as_index(), 6);
        assert_eq!(SPEND_CATEGORY_COUNT, 12);
        assert_eq!(SpendCategory::Education.as_index(), 0);
        assert_eq!(SpendCategory::DebtService.as_index(), 11);
        assert_eq!(AbateReason::Bankruptcy.as_index(), 0);
    }

    #[test]
    fn rozmiar_place_ref() {
        // PlaceRef nosi WorldCoord (12 B) + dyskryminanta — mieści się w 16 B,
        // więc wolno go trzymać w komponencie bez wskaźnika.
        assert!(size_of::<PlaceRef>() <= 16);
    }
}
