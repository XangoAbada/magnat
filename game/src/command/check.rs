//! Czy wolno — walidacja komendy gracza (`precheck`).
//!
//! # Jedna funkcja, dwa wywołania
//!
//! Panel woła ją **zanim** gracz kliknie, żeby wygasić przycisk z powodem; sesja
//! woła ją **autorytatywnie** w punkcie synchronizacji. Ta sama funkcja, bo dwie
//! rozjechałyby się przy pierwszej nowej regule — a rozjazd wyglądałby jak komenda,
//! która wygląda na dozwoloną i nie przechodzi.
//!
//! Osobny plik od [`super`], bo to jest inny temat: tam stoi **o co gracz prosi**,
//! tutaj **czego mu brakuje**. Jedno rośnie razem z wykonawcami, drugie razem
//! z regułami świata.

use magnat_core::{CitizenId, Money, SiteId};
use magnat_economy::Market;

use super::{resolve_good, zgub_sklep, CommandError, CommandView, PlayerCommand};

/// Sprawdza, czy komendę wolno wykonać. **Nie zmienia niczego.**
///
/// # Errors
/// [`CommandError`] opisujący, czego brakuje.
/// `ponytail:` jeden wyczerpujący `match` na **wszystkie** komendy i tak ma zostać —
/// to on łapie komendę bez sprawdzenia. Sufit 250 linii został **przekroczony
/// w M10g przy dwudziestu pięciu komendach** (prognoza mówiła o trzydziestu)
/// i wyjście jest już zaczęte: cztery komendy paneli głębi mają własne funkcje
/// niżej, a w ramieniu został jeden wiersz. Następna komenda, która nie mieści
/// się w dwóch wierszach, idzie tą samą drogą; przy kolejnym przekroczeniu dzieli
/// się **po dziedzinie** (gospodarka, kadry, miasto, dynastia), a nie po sztuce.
pub fn precheck(view: &CommandView<'_>, cmd: &PlayerCommand) -> Result<(), CommandError> {
    match cmd {
        // Koperta bootstrapowa: świat powstaje **przed** sesją, więc nie ma tu czego
        // sprawdzać ani czego wykonać. Jej treścią są parametry, z których replay
        // odtwarza grę (§5.13).
        PlayerCommand::StartGame { .. } => Ok(()),
        PlayerCommand::SetPrice { site, good, price } => {
            if price.get() <= 0 {
                return Err(CommandError::PriceNotPositive { price: *price });
            }
            let market = view.market.ok_or(CommandError::NoMarket)?;
            let g = resolve_good(market, good)?;
            if market.price_at(*site, g).is_none() {
                return Err(zgub_sklep(market, *site, good));
            }
            Ok(())
        }
        PlayerCommand::SetCharacter { citizen } => {
            if view.has_character {
                return Err(CommandError::CharacterAlreadySet);
            }
            let zyje = view.world.is_some_and(|w| {
                w.get::<magnat_agents::Identity>(citizen.entity())
                    .is_some_and(magnat_agents::Identity::is_alive)
            });
            if zyje {
                Ok(())
            } else {
                Err(CommandError::CitizenNotFound { citizen: *citizen })
            }
        }
        PlayerCommand::SetAutonomy { .. } => {
            if view.has_character {
                Ok(())
            } else {
                Err(CommandError::NoCharacter)
            }
        }
        PlayerCommand::AttachPolicy { site, policy } => {
            moj_zaklad(view, *site)?;
            magnat_policy::validate(policy).map_err(|e| CommandError::PolicyInvalid {
                notes: u16::try_from(e.0.len()).unwrap_or(u16::MAX),
            })
        }
        PlayerCommand::DetachPolicy { site } => {
            let firms = moj_zaklad(view, *site)?;
            if firms.site(*site).is_some_and(|s| s.delegation.is_some()) {
                Ok(())
            } else {
                Err(CommandError::NoPolicy { site: *site })
            }
        }
        PlayerCommand::FoundFirm { district, capital } => {
            if !view.has_character {
                return Err(CommandError::NoCharacter);
            }
            dodatnia(*capital)?;
            let market = view.market.ok_or(CommandError::NoMarket)?;
            if moja_firma(view).is_ok() {
                return Err(CommandError::AlreadyHasFirm);
            }
            lokal_w_dzielnicy(market, *district).map(|_| ())
        }
        PlayerCommand::OpenSite { district, capex } => {
            // Nakład **wolno** mieć zerowy: firma, której nie stać na kapitał
            // obrotowy, otwiera punkt bez niego i zobaczy to w pierwszym miesiącu
            // (`firmlife::expand`). Ujemny jest błędem, zerowy decyzją.
            if capex.get() < 0 {
                return Err(CommandError::AmountNotPositive { amount: *capex });
            }
            let market = view.market.ok_or(CommandError::NoMarket)?;
            moja_firma(view)?;
            lokal_w_dzielnicy(market, *district).map(|_| ())
        }
        PlayerCommand::CloseSite { site } => moj_zaklad(view, *site).map(|_| ()),
        PlayerCommand::SetShelfAssortment { site, goods } => {
            moj_zaklad(view, *site)?;
            let market = view.market.ok_or(CommandError::NoMarket)?;
            for g in goods {
                resolve_good(market, g)?;
            }
            Ok(())
        }
        PlayerCommand::HireCandidate {
            site,
            citizen,
            role,
        } => {
            let firms = moj_zaklad(view, *site)?;
            zyje(view, *citizen)?;
            if firms.employer_of(*citizen).is_some() {
                return Err(CommandError::AlreadyEmployed { citizen: *citizen });
            }
            let wolny = firms.site(*site).is_some_and(|z| {
                z.positions
                    .iter()
                    .any(|p| p.role.0 == *role && p.filled.len() < usize::from(p.slots))
            });
            if wolny {
                Ok(())
            } else {
                Err(CommandError::NoVacancy {
                    site: *site,
                    role: *role,
                })
            }
        }
        PlayerCommand::SetDelegationAutonomy { site, .. } => {
            let firms = moj_zaklad(view, *site)?;
            if firms.site(*site).is_some_and(|z| z.delegation.is_some()) {
                Ok(())
            } else {
                Err(CommandError::NoPolicy { site: *site })
            }
        }
        PlayerCommand::ApplyForJob { offer } => {
            if !view.has_character {
                return Err(CommandError::NoCharacter);
            }
            let world = view.world.ok_or(CommandError::NoLabor)?;
            let rynek = world
                .get_resource::<magnat_economy::LaborHandle>()
                .ok_or(CommandError::NoLabor)?;
            let uchwyt = magnat_core::ArenaHandle::from_bits(*offer)
                .ok_or(CommandError::NoJobOffer { offer: *offer })?;
            let jest = rynek
                .get()
                .is_some_and(|m| m.offer(uchwyt).is_some_and(|o| o.slots > 0));
            if jest {
                Ok(())
            } else {
                Err(CommandError::NoJobOffer { offer: *offer })
            }
        }
        PlayerCommand::TakeLoan { site } => moj_zaklad(view, *site).map(|_| ()),
        PlayerCommand::BackCandidate { candidate, amount } => {
            if !view.has_character {
                return Err(CommandError::NoCharacter);
            }
            dodatnia(*amount)?;
            let world = view.world.ok_or(CommandError::NoCity)?;
            let city = world
                .get_resource::<magnat_city::City>()
                .ok_or(CommandError::NoCity)?;
            let e = city.election.as_ref().ok_or(CommandError::NoElection)?;
            if usize::from(*candidate) < e.candidates.len() {
                Ok(())
            } else {
                Err(CommandError::NoCandidate {
                    candidate: *candidate,
                })
            }
        }
        PlayerCommand::ApplyForPermit { kind } => {
            if !view.has_character {
                return Err(CommandError::NoCharacter);
            }
            let world = view.world.ok_or(CommandError::NoCity)?;
            let city = world
                .get_resource::<magnat_city::City>()
                .ok_or(CommandError::NoCity)?;
            if usize::from(*kind) >= magnat_core::PermitKind::ALL.len() {
                return Err(CommandError::UnknownPermit { kind: *kind });
            }
            // Urząd, którego nie ma, nie przyjmie wniosku — i to jest stan świata
            // bez strony publicznej, a nie błąd gracza.
            if city.permits.offices().is_empty() {
                return Err(CommandError::NoCity);
            }
            Ok(())
        }
        PlayerCommand::SetRestockTarget { site, good, .. } => {
            moj_zaklad(view, *site)?;
            let market = view.market.ok_or(CommandError::NoMarket)?;
            let g = resolve_good(market, good)?;
            if market.price_at(*site, g).is_none() {
                return Err(zgub_sklep(market, *site, good));
            }
            Ok(())
        }
        PlayerCommand::DeclarePersonalBankruptcy | PlayerCommand::SetHeir { .. } => {
            if view.has_character {
                Ok(())
            } else {
                Err(CommandError::NoCharacter)
            }
        }
        // Sukcesja i nowa dynastia wymagają, żeby postaci **nie było**: żywy gracz
        // nie dziedziczy po sobie. Warunek sprawdza się tu, a nie w stanie gry, bo
        // replay odtwarza komendy, a nie ekrany.
        PlayerCommand::Succeed { citizen } | PlayerCommand::ContinueAsNewCitizen { citizen } => {
            zyje(view, *citizen)
        }
        // Komendy paneli głębi mają własne funkcje, a nie ciała w ramieniu: `precheck`
        // przekroczyła próg 250 linii dokładnie tam, gdzie zapowiadał to komentarz
        // w jej opisie. Wyczerpujący `match` zostaje — to on łapie komendę bez
        // sprawdzenia, a podział po dziedzinie był zapowiedzianą drogą wyjścia.
        PlayerCommand::OpenCampaign {
            site,
            channel,
            target,
            budget,
            days,
        } => kampania(view, *site, *channel, *target, *budget, *days),
        PlayerCommand::StartResearch { site, tech } => badania(view, *site, tech),
        PlayerCommand::PlaceStockOrder {
            firm,
            limit,
            bp,
            days,
            ..
        } => zlecenie(view, *firm, *limit, *bp, *days),
        PlayerCommand::GoPublic { site } => {
            let firms = moj_zaklad(view, *site)?;
            let key = firms
                .site(*site)
                .ok_or(CommandError::SiteNotFound { site: *site })?
                .firm;
            let world = view.world.ok_or(CommandError::NoEquity)?;
            let eq = world
                .get_resource::<magnat_economy::equity::Equity>()
                .ok_or(CommandError::NoEquity)?;
            // Notowana już jest — drugi debiut tej samej spółki nie znaczy nic.
            // Bez opublikowanego dodatniego wyniku nie ma czego wyceniać i to
            // jest ten sam próg, który obowiązuje firmę AI.
            let moze = eq.listing(key).is_none()
                && eq.published(key).is_some_and(|p| p.profit_12m.get() > 0);
            if moze {
                Ok(())
            } else {
                Err(CommandError::NotListed)
            }
        }
        PlayerCommand::AnswerUnion { site, .. } => {
            moj_zaklad(view, *site)?;
            let world = view.world.ok_or(CommandError::NoFirms)?;
            let trwa = world
                .get_resource::<magnat_economy::Unions>()
                .is_some_and(|u| u.in_dispute(*site));
            if trwa {
                Ok(())
            } else {
                Err(CommandError::NoDispute { site: *site })
            }
        }
    }
}

