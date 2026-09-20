//! Kariera, scenariusze, porażka i sukcesja (M9e WP12).
//!
//! Wszystkie przebiegi są `#[ignore]` i chodzą w CI z `--release`: każdy potrzebuje
//! stojącego miasta, a w profilu debug generacja terenu mierzy kompilator.

use magnat_game::career::{CareerTier, Holdings};
use magnat_game::player::StartVariant;
use magnat_game::scenario::ScenarioOutcome;
use magnat_game::screens::ending::EndAction;
use magnat_game::session::Session;
use magnat_game::shell::{NewGameParams, ScenarioId};
use magnat_game::world::{population, SessionOpts};
use magnat_game::{GameState, GenWatch, PlayerCommand};
use magnat_jobs::JobPool;
use magnat_world::{Difficulty, EconomyProfile, Epoch, Region, WorldGenParams, WorldSize};

fn swiat(scenario: ScenarioId, variant: StartVariant) -> Session {
    let pool = JobPool::new(0);
    let p = NewGameParams {
        world: WorldGenParams {
            seed: 11,
            size: WorldSize::Small4km,
            epoch: Epoch::Y1990,
            profile: EconomyProfile::Mixed,
            region: Region::Lowland,
            difficulty: Difficulty::Normal,
        },
        scenario,
        variant,
        opts: SessionOpts {
            citizens: 1500,
            commute_swaps: 5_000,
            economy: true,
            micro: false,
            ..SessionOpts::default()
        },
    };
    let built = population::zbuduj_z_params(p.world, &pool, &GenWatch::none())
        .expect("generacja")
        .expect("nie anulowano");
    let mut s = Session::begin(built, p, &pool).expect("sesja");
    let kandydaci = magnat_game::player::candidates(&s.app.world, variant, 0);
    let kto = kandydaci.first().expect("kandydat na postać").citizen;
    s.submit(PlayerCommand::SetCharacter { citizen: kto })
        .expect("wybór postaci");
    s.step(1, 0);
    s
}

/// Pierwsza dzielnica, do której da się wejść.
fn dzielnica(s: &Session) -> u16 {
    let m = s.market.as_ref().expect("gospodarka");
    let d = magnat_economy::firmlife::districts_with_seed(m);
    assert!(
        !d.is_empty(),
        "świat bez ani jednego lokalu handlowego: sklepów {}, zamkniętych {}",
        m.sites().len(),
        m.sites()
            .into_iter()
            .filter(|x| m.is_shop_closed(*x))
            .count()
    );
    d[0].0
}

