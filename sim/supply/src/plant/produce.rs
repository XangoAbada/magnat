//! `advance_production` — **jedna** funkcja produkcji dla wszystkich trzech poziomów
//! LOD (M6b §5.5, WP4; M6 §7.6, ryzyko `R8`).
//!
//! Mikro (per maszyna), mezo (per linia) i makro (per zakład) różnią się **wyłącznie
//! ziarnistością wywołania** i tym, czy powstają wizualne encje. Dlatego pętla jest
//! wewnątrz: `advance_production(site, from, 60)` robi dokładnie to samo, co sześćdziesiąt
//! wywołań po jednej minucie, łącznie z ciągiem losowań awarii — bo strumień RNG jest
//! kluczowany **absolutną minutą**, a nie licznikiem wywołań. Na tym stoi tolerancja 0
//! z 00 §4, i to jest powód, dla którego ta funkcja nie skacze między zdarzeniami.
//!
//! `ponytail:` pętla minuta po minucie, O(minutes). Sufit nazwany: przy 2 400 liniach
//! i kroku minutowym to jest kilkadziesiąt mikrosekund, więc nie ma czego optymalizować;
//! dopiero makro z krokiem dobowym zapłaci za to 1 440 obrotów. Droga wyjścia to skok
//! do najbliższego zdarzenia (koniec szarży, koniec przezbrojenia, przegląd) z osobnym
//! losowaniem awarii na przedziale — ale wtedy trzeba udowodnić, że rozkład awarii
//! się nie zmienił, a dziś nie ma czym tego zmierzyć.

use magnat_core::{
    rng, DecisionReason, Energy, GoodId, LossKind, Mass, Money, RecipeId, SimCalendar, SimMinute,
    SiteId, StreamId, Tick, UtilityService, Volume, Q,
};

use super::line::{BreakCause, Charge, LineState, ProductionLine};
use super::PlantSite;
use crate::batch::{BatchFlags, BatchOrigin};
use crate::catalog::{Catalog, OutputKind, Recipe, RecipeSource};
use crate::cost::allocate_cost;
use crate::mining::Deposits;
use crate::store::BatchDraft;
use crate::store::MassIn;
use crate::tuning::{Tuning, WattMinutes};
use crate::{Plant, Store};

/// To, co produkcja czyta i nie zmienia.
///
/// Wyjątkiem jest [`ProductionCtx::deposits`]: bilans złoża **zmienia się** w trakcie,
/// bo wydobycie to jedyne miejsce w M6, gdzie masa wchodzi do gry spoza magazynu.
/// Port bierze `&self` z wewnętrzną mutowalnością po stronie implementacji — pełne
/// uzasadnienie w [`crate::mining`]. Zakład bez złoża dostaje [`crate::NoDeposits`].
pub struct ProductionCtx<'a> {
    pub cat: &'a Catalog,
    pub tuning: &'a Tuning,
    pub world_seed: u64,
    pub deposits: &'a dyn Deposits,
}

/// Co się wydarzyło w tym wywołaniu. Sumy, nie zdarzenia — zdarzenia są w pierścieniu
/// powodów zakładu i w księdze partii.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProductionReport {
    pub batches: u32,
    pub produced: Mass,
    /// Koszt wytworzenia zaksięgowany w partiach — **nie** wydatek gotówkowy.
    /// Faktura za media jest osobno, raz na miesiąc (`Plant::bill_utilities`).
    pub cost: Money,
    pub setups: u32,
    pub breakdowns: u32,
    pub halts: u32,
}

impl ProductionReport {
    fn add(&mut self, other: ProductionReport) {
        self.batches += other.batches;
        self.produced = Mass(self.produced.0 + other.produced.0);
        self.cost = Money(self.cost.0 + other.cost.0);
        self.setups += other.setups;
        self.breakdowns += other.breakdowns;
        self.halts += other.halts;
    }
}

