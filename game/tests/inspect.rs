//! Testy podfazy M9c: postać gracza (WP4), karta inspekcji (WP5), nakładki (WP7).
//!
//! Przebieg z prawdziwym miastem jest `#[ignore]` i chodzi w CI z `--release` razem
//! z pozostałymi testami integracyjnymi — w profilu debug generacja terenu 4 km trwa
//! minuty, a mierzy wtedy kompilator, nie kod.

use magnat_core::{Entity, Subject, SubjectKind};
use magnat_game::inspect::{card, sample_subjects, CardCtx};
use magnat_game::player::{self, StartVariant};
use magnat_game::session::Session;
use magnat_game::shell::{NewGameParams, ScenarioId};
use magnat_game::world::{population, SessionOpts};
use magnat_game::{GenWatch, PlayerCommand};
use magnat_jobs::JobPool;
use magnat_ui::{Catalog, Locale, RichExt};
use magnat_world::{Difficulty, EconomyProfile, Epoch, Region, WorldGenParams, WorldSize};

fn params(variant: StartVariant) -> NewGameParams {
    NewGameParams {
        world: WorldGenParams {
            seed: 11,
            size: WorldSize::Small4km,
            epoch: Epoch::Y1990,
            profile: EconomyProfile::Mixed,
            region: Region::Lowland,
            difficulty: Difficulty::Normal,
        },
        scenario: ScenarioId::SANDBOX,
        variant,
        opts: SessionOpts {
            citizens: 1500,
            commute_swaps: 5_000,
            economy: true,
            micro: false,
        },
    }
}

fn swiat(variant: StartVariant) -> Session {
    let pool = JobPool::new(0);
    let p = params(variant);
    let built = population::zbuduj_z_params(p.world, &pool, &GenWatch::none())
        .expect("generacja")
        .expect("nie anulowano");
    Session::begin(built, p, &pool).expect("sesja")
}

fn katalog() -> Catalog {
    Catalog::load().expect("data/locale/")
}

/// Wydruk karty w obu językach — bez pustki i bez niepodstawionych parametrów.
fn wydruk(s: &Session, c: &Catalog, subject: Subject) -> (String, String) {
    let mut out = Vec::new();
    for l in [Locale::Pl, Locale::En] {
        let k = card(&CardCtx::new(c, l, s), subject);
        let t = k.render_text(c, l);
        assert!(
            !t.trim().is_empty(),
            "{l:?} {:?}: karta bez ani jednego napisu",
            subject.kind()
        );
        assert!(
            !t.contains('{'),
            "{l:?} {:?}: niepodstawiony parametr w `{t}`",
            subject.kind()
        );
        out.push(t);
    }
    (out[0].clone(), out[1].clone())
}

/// Kryterium WP4 i WP5 naraz, bo obie potrzebują tego samego stojącego miasta:
/// gracz wybiera postać, a z jej karty da się dojść do domu, pracodawcy, rodziny
/// i pojazdu — i wrócić.
#[test]
#[ignore = "pełne miasto — uruchamiane z --release"]
fn gracz_wybiera_postac_a_z_jej_karty_da_sie_dojsc_wszedzie() {
    let mut s = swiat(StartVariant::Worker);
    let c = katalog();
    let doba = s.tick().get() / 1440;

    // WP4: każdy z pięciu wariantów ma kogo zaproponować, a predykaty się różnią.
    let mut listy = Vec::new();
    for v in StartVariant::ALL {
        let k = player::candidates(&s.app.world, v, doba);
        assert!(!k.is_empty(), "wariant {v:?} nie ma ani jednego kandydata");
        listy.push(k.iter().map(|x| x.citizen).collect::<Vec<_>>());
    }
    assert_ne!(
        listy[0], listy[1],
        "absolwent i pracownik dostali tę samą listę — predykat nie działa"
    );

    let kandydat = player::candidates(&s.app.world, StartVariant::Worker, doba)[0].clone();
    let przed = majatek(&s, kandydat.citizen);
    s.submit(PlayerCommand::SetCharacter {
        citizen: kandydat.citizen,
    })
    .expect("wybór postaci");
    s.step(1, 0);

    let postac = s.player().expect("postać po wykonaniu komendy").clone();
    assert_eq!(postac.citizen, kandydat.citizen);
    assert_eq!(
        majatek(&s, kandydat.citizen).get() - przed.get(),
        StartVariant::Worker.capital().get(),
        "kapitał startowy nie wszedł do gospodarstwa"
    );
    assert!(
        s.app
            .world
            .get::<magnat_agents::Identity>(postac.citizen.entity())
            .is_some_and(|i| i.flags & magnat_agents::Identity::FLAG_PLAYER != 0),
        "postać gracza nie jest oznaczona w `Identity`"
    );
    // Drugi wybór jest błędem, nie przeprowadzką — inaczej kapitał wpadałby dwa razy.
    assert!(s
        .submit(PlayerCommand::SetCharacter {
            citizen: kandydat.citizen
        })
        .is_err());

    // WP5: karta mieszkanki, oba języki, odnośniki.
    let podmiot = Subject::Citizen(postac.citizen);
    let (pl, en) = wydruk(&s, &c, podmiot);
    assert_ne!(pl, en, "karta wygląda tak samo w obu językach");
    let karta = card(&CardCtx::new(&c, Locale::Pl, &s), podmiot);
    let linki = karta.links();
    assert!(
        !linki.is_empty(),
        "karta mieszkanki nie ma ani jednego odnośnika"
    );
    // Dom i gospodarstwo są zawsze; pracodawca i pojazd zależą od kandydata,
    // więc sprawdzamy je warunkowo — ale **każdy** odnośnik musi się otwierać.
    assert!(
        linki.iter().any(|l| l.kind() == SubjectKind::Building),
        "z karty nie da się dojść do domu"
    );
    assert!(
        linki.iter().any(|l| l.kind() == SubjectKind::Household),
        "z karty nie da się dojść do gospodarstwa"
    );
    for l in &linki {
        wydruk(&s, &c, *l);
    }

    // Stos wstecz: skok w odnośnik i powrót do pytania, od którego gracz zaczął.
    let mut nav = magnat_ui::InspectionNav::new();
    nav.go(podmiot);
    nav.go(linki[0]);
    assert!(nav.back());
    assert_eq!(nav.current(), Some(podmiot));
}

