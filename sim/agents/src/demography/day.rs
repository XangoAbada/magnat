use super::*;
use magnat_core::CitizenReason;

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
    opieka(world, day, &mut raport);
    raport
}

/// Przegląd opieki nad gospodarstwami (`R2-WP4`), raz na dobę.
///
/// Jedno przejście po gospodarstwach, a nie hak przy każdym zdarzeniu, które może
/// zabrać domowi ostatniego dorosłego — a takich zdarzeń jest cztery i wszystkie
/// są w różnych plikach: zgon, wyprowadzka do własnego lokalu, usamodzielnienie
/// i wyjazd z miasta. Hak w każdym z nich byłby czterema kopiami jednej reguły
/// i pierwsza z nich rozjechałaby się przy pierwszej piątej ścieżce (`R2` §3 pkt 2).
///
/// Cena to przejście po gospodarstwach na dobę — ułamek przejścia po mieszkańcach,
/// które ta sama doba i tak wykonuje w hazardach.
fn opieka(world: &mut World, day: u64, raport: &mut DayReport) {
    let domy: Vec<Entity> = world.resource::<Population>().households().to_vec();
    let mut cmd = CommandBuffer::new(demography_system_id());
    for hh_e in domy {
        if let Some(powod) = ustal_opiekuna(world, hh_e, day, &mut cmd) {
            raport.reasons.push(powod);
        }
    }
    magnat_ecs::flush_commands(world, std::slice::from_mut(&mut cmd));
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
                        DecisionReason::Citizen(CitizenReason::LifeEvent {
                            kind: LifeEventKind::Born,
                        }),
                    ));
                }
            }
            LifeTaskKind::Recover => {
                if let Some(l) = world.get_mut::<Lifecycle>(actor) {
                    l.flags &= !Lifecycle::FLAG_ILL;
                    raport.recoveries += 1;
                    raport.reasons.push((
                        t.actor,
                        DecisionReason::Citizen(CitizenReason::LifeEvent {
                            kind: LifeEventKind::Recovered,
                        }),
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
                DecisionReason::Citizen(CitizenReason::LifeEvent {
                    kind: LifeEventKind::Retired,
                }),
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
        //     emerytura wyżej.
        //
        //     `R2-WP1` dokłada do flagi **placówkę** i **wyjście ze szkoły**. Uczeń
        //     bez placówki nie jest odprowadzany (`household::roles` wymaga
        //     `site != NO_SITE`) i nie ma szkoły w planie dnia, więc sama flaga
        //     zostawiała dziecko urodzone w grze poza całym mechanizmem. Wyjście
        //     zwalnia `site`, bo osiemnastolatek z kluczem szkoły w komponencie
        //     wygląda dla rynku pracy na zatrudnionego — i wyglądał tak do końca
        //     życia, skoro nikt tego pola nie kasował.
        let w_szkole = world.get::<Employment>(e).is_some_and(Employment::is_pupil);
        // Etat wygrywa z wiekiem. Przy dzisiejszych danych (`school_end == work_start`)
        // te dwa stany się nie stykają, ale `labour_force.min` jest niższe od obu,
        // więc pracujący nastolatek jest możliwy — a oznaczony jako uczeń zniknąłby
        // z liczby pracujących w gospodarstwie, nie oddając przy tym etatu.
        //
        // **`is_employed`, nie `has_job`**: uczeń z generacji ma w `site` szkołę, więc
        // pod `has_job` wyglądał na pracującego i cykl zdejmował mu flagę przy pierwszej
        // jego dobie shardu — czyli rocznik z Etapu 8 przestawał być uczniami w ciągu
        // pierwszego roku gry, zachowując przy tym przypisaną szkołę.
        let pracuje = world
            .get::<Employment>(e)
            .is_some_and(Employment::is_employed);
        let wiek_szkolny =
            !pracuje && wiek >= i32::from(ages.school_start) && wiek < i32::from(ages.school_end);
        if wiek_szkolny != w_szkole {
            if wiek_szkolny {
                do_szkoly(world, e);
            } else {
                ze_szkoly(world, e, day, &tabela);
                // Wyjście ze szkoły jest w dzisiejszych danych **osiemnastką**
                // (`school_end == ages.adult`), więc gospodarstwo traci dziecko,
                // a zyskuje dorosłego. Bez przeliczenia składu skala ekwiwalentna
                // (`R2-WP11`) i typ gospodarstwa czekałyby na najbliższą zmianę
                // składu, czyli do wyprowadzki z gniazda (`leave_nest`).
                if let Some(hh_idx) = world.get::<Identity>(e).map(|i| i.household) {
                    if let Some(hh_e) = encja_gospodarstwa(world, hh_idx) {
                        if let Some(mut hh) = world.get::<Household>(hh_e).copied() {
                            przeklasyfikuj(world, hh_idx, &mut hh, day);
                            if let Some(slot) = world.get_mut::<Household>(hh_e) {
                                *slot = hh;
                            }
                        }
                    }
                }
                raport.reasons.push((
                    e.index(),
                    DecisionReason::Citizen(CitizenReason::LifeEvent {
                        kind: LifeEventKind::LeftSchool,
                    }),
                ));
            }
        } else if wiek_szkolny && !world.get::<Employment>(e).is_some_and(Employment::has_job) {
            // Uczeń, który placówki nie dostał (miasto bez szkoły w zasięgu w chwili
            // wejścia, albo szkoła postawiona później), próbuje ponownie. Bez tego
            // jedna nieudana doba zostawiałaby go bez szkoły na całe jedenaście lat.
            do_szkoly(world, e);
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
                    DecisionReason::Citizen(CitizenReason::LifeEvent {
                        kind: LifeEventKind::FellIll,
                    }),
                ));
            }
        }

        // 4. Poczęcie.
        if !identity.is_male() && poczecie(world, e, day, wiek, &tabela, &mut r) {
            raport.conceptions += 1;
            raport.reasons.push((
                e.index(),
                DecisionReason::Citizen(CitizenReason::LifeEvent {
                    kind: LifeEventKind::Conceived,
                }),
            ));
        }
    }

    if !zgony.is_empty() {
        raport.deaths += zgony.len() as u32;
        let mut cmd = CommandBuffer::new(demography_system_id());
        for e in zgony {
            raport.reasons.push((
                e.index(),
                DecisionReason::Citizen(CitizenReason::LifeEvent {
                    kind: LifeEventKind::Died,
                }),
            ));
            for (h, permille) in smierc(world, e, day, hooks, &mut cmd) {
                raport.reasons.push((h, permille));
            }
        }
        magnat_ecs::flush_commands(world, std::slice::from_mut(&mut cmd));
    }
}

