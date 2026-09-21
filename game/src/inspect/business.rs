//! Karty firmy, zakładu i pojazdu (M9c §5.7).
//!
//! Zakład handlowy **nie dostaje drugiej karty**: półki, klientów i konkurencję
//! składa `magnat_ui::ShopCard` z migawki `ShopPanelSnapshot` od M5e, a ten moduł
//! dokłada nagłówek z odnośnikiem do firmy i zakładkę „Dlaczego" z powodami przecen.
//! Druga arytmetyka tych samych liczb rozjechałaby się z pierwszą (`Y-1`).

use super::CardCtx;
use magnat_core::{CoverId, FirmId, SiteId, Subject, Tick, VehicleId};
use magnat_firms::{FirmKey, Firms};
use magnat_ui::{CardTabKind, InspectionCard, Rich, ShopCard, ShopTab, ShopView, Span};

/// `FirmId` niesie indeks encji, a `Firms` kluczuje po `FirmKey` — przejście jest
/// odwrotnością `magnat_firms::firm_id` i mieszka **w jednym miejscu**, tak samo jak
/// konwersja klucza zakładu z `K-46`.
#[must_use]
fn key_of(f: FirmId) -> FirmKey {
    FirmKey(u64::from(f.entity().index()))
}

fn firms(session: &crate::Session) -> Option<&Firms> {
    session.app.world.get_resource::<Firms>()
}

#[must_use]
pub fn firm_exists(session: &crate::Session, f: FirmId) -> bool {
    firms(session).is_some_and(|r| r.get(key_of(f)).is_some())
}

#[must_use]
pub fn site_exists(session: &crate::Session, s: SiteId) -> bool {
    firms(session).is_some_and(|r| r.site(s).is_some())
        || session
            .market
            .as_ref()
            .is_some_and(|m| m.account_of(s).is_some())
}

/// Czy polisa istnieje i nie wygasła. `false` jest normalnym stanem świata:
/// polisa kończy się z upływem okresu i nikt jej nie wskrzesza.
#[must_use]
pub fn cover_exists(session: &crate::Session, c: CoverId) -> bool {
    session
        .app
        .world
        .get_resource::<magnat_economy::insurance::Insurers>()
        .and_then(|i| i.cover(c))
        .is_some_and(|x| !x.ended)
}

#[must_use]
pub fn vehicle_exists(session: &crate::Session, v: VehicleId) -> bool {
    session
        .app
        .world
        .get::<magnat_traffic::VehicleOwner>(v.entity())
        .is_some()
}

/// Nazwa firmy z `data/names/` — proceduralna, więc bez klucza lokalizacji.
#[must_use]
pub fn firm_name(ctx: &CardCtx<'_>, f: FirmId) -> String {
    firms(ctx.session)
        .and_then(|r| r.get(key_of(f)).map(|x| x.name.clone()))
        .unwrap_or_else(|| {
            ctx.fmt(
                "ui.subject.firm",
                &[("nr", &f.entity().index().to_string())],
            )
        })
}

/// Nazwa zakładu: nazwa firmy, która go prowadzi, plus rodzaj. Własnych nazw zakłady
/// nie mają — `ponytail:` sufit nazwany, bo nazwa punktu („Dobry Koszyk nr 3") jest
/// kosmetyką, a odnośnik działa bez niej.
#[must_use]
pub fn site_name(ctx: &CardCtx<'_>, s: SiteId) -> String {
    let firma = firms(ctx.session)
        .and_then(|r| r.site(s).map(|x| x.firm))
        .and_then(|k| firms(ctx.session).and_then(|r| r.get(k).map(|f| f.name.clone())));
    match firma {
        Some(n) => n,
        None => ctx.fmt(
            "ui.subject.site",
            &[("nr", &s.entity().index().to_string())],
        ),
    }
}

#[must_use]
pub fn vehicle_name(ctx: &CardCtx<'_>, v: VehicleId) -> String {
    ctx.fmt(
        "ui.subject.vehicle",
        &[("nr", &v.entity().index().to_string())],
    )
}

