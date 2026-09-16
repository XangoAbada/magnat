//! Stanowiska, umowy i lista płac (M7a WP3, M7 §5.3, PRD §7.5).
//!
//! ## Dwa rekordy zatrudnienia i dlaczego oba są potrzebne (decyzja otwarta `D1`)
//!
//! `sim/agents::Employment` (12 B, komponent ECS) jest **linkiem**: mieszkaniec wie,
//! gdzie pracuje, w jakiej roli i na jakiej zmianie, bo pyta o to planer doby M3
//! i wybór środka transportu M4 — miliony razy na dobę gry i zawsze o tego jednego
//! człowieka. [`Employment`] tutaj jest **rekordem**: płaca, staż, benefity, ocena.
//! Jedno źródło prawdy o zatrudnieniu jest po stronie firmy; komponent agenta
//! jest denormalizacją pod szybkie zapytanie i nie niesie ani grosza.
//!
//! ## Wypłata
//!
//! Lista płac **nie księguje sama** — produkuje listę faktów, którą księguje `sim/economy`
//! (M5 jest właścicielem pieniądza). Ten sam wzorzec, którym M6 oddaje rozliczenia B2B
//! i rachunki za media: gdyby `sim/firms` sięgnął po `Books`, zależność poszłaby
//! `firms → economy`, a idzie i musi iść odwrotnie, bo to rynek pracy M7b stoi
//! na maszynerii ofert M5.

use magnat_agents::ShiftKind;
use magnat_core::hash::{HashState, StateHasher};
use magnat_core::rng::mix64;
use magnat_core::{CitizenId, JobRoleId, Money, SimMinute, SiteId};

use crate::key::FirmKey;

/// Świadczenia pozapłacowe — bitflagi, bo pytanie brzmi „czy ma", nie „ile".
/// Treść (realne zaspokojenie potrzeb, koszt dla firmy) dokłada M7b WP6.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct BenefitSet(pub u8);

impl BenefitSet {
    pub const NONE: BenefitSet = BenefitSet(0);
    pub const COMPANY_CAR: u8 = 1 << 0;
    pub const HEALTH: u8 = 1 << 1;
    pub const MEALS: u8 = 1 << 2;
    pub const TRAINING: u8 = 1 << 3;

    #[must_use]
    pub const fn has(self, flag: u8) -> bool {
        self.0 & flag != 0
    }
}

impl HashState for BenefitSet {
    #[inline]
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(self.0);
    }
}

/// Umowa o pracę — rekord po stronie firmy.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Employment {
    pub citizen: CitizenId,
    pub role: JobRoleId,
    /// Brutto, miesięcznie. Potrącenia to hook M8 (decyzja otwarta `D6`):
    /// do tego czasu brutto == netto i podpis `payroll` się przez to nie zmieni.
    pub wage_month: Money,
    pub since: SimMinute,
    pub shift: ShiftKind,
    pub benefits: BenefitSet,
    /// Wygładzona ocena wyniku, 0..=1000. Prowadzi ją M7b (premie, zwolnienia);
    /// nowa umowa startuje od środka skali, bo nikt nie zna jeszcze tego człowieka.
    pub perf_ema: u16,
    pub warnings: u8,
}

impl Employment {
    pub const PERF_START: u16 = 500;

    #[must_use]
    pub fn new(
        citizen: CitizenId,
        role: JobRoleId,
        wage_month: Money,
        since: SimMinute,
        shift: ShiftKind,
    ) -> Employment {
        Employment {
            citizen,
            role,
            wage_month,
            since,
            shift,
            benefits: BenefitSet::NONE,
            perf_ema: Employment::PERF_START,
            warnings: 0,
        }
    }
}

impl HashState for Employment {
    fn hash_state(&self, h: &mut StateHasher) {
        self.citizen.0.hash_state(h);
        self.role.hash_state(h);
        self.wage_month.hash_state(h);
        self.since.hash_state(h);
        h.write_u8(self.shift as u8);
        self.benefits.hash_state(h);
        h.write_u16(self.perf_ema);
        h.write_u8(self.warnings);
    }
}

/// Stanowisko: rola, ile etatów, kto je zajmuje.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Position {
    pub role: JobRoleId,
    pub slots: u16,
    pub filled: Vec<Employment>,
    pub managerial: bool,
    /// Widełki stawki miesięcznej, z których wychodzi oferta pracy (M7b).
    /// **Nie są tabelą płac** — są przedziałem, w którym firma wolno jej licytować.
    pub wage_band: (Money, Money),
}

impl Position {
    #[must_use]
    pub fn new(
        role: JobRoleId,
        slots: u16,
        managerial: bool,
        wage_band: (Money, Money),
    ) -> Position {
        Position {
            role,
            slots,
            filled: Vec::new(),
            managerial,
            wage_band,
        }
    }