// ── szkoła ──────────────────────────────────────────────────────────────────────

/// Wejście do szkoły: flaga plus placówka.
///
/// Placówkę wybiera ta sama reguła, którą Etap 8 generatora rozdaje szkoły całemu
/// rocznikowi (`places::nearest_school`) — jedna wiedza, jedna implementacja. Świat
/// bez katalogu miejsc albo bez szkoły w zasięgu zostawia ucznia z samą flagą;
/// następna doba shardu spróbuje ponownie.
fn do_szkoly(world: &mut World, e: Entity) {
    let dom = world
        .get::<Residence>(e)
        .and_then(crate::places::home_of)
        .and_then(|p| {
            world
                .get_resource::<crate::places::PlaceCatalog>()
                .and_then(|k| k.get().and_then(|t| t.coord_of(p)))
        });
    let szkola = dom.and_then(|at| {
        world
            .get_resource::<crate::places::PlaceCatalog>()
            .and_then(|k| k.get().and_then(|t| crate::places::nearest_school(t, at)))
    });
    if let Some(emp) = world.get_mut::<Employment>(e) {
        emp.flags |= Employment::FLAG_PUPIL;
        // Bezrobotny uczeń to nieprawda w obie strony: siedmiolatek nie szuka pracy
        // i nie wchodzi do mianownika stopy bezrobocia (`labour_force.min` = 16).
        emp.flags &= !Employment::FLAG_UNEMPLOYED;
        if let Some(klucz) = szkola {
            emp.site = klucz;
            emp.work_days = Employment::WEEKDAYS;
            emp.shift = crate::components::ShiftKind::Early as u8;
        }
    }
}

