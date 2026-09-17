//! Funkcja użyteczności zakupu i wybór oferty (M5b §5.4, PRD §6.4).
//!
//! Trzy rzeczy są tu nienegocjowalne i wszystkie trzy wynikają z dokumentu 00:
//!
//! 1. **Tylko `core::det_math`** — żadnego `f64::ln`, `f64::exp` ani `libm` (`K-6`).
//!    Systemowe `ln`/`exp` nie są bit-identyczne między platformami, a funkcja
//!    użyteczności używa obu; test hashy Windows vs. Linux wyłapałby obejście
//!    dopiero po fakcie, więc lint stoi w `clippy.toml` i jest twardy.
//! 2. **Kolejność sumowania jest ustalona sortowaniem kandydatów**, nie przypadkiem:
//!    `det_math::softmax_pick` sumuje w kolejności indeksów wejściowych bez redukcji
//!    parami, więc to sortowanie **przed** wywołaniem jest jedynym miejscem, w którym
//!    kolejność się ustala.
//! 3. **Softmax, nie argmax** (PRD §6.4). Argmax produkuje monopole: wszyscy idą
//!    do najtańszego, konkurencja pada, monopolista windzi ceny. Bramka G6 (HHI)
//!    w balansatorze pilnuje, żeby kalibracja temperatury tam nie zjechała.
//!
//! Liczby są `f64`, nie `f32` jak w pierwotnym planie §5.4: `det_math` nie ma
//! wariantów `f32`, a `clippy.toml` zakazuje `f32::ln`/`exp` wprost słowami
//! „symulacja nie używa f32". Pieniądza to nie dotyka — do funkcji wchodzi on
//! wyłącznie jako **stosunek** kwoty do budżetu odniesienia, a wychodzi jako wybór.

use magnat_agents::{Knowledge, KnowledgeKind};
use magnat_core::{
    det_math, mix64, rng, GoodId, Money, Qty, RejectCause, SiteId, StockCat, StreamId, Tick,
    UtilityKind, Q,
};

use crate::data::{EconomyData, UtilityWeights};
use crate::offer::OfferId;

/// Kandydat decyzji zakupowej: jedna oferta jednego sklepu, wyceniona na ilość,
/// której kupujący chce.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Candidate {
    pub offer: OfferId,
    pub site: SiteId,
    pub good: GoodId,
    pub qty: Qty,
    /// `unit_price × qty` — kwota, którą kupujący wyjmie z portfela (`K-7`).
    pub price_total: Money,
    pub travel_min: u16,
    /// Pieniężna część kosztu dojazdu (bilet, paliwo).
    ///
    /// `ponytail:` w M5b zawsze zero. Sufit nazwany: `PlaceProvider::candidates`
    /// nie dostaje `TravelOracle`, a wołanie go 3–15 razy na decyzję to dokładnie
    /// koszt, przed którym ostrzegają ryzyka R6 i R7 dokumentu fazy (`estimate`
    /// wycenia sześć opcji z routingiem i woła się już ponad milion razy na dobę
    /// **bez** udziału M5). Ścieżka wyjścia jest zapisana w §9 pkt 15 dokumentu
    /// fazy: `PlaceCandidate` dostaje `travel_cost: Money` wypełniane przez tego,
    /// kto i tak zna `travel_min` — jedno wywołanie zamiast piętnastu.
    pub travel_money: Money,
    pub quality: Q,
    /// Ocena sklepu z pamięci mieszkańca, 0..=100; `None` = zna, ale nie był.
    pub rating: Option<u8>,
    /// Czy mieszkaniec już tu był (premia za nowość dotyczy tylko tych, gdzie nie był).
    pub visited: bool,
}

/// To, co kupujący wnosi do funkcji użyteczności poza samą ofertą.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct BuyerState {
    pub status: Q,
    pub openness: Q,
    /// Mianownik członu ceny i odległości (§5.4). M5d podmieni na kopertę.
    pub budget_ref: Money,
    /// Wartość czasu w groszach na minutę.
    pub vot_gr_per_min: i64,
}

