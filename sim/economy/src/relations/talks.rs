//! Negocjacje, strajk i cykl życia zmowy — druga połowa systemu (M10e §5.9).
//!
//! Plik jest osobny od `system.rs`, bo to są dwa tematy: tam mieszka pomiar
//! (żal, składowa grafu, kotwica żądania), tutaj rozstrzygnięcie (co strony sobie
//! oferują, kiedy zakład staje, kiedy urząd puka do drzwi).

use magnat_agents::{brand::Touch, Household, Population};
use magnat_core::{rng, DecisionReason, DistrictId, GoodId, Money, SiteId, StreamId, Tick, Q};
use magnat_ecs::World;
use magnat_firms::{FirmKey, FirmStatus, Firms};
use smallvec::SmallVec;
use std::collections::{BTreeMap, BTreeSet};

use super::cartel::{detection_ppm, Cartels};
use super::data::{RelationsTuning, UnionParams};
use super::grievance::{accept_bp, concession_bp};
use super::system::{z_rejestrem, zaloga, zapisz_logi, DAYS_PER_MONTH};
use super::union::{DetectedCartel, StrikeCall, UnionState, Unions};

/// Spór w tej chwili: zakład, runda, czy trwa strajk, żądanie i stan funduszu.
type Sprawa = (SiteId, u8, bool, u16, Money, Money);

/// Spory, w których strony mają sobie co powiedzieć.
fn otwarte_spory(world: &World, p: &UnionParams) -> Vec<Sprawa> {
    world.get_resource::<Unions>().map_or_else(Vec::new, |u| {
        u.iter()
            .filter_map(|z| {
                let d = z.demand?;
                let (runda, strajk) = match z.state {
                    UnionState::Demand { .. } => (0, false),
                    UnionState::Talks { round, .. } => (round, false),
                    UnionState::Strike { .. } => (p.max_rounds, true),
                    UnionState::Dormant { .. } => return None,
                };
                Some((
                    z.site,
                    runda,
                    strajk,
                    d.raise_bp,
                    z.strike_fund,
                    z.daily_burn,
                ))
            })
            .collect()
    })
}

/// Ile każda z firm kładzie dziś na stole, i kto nią jest.
///
/// Rzut rozstrzyga **wielkość ustępstwa w granicach, które firma sobie sama
/// wyznaczyła**, a nie to, czy strony się dogadają: sufit jest funkcją marży,
/// podłoga jego połową, a czy to wystarczy — rozstrzyga próg akceptacji związku.
/// Bez tego rzutu każdy spór o tej samej marży kończyłby się tej samej doby,
/// a mediana długości strajku z kryterium WP10.14 byłaby jedną liczbą bez rozrzutu.
fn oferty_firm(
    world: &mut World,
    sprawy: &[Sprawa],
    p: &UnionParams,
    t: Tick,
) -> BTreeMap<u32, (FirmKey, u16)> {
    let seed = world.seed;
    let oferty: Vec<(SiteId, FirmKey, u16)> = z_rejestrem(world, |_, firms| {
        sprawy
            .iter()
            .filter_map(|(site, runda, strajk, _, _, _)| {
                let s = firms.site(*site)?;
                let marza = s.pnl.last().and_then(|m| m.margin_bp());
                let sufit = concession_bp(marza, *runda, *strajk, p);
                let podloga = sufit / 2;
                let mut r = rng(seed, StreamId::StrikeResolve, site.entity().index(), t);
                let daje = podloga
                    + u16::try_from(r.gen_range_u32(u32::from(sufit - podloga) + 1)).unwrap_or(0);
                Some((*site, s.firm, daje))
            })
            .collect()
    });
    oferty
        .into_iter()
        .map(|(s, f, bp)| (s.entity().index(), (f, bp)))
        .collect()
}

