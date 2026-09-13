//! `ClusterOccupancy` — histogram zajętości klastrów świateł (M1 §6.1, WP-R4).
//!
//! To jest **przyrząd kalibracyjny**, nie licznik dla ozdoby. M2 i M11 dostaną budżet świateł
//! na dzielnicę i będą musiały wiedzieć, ile świateł realnie trafia do jednego froxela —
//! a to zależy od ich rozmieszczenia, nie od ich liczby. Tysiąc latarni rozłożonych wzdłuż
//! ulic mieści się bez problemu; sto reflektorów wycelowanych w jeden plac przepełnia klaster
//! i światła zaczynają znikać. Różnicę widać wyłącznie w histogramie.

/// Górna granica histogramu: tyle świateł na klaster przyjmuje shader.
pub const MAX_LIGHTS_PER_CLUSTER: usize = 64;

/// Rozkład liczby świateł przypadających na klaster.
#[derive(Clone, Debug)]
pub struct ClusterOccupancy {
    /// `kubelki[i]` = liczba klastrów, do których trafiło `i` świateł; ostatni kubełek
    /// zbiera wszystkie przepełnione.
    kubelki: Vec<u32>,
    klastrow: u32,
    swiatel: u32,
    maksimum: u32,
    przepelnionych: u32,
}

impl Default for ClusterOccupancy {
    fn default() -> Self {
        ClusterOccupancy {
            kubelki: vec![0; MAX_LIGHTS_PER_CLUSTER + 1],
            klastrow: 0,
            swiatel: 0,
            maksimum: 0,
            przepelnionych: 0,
        }
    }
}

impl ClusterOccupancy {
    /// Przelicza histogram z liczników klastrów odczytanych z GPU.
    pub fn record(&mut self, counts: &[u32]) {
        self.kubelki.iter_mut().for_each(|k| *k = 0);
        self.klastrow = counts.len() as u32;
        self.swiatel = 0;
        self.maksimum = 0;
        self.przepelnionych = 0;
        for c in counts {
            let i = (*c as usize).min(MAX_LIGHTS_PER_CLUSTER);
            self.kubelki[i] += 1;
            self.swiatel += *c;
            self.maksimum = self.maksimum.max(*c);
            if *c as usize >= MAX_LIGHTS_PER_CLUSTER {
                self.przepelnionych += 1;
            }
        }
    }

    /// Największa liczba świateł w jednym klastrze.
    #[must_use]
    pub fn max(&self) -> u32 {
        self.maksimum
    }

    /// Średnia liczba świateł na klaster — sama w sobie mówi niewiele, ale w parze
    /// z maksimum pokazuje, czy rozkład jest równy, czy skupiony.
    #[must_use]
    pub fn mean(&self) -> f32 {
        if self.klastrow == 0 {
            return 0.0;
        }
        self.swiatel as f32 / self.klastrow as f32
    }

    /// Ile klastrów przekroczyło pojemność — każdy z nich gubi światła.
    #[must_use]
    pub fn overflowing(&self) -> u32 {
        self.przepelnionych
    }

    /// Histogram: `[liczba klastrów o 0 światłach, o 1, …, o ≥ MAX]`.
    #[must_use]
    pub fn histogram(&self) -> &[u32] {
        &self.kubelki
    }

    /// Wiersz do konsoli debug (F3) — zwięzły, bo ma się mieścić obok innych liczników.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "klastry: {} · świateł/klaster śr. {:.1}, maks. {}, przepełnionych {}",
            self.klastrow,
            self.mean(),
            self.maksimum,
            self.przepelnionych
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_liczy_klastry_a_nie_swiatla() {
        let mut o = ClusterOccupancy::default();
        o.record(&[0, 0, 3, 3, 3, 70]);
        assert_eq!(o.histogram()[0], 2, "dwa puste klastry");
        assert_eq!(o.histogram()[3], 3, "trzy klastry po trzy światła");
        assert_eq!(
            o.histogram()[MAX_LIGHTS_PER_CLUSTER],
            1,
            "jeden przepełniony"
        );
        assert_eq!(o.max(), 70);
        assert_eq!(o.overflowing(), 1);
        assert!((o.mean() - 79.0 / 6.0).abs() < 1e-4);
    }

    #[test]
    fn powtorny_zapis_nie_dodaje_do_poprzedniego() {
        // Histogram opisuje **bieżącą** klatkę; kumulowanie zamieniłoby przyrząd
        // w licznik od uruchomienia i przestałoby cokolwiek znaczyć.
        let mut o = ClusterOccupancy::default();
        o.record(&[5, 5, 5]);
        o.record(&[1]);
        assert_eq!(o.histogram()[5], 0);
        assert_eq!(o.histogram()[1], 1);
        assert_eq!(o.max(), 1);
    }
}
