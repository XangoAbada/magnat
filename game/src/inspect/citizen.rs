//! Karta mieszkańca i karta gospodarstwa (M9c §5.7).
//!
//! Sześć zakładek z tabeli w §5.7: Stan · Dzień · Rodzina · Majątek · Praca · Dlaczego.
//! Kolejność jest stała i to jest jej cały sens — gracz, który raz znalazł „Dlaczego"
//! na końcu, ma je tam znaleźć zawsze.
//!
//! Zakładka **Dlaczego** odpowiada na pytanie, od którego zaczyna się cała faza:
//! „dlaczego Anna nie kupiła u mnie?". Odpowiedź składa się z zapisów, które prowadzi
//! **sklep** (`Market::lost_sales_of_citizen`), a nie mieszkaniec — pierścień 256 wpisów
//! na zakład śledzony jest tańszy niż bufor odmów przy każdym z 400 tys. agentów.
//! Sufit jest przez to jawny: widać wyłącznie zakłady gracza.

use super::CardCtx;
use magnat_agents::{Household, Identity};
use magnat_core::{CitizenId, HouseholdId, Money, Subject};
use magnat_ui::{CardTabKind, CitizenModel, InspectionCard, Rich, Span};

/// Ile utraconych wizyt pokazuje zakładka „Dlaczego". Pierścień ma 256 wpisów na
/// zakład; karta pokazuje ostatnie, bo starsze odpowiadają na nieaktualne pytanie.
const LOST_SALES_SHOWN: usize = 8;

/// Ile relacji pokazuje zakladka „Rodzina" — najsilniejsze najpierw.
const RELATIONS_SHOWN: usize = 8;

#[must_use]
pub fn is_alive(session: &crate::Session, c: CitizenId) -> bool {
    session
        .app
        .world
        .get::<Identity>(c.entity())
        .is_some_and(Identity::is_alive)
}

#[must_use]
pub fn household_exists(session: &crate::Session, h: HouseholdId) -> bool {
    session
        .app
        .world
        .get::<Household>(h.entity())
        .is_some_and(|x| x.flags & Household::FLAG_ACTIVE != 0)
}

/// Imię i nazwisko z `data/names/`. Nazwy własne **nie są** lokalizacją UI (CLAUDE.md),
/// więc nie mają klucza — mieszkaniec nazwiskiem Schmidt nazywa się tak samo w obu
/// wersjach językowych.
#[must_use]
pub fn name_of(ctx: &CardCtx<'_>, c: CitizenId) -> String {
    ctx.session
        .app
        .world
        .get::<Identity>(c.entity())
        .map_or_else(
            || {
                ctx.fmt(
                    "ui.subject.citizen",
                    &[("nr", &c.entity().index().to_string())],
                )
            },
            magnat_ui::full_name,
        )
}

/// Karta mieszkańca.
#[must_use]
pub fn card(ctx: &CardCtx<'_>, citizen: CitizenId) -> InspectionCard {
    let subject = Subject::Citizen(citizen);
    let nazwa = name_of(ctx, citizen);
    let world = &ctx.session.app.world;
    let Some(model) = CitizenModel::of(ctx.c, ctx.l, world, citizen, ctx.day(), ctx.session.seed())
    else {
        // Mieszkaniec, którego już nie ma: nazwa, jedno zdanie i żadnego odnośnika.
        let body: Rich = vec![
            magnat_ui::gone_span(ctx.c, ctx.l, &nazwa),
            Span::plain("\n".to_string()),
        ];
        return InspectionCard::new(subject, magnat_ui::lines_titled(&format!("{nazwa}\n")))
            .tab(CardTabKind::State, body);
    };

    InspectionCard::new(subject, naglowek(ctx, citizen, &model))
        .tab(CardTabKind::State, stan(ctx, &model))
        .tab(CardTabKind::Day, dzien(ctx, &model))
        .tab(CardTabKind::Family, rodzina(ctx, citizen))
        .tab(CardTabKind::Wealth, majatek(ctx, citizen, &model))
        .tab(CardTabKind::Work, praca(ctx, citizen))
        .tab(CardTabKind::Brands, marki(ctx, citizen))
        .tab(CardTabKind::Why, dlaczego(ctx, citizen))
}

