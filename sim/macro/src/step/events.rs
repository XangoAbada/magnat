//! Faza 8 — zdarzenia (M10a §5.7, co krok).
//!
//! **Katalog zredukowany i to jest jego nazwa, nie skrót.** Kontrakt M10 mówi
//! „zredukowany katalog §11.2 z tymi samymi hazardami co w mezo"; katalog M8c ma
//! kilkadziesiąt zdarzeń, a każde z nich pyta świat o rzeczy, których makro nie ma:
//! pogodę godzinową, sieci przesyłowe, konkretne parcele, obsadę placówek. Tutaj
//! zostają **trzy wstrząsy w skali miasta** — recesja, ożywienie i szok podażowy —
//! bo tylko takie mają sens w agregacie dzielnica × klasa i tylko takie zostawiają
//! ślad w historii, którą gracz przeczyta („trzy recesje" z §5.7, etap D3).
//!
//! # Jeden wstrząs naraz
//!
//! `MacroState` niesie **jeden** wstrząs, nie listę. To jest sufit nazwany: dwa
//! wstrząsy naraz wymagałyby składania ich wpływów, a składanie dwóch mnożników
//! popytu to już model cyklu koniunkturalnego, a nie zdarzenie. Dopóki taki model
//! nie jest nikomu potrzebny, lista o długości jeden jest uczciwsza od listy,
//! której drugi element nigdy nie powstaje.
//!
//! # Losowanie
//!
//! `StreamId::DryRunEvent`, klucz `(indeks pierwszej komórki, tick)`. Hazard jest
//! **stały na dobę** i nie zależy od stanu świata — i to jest różnica wobec M8c,
//! gdzie zdarzenie ma sondę. Sonda w makrze mierzyłaby agregat, którego błąd sama
//! by powiększała; wstrząs losowany niezależnie jest tłem, a nie sprzężeniem.

use magnat_core::{rng, StreamId, Tick};

use crate::state::{MacroShock, MacroState, ShockKind};

use super::MacroParams;

pub fn phase(st: &mut MacroState, p: &MacroParams) {
    // Wstrząs, który się skończył, znika — i to jest jedyne miejsce, w którym znika.
    if st.shock.is_some_and(|s| st.day >= s.ends_day) {
        st.shock = None;
    }
    if st.shock.is_some() || p.shock_hazard_permille == 0 {
        return;
    }
    let mut r = rng(p.seed, StreamId::DryRunEvent, 0, Tick(u64::from(st.day)));
    // Hazard jest dobowy, a krok może obejmować kilka dób — stąd mnożnik.
    // Bez niego historia liczona krokiem sześciodobowym miałaby sześć razy mniej
    // wstrząsów niż ta sama historia liczona dobą po dobie.
    let hazard = p
        .shock_hazard_permille
        .saturating_mul(u16::from(p.days_per_step.max(1)))
        .min(1_000);
    if !r.gen_bool_permille(hazard) {
        return;
    }
    let (kind, magnitude_bp) = match r.gen_range_u32(3) {
        0 => (ShockKind::Recession, -p.shock_magnitude_bp),
        1 => (ShockKind::Boom, p.shock_magnitude_bp),
        _ => (ShockKind::SupplyShock, -p.shock_magnitude_bp),
    };
    let dni = p.shock_min_days + r.gen_range_u32(u32::from(p.shock_span_days)) as u16;
    st.shock = Some(MacroShock {
        kind,
        magnitude_bp,
        started_day: st.day,
        ends_day: st.day.saturating_add(u32::from(dni)),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ClassGrain;

    fn stan() -> MacroState {
        MacroState::empty(ClassGrain::Classes6, 1, 1)
    }

    #[test]
    fn wstrzas_konczy_sie_sam() {
        let mut st = stan();
        // Pewność, żeby test nie czekał na los.
        let mut p = MacroParams {
            shock_hazard_permille: 1_000,
            ..MacroParams::default()
        };
        phase(&mut st, &p);
        let s = st.shock.expect("wstrząs miał wystąpić");
        assert!(s.ends_day > st.day);
        st.day = s.ends_day;
        // Hazard zerowy, żeby po wygaszeniu nie wpadł od razu następny.
        p.shock_hazard_permille = 0;
        phase(&mut st, &p);
        assert!(st.shock.is_none(), "wstrząs po terminie ma zniknąć");
    }

    #[test]
    fn hazard_zerowy_nie_daje_zdarzen() {
        let mut st = stan();
        let p = MacroParams {
            shock_hazard_permille: 0,
            ..MacroParams::default()
        };
        for d in 0..1_000 {
            st.day = d;
            phase(&mut st, &p);
        }
        assert!(st.shock.is_none());
    }
}
