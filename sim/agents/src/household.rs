//! Gospodarstwo domowe — jednostka ekonomiczna i mieszkaniowa (M3c §5.6).
//!
//! Mieszkaniec ma portfel (`Wealth`), gospodarstwo ma budżet: gotówkę, konto,
//! oszczędności, dług i zapasy. W M3 to są **pola, nie operacje** — saldami zarządza
//! M5, a zapasy są abstrakcyjnymi „dniami" indeksowanymi `StockCat` (K-20, korekta C-6).
//!
//! Sześciu członków siedzi w komponencie, siódmy i dalsi w **przelewie**
//! (`HouseholdOverflow`). Nie jest to ozdoba: rodzina wielopokoleniowa z trojgiem
//! dzieci ma siedem osób i zdarza się w każdej symulacji, a obcięcie listy zgubiłoby
//! mieszkańca w sposób, którego `prop_no_orphan_household` nie odróżniłby od błędu
//! księgowego. Przelew jest `BTreeMap`, więc iteruje się deterministycznie (00 §3.2).

use crate::arrayvec::ArrayVec;
use crate::components::{Employment, Identity, ShiftKind};
use magnat_core::{HashState, Money, StateHasher, STOCK_CAT_COUNT};
use magnat_ecs::Component;
use std::collections::BTreeMap;

/// Typ gospodarstwa. Wyliczany ze składu przy każdej zmianie (`Household::classify`),
/// a nie deklarowany — inaczej rodzina po wyprowadzce dzieci zostawałaby na zawsze
/// „rodziną z dziećmi".
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum HouseholdKind {
    #[default]
    Single = 0,
    Couple = 1,
    FamilyWithKids = 2,
    MultiGen = 3,
    Roommates = 4,
    Dorm = 5,
    LoneSenior = 6,
}

impl HouseholdKind {
    #[must_use]
    pub const fn from_u8(v: u8) -> HouseholdKind {
        match v {
            1 => HouseholdKind::Couple,
            2 => HouseholdKind::FamilyWithKids,
            3 => HouseholdKind::MultiGen,
            4 => HouseholdKind::Roommates,
            5 => HouseholdKind::Dorm,
            6 => HouseholdKind::LoneSenior,
            _ => HouseholdKind::Single,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            HouseholdKind::Single => "Single",
            HouseholdKind::Couple => "Couple",
            HouseholdKind::FamilyWithKids => "FamilyWithKids",
            HouseholdKind::MultiGen => "MultiGen",
            HouseholdKind::Roommates => "Roommates",
            HouseholdKind::Dorm => "Dorm",
            HouseholdKind::LoneSenior => "LoneSenior",
        }
    }
}

/// Ilu członków mieści się w samym komponencie; reszta idzie do przelewu.
pub const HH_INLINE_MEMBERS: usize = 6;

/// Twardy sufit liczebności gospodarstwa. Nie jest to limit modelu, tylko limit
/// jednorazowej alokacji przy odczycie składu — dziesięcioosobowe gospodarstwo
/// wielopokoleniowe mieści się z zapasem.
pub const HH_MAX_MEMBERS: usize = 12;

