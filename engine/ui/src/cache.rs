//! Przebudowa tylko tego, co się zmieniło (`M9b` §5.8, korekta `DE-2`).
//!
//! # Co tu jest liczone, a co nie
//!
//! Kosztem panelu **nie jest** rysowanie kilkuset widgetów — kosztem jest złożenie
//! modelu: `Market::shop_panel` bierze zamek rynku, `CitizenPanel::model` czyta świat,
//! a karta składa kilkaset napisów z katalogu. Dlatego znacznik „brudny" siedzi przy
//! modelu, a nie przy drzewie widgetów, i dlatego kryterium WP3 („brak zmian danych →
//! 0 alokacji i 0 ms przebudowy") mierzy się tutaj.
//!
//! # Jak to działa
//!
//! [`Versions`] to tablica liczników: symulacja podbija licznik źródła, które właśnie
//! ruszyła. [`Cached<T>`] pamięta, przy jakich licznikach zbudowano wartość — i dopóki
//! ani jeden z **jego** liczników nie drgnął, `get` zwraca to, co ma, bez ani jednej
//! alokacji.
//!
//! `ponytail:` to jest **cała** migawka dla paneli i to jest zamierzone (`DE-3`).
//! Podwójne buforowanie nie kupuje dziś nic, bo symulacja i render chodzą w jednym
//! wątku pętli klatki, więc model zbudowany po `Session::step` jest z definicji spójny
//! na granicy ticku. Sufit: gdy symulacja wyjdzie na własny wątek (M12, tryb 50×),
//! `Cached` staje się buforem tylnym i dochodzi zamiana wskaźników.

/// Źródło danych, którego zmianę ktoś obserwuje.
///
/// Rozszerza się je tak samo jak `PanelId`: faza dokładająca panel dokłada źródło,
/// **jeśli** ma je czym podbijać. Wariant bez pisarza to znacznik, który nigdy
/// nie drgnie — czyli cache, który nigdy się nie odświeży.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum DataSource {
    /// Minuta gry ruszyła.
    Clock,
    /// Zmieniło się zaznaczenie (co gracz kliknął).
    Selection,
    /// Stan rynku: półki, ceny, utracone sprzedaże.
    Market,
    /// Stan świata ECS: mieszkańcy, gospodarstwa, zatrudnienie.
    World,
    /// Język albo skala interfejsu — unieważnia **każdy** model niosący tekst.
    Locale,
    /// Lista slotów zapisu.
    Slots,
    /// Historia metryk do wykresów.
    Series,
}

pub const DATA_SOURCE_COUNT: usize = 7;

impl DataSource {
    pub const ALL: [DataSource; DATA_SOURCE_COUNT] = [
        DataSource::Clock,
        DataSource::Selection,
        DataSource::Market,
        DataSource::World,
        DataSource::Locale,
        DataSource::Slots,
        DataSource::Series,
    ];

    #[must_use]
    pub const fn as_index(self) -> usize {
        self as usize
    }
}

/// Ile źródeł wolno wpisać jednemu cache'owi.
///
/// `ponytail:` stała zamiast wektora — dzięki temu [`Cached`] nie alokuje ani przy
/// tworzeniu, ani przy sprawdzaniu. Sufit: siedem źródeł istnieje, cztery wystarczają
/// każdemu modelowi w tej podfazie, a przekroczenie jest paniką przy tworzeniu, czyli
/// przy starcie, a nie w locie.
pub const MAX_DEPS: usize = 4;

/// Liczniki wersji źródeł. Jeden zasób na sesję interfejsu.
#[derive(Clone, Copy, Debug, Default)]
pub struct Versions([u64; DATA_SOURCE_COUNT]);

impl Versions {
    #[must_use]
    pub const fn new() -> Versions {
        Versions([0; DATA_SOURCE_COUNT])
    }

    /// Zgłasza, że źródło się zmieniło.
    pub fn bump(&mut self, s: DataSource) {
        self.0[s.as_index()] = self.0[s.as_index()].wrapping_add(1);
    }

    #[must_use]
    pub const fn get(&self, s: DataSource) -> u64 {
        self.0[s.as_index()]
    }
}

/// Model przebudowywany wyłącznie wtedy, gdy zmieniło się któreś z jego źródeł.
pub struct Cached<T> {
    deps: &'static [DataSource],
    stamp: [u64; MAX_DEPS],
    value: Option<T>,
    rebuilds: u32,
}

