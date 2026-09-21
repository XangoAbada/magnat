//! Planer dnia mieszkańca (M3b §5.4, WP5, PRD §5.5).
//!
//! Cztery fazy o malejącym priorytecie; **żadna faza nie usuwa slotu wstawionego przez
//! fazę wyższą**. Zobowiązania (praca, szkoła, odwożenie dzieci) są niewywłaszczalne,
//! potrzeby krytyczne wchodzą jako okna, zadania szukają luk, czas wolny bierze resztę.
//!
//! **`plan_day` jest czystą funkcją.** Nie ma stanu globalnego, nie czyta zegara
//! systemowego, a generator losowy wyprowadza z `rng(seed, StreamId::DayPlan,
//! citizen_idx, day)` — te same wejścia dają identyczne bajty `DayCanvas` (test
//! `det_plan_pure`). Dlatego karta inspekcji nie musi niczego przechowywać: woła
//! `plan_day_explained` z tym samym ziarnem i odtwarza pełne uzasadnienia.
//! Alternatywa — logowanie decyzji — to ~100 B na mieszkańca na dobę, czyli 14 GB
//! na rok gry przy 400 tys. mieszkańców.
//!
//! **Czego tu nie ma i gdzie to jest.** Ceny, budżet i funkcja użyteczności zakupu —
//! M5, za `PlaceProvider` (kryterium akceptacyjne nr 7: wymiana implementacji nie
//! zmienia tu ani jednej linii). Wybór środka transportu, korki i parkingi — M4,
//! za `TravelOracle`. Gospodarstwo domowe jako komponent — M3c; planer widzi z niego
//! `HouseholdView`, czyli zapasy i przypisane odprowadzanie dzieci, i nic więcej.

use crate::arrayvec::ArrayVec;
use crate::components::{AgentState, Employment, PlanRef};
use crate::des::{ReplanCause, REPLAN_COOLDOWN_MIN};
use crate::needs::NeedTable;
use crate::places::{
    choose_place, knowledge_key, CitizenView, KnowledgeView, PlaceCandidate, PlaceProvider,
    TravelOracle, MAX_CANDIDATES,
};
use crate::store::{PlanSlab, PlanSlot, SlabRef};
use magnat_core::{
    rng, ActivityKind, CommitmentKind, DayOfWeek, DecisionReason, HouseholdId, MinuteOfDay,
    NeedKind, PlaceRef, Rng, StockCat, StreamId, Tick, TraitId, TransportMode, Q, STOCK_CAT_COUNT,
};

mod canvas;
mod commitments;
mod rhythm;
mod tasks;

pub use canvas::{render_day_debug, DayCanvas, PlanStats};

use canvas::podsumuj;
use commitments::faza1_zobowiazania;
use rhythm::{faza2_potrzeby, faza4_czas_wolny};
use tasks::faza3_zadania;

/// Twardy limit slotów planu (§5.4). Po jego wyczerpaniu faza 4 przestaje wstawiać,
/// a faza 3 pomija zadania o najniższej pilności z powodem `SlotBudgetExhausted`.
pub const MAX_SLOTS: usize = PlanRef::MAX_SLOTS;

// ── widok gospodarstwa ──────────────────────────────────────────────────────────

/// Wycinek gospodarstwa domowego widoczny dla planera.
///
/// Widok, a nie komponent: `Household` powstaje w M3c (§5.6), a planer w M3b i nie
/// ma powodu na siebie czekać. Ta sama sztuczka co z `CitizenView` — kontrakt opisuje
/// **co planer czyta**, a nie z jakiej struktury to pochodzi. M3c wypełnia go
/// z `Household.stock` i `HouseholdRoles`, test — ręcznie.
#[derive(Clone, Copy, Debug)]
pub struct HouseholdView<'a> {
    pub id: HouseholdId,
    /// Dni zapasu per `StockCat` (M3c §5.6). W M3 abstrakcyjne; M5 zastąpi realnym
    /// towarem, a próg wyzwalający zakupy zostanie ten sam.
    pub stock: &'a [u8; STOCK_CAT_COUNT],
    /// Szkoły dzieci, które **rano odprowadza** ten mieszkaniec (podział ról w GD).
    pub escorts: &'a [PlaceRef],
    /// Szkoły dzieci, które **po południu odbiera** ten mieszkaniec (korekta C-8).
    ///
    /// Osobna lista, bo to zwykle inna osoba: szkoła kończy się o 14:00, a zmiana
    /// dzienna o 16:00, więc odbiera ten, kto kończy wcześniej (`household::roles`).
    /// Dla jedynego dorosłego w gospodarstwie obie listy są takie same.
    pub pickups: &'a [PlaceRef],
    /// Ile dzieci w gospodarstwie wymagało odprowadzenia i go **nie dostało**
    /// (`R2-WP3`): piąte ponad `MAX_ESCORTED` albo wszystkie, gdy w domu nie ma
    /// dorosłego. Planer zamienia to na `DecisionReason::EscortUnavailable`,
    /// bo inaczej stan nie zostawia śladu nigdzie.
    pub unescorted: u8,
}

