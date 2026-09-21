//! Kryterium zamknięcia WP10.9, zdanie po zdaniu:
//!
//! 1. **odblokowanie kategorii „telefon komórkowy" rzeczywiście zmienia koszyk
//!    potrzeby „kontakt społeczny"** — przed nim potrzeba `Social` nie miała żadnej
//!    kategorii zakupowej i zaspokajała się wyłącznie wizytą, po nim ma towar;
//! 2. **tworzy nowy łańcuch dostaw** — towar dociera na półkę drogą, której wcześniej
//!    nie było, a walidator grafu towarów zostaje zielony;
//! 3. **mieszkańcy z wysoką otwartością kupują pierwsi — mierzalne w rozkładzie**.
//!
//! Świat jest mały i policzalny na kartce, tak samo jak w `shop_and_purchase.rs`:
//! jeden sklep specjalistyczny, jeden towar i mieszkańcy różniący się **wyłącznie**
//! otwartością. Gdyby różnili się czymkolwiek innym, test mierzyłby to coś innego.

mod common;

use magnat_agents::{
    ArrayVec, FulfilOutcome, FulfilRequest, PlaceCandidate, PlaceProvider, MAX_CANDIDATES,
};
use magnat_core::{
    GoodId, HouseholdId, Money, NeedKind, PlaceKind, PlaceRef, Qty, StockCat, Tick, TraitId,
};
use magnat_economy::EconomyData;
use magnat_spatial::Vec2;

use common::{bench_pelny, buyer, ent, good_by_key, knows, view, WHOLESALE_BASE};

const TELEFON: &str = "cons_mobile_phone";

fn telefon() -> GoodId {
    good_by_key(
        &EconomyData::load_default().expect("data/economy/"),
        TELEFON,
    )
}

/// Sklep specjalistyczny z telefonem **poza obiegiem**.
fn sklep_z_blokada() -> common::Bench {
    bench_pelny(
        7,
        &[Vec2::new(200.0, 0.0)],
        Some(0),
        PlaceKind::Clothing,
        &[telefon()],
    )
}

#[test]
fn potrzeba_kontaktu_ma_wreszcie_kategorie_zakupowa() {
    // Do M10c `StockCat::need()` nie odwzorowywał **żadnej** kategorii na `Social`,
    // więc „kontakt społeczny" był wyłącznie wizytą w miejscu. To jest pierwsza
    // połowa kryterium i najcichsza: bez niej telefon mógłby stać na półce i nikt
    // nigdy by po niego nie wyszedł.
    assert_eq!(StockCat::Comms.need(), NeedKind::Social);
    let kategorie: Vec<StockCat> = StockCat::ALL
        .iter()
        .copied()
        .filter(|c| c.need() == NeedKind::Social)
        .collect();
    assert_eq!(kategorie, vec![StockCat::Comms]);
}

#[test]
fn przed_odblokowaniem_telefonu_nie_ma_w_miescie() {
    let b = sklep_z_blokada();
    let g = telefon();
    assert!(b.market.is_good_locked(g));
    // Sklep specjalistyczny handluje `Comms`, ale półka jest bez telefonu: blokada
    // stanęła **przed** obsadzeniem asortymentu, tak samo jak przy stawianiu świata.
    assert_eq!(b.market.shelf_qty(b.sites[0], g), None);
}

#[test]
fn odblokowanie_stawia_telefon_na_polce_i_dzieje_sie_raz() {
    let b = sklep_z_blokada();
    let g = telefon();
    let ile = b.market.stock_new_good(g, Tick(0));
    assert_eq!(ile, 1, "sklep kategorii nie przyjął nowego towaru");
    assert!(!b.market.is_good_locked(g));
    assert_eq!(b.market.shelf_qty(b.sites[0], g), Some(Qty::ZERO));
    // Drugie odkrycie tego samego węzła niczego nie wypuszcza po raz drugi.
    assert_eq!(b.market.stock_new_good(g, Tick(0)), 0);
}

