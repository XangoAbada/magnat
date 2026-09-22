//! Scenariusz `century` — wynik do pokazania podfazy M3c.
//!
//! „100 lat gry headless bez wybuchu ani wygaszenia populacji." Dokładnie to i nic
//! więcej: **tryb demograficzny** (§1 dokumentu fazy) przewija dobę na krok, nie minutę
//! na krok. Nie ma tu planu dnia, kolejki zdarzeń minutowych ani ruchu pieszego — 36 000
//! dób to 51,8 mln ticków minutowych, z czego wszystkie poza 36 000 nie miałyby nic
//! do zrobienia.
//!
//! Miasto jest **syntetyczne**: lokale i etaty jako dwie płaskie listy, bez generatora
//! M2 — dokładnie ta forma, w której `Vacancies` dostanie je od Etapu 8 (M3d). Chodzi
//! o regulator populacji, a nie o urbanistykę.
//!
//! Scenariusz jest zarazem wejściem balansatora M5 (§7.2 dokumentu fazy).

use clap::Args;
use magnat_agents::{
    register, seed_population, society, CityFacts, DemographyTable, HomeSlot, JobSlot, NeedTable,
    NoInheritance, Population, SocialClass, Vacancies,
};
use magnat_core::{Money, Q};
use magnat_ecs::World;
use magnat_io::world_state_hash;

#[derive(Args, Debug)]
pub struct CenturyArgs {
    /// Ziarno świata.
    #[arg(long, default_value_t = 1)]
    pub seed: u64,

    /// Liczba lokali mieszkalnych w mieście (pojemność mieszkaniowa).
    #[arg(long, default_value_t = 4_000)]
    pub homes: u32,

    /// Liczba etatów (pojemność rynku pracy).
    #[arg(long, default_value_t = 3_000)]
    pub jobs: u32,

    /// Ile lokali obsadzić na starcie.
    #[arg(long, default_value_t = 3_000)]
    pub seeded: u32,

    /// Ile lat gry przebiec (rok = 360 dób, K-1).
    #[arg(long, default_value_t = 100)]
    pub years: u32,

    /// Co ile lat wypisać wiersz raportu.
    #[arg(long, default_value_t = 10)]
    pub report_every: u32,

    /// Eksperyment szokowy WP8: zlikwiduj ten procent etatów w roku `shock_year`.
    #[arg(long, default_value_t = 0)]
    pub shock_percent: u32,

    /// Rok, w którym uderza szok.
    #[arg(long, default_value_t = 20)]
    pub shock_year: u32,

    /// Co ile dób liczyć hash stanu (0 = nigdy).
    #[arg(long, default_value_t = 3_600)]
    pub hash_every: u64,
}

/// Jedno mieszkanie przypada na jeden budynek, jeden etat na jeden zakład — numerowanie
/// rozłączne, żeby wydruk dało się czytać i żeby indeks budynku był zarazem kluczem
/// kwartału w `CityFacts`.
const ID_DOM: u32 = 1_000_000;
const ID_PRACA: u32 = 2_000_000;
/// Ile budynków składa się na kwartał. Sąsiedztwo jest przestrzenne (§5.8), a kwartał
/// jest jego jednostką — tu przybliżoną, bo miasto nie ma geometrii.
const DOMOW_W_KWARTALE: u32 = 12;

fn zbuduj_miasto(a: &CenturyArgs) -> (Vec<HomeSlot>, Vec<JobSlot>, CityFacts) {
    let mut domy = Vec::with_capacity(a.homes as usize);
    let mut block_of = vec![0u32; (ID_DOM + a.homes) as usize];
    for i in 0..a.homes {
        let dzielnica = (i * 12 / a.homes.max(1)) as u16;
        domy.push(HomeSlot {
            building: ID_DOM + i,
            unit: 0,
            district: dzielnica,
            // Wartość rośnie z numerem dzielnicy — starcza, żeby percentyl adresu
            // w funkcji statusu (§5.8) nie był stały.
            value: Money(1_000_000 + i64::from(dzielnica) * 250_000),
        });
        block_of[(ID_DOM + i) as usize] = i / DOMOW_W_KWARTALE;
    }

    let mut etaty = Vec::with_capacity(a.jobs as usize);
    for i in 0..a.jobs {
        etaty.push(JobSlot {
            site: ID_PRACA + i % (a.jobs / 8).max(1),
            role: (i % 40) as u16,
            shift: match i % 10 {
                0 => 0, // Early
                1 => 2, // Afternoon
                2 => 3, // Night
                _ => 1, // Day
            },
            work_days: 0b001_1111,
            district: (i * 12 / a.jobs.max(1)) as u16,
            wage_monthly: Money(250_000 + i64::from(i % 50) * 8_000),
        });
    }

    let facts = CityFacts {
        job_prestige: Vec::new(),
        district_score: (0..12u16).map(|d| (20 + d * 6).min(100) as u8).collect(),
        block_of,
    };
    (domy, etaty, facts)
}

