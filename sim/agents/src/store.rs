//! Magazyny o zmiennej długości: arena planu dnia, slab relacji, slab wiedzy (M3a §5.1).
//!
//! Trzy rzeczy, których nie da się trzymać w komponencie o stałym rozmiarze, a które
//! muszą wejść do hasha stanu i do snapshotu tak samo jak komponenty (00 §3.6).
//!
//! Slab ma **klasy rozmiaru 4 / 8 / 16 / 24 / 32** i wolną listę per klasa. Nie ma
//! fragmentacji, bo bloki w obrębie klasy są identyczne — zwolniony blok pasuje na
//! miejsce każdego innego. Mieszkaniec zaczyna od klasy 4 (większość ma kilka relacji)
//! i awansuje, gdy przestaje się mieścić; 33. wpis nie powiększa bloku, tylko wypycha
//! najsłabszy — limit 32 z §5.1 jest twardy.
//!
//! `ponytail:` jeden magazyn na „byłem" i „słyszałem" zamiast dwóch — §5.1 (pamięć)
//! i §5.7 (wiedza) to ten sam byt z innym polem `kind`. Rozdzielenie dopiero gdyby M10
//! potrzebował innego cyklu życia dla reklamy.

use magnat_core::{CitizenReason, HashState, StateHasher};

/// Klasy rozmiaru slabu. Kolejność rosnąca jest założeniem `promote`.
///
/// **Klasa 12 dołożona po pomiarze M3a** wobec 4/8/16/24/32 z §5.1: dwanaście to
/// zarazem średnia długość planu dnia, jak i częsta liczba relacji, więc bez niej
/// najliczniejszy przypadek płacił za blok szesnastoelementowy. Koszt klasy to jedna
/// pula więcej, zysk — około 25 % objętości obu magazynów (patrz tabela korekt M3a).
pub const SLAB_CLASSES: [usize; 6] = [4, 8, 12, 16, 24, 32];

/// Twardy limit wpisów na mieszkańca — `prop_knowledge_bound` z §7.2.
pub const SLAB_MAX: usize = 32;

/// O ile wpisów naraz rośnie pula, gdy zabraknie miejsca.
const GROW_ENTRIES: usize = 64 * 1024;

/// Uchwyt do bloku w slabie. Trzy pola, bo dokładnie te trzy siedzą w `RelationsRef`
/// i `KnowledgeRef` (§5.1) — slab nie wymyśla własnej reprezentacji obok komponentu.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct SlabRef {
    pub handle: u32,
    pub len: u8,
    pub class: u8,
}

impl SlabRef {
    /// Uchwyt pusty — mieszkaniec bez ani jednego wpisu nie zajmuje bloku.
    pub const EMPTY: SlabRef = SlabRef {
        handle: u32::MAX,
        len: 0,
        class: 0,
    };

    #[inline]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.handle == u32::MAX
    }
}

/// Relacja międzyludzka — 8 B. Waga jest cappowana (0..=100), a nie sumowana bez końca:
/// inaczej po stu latach gry wszyscy znaliby wszystkich maksymalnie mocno.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Relation {
    /// Indeks encji drugiej strony.
    pub other: u32,
    /// `RelationKind`.
    pub kind: u8,
    /// Q 0..=100.
    pub weight: u8,
    /// Doba świata mod 65536.
    pub last_contact_day: u16,
}

/// Rodzaj relacji. Rodzinne są obustronne z definicji (`prop_relation_symmetry`).
///
/// **Kolejność wariantów jest kontraktem zapisu gry** (`K-59`) — `kind` siedzi
/// w 8-bajtowym wpisie slabu, a slab wchodzi do hasha stanu. Dopisywać wolno
/// wyłącznie na końcu.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum RelationKind {
    #[default]
    Acquaintance = 0,
    Partner = 1,
    Parent = 2,
    Child = 3,
    Sibling = 4,
    Friend = 5,
    Colleague = 6,
    Neighbour = 7,
    /// Dziadek albo wnuk — **symetryczny** (`R2-WP2`, `K-59`).
    ///
    /// Kierunek odczytuje się z wieku, którym obie strony i tak dysponują
    /// (`Identity.birth_day`), a nie z osobnego wariantu. Wersja asymetryczna
    /// (`odwrotna(Grandparent) = Child`) zapisywałaby po stronie babci wpis
    /// nieodróżnialny od wpisu o własnym dziecku — a `spadkobiercy` filtruje
    /// dokładnie po `Child`, więc wnuk dziedziczyłby po równo z dziećmi. To jest
    /// zmiana w podziale spadku i należy do `R2-WP10`, a nie do grafu rodziny.
    Grandparent = 8,
}

