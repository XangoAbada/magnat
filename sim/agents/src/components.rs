//! Komponenty mieszkańca w układzie SoA (M3a §5.1).
//!
//! Wszystkie są `#[repr(C)]` i mają **jawne** pola wyrównujące: budżet §17.7 to 400 B
//! stanu gorącego na mieszkańca, a przy 400 tys. mieszkańców jeden bajt niejawnego
//! paddingu kosztuje 400 KB, o których nikt się nie dowie. Test `rozmiary_komponentow`
//! sprawdza `size_of` i `offset_of` wobec tabeli z dokumentu fazy — rozjazd łamie build,
//! a nie budżet.
//!
//! Czego tu **nie ma**, choć pola są: majątku się nie przelicza (operacje w M5),
//! pracy się nie zmienia (rynek pracy w M7), planu się nie układa (M3b). M3a definiuje
//! kształt i pilnuje rachunku pamięci.

use crate::des::EventQueue;
use crate::needs::NeedTable;
use crate::store::{KnowledgeSlab, PlanSlab, RelationSlab, SlabRef};
use magnat_core::{
    ActivityKind, DayOfWeek, HashState, MinuteOfDay, Money, Mood, StateHasher, TraitId, Q,
};
use magnat_ecs::Component;

// ── stan gorący: 13 komponentów, razem 140 B ────────────────────────────────────

/// Tożsamość — 16 B. `first_name`/`last_name` to indeksy w puli `data/names/`
/// (nazwy generowane proceduralnie nie są lokalizacją UI).
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Identity {
    pub first_name: u16,
    pub last_name: u16,
    /// Dni od doby 0 świata; ujemne = urodzony przed startem. Rok = 360 dni (K-1).
    pub birth_day: i32,
    /// `DistrictId`; `Identity::DISTRICT_IMMIGRANT` = przyjezdny.
    pub birth_district: u16,
    /// bit0 płeć (0 = kobieta), bit1 żywy, bit2 gracz, bit3..7 rezerwa.
    pub flags: u8,
    pub _pad: u8,
    /// Indeks encji gospodarstwa domowego.
    pub household: u32,
}

impl Identity {
    pub const DISTRICT_IMMIGRANT: u16 = u16::MAX;
    pub const FLAG_MALE: u8 = 1 << 0;
    pub const FLAG_ALIVE: u8 = 1 << 1;
    pub const FLAG_PLAYER: u8 = 1 << 2;

    #[inline]
    #[must_use]
    pub const fn is_alive(&self) -> bool {
        self.flags & Identity::FLAG_ALIVE != 0
    }

    #[inline]
    #[must_use]
    pub const fn is_male(&self) -> bool {
        self.flags & Identity::FLAG_MALE != 0
    }

    /// Wiek w dobach na dzień `today` (doba świata). Ujemny wiek nie istnieje —
    /// mieszkaniec urodzony w przyszłości to błąd generacji, nie stan do obsłużenia.
    #[inline]
    #[must_use]
    pub const fn age_days(&self, today: i32) -> i32 {
        today - self.birth_day
    }

    /// Wiek w latach gry (rok = 360 dni, K-1).
    #[inline]
    #[must_use]
    pub const fn age_years(&self, today: i32) -> i32 {
        self.age_days(today) / 360
    }
}

/// Osiem cech osobowości w skali 0..=100, indeksowanych `TraitId` (8 B).
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Personality(pub [u8; 8]);

impl Personality {
    #[inline]
    #[must_use]
    pub fn get(&self, t: TraitId) -> Q {
        Q::new(self.0[t.as_index()])
    }

    #[inline]
    pub fn set(&mut self, t: TraitId, v: Q) {
        self.0[t.as_index()] = v.get();
    }
}

/// Poziom wykształcenia. Mieszka w `sim/agents`, nie w `core`: czyta go M7 (dopasowanie
/// do roli), ale `sim/firms` i tak zależy od `sim/agents`, więc cyklu z K-8 tu nie ma.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
#[repr(u8)]
pub enum EduLevel {
    #[default]
    None = 0,
    Primary = 1,
    Vocational = 2,
    Secondary = 3,
    Higher = 4,
}

