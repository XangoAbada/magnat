//! Dostawca sklepu oparty na łańcuchu dostaw M6 — koniec „zewnętrznego dostawcy"
//! (M6 §6.3, WP11).
//!
//! `trait Wholesale` **nie zmienia kształtu** i to było zobowiązanie M6 wobec M5
//! (§6.4.4): trzy pytania zostają te same — ile by to kosztowało, zamawiam, co
//! przyjechało — zmienia się wyłącznie to, kto na nie odpowiada.
//!
//! Trzy różnice, których nie widać w sygnaturach, a które są całą treścią migracji:
//!
//! 1. **Towar przyjeżdża sam.** `place_order` otwiera zapytanie ofertowe albo zamówienie
//!    importowe; rozstrzygnięcie wystawia zlecenie transportowe, a rozładunek wkłada
//!    partie wprost do slotu zaplecza. `poll_deliveries` oddaje więc samą informację
//!    „przyjechało tyle, za tyle" — nie dokłada ani grama, bo gram już leży.
//! 2. **Cena ma z czego wynikać.** Do M6c była stałą z katalogu razy dryf; teraz jest
//!    ceną spot z rynku B2B albo ceną importową rosnącą z wolumenem, a gdy ani jednej
//!    nie ma — zapytanie po prostu nie ma odpowiedzi i sklep nie zamawia.
//! 3. **Dostawa może się nie udać.** Brak dostawcy, zablokowana trasa, pełny węzeł
//!    graniczny — każde z nich kończy się brakiem towaru na półce, i to jest dokładnie
//!    ten sygnał, którego M5 nie mogła zobaczyć przez trzy fazy.
//!
//! `ExternalSupplier` zostaje za feature'em `infinite_supply` i jest wtedy jedyną
//! implementacją. Nazwa wejścia szoku podaży (`Market::set_supply_shock`) przeżywa
//! podmianę — `AC-1` wiąże z nią bramkę **G4** balansatora, a balansator nie ma prawa
//! wiedzieć, która faza akurat stoi pod spodem.

use magnat_core::{FirmId, GoodId, Money, Qty, SimMinute, SiteId, Tick, Q};
use magnat_supply::{ChainHandle, RfqDraft, WhoTransports};

use crate::supply::{Delivery, GoodTable, OrderId, PurchaseQuote, SupplyError, Wholesale};

/// Domyślny dostawca rynku: łańcuch M6, chyba że włączono `infinite_supply`.
#[must_use]
pub fn default_supplier(
    seed: u64,
    goods: &GoodTable,
    chain: &ChainHandle,
) -> Box<dyn Wholesale + Send> {
    #[cfg(feature = "infinite_supply")]
    {
        let _ = chain;
        Box::new(crate::supply::ExternalSupplier::new(seed, goods.clone()))
    }
    #[cfg(not(feature = "infinite_supply"))]
    {
        let _ = (seed, goods);
        Box::new(ChainSupply::new(chain.clone()))
    }
}

/// Zaopatrzenie sklepu przez rynek B2B i węzły graniczne.
pub struct ChainSupply {
    chain: ChainHandle,
    next_order: u32,
}

impl ChainSupply {
    #[must_use]
    pub fn new(chain: ChainHandle) -> ChainSupply {
        ChainSupply {
            chain,
            next_order: 0,
        }
    }

    /// Cena hurtowa tony towaru: spot z rynku B2B, a gdy rynek jeszcze nie handlował —
    /// cena importowa z węzła granicznego. `None`, gdy nie ma ani jednej.
    fn cena_tony(&self, good: GoodId, mass: magnat_core::Mass, now: SimMinute) -> Option<Money> {
        let ch = self.chain.lock();
        let baza = ch.b2b.spot_index(good).or_else(|| {
            ch.b2b
                .import_quote(&self.chain.cat, good, mass, &self.chain.tuning, now)
                .map(|q| q.unit_price)
        })?;
        Some(baza)
    }
}

