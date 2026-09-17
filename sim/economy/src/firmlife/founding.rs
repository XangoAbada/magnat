//! Powstawanie firm (M7f WP15, §5.14, PRD §5.6).
//!
//! # Nisza z wiedzy mieszkańca, nie z wyroczni
//!
//! PRD §5.7 mówi wprost: przedsiębiorca wykrywa niszę w **znanej sobie okolicy**,
//! a nie w tabeli całego miasta. Dlatego kandydat patrzy wyłącznie na swoją
//! dzielnicę i wyłącznie na to, co z ulicy widać — ile sklepów którego rodzaju
//! w niej stoi i ilu ludzi w niej mieszka. Nie widzi cudzych kosztów, cudzych
//! marż ani cudzych planów; to ta sama granica, którą trzyma `FirmView` (§5.8).
//!
//! # Limit dzienny nie jest ostrożnością, tylko mechanizmem
//!
//! Bez niego każdy szok popytowy rodziłby setkę firm w jednej dobie, a potem setkę
//! upadłości kwartał później (`R5`). Limit skaluje się populacją i **wybiera
//! najlepszych**, a nie pierwszych z brzegu: sortowanie idzie po wyniku malejąco,
//! remis rozstrzyga indeks mieszkańca. Dwa przebiegi tego samego ziarna dają tę
//! samą listę założycieli.

use std::collections::BTreeMap;

use magnat_agents::{Household, Identity, Personality, Population, Residence};
use magnat_core::{CitizenId, DecisionReason, Money, PlaceKind, SimMinute, Tick, TraitId};
use magnat_ecs::World;
use magnat_firms::{Firm, Firms, Owner, Site, SitePlacement, SiteTypeCatalog, SiteTypeCategory};

use crate::books::{AccountKind, AccountOwner, Books, TxKind, TxMemo};
use crate::market::{Market, ShopSeed};

use super::FirmLifeDay;

/// Parametry powstawania firm.
///
/// Nie ma ich w `data/tuning/`, i to jest świadome: to **nie** są liczby, które
/// wolno przestawić bez zmiany znaczenia modelu (`K-35`). Próg ambicji i minimalny
/// kapitał rozstrzygają, kto w ogóle może zostać przedsiębiorcą — czyli kształt
/// rozwiązania, a nie jego kalibrację. Gdyby kiedyś balansator miał je stroić,
/// wtedy i tylko wtedy przeniosą się do danych.
#[derive(Clone, Copy, Debug)]
pub struct FoundingParams {
    /// Minimalna ambicja kandydata, 0..=100.
    pub min_ambition: u8,
    /// Minimalna skłonność do ryzyka, 0..=100.
    pub min_risk: u8,
    /// Ile firm może powstać na 100 tys. mieszkańców na dobę.
    pub per_100k_per_day: u32,
    /// Ułamek oszczędności gospodarstwa, który założyciel wnosi, w punktach bazowych.
    pub stake_bp: i32,
    /// Poniżej tej kwoty wniesionego kapitału firma nie powstaje.
    pub min_capital: Money,
    /// Ilu mieszkańców musi przypadać na jeden sklep tego rodzaju w dzielnicy,
    /// żeby mieszkaniec zobaczył tam niszę.
    ///
    /// **To jest ogranicznik, na którym stoi bramka G10**, i warto powiedzieć wprost,
    /// dlaczego nie jest nim limit dzienny. Limit dzienny chroni przed **pikiem** —
    /// setką firm po jednym szoku popytowym (`R5`). Nie chroni przed **dryfem**:
    /// jedna firma na dobę to trzysta sześćdziesiąt rocznie i przy dwustu firmach
    /// startowych miasto podwaja się w pół roku, bez żadnego szoku. Zmierzone
    /// w pierwszym przebiegu WP17: 215 → 271 firm w 150 dób, czyli +62 % rocznie.
    ///
    /// Nisza jest natomiast **wyczerpywalna**: każdy nowy sklep obniża liczbę ludzi
    /// przypadających na sklep, więc powstawanie firm samo się zatrzymuje, kiedy
    /// dzielnica jest obsłużona. To jest sprzężenie zwrotne, a nie sufit — i dlatego
    /// pasmo liczby firm z §7.10 domyka się bez żadnej stałej „ile firm wolno".
    pub min_people_per_shop: u32,
}