#[test]
fn nowy_towar_dociera_na_polke_i_da_sie_go_kupic() {
    let b = sklep_z_blokada();
    let g = telefon();
    b.market.stock_new_good(g, Tick(0));
    zatowaruj(&b, g, 40);
    let nabywca = kup(&b, 95, 1);
    assert!(
        matches!(nabywca, FulfilOutcome::Done { .. }),
        "otwarty mieszkaniec nie kupił nowego towaru: {nabywca:?}"
    );
}

#[test]
fn otwarci_oceniaja_nowy_towar_wyzej_i_rosnie_to_z_otwartoscia() {
    // **Gdzie dokładnie mierzymy.** Kryterium mówi „mieszkańcy z wysoką otwartością
    // kupują pierwsi — mierzalne w rozkładzie". Mierzymy to na **ocenie decyzji**,
    // a nie na liczbie zakupów, i to jest świadomy wybór: w tym świecie testowym
    // o zakupie rozstrzyga najpierw stać/nie stać (próg budżetu jest twardy),
    // a dopiero potem użyteczność. Liczba kupujących skacze z zera na sto przy
    // jednej wartości budżetu i nie pokazuje **niczego** o otwartości.
    //
    // Ocena pokazuje mechanizm wprost: człon nowości wchodzi do niej z wagą
    // otwartości, więc różnica „towar nowy minus ten sam towar po roku" jest zerem
    // dla mieszkańca zamkniętego i rośnie monotonicznie z otwartością. To ona
    // rozstrzyga, czyj sklep wygra przy pierwszej dostawie.
    let b = sklep_z_blokada();
    let g = telefon();
    b.market.stock_new_good(g, Tick(0));
    zatowaruj(&b, g, 400);

    let nowy: Vec<i32> = (0..100u32).map(|i| ocena(&b, i as u8, 1_000 + i)).collect();
    // Rok i jeden dzień później ten sam towar nowy już nie jest.
    b.market.set_tick(Tick(361 * 1_440));
    let stary: Vec<i32> = (0..100u32).map(|i| ocena(&b, i as u8, 2_000 + i)).collect();

    let premia: Vec<i32> = nowy.iter().zip(&stary).map(|(a, b)| a - b).collect();

    // **Dlaczego dziesiątkami, a nie punkt po punkcie.** Ocena niesie szum decyzji
    // — stały dla trójki (kupujący, oferta, decyzja), ale różny między dwiema
    // chwilami, w których mierzymy. Na pojedynczym mieszkańcu szum bywa większy
    // od premii; na dziesiątce już nie. Test punktowy mierzyłby szum i zapalałby
    // się losowo, czyli byłby gorszy od braku testu.
    let dziesiatki: Vec<i32> = premia
        .chunks(10)
        .map(|c| c.iter().sum::<i32>() / 10)
        .collect();
    assert_eq!(dziesiatki.len(), 10);
    assert!(
        dziesiatki[0] < dziesiatki[9] / 3,
        "premia najzamkniętszej dziesiątki {} nie jest wyraźnie niższa od najotwartszej {}",
        dziesiatki[0],
        dziesiatki[9]
    );
    assert!(
        dziesiatki[9] > 20,
        "najotwartsi nie dostali premii za nowość: {}",
        dziesiatki[9]
    );
    // Trend jest rosnący: górna połowa bije dolną w każdej odpowiadającej sobie
    // dziesiątce. To jest „mierzalne w rozkładzie" z kryterium WP10.9.
    for k in 0..5 {
        assert!(
            dziesiatki[5 + k] > dziesiatki[k],
            "dziesiątka {} ({}) nie bije dziesiątki {} ({})",
            5 + k,
            dziesiatki[5 + k],
            k,
            dziesiatki[k]
        );
    }
}

