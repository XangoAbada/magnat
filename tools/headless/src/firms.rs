//! Most „miasto Etapu 7 → firmy M7": z nasion generatora buduje rejestr [`Firms`].
//!
//! Siostrzany do `plants` (zakłady produkcyjne M6) i `retail` (sklepy M5), i tak samo
//! jak one iteruje `city.sites.sites` po indeksie. Różnica jest jedna i istotna:
//! tamte dwa budują **fizykę** zakładu, ten buduje jego **stronę zarządczą** — kto go
//! ma, kto w nim pracuje i za ile.
//!
//! ## Skąd bierze się obsada
//!
//! Nie z `Workplace.occupant` — to pole jest zawsze `None`, także po Etapie 8, i most
//! na nim zbudowany dałby dziesięć tysięcy firm bez ani jednego pracownika. Zatrudnienie
//! Etapu 8 żyje w **ECS**: komponent `magnat_agents::Employment` mieszkańca niesie
//! `site = SITE_KEY_BASE + indeks zakładu` i `role`. Stamtąd się je czyta.
//!
//! Widełki płacowe biorą się z `Workplace.wage_band` (M2), bo to one dopasowały dochody
//! rodzin do wartości mieszkań w Etapie 8 — most, który nadałby własne, rozjechałby
//! gospodarstwa domowe z ich czynszami w pierwszym miesiącu.

use magnat_agents::{Employment as AgentEmployment, ShiftKind};
use magnat_core::{DistrictId, JobRoleId, Money, SimMinute, SiteId};
use magnat_ecs::World;
use magnat_firms::hr::employment::Employment;
use magnat_firms::{Firm, FirmKey, Firms, Owner, Site, SitePlacement, SiteTypeCatalog};

use magnat_world::city::sites::site_id as site_id_swiata;
use magnat_world::{CityData, UnitOccupant, SITE_KEY_BASE};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FirmsReport {
    pub firms: u32,
    pub sites: u32,
    /// Zakłady pominięte, bo ich archetyp nie ma typu zakładu w `data/site_types/`.
    /// Dla instytucji miejskich (szkoła, szpital) to norma — ich firmy stawia M8.
    pub skipped_municipal: u32,
    /// Zakłady pominięte z **innego** powodu niż municypalny — czyli dziura w danych.
    /// Test krzyżowy `site_types.rs` pilnuje, żeby to było zero.
    pub skipped_unknown: u32,
    pub positions: u32,
    pub hired: u32,
}

