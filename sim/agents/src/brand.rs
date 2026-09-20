//! Pamięć marki mieszkańca — sloty afinitetu, zanik leniwy, asymetria (M10b §5.1).
//!
//! **Marka nie jest liczbą po stronie firmy.** Jest zbiorem wpisów w pamięci konkretnych
//! ludzi: ilu ją zna, czego się po niej spodziewają i czy ich nie zawiodła (PRD §7.6).
//! Agregat „siła marki" powstaje dopiero na potrzeby panelu i jest sumą tych wpisów,
//! nigdy ich źródłem.
//!
//! **Gdzie to mieszka i dlaczego akurat tu.** Slot marki jest strukturą pamięci
//! mieszkańca, czyli rzeczą M3, a `sim/agents` stoi w grafie **pod** wszystkimi, którzy
//! do niej piszą: `sim/economy` przy zakupie, `magnat_media` przy ekspozycji,
//! `sim/macro` przy zasiewie po `lower()`. Gdyby slot mieszkał u piszącego, żaden
//! z pozostałych by go nie zobaczył — ta sama reguła, którą `K-64` postawił
//! `ServiceCoverage` w `core`.
//!
//! **Slab zamiast tablicy o stałym rozmiarze.** Plan fazy (M10b §5.1, wariant C)
//! zapisywał `BrandSlots(pub [BrandAffinity; 16])` jako komponent — 128 B na każdego
//! mieszkańca, także na tego, który nie zna ani jednej marki. Tu jest ten sam rachunek
//! pamięci (400 tys. × 16 × 8 B = 51,2 MB w szczycie), tylko płacony za wpisy, które
//! istnieją: `Slab` z klasami 4/8/12/16 jest już w tym module obok relacji i wiedzy,
//! ma `HashState`, ma wolne listy i ma regułę wypychania. Druga struktura o tym samym
//! zadaniu rozjechałaby się z pierwszą przy pierwszej zmianie.

pub mod tuning;

pub use tuning::{
    BrandData, BrandDataError, BrandTuning, ChannelTuning, OutletTuning, BRAND_SCHEMA_VERSION,
    EDITORIAL_BIAS_COUNT, EVENT_CATEGORY_COUNT,
};

use crate::components::BrandsRef;
use crate::store::{Slab, SlabRef};
use magnat_core::{AdChannelKind, BrandId, DecisionReason, HashState, StateHasher, TouchSource, Q};
use magnat_ecs::{Entity, World};

/// Ile marek naraz mieści się w pamięci jednego mieszkańca (M10b §5.1, decyzja `D4`).
///
/// Szesnaście jest **startem, nie wartością docelową**: `D4` fazy M10 mówi wprost, że
/// podniesienie do 24 jest jedną zmianą stałej i ma nastąpić dopiero wtedy, gdy pomiar
/// w balansatorze pokaże medianę realnych kontaktów z marką powyżej 13.
pub const BRAND_SLOTS: usize = 16;

/// Liczba kubełków tablicy zaniku: miesiąc po miesiącu, ostatni znaczy „rok i dłużej".
pub const DECAY_BUCKETS: usize = 13;

/// Jeden slot pamięci marki — dokładnie 8 B, tak samo jak wpis wiedzy i relacja.
///
/// `last_touch_day` to doba świata mod 65536 (≈ 182 lata gry przy roku 360-dniowym),
/// ta sama konwencja co w [`Knowledge`](crate::store::Knowledge).
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct BrandAffinity {
    /// Marka, której dotyczy wpis.
    pub brand: BrandId,
    /// Sympatia −100..=100 — **to jest „marka"** w rozumieniu PRD §7.6.
    pub affinity: i8,
    /// Czego mieszkaniec się po marce spodziewa (0..=100).
    pub expected_quality: u8,
    /// Jak mocno ją zna (0..=100) — świadomość z §5.7.
    pub awareness: u8,
    /// [`TouchSource`] jako indeks: najsilniejsze źródło, jakie ten wpis kiedykolwiek miał.
    pub source: u8,
    /// Doba ostatniego kontaktu, mod 65536.
    pub last_touch_day: u16,
}

impl BrandAffinity {
    /// Istotność wpisu przy wypychaniu siedemnastej marki (M10b §5.1).
    ///
    /// Silna niechęć waży tyle samo co silna sympatia — mieszkaniec, którego marka
    /// zawiodła, pamięta to równie dobrze. Remis rozstrzyga niższy `BrandId`, bo
    /// kolejność wypychania jest stanem świata i nie wolno jej zostawić przypadkowi.
    #[must_use]
    pub fn salience(&self) -> u32 {
        u32::from(self.affinity.unsigned_abs()) * 4 + u32::from(self.awareness)
    }

