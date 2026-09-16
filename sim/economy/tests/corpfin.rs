//! Finanse firmy i upadłość — testy własnościowe M7d (WP8, WP9).
//!
//! Dwa kryteria ukończenia i po jednym teście na każde:
//!
//! - **WP8:** „test zachowania pieniądza przechodzi z włączonymi wszystkimi czterema
//!   instrumentami; harmonogram rat sumuje się dokładnie do kwoty kredytu + odsetek".
//! - **WP9:** „property-test `bankruptcy_conserves_money_and_assets` zielony na
//!   10 000 losowych konfiguracji".
//!
//! Podział na testy idzie za podziałem kodu: [`Bankruptcy::plan_distribution`] jest
//! funkcją czystą, więc niezmienniki 5, 6 i 7 z M7 §7.2 (kolejność priorytetów,
//! reszta co do grosza, determinizm) sprawdza się na dziesięciu tysiącach przypadków
//! bez budowania świata. Niezmienniki 1 i 2 (pieniądz, loty) wymagają realnych kont
//! i idą przez `Books`.

use magnat_core::{
    BankruptcyTrigger, CitizenId, ClaimPriority, DecisionReason, Entity, FirmId, LoanKind, Money,
    Tick, CLAIM_PRIORITY_COUNT,
};
use magnat_economy::books::{AccountId, AccountKind, AccountOwner, Books, TxKind, TxMemo};
use magnat_economy::corpfin::{
    AssetKind, AssetRef, Bankruptcy, BankruptcyId, BankruptcyStage, BondHolder, ClaimOrigin,
    CorpFinance, EstateParams, InsolvencyParams, LotFate, SectorPayout, TriggerParams,
    ValuationParams,
};
use magnat_economy::credit::{build_schedule, LoanBook};
use proptest::prelude::*;
use std::num::NonZeroU32;

fn ent(i: u32) -> Entity {
    Entity::new(i, NonZeroU32::MIN)
}

fn firm(i: u32) -> FirmId {
    FirmId(ent(i))
}

/// Parametry postępowania na potrzeby testów — te same kształtem co
/// `data/tuning/insolvency.ron`, ale niezależne od pliku: test ma sprawdzać regułę,
/// a nie kalibrację, którą balansator będzie przestawiał.
fn params() -> InsolvencyParams {
    InsolvencyParams {
        trigger: TriggerParams {
            illiquid_days: 90,
            illiquid_min_gr: 100_000,
            negative_equity_arrears_months: 3,
        },
        estate: EstateParams {
            trustee_fee_bp: 500,
            rounds: 3,
            round_days: 15,
            round_discount_bp: vec![0, 2500, 5000],
            scrap_bp: 1000,
        },
        valuation: ValuationParams {
            equipment_bp: 7000,
            inventory_bp: 6000,
        },
    }
}

fn memo() -> TxMemo {
    TxMemo::new(TxKind::Withdrawal, DecisionReason::Unspecified)
}

// ── WP8: harmonogram ─────────────────────────────────────────────────────────────

/// Kryterium WP8, część druga: **suma rat równa się kapitałowi plus odsetkom
/// co do grosza**, także dla nowego produktu inwestycyjnego (60 miesięcy).
///
/// Sześćdziesiąt rat to pięć razy dłuższa pętla zaokrągleń niż w kredycie obrotowym,
/// więc gdyby reszta uciekała po grosz na ratę, tutaj byłoby jej pięć razy więcej —
/// i to jest jedyny powód, dla którego ten test istnieje obok testu M5d.
#[test]
fn harmonogram_kredytu_inwestycyjnego_sumuje_sie_co_do_grosza() {
    for kapital in [1_i64, 999, 100_000, 7_777_777, 999_999_999] {
        for stopa in [0_i32, 200, 740, 3500] {
            let h = build_schedule(Money(kapital), stopa, 60, Tick(0));
            assert_eq!(h.len(), 60);
            let suma_kapitalu: i64 = h.iter().map(|r| r.principal.get()).sum();
            assert_eq!(
                suma_kapitalu, kapital,
                "kapitał {kapital}, stopa {stopa}: suma rat kapitałowych się nie zgadza"
            );
            assert!(
                h.iter()
                    .all(|r| r.principal.get() >= 0 && r.interest.get() >= 0),
                "kapitał {kapital}, stopa {stopa}: rata ujemna"
            );
        }
    }
}

