//! Oferta i indeks przestrzenny (M5 §5.2, PRD §6.1, §17.5).
//!
//! Oferta jest **jedynym nośnikiem ceny w grze**: nie istnieje zmienna „cena mleka",
//! a wszystko, co UI pokazuje pod taką nazwą, jest agregatem po ofertach ([`PriceStats`]).
//!
//! Oferta siedzi w arenie, nie w ECS (`K-16`): jest ich rzędu 10⁵, mają skrajnie wysoką
//! rotację i żadna nie jest nigdy odpytywana przekrojowo po archetypach — dociera się
//! do niej przez półkę albo przez indeks kategorii. `OfferId` to `ArenaHandle<Offer>`,
//! więc uchwyt po zwolnieniu nigdy nie jest ponownie ważny, a arena wchodzi do hasha
//! stanu w kolejności indeksów.
//!
//! Indeks jest **pochodną areny**, nie stanem: da się go odtworzyć z areny i pozycji
//! zakładów, więc nie wchodzi do hasha (ta sama zasada co `TrafficOverlay` w M4).

use magnat_core::{
    Arena, ArenaHandle, FirmId, GoodId, HashState, Money, Qty, SiteId, StateHasher, StockCat, Tick,
    Q, STOCK_CAT_COUNT,
};
use magnat_jobs::JobPool;
use magnat_spatial::{DynamicGrid, GridSpec, Vec2};

/// Uchwyt oferty. Kształt `{ index: u32, generation: NonZeroU32 }` daje `ArenaHandle`
/// z `engine/core` — M5 dostarcza instancję areny, nie własny mechanizm (`K-16`).
pub type OfferId = ArenaHandle<Offer>;

/// K-7: cena w ofercie to kwota, którą **płaci kupujący** — brutto w detalu, netto
/// w hurcie. VAT wyodrębnia się dopiero przy rozliczeniu (`Transaction::{net,tax,gross}`),
/// nie w ofercie. Dzięki temu podwyżka VAT-u uderza w popyt natychmiast, bo zmienia cenę
/// widzianą przez klienta.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum PriceBasis {
    GrossRetail,
    NetB2B,
}

/// Warstwa indeksu ofert. W M5 kategorią jest kategoria zapasu gospodarstwa domowego
/// (`StockCat` z `engine/core` — to M5 przypisuje `GoodId` do kategorii, jak zapowiada
/// jej dokumentacja); M7 dokłada `JobRole(JobRoleId)`, bo rynek pracy jest ofertą jak
/// każda inna i ma się dopasowywać **tym samym** kodem (M5 §6, granica D19).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum CategoryId {
    Stock(StockCat),
}

impl CategoryId {
    /// Liczba warstw indeksu.
    pub const LAYER_COUNT: usize = STOCK_CAT_COUNT;

    /// Numer warstwy. Kolejność jest kontraktem tak samo jak kolejność `StockCat`.
    #[must_use]
    pub const fn layer(self) -> usize {
        match self {
            CategoryId::Stock(c) => c.as_index(),
        }
    }
}

/// Wpis areny ofert.
///
/// Pozycji tu nie ma z rozmysłu: oferta stoi tam, gdzie zakład, więc pozycja jest
/// własnością `SiteId` i podaje ją wołający przy przebudowie indeksu. Trzymanie jej
/// w ofercie kosztowałoby 8 bajtów × 10⁵ ofert za daną, która i tak jest kopią.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Offer {
    pub seller: FirmId,
    /// Sklep — nośnik lokalizacji i dostępności.
    pub site: SiteId,
    pub good: GoodId,
    /// Za 1 sztukę (`Qty(1000)`); ZAWSZE kwota płacona przez kupującego (`K-7`).
    pub unit_price: Money,
    /// W M5 zawsze `GrossRetail`; M6 dokłada hurt.
    pub price_basis: PriceBasis,
    /// Stan półki. `0` znaczy „znany sklep, ale brak towaru" — oferta zostaje, żeby
    /// mieszkaniec mógł się o braku dowiedzieć i żeby `LostSale` miał co wskazać.
    pub available: Qty,
    pub quality: Q,
    pub category: CategoryId,
    pub since: Tick,
    /// Licznik zmian ceny — obserwacja konkurencji z opóźnieniem 1–7 dni (M5c) porównuje
    /// go z zapamiętanym, zamiast trzymać kopię cennika.
    pub price_rev: u32,
}