impl Default for FoundingParams {
    fn default() -> FoundingParams {
        FoundingParams {
            min_ambition: 62,
            min_risk: 55,
            per_100k_per_day: 6,
            stake_bp: 7_000,
            min_capital: Money(800_000),
            min_people_per_shop: 350,
        }
    }
}

/// Zamiar założenia firmy — co, gdzie i za ile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FoundingIntent {
    pub citizen: CitizenId,
    pub district: u16,
    pub kind: PlaceKind,
    pub capital: Money,
    /// Wynik oceny w setnych — ładunek `DecisionReason::FirmFounded`.
    pub score: u16,
}

/// Ocena kandydata: ambicja × ryzyko × kapitał, w setnych.
///
/// Funkcja czysta i **bez losowania** — remisy rozstrzyga indeks mieszkańca,
/// a nie strumień RNG. Losowanie tutaj nie kupiłoby nic poza kolejnym numerem
/// `StreamId`, który byłby wieczny (`K-4`), i kolejnym miejscem, w którym dwa
/// przebiegi mogą się rozjechać.
#[must_use]
pub fn founding_score(ambition: u8, risk: u8, capital: Money, p: &FoundingParams) -> Option<u16> {
    if ambition < p.min_ambition || risk < p.min_risk || capital < p.min_capital {
        return None;
    }
    let baza = u32::from(ambition) * u32::from(risk) / 100;
    // Kapitał ponad minimum dokłada, ale z malejącą wagą: dwukrotnie bogatszy
    // kandydat nie jest dwukrotnie lepszym przedsiębiorcą.
    let zapas = (capital.get() / p.min_capital.get().max(1)).clamp(1, 8) as u32;
    u16::try_from(baza * (8 + zapas) / 8).ok()
}

/// Skan dobowy. Zwraca liczby, a firmy zakłada po drodze.
pub fn scan_day(world: &mut World, market: &Market, t: Tick) -> FirmLifeDay {
    let mut d = FirmLifeDay::default();
    let params = world
        .get_resource::<FoundingParams>()
        .copied()
        .unwrap_or_default();
    let Some(katalog) = world.get_resource::<SiteTypeCatalog>() else {
        // Bez katalogu typów zakładów nie ma czego założyć. To jest stan scenariusza
        // bez firm, a nie błąd — `m5shop` do M7f tak właśnie działał.
        return d;
    };
    let nisze = nisze_dzielnic(world, market, &params);
    if nisze.is_empty() {
        return d;
    }
    let typ_sklepu = katalog
        .iter()
        .find(|(_, s)| s.category == SiteTypeCategory::Retail)
        .map(|(id, _)| id);
    let Some(typ_sklepu) = typ_sklepu else {
        return d;
    };

    let limit = limit_dzienny(world, params);
    if limit == 0 {
        return d;
    }
    let mut kandydaci = kandydaci(world, market, &nisze, &params);
    // Malejąco po wyniku, remis po indeksie mieszkańca — determinizm bez losowania.
    kandydaci.sort_unstable_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(a.citizen.0.index().cmp(&b.citizen.0.index()))
    });
    kandydaci.truncate(limit);

    for zamiar in kandydaci {
        // `zaloz` zwraca kwotę **faktycznie wniesioną**, a nie zamierzoną: między
        // oceną kandydata a przelewem gospodarstwo mogło już wydać część oszczędności,
        // a raport ma mówić o pieniądzu, który się ruszył.
        if let Some(wniesiony) = zaloz(world, market, &zamiar, typ_sklepu, t) {
            d.founded += 1;
            d.capital_own = Money(d.capital_own.get() + wniesiony.get());
        }
    }
    d
}

/// Ile firm może dziś powstać. Skaluje się populacją, bo miasto dziesięciokrotnie
/// większe ma dziesięciokrotnie więcej ludzi z pomysłem.
fn limit_dzienny(world: &World, p: FoundingParams) -> usize {
    let pop = world
        .get_resource::<Population>()
        .map_or(0, |x| x.citizens().len());
    ((pop as u64 * u64::from(p.per_100k_per_day)) / 100_000).max(1) as usize
}

