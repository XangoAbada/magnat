//! Cykl życia, hazardy i dziedziczenie (M3c §5.6, WP7).
//!
//! **Hazard roczny stosuje się raz w roku, nie 360 razy po 1/360.** System jest
//! shardowany 1/360 po indeksie encji (§5.6), więc każdy mieszkaniec przechodzi swoje
//! losowania dokładnie raz na 360 dób — w swojej dobie. Daje to tę samą częstość
//! zdarzeń co losowanie dzienne, kosztuje 1/360 pracy i, co ważniejsze, **zużywa jedną
//! wartość z jednego strumienia**: `rng(seed, StreamId::Demography, citizen_idx, day)`.
//! Dodanie albo usunięcie mieszkańca nie przesuwa wtedy losowań pozostałych
//! (`det_demography_independence`).
//!
//! Zdarzenia rozciągnięte w czasie — poród po ciąży, wyzdrowienie po chorobie — nie
//! mieszczą się w tym rytmie, bo mają własny termin. Idą do `LifeQueue`: kalendarza
//! dobowego opartego na `BTreeMap`, czyli tego samego pomysłu co przelew kolejki DES,
//! tylko w skali dób, a nie minut. Kolejki DES nie da się tu użyć wprost: ta liczy
//! w minutach i przewija się minuta po minucie, a tryb demograficzny (`m3_century`)
//! skacze po dobach.
//!
//! Czego tu **nie ma**: migracji (`migration.rs`), statusu, relacji i plotki
//! (`social.rs`), generacji populacji Etapu 8 (M3d). Tu jest to, co się dzieje
//! z mieszkańcem, który już istnieje.

use crate::components::{
    AgentState, Employment, Identity, KnowledgeRef, Lifecycle, Needs, Personality, PlanRef,
    RelationsRef, Residence, Skills, Vitals, Wealth,
};
use crate::household::{self, Household, HouseholdOverflow, MemberView};
use crate::store::{KnowledgeSlab, PlanSlab, Relation, RelationKind, RelationSlab, SlabRef};
use magnat_core::{
    rng, CitizenId, DecisionReason, Entity, HashState, LifeEventKind, Money, NeedKind, Rng,
    StateHasher, StreamId, Tick, Q,
};
use magnat_ecs::{CommandBuffer, SystemId, World};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

pub(super) mod day;
pub(super) mod month;
pub(super) mod table;

pub use day::step_day;
pub use month::{compatibility, przeklasyfikuj};
pub use table::{Ages, DemographyError, DemographyTable, EduStep, StatusWeights};

/// Wersja schematu `data/demography/demography.ron`. 2 → 3 przy `R2-WP5`:
/// doszła tabela `education` (lata nauki → `EduLevel`).
///
/// **`ages.childcare_end` tu nie ma i to jest decyzja.** Opieka nad dzieckiem poniżej
/// wieku szkolnego blokuje dorosłego w gospodarstwie, czyli zmienia podaż pracy całego
/// miasta — to jest własna naprawa z własnym przebiegiem balansatora, a nie pole przy
/// okazji (`R2` §3 pkt 2). Liczba bez czytelnika wygląda w danych dokładnie tak samo
/// jak działająca, więc nie wchodzi tu przed swoim konsumentem.
pub const DEMOGRAPHY_SCHEMA_VERSION: u32 = 3;

/// Ile dób dzieli dwa losowania hazardów tego samego mieszkańca (§5.6, sharding 1/360).
pub const DEMOGRAPHY_SHARDS: u64 = 360;

/// Dób w roku gry (K-1). Lokalna stała **nie istnieje** — to re-eksport, żeby
/// czytający ten plik nie musiał sprawdzać, czy ktoś tu nie policzył roku po swojemu.
pub const DAYS_PER_YEAR: u64 =
    magnat_core::time::DAYS_PER_MONTH * magnat_core::time::MONTHS_PER_YEAR;

