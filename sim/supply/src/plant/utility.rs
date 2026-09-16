//! Licznik mediów i faktura miesięczna (M6b §5.5, WP4).
//!
//! **Media są licznikiem, nie towarem w partii** — wykonanie decyzji `D9` fazy.
//! Piekarni nikt nie dowozi wody ciężarówką i nikt nie składuje prądu na palecie,
//! więc modelowanie ich jako partii dawałoby zaplecze pełne skrzynek z kilowatami.
//! Masa wody mimo to wchodzi do bilansu receptury (`Recipe::input_mass`), bo woda
//! wychodzi z pieca jako chleb — licznikowość dotyczy **dostawy**, nie masy.
//!
//! Konsekwencja, dla której ten plik jest w ogóle potrzebny: `cut_off` na liczniku
//! prądu zatrzymuje całą linię (`LineState::Broken { NoPower }`, PRD §9.6). To jest
//! jedyne wejście, którego zakład nie może sobie przywieźć.

use magnat_core::{
    Energy, FirmId, HashState, Money, SimMinute, StateHasher, UtilityService, Volume,
};

use crate::tuning::WattMinutes;

/// Licznik jednego medium w zakładzie.
#[derive(Clone, Copy, Debug)]
pub struct UtilityMeter {
    pub kind: UtilityService,
    /// Zużycie od ostatniej faktury: **watominuty** dla energii, mililitry dla cieczy.
    /// Dlaczego watominuty, a nie watogodziny — patrz [`WattMinutes`].
    pub consumed: i64,
    pub supplier: FirmId,
    /// Taryfa za kWh (energia) albo za m³ (ciecze).
    pub tariff: Money,
    pub billed_until: SimMinute,
    /// Brak płatności albo awaria sieci (M8). Zakład bez prądu stoi.
    pub cut_off: bool,
}

impl UtilityMeter {
    #[must_use]
    pub fn new(kind: UtilityService, supplier: FirmId, tariff: Money) -> UtilityMeter {
        UtilityMeter {
            kind,
            consumed: 0,
            supplier,
            tariff,
            billed_until: SimMinute(0),
            cut_off: false,
        }
    }

    /// Czy medium rozlicza się w kilowatogodzinach. Ciecze idą w metrach sześciennych
    /// i to jedyne miejsce, w którym ta różnica ma znaczenie.
    #[must_use]
    pub const fn is_energy(self) -> bool {
        matches!(
            self.kind,
            UtilityService::Electricity | UtilityService::Gas | UtilityService::Heat
        )
    }

    pub fn draw_energy(&mut self, wm: WattMinutes) {
        self.consumed += wm.0;
    }

    pub fn draw_volume(&mut self, v: Volume) {
        self.consumed += v.0;
    }

    /// Zużycie w jednostkach rozliczeniowych — watogodziny albo mililitry.
    #[must_use]
    pub const fn reading(self) -> i64 {
        if self.is_energy() {
            self.consumed / 60
        } else {
            self.consumed
        }
    }

    /// Kwota do zapłaty za to, co licznik naliczył.
    ///
    /// Dzielenie **na końcu**, po pomnożeniu przez taryfę: gdyby najpierw przeliczyć
    /// watominuty na kWh, mały zakład płaciłby zero przez cały miesiąc.
    #[must_use]
    pub fn amount_due(self) -> Money {
        let dzielnik: i128 = if self.is_energy() {
            60 * 1_000 // watominuty → kWh
        } else {
            1_000_000 // mililitry → m³
        };
        Money((i128::from(self.consumed) * i128::from(self.tariff.0) / dzielnik) as i64)
    }

    /// Zamyka okres rozliczeniowy: zwraca kwotę faktury i zeruje licznik.
    ///
    /// Reszta **nie przepada**: to, co nie złożyło się na pełną jednostkę, zostaje
    /// na liczniku i doliczy się w następnym miesiącu. Bez tego zakład o małym poborze
    /// płaciłby zero co miesiąc przez sto lat gry, a bilans pieniądza nigdy by tego
    /// nie zauważył, bo zero jest poprawną kwotą.
    pub fn bill(&mut self, until: SimMinute) -> Money {
        let kwota = self.amount_due();
        let dzielnik: i128 = if self.is_energy() { 60 * 1_000 } else { 1_000_000 };
        let rozliczone = if self.tariff.0 == 0 {
            self.consumed
        } else {
            (i128::from(kwota.0) * dzielnik / i128::from(self.tariff.0)) as i64
        };
        self.consumed -= rozliczone;
        self.billed_until = until;
        kwota
    }

    /// Energia naliczona od ostatniej faktury — do karty inspekcji i do panelu.
    #[must_use]
    pub const fn energy(self) -> Energy {
        WattMinutes(self.consumed).energy()
    }
}

impl HashState for UtilityMeter {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(self.kind as u8);
        h.write_i64(self.consumed);
        self.supplier.entity().hash_state(h);
        self.tariff.hash_state(h);
        self.billed_until.hash_state(h);
        h.write_u8(u8::from(self.cut_off));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::Entity;
    use std::num::NonZeroU32;

    fn firma() -> FirmId {
        FirmId(Entity::new(0, NonZeroU32::new(1).expect("generacja")))
    }

    /// Rachunek za prąd małego zakładu **nie jest zerem**. Linia 30 Wh/h przez miesiąc
    /// to 21,6 kWh — niedużo, ale nie nic, a przy naiwnym dzieleniu w każdej minucie
    /// wychodziłoby dokładnie nic przez całe sto lat gry.
    #[test]
    fn maly_pobor_daje_niezerowy_rachunek() {
        let mut m = UtilityMeter::new(UtilityService::Electricity, firma(), Money(65));
        for _ in 0..30 * 24 * 60 {
            m.draw_energy(WattMinutes::per_minute(Energy(30)));
        }
        assert_eq!(m.energy(), Energy(21_600));
        assert_eq!(m.bill(SimMinute(43_200)), Money(1_404)); // 21,6 kWh × 0,65 zł
    }

    /// Reszta nie przepada: to, co nie złożyło się na pełną jednostkę rozliczeniową,
    /// czeka na następną fakturę zamiast wyparować.
    #[test]
    fn reszta_przechodzi_na_nastepny_okres() {
        let mut m = UtilityMeter::new(UtilityService::Electricity, firma(), Money(65));
        // 900 watominut to 15 Wh — poniżej grosza przy tej taryfie.
        m.draw_energy(WattMinutes(900));
        assert_eq!(m.bill(SimMinute(43_200)), Money::ZERO);
        assert_eq!(m.consumed, 900, "licznik nie skasował nierozliczonego zużycia");

        // Po dołożeniu reszty rachunek wreszcie wychodzi.
        m.draw_energy(WattMinutes(60 * 1_000 - 900));
        assert_eq!(m.bill(SimMinute(86_400)), Money(65));
        assert_eq!(m.consumed, 0);
    }

    /// Woda rozlicza się w metrach sześciennych, nie w kilowatogodzinach — jedyna
    /// różnica między mediami, która ma znaczenie w kodzie.
    #[test]
    fn woda_rozlicza_sie_w_metrach_szesciennych() {
        let mut m = UtilityMeter::new(UtilityService::Water, firma(), Money(480));
        m.draw_volume(Volume(2_500_000)); // 2,5 m³
        assert_eq!(m.amount_due(), Money(1_200));
        assert!(!m.is_energy());
    }
}