/// `f(x) = −ln(1 + x)` dla `x ≥ 0` (§5.4).
///
/// Jedna funkcja dla ceny **i** odległości, bo obie są wyrażone jako ułamek budżetu
/// i przez to porównywalne. `f(0) = 0`, malejąca, o malejącej wrażliwości: kto już
/// wydał pół budżetu, nie rozróżnia drobnych różnic.
///
/// Od M7f mieszka w jądrze (`D20`), bo liczy ją także krok makro; tutaj zostaje
/// nazwa, pod którą chodzi po M5.
pub use crate::kernel::{cost_term, status_fit};

/// Wagi członów wyprowadzone z osobowości i statusu, znormalizowane do `Σ|w| = 1`.
///
/// **Elastyczność cenowa nie jest tu parametrem** — jest własnością rozkładu wagi
/// `price` w populacji i dostępności alternatyw (PRD §6.4). Balansator ją mierzy,
/// nie ustawia.
#[must_use]
pub fn weights_for(
    p: &magnat_agents::Personality,
    status: Q,
    need: magnat_core::NeedKind,
    d: &EconomyData,
) -> UtilityWeights {
    let base = d.base_weights(need);
    let t = |id: magnat_core::TraitId| f64::from(p.get(id).get()) / 100.0;
    let s = f64::from(status.get()) / 100.0;
    UtilityWeights {
        price: base.price * (0.5 + t(magnat_core::TraitId::PriceSensitivity)) * (1.4 - 0.8 * s),
        quality: base.quality * (0.6 + 0.8 * s),
        // Mobilność gospodarstwa wnosi M4 (pojazdy) i M5d (budżet na dojazdy);
        // dopóki jej nie ma, oszczędność zastępuje ją jako skłonność do chodzenia
        // dalej za tańszym. `ponytail:` sufit nazwany, ścieżka wyjścia w M5d.
        dist: base.dist * (0.6 + 0.8 * (1.0 - t(magnat_core::TraitId::Thrift))),
        loyalty: base.loyalty * t(magnat_core::TraitId::Loyalty),
        novelty: base.novelty * t(magnat_core::TraitId::Openness),
        status: base.status * s * (t(magnat_core::TraitId::Ambition) + 0.5),
        // M5: mnożone przez afinitet ≡ 0. Człon istnieje, żeby M10 dopisał wartość,
        // a nie strukturę.
        brand: base.brand,
    }
    .normalized()
}

/// Użyteczność jednej oferty (§5.4). `noise` przychodzi z zewnątrz, bo musi być
/// **stały dla trójki (kupujący, oferta, decyzja)** — inaczej ponowna ewaluacja
/// dałaby inny wynik.
#[must_use]
pub fn utility_of_offer(c: &Candidate, w: &UtilityWeights, st: &BuyerState, noise: f64) -> f64 {
    crate::kernel::purchase_score(&score_input(c, st), w, noise)
}

/// Zamiana oferty i kupującego na same liczby, których chce jądro.
///
/// To jest cała treść wydzielenia z `D20`: człony liczy `kernel::purchase_score`,
/// a tutaj zostaje wiedza o tym, czym jest oferta, pamięć miejsca i wartość czasu —
/// czyli rzeczy, których model makro nie ma i mieć nie powinien.
fn score_input(c: &Candidate, st: &BuyerState) -> crate::kernel::ScoreInput {
    let travel = c
        .travel_money
        .get()
        .saturating_add(i64::from(c.travel_min).saturating_mul(st.vot_gr_per_min));
    let loyalty = match c.rating {
        // Skurcz `n/(n+3)` z planu §5.4 zastąpiony rodzajem wiedzy: `Knowledge`
        // trzyma **jeden** wpis na cel i nie liczy wizyt, więc licznika `visits`
        // po prostu nie ma. Wizyta waży trzy razy tyle co zasłyszenie; licznik
        // wizyt wnosi M10 razem z plotką i wtedy wraca wzór z planu.
        Some(r) => (f64::from(r) - 50.0) / 50.0 * if c.visited { 0.75 } else { 0.25 },
        None => 0.0,
    };
    let novelty = if c.visited {
        0.0
    } else {
        f64::from(st.openness.get()) / 100.0
    };
    crate::kernel::ScoreInput {
        price_total: c.price_total,
        budget_ref: st.budget_ref,
        travel_cost: Money(travel),
        quality: c.quality,
        status: st.status,
        loyalty,
        // M5: afinitet marki ≡ 0. Człon istnieje, żeby M10 dopisał wartość,
        // a nie strukturę.
        brand: 0.0,
        novelty,
    }
}