impl EduLevel {
    #[must_use]
    pub const fn from_u8(v: u8) -> EduLevel {
        match v {
            1 => EduLevel::Primary,
            2 => EduLevel::Vocational,
            3 => EduLevel::Secondary,
            4 => EduLevel::Higher,
            _ => EduLevel::None,
        }
    }
}

/// Kierunek wykształcenia — podstawa `skill_match(role)` w Etapie 8 (M3d) i w M7.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
#[repr(u8)]
pub enum EduField {
    #[default]
    None = 0,
    Technical = 1,
    Humanities = 2,
    Medical = 3,
    Economic = 4,
    Artistic = 5,
}

impl EduField {
    #[must_use]
    pub const fn from_u8(v: u8) -> EduField {
        match v {
            1 => EduField::Technical,
            2 => EduField::Humanities,
            3 => EduField::Medical,
            4 => EduField::Economic,
            5 => EduField::Artistic,
            _ => EduField::None,
        }
    }
}

/// Stan fizyczny i społeczny — 8 B. Klasa społeczna **nie jest przechowywana**:
/// to przedział `status`, liczony na żądanie (M3c §5.8).
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Vitals {
    /// Q 0..=100.
    pub health: u8,
    /// Q 0..=100.
    pub energy: u8,
    /// Mood -100..=100.
    pub mood: i8,
    /// Q 0..=100.
    pub stress: u8,
    /// `EduLevel`.
    pub edu_level: u8,
    /// `EduField`.
    pub edu_field: u8,
    /// Q 0..=100.
    pub status: u8,
    pub _pad: u8,
}

impl Vitals {
    #[inline]
    #[must_use]
    pub const fn health_q(&self) -> Q {
        Q::new(self.health)
    }

    #[inline]
    #[must_use]
    pub const fn energy_q(&self) -> Q {
        Q::new(self.energy)
    }

    #[inline]
    #[must_use]
    pub const fn mood_value(&self) -> Mood {
        Mood::new(self.mood)
    }

    #[inline]
    #[must_use]
    pub const fn education(&self) -> EduLevel {
        EduLevel::from_u8(self.edu_level)
    }

    #[inline]
    #[must_use]
    pub const fn field(&self) -> EduField {
        EduField::from_u8(self.edu_field)
    }
}

/// Dwanaście potrzeb w skali Q, indeksowanych `NeedKind` (16 B).
///
/// `updated_at` to `SimMinute` obcięte do `u32` — starcza na 8100 lat gry. Poziomy
/// nie są odejmowane przyrostowo: spadek liczy się z **różnicy funkcji czasu absolutnego**
/// (`needs::decay_between`), więc shardowanie 1/60 daje ten sam wynik co przeliczanie
/// wszystkich co minutę (WP2, tolerancja 0).
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Needs {
    pub level: [u8; magnat_core::NEED_COUNT],
    pub updated_at: u32,
}

impl Default for Needs {
    fn default() -> Self {
        Needs {
            level: [100; magnat_core::NEED_COUNT],
            updated_at: 0,
        }
    }
}

impl Needs {
    #[inline]
    #[must_use]
    pub fn get(&self, n: magnat_core::NeedKind) -> Q {
        Q::new(self.level[n.as_index()])
    }

    #[inline]
    pub fn set(&mut self, n: magnat_core::NeedKind, v: Q) {
        self.level[n.as_index()] = v.get();
    }
}

/// Jedno stanowisko umiejętności — 4 B.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct SkillSlot {
    /// `JobRoleId`.
    pub role: u16,
    /// Q 0..=100.
    pub level: u8,
    /// Tempo zaniku przy nieużywaniu, w setnych punktu na dobę.
    pub decay: u8,
}

/// Cztery najlepsze umiejętności — 16 B. Kto ma ich więcej (poniżej 3 % populacji),
/// trzyma resztę w bocznej mapie po stronie M7.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Skills(pub [SkillSlot; 4]);

impl Skills {
    pub const ROLE_NONE: u16 = u16::MAX;

    /// Poziom w roli albo `Q::MIN`. Liniowe szukanie po czterech elementach —
    /// mapa byłaby wolniejsza i większa.
    #[must_use]
    pub fn level_in(&self, role: u16) -> Q {
        for s in &self.0 {
            if s.role == role {
                return Q::new(s.level);
            }
        }
        Q::MIN
    }
}

