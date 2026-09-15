//! Testy własnościowe pieniądza (M5 §7.1): **P1**, **P1b**, **P3**.
//!
//! Kryterium ukończenia WP1 brzmi „suma pieniądza = emisja − destrukcja na losowym
//! strumieniu 10⁶ przelewów, tolerancja 0 groszy". Strumień jest tu jeden, długi
//! i deterministyczny — a nie 10⁶ osobnych przypadków `proptest`: szuka się dryfu,
//! który kumuluje się przez historię, a historii o długości 1 nie ma jak skumulować.
//! `proptest` pilnuje obok tego rzeczy, do której nadaje się lepiej: skracania
//! kontrprzykładu, gdy kombinacja kanałów wywróci niezmiennik.
//!
//! P1b jest tu mimo że kanał kapitału zewnętrznego jest w M5 **nieużywany** — to nie
//! przeoczenie, tylko cel: bez tego testu kanał pękłby dopiero w M7, daleko od miejsca,
//! w którym powstał niezmiennik.

use magnat_core::{DecisionReason, Entity, FirmId, HouseholdId, Money, Rng, Tick};
use magnat_economy::books::{
    AccountId, AccountKind, AccountOwner, Books, ExternalInvestorId, LoanId, TxError, TxKind,
    TxMemo,
};
use proptest::prelude::*;
use std::num::NonZeroU32;

const ACCOUNTS: usize = 64;

fn ent(i: u32) -> Entity {
    Entity::new(i, NonZeroU32::MIN)
}

/// Świat testowy: konto `RestOfWorld` z emisją początkową plus zbiór kont domowych,
/// część z limitem debetu. Zwraca księgę i listę kont.
fn world(endowment: i64) -> (Books, Vec<AccountId>) {
    let mut b = Books::new();
    let mut ids = Vec::with_capacity(ACCOUNTS);
    ids.push(b.open_account(
        AccountOwner::RestOfWorld,
        AccountKind::Current,
        None,
        Money::ZERO,
    ));
    for i in 1..ACCOUNTS as u32 {
        let owner = match i % 4 {
            0 => AccountOwner::Household(HouseholdId(ent(i))),
            1 => AccountOwner::Firm(FirmId(ent(i))),
            2 => AccountOwner::Bank(FirmId(ent(i))),
            _ => AccountOwner::CentralBank,
        };
        // Co czwarte konto z debetem — inaczej ścieżka `InsufficientFunds`
        // byłaby jedyną odrzucającą i limit nigdy by się nie sprawdził.
        let limit = if i % 4 == 1 {
            Money(50_000)
        } else {
            Money::ZERO
        };
        ids.push(b.open_account(owner, AccountKind::Current, None, limit));
    }
    b.endow(ids[0], Money(endowment), Tick(0)).unwrap();
    (b, ids)
}

fn memo() -> TxMemo {
    TxMemo::new(TxKind::Withdrawal, DecisionReason::Unspecified)
}

/// Żadne saldo nie schodzi poniżej własnego limitu debetu (P3).
fn assert_p3(b: &Books) {
    for i in 0..b.account_count() {
        let a = b.account(AccountId(i as u32)).unwrap();
        assert!(
            a.balance().get() >= -a.overdraft_limit.get(),
            "konto {i}: saldo {} poniżej limitu debetu {}",
            a.balance().get(),
            a.overdraft_limit.get()
        );
    }
}

#[test]
fn p1_milion_operacji_zachowuje_sume_co_do_grosza() {
    let (mut b, ids) = world(5_000_000_000);
    let mut rng = Rng::from_state([0x243F_6A88_85A3_08D3, 7, 0x1319_8A2E_0370_7344, 11]);
    let mut loan = 0u32;
    let mut rejected = 0u64;

    for step in 0..1_000_000u64 {
        let from = ids[rng.gen_range_u32(ACCOUNTS as u32) as usize];
        let to = ids[rng.gen_range_u32(ACCOUNTS as u32) as usize];
        let amount = Money(i64::from(rng.gen_range_u32(10_000)) - 2_000);
        let t = Tick(step / 16);

        // Rozkład operacji: przelewy dominują, kanały emisji wchodzą rzadko —
        // tak jak w świecie, gdzie kredyt jest zdarzeniem, a zakup rutyną.
        let r = match rng.gen_range_u32(1_000) {
            0..=959 => b.transfer(from, to, amount, memo(), t),
            960..=974 => {
                loan += 1;
                b.create_credit(to, amount, LoanId(loan), DecisionReason::Unspecified, t)
            }
            975..=989 => b.destroy_credit(from, amount, LoanId(loan.max(1)), t),
            990..=994 => b.inject_external_capital(to, amount, ExternalInvestorId(1), t),
            995..=997 => b.repatriate_external_capital(from, amount, ExternalInvestorId(1), t),
            _ => b.endow(to, amount, t),
        };
        if r.is_err() {
            rejected += 1;
        }

        if step % 50_000 == 0 {
            assert_eq!(b.check_conservation(), Ok(()), "rozjazd na kroku {step}");
        }
    }

    assert_eq!(b.check_conservation(), Ok(()));
    assert_p3(&b);
    // Test bez odrzuceń nie sprawdziłby, czy nieudana operacja nie zostawia śladu.
    assert!(
        rejected > 1_000,
        "za mało odrzuceń ({rejected}) — strumień nie dotyka ścieżek błędu"
    );
    assert!(b.journal().total() + rejected == 1_000_001);
}

