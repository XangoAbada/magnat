//! Scenariusz `m5shop` — **wynik podfazy M5b**: „mieszkaniec wychodzi po chleb,
//! wybiera ofertę i wraca; pieniądz i sztuki zgadzają się po obu stronach".
//!
//! Różnica wobec `m3day` jest w jednej linii mostu: `AgentSources.places` dostaje
//! `Market` zamiast `InfinitePlaces` (`Z-1`). Wszystko inne — planer, pętla doby,
//! ruch, potrzeby — zostaje bez zmian, i to jest cały dowód, że punkt podmiany
//! był właściwy.
//!
//! Runner mierzy i wypisuje trzy rzeczy, które ta podfaza ma pokazać:
//! 1. **zakupy** — ile transakcji, za ile, ile odmów i z jakiego powodu,
//! 2. **pieniądz** — suma sald w księgach **plus** pieniądz gospodarstw musi być
//!    niezmienna co do grosza; to jest niezmiennik P1 rozszerzony o sektor
//!    gospodarstw domowych, którego saldo mieszka w komponencie M3,
//! 3. **determinizm** — hash stanu świata co `hash-every` ticków.

use std::process::ExitCode;
use std::sync::Arc;

use clap::Args;
use magnat_agents::{
    bootstrap_day, register_day, society, AgentSources, DayLoopSystem, DayStats,
    DeprivationEffectsSystem, Household, HouseholdStockSystem, NeedDecaySystem, NeedTable,
    NoInheritance, Population, ReplanCooldownSystem, SkillDriftSystem, SocietySystem,
};
use magnat_core::{Money, PlaceKind, Qty, RejectCause, SiteId, StockCat, Tick};
use magnat_ecs::{App, ScheduleBuilder};
use magnat_economy::{
    AccountKind, AccountOwner, Books, EconomyData, GoodTable, Market, MarketSystem, ShopSeed,
    TxKind, TxMemo,
};
use magnat_io::world_state_hash;
use magnat_jobs::JobPool;
use magnat_spatial::{Aabb2, GridSpec, Vec2};
use magnat_traffic::TrafficSystem;
use magnat_world::{population::SITE_KEY_BASE, CityData};

use crate::population::{swiat_agentow, zaludnij, zbuduj_miasto};

/// Kapitał obrotowy sklepu na starcie. `ponytail:` stała zamiast modelu kapitału —
/// sufit nazwany: sklep, który ma za mało, po prostu nie zamawia. Kapitał zakładany
/// przez właściciela wnosi M7 razem z zakładaniem firm.
const KAPITAL_SKLEPU: i64 = 40_000_000;

/// Ile metrów kwadratowych lokalu przypada na jedną linię asortymentu.
const M2_NA_LINIE: u32 = 18;

#[derive(Args, Debug)]
pub struct M5ShopArgs {
    #[arg(long, default_value_t = 1)]
    pub seed: u64,

    /// `4km` | `8km` | `12km` | `16km`.
    #[arg(long, default_value = "4km")]
    pub size: String,

    #[arg(long, default_value = "lowland")]
    pub region: String,

    #[arg(long, default_value = "1990")]
    pub epoch: String,

    #[arg(long, default_value = "mixed")]
    pub profile: String,

    #[arg(long, default_value_t = 0)]
    pub threads: usize,

    /// Ile dób gry przebiec.
    #[arg(long, default_value_t = 2)]
    pub days: u16,

    /// Docelowa liczba mieszkańców; 0 = z pojemności miasta.
    #[arg(long, default_value_t = 0)]
    pub citizens: u32,

    /// Co ile ticków liczyć hash stanu; 0 = nie liczyć.
    #[arg(long, default_value_t = 1440)]
    pub hash_every: u64,
}

