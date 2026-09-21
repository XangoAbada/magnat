use super::canvas::{wstaw, Gap};
use super::*;
use magnat_core::CitizenReason;

/// Poniżej tego poziomu potrzeba wywołuje własny slot w fazie 2 albo zadanie w fazie 3.
const HEALTH_TRIGGER: u8 = 30;
const CLOTHING_TRIGGER: u8 = 30;

/// Zapas gospodarstwa, poniżej którego planer wstawia zakupy — w dniach.
const STOCK_THRESHOLD_DAYS: u8 = 2;
/// Kara za minutę nadłożonej drogi przy wyborze miejsca zadania. Trzy, bo minuta
/// nadłożona boli bardziej niż minuta dojścia — idzie się nią „dodatkowo".
const W_DETOUR: i32 = 3;

// ── faza 3: zadania ─────────────────────────────────────────────────────────────

/// Zadanie doby: co zaspokoić, jak pilnie i dlaczego.
#[derive(Clone, Copy, Debug)]
struct Zadanie {
    need: NeedKind,
    pilnosc: u16,
    powod: DecisionReason,
}

impl Default for Zadanie {
    /// Wartość **wypełniająca** `ArrayVec`, nie zadanie domyślne: pilność 0 znaczy
    /// „nie ma zadania", a długość wektora i tak odcina ten slot. To samo miejsce
    /// i ten sam powód, dla którego `PlaceCandidate::default` niesie `Unspecified`.
    fn default() -> Zadanie {
        Zadanie {
            need: NeedKind::Hunger,
            pilnosc: 0,
            powod: DecisionReason::Unspecified,
        }
    }
}

/// Lista zadań doby, posortowana malejąco po pilności; remisy po indeksie potrzeby,
/// czyli deterministycznie (00 §3.2).
///
/// Żywność i napoje trafiają w ten sam głód, więc do planu wchodzi ta kategoria,
/// której brakuje bardziej — jedno wyjście po zakupy, nie dwa. Kategoria wskazująca
/// potrzebę bez miejsc w `data/needs/needs.ron` (paliwo → M4, wyposażenie → M5) nie
/// produkuje zadania w ogóle: to granica fazy, a wpis „nie znam miejsca" mówiłby
/// nieprawdę.
fn lista_zadan(ctx: &PlanCtx<'_>) -> ArrayVec<Zadanie, 16> {
    let mut out: ArrayVec<Zadanie, 16> = ArrayVec::new();
    let zdrowie = ctx.citizen.needs.get(NeedKind::Health);
    if zdrowie.get() < HEALTH_TRIGGER {
        out.push(Zadanie {
            need: NeedKind::Health,
            pilnosc: 250,
            powod: DecisionReason::Citizen(CitizenReason::NeedCritical {
                need: NeedKind::Health,
                level: zdrowie,
            }),
        });
    }

    for cat in StockCat::ALL {
        let dni = ctx.household.stock[cat.as_index()];
        if dni > STOCK_THRESHOLD_DAYS {
            continue;
        }
        let need = cat.need();
        if ctx.needs.places_for(need).is_empty() {
            continue;
        }
        let pilnosc = 200 - u16::from(dni.min(20)) * 10;
        let powod = DecisionReason::Citizen(CitizenReason::StockBelowThreshold {
            cat: *cat,
            days_left: dni,
        });
        match out.as_mut_slice().iter_mut().find(|z| z.need == need) {
            Some(istniejace) if pilnosc > istniejace.pilnosc => {
                istniejace.pilnosc = pilnosc;
                istniejace.powod = powod;
            }
            Some(_) => {}
            None => {
                out.push(Zadanie {
                    need,
                    pilnosc,
                    powod,
                });
            }
        }
    }

    let ubranie = ctx.citizen.needs.get(NeedKind::Clothing);
    if ubranie.get() < CLOTHING_TRIGGER && !out.iter().any(|z| z.need == NeedKind::Clothing) {
        out.push(Zadanie {
            need: NeedKind::Clothing,
            pilnosc: 150,
            powod: DecisionReason::Citizen(CitizenReason::NeedCritical {
                need: NeedKind::Clothing,
                level: ubranie,
            }),
        });
    }

    let mut lista = out;
    lista
        .as_mut_slice()
        .sort_by_key(|z| (std::cmp::Reverse(z.pilnosc), z.need.as_index()));
    lista
}

/// Wybrana realizacja zadania: gdzie, kiedy i za ile minut.
#[derive(Clone, Copy, Debug)]
struct Wybor {
    miejsce: PlaceRef,
    start: u16,
    tam: u16,
    wizyta: u16,
    powrot: u16,
    do_kogo: PlaceRef,
    powod: DecisionReason,
    ocena: i32,
    /// Indeks dojazdu, który to zadanie zastępuje trójką „dojdź → załatw → dojdź".
    /// `None` = zadanie mieści się w wolnej luce i niczego nie rusza.
    zastepuje: Option<usize>,
}

