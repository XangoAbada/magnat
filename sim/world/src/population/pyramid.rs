//! Kroki 1 i 2: piramida wieku epoki, skład gospodarstw i spawn mieszkańców.
//!
//! Wydzielone z `population/mod.rs` w R-WP4 bez zmiany zachowania.

use super::*;

// ── pula wieku (krok 1) ─────────────────────────────────────────────────────────

/// Wielozbiór wieków z piramidy. Nie lista osób: osoby powstają dopiero przy spawnie,
/// a tu chodzi o to, żeby skład gospodarstw **zużył piramidę do zera** — inaczej test
/// χ² mierzy nie rozkład, tylko to, kogo zabrakło pod koniec pętli.
pub(super) struct PulaWieku {
    counts: Vec<u32>,
    total: u32,
}

impl PulaWieku {
    fn new(max_age: u8) -> PulaWieku {
        PulaWieku {
            counts: vec![0; usize::from(max_age) + 1],
            total: 0,
        }
    }

    fn add(&mut self, age: usize) {
        let i = age.min(self.counts.len() - 1);
        self.counts[i] += 1;
        self.total += 1;
    }

    fn take_at(&mut self, age: usize) -> Option<i32> {
        if self.counts.get(age).copied().unwrap_or(0) == 0 {
            return None;
        }
        self.counts[age] -= 1;
        self.total -= 1;
        Some(age as i32)
    }

    /// Najbliższy niepusty rocznik w przedziale `[lo, hi]`, licząc od `target`.
    fn take_near(&mut self, target: i32, lo: i32, hi: i32) -> Option<i32> {
        let hi = hi.min(self.counts.len() as i32 - 1);
        let lo = lo.max(0);
        if lo > hi {
            return None;
        }
        let t = target.clamp(lo, hi);
        for d in 0..=(hi - lo) {
            for kandydat in [t - d, t + d] {
                if (lo..=hi).contains(&kandydat) {
                    if let Some(a) = self.take_at(kandydat as usize) {
                        return Some(a);
                    }
                }
            }
        }
        None
    }

    /// Losowy rocznik z przedziału, proporcjonalnie do liczebności.
    fn take_in(&mut self, lo: i32, hi: i32, r: &mut Rng) -> Option<i32> {
        let hi = hi.min(self.counts.len() as i32 - 1);
        let lo = lo.max(0);
        if lo > hi {
            return None;
        }
        let suma: u32 = self.counts[lo as usize..=hi as usize].iter().sum();
        if suma == 0 {
            return None;
        }
        let mut k = r.gen_range_u32(suma);
        for a in lo..=hi {
            let c = self.counts[a as usize];
            if k < c {
                return self.take_at(a as usize);
            }
            k -= c;
        }
        None
    }

    fn len_in(&self, lo: i32, hi: i32) -> u32 {
        let hi = hi.min(self.counts.len() as i32 - 1);
        let lo = lo.max(0);
        if lo > hi {
            return 0;
        }
        self.counts[lo as usize..=hi as usize].iter().sum()
    }
}

/// Krok 1: piramida wieku epoki.
pub(super) fn piramida(seed: u64, n: u32, bands: &[u16], max_age: u8) -> PulaWieku {
    let mut pula = PulaWieku::new(max_age);
    for i in 0..n {
        let mut r = rng(seed, StreamId::PopGen, i, Tick(1));
        let mut k = r.gen_range_u32(1000);
        let mut band = bands.len() - 1;
        for (b, w) in bands.iter().enumerate() {
            if k < u32::from(*w) {
                band = b;
                break;
            }
            k -= u32::from(*w);
        }
        let wiek = band as u32 * BAND_YEARS + r.gen_range_u32(BAND_YEARS);
        pula.add(wiek as usize);
    }
    pula
}

// ── krok 2: składanie gospodarstw ───────────────────────────────────────────────

/// Jedno gospodarstwo w trakcie składania: wieki członków, zanim powstaną encje.
pub(super) struct Sklad {
    adults: Vec<i32>,
    children: Vec<i32>,
    home: HomeSlot,
}

