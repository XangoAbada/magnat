//! Rynek spot: zapytanie ofertowe i jego rozstrzygnięcie (M6c §5.8, WP7).
//!
//! Zakład, któremu kaskada niedoboru kazała szukać na rynku, otwiera [`Rfq`]. Dostawcy
//! odpowiadają [`Quote`]-ami, a po zamknięciu okna wygrywa **najniższy koszt całkowity**,
//! nie najniższa cena: oferta o dziesięć groszy tańsza, ale o sto kilometrów dalej, jest
//! droższa i funkcja celu ma to widzieć. Stąd `transport` w ocenie, a nie obok niej.
//!
//! **Czego tu nie ma i dlaczego.** Sprzedawca nie negocjuje, nie odmawia i nie różnicuje
//! ceny per odbiorca — wycena jest czystą funkcją kosztu wytworzenia i jednej marży
//! z `data/tuning/supply.ron`. Osobowość cenowa firmy (`FirmPersonality`) należy do M7
//! i to ona ma podmienić tę stałą; rabat stałego klienta (`SupplierRelation::discount_bps`)
//! należy do M7/M10 i M6 wypisał to wprost w §6.4.5 dokumentu fazy. Dopóki żadnego z nich
//! nie ma, RFQ liczy się bez rabatu i bez priorytetu — degradacja jest łagodna.

use magnat_core::{
    rng, DecisionReason, FirmId, GoodId, HashState, Mass, Money, SimMinute, SiteId, StateHasher,
    StreamId, Tick, Q,
};

use crate::batch::SlotId;
use crate::catalog::Catalog;
use crate::plant::Plant;
use crate::store::Store;
use crate::transport::{FreightOracle, VehicleRequirements};
use crate::tuning::B2bTuning;

pub use crate::shortage::RfqId;

/// Identyfikator oferty. Osobny licznik od `RfqId`, bo oferta przeżywa zapytanie:
/// panel łańcucha pokazuje, kto przegrał i o ile, także po rozstrzygnięciu.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct QuoteId(pub u32);

/// Kto organizuje przewóz. Nazwa z §5.8; w praktyce rozstrzyga, po czyjej stronie
/// stoi koszt transportu w ocenie oferty — nie to, kto fizycznie jedzie.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WhoTransports {
    Seller,
    Buyer,
}

/// Zapytanie ofertowe.
#[derive(Clone, Debug)]
pub struct Rfq {
    pub id: RfqId,
    pub buyer: FirmId,
    pub deliver_to: SiteId,
    /// Slot, do którego ma trafić towar — bez niego zlecenie transportowe nie ma dokąd
    /// rozładować, a `Transport::deliver` wymaga slotu, nie zakładu.
    pub to_slot: SlotId,
    pub good: GoodId,
    pub mass: Mass,
    pub min_quality: Q,
    pub needed_by: SimMinute,
    pub opened_at: SimMinute,
    pub closes_at: SimMinute,
    pub incoterm: WhoTransports,
    /// Oferty siedzą **w** zapytaniu, a nie w osobnej arenie z uchwytami. §5.8 pokazywał
    /// `Vec<QuoteId>`, ale drugiego właściciela oferty nie ma: nikt nie sięga po ofertę
    /// inaczej niż przez zapytanie, któremu odpowiada. Arena kosztowałaby uchwyt,
    /// generację i wejście do hasha bez ani jednego konsumenta (`AH-2`).
    pub quotes: Vec<Quote>,
    pub outcome: RfqOutcome,
}

/// Czym skończyło się zapytanie. `Open` w chwili zamknięcia okna **bez ofert** staje się
/// `NoQuotes` — i to jest sygnał, na którym kaskada wchodzi szczebel wyżej, do importu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RfqOutcome {
    Open,
    Awarded { quote: QuoteId, seller: FirmId },
    NoQuotes,
}

