//! Punkty rozszerzenia dla M4 i M5 (M3a §5.3, WP4).
//!
//! **To jest kontrakt, nie szkic.** Planer (M3b) zależy wyłącznie od dwóch traitów
//! z tego modułu. Żadna sygnatura nie wspomina o cenie, `GoodId`, magazynie, pojeździe
//! ani grafie nawigacyjnym — dlatego M5 podmienia `InfinitePlaces` na indeks ofert,
//! a M4 `WalkOracle` na `engine/nav`, i planer się nie zmienia. Kryterium akceptacyjne
//! nr 7 fazy mówi to wprost: wymiana implementacji nie może wymagać zmiany ani jednej
//! linii w `planner.rs`.
//!
//! Ścieżka odmowy (`FulfilOutcome::Refused`) jest zbudowana i przetestowana **już tutaj**,
//! atrapą `FlakyPlaces`, mimo że produkcyjna `InfinitePlaces` nigdy nie odmawia. Dzięki
//! temu M5 włącza istniejącą ścieżkę sterowania, zamiast dokładać nową.

use crate::arrayvec::ArrayVec;
use crate::components::{Identity, Needs, Personality, Residence, Vitals};
use crate::needs::NeedTable;
use crate::store::Knowledge;
use magnat_core::{
    CitizenId, DayOfWeek, DecisionReason, HouseholdId, MinuteOfDay, Money, NeedKind, PlaceKind,
    PlaceRef, SimMinute, TransportMode, WorldCoord, Q,
};
use magnat_spatial::{Aabb2, CategoryGrid, GridSpec, Vec2};

/// Twardy limit kandydatów (§5.3, ryzyko R6). Nie jest orientacyjny: to on trzyma
/// planer w budżecie 10 µs także wtedy, gdy M5 wstawi do `candidates` funkcję
/// użyteczności §6.4.
pub const MAX_CANDIDATES: usize = 16;

/// Limit miejsc widocznych z trasy (§5.7).
pub const MAX_ON_ROUTE: usize = 8;

// ── katalog miejsc ──────────────────────────────────────────────────────────────

/// Miejsce w świecie: czym jest i gdzie stoi.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PlaceEntry {
    pub place: PlaceRef,
    pub kind: PlaceKind,
    pub at: WorldCoord,
}

/// Katalog miejsc z indeksem przestrzennym per rodzaj.
///
/// Miejsca wsypuje tu faza, która je zna: w M3d generator populacji z budynków i zakładów
/// M2, w testach — ręcznie. `sim/agents` **nie zależy od `sim/world`**, bo zależność idzie
/// w drugą stronę: to `sim/world` rozszerza się o populację (decyzja 9.11).
#[derive(Clone, Debug)]
pub struct PlaceTable {
    /// Posortowane po `PlaceRef` — wyszukiwanie binarne zamiast mapy, bo katalog
    /// powstaje raz i potem tylko się go czyta.
    entries: Vec<PlaceEntry>,
    index: CategoryGrid<PlaceKind, u32>,
}

impl PlaceTable {
    /// Rozmiar komórki indeksu. 150 m to około dwóch kwartałów — przy zapytaniach
    /// o promieniu 400–2000 m daje kilka komórek na zapytanie.
    pub const CELL_M: u16 = 150;

    #[must_use]
    pub fn build(mut entries: Vec<PlaceEntry>) -> PlaceTable {
        entries.sort_unstable_by_key(|e| e.place);
        entries.dedup_by_key(|e| e.place);

        let spec = if entries.is_empty() {
            GridSpec::new(Vec2::new(0.0, 0.0), PlaceTable::CELL_M, 1, 1)
        } else {
            let mut min = Vec2::new(f32::MAX, f32::MAX);
            let mut max = Vec2::new(f32::MIN, f32::MIN);
            for e in &entries {
                let p = coord_to_vec2(e.at);
                min = Vec2::new(min.x.min(p.x), min.y.min(p.y));
                max = Vec2::new(max.x.max(p.x), max.y.max(p.y));
            }
            GridSpec::covering(Aabb2::new(min, max), PlaceTable::CELL_M)
        };

        let index = CategoryGrid::build(
            spec,
            entries
                .iter()
                .enumerate()
                .map(|(i, e)| (e.kind, coord_to_vec2(e.at), i as u32)),
        );
        PlaceTable { entries, index }
    }