fn zbuduj_swiat(a: &CenturyArgs) -> Result<World, Box<dyn std::error::Error>> {
    let mut world = World::new(a.seed);
    // Warstwa M3a (slaby, kolejka zdarzeń, tabela potrzeb) jest potrzebna także tutaj:
    // relacje i wiedza mieszkają w slabach, a choroba obniża `Needs[Health]`.
    register(&mut world, NeedTable::load_default()?);
    society::register_society(&mut world, DemographyTable::load_default()?);

    let (domy, etaty, facts) = zbuduj_miasto(a);
    *world.resource_mut::<Vacancies>() = Vacancies::new(domy, etaty);
    *world.resource_mut::<CityFacts>() = facts;
    seed_population(&mut world, 0, a.seeded as usize);
    Ok(world)
}

pub fn run(a: &CenturyArgs) -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    let start = std::time::Instant::now();
    let mut world = zbuduj_swiat(a)?;
    let startowa = society::population(&world);
    let pieniadz_start = society::total_money(&world);
    eprintln!(
        "miasto: {} lokali, {} etatów · start {} mieszkańców w {} gospodarstwach ({:.2} s)",
        a.homes,
        a.jobs,
        startowa,
        society::households(&world),
        start.elapsed().as_secs_f64()
    );

    let mut hooks = NoInheritance;
    let doby = u64::from(a.years) * 360;
    let szok_doba = u64::from(a.shock_year) * 360;
    let (mut min_pop, mut max_pop) = (startowa, startowa);
    let mut hashe: Vec<(u64, String)> = Vec::new();
    let mut historia: Vec<(u32, usize, u16)> = Vec::new();
    let mut ostatnie_bezrobocie = 0u16;
    let bieg = std::time::Instant::now();

    println!("rok  populacja  gosp.  bezrob‰  atrakc.  pustost.  wakaty  0-17  18-64  65+");
    for doba in 0..doby {
        if a.shock_percent > 0 && doba == szok_doba {
            let ile = (a.jobs * a.shock_percent / 100) as usize;
            let zlikwidowane = magnat_agents::shock_retire_jobs(&mut world, ile);
            eprintln!(
                "szok w roku {}: zlikwidowano {zlikwidowane} z {ile} etatów; zostało {}",
                a.shock_year,
                world.resource::<Vacancies>().jobs_total()
            );
        }

        let raport = society::step_day(&mut world, doba, &mut hooks);
        if let Some(m) = &raport.migration {
            ostatnie_bezrobocie = m.unemployment_permille;
        }

        let pop = society::population(&world);
        min_pop = min_pop.min(pop);
        max_pop = max_pop.max(pop);
        if pop == 0 {
            eprintln!(
                "BŁĄD: populacja wygasła w dobie {doba} (rok {})",
                doba / 360
            );
            return Ok(std::process::ExitCode::FAILURE);
        }

        if doba % 360 == 0 {
            historia.push(((doba / 360) as u32, pop, ostatnie_bezrobocie));
            let rok = (doba / 360) as u32;
            if rok.is_multiple_of(a.report_every) {
                wiersz(
                    &world,
                    rok,
                    doba,
                    ostatnie_bezrobocie,
                    raport.migration.as_ref(),
                );
            }
        }
        if a.hash_every > 0 && doba % a.hash_every == 0 {
            hashe.push((doba, world_state_hash(&world).to_string()));
        }
    }

    let koncowa = society::population(&world);
    let czas = bieg.elapsed();
    let p = world.resource::<Population>();
    eprintln!(
        "{} lat gry w {:.2} s · urodzeń {}, zgonów {}, przyjazdów {}, wyjazdów {}",
        a.years,
        czas.as_secs_f64(),
        p.births,
        p.deaths,
        p.arrivals,
        p.departures
    );
    eprintln!(
        "populacja: start {startowa}, koniec {koncowa} ({:.2}×), minimum {min_pop}, maksimum {max_pop}",
        koncowa as f64 / startowa.max(1) as f64
    );

    piramida(&world, doby.saturating_sub(1));
    klasy(&world);
    rodzina(&world, doby.saturating_sub(1));
    if a.shock_percent > 0 {
        stabilizacja(&historia, a.shock_year);
    }

    let pieniadz_koniec = society::total_money(&world);
    if pieniadz_koniec != pieniadz_start {
        eprintln!("BŁĄD: pieniądz {pieniadz_start} → {pieniadz_koniec} (00 §6: tolerancja 0)");
        return Ok(std::process::ExitCode::FAILURE);
    }
    if !world
        .resource::<Population>()
        .identity_holds(startowa as u64)
    {
        eprintln!("BŁĄD: tożsamość księgowa populacji nie zamyka się (§7.2)");
        return Ok(std::process::ExitCode::FAILURE);
    }

    for (doba, h) in &hashe {
        println!("# hash {doba} {h}");
    }

    let stosunek = koncowa as f64 / startowa.max(1) as f64;
    if !(0.5..=2.0).contains(&stosunek) {
        eprintln!("BŁĄD: populacja poza przedziałem [0,5×, 2,0×] — {stosunek:.2}×");
        return Ok(std::process::ExitCode::FAILURE);
    }
    if let Some(blad) = przekroczony_prog(startowa, min_pop, max_pop) {
        eprintln!("BŁĄD: {blad}");
        return Ok(std::process::ExitCode::FAILURE);
    }
    Ok(std::process::ExitCode::SUCCESS)
}

