//! System ECS sieci przesyłowych i szew z zakładem (M8b §5.4).
//!
//! # Dlaczego ten system otwiera tick
//!
//! Stan zasilania jest **warunkiem wstępnym** dla wszystkich pozostałych: linia
//! produkcyjna pyta o prąd, zanim ruszy szarżę. System, który ustala ten warunek
//! po produkcji, spóźniałby blackout o minutę — a minuta to dokładnie ta jednostka,
//! w której test T2 mierzy „wolumen produkcji w oknie awarii dokładnie 0".
//! Bez `opens_tick` (`K-53`) system wyłączny dostaje krawędź konfliktu od każdego,
//! kto wypadł przed nim w haszu nazwy, i kolejność byłaby losem, a nie deklaracją.
//!
//! # Dlaczego wyłączny
//!
//! Deklaracja dostępu przez komponenty nic by nie kupiła: system nie dotyka ani
//! jednego komponentu, za to sięga po dwa zasoby świata i po arenę zakładów pod
//! zamkiem `ChainHandle`. To jest dokładnie przypadek `K-21` — krok jest funkcją
//! nad światem, a nie zbiorem dostępów.

use magnat_core::{Cadence, Tick, UtilityService};
use magnat_ecs::{System, SystemCtx, SystemDesc, World};
use magnat_supply::ChainHandle;

use super::tuning::GridTuning;
use super::{SupplyState, UtilityGrids};

/// Co ile minut przelicza się sieć danego medium (§5.4, tabela „Częstotliwość
/// i koszt").
///
/// Prąd co minutę, bo bufora nie ma i blackout musi być ostry. Reszta co godzinę:
/// w rurociągu stoi woda, w budynku stoi ciepło, a brak przepustowości łącza
/// to degradacja, nie zatrzymanie.
#[must_use]
pub const fn period_minutes(service: UtilityService) -> u64 {
    match service {
        UtilityService::Electricity => 1,
        _ => 60,
    }
}

/// Krok sieci: bilans, zrzut, kaskada i przepisanie odcięć do liczników zakładów.
pub struct UtilitySystem {
    desc: SystemDesc,
    tuning: GridTuning,
}

impl UtilitySystem {
    #[must_use]
    pub fn new(tuning: GridTuning) -> UtilitySystem {
        UtilitySystem {
            desc: SystemDesc::new("traffic.Utility", Cadence::EveryMinute)
                .exclusive()
                .opens_tick(),
            tuning,
        }
    }
}

impl System for UtilitySystem {
    fn desc(&self) -> &SystemDesc {
        &self.desc
    }

    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let t = ctx.tick;
        let world = ctx.world_mut();
        if world.get_resource::<UtilityGrids>().is_none() {
            return;
        }
        // Uchwyt łańcucha jest `Arc` w środku, więc klon nie kopiuje areny —
        // bierzemy go **przed** pożyczką sieci, żeby obie nie pożyczały świata naraz.
        let chain = world.get_resource::<ChainHandle>().cloned();
        let seed = world.seed;
        let grids = world.resource_mut::<UtilityGrids>();
        krok(grids, seed, t, &self.tuning, chain.as_ref());
    }
}

/// Krok sieci bez ECS — tą samą drogą chodzą testy i benchmark.
pub fn krok(
    grids: &mut UtilityGrids,
    seed: u64,
    t: Tick,
    tuning: &GridTuning,
    chain: Option<&ChainHandle>,
) {
    let hour = u32::try_from(t.0 % 1440 / 60).unwrap_or(0);
    let mut powody = Vec::new();
    let mut przepisz = false;
    for i in 0..grids.nets_mut().len() {
        let okres = period_minutes(grids.nets()[i].service);
        if okres > 1 && !t.0.is_multiple_of(okres) {
            continue;
        }
        grids.apply_links_to(u32::try_from(i).unwrap_or(u32::MAX));
        let r = {
            let net = &mut grids.nets_mut()[i];
            net.solve(seed, t, hour, &tuning.profiles, tuning.repair)
        };
        powody.extend(r.reasons.iter().copied());
        przepisz |= grids.mark_outage(i, r.outage_key, r.unserved);
    }
    // Pierścień powodów ma 64 miejsca, więc zapisywanie wcześniejszych jest pracą
    // dla kosza. Kaskada rozbijająca metropolię na setki wysp potrafi wyprodukować
    // tysiące powodów w jednym ticku i wszystkie poza ostatnimi 64 i tak wypadną.
    let od = powody.len().saturating_sub(super::REASON_RING);
    for p in powody.into_iter().skip(od) {
        grids.note(t, p);
    }
    if przepisz {
        if let Some(c) = chain {
            przepisz_liczniki(grids, tuning, c);
        }
    }
}

/// Przepisuje stan zasilania i taryfę do liczników zakładów (`sim/supply`).
///
/// To jest **cały** szew M8b → M6: sieć nie liczy zużycia i nie wystawia faktury,
/// tylko ustawia dwie rzeczy, których licznik sam o sobie nie wie — czy medium
/// jest, i po ile. Resztę robi `UtilityMeter` od M6b bez zmiany kształtu.
///
/// Kierunek zależności jest bezpieczny i jednostronny: `sim/traffic` zależy od
/// `sim/supply`, nigdy odwrotnie — ta sama konstrukcja, którą `K-44` dopuściło
/// dla pokrycia etatowego pisanego przez M7.
fn przepisz_liczniki(grids: &UtilityGrids, tuning: &GridTuning, chain: &ChainHandle) {
    let mut c = chain.lock();
    for net in grids.nets() {
        let taryfa = tuning.tariff(net.service);
        for node in &net.nodes {
            let Some(site) = node.site else { continue };
            let Some(zaklad) = c.plant.get_mut(site) else {
                continue;
            };
            let Some(m) = zaklad.meter_mut(net.service) else {
                continue;
            };
            m.cut_off = node.state != SupplyState::Ok;
            if taryfa.per_unit.get() > 0 {
                m.tariff = taryfa.per_unit;
                m.standing = taryfa.standing_charge_per_month;
            }
        }
    }
}

/// Rejestruje zasób sieci i jego hak hasha.
///
/// Osobno od `register_traffic`, bo sieci przesyłowe stawia scenariusz, który ma
/// miasto — a `m3day` i `m5shop` mają ruch bez miasta i nie mają czego zasilać.
pub fn register_grids(world: &mut World, grids: UtilityGrids) {
    world.insert_resource(grids);
    world.register_resource_hash::<UtilityGrids>();
}