/// Majątek osobisty — 16 B. **Pola, nie operacje**: salda zmienia M5.
/// Gotówka gospodarstwa, konto, oszczędności i dług siedzą w `Household` (M3c §5.6) —
/// GD jest jednostką ekonomiczną, mieszkaniec ma tylko portfel.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Wealth {
    pub cash: Money,
    pub personal_assets: Money,
}

/// Pora zmiany. Mówi **o której**, nie **w które dni** — te są w `Employment.work_days`,
/// bo zmiana weekendowa (PRD §5.5) to inna maska przy tej samej porze.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum ShiftKind {
    /// 6–14.
    Early = 0,
    /// 8–16.
    #[default]
    Day = 1,
    /// 14–22.
    Afternoon = 2,
    /// 22–6, przez północ.
    Night = 3,
    /// Ruchoma, rdzeń 10–15.
    Flex = 4,
    /// 9–17, ale maska dni obejmuje sobotę i niedzielę.
    Weekend = 5,
}

impl ShiftKind {
    #[must_use]
    pub const fn from_u8(v: u8) -> ShiftKind {
        match v {
            0 => ShiftKind::Early,
            2 => ShiftKind::Afternoon,
            3 => ShiftKind::Night,
            4 => ShiftKind::Flex,
            5 => ShiftKind::Weekend,
            _ => ShiftKind::Day,
        }
    }

    /// Okno zmiany jako `(początek, koniec)`. Zmiana nocna zawija się przez północ —
    /// koniec jest wtedy **mniejszy** od początku i planer musi to widzieć, a nie
    /// dostawać 1800 minut.
    #[must_use]
    pub const fn window(self) -> (MinuteOfDay, MinuteOfDay) {
        let (a, b) = match self {
            ShiftKind::Early => (6 * 60, 14 * 60),
            ShiftKind::Day => (8 * 60, 16 * 60),
            ShiftKind::Afternoon => (14 * 60, 22 * 60),
            ShiftKind::Night => (22 * 60, 6 * 60),
            ShiftKind::Flex => (10 * 60, 15 * 60),
            ShiftKind::Weekend => (9 * 60, 17 * 60),
        };
        (MinuteOfDay::new(a), MinuteOfDay::new(b))
    }

    #[inline]
    #[must_use]
    pub const fn crosses_midnight(self) -> bool {
        matches!(self, ShiftKind::Night)
    }
}

/// Zatrudnienie — 12 B. W M3 **statyczne z generacji**: zmienia je wyłącznie zdarzenie
/// demograficzne (emerytura, śmierć, migracja). Rynek pracy przejmuje M7 (decyzja 9.5).
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Employment {
    /// `SiteId` jako indeks encji; `Employment::NO_SITE` = brak pracy.
    pub site: u32,
    /// `JobRoleId`.
    pub role: u16,
    /// `ShiftKind`.
    pub shift: u8,
    /// bit0 uczeń, bit1 student, bit2 emeryt, bit3 bezrobotny, bit4 na zwolnieniu.
    pub flags: u8,
    /// Czas dojazdu z generacji — wejście do walidacji histogramu (Etap 8).
    pub commute_baseline_min: u16,
    /// Maska `DayOfWeek` (K-15): bit N = pracuje w dniu N. Nigdy nie liczona z dnia
    /// miesiąca — przy roku 360-dniowym tydzień dryfuje względem miesiąca.
    pub work_days: u8,
    pub _pad: u8,
}

impl Default for Employment {
    fn default() -> Self {
        Employment {
            site: Employment::NO_SITE,
            role: Skills::ROLE_NONE,
            shift: ShiftKind::Day as u8,
            flags: Employment::FLAG_UNEMPLOYED,
            commute_baseline_min: 0,
            work_days: 0,
            _pad: 0,
        }
    }
}

impl Employment {
    pub const NO_SITE: u32 = u32::MAX;
    pub const FLAG_PUPIL: u8 = 1 << 0;
    pub const FLAG_STUDENT: u8 = 1 << 1;
    pub const FLAG_RETIRED: u8 = 1 << 2;
    pub const FLAG_UNEMPLOYED: u8 = 1 << 3;
    pub const FLAG_SICK_LEAVE: u8 = 1 << 4;

    /// Poniedziałek–piątek. Punkt wyjścia, nie stała: maska weekendowa to `0b110_0000`.
    pub const WEEKDAYS: u8 = 0b001_1111;

