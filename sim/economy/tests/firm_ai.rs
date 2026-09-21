//! AI firm: asymetria informacji, tier operacyjny, taktyczny i reakcja na rywala
//! (M7e WP10–WP12, WP14; M7 §7.3).
//!
//! Świat jest tu zbudowany wprost — trzy sklepy o znanych pozycjach i znanym zapasie —
//! bo test sprawdza **decyzje firmy**, a nie generator miasta. Każdy wynik da się
//! policzyć na kartce i test, który pęka, mówi, co pękło.

mod common;

use std::collections::BTreeMap;

use common::{bench, bench_w_dzielnicy, ent, good_by_key, Bench, WHOLESALE_BASE};
use magnat_core::{
    DecisionReason, DistrictId, FirmStrategy, GoodId, Money, Qty, ReactionKind, SimMinute, SiteId,
    Tick, Q,
};
use magnat_economy::ai_run::AiInputs;
use magnat_economy::{Market, PricePolicy, PublicMarketBoard};
use magnat_firms::{
    Firm, FirmKey, FirmPersonality, Firms, Owner, Ring, Site, SitePnlMonth, SiteTypeId,
    StrategicOutlooks, Tier,
};
use magnat_policy::PolicyCatalog;
use magnat_spatial::Vec2;

const DOBA: u64 = 1_440;

fn mydlo() -> GoodId {
    let d = magnat_economy::EconomyData::load_default().expect("data/economy/");
    good_by_key(&d, "cons_soap")
}

fn katalog() -> PolicyCatalog {
    PolicyCatalog::load_default().expect("data/policies/presets.ron")
}

fn zaklad(id: SiteId, firm: FirmKey, i: usize) -> Site {
    Site {
        id,
        firm,
        site_type: SiteTypeId(0),
        building: magnat_core::BuildingId(ent(500 + i as u32)),
        district: DistrictId(i as u16),
        floor_m2: 200,
        positions: Vec::new(),
        mgmt: magnat_firms::ManagementQuality::NEUTRAL,
        tech: Q::new(50),
        fixed_cost_month: Money(100_000),
        hr_accrued: Money::ZERO,
        rnd_accrued: Money::ZERO,
        pnl: Ring::<SitePnlMonth, 36>::new(),
        opened: SimMinute(0),
        strike_bps: 0,
        strike_bp_days: 0,
        delegation: None,
        shift_profile: magnat_agents::ShiftProfile::Office,
    }
}

/// Rejestr firm: po jednej na sklep, każda z własną osobowością.
///
/// `gracz` mówi, który zakład należy do gracza — i to jest **cała** różnica między
/// nim a resztą. Kod decyzji jest jeden (`K-11`), a test asymetrii pyta, czy AI widzi
/// to, czego widzieć nie powinno.
fn firmy(b: &Bench, cechy: &[FirmPersonality], gracz: usize) -> Firms {
    let mut f = Firms::new();
    for (i, site) in b.sites.iter().enumerate() {
        let key = f.insert(|k| {
            Firm::sole_owner(
                k,
                format!("Sklep {i}"),
                SimMinute(0),
                DistrictId(i as u16),
                if i == gracz {
                    Owner::Player
                } else {
                    Owner::External
                },
            )
        });
        if let Some(f2) = f.get_mut(key) {
            f2.set_personality(cechy[i.min(cechy.len() - 1)]);
        }
        assert!(f.add_site(zaklad(*site, key, i)));
    }
    f
}

/// Wszystkie firmy poza pierwszą (gracz) w slocie operacyjnym.
fn due_ops(f: &Firms) -> Vec<(Tier, Vec<FirmKey>)> {
    let klucze: Vec<FirmKey> = f.iter().map(|(k, _)| k).skip(1).collect();
    vec![
        (Tier::Operational, klucze),
        (Tier::Tactical, Vec::new()),
        (Tier::Strategic, Vec::new()),
    ]
}

fn salda(b: &Bench) -> BTreeMap<SiteId, Money> {
    b.market
        .shop_accounts()
        .into_iter()
        .filter_map(|(s, a)| b.books.balance(a).map(|m| (s, m)))
        .collect()
}

