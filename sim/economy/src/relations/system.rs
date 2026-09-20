//! Jeden system na obie mechaniki podfazy (M10e WP10.13 i WP10.14).
//!
//! Zmowy i związki dzielą ten sam zasób kalibracji, ten sam moment w ticku i —
//! co ważniejsze — **spotykają się**: hazard wykrycia zmowy rośnie z liczby
//! niezadowolonych wtajemniczonych, a niezadowoleni wtajemniczeni to załogi
//! w sporze z pracodawcą. Dwa systemy wyłączne o wymuszonej kolejności byłyby
//! `K-53` bez potrzeby, a jeden nie przestaje mieć jednego powodu do zmiany:
//! „co się dzieje między firmami i w firmach poza rynkiem".
//!
//! Kolejność: **po `economy.Market`**, bo zmowa podnosi cenę, którą przecena
//! właśnie ustawiła, i **po `economy.Labor`**, bo żal liczy się z mediany zawodu,
//! którą rynek pracy przelicza raz na dobę. Obie deklaracje są warunkowe
//! (`after_if_present`, `K-51`): scenariusz bez rynku pracy nadal się buduje.

use std::collections::{BTreeMap, BTreeSet};

use magnat_core::{rng, Cadence, DecisionReason, Money, SiteId, StreamId, Tick, Q};
use magnat_ecs::{System, SystemCtx, SystemDesc, SystemId, World};
use magnat_firms::{FirmKey, Firms};

use crate::market::Market;

use super::cartel::Cartels;
use super::data::RelationsTuning;
use super::grievance::{demand_raise_bp, grievance, largest_component, may_form};
use super::union::{UnionDemand, UnionState, Unions};

pub(super) const MINUTES_PER_DAY: u64 = 1_440;
pub(super) const DAYS_PER_MONTH: u32 = 30;

/// Co się w tej dobie stało — do raportu scenariusza i do testów.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct RelationsDay {
    pub unions: u32,
    pub unions_formed: u32,
    pub demands: u32,
    pub settlements: u32,
    pub strikes_started: u32,
    pub strikes_ended: u32,
    pub striking: u32,
    pub cartels: u32,
    pub cartels_formed: u32,
    pub cartels_busted: u32,
    pub prices_lifted: u32,
}

/// Wpina relacje, zmowy i związki do świata.
///
/// Kalibracja jedzie tą samą drogą i **nie wchodzi do hasha**: to jest wejście,
/// a nie stan — ta sama zasada, którą `BrandData` z M10b stoi obok `BrandSlab`.
/// Przy okazji zjeżdża do `sim/supply`, bo tam mieszka tabela zaufania (`FE-13`).
pub fn register_relations(world: &mut World, tuning: RelationsTuning) {
    if let Some(m) = world.get_resource::<Market>() {
        m.chain().lock().b2b.set_relation_tuning(tuning.supplier);
    }
    world.insert_resource(tuning);
    world.insert_resource(Unions::new());
    world.insert_resource(Cartels::new());
    world.register_resource_hash::<Unions>();
    world.register_resource_hash::<Cartels>();
}

/// Relacje międzyfirmowe i pracownicze — jedna doba.
pub struct RelationsSystem {
    desc: SystemDesc,
    last: RelationsDay,
}

impl RelationsSystem {
    #[must_use]
    pub fn new() -> RelationsSystem {
        RelationsSystem {
            desc: SystemDesc::new("economy.Relations", Cadence::EveryDay)
                .exclusive()
                .after_if_present(SystemId::from_name("economy.Market"))
                .after_if_present(SystemId::from_name("economy.Labor")),
            last: RelationsDay::default(),
        }
    }

    #[must_use]
    pub fn last(&self) -> RelationsDay {
        self.last
    }
}

impl Default for RelationsSystem {
    fn default() -> RelationsSystem {
        RelationsSystem::new()
    }
}

impl System for RelationsSystem {
    fn desc(&self) -> &SystemDesc {
        &self.desc
    }

    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let t = ctx.tick;
        self.last = step_day(ctx.world_mut(), t);
    }
}

