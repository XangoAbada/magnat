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
/// `ponytail:` jedna funkcja na **wszystkie** komendy i tak ma zostać. Sufit:
/// rośnie o kilka linii z każdym nowym wariantem i przekroczy 250 gdzieś przy
/// trzydziestu komendach. Ścieżka wyjścia: rozbicie po **dziedzinie** komendy
/// (gospodarka, kadry, miasto, dynastia), a nie po sztuce — wyczerpujący `match`
/// jest tu jedynym mechanizmem, który łapie komendę bez sprawdzenia.
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
            if firms
                .site(*site)
                .is_some_and(|s| s.delegation.is_some())
            {
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
    }
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
    let z = firms.site(site).ok_or(CommandError::SiteNotFound { site })?;
    let moj = firms
        .get(z.firm)
        .is_some_and(|f| f.owners.iter().any(|o| o.owner == magnat_firms::Owner::Player));
    if moj {
        Ok(firms)
    } else {
        Err(CommandError::NotYourSite { site })
    }
}
