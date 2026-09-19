//! Cykl wyborczy: kampania, wpłaty, głosowanie, objęcie urzędu (M8e WP10).
//!
//! Osobny plik od [`crate::rule`] i osobny temat: tam miasto **rządzi** (skutki
//! uchwał, sygnały, decyzja, przetargi), tutaj **zmienia władzę**. Jedyne, co
//! łączy oba, to wyzwalacz — `rule::dobowy` woła [`cykl`] raz na dobę, a ten
//! porównuje dwie liczby i najczęściej nic nie robi.
//!
//! Sama arytmetyka wyborów — frekwencja, użyteczność kandydata, podział mandatów —
//! jest w [`crate::election`] i jest funkcją czystą: ten plik wyłącznie zbiera dla
//! niej wejście ze świata i zapisuje wynik do zasobu miasta.

use magnat_core::{DecisionReason, DistrictId, Money, SiteId, Tick, Q};
use magnat_economy::Market;
use magnat_ecs::World;

use crate::city::City;
use crate::election::{self, Backer, Election, Legality, VoterView};
use crate::policy::Policy;

/// Dobowa bramka cyklu wyborczego: ogłoszenie, kampania, głosowanie,
/// objęcie urzędu. Najczęściej nie robi nic i to jest jej normalny stan.
pub fn cykl(
    city: &mut City,
    market: &Market,
    world: &mut World,
    t: Tick,
    powody: &mut Vec<(SiteId, DecisionReason)>,
) {
    let Some(tun) = city.gov.tuning.clone() else {
        return;
    };
    // Kampania trwa `BID_DAYS`… nie: tyle, ile vacatio legis, bo to jedyny okres
    // w tych danych, który znaczy „czas na reakcję miasta".
    let kampania = u64::from(tun.vacatio_legis_days) * 1_440;

    // Kampania trwa, dopóki wybory nie mają wyniku. Wynik **zostaje** w zasobie,
    // bo karta rady go pokazuje — więc pytanie brzmi „czy trwa kampania", a nie
    // „czy są jakieś wybory". Pierwsza wersja pytała o to drugie i przeprowadzała
    // wybory **co dobę** od pierwszego rozstrzygnięcia: zasób był `Some`, termin
    // miniony, więc warunek wejścia był spełniony zawsze.
    let trwa_kampania = city.election.as_ref().is_some_and(|e| e.result.is_none());
    if !trwa_kampania {
        if !city.gov.term_over(Tick(t.0 + kampania)) {
            return;
        }
        let wyborcy = wyborcy(world);
        if wyborcy.is_empty() {
            return;
        }
        let kandydaci =
            election::nominate(&city.gov, &wyborcy, &tun, crate::rule::seed_of(world), t);
        if kandydaci.len() < 2 {
            return;
        }
        let termin = Tick(t.0 + kampania);
        city.election = Some(Election::new(termin, city.gov.term_ticks, kandydaci));
        finansuj_kampanie(city, market, world, t, powody);
        return;
    }

    let termin = city.election.as_ref().map_or(0, |e| e.scheduled_at.0);
    if t.0 < termin {
        return;
    }
    let wyborcy = wyborcy(world);
    let popr = city.gov.approval_bps_by_district.clone();
    let seed = crate::rule::seed_of(world);
    let Some(mut e) = city.election.take() else {
        return;
    };
    let Some(wynik) = election::run_election(&mut e, &wyborcy, &popr, &tun, seed, t) else {
        // Brak głosów — miasto bez dorosłych mieszkańców albo świat tuż po
        // zasiedleniu. Kampania **wraca na miejsce** razem z wpłatami, a próba
        // powtarza się następnej doby; wyrzucenie jej kasowałoby pieniądze,
        // które firmy już wydały.
        city.election = Some(e);
        return;
    };
    let zwyciezca = usize::from(wynik.mayor);
    if let Some(c) = e.candidates.get(zwyciezca) {
        city.gov.mayor = c.citizen;
        city.gov.mayor_pref = c.platform;
        city.gov.goals = crate::gov::Goals::from_preference(&c.platform);
        // Obietnica wyborcza staje się uchwałą **pierwszego miesiąca kadencji**,
        // a nie natychmiast: nowy burmistrz też ma vacatio legis, bo gracz ma
        // mieć czas na reakcję na to, co właśnie wygrało wybory.
        for (kind, delta) in c.tax_stance.clone() {
            let obecna = city.code.rate_of(kind);
            let (min_bp, max_bp) = tun.band(kind);
            let nowa = u32::try_from(
                (i64::from(obecna) + i64::from(delta)).clamp(i64::from(min_bp), i64::from(max_bp)),
            )
            .unwrap_or(obecna);
            if nowa == obecna {
                continue;
            }
            // Trzy karencje obowiązują nowego burmistrza tak samo jak poprzednika
            // (T5). Obietnica, której nie wolno dziś spełnić, **przepada** —
            // nie odkłada się na później, bo wyborca głosował na program, a nie
            // na kolejkę zadań.
            let kierunek: i8 = if nowa > obecna { 1 } else { -1 };
            if !city.gov.may_move_rate(kind, kierunek) {
                continue;
            }
            city.gov.record_rate_change(kind, kierunek);
            city.policies.enact(crate::policy::PolicyRecord {
                policy: Policy::TaxRate { kind, bps: nowa },
                enacted_at: t,
                effective_from: Tick(t.0 + u64::from(tun.vacatio_legis_days) * 1_440),
                sunset: None,
                vote: crate::policy::CouncilVote { for_bp: 10_000 },
                reason: DecisionReason::TaxRateChanged {
                    kind,
                    from_bp: u16::try_from(obecna).unwrap_or(u16::MAX),
                    to_bp: u16::try_from(nowa).unwrap_or(u16::MAX),
                    gap_bp: 0,
                },
            });
        }
    }
    city.gov.council = election::council_from(&wynik, &e.candidates);
    city.gov.term_start = t;
    city.log_reason(
        t,
        DecisionReason::ElectionHeld {
            turnout_bp: u16::try_from(wynik.turnout_bp).unwrap_or(u16::MAX),
            winner_bp: u16::try_from(wynik.winner_bp).unwrap_or(u16::MAX),
            incumbent: wynik.incumbent_won,
        },
    );
    for (_, powod) in e.vote_log.iter().take(8) {
        city.log_reason(t, *powod);
    }
    city.election = Some(e);
}