/// Jedna doba. Wolna funkcja, bo woła ją i system, i scenariusz, i test.
pub fn step_day(world: &mut World, t: Tick) -> RelationsDay {
    let Some(tune) = world.get_resource::<RelationsTuning>().copied() else {
        return RelationsDay::default();
    };
    let day = u32::try_from(t.0 / MINUTES_PER_DAY).unwrap_or(u32::MAX);
    let mut raport = RelationsDay::default();

    // Miesięcznie: pomiar żalu, zawiązywanie związków i zmów, rzut o wykrycie.
    if day > 0 && day.is_multiple_of(DAYS_PER_MONTH) {
        raport.unions_formed = zwiazki_powstaja(world, &tune, day, t);
        raport.demands = zadania(world, &tune, day, t);
        raport.cartels_formed = super::talks::zmowy_powstaja(world, &tune, day, t);
        raport.cartels_busted = super::talks::zmowy_wykrywane(world, &tune, day, t);
    }

    // Tygodniowo: runda negocjacyjna.
    if day > 0 && day.is_multiple_of(u32::from(tune.union.round_days.max(1))) {
        let (ugody, strajki) = super::talks::rundy(world, &tune, day, t);
        raport.settlements = ugody;
        raport.strikes_started = strajki;
    }

    // Codziennie: strajk kosztuje, fundusz topnieje, zmowa pilnuje ceny.
    raport.strikes_ended = super::talks::strajki_dobowo(world, &tune, day, t);
    raport.prices_lifted = super::talks::zmowy_pilnuja_ceny(world);
    super::talks::skandale(world, day);

    if let Some(u) = world.get_resource::<Unions>() {
        raport.unions = u.len() as u32;
        raport.striking = u.iter().filter(|x| x.state.is_striking()).count() as u32;
    }
    if let Some(c) = world.get_resource::<Cartels>() {
        raport.cartels = c.len() as u32;
    }
    raport
}

// ── załoga zakładu ──────────────────────────────────────────────────────────────

/// Indeksy encji pracowników zakładu, posortowane. Uczniowie są poza indeksem
/// od `K-74`, więc lista jest listą **pracujących**, a nie obecnych w budynku.
pub(super) fn zaloga(world: &World, site: SiteId) -> Vec<u32> {
    let Some(idx) = world.get_resource::<magnat_agents::SocialIndex>() else {
        return Vec::new();
    };
    let mut v: Vec<u32> = idx
        .coworkers(site.entity().index())
        .iter()
        .map(|(_, c)| *c)
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Krawędzie grafu relacji **wewnątrz** załogi, każda raz, niższy indeks pierwszy.
fn krawedzie(world: &World, crew: &[u32]) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    for c in crew {
        let Some(e) = magnat_agents::demography::citizen_by_index(world, *c) else {
            continue;
        };
        for (inny, _) in magnat_agents::relations_of(world, e) {
            let j = inny.index();
            if j > *c && crew.binary_search(&j).is_ok() {
                out.push((*c, j));
            }
        }
    }
    out
}

/// Średni stres załogi w skali 0..=100.
fn sredni_stres(world: &World, crew: &[u32]) -> u8 {
    let mut suma: u32 = 0;
    let mut ile: u32 = 0;
    for c in crew {
        let Some(e) = magnat_agents::demography::citizen_by_index(world, *c) else {
            continue;
        };
        if let Some(v) = world.get::<magnat_agents::Vitals>(e) {
            suma += u32::from(v.stress);
            ile += 1;
        }
    }
    suma.checked_div(ile)
        .map_or(0, |s| u8::try_from(s.min(100)).unwrap_or(100))
}

/// Mediana płacy **w mieście** dla każdego zawodu, z faktycznych wypłat.
///
/// Nie z `LaborMarketStats`: tamta pamięta szesnaście ostatnich **zawartych umów**
/// per zawód i dzielnicę, czyli to, co rynek płaci nowo przyjmowanym. Załoga
/// porównuje się z ludźmi, którzy tę robotę **wykonują**, a nie z ogłoszeniami —
/// i to jest różnica, którą widać w zakładzie z długim stażem i zamrożonymi
/// stawkami. Liczy się raz na krok, razem z mapą płac (`mapa_plac`).
pub(super) fn mediany_miejskie(firms: &Firms) -> BTreeMap<magnat_core::JobRoleId, Money> {
    let mut wg_roli: BTreeMap<magnat_core::JobRoleId, Vec<i64>> = BTreeMap::new();
    for (_, s) in firms.sites() {
        for p in &s.positions {
            for e in &p.filled {
                wg_roli.entry(p.role).or_default().push(e.wage_month.get());
            }
        }
    }
    wg_roli
        .into_iter()
        .map(|(r, mut v)| {
            v.sort_unstable();
            (r, Money(v[v.len() / 2]))
        })
        .collect()
}