/// Oferta dostawcy. Cena **netto** (`K-7`): w hurcie VAT jest dla firmy przelotowy,
/// a cła i akcyza doliczają się osobno przez `ChargeRegistry` M8, nigdy w środku ceny.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Quote {
    pub id: QuoteId,
    pub rfq: RfqId,
    pub seller: FirmId,
    pub from_site: SiteId,
    pub from_slot: SlotId,
    pub mass_available: Mass,
    pub quality: Q,
    /// Grosze za tonę (postaci sypkie) albo za 1000 sztuk — ta sama skala co
    /// [`crate::Batch::unit_cost`] i `Good::external_base_price`, loco magazyn sprzedawcy.
    pub price: Money,
    /// Koszt dowozu, gdy organizuje go sprzedawca. `None` nie znaczy „za darmo",
    /// tylko „po stronie kupującego" — i wtedy ocena dokłada go sama.
    pub transport: Option<Money>,
    pub ready_at: SimMinute,
    pub eta: SimMinute,
    pub valid_until: SimMinute,
    /// Minuty jazdy z wyceny przewozu — potrzebne przy zamawianiu, żeby nie pytać
    /// wyroczni drugi raz o to samo.
    pub freight_minutes: u32,
    /// Preferencja stałego dostawcy w funkcji celu, w punktach bazowych
    /// (M10e WP10.13). **Nie schodzi z ceny** — `price` zostaje bez zmian, bo
    /// kupujący faktycznie przepłaca za rzetelność (patrz `b2b::relation`).
    pub discount_bp: u16,
}

impl Quote {
    /// Wartość samego towaru: cena za tonę razy masa. Na `i128`, bo tona ceny razy
    /// masa silosu wychodzi poza `i64`.
    #[must_use]
    pub fn goods_value(&self, mass: Mass) -> Money {
        Money((i128::from(self.price.0) * i128::from(mass.0) / 1_000_000) as i64)
    }
}

impl HashState for Quote {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.id.0);
        h.write_u32(self.rfq.0);
        self.seller.entity().hash_state(h);
        self.from_site.entity().hash_state(h);
        h.write_u32(self.from_slot.0);
        self.mass_available.hash_state(h);
        h.write_u8(self.quality.get());
        self.price.hash_state(h);
        h.write_u16(self.discount_bp);
        match self.transport {
            Some(m) => {
                h.write_u8(1);
                m.hash_state(h);
            }
            None => h.write_u8(0),
        }
        self.ready_at.hash_state(h);
        self.eta.hash_state(h);
        self.valid_until.hash_state(h);
        h.write_u32(self.freight_minutes);
    }
}

impl HashState for Rfq {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.id.0);
        self.buyer.entity().hash_state(h);
        self.deliver_to.entity().hash_state(h);
        h.write_u32(self.to_slot.0);
        h.write_u16(self.good.0);
        self.mass.hash_state(h);
        h.write_u8(self.min_quality.get());
        self.needed_by.hash_state(h);
        self.opened_at.hash_state(h);
        self.closes_at.hash_state(h);
        h.write_u8(self.incoterm as u8);
        h.write_u32(self.quotes.len() as u32);
        for q in &self.quotes {
            q.hash_state(h);
        }
        match self.outcome {
            RfqOutcome::Open => h.write_u8(0),
            RfqOutcome::Awarded { quote, seller } => {
                h.write_u8(1);
                h.write_u32(quote.0);
                seller.entity().hash_state(h);
            }
            RfqOutcome::NoQuotes => h.write_u8(2),
        }
    }
}

/// To, co wołający wie, zanim zapytanie powstanie.
#[derive(Clone, Copy, Debug)]
pub struct RfqDraft {
    pub buyer: FirmId,
    pub deliver_to: SiteId,
    pub to_slot: SlotId,
    pub good: GoodId,
    pub mass: Mass,
    pub min_quality: Q,
    pub needed_by: SimMinute,
    pub incoterm: WhoTransports,
}

/// Indeks dostawców per towar (§5.8).
///
/// `ponytail:` indeks jest **przebudowywany w całości**, a nie utrzymywany przyrostowo.
/// Sufit nazwany: przy 9 000 zakładów i przebudowie raz na dobę gry to jedno przejście
/// po liniach, czyli rząd 10⁴ operacji na 1 440 ticków. Droga wyjścia, gdyby pomiar z M6e
/// tego nie potwierdził: aktualizacja przy `Plant::insert` i przy zmianie receptury linii —
/// ale to wymaga, żeby ktokolwiek zmieniał recepturę w locie, a do M7 nikt tego nie robi.
///
/// Sprzedawcą jest zakład, którego **linia produkuje** ten towar. Nie „zakład, który ma go
/// na stanie": magazyn pełen mąki w piekarni nie czyni z niej młyna, a zapytanie wysłane
/// do odbiorcy tego samego towaru zabrałoby mu wsad i wpędziło w kaskadę niedoboru.
#[derive(Default)]
pub struct SellerIndex {
    /// Gęsty wektor po `GoodId` — klucz jest indeksem katalogu, więc mapa byłaby
    /// droższa i wprowadzałaby porządek iteracji tam, gdzie go nie potrzeba (00 §3.2).
    by_good: Vec<Vec<SiteId>>,
}