impl RelationKind {
    #[must_use]
    pub const fn from_u8(v: u8) -> RelationKind {
        match v {
            1 => RelationKind::Partner,
            2 => RelationKind::Parent,
            3 => RelationKind::Child,
            4 => RelationKind::Sibling,
            5 => RelationKind::Friend,
            6 => RelationKind::Colleague,
            7 => RelationKind::Neighbour,
            8 => RelationKind::Grandparent,
            _ => RelationKind::Acquaintance,
        }
    }

    /// Czy relacja jest rodzinna. **Jedna definicja dla całego crate'u** — do `R2-WP2`
    /// były dwie i różniły się o `Partner`: dobór partnera odsiewał krewnych bez
    /// małżonka, a zanik wagi podłogował rodzinę razem z nim.
    ///
    /// Rodzina nie wygasa z braku kontaktu i nie jest ofiarą wypychania, dopóki
    /// w slabie stoi cokolwiek nierodzinnego.
    #[inline]
    #[must_use]
    pub const fn is_family(self) -> bool {
        matches!(
            self,
            RelationKind::Partner
                | RelationKind::Parent
                | RelationKind::Child
                | RelationKind::Sibling
                | RelationKind::Grandparent
        )
    }
}

/// Wpis wiedzy albo doświadczenia — 8 B. Jeden magazyn na oba (patrz nagłówek modułu).
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Knowledge {
    /// Miejsce / pracodawca / (M5: produkt) jako indeks encji.
    pub target: u32,
    /// Doba świata mod 65536 ≈ 182 lata gry przy roku 360-dniowym.
    pub day: u16,
    /// Ocena 0..=100.
    pub score: u8,
    /// `KnowledgeKind`.
    pub kind: u8,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum KnowledgeKind {
    /// Był tam osobiście — najmocniejsze źródło.
    #[default]
    Visited = 0,
    /// Usłyszał od relacji (§5.7).
    Heard = 1,
    /// Zobaczył z trasy dojścia.
    SeenOnRoute = 2,
    /// M10: reklama.
    Ad = 3,
}

impl Knowledge {
    /// Waga wpisu przy wypychaniu 33. wpisu: ocena × świeżość. Świeżość spada liniowo
    /// przez 360 dób i nie schodzi poniżej 1 — wpis stary, ale wysoko oceniony, ma
    /// przeżyć świeżą przypadkową plotkę.
    #[must_use]
    pub fn rank(&self, today: u16) -> u32 {
        let wiek = today.wrapping_sub(self.day).min(360);
        let swiezosc = u32::from(360 - wiek) / 4 + 1;
        u32::from(self.score) * swiezosc
    }
}

impl HashState for Relation {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.other);
        h.write(&[self.kind, self.weight]);
        h.write_u16(self.last_contact_day);
    }
}

impl HashState for Knowledge {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.target);
        h.write_u16(self.day);
        h.write(&[self.score, self.kind]);
    }
}

/// Slab blokowy z klasami rozmiaru.
#[derive(Clone, Debug, Default)]
pub struct Slab<T: Copy + Default> {
    pools: [Pool<T>; SLAB_CLASSES.len()],
}

