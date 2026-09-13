//! Metryki — nazwane liczniki per tick w buforze pierścieniowym (M0 §5.10).
//!
//! Konsument docelowy: balansator (M5), który uruchamia headless N razy i czyta CSV.
//! Bufor jest pierścieniowy, bo sesja stuletnia (M12) to 190 mln ticków — trzymanie
//! wszystkiego w pamięci nie jest opcją, a ostatnie kilkadziesiąt tysięcy ticków
//! wystarcza do zdiagnozowania każdego rozjazdu, który widać na wykresie.

use magnat_core::collections::{seeded_map, SeededMap};
use magnat_core::Tick;
use std::io::Write;
use std::path::Path;

pub struct MetricSink {
    capacity_ticks: usize,
    /// Kolejność wstawiania — deterministyczna (00 §3.2), więc kolumny CSV
    /// nie zmieniają kolejności między przebiegami.
    series: SeededMap<&'static str, Vec<i64>>,
    ticks: Vec<u64>,
    /// Indeks najstarszego wpisu w pierścieniu.
    start: usize,
    len: usize,
}

impl MetricSink {
    #[must_use]
    pub fn new(capacity_ticks: usize) -> MetricSink {
        assert!(capacity_ticks > 0, "MetricSink o zerowej pojemności");
        MetricSink {
            capacity_ticks,
            series: seeded_map(),
            ticks: vec![0; capacity_ticks],
            start: 0,
            len: 0,
        }
    }

    /// Zapisuje wartość licznika dla danego ticku. Pierwszy zapis w ticku przesuwa
    /// pierścień; kolejne trafiają do tej samej kolumny czasu.
    pub fn record(&mut self, tick: Tick, name: &'static str, value: i64) {
        let pozycja = match self.pozycja_ticku(tick) {
            Some(p) => p,
            None => self.rozpocznij_tick(tick),
        };
        let cap = self.capacity_ticks;
        let seria = self.series.entry(name).or_insert_with(|| vec![0; cap]);
        seria[pozycja] = value;
    }

    fn pozycja_ticku(&self, tick: Tick) -> Option<usize> {
        if self.len == 0 {
            return None;
        }
        let ostatni = (self.start + self.len - 1) % self.capacity_ticks;
        (self.ticks[ostatni] == tick.0).then_some(ostatni)
    }

    fn rozpocznij_tick(&mut self, tick: Tick) -> usize {
        let pozycja = (self.start + self.len) % self.capacity_ticks;
        if self.len == self.capacity_ticks {
            self.start = (self.start + 1) % self.capacity_ticks;
        } else {
            self.len += 1;
        }
        self.ticks[pozycja] = tick.0;
        for seria in self.series.values_mut() {
            seria[pozycja] = 0;
        }
        pozycja
    }

    /// Wartości serii w kolejności czasu (od najstarszej).
    #[must_use]
    pub fn series(&self, name: &str) -> Vec<i64> {
        let Some(seria) = self.series.get(name) else {
            return Vec::new();
        };
        (0..self.len)
            .map(|i| seria[(self.start + i) % self.capacity_ticks])
            .collect()
    }

    #[must_use]
    pub fn recorded_ticks(&self) -> Vec<u64> {
        (0..self.len)
            .map(|i| self.ticks[(self.start + i) % self.capacity_ticks])
            .collect()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Pamięć zajmowana przez bufor — do pilnowania budżetu z WP-11.
    #[must_use]
    pub fn memory_bytes(&self) -> usize {
        self.capacity_ticks * size_of::<u64>()
            + self.series.len() * self.capacity_ticks * size_of::<i64>()
    }

    /// Eksport CSV: pierwsza kolumna to tick, dalej serie w kolejności wstawiania.
    pub fn export_csv(&self, path: &Path) -> std::io::Result<()> {
        let mut f = std::fs::File::create(path)?;
        write!(f, "tick")?;
        for name in self.series.keys() {
            write!(f, ",{name}")?;
        }
        writeln!(f)?;
        let ticks = self.recorded_ticks();
        let kolumny: Vec<Vec<i64>> = self.series.keys().map(|k| self.series(k)).collect();
        for (i, tick) in ticks.iter().enumerate() {
            write!(f, "{tick}")?;
            for kolumna in &kolumny {
                write!(f, ",{}", kolumna[i])?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pierscien_trzyma_ostatnie_ticki() {
        let mut m = MetricSink::new(4);
        for t in 1..=10u64 {
            m.record(Tick(t), "encje", t as i64 * 2);
        }
        assert_eq!(m.recorded_ticks(), vec![7, 8, 9, 10]);
        assert_eq!(m.series("encje"), vec![14, 16, 18, 20]);
        assert_eq!(m.len(), 4);
    }

    #[test]
    fn kilka_licznikow_w_jednym_ticku() {
        let mut m = MetricSink::new(8);
        m.record(Tick(1), "a", 1);
        m.record(Tick(1), "b", 2);
        m.record(Tick(2), "a", 3);
        assert_eq!(m.series("a"), vec![1, 3]);
        assert_eq!(
            m.series("b"),
            vec![2, 0],
            "brak zapisu = zero, nie ostatnia wartość"
        );
        assert_eq!(m.series("nieznana"), Vec::<i64>::new());
    }

    #[test]
    fn budzet_pamieci_64_licznikow_przez_10_tys_tickow() {
        // Kryterium WP-11: 64 liczniki × 10 tys. ticków mają się mieścić
        // w rozsądnym budżecie (poniżej 8 MiB).
        let mut m = MetricSink::new(10_000);
        for i in 0..64 {
            let name: &'static str = Box::leak(format!("licznik{i}").into_boxed_str());
            m.record(Tick(1), name, i as i64);
        }
        assert!(m.memory_bytes() < 8 * 1024 * 1024, "{} B", m.memory_bytes());
    }

    #[test]
    fn eksport_csv_ma_naglowek_i_wiersze() {
        let mut m = MetricSink::new(4);
        m.record(Tick(1), "a", 10);
        m.record(Tick(2), "a", 20);
        let path = std::env::temp_dir().join("magnat-metrics-test.csv");
        m.export_csv(&path).expect("zapis csv");
        let tresc = std::fs::read_to_string(&path).expect("odczyt csv");
        assert_eq!(tresc.lines().next(), Some("tick,a"));
        assert_eq!(tresc.lines().count(), 3);
        std::fs::remove_file(&path).ok();
    }
}
