use super::day::{usun_relacje, zwolnij_slaby};
use super::*;
use crate::migration::Czesci;
use magnat_core::CitizenReason;

/// Przelicza typ gospodarstwa po zmianie składu (§5.6: typ jest funkcją składu).
pub fn przeklasyfikuj(world: &mut World, hh_idx: u32, hh: &mut Household, day: u64) {
    let ages = world.resource::<DemographyTable>().ages();
    let sklad = household::members_of(hh_idx, hh, world.resource::<HouseholdOverflow>());
    let widoki: Vec<MemberView> = sklad
        .iter()
        .filter_map(|m| {
            let c = encja(world, *m)?;
            let id = world.get::<Identity>(c)?;
            let emp = world.get::<Employment>(c)?;
            let partner_w_domu = world
                .get::<Lifecycle>(c)
                .map(|l| l.partner)
                .filter(|p| *p != Lifecycle::NO_PARTNER)
                .is_some_and(|p| sklad.contains(&p));
            Some(MemberView::new(*m, id, emp, day as i32, partner_w_domu))
        })
        .collect();
    hh.kind = household::classify(&widoki, i32::from(ages.adult), i32::from(ages.senior)) as u8;
    // Liczba dzieci wychodzi z **tego samego** składu co typ (`R2-WP11`): skala
    // ekwiwalentna po stronie M5 nie może mieć własnej definicji dziecka.
    hh.children = household::children_count(&widoki, i32::from(ages.adult));
}

// -- miesiac: zwiazki, sluby, rozstania -----------------------------------------

/// Jeden miesiąc cyklu życia gospodarstw (§5.6). Wołany co 30 dób (K-1).
///
/// Kolejność: dobór partnera → ślub → rozstanie. Para dobrana dziś nie bierze dziś
/// ślubu (dzieli je `wedding_min_days`), a para, która wzięła ślub dziś, nie rozstaje
/// się dziś (`divorce_grace_days`) — więc kolejność nie tworzy ścieżki „w jednym
/// miesiącu od poznania do rozwodu". Jedno pole `Lifecycle.next_event_day` niesie oba
/// terminy, bo są rozłączne w czasie: najpierw jest to najwcześniejsza data ślubu,
/// potem najwcześniejsza data rozstania.
pub fn step_month(world: &mut World, day: u64) -> MonthReport {
    let mut raport = MonthReport::default();
    dobierz_partnerow(world, day, &mut raport);
    sluby(world, day, &mut raport);
    rozstania(world, day, &mut raport);
    raport
}

/// Zgodność kandydata: 100 minus kara za różnicę statusu i wieku.
#[must_use]
pub fn compatibility(status_a: u8, status_b: u8, age_a: i32, age_b: i32) -> Q {
    let ds = i32::from(status_a).abs_diff(i32::from(status_b)) as i32;
    let dw = age_a.abs_diff(age_b) as i32;
    Q::new((100 - ds * 2 - dw * 3).clamp(0, 100) as u8)
}

