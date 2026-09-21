//! Publikacja wakatów, zgłoszenia kandydatów i rozstrzygnięcie (M7b WP4, M7 §5.5).
//!
//! Trzy kroki doby, w tej kolejności i nie w innej: wakat bez oferty dostaje ofertę,
//! szukający pracy oglądają **kilka** ofert (PRD §17.5, nie wszystkie), a firma wybiera
//! po wyniku — z zapisanym wynikiem drugiego w kolejce, bo to on jest odpowiedzią
//! na pytanie „dlaczego on".

use magnat_core::{
    rng::rng, DecisionReason, FirmReason, JobRoleId, Money, SimMinute, StreamId, Tick, Q,
};
use magnat_firms::{
    hr::employment::Employment, meets_requirements, score_application, switch_threshold_bp,
    CandidateFacts, Firms, HiringPolicy, OpeningFacts,
};
use std::collections::{BTreeMap, BTreeSet};

use super::offer::{JobOffer, JobOfferId, SkillReq};
use super::{LaborDay, LaborMarket, PersonFacts, Seeker, Workforce};

/// Wymagana umiejętność wyprowadzona z **wag produktywności zawodu**.
///
/// Katalog `data/jobs/roles.ron` nie ma pola „wymagana umiejętność" i nie dostaje go
/// tutaj: rola, w której umiejętność waży 700 z 1000, jest z definicji trudniejsza
/// od takiej, w której waży 250, a druga liczba o tym samym znaczeniu rozjechałaby się
/// z pierwszą przy pierwszej zmianie danych (ten sam argument co `K-43`).
//
// ponytail: dzielnik 40 daje próg 6..25 punktów przy wagach z katalogu — na tyle
// nisko, żeby absolwent gdziekolwiek wszedł. Właściwy profil kompetencji stanowiska
// jest tabelą w `data/`, której właścicielem jest M10 razem z R&D; sufit nazwany.
fn requirements(m: &LaborMarket, role: JobRoleId, managerial: bool) -> SkillReq {
    let w = m.roles.weights(role);
    let baza = (w.skill / 40) as u8;
    SkillReq {
        min_skill: Q::new(if managerial { baza + 20 } else { baza }),
        min_edu: u8::from(managerial) * 2,
    }
}

/// Wagi wyboru kandydata (M7c, `AV-6`).
///
/// Do M7c wagi były stałą `HiringPolicy::NEUTRAL` w całym mieście, więc nadzorca
/// i handlowiec zatrudniali tak samo. Od M7c wychodzą ze stylu menedżera, a **od M7e
/// — z osobowości firmy**, gdy menedżera nie ma. Ta sama kolejność co przy agresji
/// licytacyjnej i z tego samego powodu: zakładem kieruje ten, kto go prowadzi.
fn wagi_wyboru(firms: &Firms, site: magnat_core::SiteId) -> HiringPolicy {
    if let Some(s) = firms.style_of(site) {
        return magnat_firms::ManagerStyle::hiring(s);
    }
    firms
        .site(site)
        .and_then(|s| firms.get(s.firm))
        .map_or(HiringPolicy::NEUTRAL, |f| f.personality.hiring())
}

