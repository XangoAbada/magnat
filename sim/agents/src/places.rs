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
use crate::components::{Employment, Identity, Needs, Personality, Residence, Vitals};
use crate::needs::NeedTable;
use crate::store::Knowledge;
use magnat_core::{
    BuildingId, CitizenId, CitizenReason, DecisionReason, Entity, HouseholdId, MinuteOfDay, Money,
    NeedKind, PlaceKind, PlaceRef, SimMinute, SiteId, TransportMode, WorldCoord, Q,
};
use magnat_sim_snapshot::PedestrianRecord;
use magnat_spatial::{Aabb2, CategoryGrid, GridSpec, Vec2};

/// Twardy limit kandydatów (§5.3, ryzyko R6). Nie jest orientacyjny: to on trzyma
/// planer w budżecie 10 µs także wtedy, gdy M5 wstawi do `candidates` funkcję
/// użyteczności §6.4.
pub const MAX_CANDIDATES: usize = 16;

/// Limit miejsc widocznych z trasy (§5.7).
pub const MAX_ON_ROUTE: usize = 8;

/// Prędkość marszu w metrach na minutę (4,86 km/h).
///
/// Mieszka tutaj, a nie w `sim/traffic`, świadomie: to jest **ranking kandydatów
/// bez sieci**, a nie routing. `PlaceProvider` musi umieć odsiać sto miejsc w promieniu
/// zanim ktokolwiek policzy trasę do choćby jednego z nich, a `sim/agents` nie zależy
/// od crate'u ruchu (zależność idzie w drugą stronę). Ta sama liczba jest bazą
/// szacunku marszu w `magnat_traffic::oracle` — i tam, i tu jest kalibracją M3b.
pub const BASE_SPEED_M_PER_MIN: f32 = 81.0;

/// Czas dojścia liczony manhattanowo, w minutach. Szacunek do rankingu, nie do planu:
/// czasem, który trafia do slotu `Commute`, jest zawsze wynik `TravelOracle`.
///
/// **Bez mnożnika nadłożenia.** Odległość manhattanowa **jest** odległością po siatce
/// ulic wszędzie tam, gdzie ulice biegną wzdłuż osi — a tak wygląda większość miasta
/// M2. Korekta za nadłożenie należy do estymatora, który wie, że nie ma sieci
/// (`magnat_traffic`, ścieżka zapasowa), a nie do rankingu kandydatów.
#[must_use]
pub fn walk_minutes(from: WorldCoord, to: WorldCoord, speed_pct: u32) -> u16 {
    let dist = i64::from((to.x - from.x).abs()) + i64::from((to.y - from.y).abs());
    let v = (8_100 * i64::from(speed_pct) / 100).max(1);
    // W górę, nie do najbliższej: dojście trwające 3,7 minuty kończy się w czwartej
    // minucie, a plan operuje minutami całymi i nie ma prawa obiecywać przybycia
    // wcześniej, niż mieszkaniec dojdzie.
    ((dist + v - 1) / v).clamp(1, i64::from(u16::MAX)) as u16
}

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

/// Promienie szukania szkoły: kwartał, dzielnica, pół miasta.
const PROMIENIE_SZKOLY: [f32; 3] = [800.0, 2500.0, 8000.0];

/// Najbliższa szkoła dla mieszkańca spod `at`, jako klucz wiedzy (`Employment.site`).
///
/// Jedna reguła dla dwóch wołających: Etap 8 generatora rozdaje szkoły całemu rocznikowi
/// naraz, a dobowy cykl życia (`R2-WP1`) pojedynczemu siedmiolatkowi — i muszą wybierać
/// tak samo, bo inaczej dziecko urodzone w grze chodziłoby do innej szkoły niż jego
/// rówieśnik z generacji, przy tym samym domu. Rozstrzygnięcie jest deterministyczne
/// **bez losowania**: najbliższa, a przy równej odległości ta o niższym kluczu. Stąd
/// ten pakiet nie zajmuje numeru `StreamId`, choć plan R2a taki numer zapowiadał.
///
/// Pojemności placówki reguła nie zna i nie sprawdza — normatyw obwodu liczy M8d
/// (`ServiceCoverage`), a przeciążona szkoła obniża jakość usługi, nie odsyła ucznia.
#[must_use]
pub fn nearest_school(places: &PlaceTable, at: WorldCoord) -> Option<u32> {
    let mut najblizsza: Option<(i64, u32)> = None;
    for r in PROMIENIE_SZKOLY {
        places.for_each_near(PlaceKind::Education, at, r, |e| {
            let d = e.at.distance_sq_xy(at);
            let klucz = knowledge_key(e.place).unwrap_or(u32::MAX);
            if najblizsza.is_none_or(|(bd, bk)| (d, klucz) < (bd, bk)) {
                najblizsza = Some((d, klucz));
            }
        });
        if najblizsza.is_some() {
            break;
        }
    }
    najblizsza.map(|(_, klucz)| klucz)
}

