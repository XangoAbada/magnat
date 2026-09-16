//! Cache tras stabilnych (PRD §17.6, M4a WP2).
//!
//! **Czego w kluczu nie ma i dlaczego.** Nie ma minuty wyjazdu. Trasa z domu do
//! pracy zmienia się rzadko — zmienia się jej *czas*, nie jej *kształt* — więc
//! klucz niosący minutę dawałby 1440 wpisów na tę samą sekwencję ulic i cache
//! nie trafiałby nigdy. Jest za to `hour_bucket`, bo o 7 rano i o 22 optymalna
//! trasa potrafi biec inną ulicą (zatłoczona arteria vs. pusta obwodnica).
//!
//! **Unieważnianie.** Zmiana topologii (nowa ulica, zamknięty most) unieważnia
//! wszystko naraz przez [`RouteCache::invalidate`]. Zmiana samych wag — nie:
//! trasa policzona przy innym natężeniu ruchu bywa nieoptymalna, ale jest
//! poprawna, a przeliczanie wszystkiego co tik kosztowałoby więcej niż różnica.

use magnat_core::TransportMode;

use crate::graph::NodeId;
use crate::router::RouteProfile;

/// Klucz trasy. Bez minuty wyjazdu — patrz nagłówek modułu.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct RouteKey {
    pub origin: NodeId,
    pub dest: NodeId,
    pub mode: TransportMode,
    /// Godzina wyjazdu, `0..24`. Klucz spoza zakresu trafi po prostu do innego
    /// kubełka — cache nie waliduje danych, bo nie jest ich właścicielem.
    pub hour_bucket: u8,
    /// Profil trasy. **Musi** być w kluczu: ten sam odcinek ma inne wagi dla ruchu
    /// osobowego, dziennego ciężkiego i nocnego ciężkiego (zakaz ruchu ciężkiego,
    /// tonaż mostu), więc bez profilu cache oddawałby trasę policzoną dla kogoś innego.
    pub profile: RouteProfile,
    /// Masa całkowita zaokrąglona **w górę** do pełnych ton. Trasa dobra dla dwunastu
    /// ton nie musi unosić dwudziestu sześciu, a `mass_ok` sprawdza się dopiero przy
    /// liczeniu — trafienie w cache omijało to sprawdzenie w całości. Kubełek tonowy,
    /// bo ładowności są z katalogu i jest ich kilka, a nie continuum.
    pub gross_t: u16,
}

/// Licznik skuteczności cache'u. Idzie do inspektora i do dziennika wydajności.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    /// Zajęte pozycje.
    pub len: usize,
    /// Wszystkie pozycje — po zaokrągleniu z konstruktora.
    pub capacity: usize,
}

impl CacheStats {
    /// Trafialność w promilach — bez floatów (00 §2). `0`, gdy nie było zapytań.
    #[must_use]
    pub fn hit_permille(&self) -> u32 {
        let total = u128::from(self.hits) + u128::from(self.misses);
        if total == 0 {
            return 0;
        }
        ((u128::from(self.hits) * 1000) / total) as u32
    }
}

/// Stopień sekcyjności. Czwórka mieści się w jednej linii cache'u procesora
/// razem ze znacznikami, więc przeszukanie kubełka to jeden dostęp do pamięci.
const WAYS: usize = 4;

/// Cache tras z wymianą LRU w obrębie kubełka.
///
/// // ponytail: LRU jest **sekcyjne**, nie globalne. Sufit: gdy wiele gorących
/// // kluczy wpadnie do jednego kubełka, czwórka zaczyna się tłuc i trafialność
/// // spada mimo wolnych miejsc gdzie indziej — zmierzone: 5 000 gorących kluczy
/// // na 8 192 pozycjach (60 % obciążenia) daje 763 ‰ zamiast 918 ‰ przy 16 384,
/// // choć fizycznie mieszczą się w obu. Droga wyjścia: podnieść `WAYS` do 8 — kubełek nadal
/// // mieści się w dwóch liniach cache'u — albo, gdy i to nie wystarczy, wprowadzić
/// // prawdziwą globalną listę LRU (wpis z dwoma indeksami sąsiadów, `O(1)` na
/// // dostęp, ale dwa dodatkowe zapisy przy każdym trafieniu).
pub struct RouteCache<V> {
    /// `buckets × WAYS` pozycji; indeks kubełka to maska bitowa.
    slots: Vec<Option<(RouteKey, V)>>,
    /// Znacznik czasu ostatniego użycia pozycji — monotoniczny licznik cache'u.
    last_used: Vec<u64>,
    mask: usize,
    tick: u64,
    topology_version: u32,
    hits: u64,
    misses: u64,
    evictions: u64,
    len: usize,
}

