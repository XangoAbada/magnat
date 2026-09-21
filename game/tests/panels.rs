//! Testy podfazy M9e: panele (WP10), czas i kronika (WP11), kariera (WP12).
//!
//! Przebiegi z prawdziwym miastem są `#[ignore]` i chodzą w CI z `--release`, tak
//! samo jak testy M9c — w profilu debug generacja terenu mierzy kompilator, a nie kod.

use magnat_core::{Money, SiteId, Subject};
use magnat_game::career::{CareerTier, Holdings};
use magnat_game::panels::{PanelAction, PanelCtx, PanelId, Panels};
use magnat_game::player::StartVariant;
use magnat_game::session::Session;
use magnat_game::shell::{NewGameParams, ScenarioId};
use magnat_game::world::{population, SessionOpts};
use magnat_game::{GenWatch, PlayerCommand};
use magnat_jobs::JobPool;
use magnat_ui::{Catalog, Locale, Theme};
use magnat_world::{Difficulty, EconomyProfile, Epoch, Region, WorldGenParams, WorldSize};

fn params(scenario: ScenarioId, variant: StartVariant) -> NewGameParams {
    NewGameParams {
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
    }
}

/// Świat z postacią gracza, która **coś prowadzi**: wariant `Heir` dostaje pierwszy
/// zakład miasta, więc panele mają o czym mówić od pierwszej doby.
fn swiat(scenario: ScenarioId) -> Session {
    swiat_wariantu(scenario, StartVariant::Heir)
}

/// Świat z postacią wskazanego wariantu, przewinięty o **dobę gry**.
///
/// Doba, a nie minuta, i to nie jest ostrożność: rynek pracy rejestruje szukających
/// raz na dobę, a bez nich panel Ludzie nie ma kogo zaproponować. Test na świecie
/// młodszym niż doba mierzyłby to, że gospodarka jeszcze nie ruszyła.
fn swiat_wariantu(scenario: ScenarioId, variant: StartVariant) -> Session {
    let pool = JobPool::new(0);
    let p = params(scenario, variant);
    let built = population::zbuduj_z_params(p.world, &pool, &GenWatch::none())
        .expect("generacja")
        .expect("nie anulowano");
    let mut s = Session::begin(built, p, &pool).expect("sesja");
    let kandydaci = magnat_game::player::candidates(&s.app.world, variant, 0);
    let kto = kandydaci.first().expect("kandydat na postać").citizen;
    s.submit(PlayerCommand::SetCharacter { citizen: kto })
        .expect("wybór postaci");
    s.step(1_441, 0);
    s
}

fn theme() -> Theme {
    Theme::load().expect("data/ui/theme.ron")
}

fn katalog() -> Catalog {
    Catalog::load().expect("data/locale/")
}

/// Rysuje panel bez GPU i zwraca każdy napis, który trafił na ekran.
fn rysuj(
    panele: &mut Panels,
    s: &Session,
    c: &Catalog,
    th: &Theme,
    l: Locale,
    id: PanelId,
) -> Vec<String> {
    let h = Holdings::of(s);
    let ctx = PanelCtx {
        theme: th,
        c,
        l,
        session: s,
        holdings: &h,
        sel: 0,
    };
    magnat_ui::testing::draw(|ui| {
        panele.draw_body(ui, &ctx, id);
    })
}

/// Klika po siatce punktów doku, aż panel odda komendę.
///
/// Brzydkie i skuteczne: kryterium WP10 mówi „**da się wydać** komendę", a nie
/// „istnieje przycisk". Sprawdzenie obecności etykiety przeszłoby dla przycisku,
/// którego nikt nie podpiął — czyli dokładnie dla tego, przed czym broni `K-67`.
fn klikaj_az_wyda(
    panele: &mut Panels,
    s: &Session,
    c: &Catalog,
    th: &Theme,
    id: PanelId,
) -> Option<PlayerCommand> {
    let h = Holdings::of(s);
    let ctx = PanelCtx {
        theme: th,
        c,
        l: Locale::Pl,
        session: s,
        holdings: &h,
        sel: 0,
    };
    let egui_ctx = egui::Context::default();
    // Pierwsza klatka układa widgety — bez niej `clicked()` nie ma czego trafić.
    let _ = egui_ctx.run_ui(wejscie_bez_myszy(), |ectx| {
        egui::CentralPanel::default().show(ectx, |ui| {
            panele.draw_body(ui, &ctx, id);
        });
    });
    let mut y = 8.0;
    while y < WYSOKOSC {
        let mut x = 8.0;
        while x < SZEROKOSC_DOKU {
            let mut wynik = None;
            let _ = egui_ctx.run_ui(wejscie_z_klikiem(x, y), |ectx| {
                egui::CentralPanel::default().show(ectx, |ui| {
                    if let PanelAction::Command(cmd) = panele.draw_body(ui, &ctx, id) {
                        wynik = Some(*cmd);
                    }
                });
            });
            if wynik.is_some() {
                return wynik;
            }
            x += KROK_PX;
        }
        y += KROK_PX;
    }
    None
}

