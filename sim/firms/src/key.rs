//! `FirmKey`, sloty decyzyjne i scheduler trzech poziomów (M7a WP1, M7 §5.6).
//!
//! Klucz firmy jest **monotonicznym licznikiem świata**, a nie indeksem encji.
//! Powód jest twardy: indeks encji bywa recyklingowany po usunięciu, a ten klucz
//! steruje shardingiem decyzji i strumieniem RNG — firma założona po bankructwie
//! sąsiada odziedziczyłaby wtedy jego minutę decyzyjną i jego szum.
//!
//! **Budżet obliczeniowy wyraża się w liczbie firm na tick, nigdy w czasie
//! rzeczywistym** (M7 §5.6, dokument 00 §3.5). Zegar ścienny jest niedeterministyczny,
//! więc nie może sterować tym, ile decyzji zapadnie; czas mierzymy w benchmarkach.

use magnat_core::hash::{HashState, StateHasher};
use magnat_core::rng::mix64;
use magnat_core::{FirmId, SimCalendar};
use serde::{Deserialize, Serialize};

/// Stabilny klucz firmy: monotoniczny licznik świata, trwały w zapisie gry.
///
/// `FirmId` (encja) zostaje osobno, bo niosą go już księgi M5, partie M6 i sklepy —
/// klucz jest tożsamością w czasie, encja tożsamością w świecie.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash, Serialize, Deserialize)]
pub struct FirmKey(pub u64);

impl HashState for FirmKey {
    #[inline]
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.0);
    }
}

/// Licznik nadający klucze. Jeden na świat, wchodzi do hasha — dwa przebiegi
/// tego samego ziarna muszą nadać te same klucze tym samym firmom.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct KeyCounter(u64);

impl KeyCounter {
    /// Nadaje kolejny klucz. Zero zostaje wolne: pierwszy nadany klucz to 1,
    /// a sentinela nie ma, bo `Option<FirmKey>` jest tańszy.
    ///
    /// Nie `next()` — ta nazwa myli się z `Iterator::next`, a licznik iteratorem nie jest.
    pub fn issue(&mut self) -> FirmKey {
        self.0 += 1;
        FirmKey(self.0)
    }

    #[must_use]
    pub const fn issued(self) -> u64 {
        self.0
    }
}

impl HashState for KeyCounter {
    #[inline]
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.0);
    }
}

/// Trzy sloty decyzyjne firmy — kiedy w kalendarzu zapada decyzja każdego poziomu.
///
/// Funkcja czysta klucza, stała przez życie firmy: nie trzeba jej zapisywać,
/// nie może się rozjechać i nie zależy od kolejności zakładania firm.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct DecisionSlots {
    /// Minuta doby, w której zapada decyzja operacyjna (1 × doba).
    pub ops_minute_of_day: u16,
    /// Dzień miesiąca decyzji taktycznej, 0..30 (1 × miesiąc; kalendarz 12 × 30, `K-1`).
    pub tac_day_of_month: u8,
    pub tac_hour: u8,
    /// Dzień kwartału decyzji strategicznej, 0..90 (1 × kwartał).
    pub str_day_of_quarter: u8,
    pub str_hour: u8,
}

/// Poziom decyzji. Kolejność wariantów jest kontraktem: `as_index()` indeksuje
/// kubełki schedulera i histogram w panelu.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum Tier {
    Operational = 0,
    Tactical = 1,
    Strategic = 2,
}

impl Tier {
    pub const ALL: [Tier; 3] = [Tier::Operational, Tier::Tactical, Tier::Strategic];

    /// Sufit firm obsługiwanych w jednym ticku (M7 §5.6, „twardy limit awaryjny").
    /// Nadmiar przechodzi do następnego ticku kolejką FIFO posortowaną po `FirmKey`.
    #[must_use]
    pub const fn max_per_tick(self) -> usize {
        match self {
            Tier::Operational => 32,
            Tier::Tactical => 4,
            Tier::Strategic => 1,
        }
    }

    #[must_use]
    pub const fn as_index(self) -> usize {
        self as usize
    }

    /// Nazwa wariantu — klucz tekstu w `data/locale/`, ta sama konwencja
    /// co w słownikach `vocab_enum!` z `core`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Tier::Operational => "Operational",
            Tier::Tactical => "Tactical",
            Tier::Strategic => "Strategic",
        }
    }
}

/// Slot decyzyjny firmy — funkcja czysta klucza (M7 §5.6).
///
/// Rozkład jest równomierny i deterministyczny; firmy założone tego samego dnia
/// nie zlepiają się w jeden pik, bo klucz idzie przez `mix64`, a nie przez kolejny numer.
#[must_use]
pub fn slots(key: FirmKey) -> DecisionSlots {
    let h = mix64(key.0);
    DecisionSlots {
        ops_minute_of_day: (h % 1440) as u16,
        tac_day_of_month: ((h >> 12) % 30) as u8,
        tac_hour: ((h >> 20) % 24) as u8,
        str_day_of_quarter: ((h >> 28) % 90) as u8,
        str_hour: ((h >> 36) % 24) as u8,
    }
}

