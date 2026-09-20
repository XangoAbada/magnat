//! Most: zakłady Etapu 7 z `sim/world` → gospodarka detaliczna z `sim/economy`.
//!
//! **Jedna kopia tej wiedzy, trzech konsumentów** (`K-18`, DRY): scenariusz
//! `m5shop`, klient graficzny `tools/magnat` (WP12, `AB-1`) i balansator (WP13)
//! stawiają **ten sam** rynek. Dopóki mieszkało to w ciele scenariusza, dwa
//! pozostałe musiałyby to przepisać — a przepisany most rozjeżdża się przy
//! pierwszej zmianie i rozjazd widać dopiero jako inny wynik w bramce.
//!
//! Dlaczego tutaj, a nie w `sim/economy`: `sim/economy` nie zna `CityData` i znać
//! go nie może (`sim/world` zależy od `sim/agents`, a most potrzebuje obu stron).
//! Właścicielem obu stron jest narzędzie — stąd biblioteka `magnat-headless`,
//! którą decyzja otwarta nr 10 dokumentu fazy i tak każe wystawić.

use std::sync::Arc;

use magnat_agents::{AgentSources, NeedTable, PlaceTable, TravelOracle};
use magnat_core::{DecisionReason, Entity, FirmId, Mass, Money, PlaceKind, Qty, SiteId, Tick};
use magnat_economy::{
    AccountId, AccountKind, AccountOwner, Books, EconomyData, GoodTable, HouseholdMonthReport,
    Market, ShopSeed, TxKind, TxMemo,
};
use magnat_ecs::World;
use magnat_jobs::JobPool;
use magnat_spatial::{Aabb2, GridSpec, Vec2};
use magnat_supply::{ChainHandle, FreightOracle, TariffTable, Tuning};
use magnat_traffic::TrafficOracle;
use magnat_world::{population::SITE_KEY_BASE, CityData};

/// Emisja startowa na koncie reszty świata — **część stała**. Jedyne miejsce,
/// w którym pieniądz powstaje z niczego; wszystko dalej jest przelewem, żeby P1 miał
/// co pilnować.
pub const EMISJA: i64 = 10_000_000_000;

/// Emisja dla świata o tylu zakładach Etapu 7 (`AP-3`).
///
/// Od M6e kapitał obrotowy dostaje nie tylko sklep, ale i zakład produkcyjny, więc
/// stała emisja przestała wystarczać: metropolia ma rząd tysiąca zakładów, a stała
/// pokrywała kilkadziesiąt. Objaw był myląco odległy od przyczyny — `brak środków
/// w granicach limitu debetu` przy zakładaniu **losowego** zakładu, czyli tego,
/// na którym konto reszty świata akurat się skończyło.
///
/// Emisja pokrywa każdy zakład stawką zakładu produkcyjnego, także sklep: różnica
/// idzie na zapas startowy i wypłaty, a zaniżenie kosztowałoby drugi taki błąd.
#[must_use]
pub fn emisja(sites: usize) -> Money {
    Money(EMISJA.saturating_add(sites as i64 * crate::world::plants::KAPITAL_ZAKLADU))
}

/// Kapitał obrotowy sklepu. `ponytail:` stała zamiast modelu kapitału — sufit
/// nazwany: sklep, który ma za mało, po prostu nie zamawia. Kapitał zakładany
/// przez właściciela wnosi M7 razem z zakładaniem firm.
pub const KAPITAL_SKLEPU: i64 = 40_000_000;

/// Kapitał banku miasta. Bank w M5 nie zbiera depozytów jako źródła akcji
/// kredytowej — kreacja idzie przez `Books::create_credit` — ale musi mieć konto,
/// bo przez nie przechodzi każdy grosz kapitału i odsetek.
pub const KAPITAL_BANKU: i64 = 5_000_000_000;

/// Ile metrów kwadratowych lokalu przypada na jedną linię asortymentu.
const M2_NA_LINIE: u32 = 18;

