//! Co dzieje się z towarem bez niczyjego udziału: psucie i scalanie partii (M6a WP2).
//!
//! Dwie operacje o przeciwnym wpływie na bilans i dlatego stojące obok siebie: psucie
//! **zdejmuje** masę i musi ją zaksięgować z kategorią ([`LossKind::Expired`]), scalanie
//! nie zmienia ani masy, ani kosztu i musi tego dowieść. Obie są wołane cyklicznie —
//! `SpoilageSystem` w M6b i `BatchCoalesceSystem` w M6e — więc obie tu zostaną.

use magnat_core::{LossKind, Mass, Money, SimMinute, Volume, Q};

use super::{klucz_fefo, SlotId, Store};

/// Jeden odpis terminu: co, gdzie i za ile zeszło z bilansu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Spoiled {
    pub slot: SlotId,
    pub good: magnat_core::GoodId,
    pub mass: Mass,
    pub cost: Money,
}
use crate::batch::{BatchId, BatchOrigin, CoalesceKey};

impl Store {
    /// Usuwa partie przeterminowane i księguje ich masę jako [`LossKind::Expired`].
    /// Zwraca **listę odpisów**: slot, towar, masa i koszt własny.
    ///
    /// Lista, a nie sama liczba, bo odpis dotyka pieniądza i ktoś musi go zaksięgować
    /// w rachunku wyniku właściciela slotu — a `sim/supply` księgi nie widzi i widzieć
    /// nie może. To jest ten sam wzorzec, którym rynek B2B oddaje `Settlement`,
    /// a zakład fakturę za media: **fakty wychodzą listą, księguje ten, kto ma księgę**
    /// (`AI-1`). Do M6c zwracana była liczba partii i nie dawało się z niej nic policzyć.
    ///
    /// Wołane **przed** sprzedażą — kolejność `SpoilageSystem` przed `RetailSystem`
    /// jest kontraktem międzyfazowym (M6 `D12`), a nie przypadkiem: to na niej stoi
    /// `prop_no_expired_on_shelf`.
    pub fn spoil(&mut self, now: SimMinute) -> Vec<Spoiled> {
        let mut usuniete = Vec::new();
        for i in 0..self.slots.len() {
            let przeterminowane: Vec<BatchId> = self.slots[i]
                .batches
                .iter()
                .copied()
                .filter(|b| self.batches.get(*b).is_some_and(|b| b.expired_at(now)))
                .collect();
            for id in przeterminowane {
                let Some(b) = self.batches.remove(id) else {
                    continue;
                };
                self.slots[i].batches.retain(|x| *x != id);
                self.slots[i].used_mass = Mass(self.slots[i].used_mass.0 - b.mass.0);
                self.slots[i].used_volume = Volume(self.slots[i].used_volume.0 - b.volume.0);
                self.mass[b.good.0 as usize].losses[LossKind::Expired.as_index()] += b.mass.0;
                self.write_offs = Money(self.write_offs.0 + b.cost_total.0);
                usuniete.push(Spoiled {
                    slot: SlotId(i as u32),
                    good: b.good,
                    mass: b.mass,
                    cost: b.cost_total,
                });
            }
        }
        usuniete
    }

