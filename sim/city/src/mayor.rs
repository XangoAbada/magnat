//! Decyzja burmistrza: menu działań i trzy karencje, które trzymają je w ryzach
//! (M8e WP9, PRD §10.1).
//!
//! Burmistrz nie jest solverem i nie ma być. Raz w miesiącu patrzy na cztery
//! liczby — saldo budżetu, zadłużenie, poparcie i lukę w usługach — wybiera
//! **jedno** działanie z zamkniętego menu i uchwala je z terminem wejścia w życie.
//! Heurystyka oczekiwanego skutku, nie optymalizacja: miasto ma być przewidywalne
//! dla gracza, a nie optymalne.
//!
//! **Histereza jest mechanizmem, nie ostrożnością.** Bez niej AI oscyluje: stawka
//! w górę, bo miesiąc wyszedł na minusie, w dół, bo następny na plusie, i tak co
//! trzydzieści dób. Trzy zabezpieczenia, każde na inną postać tej samej choroby:
//!
//! 1. **Nacisk musi się utrzymać** `hysteresis_months` miesięcy w tę samą stronę,
//!    zanim cokolwiek się stanie. Jeden zły miesiąc nie jest sygnałem.
//! 2. **Ta sama danina ma karencję** `min_months_between_changes` — miasto, które
//!    właśnie podniosło VAT, nie podnosi go znowu w kwartale.
//! 3. **Zwrot kierunku kosztuje** `min_months_for_reversal` miesięcy. To jest
//!    dosłowna treść testu T5 i dlatego nie jest progiem miękkim: zmiana kierunku
//!    przed upływem tego czasu **nie jest brana pod uwagę w menu**, a nie tylko
//!    nisko punktowana. Obowiązuje też obietnicę wyborczą (`CI-8`).
//!
//! Zero floatów, tak jak w całej fazie: sygnały w punktach bazowych, kwoty
//! w groszach, punktacja menu w `i64`.

use magnat_core::{
    AgencyKind, DecisionReason, DistrictId, Money, OpenHours, PolicyKind, ServiceKind,
    SpendCategory, TaxKind, Tick, UtilityService, SPEND_CATEGORY_COUNT, TAX_KIND_COUNT,
};

use crate::gov::{Axis, GovTuning, Government, Signals};
use crate::policy::{CouncilVote, Policy, PolicyRecord, PolicySet};

/// Jedno działanie z menu wraz z punktacją i osią, po której je zważono.
struct Kandydat {
    policy: Policy,
    score: i64,
    reason: DecisionReason,
}

