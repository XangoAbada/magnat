//! Partia w drodze: podział, załadunek, rozładunek (M6b §5.6, WP5).
//!
//! Jeden niezmiennik i cały plik jest po to, żeby dało się go udowodnić:
//! **partia zmienia lokację wyłącznie przez zlecenie transportowe albo w obrębie
//! tego samego zakładu** (`prop_no_teleport`, M6 §7.3 pkt 3). Dlatego `location`
//! jest polem prywatnym dla wszystkich poza [`Store`], a jedyne dwie drogi między
//! zakładami to [`Store::load`] i [`Store::unload`] — obie biorą `TransportOrderId`
//! i nie da się ich wywołać bez zlecenia.
//!
//! Partia w drodze **nie znika z bilansu**: zostaje w arenie, więc `total_stock`
//! dalej ją liczy, a wypada tylko z listy slotu i z jego zajętości. To jest właściwe
//! zachowanie — towar na ciężarówce jest zapasem miasta, po prostu nie leży na półce.

use magnat_core::{split_proportional, GoodId, Mass, Money, Volume, Q};

use super::{klucz_fefo, wstaw_fefo, SlotId, Store, StoreError};
use crate::batch::{BatchFlags, BatchId, BatchLocation, TransportOrderId};
use crate::catalog::Catalog;

impl Store {
    /// Dzieli partię: z oryginału schodzi `mass`, powstaje nowa partia o tej masie,
    /// tym samym pochodzeniu i **proporcjonalnym** koszcie. Nowa partia trafia do tego
    /// samego slotu co oryginał.
    ///
    /// Koszt dzieli [`split_proportional`], więc suma obu części równa się kwocie
    /// sprzed podziału **zawsze** — także przy tysiącu podziałów i kwocie niepodzielnej.
    /// Podział na całość albo na zero nie ma sensu i zwraca `None`: partia o masie 0
    /// nie ma prawa istnieć (`prop_no_negative_stock`).
    pub fn split(&mut self, id: BatchId, mass: Mass) -> Option<BatchId> {
        let b = self.batches.get(id)?;
        if mass.0 <= 0 || mass.0 >= b.mass.0 {
            return None;
        }
        let BatchLocation::Slot(slot) = b.location else {
            return None;
        };
        let czesci = split_proportional(
            b.cost_total,
            &[mass.0.unsigned_abs(), (b.mass.0 - mass.0).unsigned_abs()],
        );
        let objetosc_czesci = Volume(podziel_proporcjonalnie(b.volume.0, mass.0, b.mass.0));
        let qty_czesci = magnat_core::Qty(podziel_proporcjonalnie(b.qty.0, mass.0, b.mass.0));
        let mut nowa = b.clone();
        nowa.mass = mass;
        nowa.volume = objetosc_czesci;
        nowa.qty = qty_czesci;
        nowa.cost_total = czesci[0];
        nowa.flags.clear(BatchFlags::RESERVED);

        let stara = self.batches.get_mut(id)?;
        stara.mass = Mass(stara.mass.0 - mass.0);
        stara.volume = Volume(stara.volume.0 - objetosc_czesci.0);
        stara.qty = magnat_core::Qty(stara.qty.0 - qty_czesci.0);
        stara.cost_total = czesci[1];

        let nowy_id = self.batches.insert(nowa);
        let arena = &self.batches;
        let lista = &mut self.slots[slot.0 as usize].batches;
        let k = klucz_fefo(arena, nowy_id);
        let poz = lista.partition_point(|b| klucz_fefo(arena, *b) < k);
        lista.insert(poz, nowy_id);
        // Odłamek dziedziczy ślad: `Batch::clone` przeniósł flagi, więc kawałek partii
        // śledzonej też jest śledzony — a ślad musi wiedzieć, z czego się oddzielił,
        // inaczej „od pola do półki" urywa się na pierwszym załadunku ciężarówki.
        let miejsce = self.site_of(slot);
        self.zapisz(nowy_id, crate::batch::TraceKind::Split, miejsce);
        self.ledger.link(nowy_id, id);
        Some(nowy_id)
    }

    /// Zdejmuje ze slotu `mass` gramów towaru w porządku FEFO i przekazuje je zleceniu.
    /// Ostatnia partia jest dzielona, jeśli trzeba — ładujemy tonę, a nie „tyle, ile
    /// akurat wyszło".
    ///
    /// Zwraca uchwyty ładunku. `None`, jeśli w slocie nie ma tyle towaru — wtedy nic
    /// się nie zmienia, bo dostawa częściowa jest decyzją wołającego, nie magazynu.
    pub fn load(
        &mut self,
        slot: SlotId,
        good: GoodId,
        mass: Mass,
        min_q: Q,
        order: TransportOrderId,
    ) -> Option<Vec<BatchId>> {
        let sl = self.slots.get(slot.0 as usize)?;
        let mut plan: Vec<(BatchId, Mass)> = Vec::new();
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
            plan.push((*id, Mass(bierz)));
            zostalo -= bierz;
        }
        if zostalo > 0 {
            return None;
        }

