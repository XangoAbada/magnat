//! Sieci zewnętrzne (M7f WP15, §5.14, PRD §12.4).
//!
//! Sieć wchodzi do miasta, gdy dzielnica przekroczy progi z `data/chains/external.ron`:
//! populacja, mediana dochodu, liczba istniejących konkurentów. Wnosi **kapitał
//! spoza miasta** i to jest jedyny w tej fazie punkt, w którym pieniądza w mieście
//! przybywa bez drugiej strony w mieście.
//!
//! # Dlaczego kapitał sieci musi przejść przez rejestr emisji
//!
//! Globalny test zachowania pieniądza porównuje sumę przed i po przebiegu (00 §6).
//! Kapitał wniesiony „z zewnątrz" bez zapisu wyglądałby w nim jak pieniądz stworzony
//! z niczego i test pękłby — w miejscu odległym od przyczyny, bo pęka na sumie,
//! a nie na operacji. Dlatego idzie przez `Books::inject_external_capital`, czyli
//! przez pozycję `external_capital_in` w rejestrze podaży (`D10`).

use std::collections::BTreeMap;

use magnat_agents::{Identity, Population, Residence};
use magnat_core::{DecisionReason, DistrictId, FirmReason, Money, SimMinute, Tick};
use magnat_ecs::World;
use magnat_firms::{Firm, Firms, Owner, Site, SitePlacement, SiteTypeCatalog};

use crate::books::{AccountKind, AccountOwner, Books, ExternalInvestorId};
use crate::market::{Market, ShopSeed};

use super::FirmLifeDay;

/// Wersja schematu `data/chains/external.ron`.
pub const CHAINS_SCHEMA_VERSION: u32 = 1;

/// Jedna sieć: progi wejścia i kapitał, który wnosi.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct ChainSpec {
    /// Klucz tekstowy — kontrakt zapisu gry, tak samo jak klucz presetu polityki.
    pub key: String,
    /// Minimalna liczba mieszkańców dzielnicy.
    pub min_population: u32,
    /// Minimalna mediana majątku gospodarstwa w dzielnicy, w groszach.
    pub min_median_wealth: i64,
    /// Ile sklepów musi już w dzielnicy stać, żeby sieć uznała ją za rynek.
    pub min_competitors: u32,
    /// Ile najwyżej, żeby uznała ją za wolną.
    pub max_competitors: u32,
    /// Kapitał na jeden zakład, w groszach.
    pub capital_per_site: i64,
    /// Ile zakładów sieć otwiera przy jednym wejściu.
    pub sites: u8,
}

/// Katalog sieci z `data/chains/external.ron`.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct ChainCatalog {
    pub schema_version: u32,
    pub chains: Vec<ChainSpec>,
}

impl ChainCatalog {
    /// Wczytuje katalog z `data/chains/external.ron`.
    ///
    /// # Errors
    /// Zwraca błąd, gdy pliku nie ma, nie parsuje się albo ma inną wersję schematu.
    pub fn load_default() -> Result<ChainCatalog, String> {
        let path = magnat_core::assets::data_path("chains/external.ron");
        let txt = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let cat: ChainCatalog =
            ron::from_str(&txt).map_err(|e| format!("{}: {e}", path.display()))?;
        if cat.schema_version != CHAINS_SCHEMA_VERSION {
            return Err(format!(
                "{}: schema_version {} != {CHAINS_SCHEMA_VERSION}",
                path.display(),
                cat.schema_version
            ));
        }
        Ok(cat)
    }
}