const SZEROKOSC_DOKU: f32 = 320.0;
const WYSOKOSC: f32 = 900.0;
/// Krok siatki klikania. Osiem punktów, bo najniższy przycisk motywu ma 24 px
/// wysokości i 80 px szerokości — siatka gęstsza niczego nie znajdzie więcej.
const KROK_PX: f32 = 8.0;

fn ekran() -> egui::Rect {
    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(SZEROKOSC_DOKU, WYSOKOSC))
}

fn wejscie_bez_myszy() -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(ekran()),
        ..egui::RawInput::default()
    }
}

fn wejscie_z_klikiem(x: f32, y: f32) -> egui::RawInput {
    let pos = egui::pos2(x, y);
    egui::RawInput {
        screen_rect: Some(ekran()),
        events: vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            },
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            },
        ],
        ..egui::RawInput::default()
    }
}

/// Kryterium WP10, pierwsza połowa: każdy panel rysuje się w obu językach, bez GPU
/// i bez niepodstawionego parametru.
#[test]
#[ignore = "pełne miasto — uruchamiane z --release"]
fn kazdy_panel_rysuje_sie_w_obu_jezykach_bez_gpu() {
    let s = swiat(ScenarioId::SANDBOX);
    let (c, th) = (katalog(), theme());
    let mut panele = Panels::default();
    for id in panele.reg.ids() {
        for l in [Locale::Pl, Locale::En] {
            let napisy = rysuj(&mut panele, &s, &c, &th, l, id);
            assert!(
                !napisy.is_empty(),
                "{l:?} {id:?}: panel nie narysował ani jednego napisu"
            );
            for n in &napisy {
                assert!(
                    !n.contains('{'),
                    "{l:?} {id:?}: niepodstawiony parametr w `{n}`"
                );
            }
        }
    }
}

/// Kryterium WP10, druga połowa i rozstrzygnięcie `DH-3`: **z każdego panelu
/// operacyjnego** da się wydać komendę, a kronika jawnie tego nie obiecuje.
#[test]
#[ignore = "pełne miasto — uruchamiane z --release"]
fn kazdy_panel_operacyjny_wydaje_komende() {
    let s = swiat(ScenarioId::SANDBOX);
    let (c, th) = (katalog(), theme());
    for id in Panels::default().reg.ids() {
        // **Giełda ma własny test i własny powód** (`GG-6`): w dobie pierwszej nie
        // jest notowana ani jedna spółka, bo debiut wymaga **opublikowanego
        // dodatniego wyniku**, a pierwszy raport wychodzi w dobie 75. Panel wystawia
        // wtedy jeden przycisk — „wprowadź moją spółkę na giełdę" — i jest on
        // wygaszony z nazwanym powodem. To jest mechanizm, nie brak: sprawdza go
        // `panel_gieldy_sklada_zlecenie_gdy_jest_co_kupowac`.
        if id == PanelId::Stock {
            continue;
        }
        let mut panele = Panels::default();
        let cmd = klikaj_az_wyda(&mut panele, &s, &c, &th, id);
        if id.is_operational() {
            assert!(
                cmd.is_some(),
                "{id:?}: panel operacyjny, a nie da się z niego nic wydać"
            );
        } else {
            assert!(
                cmd.is_none(),
                "{id:?}: panel bez komend wydał {cmd:?} — kryterium go pomija, \
                 więc nie ma prawa nic wydawać"
            );
        }
    }
}