        let mut ladunek = Vec::with_capacity(plan.len());
        for (id, ile) in plan {
            let cala = self.batches.get(id).is_some_and(|b| b.mass == ile);
            let uchwyt = if cala { id } else { self.split(id, ile)? };
            ladunek.push(uchwyt);
        }
        for id in &ladunek {
            let Some(b) = self.batches.get(*id) else {
                continue;
            };
            let (m, v) = (b.mass, b.volume);
            let sl = &mut self.slots[slot.0 as usize];
            sl.batches.retain(|x| x != id);
            sl.used_mass = Mass(sl.used_mass.0 - m.0);
            sl.used_volume = Volume(sl.used_volume.0 - v.0);
            if let Some(b) = self.batches.get_mut(*id) {
                b.location = BatchLocation::InTransit(order);
                b.flags.clear(BatchFlags::RESERVED);
            }
            let skad = self.site_of(slot);
            self.zapisz(*id, crate::batch::TraceKind::Departed, skad);
        }
        Some(ladunek)
    }

    /// Wstawia ładunek zlecenia do slotu odbiorcy. Partia, która nie mieści się
    /// w pojemności, **zostaje w drodze** i wraca w wyniku — to jest sygnał
    /// „rampa przyjęła, magazyn nie", a nie cicha utrata towaru.
    ///
    /// Sprawdza, że każdy uchwyt naprawdę należy do tego zlecenia: rozładowanie cudzego
    /// ładunku byłoby teleportacją z poprawnym numerem w papierach.
    pub fn unload(
        &mut self,
        cat: &Catalog,
        order: TransportOrderId,
        cargo: &[BatchId],
        slot: SlotId,
    ) -> Result<Vec<BatchId>, StoreError> {
        if self.slots.len() <= slot.0 as usize {
            return Err(StoreError::UnknownSlot);
        }
        let mut odrzucone = Vec::new();
        for id in cargo {
            let Some(b) = self.batches.get(*id) else {
                continue;
            };
            if b.location != BatchLocation::InTransit(order) {
                return Err(StoreError::StaleReservation);
            }
            let (m, v, hazard) = (b.mass, b.volume, cat.good(b.good).hazard);
            let sl = &self.slots[slot.0 as usize];
            if hazard.needs_permit() && sl.hazard_mask & hazard.bit() == 0 {
                odrzucone.push(*id);
                continue;
            }
            if sl.used_mass.0 + m.0 > sl.cap_mass.0 || sl.used_volume.0 + v.0 > sl.cap_volume.0 {
                odrzucone.push(*id);
                continue;
            }
            let sl = &mut self.slots[slot.0 as usize];
            sl.used_mass = Mass(sl.used_mass.0 + m.0);
            sl.used_volume = Volume(sl.used_volume.0 + v.0);
            wstaw_fefo(&mut sl.batches, &self.batches, *id);
            if let Some(b) = self.batches.get_mut(*id) {
                b.location = BatchLocation::Slot(slot);
            }
            let dokad = self.site_of(slot);
            self.zapisz(*id, crate::batch::TraceKind::Unloaded, dokad);
        }
        Ok(odrzucone)
    }

    /// Ubytek w transporcie: masa schodzi z partii i księguje się jako
    /// [`LossKind::TransportDamage`](magnat_core::LossKind). Partia zjedzona w całości
    /// znika w tym samym kroku.
    pub fn damage_in_transit(
        &mut self,
        id: BatchId,
        mass: Mass,
        kind: magnat_core::LossKind,
    ) -> Mass {
        let Some(b) = self.batches.get_mut(id) else {
            return Mass::ZERO;
        };
        let ile = mass.0.min(b.mass.0).max(0);
        if ile == 0 {
            return Mass::ZERO;
        }
        let czesci = split_proportional(
            b.cost_total,
            &[ile.unsigned_abs(), (b.mass.0 - ile).unsigned_abs()],
        );
        let (good, odpis) = (b.good, czesci[0]);
        b.mass = Mass(b.mass.0 - ile);
        b.cost_total = Money(b.cost_total.0 - odpis.0);
        let pusta = b.mass.0 == 0;
        self.mass[good.0 as usize].losses[kind.as_index()] += ile;
        self.write_offs = Money(self.write_offs.0 + odpis.0);
        if pusta {
            self.batches.remove(id);
        }
        Mass(ile)
    }

    /// Masa towaru w drodze na tym zleceniu — lewa strona pytania „czy dostawa dojechała
    /// w całości".
    #[must_use]
    pub fn in_transit_mass(&self, cargo: &[BatchId]) -> Mass {
        Mass(
            cargo
                .iter()
                .filter_map(|b| self.batches.get(*b))
                .map(|b| b.mass.0)
                .sum(),
        )
    }

    /// Przeszacowuje koszt własny ładunku na **cenę, którą zapłacił kupujący**
    /// (`AP-7`).
    ///
    /// **Dlaczego to musi istnieć.** `cost_total` partii znaczy „ile ta masa kosztuje
    /// tego, kto ją trzyma". Przy przewozie między zakładami partia zmienia właściciela,
    /// a jej koszt zostawał kosztem **sprzedawcy** — więc piekarnia wyceniała mąkę po
    /// koszcie wytworzenia młyna, płacąc za nią cenę z marżą. Różnica to dokładnie
    /// marża sprzedawcy i znikała z bilansu: `check_cost` przechodził, bo `paid_in`
    /// też jej nie widział, ale księga kupującego (`InventoryGoods`) rozjeżdżała się
    /// z wyceną magazynu o wartość każdej lokalnej dostawy. To jest niezmiennik `P5`
    /// i to on ten błąd znalazł.
    ///
    /// Ścieżka importowa robiła to od M6c poprawnie (`cost: paid + duty`) — ale import
    /// **tworzy** partię, a sprzedaż lokalna ją **przenosi**, więc jedyna droga, która
    /// tego nie robiła, była zarazem jedyną, której do M6e nikt nie przeszedł: zakłady
    /// nie produkowały, więc nie miały czego sprzedawać.
    ///
    /// Obie księgi kontrolne rosną razem z kosztem: `cogs` o koszt sprzedawcy (towar
    /// zszedł z jego bilansu), `paid_in` o cenę kupującego. Dzięki temu
    /// [`Store::check_cost`] domyka się co do grosza, a różnica jest tym, czym jest —
    /// wynikiem sprzedawcy, a nie zgubionym groszem.
    /// Zwraca **koszt własny sprzedawcy** — tę samą liczbę, którą dopisuje do `cogs`.
    /// Rozliczenie B2B niesie ją dalej jako `Settlement::seller_cogs`, żeby rachunek
    /// wyniku zakładu produkcyjnego liczył marżę z partii, a nie ze średniej (`R2-WP7`).
    pub fn resell(&mut self, cargo: &[BatchId], price: Money) -> Money {
        if cargo.is_empty() || price.0 <= 0 {
            return Money::ZERO;
        }
        let stary: i64 = cargo
            .iter()
            .filter_map(|b| self.batches.get(*b))
            .map(|b| b.cost_total.0)
            .sum();
        let wagi: Vec<u64> = cargo
            .iter()
            .map(|b| self.batches.get(*b).map_or(0, |x| x.mass.0.max(0) as u64))
            .collect();
        if wagi.iter().all(|w| *w == 0) {
            return Money::ZERO;
        }
        // Podział sumuje się do kwoty dzielonej co do grosza (00 §2); reszta trafia
        // do pierwszej partii wg ustalonego porządku, czyli kolejności ładowania.
        let czesci = split_proportional(price, &wagi);
        for (b, c) in cargo.iter().zip(czesci) {
            if let Some(x) = self.batches.get_mut(*b) {
                x.cost_total = c;
            }
        }
        self.cogs = Money(self.cogs.0 + stary);
        self.paid_in = Money(self.paid_in.0 + price.0);
        Money(stary)
    }
}

