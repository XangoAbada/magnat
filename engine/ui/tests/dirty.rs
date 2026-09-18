//! Kryterium WP3: **brak zmian danych → 0 alokacji i 0 ms przebudowy** (`M9b` §5.8).
//!
//! Mierzymy to tam, gdzie leży koszt: na przebudowie **modelu** panelu (`DE-2`).
//! Karta sklepu bierze migawkę rynku i składa kilkaset napisów z katalogu — rysowanie
//! kilkuset widgetów `egui` jest przy tym szumem, a licznik alokacji na drzewie
//! widgetów mierzyłby `egui`, a nie nas.
//!
//! Alokator liczący siedzi na całym pliku testowym, bo `#[global_allocator]` jest
//! globalny z definicji. To jest jedyny sposób, żeby „zero alokacji" było liczbą,
//! a nie deklaracją — i zarazem powód, dla którego **cały pomiar jest jednym testem**:
//! licznik jest wspólny dla procesu, a testy chodzą równolegle, więc drugi test
//! w tym pliku dokładałby alokacje do cudzego pomiaru.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use magnat_ui::{Cached, Catalog, DataSource, Locale, RichExt, ShopCard, ShopTab, ShopView};

static ALOKACJE: AtomicUsize = AtomicUsize::new(0);

struct Liczacy;

// SAFETY: opakowanie delegujące do `System` bez zmiany zachowania; licznik jest
// atomowy i nie dotyka pamięci zarządzanej przez alokator.
unsafe impl GlobalAlloc for Liczacy {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALOKACJE.fetch_add(1, Ordering::Relaxed);
        // SAFETY: te same argumenty, które dostaliśmy; delegacja bez zmian.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: jak wyżej — `ptr` pochodzi z naszego `alloc`, czyli z `System`.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALOKACJE.fetch_add(1, Ordering::Relaxed);
        // SAFETY: jak wyżej.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOC: Liczacy = Liczacy;

fn migawka() -> magnat_economy::ShopPanelSnapshot {
    magnat_economy::ShopPanelSnapshot {
        site: magnat_core::SiteId(magnat_core::Entity::new(1, std::num::NonZeroU32::MIN)),
        firm: magnat_core::FirmId(magnat_core::Entity::new(1, std::num::NonZeroU32::MIN)),
        kind: magnat_core::PlaceKind::Grocery,
        at: magnat_core::Tick(0),
        tracking: magnat_economy::LostSaleTracking::Histogram,
        shelves: Vec::new(),
        customers: magnat_economy::CustomerStats::default(),
        lost_sales: magnat_economy::LostSalesView::default(),
        competition: Vec::new(),
        finance: magnat_economy::FinanceSummary {
            statement: magnat_economy::IncomeStatement::default(),
            balance: magnat_economy::BalanceSheet::default(),
            cash: magnat_economy::CashFlow::default(),
            inventory_value: magnat_core::Money::ZERO,
            loan: None,
        },
        reprices: Vec::new(),
        good_keys: Vec::new(),
    }
}

#[test]
fn klatka_bez_zmian_danych_nie_alokuje_ani_razu() {
    let c = Catalog::load().expect("data/locale/");
    let s = migawka();
    let mut wersje = magnat_ui::Versions::new();
    static ZRODLA: [DataSource; 2] = [DataSource::Market, DataSource::Locale];
    let mut model: Cached<ShopCard> = Cached::new(&ZRODLA);

    let buduj = || {
        ShopCard::build(
            &c,
            Locale::Pl,
            &ShopView {
                snapshot: &s,
                kind: s.kind,
                period_from: magnat_core::Tick(0),
            },
        )
    };

    // Pierwsza klatka buduje model — i alokuje, bo składa napisy. To jest w porządku.
    let pierwszy = model
        .get(&wersje, buduj)
        .render_tab(&c, Locale::Pl, ShopTab::Shelves);
    assert!(!pierwszy.to_plain().is_empty());
    assert_eq!(model.rebuilds(), 1);

    // Sto klatek bez zmiany danych: ani jednej alokacji i ani jednej przebudowy.
    let przed = ALOKACJE.load(Ordering::Relaxed);
    for _ in 0..100 {
        let _ = model.get(&wersje, || panic!("model przebudowany bez zmiany danych"));
    }
    let po = ALOKACJE.load(Ordering::Relaxed);
    assert_eq!(
        po - przed,
        0,
        "sto klatek bez zmian danych zaalokowało {} razy",
        po - przed
    );
    assert_eq!(model.rebuilds(), 1);

    // Ruch w rynku budzi model — bo ma budzić.
    wersje.bump(DataSource::Market);
    let _ = model.get(&wersje, buduj);
    assert_eq!(model.rebuilds(), 2);

    // Samo pytanie „czy brudny" też nie ma prawa alokować: woła się je raz na klatkę
    // dla każdego otwartego panelu.
    let przed = ALOKACJE.load(Ordering::Relaxed);
    for _ in 0..1000 {
        assert!(!model.is_dirty(&wersje));
    }
    assert_eq!(ALOKACJE.load(Ordering::Relaxed) - przed, 0);
}
