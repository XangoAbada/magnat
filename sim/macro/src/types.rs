//! Typy pomocnicze modelu makro (M7f WP13, §5.10).
//!
//! Wszystko tutaj jest **nośnikiem liczby**, nie regułą: `SparseVec` trzyma stan
//! rzadki, `ClassGrain` mówi, ile klas mieści się w komórce, `MacroLedger` sumuje
//! pieniądz. Reguła — cena, płaca, przepustowość — mieszka w `sim/economy::kernel`
//! i ten crate jej nie powtarza (`K-50`, M10a §5.1).

use magnat_core::{GoodId, HashState, Money, StateHasher};

/// Rzadki wektor `klucz -> wartość`, posortowany po kluczu.
///
/// Dlaczego nie `BTreeMap`: 00 §3.2 zakazuje `HashMap`, a `BTreeMap` byłby tu
/// drzewem o kilkunastu wpisach z alokacją na węzeł. Wektor par posortowany po
/// kluczu daje tę samą deterministyczną kolejność iteracji przy jednym bloku
/// pamięci — a przy kilkunastu towarach na firmę wyszukiwanie binarne wygrywa
/// z drzewem także czasem.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SparseVec<K, V> {
    entries: Vec<(K, V)>,
}

impl<K: Copy + Ord, V: Copy> SparseVec<K, V> {
    #[must_use]
    pub const fn new() -> SparseVec<K, V> {
        SparseVec {
            entries: Vec::new(),
        }
    }

    #[must_use]
    pub fn get(&self, key: K) -> Option<V> {
        self.entries
            .binary_search_by_key(&key, |(k, _)| *k)
            .ok()
            .map(|i| self.entries[i].1)
    }

    pub fn set(&mut self, key: K, value: V) {
        match self.entries.binary_search_by_key(&key, |(k, _)| *k) {
            Ok(i) => self.entries[i].1 = value,
            Err(i) => self.entries.insert(i, (key, value)),
        }
    }

    /// Iteracja **zawsze po rosnącym kluczu** — to jest cała umowa tego typu.
    pub fn iter(&self) -> impl Iterator<Item = (K, V)> + '_ {
        self.entries.iter().map(|(k, v)| (*k, *v))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

impl SparseVec<GoodId, i64> {
    /// Dodaje do wpisu (zakłada go, jeśli go nie było). Saturacja zamiast
    /// przepełnienia: masa w gramach przy metropolii mieści się w `i64` z zapasem,
    /// ale panika w kroku makro byłaby gorsza od przycięcia.
    pub fn add(&mut self, key: GoodId, delta: i64) {
        let teraz = self.get(key).unwrap_or(0);
        self.set(key, teraz.saturating_add(delta));
    }
}

impl<K: Copy + Ord + Into<u64>, V: Copy + Into<i64>> HashState for SparseVec<K, V> {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.entries.len() as u32);
        for (k, v) in &self.entries {
            h.write_u64((*k).into());
            h.write_i64((*v).into());
        }
    }
}

/// Ilość towaru w jednostce **natywnej** danego `GoodId` z `data/goods/`.
///
/// Bez konwersji jednostek na granicy LOD (ustalenie M6/M10, `M6a` §5.2):
/// konwersja jest zaokrągleniem, a zaokrąglenie łamie zachowanie masy.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MacroStock(pub SparseVec<GoodId, i64>);

impl MacroStock {
    #[must_use]
    pub const fn new() -> MacroStock {
        MacroStock(SparseVec::new())
    }

    #[must_use]
    pub fn get(&self, g: GoodId) -> i64 {
        self.0.get(g).unwrap_or(0)
    }

    pub fn add(&mut self, g: GoodId, delta: i64) {
        self.0.add(g, delta);
    }

    pub fn set(&mut self, g: GoodId, v: i64) {
        self.0.set(g, v);
    }

    pub fn iter(&self) -> impl Iterator<Item = (GoodId, i64)> + '_ {
        self.0.iter()
    }

    /// Suma wszystkich pozycji — wejście testu zachowania masy.
    #[must_use]
    pub fn total(&self) -> i64 {
        self.0
            .iter()
            .map(|(_, v)| v)
            .fold(0i64, i64::saturating_add)
    }
}

impl HashState for MacroStock {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.0.len() as u32);
        for (g, v) in self.0.iter() {
            h.write_u16(g.0);
            h.write_i64(v);
        }
    }
}

/// Klasa społeczna komórki **po scaleniu ziarnem** (`cell_grain`).
///
/// Nie jest tym samym co `agents::SocialClass`: ta ostatnia ma zawsze sześć
/// wariantów, a `ClassId` tyle, ile ziarno zostawiło. Odwzorowanie robi
/// [`ClassGrain::class_of`] i jest jedynym miejscem, w którym klasy się łączą.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ClassId(pub u8);