/// Gospodarstwo domowe — **120 B** (§5.6 mówi 128; korekta G-1).
///
/// Suma pól wypisanych w §5.6 daje 120 bajtów i nie ma tam czego dołożyć — etykieta
/// „128 B" była zaokrągleniem w górę, nie rachunkiem. Osiem bajtów mniej na gospodarstwo
/// to 1,3 MB przy 167 tys. gospodarstw metropolii.
///
/// Rezerwa dla M5/M9 (ryzyko R5) schudła w `R2-WP4` z 16 B do 12 B — cztery bajty
/// wziął `guardian`. Struktura zostaje 120 B i to jest cały sens tej rezerwy:
/// jest miejscem na pole, które właśnie dostało czytelnika, a nie nietykalnym zapasem.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Household {
    /// `HouseholdKind`.
    pub kind: u8,
    /// Liczba członków **łącznie z przelewem**.
    pub size: u8,
    /// bit0 aktywne (ma przynajmniej jednego członka).
    pub flags: u16,
    /// `DistrictId`.
    pub district: u16,
    pub _pad: u16,
    /// Indeksy encji członków; `Household::NO_MEMBER` = puste miejsce.
    pub members: [u32; HH_INLINE_MEMBERS],
    /// `BuildingId` jako indeks encji; `Household::NO_BUILDING` = bez lokalu.
    pub building: u32,
    pub unit: u16,
    pub _pad2: u16,
    pub cash: Money,
    pub bank: Money,
    pub savings: Money,
    pub debt: Money,
    /// Z generacji; M5/M7 aktualizują.
    pub income_monthly: Money,
    /// Dni zapasu per `StockCat` (K-20). M5 zastąpi realnymi towarami.
    pub stock: [u8; STOCK_CAT_COUNT],
    /// Indeks członka robiącego zakupy; rotuje wg grafików.
    pub shopper_rotation: u8,
    /// Wyrównanie do `vehicle_slots`. **Skróciło się z 3 B do 2 B w M10c**, bo
    /// `StockCat` dostał dziewiąty wariant (`K-83`) i `stock` urosło o bajt.
    /// Rezerwa wyrównania jest po to, żeby dopisanie kategorii nie przesuwało
    /// offsetów — i tu właśnie zadziałała.
    pub _pad3: [u8; 2],
    /// M4 wypełnia.
    pub vehicle_slots: [u32; 2],
    /// Opiekun prawny — indeks encji dorosłego **spoza składu**; `NO_MEMBER`, gdy
    /// gospodarstwo radzi sobie samo (`R2-WP4`, `D-N2`).
    ///
    /// Pole, a nie encja: instytucja opiekuńcza wymaga usługi publicznej z obsadą
    /// i finansowaniem, czyli należy do M8d. R2 domyka stan nieopisany — gospodarstwo
    /// z dziećmi i zerem dorosłych — a nie buduje mechaniki.
    ///
    /// Cztery bajty pochodzą **z rezerwy M5/M9** (16 B → 12 B), a nie z rozmiaru
    /// struktury: gospodarstwo zostaje 120 B. Rezerwa jest miejscem dla czytelnika,
    /// który jeszcze nie istnieje, a opiekun czytelnika ma dziś (`household::roles`,
    /// `SocialIndex::wards`).
    pub guardian: u32,
    /// Rezerwa dla M5/M9, żeby nie przebudowywać archetypu (ryzyko R5).
    pub _reserved: [u8; 12],
    pub _pad4: [u8; 4],
}

impl Default for Household {
    fn default() -> Self {
        Household {
            kind: HouseholdKind::Single as u8,
            size: 0,
            flags: 0,
            district: 0,
            _pad: 0,
            members: [Household::NO_MEMBER; HH_INLINE_MEMBERS],
            building: Household::NO_BUILDING,
            unit: 0,
            _pad2: 0,
            cash: Money::ZERO,
            bank: Money::ZERO,
            savings: Money::ZERO,
            debt: Money::ZERO,
            income_monthly: Money::ZERO,
            stock: [0; STOCK_CAT_COUNT],
            shopper_rotation: 0,
            _pad3: [0; 2],
            vehicle_slots: [u32::MAX; 2],
            guardian: Household::NO_MEMBER,
            _reserved: [0; 12],
            _pad4: [0; 4],
        }
    }
}