/// Miesiąc miasta: która sieć wchodzi i gdzie.
///
/// Jedno wejście na miesiąc dla całego miasta — sieci nie wchodzą hurtem, bo wtedy
/// pasmo liczby firm z bramki mierzyłoby próg, a nie gospodarkę.
pub fn step_month(world: &mut World, market: &Market, t: Tick) -> FirmLifeDay {
    let mut d = FirmLifeDay::default();
    let Some(katalog) = world.get_resource::<ChainCatalog>().cloned() else {
        return d;
    };
    let profile = profile_dzielnic(world, market);
    let Some(typ) = typ_sklepu(world) else {
        return d;
    };

    for (i, spec) in katalog.chains.iter().enumerate() {
        // Pierwsza dzielnica spełniająca progi, w kolejności numerów — deterministycznie
        // i bez rankingu, bo sieć nie ma skąd znać rankingu (widzi to samo, co widać
        // z ulicy: ile ludzi, ile pieniędzy, ilu konkurentów).
        let Some((&dzielnica, p)) = profile.iter().find(|(_, p)| pasuje(spec, p)) else {
            continue;
        };
        let _ = p;
        let wejscie = wejdz(
            world,
            market,
            spec,
            u32::try_from(i).unwrap_or(0),
            DistrictId(dzielnica),
            typ,
            t,
        );
        if wejscie.chain_entries > 0 {
            d.chain_entries += wejscie.chain_entries;
            d.capital_in = Money(d.capital_in.get() + wejscie.capital_in.get());
            // Jedno wejście na miesiąc. Reszta sieci poczeka do następnego.
            break;
        }
    }
    d
}

/// Co sieć widzi w dzielnicy.
#[derive(Clone, Copy, Debug, Default)]
struct Profil {
    population: u32,
    median_wealth: i64,
    competitors: u32,
}

fn pasuje(s: &ChainSpec, p: &Profil) -> bool {
    p.population >= s.min_population
        && p.median_wealth >= s.min_median_wealth
        && p.competitors >= s.min_competitors
        && p.competitors <= s.max_competitors
}

fn profile_dzielnic(world: &World, market: &Market) -> BTreeMap<u16, Profil> {
    let mut out: BTreeMap<u16, Profil> = BTreeMap::new();
    let mut majatki: BTreeMap<u16, Vec<i64>> = BTreeMap::new();
    if let Some(pop) = world.get_resource::<Population>() {
        for e in pop.citizens() {
            if !world.get::<Identity>(*e).is_some_and(Identity::is_alive) {
                continue;
            }
            let d = world.get::<Residence>(*e).map_or(0, |r| r.district);
            out.entry(d).or_default().population += 1;
        }
        for h in pop.households() {
            let Some(gd) = world.get::<magnat_agents::Household>(*h) else {
                continue;
            };
            majatki
                .entry(gd.district)
                .or_default()
                .push(gd.cash.get() + gd.bank.get() + gd.savings.get());
        }
    }
    for (d, mut v) in majatki {
        v.sort_unstable();
        let mediana = v.get(v.len() / 2).copied().unwrap_or(0);
        out.entry(d).or_default().median_wealth = mediana;
    }
    for s in market.shop_seeds() {
        out.entry(s.district).or_default().competitors += 1;
    }
    out
}

fn typ_sklepu(world: &World) -> Option<magnat_firms::SiteTypeId> {
    world.get_resource::<SiteTypeCatalog>().and_then(|c| {
        c.iter()
            .find(|(_, s)| s.category == magnat_firms::SiteTypeCategory::Retail)
            .map(|(id, _)| id)
    })
}

