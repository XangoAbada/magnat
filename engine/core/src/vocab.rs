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
use crate::types::DistrictId;
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
    /// Kategoria zapasu gospodarstwa domowego — indeks w `Household.stock: [u8; 8]`
    /// (M3c §5.6) i ładunek `DecisionReason::StockBelowThreshold` (M3b §5.4).
    ///
    /// Kolejność jest kontraktem tak samo jak przy `NeedKind`: to ona indeksuje tablicę
    /// dni zapasu. W M3 zapas jest abstrakcyjnymi „dniami"; M5 zastępuje go realnymi
    /// towarami i to on przypisuje `GoodId` do kategorii — `core` nadal nie wie,
    /// co w kategorii leży.
    StockCat {
        Food, Drink, Hygiene, Cleaning, Clothing, Medicine, Fuel, Other,
    }
}

/// Liczba kategorii zapasu — rozmiar tablicy `Household.stock`.
pub const STOCK_CAT_COUNT: usize = StockCat::ALL.len();

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
    LifeEventKind {
        Born, Conceived, Retired, FellIll, Recovered, Died,
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
    /// Rodzaj mediów. M5 używa go w `TxKind::Utility`, zanim M8 zbuduje sieci przesyłowe —
    /// i to jest dokładnie powód, dla którego enum stoi tutaj, a nie w `sim/city`.
    UtilityService {
        Electricity, Water, Sewage, Gas, Heat, Waste, Internet,
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
    }

    #[test]
    fn rozmiar_place_ref() {
        // PlaceRef nosi WorldCoord (12 B) + dyskryminanta — mieści się w 16 B,
        // więc wolno go trzymać w komponencie bez wskaźnika.
        assert!(size_of::<PlaceRef>() <= 16);
    }
}