/// Buduje rejestr firm z miasta i z zaludnionego świata.
///
/// `world` jest potrzebny, bo obsada siedzi w komponentach mieszkańców; świat bez
/// populacji da firmy z pustymi etatami i to jest poprawny wynik, a nie błąd —
/// tak wygląda miasto w chwili, w której nikt jeszcze nie został zatrudniony.
pub fn zbuduj_firmy(
    city: &CityData,
    world: &mut World,
    types: &SiteTypeCatalog,
) -> (Firms, FirmsReport) {
    let mut rep = FirmsReport::default();
    let mut firms = Firms::new();

    // Indeks w `city.sites.firms` → klucz nadany przez rejestr. Generator numeruje
    // firmy od zera, rejestr od jedynki i przez licznik świata — te dwie numeracje
    // nie mają prawa być tą samą liczbą, więc mapa zamiast arytmetyki.
    let mut klucz_firmy: BTreeMap<u32, FirmKey> = BTreeMap::new();
    for (i, seed) in city.sites.firms.iter().enumerate() {
        if seed.sector.is_municipal() {
            continue;
        }
        let dzielnica = seed
            .sites
            .first()
            .and_then(|s| city.sites.sites.get(s.0.index() as usize))
            .map_or(0, |s| dzielnica_zakladu(city, s));
        let key = firms.insert(|key| {
            Firm::sole_owner(
                key,
                seed.name.clone(),
                SimMinute(0),
                DistrictId(dzielnica),
                // Właściciela-mieszkańca przypisuje M7f (powstawanie firm); do tego
                // czasu firmy zastane są w rękach zewnętrznych, a nie niczyje.
                Owner::External,
            )
        });
        klucz_firmy.insert(i as u32, key);
        rep.firms += 1;
    }

    // Kto gdzie pracuje — jeden przebieg po mieszkańcach zamiast zapytania na zakład.
    let zatrudnieni = obsada_z_ecs(world);

    for (i, s) in city.sites.sites.iter().enumerate() {
        let arch = city.site_catalog.get(s.archetype);
        let Some(&firma) = klucz_firmy.get(&s.firm.0.index()) else {
            rep.skipped_municipal += 1;
            continue;
        };
        let Some(type_id) = types.id(arch.key()) else {
            if arch.spec.sector.is_municipal() {
                rep.skipped_municipal += 1;
            } else {
                rep.skipped_unknown += 1;
            }
            continue;
        };
        // **Dwie przestrzenie identyfikatorów, jeden zakład** (`AU-1`). Generator
        // numeruje zakłady od zera i tym numerem oznacza lokale (`UnitOccupant::Site`),
        // a gospodarka — M5 (sklepy), M6 (zakłady) i komponent `Employment` mieszkańca —
        // używa klucza przesuniętego o `SITE_KEY_BASE`. `Site.id` musi być tym drugim:
        // inaczej firma i jej zakład produkcyjny są dla kodu dwoma różnymi miejscami,
        // a rynek pracy nie ma jak dopisać pokrycia etatowego do właściwej linii.
        let id = crate::plants::site_id(i);
        let id_swiata = site_id_swiata(i as u32);
        let widelki = widelki_zakladu(city, s);
        let mut site = Site::from_type(
            SitePlacement {
                id,
                building: s.building,
                district: DistrictId(dzielnica_zakladu(city, s)),
                floor_m2: powierzchnia_zakladu(city, s, id_swiata),
                opened: SimMinute(0),
            },
            firma,
            type_id,
            types.get(type_id),
            |role| widelki.get(&role).copied().unwrap_or(DOMYSLNE_WIDELKI),
        );
        rep.positions += site.positions.len() as u32;
        rep.hired += obsadz(
            &mut site,
            zatrudnieni.get(&(i as u32)).map_or(&[], Vec::as_slice),
        );
        if firms.add_site(site) {
            rep.sites += 1;
        }
    }
    (firms, rep)
}

/// Widełki, gdy zakład nie ma ani jednego `Workplace` tej roli — bo obsada z katalogu
/// M7 jest drobniejsza niż podział lokali M2 i taka rola się zdarza.
const DOMYSLNE_WIDELKI: (Money, Money) = (Money(280_000), Money(520_000));

/// Obsada per indeks zakładu, czytana z komponentów mieszkańców.
fn obsada_z_ecs(world: &mut World) -> BTreeMap<u32, Vec<(magnat_core::CitizenId, JobRoleId, u8)>> {
    let mut out: BTreeMap<u32, Vec<_>> = BTreeMap::new();
    for (e, emp) in world
        .query::<(magnat_core::Entity, &AgentEmployment), ()>()
        .iter()
    {
        if emp.site == AgentEmployment::NO_SITE || emp.site < SITE_KEY_BASE {
            continue;
        }
        out.entry(emp.site - SITE_KEY_BASE).or_default().push((
            magnat_core::CitizenId(e),
            JobRoleId(emp.role),
            emp.shift,
        ));
    }
    // Kolejność w obrębie zakładu musi być niezależna od kolejności archetypów w ECS,
    // bo z niej wychodzi kolejność wypłat, a ta wchodzi do hasha stanu.
    for v in out.values_mut() {
        v.sort_by_key(|(c, _, _)| c.0.to_bits());
    }
    out
}

