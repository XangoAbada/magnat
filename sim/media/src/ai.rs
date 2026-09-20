//! Kto kupuje reklamę i kto wydaje gazetę (M10b §5.2, §5.3).
//!
//! **Mechanizm bez wołającego jest martwy** — to jest wniosek zapisany w dzienniku
//! po M9e i dlatego ten moduł istnieje. Kampania, której nikt nigdy nie otwiera,
//! przechodzi każdy test jednostkowy i wygląda tak samo jak działająca.
//!
//! Decyzję podejmuje **`magnat_media`, a nie `sim/firms`**, i to jest ta sama droga,
//! którą `K-54` przeprowadził tier strategiczny: repertuar akcji firmy mieszka
//! w `sim/firms`, ale `sim/firms` stoi **pod** tym crate'em i o kampaniach nie wie.
//! Widzi je ten, kto widzi naraz rejestr firm, rynek i pamięć mieszkańców.

use crate::campaign::{AdCampaign, AdChannel, CampaignMetrics, Campaigns};
use crate::outlet::{MediaOutlet, Outlets};
use magnat_core::{
    DistrictId, EditorialBias, FirmStrategy, MediaKind, Money, SimMinute, SiteId, Tick, Q,
};
use magnat_ecs::World;

/// Klucze rodzajów zakładu z `data/site_types/media.ron`, które są tytułami.
///
/// Kolejność odpowiada [`MediaKind`]; klucz tekstowy, bo `SiteTypeId` jest indeksem
/// w katalogu i zmienia się z jego zawartością, a klucz jest kontraktem danych.
const KLUCZE_TYTULOW: [(&str, MediaKind, EditorialBias); 4] = [
    ("newspaper", MediaKind::Newspaper, EditorialBias::Local),
    ("radio_station", MediaKind::Radio, EditorialBias::Social),
    ("tv_station", MediaKind::Tv, EditorialBias::Sensational),
    ("news_portal", MediaKind::Portal, EditorialBias::Sensational),
];

/// Zamienia zakłady medialne postawione przez generator w tytuły z czytelnictwem.
///
/// Wołane raz, przy stawianiu świata. Miasto bez zakładu medialnego dostaje pusty
/// rejestr i to jest poprawny stan: w małej mieścinie nie ma gazety.
pub fn stand_up_outlets(world: &mut World) {
    let Some(firms) = world.get_resource::<magnat_firms::Firms>() else {
        return;
    };
    let Some(katalog) = world.get_resource::<magnat_firms::SiteTypeCatalog>() else {
        return;
    };
    let dane = world.resource::<magnat_agents::BrandData>().outlets.clone();
    let tytuly: Vec<(SiteId, MediaKind, EditorialBias, DistrictId)> = firms
        .sites()
        .filter_map(|(id, s)| {
            let klucz = katalog.get(s.site_type).key.as_str();
            let (_, kind, bias) = KLUCZE_TYTULOW.iter().find(|(k, _, _)| *k == klucz)?;
            Some((id, *kind, *bias, s.district))
        })
        .collect();
    if tytuly.is_empty() {
        return;
    }

    // Dzielnice, w których ktoś mieszka — tylko tam czytelnictwo ma sens.
    let dzielnice: Vec<DistrictId> = {
        let spis = crate::reach::DistrictRoster::build(world);
        spis.districts().collect()
    };

    for (site, kind, bias, moja) in tytuly {
        // Czytelnictwo **nie jest losowane**: tytuł czyta się najlepiej tam, gdzie
        // ma redakcję, i dwa razy słabiej poza nią. Osobny strumień `StreamId` na
        // rozrzut czytelnictwa zapisałby na wieczność liczbę, której nikt nie stroi;
        // kalibrację niesie `data/tuning/brand.ron`, a różnice między dzielnicami
        // wnosi M10f razem z kartą tytułu.
        let baza = match kind {
            MediaKind::Newspaper => 300u16,
            MediaKind::Radio => 220,
            MediaKind::Tv => 450,
            MediaKind::Portal => 120,
        };
        let readership: Vec<(DistrictId, u16)> = dzielnice
            .iter()
            .map(|d| {
                let p = if *d == moja { baza } else { baza / 2 };
                (*d, p)
            })
            .collect();
        world.resource_mut::<Outlets>().insert(MediaOutlet {
            site,
            kind,
            readership,
            credibility: Q::new(dane.base_credibility),
            bias,
            inventory_daily: dane.inventory_daily[kind.as_index()],
            inventory_left: dane.inventory_daily[kind.as_index()],
            slot_price: dane.slot_price(kind),
        });
    }
}