/// Runda negocjacyjna. Zwraca `(ugody, rozpoczęte strajki)`.
pub(super) fn rundy(world: &mut World, tune: &RelationsTuning, day: u32, t: Tick) -> (u32, u32) {
    let p = tune.union;
    let sprawy = otwarte_spory(world, &p);
    if sprawy.is_empty() {
        return (0, 0);
    }
    let oferta = oferty_firm(world, &sprawy, &p, t);

    let mut ugody = 0;
    let mut nowe_strajki: Vec<(SiteId, FirmKey, u16)> = Vec::new();
    let mut podwyzki: Vec<(SiteId, u16)> = Vec::new();
    // Ugoda zawarta **w trakcie** strajku musi zdjąć przestój z zakładu — inaczej
    // hala stoi dalej, lista płac nie płaci, a zdarzenie `social/strike` nigdy nie
    // gaśnie, bo jego sonda czyta właśnie to pole.
    let mut wracaja: Vec<SiteId> = Vec::new();
    let mut logi: Vec<(FirmKey, DecisionReason)> = Vec::new();

    if let Some(u) = world.get_resource_mut::<Unions>() {
        for (site, runda, strajk, raise, fundusz, _) in sprawy {
            let Some((firm, daje)) = oferta.get(&site.entity().index()).copied() else {
                continue;
            };
            let Some(z) = u.get_mut(site) else { continue };
            // Próg akceptacji. Przed strajkiem schodzi z rundami — załoga zaczyna
            // od pełnego żądania i mięknie, im dłużej nic z tego nie ma. W strajku
            // rozstrzyga **fundusz**, bo to on się kończy (§5.9).
            let prog = if strajk {
                // Fundusz w chwili wyjścia z pracy odtwarza się z bieżącego stanu
                // i z liczby przepalonych dób — dokładnie, bo ubytek jest stały.
                // Osobnego pola na to nie ma z rozmysłu: byłoby drugą prawdą
                // o tej samej liczbie i wchodziłoby do hasha bez potrzeby.
                let dni = match z.state {
                    UnionState::Strike { since_day, .. } => {
                        i64::from(day.saturating_sub(since_day))
                    }
                    _ => 0,
                };
                let start = Money(fundusz.get() + z.daily_burn.get() * dni);
                accept_bp(raise, fundusz, start.max(Money(1)))
            } else {
                let r = u32::from(runda);
                let max = u32::from(p.max_rounds).max(1);
                u16::try_from(u32::from(raise) * (2 * max - r.min(max)) / (2 * max))
                    .unwrap_or(raise)
            };
            if daje >= prog && daje > 0 {
                let dni = match z.state {
                    UnionState::Strike { since_day, .. } => {
                        u16::try_from(day.saturating_sub(since_day)).unwrap_or(u16::MAX)
                    }
                    _ => 0,
                };
                let dano = daje.min(raise);
                podwyzki.push((site, dano));
                if strajk {
                    wracaja.push(site);
                }
                z.demand = None;
                z.strike_fund = Money::ZERO;
                z.daily_burn = Money::ZERO;
                z.state = UnionState::Dormant {
                    until_day: day + u32::from(p.truce_months) * DAYS_PER_MONTH,
                };
                // Wygrany spór podnosi wojowniczość: udało się raz, uda się znowu.
                z.militancy = Q::new((z.militancy.get().saturating_add(10)).min(100));
                ugody += 1;
                logi.push((
                    firm,
                    DecisionReason::StrikeEnded {
                        days: dni,
                        raise_bp: dano,
                    },
                ));
                continue;
            }
            if strajk {
                continue;
            }
            let nastepna = runda.saturating_add(1);
            if nastepna < p.max_rounds {
                z.state = UnionState::Talks {
                    round: nastepna,
                    since_day: day,
                };
                continue;
            }
            // Rundy się skończyły — załoga wychodzi. Ilu wyjdzie, rozstrzyga
            // wojowniczość: przekonanych jest tylu, ilu ich jest.
            let udzial =
                u16::try_from(u32::from(p.participation_bp) * u32::from(z.militancy.get()) / 100)
                    .unwrap_or(p.participation_bp);
            z.state = UnionState::Strike {
                since_day: day,
                participation_bp: udzial,
            };
            nowe_strajki.push((site, firm, udzial));
            logi.push((
                firm,
                DecisionReason::StrikeStarted {
                    participation_bp: udzial,
                    round: nastepna,
                },
            ));
        }
    }

    zastosuj_podwyzki(world, &podwyzki);
    if let Some(firms) = world.get_resource_mut::<Firms>() {
        for site in &wracaja {
            if let Some(s) = firms.site_mut(*site) {
                s.strike_bps = 0;
            }
        }
    }
    let ile = uruchom_strajki(world, &nowe_strajki, day);
    zapisz_logi(world, t, logi);
    (ugody, ile)
}

