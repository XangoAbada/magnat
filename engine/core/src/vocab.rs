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
    NeedKind {
        Hunger, Thirst, Rest, Hygiene, Health, Social, Fun, Safety, Esteem, Education,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slownik_ma_stabilne_indeksy() {
        assert_eq!(UtilityKind::Price.as_index(), 0);
        assert_eq!(UtilityKind::from_index(0), Some(UtilityKind::Price));
        assert_eq!(TransportMode::ALL.len(), 7);
        assert_eq!(NeedKind::ALL.len(), 10);
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
    fn rozmiar_place_ref() {
        // PlaceRef nosi WorldCoord (12 B) + dyskryminanta — mieści się w 16 B,
        // więc wolno go trzymać w komponencie bez wskaźnika.
        assert!(size_of::<PlaceRef>() <= 16);
    }
}