    /// Czy wpis jest przypięty — pracodawca albo sklep, w którym mieszkaniec bywa.
    ///
    /// Przypięcie jest odpowiedzią na ryzyko `R4` fazy M10: bez niego agresywna
    /// kampania wypchnęłaby z pamięci sklep, do którego ktoś chodzi od dziesięciu lat,
    /// a lojalność z §5.1 stałaby się funkcją cudzego budżetu reklamowego.
    #[must_use]
    pub fn is_pinned(&self) -> bool {
        self.source == TouchSource::Owned as u8
    }

    /// Źródło wpisu.
    #[must_use]
    pub fn source(&self) -> TouchSource {
        TouchSource::from_index(self.source as usize).unwrap_or(TouchSource::Ad)
    }
}

impl HashState for BrandAffinity {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u16(self.brand.0);
        h.write(&[
            self.affinity as u8,
            self.expected_quality,
            self.awareness,
            self.source,
        ]);
        h.write_u16(self.last_touch_day);
    }
}

/// Slab slotów marek — zasób świata, jeden na symulację.
pub type BrandSlab = Slab<BrandAffinity>;

/// Czym mieszkaniec właśnie zetknął się z marką.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Touch {
    /// Ekspozycja reklamowa: kanał i **deklarowana** jakość.
    ///
    /// Reklama nie zmienia afinitetu ani o punkt — tworzy oczekiwanie, nie sympatię.
    /// To jest cała asymetria z PRD §7.6 widziana od strony wejścia.
    Ad { channel: AdChannelKind, claim: Q },
    /// Zakup: **faktyczna** jakość zdjęta z partii.
    Experience { actual: Q },
    /// Plotka od kogoś, kto markę zna: jego wpis, waga relacji i wiarygodność źródła.
    Rumor {
        from: BrandAffinity,
        weight: u8,
        credibility: u8,
    },
    /// Publikacja w mediach: przekaz i wiarygodność tytułu.
    Media { claim: Q, credibility: u8 },
    /// Przypięcie: pracodawca albo sklep, w którym mieszkaniec bywa.
    Own,
}

impl Touch {
    /// Źródło, którym ten kontakt oznacza slot.
    #[must_use]
    pub fn source(&self) -> TouchSource {
        match self {
            Touch::Ad { .. } => TouchSource::Ad,
            Touch::Experience { .. } => TouchSource::Experience,
            Touch::Rumor { .. } => TouchSource::Rumor,
            Touch::Media { .. } => TouchSource::Media,
            Touch::Own => TouchSource::Owned,
        }
    }

    /// Kanał kampanii, jeśli kontakt był kampanią.
    #[must_use]
    pub fn channel(&self) -> Option<AdChannelKind> {
        match self {
            Touch::Ad { channel, .. } => Some(*channel),
            _ => None,
        }
    }
}

/// Zanik leniwy — stan wpisu **dziś**, bez żadnego systemu iterującego po slotach.
///
/// Liczony przy odczycie, O(1): dwa wyszukania w tablicy i dwa mnożenia. Oczekiwana
/// jakość **nie zanika** — mieszkaniec zapomina, że zna markę, i przestaje ją lubić,
/// ale dopóki pamięta, pamięta też, czego się po niej spodziewał.
///
/// `ponytail:` zanik przy odczycie zamiast przebiegu wsadowego. Sufit: gdyby profil
/// pokazał, że odczyty dominują nad kontaktami, przenieść na przebieg `EveryMonth`
/// po chunkach — ale nie wcześniej, bo dziś odczyt kosztuje dwa mnożenia, a przebieg
/// kosztowałby 6,4 mln slotów raz w miesiącu.
#[must_use]
pub fn decayed(mut a: BrandAffinity, today: u16, tune: &BrandTuning) -> BrandAffinity {
    let bucket = bucket_of(a.last_touch_day, today);
    let fa = i32::from(tune.decay_affinity[bucket]);
    let fw = u32::from(tune.decay_awareness[bucket]);
    // **Dzielenie, nie przesunięcie.** `>>` zaokrągla w dół, więc `-1` po każdym
    // mnożniku zostaje `-1`: niechęć nigdy nie dochodziłaby do zera, a sympatia
    // by dochodziła. Slot z niechęcią nie zwolniłby wtedy miejsca **nigdy**
    // i komplet szesnastu zapychałby się na stałe. Dzielenie całkowite obcina
    // ku zeru, więc obie strony skali gasną tak samo.
    a.affinity = ((i32::from(a.affinity) * fa) / 256) as i8;
    a.awareness = ((u32::from(a.awareness) * fw) / 256) as u8;
    a
}