    /// Czy `site` w ogóle na coś wskazuje. **To nie jest „pracuje"** — uczeń ma
    /// w `site` swoją szkołę (`E-19` w M3d), więc przechodzi przez ten warunek tak
    /// samo jak pracownik. Pytanie „czy pracuje" ma własną metodę niżej i to jej
    /// używa kod, który liczy zatrudnienie, płace i relacje w zakładzie.
    #[inline]
    #[must_use]
    pub const fn has_job(&self) -> bool {
        self.site != Employment::NO_SITE
    }

    #[inline]
    #[must_use]
    pub const fn is_pupil(&self) -> bool {
        self.flags & (Employment::FLAG_PUPIL | Employment::FLAG_STUDENT) != 0
    }

    /// Czy mieszkaniec **pracuje**: ma miejsce i nie jest ono szkołą (`R2-WP1`).
    ///
    /// Rozdzielenie ucznia od pracownika idzie jawną metodą, a nie nową flagą, bo
    /// flaga już jest — brakowało tylko jednego miejsca, w którym obie odpowiedzi
    /// spotykają się w jednym wyrażeniu. Przed tą metodą uczeń wchodził do indeksu
    /// miejsc pracy i dostawał relacje `Colleague` z klasą, a `M10e` §5.9 liczy
    /// warunek uzwiązkowienia właśnie na spójnej składowej tego indeksu — pierwszą
    /// kandydatką do uzwiązkowienia była szkoła podstawowa.
    #[inline]
    #[must_use]
    pub const fn is_employed(&self) -> bool {
        self.has_job() && !self.is_pupil()
    }

    /// Czy pracuje w tym dniu tygodnia. **Jedyna** droga do tej odpowiedzi — liczenie
    /// `dzień % 30` albo zakładanie 22 dni roboczych w miesiącu łamie K-15 po cichu.
    #[inline]
    #[must_use]
    pub const fn works_on(&self, dow: DayOfWeek) -> bool {
        self.has_job() && (self.work_days & (1 << (dow as u8))) != 0
    }

    #[inline]
    #[must_use]
    pub const fn shift_kind(&self) -> ShiftKind {
        ShiftKind::from_u8(self.shift)
    }
}

/// Miejsce zamieszkania — 8 B.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Residence {
    /// `BuildingId` jako indeks encji; `Residence::HOMELESS` = bez przypisanego lokalu.
    pub building: u32,
    /// Numer lokalu w budynku.
    pub unit: u16,
    /// `DistrictId`.
    pub district: u16,
}

impl Default for Residence {
    fn default() -> Self {
        Residence {
            building: Residence::HOMELESS,
            unit: 0,
            district: 0,
        }
    }
}

impl Residence {
    pub const HOMELESS: u32 = u32::MAX;

    #[inline]
    #[must_use]
    pub const fn has_home(&self) -> bool {
        self.building != Residence::HOMELESS
    }
}

/// Uchwyt do planu dnia w arenie — 8 B. Plan starszy niż `plan_day` jest nieaktualny
/// i nie trzeba go z niczego usuwać (lazy scheduling, §5.2).
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PlanRef {
    /// Uchwyt do slabu planów: klasa rozmiaru w bitach 29..31, numer bloku niżej.
    pub offset: u32,
    /// Liczba slotów, ≤ 24.
    pub len: u8,
    /// Indeks bieżącego slotu.
    pub cursor: u8,
    /// Doba świata mod 65536 — wykrywa nieaktualny plan.
    pub plan_day: u16,
}

impl Default for PlanRef {
    fn default() -> Self {
        PlanRef {
            offset: PlanRef::NO_PLAN,
            len: 0,
            cursor: 0,
            plan_day: 0,
        }
    }
}

impl PlanRef {
    pub const MAX_SLOTS: usize = 24;
    /// Brak planu. `offset` jest uchwytem do slabu, a nie przesunięciem w buforze —
    /// klasa rozmiaru siedzi w trzech najstarszych bitach (§5.1, korekta po pomiarze).
    pub const NO_PLAN: u32 = u32::MAX;

    const CLASS_SHIFT: u32 = 29;
    const HANDLE_MASK: u32 = (1 << PlanRef::CLASS_SHIFT) - 1;