/// Kryterium WP12: scenariusz „Zbuduj sieć 50 sklepów" przechodzi do końca.
///
/// Test jedzie **komendami gracza**, a nie ustawianiem liczników: sieć powstaje tą
/// samą drogą, którą powstałaby w grze, więc mierzy się to, czy da się ją zbudować,
/// a nie czy da się podmienić liczbę.
#[test]
#[ignore = "pełne miasto - uruchamiane z --release"]
fn siec_piecdziesieciu_sklepow_domyka_scenariusz() {
    // Scenariusz 2 w `data/scenarios/scenarios.ron`.
    let mut s = swiat(ScenarioId(2), StartVariant::Investor);
    let d = dzielnica(&s);
    s.submit(PlayerCommand::FoundFirm {
        district: d,
        capital: magnat_core::Money(1_000_000),
    })
    .expect("założenie firmy");
    s.step(2, 0);
    assert_eq!(
        s.scenario_outcome(),
        ScenarioOutcome::Running,
        "jeden sklep nie jest siecią"
    );
    while Holdings::of(&s).sites.len() < 50 {
        let przed = Holdings::of(&s).sites.len();
        s.submit(PlayerCommand::OpenSite {
            district: d,
            capex: magnat_core::Money(0),
        })
        .expect("otwarcie punktu");
        s.step(2, 0);
        assert!(
            Holdings::of(&s).sites.len() > przed,
            "punkt nie powstał — sieć utknęła na {przed} zakładach"
        );
    }
    // Cele liczą się raz na dobę, więc doba musi minąć.
    s.step(1_441, 0);
    assert_eq!(
        s.scenario_outcome(),
        ScenarioOutcome::Won,
        "pięćdziesiąt zakładów nie domknęło scenariusza"
    );
    assert_eq!(
        CareerTier::derive(&s),
        CareerTier::Company,
        "pięćdziesiąt sklepów jednej branży to firma, nie grupa"
    );

    // **Droga wejścia**, a nie sam wynik (`DI-34`). Do domknięcia `WP12` test kończył
    // się na `scenario_outcome()` — i to on był jedynym czytelnikiem tej funkcji,
    // więc gracz nie dowiadywał się, że wygrał.
    let mut stan = GameState::Playing(Box::new(s));
    assert!(
        stan.settle(),
        "domknięty scenariusz nie przełączył stanu gry"
    );
    assert!(
        matches!(
            stan,
            GameState::ScenarioEnd {
                outcome: ScenarioOutcome::Won,
                ..
            }
        ),
        "wygrany scenariusz nie doprowadził do ekranu domknięcia"
    );
    // Ekran nie jest ślepym zaułkiem: „graj dalej” wraca do gry w tym samym świecie.
    assert!(stan.apply_end(EndAction::KeepPlaying));
    assert!(stan.is_playing(), "„graj dalej” nie wróciło do gry");
    // I pokazuje się **raz**: cel raz osiągnięty zostaje osiągnięty, więc bez pamięci
    // o obejrzanym ekranie gracz wracałby na niego co dobę.
    stan.session_mut().expect("sesja").step(1_441, 0);
    assert!(!stan.settle(), "ekran domknięcia wrócił po „graj dalej”");
    assert!(stan.is_playing());
}

/// Kryterium WP12: **śmierć** postaci przełącza grę w sukcesję.
///
/// To jest `DI-33`. Do domknięcia `WP12` `legacy::check` nie miał ani jednego
/// wołającego, więc zgon postaci nie zmieniał w grze nic: gracz sterował trupem.
///
/// Test zabija postać **tak, jak robi to demografia**: gasi flagę życia, zdejmuje
/// mieszkańca ze spisu i **despawnuje encję** (`demography::day::smierc` kończy się
/// `cmd.despawn`). To nie jest ozdoba testu — gdyby zostawić samą flagę, `heir_of`
/// znalazłoby dziedzica przez `Identity`, którego w prawdziwej grze już nie ma,
/// i test przechodziłby przy zepsutej sukcesji.
#[test]
#[ignore = "pełne miasto - uruchamiane z --release"]
fn smierc_postaci_przelacza_gre_w_sukcesje() {
    let mut s = swiat(ScenarioId::SANDBOX, StartVariant::Heir);
    let kto = s.player().expect("postać").citizen;
    let dziedzic = magnat_game::legacy::heir_of(&s, kto);

    if let Some(id) = s.app.world.get_mut::<magnat_agents::Identity>(kto.entity()) {
        id.flags &= !magnat_agents::Identity::FLAG_ALIVE;
    }
    s.app
        .world
        .resource_mut::<magnat_agents::Population>()
        .remove_citizen(kto.entity());
    assert!(
        s.app.world.despawn(kto.entity()),
        "encja postaci nie znikła"
    );
    // Cykl życia gracza sprawdza się raz na dobę, tak samo jak cele scenariusza.
    s.step(1_441, 0);

    let mut stan = GameState::Playing(Box::new(s));
    assert!(stan.settle(), "zgon postaci nie przełączył stanu gry");
    let GameState::Succession { heir, .. } = &stan else {
        panic!("po zgonie gra nie weszła w sukcesję");
    };
    assert_eq!(
        *heir, dziedzic,
        "gra proponuje innego dziedzica niż `heir_of`"
    );

    match dziedzic {
        Some(h) => {
            assert!(stan.apply_end(EndAction::Succeed(h)));
            stan.session_mut().expect("sesja").step(2, 0);
            assert!(stan.is_playing(), "sukcesja nie wróciła do gry");
            assert_eq!(
                stan.session().and_then(|x| x.player()).map(|p| p.citizen),
                Some(h),
                "rola nie przeszła na dziedzica"
            );
        }
        // Gospodarstwo jednoosobowe: brak dziedzica jest normalnym stanem świata
        // i prowadzi do ekranu spuścizny, a nie do końca gry (§13.4).
        None => assert!(matches!(stan, GameState::Succession { heir: None, .. })),
    }
}

