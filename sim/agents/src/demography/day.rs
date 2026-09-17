use super::*;

// ── doba ────────────────────────────────────────────────────────────────────────

/// Jedna doba demografii (§5.6).
///
/// Kolejność jest istotna i jest kontraktem: najpierw terminarz (porody, wyzdrowienia),
/// potem hazardy shardu. Odwrotna kolejność kazałaby ciężarnej, która dziś umiera,
/// najpierw urodzić — co jest może i realistyczne, ale przestaje być deterministyczne,
/// bo zależy od tego, czy jej doba porodu wypadła w jej dobie shardu.
pub fn step_day(world: &mut World, day: u64, hooks: &mut dyn InheritanceHook) -> DayReport {
    let mut raport = DayReport::default();
    terminarz(world, day, &mut raport);
    hazardy(world, day, hooks, &mut raport);
    raport
}

fn terminarz(world: &mut World, day: u64, raport: &mut DayReport) {
    let mut zadania: Vec<LifeTask> = Vec::new();
    world
        .resource_mut::<LifeQueue>()
        .take_due(day, &mut zadania);
    for t in zadania {
        let Some(actor) = encja(world, t.actor) else {
            continue;
        };
        match t.kind {
            LifeTaskKind::Birth => {
                if uroda(world, actor, day) {
                    raport.births += 1;
                    raport.reasons.push((
                        t.actor,
                        DecisionReason::LifeEvent {
                            kind: LifeEventKind::Born,
                        },
                    ));
                }
            }
            LifeTaskKind::Recover => {
                if let Some(l) = world.get_mut::<Lifecycle>(actor) {
                    l.flags &= !Lifecycle::FLAG_ILL;
                    raport.recoveries += 1;
                    raport.reasons.push((
                        t.actor,
                        DecisionReason::LifeEvent {
                            kind: LifeEventKind::Recovered,
                        },
                    ));
                }
            }
        }
    }
}