/// Ile klas mieści się w komórce przy tej wielkości miasta (M10a §5.6).
///
/// Klasy łączy się **parami po sąsiadujących przedziałach statusu**, nigdy losowo —
/// inaczej „komórka" przestałaby cokolwiek znaczyć.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassGrain {
    Classes6,
    Classes3,
    Classes2,
}

/// Dolna granica liczebności komórki. Poniżej niej odchylenie agregatu przestaje
/// być 3–12% i robi się nieprzewidywalne (00 §4: pasmo 0,5% obowiązuje od n ≥ 500).
pub const MIN_CELL_POP: u32 = 500;

impl ClassGrain {
    #[must_use]
    pub const fn classes(self) -> u8 {
        match self {
            ClassGrain::Classes6 => 6,
            ClassGrain::Classes3 => 3,
            ClassGrain::Classes2 => 2,
        }
    }

    /// Odwzorowanie sześciu klas społecznych na klasę komórki.
    ///
    /// `Classes3` = (niższa + robotnicza), (niższa średnia + wyższa średnia),
    /// (wyższa + elita). `Classes2` = dolna połowa i górna połowa.
    #[must_use]
    pub const fn class_of(self, social: u8) -> ClassId {
        match self {
            ClassGrain::Classes6 => ClassId(social),
            ClassGrain::Classes3 => ClassId(social / 2),
            ClassGrain::Classes2 => ClassId(social / 3),
        }
    }
}

/// Ziarno dobrane tak, żeby na komórkę przypadało co najmniej [`MIN_CELL_POP`] osób.
///
/// Nigdzie w tym crate'cie nie ma stałej liczby komórek — liczbę zawsze podaje
/// `state.cells.len()` (zobowiązanie `D11b` fazy M12).
#[must_use]
pub fn cell_grain(pop: u32, districts: u16) -> ClassGrain {
    let d = u32::from(districts.max(1));
    for grain in [
        ClassGrain::Classes6,
        ClassGrain::Classes3,
        ClassGrain::Classes2,
    ] {
        if pop / (d * u32::from(grain.classes())) >= MIN_CELL_POP {
            return grain;
        }
    }
    ClassGrain::Classes2
}

/// Branża firmy = kategoria typu zakładu (`SiteTypeCategory::as_index`).
///
/// Osobny newtype, a nie sam enum, bo makro porównuje branże liczbowo i nie
/// rozgałęzia się po nich ani razu — rozgałęzienie byłoby regułą ekonomiczną,
/// a tych tu nie ma.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct BranchId(pub u16);

/// Poziom techniczny zakładu — **poziom, nie zbiór węzłów** (M7f §5.10).
///
/// Nie zna `TechId` i nigdy nie będzie znał w M7: drzewa technologii nie ma
/// (§7.7 PRD to M10). M7 tę liczbę **czyta i nie zmienia**.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct TechLevel(pub u8);

/// Tożsamość mieszkańca w komórce — 8 bajtów (M10a §5.8).
///
/// Cech stałych się nie przechowuje: wyprowadza je `personality(world_seed,
/// birth_index)`. Dzięki temu przejście makro → mezo odtwarza tego samego
/// człowieka, a nie statystycznie podobnego (00 §4 pkt 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CitizenSeed {
    pub birth_index: u32,
    pub cell: u16,
}

/// Macierz czasów dojazdu dzielnica × dzielnica, w minutach.
///
/// `ponytail:` w M7 jest **płaska** — każda para dzielnic kosztuje tyle samo.
/// Sufit nazwany: przy płaskiej macierzy faza 2 nie odróżnia dzielnicy dobrze
/// skomunikowanej od odciętej, więc podaż roli jest w mieście jednorodna.
/// Ścieżka wyjścia jest w kontrakcie i nie wymaga zmiany kształtu: „M10 trzyma,
/// M4 wypełnia" — snapshot `TravelTimeMatrix` z chwili `lift()` (M10 §6).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommuteMatrix {
    districts: u16,
    minutes: Vec<u16>,
}

impl CommuteMatrix {
    /// Macierz, w której każdy dojazd kosztuje `minutes` minut.
    #[must_use]
    pub fn flat(districts: u16, minutes: u16) -> CommuteMatrix {
        let n = usize::from(districts);
        CommuteMatrix {
            districts,
            minutes: vec![minutes; n * n],
        }
    }

    #[must_use]
    pub fn districts(&self) -> u16 {
        self.districts
    }

    /// Czas dojazdu z `from` do `to`; poza zakresem zwraca `u16::MAX`,
    /// czyli „nie dojedzie" — a nie zero, które znaczyłoby „za miedzą".
    #[must_use]
    pub fn minutes(&self, from: u16, to: u16) -> u16 {
        let n = usize::from(self.districts);
        if usize::from(from) >= n || usize::from(to) >= n {
            return u16::MAX;
        }
        self.minutes[usize::from(from) * n + usize::from(to)]
    }
}

impl HashState for CommuteMatrix {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u16(self.districts);
        for m in &self.minutes {
            h.write_u16(*m);
        }
    }
}

/// Epoka świata w chwili zdjęcia. W M7 nieruchoma — epoki wprowadza M10.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EpochState {
    pub key: u16,
}