/// Karta firmy: właściciele, zakłady i powody ostatnich decyzji.
///
/// Pełny pulpit firmy (przepływy, KPI, alerty) jest panelem biznesowym i należy
/// do `M9e` — tutaj jest **karta**, czyli tożsamość i odnośniki.
#[must_use]
pub fn firm_card(ctx: &CardCtx<'_>, f: FirmId) -> InspectionCard {
    let subject = Subject::Firm(f);
    let nazwa = firm_name(ctx, f);
    let naglowek = magnat_ui::lines_titled(&format!("{nazwa}\n"));
    let Some(firm) = firms(ctx.session).and_then(|r| r.get(key_of(f))) else {
        return InspectionCard::new(subject, naglowek).tab(
            CardTabKind::State,
            vec![
                magnat_ui::gone_span(ctx.c, ctx.l, &nazwa),
                Span::plain("\n".to_string()),
            ],
        );
    };

    let mut stan: Rich = Vec::new();
    ctx.line(
        &mut stan,
        "ui.firm.status",
        &[("stan", &format!("{:?}", firm.status))],
    );
    ctx.line(
        &mut stan,
        "ui.firm.strategy",
        &[(
            "kurs",
            &ctx.text(&format!("ui.strategy.{}", firm.strategy.name())),
        )],
    );
    if let Some(d) = firm.director {
        ctx.link_line(&mut stan, "ui.firm.director", Subject::Citizen(d));
    }
    ctx.link_line(
        &mut stan,
        "ui.card.district",
        Subject::District(firm.hq_district),
    );
    for s in &firm.sites {
        ctx.link_line(&mut stan, "ui.firm.site", Subject::Site(*s));
    }
    // Zmowa i stali dostawcy są **sekcjami** karty firmy, a nie własnymi podmiotami
    // (`FF-22`): pytanie gracza brzmi „z kim ta firma trzyma", a nie „pokaż mi
    // relację numer cztery". Sufit siedmiu zakładek zostaje nietknięty (§5.12).
    zmowa(ctx, &mut stan, key_of(f));
    dostawcy(ctx, &mut stan, f);

    let mut dlaczego: Rich = Vec::new();
    for d in firm.log.iter() {
        dlaczego.push(Span::plain(format!(
            "{}  {}\n",
            magnat_ui::zegar((d.tick.get() % 1440) as u16),
            ctx.reason(d.reason)
        )));
    }

    InspectionCard::new(subject, naglowek)
        .tab(CardTabKind::State, stan)
        .tab(CardTabKind::Why, dlaczego)
}

/// Karta zakładu. Sklep dostaje trzy zakładki z migawki M5e; zakład bez półki —
/// tożsamość i odnośniki.
#[must_use]
pub fn site_card(ctx: &CardCtx<'_>, s: SiteId) -> InspectionCard {
    let subject = Subject::Site(s);
    let nazwa = site_name(ctx, s);
    let t = ctx.session.tick();
    let od = Tick(t.get() - t.get() % magnat_core::time::MINUTES_PER_MONTH);
    let migawka = ctx
        .session
        .market
        .as_ref()
        .and_then(|m| m.shop_panel(s, od, t));

    if let Some(snap) = migawka {
        let card = ShopCard::build(
            ctx.c,
            ctx.l,
            &ShopView {
                snapshot: &snap,
                kind: snap.kind,
                period_from: od,
            },
        );
        let mut naglowek: Rich = vec![Span::emphasis(format!("{nazwa}\n"))];
        naglowek.extend(card.render_header(ctx.c, ctx.l));
        naglowek.push(Span::plain(format!("{}: ", ctx.text("ui.firm.owner"))));
        naglowek.push(ctx.subject_span(Subject::Firm(snap.firm)));
        naglowek.push(Span::plain("\n".to_string()));

        let mut dlaczego: Rich = Vec::new();
        for r in &snap.reprices {
            dlaczego.push(Span::plain(format!("{}\n", ctx.reason(*r))));
        }
        // Tytuł medialny i związek zawodowy wchodzą jako sekcje zakładki „Stan",
        // której karta sklepu do tej pory nie miała — puste ciało nie tworzy
        // zakładki, więc sklep bez sporu i bez redakcji wygląda jak dotąd.
        let mut stan: Rich = Vec::new();
        tytul_medialny(ctx, &mut stan, s);
        zwiazek(ctx, &mut stan, s);
        return InspectionCard::new(subject, naglowek)
            .tab(CardTabKind::State, stan)
            .tab(
                CardTabKind::Shelves,
                card.render_tab(ctx.c, ctx.l, ShopTab::Shelves),
            )
            .tab(
                CardTabKind::Customers,
                card.render_tab(ctx.c, ctx.l, ShopTab::Customers),
            )
            .tab(
                CardTabKind::Competition,
                card.render_tab(ctx.c, ctx.l, ShopTab::Competition),
            )
            .tab(CardTabKind::Why, dlaczego);
    }

    let naglowek = magnat_ui::lines_titled(&format!("{nazwa}\n"));
    let mut stan: Rich = Vec::new();
    match firms(ctx.session).and_then(|r| r.site(s)) {
        Some(site) => {
            ctx.link_line(
                &mut stan,
                "ui.firm.owner",
                Subject::Firm(owner_id(site.firm)),
            );
            ctx.link_line(
                &mut stan,
                "ui.card.building",
                Subject::Building(site.building),
            );
            ctx.link_line(
                &mut stan,
                "ui.card.district",
                Subject::District(site.district),
            );
            ctx.line(
                &mut stan,
                "ui.site.staff",
                &[("ile", &site.positions.len().to_string())],
            );
        }
        None => stan.push(magnat_ui::gone_span(ctx.c, ctx.l, &nazwa)),
    }
    tytul_medialny(ctx, &mut stan, s);
    zwiazek(ctx, &mut stan, s);
    InspectionCard::new(subject, naglowek).tab(CardTabKind::State, stan)
}

