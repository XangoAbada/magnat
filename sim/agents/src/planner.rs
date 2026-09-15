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

/// Twardy limit slotów planu (§5.4). Po jego wyczerpaniu faza 4 przestaje wstawiać,
/// a faza 3 pomija zadania o najniższej pilności z powodem `SlotBudgetExhausted`.
pub const MAX_SLOTS: usize = PlanRef::MAX_SLOTS;

/// Minut między pobudką a wyjściem z domu (mycie, śniadanie, zbieranie się).
const PREP_MIN: u16 = 50;

/// Zapas gospodarstwa, poniżej którego planer wstawia zakupy — w dniach.
const STOCK_THRESHOLD_DAYS: u8 = 2;

/// Kara za minutę nadłożonej drogi przy wyborze miejsca zadania. Trzy, bo minuta
/// nadłożona boli bardziej niż minuta dojścia — idzie się nią „dodatkowo".
const W_DETOUR: i32 = 3;

/// Luka krótsza niż tyle nie dostaje zajęcia, tylko wypełniacz.
const LEISURE_MIN_GAP: u16 = 30;

/// Przekazanie dziecka w szkole.
const ESCORT_HANDOVER_MIN: u16 = 10;

/// Okno szkolne. `ponytail:` stała w kodzie, nie dane — właścicielem szkoły jako
/// instytucji z pojemnością i grafikiem jest M8 (decyzja 9.15), a epoki wchodzą
/// z `data/epochs/` w M3d. Do tego czasu jedno okno dla wszystkich klas.
const SCHOOL_OPEN: u16 = 8 * 60;
const SCHOOL_CLOSE: u16 = 14 * 60;

/// Poniżej tego poziomu potrzeba wywołuje własny slot w fazie 2 albo zadanie w fazie 3.
const HYGIENE_TRIGGER: u8 = 40;
const HEALTH_TRIGGER: u8 = 30;
const CLOTHING_TRIGGER: u8 = 30;

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

// ── plan dnia ───────────────────────────────────────────────────────────────────

/// Wolny przedział doby.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct Gap {
    start: u16,
    end: u16,
}

impl Gap {
    #[inline]
    fn len(self) -> u16 {
        self.end.saturating_sub(self.start)
    }
}

/// Plan doby: sloty posortowane po `start_min`, bez nakładania, bez przechodzenia
/// przez północ (doba kończy się snem i zaczyna nowym planem).
///
/// `ponytail:` wstawianie liniowe zamiast drzewa przedziałów — przy 24 slotach całość
/// to najwyżej ~300 porównań na kilku liniach cache. Drzewo dopiero, gdyby limit slotów
/// przekroczył ~64.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DayCanvas {
    slots: ArrayVec<PlanSlot, MAX_SLOTS>,
    /// Cel slotu jako `PlaceRef`, w lockstepie ze `slots`. W zapisie zostaje sam
    /// `PlanSlot.target` (indeks encji) — to jest kopia robocza planera, żeby faza 3
    /// wiedziała, **gdzie** mieszkaniec jest na początku luki.
    places: ArrayVec<PlaceRef, MAX_SLOTS>,
}

impl Default for DayCanvas {
    fn default() -> Self {
        DayCanvas::new()
    }
}

impl DayCanvas {
    #[must_use]
    pub fn new() -> DayCanvas {
        DayCanvas {
            slots: ArrayVec::new(),
            places: ArrayVec::new(),
        }
    }