/// Katalog miejsc jako zasób świata.
///
/// Ten sam `Arc`, który dostaje `Market` i `TrafficOracle` — nie druga kopia. Zasób
/// istnieje, bo `Sources` jest na czas minuty **wyjmowane** z ECS (pętla doby trzyma
/// `&mut World` obok), więc dobowy przebieg demografii nie ma jak przez nie sięgnąć
/// po katalog, a szkoła dla siedmiolatka jest potrzebna dokładnie tam.
///
/// Nie wchodzi do hasha stanu: katalog jest daną miasta, a nie stanem symulacji —
/// powstaje raz, przy zaludnianiu, i potem się go tylko czyta. Świat bez katalogu
/// (scenariusze M3 bez miasta) dostaje `None` i wtedy uczeń zostaje bez placówki,
/// tak samo jak dziś.
#[derive(Clone, Default)]
pub struct PlaceCatalog(Option<std::sync::Arc<PlaceTable>>);

impl PlaceCatalog {
    #[must_use]
    pub fn new(places: std::sync::Arc<PlaceTable>) -> PlaceCatalog {
        PlaceCatalog(Some(places))
    }

    #[must_use]
    pub fn get(&self) -> Option<&PlaceTable> {
        self.0.as_deref()
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
    /// Marki, które mieszkaniec zna (M10b §5.1). Pusty widok = decyzja bez członu marki.
    pub brands: BrandView<'a>,
}

/// Marki, które mieszkaniec zna, z naniesionym zanikiem (M10b §5.1).
///
/// Ten sam wzorzec co [`KnowledgeView`] i z tego samego powodu: `PlaceProvider`
/// dostaje `&self`, a nie `&World`, więc pamięć mieszkańca musi przyjechać razem
/// z nim. Widok jest pusty w każdym świecie bez marek i wtedy człon marki w decyzji
/// zakupowej jest zerem — czyli dokładnie tym, czym był do M10b.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct BrandView<'a> {
    slots: &'a [crate::brand::BrandAffinity],
}