impl HouseholdView<'_> {
    /// Zapas pełny — gospodarstwo bez braków. Wartość do testów i do scenariuszy,
    /// w których zapasów jeszcze nikt nie prowadzi.
    pub const FULL: [u8; STOCK_CAT_COUNT] = [30; STOCK_CAT_COUNT];

    /// Widok gospodarstwa bez dzieci do odprowadzenia i z pełnym zapasem — punkt
    /// wyjścia dla testów i scenariuszy, które gospodarstw jeszcze nie prowadzą.
    #[must_use]
    pub fn empty(id: HouseholdId, stock: &[u8; STOCK_CAT_COUNT]) -> HouseholdView<'_> {
        HouseholdView {
            id,
            stock,
            escorts: &[],
            pickups: &[],
            unescorted: 0,
        }
    }
}

// ── kontekst planowania ─────────────────────────────────────────────────────────

/// Wszystko, czego planer potrzebuje, i nic ponadto.
///
/// `seed` zamiast gotowego generatora (odchylenie od szkicu §5.4, korekta B-11):
/// `plan_day` bierze `&PlanCtx`, więc nie mógłby przesuwać stanu generatora trzymanego
/// w kontekście. Wyprowadzenie go w środku z `(seed, StreamId::DayPlan, citizen, day)`
/// jest tym, czego wymaga 00 §3.1 — i dopiero ono czyni funkcję czystą.
pub struct PlanCtx<'a> {
    pub seed: u64,
    /// Absolutny numer doby świata; rok = 360 dni (K-1).
    pub day: u64,
    /// Z `SimCalendar` (K-15) — **nigdy** liczony lokalnie z dnia miesiąca.
    pub dow: DayOfWeek,
    pub citizen: CitizenView<'a>,
    pub household: HouseholdView<'a>,
    pub employment: &'a Employment,
    pub known: KnowledgeView<'a>,
    pub needs: &'a NeedTable,
    pub home: PlaceRef,
    pub work: Option<PlaceRef>,
    pub school: Option<PlaceRef>,
    pub places: &'a dyn PlaceProvider,
    pub travel: &'a dyn TravelOracle,
    /// Zasięg osobisty zadania, w minutach marszu.
    pub max_task_travel_min: u16,
}

impl PlanCtx<'_> {
    fn rng_for(&self, minute: u16) -> Rng {
        rng(
            self.seed,
            StreamId::DayPlan,
            self.citizen.id.entity().index(),
            Tick(self.day * 1440 + u64::from(minute)),
        )
    }
}

/// Wpis pełnego uzasadnienia. `slot` wskazuje slot planu albo `ReasonLog::NO_SLOT`
/// dla decyzji, która **nie** wytworzyła slotu (pominięte zadanie, absencja).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ReasonEntry {
    pub slot: u8,
    pub reason: DecisionReason,
}

/// Pełne uzasadnienia planu — **odtwarzane**, nie przechowywane (§5.4).
#[derive(Clone, Debug, Default)]
pub struct ReasonLog {
    entries: Vec<ReasonEntry>,
}

impl ReasonLog {
    pub const NO_SLOT: u8 = 0xFF;