/// Posuwa produkcję zakładu o `minutes` minut, zaczynając od `from`.
pub fn advance_production(
    ctx: &ProductionCtx<'_>,
    store: &mut Store,
    plant: &mut Plant,
    site: SiteId,
    from: SimMinute,
    minutes: u32,
) -> ProductionReport {
    let mut raport = ProductionReport::default();
    let Some(zaklad) = plant.get_mut(site) else {
        return raport;
    };
    for m in 0..u64::from(minutes) {
        let teraz = SimMinute(from.0 + m);
        zaklad.emissions.pm_g_last_minute = 0;
        for i in 0..zaklad.lines.len() {
            raport.add(krok_linii(ctx, store, zaklad, i, teraz));
        }
    }
    raport
}

/// Jedna minuta jednej linii.
fn krok_linii(
    ctx: &ProductionCtx<'_>,
    store: &mut Store,
    zaklad: &mut PlantSite,
    i: usize,
    now: SimMinute,
) -> ProductionReport {
    let mut raport = ProductionReport::default();
    let mut l = zaklad.lines[i];
    let przed = l.state;

    match l.state {
        LineState::Broken { cause, .. } => naprawa(ctx, store, zaklad, &mut l, cause, now),
        LineState::Maintenance { until } if now.0 >= until.0 => {
            l.condition = Q::new(ctx.tuning.plant.condition_after_service);
            l.worked_since_service = 0;
            l.next_maintenance =
                SimMinute(now.0 + u64::from(zaklad.schedule.maintenance_interval_hours) * 60);
            l.state = LineState::Idle;
        }
        LineState::Setup { until, to_recipe } if now.0 >= until.0 => {
            l.recipe = Some(to_recipe);
            l.state = LineState::Idle;
        }
        LineState::Running { ends, .. } if now.0 >= ends.0 => {
            raport.add(zamknij_szarze(ctx, store, zaklad, &mut l, now));
        }
        _ => {}
    }

    // Przegląd planowy **przed** próbą startu: linia, która właśnie zamknęła szarżę,
    // w tej samej minucie zaczęłaby następną i nigdy nie trafiłaby na przegląd.
    // Szarży w toku przegląd nie przerywa — przerwany wypiek jest stratą większą
    // niż tydzień zwłoki w konserwacji.
    if matches!(l.state, LineState::Idle) && now.0 >= l.next_maintenance.0 {
        l.state = LineState::Maintenance {
            until: SimMinute(now.0 + u64::from(ctx.tuning.plant.maintenance_minutes)),
        };
    }

    if matches!(
        l.state,
        LineState::Idle | LineState::Starved { .. } | LineState::Blocked { .. }
    ) {
        raport.setups += u32::from(sprobuj_start(ctx, store, zaklad, &mut l, now));
    }

    // Odcięcie medium zatrzymuje linię **w toku**, a nie dopiero przy próbie startu
    // następnej szarży (M8b). Do M8b `cut_off` sprawdzało się wyłącznie w
    // `sprobuj_start`, więc szarża rozpoczęta przed blackoutem dochodziła do końca
    // i pobierała prąd, którego nie było — a test T2 mierzy „wolumen produkcji
    // w oknie awarii dokładnie 0", nie „po zakończeniu bieżącej szarży".
    //
    // Wsad przepada, tak samo jak przy awarii mechanicznej dwie linie niżej:
    // to jest ta sama sytuacja i ten sam zapis stanu, więc nie ma powodu, żeby
    // bilans masy rozróżniał, dlaczego maszyna stanęła. Przerwany wypiek jest
    // stratą i tak ma być.
    if let LineState::Running { recipe, .. } = l.state {
        let bez_pradu = l.power_draw.0 > 0 && !zaklad.has_utility(UtilityService::Electricity);
        // Woda idzie tą samą drogą i z tego samego powodu: receptura pobiera ją
        // przy **zamknięciu** szarży, więc szarża dowieziona do końca bez wody
        // zużyłaby wodę, której nie było. `sprobuj_start` blokował tylko start.
        let bez_wody =
            ctx.cat.recipe(recipe).water.0 > 0 && !zaklad.has_utility(UtilityService::Water);
        if bez_pradu || bez_wody {
            l.state = LineState::Broken {
                since: now,
                cause: if bez_pradu {
                    BreakCause::NoPower
                } else {
                    BreakCause::NoWater
                },
            };
        }
    }

    if let LineState::Running { .. } = l.state {
        l.age_minutes += 1;
        l.worked_since_service += 1;
        if let Some(m) = zaklad.meter_mut(UtilityService::Electricity) {
            m.draw_energy(WattMinutes::per_minute(l.power_draw));
        }
        if ctx.tuning.plant.wear_minutes_per_point > 0
            && l.worked_since_service
                .is_multiple_of(ctx.tuning.plant.wear_minutes_per_point)
        {
            l.condition = l.condition.saturating_sub(1);
        }
        if awaria(ctx, zaklad.site, i, &l, now) {
            l.state = LineState::Broken {
                since: now,
                cause: BreakCause::Wear,
            };
            raport.breakdowns += 1;
        }
    }

    zaklad.lines[i] = l;
    if let Some(powod) = nowy_postoj(przed, l.state) {
        zaklad.note(
            now,
            DecisionReason::ProductionHalted {
                site: zaklad.site,
                line: i as u16,
                cause: powod,
            },
        );
        raport.halts += 1;
    }
    raport
}

