//! Faza 5 — ceny (M10a §5.7, co krok).
//!
//! `kernel::next_price` per firma per towar — **ta sama funkcja**, którą sterownik
//! ceny woła w mezo (§6.3). Makro podaje jej te same wejścia, tylko policzone
//! z agregatu: pokrycie zapasu z własnego stanu, cenę konkurencji jako medianę
//! dzielnicy. Żadnego mnożnika po drodze — mnożnik byłby drugą regułą cenową.

use magnat_core::Money;
use magnat_economy::kernel::{self, PriceInput};

use crate::state::MacroState;

use super::MacroParams;

pub fn phase(st: &mut MacroState, p: &MacroParams) {
    // Przecena nie wypada codziennie, i to jest zbliżenie do mezo, a nie skrót:
    // `data/economy/shop.ron` daje każdej firmie czujność 1–7 dób, a sklep patrzy
    // na ceny konkurencji z takim właśnie opóźnieniem (M5c). Model makro nie ma
    // osobowości firm, więc bierze **jedną kadencję dla wszystkich** — środek
    // tamtego przedziału. Przecena codzienna, którą M7f tu zostawił, była w istocie
    // modelem agresywniejszym od mezo i różnicę widać było jako szybszą zbieżność
    // cen w „co jeśli" niż w przebiegu, który ten „co jeśli" przewidywał.
    if st.day > 0
        && !super::przekroczono(
            st.day,
            u32::from(p.days_per_step.max(1)),
            u32::from(p.reprice_every_days.max(1)),
        )
    {
        return;
    }
    // Mediana ceny w dzielnicy per towar — obraz konkurencji. Liczona przed
    // przeceną, bo wszyscy patrzą na **wczorajsze** ceny (w mezo robi to tablica
    // publiczna z opóźnieniem 1–7 dób).
    let mut rynek: Vec<(u16, u16, i64)> = Vec::new();
    for f in &st.firms {
        for (g, cena) in f.price.iter() {
            if cena.get() > 0 {
                rynek.push((f.district.0, g.0, cena.get()));
            }
        }
    }
    rynek.sort_unstable();

    for f in &mut st.firms {
        let towary: Vec<magnat_core::GoodId> = f.price.iter().map(|(g, _)| g).collect();
        // Dobowy obrót na towar — mianownik pokrycia zapasu.
        let dzienny = (f.capacity_daily.get() / magnat_firms::hr::productivity::FULL_TIME)
            .max(1)
            .saturating_mul(p.nominal_per_slot_hour)
            * 24
            / towary.len().max(1) as i64;
        for g in towary {
            let koszt = f.cost.get(g).unwrap_or(Money::ZERO);
            if koszt.get() <= 0 {
                continue;
            }
            let cel = i64::from(p.target_cover_days).max(1);
            let pokrycie = f.stock.get(g) / dzienny.max(1);
            let stock_bp = i32::try_from(pokrycie.saturating_mul(10_000) / cel)
                .unwrap_or(i32::MAX)
                .clamp(0, 100_000);
            let konkurencja = mediana(&rynek, f.district.0, g.0);
            let nowa = kernel::next_price(PriceInput {
                unit_cost: koszt,
                target_margin_bp: p.target_margin_bp,
                min_margin_bp: p.margin_bp.0,
                max_margin_bp: p.margin_bp.1,
                stock_bp_of_target: stock_bp,
                k_stock: p.k_stock,
                competitor_net: konkurencja,
                k_comp: p.k_comp,
                adj_elast_bp: 0,
                adj_spoil_bp: 0,
            });
            f.price.set(g, nowa);
        }
    }
}

/// Mediana ceny towaru w dzielnicy. `None`, gdy firma jest w niej jedyna — brak
/// konkurenta to **brak liczby**, a nie zero, bo zero przyciągnęłoby cenę do dna.
fn mediana(rynek: &[(u16, u16, i64)], district: u16, good: u16) -> Option<Money> {
    let start = rynek.partition_point(|(d, g, _)| (*d, *g) < (district, good));
    let end = rynek.partition_point(|(d, g, _)| (*d, *g) <= (district, good));
    match end - start {
        0 | 1 => None,
        n => Some(Money(rynek[start + n / 2].2)),
    }
}
