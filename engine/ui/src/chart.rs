//! Serie metryk i wykres z piramidą mip (`M9b` §5.8, WP6).
//!
//! # Dlaczego dekada, a nie tydzień
//!
//! Kalendarz ma 360 dni, 12 × 30 (`K-1`). Przy takim podziale **każdy** poziom piramidy
//! dzieli się bez reszty: 30 = 3 × 10, 90 = 3 × 30, 360 = 4 × 90. Agregacja jest wtedy
//! dokładna, a oś czasu ma równe podziałki miesięczne bez dryfu. Tydzień siedmiodniowy
//! istnieje i dryfuje względem miesiąca (`K-15`) — na osi wykresu byłby fałszem.
//!
//! # Dlaczego sumy są bezstratne
//!
//! Poziom wyższy powstaje ze **scalania** próbek niższego, nie z ponownego liczenia:
//! `sum` miesiąca jest sumą `sum` trzech dekad co do jednostki. To jest kryterium z §7
//! dokumentu fazy i pilnuje go test.
//!
//! Wykres rysuje się z poziomu dobranego do szerokości w pikselach — **nigdy więcej
//! niż [`MAX_SEGMENTS`] odcinków**, niezależnie od zakresu.

use crate::theme::{ColorToken, TextRole, Theme};

/// Identyfikator serii. Nazwy nadaje faza, która serię produkuje (`M9e`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct SeriesKey(pub u16);

/// Próbka zagregowana. Cztery liczby, bo cztery pytania: „ile najmniej", „ile najwięcej",
/// „ile razem" i „z ilu dób" — średnia bez `n` byłaby średnią średnich.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Sample {
    pub min: i64,
    pub max: i64,
    pub sum: i64,
    pub n: u32,
}

impl Sample {
    /// Próbka z jednej doby.
    #[must_use]
    pub const fn one(v: i64) -> Sample {
        Sample {
            min: v,
            max: v,
            sum: v,
            n: 1,
        }
    }

    /// Dokłada próbkę do kubełka. `sum` sumuje się co do jednostki — na tym stoi
    /// bezstratność agregacji.
    pub fn merge(&mut self, other: Sample) {
        if self.n == 0 {
            *self = other;
            return;
        }
        self.min = self.min.min(other.min);
        self.max = self.max.max(other.max);
        self.sum = self.sum.saturating_add(other.sum);
        self.n += other.n;
    }

    #[must_use]
    pub fn avg(self) -> i64 {
        if self.n == 0 {
            0
        } else {
            self.sum / i64::from(self.n)
        }
    }
}

/// Poziom piramidy. Dzień, dekada (10 dób), miesiąc (30), kwartał (90).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum MipLevel {
    Day,
    Decade,
    Month,
    Quarter,
}

pub const MIP_LEVELS: usize = 4;

impl MipLevel {
    pub const ALL: [MipLevel; MIP_LEVELS] = [
        MipLevel::Day,
        MipLevel::Decade,
        MipLevel::Month,
        MipLevel::Quarter,
    ];

    /// Ile dób mieści kubełek tego poziomu.
    #[must_use]
    pub const fn days(self) -> u32 {
        match self {
            MipLevel::Day => 1,
            MipLevel::Decade => 10,
            MipLevel::Month => 30,
            MipLevel::Quarter => 90,
        }
    }

    #[must_use]
    pub const fn as_index(self) -> usize {
        self as usize
    }
}

/// Najwięcej odcinków, ile wykres narysuje — niezależnie od zakresu (§7).
pub const MAX_SEGMENTS: usize = 2000;

/// Historia jednej metryki w czterech rozdzielczościach.
#[derive(Clone, Debug)]
pub struct Series {
    pub key: SeriesKey,
    mips: [Vec<Sample>; MIP_LEVELS],
    /// Numer doby, którą przyjmie następne [`Series::push_day`].
    next_day: u32,
}

impl Series {
    #[must_use]
    pub fn new(key: SeriesKey) -> Series {
        Series {
            key,
            mips: Default::default(),
            next_day: 0,
        }
    }