/// Krok 2: skład gospodarstw z puli wieku.
///
/// Typ gospodarstwa jest **punktem wyjścia**, nie wyrokiem: liczba dzieci jest skalowana
/// tak, żeby pula dzieci z piramidy wyszła na zero. Bez tego skład i piramida opisywałyby
/// dwa różne miasta i test χ² mierzyłby, które z nich wygrało.
pub(super) fn sklady(
    seed: u64,
    pula: &mut PulaWieku,
    homes: &[HomeSlot],
    mix: &[table::HouseholdMix],
    adult_age: i32,
    max_age: i32,
) -> Vec<Sklad> {
    let doroslych = pula.len_in(adult_age, max_age);
    let dzieci = pula.len_in(0, adult_age - 1);
    let w_suma: u32 = mix.iter().map(|m| u32::from(m.weight)).sum::<u32>().max(1);
    // Średni skład z wag — w setnych osoby, żeby nie schodzić do floatów.
    let sr_doroslych = (mix
        .iter()
        .map(|m| u32::from(m.weight) * u32::from(m.adults))
        .sum::<u32>()
        * 100
        / w_suma)
        .max(100);
    let sr_dzieci = mix
        .iter()
        .map(|m| u32::from(m.weight) * u32::from(m.children))
        .sum::<u32>()
        * 100
        / w_suma;

    let h = ((doroslych * 100 / sr_doroslych) as usize).min(homes.len());
    let chciane = (h as u32) * sr_dzieci / 100;
    // Skala liczby dzieci: ile ich naprawdę jest wobec tego, ile chciałby rozkład.
    let skala = (dzieci * 1000).checked_div(chciane).unwrap_or(0);

    let mut out: Vec<Sklad> = Vec::with_capacity(h);
    for (i, home) in homes.iter().enumerate().take(h) {
        let mut r = rng(seed, StreamId::PopGen, i as u32, Tick(2));
        let mut k = r.gen_range_u32(w_suma);
        let mut typ = mix[0];
        for m in mix {
            if k < u32::from(m.weight) {
                typ = *m;
                break;
            }
            k -= u32::from(m.weight);
        }

        // Pierwszy dorosły z rozkładu; partner z Δwieku ~ 2 ± 4 lata (§5.9 krok 2).
        let Some(pierwszy) = pula.take_in(adult_age, max_age, &mut r) else {
            break;
        };
        let mut adults = vec![pierwszy];
        for _ in 1..typ.adults {
            let delta = 2 + r.gen_range_u32(9) as i32 - 4;
            match pula.take_near(pierwszy + delta, adult_age, max_age) {
                Some(a) => adults.push(a),
                None => break,
            }
        }

        let chce = (u32::from(typ.children) * skala + r.gen_range_u32(1000)) / 1000;
        let najstarszy = adults.iter().copied().max().unwrap_or(adult_age);
        let mut children = Vec::new();
        for _ in 0..chce.min(u32::from(typ.children) + 2) {
            // Rodzic starszy od dziecka o co najmniej 18 lat (§5.9 krok 2).
            let gorna = (najstarszy - adult_age).min(adult_age - 1);
            match pula.take_near((najstarszy - 30).max(0), 0, gorna) {
                Some(c) => children.push(c),
                None => break,
            }
        }

        out.push(Sklad {
            adults,
            children,
            home: *home,
        });
    }

    // Dzieci, które zostały, dopisujemy do gospodarstw mających dorosłego w wieku
    // rodzicielskim. Mieszkaniec zgubiony „bo skończyła się pętla" byłby dziurą
    // w piramidzie, której test χ² nie odróżni od błędu rozkładu.
    let mut i = 0usize;
    let limit = out.len().saturating_mul(4) + 1;
    let ile_gosp = out.len();
    while pula.len_in(0, adult_age - 1) > 0 && ile_gosp > 0 && i < limit {
        let g = &mut out[i % ile_gosp];
        let najstarszy = g.adults.iter().copied().max().unwrap_or(adult_age);
        let gorna = (najstarszy - adult_age).min(adult_age - 1);
        if g.adults.len() + g.children.len() < household::HH_MAX_MEMBERS {
            if let Some(c) = pula.take_near((najstarszy - 30).max(0), 0, gorna) {
                g.children.push(c);
            }
        }
        i += 1;
    }
    out
}

// ── krok 2b: spawn tym samym generatorem co migracja (korekta E-13) ─────────────

/// Krok 2b: gospodarstwa i mieszkańcy z gotowych składów.
pub(super) fn zasiedl(
    world: &mut World,
    sklady: &[Sklad],
    seed: u64,
    ages: Ages,
) -> (Vec<Entity>, Vec<Entity>) {
    let mut gospodarstwa: Vec<Entity> = Vec::with_capacity(sklady.len());
    let mut mieszkancy: Vec<Entity> = Vec::new();
    for (i, s) in sklady.iter().enumerate() {
        let mut r = rng(seed, StreamId::PopGen, i as u32, Tick(3));
        let hh = migration::spawn_household_aged(
            world,
            0,
            &mut r,
            s.adults.len() as u8,
            s.children.len() as u8,
            s.home,
            ages.adult,
            1,
        );
        gospodarstwa.push(hh);

        // Nadpisanie wieków: `spawn_household_aged` losuje je z parametrów migracji,
        // a Etap 8 ma je z piramidy epoki. Kolejność członków jest kolejnością spawnu —
        // najpierw dorośli, potem dzieci (§5.9 krok 2).
        let hh_c = *world.get::<Household>(hh).expect("gospodarstwo po spawnie");
        let sklad = household::members_of(hh.index(), &hh_c, world.resource::<HouseholdOverflow>());
        for (k, m) in sklad.iter().enumerate() {
            let wiek = if k < s.adults.len() {
                s.adults[k]
            } else {
                s.children[k - s.adults.len()]
            };
            let Some(c) = demography::citizen_by_index(world, *m) else {
                continue;
            };
            if let Some(id) = world.get_mut::<Identity>(c) {
                id.birth_day = -wiek * 360;
                id.birth_district = s.home.district;
            }
            if let Some(e) = world.get_mut::<Employment>(c) {
                let uczen =
                    wiek >= i32::from(ages.school_start) && wiek < i32::from(ages.school_end);
                e.flags = if uczen {
                    Employment::FLAG_PUPIL
                } else if wiek >= i32::from(ages.retirement) {
                    Employment::FLAG_RETIRED | Employment::FLAG_UNEMPLOYED
                } else {
                    Employment::FLAG_UNEMPLOYED
                };
                e.site = Employment::NO_SITE;
            }
            mieszkancy.push(c);
        }
        // Typ gospodarstwa liczy się ze składu, a skład właśnie zmienił wiek.
        let mut hh_c = hh_c;
        demography::przeklasyfikuj(world, hh.index(), &mut hh_c, 0);
        if let Some(slot) = world.get_mut::<Household>(hh) {
            slot.kind = hh_c.kind;
        }
    }
    (gospodarstwa, mieszkancy)
}

