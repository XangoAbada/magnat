//! Silnik zdarzeń dyskretnych (M3a §5.2, PRD §17.3, WP3).
//!
//! Koło czasu o 2880 kubełkach minutowych (dwie doby) plus `BTreeMap` przelewowy
//! na zdarzenia dalsze — urodziny, zgon, rocznice. Wybór jest podyktowany rozkładem:
//! 99 % zdarzeń dzieje się w ciągu najbliższych kilku godzin i trafia w kubełek
//! w czasie stałym, a te nieliczne, które sięgają dekad w przód, nie mają prawa
//! kosztować 43 200 pustych kubełków na miesiąc.
//!
//! **Kolejność jest kontraktem, nie szczegółem.** `order_key` daje porządek totalny
//! niezależny od kolejności wstawiania i od liczby wątków — bez tego dwa przebiegi
//! tego samego ziarna rozjeżdżałyby się na pierwszym remisie czasowym (00 §3.3).
//! Klucz wchodzi do sekcji „zamrożone" fazy (M3 §6.4): jego zmiana unieważnia
//! każdy istniejący zapis gry.
//!
//! **Lazy scheduling.** Planer nie wypycha dwunastu zdarzeń naraz — do kolejki trafia
//! wyłącznie **następne** zdarzenie mieszkańca, a jego obsługa harmonogramuje kolejne.
//! Kolejka trzyma wtedy ~1 zdarzenie na mieszkańca (400 tys. × 16 B = 6,4 MB) zamiast
//! dwunastu, a przeplanowanie nie musi niczego z niej usuwać: nieaktualne zdarzenie
//! rozpoznaje się przy obsłudze po parze `(plan_day, slot)`.

use magnat_core::{HashState, NeedKind, PlaceRef, StateHasher};
use std::collections::BTreeMap;

/// Pojemność koła czasu: dwie doby minut.
pub const WHEEL_MINUTES: usize = 2 * 1440;

/// O ile zdarzeń naraz rośnie kubełek koła czasu.
const BUCKET_GROW: usize = 64;

/// Rodzaj zdarzenia. **Kolejność wariantów jest kolejnością obsługi** przy remisie
/// czasowym (`order_key` stawia `kind` na najstarszych bitach): przybycie musi się
/// wykonać przed tikiem potrzeb, tik potrzeb przed rozpoczęciem czynności, a plan
/// na nowy dzień na samym końcu minuty.
///
/// Dyskryminatory są zamrożone (M3 §6.4) — kolejne podfazy i fazy dopisują **na końcu**.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum EventKind {
    /// Mieszkaniec dotarł do celu (`TravelOracle::begin_trip`).
    Arrive = 0,
    /// Godzinowy tik potrzeb dla agenta poza shardem systemowym.
    NeedTick = 1,
    /// Początek czynności ze slotu planu — tu woła się `PlaceProvider::fulfil`.
    StartActivity = 2,
    /// Koniec czynności; harmonogramuje następne zdarzenie mieszkańca.
    EndActivity = 3,
    /// Ułożenie planu na nową dobę.
    PlanDay = 4,
}

impl EventKind {
    #[must_use]
    pub const fn from_u8(v: u8) -> Option<EventKind> {
        match v {
            0 => Some(EventKind::Arrive),
            1 => Some(EventKind::NeedTick),
            2 => Some(EventKind::StartActivity),
            3 => Some(EventKind::EndActivity),
            4 => Some(EventKind::PlanDay),
            _ => None,
        }
    }
}

/// Zdarzenie — 16 B.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct SimEvent {
    /// `SimMinute` obcięte do `u32` (≈ 8100 lat gry).
    pub time: u32,
    /// Indeks encji aktora.
    pub actor: u32,
    /// `EventKind`.
    pub kind: u8,
    /// Indeks slotu planu; `SimEvent::NO_SLOT` = nie dotyczy.
    pub slot: u8,
    pub payload: u16,
    pub _pad: u32,
}

impl SimEvent {
    pub const NO_SLOT: u8 = 0xFF;

