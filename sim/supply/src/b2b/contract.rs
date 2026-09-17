//! Kontrakty terminowe (M6c §5.8, WP8).
//!
//! Spot odpowiada na pytanie „skąd wziąć dziś", kontrakt na „skąd brać przez rok".
//! Różnica, która ma znaczenie w symulacji, jest jedna: kontrakt **zobowiązuje obie
//! strony** i za niedotrzymanie się płaci. Bez kary kontrakt byłby deklaracją intencji,
//! a niedobór dalej domykałby się znikającym zapotrzebowaniem.
//!
//! **Kara umowna to pieniądz, kary w funkcji celu RFQ to wagi.** Dwie różne liczby, obie
//! nazywają się karą, i tylko ta pierwsza wchodzi do księgi. `late_penalty` z `B2bTuning`
//! nie jest przez nikogo płacone; [`Penalty`] jest.

use magnat_core::{
    ContractId, DecisionReason, FirmId, GoodId, HashState, Mass, Money, OpenHours, SimMinute,
    SiteId, StateHasher, Q,
};

use crate::b2b::rfq::WhoTransports;
use crate::batch::SlotId;

/// Harmonogram dostaw. `window` ogranicza godziny przyjęcia na rampie — dostawa
/// poza oknem czeka, a nie znika (`D5`; M8 podmieni okno na regulację miejską).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DeliverySchedule {
    pub every_minutes: u32,
    pub mass: Mass,
    pub window: OpenHours,
}

/// Cennik kontraktu. Wszystkie trzy warianty liczą **netto** (`K-7`) i wszystkie trzy
/// przeliczają się całkowitoliczbowo — indeksacja w promilach dałaby dryf, którego
/// nikt by nie zauważył przez pierwsze pół roku gry.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ContractPricing {
    Fixed {
        price: Money,
    },
    Indexed {
        base: Money,
        index: GoodId,
        ref_price: Money,
        pass_through_pct: u8,
    },
    Collar {
        base: Money,
        index: GoodId,
        ref_price: Money,
        floor: Money,
        cap: Money,
    },
}

impl ContractPricing {
    /// Towar indeksowy, jeśli cennik za czymś chodzi.
    #[must_use]
    pub const fn index_good(&self) -> Option<GoodId> {
        match *self {
            ContractPricing::Fixed { .. } => None,
            ContractPricing::Indexed { index, .. } | ContractPricing::Collar { index, .. } => {
                Some(index)
            }
        }
    }

    /// Czy cena chodzi za indeksem — to jest jedyne rozróżnienie, o które pyta gracz.
    /// `Collar` jest indeksowany z klamrą, więc po tej stronie stoi razem z `Indexed`.
    #[must_use]
    pub const fn is_indexed(&self) -> bool {
        !matches!(self, ContractPricing::Fixed { .. })
    }

    /// Cena obowiązująca przy danym poziomie indeksu. `index_now` jest ignorowane
    /// dla ceny stałej — i to jest jedyny wariant, w którym wolno je pominąć.
    #[must_use]
    pub fn price_at(&self, index_now: Money) -> Money {
        match *self {
            ContractPricing::Fixed { price } => price,
            ContractPricing::Indexed {
                base,
                ref_price,
                pass_through_pct,
                ..
            } => Money(base.0 + (index_now.0 - ref_price.0) * i64::from(pass_through_pct) / 100),
            ContractPricing::Collar {
                base,
                ref_price,
                floor,
                cap,
                ..
            } => Money((base.0 + index_now.0 - ref_price.0).clamp(floor.0, cap.0)),
        }
    }
}

/// Kara umowna za masę niedostarczoną. `cap_pct` liczy się od wartości **całego**
/// kontraktu, nie od jednej dostawy — inaczej sufit nie byłby sufitem.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Penalty {
    pub per_tonne_missed: Money,
    pub cap_pct: u8,
    /// Spóźnienie w granicach łaski **nie jest** niedostarczeniem. Rozróżnienie jest
    /// w §6.4.5 dokumentu fazy wymuszone przez M10: `trust` karze za spóźnienia ponad
    /// `grace_minutes`, a nie za spóźnione w ogóle, i te dwie liczby nie mogą być
    /// liczone osobno po dwóch stronach.
    pub grace_minutes: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ContractError {
    Unknown,
    Expired,
    NotAParty,
    BadSchedule,
}