/// Doba miasta w kolejności z `MarketSystem`: tablica → obserwacja → AI → przecena.
fn doba(
    m: &Market,
    f: &mut Firms,
    board: &mut PublicMarketBoard,
    cash: &BTreeMap<SiteId, Money>,
    due: &[(Tier, Vec<FirmKey>)],
    t: Tick,
) -> magnat_economy::FirmAiDay {
    m.refresh_board(board, t);
    m.observe_competitors(t);
    let d = m.run_firm_ai(
        f,
        &AiInputs {
            board,
            catalog: &katalog(),
            outlooks: &StrategicOutlooks::new(),
            cash,
        },
        due,
        t,
    );
    m.reprice_all(t);
    d
}

/// Odcisk decyzji wszystkich firm AI — to on jest przedmiotem testu asymetrii.
fn odcisk(f: &Firms) -> u64 {
    let mut h: u64 = 1469598103934665603;
    for (k, firma) in f.iter() {
        for w in firma.log.iter() {
            for b in format!("{k:?}{:?}{:?}", w.tick, w.reason).bytes() {
                h ^= u64::from(b);
                h = h.wrapping_mul(1099511628211);
            }
        }
    }
    h
}

/// Przebieg: sklep gracza i sklep firmy AI obok siebie, w jednej dzielnicy.
///
/// Gracz ma zadany **koszt własny** (ukryty), **gotówkę** (ukryta) i **cenę półkową**
/// (jawna). Konkurent ma koszt dwukrotnie wyższy, więc to cena gracza rozstrzyga,
/// kto w tej dzielnicy jest najtańszy — i dzięki temu zmiana danej jawnej ma czym
/// się objawić, a zmiana ukrytej nie ma czym.
///
/// Zwraca odcisk decyzji **firmy AI**.
fn przebieg(koszt_gracza: i64, gotowka_razy: i64, cena_gracza: i64) -> u64 {
    let g = mydlo();
    let mut b = bench_w_dzielnicy(77, &[Vec2::new(300.0, 0.0), Vec2::new(340.0, 0.0)], Some(0));
    for (i, s) in b.sites.iter().enumerate() {
        let koszt = if i == 0 {
            koszt_gracza
        } else {
            WHOLESALE_BASE * 2
        };
        b.market
            .deliver_now(*s, g, Qty(2_000_000), Money(2_000 * koszt), None, Tick(0));
    }
    if gotowka_razy > 1 {
        let konto = b.market.shop_account(b.sites[0]).expect("konto gracza");
        b.books
            .transfer(
                b.rest,
                konto,
                Money(50_000_000 * (gotowka_razy - 1)),
                magnat_economy::TxMemo::new(
                    magnat_economy::TxKind::Endowment,
                    DecisionReason::Unspecified,
                ),
                Tick(0),
            )
            .expect("dotacja");
    }
    b.market.restock_shelves();
    b.market.rebuild_index(&magnat_jobs::JobPool::new(1));
    // Cena gracza jest **zawsze** przypięta na sztywno. Bez tego obniżka kosztu
    // własnego przeciekłaby na półkę przez sterownik marży i test asymetrii
    // mierzyłby dane jawne, udając, że mierzy ukryte.
    assert!(b.market.set_policy(
        b.sites[0],
        g,
        PricePolicy::Fixed {
            price: Money(cena_gracza)
        },
        false
    ));

    let mut f = firmy(&b, &[FirmPersonality::NEUTRAL], 0);
    let due = due_ops(&f);
    let cash = salda(&b);
    let mut board = PublicMarketBoard::new();
    for d in 0..14u64 {
        doba(&b.market, &mut f, &mut board, &cash, &due, Tick(d * DOBA));
    }
    // Kontrola założenia: ogranicznik marży nie przyciął ceny gracza. Gdyby przyciął,
    // dana „jawna" zależałaby od ukrytego kosztu i test mierzyłby co innego,
    // niż deklaruje.
    assert_eq!(
        b.market.shelf_price(b.sites[0], g),
        Some(Money(cena_gracza)),
        "ogranicznik marży ruszył cenę gracza — test przestał mierzyć to, co deklaruje"
    );
    odcisk(&f)
}

