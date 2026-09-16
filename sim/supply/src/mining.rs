//! Wydobycie (M6d §5.10, WP10).
//!
//! **Podział własności jest tu ostrzejszy niż gdziekolwiek indziej w M6 i wynika
//! wprost z `K-13`:** złoże należy do M1 i tylko M1 prowadzi jego bilans masy.
//! `sim/supply` nie trzyma kopii geologii, nie odejmuje voxeli i nie wie, gdzie
//! złoże leży — wie wyłącznie, *ile kosztuje wyjąć z niego tonę*, a to jest pytanie
//! ekonomiczne, nie geologiczne.
//!
//! Techniczną konsekwencją jest port [`Deposits`]: trzy metody, żadnego typu z `sim/world`
//! w sygnaturze. Wzorzec jest ten sam co [`crate::FreightOracle`] i co `TravelOracle`
//! w M3 (`Z-1`) — definicja i atrapa w crate'cie, który pyta, implementacja w tym,
//! który umie odpowiedzieć. Kierunek zależności zamyka sprawę: `sim/world` zależy
//! od `sim/supply`, nigdy odwrotnie.
//!
//! `extract` bierze `&self`, a nie `&mut self`, i to jest decyzja, nie przeoczenie:
//! wydobycie dzieje się w środku `advance_production`, gdzie `&mut Store` i `&mut Plant`
//! są już pożyczone, a przeciskanie czwartej pożyczki mutowalnej przez wszystkie
//! sygnatury pętli minutowej kosztowałoby więcej niż wewnętrzna mutowalność po stronie
//! implementacji — tak samo jak `TrafficOracle` trzyma router pod `Mutex` za `&self`.
//! Determinizm na tym nie cierpi: kolejność zakładów w pętli produkcji jest ustalona,
//! więc kolejność wydobycia też.

use magnat_core::{DepositId, Mass, Money, Q};

/// Bilans złoża — jedyne, czego M6 od M1 potrzebuje (`K-13`).
///
/// Masa jest w **gramach**, w arytmetyce całkowitej. Voxele wyrobiska są pochodną
/// wizualną i liczy je M1 (`Deposit::voxels_for`); tutaj nie pojawiają się ani razu.
pub trait Deposits {
    /// Ile jeszcze zostało.
    fn remaining(&self, id: DepositId) -> Mass;
    /// Ile było na początku — mianownik wyczerpania.
    fn initial(&self, id: DepositId) -> Mass;
    /// Wydobywa; zwraca **faktycznie** wydobyte, co może być mniej niż `want`.
    /// Wartość zwrócona jest tą, która wchodzi do partii — nigdy `want`.
    fn extract(&self, id: DepositId, want: Mass) -> Mass;
}

/// Zakład bez złoża: każde złoże jest puste.
///
/// To jest domyślne wejście produkcji dla zakładów przetwórczych — młyn nie ma złoża
/// i nie ma prawa go dostać. Receptura `Extraction` w zakładzie bez [`MiningSite`]
/// stoi na `Starved`, bo jej masa nie ma skąd przyjść; to samo, co wcześniej robił
/// `batch_mass == 0`, tylko z powodem widocznym w karcie inspekcji.
pub struct NoDeposits;

impl Deposits for NoDeposits {
    fn remaining(&self, _id: DepositId) -> Mass {
        Mass::ZERO
    }
    fn initial(&self, _id: DepositId) -> Mass {
        Mass::ZERO
    }
    fn extract(&self, _id: DepositId, _want: Mass) -> Mass {
        Mass::ZERO
    }
}