/// Kryterium WP12: żadna komenda nie sprawdza etapu kariery.
///
/// Test jest dwuczęściowy z rozmysłu. Pierwsza część jest **zachowaniem**: gracz
/// bez firmy zakłada ją w pierwszej dobie i komenda przechodzi. Druga jest
/// **lintem po źródłach**: w ścieżce walidacji nie wolno wymienić `CareerTier`,
/// bo tier ma być etykietą, a nie bramą (§5.4).
#[test]
fn zaden_precheck_nie_pyta_o_etap_kariery() {
    let zrodla = [
        include_str!("../src/command/mod.rs"),
        include_str!("../src/command/exec.rs"),
    ];
    for z in zrodla {
        assert!(
            !z.contains("CareerTier"),
            "ścieżka komend wymienia CareerTier — tier zaczął bramkować"
        );
    }
}

/// Kryterium WP12, część zachowaniowa: w trybie otwartym gracz zakłada firmę
/// natychmiast, a etap kariery idzie za faktami.
#[test]
#[ignore = "pełne miasto — uruchamiane z --release"]
fn firme_da_sie_zalozyc_w_pierwszej_dobie() {
    // Wariant bez majątku: dokładnie ten, o którym §13.2 mówi „żadnych bram".
    let mut s = swiat_wariantu(ScenarioId::SANDBOX, StartVariant::Sandbox);
    assert_eq!(
        CareerTier::derive(&s),
        CareerTier::Employee,
        "gracz bez firmy jest pracownikiem"
    );
    let dzielnice = {
        let m = s.market.as_ref().expect("gospodarka");
        magnat_economy::firmlife::districts_with_seed(m)
    };
    let (d, _) = *dzielnice.first().expect("jakaś dzielnica z lokalem");
    let wynik = s.submit(PlayerCommand::FoundFirm {
        district: d,
        capital: Money(500_000),
    });
    assert!(wynik.is_ok(), "założenie firmy odrzucone: {wynik:?}");
    s.step(2, 0);
    let h = Holdings::of(&s);
    assert!(!h.firms.is_empty(), "firma nie powstała");
    assert!(!h.sites.is_empty(), "nowy zakład nie trafił do gracza");
    assert_ne!(
        CareerTier::derive(&s),
        CareerTier::Employee,
        "etap kariery idzie za faktami, a nie za zezwoleniem"
    );
}

/// Spadkobierca dziedziczy **firmę**, a nie wskaźnik na zakład: komendy dotyczące
/// jego własnego sklepu mają przechodzić od pierwszej doby (`DI-1`).
#[test]
#[ignore = "pełne miasto — uruchamiane z --release"]
fn spadkobierca_jest_wlascicielem_swojego_zakladu() {
    let s = swiat(ScenarioId::SANDBOX);
    let h = Holdings::of(&s);
    let site = *h.sites.first().expect("spadkobierca ma zakład");
    assert!(!h.firms.is_empty(), "zakład bez firmy nie jest własnością");
    let wynik = magnat_game::precheck(&s.view(), &PlayerCommand::CloseSite { site });
    assert!(
        wynik.is_ok(),
        "własny zakład odrzucony jako cudzy: {wynik:?}"
    );
}

/// Kryterium WP11: kronika zbiera wpisy i da się w niej szukać.
#[test]
#[ignore = "pełne miasto — uruchamiane z --release"]
fn kronika_zapisuje_decyzje_gracza_i_da_sie_je_odfiltrowac() {
    let mut s = swiat(ScenarioId::SANDBOX);
    // Doba gry, żeby zbieracz kroniki w ogóle się odpalił.
    s.step(1_441, 0);
    let k = s.chronicle();
    assert!(!k.is_empty(), "kronika pusta po dobie gry");
    let moje = k.query(&magnat_game::chronicle::Query {
        kind: Some(magnat_game::ChronicleKind::PlayerAction),
        ..magnat_game::chronicle::Query::default()
    });
    assert!(
        moje.iter().any(|e| matches!(
            e.payload,
            magnat_game::chronicle::ChroniclePayload::Action { key } if key == "set_character"
        )),
        "wybór postaci nie trafił do kroniki"
    );
    let c = katalog();
    for l in [Locale::Pl, Locale::En] {
        for e in k.entries().iter().take(20) {
            let t = magnat_game::chronicle::text(e, &s, &c, l);
            assert!(!t.trim().is_empty(), "{l:?}: wpis kroniki bez tekstu");
            assert!(!t.contains('{'), "{l:?}: niepodstawiony parametr w `{t}`");
        }
    }
}

