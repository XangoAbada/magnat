//! `Table<T>` — wirtualizowana tabela na sto tysięcy wierszy (`M9b` §5.8, WP6).
//!
//! # Trzy rzeczy, które trzymają budżet z §7
//!
//! 1. **Nic nie jest kopiowane.** Tabela trzyma [`RowSource`] — widok na migawkę —
//!    i pyta go o komórkę dopiero przy rysowaniu wiersza, który naprawdę widać.
//! 2. **Widać około sześćdziesięciu wierszy.** Wysokość wiersza jest stała w obrębie
//!    tabeli, więc matematyka przewijania jest O(1) i `egui` rysuje wyłącznie okno.
//!    Przewijanie nie sortuje i nie filtruje ponownie — nie robi **niczego** O(n).
//! 3. **Sortowanie i filtrowanie dzieje się poza klatką.** Wynikiem jest permutacja
//!    `Vec<u32>`; do czasu jej gotowości widać poprzedni porządek, a nie kręciołek.
//!
//! `ponytail:` zadanie idzie na `std::thread`, nie na `engine/jobs` (`DE-10`) —
//! `JobPool` wystawia wyłącznie funkcje blokujące, a czekanie na wynik w wątku pętli
//! klatki jest dokładnie tym, czego budżet „poza klatką" zabrania. Sufit nazwany:
//! `JobPool::spawn` zwracające uchwyt, właściciel M0.
//!
//! `ponytail:` filtr daje od razu permutację, a nie bitset — jedno przejście zamiast
//! dwóch. Sufit: filtrowanie **przyrostowe** (dołóż warunek do już policzonego wyniku)
//! wymagałoby bitsetu i wejdzie razem z panelem, który tego zażąda (`M9e`).

use std::sync::Arc;

use crate::theme::{ColorToken, TextRole, Theme};

/// Indeks kolumny w obrębie jednej tabeli.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ColumnId(pub u16);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortDir {
    Asc,
    Desc,
}

impl SortDir {
    #[must_use]
    pub const fn flip(self) -> SortDir {
        match self {
            SortDir::Asc => SortDir::Desc,
            SortDir::Desc => SortDir::Asc,
        }
    }
}

/// Wyrównanie komórki. Liczbowe do prawej, tekstowe do lewej (`ui-design.md` §4).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Align {
    Left,
    Right,
}

pub struct Column {
    pub id: ColumnId,
    /// Nagłówek w języku gracza — wołający bierze go z katalogu.
    pub title: String,
    pub width: f32,
    pub align: Align,
    /// Czy kliknięcie w nagłówek sortuje.
    pub sortable: bool,
}

/// Źródło wierszy: widok na migawkę, bez kopiowania danych.
///
/// `Send + Sync + 'static`, bo sortowanie i filtrowanie dzieje się na innym wątku.
pub trait RowSource: Send + Sync + 'static {
    fn len(&self) -> u32;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Klucz sortowania — sortujemy tablicę `u64`, a nie wiersze, bo sto tysięcy
    /// porównań przez `dyn` kosztowałoby więcej niż samo sortowanie.
    fn sort_key(&self, col: ColumnId, row: u32) -> u64;

    /// Zawartość komórki jako tekst. Wołana **wyłącznie** dla wierszy w oknie.
    fn cell(&self, col: ColumnId, row: u32) -> String;
}

/// Filtr wiersza. Predykat, a nie AST (`DE-6`): tabela potrzebuje odpowiedzi
/// „ten wiersz zostaje". Tam, gdzie warunek pisze gracz, pisze go w języku reguł,
/// a ewaluator `sim/policy` zwraca właśnie taki predykat.
pub type Filter = Arc<dyn Fn(u32) -> bool + Send + Sync>;

/// Permutacja wierszy po sortowaniu i filtrowaniu — to jest cały wynik pracy
/// wykonanej poza klatką.
pub fn compute_order(
    source: &dyn RowSource,
    sort: Option<(ColumnId, SortDir)>,
    filter: Option<&Filter>,
) -> Vec<u32> {
    let n = source.len();
    let mut out: Vec<u32> = match filter {
        Some(f) => (0..n).filter(|i| f(*i)).collect(),
        None => (0..n).collect(),
    };
    if let Some((col, dir)) = sort {
        // Klucze wyciągnięte raz, do ciągłej tablicy: `sort_unstable_by_key`
        // wołałby `sort_key` przez `dyn` przy każdym porównaniu.
        let mut pary: Vec<(u64, u32)> =
            out.iter().map(|i| (source.sort_key(col, *i), *i)).collect();
        // Remis rozstrzyga indeks wiersza — inaczej kolejność równych wartości
        // zależałaby od algorytmu, a gracz widziałby, jak przeskakują.
        pary.sort_unstable();
        if dir == SortDir::Desc {
            pary.reverse();
        }
        for (dokad, (_, i)) in out.iter_mut().zip(pary) {
            *dokad = i;
        }
    }
    out
}

