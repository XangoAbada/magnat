//! Zapytanie widoku i deterministyczny wybór top-K (M11a §5.2).
//!
//! [`ViewQuery`] jest **jedynym kanałem render → sim** i nie może wpływać na stan: jest
//! `Copy`, nie zawiera referencji ani uchwytów, a wypełniacz bierze ją przez wartość.
//! Ryzyko `R10` fazy („ktoś w przyszłości tymczasowo doda pole zwrotne") jest tu zamknięte
//! kształtem typu, a nie regulaminem — pole zwrotne w strukturze `Copy` bez referencji
//! nie ma gdzie oddać wyniku.

/// Prostopadłościan w milimetrach świata. Tych samych jednostek używają pozycje
/// w rekordach, więc test przynależności nie przelicza niczego.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct Aabb {
    pub min: [i32; 3],
    pub max: [i32; 3],
}

impl Aabb {
    #[must_use]
    pub const fn contains(&self, p: [i32; 3]) -> bool {
        p[0] >= self.min[0]
            && p[0] <= self.max[0]
            && p[1] >= self.min[1]
            && p[1] <= self.max[1]
            && p[2] >= self.min[2]
            && p[2] <= self.max[2]
    }

    /// Prostopadłościan wokół punktu o zadanym półboku poziomym i pionowym.
    /// Nasycenie zamiast zawijania: kadr przy krawędzi mapy ma się rozciągać do niej,
    /// a nie przeskoczyć na drugą stronę świata.
    #[must_use]
    pub const fn around(c: [i32; 3], r_xy_mm: i32, r_z_mm: i32) -> Aabb {
        Aabb {
            min: [
                c[0].saturating_sub(r_xy_mm),
                c[1].saturating_sub(r_xy_mm),
                c[2].saturating_sub(r_z_mm),
            ],
            max: [
                c[0].saturating_add(r_xy_mm),
                c[1].saturating_add(r_xy_mm),
                c[2].saturating_add(r_z_mm),
            ],
        }
    }
}

/// Twarde limity zawartości snapshotu.
///
/// **Cap jest stałą konfiguracji, nie zmienną runtime'u** (decyzja 9.11, zamknięta przez
/// koordynatora). `RenderBudget` z M11e nie ma prawa go obniżać: obniżanie capu byłoby
/// sprzężeniem render → symulacja, czyli dokładnie tym, czemu ten crate zapobiega.
/// Podnieść go wolno opcją dla mocnego sprzętu — przed startem świata, nie w klatce.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct SnapshotCaps {
    pub citizens: u32,
    pub vehicles: u32,
    pub sites: u32,
    pub crowd: u32,
}

impl SnapshotCaps {
    /// Wartości z rachunku pasma §5.2. Suma daje ≈1,30 MB na bufor.
    pub const DEFAULT: SnapshotCaps = SnapshotCaps {
        citizens: 24_576,
        vehicles: 8_192,
        sites: 4_096,
        crowd: 2_048,
    };
}

impl Default for SnapshotCaps {
    fn default() -> Self {
        SnapshotCaps::DEFAULT
    }
}

/// Czego render chce od symulacji w tej publikacji.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct ViewQuery {
    /// Stożek widzenia rozszerzony o 20 % — histereza na obrót kamery, żeby encja
    /// przy krawędzi kadru nie wpadała i nie wypadała co klatkę.
    pub aabb: Aabb,
    /// Oko kamery w milimetrach — punkt odniesienia dla wyboru top-K.
    pub eye: [i32; 3],
    pub caps: SnapshotCaps,
    /// Czas animacji w milisekundach — monotoniczny, stoi przy pauzie (M11b §5.4).
    ///
    /// Nie jest minutą symulacji i nie ma nią być: minuta trwa sekundę realną przy
    /// prędkości ×1, więc klip przesuwałby się o jedną klatkę na sekundę i postać
    /// chodziłaby w zwolnionym tempie. Nie jest też czasem świata mnożonym przez
    /// prędkość — przy ×10 nogi przebierałyby dziesięć razy szybciej, niż da się
    /// zobaczyć. Jest zegarem **prezentacji**, który wchodzi do snapshotu wyłącznie
    /// jako faza klipu i nie dotyka niczego w symulacji.
    pub anim_ms: u64,
}

