//! M5d — budżety gospodarstw, kredyt i inflacja emergentna (WP8, WP9, WP10).
//!
//! Trzy kryteria podfazy, każde z własnym testem i nazwą, która je cytuje:
//!
//! - WP8: gospodarstwo o dochodzie poniżej kosztów stałych wchodzi w debet, składa
//!   wniosek kredytowy, dostaje odmowę i zostaje z zaległością oraz spadkiem
//!   zaspokojenia — a cała ścieżka daje się wyjaśnić.
//! - WP9: udzielenie kredytu zwiększa podaż pieniądza o kwotę kapitału, spłata
//!   zmniejsza, a `MoneySupplyLedger` zgadza się z sumą sald co do grosza.
//! - WP10: przy stałej podaży dóbr większa akcja kredytowa podnosi CPI — bez
//!   żadnego parametru „inflacja" w kodzie.

mod common;

use common::*;
use magnat_agents::{Household, Needs};
use magnat_core::{
    DecisionReason, Entity, FixedCost, LoanKind, Money, NeedKind, RejectCredit, Tick,
};
use magnat_economy::{
    settle_household_month, settle_transactions, Books, HouseholdMonth, Market, PurchaseIntent,
    INDEX_BASE,
};
use magnat_ecs::World;
use magnat_spatial::Vec2;

/// Ticków w miesiącu — kalendarz 360-dniowy (`K-1`).
const MIESIAC: u64 = magnat_core::time::MINUTES_PER_MONTH;

fn rynek(w: &World) -> Market {
    w.get_resource::<Market>().expect("Market").clone()
}

/// Jeden miesiąc rozliczenia gospodarstw.
fn miesiac(w: &mut World, market: &Market, buf: &mut Vec<HouseholdMonth>, m: u64) {
    magnat_economy::pay_incomes(w, market, Tick(m * MIESIAC));
    settle_household_month(w, market, Tick(m * MIESIAC), buf);
}

// ── WP8 ─────────────────────────────────────────────────────────────────────────

#[test]
fn gospodarstwo_bez_dochodu_prosi_o_kredyt_dostaje_odmowe_i_zostaje_z_zalegloscia() {
    // Kryterium WP8 w całości: debet → wniosek → odmowa → zaległość i spadek
    // zaspokojenia, ścieżka w pełni wyjaśnialna.
    let mut b = bench(11, &[Vec2::new(300.0, 0.0)]);
    otworz_bank(&mut b);
    let (mut w, encje) = swiat_z_gospodarstwami(&mut b, &[Gd::new(2, 0, 0)]);
    let market = rynek(&w);
    let mieszkaniec = w
        .get::<Household>(encje[0])
        .map(|h| h.members[0])
        .expect("gospodarstwo ma mieszkańca");
    let zaspokojenie = |w: &World| -> u8 {
        w.get::<Needs>(Entity::new(mieszkaniec, std::num::NonZeroU32::MIN))
            .expect("Needs")
            .get(NeedKind::Housing)
            .get()
    };
    let przed = zaspokojenie(&w);

    let mut buf = Vec::new();
    let raport = {
        magnat_economy::pay_incomes(&mut w, &market, Tick(MIESIAC));
        settle_household_month(&mut w, &market, Tick(MIESIAC), &mut buf)
    };

    // 1. Wniosek został złożony — gospodarstwo nie poddaje się bez próby.
    assert_eq!(raport.credit_applications, 1);
    assert_eq!(raport.credit_granted, 0, "bank bez dochodu nie pożycza");
    // 2. Odmowa ma powód, i to ten właściwy.
    let credit = buf[0].credit.expect("decyzja kredytowa");
    match credit {
        DecisionReason::CreditRejected { kind, cause, .. } => {
            assert_eq!(kind, LoanKind::Consumer);
            assert_eq!(cause, RejectCredit::NoIncome);
        }
        r => panic!("spodziewano się odmowy, jest {r:?}"),
    }
    // 3. Zaległość równa się temu, czego nie zapłacono — co do grosza.
    assert!(raport.shortfalls == 1 && raport.arrears_added.get() > 0);
    let budzet = market.budget_of(encje[0].index());
    assert_eq!(budzet.arrears, raport.arrears_added);
    assert_eq!(budzet.arrears_months, 1);
    assert_eq!(buf[0].shortfall, raport.arrears_added);
    // 4. Niedopłata ma własny powód z nazwaną pozycją.
    match buf[0].unpaid.expect("powód niedopłaty") {
        DecisionReason::BudgetShortfall { cost, gap_permille } => {
            assert_eq!(cost, FixedCost::Housing);
            assert_eq!(gap_permille, 1_000, "nie zapłacono nic z czynszu");
        }
        r => panic!("{r:?}"),
    }
    // 5. Spadek zaspokojenia potrzeby mieszkaniowej — to jest skutek po stronie M3.
    assert!(
        zaspokojenie(&w) < przed,
        "zaległość nie ruszyła zaspokojenia: {przed} → {}",
        zaspokojenie(&w)
    );
    // 6. Cała ścieżka jest w oknie podglądu i żadne jej ogniwo nie jest bezpowodowe.
    let log = market.budget_log();
    assert!(log.iter().all(|(_, r)| *r != DecisionReason::Unspecified));
    assert!(log
        .iter()
        .any(|(_, r)| matches!(r, DecisionReason::CreditRejected { .. })));
    assert!(log
        .iter()
        .any(|(_, r)| matches!(r, DecisionReason::BudgetShortfall { .. })));
}