    #[must_use]
    pub fn get(&self, place: PlaceRef) -> Option<&PlaceEntry> {
        self.entries
            .binary_search_by_key(&place, |e| e.place)
            .ok()
            .map(|i| &self.entries[i])
    }

    /// Współrzędna miejsca. `PlaceRef::Coord` jest swoją własną współrzędną — dzięki
    /// temu planer może kotwiczyć się na punkcie, który nie jest żadnym budynkiem.
    #[must_use]
    pub fn coord_of(&self, place: PlaceRef) -> Option<WorldCoord> {
        match place {
            PlaceRef::Coord(c) => Some(c),
            other => self.get(other).map(|e| e.at),
        }
    }

    pub fn for_each_near(
        &self,
        kind: PlaceKind,
        at: WorldCoord,
        radius_m: f32,
        mut f: impl FnMut(&PlaceEntry),
    ) {
        self.index
            .for_each_in_radius(kind, coord_to_vec2(at), radius_m, |_, i| {
                f(&self.entries[i as usize]);
            });
    }

    #[must_use]
    pub fn entries(&self) -> &[PlaceEntry] {
        &self.entries
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Współrzędne świata są w centymetrach (`WorldCoord`), indeks przestrzenny w metrach.
/// Jedna funkcja na całą konwersję, żeby dzielenie przez 100 nie rozlazło się po kodzie.
#[inline]
#[must_use]
pub fn coord_to_vec2(c: WorldCoord) -> Vec2 {
    Vec2::new(c.x as f32 / 100.0, c.y as f32 / 100.0)
}

// ── widoki ──────────────────────────────────────────────────────────────────────

/// Wycinek stanu mieszkańca widoczny dla implementacji traitów. Referencje, nie kopie:
/// `estimate` bywa wołane setki tysięcy razy na dobę gry.
#[derive(Clone, Copy, Debug)]
pub struct CitizenView<'a> {
    pub id: CitizenId,
    pub identity: &'a Identity,
    pub vitals: &'a Vitals,
    pub needs: &'a Needs,
    pub personality: &'a Personality,
    pub residence: &'a Residence,
    /// Doba świata — wiek liczy się z niej, a nie z zegara systemowego (00 §3.5).
    pub today: i32,
}

/// Wiedza mieszkańca o miejscach (§5.7). Kandydatem może być **wyłącznie** miejsce,
/// które mieszkaniec zna — nowy sklep zaczyna od przechodniów, a nie od całego miasta.
#[derive(Clone, Copy, Debug)]
pub struct KnowledgeView<'a> {
    entries: &'a [Knowledge],
}

