//! Krok 5: dopasowanie pracy w granicach dzielnicy i przypisanie szkół uczniom.
//!
//! Wydzielone z `population/mod.rs` w R-WP4 bez zmiany zachowania.

use super::traits::skill_match;
use super::*;

// ── krok 5: dopasowanie pracy ───────────────────────────────────────────────────

/// Wynik kroku 5.
pub(super) struct Zatrudnienie {
    /// Etaty, które zostały wolne.
    pub(super) wolne: Vec<JobSlot>,
    /// Wszystkie etaty miasta — `Vacancies` potrzebuje ich do mapy zakładów.
    pub(super) wszystkie: Vec<JobSlot>,
    pub(super) active: u32,
    pub(super) employed: u32,
    pub(super) pupils: u32,
    pub(super) fit_ok: u32,
}

/// Ile wolnych etatów przejrzeć w dzielnicy, zanim wybierzemy najlepszy. Limit jest
/// twardy, bo bez niego krok 5 jest kwadratowy wobec wielkości dzielnicy.
const PRZEGLAD_ETATOW: usize = 24;

/// Krok 5: dopasowanie pracy (§5.9, korekta E-15).
///
/// Zachłannie po malejącym `skill_match`, ale **w granicach dzielnicy**: praca ma być
/// w zasięgu dojazdu z domu. Bez tego relacja współpracownicza spina przeciwne końce
/// miasta i plotka przeskakuje pięć kilometrów w jednym kroku (kryterium WP9).
pub(super) fn dopasuj_prace(
    world: &mut World,
    t: &PopulationTable,
    jobs: &crate::city::build::JobTable,
    mieszkancy: &[Entity],
    etaty: Vec<JobSlot>,
    unemp: u16,
    ages: Ages,
) -> Zatrudnienie {
    let wszystkie = etaty.clone();
    // Wolne etaty per dzielnica — indeks, którego `Vacancies` nie ma (korekta E-20).
    let mut wolne_w: BTreeMap<u16, Vec<u32>> = BTreeMap::new();
    for (i, j) in etaty.iter().enumerate() {
        wolne_w.entry(j.district).or_default().push(i as u32);
    }
    let mut zajety = vec![false; etaty.len()];

    // Kandydaci: aktywni zawodowo, malejąco po najlepszym dopasowaniu. Remis
    // rozstrzyga indeks encji (00 §3.2).
    let mut kandydaci: Vec<(u8, u32, Entity, u16)> = Vec::new();
    let (mut active, mut pupils) = (0u32, 0u32);
    for c in mieszkancy {
        let Some(id) = world.get::<Identity>(*c) else {
            continue;
        };
        let wiek = id.age_years(0);
        let emp = world.get::<Employment>(*c).copied().unwrap_or_default();
        if emp.flags & Employment::FLAG_PUPIL != 0 {
            pupils += 1;
            continue;
        }
        if wiek < i32::from(ages.work_start) || wiek >= i32::from(ages.retirement) {
            continue;
        }
        active += 1;
        let v = world.get::<Vitals>(*c).copied().unwrap_or_default();
        let s = world.get::<Skills>(*c).copied().unwrap_or_default();
        let najlepsze = jobs
            .roles
            .iter()
            .enumerate()
            .map(|(i, r)| skill_match(t, &r.key, &v, &s, i as u16))
            .max()
            .unwrap_or(0);
        let d = world.get::<Residence>(*c).map_or(0, |r| r.district);
        kandydaci.push((najlepsze, c.index(), *c, d));
    }
    kandydaci.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));

    // Ilu ma zostać bez pracy: dokładnie tylu, ilu każe cel bezrobocia (§5.9 krok 5).
    let limit =
        ((u64::from(active) * u64::from(1000 - unemp.min(1000)) / 1000) as usize).min(etaty.len());

    let mut employed = 0usize;
    let mut fit_ok = 0u32;
    for (_, _, c, dzielnica) in &kandydaci {
        if employed >= limit {
            break;
        }
        let v = world.get::<Vitals>(*c).copied().unwrap_or_default();
        let s = world.get::<Skills>(*c).copied().unwrap_or_default();

        // Najlepszy etat w dzielnicy zamieszkania; dopiero gdy dzielnica jest pusta,
        // szukamy gdziekolwiek — i to jest ta połowa kryterium WP9, której §5.9
        // nie wypisywało (korekta E-15).
        let wybor = wybierz_etat(&mut wolne_w, &zajety, &etaty, *dzielnica, t, jobs, &v, &s)
            .or_else(|| wybierz_gdziekolwiek(&mut wolne_w, &zajety, &etaty, t, jobs, &v, &s));
        let Some(idx) = wybor else { continue };
        zajety[idx as usize] = true;
        let j = etaty[idx as usize];

        if let Some(e) = world.get_mut::<Employment>(*c) {
            e.site = j.site;
            e.role = j.role;
            e.shift = j.shift;
            e.work_days = j.work_days;
            e.flags &= !Employment::FLAG_UNEMPLOYED;
        }
        let klucz = &jobs.roles[(j.role as usize).min(jobs.roles.len() - 1)].key;
        if skill_match(t, klucz, &v, &s, j.role) >= 40 {
            fit_ok += 1;
        }
        let hh = gospodarstwo_mieszkanca(world, *c);
        if let Some(h) = hh.and_then(|e| world.get_mut::<Household>(e)) {
            h.income_monthly = Money(h.income_monthly.get().saturating_add(j.wage_monthly.get()));
        }
        employed += 1;
    }

    let wolne: Vec<JobSlot> = etaty
        .iter()
        .enumerate()
        .filter(|(i, _)| !zajety[*i])
        .map(|(_, j)| *j)
        .collect();

    Zatrudnienie {
        wolne,
        wszystkie,
        active,
        employed: employed as u32,
        pupils,
        fit_ok,
    }
}