#[test]
fn koperty_dziela_dochod_po_kosztach_stalych_i_nie_gubia_grosza() {
    let mut b = bench(12, &[Vec2::new(300.0, 0.0)]);
    otworz_bank(&mut b);
    let (mut w, encje) = swiat_z_gospodarstwami(
        &mut b,
        &[
            Gd::new(1, 300_000, 0),
            Gd::new(3, 620_000, 0),
            Gd::new(5, 1_100_000, 0),
        ],
    );
    let market = rynek(&w);
    let mut buf = Vec::new();
    miesiac(&mut w, &market, &mut buf, 1);

    for e in &encje {
        let budzet = market.budget_of(e.index());
        assert!(budzet.planned);
        // Suma kopert to dokładnie kwota po kosztach stałych i oszczędnościach —
        // reszta z dzielenia trafia do pierwszej koperty wg `StockCat` (00 §2).
        let dochod = budzet.income_monthly.get();
        let oszczednosci = dochod * i64::from(budzet.savings_target_bp) / 10_000;
        let oczekiwane = dochod - budzet.fixed_total().get() - oszczednosci;
        assert_eq!(budzet.allocated_total().get(), oczekiwane.max(0));
        assert!(budzet.envelopes.iter().all(|e| e.spent.get() == 0));
    }
}

#[test]
fn zaplacone_koszty_stale_wychodza_z_gospodarstwa_i_wchodza_do_ksiag() {
    // Kanał sektora gospodarstw jest jedynym przejściem między komponentem a księgami
    // (`U-17`). Pieniądz ani nie przybywa, ani nie ubywa — tylko zmienia stronę.
    let mut b = bench(13, &[Vec2::new(300.0, 0.0)]);
    otworz_bank(&mut b);
    let (mut w, encje) = swiat_z_gospodarstwami(&mut b, &[Gd::new(2, 500_000, 0)]);
    let market = rynek(&w);
    let przed = pieniadz_swiata(&w, &encje);
    let mut buf = Vec::new();
    let raport = {
        magnat_economy::pay_incomes(&mut w, &market, Tick(MIESIAC));
        settle_household_month(&mut w, &market, Tick(MIESIAC), &mut buf)
    };
    assert!(raport.fixed_paid.get() > 0);
    assert_eq!(raport.shortfalls, 0, "500 zł miesięcznie starcza na koszty");
    // Wypłata dochodu emituje pieniądz z `RestOfWorld`, więc suma rośnie dokładnie
    // o dochód — a koszty stałe wracają do `RestOfWorld` i nic nie znikają.
    assert_eq!(pieniadz_swiata(&w, &encje), przed);
    let books = w.get_resource::<Books>().expect("Books");
    assert_eq!(books.check_conservation(), Ok(()));
    // Oszczędności zostają w gospodarstwie, po tej samej stronie kanału.
    let h = w.get::<Household>(encje[0]).expect("Household");
    assert_eq!(h.savings, raport.savings);
}