/// Ile najwyżej firma przeznacza na jedną kampanię — ułamek salda, w punktach bazowych.
///
/// Trzy procent, bo to jest ta wielkość, przy której kampania jest zauważalna
/// i nie wywraca płynności. Liczba mieszka tutaj, a nie w `data/tuning/brand.ron`,
/// bo jest **decyzją firmy**, nie kalibracją modelu marki — kiedy M10f da graczowi
/// panel marketingu, ta sama liczba stanie się suwakiem, a nie wpisem w pliku.
const BUDZET_BP: i64 = 300;

/// Próg salda, poniżej którego firma reklamy nie kupuje.
const PROG_GOTOWKI: i64 = 200_000;

/// Raz na miesiąc: firmy AI decydują, czy kupić kampanię.
///
/// Repertuar jest krótki i to jest zamierzone: kanał wynika z zasobności, obietnica
/// z kursu firmy. Firma ostrożna nie reklamuje się wcale, firma innowacyjna obiecuje
/// więcej, niż ma — i płaci za to afinitetem, kiedy klient spróbuje (§5.1).
pub fn monthly(world: &mut World, t: Tick) -> u32 {
    let Some(market) = world.get_resource::<magnat_economy::Market>().cloned() else {
        return 0;
    };
    let Some(firms) = world.get_resource::<magnat_firms::Firms>() else {
        return 0;
    };
    // Zakłady handlowe z firmą i z kontem — tylko one mają czym zapłacić i co reklamować.
    let kandydaci: Vec<(SiteId, magnat_firms::FirmKey)> = market
        .sites()
        .into_iter()
        .filter_map(|s| firms.site(s).map(|site| (s, site.firm)))
        .collect();
    if kandydaci.is_empty() {
        return 0;
    }

    let now = SimMinute(t.0);
    let zajete: Vec<SiteId> = world
        .resource::<Campaigns>()
        .live(now)
        .map(|c| c.site)
        .collect();
    let tytuly: Vec<(SiteId, MediaKind)> = world
        .resource::<Outlets>()
        .iter()
        .map(|(s, o)| (*s, o.kind))
        .collect();

    let mut otwarte = 0u32;
    for (site, key) in kandydaci {
        if zajete.contains(&site) {
            continue;
        }
        let Some(firma) = world
            .get_resource::<magnat_firms::Firms>()
            .and_then(|f| f.get(key))
            .map(|f| (f.strategy, f.personality))
        else {
            continue;
        };
        let (kurs, osobowosc) = firma;
        if kurs == FirmStrategy::Cautious {
            continue;
        }
        let Some(saldo) = market
            .account_of(site)
            .and_then(|a| world.get_resource::<magnat_economy::Books>()?.balance(a))
        else {
            continue;
        };
        if saldo.get() < PROG_GOTOWKI {
            continue;
        }
        let budzet = Money(saldo.get() * BUDZET_BP / 10_000);
        let Some(brand) = magnat_supply::brand_of(magnat_firms::firm_id(key)) else {
            continue;
        };

        // Kanał z zasobności: kto ma mało, roznosi ulotki; kto ma dużo, kupuje spot.
        // Tytuł bierzemy pierwszy pasujący — wybór tytułu jest decyzją, której dziś
        // nikt nie ma czym podjąć (karta tytułu jest w M10f).
        let kanal = if budzet.get() >= 50_000_000 {
            tytuly
                .iter()
                .find(|(_, k)| *k == MediaKind::Tv)
                .map(|(s, _)| AdChannel::Tv { outlet: *s })
        } else if budzet.get() >= 10_000_000 {
            tytuly
                .iter()
                .find(|(_, k)| *k == MediaKind::Newspaper)
                .map(|(s, _)| AdChannel::Press { outlet: *s })
        } else {
            None
        }
        .unwrap_or(AdChannel::Leaflet {
            origin: site,
            radius_m: world
                .resource::<magnat_agents::BrandData>()
                .channels
                .leaflet_radius_m,
        });

        // Obietnica: kurs firmy plus jej skłonność do przesady. `Innovative`
        // i `AggressiveExpansion` obiecują najwięcej i najczęściej się na tym przejeżdżają.
        let baza = 55i32;
        let dodatek = match kurs {
            FirmStrategy::NicheQuality => 20,
            FirmStrategy::Innovative | FirmStrategy::AggressiveExpansion => 30,
            FirmStrategy::Discount => 5,
            _ => 10,
        };
        let przesada = i32::from(osobowosc.aggression) / 10;
        let claim = Q::new((baza + dodatek + przesada).clamp(0, 100) as u8);

        world.resource_mut::<Campaigns>().open(|id| AdCampaign {
            id,
            site,
            brand,
            channel: kanal,
            claim,
            budget: budzet,
            spent: Money::ZERO,
            // Miesiąc emisji: trzydzieści dób kalendarza `K-1`.
            window: (now, SimMinute(now.get() + 30 * 1_440)),
            metrics: CampaignMetrics::default(),
        });
        otwarte += 1;
    }
    otwarte
}
