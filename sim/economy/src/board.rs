//! `PublicMarketBoard` — jawny obraz cen w mieście (M7e WP10, M7 §5.8, PRD §12.2).
//!
//! # Po co to jest, skoro sklep ma już `CompetitorSnapshot`
//!
//! Migawka z M5c jest **prywatnym okiem jednego sklepu**: powstaje z promienia wokół
//! jego drzwi i widzi tylko towary z jego półki. Firma produkcyjna nie ma drzwi ani
//! półki, a tier taktyczny pyta o dzielnicę, nie o promień — potrzebny jest więc
//! obraz, którego nikt nie jest właścicielem i w którym da się sprawdzić, po ile
//! chodzi mąka w Śródmieściu, nie będąc sklepem w Śródmieściu.
//!
//! Tablica jest **jedna na świat**: `(dzielnica × towar) → siedem dób obserwacji`.
//! Wariant „kopia obserwacji per firma" byłby `O(firmy × konkurenci)` i został
//! odrzucony w planie fazy; tutaj koszt to rzędu setek kilobajtów, bo klucze powstają
//! wyłącznie dla par, które **ktoś faktycznie sprzedaje**.
//!
//! # Opóźnienie realizuje się przy odczycie
//!
//! Tablica zapisuje prawdę każdej doby; to [`PublicMarketBoard::observed`] zwraca
//! dobę sprzed `lag_days`. Dzięki temu jedno źródło obsługuje firmę czujną i firmę
//! gapowatą, a opóźnienie jest cechą **patrzącego**, nie obrazu (§5.8).
//!
//! # Co tu jest, a czego nie ma
//!
//! Są: cena najtańsza, mediana, liczba ofert i **tożsamość najtańszego sprzedawcy** —
//! wszystko to widać z ulicy. Nie ma i być nie może: kosztu, marży, zapasu ani gotówki
//! kogokolwiek. Cena trzymana jest w podstawie **netto** (`K-7` pkt 4): detal publikuje
//! brutto, hurt netto, a firma porównująca je bez sprowadzenia do jednej podstawy
//! „zobaczyłaby" u hurtownika cenę niższą o stawkę i weszła w wojnę cenową z powietrzem.

use std::collections::BTreeMap;

use magnat_core::hash::{HashState, StateHasher};
use magnat_core::{DistrictId, GoodId, Money, SiteId};

/// Długość okna obserwacji w dobach (§5.8). Siedem, bo `lag_days` ma zakres 1..=7 —
/// ósma doba nie miałaby czytelnika.
pub const WINDOW_DAYS: usize = 7;

/// Jedna doba obserwacji pary (dzielnica, towar).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ObservedPrice {
    /// Najtańsza cena netto w tej dzielnicy tej doby.
    pub cheapest_net: Money,
    /// Mediana cen netto — element **górny** przy parzystej liczbie ofert,
    /// ta sama konwencja co w `offer::price_stats`.
    pub median_net: Money,
    /// Który **zakład** oferował najtaniej. Zakład, a nie firma: to on stoi przy
    /// ulicy i to jego cenę widać. `FirmId` sklepu pochodzi z generatora miasta,
    /// a `FirmKey` z rejestru firm — nie są tą samą liczbą (`K-46`), więc tożsamość
    /// przenoszona między crate'ami musi być tą, która nie ma dwóch numeracji.
    pub cheapest_site: Option<SiteId>,
    /// Ile ofert złożyło się na tę obserwację. Zero znaczy „tej doby nikt tu tego
    /// nie sprzedawał" i **nie** jest tym samym co cena zero.
    pub offers: u16,
}

impl ObservedPrice {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.offers == 0
    }
}

impl HashState for ObservedPrice {
    fn hash_state(&self, h: &mut StateHasher) {
        self.cheapest_net.hash_state(h);
        self.median_net.hash_state(h);
        match self.cheapest_site {
            None => h.write_u8(0),
            Some(s) => {
                h.write_u8(1);
                s.0.hash_state(h);
            }
        }
        h.write_u16(self.offers);
    }
}