impl Household {
    pub const NO_MEMBER: u32 = u32::MAX;
    pub const NO_BUILDING: u32 = u32::MAX;
    pub const FLAG_ACTIVE: u16 = 1 << 0;
    /// Gospodarstwo **dzieli cudzy lokal** (`R2-WP3`).
    ///
    /// Powstaje wtedy, gdy do gospodarstwa o pełnym składzie (`HH_MAX_MEMBERS`)
    /// urodzi się dziecko: zamiast zginąć w milczeniu, zakłada gospodarstwo pochodne
    /// pod tym samym adresem. Jego rozmiar **jest** liczbą osób ponad limit.
    ///
    /// Flaga niesie też skutek księgowy: lokal należy do gospodarstwa pierwotnego,
    /// więc pochodne **nie oddaje go do puli pustostanów**, kiedy się rozwiązuje.
    /// Bez tego jeden lokal wracałby do puli dwa razy i miasto miałoby mieszkania,
    /// których nie ma.
    pub const FLAG_OVERCROWDED: u16 = 1 << 1;

    #[inline]
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.flags & Household::FLAG_ACTIVE != 0
    }

    /// Czy gospodarstwo mieszka kątem u innego — patrz [`Household::FLAG_OVERCROWDED`].
    #[inline]
    #[must_use]
    pub const fn is_overcrowded(&self) -> bool {
        self.flags & Household::FLAG_OVERCROWDED != 0
    }

    /// Opiekun prawny, jeśli jest — patrz [`Household::guardian`].
    #[inline]
    #[must_use]
    pub const fn guardian_of(&self) -> Option<u32> {
        if self.guardian == Household::NO_MEMBER {
            None
        } else {
            Some(self.guardian)
        }
    }

    #[inline]
    #[must_use]
    pub const fn has_home(&self) -> bool {
        self.building != Household::NO_BUILDING
    }

    #[inline]
    #[must_use]
    pub const fn household_kind(&self) -> HouseholdKind {
        HouseholdKind::from_u8(self.kind)
    }

    /// Członkowie siedzący w samym komponencie.
    pub fn inline_members(&self) -> impl Iterator<Item = u32> + '_ {
        self.members
            .iter()
            .copied()
            .filter(|m| *m != Household::NO_MEMBER)
    }

    #[must_use]
    pub fn contains_inline(&self, member: u32) -> bool {
        self.members.contains(&member)
    }
}

/// Członkowie ponad szóstkę, kluczowane indeksem encji gospodarstwa.
///
/// `BTreeMap`, nie `HashMap`: 00 §3.2 zakazuje iterowania po `HashMap` w kodzie
/// symulacji, a ten magazyn wchodzi do hasha stanu.
#[derive(Clone, Debug, Default)]
pub struct HouseholdOverflow {
    by_household: BTreeMap<u32, Vec<u32>>,
}

impl HouseholdOverflow {
    #[must_use]
    pub fn new() -> HouseholdOverflow {
        HouseholdOverflow::default()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_household.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_household.is_empty()
    }

    #[must_use]
    pub fn get(&self, household: u32) -> &[u32] {
        self.by_household
            .get(&household)
            .map_or(&[][..], Vec::as_slice)
    }

    fn push(&mut self, household: u32, member: u32) {
        let v = self.by_household.entry(household).or_default();
        if !v.contains(&member) {
            v.push(member);
        }
    }

    fn remove(&mut self, household: u32, member: u32) -> bool {
        let Some(v) = self.by_household.get_mut(&household) else {
            return false;
        };
        let Some(i) = v.iter().position(|m| *m == member) else {
            return false;
        };
        v.remove(i);
        if v.is_empty() {
            self.by_household.remove(&household);
        }
        true
    }

    /// Wyjmuje pierwszego z przelewu — używane, gdy w komponencie zwalnia się miejsce.
    fn pop(&mut self, household: u32) -> Option<u32> {
        let v = self.by_household.get_mut(&household)?;
        let m = if v.is_empty() {
            None
        } else {
            Some(v.remove(0))
        };
        if v.is_empty() {
            self.by_household.remove(&household);
        }
        m
    }

    pub fn clear_household(&mut self, household: u32) {
        self.by_household.remove(&household);
    }
}