/// Wyjście ze szkoły: placówka zwolniona, wykształcenie policzone.
///
/// Zwolnienie `site` jest tu połową naprawy, a nie sprzątaniem: absolwent z kluczem
/// szkoły w komponencie wygląda dla rynku pracy i dla indeksu relacji na kogoś, kto
/// ma miejsce — więc nie szuka pracy i nie zasila puli kandydatów.
fn ze_szkoly(world: &mut World, e: Entity, day: u64, tabela: &DemographyTable) {
    let ages = tabela.ages();
    let lat = world
        .get::<Identity>(e)
        .map_or(0, |i| i.age_years(day as i32))
        .clamp(0, i32::from(u8::MAX));
    // Ile lat faktycznie przechodził: od `school_start` do dziś, przycięte do pełnego
    // cyklu. Piętnastolatek, który wziął etat, ma osiem lat nauki, a nie jedenaście.
    // `ponytail:` lata nauki liczą się od `school_start`, a nie od dnia, w którym
    // ten mieszkaniec faktycznie wszedł do szkoły — bo nikt tego dnia nie pamięta.
    // Sufit jest widoczny w zachowaniu: przybysz, który przyjechał w wieku czternastu
    // lat z flagą ucznia, wyjdzie stąd z wykształceniem pełnego cyklu za cztery lata
    // chodzenia. Droga wyjścia kosztuje bajt na mieszkańca (wiek wejścia w `Employment`)
    // i należy do fazy, która będzie miała dla niego drugiego czytelnika.
    let lat_nauki = (lat.min(i32::from(ages.school_end)) - i32::from(ages.school_start)).max(0);
    let mut chodzil = false;
    if let Some(emp) = world.get_mut::<Employment>(e) {
        // Obie flagi, nie jedna: `is_pupil()` pyta o `FLAG_PUPIL | FLAG_STUDENT`,
        // więc zdjęcie samej pierwszej zostawiłoby studenta w stanie, z którego
        // cykl próbowałby go wypisywać ze szkoły raz na rok do końca życia —
        // zabierając mu przy okazji etat, gdyby jakiś zdążył dostać.
        emp.flags &= !(Employment::FLAG_PUPIL | Employment::FLAG_STUDENT);
        if emp.site != Employment::NO_SITE {
            chodzil = true;
            emp.site = Employment::NO_SITE;
            emp.work_days = 0;
        }
        // Kto wychodzi ze szkoły w wieku produkcyjnym, wchodzi na rynek pracy;
        // kto wcześniej (praca zamiast szkoły), ma już etat i flaga byłaby
        // nieprawdą — dlatego pyta o wiek, a nie o samo wyjście. **Poza gałęzią
        // placówki**: uczeń, któremu miasto nigdy szkoły nie dało, jest w wieku
        // osiemnastu lat tak samo bezrobotny jak ten, który ją skończył.
        if lat >= i32::from(ages.labour_force.min) && !emp.is_employed() {
            emp.flags |= Employment::FLAG_UNEMPLOYED;
        }
    }
    // Wykształcenie dostaje ten, kto **miał do czego chodzić**. Miasto bez szkoły
    // w zasięgu zostawia dziecko z samą flagą wieku szkolnego — i wtedy osiemnastka
    // wychodzi z zerem, a nie z wykształceniem średnim z tabeli. To jest cały wpływ
    // sieci placówek na kapitał ludzki, dopóki M8d nie doda jakości pojedynczej szkoły.
    if chodzil {
        edukacja(world, e, lat_nauki as u16, tabela);
    }
}