#[derive(Clone, Debug, Default)]
struct Pool<T: Copy + Default> {
    /// Bloki po `SLAB_CLASSES[i]` wpisów, ułożone ciągiem.
    data: Vec<T>,
    /// Indeksy zwolnionych bloków. Kolejność **wchodzi do hasha**: to od niej zależy,
    /// który blok dostanie następny mieszkaniec, a więc i układ całego magazynu.
    free: Vec<u32>,
}

impl<T: Copy + Default> Slab<T> {
    #[must_use]
    pub fn new() -> Slab<T> {
        Slab {
            pools: std::array::from_fn(|_| Pool {
                data: Vec::new(),
                free: Vec::new(),
            }),
        }
    }

    /// Liczba zajętych bloków we wszystkich klasach — `prop_knowledge_bound` sprawdza
    /// nią wycieki (liczba bloków == liczba żywych mieszkańców z wpisami).
    #[must_use]
    pub fn occupied_blocks(&self) -> usize {
        self.pools
            .iter()
            .enumerate()
            .map(|(i, p)| p.data.len() / SLAB_CLASSES[i] - p.free.len())
            .sum()
    }

    /// Zaalokowane bajty — wejście do rachunku budżetu §17.7.
    #[must_use]
    pub fn allocated_bytes(&self) -> usize {
        self.pools
            .iter()
            .map(|p| p.data.capacity() * size_of::<T>() + p.free.capacity() * 4)
            .sum()
    }

    #[must_use]
    pub fn entries(&self, r: SlabRef) -> &[T] {
        if r.is_empty() || r.len == 0 {
            return &[];
        }
        let cap = SLAB_CLASSES[r.class as usize];
        let start = r.handle as usize * cap;
        &self.pools[r.class as usize].data[start..start + r.len as usize]
    }

    pub fn entries_mut(&mut self, r: SlabRef) -> &mut [T] {
        if r.is_empty() || r.len == 0 {
            return &mut [];
        }
        let cap = SLAB_CLASSES[r.class as usize];
        let start = r.handle as usize * cap;
        &mut self.pools[r.class as usize].data[start..start + r.len as usize]
    }

    /// Dopisuje wpis. Gdy blok jest pełny: awansuje do większej klasy, a przy klasie
    /// największej wypycha wpis wskazany przez `victim` (indeks w bieżącej zawartości).
    ///
    /// `victim` zamiast traitu z metodą rangowania: dwa magazyny, dwie różne reguły,
    /// obie jednolinijkowe po stronie wywołującego — trait na to nie zarabia.
    pub fn push(&mut self, r: &mut SlabRef, v: T, victim: impl Fn(&[T]) -> usize) {
        if r.is_empty() {
            *r = self.alloc_block(0);
        }
        let cap = SLAB_CLASSES[r.class as usize];
        if r.len as usize == cap {
            if r.class as usize + 1 < SLAB_CLASSES.len() {
                self.promote(r);
            } else {
                let i = victim(self.entries(*r));
                debug_assert!(i < SLAB_MAX, "victim wskazał wpis spoza bloku");
                self.entries_mut(*r)[i] = v;
                return;
            }
        }
        let cap = SLAB_CLASSES[r.class as usize];
        let start = r.handle as usize * cap + r.len as usize;
        self.pools[r.class as usize].data[start] = v;
        r.len += 1;
    }

    /// Zapisuje całą zawartość naraz, dobierając **od razu** właściwą klasę.
    ///
    /// Dla planu dnia to jedyna sensowna droga: plan powstaje w całości i w całości
    /// jest zastępowany, więc awansowanie po jednym wpisie przepisywałoby blok
    /// cztery razy po drodze.
    pub fn store(&mut self, r: &mut SlabRef, items: &[T]) {
        let potrzebna = SLAB_CLASSES
            .iter()
            .position(|c| *c >= items.len())
            .unwrap_or(SLAB_CLASSES.len() - 1);
        if r.is_empty() || r.class as usize != potrzebna {
            self.free(r);
            *r = self.alloc_block(potrzebna);
        }
        let cap = SLAB_CLASSES[potrzebna];
        let start = r.handle as usize * cap;
        let n = items.len().min(cap);
        let pool = &mut self.pools[potrzebna];
        pool.data[start..start + n].copy_from_slice(&items[..n]);
        pool.data[start + n..start + cap].fill(T::default());
        r.len = n as u8;
    }