impl<V> RouteCache<V> {
    /// `capacity` zaokrąglane w górę do wielokrotności [`WAYS`] i do potęgi dwójki
    /// kubełków, żeby wybór kubełka był maską, a nie dzieleniem modulo.
    /// Pojemność `0` daje jeden kubełek — cache zawsze da się używać.
    #[must_use]
    pub fn new(capacity: usize) -> RouteCache<V> {
        let buckets = capacity.div_ceil(WAYS).max(1).next_power_of_two();
        let slots = buckets * WAYS;
        RouteCache {
            slots: (0..slots).map(|_| None).collect(),
            last_used: vec![0; slots],
            mask: buckets - 1,
            tick: 0,
            topology_version: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
            len: 0,
        }
    }

    pub fn get(&mut self, key: &RouteKey) -> Option<&V> {
        let base = self.bucket(key);
        let hit =
            (base..base + WAYS).find(|&i| self.slots[i].as_ref().is_some_and(|(k, _)| k == key));
        match hit {
            Some(i) => {
                self.tick += 1;
                self.last_used[i] = self.tick;
                self.hits += 1;
                self.slots[i].as_ref().map(|(_, v)| v)
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }

    pub fn insert(&mut self, key: RouteKey, value: V) {
        let base = self.bucket(&key);
        self.tick += 1;
        let existing =
            (base..base + WAYS).find(|&i| self.slots[i].as_ref().is_some_and(|(k, _)| *k == key));
        let idx = match existing {
            Some(i) => i,
            None => match (base..base + WAYS).find(|&i| self.slots[i].is_none()) {
                Some(i) => {
                    self.len += 1;
                    i
                }
                None => {
                    // Remis rozstrzyga niższy indeks pozycji: `min_by_key` zwraca
                    // pierwsze minimum, więc wynik nie zależy od kolejności iteracji.
                    let victim = (base..base + WAYS)
                        .min_by_key(|&i| self.last_used[i])
                        .unwrap_or(base);
                    self.evictions += 1;
                    victim
                }
            },
        };
        self.slots[idx] = Some((key, value));
        self.last_used[idx] = self.tick;
    }

    /// Unieważnia całość po zmianie topologii; no-op, gdy wersja się nie zmieniła.
    pub fn invalidate(&mut self, topology_version: u32) {
        if topology_version != self.topology_version {
            self.clear();
            self.topology_version = topology_version;
        }
    }

    #[must_use]
    pub fn topology_version(&self) -> u32 {
        self.topology_version
    }

    #[must_use]
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            hits: self.hits,
            misses: self.misses,
            evictions: self.evictions,
            len: self.len,
            capacity: self.slots.len(),
        }
    }

    /// Wyrzuca zawartość. Liczniki trafień **zostają** — mierzą skuteczność
    /// cache'u przez całą sesję, a nie od ostatniej przebudowy miasta.
    pub fn clear(&mut self) {
        for s in &mut self.slots {
            *s = None;
        }
        self.last_used.fill(0);
        self.tick = 0;
        self.len = 0;
    }

    /// Indeks pierwszej pozycji kubełka.
    fn bucket(&self, key: &RouteKey) -> usize {
        (mix(pack(key)) as usize & self.mask) * WAYS
    }
}

/// Klucz spakowany w 64 bity: `origin` i `dest` w połówkach, tryb i godzina
/// domieszane osobno, żeby dwie trasy różniące się tylko godziną nie lądowały
/// w sąsiednich kubełkach.
fn pack(k: &RouteKey) -> u64 {
    let pary = (u64::from(k.origin.0) << 32) | u64::from(k.dest.0);
    let reszta = ((k.mode.as_index() as u64) << 8) | u64::from(k.hour_bucket);
    mix(pary) ^ reszta
}