fn hazardy(world: &mut World, day: u64, hooks: &mut dyn InheritanceHook, raport: &mut DayReport) {
    let shard = day % DEMOGRAPHY_SHARDS;
    // `ponytail:` shard jest filtrem w przebiegu po spisie, a nie 360 kubełkami.
    // Sufit znany: jedno dzielenie na mieszkańca na dobę, czyli przy 400 tys.
    // mieszkańców kilka milisekund na dobę gry. Kubełki per shard opłacą się dopiero,
    // gdyby tryb 50× z M12 mierzył się z tym samym rachunkiem co tryb demograficzny.
    let dzisiaj: Vec<Entity> = world
        .resource::<Population>()
        .citizens()
        .iter()
        .copied()
        .filter(|e| u64::from(e.index()) % DEMOGRAPHY_SHARDS == shard)
        .collect();
    if dzisiaj.is_empty() {
        return;
    }

    let tabela = world.resource::<DemographyTable>().clone();
    let ages = tabela.ages();
    // Mnożniki zdarzeniowe (`D11` fazy M8): epidemia podnosi śmiertelność, zapaść
    // gospodarcza obniża dzietność. Świat bez `sim/events` dostaje jedynki.
    let dparams = world
        .get_resource::<crate::worldparams::DemographyParams>()
        .copied()
        .unwrap_or_default();
    let mut zgony: Vec<Entity> = Vec::new();

    for e in dzisiaj {
        let Some(identity) = world.get::<Identity>(e).copied() else {
            continue;
        };
        if !identity.is_alive() {
            continue;
        }
        let wiek = identity.age_years(day as i32);
        let vitals = world.get::<Vitals>(e).copied().unwrap_or_default();
        let needs = world.get::<Needs>(e).copied().unwrap_or_default();
        let mut r = rng(
            world.seed,
            StreamId::Demography,
            e.index(),
            stream_key(day, K_DAILY),
        );

        // 1. Zgon. Wiek ponad sufit tabeli jest pewny — hazard sam z siebie zostawia
        // ogon, a `prop_no_immortals` nie przyjmuje ogona.
        let hazard = if wiek >= i32::from(ages.max) {
            100_000
        } else {
            crate::worldparams::DemographyParams::scale(
                tabela.mortality_per_100k(wiek, vitals.health_q()),
                dparams.mortality_bps,
            )
        };
        if r.gen_range_u32(100_000) < hazard {
            zgony.push(e);
            continue;
        }

        // 2. Emerytura — na najbliższej dobie shardu po przekroczeniu progu.
        let na_emeryturze = world
            .get::<Employment>(e)
            .is_some_and(|x| x.flags & Employment::FLAG_RETIRED != 0);
        if wiek >= i32::from(ages.retirement) && !na_emeryturze {
            // Etat wraca do puli **zanim** zniknie z komponentu — inaczej miasto traci
            // miejsce pracy przy każdej emeryturze i pojemność rynku maleje z każdym
            // pokoleniem (korekta G-7).
            crate::migration::release_job_of(world, e);
            if let Some(emp) = world.get_mut::<Employment>(e) {
                emp.flags |= Employment::FLAG_RETIRED;
                // Emeryt nie jest bezrobotny: `release_job_of` podnosi flagę
                // bezrobocia dla każdego, kto oddaje etat, a tu jest ona nieprawdą.
                emp.flags &= !Employment::FLAG_UNEMPLOYED;
                emp.site = Employment::NO_SITE;
                emp.work_days = 0;
            }
            if let Some(l) = world.get_mut::<Lifecycle>(e) {
                l.flags |= Lifecycle::FLAG_RETIRED;
            }
            raport.retirements += 1;
            raport.reasons.push((
                e.index(),
                DecisionReason::LifeEvent {
                    kind: LifeEventKind::Retired,
                },
            ));
        }

        // 2a. Cykl szkolny. **Dziecko urodzone w grze nie dostawało flagi ucznia**
        //     i nie dostawało jej nigdy: `FLAG_PUPIL` stawiał wyłącznie napływ
        //     migracyjny, w chwili przyjazdu (pozycja 1 wykazu `R2`). Bez tego
        //     szkoła — cokolwiek by robiła — nie miała kogo uczyć poza rocznikami
        //     zasiedlenia świata, a po jednym pokoleniu nie uczyła nikogo.
        //
        //     Zapis i wypis idą tym samym progiem co reszta cyklu życia: na
        //     najbliższej dobie shardu po przekroczeniu wieku, tak samo jak
        //     emerytura wyżej. Uczeń nie dostaje przy tym etatu w szkole — miejsce
        //     w placówce rozdaje pula wakatów (`Vacancies`), a pojemność obwodu
        //     liczy M8d z normatywu; flaga mówi „chodzi do szkoły", a nie „pracuje".
        let w_szkole = world
            .get::<Employment>(e)
            .is_some_and(|x| x.flags & Employment::FLAG_PUPIL != 0);
        // Etat wygrywa z wiekiem. Przy dzisiejszych danych (`school_end == work_start`)
        // te dwa stany się nie stykają, ale `labour_force.min` jest niższe od obu,
        // więc pracujący nastolatek jest możliwy — a oznaczony jako uczeń zniknąłby
        // z liczby pracujących w gospodarstwie, nie oddając przy tym etatu.
        let pracuje = world.get::<Employment>(e).is_some_and(Employment::has_job);
        let wiek_szkolny = !pracuje
            && wiek >= i32::from(ages.school_start)
            && wiek < i32::from(ages.school_end);
        if wiek_szkolny != w_szkole {
            if let Some(emp) = world.get_mut::<Employment>(e) {
                if wiek_szkolny {
                    emp.flags |= Employment::FLAG_PUPIL;
                } else {
                    emp.flags &= !Employment::FLAG_PUPIL;
                }
            }
        }

        // 3. Choroba. C-9: choroba **obniża `Health`** zamiast dokładać flagę do
        // `PlanCtx` — wyzwalacz „chory → wizyta u lekarza" w planerze już czyta
        // `Needs[Health] < 30`, więc nie trzeba niczego podłączać.
        let chory = world
            .get::<Lifecycle>(e)
            .is_some_and(|l| l.flags & Lifecycle::FLAG_ILL != 0);
        if !chory {
            let h = tabela.illness_per_100k(wiek, needs.get(NeedKind::Hygiene));
            if r.gen_range_u32(100_000) < h {
                zachoruj(world, e, day, &tabela, &mut r);
                raport.illnesses += 1;
                raport.reasons.push((
                    e.index(),
                    DecisionReason::LifeEvent {
                        kind: LifeEventKind::FellIll,
                    },
                ));
            }
        }

        // 4. Poczęcie.
        if !identity.is_male() && poczecie(world, e, day, wiek, &tabela, &mut r) {
            raport.conceptions += 1;
            raport.reasons.push((
                e.index(),
                DecisionReason::LifeEvent {
                    kind: LifeEventKind::Conceived,
                },
            ));
        }
    }

    if !zgony.is_empty() {
        raport.deaths += zgony.len() as u32;
        let mut cmd = CommandBuffer::new(demography_system_id());
        for e in zgony {
            raport.reasons.push((
                e.index(),
                DecisionReason::LifeEvent {
                    kind: LifeEventKind::Died,
                },
            ));
            for (h, permille) in smierc(world, e, day, hooks, &mut cmd) {
                raport.reasons.push((h, permille));
            }
        }
        magnat_ecs::flush_commands(world, std::slice::from_mut(&mut cmd));
    }
}