    /// Scala partie o zgodnym kluczu agregacji w jednym slocie (WP15 woła to codziennie;
    /// tutaj jest sama operacja).
    ///
    /// Jakość jako średnia ważona masą, `expires_at` jako **minimum** — konserwatywnie,
    /// bo przedłużanie przydatności byłoby tworzeniem świeżości z niczego. Koszt sumuje
    /// się dokładnie, bez zaokrągleń.
    pub fn merge_in_slot(&mut self, slot: SlotId) -> usize {
        let Some(sl) = self.slots.get(slot.0 as usize) else {
            return 0;
        };
        let mut grupy: Vec<(CoalesceKey, Vec<BatchId>)> = Vec::new();
        for id in &sl.batches {
            let Some(b) = self.batches.get(*id) else {
                continue;
            };
            let Some(k) = b.coalesce_key() else { continue };
            match grupy.iter_mut().find(|(g, _)| *g == k) {
                Some((_, v)) => v.push(*id),
                None => grupy.push((k, vec![*id])),
            }
        }
        let mut scalone = 0;
        for (_, v) in grupy.into_iter().filter(|(_, v)| v.len() > 1) {
            let glowna = v[0];
            let mut masa = 0i64;
            let mut jakosc = 0i128;
            let mut koszt = 0i64;
            let mut objetosc = 0i64;
            let mut qty = 0i64;
            let mut expires: Option<SimMinute> = None;
            let mut depth = 0u8;
            for id in &v {
                let b = self.batches.get(*id).expect("partia z grupy żyje");
                masa += b.mass.0;
                jakosc += i128::from(b.quality.get()) * i128::from(b.mass.0);
                koszt += b.cost_total.0;
                objetosc += b.volume.0;
                qty += b.qty.0;
                depth = depth.max(b.origin.depth);
                expires = match (expires, b.expires_at) {
                    (None, e) => e,
                    (Some(a), Some(c)) => Some(SimMinute(a.0.min(c.0))),
                    (Some(a), None) => Some(a),
                };
            }
            // Pochodzenie zachowuje się tylko wtedy, gdy jest **wspólne** dla całej grupy.
            // Inaczej scalona partia twierdziłaby, że przyszła z zakładu, z którego przyszła
            // tylko jej część — a zmyślony ślad jest gorszy od braku śladu (M6 §6.4.2).
            let wzor = self.batches.get(glowna).expect("partia główna żyje").origin;
            let mut site = wzor.site;
            let mut recipe = wzor.recipe;
            let mut deposit = wzor.deposit;
            for id in &v {
                let o = self.batches.get(*id).expect("partia z grupy żyje").origin;
                if o.site != wzor.site {
                    site = None;
                }
                if o.recipe != wzor.recipe {
                    recipe = None;
                }
                if o.deposit != wzor.deposit {
                    deposit = None;
                }
            }
            let b = self.batches.get_mut(glowna).expect("partia główna żyje");
            b.mass = Mass(masa);
            b.qty = magnat_core::Qty(qty);
            b.volume = Volume(objetosc);
            b.quality = Q::new(if masa > 0 {
                (jakosc / i128::from(masa)) as u8
            } else {
                0
            });
            b.cost_total = Money(koszt);
            b.expires_at = expires;
            b.origin = BatchOrigin {
                site,
                recipe,
                depth,
                deposit,
            };
            for id in v.iter().skip(1) {
                self.batches.remove(*id);
                self.slots[slot.0 as usize].batches.retain(|x| x != id);
                scalone += 1;
            }
        }
        if scalone > 0 {
            let arena = &self.batches;
            self.slots[slot.0 as usize]
                .batches
                .sort_by_key(|b| klucz_fefo(arena, *b));
        }
        scalone
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests_support::{draft, magazyn, CHLEB};
    use crate::store::MassIn;

    /// Masa znikająca bez kategorii jest błędem testu, nie zaokrągleniem: przeterminowany
    /// chleb schodzi z magazynu **jako `LossKind::Expired`**, a bilans dalej się domyka.
    #[test]
    fn psucie_ksieguje_strate_z_kategoria() {
        let (cat, mut s, slot) = magazyn();
        s.put(&cat, slot, draft(CHLEB, 5000, 700, 0), MassIn::Produced)
            .expect("wstawienie");
        assert!(s.spoil(SimMinute(1439)).is_empty(), "przed datą nic nie znika");
        assert_eq!(s.spoil(SimMinute(1440)).len(), 1, "w dacie partia schodzi");
        assert_eq!(s.total_stock(CHLEB), Mass::ZERO);
        assert_eq!(s.losses(CHLEB, LossKind::Expired), Mass(5000));
        s.check_mass(CHLEB).expect("bilans masy");
        s.check_cost().expect("bilans pieniądza");
        assert_eq!(s.free_capacity(slot).0, Mass(10_000_000));
    }

    /// Scalanie sumuje masę i koszt dokładnie, jakość bierze jako średnią ważoną,
    /// a datę przydatności jako **minimum** — nigdy nie przedłuża świeżości.
    #[test]
    fn scalanie_nie_przedluza_swiezosci_i_nie_gubi_grosza() {
        let (cat, mut s, slot) = magazyn();
        let mut a = draft(CHLEB, 1000, 333, 0);
        a.quality = Q::new(40);
        let mut b = draft(CHLEB, 3000, 667, 60);
        b.quality = Q::new(44);
        s.put(&cat, slot, a, MassIn::Produced).expect("a");
        s.put(&cat, slot, b, MassIn::Produced).expect("b");
        assert_eq!(s.live_batches(), 2);

        assert_eq!(s.merge_in_slot(slot), 1);
        assert_eq!(s.live_batches(), 1);
        let (_, scalona) = s.batches.iter().next().expect("scalona partia");
        assert_eq!(scalona.mass, Mass(4000));
        assert_eq!(scalona.cost_total, Money(1000));
        assert_eq!(scalona.quality, Q::new(43)); // (40*1000 + 44*3000) / 4000
        assert_eq!(
            scalona.expires_at,
            Some(SimMinute(1440)),
            "minimum, nie maksimum"
        );
        s.check_mass(CHLEB).expect("bilans masy");
        s.check_cost().expect("bilans pieniądza");
    }
}