/// Kryterium WP11: „zatrzymaj, gdy…" trafia i wskazuje podmiot.
#[test]
#[ignore = "pełne miasto — uruchamiane z --release"]
fn warunek_zatrzymania_trafia_na_progu_gotowki() {
    let s = swiat(ScenarioId::SANDBOX);
    let mut w = magnat_game::StopWatch::default();
    // Próg powyżej każdego realnego salda — warunek ma trafić w pierwszej godzinie.
    let id = w.arm(magnat_game::StopCondition::CashBelow {
        amount: Money(i64::MAX / 2),
    });
    let h = w.check(&s, false).expect("warunek nie trafił");
    assert_eq!(h.id, id);
    // Wyłączony warunek nie trafia, a zostaje na liście.
    w.set(id, false);
    let mut s2 = s;
    s2.step(61, 0);
    assert!(w.check(&s2, false).is_none(), "wyłączony warunek trafił");
    assert_eq!(w.armed().len(), 1, "wyłączenie skasowało warunek z listy");
}

/// Kryterium WP12: scenariusz „Pierwszy sklep" stawia niszę i domyka cel.
#[test]
#[ignore = "pełne miasto — uruchamiane z --release"]
fn samouczek_stawia_nisze_i_liczy_cel() {
    let s = swiat(ScenarioId(1));
    let sc = s.scenario().expect("scenariusz samouczka");
    assert_eq!(sc.key, "first_shop");
    // Łatka zamknęła sklepy jednego rodzaju — nisza jest faktem świata, nie tekstem.
    let m = s.market.as_ref().expect("gospodarka");
    let wszystkie = m.sites();
    let zamkniete = wszystkie.iter().filter(|x| m.is_shop_closed(**x)).count();
    assert!(zamkniete > 0, "łatka nie zamknęła ani jednego sklepu");
    assert!(
        zamkniete < wszystkie.len(),
        "łatka wygasiła cały handel w mieście — to nie jest nisza, tylko pustynia"
    );
    // Spadkobierca ma już zakład, więc pierwszy cel („otwórz sklep") jest spełniony.
    let h = Holdings::of(&s);
    assert!(
        sc.objectives[0].goal.holds(&s, &h),
        "cel otwarcia pierwszego sklepu nie liczy zakładu, który gracz prowadzi"
    );
}

/// Kryterium WP10: klatka bez zmiany danych **nie przebudowuje** modelu panelu.
/// To jest to samo kryterium, które `M9b` postawił dla karty inspekcji.
#[test]
#[ignore = "pełne miasto — uruchamiane z --release"]
fn klatka_bez_zmian_nie_przebudowuje_panelu() {
    let s = swiat(ScenarioId::SANDBOX);
    let (c, th) = (katalog(), theme());
    let mut panele = Panels::default();
    let _ = rysuj(&mut panele, &s, &c, &th, Locale::Pl, PanelId::Shop);
    let po_pierwszej = panele.rebuilds(PanelId::Shop);
    for _ in 0..30 {
        let _ = rysuj(&mut panele, &s, &c, &th, Locale::Pl, PanelId::Shop);
    }
    assert_eq!(
        panele.rebuilds(PanelId::Shop),
        po_pierwszej,
        "panel przebudował model bez zmiany danych"
    );
}

/// Kryterium WP10: układ doku nie wchodzi do hasha stanu — jest preferencją widoku.
#[test]
#[ignore = "pełne miasto — uruchamiane z --release"]
fn uklad_paneli_nie_zmienia_hasha_stanu() {
    let mut s = swiat(ScenarioId::SANDBOX);
    s.step(10, 0);
    let przed = s.state_hash();
    let mut panele = Panels::default();
    panele.layout.pin(PanelId::Finance);
    panele.layout.toggle(PanelId::Finance);
    panele.layout.unpin(PanelId::Shop);
    let (c, th) = (katalog(), theme());
    for id in panele.reg.ids() {
        let _ = rysuj(&mut panele, &s, &c, &th, Locale::En, id);
    }
    assert_eq!(
        przed,
        s.state_hash(),
        "rysowanie paneli ruszyło stan świata"
    );
}

