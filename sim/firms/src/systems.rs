//! Rejestracja firm w świecie i doba firmy (M7a WP1, M7 §5.16).
//!
//! Jeden system, nie trzy. Trzy poziomy decyzji dzielą ten sam rejestr, więc jako
//! osobne systemy byłyby trzema systemami wyłącznymi nad jednym zasobem — czyli
//! trzema poziomami harmonogramu i zerem zrównoleglenia. Ta sama nauka, którą M6e
//! zapisał przy `supply.Chain` (`AP-4`).

use magnat_core::{Cadence, SimCalendar};
use magnat_ecs::{System, SystemCtx, SystemDesc, World};

use crate::hr::employment::PayrollRun;
use crate::registry::Firms;

/// Wpina rejestr firm do świata i do funkcji haszującej stan (dokument 00 §3.6).
pub fn register_firms(world: &mut World, firms: Firms) {
    world.insert_resource(firms);
    world.register_resource_hash::<Firms>();
}

/// Minuta firm: przydział slotów decyzyjnych i — na granicy doby — lista płac.
///
/// System **nie księguje** wypłat: odkłada je w zasobie [`PayrollOutbox`], skąd
/// bierze je `sim/economy`. Właścicielem pieniądza jest M5 i tak zostaje.
pub struct FirmSystem {
    desc: SystemDesc,
}

impl FirmSystem {
    #[must_use]
    pub fn new() -> FirmSystem {
        FirmSystem {
            // Wyłączny (`K-21`): krok jest funkcją nad całym rejestrem firm.
            // **Otwiera tick** (`K-53`): rozdaje sloty decyzyjne i odkłada listę
            // płac, a wszyscy pozostali czytają to, co po nim zostało. Bez tej
            // deklaracji kierunek wobec każdego innego systemu wyłącznego brałby się
            // z hasha nazwy, a jedyne jawne ograniczenie, jakie ktoś wobec firm
            // zgłasza (`economy.Market.after_if_present`), domykało wtedy cykl.
            desc: SystemDesc::new("firms.Firm", Cadence::EveryMinute)
                .exclusive()
                .opens_tick(),
        }
    }
}

impl Default for FirmSystem {
    fn default() -> FirmSystem {
        FirmSystem::new()
    }
}

/// Firmy, którym w tej minucie wypadł slot decyzyjny (M7e WP11).
///
/// Skrzynka z tego samego powodu co [`PayrollOutbox`]: decyzje wykonuje
/// `sim/economy`, bo tam są półki, księgi i tablica publiczna, a `Firms::schedule`
/// **musi** mieć dokładnie jednego wołającego — to on zdejmuje firmy z kolejki
/// przepełnienia, a kolejka wchodzi do hasha stanu (`R4`). Dwóch wołających
/// znaczyłoby, że część decyzji przepada bez śladu.
#[derive(Default)]
pub struct DecisionOutbox {
    pub due: Vec<(crate::key::Tier, Vec<crate::key::FirmKey>)>,
}

impl DecisionOutbox {
    /// Zabiera to, co czeka, zostawiając skrzynkę pustą.
    pub fn take(&mut self) -> Vec<(crate::key::Tier, Vec<crate::key::FirmKey>)> {
        std::mem::take(&mut self.due)
    }
}

/// Wypłaty czekające na zaksięgowanie przez `sim/economy`.
///
/// Skrzynka, a nie wywołanie: gdyby `sim/firms` sięgnął po `Books`, zależność
/// poszłaby `firms → economy`, a idzie odwrotnie. Ten sam wzorzec, którym M6
/// oddaje rozliczenia B2B.
#[derive(Default)]
pub struct PayrollOutbox {
    pub pending: PayrollRun,
}

impl PayrollOutbox {
    /// Zabiera to, co czeka, zostawiając skrzynkę pustą.
    pub fn take(&mut self) -> PayrollRun {
        std::mem::take(&mut self.pending)
    }
}

impl System for FirmSystem {
    fn desc(&self) -> &SystemDesc {
        &self.desc
    }

    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let cal = SimCalendar::new(ctx.tick);
        let world = ctx.world_mut();
        let Some(firms) = world.get_resource_mut::<Firms>() else {
            return;
        };
        // Sloty przydzielamy co minutę: poziom operacyjny ma minutę doby, a taktyczny
        // i strategiczny wpadają tylko na pełnej godzinie — funkcja `due` to rozstrzyga.
        let sloty = firms.schedule(cal);
        // Decyzje wykonuje `sim/economy::ai_run` (M7e): tam są półki, księgi
        // i tablica publiczna. Tutaj zostaje sam przydział — i to on jest
        // deterministyczny, czego dowodzi test `scheduling.rs`.
        let cos_do_zrobienia = sloty.iter().any(|(_, k)| !k.is_empty());
        if cos_do_zrobienia {
            if let Some(out) = world.get_resource_mut::<DecisionOutbox>() {
                out.due = sloty;
            }
        }
        let Some(firms) = world.get_resource_mut::<Firms>() else {
            return;
        };
        let wyplaty = if cal.minute_of_day() == 0 {
            firms.run_payroll(cal)
        } else {
            PayrollRun::default()
        };
        if !wyplaty.is_empty() {
            if let Some(out) = world.get_resource_mut::<PayrollOutbox>() {
                out.pending.items.extend(wyplaty.items);
                out.pending.hr_costs.extend(wyplaty.hr_costs);
            }
        }
    }
}