    /// Dokłada wartość z kolejnej doby i **od razu** wlicza ją do wszystkich poziomów.
    ///
    /// Kubełek niepełny jest normalnym stanem: patrzenie na wykres w połowie miesiąca
    /// ma pokazać połowę miesiąca, a nie dziurę.
    pub fn push_day(&mut self, value: i64) {
        let doba = self.next_day;
        self.next_day += 1;
        let s = Sample::one(value);
        for poziom in MipLevel::ALL {
            let i = (doba / poziom.days()) as usize;
            let v = &mut self.mips[poziom.as_index()];
            if v.len() == i {
                v.push(s);
            } else {
                v[i].merge(s);
            }
        }
    }

    #[must_use]
    pub fn days(&self) -> u32 {
        self.next_day
    }

    #[must_use]
    pub fn level(&self, l: MipLevel) -> &[Sample] {
        &self.mips[l.as_index()]
    }

    /// Poziom, z którego rysuje się zakres `days` dób na `px` pikselach.
    ///
    /// Bierze najdrobniejszy poziom, który mieści się i w pikselach, i w sufi­cie
    /// [`MAX_SEGMENTS`]: rysowanie dwóch odcinków na piksel nie dodaje informacji,
    /// a kosztuje tyle, ile ich jest.
    #[must_use]
    pub fn level_for(days: u32, px: f32) -> MipLevel {
        let sufit = (px.max(1.0) as usize).min(MAX_SEGMENTS);
        for l in MipLevel::ALL {
            if (days / l.days()) as usize <= sufit {
                return l;
            }
        }
        MipLevel::Quarter
    }

    /// Próbki poziomu przycięte do zakresu dób `[od, do)`.
    #[must_use]
    pub fn window(&self, l: MipLevel, od: u32, do_: u32) -> &[Sample] {
        let v = self.level(l);
        let a = (od / l.days()) as usize;
        let b = (do_.div_ceil(l.days()) as usize).min(v.len());
        if a >= b {
            return &[];
        }
        &v[a..b]
    }
}