/// Mediana płacy w zakładzie i mediana miejska jego dominującego zawodu — para
/// liczb, na której stoi cała luka płacowa.
pub(super) fn stawki(
    firms: &Firms,
    miejskie: &BTreeMap<magnat_core::JobRoleId, Money>,
    site: SiteId,
) -> Option<(Money, Option<Money>)> {
    let s = firms.site(site)?;
    let mut tutaj: Vec<Money> = Vec::new();
    let mut role: BTreeMap<magnat_core::JobRoleId, u32> = BTreeMap::new();
    for p in &s.positions {
        for e in &p.filled {
            tutaj.push(e.wage_month);
            *role.entry(p.role).or_insert(0) += 1;
        }
    }
    if tutaj.is_empty() {
        return None;
    }
    tutaj.sort_unstable();
    let mediana_tu = tutaj[tutaj.len() / 2];
    // Zawód, w którym zakład zatrudnia najwięcej ludzi — to jego załoga porównuje
    // z miastem. Przy remisie niższy `JobRoleId`, bo kolejność ma być ustalona.
    let dominujacy = role
        .iter()
        .max_by_key(|(r, n)| (**n, std::cmp::Reverse(r.0)))
        .map(|(r, _)| *r)?;
    Some((mediana_tu, miejskie.get(&dominujacy).copied()))
}

/// Wyjmuje rejestr firm ze świata na czas kroku i oddaje po nim — ta sama droga,
/// którą `sim/events` wyjmuje swój rejestr. Dwóch pożyczek `&mut World` naraz nie ma.
pub(super) fn z_rejestrem<R>(world: &mut World, f: impl FnOnce(&mut World, &mut Firms) -> R) -> R
where
    R: Default,
{
    let Some(mut firms) = world.get_resource_mut::<Firms>().map(std::mem::take) else {
        return R::default();
    };
    let out = f(world, &mut firms);
    if let Some(slot) = world.get_resource_mut::<Firms>() {
        *slot = firms;
    }
    out
}

pub(super) fn zapisz_logi(world: &mut World, t: Tick, logi: Vec<(FirmKey, DecisionReason)>) {
    if logi.is_empty() {
        return;
    }
    if let Some(f) = world.get_resource_mut::<Firms>() {
        for (key, r) in logi {
            f.log(key, t, r);
        }
    }
}

// ── WP10.14: powstawanie związku ────────────────────────────────────────────────

/// Miesięczny pomiar żalu i zawiązywanie związków.
fn zwiazki_powstaja(world: &mut World, tune: &RelationsTuning, day: u32, t: Tick) -> u32 {
    let seed = world.seed;
    let p = tune.union;
    // Pomiar liczy się bez `&mut World`, więc najpierw zbieramy wnioski, potem je
    // stosujemy — ta sama kolejność co przy każdym przejściu po populacji.
    let wnioski: Vec<(SiteId, FirmKey, u8, u32, u32, u32)> = z_rejestrem(world, |world, firms| {
        let miejskie = mediany_miejskie(firms);
        let mut out = Vec::new();
        for (id, s) in firms.sites() {
            let crew = zaloga(world, id);
            if crew.len() < usize::from(p.min_component) {
                continue;
            }
            let Some((tu, miejska)) = stawki(firms, &miejskie, id) else {
                continue;
            };
            let marza = s.pnl.last().and_then(|m| m.margin_bp());
            let g = grievance(marza, tu, miejska, sredni_stres(world, &crew));
            let poziom = g.total(&p);
            // Graf relacji przechodzi się **tylko wtedy, gdy jest o co**. Przejście
            // po krawędziach całej załogi jest najdroższą rzeczą w tym kroku, a dla
            // zakładu, w którym żal nie sięga progu, wynik i tak nie ma czytelnika:
            // `may_form` odsieje go na pierwszym warunku. Pomiar żalu zostaje dla
            // każdego zakładu, bo to on prowadzi licznik dób.
            let skladowa = if poziom >= p.grievance_threshold {
                largest_component(&crew, &krawedzie(world, &crew))
            } else {
                0
            };
            // Gęstość potencjalnego członkostwa: ilu pracowników zarabia **osobiście**
            // poniżej mediany miejskiej. To jest inne pytanie niż spójna składowa
            // i dlatego jest osobnym warunkiem: tamto mierzy, czy załoga się zna,
            // to — czy ma o co walczyć.
            let pokrzywdzeni = miejska.map_or(0u32, |med| {
                s.positions
                    .iter()
                    .flat_map(|q| q.filled.iter())
                    .filter(|e| e.wage_month.get() < med.get())
                    .count() as u32
            });
            let gestosc_bp = pokrzywdzeni.saturating_mul(10_000) / (crew.len() as u32).max(1);
            out.push((id, s.firm, poziom, skladowa, gestosc_bp, crew.len() as u32));
        }
        out
    });

    let mut powstalo = 0;
    let mut logi: Vec<(FirmKey, DecisionReason)> = Vec::new();
    if let Some(u) = world.get_resource_mut::<Unions>() {
        for (site, firm, poziom, skladowa, gestosc_bp, crew) in wnioski {
            let g = u.note_grievance(
                site,
                poziom,
                p.grievance_threshold,
                DAYS_PER_MONTH as u16,
                skladowa,
                gestosc_bp,
            );
            if u.get(site).is_some() || !may_form(g.days_above, crew, skladowa, gestosc_bp, &p) {
                continue;
            }
            // Warunki są twarde; rzut rozstrzyga **kiedy w oknie**. Bez niego
            // wszystkie zakłady spełniające warunki zawiązałyby związek w tej samej
            // dobie, bo warunki są funkcją tych samych miejskich median.
            let mut r = rng(seed, StreamId::UnionFormation, site.entity().index(), t);
            if r.gen_range_u32(3) != 0 {
                continue;
            }
            let gestosc = Q::new(u8::try_from(gestosc_bp / 100).unwrap_or(100).min(100));
            u.form(site, firm, gestosc, day);
            powstalo += 1;
            logi.push((
                firm,
                DecisionReason::UnionFormed {
                    density: gestosc,
                    grievance: Q::new(poziom),
                },
            ));
        }
    }
    zapisz_logi(world, t, logi);
    powstalo
}

