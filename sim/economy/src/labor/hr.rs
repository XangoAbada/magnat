//! Kadry: domknięcie po M3, rotacja, świadczenia, premie i szkolenia (M7b WP6).
//!
//! Wszystkie kroki tej doby, które zmieniają **obsadę** albo **człowieka**, a nie rynek.
//! Reguły są w `sim/firms` (`hr::turnover`, `hr::training`) — tu jest kolejność ich
//! stosowania i zapis skutku, bo skutek dotyka trzech miejsc naraz: rejestru firm,
//! komponentów mieszkańca i puli etatów miasta.
//!
//! **Odejście zawsze ma powód po stronie odchodzącego** (kryterium WP6) i zawsze idzie
//! przez [`odejdz`] — jedną drogą, bo etat, który nie wraca do puli, znika z miasta
//! na zawsze (M3c, korekta G-7).

use magnat_core::{
    rng::rng, CitizenId, DecisionReason, JobRoleId, LeaveCause, Money, SimMinute, SiteId, StreamId,
    Tick,
};
use magnat_firms::{
    benefit_cost, benefit_gains, bonus, effective_labor, quit_pressure, severance, should_dismiss,
    trained_skill, update_perf, Firms,
};

use super::{LaborDay, LaborMarket, Seeker, Workforce};

/// Pokrycie etatowe wszystkich zakładów w promilach (`K-44`).
///
/// Osobno od zapisu do `PlantSite`, bo to jest cała treść tego kroku: **ile pracy
/// naprawdę stoi za tym zakładem**. Zapis jest pętlą po wyniku i nie ma czego zepsuć;
/// psuje się tu — w tym, kogo policzono i z jaką formą.
#[must_use]
pub fn labor_coverage(
    firms: &Firms,
    roles: &magnat_firms::RoleTable,
    people: &impl Workforce,
) -> Vec<(SiteId, u16)> {
    firms
        .sites()
        .map(|(id, site)| {
            let pct = site.labor_pct(roles, &|c: CitizenId| {
                let f = people.facts(c)?;
                let (_, role) = f.job?;
                // **Zwolnienie lekarskie zabiera cały etat, a nie jego część** (M8d WP7).
                // Człowiek na zwolnieniu nie jest słabszym pracownikiem — nie ma go
                // w pracy. Osłabienie niesie już `Vitals.health` i to są dwie różne
                // rzeczy: przychodnia skraca nieobecność, a nie poprawia formę.
                if f.on_sick_leave {
                    return None;
                }
                Some((f.vitals, people.skill_in(c, role)))
            });
            (id, pct)
        })
        .collect()
}