    /// Usuwa wpis o indeksie `i`, zachowując kolejność pozostałych.
    ///
    /// Kolejność, a nie `swap_remove`: relacje i wiedza są rangowane przy wypychaniu
    /// 33. wpisu, a `swap_remove` zmieniałby, kogo wypchnie następne przepełnienie —
    /// czyli stan świata — w sposób zależny od historii usunięć. Blok, z którego
    /// usunięto ostatni wpis, wraca na wolną listę (`prop_knowledge_bound` liczy
    /// zajęte bloki i wyciek byłby w nim widoczny).
    pub fn remove_at(&mut self, r: &mut SlabRef, i: usize) {
        if r.is_empty() || i >= r.len as usize {
            return;
        }
        let cap = SLAB_CLASSES[r.class as usize];
        let start = r.handle as usize * cap;
        let pool = &mut self.pools[r.class as usize];
        for j in i..r.len as usize - 1 {
            pool.data[start + j] = pool.data[start + j + 1];
        }
        pool.data[start + r.len as usize - 1] = T::default();
        r.len -= 1;
        if r.len == 0 {
            self.free(r);
        }
    }

    /// Zwalnia blok. Zawartość jest **zerowana**, żeby dwa przebiegi o tej samej
    /// historii alokacji dawały identyczny hash niezależnie od tego, co w bloku było.
    pub fn free(&mut self, r: &mut SlabRef) {
        if r.is_empty() {
            return;
        }
        let cap = SLAB_CLASSES[r.class as usize];
        let start = r.handle as usize * cap;
        let pool = &mut self.pools[r.class as usize];
        pool.data[start..start + cap].fill(T::default());
        pool.free.push(r.handle);
        *r = SlabRef::EMPTY;
    }

    fn alloc_block(&mut self, class: usize) -> SlabRef {
        let cap = SLAB_CLASSES[class];
        let pool = &mut self.pools[class];
        let handle = match pool.free.pop() {
            Some(h) => h,
            None => {
                let h = (pool.data.len() / cap) as u32;
                // `reserve_exact` porcjami zamiast podwajania: przy 400 tys.
                // mieszkańców zapas alokatora to inaczej drugie tyle pamięci,
                // której nikt nigdy nie zapisze (pomiar M3a, tabela korekt).
                if pool.data.capacity() - pool.data.len() < cap {
                    pool.data.reserve_exact(GROW_ENTRIES.max(cap));
                }
                pool.data.resize(pool.data.len() + cap, T::default());
                h
            }
        };
        SlabRef {
            handle,
            len: 0,
            class: class as u8,
        }
    }

    fn promote(&mut self, r: &mut SlabRef) {
        let stary = *r;
        let mut nowy = self.alloc_block(stary.class as usize + 1);
        let cap = SLAB_CLASSES[nowy.class as usize];
        let dst = nowy.handle as usize * cap;
        for i in 0..stary.len as usize {
            let src = stary.handle as usize * SLAB_CLASSES[stary.class as usize] + i;
            self.pools[nowy.class as usize].data[dst + i] =
                self.pools[stary.class as usize].data[src];
        }
        nowy.len = stary.len;
        let mut do_zwolnienia = stary;
        self.free(&mut do_zwolnienia);
        *r = nowy;
    }
}

impl<T: Copy + Default + HashState> HashState for Slab<T> {
    /// Hash obejmuje **zawartość, wolne listy i rozmiary pul** — nie samą zawartość
    /// bloków zajętych. Dwa przebiegi o identycznej treści, ale różnym układzie bloków,
    /// są różnymi stanami: następna alokacja pójdzie w nich w inne miejsce (00 §K-16
    /// mówi to samo o arenach).
    fn hash_state(&self, h: &mut StateHasher) {
        for (i, p) in self.pools.iter().enumerate() {
            h.write_u32(SLAB_CLASSES[i] as u32);
            h.write_u64(p.data.len() as u64);
            for v in &p.data {
                v.hash_state(h);
            }
            h.write_u64(p.free.len() as u64);
            for f in &p.free {
                h.write_u32(*f);
            }
        }
    }
}

