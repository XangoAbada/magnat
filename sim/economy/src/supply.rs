//! Towar detaliczny i zaopatrzenie sklepu (M5b §5.7).
//!
//! To jest **jedyne źródło towaru w M5** i jedyne miejsce, które M6 musi wymienić.
//! `trait Wholesale` jest napisany tak, żeby M6 podmienił implementację, a nie
//! interfejs: rynek B2B (spot + kontrakty) odpowiada na te same trzy pytania —
//! ile by to kosztowało, zamawiam, co przyjechało.
//!
//! Pieniądz za dostawę idzie na konto `AccountOwner::RestOfWorld`, które jest
//! **zwykłym kontem w `Books`**, a nie ujściem — dzięki temu zakup u dostawcy
//! zewnętrznego nie rusza niezmiennika P1 i nie wymaga osobnej ewidencji ujść.

#[cfg(feature = "infinite_supply")]
use magnat_core::{rng, StreamId};
use magnat_core::{FirmId, GoodId, Money, Qty, SimMinute, StockCat, Tick, Q, STOCK_CAT_COUNT};

use crate::data::RetailTable;

/// Ile jednostek natywnych niesie jedna „sztuka" ceny. `Offer.unit_price`
/// i `Good.external_base_price` są **za 1000 jednostek natywnych** — za kilogram
/// dla towarów masowych, za sztukę dla sztukowych.
pub const PRICE_UNIT: i64 = 1_000;

/// Kwota za podaną ilość przy podanej cenie jednostkowej.
///
/// Zaokrąglenie jest jawne (`mul_ratio` — połowa od zera), bo to jest cena płacona
/// w sklepie, a nie wynik pośredni: obcięcie w dół oddawałoby grosz kupującemu
/// przy każdej transakcji, a tych jest rzędu 450 tys. na dobę.
#[must_use]
pub fn line_total(unit_price: Money, qty: Qty) -> Money {
    unit_price.mul_ratio(qty.get(), PRICE_UNIT)
}

// ── katalog detaliczny ───────────────────────────────────────────────────────────

/// Towar widziany przez handel detaliczny.
///
/// Powstaje ze **złączenia** `data/goods/` (cena hurtowa, jednostka) z
/// `data/economy/retail.ron` (kategoria, trwałość, dostawa, narzut) i z koszyka
/// epoki (`data/chains/needs.ron` — ile tego zjada człowiek na dobę). Złączenie
/// robi wołający, bo `GoodId` nadaje katalog M2 i tylko on zna kolejność kluczy.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GoodSpec {
    pub good: GoodId,
    pub cat: StockCat,
    /// Cena hurtowa bazowa za `PRICE_UNIT` jednostek natywnych.
    pub wholesale_base: Money,
    pub quality: Q,
    /// `0` = towar się nie psuje.
    pub shelf_life_days: u16,
    pub lead_time_days: u8,
    pub markup_bp: i32,
    /// Zużycie na osobę na dobę, w tych samych jednostkach co `Qty`.
    pub daily_per_person: Qty,
}

impl GoodSpec {
    /// Cena detaliczna startowa. Od M5c ustala ją `reprice`, a to jest punkt wyjścia.
    #[must_use]
    pub fn retail_price(&self) -> Money {
        self.wholesale_base
            .mul_ratio(10_000i64.saturating_add(i64::from(self.markup_bp)), 10_000)
    }
}

/// Towary detaliczne w kolejności rangi substytutu wewnątrz kategorii.
#[derive(Clone, Default)]
pub struct GoodTable {
    specs: Vec<GoodSpec>,
    /// Indeksy do `specs` per kategoria, w kolejności z `retail.ron` (ranga substytutu).
    by_cat: [Vec<u32>; STOCK_CAT_COUNT],
    /// Klucz tekstowy → identyfikator. Potrzebny wszędzie tam, gdzie dane wskazują
    /// towar nazwą, a nie indeksem — w M5 robi to koszyk CPI (`data/economy/cpi.ron`).
    /// `BTreeMap`, nie `HashMap`: iteracja po tym słowniku musi być deterministyczna
    /// (00 §3.2), nawet jeśli dziś nikt po nim nie iteruje.
    by_key: std::collections::BTreeMap<String, GoodId>,
}