impl HashState for HouseholdOverflow {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.by_household.len() as u64);
        for (hh, members) in &self.by_household {
            h.write_u32(*hh);
            h.write_u64(members.len() as u64);
            for m in members {
                h.write_u32(*m);
            }
        }
    }
}

/// Skład gospodarstwa: komponent plus przelew, w kolejności wstawiania.
#[must_use]
pub fn members_of(
    household: u32,
    hh: &Household,
    overflow: &HouseholdOverflow,
) -> ArrayVec<u32, HH_MAX_MEMBERS> {
    let mut out: ArrayVec<u32, HH_MAX_MEMBERS> = ArrayVec::new();
    for m in hh.inline_members() {
        if !out.is_full() {
            out.push(m);
        }
    }
    for m in overflow.get(household) {
        if !out.is_full() {
            out.push(*m);
        }
    }
    out
}

/// Dopisuje członka. Zwraca `false`, gdy gospodarstwo osiągnęło `HH_MAX_MEMBERS` —
/// wywołujący ma wtedy założyć nowe gospodarstwo, a nie zgubić mieszkańca.
///
/// `#[must_use]` nie jest ozdobą (`R2-WP3`): trzech z czterech wołających ignorowało
/// wynik, a mieszkaniec, którego nie dopisano, zostawał z `Identity.household`
/// wskazującym na gospodarstwo, w którego składzie go nie ma. Istniał wtedy i nie
/// istniał naraz — karta gospodarstwa pokazywała inny skład niż karta mieszkańca,
/// a role, klasyfikacja i rozmiar go nie widziały.
#[must_use]
pub fn add_member(
    household: u32,
    hh: &mut Household,
    overflow: &mut HouseholdOverflow,
    member: u32,
) -> bool {
    if hh.contains_inline(member) || overflow.get(household).contains(&member) {
        return true;
    }
    if hh.size as usize >= HH_MAX_MEMBERS {
        return false;
    }
    match hh.members.iter_mut().find(|m| **m == Household::NO_MEMBER) {
        Some(slot) => *slot = member,
        None => overflow.push(household, member),
    }
    hh.size += 1;
    hh.flags |= Household::FLAG_ACTIVE;
    true
}

/// Usuwa członka. Miejsce zwolnione w komponencie zabiera pierwszy z przelewu,
/// żeby lista w komponencie nie dziurawiła się na zawsze.
pub fn remove_member(
    household: u32,
    hh: &mut Household,
    overflow: &mut HouseholdOverflow,
    member: u32,
) -> bool {
    let usuniety = match hh.members.iter().position(|m| *m == member) {
        Some(i) => {
            hh.members[i] = overflow.pop(household).unwrap_or(Household::NO_MEMBER);
            true
        }
        None => overflow.remove(household, member),
    };
    if !usuniety {
        return false;
    }
    hh.size = hh.size.saturating_sub(1);
    if hh.size == 0 {
        hh.flags &= !Household::FLAG_ACTIVE;
    }
    true
}

/// Dane jednego członka potrzebne do klasyfikacji i podziału ról.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MemberView {
    pub citizen: u32,
    pub age_years: i32,
    pub is_pupil: bool,
    pub works: bool,
    /// `ShiftKind` — rozstrzyga, kto odbiera dziecko ze szkoły.
    pub shift: ShiftKind,
    /// `Employment.site` — dla ucznia jest to szkoła.
    pub site: u32,
    /// Czy jest w związku z innym członkiem tego gospodarstwa.
    pub partnered_inside: bool,
}

impl MemberView {
    /// Widok zbudowany z komponentów. Jedno miejsce, w którym „uczeń" i „pracuje"
    /// są definiowane — inaczej każdy system czytałby flagi po swojemu.
    #[must_use]
    pub fn new(
        citizen: u32,
        identity: &Identity,
        employment: &Employment,
        today: i32,
        partnered_inside: bool,
    ) -> MemberView {
        MemberView {
            citizen,
            age_years: identity.age_years(today),
            is_pupil: employment.is_pupil(),
            works: employment.is_employed(),
            shift: employment.shift_kind(),
            site: employment.site,
            partnered_inside,
        }
    }
}

