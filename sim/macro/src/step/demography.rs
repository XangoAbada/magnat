//! Faza 1 — demografia (M10a §5.7, raz na rok gry).
//!
//! Dwie rzeczy i ani jednej więcej: **starzenie kohort** i **migracja między
//! komórkami**. Obie są przesunięciem liczb, nie regułą ekonomiczną — pieniądz
//! jedzie z człowiekiem i dojeżdża co do grosza.
//!
//! # Populacja jest zamknięta i to jest decyzja, nie brak
//!
//! Zbiór `citizens` komórki **nie zmienia się** w kroku makro: ludzie się
//! przeprowadzają, ale nikt nie powstaje i nikt nie znika. Powód jest strukturalny
//! i wart zapisania, bo plan mówił inaczej (§5.7, etap D0: „miasto mniejsze").
//!
//! `CitizenSeed.birth_index` jest **indeksem encji ECS** — tak wypełnia go `lift()`
//! i tak czyta go `lower()`. Nowy mieszkaniec w makrze musiałby więc dostać indeks
//! encji, której jeszcze nie ma, a zmarły — zostawić encję do usunięcia. Jedno
//! i drugie znaczy, że `lower()` przestaje być naniesieniem stanu i staje się
//! **drugim generatorem populacji** obok Etapu 8 — dwie drogi do tych samych ludzi,
//! czyli dokładnie to, przed czym broni `K-8`.
//!
//! Co z tego wynika i czego to kosztuje: rok gry przesuwa **strukturę wieku**
//! (kohorty, obciążenie demograficzne, siłę roboczą), a nie liczbę mieszkańców.
//! Odejścia z najstarszej kohorty wracają do kohorty zerowej, więc piramida dąży
//! do stanu stacjonarnego zamiast się wyludniać. Wzrost i spadek populacji
//! w historii „na sucho" wymaga tworzenia encji i należy do M12c razem z trybem
//! 50× (`FastForwardConfig::keep_identities`), gdzie churn encji i tak musi powstać.
//!
//! # Czego tu nie ma
//!
//! Losowania. Starzenie jest podziałem całkowitoliczbowym, a kierunek migracji —
//! porównaniem dwóch liczb. `StreamId::MacroStep` zostaje **zarezerwowany
//! i niezajęty**: strumień „na losowanie demografii" opisywałby mechanizm, którego
//! ta faza świadomie nie ma (ten sam przypadek co `StreamId::EventHazard` w `K-63`).

use magnat_core::{split_proportional, Money};

use crate::state::MacroState;
use crate::types::CitizenSeed;

/// Kohorta `age_hist` obejmuje pięć roczników, więc rok przesuwa jedną piątą.
const LAT_W_KOHORCIE: u32 = 5;

/// Ostatnia kohorta (85+) — z niej odchodzą ludzie i do niej nikt nie awansuje.
const OSTATNIA: usize = 17;

/// Rok gry ma 360 dób (`K-1`).
const DOB_W_ROKU: u32 = 360;

/// Czy dziś wypada rocznica. Faza liczy się raz na rok, bo kohorta jest
/// pięcioletnia: liczenie jej co dobę dałoby przesunięcie 1/1800 rocznika,
/// czyli zero po zaokrągleniu, i piramida stałaby w miejscu.
#[must_use]
pub fn rocznica(day: u32, dni_kroku: u32) -> bool {
    day > 0 && super::przekroczono(day, dni_kroku, DOB_W_ROKU)
}

pub fn phase(st: &mut MacroState, p: &super::MacroParams) {
    if !rocznica(st.day, u32::from(p.days_per_step.max(1))) {
        return;
    }
    for cell in &mut st.cells {
        starzenie(&mut cell.age_hist, p.elder_mortality_permille);
    }
    migracja(st, p);
}

/// Przesuwa jedną piątą każdej kohorty o oczko w górę, a z najstarszej odprowadza
/// zgony — których liczba wraca do kohorty zerowej, bo populacja jest zamknięta.
///
/// Iteracja idzie **od góry**, żeby ludzie nie przeskakiwali dwóch kohort w jednym
/// roku: gdyby zacząć od dołu, awansowani z kohorty 0 zostaliby w tym samym
/// przebiegu awansowani jeszcze raz z kohorty 1.
fn starzenie(hist: &mut [u32; 18], mortalnosc_permille: u16) {
    let zgony = u64::from(hist[OSTATNIA]) * u64::from(mortalnosc_permille) / 1_000;
    let zgony = u32::try_from(zgony).unwrap_or(0).min(hist[OSTATNIA]);
    hist[OSTATNIA] -= zgony;

    for i in (0..OSTATNIA).rev() {
        let awans = hist[i] / LAT_W_KOHORCIE;
        hist[i] -= awans;
        hist[i + 1] = hist[i + 1].saturating_add(awans);
    }
    // Urodzenia domykają populację: tyle, ile odeszło.
    hist[0] = hist[0].saturating_add(zgony);
}

