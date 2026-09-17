//! System ECS modelu makro (M7f WP13, §5.16).
//!
//! # Jedno zdjęcie na kwartał, jedno „co jeśli" na firmę
//!
//! `lift()` jest drogi (przechodzi po całej populacji i po wszystkich półkach),
//! więc woła się **raz na kwartał** i wynik jest współdzielony. Firma robi na nim
//! `what_if()` na klonie — i to jest cały budżet: bez współdzielenia byłoby
//! 10 tys. zdjęć miasta na kwartał zamiast jednego.
//!
//! Firmy przelicza się **rozłożone po dobach kwartału**: klucz firmy modulo 90
//! wybiera dobę. Nie dlatego, że nie zdążyłyby wszystkie naraz, tylko dlatego, że
//! budżet tej fazy jest wyrażony w **liczbie firm na tick** (`R4`) i pik raz na
//! kwartał byłby dokładnie tym, czego 00 §3.5 zabrania mierzyć zegarem.
//!
//! # Dlaczego ten system stoi tutaj, a nie w `sim/economy::ai_run`
//!
//! Bo `sim/macro` zależy od `sim/economy` (jądro, `K-50`), więc zależność w drugą
//! stronę zamknęłaby cykl. Dwa pozostałe tiery zostają tam, gdzie były; ten jeden
//! pisze wynik do [`StrategicOutlooks`] — zasobu, który mieszka w `sim/firms`,
//! czyli we wspólnym przodku obu crate'ów.

use magnat_core::{Cadence, HashState, StateHasher, Tick, Trend};
use magnat_economy::Market;
use magnat_ecs::{System, SystemCtx, SystemDesc, SystemId, World};
use magnat_firms::view::{CityFacts, FirmView, GoodFacts, SiteFacts};
use magnat_firms::{FirmKey, Firms, Outlook, StrAction, StrategicOutlooks};

use crate::lift::lift;
use crate::state::MacroState;
use crate::whatif::{rank_variants, Scenario};

/// Kwartał ma 90 dób (`K-1`: 3 miesiące × 30).
pub const DAYS_PER_QUARTER: u64 = 90;

/// Horyzont „co jeśli": jeden kwartał do przodu. Dłuższy nie zwiększa trafności —
/// zwiększa tylko deklarowany margines (patrz `whatif::margines`), więc zwycięzca
/// robi się rzadszy, a nie pewniejszy.
pub const HORIZON_DAYS: u16 = DAYS_PER_QUARTER as u16;

/// Uchwyt zdjęcia makro w świecie.
///
/// `Option` w środku z tego samego powodu co przy `LaborHandle`: krok systemu dotyka
/// naraz zdjęcia, rejestru firm i rynku, a dwóch pożyczek `&mut World` naraz nie ma.
#[derive(Default)]
pub struct MacroHandle(Option<MacroState>);

impl MacroHandle {
    #[must_use]
    pub fn get(&self) -> Option<&MacroState> {
        self.0.as_ref()
    }

    fn set(&mut self, st: MacroState) {
        self.0 = Some(st);
    }
}

impl HashState for MacroHandle {
    fn hash_state(&self, h: &mut StateHasher) {
        match &self.0 {
            None => h.write_u8(0),
            Some(st) => {
                h.write_u8(1);
                st.hash_state(h);
            }
        }
    }
}

/// Wpina model makro do świata: zdjęcie i tablicę uporządkowań.
///
/// Oba **wchodzą do hasha stanu**. Zdjęcie, bo firmy podejmują na nim decyzje;
/// uporządkowania, bo są tym, co firma widzi. Gdyby któreś z nich zostało poza
/// hashem, dwa przebiegi tego samego ziarna mogłyby się rozjechać w decyzji,
/// a nie w liczbie — czyli w miejscu, w którym najtrudniej to zauważyć.
pub fn register_macro(world: &mut World) {
    world.insert_resource(MacroHandle::default());
    world.register_resource_hash::<MacroHandle>();
    world.insert_resource(StrategicOutlooks::new());
    world.register_resource_hash::<StrategicOutlooks>();
}

/// Doba modelu makro.
pub struct MacroSystem {
    desc: SystemDesc,
}

impl MacroSystem {
    #[must_use]
    pub fn new() -> MacroSystem {
        MacroSystem {
            desc: SystemDesc::new("macro.WhatIf", Cadence::EveryDay)
                .exclusive()
                // Po rynku pracy, bo obsada zakładów z tej doby jest wejściem zdjęcia.
                // `after_if_present`, bo scenariusz może stawiać makro bez rynku pracy
                // (`K-51`) — i wtedy brak celu jest brakiem krawędzi, nie błędem.
                .after_if_present(SystemId::from_name("economy.Labor")),
        }
    }
}

impl Default for MacroSystem {
    fn default() -> MacroSystem {
        MacroSystem::new()
    }
}

impl System for MacroSystem {
    fn desc(&self) -> &SystemDesc {
        &self.desc
    }

    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let t = ctx.tick;
        let doba = t.get() / magnat_core::time::MINUTES_PER_DAY;

        // 1. Zdjęcie — raz na kwartał.
        if doba.is_multiple_of(DAYS_PER_QUARTER) {
            let st = lift(ctx.world());
            if let Some(h) = ctx.world_mut().get_resource_mut::<MacroHandle>() {
                h.set(st);
            }
        }