impl GoodTable {
    /// Buduje katalog detaliczny. `resolve` dostaje klucz tekstowy z `retail.ron`
    /// i zwraca `(GoodId, cena hurtowa, zużycie na osobę na dobę)` albo `None`,
    /// jeśli tego towaru w danych epoki nie ma — wtedy wpis jest **pomijany**,
    /// a nie zgłaszany jako błąd: koszyk epoki powojennej nie zna lodówki.
    #[must_use]
    pub fn build(
        retail: &RetailTable,
        resolve: impl Fn(&str) -> Option<(GoodId, Money, Qty)>,
    ) -> GoodTable {
        let mut t = GoodTable::default();
        for g in &retail.goods {
            let Some((id, base, daily)) = resolve(&g.key) else {
                continue;
            };
            if base.get() <= 0 || daily.get() <= 0 {
                continue;
            }
            let i = u32::try_from(t.specs.len()).expect("katalog detaliczny > 4 mld pozycji");
            t.by_cat[g.cat.as_index()].push(i);
            t.by_key.insert(g.key.clone(), id);
            t.specs.push(GoodSpec {
                good: id,
                cat: g.cat,
                wholesale_base: base,
                quality: Q::new(g.quality),
                shelf_life_days: g.shelf_life_days,
                lead_time_days: g.lead_time_days,
                markup_bp: g.markup_bp.unwrap_or(retail.default_markup_bp),
                daily_per_person: daily,
            });
        }
        t
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.specs.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }

    #[must_use]
    pub fn at(&self, i: u32) -> &GoodSpec {
        &self.specs[i as usize]
    }

    /// Indeksy towarów kategorii, **w kolejności rangi substytutu**.
    #[must_use]
    pub fn in_cat(&self, c: StockCat) -> &[u32] {
        &self.by_cat[c.as_index()]
    }

    #[must_use]
    pub fn spec(&self, g: GoodId) -> Option<&GoodSpec> {
        self.specs.iter().find(|s| s.good == g)
    }

    /// Identyfikator towaru o kluczu z `retail.ron`. `None`, jeśli katalog epoki
    /// go nie wyprodukował — i to jest prawdziwa odpowiedź, nie błąd.
    #[must_use]
    pub fn id_of_key(&self, key: &str) -> Option<GoodId> {
        self.by_key.get(key).copied()
    }

    /// Klucz tekstowy towaru — droga powrotna, której potrzebuje panel sklepu:
    /// `GoodId` nie jest nazwą, a gracz ma zobaczyć „chleb", nie „17".
    ///
    /// Przeszukanie liniowe po `BTreeMap` jest tu właściwe: katalog M5 ma 18 pozycji,
    /// a panel woła to raz na wiersz półki, nie w gorącej ścieżce. Gdyby katalog M6
    /// urósł do czterystu i ktoś zawołał to w pętli po ofertach, właściwą odpowiedzią
    /// jest odwrotny indeks w `GoodTable`, a nie cache po stronie wołającego.
    #[must_use]
    pub fn key_of(&self, good: GoodId) -> Option<&str> {
        self.by_key
            .iter()
            .find(|(_, id)| **id == good)
            .map(|(k, _)| k.as_str())
    }

    /// Wszystkie pozycje katalogu w kolejności budowania.
    pub fn iter(&self) -> impl Iterator<Item = &GoodSpec> {
        self.specs.iter()
    }
}

// ── kontrakt zaopatrzenia ────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct OrderId(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SupplyError {
    /// Dostawca nie handluje tym towarem.
    NoSuchGood,
    /// Ilość niedodatnia — błąd wołającego.
    BadQuantity,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PurchaseQuote {
    pub good: GoodId,
    pub qty: Qty,
    /// Cena hurtowa za `PRICE_UNIT` jednostek.
    pub unit_price: Money,
    /// `teraz + lead_time_days`.
    pub delivery_at: Tick,
    pub quality: Q,
    pub shelf_life: Option<SimMinute>,
}