/// Nisza dzielnicy: rodzaj sklepu, na który przypada w niej najwięcej ludzi.
///
/// To jest cała „wiedza o okolicy" z §5.7: ilu ludzi, ile sklepów którego rodzaju.
/// Dzielnica bez ani jednego sklepu nie ma niszy — nie dlatego, że jej nie ma,
/// tylko dlatego, że mieszkaniec nie ma stamtąd czego skopiować (`ponytail:` sufit
/// nazwany; wybór typu zakładu z katalogu zamiast z sąsiedztwa wymaga rynku
/// nieruchomości, czyli M10).
fn nisze_dzielnic(
    world: &World,
    market: &Market,
    p: &FoundingParams,
) -> BTreeMap<u16, (PlaceKind, ShopSeed)> {
    let mut ludzie: BTreeMap<u16, u32> = BTreeMap::new();
    if let Some(pop) = world.get_resource::<Population>() {
        for e in pop.citizens() {
            if world.get::<Identity>(*e).is_some_and(Identity::is_alive) {
                let d = world.get::<Residence>(*e).map_or(0, |r| r.district);
                *ludzie.entry(d).or_insert(0) += 1;
            }
        }
    }
    let nasiona = market.shop_seeds();
    // (dzielnica, rodzaj) → (ile sklepów, pierwsze nasiono)
    let mut sklepy: BTreeMap<(u16, usize), (u32, ShopSeed)> = BTreeMap::new();
    for s in &nasiona {
        let e = sklepy
            .entry((s.district, s.kind.as_index()))
            .or_insert((0, *s));
        e.0 += 1;
    }

    let mut out: BTreeMap<u16, (PlaceKind, ShopSeed)> = BTreeMap::new();
    for ((dzielnica, rodzaj), (ile, seed)) in sklepy {
        let ludzi = ludzie.get(&dzielnica).copied().unwrap_or(0);
        let na_sklep = ludzi / ile.max(1);
        let lepsza = out
            .get(&dzielnica)
            .map(|(k, _)| {
                let poprzedni = sklepy_rodzaju(&nasiona, dzielnica, *k);
                na_sklep > ludzi / poprzedni.max(1)
            })
            .unwrap_or(true);
        // Nisza istnieje dopiero wtedy, gdy na sklep przypada dość ludzi. Poniżej
        // progu dzielnica jest obsłużona i mieszkaniec **nie widzi tam interesu** —
        // a to, a nie limit dzienny, zatrzymuje dryf liczby firm.
        if lepsza && na_sklep >= p.min_people_per_shop {
            let Some(kind) = PlaceKind::from_index(rodzaj) else {
                continue;
            };
            out.insert(dzielnica, (kind, seed));
        }
    }
    out
}

fn sklepy_rodzaju(nasiona: &[ShopSeed], district: u16, kind: PlaceKind) -> u32 {
    nasiona
        .iter()
        .filter(|s| s.district == district && s.kind == kind)
        .count() as u32
}

/// Kandydaci na przedsiębiorców. Jeden przebieg po spisie; mieszkaniec, którego
/// gospodarstwo już finansuje czyjąś firmę, nie zakłada drugiej w tej samej dobie.
fn kandydaci(
    world: &World,
    market: &Market,
    nisze: &BTreeMap<u16, (PlaceKind, ShopSeed)>,
    p: &FoundingParams,
) -> Vec<FoundingIntent> {
    let Some(pop) = world.get_resource::<Population>() else {
        return Vec::new();
    };
    let wlasciciele = wlasciciele(world);
    let mut out = Vec::new();
    for e in pop.citizens() {
        let Some(id) = world.get::<Identity>(*e) else {
            continue;
        };
        if !id.is_alive() || wlasciciele.contains(&e.index()) {
            continue;
        }
        let Some(osob) = world.get::<Personality>(*e) else {
            continue;
        };
        let district = world.get::<Residence>(*e).map_or(0, |r| r.district);
        if !nisze.contains_key(&district) {
            continue;
        }
        let Some(gd) = magnat_agents::demography::household_by_index(world, id.household)
            .and_then(|h| world.get::<Household>(h))
        else {
            continue;
        };
        let kapital = Money((gd.savings.get() + gd.bank.get()) * i64::from(p.stake_bp) / 10_000);
        let Some(score) = founding_score(
            osob.get(TraitId::Ambition).get(),
            osob.get(TraitId::Risk).get(),
            kapital,
            p,
        ) else {
            continue;
        };
        let (kind, _) = nisze[&district];
        out.push(FoundingIntent {
            citizen: CitizenId(*e),
            district,
            kind,
            capital: kapital,
            score,
        });
    }
    let _ = market;
    out
}