/// Identyfikator zakładu węzła granicznego. Poza przestrzenią zakładów miasta,
/// tak samo jak bank: brama nie jest zakładem Etapu 7.
const SITE_BRAMY: u32 = u32::MAX - 2;

/// Przepustowość bramy towarowej w tonach na dobę. Ta sama liczba, którą generator
/// miasta bierze do domknięcia podaży (`brama_t_na_dobe` dla `Highway`) — i to jest
/// **celowo** ta sama liczba, bo pyta o to samo: ile tona po tonie przechodzi granicę.
/// Różnica jest w tym, że generator używa jej raz, a węzeł w każdej dobie gry (`AK-1`).
const BRAMA_T_NA_DOBE: i64 = 4_000;

/// Czas dostawy importowej z bramy drogowej — dwie doby, zanim zaległość go wydłuży.
const BRAMA_LEAD_MINUT: u32 = 2_880;

/// Stała opłata za podstawienie pojazdu, w groszach (`AM-5`).
///
/// Wyodrębniona z kilometrów, bo bez niej konsolidacja dostaw nie oszczędza nic:
/// koszt liniowy w masie znaczy, że dziesięć kursów po tonie kosztuje tyle samo co
/// jeden po dziesięć, a wtedy `dc_beats_direct` spełnia się tożsamościowo.
const PODSTAWIENIE_GR: i64 = 4_000;

/// Buduje łańcuch dostaw miasta: katalog, strojenie, trasę i bramę towarową.
///
/// **Brama jest tu jedynym źródłem towaru i to jest świadome.** Zakłady produkcyjne
/// Etapu 7 stoją w danych miasta, ale nie mają jeszcze linii, obsady ani zapasu
/// startowego — ich uruchomienie to `R2` z §8 dokumentu fazy i należy do M6e razem
/// z `data/scenarios/initial_stock.ron`. Do tego czasu sklep kupuje towar **przez
/// granicę**: za pieniądze, z czasem dostawy, z ceną rosnącą przy dużych zakupach
/// i z przepustowością, która się kończy. To jest cała różnica wobec `ExternalSupplier`,
/// który dawał wszystko natychmiast i po stałej cenie.
fn zbuduj_lancuch(
    city: &CityData,
    traffic: &Arc<TrafficOracle>,
    seed: u64,
) -> Result<ChainHandle, Box<dyn std::error::Error>> {
    let cat = Arc::new(city.catalog.clone());
    let tuning = Arc::new(Tuning::load_default()?);
    // **Prawdziwe kilometry po grafie M4** (`AO-4`). Do M6d stała tu atrapa
    // `FlatRateFreight` z jedną odległością dla całego miasta — wystarczała do
    // przetestowania maszyny stanów zlecenia i do niczego więcej: przy stałej
    // odległości centrum dystrybucyjne wygrywa albo przegrywa z arytmetyki,
    // a nie z geografii. Brama graniczna dostaje pozycję razem z zakładami, bo
    // inaczej trasa do niej nie istnieje i import przestaje być wykonalny.
    let mut rampy = crate::world::plants::rampy(city);
    let brama = SiteId(Entity::new(SITE_BRAMY, std::num::NonZeroU32::MIN));
    rampy.insert(brama, brama_pos(city));
    let oracle: Arc<dyn FreightOracle> = Arc::new(magnat_traffic::freight::RoadFreight::new(
        traffic.clone(),
        traffic.catalog().clone(),
        rampy,
        PODSTAWIENIE_GR,
    ));
    Ok(ChainHandle::with_import_gate(
        cat,
        tuning,
        oracle,
        TariffTable::load_default()?,
        seed,
        brama,
        Mass(BRAMA_T_NA_DOBE * 1_000_000),
        BRAMA_LEAD_MINUT,
    )
    .with_deposits(city.deposits.clone()))
}