/// Wpisuje zatrudnionych na stanowiska ich roli. Zwraca liczbę obsadzonych etatów.
///
/// Pracownik roli, której katalog M7 w tym zakładzie nie przewiduje, **nie znika** —
/// dostaje stanowisko dopisane na końcu. Etap 8 obsadził go wg podziału lokali M2
/// i to jest fakt o mieście, a nie błąd do wyrzucenia; rozbieżność podziałów zamyka
/// rynek pracy M7b.
fn obsadz(site: &mut Site, ludzie: &[(magnat_core::CitizenId, JobRoleId, u8)]) -> u32 {
    let mut ile = 0;
    for (c, role, shift) in ludzie {
        let idx = match site.positions.iter().position(|p| p.role == *role) {
            Some(i) => i,
            None => {
                site.positions.push(magnat_firms::Position::new(
                    *role,
                    0,
                    false,
                    DOMYSLNE_WIDELKI,
                ));
                site.positions.len() - 1
            }
        };
        let p = &mut site.positions[idx];
        // Stawka startowa to środek widełek stanowiska — licytację o pracownika
        // otwiera dopiero M7b, a do tego czasu płaca ma być przewidywalna.
        let stawka = Money((p.wage_band.0.get() + p.wage_band.1.get()) / 2);
        p.filled.push(Employment::new(
            *c,
            *role,
            stawka,
            SimMinute(0),
            zmiana(*shift),
        ));
        // Etat obsadzony poza planem katalogu podnosi liczbę etatów, a nie tworzy
        // ujemnego wakatu.
        if p.filled.len() as u16 > p.slots {
            p.slots = p.filled.len() as u16;
        }
        ile += 1;
    }
    site.positions.sort_by_key(|p| p.role);
    ile
}

fn zmiana(raw: u8) -> ShiftKind {
    match raw {
        0 => ShiftKind::Early,
        2 => ShiftKind::Afternoon,
        3 => ShiftKind::Night,
        4 => ShiftKind::Flex,
        5 => ShiftKind::Weekend,
        _ => ShiftKind::Day,
    }
}

/// Dzielnica zakładu — przez parcelę budynku, bo budynek jej nie niesie.
fn dzielnica_zakladu(city: &CityData, s: &magnat_world::city::sites::SiteSeed) -> u16 {
    let Some(b) = city.buildings.buildings.get(s.building.0.index() as usize) else {
        return 0;
    };
    city.parcels
        .parcels
        .get(b.parcel.0.index() as usize)
        .map_or(0, |p| p.district.0)
}

/// Powierzchnia **zakładu**, nie budynku.
///
/// `SiteSeed.units` obejmuje wszystkie lokale budynku, więc dla kamienicy z parterem
/// handlowym suma dałaby powierzchnię całej kamienicy razem z mieszkaniami. Etap 7
/// oznacza lokale zakładu przez `UnitOccupant::Site` i to jest jedyny wiarygodny filtr.
fn powierzchnia_zakladu(
    city: &CityData,
    s: &magnat_world::city::sites::SiteSeed,
    id: SiteId,
) -> u32 {
    let od = s.units.start as usize;
    let do_ = (s.units.end as usize).min(city.buildings.units.len());
    let suma: u32 = city.buildings.units[od.min(do_)..do_]
        .iter()
        .filter(|u| u.occupant == UnitOccupant::Site(id))
        .map(|u| u32::from(u.area_m2))
        .sum();
    // Zakład bez ani jednego oznaczonego lokalu i tak gdzieś stoi — wyrobisko, pole,
    // plac składowy. Minimum zamiast zera, żeby obsada nie wyszła pusta z arytmetyki.
    suma.max(50)
}

/// Mediana widełek per rola, z `Workplace` postawionych przez M2.
fn widelki_zakladu(
    city: &CityData,
    s: &magnat_world::city::sites::SiteSeed,
) -> BTreeMap<JobRoleId, (Money, Money)> {
    let mut out = BTreeMap::new();
    let od = s.workplaces.start as usize;
    let do_ = (s.workplaces.end as usize).min(city.buildings.workplaces.len());
    for w in &city.buildings.workplaces[od.min(do_)..do_] {
        out.entry(w.role)
            .or_insert((w.wage_band.min, w.wage_band.max));
    }
    out
}
