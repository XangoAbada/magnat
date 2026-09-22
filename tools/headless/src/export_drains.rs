//! Scenariusz `export-drains` — **druga połowa kryterium WP9 fazy M6** (`R2-WP34`).
//!
//! `M6` §7.7 wymienia go jako test: „wzrost ceny zewnętrznej o 40 % → mierzalny odpływ
//! masy **i wzrost cen lokalnych**, bez zaprogramowanej reguły". Kryterium ma dwie
//! połowy i do R2f zmierzona była tylko pierwsza: `AH-12` zawęziło je do samego drenażu
//! masy — słusznie, bo ceny nie drgną, dopóki półka nie kupuje z rynku B2B — a `AI-8`
//! przeniosło pomiar cen za WP11, czyli do M6e. **M6e zamknęło się bez niego**, a nazwa
//! `export_drains` występowała odtąd wyłącznie w dwóch dokumentach planu.
//!
//! # Co ten przebieg robi
//!
//! Stawia to samo miasto co `m7-miasto`, wybiera towar o największym zapasie wśród
//! tych, którymi węzeł graniczny **handluje**, i w ustalonej dobie podnosi jego cenę
//! zewnętrzną o 40 %. Reszta świata bez zmian. Mierzy dwie krzywe doba po dobie:
//!
//! 1. **masa wysłana na eksport** (skumulowana) i zapas lokalny towaru,
//! 2. **mediana ceny półkowej** tego samego towaru.
//!
//! Pyta o **kierunek i opóźnienie**, nie o wartość. Żadnej reguły „eksport podnosi ceny"
//! się nie pisze i to jest sedno testu: jeśli cena nie drgnie, to znaczy, że kanał
//! między rynkiem B2B a półką nie istnieje — i **to jest wynik**, a nie porażka pomiaru.
//!
//! # Dlaczego eksport woła scenariusz, a nie system
//!
//! `B2b::try_export` nie ma w repozytorium ani jednego wołającego poza testami
//! (pozycja 74 wykazu `R2`), więc mechanizm drenażu nie uruchamia się w żadnym
//! wygenerowanym mieście. Ten przebieg jest jego pierwszym wołającym i **nie udaje,
//! że jest systemem**: decyzja „czy eksportować" należy do AI firm (M7e) i tam ma
//! powstać. Do tego czasu scenariusz odpytuje ją sam, bo inaczej nie byłoby czego
//! zmierzyć — a brak pomiaru jest gorszy od pomiaru z podanym wołającym.
//!
//! `ponytail:` `best_local` to **mediana ceny półkowej przeliczona na tonę**, a dla
//! towaru bez półki — **parytet importowy**. Ani jedno, ani drugie nie jest ceną
//! hurtową u lokalnego odbiorcy, bo takiej ceny nie ma czym zapytać. Sufit: półka jest
//! najwyższą ceną w mieście, a parytet importowy najniższą, po jakiej to samo dałoby
//! się sprowadzić — eksport wygrywający z którąkolwiek jest sygnałem mocnym, nie czułym.
//! Ścieżka wyjścia: gdy AI firm dostanie decyzję eksportową, `best_local` przyjdzie
//! z zapytania ofertowego, a nie stąd.
//!
//! # Co ten przebieg zmierzył
//!
//! Świat odniesienia (4 km, `industrial`, ziarno 1, 40 dób, szok w dobie 20): **0 kg**.
//! Powód nie jest cenowy i dlatego raport kończy się tabelą kandydatów: pięć towarów
//! w obrocie granicznym o największym zapasie ma **zero kilogramów w slocie wyjściowym
//! któregokolwiek zakładu**. Zapas w mieście jest — leży w slotach **wejściowych**
//! cudzych zakładów i na placu bramy. Wyrób gotowy nie leży u producenta ani minuty.
//! Wpisane jako `AS-1` w `M6-lancuch-dostaw.md` i pozycja 83 wykazu `R2`.

use std::process::ExitCode;