fn owner_id(k: FirmKey) -> FirmId {
    magnat_firms::firm_id(k)
}

/// Karta pojazdu: stan techniczny, paliwo i właściciel.
#[must_use]
pub fn vehicle_card(ctx: &CardCtx<'_>, v: VehicleId) -> InspectionCard {
    let subject = Subject::Vehicle(v);
    let nazwa = vehicle_name(ctx, v);
    let naglowek = magnat_ui::lines_titled(&format!("{nazwa}\n"));
    let world = &ctx.session.app.world;
    let mut stan: Rich = Vec::new();

    let Some(owner) = world
        .get::<magnat_traffic::VehicleOwner>(v.entity())
        .copied()
    else {
        return InspectionCard::new(subject, naglowek).tab(
            CardTabKind::State,
            vec![
                magnat_ui::gone_span(ctx.c, ctx.l, &nazwa),
                Span::plain("\n".to_string()),
            ],
        );
    };

    if let Some(cond) = world.get::<magnat_traffic::VehicleCondition>(v.entity()) {
        ctx.line(
            &mut stan,
            "ui.vehicle.wear",
            &[("ile", &cond.wear.to_string())],
        );
    }
    if let Some(tank) = world.get::<magnat_traffic::FuelTank>(v.entity()) {
        // Poziom paliwa jest w mikrolitrach (`K-25`); gracz czyta litry.
        ctx.line(
            &mut stan,
            "ui.vehicle.fuel",
            &[("litry", &format!("{:.1}", tank.level as f64 / 1_000_000.0))],
        );
    }
    if owner.primary_driver != magnat_traffic::VehicleOwner::NO_DRIVER {
        if let Some(e) = magnat_agents::citizen_by_index(world, owner.primary_driver) {
            ctx.link_line(
                &mut stan,
                "ui.vehicle.driver",
                Subject::Citizen(magnat_core::CitizenId(e)),
            );
        }
    }
    if owner.kind == magnat_traffic::OwnerKind::Household as u8 {
        if let Some(e) = magnat_agents::household_by_index(world, owner.owner) {
            ctx.link_line(
                &mut stan,
                "ui.card.household",
                Subject::Household(magnat_core::HouseholdId(e)),
            );
        }
    }
    InspectionCard::new(subject, naglowek).tab(CardTabKind::State, stan)
}

// ── M10g: sekcje głębi w kartach zakładu i firmy, karta polisy ──────────────────