/// Związek, który przespał okres spokoju, przedstawia żądanie.
fn zadania(world: &mut World, tune: &RelationsTuning, day: u32, t: Tick) -> u32 {
    let p = tune.union;
    let cele: Vec<SiteId> = world.get_resource::<Unions>().map_or_else(Vec::new, |u| {
        u.iter()
            .filter(|x| matches!(x.state, UnionState::Dormant { until_day } if day >= until_day))
            .map(|x| x.site)
            .collect()
    });
    if cele.is_empty() {
        return 0;
    }
    // Kotwica żądania: co **załoga wie** o płacach gdzie indziej — mediana zarobków
    // ludzi, których pracownicy tego zakładu znają z grafu relacji, a którzy pracują
    // u kogoś innego (§5.9). Nie prawdziwa mediana miejska: związek może żądać
    // za dużo albo za mało i to jest realizm z systemu, a nie z parametru.
    let nowe: Vec<(SiteId, FirmKey, u16, Money)> = z_rejestrem(world, |world, firms| {
        let place = mapa_plac(firms);
        let miejskie = mediany_miejskie(firms);
        let mut out = Vec::new();
        for site in cele {
            let crew = zaloga(world, site);
            let Some((tu, _)) = stawki(firms, &miejskie, site) else {
                continue;
            };
            let swoi: BTreeSet<u32> = crew.iter().copied().collect();
            let mut znane: Vec<Money> = Vec::new();
            for c in &crew {
                let Some(e) = magnat_agents::demography::citizen_by_index(world, *c) else {
                    continue;
                };
                for (inny, _) in magnat_agents::relations_of(world, e) {
                    let j = inny.index();
                    if !swoi.contains(&j) {
                        if let Some(w) = place.get(&j) {
                            znane.push(*w);
                        }
                    }
                }
            }
            znane.sort_unstable();
            let kotwica = znane.get(znane.len() / 2).copied().unwrap_or(tu);
            let raise = demand_raise_bp(tu, kotwica, &p);
            let Some(firm) = firms.site(site).map(|s| s.firm) else {
                continue;
            };
            if raise > 0 {
                out.push((site, firm, raise, kotwica));
            }
        }
        out
    });

    let mut ile = 0;
    let mut logi: Vec<(FirmKey, DecisionReason)> = Vec::new();
    if let Some(u) = world.get_resource_mut::<Unions>() {
        for (site, firm, raise, kotwica) in nowe {
            let Some(zw) = u.get_mut(site) else { continue };
            zw.demand = Some(UnionDemand {
                raise_bp: raise,
                anchor: kotwica,
            });
            zw.state = UnionState::Demand { since_day: day };
            ile += 1;
            logi.push((
                firm,
                DecisionReason::WageDemandMade {
                    raise_bp: raise,
                    anchor: kotwica,
                },
            ));
        }
    }
    zapisz_logi(world, t, logi);
    ile
}

/// Kto ile zarabia — jedna mapa na cały krok, bo inaczej każde żądanie
/// przechodziłoby po wszystkich zakładach od nowa.
pub(super) fn mapa_plac(firms: &Firms) -> BTreeMap<u32, Money> {
    let mut m = BTreeMap::new();
    for (_, s) in firms.sites() {
        for p in &s.positions {
            for e in &p.filled {
                m.insert(e.citizen.0.index(), e.wage_month);
            }
        }
    }
    m
}