// ── rejestr populacji ───────────────────────────────────────────────────────────

/// Spis żywych mieszkańców i gospodarstw plus księga zmian.
///
/// Spis, a nie zapytanie ECS, z dwóch powodów. Po pierwsze **kolejność**: archetypowy
/// ECS oddaje encje w kolejności chunków, a ta zależy od historii mutacji
/// strukturalnych; demografia musi iterować po rosnącym indeksie encji, bo to on jest
/// argumentem strumienia RNG. Po drugie **księga**: `prop_population_identity` żąda,
/// żeby dla każdej doby `pop(t+1) = pop(t) + narodziny − zgony + napływ − odpływ`
/// zgadzało się co do osoby, a to jest stwierdzenie o czterech licznikach, nie
/// o rozmiarze tabeli.
#[derive(Clone, Debug, Default)]
pub struct Population {
    citizens: Vec<Entity>,
    households: Vec<Entity>,
    pub births: u64,
    pub deaths: u64,
    pub arrivals: u64,
    pub departures: u64,
    /// Pieniądz, który wyjechał z miasta razem z mieszkańcem.
    pub emigrated: Money,
    /// Pieniądz po zmarłym bez spadkobierców. Nie znika — inaczej test zachowania
    /// pieniądza (00 §6) miałby dziurę wielkości jednego pokolenia.
    pub escheat: Money,
}

impl Population {
    #[must_use]
    pub fn new() -> Population {
        Population::default()
    }

    #[must_use]
    pub fn citizens(&self) -> &[Entity] {
        &self.citizens
    }

    #[must_use]
    pub fn households(&self) -> &[Entity] {
        &self.households
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.citizens.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.citizens.is_empty()
    }

    /// Tożsamość księgowa doby: stan początkowy plus bilans = stan bieżący.
    #[must_use]
    pub fn identity_holds(&self, start: u64) -> bool {
        let saldo = start as i128 + self.births as i128 + self.arrivals as i128
            - self.deaths as i128
            - self.departures as i128;
        saldo == self.citizens.len() as i128
    }

    pub fn add_citizen(&mut self, e: Entity) {
        if let Err(i) = self
            .citizens
            .binary_search_by_key(&e.index(), |x| x.index())
        {
            self.citizens.insert(i, e);
        }
    }

    pub fn remove_citizen(&mut self, e: Entity) {
        if let Ok(i) = self
            .citizens
            .binary_search_by_key(&e.index(), |x| x.index())
        {
            self.citizens.remove(i);
        }
    }

    pub fn add_household(&mut self, e: Entity) {
        if let Err(i) = self
            .households
            .binary_search_by_key(&e.index(), |x| x.index())
        {
            self.households.insert(i, e);
        }
    }

    pub fn remove_household(&mut self, e: Entity) {
        if let Ok(i) = self
            .households
            .binary_search_by_key(&e.index(), |x| x.index())
        {
            self.households.remove(i);
        }
    }

    /// Odbudowa spisu ze świata — dla scenariuszy, które zaludniają go same
    /// (Etap 8 w M3d, testy). Po niej spis jest posortowany po indeksie encji.
    pub fn rebuild(&mut self, world: &World) {
        self.citizens.clear();
        self.households.clear();
        let mut c: Vec<Entity> = Vec::new();
        let mut h: Vec<Entity> = Vec::new();
        for a in world.archetypes().iter() {
            for chunk in a.chunks() {
                for e in chunk.entities() {
                    if world.get::<Identity>(*e).is_some_and(Identity::is_alive) {
                        c.push(*e);
                    } else if world.has::<Household>(*e) {
                        h.push(*e);
                    }
                }
            }
        }
        c.sort_unstable_by_key(|e| e.index());
        h.sort_unstable_by_key(|e| e.index());
        self.citizens = c;
        self.households = h;
    }
}

