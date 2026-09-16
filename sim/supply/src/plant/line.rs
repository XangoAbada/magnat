//! Linia produkcyjna i jej harmonogram (M6b §5.5, WP4).
//!
//! Linia jest **stanem, nie zdarzeniem**: w każdej minucie stoi w dokładnie jednym
//! [`LineState`], a przejścia między nimi są tym, co gracz czyta w karcie inspekcji
//! jako odpowiedź na „dlaczego moja fabryka nie produkuje".
//!
//! `Starved` i `Blocked` są osobnymi stanami, nie `Idle` — bo to one lądują w alertach
//! pulpitu firmy, a „bezczynna" i „nie ma mąki" to dla gracza dwa różne problemy
//! z dwoma różnymi rozwiązaniami.

use magnat_core::{
    Energy, GoodId, HashState, LineStopCause, Mass, Money, RecipeId, SimMinute, StateHasher, Q,
};

use crate::batch::BatchOrigin;
use crate::catalog::MachineClassId;

/// Wsad, który linia zabrała z magazynu i trzyma do końca szarży.
///
/// **Nie trzyma uchwytów partii** (`AE-1`): partie wejściowe zostały zużyte w chwili
/// załadowania linii, więc to, co zostaje, jest ich podsumowaniem — masa, jakość,
/// koszt i pochodzenie. Trzymanie `BatchId` znaczyłoby, że linia ma własną listę partii
/// obok magazynu, a to jest dokładnie ta druga lista, której `AD-7` zabrania.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Charge {
    /// Masa wsadu w gramach — to od niej liczy się skala wyjść wobec `Recipe::batch_mass`.
    pub mass: Mass,
    /// Jakość wejść, średnia ważona masą.
    pub q_in: Q,
    /// Najgorsze wejście — sufit jakości wyjścia, gdy receptura go stosuje.
    pub q_worst: Q,
    pub cost: Money,
    /// Umiejętność obsady, która szarżę **zaczęła**. Trzymana we wsadzie, a nie czytana
    /// przy zamknięciu, bo wypiek trwa 190 minut i kończy go często inna zmiana —
    /// a jakość chleba robi ta, która go nastawiła.
    pub skill: Q,
    pub wage_mult_pct: u16,
    pub origin: BatchOrigin,
}

impl HashState for Charge {
    fn hash_state(&self, h: &mut StateHasher) {
        self.mass.hash_state(h);
        self.q_in.hash_state(h);
        self.q_worst.hash_state(h);
        self.cost.hash_state(h);
        self.skill.hash_state(h);
        h.write_u16(self.wage_mult_pct);
        h.write_u32(self.origin.site.map_or(u32::MAX, |s| s.entity().index()));
        h.write_u16(self.origin.recipe.map_or(u16::MAX, |r| r.0));
        h.write_u8(self.origin.depth);
        h.write_u16(self.origin.deposit.map_or(u16::MAX, |d| d.0));
    }
}

/// Powód awarii maszyny. Węższy niż [`LineStopCause`], bo awaria to nie to samo co
/// postój: linia bezczynna jest cicha, linia w awarii zapala `SITE_FAULT` w snapshocie
/// renderu (M6 §6.4.3) i wymaga części z magazynu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum BreakCause {
    Wear = 0,
    NoPower = 1,
    NoWater = 2,
    NoStaff = 3,
    Strike = 4,
}

impl BreakCause {
    /// Odwzorowanie na słownik z `core`, który idzie do karty inspekcji.
    #[must_use]
    pub const fn stop_cause(self) -> LineStopCause {
        match self {
            BreakCause::Wear => LineStopCause::Wear,
            BreakCause::NoPower => LineStopCause::NoPower,
            BreakCause::NoWater => LineStopCause::NoWater,
            BreakCause::NoStaff => LineStopCause::NoStaff,
            BreakCause::Strike => LineStopCause::Strike,
        }
    }