/// Kontrakt dostawy.
#[derive(Clone, Debug)]
pub struct SupplyContract {
    pub id: ContractId,
    pub buyer: FirmId,
    pub seller: FirmId,
    pub good: GoodId,
    pub min_quality: Q,
    pub deliver_from: SiteId,
    pub from_slot: SlotId,
    pub deliver_to: SiteId,
    pub to_slot: SlotId,
    pub incoterm: WhoTransports,
    pub schedule: DeliverySchedule,
    pub pricing: ContractPricing,
    pub penalty: Penalty,
    pub valid_from: SimMinute,
    pub valid_to: SimMinute,
    pub notice_minutes: u32,
    /// Masa, której harmonogram zażądał do tej pory. Liczona, a nie wyprowadzana
    /// z dat: wypowiedzenie ucina harmonogram w środku okresu, a `prop_contract_penalty`
    /// sprawdza `fulfilled + missed == scheduled` **co do grama**.
    pub scheduled_mass: Mass,
    pub fulfilled_mass: Mass,
    pub missed_mass: Mass,
    pub late_deliveries: u32,
    /// Kara naliczona i kara zapłacona — dwie liczby, bo między naliczeniem a zapłatą
    /// jest rozliczenie po stronie księgi, a `prop_contract_penalty` pyta, czy się zeszły.
    pub penalty_accrued: Money,
    pub penalty_paid: Money,
    /// Minuta ostatniej dostawy wystawionej przez harmonogram.
    pub last_issued: SimMinute,
    pub terminated_at: Option<SimMinute>,
}

impl SupplyContract {
    /// Czy kontrakt obowiązuje w tej minucie.
    #[must_use]
    pub fn is_active(&self, now: SimMinute) -> bool {
        now.0 >= self.valid_from.0
            && now.0 < self.valid_to.0
            && self.terminated_at.is_none_or(|t| now.0 < t.0)
    }

    /// Wartość całego kontraktu przy cenie bazowej — mianownik sufitu kary.
    #[must_use]
    pub fn total_value(&self, index_now: Money) -> Money {
        let okres = self.valid_to.0.saturating_sub(self.valid_from.0);
        let dostaw = if self.schedule.every_minutes == 0 {
            0
        } else {
            okres / u64::from(self.schedule.every_minutes)
        };
        let masa = i128::from(dostaw) * i128::from(self.schedule.mass.0);
        Money((i128::from(self.pricing.price_at(index_now).0) * masa / 1_000_000) as i64)
    }

    /// Nalicza karę za masę niedostarczoną, z sufitem `cap_pct`. Zwraca kwotę **faktycznie
    /// naliczoną** — może być mniejsza od wyliczonej, jeśli sufit już się wyczerpał.
    pub fn accrue_penalty(&mut self, missed: Mass, index_now: Money) -> Money {
        self.missed_mass = Mass(self.missed_mass.0 + missed.0);
        let surowa = Money(
            (i128::from(self.penalty.per_tonne_missed.0) * i128::from(missed.0) / 1_000_000) as i64,
        );
        let sufit = self
            .total_value(index_now)
            .mul_ratio(i64::from(self.penalty.cap_pct), 100);
        let zostalo = Money(sufit.0.saturating_sub(self.penalty_accrued.0).max(0));
        let kwota = Money(surowa.0.min(zostalo.0));
        self.penalty_accrued = Money(self.penalty_accrued.0 + kwota.0);
        kwota
    }

    /// Kara zapłacona: schodzi **dokładnie tyle, ile jest długu**, i nigdy więcej.
    ///
    /// Metoda kontraktu, nie rynku (`AP-9`): to jest niezmiennik `prop_contract_penalty`
    /// („suma kar naliczonych równa sumie zapłaconych"), a niezmiennik ma mieszkać przy
    /// danych, których dotyczy. [`crate::B2b::pay_penalty`] wyłącznie odnajduje kontrakt.
    pub fn pay_penalty(&mut self, amount: Money) -> Money {
        let dlug = Money(self.penalty_accrued.0 - self.penalty_paid.0);
        let kwota = Money(amount.0.min(dlug.0).max(0));
        self.penalty_paid = Money(self.penalty_paid.0 + kwota.0);
        kwota
    }