fn dobierz_partnerow(world: &mut World, day: u64, raport: &mut MonthReport) {
    let tabela = world.resource::<DemographyTable>().clone();
    let ages = tabela.ages();
    let (max_ds, max_dw, min_waga) = tabela.partner_limits();
    let (wedding_min_days, _) = tabela.wedding_rules();
    let spis: Vec<Entity> = world.resource::<Population>().citizens().to_vec();

    for e in spis {
        let Some(id) = world.get::<Identity>(e).copied() else {
            continue;
        };
        let wiek = id.age_years(day as i32);
        if wiek < i32::from(ages.adult) {
            continue;
        }
        let l = world.get::<Lifecycle>(e).copied().unwrap_or_default();
        if l.partner != Lifecycle::NO_PARTNER {
            continue;
        }
        let mut r = rng(
            world.seed,
            StreamId::Relations,
            e.index(),
            stream_key(day, K_PARTNER),
        );
        if !r.gen_bool_permille(tabela.partnering_permille(wiek)) {
            continue;
        }

        let status = world.get::<Vitals>(e).map_or(50, |v| v.status);
        let Some(rel) = world.get::<RelationsRef>(e).copied() else {
            continue;
        };
        let kandydaci: Vec<(u32, u8)> = world
            .resource::<RelationSlab>()
            .entries(relations_ref(&rel))
            .iter()
            .filter(|x| x.weight >= min_waga && !RelationKind::from_u8(x.kind).is_family())
            .map(|x| (x.other, x.weight))
            .collect();

        let mut najlepszy: Option<(Entity, Q, u8)> = None;
        let mut rozwazonych = 0u8;
        for (inny, waga) in kandydaci {
            let Some(c) = encja(world, inny) else {
                continue;
            };
            let Some(cid) = world.get::<Identity>(c).copied() else {
                continue;
            };
            let cwiek = cid.age_years(day as i32);
            if cwiek < i32::from(ages.adult) || cwiek.abs_diff(wiek) > u32::from(max_dw) {
                continue;
            }
            if world
                .get::<Lifecycle>(c)
                .is_some_and(|x| x.partner != Lifecycle::NO_PARTNER)
            {
                continue;
            }
            let cstatus = world.get::<Vitals>(c).map_or(50, |v| v.status);
            if u32::from(status).abs_diff(u32::from(cstatus)) > u32::from(max_ds) {
                continue;
            }
            rozwazonych = rozwazonych.saturating_add(1);
            let zgodnosc = compatibility(status, cstatus, wiek, cwiek);
            // Remis rozstrzyga waga relacji, a potem indeks encji (00 §3.2).
            let lepszy = najlepszy.is_none_or(|(be, bz, bw)| {
                (zgodnosc.get(), waga, std::cmp::Reverse(c.index()))
                    > (bz.get(), bw, std::cmp::Reverse(be.index()))
            });
            if lepszy {
                najlepszy = Some((c, zgodnosc, waga));
            }
        }

        let Some((partner, zgodnosc, _)) = najlepszy else {
            continue;
        };
        for (a, b) in [(e, partner), (partner, e)] {
            if let Some(l) = world.get_mut::<Lifecycle>(a) {
                l.partner = b.index();
                l.flags |= Lifecycle::FLAG_PARTNERED;
                l.next_event_day = ((day + u64::from(wedding_min_days)) % 65_536) as u16;
            }
        }
        let waga = tabela.social().family_weight;
        powiaz(world, e, partner, RelationKind::Partner, waga, day);
        raport.partnerships += 1;
        raport.reasons.push((
            e.index(),
            DecisionReason::Citizen(CitizenReason::PartnerChosen {
                compatibility: zgodnosc,
                candidates: rozwazonych,
            }),
        ));
    }
}

/// Wspólne gospodarstwo po `wedding_min_days` dobach związku o wadze >= progu (§5.6).
///
/// Przenosi się gospodarstwo **mniejsze** — razem z dziećmi, bo samotny rodzic nie
/// zostawia ich pod starym adresem. Opuszczony lokal wraca do puli pustostanów i to
/// jest jedna z trzech dróg, którymi pustostan powstaje (pozostałe to zgon i wyjazd).
fn sluby(world: &mut World, day: u64, raport: &mut MonthReport) {
    let tabela = world.resource::<DemographyTable>().clone();
    let (_, min_waga) = tabela.wedding_rules();
    let grace = tabela.divorce_grace_days();
    let spis: Vec<Entity> = world.resource::<Population>().citizens().to_vec();

    for e in spis {
        let l = world.get::<Lifecycle>(e).copied().unwrap_or_default();
        if l.partner == Lifecycle::NO_PARTNER || l.partner < e.index() {
            continue; // parę obsługuje ten z niższym indeksem
        }
        let Some(partner) = encja(world, l.partner) else {
            continue;
        };
        let (Some(a), Some(b)) = (
            world.get::<Identity>(e).copied(),
            world.get::<Identity>(partner).copied(),
        ) else {
            continue;
        };
        if a.household == b.household {
            continue; // już mieszkają razem
        }
        if (day % 65_536) < u64::from(l.next_event_day) {
            continue;
        }
        let Some(rel) = world.get::<RelationsRef>(e).copied() else {
            continue;
        };
        let waga = world
            .resource::<RelationSlab>()
            .entries(relations_ref(&rel))
            .iter()
            .find(|x| x.other == partner.index())
            .map_or(0, |x| x.weight);
        if waga < min_waga {
            continue;
        }
        if polacz_gospodarstwa(world, a.household, b.household, day) {
            raport.weddings += 1;
            for kto in [e, partner] {
                if let Some(lc) = world.get_mut::<Lifecycle>(kto) {
                    lc.next_event_day = ((day + u64::from(grace)) % 65_536) as u16;
                }
            }
        }
    }
}