/// Pozycja bramy towarowej: pierwsza brama drogowa miasta, a gdy takiej nie ma —
/// środek miasta. Brama nie jest zakładem Etapu 7, więc nie ma budynku, z którego
/// dałoby się wziąć AABB.
fn brama_pos(city: &CityData) -> magnat_core::WorldCoord {
    city.roads
        .gates
        .iter()
        .find(|g| !g.kind.is_rail())
        .map_or_else(
            || magnat_core::WorldCoord::new(city.center.x as i32, city.center.y as i32, 0),
            |g| magnat_core::WorldCoord::new(g.pos.x as i32, g.pos.y as i32, 0),
        )
}

/// Wynik postawienia gospodarki.
pub struct Retail {
    pub market: Market,
    pub shops: usize,
    /// Zakłady produkcyjne Etapu 7 postawione jako `PlantSite` (`AO-3`).
    pub plants: crate::world::plants::PlantsReport,
    /// Ile gospodarstw dostało pierwszą wypłatę i na jaką sumę.
    pub incomes: (u64, Money),
    /// Pierwsze rozliczenie miesiąca gospodarstw — koperty, koszty stałe, kredyty.
    pub budgets: HouseholdMonthReport,
}

/// Klucz epoki, po którym `data/chains/needs.ron` indeksuje koszyk.
#[must_use]
pub fn koszyk_epoki(epoch: &str) -> &'static str {
    match epoch {
        "1950" | "1970" => "postwar",
        "2010" | "2020" => "contemporary",
        _ => "transition",
    }
}

/// Złączenie `data/goods/` (cena hurtowa), `data/economy/retail.ron` (kategoria,
/// trwałość, narzut) i koszyka epoki (zużycie na osobę na dobę).
///
/// Robi je **wołający**, bo `GoodId` nadaje katalog M2 i tylko on zna kolejność
/// kluczy tekstowych; `sim/economy` nie zna schematu `data/goods/` i nie musi.
#[must_use]
pub fn katalog_detaliczny(city: &CityData, data: &EconomyData) -> GoodTable {
    GoodTable::build(&data.retail, |key| {
        let id = city.catalog.good_id(key)?;
        let base = city.catalog.good(id).external_base_price?;
        let dobowe = city
            .catalog
            .basket
            .iter()
            .find(|(g, _)| *g == id)
            .map(|(_, g)| *g)?;
        Some((id, base, Qty(dobowe)))
    })
}

/// Siatka indeksu ofert pokrywająca miasto.
#[must_use]
pub fn siatka(city: &CityData) -> GridSpec {
    let mut min = Vec2::new(f32::MAX, f32::MAX);
    let mut max = Vec2::new(f32::MIN, f32::MIN);
    for b in &city.buildings.buildings {
        min = Vec2::new(min.x.min(b.aabb.min.x), min.y.min(b.aabb.min.y));
        max = Vec2::new(max.x.max(b.aabb.max.x), max.y.max(b.aabb.max.y));
    }
    if min.x > max.x {
        min = Vec2::ZERO;
        max = Vec2::new(1_000.0, 1_000.0);
    }
    GridSpec::covering(Aabb2::new(min, max), 200)
}