#[test]
fn firma_ai_nie_widzi_ukrytych_danych_gracza() {
    // §7.3: mutujemy **wyłącznie** dane ukryte gracza — koszt własny o 30 % niżej
    // i gotówkę dziesięciokrotnie. Dane jawne (cena półkowa, lokalizacja, asortyment)
    // zostają identyczne. Decyzje AI muszą być co do bajta te same.
    let baza = przebieg(WHOLESALE_BASE, 1, 300);
    let zmutowane = przebieg(WHOLESALE_BASE * 70 / 100, 10, 300);
    assert_eq!(
        baza, zmutowane,
        "decyzje AI zmieniły się po mutacji danych, których firma widzieć nie może"
    );
}

#[test]
fn ale_widzi_cene_polkowa_gracza() {
    // Wariant negatywny — test testu. Gdyby przechodził, poprzedni byłby ślepy:
    // przechodziłby dlatego, że AI nie patrzy na nic, a nie dlatego, że patrzy
    // wyłącznie na rzeczy jawne.
    let baza = przebieg(WHOLESALE_BASE, 1, 450);
    let taniej = przebieg(WHOLESALE_BASE, 1, 300);
    assert_ne!(
        baza, taniej,
        "obniżka ceny gracza nie zmieniła ani jednej decyzji konkurenta"
    );
}

#[test]
fn moduly_ai_nie_siegaja_po_swiat() {
    // Druga połowa kryterium WP10: lint na graf zależności. Rusztowanie jest tekstowe,
    // tak samo jak w `single_entry_point.rs` — i tak samo jak tam sprawdza rzecz,
    // której typ sam nie wyrazi.
    let korzen = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("katalog workspace");
    let manifest =
        std::fs::read_to_string(korzen.join("sim/firms/Cargo.toml")).expect("Cargo.toml firm");
    assert!(
        !manifest.contains("magnat-economy"),
        "sim/firms zależy od sim/economy — asymetria informacji przestała być gwarancją grafu"
    );

    let ai = korzen.join("sim/firms/src/ai");
    let mut plikow = 0;
    for wpis in std::fs::read_dir(&ai).expect("sim/firms/src/ai") {
        let sciezka = wpis.expect("wpis").path();
        if sciezka.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let tresc = std::fs::read_to_string(&sciezka).expect("plik modułu ai");
        let kod: String = tresc
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for zakazane in ["magnat_ecs", "World", "magnat_economy", "magnat_supply"] {
            assert!(
                !kod.contains(zakazane),
                "{}: `{zakazane}` w module ai — decyzja firmy sięga po świat",
                sciezka.display()
            );
        }
        plikow += 1;
    }
    assert!(
        plikow >= 3,
        "moduł ai zniknął — test przestał czegokolwiek pilnować"
    );
}