/// Nagłówek: tożsamość i adres. Adres jest odnośnikiem do budynku (`Z-2`) — to jest
/// pole `State`, a nie `Relations`, i dlatego odnośnik musiał trafić do kawałka tekstu.
fn naglowek(ctx: &CardCtx<'_>, citizen: CitizenId, m: &CitizenModel) -> Rich {
    let h = &m.card.header;
    let mut out: Rich = vec![Span::emphasis(format!(
        "{}, {}, {} — ",
        h.name,
        magnat_ui::inspect::reason::years(ctx.c, ctx.l, h.age_years),
        h.occupation
    ))];
    match dom(ctx, citizen) {
        Some(b) => out.push(ctx.subject_span(b)),
        None => out.push(Span::plain(h.address.clone())),
    }
    out.push(Span::plain("\n".to_string()));
    out
}

fn stan(ctx: &CardCtx<'_>, m: &CitizenModel) -> Rich {
    let mut out: Rich = Vec::new();
    ctx.line(&mut out, "ui.card.needs", &[]);
    for n in &m.card.needs {
        out.push(Span::plain(format!(
            "  {:<14} {:>3}/100{}  — {}\n",
            n.label,
            n.level,
            if n.critical { " !" } else { "  " },
            n.tooltip
        )));
    }
    out.push(Span::plain(format!(
        "{}: {} ({}/100)\n",
        ctx.text("ui.card.status"),
        m.card.social_class,
        m.card.status
    )));
    for r in &m.card.status_rows {
        out.push(Span::plain(format!(
            "  {:<22} {:>3}/100 × {} %\n",
            r.label, r.value, r.weight
        )));
    }
    out
}

fn dzien(ctx: &CardCtx<'_>, m: &CitizenModel) -> Rich {
    let timeline = m.timeline();
    let mut out: Rich = Vec::new();
    for r in timeline.rows(ctx.c, ctx.l) {
        let mut linia = format!(
            "{}–{}  {:<12} ({})",
            magnat_ui::zegar(r.start_min),
            magnat_ui::zegar(r.end_min % 1440),
            r.label,
            r.reason
        );
        for a in &r.alternatives {
            linia.push_str(&format!(" · {a}"));
        }
        linia.push('\n');
        out.push(Span::plain(linia));
    }
    let pominiete = timeline.skipped(ctx.c, ctx.l);
    if !pominiete.is_empty() {
        ctx.line(&mut out, "ui.card.skipped", &[]);
        for p in pominiete {
            out.push(Span::plain(format!("  {p}\n")));
        }
    }
    out
}

/// Rodzina: gospodarstwo i jego skład — każdy członek z odnośnikiem do własnej karty.
/// To jest droga „od Anny do jej męża" z kryterium WP5.
fn rodzina(ctx: &CardCtx<'_>, citizen: CitizenId) -> Rich {
    let mut out: Rich = Vec::new();
    let Some((hh_e, hh)) = gospodarstwo(ctx, citizen) else {
        ctx.line(&mut out, "ui.card.no_household", &[]);
        return out;
    };
    ctx.link_line(
        &mut out,
        "ui.card.household",
        Subject::Household(HouseholdId(hh_e)),
    );
    ctx.line(
        &mut out,
        "ui.card.household_kind",
        &[
            (
                "typ",
                &magnat_ui::inspect::reason::household_kind(
                    ctx.c,
                    ctx.l,
                    magnat_agents::HouseholdKind::from_u8(hh.kind),
                ),
            ),
            ("ile", &hh.size.to_string()),
        ],
    );
    for m in czlonkowie(ctx, hh_e.index(), &hh) {
        if m == citizen {
            continue;
        }
        out.push(Span::plain("  ".to_string()));
        out.push(ctx.subject_span(Subject::Citizen(m)));
        out.push(Span::plain("\n".to_string()));
    }
    znajomi(ctx, citizen, &mut out);
    out
}

