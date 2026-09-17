//! Rozwinięcie efektu na klucze parametrów i zapis ich do właścicieli (M8c §5.5).
//!
//! To jest miejsce, w którym „zdarzenie zmienia parametry" przestaje być zdaniem
//! w dokumencie. Reguła jest jedna i widać ją w każdej z sześciu gałęzi: nakładka
//! **zapamiętuje, co zastała**, zapisuje swoje, a przy wygaśnięciu przywraca
//! zastane. Właściciel pola o niczym nie wie i nie musi — nie ma tu ani jednego
//! `if zdarzenie` po stronie M5, M6 ani M7.

use crate::param::{ParamPatch, SimParam, NEUTRAL};
use crate::probe::{SiteClass, SiteFilter};
use crate::registry::{Events, ResolvedEffect, ScopeInstance};
use magnat_core::{SiteId, TariffClassId, UtilityService};
use magnat_ecs::World;
use magnat_firms::{FirmKey, Firms};
use magnat_supply::ChainHandle;
use magnat_traffic::utility::UtilityGrids;

/// `SiteId` z indeksu encji — jedno miejsce po tej stronie granicy (`K-46`).
#[must_use]
pub fn site_of(index: u32) -> SiteId {
    SiteId(magnat_core::Entity::new(index, std::num::NonZeroU32::MIN))
}

/// Rozwija efekty definicji na patche dla konkretnej instancji zakresu.
///
/// Rozwinięcie dzieje się **raz, przy powstaniu zdarzenia**. Lista zakładów
/// dzielnicy nie zmienia się w trakcie suszy, a gdyby się zmieniła, zakład
/// postawiony w jej środku nie powinien dostać skutku wstecz.
#[must_use]
pub fn expand(
    ev: &Events,
    def: u16,
    scope: ScopeInstance,
    severity_bps: u16,
    firms: Option<&Firms>,
    grids: Option<&UtilityGrids>,
) -> Vec<ParamPatch> {
    let mut out = Vec::new();
    for e in ev.effects(def) {
        match *e {
            ResolvedEffect::SiteOutput { mul_bps, filter } => {
                let v = crate::param::Effect::lerp(i64::from(mul_bps), u32::from(severity_bps));
                for s in cele_zakladow(ev, scope, filter, firms) {
                    out.push(ParamPatch {
                        param: SimParam::SiteOutputMulBps(s.0.index()),
                        value: v,
                    });
                }
            }
            ResolvedEffect::SourceOnline => {
                for (svc, node) in zrodla(scope, grids) {
                    out.push(ParamPatch {
                        param: SimParam::SourceOnline(svc, node),
                        // Zero znaczy „zgaszone". Wartość jest przypisywana,
                        // nie mnożona, więc siła zdarzenia jej nie skaluje —
                        // blok albo stoi, albo nie, i to jest różnica między
                        // `SourceOnline` a `SourceCapacity`.
                        value: 0,
                    });
                }
            }
            ResolvedEffect::SourceCapacity { mul_bps } => {
                let v = crate::param::Effect::lerp(i64::from(mul_bps), u32::from(severity_bps));
                for (svc, node) in zrodla(scope, grids) {
                    out.push(ParamPatch {
                        param: SimParam::SourceCapacityMulBps(svc, node),
                        value: v,
                    });
                }
            }
            ResolvedEffect::ExternalPrice { good, mul_bps } => out.push(ParamPatch {
                param: SimParam::ExternalPriceMulBps(good.0),
                value: crate::param::Effect::lerp(i64::from(mul_bps), u32::from(severity_bps)),
            }),
            ResolvedEffect::Duty { class, bp } => out.push(ParamPatch {
                param: SimParam::DutyBp(class.0),
                // Cło jest **poziomem**, nie zmianą: uchwała ustala stawkę,
                // a nie „o tyle procent więcej". Siła zdarzenia skaluje odległość
                // od stawki zastanej, ale tę zna dopiero nakładka, więc tu idzie
                // wartość docelowa.
                value: bp,
            }),
            ResolvedEffect::NeedDecay { need, mul_bps } => out.push(ParamPatch {
                param: SimParam::NeedDecayMulBps(need.as_index() as u8),
                value: crate::param::Effect::lerp(i64::from(mul_bps), u32::from(severity_bps)),
            }),
        }
    }
    out
}

