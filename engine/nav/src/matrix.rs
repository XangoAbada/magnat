//! Macierz czasów przejazdu dzielnica × dzielnica × godzina × środek transportu
//! (PRD §17.6, M4a WP2).
//!
//! **Po co osobno od routera.** Planowanie mezo, filtrowanie ofert pracy (M7 §5.6)
//! i LOD makro (M12) nie potrzebują trasy — potrzebują odpowiedzi „ile zajmie
//! dojazd stamtąd tam o tej porze". Uruchamianie routera dla każdego kandydata
//! na pracę byłoby kilka rzędów wielkości za drogie, a odpowiedź i tak wpadłaby
//! do jednej z kilkudziesięciu dzielnic.
//!
//! **Skąd się bierze treść.** Z obserwacji, nie z modelu: agent, który faktycznie
//! przejechał, dokłada swój czas przez [`TravelTimeMatrix::observe`]. Uśrednianie
//! wykładnicze na liczbach całkowitych, bez floatów (00 §2) — macierz jest stanem
//! symulacji i wchodzi do hasha (00 §3.6), więc float rozjechałby zapis.
//!
//! **Komórka bez obserwacji nie kłamie.** `lookup` zwraca `None`, a nie zero ani
//! zgadniętą wartość — konsument sam decyduje, czy podstawić odległość w linii
//! prostej ([`TravelTimeMatrix::lookup_or`]), czy w ogóle odrzucić opcję.

use magnat_core::{DistrictId, HashState, StateHasher, TransportMode};

/// Liczba środków transportu — ostatni wymiar macierzy.
const MODES: usize = TransportMode::ALL.len();

/// Liczba przedziałów godzinowych doby.
const HOURS: usize = 24;

/// Dzielnik EWMA: `α = 1/8`. Ósemka, bo przy dobowym cyklu obserwacji daje
/// stałą czasową rzędu doby — dość, żeby korek w poniedziałek nie przestawił
/// całej macierzy, i mało, żeby zamknięcie mostu było widać po dwóch dniach.
const EWMA_DIV: u32 = 8;

/// „Brak obserwacji". Wartość realna nigdy jej nie osiąga, bo `observe`
/// przycina wejście do `u16::MAX - 1`.
const PUSTA: u16 = u16::MAX;

/// Czasy przejazdu między dzielnicami, w minutach.
///
/// // ponytail: macierz jest **gęsta**. Sufit: `n² · 24 · 7 · (2 + 4)` bajtów,
/// // czyli ok. 4,1 MB przy 64 dzielnicach i już ok. 10 MB przy stu — a miasto
/// // i tak wypełnia z tego garstkę komórek, bo nikt nie dojeżdża wszystkimi
/// // siedmioma środkami transportu do wszystkich dzielnic o każdej godzinie.
/// // Droga wyjścia: wiersze rzadkie (CSR po parze dzielnic) albo zwinięcie
/// // godzin do kilku przedziałów, gdy liczba dzielnic przekroczy sto.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TravelTimeMatrix {
    n: u16,
    /// Minuty; [`PUSTA`] znaczy „nie ma obserwacji".
    minutes: Vec<u16>,
    /// Liczba obserwacji tej komórki — zaufanie do wartości i waga przy scalaniu.
    samples: Vec<u32>,
}

impl TravelTimeMatrix {
    /// `n_districts × n_districts × 24 × TransportMode::ALL.len()` pozycji.
    #[must_use]
    pub fn new(n_districts: u16) -> TravelTimeMatrix {
        let n = n_districts as usize;
        let len = n * n * HOURS * MODES;
        TravelTimeMatrix {
            n: n_districts,
            minutes: vec![PUSTA; len],
            samples: vec![0; len],
        }
    }

    #[must_use]
    pub fn n_districts(&self) -> u16 {
        self.n
    }

    /// Zajętość pamięci w bajtach — 2 B na czas i 4 B na licznik obserwacji.
    #[must_use]
    pub fn memory_bytes(&self) -> usize {
        std::mem::size_of::<TravelTimeMatrix>()
            + self.minutes.len() * std::mem::size_of::<u16>()
            + self.samples.len() * std::mem::size_of::<u32>()
    }