/// Comiesięczna decyzja burmistrza (`EveryMonth`, krok systemu `city.Tax`).
///
/// Zwraca **co najwyżej jedną** uchwałę. Jedną, a nie wszystkie opłacalne, bo
/// miasto zmieniające pięć rzeczy naraz jest dla gracza nieodróżnialne od miasta
/// losowego — a §1 dokumentu fazy obiecuje drugiego gracza, którego da się czytać.
pub fn decide_month(
    gov: &mut Government,
    policies: &PolicySet,
    rates: &[u32; TAX_KIND_COUNT],
    base_shares: &[u32; SPEND_CATEGORY_COUNT],
    emission_limit: i64,
    sig: Signals,
    t: Tick,
) -> Option<PolicyRecord> {
    // Kopia, a nie pożyczka: decyzja **zmienia** rejestr histerezy w tej samej
    // funkcji, w której czyta progi, a siedem widełek raz na miesiąc nie jest
    // kosztem, o który warto się spierać z pożyczaniem.
    let tun = gov.tuning.clone()?;
    let tun = &tun;
    gov.signals = sig;

    // Nacisk fiskalny musi się utrzymać — jeden zły miesiąc nie jest sygnałem.
    let znak = if sig.fiscal_bp < 0 {
        -1
    } else if sig.fiscal_bp > 0 {
        1
    } else {
        0
    };
    if znak != 0 && gov.fiscal_streak.signum() as i32 == znak {
        gov.fiscal_streak += znak as i16;
    } else {
        gov.fiscal_streak = znak as i16;
    }
    let utrwalony = gov.fiscal_streak.unsigned_abs() >= u16::from(tun.hysteresis_months);

    let mut menu: Vec<Kandydat> = Vec::new();
    zbierz_podatki(gov, tun, rates, sig, utrwalony, &mut menu);
    zbierz_wydatki(gov, tun, policies, base_shares, sig, t, &mut menu);
    zbierz_regulacje(gov, tun, policies, sig, emission_limit, t, &mut menu);

    // Remis rozstrzyga kolejność `PolicyKind`, a nie kolejność zbierania —
    // strumienia „na wybór działania" ta faza świadomie nie ma (`StreamId` 249).
    menu.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(a.policy.kind().as_index().cmp(&b.policy.kind().as_index()))
    });
    let best = menu.into_iter().next()?;
    if best.score < tun.action_threshold {
        return None;
    }

    // Głosowanie rady: mandaty o programie bliskim uchwale głosują za.
    let vote = glosowanie(gov, &best.policy);
    if !vote.passed() {
        return None;
    }
    if let Policy::TaxRate { kind, bps } = best.policy {
        let kierunek: i8 = if bps > rates[kind.as_index()] { 1 } else { -1 };
        gov.record_rate_change(kind, kierunek);
        // Zmiana stawki gasi nacisk: miasto, które właśnie zareagowało, zaczyna
        // liczyć od nowa. Bez tego seria deficytów dałaby serię podwyżek.
        gov.fiscal_streak = 0;
    }
    gov.enacted[best.policy.kind().as_index()] += 1;
    Some(PolicyRecord {
        policy: best.policy,
        enacted_at: t,
        effective_from: Tick(t.0 + u64::from(tun.vacatio_legis_days) * 1_440),
        sunset: None,
        vote,
        reason: best.reason,
    })
}

/// Głosowanie rady: mandat głosuje za, jeśli uchwała jest po jego stronie.
///
/// Program mandatu bliski programowi burmistrza znaczy poparcie — i to jest
/// cała treść koalicji w tej fazie. Rada, w której przewagę ma inny program niż
/// u burmistrza, blokuje jego uchwały, więc podzielony wynik wyborów ma skutek.
fn glosowanie(gov: &Government, p: &Policy) -> CouncilVote {
    if gov.council.is_empty() {
        // Miasto bez rady: burmistrz rządzi sam i głosowanie jest tożsamością.
        return CouncilVote { for_bp: 10_000 };
    }
    let os = os_dzialania(p);
    let mut za = 0u32;
    for (_, pref) in &gov.council {
        // Mandat popiera uchwałę, jeśli jej oś jest u niego ponadprzeciętna.
        if pref.axis(os) >= 2_500 {
            za += 1;
        }
    }
    let bp = u32::try_from(u64::from(za) * 10_000 / gov.council.len() as u64).unwrap_or(0);
    CouncilVote {
        for_bp: u16::try_from(bp).unwrap_or(u16::MAX),
    }
}

/// Oś, po której waży się dane działanie.
#[must_use]
pub fn os_dzialania(p: &Policy) -> Axis {
    match p {
        Policy::TaxRate { .. } => Axis::Growth,
        Policy::MinWage(_) | Policy::TradingHours { .. } => Axis::Social,
        Policy::EmissionLimit { .. } => Axis::Green,
        // Sufit taryfy i obsada skarbówki to dwie postacie tego samego:
        // władza pokazuje, że komuś patrzy na ręce.
        Policy::TariffCap { .. } | Policy::AgencyStaffing { .. } => Axis::Populist,
        // Udział w planie wydatków waży się kierunkiem, na który idzie —
        // park to nie to samo co posterunek, choć uchwała jest tego samego rodzaju.
        Policy::SpendShare { category, .. } => match category {
            SpendCategory::Parks => Axis::Green,
            SpendCategory::CapitalInvestment | SpendCategory::RoadMaintenance => Axis::Growth,
            SpendCategory::Subsidies | SpendCategory::TransitSubsidy => Axis::Populist,
            _ => Axis::Social,
        },
    }
}