    #[must_use]
    pub fn new(time: u32, actor: u32, kind: EventKind, slot: u8) -> SimEvent {
        SimEvent {
            time,
            actor,
            kind: kind as u8,
            slot,
            payload: 0,
            _pad: 0,
        }
    }

    #[inline]
    #[must_use]
    pub const fn event_kind(&self) -> Option<EventKind> {
        EventKind::from_u8(self.kind)
    }
}

impl HashState for SimEvent {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.time);
        h.write_u32(self.actor);
        h.write(&[self.kind, self.slot]);
        h.write_u16(self.payload);
    }
}

/// Klucz porządkujący zdarzenia w obrębie jednej minuty (§5.2).
///
/// **Zamrożony (M3 §6.4).** Porządek jest totalny: dwa zdarzenia o identycznym kluczu
/// są zabronione, bo jeden aktor nie może mieć dwóch zdarzeń tego samego rodzaju
/// i slotu w tej samej minucie. Gdyby mógł, kolejność ich obsługi zależałaby od
/// kolejności wstawiania — czyli od liczby wątków.
#[inline]
#[must_use]
pub const fn order_key(e: &SimEvent) -> u64 {
    ((e.kind as u64) << 48) | ((e.actor as u64) << 16) | (e.slot as u64)
}

/// Kolejka zdarzeń: koło czasu + przelew.
#[derive(Clone, Debug)]
pub struct EventQueue {
    wheel: Vec<Vec<SimEvent>>,
    overflow: BTreeMap<u32, Vec<SimEvent>>,
    /// Minuta **aktualnie obsługiwana**, nie następna (korekta D-14). Handler musi
    /// widzieć zegar na swojej minucie, bo to od niego liczy się czas przybycia:
    /// `TravelOracle::begin_trip` harmonogramuje `Arrive` na `now + minutes`, a gdyby
    /// `now` było już minutę do przodu, każda podróż w symulacji trwałaby o minutę
    /// dłużej niż w planie — i planer rozjechałby się z ruchem o jedną minutę na dojazd.
    now: u32,
    /// Czy minuta `now` została już rozdana. Dopóki nie, wolno na nią harmonogramować.
    dispatched: bool,
    len: usize,
}

impl Default for EventQueue {
    fn default() -> Self {
        EventQueue::new()
    }
}

impl EventQueue {
    #[must_use]
    pub fn new() -> EventQueue {
        EventQueue {
            wheel: vec![Vec::new(); WHEEL_MINUTES],
            overflow: BTreeMap::new(),
            now: 0,
            dispatched: false,
            len: 0,
        }
    }

    #[inline]
    #[must_use]
    pub const fn now(&self) -> u32 {
        self.now
    }

    #[inline]
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[inline]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Wstawia zdarzenie. Zdarzenie z przeszłości jest błędem wywołującego — kolejka
    /// nie ma jak go obsłużyć, a ciche przesunięcie na „teraz" zmieniłoby wynik
    /// symulacji w sposób zależny od momentu, w którym błąd powstał.
    ///
    /// **Minuta już rozdana też jest przeszłością.** Kubełek bieżącej minuty został
    /// opróżniony, więc zdarzenie dopisane do niego z handlera nie wyszłoby nigdy —
    /// zginęłoby po cichu, razem z licznikiem długości kolejki. Handler, który chce
    /// „zaraz potem", harmonogramuje na `now + 1` (korekta D-14).
    pub fn schedule(&mut self, e: SimEvent) {
        assert!(
            e.time > self.now || !self.dispatched,
            "zdarzenie na minutę {} przy zegarze {} (minuta już rozdana)",
            e.time,
            self.now
        );
        let delta = e.time - self.now;
        if (delta as usize) < WHEEL_MINUTES {
            let bucket = &mut self.wheel[e.time as usize % WHEEL_MINUTES];
            // Wzrost porcjami zamiast podwajania: kubełek przy 400 tys. mieszkańców
            // ma rzędu 300 zdarzeń, a podwajanie zostawia w nim drugie tyle pustego
            // miejsca — 14 B na mieszkańca z budżetu §17.7 (pomiar M3a).
            if bucket.len() == bucket.capacity() {
                bucket.reserve_exact(BUCKET_GROW);
            }
            bucket.push(e);
        } else {
            self.overflow.entry(e.time).or_default().push(e);
        }
        self.len += 1;
    }