/// Scala dwa gospodarstwa. Zwraca `false`, gdy nie ma miejsca — para zostaje wtedy
/// osobno i spróbuje w następnym miesiącu.
fn polacz_gospodarstwa(world: &mut World, a: u32, b: u32, day: u64) -> bool {
    let (Some(ea), Some(eb)) = (encja_gospodarstwa(world, a), encja_gospodarstwa(world, b)) else {
        return false;
    };
    let (Some(ha), Some(hb)) = (
        world.get::<Household>(ea).copied(),
        world.get::<Household>(eb).copied(),
    ) else {
        return false;
    };
    // Zostaje gospodarstwo z lokalem, a przy remisie liczniejsze.
    let (docelowe, zrodlo) = if (ha.has_home(), ha.size) >= (hb.has_home(), hb.size) {
        ((ea, a), (eb, b))
    } else {
        ((eb, b), (ea, a))
    };

    let zrodlowe = world
        .get::<Household>(zrodlo.0)
        .copied()
        .unwrap_or_default();
    let przenoszeni =
        household::members_of(zrodlo.1, &zrodlowe, world.resource::<HouseholdOverflow>());
    let mut cel = world
        .get::<Household>(docelowe.0)
        .copied()
        .unwrap_or_default();
    for m in przenoszeni.iter() {
        let ov = world.resource_mut::<HouseholdOverflow>();
        if !household::add_member(docelowe.1, &mut cel, ov, *m) {
            return false;
        }
    }
    for m in przenoszeni.iter() {
        let Some(c) = encja(world, *m) else { continue };
        if let Some(id) = world.get_mut::<Identity>(c) {
            id.household = docelowe.1;
        }
        if let Some(res) = world.get_mut::<Residence>(c) {
            res.building = cel.building;
            res.unit = cel.unit;
            res.district = cel.district;
        }
    }

    // Majątek gospodarstwa idzie za ludźmi — inaczej znikałby przy każdym ślubie.
    cel.cash = cel.cash.checked_add(zrodlowe.cash).unwrap_or(cel.cash);
    cel.bank = cel.bank.checked_add(zrodlowe.bank).unwrap_or(cel.bank);
    cel.savings = cel
        .savings
        .checked_add(zrodlowe.savings)
        .unwrap_or(cel.savings);
    cel.debt = cel.debt.checked_add(zrodlowe.debt).unwrap_or(cel.debt);
    cel.income_monthly = cel
        .income_monthly
        .checked_add(zrodlowe.income_monthly)
        .unwrap_or(cel.income_monthly);
    przeklasyfikuj(world, docelowe.1, &mut cel, day);
    if let Some(slot) = world.get_mut::<Household>(docelowe.0) {
        *slot = cel;
    }

    // Opuszczony lokal wraca do puli, gospodarstwo znika.
    if zrodlowe.has_home() {
        world
            .resource_mut::<crate::migration::Vacancies>()
            .release_home(crate::migration::HomeSlot {
                building: zrodlowe.building,
                unit: zrodlowe.unit,
                district: zrodlowe.district,
                value: Money::ZERO,
            });
    }
    world
        .resource_mut::<HouseholdOverflow>()
        .clear_household(zrodlo.1);
    world
        .resource_mut::<crate::migration::Unsettled>()
        .forget(zrodlo.1);
    world
        .resource_mut::<Population>()
        .remove_household(zrodlo.0);
    let mut cmd = CommandBuffer::new(demography_system_id());
    cmd.despawn(zrodlo.0);
    magnat_ecs::flush_commands(world, std::slice::from_mut(&mut cmd));
    true
}