/// Kto kieruje zakładem: obsadzone stanowisko kierownicze staje się menedżerem
/// (M7c WP7).
///
/// Bez tego kroku menedżerowie istnieliby wyłącznie tam, gdzie ktoś ich przypisał
/// ręcznie — czyli w testach i u gracza — a miasto stawiane przez most M7a miałoby
/// dziesięć tysięcy zakładów o zarządzaniu dokładnie przeciętnym. Katalog typów
/// zakładów wypisuje stanowiska kierownicze (`Staffing::managerial`), rynek pracy
/// je obsadza; tutaj domyka się pętla: **kto siedzi na tym etacie, ten kieruje**.
///
/// Odejście z etatu kierowniczego zdejmuje menedżera i zakład spada do jakości
/// zastępstwa — tą samą drogą, którą przechodzi menedżer podkupiony przez konkurenta.
///
/// **Polityki tu nie ma i być nie może.** Zestaw reguł firmy AI generuje tier
/// taktyczny (M7e); do tego czasu delegacja niesie politykę pustą, czyli menedżera
/// bez instrukcji. To jest różnica, którą widać w wyniku: jakość zarządzania działa
/// od razu, wykonywanie reguł czeka na źródło reguł.
pub(super) fn reconcile_managers(
    m: &LaborMarket,
    firms: &mut Firms,
    people: &impl Workforce,
    now: SimMinute,
) {
    use magnat_firms::{Autonomy, Manager, ManagerStyle, SiteDelegation};

    let t = m.tuning.manager;
    // Kto powinien kierować którym zakładem: najlepszy z obsadzonych stanowisk
    // kierowniczych. Kolejność po `SiteId`, więc nie zależy od niczego poza rejestrem.
    /// Kandydat na menedżera zakładu: kto, z jaką umiejętnością i w jakim stylu.
    type Kandydat = (CitizenId, magnat_core::Q, ManagerStyle);
    let mut plan: Vec<(SiteId, Option<Kandydat>)> = Vec::new();
    for (id, site) in firms.sites() {
        let mut naj: Option<Kandydat> = None;
        for p in site.positions.iter().filter(|p| p.managerial) {
            for e in &p.filled {
                let Some(f) = people.facts(e.citizen) else {
                    continue;
                };
                let skill = people.skill_in(e.citizen, p.role);
                if naj.is_none_or(|n| skill.get() > n.1.get()) {
                    naj = Some((e.citizen, skill, styl(f.ambition, f.loyalty)));
                }
            }
        }
        let obecny = firms.manager_of_site(id).map(|mgr| mgr.citizen);
        match (obecny, naj) {
            // Ten sam człowiek dalej na stanowisku — nic się nie zmienia.
            (Some(a), Some((b, _, _))) if a == b => continue,
            (None, None) => continue,
            _ => plan.push((id, naj)),
        }
    }

    // Nastroje **raz na przebieg**, nie raz na zakład. Przypisanie menedżera nie
    // zmienia obsady, więc nastrój załogi nie ma jak drgnąć w środku tej pętli —
    // a liczenie go w środku dałoby pierwszej dobie świata 10 tys. × 10 tys.
    // przebiegów po obsadzie, czyli dobę realną zamiast milisekundy.
    let nastroje = site_morale(firms, people);
    let nastroj = |s: SiteId| nastroje.get(&s).copied().unwrap_or(magnat_core::Q::new(50));
    for (id, kandydat) in plan {
        // **Odpinamy ten jeden zakład, nie całego menedżera.** `release_manager`
        // odpowiada na „ten człowiek przestał pracować" i zdejmuje mu wszystkie
        // zakłady — użyta tutaj kosztowałaby zakład, w którym nic się nie zmieniło.
        firms.detach_site(id, &nastroj, &t);
        let Some((c, skill, styl)) = kandydat else {
            continue;
        };
        // Polityka zostaje, jeśli zakład już jakąś nosił — menedżer się zmienił,
        // a instrukcje nie.
        let polityka = firms
            .site(id)
            .and_then(|s| s.delegation.as_ref().map(|d| d.policy.clone()))
            .unwrap_or_else(|| {
                magnat_policy::Policy::empty(
                    magnat_core::PolicyId(0),
                    "",
                    magnat_policy::PolicyDomain::Pricing,
                )
            });
        let mgr = Manager::new(c, skill, styl, now);
        let deleg = SiteDelegation::new(c, polityka, Autonomy::PricesOnly);
        firms.assign_manager(id, mgr, deleg, nastroj, &t, Tick(now.0));
    }
}

/// Styl kierowania z cech dyrektora-mieszkańca.
///
/// **Wyprowadzony z `Personality` M3, a nie losowany**: ambicja i lojalność są już
/// w komponencie mieszkańca i to one mają tłumaczyć, dlaczego ten człowiek kieruje
/// tak, a nie inaczej. Losowanie dałoby ten sam rozkład i zero wyjaśnienia,
/// a M7e podmieni tu źródło na pełną osobowość, nie na inną monetę.
fn styl(ambition: magnat_core::Q, loyalty: magnat_core::Q) -> magnat_firms::ManagerStyle {
    use magnat_firms::ManagerStyle as S;
    match (ambition.get() >= 60, loyalty.get() >= 60) {
        (true, false) => S::Dealmaker,
        (true, true) => S::Taskmaster,
        (false, true) => S::Coach,
        (false, false) => S::Bureaucrat,
    }
}