    /// Czy awaria wymaga części zamiennej. Brak prądu i brak obsady mijają same,
    /// zużycie mechaniczne — nie.
    #[must_use]
    pub const fn needs_part(self) -> bool {
        matches!(self, BreakCause::Wear)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LineState {
    Idle,
    Setup {
        until: SimMinute,
        to_recipe: RecipeId,
    },
    Running {
        recipe: RecipeId,
        started: SimMinute,
        ends: SimMinute,
        charge: Charge,
    },
    Broken {
        since: SimMinute,
        cause: BreakCause,
    },
    Maintenance {
        until: SimMinute,
    },
    /// Brak wejścia.
    Starved {
        missing: GoodId,
    },
    /// Pełny magazyn wyjściowy.
    Blocked {
        full: GoodId,
    },
}

impl LineState {
    /// Powód postoju w słowniku `core`, albo `None`, gdy linia pracuje.
    #[must_use]
    pub const fn stop_cause(self) -> Option<LineStopCause> {
        match self {
            LineState::Running { .. } | LineState::Idle => None,
            LineState::Setup { .. } => Some(LineStopCause::Setup),
            LineState::Broken { cause, .. } => Some(cause.stop_cause()),
            LineState::Maintenance { .. } => Some(LineStopCause::Maintenance),
            LineState::Starved { .. } => Some(LineStopCause::Starved),
            LineState::Blocked { .. } => Some(LineStopCause::Blocked),
        }
    }

    /// Wkład do skali `activity` w snapshocie renderu (M6 §6.4.3). Przezbrojenie daje
    /// 0,4 — maszyna pracuje, ale nie produkuje, i **słychać ją**. Stojąca linia daje
    /// ciszę, zgodnie z PRD §15.5.
    #[must_use]
    pub const fn activity_permille(self) -> u16 {
        match self {
            LineState::Running { .. } => 1000,
            LineState::Setup { .. } => 400,
            _ => 0,
        }
    }

    /// Czy linia jest w awarii — bit `SITE_FAULT` w snapshocie.
    #[must_use]
    pub const fn is_fault(self) -> bool {
        matches!(self, LineState::Broken { .. })
    }
}

impl HashState for LineState {
    fn hash_state(&self, h: &mut StateHasher) {
        match self {
            LineState::Idle => h.write_u8(0),
            LineState::Setup { until, to_recipe } => {
                h.write_u8(1);
                until.hash_state(h);
                h.write_u16(to_recipe.0);
            }
            LineState::Running {
                recipe,
                started,
                ends,
                charge,
            } => {
                h.write_u8(2);
                h.write_u16(recipe.0);
                started.hash_state(h);
                ends.hash_state(h);
                charge.hash_state(h);
            }
            LineState::Broken { since, cause } => {
                h.write_u8(3);
                since.hash_state(h);
                h.write_u8(*cause as u8);
            }
            LineState::Maintenance { until } => {
                h.write_u8(4);
                until.hash_state(h);
            }
            LineState::Starved { missing } => {
                h.write_u8(5);
                h.write_u16(missing.0);
            }
            LineState::Blocked { full } => {
                h.write_u8(6);
                h.write_u16(full.0);
            }
        }
    }
}

/// Linia produkcyjna.
#[derive(Clone, Copy, Debug)]
pub struct ProductionLine {
    pub machine_class: MachineClassId,
    /// Aktualnie ustawiona receptura. `None` = linia nieprzezbrojona na nic.
    pub recipe: Option<RecipeId>,
    /// Masa wsadu na godzinę przy 100% — to **linia**, a nie receptura, decyduje
    /// o wielkości szarży. Receptura daje proporcje i czas, linia daje przepustowość.
    pub nominal_throughput: Mass,
    pub age_minutes: u64,
    pub condition: Q,
    /// Bazowy średni czas między awariami. Efektywny to `mtbf * condition / 100` —
    /// zajeżdżona maszyna psuje się częściej i to jest cała mechanika konserwacji.
    pub mtbf_hours: u32,
    /// Pobór przy pracy, watogodziny na godzinę.
    pub power_draw: Energy,
    /// Część potrzebna do naprawy — wpięcie linii w łańcuch dostaw. Bez niej awaria
    /// byłaby licznikiem czasu, a nie problemem zaopatrzeniowym.
    pub spare_part: GoodId,
    pub state: LineState,
    /// Minuty pracy od ostatniego przeglądu — licznik zużycia.
    pub worked_since_service: u32,
    /// Kiedy tę linię czeka przegląd planowy.
    ///
    /// Stan **linii**, nie harmonogramu zakładu — i to nie jest kosmetyka. Termin
    /// wspólny dla całego zakładu wyglądał poprawnie, dopóki wszystkie linie stawały
    /// naraz; gdy jedna produkowała bez przerwy, termin nigdy się nie przesuwał,
    /// a pozostałe wracały do przeglądu w tej samej minucie, w której go kończyły.
    /// Częstotliwość zostaje polityką zakładu (`ProductionSchedule`), termin jest tu.
    pub next_maintenance: SimMinute,
}

impl ProductionLine {
    /// Nowa linia, prosto z fabryki maszyn: stan idealny, zero przebiegu.
    #[must_use]
    pub fn new(
        machine_class: MachineClassId,
        nominal_throughput: Mass,
        power_draw: Energy,
        spare_part: GoodId,
    ) -> ProductionLine {
        ProductionLine {
            machine_class,
            recipe: None,
            nominal_throughput,
            age_minutes: 0,
            condition: Q::MAX,
            mtbf_hours: 1_400,
            power_draw,
            spare_part,
            state: LineState::Idle,
            worked_since_service: 0,
            next_maintenance: SimMinute(u64::MAX),
        }
    }

    /// Efektywny MTBF w **minutach**. Maszyna w stanie 0 psuje się co minutę —
    /// dlatego dolne ograniczenie to jedna minuta, a nie dzielenie przez zero.
    #[must_use]
    pub const fn mtbf_minutes(&self) -> u32 {
        let m = self.mtbf_hours as u64 * 60 * self.condition.get() as u64 / 100;
        if m < 1 {
            1
        } else {
            m as u32
        }
    }

    /// Masa wsadu jednej szarży przy danym czasie trwania i obniżeniu produkcji.
    /// `throttle_pct` pochodzi z kaskady niedoboru (§5.7) i jest jedynym miejscem,
    /// w którym niedobór zmienia **wielkość** szarży, a nie samą decyzję o jej starcie.
    #[must_use]
    pub fn charge_mass(&self, duration_minutes: u32, throttle_pct: u8) -> Mass {
        let pelna = i128::from(self.nominal_throughput.0) * i128::from(duration_minutes) / 60;
        Mass((pelna * i128::from(throttle_pct) / 100) as i64)
    }
}

impl HashState for ProductionLine {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u16(self.machine_class.0);
        h.write_u16(self.recipe.map_or(u16::MAX, |r| r.0));
        self.nominal_throughput.hash_state(h);
        h.write_u64(self.age_minutes);
        self.condition.hash_state(h);
        h.write_u32(self.mtbf_hours);
        self.power_draw.hash_state(h);
        h.write_u16(self.spare_part.0);
        self.state.hash_state(h);
        h.write_u32(self.worked_since_service);
        self.next_maintenance.hash_state(h);
    }
}

/// Zmiana robocza. Godziny w minutach doby, więc zmiana nocna zawija się przez północ.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Shift {
    pub from: u16,
    pub to: u16,
    pub headcount: u16,
    pub wage_multiplier_pct: u16,
    /// Średnia umiejętność obsady — wejście [`QualityModel`](crate::QualityModel).
    ///
    /// `ponytail:` w M6 to liczba na zmianie, a nie średnia po ludziach. Sufit nazwany:
    /// M3 dostarcza `skill` mieszkańca, M7 obsadza zmiany etatami — wtedy to pole
    /// przestaje być parametrem i staje się wynikiem. Do tego czasu jedno pole
    /// jest tańsze niż udawana agregacja po pustym zbiorze pracowników.
    pub skill: Q,
}