    #[must_use]
    pub fn new() -> ReasonLog {
        ReasonLog::default()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    #[must_use]
    pub fn entries(&self) -> &[ReasonEntry] {
        &self.entries
    }

    /// Powody przypisane slotowi o danym indeksie — pierwszy jest powodem wstawienia,
    /// kolejne to odrzucone alternatywy.
    pub fn for_slot(&self, slot: usize) -> impl Iterator<Item = &ReasonEntry> {
        self.entries.iter().filter(move |e| e.slot as usize == slot)
    }

    /// Decyzje, które nie wytworzyły slotu: pominięte zadania, absencja, brak wiedzy.
    pub fn skipped(&self) -> impl Iterator<Item = &ReasonEntry> {
        self.entries.iter().filter(|e| e.slot == ReasonLog::NO_SLOT)
    }
}

/// Ujście uzasadnień. `plan_day` dostaje puste, `plan_day_explained` — prawdziwe;
/// obie ścieżki wykonują **ten sam** kod, więc nie mają jak się rozjechać
/// (test `plan_explain_matches`).
struct Log<'a>(Option<&'a mut ReasonLog>);

impl Log<'_> {
    #[inline]
    fn add(&mut self, slot: u8, reason: DecisionReason) {
        if let Some(l) = self.0.as_mut() {
            l.entries.push(ReasonEntry { slot, reason });
        }
    }

    /// Wstawienie slotu w środek planu przesuwa indeksy slotów późniejszych — i musi
    /// przesunąć też ich uzasadnienia. Bez tego karta inspekcji podpisuje pracę
    /// powodem snu, a wykrywa się to dopiero okiem na wydruku.
    fn shift_from(&mut self, i: u8) {
        if let Some(l) = self.0.as_mut() {
            for e in &mut l.entries {
                if e.slot != ReasonLog::NO_SLOT && e.slot >= i {
                    e.slot += 1;
                }
            }
        }
    }

    /// Usunięcie slotu: jego uzasadnienia znikają, późniejsze przesuwają się w dół.
    fn drop_slot(&mut self, i: u8) {
        if let Some(l) = self.0.as_mut() {
            l.entries.retain(|e| e.slot != i);
            for e in &mut l.entries {
                if e.slot != ReasonLog::NO_SLOT && e.slot > i {
                    e.slot -= 1;
                }
            }
        }
    }

    /// Powód decyzji, która nie wytworzyła slotu.
    #[inline]
    fn skip(&mut self, reason: DecisionReason) {
        self.add(ReasonLog::NO_SLOT, reason);
    }
}

/// Czysta funkcja: te same wejścia → identyczne wyjście, bez stanu globalnego.
pub fn plan_day(ctx: &PlanCtx<'_>, out: &mut DayCanvas) -> PlanStats {
    uloz(ctx, out, Log(None))
}

/// Ten sam algorytm, dodatkowo zapisujący pełne uzasadnienia. Używane przez UI i testy.
pub fn plan_day_explained(
    ctx: &PlanCtx<'_>,
    out: &mut DayCanvas,
    log: &mut ReasonLog,
) -> PlanStats {
    log.clear();
    uloz(ctx, out, Log(Some(log)))
}

fn uloz(ctx: &PlanCtx<'_>, out: &mut DayCanvas, mut log: Log<'_>) -> PlanStats {
    out.clear();
    let mut stats = PlanStats::default();
    faza1_zobowiazania(ctx, out, &mut log, &mut stats);
    faza2_potrzeby(ctx, out, &mut log);
    faza3_zadania(ctx, out, &mut log, &mut stats, 0);
    faza4_czas_wolny(ctx, out, &mut log, 0);
    podsumuj(out, &mut stats);
    stats
}

// ── przeplanowanie ──────────────────────────────────────────────────────────────

/// Przeplanowanie od minuty `from`.
///
/// **Inkrementalne, nie od zera** (§5.2): sloty już rozpoczęte, zobowiązania i potrzeby
/// krytyczne zostają; przeliczane są wyłącznie zadania i czas wolny. Pełne przeplanowanie
/// tylko wtedy, gdy padło samo zobowiązanie (`ReplanCause::is_full_replan`).
pub fn replan(
    ctx: &PlanCtx<'_>,
    from: MinuteOfDay,
    cause: ReplanCause,
    canvas: &mut DayCanvas,
) -> PlanStats {
    replan_explained(ctx, from, cause, canvas, &mut Log(None))
}