use clap::Args;
use magnat_agents::{
    bootstrap_day, register_day, society, DayLoopSystem, DeprivationEffectsSystem,
    HouseholdStockSystem, NeedDecaySystem, NoInheritance, ReplanCooldownSystem, SkillDriftSystem,
    SocietySystem,
};
use magnat_core::{GoodId, Mass, Money, SimMinute};
use magnat_economy::corpfin::system::InsolvencySystem;
use magnat_economy::labor::LaborSystem;
use magnat_economy::{Market, MarketSystem};
use magnat_ecs::{App, ScheduleBuilder};
use magnat_headless::full;
use magnat_headless::population::{swiat_agentow, zaludnij, zbuduj_miasto};
use magnat_jobs::JobPool;
use magnat_supply::b2b::TradeNodeId;
use magnat_supply::{Chain, ChainHandle};
use magnat_traffic::TrafficSystem;

/// Szok ceny zewnętrznej: +40 % (M6 §7.7).
const SZOK_BP: i64 = 14_000;
/// W ilu dobach od szoku cena półkowa ma drgnąć, żeby kryterium było spełnione.
const OKNO_ODPOWIEDZI: u32 = 14;

#[derive(Args, Debug)]
pub struct ExportDrainsArgs {
    #[arg(long, default_value_t = 1)]
    pub seed: u64,

    /// `4km` | `8km` | `12km` | `16km`.
    #[arg(long, default_value = "4km")]
    pub size: String,

    #[arg(long, default_value = "lowland")]
    pub region: String,

    #[arg(long, default_value = "1990")]
    pub epoch: String,

    /// Profil `industrial` daje najwięcej zakładów produkcyjnych, czyli najwięcej
    /// towaru, który w ogóle da się wywieźć.
    #[arg(long, default_value = "industrial")]
    pub profile: String,

    #[arg(long, default_value_t = 0)]
    pub threads: usize,

    #[arg(long, default_value_t = 3_000)]
    pub citizens: u32,

    /// Ile dób przebiec. Szok wypada w połowie, więc krótszy przebieg niż
    /// `2 × OKNO_ODPOWIEDZI` nie ma czego zmierzyć po szoku.
    #[arg(long, default_value_t = 60)]
    pub days: u16,

    /// Doba szoku; 0 = połowa przebiegu.
    #[arg(long, default_value_t = 0)]
    pub shock_day: u32,
}

/// Jedna doba pomiaru.
struct Doba {
    dzien: u32,
    /// Masa wysłana na eksport od początku przebiegu, w gramach.
    wyslane_g: i64,
    /// Zapas towaru w magazynach miasta, w gramach.
    zapas_g: i64,
    /// Mediana ceny półkowej towaru w groszach za sztukę; `None` = nikt go nie oferuje.
    polka_gr: Option<i64>,
    /// Lokalny punkt odniesienia producenta w groszach za tonę — półka albo parytet
    /// importowy (patrz [`cena_lokalna_za_tone`]).
    lokalna_gr_t: Option<i64>,
    /// Najlepsza cena eksportowa netto za tonę, po odjęciu frachtu; `None` = nikt
    /// za granicą tego nie kupuje albo nie ma skąd wieźć.
    eksport_gr_t: Option<i64>,
}