/// Wykształcenie po wyjściu ze szkoły (`R2-WP5`).
///
/// Do tej naprawy `Vitals::edu_level` ustawiał **wyłącznie** Etap 8 generatora, a
/// noworodek dostawał `Vitals::default()`, czyli zero. Skutek był cichy i rósł z każdym
/// rocznikiem: `skill_ceiling` (`sim/firms::hr`) liczy sufit umiejętności z wykształcenia,
/// więc każdy urodzony w grze miał sufit „bez wykształcenia" do końca życia — gorszy
/// niż ktokolwiek z generacji.
///
/// Poziom **nigdy nie spada**. Przybysz z wyższym wykształceniem, który dokończył tu
/// dwa lata szkoły, nie ma go stracić dlatego, że lokalny licznik lat nauki zaczął
/// się liczyć od jego przyjazdu.
fn edukacja(world: &mut World, e: Entity, lat_nauki: u16, tabela: &DemographyTable) {
    let poziom = tabela.education_level(lat_nauki);
    if let Some(v) = world.get_mut::<Vitals>(e) {
        v.edu_level = v.edu_level.max(poziom);
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
    let opieka = world
        .get_resource::<magnat_core::ServiceCoverage>()
        .map_or(0, |c| {
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
    // **Osiem niezależnych losowań, a nie mieszanka cech rodziców** (`R2-WP6`,
    // `D-N4`) — i to jest rozstrzygnięcie, a nie brak. Dziedziczenie osobowości
    // wymaga stanu per mieszkaniec, a `M10a` §5.8 opiera most makro↔mezo na tym,
    // że cechy są funkcją `(world_seed, birth_index)` i nigdy nie są przechowywane.
    // Zmiana należałaby do kontraktu M10, nie do `sim/agents`.
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
        .with(crate::components::BrandsRef::default())
        .with(Lifecycle::default())
        .id();

    world.resource_mut::<Population>().add_citizen(dziecko);
    world.resource_mut::<Population>().births += 1;

    // Miejsce w gospodarstwie. Brak miejsca (ponad `HH_MAX_MEMBERS`) był do `R2-WP3`
    // **milczeniem**: dziecko dostawało `Identity.household` wskazujące na dom rodziców
    // i nie wchodziło do ich składu, więc nie widziała go ani klasyfikacja, ani role,
    // ani karta gospodarstwa — a karta mieszkańca pokazywała je pod tym adresem.
    // Teraz zakłada gospodarstwo pochodne pod tym samym adresem i jest gdzieś w całości.
    let mut hh_kopia = world.get::<Household>(hh_e).copied().unwrap_or_default();
    let zmiescil_sie = {
        let ov = world.resource_mut::<HouseholdOverflow>();
        household::add_member(hh_idx, &mut hh_kopia, ov, dziecko.index())
    };
    if let Some(slot) = world.get_mut::<Household>(hh_e) {
        *slot = hh_kopia;
    }
    if !zmiescil_sie {
        crate::migration::zaloz_gospodarstwo_pochodne(world, dziecko, &hh_kopia, day);
    }

    // Relacje rodzinne — obustronne z definicji (`prop_relation_symmetry`).
    //
    // Matka i ojciec **najpierw i jawnie**: to jest wiedza mocniejsza niż wiek,
    // a `zwiaz_rodzine` nie nadpisuje istniejącego wpisu. Bez tego kroku
    // czterdziestopięciolatka z dorosłym dzieckiem w domu wyszłaby z reguły
    // pokoleniowej babcią własnego noworodka.
    let waga = world.resource::<DemographyTable>().social().family_weight;
    powiaz(world, matka, dziecko, RelationKind::Child, waga, day);
    if let Some(o) = ojciec.and_then(|i| encja(world, i)) {
        powiaz(world, o, dziecko, RelationKind::Child, waga, day);
    }
    // Reszta domu — rodzeństwo i dziadkowie — jedną regułą, tą samą, którą wiąże
    // napływ migracyjny (`R2-WP2`). Przedtem stała tu druga pętla i dawała babci
    // relację rodzeństwa z wnukiem.
    let sklad: Vec<Entity> =
        household::members_of(hh_idx, &hh_kopia, world.resource::<HouseholdOverflow>())
            .iter()
            .filter_map(|m| encja(world, *m))
            .collect();
    zwiaz_rodzine(world, &sklad, day);
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
    let mut suma = majatek
        .cash
        .checked_add(majatek.personal_assets)
        .unwrap_or(majatek.cash);

    // Majątek gospodarstwa wchodzi do masy spadkowej, kiedy odchodzi **ostatni**
    // domownik (`R2-WP8`, `K-61`). Wcześniej gospodarstwo zostawało z resztą
    // składu i saldo należy do niego, nie do zmarłego.
    //
    // Dlaczego tutaj, a nie w `rozwiaz_gospodarstwo`: to tam gospodarstwo znika,
    // ale `smierc` woła przedtem `usun_relacje`, więc pod tamtym adresem lista
    // spadkobierców jest już z definicji pusta i cały majątek szedłby na konto
    // techniczne. Reguła podziału jest jedna i stoi w jednym miejscu.
    if let Some(sakiewka) = majatek_ostatniego(world, e) {
        suma = suma.checked_add(sakiewka.total()).unwrap_or(suma);
    }

    // Zobowiązania masy schodzą **przed** podziałem (`R2-WP10`): spadkobierca dziedziczy
    // to, co zostało po wierzycielach, a nie kwotę brutto. Hak, który coś tu zabierze,
    // wpłaca to w tej samej operacji — inaczej pieniądz zniknąłby ze świata.
    let obciazenie = hooks.estate_charge(world, CitizenId(e), suma);
    let suma = Money((suma.get() - obciazenie.get().max(0)).max(0));

    let mut lista: Vec<(CitizenId, u16)> = Vec::with_capacity(spadkobiercy.len());
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
                DecisionReason::Citizen(CitizenReason::Inheritance {
                    permille: p,
                    heirs: spadkobiercy.len().min(255) as u8,
                }),
            ));
        }
    }
    // **Zawsze**, także z pustą listą (`R2-WP10`). Do R2 hak stał w gałęzi „są
    // spadkobiercy", więc udziały w firmie po bezdzietnym właścicielu zostawały
    // przy nieżyjącym mieszkańcu i nie miał ich kto przejąć.
    hooks.on_inheritance(world, CitizenId(e), &lista, cmd);
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