/// Indeksy mieszkańców, którzy już są właścicielami firmy.
fn wlasciciele(world: &World) -> std::collections::BTreeSet<u32> {
    world
        .get_resource::<Firms>()
        .map_or_else(std::collections::BTreeSet::new, |f| {
            f.iter()
                .flat_map(|(_, firm)| firm.owners.iter())
                .filter_map(|s| match s.owner {
                    Owner::Citizen(c) => Some(c.0.index()),
                    _ => None,
                })
                .collect()
        })
}

/// Zakłada firmę: rejestr, rachunek, przelew kapitału, lokal z półką.
///
/// Zwraca `false`, gdy którykolwiek krok nie wyszedł — i **cofa to, co zdążył
/// zrobić**, tylko w jednym miejscu: pieniądz przenosi się dopiero wtedy, gdy sklep
/// już stoi. Odwrotna kolejność zostawiałaby przy nieudanym otwarciu gotówkę na
/// koncie firmy, której nie ma.
fn zaloz(
    world: &mut World,
    market: &Market,
    zamiar: &FoundingIntent,
    typ: magnat_firms::SiteTypeId,
    t: Tick,
) -> Option<Money> {
    let wzor = market
        .shop_seeds()
        .into_iter()
        .find(|s| s.district == zamiar.district && s.kind == zamiar.kind)?;
    let katalog = world.get_resource::<SiteTypeCatalog>().cloned()?;

    // 1. Rejestr firm: klucz nadaje licznik świata, nie wołający.
    let mut firms = world.get_resource_mut::<Firms>().map(std::mem::take)?;
    let nowy = nowy_site_id(&firms, market);
    let key = firms.insert(|key| {
        Firm::sole_owner(
            key,
            nazwa_firmy(key),
            SimMinute(t.get()),
            magnat_core::DistrictId(zamiar.district),
            Owner::Citizen(zamiar.citizen),
        )
    });
    let spec = katalog.get(typ);
    let site = Site::from_type(
        SitePlacement {
            id: nowy,
            // Nowa firma wchodzi do lokalu, który już stoi — patrz
            // `Market::shop_seeds`. Budynek bierze z zakładu wzorcowego.
            building: magnat_core::BuildingId(wzor.site.0),
            district: magnat_core::DistrictId(zamiar.district),
            floor_m2: DOMYSLNA_POWIERZCHNIA,
            opened: SimMinute(t.get()),
        },
        key,
        typ,
        spec,
        |_| DOMYSLNE_WIDELKI,
    );
    let dodany = firms.add_site(site);
    firms.log(
        key,
        t,
        DecisionReason::FirmFounded {
            score: zamiar.score,
            capital: zamiar.capital,
        },
    );
    *world.resource_mut::<Firms>() = firms;
    if !dodany {
        return None;
    }

    // 2. Rachunek i lokal.
    let firm_id = magnat_firms::firm_id(key);
    let konto = world.get_resource_mut::<Books>().map(|b| {
        b.open_account(
            AccountOwner::Firm(firm_id),
            AccountKind::Current,
            None,
            Money::ZERO,
        )
    })?;
    let otwarty = market.open_shop(
        ShopSeed {
            site: nowy,
            firm: firm_id,
            pos: wzor.pos,
            kind: zamiar.kind,
            shelf_slots: wzor.shelf_slots,
            capacity_m3: wzor.capacity_m3,
            district: zamiar.district,
        },
        konto,
        t,
    );
    if !otwarty {
        return None;
    }

    // 3. Kapitał: z komponentu gospodarstwa na rachunek firmy. Obie strony w jednej
    // operacji — `household_pay` podnosi pozycję `household_sector_in` w rejestrze
    // podaży, a zdjęcie z komponentu domyka ją po drugiej stronie granicy sektora
    // (`BB-4`). Pominięcie którejkolwiek połowy łamie globalny test pieniądza.
    let wniesiony = zdejmij_z_gospodarstwa(world, zamiar.citizen, zamiar.capital);
    if wniesiony.get() > 0 {
        if let Some(books) = world.get_resource_mut::<Books>() {
            let _ = books.household_pay(
                konto,
                wniesiony,
                TxMemo::new(TxKind::Endowment, DecisionReason::Unspecified),
                t,
            );
        }
        market.record_capital(nowy, wniesiony, t);
    }
    Some(wniesiony)
}