    #[must_use]
    pub fn slots(&self) -> &[PlanSlot] {
        self.slots.as_slice()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    #[must_use]
    pub fn is_full(&self) -> bool {
        self.slots.is_full()
    }

    /// Wczytuje zapisany plan z areny z powrotem na kanwę — wejście przeplanowania
    /// i karty inspekcji. Cele slotów odtwarza się z ich kluczy, bo arena trzyma
    /// sam indeks encji (§5.1).
    pub fn load(&mut self, slots: &[PlanSlot]) {
        self.slots.clear();
        self.places.clear();
        for s in slots.iter().take(MAX_SLOTS) {
            self.slots.push(*s);
            self.places
                .push(crate::places::place_from_key(s.target).unwrap_or_default());
        }
    }

    pub fn clear(&mut self) {
        self.slots.clear();
        self.places.clear();
    }

    /// Miejsce slotu o podanym indeksie.
    #[must_use]
    pub fn place_of(&self, i: usize) -> PlaceRef {
        self.places[i]
    }

    /// Wstawia slot z zachowaniem porządku. Zwraca `false`, gdy plan jest pełny albo
    /// slot nachodziłby na już wstawiony — **cisza zamiast paniki jest tu celowa**:
    /// „nie zmieściło się" to normalny wynik planowania, a nie błąd programu.
    fn insert(&mut self, s: PlanSlot, at: PlaceRef) -> Option<usize> {
        if s.dur_min == 0 || u32::from(s.start_min) + u32::from(s.dur_min) > 1440 {
            return None;
        }
        if self.slots.is_full() {
            return None;
        }
        let koniec = s.end_min();
        let mut poz = self.slots.len();
        for (i, inny) in self.slots.iter().enumerate() {
            if inny.start_min < koniec && s.start_min < inny.end_min() {
                return None; // nakładanie
            }
            if inny.start_min >= koniec {
                poz = i;
                break;
            }
        }
        self.slots.insert(poz, s);
        self.places.insert(poz, at);
        Some(poz)
    }

    fn remove(&mut self, i: usize) {
        self.slots.remove(i);
        self.places.remove(i);
    }

    /// Skąd mieszkaniec wyrusza w slocie `i`: z celu slotu poprzedniego albo z domu.
    fn origin_of(&self, i: usize, home: PlaceRef) -> PlaceRef {
        if i == 0 {
            home
        } else {
            self.places[i - 1]
        }
    }

    /// Wolne przedziały doby od minuty `from`, w kolejności czasu.
    fn gaps(&self, from: u16, out: &mut ArrayVec<Gap, 32>) {
        out.clear();
        let mut kursor = from;
        for s in self.slots.iter() {
            if s.end_min() <= kursor {
                continue;
            }
            if s.start_min > kursor {
                out.push(Gap {
                    start: kursor,
                    end: s.start_min,
                });
            }
            kursor = kursor.max(s.end_min());
        }
        if kursor < 1440 {
            out.push(Gap {
                start: kursor,
                end: 1440,
            });
        }
    }

    /// Gdzie mieszkaniec jest w minucie `m`: cel slotu trwającego w tej minucie albo
    /// ostatniego zakończonego przed nią. Brak takiego slotu = dom.
    fn place_at(&self, m: u16, home: PlaceRef) -> PlaceRef {
        let mut gdzie = home;
        for (i, s) in self.slots.iter().enumerate() {
            if s.start_min > m {
                break;
            }
            // Slot dojścia kończy się tam, dokąd prowadzi — po nim mieszkaniec jest
            // w celu, a nie w punkcie startowym.
            gdzie = self.places[i];
        }
        gdzie
    }

    /// Slot zaczynający się dokładnie w minucie `m` (cel, do którego trzeba zdążyć).
    fn slot_starting_at(&self, m: u16) -> Option<usize> {
        self.slots.iter().position(|s| s.start_min == m)
    }

    /// Gdzie mieszkaniec **musi być** w minucie `m`, żeby wykonać to, co się wtedy
    /// zaczyna.
    ///
    /// Dla dojazdu to jego **punkt wyjścia**, nie cel: `place_of` slotu dojazdu mówi,
    /// dokąd on prowadzi, a przed nim trzeba stać na jego początku. Pomylenie tych
    /// dwóch rzeczy kazało planerowi wysyłać mieszkańca po zakupy z powrotem do pracy,
    /// a potem liczyć dojazd do domu tak, jakby wychodził z domu — czas dojścia
    /// w planie przestawał się zgadzać z czasem policzonym przy wyruszeniu.
    fn required_place_at(&self, m: u16, home: PlaceRef) -> Option<PlaceRef> {
        let j = self.slot_starting_at(m)?;
        if self.slots[j].kind == ActivityKind::Commute as u8 {
            Some(self.origin_of(j, home))
        } else {
            Some(self.places[j])
        }
    }

    fn first_start(&self) -> Option<u16> {
        self.slots.first().map(|s| s.start_min)
    }
}

/// Statystyka planu — wejście do histogramu wykorzystania doby i do testów.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct PlanStats {
    pub slots: u8,
    pub commitments: u8,
    pub tasks_placed: u8,
    pub tasks_dropped: u8,
    pub travel_min: u16,
    pub free_min: u16,
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

fn podsumuj(canvas: &DayCanvas, stats: &mut PlanStats) {
    stats.slots = canvas.len() as u8;
    stats.travel_min = canvas
        .slots()
        .iter()
        .filter(|s| s.kind == ActivityKind::Commute as u8)
        .map(|s| s.dur_min)
        .sum();
    let zajete: u32 = canvas.slots().iter().map(|s| u32::from(s.dur_min)).sum();
    stats.free_min = (1440 - zajete.min(1440)) as u16;
}

// ── faza 1: zobowiązania stałe ──────────────────────────────────────────────────

fn faza1_zobowiazania(
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

// ── faza 2: potrzeby krytyczne jako okna ────────────────────────────────────────

fn faza2_potrzeby(ctx: &PlanCtx<'_>, canvas: &mut DayCanvas, log: &mut Log<'_>) {
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
    let powod_snu = DecisionReason::NeedCritical {
        need: NeedKind::Sleep,
        level: poziom_snu,
    };
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
            DecisionReason::NeedCritical {
                need: NeedKind::Hygiene,
                level: higiena,
            },
            NeedKind::Hygiene.as_index() as u8,
        );
    }