/// Siedmiodobowy pierścień jednej pary (dzielnica, towar).
///
/// Tablica, nie kolejka: siedem wpisów po 24 bajty to 168 bajtów bez ani jednej
/// alokacji na parę — ten sam rachunek, którym M6 uzasadnił `SpotWindow`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct Window {
    dni: [ObservedPrice; WINDOW_DAYS],
    /// Numer doby ostatniego zapisu. Trzymany, bo pierścień indeksuje się dobą
    /// modulo siedem i bez tego nie da się odróżnić „wczoraj" od „tydzień temu".
    last_day: u32,
}

/// Jawny obraz cen miasta — jeden zasób na świat.
///
/// `BTreeMap`, a nie `HashMap`: po tej kolejności idzie hash stanu (00 §3.2).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct PublicMarketBoard {
    okna: BTreeMap<(DistrictId, GoodId), Window>,
    /// Ostatnia doba, którą tablica widziała. Poza kluczami, bo pusta tablica też
    /// ma dobę — i dzięki temu odczyt z pustej tablicy nie udaje, że jest w dobie zero.
    day: u32,
}

impl PublicMarketBoard {
    #[must_use]
    pub fn new() -> PublicMarketBoard {
        PublicMarketBoard::default()
    }

    #[must_use]
    pub const fn day(&self) -> u32 {
        self.day
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.okna.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.okna.is_empty()
    }

    /// Zapisuje dzisiejszą obserwację pary. Woła się **raz na dobę na parę**;
    /// drugie wywołanie tej samej doby nadpisuje, a nie dopisuje.
    pub fn record(&mut self, district: DistrictId, good: GoodId, day: u32, obs: ObservedPrice) {
        self.day = self.day.max(day);
        let w = self.okna.entry((district, good)).or_default();
        // Przerwa dłuższa niż okno znaczy, że wszystko w środku jest nieaktualne.
        // Bez tego zamknięty na miesiąc sklep „wracałby" z ceną sprzed miesiąca.
        if day.saturating_sub(w.last_day) as usize >= WINDOW_DAYS {
            w.dni = [ObservedPrice::default(); WINDOW_DAYS];
        } else {
            for d in (w.last_day + 1)..day {
                w.dni[(d as usize) % WINDOW_DAYS] = ObservedPrice::default();
            }
        }
        w.dni[(day as usize) % WINDOW_DAYS] = obs;
        w.last_day = day;
    }

    /// Co firma **widzi** dziś przy swoim opóźnieniu (§5.8).
    ///
    /// `lag_days` jest przycinane do okna: firma o opóźnieniu siedmiu dób patrzy
    /// na najstarszą dobę, jaką tablica trzyma, a nie na żadną. Doba bez ani jednej
    /// oferty zwraca `None` — „nikogo nie widać" i „widać kogoś za zero" to dwa różne
    /// zdania i tylko pierwsze z nich jest prawdziwe.
    #[must_use]
    pub fn observed(
        &self,
        district: DistrictId,
        good: GoodId,
        lag_days: u8,
    ) -> Option<ObservedPrice> {
        let w = self.okna.get(&(district, good))?;
        let lag = u32::from(lag_days.clamp(1, WINDOW_DAYS as u8 - 1));
        let doba = self.day.checked_sub(lag)?;
        if self.day.saturating_sub(w.last_day) as usize >= WINDOW_DAYS {
            return None;
        }
        let o = w.dni[(doba as usize) % WINDOW_DAYS];
        (!o.is_empty()).then_some(o)
    }

    /// Najświeższa obserwacja pary — wyłącznie dla panelu i testów. **Nie dla AI**:
    /// firma patrzy przez [`PublicMarketBoard::observed`], bo firma nie jest urzędem.
    #[must_use]
    pub fn latest(&self, district: DistrictId, good: GoodId) -> Option<ObservedPrice> {
        let w = self.okna.get(&(district, good))?;
        let o = w.dni[(w.last_day as usize) % WINDOW_DAYS];
        (!o.is_empty()).then_some(o)
    }
}

