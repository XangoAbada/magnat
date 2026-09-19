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
    // Zbiór ofert jest **wspólny dla wszystkich klas dzielnicy** i budowany raz na
    // dzielnicę, a nie raz na komórkę. `cells` jest posortowane po (dzielnica, klasa),
    // więc wystarczy pamiętać poprzednią. Poza oszczędnością (sześć przebiegów po
    // trzech tysiącach firm zamiast jednego) jest w tym treść: wszystkie klasy
    // dzielnicy widzą **tę samą półkę na początek doby**. Klasa, która kupuje jako
    // druga, zastanie mniej towaru — i to jest jedyna kolejność, jaka tu działa,
    // bo równoczesność w kroku dobowym nie istnieje.
    let mut zbudowana_dla: Option<u16> = None;
    for ci in 0..st.cells.len() {
        let dzielnica = st.cells[ci].key.0 .0;
        // Wstrząs w skali miasta (faza 8) przesuwa skłonność do wydawania, a nie
        // zasobność: recesja nie zabiera gospodarstwu pieniędzy, tylko każe mu
        // je trzymać. Zero korekty znaczy dobę bez wstrząsu i wtedy rachunek jest
        // dokładnie ten sam co przed M10a.
        // Podstawą jest **majątek płynny**: gotówka i depozyt. Gospodarstwo płaci
        // kartą tak samo jak gotówką, a w makrze rozróżnienie tych dwóch zmieniałoby
        // wyłącznie to, ile miasto kupuje — na świecie, w którym `Household.cash`
        // bywa zerem, sprowadzałoby popyt do zera.
        let plynne = Money(
            st.cells[ci]
                .cash
                .get()
                .saturating_add(st.cells[ci].deposits.get()),
        );
        let podstawa = kernel::apply_bp(plynne, p.spend_rate_bp);
        let budzet = podstawa
            .get()
            .saturating_add(kernel::apply_bp(podstawa, st.demand_shift_bp()).get())
            .saturating_mul(i64::from(p.days_per_step.max(1)));
        if budzet <= 0 {
            continue;
        }
        // Status klasy w skali 0..100 — elita kupuje inaczej niż klasa niższa,
        // i to jest jedyna rzecz, która w makro odróżnia jedną komórkę dzielnicy
        // od drugiej po stronie popytu.
        let status = Q::new((f64::from(st.cells[ci].key.1 .0) / klasy * 100.0) as u8);
        let na_glowe = Money(budzet / i64::from(st.cells[ci].population().max(1)));

        if zbudowana_dla != Some(dzielnica) {
            zbierz_kandydatow(st, dzielnica, p.candidates_per_good, &mut kandydaci);
            zbudowana_dla = Some(dzielnica);
        }
        if kandydaci.is_empty() {
            continue;
        }

        // Oceny liczą się per komórka, bo zależą od statusu klasy i od budżetu
        // na głowę — i to jest jedyna część tej pętli, której nie da się podzielić
        // między klasy jednej dzielnicy.
        oceny.clear();
        oceny.extend(kandydaci.iter().map(|(_, _, cena)| {
            kernel::purchase_score(
                &ScoreInput {
                    price_total: *cena,
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
            )
        }));

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
            // Płaci się najpierw gotówką, a resztę z depozytu — ta sama kolejność,
            // którą ma gospodarstwo w mezo (`Market::pay`, M5d).
            let z_gotowki = wydano.min(st.cells[ci].cash.get().max(0));
            st.cells[ci].cash = Money(st.cells[ci].cash.get() - z_gotowki);
            st.cells[ci].deposits = Money(
                st.cells[ci]
                    .deposits
                    .get()
                    .saturating_sub(wydano - z_gotowki),
            );
            st.ledger
                .transfer(MacroAccount::Households, MacroAccount::Firms, Money(wydano));
        }
    }
}

/// Oferty dzielnicy ograniczone do `na_towar` najtańszych na każdy towar.
///
/// **To nie jest optymalizacja, tylko odwzorowanie mezo.** Mieszkaniec w M5 nie
/// ocenia wszystkich ofert w promieniu — pyta o `k_min..k_max` najbliższych
/// (`data/economy/choice.ron`) i wybiera spośród nich. Komórka robi to samo, tylko
/// kryterium bliskości nie ma: w obrębie dzielnicy dojazd jest jednakowy (macierz
/// M4 jest płaska), więc zawężenie idzie po cenie. Bez tego ograniczenia komórka
/// oceniałaby każdą półkę w dzielnicy, czyli wiedziałaby o rynku więcej niż
/// mieszkaniec, którego zastępuje — a to jest rozjazd modeli, nie oszczędność.
///
/// Sufit jest zarazem tym, co trzyma krok w budżecie z §7.5: rachunek „240 komórek
/// × ~60 towarów" z §5.7 daje 10⁵ operacji na krok **tylko** przy takim zawężeniu.
fn zbierz_kandydatow(
    st: &MacroState,
    dzielnica: u16,
    na_towar: u8,
    out: &mut Vec<(usize, GoodId, Money)>,
) {
    out.clear();
    for (fi, f) in st.firms.iter().enumerate() {
        if f.district.0 != dzielnica {
            continue;
        }
        for (g, cena) in f.price.iter() {
            if cena.get() > 0 && f.stock.get(g) > 0 {
                out.push((fi, g, cena));
            }
        }
    }
    // Po towarze, potem po cenie, a przy równej cenie po indeksie firmy: remis
    // rozstrzyga kolejność `FirmId`, bo `firms` jest po niej posortowane.
    out.sort_unstable_by_key(|(fi, g, cena)| (g.0, cena.get(), *fi));
    let limit = usize::from(na_towar.max(1));
    let mut poprzedni: Option<u16> = None;
    let mut ile = 0usize;
    out.retain(|(_, g, _)| {
        if poprzedni != Some(g.0) {
            poprzedni = Some(g.0);
            ile = 0;
        }
        ile += 1;
        ile <= limit
    });
}
