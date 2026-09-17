//! Faza 6 — finanse (M10a §5.7, co krok).
//!
//! Wypłaty, odsetki i test wypłacalności. Podatków nie ma: stawki należą do M8
//! i do tego czasu hak jest **zerem**, a nie liczbą „na oko" (`R13`).
//!
//! Każda kwota przechodzi przez [`crate::types::MacroLedger::transfer`], czyli
//! przez jedyne wejście, które umie pieniądz **przesunąć** i nie umie go stworzyć.
//! To jest cały mechanizm, którym makro spełnia tolerancję zero z 00 §4 pkt 1.

use magnat_core::Money;
use magnat_economy::kernel;

use crate::state::MacroState;
use crate::types::MacroAccount;

/// Miesiąc ma 30 dób (`K-1`), więc dobowa część listy płac to jedna trzydziesta.
const DAYS_PER_MONTH: i64 = 30;

pub fn phase(st: &mut MacroState, p: &super::MacroParams) {
    for fi in 0..st.firms.len() {
        let dzielnica = st.firms[fi].district.0;

        // ── wypłaty ──────────────────────────────────────────────────────────
        let dniowka = st.firms[fi].wage_bill.get() / DAYS_PER_MONTH;
        let wyplata = dniowka.min(st.firms[fi].capital.get().max(0));
        if wyplata > 0 {
            st.firms[fi].capital = Money(st.firms[fi].capital.get() - wyplata);
            st.ledger.transfer(
                MacroAccount::Firms,
                MacroAccount::Households,
                Money(wyplata),
            );
            rozdziel_wyplate(st, dzielnica, wyplata);
        }

        // ── odsetki ──────────────────────────────────────────────────────────
        let odsetki = kernel::monthly_interest(st.firms[fi].debt, p.debt_rate_bp_month).get()
            / DAYS_PER_MONTH;
        let splata = odsetki.min(st.firms[fi].capital.get().max(0));
        if splata > 0 {
            st.firms[fi].capital = Money(st.firms[fi].capital.get() - splata);
            st.ledger.transfer(
                MacroAccount::Firms,
                MacroAccount::RestOfWorld,
                Money(splata),
            );
        }
    }
}

/// Wypłata idzie do komórek dzielnicy proporcjonalnie do liczby zatrudnionych,
/// a reszta z dzielenia do pierwszej z nich (00 §2). Bez tego suma gotówki
/// gospodarstw różniłaby się od konta `Households` o kilka groszy na firmę —
/// i test zachowania pieniądza pękałby z właściwego powodu, w złym miejscu.
fn rozdziel_wyplate(st: &mut MacroState, dzielnica: u16, kwota: i64) {
    let indeksy: Vec<usize> = st
        .cells
        .iter()
        .enumerate()
        .filter(|(_, c)| c.key.0 .0 == dzielnica && c.employed > 0)
        .map(|(i, _)| i)
        .collect();
    if indeksy.is_empty() {
        // Dzielnica bez zatrudnionych: pieniądz i tak musi gdzieś wylądować,
        // bo konto `Households` już go dostało. Trafia do pierwszej komórki
        // w kolejności klucza — dowolność jest tu jawna i bez konsekwencji,
        // bo taka firma nie ma komu płacić i jej `wage_bill` jest zerem.
        if let Some(c) = st.cells.first_mut() {
            c.cash = Money(c.cash.get().saturating_add(kwota));
        }
        return;
    }
    let suma: u64 = indeksy
        .iter()
        .map(|i| u64::from(st.cells[*i].employed))
        .sum();
    let mut rozdane: i64 = 0;
    for i in indeksy.iter().skip(1) {
        let czesc = (kwota as i128 * i128::from(st.cells[*i].employed) / i128::from(suma)) as i64;
        st.cells[*i].cash = Money(st.cells[*i].cash.get().saturating_add(czesc));
        rozdane += czesc;
    }
    let pierwsza = indeksy[0];
    st.cells[pierwsza].cash = Money(
        st.cells[pierwsza]
            .cash
            .get()
            .saturating_add(kwota - rozdane),
    );
}