/// Krewni spoza gospodarstwa i znajomi — z grafu relacji.
///
/// Osobno od skladu gospodarstwa, bo to sa dwie rozne odpowiedzi: „z kim mieszka"
/// i „kogo zna". Karta pokazuje do [`RELATIONS_SHOWN`] najsilniejszych — ma odpowiadac
/// na pytanie, a nie byc spisem kontaktow.
fn znajomi(ctx: &CardCtx<'_>, citizen: CitizenId, out: &mut Rich) {
    let world = &ctx.session.app.world;
    let Some(r) = world.get::<magnat_agents::RelationsRef>(citizen.entity()) else {
        return;
    };
    let mut wpisy: Vec<magnat_agents::Relation> = world
        .resource::<magnat_agents::RelationSlab>()
        .entries(magnat_agents::relations_ref(r))
        .to_vec();
    if wpisy.is_empty() {
        ctx.line(out, "ui.card.relations_none", &[]);
        return;
    }
    // Najsilniejsze najpierw, remis po indeksie encji — kolejnosc ma byc ta sama
    // przy kazdym otwarciu karty, a slab nie obiecuje zadnej.
    wpisy.sort_by_key(|x| (std::cmp::Reverse(x.weight), x.other));
    ctx.line(out, "ui.card.relations", &[]);
    for w in wpisy.iter().take(RELATIONS_SHOWN) {
        let Some(e) = magnat_agents::citizen_by_index(world, w.other) else {
            continue;
        };
        let rodzaj = ctx.text(&format!("ui.relation.{}", relation_key(w.kind)));
        out.push(Span::plain(format!("  {rodzaj}: ")));
        out.push(ctx.subject_span(Subject::Citizen(CitizenId(e))));
        out.push(Span::plain(format!(
            " ({}/100)
",
            w.weight
        )));
    }
}

/// Klucz rodzaju relacji w `data/locale/`. Rodzaj spoza listy M3 zostaje przy
/// „znajomy": slab niesie bajt, a nie enum, wiec wartosc spoza zakresu jest mozliwa.
const fn relation_key(kind: u8) -> &'static str {
    match kind {
        1 => "partner",
        2 => "parent",
        3 => "child",
        4 => "sibling",
        5 => "friend",
        6 => "colleague",
        7 => "neighbour",
        _ => "acquaintance",
    }
}

fn majatek(ctx: &CardCtx<'_>, citizen: CitizenId, m: &CitizenModel) -> Rich {
    let mut out: Rich = Vec::new();
    for (k, v) in [
        ("ui.card.wealth.cash", m.card.cash),
        ("ui.card.wealth.household", m.card.household_budget),
        ("ui.card.wealth.income", m.card.income_monthly),
    ] {
        out.push(Span::plain(format!(
            "{}: {}\n",
            ctx.text(k),
            magnat_ui::zlotowki(v)
        )));
    }
    if let Some(b) = dom(ctx, citizen) {
        ctx.link_line(&mut out, "ui.card.home", b);
    }
    for v in pojazdy(ctx, citizen) {
        ctx.link_line(&mut out, "ui.card.vehicle", v);
    }
    out
}

/// Marki, które mieszkaniec zna (M10b §5.1, M10 §6 pkt 6).
///
/// To jest miejsce, w którym widać, że marka **nie jest liczbą po stronie firmy**:
/// każdy wiersz mówi, czego ten człowiek się po marce spodziewa, jak ją lubi i skąd
/// ją zna. Sloty są już z naniesionym zanikiem — `slots_of` liczy go przy odczycie.
///
/// Mieszkaniec bez ani jednego slotu nie dostaje zakładki: `InspectionCard::tab`
/// pomija pustą treść, a pusta zakładka obiecuje coś, czego nie ma.
fn marki(ctx: &CardCtx<'_>, citizen: CitizenId) -> Rich {
    let world = &ctx.session.app.world;
    let mut sloty = magnat_agents::slots_of(world, citizen.entity(), ctx.day())
        .as_slice()
        .to_vec();
    if sloty.is_empty() {
        return Vec::new();
    }
    // Najmocniej odczuwane najpierw; remis po numerze marki (determinizm wydruku).
    sloty.sort_by_key(|s| (std::cmp::Reverse(s.salience()), s.brand.0));
    let mut out: Rich = Vec::new();
    for s in sloty {
        out.push(ctx.subject_span(Subject::Firm(magnat_supply::firm_of(s.brand))));
        out.push(Span::plain(format!(
            " — {}\n",
            ctx.fmt(
                "ui.brand.line",
                &[
                    ("sympatia", &format!("{}", s.affinity)),
                    ("jakosc", &format!("{}", s.expected_quality)),
                    ("znajomosc", &format!("{}", s.awareness)),
                    (
                        "skad",
                        &magnat_ui::inspect::reason::touch_source(ctx.c, ctx.l, s.source()),
                    ),
                ],
            )
        )));
    }
    out
}