impl HashState for Population {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.citizens.len() as u64);
        for e in &self.citizens {
            h.write_u32(e.index());
        }
        h.write_u64(self.households.len() as u64);
        for e in &self.households {
            h.write_u32(e.index());
        }
        h.write_u64(self.births);
        h.write_u64(self.deaths);
        h.write_u64(self.arrivals);
        h.write_u64(self.departures);
        self.emigrated.hash_state(h);
        self.escheat.hash_state(h);
    }
}

// ── kalendarz zdarzeń życiowych ─────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum LifeTaskKind {
    /// Koniec ciąży.
    Birth = 0,
    /// Koniec choroby.
    Recover = 1,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct LifeTask {
    pub kind: LifeTaskKind,
    pub actor: u32,
}

/// Terminarz zdarzeń o własnej dacie: poród, wyzdrowienie.
///
/// `BTreeMap` po dobie; w obrębie doby zadania sortują się po `(kind, actor)`, czyli
/// tak samo jak zdarzenia DES po `order_key` — porządek totalny niezależny od kolejności
/// wstawiania (00 §3.3).
#[derive(Clone, Debug, Default)]
pub struct LifeQueue {
    by_day: BTreeMap<u32, Vec<LifeTask>>,
    len: usize,
}

impl LifeQueue {
    #[must_use]
    pub fn new() -> LifeQueue {
        LifeQueue::default()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn schedule(&mut self, day: u64, task: LifeTask) {
        self.by_day.entry(day as u32).or_default().push(task);
        self.len += 1;
    }

    /// Zadania na dobę `day`, w porządku totalnym. Zadania z dób **wcześniejszych**
    /// też wychodzą: tryb demograficzny skacze po dobach, a zadanie pominięte
    /// zostałoby w mapie na zawsze i wisiało w hashu stanu.
    pub fn take_due(&mut self, day: u64, out: &mut Vec<LifeTask>) {
        out.clear();
        let granica = day as u32;
        while let Some((&d, _)) = self.by_day.iter().next() {
            if d > granica {
                break;
            }
            let v = self.by_day.remove(&d).unwrap_or_default();
            self.len -= v.len();
            out.extend(v);
        }
        out.sort_unstable();
    }

    /// Usuwa wszystkie zadania aktora — wołane przy zgonie, żeby martwa ciężarna
    /// nie urodziła za trzy miesiące.
    pub fn cancel(&mut self, actor: u32) {
        let mut puste: Vec<u32> = Vec::new();
        for (d, v) in &mut self.by_day {
            let przed = v.len();
            v.retain(|t| t.actor != actor);
            self.len -= przed - v.len();
            if v.is_empty() {
                puste.push(*d);
            }
        }
        for d in puste {
            self.by_day.remove(&d);
        }
    }
}

impl HashState for LifeQueue {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.len as u64);
        for (d, v) in &self.by_day {
            h.write_u32(*d);
            let mut s = v.clone();
            s.sort_unstable();
            h.write_u64(s.len() as u64);
            for t in s {
                h.write_u8(t.kind as u8);
                h.write_u32(t.actor);
            }
        }
    }
}

// ── hook dziedziczenia (§5.6, decyzja 9.10) ─────────────────────────────────────

/// Rejestracja aktywów, których M3 nie zna.
///
/// M3 dzieli **wyłącznie pieniądz i mieszkanie**. M7 rejestruje tu udziały w firmach,
/// M5 długi i rachunki, M10 aktywa giełdowe. Permile sumują się do 1000; reszta
/// z dzielenia idzie do pierwszego spadkobiercy wg `entity_index` — ta sama reguła,
/// co przy podziale kwoty w 00 §2.
pub trait InheritanceHook: Send + Sync {
    /// Wołany po podziale `Money` i mieszkania, **przed** usunięciem encji zmarłego.
    fn on_inheritance(
        &mut self,
        deceased: CitizenId,
        heirs: &[(CitizenId, u16)],
        cmd: &mut CommandBuffer,
    );
}