/// Który człon przeważył — ładunek `DecisionReason::ShopChosen` (PRD §14.1).
///
/// „Przeważył" znaczy: ma największy **wkład bezwzględny** do sumy. Człon o wadze
/// 0,4 i wartości 0 nie tłumaczy wyboru; człon o wadze 0,1 i wartości −2 tak.
#[must_use]
pub fn dominant_term(c: &Candidate, w: &UtilityWeights, st: &BuyerState) -> UtilityKind {
    let budget = st.budget_ref.get().max(1) as f64;
    let travel = c
        .travel_money
        .get()
        .saturating_add(i64::from(c.travel_min).saturating_mul(st.vot_gr_per_min));
    let wklady = [
        (
            UtilityKind::Price,
            (w.price * cost_term(c.price_total.get() as f64 / budget)).abs(),
        ),
        (
            UtilityKind::Quality,
            (w.quality * f64::from(c.quality.get()) / 100.0).abs(),
        ),
        (
            UtilityKind::Distance,
            (w.dist * cost_term(travel as f64 / budget)).abs(),
        ),
        (
            UtilityKind::Habit,
            (w.loyalty * c.rating.map_or(0.0, |r| (f64::from(r) - 50.0) / 50.0)).abs(),
        ),
        (
            UtilityKind::Convenience,
            (w.status * status_fit(c.quality, st.status)).abs(),
        ),
        (
            UtilityKind::Variety,
            if c.visited {
                0.0
            } else {
                (w.novelty * f64::from(st.openness.get()) / 100.0).abs()
            },
        ),
    ];
    // Iteracja po tablicy, nie po mapie; remis rozstrzyga kolejność deklaracji.
    let mut best = wklady[0];
    for k in &wklady[1..] {
        if k.1 > best.1 {
            best = *k;
        }
    }
    best.0
}

/// Wynik wyboru.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Choice {
    Buy { idx: usize },
    Defer { cause: RejectCause },
}

/// Szum decyzji — **stały dla trójki (kupujący, oferta, decyzja)** (§5.4).
///
/// Klucz jest złożony z tożsamości kupującego, uchwytu oferty i ticka, a nie
/// z pozycji kandydata w rankingu. To rozróżnienie **ma skutki mierzalne**:
/// przy szumie zależnym od pozycji podniesienie ceny w jednym sklepie przesuwa
/// kolejność sortowania, a wtedy oferty zamieniają się szumem i udział rynkowy
/// przestaje reagować na cenę monotonicznie. Kryterium WP4 wyłapało to od razu.
///
/// `mix64` jest tym samym mieszalnikiem, który rozprasza ziarna w `core::rng`
/// (upubliczniony w M5b) — nie drugim własnym.
#[must_use]
pub fn offer_noise(seed: u64, buyer_index: u32, offer: OfferId, t: Tick, sigma: f64) -> f64 {
    let klucz = mix64(
        mix64(seed ^ (u64::from(buyer_index) << 1))
            ^ mix64(offer.to_bits())
            ^ mix64(t.get().wrapping_mul(0x9E37_79B9_7F4A_7C15)),
    );
    // 53 bity mantysy — ta sama konwersja co w `det_math::softmax_pick`.
    let u = (klucz >> 11) as f64 / 9_007_199_254_740_992.0;
    (u * 2.0 - 1.0) * sigma
}

/// Wybór oferty z rozkładu softmax (§6.4 — **nie** argmax).
///
/// `cands` musi być już posortowane deterministycznie, bo `softmax_pick` sumuje
/// w kolejności indeksów wejściowych.
#[must_use]
pub fn choose_offer(
    utils: &[f64],
    temperature: f64,
    seed: u64,
    buyer_index: u32,
    t: Tick,
) -> Option<usize> {
    if utils.is_empty() {
        return None;
    }
    let mut r = rng(seed, StreamId::PurchaseChoice, buyer_index, t);
    Some(det_math::softmax_pick(utils, temperature, &mut r))
}