/// Kandydat do snapshotu. `src` jest indeksem w tablicy źródłowej wypełniacza —
/// selektor nie wie i nie ma wiedzieć, co tam leży.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub struct Candidate {
    /// Kwadrat odległości od oka w mm². `u64`, bo (2 mln mm)² nie mieści się w `u32`.
    pub dist2: u64,
    /// Indeks encji — **rozstrzyga remisy** i to on czyni wybór deterministycznym.
    pub entity: u32,
    /// Indeks w tablicy źródłowej.
    pub src: u32,
}

/// Kwadrat odległości w mm², bez przepełnienia i bez floatów.
#[must_use]
pub fn dist2_mm(a: [i32; 3], b: [i32; 3]) -> u64 {
    let d = |i: usize| {
        // Różnica w `u64` przez wartość bezwzględną, nie przez `i64 * i64`: kwadrat
        // pełnego zakresu `i32` (4,29e9)² to 1,8447e19 — mieści się w `u64` i **nie**
        // mieści w `i64`, więc mnożenie ze znakiem przepełnia się na krańcach mapy.
        let v = (i64::from(a[i]) - i64::from(b[i])).unsigned_abs();
        v.saturating_mul(v)
    };
    d(0).saturating_add(d(1)).saturating_add(d(2))
}

/// Zostawia `k` kandydatów najbliższych oku, **uporządkowanych rosnąco po
/// `(dist2, entity)`**, i obcina resztę.
///
/// Determinizm bierze się stąd, że klucz jest **totalny**: dwie encje w tej samej
/// odległości rozstrzygają się indeksem, więc nie ma pary, o której kolejności
/// decydowałby algorytm sortowania. To jest wprost kryterium
/// `snapshot_selection_is_deterministic` z §7.1 — sto wywołań ma dać tę samą listę
/// w tej samej kolejności.
///
/// Uporządkowanie prefiksu nie jest kosmetyką: ścieżka klatki (§5.3) degraduje LOD
/// „najdalszych ponad limit", co wymaga tablicy posortowanej po odległości.
pub fn select_top_k(cands: &mut Vec<Candidate>, k: usize) {
    if cands.len() > k {
        // `select_nth_unstable` jest niedeterministyczny w **kolejności**, ale nie
        // w **zawartości** prefiksu: przy totalnym kluczu podział na „k najmniejszych"
        // jest jednoznaczny. Kolejność porządkuje sortowanie zaraz po nim.
        cands.select_nth_unstable(k);
        cands.truncate(k);
    }
    cands.sort_unstable();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kandydaci() -> Vec<Candidate> {
        // Cztery pary o identycznej odległości — same remisy, czyli najgorszy przypadek
        // dla determinizmu.
        (0..8u32)
            .map(|i| Candidate {
                dist2: u64::from(i / 2) * 100,
                entity: 1000 - i,
                src: i,
            })
            .collect()
    }

    #[test]
    fn wybor_jest_deterministyczny_mimo_remisow() {
        let wzorzec = {
            let mut c = kandydaci();
            select_top_k(&mut c, 4);
            c
        };
        for _ in 0..100 {
            let mut c = kandydaci();
            // Kolejność wejścia nie ma prawa zmienić wyniku — klucz jest totalny.
            c.reverse();
            select_top_k(&mut c, 4);
            assert_eq!(c, wzorzec, "ta sama para dała inną listę");
        }
        assert_eq!(wzorzec.len(), 4);
        // Remis rozstrzyga **niższy** indeks encji.
        assert_eq!(wzorzec[0].entity, 999);
        assert_eq!(wzorzec[1].entity, 1000);
    }

    #[test]
    fn ponizej_capu_nie_obcina_a_porzadkuje() {
        let mut c = kandydaci();
        select_top_k(&mut c, 64);
        assert_eq!(c.len(), 8);
        assert!(c.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn odleglosc_nie_przepelnia_sie_na_krancach_swiata() {
        let a = [i32::MIN, i32::MIN, i32::MIN];
        let b = [i32::MAX, i32::MAX, i32::MAX];
        assert!(dist2_mm(a, b) > 0, "odległość zawinęła się do zera");
        assert_eq!(dist2_mm(a, a), 0);
    }

    #[test]
    fn aabb_nasyca_sie_przy_krawedzi_mapy() {
        let a = Aabb::around([i32::MAX - 10, 0, 0], 1000, 100);
        assert_eq!(a.max[0], i32::MAX);
        assert!(a.contains([i32::MAX - 10, 0, 0]));
        assert!(!a.contains([0, 0, 0]));
    }
}
