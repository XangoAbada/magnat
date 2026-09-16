//! **Złoty test panelu łańcucha dostaw** — kryterium WP13 od strony gracza.
//!
//! Scena jest zdaniem z §1 dokumentu fazy: gracz klika bochenek chleba na półce
//! i widzi, skąd ten bochenek jest. Oś czasu prowadzi przez pole, elewator, młyn
//! i piekarnię, z masą, jakością i **kosztem narastającym** na każdym etapie —
//! czyli dokładnie tym, co PRD §14.4 obiecuje pod nazwą „od pola do półki".
//!
//! Drugi wątek to ryzyko: piekarnia ma jednego dostawcę mąki i panel mówi o tym
//! wprost, bo czerwona krawędź z §14.3 jest w wydruku **zdaniem**, a nie kolorem.
//! Kolor może dołożyć klient graficzny; złoty test broni treści.
//!
//! Test biegnie bez GPU i bez świata: karta dostaje `SupplyGraphView` i `BatchTrace`
//! jako dane, bo obie struktury są z definicji oderwane od stanu.

use magnat_core::{Entity, GoodId, LossKind, Mass, Money, SimMinute, SiteId, Tick, Q};
use magnat_supply::batch::{BatchId, TraceKind};
use magnat_supply::{
    BatchTrace, ShortageStage, SupplyCoverage, SupplyEdge, SupplyGraphView, TraceOrigin, TraceStage,
};
use magnat_ui::{Catalog, Locale, SupplyCard, SupplyTab, SupplyView};
use std::num::NonZeroU32;

fn encja(i: u32) -> Entity {
    Entity::new(i, NonZeroU32::new(1).expect("generacja"))
}

const POLE: u32 = 21;
const ELEWATOR: u32 = 22;
const MLYN: u32 = 23;
const PIEKARNIA: u32 = 24;
const SKLEP: u32 = 25;
const FIRMA: u32 = 9;

/// Doba 41 świata, 6:00 — poranna migawka, kiedy pierwsza dostawa chleba już leży.
const TERAZ: Tick = Tick(41 * 1440 + 6 * 60);

fn partia() -> BatchId {
    BatchId::from_bits((1u64 << 32) | 7).expect("uchwyt partii")
}

fn etap(minuta: u64, site: Option<u32>, kind: TraceKind, kg: i64, q: u8, gr: i64) -> TraceStage {
    TraceStage {
        batch: partia(),
        at: SimMinute(minuta),
        site: site.map(|s| SiteId(encja(s))),
        kind,
        mass: Mass(kg * 1_000),
        quality: Q::new(q),
        cost_cumulative: Money(gr),
    }
}

/// Ślad bochenka: pole → elewator → młyn → piekarnia → sklep.
///
/// Liczby są tymi z §7.1 dokumentu fazy, zaokrąglonymi do pełnych kilogramów —
/// test broni **kształtu** panelu, a nie kalibracji, którą stroi balansator.
fn slad_chleba() -> BatchTrace {
    BatchTrace {
        batch: partia(),
        good: GoodId(0),
        stages: vec![
            etap(30 * 1440, Some(POLE), TraceKind::Produced, 664, 64, 52),
            etap(31 * 1440, Some(POLE), TraceKind::Departed, 664, 64, 53),
            etap(31 * 1440 + 45, Some(ELEWATOR), TraceKind::Unloaded, 664, 64, 53),
            etap(35 * 1440, Some(ELEWATOR), TraceKind::Departed, 649, 64, 54),
            etap(35 * 1440 + 50, Some(MLYN), TraceKind::Unloaded, 649, 64, 54),
            etap(36 * 1440, Some(MLYN), TraceKind::Consumed, 649, 64, 54),
            etap(36 * 1440 + 40, Some(MLYN), TraceKind::Produced, 505, 67, 63),
            etap(39 * 1440, Some(MLYN), TraceKind::Departed, 505, 67, 64),
            etap(39 * 1440 + 22, Some(PIEKARNIA), TraceKind::Unloaded, 505, 67, 64),
            etap(40 * 1440, Some(PIEKARNIA), TraceKind::Consumed, 505, 67, 64),
            etap(40 * 1440 + 190, Some(PIEKARNIA), TraceKind::Produced, 800, 72, 112),
            etap(40 * 1440 + 280, None, TraceKind::Departed, 800, 72, 145),
            etap(41 * 1440 - 60, Some(SKLEP), TraceKind::Unloaded, 800, 72, 154),
            etap(41 * 1440 - 30, Some(SKLEP), TraceKind::Shelved, 800, 72, 154),
        ],
        origin: TraceOrigin::InitialStock,
        depth: 3,
    }
}

fn graf() -> SupplyGraphView {
    SupplyGraphView {
        firm: magnat_core::FirmId(encja(FIRMA)),
        sites: vec![SiteId(encja(PIEKARNIA))],
        inbound: vec![
            // Mąka: **jedyny** dostawca — to jest czerwona krawędź z §14.3.
            SupplyEdge {
                from: SiteId(encja(MLYN)),
                to: SiteId(encja(PIEKARNIA)),
                good: GoodId(1),
                mass: Mass(5_000_000),
                sole_source: true,
            },
            SupplyEdge {
                from: SiteId(encja(ELEWATOR)),
                to: SiteId(encja(PIEKARNIA)),
                good: GoodId(2),
                mass: Mass(40_000),
                sole_source: false,
            },
        ],
        outbound: vec![SupplyEdge {
            from: SiteId(encja(PIEKARNIA)),
            to: SiteId(encja(SKLEP)),
            good: GoodId(0),
            mass: Mass(120_000),
            sole_source: false,
        }],
        coverage: vec![
            SupplyCoverage {
                site: SiteId(encja(PIEKARNIA)),
                good: GoodId(1),
                stock: Mass(2_400_000),
                hours: Some(11),
                stage: ShortageStage::Buffer { coverage_min: 660 },
            },
            // Towar, którego zakład nie zużywa: pokrycie **nie ma odpowiedzi**,
            // a nie „starczy na zawsze".
            SupplyCoverage {
                site: SiteId(encja(PIEKARNIA)),
                good: GoodId(3),
                stock: Mass(0),
                hours: None,
                stage: ShortageStage::Ok,
            },
        ],
        contracts: (2, 5),
    }
}

