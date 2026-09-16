//! Półka i zaplecze sklepu — strona magazynowa migracji WP11 (M6 §6.3).
//!
//! Do M6c sklep był **jedynym sankcjonowanym wyjątkiem od zasady zachowania masy**:
//! `ExternalSupplier` tworzył towar z niczego po cenie z katalogu. Ten plik jest tym,
//! co zajmuje jego miejsce po stronie magazynu — trzy operacje i ani jednej linijki
//! polityki, bo polityka (ile wyłożyć, kiedy zamówić, po ile sprzedać) należy do M5.
//!
//! Trzy rzeczy, które warto rozróżnić, bo wyglądają podobnie:
//!
//! 1. [`Store::move_within_site`] **nie rusza bilansu**. Towar przeniesiony z zaplecza
//!    na półkę nie został ani zużyty, ani wyprodukowany — zmienił półkę w tym samym
//!    zakładzie. To jest ta sama klasa ruchu co magazyn → linia → magazyn wyjściowy
//!    i `prop_no_teleport` dopuszcza ją wprost.
//! 2. [`Store::shelf_pick`] **rusza bilansu**: sprzedaż jest zużyciem i tak się księguje,
//!    razem z kosztem własnym wyliczonym z partii, a nie ze średniej z katalogu.
//! 3. [`Store::shelf_state`] niczego nie rusza — to odczyt dla oferty i dla panelu.

use magnat_core::{GoodId, Mass, Money, Q, SimMinute, Volume};

use super::{wstaw_fefo, BatchSlice, SlotId, Store, WarehouseRole};
use crate::batch::{BatchFlags, BatchId, BatchLocation, BrandId};
use crate::catalog::Catalog;

/// Co widać na półce — wejście oferty detalicznej M5 i karty sklepu.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ShelfState {
    pub mass: Mass,
    /// Średnia ważona masą. `Q::MIN` przy pustej półce, bo „nie ma" nie ma jakości.
    pub quality: Q,
    pub brand: Option<BrandId>,
    /// Najwcześniejsza data przydatności — po niej idzie przecena i odpis.
    pub expires_at: Option<SimMinute>,
    /// Koszt nabycia towaru leżącego na półce. Wejście `PriceRule`: od WP11 marża
    /// liczy się od kosztu **partii**, a nie od stałej z katalogu.
    pub cost_total: Money,
}

impl Store {
    /// Przenosi do `mass` gramów towaru między slotami **tego samego zakładu**,
    /// w porządku FEFO. Zwraca faktycznie przeniesioną masę — mniejszą, gdy w źródle
    /// nie było tyle albo gdy cel się zapełnił.
    ///
    /// Zwraca zero, jeśli sloty należą do różnych zakładów: to byłaby teleportacja,
    /// a między zakładami jedzie się zleceniem transportowym i nie ma od tego wyjątku.
    pub fn move_within_site(
        &mut self,
        cat: &Catalog,
        from: SlotId,
        to: SlotId,
        good: GoodId,
        mass: Mass,
    ) -> Mass {
        if from == to || mass.0 <= 0 {
            return Mass::ZERO;
        }
        let (Some(zrodlo), Some(cel)) = (
            self.slots.get(from.0 as usize),
            self.slots.get(to.0 as usize),
        ) else {
            return Mass::ZERO;
        };
        if zrodlo.site != cel.site {
            return Mass::ZERO;
        }
        let hazard = cat.good(good).hazard;
        if hazard.needs_permit() && cel.hazard_mask & hazard.bit() == 0 {
            return Mass::ZERO;
        }

        // Plan: które partie i ile z każdej, w porządku FEFO slotu źródłowego.
        let mut plan: Vec<(BatchId, Mass)> = Vec::new();
        let mut zostalo = mass.0;
        for id in &zrodlo.batches {
            if zostalo <= 0 {
                break;
            }
            let Some(b) = self.batches.get(*id) else {
                continue;
            };
            if b.good != good || b.flags.has(BatchFlags::QUARANTINED) {
                continue;
            }
            let bierz = b.mass.0.min(zostalo);
            plan.push((*id, Mass(bierz)));
            zostalo -= bierz;
        }

        let mut przeniesione = 0i64;
        for (id, ile) in plan {
            let cala = self.batches.get(id).is_some_and(|b| b.mass == ile);
            let Some(uchwyt) = (if cala { Some(id) } else { self.split(id, ile) }) else {
                continue;
            };
            let Some(b) = self.batches.get(uchwyt) else {
                continue;
            };
            let (m, v) = (b.mass, b.volume);
            let cel = &self.slots[to.0 as usize];
            if cel.used_mass.0 + m.0 > cel.cap_mass.0 || cel.used_volume.0 + v.0 > cel.cap_volume.0
            {
                // Cel pełny. Partia zostaje tam, gdzie była — połowicznie przeniesiona
                // partia nie istnieje, a `split` już oddzielił dokładnie tę masę,
                // więc źródło jest spójne.
                break;
            }
            let zrodlo = &mut self.slots[from.0 as usize];
            zrodlo.batches.retain(|x| *x != uchwyt);
            zrodlo.used_mass = Mass(zrodlo.used_mass.0 - m.0);
            zrodlo.used_volume = Volume(zrodlo.used_volume.0 - v.0);
            let cel = &mut self.slots[to.0 as usize];
            cel.used_mass = Mass(cel.used_mass.0 + m.0);
            cel.used_volume = Volume(cel.used_volume.0 + v.0);
            wstaw_fefo(&mut cel.batches, &self.batches, uchwyt);
            if let Some(b) = self.batches.get_mut(uchwyt) {
                b.location = BatchLocation::Slot(to);
            }
            let cel_site = self.site_of(to);
            self.zapisz(uchwyt, crate::batch::TraceKind::Shelved, cel_site);
            przeniesione += m.0;
        }
        Mass(przeniesione)
    }