    /// Zdarzenia bieżącej minuty w porządku totalnym; przesuwa zegar o minutę.
    ///
    /// `out` jest czyszczony na wejściu — wywołujący trzyma jeden bufor przez całą
    /// symulację i nie alokuje w pętli gorącej.
    pub fn drain_minute(&mut self, out: &mut Vec<SimEvent>) {
        out.clear();
        if self.dispatched {
            self.now += 1;
        }
        self.dispatched = true;
        let minuta = self.now;

        // Przelew: wszystko, co właśnie weszło w zasięg koła, przepisujemy do kubełków.
        // Robione przy okazji minuty, a nie osobnym przebiegiem, bo `BTreeMap` i tak
        // trzeba odpytać o pierwszy klucz.
        while let Some((&t, _)) = self.overflow.iter().next() {
            if t >= minuta.saturating_add(WHEEL_MINUTES as u32) {
                break;
            }
            let zdarzenia = self.overflow.remove(&t).unwrap_or_default();
            let kubelek = t as usize % WHEEL_MINUTES;
            self.wheel[kubelek].extend(zdarzenia);
        }

        let kubelek = minuta as usize % WHEEL_MINUTES;
        // Kubełek może zawierać zdarzenia z minuty o 2880 dalej (to samo `% 2880`),
        // jeśli ktoś zaplanował dokładnie dwie doby w przód — bierzemy tylko te na teraz.
        let bucket = &mut self.wheel[kubelek];
        let mut i = 0;
        while i < bucket.len() {
            if bucket[i].time == minuta {
                out.push(bucket.swap_remove(i));
            } else {
                i += 1;
            }
        }
        self.len -= out.len();

        out.sort_unstable_by_key(order_key);
        debug_assert!(
            out.windows(2).all(|w| order_key(&w[0]) != order_key(&w[1])),
            "dwa zdarzenia o identycznym kluczu w minucie {minuta} — porządek przestał \
             być totalny, a wynik zaczął zależeć od kolejności wstawiania"
        );
    }

    /// Zaalokowane bajty — wejście do rachunku budżetu §17.7.
    #[must_use]
    pub fn allocated_bytes(&self) -> usize {
        let kolo: usize = self
            .wheel
            .iter()
            .map(|b| b.capacity() * size_of::<SimEvent>())
            .sum();
        let przelew: usize = self
            .overflow
            .values()
            .map(|b| b.capacity() * size_of::<SimEvent>() + 32)
            .sum();
        kolo + przelew + self.wheel.capacity() * size_of::<Vec<SimEvent>>()
    }
}

impl HashState for EventQueue {
    /// Kolejka jest stanem trwałym: to, co się wydarzy jutro, jest częścią świata
    /// tak samo jak to, co jest dziś. Hash idzie po **posortowanych** zdarzeniach,
    /// bo kubełek trzyma je w kolejności wstawiania, a ta zależy od liczby wątków.
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.now);
        h.write_u8(u8::from(self.dispatched));
        h.write_u64(self.len as u64);
        let mut wszystkie: Vec<&SimEvent> = self
            .wheel
            .iter()
            .flatten()
            .chain(self.overflow.values().flatten())
            .collect();
        wszystkie.sort_unstable_by_key(|e| (e.time, order_key(e)));
        for e in wszystkie {
            e.hash_state(h);
        }
    }
}

/// Powód przeplanowania dnia (§5.2).
///
/// Enum żyje w `sim/agents`, a nie w `core`: jest narzędziem planera, nie słownikiem
/// dzielonym przez fazy. Warianty, których M3 nie wypełnia (`WeatherChanged`), są tu
/// po to, żeby M8 dopisał obsługę, a nie własny mechanizm obok.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReplanCause {
    /// Przybycie później niż ETA (M4: korek).
    Late {
        delay_min: u16,
    },
    PlaceClosed {
        place: PlaceRef,
    },
    /// M5: brak towaru albo za drogo.
    PlaceRefused {
        place: PlaceRef,
    },
    /// Potrzeba spadła poniżej progu awaryjnego.
    NeedCritical {
        need: NeedKind,
    },
    TripBlocked,
    HouseholdEvent {
        kind: HhEventKind,
    },
    ShiftChanged,
    /// M8 wypełnia; M3 zostawia wariant.
    WeatherChanged,
}