/// Zakłady objęte zakresem, po filtrze rodzaju. Kolejność rosnąca po identyfikatorze
/// — po niej idzie kolejność patchy, a ta wchodzi do hasha stanu.
fn cele_zakladow(
    ev: &Events,
    scope: ScopeInstance,
    filter: SiteFilter,
    firms: Option<&Firms>,
) -> Vec<SiteId> {
    let pasuje = |k: SiteClass| filter.accepts(k);
    match scope {
        ScopeInstance::Site(s) => ev
            .sites()
            .iter()
            .filter(|r| r.site == s && pasuje(r.kind))
            .map(|r| r.site)
            .collect(),
        ScopeInstance::District(d) => ev
            .sites()
            .iter()
            .filter(|r| r.district == d && pasuje(r.kind))
            .map(|r| r.site)
            .collect(),
        ScopeInstance::World => ev
            .sites()
            .iter()
            .filter(|r| pasuje(r.kind))
            .map(|r| r.site)
            .collect(),
        ScopeInstance::Firm(k) => {
            let Some(f) = firms.and_then(|r| r.get(k)) else {
                return Vec::new();
            };
            let mut v: Vec<SiteId> = f
                .sites
                .iter()
                .filter(|s| ev.sites().iter().any(|r| r.site == **s && pasuje(r.kind)))
                .copied()
                .collect();
            v.sort_unstable_by_key(|s| s.0.index());
            v
        }
        ScopeInstance::Network(_) => Vec::new(),
    }
}

/// Węzły źródłowe sieci objętej zakresem.
fn zrodla(scope: ScopeInstance, grids: Option<&UtilityGrids>) -> Vec<(u8, u32)> {
    let (ScopeInstance::Network(n), Some(g)) = (scope, grids) else {
        return Vec::new();
    };
    let Some(net) = g.nets().get(n as usize) else {
        return Vec::new();
    };
    let svc = net.service.as_index() as u8;
    net.sources().into_iter().map(|i| (svc, i)).collect()
}

/// Zapisuje wartości efektywne do właścicieli i przywraca zastane tam, gdzie
/// zdarzenie wygasło.
///
/// Zwraca liczbę zapisanych i przywróconych parametrów — do raportu scenariusza.
pub fn apply(ev: &mut Events, world: &mut World) -> (usize, usize) {
    let patches = ev.compose_patches();
    let nowe = crate::param::ParamOverlay::compose(&patches);
    let (zapis, powrot) = ev.overlay_mut().diff(nowe);

    let chain = world.get_resource::<ChainHandle>().cloned();
    for (p, v) in &zapis {
        let base = odczytaj(*p, world, chain.as_ref());
        ev.overlay_mut().remember_base(*p, base);
        let baza = ev.overlay().base(*p).unwrap_or(base);
        zapisz(*p, *v, baza, Jak::Nakladaj, world, chain.as_ref());
    }
    for p in &powrot {
        if let Some(base) = ev.overlay_mut().take_base(*p) {
            zapisz(*p, base, base, Jak::Przywroc, world, chain.as_ref());
        }
    }
    (zapis.len(), powrot.len())
}

/// Czy zapis nakłada nową wartość, czy przywraca zastaną.
///
/// Rozróżnienie jest jawne, a nie wyprowadzane z `v == base`, i to nie jest
/// ostrożność na zapas: dla mocy źródła `v` jest **mnożnikiem**, a `base`
/// **watami**, więc ich przypadkowa równość (elektrownia 5 000 W przy mnożniku
/// 5 000 bps) zamieniłaby przycięcie mocy o połowę w brak zmiany.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Jak {
    Nakladaj,
    Przywroc,
}