/// Firmy dokładają się do kampanii. To jest **drugi konsument** `back_candidate`
/// obok gracza (M9) — i bez niego cały kanał medialny byłby martwy, bo kandydat
/// bez pieniędzy ma zasięg zero (`R2`).
///
/// Kto komu daje: zakład o wyniku dodatnim wspiera kandydata, którego program
/// jest najbliżej jego interesu, czyli najdalej od podwyżki CIT-u. Wpłata jest
/// **ułamkiem gotówki**, więc duża firma waży więcej — i to jest cała treść
/// zdania „pieniądz ma wpływ na wybory".
fn finansuj_kampanie(
    city: &mut City,
    market: &Market,
    world: &mut World,
    t: Tick,
    powody: &mut Vec<(SiteId, DecisionReason)>,
) {
    // Kandydat najbardziej prorozwojowy — na niego idą pieniądze firm.
    let Some(idx) = city.election.as_ref().and_then(|e| {
        e.candidates
            .iter()
            .enumerate()
            .max_by_key(|(i, c)| (c.platform.growth_bps, std::cmp::Reverse(*i)))
            .map(|(i, _)| i)
    }) else {
        return;
    };
    let rest = market.rest_of_world();
    // Kto daje i jak. Firma, która **już dziś ukrywa** część obrotu, daje poza
    // rejestrem wpłat — i to nie jest losowanie charakteru, tylko ten sam
    // wskaźnik, na którym stoi kontrola skarbowa. Firma, która deklaruje
    // wszystko, daje jawnie. Dzięki temu `Legality::Illegal` ma w tym świecie
    // pisarza **przed** graczem (M9), a nie dopiero razem z nim (`R2`).
    let prog = city
        .tuning
        .0
        .as_ref()
        .map_or(3_000, |t| u32::from(t.shadow.ceiling_bp) / 2);
    let mut wplaty: Vec<(SiteId, magnat_economy::AccountId, Money, bool)> = Vec::new();
    for site in market.sites() {
        let (aktywa, _) = market.book_assets_of(site);
        if aktywa.get() < 1_000_000 {
            continue;
        }
        let Some(konto) = market.account_of(site) else {
            continue;
        };
        // Jeden procent aktywów księgowych, sufit sto tysięcy złotych.
        let kwota = Money((aktywa.get() / 100).min(10_000_000));
        let nielegalnie = u32::from(market.unreported_bps_of(site)) >= prog;
        wplaty.push((site, konto, kwota, nielegalnie));
        if wplaty.len() >= 32 {
            break;
        }
    }
    let klucze: Vec<(SiteId, magnat_firms::FirmKey)> = world
        .get_resource::<magnat_firms::Firms>()
        .map(|f| f.sites().map(|(id, s)| (id, s.firm)).collect())
        .unwrap_or_default();
    let mut nielegalni: Vec<(SiteId, magnat_core::FirmId)> = Vec::new();
    for (site, konto, kwota, nielegalnie) in wplaty {
        let klucz = klucze
            .iter()
            .find(|(s, _)| *s == site)
            .map_or(magnat_firms::FirmKey(0), |(_, k)| *k);
        let legalnosc = if nielegalnie {
            Legality::Illegal
        } else {
            Legality::Legal
        };
        // **Najpierw przelew, potem wpis.** Odwrotna kolejność zasilała kampanię
        // pieniądzem, który nigdy nie wyszedł z firmy: próg wejścia stoi na
        // aktywach księgowych, a płaci się gotówką, więc nieudany przelew jest
        // normalnym stanem, a nie awarią.
        let powod = DecisionReason::CampaignBacked {
            candidate: u8::try_from(idx).unwrap_or(0),
            amount: kwota,
            illegal: nielegalnie,
        };
        let memo = magnat_economy::TxMemo::new(
            magnat_economy::TxKind::CampaignDonation {
                candidate: u8::try_from(idx).unwrap_or(0),
                illegal: nielegalnie,
            },
            powod,
        );
        let ok = world
            .get_resource_mut::<magnat_economy::Books>()
            .map(|b| b.transfer(konto, rest, kwota, memo, t).is_ok())
            .unwrap_or(false);
        if !ok {
            continue;
        }
        if let Some(e) = city.election.as_mut() {
            e.back_candidate(idx, Backer::Firm(klucz), kwota, legalnosc);
        }
        powody.push((site, powod));
        if nielegalnie {
            nielegalni.push((
                site,
                market.firm_of(site).unwrap_or(magnat_core::FirmId(site.0)),
            ));
        }
    }

    // Ujawnienie. Wpłata poza rejestrem jest sprawą dla prokuratury — a tę
    // prowadzi urząd antymonopolowy, bo to on w tej fazie zajmuje się tym,
    // co firma robi poza rynkiem (§5.7). Sprawa idzie zwykłą drogą: dowody
    // rosną, kara przychodzi na końcu, budżet ją księguje. **Drugiej ścieżki
    // nie ma i nie będzie** — `political/scandal` jako osobne zdarzenie byłoby
    // drugim wejściem do tego samego skutku (`K-11`, `K-13`).
    for (site, firma) in nielegalni {
        if let Some(powod) = city.enforcement.otworz(
            magnat_core::AgencyKind::Antitrust,
            site,
            firma,
            Q::new(20),
            t,
        ) {
            powody.push((site, powod));
            // Afera uderza też w kandydata: pieniądz spoza rejestru przestaje
            // działać w chwili, w której stał się zarzutem.
            if let Some(e) = city.election.as_mut() {
                if let Some(c) = e.candidates.get_mut(idx) {
                    c.funding = Money(c.funding.get() - c.illegal_funding.get());
                    c.illegal_funding = Money::ZERO;
                }
            }
        }
    }
}