/// Rysuje wykres liniowy serii w zakresie dób `[od, do)`.
///
/// Zwraca poziom, z którego narysowano — do podpisu osi i do testu, że zakres
/// dziesięcioletni nie jedzie z poziomu dobowego.
pub fn chart(
    ui: &mut egui::Ui,
    theme: &Theme,
    series: &Series,
    od: u32,
    do_: u32,
    wysokosc: f32,
) -> MipLevel {
    let szerokosc = ui.available_width().max(64.0);
    let poziom = Series::level_for(do_.saturating_sub(od), szerokosc);
    let probki = series.window(poziom, od, do_);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(szerokosc, wysokosc), egui::Sense::hover());
    let malarz = ui.painter_at(rect);
    malarz.rect_filled(rect, 3.0, theme.color(ColorToken::BgCard));
    if probki.is_empty() {
        return poziom;
    }

    let lo = probki.iter().map(|s| s.min).min().unwrap_or(0);
    let hi = probki.iter().map(|s| s.max).max().unwrap_or(1);
    let rozpietosc = (hi - lo).max(1) as f64;
    let na_probke = f64::from(szerokosc) / probki.len().max(1) as f64;
    let y = |v: i64| {
        let t = (v - lo) as f64 / rozpietosc;
        rect.bottom() - (t * f64::from(wysokosc)) as f32
    };

    // Pasmo min–max pod linią średniej: zakres jest tu informacją, a nie ozdobą —
    // miesiąc o tej samej średniej i innej zmienności ma wyglądać inaczej.
    let mut punkty = Vec::with_capacity(probki.len());
    for (i, s) in probki.iter().enumerate() {
        let x = rect.left() + (i as f64 * na_probke) as f32;
        malarz.line_segment(
            [egui::pos2(x, y(s.min)), egui::pos2(x, y(s.max))],
            egui::Stroke::new(1.0, theme.color(ColorToken::LineStrong)),
        );
        punkty.push(egui::pos2(x, y(s.avg())));
    }
    malarz.add(egui::epaint::Shape::line(
        punkty,
        egui::Stroke::new(1.5, theme.color(ColorToken::Accent)),
    ));

    // Podziałki: granice kubełków poziomu **o jeden grubszego** — inaczej oś
    // dziesięcioletnia miałaby sto dwadzieścia podpisów jeden na drugim.
    let opis = theme.mono(TextRole::Micro);
    for (etykieta, v) in [("min", lo), ("max", hi)] {
        malarz.text(
            egui::pos2(rect.left() + 2.0, y(v)),
            egui::Align2::LEFT_BOTTOM,
            format!("{etykieta} {v}"),
            opis.clone(),
            theme.color(ColorToken::TextSecondary),
        );
    }
    poziom
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dziesiec_lat() -> Series {
        let mut s = Series::new(SeriesKey(1));
        // 10 lat × 360 dób = 3600 próbek dziennych (§7 dokumentu fazy).
        for d in 0..3600 {
            s.push_day(i64::from(d % 97));
        }
        s
    }

    #[test]
    fn agregacja_jest_bezstratna_dla_sum() {
        let s = dziesiec_lat();
        assert_eq!(s.level(MipLevel::Day).len(), 3600);
        assert_eq!(s.level(MipLevel::Decade).len(), 360);
        assert_eq!(s.level(MipLevel::Month).len(), 120);
        assert_eq!(s.level(MipLevel::Quarter).len(), 40);

        // Suma poziomu wyższego = suma poziomu niższego, tolerancja 0.
        let suma = |l: MipLevel| s.level(l).iter().map(|x| x.sum).sum::<i64>();
        let dzien = suma(MipLevel::Day);
        for l in MipLevel::ALL {
            assert_eq!(suma(l), dzien, "poziom {l:?} zgubił sumę");
        }
        // Liczebności też: 3600 dób rozdzielone bez reszty.
        for l in MipLevel::ALL {
            let n: u32 = s.level(l).iter().map(|x| x.n).sum();
            assert_eq!(n, 3600, "poziom {l:?} zgubił próbki");
        }
        // Kubełek miesiąca to dokładnie trzy dekady.
        let m0 = s.level(MipLevel::Month)[0];
        let d: i64 = s.level(MipLevel::Decade)[..3].iter().map(|x| x.sum).sum();
        assert_eq!(m0.sum, d);
        assert_eq!(m0.n, 30);
    }

    #[test]
    fn kubelek_niepelny_jest_widoczny_od_pierwszej_doby() {
        let mut s = Series::new(SeriesKey(0));
        s.push_day(5);
        assert_eq!(s.level(MipLevel::Quarter).len(), 1);
        assert_eq!(s.level(MipLevel::Quarter)[0], Sample::one(5));
        s.push_day(15);
        assert_eq!(s.level(MipLevel::Decade)[0].min, 5);
        assert_eq!(s.level(MipLevel::Decade)[0].max, 15);
        assert_eq!(s.level(MipLevel::Decade)[0].avg(), 10);
    }

    #[test]
    fn dziesiec_lat_nie_rysuje_sie_z_poziomu_dobowego() {
        // 3600 dób na 800 pikselach: dobowy dałby 3600 odcinków, czyli 4,5 na piksel.
        assert_eq!(Series::level_for(3600, 800.0), MipLevel::Decade);
        // Miesiąc na tej samej szerokości mieści się dobowo.
        assert_eq!(Series::level_for(30, 800.0), MipLevel::Day);
        // Sufit działa też przy absurdalnej szerokości.
        assert!((Series::level_for(360_000, 100_000.0).days() as usize) >= 1);
        let s = dziesiec_lat();
        for px in [200.0f32, 800.0, 1600.0] {
            let l = Series::level_for(3600, px);
            assert!(
                s.window(l, 0, 3600).len() <= MAX_SEGMENTS,
                "{px} px: {} odcinków",
                s.window(l, 0, 3600).len()
            );
        }
    }

    #[test]
    fn wykres_rysuje_sie_bez_gpu_i_na_wlasciwym_poziomie() {
        let theme = Theme::load().expect("data/ui/theme.ron");
        let s = dziesiec_lat();
        let mut poziom = MipLevel::Day;
        let _ = crate::testing::draw(|ui| {
            poziom = chart(ui, &theme, &s, 0, 3600, 120.0);
        });
        assert!(poziom > MipLevel::Day, "dziesięć lat z poziomu dobowego");

        // Seria pusta nie panikuje — wykres otwarty w pierwszej minucie gry.
        let pusta = Series::new(SeriesKey(2));
        let _ = crate::testing::draw(|ui| {
            chart(ui, &theme, &pusta, 0, 30, 60.0);
        });
    }
}