fn rozstania(world: &mut World, day: u64, raport: &mut MonthReport) {
    let tabela = world.resource::<DemographyTable>().clone();
    let spis: Vec<Entity> = world.resource::<Population>().citizens().to_vec();

    for e in spis {
        let l = world.get::<Lifecycle>(e).copied().unwrap_or_default();
        if l.partner == Lifecycle::NO_PARTNER || l.partner < e.index() {
            continue;
        }
        let Some(partner) = encja(world, l.partner) else {
            continue;
        };
        let (Some(a), Some(b)) = (
            world.get::<Identity>(e).copied(),
            world.get::<Identity>(partner).copied(),
        ) else {
            continue;
        };
        if a.household != b.household || (day % 65_536) < u64::from(l.next_event_day) {
            continue;
        }

        let stres = world
            .get::<Vitals>(e)
            .map_or(0, |v| v.stress)
            .max(world.get::<Vitals>(partner).map_or(0, |v| v.stress));
        let hh = encja_gospodarstwa(world, a.household)
            .and_then(|h| world.get::<Household>(h).copied())
            .unwrap_or_default();
        let p = tabela.divorce_permille(Q::new(stres), hh.debt, hh.income_monthly);
        let mut r = rng(
            world.seed,
            StreamId::Relations,
            e.index(),
            stream_key(day, K_SEPARATION),
        );
        if !r.gen_bool_permille(p) {
            continue;
        }

        // Długość związku liczona od najwcześniejszej możliwej daty ślubu, czyli od
        // momentu, w którym `next_event_day` przestał być terminem ślubu — z dokładnością
        // do miesiąca, bo dokładniejsza data nie ma gdzie mieszkać, a i tak jest tylko
        // treścią wyjaśnienia (00 §7), nie wejściem żadnej reguły.
        let razem =
            ((day.saturating_sub(u64::from(l.next_event_day))) / DAYS_PER_YEAR).min(255) as u8;
        for kto in [e, partner] {
            if let Some(lc) = world.get_mut::<Lifecycle>(kto) {
                lc.partner = Lifecycle::NO_PARTNER;
                lc.flags &= !Lifecycle::FLAG_PARTNERED;
                lc.next_event_day = 0;
            }
        }
        let _ = b;
        // Wyprowadza się ten z wyższym indeksem — dzieci zostają z drugim.
        // Brak pustostanu nie blokuje rozstania: gospodarstwo bez lokalu obsługuje
        // migracja (§5.7, `settle_grace_days`), a zmuszanie pary do wspólnego życia
        // z powodu rynku mieszkaniowego byłoby regułą, której nikt nie zapisał.
        // Przeprowadzka jest lokalna albo jej nie ma: w mieście bez pustostanu
        // w dzielnicy rozstana para mieszka dalej razem, a próbuje ponownie
        // w następnym miesiącu. Związek jest już rozwiązany w obu `Lifecycle`.
        // Podział majątku i długu na dwie równe części (`R2-WP8`). Dzieci zostają
        // z jednym z rodziców, ale nie są stroną umowy majątkowej, więc podział jest
        // na dwoje, a nie na tylu, ilu domowników.
        let stare = encja_gospodarstwa(world, a.household);
        if let (Some(stare), Some(nowe)) = (
            stare,
            crate::migration::zaloz_gospodarstwo(world, partner, day, Czesci::Polowa),
        ) {
            // `LoanBook` wiąże kredyt z gospodarstwem, więc raty nadal pilnuje jedna
            // strona — R2 odmawia gubienia kwoty, a nie przepisuje umowy kredytowej.
            let polowa = world.get_mut::<Household>(stare).map(Household::split_debt);
            if let (Some(polowa), Some(h)) = (polowa, world.get_mut::<Household>(nowe)) {
                h.debt = polowa;
            }
        }
        raport.separations += 1;
        raport.reasons.push((
            e.index(),
            DecisionReason::Citizen(CitizenReason::SeparationFiled {
                stress: Q::new(stres),
                years_together: razem,
            }),
        ));
    }
}

/// Wyjazd mieszkańca z miasta: zwolnienie relacji i slabów, wypisanie ze spisu.
///
/// Różni się od zgonu tym, że nie ma dziedziczenia — majątek jedzie z człowiekiem,
/// więc trafia do licznika `Population::emigrated`, żeby test zachowania pieniądza
/// (00 §6) widział, dokąd poszedł.
pub fn wyprowadz_mieszkanca(world: &mut World, e: Entity, cmd: &mut CommandBuffer) {
    let majatek = world.get::<Wealth>(e).copied().unwrap_or_default();
    let suma = majatek
        .cash
        .checked_add(majatek.personal_assets)
        .unwrap_or(majatek.cash);
    let p = world.resource_mut::<Population>();
    p.emigrated = p.emigrated.checked_add(suma).unwrap_or(p.emigrated);

    if let Some(partner) = world
        .get::<Lifecycle>(e)
        .map(|l| l.partner)
        .filter(|x| *x != Lifecycle::NO_PARTNER)
        .and_then(|x| encja(world, x))
    {
        if let Some(l) = world.get_mut::<Lifecycle>(partner) {
            l.partner = Lifecycle::NO_PARTNER;
            l.flags &= !Lifecycle::FLAG_PARTNERED;
        }
    }
    crate::migration::release_job_of(world, e);
    usun_relacje(world, e);
    zwolnij_slaby(world, e);
    world.resource_mut::<LifeQueue>().cancel(e.index());
    if let Some(id) = world.get_mut::<Identity>(e) {
        id.flags &= !Identity::FLAG_ALIVE;
    }
    if let Some(w) = world.get_mut::<Wealth>(e) {
        w.cash = Money::ZERO;
        w.personal_assets = Money::ZERO;
    }
    world.resource_mut::<Population>().remove_citizen(e);
    cmd.despawn(e);
}
