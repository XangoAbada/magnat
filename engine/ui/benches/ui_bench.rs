//! Benchmarki WP6 — budżety klatki z §7 dokumentu fazy M9.
//!
//! UI dostaje **4,0 ms CPU** na klatkę przy otwartych panelach. Z tego budżetu
//! mierzymy tu trzy widgety, które WP6 wnosi, i tylko je: `Table<T>`, `Series`
//! i miniaturę mapy cieplnej. Graf łańcucha dostaw i Gantt mają budżety w tej samej
//! tabeli, ale nie mają jeszcze konsumenta — zamykają się razem ze swoimi panelami
//! w `M9e` (`DE-5`).
//!
//! | Ścieżka | Budżet §7 |
//! |---|---|
//! | `Table` 100 tys. — budowa i rysowanie widoku | ≤ 0,5 ms |
//! | `Table` 100 tys. — przewijanie | ≤ 0,2 ms |
//! | `Table` 100 tys. — zmiana sortowania | ≤ 12 ms **poza klatką** |
//! | `Table` 100 tys. — zmiana filtra | ≤ 8 ms poza klatką |
//! | Wykres 10 lat × 8 serii | ≤ 0,3 ms CPU |
//! | Miniatura mapy cieplnej 256 × 256 | ≤ 0,3 ms |
//!
//! Wszystko na uprzęży bezgłowej (`egui` bez karty graficznej), zgodnie z zasadą
//! headless-first z 00 §6 — czas GPU mierzy przelot `magnat --bench`.

use std::hint::black_box;
use std::sync::Arc;

use criterion::{criterion_group, criterion_main, Criterion};
use magnat_ui::table::{compute_order, Filter};
use magnat_ui::{
    testing, Align, Column, ColumnId, RowSource, Series, SeriesKey, SortDir, Table, Theme,
};

const WIERSZE: u32 = 100_000;
const LAT: u32 = 10;

/// Źródło wierszy udające migawkę: klucz sortowania i komórka liczone z indeksu,
/// bez własnej tablicy — mierzymy tabelę, a nie dostęp do pamięci danych.
struct Mieszkancy(u32);

impl RowSource for Mieszkancy {
    fn len(&self) -> u32 {
        self.0
    }
    fn sort_key(&self, c: ColumnId, row: u32) -> u64 {
        u64::from(row.rotate_left(u32::from(c.0) * 7 + 11))
    }
    fn cell(&self, _c: ColumnId, row: u32) -> String {
        row.to_string()
    }
}

fn kolumny() -> Vec<Column> {
    (0..4)
        .map(|i| Column {
            id: ColumnId(i),
            title: format!("k{i}"),
            width: 120.0,
            align: if i == 0 { Align::Left } else { Align::Right },
            sortable: true,
        })
        .collect()
}

fn tabela(c: &mut Criterion) {
    let theme = Theme::load().expect("data/ui/theme.ron");
    let zrodlo = Arc::new(Mieszkancy(WIERSZE));
    let ctx = egui::Context::default();

    // Budowa i rysowanie widoku: wirtualizacja ma sprawić, że sto tysięcy wierszy
    // kosztuje tyle, co sześćdziesiąt widocznych.
    c.bench_function("table_100k_draw", |b| {
        let mut t = Table::new(zrodlo.clone(), kolumny(), &theme);
        b.iter(|| {
            let (teksty, _) = testing::draw_in(&ctx, testing::input(testing::EKRAN), |ui| {
                t.show(ui, &theme);
            });
            black_box(teksty.len());
        });
    });

    // Przewijanie: nie wolno mu sortować ani filtrować ponownie, więc kosztuje
    // dokładnie tyle, co rysowanie innego okna.
    c.bench_function("table_100k_scroll", |b| {
        let mut t = Table::new(zrodlo.clone(), kolumny(), &theme);
        let mut offset = 0.0f32;
        b.iter(|| {
            offset = (offset + 220.0) % 40_000.0;
            let mut we = testing::input(testing::EKRAN);
            we.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -220.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            });
            let (teksty, _) = testing::draw_in(&ctx, we, |ui| {
                t.show(ui, &theme);
            });
            black_box(teksty.len());
        });
    });

    // Sortowanie i filtrowanie **poza klatką**: mierzymy samą funkcję, bo to ona
    // jedzie na osobnym wątku, a klatka w tym czasie pokazuje poprzedni porządek.
    c.bench_function("table_100k_sort", |b| {
        b.iter(|| {
            black_box(compute_order(
                zrodlo.as_ref(),
                Some((ColumnId(2), SortDir::Desc)),
                None,
            ))
        });
    });

    let filtr: Filter = Arc::new(|i: u32| i.is_multiple_of(3));
    c.bench_function("table_100k_filter", |b| {
        b.iter(|| black_box(compute_order(zrodlo.as_ref(), None, Some(&filtr))));
    });
}

fn wykres(c: &mut Criterion) {
    let theme = Theme::load().expect("data/ui/theme.ron");
    let ctx = egui::Context::default();
    // Dziesięć lat danych dziennych, osiem serii — 3600 próbek na serię (`K-1`).
    let dni = LAT * 360;
    let serie: Vec<Series> = (0..8)
        .map(|k| {
            let mut s = Series::new(SeriesKey(k));
            for d in 0..dni {
                s.push_day(i64::from((d * (u32::from(k) + 1)) % 9973));
            }
            s
        })
        .collect();

    c.bench_function("chart_10lat_8serii", |b| {
        b.iter(|| {
            let (_, zakres) = testing::draw_in(&ctx, testing::input(testing::EKRAN), |ui| {
                for s in &serie {
                    magnat_ui::chart(ui, &theme, s, 0, dni, 90.0);
                }
            });
            black_box(zakres);
        });
    });

    // Sama agregacja: dorzucenie doby ma być stałe, niezależnie od długości historii.
    c.bench_function("series_push_day", |b| {
        let mut s = Series::new(SeriesKey(0));
        for d in 0..dni {
            s.push_day(i64::from(d));
        }
        let mut v = 0i64;
        b.iter(|| {
            v += 1;
            s.push_day(black_box(v));
        });
    });
}

fn mapa_cieplna(c: &mut Criterion) {
    let ctx = egui::Context::default();
    let dim = 128u32;
    let pole: Vec<u8> = (0..dim * dim).map(|i| (i % 9) as u8).collect();
    let paleta: Vec<[u8; 4]> = (0..9u8).map(|i| [i * 25, 128, 255 - i * 25, 255]).collect();

    c.bench_function("heatmap_256", |b| {
        let mut m = magnat_ui::HeatmapThumb::new("bench");
        b.iter(|| black_box(m.update(&ctx, dim, &pole, &paleta)));
    });
}

criterion_group!(benches, tabela, wykres, mapa_cieplna);
criterion_main!(benches);
