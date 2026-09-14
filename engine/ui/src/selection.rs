//! Selekcja encji w widoku 3D (M3d §5.11, decyzja 9.3).
//!
//! **Model selekcji jest po stronie UI, mechanizm wskazywania po stronie renderu.**
//! `engine/ui` nie wie, jak powstaje odpowiedź na pytanie „co jest pod kursorem" —
//! dostaje ją przez [`Picker`] i zamienia na [`Selection`]. Dzięki temu wymiana
//! bufora ID na raycast po CPU (albo odwrotnie) nie dotyka ani karty inspekcji,
//! ani sterowania czasem.
//!
//! Fallback z ryzyka R8 jest wbudowany, a nie dopisany: [`Selection`] da się ustawić
//! wprost z listy mieszkańców ([`Picker::by_index`]), więc karta inspekcji działa
//! także wtedy, gdy klikanie w świat jeszcze nie działa.

use magnat_core::{BuildingId, CitizenId, Entity, ParcelId};

/// Co jest zaznaczone. Rozszerzanie listy należy do M9 (firmy, pojazdy, sieci).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Selection {
    #[default]
    None,
    Citizen(CitizenId),
    Building(BuildingId),
    Parcel(ParcelId),
}

impl Selection {
    #[must_use]
    pub const fn citizen(self) -> Option<CitizenId> {
        match self {
            Selection::Citizen(c) => Some(c),
            _ => None,
        }
    }

    #[must_use]
    pub const fn is_none(self) -> bool {
        matches!(self, Selection::None)
    }
}

/// Źródło odpowiedzi „co jest pod kursorem".
///
/// Implementuje je klient graficzny (bufor ID renderowany razem z geometrią,
/// decyzja 9.3). `engine/ui` widzi wyłącznie ten interfejs — i dlatego karta
/// inspekcji jest testowalna bez GPU.
pub trait Picker {
    /// Encja pod punktem ekranu albo `None`. Współrzędne w pikselach, od lewego
    /// górnego rogu.
    fn pick(&self, x: u32, y: u32) -> Option<PickResult>;
}

/// Trafienie: co i jak daleko od kamery.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PickResult {
    pub selection: Selection,
    /// Odległość od kamery w metrach — do rozstrzygania, co jest bliżej.
    pub distance_m: f32,
}

/// Picker, który nie widzi niczego — stan przed wpięciem bufora ID i zarazem
/// atrapa w testach.
pub struct NoPicker;

impl Picker for NoPicker {
    fn pick(&self, _x: u32, _y: u32) -> Option<PickResult> {
        None
    }
}

/// Picker po liście mieszkańców — fallback z ryzyka R8 (§8 dokumentu fazy).
///
/// Wyszukiwarka zamiast klikania: karta inspekcji otwiera się po indeksie encji,
/// więc złoty test i przegląd fazy nie czekają na bufor ID w renderze.
pub struct ListPicker {
    citizens: Vec<Entity>,
}

impl ListPicker {
    #[must_use]
    pub fn new(citizens: Vec<Entity>) -> ListPicker {
        ListPicker { citizens }
    }

    /// `n`-ty mieszkaniec z listy.
    #[must_use]
    pub fn by_index(&self, n: usize) -> Selection {
        self.citizens
            .get(n)
            .map_or(Selection::None, |e| Selection::Citizen(CitizenId(*e)))
    }

    /// Mieszkaniec o danym indeksie encji.
    #[must_use]
    pub fn by_entity_index(&self, index: u32) -> Selection {
        self.citizens
            .binary_search_by_key(&index, |e| e.index())
            .ok()
            .map_or(Selection::None, |i| {
                Selection::Citizen(CitizenId(self.citizens[i]))
            })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.citizens.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.citizens.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU32;

    fn encja(i: u32) -> Entity {
        Entity::new(i, NonZeroU32::new(1).unwrap())
    }

    #[test]
    fn wybor_po_indeksie_daje_citizen_id() {
        let p = ListPicker::new(vec![encja(3), encja(7), encja(11)]);
        assert_eq!(p.by_index(1).citizen(), Some(CitizenId(encja(7))));
        assert_eq!(p.by_entity_index(11).citizen(), Some(CitizenId(encja(11))));
        assert!(p.by_entity_index(4).is_none());
        assert!(p.by_index(99).is_none());
    }

    #[test]
    fn brak_pickera_nie_zaznacza_niczego() {
        assert!(NoPicker.pick(10, 10).is_none());
        assert!(Selection::default().is_none());
    }
}