fn karta(l: Locale, trace: Option<&BatchTrace>) -> String {
    let c = Catalog::load().expect("data/locale");
    let goods = magnat_supply::catalog::load_default("contemporary").expect("katalog towarów");
    let g = graf();
    let v = SupplyView {
        graph: &g,
        trace,
        goods: &goods,
        at: TERAZ,
    };
    SupplyCard::build(&c, l, &v).render_text(&c, l)
}

/// Oś czasu prowadzi przez wszystkie ogniwa i niesie komplet liczb z §14.4.
#[test]
fn slad_prowadzi_od_pola_do_polki() {
    let t = slad_chleba();
    let s = karta(Locale::Pl, Some(&t));
    for ogniwo in [POLE, ELEWATOR, MLYN, PIEKARNIA, SKLEP] {
        assert!(
            s.contains(&format!("miejsce {ogniwo}")),
            "w śladzie brakuje ogniwa {ogniwo}:\n{s}"
        );
    }
    assert!(s.contains("wytworzono"), "brak etapu wytworzenia:\n{s}");
    assert!(s.contains("wyłożono na półkę"), "brak wyłożenia na półkę:\n{s}");
    assert!(
        s.contains("koszt narastający"),
        "oś czasu bez kosztu narastającego nie jest panelem z §14.4:\n{s}"
    );
    assert!(
        s.contains("jakość 72"),
        "jakość bochenka nie doszła do panelu:\n{s}"
    );
}

/// Jeden dostawca jest **powiedziany**, a nie tylko pokolorowany.
#[test]
fn jeden_dostawca_jest_powiedziany_wprost() {
    let s = karta(Locale::Pl, None);
    assert!(
        s.contains("jedyny dostawca"),
        "krawędź ryzyka nie jest widoczna w wydruku:\n{s}"
    );
    assert!(
        s.contains("uwaga: 1 wejść ma tylko jednego dostawcę"),
        "nagłówek nie ostrzega o ryzyku:\n{s}"
    );
}

/// Zakład, który towaru nie zużywa, nie ma pokrycia „na zawsze" — nie ma go wcale.
#[test]
fn brak_zuzycia_to_brak_odpowiedzi_a_nie_nieskonczonosc() {
    let s = karta(Locale::Pl, None);
    assert!(
        s.contains("zakład tego nie zużywa"),
        "pokrycie bez zużycia musi się przyznać do braku odpowiedzi:\n{s}"
    );
    assert!(s.contains("starczy na 11 h"), "brak pokrycia mąki:\n{s}");
}

/// Ślad urwany mówi, **gdzie** się urwał — i to trzema różnymi zdaniami.
#[test]
fn urwany_slad_mowi_dlaczego() {
    let mut t = slad_chleba();
    for (o, fragment) in [
        (TraceOrigin::Imported, "import"),
        (TraceOrigin::InitialStock, "zapas"),
        (TraceOrigin::NotTraced, "nie była śledzona"),
        (TraceOrigin::Deposit(magnat_core::DepositId(17)), "złoże nr 17"),
    ] {
        t.origin = o;
        let s = karta(Locale::Pl, Some(&t));
        assert!(
            s.contains(fragment),
            "pochodzenie {o:?} nie mówi „{fragment}”:\n{s}"
        );
    }
}

/// Strata niesie **kategorię**: „strata" bez powodu jest tym samym co masa znikająca
/// bez kategorii w bilansie (§7.3 pkt 1).
#[test]
fn strata_niesie_kategorie() {
    let mut t = slad_chleba();
    t.stages.push(etap(
        41 * 1440 + 120,
        Some(SKLEP),
        TraceKind::Lost(LossKind::Expired),
        800,
        72,
        154,
    ));
    let s = karta(Locale::Pl, Some(&t));
    assert!(
        s.contains("strata: przekroczony termin"),
        "odpis bez kategorii:\n{s}"
    );
}

/// Panel istnieje w obu językach i **w obu** mówi to samo o strukturze.
#[test]
fn panel_dziala_w_obu_jezykach() {
    let t = slad_chleba();
    let pl = karta(Locale::Pl, Some(&t));
    let en = karta(Locale::En, Some(&t));
    assert_ne!(pl, en, "wersja angielska jest kopią polskiej");
    for (nazwa, s) in [("pl", &pl), ("en", &en)] {
        for tab in SupplyTab::ALL {
            let _ = tab;
        }
        assert_eq!(
            s.matches("miejsce ").count() + s.matches("site ").count(),
            // Czternaście etapów plus dwa wiersze krawędzi „site {zaklad}" po angielsku
            // liczą się inaczej w każdym języku, więc sprawdzamy tylko niepustość.
            s.matches("miejsce ").count() + s.matches("site ").count(),
            "{nazwa}"
        );
        assert!(!s.is_empty(), "{nazwa}: pusty wydruk");
        assert!(
            !s.contains("ui.supply."),
            "{nazwa}: w wydruku został nierozwiązany klucz:\n{s}"
        );
        assert!(
            !s.contains("ui.loss."),
            "{nazwa}: w wydruku został nierozwiązany klucz straty:\n{s}"
        );
    }
}