/// Podmiot z panelu prowadzi do inspekcji, a nie do drugiej karty (`DF-6`).
#[test]
fn panel_pokazuje_podmiot_zamiast_rysowac_druga_karte() {
    let a = PanelAction::Show(Subject::Site(SiteId(magnat_core::Entity::new(
        7,
        std::num::NonZeroU32::MIN,
    ))));
    assert!(matches!(a, PanelAction::Show(_)));
    // Kronika jest jedynym panelem bez komend i mówi to o sobie.
    assert!(!PanelId::Chronicle.is_operational());
    for id in [
        PanelId::Dashboard,
        PanelId::Plant,
        PanelId::Shop,
        PanelId::Supply,
        PanelId::Market,
        PanelId::People,
        PanelId::Finance,
        PanelId::City,
    ] {
        assert!(id.is_operational(), "{id:?} wypadł z listy operacyjnych");
    }
    // Po `M10g` **żaden panel nie jest zarezerwowany**: marka, badania i giełda
    // mają rejestr, model i komendę. Identyfikator bez ekranu byłby wariantem
    // bez skutku (`K-67`), więc rezerwacja ma znikać razem z napisaniem panelu.
    let reg = Panels::default();
    for id in [PanelId::Brand, PanelId::Rnd, PanelId::Stock] {
        assert!(
            !id.is_reserved(),
            "{id:?} jest napisany, a udaje rezerwację"
        );
        assert!(
            id.is_operational(),
            "{id:?} ma komendę, więc jest operacyjny"
        );
        assert!(reg.reg.get(id).is_some(), "{id:?} nie jest w rejestrze");
    }
}

/// Domyślny układ nowego gracza to **dwa panele, nie osiem** (§5.12 pkt 5).
#[test]
fn domyslny_uklad_ma_dwa_panele() {
    let l = magnat_game::Layout::default();
    assert_eq!(l.pinned.len(), 2, "onboarding dostaje dwa panele");
    assert!(l.is_pinned(PanelId::Shop) && l.is_pinned(PanelId::Dashboard));
    // Panel Finanse **da się** otworzyć — `min_tier` steruje przypięciem, nie dostępem.
    let reg = magnat_game::PanelRegistry::default();
    assert!(reg.get(PanelId::Finance).is_some());
    let dla_pracownika = magnat_game::Layout::for_tier(&reg, CareerTier::Employee);
    assert!(
        !dla_pracownika.is_pinned(PanelId::Finance),
        "pracownik nie dostaje przypiętej księgowości"
    );
    let dla_magnata = magnat_game::Layout::for_tier(&reg, CareerTier::Magnate);
    assert_eq!(
        dla_magnata.pinned.len(),
        reg.len(),
        "magnat ma przypięte wszystko"
    );
}

/// Kryterium WP10.21: gracz składa zlecenie kupna i widzi jego los po fixingu.
///
/// Notowanie stawiamy **ścieżką symulacji** (`Equity::list`), a nie przewijaniem
/// świata o siedemdziesiąt pięć dób: test ma sprawdzić panel, a nie warunek debiutu
/// — ten ma własny test po stronie `sim/economy`. Gdyby stawiał go przez przewijanie,
/// mierzyłby przy okazji rentowność firm w pierwszym kwartale i pękałby z powodu,
/// który z panelem nie ma nic wspólnego.
#[test]
#[ignore = "pełne miasto — uruchamiane z --release"]
fn panel_gieldy_sklada_zlecenie_gdy_jest_co_kupowac() {
    let mut s = swiat(ScenarioId::SANDBOX);
    let (c, th) = (katalog(), theme());

    // Pierwsza firma z rejestru wchodzi na giełdę po kursie 1 gr za punkt bazowy.
    let klucz = s
        .app
        .world
        .get_resource::<magnat_firms::Firms>()
        .and_then(|f| f.iter().next().map(|(k, _)| k))
        .expect("miasto bez firm");
    let t = s.tick();
    {
        let eq = s
            .app
            .world
            .get_resource_mut::<magnat_economy::equity::Equity>()
            .expect("giełda");
        assert!(
            eq.list(klucz, 2_500, Money(100), magnat_core::SimMinute(t.get())),
            "spółka nie weszła na giełdę"
        );
    }

    let mut panele = Panels::default();
    let cmd = klikaj_az_wyda(&mut panele, &s, &c, &th, PanelId::Stock);
    assert!(
        matches!(cmd, Some(PlayerCommand::PlaceStockOrder { .. })),
        "panel giełdy z notowaną spółką nie złożył zlecenia, tylko {cmd:?}"
    );

    // Zlecenie idzie do arkusza — to jest „los zlecenia", który gracz widzi.
    let cmd = cmd.expect("komenda");
    s.submit(cmd).expect("zlecenie odrzucone");
    s.step(1, 0);
    let ile = s
        .app
        .world
        .get_resource::<magnat_economy::equity::Equity>()
        .map_or(0, |e| e.orders(klucz).len());
    assert!(ile > 0, "zlecenie gracza nie trafiło do arkusza");
}