// ── opiekun prawny (`R2-WP4`) ───────────────────────────────────────────────────

/// Ustala opiekuna gospodarstwa, jeśli jest potrzebny, i zwraca powód do dziennika.
///
/// Do R2 śmierć ostatniego dorosłego nie robiła **nic**: gospodarstwo trwało dalej,
/// klasyfikacja dawała `FamilyWithKids`, a lista odprowadzanych była czyszczona, bo
/// dorosłych było zero. Dzieci zostawały same i słowo „sierota" nie występowało
/// w kodzie symulacji w żadnym znaczeniu domenowym.
///
/// Opiekun jest potrzebny, gdy w składzie jest dziecko i **nie ma dorosłego**.
/// Szuka się go w trzech krokach, każdy deterministyczny:
/// 1. krewny (`Parent`/`Sibling`/`Grandparent`/`Partner`) któregokolwiek z dzieci —
///    najwyższa waga relacji, przy remisie najniższy indeks encji;
/// 2. dowolny dorosły z relacją do któregokolwiek dziecka, ta sama reguła wyboru;
/// 3. brak kandydata — dzieci **wyjeżdżają z miasta** (`D-N8`). Wariant
///    „gospodarstwo instytucjonalne" wymaga usługi publicznej i należy do M8d,
///    a „dziecko zostaje samo" jest tym, co naprawiamy.
fn ustal_opiekuna(
    world: &mut World,
    hh_e: Entity,
    day: u64,
    cmd: &mut CommandBuffer,
) -> Option<(u32, DecisionReason)> {
    let hh = world.get::<Household>(hh_e).copied()?;
    if !hh.is_active() {
        return None;
    }
    let tabela = world.resource::<DemographyTable>().clone();
    let ages = tabela.ages();
    let prog_opieki = tabela.care_health_threshold();
    let sklad = household::members_of(hh_e.index(), &hh, world.resource::<HouseholdOverflow>());
    let mut dzieci: Vec<Entity> = Vec::new();
    let mut dorosli: Vec<Entity> = Vec::new();
    let mut niedolezni: Vec<Entity> = Vec::new();
    for m in sklad.iter() {
        let Some(c) = encja(world, *m) else { continue };
        let wiek = world
            .get::<Identity>(c)
            .map_or(0, |i| i.age_years(day as i32));
        if wiek < i32::from(ages.adult) {
            dzieci.push(c);
            continue;
        }
        dorosli.push(c);
        let zdrowie = world.get::<Vitals>(c).map_or(100, |v| v.health);
        if wiek >= i32::from(ages.senior) && zdrowie < prog_opieki {
            niedolezni.push(c);
        }
    }

    // Dwa powody, jedna reguła. Sierotą jest gospodarstwo z dzieckiem i bez dorosłego;
    // niedołężnym — takie, w którym **każdy** dorosły jest seniorem o zdrowiu poniżej
    // progu. Drugi warunek jest lustrem pierwszego po drugiej stronie wieku i miał
    // do R2 dokładnie te same skutki: żadnych.
    let sierota = dorosli.is_empty() && !dzieci.is_empty();
    let niedolezne = !dorosli.is_empty() && niedolezni.len() == dorosli.len();
    let podopieczni: &[Entity] = if sierota { &dzieci } else { &niedolezni };
    if !sierota && !niedolezne {
        if let Some(slot) = world.get_mut::<Household>(hh_e) {
            slot.guardian = Household::NO_MEMBER;
        }
        return None;
    }

    let kandydat = kandydat_na_opiekuna(world, podopieczni, i32::from(ages.adult), day)
        .or_else(|| sasiad_z_dzielnicy(world, hh.district, i32::from(ages.adult), day));
    let Some((opiekun, waga, rodzina)) = kandydat else {
        if sierota {
            // `D-N8`, i to jest **przypadek zwyrodniały**, a nie zwykła ścieżka:
            // żeby tu dojść, w całej dzielnicy nie może być ani jednego dorosłego.
            // Wtedy dzieci wyjeżdżają z miasta, tak samo jak dorosły bez pracy
            // i bez lokalu; rejestruje to `Population::departures`, więc
            // `prop_population_identity` widzi ubytek jako wyjazd, a nie jako dziurę.
            crate::migration::wyprowadz(world, hh_e, cmd);
        }
        // Senior bez nikogo radzi sobie sam — to jest stan dzisiejszy i nie jest
        // usterką. Deportowanie go za brak rodziny byłoby mechaniką, nie naprawą.
        return None;
    };
    if opiekun.index() == hh.guardian {
        return None;
    }
    if let Some(slot) = world.get_mut::<Household>(hh_e) {
        slot.guardian = opiekun.index();
    }
    Some((
        opiekun.index(),
        DecisionReason::Citizen(CitizenReason::GuardianAppointed {
            wards: podopieczni.len().min(255) as u8,
            weight: waga,
            kin: rodzina,
        }),
    ))
}