    /// Dostawa doszła. `late_minutes` liczy się od terminu, nie od wysyłki.
    pub fn record_fulfilled(&mut self, mass: Mass, late_minutes: u32) {
        self.fulfilled_mass = Mass(self.fulfilled_mass.0 + mass.0);
        if late_minutes > self.penalty.grace_minutes {
            self.late_deliveries += 1;
        }
    }

    #[must_use]
    pub fn reason(&self) -> DecisionReason {
        let miesiace = self.valid_to.0.saturating_sub(self.valid_from.0) / (30 * 1_440);
        DecisionReason::ContractSigned {
            good: self.good,
            seller: self.seller,
            months: miesiace.min(u64::from(u16::MAX)) as u16,
            indexed: self.pricing.is_indexed(),
        }
    }
}

impl HashState for SupplyContract {
    fn hash_state(&self, h: &mut StateHasher) {
        self.id.entity().hash_state(h);
        self.buyer.entity().hash_state(h);
        self.seller.entity().hash_state(h);
        h.write_u16(self.good.0);
        h.write_u8(self.min_quality.get());
        self.deliver_from.entity().hash_state(h);
        h.write_u32(self.from_slot.0);
        self.deliver_to.entity().hash_state(h);
        h.write_u32(self.to_slot.0);
        h.write_u8(self.incoterm as u8);
        h.write_u32(self.schedule.every_minutes);
        self.schedule.mass.hash_state(h);
        h.write_u16(self.schedule.window.open.get());
        h.write_u16(self.schedule.window.close.get());
        h.write_u8(self.schedule.window.days);
        hash_pricing(&self.pricing, h);
        self.penalty.per_tonne_missed.hash_state(h);
        h.write_u8(self.penalty.cap_pct);
        h.write_u32(self.penalty.grace_minutes);
        self.valid_from.hash_state(h);
        self.valid_to.hash_state(h);
        h.write_u32(self.notice_minutes);
        self.scheduled_mass.hash_state(h);
        self.fulfilled_mass.hash_state(h);
        self.missed_mass.hash_state(h);
        h.write_u32(self.late_deliveries);
        self.penalty_accrued.hash_state(h);
        self.penalty_paid.hash_state(h);
        self.last_issued.hash_state(h);
        match self.terminated_at {
            Some(t) => {
                h.write_u8(1);
                t.hash_state(h);
            }
            None => h.write_u8(0),
        }
    }
}

fn hash_pricing(p: &ContractPricing, h: &mut StateHasher) {
    match *p {
        ContractPricing::Fixed { price } => {
            h.write_u8(0);
            price.hash_state(h);
        }
        ContractPricing::Indexed {
            base,
            index,
            ref_price,
            pass_through_pct,
        } => {
            h.write_u8(1);
            base.hash_state(h);
            h.write_u16(index.0);
            ref_price.hash_state(h);
            h.write_u8(pass_through_pct);
        }
        ContractPricing::Collar {
            base,
            index,
            ref_price,
            floor,
            cap,
        } => {
            h.write_u8(2);
            base.hash_state(h);
            h.write_u16(index.0);
            ref_price.hash_state(h);
            floor.hash_state(h);
            cap.hash_state(h);
        }
    }
}

/// To, co wołający wie przed podpisaniem. `ContractId` nadaje rynek.
#[derive(Clone, Copy, Debug)]
pub struct SupplyContractDraft {
    pub buyer: FirmId,
    pub seller: FirmId,
    pub good: GoodId,
    pub min_quality: Q,
    pub deliver_from: SiteId,
    pub from_slot: SlotId,
    pub deliver_to: SiteId,
    pub to_slot: SlotId,
    pub incoterm: WhoTransports,
    pub schedule: DeliverySchedule,
    pub pricing: ContractPricing,
    pub penalty: Penalty,
    pub valid_from: SimMinute,
    pub valid_to: SimMinute,
    pub notice_minutes: u32,
}

