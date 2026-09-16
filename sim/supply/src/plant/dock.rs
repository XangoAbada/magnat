//! Rampa: kolejka M/D/c z godzinami dostaw (M6b §5.5, WP4).
//!
//! **Zakład trzyma kolejkę, M4 dowozi i odbiera czas zwolnienia.** M4 odmówił budowania
//! osobnego modelu ruchu dla ciężarówek — drugi model to drugie miejsce, w którym może
//! pęknąć tolerancja 0 mikro↔mezo — i słusznie: kolejka rampy jest nasza, przejazd jego.
//!
//! `release_at` wchodzi do księgi, czyli **dotyka pieniądza**, więc obowiązują cztery
//! warunki brzegowe z §5.5:
//!
//! 1. **Deterministyczne i niezależne od LOD.** Mikro odgrywa stojącą ciężarówkę
//!    wizualnie, ale nie decyduje, kiedy odjedzie — decyduje kolejka, identycznie
//!    w mikro i w mezo.
//! 2. **Klucz kolejki jest totalny:** `(minuta przybycia, indeks encji pojazdu)`.
//!    Porządek po `TransportOrderId` byłby błędem, bo jeden pojazd wiezie kilka zleceń
//!    (milk-run, §5.6) i przy konsolidacji dałby remis rozstrzygany przypadkowo.
//! 3. **Przepełnienie placu wylewa się na ulicę** — pojazdy ponad `yard_capacity`
//!    zajmują pojemność przyuliczną i biorą udział w zatorze. To jedyny punkt,
//!    w którym ta kolejka dotyka sieci drogowej, i jest pożądany: zastawiona ulica
//!    pod źle zaprojektowaną hurtownią to czytelny sygnał, że rampa jest za mała.
//! 4. Strażnikiem jest test M4 `ramp_wait_lod_invariant`.

use magnat_core::{
    DayOfWeek, HashState, Mass, MinuteOfDay, OpenHours, SimCalendar, SimMinute, SiteId,
    StateHasher, Tick, VehicleId,
};

use crate::batch::TransportOrderId;
use crate::tuning::DockTuning;

/// Ile stanowisk mieści najbardziej rozbudowana rampa. Terminal paliwowy z §7.2 ma
/// sześć; osiem daje zapas bez rozdmuchiwania rekordu.
pub const MAX_BAYS: usize = 8;

/// M4 → M6 przy dojeździe pojazdu na teren zakładu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct VehicleArrivedAtSite {
    pub vehicle: VehicleId,
    pub site: SiteId,
    pub order: TransportOrderId,
    pub at: SimMinute,
    /// Masa do przeładowania — to ona, a nie zlecenie, decyduje o czasie obsługi.
    pub mass: Mass,
}

/// M6 → M4 w odpowiedzi. M4 księguje oczekiwanie jako pozycję w rejestrze przejazdu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SiteDwellResponse {
    pub release_at: SimMinute,
    /// Paliwo spalone na postoju — wraca do M4, bo to jego rejestr przejazdu.
    pub idle_fuel: magnat_core::Volume,
    /// Pojazd nie zmieścił się na placu i stoi na krawędzi dostępowej, biorąc udział
    /// w zatorze. Warunek brzegowy 3.
    pub on_street: bool,
}

/// Pojazd czekający albo obsługiwany. Klucz porządku jest w pierwszych dwóch polach
/// i jest **totalny** — warunek brzegowy 2.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DockEntry {
    pub arrived: SimMinute,
    pub vehicle_index: u32,
    pub order: TransportOrderId,
    pub release_at: SimMinute,
}

/// Rampa — realne wąskie gardło (§7.3).
#[derive(Clone, Debug)]
pub struct Dock {
    pub bays: u8,
    /// Podjazd, dokumenty — czas niezależny od masy.
    pub fixed_minutes: u16,
    pub minutes_per_tonne: u16,
    /// Godziny dostaw. Ograniczenie z §8.3; do M8 z danych zakładu, potem z przepisu.
    pub hours: OpenHours,
    /// Ile pojazdów mieści plac poza stanowiskami.
    pub yard_capacity: u8,
    queue: Vec<DockEntry>,
    busy_until: [SimMinute; MAX_BAYS],
}

impl Dock {
    #[must_use]
    pub fn new(bays: u8, fixed_minutes: u16, minutes_per_tonne: u16, hours: OpenHours) -> Dock {
        Dock {
            bays: bays.min(MAX_BAYS as u8).max(1),
            fixed_minutes,
            minutes_per_tonne,
            hours,
            yard_capacity: 4,
            queue: Vec::new(),
            busy_until: [SimMinute(0); MAX_BAYS],
        }
    }

    /// Czas obsługi pojazdu o tej masie.
    #[must_use]
    pub fn service_minutes(&self, mass: Mass) -> u32 {
        let tony = mass.0 / 1_000_000;
        u32::from(self.fixed_minutes)
            + (tony * i64::from(self.minutes_per_tonne)).clamp(0, i64::from(u32::MAX)) as u32
    }