impl SellerIndex {
    #[must_use]
    pub fn new(goods: usize) -> SellerIndex {
        SellerIndex {
            by_good: vec![Vec::new(); goods],
        }
    }

    /// Przebudowa z aktualnego stanu zakładów. Kolejność wynika z `Plant::iter`,
    /// czyli z indeksu encji — jedynego dozwolonego porządku.
    pub fn rebuild(&mut self, cat: &Catalog, plant: &Plant) {
        for v in &mut self.by_good {
            v.clear();
        }
        for (site, p) in plant.iter() {
            for l in &p.lines {
                let Some(rid) = l.recipe else { continue };
                for o in &cat.recipe(rid).outputs {
                    let slot = &mut self.by_good[o.good.0 as usize];
                    if !slot.contains(&site) {
                        slot.push(site);
                    }
                }
            }
        }
    }

    #[must_use]
    pub fn sellers_of(&self, good: GoodId) -> &[SiteId] {
        self.by_good
            .get(good.0 as usize)
            .map_or(&[][..], Vec::as_slice)
    }
}

/// Zebranie ofert na otwarte zapytanie. Wołane raz, przy otwarciu — dostawca nie zmienia
/// zdania w trakcie okna, bo nie ma czym: jego wycena jest funkcją stanu magazynu,
/// a ten w oknie godzinnym i tak stoi.
#[allow(clippy::too_many_arguments)]
pub fn collect_quotes(
    rfq: &mut Rfq,
    cat: &Catalog,
    store: &Store,
    plant: &Plant,
    index: &SellerIndex,
    exclusives: &crate::b2b::Exclusives,
    relations: &crate::b2b::Relations,
    oracle: &dyn FreightOracle,
    t: &B2bTuning,
    world_seed: u64,
    next_quote: &mut u32,
) {
    let wymagania = VehicleRequirements::for_good(cat, rfq.good, rfq.mass);
    for &sprzedawca in index.sellers_of(rfq.good) {
        if rfq.quotes.len() >= t.max_candidates as usize {
            break;
        }
        if sprzedawca == rfq.deliver_to {
            continue;
        }
        // Wyłączność cudzego kontraktu (M7e WP14): ten zakład sprzedaje dziś komu
        // innemu. Nie jest to odmowa — pytający po prostu tego dostawcy nie widzi,
        // tak samo jak nie widzi zakładu z pustą wystawką.
        let wylacznosc = exclusives.holder(sprzedawca, rfq.good, rfq.opened_at);
        if wylacznosc.is_some_and(|l| l.holder != rfq.buyer) {
            continue;
        }
        let Some(p) = plant.get(sprzedawca) else {
            continue;
        };
        let Some((slot, masa, jakosc, koszt)) = wystawka(store, p, rfq.good, rfq.min_quality)
        else {
            continue;
        };
        let Some(przewoz) = oracle.quote(sprzedawca, rfq.deliver_to, rfq.mass, &wymagania) else {
            // Trasa niewykonalna — tonaż mostu, zakaz ruchu ciężkiego. Rozstrzygnięte
            // przy planowaniu (§6.2), więc dostawca po prostu nie startuje w przetargu.
            continue;
        };
        // Cena wyłączności: kto zabezpieczył sobie dostawcę, płaci mu premię.
        // Bez niej wyłączność byłaby darmowa, a reakcja na rywala ma boleć (`R12`).
        let cena = match wylacznosc {
            Some(l) => Money(
                z_narzutem(koszt, sprzedawca, rfq.opened_at, t, world_seed)
                    .0
                    .saturating_mul(i64::from(10_000 + l.premium_bp))
                    / 10_000,
            ),
            None => z_narzutem(koszt, sprzedawca, rfq.opened_at, t, world_seed),
        };
        let id = QuoteId(*next_quote);
        *next_quote += 1;
        rfq.quotes.push(Quote {
            id,
            rfq: rfq.id,
            seller: p.owner,
            from_site: sprzedawca,
            from_slot: slot,
            mass_available: masa,
            quality: jakosc,
            price: cena,
            transport: match rfq.incoterm {
                WhoTransports::Seller => Some(przewoz.cost),
                WhoTransports::Buyer => None,
            },
            ready_at: rfq.opened_at,
            eta: SimMinute(rfq.opened_at.0 + u64::from(przewoz.minutes)),
            valid_until: rfq.closes_at,
            freight_minutes: przewoz.minutes,
            // Preferencja stałego dostawcy stoi **obok** premii za wyłączność
            // i w tym samym miejscu: obie są wiedzą o relacji, a nie o towarze.
            // Wyłączność podnosi cenę, zaufanie obniża ocenę — i to jest cała
            // różnica między „zapłać mi więcej" a „wolę ciebie".
            discount_bp: relations.discount_bp(rfq.buyer, p.owner, rfq.good),
        });
    }
}

