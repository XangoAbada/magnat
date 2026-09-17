//! Parametry, które światu nadaje **ktoś z zewnątrz** — dziś generator zdarzeń
//! i pogoda z `sim/events` (M8c WP4 i WP6, PRD §11.1 i §11.3).
//!
//! Dlaczego mieszkają tutaj, a nie w `sim/events`: stosuje je M3 (decyzja `D11`
//! fazy M8 — „M8 liczy parametry, M3 je stosuje do populacji"), a `sim/agents`
//! nie może zależeć od `sim/events`, bo zależność idzie w drugą stronę.
//! To ta sama reguła, która trzyma słowniki w `engine/core` (`K-8`), zastosowana
//! do struktury zamiast do enuma — dokładnie tak, jak `K-54` rozstrzygnął miejsce
//! `StrategicOutlooks` między `sim/macro` a `sim/economy`.
//!
//! Obie struktury są **neutralne w świecie bez zdarzeń**: mnożniki stoją na 10 000,
//! czyli na jedności, więc scenariusze M3 i M4 nie zmieniają się o ani jedno
//! losowanie, dopóki nikt do nich nie napisze.

use magnat_core::{HashState, StateHasher, NEED_COUNT};

/// Neutralny mnożnik: 10 000 punktów bazowych to „bez zmian".
pub const NEUTRAL_BPS: u16 = 10_000;

/// Mnożniki tempa spadku potrzeb, po jednym na `NeedKind`.
///
/// Wejście dla epidemii (zdrowie spada szybciej), festynu (rozrywka wolniej)
/// i mody (potrzeba przyspiesza). Tempo bazowe zostaje w `data/needs/needs.ron` —
/// zdarzenie zmienia mnożnik, nie daną, więc po jego wygaśnięciu świat wraca
/// do tabeli, a nie do liczby zapisanej przez ostatnie zdarzenie.
#[derive(Clone, Copy, Debug)]
pub struct NeedModifiers {
    pub decay_bps: [u16; NEED_COUNT],
}

impl Default for NeedModifiers {
    fn default() -> NeedModifiers {
        NeedModifiers {
            decay_bps: [NEUTRAL_BPS; NEED_COUNT],
        }
    }
}

impl NeedModifiers {
    /// Tempo spadku potrzeby po uwzględnieniu zdarzeń, w setnych punktu na godzinę.
    #[must_use]
    pub fn decay(&self, need_index: usize, base_centi_per_hour: u32) -> u32 {
        let m = u32::from(
            self.decay_bps
                .get(need_index)
                .copied()
                .unwrap_or(NEUTRAL_BPS),
        );
        base_centi_per_hour.saturating_mul(m) / u32::from(NEUTRAL_BPS)
    }
}

impl HashState for NeedModifiers {
    fn hash_state(&self, h: &mut StateHasher) {
        for m in self.decay_bps {
            h.write_u16(m);
        }
    }
}

/// Mnożniki hazardów demograficznych (`D11` fazy M8, PRD §11.3).
///
/// Liczy je `sim/events` ze wskaźników miasta — bezrobocia, inflacji, nastroju —
/// a stosuje `sim/agents` w tych samych trzech miejscach, w których hazard i tak
/// się losuje. Dzietność zależna od warunków, migracja zależna od koniunktury
/// i śmiertelność zależna od epidemii są w PRD jednym zdaniem; tutaj są trzema
/// mnożnikami, bo to wszystko, czego M3 potrzebuje, żeby je pokazać.
#[derive(Clone, Copy, Debug)]
pub struct DemographyParams {
    pub fertility_bps: u16,
    pub mortality_bps: u16,
    pub immigration_bps: u16,
    pub emigration_bps: u16,
}

impl Default for DemographyParams {
    fn default() -> DemographyParams {
        DemographyParams {
            fertility_bps: NEUTRAL_BPS,
            mortality_bps: NEUTRAL_BPS,
            immigration_bps: NEUTRAL_BPS,
            emigration_bps: NEUTRAL_BPS,
        }
    }
}

impl DemographyParams {
    /// Hazard na 100 000 po przemnożeniu przez mnożnik, przycięty do skali.
    #[must_use]
    pub fn scale(hazard_per_100k: u32, bps: u16) -> u32 {
        let v = u64::from(hazard_per_100k) * u64::from(bps) / u64::from(NEUTRAL_BPS);
        u32::try_from(v.min(100_000)).unwrap_or(100_000)
    }
}

impl HashState for DemographyParams {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u16(self.fertility_bps);
        h.write_u16(self.mortality_bps);
        h.write_u16(self.immigration_bps);
        h.write_u16(self.emigration_bps);
    }
}

/// Wstawia oba zasoby do świata i wpina je w hash stanu.
///
/// Woła to most stawiający świat agentów — **zawsze**, także bez zdarzeń. Zasób,
/// którego czasem nie ma, znaczyłby dwie ścieżki odczytu w każdym miejscu użycia,
/// a wartości neutralne kosztują osiem bajtów.
pub fn register_world_params(world: &mut magnat_ecs::World) {
    world.insert_resource(NeedModifiers::default());
    world.register_resource_hash::<NeedModifiers>();
    world.insert_resource(DemographyParams::default());
    world.register_resource_hash::<DemographyParams>();
}