fn praca(ctx: &CardCtx<'_>, citizen: CitizenId) -> Rich {
    let mut out: Rich = Vec::new();
    let world = &ctx.session.app.world;
    let Some(e) = world.get::<magnat_agents::Employment>(citizen.entity()) else {
        return out;
    };
    match magnat_agents::places::place_from_key(e.site) {
        Some(magnat_core::PlaceRef::Site(s)) if e.is_employed() => {
            ctx.link_line(&mut out, "ui.card.employer", Subject::Site(s));
        }
        _ => ctx.line(&mut out, "ui.card.no_job", &[]),
    }
    if e.has_job() {
        ctx.line(
            &mut out,
            "ui.card.commute",
            &[("minuty", &e.commute_baseline_min.to_string())],
        );
    }
    out
}

/// „Dlaczego Anna nie kupiła u mnie" — wpisy z pierścienia utraconych sprzedaży
/// zakładów śledzonych, od najnowszego.
fn dlaczego(ctx: &CardCtx<'_>, citizen: CitizenId) -> Rich {
    let mut out: Rich = Vec::new();
    let Some(market) = ctx.session.market.as_ref() else {
        return out;
    };
    let mut wpisy = market.lost_sales_of_citizen(citizen);
    wpisy.reverse();
    if wpisy.is_empty() {
        ctx.line(&mut out, "ui.card.no_lost_sales", &[]);
        return out;
    }
    for (site, s) in wpisy.into_iter().take(LOST_SALES_SHOWN) {
        let minuta = (s.when.get() % 1440) as u16;
        out.push(Span::plain(format!(
            "{}  {} — ",
            magnat_ui::zegar(minuta),
            towar(ctx, market, s.good)
        )));
        out.push(ctx.subject_span(Subject::Site(site)));
        out.push(Span::plain(format!(
            ": {}",
            magnat_ui::inspect::reason::reject_cause(ctx.c, ctx.l, s.cause)
        )));
        if let Some(rywal) = s.went_to {
            out.push(Span::plain(format!(" · {} ", ctx.text("ui.card.went_to"))));
            out.push(ctx.subject_span(Subject::Site(rywal)));
        }
        out.push(Span::plain("\n".to_string()));
    }
    out
}

/// Karta gospodarstwa domowego: skład, budżet, mieszkanie.
#[must_use]
pub fn household_card(ctx: &CardCtx<'_>, h: HouseholdId) -> InspectionCard {
    let subject = Subject::Household(h);
    let nazwa = ctx.subject_name(subject);
    let naglowek = magnat_ui::lines_titled(&format!("{nazwa}\n"));
    let world = &ctx.session.app.world;
    let Some(hh) = world.get::<Household>(h.entity()).copied() else {
        let mut body: Rich = vec![
            magnat_ui::gone_span(ctx.c, ctx.l, &nazwa),
            Span::plain("\n".to_string()),
        ];
        body.push(Span::plain(String::new()));
        return InspectionCard::new(subject, naglowek).tab(CardTabKind::State, body);
    };

    let mut sklad: Rich = Vec::new();
    for m in czlonkowie(ctx, h.entity().index(), &hh) {
        sklad.push(Span::plain("  ".to_string()));
        sklad.push(ctx.subject_span(Subject::Citizen(m)));
        sklad.push(Span::plain("\n".to_string()));
    }

    let mut budzet: Rich = Vec::new();
    for (k, v) in [
        ("ui.card.wealth.cash", hh.cash),
        ("ui.household.bank", hh.bank),
        ("ui.household.savings", hh.savings),
        ("ui.household.debt", hh.debt),
        ("ui.card.wealth.income", hh.income_monthly),
    ] {
        budzet.push(Span::plain(format!(
            "{}: {}\n",
            ctx.text(k),
            magnat_ui::zlotowki(v)
        )));
    }

    let mut majatek: Rich = Vec::new();
    if hh.building != Household::NO_BUILDING {
        if let Some(magnat_core::PlaceRef::Building(b)) =
            magnat_agents::places::place_from_key(hh.building)
        {
            ctx.link_line(&mut majatek, "ui.card.home", Subject::Building(b));
        }
    }

    InspectionCard::new(subject, naglowek)
        .tab(CardTabKind::Family, sklad)
        .tab(CardTabKind::State, budzet)
        .tab(CardTabKind::Wealth, majatek)
}

