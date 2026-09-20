//! Przebieg balansatora: ten sam świat co scenariusz `m5shop`, tylko mierzony.
//!
//! **Headless jest wołany jako biblioteka, nie jako proces** (decyzja otwarta nr 10
//! dokumentu fazy M5, rozstrzygnięta zgodnie z propozycją domyślną). `--jobs N`
//! steruje liczbą **ziaren liczonych równolegle** — każde ziarno dostaje własny
//! `JobPool::new(1)`, więc równoległość idzie po ziarnach, a nie po systemach.
//! Izolacja awarii przez osobne procesy nie jest warta drugiego mechanizmu:
//! panika workera i tak wraca na wątek wołający (`M0 §5.4`), a przebieg, który
//! spanikował, jest błędem do naprawienia, nie stanem do przeżycia.
//!
//! Harmonogram jest **dokładnie ten sam i w tej samej kolejności** co w `m5shop`
//! (`MarketSystem` pierwszy). Inna kolejność znaczyłaby, że balansator stroi inny
//! świat niż gra — a wtedy zielona bramka nie mówi nic o grze.

use std::path::{Path, PathBuf};

use magnat_agents::{
    bootstrap_day, register_day, DayLoopSystem, DeprivationEffectsSystem, Household,
    HouseholdStockSystem, NeedDecaySystem, NoInheritance, Population, ReplanCooldownSystem,
    SkillDriftSystem, SocietySystem,
};
use magnat_core::{DecisionReason, Tick};
use magnat_economy::{Books, CompetitorRef, Market, MarketStats, MarketSystem, PricePolicy, TxId};
use magnat_ecs::{App, ScheduleBuilder};
use magnat_headless::full;
use magnat_headless::population::{swiat_agentow, zaludnij, zbuduj_miasto};
use magnat_io::world_state_hash;
use magnat_jobs::JobPool;
use magnat_traffic::TrafficSystem;

use crate::metrics::{DayMetrics, PriceRow, RunFile, SCHEMA_VERSION};

/// Region, epoka i profil miasta są **ustalone**, a nie wystawione w CLI: bramki
/// porównują liczby między przebiegami, więc parametr, którego nikt nie stroi,
/// byłby tylko kolejnym sposobem na przypadkowe porównanie dwóch różnych światów.
const REGION: &str = "lowland";
const EPOCH: &str = "1990";
const PROFILE: &str = "mixed";
/// Tyle prób zamiany mieszkań robi Etap 8 — ta sama liczba co w `m5shop`.
const SWAPS: u32 = 200_000;
/// Towar, który dostaje szok podaży. Klucz, nie `GoodId`: identyfikator zależy
/// od katalogu miasta, a nazwa nie.
const SHOCK_GOOD: &str = "food_bread_wheat";
/// Szok podaży: +80 % ceny hurtowej (§7.4, bramka G4).
const SHOCK_BP: i32 = 18_000;
/// Wojna cenowa gracza: −15 % względem najtańszego w promieniu 1 200 m.
const WAR_DELTA_BP: i32 = -1_500;
const WAR_RADIUS_M: u32 = 1_200;
/// Szok dochodowy: dochody gospodarstw × 0,8 (ryzyko R9 z §8).
const INCOME_SHOCK_BP: i64 = 8_000;

/// Scenariusz przebiegu. Różnią się **wyłącznie** jednym zdarzeniem w środku —
/// świat, harmonogram i dane są te same.
#[derive(Clone, Copy, PartialEq, Eq, Debug, clap::ValueEnum)]
pub enum Scenario {
    Base,
    SupplyShock,
    PlayerPriceWar,
    IncomeShock,
}

impl Scenario {
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Scenario::Base => "base",
            Scenario::SupplyShock => "supply-shock",
            Scenario::PlayerPriceWar => "player-price-war",
            Scenario::IncomeShock => "income-shock",
        }
    }
}

/// Doba zdarzenia dla szoku podaży i dochodowego: 180, a w przebiegu krótszym
/// niż rok gry — połowa. Ta sama funkcja liczy to w bramce G4, więc obie strony
/// mają jedno źródło prawdy.
#[must_use]
pub fn event_day(days: u16) -> u32 {
    if days < 360 {
        u32::from(days) / 2
    } else {
        180
    }
}