/// Co sprzedawca ma na wystawce: slot wyjściowy z największą masą towaru o wymaganej
/// jakości, wraz z jej średnim kosztem wytworzenia i średnią jakością ważoną masą.
///
/// Średnia **ważona masą**, a nie jakość pierwszej partii: FEFO wyda mieszankę, więc
/// obiecywanie jakości najlepszej partii byłoby obietnicą, której dostawa nie dotrzyma.
fn wystawka(
    store: &Store,
    p: &crate::plant::PlantSite,
    good: GoodId,
    min_q: Q,
) -> Option<(SlotId, Mass, Q, Money)> {
    let mut najlepszy: Option<(SlotId, Mass, Q, Money)> = None;
    for &slot in &p.outputs {
        let masa = store.available(slot, good, min_q);
        if masa.0 <= 0 {
            continue;
        }
        let Some(s) = store.slot(slot) else { continue };
        let (mut suma_masy, mut suma_kosztu, mut suma_jakosci) = (0i128, 0i128, 0i128);
        for &b in s.batches() {
            let Some(partia) = store.batch(b) else {
                continue;
            };
            if partia.good != good || partia.quality < min_q {
                continue;
            }
            suma_masy += i128::from(partia.mass.0);
            suma_kosztu += i128::from(partia.cost_total.0);
            suma_jakosci += i128::from(partia.mass.0) * i128::from(partia.quality.get());
        }
        if suma_masy <= 0 {
            continue;
        }
        let jakosc = Q::new((suma_jakosci / suma_masy) as u8);
        let koszt = Money((suma_kosztu * 1_000_000 / suma_masy) as i64);
        if najlepszy.is_none_or(|(_, m, _, _)| masa.0 > m.0) {
            najlepszy = Some((slot, masa, jakosc, koszt));
        }
    }
    najlepszy
}

/// Koszt wytworzenia powiększony o marżę i drobny szum (`StreamId::SupplyQuoteNoise`).
fn z_narzutem(
    koszt: Money,
    site: SiteId,
    opened_at: SimMinute,
    t: &B2bTuning,
    world_seed: u64,
) -> Money {
    let baza = koszt.mul_ratio(10_000 + t.seller_margin_bp, 10_000);
    if t.quote_noise_bp <= 0 {
        return baza;
    }
    let mut r = rng(
        world_seed,
        StreamId::SupplyQuoteNoise,
        site.entity().index(),
        Tick(opened_at.0),
    );
    let rozpietosc = (2 * t.quote_noise_bp + 1) as u32;
    let szum = i64::from(r.gen_range_u32(rozpietosc)) - t.quote_noise_bp;
    baza.mul_ratio(10_000 + szum, 10_000)
}