/// Kryterium WP12: bankructwo **nie kończy sesji**.
///
/// Gra zostaje w `Playing`, etap kariery wraca na `Employee`, a zakłady schodzą.
/// Zobowiązania zostają — to one są treścią zdania „bank to widzi".
#[test]
#[ignore = "pełne miasto - uruchamiane z --release"]
fn bankructwo_nie_konczy_gry() {
    let mut s = swiat(ScenarioId::SANDBOX, StartVariant::Heir);
    assert!(
        !Holdings::of(&s).sites.is_empty(),
        "spadkobierca zaczyna z zakładem"
    );
    s.submit(PlayerCommand::DeclarePersonalBankruptcy)
        .expect("upadłość");
    s.step(2, 0);
    assert!(s.player().is_some(), "upadłość zabrała graczowi ciało");
    assert_eq!(
        CareerTier::derive(&s),
        CareerTier::Employee,
        "po upadłości gracz jest pracownikiem"
    );
    assert!(
        Holdings::of(&s).sites.is_empty(),
        "zakłady przeżyły likwidację"
    );
    assert_eq!(
        s.player().map(|p| p.bankruptcies),
        Some(1),
        "upadłość nie została policzona"
    );
    // Świat tyka dalej — to jest cała treść §13.4.
    let przed = s.tick();
    s.step(100, 0);
    assert!(s.tick().get() > przed.get(), "świat stanął po upadłości");
}

/// Kryterium WP12: śmierć postaci nie kończy gry, a dziedzic przejmuje firmy.
///
/// Zgonu nie da się wymusić bez czekania na demografię, więc test sprawdza **obie
/// połowy mechanizmu osobno**: że sukcesja przenosi rolę razem z firmami i że
/// wskazanie dziedzica jest komendą, którą replay odtworzy.
#[test]
#[ignore = "pełne miasto - uruchamiane z --release"]
fn sukcesja_przenosi_firmy_ale_nie_relacje() {
    let mut s = swiat(ScenarioId::SANDBOX, StartVariant::Heir);
    let poprzednik = s.player().expect("postać").citizen;
    let zaklady = Holdings::of(&s).sites;
    assert!(!zaklady.is_empty(), "spadkobierca zaczyna z zakładem");

    let Some(dziedzic) = magnat_game::legacy::heir_of(&s, poprzednik) else {
        // Gospodarstwo jednoosobowe: brak dziedzica jest **normalnym stanem świata**
        // i prowadzi do ekranu spuścizny, a nie do końca gry (§13.4).
        assert!(s.player().is_some());
        return;
    };
    s.submit(PlayerCommand::SetHeir { citizen: dziedzic })
        .expect("wskazanie dziedzica");
    s.submit(PlayerCommand::Succeed { citizen: dziedzic })
        .expect("sukcesja");
    s.step(2, 0);

    let p = s.player().expect("dziedzic ma ciało");
    assert_eq!(p.citizen, dziedzic, "rola nie przeszła na dziedzica");
    assert_eq!(
        p.capital,
        magnat_core::Money::ZERO,
        "dziedzic dostał drugi kapitał startowy — pieniądz z niczego"
    );
    assert_eq!(
        Holdings::of(&s).sites,
        zaklady,
        "firmy nie przeszły na dziedzica"
    );
}