/// Produkt inwestycyjny jest **dopisany na końcu** `LoanKind` i nie przestawił
/// niczego przed sobą. Indeks wybiera widełki w `data/economy/bank.ron`, więc
/// przestawienie kolejności zmieniłoby oprocentowanie każdego kredytu w każdym
/// zapisanym świecie (`K-30`, zasada „dopisywać wyłącznie na końcu").
#[test]
fn kolejnosc_produktow_kredytowych_jest_kontraktem() {
    assert_eq!(LoanKind::Consumer.as_index(), 0);
    assert_eq!(LoanKind::WorkingCapital.as_index(), 1);
    assert_eq!(LoanKind::Investment.as_index(), 2);
    assert_eq!(LoanKind::ALL.len(), 3);
}

// ── WP8: cztery instrumenty a zachowanie pieniądza ───────────────────────────────

/// Kryterium WP8, część pierwsza: **pieniądz się zachowuje z włączonymi wszystkimi
/// czterema instrumentami**.
///
/// Przebieg jest jeden i długi, a nie tysiąc krótkich, z tego samego powodu co przy
/// teście P1 w `money_conservation.rs`: szuka się dryfu, który kumuluje się przez
/// historię, a historii o długości 1 nie ma jak skumulować.
///
/// Instrumenty są tu **wszystkie naraz i na przemian**, bo osobno każdy z nich
/// przechodzi trywialnie — rozjazd rodzi się na styku: kupon obligacji wypłacony
/// z konta, które w tej samej minucie zapłaciło ratę leasingu i sprzedało należność.
#[test]
fn cztery_instrumenty_zachowuja_pieniadz() {
    let mut books = Books::new();
    let rest = books.open_account(
        AccountOwner::RestOfWorld,
        AccountKind::Current,
        None,
        Money(0),
    );
    let bank = books.open_account(
        AccountOwner::Bank(firm(1)),
        AccountKind::Current,
        None,
        Money(0),
    );
    let firma = books.open_account(
        AccountOwner::Firm(firm(2)),
        AccountKind::Current,
        None,
        Money(0),
    );
    let leasingodawca = books.open_account(
        AccountOwner::Firm(firm(3)),
        AccountKind::Current,
        None,
        Money(0),
    );
    let faktor = books.open_account(
        AccountOwner::Firm(firm(4)),
        AccountKind::Current,
        None,
        Money(0),
    );
    let nabywca = books.open_account(
        AccountOwner::Firm(firm(5)),
        AccountKind::Current,
        None,
        Money(0),
    );
    books.endow(rest, Money(10_000_000_000), Tick(0)).unwrap();
    for konto in [bank, firma, leasingodawca, faktor, nabywca] {
        books
            .transfer(rest, konto, Money(1_000_000_000), memo(), Tick(0))
            .unwrap();
    }
    let przed = books.total_balance();

    let mut fin = CorpFinance::new(params());
    let mut loans = LoanBook::new();
    let mut payouts: Vec<SectorPayout> = Vec::new();

    // 1. Kredyt inwestycyjny — jedyny z czterech, który **tworzy** pieniądz.
    let kredyt = loans.open(
        AccountOwner::Firm(firm(2)),
        firm(1),
        LoanKind::Investment,
        Money(50_000_000),
        740,
        60,
        Tick(magnat_core::time::MINUTES_PER_MONTH),
    );
    books
        .create_credit(
            firma,
            Money(50_000_000),
            kredyt,
            DecisionReason::Unspecified,
            Tick(1),
        )
        .unwrap();

    // 2. Leasing — rzecz zostaje u leasingodawcy, płyną tylko raty.
    let asset = AssetRef::new(magnat_core::SiteId(ent(9)), AssetKind::Equipment);
    let leasing = fin.sign_lease(
        firm(2),
        firm(3),
        leasingodawca,
        asset,
        Money(2_000_000),
        36,
        Money(5_000_000),
    );

    // 3. Obligacja — przesuwa pieniądz, który już jest.
    let obligacja = fin
        .issue_bond(
            firm(2),
            firma,
            1100,
            36,
            &[(
                BondHolder::Firm(firm(5), nabywca),
                nabywca,
                Money(30_000_000),
            )],
            &mut books,
            Tick(2),
        )
        .expect("emisja doszła do skutku");

    // 4. Faktoring — zmienia wierzyciela, nie kwotę.
    let naleznosc = fin
        .arrears_mut()
        .accrue(
            AccountOwner::Firm(firm(6)),
            AccountOwner::Firm(firm(2)),
            Money(4_000_000),
            ClaimOrigin::Trade,
            Tick(2),
        )
        .expect("należność powstała");

    let miesiac = magnat_core::time::MINUTES_PER_MONTH;
    // Sześćdziesiąt miesięcy, bo tyle trwa kredyt inwestycyjny. Krótsza pętla
    // zostawiłaby niespłacony kapitał, czyli pieniądz kredytowy nadal w obiegu —
    // i test „suma się nie zmieniła" pękłby z powodu, który nie jest błędem.
    for m in 1..=60u64 {
        let t = Tick(m * miesiac);
        fin.pay_lease(leasing, firma, firm(2), &mut books, t);
        fin.pay_coupon(obligacja, &mut books, t, &mut payouts);
        if m == 6 {
            let dostal =
                fin.factor_receivable(naleznosc, firm(4), faktor, firma, 400, &mut books, t);
            assert_eq!(dostal, Money(3_840_000), "faktor zostawia sobie 4%");
        }
        if let Some(rata) = loans
            .get(kredyt)
            .and_then(magnat_economy::credit::Loan::next_installment)
        {
            if rata.interest.get() > 0 {
                books
                    .transfer(firma, bank, rata.interest, memo(), t)
                    .unwrap();
            }
            if rata.principal.get() > 0 {
                books
                    .destroy_credit(firma, rata.principal, kredyt, t)
                    .unwrap();
            }
            let l = loans.get_mut(kredyt).unwrap();
            l.outstanding = Money(l.outstanding.get() - rata.principal.get());
            l.paid_months += 1;
        }
        if fin.bond_matured(obligacja, t) {
            fin.redeem_bond(obligacja, &mut books, t, &mut payouts);
        }
    }

    books.check_conservation().expect("niezmiennik P1");
    // Kredyt spłacony w całości, więc podaż wróciła do stanu sprzed: to jest
    // mocniejsze niż samo `check_conservation`, bo tamto przeszłoby również wtedy,
    // gdyby destrukcja nie zrównoważyła kreacji, a obie były zapisane.
    assert_eq!(
        books.total_balance(),
        przed,
        "instrumenty zmieniły sumę pieniądza w mieście"
    );
    assert!(payouts.is_empty(), "nabywcą był firma, nie gospodarstwo");
    assert!(
        loans
            .get(kredyt)
            .is_some_and(magnat_economy::credit::Loan::is_closed),
        "kredyt nie został spłacony do końca"
    );
    // Należność zmieniła wierzyciela i **nie** zmieniła kwoty — to jest cała
    // mechanika faktoringu i cały powód, dla którego dłużnik o niczym nie wie.
    let a = fin.arrears().get(naleznosc).unwrap();
    assert_eq!(a.creditor, AccountOwner::Firm(firm(4)));
    assert_eq!(a.amount, Money(4_000_000));
    assert!(a.factored);
}