/// Postój, który właśnie się zaczął — a nie postój, który trwa. Karta inspekcji ma
/// pokazać zdarzenie, nie powtarzać tej samej linijki 1 440 razy na dobę.
fn nowy_postoj(przed: LineState, po: LineState) -> Option<magnat_core::LineStopCause> {
    let a = przed.stop_cause();
    let b = po.stop_cause();
    if a == b {
        None
    } else {
        b
    }
}

/// Naprawa: zużycie mechaniczne wymaga **części z magazynu**. Bez niej linia stoi —
/// i to jest cały sens wpięcia awarii w łańcuch dostaw (kryterium ukończenia WP4).
/// Brak prądu, wody i obsady mija sam, gdy przyczyna ustąpi.
fn naprawa(
    ctx: &ProductionCtx<'_>,
    store: &mut Store,
    zaklad: &mut PlantSite,
    l: &mut ProductionLine,
    cause: BreakCause,
    now: SimMinute,
) {
    if !cause.needs_part() {
        let ustapilo = match cause {
            BreakCause::NoPower => zaklad.has_utility(UtilityService::Electricity),
            BreakCause::NoWater => zaklad.has_utility(UtilityService::Water),
            _ => true,
        };
        if ustapilo {
            l.state = LineState::Idle;
        }
        return;
    }
    let potrzeba = masa_czesci(ctx.cat, l.spare_part);
    for slot in zaklad.inputs.clone() {
        if store.available(slot, l.spare_part, Q::MIN).0 < potrzeba.0 {
            continue;
        }
        let Some(r) = store.reserve(slot, l.spare_part, potrzeba, Q::MIN) else {
            continue;
        };
        if store.take(r).is_ok() {
            l.state = LineState::Maintenance {
                until: SimMinute(now.0 + u64::from(ctx.tuning.plant.repair_minutes)),
            };
            return;
        }
    }
}

/// Masa jednej sztuki części. Dla towaru sypkiego — kilogram, bo „sztuka piasku"
/// nie istnieje, a naprawa czymś sypkim i tak jest wyjątkiem.
fn masa_czesci(cat: &Catalog, good: GoodId) -> Mass {
    let m = cat.mass_of(good, magnat_core::Qty(1_000));
    if m.0 > 0 {
        m
    } else {
        Mass(1_000)
    }
}

/// Losowanie awarii. Rozkład geometryczny o średniej `mtbf * condition/100`, losowany
/// **co minutę pracy** — dzięki temu ta sama linia psuje się w tej samej minucie
/// niezależnie od tego, czy krok przyszedł po minucie, czy po godzinie.
fn awaria(
    ctx: &ProductionCtx<'_>,
    site: SiteId,
    line: usize,
    l: &ProductionLine,
    now: SimMinute,
) -> bool {
    // Klucz strumienia: zakład i linia w jednej liczbie. Osiem bitów na linię to
    // 256 linii w zakładzie — dwukrotność największej realnej rafinerii.
    let klucz = magnat_core::mix64((u64::from(site.entity().index()) << 8) | line as u64) as u32;
    let mut r = rng(
        ctx.world_seed,
        StreamId::SupplyBreakdown,
        klucz,
        Tick(now.0),
    );
    r.gen_range_u32(l.mtbf_minutes()) == 0
}