impl<'a> KnowledgeView<'a> {
    #[must_use]
    pub fn new(entries: &'a [Knowledge]) -> KnowledgeView<'a> {
        KnowledgeView { entries }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub fn entry(&self, place: PlaceRef) -> Option<&Knowledge> {
        let key = knowledge_key(place)?;
        self.entries.iter().find(|k| k.target == key)
    }

    #[must_use]
    pub fn knows(&self, place: PlaceRef) -> bool {
        self.entry(place).is_some()
    }

    #[must_use]
    pub fn as_slice(&self) -> &[Knowledge] {
        self.entries
    }
}

/// Klucz miejsca w magazynie wiedzy: indeks encji. Encje są unikalne w obrębie świata,
/// więc budynek i zakład o tym samym indeksie nie istnieją. Dzielnica i punkt w terenie
/// nie są encjami — o nich się nie „wie" w sensie §5.7.
#[must_use]
pub fn knowledge_key(place: PlaceRef) -> Option<u32> {
    match place {
        PlaceRef::Building(b) => Some(b.entity().index()),
        PlaceRef::Parcel(p) => Some(p.entity().index()),
        PlaceRef::Site(s) => Some(s.entity().index()),
        PlaceRef::District(_) | PlaceRef::Coord(_) => None,
    }
}

// ── typy kontraktu ──────────────────────────────────────────────────────────────

/// Godziny otwarcia miejsca. `days` to maska `DayOfWeek` (K-15) — nigdy dzień miesiąca.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct OpenHours {
    pub open: MinuteOfDay,
    pub close: MinuteOfDay,
    pub days: u8,
}

impl OpenHours {
    /// Czynne bez przerwy — dom, szpital.
    pub const ALWAYS: OpenHours = OpenHours {
        open: MinuteOfDay::MIDNIGHT,
        close: MinuteOfDay::MIDNIGHT,
        days: 0b111_1111,
    };

    #[must_use]
    pub fn new(open_min: u16, close_min: u16, days: u8) -> OpenHours {
        OpenHours {
            open: MinuteOfDay::new(open_min),
            close: MinuteOfDay::new(close_min),
            days,
        }
    }

    #[must_use]
    pub const fn is_always(&self) -> bool {
        self.open.get() == self.close.get() && self.days == 0b111_1111
    }

    #[must_use]
    pub fn is_open(&self, dow: DayOfWeek, at: MinuteOfDay) -> bool {
        if self.days & (1 << (dow as u8)) == 0 {
            return false;
        }
        if self.is_always() {
            return true;
        }
        let (o, c, t) = (self.open.get(), self.close.get(), at.get());
        if o <= c {
            t >= o && t < c
        } else {
            // Lokal czynny przez północ — wtedy „po otwarciu LUB przed zamknięciem".
            t >= o || t < c
        }
    }
}

/// Domyślne godziny otwarcia per rodzaj miejsca.
///
/// `ponytail:` stała w kodzie, nie dane. Sufit znany: w M3 nikt tych godzin nie ustala —
/// właścicielem godzin handlu jest M5 (polityka sklepu), a instytucji publicznych M8.
/// Wtedy `opening_hours` zacznie je czytać z ich danych i ta funkcja zniknie.
#[must_use]
pub fn default_hours(kind: PlaceKind) -> OpenHours {
    const ALL_DAYS: u8 = 0b111_1111;
    const MON_SAT: u8 = 0b011_1111;
    const MON_FRI: u8 = 0b001_1111;
    match kind {
        PlaceKind::Home | PlaceKind::Hospital => OpenHours::ALWAYS,
        PlaceKind::Grocery => OpenHours::new(7 * 60, 21 * 60, ALL_DAYS),
        PlaceKind::Eatery | PlaceKind::Leisure | PlaceKind::Social => {
            OpenHours::new(10 * 60, 22 * 60, ALL_DAYS)
        }
        PlaceKind::Clothing => OpenHours::new(10 * 60, 20 * 60, MON_SAT),
        PlaceKind::Doctor => OpenHours::new(8 * 60, 18 * 60, MON_FRI),
        PlaceKind::Pharmacy => OpenHours::new(8 * 60, 20 * 60, MON_SAT),
        PlaceKind::Education => OpenHours::new(7 * 60, 16 * 60, MON_FRI),
        PlaceKind::Workplace => OpenHours::new(6 * 60, 22 * 60, MON_SAT),
    }
}

/// Kandydat na miejsce zaspokojenia potrzeby.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PlaceCandidate {
    pub place: PlaceRef,
    pub travel_min: u16,
    /// M3: `-(travel_min)`. M5: użyteczność §6.4 × 1000.
    pub score: i32,
    pub reason: DecisionReason,
}