#[test]
fn towar_ktory_przestal_byc_nowy_przestaje_dawac_premie() {
    // Kontrola negatywna dla mechanizmu wyżej: gdyby premia brała się z czegokolwiek
    // innego niż nowość, upływ roku by jej nie zdjął.
    let b = sklep_z_blokada();
    let g = telefon();
    b.market.stock_new_good(g, Tick(0));
    zatowaruj(&b, g, 400);
    let swiezy = ocena(&b, 99, 1);
    b.market.set_tick(Tick(361 * 1_440));
    let stary = ocena(&b, 99, 2);
    assert!(swiezy > stary, "nowość nie wygasła: {swiezy} wobec {stary}");
    // A sam towar dalej stoi na półce i dalej da się go kupić — wygasła nowość,
    // a nie dostępność.
    assert!(b
        .market
        .shelf_qty(b.sites[0], g)
        .is_some_and(|q| q.get() > 0));
}

// ── rusztowanie ──────────────────────────────────────────────────────────────────

/// Dostawa i wyłożenie na półkę.
fn zatowaruj(b: &common::Bench, g: GoodId, units: i64) {
    b.market.deliver_now(
        b.sites[0],
        g,
        Qty(units * 1_000),
        Money(units * WHOLESALE_BASE),
        None,
        Tick(0),
    );
    b.market.restock_shelves();
    b.market.rebuild_index(&magnat_jobs::JobPool::new(1));
}

/// Ocena, jaką mieszkaniec o zadanej otwartości wystawia sklepowi z telefonem.
///
/// To jest liczba, którą rynek zwraca planowi dnia (`PlaceCandidate::score`),
/// czyli użyteczność decyzji razy tysiąc. Na niej rozstrzyga się, do którego
/// sklepu mieszkaniec pójdzie — i to w niej widać człon nowości.
fn ocena(b: &common::Bench, otwartosc: u8, kto: u32) -> i32 {
    let mut osoba = buyer(kto, 50);
    osoba.personality.0[TraitId::Openness.as_index()] = otwartosc;
    let k = knows(&[PlaceRef::Site(b.sites[0]), b.home]);
    let mut out: ArrayVec<PlaceCandidate, MAX_CANDIDATES> = ArrayVec::new();
    b.market.candidates(
        NeedKind::Social,
        b.home,
        60,
        &view(&k),
        &osoba.view(kto),
        &mut out,
    );
    out.iter()
        .find(|c| c.place == PlaceRef::Site(b.sites[0]))
        .map_or(i32::MIN, |c| c.score)
}

/// Jedna próba zakupu przez mieszkańca o zadanej otwartości.
///
/// Pełna ścieżka, nie skrót: **najpierw wybór sklepu**, potem zakup. Otwartość
/// wchodzi do decyzji przez `candidates`, bo to tam powstaje plan zakupu z wagami
/// i progiem — `fulfil` bez planu traktuje kupującego jako przeciętnego i różnicy
/// w otwartości by nie zobaczył.
fn kup(b: &common::Bench, otwartosc: u8, kto: u32) -> FulfilOutcome {
    let mut osoba = buyer(kto, 50);
    osoba.personality.0[TraitId::Openness.as_index()] = otwartosc;
    let k = knows(&[PlaceRef::Site(b.sites[0]), b.home]);
    let mut out: ArrayVec<PlaceCandidate, MAX_CANDIDATES> = ArrayVec::new();
    b.market.candidates(
        NeedKind::Social,
        b.home,
        60,
        &view(&k),
        &osoba.view(kto),
        &mut out,
    );
    b.market.fulfil(&FulfilRequest {
        citizen: magnat_core::CitizenId(ent(kto)),
        household: HouseholdId(ent(kto)),
        need: NeedKind::Social,
        place: PlaceRef::Site(b.sites[0]),
        at: magnat_core::SimMinute(600),
        budget_hint: Money(50_000_000),
        household_size: 1,
        household_children: 0,
        brands: Default::default(),
    })
}