#[allow(clippy::too_many_arguments)]
fn wybierz_etat(
    wolne: &mut BTreeMap<u16, Vec<u32>>,
    zajety: &[bool],
    etaty: &[JobSlot],
    dzielnica: u16,
    t: &PopulationTable,
    jobs: &crate::city::build::JobTable,
    v: &Vitals,
    s: &Skills,
) -> Option<u32> {
    let lista = wolne.get_mut(&dzielnica)?;
    while lista.last().is_some_and(|i| zajety[*i as usize]) {
        lista.pop();
    }
    if lista.is_empty() {
        return None;
    }
    let od = lista.len().saturating_sub(PRZEGLAD_ETATOW);
    let (mut best, mut best_score) = (None, -1i32);
    for (k, i) in lista[od..].iter().enumerate() {
        if zajety[*i as usize] {
            continue;
        }
        let role = etaty[*i as usize].role;
        let key = &jobs.roles[(role as usize).min(jobs.roles.len() - 1)].key;
        let score = i32::from(skill_match(t, key, v, s, role));
        if score > best_score {
            best_score = score;
            best = Some((od + k, *i));
        }
    }
    let (poz, idx) = best?;
    lista.remove(poz);
    Some(idx)
}

#[allow(clippy::too_many_arguments)]
fn wybierz_gdziekolwiek(
    wolne: &mut BTreeMap<u16, Vec<u32>>,
    zajety: &[bool],
    etaty: &[JobSlot],
    t: &PopulationTable,
    jobs: &crate::city::build::JobTable,
    v: &Vitals,
    s: &Skills,
) -> Option<u32> {
    let dzielnice: Vec<u16> = wolne
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(d, _)| *d)
        .collect();
    for d in dzielnice {
        if let Some(i) = wybierz_etat(wolne, zajety, etaty, d, t, jobs, v, s) {
            return Some(i);
        }
    }
    None
}

fn gospodarstwo_mieszkanca(world: &World, c: Entity) -> Option<Entity> {
    let id = world.get::<Identity>(c)?;
    demography::household_by_index(world, id.household)
}

// ── krok 5b: szkoły ─────────────────────────────────────────────────────────────

/// Uczeń bez placówki nie jest odprowadzany i nie ma szkoły w planie dnia
/// (`household::roles` wymaga `site != NO_SITE`). Zwraca liczbę uczniów, dla których
/// miasto nie miało ani jednej szkoły w zasięgu — to jest liczba do raportu, nie
/// do wygładzenia.
/// Reguła wyboru placówki mieszka w `sim/agents::places::nearest_school`, bo ma
/// **dwóch wołających**: ten krok generatora i dobowy cykl życia (`R2-WP1`), który
/// posyła do szkoły siedmiolatka urodzonego w grze. Dwie kopie rozjechałyby się przy
/// pierwszej zmianie promieni, a rozjazd byłoby widać jako dziecko chodzące do innej
/// szkoły niż jego rówieśnik spod tego samego adresu.
pub(super) fn przypisz_szkoly(
    world: &mut World,
    mieszkancy: &[Entity],
    places: &PlaceTable,
) -> u32 {
    let mut bez = 0u32;
    for c in mieszkancy {
        let uczen = world
            .get::<Employment>(*c)
            .is_some_and(Employment::is_pupil);
        if !uczen {
            continue;
        }
        let szkola = world
            .get::<Residence>(*c)
            .and_then(home_place)
            .and_then(|dom| places.coord_of(dom))
            .and_then(|at| magnat_agents::nearest_school(places, at));
        match szkola {
            Some(klucz) => {
                if let Some(e) = world.get_mut::<Employment>(*c) {
                    e.site = klucz;
                    e.work_days = Employment::WEEKDAYS;
                    e.shift = ShiftKind::Early as u8;
                }
            }
            None => bez += 1,
        }
    }
    bez
}