/// Nastrój załogi zakładu w skali `Q` — wejście jakości zarządzania (M7c §5.4).
///
/// Średnia po obsadzie, w kolejności stanowisk i obsadzenia, nigdy po mapie (00 §3.2).
/// Zakład bez załogi dostaje środek skali: pusty zakład nie jest zakładem o złym
/// nastroju, tylko zakładem bez ludzi — a menedżer nie ma tam czego popsuć.
#[must_use]
pub fn site_morale(
    firms: &Firms,
    people: &impl Workforce,
) -> std::collections::BTreeMap<SiteId, magnat_core::Q> {
    firms
        .sites()
        .map(|(id, site)| {
            let mut suma = 0i32;
            let mut ilu = 0i32;
            for p in &site.positions {
                for e in &p.filled {
                    if let Some(f) = people.facts(e.citizen) {
                        suma += i32::from(f.vitals.mood);
                        ilu += 1;
                    }
                }
            }
            let q = if ilu == 0 {
                50
            } else {
                ((suma / ilu + 100) / 2).clamp(0, 100)
            };
            (id, magnat_core::Q::new(q as u8))
        })
        .collect()
}

/// Ile dób pracuje ten człowiek na tym etacie.
fn staz(since: SimMinute, now: SimMinute) -> u32 {
    ((now.0.saturating_sub(since.0)) / 1440) as u32
}

/// Kto, skąd i dlaczego odchodzi — komplet, bez którego odejścia nie da się zapisać.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct Leave {
    pub citizen: CitizenId,
    pub site: SiteId,
    pub role: JobRoleId,
    pub cause: LeaveCause,
    pub tenure_days: u32,
    /// Gospodarstwo z **umowy**, nie z komponentu odchodzącego (`R2-WP9`).
    /// Przy zgonie i przy wyjeździe z miasta encji mieszkańca już nie ma.
    pub household: u32,
}

/// Wyjście z etatu — jedyna droga (M3c, `release_job_of`). Zwraca stawkę, którą
/// odchodzący miał: potrzebuje jej i płaca progowa bezrobotnego, i odprawa.
pub(super) fn odejdz(
    firms: &mut Firms,
    people: &mut impl Workforce,
    l: Leave,
    now: SimMinute,
) -> Money {
    let Leave {
        citizen: c,
        site,
        role,
        cause,
        tenure_days,
        household,
    } = l;
    let mut stawka = Money::ZERO;
    let mut firma = None;
    if let Some(s) = firms.site_mut(site) {
        firma = Some(s.firm);
        if let Some(p) = s.positions.iter_mut().find(|p| p.role == role) {
            if let Some(i) = p.filled.iter().position(|e| e.citizen == c) {
                stawka = p.filled.remove(i).wage_month;
            }
        }
    }
    if let Some(f) = firma {
        firms.log(
            f,
            Tick(now.0),
            DecisionReason::JobLeft {
                role,
                cause,
                tenure_days: tenure_days.min(u32::from(u16::MAX)) as u16,
            },
        );
    }
    // Zwalniamy **ten** etat, a nie „jakikolwiek": `release` czyści komponent
    // mieszkańca bez pytania o zakład, więc wywołane dla kogoś, kto pracuje gdzie
    // indziej, skasowałoby mu prawdziwą pracę. Dziś taki rozjazd nie powstaje
    // (pilnuje go `K-46`), ale to jest **jedyna** droga wyjścia z etatu i nie wolno
    // jej wierzyć wołającemu na słowo.
    //
    // `f.job.is_none()` jest tu **trzecim** dopuszczalnym przypadkiem i bez niego
    // emeryt zostawiał płacę w dochodzie gospodarstwa (`R2-WP9`): `sim/agents` czyści
    // mu komponent przy przejściu na emeryturę, więc fakty mówią „nie ma pracy",
    // rejestr firm mówi „ma etat tutaj" — i warunek wpadał między jedno a drugie.
    // Bezpieczeństwo zostaje: nikomu, kto pracuje **gdzie indziej**, nadal nic się
    // nie kasuje.
    let zwolnij = people
        .facts(c)
        .is_none_or(|f| f.job.is_none() || f.job == Some((site, role)));
    if zwolnij {
        people.release(c, stawka, household);
    }
    stawka
}

