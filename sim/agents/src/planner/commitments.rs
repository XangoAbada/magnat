use super::canvas::wstaw;
use super::*;

/// Przekazanie dziecka w szkole.
const ESCORT_HANDOVER_MIN: u16 = 10;
/// Okno szkolne. `ponytail:` stała w kodzie, nie dane — właścicielem szkoły jako
/// instytucji z pojemnością i grafikiem jest M8 (decyzja 9.15), a epoki wchodzą
/// z `data/epochs/` w M3d. Do tego czasu jedno okno dla wszystkich klas.
const SCHOOL_OPEN: u16 = 8 * 60;
const SCHOOL_CLOSE: u16 = 14 * 60;

// ── faza 1: zobowiązania stałe ──────────────────────────────────────────────────

pub(super) fn faza1_zobowiazania(
    ctx: &PlanCtx<'_>,
    canvas: &mut DayCanvas,
    log: &mut Log<'_>,
    stats: &mut PlanStats,
) {
    let pracuje = ctx.employment.works_on(ctx.dow) && ctx.work.is_some();
    let uczen = ctx.employment.flags & Employment::FLAG_PUPIL != 0
        && !ctx.dow.is_weekend()
        && ctx.school.is_some();

    // Absencja (B-7): skutek progowy `AbsenceRisk` stosuje faza-właściciel, bo tylko
    // ona wie, co znaczy „nie poszedł do pracy". Wartości w `data/needs/needs.ron`.
    let absencja = if pracuje || uczen {
        absencja(ctx)
    } else {
        None
    };
    if let Some((need, effect)) = absencja {
        log.skip(DecisionReason::Deprivation { need, effect });
        return;
    }

    if pracuje {
        let cel = ctx.work.expect("pracuje bez miejsca pracy");
        let zmiana = ctx.employment.shift_kind();
        let (a, b) = zmiana.window();
        if zmiana.crosses_midnight() {
            // Nocka zawija się przez północ, a plan nie: dzisiejsza doba dostaje
            // ogon zmiany rozpoczętej wczoraj i początek dzisiejszej.
            if ctx.employment.works_on(poprzedni_dzien(ctx.dow)) {
                wstaw_zobowiazanie(canvas, log, stats, ActivityKind::Work, 0, b.get(), cel);
                powrot(ctx, canvas, log, stats, b.get(), cel);
            }
            let start = a.get();
            dojazd_przed(ctx, canvas, log, stats, start, ctx.home, cel);
            wstaw_zobowiazanie(
                canvas,
                log,
                stats,
                ActivityKind::Work,
                start,
                1440 - start,
                cel,
            );
        } else {
            let (start, dur) = (a.get(), b.get() - a.get());
            dojazd_przed(ctx, canvas, log, stats, start, ctx.home, cel);
            wstaw_zobowiazanie(canvas, log, stats, ActivityKind::Work, start, dur, cel);
            powrot(ctx, canvas, log, stats, start + dur, cel);
        }
    } else if uczen {
        let cel = ctx.school.expect("uczeń bez szkoły");
        dojazd_przed(ctx, canvas, log, stats, SCHOOL_OPEN, ctx.home, cel);
        wstaw_zobowiazanie(
            canvas,
            log,
            stats,
            ActivityKind::School,
            SCHOOL_OPEN,
            SCHOOL_CLOSE - SCHOOL_OPEN,
            cel,
        );
        dojazd_po(ctx, canvas, log, stats, SCHOOL_CLOSE, cel, ctx.home);
    } else if !ctx.household.escorts.is_empty() || !ctx.household.pickups.is_empty() {
        // Nie pracuje, ale odprowadza: dwie osobne wyprawy dom → szkoła → dom.
        odprowadzenie_osobne(ctx, canvas, log, stats);
    }
}

/// Czy mieszkaniec nie idzie dziś do pracy z powodu deprywacji.
///
/// Jedno losowanie na dobę, po **największym** ryzyku spośród potrzeb w deprywacji —
/// nie po sumie: dwie potrzeby poniżej progu nie mają dawać pewnej absencji.
/// Kolejność `NeedKind::ALL` jest deterministyczna (00 §3.2).
fn absencja(ctx: &PlanCtx<'_>) -> Option<(NeedKind, magnat_core::DeprivationEffect)> {
    let mut najwieksze = 0u32;
    let mut winna = None;
    for n in NeedKind::ALL {
        let poziom = ctx.citizen.needs.get(*n);
        if !ctx.needs.is_deprived(*n, poziom) {
            continue;
        }
        for e in &ctx.needs.spec(*n).effects {
            if e.effect == magnat_core::DeprivationEffect::AbsenceRisk && e.magnitude > najwieksze {
                najwieksze = e.magnitude;
                winna = Some((*n, e.effect));
            }
        }
    }
    let (need, effect) = winna?;
    let mut r = ctx.rng_for(0);
    if r.gen_bool_permille(najwieksze.min(1000) as u16) {
        Some((need, effect))
    } else {
        None
    }
}