#[test]
fn dwa_sklepy_o_roznej_osobowosci_celuja_w_inna_marze() {
    // §12.1 w jednym zdaniu: dwa zakłady o tym samym koszcie i tym samym otoczeniu
    // mają się różnić tym, co robią. Różnicę robi kurs, a kurs wychodzi z cech.
    let g = mydlo();
    let b = bench_w_dzielnicy(81, &[Vec2::new(300.0, 0.0), Vec2::new(340.0, 0.0)], Some(0));
    for s in &b.sites {
        b.market.deliver_now(
            *s,
            g,
            Qty(2_000_000),
            Money(2_000 * WHOLESALE_BASE),
            None,
            Tick(0),
        );
    }
    b.market.restock_shelves();
    b.market.rebuild_index(&magnat_jobs::JobPool::new(1));

    let mut tani = FirmPersonality::NEUTRAL;
    tani.price_focus = 95;
    tani.quality_focus = 10;
    let mut drogi = FirmPersonality::NEUTRAL;
    drogi.quality_focus = 95;
    drogi.price_focus = 10;
    assert_eq!(tani.strategy(), FirmStrategy::Discount);
    assert_eq!(drogi.strategy(), FirmStrategy::NicheQuality);

    let mut f = firmy(&b, &[tani, drogi], usize::MAX);
    let klucze: Vec<FirmKey> = f.iter().map(|(k, _)| k).collect();
    let due = vec![
        (Tier::Operational, klucze),
        (Tier::Tactical, Vec::new()),
        (Tier::Strategic, Vec::new()),
    ];
    let cash = salda(&b);
    let mut board = PublicMarketBoard::new();
    for d in 0..3u64 {
        doba(&b.market, &mut f, &mut board, &cash, &due, Tick(d * DOBA));
    }

    let marza = |site: SiteId| match b.market.price_policy(site, g) {
        Some(PricePolicy::Dynamic {
            target_margin_bp, ..
        }) => target_margin_bp,
        inne => panic!("tier operacyjny nie ustawił celu: {inne:?}"),
    };
    assert!(
        marza(b.sites[0]) < marza(b.sites[1]),
        "dyskont {} nie stoi niżej od niszy {}",
        marza(b.sites[0]),
        marza(b.sites[1])
    );
}

#[test]
fn trwale_nierentowny_zaklad_zostaje_zamkniety_i_znika_z_rynku() {
    // Kryterium WP12 w całości: decyzja, powód z liczbami i **skutek** — półka schodzi,
    // zakład wychodzi z listy firmy, sklep przestaje sprzedawać.
    let g = mydlo();
    let b = bench(91, &[Vec2::new(300.0, 0.0)]);
    b.market.deliver_now(
        b.sites[0],
        g,
        Qty(1_000_000),
        Money(1_000 * WHOLESALE_BASE),
        None,
        Tick(0),
    );
    b.market.restock_shelves();
    b.market.rebuild_index(&magnat_jobs::JobPool::new(1));

    let mut f = firmy(&b, &[FirmPersonality::NEUTRAL], usize::MAX);
    let klucz = f.first().expect("firma");
    // Trzy miesiące zamknięte stratą — tyle, ile znosi najcierpliwszy dyrektor.
    for m in 0..3u32 {
        assert!(f.post_revenue(b.sites[0], m, Money(1_000_000), Money(1_500_000)));
    }
    assert_eq!(
        f.site(b.sites[0])
            .and_then(|s| s.pnl.last())
            .and_then(SitePnlMonth::margin_bp),
        Some(-6_000),
        "utarg 10 000 zł, koszt własny 15 000, koszt stały 1 000 — strata 60 % utargu"
    );

    let due = vec![
        (Tier::Operational, Vec::new()),
        (Tier::Tactical, vec![klucz]),
        (Tier::Strategic, Vec::new()),
    ];
    let cash = salda(&b);
    let mut board = PublicMarketBoard::new();
    let d = doba(&b.market, &mut f, &mut board, &cash, &due, Tick(DOBA));

    assert_eq!(d.to_close, vec![b.sites[0]], "zakład nie został zamknięty");
    let powod = f
        .get(klucz)
        .and_then(|x| x.log.last().copied())
        .map(|w| w.reason)
        .expect("powód w dzienniku");
    assert!(
        matches!(powod, DecisionReason::SiteClosed { months, margin_bp } if months == 3 && margin_bp < 0),
        "powód bez liczb: {powod:?}"
    );

    // Skutek. `close_shop` domyka wołający (w symulacji `MarketSystem`), więc test
    // woła go tak samo jak on.
    assert!(b.market.close_shop(b.sites[0]));
    assert!(b.market.is_shop_closed(b.sites[0]));
    assert!(
        b.market.reorder_target(b.sites[0], g).is_none(),
        "zamknięty zakład nadal zamawia"
    );
    f.close_site(b.sites[0]);
    assert!(f.site(b.sites[0]).is_none(), "zakład został w rejestrze");
}