/// Hook, który nic nie robi — domyślny do czasu M5/M7.
pub struct NoInheritance;

impl InheritanceHook for NoInheritance {
    fn on_inheritance(&mut self, _d: CitizenId, _h: &[(CitizenId, u16)], _c: &mut CommandBuffer) {}
}

// ── raport doby ─────────────────────────────────────────────────────────────────

/// Co się wydarzyło w tej dobie. Wejście do raportu `m3_century` i do testów.
///
/// `reasons` niesie **uzasadnienia** (00 §7): karta inspekcji M3d bierze je stąd,
/// a nie z logu per mieszkaniec. Lista jest krótka z natury rzeczy — hazardy dotykają
/// 1/360 populacji na dobę, a zdarza się z tego kilka promili — więc `Vec` nie jest
/// tu kosztem, tylko najprostszą rzeczą, która działa.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct DayReport {
    pub births: u32,
    pub deaths: u32,
    pub conceptions: u32,
    pub illnesses: u32,
    pub recoveries: u32,
    pub retirements: u32,
    pub reasons: Vec<(u32, DecisionReason)>,
}

/// Co się wydarzyło w tym miesiącu.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct MonthReport {
    pub partnerships: u32,
    pub weddings: u32,
    pub separations: u32,
    pub left_nest: u32,
    pub reasons: Vec<(u32, DecisionReason)>,
}

/// Klucz strumienia RNG w obrębie doby: `doba × 8 + numer losowania`.
///
/// Osiem miejsc na dobę, bo hazardy dobowe i miesięczne trafiają w te same doby
/// (miesiąc zaczyna się co 30 dób, shard mieszkańca wypada co 360) i bez rozdzielenia
/// czerpałyby z **tego samego** generatora — czyli decyzja o rozwodzie zależałaby od
/// tego, czy tego dnia wypadło też losowanie zgonu. Numery są przypisane raz:
/// 0 hazardy dobowe, 1 poród, 2 dobór partnera, 3 rozstanie, 4 wyprowadzka z domu,
/// 5 napływ, 6 plotka, 7 odpływ z powodu bezrobocia. Mnożnik 8 jest do podniesienia
/// przez fazę, której zabraknie miejsc — zmienia wtedy **każdy** świat z tego ziarna,
/// więc robi się to raz i świadomie, a nie przy okazji.
#[inline]
#[must_use]
pub const fn stream_key(day: u64, slot: u64) -> Tick {
    Tick(day * 8 + slot)
}

pub const K_DAILY: u64 = 0;
pub const K_BIRTH: u64 = 1;
pub const K_PARTNER: u64 = 2;
pub const K_SEPARATION: u64 = 3;
pub const K_NEST: u64 = 4;
pub const K_MIGRATION: u64 = 5;
pub const K_GOSSIP: u64 = 6;
pub const K_JOBLESS: u64 = 7;

/// Identyfikator systemu dla buforów komend tej podfazy (00 §3.4: sortowanie po
/// `(SystemId, entity_index)`). Jedna nazwa, a nie `SystemId::from_name` w każdym
/// wywołaniu — kolejność aplikacji komend jest wtedy funkcją nazwy, nie miejsca w kodzie.
#[must_use]
pub fn demography_system_id() -> SystemId {
    SystemId::from_name("agents.Demography")
}

fn encja(world: &World, index: u32) -> Option<Entity> {
    let p = world.resource::<Population>();
    p.citizens
        .binary_search_by_key(&index, |x| x.index())
        .ok()
        .map(|i| p.citizens[i])
}

fn encja_gospodarstwa(world: &World, index: u32) -> Option<Entity> {
    let p = world.resource::<Population>();
    p.households
        .binary_search_by_key(&index, |x| x.index())
        .ok()
        .map(|i| p.households[i])
}