    /// Wyłożenie towaru z zaplecza na półkę (§6.3 krok 3). Nazwa z kontraktu §6.1;
    /// treść to [`Store::move_within_site`] z jawnie nazwanymi rolami.
    pub fn backroom_to_shelf(
        &mut self,
        cat: &Catalog,
        backroom: SlotId,
        shelf: SlotId,
        good: GoodId,
        mass: Mass,
    ) -> Mass {
        debug_assert!(self
            .slot(backroom)
            .is_some_and(|s| s.role == WarehouseRole::Backroom));
        debug_assert!(self.slot(shelf).is_some_and(|s| s.role == WarehouseRole::Shelf));
        self.move_within_site(cat, backroom, shelf, good, mass)
    }

    /// Sprzedaż z półki (§6.3 krok 5). FEFO, więc klient dostaje to, co leży najkrócej
    /// do daty — i to stąd transakcja M5 bierze **jakość, markę i koszt własny**,
    /// zamiast czytać średnią z katalogu.
    ///
    /// `None`, gdy na półce nie ma tyle towaru. Sprzedaż częściowa jest decyzją
    /// sklepu, nie magazynu — M5 pyta najpierw [`Store::shelf_state`].
    pub fn shelf_pick(&mut self, shelf: SlotId, good: GoodId, mass: Mass) -> Option<BatchSlice> {
        if mass.0 <= 0 {
            return None;
        }
        let r = self.reserve(shelf, good, mass, Q::MIN)?;
        self.take_as(r, crate::store::TakeKind::Sell).ok()
    }

    /// Stan półki dla jednego towaru — masa, jakość, marka, najwcześniejsza data
    /// i koszt nabycia.
    #[must_use]
    pub fn shelf_state(&self, shelf: SlotId, good: GoodId) -> ShelfState {
        let Some(sl) = self.slots.get(shelf.0 as usize) else {
            return ShelfState::default();
        };
        let mut st = ShelfState::default();
        let mut jakosc = 0i128;
        for id in &sl.batches {
            let Some(b) = self.batches.get(*id) else {
                continue;
            };
            if b.good != good || b.flags.has(BatchFlags::QUARANTINED) {
                continue;
            }
            st.mass = Mass(st.mass.0 + b.mass.0);
            st.cost_total = Money(st.cost_total.0 + b.cost_total.0);
            jakosc += i128::from(b.quality.get()) * i128::from(b.mass.0);
            if st.brand.is_none() {
                st.brand = b.brand;
            }
            st.expires_at = match (st.expires_at, b.expires_at) {
                (None, e) => e,
                (Some(a), Some(c)) => Some(SimMinute(a.0.min(c.0))),
                (Some(a), None) => Some(a),
            };
        }
        if st.mass.0 > 0 {
            st.quality = Q::new((jakosc / i128::from(st.mass.0)) as u8);
        }
        st
    }
}

impl Store {
    /// Wstawia z powrotem partię zdjętą przez [`Store::shelf_pick`], odkręcając
    /// zużycie i koszt własny.
    ///
    /// Istnieje dla jednej ścieżki i to jest cała jej racja bytu: sprzedaż zdejmuje
    /// towar z półki **przed** rozliczeniem pieniądza, a nieudany przelew musi oddać
    /// dokładnie to, co wzięła — z jakością, marką, datą i pochodzeniem. Zwrot samej
    /// masy zostawiłby świeżość i ślad zmyślone, a bilans pieniądza rozjechany
    /// o koszt własny.
    ///
    /// Nie idzie przez [`Store::put`], bo `put` **dolicza masę do bilansu** jako nową —
    /// a ta masa z bilansu nigdy nie wyszła, wyszła tylko z półki. Stąd własna droga
    /// i jawne odkręcenie `consumed`/`cogs`.
    ///
    /// Zwraca `false`, gdy slot nie przyjmuje (pojemność, klasa niebezpieczeństwa);
    /// wtedy towar **nie wraca** i wołający ma o tym wiedzieć.
    pub fn return_taken(&mut self, cat: &Catalog, slot: SlotId, slice: &BatchSlice) -> bool {
        if slice.mass.0 <= 0 {
            return false;
        }
        let good = cat.good(slice.good);
        let volume = good.volume_of(slice.mass);
        let Some(sl) = self.slots.get(slot.0 as usize) else {
            return false;
        };
        if good.hazard.needs_permit() && sl.hazard_mask & good.hazard.bit() == 0 {
            return false;
        }
        if sl.used_mass.0 + slice.mass.0 > sl.cap_mass.0
            || sl.used_volume.0 + volume.0 > sl.cap_volume.0
        {
            return false;
        }
        let batch = crate::batch::Batch {
            good: slice.good,
            mass: slice.mass,
            qty: good.units_of_mass(slice.mass),
            volume,
            quality: slice.quality,
            brand: slice.brand,
            producer: slice.producer,
            produced_at: SimMinute(0),
            expires_at: slice.expires_at,
            cost_total: slice.cost_total,
            origin: slice.origin,
            location: BatchLocation::Slot(slot),
            flags: BatchFlags::default(),
        };
        let id = self.batches.insert(batch);
        let sl = &mut self.slots[slot.0 as usize];
        sl.used_mass = Mass(sl.used_mass.0 + slice.mass.0);
        sl.used_volume = Volume(sl.used_volume.0 + volume.0);
        wstaw_fefo(&mut sl.batches, &self.batches, id);
        self.mass[slice.good.0 as usize].consumed -= slice.mass.0;
        self.cogs = Money(self.cogs.0 - slice.cost_total.0);
        true
    }
}
