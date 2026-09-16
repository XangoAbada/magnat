//! Operacje magazynowe: wstawienie, rezerwacja, wydanie FEFO (M6a §5.2, WP2).
//!
//! Każda z nich rusza **jednocześnie** slot i arenę partii — i to jest powód, dla którego
//! obie te rzeczy mieszkają w jednym zasobie (`AD-7` w dokumencie fazy).
//!
//! To, co dzieje się z towarem **bez niczyjego udziału** — psucie i scalanie — jest obok,
//! w [`super::aging`]: tam masa schodzi z bilansu z kategorią albo nie zmienia się wcale,
//! tu zawsze ktoś ją kładzie albo bierze.

use magnat_core::{split_proportional, GoodId, Mass, Money, SimMinute, Volume, Q};

use super::{
    qty_of, wstaw_fefo, BatchDraft, BatchSlice, MassIn, Reservation, SlotId, Store, StoreError,
};
use crate::batch::{Batch, BatchFlags, BatchId, BatchLocation};
use crate::catalog::Catalog;

impl Store {
    pub fn put(
        &mut self,
        cat: &Catalog,
        slot: SlotId,
        draft: BatchDraft,
        source: MassIn,
    ) -> Result<BatchId, StoreError> {
        let good = cat.good(draft.good);
        let sl = self
            .slots
            .get(slot.0 as usize)
            .ok_or(StoreError::UnknownSlot)?;

        if good.hazard.needs_permit() && sl.hazard_mask & good.hazard.bit() == 0 {
            return Err(StoreError::HazardNotAllowed);
        }
        let volume = good.volume_of(draft.mass);
        if sl.used_mass.0 + draft.mass.0 > sl.cap_mass.0
            || sl.used_volume.0 + volume.0 > sl.cap_volume.0
        {
            return Err(StoreError::OutOfCapacity);
        }

        let expires_at = good
            .shelf_life_minutes
            .map(|m| SimMinute(draft.produced_at.0 + u64::from(m)));
        let quality = if good.has_quality {
            draft.quality
        } else {
            Q::new(50)
        };
        let b = Batch {
            good: draft.good,
            mass: draft.mass,
            qty: qty_of(cat, draft.good, draft.mass),
            volume,
            quality,
            brand: draft.brand,
            producer: draft.producer,
            produced_at: draft.produced_at,
            expires_at,
            cost_total: draft.cost,
            origin: draft.origin,
            location: BatchLocation::Slot(slot),
            flags: draft.flags,
        };
        let sledzona = b.flags.has(BatchFlags::TRACED);
        let id = self.batches.insert(b);

        let sl = &mut self.slots[slot.0 as usize];
        sl.used_mass = Mass(sl.used_mass.0 + draft.mass.0);
        sl.used_volume = Volume(sl.used_volume.0 + volume.0);
        wstaw_fefo(&mut sl.batches, &self.batches, id);

        let row = &mut self.mass[draft.good.0 as usize];
        match source {
            MassIn::Produced => row.produced += draft.mass.0,
            MassIn::Imported => row.imported += draft.mass.0,
            MassIn::Initial => row.initial += draft.mass.0,
        }
        self.paid_in = Money(self.paid_in.0 + draft.cost.0);

        if sledzona {
            self.zapisz(id, draft.produced_at, crate::batch::TraceKind::Produced);
        }
        Ok(id)
    }

    /// Ile towaru **da się** wydać ze slotu przy tym progu jakości — bez rezerwowania
    /// czegokolwiek.
    ///
    /// Istnieje, bo linia produkcyjna musi sprawdzić **wszystkie** wejścia, zanim
    /// weźmie którekolwiek: rezerwacja trzech wejść i porażka na czwartym zostawiłaby
    /// trzy partie oznaczone i zjedzony wsad, a szarża i tak by nie ruszyła.
    #[must_use]
    pub fn available(&self, slot: SlotId, good: GoodId, min_q: Q) -> Mass {
        let Some(sl) = self.slots.get(slot.0 as usize) else {
            return Mass::ZERO;
        };
        Mass(
            sl.batches
                .iter()
                .filter_map(|b| self.batches.get(*b))
                .filter(|b| {
                    b.good == good && b.quality >= min_q && !b.flags.has(BatchFlags::QUARANTINED)
                })
                .map(|b| b.mass.0)
                .sum(),
        )
    }