/// Konta księgi makro.
///
/// To **nie** jest `economy::Ledger` z M5c, i to jest świadoma poprawka kontraktu:
/// tamten jest księgą **jednego zakładu** (`site`, `firm`, domknięcia miesięczne),
/// a `MacroState` potrzebuje sumy całego miasta. Wspólna zostaje rzecz istotna —
/// każde księgowanie przechodzi przez `kernel::ledger_post`, więc suma Wn i Ma
/// domyka się co do grosza tą samą funkcją co w mezo.
pub const MACRO_ACCOUNTS: usize = 3;

/// Indeksy kont w [`MacroLedger`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroAccount {
    /// Gotówka gospodarstw domowych.
    Households = 0,
    /// Gotówka firm.
    Firms = 1,
    /// Reszta świata: import, eksport, podatki — druga strona zapisu.
    ///
    /// Długu w tej księdze **nie ma i nie będzie**: dług nie jest pieniądzem, tylko
    /// zobowiązaniem, i siedzi w `MacroCell.debt` oraz `MacroFirm.debt`. Konto długu
    /// obok trzech powyższych psułoby jedyny niezmiennik, który ta struktura niesie —
    /// suma kont przestałaby być sumą pieniądza w mieście.
    RestOfWorld = 2,
}

/// Suma pieniądza w modelu makro, w rozbiciu na trzy strony.
///
/// Niezmiennik, który ten typ niesie i którego pilnuje test: **suma trzech kont
/// nie zmienia się przez cały krok makro**. Makro nie tworzy ani nie niszczy grosza
/// (00 §4, tolerancja 0) — wolno mu wyłącznie przesuwać.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MacroLedger {
    pub accounts: [Money; MACRO_ACCOUNTS],
}

impl MacroLedger {
    /// Przesuwa kwotę między dwoma kontami. Jedyne wejście mutujące — dzięki temu
    /// „ktoś dopisał pieniądz" jest w tym crate'cie niewyrażalne.
    pub fn transfer(&mut self, from: MacroAccount, to: MacroAccount, amount: Money) {
        let i = from as usize;
        let j = to as usize;
        self.accounts[i] = Money(self.accounts[i].get().saturating_sub(amount.get()));
        self.accounts[j] = Money(self.accounts[j].get().saturating_add(amount.get()));
    }

    #[must_use]
    pub fn get(&self, a: MacroAccount) -> Money {
        self.accounts[a as usize]
    }

    pub fn set(&mut self, a: MacroAccount, v: Money) {
        self.accounts[a as usize] = v;
    }

    /// Suma wszystkich kont — liczba, która nie ma prawa drgnąć w kroku.
    #[must_use]
    pub fn total(&self) -> Money {
        Money(
            self.accounts
                .iter()
                .map(|m| m.get())
                .fold(0i64, i64::saturating_add),
        )
    }
}

impl HashState for MacroLedger {
    fn hash_state(&self, h: &mut StateHasher) {
        for a in &self.accounts {
            h.write_i64(a.get());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_vec_iteruje_po_rosnacym_kluczu() {
        let mut v: SparseVec<GoodId, i64> = SparseVec::new();
        v.set(GoodId(7), 70);
        v.set(GoodId(2), 20);
        v.set(GoodId(5), 50);
        v.add(GoodId(2), 5);
        let k: Vec<_> = v.iter().collect();
        assert_eq!(k, vec![(GoodId(2), 25), (GoodId(5), 50), (GoodId(7), 70)]);
    }

    #[test]
    fn ziarno_schodzi_do_dwoch_klas_przy_malym_miescie() {
        // 40 dzielnic × 6 klas × 500 = 120 tys. — metropolia zostaje przy sześciu.
        assert_eq!(cell_grain(150_000, 40), ClassGrain::Classes6);
        // 20 tys. przy 8 dzielnicach: 6 klas dałoby 416 osób na komórkę, 3 dają 833.
        assert_eq!(cell_grain(20_000, 8), ClassGrain::Classes3);
        assert_eq!(cell_grain(3_000, 8), ClassGrain::Classes2);
    }

    #[test]
    fn scalanie_klas_laczy_sasiadow() {
        let g = ClassGrain::Classes3;
        assert_eq!(g.class_of(0), g.class_of(1));
        assert_eq!(g.class_of(2), g.class_of(3));
        assert_eq!(g.class_of(4), g.class_of(5));
        assert_ne!(g.class_of(1), g.class_of(2));
    }

    #[test]
    fn ksiega_makro_nie_tworzy_pieniadza() {
        let mut l = MacroLedger::default();
        l.set(MacroAccount::Households, Money(1_000));
        l.set(MacroAccount::Firms, Money(500));
        let przed = l.total();
        l.transfer(MacroAccount::Households, MacroAccount::Firms, Money(250));
        assert_eq!(l.total(), przed);
        assert_eq!(l.get(MacroAccount::Households), Money(750));
    }
}