    /// Ilu czeka albo jest obsługiwanych w tej minucie.
    #[must_use]
    pub fn occupancy(&self, now: SimMinute) -> usize {
        self.queue.iter().filter(|e| e.release_at.0 > now.0).count()
    }

    /// Przyjmuje pojazd i **od razu** mówi, kiedy odjedzie.
    ///
    /// Odpowiedź jest ostateczna, bo pojazd, który przyjedzie później, nie może
    /// przyspieszyć ani opóźnić tego, który stoi już w kolejce — FIFO po totalnym
    /// kluczu. Dlatego wołający **musi** podawać przybycia w kolejności tego klucza;
    /// system transportowy je sortuje, zanim tu zawoła.
    pub fn arrive(&mut self, ev: &VehicleArrivedAtSite, tuning: &DockTuning) -> SiteDwellResponse {
        self.prune(ev.at);
        let przed = self.occupancy(ev.at);

        // Najwcześniej zwalniające się stanowisko. Remis rozstrzyga niższy indeks,
        // więc wybór jest funkcją stanu, a nie kolejności iteracji.
        let bay = (0..self.bays as usize)
            .min_by_key(|i| (self.busy_until[*i].0, *i))
            .unwrap_or(0);
        let wolne_od = self.busy_until[bay].0.max(ev.at.0);
        let start = self.next_open_minute(SimMinute(wolne_od));
        let release = SimMinute(start.0 + u64::from(self.service_minutes(ev.mass)));
        self.busy_until[bay] = release;

        let wpis = DockEntry {
            arrived: ev.at,
            vehicle_index: ev.vehicle.0.index(),
            order: ev.order,
            release_at: release,
        };
        let k = (wpis.arrived.0, wpis.vehicle_index);
        let poz = self
            .queue
            .partition_point(|e| (e.arrived.0, e.vehicle_index) < k);
        self.queue.insert(poz, wpis);

        let czekanie = start.0.saturating_sub(ev.at.0);
        SiteDwellResponse {
            release_at: release,
            idle_fuel: magnat_core::Volume(czekanie as i64 * tuning.idle_fuel_ml_per_minute),
            on_street: przed >= usize::from(self.bays) + usize::from(self.yard_capacity),
        }
    }

    /// Najbliższa minuta, w której rampa przyjmuje. Ciężarówka, która przyjechała
    /// po zamknięciu, **czeka do otwarcia** — nie znika i nie rozładowuje się nocą.
    fn next_open_minute(&self, at: SimMinute) -> SimMinute {
        if self.hours.is_always() {
            return at;
        }
        let k = SimCalendar::from_minute(at);
        match self.hours.next_open(k.day_index(), k.minute_of_day_typed()) {
            Some((doba, minuta)) => SimMinute(doba * 1_440 + u64::from(minuta.get())),
            // Maska dni pusta — rampa nie otworzy się nigdy. Zwracamy `at`, bo
            // zwrócenie nieskończoności zamieniłoby błąd danych w zawieszony pojazd.
            None => at,
        }
    }

    fn prune(&mut self, now: SimMinute) {
        self.queue.retain(|e| e.release_at.0 > now.0);
    }

    /// Kolejka do panelu zakładu — w porządku totalnego klucza.
    #[must_use]
    pub fn queue(&self) -> &[DockEntry] {
        &self.queue
    }
}

impl HashState for Dock {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(self.bays);
        h.write_u16(self.fixed_minutes);
        h.write_u16(self.minutes_per_tonne);
        h.write_u16(self.hours.open.get());
        h.write_u16(self.hours.close.get());
        h.write_u8(self.hours.days);
        h.write_u8(self.yard_capacity);
        h.write_u32(self.queue.len() as u32);
        for e in &self.queue {
            e.arrived.hash_state(h);
            h.write_u32(e.vehicle_index);
            h.write_u32(e.order.0);
            e.release_at.hash_state(h);
        }
        for b in &self.busy_until {
            b.hash_state(h);
        }
    }
}