/// Próba uruchomienia szarży. Zwraca `true`, jeśli linia weszła w przezbrojenie —
/// to jedyna rzecz, którą raport liczy osobno, bo kosztuje czas i masę.
fn sprobuj_start(
    ctx: &ProductionCtx<'_>,
    store: &mut Store,
    zaklad: &mut PlantSite,
    l: &mut ProductionLine,
    now: SimMinute,
) -> bool {
    let minuta_doby = SimCalendar::from_minute(now).minute_of_day();
    let Some(zmiana) = zaklad.schedule.staffed(minuta_doby).copied() else {
        // Poza godzinami zmiany linia stoi bezczynnie. To nie jest awaria i nie ma
        // prawa zapalać `SITE_FAULT` — zakład jednozmianowy stoi dwie trzecie doby
        // i jest to normą, nie problemem.
        l.state = LineState::Idle;
        return false;
    };

    let Some(rid) = nastepna_receptura(zaklad, l, now) else {
        l.state = LineState::Idle;
        return false;
    };
    if l.recipe != Some(rid) {
        let r = ctx.cat.recipe(rid);
        przezbrojenie(ctx.cat, store, zaklad, l, r, rid, now);
        return true;
    }
    let r = ctx.cat.recipe(rid);

    if let Some(mc) = ctx.cat.machine_class_of(r) {
        if mc != l.machine_class {
            l.state = LineState::Idle;
            return false;
        }
    }
    if !zaklad.has_utility(UtilityService::Electricity) {
        l.state = LineState::Broken {
            since: now,
            cause: BreakCause::NoPower,
        };
        return false;
    }
    if r.water.0 > 0 && !zaklad.has_utility(UtilityService::Water) {
        l.state = LineState::Broken {
            since: now,
            cause: BreakCause::NoWater,
        };
        return false;
    }

    let pct = r
        .inputs
        .iter()
        .map(|i| zaklad.throttle_pct(i.good))
        .min()
        .unwrap_or(100);
    // Dwa niezależne ograniczniki szarży i **oba** muszą się zmieścić: wsad mówi,
    // ile jest z czego robić, obsada — ile jest komu. Mnożenie, a nie minimum:
    // zakład z połową ludzi i połową mąki robi ćwierć szarży, bo brakuje mu obu rzeczy.
    let wsad = crate::kernel::throughput(
        l.nominal_throughput,
        r.duration_minutes,
        pct,
        zaklad.effective_labor_pct(),
    );
    if wsad.0 <= 0 || r.batch_mass.0 <= 0 {
        l.state = LineState::Idle;
        return false;
    }

    // Wydobycie: masa wsadu pochodzi ze złoża, nie z magazynu (§5.10). Sprawdza się ją
    // w tym samym miejscu co wejścia towarowe, bo skutek jest ten sam — linia bez wsadu
    // stoi na `Starved`, zamiast udawać, że pracuje. Wyczerpane złoże jest więc
    // nieodróżnialne od braku dostawy i **ma być** nieodróżnialne: w dół łańcucha
    // kaskada niedoboru odpala się identycznie.
    if let RecipeSource::Extraction(_) = r.source {
        let starczy = zaklad
            .mining
            .is_some_and(|m| ctx.deposits.remaining(m.deposit).0 >= wsad.0);
        if !starczy {
            l.state = LineState::Starved {
                missing: r.outputs.first().map_or(GoodId(0), |o| o.good),
            };
            return false;
        }
    }

    // Wszystkie wejścia sprawdzamy, **zanim** weźmiemy którekolwiek: rezerwacja trzech
    // i porażka na czwartym zostawiłaby zjedzony wsad i nieruszoną szarżę.
    for we in &r.inputs {
        let potrzeba = skaluj(we.mass, wsad, r.batch_mass);
        let jest: i64 = zaklad
            .inputs
            .iter()
            .map(|s| store.available(*s, we.good, we.min_quality).0)
            .sum();
        if jest < potrzeba.0 {
            l.state = LineState::Starved { missing: we.good };
            return false;
        }
    }
    if let Some(pelny) = brak_miejsca(ctx.cat, store, zaklad, r, wsad) {
        l.state = LineState::Blocked { full: pelny };
        return false;
    }

    let charge = pobierz_wsad(ctx, store, zaklad, r, rid, wsad, &zmiana);
    l.state = LineState::Running {
        recipe: rid,
        started: now,
        ends: SimMinute(now.0 + u64::from(r.duration_minutes)),
        charge,
    };
    false
}

