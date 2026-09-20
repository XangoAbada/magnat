//! Testy podfazy M9a: dziennik wejść, odtworzenie sesji, sloty i anulowanie.
//!
//! Przebieg z prawdziwym miastem jest `#[ignore]` i chodzi w CI z `--release`
//! razem z pozostałymi testami integracyjnymi miasta — w profilu debug generacja
//! terenu 4 km trwa minuty, a mierzy wtedy kompilator, nie kod.

use magnat_core::Money;
use magnat_game::save::{read_header, write_slot, SaveError, SaveSlot, SAVE_SCHEMA_VERSION};
use magnat_game::session::{replay, Session};
use magnat_game::shell::{NewGameParams, ScenarioId, StartVariant, WorldGenJob};
use magnat_game::world::{population, SessionOpts};
use magnat_game::{GenWatch, PlayerCommand, ReplayError, ReplayLog};
use magnat_jobs::JobPool;
use magnat_world::{Difficulty, EconomyProfile, Epoch, Region, WorldGenParams, WorldSize};

fn params() -> NewGameParams {
    NewGameParams {
        world: WorldGenParams {
            seed: 7,
            size: WorldSize::Small4km,
            epoch: Epoch::Y1990,
            profile: EconomyProfile::Mixed,
            region: Region::Lowland,
            difficulty: Difficulty::Normal,
        },
        scenario: ScenarioId::SANDBOX,
        variant: StartVariant::Worker,
        opts: SessionOpts {
            citizens: 1500,
            commute_swaps: 5_000,
            economy: true,
            micro: false,
            ..SessionOpts::default()
        },
    }
}

/// Kryterium WP2: nagrana sesja odtworzona daje **ten sam łańcuch hashy stanu**,
/// a komendy odrzucone odrzucają się identycznie.
#[test]
#[ignore = "pełne miasto — uruchamiane z --release"]
fn sesja_odtwarza_sie_z_dziennika() {
    let pool = JobPool::new(0);
    let p = params();
    let built = population::zbuduj_z_params(p.world, &pool, &GenWatch::none())
        .expect("generacja")
        .expect("nie anulowano");
    let mut s = Session::begin(built, p, &pool).expect("sesja");

    let mut hashe = s.run_hashing(720, 240);
    let market = s.market.clone().expect("gospodarka włączona");
    let sklep = market.sites()[0];
    let towar = market
        .shelf_snapshot()
        .iter()
        .find(|x| x.site == sklep)
        .and_then(|x| market.good_key(x.good))
        .expect("sklep ma choć jeden towar");

    // Jedna poprawna, jedna z nieistniejącym towarem, jedna z ceną zero.
    assert!(s
        .submit(PlayerCommand::SetPrice {
            site: sklep,
            good: towar.clone(),
            price: Money(199),
        })
        .is_ok());
    assert!(s
        .submit(PlayerCommand::SetPrice {
            site: sklep,
            good: "nie_ma_takiego".to_string(),
            price: Money(199),
        })
        .is_err());
    assert!(s
        .submit(PlayerCommand::SetPrice {
            site: sklep,
            good: towar,
            price: Money(0),
        })
        .is_err());

    hashe.extend(s.run_hashing(720, 240));
    assert_eq!(s.log().commands.len(), 4, "StartGame + trzy komendy gracza");
    assert_eq!(s.log().rejected.len(), 2, "dwie odrzucone przy stosowaniu");

    let dziennik = s.log().clone();
    let koniec = s.state_hash();
    drop(s);

    let (odtworzona, hashe2) = replay(&dziennik, 1440, 240, &pool).expect("odtworzenie");
    assert_eq!(hashe, hashe2, "łańcuch hashy musi być identyczny");
    assert_eq!(koniec, odtworzona.state_hash());
    assert_eq!(odtworzona.log().rejected.len(), 2);
}