        // 2. Firmy przypadające na dzisiejszą dobę kwartału.
        let dzis = doba % DAYS_PER_QUARTER;
        let Some(st) = ctx
            .world_mut()
            .get_resource_mut::<MacroHandle>()
            .and_then(|h| h.0.take())
        else {
            return;
        };
        let wynik = przelicz(ctx.world(), &st, dzis, t);
        if let Some(h) = ctx.world_mut().get_resource_mut::<MacroHandle>() {
            h.set(st);
        }
        if let Some(tab) = ctx.world_mut().get_resource_mut::<StrategicOutlooks>() {
            for (key, o) in wynik {
                tab.set(key, o);
            }
            // Prognoza starsza niż kwartał nie jest „trochę mniej pewna" —
            // jest o innym świecie.
            tab.expire(t, DAYS_PER_QUARTER * magnat_core::time::MINUTES_PER_DAY);
        }
    }
}

/// Przelicza warianty firm przypadających na tę dobę kwartału.
///
/// Zwraca listę zamiast pisać w miejscu, bo świat jest tu pożyczony niemutowalnie:
/// zdjęcie, rejestr firm i rynek czyta się naraz, a zapis idzie osobno.
fn przelicz(world: &World, st: &MacroState, dzis: u64, t: Tick) -> Vec<(FirmKey, Outlook)> {
    let Some(firms) = world.get_resource::<Firms>() else {
        return Vec::new();
    };
    let market = world.get_resource::<Market>();
    let mut out = Vec::new();
    let mut sites: Vec<SiteFacts> = Vec::new();
    let goods: Vec<GoodFacts> = Vec::new();

    for (key, firma) in firms.iter() {
        if key.0 % DAYS_PER_QUARTER != dzis {
            continue;
        }
        sites.clear();
        for id in &firma.sites {
            let Some(s) = firms.site(*id) else { continue };
            sites.push(SiteFacts {
                site: *id,
                district: s.district,
                vacancies: s.vacancies(),
                headcount: s.headcount() as u32,
                months_in_loss: miesiace_straty(s),
                last_margin_bp: s.pnl.last().and_then(magnat_firms::SitePnlMonth::margin_bp),
                delegated: s.delegation.is_some(),
                needs_policy: false,
                scarcest_role: None,
            });
        }
        let gotowka = market
            .and_then(|m| m.account_of_firm(magnat_firms::firm_id(key)))
            .and_then(|a| world.get_resource::<magnat_economy::Books>()?.balance(a))
            .unwrap_or(magnat_core::Money::ZERO);
        let v = FirmView {
            key,
            cash: gotowka,
            personality: firma.personality,
            strategy: firma.strategy,
            margin_floor_bp: 0,
            margin_ceiling_bp: 0,
            lag_days: FirmView::lag_for(firma.sites.len(), &firma.personality),
            tick: t,
            sites: &sites,
            goods: &goods,
            city: CityFacts::default(),
            // Tier strategiczny **proponuje** warianty bez prognozy: pytanie bierze
            // się z własnych ksiąg, odpowiedź z modelu (§5.10).
            outlook: None,
        };
        let warianty = magnat_firms::propose_variants(&v);
        if warianty.len() < 2 {
            continue;
        }
        let Some(id) = st
            .firm_index(magnat_firms::firm_id(key))
            .map(|i| st.firms[i].id)
        else {
            continue;
        };
        let scenariusze: Vec<Scenario> = warianty.iter().map(|a| na_scenariusz(*a, id)).collect();
        let ranking = rank_variants(st, &scenariusze, HORIZON_DAYS);
        let zwyciezca = ranking.decisive_winner();
        out.push((
            key,
            Outlook {
                variants: warianty,
                winner: zwyciezca.map(|i| u8::try_from(i).unwrap_or(0)),
                trend: zwyciezca.map_or(Trend::Flat, |i| ranking.direction(i)),
                error_margin_bp: ranking.error_margin_bp,
                at: t,
            },
        ));
    }
    out
}

/// Ile ostatnich miesięcy zakład zamknął stratą. Liczy się od końca i przerywa
/// na pierwszym miesiącu bez pomiaru — miesiąc bez przychodu nie jest miesiącem
/// straty, tylko miesiącem, o którym nic nie wiadomo (`SitePnlMonth::margin_bp`).
fn miesiace_straty(s: &magnat_firms::Site) -> u8 {
    let mut n = 0u8;
    for m in s.pnl.iter().rev() {
        match m.margin_bp() {
            Some(bp) if bp < 0 => n = n.saturating_add(1),
            _ => break,
        }
    }
    n
}

/// Wariant firmy → scenariusz modelu. Jedyne tłumaczenie między dwoma słownikami
/// i dlatego jedno miejsce, w którym mogłyby się rozjechać.
fn na_scenariusz(a: StrAction, firm: magnat_core::FirmId) -> Scenario {
    match a {
        StrAction::KeepCourse => Scenario::KeepCourse { firm },
        StrAction::OpenSite {
            district,
            slots,
            capex,
        } => Scenario::OpenSite {
            firm,
            district: district.0,
            slots,
            capex,
        },
        StrAction::CloseSite { site, slots } => Scenario::CloseSite { firm, site, slots },
        // Zwinięcie firmy to zamknięcie wszystkiego naraz — w modelu wygląda jak
        // zejście zdolności do zera, bo tym właśnie jest.
        StrAction::RequestVoluntaryClosure => Scenario::CloseSite {
            firm,
            site: magnat_core::SiteId(magnat_core::Entity::new(0, std::num::NonZeroU32::MIN)),
            slots: u32::MAX / 2,
        },
    }
}
