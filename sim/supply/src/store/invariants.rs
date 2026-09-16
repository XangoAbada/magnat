//! Niezmienniki magazynu — to, co operacje z [`super::ops`] mają zachowywać (M6a WP2).
//!
//! Mieszkają w crate'cie, a nie w teście, bo pola slotu są prywatne — i dlatego, że są
//! własnością magazynu, nie właściwością jednego scenariusza. Każdy zwraca `Err` z liczbami,
//! a nie `bool`: przy rozjeździe pierwsze pytanie brzmi „o ile", a nie „czy".

use magnat_core::{GoodId, LossKind, Mass, Money};

use super::Store;

impl Store {
    /// rozjeździe pierwsze pytanie brzmi „o ile", a nie „czy".
    pub fn check_mass(&self, good: GoodId) -> Result<(), (i64, i64)> {
        let row = &self.mass[good.0 as usize];
        let lewa = row.produced + row.imported + row.initial;
        let prawa =
            row.consumed + row.exported + row.losses.iter().sum::<i64>() + self.total_stock(good).0;
        if lewa == prawa {
            Ok(())
        } else {
            Err((lewa, prawa))
        }
    }

    /// Niezmiennik `prop_no_negative_stock`: po każdym punkcie synchronizacji zapełnienie
    /// slotu mieści się w pojemności, a partia o masie 0 nie zostaje jako duch.
    ///
    /// Mieszka w crate'cie, a nie w teście, bo pola slotu są prywatne — i dlatego, że
    /// to jest niezmiennik magazynu, nie właściwość jednego scenariusza.
    pub fn check_no_negative(&self) -> Result<(), String> {
        for (i, s) in self.slots.iter().enumerate() {
            if s.used_mass.0 < 0 || s.used_mass.0 > s.cap_mass.0 {
                return Err(format!(
                    "slot {i}: masa {} poza [0; {}]",
                    s.used_mass.0, s.cap_mass.0
                ));
            }
            if s.used_volume.0 < 0 || s.used_volume.0 > s.cap_volume.0 {
                return Err(format!(
                    "slot {i}: objętość {} poza [0; {}]",
                    s.used_volume.0, s.cap_volume.0
                ));
            }
            let suma: i64 = s
                .batches
                .iter()
                .filter_map(|b| self.batches.get(*b))
                .map(|b| b.mass.0)
                .sum();
            if suma != s.used_mass.0 {
                return Err(format!(
                    "slot {i}: suma partii {suma} g wobec zapisanych {} g",
                    s.used_mass.0
                ));
            }
            for b in &s.batches {
                if !self.batches.contains(*b) {
                    return Err(format!("slot {i}: martwy uchwyt na liście"));
                }
            }
        }
        for (h, b) in self.batches.iter() {
            if b.mass.0 <= 0 {
                return Err(format!("partia {} ma masę {}", h.index(), b.mass.0));
            }
        }
        Ok(())
    }

    /// Niezmiennik pieniądza: suma kosztów żywych partii równa się zapłaconemu minus
    /// rozliczony COGS minus odpisy strat. Wiąże bilans masy z bilansem pieniądza
    /// (dok. 00 §6) i wykrywa gubienie groszy przy podziale partii.
    pub fn check_cost(&self) -> Result<(), (Money, Money)> {
        let w_partiach: i64 = self.batches.iter().map(|(_, b)| b.cost_total.0).sum();
        let oczekiwane = self.paid_in.0 - self.cogs.0 - self.write_offs.0;
        if w_partiach == oczekiwane {
            Ok(())
        } else {
            Err((Money(w_partiach), Money(oczekiwane)))
        }
    }

    /// Straty danego rodzaju dla towaru — do panelu zakładu i do testu bilansu.
    #[must_use]
    pub fn losses(&self, good: GoodId, kind: LossKind) -> Mass {
        Mass(self.mass[good.0 as usize].losses[kind.as_index()])
    }
}