#[test]
fn utrata_udzialu_wyzwala_odpowiedz_z_zapisanym_powodem() {
    // WP14: rywal wchodzi obok, zabiera klientów, firma odpowiada — i zapisuje,
    // czym i ile ją to kosztuje. Bez utraty klientów nie odpowiada wcale.
    let g = mydlo();
    let b = bench_w_dzielnicy(
        101,
        &[Vec2::new(300.0, 0.0), Vec2::new(320.0, 0.0)],
        Some(0),
    );
    for s in &b.sites {
        b.market.deliver_now(
            *s,
            g,
            Qty(2_000_000),
            Money(2_000 * WHOLESALE_BASE),
            None,
            Tick(0),
        );
    }
    b.market.restock_shelves();
    b.market.rebuild_index(&magnat_jobs::JobPool::new(1));

    // Zakład 0 to lokalny konkurent, zakład 1 to nowo otwarty sklep gracza.
    // Który z nich jest czyj, nie ma dla kodu decyzji żadnego znaczenia (`K-11`) —
    // ma znaczenie to, komu spadła sprzedaż.
    let mut agresywny = FirmPersonality::NEUTRAL;
    agresywny.aggression = 95;
    let mut f = firmy(&b, &[agresywny, FirmPersonality::NEUTRAL], 1);
    let klucz = f.first().expect("firma");

    // Sprzedaż lokalnego konkurenta spada o 40 % wobec poprzedniego tygodnia.
    // Obie liczby są jawne — to jego własna sprzedaż.
    assert!(b.market.set_weekly_sales(b.sites[0], g, 60, 100));
    assert!(b.market.set_weekly_sales(b.sites[1], g, 100, 100));

    // Tablica publiczna: przez dwie doby w dzielnicy był jeden sklep, potem doszedł
    // drugi i to on jest najtańszy. Obie obserwacje są jawne — cena z ulicy.
    let mut board = PublicMarketBoard::new();
    for d in 0..=7u32 {
        let (ofert, najtanszy) = if d < 2 {
            (1, b.sites[0])
        } else {
            (2, b.sites[1])
        };
        board.record(
            DistrictId(0),
            g,
            d,
            magnat_economy::ObservedPrice {
                cheapest_net: Money(180),
                median_net: Money(220),
                cheapest_site: Some(najtanszy),
                offers: ofert,
            },
        );
    }

    let due = vec![
        (Tier::Operational, Vec::new()),
        (Tier::Tactical, Vec::new()),
        (Tier::Strategic, vec![klucz]),
    ];
    let cash = salda(&b);
    let d = b.market.run_firm_ai(
        &mut f,
        &AiInputs {
            board: &board,
            catalog: &katalog(),
            outlooks: &StrategicOutlooks::new(),
            cash: &cash,
        },
        &due,
        Tick(8 * DOBA),
    );

    assert_eq!(d.campaigns_started, 1, "firma nie odpowiedziała na rywala");
    let kampania = f.get(klucz).and_then(|x| x.campaign).expect("kampania");
    assert_eq!(kampania.kind, ReactionKind::PriceWar);
    assert_eq!(
        kampania.target, b.sites[1],
        "firma odpowiedziała nie temu rywalowi"
    );
    assert!(
        kampania.depth_bp > 0,
        "odpowiedź bez kosztu nie jest odpowiedzią"
    );
    let powod = f
        .get(klucz)
        .and_then(|x| x.log.last().copied())
        .map(|w| w.reason)
        .expect("powód w dzienniku");
    assert!(
        matches!(powod, DecisionReason::CompetitiveResponse { kind, .. } if kind == ReactionKind::PriceWar),
        "powód bez odpowiedzi: {powod:?}"
    );

    // Powtarzalność: ten sam stan daje tę samą decyzję, a firma prowadząca kampanię
    // nie zaczyna drugiej.
    let d2 = b.market.run_firm_ai(
        &mut f,
        &AiInputs {
            board: &board,
            catalog: &katalog(),
            outlooks: &StrategicOutlooks::new(),
            cash: &cash,
        },
        &due,
        Tick(9 * DOBA),
    );
    assert_eq!(d2.campaigns_started, 0, "firma zaczęła drugą kampanię");
}