/// Zawiązuje relację obustronnie. Strona `b` dostaje relację odwrotną: rodzic widzi
/// dziecko, dziecko rodzica — `prop_relation_symmetry` sprawdza obie strony.
pub fn powiaz(world: &mut World, a: Entity, b: Entity, kind: RelationKind, weight: u8, day: u64) {
    wpisz_relacje(world, a, b, kind, weight, day);
    wpisz_relacje(world, b, a, odwrotna(kind), weight, day);
}

const fn odwrotna(k: RelationKind) -> RelationKind {
    match k {
        RelationKind::Parent => RelationKind::Child,
        RelationKind::Child => RelationKind::Parent,
        other => other,
    }
}

fn wpisz_relacje(
    world: &mut World,
    kto: Entity,
    z_kim: Entity,
    kind: RelationKind,
    weight: u8,
    day: u64,
) {
    let Some(rel) = world.get::<RelationsRef>(kto).copied() else {
        return;
    };
    let mut r = SlabRef {
        handle: rel.handle,
        len: rel.len,
        class: rel.class,
    };
    if rel.len == 0 && rel.handle == 0 {
        r = SlabRef::EMPTY;
    }
    let dzis = (day % 65_536) as u16;
    let slab = world.resource_mut::<RelationSlab>();
    if let Some(istnieje) = slab
        .entries_mut(r)
        .iter_mut()
        .find(|x| x.other == z_kim.index())
    {
        istnieje.weight = istnieje.weight.max(weight);
        istnieje.last_contact_day = dzis;
        return;
    }
    slab.push(
        &mut r,
        Relation {
            other: z_kim.index(),
            kind: kind as u8,
            weight,
            last_contact_day: dzis,
        },
        |wpisy| {
            // Wypycha najsłabszą relację, a przy remisie najstarszy kontakt.
            wpisy
                .iter()
                .enumerate()
                .min_by_key(|(i, w)| (w.weight, w.last_contact_day, *i))
                .map_or(0, |(i, _)| i)
        },
    );
    if let Some(slot) = world.get_mut::<RelationsRef>(kto) {
        slot.handle = r.handle;
        slot.len = r.len;
        slot.class = r.class;
    }
}

/// Uchwyt relacji mieszkańca jako `SlabRef`. Jedno miejsce konwersji — komponent
/// i slab mają trzy te same pola, ale `SlabRef::EMPTY` jest sentinelem, a
/// `RelationsRef::default()` zerem.
#[must_use]
pub fn relations_ref(r: &RelationsRef) -> SlabRef {
    if r.len == 0 {
        SlabRef::EMPTY
    } else {
        SlabRef {
            handle: r.handle,
            len: r.len,
            class: r.class,
        }
    }
}

/// Uchwyt wiedzy mieszkańca jako `SlabRef`.
#[must_use]
pub fn knowledge_ref(k: &KnowledgeRef) -> SlabRef {
    if k.len == 0 {
        SlabRef::EMPTY
    } else {
        SlabRef {
            handle: k.handle,
            len: k.len,
            class: k.class,
        }
    }
}

/// Encja mieszkańca po indeksie — publiczna, bo używa jej `migration` i Etap 8 w M3d.
#[must_use]
pub fn citizen_by_index(world: &World, index: u32) -> Option<Entity> {
    encja(world, index)
}

