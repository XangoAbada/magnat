//! Faza 7 — relacje dostawców (M10a §5.7, co krok).
//!
//! Wynik tej fazy jest jednym z trzech, po które robi się historię „na sucho":
//! PRD §4.2 Etap 9 obiecuje „relacje między firmami — stali dostawcy", a plan
//! trzyma je w `MacroFirm.suppliers`. Faza wybiera dostawcę i **przykleja go**:
//! zmiana następuje dopiero wtedy, gdy nowy jest wyraźnie tańszy.
//!
//! Przyklejenie jest tu istotą, nie ozdobą. Bez histerezy „stały dostawca" byłby
//! nazwą dla wyniku porównania z ostatniej doby — czyli dla czegoś, co zmienia się
//! codziennie i nie jest żadną relacją. Próg zmiany mówi, ile trzeba zaoszczędzić,
//! żeby zerwać istniejącą współpracę, i to on odróżnia sieć dostaw od cennika.
//!
//! # Czego ta faza jeszcze nie robi
//!
//! Wybrany dostawca **nie dostaje zamówienia** — zatowarowanie w fazie 3 dalej idzie
//! z importu (konto reszty świata). Zamknięcie tej pętli to `SupplierRelation`
//! z WP10.13, czyli podfaza M10e; tutaj powstaje strona, która wie **kto z kim**,
//! bo bez niej tamta nie ma z czego wystartować, a dry-run nie ma czego wypisać
//! w raporcie.

use magnat_core::{FirmId, GoodId, Money};
use magnat_economy::kernel;

use crate::state::MacroState;

use super::MacroParams;

/// Przegląd dostawców wypada raz na miesiąc (`K-1`: 30 dób).
///
/// Nie co dobę, i to nie jest oszczędność czasu wzięta z sufitu: relacja z progiem
/// zmiany (`supplier_switch_bp`) przeglądana codziennie i tak zmienia się rzadko,
/// więc dwadzieścia dziewięć przebiegów na trzydzieści **nic nie zmienia** i tylko
/// przepisuje tę samą listę. Miesiąc jest zarazem rytmem, w którym negocjuje się
/// warunki dostawy w mezo (`SupplyContract`, M6b).
const DOB_MIEDZY_PRZEGLADAMI: u32 = 30;

pub fn phase(st: &mut MacroState, p: &MacroParams) {
    if st.day > 0
        && !super::przekroczono(
            st.day,
            u32::from(p.days_per_step.max(1)),
            DOB_MIEDZY_PRZEGLADAMI,
        )
    {
        return;
    }
    // Oferty per dzielnica: (dzielnica, towar, cena, firma). Zbierane raz na przegląd
    // i sortowane, bo wybór ma być funkcją zbioru, a nie kolejności iteracji.
    let mut oferty: Vec<Oferta> = Vec::new();
    for f in &st.firms {
        for (g, cena) in f.price.iter() {
            if cena.get() > 0 && f.stock.get(g) > 0 {
                oferty.push(Oferta {
                    district: f.district.0,
                    good: g.0,
                    cena: cena.get(),
                    firma: f.id,
                });
            }
        }
    }
    oferty.sort_unstable_by_key(|o| (o.district, o.good, o.cena, o.firma.0.to_bits()));

    for fi in 0..st.firms.len() {
        let dzielnica = st.firms[fi].district.0;
        let ja = st.firms[fi].id.0.to_bits();
        // Kupuje się to, czym się handluje: towar, dla którego firma prowadzi koszt
        // własny, jest towarem, który skądś bierze.
        let potrzeby: Vec<GoodId> = st.firms[fi].cost.iter().map(|(g, _)| g).collect();
        for g in potrzeby {
            let Some((cena, kto)) = najtanszy(&oferty, dzielnica, g.0, ja) else {
                continue;
            };
            let obecny = st.firms[fi]
                .suppliers
                .iter()
                .find(|(gg, _)| *gg == g)
                .map(|(_, f)| *f);
            match obecny {
                Some(stary) if stary.0.to_bits() != kto.0.to_bits() => {
                    let cena_starego = cena_u(&oferty, dzielnica, g.0, stary);
                    if !warto_zmienic(cena_starego, cena, p.supplier_switch_bp) {
                        continue;
                    }
                    ustaw(st, fi, g, kto);
                }
                Some(_) => {}
                None => ustaw(st, fi, g, kto),
            }
        }
    }
}

/// Jedna stojąca oferta w dzielnicy. Struktura, a nie krotka: pięć pól bez nazw
/// czyta się w filtrach gorzej, niż kosztuje ich napisanie.
struct Oferta {
    district: u16,
    good: u16,
    cena: i64,
    firma: FirmId,
}

/// Wycinek listy ofert dotyczący jednej pary (dzielnica, towar).
///
/// Wyszukiwanie binarne, nie filtr po całej liście, i to nie jest mikrooptymalizacja:
/// metropolia ma ~3 tys. firm po kilkanaście towarów, czyli ~48 tys. ofert, a pytanie
/// zadaje się raz na firmę i towar. Przegląd liniowy dałby 2·10⁹ porównań **na dobę**
/// i sam zjadłby cały budżet kroku z §7.5.
fn wycinek(oferty: &[Oferta], dzielnica: u16, good: u16) -> &[Oferta] {
    let od = oferty.partition_point(|o| (o.district, o.good) < (dzielnica, good));
    let do_ = oferty.partition_point(|o| (o.district, o.good) <= (dzielnica, good));
    &oferty[od..do_]
}

/// Najtańsza oferta towaru w dzielnicy, pomijając samą firmę. Remis rozstrzyga
/// niższy identyfikator firmy — wycinek jest posortowany po cenie, więc pierwsza
/// trafiona jest zarazem najtańsza i najniższa w kolejności.
fn najtanszy(oferty: &[Oferta], dzielnica: u16, good: u16, ja: u64) -> Option<(i64, FirmId)> {
    wycinek(oferty, dzielnica, good)
        .iter()
        .find(|o| o.firma.0.to_bits() != ja)
        .map(|o| (o.cena, o.firma))
}

/// Cena, po której sprzedaje obecny dostawca; `None`, gdy przestał oferować towar.
fn cena_u(oferty: &[Oferta], dzielnica: u16, good: u16, firma: FirmId) -> Option<i64> {
    let bits = firma.0.to_bits();
    wycinek(oferty, dzielnica, good)
        .iter()
        .find(|o| o.firma.0.to_bits() == bits)
        .map(|o| o.cena)
}

/// Czy nowy dostawca jest tańszy o więcej niż próg zmiany.
///
/// Dostawca, który zniknął z rynku (`None`), zawsze przegrywa — relacja bez oferty
/// nie jest relacją.
fn warto_zmienic(stary: Option<i64>, nowy: i64, prog_bp: i32) -> bool {
    let Some(stary) = stary else {
        return true;
    };
    let zapas = kernel::apply_bp(Money(stary), prog_bp).get();
    nowy < stary.saturating_sub(zapas)
}

fn ustaw(st: &mut MacroState, fi: usize, g: GoodId, kto: FirmId) {
    let f = &mut st.firms[fi];
    match f.suppliers.iter().position(|(gg, _)| *gg == g) {
        Some(i) => f.suppliers[i].1 = kto,
        None => {
            if f.suppliers.len() < f.suppliers.inline_size() {
                f.suppliers.push((g, kto));
                f.suppliers.sort_unstable_by_key(|(gg, _)| gg.0);
            }
        }
    }
}