/// Stawia sklepy tam, gdzie Etap 7 postawił zakłady handlowe.
pub fn obsadz_sklepy(
    city: &CityData,
    market: &Market,
    books: &mut Books,
    rest: AccountId,
) -> usize {
    let mut ile = 0;
    for (i, s) in city.sites.sites.iter().enumerate() {
        let Some(kind) = city.site_catalog.get(s.archetype).spec.place_kind else {
            continue;
        };
        if !matches!(
            kind,
            PlaceKind::Grocery | PlaceKind::Pharmacy | PlaceKind::Clothing
        ) {
            continue;
        }
        let b = &city.buildings.buildings[s.building.0.index() as usize];
        let pos = Vec2::new(
            (b.aabb.min.x + b.aabb.max.x) / 2.0,
            (b.aabb.min.y + b.aabb.max.y) / 2.0,
        );
        let powierzchnia: u32 = city.buildings.units[s.units.start as usize..s.units.end as usize]
            .iter()
            .map(|u| u32::from(u.area_m2))
            .sum();
        let firm = s.firm;
        let konto = books.open_account(
            AccountOwner::Firm(firm),
            AccountKind::Current,
            None,
            Money::ZERO,
        );
        // Kapitał obrotowy jest **przelewem**, nie emisją: pieniądz sklepu musi mieć
        // skąd pochodzić, inaczej niezmiennik P1 przestaje cokolwiek znaczyć już
        // w pierwszym ticku.
        if books
            .transfer(
                rest,
                konto,
                Money(KAPITAL_SKLEPU),
                TxMemo::new(TxKind::Endowment, DecisionReason::Unspecified),
                Tick(0),
            )
            .is_err()
        {
            continue;
        }
        let seed = ShopSeed {
            site: SiteId(Entity::new(
                SITE_KEY_BASE + i as u32,
                std::num::NonZeroU32::MIN,
            )),
            firm,
            pos,
            kind,
            shelf_slots: (powierzchnia / M2_NA_LINIE).clamp(2, 40) as u16,
            capacity_m3: i64::from(powierzchnia) * 3,
            // Dzielnica z parceli budynku — rynek sam jej nie wyprowadzi z pozycji,
            // a bramka G6 mierzy koncentrację **per dzielnica**, nie per miasto.
            district: city
                .parcels
                .parcels
                .get(b.parcel.0.index() as usize)
                .map_or(0, |p| p.district.0),
        };
        if market.open_shop(seed, konto, Tick(0)) {
            // Kapitał obrotowy musi trafić także do księgi zakładu (M5c §5.8),
            // inaczej `BankCurrent` od pierwszej minuty nie zgadza się z rachunkiem.
            market.record_capital(seed.site, Money(KAPITAL_SKLEPU), Tick(0));
            ile += 1;
        }
    }
    ile
}