/// Pierwszy wolny klucz zakładu ponad wszystkim, co już w mieście stoi.
///
/// Klucz zakładu **nie jest encją ECS**, tylko numerem w przestrzeni przesuniętej
/// o `SITE_KEY_BASE` (`K-46`) — więc nowy bierze się z maksimum, a nie ze spawnu.
/// Maksimum liczy się z **obu** rejestrów, bo zakład produkcyjny stoi tylko w jednym,
/// a sklep w obu; kolizja numerów dałaby dwa różne miejsca o tym samym adresie.
pub(super) fn nowy_site_id(firms: &Firms, market: &Market) -> magnat_core::SiteId {
    nowy_site_id_z(firms, market, &[])
}

/// To samo, ale z uwzględnieniem kluczy nadanych **w tej samej operacji** i jeszcze
/// niewidocznych w żadnym rejestrze. Bez tego sieć otwierająca trzy sklepy naraz
/// nadałaby im trzy razy ten sam numer.
pub(super) fn nowy_site_id_z(
    firms: &Firms,
    market: &Market,
    swieze: &[magnat_core::SiteId],
) -> magnat_core::SiteId {
    let z_firm = firms.sites().map(|(id, _)| id.0.index()).max().unwrap_or(0);
    let z_rynku = market
        .sites()
        .iter()
        .map(|s| s.0.index())
        .max()
        .unwrap_or(0);
    let z_swiezych = swieze.iter().map(|s| s.0.index()).max().unwrap_or(0);
    magnat_core::SiteId(magnat_core::Entity::new(
        z_firm.max(z_rynku).max(z_swiezych).saturating_add(1),
        std::num::NonZeroU32::MIN,
    ))
}

/// Zdejmuje kapitał z gospodarstwa założyciela: najpierw z oszczędności, potem
/// z rachunku. Zwraca kwotę faktycznie zdjętą — gospodarstwo nie schodzi poniżej zera.
fn zdejmij_z_gospodarstwa(world: &mut World, c: CitizenId, kwota: Money) -> Money {
    let Some(h) = world
        .get::<Identity>(c.0)
        .and_then(|id| magnat_agents::demography::household_by_index(world, id.household))
    else {
        return Money::ZERO;
    };
    let Some(gd) = world.get_mut::<Household>(h) else {
        return Money::ZERO;
    };
    let mut zostalo = kwota.get();
    let z_oszczednosci = zostalo.min(gd.savings.get().max(0));
    gd.savings = Money(gd.savings.get() - z_oszczednosci);
    zostalo -= z_oszczednosci;
    let z_banku = zostalo.min(gd.bank.get().max(0));
    gd.bank = Money(gd.bank.get() - z_banku);
    Money(z_oszczednosci + z_banku)
}

/// Nazwa firmy z klucza. Nazwy własne generowane proceduralnie pochodzą
/// z `data/names/` i **nie są lokalizacją UI** (CLAUDE.md) — ale generator nazw
/// firm należy do M2 i nie ma go po tej stronie granicy crate'u, więc nowa firma
/// dostaje nazwę roboczą z numeru. `ponytail:` sufit nazwany; ścieżka wyjścia to
/// `magnat_world::naming` wstrzyknięte razem z mostem, tak samo jak `PlaceTable`.
fn nazwa_firmy(key: magnat_firms::FirmKey) -> String {
    format!("Firma {}", key.0)
}

/// Widełki płacowe nowej firmy — te same, których most M7a używa dla roli bez
/// własnego `Workplace`.
const DOMYSLNE_WIDELKI: (Money, Money) = (Money(280_000), Money(520_000));

/// Powierzchnia lokalu nowego sklepu w metrach. Wchodzi wyłącznie do kosztu stałego
/// zakładu; prawdziwą powierzchnię zna generator miasta, którego ten crate nie widzi.
const DOMYSLNA_POWIERZCHNIA: u32 = 120;