impl HashState for PublicMarketBoard {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.day);
        h.write_u32(self.okna.len() as u32);
        for ((d, g), w) in &self.okna {
            d.hash_state(h);
            g.hash_state(h);
            h.write_u32(w.last_day);
            for o in &w.dni {
                o.hash_state(h);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn odcisk(b: &PublicMarketBoard) -> magnat_core::hash::StateHash {
        let mut h = StateHasher::new();
        b.hash_state(&mut h);
        h.finish()
    }

    fn obs(cena: i64, ofert: u16) -> ObservedPrice {
        ObservedPrice {
            cheapest_net: Money(cena),
            median_net: Money(cena + 20),
            cheapest_site: None,
            offers: ofert,
        }
    }

    #[test]
    fn opoznienie_jest_cecha_patrzacego_a_nie_obrazu() {
        let mut b = PublicMarketBoard::new();
        for d in 0..7u32 {
            b.record(DistrictId(1), GoodId(3), d, obs(100 + i64::from(d), 2));
        }
        // Dzień 6 to „dziś". Czujna firma widzi wczoraj, gapowata — sprzed sześciu dób.
        assert_eq!(
            b.observed(DistrictId(1), GoodId(3), 1)
                .map(|o| o.cheapest_net),
            Some(Money(105))
        );
        assert_eq!(
            b.observed(DistrictId(1), GoodId(3), 6)
                .map(|o| o.cheapest_net),
            Some(Money(100))
        );
    }

    #[test]
    fn nikt_nie_sprzedaje_to_nie_to_samo_co_cena_zero() {
        let b = PublicMarketBoard::new();
        assert!(b.observed(DistrictId(1), GoodId(3), 1).is_none());

        let mut b = PublicMarketBoard::new();
        b.record(DistrictId(1), GoodId(3), 0, obs(0, 0));
        b.record(DistrictId(1), GoodId(3), 1, obs(100, 1));
        assert!(
            b.observed(DistrictId(1), GoodId(3), 1).is_none(),
            "doba pusta"
        );
    }

    #[test]
    fn przerwa_dluzsza_od_okna_czysci_pamiec() {
        let mut b = PublicMarketBoard::new();
        b.record(DistrictId(1), GoodId(3), 0, obs(100, 2));
        b.record(DistrictId(1), GoodId(3), 30, obs(500, 1));
        // Cena sprzed miesiąca nie ma prawa wrócić jako „sprzed sześciu dób".
        for lag in 1..7u8 {
            assert!(
                b.observed(DistrictId(1), GoodId(3), lag).is_none(),
                "lag {lag} wskrzesił starą cenę"
            );
        }
        assert_eq!(
            b.latest(DistrictId(1), GoodId(3)).map(|o| o.cheapest_net),
            Some(Money(500))
        );
    }

    #[test]
    fn luka_w_srodku_okna_nie_udaje_obserwacji() {
        let mut b = PublicMarketBoard::new();
        b.record(DistrictId(1), GoodId(3), 0, obs(100, 2));
        b.record(DistrictId(1), GoodId(3), 3, obs(200, 2));
        // Doby 1 i 2 nikt nie obserwował — mają być puste, a nie nieść ceny z doby 0.
        assert!(b.observed(DistrictId(1), GoodId(3), 1).is_none());
        assert!(b.observed(DistrictId(1), GoodId(3), 2).is_none());
        assert_eq!(
            b.observed(DistrictId(1), GoodId(3), 3)
                .map(|o| o.cheapest_net),
            Some(Money(100))
        );
    }

    #[test]
    fn hash_zalezy_od_tresci_i_od_kolejnosci_kluczy() {
        let mut a = PublicMarketBoard::new();
        a.record(DistrictId(1), GoodId(3), 0, obs(100, 2));
        let mut b = PublicMarketBoard::new();
        b.record(DistrictId(1), GoodId(3), 0, obs(100, 2));
        assert_eq!(odcisk(&a), odcisk(&b));

        b.record(DistrictId(1), GoodId(3), 0, obs(101, 2));
        assert_ne!(odcisk(&a), odcisk(&b));
    }
}