/// Jak `replan`, ale z pełnym uzasadnieniem.
pub fn replan_explained_into(
    ctx: &PlanCtx<'_>,
    from: MinuteOfDay,
    cause: ReplanCause,
    canvas: &mut DayCanvas,
    log: &mut ReasonLog,
) -> PlanStats {
    log.clear();
    replan_explained(ctx, from, cause, canvas, &mut Log(Some(log)))
}

fn replan_explained(
    ctx: &PlanCtx<'_>,
    from: MinuteOfDay,
    cause: ReplanCause,
    canvas: &mut DayCanvas,
    log: &mut Log<'_>,
) -> PlanStats {
    if cause.is_full_replan() {
        let mut stats = uloz(ctx, canvas, Log(log.0.take()));
        stats.slots = canvas.len() as u8;
        log.add(
            ReasonLog::NO_SLOT,
            DecisionReason::Replanned {
                cause_tag: cause.tag(),
                slots_changed: stats.slots,
            },
        );
        return stats;
    }

    let t = from.get();
    let przed = canvas.len() as u8;
    // Zostaje: co się już zaczęło, zobowiązania (faza 1) i potrzeby krytyczne (faza 2).
    // Znika: zadania i czas wolny od `from` w przód — i tylko one.
    let zachowaj = |s: &PlanSlot| {
        let tag = u16::from(s.reason_tag());
        s.start_min < t
            || tag
                == DecisionReason::Commitment {
                    kind: CommitmentKind::Work,
                }
                .discriminant()
            || tag
                == DecisionReason::NeedCritical {
                    need: NeedKind::Sleep,
                    level: Q::MIN,
                }
                .discriminant()
    };
    let mut ocalale = DayCanvas::new();
    for (i, s) in canvas.slots().iter().enumerate() {
        if zachowaj(s) {
            ocalale.insert(*s, canvas.place_of(i));
        }
    }
    *canvas = ocalale;

    let zobowiazania = canvas
        .slots()
        .iter()
        .filter(|s| {
            u16::from(s.reason_tag())
                == DecisionReason::Commitment {
                    kind: CommitmentKind::Work,
                }
                .discriminant()
        })
        .count() as u8;
    let mut stats = PlanStats {
        commitments: zobowiazania,
        ..PlanStats::default()
    };
    faza3_zadania(ctx, canvas, log, &mut stats, t);
    faza4_czas_wolny(ctx, canvas, log, t);
    podsumuj(canvas, &mut stats);
    log.add(
        ReasonLog::NO_SLOT,
        DecisionReason::Replanned {
            cause_tag: cause.tag(),
            slots_changed: stats.slots.abs_diff(przed),
        },
    );
    stats
}

/// Zgłoszenie przeplanowania z debouncingiem (§5.2, ryzyko R3).
///
/// Zwraca `true` najwyżej raz na `REPLAN_COOLDOWN_MIN` minut gry — lawina zdarzeń
/// miejskich uderzająca w setki tysięcy agentów nie ma jak przełożyć się na setki
/// tysięcy przeplanowań w tym samym ticku.
pub fn request_replan(state: &mut AgentState) -> bool {
    if state.replan_cooldown > 0 {
        return false;
    }
    state.replan_cooldown = REPLAN_COOLDOWN_MIN;
    true
}

/// Odlicza minutę debouncingu. Wołane raz na tick minutowy przez system agentów.
pub fn tick_replan_cooldown(state: &mut AgentState) {
    state.replan_cooldown = state.replan_cooldown.saturating_sub(1);
}

// ── zapis planu ─────────────────────────────────────────────────────────────────

/// Zapisuje plan do slabu i aktualizuje uchwyt mieszkańca.
pub fn store_plan(canvas: &DayCanvas, plan: &mut PlanRef, slab: &mut PlanSlab, day: u64) {
    let mut r: SlabRef = plan.slab_ref();
    slab.store(&mut r, canvas.slots());
    plan.set_slab_ref(r);
    plan.cursor = 0;
    plan.plan_day = (day % 65_536) as u16;
}

/// Odczytuje zapisany plan.
#[must_use]
pub fn load_plan<'a>(plan: &PlanRef, slab: &'a PlanSlab) -> &'a [PlanSlot] {
    slab.entries(plan.slab_ref())
}