pub(super) fn faza3_zadania(
    ctx: &PlanCtx<'_>,
    canvas: &mut DayCanvas,
    log: &mut Log<'_>,
    stats: &mut PlanStats,
    od: u16,
) {
    let mut kandydaci: ArrayVec<PlaceCandidate, MAX_CANDIDATES> = ArrayVec::new();
    for zad in lista_zadan(ctx).iter() {
        if canvas.len() + 3 > MAX_SLOTS {
            log.skip(DecisionReason::Citizen(
                CitizenReason::SlotBudgetExhausted { dropped: zad.need },
            ));
            stats.tasks_dropped += 1;
            continue;
        }
        match najlepsza_realizacja(ctx, canvas, log, zad, &mut kandydaci, od) {
            Ok(w) => {
                if let Some(i) = w.zastepuje {
                    canvas.remove(i);
                    log.drop_slot(i as u8);
                }
                wstaw(
                    canvas,
                    log,
                    ActivityKind::Commute,
                    w.start,
                    w.tam,
                    w.miejsce,
                    zad.powod,
                    zad.need.as_index() as u8,
                );
                wstaw(
                    canvas,
                    log,
                    aktywnosc_zadania(zad.need),
                    w.start + w.tam,
                    w.wizyta,
                    w.miejsce,
                    w.powod,
                    zad.need.as_index() as u8,
                );
                if w.powrot > 0 {
                    wstaw(
                        canvas,
                        log,
                        ActivityKind::Commute,
                        w.start + w.tam + w.wizyta,
                        w.powrot,
                        w.do_kogo,
                        DecisionReason::Citizen(CitizenReason::Commitment {
                            kind: CommitmentKind::Commute,
                        }),
                        CommitmentKind::Commute as u8,
                    );
                }
                stats.tasks_placed += 1;
            }
            Err(powod) => {
                log.skip(powod);
                stats.tasks_dropped += 1;
            }
        }
    }
}

/// Dwie drogi do załatwienia zadania; wygrywa lepiej oceniona.
///
/// **A — wolna luka.** Mieszkaniec wychodzi skądś, załatwia i wraca tam, gdzie ma być
/// po luce. Tak wygląda wyprawa po zakupy z domu.
///
/// **B — wplecenie w dojazd** (premia „po drodze" z §5.4). Dojazd A → B, po którym
/// nic nie musi się zacząć o konkretnej minucie, rozpada się na `A → P`, wizytę w `P`
/// i `P → B`. To jest dokładnie przypadek z PRD §5.5: market po drodze z pracy do domu,
/// a nie osobna wyprawa po kolacji. Dojazdy **do** zobowiązania są nietykalne —
/// spóźnić się do pracy przez zakupy to nie jest optymalizacja trasy.
fn najlepsza_realizacja(
    ctx: &PlanCtx<'_>,
    canvas: &DayCanvas,
    log: &mut Log<'_>,
    zad: &Zadanie,
    kandydaci: &mut ArrayVec<PlaceCandidate, MAX_CANDIDATES>,
    od: u16,
) -> Result<Wybor, DecisionReason> {
    let wizyta = ctx.needs.spec(zad.need).visit_min;
    let mut luki: ArrayVec<Gap, 32> = ArrayVec::new();
    canvas.gaps(od, &mut luki);

    let mut najlepszy: Option<Wybor> = None;
    let mut najdluzsza = 0u16;
    let mut brak_wiedzy: Option<DecisionReason> = None;

    // A — wolne luki.
    for g in luki.iter() {
        najdluzsza = najdluzsza.max(g.len());
        if g.len() <= wizyta {
            continue;
        }
        let kotwica = canvas.place_at(g.start, ctx.home);
        let po_luce = canvas.required_place_at(g.end, ctx.home).unwrap_or(kotwica);
        rozwaz(
            ctx,
            log,
            zad,
            kandydaci,
            &mut najlepszy,
            &mut brak_wiedzy,
            Okno {
                kotwica,
                cel_po: po_luce,
                start: g.start,
                koniec: g.end,
                wizyta,
                zastepuje: None,
            },
        );
    }

    // B — wplecenie w dojazd o elastycznym końcu.
    for (i, slot) in canvas.slots().iter().enumerate() {
        if slot.kind != ActivityKind::Commute as u8 || slot.start_min < od {
            continue;
        }
        if canvas.slot_starting_at(slot.end_min()).is_some() {
            continue; // dojazd do zobowiązania — nie wolno go rozciągać
        }
        let luz = luki
            .iter()
            .find(|g| g.start == slot.end_min())
            .map_or(0, |g| g.len());
        rozwaz(
            ctx,
            log,
            zad,
            kandydaci,
            &mut najlepszy,
            &mut brak_wiedzy,
            Okno {
                kotwica: canvas.origin_of(i, ctx.home),
                cel_po: canvas.place_of(i),
                start: slot.start_min,
                koniec: slot.end_min() + luz,
                wizyta,
                zastepuje: Some(i),
            },
        );
    }

    match najlepszy {
        Some(w) => Ok(w),
        None => Err(
            brak_wiedzy.unwrap_or(DecisionReason::Citizen(CitizenReason::NoTimeWindow {
                need: zad.need,
                needed_min: wizyta,
                longest_gap_min: najdluzsza,
            })),
        ),
    }
}