/// Wystawia ofertę na każdy wakat, który jeszcze jej nie ma (WP4).
///
/// Stawka startowa to **dolny kraniec widełek**, a nie mediana rynku: firma licytuje
/// w górę dopiero wtedy, gdy nikt nie przychodzi. To jest cała degresja przy nadmiarze
/// podaży pracy (§5.5 pkt 3) — nowe oferty stoją poniżej mediany i znajdują chętnych,
/// bo płaca progowa bezrobotnego też z czasem schodzi.
pub(super) fn post_offers(m: &mut LaborMarket, firms: &Firms, now: SimMinute, d: &mut LaborDay) {
    m.index.rebuild(&m.offers);
    let waznosc = u64::from(m.tuning.search.offer_days_valid) * 1440;
    let mut nowe: Vec<JobOffer> = Vec::new();
    // Liczba etatów w wiszącej ofercie musi iść za wakatami, a nie zastygać na
    // stanie z dnia publikacji: zakład, z którego odeszła połowa załogi, miałby
    // inaczej ogłoszenie na jeden etat i nie miałby jak ogłosić reszty.
    let mut przeliczenia: Vec<(JobOfferId, u16)> = Vec::new();
    for (id, site) in firms.sites() {
        for p in &site.positions {
            let wolne = p.vacancies();
            if let Some(oferta) = m.index.at_position(id, p.role) {
                przeliczenia.push((oferta, wolne));
                continue;
            }
            if wolne == 0 {
                continue;
            }
            // Grafik **następnego** wolnego etatu — ogłoszenie ma mówić, na którą
            // zmianę się szuka (`R2-WP37`). Wiążący jest ten liczony przy obsadzeniu,
            // bo jedna oferta wisi na wszystkie wolne etaty stanowiska naraz.
            let (zmiana, dni) =
                magnat_agents::ShiftKind::schedule(site.shift_profile, p.filled.len() as u32);
            nowe.push(JobOffer {
                firm: site.firm,
                site: id,
                role: p.role,
                district: site.district,
                wage_month: p.wage_band.0,
                slots: wolne,
                shift: zmiana,
                work_days: dni,
                requirements: requirements(m, p.role, p.managerial),
                benefits: magnat_firms::BenefitSet::NONE,
                targeted: None,
                posted: now,
                expires: SimMinute(now.0 + waznosc),
                days_open: 0,
                raises: 0,
                frozen: false,
                applicants: 0,
                band: p.wage_band,
            });
        }
    }
    let mut wygaszone = Vec::new();
    for (id, wolne) in przeliczenia {
        let Some(o) = m.offers.get_mut(id) else {
            continue;
        };
        o.slots = wolne;
        if wolne == 0 {
            wygaszone.push(id);
        }
    }
    for id in wygaszone {
        m.offers.remove(id);
    }
    d.posted += nowe.len() as u32;
    for o in nowe {
        m.post_offer(o);
    }
    m.index.rebuild(&m.offers);
}