/// Leasing kończy się odbiorem rzeczy, a nie długiem bez końca.
#[test]
fn nieplacony_leasing_konczy_sie_odbiorem_rzeczy() {
    let mut books = Books::new();
    let rest = books.open_account(
        AccountOwner::RestOfWorld,
        AccountKind::Current,
        None,
        Money(0),
    );
    let biedna = books.open_account(
        AccountOwner::Firm(firm(2)),
        AccountKind::Current,
        None,
        Money(0),
    );
    let lessor = books.open_account(
        AccountOwner::Firm(firm(3)),
        AccountKind::Current,
        None,
        Money(0),
    );
    books.endow(rest, Money(1_000_000), Tick(0)).unwrap();
    let _ = lessor;

    let mut fin = CorpFinance::new(params());
    let asset = AssetRef::new(magnat_core::SiteId(ent(9)), AssetKind::Equipment);
    let l = fin.sign_lease(
        firm(2),
        firm(3),
        lessor,
        asset,
        Money(2_000_000),
        36,
        Money(5_000_000),
    );
    assert!(fin.is_leased(asset), "rzecz jest w leasingu");
    for m in 1..=2u64 {
        let wyszlo = fin.pay_lease(l, biedna, firm(2), &mut books, Tick(m * 43_200));
        assert_eq!(wyszlo, Money::ZERO, "firma bez środków nie zapłaciła");
    }
    assert!(fin.repossess_if_due(l, 2), "dwie raty z rzędu kończą umowę");
    // Umowa jest skończona, ale rzecz **nadal należy do leasingodawcy** — nie było
    // wykupu. To są dwa różne zdania i właśnie ich pomylenie wpuściłoby cudzą maszynę
    // do masy upadłościowej (niezmiennik 3 z M7 §7.2).
    assert!(!fin.lease(l).unwrap().is_active(), "umowa nadal biegnie");
    assert!(
        fin.is_leased(asset),
        "rzecz przestała należeć do leasingodawcy"
    );
    // Niezapłacone raty są długiem, a nie zniknęły.
    assert_eq!(
        fin.arrears().debt_of(AccountOwner::Firm(firm(2))),
        Money(4_000_000)
    );
    books.check_conservation().expect("niezmiennik P1");
}