/// Najlepszy kandydat na opiekuna: `(encja, waga relacji, czy krewny)`.
///
/// Krewny bije obcego niezależnie od wagi — babcia, której dziecko nie odwiedzało od
/// roku, jest bliżej niż sąsiadka, z którą chodzi się na zakupy.
fn kandydat_na_opiekuna(
    world: &World,
    dzieci: &[Entity],
    adult_age: i32,
    day: u64,
) -> Option<(Entity, u8, bool)> {
    let mut najlepszy: Option<(bool, u8, u32, Entity)> = None;
    for dziecko in dzieci {
        let Some(rel) = world.get::<RelationsRef>(*dziecko).copied() else {
            continue;
        };
        // Kopia wpisów: `encja` bierze `&World`, a slab siedzi w tym samym świecie.
        let wpisy: Vec<Relation> = world
            .resource::<RelationSlab>()
            .entries(relations_ref(&rel))
            .to_vec();
        for w in wpisy {
            let Some(c) = encja(world, w.other) else {
                continue;
            };
            let wiek = world
                .get::<Identity>(c)
                .map_or(0, |i| i.age_years(day as i32));
            if wiek < adult_age {
                continue;
            }
            let rodzina = RelationKind::from_u8(w.kind).is_family();
            // Porządek totalny: krewny, potem waga, potem **najniższy** indeks encji.
            let klucz = (rodzina, w.weight, u32::MAX - w.other, c);
            if najlepszy.is_none_or(|b| klucz > (b.0, b.1, b.2, b.3)) {
                najlepszy = Some(klucz);
            }
        }
    }
    najlepszy.map(|(rodzina, waga, _, e)| (e, waga, rodzina))
}