/// Próg odłożenia zakupu (§6.4).
///
/// ```text
/// U_threshold = thr0(need)
///             − k_urgency · (100 − satisfaction)/100     // głodny kupi drożej
///             + k_envelope · max(0, overspend_bp)/10_000 // wyczerpana koperta podnosi próg
/// ```
///
/// `overspend_bp` jest w M5b zawsze zerem: koperty wydatków powstają w M5d/WP8.
/// Parametr zostaje w sygnaturze, bo to on odróżnia „nie stać mnie" od „nie warto",
/// a obie odpowiedzi trafiają do karty inspekcji.
#[must_use]
pub fn purchase_threshold(
    need: magnat_core::NeedKind,
    satisfaction: Q,
    overspend_bp: i32,
    d: &EconomyData,
) -> f64 {
    let s = d.threshold(need);
    s.thr0 - s.k_urgency * (100.0 - f64::from(satisfaction.get())) / 100.0
        + s.k_envelope * f64::from(overspend_bp.max(0)) / 10_000.0
}

/// Ilość, której gospodarstwo chce: `dni × osoby × zużycie na osobę na dobę`.
///
/// Zaokrąglenie w górę do pełnej jednostki ceny (`PRICE_UNIT`) byłoby fałszem —
/// chleb kupuje się na wagę. Zwraca co najmniej jedną jednostkę, żeby zakup
/// nigdy nie był zerowy.
#[must_use]
pub fn wanted_qty(daily_per_person: Qty, persons: u8, days: u8) -> Qty {
    let q = daily_per_person
        .get()
        .saturating_mul(i64::from(persons.max(1)))
        .saturating_mul(i64::from(days.max(1)));
    Qty(q.max(1))
}

/// Ile dni zapasu kategorii daje kupiona ilość — odwrotność [`wanted_qty`].
#[must_use]
pub fn days_bought(daily_per_person: Qty, persons: u8, qty: Qty) -> u8 {
    let per_day = daily_per_person
        .get()
        .saturating_mul(i64::from(persons.max(1)))
        .max(1);
    (qty.get() / per_day).clamp(0, 255) as u8
}

/// Ocena i rodzaj wiedzy o miejscu — wejście członów lojalności i nowości.
#[must_use]
pub fn rating_of(k: Option<&Knowledge>) -> (Option<u8>, bool) {
    match k {
        Some(e) => (Some(e.score), e.kind == KnowledgeKind::Visited as u8),
        None => (None, false),
    }
}