/// Nazwa towaru w języku gracza. Towar bez wpisu w katalogu tekstów zostaje przy
/// swoim kluczu — ta sama zasada co w karcie sklepu: surowy klucz jest lepszy
/// od wywróconego interfejsu.
fn towar(ctx: &CardCtx<'_>, market: &magnat_economy::Market, g: magnat_core::GoodId) -> String {
    let Some(key) = market.good_key(g) else {
        return format!("#{}", g.0);
    };
    ctx.c
        .key(&format!("ui.good.{key}"))
        .map_or(key, |k| ctx.c.text(ctx.l, k).to_string())
}

// ── pomocnicze ───────────────────────────────────────────────────────────────────

fn gospodarstwo(ctx: &CardCtx<'_>, citizen: CitizenId) -> Option<(magnat_core::Entity, Household)> {
    let world = &ctx.session.app.world;
    let id = world.get::<Identity>(citizen.entity())?;
    let e = magnat_agents::household_by_index(world, id.household)?;
    Some((e, *world.get::<Household>(e)?))
}

fn czlonkowie(ctx: &CardCtx<'_>, household_index: u32, hh: &Household) -> Vec<CitizenId> {
    let world = &ctx.session.app.world;
    let overflow = world.resource::<magnat_agents::HouseholdOverflow>();
    magnat_agents::members_of(household_index, hh, overflow)
        .iter()
        .filter_map(|i| magnat_agents::citizen_by_index(world, *i).map(CitizenId))
        .collect()
}

fn dom(ctx: &CardCtx<'_>, citizen: CitizenId) -> Option<Subject> {
    let r = ctx
        .session
        .app
        .world
        .get::<magnat_agents::Residence>(citizen.entity())?;
    match magnat_agents::places::home_of(r) {
        Some(magnat_core::PlaceRef::Building(b)) => Some(Subject::Building(b)),
        _ => None,
    }
}

/// Pojazdy gospodarstwa — z `VehicleOwner`, a nie z numerów slotów.
///
/// Do R2e karta czytała `Household.vehicle_slots` i pokazywała **pustą listę
/// zawsze**: pole miało czytelnika (tę funkcję), ale nie miało pisarza — M4 miał
/// je wypełniać i nigdy tego nie zrobił. `VehicleOwner.owner` jest za to prawdziwy
/// i jest jedynym źródłem prawdy o tym, czyj jest pojazd (`R2-WP22`).
///
/// `ponytail:` przejście po całej flocie, bo indeksu „gospodarstwo → pojazdy" nie
/// ma. Sufit: karta otwiera się kliknięciem, a nie co klatkę, a flota metropolii
/// to rząd stu tysięcy encji. Wyjście, gdy pojawi się drugi czytelnik: odwrotność
/// `VehicleOwner` w `TrafficServices`, budowana razem z flotą.
fn pojazdy(ctx: &CardCtx<'_>, citizen: CitizenId) -> Vec<Subject> {
    let world = &ctx.session.app.world;
    let Some((hh_id, _)) = gospodarstwo(ctx, citizen) else {
        return Vec::new();
    };
    let Some(t) = world.get_resource::<magnat_traffic::TrafficServices>() else {
        return Vec::new();
    };
    let dom = hh_id.index();
    t.fleet
        .iter()
        .filter(|e| {
            world
                .get::<magnat_traffic::VehicleOwner>(**e)
                .is_some_and(|o| {
                    o.kind == magnat_traffic::OwnerKind::Household as u8 && o.owner == dom
                })
        })
        .map(|e| Subject::Vehicle(magnat_core::VehicleId(*e)))
        .collect()
}

/// Suma gotówki gospodarstwa — wiersz slotu zapisu (majątek gracza) czyta to samo.
#[must_use]
pub fn household_worth(hh: &Household) -> Money {
    Money(hh.cash.get() + hh.bank.get() + hh.savings.get() - hh.debt.get())
}