/// Domknięcie po M3: kto zniknął z rynku pracy, ten nie zajmuje etatu (WP6).
///
/// Emeryturę, zgon i wyprowadzkę prowadzi `sim/agents` i on zwalnia komponent. Rejestr
/// firm o tym nie wie, więc bez tego kroku zakład miałby na liście płac ludzi, których
/// nie ma — a wakat po nich nigdy by się nie otworzył.
pub(super) fn reconcile(
    m: &mut LaborMarket,
    firms: &mut Firms,
    people: &mut impl Workforce,
    now: SimMinute,
    d: &mut LaborDay,
) {
    let mut znikli: Vec<(CitizenId, SiteId, JobRoleId, u32, u32)> = Vec::new();
    for (id, site) in firms.sites() {
        for p in &site.positions {
            for e in &p.filled {
                let nadal_tu = people
                    .facts(e.citizen)
                    .is_some_and(|f| f.job == Some((id, p.role)));
                if !nadal_tu {
                    znikli.push((e.citizen, id, p.role, staz(e.since, now), e.household));
                }
            }
        }
    }
    // Ewidencja szukających też wymaga domknięcia, i z tego samego powodu: bezrobotny,
    // który umarł, przeszedł na emeryturę albo wyjechał, **nigdy nie stał na liście płac**,
    // więc pętla wyżej go nie dotknie. Zostałby w mapie do końca gry — a mapa wchodzi
    // do hasha stanu i do zapisu, więc rosłaby przez sto lat bez granicy.
    let martwi: Vec<CitizenId> = m
        .seekers
        .keys()
        .copied()
        .filter(|c| people.facts(*c).is_none())
        .collect();
    for c in martwi {
        m.seekers.remove(&c);
    }
    for (c, site, role, staz, household) in znikli {
        odejdz(
            firms,
            people,
            Leave {
                citizen: c,
                site,
                role,
                cause: LeaveCause::LeftLabourForce,
                tenure_days: staz,
                household,
            },
            now,
        );
        m.seekers.remove(&c);
        d.left_force += 1;
    }
}