    /// Czas przejazdu w minutach; `None`, gdy komórka nie ma jeszcze obserwacji
    /// albo gdy indeks wychodzi poza macierz.
    #[must_use]
    pub fn lookup(
        &self,
        from: DistrictId,
        to: DistrictId,
        hour: u8,
        mode: TransportMode,
    ) -> Option<u16> {
        let i = self.index(from, to, hour, mode)?;
        match self.minutes[i] {
            PUSTA => None,
            v => Some(v),
        }
    }

    /// Jak [`TravelTimeMatrix::lookup`], ale nigdy nie zawodzi: brak obserwacji
    /// → `fallback`. Konsument mezo woli oszacowanie od gałęzi `None` w pętli
    /// po kilkuset ofertach pracy.
    #[must_use]
    pub fn lookup_or(
        &self,
        from: DistrictId,
        to: DistrictId,
        hour: u8,
        mode: TransportMode,
        fallback: u16,
    ) -> u16 {
        self.lookup(from, to, hour, mode).unwrap_or(fallback)
    }

    /// Liczba obserwacji w komórce — miara zaufania do wartości. `0` poza zakresem.
    #[must_use]
    pub fn samples(&self, from: DistrictId, to: DistrictId, hour: u8, mode: TransportMode) -> u32 {
        self.index(from, to, hour, mode)
            .map_or(0, |i| self.samples[i])
    }

    /// Aktualizacja z obserwacji: EWMA na liczbach całkowitych,
    /// `nowa = (stara · 7 + obserwacja + 4) / 8` z zaokrągleniem do najbliższej.
    /// Pierwsza obserwacja ustawia wartość wprost — uśrednianie z sentinelem
    /// dałoby bzdurę rzędu tysięcy minut.
    ///
    /// Obserwacja spoza zakresu indeksów jest **pomijana**: w wydaniu deweloperskim
    /// przerywa `debug_assert`, w produkcyjnym nie rusza pamięci. Wartość przycina
    /// się do `u16::MAX - 1`, żeby nigdy nie zapisać sentinela „brak obserwacji".
    pub fn observe(
        &mut self,
        from: DistrictId,
        to: DistrictId,
        hour: u8,
        mode: TransportMode,
        minutes: u16,
    ) {
        let Some(i) = self.index(from, to, hour, mode) else {
            debug_assert!(
                false,
                "obserwacja poza macierzą: {from:?}→{to:?} godz. {hour} {mode:?}"
            );
            return;
        };
        let obs = minutes.min(PUSTA - 1);
        if self.samples[i] == 0 {
            self.minutes[i] = obs;
            self.samples[i] = 1;
        } else {
            let old = u32::from(self.minutes[i]);
            let nowa = (old * (EWMA_DIV - 1) + u32::from(obs) + EWMA_DIV / 2) / EWMA_DIV;
            self.minutes[i] = nowa.min(u32::from(PUSTA - 1)) as u16;
            self.samples[i] = self.samples[i].saturating_add(1);
        }
    }

    /// Scala obserwacje z innej macierzy tego samego kształtu — deterministyczne
    /// składanie wyników liczonych równolegle po chunkach (00 §3.3).
    ///
    /// Komórka wypełniona tylko po jednej stronie przechodzi bez zmian; wypełniona
    /// po obu daje średnią **ważoną liczbą obserwacji**, bo inaczej chunk z jedną
    /// próbką ważyłby tyle co chunk z tysiącem i wynik zależałby od podziału pracy.
    ///
    /// # Panics
    /// Gdy kształty się różnią.
    pub fn merge(&mut self, other: &TravelTimeMatrix) {
        assert_eq!(
            self.n, other.n,
            "scalanie macierzy o różnej liczbie dzielnic"
        );
        for i in 0..self.minutes.len() {
            let wo = u64::from(other.samples[i]);
            if wo == 0 {
                continue;
            }
            let ws = u64::from(self.samples[i]);
            if ws == 0 {
                self.minutes[i] = other.minutes[i];
            } else {
                let suma = ws + wo;
                let v =
                    (u64::from(self.minutes[i]) * ws + u64::from(other.minutes[i]) * wo + suma / 2)
                        / suma;
                self.minutes[i] = v.min(u64::from(PUSTA - 1)) as u16;
            }
            self.samples[i] = self.samples[i].saturating_add(other.samples[i]);
        }
    }