/// Czy firma decyduje na tym poziomie w tej chwili kalendarza.
///
/// Osobna funkcja czysta, bo pyta o nią i scheduler, i test, i karta inspekcji
/// („kiedy on znowu popatrzy na ceny").
#[must_use]
pub fn due(slots: DecisionSlots, tier: Tier, cal: SimCalendar) -> bool {
    match tier {
        Tier::Operational => cal.minute_of_day() == slots.ops_minute_of_day,
        // `SimCalendar` liczy dzień miesiąca i miesiąc roku od jedynki, slot od zera.
        Tier::Tactical => {
            cal.day_of_month() - 1 == slots.tac_day_of_month
                && cal.hour_of_day() == slots.tac_hour
                && cal.minute_of_hour() == 0
        }
        Tier::Strategic => {
            let doba_kwartalu =
                u16::from((cal.month_of_year() - 1) % 3) * 30 + u16::from(cal.day_of_month() - 1);
            doba_kwartalu == u16::from(slots.str_day_of_quarter)
                && cal.hour_of_day() == slots.str_hour
                && cal.minute_of_hour() == 0
        }
    }
}

/// Kolejka firm czekających na decyzję danego poziomu.
///
/// Trzy kolejki zamiast jednej, bo sufity są różne i nadmiar operacyjny nie może
/// wypchnąć decyzji strategicznej. FIFO po `FirmKey`, więc przesunięcie jest
/// deterministyczne i wchodzi do hasha stanu (M7 §8, ryzyko `R4`).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Scheduler {
    pending: [Vec<FirmKey>; 3],
}

impl Scheduler {
    #[must_use]
    pub fn new() -> Scheduler {
        Scheduler::default()
    }

    /// Dopisuje do kolejki firmy, którym wypadł slot. Wejście **musi** być
    /// posortowane po kluczu — sortuje je wywołujący, bo zna źródło (rejestr firm
    /// iteruje po `BTreeMap`, więc porządek ma za darmo).
    pub fn enqueue(&mut self, tier: Tier, keys: impl IntoIterator<Item = FirmKey>) {
        self.pending[tier.as_index()].extend(keys);
    }

    /// Zdejmuje z kolejki tyle firm, ile wolno obsłużyć w tym ticku.
    /// Reszta zostaje i wyjdzie w następnym — nigdy nie znika.
    pub fn take(&mut self, tier: Tier) -> Vec<FirmKey> {
        let q = &mut self.pending[tier.as_index()];
        let n = q.len().min(tier.max_per_tick());
        q.drain(..n).collect()
    }

    #[must_use]
    pub fn backlog(&self, tier: Tier) -> usize {
        self.pending[tier.as_index()].len()
    }
}

impl HashState for Scheduler {
    fn hash_state(&self, h: &mut StateHasher) {
        for tier in Tier::ALL {
            let q = &self.pending[tier.as_index()];
            h.write_u32(q.len() as u32);
            for k in q {
                k.hash_state(h);
            }
        }
    }
}

/// Encja firmy wyprowadzona z klucza.
///
/// Firmy **nie są encjami ECS** (nie mają komponentów — cały ich stan siedzi
/// w rejestrze), ale `FirmId` niosą już księgi M5, partie M6 i sklepy, więc musi
/// istnieć i musi być stabilny. Generacja stała, bo nie ma czego recyklingować.
///
// ponytail: sufit to 4 mld firm w historii jednego świata; gdyby kiedyś zabrakło,
// wyjściem jest mapa `FirmKey -> FirmId` w rejestrze, nie szersza encja.
#[must_use]
pub fn firm_id(key: FirmKey) -> FirmId {
    FirmId(magnat_core::Entity::new(
        key.0 as u32,
        std::num::NonZeroU32::MIN,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sloty_sa_funkcja_czysta_klucza() {
        let k = FirmKey(12_345);
        assert_eq!(slots(k), slots(k));
        // Sąsiednie klucze nie zlepiają się w jedną minutę.
        let a = slots(FirmKey(1)).ops_minute_of_day;
        let b = slots(FirmKey(2)).ops_minute_of_day;
        assert_ne!(a, b);
    }

    #[test]
    fn sloty_mieszcza_sie_w_kalendarzu() {
        for i in 1..20_000u64 {
            let s = slots(FirmKey(i));
            assert!(s.ops_minute_of_day < 1440);
            assert!(s.tac_day_of_month < 30);
            assert!(s.tac_hour < 24);
            assert!(s.str_day_of_quarter < 90);
            assert!(s.str_hour < 24);
        }
    }

    #[test]
    fn rozklad_minut_jest_rownomierny() {
        // 10 tys. firm na 1440 minut: średnio ~7 na minutę. Pik powyżej sufitu
        // 32 znaczyłby, że kolejka przepełnienia pracuje codziennie.
        let mut kubelki = vec![0u32; 1440];
        for i in 1..=10_000u64 {
            kubelki[slots(FirmKey(i)).ops_minute_of_day as usize] += 1;
        }
        let max = *kubelki.iter().max().expect("1440 kubełków");
        assert!(max <= Tier::Operational.max_per_tick() as u32, "pik {max}");
    }

    #[test]
    fn kolejka_nie_gubi_firm() {
        let mut s = Scheduler::new();
        let wszystkie: Vec<FirmKey> = (1..=100).map(FirmKey).collect();
        s.enqueue(Tier::Operational, wszystkie.clone());
        let mut wydane = Vec::new();
        while s.backlog(Tier::Operational) > 0 {
            wydane.extend(s.take(Tier::Operational));
        }
        assert_eq!(wydane, wszystkie, "FIFO po kluczu, nic nie znika");
    }

    #[test]
    fn licznik_kluczy_jest_monotoniczny() {
        let mut c = KeyCounter::default();
        assert_eq!(c.issue(), FirmKey(1));
        assert_eq!(c.issue(), FirmKey(2));
        assert_eq!(c.issued(), 2);
    }

    #[test]
    fn encja_firmy_jest_funkcja_klucza() {
        assert_eq!(firm_id(FirmKey(7)), firm_id(FirmKey(7)));
        assert_ne!(firm_id(FirmKey(7)), firm_id(FirmKey(8)));
    }
}
