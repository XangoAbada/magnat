//! Krok 10: raport z generacji — liczby, które sprawdza §7.4 dokumentu fazy.
//!
//! Wydzielone z `population/mod.rs` w R-WP4 bez zmiany zachowania.

use super::homes::{histogram_docelowy, Mieszkania};
use super::jobs::Zatrudnienie;
use super::*;

/// Wynik generacji — liczby, które sprawdza §7.4 dokumentu fazy.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct PopulationReport {
    pub citizens: u32,
    pub households: u32,
    pub homes_total: u32,
    pub homes_free: u32,
    pub jobs_total: u32,
    pub jobs_free: u32,
    /// Aktywni zawodowo: wiek produkcyjny i nie uczą się.
    pub active: u32,
    pub employed: u32,
    pub pupils: u32,
    pub unemployment_permille: u16,
    pub unemployment_target_permille: u16,
    /// Udział zatrudnionych z `skill_match(role) ≥ 40`, w promilach.
    pub skill_fit_permille: u16,
    pub commute_median_min: u16,
    pub commute_target_min: u16,
    /// Histogram uzyskany — **liczności**, nie promile: χ² liczy się z liczności,
    /// a przeliczenie na promile gubi po jednym na kubełek i samo w sobie daje
    /// χ² rzędu kilkudziesięciu przy stu tysiącach mieszkańców.
    pub commute_hist: [u32; COMMUTE_BINS],
    /// Histogram docelowy w promilach.
    pub commute_target_hist: [u32; COMMUTE_BINS],
    /// Korelacja rang Spearmana (dochód GD, wartość lokalu) × 100.
    pub income_housing_rho_centi: i16,
    /// Piramida wieku uzyskana — liczności (jak wyżej).
    pub pyramid: [u32; AGE_BANDS],
    /// Piramida docelowa epoki, w promilach.
    pub pyramid_target: [u16; AGE_BANDS],
    pub places: u32,
    pub knowledge_entries: u64,
    pub relations: u64,
    /// Żądana mediana dojazdu, zanim przycięło ją to, co miasto dopuszcza.
    pub commute_requested_min: u16,
    /// Przedział median osiągalnych w tym mieście przy ruchu wyłącznie pieszym.
    pub commute_feasible: (u16, u16),
    /// Czas kroków w sekundach — wejście bramki wydajności §7.5 (`bench_population_gen`).
    /// Mierzony tak samo jak czasy etapów w `GenerationReport` M2.
    pub timings: Vec<(&'static str, f32)>,
    /// Mieszkańcy bez lokalu — kryterium `gen_everyone_has_home` mówi „zero".
    pub homeless: u32,
    /// Uczniowie bez przypisanej placówki — tyle samo warte co wyżej.
    pub pupils_without_school: u32,
    /// Mieszkańcy, którzy nie znają **żadnego** miejsca zaspokajającego potrzebę.
    ///
    /// To **nie jest błąd**, tylko treść §5.7: kto mieszka na obrzeżu bez sklepu
    /// w promieniu zasiewu, ten zna tylko własną pracę, a resztę dostanie od sąsiadów
    /// przez plotkę. Ale udział takich ludzi trzeba widzieć — jeden procent to
    /// przedmieście, połowa to miasto stojące w miejscu (korekta H-20).
    pub without_knowledge: u32,
}

impl PopulationReport {
    /// Statystyka χ² histogramu czasu dojazdu wobec celu (test `gen_commute_hist`).
    #[must_use]
    pub fn commute_chi2(&self) -> f64 {
        let n: u32 = self.commute_hist.iter().sum();
        chi2(&self.commute_hist, &self.commute_target_hist, n)
    }

    /// Statystyka χ² piramidy wieku wobec piramidy epoki (test `gen_age_pyramid`).
    #[must_use]
    pub fn pyramid_chi2(&self) -> f64 {
        let b: [u32; AGE_BANDS] = std::array::from_fn(|i| u32::from(self.pyramid_target[i]));
        chi2(&self.pyramid, &b, self.citizens)
    }

    /// Histogram dojazdu w promilach — do wydruku, nie do testu.
    #[must_use]
    pub fn commute_permille(&self) -> [u32; COMMUTE_BINS] {
        let n: u32 = self.commute_hist.iter().sum();
        std::array::from_fn(|i| (self.commute_hist[i] * 1000).checked_div(n).unwrap_or(0))
    }

    /// Piramida w promilach — do wydruku, nie do testu.
    #[must_use]
    pub fn pyramid_permille(&self) -> [u32; AGE_BANDS] {
        std::array::from_fn(|i| {
            (self.pyramid[i] * 1000)
                .checked_div(self.citizens)
                .unwrap_or(0)
        })
    }