/// Rozstrzygnięcie zapytania: minimalizacja funkcji celu z §5.8.
///
/// `score = price*mass + transport + late_penalty*spóźnienie + quality_penalty*niedobór_jakości`
///
/// Remis rozstrzyga `(seller.entity_index, quote.id)` — **nigdy** kolejność wpisu
/// do kolekcji. To jest cała różnica między przebiegiem powtarzalnym a takim, który
/// zależy od tego, w jakiej kolejności scheduler obudził zakłady.
#[must_use]
pub fn score(rfq: &Rfq, q: &Quote, t: &B2bTuning) -> i128 {
    let masa = rfq.mass.0.min(q.mass_available.0);
    let towar = i128::from(q.price.0) * i128::from(masa) / 1_000_000;
    // Preferencja stałego dostawcy: jego oferta **liczy się** taniej, choć kosztuje
    // tyle samo. `score_raw` niżej jest tą samą funkcją bez tego członu i służy
    // do jednego pytania: czy to zaufanie rozstrzygnęło przetarg (M10e WP10.13).
    let preferencja = towar * i128::from(q.discount_bp) / 10_000;
    let przewoz = i128::from(q.transport.map_or(0, |m| m.0));
    let spoznienie = q.eta.0.saturating_sub(rfq.needed_by.0);
    let kara_czas =
        i128::from(spoznienie) * i128::from(t.late_penalty_gr_per_tonne_minute) * i128::from(masa)
            / 1_000_000;
    let brak_jakosci = i128::from(rfq.min_quality.get().saturating_sub(q.quality.get()));
    let kara_jakosc =
        brak_jakosci * i128::from(t.quality_penalty_gr_per_tonne_point) * i128::from(masa)
            / 1_000_000;
    towar - preferencja + przewoz + kara_czas + kara_jakosc
}

/// Ocena bez preferencji stałego dostawcy — do odpowiedzi na pytanie „czy zaufanie
/// rozstrzygnęło". Osobna funkcja, a nie flaga w [`score`]: flaga byłaby parametrem,
/// który w gorącej ścieżce zawsze ma tę samą wartość.
#[must_use]
pub fn score_raw(rfq: &Rfq, q: &Quote, t: &B2bTuning) -> i128 {
    let mut bez = *q;
    bez.discount_bp = 0;
    score(rfq, &bez, t)
}

/// Zwycięzca i jego przewaga nad drugim w punktach bazowych funkcji celu.
///
/// Przewaga zero znaczy **jedyną ofertę**, a nie remis: remis jest rozstrzygany
/// tie-breakiem i wtedy przewaga też wynosi zero, ale gracz widzi w powodzie liczbę
/// ofert i sam odróżni jedno od drugiego.
#[must_use]
pub fn best(rfq: &Rfq, t: &B2bTuning) -> Option<(usize, u16)> {
    let mut wynik: Option<(usize, i128)> = None;
    let mut drugi: Option<i128> = None;
    for (i, q) in rfq.quotes.iter().enumerate() {
        if q.valid_until.0 < rfq.closes_at.0 {
            continue;
        }
        let s = score(rfq, q, t);
        match wynik {
            Some((bi, bs))
                if (
                    bs,
                    rfq.quotes[bi].seller.entity().index(),
                    rfq.quotes[bi].id.0,
                ) <= (s, q.seller.entity().index(), q.id.0) =>
            {
                if drugi.is_none_or(|d| s < d) {
                    drugi = Some(s);
                }
            }
            _ => {
                if let Some((_, bs)) = wynik {
                    if drugi.is_none_or(|d| bs < d) {
                        drugi = Some(bs);
                    }
                }
                wynik = Some((i, s));
            }
        }
    }
    let (i, s) = wynik?;
    let przewaga = match drugi {
        Some(d) if s > 0 => (((d - s) * 10_000) / s).clamp(0, i128::from(u16::MAX)) as u16,
        _ => 0,
    };
    Some((i, przewaga))
}

/// Powód decyzji dla karty inspekcji i panelu łańcucha (00 §7).
#[must_use]
pub fn reason(rfq: &Rfq, q: &Quote, saving_bp: u16) -> DecisionReason {
    DecisionReason::SupplierChosen {
        good: rfq.good,
        seller: q.seller,
        quotes: rfq.quotes.len().min(usize::from(u16::MAX)) as u16,
        saving_bp,
    }
}

use crate::b2b::{B2b, SellerRef, Settlement};
use crate::transport::{Transport, TransportRequest};
use crate::tuning::Tuning;