fn zachoruj(world: &mut World, e: Entity, day: u64, tabela: &DemographyTable, r: &mut Rng) {
    let f = &tabela.f;
    let spadek = f.illness_health_drop.min
        + r.gen_range_u32(u32::from(
            f.illness_health_drop.max - f.illness_health_drop.min + 1,
        )) as u8;
    let losowe = u64::from(
        f.illness_days.min
            + r.gen_range_u32(u32::from(f.illness_days.max - f.illness_days.min + 1)) as u8,
    );
    // **Opieka zdrowotna skraca zwolnienie, a nie leczy formę** (M8d WP7, PRD §10.3).
    // Szpital i przychodnia są tym samym kanałem skutku i bierze się z nich lepszą
    // liczbę: przychodnia w dzielnicy załatwia zwykłą infekcję, szpital resztę.
    // Pełne pokrycie skraca chorobę o połowę, zerowe nie zmienia nic — liniowo, bo
    // krzywa bez danych, które by ją uzasadniły, jest ozdobą.
    //
    // Świat bez miasta jako aktora nie ma tej tablicy i choruje tak jak przed M8d.
    let opieka = world.get_resource::<magnat_core::ServiceCoverage>().map_or(0, |c| {
        let d = world
            .get::<crate::Residence>(e)
            .map_or(magnat_core::DistrictId(0), |r| {
                magnat_core::DistrictId(r.district)
            });
        c.at(d, magnat_core::ServiceKind::Hospital)
            .get()
            .max(c.at(d, magnat_core::ServiceKind::Clinic).get())
    });
    let dni = (losowe * u64::from(200 - u16::from(opieka)) / 200).max(1);
    if let Some(n) = world.get_mut::<Needs>(e) {
        let teraz = n.get(NeedKind::Health);
        n.set(NeedKind::Health, teraz.saturating_sub(spadek));
    }
    if let Some(v) = world.get_mut::<Vitals>(e) {
        v.health = v.health.saturating_sub(spadek / 2);
    }
    if let Some(l) = world.get_mut::<Lifecycle>(e) {
        l.flags |= Lifecycle::FLAG_ILL;
    }
    world.resource_mut::<LifeQueue>().schedule(
        day + dni,
        LifeTask {
            kind: LifeTaskKind::Recover,
            actor: e.index(),
        },
    );
}