/// Kubełek tablicy zaniku: pełne miesiące od ostatniego kontaktu, przycięte do roku.
fn bucket_of(last: u16, today: u16) -> usize {
    let wiek = today.wrapping_sub(last);
    (usize::from(wiek) / 30).min(DECAY_BUCKETS - 1)
}

/// Co mieszkaniec wie o marce **dziś**, z naniesionym zanikiem.
///
/// `None` znaczy „nie zna" i jest normalnym stanem świata: w decyzji zakupowej człon
/// marki jest wtedy zerem, a nie domysłem.
#[must_use]
pub fn affinity_of(
    world: &World,
    citizen: Entity,
    brand: BrandId,
    today: u64,
    tune: &BrandTuning,
) -> Option<BrandAffinity> {
    let bref = world.get::<BrandsRef>(citizen)?;
    let dzis = (today % 65_536) as u16;
    world
        .resource::<BrandSlab>()
        .entries(slab_ref(bref))
        .iter()
        .find(|s| s.brand == brand)
        .map(|s| decayed(*s, dzis, tune))
}

/// Komplet slotów jednego mieszkańca na stosie — 128 B, bez alokacji.
pub type BrandSlots = crate::arrayvec::ArrayVec<BrandAffinity, BRAND_SLOTS>;

/// Wszystkie marki, które mieszkaniec zna, z naniesionym zanikiem.
///
/// Bez alokacji: szesnaście slotów po 8 B mieści się na stosie, a funkcja leży
/// na ścieżce decyzji zakupowej (§7.5: odczyt afinitetu ≤ 80 ns). `Vec` kosztowałby
/// tu jedną alokację na każdą zaspokajaną potrzebę, czyli miliony na dobę gry.
#[must_use]
pub fn slots_of(world: &World, citizen: Entity, today: u64) -> BrandSlots {
    let tune = &world.resource::<BrandData>().memory;
    let dzis = (today % 65_536) as u16;
    let mut out = BrandSlots::new();
    if let Some(bref) = world.get::<BrandsRef>(citizen) {
        for s in world.resource::<BrandSlab>().entries(slab_ref(bref)) {
            out.push(decayed(*s, dzis, tune));
        }
    }
    out
}

/// Ilu mieszkańców zna markę i ilu jest w ogóle — agregat „siła marki" dla UI.
///
/// Odpowiednik [`crate::social::awareness_of`] dla marki zamiast dla miejsca, i tak
/// samo jak tamten: **liczony na żądanie, nie trzymany jako pole**. Pole byłoby drugim
/// źródłem prawdy o tym, ilu ludzi zna markę, i rozjechałoby się przy pierwszym
/// mieszkańcu, który umarł z jej wpisem w pamięci.
#[must_use]
pub fn brand_strength(world: &World, brand: BrandId, today: u64) -> BrandStrength {
    let tune = &world.resource::<BrandData>().memory;
    let dzis = (today % 65_536) as u16;
    let spis = world.resource::<crate::demography::Population>().citizens();
    let slab = world.resource::<BrandSlab>();
    let mut out = BrandStrength {
        population: spis.len() as u32,
        ..BrandStrength::default()
    };
    let mut suma_af = 0i64;
    let mut suma_ex = 0i64;
    for e in spis.iter() {
        let Some(bref) = world.get::<BrandsRef>(*e) else {
            continue;
        };
        let Some(s) = slab
            .entries(slab_ref(bref))
            .iter()
            .find(|s| s.brand == brand)
        else {
            continue;
        };
        let s = decayed(*s, dzis, tune);
        out.known += 1;
        suma_af += i64::from(s.affinity);
        suma_ex += i64::from(s.expected_quality);
    }
    if out.known > 0 {
        out.mean_affinity = (suma_af / i64::from(out.known)) as i8;
        out.mean_expected = Q::new((suma_ex / i64::from(out.known)) as u8);
    }
    out
}

/// Agregat marki policzony z pamięci mieszkańców — **wyłącznie dla UI i testów**.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct BrandStrength {
    pub known: u32,
    pub population: u32,
    pub mean_affinity: i8,
    pub mean_expected: Q,
}