/// Najmniejsza populacja, jaką `prop_century_survival` dopuszcza w **którymkolwiek**
/// momencie stulecia (M3 §7.2).
const MIN_POPULACJA: usize = 1_000;
/// Największa populacja jako wielokrotność startowej, w którymkolwiek momencie (M3 §7.2).
const MAX_KROTNOSC: usize = 3;

/// Progi §7.2 dla całego przebiegu, nie tylko dla jego końca (N1.3). Do E1 runner
/// liczył `min_pop` i `max_pop` wyłącznie do wydruku — miasto, które w 40. roku spadło
/// do 300 mieszkańców i odbiło do startowej, przechodziło bez słowa.
fn przekroczony_prog(startowa: usize, min_pop: usize, max_pop: usize) -> Option<String> {
    if min_pop < MIN_POPULACJA {
        return Some(format!(
            "populacja spadła do {min_pop} — §7.2: nigdy poniżej {MIN_POPULACJA}"
        ));
    }
    if max_pop > startowa.saturating_mul(MAX_KROTNOSC) {
        return Some(format!(
            "populacja urosła do {max_pop} przy starcie {startowa} — §7.2: nigdy ponad {MAX_KROTNOSC}× startowej"
        ));
    }
    None
}

/// Ocena eksperymentu szokowego (kryterium WP8).
///
/// „Stabilizacja bezrobocia w ≤ 5 latach" mierzy się jako **zanik nadwyżki**, a nie
/// jako powrót do dawnej liczby: po likwidacji 20 % etatów równowaga bezrobocia jest
/// z definicji inna niż przed, bo miasto ma mniej pracy. Za ustabilizowane uznajemy
/// więc bezrobocie, gdy nadwyżka ponad poziom sprzed szoku spadła poniżej 20 % skoku.
fn stabilizacja(historia: &[(u32, usize, u16)], rok_szoku: u32) {
    let Some(przed) = historia.iter().find(|(r, _, _)| *r + 1 == rok_szoku) else {
        return;
    };
    let po: Vec<&(u32, usize, u16)> = historia
        .iter()
        .filter(|(r, _, _)| *r >= rok_szoku)
        .collect();
    let Some(szczyt) = po.iter().map(|(_, _, b)| *b).max() else {
        return;
    };
    let prog = przed.2 + (szczyt.saturating_sub(przed.2)) / 5;
    let wrocilo = po
        .iter()
        .find(|(r, _, b)| *r > rok_szoku && *b <= prog)
        .map(|(r, _, _)| *r - rok_szoku);

    // Amplitudę mierzy się **po ustabilizowaniu**, nie licząc samego skoku: kryterium
    // WP8 zabrania oscylacji regulatora, a nie zabrania miastu zareagować na szok.
    let (min_pop, max_pop) = po
        .iter()
        .filter(|(r, _, _)| *r >= rok_szoku + 5)
        .fold((usize::MAX, 0usize), |(a, b), (_, p, _)| {
            (a.min(*p), b.max(*p))
        });
    let amplituda = if max_pop == 0 {
        0.0
    } else {
        (max_pop - min_pop) as f64 * 100.0 / max_pop as f64
    };

    println!(
        "
eksperyment szokowy (WP8)"
    );
    println!("  bezrobocie przed szokiem:   {} ‰", przed.2);
    println!("  szczyt po szoku:            {szczyt} ‰");
    println!("  próg zaniku nadwyżki:       {prog} ‰");
    match wrocilo {
        Some(lat) => println!("  osiągnięty po:              {lat} latach gry"),
        None => println!("  osiągnięty po:              NIE OSIĄGNIĘTY"),
    }
    println!("  amplituda populacji po 5 latach od szoku: {amplituda:.1} %");
}