// ── Rynek po stronie zasobu ─────────────────────────────────────────────────────
//
// Metody [`B2b`] mieszkają przy module, którego dotyczą, a nie w pliku rodzica.
// Rust pozwala na kilka bloków `impl` tego samego typu w jednym crate'cie, a moduł
// potomny widzi prywatne pola rodzica — więc podział nic nie kosztuje w dostępie
// i ratuje plik rodzica przed rolą worka na trzy pakiety robocze naraz.

impl B2b {
    // ── WP7: rynek spot ─────────────────────────────────────────────────────────

    /// Otwiera zapytanie i **od razu** zbiera oferty (§5.8).
    ///
    /// Zbieranie jest jednorazowe, bo wycena dostawcy jest czystą funkcją stanu jego
    /// magazynu, a ten w oknie godzinnym stoi. Okno zostaje mimo to: jest tym, co gracz
    /// widzi jako „zakład szuka na rynku spot", i tym, na czym M7 zawiesi politykę
    /// cenową firmy, która **będzie** zmieniać zdanie w trakcie.
    #[allow(clippy::too_many_arguments)]
    pub fn open_rfq(
        &mut self,
        d: RfqDraft,
        cat: &Catalog,
        store: &Store,
        plant: &Plant,
        oracle: &dyn FreightOracle,
        t: &Tuning,
        now: SimMinute,
    ) -> RfqId {
        let id = RfqId(self.next_rfq);
        self.next_rfq += 1;
        let mut r = Rfq {
            id,
            buyer: d.buyer,
            deliver_to: d.deliver_to,
            to_slot: d.to_slot,
            good: d.good,
            mass: d.mass,
            min_quality: d.min_quality,
            needed_by: d.needed_by,
            opened_at: now,
            closes_at: SimMinute(now.0 + u64::from(t.b2b.rfq_window_minutes)),
            incoterm: d.incoterm,
            quotes: Vec::new(),
            outcome: RfqOutcome::Open,
        };
        collect_quotes(
            &mut r,
            cat,
            store,
            plant,
            &self.sellers,
            &self.exclusives,
            &self.relations,
            oracle,
            &t.b2b,
            self.world_seed,
            &mut self.next_quote,
        );
        self.rfqs.insert(id.0, r);
        id
    }

