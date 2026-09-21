use super::canvas::{wstaw, Gap};
use super::*;
use magnat_core::CitizenReason;

/// Minut między pobudką a wyjściem z domu (mycie, śniadanie, zbieranie się).
const PREP_MIN: u16 = 50;
/// Luka krótsza niż tyle nie dostaje zajęcia, tylko wypełniacz.
const LEISURE_MIN_GAP: u16 = 30;
/// Poniżej tego poziomu potrzeba wywołuje własny slot w fazie 2 albo zadanie w fazie 3.
const HYGIENE_TRIGGER: u8 = 40;

// ── faza 2: potrzeby krytyczne jako okna ────────────────────────────────────────

pub(super) fn faza2_potrzeby(ctx: &PlanCtx<'_>, canvas: &mut DayCanvas, log: &mut Log<'_>) {
    let (bed, sleep_need) = chronotyp(ctx);
    let pierwsze = canvas.first_start();

    // Pobudka: tyle przed pierwszym zobowiązaniem, żeby zdążyć się zebrać.
    let wake = match pierwsze {
        Some(t) if t > 0 => t.saturating_sub(PREP_MIN),
        // Zmiana nocna: pierwszym slotem doby jest praca od północy, więc sen
        // nie zaczyna się o 00:00 — trafia do luki dziennej niżej.
        Some(_) => 0,
        None => (bed + sleep_need) % 1440,
    };

    let poziom_snu = ctx.citizen.needs.get(NeedKind::Sleep);
    let powod_snu = DecisionReason::Citizen(CitizenReason::NeedCritical {
        need: NeedKind::Sleep,
        level: poziom_snu,
    });
    let mut spal = false;
    if wake > 0 {
        spal |= wstaw(
            canvas,
            log,
            ActivityKind::Sleep,
            0,
            wake,
            ctx.home,
            powod_snu,
            NeedKind::Sleep.as_index() as u8,
        );
    }
    let bed = bed.max(ostatni_koniec(canvas).saturating_add(30)).min(1439);
    spal |= wstaw(
        canvas,
        log,
        ActivityKind::Sleep,
        bed,
        1440 - bed,
        ctx.home,
        powod_snu,
        poziom_snu.get(),
    );
    if !spal {
        // Nocka albo plan tak ciasny, że okno snu nie weszło w swoje miejsce:
        // sen ląduje w najdłuższej wolnej luce. Sen jest niewywłaszczalny tak samo
        // jak praca (test `plan_sleep_and_meal_survive`) — musi gdzieś być.
        let mut luki: ArrayVec<Gap, 32> = ArrayVec::new();
        canvas.gaps(0, &mut luki);
        if let Some(g) = luki
            .iter()
            .copied()
            .max_by_key(|g| (g.len(), 1440 - g.start))
        {
            let dur = g.len().min(sleep_need);
            wstaw(
                canvas,
                log,
                ActivityKind::Sleep,
                g.start,
                dur,
                ctx.home,
                powod_snu,
                NeedKind::Sleep.as_index() as u8,
            );
        }
    }

    let higiena = ctx.citizen.needs.get(NeedKind::Hygiene);
    if higiena.get() < HYGIENE_TRIGGER {
        let dur = ctx.needs.spec(NeedKind::Hygiene).visit_min;
        wstaw_w_oknie(
            ctx,
            canvas,
            log,
            Gap {
                start: wake,
                end: wake.saturating_add(120).min(1440),
            },
            dur,
            ActivityKind::Idle,
            DecisionReason::Citizen(CitizenReason::NeedCritical {
                need: NeedKind::Hygiene,
                level: higiena,
            }),
            NeedKind::Hygiene.as_index() as u8,
        );
    }

    let glod = ctx.citizen.needs.get(NeedKind::Hunger);
    let posilek = ctx.needs.spec(NeedKind::Hunger).visit_min;
    let powod_jedzenia = DecisionReason::Citizen(CitizenReason::NeedCritical {
        need: NeedKind::Hunger,
        level: glod,
    });
    let okna = [
        Gap {
            start: wake,
            end: wake.saturating_add(150).min(1440),
        },
        Gap {
            start: 11 * 60,
            end: 15 * 60,
        },
        Gap {
            start: 17 * 60,
            end: 21 * 60,
        },
    ];
    let mut zjadl = 0u8;
    let mut najdluzsza = 0u16;
    for okno in okna {
        let wynik = wstaw_w_oknie(
            ctx,
            canvas,
            log,
            okno,
            posilek,
            ActivityKind::Eat,
            powod_jedzenia,
            NeedKind::Hunger.as_index() as u8,
        );
        match wynik {
            Some(_) => zjadl += 1,
            None => najdluzsza = najdluzsza.max(najdluzsza_luka(canvas, okno)),
        }
    }
    if zjadl == 0 {
        log.skip(DecisionReason::Citizen(CitizenReason::NoTimeWindow {
            need: NeedKind::Hunger,
            needed_min: posilek,
            longest_gap_min: najdluzsza,
        }));
    }
}

/// Pora snu i jego długość. Chronotyp zależy od towarzyskości (sowy są towarzyskie)
/// i od wieku — dzieci i seniorzy śpią dłużej i kładą się wcześniej.
fn chronotyp(ctx: &PlanCtx<'_>) -> (u16, u16) {
    let lata = ctx.citizen.identity.age_years(ctx.citizen.today);
    let potrzeba: u16 = match lata {
        ..=13 => 600,
        14..=17 => 540,
        18..=64 => 480,
        _ => 420,
    };
    let towarzyskosc = i32::from(ctx.citizen.personality.get(TraitId::Sociability).get());
    let przesuniecie = (towarzyskosc - 50) * 6 / 10; // ±30 min
    let baza: i32 = match lata {
        ..=13 => 20 * 60,
        14..=17 => 22 * 60,
        18..=64 => 22 * 60 + 30,
        _ => 21 * 60 + 30,
    };
    (
        (baza + przesuniecie).clamp(19 * 60, 23 * 60 + 30) as u16,
        potrzeba,
    )
}