fn poczecie(
    world: &mut World,
    e: Entity,
    day: u64,
    wiek: i32,
    tabela: &DemographyTable,
    r: &mut Rng,
) -> bool {
    let l = world.get::<Lifecycle>(e).copied().unwrap_or_default();
    if l.flags & Lifecycle::FLAG_PREGNANT != 0 {
        return false;
    }
    let partnered = l.partner != Lifecycle::NO_PARTNER;
    let hh_idx = world.get::<Identity>(e).map_or(u32::MAX, |i| i.household);
    let dzieci = liczba_dzieci(world, hh_idx, tabela.ages().adult, day);
    let housing = world
        .get::<Needs>(e)
        .map_or(Q::MAX, |n| n.get(NeedKind::Housing));
    let h = crate::worldparams::DemographyParams::scale(
        tabela.fertility_per_100k(wiek, dzieci, partnered, housing),
        world
            .get_resource::<crate::worldparams::DemographyParams>()
            .copied()
            .unwrap_or_default()
            .fertility_bps,
    );
    if h == 0 || r.gen_range_u32(100_000) >= h {
        return false;
    }
    if let Some(l) = world.get_mut::<Lifecycle>(e) {
        l.flags |= Lifecycle::FLAG_PREGNANT;
    }
    world.resource_mut::<LifeQueue>().schedule(
        day + u64::from(tabela.gestation_days()),
        LifeTask {
            kind: LifeTaskKind::Birth,
            actor: e.index(),
        },
    );
    true
}

fn liczba_dzieci(world: &World, household: u32, adult_age: u8, day: u64) -> usize {
    let Some(hh_e) = encja_gospodarstwa(world, household) else {
        return 0;
    };
    let Some(hh) = world.get::<Household>(hh_e) else {
        return 0;
    };
    let ov = world.resource::<HouseholdOverflow>();
    household::members_of(household, hh, ov)
        .iter()
        .filter(|m| {
            encja(world, **m)
                .and_then(|c| world.get::<Identity>(c))
                .is_some_and(|i| i.age_years(day as i32) < i32::from(adult_age))
        })
        .count()
}

// ── narodziny ───────────────────────────────────────────────────────────────────