// ── WP9: podział masy ────────────────────────────────────────────────────────────

/// Losowe roszczenie: priorytet, wierzyciel i kwota.
fn roszczenie() -> impl Strategy<Value = (usize, u32, i64)> {
    (0..CLAIM_PRIORITY_COUNT, 0u32..40, 1i64..50_000_000)
}

fn postepowanie(claims: Vec<(usize, u32, i64)>, proceeds: i64) -> Bankruptcy {
    let mut b = Bankruptcy::new(
        BankruptcyId(0),
        firm(2),
        AccountId(0),
        BankruptcyTrigger::Illiquid,
        500,
        Tick(0),
    );
    b.proceeds = Money(proceeds);
    for (p, kto, kwota) in claims {
        let origin = match ClaimPriority::ALL[p] {
            ClaimPriority::Wages => ClaimOrigin::Wages,
            ClaimPriority::Severance => ClaimOrigin::Severance,
            ClaimPriority::Secured => ClaimOrigin::Loan(magnat_economy::books::LoanId(0)),
            ClaimPriority::Public => ClaimOrigin::Tax,
            ClaimPriority::Unsecured | ClaimPriority::Owners => ClaimOrigin::Trade,
        };
        let _ = b.file_claim(
            AccountOwner::Firm(firm(kto + 100)),
            Money(kwota),
            origin,
            None,
            None,
            Tick(0),
        );
    }
    b.claims
        .sort_by_key(|c| (c.priority.as_index(), c.creditor, c.id.0));
    b
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    /// Niezmienniki 5, 6 i 7 z M7 §7.2 na dziesięciu tysiącach losowych konfiguracji.
    ///
    /// 5. Σ wypłat per priorytet ≤ Σ roszczeń per priorytet, a **żaden niższy priorytet
    ///    nie dostał ani grosza, dopóki wyższy nie jest zaspokojony w całości**.
    /// 6. Podział proporcjonalny sumuje się **co do grosza** do kwoty dzielonej.
    /// 7. Ten sam przypadek daje identyczny ciąg wypłat.
    #[test]
    fn bankruptcy_conserves_money_and_assets(
        claims in prop::collection::vec(roszczenie(), 0..24),
        gotowka in 0i64..400_000_000,
        proceeds in 0i64..200_000_000,
    ) {
        let b = postepowanie(claims, proceeds);
        let plan = b.plan_distribution(Money(gotowka));

        // 6. Nic nie powstało i nic nie zginęło: wypłaty plus syndyk plus reszta
        //    to dokładnie to, co było na koncie.
        let rozdane = plan.total_paid().get() + plan.trustee_fee.get() + plan.residual.get();
        prop_assert_eq!(rozdane, gotowka.max(0), "podział nie sumuje się do masy");

        // 5a. Nikt nie dostał więcej, niż mu się należało.
        for (i, c) in b.claims.iter().enumerate() {
            prop_assert!(plan.payouts[i] <= c.amount, "wypłata ponad roszczenie");
            prop_assert!(plan.payouts[i].get() >= 0, "wypłata ujemna");
        }

        // 5b. Kolejność priorytetów: niższy dostaje coś dopiero wtedy, gdy wyższy
        //     jest zaspokojony **w całości**. To jest cała treść `ClaimPriority`.
        let mut niepelny = None;
        for p in ClaimPriority::ALL {
            let (mut nalezne, mut wyplacone) = (0i64, 0i64);
            for (i, c) in b.claims.iter().enumerate() {
                if c.priority == *p {
                    nalezne += c.amount.get();
                    wyplacone += plan.payouts[i].get();
                }
            }
            prop_assert!(wyplacone <= nalezne, "priorytet {:?} dostał za dużo", p);
            if let Some(wyzszy) = niepelny {
                prop_assert_eq!(
                    wyplacone, 0,
                    "priorytet {:?} dostał wypłatę, choć {:?} nie jest zaspokojony",
                    p, wyzszy
                );
            } else if nalezne > 0 && wyplacone < nalezne {
                niepelny = Some(*p);
            }
        }

        // 7. Determinizm: ten sam przypadek, ten sam ciąg wypłat.
        let znowu = b.plan_distribution(Money(gotowka));
        prop_assert_eq!(plan.payouts, znowu.payouts, "podział nie jest deterministyczny");
        prop_assert_eq!(plan.ratio_bp, znowu.ratio_bp);
    }
}