pub fn run(a: &ExportDrainsArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let pool = JobPool::new(a.threads);
    let city = zbuduj_miasto(a.seed, &a.size, &a.region, &a.epoch, &a.profile, &pool)?;
    let mut world = swiat_agentow(a.seed)?;
    register_day(&mut world);
    let zaludnione = zaludnij(&mut world, &city, a.citizens, 200_000)?;
    eprintln!(
        "{} mieszkańców, {} gospodarstw",
        society::population(&world),
        society::households(&world)
    );

    let f = full::setup(
        &mut world,
        &city,
        zaludnione.places.clone(),
        zaludnione.travel_oracle(),
        &zaludnione.traffic,
        a.seed,
        &pool,
    )?;
    let market = f.retail.market.clone();
    let chain = world.resource::<ChainHandle>().clone();

    let Some(towar) = towar_eksportowy(&chain, &market) else {
        eprintln!(
            "BRAK TOWARU, KTÓRY NARAZ STOI NA PÓŁCE I JEST W OBROCIE GRANICZNYM              — kryterium WP9 nie ma czego zmierzyć"
        );
        return Ok(ExitCode::FAILURE);
    };
    let klucz = chain.cat.good(towar).key.to_string();
    eprintln!("towar eksportowy: {klucz} ({towar:?})");

    bootstrap_day(&mut world, 0);
    let mut builder = ScheduleBuilder::new();
    builder
        .add(magnat_supply::ChainSystem::new())
        .add(magnat_firms::systems::FirmSystem::new())
        .add(MarketSystem::new(&world))
        .add(LaborSystem::new())
        .add(InsolvencySystem::new())
        .add(DayLoopSystem::new(&world))
        .add(ReplanCooldownSystem::new(&world))
        .add(NeedDecaySystem::new(&world))
        .add(DeprivationEffectsSystem::new(&world))
        .add(SkillDriftSystem::new(&world))
        .add(HouseholdStockSystem::new(&world))
        .add(SocietySystem::new(Box::new(NoInheritance)))
        .add(TrafficSystem::new(&world));
    let schedule = builder.build()?;
    let mut app = App::new(world, schedule, a.threads);

    let doba_szoku = if a.shock_day == 0 {
        u32::from(a.days) / 2
    } else {
        a.shock_day
    };
    let mut wyslane_g: i64 = 0;
    let mut serie: Vec<Doba> = Vec::new();

    for dzien in 1..=u32::from(a.days) {
        for _ in 0..1_440 {
            app.tick();
        }
        let teraz = SimMinute(u64::from(dzien) * 1_440);
        if dzien == doba_szoku {
            podnies_cene_zewnetrzna(&chain, towar);
            eprintln!("doba {dzien}: cena zewnętrzna {klucz:?} w górę o 40 %");
        }
        let start = std::time::Instant::now();
        let dzisiaj = wywiez_co_sie_da(&chain, &market, towar, teraz);
        wyslane_g += dzisiaj;
        // Postęp na `stderr`, bo przebieg jest długi, a bez niego nie widać, czy
        // scenariusz liczy dobę, czy utknął na flocie.
        let zapas_teraz = chain.lock().store.total_stock(towar).0;
        eprintln!(
            "doba {dzien}: wyslane {} kg, zapas {} kg, eksport zajal {:.1} s",
            dzisiaj / 1_000,
            zapas_teraz / 1_000,
            start.elapsed().as_secs_f64()
        );
        // **Dwa zamki, nigdy zagnieżdżone.** `Market::balance_sample` sięga po zamek
        // łańcucha, więc policzenie ceny półkowej **w środku** wyrażenia trzymającego
        // `chain.lock()` zawiesza przebieg na amen — zdarzyło się i kosztowało dwa
        // przebiegi, zanim stało się widoczne. Obie liczby powstają osobno, przed
        // złożeniem próbki.
        let polka_gr = polka(&market, towar);
        let lokalna = cena_lokalna_za_tone(&chain, &market, towar);
        let eksport = cena_eksportowa_za_tone(&chain, towar);
        serie.push(Doba {
            dzien,
            wyslane_g,
            zapas_g: zapas_teraz,
            polka_gr,
            lokalna_gr_t: lokalna.map(|m| m.0),
            eksport_gr_t: eksport.map(|m| m.0),
        });
    }

    raport(&serie, doba_szoku, &klucz);
    rozbior_kandydatow(&chain, &market);
    Ok(ExitCode::SUCCESS)
}

