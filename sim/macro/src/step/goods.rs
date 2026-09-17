//! Faza 4 — rynek dóbr (M10a §5.7, co krok).
//!
//! **To jest miejsce, w którym mezo i makro różnią się i tylko tutaj.** Ten sam
//! `kernel::purchase_score` ocenia ofertę, ten sam `kernel::softmax_shares` daje
//! rozkład — mezo losuje z niego jeden sklep dla jednego agenta, makro stosuje go
//! jako wagi i dzieli popyt komórki proporcjonalnie (M10a §5.1). Wzór jest jeden,
//! więc dwa poziomy nie mają się z czego rozjechać.
//!
//! Komórka kupuje **u firm swojej dzielnicy**, tak samo jak zatrudnia w swojej
//! dzielnicy (faza 2) i z tego samego powodu: `CommuteMatrix` jest płaska.

use magnat_core::{GoodId, Money, Q};
use magnat_economy::kernel::{self, ScoreInput};

use crate::state::MacroState;
use crate::types::MacroAccount;

use super::MacroParams;

pub fn phase(st: &mut MacroState, p: &MacroParams) {
    // (indeks firmy, towar, cena) w kolejności firm i towarów — wejście softmaxu
    // musi mieć ustaloną kolejność, bo suma `f64` nie jest łączna (00 §2).
    let mut kandydaci: Vec<(usize, GoodId, Money)> = Vec::new();
    let mut oceny: Vec<f64> = Vec::new();

    let klasy = f64::from(st.grain.classes().max(2) - 1);
    for ci in 0..st.cells.len() {
        let dzielnica = st.cells[ci].key.0 .0;
        let budzet = st.cells[ci].cash.get() * i64::from(p.spend_rate_bp) / 10_000;
        if budzet <= 0 {
            continue;
        }
        // Status klasy w skali 0..100 — elita kupuje inaczej niż klasa niższa,
        // i to jest jedyna rzecz, która w makro odróżnia jedną komórkę dzielnicy
        // od drugiej po stronie popytu.
        let status = Q::new((f64::from(st.cells[ci].key.1 .0) / klasy * 100.0) as u8);
        let na_glowe = Money(budzet / i64::from(st.cells[ci].population().max(1)));

        kandydaci.clear();
        oceny.clear();
        for (fi, f) in st.firms.iter().enumerate() {
            if f.district.0 != dzielnica {
                continue;
            }
            for (g, cena) in f.price.iter() {
                if cena.get() <= 0 || f.stock.get(g) <= 0 {
                    continue;
                }
                kandydaci.push((fi, g, cena));
                oceny.push(kernel::purchase_score(
                    &ScoreInput {
                        price_total: cena,
                        budget_ref: na_glowe.max(Money(1)),
                        // Dojazd w obrębie dzielnicy jest w makro jednakowy dla
                        // wszystkich sklepów — różnicowałaby go dopiero macierz M4.
                        travel_cost: Money(0),
                        quality: Q::new(50),
                        status,
                        loyalty: 0.0,
                        novelty: 0.0,
                        brand: 0.0,
                    },
                    &p.weights,
                    0.0,
                ));
            }
        }
        if kandydaci.is_empty() {
            continue;
        }

        let udzialy = kernel::softmax_shares(&oceny, p.temperature);
        let mut wydano: i64 = 0;
        for (k, (fi, g, cena)) in kandydaci.iter().enumerate() {
            let kwota = (budzet as f64 * udzialy[k]) as i64;
            if kwota <= 0 {
                continue;
            }
            let chce = kwota / cena.get().max(1);
            let ile = chce.min(st.firms[*fi].stock.get(*g));
            if ile <= 0 {
                continue;
            }
            let zaplata = ile.saturating_mul(cena.get());
            st.firms[*fi].stock.add(*g, -ile);
            st.firms[*fi].capital = Money(st.firms[*fi].capital.get().saturating_add(zaplata));
            st.cells[ci].demand.add(*g, ile);
            wydano = wydano.saturating_add(zaplata);
        }
        if wydano > 0 {
            st.cells[ci].cash = Money(st.cells[ci].cash.get().saturating_sub(wydano));
            st.ledger
                .transfer(MacroAccount::Households, MacroAccount::Firms, Money(wydano));
        }
    }
}