fn poprzedni_dzien(d: DayOfWeek) -> DayOfWeek {
    DayOfWeek::from_day_index((d as u64 + 6) % 7)
}

fn wstaw_zobowiazanie(
    canvas: &mut DayCanvas,
    log: &mut Log<'_>,
    stats: &mut PlanStats,
    kind: ActivityKind,
    start: u16,
    dur: u16,
    cel: PlaceRef,
) -> bool {
    let rodzaj = match kind {
        ActivityKind::Work => CommitmentKind::Work,
        ActivityKind::School => CommitmentKind::School,
        ActivityKind::Errand => CommitmentKind::Childcare,
        _ => CommitmentKind::Commute,
    };
    let powod = DecisionReason::Commitment { kind: rodzaj };
    if wstaw(canvas, log, kind, start, dur, cel, powod, rodzaj as u8) {
        stats.commitments += 1;
        return true;
    }
    false
}

/// Dojazd kończący się dokładnie o `arrive`.
///
/// Odprowadzenie dziecka rozbija poranną drogę na **trzy sloty** — dom → szkoła,
/// przekazanie, szkoła → praca — zamiast jednego o zsumowanym czasie. Powód jest
/// twardy: każdy slot `Commute` musi trwać dokładnie tyle, ile `TravelOracle` liczy
/// dla jego pary miejsc, bo przy wykonaniu planu `begin_trip` dostanie właśnie tę
/// parę. Slot „dom → praca przez szkołę" łamałby to przy pierwszym wyjściu z domu
/// i pieszy docierałby o dziesięć minut wcześniej, niż plan zakłada.
///
/// Odbiór dziecka po południu należy do M3c: wynika z podziału ról w gospodarstwie,
/// a szkoła kończy się przed pracą. M3b odprowadza rano i na tym poprzestaje.
fn dojazd_przed(
    ctx: &PlanCtx<'_>,
    canvas: &mut DayCanvas,
    log: &mut Log<'_>,
    stats: &mut PlanStats,
    arrive: u16,
    from: PlaceRef,
    to: PlaceRef,
) {
    let przez = ctx
        .household
        .escorts
        .first()
        .copied()
        .filter(|_| from == ctx.home);
    let Some(szkola) = przez else {
        let dur = minuty(ctx, from, to, arrive);
        wstaw_dojazd(
            canvas,
            log,
            stats,
            arrive.saturating_sub(dur),
            dur,
            to,
            CommitmentKind::Commute,
        );
        return;
    };

    let do_szkoly = minuty(ctx, from, szkola, arrive);
    let ze_szkoly = minuty(ctx, szkola, to, arrive);
    let calosc = do_szkoly
        .saturating_add(ESCORT_HANDOVER_MIN)
        .saturating_add(ze_szkoly);
    let start = arrive.saturating_sub(calosc);
    wstaw_dojazd(
        canvas,
        log,
        stats,
        start,
        do_szkoly,
        szkola,
        CommitmentKind::Childcare,
    );
    wstaw_zobowiazanie(
        canvas,
        log,
        stats,
        ActivityKind::Errand,
        start + do_szkoly,
        ESCORT_HANDOVER_MIN,
        szkola,
    );
    wstaw_dojazd(
        canvas,
        log,
        stats,
        start + do_szkoly + ESCORT_HANDOVER_MIN,
        ze_szkoly,
        to,
        CommitmentKind::Commute,
    );
}