/// Dopisuje albo odświeża slot marki. **Jedyna droga**, którą marka trafia do pamięci.
///
/// Jedno wejście znaczy jeden limit [`BRAND_SLOTS`], jedna reguła wypychania i jedno
/// miejsce, w którym powstaje powód do karty inspekcji — ta sama zasada, którą M3
/// postawił przy [`crate::social::learn_place`]. Zwraca powód, jeśli kontakt faktycznie
/// czymkolwiek ruszył; `None` znaczy „nic się nie zmieniło" i nie ma czego pokazywać.
pub fn touch(
    world: &mut World,
    citizen: Entity,
    brand: BrandId,
    t: Touch,
    day: u64,
) -> Option<DecisionReason> {
    let bref = world.get::<BrandsRef>(citizen).copied()?;
    let tune = world.resource::<BrandData>().memory.clone();
    let dzis = (day % 65_536) as u16;
    let mut sr = slab_ref(&bref);
    let slab = world.resource_mut::<BrandSlab>();

    let idx = slab.entries(sr).iter().position(|s| s.brand == brand);
    let powod = match idx {
        Some(i) => {
            let stary = decayed(slab.entries(sr)[i], dzis, &tune);
            let (n, p) = apply(stary, t, dzis, &tune);
            slab.entries_mut(sr)[i] = n;
            p
        }
        None => {
            let pusty = BrandAffinity {
                brand,
                affinity: 0,
                expected_quality: 0,
                awareness: 0,
                source: TouchSource::Ad as u8,
                last_touch_day: dzis,
            };
            let (n, p) = apply(pusty, t, dzis, &tune);
            wstaw(slab, &mut sr, n);
            p
        }
    };
    zapisz_uchwyt(world, citizen, sr);
    powod
}

/// Wstawia nowy slot, wypychając najmniej istotny, gdy komplet jest pełny.
///
/// Wypychanie idzie po `salience`, a nie po czasie: marka, o której mieszkaniec ma
/// mocne zdanie, przeżywa świeży plakat. Slot przypięty nie kandyduje **nigdy** —
/// gdyby kandydował, kampania konkurenta kasowałaby sklep, do którego ktoś chodzi
/// od dziesięciu lat (ryzyko `R4`).
fn wstaw(slab: &mut BrandSlab, sr: &mut SlabRef, nowy: BrandAffinity) {
    if sr.len as usize >= BRAND_SLOTS {
        let wpisy = slab.entries(*sr);
        let ofiara = wpisy
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.is_pinned())
            .min_by_key(|(_, s)| (s.salience(), s.brand.0))
            .map(|(i, _)| i);
        // Komplet przypiętych znaczy, że mieszkaniec ma szesnaście marek, z którymi
        // jest realnie związany. Siedemnasta po prostu się nie mieści i to jest
        // właściwa odpowiedź, a nie wypchnięcie pracodawcy.
        if let Some(i) = ofiara {
            slab.entries_mut(*sr)[i] = nowy;
        }
        return;
    }
    slab.push(sr, nowy, |wpisy| {
        wpisy
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.is_pinned())
            .min_by_key(|(_, s)| (s.salience(), s.brand.0))
            .map_or(0, |(i, _)| i)
    });
}