/// Poród. Dziecko dostaje komplet trzynastu komponentów mieszkańca, gospodarstwo matki
/// i relacje rodzinne — nic ponadto: pracy szuka M7, plan dnia ułoży mu planer.
fn uroda(world: &mut World, matka: Entity, day: u64) -> bool {
    let Some(l) = world.get_mut::<Lifecycle>(matka) else {
        return false;
    };
    if l.flags & Lifecycle::FLAG_PREGNANT == 0 {
        return false;
    }
    l.flags &= !Lifecycle::FLAG_PREGNANT;

    let Some(m_id) = world.get::<Identity>(matka).copied() else {
        return false;
    };
    let hh_idx = m_id.household;
    let Some(hh_e) = encja_gospodarstwa(world, hh_idx) else {
        return false;
    };
    let hh = world.get::<Household>(hh_e).copied().unwrap_or_default();
    let ojciec = world
        .get::<Lifecycle>(matka)
        .map(|l| l.partner)
        .filter(|p| *p != Lifecycle::NO_PARTNER);

    let mut r = rng(
        world.seed,
        StreamId::Demography,
        matka.index(),
        stream_key(day, K_BIRTH),
    );
    let plec = if r.gen_bool_permille(500) {
        Identity::FLAG_MALE
    } else {
        0
    };
    let cechy: [u8; 8] = std::array::from_fn(|_| r.gen_q().get());

    // Imię z puli **regionu rodziny**, odczytanego z dziedziczonego nazwiska (§5.10):
    // dziecko Schmidtów nie nazywa się Agnieszka, a `Identity` nie rośnie o pole regionu.
    let nazwy = crate::names::catalog();
    let imie = nazwy.pick_first(
        nazwy.region_of_surname(m_id.last_name),
        plec == Identity::FLAG_MALE,
        &mut r,
    );

    let dziecko = world
        .spawn()
        .with(Identity {
            first_name: imie,
            last_name: m_id.last_name,
            birth_day: day as i32,
            birth_district: hh.district,
            flags: Identity::FLAG_ALIVE | plec,
            _pad: 0,
            household: hh_idx,
        })
        .with(Personality(cechy))
        .with(Vitals {
            health: 90 + r.gen_range_u32(11) as u8,
            energy: 100,
            ..Vitals::default()
        })
        .with(Needs {
            level: [100; magnat_core::NEED_COUNT],
            updated_at: (day * 1440) as u32,
        })
        .with(Skills::default())
        .with(Wealth::default())
        .with(Employment::default())
        .with(Residence {
            building: hh.building,
            unit: hh.unit,
            district: hh.district,
        })
        .with(PlanRef::default())
        .with(AgentState::default())
        .with(KnowledgeRef::default())
        .with(RelationsRef::default())
        .with(Lifecycle::default())
        .id();

    world.resource_mut::<Population>().add_citizen(dziecko);
    world.resource_mut::<Population>().births += 1;

    // Miejsce w gospodarstwie; brak miejsca (ponad `HH_MAX_MEMBERS`) znaczy, że
    // dziecko i tak mieszka z rodzicami — składu się wtedy nie powiększa, ale
    // `Identity.household` zostaje, więc `prop_no_orphan_household` widzi spójność.
    let mut hh_kopia = world.get::<Household>(hh_e).copied().unwrap_or_default();
    {
        let ov = world.resource_mut::<HouseholdOverflow>();
        household::add_member(hh_idx, &mut hh_kopia, ov, dziecko.index());
    }
    if let Some(slot) = world.get_mut::<Household>(hh_e) {
        *slot = hh_kopia;
    }

    // Relacje rodzinne — obustronne z definicji (`prop_relation_symmetry`).
    let waga = world.resource::<DemographyTable>().social().family_weight;
    powiaz(world, matka, dziecko, RelationKind::Child, waga, day);
    if let Some(o) = ojciec.and_then(|i| encja(world, i)) {
        powiaz(world, o, dziecko, RelationKind::Child, waga, day);
    }
    for m in household::members_of(hh_idx, &hh_kopia, world.resource::<HouseholdOverflow>())
        .iter()
        .copied()
        .collect::<Vec<u32>>()
    {
        if m == dziecko.index() || Some(m) == ojciec || m == matka.index() {
            continue;
        }
        let Some(inny) = encja(world, m) else {
            continue;
        };
        let rodzenstwo = world
            .get::<Identity>(inny)
            .is_some_and(|i| i.household == hh_idx);
        if rodzenstwo {
            powiaz(world, inny, dziecko, RelationKind::Sibling, waga, day);
        }
    }
    true
}

// ── zgon i dziedziczenie ────────────────────────────────────────────────────────