/// Stawia całą gospodarkę detaliczną na zaludnionym świecie i **podmienia
/// `Sources.places`** na rynek (`Z-1`).
///
/// Kolejność kroków jest kontraktem, nie wygodą:
/// 1. konto reszty świata i emisja — zanim powstanie pierwszy sklep,
/// 2. sklepy z zakładów Etapu 7 z kapitałem obrotowym,
/// 3. bank miasta — bez niego każdy wniosek kończy się `RejectCredit::NoLender`,
/// 4. zapas startowy i indeks ofert,
/// 5. zasoby świata i haki hasha,
/// 6. podmiana `AgentSources`,
/// 7. **wypłata przed budżetem** — plan dzieli to, co wpłynęło (`Y-6`).
///
/// Po powrocie wołający dokłada `MarketSystem` do harmonogramu i zasiewa dobę.
#[allow(clippy::too_many_arguments)]
pub fn setup(
    world: &mut World,
    city: &CityData,
    places: Arc<PlaceTable>,
    travel: Box<dyn TravelOracle>,
    traffic: &Arc<TrafficOracle>,
    seed: u64,
    pool: &JobPool,
    // Towary **poza obiegiem** na starcie partii: te, które dopiero wypuści
    // technologia (M10c WP10.9). Pusty wycinek = świat bez drzewa technologii,
    // zachowujący się dokładnie jak przed M10c. Parametr, a nie odczyt z zasobu:
    // blokada musi stanąć **między** powstaniem rynku a obsadzeniem sklepów, bo
    // sklep obsadzony wcześniej wystawiłby towar, którego w mieście jeszcze nie ma.
    locked: &[magnat_core::GoodId],
) -> Result<Retail, Box<dyn std::error::Error>> {
    let data = EconomyData::load_default()?;
    let goods = katalog_detaliczny(city, &data);
    let needs = Arc::new(NeedTable::load_default()?);

    let mut books = Books::new();
    let rest = books.open_account(
        AccountOwner::RestOfWorld,
        AccountKind::Current,
        None,
        Money::ZERO,
    );
    books.endow(rest, emisja(city.sites.sites.len()), Tick(0))?;

    let chain = zbuduj_lancuch(city, traffic, seed)?;
    let market = Market::new(
        siatka(city),
        seed,
        data,
        goods,
        chain.clone(),
        needs,
        places,
        rest,
    );
    for g in locked {
        market.lock_good(*g);
    }
    let shops = obsadz_sklepy(city, &market, &mut books, rest);
    // Zakłady produkcyjne — **po** sklepach, bo obie ścieżki chodzą po tej samej liście
    // `city.sites.sites` i muszą się na niej nie przeciąć, a `to_sklep` jest jedynym
    // rozstrzygnięciem, które je rozdziela.
    let plants = if std::env::var_os("MAGNAT_NO_PLANTS").is_some() {
        Default::default()
    } else {
        crate::world::plants::obsadz_zaklady(
            city,
            &chain,
            &market,
            &mut books,
            rest,
            &crate::world::plants::InitialStock::load_default()?,
        )
    };

    // Bank miasta (M5d §5.10, decyzja otwarta nr 7): `FirmId`, konto i kapitał,
    // a jego „AI" to `assess_credit`. Identyfikator poza przestrzenią firm miasta,
    // bo bank nie jest zakładem Etapu 7.
    let bank_firm = FirmId(Entity::new(u32::MAX - 1, std::num::NonZeroU32::MIN));
    let konto_banku = books.open_account(
        AccountOwner::Bank(bank_firm),
        AccountKind::Current,
        None,
        Money::ZERO,
    );
    books.transfer(
        rest,
        konto_banku,
        Money(KAPITAL_BANKU),
        TxMemo::new(TxKind::Endowment, DecisionReason::Unspecified),
        Tick(0),
    )?;
    market.open_bank(bank_firm, konto_banku);

    market.stock_initial(&mut books, Tick(0));
    market.rebuild_index(pool);

    world.insert_resource(books);
    world.insert_resource(market.clone());
    world.insert_resource(chain);
    world.register_resource_hash::<Books>();
    world.register_resource_hash::<Market>();
    // Łańcuch wchodzi do hasha jako **jeden** zasób (`AL-3`): magazyn, zakłady,
    // transport i rynek B2B w tej kolejności, arena partii w kolejności indeksów
    // (`K-16`). Cztery osobne zasoby rozbiłyby krok, który i tak dotyka ich wszystkich.
    world.register_resource_hash::<ChainHandle>();

    // Tablica publiczna cen i katalog presetów polityk (M7e WP10, `AZ-1`).
    // Tablica wchodzi do hasha, katalog nie — jest danymi z `data/policies/`,
    // tak samo jak tabela ról.
    magnat_economy::register_firm_ai(
        world,
        magnat_economy::PublicMarketBoard::new(),
        magnat_policy::PolicyCatalog::load_default()?,
    );

    // Kalibracja jakości wykonania polityk (M9d WP9). Wchodzi **tutaj**, a nie
    // do hasha: jest daną z `data/tuning/`, tak samo jak katalog presetów. Błąd
    // pliku zatrzymuje stawianie świata — świat bez tej tabeli wykonywałby reguły
    // bezbłędnie i natychmiast, a wtedy zatrudnienie menedżera nie miałoby skutku.
    world.insert_resource(magnat_economy::PolicyTuning::load_default()?);

    // **To jest cała podmiana z `Z-1`**: rynek zamiast atrapy miejsc z M3.
    *world.resource_mut::<AgentSources>() = AgentSources::new(Box::new(market.clone()), travel);

    // Gospodarstwa z generacji mają saldo zero, a pensja wpada dopiero na granicy
    // miesiąca — bez tej wypłaty dzień pierwszy jest dniem, w którym nikogo nie
    // stać na chleb. Budżet w tej samej chwili, bo plan dzieli to, co wpłynęło.
    let incomes = magnat_economy::pay_incomes(world, &market, Tick(0));
    let budgets = magnat_economy::settle_household_month(world, &market, Tick(0), &mut Vec::new());

    Ok(Retail {
        market,
        shops,
        plants,
        incomes,
        budgets,
    })
}