impl PurchaseQuote {
    #[must_use]
    pub fn total(&self) -> Money {
        line_total(self.unit_price, self.qty)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Delivery {
    pub order: OrderId,
    pub buyer: FirmId,
    pub site: magnat_core::SiteId,
    pub good: GoodId,
    pub qty: Qty,
    /// Kwota zapłacona przy zamówieniu — wchodzi wprost do `StockLine.cost_total`.
    pub paid: Money,
    pub quality: Q,
    pub expires: Option<SimMinute>,
}

/// Kontrakt zaopatrzenia sklepu. W M5: jedna implementacja ([`ExternalSupplier`]).
/// W M6: zastąpiona przez rynek B2B (spot + kontrakty) — **sygnatura bez zmian**.
pub trait Wholesale {
    fn quote(
        &self,
        good: GoodId,
        qty: Qty,
        at: magnat_core::SiteId,
        t: Tick,
    ) -> Option<PurchaseQuote>;

    /// `site` ponad plan §5.7: dostawa musi wiedzieć, do którego zakładu jedzie,
    /// a `FirmId` tego nie mówi — firma może mieć wiele sklepów już w M5.
    fn place_order(
        &mut self,
        q: &PurchaseQuote,
        buyer: FirmId,
        site: magnat_core::SiteId,
        t: Tick,
    ) -> Result<OrderId, SupplyError>;

    fn poll_deliveries(&mut self, t: Tick, out: &mut Vec<Delivery>);

    /// Mnożnik ceny hurtowej towaru (10 000 = bez zmian, 18 000 = +80 %).
    ///
    /// W traicie, a nie na typie konkretnym, i to jest wykonanie `AC-1`: scenariusz
    /// `supply-shock` balansatora (bramka **G4**) musi mieć czym szokować **niezależnie
    /// od tego, która faza stoi pod spodem**. Do M6c metoda wisiała na
    /// [`ExternalSupplier`] i podmiana dostawcy zabrałaby bramce jedyne zdarzenie
    /// zewnętrzne, jakie ma.
    fn set_shock(&mut self, good: GoodId, factor_bp: i32);
}

/// Dostawca zewnętrzny: cena hurtowa z danych, dryf deterministyczny, dostawa po czasie.
///
/// **Za feature'em `infinite_supply`, domyślnie wyłączonym** (M6 §6.3 krok 2, `AK-2`).
/// Tworzy masę z niczego i był do M6c jedynym sankcjonowanym wyjątkiem od zasady
/// zachowania masy; od WP11 zostaje wyłącznie do izolowanych testów warstwy detalicznej
/// i do scenariuszy balansatora badających samą półkę.
///
/// Trzy jawne konsekwencje, które M6 musi znać (§5.7):
/// 1. `RestOfWorld` jest kontem, nie ujściem — pieniądz nie wypada z systemu.
/// 2. Zapas wycenia się **średnią ważoną** (`StockLine.cost_total / qty`), bo nie
///    ma partii; M6 wprowadza `BatchId` i FIFO, co **zmienia COGS**.
/// 3. Dostawa nie zajmuje pojazdu ani rampy — tylko czas. M6 to urealnia.
#[cfg(feature = "infinite_supply")]
pub struct ExternalSupplier {
    seed: u64,
    goods: GoodTable,
    /// Mnożnik ceny hurtowej w punktach bazowych, per towar; `0` = brak szoku.
    ///
    /// Wejście scenariusza `supply-shock` balansatora (§7.4, bramka G4). Szok jest
    /// **stanem dostawcy**, a nie parametrem zapytania, bo dokładnie tak zachowuje
    /// się prawdziwy: cena skacze wszystkim naraz i zostaje. M6 zastąpi to ceną
    /// wynikającą z rynku B2B i mnożnik zniknie razem z zaślepką.
    shock_bp: Vec<(GoodId, i32)>,
    /// Zamówienia w drodze, w kolejności złożenia — `OrderId` rośnie, więc wynik
    /// `poll_deliveries` nie zależy od tego, ile ticków minęło między pollami.
    pending: Vec<(Tick, Delivery)>,
    next_order: u32,
}

#[cfg(feature = "infinite_supply")]
impl ExternalSupplier {
    #[must_use]
    pub fn new(seed: u64, goods: GoodTable) -> ExternalSupplier {
        ExternalSupplier {
            seed,
            goods,
            shock_bp: Vec::new(),
            pending: Vec::new(),
            next_order: 0,
        }
    }

    /// Ustawia mnożnik ceny hurtowej towaru (10 000 = bez zmian, 18 000 = +80 %).
    ///
    /// Lista jest krótka i posortowana po `GoodId` — szok dotyczy jednego, może
    /// dwóch towarów, więc `Vec` z wyszukaniem liniowym jest tańszy od mapy
    /// i deterministyczny bez dodatkowych zastrzeżeń (00 §3.2).
    pub fn set_shock(&mut self, good: GoodId, factor_bp: i32) {
        match self.shock_bp.binary_search_by_key(&good.0, |(g, _)| g.0) {
            Ok(i) => self.shock_bp[i].1 = factor_bp,
            Err(i) => self.shock_bp.insert(i, (good, factor_bp)),
        }
    }

    #[must_use]
    pub fn goods(&self) -> &GoodTable {
        &self.goods
    }

    /// Cena hurtowa doby: baza × dryf. Dryf jest czystą funkcją `(towar, doba)`,
    /// więc dwa zapytania w tej samej dobie dają tę samą cenę niezależnie od tego,
    /// kto pyta pierwszy — inaczej kolejność sklepów wpływałaby na ceny.
    ///
    /// Sezonowości w M5b nie ma: wymaga koszyka miesięcznego, który powstaje razem
    /// z CPI w M5d. `ponytail:` sufit nazwany, ścieżka wyjścia to mnożnik miesiąca
    /// obok dryfu, w tej samej funkcji.
    #[must_use]
    pub fn wholesale_price(&self, good: GoodId, t: Tick) -> Option<Money> {
        let spec = self.goods.spec(good)?;
        let day = t.get() / magnat_core::time::MINUTES_PER_DAY;
        let mut r = rng(
            self.seed,
            StreamId::ExternalPriceDrift,
            u32::from(good.get()),
            Tick(day),
        );
        // ±3 % w punktach bazowych; rozkład jednostajny wystarcza, bo to jest szum
        // rynku, a nie model rynku — model jest w M6.
        let drift_bp = i64::from(r.gen_range_u32(601)) - 300;
        let baza = spec.wholesale_base.mul_ratio(10_000 + drift_bp, 10_000);
        match self.shock_bp.binary_search_by_key(&good.0, |(g, _)| g.0) {
            Ok(i) => Some(baza.mul_ratio(i64::from(self.shock_bp[i].1), 10_000)),
            Err(_) => Some(baza),
        }
    }
}

#[cfg(feature = "infinite_supply")]
impl Wholesale for ExternalSupplier {
    fn quote(
        &self,
        good: GoodId,
        qty: Qty,
        _at: magnat_core::SiteId,
        t: Tick,
    ) -> Option<PurchaseQuote> {
        if qty.get() <= 0 {
            return None;
        }
        let spec = *self.goods.spec(good)?;
        let unit_price = self.wholesale_price(good, t)?;
        let lead = u64::from(spec.lead_time_days) * magnat_core::time::MINUTES_PER_DAY;
        Some(PurchaseQuote {
            good,
            qty,
            unit_price,
            delivery_at: Tick(t.get().saturating_add(lead)),
            quality: spec.quality,
            shelf_life: (spec.shelf_life_days > 0).then(|| {
                SimMinute(u64::from(spec.shelf_life_days) * magnat_core::time::MINUTES_PER_DAY)
            }),
        })
    }

    fn place_order(
        &mut self,
        q: &PurchaseQuote,
        buyer: FirmId,
        site: magnat_core::SiteId,
        t: Tick,
    ) -> Result<OrderId, SupplyError> {
        if q.qty.get() <= 0 {
            return Err(SupplyError::BadQuantity);
        }
        if self.goods.spec(q.good).is_none() {
            return Err(SupplyError::NoSuchGood);
        }
        let id = OrderId(self.next_order);
        self.next_order += 1;
        let _ = t;
        self.pending.push((
            q.delivery_at,
            Delivery {
                order: id,
                buyer,
                site,
                good: q.good,
                qty: q.qty,
                paid: q.total(),
                quality: q.quality,
                expires: q
                    .shelf_life
                    .map(|s| SimMinute(q.delivery_at.get().saturating_add(s.get()))),
            },
        ));
        Ok(id)
    }

    fn set_shock(&mut self, good: GoodId, factor_bp: i32) {
        ExternalSupplier::set_shock(self, good, factor_bp);
    }

    fn poll_deliveries(&mut self, t: Tick, out: &mut Vec<Delivery>) {
        out.clear();
        // `retain` zachowuje kolejność, więc dostawy wychodzą rosnąco po `OrderId`
        // niezależnie od tego, ile ticków minęło między dwoma wywołaniami.
        self.pending.retain(|(due, d)| {
            if due.get() <= t.get() {
                out.push(*d);
                false
            } else {
                true
            }
        });
    }
}