    let glod = ctx.citizen.needs.get(NeedKind::Hunger);
    let posilek = ctx.needs.spec(NeedKind::Hunger).visit_min;
    let powod_jedzenia = DecisionReason::NeedCritical {
        need: NeedKind::Hunger,
        level: glod,
    };
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
        log.skip(DecisionReason::NoTimeWindow {
            need: NeedKind::Hunger,
            needed_min: posilek,
            longest_gap_min: najdluzsza,
        });
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
            powod: DecisionReason::NeedCritical {
                need: NeedKind::Health,
                level: zdrowie,
            },
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
        let powod = DecisionReason::StockBelowThreshold {
            cat: *cat,
            days_left: dni,
        };
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
            powod: DecisionReason::NeedCritical {
                need: NeedKind::Clothing,
                level: ubranie,
            },
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

fn faza3_zadania(
    ctx: &PlanCtx<'_>,
    canvas: &mut DayCanvas,
    log: &mut Log<'_>,
    stats: &mut PlanStats,
    od: u16,
) {
    let mut kandydaci: ArrayVec<PlaceCandidate, MAX_CANDIDATES> = ArrayVec::new();
    for zad in lista_zadan(ctx).iter() {
        if canvas.len() + 3 > MAX_SLOTS {
            log.skip(DecisionReason::SlotBudgetExhausted { dropped: zad.need });
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
                        DecisionReason::Commitment {
                            kind: CommitmentKind::Commute,
                        },
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
        None => Err(brak_wiedzy.unwrap_or(DecisionReason::NoTimeWindow {
            need: zad.need,
            needed_min: wizyta,
            longest_gap_min: najdluzsza,
        })),
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
        *brak_wiedzy = Some(DecisionReason::PlaceUnknown {
            need: zad.need,
            known_count: ctx.known.len().min(255) as u8,
        });
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
            log.skip(DecisionReason::PlaceClosed {
                place: kand.place,
                opens_at: godziny.open,
            });
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
                DecisionReason::ChosenOnRoute {
                    detour_min: nadlozenie,
                    direct_min: wprost,
                },
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

// ── faza 4: czas wolny ──────────────────────────────────────────────────────────

fn faza4_czas_wolny(ctx: &PlanCtx<'_>, canvas: &mut DayCanvas, log: &mut Log<'_>, od: u16) {
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
                DecisionReason::FreeTimePreference {
                    trait_id: cecha,
                    weight: waga,
                },
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

// ── wstawianie slotu ────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn wstaw(
    canvas: &mut DayCanvas,
    log: &mut Log<'_>,
    kind: ActivityKind,
    start: u16,
    dur: u16,
    cel: PlaceRef,
    powod: DecisionReason,
    param: u8,
) -> bool {
    let tag = powod.discriminant();
    debug_assert!(
        tag <= 255,
        "PlanSlot.reason pakuje tag w bajt (M3a §5.1); powód {tag} tego nie mieści"
    );
    let slot = PlanSlot {
        start_min: start,
        dur_min: dur,
        target: knowledge_key(cel).unwrap_or(PlanSlot::NO_TARGET),
        kind: kind as u8,
        mode: if kind == ActivityKind::Commute {
            TransportMode::Walk as u8
        } else {
            u8::MAX
        },
        reason: PlanSlot::pack_reason(tag as u8, param),
    };
    let Some(i) = canvas.insert(slot, cel) else {
        return false;
    };
    log.shift_from(i as u8);
    log.add(i as u8, powod);
    true
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

// ── wydruk diagnostyczny ────────────────────────────────────────────────────────

/// Wydruk planu doby w formie tekstowej — **artefakt diagnostyczny, nie interfejs
/// gracza.** Identyfikatory są angielskie, bo to kod, a nie tekst do przetłumaczenia.
///
/// Player-facing oś czasu po polsku i po angielsku buduje M3d
/// (`engine::ui::inspect::timeline`) nad **tymi samymi** danymi: `DayCanvas` plus
/// `ReasonLog`. Nie ma tu drugiego planera ani drugiego źródła uzasadnień — jest drugi
/// formatter, z których jeden idzie do złotego testu i do runnera headless, a drugi
/// przez `LocKey` do karty inspekcji.
///
/// Powód renderuje się przez `Debug` `DecisionReason`, a nie przez własny `match`:
/// wydruk ma być stabilny i wyczerpujący, a wariant dołożony przez M4 ma się w nim
/// pojawić bez dopisywania czegokolwiek tutaj.
#[must_use]
pub fn render_day_debug(ctx: &PlanCtx<'_>, canvas: &DayCanvas, log: &ReasonLog) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(128 * (canvas.len() + 4));
    let _ = writeln!(
        s,
        "citizen {} | day {} | {:?} | age {}",
        ctx.citizen.id.entity().index(),
        ctx.day,
        ctx.dow,
        ctx.citizen.identity.age_years(ctx.citizen.today),
    );
    for (i, slot) in canvas.slots().iter().enumerate() {
        let cel = match canvas.place_of(i) {
            p if p == ctx.home => "home".to_string(),
            p => match knowledge_key(p) {
                Some(k) => format!("#{k}"),
                None => "-".to_string(),
            },
        };
        let powody: Vec<String> = log.for_slot(i).map(|e| format!("{:?}", e.reason)).collect();
        let powod = if powody.is_empty() {
            format!("tag={} param={}", slot.reason_tag(), slot.reason_param())
        } else {
            powody.join(" | ")
        };
        let _ = writeln!(
            s,
            "{}-{} {:<8} {:<8} {}",
            MinuteOfDay::new(slot.start_min),
            MinuteOfDay::new(slot.end_min() % 1440),
            ActivityKind::from_index(slot.kind as usize).map_or("?", ActivityKind::name),
            cel,
            powod,
        );
    }
    for e in log.skipped() {
        let _ = writeln!(s, "skipped: {:?}", e.reason);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luki_wypelniaja_dobe_bez_dziur_i_bez_nakladania() {
        let mut c = DayCanvas::new();
        let slot = |start: u16, dur: u16| PlanSlot {
            start_min: start,
            dur_min: dur,
            ..PlanSlot::default()
        };
        assert_eq!(c.insert(slot(480, 60), PlaceRef::default()), Some(0));
        assert_eq!(c.insert(slot(0, 400), PlaceRef::default()), Some(0));
        assert_eq!(
            c.insert(slot(390, 100), PlaceRef::default()),
            None,
            "nakładanie przeszło"
        );
        assert_eq!(
            c.insert(slot(1400, 100), PlaceRef::default()),
            None,
            "slot za północ"
        );

        let mut luki: ArrayVec<Gap, 32> = ArrayVec::new();
        c.gaps(0, &mut luki);
        assert_eq!(
            luki.as_slice(),
            &[
                Gap {
                    start: 400,
                    end: 480
                },
                Gap {
                    start: 540,
                    end: 1440
                }
            ]
        );
        let suma: u32 = c.slots().iter().map(|s| u32::from(s.dur_min)).sum::<u32>()
            + luki.iter().map(|g| u32::from(g.len())).sum::<u32>();
        assert_eq!(suma, 1440, "sloty plus luki nie składają się na dobę");
    }
}