/// Docelowa liczba mieszkańców, gdy nikt jej nie narzucił.
///
/// Liczba **wynika z miasta**: kryterium `gen_jobs_filled` mówi
/// `|JobSlot| ≈ |aktywni| × (1 + bezrobocie)`, więc to liczba etatów wyznacza liczbę
/// aktywnych, a piramida wieku przelicza aktywnych na mieszkańców. Drugą granicą
/// jest liczba lokali: miasto nie zmieści więcej ludzi, niż ma mieszkań.
pub(super) fn docelowa_populacja(
    params: &PopulationParams,
    bands: &[u16],
    jobs_total: u32,
    homes_total: u32,
    unemp: u16,
    t: &PopulationTable,
    ages: Ages,
) -> u32 {
    if let Some(n) = params.target_population {
        return n;
    }
    // Pasma piramidy są pięcioletnie, a granice wieku produkcyjnego nie muszą na nie
    // trafiać: 18 lat wypada w środku pasma 15–19. Zaokrąglenie pasma w dół albo w górę
    // przesuwa udział aktywnych o kilka procent, a kryterium `gen_jobs_filled` ma
    // tolerancję 2 % — więc liczymy **część wspólną** pasma z przedziałem wieku.
    let (od, do_) = (u32::from(ages.work_start), u32::from(ages.retirement));
    let aktywnych_permille: u32 = bands
        .iter()
        .enumerate()
        .map(|(b, w)| {
            let (a, z) = (b as u32 * BAND_YEARS, b as u32 * BAND_YEARS + BAND_YEARS);
            let wspolne = z.min(do_).saturating_sub(a.max(od));
            u32::from(*w) * wspolne / BAND_YEARS
        })
        .sum::<u32>()
        .max(1);
    let aktywni = u64::from(jobs_total) * 1000 / u64::from(1000 + u32::from(unemp));
    let z_etatow = (aktywni * 1000 / u64::from(aktywnych_permille)) as u32;

    let w_suma: u32 = t
        .households
        .iter()
        .map(|m| u32::from(m.weight))
        .sum::<u32>()
        .max(1);
    let sr_osob = t
        .households
        .iter()
        .map(|m| u32::from(m.weight) * u32::from(m.adults + m.children))
        .sum::<u32>()
        / w_suma;
    z_etatow.min(homes_total.saturating_mul(sr_osob.max(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pula_wieku_oddaje_najblizszy_niepusty_rocznik() {
        let mut p = PulaWieku::new(110);
        p.add(30);
        p.add(34);
        // Remis rozstrzyga się na korzyść młodszego rocznika — deterministycznie
        // i niezależnie od kolejności wstawiania.
        assert_eq!(p.take_near(32, 18, 110), Some(30));
        assert_eq!(p.take_near(32, 18, 110), Some(34));
        assert_eq!(p.take_near(32, 18, 110), None);
    }

    #[test]
    fn piramida_odtwarza_rozklad() {
        let bands: Vec<u16> = (0..AGE_BANDS)
            .map(|i| if i < 10 { 100 } else { 0 })
            .collect();
        let pula = piramida(7, 20_000, &bands, 110);
        assert_eq!(pula.total, 20_000);
        // Wszyscy poniżej 50 lat, bo dziesięć pierwszych pasm po pięć lat.
        assert_eq!(pula.len_in(50, 110), 0);
        // Każde z dziesięciu pasm ma ~2000 osób (±3 σ ≈ ±135).
        for b in 0..10i32 {
            let n = pula.len_in(b * 5, b * 5 + 4);
            assert!((1700..2300).contains(&n), "pasmo {b}: {n}");
        }
    }
}