/// Czy firma może **zacząć** ten węzeł: nie zna go, zna wszystkie warunki wstępne
/// i nie blokuje go cudzy czynny patent.
///
/// Te same trzy warunki, którymi wybiera projekt firma AI (`rnd::progress`) —
/// gracz gra według tych samych reguł co ona (`K-11`), a nie własnych.
fn osiagalny(
    firms: &magnat_firms::Firms,
    data: &magnat_firms::RndData,
    key: magnat_firms::FirmKey,
    id: magnat_core::TechId,
    now: magnat_core::SimMinute,
) -> bool {
    let st = firms.rnd();
    !st.knows(key, id)
        && data.tree.node(id).prereqs.iter().all(|p| st.knows(key, *p))
        && st.blocked_by(key, id, now).is_none()
}

/// Czy gracz może kupić tę kampanię: zakład jego, budżet dodatni, marka jest,
/// kanał ma gdzie stanąć i nic tam jeszcze nie leci.
fn kampania(
    view: &CommandView<'_>,
    site: SiteId,
    channel: u8,
    target: u32,
    budget: Money,
    days: u16,
) -> Result<(), CommandError> {
    let firms = moj_zaklad(view, site)?;
    dodatnia(budget)?;
    if days == 0 {
        return Err(CommandError::NoAdTarget);
    }
    let key = firms
        .site(site)
        .ok_or(CommandError::SiteNotFound { site })?
        .firm;
    if magnat_supply::brand_of(magnat_firms::firm_id(key)).is_none() {
        return Err(CommandError::NoBrand);
    }
    let world = view.world.ok_or(CommandError::NoMedia)?;
    let campaigns = world
        .get_resource::<magnat_media::Campaigns>()
        .ok_or(CommandError::NoMedia)?;
    // Jedna żywa kampania na zakład — ten sam warunek, którym odsiewa się AI firm
    // (`ai::monthly`). Okno kampanii zaczyna się „teraz", więc wystarczy zapytać
    // o wszystkie, nie tylko o żywe w tej minucie.
    if campaigns.iter().any(|(_, c)| c.site == site) {
        return Err(CommandError::CampaignRunning { site });
    }
    crate::panels::brand::kanal(world, site, channel, target)
        .map(|_| ())
        .ok_or(CommandError::NoAdTarget)
}

