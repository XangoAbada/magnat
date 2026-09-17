//! Pokrycie usługami publicznymi — jedna tablica, którą czytają trzy fazy (M8d WP7).
//!
//! # Dlaczego to stoi w `core`, skoro liczy to `sim/city`
//!
//! Ten sam powód, dla którego w `core` stoi `OpenHours` (`K-34`): to jest **struktura
//! danych bez logiki**, a czytelnicy stoją w grafie zależności **poniżej** tego, kto ją
//! wypełnia. Szkoła zmienia tempo nauki dziecka (`sim/agents`), posterunek zmienia
//! straty inwentaryzacyjne sklepu (`sim/economy`), szpital skraca chorobę
//! (`sim/agents`) — a żaden z tych crate'ów nie może zależeć od `sim/city`, bo
//! `sim/city` zależy od nich.
//!
//! Rozwiązanie jest tym samym wzorcem, którym `sim/events` nakłada parametry
//! (`CE-4`): **piszący sięga do pola czytelnika**, a nie odwrotnie. Tu jest o tyle
//! prościej, że pole jest jedno i wspólne — zasób świata, który `sim/city` wypełnia
//! raz w miesiącu, a reszta tylko czyta.
//!
//! `core` nie dostaje przy tym ani jednej reguły: jak liczy się jakość placówki
//! i jak zanika z odległością, wie wyłącznie `sim/city`.

use crate::hash::{HashState, StateHasher};
use crate::types::{DistrictId, Q};
use crate::vocab::{ServiceKind, SERVICE_KIND_COUNT};

/// Jakość usług publicznych widziana z dzielnicy, w skali `Q` (0..=100).
///
/// Wiersz na dzielnicę, kolumna na [`ServiceKind`]. Zero znaczy „w tej dzielnicy
/// ta usługa nie dociera", a nie „brak danych" — i to jest celowe: mieszkaniec
/// dzielnicy bez szkoły ma zerowe pokrycie edukacyjne, a nie nieznane.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct ServiceCoverage {
    rows: Vec<[u8; SERVICE_KIND_COUNT]>,
}

impl ServiceCoverage {
    /// Pokrycie zerowe dla `districts` dzielnic — stan przed pierwszym domknięciem
    /// miesiąca, a także stan świata, w którym miasta jako aktora w ogóle nie ma.
    #[must_use]
    pub fn new(districts: usize) -> ServiceCoverage {
        ServiceCoverage {
            rows: vec![[0u8; SERVICE_KIND_COUNT]; districts],
        }
    }

    #[must_use]
    pub fn districts(&self) -> usize {
        self.rows.len()
    }

    /// Jakość usługi w dzielnicy. Dzielnica spoza zakresu daje zero — świat bez
    /// miasta jako aktora ma pokrycie zerowe, a nie panikę.
    #[must_use]
    pub fn at(&self, district: DistrictId, kind: ServiceKind) -> Q {
        Q::new(self
            .rows
            .get(district.0 as usize)
            .map_or(0, |r| r[kind.as_index()]))
    }

    pub fn set(&mut self, district: DistrictId, kind: ServiceKind, q: Q) {
        if let Some(r) = self.rows.get_mut(district.0 as usize) {
            r[kind.as_index()] = q.get();
        }
    }

    /// Zeruje wszystkie wiersze, zachowując liczbę dzielnic. Wołane na początku
    /// publikacji, żeby dzielnica, która straciła placówkę, spadła do zera zamiast
    /// zostać z wartością sprzed miesiąca.
    pub fn clear(&mut self) {
        for r in &mut self.rows {
            *r = [0u8; SERVICE_KIND_COUNT];
        }
    }

    /// Średnia miejska danej usługi — podstawa raportu i punkt odniesienia testu A/B.
    #[must_use]
    pub fn city_mean(&self, kind: ServiceKind) -> Q {
        if self.rows.is_empty() {
            return Q::MIN;
        }
        let suma: u32 = self
            .rows
            .iter()
            .map(|r| u32::from(r[kind.as_index()]))
            .sum();
        Q::new((suma / self.rows.len() as u32) as u8)
    }
}

impl HashState for ServiceCoverage {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.rows.len() as u64);
        for r in &self.rows {
            for v in r {
                h.write_u8(*v);
            }
        }
    }
}