/// Menu podatkowe: podwyżka przy utrwalonym deficycie, obniżka przy nadwyżce.
fn zbierz_podatki(
    gov: &Government,
    tun: &GovTuning,
    rates: &[u32; TAX_KIND_COUNT],
    sig: Signals,
    utrwalony: bool,
    menu: &mut Vec<Kandydat>,
) {
    if !utrwalony || sig.fiscal_bp == 0 {
        return;
    }
    let w_gore = sig.fiscal_bp < 0;
    for k in TaxKind::ALL {
        let i = k.as_index();
        // Cło i koncesje nie są narzędziem salda: pierwsze jest polityką handlową,
        // drugie opłatą za wpis, a ruszanie ich po to, żeby domknąć miesiąc,
        // byłoby narzędziem bez związku z problemem.
        if matches!(k, TaxKind::Duty | TaxKind::License) {
            continue;
        }
        let (min_bp, max_bp) = tun.band(*k);
        let obecna = rates[i];
        let nowa = if w_gore {
            obecna.saturating_add(tun.tax_step_bp).min(max_bp)
        } else {
            obecna.saturating_sub(tun.tax_step_bp).max(min_bp)
        };
        if nowa == obecna {
            continue;
        }
        let kierunek: i8 = if w_gore { 1 } else { -1 };
        if !wolno_ruszyc(gov, tun, i, kierunek) {
            continue;
        }
        // Punktacja: siła nacisku × waga salda, z karą za odległość od środka
        // widełek — miasto na suficie stawki podnosi ją niechętnie.
        let sila = i64::from(sig.fiscal_bp.unsigned_abs().min(10_000));
        let srodek = i64::from(min_bp + max_bp) / 2;
        let odleglosc = (i64::from(nowa) - srodek).abs() * 10_000 / i64::from(max_bp.max(1));
        let score = sila * i64::from(gov.goals.balance_w) / 10_000 - odleglosc;
        // Podwyżka przy wysokim zadłużeniu jest pilniejsza, obniżka — mniej możliwa.
        let score = if w_gore {
            score + i64::from(sig.debt_bp) / 4
        } else {
            score - i64::from(sig.debt_bp) / 2
        };
        menu.push(Kandydat {
            policy: Policy::TaxRate {
                kind: *k,
                bps: nowa,
            },
            score,
            reason: DecisionReason::TaxRateChanged {
                kind: *k,
                from_bp: u16::try_from(obecna).unwrap_or(u16::MAX),
                to_bp: u16::try_from(nowa).unwrap_or(u16::MAX),
                gap_bp: i16::try_from(sig.fiscal_bp.clamp(-30_000, 30_000) / 10).unwrap_or(0),
            },
        });
    }
}

/// Trzy karencje z nagłówka modułu. Zwrot kierunku **odpada z menu**, a nie
/// dostaje niską punktację — inaczej dość silny sygnał przepchnąłby go mimo to
/// i test T5 pękałby raz na kilkanaście lat gry, czyli najgorzej, jak się da.
pub(crate) fn wolno_ruszyc(gov: &Government, tun: &GovTuning, i: usize, kierunek: i8) -> bool {
    let ostatni = gov.tax_last_month[i];
    if ostatni == u32::MAX {
        return true;
    }
    let minelo = gov.month.saturating_sub(ostatni);
    if minelo < u32::from(tun.min_months_between_changes) {
        return false;
    }
    if gov.tax_last_dir[i] != 0
        && gov.tax_last_dir[i] != kierunek
        && minelo < u32::from(tun.min_months_for_reversal)
    {
        return false;
    }
    true
}