    /// Ile etatów stoi pustych. Wakat nieobsadzony jest **poprawnym wynikiem**,
    /// a nie błędem (M7 §7.1 pkt 4).
    #[must_use]
    pub fn vacancies(&self) -> u16 {
        self.slots.saturating_sub(self.filled.len() as u16)
    }
}

impl HashState for Position {
    fn hash_state(&self, h: &mut StateHasher) {
        self.role.hash_state(h);
        h.write_u16(self.slots);
        h.write_u32(self.filled.len() as u32);
        for e in &self.filled {
            e.hash_state(h);
        }
        h.write_u8(u8::from(self.managerial));
        self.wage_band.0.hash_state(h);
        self.wage_band.1.hash_state(h);
    }
}

/// Dzień miesiąca, w którym firma wypłaca pensje, 0..29.
///
/// Sharding jest **po firmie, nie po pracowniku**: ludzie z jednego zakładu dostają
/// wypłatę tego samego dnia, bo tak wygląda to w świecie i bo inaczej dzienny bilans
/// gotówki firmy byłby rozmyty na trzydzieści rat. Własne mieszanie, a nie slot
/// taktyczny z [`crate::key::slots`] — przesunięcie decyzji taktycznej nie ma
/// przesuwać wypłat.
#[must_use]
pub fn payday(key: FirmKey) -> u8 {
    (mix64(key.0 ^ 0x5041_5944_5041_5944) % 30) as u8
}

/// Jedna wypłata do zaksięgowania.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PayrollItem {
    pub firm: FirmKey,
    pub site: SiteId,
    pub citizen: CitizenId,
    /// Brutto — kwota obciążająca firmę.
    pub gross: Money,
    /// Potrącenia (PIT, składki). Hook M8: do tego czasu zawsze zero (`D6`).
    pub deductions: Money,
}

impl PayrollItem {
    /// Kwota, która trafia do gospodarstwa domowego.
    #[must_use]
    pub fn net(&self) -> Money {
        Money(self.gross.get() - self.deductions.get())
    }
}

/// Wynik przebiegu listy płac: fakty do zaksięgowania przez `sim/economy`.
///
/// Kolejność jest ustalona — po `(FirmKey, SiteId, CitizenId)` — bo księgowanie
/// w innej kolejności dałoby inny ciąg transakcji, a ten wchodzi do hasha stanu.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct PayrollRun {
    pub items: Vec<PayrollItem>,
}

impl PayrollRun {
    /// Łączne obciążenie firm — wejście testu własnościowego pieniądza.
    #[must_use]
    pub fn total_gross(&self) -> Money {
        Money(self.items.iter().map(|i| i.gross.get()).sum())
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::Entity;
    use std::num::NonZeroU32;

    fn umowa(i: u32, wage: i64) -> Employment {
        Employment::new(
            CitizenId(Entity::new(i, NonZeroU32::MIN)),
            JobRoleId(0),
            Money(wage),
            SimMinute(0),
            ShiftKind::Day,
        )
    }

    #[test]
    fn wakat_to_roznica_etatow_i_obsady() {
        let mut p = Position::new(JobRoleId(0), 5, false, (Money(300_000), Money(500_000)));
        assert_eq!(p.vacancies(), 5);
        p.filled.push(umowa(1, 400_000));
        p.filled.push(umowa(2, 400_000));
        assert_eq!(p.vacancies(), 3);
    }

    #[test]
    fn nadmiar_obsady_nie_daje_ujemnego_wakatu() {
        let mut p = Position::new(JobRoleId(0), 1, false, (Money(1), Money(2)));
        p.filled.push(umowa(1, 1));
        p.filled.push(umowa(2, 1));
        assert_eq!(p.vacancies(), 0);
    }

    #[test]
    fn dzien_wyplaty_jest_funkcja_czysta_i_miesci_sie_w_miesiacu() {
        for i in 1..10_000u64 {
            assert!(payday(FirmKey(i)) < 30);
        }
        assert_eq!(payday(FirmKey(42)), payday(FirmKey(42)));
    }

    #[test]
    fn dni_wyplat_rozkladaja_sie_na_caly_miesiac() {
        let mut kubelki = [0u32; 30];
        for i in 1..=10_000u64 {
            kubelki[payday(FirmKey(i)) as usize] += 1;
        }
        assert!(
            kubelki.iter().all(|&n| n > 0),
            "dzień bez ani jednej wypłaty"
        );
    }

    #[test]
    fn brutto_bez_potracen_rowna_sie_netto() {
        let i = PayrollItem {
            firm: FirmKey(1),
            site: SiteId(Entity::new(1, NonZeroU32::MIN)),
            citizen: CitizenId(Entity::new(1, NonZeroU32::MIN)),
            gross: Money(450_000),
            deductions: Money::ZERO,
        };
        assert_eq!(i.net(), Money(450_000));
    }
}
