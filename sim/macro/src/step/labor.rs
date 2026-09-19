//! Faza 2 — rynek pracy (M10a §5.7, co krok).
//!
//! Podaż roli w komórce zestawiona z popytem firm, stawka z `kernel::wage_bid`,
//! zmiana przycięta do ±2 %/dobę. Firma zatrudnia z **puli swojej dzielnicy**:
//! `CommuteMatrix` jest dziś płaska (`ponytail:` w `types.rs`), więc ograniczenie
//! do dzielnicy jest jedyną rzeczą, która w ogóle odróżnia dzielnicę od dzielnicy.
//! Kiedy M4 wypełni macierz, pula rozszerzy się o dzielnice w zasięgu dojazdu —
//! i to jest cała zmiana, jakiej to będzie wymagało.
//!
//! # Co ten zakaz kosztuje, zmierzone w M10a
//!
//! Miasto ma dzielnice przemysłowe bez mieszkań i mieszkaniowe bez zakładów, więc
//! przy rekrutacji zamkniętej w granicach dzielnicy **jedna trzecia firm nie dostaje
//! ani jednego pracownika przez trzydzieści lat** historii „na sucho". Te firmy nic
//! nie produkują, rynek zjada zapas z Etapu 7 i bramka 1 Etapu 10 (nierównowaga)
//! wychodzi 1000 ‰.
//!
//! Otwarcie puli na całe miasto **zostało spróbowane i cofnięte**: bezrobocie schodzi
//! wtedy do zera, firmy o niższym `FirmId` zabierają całą pulę, a odsetek firm bez
//! obsady rośnie z 291 ‰ do 342 ‰. Problemem nie jest granica dzielnicy, tylko
//! **brak kosztu dojazdu**: bez niego rekrutacja jest albo zakazana, albo darmowa,
//! a żadne z tych dwojga nie jest rynkiem pracy. Rozstrzyga to wypełnienie macierzy
//! przez M4 — wtedy „najpierw swoi" stanie się preferencją kosztową, a nie zakazem.

use magnat_core::Money;
use magnat_economy::kernel;
use magnat_firms::hr::productivity::FULL_TIME;

use crate::state::MacroState;

use super::{MacroParams, WAGE_STEP_CAP_BP};

pub fn phase(st: &mut MacroState, p: &MacroParams) {
    let dzielnice = st.commute.districts();
    // Pula bezrobotnych per dzielnica — liczona raz, zużywana po kolei przez firmy
    // w kolejności `FirmId`. Kolejność jest deterministyczna i to ona rozstrzyga,
    // kto pierwszy dostanie ludzi przy niedoborze.
    let mut pula: Vec<u32> = vec![0; usize::from(dzielnice)];
    for c in &st.cells {
        if let Some(slot) = pula.get_mut(usize::from(c.key.0 .0)) {
            *slot = slot.saturating_add(c.unemployed);
        }
    }

    for f in &mut st.firms {
        // Firma bez kapitału nie licytuje i nie zatrudnia. To nie jest upadłość —
        // tę prowadzi mezo (`K-10`) — tylko zatrzymanie ekspansji, które w horyzoncie
        // kwartału odróżnia wariant wykonalny od niewykonalnego.
        let wyplacalna = f.capital.get() >= 0;
        let etaty = u32::try_from((f.capacity_daily.get() / FULL_TIME).max(0)).unwrap_or(0);
        if etaty == 0 {
            continue;
        }
        let wakaty = etaty.saturating_sub(f.employees);
        let d = usize::from(f.district.0);

        // Stawka jednostkowa: dzisiejszy rachunek podzielony przez obsadę.
        let stawka = if f.employees == 0 {
            p.default_wage_month
        } else {
            Money(f.wage_bill.get() / i64::from(f.employees))
        };

        if wakaty > 0 && wyplacalna {
            // Indeks niedoboru w promilach zamówionych etatów — to samo znaczenie
            // co `shortage_index` rynku pracy mezo: „ile z tego, czego chcę, stoi puste".
            let niedobor =
                u16::try_from(u64::from(wakaty) * 1_000 / u64::from(etaty)).unwrap_or(1_000);
            let sufit = Money(stawka.get() * 3);
            let bid = kernel::wage_bid(
                stawka,
                (Money(stawka.get() / 2), sufit),
                niedobor,
                p.aggression,
                // Zapas marży: makro nie zna rachunku wyniku zakładu, więc podaje
                // widełki z danych. To jest ten sam hak zerowy, którym M7b stał
                // do M7e — i tu zostaje, bo `SitePnlMonth` jest wielkością mezo.
                p.margin_bp.1,
                &p.wage,
            );
            let cel = bid.wage.get();
            let cap = kernel::apply_bp(stawka, WAGE_STEP_CAP_BP).get();
            let nowa = cel.min(stawka.get().saturating_add(cap.max(1)));

            let mozliwe =
                u32::from(p.hire_speed_permille) * u32::from(p.days_per_step.max(1)) * etaty
                    / 1_000;
            let chetni = wakaty.min(mozliwe.max(1)).min(pula[d]);
            if chetni > 0 {
                pula[d] -= chetni;
                f.employees = f.employees.saturating_add(chetni);
            }
            f.wage_bill = Money(nowa.saturating_mul(i64::from(f.employees)));
        } else if f.employees > etaty {
            // Redukcja do zamówionych etatów. Zwolnieni wracają do puli **tej samej
            // doby** — inaczej zamknięcie zakładu w wariancie „co jeśli" wyglądałoby
            // jak zniknięcie ludzi z miasta.
            let nadmiar = f.employees - etaty;
            f.employees = etaty;
            pula[d] = pula[d].saturating_add(nadmiar);
            f.wage_bill = Money(stawka.get().saturating_mul(i64::from(f.employees)));
        }
    }

    rozlej_zatrudnienie(st, &pula);
}