fn przezbrojenie(
    cat: &Catalog,
    store: &mut Store,
    zaklad: &mut PlantSite,
    l: &mut ProductionLine,
    r: &Recipe,
    rid: RecipeId,
    now: SimMinute,
) {
    // Złom przezbrojenia schodzi z **poprzedniej** receptury: to jej materiał zostaje
    // w maszynie. Linia nieprzezbrojona na nic nie ma czego zezłomować.
    if r.setup.scrap_mass().0 > 0 {
        if let Some(poprzednia) = l.recipe {
            zezlomuj(cat, store, zaklad, poprzednia, r.setup.scrap_mass());
        }
    }
    l.state = LineState::Setup {
        until: SimMinute(now.0 + u64::from(r.setup.minutes)),
        to_recipe: rid,
    };
}

/// Złom przezbrojenia schodzi z **pierwszego wejścia poprzedniej receptury** — to jego
/// materiał został w maszynie. Zakład, który tego wejścia już nie ma, nie złomuje niczego
/// i to jest poprawne: nie ma czego wyrzucić.
fn zezlomuj(
    cat: &Catalog,
    store: &mut Store,
    zaklad: &mut PlantSite,
    poprzednia: RecipeId,
    masa: Mass,
) {
    let Some(we) = cat.recipe(poprzednia).inputs.first() else {
        return;
    };
    for slot in zaklad.inputs.clone() {
        let zdjete = store.write_off(slot, we.good, masa, LossKind::Setup);
        if zdjete.0 > 0 {
            zaklad.losses[LossKind::Setup.as_index()] += zdjete.0;
            return;
        }
    }
}

/// Następna receptura do uruchomienia: z planu, jeśli plan coś przewiduje na teraz,
/// inaczej ta, na którą linia jest przezbrojona. Zakład bez planu po prostu produkuje
/// to, co produkował — piekarnia nie czeka na zlecenie, żeby upiec chleb.
fn nastepna_receptura(zaklad: &PlantSite, l: &ProductionLine, now: SimMinute) -> Option<RecipeId> {
    zaklad
        .schedule
        .plan
        .front()
        .filter(|p| p.earliest_start.0 <= now.0 && p.remaining.0 > 0)
        .map(|p| p.recipe)
        .or(l.recipe)
}

fn skaluj(ile: Mass, wsad: Mass, batch: Mass) -> Mass {
    if batch.0 <= 0 {
        return Mass::ZERO;
    }
    Mass((i128::from(ile.0) * i128::from(wsad.0) / i128::from(batch.0)) as i64)
}

/// Towar, dla którego zabrakło miejsca w magazynie wyjściowym, jeśli taki jest.
fn brak_miejsca(
    cat: &Catalog,
    store: &Store,
    zaklad: &PlantSite,
    r: &Recipe,
    wsad: Mass,
) -> Option<GoodId> {
    let Some(slot) = zaklad.outputs.first().copied() else {
        return r.outputs.first().map(|o| o.good);
    };
    let (mut masa, mut objetosc) = (0i64, 0i64);
    for o in r
        .outputs
        .iter()
        .filter(|o| o.kind != OutputKind::SelfConsumed)
    {
        let m = skaluj(o.mass, wsad, r.batch_mass);
        masa += m.0;
        objetosc += cat.volume_of(o.good, m).0;
    }
    let (wolna_masa, wolna_objetosc) = store.free_capacity(slot);
    if masa > wolna_masa.0 || objetosc > wolna_objetosc.0 {
        r.outputs.first().map(|o| o.good)
    } else {
        None
    }
}