/// Podnosi płace całej załogi o `raise_bp`. Jedyna droga, którą ugoda zmienia
/// pieniądz — reszta dzieje się sama, przez listę płac M7.
fn zastosuj_podwyzki(world: &mut World, podwyzki: &[(SiteId, u16)]) {
    if podwyzki.is_empty() {
        return;
    }
    let Some(firms) = world.get_resource_mut::<Firms>() else {
        return;
    };
    for (site, bp) in podwyzki {
        let Some(s) = firms.site_mut(*site) else {
            continue;
        };
        for p in &mut s.positions {
            for e in &mut p.filled {
                e.wage_month =
                    Money(e.wage_month.get() * i64::from(10_000 + u32::from(*bp)) / 10_000);
            }
        }
    }
}

/// Wyjście z pracy: zakład dostaje `strike_bps`, fundusz dostaje wartość,
/// a `sim/events` — wezwanie do otwarcia zdarzenia.
fn uruchom_strajki(world: &mut World, nowe: &[(SiteId, FirmKey, u16)], _day: u32) -> u32 {
    if nowe.is_empty() {
        return 0;
    }
    // Fundusz strajkowy: płynne oszczędności gospodarstw strajkującej załogi,
    // zdjęte **raz**, w chwili wyjścia z pracy. Jedno przejście po gospodarstwach
    // na wszystkie dzisiejsze strajki — a strajk zaczyna się rzadko.
    let zalogi: Vec<(SiteId, Vec<u32>)> = nowe
        .iter()
        .map(|(s, _, _)| (*s, zaloga(world, *s)))
        .collect();
    let mut fundusze: BTreeMap<u32, i64> = BTreeMap::new();
    if let Some(pop) = world.get_resource::<Population>() {
        let gospodarstwa: Vec<magnat_core::Entity> = pop.households().to_vec();
        let zbiory: Vec<(u32, BTreeSet<u32>)> = zalogi
            .iter()
            .map(|(s, c)| (s.entity().index(), c.iter().copied().collect()))
            .collect();
        for e in gospodarstwa {
            let Some(h) = world.get::<Household>(e) else {
                continue;
            };
            if h.flags & Household::FLAG_ACTIVE == 0 {
                continue;
            }
            // Fundusz to oszczędności **ponad miesiąc utrzymania**, a nie całe saldo:
            // rodzina, która wydaje miesięczny dochód na czynsz i jedzenie, nie ma
            // z czego strajkować, choć na koncie coś jej leży. Bez tej rezerwy
            // strajki ciągnęłyby się tak długo, jak długo starcza pieniędzy na życie,
            // czyli miesiącami — i mediana z kryterium WP10.14 byłaby nieosiągalna.
            let plynne = h.cash.get() + h.bank.get() + h.savings.get() - h.income_monthly.get();
            if plynne <= 0 {
                continue;
            }
            for (site, crew) in &zbiory {
                if h.members.iter().any(|m| crew.contains(m)) {
                    *fundusze.entry(*site).or_insert(0) += plynne;
                }
            }
        }
    }

    // Dzienny ubytek: utracone zarobki strajkujących. To jest ta sama liczba,
    // o którą lista płac pomniejszy koszt pracy w rachunku wyniku zakładu —
    // fundusz i rachunek firmy patrzą na jeden strajk z dwóch stron. Po stronie
    // gospodarstw jest to **przybliżenie, nie odczyt**: dochód gospodarstwa jest
    // egzogeniczny do czasu, aż lista płac dostanie konsumenta (nagłówek `union.rs`).
    let mut burn: BTreeMap<u32, i64> = BTreeMap::new();
    if let Some(firms) = world.get_resource::<Firms>() {
        for (site, _, udzial) in nowe {
            let Some(s) = firms.site(*site) else { continue };
            let suma: i64 = s
                .positions
                .iter()
                .flat_map(|p| p.filled.iter())
                .map(|e| e.wage_month.get())
                .sum();
            burn.insert(
                site.entity().index(),
                suma * i64::from(*udzial) / 10_000 / i64::from(DAYS_PER_MONTH),
            );
        }
    }

    let mut ile = 0;
    if let Some(u) = world.get_resource_mut::<Unions>() {
        for (site, firm, udzial) in nowe {
            let Some(z) = u.get_mut(*site) else { continue };
            z.strike_fund = Money(
                fundusze
                    .get(&site.entity().index())
                    .copied()
                    .unwrap_or_default(),
            );
            z.daily_burn = Money(
                burn.get(&site.entity().index())
                    .copied()
                    .unwrap_or_default()
                    .max(1),
            );
            u.call_strike(StrikeCall {
                site: *site,
                firm: magnat_firms::firm_id(*firm),
                participation_bp: *udzial,
            });
            ile += 1;
        }
    }
    // Zakład dowiaduje się o strajku przez pole, które czyta i lista płac,
    // i sonda generatora zdarzeń (`K-89`).
    if let Some(firms) = world.get_resource_mut::<Firms>() {
        for (site, _, udzial) in nowe {
            if let Some(s) = firms.site_mut(*site) {
                s.strike_bps = *udzial;
            }
        }
    }
    ile
}