/// Powrót z pracy do domu — prosto albo **przez szkołę po dziecko** (korekta C-8).
///
/// Trzy sloty zamiast jednego, z tego samego powodu co przy odprowadzaniu rano: każdy
/// `Commute` musi trwać dokładnie tyle, ile `TravelOracle` liczy dla jego pary miejsc,
/// bo przy wykonaniu planu `begin_trip` dostanie właśnie tę parę.
fn powrot(
    ctx: &PlanCtx<'_>,
    canvas: &mut DayCanvas,
    log: &mut Log<'_>,
    stats: &mut PlanStats,
    depart: u16,
    from: PlaceRef,
) {
    let Some(szkola) = ctx.household.pickups.first().copied() else {
        dojazd_po(ctx, canvas, log, stats, depart, from, ctx.home);
        return;
    };
    let do_szkoly = minuty(ctx, from, szkola, depart);
    let do_domu = minuty(ctx, szkola, ctx.home, depart);
    let calosc = do_szkoly
        .saturating_add(ESCORT_HANDOVER_MIN)
        .saturating_add(do_domu);
    if u32::from(depart) + u32::from(calosc) > 1440 {
        // Nie mieści się przed północą — wtedy nie wchodzi wcale, tak samo jak
        // pojedynczy dojazd (korekta F-14). Odbiór dziecka jest zobowiązaniem,
        // ale plan nie przechodzi przez północ.
        return;
    }
    wstaw_dojazd(
        canvas,
        log,
        stats,
        depart,
        do_szkoly,
        szkola,
        CommitmentKind::Childcare,
    );
    wstaw_zobowiazanie(
        canvas,
        log,
        stats,
        ActivityKind::Errand,
        depart + do_szkoly,
        ESCORT_HANDOVER_MIN,
        szkola,
    );
    wstaw_dojazd(
        canvas,
        log,
        stats,
        depart + do_szkoly + ESCORT_HANDOVER_MIN,
        do_domu,
        ctx.home,
        CommitmentKind::Commute,
    );
}

fn dojazd_po(
    ctx: &PlanCtx<'_>,
    canvas: &mut DayCanvas,
    log: &mut Log<'_>,
    stats: &mut PlanStats,
    depart: u16,
    from: PlaceRef,
    to: PlaceRef,
) {
    let dur = minuty(ctx, from, to, depart);
    // Dojazd, który nie mieści się przed północą, nie wchodzi do planu **skrócony**:
    // skrócony slot kłamałby o czasie dojścia, a plan nie przechodzi przez północ.
    if u32::from(depart) + u32::from(dur) > 1440 {
        return;
    }
    wstaw_dojazd(canvas, log, stats, depart, dur, to, CommitmentKind::Commute);
}

#[inline]
fn minuty(ctx: &PlanCtx<'_>, from: PlaceRef, to: PlaceRef, kiedy: u16) -> u16 {
    ctx.travel
        .estimate(from, to, MinuteOfDay::new(kiedy), &ctx.citizen)
        .minutes
}

fn wstaw_dojazd(
    canvas: &mut DayCanvas,
    log: &mut Log<'_>,
    stats: &mut PlanStats,
    start: u16,
    dur: u16,
    to: PlaceRef,
    kind: CommitmentKind,
) {
    if wstaw(
        canvas,
        log,
        ActivityKind::Commute,
        start,
        dur,
        to,
        DecisionReason::Commitment { kind },
        kind as u8,
    ) {
        stats.commitments += 1;
    }
}

/// Odprowadzenie bez pracy własnej: dom → szkoła → dom, rano i po odbiór po południu.
/// Każdy odcinek osobnym slotem, z tego samego powodu co wyżej.
///
/// Rano jedzie się do szkoły z `escorts`, po południu do tej z `pickups` — dla jedynego
/// dorosłego w gospodarstwie to ta sama szkoła, ale dla dwojga rodziców podział ról
/// może przypisać poranek jednemu, a popołudnie drugiemu (korekta C-8).
fn odprowadzenie_osobne(
    ctx: &PlanCtx<'_>,
    canvas: &mut DayCanvas,
    log: &mut Log<'_>,
    stats: &mut PlanStats,
) {
    let kursy = [
        (SCHOOL_OPEN, true, ctx.household.escorts.first().copied()),
        (SCHOOL_CLOSE, false, ctx.household.pickups.first().copied()),
    ];
    for (kiedy, do_szkoly, szkola) in kursy {
        let Some(szkola) = szkola else { continue };
        let tam = minuty(ctx, ctx.home, szkola, kiedy);
        let start = if do_szkoly {
            kiedy.saturating_sub(tam + ESCORT_HANDOVER_MIN)
        } else {
            kiedy.saturating_sub(tam)
        };
        wstaw_dojazd(
            canvas,
            log,
            stats,
            start,
            tam,
            szkola,
            CommitmentKind::Childcare,
        );
        wstaw_zobowiazanie(
            canvas,
            log,
            stats,
            ActivityKind::Errand,
            start + tam,
            ESCORT_HANDOVER_MIN,
            szkola,
        );
        wstaw_dojazd(
            canvas,
            log,
            stats,
            start + tam + ESCORT_HANDOVER_MIN,
            tam,
            ctx.home,
            CommitmentKind::Commute,
        );
    }
}
