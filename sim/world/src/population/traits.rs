//! Kroki 3 i 4: wykształcenie, umiejętności, osobowość i dopasowanie do roli.
//!
//! Wydzielone z `population/mod.rs` w R-WP4 bez zmiany zachowania.

use super::*;

// ── kroki 3–4: wykształcenie, umiejętności, osobowość ───────────────────────────

/// Krok 3: wykształcenie i kierunek.
fn wyksztalcenie(
    mix: &[u16],
    fields: &[u16],
    wiek: i32,
    klasa: u32,
    adult_age: i32,
    r: &mut Rng,
) -> (u8, u8) {
    if wiek < 7 {
        return (0, 0);
    }
    if wiek < 15 {
        return (1, 0);
    }
    if wiek < adult_age {
        return (u8::from(r.gen_bool_permille(300)) + 1, 0);
    }
    let mut k = r.gen_range_u32(1000);
    let mut level = 0u8;
    for (i, w) in mix.iter().enumerate() {
        if k < u32::from(*w) {
            level = i as u8;
            break;
        }
        k -= u32::from(*w);
    }
    // Klasa zalążkowa gospodarstwa podnosi wykształcenie o stopień z prawdopodobieństwem
    // rosnącym z decylem — to jest cała „f(klasa)" z §5.9, wyrażona jednym hazardem.
    if level < 4 && r.gen_bool_permille((klasa * 60).min(1000) as u16) {
        level += 1;
    }
    // Dyplom przed dwudziestym czwartym rokiem życia jest rzadki.
    if level == 4 && wiek < 24 {
        level = 3;
    }
    if level <= 1 {
        return (level, 0);
    }
    let mut k = r.gen_range_u32(1000);
    let mut field = 1u8;
    for (i, w) in fields.iter().enumerate() {
        if k < u32::from(*w) {
            field = i as u8 + 1;
            break;
        }
        k -= u32::from(*w);
    }
    (level, field)
}

/// Krok 4: osiem cech. Rozkład trójkątny wokół średniej przesuniętej wykształceniem
/// i klasą — korelacja siedzi w danych (`population.ron`), nie tutaj.
fn osobowosc(specs: &[table::TraitSpec], level: u8, klasa: u32, r: &mut Rng) -> [u8; 8] {
    std::array::from_fn(|i| {
        let s = specs[i];
        let srodek = i32::from(s.mean)
            + i32::from(s.edu_bias) * (i32::from(level) - 2)
            + i32::from(s.status_bias) * (klasa as i32 - 5) / 2;
        let rozrzut = u32::from(s.spread).max(2);
        let szum =
            (r.gen_range_u32(rozrzut) + r.gen_range_u32(rozrzut)) as i32 / 2 - (rozrzut as i32) / 2;
        (srodek + szum).clamp(0, 100) as u8
    })
}

/// Krok 3: umiejętności. 1–3 sloty, poziom rośnie ze stażem i z dopasowaniem kierunku.
fn umiejetnosci(
    t: &PopulationTable,
    jobs: &crate::city::build::JobTable,
    wiek: i32,
    level: u8,
    field: u8,
    work_start: u8,
    r: &mut Rng,
) -> Skills {
    let mut out = Skills(
        [SkillSlot {
            role: Skills::ROLE_NONE,
            level: 0,
            decay: 0,
        }; 4],
    );
    if wiek < i32::from(work_start) || jobs.roles.is_empty() {
        return out;
    }
    let staz = (wiek - i32::from(work_start)).clamp(0, 40) as u32;
    let ile = (1 + r.gen_range_u32(3)).min(jobs.roles.len() as u32).min(4);
    let start = r.gen_range_u32(jobs.roles.len() as u32);
    for k in 0..ile {
        let idx = ((start + k) % jobs.roles.len() as u32) as usize;
        let dopasowanie = u32::from(t.fit_of(&jobs.roles[idx].key, field));
        let poziom =
            (staz * 3 / 2 + dopasowanie / 3 + u32::from(level) * 4 + r.gen_range_u32(15)).min(100);
        out.0[k as usize] = SkillSlot {
            role: idx as u16,
            level: poziom as u8,
            decay: 2,
        };
    }
    out
}

/// Dopasowanie mieszkańca do roli: kierunek wykształcenia (dane), poziom wykształcenia
/// i posiadana umiejętność. 0..=100.
pub(super) fn skill_match(
    t: &PopulationTable,
    role_key: &str,
    v: &Vitals,
    s: &Skills,
    role: u16,
) -> u8 {
    let baza = u32::from(t.fit_of(role_key, v.edu_field));
    let poziom = u32::from(s.level_in(role).get());
    let edu = u32::from(v.edu_level) * 5;
    ((baza * 6 + poziom * 3 + edu * 10) / 10).min(100) as u8
}

/// Kroki 3 i 4: wykształcenie, umiejętności i osobowość całej populacji.
#[allow(clippy::too_many_arguments)]
pub(super) fn nadaj_cechy(
    world: &mut World,
    mieszkancy: &[Entity],
    t: &PopulationTable,
    jobs_table: &crate::city::build::JobTable,
    epoch: &str,
    seed: u64,
    adult_age: i32,
    ages: Ages,
) {
    let edu_mix = t.education_of(epoch).to_vec();
    for c in mieszkancy {
        let mut r = rng(seed, StreamId::PersonalityGen, c.index(), Tick(4));
        // Klasa zalążkowa: dzielnica zamieszkania mówi o zamożności adresu, reszta
        // jest losowa. Prawdziwy status policzy `social::step_month` po pierwszym
        // miesiącu — tu chodzi tylko o to, żeby wykształcenie nie było niezależne
        // od tego, gdzie ktoś się urodził.
        let klasa = (u32::from(world.get::<Residence>(*c).map_or(0, |r| r.district) % 10)
            + r.gen_range_u32(10))
            / 2;
        let wiek = world.get::<Identity>(*c).map_or(0, |id| id.age_years(0));
        let (level, field) = wyksztalcenie(&edu_mix, &t.fields, wiek, klasa, adult_age, &mut r);
        if let Some(v) = world.get_mut::<Vitals>(*c) {
            v.edu_level = level;
            v.edu_field = field;
        }
        let cechy = osobowosc(&t.traits, level, klasa, &mut r);
        if let Some(p) = world.get_mut::<Personality>(*c) {
            *p = Personality(cechy);
        }
        let skills = umiejetnosci(t, jobs_table, wiek, level, field, ages.work_start, &mut r);
        if let Some(s) = world.get_mut::<Skills>(*c) {
            *s = skills;
        }
    }
}