    /// Rozstrzyga zapytania, których okno się zamknęło. Zwraca fakty do zaksięgowania.
    ///
    /// Zwycięzca dostaje zlecenie transportowe **wystawione i wysłane w tym samym kroku** —
    /// towar fizycznie opuszcza magazyn sprzedawcy przez [`Transport::dispatch`], czyli
    /// jedyną drogę, którą partia zmienia lokację (`prop_no_teleport`).
    #[allow(clippy::too_many_arguments)]
    pub fn resolve_due(
        &mut self,
        cat: &Catalog,
        store: &mut Store,
        transport: &mut Transport,
        oracle: &dyn FreightOracle,
        t: &Tuning,
        now: SimMinute,
    ) -> Vec<Settlement> {
        let mut wynik = Vec::new();
        // Kolejność: **priorytet stałego klienta, potem `RfqId`** (M10e WP10.13).
        // Do M10d rozstrzygał sam numer zapytania, czyli minuta otwarcia — a to
        // znaczyło, że w niedoborze wygrywa ten, kto zapytał pół minuty wcześniej.
        // Priorytet bierze się z zaufania, więc „stały dostawca ma priorytet
        // w niedoborze" z PRD §7.9 jest kolejnością tej listy, a nie gałęzią
        // w środku przydziału. Klucz zostaje w pełni deterministyczny: przy równym
        // priorytecie nadal rozstrzyga `RfqId` (00 §3.2).
        let mut dojrzale: Vec<(u8, u32)> = self
            .rfqs
            .iter()
            .filter(|(_, r)| r.outcome == RfqOutcome::Open && r.closes_at.0 <= now.0)
            .map(|(k, r)| (self.relations.buyer_priority(r.buyer, r.good), *k))
            .collect();
        dojrzale.sort_unstable_by_key(|(p, k)| (std::cmp::Reverse(*p), *k));
        for (_, k) in dojrzale {
            let Some(r) = self.rfqs.get_mut(&k) else {
                continue;
            };
            let Some((i, przewaga)) = best(r, &t.b2b) else {
                r.outcome = RfqOutcome::NoQuotes;
                continue;
            };
            let q = r.quotes[i];
            let masa = Mass(r.mass.0.min(q.mass_available.0));
            // Czy przetarg rozstrzygnęło zaufanie: bez preferencji wygrałby ktoś inny.
            // Pytanie zadaje się **tylko** wtedy, gdy preferencja w ogóle jest —
            // inaczej byłby to drugi przebieg po ofertach w każdym zapytaniu świata.
            let z_zaufania = q.discount_bp > 0
                && r.quotes
                    .iter()
                    .enumerate()
                    .any(|(j, o)| j != i && score_raw(r, o, &t.b2b) < score_raw(r, &q, &t.b2b));
            let powod = if z_zaufania {
                DecisionReason::TrustedSupplier {
                    supplier: q.seller,
                    trust: self
                        .relations
                        .get(r.buyer, q.seller, r.good)
                        .map_or(magnat_core::Q::MIN, |rel| rel.trust),
                    discount_bp: q.discount_bp,
                }
            } else {
                reason(r, &q, przewaga)
            };
            r.outcome = RfqOutcome::Awarded {
                quote: q.id,
                seller: q.seller,
            };
            let (good, kupujacy, do_zakladu, do_slotu) = (r.good, r.buyer, r.deliver_to, r.to_slot);
            let wyslane = self.wyslij(
                cat, store, transport, oracle, t, now, &q, good, masa, do_zakladu, do_slotu, powod,
            );
            // Zakup spotowy też buduje relację — i to jest jedyna droga, którą
            // relacja **powstaje** w świecie bez kontraktów długoterminowych.
            // Terminu tu nie ma, więc spóźnienia też nie ma; jest za to nieudana
            // wysyłka, i ona liczy się jako masa niedostarczona.
            self.relations.record(
                crate::b2b::DeliveryOutcome {
                    buyer: kupujacy,
                    supplier: q.seller,
                    good,
                    delivered: if wyslane.is_some() { masa } else { Mass::ZERO },
                    missed: if wyslane.is_some() { Mass::ZERO } else { masa },
                    late: false,
                },
                now,
            );
            if let Some(s) = wyslane {
                wynik.push(s);
            }
        }
        wynik
    }

    /// Wysyła towar od sprzedawcy do kupującego i zapisuje cenę w oknie spot.
    #[allow(clippy::too_many_arguments)]
    fn wyslij(
        &mut self,
        cat: &Catalog,
        store: &mut Store,
        transport: &mut Transport,
        oracle: &dyn FreightOracle,
        t: &Tuning,
        now: SimMinute,
        q: &Quote,
        good: GoodId,
        masa: Mass,
        do_zakladu: SiteId,
        do_slotu: SlotId,
        powod: DecisionReason,
    ) -> Option<Settlement> {
        if masa.0 <= 0 {
            return None;
        }
        let id = transport.order(
            oracle,
            TransportRequest {
                from: q.from_site,
                to: do_zakladu,
                from_slot: q.from_slot,
                to_slot: do_slotu,
                good,
                mass: masa,
                requires: VehicleRequirements::for_good(cat, good, masa),
                ready_at: now,
                due_at: SimMinute(now.0 + u64::from(q.freight_minutes) + 60),
            },
            powod,
        );
        transport
            .dispatch(
                oracle,
                store,
                id,
                crate::transport::Carrier::Unassigned,
                now,
            )
            .ok()?;
        // Towar zmienił właściciela — koszt własny musi stać się **ceną zapłaconą**
        // przez kupującego, a nie zostać kosztem wytworzenia sprzedawcy (`AP-7`).
        let cena = q.goods_value(masa);
        let koszt_sprzedawcy = if let Some(o) = transport.get(id) {
            let cargo = o.cargo.clone();
            store.resell(&cargo, cena)
        } else {
            Money::ZERO
        };
        let _ = t;
        self.spot[good.0 as usize].record(q.price, masa);
        Some(Settlement {
            buyer: FirmId(do_zakladu.entity()),
            deliver_to: do_zakladu,
            seller: SellerRef::Firm(q.seller),
            seller_site: Some(q.from_site),
            seller_cogs: koszt_sprzedawcy,
            good,
            mass: masa,
            net: cena,
            duty: Money::ZERO,
            order: Some(id),
            reason: powod,
        })
    }
}
