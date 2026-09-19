//! Faza 6 — finanse (M10a §5.7, co krok).
//!
//! Wypłaty, odsetki i test wypłacalności. Podatków nie ma i to jest **stan po
//! M10a**, nie przeoczenie: danina jest naliczana na transakcji, a transakcją
//! w makro jest zakup komórki (faza 4) i rozliczenie hurtowe (faza 3) — obie
//! stoją poza tą fazą. Arytmetyka czeka gotowa w `kernel::tax` (WP10.2), a wpięcie
//! stawek z `data/city/tax.ron` należy do M10f razem z resztą domknięcia fazy;
//! do tego czasu hak jest **zerem**, a nie liczbą „na oko" (`R13`).
//!
//! Każda kwota przechodzi przez [`crate::types::MacroLedger::transfer`], czyli
//! przez jedyne wejście, które umie pieniądz **przesunąć** i nie umie go stworzyć.
//! To jest cały mechanizm, którym makro spełnia tolerancję zero z 00 §4 pkt 1.

use magnat_core::Money;
use magnat_economy::kernel;

use crate::state::MacroState;
use crate::types::MacroAccount;

/// Miesiąc ma 30 dób (`K-1`), więc dobowa część listy płac to jedna trzydziesta.
/// Odsetki liczy `kernel::interest_accrual`, który tę samą liczbę zna sam.
const DAYS_PER_MONTH: i64 = 30;

pub fn phase(st: &mut MacroState, p: &super::MacroParams) {
    // Krok może reprezentować więcej niż dobę (`MacroParams::days_per_step`), więc
    // każdy przepływ mnoży się przez tę liczbę. Bez tego osiemdziesiąt lat liczone
    // krokiem sześciodobowym wypłaciłoby jedną szóstą należnych płac — i to jest
    // rodzaj błędu, który nie pokazuje się w żadnym teście z krokiem jednodobowym.
    let dni = i64::from(p.days_per_step.max(1));
    for fi in 0..st.firms.len() {
        let dzielnica = st.firms[fi].district.0;

        // ── wypłaty ──────────────────────────────────────────────────────────
        let dniowka = st.firms[fi].wage_bill.get() * dni / DAYS_PER_MONTH;
        // ── kredyt obrotowy ──────────────────────────────────────────────────
        // Firma, która nie ma z czego zapłacić ludziom, pożycza — a nie zwalnia
        // w tej samej dobie. Pieniądz przychodzi z **reszty świata**, czyli od
        // wierzyciela poza modelem, więc suma w mieście się nie zmienia: makro
        // nie umie wykreować pieniądza i nie ma takiej drogi (`MacroLedger`).
        //
        // Bez tego kanału `MacroFirm.debt` nigdy nie rośnie: `lift()` odczytuje
        // dług jako ujemne saldo rachunku, a świeżo postawione miasto ma wszystkie
        // salda dodatnie. Historia „na sucho" obiecuje „zadłużenie firm" (PRD §4.2
        // Etap 9), a bez pożyczki oddawałaby same zera — i bramka 5 Etapu 10
        // mierzyłaby brak mechanizmu zamiast stanu świata.
        let brak = dniowka.saturating_sub(st.firms[fi].capital.get().max(0));
        if brak > 0 && st.firms[fi].wage_bill.get() > 0 && stac_na_kredyt(st, fi, p) {
            st.firms[fi].capital = Money(st.firms[fi].capital.get().saturating_add(brak));
            st.firms[fi].debt = Money(st.firms[fi].debt.get().saturating_add(brak));
            st.ledger
                .transfer(MacroAccount::RestOfWorld, MacroAccount::Firms, Money(brak));
        }
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
        // Naliczenie **dobowe** z jednym dzieleniem na końcu (`kernel::interest_accrual`,
        // M10a/WP10.2). Wcześniej stało tu `monthly_interest(saldo) / 30`, czyli dwa
        // zaokrąglenia jedno po drugim, oba w tę samą stronę: przy trzydziestu
        // naliczeniach w miesiącu odsetki wychodziły systematycznie niższe od
        // miesięcznych, a błąd nie znosił się, tylko kumulował przez osiemdziesiąt
        // lat dry-runu.
        let odsetki = kernel::interest_accrual(
            st.firms[fi].debt,
            p.debt_rate_bp_month,
            u16::from(p.days_per_step.max(1)),
        )
        .get();
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

/// Czy firma mieści się jeszcze pod sufitem dźwigni.
///
/// To jest jedyny hamulec kredytu w makrze i dlatego stoi osobno: bez niego firma
/// trwale nierentowna pożyczałaby co dobę przez osiemdziesiąt lat, a saldo reszty
/// świata zeszłoby do zakresu, w którym `i64` przestaje być liczbą. Upadłość jest
/// mechanizmem M7 (`K-10`) i makro jej nie powtarza — zamiast niej zostaje firma,
/// która przestaje płacić ludziom.
fn stac_na_kredyt(st: &MacroState, fi: usize, p: &super::MacroParams) -> bool {
    let f = &st.firms[fi];
    let aktywa = f.capital.get().max(0).saturating_add(f.debt.get().max(0));
    if aktywa <= 0 {
        return true;
    }
    f.debt.get().max(0).saturating_mul(1_000) / aktywa < p.max_leverage_permille
}