impl ReplanCause {
    /// Czy powód wywraca **zobowiązania** (fazę 1 planera), czy tylko zadania i czas
    /// wolny. Od tego zależy, czy przeplanowanie jest przyrostowe, czy pełne (§5.2).
    #[must_use]
    pub const fn is_full_replan(&self) -> bool {
        matches!(
            self,
            ReplanCause::ShiftChanged
                | ReplanCause::HouseholdEvent {
                    kind: HhEventKind::Death
                }
        )
    }

    /// Skrót do pola `PlanSlot.reason` i do logu — jeden bajt zamiast całego wariantu.
    #[must_use]
    pub const fn tag(&self) -> u8 {
        match self {
            ReplanCause::Late { .. } => 0,
            ReplanCause::PlaceClosed { .. } => 1,
            ReplanCause::PlaceRefused { .. } => 2,
            ReplanCause::NeedCritical { .. } => 3,
            ReplanCause::TripBlocked => 4,
            ReplanCause::HouseholdEvent { .. } => 5,
            ReplanCause::ShiftChanged => 6,
            ReplanCause::WeatherChanged => 7,
        }
    }
}

/// Zdarzenie w gospodarstwie domowym, które może wywrócić plan dnia jego członkom.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HhEventKind {
    Birth,
    ChildIll,
    Death,
    MemberJoined,
    MemberLeft,
    Moved,
}

/// Debouncing przeplanowań: 15 minut gry (§5.2, ryzyko R3).
pub const REPLAN_COOLDOWN_MIN: u8 = 15;

