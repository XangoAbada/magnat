//! Operacje, które właściciel wykonuje **ręcznie** — te same, które tiery AI
//! wykonują same (M9e WP10).
//!
//! # Po co ten moduł istnieje
//!
//! Gracz jest właścicielem firmy, a nie osobnym rodzajem bytu. Zamknięcie zakładu,
//! zatrudnienie człowieka i złożenie podania o pracę mają więc iść **tą samą drogą**,
//! którą chodzą decyzje tieru taktycznego i rynku pracy — inaczej powstałyby dwie
//! implementacje jednej reguły, a rozjazd wyszedłby dopiero jako inny wynik bramki
//! (`K-11` w wydaniu operacyjnym).
//!
//! Dlatego każda z tych funkcji jest **cienka**: rozwiązuje uchwyty i woła to, co
//! już istnieje. Żadna nie wnosi własnej reguły. Gdyby wnosiła, byłaby drugą ścieżką.
//!
//! # Czego tu nie ma
//!
//! Przypisania menedżera. Menedżerem zostaje najlepszy człowiek na stanowisku
//! kierowniczym i wybiera go `hr::reconcile_managers` codziennie — komenda „postaw
//! tego" byłaby nadpisana następnej doby. Gracz stawia menedżera **zatrudniając go**
//! ([`hire`]), a steruje nim przez autonomię i politykę.

use magnat_core::{CitizenId, JobRoleId, Money, SiteId, Tick};
use magnat_ecs::World;
use magnat_firms::{Employment, Firms};

use crate::labor::{LaborHandle, LaborMarket, Workforce};
use crate::Market;

/// Zamyka zakład: załoga schodzi z etatów, półka znika, rejestr o tym wie.
///
/// To jest dokładnie ta funkcja, którą wykonuje decyzja tieru taktycznego —
/// wystawiona na zewnątrz, bo właściciel ma prawo zamknąć zakład sam.
pub fn close_site(world: &mut World, market: &Market, site: SiteId, t: Tick) {
    crate::systems::close_site(world, market, site, t);
}

/// Zatrudnia mieszkańca na stanowisko w zakładzie.
///
/// Umowa zapisuje się **po obu stronach** — w rejestrze firm i w komponencie
/// mieszkańca — tak samo jak przy doborze z rynku pracy. Zapisanie jednej strony
/// bez drugiej łamie niezmiennik „każdy `Employment` zakończony dokładnie raz".
///
/// Zwraca `false`, gdy stanowiska nie ma, wszystkie etaty są obsadzone albo
/// mieszkaniec już gdzieś pracuje.
pub fn hire(world: &mut World, site: SiteId, citizen: CitizenId, role: JobRoleId, t: Tick) -> bool {
    let Some(mut firms) = world.get_resource_mut::<Firms>().map(std::mem::take) else {
        return false;
    };
    if firms.employer_of(citizen).is_some() {
        *world.resource_mut::<Firms>() = firms;
        return false;
    }
    let dane = firms.site(site).and_then(|z| {
        z.positions
            .iter()
            .find(|p| p.role == role && p.filled.len() < usize::from(p.slots))
            .map(|p| (p.wage_band.0, p.filled.len(), z.shift_profile))
    });
    let Some((stawka, obsadzonych, profil)) = dane else {
        *world.resource_mut::<Firms>() = firms;
        return false;
    };
    // Zmiana **z grafiku zakładu**, nie dzienna (`R2-WP37`). Panel gracza o grafik
    // nie pyta i pytać nie musi: nowy pracownik wchodzi na pierwszą wolną brygadę,
    // tą samą regułą, którą obsadza zakład rynek pracy i generator miasta. Gdyby
    // była dzienna, gracz obsadzałby hutę wyłącznie na pierwszą zmianę.
    let (zmiana, dni) = magnat_agents::ShiftKind::schedule(
        profil,
        u32::from(obsadzonych.min(u16::MAX as usize) as u16),
    );
    // Drugie wyszukanie musi mieć **ten sam warunek** co pierwsze: bez sprawdzenia
    // wolnego etatu obsada przekroczyłaby `slots`, gdyby archetyp wymienił ten sam
    // zawód dwa razy — czyli dwie pensje za jedno stanowisko.
    // Gospodarstwo do umowy (`R2-WP9`) — czytane, dopóki encja mieszkańca istnieje.
    let gospodarstwo = world
        .get::<magnat_agents::Identity>(citizen.0)
        .map_or(Employment::NO_HOUSEHOLD, |id| id.household);
    let ok = firms.site_mut(site).is_some_and(|z| {
        z.positions
            .iter_mut()
            .find(|p| p.role == role && p.filled.len() < usize::from(p.slots))
            .is_some_and(|p| {
                p.filled.push(Employment::new(
                    citizen,
                    role,
                    stawka,
                    magnat_core::SimMinute(t.get()),
                    zmiana,
                    gospodarstwo,
                ));
                true
            })
    });
    *world.resource_mut::<Firms>() = firms;
    if !ok {
        return false;
    }
    {
        let mut people = crate::labor::system::WorldWorkforce::new(world);
        people.hire(citizen, site, role, zmiana, dni, stawka);
    }
    // Zatrudniony **schodzi z listy szukających pracy**, tak samo jak po doborze
    // (`matching::hire`). Bez tego zostawałby w statystyce bezrobocia i wracał
    // na listę kandydatów panelu — a gracz próbowałby zatrudnić go drugi raz.
    if let Some(m) = world
        .get_resource_mut::<LaborHandle>()
        .and_then(LaborHandle::get_mut)
    {
        m.drop_seeker(citizen);
    }
    true
}