    #[inline]
    #[must_use]
    pub const fn is_current(&self, day: u64) -> bool {
        self.len > 0 && self.plan_day == (day % 65_536) as u16
    }

    /// Uchwyt do slabu planów. Upakowanie klasy w `offset` zamiast dołożenia pola:
    /// `PlanRef` ma mieć 8 B (tabela budżetu §5.1), a 2^29 bloków to pół miliarda
    /// planów — trzy rzędy wielkości ponad metropolię.
    #[inline]
    #[must_use]
    pub const fn slab_ref(&self) -> SlabRef {
        if self.offset == PlanRef::NO_PLAN {
            return SlabRef::EMPTY;
        }
        SlabRef {
            handle: self.offset & PlanRef::HANDLE_MASK,
            len: self.len,
            class: (self.offset >> PlanRef::CLASS_SHIFT) as u8,
        }
    }

    #[inline]
    pub fn set_slab_ref(&mut self, r: SlabRef) {
        if r.is_empty() {
            self.offset = PlanRef::NO_PLAN;
            self.len = 0;
            return;
        }
        debug_assert!(r.handle <= PlanRef::HANDLE_MASK && r.class < 8);
        self.offset = (u32::from(r.class) << PlanRef::CLASS_SHIFT) | r.handle;
        self.len = r.len;
    }
}

/// Poziom szczegółowości agenta (00 §4). Mikro jest **wizualizatorem bez prawa zapisu**
/// do stanu ekonomicznego: zbiór encji w Mikro zależy od kamery, a kamera nie wchodzi
/// do hasha stanu.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum Lod {
    #[default]
    Meso = 0,
    Micro = 1,
}

/// Bieżący stan agenta — 8 B.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct AgentState {
    /// `ActivityKind`.
    pub activity: u8,
    /// `Lod`.
    pub lod: u8,
    /// Minuty do końca debouncingu przeplanowań (§5.2).
    pub replan_cooldown: u8,
    pub flags: u8,
    /// Mezo: indeks odcinka ulicy. Mikro: indeks w `PedestrianBuffer`.
    pub position: u32,
}

impl AgentState {
    #[inline]
    #[must_use]
    pub fn activity_kind(&self) -> ActivityKind {
        ActivityKind::from_index(self.activity as usize).unwrap_or(ActivityKind::Idle)
    }

    #[inline]
    #[must_use]
    pub const fn is_micro(&self) -> bool {
        self.lod == Lod::Micro as u8
    }
}

/// Uchwyt do slabu wiedzy/doświadczeń — 8 B. `class` wskazuje klasę rozmiaru slabu,
/// `handle` blok w jej obrębie (patrz `store::Slab`).
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct KnowledgeRef {
    pub handle: u32,
    pub len: u8,
    pub class: u8,
    pub _pad: u16,
}

/// Uchwyt do slabu relacji — 8 B.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct RelationsRef {
    pub handle: u32,
    pub len: u8,
    pub class: u8,
    pub _pad: u16,
}

/// Uchwyt do slabu slotów marek — 8 B (M10b §5.1).
///
/// Osobny slab od wiedzy, choć kształt uchwytu jest ten sam, i to jest decyzja:
/// wpis wiedzy wskazuje **miejsce** (indeks encji), slot marki wskazuje **markę**
/// (`BrandId`), a limity są różne — 32 wpisy wiedzy wobec 16 slotów marek. Wspólny
/// magazyn znaczyłby, że kampania reklamowa wypycha z pamięci przystanek autobusowy.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct BrandsRef {
    pub handle: u32,
    pub len: u8,
    pub class: u8,
    pub _pad: u16,
}

/// Cykl życia — 8 B. `next_event_day` to doba najbliższego zdarzenia życiowego;
/// samo zdarzenie siedzi w przelewie kolejki DES, nie w kole czasu.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Lifecycle {
    /// Indeks encji partnera; `Lifecycle::NO_PARTNER` = brak.
    pub partner: u32,
    pub next_event_day: u16,
    /// bit0 w związku, bit1 w ciąży, bit2 chory, bit3 na emeryturze.
    pub flags: u16,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Lifecycle {
            partner: Lifecycle::NO_PARTNER,
            next_event_day: 0,
            flags: 0,
        }
    }
}