/// Dobowy przebieg strajku: przestój narasta, fundusz topnieje, a pusty fundusz
/// kończy spór bez podwyżki.
pub(super) fn strajki_dobowo(world: &mut World, tune: &RelationsTuning, day: u32, t: Tick) -> u32 {
    let p = tune.union;
    let strajkujace: Vec<(SiteId, u32, u16)> =
        world.get_resource::<Unions>().map_or_else(Vec::new, |u| {
            u.iter()
                .filter_map(|z| match z.state {
                    UnionState::Strike {
                        since_day,
                        participation_bp,
                    } => Some((z.site, since_day, participation_bp)),
                    _ => None,
                })
                .collect()
        });
    if strajkujace.is_empty() {
        return 0;
    }

    // Przestój dopisuje się do licznika zakładu — to z niego lista płac policzy,
    // za ile dni nie płaci.
    if let Some(firms) = world.get_resource_mut::<Firms>() {
        for (site, _, udzial) in &strajkujace {
            if let Some(s) = firms.site_mut(*site) {
                s.strike_bp_days = s.strike_bp_days.saturating_add(u32::from(*udzial));
            }
        }
    }

    let mut koniec: Vec<(SiteId, u16)> = Vec::new();
    if let Some(u) = world.get_resource_mut::<Unions>() {
        for (site, od, _) in strajkujace {
            let Some(z) = u.get_mut(site) else { continue };
            z.strike_fund = Money((z.strike_fund.get() - z.daily_burn.get()).max(0));
            let dni = u16::try_from(day.saturating_sub(od)).unwrap_or(u16::MAX);
            // Fundusz rozstrzyga; sufit dób jest bezpiecznikiem na przypadek,
            // w którym załoga nie ma nic do stracenia, bo nie ma nic.
            if z.strike_fund.get() <= 0 || dni >= p.max_strike_days {
                z.demand = None;
                z.strike_fund = Money::ZERO;
                z.daily_burn = Money::ZERO;
                z.state = UnionState::Dormant {
                    until_day: day + u32::from(p.truce_months) * DAYS_PER_MONTH,
                };
                // Przegrany strajk łamie wojowniczość — następnym razem pójdzie
                // mniej ludzi, a może nikt.
                z.militancy = Q::new(z.militancy.get().saturating_sub(25));
                koniec.push((site, dni));
            }
        }
    }
    if koniec.is_empty() {
        return 0;
    }
    let mut logi: Vec<(FirmKey, DecisionReason)> = Vec::new();
    if let Some(firms) = world.get_resource_mut::<Firms>() {
        for (site, dni) in &koniec {
            if let Some(s) = firms.site_mut(*site) {
                s.strike_bps = 0;
                logi.push((
                    s.firm,
                    DecisionReason::StrikeEnded {
                        days: *dni,
                        raise_bp: 0,
                    },
                ));
            }
        }
    }
    let ile = koniec.len() as u32;
    zapisz_logi(world, t, logi);
    ile
}

// ── WP10.13: zmowa cenowa ───────────────────────────────────────────────────────