/// Składa podanie o pracę w imieniu mieszkańca.
///
/// Podanie, nie zatrudnienie: rozstrzyga je dobór następnej doby, tym samym
/// porównaniem, które rozpatruje wszystkich pozostałych kandydatów. Gracz nie
/// dostaje przy tym ani pierwszeństwa, ani ulgi — dostaje **wejście**.
///
/// Zwraca `false`, gdy oferty nie ma, wygasła albo jest skierowana do kogoś innego.
pub fn apply_for_job(world: &mut World, offer: u64, citizen: CitizenId, t: Tick) -> bool {
    let Some(uchwyt) = magnat_core::ArenaHandle::from_bits(offer) else {
        return false;
    };
    let Some((rola, stawka)) = world
        .get_resource::<LaborHandle>()
        .and_then(LaborHandle::get)
        .and_then(|m| m.offer(uchwyt))
        .map(|o| (o.role, o.wage_month))
    else {
        return false;
    };
    let fakty = {
        let people = crate::labor::system::WorldWorkforce::new(world);
        people
            .facts(citizen)
            .map(|f| (people.skill_in(citizen, rola), f.education()))
    };
    let Some((skill, education)) = fakty else {
        return false;
    };
    // Oczekiwanie płacowe: stawka z oferty. Płaca progowa kandydata jest wiedzą
    // rynku, a nie gracza — dobór i tak porówna ją po swojemu.
    let oczekiwanie = stawka;
    let Some(m) = world
        .get_resource_mut::<LaborHandle>()
        .and_then(LaborHandle::get_mut)
    else {
        return false;
    };
    m.apply_for(
        uchwyt,
        citizen,
        oczekiwanie,
        skill,
        education,
        magnat_core::SimMinute(t.get()),
    )
}

/// Ile wolno menedżerowi tego zakładu. Zwraca `false`, gdy zakład nie jest
/// zdelegowany — autonomia bez menedżera nie ma kogo ograniczać.
pub fn set_autonomy(world: &mut World, site: SiteId, a: magnat_firms::Autonomy) -> bool {
    world
        .get_resource_mut::<Firms>()
        .and_then(|f| f.site_mut(site))
        .and_then(|z| z.delegation.as_mut())
        .is_some_and(|d| {
            d.autonomy = a;
            true
        })
}

/// Oferty pracy, o które ten mieszkaniec może się ubiegać — wejście panelu Ludzie.
///
/// Zwraca bity uchwytu, bo tyle wystarczy komendzie i tyle przeżywa zapis.
#[must_use]
pub fn open_jobs_for(world: &World, citizen: CitizenId) -> Vec<(u64, SiteId, JobRoleId, Money)> {
    world
        .get_resource::<LaborHandle>()
        .and_then(LaborHandle::get)
        .map_or_else(Vec::new, |m: &LaborMarket| {
            m.offers_for(citizen)
                .into_iter()
                .map(|(id, o)| (id.to_bits(), o.site, o.role, o.wage_month))
                .collect()
        })
}