/// Typ gospodarstwa wyliczony ze składu (§5.6).
///
/// Kolejność sprawdzeń jest kolejnością szczegółowości: wielopokoleniowość wygrywa
/// z rodziną z dziećmi, bo jest jej nadzbiorem.
#[must_use]
pub fn classify(members: &[MemberView], adult_age: i32, senior_age: i32) -> HouseholdKind {
    if members.is_empty() {
        return HouseholdKind::Single;
    }
    let dzieci = members.iter().filter(|m| m.age_years < adult_age).count();
    let dorosli = members.len() - dzieci;
    let seniorzy = members.iter().filter(|m| m.age_years >= senior_age).count();
    let para = members.iter().any(|m| m.partnered_inside);

    if members.len() == 1 {
        return if seniorzy == 1 {
            HouseholdKind::LoneSenior
        } else {
            HouseholdKind::Single
        };
    }
    // Trzy pokolenia: dziecko, dorosły w wieku produkcyjnym i senior pod jednym dachem.
    if dzieci > 0 && seniorzy > 0 && dorosli > seniorzy {
        return HouseholdKind::MultiGen;
    }
    if dzieci > 0 {
        return HouseholdKind::FamilyWithKids;
    }
    if para && dorosli == 2 {
        return HouseholdKind::Couple;
    }
    if dorosli >= 4 {
        HouseholdKind::Dorm
    } else {
        HouseholdKind::Roommates
    }
}

// ── podział ról (§5.6) ──────────────────────────────────────────────────────────

/// Ile dzieci naraz da się odprowadzić; planer i tak wstawia trzy sloty na szkołę.
pub const MAX_ESCORTED: usize = 4;

/// Kto co robi w gospodarstwie. Wynik jest **funkcją składu**, nie stanem — liczy się
/// go przy planowaniu doby, więc zmiana pracy albo narodziny zmieniają go natychmiast.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct HouseholdRoles {
    /// Kto odprowadza rano; `Household::NO_MEMBER` = nikt nie musi.
    pub escort: u32,
    /// Kto odbiera po południu (korekta C-8).
    pub pickup: u32,
    /// Kto robi dziś zakupy.
    pub shopper: u32,
    /// Dzieci wymagające odprowadzenia, w kolejności rosnących indeksów encji.
    pub escorted: ArrayVec<u32, MAX_ESCORTED>,
    /// Ile dzieci wymagało odprowadzenia i **go nie dostało** (`R2-WP3`).
    ///
    /// Dwa powody, jeden licznik: piąte dziecko ponad `MAX_ESCORTED` i każde dziecko
    /// w gospodarstwie bez dorosłego. Przedtem oba kończyły się `break` albo `clear()`
    /// i nie zostawiały śladu — a „dlaczego moje dziecko nie poszło do szkoły" jest
    /// pytaniem, na które karta inspekcji musi umieć odpowiedzieć (00 §7).
    pub unescorted: u8,
}

