//! Wykonanie komend, które dotykają świata (M9e WP10).
//!
//! # Dlaczego osobno od `precheck`
//!
//! [`super::precheck`] widzi [`CommandView`](super::CommandView) — rynek, świat
//! do odczytu i jedną flagę. Tyle wystarczy, żeby panel wygasił przycisk **z
//! powodem**, i tyle ma wystarczać: walidacja, która dostaje `&mut World`, prędzej
//! czy później zacznie coś po drodze zmieniać.
//!
//! Wykonanie potrzebuje więcej: rejestru firm, ksiąg, rynku pracy i strony
//! publicznej — na zapis. Stoi więc tutaj, w jednym miejscu, i woła je **sesja**
//! w punkcie synchronizacji na początku ticku (00 §3.4).
//!
//! # Jedna droga dla gracza i dla AI
//!
//! Żaden z tych wykonawców nie wnosi własnej reguły. Zakładanie firmy idzie przez
//! `firmlife::found`, otwieranie punktu przez `firmlife::expand::open_all`,
//! zamknięcie, zatrudnienie i podanie o pracę przez `magnat_economy::owner_ops`,
//! kredyt przez `Market::request_working_capital`, wpłata na kampanię przez
//! `Election::back_candidate`. To jest `K-11` zastosowane poza silnikiem reguł:
//! gracz naciska to samo, co AI robi samo.

use magnat_core::{DistrictId, Money, SiteId, Tick};
use magnat_economy::{firmlife, owner_ops, Books, TxKind, TxMemo};

use super::check::moja_firma;
use super::{CommandError, PlayerCommand};
use crate::Session;

/// Ile etatów ma nowy punkt. Tyle, ile mieści wzorcowy lokal handlowy — liczba
/// stoi tu, a nie w kopercie komendy, bo gracz otwiera **sklep**, a nie etaty.
const ETATY_NOWEGO_PUNKTU: u32 = 4;