/// Podział wielkości pochodnej (objętość, milisztuki) proporcjonalnie do masy.
/// Na `i128`, bo objętość silosu razy masa partii wychodzi poza `i64`.
fn podziel_proporcjonalnie(calosc: i64, czesc: i64, suma: i64) -> i64 {
    if suma <= 0 {
        return 0;
    }

    (i128::from(calosc) * i128::from(czesc) / i128::from(suma)) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests_support::{draft, magazyn, CHLEB, PIASEK};
    use crate::store::MassIn;
    use magnat_core::LossKind;

    /// Podział nie tworzy i nie niszczy ani grama, ani grosza — a suma części równa się
    /// całości także wtedy, gdy koszt jest niepodzielny.
    #[test]
    fn podzial_zachowuje_mase_i_pieniadz() {
        let (cat, mut s, slot) = magazyn();
        let id = s
            .put(
                &cat,
                slot,
                draft(PIASEK, 999, 1_000_001, 0),
                MassIn::Produced,
            )
            .expect("wstawienie");
        let nowa = s.split(id, Mass(333)).expect("podział");
        assert_eq!(s.batch(id).expect("stara").mass, Mass(666));
        assert_eq!(s.batch(nowa).expect("nowa").mass, Mass(333));
        assert_eq!(
            s.batch(id).expect("stara").cost_total.0 + s.batch(nowa).expect("nowa").cost_total.0,
            1_000_001
        );
        assert_eq!(s.total_stock(PIASEK), Mass(999));
        s.check_mass(PIASEK).expect("bilans masy");
        s.check_cost().expect("bilans pieniądza");
        s.check_no_negative().expect("niezmienniki slotu");

        // Podział na całość albo na nic nie ma sensu i mówi to wprost.
        assert!(s.split(id, Mass(666)).is_none());
        assert!(s.split(id, Mass(0)).is_none());
    }

    /// Ładunek schodzi ze slotu, ale **nie z bilansu**: towar na ciężarówce jest
    /// zapasem miasta, po prostu nie leży na półce.
    #[test]
    fn zaladunek_zdejmuje_ze_slotu_ale_nie_z_bilansu() {
        let (cat, mut s, slot) = magazyn();
        s.put(&cat, slot, draft(CHLEB, 5_000, 700, 0), MassIn::Produced)
            .expect("wstawienie");
        let zlecenie = TransportOrderId(1);
        let ladunek = s
            .load(slot, CHLEB, Mass(2_000), Q::MIN, zlecenie)
            .expect("załadunek");

        assert_eq!(
            s.stock_of(slot, CHLEB),
            Mass(3_000),
            "w slocie zostaje reszta"
        );
        assert_eq!(
            s.total_stock(CHLEB),
            Mass(5_000),
            "w bilansie nic nie ubyło"
        );
        assert_eq!(s.in_transit_mass(&ladunek), Mass(2_000));
        s.check_mass(CHLEB).expect("bilans masy");
        s.check_no_negative().expect("niezmienniki slotu");

        // Czego nie ma w slocie, tego nie da się załadować — i nic się przy tym
        // nie zmienia, bo dostawa częściowa jest decyzją wołającego.
        assert!(s.load(slot, CHLEB, Mass(9_000), Q::MIN, zlecenie).is_none());
        assert_eq!(s.stock_of(slot, CHLEB), Mass(3_000));
    }

    /// Rozładunek do ciasnego slotu **zostawia** partię w drodze zamiast ją zgubić.
    #[test]
    fn rozladunek_do_pelnego_slotu_zwraca_odrzucone() {
        let (cat, mut s, slot) = magazyn();
        s.put(&cat, slot, draft(CHLEB, 5_000, 700, 0), MassIn::Produced)
            .expect("wstawienie");
        let zlecenie = TransportOrderId(7);
        let ladunek = s
            .load(slot, CHLEB, Mass(5_000), Q::MIN, zlecenie)
            .expect("załadunek");
        let ciasny = s.add_slot(
            s.slot(slot).expect("slot").site,
            crate::store::WarehouseRole::Input,
            crate::catalog::StorageClass::Ambient,
            Mass(1_000),
            Volume(1_000_000_000),
            0,
        );
        let odrzucone = s
            .unload(&cat, zlecenie, &ladunek, ciasny)
            .expect("rozładunek");
        assert_eq!(odrzucone, ladunek, "nic się nie zmieściło");
        assert_eq!(
            s.in_transit_mass(&ladunek),
            Mass(5_000),
            "towar dalej istnieje"
        );
        s.check_mass(CHLEB).expect("bilans masy");

        // Cudzego ładunku nie da się rozładować pod swoim numerem zlecenia.
        assert_eq!(
            s.unload(&cat, TransportOrderId(8), &ladunek, ciasny),
            Err(StoreError::StaleReservation)
        );
    }

    /// Stłuczka w drodze to strata **z kategorią** — masa znikająca bez kategorii
    /// jest błędem testu, nie zaokrągleniem (§7.3 pkt 1).
    #[test]
    fn stluczka_w_drodze_ma_kategorie() {
        let (cat, mut s, slot) = magazyn();
        s.put(&cat, slot, draft(CHLEB, 4_000, 800, 0), MassIn::Produced)
            .expect("wstawienie");
        let ladunek = s
            .load(slot, CHLEB, Mass(4_000), Q::MIN, TransportOrderId(2))
            .expect("załadunek");
        let stracone = s.damage_in_transit(ladunek[0], Mass(1_000), LossKind::TransportDamage);
        assert_eq!(stracone, Mass(1_000));
        assert_eq!(s.losses(CHLEB, LossKind::TransportDamage), Mass(1_000));
        assert_eq!(s.total_stock(CHLEB), Mass(3_000));
        s.check_mass(CHLEB).expect("bilans masy");
        s.check_cost().expect("bilans pieniądza");
    }
}