/// Kryterium WP5: „dlaczego Anna nie kupiła u mnie" — z nazwanym konkurentem
/// i klikalnym odnośnikiem, w obu językach.
#[test]
#[ignore = "pełne miasto — uruchamiane z --release"]
fn karta_mieszkanca_mowi_dlaczego_nie_kupil_i_u_kogo_kupil() {
    let mut s = swiat(StartVariant::Worker);
    let c = katalog();
    let market = s.market.clone().expect("gospodarka włączona");

    // Wszystkie zakłady śledzone: pierścień utraconych sprzedaży prowadzą tylko one.
    for site in market.sites() {
        market.set_tracking(site, magnat_economy::LostSaleTracking::Full);
    }
    // Jeden sklep z celowo zawyżoną ceną — scenariusz akceptacyjny „Anna" z §7.
    let drogi = market.sites()[0];
    for w in market.shelf_snapshot() {
        if w.site == drogi {
            if let Some(cena) = market.price_at(drogi, w.good) {
                market.set_price(drogi, w.good, magnat_core::Money(cena.get() * 5));
            }
        }
    }
    s.step(14 * 1440, 0);

    // Kto odszedł do konkurencji — ten wpis niesie i powód, i zwycięzcę.
    let (site, wpis) = market
        .sites()
        .into_iter()
        .flat_map(|x| market.lost_sales(x).into_iter().map(move |w| (x, w)))
        .find(|(_, w)| w.went_to.is_some())
        .unwrap_or_else(|| {
            let ile: usize = market
                .sites()
                .iter()
                .map(|x| market.lost_sales(*x).len())
                .sum();
            let st = market.stats();
            panic!(
                "nikt nie odszedł do konkurencji; zakładów {}, wpisów {ile}, zakupów {}",
                market.sites().len(),
                st.purchases
            )
        });
    assert_ne!(
        site,
        wpis.went_to.expect("wpis z konkurentem"),
        "konkurentem jest ten sam zakład"
    );

    // Karta pokazuje **ostatnie** wizyty, więc test pyta o tego konkurenta,
    // którego gracz na niej zobaczy — inaczej sprawdzałby wpis spoza ekranu.
    let jego = market.lost_sales_of_citizen(wpis.citizen);
    let rywal = jego
        .iter()
        .rev()
        .take(8)
        .find_map(|(_, w)| w.went_to)
        .expect("ostatnie wizyty tego mieszkańca bez konkurenta");

    let podmiot = Subject::Citizen(wpis.citizen);
    for l in [Locale::Pl, Locale::En] {
        let k = card(&CardCtx::new(&c, l, &s), podmiot);
        let why = k
            .tabs
            .iter()
            .find(|t| t.kind == magnat_ui::CardTabKind::Why)
            .unwrap_or_else(|| panic!("{l:?}: karta bez zakładki „Dlaczego”"));
        let tekst = why.body.to_plain();
        assert!(!tekst.trim().is_empty(), "{l:?}: pusta zakładka „Dlaczego”");
        assert!(
            why.body
                .iter()
                .any(|sp| sp.link == Some(Subject::Site(rywal))),
            "{l:?}: konkurent nie jest odnośnikiem — `{tekst}`"
        );
        let nazwa = CardCtx::new(&c, l, &s).subject_name(Subject::Site(rywal));
        assert!(
            tekst.contains(&nazwa),
            "{l:?}: konkurent nie jest nazwany — `{tekst}`"
        );
    }
}