/// Sekcja „tytuł medialny" w karcie zakładu (`FF-4`).
///
/// Bez własnego wariantu `Subject`, bo **tytuł jest zakładem**: gazeta ma lokal,
/// załogę i księgę tak samo jak piekarnia. Drugi podmiot o tej samej tożsamości
/// byłby dokładnie tą drugą prawdą, którą `K-39` raz już usuwał.
fn tytul_medialny(ctx: &CardCtx<'_>, out: &mut Rich, s: SiteId) {
    let Some(o) = ctx
        .session
        .app
        .world
        .get_resource::<magnat_media::Outlets>()
        .and_then(|r| r.get(s))
    else {
        return;
    };
    ctx.line(
        out,
        "ui.card.outlet_kind",
        &[(
            "rodzaj",
            &ctx.text(&format!("ui.media_kind.{}", o.kind.name())),
        )],
    );
    ctx.line(
        out,
        "ui.card.outlet_credibility",
        &[("ile", &o.credibility.get().to_string())],
    );
    ctx.line(
        out,
        "ui.card.outlet_bias",
        &[(
            "linia",
            &ctx.text(&format!("ui.editorial_bias.{}", o.bias.name())),
        )],
    );
    // Czytelnictwo: dzielnice, do których tytuł naprawdę dociera, najmocniejsza
    // pierwsza. Zera nie pokazujemy — „nie dociera" to brak wiersza, nie wiersz zero.
    let mut zasieg: Vec<(magnat_core::DistrictId, u16)> = o
        .readership
        .iter()
        .copied()
        .filter(|(_, p)| *p > 0)
        .collect();
    zasieg.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0 .0.cmp(&b.0 .0)));
    for (d, permille) in zasieg.into_iter().take(5) {
        out.push(Span::plain(format!(
            "{}: ",
            ctx.text("ui.card.outlet_readership")
        )));
        out.push(ctx.subject_span(Subject::District(d)));
        out.push(Span::plain(format!(" {permille}\u{2030}\n")));
    }
}

/// Sekcja „związek zawodowy" w karcie zakładu (`FF-22`).
///
/// Też bez wariantu `Subject`: spór **jest stanem zakładu**, a nie bytem obok niego —
/// gracz pyta „co się dzieje w tej hali", a nie „pokaż mi związek numer siedem".
fn zwiazek(ctx: &CardCtx<'_>, out: &mut Rich, s: SiteId) {
    let Some(u) = ctx
        .session
        .app
        .world
        .get_resource::<magnat_economy::Unions>()
    else {
        return;
    };
    let zal = u.grievance(s);
    if zal.level > 0 {
        ctx.line(
            out,
            "ui.card.grievance",
            &[
                ("poziom", &zal.level.to_string()),
                ("dni", &zal.days_above.to_string()),
            ],
        );
    }
    let Some(z) = u.get(s) else { return };
    let stan = match z.state {
        magnat_economy::UnionState::Dormant { .. } => "dormant",
        magnat_economy::UnionState::Demand { .. } => "demand",
        magnat_economy::UnionState::Talks { .. } => "talks",
        magnat_economy::UnionState::Strike { .. } => "strike",
    };
    ctx.line(
        out,
        "ui.card.union",
        &[
            ("stan", &ctx.text(&format!("ui.union_state.{stan}"))),
            ("gestosc", &z.density.get().to_string()),
        ],
    );
    if let Some(d) = z.demand {
        ctx.line(
            out,
            "ui.card.union_demand",
            &[("bp", &d.raise_bp.to_string())],
        );
    }
    if !matches!(z.state, magnat_economy::UnionState::Dormant { .. }) {
        ctx.line(
            out,
            "ui.card.strike_fund",
            &[("kwota", &ctx.money_str(z.strike_fund))],
        );
    }
}

/// Sekcja „zmowa" w karcie firmy (`FF-22`).
///
/// **Widoczna dopiero po wykryciu, nie wcześniej** — i to jest kryterium WP10.22,
/// nie ozdoba. Zmowa jest nielegalna od pierwszej minuty, ale nikt o niej nie wie:
/// uczestnicy pilnują tajemnicy, a karta pokazująca żywy kartel zdradzałaby ją
/// każdemu, kto kliknie w firmę. Dlatego sekcja czyta **dziennik decyzji firmy**
/// i bierze z niego wyłącznie `CartelDetected`, a nie rejestr `Cartels`.
///
/// Rejestr i tak by tu nie pomógł: `Cartels::bust` **usuwa** zmowę z listy razem
/// z wykryciem, więc po wykryciu nie ma jej w `iter()`, a przed wykryciem jest.
/// Czytanie go dałoby dokładnie odwrotność kryterium.
fn zmowa(ctx: &CardCtx<'_>, out: &mut Rich, key: FirmKey) {
    let Some(firms) = firms(ctx.session) else {
        return;
    };
    let Some(firm) = firms.get(key) else { return };
    for d in firm.log.iter() {
        let magnat_core::DecisionReason::CartelDetected {
            members, months, ..
        } = d.reason
        else {
            continue;
        };
        ctx.line(
            out,
            "ui.card.cartel",
            &[
                ("ilu", &members.to_string()),
                ("miesiecy", &months.to_string()),
            ],
        );
    }
}