/// Przepisuje pulę z powrotem na komórki, w kolejności klucza komórki, i domyka
/// różnicę na `employed`. Bez tego bezrobocie liczyłoby się z liczby, która
/// zmieniła się tylko po stronie firm.
fn rozlej_zatrudnienie(st: &mut MacroState, pula: &[u32]) {
    for (d, wolni) in pula.iter().enumerate() {
        let mut zostalo = *wolni;
        let indeksy: Vec<usize> = st
            .cells
            .iter()
            .enumerate()
            .filter(|(_, c)| usize::from(c.key.0 .0) == d)
            .map(|(i, _)| i)
            .collect();
        if indeksy.is_empty() {
            continue;
        }
        // Siła robocza dzielnicy się nie zmienia — zmienia się jej podział.
        let sila: u32 = indeksy
            .iter()
            .map(|i| st.cells[*i].labour_force())
            .fold(0, u32::saturating_add);
        for i in &indeksy {
            let c = &mut st.cells[*i];
            // Siłę roboczą komórki trzeba **zapamiętać przed zapisem**: to ona jest
            // mianownikiem i ona jest tym, co się nie zmienia. Odczyt po zmianie
            // `unemployed` liczyłby udział z liczby, którą właśnie się nadpisało —
            // i miasto gubiłoby ludzi po kilku na dobę.
            let moja = c.labour_force();
            let udzial = if sila == 0 {
                0
            } else {
                (u64::from(zostalo) * u64::from(moja) / u64::from(sila)) as u32
            };
            let bez = udzial.min(moja);
            c.unemployed = bez;
            c.employed = moja.saturating_sub(bez);
            zostalo = zostalo.saturating_sub(bez);
        }
        // Reszta z dzielenia trafia do ostatniej komórki dzielnicy — tak samo jak
        // reszta z podziału kwoty trafia do pierwszej strony (00 §2). Liczba ludzi
        // ma się zgadzać co do osoby, nie „w przybliżeniu".
        if zostalo > 0 {
            if let Some(i) = indeksy.last() {
                let c = &mut st.cells[*i];
                let dodaj = zostalo.min(c.employed);
                c.unemployed = c.unemployed.saturating_add(dodaj);
                c.employed -= dodaj;
            }
        }
    }
}