/// Dlaczego eksport nie ma czego wywieźć — pięć towarów o największym zapasie
/// z odpowiedzią, czy ktokolwiek sprzedaje je w mieście.
///
/// Ta tabela powstała z wyniku, a nie z planu: pierwszy przebieg pokazał zero
/// wywiezionej masy i zerowy zapas towaru wybranego do pomiaru, a pytanie „to gdzie
/// jest ta masa" nie miało gdzie się zadać. Bez niej raport mówi „nic się nie stało",
/// co jest prawdą i nie jest informacją.
fn rozbior_kandydatow(chain: &ChainHandle, market: &Market) {
    let polki: std::collections::BTreeMap<GoodId, i64> = market
        .balance_sample()
        .prices
        .into_iter()
        .filter(|d| d.offers > 0)
        .map(|d| (d.good, d.p50.0))
        .collect();
    let c = chain.lock();
    let Some(node) = c.b2b.node(TradeNodeId(0)) else {
        return;
    };
    let mut wiersze: Vec<(i64, GoodId)> = node
        .goods
        .iter()
        .map(|tg| (c.store.total_stock(tg.good).0, tg.good))
        .filter(|(zapas, _)| *zapas > 0)
        .collect();
    wiersze.sort_unstable_by_key(|(zapas, g)| (std::cmp::Reverse(*zapas), g.0));
    println!(
        "
── towary w obrocie granicznym, pięć o największym zapasie ──"
    );
    println!("towar | zapas (kg) | **na wyjściu zakładu** (kg) | cena półkowa (gr/szt)");
    for (zapas, g) in wiersze.into_iter().take(5) {
        // Kolumna, która rozstrzyga cały pomiar: producent eksportuje **swój wyrób**,
        // czyli zawartość slotu wyjściowego. Zapas leżący w slocie wejściowym cudzego
        // zakładu albo na placu bramy granicznej nie jest niczym, co da się wywieźć.
        let na_wyjsciu: i64 = c
            .plant
            .iter()
            .flat_map(|(_, z)| z.outputs.iter().map(|s| c.store.stock_of(*s, g).0))
            .sum();
        println!(
            "{} | {} | {} | {}",
            chain.cat.good(g).key,
            zapas / 1_000,
            na_wyjsciu / 1_000,
            polki
                .get(&g)
                .map_or("— nikt nie sprzedaje".to_string(), |p| p.to_string())
        );
    }
}

/// Towar, który **naraz** jest w obrocie granicznym i stoi na półce, o największym
/// zapasie lokalnym.
///
/// „Wysoki udział eksportu" nie jest **daną**: w `data/goods/` nie ma takiego pola,
/// a eksportowalność wynika z dwóch warunków naraz (`external_base_price` i brama
/// w `import_via`). Zapas jest jedynym wskaźnikiem, który mówi, czy w mieście jest
/// co wywozić — i to jest ta sama liczba, którą potem oglądamy jako drenaż.
///
/// **Towar na półce ma pierwszeństwo i to nie jest wygoda, tylko treść kryterium.**
/// WP9 pyta o dwie krzywe **tego samego towaru**: masę wychodzącą i cenę lokalną.
/// Towar, którego nikt nie sprzedaje detalicznie, nie ma ceny lokalnej, więc druga
/// połowa kryterium nie ma czego dotyczyć.
///
/// Gdy żaden towar z zapasem nie stoi na półce — a w mieście odniesienia **tak
/// właśnie jest** — bierzemy ten o największym zapasie i mierzymy samą masę.
/// Zero wywiezionej masy przy zerowym zapasie to pomiar niczego; zero wywiezionej
/// masy przy zapasie 489 t to pomiar czegoś.
fn towar_eksportowy(chain: &ChainHandle, market: &Market) -> Option<GoodId> {
    let na_polce: std::collections::BTreeSet<GoodId> = market
        .balance_sample()
        .prices
        .into_iter()
        .filter(|d| d.offers > 0)
        .map(|d| d.good)
        .collect();
    let c = chain.lock();
    let node = c.b2b.node(TradeNodeId(0))?;
    let z_zapasem: Vec<(GoodId, i64)> = node
        .goods
        .iter()
        .map(|tg| (tg.good, c.store.total_stock(tg.good).0))
        .filter(|(_, zapas)| *zapas > 0)
        .collect();
    let wybierz = |lista: &[(GoodId, i64)]| {
        lista
            .iter()
            .max_by_key(|(g, zapas)| (*zapas, std::cmp::Reverse(g.0)))
            .map(|(g, _)| *g)
    };
    // **Zapas ma pierwszeństwo przed półką** i to jest poprawka z pomiaru, nie
    // z założenia. Wybór „najpierw towar z półki" dał `food_milk` — 37 t w dobie zero
    // i **zero od doby pierwszej**, bo wyrób gotowy nie leży u producenta, tylko idzie
    // na półkę tego samego dnia. Scenariusz mierzył wtedy eksport towaru, którego nie
    // ma, czyli własny filtr. Zapas jest warunkiem koniecznym: bez niego nie ma czego
    // wywieźć i pierwsza połowa kryterium też przestaje cokolwiek znaczyć.
    let wybrany = wybierz(&z_zapasem)?;
    if !na_polce.contains(&wybrany) {
        eprintln!(
            "UWAGA: towar z największym zapasem nie stoi na półce — druga połowa              kryterium WP9 (cena lokalna) nie ma czego dotyczyć. Patrz tabela na końcu."
        );
    }
    Some(wybrany)
}