/// Rotacja dobrowolna i przymusowa (WP6).
pub(super) fn turnover(
    m: &mut LaborMarket,
    firms: &mut Firms,
    people: &mut impl Workforce,
    seed: u64,
    now: SimMinute,
    d: &mut LaborDay,
) {
    let hr = m.tuning.hr;
    let mt = m.tuning.manager;
    let mut plan: Vec<(CitizenId, SiteId, JobRoleId, LeaveCause, u32, Money, u32)> = Vec::new();
    for (id, site) in firms.sites() {
        for p in &site.positions {
            for e in &p.filled {
                let Some(f) = people.facts(e.citizen) else {
                    continue;
                };
                let staz_dni = staz(e.since, now);
                // Odniesieniem jest mediana zawartych umów w tym zawodzie i w tej
                // dzielnicy — czyli to, co rynek naprawdę płaci, a nie tabela.
                // Zacisk, a nie `as i32`: przy medianie groszowej (pierwsza umowa
                // w zawodzie) iloraz przekracza `i32` i zawija się na ujemny, czyli
                // **przepłacany** pracownik dostawałby maksymalne ciśnienie na odejście.
                let luka = match m.stats.median_accepted(p.role, site.district) {
                    Some(med) if med.get() > 0 => {
                        ((e.wage_month.get() - med.get()).saturating_mul(10_000) / med.get())
                            .clamp(i64::from(i32::MIN), i64::from(i32::MAX))
                            as i32
                    }
                    _ => 0,
                };
                // Jakość zarządzania **mnoży** ciśnienie na odejście — trzeci kanał
                // wpływu menedżera (M7c §5.4). Do M7c rotacja nie zależała od tego,
                // kto zakładem kieruje, choć tabela w planie ten kanał obiecywała.
                let cisnienie = quit_pressure(
                    f.vitals.mood,
                    f.vitals.stress,
                    staz_dni,
                    luka,
                    site.mgmt,
                    &hr,
                    &mt,
                );
                let mut r = rng(seed, StreamId::LaborQuit, e.citizen.0.index(), Tick(now.0));
                if u32::from(cisnienie.per_10k) > r.gen_range_u32(10_000) {
                    plan.push((
                        e.citizen,
                        id,
                        p.role,
                        cisnienie.cause,
                        staz_dni,
                        Money::ZERO,
                        e.household,
                    ));
                } else if should_dismiss(e.perf_ema, e.warnings, &hr) {
                    plan.push((
                        e.citizen,
                        id,
                        p.role,
                        LeaveCause::Dismissed,
                        staz_dni,
                        severance(e.wage_month, staz_dni, &hr),
                        e.household,
                    ));
                }
            }
        }
    }
    for (c, site, role, cause, staz_dni, odprawa, household) in plan {
        let stawka = odejdz(
            firms,
            people,
            Leave {
                citizen: c,
                site,
                role,
                cause,
                tenure_days: staz_dni,
                household,
            },
            now,
        );
        m.seekers.insert(
            c,
            Seeker {
                since_day: m.day,
                last_wage: stawka,
            },
        );
        if odprawa.get() > 0 {
            if let Some(s) = firms.site_mut(site) {
                s.accrue_hr(odprawa);
            }
            d.severance = Money(d.severance.get().saturating_add(odprawa.get()));
        }
        if cause == LeaveCause::Dismissed {
            d.dismissals += 1;
        } else {
            d.quits += 1;
        }
    }
}

/// Świadczenia (codziennie), ocena wyniku (codziennie), premie i szkolenia (raz
/// w miesiącu) — WP6.
///
/// Trzy kroki o trzech kadencjach, każdy w osobnej funkcji: doba, miesiąc i termin
/// szkoleniowy. Dzielą wyłącznie strojenie, więc rozbicie jest przeniesieniem bloku,
/// a nie zmianą kształtu.
pub(super) fn personnel(
    m: &mut LaborMarket,
    firms: &mut Firms,
    people: &mut impl Workforce,
    d: &mut LaborDay,
) {
    let hr = m.tuning.hr;
    let ben = m.tuning.benefits;
    doba_kadrowa(m, firms, people, &ben);
    if m.day.is_multiple_of(30) {
        miesiac_kadrowy(firms, &hr, &ben, d);
    }
    if m.day
        .is_multiple_of(30 * u32::from(hr.training_every_months.max(1)))
    {
        termin_szkoleniowy(firms, people, &hr, d);
    }
}