    /// Płaski indeks komórki; `None`, gdy którykolwiek wymiar wychodzi poza macierz.
    fn index(
        &self,
        from: DistrictId,
        to: DistrictId,
        hour: u8,
        mode: TransportMode,
    ) -> Option<usize> {
        if from.0 >= self.n || to.0 >= self.n || (hour as usize) >= HOURS {
            return None;
        }
        let n = self.n as usize;
        let para = from.0 as usize * n + to.0 as usize;
        Some((para * HOURS + hour as usize) * MODES + mode.as_index())
    }
}

impl HashState for TravelTimeMatrix {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u16(self.n);
        for (&m, &s) in self.minutes.iter().zip(self.samples.iter()) {
            h.write_u16(m);
            h.write_u32(s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hasz(m: &TravelTimeMatrix) -> u128 {
        let mut h = StateHasher::new();
        m.hash_state(&mut h);
        h.finish().0
    }

    const D0: DistrictId = DistrictId(0);
    const D1: DistrictId = DistrictId(1);
    const CAR: TransportMode = TransportMode::Car;

    #[test]
    fn pusta_komorka_nie_udaje_zera() {
        let m = TravelTimeMatrix::new(4);
        assert_eq!(m.n_districts(), 4);
        assert_eq!(m.lookup(D0, D1, 8, CAR), None);
        assert_eq!(m.lookup_or(D0, D1, 8, CAR, 99), 99);
        assert_eq!(m.samples(D0, D1, 8, CAR), 0);
    }

    #[test]
    fn pierwsza_obserwacja_wchodzi_bez_usredniania() {
        let mut m = TravelTimeMatrix::new(4);
        m.observe(D0, D1, 8, CAR, 25);
        assert_eq!(m.lookup(D0, D1, 8, CAR), Some(25));
        assert_eq!(m.samples(D0, D1, 8, CAR), 1);
        // Sąsiednie komórki nietknięte — indeksowanie nie przecieka.
        assert_eq!(m.lookup(D0, D1, 9, CAR), None);
        assert_eq!(m.lookup(D1, D0, 8, CAR), None);
        assert_eq!(m.lookup(D0, D1, 8, TransportMode::Bus), None);
    }

    #[test]
    fn ewma_zbiega_monotonicznie_i_nie_przestrzeliwuje() {
        for (start, cel) in [(10u16, 100u16), (100, 10), (42, 42)] {
            let mut m = TravelTimeMatrix::new(2);
            m.observe(D0, D1, 0, CAR, start);
            let mut poprzednia = start;
            for _ in 0..200 {
                m.observe(D0, D1, 0, CAR, cel);
                let v = m.lookup(D0, D1, 0, CAR).expect("komórka ma obserwację");
                if cel >= start {
                    assert!(v >= poprzednia, "spadek przy zbieżności w górę");
                    assert!(v <= cel, "przestrzelenie {v} ponad cel {cel}");
                } else {
                    assert!(v <= poprzednia, "wzrost przy zbieżności w dół");
                    assert!(v >= cel, "przestrzelenie {v} poniżej celu {cel}");
                }
                poprzednia = v;
            }
            // Martwa strefa: patrz `EWMA_DIV`. Zbieżność zatrzymuje się w paśmie
            // `cel-3 .. cel+4`, nie na samym celu — to sufit arytmetyki całkowitej,
            // nie błąd zbieżności.
            assert!(
                poprzednia.abs_diff(cel) <= 4,
                "utknęło na {poprzednia}, cel {cel}"
            );
            assert_eq!(m.samples(D0, D1, 0, CAR), 201);
        }
    }

    #[test]
    fn obserwacja_nie_zapisuje_sentinela() {
        let mut m = TravelTimeMatrix::new(2);
        m.observe(D0, D0, 0, CAR, u16::MAX);
        assert_eq!(m.lookup(D0, D0, 0, CAR), Some(u16::MAX - 1));
        for _ in 0..50 {
            m.observe(D0, D0, 0, CAR, u16::MAX);
        }
        assert_eq!(m.lookup(D0, D0, 0, CAR), Some(u16::MAX - 1));
    }

    #[test]
    fn scalanie_rozlacznych_macierzy_daje_to_samo_co_wypelnienie_wprost() {
        let mut a = TravelTimeMatrix::new(3);
        let mut b = TravelTimeMatrix::new(3);
        let mut wprost = TravelTimeMatrix::new(3);

        for (i, godz) in [(0u16, 6u8), (1, 7), (2, 8)] {
            let d = DistrictId(i);
            a.observe(d, D0, godz, CAR, 10 + i);
            wprost.observe(d, D0, godz, CAR, 10 + i);
        }
        for (i, godz) in [(0u16, 16u8), (1, 17), (2, 18)] {
            let d = DistrictId(i);
            b.observe(d, D1, godz, TransportMode::Bus, 30 + i);
            wprost.observe(d, D1, godz, TransportMode::Bus, 30 + i);
        }

        a.merge(&b);
        assert_eq!(a, wprost);
        assert_eq!(hasz(&a), hasz(&wprost));
    }

    #[test]
    fn scalanie_wazy_wspolna_komorke_liczba_obserwacji() {
        let mut a = TravelTimeMatrix::new(2);
        let mut b = TravelTimeMatrix::new(2);
        a.observe(D0, D1, 8, CAR, 20);
        a.observe(D0, D1, 8, CAR, 20);
        a.observe(D0, D1, 8, CAR, 20);
        b.observe(D0, D1, 8, CAR, 40);
        a.merge(&b);
        // (20·3 + 40·1 + 2) / 4 = 25
        assert_eq!(a.lookup(D0, D1, 8, CAR), Some(25));
        assert_eq!(a.samples(D0, D1, 8, CAR), 4);
    }

    #[test]
    #[should_panic(expected = "różnej liczbie dzielnic")]
    fn scalanie_innego_ksztaltu_panikuje() {
        let mut a = TravelTimeMatrix::new(2);
        a.merge(&TravelTimeMatrix::new(3));
    }

    #[test]
    fn indeksy_spoza_zakresu_sa_bezpieczne() {
        let mut m = TravelTimeMatrix::new(2);
        let poza = DistrictId(9);
        assert_eq!(m.lookup(poza, D0, 0, CAR), None);
        assert_eq!(m.lookup(D0, poza, 0, CAR), None);
        assert_eq!(m.lookup(D0, D0, 24, CAR), None);
        assert_eq!(m.lookup_or(D0, D0, 250, CAR, 7), 7);
        assert_eq!(m.samples(poza, poza, 99, CAR), 0);
        // `observe` poza zakresem nie może niczego ruszyć (tu bez debug_assert,
        // bo ten wariant sprawdza wydanie produkcyjne — patrz doc `observe`).
        let przed = hasz(&m);
        m.observe(D0, D0, 0, CAR, 15);
        assert_ne!(hasz(&m), przed, "zapis w zakresie ma zmieniać hash");
    }

    #[test]
    fn hash_zmienia_sie_z_komorka_i_jest_stabilny_bez_zmian() {
        let mut m = TravelTimeMatrix::new(3);
        let pusty = hasz(&m);
        assert_eq!(hasz(&TravelTimeMatrix::new(3)), pusty, "stabilny");
        assert_ne!(
            hasz(&TravelTimeMatrix::new(4)),
            pusty,
            "kształt wchodzi do hasha"
        );

        m.observe(D0, D1, 8, CAR, 25);
        let po = hasz(&m);
        assert_ne!(po, pusty);
        // Powtórzona ta sama obserwacja zmienia licznik próbek, więc i hash.
        m.observe(D0, D1, 8, CAR, 25);
        assert_ne!(hasz(&m), po);
    }

    #[test]
    fn pamiec_jest_w_zapowiedzianym_rzedzie_wielkosci() {
        let m = TravelTimeMatrix::new(64);
        let komorki = 64usize * 64 * HOURS * MODES;
        assert_eq!(
            m.memory_bytes(),
            std::mem::size_of::<TravelTimeMatrix>() + komorki * 6
        );
        // ok. 4,1 MB — zgodnie z komentarzem `ponytail:` przy definicji typu.
        assert!(m.memory_bytes() < 5_000_000);
    }
}