/// Cena zewnętrzna towaru w górę o 40 % — we wszystkich węzłach, bo szok jest
/// światowy, a nie portowy.
fn podnies_cene_zewnetrzna(chain: &ChainHandle, good: GoodId) {
    let mut c = chain.lock();
    for id in 0..u32::MAX {
        let Some(node) = c.b2b.node_mut(TradeNodeId(id)) else {
            break;
        };
        if let Some(tg) = node.good_mut(good) {
            tg.base_price = Money(tg.base_price.0.saturating_mul(SZOK_BP) / 10_000);
        }
    }
}

/// Mediana ceny półkowej towaru w groszach za sztukę.
fn polka(market: &Market, good: GoodId) -> Option<i64> {
    market
        .balance_sample()
        .prices
        .into_iter()
        .find(|d| d.good == good)
        .filter(|d| d.offers > 0)
        .map(|d| d.p50.0)
}

/// Próbuje wywieźć towar z każdego zakładu, który go trzyma. Zwraca masę wysłaną
/// w tej dobie, w gramach.
///
/// Reguły drenażu tu nie ma: `try_export` sam odrzuca ofertę, która nie bije ceny
/// lokalnej. Ta funkcja wyłącznie **pyta** — raz na dobę, każdy zakład osobno.
fn wywiez_co_sie_da(chain: &ChainHandle, market: &Market, good: GoodId, now: SimMinute) -> i64 {
    let Some(lokalnie) = cena_lokalna_za_tone(chain, market, good) else {
        return 0;
    };
    let cat = chain.cat.clone();
    let oracle = chain.oracle.clone();
    let mut c = chain.lock();
    // Kolejność zakładów jest kolejnością areny, a nie kolejnością odpytania —
    // dziennik transakcji wchodzi do hasha stanu (00 §3.2).
    let zrodla: Vec<(
        magnat_core::SiteId,
        magnat_core::FirmId,
        Vec<magnat_supply::SlotId>,
    )> = c
        .plant
        .iter()
        .map(|(id, z)| (id, z.owner, z.outputs.clone()))
        .collect();
    let mut wyslane = 0i64;
    for (site, owner, sloty) in zrodla {
        for slot in sloty {
            let masa = c.store.stock_of(slot, good);
            if masa.0 <= 0 {
                continue;
            }
            let Chain {
                store,
                transport,
                b2b,
                ..
            } = &mut *c;
            if let Some(s) = b2b.try_export(
                &cat,
                store,
                transport,
                good,
                site,
                slot,
                masa,
                owner,
                lokalnie,
                oracle.as_ref(),
                now,
            ) {
                wyslane += s.mass.0;
            }
        }
    }
    wyslane
}

/// Cena lokalna za tonę, z dwóch źródeł w tej kolejności.
///
/// 1. **Mediana półkowa** przeliczona przez masę jednostkową — cena, którą ktoś
///    w mieście naprawdę płaci.
/// 2. **Parytet importowy** (`TradeGood::import_price`) dla towaru, którego nikt nie
///    sprzedaje detalicznie: surowiec i półprodukt nie mają półki, ale mają cenę,
///    po której to samo dałoby się sprowadzić — i to jest klasyczny lokalny punkt
///    odniesienia producenta. Bez niego eksport nie byłby **pytany ani razu**
///    o towary, które jako jedyne mają zapas, czyli scenariusz mierzyłby własny filtr.
///
/// Porównania z zerem nie ma nigdzie: wypuściłoby wszystko za granicę i mierzyłoby
/// regułę, a nie rynek.
fn cena_lokalna_za_tone(chain: &ChainHandle, market: &Market, good: GoodId) -> Option<Money> {
    if let Some(p50) = polka(market, good) {
        let masa_jednostki = chain.cat.good(good).unit_mass;
        if masa_jednostki.0 > 0 {
            return Some(Money(
                (i128::from(p50) * 1_000_000 / i128::from(masa_jednostki.0)) as i64,
            ));
        }
    }
    let c = chain.lock();
    Some(c.b2b.node(TradeNodeId(0))?.good(good)?.import_price())
}