/// Wyborcy: dorośli, żywi, po indeksie encji rosnąco.
///
/// Kolejność jest **wymogiem determinizmu** (`run_election` iteruje po tym, co
/// dostanie) i dlatego sortowanie jest tutaj, a nie w wywołującym.
fn wyborcy(world: &World) -> Vec<VoterView> {
    use magnat_agents::{Employment, Identity, Population, Residence, Vitals};

    let doba = i32::try_from(world.tick.0 / 1_440).unwrap_or(0);
    let Some(p) = world.get_resource::<Population>() else {
        return Vec::new();
    };
    let mut out: Vec<VoterView> = Vec::with_capacity(p.citizens().len());
    for e in p.citizens() {
        let Some(id) = world.get::<Identity>(*e) else {
            continue;
        };
        if !id.is_alive() || id.age_years(doba) < 18 {
            continue;
        }
        let (Some(v), Some(res)) = (world.get::<Vitals>(*e), world.get::<Residence>(*e)) else {
            continue;
        };
        out.push(VoterView {
            citizen: e.index(),
            district: DistrictId(res.district),
            age_years: u32::try_from(id.age_years(doba)).unwrap_or(18),
            status: v.status,
            mood: v.mood,
            employed: world
                .get::<Employment>(*e)
                .is_some_and(Employment::is_employed),
        });
    }
    out.sort_unstable_by_key(|v| v.citizen);
    out
}