/// Menu wydatkowe: udział planu na usługi, dotacje, obsada urzędów.
///
/// **Wszystkie trzy ruszają udziałem w planie wydatków, a nie kwotą obok niego.**
/// Udział czyta i przelew, i jakość placówki (`PolicySet::shares`), więc uchwała
/// podnosząca jakość szkoły zawsze kosztuje tyle, ile podnosi — a to jest jedyna
/// postać, w której „dofinansowanie oświaty" znaczy w tej grze cokolwiek.
fn zbierz_wydatki(
    gov: &Government,
    tun: &GovTuning,
    policies: &PolicySet,
    base_shares: &[u32; SPEND_CATEGORY_COUNT],
    sig: Signals,
    t: Tick,
    menu: &mut Vec<Kandydat>,
) {
    let zadluzone = sig.debt_bp >= tun.debt_alarm_bp;
    let biezace = policies.shares(*base_shares, t);
    // Krok udziału: jedna dziesiąta obecnego, nie mniej niż 100 bp. Procentowo,
    // bo kierunki wydatku różnią się o rząd wielkości i stały krok albo nie
    // ruszyłby oświaty, albo podwoiłby parki.
    let krok = |bp: u32| (bp / 10).max(100);

    // Luka w usługach: podnosi się udział tego kierunku, który obsługuje rodzaj
    // usługi o najgorszym pokryciu.
    if sig.service_gap_bp > 2_000 && sig.fiscal_bp >= 0 && !zadluzone {
        let kind =
            ServiceKind::from_index(usize::from(sig.worst_service)).unwrap_or(ServiceKind::School);
        let kat = crate::services::PublicService::spend_category(kind);
        let i = kat.as_index();
        menu.push(Kandydat {
            policy: Policy::SpendShare {
                category: kat,
                bp: biezace[i] + krok(biezace[i]),
            },
            score: i64::from(sig.service_gap_bp) * i64::from(gov.goals.approval_w) / 10_000
                + i64::from(gov.mayor_pref.social_bps) / 4,
            reason: DecisionReason::PolicyEnacted {
                kind: PolicyKind::SpendShare,
                for_bp: 0,
                delay_days: tun.vacatio_legis_days,
            },
        });
    }
    // Dotacje przy wysokim bezrobociu. To jest ten sam mechanizm i ta sama
    // uchwała — zmienia się wyłącznie kierunek wydatku.
    if sig.unemployment_permille > 80 && sig.fiscal_bp >= 0 && !zadluzone {
        let i = SpendCategory::Subsidies.as_index();
        menu.push(Kandydat {
            policy: Policy::SpendShare {
                category: SpendCategory::Subsidies,
                bp: biezace[i] + krok(biezace[i]),
            },
            score: i64::from(sig.unemployment_permille) * i64::from(gov.goals.growth_w) / 1_000
                + i64::from(gov.mayor_pref.populist_bps) / 4,
            reason: DecisionReason::PolicyEnacted {
                kind: PolicyKind::SpendShare,
                for_bp: 0,
                delay_days: tun.vacatio_legis_days,
            },
        });
    }
    // Deficyt utrwalony: tnie się ten kierunek, który burmistrz ceni najmniej.
    // Cięcie jest **uchwałą jak każda inna**, więc widać je w karcie rady obok
    // podwyżek — a nie dzieje się samo w `domknij_deficyt`.
    if sig.fiscal_bp < -i32::try_from(tun.deficit_cut_bp).unwrap_or(500) {
        let kat = if gov.mayor_pref.green_bps < 2_500 {
            SpendCategory::Parks
        } else {
            SpendCategory::TransitSubsidy
        };
        let i = kat.as_index();
        if biezace[i] > 200 {
            menu.push(Kandydat {
                policy: Policy::SpendShare {
                    category: kat,
                    bp: biezace[i].saturating_sub(krok(biezace[i])),
                },
                score: i64::from(sig.fiscal_bp.unsigned_abs()) * i64::from(gov.goals.balance_w)
                    / 10_000,
                reason: DecisionReason::PolicyEnacted {
                    kind: PolicyKind::SpendShare,
                    for_bp: 0,
                    delay_days: tun.vacatio_legis_days,
                },
            });
        }
    }
    // Obsada urzędu kontrolnego rośnie, gdy szara strefa rośnie (`CH-6`).
    if sig.shadow_bp > 2_000 {
        let obecna = match policies.current(
            PolicyKind::AgencyStaffing,
            AgencyKind::TaxOffice.as_index() as u32,
            t,
        ) {
            Some(Policy::AgencyStaffing { inspectors, .. }) => *inspectors,
            _ => 0,
        };
        if obecna < 40 {
            menu.push(Kandydat {
                policy: Policy::AgencyStaffing {
                    agency: AgencyKind::TaxOffice,
                    inspectors: (obecna + 2).max(4),
                },
                score: i64::from(sig.shadow_bp) / 2 + i64::from(gov.goals.balance_w) / 10,
                reason: DecisionReason::PolicyEnacted {
                    kind: PolicyKind::AgencyStaffing,
                    for_bp: 0,
                    delay_days: tun.vacatio_legis_days,
                },
            });
        }
    }
}