/// Najlepsza cena eksportowa netto za tonę — to samo pytanie, które zadaje
/// `try_export`, tylko bez kupowania.
///
/// Bez tej liczby raport mówi „wywieziono zero" i nie mówi **dlaczego**: czy nikt
/// nie chciał kupić, czy producent miał lepiej u siebie. Pytamy o tonę z pierwszego
/// zakładu, który cokolwiek trzyma, bo fracht zależy od miejsca nadania.
fn cena_eksportowa_za_tone(chain: &ChainHandle, good: GoodId) -> Option<Money> {
    let cat = chain.cat.clone();
    let oracle = chain.oracle.clone();
    let c = chain.lock();
    let skad = c
        .plant
        .iter()
        .find(|(_, z)| z.outputs.iter().any(|s| c.store.stock_of(*s, good).0 > 0))
        .map(|(id, _)| id)?;
    let (_, netto) = c
        .b2b
        .export_price(&cat, good, skad, Mass(1_000_000), oracle.as_ref())?;
    Some(netto)
}

fn raport(serie: &[Doba], doba_szoku: u32, klucz: &str) {
    println!("── export_drains: {klucz} ────────────");
    println!(
        "doba | wysłane (kg) | zapas (kg) | półka (gr/szt) | lokalnie (gr/t) | eksport netto (gr/t)"
    );
    let liczba = |v: Option<i64>| v.map_or("—".to_string(), |p| p.to_string());
    for d in serie {
        let znacznik = if d.dzien == doba_szoku {
            " ← szok"
        } else {
            ""
        };
        println!(
            "{:>4} | {:>12} | {:>10} | {:>14} | {:>15} | {:>20}{znacznik}",
            d.dzien,
            d.wyslane_g / 1_000,
            d.zapas_g / 1_000,
            liczba(d.polka_gr),
            liczba(d.lokalna_gr_t),
            liczba(d.eksport_gr_t)
        );
    }

    let przed = serie.iter().rfind(|d| d.dzien == doba_szoku);
    let po: Vec<&Doba> = serie
        .iter()
        .filter(|d| d.dzien > doba_szoku && d.dzien <= doba_szoku + OKNO_ODPOWIEDZI)
        .collect();
    let (Some(przed), false) = (przed, po.is_empty()) else {
        println!("przebieg za krótki, żeby zobaczyć skutek szoku");
        return;
    };

    let masa_po = po.last().map_or(0, |d| d.wyslane_g) - przed.wyslane_g;
    println!(
        "\nmasa wysłana w {OKNO_ODPOWIEDZI} dobach od szoku: {} kg",
        masa_po / 1_000
    );
    match (przed.polka_gr, po.iter().filter_map(|d| d.polka_gr).max()) {
        (Some(p0), Some(pmax)) => println!(
            "cena półkowa: {p0} → {pmax} gr/szt ({}{} ‰)",
            if pmax >= p0 { "+" } else { "" },
            (pmax - p0) * 1_000 / p0.max(1)
        ),
        _ => println!("cena półkowa: towar nie stoi na półce — kanał B2B → półka nie istnieje"),
    }
    if let (Some(l), Some(e)) = (przed.lokalna_gr_t, przed.eksport_gr_t) {
        println!(
            "w dobie szoku: lokalnie {l} gr/t, eksport netto {e} gr/t — eksport {} lokalną",
            if e > l { "bije" } else { "przegrywa z" }
        );
    }
}