fn wiersz(
    world: &World,
    rok: u32,
    doba: u64,
    bezrobocie: u16,
    migracja: Option<&magnat_agents::MigrationReport>,
) {
    let (dzieci, dorosli, seniorzy) = grupy_wieku(world, doba);
    let v = world.resource::<Vacancies>();
    println!(
        "{rok:>4} {:>10} {:>6} {bezrobocie:>8} {:>8} {:>9} {:>7} {dzieci:>5} {dorosli:>6} {seniorzy:>4}",
        society::population(world),
        society::households(world),
        migracja.map_or(0, |m| m.attractiveness),
        v.free_homes(),
        v.free_jobs(),
    );
}

fn grupy_wieku(world: &World, doba: u64) -> (u32, u32, u32) {
    let mut g = (0u32, 0u32, 0u32);
    for e in world.resource::<Population>().citizens() {
        let Some(id) = world.get::<magnat_agents::Identity>(*e) else {
            continue;
        };
        match id.age_years(doba as i32) {
            ..=17 => g.0 += 1,
            18..=64 => g.1 += 1,
            _ => g.2 += 1,
        }
    }
    g
}

/// Piramida wieku w pasmach dziesięcioletnich — sprawdzian „piramida sensowna" z §7.6.
fn piramida(world: &World, doba: u64) {
    let mut kubelki = [0u32; 12];
    let mut razem = 0u32;
    for e in world.resource::<Population>().citizens() {
        let Some(id) = world.get::<magnat_agents::Identity>(*e) else {
            continue;
        };
        let k = (id.age_years(doba as i32).max(0) / 10).min(11) as usize;
        kubelki[k] += 1;
        razem += 1;
    }
    println!("\npiramida wieku (koniec przebiegu)");
    for (i, n) in kubelki.iter().enumerate() {
        let udzial = f64::from(*n) * 100.0 / f64::from(razem.max(1));
        println!(
            "  {:>3}–{:<3} {n:>7}  {:>5.1} %  {}",
            i * 10,
            i * 10 + 9,
            udzial,
            "#".repeat((udzial * 2.0) as usize)
        );
    }
}