/// Miesięczne zawiązywanie zmów.
pub(super) fn zmowy_powstaja(world: &mut World, tune: &RelationsTuning, day: u32, t: Tick) -> u32 {
    let p = tune.cartel;
    let Some(market) = world.get_resource::<Market>().cloned() else {
        return 0;
    };
    let polki = market.shelf_snapshot();
    if polki.is_empty() {
        return 0;
    }
    // Kandydaci: sprzedawcy tego samego towaru w tej samej dzielnicy, których
    // właściciel jest skłonny do ryzyka. Skłonność jest cechą firmy z M7e i to
    // ona rozstrzyga, kto w ogóle rozważa zmowę — nie liczba i nie cena.
    let sklonni: BTreeSet<u64> = z_rejestrem(world, |_, firms| {
        firms
            .iter()
            .filter(|(_, f)| {
                f.status == FirmStatus::Active && f.personality.risk_tolerance >= p.risk_threshold
            })
            .map(|(k, _)| k.0)
            .collect()
    });
    let klucz_firmy: BTreeMap<u32, FirmKey> = z_rejestrem(world, |_, firms| {
        firms
            .iter()
            .map(|(k, _)| (firms.id_of(k).0.index(), k))
            .collect()
    });

    let mut grupy: BTreeMap<(u16, u16), Vec<(FirmKey, Money)>> = BTreeMap::new();
    for s in &polki {
        let Some(key) = klucz_firmy.get(&s.firm.0.index()).copied() else {
            continue;
        };
        if !sklonni.contains(&key.0) {
            continue;
        }
        let g = grupy.entry((s.good.0, s.district.0)).or_default();
        if !g.iter().any(|(k, _)| *k == key) {
            g.push((key, s.price_gross));
        }
    }

    let mut nowe: Vec<(SmallVec<[FirmKey; 8]>, GoodId, DistrictId, Money)> = Vec::new();
    {
        let Some(c) = world.get_resource::<Cartels>() else {
            return 0;
        };
        for ((good, district), mut czlonkowie) in grupy {
            let (good, district) = (GoodId(good), DistrictId(district));
            if czlonkowie.len() < usize::from(p.min_members) || !c.allowed(good, district, day) {
                continue;
            }
            if czlonkowie
                .iter()
                .any(|(k, _)| c.floor_for(*k, good, district).is_some())
            {
                continue;
            }
            czlonkowie.sort_unstable_by_key(|(k, _)| k.0);
            czlonkowie.truncate(usize::from(p.max_members));
            let mut ceny: Vec<i64> = czlonkowie.iter().map(|(_, m)| m.get()).collect();
            ceny.sort_unstable();
            let benchmark = Money(ceny[ceny.len() / 2]);
            if benchmark.get() <= 0 {
                continue;
            }
            nowe.push((
                czlonkowie.iter().map(|(k, _)| *k).collect(),
                good,
                district,
                benchmark,
            ));
        }
    }

    let mut ile = 0;
    let mut logi: Vec<(FirmKey, DecisionReason)> = Vec::new();
    if let Some(c) = world.get_resource_mut::<Cartels>() {
        for (czlonkowie, good, district, benchmark) in nowe {
            let lista = czlonkowie.clone();
            let (_, powod) = c.form(czlonkowie, good, district, benchmark, day, &p);
            ile += 1;
            for k in lista {
                logi.push((k, powod));
            }
        }
    }
    zapisz_logi(world, t, logi);
    ile
}

/// Miesięczny rzut o wykrycie zmowy.
pub(super) fn zmowy_wykrywane(world: &mut World, tune: &RelationsTuning, day: u32, t: Tick) -> u32 {
    let p = tune.cartel;
    let seed = world.seed;
    // Niezadowoleni wtajemniczeni: załogi członków zmowy będące w sporze
    // z pracodawcą. Plan §5.9 pisał „menedżer z nastrojem < −40 lub zwolniony";
    // nastroju menedżera nie ma dziś gdzie przeczytać (`ManagementQuality` jest
    // umiejętnością, nie samopoczuciem), a **załoga w sporze jest tym samym
    // sygnałem i ma nośnik**: to są ludzie, którzy mają powód mówić o firmie źle.
    let w_sporze: BTreeSet<u64> = world
        .get_resource::<Unions>()
        .map_or_else(BTreeSet::new, |u| {
            u.iter()
                .filter(|z| z.state.in_dispute())
                .map(|z| z.firm.0)
                .collect()
        });

    let rzuty: Vec<(super::cartel::CartelId, u32, bool)> =
        world.get_resource::<Cartels>().map_or_else(Vec::new, |c| {
            let akt = c.enforcement_bp();
            c.iter()
                .map(|k| {
                    let niezadowoleni =
                        k.members.iter().filter(|m| w_sporze.contains(&m.0)).count() as u32;
                    let ppm = detection_ppm(
                        k.members.len() as u32,
                        k.deviation_pct(),
                        niezadowoleni,
                        akt,
                        &p,
                    );
                    let mut r = rng(seed, StreamId::CartelDetection, k.id.0, t);
                    (k.id, ppm, r.gen_range_u32(1_000_000) < ppm)
                })
                .collect()
        });

    let mut ile = 0;
    let mut logi: Vec<(FirmKey, DecisionReason)> = Vec::new();
    let mut zgloszenia: Vec<(FirmKey, DecisionReason)> = Vec::new();
    if let Some(c) = world.get_resource_mut::<Cartels>() {
        for (id, _, trafiony) in rzuty {
            if !trafiony {
                continue;
            }
            let Some((powod, czlonkowie)) = c.bust(id, day, &p) else {
                continue;
            };
            ile += 1;
            for k in czlonkowie {
                logi.push((k, powod));
                zgloszenia.push((k, powod));
            }
        }
    }
    if ile == 0 {
        return 0;
    }
    // Sprawę prowadzi urząd (`K-10`) — model zostawia fakt, a marki obrywają
    // wtedy, gdy miasto się dowie, czyli razem ze zgłoszeniem.
    let sprawy: Vec<(FirmKey, Option<SiteId>, Option<magnat_core::BrandId>)> =
        z_rejestrem(world, |_, firms| {
            zgloszenia
                .iter()
                .map(|(k, _)| {
                    (
                        *k,
                        firms.firm(*k).and_then(|f| f.sites.first().copied()),
                        magnat_supply::brand_of(firms.id_of(*k)),
                    )
                })
                .collect()
        });
    if let Some(c) = world.get_resource_mut::<Cartels>() {
        for (k, site, brand) in sprawy {
            if let Some(site) = site {
                c.report(DetectedCartel {
                    firm: magnat_firms::firm_id(k),
                    site,
                    evidence: Q::new(p.evidence),
                });
            }
            if let Some(b) = brand {
                c.note_scandal(b, p.brand_drop);
            }
        }
    }
    zapisz_logi(world, t, logi);
    ile
}