    /// Odpisuje masę ze slotu z podaną kategorią straty — złom przezbrojenia, ubytek
    /// w magazynie, towar zniszczony kontrolą. Zwraca masę faktycznie odpisaną.
    ///
    /// Osobno od [`Store::take`], bo `take` księguje `consumed` (towar poszedł dalej
    /// w łańcuch), a to jest `losses` (towar zszedł z bilansu). Pomylenie tych dwóch
    /// domknęłoby bilans i skłamało w rachunku wyniku.
    pub fn write_off(
        &mut self,
        slot: SlotId,
        good: GoodId,
        mass: Mass,
        kind: magnat_core::LossKind,
    ) -> Mass {
        let Some(r) = self.reserve(slot, good, mass, Q::MIN) else {
            // Mniej niż żądano — odpisujemy tyle, ile jest.
            let jest = self.available(slot, good, Q::MIN);
            if jest.0 <= 0 {
                return Mass::ZERO;
            }
            return self.write_off(slot, good, jest, kind);
        };
        let Ok(kawalek) = self.take(r) else {
            return Mass::ZERO;
        };
        // `take` zaksięgowało to jako zużycie i COGS — a to była strata. Przeksięgowanie
        // w jednym miejscu jest tańsze niż drugi wariant `take` z flagą.
        self.mass[good.0 as usize].consumed -= kawalek.mass.0;
        self.mass[good.0 as usize].losses[kind.as_index()] += kawalek.mass.0;
        self.cogs = Money(self.cogs.0 - kawalek.cost_total.0);
        self.write_offs = Money(self.write_offs.0 + kawalek.cost_total.0);
        kawalek.mass
    }

    /// Wydaje masę **poza miasto** — eksport przez węzeł graniczny (M6c §5.9).
    /// Zwraca `(masa, koszt własny)`; masę wypuszczoną i jej koszt księguje wołający.
    ///
    /// Osobna operacja od [`Store::take`] i od [`Store::write_off`] z tego samego powodu,
    /// dla którego tamte dwie są osobne: bilans masy (`00` §6) rozróżnia trzy wyjścia
    /// i tylko jedno z nich jest eksportem. Towar zjedzony przez linię to `consumed`,
    /// zepsuty to `losses`, a sprzedany za granicę to `exported` — zlanie ich domknęłoby
    /// bilans i skłamało w rachunku, bo za eksport ktoś zapłacił.
    pub fn export(&mut self, slot: SlotId, good: GoodId, mass: Mass) -> (Mass, Money) {
        let Some(r) = self.reserve(slot, good, mass, Q::MIN) else {
            let jest = self.available(slot, good, Q::MIN);
            if jest.0 <= 0 {
                return (Mass::ZERO, Money::ZERO);
            }
            return self.export(slot, good, jest);
        };
        let Ok(kawalek) = self.take(r) else {
            return (Mass::ZERO, Money::ZERO);
        };
        // `take` zaksięgowało wydanie jako zużycie w łańcuchu. To było wyjście z miasta.
        self.mass[good.0 as usize].consumed -= kawalek.mass.0;
        self.mass[good.0 as usize].exported += kawalek.mass.0;
        (kawalek.mass, kawalek.cost_total)
    }

    /// Plan wydania `mass` gramów towaru ze slotu, w porządku FEFO, pomijając partie
    /// poniżej `min_q` i wstrzymane. `None`, jeśli w slocie nie ma tyle towaru.
    pub fn reserve(
        &mut self,
        slot: SlotId,
        good: GoodId,
        mass: Mass,
        min_q: Q,
    ) -> Option<Reservation> {
        let sl = self.slots.get(slot.0 as usize)?;
        let mut items = Vec::new();
        let mut zostalo = mass.0;
        for id in &sl.batches {
            if zostalo <= 0 {
                break;
            }
            let Some(b) = self.batches.get(*id) else {
                continue;
            };
            if b.good != good || b.quality < min_q || b.flags.has(BatchFlags::QUARANTINED) {
                continue;
            }
            let bierz = b.mass.0.min(zostalo);
            items.push((*id, Mass(bierz)));
            zostalo -= bierz;
        }
        if zostalo > 0 {
            return None;
        }
        for (id, _) in &items {
            if let Some(b) = self.batches.get_mut(*id) {
                b.flags.set(BatchFlags::RESERVED);
            }
        }
        Some(Reservation {
            slot,
            good,
            mass,
            items,
        })
    }

