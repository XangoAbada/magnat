//! Domknięcie M10: determinizm pełnego świata i historia w kronice (M10f).
//!
//! # Po co ten plik, skoro testów hasha jest już pięć
//!
//! Bo żaden z nich nie stawia **naraz** mediów, giełdy, ubezpieczeń, związków
//! i makra. `sim/agents/tests/determinism.rs` pilnuje mieszkańców,
//! `sim/economy` i `sim/firms` mają własne testy zasobów, a `macro_lod.rs`
//! porównuje dwa dry-runy. Każdy z nich przechodziłby, gdyby któryś z zasobów
//! M10 wypadł z hasha — bo w tamtych światach go nie ma.
//!
//! Zdanie „stan M10 jest w hashu" ma więc do tej pory jedno źródło: przegląd
//! kodu. Ten plik zamienia je na pomiar i to jest cała treść WP10.16.
//!
//! Świat stawia **`game::world::stand_up`**, a nie własny `ScheduleBuilder`
//! (`K-68`): test na innym harmonogramie niż gra mierzyłby inny świat, a różnicy
//! nie widziałby nikt.
//!
//! `#[ignore]` z tego samego powodu co cała rodzina testów generujących świat:
//! najmniejsze miasto to kilka tysięcy mieszkańców i kilkanaście sekund.
//! CI uruchamia je jawnie przez `--include-ignored`.

use magnat_game::world::{population, stand_up, SessionOpts, Standing};
use magnat_game::GenWatch;
use magnat_io::{world_state_hash, StateHash};
use magnat_jobs::JobPool;
use magnat_world::{Difficulty, EconomyProfile, Epoch, Region, WorldGenParams, WorldSize};

const ZIARNO: u64 = 11;
/// Ile mieszkańców. Tyle, żeby powstały firmy, media i rynek pracy, i ani jednego
/// więcej — doba pełnej gospodarki kosztuje sekundy, a test ma mierzyć hash.
const MIESZKANCOW: u32 = 1_500;

fn params() -> WorldGenParams {
    WorldGenParams {
        seed: ZIARNO,
        size: WorldSize::Small4km,
        epoch: Epoch::Y1990,
        profile: EconomyProfile::Mixed,
        region: Region::Lowland,
        difficulty: Difficulty::Normal,
    }
}

fn swiat(threads: usize, dry_run_years: u16) -> Standing {
    let pool = JobPool::new(threads);
    let built = population::zbuduj_z_params(params(), &pool, &GenWatch::none())
        .expect("generacja")
        .expect("nie anulowano");
    stand_up(
        &built,
        ZIARNO,
        SessionOpts {
            citizens: MIESZKANCOW,
            commute_swaps: 5_000,
            economy: true,
            micro: false,
            dry_run_years,
        },
        &pool,
    )
    .expect("świat stoi")
}

/// Ciąg hashy z `minut` minut gry, próbkowany co `co`.
fn ciag_hashy(mut st: Standing, minut: u64, co: u64) -> Vec<StateHash> {
    let mut out = Vec::new();
    for t in 1..=minut {
        st.app.tick();
        if t.is_multiple_of(co) {
            out.push(world_state_hash(&st.app.world));
        }
    }
    out
}

/// WP10.16: dwa przebiegi tego samego ziarna dają ten sam ciąg hashy.
///
/// Trzy doby, a nie jedna: systemy M10 mają różne częstotliwości — fixing jest
/// dobowy, redakcja dobowa, negocjacje tygodniowe, a składka miesięczna.
/// Przebieg krótszy niż doba nie uruchomiłby połowy z nich ani razu.
#[test]
#[ignore = "generuje miasto"]
fn dwa_przebiegi_pelnego_swiata_daja_ten_sam_ciag_hashy() {
    let a = ciag_hashy(swiat(1, 0), 3 * 1_440, 480);
    let b = ciag_hashy(swiat(1, 0), 3 * 1_440, 480);
    assert!(
        !a.is_empty(),
        "test bez ani jednej próbki niczego nie mierzy"
    );
    assert_eq!(a, b, "ten sam świat, ten sam ciąg hashy");
}

/// To samo przy wielu wątkach. Osobny test, bo łapie inną klasę błędu:
/// redukcję składaną w kolejności **ukończenia** jobów zamiast w kolejności
/// indeksu chunka (00 §3.3).
#[test]
#[ignore = "generuje miasto"]
fn wiele_watkow_nie_zmienia_ciagu_hashy() {
    let a = ciag_hashy(swiat(1, 0), 2 * 1_440, 480);
    let b = ciag_hashy(swiat(4, 0), 2 * 1_440, 480);
    assert_eq!(a, b, "liczba wątków nie ma prawa zmienić wyniku");
}