/// Zgon: podział majątku, zwolnienie miejsca, usunięcie relacji, despawn.
///
/// Kolejność nie jest dowolna. Spadkobierców szuka się **przed** usunięciem relacji,
/// bo to relacje mówią, kto jest dzieckiem; hook woła się **przed** despawnem, bo M7
/// musi zdążyć przeczytać udziały zmarłego (§5.6).
fn smierc(
    world: &mut World,
    e: Entity,
    day: u64,
    hooks: &mut dyn InheritanceHook,
    cmd: &mut CommandBuffer,
) -> Vec<(u32, DecisionReason)> {
    let mut powody: Vec<(u32, DecisionReason)> = Vec::new();
    let spadkobiercy = spadkobiercy(world, e);
    let majatek = world.get::<Wealth>(e).copied().unwrap_or_default();
    let suma = majatek
        .cash
        .checked_add(majatek.personal_assets)
        .unwrap_or(majatek.cash);

    if spadkobiercy.is_empty() {
        world.resource_mut::<Population>().escheat = world
            .resource::<Population>()
            .escheat
            .checked_add(suma)
            .unwrap_or(Money(i64::MAX));
    } else {
        let wagi: Vec<u64> = vec![1; spadkobiercy.len()];
        let udzialy = magnat_core::split_proportional(suma, &wagi);
        let permil = (1000 / spadkobiercy.len()) as u16;
        let mut lista: Vec<(CitizenId, u16)> = Vec::with_capacity(spadkobiercy.len());
        for (i, h) in spadkobiercy.iter().enumerate() {
            if let Some(w) = world.get_mut::<Wealth>(*h) {
                w.cash = w.cash.checked_add(udzialy[i]).unwrap_or(w.cash);
            }
            let p = if i == 0 {
                permil + (1000 - permil * spadkobiercy.len() as u16)
            } else {
                permil
            };
            lista.push((CitizenId(*h), p));
            powody.push((
                h.index(),
                DecisionReason::Inheritance {
                    permille: p,
                    heirs: spadkobiercy.len().min(255) as u8,
                },
            ));
        }
        hooks.on_inheritance(CitizenId(e), &lista, cmd);
    }
    if let Some(w) = world.get_mut::<Wealth>(e) {
        w.cash = Money::ZERO;
        w.personal_assets = Money::ZERO;
    }

    // Partner zostaje sam.
    let partner = world
        .get::<Lifecycle>(e)
        .map(|l| l.partner)
        .filter(|p| *p != Lifecycle::NO_PARTNER)
        .and_then(|p| encja(world, p));
    if let Some(p) = partner {
        if let Some(l) = world.get_mut::<Lifecycle>(p) {
            l.partner = Lifecycle::NO_PARTNER;
            l.flags &= !Lifecycle::FLAG_PARTNERED;
        }
    }

    crate::migration::release_job_of(world, e);
    usun_relacje(world, e);
    zwolnij_slaby(world, e);
    world.resource_mut::<LifeQueue>().cancel(e.index());
    opusc_gospodarstwo(world, e, day, cmd);

    if let Some(id) = world.get_mut::<Identity>(e) {
        id.flags &= !Identity::FLAG_ALIVE;
    }
    world.resource_mut::<Population>().remove_citizen(e);
    world.resource_mut::<Population>().deaths += 1;
    cmd.despawn(e);
    powody
}

/// Spadkobiercy: współmałżonek, dalej dzieci po równo (§5.6).
fn spadkobiercy(world: &World, e: Entity) -> Vec<Entity> {
    if let Some(p) = world
        .get::<Lifecycle>(e)
        .map(|l| l.partner)
        .filter(|p| *p != Lifecycle::NO_PARTNER)
        .and_then(|p| encja(world, p))
    {
        return vec![p];
    }
    let Some(rel) = world.get::<RelationsRef>(e).copied() else {
        return Vec::new();
    };
    let slab = world.resource::<RelationSlab>();
    let mut dzieci: Vec<Entity> = slab
        .entries(relations_ref(&rel))
        .iter()
        .filter(|r| r.kind == RelationKind::Child as u8)
        .filter_map(|r| encja(world, r.other))
        .collect();
    dzieci.sort_unstable_by_key(|x| x.index());
    dzieci.dedup();
    dzieci
}