impl<'a> BrandView<'a> {
    #[must_use]
    pub fn new(slots: &'a [crate::brand::BrandAffinity]) -> BrandView<'a> {
        BrandView { slots }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Co mieszkaniec myśli o tej marce. `None` = nie zna jej.
    #[must_use]
    pub fn get(&self, brand: magnat_core::BrandId) -> Option<&crate::brand::BrandAffinity> {
        self.slots.iter().find(|s| s.brand == brand)
    }

    #[must_use]
    pub fn as_slice(&self) -> &[crate::brand::BrandAffinity] {
        self.slots
    }
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

/// Odwrotność [`knowledge_key`]: miejsce z samego indeksu encji.
///
/// Konwencja kluczy jest jedna na cały projekt i ustala ją Etap 8 (M3d §5.9): budynki
/// numerowane od zera, zakłady od `1 << 24`. Dzięki temu `Residence.building`
/// i `Employment.site` — oba zwykłe `u32` — dają się odróżnić bez dodatkowego pola,
/// a magazyn wiedzy ma jedną przestrzeń kluczy bez kolizji.
///
/// `sim/agents` zna **konwencję**, a nie miasto: skąd się biorą numery, wie generator
/// populacji, który jako jedyny widzi jednocześnie budynki M2 i mieszkańców.
pub const SITE_KEY_BASE: u32 = 1 << 24;

/// Miejsce z klucza wiedzy. `None` dla `PlanSlot::NO_TARGET`.
#[inline]
#[must_use]
pub fn place_from_key(key: u32) -> Option<PlaceRef> {
    if key == u32::MAX {
        return None;
    }
    let e = Entity::new(key, std::num::NonZeroU32::new(1).expect("1 != 0"));
    Some(if key >= SITE_KEY_BASE {
        PlaceRef::Site(SiteId(e))
    } else {
        PlaceRef::Building(BuildingId(e))
    })
}

/// Dom mieszkańca jako `PlaceRef`; `None`, gdy nie ma lokalu.
#[inline]
#[must_use]
pub fn home_of(r: &Residence) -> Option<PlaceRef> {
    place_from_key(r.building).filter(|_| r.building != Residence::HOMELESS)
}

/// Zakład (albo szkoła ucznia) jako `PlaceRef`; `None`, gdy nie ma pracy.
#[inline]
#[must_use]
pub fn site_of(e: &Employment) -> Option<PlaceRef> {
    place_from_key(e.site).filter(|_| e.site != Employment::NO_SITE)
}

// ── typy kontraktu ──────────────────────────────────────────────────────────────

/// Godziny otwarcia. **Typ mieszka od M6b w `engine/core`** (`K-8`): pyta o niego M3
/// przy planowaniu doby, M6 przy kolejce rampy (godziny dostaw) i M8 przy regulacji
/// miejskiej, a duplikat rozjechałby się przy pierwszej zmianie warunku „czynne przez
/// północ". Reeksport, żeby nazwy z M3 nie drgnęły.
pub use magnat_core::OpenHours;

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
pub struct FulfilRequest<'a> {
    pub citizen: CitizenId,
    pub household: HouseholdId,
    pub need: NeedKind,
    pub place: PlaceRef,
    pub at: SimMinute,
    /// M3: `Money(0)` = bez limitu. M5: realny budżet GD.
    pub budget_hint: Money,
    /// Ilu ludzi żywi ten zakup. M3 wypełnia jedynką, bo nie kupuje niczego;
    /// M5b liczy z tego ilość towaru — koszyk potrzeb epoki jest podany
    /// **na mieszkańca na dobę**, więc bez liczebności nie da się go przeliczyć
    /// na sztuki.
    pub household_size: u8,
    /// Ilu z nich nie ukończyło `ages.adult` (`R2-WP11`). Skala ekwiwalentna,
    /// którą M5 z tego liczy, jest **daną gospodarki** (`envelopes.ron`), więc
    /// `sim/agents` podaje sam skład, a nie wynik.
    pub household_children: u8,
    /// Marki, które kupujący zna (M10b §5.1) — ten sam widok, którym liczył się
    /// wybór sklepu. Gdyby próg akceptacji liczył się bez niego, mieszkaniec
    /// wybierałby sklep z powodu marki i odrzucał go z braku marki.
    pub brands: BrandView<'a>,
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
    /// Po co ta podróż — wejście wartości czasu po stronie M4c.
    ///
    /// Pole dołożone w R2e (`R2-WP22`). Do tej chwili zlecenie celu nie niosło,
    /// a `TrafficOracle::start_trip` wpisywał **każdej** podróży `Work`: z siedmiu
    /// mnożników w `data/roads/mode_choice.ron` żył jeden, a odprowadzenie dziecka
    /// wyceniało czas tak samo jak dojazd do pracy, choć tabela mówi 1,50 wobec 1,30.
    /// Cel zna planer doby — bo to on wie, do czego mieszkaniec wychodzi — więc
    /// wychodzi stąd, a nie z domysłu po stronie ruchu.
    pub purpose: magnat_core::TripPurpose,
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
    ///
    /// `who` wnosi M5b: funkcja użyteczności zakupu (PRD §6.4) wyprowadza wagi
    /// z osobowości i statusu, a `PlaceCandidate.score` ma być tą użytecznością
    /// razy 1000 — co zapowiada dokumentacja pola od M3a. Bez kupującego w sygnaturze
    /// nie dałoby się jej policzyć tam, gdzie zapada wybór sklepu, czyli w planerze.
    /// `InfinitePlaces` z M3 argumentu nie używa i nie musi.
    fn candidates(
        &self,
        need: NeedKind,
        from: PlaceRef,
        max_travel_min: u16,
        known: &KnowledgeView<'_>,
        who: &CitizenView<'_>,
        out: &mut ArrayVec<PlaceCandidate, MAX_CANDIDATES>,
    );

    fn opening_hours(&self, place: PlaceRef) -> OpenHours;

    /// Realizacja wizyty w chwili zdarzenia `StartActivity`.
    fn fulfil(&mut self, req: &FulfilRequest<'_>) -> FulfilOutcome;
}

/// Estymator w linii prostej — **dubler testowy** i nic więcej.
///
/// Zastępuje `WalkOracle` z M3 w testach tego crate'u i w benchmarkach: `sim/agents`
/// nie może zależeć od `sim/traffic` (zależność idzie w drugą stronę), a testy planera
/// potrzebują jakiegokolwiek czasu dojścia. Wzór jest ten sam, który `InfinitePlaces`
/// stosuje do rankingu kandydatów — manhattan z nadłożeniem 1,25. Prawdziwy czas
/// podróży liczy `magnat_traffic::TrafficOracle` na sieci.
pub struct StraightLineTravel {
    places: std::sync::Arc<PlaceTable>,
    next_trip: std::sync::atomic::AtomicU32,
}

impl StraightLineTravel {
    #[must_use]
    pub fn new(places: std::sync::Arc<PlaceTable>) -> StraightLineTravel {
        StraightLineTravel {
            places,
            next_trip: std::sync::atomic::AtomicU32::new(1),
        }
    }

    fn minutes(&self, from: PlaceRef, to: PlaceRef) -> u16 {
        match (self.places.coord_of(from), self.places.coord_of(to)) {
            (Some(a), Some(b)) => walk_minutes(a, b, 100),
            _ => 1,
        }
    }
}

impl TravelOracle for StraightLineTravel {
    fn estimate(
        &self,
        from: PlaceRef,
        to: PlaceRef,
        _depart: MinuteOfDay,
        _who: &CitizenView<'_>,
    ) -> TravelEstimate {
        let minutes = self.minutes(from, to);
        TravelEstimate {
            minutes,
            cost: Money::ZERO,
            mode: TransportMode::Walk,
            reason: DecisionReason::Citizen(CitizenReason::ModeWalkOnly { minutes }),
        }
    }

    fn begin_trip(
        &mut self,
        trip: TripRequest,
        _who: &CitizenView<'_>,
        q: &mut crate::des::EventQueue,
    ) -> TripHandle {
        let minutes = self.minutes(trip.from, trip.to);
        let arrive_at = q.now() + u32::from(minutes);
        q.schedule(crate::des::SimEvent::new(
            arrive_at,
            trip.traveller.entity().index(),
            crate::des::EventKind::Arrive,
            trip.slot,
        ));
        TripHandle {
            id: self
                .next_trip
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            from: trip.from,
            to: trip.to,
            arrive_at,
            minutes,
            mode: TransportMode::Walk,
        }
    }

    fn places_on_route(&self, _trip: &TripHandle, out: &mut ArrayVec<PlaceRef, MAX_ON_ROUTE>) {
        out.clear();
    }
}

/// Czas, koszt i tryb podróży oraz jej rozpoczęcie.
///
/// M3: `WalkOracle` — estymator manhattanowy, tryb zawsze `Walk`.
/// M4b: `magnat_traffic::TrafficOracle` — sieć, router CCH, pojazdy i warstwa mezo.
///
/// **Kto wstawia `Arrive`.** Do M4b zawsze implementacja traitu, w `begin_trip`.
/// Od M4b tylko dla podróży, które nie wchodzą na sieć (pieszo); dla przejazdu
/// samochodem zdarzenie wstawia warstwa mezo w minucie faktycznego przyjazdu, bo to
/// ona, a nie szacunek, wie o korku. Wołający się przez to nie zmienia: dostaje
/// `TripHandle` z czasem planowanym i `Arrive` przychodzi tak samo jak wcześniej.
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

    // ── warstwa Mikro ───────────────────────────────────────────────────────────
    //
    // Cztery metody z domyślną implementacją pustą, wszystkie po `&self`: system
    // `TravelMicroSystem` deklaruje **odczyt** zasobu `AgentSources`, więc mutacja
    // musi iść wewnątrz implementacji (zamek albo atomiki). Domyślne ciała są puste,
    // bo warstwa Mikro jest wizualizacją: implementacja, która jej nie ma, nadal
    // spełnia kontrakt ekonomiczny w całości (00 §4).

    /// Wpuszcza podróżnika do warstwy Mikro, jeśli mieści się w oknie kamery (`Z-6`).
    fn enter_micro(&self, _handle: &TripHandle, _traveller: u32, _depart: MinuteOfDay) {}

    /// Krok warstwy Mikro; `now_ms` to milisekunda doby, tick 100 ms (00 §4).
    fn micro_step(&self, _now_ms: u64) {}

    /// Usuwa z warstwy tych, którzy już dotarli.
    fn micro_retire(&self, _now_min: u16) {}

    /// Ilu podróżników jest w tej chwili w warstwie Mikro.
    fn micro_len(&self) -> usize {
        0
    }

    /// Okno kamery: podróżnik wchodzi do warstwy Mikro tylko wtedy, gdy któryś
    /// koniec jego trasy się w nim mieści. Promień 0 = warstwa wyłączona (`Z-6`).
    fn set_micro_window(&self, _center: Option<(i32, i32)>, _radius_m: u32) {}

    /// Mieszkańcy, którzy zostają w warstwie Mikro **niezależnie od kadru** (`LodPin`,
    /// M9 §9 pkt 2): postać gracza i cel trybu „śledź". Lista jest krótka i to jest
    /// jej cena — przypięty mieszkaniec liczy się mikro także poza ekranem.
    fn set_micro_pins(&self, _citizens: &[u32]) {}

    /// Zrzut dla renderera — **w docelowej strukturze**, nie w krotce pośredniej.
    ///
    /// Renderer bierze `&[PedestrianRecord]`, więc zrzut do krotki kazał wołającemu
    /// przepisać całość drugi raz w tej samej klatce, tylko po to, żeby odrzucić
    /// postęp, którego nikt nie czyta (M4c §5.12 punkt 3). Selekcja kadru — promień
    /// i stożek widzenia — zostaje po stronie renderera, bo tylko on zna kamerę.
    fn micro_snapshot(&self, out: &mut Vec<PedestrianRecord>) {
        out.clear();
    }
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
    who: &CitizenView<'_>,
    out: &mut ArrayVec<PlaceCandidate, MAX_CANDIDATES>,
) -> Result<PlaceCandidate, DecisionReason> {
    places.candidates(need, from, max_travel_min, known, who, out);
    match out.first() {
        Some(best) => Ok(*best),
        None => Err(DecisionReason::Citizen(CitizenReason::PlaceUnknown {
            need,
            known_count: known.len().min(255) as u8,
        })),
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
            walk_m_per_min: BASE_SPEED_M_PER_MIN,
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
        _who: &CitizenView<'_>,
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
                let minuty = walk_minutes(origin, e.at, 100);
                if minuty > max_travel_min {
                    return;
                }
                let kandydat = PlaceCandidate {
                    place: e.place,
                    travel_min: minuty,
                    score: -i32::from(minuty),
                    // Uzupełniane niżej: dopóki nie znamy całej listy, nie wiadomo,
                    // o ile gorsza była alternatywa.
                    reason: DecisionReason::Citizen(CitizenReason::ChosenNearest {
                        travel_min: minuty,
                        runner_up_min: minuty,
                    }),
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
            c.reason = DecisionReason::Citizen(CitizenReason::ChosenNearest {
                travel_min: c.travel_min,
                runner_up_min: runner_up,
            });
        }
    }

    fn opening_hours(&self, place: PlaceRef) -> OpenHours {
        self.places
            .get(place)
            .map_or(OpenHours::ALWAYS, |e| default_hours(e.kind))
    }

    fn fulfil(&mut self, req: &FulfilRequest<'_>) -> FulfilOutcome {
        let spec = self.needs.spec(req.need);
        FulfilOutcome::Done {
            satisfaction: Q::new(spec.satisfaction),
            spent: Money::ZERO,
            duration_min: spec.visit_min,
            reason: DecisionReason::Citizen(CitizenReason::NeedSatisfied {
                need: req.need,
                gain: Q::new(spec.satisfaction),
            }),
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
        who: &CitizenView<'_>,
        out: &mut ArrayVec<PlaceCandidate, MAX_CANDIDATES>,
    ) {
        self.inner
            .candidates(need, from, max_travel_min, known, who, out);
    }

    fn opening_hours(&self, place: PlaceRef) -> OpenHours {
        self.inner.opening_hours(place)
    }

    fn fulfil(&mut self, req: &FulfilRequest<'_>) -> FulfilOutcome {
        self.licznik += 1;
        if self.licznik.is_multiple_of(3) {
            return FulfilOutcome::Refused(DecisionReason::Citizen(CitizenReason::PlaceUnknown {
                need: req.need,
                known_count: 0,
            }));
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
        _who: &CitizenView<'_>,
        _out: &mut ArrayVec<PlaceCandidate, MAX_CANDIDATES>,
    ) {
        panic!("PanickingPlaces::candidates");
    }

    fn opening_hours(&self, _place: PlaceRef) -> OpenHours {
        panic!("PanickingPlaces::opening_hours");
    }

    fn fulfil(&mut self, _req: &FulfilRequest<'_>) -> FulfilOutcome {
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
        _who: &CitizenView<'_>,
        out: &mut ArrayVec<PlaceCandidate, MAX_CANDIDATES>,
    ) {
        out.clear();
    }

    fn opening_hours(&self, _place: PlaceRef) -> OpenHours {
        OpenHours::ALWAYS
    }

    fn fulfil(&mut self, req: &FulfilRequest<'_>) -> FulfilOutcome {
        FulfilOutcome::Refused(DecisionReason::Citizen(CitizenReason::PlaceUnknown {
            need: req.need,
            known_count: 0,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::DayOfWeek;

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