/// WP10.15: historia „na sucho" dochodzi do świata gry, a nie tylko do raportu
/// w `tools/headless`.
///
/// Sprawdzamy dwie rzeczy naraz, bo jedna bez drugiej nic nie znaczy: że wpisy
/// **powstają** (inaczej filtr pochodzenia filtrowałby pustkę) i że jest ich
/// tyle, ile obiecuje ryzyko `R9` — kronika zalana osiemdziesięcioma latami
/// jest funkcją narracyjną martwą tak samo jak kronika pusta.
#[test]
#[ignore = "generuje miasto"]
fn historia_na_sucho_dochodzi_do_swiata_gry() {
    let st = swiat(1, 30);
    assert!(
        !st.history.is_empty(),
        "trzydzieści lat historii bez ani jednego wpisu kronikarskiego znaczy, \
         że próg skali w `sim/macro` odcina wszystko"
    );
    assert!(
        st.history.len() <= 400,
        "cel z `R9` to 50–200 wpisów na 80 lat; {} na 30 lat znaczy, \
         że próg skali jest za niski",
        st.history.len()
    );
    // Historia mieści się w swoim oknie: trzydzieści lat kończących się rokiem
    // startowym partii. Ostatni krok **wolno** wylądować w 1990 — to jest doba
    // zero rozgrywki, a nie rok, którego nikt nie rozegrał.
    for e in &st.history {
        assert!(
            (1960..=1990).contains(&e.year),
            "wpis z roku {} wypada poza oknem historii 1960–1990",
            e.year
        );
    }
    assert!(
        st.history.iter().any(|e| e.year < 1990),
        "cała historia w roku startowym znaczy, że krok makro nie posuwał kalendarza"
    );
}

/// Historia zmienia świat, na którym gra się zaczyna — i to jest jej sens.
///
/// Bez tego `dry_run_years` byłoby nastawą bez skutku: przechodziłaby każdy test
/// i wyglądała tak samo jak działająca (`K-67`).
#[test]
#[ignore = "generuje miasto"]
fn historia_zmienia_swiat_startowy() {
    let bez = world_state_hash(&swiat(1, 0).app.world);
    let z = world_state_hash(&swiat(1, 30).app.world);
    assert_ne!(
        bez, z,
        "trzydzieści lat historii nie ruszyło ani jednej liczby"
    );
}

/// WP10.15, audyt wpisów: **każdy wpis kroniki daje się wyrenderować w obu
/// językach**, a dwadzieścia pierwszych wypisuje się do przeglądu ręcznego.
///
/// Test jest jednocześnie bramką i przyrządem. Bramką, bo `Catalog::must`
/// panikuje na brakującym kluczu — więc wpis, którego nikt nie przetłumaczył,
/// wywraca ten test, a nie wychodzi u gracza po zmianie języka (CLAUDE.md).
/// Przyrządem, bo `-- --nocapture` wypisuje zdania, których kryterium WP10.15
/// każe przeczytać dwadzieścia i ocenić, czy są zrozumiałe dla człowieka.
#[test]
#[ignore = "generuje miasto"]
fn wpisy_kroniki_renderuja_sie_w_obu_jezykach() {
    use magnat_game::{GenWatch as _GW, NewGameParams, ScenarioId, Session, StartVariant};

    let _ = _GW::none();
    let pool = JobPool::new(0);
    let p = NewGameParams {
        world: params(),
        scenario: ScenarioId::SANDBOX,
        variant: StartVariant::default(),
        opts: SessionOpts {
            citizens: MIESZKANCOW,
            commute_swaps: 5_000,
            economy: true,
            micro: false,
            dry_run_years: 30,
        },
    };
    let built = population::zbuduj_z_params(p.world, &pool, &GenWatch::none())
        .expect("generacja")
        .expect("nie anulowano");
    let mut s = Session::begin(built, p, &pool).expect("sesja");
    // Trzy doby, żeby oprócz historii weszły zdarzenia świata i uchwały rady.
    s.step(3 * 1_440, 0);

    let c = magnat_ui::Catalog::load().expect("data/locale/");
    let wpisy: Vec<_> = s
        .chronicle()
        .query(&magnat_game::chronicle::Query::default())
        .into_iter()
        .copied()
        .collect();
    assert!(!wpisy.is_empty(), "kronika po trzech dobach jest pusta");

    for (i, e) in wpisy.iter().enumerate() {
        // `Catalog::must` panikuje na brakującym kluczu — to jest cała asercja.
        let pl = magnat_game::chronicle::text(e, &s, &c, magnat_ui::Locale::Pl);
        let en = magnat_game::chronicle::text(e, &s, &c, magnat_ui::Locale::En);
        assert!(!pl.is_empty() && !en.is_empty(), "puste zdanie wpisu {i}");
        if i < 20 {
            println!("[{:?}/{:?}] {pl}  ||  {en}", e.provenance, e.kind);
        }
    }
    println!("wpisów razem: {}", wpisy.len());
}