/// Stan tabeli między klatkami.
pub struct Table {
    source: Arc<dyn RowSource>,
    columns: Vec<Column>,
    order: Vec<u32>,
    sort: Option<(ColumnId, SortDir)>,
    filter: Option<Filter>,
    zadanie: Option<std::thread::JoinHandle<Vec<u32>>>,
    row_h: f32,
    selected: Option<u32>,
}

impl Table {
    /// Nowa tabela. Porządek startowy to kolejność źródła — pierwsza klatka nie płaci
    /// za sortowanie, którego nikt nie zamówił.
    #[must_use]
    pub fn new(source: Arc<dyn RowSource>, columns: Vec<Column>, theme: &Theme) -> Table {
        let order = (0..source.len()).collect();
        Table {
            source,
            columns,
            order,
            sort: None,
            filter: None,
            zadanie: None,
            // Wysokość wiersza 22 px przy skali 1,0 (`ui-design.md` §3.3) —
            // wielokrotność siatki nie wychodzi, i to jest wyjątek świadomy:
            // 20 px jest ciasne dla 13-piksela, 24 px daje panel o ćwierć dłuższy.
            row_h: theme.gap(5) + 2.0,
            selected: None,
        }
    }

    #[must_use]
    pub fn sort(&self) -> Option<(ColumnId, SortDir)> {
        self.sort
    }

    #[must_use]
    pub fn selected(&self) -> Option<u32> {
        self.selected
    }

    /// Ile wierszy przechodzi przez filtr i jest widocznych w tej chwili.
    #[must_use]
    pub fn visible_len(&self) -> usize {
        self.order.len()
    }