/// Czy firma może zacząć badać ten węzeł.
fn badania(view: &CommandView<'_>, site: SiteId, tech: &str) -> Result<(), CommandError> {
    let firms = moj_zaklad(view, site)?;
    let key = firms
        .site(site)
        .ok_or(CommandError::SiteNotFound { site })?
        .firm;
    let world = view.world.ok_or(CommandError::NoRnd)?;
    let data = world
        .get_resource::<magnat_firms::RndData>()
        .ok_or(CommandError::NoRnd)?;
    let id = data
        .tree
        .id(tech)
        .ok_or_else(|| CommandError::UnknownTech { key: tech.into() })?;
    if firms.rnd().projects.contains_key(&key) {
        return Err(CommandError::ResearchBusy);
    }
    if !osiagalny(
        firms,
        data,
        key,
        id,
        magnat_core::SimMinute(view.tick.get()),
    ) {
        return Err(CommandError::TechLocked { key: tech.into() });
    }
    Ok(())
}

/// Czy gracz może złożyć to zlecenie giełdowe.
fn zlecenie(
    view: &CommandView<'_>,
    firm: u64,
    limit: Money,
    bp: u16,
    days: u16,
) -> Result<(), CommandError> {
    if !view.has_character {
        return Err(CommandError::NoCharacter);
    }
    dodatnia(limit)?;
    // Zlecenie na zero punktów bazowych albo bez terminu ważności nie jest
    // zleceniem — a arkusz przyjąłby jedno i drugie i nigdy by ich nie zestawił.
    if bp == 0 || days == 0 {
        return Err(CommandError::NotListed);
    }
    let world = view.world.ok_or(CommandError::NoEquity)?;
    let eq = world
        .get_resource::<magnat_economy::equity::Equity>()
        .ok_or(CommandError::NoEquity)?;
    if eq.listing(magnat_firms::FirmKey(firm)).is_none() {
        return Err(CommandError::NotListed);
    }
    Ok(())
}