impl Shift {
    #[must_use]
    pub fn covers(&self, minute_of_day: u16) -> bool {
        if self.from <= self.to {
            minute_of_day >= self.from && minute_of_day < self.to
        } else {
            minute_of_day >= self.from || minute_of_day < self.to
        }
    }
}

/// Zaplanowana szarża.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PlannedRun {
    pub recipe: RecipeId,
    /// Ile masy wyrobu jeszcze zostało do zrobienia z tego zlecenia.
    pub remaining: Mass,
    pub earliest_start: SimMinute,
    pub priority: u8,
}

/// Harmonogram zakładu: zmiany, plan szarż, konserwacja.
#[derive(Clone, Debug)]
pub struct ProductionSchedule {
    pub shifts: [Option<Shift>; 3],
    pub plan: std::collections::VecDeque<PlannedRun>,
    /// Co ile godzin pracy linii należy się przegląd. **Częstotliwość jest polityką
    /// zakładu, termin stanem linii** ([`ProductionLine::next_maintenance`]).
    pub maintenance_interval_hours: u32,
}

impl Default for ProductionSchedule {
    fn default() -> ProductionSchedule {
        ProductionSchedule {
            // Jedna zmiana dzienna 6:00–14:00. Decyzję o drugiej i trzeciej podejmuje M7
            // (zmiana nocna kosztuje więcej); M6 ją wyłącznie wykonuje.
            shifts: [
                Some(Shift {
                    from: 6 * 60,
                    to: 14 * 60,
                    headcount: 8,
                    wage_multiplier_pct: 100,
                    skill: Q::new(60),
                }),
                None,
                None,
            ],
            plan: std::collections::VecDeque::new(),
            maintenance_interval_hours: 720,
        }
    }
}