impl Lifecycle {
    pub const NO_PARTNER: u32 = u32::MAX;
    pub const FLAG_PARTNERED: u16 = 1 << 0;
    pub const FLAG_PREGNANT: u16 = 1 << 1;
    pub const FLAG_ILL: u16 = 1 << 2;
    pub const FLAG_RETIRED: u16 = 1 << 3;

    /// Czy mieszkaniec jest dziś na zwolnieniu. Do M8d flagę czytała wyłącznie
    /// demografia, która ją stawia; od M8d czyta ją też rynek pracy — chory nie
    /// wlicza się do pokrycia etatowego zakładu (`K-44`).
    #[inline]
    #[must_use]
    pub const fn is_ill(&self) -> bool {
        self.flags & Lifecycle::FLAG_ILL != 0
    }
}

// ── hash stanu (00 §3.6) ────────────────────────────────────────────────────────
// Pola `_pad` są poza hashem: są zawsze zerem i nie są stanem. Gdyby kiedykolwiek
// przestały nim być, przestaną się też nazywać `_pad`.

impl HashState for Identity {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u16(self.first_name);
        h.write_u16(self.last_name);
        h.write_u32(self.birth_day as u32);
        h.write_u16(self.birth_district);
        h.write_u8(self.flags);
        h.write_u32(self.household);
    }
}

impl HashState for Personality {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write(&self.0);
    }
}

impl HashState for Vitals {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write(&[
            self.health,
            self.energy,
            self.mood as u8,
            self.stress,
            self.edu_level,
            self.edu_field,
            self.status,
        ]);
    }
}

impl HashState for Needs {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write(&self.level);
        h.write_u32(self.updated_at);
    }
}

impl HashState for Skills {
    fn hash_state(&self, h: &mut StateHasher) {
        for s in &self.0 {
            h.write_u16(s.role);
            h.write_u8(s.level);
            h.write_u8(s.decay);
        }
    }
}

impl HashState for Wealth {
    fn hash_state(&self, h: &mut StateHasher) {
        self.cash.hash_state(h);
        self.personal_assets.hash_state(h);
    }
}

impl HashState for Employment {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.site);
        h.write_u16(self.role);
        h.write(&[self.shift, self.flags]);
        h.write_u16(self.commute_baseline_min);
        h.write_u8(self.work_days);
    }
}

impl HashState for Residence {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.building);
        h.write_u16(self.unit);
        h.write_u16(self.district);
    }
}

impl HashState for PlanRef {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.offset);
        h.write(&[self.len, self.cursor]);
        h.write_u16(self.plan_day);
    }
}

impl HashState for AgentState {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write(&[self.activity, self.lod, self.replan_cooldown, self.flags]);
        h.write_u32(self.position);
    }
}

impl HashState for KnowledgeRef {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.handle);
        h.write(&[self.len, self.class]);
    }
}

impl HashState for RelationsRef {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.handle);
        h.write(&[self.len, self.class]);
    }
}

impl HashState for BrandsRef {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.handle);
        h.write(&[self.len, self.class]);
    }
}

impl HashState for Lifecycle {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.partner);
        h.write_u16(self.next_event_day);
        h.write_u16(self.flags);
    }
}

macro_rules! komponent {
    ($($t:ty => $name:literal),+ $(,)?) => {$(
        impl Component for $t {
            const NAME: &'static str = $name;
        }
    )+};
}

komponent! {
    Identity => "agents.Identity",
    Personality => "agents.Personality",
    Vitals => "agents.Vitals",
    Needs => "agents.Needs",
    Skills => "agents.Skills",
    Wealth => "agents.Wealth",
    Employment => "agents.Employment",
    Residence => "agents.Residence",
    PlanRef => "agents.PlanRef",
    AgentState => "agents.AgentState",
    KnowledgeRef => "agents.KnowledgeRef",
    RelationsRef => "agents.RelationsRef",
    BrandsRef => "agents.BrandsRef",
    Lifecycle => "agents.Lifecycle",
}