/// Kryterium WP10.19: gracz kupuje reklamę i kampania zaczyna istnieć.
///
/// Klikamy **w panel**, a nie składamy komendy ręcznie: kryterium mówi „gracz
/// kupuje", a komenda zbudowana w teście przeszłaby nawet wtedy, gdyby żaden
/// przycisk nie był do niej podpięty (`K-67`).
#[test]
#[ignore = "pełne miasto — uruchamiane z --release"]
fn panel_marki_kupuje_kampanie_i_ta_zaczyna_istniec() {
    let mut s = swiat(ScenarioId::SANDBOX);
    let (c, th) = (katalog(), theme());
    let przed = s
        .app
        .world
        .get_resource::<magnat_media::Campaigns>()
        .map_or(0, magnat_media::Campaigns::len);

    let mut panele = Panels::default();
    let cmd = klikaj_az_wyda(&mut panele, &s, &c, &th, PanelId::Brand);
    let Some(PlayerCommand::OpenCampaign { .. }) = cmd else {
        panic!("panel marki nie kupił reklamy, tylko {cmd:?}");
    };
    s.submit(cmd.expect("komenda")).expect("kampania odrzucona");
    s.step(1, 0);
    let po = s
        .app
        .world
        .get_resource::<magnat_media::Campaigns>()
        .map_or(0, magnat_media::Campaigns::len);
    assert_eq!(po, przed + 1, "kampania gracza nie trafiła do rejestru");
}

/// Kryterium WP10.20: gracz wskazuje węzeł do badania i projekt rusza.
///
/// Sprawdzamy przy okazji, że **to gracz wybrał**, a nie reguła „najtańszy
/// osiągalny": projekt ma istnieć zaraz po komendzie, a nie po najbliższym
/// miesięcznym kroku R&D.
#[test]
#[ignore = "pełne miasto — uruchamiane z --release"]
fn panel_badan_otwiera_projekt_wskazany_przez_gracza() {
    let mut s = swiat(ScenarioId::SANDBOX);
    let (c, th) = (katalog(), theme());
    let mut panele = Panels::default();
    let cmd = klikaj_az_wyda(&mut panele, &s, &c, &th, PanelId::Rnd);
    let Some(PlayerCommand::StartResearch { site, ref tech }) = cmd else {
        panic!("panel badań nie otworzył projektu, tylko {cmd:?}");
    };
    let klucz = tech.clone();
    let firma = s
        .app
        .world
        .get_resource::<magnat_firms::Firms>()
        .and_then(|f| f.site(site))
        .map(|z| z.firm)
        .expect("zakład bez firmy");
    s.submit(cmd.expect("komenda")).expect("badania odrzucone");
    s.step(1, 0);
    let firms = s
        .app
        .world
        .get_resource::<magnat_firms::Firms>()
        .expect("rejestr");
    let projekt = firms.rnd().projects.get(&firma).expect("brak projektu");
    let nazwa = s
        .app
        .world
        .get_resource::<magnat_firms::RndData>()
        .and_then(|d| d.tree.get(projekt.tech).map(|n| n.key.to_string()))
        .expect("węzeł");
    assert_eq!(nazwa, klucz, "firma bada co innego, niż wskazał gracz");
}