/// Ekonomia wydobycia w zakładzie: co kopiemy, jak głęboko i jak bogata jest skała.
///
/// `concentration_pct` i `depth_m` są **kopiowane raz**, przy założeniu zakładu —
/// to parametry ekonomiczne, nie stan terenu. Stan terenu żyje po stronie M1 i pyta
/// się o niego przez [`Deposits::remaining`].
///
/// **Czego tu nie ma wobec §5.10 i dlaczego** (`AL-1`): `daily_capacity` nie powstaje,
/// bo to ta sama liczba co `ProductionLine::nominal_throughput` — dwa pola o jednej
/// treści rozjeżdżają się przy pierwszej zmianie, a linia i tak jest tym, co ogranicza
/// przerób. `good` też nie powstaje: mówi o nim wyjście receptury `Extraction(kind)`,
/// czyli dane, a nie druga tabela w kodzie.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MiningSite {
    pub deposit: DepositId,
    /// Ile masy użytecznej na tonę urobku, w procentach.
    pub concentration_pct: u8,
    pub depth_m: u16,
}

impl MiningSite {
    #[must_use]
    pub const fn new(deposit: DepositId, concentration: Q, depth_m: u16) -> MiningSite {
        MiningSite {
            deposit,
            concentration_pct: concentration.get(),
            depth_m,
        }
    }

    /// Wyczerpanie w promilach: 0 = nietknięte, 1000 = puste.
    #[must_use]
    pub fn depletion_permille(&self, remaining: Mass, initial: Mass) -> i64 {
        if initial.0 <= 0 {
            return 1000;
        }
        let z = 1000 - (i128::from(remaining.0.max(0)) * 1000 / i128::from(initial.0)) as i64;
        z.clamp(0, 1000)
    }

    /// Efektywna koncentracja: najlepsze żyły idą pierwsze, więc to, co zostaje,
    /// jest coraz uboższe. Pierwiastek trzeciego stopnia z udziału pozostałej masy,
    /// w promilach — tablicowany, bez ani jednej funkcji przestępnej (`K-6`).
    #[must_use]
    pub fn concentration_eff(&self, remaining: Mass, initial: Mass) -> i64 {
        let c = i64::from(self.concentration_pct).max(1);
        c * cbrt_permille(remaining, initial) / 1000
    }

    /// Koszt wydobycia tony urobku (§5.10). Rośnie z głębokością, z ubytkiem
    /// koncentracji i **kwadratowo** z wyczerpaniem — to ten ostatni człon sprawia,
    /// że złoże zamyka się samo, zanim fizycznie się skończy: w pewnym momencie
    /// import jest po prostu tańszy i nikt tego nie musi zaprogramować.
    #[must_use]
    pub fn cost_per_tonne(&self, base: Money, remaining: Mass, initial: Mass) -> Money {
        let z = self.depletion_permille(remaining, initial);
        let conc = self.concentration_eff(remaining, initial).max(1);
        let mut v = i128::from(base.0);
        v = v * (100 + i128::from(self.depth_m) / 20) / 100;
        v = v * 100 / i128::from(conc);
        v = v * (1000 + 2 * i128::from(z) * i128::from(z) / 1000) / 1000;
        Money(v.min(i128::from(i64::MAX)) as i64)
    }

    /// Koszt wydobycia podanej masy.
    #[must_use]
    pub fn cost_of(&self, base: Money, mass: Mass, remaining: Mass, initial: Mass) -> Money {
        let per_t = self.cost_per_tonne(base, remaining, initial);
        Money((i128::from(per_t.0) * i128::from(mass.0.max(0)) / 1_000_000) as i64)
    }
}

/// `cbrt(remaining/initial)` w promilach, z tablicy 64 wartości (§5.10).
///
/// Tablica, a nie `f64::cbrt`: `K-6` zakazuje funkcji przestępnych ze `std` w kodzie
/// symulacji, a ta liczba wchodzi wprost do kosztu, czyli do pieniądza. 64 kubełki
/// dają skok koncentracji poniżej punktu procentowego — poniżej rozdzielczości,
/// z jaką ktokolwiek tę liczbę ogląda.
#[must_use]
pub fn cbrt_permille(remaining: Mass, initial: Mass) -> i64 {
    if initial.0 <= 0 || remaining.0 <= 0 {
        return 0;
    }
    if remaining.0 >= initial.0 {
        return 1000;
    }
    let i = (i128::from(remaining.0) * 64 / i128::from(initial.0)) as usize;
    CBRT_LUT[i.min(63)]
}