/// Slab relacji — zasób świata, jeden na symulację.
pub type RelationSlab = Slab<Relation>;
/// Slab wiedzy i doświadczeń — zasób świata, jeden na symulację.
pub type KnowledgeSlab = Slab<Knowledge>;

/// Jeden blok planu dnia — 12 B (§5.1).
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct PlanSlot {
    /// Minuta doby 0..1439.
    pub start_min: u16,
    pub dur_min: u16,
    /// Encja miejsca (budynek / zakład / dom); `PlanSlot::NO_TARGET` = brak.
    pub target: u32,
    /// `ActivityKind`.
    pub kind: u8,
    /// `TransportMode` — M3 pisze `Walk`, M4 pisze resztę.
    pub mode: u8,
    /// Skrót powodu: `tag(u8) | param(u8) << 8`. Pełne uzasadnienie nie jest
    /// przechowywane, tylko odtwarzane przez `plan_day_explained` (M3b §5.4) —
    /// logi decyzji dla 400 tys. mieszkańców to 14 GB na rok gry.
    pub reason: u16,
}

impl PlanSlot {
    pub const NO_TARGET: u32 = u32::MAX;

    #[inline]
    #[must_use]
    pub const fn reason_tag(&self) -> u8 {
        self.reason as u8
    }

    #[inline]
    #[must_use]
    pub const fn reason_param(&self) -> u8 {
        (self.reason >> 8) as u8
    }

    #[inline]
    #[must_use]
    pub const fn pack_reason(tag: u8, param: u8) -> u16 {
        tag as u16 | ((param as u16) << 8)
    }

    /// Minuta zakończenia bez zawijania — plan nie przechodzi przez północ,
    /// doba kończy się snem i zaczyna nowym planem (§5.4).
    #[inline]
    #[must_use]
    pub const fn end_min(&self) -> u16 {
        self.start_min + self.dur_min
    }

    /// Po co mieszkaniec wychodzi do tego slotu — wejście wartości czasu w M4c
    /// (`R2-WP22`).
    ///
    /// Cel podróży ustala **planer doby**, bo to on wie, do czego mieszkaniec wychodzi;
    /// ruch zna trasę i pojazd, ale nie zna powodu. Do R2e nie ustalał go nikt
    /// i `start_trip` wpisywał każdej podróży `Work`.
    ///
    /// Odprowadzenie dziecka jest **slotem dojazdu** (`ActivityKind::Commute`)
    /// z powodem `Commitment { kind: Childcare }`, więc rozstrzyga o nim ładunek
    /// powodu, a nie czynność: dojazd do pracy i odprowadzenie do szkoły mają tę samą
    /// czynność i różne mnożniki czasu (1,30 wobec 1,50).
    ///
    /// `ponytail:` `Medical` nie ma tu producenta i to jest sufit nazwany. Wizyta
    /// u lekarza jedzie dziś jako `Errand`, bo `ActivityKind` nie odróżnia jej od
    /// innego załatwiania spraw, a rozstrzygnięcie po rodzaju miejsca docelowego
    /// (`PlaceKind::Doctor`) wymaga zajrzenia do katalogu miejsc przy każdym
    /// wyruszeniu. Wyjście: usługi zdrowotne M8d, które i tak dadzą wizycie własną
    /// czynność. `Refuel` producenta nie potrzebuje — tankowanie jest **odcinkiem
    /// wewnątrz** podróży i nadaje je `sim/traffic`, a nie plan doby.
    #[must_use]
    pub fn purpose(&self) -> magnat_core::TripPurpose {
        use magnat_core::{ActivityKind, CommitmentKind, DecisionReason, TripPurpose};

        // Dojazd: powód niesie rodzaj zobowiązania i to on rozstrzyga.
        if self.kind == ActivityKind::Commute.as_index() as u8 {
            let zobowiazanie = DecisionReason::Citizen(CitizenReason::Commitment {
                kind: CommitmentKind::Work,
            });
            if self.reason_tag() == zobowiazanie.discriminant() as u8 {
                return match CommitmentKind::from_index(usize::from(self.reason_param())) {
                    Some(CommitmentKind::Childcare) => TripPurpose::Escort,
                    Some(CommitmentKind::School) => TripPurpose::School,
                    _ => TripPurpose::Work,
                };
            }
            return TripPurpose::Work;
        }
        match ActivityKind::from_index(usize::from(self.kind)) {
            Some(ActivityKind::School) => TripPurpose::School,
            Some(ActivityKind::Shop | ActivityKind::Errand) => TripPurpose::Shopping,
            Some(ActivityKind::Eat | ActivityKind::Leisure | ActivityKind::Social) => {
                TripPurpose::Leisure
            }
            // Sen i bezczynność to powrót do domu — wyceniany jak czas wolny,
            // bo nikt nie wraca do domu w interesach.
            Some(ActivityKind::Sleep | ActivityKind::Idle) => TripPurpose::Leisure,
            _ => TripPurpose::Work,
        }
    }
}