/// Wejście sieci: firma, zakłady, kapitał z zewnątrz.
fn wejdz(
    world: &mut World,
    market: &Market,
    spec: &ChainSpec,
    indeks_sieci: u32,
    district: DistrictId,
    typ: magnat_firms::SiteTypeId,
    t: Tick,
) -> FirmLifeDay {
    let mut d = FirmLifeDay::default();
    let Some(wzor) = market
        .shop_seeds()
        .into_iter()
        .find(|s| s.district == district.0)
    else {
        return d;
    };
    let Some(katalog) = world.get_resource::<SiteTypeCatalog>().cloned() else {
        return d;
    };
    let Some(mut firms) = world.get_resource_mut::<Firms>().map(std::mem::take) else {
        return d;
    };
    let key = firms.insert(|key| {
        Firm::sole_owner(
            key,
            spec.key.clone(),
            SimMinute(t.get()),
            district,
            Owner::External,
        )
    });
    let mut nowe = Vec::new();
    for _ in 0..spec.sites.max(1) {
        let id = super::founding::nowy_site_id_z(&firms, market, &nowe);
        let site = Site::from_type(
            SitePlacement {
                id,
                building: magnat_core::BuildingId(wzor.site.0),
                district,
                floor_m2: POWIERZCHNIA,
                opened: SimMinute(t.get()),
            },
            key,
            typ,
            katalog.get(typ),
            |_| WIDELKI,
        );
        if firms.add_site(site) {
            nowe.push(id);
        }
    }
    let kapital = Money(spec.capital_per_site * i64::from(spec.sites.max(1)));
    firms.log(
        key,
        t,
        DecisionReason::Firm(FirmReason::ChainEntered {
            capital: kapital,
            sites: nowe.len() as u8,
        }),
    );
    *world.resource_mut::<Firms>() = firms;
    if nowe.is_empty() {
        return d;
    }

    let firm_id = magnat_firms::firm_id(key);
    let na_zaklad = Money(kapital.get() / nowe.len() as i64);
    // Identyfikator inwestora: numer sieci w katalogu. Stały w obrębie wersji danych,
    // bo indeksuje wpis w `data/chains/external.ron` — tak samo jak `GoodId` indeksuje
    // katalog towarów.
    let inwestor = ExternalInvestorId(indeks_sieci);
    for (i, id) in nowe.iter().enumerate() {
        let Some(konto) = world.get_resource_mut::<Books>().map(|b| {
            b.open_account(
                AccountOwner::Firm(firm_id),
                AccountKind::Current,
                None,
                Money::ZERO,
            )
        }) else {
            continue;
        };
        if !market.open_shop(
            ShopSeed {
                site: *id,
                firm: firm_id,
                pos: wzor.pos,
                kind: wzor.kind,
                shelf_slots: wzor.shelf_slots,
                capacity_m3: wzor.capacity_m3,
                district: district.0,
            },
            konto,
            t,
        ) {
            continue;
        }
        // Reszta z dzielenia do pierwszego zakładu (00 §2) — kapitał sieci ma się
        // zgadzać co do grosza z tym, co wpisał rejestr emisji.
        let kwota = if i == 0 {
            Money(na_zaklad.get() + (kapital.get() - na_zaklad.get() * nowe.len() as i64))
        } else {
            na_zaklad
        };
        if let Some(books) = world.get_resource_mut::<Books>() {
            // Powód wejścia sieci — ten sam wariant, który kronika pokazuje graczowi
            // (`FirmReason::ChainEntered`). Do `R2-WP39` stał tu `Unspecified` wpisany
            // w środku `Books`, więc bramka G9 liczyła emisję kapitału jako decyzję
            // bez powodu, a wołający nie miał czym tego naprawić.
            let powod = DecisionReason::Firm(FirmReason::ChainEntered {
                capital: kwota,
                sites: u8::try_from(nowe.len()).unwrap_or(u8::MAX),
            });
            if books
                .inject_external_capital(konto, kwota, inwestor, powod, t)
                .is_ok()
            {
                d.capital_in = Money(d.capital_in.get() + kwota.get());
                market.record_capital(*id, kwota, t);
            }
        }
    }
    d.chain_entries = 1;
    d
}

/// Powierzchnia lokalu sieci w metrach — wchodzi wyłącznie do kosztu stałego.
const POWIERZCHNIA: u32 = 220;

/// Widełki płacowe sieci: te same co wszędzie indziej, gdzie rola nie ma własnego
/// `Workplace`. Sieć płacąca lepiej od miejscowych to decyzja, której nikt jeszcze
/// nie podejmuje — jej miejsce jest w polityce kadrowej, nie w moście wejścia.
const WIDELKI: (Money, Money) = (Money(280_000), Money(520_000));