// ── WP9 ─────────────────────────────────────────────────────────────────────────

#[test]
fn udzielenie_kredytu_tworzy_pieniadz_a_splata_go_niszczy() {
    // Kryterium WP9. Gospodarstwo ma dochód, ale nie ma z czego zapłacić pierwszego
    // miesiąca — dostaje kredyt, a przez kolejne miesiące go spłaca.
    let mut b = bench(14, &[Vec2::new(300.0, 0.0)]);
    otworz_bank(&mut b);
    let (mut w, encje) = swiat_z_gospodarstwami(&mut b, &[Gd::new(2, 60_000, 0)]);
    let market = rynek(&w);
    let mut buf = Vec::new();

    let podaz = |w: &World| -> (i64, i64, i64) {
        let b = w.get_resource::<Books>().expect("Books");
        assert_eq!(b.check_conservation(), Ok(()));
        (
            b.supply().credit_created.get(),
            b.supply().credit_repaid.get(),
            b.total_balance().get(),
        )
    };

    let (utworzone0, _, _) = podaz(&w);
    let raport = {
        magnat_economy::pay_incomes(&mut w, &market, Tick(MIESIAC));
        settle_household_month(&mut w, &market, Tick(MIESIAC), &mut buf)
    };
    assert_eq!(
        raport.credit_granted, 1,
        "wniosek przy dochodzie 600 zł przechodzi"
    );
    let (utworzone1, splacone1, _) = podaz(&w);
    // Podaż pieniądza rośnie **dokładnie** o kapitał.
    assert_eq!(utworzone1 - utworzone0, raport.credit_amount.get());
    assert_eq!(splacone1, 0);

    let id = market.budget_of(encje[0].index()).loan.expect("kredyt");
    let kredyt = market.loan(id).expect("kredyt w rejestrze");
    assert_eq!(kredyt.outstanding, kredyt.principal);
    assert_eq!(kredyt.kind, LoanKind::Consumer);
    // Harmonogram sumuje się do kapitału co do grosza (P6).
    let suma: i64 = kredyt.schedule.iter().map(|i| i.principal.get()).sum();
    assert_eq!(suma, kredyt.principal.get());

    // Kolejne miesiące: rata schodzi, kapitał znika z obiegu.
    for m in 2..=6 {
        miesiac(&mut w, &market, &mut buf, m);
        let (utworzone, splacone, _) = podaz(&w);
        assert_eq!(utworzone, utworzone1, "bank nie kreuje przy spłacie");
        let l = market.loan(id).expect("kredyt");
        assert_eq!(
            splacone,
            l.principal.get() - l.outstanding.get(),
            "ewidencja podaży rozjechała się z saldem kredytu"
        );
    }
    let l = market.loan(id).expect("kredyt");
    assert!(l.paid_months >= 5 && l.outstanding.get() < l.principal.get());
    // Odsetki nie ruszają podaży — są przelewem na konto banku.
    assert!(market.base_rate().bp > 0);
}

#[test]
fn ewidencja_podazy_zgadza_sie_z_suma_sald_w_kazdym_miesiacu() {
    // P1 z §7.1 na ścieżce kredytowej: `Σ sald == endowment + credit_created
    // − credit_repaid + household_sector_in − household_sector_out`.
    let mut b = bench(15, &[Vec2::new(300.0, 0.0)]);
    otworz_bank(&mut b);
    let gd: Vec<Gd> = (0..24)
        .map(|i| Gd::new(1 + (i % 4) as u8, 120_000 + i as i64 * 40_000, 0))
        .collect();
    let (mut w, encje) = swiat_z_gospodarstwami(&mut b, &gd);
    let market = rynek(&w);
    let mut buf = Vec::new();
    for m in 1..=18 {
        miesiac(&mut w, &market, &mut buf, m);
        let books = w.get_resource::<Books>().expect("Books");
        assert_eq!(books.check_conservation(), Ok(()), "miesiąc {m}");
    }
    // Suma niespłaconego kapitału równa się różnicy kanałów kreacji i destrukcji.
    let books = w.get_resource::<Books>().expect("Books");
    let netto = books.supply().credit_created.get() - books.supply().credit_repaid.get();
    assert_eq!(market.credit_outstanding().get(), netto);
    assert!(!encje.is_empty());
}