/// Okno, w którym zadanie ma się zmieścić: skąd, dokąd, między którymi minutami.
#[derive(Clone, Copy, Debug)]
struct Okno {
    kotwica: PlaceRef,
    cel_po: PlaceRef,
    start: u16,
    koniec: u16,
    wizyta: u16,
    zastepuje: Option<usize>,
}

fn rozwaz(
    ctx: &PlanCtx<'_>,
    log: &mut Log<'_>,
    zad: &Zadanie,
    kandydaci: &mut ArrayVec<PlaceCandidate, MAX_CANDIDATES>,
    najlepszy: &mut Option<Wybor>,
    brak_wiedzy: &mut Option<DecisionReason>,
    okno: Okno,
) {
    if choose_place(
        ctx.places,
        zad.need,
        okno.kotwica,
        ctx.max_task_travel_min,
        &ctx.known,
        &ctx.citizen,
        kandydaci,
    )
    .is_err()
    {
        *brak_wiedzy = Some(DecisionReason::Citizen(CitizenReason::PlaceUnknown {
            need: zad.need,
            known_count: ctx.known.len().min(255) as u8,
        }));
        return;
    }

    let dostepne = okno.koniec.saturating_sub(okno.start);
    for kand in kandydaci.iter() {
        let tam = ctx
            .travel
            .estimate(
                okno.kotwica,
                kand.place,
                MinuteOfDay::new(okno.start),
                &ctx.citizen,
            )
            .minutes;
        let przyjscie = okno.start.saturating_add(tam);
        let godziny = ctx.places.opening_hours(kand.place);
        if !godziny.is_open(ctx.dow, MinuteOfDay::new(przyjscie)) {
            log.skip(DecisionReason::Citizen(CitizenReason::PlaceClosed {
                place: kand.place,
                opens_at: godziny.open,
            }));
            continue;
        }
        let powrot = if okno.cel_po == kand.place {
            0
        } else {
            ctx.travel
                .estimate(
                    kand.place,
                    okno.cel_po,
                    MinuteOfDay::new(przyjscie.saturating_add(okno.wizyta)),
                    &ctx.citizen,
                )
                .minutes
        };
        let potrzeba = tam.saturating_add(okno.wizyta).saturating_add(powrot);
        if potrzeba > dostepne {
            continue;
        }
        // „Po drodze" to sytuacja, w której po oknie mieszkaniec i tak musi być gdzie
        // indziej niż na jego początku: liczy się wtedy nadłożenie, nie cała wyprawa.
        let (powod, kara) = if okno.cel_po != okno.kotwica {
            let wprost = ctx
                .travel
                .estimate(
                    okno.kotwica,
                    okno.cel_po,
                    MinuteOfDay::new(okno.start),
                    &ctx.citizen,
                )
                .minutes;
            let nadlozenie = tam.saturating_add(powrot).saturating_sub(wprost);
            (
                DecisionReason::Citizen(CitizenReason::ChosenOnRoute {
                    detour_min: nadlozenie,
                    direct_min: wprost,
                }),
                i32::from(nadlozenie),
            )
        } else {
            (kand.reason, i32::from(tam) * 2)
        };
        // Zadanie zaczyna się tak późno, jak się da — mieszkaniec wychodzi po chleb
        // tuż przed tym, co ma po luce, a nie zaraz po śniadaniu. Wplecenie w dojazd
        // musi jednak ruszyć w jego minucie, bo to ten dojazd zastępuje.
        let start = if okno.zastepuje.is_some() {
            okno.start
        } else {
            okno.koniec.saturating_sub(potrzeba).max(okno.start)
        };
        let w = Wybor {
            miejsce: kand.place,
            start,
            tam,
            wizyta: okno.wizyta,
            powrot,
            do_kogo: okno.cel_po,
            powod,
            ocena: kand.score - W_DETOUR * kara,
            zastepuje: okno.zastepuje,
        };
        let lepszy = najlepszy.is_none_or(|b| {
            (w.ocena, std::cmp::Reverse(w.start)) > (b.ocena, std::cmp::Reverse(b.start))
        });
        if lepszy {
            *najlepszy = Some(w);
        }
    }
}

fn aktywnosc_zadania(need: NeedKind) -> ActivityKind {
    match need {
        NeedKind::Health => ActivityKind::Errand,
        _ => ActivityKind::Shop,
    }
}