/// Dobowe pilnowanie ceny minimalnej. Zwraca, ile cen podniesiono.
///
/// Zmowa **nie ma własnego cennika** — podnosi ten, który jest, przez jedyne
/// wejście, którym cena się zmienia. Stoi po przecenie, więc to ona ma ostatnie
/// słowo w tej dobie.
pub(super) fn zmowy_pilnuja_ceny(world: &mut World) -> u32 {
    let Some(market) = world.get_resource::<Market>().cloned() else {
        return 0;
    };
    let ma_zmowy = world
        .get_resource::<Cartels>()
        .is_some_and(|c| !c.is_empty());
    if !ma_zmowy {
        return 0;
    }
    let polki = market.shelf_snapshot();
    let klucz_firmy: BTreeMap<u32, FirmKey> = z_rejestrem(world, |_, firms| {
        firms
            .iter()
            .map(|(k, _)| (firms.id_of(k).0.index(), k))
            .collect()
    });
    let Some(progi) = world.get_resource::<Cartels>().map(Cartels::floors) else {
        return 0;
    };
    let mut ile = 0;
    for s in &polki {
        let Some((prog, czlonkowie)) = progi.get(&(s.good.0, s.district.0)) else {
            continue;
        };
        if s.price_gross.get() >= prog.get() {
            continue;
        }
        let Some(key) = klucz_firmy.get(&s.firm.0.index()).copied() else {
            continue;
        };
        if czlonkowie.contains(&key) && market.set_price(s.site, s.good, *prog) {
            ile += 1;
        }
    }
    ile
}

/// Skandal dociera do tych, którzy markę **znają** — i tylko do nich (§5.9).
pub(super) fn skandale(world: &mut World, day: u32) {
    let lista = world
        .get_resource_mut::<Cartels>()
        .map(Cartels::take_scandals)
        .unwrap_or_default();
    if lista.is_empty() {
        return;
    }
    let Some(pop) = world.get_resource::<Population>() else {
        return;
    };
    let mieszkancy: Vec<magnat_core::Entity> = pop.citizens().to_vec();
    for (brand, drop) in lista {
        // Dwa przejścia, bo pierwsze potrzebuje `&World`, a drugie `&mut World`.
        let znajacy: Vec<magnat_core::Entity> = mieszkancy
            .iter()
            .copied()
            .filter(|e| {
                magnat_agents::brand::slots_of(world, *e, u64::from(day))
                    .iter()
                    .any(|s| s.brand == brand)
            })
            .collect();
        for e in znajacy {
            magnat_agents::touch(world, e, brand, Touch::Scandal { drop }, u64::from(day));
        }
    }
}

use crate::market::Market;