/// Suma `size_of` wszystkich czternastu komponentów mieszkańca — 148 B (§5.1).
///
/// Czternasty to `BrandsRef` (M10b): **uchwyt** do slabu marek, nie same marki.
/// Sloty leżą w slabie razem z wiedzą i relacjami, więc stan gorący rośnie o osiem
/// bajtów, a nie o sto dwadzieścia osiem.
pub const HOT_COMPONENT_BYTES: usize = size_of::<Identity>()
    + size_of::<Personality>()
    + size_of::<Vitals>()
    + size_of::<Needs>()
    + size_of::<Skills>()
    + size_of::<Wealth>()
    + size_of::<Employment>()
    + size_of::<Residence>()
    + size_of::<PlanRef>()
    + size_of::<AgentState>()
    + size_of::<KnowledgeRef>()
    + size_of::<RelationsRef>()
    + size_of::<BrandsRef>()
    + size_of::<Lifecycle>();

/// Rejestracja komponentów, magazynów i zasobów M3 w świecie ECS.
///
/// Jedno miejsce zamiast trzynastu wywołań w każdym scenariuszu: komponent
/// niezarejestrowany nie wchodzi do hasha stanu (00 §3.6) i rozjazd przeszedłby
/// przez CI niezauważony — dokładnie tak, jak K-16 opisuje to dla aren.
///
/// Tabela potrzeb wchodzi jako parametr, a nie jest tu wczytywana: dane ładuje
/// i waliduje wywołujący (00 §5), a rejestracja nie ma prawa się wywalić na I/O.
/// Sama tabela **nie jest** haszowana — to dane wejściowe, nie stan świata.
pub fn register(world: &mut magnat_ecs::World, needs: NeedTable) {
    register_components(world);
    register_resources(world, needs);
}

/// Same komponenty — bez zasobów i bez haków hasha.
///
/// Rozdzielone, bo minimalny snapshot z M0 **nie niesie jeszcze zasobów**: `load_world`
/// zwraca świat z komponentami, ale bez slabów i bez kolejki zdarzeń, więc świat
/// z zarejestrowanymi hakami zasobów nie przechodzi zapisu i odczytu bez rozjazdu
/// hasha. Pełne wersjonowanie sekcji zapisu należy do M12 (00 §1) — do tego czasu
/// test round-tripu używa tej funkcji, a scenariusze produkcyjne `register`.
pub fn register_components(world: &mut magnat_ecs::World) {
    world.register_component::<Identity>();
    world.register_component::<Personality>();
    world.register_component::<Vitals>();
    world.register_component::<Needs>();
    world.register_component::<Skills>();
    world.register_component::<Wealth>();
    world.register_component::<Employment>();
    world.register_component::<Residence>();
    world.register_component::<PlanRef>();
    world.register_component::<AgentState>();
    world.register_component::<KnowledgeRef>();
    world.register_component::<RelationsRef>();
    world.register_component::<BrandsRef>();
    world.register_component::<Lifecycle>();
    // Gospodarstwo jest osobną encją, nie komponentem mieszkańca (M3c §5.6) —
    // rejestruje się je tutaj, bo snapshot ma je nieść tak samo jak resztę.
    world.register_component::<crate::household::Household>();
}