/// Trzeci krok wyboru opiekuna: **pierwszy dorosły z dzielnicy** (`R2-WP4`).
///
/// Wchodzi wtedy, gdy podopieczni nie mają ani jednej relacji z dorosłym — a to jest
/// zwykły stan dziecka, które właśnie straciło oboje rodziców i nie zdążyło poznać
/// nikogo spoza domu. Bez tego kroku jedyną odpowiedzią byłby wyjazd z miasta, czyli
/// kara za brak znajomości.
///
/// Obcy opiekun nie udaje rodziny: powód decyzji niesie `kin: false` i wagę zero,
/// więc karta inspekcji mówi wprost, że to kuratela, a nie babcia.
///
/// `ponytail:` przejście po spisie do pierwszego trafienia, bez indeksu per dzielnica.
/// Sufit nazwany: ścieżka odpala się wyłącznie dla gospodarstwa osieroconego **bez
/// żadnej relacji**, czyli rzędu jednostek na rok gry; indeks kosztowałby pamięć
/// w każdej dobie po to, żeby czasem oszczędzić jedno przejście.
fn sasiad_z_dzielnicy(
    world: &World,
    district: u16,
    adult_age: i32,
    day: u64,
) -> Option<(Entity, u8, bool)> {
    for c in world.resource::<Population>().citizens() {
        let Some(res) = world.get::<Residence>(*c) else {
            continue;
        };
        if res.district != district {
            continue;
        }
        let dorosly = world
            .get::<Identity>(*c)
            .is_some_and(|i| i.is_alive() && i.age_years(day as i32) >= adult_age);
        if dorosly {
            return Some((*c, 0, false));
        }
    }
    None
}

/// Płynny majątek gospodarstwa, jeśli `e` jest jego **ostatnim** członkiem.
///
/// Zdejmuje go z gospodarstwa w tej samej operacji, bo dwa źródła tej samej kwoty
/// przez jeden krok symulacji znaczyłyby pieniądz policzony dwa razy.
fn majatek_ostatniego(world: &mut World, e: Entity) -> Option<crate::household::Purse> {
    let hh_idx = world.get::<Identity>(e)?.household;
    let hh_e = encja_gospodarstwa(world, hh_idx)?;
    let hh = world.get::<Household>(hh_e)?;
    if hh.size != 1 {
        return None;
    }
    let sakiewka = world.get_mut::<Household>(hh_e)?.take_purse();
    (!sakiewka.is_empty()).then_some(sakiewka)
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
///
/// Gospodarstwo pochodne (`FLAG_OVERCROWDED`, `R2-WP3`) **nie oddaje lokalu**: mieszka
/// kątem u innego i nigdy pustostanu nie wzięło. Bez tego warunku ten sam lokal wracałby
/// do puli dwa razy i miasto miałoby mieszkania, których nie ma — a regulator napływu
/// liczy `min(wakaty, pustostany)`.
///
/// **Reszta płynnego majątku idzie na konto techniczne** (`Population::escheat`,
/// `R2-WP8`, `K-61`). Normalnie jest zerem, bo pieniądz zdejmuje przedtem ten, kto
/// odchodzi: masa spadkowa przy zgonie, udział przy wyprowadzce, połowa przy rozstaniu.
/// Zostaje przy wyjeździe z miasta ostatniego domownika — i wtedy ma mieć adres,
/// a nie znikać razem z encją.
fn rozwiaz_gospodarstwo(world: &mut World, hh_e: Entity, hh: &Household, cmd: &mut CommandBuffer) {
    let reszta = world
        .get_mut::<Household>(hh_e)
        .map(Household::take_purse)
        .unwrap_or_default();
    if !reszta.is_empty() {
        let p = world.resource_mut::<Population>();
        p.escheat = p
            .escheat
            .checked_add(reszta.total())
            .unwrap_or(Money(i64::MAX));
    }
    if hh.has_home() && !hh.is_overcrowded() {
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