impl SupplyContractDraft {
    /// Kontrakt wyprowadzony z wygranej oferty spot — najkrótsza droga od „kupiliśmy raz"
    /// do „kupujemy regularnie". Cena stała, bo oferta spot nie niesie indeksu; kontrakt
    /// indeksowany podpisuje się świadomie, podając `pricing` wprost.
    #[must_use]
    pub fn from_quote(
        q: &crate::b2b::rfq::Quote,
        r: &crate::b2b::rfq::Rfq,
        every_minutes: u32,
        valid_to: SimMinute,
        penalty: Penalty,
    ) -> SupplyContractDraft {
        SupplyContractDraft {
            buyer: r.buyer,
            seller: q.seller,
            good: r.good,
            min_quality: r.min_quality,
            deliver_from: q.from_site,
            from_slot: q.from_slot,
            deliver_to: r.deliver_to,
            to_slot: r.to_slot,
            incoterm: r.incoterm,
            schedule: DeliverySchedule {
                every_minutes,
                mass: r.mass,
                window: OpenHours::ALWAYS,
            },
            pricing: ContractPricing::Fixed { price: q.price },
            penalty,
            valid_from: r.closes_at,
            valid_to,
            notice_minutes: every_minutes * 2,
        }
    }
}

/// Dostawa, której harmonogram zażądał w tej minucie.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ContractDelivery {
    pub contract: ContractId,
    pub good: GoodId,
    pub mass: Mass,
    pub from: SiteId,
    pub from_slot: SlotId,
    pub to: SiteId,
    pub to_slot: SlotId,
    pub due_at: SimMinute,
    pub unit_price: Money,
}

// ── Kontrakty po stronie zasobu ─────────────────────────────────────────────────
//
// Jak w `rfq.rs`: blok `impl B2b` stoi przy swoim module.

use crate::b2b::{B2b, SellerRef, Settlement};
use crate::catalog::Catalog;
use crate::store::Store;
use crate::transport::{FreightOracle, Transport, TransportRequest, VehicleRequirements};
use magnat_core::Entity;

impl B2b {
    // ── WP8: kontrakty ──────────────────────────────────────────────────────────

    /// Podpisuje kontrakt. `ContractId` mintuje rynek.
    ///
    /// `ponytail:` identyfikator powstaje z własnego licznika jako `Entity`, bo
    /// `ContractId(pub Entity)` jest kontraktem z 00 §2, a `sim/supply` nie ma świata,
    /// w którym mógłby zawołać `spawn`. Sufit nazwany: indeksy nie są indeksami ECS
    /// i **nikomu nie wolno szukać po nich w `World`** — kontrakt żyje wyłącznie tutaj.
    /// Droga wyjścia: M7 jest właścicielem firm i kontraktów jako encji i wtedy licznik
    /// znika, a nie przenosi się gdzie indziej.
    pub fn sign_contract(&mut self, d: SupplyContractDraft) -> Result<ContractId, ContractError> {
        if d.schedule.every_minutes == 0 || d.schedule.mass.0 <= 0 {
            return Err(ContractError::BadSchedule);
        }
        if d.valid_to.0 <= d.valid_from.0 {
            return Err(ContractError::Expired);
        }
        let idx = self.next_contract;
        self.next_contract += 1;
        let id = ContractId(Entity::new(idx, std::num::NonZeroU32::MIN));
        self.contracts.insert(
            idx,
            SupplyContract {
                id,
                buyer: d.buyer,
                seller: d.seller,
                good: d.good,
                min_quality: d.min_quality,
                deliver_from: d.deliver_from,
                from_slot: d.from_slot,
                deliver_to: d.deliver_to,
                to_slot: d.to_slot,
                incoterm: d.incoterm,
                schedule: d.schedule,
                pricing: d.pricing,
                penalty: d.penalty,
                valid_from: d.valid_from,
                valid_to: d.valid_to,
                notice_minutes: d.notice_minutes,
                scheduled_mass: Mass::ZERO,
                fulfilled_mass: Mass::ZERO,
                missed_mass: Mass::ZERO,
                late_deliveries: 0,
                penalty_accrued: Money::ZERO,
                penalty_paid: Money::ZERO,
                last_issued: d.valid_from,
                terminated_at: None,
            },
        );
        Ok(id)
    }