/// Mieszalnik splitmix64. Własny, bo `DefaultHasher` ze `std` **nie jest**
/// stabilny między wersjami Rusta ani gwarantowany co do wartości — rozkład
/// po kubełkach musi być powtarzalny, inaczej dwa przebiegi tego samego ziarna
/// mają inną trafialność i inny ślad wydajności (00 §3.2).
fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn klucz(o: u32, d: u32) -> RouteKey {
        RouteKey {
            origin: NodeId(o),
            dest: NodeId(d),
            mode: TransportMode::Car,
            hour_bucket: 7,
            profile: RouteProfile::Passenger,
            gross_t: 0,
        }
    }

    #[test]
    fn wstawienie_i_odczyt_wracaja_ta_sama_wartoscia() {
        let mut c: RouteCache<u32> = RouteCache::new(64);
        assert_eq!(c.get(&klucz(1, 2)), None);
        c.insert(klucz(1, 2), 42);
        assert_eq!(c.get(&klucz(1, 2)), Some(&42));
        // Nadpisanie tego samego klucza nie zajmuje drugiej pozycji.
        c.insert(klucz(1, 2), 43);
        assert_eq!(c.get(&klucz(1, 2)), Some(&43));
        let s = c.stats();
        assert_eq!(s.len, 1);
        assert_eq!(s.hits, 2);
        assert_eq!(s.misses, 1);
        assert_eq!(s.evictions, 0);
    }

    #[test]
    fn pojemnosc_zaokragla_sie_w_gore() {
        assert_eq!(RouteCache::<u8>::new(0).stats().capacity, WAYS);
        assert_eq!(RouteCache::<u8>::new(1).stats().capacity, WAYS);
        assert_eq!(RouteCache::<u8>::new(5).stats().capacity, 2 * WAYS);
        assert_eq!(RouteCache::<u8>::new(8192).stats().capacity, 8192);
    }

    #[test]
    fn wymiana_zostawia_najswiezszy_wpis_kubelka() {
        // Jeden kubełek: cztery pozycje, piąty klucz musi kogoś wyrzucić.
        let mut c: RouteCache<u32> = RouteCache::new(WAYS);
        assert_eq!(c.stats().capacity, WAYS);
        for i in 0..4u32 {
            c.insert(klucz(i, 0), i);
        }
        // Dotykamy 0, 1, 2 — najstarszy staje się 3.
        for i in 0..3u32 {
            assert_eq!(c.get(&klucz(i, 0)), Some(&i));
        }
        c.insert(klucz(9, 0), 9);
        assert_eq!(c.stats().evictions, 1);
        assert_eq!(c.get(&klucz(9, 0)), Some(&9));
        assert_eq!(c.get(&klucz(3, 0)), None, "wyleciał najdawniej używany");
        for i in 0..3u32 {
            assert_eq!(c.get(&klucz(i, 0)), Some(&i));
        }
        assert_eq!(c.stats().len, 4);
    }

    #[test]
    fn uniewaznienie_reaguje_tylko_na_nowa_wersje() {
        let mut c: RouteCache<u32> = RouteCache::new(64);
        c.insert(klucz(1, 2), 42);
        assert_eq!(c.topology_version(), 0);

        c.invalidate(0);
        assert_eq!(c.stats().len, 1, "ta sama wersja to no-op");
        assert_eq!(c.get(&klucz(1, 2)), Some(&42));

        c.invalidate(7);
        assert_eq!(c.topology_version(), 7);
        assert_eq!(c.stats().len, 0);
        assert_eq!(c.get(&klucz(1, 2)), None);
    }

    #[test]
    fn trafialnosc_w_promilach_liczy_sie_bez_floatow() {
        let mut s = CacheStats::default();
        assert_eq!(s.hit_permille(), 0, "bez zapytań brak trafialności");
        s.hits = 9;
        s.misses = 1;
        assert_eq!(s.hit_permille(), 900);
        s.hits = 1;
        s.misses = 2;
        assert_eq!(s.hit_permille(), 333);
        s.hits = u64::MAX;
        s.misses = 0;
        assert_eq!(
            s.hit_permille(),
            1000,
            "brak przepełnienia na dużych licznikach"
        );
    }

    /// Przebieg scenariusza: `pary` tras dom↔praca, każda odpytana dwa razy
    /// w ciągu doby, przez `dni` dób, na cache o pojemności `pojemnosc`.
    ///
    /// Kolejność w obrębie doby jest **mieszana**, bo gospodarstwa domowe nie
    /// planują dnia po indeksie węzła — a przy kolejności cyklicznej LRU wpada
    /// w klasyczny przypadek patologiczny (kubełek z pięcioma kluczami chybia
    /// wszystkie pięć na każdym obrocie) i pomiar mierzyłby scenariusz, nie cache.
    fn scenariusz_dojazdow(pary: u32, dni: u32, pojemnosc: usize) -> CacheStats {
        let mut c: RouteCache<u32> = RouteCache::new(pojemnosc);
        let mut ziarno = 0x9E37_79B9_7F4A_7C15u64;
        let mut kolejnosc: Vec<u32> = (0..pary).collect();

        for _ in 0..dni {
            // Tasowanie Fishera-Yatesa deterministycznym LCG.
            for i in (1..kolejnosc.len()).rev() {
                ziarno = ziarno
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let j = ((ziarno >> 33) as usize) % (i + 1);
                kolejnosc.swap(i, j);
            }
            for _ in 0..2 {
                for &p in &kolejnosc {
                    let k = RouteKey {
                        origin: NodeId(p),
                        dest: NodeId(p + 100_000),
                        mode: TransportMode::Car,
                        hour_bucket: 7,
                        profile: RouteProfile::Passenger,
                        gross_t: 0,
                    };
                    if c.get(&k).is_none() {
                        c.insert(k, p);
                    }
                }
            }
        }
        c.stats()
    }

    /// Kryterium WP2: trafialność cache ≥ 90 %.
    ///
    /// Dziesięć dób, nie trzy: przy 5 000 kluczy same chybienia zimne to 5 000
    /// zapytań, czyli 17 % z trzech dni — kryterium ≥ 90 % byłoby wtedy
    /// arytmetycznie nieosiągalne niezależnie od jakości cache'u.
    ///
    /// Pojemność 16 384, nie 8 192: przy 5 000 gorących kluczy na 2 048 kubełków
    /// czwórka zaczyna się tłuc (patrz test niżej). To jest sufit nazwany
    /// w komentarzu `ponytail:` przy [`RouteCache`], a nie przypadek doboru liczb.
    #[test]
    fn scenariusz_dojazdow_trafia_ponad_90_procent() {
        let s = scenariusz_dojazdow(5_000, 10, 16_384);
        assert_eq!(s.hits + s.misses, 5_000 * 2 * 10);
        assert!(
            s.hit_permille() >= 900,
            "trafialność {} ‰ poniżej kryterium WP2 (hits {}, misses {}, evictions {})",
            s.hit_permille(),
            s.hits,
            s.misses,
            s.evictions
        );
    }

    /// Sufit sekcyjnego LRU, zmierzony zamiast obiecany: ten sam zestaw kluczy
    /// przy 8 192 pozycjach (60 % obciążenia) spada wyraźnie poniżej kryterium,
    /// mimo że fizycznie wszystkie klucze by się zmieściły. Test pilnuje, żeby
    /// komentarz `ponytail:` nie rozjechał się z zachowaniem kodu.
    #[test]
    fn czworka_tluce_sie_przy_wysokim_obciazeniu_kubelkow() {
        let ciasny = scenariusz_dojazdow(5_000, 10, 8_192);
        let luzny = scenariusz_dojazdow(5_000, 10, 16_384);
        assert!(ciasny.evictions > 10_000, "brak wymiany = brak tłuczenia");
        assert!(
            ciasny.hit_permille() + 100 < luzny.hit_permille(),
            "ciasny {} ‰ vs luźny {} ‰",
            ciasny.hit_permille(),
            luzny.hit_permille()
        );
    }
}