/// Wykonuje komendę zmieniającą świat.
///
/// Wołane **wyłącznie** z [`Session`] po udanym `precheck`, w punkcie synchronizacji.
///
/// # Errors
/// To samo co `precheck` — świat mógł się zmienić między sprawdzeniem a tickiem.
/// `ponytail:` sufit ten sam co przy `precheck` i z tego samego powodu: ramię
/// na komendę, wyczerpująco. Dłuższe wykonania już teraz wychodzą do funkcji
/// niżej, więc rośnie **o jedną linię** na komendę, a nie o wykonanie.
pub(crate) fn run(s: &mut Session, t: Tick, cmd: &PlayerCommand) -> Result<(), CommandError> {
    super::precheck(&s.view(), cmd)?;
    match cmd {
        PlayerCommand::StartGame { .. } => Ok(()),
        PlayerCommand::SetPrice { .. } => super::apply(&s.view(), cmd),
        PlayerCommand::SetCharacter { citizen } => {
            let wariant = s.variant();
            let postac = crate::player::take_role(
                &mut s.app.world,
                s.market.as_ref(),
                wariant,
                *citizen,
                t,
            )
            .ok_or(CommandError::CitizenNotFound { citizen: *citizen })?;
            *s.player_mut() = Some(postac);
            Ok(())
        }
        PlayerCommand::SetAutonomy { field, control } => {
            let p = s.player_mut().as_mut().ok_or(CommandError::NoCharacter)?;
            p.autonomy.set(*field, *control);
            Ok(())
        }
        PlayerCommand::AttachPolicy { site, policy } => attach_policy(s, site, policy),
        PlayerCommand::DetachPolicy { site } => {
            let firms = s
                .app
                .world
                .get_resource_mut::<magnat_firms::Firms>()
                .ok_or(CommandError::NoFirms)?;
            let z = firms
                .site_mut(*site)
                .ok_or(CommandError::SiteNotFound { site: *site })?;
            z.delegation = None;
            Ok(())
        }
        PlayerCommand::FoundFirm { district, capital } => found_firm(s, *district, *capital, t),
        PlayerCommand::OpenSite { district, capex } => open_site(s, *district, *capex, t),
        PlayerCommand::CloseSite { site } => {
            let m = s.market.clone().ok_or(CommandError::NoMarket)?;
            owner_ops::close_site(&mut s.app.world, &m, *site, t);
            if let Some(p) = s.player_mut().as_mut() {
                p.owned_sites.retain(|x| x != site);
            }
            Ok(())
        }
        PlayerCommand::SetShelfAssortment { site, goods } => {
            let m = s.market.as_ref().ok_or(CommandError::NoMarket)?;
            let mut g = Vec::with_capacity(goods.len());
            for k in goods {
                g.push(super::resolve_good(m, k)?);
            }
            // `None` znaczy zakład zamknięty albo rynkowi nieznany — w obu wypadkach
            // komenda nie zrobiła tego, o co prosiła, i ma to powiedzieć. Sąsiedni
            // `SetRestockTarget` odpowiada tak samo na tę samą klasę błędu.
            if m.set_assortment(*site, &g, t).is_none() {
                return Err(CommandError::SiteNotFound { site: *site });
            }
            Ok(())
        }
        PlayerCommand::SetRestockTarget { site, good, days } => {
            let m = s.market.as_ref().ok_or(CommandError::NoMarket)?;
            let g = super::resolve_good(m, good)?;
            if m.set_restock_days(*site, g, *days) {
                Ok(())
            } else {
                Err(CommandError::SiteNotFound { site: *site })
            }
        }
        PlayerCommand::HireCandidate {
            site,
            citizen,
            role,
        } => {
            if owner_ops::hire(
                &mut s.app.world,
                *site,
                *citizen,
                magnat_core::JobRoleId(*role),
                t,
            ) {
                Ok(())
            } else {
                Err(CommandError::NoVacancy {
                    site: *site,
                    role: *role,
                })
            }
        }
        PlayerCommand::SetDelegationAutonomy { site, autonomy } => {
            if owner_ops::set_autonomy(&mut s.app.world, *site, autonomy.to_firms()) {
                Ok(())
            } else {
                Err(CommandError::NoPolicy { site: *site })
            }
        }
        PlayerCommand::ApplyForJob { offer } => {
            let c = s.player().ok_or(CommandError::NoCharacter)?.citizen;
            if owner_ops::apply_for_job(&mut s.app.world, *offer, c, t) {
                Ok(())
            } else {
                Err(CommandError::NoJobOffer { offer: *offer })
            }
        }
        PlayerCommand::TakeLoan { site } => take_loan(s, *site, t),
        PlayerCommand::BackCandidate { candidate, amount } => {
            back_candidate(s, *candidate, *amount, t)
        }
        PlayerCommand::ApplyForPermit { kind } => {
            let rodzaj = magnat_core::PermitKind::ALL
                .get(usize::from(*kind))
                .copied()
                .ok_or(CommandError::UnknownPermit { kind: *kind })?;
            let city = s
                .app
                .world
                .get_resource_mut::<magnat_city::City>()
                .ok_or(CommandError::NoCity)?;
            let strojenie = city.tuning.clone();
            let t_ref = strojenie.0.as_ref().ok_or(CommandError::NoCity)?;
            city.permits
                .file(magnat_city::Applicant::Player, rodzaj, t_ref, t)
                .map(|_| ())
                .ok_or(CommandError::NoCity)
        }
        PlayerCommand::DeclarePersonalBankruptcy => {
            likwiduj(s, t);
            let p = s.player_mut().as_mut().ok_or(CommandError::NoCharacter)?;
            // Autonomia wraca na ręczną: gracz, który stracił firmę, znowu sam
            // decyduje, gdzie pracuje. Reputację widzi bank przez **zaległości**
            // pozostałych kredytów, a nie przez osobne pole (`DI-4`).
            p.autonomy
                .set(crate::AutonomyField::Job, crate::Control::Manual);
            p.bankruptcies = p.bankruptcies.saturating_add(1);
            Ok(())
        }
        PlayerCommand::SetHeir { citizen } => {
            let p = s.player_mut().as_mut().ok_or(CommandError::NoCharacter)?;
            p.heir = Some(*citizen);
            Ok(())
        }
        PlayerCommand::Succeed { citizen } => {
            let poprzednik = s.player().ok_or(CommandError::NoCharacter)?.citizen;
            if crate::legacy::heir_of(s, poprzednik) != Some(*citizen)
                && s.player().and_then(|p| p.heir) != Some(*citizen)
            {
                return Err(CommandError::NotAnHeir { citizen: *citizen });
            }
            przejmij(s, *citizen, t)
        }
        PlayerCommand::ContinueAsNewCitizen { citizen } => {
            // Nowej dynastii nikt nie zapisał firm — idą do likwidacji.
            likwiduj(s, t);
            przejmij(s, *citizen, t)
        }
    }
}

