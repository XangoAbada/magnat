//! Rozliczanie należności: jedyne miejsce, w którym pieniądz przechodzi
//! od płatnika do miasta (M8a WP2).
//!
//! Jedno miejsce, bo inaczej domknięcie `Σ Settled == Δ CityBudget.revenue`
//! byłoby umową, a nie faktem. Pieniądz rusza **wyłącznie** `Books::transfer`
//! (00 §5 fazy, `Z6`): miasto jest zwykłym posiadaczem konta, objętym
//! niezmiennikiem `MoneySupplyLedger` bez wyjątków.
//!
//! Brak środków u płatnika **nie jest awarią**: należność przechodzi w zaległość,
//! rosną odsetki, a po okresie przedawnienia zostaje umorzona. Panika byłaby tu
//! najgorszą z możliwych odpowiedzi — firma bez gotówki to normalny stan
//! gospodarki, a nie błąd programu.

use magnat_core::{AbateReason, CityReason, DecisionReason, Money, SimCalendar, TaxKind, Tick};
use magnat_economy::{Books, ChargeKind, Market, TxKind, TxMemo};

use crate::calc::late_interest;
use crate::charge::{TaxChargeId, TaxPayer};
use crate::city::City;

/// Ile należności rozliczono i na jaką kwotę.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct SettleReport {
    pub settled: u32,
    pub amount: Money,
    pub overdue: u32,
    pub abated: u32,
}

/// Rozlicza wszystko, co wymagalne w tym ticku. Kolejność jest kolejnością
/// identyfikatorów, czyli powstania — deterministyczna bez jednego losowania.
pub fn settle_due(
    city: &mut City,
    market: &Market,
    books: &mut Books,
    rest: magnat_economy::AccountId,
    t: Tick,
) -> SettleReport {
    let mut rap = SettleReport::default();
    let konto_miasta = city.budget.account;
    for id in city.charges.due_now(t) {
        let Some(c) = city.charges.get(id).copied() else {
            continue;
        };
        // **Płatnik i źródło pieniądza to nie zawsze to samo konto** — i tylko przy
        // cle się rozchodzą. Importer jest płatnikiem ekonomicznym i to jego karta
        // inspekcji ma pokazać cło, ale zapłacił je **już przy odprawie**, razem
        // z zapłatą za towar (`payable() = net + duty`, `K-36`). Ściągnięcie go
        // drugi raz z konta zakładu obciążyłoby firmę podwójnie; pieniądz stoi
        // w kanale importowym i stamtąd wchodzi do budżetu.
        let zrodlo = match (c.kind, c.payer) {
            (TaxKind::Duty, _) => Some(rest),
            (_, TaxPayer::Site(site)) => market.account_of(site),
            // Zaliczki PIT stoją u pracodawcy — w M8a jest nim „reszta świata".
            (_, TaxPayer::External) => Some(rest),
            (_, TaxPayer::Household(_)) => None,
        };
        let Some(zrodlo) = zrodlo else {
            // Płatnik bez konta zniknął ze świata; należności nie ma z czego
            // ściągnąć i udawanie, że stoi, zafałszowałoby budżet.
            city.charges.abate(id, t, AbateReason::PayerGone);
            rap.abated += 1;
            continue;
        };
        let memo = TxMemo::new(
            TxKind::TaxPayment {
                charge: ChargeKind(c.kind.as_index() as u16),
            },
            DecisionReason::City(CityReason::TaxSettled {
                kind: c.kind,
                amount: c.amount,
            }),
        );
        if books
            .transfer(zrodlo, konto_miasta, c.amount, memo, t)
            .is_err()
        {
            // Nie ma z czego zapłacić — zaległość, nie panika.
            oznacz_zalegla(city, id, t, &mut rap);
            continue;
        }
        // Księga zakładu: zobowiązanie schodzi, gotówka schodzi. Dla VAT-u i akcyzy
        // zobowiązanie stoi na `TaxPayable` od chwili sprzedaży; dla danin naliczanych
        // okresowo wpisuje je `post_tax_accrual` w chwili naliczenia.
        //
        // Cło tędy **nie idzie**: nie było zobowiązania do zdjęcia, bo importer
        // zapłacił przy odprawie, a kwota siedzi w koszcie nabycia partii
        // (`Dr Inventory` w `absorb_settlements`) — dokładnie tam, gdzie cło ma być.
        if let (TaxPayer::Site(site), false) = (c.payer, c.kind == TaxKind::Duty) {
            market.post_tax_payment(site, c.amount, memo.reason);
        }
        city.charges.mark_settled(id, t);
        city.credit_revenue(c.kind, c.amount);
        rap.settled += 1;
        rap.amount = Money(rap.amount.get() + c.amount.get());
    }
    rap
}

fn oznacz_zalegla(city: &mut City, id: TaxChargeId, t: Tick, rap: &mut SettleReport) {
    let Some(c) = city.charges.get(id).copied() else {
        return;
    };
    // Odsetki za jedną dobę zwłoki. Naliczamy je raz na dobę, bo system chodzi
    // raz na dobę — gdyby chodził częściej, ta sama doba policzyłaby się dwa razy.
    let dzienne = late_interest(c.amount, city.code.late_interest_bp_per_year, 1);
    if city.charges.mark_overdue(id, t, dzienne) {
        rap.overdue += 1;
    }
}

/// Starzenie zaległości: odsetki i przedawnienie. Raz na dobę.
pub fn age_overdue(city: &mut City, t: Tick) -> u32 {
    let przedawnienie = u64::from(city.code.time_bar_days) * 1_440;
    let mut umorzone = 0;
    let zalegle = city.charges.overdue_now();
    let bp = city.code.late_interest_bp_per_year;
    for (id, od, kwota) in zalegle {
        if t.0.saturating_sub(od.0) >= przedawnienie {
            city.charges.abate(id, t, AbateReason::TimeBarred);
            umorzone += 1;
            continue;
        }
        city.charges
            .mark_overdue(id, t, late_interest(kwota, bp, 1));
    }
    umorzone
}

/// Umarza otwarte należności płatnika, którego postępowanie upadłościowe nie
/// zaspokoiło (`K-10`, `ClaimPriority::Public`).
///
/// M8 jest tu **wierzycielem**, nie organem egzekucyjnym: postępowanie prowadzi
/// M7 i to ono decyduje, ile masy przypadnie na zobowiązania publiczne. Miasto
/// zamyka swoją stronę księgi i nic poza tym.
pub fn abate_bankrupt(city: &mut City, payer: TaxPayer, t: Tick) -> u32 {
    let mut n = 0;
    for id in city.charges.open_of_payer(payer) {
        city.charges.abate(id, t, AbateReason::Bankruptcy);
        n += 1;
    }
    n
}

/// Dzień miesiąca, w którym system miasta domyka miesiąc — pierwszy.
#[must_use]
pub fn is_month_start(t: Tick) -> bool {
    let cal = SimCalendar::new(t);
    cal.day_of_month() == 1
}

/// Czy ta doba zaczyna rok kalendarzowy.
#[must_use]
pub fn is_year_start(t: Tick) -> bool {
    let cal = SimCalendar::new(t);
    cal.day_of_month() == 1 && cal.month_of_year() == 1
}