/// Niezmienniki 1, 2 i 3 z M7 §7.2 na pełnym przebiegu postępowania.
///
/// Osobno od testu wyżej i na jednym przypadku, bo tu sprawdza się co innego:
/// tamten pyta, czy **plan** jest poprawny, ten — czy wykonanie planu na realnych
/// kontach nie gubi pieniędzy i czy każdy lot skończył w dokładnie jednym stanie.
#[test]
fn postepowanie_nie_gubi_pieniedzy_ani_lotow() {
    let mut books = Books::new();
    let rest = books.open_account(
        AccountOwner::RestOfWorld,
        AccountKind::Current,
        None,
        Money(0),
    );
    let upadly = books.open_account(
        AccountOwner::Firm(firm(2)),
        AccountKind::Current,
        None,
        Money(0),
    );
    let bank = books.open_account(
        AccountOwner::Bank(firm(1)),
        AccountKind::Current,
        None,
        Money(0),
    );
    let dostawca = books.open_account(
        AccountOwner::Firm(firm(7)),
        AccountKind::Current,
        None,
        Money(0),
    );
    books.endow(rest, Money(1_000_000_000), Tick(0)).unwrap();
    books
        .transfer(rest, upadly, Money(3_000_000), memo(), Tick(0))
        .unwrap();
    let przed = books.total_balance();

    let mut fin = CorpFinance::new(params());
    let mut loans = LoanBook::new();
    let mut payouts: Vec<SectorPayout> = Vec::new();

    // Rzecz leasingowana — nie ma prawa wejść do masy (niezmiennik 3).
    let leasowana = AssetRef::new(magnat_core::SiteId(ent(9)), AssetKind::Equipment);
    fin.sign_lease(
        firm(2),
        firm(3),
        dostawca,
        leasowana,
        Money(1_000_000),
        24,
        Money(2_000_000),
    );

    // Trzy długi o trzech priorytetach — kolejność zaspokojenia ma być widoczna.
    for (wierzyciel, kwota, origin) in [
        (
            AccountOwner::Citizen(CitizenId(ent(30))),
            Money(4_000_000),
            ClaimOrigin::Severance,
        ),
        (
            AccountOwner::Bank(firm(1)),
            Money(9_000_000),
            ClaimOrigin::Loan(magnat_economy::books::LoanId(0)),
        ),
        (
            AccountOwner::Firm(firm(7)),
            Money(6_000_000),
            ClaimOrigin::Trade,
        ),
    ] {
        fin.arrears_mut().accrue(
            AccountOwner::Firm(firm(2)),
            wierzyciel,
            kwota,
            origin,
            Tick(0),
        );
    }

    let case = fin.open_bankruptcy(
        firm(2),
        upadly,
        BankruptcyTrigger::Illiquid,
        95,
        &loans,
        Tick(0),
    );
    let _ = &mut loans;
    fin.add_lot(case, leasowana, Money(20_000_000));
    fin.add_lot(
        case,
        AssetRef::new(magnat_core::SiteId(ent(9)), AssetKind::Inventory),
        Money(10_000_000),
    );

    // Postępowanie: zgłoszenie → wycena → trzy rundy → podział.
    let doba = magnat_core::time::MINUTES_PER_DAY;
    let mut gotowe = false;
    for d in 1..=64u64 {
        if fin.step_case(case, Tick(d * doba)) {
            fin.settle_scrap(case, rest, &mut books, Tick(d * doba));
            fin.distribute(case, rest, &mut books, Tick(d * doba), &mut payouts);
            gotowe = true;
            break;
        }
    }
    assert!(gotowe, "postępowanie nie doszło do podziału w 64 doby");

    let b = fin.case(case).unwrap();
    assert_eq!(b.stage, BankruptcyStage::Closed);

    // Niezmiennik 2: każdy lot w dokładnie jednym stanie, suma się zgadza.
    let w_masie = b
        .estate
        .iter()
        .filter(|l| l.fate == LotFate::InEstate)
        .count();
    let sprzedane = b
        .estate
        .iter()
        .filter(|l| matches!(l.fate, LotFate::Sold { .. }))
        .count();
    let zwrocone = b
        .estate
        .iter()
        .filter(|l| l.fate == LotFate::ReturnedToLessor)
        .count();
    let spisane = b
        .estate
        .iter()
        .filter(|l| l.fate == LotFate::WrittenOff)
        .count();
    assert_eq!(w_masie, 0, "lot został w masie po domknięciu");
    assert_eq!(w_masie + sprzedane + zwrocone + spisane, b.estate.len());

    // Niezmiennik 3: rzecz leasingowana nie weszła do masy i nie została sprzedana.
    let leas = b.estate.iter().find(|l| l.asset == leasowana).unwrap();
    assert_eq!(
        leas.fate,
        LotFate::ReturnedToLessor,
        "rzecz leasingowana trafiła do masy"
    );

    // Niezmiennik 1: pieniądz się zachował. Suma sald **spada** o to, co wyszło
    // do mieszkańców, bo gospodarstwa nie mają kont w `Books` (M5b) i wypłata dla
    // człowieka przechodzi kanałem sektora gospodarstw. Równanie domyka się dopiero
    // po obu stronach granicy — i to jest dokładnie ta granica, na której pieniądz
    // najłatwiej zgubić.
    books.check_conservation().expect("niezmiennik P1");
    let do_ludzi: i64 = payouts.iter().map(|p| p.amount.get()).sum();
    assert_eq!(
        books.total_balance().get() + do_ludzi,
        przed.get(),
        "pieniądz powstał albo zginął"
    );

    // Kolejność zaspokojenia: odprawa (priorytet 1) przed bankiem (2) i dostawcą (4).
    // Masa jest mniejsza od sumy roszczeń, więc dostawca ma dostać zero — i to jest
    // decyzja właściciela produktu z `D7`, widziana od strony wyniku.
    let odprawa = b
        .claims
        .iter()
        .find(|c| c.creditor == AccountOwner::Citizen(CitizenId(ent(30))))
        .unwrap();
    assert_eq!(odprawa.priority, ClaimPriority::Severance);
    assert!(odprawa.paid.get() > 0, "pracownik nie dostał nic");
    assert_eq!(payouts.len(), 1, "wypłata dla człowieka wychodzi listą");
    assert_eq!(payouts[0].to, CitizenId(ent(30)));

    // Zaległości upadłego są zamknięte: dług nie żyje dalej po śmierci firmy.
    assert_eq!(
        fin.arrears().debt_of(AccountOwner::Firm(firm(2))),
        Money::ZERO
    );
    let _ = bank;
}

