//! Nauka i dryf umiejętności mieszkańca (M3d §5.12, M8d WP7).
//!
//! Wyprowadzone z `systems.rs` przy M8d, bo szkoła zrobiła z jednego systemu
//! **drugi temat**: do tej pory umiejętność rosła wyłącznie przez pracę w roli,
//! a teraz rośnie także przez chodzenie do szkoły — i tempo bierze się z pokrycia
//! edukacyjnego dzielnicy, czyli z warstwy, o której reszta `systems.rs` nie wie.
//! Podział jest mechaniczny: przeniesienie symboli bez zmiany zachowania.

use magnat_core::{Cadence, DistrictId, Entity, ServiceCoverage};
use magnat_ecs::{System, SystemCtx, SystemDesc, World};

use crate::components::{Employment, Residence, Skills};

/// Ile shardów ma doba tygodniowa: system dotyka 1/7 populacji na dobę.
pub const WEEK_SHARDS: u32 = 7;

/// Wzrost umiejętności przez pracę i zanik przez nieużywanie (§5.12),
/// a od M8d także **przez szkołę** (M8d WP7, PRD §10.3).
///
/// Do M8d dziecko nie uczyło się niczego: `Skills::default()` daje cztery sloty
/// z `ROLE_NONE`, a ten system rósł wyłącznie w roli zgodnej z etatem. Uczeń był
/// stanem (`Employment::FLAG_PUPIL`, szkoła w `Employment.site`), który nie miał
/// żadnego skutku. Szkoła wypełnia **slot zerowy** — wykształcenie ogólne — i to
/// ono jest tym, z czym osiemnastolatek wychodzi na rynek pracy.
///
/// Tempo jest funkcją pokrycia edukacyjnego dzielnicy, a nie stałą: szkoła
/// niedofinansowana i przepełniona uczy wolniej, i to jest cała treść skutku.
pub struct SkillDriftSystem {
    desc: SystemDesc,
}

impl SkillDriftSystem {
    /// Ile „punktotygodni" kosztuje punkt wykształcenia przy pełnym pokryciu.
    ///
    /// 400 / jakość = liczba tygodni na punkt, więc przy jakości 100 punkt wypada co
    /// cztery tygodnie (≈13 na rok gry), a przy 50 — co osiem. Jedenaście lat szkoły
    /// przy pełnym pokryciu dobija do sufitu skali, przy połowicznym kończy w okolicy
    /// siedemdziesiątki, a różnica przekracza 8 punktów `Q` z kryterium T4 już w drugim
    /// roku. Liczba jest kalibracją i mieszka tutaj, bo nie ma drugiego czytelnika.
    const SCHOOL_WEEKS_PER_POINT: u32 = 400;

    #[must_use]
    pub fn new(world: &World) -> SkillDriftSystem {
        SkillDriftSystem {
            desc: SystemDesc::new("agents.SkillDrift", Cadence::EveryDay).with_query::<(
                Entity,
                &Employment,
                &Residence,
                &mut Skills,
            ), ()>(world),
        }
    }
}

impl System for SkillDriftSystem {
    fn desc(&self) -> &SystemDesc {
        &self.desc
    }

    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let doba = ctx.tick.0 / 1440;
        // Pokrycie edukacyjne jest **kopiowane przed zapytaniem**: zapytanie pożycza
        // świat, a tablica jest mała (dzielnica × rodzaj usługi) i zmienia się raz
        // w miesiącu. Świat bez miasta jako aktora jej nie ma i wtedy szkoła nie uczy
        // — to jest scenariusz sprzed M8d, a nie awaria.
        let pokrycie = ctx.world().get_resource::<ServiceCoverage>().cloned();
        let pool = ctx.pool;
        ctx.query::<(Entity, &Employment, &Residence, &mut Skills), ()>()
            .par_for_each(pool, |(e, emp, res, skills)| {
                krok_umiejetnosci(e, emp, res, skills, doba, pokrycie.as_ref());
            });
    }
}

/// Tydzień jednego mieszkańca: nauka w szkole albo praca i zanik.
///
/// Wolna funkcja, bo mają ją dwa wołania i **jedną regułę**: system ECS puszcza ją
/// równolegle po shardzie doby, a [`skill_drift_day`] szeregowo — testowi A/B
/// i scenariuszowi bez pełnego harmonogramu nie opłaca się stawiać puli wątków
/// dla jednego systemu. Dwie kopie tej reguły rozjechałyby się przy pierwszej zmianie.
fn krok_umiejetnosci(
    e: Entity,
    emp: &Employment,
    res: &Residence,
    skills: &mut Skills,
    doba: u64,
    pokrycie: Option<&ServiceCoverage>,
) {
    let shard = (doba % u64::from(WEEK_SHARDS)) as u32;
    if e.index() % WEEK_SHARDS != shard {
        return;
    }
    let tydzien = doba / u64::from(WEEK_SHARDS);
    // Uczeń: rośnie slot zerowy, w tempie z pokrycia jego dzielnicy.
    if emp.flags & Employment::FLAG_PUPIL != 0 {
        let jakosc = pokrycie.map_or(0, |c| {
            c.at(DistrictId(res.district), magnat_core::ServiceKind::School)
                .get()
        });
        if jakosc > 0 {
            let co_ile = (SkillDriftSystem::SCHOOL_WEEKS_PER_POINT / u32::from(jakosc)).max(1);
            if tydzien.is_multiple_of(u64::from(co_ile)) {
                skills.0[0].level = skills.0[0].level.saturating_add(1).min(100);
            }
        }
        return;
    }
    for s in &mut skills.0 {
        if s.role == Skills::ROLE_NONE {
            continue;
        }
        // Tydzień pracy w roli podnosi ją o punkt; tydzień bez niej odbiera tyle,
        // ile mówi `decay` (setne punktu na dobę × 7).
        if emp.has_job() && emp.role == s.role {
            s.level = s.level.saturating_add(1).min(100);
        } else {
            let ubytek = (u32::from(s.decay) * 7 / 100).max(1) as u8;
            s.level = s.level.saturating_sub(ubytek);
        }
    }
}

/// Doba dryfu umiejętności bez harmonogramu — dla testów i scenariuszy, które
/// stawiają wycinek symulacji zamiast całego świata.
pub fn skill_drift_day(world: &mut World, day: u64) {
    let pokrycie = world.get_resource::<ServiceCoverage>().cloned();
    for (e, emp, res, skills) in world
        .query::<(Entity, &Employment, &Residence, &mut Skills), ()>()
        .iter()
    {
        krok_umiejetnosci(e, emp, res, skills, day, pokrycie.as_ref());
    }
}