pub(super) fn usun_relacje(world: &mut World, e: Entity) {
    let Some(rel) = world.get::<RelationsRef>(e).copied() else {
        return;
    };
    let inni: Vec<u32> = world
        .resource::<RelationSlab>()
        .entries(relations_ref(&rel))
        .iter()
        .map(|r| r.other)
        .collect();
    for idx in inni {
        let Some(inny) = encja(world, idx) else {
            continue;
        };
        let Some(r2) = world.get::<RelationsRef>(inny).copied() else {
            continue;
        };
        let mut sr = relations_ref(&r2);
        let slab = world.resource_mut::<RelationSlab>();
        if let Some(i) = slab.entries(sr).iter().position(|x| x.other == e.index()) {
            slab.remove_at(&mut sr, i);
            if let Some(slot) = world.get_mut::<RelationsRef>(inny) {
                slot.handle = sr.handle;
                slot.len = sr.len;
                slot.class = sr.class;
            }
        }
    }
}

pub(super) fn zwolnij_slaby(world: &mut World, e: Entity) {
    if let Some(rel) = world.get::<RelationsRef>(e).copied() {
        let mut r = relations_ref(&rel);
        world.resource_mut::<RelationSlab>().free(&mut r);
        if let Some(slot) = world.get_mut::<RelationsRef>(e) {
            *slot = RelationsRef::default();
        }
    }
    if let Some(k) = world.get::<KnowledgeRef>(e).copied() {
        let mut r = knowledge_ref(&k);
        world.resource_mut::<KnowledgeSlab>().free(&mut r);
        if let Some(slot) = world.get_mut::<KnowledgeRef>(e) {
            *slot = KnowledgeRef::default();
        }
    }
    if let Some(p) = world.get::<PlanRef>(e).copied() {
        let mut r = p.slab_ref();
        world.resource_mut::<PlanSlab>().free(&mut r);
        if let Some(slot) = world.get_mut::<PlanRef>(e) {
            *slot = PlanRef::default();
        }
    }
}

/// Wypisanie ze składu gospodarstwa; gospodarstwo bez członków **rozwiązuje się**.
///
/// Rozwiązanie oddaje lokal do puli pustostanów i to jest główna droga, którą pustostan
/// w ogóle powstaje (§5.7). Bez niej gospodarstwo zmarłego trzymałoby mieszkanie do końca
/// świata: pustostanów by nie było, `min(wakaty, pustostany)` zostawałoby na zerze,
/// napływ wygasłby, a populacja mogłaby już tylko maleć — regulator przestałby regulować.
pub fn opusc_gospodarstwo(
    world: &mut World,
    e: Entity,
    day: u64,
    cmd: &mut CommandBuffer,
) -> Option<Entity> {
    let hh_idx = world.get::<Identity>(e)?.household;
    let hh_e = encja_gospodarstwa(world, hh_idx)?;
    let mut hh = world.get::<Household>(hh_e).copied()?;
    {
        let ov = world.resource_mut::<HouseholdOverflow>();
        household::remove_member(hh_idx, &mut hh, ov, e.index());
    }
    if hh.size == 0 {
        rozwiaz_gospodarstwo(world, hh_e, &hh, cmd);
        return Some(hh_e);
    }
    przeklasyfikuj(world, hh_idx, &mut hh, day);
    if let Some(slot) = world.get_mut::<Household>(hh_e) {
        *slot = hh;
    }
    Some(hh_e)
}

/// Rozwiązanie pustego gospodarstwa: lokal wraca do puli, wpisy pomocnicze znikają.
fn rozwiaz_gospodarstwo(world: &mut World, hh_e: Entity, hh: &Household, cmd: &mut CommandBuffer) {
    if hh.has_home() {
        world
            .resource_mut::<crate::migration::Vacancies>()
            .release_home(crate::migration::HomeSlot {
                building: hh.building,
                unit: hh.unit,
                district: hh.district,
                value: Money::ZERO,
            });
    }
    world
        .resource_mut::<HouseholdOverflow>()
        .clear_household(hh_e.index());
    world
        .resource_mut::<crate::migration::Unsettled>()
        .forget(hh_e.index());
    world.resource_mut::<Population>().remove_household(hh_e);
    cmd.despawn(hh_e);
}