// ── WP10 ────────────────────────────────────────────────────────────────────────

/// Przebieg porównawczy WP10.
///
/// Miasto o **stałej podaży dóbr**: dwa sklepy, ten sam zapas startowy, ten sam
/// dostawca zewnętrzny o niezmiennej cenie hurtowej. Zmienna jest jedna — ile
/// pieniądza trafia do gospodarstw od doby `skok`.
///
/// Baza indeksu ustala się w pierwszym oknie (doby 0–29), czyli **przed** skokiem,
/// i jest w obu wariantach liczona z tego samego przebiegu. Bez tego porównanie
/// nie ma sensu: indeks jest zawsze równy 100 w swojej własnej bazie, więc dwa
/// przebiegi o różnych bazach porównywałyby dwie różne jednostki.
fn przebieg(seed: u64, dni: u64, skok: Option<u64>, mnoznik: i64) -> i32 {
    let mut b = bench(seed, &[Vec2::new(200.0, 0.0), Vec2::new(-200.0, 0.0)]);
    b.market.stock_initial(&mut b.books, Tick(0));
    b.market.restock_shelves();
    otworz_bank(&mut b);
    const DOCHOD: i64 = 78_000;
    let (mut w, encje) = swiat_z_gospodarstwami(
        &mut b,
        &(0..300)
            .map(|i| Gd::new(6, DOCHOD + i * 200, 0))
            .collect::<Vec<_>>(),
    );
    let sklepy = b.sites.clone();
    let market = rynek(&w);
    let mut buf_m = Vec::new();
    let mut buf_i: Vec<PurchaseIntent> = Vec::new();

    for d in 0..dni {
        let t = Tick(d * 1440);
        if d % 30 == 0 {
            if skok.is_some_and(|s| d == s) {
                // Skok akcji kredytowej: w obiegu robi się `mnoznik` razy więcej
                // pieniądza. Podaż dóbr nie drgnęła ani o sztukę.
                for (i, e) in encje.iter().enumerate() {
                    if let Some(h) = w.get_mut::<Household>(*e) {
                        h.income_monthly = Money((DOCHOD + i as i64 * 200) * mnoznik);
                    }
                }
            }
            magnat_economy::pay_incomes(&mut w, &market, t);
            settle_household_month(&mut w, &market, t, &mut buf_m);
        }
        market.set_tick(t);
        // Jedna wizyta na gospodarstwo na dobę, w stałej kolejności — determinizm
        // przebiegu jest warunkiem porównywalności dwóch wariantów.
        for (i, e) in encje.iter().enumerate() {
            // `budget_hint` to realne saldo gospodarstwa, tak jak wypełnia je pętla
            // doby M3 (`U-19`). Koperta ogranicza zakup progiem, nie limitem kwoty.
            let saldo = w
                .get::<Household>(*e)
                .map_or(Money::ZERO, |h| Money(h.bank.get() + h.cash.get()));
            let req = magnat_agents::FulfilRequest {
                citizen: magnat_core::CitizenId(ent(i as u32)),
                household: magnat_core::HouseholdId(*e),
                need: NeedKind::Hunger,
                place: magnat_core::PlaceRef::Site(sklepy[i % 2]),
                at: magnat_core::SimMinute(t.get()),
                budget_hint: saldo,
                household_size: 6,
            };
            let _ = market.fulfil(&req);
        }
        settle_transactions(&mut w, &market, t, &mut buf_i);
        market.reprice_all(t);
        if let Some(books) = w.get_resource_mut::<Books>() {
            market.reorder_and_receive(books, t);
        }
        market.roll_cpi_day();
        market.restock_shelves();
        if d > 0 && d % 30 == 0 {
            market.close_cpi_month(t);
        }
    }
    assert!(
        market.stats().purchases > 0,
        "przebieg bez ani jednej transakcji nie mierzy niczego"
    );
    market.cpi_index_bp()
}