    /// Wypowiedzenie. Zwraca kwotę do zapłaty przez wypowiadającego: kara za masę,
    /// której harmonogram zażądałby w okresie wypowiedzenia.
    ///
    /// Kontrakt **nie znika** — dochodzi do końca okresu wypowiedzenia, bo inaczej
    /// wypowiedzenie byłoby darmowym wyjściem z każdego zobowiązania.
    pub fn terminate_contract(
        &mut self,
        id: ContractId,
        by: FirmId,
        now: SimMinute,
    ) -> Result<Money, ContractError> {
        let c = self
            .contracts
            .get_mut(&id.entity().index())
            .ok_or(ContractError::Unknown)?;
        if by != c.buyer && by != c.seller {
            return Err(ContractError::NotAParty);
        }
        if c.terminated_at.is_some() || !c.is_active(now) {
            return Err(ContractError::Expired);
        }
        let koniec = SimMinute((now.0 + u64::from(c.notice_minutes)).min(c.valid_to.0));
        c.terminated_at = Some(koniec);
        let dostaw = u64::from(c.notice_minutes) / u64::from(c.schedule.every_minutes);
        let masa = Mass((dostaw as i64) * c.schedule.mass.0);
        Ok(Money(
            (i128::from(c.penalty.per_tonne_missed.0) * i128::from(masa.0) / 1_000_000) as i64,
        ))
    }

    /// Dostawy, których harmonogram zażądał do tej minuty.
    pub fn due_deliveries(&mut self, now: SimMinute) -> Vec<ContractDelivery> {
        let mut wynik = Vec::new();
        let indeksy: Vec<(u32, Option<GoodId>)> = self
            .contracts
            .iter()
            .map(|(k, c)| (*k, c.pricing.index_good()))
            .collect();
        for (k, index_good) in indeksy {
            let index_now = index_good
                .and_then(|g| self.spot_index(g))
                .unwrap_or(Money::ZERO);
            let Some(c) = self.contracts.get_mut(&k) else {
                continue;
            };
            if !c.is_active(now) {
                continue;
            }
            let co = u64::from(c.schedule.every_minutes);
            while c.last_issued.0 + co <= now.0 {
                c.last_issued = SimMinute(c.last_issued.0 + co);
                c.scheduled_mass = Mass(c.scheduled_mass.0 + c.schedule.mass.0);
                wynik.push(ContractDelivery {
                    contract: c.id,
                    good: c.good,
                    mass: c.schedule.mass,
                    from: c.deliver_from,
                    from_slot: c.from_slot,
                    to: c.deliver_to,
                    to_slot: c.to_slot,
                    due_at: c.last_issued,
                    unit_price: c.pricing.price_at(index_now),
                });
            }
        }
        wynik
    }