/// Zabiera wejścia z magazynu i składa z nich wsad linii.
fn pobierz_wsad(
    ctx: &ProductionCtx<'_>,
    store: &mut Store,
    zaklad: &PlantSite,
    r: &Recipe,
    rid: RecipeId,
    wsad: Mass,
    zmiana: &super::line::Shift,
) -> Charge {
    if let RecipeSource::Extraction(_) = r.source {
        return wydobadz(ctx, zaklad, rid, wsad, zmiana);
    }
    let (mut masa, mut koszt) = (0i64, 0i64);
    let mut jakosc_wazona = 0i128;
    let mut najgorsze = Q::MAX;
    let mut rodzice: Vec<BatchOrigin> = Vec::new();

    for we in &r.inputs {
        let mut zostalo = skaluj(we.mass, wsad, r.batch_mass).0;
        for slot in &zaklad.inputs {
            if zostalo <= 0 {
                break;
            }
            let ile = store
                .available(*slot, we.good, we.min_quality)
                .0
                .min(zostalo);
            if ile <= 0 {
                continue;
            }
            let Some(rez) = store.reserve(*slot, we.good, Mass(ile), we.min_quality) else {
                continue;
            };
            let Ok(kawalek) = store.take(rez) else {
                continue;
            };
            masa += kawalek.mass.0;
            koszt += kawalek.cost_total.0;
            jakosc_wazona += i128::from(kawalek.quality.get()) * i128::from(kawalek.mass.0);
            najgorsze = najgorsze.min(kawalek.quality);
            rodzice.push(kawalek.origin);
            zostalo -= kawalek.mass.0;
        }
    }
    let q_in = if masa > 0 {
        Q::new((jakosc_wazona / i128::from(masa)) as u8)
    } else {
        Q::MIN
    };
    Charge {
        // **Planowana** wielkość szarży, nie suma pobranych wejść. Różnica jest realna
        // i kosztowna, gdy ją pomylić: `Recipe::batch_mass` piekarni to 165,8 kg razem
        // z wodą z licznika, a z magazynu schodzi 103,8 kg. Skalowanie wyjść sumą
        // pobranego dałoby chleb mniejszy o zawartość wody — czyli o całą różnicę,
        // dla której woda w ogóle jest w bilansie.
        mass: wsad,
        q_in,
        q_worst: if rodzice.is_empty() {
            Q::MAX
        } else {
            najgorsze
        },
        cost: Money(koszt),
        skill: zmiana.skill,
        wage_mult_pct: zmiana.wage_multiplier_pct,
        origin: BatchOrigin::from_inputs(zaklad.site, rid, &rodzice),
    }
}