/// Kryterium WP5: **każdy** wariant `Subject` renderuje się w obu językach, a cel,
/// którego nie ma, renderuje się bez odnośnika (`Z-5`).
#[test]
#[ignore = "pełne miasto — uruchamiane z --release"]
fn kazdy_podmiot_ma_karte_a_nieistniejacy_nie_ma_odnosnika() {
    let s = swiat(StartVariant::Sandbox);
    let c = katalog();

    let mut rodzaje: Vec<SubjectKind> = Vec::new();
    for p in sample_subjects(|i| Entity::new(i, std::num::NonZeroU32::MIN)) {
        wydruk(&s, &c, p);
        rodzaje.push(p.kind());
    }
    assert_eq!(
        rodzaje.len(),
        SubjectKind::ALL.len(),
        "lista przykładowych podmiotów rozjechała się z `SubjectKind`"
    );

    // Mieszkaniec o indeksie, którego w tym świecie nie ma: nazwa zostaje, odnośnik nie.
    let duch = Subject::Citizen(magnat_core::CitizenId(Entity::new(
        9_000_000,
        std::num::NonZeroU32::MIN,
    )));
    let ctx = CardCtx::new(&c, Locale::Pl, &s);
    assert!(
        !magnat_game::inspect::resolve(&s, duch),
        "nieistniejący mieszkaniec uchodzi za żywego"
    );
    let span = ctx.subject_span(duch);
    assert!(span.link.is_none(), "odnośnik prowadzi w pustkę");
    assert!(
        !span.text.trim().is_empty(),
        "cel bez nazwy i bez odnośnika"
    );
}

/// Kryterium WP7: dziewięć nakładek z §14.2 buduje pole, a rezerwacja M10 — nie.
#[test]
#[ignore = "pełne miasto — uruchamiane z --release"]
fn dziewiec_nakladek_danych_buduje_pole() {
    let mut s = swiat(StartVariant::Worker);
    // Dwie doby, żeby ceny i ruch miały co pokazać: nakładka zbudowana w ticku zero
    // jest technicznie poprawna i pusta, a pustka wygląda tak samo jak zepsucie.
    s.step(2 * 1440, 0);
    let market = s.market.clone().expect("gospodarka włączona");
    let site = market.sites()[0];
    market.set_tracking(site, magnat_economy::LostSaleTracking::Full);
    let good = market.shelf_snapshot()[0].good;

    for f in magnat_game::OverlayField::all(site, good) {
        let pole = magnat_game::overlays::build(&s, f)
            .unwrap_or_else(|| panic!("nakładka {} nie zbudowała pola", f.key()));
        assert!(pole.dim > 0 && pole.cell_m > 0.0, "{}: puste pole", f.key());
        assert_eq!(
            pole.values.len(),
            (pole.dim as usize) * (pole.dim as usize),
            "{}: raster niezgodny z wymiarem",
            f.key()
        );
        assert!(
            !pole.legend.is_empty(),
            "{}: legenda bez podziałek",
            f.key()
        );
        assert!(!pole.unit.is_empty(), "{}: legenda bez jednostki", f.key());
    }

    // Cztery nakładki **muszą** mieć niezerowe komórki w każdym zaludnionym mieście —
    // inaczej „pole się zbudowało" nic nie znaczy. Pozostałe pięć zależy od tego, co
    // akurat dzieje się w gospodarce, i zero jest dla nich poprawną odpowiedzią.
    for f in [
        magnat_game::OverlayField::LandValue,
        magnat_game::OverlayField::HouseholdIncome,
        magnat_game::OverlayField::Unemployment,
        magnat_game::OverlayField::Health,
    ] {
        let pole = magnat_game::overlays::build(&s, f).expect("pole");
        assert!(
            pole.values.iter().any(|v| *v > 0),
            "raster nakładki {} jest pusty",
            f.key()
        );
    }

    assert!(
        magnat_game::overlays::build(&s, magnat_game::OverlayField::BrandAwareness).is_none(),
        "nakładka zarezerwowana dla M10 rysuje zera zamiast milczeć"
    );
}

fn majatek(s: &Session, c: magnat_core::CitizenId) -> magnat_core::Money {
    let id = s
        .app
        .world
        .get::<magnat_agents::Identity>(c.entity())
        .expect("mieszkaniec");
    let e = magnat_agents::household_by_index(&s.app.world, id.household).expect("gospodarstwo");
    let h = s
        .app
        .world
        .get::<magnat_agents::Household>(e)
        .expect("komponent gospodarstwa");
    magnat_core::Money(h.cash.get() + h.bank.get() + h.savings.get())
}