/// Rozkład klas — dowód, że status jest liczony, a nie przechowywany (§5.8).
/// Tabela rodziny i cyklu życia — wynik do pokazania podfazy `R2a`.
///
/// Cztery liczby, z których **trzy były przed R2 stałe albo zerowe**: uczniów nie
/// przybywało (flagę stawiał wyłącznie generator), gospodarstw osieroconych nie było
/// w kodzie w żadnym znaczeniu, a wykształcenie rocznika urodzonego w grze zostawało
/// na zerze. Czwarta — rozkład rodzajów relacji — nie miała ani `Grandparent`,
/// ani `Sibling` poza porodem.
fn rodzina(world: &World, day: u64) {
    let tabela = world.resource::<DemographyTable>();
    let ages = tabela.ages();
    let dzis = day as i32;

    let (mut uczniow, mut w_wieku_szkolnym, mut z_placowka) = (0u32, 0u32, 0u32);
    let mut edu_25: Vec<u8> = Vec::new();
    for e in world.resource::<Population>().citizens() {
        let Some(id) = world.get::<magnat_agents::Identity>(*e) else {
            continue;
        };
        let wiek = id.age_years(dzis);
        if wiek >= i32::from(ages.school_start) && wiek < i32::from(ages.school_end) {
            w_wieku_szkolnym += 1;
            if let Some(emp) = world.get::<magnat_agents::Employment>(*e) {
                if emp.is_pupil() {
                    uczniow += 1;
                    if emp.has_job() {
                        z_placowka += 1;
                    }
                }
            }
        }
        if wiek == 25 {
            if let Some(v) = world.get::<magnat_agents::Vitals>(*e) {
                edu_25.push(v.edu_level);
            }
        }
    }
    edu_25.sort_unstable();
    let mediana = edu_25.get(edu_25.len() / 2).copied().unwrap_or(0);

    let (mut osierocone, mut z_opiekunem, mut przeludnione) = (0u32, 0u32, 0u32);
    for hh in world.resource::<Population>().households() {
        let Some(h) = world.get::<magnat_agents::Household>(*hh).copied() else {
            continue;
        };
        if h.is_overcrowded() {
            przeludnione += 1;
        }
        if h.guardian_of().is_some() {
            z_opiekunem += 1;
        }
        let sklad = magnat_agents::household::members_of(
            hh.index(),
            &h,
            world.resource::<magnat_agents::HouseholdOverflow>(),
        );
        let (mut dzieci, mut dorosli) = (0u32, 0u32);
        for m in sklad.iter() {
            let Some(c) = magnat_agents::demography::citizen_by_index(world, *m) else {
                continue;
            };
            let wiek = world
                .get::<magnat_agents::Identity>(c)
                .map_or(0, |i| i.age_years(dzis));
            if wiek < i32::from(ages.adult) {
                dzieci += 1;
            } else {
                dorosli += 1;
            }
        }
        if dzieci > 0 && dorosli == 0 {
            osierocone += 1;
        }
    }

    let mut relacje = [0u32; 9];
    for e in world.resource::<Population>().citizens() {
        let Some(r) = world.get::<magnat_agents::RelationsRef>(*e).copied() else {
            continue;
        };
        for w in world
            .resource::<magnat_agents::RelationSlab>()
            .entries(magnat_agents::demography::relations_ref(&r))
        {
            relacje[usize::from(w.kind).min(8)] += 1;
        }
    }

    println!("\nrodzina i cykl życia (R2a)");
    println!(
        "  uczniowie                {uczniow:>7} / {w_wieku_szkolnym} w wieku {}–{} ({:.1} %)",
        ages.school_start,
        ages.school_end - 1,
        f64::from(uczniow) * 100.0 / f64::from(w_wieku_szkolnym.max(1))
    );
    println!(
        "    w tym z placówką       {z_placowka:>7}  — bez niej `edu_level` zostaje zerem (`K-74`)"
    );
    println!("  gospodarstwa osierocone  {osierocone:>7}  (z opiekunem: {z_opiekunem})");
    println!("  gospodarstwa przeludnione{przeludnione:>7}");
    println!(
        "  mediana edu_level (25 lat){mediana:>6}  z {} osób w kohorcie",
        edu_25.len()
    );
    println!("  rozkład relacji:");
    for (i, nazwa) in [
        "Acquaintance",
        "Partner",
        "Parent",
        "Child",
        "Sibling",
        "Friend",
        "Colleague",
        "Neighbour",
        "Grandparent",
    ]
    .into_iter()
    .enumerate()
    {
        println!("    {nazwa:<13} {:>8}", relacje[i]);
    }
}

fn klasy(world: &World) {
    let mut liczniki = [0u32; 6];
    let mut razem = 0u32;
    for e in world.resource::<Population>().citizens() {
        let Some(v) = world.get::<magnat_agents::Vitals>(*e) else {
            continue;
        };
        liczniki[SocialClass::of(Q::new(v.status)) as usize] += 1;
        razem += 1;
    }
    println!("\nklasy społeczne (przedziały statusu, §5.8)");
    for (i, n) in liczniki.iter().enumerate() {
        let klasa = [
            SocialClass::Lower,
            SocialClass::Working,
            SocialClass::LowerMiddle,
            SocialClass::UpperMiddle,
            SocialClass::Upper,
            SocialClass::Elite,
        ][i];
        println!(
            "  {:<12} {n:>7}  {:>5.1} %",
            klasa.name(),
            f64::from(*n) * 100.0 / f64::from(razem.max(1))
        );
    }
}

#[cfg(test)]
mod tests {
    use super::przekroczony_prog;

    #[test]
    fn progi_stulecia_obejmuja_caly_przebieg() {
        assert!(
            przekroczony_prog(3_000, 1_000, 9_000).is_none(),
            "na granicy przechodzi"
        );
        assert!(
            przekroczony_prog(3_000, 999, 3_000).is_some(),
            "dołek poniżej 1000"
        );
        assert!(
            przekroczony_prog(3_000, 2_000, 9_001).is_some(),
            "szczyt ponad 3×"
        );
    }
}