/// Zbiera zgłoszenia kandydatów (WP4).
///
/// Kandydat ogląda **3–15 ofert**, nie wszystkie (PRD §17.5), a to, które obejrzy,
/// rozstrzyga strumień `LaborSearch` — bez losowania przeglądanie szłoby zawsze
/// po kolejności areny i ten sam zakład byłby oglądany pierwszy przez całe miasto
/// przez całą grę.
pub(super) fn collect_applications(
    m: &mut LaborMarket,
    firms: &Firms,
    people: &mut impl Workforce,
    seed: u64,
    now: SimMinute,
    d: &mut LaborDay,
) {
    m.apps.clear();
    let mut szukajacy = Vec::new();
    people.job_seekers(m.day, m.tuning.search.on_the_job_every_days, &mut szukajacy);
    let mut dzis: BTreeMap<(JobRoleId, magnat_core::DistrictId), u32> = BTreeMap::new();
    let mut bufor: Vec<JobOfferId> = Vec::new();
    for c in szukajacy {
        let Some(f) = people.facts(c) else { continue };
        // Bezrobotny wchodzi do ewidencji w pierwszej dobie bez pracy — od niej liczy
        // się czas bezrobocia, czyli tempo, w jakim schodzi jego płaca progowa.
        if !f.employed() {
            m.seekers.entry(c).or_insert(Seeker {
                since_day: m.day,
                last_wage: Money::ZERO,
            });
        }
        zbierz_oferty(m, c, &f, &mut bufor);
        if bufor.is_empty() {
            continue;
        }
        let mut r = rng(seed, StreamId::LaborSearch, c.0.index(), Tick(now.0));
        let widelki = u32::from(m.tuning.search.offers_max - m.tuning.search.offers_min + 1);
        let ile = (u32::from(m.tuning.search.offers_min) + r.gen_range_u32(widelki)) as usize;
        if bufor.len() > ile {
            r.shuffle(&mut bufor);
            bufor.truncate(ile);
            // Po skróceniu wracamy do porządku uchwytów: kolejność zgłoszeń wchodzi
            // do hasha, a tasowanie miało wybrać podzbiór, nie ustawić kolejkę.
            bufor.sort_unstable();
        }
        let obecna = f
            .job
            .and_then(|(s, role)| stawka(firms, s, role, c))
            .unwrap_or(Money::ZERO);
        for id in &bufor {
            let Some(o) = m.offers.get(*id).copied() else {
                continue;
            };
            let umiejetnosc = people.skill_in(c, o.role);
            let oczekiwanie = oczekiwanie_kandydata(m, c, &f, &o, obecna);
            let kandydat = CandidateFacts {
                skill: umiejetnosc,
                education: f.education(),
                wage_expectation: oczekiwanie,
            };
            let wakat = OpeningFacts {
                wage_month: o.wage_month,
                min_skill: o.requirements.min_skill,
                min_edu: o.requirements.min_edu,
            };
            // Do własnego stanowiska się nie aplikuje — patrz warunek w `hire`.
            if !meets_requirements(&kandydat, &wakat) || f.job == Some((o.site, o.role)) {
                continue;
            }
            if m.apply_for(*id, c, oczekiwanie, umiejetnosc, f.education(), now) {
                d.applications += 1;
                *dzis.entry((o.role, o.district)).or_default() += 1;
            }
        }
    }
    // Suma malejąca aplikacji — wejście indeksu niedoboru. Idzie po **wszystkich**
    // kluczach, bo zawód, w którym dziś nikt nie aplikował, też ma o dobę zapomnieć.
    let klucze: Vec<_> = m.stats.per_role.keys().copied().collect();
    for k in klucze {
        let dzisiaj = dzis.get(&k).copied().unwrap_or(0);
        m.stats.entry(k.0, k.1).tick_applicants(dzisiaj);
    }
    for (k, n) in dzis {
        if !m.stats.per_role.contains_key(&k) {
            m.stats.entry(k.0, k.1).tick_applicants(n);
        }
    }
}

/// Stawka obecnej umowy. Jedynym źródłem prawdy o płacy jest rekord po stronie firmy —
/// komponent mieszkańca nie niesie ani grosza (`D1`), więc pytamy zakład, a nie człowieka.
fn stawka(
    firms: &Firms,
    site: magnat_core::SiteId,
    role: JobRoleId,
    c: magnat_core::CitizenId,
) -> Option<Money> {
    firms
        .site(site)?
        .positions
        .iter()
        .find(|p| p.role == role)?
        .filled
        .iter()
        .find(|e| e.citizen == c)
        .map(|e| e.wage_month)
}

/// Oferty, które ten kandydat w ogóle zobaczy: najpierw swój zawód w swojej dzielnicy,
/// a absolwent bez ani jednej umiejętności — cokolwiek w dzielnicy.
fn zbierz_oferty(
    m: &LaborMarket,
    c: magnat_core::CitizenId,
    f: &PersonFacts,
    out: &mut Vec<JobOfferId>,
) {
    out.clear();
    let zawod = f.best_role.or(f.job.map(|(_, r)| r));
    match zawod {
        Some(role) => out.extend_from_slice(m.index.in_role(role, f.district)),
        None => out.extend_from_slice(m.index.in_district(f.district)),
    }
    // Oferty bezpośrednie widzi wyłącznie ten, do kogo zostały wysłane — i widzi je
    // **zawsze**, nawet jeśli nie szukałby pracy w swoim zawodzie ani w swojej dzielnicy.
    // Na tym polega przeciąganie pracownika.
    out.extend_from_slice(m.index.for_target(c));
}