fn ostatni_koniec(canvas: &DayCanvas) -> u16 {
    canvas
        .slots()
        .iter()
        .map(PlanSlot::end_min)
        .max()
        .unwrap_or(0)
}

fn najdluzsza_luka(canvas: &DayCanvas, okno: Gap) -> u16 {
    let mut luki: ArrayVec<Gap, 32> = ArrayVec::new();
    canvas.gaps(0, &mut luki);
    luki.iter()
        .map(|g| {
            let a = g.start.max(okno.start);
            let b = g.end.min(okno.end);
            b.saturating_sub(a)
        })
        .max()
        .unwrap_or(0)
}

/// Wstawia czynność w pierwszej luce przecinającej okno. Miejsce = tam, gdzie
/// mieszkaniec wtedy jest, bo faza 2 nikogo nigdzie nie wysyła.
#[allow(clippy::too_many_arguments)]
fn wstaw_w_oknie(
    ctx: &PlanCtx<'_>,
    canvas: &mut DayCanvas,
    log: &mut Log<'_>,
    okno: Gap,
    dur: u16,
    kind: ActivityKind,
    powod: DecisionReason,
    param: u8,
) -> Option<u16> {
    let mut luki: ArrayVec<Gap, 32> = ArrayVec::new();
    canvas.gaps(0, &mut luki);
    for g in luki.iter() {
        let start = g.start.max(okno.start);
        let end = g.end.min(okno.end);
        if end.saturating_sub(start) < dur {
            continue;
        }
        let gdzie = canvas.place_at(start, ctx.home);
        if wstaw(canvas, log, kind, start, dur, gdzie, powod, param) {
            return Some(start);
        }
    }
    None
}

// ── faza 4: czas wolny ──────────────────────────────────────────────────────────

pub(super) fn faza4_czas_wolny(
    ctx: &PlanCtx<'_>,
    canvas: &mut DayCanvas,
    log: &mut Log<'_>,
    od: u16,
) {
    // Dwa przebiegi: najpierw luki nadające się na zajęcie, potem reszta jako
    // wypełniacz. Odwrotna kolejność zjadłaby sloty na trzyminutowe przerwy
    // i wieczór zostałby pusty.
    for dlugie in [true, false] {
        let mut luki: ArrayVec<Gap, 32> = ArrayVec::new();
        canvas.gaps(od, &mut luki);
        for g in luki.iter() {
            if canvas.is_full() {
                return;
            }
            if (g.len() >= LEISURE_MIN_GAP) != dlugie {
                continue;
            }
            let gdzie = canvas.place_at(g.start, ctx.home);
            let (kind, cecha, waga) = if dlugie {
                wybierz_zajecie(ctx, *g)
            } else {
                (ActivityKind::Idle, TraitId::Conscientiousness, 0)
            };
            wstaw(
                canvas,
                log,
                kind,
                g.start,
                g.len(),
                gdzie,
                DecisionReason::Citizen(CitizenReason::FreeTimePreference {
                    trait_id: cecha,
                    weight: waga,
                }),
                waga,
            );
        }
    }
}

/// Losowanie zajęcia z wagami z osobowości, potrzeb i pory doby.
///
/// `ponytail:` losowanie ważone zamiast softmaksu. Sufit nazwany: nie da się nim
/// wyrazić „temperatury" wyboru. Softmax wymagałby `det_math::exp` na trzech wagach
/// w ścieżce planera, a różnica w rozkładzie jest tu nieodróżnialna od strojenia wag.
///
/// Czas wolny spędza się **tam, gdzie mieszkaniec jest** — potrzeby `Leisure`
/// i `Social` mają w `data/needs/needs.ron` `Home` na liście miejsc. Wyjście na miasto
/// wymaga budżetu rozrywki, a ten należy do M5.
fn wybierz_zajecie(ctx: &PlanCtx<'_>, g: Gap) -> (ActivityKind, TraitId, u8) {
    let noc = g.end <= 6 * 60 || g.start >= 22 * 60;
    let poziom = |n: NeedKind| u32::from(100 - ctx.citizen.needs.get(n).get().min(100));
    let cecha = |t: TraitId| u32::from(ctx.citizen.personality.get(t).get());

    let mut w_leisure = 40 + poziom(NeedKind::Leisure) / 2 + cecha(TraitId::Openness) / 4;
    let mut w_social = 20 + poziom(NeedKind::Social) / 2 + cecha(TraitId::Sociability) / 3;
    let w_dom = 40 + cecha(TraitId::Conscientiousness) / 4;
    if ctx.dow.is_weekend() {
        w_social = w_social * 3 / 2;
        w_leisure = w_leisure * 3 / 2;
    }
    if noc {
        w_social = 0;
        w_leisure /= 4;
    }

    let suma = (w_leisure + w_social + w_dom).max(1);
    let mut r = ctx.rng_for(g.start);
    let los = r.gen_range_u32(suma);
    if los < w_leisure {
        (
            ActivityKind::Leisure,
            TraitId::Openness,
            (w_leisure * 100 / suma).min(100) as u8,
        )
    } else if los < w_leisure + w_social {
        (
            ActivityKind::Social,
            TraitId::Sociability,
            (w_social * 100 / suma).min(100) as u8,
        )
    } else {
        (
            ActivityKind::Idle,
            TraitId::Conscientiousness,
            (w_dom * 100 / suma).min(100) as u8,
        )
    }
}