/// Podział ról dla gospodarstwa.
///
/// Odprowadza ten dorosły, którego zmiana zaczyna się **najpóźniej**; odbiera ten,
/// którego zmiana kończy się **najwcześniej**. To jest cała reguła i wynika z zegara,
/// nie z ról płciowych: szkoła kończy się o 14:00, a zmiana dzienna o 16:00, więc
/// odbiera ktoś inny niż odprowadza, o ile w gospodarstwie jest ktoś inny (C-8).
/// Remisy rozstrzyga indeks encji (00 §3.2).
///
/// `escort_age` to wiek, poniżej którego dziecko wymaga odprowadzenia (starszy uczeń
/// chodzi sam), `adult_age` — wiek, od którego mieszkaniec może odprowadzać i robić
/// zakupy. To są dwa różne progi i mylenie ich odprowadza siedemnastolatka do liceum.
///
/// `guardian` to dorosły **spoza składu** (`R2-WP4`): gospodarstwo osierocone nie ma
/// własnego dorosłego, więc bez niego lista odprowadzanych była czyszczona i dzieci
/// szły do szkoły same. Opiekun liczy się do ról dokładnie tak jak domownik — zmiana
/// rozstrzyga, czy odprowadza czy odbiera — bo tylko o to w tej funkcji chodzi.
#[must_use]
pub fn roles(
    members: &[MemberView],
    escort_age: i32,
    adult_age: i32,
    day: u64,
    guardian: Option<MemberView>,
) -> HouseholdRoles {
    let mut out = HouseholdRoles {
        escort: Household::NO_MEMBER,
        pickup: Household::NO_MEMBER,
        shopper: Household::NO_MEMBER,
        escorted: ArrayVec::new(),
        unescorted: 0,
    };
    if members.is_empty() {
        return out;
    }

    for m in members {
        if m.is_pupil && m.age_years < escort_age && m.site != Employment::NO_SITE {
            if out.escorted.is_full() {
                out.unescorted = out.unescorted.saturating_add(1);
                continue;
            }
            out.escorted.push(m.citizen);
        }
    }

    // Zakupy: rotacja po dorosłych, kluczem jest doba — więc zmienia się sama,
    // bez przechowywania licznika, i nie zależy od tego, kiedy ktoś do gospodarstwa
    // dołączył. `shopper_rotation` w komponencie zostaje dla M5, które może chcieć
    // rotować rzadziej niż codziennie.
    // Opiekun doklejony **za** składem: kolejność składu jest kolejnością wejścia
    // i nikogo nie przestawia, a on do składu nie należy. Bez kopii — iterator,
    // bo to jest ścieżka liczona dla każdego mieszkańca przy każdym planowaniu doby.
    let opiekun = guardian.filter(|g| !members.iter().any(|m| m.citizen == g.citizen));
    let wszyscy = || members.iter().chain(opiekun.as_ref());

    let doroslych = wszyscy().filter(|m| m.age_years >= adult_age).count();
    if doroslych > 0 {
        out.shopper = wszyscy()
            .filter(|m| m.age_years >= adult_age)
            .nth((day as usize) % doroslych)
            .map_or(Household::NO_MEMBER, |m| m.citizen);
    }

    if out.escorted.is_empty() {
        return out;
    }
    if doroslych == 0 {
        // Dziecko bez dorosłego w gospodarstwie nie jest odprowadzane — i to jest
        // stan do pokazania w karcie inspekcji, a nie do wygładzenia tutaj. Od
        // `R2-WP3` ma nośnik: licznik, który planer zamienia w powód decyzji.
        out.unescorted = out
            .unescorted
            .saturating_add(out.escorted.len().min(255) as u8);
        out.escorted.clear();
        return out;
    }

    // Odprowadza ten dorosły, którego zmiana zaczyna się **najpóźniej**; niepracujący
    // jest dostępny zawsze, więc liczy się jak zmiana o północy dnia następnego.
    // Odbiera ten, którego zmiana kończy się **najwcześniej**. To jest cała reguła
    // i wynika z zegara, nie z ról w rodzinie (C-8). Remisy po indeksie encji.
    let poczatek = |m: &MemberView| -> u16 {
        if m.works {
            m.shift.window().0.get()
        } else {
            u16::MAX
        }
    };
    let koniec = |m: &MemberView| -> u16 {
        if m.works {
            m.shift.window().1.get()
        } else {
            0
        }
    };
    let dorosly = |m: &&MemberView| m.age_years >= adult_age;

    out.escort = wszyscy()
        .filter(dorosly)
        .max_by_key(|m| (poczatek(m), std::cmp::Reverse(m.citizen)))
        .map_or(Household::NO_MEMBER, |m| m.citizen);

    // Jedna osoba robi oba kursy tylko wtedy, gdy jest sama.
    let escort = out.escort;
    out.pickup = wszyscy()
        .filter(dorosly)
        .filter(|m| doroslych == 1 || m.citizen != escort)
        .min_by_key(|m| (koniec(m), m.citizen))
        .map_or(escort, |m| m.citizen);
    out
}