/// Twardy limit zgłoszeń przeplanowania na tick; nadmiar przechodzi FIFO na następny
/// (§5.2 pkt 3). Zamiast spajku klatki przy zdarzeniu miejskim dotykającym setek tysięcy
/// agentów mamy rozłożenie kosztu na kilka ticków.
pub const REPLAN_BUDGET_PER_TICK: usize = 20_000;

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(time: u32, actor: u32, kind: EventKind) -> SimEvent {
        SimEvent::new(time, actor, kind, SimEvent::NO_SLOT)
    }

    #[test]
    fn rozmiar_zdarzenia_zgadza_sie_z_budzetem() {
        assert_eq!(size_of::<SimEvent>(), 16);
    }

    #[test]
    fn kolejnosc_nie_zalezy_od_kolejnosci_wstawiania() {
        // WP3: 100 tys. zdarzeń w tej samej minucie, wstawione w dwóch różnych
        // kolejnościach, muszą wyjść identycznie.
        let zbior: Vec<SimEvent> = (0..100_000u32)
            .map(|i| {
                let kind = match i % 4 {
                    0 => EventKind::Arrive,
                    1 => EventKind::NeedTick,
                    2 => EventKind::StartActivity,
                    _ => EventKind::PlanDay,
                };
                ev(0, i, kind)
            })
            .collect();

        let przebieg = |kolejnosc: Box<dyn Iterator<Item = &SimEvent>>| {
            let mut q = EventQueue::new();
            for e in kolejnosc {
                q.schedule(*e);
            }
            let mut out = Vec::new();
            q.drain_minute(&mut out);
            out
        };

        let a = przebieg(Box::new(zbior.iter()));
        let b = przebieg(Box::new(zbior.iter().rev()));
        assert_eq!(a.len(), zbior.len());
        assert_eq!(a, b, "kolejność obsługi zależy od kolejności wstawiania");

        // I jest to kolejność semantyczna: Arrive przed NeedTick przed StartActivity.
        assert_eq!(a[0].event_kind(), Some(EventKind::Arrive));
        assert_eq!(a.last().unwrap().event_kind(), Some(EventKind::PlanDay));
    }

    #[test]
    fn przelew_wraca_w_swojej_minucie() {
        let mut q = EventQueue::new();
        let daleko = WHEEL_MINUTES as u32 * 3 + 7; // poza kołem, więc do BTreeMap
        q.schedule(ev(daleko, 1, EventKind::PlanDay));
        q.schedule(ev(5, 2, EventKind::Arrive));
        assert_eq!(q.len(), 2);

        let mut out = Vec::new();
        for _ in 0..6 {
            q.drain_minute(&mut out);
        }
        assert_eq!(out.len(), 1, "zdarzenie z koła nie wyszło w swojej minucie");
        assert_eq!(out[0].actor, 2);

        // Przewijamy do minuty zdarzenia dalekiego — musi się pojawić dokładnie raz.
        let mut trafienia = 0;
        while q.now() < daleko {
            q.drain_minute(&mut out);
            trafienia += out.iter().filter(|e| e.actor == 1).count();
        }
        assert_eq!(
            trafienia, 1,
            "zdarzenie z przelewu zgubiło się albo podwoiło"
        );
        assert!(q.is_empty());
    }

    #[test]
    fn zdarzenie_dokladnie_dwie_doby_w_przod_nie_wychodzi_za_wczesnie() {
        // Kubełek jest wspólny dla minut różniących się o 2880 — bez sprawdzenia
        // `time == minuta` zdarzenie wyszłoby natychmiast, dwie doby przed czasem.
        let mut q = EventQueue::new();
        q.schedule(ev(WHEEL_MINUTES as u32, 1, EventKind::PlanDay));
        let mut out = Vec::new();
        q.drain_minute(&mut out);
        assert!(out.is_empty(), "zdarzenie wyszło 2880 minut za wcześnie");
        assert_eq!(q.len(), 1);
    }

    #[test]
    fn zegar_stoi_na_minucie_ktora_wlasnie_rozdaje() {
        // Korekta D-14: handler widzi zegar na **swojej** minucie, bo to od niego
        // liczy czas przybycia. Gdyby `now` było już minutę dalej, każda podróż
        // trwałaby o minutę dłużej niż w planie.
        let mut q = EventQueue::new();
        q.schedule(ev(5, 1, EventKind::StartActivity));
        let mut out = Vec::new();
        for oczekiwana in 0..=5u32 {
            q.drain_minute(&mut out);
            assert_eq!(q.now(), oczekiwana);
        }
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].time, q.now());
        // „Zaraz potem" to `now + 1`, nie `now`: kubełek bieżącej minuty jest już pusty.
        q.schedule(ev(q.now() + 1, 1, EventKind::EndActivity));
        q.drain_minute(&mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].event_kind(), Some(EventKind::EndActivity));
    }

    #[test]
    #[should_panic(expected = "minuta już rozdana")]
    fn zdarzenie_na_minute_juz_rozdana_nie_ginie_po_cichu() {
        let mut q = EventQueue::new();
        let mut out = Vec::new();
        q.drain_minute(&mut out);
        q.schedule(ev(0, 1, EventKind::Arrive));
    }

    #[test]
    fn hash_kolejki_nie_zalezy_od_kolejnosci_wstawiania() {
        let odcisk = |odwrotnie: bool| {
            let mut q = EventQueue::new();
            let mut zdarzenia: Vec<SimEvent> = (0..1000u32)
                .map(|i| ev(i % 100, i, EventKind::Arrive))
                .collect();
            if odwrotnie {
                zdarzenia.reverse();
            }
            for e in zdarzenia {
                q.schedule(e);
            }
            let mut h = StateHasher::new();
            q.hash_state(&mut h);
            h.finish()
        };
        assert_eq!(odcisk(false), odcisk(true));
    }

    #[test]
    fn przeplanowanie_pelne_tylko_gdy_padlo_zobowiazanie() {
        assert!(ReplanCause::ShiftChanged.is_full_replan());
        assert!(ReplanCause::HouseholdEvent {
            kind: HhEventKind::Death
        }
        .is_full_replan());
        assert!(!ReplanCause::Late { delay_min: 30 }.is_full_replan());
        assert!(!ReplanCause::HouseholdEvent {
            kind: HhEventKind::Birth
        }
        .is_full_replan());
    }
}