/// Płaca progowa wobec konkretnej oferty.
///
/// Bezrobotny wnosi swoje oczekiwanie z [`LaborMarket::reservation_wage`]. Zatrudniony
/// wnosi **obecną stawkę powiększoną o próg zmiany pracy**: ambicja go obniża, lojalność
/// podwyższa (§5.5 pkt 2). Bez tego progu rotacja byłaby młynkiem (`R11`).
fn oczekiwanie_kandydata(
    m: &LaborMarket,
    c: magnat_core::CitizenId,
    f: &PersonFacts,
    o: &JobOffer,
    obecna: Money,
) -> Money {
    if f.employed() {
        let prog = switch_threshold_bp(f.ambition, f.loyalty, &m.tuning.wage);
        let baza = if obecna.get() > 0 {
            obecna
        } else {
            m.stats
                .median_accepted(o.role, o.district)
                .unwrap_or(Money::ZERO)
        };
        return Money(baza.get().saturating_mul(i64::from(10_000 + prog)) / 10_000);
    }
    let s = m.seekers.get(&c).copied().unwrap_or(Seeker {
        since_day: m.day,
        last_wage: Money::ZERO,
    });
    m.reservation_wage(&s, o.role, o.district)
}

/// Rozstrzygnięcie: firma wybiera kandydata i zapisuje, o ile lepszy był od drugiego
/// (WP4, kryterium ukończenia).
pub(super) fn hire(
    m: &mut LaborMarket,
    firms: &mut Firms,
    people: &mut impl Workforce,
    now: SimMinute,
    d: &mut LaborDay,
) {
    // Zgłoszenia po ofercie, oferty po uchwycie — kolejność rozstrzygania nie może
    // zależeć od tego, w jakiej kolejności ludzie aplikowali.
    let mut wg_oferty: BTreeMap<JobOfferId, Vec<usize>> = BTreeMap::new();
    for (i, a) in m.apps.iter().enumerate() {
        wg_oferty.entry(a.offer).or_default().push(i);
    }
    let mut zatrudnieni_dzis: BTreeSet<magnat_core::CitizenId> = BTreeSet::new();
    let mut do_usuniecia: Vec<JobOfferId> = Vec::new();

    for (id, idx) in wg_oferty {
        let Some(o) = m.offers.get(id).copied() else {
            continue;
        };
        let wakat = OpeningFacts {
            wage_month: o.wage_month,
            min_skill: o.requirements.min_skill,
            min_edu: o.requirements.min_edu,
        };
        let wagi = wagi_wyboru(firms, o.site);
        let mut ranking: Vec<(i32, u32, usize)> = idx
            .iter()
            .filter(|i| !zatrudnieni_dzis.contains(&m.apps[**i].citizen))
            .map(|i| {
                let a = &m.apps[*i];
                let kandydat = CandidateFacts {
                    skill: a.skill,
                    education: a.education,
                    wage_expectation: a.wage_expectation,
                };
                (
                    score_application(&kandydat, &wakat, &wagi),
                    a.citizen.0.index(),
                    *i,
                )
            })
            .collect();
        // Malejąco po wyniku, remis po indeksie encji — nigdy po kolejności zgłoszeń.
        ranking.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        if ranking.is_empty() {
            continue;
        }

        // Obsadzamy **do wyczerpania wakatu**, a nie do wyczerpania planu: między
        // publikacją oferty a dzisiejszym rozstrzygnięciem stanowisko mogło się
        // zapełnić z drugiej oferty (publiczna i bezpośrednia dotyczą tego samego
        // etatu), a kandydat mógł zniknąć z miasta. Liczymy więc **zatrudnionych**,
        // a nie zaplanowanych — inaczej etat znika z oferty bez zatrudnienia,
        // a zakład potrafi wypłacać dwie pensje za jedno stanowisko.
        let mut obsadzeni: u16 = 0;
        for miejsce in 0..ranking.len() {
            if obsadzeni >= o.slots {
                break;
            }
            // Wakat sprawdzamy **przed** każdym zatrudnieniem i przed wyjściem
            // kandydata z poprzedniej pracy: obsada ponad liczbę etatów jest błędem
            // księgowym, którego nic później nie naprawia.
            let wolne = firms
                .site(o.site)
                .and_then(|s| s.positions.iter().find(|p| p.role == o.role))
                .is_some_and(|p| p.vacancies() > 0);
            if !wolne {
                break;
            }
            let (wynik, _, i) = ranking[miejsce];
            let drugi = ranking.get(miejsce + 1).map_or(i32::MIN, |r| r.0);
            let c = m.apps[i].citizen;
            let Some(f) = people.facts(c) else { continue };
            // Własnego pracownika na jego własne stanowisko się nie „zatrudnia".
            // Bez tego warunku eskalacja stawki w ofercie przebijałaby jego próg
            // zmiany pracy, a wynikiem byłoby odejście i zatrudnienie w tej samej
            // dobie, w tym samym zakładzie: obsada bez zmian, wakat zużyty,
            // a mediana zawartych umów zanieczyszczona zatrudnieniem widmem.
            if f.job == Some((o.site, o.role)) {
                continue;
            }
            // Zmiana pracy: najpierw wyjście ze starego etatu, i tylko tą drogą.
            if let Some((stary_site, stara_rola)) = f.job {
                let umowa = firms
                    .site(stary_site)
                    .and_then(|s| s.positions.iter().find(|p| p.role == stara_rola))
                    .and_then(|p| p.filled.iter().find(|e| e.citizen == c));
                let staz = umowa.map_or(0, |e| (now.0.saturating_sub(e.since.0) / 1440) as u32);
                let household = umowa.map_or(Employment::NO_HOUSEHOLD, |e| e.household);
                super::hr::odejdz(
                    firms,
                    people,
                    super::hr::Leave {
                        citizen: c,
                        site: stary_site,
                        role: stara_rola,
                        cause: magnat_core::LeaveCause::BetterOffer,
                        tenure_days: staz,
                        household,
                    },
                    now,
                );
                d.quits += 1;
            }
            let Some(site) = firms.site_mut(o.site) else {
                continue;
            };
            let Some(p) = site.positions.iter_mut().find(|p| p.role == o.role) else {
                continue;
            };
            // Grafik liczy się **przy obsadzeniu**, a nie przy ogłoszeniu (`R2-WP37`).
            // Oferta wisi na wszystkie wolne etaty stanowiska naraz (`slots`), więc
            // grafik wzięty z niej dałby całej ósemce tę samą brygadę — huta miałaby
            // ośmiu spawaczy na porannej i nikogo w nocy. Numerem brygady jest liczba
            // już obsadzonych etatów, więc kolejni wchodzą kolejno.
            let (zmiana, dni) =
                magnat_agents::ShiftKind::schedule(site.shift_profile, p.filled.len() as u32);
            // Gospodarstwo zapisuje się w umowie (`R2-WP9`): w chwili odejścia
            // encji mieszkańca może już nie być.
            let gospodarstwo = people.household_of(c);
            p.filled.push(Employment::new(
                c,
                o.role,
                o.wage_month,
                now,
                zmiana,
                gospodarstwo,
            ));
            people.hire(c, o.site, o.role, zmiana, dni, o.wage_month);
            m.seekers.remove(&c);
            zatrudnieni_dzis.insert(c);
            obsadzeni += 1;
            m.stats
                .entry(o.role, o.district)
                .record_hire(o.wage_month, o.days_open);
            firms.log(
                o.firm,
                Tick(now.0),
                DecisionReason::Firm(FirmReason::Hired {
                    role: o.role,
                    score: wynik,
                    runner_up: drugi,
                }),
            );
            d.hires += 1;
        }
        if let Some(o) = m.offers.get_mut(id) {
            o.slots = o.slots.saturating_sub(obsadzeni);
            if obsadzeni > 0 {
                o.days_open = 0;
                o.applicants = 0;
            }
            if o.slots == 0 {
                do_usuniecia.push(id);
            }
        }
    }
    for id in do_usuniecia {
        m.offers.remove(id);
        m.index.mark_dirty();
    }
    m.apps.clear();
}