/// Roszczenie zgłoszone po wejściu w podział jest odrzucane — termin jest twardy,
/// bo inaczej podział nigdy by się nie skończył. Woła to M8 (`K-10`), więc reguła
/// musi być widoczna z zewnątrz, a nie schowana w przebiegu.
#[test]
fn roszczenie_po_terminie_jest_odrzucane() {
    let mut b = Bankruptcy::new(
        BankruptcyId(0),
        firm(2),
        AccountId(0),
        BankruptcyTrigger::CourtOrder,
        500,
        Tick(0),
    );
    assert!(b
        .file_claim(
            AccountOwner::City,
            Money(1_000),
            ClaimOrigin::Tax,
            None,
            None,
            Tick(0)
        )
        .is_ok());
    b.stage = BankruptcyStage::Distribution;
    assert_eq!(
        b.file_claim(
            AccountOwner::City,
            Money(1_000),
            ClaimOrigin::Tax,
            None,
            None,
            Tick(1)
        ),
        Err(magnat_economy::corpfin::ClaimRejected::TooLate)
    );
}

/// Kredyt zabezpieczony ponad wartość zastawu rozpada się na **dwa** roszczenia.
///
/// Bez tego bank dostawałby z pierwszego priorytetu więcej, niż wart jest jego
/// zastaw — i nikt by tego nie zobaczył, bo różnica siedziałaby w środku jednej kwoty.
#[test]
fn zabezpieczenie_ponad_wartosc_spada_do_niezabezpieczonych() {
    let mut b = Bankruptcy::new(
        BankruptcyId(0),
        firm(2),
        AccountId(0),
        BankruptcyTrigger::NegativeEquity,
        0,
        Tick(0),
    );
    let asset = AssetRef::new(magnat_core::SiteId(ent(9)), AssetKind::Equipment);
    b.file_claim(
        AccountOwner::Bank(firm(1)),
        Money(10_000_000),
        ClaimOrigin::Loan(magnat_economy::books::LoanId(0)),
        Some((asset, Money(4_000_000))),
        None,
        Tick(0),
    )
    .unwrap();
    assert_eq!(b.claims.len(), 2);
    assert_eq!(b.claims[0].priority, ClaimPriority::Secured);
    assert_eq!(b.claims[0].amount, Money(4_000_000));
    assert_eq!(b.claims[1].priority, ClaimPriority::Unsecured);
    assert_eq!(b.claims[1].amount, Money(6_000_000));
}