/// Doba wojny cenowej — ćwierć przebiegu, żeby zostało na reakcję konkurencji.
#[must_use]
pub fn war_day(days: u16) -> u32 {
    if days < 360 {
        u32::from(days) / 4
    } else {
        90
    }
}

/// Parametry wspólne dla wszystkich ziaren przebiegu.
#[derive(Clone, Debug)]
pub struct RunCfg {
    pub scenario: Scenario,
    pub days: u16,
    /// `4km` | `8km` | `12km` | `16km`.
    pub size: String,
    pub citizens: u32,
}

/// Jeden przebieg: jedno ziarno, `cfg.days` dób, próbka na granicy każdej doby.
///
/// Błąd wraca jako `String`, bo `Box<dyn Error>` nie jest `Send` i nie przeszedłby
/// przez `thread::scope`.
pub fn single(cfg: &RunCfg, seed: u64) -> Result<RunFile, String> {
    let pool = JobPool::new(1);
    let city = zbuduj_miasto(seed, &cfg.size, REGION, EPOCH, PROFILE, &pool).map_err(opis)?;
    let mut world = swiat_agentow(seed).map_err(opis)?;
    register_day(&mut world);
    let zaludnione = zaludnij(&mut world, &city, cfg.citizens, SWAPS).map_err(opis)?;
    // **Pełne miasto, nie wycinek** (M7f WP17). Do M7f balansator mierzył świat
    // bez rejestru firm, więc bramki G1–G9 opisywały gospodarkę, w której nikt nie
    // zatrudniał, nie płacił i nie bankrutował. Od tej chwili mierzą to samo miasto,
    // które widzi gracz — i to jest jedyny stan, w którym bramka cokolwiek obiecuje.
    let f = full::setup(
        &mut world,
        &city,
        zaludnione.places.clone(),
        zaludnione.travel_oracle(),
        &zaludnione.traffic,
        seed,
        &pool,
    )
    .map_err(opis)?;
    let r = f.retail;
    let market = r.market.clone();
    if r.shops == 0 {
        return Err(format!("ziarno {seed}: miasto bez ani jednego sklepu"));
    }
    // Szokowany towar rozwiązuje się **raz, na starcie**: `GoodId` zależy od
    // katalogu miasta, a bramka G4 musi wiedzieć, którą serię mierzy.
    let shock_good = (cfg.scenario == Scenario::SupplyShock)
        .then(|| market.good_of_key(SHOCK_GOOD))
        .flatten();
    bootstrap_day(&mut world, 0);

    let mut builder = ScheduleBuilder::new();
    builder
        .add(magnat_supply::ChainSystem::new())
        .add(magnat_firms::systems::FirmSystem::new())
        .add(MarketSystem::new(&world))
        .add(magnat_economy::labor::LaborSystem::new())
        .add(magnat_economy::corpfin::system::InsolvencySystem::new())
        .add(magnat_macro::MacroSystem::new())
        .add(magnat_media::MediaSystem::new())
        .add(DayLoopSystem::new(&world))
        .add(ReplanCooldownSystem::new(&world))
        .add(NeedDecaySystem::new(&world))
        .add(DeprivationEffectsSystem::new(&world))
        .add(SkillDriftSystem::new(&world))
        .add(HouseholdStockSystem::new(&world))
        .add(SocietySystem::new(Box::new(NoInheritance)))
        .add(TrafficSystem::new(&world));
    let schedule = builder.build().map_err(opis)?;
    let mut app = App::new(world, schedule, 1);

    let citizens =
        u32::try_from(magnat_agents::society::population(&app.world)).unwrap_or(u32::MAX);
    let mut probka = Probka::default();
    let mut days_data = vec![probka.dobowa(&app, &market, 0)];
    let mut hashes = vec![(0u64, world_state_hash(&app.world).to_string())];

    for d in 1..=u32::from(cfg.days) {
        for _ in 0..1_440 {
            app.tick();
        }
        days_data.push(probka.dobowa(&app, &market, d));
        hashes.push((
            u64::from(d) * 1_440,
            world_state_hash(&app.world).to_string(),
        ));
        zdarzenie(cfg, &mut app, &market, d, shock_good);
    }

    let conservation_ok = app.world.resource::<Books>().check_conservation().is_ok();

    Ok(RunFile {
        schema_version: SCHEMA_VERSION,
        scenario: cfg.scenario.key().to_string(),
        seed,
        days: cfg.days,
        citizens,
        shops: u32::try_from(r.shops).unwrap_or(u32::MAX),
        hashes,
        days_data,
        shock_good: shock_good.map(|g| g.0),
        conservation_ok,
        decisions_without_reason: probka.bez_powodu,
        decisions_sampled: probka.probkowanych,
    })
}