impl<T> Cached<T> {
    /// # Panics
    /// Gdy `deps` ma więcej niż [`MAX_DEPS`] pozycji — to jest błąd w kodzie panelu,
    /// wykrywany przy jego powstaniu.
    #[must_use]
    pub fn new(deps: &'static [DataSource]) -> Cached<T> {
        assert!(
            deps.len() <= MAX_DEPS,
            "cache deklaruje {} źródeł, sufit to {MAX_DEPS}",
            deps.len()
        );
        Cached {
            deps,
            // `u64::MAX` zamiast zera: świeży cache ma być brudny także wtedy, gdy
            // nikt jeszcze niczego nie podbił, czyli w pierwszej klatce sesji.
            stamp: [u64::MAX; MAX_DEPS],
            value: None,
            rebuilds: 0,
        }
    }

    /// Czy `build` zostanie wywołane przy najbliższym [`Cached::get`].
    #[must_use]
    pub fn is_dirty(&self, v: &Versions) -> bool {
        if self.value.is_none() {
            return true;
        }
        self.deps
            .iter()
            .enumerate()
            .any(|(i, s)| self.stamp[i] != v.get(*s))
    }

    /// Wartość modelu; `build` woła się **tylko** wtedy, gdy któreś źródło drgnęło.
    pub fn get(&mut self, v: &Versions, build: impl FnOnce() -> T) -> &T {
        if self.is_dirty(v) {
            for (i, s) in self.deps.iter().enumerate() {
                self.stamp[i] = v.get(*s);
            }
            self.value = Some(build());
            self.rebuilds += 1;
        }
        self.value.as_ref().expect("dopiero co zbudowane")
    }

    /// Wartość bez budowania — dla wołających, którzy wolą pokazać stare dane
    /// niż zapłacić za nowe w tej klatce.
    #[must_use]
    pub fn peek(&self) -> Option<&T> {
        self.value.as_ref()
    }

    /// Ile razy model faktycznie powstał. To jest liczba, na której stoi kryterium WP3.
    #[must_use]
    pub const fn rebuilds(&self) -> u32 {
        self.rebuilds
    }

    /// Wyrzuca wartość — po zamknięciu panelu, żeby model nie trzymał pamięci.
    pub fn clear(&mut self) {
        self.value = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bez_zmiany_zrodla_model_nie_powstaje_drugi_raz() {
        static DEPS: [DataSource; 2] = [DataSource::Market, DataSource::Locale];
        let mut v = Versions::new();
        let mut c: Cached<String> = Cached::new(&DEPS);

        assert_eq!(*c.get(&v, || "pierwszy".to_string()), "pierwszy");
        assert_eq!(c.rebuilds(), 1);

        // Dziesięć klatek bez zmiany danych — ani jednej przebudowy.
        for _ in 0..10 {
            let _ = c.get(&v, || panic!("model przebudowany bez zmiany źródła"));
        }
        assert_eq!(c.rebuilds(), 1);

        // Ruch w źródle, którego ten cache **nie** obserwuje, też go nie budzi.
        v.bump(DataSource::Clock);
        v.bump(DataSource::World);
        let _ = c.get(&v, || panic!("obce źródło zbudziło cache"));
        assert_eq!(c.rebuilds(), 1);

        // A własne — budzi.
        v.bump(DataSource::Market);
        assert!(c.is_dirty(&v));
        assert_eq!(*c.get(&v, || "drugi".to_string()), "drugi");
        assert_eq!(c.rebuilds(), 2);
    }

    #[test]
    fn zmiana_jezyka_uniewaznia_kazdy_model_z_tekstem() {
        static DEPS: [DataSource; 1] = [DataSource::Locale];
        let mut v = Versions::new();
        let mut c: Cached<u32> = Cached::new(&DEPS);
        assert_eq!(*c.get(&v, || 1), 1);
        v.bump(DataSource::Locale);
        assert_eq!(*c.get(&v, || 2), 2);
        assert_eq!(c.rebuilds(), 2);
    }

    #[test]
    fn cache_bez_zrodel_buduje_sie_raz_i_juz_nigdy() {
        static DEPS: [DataSource; 0] = [];
        let mut v = Versions::new();
        let mut c: Cached<u32> = Cached::new(&DEPS);
        assert_eq!(*c.get(&v, || 7), 7);
        for s in DataSource::ALL {
            v.bump(s);
        }
        assert_eq!(*c.get(&v, || panic!("stała przeliczona drugi raz")), 7);
        c.clear();
        assert!(c.peek().is_none());
        assert!(c.is_dirty(&v));
    }
}