// ── hash stanu (00 §3.6) ────────────────────────────────────────────────────────

impl HashState for Household {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write(&[self.kind, self.size]);
        h.write_u16(self.flags);
        h.write_u16(self.district);
        for m in &self.members {
            h.write_u32(*m);
        }
        h.write_u32(self.building);
        h.write_u16(self.unit);
        self.cash.hash_state(h);
        self.bank.hash_state(h);
        self.savings.hash_state(h);
        self.debt.hash_state(h);
        self.income_monthly.hash_state(h);
        h.write(&self.stock);
        h.write_u8(self.shopper_rotation);
        h.write_u32(self.vehicle_slots[0]);
        h.write_u32(self.vehicle_slots[1]);
        h.write_u32(self.guardian);
        h.write(&self._reserved);
    }
}

impl Component for Household {
    const NAME: &'static str = "agents.Household";
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::MinuteOfDay;

    fn czlonek(citizen: u32, wiek: i32) -> MemberView {
        MemberView {
            citizen,
            age_years: wiek,
            is_pupil: false,
            works: false,
            shift: ShiftKind::Day,
            site: Employment::NO_SITE,
            partnered_inside: false,
        }
    }

    #[test]
    fn rozmiar_gospodarstwa_zgadza_sie_z_budzetem() {
        // §5.6 po korekcie G-1: 120 B. Budżet §17.7 liczy GD osobno (≈167 tys. × 120 B),
        // więc każdy bajt ponad to jest 167 KB, o których nikt się nie dowie.
        // Offsety pilnują, że kompilator nie wsunął niejawnego paddingu między pola.
        assert_eq!(
            (
                std::mem::offset_of!(Household, members),
                std::mem::offset_of!(Household, building),
                std::mem::offset_of!(Household, cash),
                std::mem::offset_of!(Household, stock),
                std::mem::offset_of!(Household, vehicle_slots),
                std::mem::offset_of!(Household, guardian),
                std::mem::offset_of!(Household, _reserved),
                size_of::<Household>(),
            ),
            (8, 32, 40, 80, 92, 100, 104, 120)
        );
    }

    #[test]
    fn siodmy_czlonek_idzie_do_przelewu_i_wraca_na_zwolnione_miejsce() {
        let mut hh = Household::default();
        let mut ov = HouseholdOverflow::new();
        for i in 0..7 {
            assert!(add_member(1, &mut hh, &mut ov, i));
        }
        assert_eq!(hh.size, 7);
        assert_eq!(ov.get(1), &[6]);
        assert_eq!(
            members_of(1, &hh, &ov).as_slice(),
            &[0, 1, 2, 3, 4, 5, 6],
            "przelew zgubił członka"
        );

        // Zwolnienie miejsca w komponencie zasysa pierwszego z przelewu.
        assert!(remove_member(1, &mut hh, &mut ov, 2));
        assert!(
            ov.is_empty(),
            "przelew został, mimo że miejsce się zwolniło"
        );
        assert_eq!(hh.size, 6);
        let skl = members_of(1, &hh, &ov);
        assert!(!skl.contains(&2));
        assert!(skl.contains(&6));
    }

    #[test]
    fn gospodarstwo_bez_czlonkow_przestaje_byc_aktywne() {
        let mut hh = Household::default();
        let mut ov = HouseholdOverflow::new();
        assert!(add_member(3, &mut hh, &mut ov, 10));
        assert!(hh.is_active());
        remove_member(3, &mut hh, &mut ov, 10);
        assert!(!hh.is_active());
        assert_eq!(hh.size, 0);
        assert!(
            !remove_member(3, &mut hh, &mut ov, 10),
            "podwójne usunięcie"
        );
    }