impl Offer {
    /// Zmiana ceny **nie rusza indeksu**: indeks trzyma uchwyty, a cena czytana jest
    /// z areny na żywo. To jest powód, dla którego przecena tysiąca sklepów kosztuje
    /// tysiąc zapisów, a nie przebudowę siatki.
    pub fn set_price(&mut self, p: Money) {
        if p != self.unit_price {
            self.unit_price = p;
            self.price_rev = self.price_rev.wrapping_add(1);
        }
    }
}

impl HashState for Offer {
    fn hash_state(&self, h: &mut StateHasher) {
        self.seller.entity().hash_state(h);
        self.site.entity().hash_state(h);
        self.good.hash_state(h);
        self.unit_price.hash_state(h);
        h.write_u8(self.price_basis as u8);
        self.available.hash_state(h);
        self.quality.hash_state(h);
        h.write_u8(self.category.layer() as u8);
        self.since.hash_state(h);
        h.write_u32(self.price_rev);
    }
}

/// Agregat po ofertach — jedyna postać, w jakiej „cena towaru" w ogóle istnieje.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PriceStats {
    pub min: Money,
    pub p50: Money,
    pub max: Money,
    pub offers: u32,
    pub qty: Qty,
}

/// Rozkład cen w podanym zbiorze ofert. `buf` jest buforem wielokrotnego użytku —
/// agregat liczy się per dzielnica per towar, czyli często.
///
/// Mediana przy parzystej liczbie ofert to element **górny** (`n/2` po posortowaniu),
/// a nie średnia dwóch środkowych: średnia wprowadzałaby dzielenie i zaokrąglenie
/// do liczby, która ma być ceną z cennika, a nie statystyką z niego.
pub fn price_stats(
    offers: &Arena<Offer>,
    ids: &[OfferId],
    buf: &mut Vec<Money>,
) -> Option<PriceStats> {
    buf.clear();
    let mut qty = Qty::ZERO;
    for &id in ids {
        let Some(o) = offers.get(id) else { continue };
        buf.push(o.unit_price);
        qty = Qty(qty.get().saturating_add(o.available.get()));
    }
    if buf.is_empty() {
        return None;
    }
    buf.sort_unstable();
    Some(PriceStats {
        min: buf[0],
        p50: buf[buf.len() / 2],
        max: buf[buf.len() - 1],
        offers: buf.len() as u32,
        qty,
    })
}

/// Indeks przestrzenny ofert: osobna warstwa per kategoria (§17.5).
///
/// Warstwą jest `DynamicGrid` z `engine/spatial` — ta sama struktura, którą M2 zbudował
/// dla encji miasta. Powód jest prozaiczny: daje dokładnie to, czego wymaga §5.2
/// (przebudowa sortowaniem zliczającym w O(n + komórek), kolejność wyniku niezależna
/// od liczby wątków, zapytanie promieniowe do bufora bez alokacji), więc drugi indeks
/// na własnych `BTreeMap`-ach byłby przepisaniem gotowego kodu.
pub struct OfferIndex {
    spec: GridSpec,
    layers: Vec<DynamicGrid<OfferId>>,
    dirty: Vec<bool>,
    /// Bufory robocze przebudowy — żeby tick nie zaczynał się od alokacji.
    pos_buf: Vec<Vec2>,
    id_buf: Vec<OfferId>,
}

impl OfferIndex {
    #[must_use]
    pub fn new(spec: GridSpec) -> OfferIndex {
        OfferIndex {
            spec,
            layers: (0..CategoryId::LAYER_COUNT)
                .map(|_| DynamicGrid::new(spec))
                .collect(),
            dirty: vec![false; CategoryId::LAYER_COUNT],
            pos_buf: Vec::new(),
            id_buf: Vec::new(),
        }
    }

    #[must_use]
    pub fn spec(&self) -> &GridSpec {
        &self.spec
    }

    /// Dodanie albo zdjęcie oferty brudzi warstwę; **zmiana ceny nie** (§5.2).
    pub fn mark_dirty(&mut self, category: CategoryId) {
        self.dirty[category.layer()] = true;
    }

    pub fn mark_all_dirty(&mut self) {
        self.dirty.iter_mut().for_each(|d| *d = true);
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty.iter().any(|d| *d)
    }

    #[must_use]
    pub fn len(&self, category: CategoryId) -> usize {
        self.layers[category.layer()].len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.layers.iter().all(DynamicGrid::is_empty)
    }