/// Zamyka wszystkie zakłady gracza. Zobowiązania **zostają**: kredyt nie znika
/// razem ze sklepem i to on jest treścią zdania „bank to widzi".
fn likwiduj(s: &mut Session, t: Tick) {
    let Some(m) = s.market.clone() else { return };
    let zaklady = crate::career::Holdings::of(s).sites;
    for site in zaklady {
        owner_ops::close_site(&mut s.app.world, &m, site, t);
    }
    if let Some(p) = s.player_mut().as_mut() {
        p.owned_sites.clear();
    }
}

/// Nowa postać przejmuje rolę gracza.
///
/// Znacznik gracza schodzi ze starej postaci i siada na nowej, a `owned_sites`
/// odbudowuje się z rejestru firm — własność jest tam, a nie na liście w postaci.
fn przejmij(s: &mut Session, citizen: magnat_core::CitizenId, t: Tick) -> Result<(), CommandError> {
    let stary = s.player().map(|p| p.citizen);
    if let Some(c) = stary {
        if let Some(x) = s
            .app
            .world
            .get_mut::<magnat_agents::Identity>(c.entity())
        {
            x.flags &= !magnat_agents::Identity::FLAG_PLAYER;
        }
    }
    let wariant = s.variant();
    // Dziedzic **nie dostaje kapitału startowego**: majątek już jest w firmach
    // i w gospodarstwie. Drugi zastrzyk byłby pieniądzem z niczego.
    let mut postac = crate::player::take_role(&mut s.app.world, None, wariant, citizen, t)
        .ok_or(CommandError::CitizenNotFound { citizen })?;
    postac.owned_sites = crate::career::Holdings::of(s).sites;
    postac.capital = magnat_core::Money::ZERO;
    *s.player_mut() = Some(postac);
    Ok(())
}

/// Przypięcie polityki do zakładu.
///
/// Włącza przy okazji **śledzenie zakładu**: bez niego nie ma śladu doby, a bez
/// śladu dry-run nie ma na czym pracować (`Z-3` fazy M5e — poziom śledzenia idzie
/// za własnością, a nie za otwartym oknem).
fn attach_policy(
    s: &mut Session,
    site: &SiteId,
    policy: &magnat_policy::Policy,
) -> Result<(), CommandError> {
    if let Some(m) = s.market.as_ref() {
        m.set_tracking(*site, magnat_economy::LostSaleTracking::Full);
    }
    let firms = s
        .app
        .world
        .get_resource_mut::<magnat_firms::Firms>()
        .ok_or(CommandError::NoFirms)?;
    let z = firms
        .site_mut(*site)
        .ok_or(CommandError::SiteNotFound { site: *site })?;
    match z.delegation.as_mut() {
        // Zakład, który już ma menedżera, dostaje **nową regułę**, a nie nowe
        // pełnomocnictwo: gracz zmienia politykę, a nie zwalnia człowieka.
        Some(d) => d.policy = policy.clone(),
        None => {
            z.delegation = Some(magnat_firms::SiteDelegation {
                manager: None,
                policy: policy.clone(),
                autonomy: magnat_firms::Autonomy::Full,
                report_freq: magnat_core::Cadence::EveryMonth,
                last_run: Tick(0),
            });
        }
    }
    Ok(())
}

fn found_firm(
    s: &mut Session,
    district: u16,
    capital: Money,
    t: Tick,
) -> Result<(), CommandError> {
    let m = s.market.clone().ok_or(CommandError::NoMarket)?;
    let citizen = s.player().ok_or(CommandError::NoCharacter)?.citizen;
    let kind = firmlife::districts_with_seed(&m)
        .into_iter()
        .find(|(d, _)| *d == district)
        .map(|(_, k)| k)
        .ok_or(CommandError::NoSeedInDistrict { district })?;
    let typ = firmlife::retail_site_type(&s.app.world).ok_or(CommandError::NoFirms)?;
    let zamiar = firmlife::FoundingIntent {
        owner: magnat_firms::Owner::Player,
        citizen,
        district,
        kind,
        capital,
        score: 0,
    };
    let f = firmlife::found(&mut s.app.world, &m, &zamiar, typ, t).ok_or(
        CommandError::NoSeedInDistrict { district },
    )?;
    // Kapitał **faktycznie wniesiony** może być mniejszy od żądanego, bo gospodarstwo
    // mogło wydać część oszczędności. Firma wtedy powstaje, tylko chudsza — i to jest
    // poprawny wynik, a nie błąd komendy.
    m.set_tracking(f.site, magnat_economy::LostSaleTracking::Full);
    if let Some(p) = s.player_mut().as_mut() {
        p.owned_sites.push(f.site);
    }
    Ok(())
}