impl HashState for PlanSlot {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u16(self.start_min);
        h.write_u16(self.dur_min);
        h.write_u32(self.target);
        h.write(&[self.kind, self.mode]);
        h.write_u16(self.reason);
    }
}

/// Magazyn planów dnia — ten sam slab co relacje i wiedza (§5.1).
///
/// **Zmiana wobec pierwotnego projektu §5.1** (arena bufowana podwójnie, kompaktowana
/// co dobę), wymuszona pomiarem: przy dwóch buforach plan wczorajszy żyje przez całą
/// dobę obok dzisiejszego, bo mieszkaniec wykonuje go aż do przeplanowania przy
/// pobudce. To jest **dwa razy 144 B na mieszkańca**, czyli 288 B z budżetu 400 —
/// sam plan zjadłby trzy czwarte stanu gorącego. Slab z klasami rozmiaru trzyma
/// dokładnie jeden plan na mieszkańca, nie wymaga kompaktowania (a więc i dobowego
/// przebiegu przepisującego `PlanRef.offset` wszystkim), i jest kodem, który już jest.
/// Szczegóły rachunku: tabela korekt w `M3a-fundament-agenta.md`.
pub type PlanSlab = Slab<PlanSlot>;

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(other: u32) -> Relation {
        Relation {
            other,
            kind: RelationKind::Friend as u8,
            weight: 50,
            last_contact_day: 0,
        }
    }

    #[test]
    fn slab_awansuje_klasy_zamiast_fragmentowac() {
        let mut s: RelationSlab = Slab::new();
        let mut r = SlabRef::EMPTY;
        for i in 0..4 {
            s.push(&mut r, rel(i), |_| 0);
        }
        assert_eq!(
            r.class, 0,
            "cztery wpisy mieszczą się w najmniejszej klasie"
        );
        s.push(&mut r, rel(4), |_| 0);
        assert_eq!(r.class, 1, "piąty wpis awansuje do klasy 8");
        assert_eq!(r.len, 5);
        assert_eq!(
            s.entries(r).iter().map(|x| x.other).collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 4],
            "awans zgubił zawartość"
        );
        assert_eq!(
            s.occupied_blocks(),
            1,
            "stary blok nie wrócił na wolną listę"
        );
    }

    #[test]
    fn trzydziesty_trzeci_wpis_wypycha_najslabszy() {
        let mut s: RelationSlab = Slab::new();
        let mut r = SlabRef::EMPTY;
        for i in 0..SLAB_MAX as u32 {
            s.push(&mut r, rel(i), |_| 0);
        }
        assert_eq!(r.len as usize, SLAB_MAX);
        assert_eq!(r.class as usize, SLAB_CLASSES.len() - 1);

        // Wypychamy wpis o najniższej wadze — tu: pierwszy.
        s.push(&mut r, rel(999), |wpisy| {
            wpisy
                .iter()
                .enumerate()
                .min_by_key(|(i, w)| (w.weight, *i))
                .map(|(i, _)| i)
                .unwrap_or(0)
        });
        assert_eq!(r.len as usize, SLAB_MAX, "limit 32 przestał być limitem");
        assert!(s.entries(r).iter().any(|w| w.other == 999));
    }

    #[test]
    fn zwolniony_blok_wraca_na_wolna_liste_i_jest_wyczyszczony() {
        let mut s: RelationSlab = Slab::new();
        let mut a = SlabRef::EMPTY;
        s.push(&mut a, rel(7), |_| 0);
        let handle = a.handle;
        s.free(&mut a);
        assert!(a.is_empty());
        assert_eq!(s.occupied_blocks(), 0);

        let mut b = SlabRef::EMPTY;
        s.push(&mut b, rel(8), |_| 0);
        assert_eq!(b.handle, handle, "wolna lista nie została użyta");
        assert_eq!(s.entries(b), &[rel(8)]);
    }

    #[test]
    fn hash_slabu_widzi_uklad_a_nie_tylko_tresc() {
        let odcisk = |s: &RelationSlab| {
            let mut h = StateHasher::new();
            s.hash_state(&mut h);
            h.finish()
        };

        let mut wprost: RelationSlab = Slab::new();
        let mut r1 = SlabRef::EMPTY;
        wprost.push(&mut r1, rel(10), |_| 0);

        let mut po_cyklu: RelationSlab = Slab::new();
        let mut a = SlabRef::EMPTY;
        po_cyklu.push(&mut a, rel(99), |_| 0);
        let mut b = SlabRef::EMPTY;
        po_cyklu.push(&mut b, rel(98), |_| 0);
        po_cyklu.free(&mut a);
        let mut c = SlabRef::EMPTY;
        po_cyklu.push(&mut c, rel(10), |_| 0);

        assert_ne!(
            odcisk(&wprost),
            odcisk(&po_cyklu),
            "różne układy slabu dały ten sam hash"
        );
    }

    #[test]
    fn ranga_wiedzy_broni_starej_dobrej_oceny_przed_swieza_plotka() {
        let stara_dobra = Knowledge {
            target: 1,
            day: 0,
            score: 90,
            kind: KnowledgeKind::Visited as u8,
        };
        let swieza_slaba = Knowledge {
            target: 2,
            day: 300,
            score: 10,
            kind: KnowledgeKind::Heard as u8,
        };
        assert!(stara_dobra.rank(300) > swieza_slaba.rank(300));
    }

    #[test]
    fn plan_zastepuje_sie_w_miejscu_bez_wycieku_bloku() {
        let mut s: PlanSlab = Slab::new();
        let slot = |m: u16| PlanSlot {
            start_min: m,
            dur_min: 60,
            ..PlanSlot::default()
        };
        let mut r = SlabRef::EMPTY;

        s.store(&mut r, &[slot(480), slot(540)]);
        assert_eq!(s.entries(r).len(), 2);
        assert_eq!(SLAB_CLASSES[r.class as usize], 4);

        // Plan na dwanaście slotów mieści się w klasie 12 — to dla niej ta klasa
        // powstała (średnia długość planu dnia).
        let dzien: Vec<PlanSlot> = (0..12).map(|i| slot(i * 60)).collect();
        s.store(&mut r, &dzien);
        assert_eq!(s.entries(r).len(), 12);
        assert_eq!(SLAB_CLASSES[r.class as usize], 12);
        assert_eq!(
            s.occupied_blocks(),
            1,
            "stary blok nie wrócił na wolną listę"
        );

        // Powrót do krótszego planu zwalnia większy blok.
        s.store(&mut r, &[slot(0)]);
        assert_eq!(SLAB_CLASSES[r.class as usize], 4);
        assert_eq!(s.occupied_blocks(), 1);
        assert_eq!(s.entries(r), &[slot(0)]);
    }
}