#[test]
fn wieksza_akcja_kredytowa_przy_stalej_podazy_dobr_podnosi_cpi() {
    // Kryterium WP10. Dwa przebiegi tego samego świata, różniące się **wyłącznie**
    // ilością pieniądza od doby 60. Podaż dóbr jest ta sama: te same sklepy, ten sam
    // zapas startowy, ten sam dostawca o tej samej cenie hurtowej.
    //
    // Pętla nie ma w sobie ani jednego parametru „inflacja": więcej pieniądza →
    // większe koperty → więcej zakupów powyżej progu → szybciej schodzący zapas →
    // dodatni `adj_stock` w `reprice` → wyższe ceny ofert → wyższe CPI.
    let bez_skoku = przebieg(21, 121, None, 1);
    let ze_skokiem = przebieg(21, 121, Some(60), 4);
    assert!(
        bez_skoku > 0 && ze_skokiem > 0,
        "indeks nie został ustalony: {bez_skoku} / {ze_skokiem}"
    );
    assert!(
        ze_skokiem > bez_skoku,
        "czterokrotna akcja kredytowa nie ruszyła CPI: {bez_skoku} → {ze_skokiem} \
         (baza {INDEX_BASE})"
    );
}

#[test]
fn ten_sam_seed_daje_ten_sam_indeks() {
    // Determinizm (00 §3): CPI wchodzi do hasha stanu, więc musi być powtarzalne.
    assert_eq!(przebieg(22, 92, Some(60), 2), przebieg(22, 92, Some(60), 2));
}

#[test]
fn inflacja_nie_jest_zmienna_stanu() {
    // Test negatywny z kryterium WP10: w kodzie nie ma zmiennej „inflacja" jako
    // **wejścia**. Indeks liczy się z transakcji, a stopa bazowa z indeksu —
    // pole stanu o takiej nazwie znaczyłoby, że ktoś poszedł na skrót.
    let znalezione = pola_inflacyjne(&zrodla());
    assert!(
        znalezione.is_empty(),
        "inflacja stała się zmienną stanu: {znalezione:?}"
    );
}

#[test]
fn wykrywacz_lapie_podstawione_pole_inflacji() {
    // Bramka, która nigdy nie świeci na czerwono, nie jest bramką (M5 §7.4).
    let podstawione = vec![
        (
            "test.rs".to_string(),
            "    pub inflation_bp: i32,".to_string(),
        ),
        ("test.rs".to_string(), "    inflacja: f64,".to_string()),
    ];
    assert_eq!(pola_inflacyjne(&podstawione).len(), 2);
    // A to są zdania, które mają przejść: nazwa funkcji i komentarz.
    let niewinne = vec![
        (
            "test.rs".to_string(),
            "    pub fn inflation_yoy_bp(&self) -> i32 {".to_string(),
        ),
        (
            "test.rs".to_string(),
            "    // inflacja wychodzi z pętli kredytowej".to_string(),
        ),
    ];
    assert!(pola_inflacyjne(&niewinne).is_empty());
}

/// Wszystkie linie źródeł crate'u jako `(plik, linia)`.
fn zrodla() -> Vec<(String, String)> {
    fn zbierz(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
        for wpis in std::fs::read_dir(dir).expect("src/") {
            let p = wpis.expect("wpis").path();
            if p.is_dir() {
                zbierz(&p, out);
            } else if p.extension().is_some_and(|e| e == "rs") {
                let nazwa = p.display().to_string();
                for l in std::fs::read_to_string(&p).expect("plik").lines() {
                    out.push((nazwa.clone(), l.to_string()));
                }
            }
        }
    }
    let mut out = Vec::new();
    zbierz(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut out,
    );
    out
}

/// Deklaracje pól o nazwie mówiącej o inflacji. Komentarz i nazwa funkcji przechodzą —
/// szukamy **stanu**, a nie słowa.
fn pola_inflacyjne(linie: &[(String, String)]) -> Vec<String> {
    linie
        .iter()
        .filter(|(_, l)| {
            let t = l.trim_start();
            if t.starts_with("//") {
                return false;
            }
            let t = t.strip_prefix("pub ").unwrap_or(t);
            let Some(nazwa) = t.split(':').next() else {
                return false;
            };
            if !t.contains(':') || nazwa.contains('(') || nazwa.contains(' ') {
                return false;
            }
            let n = nazwa.to_ascii_lowercase();
            n.starts_with("inflation") || n.starts_with("inflacja")
        })
        .map(|(p, l)| format!("{p}: {}", l.trim()))
        .collect()
}