fn open_site(s: &mut Session, district: u16, capex: Money, t: Tick) -> Result<(), CommandError> {
    let m = s.market.clone().ok_or(CommandError::NoMarket)?;
    let (key, firms) = moja_firma(&s.view())?;
    let przed: Vec<SiteId> = firms.get(key).map_or_else(Vec::new, |f| f.sites.to_vec());
    firmlife::expand::open_all(
        &mut s.app.world,
        &m,
        &[(key, DistrictId(district), ETATY_NOWEGO_PUNKTU, capex)],
        t,
    );
    let po: Vec<SiteId> = s
        .app
        .world
        .get_resource::<magnat_firms::Firms>()
        .and_then(|f| f.get(key))
        .map_or_else(Vec::new, |f| f.sites.to_vec());
    let Some(nowy) = po.into_iter().find(|x| !przed.contains(x)) else {
        return Err(CommandError::NoSeedInDistrict { district });
    };
    m.set_tracking(nowy, magnat_economy::LostSaleTracking::Full);
    if let Some(p) = s.player_mut().as_mut() {
        p.owned_sites.push(nowy);
    }
    Ok(())
}

fn take_loan(s: &mut Session, site: SiteId, t: Tick) -> Result<(), CommandError> {
    let m = s.market.clone().ok_or(CommandError::NoMarket)?;
    let mut books = s
        .app
        .world
        .get_resource_mut::<Books>()
        .map(std::mem::take)
        .ok_or(CommandError::NoMarket)?;
    let wynik = m.request_working_capital(&mut books, site, t);
    *s.app.world.resource_mut::<Books>() = books;
    wynik
        .map(|_| ())
        .map_err(|cause| CommandError::CreditRefused { cause })
}

/// Wpłata na kampanię (`K-66`).
///
/// **Najpierw przelew, potem wpis** — ta sama kolejność co po stronie firm
/// (`sim/city::ballot`): kampania nie ma prawa dostać pieniądza, który nie wyszedł
/// z gospodarstwa. Odbiorcą jest reszta świata, bo media są firmami dopiero w M10.
fn back_candidate(
    s: &mut Session,
    candidate: u8,
    amount: Money,
    t: Tick,
) -> Result<(), CommandError> {
    let m = s.market.clone().ok_or(CommandError::NoMarket)?;
    let hh = s.player().ok_or(CommandError::NoCharacter)?.household;
    let saldo = s
        .app
        .world
        .get::<magnat_agents::Household>(hh.0)
        .map_or(Money::ZERO, |h| h.bank);
    if saldo.get() < amount.get() {
        return Err(CommandError::NotEnoughCash {
            need: amount,
            have: saldo,
        });
    }
    // **Najpierw sprawdzenie kandydata, potem przelew.** Odwrotna kolejność
    // wyprowadzała gotówkę z gospodarstwa i dopiero potem odkrywała, że kampanii
    // nie ma — a pieniądz był już po drugiej stronie, bez wpisu u odbiorcy.
    let jest = s
        .app
        .world
        .get_resource::<magnat_city::City>()
        .and_then(|c| c.election.as_ref())
        .is_some_and(|e| usize::from(candidate) < e.candidates.len());
    if !jest {
        return Err(CommandError::NoCandidate { candidate });
    }
    let powod = magnat_core::DecisionReason::CampaignBacked {
        candidate,
        amount,
        illegal: false,
    };
    let memo = TxMemo::new(
        TxKind::CampaignDonation {
            candidate,
            illegal: false,
        },
        powod,
    );
    let rest = m.rest_of_world();
    let ok = s
        .app
        .world
        .get_resource_mut::<Books>()
        .is_some_and(|b| b.household_pay(rest, amount, memo, t).is_ok());
    if !ok {
        return Err(CommandError::NotEnoughCash {
            need: amount,
            have: saldo,
        });
    }
    if let Some(h) = s.app.world.get_mut::<magnat_agents::Household>(hh.0) {
        h.bank = Money(h.bank.get() - amount.get());
    }
    let city = s
        .app
        .world
        .get_resource_mut::<magnat_city::City>()
        .ok_or(CommandError::NoCity)?;
    let e = city.election.as_mut().ok_or(CommandError::NoElection)?;
    e.back_candidate(
        usize::from(candidate),
        magnat_city::Backer::Player,
        amount,
        magnat_city::Legality::Legal,
    )
    .ok_or(CommandError::NoCandidate { candidate })?;
    Ok(())
}