/// Kwota musi być dodatnia. Osobno od ceny, bo komunikat jest inny.
fn dodatnia(m: Money) -> Result<(), CommandError> {
    if m.get() > 0 {
        Ok(())
    } else {
        Err(CommandError::AmountNotPositive { amount: m })
    }
}

/// Mieszkaniec istnieje i żyje.
fn zyje(view: &CommandView<'_>, c: CitizenId) -> Result<(), CommandError> {
    let ok = view.world.is_some_and(|w| {
        w.get::<magnat_agents::Identity>(c.entity())
            .is_some_and(magnat_agents::Identity::is_alive)
    });
    if ok {
        Ok(())
    } else {
        Err(CommandError::CitizenNotFound { citizen: c })
    }
}

/// Firma gracza. Pierwsza w kolejności kluczy — gracz ma najwyżej jedną, bo drugą
/// da się zdobyć wyłącznie przejęciem, a przejęcia należą do M10.
pub(crate) fn moja_firma<'a>(
    view: &CommandView<'a>,
) -> Result<(magnat_firms::FirmKey, &'a magnat_firms::Firms), CommandError> {
    let firms = view
        .world
        .and_then(magnat_ecs::World::get_resource::<magnat_firms::Firms>)
        .ok_or(CommandError::NoFirms)?;
    firms
        .iter()
        .find(|(_, f)| {
            f.owners
                .iter()
                .any(|o| o.owner == magnat_firms::Owner::Player)
        })
        .map(|(k, _)| (k, firms))
        .ok_or(CommandError::NoFirm)
}

/// Rodzaj lokalu, do którego da się wejść w tej dzielnicy.
///
/// Firma wchodzi do lokalu, który **już stoi** — budowy nie ma ani po stronie
/// gracza, ani po stronie AI (`ponytail:` w `firmlife`). Dzielnica bez wzoru lokalu
/// odrzuca komendę **przed** kliknięciem, a nie po.
fn lokal_w_dzielnicy(
    market: &Market,
    district: u16,
) -> Result<magnat_core::PlaceKind, CommandError> {
    magnat_economy::firmlife::districts_with_seed(market)
        .into_iter()
        .find(|(d, _)| *d == district)
        .map(|(_, k)| k)
        .ok_or(CommandError::NoSeedInDistrict { district })
}

/// Zakład, do którego gracz ma prawo przypiąć regułę.
///
/// Własność sprawdza się **tutaj**, a nie przy wykonaniu, bo panel ma wygasić
/// przycisk z powodem, zanim gracz kliknie — to jest cała treść jednej funkcji
/// `precheck` dla obu stron.
fn moj_zaklad<'a>(
    view: &CommandView<'a>,
    site: SiteId,
) -> Result<&'a magnat_firms::Firms, CommandError> {
    let firms = view
        .world
        .and_then(magnat_ecs::World::get_resource::<magnat_firms::Firms>)
        .ok_or(CommandError::NoFirms)?;
    let z = firms
        .site(site)
        .ok_or(CommandError::SiteNotFound { site })?;
    let moj = firms.get(z.firm).is_some_and(|f| {
        f.owners
            .iter()
            .any(|o| o.owner == magnat_firms::Owner::Player)
    });
    if moj {
        Ok(firms)
    } else {
        Err(CommandError::NotYourSite { site })
    }
}