    pub fn lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        out.push(format!(
            "populacja: {} mieszkańców w {} gospodarstwach, {} miejsc w katalogu",
            self.citizens, self.households, self.places
        ));
        out.push(format!(
            "lokale: {}/{} wolnych · etaty: {}/{} wolnych",
            self.homes_free, self.homes_total, self.jobs_free, self.jobs_total
        ));
        out.push(format!(
            "praca: {} aktywnych, {} zatrudnionych, bezrobocie {} ‰ (cel {} ‰), dopasowanie {} ‰",
            self.active,
            self.employed,
            self.unemployment_permille,
            self.unemployment_target_permille,
            self.skill_fit_permille
        ));
        out.push(format!(
            "dojazd: mediana {} min (cel {} min, żądano {}, osiągalne {}–{}), χ² {:.1}",
            self.commute_median_min,
            self.commute_target_min,
            self.commute_requested_min,
            self.commute_feasible.0,
            self.commute_feasible.1,
            self.commute_chi2()
        ));
        out.push(format!(
            "dochód↔mieszkanie: ρ = {:.2} · piramida χ² {:.1}",
            f64::from(self.income_housing_rho_centi) / 100.0,
            self.pyramid_chi2()
        ));
        out.push(format!(
            "wiedza: {} wpisów, relacje: {} krawędzi, uczniów {} (bez szkoły {}), bezdomnych {}",
            self.knowledge_entries,
            self.relations,
            self.pupils,
            self.pupils_without_school,
            self.homeless
        ));
        out.push(format!(
            "bez znajomości miejsc: {} ({:.1} %)",
            self.without_knowledge,
            f64::from(self.without_knowledge) * 100.0 / f64::from(self.citizens.max(1))
        ));
        if !self.timings.is_empty() {
            out.push(format!(
                "kroki: {}",
                self.timings
                    .iter()
                    .map(|(n, t)| format!("{n} {t:.1} s"))
                    .collect::<Vec<_>>()
                    .join(" · ")
            ));
        }
        out
    }
}

/// χ² liczności wobec rozkładu docelowego podanego w promilach.
///
/// Liczności, nie promile: przeliczenie obserwacji na promile obcina po ułamku
/// na kubełek, a przy stu tysiącach obserwacji samo to obcięcie daje χ² rzędu
/// kilkudziesięciu — czyli więcej niż próg testu.
fn chi2(obs: &[u32], exp_permille: &[u32], n: u32) -> f64 {
    if n == 0 {
        return 0.0;
    }
    let mut s = 0.0;
    for (o, e) in obs.iter().zip(exp_permille) {
        let oczekiwane = f64::from(*e) * f64::from(n) / 1000.0;
        if oczekiwane < 1.0 {
            continue;
        }
        let d = f64::from(*o) - oczekiwane;
        s += d * d / oczekiwane;
    }
    s
}

// ── krok 10: weryfikacja ────────────────────────────────────────────────────────

pub(super) struct Liczby {
    pub(super) homes_total: u32,
    pub(super) homes_free: u32,
    pub(super) jobs_total: u32,
    pub(super) jobs_free: u32,
    pub(super) unemp: u16,
    pub(super) commute_cel: u16,
    pub(super) commute_sigma: u16,
    pub(super) places: u32,
    pub(super) wiedza: u64,
    pub(super) relacje: u64,
    pub(super) bez_szkoly: u32,
}

pub(super) fn raport(
    world: &World,
    mieszkancy: &[Entity],
    gospodarstwa: &[Entity],
    bands: &[u16],
    z: &Zatrudnienie,
    m: &Mieszkania,
    l: Liczby,
) -> PopulationReport {
    let n = mieszkancy.len() as u32;
    let mut piramida = [0u32; AGE_BANDS];
    let mut bezdomnych = 0u32;
    let mut bez_wiedzy = 0u32;
    for c in mieszkancy {
        if world
            .get::<magnat_agents::KnowledgeRef>(*c)
            .is_none_or(|k| k.len == 0)
        {
            bez_wiedzy += 1;
        }
        if let Some(id) = world.get::<Identity>(*c) {
            let b = (id.age_years(0).max(0) as usize / BAND_YEARS as usize).min(AGE_BANDS - 1);
            piramida[b] += 1;
        }
        if world
            .get::<Residence>(*c)
            .is_none_or(|r| r.building == Residence::HOMELESS)
        {
            bezdomnych += 1;
        }
    }

    let bezrobocie = if z.active == 0 {
        0
    } else {
        ((u64::from(z.active - z.employed) * 1000) / u64::from(z.active)) as u16
    };

    PopulationReport {
        citizens: n,
        households: gospodarstwa.len() as u32,
        homes_total: l.homes_total,
        homes_free: l.homes_free,
        jobs_total: l.jobs_total,
        jobs_free: l.jobs_free,
        active: z.active,
        employed: z.employed,
        pupils: z.pupils,
        unemployment_permille: bezrobocie,
        unemployment_target_permille: l.unemp,
        skill_fit_permille: if z.employed == 0 {
            0
        } else {
            ((u64::from(z.fit_ok) * 1000) / u64::from(z.employed)) as u16
        },
        commute_median_min: m.median,
        commute_target_min: m.cel,
        commute_hist: m.hist,
        commute_target_hist: histogram_docelowy(m.cel, l.commute_sigma),
        income_housing_rho_centi: m.rho_centi,
        commute_requested_min: l.commute_cel,
        commute_feasible: m.osiagalne,
        pyramid: piramida,
        pyramid_target: std::array::from_fn(|i| bands.get(i).copied().unwrap_or(0)),
        places: l.places,
        knowledge_entries: l.wiedza,
        relations: l.relacje,
        timings: Vec::new(),
        homeless: bezdomnych,
        pupils_without_school: l.bez_szkoly,
        without_knowledge: bez_wiedzy,
    }
}