/// Sekcja „stali dostawcy" w karcie firmy (`FF-22`).
fn dostawcy(ctx: &CardCtx<'_>, out: &mut Rich, f: FirmId) {
    let Some(m) = ctx.session.market.as_ref() else {
        return;
    };
    let chain = m.chain();
    let lock = chain.lock();
    let tuning = *lock.b2b.relations().tuning();
    let mut rel: Vec<(FirmId, u8, u16)> = lock
        .b2b
        .relations()
        .iter()
        .filter(|r| r.buyer == f)
        .map(|r| (r.supplier, r.trust.get(), r.discount_bp(&tuning)))
        .collect();
    // Najbardziej zaufany pierwszy; remis rozstrzyga niższy indeks dostawcy, żeby
    // karta wyglądała tak samo w dwóch przebiegach tego samego świata.
    rel.sort_unstable_by(|a, b| {
        b.1.cmp(&a.1)
            .then(a.0.entity().index().cmp(&b.0.entity().index()))
    });
    for (dostawca, trust, rabat) in rel.into_iter().take(6) {
        out.push(Span::plain(format!("{}: ", ctx.text("ui.card.supplier"))));
        out.push(ctx.subject_span(Subject::Firm(dostawca)));
        out.push(Span::plain(format!(
            " {} {trust}, {} {rabat} bp\n",
            ctx.text("ui.card.trust"),
            ctx.text("ui.card.preference")
        )));
    }
}

/// Karta polisy ubezpieczeniowej (`FF-15`, `K-85`).
///
/// Jedna zakładka i zapas do sufitu siedmiu: polisa jest umową o czterech liczbach,
/// a nie bytem z historią własnych decyzji. **Historia szkód jest dzielnicowa**,
/// bo taka jest w modelu — ubezpieczyciel wycenia ryzyko z przebiegu dzielnicy,
/// a nie z przebiegu jednego magazynu (`data/tuning/insurance.ron` nie ma ani jednej
/// liczby o ryzyku miejsca i to jest treść kryterium WP10.12).
#[must_use]
pub fn cover_card(ctx: &CardCtx<'_>, c: CoverId) -> InspectionCard {
    let subject = Subject::Cover(c);
    let nazwa = ctx.subject_name(subject);
    let naglowek = magnat_ui::lines_titled(&format!("{nazwa}\n"));
    let mut stan: Rich = Vec::new();
    let polisa = ctx
        .session
        .app
        .world
        .get_resource::<magnat_economy::insurance::Insurers>()
        .and_then(|i| i.cover(c).copied());
    let Some(p) = polisa else {
        stan.push(magnat_ui::gone_span(ctx.c, ctx.l, &nazwa));
        return InspectionCard::new(subject, naglowek).tab(CardTabKind::State, stan);
    };
    ctx.link_line(&mut stan, "ui.card.insured_site", Subject::Site(p.site));
    ctx.link_line(
        &mut stan,
        "ui.card.insurer",
        Subject::Firm(magnat_firms::firm_id(p.insurer)),
    );
    ctx.line(
        &mut stan,
        "ui.card.peril",
        &[("rodzaj", &ctx.text(&format!("ui.peril.{}", p.peril.name())))],
    );
    ctx.line(
        &mut stan,
        "ui.card.sum_insured",
        &[("kwota", &ctx.money_str(p.sum_insured))],
    );
    ctx.line(
        &mut stan,
        "ui.card.premium",
        &[("kwota", &ctx.money_str(p.premium_monthly))],
    );
    ctx.line(
        &mut stan,
        "ui.card.deductible",
        &[("kwota", &ctx.money_str(p.deductible))],
    );
    ctx.line(
        &mut stan,
        "ui.card.cover_rate",
        &[("bp", &p.rate_bp.to_string())],
    );
    if p.ended {
        ctx.line(&mut stan, "ui.card.cover_ended", &[]);
    }
    // Historia szkód dzielnicy — to z niej wychodzi składka i to jest jedyne
    // wyjaśnienie, dlaczego ta polisa kosztuje tyle, ile kosztuje.
    let st = ctx
        .session
        .app
        .world
        .get_resource::<magnat_economy::insurance::Insurers>()
        .map(|i| i.stats(p.peril, p.district));
    if let Some(st) = st {
        ctx.line(
            &mut stan,
            "ui.card.peril_history",
            &[
                ("szkod", &st.claims.to_string()),
                ("kwota", &ctx.money_str(st.loss_total)),
            ],
        );
    }
    InspectionCard::new(subject, naglowek).tab(CardTabKind::State, stan)
}