/// Zasoby fazy i ich haki do hasha stanu.
pub fn register_resources(world: &mut magnat_ecs::World, needs: NeedTable) {
    world.insert_resource(needs);
    world.insert_resource(RelationSlab::new());
    world.insert_resource(KnowledgeSlab::new());
    world.insert_resource(PlanSlab::new());
    world.insert_resource(crate::brand::BrandSlab::new());
    world.insert_resource(crate::brand::BrandData::default());
    world.insert_resource(EventQueue::new());
    world.register_resource_hash::<RelationSlab>();
    world.register_resource_hash::<KnowledgeSlab>();
    world.register_resource_hash::<PlanSlab>();
    // Sloty marek są stanem świata tak samo jak wiedza: to, co mieszkaniec myśli
    // o marce, wchodzi do jego decyzji zakupowej (M10b §5.1). Kalibracja z
    // `data/tuning/brand.ron` **nie** wchodzi — to dane wejściowe, jak tabela potrzeb.
    world.register_resource_hash::<crate::brand::BrandSlab>();
    // Kolejka zdarzeń jest stanem trwałym: to, co ma się wydarzyć jutro, jest częścią
    // świata tak samo jak to, co jest dziś (§5.2, lazy scheduling).
    world.register_resource_hash::<EventQueue>();
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::NeedKind;

    #[test]
    fn rozmiary_komponentow_zgadzaja_sie_z_budzetem() {
        // Tabela z M3a §5.1. Zmiana którejkolwiek liczby wymaga przeliczenia budżetu
        // §17.7 na nowo, a nie poprawienia testu.
        assert_eq!(size_of::<Identity>(), 16);
        assert_eq!(size_of::<Personality>(), 8);
        assert_eq!(size_of::<Vitals>(), 8);
        assert_eq!(size_of::<Needs>(), 16);
        assert_eq!(size_of::<SkillSlot>(), 4);
        assert_eq!(size_of::<Skills>(), 16);
        assert_eq!(size_of::<Wealth>(), 16);
        assert_eq!(size_of::<Employment>(), 12);
        assert_eq!(size_of::<Residence>(), 8);
        assert_eq!(size_of::<PlanRef>(), 8);
        assert_eq!(size_of::<AgentState>(), 8);
        assert_eq!(size_of::<KnowledgeRef>(), 8);
        assert_eq!(size_of::<RelationsRef>(), 8);
        // Czternasty komponent, dołożony w M10b: uchwyt do slabu marek. Sloty leżą
        // w slabie razem z wiedzą i relacjami, więc stan gorący rośnie o osiem bajtów,
        // a nie o sto dwadzieścia osiem (M10b §5.1, wariant C).
        assert_eq!(size_of::<BrandsRef>(), 8);
        assert_eq!(size_of::<Lifecycle>(), 8);
        assert_eq!(HOT_COMPONENT_BYTES, 148);
    }

    #[test]
    fn brak_paddingu_niejawnego() {
        // `offset_of` pilnuje, że pola leżą tam, gdzie mówi tabela — sam `size_of`
        // by tego nie wyłapał, bo kompilator mógłby przestawić padding między pola.
        assert_eq!(std::mem::offset_of!(Identity, birth_day), 4);
        assert_eq!(std::mem::offset_of!(Identity, household), 12);
        assert_eq!(std::mem::offset_of!(Needs, updated_at), 12);
        assert_eq!(std::mem::offset_of!(Employment, commute_baseline_min), 8);
        assert_eq!(std::mem::offset_of!(AgentState, position), 4);
    }

    #[test]
    fn dni_robocze_ida_z_dnia_tygodnia_a_nie_z_dnia_miesiaca() {
        // R13: `day % 30` daje inną odpowiedź niż `DayOfWeek` już po pierwszym miesiącu.
        let e = Employment {
            site: 7,
            work_days: Employment::WEEKDAYS,
            ..Employment::default()
        };
        assert!(e.works_on(DayOfWeek::Monday));
        assert!(e.works_on(DayOfWeek::Friday));
        assert!(!e.works_on(DayOfWeek::Saturday));

        let weekendowa = Employment {
            site: 7,
            shift: ShiftKind::Weekend as u8,
            work_days: 0b110_0000,
            ..Employment::default()
        };
        assert!(weekendowa.works_on(DayOfWeek::Sunday));
        assert!(!weekendowa.works_on(DayOfWeek::Wednesday));

        // Bez pracy żadna maska nie czyni dnia roboczym.
        assert!(!Employment {
            work_days: 0xFF,
            ..Employment::default()
        }
        .works_on(DayOfWeek::Monday));
    }

    #[test]
    fn zmiana_nocna_zawija_sie_przez_polnoc() {
        let (a, b) = ShiftKind::Night.window();
        assert_eq!(a, MinuteOfDay::new(22 * 60));
        assert_eq!(b, MinuteOfDay::new(6 * 60));
        assert!(ShiftKind::Night.crosses_midnight());
        assert!(!ShiftKind::Day.crosses_midnight());
    }

    #[test]
    fn potrzeby_indeksuja_sie_slownikiem_a_nie_liczba() {
        let mut n = Needs::default();
        n.set(NeedKind::Sleep, Q::new(42));
        assert_eq!(n.get(NeedKind::Sleep), Q::new(42));
        assert_eq!(n.get(NeedKind::Hunger), Q::MAX);
        assert_eq!(n.level[NeedKind::Sleep.as_index()], 42);
    }

    #[test]
    fn wiek_liczy_sie_w_roku_360_dniowym() {
        let i = Identity {
            birth_day: -360 * 30,
            ..Identity::default()
        };
        assert_eq!(i.age_years(0), 30);
        assert_eq!(i.age_years(360), 31);
    }
}