/// Przeprowadzki między komórkami **tej samej klasy**.
///
/// Kierunek jest funkcją dwóch liczb, które komórka już ma: średniego zaspokojenia
/// potrzeb i bezrobocia. Komórka o najgorszym wyniku oddaje ułamek mieszkańców
/// komórce o najlepszym — i oddaje razem z nimi **ich część pieniądza**, liczoną
/// jako udział w gotówce, depozytach i długu. To jest cały mechanizm „gentryfikacji
/// Starego Portu" z §5.7: dzielnica, w której nie ma pracy i nie ma czego kupić,
/// pustoszeje, a jej pieniądz wyprowadza się razem z ludźmi.
///
/// Klasa się nie zmienia, bo awans społeczny jest zmianą `Vitals.status`, czyli
/// wielkością mezo — makro nie ma jak go policzyć i nie udaje, że ma.
fn migracja(st: &mut MacroState, p: &super::MacroParams) {
    let klasy: Vec<u8> = {
        let mut k: Vec<u8> = st.cells.iter().map(|c| c.key.1 .0).collect();
        k.sort_unstable();
        k.dedup();
        k
    };
    for klasa in klasy {
        let indeksy: Vec<usize> = st
            .cells
            .iter()
            .enumerate()
            .filter(|(_, c)| c.key.1 .0 == klasa && c.population() > 0)
            .map(|(i, _)| i)
            .collect();
        if indeksy.len() < 2 {
            continue;
        }
        let oceny: Vec<(i64, usize)> = indeksy.iter().map(|i| (atrakcyjnosc(st, *i), *i)).collect();
        let najgorsza = oceny.iter().min().copied();
        let najlepsza = oceny.iter().max().copied();
        let (Some((ocena_zla, z)), Some((ocena_dobra, do_))) = (najgorsza, najlepsza) else {
            continue;
        };
        if z == do_ || ocena_dobra - ocena_zla < i64::from(p.migration_gap) {
            continue;
        }
        let ilu = u32::try_from(
            u64::from(st.cells[z].population()) * u64::from(p.migration_permille) / 1_000,
        )
        .unwrap_or(0)
        .min(st.cells[z].population().saturating_sub(1));
        if ilu == 0 {
            continue;
        }
        przeprowadz(st, z, do_, ilu);
    }
}

/// Im wyżej, tym lepiej się żyje: średnie zaspokojenie potrzeb minus bezrobocie,
/// obie w skali promilowej. Liczba jest **porządkowa**, nie fizyczna — służy
/// wyłącznie do wskazania najlepszej i najgorszej komórki w klasie.
fn atrakcyjnosc(st: &MacroState, i: usize) -> i64 {
    let c = &st.cells[i];
    let potrzeby: u64 = c.need_sat.iter().map(|q| u64::from(q.get())).sum();
    let srednia = potrzeby * 10 / (c.need_sat.len() as u64);
    let sila = u64::from(c.labour_force()).max(1);
    let bezrobocie = u64::from(c.unemployed) * 1_000 / sila;
    i64::try_from(srednia).unwrap_or(0) - i64::try_from(bezrobocie).unwrap_or(0)
}