impl Default for PlaceCandidate {
    /// Wartość wypełniająca `ArrayVec`, nie wartość domyślna kandydata. To **jedyne**
    /// miejsce w `sim/agents`, w którym pojawia się `DecisionReason::Unspecified` —
    /// i pojawia się w slocie, którego nikt nie czyta (długość wektora go odcina).
    fn default() -> Self {
        PlaceCandidate {
            place: PlaceRef::Coord(WorldCoord::ORIGIN),
            travel_min: u16::MAX,
            score: i32::MIN,
            reason: DecisionReason::Unspecified,
        }
    }
}

/// Żądanie realizacji wizyty.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FulfilRequest {
    pub citizen: CitizenId,
    pub household: HouseholdId,
    pub need: NeedKind,
    pub place: PlaceRef,
    pub at: SimMinute,
    /// M3: `Money(0)` = bez limitu. M5: realny budżet GD.
    pub budget_hint: Money,
}

/// Wynik wizyty.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FulfilOutcome {
    Done {
        satisfaction: Q,
        /// M3 zawsze `Money(0)`. **Pole nie modyfikuje żadnego salda** — saldami
        /// zarządza M5 w swoim systemie (decyzja 9.8).
        spent: Money,
        duration_min: u16,
        reason: DecisionReason,
    },
    /// M3 nigdy nie zwraca; M5 zwraca przy braku towaru albo zbyt wysokiej cenie.
    Refused(DecisionReason),
}

/// Oszacowanie podróży.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TravelEstimate {
    pub minutes: u16,
    /// M3: `Money(0)`.
    pub cost: Money,
    /// M3: `Walk`.
    pub mode: TransportMode,
    pub reason: DecisionReason,
}

/// Zlecenie podróży.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TripRequest {
    pub traveller: CitizenId,
    pub from: PlaceRef,
    pub to: PlaceRef,
    pub depart: MinuteOfDay,
    /// Slot planu, którego dotyczy podróż — wraca w zdarzeniu `Arrive`.
    pub slot: u8,
}

/// Uchwyt do podróży w toku. Niesie trasę w formie, która nie zdradza niczego
/// o grafie: dwa końce i czas. M4 wypełni go swoim identyfikatorem trasy.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TripHandle {
    pub id: u32,
    pub from: PlaceRef,
    pub to: PlaceRef,
    /// Minuta świata, w której zaplanowano `Arrive`.
    pub arrive_at: u32,
    pub minutes: u16,
    pub mode: TransportMode,
}

// ── traity ──────────────────────────────────────────────────────────────────────

/// Źródło miejsc zaspokajających potrzeby.
///
/// M3: `InfinitePlaces` — nieskończone, zawsze udane, wybór = najkrótszy dojazd.
/// M5: indeks ofert sklepów z magazynem, cenami i użytecznością §6.4.
pub trait PlaceProvider: Send + Sync {
    /// Kandydaci zaspokajający potrzebę, wyłącznie **znani** mieszkańcowi (§5.7),
    /// w porządku deterministycznym, malejąco po `score`.
    fn candidates(
        &self,
        need: NeedKind,
        from: PlaceRef,
        max_travel_min: u16,
        known: &KnowledgeView<'_>,
        out: &mut ArrayVec<PlaceCandidate, MAX_CANDIDATES>,
    );

    fn opening_hours(&self, place: PlaceRef) -> OpenHours;

    /// Realizacja wizyty w chwili zdarzenia `StartActivity`.
    fn fulfil(&mut self, req: &FulfilRequest) -> FulfilOutcome;
}

/// Czas, koszt i tryb podróży oraz jej rozpoczęcie.
///
/// M3: `WalkOracle` — prywatny estymator, tryb zawsze `Walk`.
/// M4: `engine/nav` — CH, wybór środka transportu, pojazdy, parking (K-2).
pub trait TravelOracle: Send + Sync {
    fn estimate(
        &self,
        from: PlaceRef,
        to: PlaceRef,
        depart: MinuteOfDay,
        who: &CitizenView<'_>,
    ) -> TravelEstimate;