/// Doba: ocena wyniku każdego pracownika i świadczenia, które realnie podnoszą
/// jego potrzeby.
fn doba_kadrowa(
    m: &LaborMarket,
    firms: &mut Firms,
    people: &mut impl Workforce,
    ben: &magnat_firms::hr::tuning::BenefitTuning,
) {
    // 1. Ocena wyniku i świadczenia. Plan najpierw, bo jedno pyta o mieszkańca,
    //    a drugie do niego pisze.
    let mut podniesienia: Vec<(CitizenId, magnat_core::NeedKind, u8)> = Vec::new();
    let mut oceny: Vec<(SiteId, JobRoleId, CitizenId, u16)> = Vec::new();
    for (id, site) in firms.sites() {
        for p in &site.positions {
            let wagi = m.roles.weights(p.role);
            for e in &p.filled {
                let Some(f) = people.facts(e.citizen) else {
                    continue;
                };
                let praca = effective_labor(
                    &f.vitals,
                    people.skill_in(e.citizen, p.role),
                    site.tech,
                    site.mgmt,
                    &wagi,
                );
                oceny.push((id, p.role, e.citizen, update_perf(e.perf_ema, praca.0)));
                if e.benefits != magnat_firms::BenefitSet::NONE {
                    for (need, punkty) in benefit_gains(e.benefits, ben) {
                        if punkty > 0 {
                            podniesienia.push((e.citizen, need, punkty));
                        }
                    }
                }
            }
        }
    }
    for (c, need, punkty) in podniesienia {
        people.add_need(c, need, punkty);
    }
    for (site, role, c, ocena) in oceny {
        if let Some(s) = firms.site_mut(site) {
            if let Some(p) = s.positions.iter_mut().find(|p| p.role == role) {
                if let Some(e) = p.filled.iter_mut().find(|e| e.citizen == c) {
                    e.perf_ema = ocena;
                }
            }
        }
    }
}

/// Miesiąc: premia, koszt świadczeń i upomnienia.
fn miesiac_kadrowy(
    firms: &mut Firms,
    hr: &magnat_firms::hr::tuning::HrTuning,
    ben: &magnat_firms::hr::tuning::BenefitTuning,
    d: &mut LaborDay,
) {
    // 2. Miesiąc: premia, koszt świadczeń i upomnienia. Upomnienie jest tu jedynym
    //    pisarzem `warnings` — bez niego zwolnienie za wynik nigdy by nie nastąpiło,
    //    bo wymaga **i** słabej oceny, **i** upomnień.
    // Premia i koszt świadczeń idą **osobno**, choć obciążają ten sam zakład:
    // balansator ma odróżnić wzrost premii od wzrostu ceny opieki medycznej,
    // a z jednej liczby tego nie wyczyta.
    let mut koszt: Vec<(SiteId, Money, Money)> = Vec::new();
    for (id, site) in firms.sites() {
        let mut premia = 0i64;
        let mut swiadczenia = 0i64;
        for p in &site.positions {
            for e in &p.filled {
                premia += bonus(e.perf_ema, e.wage_month, hr).get();
                swiadczenia += benefit_cost(e.benefits, e.wage_month, ben).get();
            }
        }
        if premia > 0 || swiadczenia > 0 {
            koszt.push((id, Money(premia), Money(swiadczenia)));
        }
    }
    for (id, premia, swiadczenia) in koszt {
        if let Some(s) = firms.site_mut(id) {
            s.accrue_hr(Money(premia.get().saturating_add(swiadczenia.get())));
        }
        d.bonus = Money(d.bonus.get().saturating_add(premia.get()));
        d.benefits = Money(d.benefits.get().saturating_add(swiadczenia.get()));
    }

    let mut upomnienia: Vec<(SiteId, JobRoleId, CitizenId, u8)> = Vec::new();
    for (id, site) in firms.sites() {
        for p in &site.positions {
            for e in &p.filled {
                let nowe = if e.perf_ema < hr.dismiss_perf {
                    e.warnings.saturating_add(1)
                } else {
                    0
                };
                if nowe != e.warnings {
                    upomnienia.push((id, p.role, e.citizen, nowe));
                }
            }
        }
    }
    for (site, role, c, n) in upomnienia {
        if let Some(s) = firms.site_mut(site) {
            if let Some(p) = s.positions.iter_mut().find(|p| p.role == role) {
                if let Some(e) = p.filled.iter_mut().find(|e| e.citizen == c) {
                    e.warnings = n;
                }
            }
        }
    }
}