/// Rachunek jednego kontaktu — czysta funkcja, bo to ona jest treścią testów WP10.5.
///
/// Arytmetyka całkowita, stałe w 1/256 (00 §2: float nie ma prawa wejść do stanu,
/// a slot marki wchodzi do hasha).
#[must_use]
pub fn apply(
    mut s: BrandAffinity,
    t: Touch,
    dzis: u16,
    tune: &BrandTuning,
) -> (BrandAffinity, Option<DecisionReason>) {
    let przed = s;
    s.last_touch_day = dzis;
    let zrodlo = t.source() as u8;
    // Źródło silniejsze wygrywa — malejąca siła to rosnąca dyskryminanta, dokładnie
    // jak w `KnowledgeKind`: reklama nie zamienia „kupiłem i wiem" w „widziałem plakat".
    if zrodlo < s.source || przed.awareness == 0 {
        s.source = zrodlo;
    }

    let powod = match t {
        Touch::Ad { channel, claim } => {
            let gain = tune.awareness_gain[channel.as_index()];
            s.awareness = s.awareness.saturating_add(gain).min(100);
            s.expected_quality = ucz(s.expected_quality, claim.get(), tune.ad_learn);
            Some(DecisionReason::BrandLearned {
                brand: s.brand,
                source: TouchSource::Ad,
                channel: Some(channel),
                awareness: Q::new(s.awareness),
            })
        }
        Touch::Media { claim, credibility } => {
            // Wiarygodność tytułu moduluje **siłę** aktualizacji, nie jej kierunek:
            // gazeta, której czytelnik nie wierzy, dociera do niego tak samo, ale
            // przekonuje słabiej.
            let gain = tune.awareness_gain[AdChannelKind::Press.as_index()];
            s.awareness = s.awareness.saturating_add(gain).min(100);
            let sila = (u32::from(tune.ad_learn) * u32::from(credibility) / 100) as u16;
            s.expected_quality = ucz(s.expected_quality, claim.get(), sila);
            Some(DecisionReason::BrandLearned {
                brand: s.brand,
                source: TouchSource::Media,
                channel: Some(AdChannelKind::Press),
                awareness: Q::new(s.awareness),
            })
        }
        Touch::Experience { actual } => {
            let d = i32::from(actual.get()) - i32::from(s.expected_quality);
            let k = if d >= 0 { tune.k_up } else { tune.k_down };
            // Dzielenie ku zeru, nie `>>` — inaczej zachwyt o jeden punkt daje zero,
            // a rozczarowanie o jeden punkt daje minus jeden, i asymetria jest
            // w praktyce ostrzejsza od kalibracji z `brand.ron`.
            let delta = (d * i32::from(k)) / 256;
            s.affinity = (i32::from(s.affinity) + delta).clamp(-100, 100) as i8;
            // Znajomość rośnie tak, żeby `pin_visits` zakupów wyczerpało skalę —
            // i to jest cała reguła przypięcia (ryzyko `R4`). Licznika wizyt nie ma
            // i nie powstaje: `Knowledge` trzyma **jeden** wpis na cel i nie ma w nim
            // ani bitu wolnego, a ta sama informacja siedzi już w znajomości marki.
            let krok = 100 / tune.pin_visits.max(1);
            s.awareness = s.awareness.saturating_add(krok.max(1)).min(100);
            if s.awareness >= 100 && s.source == TouchSource::Experience as u8 {
                s.source = TouchSource::Owned as u8;
            }
            s.expected_quality = ucz(s.expected_quality, actual.get(), tune.exp_learn);
            Some(DecisionReason::BrandExperience {
                brand: s.brand,
                expected: Q::new(przed.expected_quality),
                actual,
                delta: delta as i16,
            })
        }
        Touch::Rumor {
            from,
            weight,
            credibility,
        } => {
            // Przekaz nie kopiuje afinitetu, tylko go przybliża — i nigdy nie podnosi
            // oczekiwań powyżej tego, co ma opowiadający. Inaczej dałoby się nakręcić
            // markę pętlą plotek między dwoma mieszkańcami.
            let waga = u32::from(tune.rumor_pull) * u32::from(weight) * u32::from(credibility);
            let d = i64::from(from.affinity) - i64::from(s.affinity);
            let delta = (d * i64::from(waga)) / (256 * 100 * 100);
            s.affinity = (i64::from(s.affinity) + delta).clamp(-100, 100) as i8;
            s.awareness = s.awareness.max(from.awareness / 2);
            // Oczekiwania idą **ku** oczekiwaniom opowiadającego tym samym krokiem,
            // co afinitet, i **nigdy powyżej** jego wartości: dwaj mieszkańcy
            // opowiadający sobie nawzajem nie nakręcą marki w nieskończoność.
            let ku = (u32::from(tune.rumor_pull) * u32::from(weight) * u32::from(credibility))
                / (100 * 100);
            let cel = from.expected_quality;
            let po = ucz(przed.expected_quality, cel, ku.min(256) as u16);
            s.expected_quality = if cel >= przed.expected_quality {
                po.min(cel)
            } else {
                po.max(cel)
            };
            if s == przed {
                None
            } else {
                Some(DecisionReason::BrandLearned {
                    brand: s.brand,
                    source: TouchSource::Rumor,
                    channel: None,
                    awareness: Q::new(s.awareness),
                })
            }
        }
        Touch::Own => {
            s.source = TouchSource::Owned as u8;
            s.awareness = s.awareness.max(60);
            Some(DecisionReason::BrandLearned {
                brand: s.brand,
                source: TouchSource::Owned,
                channel: None,
                awareness: Q::new(s.awareness),
            })
        }
    };
    (s, powod)
}

/// Krok uczenia ku wartości `cel`, współczynnik w 1/256.
///
/// **Sufit jest nazwany:** przy `ad_learn = 24` różnica poniżej jedenastu punktów
/// daje krok zerowy, więc oczekiwania zatrzymują się kilka punktów pod obietnicą
/// i nigdy jej nie dosięgają. Tak ma być — reklama nie przekonuje do końca, a krok
/// ułamkowy wymagałby trzymania reszty w slocie, czyli dwóch bajtów na mieszkańca
/// za dokładność, której nikt nie zobaczy.
fn ucz(teraz: u8, cel: u8, k: u16) -> u8 {
    let d = i32::from(cel) - i32::from(teraz);
    (i32::from(teraz) + ((d * i32::from(k)) / 256)).clamp(0, 100) as u8
}