/// Encja gospodarstwa po indeksie.
#[must_use]
pub fn household_by_index(world: &World, index: u32) -> Option<Entity> {
    encja_gospodarstwa(world, index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabela_z_danych_pokrywa_kazdy_wiek() {
        let t = DemographyTable::load_default().expect("data/demography/demography.ron");
        let ages = t.ages();
        assert!(ages.retirement > ages.work_start);
        assert!(ages.max > ages.retirement);
        for wiek in 0..=i32::from(ages.max) {
            assert!(
                t.mortality_per_100k(wiek, Q::MAX) > 0,
                "zerowa umieralność w wieku {wiek} — dziura w tabeli"
            );
            assert!(t.illness_per_100k(wiek, Q::MAX) > 0, "wiek {wiek}");
        }
        assert_eq!(t.status_weights().sum(), 100);
    }

    #[test]
    fn plodnosc_spada_z_kolejnym_dzieckiem_i_bez_partnera() {
        let t = DemographyTable::load_default().expect("dane");
        let pelna = t.fertility_per_100k(28, 0, true, Q::MAX);
        assert!(pelna > 0);
        assert!(t.fertility_per_100k(28, 3, true, Q::MAX) < pelna);
        assert!(t.fertility_per_100k(28, 0, false, Q::MAX) < pelna);
        assert!(t.fertility_per_100k(28, 0, true, Q::new(10)) < pelna);
        // Poza oknem rozrodczym hazard jest zerowy, a nie „mały".
        assert_eq!(t.fertility_per_100k(60, 0, true, Q::MAX), 0);
        assert_eq!(t.fertility_per_100k(10, 0, true, Q::MAX), 0);
    }

    #[test]
    fn chory_umiera_czesciej_niz_zdrowy() {
        let t = DemographyTable::load_default().expect("dane");
        assert!(t.mortality_per_100k(40, Q::MIN) > t.mortality_per_100k(40, Q::MAX));
        // Sufit tabeli to nie „bardzo duży hazard", tylko koniec tabeli — pewność
        // dokłada `hazardy`, żeby `prop_no_immortals` nie zależał od ogona rozkładu.
        assert!(t.mortality_per_100k(i32::from(t.ages().max), Q::MAX) < 100_000);
    }

    #[test]
    fn terminarz_wydaje_zadania_w_porzadku_totalnym() {
        let mut q = LifeQueue::new();
        for a in [7u32, 3, 9, 1] {
            q.schedule(
                10,
                LifeTask {
                    kind: LifeTaskKind::Recover,
                    actor: a,
                },
            );
            q.schedule(
                10,
                LifeTask {
                    kind: LifeTaskKind::Birth,
                    actor: a,
                },
            );
        }
        q.schedule(
            5,
            LifeTask {
                kind: LifeTaskKind::Birth,
                actor: 100,
            },
        );
        assert_eq!(q.len(), 9);

        // Doba 12 zabiera też zaległą dobę 5 — w trybie demograficznym skacze się
        // po dobach i zadanie pominięte wisiałoby w mapie na zawsze.
        let mut out = Vec::new();
        q.take_due(12, &mut out);
        assert_eq!(out.len(), 9);
        assert!(q.is_empty());
        assert!(
            out.windows(2).all(|w| w[0] <= w[1]),
            "porządek nie jest totalny: {out:?}"
        );
        assert_eq!(out[0].kind, LifeTaskKind::Birth);

        q.schedule(
            20,
            LifeTask {
                kind: LifeTaskKind::Birth,
                actor: 42,
            },
        );
        q.cancel(42);
        assert!(q.is_empty(), "anulowanie zostawiło zadanie martwego aktora");
    }

    #[test]
    fn spis_populacji_trzyma_porzadek_i_ksiege() {
        use std::num::NonZeroU32;
        let g = NonZeroU32::new(1).unwrap();
        let mut p = Population::new();
        for i in [5u32, 1, 9, 3] {
            p.add_citizen(Entity::new(i, g));
        }
        assert_eq!(
            p.citizens().iter().map(|e| e.index()).collect::<Vec<_>>(),
            vec![1, 3, 5, 9]
        );
        p.add_citizen(Entity::new(5, g));
        assert_eq!(p.len(), 4, "podwójne dopisanie tej samej encji");

        let start = 4;
        assert!(p.identity_holds(start));
        p.remove_citizen(Entity::new(3, g));
        p.deaths += 1;
        assert!(p.identity_holds(start));
        p.add_citizen(Entity::new(11, g));
        assert!(!p.identity_holds(start), "spis rozjechał się z księgą");
        p.births += 1;
        assert!(p.identity_holds(start));
    }
}