/// Termin szkoleniowy: każdy zakład szkoli najsłabszego.
fn termin_szkoleniowy(
    firms: &mut Firms,
    people: &mut impl Workforce,
    hr: &magnat_firms::hr::tuning::HrTuning,
    d: &mut LaborDay,
) {
    // 3. Szkolenie: każdy zakład szkoli **najsłabszego** — tego, którego przyrost
    //    najbardziej podniesie przepustowość. Jeden na zakład, bo szkolenie kosztuje
    //    i trwa; decyzja „ilu i za ile" jest polityką firmy, czyli M7c.
    let mut szkoleni: Vec<(SiteId, JobRoleId, CitizenId, u8, Money)> = Vec::new();
    for (id, site) in firms.sites() {
        let mut naj: Option<(JobRoleId, CitizenId, u8, u8, Money)> = None;
        for p in &site.positions {
            for e in &p.filled {
                let Some(f) = people.facts(e.citizen) else {
                    continue;
                };
                let s = people.skill_in(e.citizen, p.role).get();
                if naj.is_none_or(|n| s < n.2) {
                    naj = Some((p.role, e.citizen, s, f.education(), e.wage_month));
                }
            }
        }
        if let Some((role, c, s, edu, wage)) = naj {
            let po = trained_skill(magnat_core::Q::new(s), edu, hr);
            if po.get() > s {
                szkoleni.push((
                    id,
                    role,
                    c,
                    po.get(),
                    magnat_firms::hr::training::training_cost(wage, hr),
                ));
            }
        }
    }
    for (site, role, c, poziom, cena) in szkoleni {
        people.set_skill(c, role, magnat_core::Q::new(poziom));
        if let Some(s) = firms.site_mut(site) {
            s.accrue_hr(cena);
        }
        d.training_cost = Money(d.training_cost.get().saturating_add(cena.get()));
        d.trained += 1;
    }
}

/// Rozwiązanie **wszystkich** umów zakładu — upadłość pracodawcy (M7d WP9).
///
/// Nie jest to ani rotacja, ani zwolnienie za wynik: firma przestaje istnieć, więc
/// odchodzą wszyscy i wszyscy z tego samego powodu (`LeaveCause::Redundancy`).
/// Wariant istnieje w `core` od M7b **właśnie po to** i do tej chwili nie miał pisarza.
///
/// Idzie przez [`odejdz`], czyli przez tę samą jedyną drogę co każde inne wyjście
/// z etatu — dzięki temu etat wraca do puli wakatów miasta, komponent mieszkańca się
/// czyści, a niezmiennik 4 z M7 §7.2 („każdy `Employment` zakończony dokładnie raz")
/// trzyma się bez osobnej ścieżki, którą trzeba by osobno testować.
///
/// Zwraca listę `(mieszkaniec, odprawa)`. Odprawa jest **naliczona, nie wypłacona** —
/// firma w upadłości z definicji nie ma czym płacić, więc kwota staje się roszczeniem
/// w postępowaniu, a nie przelewem.
pub fn dismiss_all(
    hr: &magnat_firms::HrTuning,
    firms: &mut Firms,
    people: &mut impl Workforce,
    site: SiteId,
    now: SimMinute,
) -> Vec<(CitizenId, Money)> {
    let zaloga: Vec<(CitizenId, JobRoleId, u32, Money, u32)> = match firms.site(site) {
        Some(s) => s
            .positions
            .iter()
            .flat_map(|p| {
                p.filled.iter().map(move |e| {
                    (
                        e.citizen,
                        p.role,
                        staz(e.since, now),
                        e.wage_month,
                        e.household,
                    )
                })
            })
            .collect(),
        None => return Vec::new(),
    };
    let mut out = Vec::with_capacity(zaloga.len());
    for (c, role, staz_dni, stawka, household) in zaloga {
        odejdz(
            firms,
            people,
            Leave {
                citizen: c,
                site,
                role,
                cause: LeaveCause::Redundancy,
                tenure_days: staz_dni,
                household,
            },
            now,
        );
        out.push((c, severance(stawka, staz_dni, hr)));
    }
    out
}