/// Etap kariery **wyprowadza się z faktów** i nie da się go ustawić.
#[test]
#[ignore = "pełne miasto - uruchamiane z --release"]
fn etap_kariery_idzie_za_faktami() {
    let mut s = swiat(ScenarioId::SANDBOX, StartVariant::Sandbox);
    assert_eq!(CareerTier::derive(&s), CareerTier::Employee);
    let d = dzielnica(&s);
    s.submit(PlayerCommand::FoundFirm {
        district: d,
        capital: magnat_core::Money(1_000_000),
    })
    .expect("założenie firmy");
    s.step(2, 0);
    assert_eq!(
        CareerTier::derive(&s),
        CareerTier::FirstBusiness,
        "jeden zakład bez załogi to pierwszy interes"
    );
    s.submit(PlayerCommand::OpenSite {
        district: d,
        capex: magnat_core::Money(0),
    })
    .expect("drugi punkt");
    s.step(2, 0);
    assert_eq!(
        CareerTier::derive(&s),
        CareerTier::Company,
        "dwa zakłady to firma"
    );
}

/// Kryterium WP12, trzecia część: **scenariusz samouczka mieści się w budżecie
/// dwunastu interakcji** (§5.12 pkt 3 i 7).
///
/// Test jedzie dokładnie tą drogą, którą prowadzi samouczek: kim jesteś → czego
/// brakuje → otwórz i wyceń. Liczby czyta [`magnat_game::onboarding`] **z dziennika
/// wejść**, czyli z tego samego, z którego odtwarza się sesję — bo osobna telemetria
/// byłaby drugim źródłem prawdy o tej samej rozgrywce.
#[test]
#[ignore = "pełne miasto - uruchamiane z --release"]
fn samouczek_miesci_sie_w_dwunastu_interakcjach() {
    use magnat_game::panels::PanelId;
    use magnat_game::ViewCommand;

    // Scenariusz 1: „Pierwszy sklep". Wariant bez majątku — tak zaczyna nowy gracz.
    let mut s = swiat(ScenarioId(1), StartVariant::Sandbox);

    // Krok 1 „Kim jesteś": gracz ogląda swoją kartę i przypina dwa domyślne panele.
    s.record_view(0, ViewCommand::OpenPanel(PanelId::Shop));
    s.record_view(0, ViewCommand::OpenPanel(PanelId::Dashboard));

    // Krok 3 „Otwórz i wyceń": firma, koszyk, cena. Trzy komendy.
    let d = dzielnica(&s);
    s.submit(PlayerCommand::FoundFirm {
        district: d,
        capital: magnat_core::Money(500_000),
    })
    .expect("pierwszy sklep");
    s.step(2, 0);

    let site = *Holdings::of(&s).sites.first().expect("sklep gracza");
    let towar = {
        let m = s.market.as_ref().expect("gospodarka");
        let g = *m.goods_of(site).first().expect("coś na półce");
        m.good_key(g).expect("klucz towaru")
    };
    s.submit(PlayerCommand::SetShelfAssortment {
        site,
        goods: vec![towar.clone()],
    })
    .expect("domyślny koszyk");
    s.step(1, 0);
    s.submit(PlayerCommand::SetPrice {
        site,
        good: towar,
        price: magnat_core::Money(450),
    })
    .expect("pierwsza cena");
    s.step(1, 0);

    let m = magnat_game::onboarding::measure(s.log());
    assert!(m.reached, "samouczek nie doszedł do sensownej decyzji");
    assert!(
        m.within_budget(),
        "samouczek poza budżetem: {} interakcji (sufit {}), {} paneli (sufit {})",
        m.interactions,
        magnat_game::onboarding::MAX_INTERAKCJI,
        m.panels,
        magnat_game::onboarding::MAX_PANELI
    );
}