    /// Realizuje dostawy kontraktowe. Czego sprzedawca nie ma, to jest **niedostarczone**
    /// i naliczana jest kara — nie „przesunięte na jutro".
    #[allow(clippy::too_many_arguments)]
    pub fn run_contracts(
        &mut self,
        cat: &Catalog,
        store: &mut Store,
        transport: &mut Transport,
        oracle: &dyn FreightOracle,
        now: SimMinute,
    ) -> Vec<Settlement> {
        let mut wynik = Vec::new();
        for d in self.due_deliveries(now) {
            let dostepne = store.available(d.from_slot, d.good, magnat_core::Q::MIN);
            let masa = Mass(d.mass.0.min(dostepne.0.max(0)));
            // Niedostarczone liczy się **przyrostowo**: dostawa, która nie wyjechała,
            // dopisuje się tutaj niżej. Gdyby brak był policzony raz, na starcie, masa
            // z nieudanego załadunku nie trafiłaby ani do zrealizowanych, ani do
            // niedostarczonych — i `fulfilled + missed == scheduled` przestałoby się
            // domykać dokładnie wtedy, gdy przewoźnik zawodzi, czyli w jedynym przypadku,
            // w którym ktoś na tę liczbę patrzy.
            let mut brak = Mass(d.mass.0 - masa.0);
            let idx = d.contract.entity().index();
            if masa.0 > 0 {
                let powod = self
                    .contracts
                    .get(&idx)
                    .map_or(DecisionReason::Unspecified, SupplyContract::reason);
                let id = transport.order(
                    oracle,
                    TransportRequest {
                        from: d.from,
                        to: d.to,
                        from_slot: d.from_slot,
                        to_slot: d.to_slot,
                        good: d.good,
                        mass: masa,
                        requires: VehicleRequirements::for_good(cat, d.good, masa),
                        ready_at: now,
                        due_at: d.due_at,
                    },
                    powod,
                );
                let wyslane = transport
                    .dispatch(
                        oracle,
                        store,
                        id,
                        crate::transport::Carrier::Unassigned,
                        now,
                    )
                    .is_ok();
                let cena =
                    Money((i128::from(d.unit_price.0) * i128::from(masa.0) / 1_000_000) as i64);
                if wyslane {
                    // Zmiana właściciela przeszacowuje koszt własny na cenę zapłaconą
                    // (`AP-7`) — tak samo jak przy sprzedaży spotowej.
                    if let Some(o) = transport.get(id) {
                        let cargo = o.cargo.clone();
                        store.resell(&cargo, cena);
                    }
                }
                if let Some(c) = self.contracts.get_mut(&idx) {
                    if wyslane {
                        c.record_fulfilled(masa, 0);
                        wynik.push(Settlement {
                            buyer: c.buyer,
                            deliver_to: c.deliver_to,
                            seller: SellerRef::Firm(c.seller),
                            good: d.good,
                            mass: masa,
                            net: cena,
                            duty: Money::ZERO,
                            order: Some(id),
                            reason: powod,
                        });
                    }
                }
                if wyslane {
                    self.spot[d.good.0 as usize].record(d.unit_price, masa);
                } else {
                    brak = Mass(brak.0 + masa.0);
                }
            }
            if brak.0 > 0 {
                let index_now = self
                    .contracts
                    .get(&idx)
                    .and_then(|c| c.pricing.index_good())
                    .and_then(|g| self.spot_index(g))
                    .unwrap_or(Money::ZERO);
                if let Some(c) = self.contracts.get_mut(&idx) {
                    c.accrue_penalty(brak, index_now);
                }
            }
        }
        wynik
    }

    /// Kara umowna zapłacona — druga połowa `prop_contract_penalty`. Wołający przelewa
    /// pieniądz, rynek odnotowuje, że dług zszedł.
    pub fn pay_penalty(&mut self, id: ContractId, amount: Money) -> Result<Money, ContractError> {
        Ok(self
            .contracts
            .get_mut(&id.entity().index())
            .ok_or(ContractError::Unknown)?
            .pay_penalty(amount))
    }

    /// Wszystkie kary naliczone i zapłacone w mieście — wejście testu własnościowego.
    #[must_use]
    pub fn penalty_totals(&self) -> (Money, Money) {
        let mut a = 0i64;
        let mut p = 0i64;
        for c in self.contracts.values() {
            a += c.penalty_accrued.0;
            p += c.penalty_paid.0;
        }
        (Money(a), Money(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn indeks(base: i64, refp: i64, floor: i64, cap: i64) -> ContractPricing {
        ContractPricing::Collar {
            base: Money(base),
            index: GoodId(0),
            ref_price: Money(refp),
            floor: Money(floor),
            cap: Money(cap),
        }
    }

    /// `Collar` nie wychodzi poza widełki ani w górę, ani w dół — to jest połowa
    /// kryterium `prop_contract_penalty` i da się ją sprawdzić bez świata.
    #[test]
    fn collar_trzyma_sie_widelek() {
        let p = indeks(100_000, 78_000, 90_000, 110_000);
        assert_eq!(p.price_at(Money(78_000)), Money(100_000));
        assert_eq!(p.price_at(Money(200_000)), Money(110_000));
        assert_eq!(p.price_at(Money(0)), Money(90_000));
    }

    /// Indeksacja z częściowym przeniesieniem: 85 % z §7.1, liczone całkowitoliczbowo.
    #[test]
    fn indeksacja_przenosi_osiemdziesiat_piec_procent() {
        let p = ContractPricing::Indexed {
            base: Money(124_600),
            index: GoodId(0),
            ref_price: Money(78_000),
            pass_through_pct: 85,
        };
        // Zboże drożeje o 100 zł/t; mąka powinna zdrożeć o 85 zł/t.
        assert_eq!(p.price_at(Money(88_000)), Money(124_600 + 8_500));
        assert!(!ContractPricing::Fixed { price: Money(1) }.is_indexed());
        assert!(p.is_indexed());
    }
}