/// `round(1000 * cbrt(i/64))` dla `i` od 0 do 63. Wartość dla pełnego złoża (1000)
/// zwracana jest gałęzią wyżej, bo `i == 64` nie mieści się w tablicy.
const CBRT_LUT: [i64; 64] = [
    0, 250, 315, 361, 397, 427, 454, 478, 500, 520, 539, 556,
    572, 588, 603, 617, 630, 643, 655, 667, 679, 690, 701, 711,
    721, 731, 741, 750, 759, 768, 777, 785, 794, 802, 810, 818,
    825, 833, 840, 848, 855, 862, 869, 876, 883, 889, 896, 902,
    909, 915, 921, 927, 933, 939, 945, 951, 956, 962, 968, 973,
    979, 984, 989, 995,
];

#[cfg(test)]
mod tests {
    use super::*;

    fn szyb() -> MiningSite {
        // Szyb naftowy z §7.2: koncentracja 82 %, głębokość 1 800 m.
        MiningSite::new(DepositId(0), Q::new(82), 1_800)
    }

    /// Tablica sprawdzana **odwrotnością**, a nie `f64::cbrt`: `K-6` zakazuje funkcji
    /// przestępnych ze `std` w kodzie symulacji, a test jest kodem tego samego repozytorium
    /// i lint `clippy.toml` nie robi dla niego wyjątku. Podniesienie do sześcianu jest
    /// zresztą mocniejsze od porównania z floatem — mówi, że wpisu **nie da się** poprawić
    /// o jeden w żadną stronę.
    #[test]
    fn tablica_pierwiastka_zgadza_sie_z_definicja() {
        // |v³·64 − i·10⁹| ma być najmniejsze dla wpisanego `v`.
        let blad = |v: i128, i: i128| (v * v * v * 64 - i * 1_000_000_000).abs();
        for i in 0..64i128 {
            let v = i128::from(CBRT_LUT[i as usize]);
            assert!(
                blad(v, i) <= blad(v - 1, i) && blad(v, i) <= blad(v + 1, i),
                "kubełek {i}: {v} nie jest najlepszym zaokrągleniem"
            );
        }
        assert_eq!(CBRT_LUT[0], 0);
    }

    #[test]
    fn koszt_rosnie_monotonicznie_z_wyczerpaniem() {
        let m = szyb();
        let init = Mass(2_400_000_000_000);
        let base = Money(69_915);
        let mut poprzedni = 0i64;
        for krok in 0..=100 {
            let remaining = Mass(init.0 * (100 - krok) / 100);
            let c = m.cost_per_tonne(base, remaining, init).0;
            assert!(
                c >= poprzedni,
                "koszt spadł przy wyczerpaniu {krok} %: {poprzedni} → {c}"
            );
            poprzedni = c;
        }
    }

    #[test]
    fn koncentracja_efektywna_nie_rosnie() {
        let m = szyb();
        let init = Mass(1_000_000_000);
        let mut poprzednia = i64::MAX;
        for krok in 0..=64 {
            let remaining = Mass(init.0 * (64 - krok) / 64);
            let c = m.concentration_eff(remaining, init);
            assert!(c <= poprzednia, "koncentracja wzrosła na kroku {krok}");
            poprzednia = c;
        }
    }

    #[test]
    fn zloze_puste_nie_dzieli_przez_zero() {
        let m = szyb();
        assert_eq!(m.depletion_permille(Mass::ZERO, Mass::ZERO), 1000);
        assert!(m.cost_per_tonne(Money(1_000), Mass::ZERO, Mass(10)).0 > 0);
    }
}