    /// Wydaje zarezerwowaną masę. Partia wyczerpana w całości **znika w tym samym
    /// kroku** — partia o masie 0 nie zostaje jako duch (`prop_no_negative_stock`).
    ///
    /// Koszt dzieli się proporcjonalnie do masy przez [`split_proportional`], więc suma
    /// części równa się kwocie dzielonej **zawsze**, także przy tysiącu podziałów.
    pub fn take(&mut self, r: Reservation) -> Result<BatchSlice, StoreError> {
        for (id, _) in &r.items {
            if !self.batches.contains(*id) {
                return Err(StoreError::StaleReservation);
            }
        }
        let mut masa = 0i64;
        let mut koszt = 0i64;
        let mut jakosc_wazona = 0i128;
        let mut brand = None;
        let mut expires_at: Option<SimMinute> = None;
        let mut origin: Option<crate::batch::BatchOrigin> = None;
        let mut puste: Vec<BatchId> = Vec::new();

        for (id, ile) in &r.items {
            let b = self
                .batches
                .get_mut(*id)
                .ok_or(StoreError::StaleReservation)?;
            // Podział kosztu: [wydane, zostające]. Reszta trafia do pierwszej części
            // wg ustalonego porządku (dok. 00 §2), więc nic nie ginie i nic nie powstaje.
            let czesci = split_proportional(
                b.cost_total,
                &[ile.0.unsigned_abs(), (b.mass.0 - ile.0).unsigned_abs()],
            );
            let koszt_wydany = czesci[0];
            b.cost_total = Money(b.cost_total.0 - koszt_wydany.0);
            b.mass = Mass(b.mass.0 - ile.0);
            b.flags.clear(BatchFlags::RESERVED);

            masa += ile.0;
            koszt += koszt_wydany.0;
            jakosc_wazona += i128::from(b.quality.get()) * i128::from(ile.0);
            if brand.is_none() {
                brand = b.brand;
            }
            expires_at = match (expires_at, b.expires_at) {
                (None, e) => e,
                (Some(a), Some(c)) => Some(SimMinute(a.0.min(c.0))),
                (Some(a), None) => Some(a),
            };
            // Pochodzenie scala się tą samą regułą co przy łączeniu partii: wspólne
            // pole zostaje, różne się zeruje. Zmyślony ślad jest gorszy od braku
            // śladu (M6 §6.4.2) — i to jest jedyne miejsce, w którym można go zmyślić.
            origin = Some(match origin {
                None => b.origin,
                Some(o) => crate::batch::BatchOrigin {
                    site: if o.site == b.origin.site {
                        o.site
                    } else {
                        None
                    },
                    recipe: if o.recipe == b.origin.recipe {
                        o.recipe
                    } else {
                        None
                    },
                    depth: o.depth.max(b.origin.depth),
                    deposit: if o.deposit == b.origin.deposit {
                        o.deposit
                    } else {
                        None
                    },
                },
            });
            if b.mass.0 == 0 {
                puste.push(*id);
            }
        }

        let sl = &mut self.slots[r.slot.0 as usize];
        sl.used_mass = Mass(sl.used_mass.0 - masa);
        for id in &puste {
            sl.batches.retain(|x| x != id);
        }
        for id in puste {
            if let Some(b) = self.batches.remove(id) {
                sl.used_volume = Volume(sl.used_volume.0 - b.volume.0);
            }
        }
        self.przelicz_objetosc(r.slot);

        self.mass[r.good.0 as usize].consumed += masa;
        self.cogs = Money(self.cogs.0 + koszt);

        let jakosc = if masa > 0 {
            (jakosc_wazona / i128::from(masa)) as u8
        } else {
            0
        };
        Ok(BatchSlice {
            good: r.good,
            mass: Mass(masa),
            quality: Q::new(jakosc),
            cost_total: Money(koszt),
            brand,
            expires_at,
            origin: origin.unwrap_or_default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{HazardClass, StorageClass};
    use crate::store::tests_support::{draft, encja, katalog, magazyn, CHLEB, PIASEK};
    use crate::store::WarehouseRole;
    use magnat_core::SiteId;

    /// Kryterium ukończenia WP2: **partia dzielona 1000 razy nie gubi ani grama,
    /// ani grosza.** Kwota jest celowo niepodzielna przez 1000, żeby reszta musiała
    /// gdzieś trafić — i żeby było widać, że trafia dokładnie raz.
    #[test]
    fn tysiac_podzialow_nie_gubi_grama_ani_grosza() {
        let (cat, mut s, slot) = magazyn();
        s.put(
            &cat,
            slot,
            draft(PIASEK, 1_000_000, 999_999, 0),
            MassIn::Produced,
        )
        .expect("wstawienie");

        let mut wydane = 0i64;
        let mut koszt_wydany = 0i64;
        for _ in 0..1000 {
            let r = s
                .reserve(slot, PIASEK, Mass(1000), Q::MIN)
                .expect("rezerwacja");
            let sl = s.take(r).expect("wydanie");
            wydane += sl.mass.0;
            koszt_wydany += sl.cost_total.0;
        }
        assert_eq!(wydane, 1_000_000, "masa wydana");
        assert_eq!(koszt_wydany, 999_999, "koszt wydany co do grosza");
        assert_eq!(s.total_stock(PIASEK), Mass::ZERO);
        assert_eq!(
            s.live_batches(),
            0,
            "partia o masie 0 nie zostaje jako duch"
        );
        s.check_mass(PIASEK).expect("bilans masy");
        s.check_cost().expect("bilans pieniądza");
    }

    /// FEFO: wydaje się to, co psuje się najwcześniej, a nie to, co przyszło pierwsze.
    #[test]
    fn fefo_wydaje_najblizsza_date_a_nie_najstarsza_partie() {
        let (cat, mut s, slot) = magazyn();
        // Chleb upieczony wcześniej, ale o dłuższym życiu, stoi ZA świeższym o krótszym.
        s.put(&cat, slot, draft(CHLEB, 1000, 100, 0), MassIn::Produced)
            .expect("stary");
        s.put(&cat, slot, draft(CHLEB, 1000, 100, 100), MassIn::Produced)
            .expect("nowy");
        let stary_wygasa = SimMinute(1440);

        let r = s
            .reserve(slot, CHLEB, Mass(1000), Q::MIN)
            .expect("rezerwacja");
        let sl = s.take(r).expect("wydanie");
        assert_eq!(sl.expires_at, Some(stary_wygasa));
        assert_eq!(s.total_stock(CHLEB), Mass(1000));
        s.check_mass(CHLEB).expect("bilans masy");
    }

    /// Slot bez maski odmawia przyjęcia ładunku niebezpiecznego — to granica zaufania.
    #[test]
    fn slot_bez_maski_odmawia_paliwa() {
        let cat = katalog();
        let mut cat = cat;
        cat.goods[1].hazard = HazardClass::Flammable;
        let mut s = Store::new(cat.goods.len());
        let zwykly = s.add_slot(
            SiteId(encja(1)),
            WarehouseRole::Input,
            StorageClass::Ambient,
            Mass(1_000_000),
            Volume(10_000_000),
            0,
        );
        let zbiornik = s.add_slot(
            SiteId(encja(1)),
            WarehouseRole::Input,
            StorageClass::Tank,
            Mass(1_000_000),
            Volume(10_000_000),
            HazardClass::Flammable.bit(),
        );
        assert_eq!(
            s.put(&cat, zwykly, draft(PIASEK, 1000, 10, 0), MassIn::Imported),
            Err(StoreError::HazardNotAllowed)
        );
        assert!(s
            .put(&cat, zbiornik, draft(PIASEK, 1000, 10, 0), MassIn::Imported)
            .is_ok());
    }

    #[test]
    fn pojemnosc_jest_twarda_w_obu_wymiarach() {
        let cat = katalog();
        let mut s = Store::new(cat.goods.len());
        let ciasny = s.add_slot(
            SiteId(encja(1)),
            WarehouseRole::Input,
            StorageClass::Ambient,
            Mass(1000),
            Volume(1_000_000),
            0,
        );
        assert!(s
            .put(&cat, ciasny, draft(PIASEK, 1000, 10, 0), MassIn::Imported)
            .is_ok());
        assert_eq!(
            s.put(&cat, ciasny, draft(PIASEK, 1, 1, 0), MassIn::Imported),
            Err(StoreError::OutOfCapacity)
        );
    }
}