    /// Czy trwa liczenie nowego porządku. Tabela pokazuje wtedy poprzedni.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        self.zadanie.is_some()
    }

    pub fn set_sort(&mut self, sort: Option<(ColumnId, SortDir)>) {
        self.sort = sort;
        self.przelicz();
    }

    pub fn set_filter(&mut self, filter: Option<Filter>) {
        self.filter = filter;
        self.przelicz();
    }

    /// Zgłasza, że źródło się zmieniło — porządek trzeba policzyć od nowa.
    pub fn source_changed(&mut self) {
        self.przelicz();
    }

    fn przelicz(&mut self) {
        // Zadanie w locie zostaje: jego wynik i tak zostanie odrzucony przy odbiorze,
        // bo `poll` porównuje, czy zamówienie jest wciąż aktualne. Zabijanie wątku
        // w połowie sortowania kosztowałoby więcej niż pozwolenie mu dobiec.
        let source = self.source.clone();
        let sort = self.sort;
        let filter = self.filter.clone();
        self.zadanie = Some(std::thread::spawn(move || {
            compute_order(source.as_ref(), sort, filter.as_ref())
        }));
    }

    /// Odbiera gotowy porządek, jeśli jest. Woła się raz na klatkę, **nie blokuje**.
    pub fn poll(&mut self) -> bool {
        let gotowe = self.zadanie.as_ref().is_some_and(|z| z.is_finished());
        if !gotowe {
            return false;
        }
        let Some(z) = self.zadanie.take() else {
            return false;
        };
        match z.join() {
            Ok(v) => {
                self.order = v;
                if self.selected.is_some_and(|s| !self.order.contains(&s)) {
                    self.selected = None;
                }
                true
            }
            // Panika w wątku sortującym nie ma prawa zabrać ze sobą gry: porządek
            // zostaje poprzedni, a wiersz o tym mówi w konsoli.
            Err(_) => {
                eprintln!("sortowanie tabeli zakończyło się paniką — porządek bez zmian");
                false
            }
        }
    }

    /// Rysuje tabelę. Zwraca indeks wiersza, w który gracz właśnie kliknął.
    pub fn show(&mut self, ui: &mut egui::Ui, theme: &Theme) -> Option<u32> {
        self.poll();
        let mut klikniety = None;
        let mut nowy_sort = None;

        // Nagłówek przyklejony: rysuje się poza obszarem przewijania (`ui-design.md` §4).
        ui.horizontal(|ui| {
            for c in &self.columns {
                let strzalka = match self.sort {
                    Some((id, SortDir::Asc)) if id == c.id => " ▲",
                    Some((id, SortDir::Desc)) if id == c.id => " ▼",
                    _ => "",
                };
                let tekst = egui::RichText::new(format!("{}{strzalka}", c.title))
                    .font(theme.font(TextRole::Strong))
                    .color(theme.color(ColorToken::TextSecondary));
                let odp = ui.add_sized(
                    egui::vec2(c.width, self.row_h),
                    egui::Label::new(tekst).sense(if c.sortable {
                        egui::Sense::click()
                    } else {
                        egui::Sense::hover()
                    }),
                );
                if c.sortable && odp.clicked() {
                    nowy_sort = Some(match self.sort {
                        Some((id, d)) if id == c.id => (c.id, d.flip()),
                        _ => (c.id, SortDir::Asc),
                    });
                }
            }
        });
        ui.separator();

        let kolumny: Vec<(ColumnId, f32, Align)> = self
            .columns
            .iter()
            .map(|c| (c.id, c.width, c.align))
            .collect();
        let zrodlo = self.source.clone();
        let porzadek = &self.order;
        let wybrany = self.selected;
        let wys = self.row_h;
        let mono = theme.mono(TextRole::Body);
        let font = theme.font(TextRole::Body);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, wys, porzadek.len(), |ui, widoczne| {
                for i in widoczne {
                    let wiersz = porzadek[i];
                    ui.horizontal(|ui| {
                        for (id, szer, align) in &kolumny {
                            let tekst = zrodlo.cell(*id, wiersz);
                            let rt = egui::RichText::new(tekst)
                                .font(if *align == Align::Right {
                                    mono.clone()
                                } else {
                                    font.clone()
                                })
                                .color(if wybrany == Some(wiersz) {
                                    theme.color(ColorToken::AccentHi)
                                } else {
                                    theme.color(ColorToken::TextPrimary)
                                });
                            let etykieta = egui::Label::new(rt)
                                .halign(match align {
                                    Align::Left => egui::Align::LEFT,
                                    Align::Right => egui::Align::RIGHT,
                                })
                                .sense(egui::Sense::click());
                            if ui.add_sized(egui::vec2(*szer, wys), etykieta).clicked() {
                                klikniety = Some(wiersz);
                            }
                        }
                    });
                }
            });

        if let Some(s) = nowy_sort {
            self.set_sort(Some(s));
        }
        if let Some(w) = klikniety {
            self.selected = Some(w);
        }
        klikniety
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Liczby(u32);

    impl RowSource for Liczby {
        fn len(&self) -> u32 {
            self.0
        }
        fn sort_key(&self, _c: ColumnId, row: u32) -> u64 {
            // Odwrotna kolejność, żeby sortowanie miało co przestawić.
            u64::from(self.0 - row)
        }
        fn cell(&self, _c: ColumnId, row: u32) -> String {
            row.to_string()
        }
    }

    fn kolumny() -> Vec<Column> {
        vec![Column {
            id: ColumnId(0),
            title: "n".into(),
            width: 80.0,
            align: Align::Right,
            sortable: true,
        }]
    }

    #[test]
    fn porzadek_sortuje_i_filtruje_w_jednym_przejsciu() {
        let z = Liczby(1000);
        let rosnaco = compute_order(&z, Some((ColumnId(0), SortDir::Asc)), None);
        assert_eq!(rosnaco.len(), 1000);
        assert_eq!(rosnaco[0], 999, "najmniejszy klucz ma ostatni wiersz");
        assert_eq!(rosnaco[999], 0);

        let malejaco = compute_order(&z, Some((ColumnId(0), SortDir::Desc)), None);
        assert_eq!(malejaco[0], 0);

        let parzyste: Filter = Arc::new(|i: u32| i.is_multiple_of(2));
        let f = compute_order(&z, None, Some(&parzyste));
        assert_eq!(f.len(), 500);
        assert!(f.iter().all(|i| i % 2 == 0));

        let obie = compute_order(&z, Some((ColumnId(0), SortDir::Asc)), Some(&parzyste));
        assert_eq!(obie.len(), 500);
        assert!(obie
            .windows(2)
            .all(|w| z.sort_key(ColumnId(0), w[0]) <= z.sort_key(ColumnId(0), w[1])));
    }

    #[test]
    fn tabela_stu_tysiecy_wierszy_rysuje_okno_a_nie_wszystko() {
        let theme = Theme::load().expect("data/ui/theme.ron");
        let zrodlo = Arc::new(Liczby(100_000));
        let mut t = Table::new(zrodlo, kolumny(), &theme);
        assert_eq!(t.visible_len(), 100_000);

        let ctx = egui::Context::default();
        let (teksty, _) =
            crate::testing::draw_in(&ctx, crate::testing::input(crate::testing::EKRAN), |ui| {
                t.show(ui, &theme);
            });
        // Nagłówek plus okno widocznych wierszy — nie sto tysięcy etykiet.
        assert!(
            teksty.len() < 200,
            "narysowano {} tekstów, a miało być okno",
            teksty.len()
        );
        assert!(teksty.iter().any(|s| s.starts_with('n')), "brak nagłówka");
    }

    #[test]
    fn sortowanie_liczy_sie_poza_klatka_i_nie_gubi_wyniku() {
        let theme = Theme::load().expect("data/ui/theme.ron");
        let mut t = Table::new(Arc::new(Liczby(10_000)), kolumny(), &theme);
        t.set_sort(Some((ColumnId(0), SortDir::Asc)));
        assert!(t.is_pending(), "zadanie miało pójść w tło");
        // Czekamy na wątek tak, jak czeka pętla klatki: pytając, nie blokując.
        // Sufit czasowy, a nie licznik obrotów — licznik mierzyłby obciążenie maszyny.
        let start = std::time::Instant::now();
        while !t.poll() && start.elapsed() < std::time::Duration::from_secs(10) {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(!t.is_pending());
        assert_eq!(t.visible_len(), 10_000);
        assert_eq!(t.order[0], 9_999);
    }
}