/// Wsad szyby, kopalni i ujęcia wody: masa schodzi ze złoża, nie z magazynu.
///
/// Trzy rzeczy dzieją się tu i **tylko** tu, bo nigdzie indziej w M6 masa nie wchodzi
/// do gry spoza magazynu:
/// 1. `Deposits::extract` zmniejsza rezerwę po stronie M1 i zwraca to, co faktycznie
///    dał — do partii wchodzi **ta** liczba, nigdy żądana. Inaczej ostatnia szarża
///    złoża wyprodukowałaby więcej, niż w nim było.
/// 2. Koszt szarży to koszt wydobycia z [`MiningSite::cost_of`] — rośnie z głębokością,
///    ubytkiem koncentracji i kwadratowo z wyczerpaniem. To on, a nie żadna reguła,
///    zamyka szyb: w pewnym momencie import jest tańszy.
/// 3. Jakość wsadu to **efektywna** koncentracja, czyli malejąca. Najlepsze żyły idą
///    pierwsze, więc urobek z końcówki złoża jest gorszy — i widać to w produkcie.
///
/// [`MiningSite::cost_of`]: crate::MiningSite::cost_of
fn wydobadz(
    ctx: &ProductionCtx<'_>,
    zaklad: &PlantSite,
    rid: RecipeId,
    wsad: Mass,
    zmiana: &super::line::Shift,
) -> Charge {
    let Some(ms) = zaklad.mining else {
        return Charge {
            mass: Mass::ZERO,
            q_in: Q::MIN,
            q_worst: Q::MAX,
            cost: Money::ZERO,
            skill: zmiana.skill,
            wage_mult_pct: zmiana.wage_multiplier_pct,
            origin: BatchOrigin::from_inputs(zaklad.site, rid, &[]),
        };
    };
    let poczatek = ctx.deposits.initial(ms.deposit);
    let przed = ctx.deposits.remaining(ms.deposit);
    let wydobyte = ctx.deposits.extract(ms.deposit, wsad);
    let baza = Money(ctx.tuning.mining.base_gr_per_tonne);
    let koszt = ms.cost_of(baza, wydobyte, przed, poczatek);
    let q = ms.concentration_eff(przed, poczatek).clamp(0, 100) as u8;
    let mut origin = BatchOrigin::from_inputs(zaklad.site, rid, &[]);
    origin.deposit = Some(ms.deposit);
    Charge {
        // Faktycznie wydobyte, nie planowane — patrz punkt 1 w opisie.
        mass: wydobyte,
        q_in: Q::new(q),
        q_worst: Q::MAX,
        cost: koszt,
        skill: zmiana.skill,
        wage_mult_pct: zmiana.wage_multiplier_pct,
        origin,
    }
}