/// Kompaktowanie wygasłych slotów — **jedyny** przebieg iterujący po pamięci marek.
///
/// Wołany raz w miesiącu z rytmu społeczeństwa. Nie liczy niczego: zdejmuje sloty,
/// które zanik sprowadził do zera na obu skalach, i oddaje blok do wolnej listy.
/// Slot przypięty nie wygasa, bo przypięcie mówi o relacji, a nie o pamięci.
pub fn compact_month(world: &mut World, day: u64) -> u32 {
    let tune = world.resource::<BrandData>().memory.clone();
    let dzis = (day % 65_536) as u16;
    let spis: Vec<Entity> = world
        .resource::<crate::demography::Population>()
        .citizens()
        .to_vec();
    let mut zdjete = 0u32;
    for e in spis {
        let Some(bref) = world.get::<BrandsRef>(e).copied() else {
            continue;
        };
        let mut sr = slab_ref(&bref);
        if sr.is_empty() {
            continue;
        }
        let slab = world.resource_mut::<BrandSlab>();
        let mut i = 0usize;
        while i < sr.len as usize {
            let s = decayed(slab.entries(sr)[i], dzis, &tune);
            if !s.is_pinned() && s.awareness == 0 && s.affinity == 0 {
                slab.remove_at(&mut sr, i);
                zdjete += 1;
            } else {
                // **Wartość zanikła się nie zapisuje.** Zanik jest funkcją doby
                // ostatniego kontaktu, a ta się tu nie zmienia — wpisanie wyniku
                // z powrotem do slotu znaczyłoby, że następny odczyt zanika go
                // drugi raz od tego samego punktu. Ten przebieg **tylko zwalnia
                // miejsce** i to jest cała jego rola.
                i += 1;
            }
        }
        zapisz_uchwyt(world, e, sr);
    }
    zdjete
}

/// Uchwyt slotów marek jako `SlabRef`. Jedno miejsce konwersji, bo `SlabRef::EMPTY`
/// jest sentinelem, a `BrandsRef::default()` zerem — ta sama pułapka i to samo
/// rozwiązanie co przy [`crate::demography::knowledge_ref`].
fn slab_ref(b: &BrandsRef) -> SlabRef {
    if b.len == 0 {
        SlabRef::EMPTY
    } else {
        SlabRef {
            handle: b.handle,
            len: b.len,
            class: b.class,
        }
    }
}