/// Menu regulacyjne: emisje, taryfy, godziny handlu, płaca minimalna.
fn zbierz_regulacje(
    gov: &Government,
    tun: &GovTuning,
    policies: &PolicySet,
    sig: Signals,
    emission_limit: i64,
    t: Tick,
    menu: &mut Vec<Kandydat>,
) {
    // Limit emisji: zaostrza się, gdy najbrudniejszy zakład mieści się w nim
    // z zapasem — czyli gdy limit przestał cokolwiek znaczyć. Nowy limit liczy
    // się **z obowiązującego**, nie ze stałej: stała byłaby drugim źródłem
    // liczby, którą miasto już zna, i w innych jednostkach niż pierwsze.
    if sig.emission_bp < 5_000 && gov.mayor_pref.green_bps > 2_000 && emission_limit > 4 {
        menu.push(Kandydat {
            policy: Policy::EmissionLimit {
                max_g_per_min: emission_limit * 4 / 5,
            },
            score: i64::from(gov.mayor_pref.green_bps) / 2,
            reason: DecisionReason::PolicyEnacted {
                kind: PolicyKind::EmissionLimit,
                for_bp: 0,
                delay_days: tun.vacatio_legis_days,
            },
        });
    }
    // Sufit taryfy: populista sięga po niego, gdy poparcie siada.
    if gov.mayor_pref.populist_bps > 3_000
        && policies
            .current(
                PolicyKind::TariffCap,
                UtilityService::Electricity.as_index() as u32,
                t,
            )
            .is_none()
        && gov.approval_mean_bp() < 4_500
    {
        menu.push(Kandydat {
            policy: Policy::TariffCap {
                service: UtilityService::Electricity,
                max_per_unit: Money(60),
            },
            score: i64::from(10_000 - gov.approval_mean_bp())
                * i64::from(gov.mayor_pref.populist_bps)
                / 10_000,
            reason: DecisionReason::PolicyEnacted {
                kind: PolicyKind::TariffCap,
                for_bp: 0,
                delay_days: tun.vacatio_legis_days,
            },
        });
    }
    // Płaca minimalna: socjalny burmistrz sięga po nią wcześniej niż inni.
    if gov.mayor_pref.social_bps > 3_000 && policies.current(PolicyKind::MinWage, 0, t).is_none() {
        menu.push(Kandydat {
            policy: Policy::MinWage(Money(180_000)),
            score: i64::from(gov.mayor_pref.social_bps) / 2 + i64::from(gov.goals.approval_w) / 4,
            reason: DecisionReason::PolicyEnacted {
                kind: PolicyKind::MinWage,
                for_bp: 0,
                delay_days: tun.vacatio_legis_days,
            },
        });
    }
    // Wolna niedziela w dzielnicy o najniższym poparciu. Socjalny i ekologiczny
    // burmistrz widzą w tym to samo: dzień, w którym miasto nie pracuje.
    if gov.mayor_pref.social_bps + gov.mayor_pref.green_bps > 5_500 {
        if let Some((d, _)) = gov
            .approval_bps_by_district
            .iter()
            .enumerate()
            .min_by_key(|(i, v)| (**v, *i))
        {
            let dzielnica = DistrictId(u16::try_from(d).unwrap_or(0));
            if policies
                .current(PolicyKind::TradingHours, u32::from(dzielnica.0), t)
                .is_none()
            {
                menu.push(Kandydat {
                    // Od szóstej do dwudziestej drugiej, od poniedziałku do soboty.
                    policy: Policy::TradingHours {
                        district: dzielnica,
                        hours: OpenHours::new(360, 1_320, 0b011_1111),
                    },
                    score: i64::from(gov.mayor_pref.social_bps) / 3,
                    reason: DecisionReason::PolicyEnacted {
                        kind: PolicyKind::TradingHours,
                        for_bp: 0,
                        delay_days: tun.vacatio_legis_days,
                    },
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gov::Preference;

    fn tuning() -> GovTuning {
        GovTuning::load_default().expect("data/city/government.ron")
    }

    /// Test T5 na samej histerezie, bez świata: dwadzieścia lat naprzemiennych
    /// sygnałów nie może dać ani jednego zwrotu kierunku częściej niż raz na 24
    /// miesiące. Sygnał zmienia znak **co miesiąc** — to jest najgorszy możliwy
    /// przypadek, a nie typowy.
    #[test]
    fn stawka_nie_zmienia_kierunku_czesciej_niz_raz_na_dwa_lata() {
        let tun = tuning();
        let mut gov = Government::new(Preference::default(), 4, Some(tun));
        let policies = PolicySet::new();
        let mut rates = [1_900u32; TAX_KIND_COUNT];
        let mut zmiany: Vec<(u32, usize, i8)> = Vec::new();
        for m in 0..240u32 {
            gov.month = m;
            let sig = Signals {
                // Naprzemiennie deficyt i nadwyżka, obie utrwalone przez trzy miesiące.
                fiscal_bp: if (m / 3) % 2 == 0 { -4_000 } else { 4_000 },
                debt_bp: 5_000,
                approval_bp: 5_000,
                service_gap_bp: 0,
                worst_service: 0,
                unemployment_permille: 50,
                emission_bp: 9_000,
                shadow_bp: 0,
            };
            if let Some(r) = decide_month(
                &mut gov,
                &policies,
                &rates,
                &[1_000; SPEND_CATEGORY_COUNT],
                48,
                sig,
                Tick(u64::from(m) * 43_200),
            ) {
                if let Policy::TaxRate { kind, bps } = r.policy {
                    let i = kind.as_index();
                    let dir: i8 = if bps > rates[i] { 1 } else { -1 };
                    zmiany.push((m, i, dir));
                    rates[i] = bps;
                }
            }
        }
        assert!(!zmiany.is_empty(), "burmistrz nie ruszył ani jednej stawki");
        for k in 0..TAX_KIND_COUNT {
            let swoje: Vec<(u32, i8)> = zmiany
                .iter()
                .filter(|(_, i, _)| *i == k)
                .map(|(m, _, d)| (*m, *d))
                .collect();
            for w in swoje.windows(2) {
                if w[0].1 != w[1].1 {
                    assert!(
                        w[1].0 - w[0].0 >= 24,
                        "danina {k}: zwrot kierunku po {} miesiącach",
                        w[1].0 - w[0].0
                    );
                }
            }
        }
    }
}