/// Komenda gracza ma **zmieniać stan świata** — inaczej test odtworzenia jest
/// spełniony tożsamościowo i nie sprawdza niczego.
#[test]
#[ignore = "pełne miasto — uruchamiane z --release"]
fn komenda_gracza_zmienia_hash_stanu() {
    let pool = JobPool::new(0);
    let p = params();

    let hash_z_komenda = {
        let built = population::zbuduj_z_params(p.world, &pool, &GenWatch::none())
            .unwrap()
            .unwrap();
        let mut s = Session::begin(built, p, &pool).unwrap();
        s.run_hashing(720, 0);
        let m = s.market.clone().unwrap();
        let sklep = m.sites()[0];
        let towar = m
            .shelf_snapshot()
            .iter()
            .find(|x| x.site == sklep)
            .and_then(|x| m.good_key(x.good))
            .unwrap();
        s.submit(PlayerCommand::SetPrice {
            site: sklep,
            good: towar,
            price: Money(999),
        })
        .unwrap();
        s.run_hashing(720, 0);
        s.state_hash()
    };

    let hash_bez = {
        let built = population::zbuduj_z_params(p.world, &pool, &GenWatch::none())
            .unwrap()
            .unwrap();
        let mut s = Session::begin(built, p, &pool).unwrap();
        s.run_hashing(1440, 0);
        s.state_hash()
    };

    assert_ne!(
        hash_z_komenda, hash_bez,
        "cena ustawiona przez gracza nie ruszyła stanu świata"
    );
}

/// Anulowanie generacji wraca bez świata i **bez wątku za sobą**.
#[test]
#[ignore = "generacja terenu — uruchamiane z --release"]
fn anulowana_generacja_nie_zostawia_watku() {
    for _ in 0..20 {
        let job = WorldGenJob::start(params().world, 0);
        job.cancel();
        let out = job.join().expect("anulowanie nie jest błędem");
        assert!(out.is_none(), "anulowana generacja nie oddaje świata");
    }
}

#[test]
fn slot_w_starszej_wersji_daje_opisany_blad() {
    let dir = std::env::temp_dir().join(format!("magnat-m9a-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let slot = SaveSlot {
        id: 3,
        city: "Dąbrowa".to_string(),
        game_date: magnat_core::SimMinute(1440),
        net_worth: Money(0),
        played_secs: 61,
        world: params().world,
        schema_version: SAVE_SCHEMA_VERSION,
        saved_at_wall: 0,
    };
    let log = ReplayLog::new(params());
    write_slot(&dir, &slot, &log).expect("zapis slotu");
    assert_eq!(read_header(&dir, 3).expect("odczyt").city, "Dąbrowa");

    // Podmiana wersji w pliku — tak wygląda zapis z innej wersji gry.
    let p = magnat_game::save::meta_path(&dir, 3);
    let tresc = std::fs::read_to_string(&p).unwrap().replace(
        &format!("schema_version: {SAVE_SCHEMA_VERSION}"),
        "schema_version: 0",
    );
    std::fs::write(&p, tresc).unwrap();
    match read_header(&dir, 3) {
        Err(SaveError::SchemaTooOld { found, supported }) => {
            assert_eq!((found, supported), (0, SAVE_SCHEMA_VERSION));
        }
        inne => panic!("oczekiwano SchemaTooOld, jest {inne:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dziennik_w_nieznanej_wersji_nie_panikuje() {
    let log = ReplayLog::new(params());
    let tekst = ron::ser::to_string_pretty(&log, ron::ser::PrettyConfig::default()).unwrap();
    let starszy = tekst.replace("schema_version: 1", "schema_version: 0");
    match ReplayLog::parse_ron(&starszy) {
        Err(ReplayError::SchemaTooOld { found, .. }) => assert_eq!(found, 0),
        inne => panic!("oczekiwano SchemaTooOld, jest {inne:?}"),
    }
    assert!(ReplayLog::parse_ron(&tekst).is_ok());
}

/// Koperta `StartGame` niesie **komplet** parametrów świata, nie samo ziarno
/// (`Z-3`): dziennik opisujący inny świat niż nagłówek ma zostać odrzucony.
#[test]
fn dziennik_z_rozjechanymi_parametrami_jest_odrzucony() {
    let pool = JobPool::new(1);
    let mut log = ReplayLog::new(params());
    let mut inny = params().world;
    inny.size = WorldSize::Medium8km;
    log.commands.push(magnat_game::CommandEnvelope {
        seq: 0,
        tick: magnat_core::Tick(0),
        actor: magnat_game::PlayerId(0),
        cmd: PlayerCommand::StartGame {
            world: inny,
            scenario: ScenarioId::SANDBOX,
            variant: StartVariant::Worker,
            pick: None,
        },
    });
    let wynik = replay(&log, 0, 0, &pool);
    assert!(
        matches!(wynik, Err(magnat_game::ReplayMismatch::ParamsDiffer)),
        "rozjazd parametrów musi być błędem, a nie cichym odtworzeniem innego świata"
    );
}