/// Dzień tygodnia minuty — skrót do godzin dostaw, żeby wołający nie składał kalendarza.
#[must_use]
pub fn day_of(at: SimMinute) -> (DayOfWeek, MinuteOfDay) {
    let k = SimCalendar::new(Tick(at.0));
    (k.day_of_week(), k.minute_of_day_typed())
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::Entity;
    use std::num::NonZeroU32;

    fn pojazd(i: u32) -> VehicleId {
        VehicleId(Entity::new(i, NonZeroU32::new(1).expect("generacja")))
    }

    fn zdarzenie(i: u32, at: u64, tony: i64) -> VehicleArrivedAtSite {
        VehicleArrivedAtSite {
            vehicle: pojazd(i),
            site: SiteId(Entity::new(1, NonZeroU32::new(1).expect("generacja"))),
            order: TransportOrderId(i),
            at: SimMinute(at),
            mass: Mass(tony * 1_000_000),
        }
    }

    fn strojenie() -> DockTuning {
        DockTuning {
            idle_fuel_ml_per_minute: 25,
        }
    }

    /// Rampa o jednym stanowisku szereguje pojazdy, a nie obsługuje ich naraz.
    /// To jest cały sens wąskiego gardła z §7.3.
    #[test]
    fn jedno_stanowisko_szereguje_a_nie_zwielokrotnia() {
        let mut d = Dock::new(1, 10, 1, OpenHours::ALWAYS);
        let a = d.arrive(&zdarzenie(1, 100, 24), &strojenie());
        let b = d.arrive(&zdarzenie(2, 100, 24), &strojenie());
        assert_eq!(a.release_at, SimMinute(134), "10 min + 24 t × 1 min/t");
        assert_eq!(b.release_at, SimMinute(168), "drugi czeka na pierwszego");
        assert_eq!(a.idle_fuel, magnat_core::Volume(0));
        assert_eq!(
            b.idle_fuel,
            magnat_core::Volume(34 * 25),
            "34 minuty postoju"
        );
    }

    /// Dwa stanowiska obsługują dwa pojazdy równolegle — i przydział stanowiska
    /// jest funkcją stanu, a nie kolejności iteracji.
    #[test]
    fn dwa_stanowiska_obsluguja_rownolegle() {
        let mut d = Dock::new(2, 10, 1, OpenHours::ALWAYS);
        let a = d.arrive(&zdarzenie(1, 0, 10), &strojenie());
        let b = d.arrive(&zdarzenie(2, 0, 10), &strojenie());
        assert_eq!(a.release_at, b.release_at);
        assert_eq!(d.occupancy(SimMinute(5)), 2);
        assert_eq!(d.occupancy(SimMinute(25)), 0);
    }

    /// Ciężarówka, która przyjechała po zamknięciu, **czeka do otwarcia**.
    /// Alternatywą jest rozładunek o trzeciej w nocy pod oknami mieszkańców.
    #[test]
    fn po_zamknieciu_ciezarowka_czeka_do_rana() {
        // Doba 0 to poniedziałek (K-15). Rampa czynna 6:00–14:00 codziennie.
        let mut d = Dock::new(
            1,
            10,
            0,
            OpenHours::new(6 * 60, 14 * 60, OpenHours::ALL_DAYS),
        );
        let r = d.arrive(&zdarzenie(1, 20 * 60, 5), &strojenie());
        assert_eq!(
            r.release_at,
            SimMinute(24 * 60 + 6 * 60 + 10),
            "obsługa zaczyna się o 6:00 następnej doby"
        );
        assert!(r.idle_fuel.0 > 0, "postój nocny kosztuje paliwo");
    }

    /// Przepełnienie placu wylewa się na ulicę — warunek brzegowy 3.
    #[test]
    fn przepelniony_plac_wylewa_sie_na_ulice() {
        let mut d = Dock::new(1, 60, 0, OpenHours::ALWAYS);
        d.yard_capacity = 2;
        let mut na_ulicy = Vec::new();
        for i in 1..=5 {
            na_ulicy.push(d.arrive(&zdarzenie(i, 0, 1), &strojenie()).on_street);
        }
        assert_eq!(
            na_ulicy,
            vec![false, false, false, true, true],
            "1 stanowisko + 2 miejsca na placu = czwarty stoi na ulicy"
        );
    }

    /// Ta sama sekwencja przybyć daje ten sam wynik niezależnie od tego, czy minuty
    /// przyszły po jednej, czy hurtem — na tym stoi `ramp_wait_lod_invariant` po
    /// stronie M4.
    #[test]
    fn wynik_nie_zalezy_od_ziarnistosci_wolania() {
        let sekwencja = [(1u32, 0u64, 8i64), (2, 3, 12), (3, 3, 4), (4, 40, 20)];
        let policz = |d: &mut Dock| {
            sekwencja
                .iter()
                .map(|(i, at, t)| d.arrive(&zdarzenie(*i, *at, *t), &strojenie()).release_at)
                .collect::<Vec<_>>()
        };
        let mut a = Dock::new(2, 12, 1, OpenHours::ALWAYS);
        let mut b = Dock::new(2, 12, 1, OpenHours::ALWAYS);
        // Drugi przebieg z „przewijaniem" kolejki w międzyczasie — czyszczenie
        // historii nie ma prawa zmienić ani jednej minuty zwolnienia.
        let wynik_a = policz(&mut a);
        let wynik_b = {
            let mut v = Vec::new();
            for (i, at, t) in sekwencja {
                v.push(b.arrive(&zdarzenie(i, at, t), &strojenie()).release_at);
                b.prune(SimMinute(at));
            }
            v
        };
        assert_eq!(wynik_a, wynik_b);
    }
}