fn zapisz_uchwyt(world: &mut World, citizen: Entity, sr: SlabRef) {
    if let Some(slot) = world.get_mut::<BrandsRef>(citizen) {
        slot.handle = sr.handle;
        slot.len = sr.len;
        slot.class = sr.class;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pusty(brand: u16) -> BrandAffinity {
        BrandAffinity {
            brand: BrandId(brand),
            affinity: 0,
            expected_quality: 0,
            awareness: 0,
            source: TouchSource::Ad as u8,
            last_touch_day: 0,
        }
    }

    /// `data/tuning/brand.ron` **wczytuje się i przechodzi walidację**.
    ///
    /// Test powstał po M10b i znalazł błąd w tej samej godzinie, w której powstał:
    /// plik miał tablice o stałym rozmiarze zapisane jako listy `[...]`, a RON czyta
    /// takie jak krotki. Przez cały czas, kiedy `register_resources` wstawiało
    /// `BrandData::default()`, nikt pliku nie otwierał — więc parsował się dopiero
    /// wtedy, gdy przestał być martwy. Loader bez testu jest loaderem bez dowodu.
    #[test]
    fn plik_strojenia_marki_sie_wczytuje() {
        let d = BrandData::load_default().expect("data/tuning/brand.ron");
        assert!(
            d.memory.k_down > d.memory.k_up,
            "asymetria PRD §7.6 odwrócona w danych"
        );
        assert_eq!(d.memory.decay_affinity[0], 256, "zanik w dobie kontaktu");
        assert!(d.outlets.stories_max >= d.outlets.stories_min);
        assert!(d.channels.billboard_notice_bps > 0 && d.channels.billboard_notice_bps <= 10_000);
        assert!(d.channels.cpm(AdChannelKind::Tv) > d.channels.cpm(AdChannelKind::Leaflet));
    }

    #[test]
    fn slot_ma_osiem_bajtow() {
        // 400 tys. mieszkańców × 16 slotów × 8 B = 51,2 MB (M10b §5.1, wariant C).
        assert_eq!(size_of::<BrandAffinity>(), 8);
    }

    #[test]
    fn reklama_podnosi_oczekiwania_powoli() {
        // Kryterium WP10.5: +20 Q oczekiwań wymaga ≥ 7 ekspozycji.
        let tune = BrandTuning::default();
        let mut s = pusty(1);
        s.expected_quality = 50;
        let cel = s.expected_quality + 20;
        let mut n = 0;
        while s.expected_quality < cel {
            let (n2, _) = apply(
                s,
                Touch::Ad {
                    channel: AdChannelKind::Billboard,
                    claim: Q::new(90),
                },
                0,
                &tune,
            );
            s = n2;
            n += 1;
            assert!(n < 100, "oczekiwania nie rosną — kalibracja `ad_learn`");
        }
        assert!(n >= 7, "+20 Q po {n} ekspozycjach, wymagane ≥ 7");
        // Reklama nie rusza sympatii ani o punkt.
        assert_eq!(s.affinity, 0);
    }

    #[test]
    fn rozczarowanie_kosztuje_trzy_razy_wiecej_niz_zachwyt_daje() {
        // Kryterium WP10.5: −20 Q rozczarowania kosztuje ≥ 14 pkt afinitetu,
        // a odbudowa wymaga ≥ 3 doświadczeń o tej samej sile.
        let tune = BrandTuning::default();
        let mut s = pusty(1);
        s.expected_quality = 70;
        s.awareness = 80;

        let (po, _) = apply(s, Touch::Experience { actual: Q::new(50) }, 0, &tune);
        assert!(
            po.affinity <= -14,
            "rozczarowanie o 20 Q zabrało {} pkt, wymagane ≥ 14",
            -po.affinity
        );

        // Odbudowa: każde kolejne doświadczenie o tej samej sile (+20 wobec bieżących
        // oczekiwań) — liczymy, ile ich trzeba, żeby wrócić do zera.
        let strata = po.affinity;
        s = po;
        let mut n = 0;
        while s.affinity < 0 {
            let cel = Q::new((s.expected_quality + 20).min(100));
            let (n2, _) = apply(s, Touch::Experience { actual: cel }, 0, &tune);
            s = n2;
            n += 1;
            assert!(n < 50, "afinitet nie wraca — kalibracja `k_up`");
        }
        assert!(
            n >= 3,
            "odbudowa {strata} pkt zajęła {n} doświadczeń, wymagane ≥ 3"
        );
    }

    #[test]
    fn obietnica_ponad_stan_jest_samokarzaca() {
        // Gracz, który reklamuje tandetę, niszczy sobie markę własną kampanią:
        // reklama podnosi `expected_quality`, więc każdy kolejny zakup boli mocniej.
        let tune = BrandTuning::default();
        let towar = Q::new(40);

        let mut bez = pusty(1);
        bez.expected_quality = 40;
        let (bez, _) = apply(bez, Touch::Experience { actual: towar }, 0, &tune);

        let mut z = pusty(1);
        z.expected_quality = 40;
        for _ in 0..12 {
            let (n, _) = apply(
                z,
                Touch::Ad {
                    channel: AdChannelKind::Tv,
                    claim: Q::new(95),
                },
                0,
                &tune,
            );
            z = n;
        }
        let (z, _) = apply(z, Touch::Experience { actual: towar }, 0, &tune);

        assert_eq!(bez.affinity, 0, "zgodne oczekiwania nie ruszają sympatii");
        assert!(
            z.affinity < bez.affinity,
            "kampania nie ukarała przereklamowanej tandety: {} vs {}",
            z.affinity,
            bez.affinity
        );
    }

    #[test]
    fn zanik_jest_leniwy_i_monotoniczny() {
        let tune = BrandTuning::default();
        let mut s = pusty(1);
        s.affinity = 80;
        s.awareness = 90;
        s.expected_quality = 70;
        let po_miesiacu = decayed(s, 30, &tune);
        let po_roku = decayed(s, 360, &tune);
        assert!(po_miesiacu.affinity < s.affinity);
        assert!(po_roku.awareness < po_miesiacu.awareness);
        assert_eq!(
            po_roku.expected_quality, s.expected_quality,
            "oczekiwana jakość nie zanika — zanika pamięć, nie treść"
        );

        // Niechęć gaśnie tak samo jak sympatia. Przy przesunięciu bitowym `-1`
        // zostawało `-1` w nieskończoność, więc slot z niechęcią nigdy nie zwalniał
        // miejsca i komplet szesnastu zapychał się na stałe.
        let mut zly = pusty(2);
        zly.affinity = -3;
        zly.awareness = 1;
        let mut lata = 0;
        while zly.affinity != 0 || zly.awareness != 0 {
            zly = decayed(zly, 360 * lata, &tune);
            zly.last_touch_day = 0;
            lata += 1;
            assert!(lata < 50, "niechęć nie dochodzi do zera: {}", zly.affinity);
        }
    }

    #[test]
    fn plotka_nie_nakreca_oczekiwan_petla() {
        let tune = BrandTuning::default();
        let mut a = pusty(1);
        a.affinity = 60;
        a.awareness = 80;
        a.expected_quality = 75;
        let mut b = pusty(1);
        b.expected_quality = 30;
        let b0 = b.expected_quality;
        for _ in 0..20 {
            let (nb, _) = apply(
                b,
                Touch::Rumor {
                    from: a,
                    weight: 100,
                    credibility: 100,
                },
                0,
                &tune,
            );
            b = nb;
            let (na, _) = apply(
                a,
                Touch::Rumor {
                    from: b,
                    weight: 100,
                    credibility: 100,
                },
                0,
                &tune,
            );
            a = na;
        }
        // Górny warunek jest sufitem, ale sam w sobie przechodziłby także wtedy,
        // gdyby plotka nie robiła **nic** — i tak było przed recenzją M10b.
        assert!(
            b.expected_quality > b0,
            "plotka nie ruszyła oczekiwań: {b0} → {}",
            b.expected_quality
        );
        assert!(b.expected_quality <= 75, "plotka nakręciła oczekiwania");
        assert!(a.expected_quality <= 75);
    }

    #[test]
    fn staly_klient_przypina_slot_i_kampania_go_nie_wypycha() {
        // Ryzyko `R4` fazy M10: agresywna kampania nie ma prawa wypchnąć z pamięci
        // sklepu, do którego mieszkaniec chodzi od lat.
        let tune = BrandTuning::default();
        let mut moj = pusty(7);
        moj.expected_quality = 60;
        for _ in 0..u32::from(tune.pin_visits) + 1 {
            let (n, _) = apply(moj, Touch::Experience { actual: Q::new(60) }, 0, &tune);
            moj = n;
        }
        assert!(
            moj.is_pinned(),
            "po {} zakupach slot nie jest przypięty (znajomość {})",
            tune.pin_visits,
            moj.awareness
        );

        let mut slab = BrandSlab::new();
        let mut sr = SlabRef::EMPTY;
        wstaw(&mut slab, &mut sr, moj);
        // Piętnaście obcych marek zapełnia resztę kompletu, szesnasta wypycha...
        for i in 0..BRAND_SLOTS as u16 + 8 {
            let mut obca = pusty(100 + i);
            obca.awareness = 90;
            wstaw(&mut slab, &mut sr, obca);
        }
        assert!(
            slab.entries(sr).iter().any(|s| s.brand == BrandId(7)),
            "kampania wypchnęła przypiętą markę stałego klienta"
        );
        assert_eq!(slab.entries(sr).len(), BRAND_SLOTS);
    }

    #[test]
    fn kompaktowanie_nie_zanika_slotu_drugi_raz() {
        // Zanik jest funkcją doby ostatniego kontaktu i liczy się **przy odczycie**.
        // Miesięczne kompaktowanie ma tylko zwalniać miejsce — gdyby zapisywało
        // wynik zaniku z powrotem do slotu, następny odczyt zanikałby go od nowa
        // i marka gasłaby dwa razy szybciej, niż mówi tablica.
        let tune = BrandTuning::default();
        let mut slab = BrandSlab::new();
        let mut sr = SlabRef::EMPTY;
        let mut s = pusty(3);
        s.affinity = 60;
        s.awareness = 80;
        wstaw(&mut slab, &mut sr, s);

        let po_pol_roku = decayed(slab.entries(sr)[0], 180, &tune);
        // Kompaktowanie po trzech miesiącach: slot żyje, więc zostaje nietknięty.
        let przed = slab.entries(sr)[0];
        assert_eq!(przed, s, "kompaktowanie nie ma prawa ruszyć żywego slotu");
        assert_eq!(decayed(slab.entries(sr)[0], 180, &tune), po_pol_roku);
    }

    #[test]
    fn istotnosc_wazy_niechec_tak_samo_jak_sympatie() {
        let mut lubi = pusty(1);
        lubi.affinity = 50;
        let mut nie_lubi = pusty(2);
        nie_lubi.affinity = -50;
        assert_eq!(lubi.salience(), nie_lubi.salience());
    }
}