    /// Rozpoczyna podróż; implementacja sama harmonogramuje zdarzenie `Arrive`.
    fn begin_trip(
        &mut self,
        trip: TripRequest,
        who: &CitizenView<'_>,
        q: &mut crate::des::EventQueue,
    ) -> TripHandle;

    /// Miejsca widoczne z trasy (§5.7).
    fn places_on_route(&self, trip: &TripHandle, out: &mut ArrayVec<PlaceRef, MAX_ON_ROUTE>);
}

/// Wybór miejsca dla zadania — **jedyne** miejsce, w którym M3 rozstrzyga „gdzie".
///
/// Funkcja wolna, nie metoda traitu: planer ma się kompilować przeciwko `dyn
/// PlaceProvider`, a nie przeciwko konkretnej implementacji (kryterium WP4). Odmowa
/// wraca jako `DecisionReason`, bo mieszkaniec, który nigdzie nie poszedł, też ma
/// powód do pokazania w karcie inspekcji (00 §7).
pub fn choose_place(
    places: &dyn PlaceProvider,
    need: NeedKind,
    from: PlaceRef,
    max_travel_min: u16,
    known: &KnowledgeView<'_>,
    out: &mut ArrayVec<PlaceCandidate, MAX_CANDIDATES>,
) -> Result<PlaceCandidate, DecisionReason> {
    places.candidates(need, from, max_travel_min, known, out);
    match out.first() {
        Some(best) => Ok(*best),
        None => Err(DecisionReason::PlaceUnknown {
            need,
            known_count: known.len().min(255) as u8,
        }),
    }
}

// ── implementacja tymczasowa: InfinitePlaces (M5 zastępuje) ─────────────────────

/// Nieskończone źródła miejsc: każde znane miejsce właściwego rodzaju zaspokaja
/// potrzebę, zawsze, za darmo. Wybór = najkrótszy dojazd.
///
/// To nie jest uproszczenie „na razie": to jest **sklep bez gospodarki**, czyli
/// dokładnie tyle, ile M3 wolno wiedzieć. M5 podmienia implementację i nic poza nią.
pub struct InfinitePlaces {
    places: std::sync::Arc<PlaceTable>,
    needs: std::sync::Arc<NeedTable>,
    /// Prędkość marszu w metrach na minutę — do przeliczenia zasięgu na promień.
    walk_m_per_min: f32,
}

impl InfinitePlaces {
    #[must_use]
    pub fn new(
        places: std::sync::Arc<PlaceTable>,
        needs: std::sync::Arc<NeedTable>,
    ) -> InfinitePlaces {
        InfinitePlaces {
            places,
            needs,
            walk_m_per_min: crate::walk::BASE_SPEED_M_PER_MIN,
        }
    }
}