#[test]
fn nieudana_operacja_nie_zostawia_sladu() {
    let (mut b, ids) = world(1_000);
    let before: Vec<Money> = (0..b.account_count())
        .map(|i| b.balance(AccountId(i as u32)).unwrap())
        .collect();
    let supply_before = *b.supply();
    let journal_before = b.journal().total();

    assert_eq!(
        // ids[2] jest bankiem bez limitu debetu — jedyne konto z pieniędzmi to ids[0].
        b.transfer(ids[2], ids[3], Money(1), memo(), Tick(0)),
        Err(TxError::InsufficientFunds)
    );
    assert_eq!(
        b.transfer(ids[0], ids[0], Money(1), memo(), Tick(0)),
        Err(TxError::SameAccount)
    );
    assert_eq!(
        b.transfer(ids[0], ids[1], Money(0), memo(), Tick(0)),
        Err(TxError::NonPositive)
    );
    assert_eq!(
        b.create_credit(AccountId::OUTSIDE, Money(5), LoanId(1), DecisionReason::Unspecified, Tick(0)),
        Err(TxError::Unknown)
    );
    assert_eq!(
        b.inject_external_capital(ids[0], Money(-5), ExternalInvestorId(1), Tick(0)),
        Err(TxError::NonPositive)
    );

    let after: Vec<Money> = (0..b.account_count())
        .map(|i| b.balance(AccountId(i as u32)).unwrap())
        .collect();
    assert_eq!(before, after);
    assert_eq!(supply_before, *b.supply());
    assert_eq!(journal_before, b.journal().total());
}

#[test]
fn przepelnienie_konczy_sie_bledem_a_nie_panika() {
    let (mut b, ids) = world(1_000);
    b.endow(ids[1], Money(i64::MAX - 1_000), Tick(0)).unwrap();
    assert_eq!(
        b.endow(ids[1], Money(i64::MAX), Tick(0)),
        Err(TxError::Overflow)
    );
    assert_eq!(b.check_conservation(), Ok(()));
}

/// Operacja strumienia P1b. Kanały są wymieszane celowo: pojedynczo każdy z nich
/// domyka się trywialnie, a niezmiennik pęka na ich przeplocie.
#[derive(Clone, Copy, Debug)]
enum Op {
    Transfer(usize, usize, i64),
    Credit(usize, i64),
    Repay(usize, i64),
    Inject(usize, i64),
    Repatriate(usize, i64),
}

fn op_strategy() -> impl Strategy<Value = Op> {
    let acc = 0usize..ACCOUNTS;
    let amt = -1_000i64..1_000_000i64;
    prop_oneof![
        (acc.clone(), acc.clone(), amt.clone()).prop_map(|(a, b, m)| Op::Transfer(a, b, m)),
        (acc.clone(), amt.clone()).prop_map(|(a, m)| Op::Credit(a, m)),
        (acc.clone(), amt.clone()).prop_map(|(a, m)| Op::Repay(a, m)),
        (acc.clone(), amt.clone()).prop_map(|(a, m)| Op::Inject(a, m)),
        (acc, amt).prop_map(|(a, m)| Op::Repatriate(a, m)),
    ]
}

proptest! {
    /// P1b — kanał kapitału zewnętrznego wpleciony w strumień przelewów i kredytów
    /// nie łamie P1.
    #[test]
    fn p1b_kanal_kapitalu_nie_lamie_zachowania_pieniadza(ops in prop::collection::vec(op_strategy(), 1..300)) {
        let (mut b, ids) = world(10_000_000);
        for (i, op) in ops.iter().enumerate() {
            let t = Tick(i as u64);
            let _ = match *op {
                Op::Transfer(a, c, m) => b.transfer(ids[a], ids[c], Money(m), memo(), t),
                Op::Credit(a, m) => b.create_credit(ids[a], Money(m), LoanId(1), DecisionReason::Unspecified, t),
                Op::Repay(a, m) => b.destroy_credit(ids[a], Money(m), LoanId(1), t),
                Op::Inject(a, m) => b.inject_external_capital(ids[a], Money(m), ExternalInvestorId(3), t),
                Op::Repatriate(a, m) => b.repatriate_external_capital(ids[a], Money(m), ExternalInvestorId(3), t),
            };
            prop_assert_eq!(b.check_conservation(), Ok(()));
        }
        assert_p3(&b);
    }
}