/// Zamknięcie szarży: koszt, jakość, wyjścia, emisje, straty.
fn zamknij_szarze(
    ctx: &ProductionCtx<'_>,
    store: &mut Store,
    zaklad: &mut PlantSite,
    l: &mut ProductionLine,
    now: SimMinute,
) -> ProductionReport {
    let mut raport = ProductionReport::default();
    let LineState::Running { recipe, charge, .. } = l.state else {
        return raport;
    };
    let r = ctx.cat.recipe(recipe);
    let wsad = charge.mass;

    // ── media szarży ────────────────────────────────────────────────────────────
    // Zniżka technologiczna (M10c `TechEffect::CostReduction`) zdejmuje część
    // rachunku za media i **tylko** jego: masa wsadu i masa wyrobu zostają, bo
    // inaczej bilans masy z 00 §6 przestałby się domykać.
    let zostaje = i64::from(10_000u16.saturating_sub(zaklad.utility_bonus_bp));
    let energia = Energy(skaluj(Mass(r.energy.0), wsad, r.batch_mass).0 * zostaje / 10_000);
    let woda = Volume(skaluj(Mass(r.water.0), wsad, r.batch_mass).0 * zostaje / 10_000);
    if let Some(m) = zaklad.meter_mut(UtilityService::Electricity) {
        m.draw_energy(WattMinutes::from_energy(energia));
    }
    if woda.0 > 0 {
        if let Some(m) = zaklad.meter_mut(UtilityService::Water) {
            m.draw_volume(woda);
        }
    }

    // ── koszt wytworzenia ───────────────────────────────────────────────────────
    //
    // Koszt mediów wchodzi w koszt wyrobu **i** naliczy się na liczniku do faktury —
    // to nie jest podwójne liczenie, tylko dwie różne księgi: pierwsza wycenia zapas
    // (COGS), druga jest wydatkiem gotówkowym. Bilans `Store` domyka się na pierwszej,
    // rachunek firmy na drugiej, i tak ma być.
    let osobominuty: i64 = r
        .labour
        .iter()
        .map(|(_, m)| skaluj(Mass(i64::from(*m)), wsad, r.batch_mass).0)
        .sum();
    let koszt_pracy =
        osobominuty * ctx.tuning.plant.wage_gr_per_person_minute * i64::from(charge.wage_mult_pct)
            / 100;
    let koszt_energii =
        i128::from(energia.0) * i128::from(ctx.tuning.utility.power_gr_per_kwh) / 1_000;
    let koszt_wody =
        i128::from(woda.0) * i128::from(ctx.tuning.utility.water_gr_per_m3) / 1_000_000;
    let narzut = skaluj(
        Mass(ctx.tuning.plant.overhead_gr_per_batch),
        wsad,
        r.batch_mass,
    )
    .0;
    let razem =
        Money(charge.cost.0 + koszt_pracy + koszt_energii as i64 + koszt_wody as i64 + narzut);

    // ── jakość ──────────────────────────────────────────────────────────────────
    let jakosc = r.quality.quality(
        charge.q_in,
        charge.q_worst,
        charge.skill,
        zaklad.tech,
        l.condition,
    );

    // ── wyjścia ─────────────────────────────────────────────────────────────────
    let podzial = allocate_cost(ctx.cat, r, razem);
    let mut kolejka = podzial.into_iter();
    let slot = zaklad.outputs.first().copied();
    for o in &r.outputs {
        match o.kind {
            // Gaz opałowy spalany na miejscu **nie tworzy partii** (§5.4). Jego masa
            // nigdy nie wchodzi do magazynu, więc nie ma jej skąd ubyć — bilans towaru
            // jest per towar i ten towar po prostu w nim nie uczestniczy.
            OutputKind::SelfConsumed => continue,
            OutputKind::Main | OutputKind::ByProduct | OutputKind::Waste => {}
        }
        let masa = skaluj(o.mass, wsad, r.batch_mass);
        if masa.0 <= 0 {
            continue;
        }
        let koszt = if o.kind == OutputKind::Waste {
            // Odpad kosztu nie dostaje — on go **generuje** (utylizacja, §5.4).
            Money::ZERO
        } else {
            kolejka.next().map_or(Money::ZERO, |(_, m)| m)
        };
        let Some(slot) = slot else { continue };
        let draft = BatchDraft {
            good: o.good,
            mass: masa,
            quality: jakosc,
            // Marka jest firmą, która to wyprodukowała (`K-79`) — od M10b partia niesie
            // ją realnie, a nie jako pole zawsze puste.
            brand: crate::batch::brand_of(zaklad.owner),
            producer: zaklad.owner,
            produced_at: now,
            cost: koszt,
            origin: charge.origin,
            flags: BatchFlags::default(),
        };
        if store.put(ctx.cat, slot, draft, MassIn::Produced).is_ok() && o.kind != OutputKind::Waste
        {
            raport.produced = Mass(raport.produced.0 + masa.0);
        }
    }

    // ── ubytek procesowy i emisje ───────────────────────────────────────────────
    //
    // Ubytek procesowy jest wielkością **receptury**, nie towaru: woda, która wyparowała
    // z chleba, nigdy nie była partią. Bilans per towar domyka się bez niego (pilnuje
    // tego walidator katalogu przy ładowaniu), więc tutaj trafia wyłącznie do histogramu
    // strat zakładu — po to, żeby panel umiał powiedzieć, gdzie schodzi masa.
    zaklad.losses[r.loss_kind.as_index()] += skaluj(r.process_loss, wsad, r.batch_mass).0;
    let e = &r.emissions;
    let pm = skaluj(Mass(i64::from(e.pm_g)), wsad, r.batch_mass).0;
    zaklad.emissions.pm_g += pm;
    zaklad.emissions.pm_g_last_minute += pm;
    zaklad.emissions.co2_g += skaluj(Mass(i64::from(e.co2_g)), wsad, r.batch_mass).0;
    zaklad.emissions.wastewater =
        Volume(zaklad.emissions.wastewater.0 + skaluj(Mass(e.wastewater_ml), wsad, r.batch_mass).0);
    zaklad.emissions.peak_noise_db = zaklad.emissions.peak_noise_db.max(e.noise_db);

    // ── plan ────────────────────────────────────────────────────────────────────
    if let Some(p) = zaklad.schedule.plan.front_mut() {
        if p.recipe == recipe {
            p.remaining = Mass((p.remaining.0 - raport.produced.0).max(0));
            if p.remaining.0 == 0 {
                zaklad.schedule.plan.pop_front();
            }
        }
    }

    l.state = LineState::Idle;
    raport.batches = 1;
    raport.cost = razem;
    raport
}