impl Wholesale for ChainSupply {
    fn quote(&self, good: GoodId, qty: Qty, at: SiteId, t: Tick) -> Option<PurchaseQuote> {
        if qty.get() <= 0 {
            return None;
        }
        let g = self.chain.cat.good(good);
        let mass = g.mass_of_units(qty);
        if mass.0 <= 0 {
            return None;
        }
        let za_tone = self.cena_tony(good, mass, SimMinute(t.get()))?;
        // `PurchaseQuote::unit_price` jest za `PRICE_UNIT` jednostek handlowych,
        // czyli za kilogram towaru masowego albo za sztukę sztukowego; cena z rynku
        // B2B jest za tonę. Przeliczenie jest jedno i jest tutaj.
        let za_jednostke = Money(
            (i128::from(za_tone.get()) * i128::from(g.mass_of_units(Qty(1_000)).0) / 1_000_000)
                as i64,
        );
        let _ = at;
        let shelf_life = g.shelf_life_minutes.map(|m| SimMinute(u64::from(m)));
        Some(PurchaseQuote {
            good,
            qty,
            unit_price: za_jednostke,
            // Termin dostawy jest **szacunkiem**, a nie obietnicą: rozstrzyga go rynek,
            // a ten może nie znaleźć dostawcy. `PurchaseQuote::delivery_at` służy M5
            // wyłącznie do wyceny terminu ważności zapasu.
            delivery_at: Tick(
                t.get()
                    .saturating_add(u64::from(self.chain.tuning.b2b.rfq_window_minutes + 120)),
            ),
            quality: Q::new(60),
            shelf_life,
        })
    }

    fn place_order(
        &mut self,
        q: &PurchaseQuote,
        buyer: FirmId,
        site: SiteId,
        t: Tick,
    ) -> Result<OrderId, SupplyError> {
        if q.qty.get() <= 0 {
            return Err(SupplyError::BadQuantity);
        }
        let mass = self.chain.cat.good(q.good).mass_of_units(q.qty);
        if mass.0 <= 0 {
            return Err(SupplyError::NoSuchGood);
        }
        let now = SimMinute(t.get());
        let id = OrderId(self.next_order);
        self.next_order += 1;

        let (cat, tuning, oracle) = (
            self.chain.cat.clone(),
            self.chain.tuning.clone(),
            self.chain.oracle.clone(),
        );
        let mut ch = self.chain.lock();
        let Some(slot) = ch.plant.get(site).and_then(|p| p.inputs.first().copied()) else {
            return Err(SupplyError::NoSuchGood);
        };
        let draft = RfqDraft {
            buyer,
            deliver_to: site,
            to_slot: slot,
            good: q.good,
            mass,
            min_quality: Q::MIN,
            needed_by: SimMinute(now.0 + u64::from(tuning.b2b.rfq_window_minutes) + 120),
            incoterm: WhoTransports::Seller,
        };
        // **Rynek lokalny ma pierwszeństwo, granica jest zapasowym wyjściem.**
        // Zapytanie ofertowe bez ani jednego dostawcy nie kończy się importem, tylko
        // odmową — więc „najpierw u siebie" trzeba rozstrzygnąć tutaj, a nie liczyć
        // na to, że rynek sam się domyśli. Miasto, w którym nikt jeszcze nie piecze
        // chleba, sprowadza go zza granicy; kiedy stanie pierwsza piekarnia, wygra
        // ceną, bo transport zza granicy kosztuje i kolejka na bramie rośnie.
        if ch.b2b.sellers_of(q.good).is_empty() {
            let Some(oferta) = ch.b2b.import_quote(&cat, q.good, mass, &tuning, now) else {
                return Err(SupplyError::NoSuchGood);
            };
            return match ch.b2b.place_import(oferta, buyer, site, slot) {
                Ok(()) => Ok(id),
                Err(_) => Err(SupplyError::NoSuchGood),
            };
        }
        let magnat_supply::Chain {
            store, plant, b2b, ..
        } = &mut *ch;
        let _ = b2b.open_rfq(draft, &cat, store, plant, oracle.as_ref(), &tuning, now);
        Ok(id)
    }

    /// **Pusto i tak ma być.** Lista dostaw z `Wholesale` znaczy „dostawca materializuje
    /// ci towar" — i tak działał `ExternalSupplier`. Łańcuch nie materializuje niczego:
    /// partie leżą w slocie zaplecza, odkąd ciężarówka je rozładowała, a pieniądz idzie
    /// osobno, listą `Settlement` (`Market::absorb_settlements`). Zwracanie ich tutaj
    /// policzyłoby ten sam towar dwa razy.
    fn poll_deliveries(&mut self, _t: Tick, out: &mut Vec<Delivery>) {
        out.clear();
    }

    /// Szok idzie **na węzeł graniczny**, a nie na wycenę w tym typie.
    ///
    /// To jest różnica między zdarzeniem a etykietą: cena z `quote` służy sklepowi
    /// do decyzji „stać mnie czy nie", ale płaci się to, co wyjdzie z rozliczenia
    /// rynku. Szok postawiony tutaj podniósłby liczbę w decyzji i nie ruszył kwoty
    /// w księdze — czyli bramka **G4** mierzyłaby własny parametr zamiast skutku.
    fn set_shock(&mut self, good: GoodId, factor_bp: i32) {
        self.chain.lock().b2b.set_supply_shock(good, factor_bp);
    }
}