/// Klucz epoki, po którym `data/chains/needs.ron` indeksuje koszyk.
fn koszyk_epoki(epoch: &str) -> &'static str {
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
fn katalog_detaliczny(city: &CityData, data: &EconomyData) -> GoodTable {
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
fn siatka(city: &CityData) -> GridSpec {
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
fn obsadz_sklepy(city: &CityData, market: &Market, books: &mut Books, rest: magnat_economy::AccountId) -> usize {
    let mut ile = 0;
    for (i, s) in city.sites.sites.iter().enumerate() {
        let Some(kind) = city.site_catalog.get(s.archetype).spec.place_kind else {
            continue;
        };
        if !matches!(kind, PlaceKind::Grocery | PlaceKind::Pharmacy | PlaceKind::Clothing) {
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
        // Kapitał obrotowy jest **przelewem** z reszty świata, nie emisją: pieniądz
        // sklepu musi mieć skąd pochodzić, inaczej niezmiennik P1 przestaje cokolwiek
        // znaczyć już w pierwszym ticku.
        if books
            .transfer(
                rest,
                konto,
                Money(KAPITAL_SKLEPU),
                TxMemo::new(TxKind::Endowment, magnat_core::DecisionReason::Unspecified),
                Tick(0),
            )
            .is_err()
        {
            continue;
        }
        let seed = ShopSeed {
            site: SiteId(magnat_core::Entity::new(
                SITE_KEY_BASE + i as u32,
                std::num::NonZeroU32::MIN,
            )),
            firm,
            pos,
            kind,
            shelf_slots: (powierzchnia / M2_NA_LINIE).clamp(2, 40) as u16,
            capacity_m3: i64::from(powierzchnia) * 3,
        };
        if market.open_shop(seed, konto, Tick(0)) {
            ile += 1;
        }
    }
    ile
}

/// Pieniądz gospodarstw — druga strona niezmiennika, której `Books` nie widzi.
fn pieniadz_gospodarstw(world: &magnat_ecs::World) -> i64 {
    let Some(p) = world.get_resource::<Population>() else {
        return 0;
    };
    p.households()
        .iter()
        .filter_map(|e| world.get::<Household>(*e))
        .map(|h| {
            h.cash
                .get()
                .saturating_add(h.bank.get())
                .saturating_add(h.savings.get())
        })
        .sum()
}

pub fn run(a: &M5ShopArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let pool = JobPool::new(a.threads);
    let start = std::time::Instant::now();
    let city = zbuduj_miasto(a.seed, &a.size, &a.region, &a.epoch, &a.profile, &pool)?;
    eprintln!("miasto {:.1} s", start.elapsed().as_secs_f64());

    let mut world = swiat_agentow(a.seed)?;
    register_day(&mut world);
    let start = std::time::Instant::now();
    let zaludnione = zaludnij(&mut world, &city, a.citizens, 200_000)?;
    eprintln!(
        "Etap 8 {:.1} s, {} mieszkańców, {} gospodarstw",
        start.elapsed().as_secs_f64(),
        society::population(&world),
        society::households(&world)
    );

    // ── gospodarka ───────────────────────────────────────────────────────────────
    let data = EconomyData::load_default()?;
    let _ = koszyk_epoki(&a.epoch);
    let goods = katalog_detaliczny(&city, &data);
    eprintln!("katalog detaliczny: {} towarów", goods.len());
    let tabela = Arc::new(NeedTable::load_default()?);

    let mut books = Books::new();
    let rest = books.open_account(
        AccountOwner::RestOfWorld,
        AccountKind::Current,
        None,
        Money::ZERO,
    );
    books.endow(rest, Money(10_000_000_000), Tick(0))?;

    let market = Market::new(
        siatka(&city),
        a.seed,
        data,
        goods,
        tabela,
        zaludnione.places.clone(),
        rest,
    );
    let sklepow = obsadz_sklepy(&city, &market, &mut books, rest);
    market.stock_initial(&mut books, Tick(0));
    market.rebuild_index(&pool);
    eprintln!(
        "rynek: {sklepow} sklepów, {} ofert, zapas startowy za {} zł",
        market.offer_count(),
        market
            .sites()
            .iter()
            .map(|s| market.inventory_value(*s).get())
            .sum::<i64>()
            / 100
    );
    if sklepow == 0 {
        eprintln!("BRAK SKLEPÓW — scenariusz nie ma czego pokazać");
        return Ok(ExitCode::FAILURE);
    }

    world.insert_resource(books);
    world.insert_resource(market.clone());
    world.register_resource_hash::<Books>();
    world.register_resource_hash::<Market>();

    // **To jest cała podmiana z `Z-1`**: rynek zamiast atrapy miejsc.
    *world.resource_mut::<AgentSources>() = AgentSources::new(
        Box::new(market.clone()),
        zaludnione.travel_oracle(),
    );

    // Pierwsza wypłata przed startem doby: gospodarstwa z generacji mają saldo zero,
    // a pensja wpada dopiero na granicy miesiąca. Bez tego dzień 1 jest dniem,
    // w którym nikogo nie stać na chleb.
    let (ilu, pensje) = magnat_economy::pay_incomes(&mut world, &market, Tick(0));
    eprintln!("wypłata startowa: {ilu} gospodarstw, {} zł", pensje.get() / 100);

    let zaplanowanych = bootstrap_day(&mut world, 0);
    eprintln!("kolejka zasiana: {zaplanowanych} mieszkańców");

    let mut builder = ScheduleBuilder::new();
    builder
        .add(MarketSystem::new(&world))
        .add(DayLoopSystem::new(&world))
        .add(ReplanCooldownSystem::new(&world))
        .add(NeedDecaySystem::new(&world))
        .add(DeprivationEffectsSystem::new(&world))
        .add(SkillDriftSystem::new(&world))
        .add(HouseholdStockSystem::new(&world))
        .add(SocietySystem::new(Box::new(NoInheritance)))
        .add(TrafficSystem::new(&world));
    let schedule = builder.build()?;
    eprintln!(
        "harmonogram: {} systemów w {} etapach, odcisk {:#018x}",
        schedule.system_count(),
        schedule.stage_count(),
        schedule.fingerprint()
    );

    let mut app = App::new(world, schedule, a.threads);
    let pieniadz_start = app
        .world
        .resource::<Books>()
        .total_balance()
        .get()
        .saturating_add(pieniadz_gospodarstw(&app.world));

    let ticki = u64::from(a.days) * 1440;
    let bieg = std::time::Instant::now();
    let mut hashe: Vec<(u64, String)> = Vec::new();
    for t in 1..=ticki {
        app.tick();
        if a.hash_every > 0 && t.is_multiple_of(a.hash_every) {
            hashe.push((t, world_state_hash(&app.world).to_string()));
        }
    }
    let czas = bieg.elapsed().as_secs_f64();

    // ── raport ───────────────────────────────────────────────────────────────────
    let s = market.stats();
    let dzien = app.world.resource::<DayStats>();
    println!("── doba ──────────────────────────────────────────────");
    println!(
        "{} dób w {:.2} s, {} zdarzeń, {} wizyt zrealizowanych, {} odmów",
        a.days, czas, dzien.events, dzien.fulfilled, dzien.refused
    );
    println!("── zakupy ────────────────────────────────────────────");
    println!(
        "transakcje {}, obrót {} zł, sprzedano {} jednostek",
        s.purchases,
        s.revenue.get() / 100,
        s.purchased_qty / 1_000
    );
    println!(
        "odmowy: brak towaru {}, brak środków {}, poniżej progu {}, brak ofert w zasięgu {}",
        s.stockouts, s.budget_refusals, s.deferrals, s.no_candidates
    );
    println!(
        "zaopatrzenie: {} dostaw, {} uzupełnień półki",
        s.deliveries, s.restocks
    );

    let books = app.world.resource::<Books>();
    let pieniadz_koniec = books
        .total_balance()
        .get()
        .saturating_add(pieniadz_gospodarstw(&app.world));
    println!("── pieniądz ──────────────────────────────────────────");
    println!(
        "P1 (księgi): {}",
        match books.check_conservation() {
            Ok(()) => "OK".to_string(),
            Err((suma, podaz)) => format!("ROZJAZD suma={suma:?} podaż={podaz:?}"),
        }
    );
    println!(
        "kanał sektora gospodarstw: +{} zł / −{} zł",
        books.supply().household_sector_in.get() / 100,
        books.supply().household_sector_out.get() / 100
    );
    println!(
        "suma świata (księgi + gospodarstwa): start {} zł, koniec {} zł, różnica {} gr",
        pieniadz_start / 100,
        pieniadz_koniec / 100,
        pieniadz_koniec - pieniadz_start
    );

    // ── pierwszy sklep z brzegu: co widać w panelu (M5e zrobi z tego ekran) ──────
    if let Some(site) = market.sites().first().copied() {
        market.set_tracking(site, magnat_economy::LostSaleTracking::Full);
        println!("── sklep {} ─────────────────────────────────────────", site.entity().index());
        println!(
            "wartość zapasu {} zł, konto {} zł",
            market.inventory_value(site).get() / 100,
            market
                .account_of(site)
                .and_then(|a| books.balance(a))
                .unwrap_or(Money::ZERO)
                .get()
                / 100
        );
        if let Some(h) = market.lost_histogram(site) {
            for c in RejectCause::ALL {
                let n = h.by_cause[c.as_index()];
                if n > 0 {
                    println!("  utracone ({}): {n}", c.name());
                }
            }
        }
    }

    for (t, h) in &hashe {
        println!("hash {t:>7}: {h}");
    }

    let zgadza_sie = pieniadz_koniec == pieniadz_start && books.check_conservation().is_ok();
    let sprzedano = s.purchases > 0;
    if !zgadza_sie {
        eprintln!("BŁĄD: pieniądz nie zgadza się po obu stronach");
    }
    if !sprzedano {
        eprintln!("BŁĄD: przez {} dób nikt nic nie kupił", a.days);
    }
    let _ = StockCat::Food;
    Ok(if zgadza_sie && sprzedano {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