    #[test]
    fn typ_gospodarstwa_wynika_ze_skladu_a_nie_z_deklaracji() {
        let (dorosly, senior) = (18, 65);
        assert_eq!(classify(&[], dorosly, senior), HouseholdKind::Single);
        assert_eq!(
            classify(&[czlonek(1, 30)], dorosly, senior),
            HouseholdKind::Single
        );
        assert_eq!(
            classify(&[czlonek(1, 72)], dorosly, senior),
            HouseholdKind::LoneSenior
        );

        let mut para = [czlonek(1, 30), czlonek(2, 32)];
        para[0].partnered_inside = true;
        para[1].partnered_inside = true;
        assert_eq!(classify(&para, dorosly, senior), HouseholdKind::Couple);

        let rodzina = [czlonek(1, 34), czlonek(2, 36), czlonek(3, 8)];
        assert_eq!(
            classify(&rodzina, dorosly, senior),
            HouseholdKind::FamilyWithKids
        );

        let trzy_pokolenia = [
            czlonek(1, 34),
            czlonek(2, 36),
            czlonek(3, 8),
            czlonek(4, 70),
        ];
        assert_eq!(
            classify(&trzy_pokolenia, dorosly, senior),
            HouseholdKind::MultiGen
        );

        let wspollokatorzy = [czlonek(1, 24), czlonek(2, 26), czlonek(3, 25)];
        assert_eq!(
            classify(&wspollokatorzy, dorosly, senior),
            HouseholdKind::Roommates
        );
    }

    #[test]
    fn odprowadza_kto_zaczyna_pozniej_odbiera_kto_konczy_wczesniej() {
        // C-8: szkoła kończy się o 14:00, zmiana dzienna o 16:00 — gdyby odbierał ten
        // sam, kto odprowadza, dziecko czekałoby dwie godziny przed szkołą.
        let mut wczesna = czlonek(1, 35);
        wczesna.works = true;
        wczesna.shift = ShiftKind::Early; // 6–14
        let mut dzienna = czlonek(2, 37);
        dzienna.works = true;
        dzienna.shift = ShiftKind::Day; // 8–16
        let mut dziecko = czlonek(3, 8);
        dziecko.is_pupil = true;
        dziecko.site = 4242;

        let r = roles(&[wczesna, dzienna, dziecko], 10, 18, 0, None);
        assert_eq!(r.escorted.as_slice(), &[3]);
        assert_eq!(r.escort, 2, "odprowadza zmiana zaczynająca się później");
        assert_eq!(r.pickup, 1, "odbiera zmiana kończąca się wcześniej");
        assert_eq!(ShiftKind::Early.window().1, MinuteOfDay::new(14 * 60));

        // Samotny rodzic robi oba kursy — bo nie ma komu ich rozdzielić.
        let sam = roles(&[dzienna, dziecko], 10, 18, 0, None);
        assert_eq!(sam.escort, 2);
        assert_eq!(sam.pickup, 2);

        // Nastolatek chodzi do szkoły sam.
        let mut nastolatek = czlonek(4, 15);
        nastolatek.is_pupil = true;
        nastolatek.site = 77;
        let bez = roles(&[dzienna, nastolatek], 10, 18, 0, None);
        assert!(bez.escorted.is_empty());
        assert_eq!(bez.escort, Household::NO_MEMBER);
    }

    #[test]
    fn zakupy_rotuja_po_dorosłych() {
        let m = [czlonek(5, 40), czlonek(9, 42), czlonek(1, 6)];
        assert_eq!(roles(&m, 10, 18, 0, None).shopper, 5);
        assert_eq!(roles(&m, 10, 18, 1, None).shopper, 9);
        assert_eq!(roles(&m, 10, 18, 2, None).shopper, 5);
    }
}