fn opis(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Zdarzenie scenariusza — jedyne miejsce, w którym scenariusze się różnią.
fn zdarzenie(
    cfg: &RunCfg,
    app: &mut App,
    market: &Market,
    day: u32,
    shock_good: Option<magnat_core::GoodId>,
) {
    match cfg.scenario {
        Scenario::Base => {}
        Scenario::SupplyShock if day == event_day(cfg.days) => {
            if let Some(g) = shock_good {
                market.set_supply_shock(g, SHOCK_BP);
            }
        }
        Scenario::PlayerPriceWar if day == war_day(cfg.days) => {
            wojna_cenowa(market, Tick(u64::from(day) * 1_440));
        }
        Scenario::IncomeShock if day == event_day(cfg.days) => {
            szok_dochodowy(app);
        }
        _ => {}
    }
}

/// Największy sklep w mieście delegowuje **każdy** swój towar polityce
/// „−15 % względem najtańszego w 1 200 m". To jest wejście do pytania, na które
/// bramka G6 odpowiada: czy rynek da się zmonopolizować ceną.
fn wojna_cenowa(market: &Market, t: Tick) {
    let mut najwiekszy: Option<(usize, magnat_core::SiteId, Vec<magnat_core::GoodId>)> = None;
    for site in market.sites() {
        let Some(panel) = market.shop_panel(site, Tick(0), t) else {
            continue;
        };
        let linie = panel.shelves.len();
        if najwiekszy.as_ref().is_none_or(|(n, _, _)| linie > *n) {
            najwiekszy = Some((
                linie,
                site,
                panel.shelves.iter().map(|s| s.good).collect::<Vec<_>>(),
            ));
        }
    }
    let Some((_, site, towary)) = najwiekszy else {
        return;
    };
    for good in towary {
        market.set_policy(
            site,
            good,
            PricePolicy::MatchCompetitor {
                delta_bp: WAR_DELTA_BP,
                radius_m: WAR_RADIUS_M,
                reference: CompetitorRef::Cheapest,
            },
            true,
        );
    }
}

/// Dochody wszystkich gospodarstw × 0,8.
///
/// Ryzyko R9 z §8 dokumentu fazy wprost: w M5 dochód jest **egzogeniczny**
/// (`RestOfWorld`), więc ten scenariusz jest jedynym miejscem, w którym balansator
/// w ogóle widzi, co się dzieje, gdy przestaje być stały. Wynik pozostaje
/// **warunkowy** do czasu M7, który dochód uczyni emergentnym.
fn szok_dochodowy(app: &mut App) {
    let Some(p) = app.world.get_resource::<Population>() else {
        return;
    };
    let gospodarstwa: Vec<magnat_core::Entity> = p.households().to_vec();
    for e in gospodarstwa {
        if let Some(h) = app.world.get_mut::<Household>(e) {
            h.income_monthly = h.income_monthly.mul_ratio(INCOME_SHOCK_BP, 10_000);
        }
    }
}

/// Stan potrzebny do policzenia przyrostów między dobami.
#[derive(Default)]
struct Probka {
    poprzednie: MarketStats,
    /// Ostatnia obejrzana transakcja — okno dziennika jest pierścieniem, więc bez
    /// tego ta sama transakcja liczyłaby się co dobę na nowo.
    ostatnia_tx: u64,
    bez_powodu: u64,
    probkowanych: u64,
}

impl Probka {
    fn dobowa(&mut self, app: &App, market: &Market, day: u32) -> DayMetrics {
        // Jeden odczyt rynku na dobę (`BalanceSample`), nie sto tysięcy pojedynczych:
        // bramki porównują liczby **między** dobami, więc spójność wewnątrz doby
        // jest warunkiem, żeby cokolwiek znaczyły.
        let b = market.balance_sample();
        let s = market.stats();
        let p = self.poprzednie;

        let decyzje = (s.purchases - p.purchases)
            + (s.deferrals - p.deferrals)
            + (s.stockouts - p.stockouts)
            + (s.budget_refusals - p.budget_refusals)
            + (s.no_candidates - p.no_candidates);
        let deferral_permille = ((s.deferrals - p.deferrals) * 1_000)
            .checked_div(decyzje)
            .map_or(0, |v| i32::try_from(v).unwrap_or(1_000));
        let stockout_refusal_permille = ((s.stockouts - p.stockouts) * 1_000)
            .checked_div(decyzje)
            .map_or(0, |v| i32::try_from(v).unwrap_or(1_000));

        let books = app.world.resource::<Books>();
        self.policz_powody(books);
        let zycie = app
            .world
            .get_resource::<magnat_economy::firmlife::FirmLifeLog>()
            .copied()
            .unwrap_or_default();
        let zamkniete = app
            .world
            .get_resource::<magnat_economy::corpfin::CorpFinance>()
            .map_or(0, |f| u32::try_from(f.case_count()).unwrap_or(u32::MAX));

        let m = DayMetrics {
            day,
            prices: b
                .prices
                .iter()
                .map(|d| PriceRow {
                    good: d.good.0,
                    min: d.min.get(),
                    p10: d.p10.get(),
                    p50: d.p50.get(),
                    p90: d.p90.get(),
                    max: d.max.get(),
                    offers: d.offers,
                })
                .collect(),
            cpi_index_bp: market.cpi_index_bp(),
            cpi_mom_bp: market.cpi_mom_bp(),
            cpi_yoy_bp: market.cpi_yoy_bp(),
            base_rate_bp: market.base_rate().bp,
            margin_median_bp: b.margin_median_bp,
            shops: b.shops,
            insolvent: b.insolvent,
            hhi_median: b.hhi_median,
            hhi_pairs: b.hhi_pairs,
            live_categories: b.live_categories,
            stockout_permille: b.stockout_permille,
            stockout_refusal_permille,
            deferral_permille,
            purchases: s.purchases - p.purchases,
            revenue_gr: s.revenue.get() - p.revenue.get(),
            reprices: s.reprices - p.reprices,
            write_offs: s.write_offs - p.write_offs,
            write_off_gr: s.write_off_value.get() - p.write_off_value.get(),
            expired_qty: s.expired_qty - p.expired_qty,
            loans: u32::try_from(market.loan_count()).unwrap_or(u32::MAX),
            credit_outstanding_gr: market.credit_outstanding().get(),
            money_supply_gr: books.supply().total().get(),
            firms: app
                .world
                .get_resource::<magnat_firms::Firms>()
                .map_or(0, |f| u32::try_from(f.len()).unwrap_or(u32::MAX)),
            firm_sites: app
                .world
                .get_resource::<magnat_firms::Firms>()
                .map_or(0, |f| u32::try_from(f.site_count()).unwrap_or(u32::MAX)),
            unemployment_permille: app
                .world
                .get_resource::<magnat_economy::labor::LaborHandle>()
                .and_then(magnat_economy::labor::LaborHandle::get)
                .map_or(0, |m| m.last_day().unemployment_permille()),
            labour_force: app
                .world
                .get_resource::<magnat_economy::labor::LaborHandle>()
                .and_then(magnat_economy::labor::LaborHandle::get)
                .map_or(0, |m| m.last_day().labour_force),
            vacancies: app
                .world
                .get_resource::<magnat_economy::labor::LaborHandle>()
                .and_then(magnat_economy::labor::LaborHandle::get)
                .map_or(0, |m| m.last_day().vacancies),
            firms_founded: zycie.total.founded,
            // Zniknięcia liczą się **razem**: dla pasma liczby firm nie ma różnicy,
            // czy właściciel zwinął interes sam, czy zrobił to syndyk. Rozróżnienie
            // jest w raporcie scenariusza, bo tam odpowiada na inne pytanie.
            firms_gone: zycie.total.wound_down + zamkniete,
        };
        self.poprzednie = s;
        m
    }

    /// Bramka G9: ile **decyzji** przeszło przez księgi bez powodu.
    ///
    /// To jest próbka, nie audyt: `TxJournal` jest pierścieniem 4 096 wpisów, więc
    /// doba, w której transakcji było więcej, zostawia część poza oknem. `TxId`
    /// rośnie monotonicznie, więc przynajmniej nic nie liczy się dwa razy.
    ///
    /// **Nie każda transakcja jest decyzją** i to rozróżnienie jest treścią bramki,
    /// a nie obejściem progu. PRD §14.1 mówi „każda **decyzja** agenta lub firmy
    /// zapisuje powód"; czynsz, media, płace, rata kredytu, wypłata dochodu
    /// i kapitał założycielski to **zobowiązania wykonane, nie wybrane** — nikt ich
    /// nie podejmuje w chwili, w której przechodzą przez księgi, a wpisanie im
    /// powodu znaczyłoby tyle, co wpisanie „bo tak stoi w umowie". Liczone są
    /// więc wyłącznie te rodzaje, przy których ktoś **wybierał**: sprzedaż
    /// detaliczna (wybór oferty), zamówienie u dostawcy (polityka zapasu),
    /// uruchomienie kredytu (ocena zdolności) i przepływy podatkowe M8.
    ///
    /// Bramka **nadal potrafi zaczerwienić**: to na tych czterech ścieżkach
    /// powstają powody, więc ścieżka, która o powodzie zapomni, wyjdzie tutaj.
    fn policz_powody(&mut self, books: &Books) {
        for tx in books.journal().iter() {
            let TxId(id) = tx.id;
            if id <= self.ostatnia_tx {
                continue;
            }
            self.ostatnia_tx = id;
            if !jest_decyzja(&tx.kind) {
                continue;
            }
            self.probkowanych += 1;
            if tx.reason == DecisionReason::Unspecified {
                self.bez_powodu += 1;
            }
        }
    }
}

/// Czy ten rodzaj transakcji niesie **wybór**, czy wykonanie zobowiązania.
///
/// Lista jest jawna i wyczerpująca z rozmysłu: nowy wariant `TxKind` **nie
/// skompiluje się** bez rozstrzygnięcia, po której stronie stoi — dokładnie tak,
/// jak `K-12` wymusza ramię w renderze powodów. Milcząca gałąź `_ => false`
/// zrobiłaby z bramki G9 coś, co przestaje mierzyć przy pierwszej nowej ścieżce
/// pieniądza, i nikt by tego nie zauważył.
fn jest_decyzja(kind: &magnat_economy::TxKind) -> bool {
    use magnat_economy::TxKind as K;
    match kind {
        // Wybór: kupujący wybrał ofertę, sklep zdecydował o zamówieniu,
        // bank ocenił zdolność, miasto rozdysponowało środki.
        K::RetailSale { .. }
        | K::WholesalePurchase { .. }
        | K::LoanDraw { .. }
        | K::TaxPayment { .. }
        | K::PublicSpend { .. }
        | K::ExternalCapital { .. }
        // Wpłata na kampanię jest wyborem w najczystszej postaci: firma nie musi
        // jej robić, nikt jej do tego nie zobowiązał, a decyduje o niej rachunek
        // „czy ten kandydat mi się opłaci" (M8e).
        | K::CampaignDonation { .. }
        // Reklama też: nikt nie zobowiązał firmy do kupienia billboardu, a decyduje
        // o tym rachunek „czy ta kampania mi się zwróci" (M10b).
        | K::AdSpend { .. }
        // Budżet laboratorium tak samo: badania są wydatkiem, którego nikt firmie
        // nie narzuca, a przestawia go kurs firmy (M10c, `K-49`).
        | K::RndSpend { .. } => true,
        // Zobowiązanie: umowa, harmonogram albo warunek początkowy świata.
        K::Wage { .. }
        // Licencja jest **zobowiązaniem**, nie wyborem, i to jest różnica wobec
        // budżetu badań: podpisana umowa każe płacić royalty co miesiąc niezależnie
        // od tego, czy firma nadal chce (M10c §5.4 pkt 3).
        | K::LicenseFee { .. }
        | K::Rent { .. }
        | K::Utility { .. }
        | K::LoanPayment { .. }
        | K::Deposit { .. }
        | K::Withdrawal
        | K::Endowment => false,
    }
}

/// Wszystkie ziarna scenariusza, `--jobs` naraz, plus bliźniak `seed0` dla G8.
///
/// Zwraca listę `(nazwa pliku, przebieg)` — zapis robi wołający, żeby ta funkcja
/// dała się przetestować bez dysku.
pub fn many(cfg: &RunCfg, seed0: u64, seeds: u32, jobs: usize) -> Vec<(String, RunFile)> {
    let mut zadania: Vec<(String, u64)> = (0..seeds)
        .map(|i| {
            let s = seed0 + u64::from(i);
            (format!("{}-{s}.ron", cfg.scenario.key()), s)
        })
        .collect();
    // Bramka G8 liczy się **w balansatorze**: to samo ziarno puszczone drugi raz
    // musi dać ten sam ciąg hashy. Plik bliźniaczy ma to samo `(scenario, seed)`
    // w treści, więc bramka rozpoznaje go po zawartości, a nie po nazwie.
    if seeds > 0 {
        zadania.push((format!("{}-{seed0}-b.ron", cfg.scenario.key()), seed0));
    }

    let jobs = jobs.max(1).min(zadania.len().max(1));
    let porcja = zadania.len().div_ceil(jobs);
    let mut wyniki: Vec<(String, RunFile)> = Vec::new();
    std::thread::scope(|s| {
        let uchwyty: Vec<_> = zadania
            .chunks(porcja)
            .map(|c| {
                s.spawn(move || {
                    c.iter()
                        .map(|(nazwa, seed)| {
                            let start = std::time::Instant::now();
                            let w = single(cfg, *seed);
                            eprintln!(
                                "ziarno {seed} ({}): {:.1} s{}",
                                cfg.scenario.key(),
                                start.elapsed().as_secs_f64(),
                                match &w {
                                    Ok(_) => String::new(),
                                    Err(e) => format!(" — BŁĄD: {e}"),
                                }
                            );
                            (nazwa.clone(), w)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for u in uchwyty {
            for (nazwa, w) in u.join().unwrap_or_default() {
                if let Ok(run) = w {
                    wyniki.push((nazwa, run));
                }
            }
        }
    });
    wyniki.sort_by(|a, b| a.0.cmp(&b.0));
    wyniki
}

/// Zapis przebiegów do katalogu w RON.
pub fn zapisz(out: &Path, wyniki: &[(String, RunFile)]) -> std::io::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(out)?;
    let mut sciezki = Vec::new();
    for (nazwa, run) in wyniki {
        let p = out.join(nazwa);
        let tekst = ron::ser::to_string_pretty(run, ron::ser::PrettyConfig::default())
            .map_err(std::io::Error::other)?;
        std::fs::write(&p, tekst)?;
        sciezki.push(p);
    }
    Ok(sciezki)
}

/// Wczytanie katalogu przebiegów — wejście podpolecenia `gate`.
pub fn wczytaj(dir: &Path) -> std::io::Result<Vec<RunFile>> {
    let mut pliki: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "ron"))
        .collect();
    pliki.sort();
    let mut out = Vec::new();
    for p in pliki {
        let tekst = std::fs::read_to_string(&p)?;
        let run: RunFile = ron::from_str(&tekst).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{}: {e}", p.display()),
            )
        })?;
        out.push(run);
    }
    Ok(out)
}

/// Dolny ogranicznik marży z `data/economy/shop.ron` — próg bramki G3.
/// Dolna krawędź widełek, bo żadna firma nie losuje podłogi niżej.
pub fn min_margin_bp() -> Result<i32, String> {
    magnat_economy::EconomyData::load_default()
        .map(|d| d.pricing.min_margin_bp.min)
        .map_err(|e| format!("{e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::Money;

    #[test]
    fn doba_zdarzenia_skaluje_sie_z_dlugoscia_przebiegu() {
        // Rok gry ma 360 dób (`K-1`), więc szok w 180. dobie to połowa roku.
        assert_eq!(event_day(730), 180);
        assert_eq!(event_day(360), 180);
        assert_eq!(event_day(40), 20);
        assert_eq!(war_day(730), 90);
        assert_eq!(war_day(40), 10);
    }

    #[test]
    fn szok_dochodowy_to_dokladnie_minus_dwadziescia_procent() {
        assert_eq!(Money(1_000).mul_ratio(INCOME_SHOCK_BP, 10_000), Money(800));
    }
}