impl ProductionSchedule {
    /// Zmiana obsadzająca tę minutę doby, jeśli jakaś jest.
    #[must_use]
    pub fn staffed(&self, minute_of_day: u16) -> Option<&Shift> {
        self.shifts
            .iter()
            .flatten()
            .find(|s| s.headcount > 0 && s.covers(minute_of_day))
    }
}

impl HashState for ProductionSchedule {
    fn hash_state(&self, h: &mut StateHasher) {
        for s in &self.shifts {
            match s {
                None => h.write_u8(0),
                Some(s) => {
                    h.write_u8(1);
                    h.write_u16(s.from);
                    h.write_u16(s.to);
                    h.write_u16(s.headcount);
                    h.write_u16(s.wage_multiplier_pct);
                    s.skill.hash_state(h);
                }
            }
        }
        h.write_u32(self.plan.len() as u32);
        for p in &self.plan {
            h.write_u16(p.recipe.0);
            p.remaining.hash_state(h);
            p.earliest_start.hash_state(h);
            h.write_u8(p.priority);
        }
        h.write_u32(self.maintenance_interval_hours);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Zmiana nocna zawija się przez północ — jedyny warunek brzegowy w tym pliku,
    /// który da się pomylić, a pomyłka oznacza zakład stojący przez pół doby.
    #[test]
    fn zmiana_nocna_zawija_sie_przez_polnoc() {
        let noc = Shift {
            from: 22 * 60,
            to: 6 * 60,
            headcount: 4,
            wage_multiplier_pct: 150,
            skill: Q::new(50),
        };
        assert!(noc.covers(23 * 60));
        assert!(noc.covers(60));
        assert!(!noc.covers(12 * 60));

        let dzien = Shift {
            from: 6 * 60,
            to: 14 * 60,
            ..noc
        };
        assert!(dzien.covers(7 * 60));
        assert!(!dzien.covers(23 * 60));
    }

    /// Zajeżdżona maszyna psuje się częściej — to jest cała mechanika konserwacji
    /// i musi być monotoniczna, inaczej przegląd byłby wydatkiem bez skutku.
    #[test]
    fn zuzycie_skraca_czas_miedzy_awariami() {
        let mut l = ProductionLine::new(MachineClassId(0), Mass(3_000_000), Energy(55), GoodId(0));
        let nowa = l.mtbf_minutes();
        l.condition = Q::new(50);
        let zuzyta = l.mtbf_minutes();
        l.condition = Q::MIN;
        assert!(zuzyta < nowa, "{zuzyta} < {nowa}");
        assert_eq!(
            l.mtbf_minutes(),
            1,
            "maszyna w stanie 0 nie dzieli przez zero"
        );
    }

    /// Obniżenie produkcji z kaskady zmienia **wielkość** szarży, a nie decyzję
    /// o jej starcie — zakład nie staje, tylko zwalnia.
    #[test]
    fn obnizenie_produkcji_skaluje_szarze() {
        let l = ProductionLine::new(MachineClassId(0), Mass(3_000_000), Energy(55), GoodId(0));
        assert_eq!(l.charge_mass(40, 100), Mass(2_000_000));
        assert_eq!(l.charge_mass(40, 25), Mass(500_000));
        assert_eq!(l.charge_mass(40, 0), Mass::ZERO);
    }
}