impl PlaceProvider for InfinitePlaces {
    fn candidates(
        &self,
        need: NeedKind,
        from: PlaceRef,
        max_travel_min: u16,
        known: &KnowledgeView<'_>,
        out: &mut ArrayVec<PlaceCandidate, MAX_CANDIDATES>,
    ) {
        out.clear();
        let Some(origin) = self.places.coord_of(from) else {
            return;
        };
        let radius_m = f32::from(max_travel_min) * self.walk_m_per_min;

        for kind in self.needs.places_for(need) {
            self.places.for_each_near(*kind, origin, radius_m, |e| {
                if e.place == from || !known.knows(e.place) {
                    return;
                }
                let minuty = crate::walk::walk_minutes(origin, e.at, 100);
                if minuty > max_travel_min {
                    return;
                }
                let kandydat = PlaceCandidate {
                    place: e.place,
                    travel_min: minuty,
                    score: -i32::from(minuty),
                    // Uzupełniane niżej: dopóki nie znamy całej listy, nie wiadomo,
                    // o ile gorsza była alternatywa.
                    reason: DecisionReason::ChosenNearest {
                        travel_min: minuty,
                        runner_up_min: minuty,
                    },
                };
                // Wstawianie z utrzymaniem porządku: lista ma 16 pozycji, więc
                // to jest tańsze niż zebranie wszystkiego i posortowanie.
                // Remis rozstrzyga `PlaceRef` — inaczej kolejność zależałaby
                // od układu komórek indeksu przestrzennego.
                let klucz = (minuty, e.place);
                let poz = out
                    .iter()
                    .position(|c| (c.travel_min, c.place) > klucz)
                    .unwrap_or(out.len());
                if out.is_full() {
                    if poz == out.len() {
                        return;
                    }
                    out.pop();
                }
                out.insert(poz, kandydat);
            });
        }

        // Druga alternatywa jest częścią wyjaśnienia, nie ozdobą: PRD §5.5 chce
        // w karcie inspekcji zdania „wybrany: 2. najtańszy, ale po drodze".
        // Czasy na stos, nie do `Vec`: to jest ścieżka gorąca planera (R6).
        let mut czasy = [0u16; MAX_CANDIDATES];
        let n = out.len();
        for (i, c) in out.iter().enumerate() {
            czasy[i] = c.travel_min;
        }
        for (i, c) in out.iter_mut().enumerate() {
            let runner_up = if i + 1 < n {
                czasy[i + 1]
            } else {
                c.travel_min
            };
            c.reason = DecisionReason::ChosenNearest {
                travel_min: c.travel_min,
                runner_up_min: runner_up,
            };
        }
    }

    fn opening_hours(&self, place: PlaceRef) -> OpenHours {
        self.places
            .get(place)
            .map_or(OpenHours::ALWAYS, |e| default_hours(e.kind))
    }

    fn fulfil(&mut self, req: &FulfilRequest) -> FulfilOutcome {
        let spec = self.needs.spec(req.need);
        FulfilOutcome::Done {
            satisfaction: Q::new(spec.satisfaction),
            spent: Money::ZERO,
            duration_min: spec.visit_min,
            reason: DecisionReason::NeedSatisfied {
                need: req.need,
                gain: Q::new(spec.satisfaction),
            },
        }
    }
}

// ── atrapy testowe (zostają jako testy kontraktowe dla M4 i M5) ─────────────────

/// Odmawia co trzeciej wizycie. Buduje ścieżkę `ReplanCause::PlaceRefused` w M3,
/// zanim M5 będzie miał czym odmawiać.
pub struct FlakyPlaces {
    inner: InfinitePlaces,
    licznik: u32,
}

impl FlakyPlaces {
    #[must_use]
    pub fn new(inner: InfinitePlaces) -> FlakyPlaces {
        FlakyPlaces { inner, licznik: 0 }
    }
}

impl PlaceProvider for FlakyPlaces {
    fn candidates(
        &self,
        need: NeedKind,
        from: PlaceRef,
        max_travel_min: u16,
        known: &KnowledgeView<'_>,
        out: &mut ArrayVec<PlaceCandidate, MAX_CANDIDATES>,
    ) {
        self.inner
            .candidates(need, from, max_travel_min, known, out);
    }

    fn opening_hours(&self, place: PlaceRef) -> OpenHours {
        self.inner.opening_hours(place)
    }

    fn fulfil(&mut self, req: &FulfilRequest) -> FulfilOutcome {
        self.licznik += 1;
        if self.licznik.is_multiple_of(3) {
            return FulfilOutcome::Refused(DecisionReason::PlaceUnknown {
                need: req.need,
                known_count: 0,
            });
        }
        self.inner.fulfil(req)
    }
}

/// Panikuje w każdej metodzie. Istnieje po to, żeby test mógł **dowieść**, że kod
/// wołający trzyma się traitu: jeśli da się go podstawić i skompilować, to znaczy,
/// że nigdzie nie przecieka typ implementacji (kryterium WP4).
pub struct PanickingPlaces;

impl PlaceProvider for PanickingPlaces {
    fn candidates(
        &self,
        _need: NeedKind,
        _from: PlaceRef,
        _max_travel_min: u16,
        _known: &KnowledgeView<'_>,
        _out: &mut ArrayVec<PlaceCandidate, MAX_CANDIDATES>,
    ) {
        panic!("PanickingPlaces::candidates");
    }