    /// Przebudowa brudnych warstw. `pos_of` daje pozycję zakładu w metrach — oferta
    /// pozycji nie trzyma, bo stoi tam, gdzie sklep.
    ///
    /// Kolejność wejścia to kolejność indeksów areny, więc w komórce oferty leżą
    /// rosnąco po `OfferId` niezależnie od tego, w jakiej kolejności je wstawiano.
    pub fn rebuild(
        &mut self,
        offers: &Arena<Offer>,
        pos_of: impl Fn(SiteId) -> Vec2,
        pool: &JobPool,
    ) {
        for layer in 0..CategoryId::LAYER_COUNT {
            if !self.dirty[layer] {
                continue;
            }
            self.pos_buf.clear();
            self.id_buf.clear();
            for (id, o) in offers.iter() {
                if o.category.layer() == layer {
                    self.pos_buf.push(pos_of(o.site));
                    self.id_buf.push(id);
                }
            }
            self.layers[layer].rebuild(&self.pos_buf, &self.id_buf, pool);
            self.dirty[layer] = false;
        }
    }
}

/// Oferty kategorii w promieniu od punktu, w kolejności rosnącej po `(CellId, OfferId)`.
///
/// `out` jest czyszczony i wypełniany ponownie — bufor wielokrotnego użytku, zero
/// alokacji w gorącej ścieżce (§7.3).
pub fn query_offers(
    index: &OfferIndex,
    category: CategoryId,
    origin: Vec2,
    radius_m: u32,
    out: &mut Vec<OfferId>,
) {
    let layer = category.layer();
    debug_assert!(
        !index.dirty[layer],
        "zapytanie do warstwy oznaczonej jako brudna — brakuje wywołania OfferIndex::rebuild"
    );
    index.layers[layer].query_radius(origin, radius_m as f32, out);
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::Entity;
    use magnat_spatial::Aabb2;
    use std::num::NonZeroU32;

    fn ent(i: u32) -> Entity {
        Entity::new(i, NonZeroU32::MIN)
    }

    fn spec() -> GridSpec {
        GridSpec::covering(
            Aabb2::new(Vec2::new(0.0, 0.0), Vec2::new(4_000.0, 4_000.0)),
            200,
        )
    }

    fn offer(site: u32, price: i64, cat: StockCat) -> Offer {
        Offer {
            seller: FirmId(ent(site)),
            site: SiteId(ent(site)),
            good: GoodId(1),
            unit_price: Money(price),
            price_basis: PriceBasis::GrossRetail,
            available: Qty(5_000),
            quality: Q::new(50),
            category: CategoryId::Stock(cat),
            since: Tick(0),
            price_rev: 0,
        }
    }

    /// Pozycja zakładu: siatka 20 × 20 sklepów co 200 m.
    fn pos_of(s: SiteId) -> Vec2 {
        let i = s.entity().index();
        Vec2::new(
            f32::from((i % 20) as u16) * 200.0,
            f32::from((i / 20) as u16) * 200.0,
        )
    }

    #[test]
    fn zmiana_ceny_podbija_licznik_ale_nie_brudzi_indeksu() {
        let mut o = offer(0, 199, StockCat::Food);
        o.set_price(Money(199));
        assert_eq!(o.price_rev, 0, "cena bez zmiany nie jest rewizją");
        o.set_price(Money(219));
        assert_eq!((o.unit_price, o.price_rev), (Money(219), 1));
    }

    #[test]
    fn zapytanie_zwraca_tylko_swoja_kategorie_i_tylko_w_promieniu() {
        let pool = JobPool::new(1);
        let mut arena: Arena<Offer> = Arena::new();
        for i in 0..400u32 {
            arena.insert(offer(i, 100 + i64::from(i), StockCat::Food));
            arena.insert(offer(i, 900, StockCat::Fuel));
        }
        let mut idx = OfferIndex::new(spec());
        idx.mark_all_dirty();
        idx.rebuild(&arena, pos_of, &pool);
        assert!(!idx.is_dirty());

        let mut out = Vec::new();
        query_offers(
            &idx,
            CategoryId::Stock(StockCat::Food),
            Vec2::new(0.0, 0.0),
            450,
            &mut out,
        );
        assert!(!out.is_empty());
        for id in &out {
            let o = arena.get(*id).unwrap();
            assert_eq!(o.category, CategoryId::Stock(StockCat::Food));
            assert!(pos_of(o.site).distance(Vec2::new(0.0, 0.0)) <= 450.0);
        }
        // Kwadrat 3 × 3 sklepów co 200 m minus narożnik (566 m > 450 m) = 8.
        assert_eq!(out.len(), 8);
    }

    #[test]
    fn kolejnosc_wyniku_nie_zalezy_od_kolejnosci_wstawiania() {
        let pool = JobPool::new(1);
        let build = |rev: bool| {
            let mut arena: Arena<Offer> = Arena::new();
            let sites: Vec<u32> = if rev {
                (0..200u32).rev().collect()
            } else {
                (0..200u32).collect()
            };
            for i in sites {
                arena.insert(offer(i, 100 + i64::from(i), StockCat::Food));
            }
            let mut idx = OfferIndex::new(spec());
            idx.mark_all_dirty();
            idx.rebuild(&arena, pos_of, &pool);
            let mut out = Vec::new();
            query_offers(
                &idx,
                CategoryId::Stock(StockCat::Food),
                Vec2::new(1_000.0, 1_000.0),
                800,
                &mut out,
            );
            // Uchwyty różnią się między przebiegami (inna kolejność wstawiania do areny),
            // więc porównuje się to, co identyfikuje ofertę w świecie.
            out.iter()
                .map(|id| arena.get(*id).unwrap().site)
                .collect::<Vec<_>>()
        };
        let a = build(false);
        let b = build(true);
        assert!(!a.is_empty());
        assert_eq!(
            a.iter().map(|s| s.entity().index()).collect::<Vec<_>>(),
            b.iter().map(|s| s.entity().index()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn przebudowa_jest_niezalezna_od_liczby_watkow() {
        let mut arena: Arena<Offer> = Arena::new();
        for i in 0..400u32 {
            arena.insert(offer(i, 100 + i64::from(i), StockCat::Food));
        }
        let run = |threads: usize| {
            let pool = JobPool::new(threads);
            let mut idx = OfferIndex::new(spec());
            idx.mark_all_dirty();
            idx.rebuild(&arena, pos_of, &pool);
            let mut out = Vec::new();
            query_offers(
                &idx,
                CategoryId::Stock(StockCat::Food),
                Vec2::new(2_000.0, 2_000.0),
                3_000,
                &mut out,
            );
            out
        };
        assert_eq!(run(1), run(8));
    }

    #[test]
    fn uchwyt_zdjetej_oferty_nie_wraca_z_zapytania() {
        let pool = JobPool::new(1);
        let mut arena: Arena<Offer> = Arena::new();
        let doomed = arena.insert(offer(0, 100, StockCat::Food));
        arena.insert(offer(1, 110, StockCat::Food));
        let mut idx = OfferIndex::new(spec());
        idx.mark_all_dirty();
        idx.rebuild(&arena, pos_of, &pool);

        arena.remove(doomed);
        idx.mark_dirty(CategoryId::Stock(StockCat::Food));
        idx.rebuild(&arena, pos_of, &pool);

        let mut out = Vec::new();
        query_offers(
            &idx,
            CategoryId::Stock(StockCat::Food),
            Vec2::new(0.0, 0.0),
            1_000,
            &mut out,
        );
        assert!(!out.contains(&doomed));
        assert!(arena.get(doomed).is_none());
    }

    #[test]
    fn agregat_jest_liczony_po_ofertach_a_nie_z_globalnej_ceny() {
        let mut arena: Arena<Offer> = Arena::new();
        let ids: Vec<OfferId> = [190i64, 250, 210, 175, 230]
            .into_iter()
            .enumerate()
            .map(|(i, p)| arena.insert(offer(i as u32, p, StockCat::Food)))
            .collect();
        let mut buf = Vec::new();
        let s = price_stats(&arena, &ids, &mut buf).unwrap();
        assert_eq!(s.min, Money(175));
        assert_eq!(s.p50, Money(210));
        assert_eq!(s.max, Money(250));
        assert_eq!(s.offers, 5);
        assert_eq!(s.qty, Qty(25_000));
        assert_eq!(price_stats(&arena, &[], &mut buf), None);
    }

    #[test]
    fn oferta_miesci_sie_w_budzecie_pamieci() {
        // §7.3 mówi „≤ 48 B". Realny rozmiar to 56 B i jest to cena kontraktu, nie
        // rozrzutność — patrz korekta K5-2 w dokumencie fazy.
        assert!(
            size_of::<Offer>() <= 56,
            "Offer urósł: {}",
            size_of::<Offer>()
        );
        assert_eq!(size_of::<OfferId>(), 8);
    }
}