/// Wartość zastana parametru — to, do czego świat ma wrócić po zdarzeniu.
fn odczytaj(p: SimParam, world: &World, chain: Option<&ChainHandle>) -> i64 {
    match p {
        SimParam::SiteOutputMulBps(_) => i64::from(magnat_supply::PlantSite::NO_EVENT),
        SimParam::SourceOnline(svc, node) => {
            let Some(g) = world.get_resource::<UtilityGrids>() else {
                return 1;
            };
            let Some(s) = UtilityService::from_index(svc as usize) else {
                return 1;
            };
            g.net(s)
                .and_then(|n| n.source_info(node))
                .map_or(1, |(_, online, _)| i64::from(online))
        }
        SimParam::SourceCapacityMulBps(svc, node) => {
            let Some(g) = world.get_resource::<UtilityGrids>() else {
                return 0;
            };
            let Some(s) = UtilityService::from_index(svc as usize) else {
                return 0;
            };
            g.net(s)
                .and_then(|n| n.source_info(node))
                .map_or(0, |(cap, _, _)| cap)
        }
        SimParam::ExternalPriceMulBps(g) => chain.map_or(NEUTRAL, |c| {
            i64::from(c.lock().b2b.supply_shock(magnat_core::GoodId(g)))
        }),
        SimParam::DutyBp(c) => chain.map_or(0, |h| h.lock().b2b.duty_bp(TariffClassId(c))),
        SimParam::NeedDecayMulBps(_) => NEUTRAL,
    }
}

/// Zapisuje wartość do właściciela pola.
///
/// `base` jest potrzebne wyłącznie dla mocy źródła: nakładka trzyma **mnożnik**,
/// a sieć trzyma waty, więc przeliczenie musi znać punkt wyjścia. Gdyby nakładka
/// trzymała waty, dwa zdarzenia na jedną elektrownię nie dałyby się złożyć.
fn zapisz(
    p: SimParam,
    v: i64,
    base: i64,
    jak: Jak,
    world: &mut World,
    chain: Option<&ChainHandle>,
) {
    match p {
        SimParam::SiteOutputMulBps(idx) => {
            let Some(c) = chain else { return };
            let mut g = c.lock();
            if let Some(z) = g.plant.get_mut(site_of(idx)) {
                z.event_output_bps = u16::try_from(v.clamp(0, i64::from(u16::MAX)))
                    .unwrap_or(magnat_supply::PlantSite::NO_EVENT);
            }
        }
        SimParam::SourceOnline(svc, node) => {
            let Some(s) = UtilityService::from_index(svc as usize) else {
                return;
            };
            if let Some(g) = world.get_resource_mut::<UtilityGrids>() {
                if let Some(n) = g.net_mut(s) {
                    n.set_source_online(node, v != 0);
                }
            }
        }
        SimParam::SourceCapacityMulBps(svc, node) => {
            let Some(s) = UtilityService::from_index(svc as usize) else {
                return;
            };
            // Przy przywracaniu `v` jest już mocą w watach, a nie mnożnikiem,
            // więc mnożenie zepsułoby powrót.
            let moc = match jak {
                Jak::Przywroc => v,
                Jak::Nakladaj => base.saturating_mul(v) / NEUTRAL,
            };
            if let Some(g) = world.get_resource_mut::<UtilityGrids>() {
                if let Some(n) = g.net_mut(s) {
                    n.set_source_capacity(node, moc);
                }
            }
        }
        SimParam::ExternalPriceMulBps(g) => {
            if let Some(c) = chain {
                c.lock()
                    .b2b
                    .set_supply_shock(magnat_core::GoodId(g), i32::try_from(v).unwrap_or(10_000));
            }
        }
        SimParam::DutyBp(c) => {
            if let Some(h) = chain {
                h.lock().b2b.set_duty_bp(TariffClassId(c), v);
            }
        }
        SimParam::NeedDecayMulBps(n) => {
            if let Some(m) = world.get_resource_mut::<magnat_agents::NeedModifiers>() {
                if let Some(slot) = m.decay_bps.get_mut(n as usize) {
                    *slot = u16::try_from(v.clamp(0, i64::from(u16::MAX))).unwrap_or(10_000);
                }
            }
        }
    }
}

/// Instancje zakresu `Firm` — tu, a nie w rejestrze, bo tylko ten moduł widzi
/// `Firms`. Kolejność rosnąca po kluczu firmy.
#[must_use]
pub fn firm_instances(firms: Option<&Firms>) -> Vec<FirmKey> {
    let Some(f) = firms else {
        return Vec::new();
    };
    let mut v: Vec<FirmKey> = f.iter().map(|(k, _)| k).collect();
    v.sort_unstable_by_key(|k| k.0);
    v
}