/// Przenosi `ilu` mieszkańców z końca listy komórki źródłowej do docelowej razem
/// z ich udziałem w pieniądzu. Podział idzie przez `split_proportional`, więc suma
/// obu komórek po przeprowadzce jest równa sumie przed nią **co do grosza**.
fn przeprowadz(st: &mut MacroState, z: usize, do_: usize, ilu: u32) {
    let n = st.cells[z].population();
    let wagi = [u64::from(n - ilu), u64::from(ilu)];
    let (zostaje_cash, jedzie_cash) = podziel(st.cells[z].cash, &wagi);
    let (zostaje_dep, jedzie_dep) = podziel(st.cells[z].deposits, &wagi);
    let (zostaje_debt, jedzie_debt) = podziel(st.cells[z].debt, &wagi);

    // Wyjeżdżają ostatni w kolejności `birth_index` — kolejność jest ustalona,
    // więc wybór jest deterministyczny i nie wymaga losowania.
    let mut przenoszeni: Vec<CitizenSeed> = st.cells[z]
        .citizens
        .split_off((n - ilu) as usize)
        .into_iter()
        .collect();
    let cel = u16::try_from(do_).unwrap_or(u16::MAX);
    for c in &mut przenoszeni {
        c.cell = cel;
    }

    // Struktura wieku jedzie proporcjonalnie — inaczej wyjeżdżaliby sami
    // osiemdziesięciolatkowie i piramida wieku obu komórek rozjechałaby się
    // po kilku latach bez żadnego powodu.
    let mut przenoszone_kohorty = [0u32; 18];
    for (k, slot) in przenoszone_kohorty.iter_mut().enumerate() {
        let w = u64::from(st.cells[z].age_hist[k]) * u64::from(ilu) / u64::from(n.max(1));
        *slot = u32::try_from(w).unwrap_or(0).min(st.cells[z].age_hist[k]);
    }
    let przenoszona_sila = przenies_sile(st, z, ilu, n);

    {
        let zrodlo = &mut st.cells[z];
        zrodlo.cash = zostaje_cash;
        zrodlo.deposits = zostaje_dep;
        zrodlo.debt = zostaje_debt;
        for (k, v) in przenoszone_kohorty.iter().enumerate() {
            zrodlo.age_hist[k] -= v;
        }
    }
    {
        let cel = &mut st.cells[do_];
        cel.citizens.append(&mut przenoszeni);
        cel.citizens.sort_unstable_by_key(|c| c.birth_index);
        cel.cash = Money(cel.cash.get().saturating_add(jedzie_cash.get()));
        cel.deposits = Money(cel.deposits.get().saturating_add(jedzie_dep.get()));
        cel.debt = Money(cel.debt.get().saturating_add(jedzie_debt.get()));
        for (k, v) in przenoszone_kohorty.iter().enumerate() {
            cel.age_hist[k] = cel.age_hist[k].saturating_add(*v);
        }
        cel.employed = cel.employed.saturating_add(przenoszona_sila.0);
        cel.unemployed = cel.unemployed.saturating_add(przenoszona_sila.1);
    }
}

/// Zabiera z komórki źródłowej proporcjonalną część zatrudnionych i bezrobotnych.
/// Zwraca to, co ma dojechać — liczba osób ma się zgadzać co do osoby, tak samo
/// jak kwota co do grosza.
fn przenies_sile(st: &mut MacroState, z: usize, ilu: u32, n: u32) -> (u32, u32) {
    let c = &mut st.cells[z];
    let zatr = u32::try_from(u64::from(c.employed) * u64::from(ilu) / u64::from(n.max(1)))
        .unwrap_or(0)
        .min(c.employed);
    let bez = u32::try_from(u64::from(c.unemployed) * u64::from(ilu) / u64::from(n.max(1)))
        .unwrap_or(0)
        .min(c.unemployed);
    c.employed -= zatr;
    c.unemployed -= bez;
    (zatr, bez)
}

fn podziel(kwota: Money, wagi: &[u64; 2]) -> (Money, Money) {
    let v = split_proportional(kwota, wagi);
    (v[0], v[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starzenie_nie_gubi_ludzi() {
        let mut hist = [1_000u32; 18];
        let przed: u32 = hist.iter().sum();
        for _ in 0..40 {
            starzenie(&mut hist, 120);
        }
        assert_eq!(hist.iter().sum::<u32>(), przed, "populacja jest zamknięta");
    }

    #[test]
    fn starzenie_przesuwa_piramide_w_gore() {
        // Miasto samych dwudziestolatków po dwudziestu latach ma ludzi w kohortach
        // czterdziestolatków — a gdyby iteracja szła od dołu, mieliby po sto lat.
        let mut hist = [0u32; 18];
        hist[4] = 10_000;
        for _ in 0..20 {
            starzenie(&mut hist, 0);
        }
        assert!(hist[4] < 10_000, "kohorta się opróżnia");
        assert!(hist[7] > 0 && hist[9] > 0, "ludzie doszli do kohort 35–49");
        assert_eq!(hist[17], 0, "nikt nie przeskoczył całej piramidy");
    }
}