/// Mianownik członu ceny dla kategorii (§5.4). Wydzielone, bo M5d podmienia
/// **to jedno wywołanie** na `budget_ref_for_need` liczone z koperty gospodarstwa.
#[must_use]
pub fn budget_ref_for(cat: StockCat, d: &EconomyData) -> Money {
    d.budget_ref(cat)
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::Entity;
    use std::num::NonZeroU32;

    fn kandydat(price: i64, quality: u8, travel: u16) -> Candidate {
        Candidate {
            offer: OfferId::from_bits(1 << 32).unwrap(),
            site: SiteId(Entity::new(1, NonZeroU32::MIN)),
            good: GoodId(1),
            qty: Qty(1_000),
            price_total: Money(price),
            travel_min: travel,
            travel_money: Money::ZERO,
            quality: Q::new(quality),
            rating: None,
            visited: false,
        }
    }

    fn stan() -> BuyerState {
        BuyerState {
            status: Q::new(50),
            openness: Q::new(50),
            budget_ref: Money(6_000),
            vot_gr_per_min: 9,
        }
    }

    #[test]
    fn czlon_kosztu_jest_malejacy_i_zeruje_sie_w_zerze() {
        assert!((cost_term(0.0)).abs() < 1e-12);
        let a = cost_term(0.1);
        let b = cost_term(0.5);
        let c = cost_term(2.0);
        assert!(a > b && b > c, "{a} {b} {c}");
        // Malejąca wrażliwość: różnica 0,1 → 0,5 jest większa niż 2,0 → 2,4.
        assert!(a - b > cost_term(2.0) - cost_term(2.4));
    }

    #[test]
    fn drozsza_oferta_ma_nizsza_uzytecznosc() {
        let d = EconomyData::load_default().expect("data/economy/");
        let p = magnat_agents::Personality([50; 8]);
        let w = weights_for(&p, Q::new(50), magnat_core::NeedKind::Hunger, &d);
        let st = stan();
        let tanio = utility_of_offer(&kandydat(1_000, 50, 5), &w, &st, 0.0);
        let drogo = utility_of_offer(&kandydat(1_500, 50, 5), &w, &st, 0.0);
        assert!(drogo < tanio, "{drogo} !< {tanio}");
        // I dalej: o 10 % drożej zawsze gorzej, dla każdej ceny bazowej.
        for baza in [300i64, 900, 2_500, 9_000] {
            let a = utility_of_offer(&kandydat(baza, 50, 5), &w, &st, 0.0);
            let b = utility_of_offer(&kandydat(baza * 11 / 10, 50, 5), &w, &st, 0.0);
            assert!(b < a, "baza {baza}: {b} !< {a}");
        }
    }

    #[test]
    fn dalszy_sklep_ma_nizsza_uzytecznosc() {
        let d = EconomyData::load_default().expect("data/economy/");
        let p = magnat_agents::Personality([50; 8]);
        let w = weights_for(&p, Q::new(50), magnat_core::NeedKind::Hunger, &d);
        let st = stan();
        let blisko = utility_of_offer(&kandydat(1_000, 50, 4), &w, &st, 0.0);
        let daleko = utility_of_offer(&kandydat(1_000, 50, 25), &w, &st, 0.0);
        assert!(daleko < blisko);
    }

    #[test]
    fn wrazliwosc_cenowa_przesuwa_wage_ceny() {
        let d = EconomyData::load_default().expect("data/economy/");
        let mut oszczedny = magnat_agents::Personality([50; 8]);
        oszczedny.set(magnat_core::TraitId::PriceSensitivity, Q::new(100));
        let mut rozrzutny = magnat_agents::Personality([50; 8]);
        rozrzutny.set(magnat_core::TraitId::PriceSensitivity, Q::new(0));
        let a = weights_for(&oszczedny, Q::new(50), magnat_core::NeedKind::Hunger, &d);
        let b = weights_for(&rozrzutny, Q::new(50), magnat_core::NeedKind::Hunger, &d);
        assert!(a.price > b.price, "{} !> {}", a.price, b.price);
        // Normalizacja trzyma obie na wspólnej skali.
        for w in [a, b] {
            let s = w.price + w.quality + w.brand + w.dist + w.loyalty + w.status + w.novelty;
            assert!((s - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn status_nie_kupuje_skrajnosci() {
        // Elita przy najtańszym i najbiedniejszy przy luksusie dostają to samo
        // dopasowanie — bo to jest ta sama odległość w tierach.
        assert!((status_fit(Q::new(0), Q::new(100)) - 0.0).abs() < 1e-12);
        assert!((status_fit(Q::new(100), Q::new(0)) - 0.0).abs() < 1e-12);
        assert!((status_fit(Q::new(50), Q::new(50)) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn wybor_jest_powtarzalny_i_nie_jest_argmaxem() {
        let utils = vec![0.10, 0.09, 0.08, -0.40];
        let a = choose_offer(&utils, 0.35, 7, 42, Tick(100));
        let b = choose_offer(&utils, 0.35, 7, 42, Tick(100));
        assert_eq!(a, b, "ta sama decyzja musi dać ten sam wybór");
        // Przy tej temperaturze najgorszy kandydat bywa wybierany rzadko, ale
        // najlepszy **nie** wygrywa zawsze — inaczej mielibyśmy argmax i monopol.
        let rozne: std::collections::BTreeSet<usize> = (0..400u32)
            .filter_map(|i| choose_offer(&utils, 0.35, 7, i, Tick(1)))
            .collect();
        assert!(rozne.len() > 1, "softmax zdegenerował się do argmax");
    }

    #[test]
    fn prog_rosnie_wraz_z_zaspokojeniem() {
        let d = EconomyData::load_default().expect("data/economy/");
        let glodny = purchase_threshold(magnat_core::NeedKind::Hunger, Q::new(5), 0, &d);
        let syty = purchase_threshold(magnat_core::NeedKind::Hunger, Q::new(95), 0, &d);
        assert!(glodny < syty, "głodny ma kupić drożej: {glodny} !< {syty}");
    }

    #[test]
    fn ilosc_i_dni_sa_wzajemnie_odwrotne() {
        let daily = Qty(150);
        let q = wanted_qty(daily, 3, 4);
        assert_eq!(q, Qty(1_800));
        assert_eq!(days_bought(daily, 3, q), 4);
        assert_eq!(days_bought(daily, 3, Qty(0)), 0);
        // Zerowa liczebność i zero dni nie produkują zerowego zakupu.
        assert!(wanted_qty(daily, 0, 0).get() > 0);
    }
}