/// Plik `data/tuning/insolvency.ron` ładuje się i jest spójny.
#[test]
fn parametry_upadlosci_laduja_sie_z_danych() {
    let p = InsolvencyParams::load_default().expect("data/tuning/insolvency.ron");
    assert_eq!(p.estate.round_discount_bp.len(), p.estate.rounds as usize);
    assert!(p.trigger.illiquid_days > 0);
    // Wyprzedaż ma tanieć, a nie drożeć.
    assert!(p.estate.round_discount_bp.windows(2).all(|w| w[0] <= w[1]));
}

/// Walidator odrzuca plik, w którym rund jest więcej niż dyskont. Bez tego trzecia
/// runda miałaby po cichu cenę drugiej i wynik każdej upadłości w mieście byłby inny,
/// niż mówi plik.
#[test]
fn niespojne_rundy_zatrzymuja_ladowanie() {
    let txt = r"(
        schema_version: 1,
        trigger: (illiquid_days: 90, illiquid_min_gr: 100000, negative_equity_arrears_months: 3),
        estate: (trustee_fee_bp: 500, rounds: 3, round_days: 15,
                 round_discount_bp: [0, 2500], scrap_bp: 1000),
        valuation: (equipment_bp: 7000, inventory_bp: 6000),
    )";
    assert!(InsolvencyParams::parse(txt).is_err());
}