    fn opening_hours(&self, _place: PlaceRef) -> OpenHours {
        panic!("PanickingPlaces::opening_hours");
    }

    fn fulfil(&mut self, _req: &FulfilRequest) -> FulfilOutcome {
        panic!("PanickingPlaces::fulfil");
    }
}

/// Nie zna żadnego miejsca — mieszkaniec dostaje `PlaceUnknown` zamiast teleportacji.
pub struct EmptyPlaces;

impl PlaceProvider for EmptyPlaces {
    fn candidates(
        &self,
        _need: NeedKind,
        _from: PlaceRef,
        _max_travel_min: u16,
        _known: &KnowledgeView<'_>,
        out: &mut ArrayVec<PlaceCandidate, MAX_CANDIDATES>,
    ) {
        out.clear();
    }

    fn opening_hours(&self, _place: PlaceRef) -> OpenHours {
        OpenHours::ALWAYS
    }

    fn fulfil(&mut self, req: &FulfilRequest) -> FulfilOutcome {
        FulfilOutcome::Refused(DecisionReason::PlaceUnknown {
            need: req.need,
            known_count: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn godziny_otwarcia_zawijaja_sie_przez_polnoc() {
        let nocny = OpenHours::new(22 * 60, 2 * 60, 0b111_1111);
        assert!(nocny.is_open(DayOfWeek::Monday, MinuteOfDay::new(23 * 60)));
        assert!(nocny.is_open(DayOfWeek::Monday, MinuteOfDay::new(60)));
        assert!(!nocny.is_open(DayOfWeek::Monday, MinuteOfDay::new(12 * 60)));

        let sklep = default_hours(PlaceKind::Grocery);
        assert!(sklep.is_open(DayOfWeek::Sunday, MinuteOfDay::new(10 * 60)));
        assert!(!sklep.is_open(DayOfWeek::Sunday, MinuteOfDay::new(6 * 60)));

        let szkola = default_hours(PlaceKind::Education);
        assert!(!szkola.is_open(DayOfWeek::Saturday, MinuteOfDay::new(10 * 60)));
        assert!(OpenHours::ALWAYS.is_open(DayOfWeek::Sunday, MinuteOfDay::new(3 * 60)));
    }

    #[test]
    fn katalog_miejsc_znajduje_po_uchwycie_i_po_promieniu() {
        use magnat_core::{BuildingId, Entity};
        let bud = |i: u32| {
            PlaceRef::Building(BuildingId(Entity::new(
                i,
                std::num::NonZeroU32::new(1).unwrap(),
            )))
        };
        let t = PlaceTable::build(vec![
            PlaceEntry {
                place: bud(1),
                kind: PlaceKind::Grocery,
                at: WorldCoord::new(0, 0, 0),
            },
            PlaceEntry {
                place: bud(2),
                kind: PlaceKind::Grocery,
                at: WorldCoord::new(30_000, 0, 0), // 300 m
            },
            PlaceEntry {
                place: bud(3),
                kind: PlaceKind::Doctor,
                at: WorldCoord::new(10_000, 0, 0),
            },
        ]);
        assert_eq!(t.len(), 3);
        assert_eq!(t.get(bud(2)).map(|e| e.at.x), Some(30_000));
        assert_eq!(
            t.coord_of(PlaceRef::Coord(WorldCoord::new(5, 6, 7)))
                .unwrap()
                .x,
            5
        );

        let mut znalezione = Vec::new();
        t.for_each_near(PlaceKind::Grocery, WorldCoord::new(0, 0, 0), 400.0, |e| {
            znalezione.push(e.place);
        });
        znalezione.sort_unstable();
        assert_eq!(
            znalezione,
            vec![bud(1), bud(2)],
            "lekarz trafił do spożywczych"
        );
    }
}
