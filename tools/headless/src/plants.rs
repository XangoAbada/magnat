//! Zakłady Etapu 7 → fizyczne zakłady łańcucha dostaw (`AO-3`, ryzyko `R2`).
//!
//! **Czego brakowało do M6d.** Generator miasta stawiał `SiteSeed` z recepturami
//! i domykał na nich bilans podaży (KROK 5 `closure.rs`), ale nikt nie budował z tego
//! `PlantSite`: nie było linii, licznika mediów, rampy ani zapasu startowego. Skutek
//! był taki, że łańcuch referencyjny z §7.1 („pole → elewator → młyn → piekarnia →
//! sklep") nie miał w mieście **ani jednego ogniwa produkcyjnego**, a sklepy kupowały
//! przez bramę graniczną. Bilans masy się domykał — import jest prawdziwym źródłem —
//! ale miasto nie produkowało niczego.
//!
//! **Jedna liczba, której nie wolno tu zgadnąć.** Przepustowość linii wyprowadza się
//! z receptury i skali zakładu: `batch_mass × 60 / duration_minutes × capacity_scale
//! / SCALE_BASE`. To nie jest wygoda — to jest **ta sama arytmetyka**, którą KROK 5
//! domknięcia łańcuchów wyrównywał podaż do popytu (`daily_yield × capacity_scale
//! / SCALE_BASE`). Dowolna inna liczba znaczyłaby, że miasto zbilansowane na papierze
//! produkuje w symulacji co innego, a różnicę widać dopiero jako niedobór po dwóch
//! tygodniach gry.
//!
//! **Czego tu nie ma i dlaczego.** Obsada etatowa, płace i decyzje inwestycyjne należą
//! do M7 (§2 dokumentu fazy, tabela „nie wchodzi"). Zakład dostaje jedną zmianę dzienną
//! z `ProductionSchedule::default` i konto z kapitałem obrotowym — tyle, ile potrzeba,
//! żeby kupował wejścia i sprzedawał wyjścia. „AI firmy" to w M6 brak AI, a nie
//! zaślepka udająca decyzje.

use std::collections::BTreeMap;

use magnat_core::{
    DecisionReason, Energy, Entity, FirmId, GoodId, Mass, Money, OpenHours, PlaceKind, Q, RecipeId,
    SimMinute, SiteId, UtilityService, Volume,
};
use magnat_economy::{AccountId, AccountKind, AccountOwner, Books, Market, TxKind, TxMemo};
use magnat_supply::{
    catalog::Catalog, plant::Dock, store::BatchDraft, store::MassIn, store::WarehouseRole,
    ChainHandle, InventoryRule, MiningSite, PlantSite, ProductionLine, RecipeSource, SlotId,
    StorageClass, Store, UtilityMeter,
};
use magnat_world::{population::SITE_KEY_BASE, CityData};
use serde::Deserialize;

/// Kapitał obrotowy zakładu produkcyjnego.
///
/// `ponytail:` stała, tak samo jak [`crate::retail::KAPITAL_SKLEPU`] — sufit nazwany:
/// zakład, któremu zabraknie, po prostu przestaje płacić i rośnie mu zobowiązanie.
/// Kapitał założycielski wnosi M7 razem z zakładaniem firm.
pub const KAPITAL_ZAKLADU: i64 = 120_000_000;

/// Skala bazowa `SiteSeed::capacity_scale` — 1000 znaczy „bez zmiany".
const SCALE_BASE: i64 = 1_000;

/// Dostawca mediów. Jedna firma na całe miasto, poza przestrzenią zakładów Etapu 7:
/// sieci przesyłowe są zakresem M8, a licznik i faktura muszą działać już teraz.
const FIRMA_MEDIOW: u32 = u32::MAX - 3;

/// Pojemność magazynu wejściowego jako wielokrotność dobowego zużycia.
///
/// Większa niż `input_days` z danych, bo do magazynu ma się jeszcze **zmieścić**
/// dostawa uzupełniająca. Magazyn równy zapasowi startowemu odrzucałby każdą dostawę
/// przyjeżdżającą, zanim bufor zejdzie do zera, i wyglądałoby to na problem z rampą.
const ZAPAS_POJEMNOSC: i64 = 3;

/// Zapas startowy zakładów — `data/scenarios/initial_stock.ron`.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct InitialStock {
    pub schema_version: u32,
    pub input_days: i64,
    pub output_days: i64,
    pub quality: u8,
    pub reorder_days: i64,
    pub target_days: i64,
}

impl InitialStock {
    /// # Errors
    /// Gdy pliku nie ma, nie da się go sparsować albo ma inną wersję schematu.
    pub fn load_default() -> Result<InitialStock, Box<dyn std::error::Error>> {
        let p = magnat_core::data_path("scenarios/initial_stock.ron");
        let s = std::fs::read_to_string(&p)?;
        let v: InitialStock = ron::from_str(&s)?;
        if v.schema_version != 1 {
            return Err(format!("{}: schema_version {} ≠ 1", p.display(), v.schema_version).into());
        }
        Ok(v)
    }
}

/// Co postawiono — raport do scenariusza i do testów.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlantsReport {
    pub sites: usize,
    pub lines: usize,
    /// Zakłady wydobywcze, którym Etap 7 przypisał złoże (`AL-17`).
    pub mines: usize,
    /// Zakłady z recepturą `Extraction`, które złoża **nie** dostały — stoją
    /// na `Starved` i to jest właściwa odpowiedź, nie błąd danych.
    pub mines_without_deposit: usize,
    pub stock_value: Money,
}

/// Czy ten archetyp jest sklepem — stawia go [`crate::retail::obsadz_sklepy`]
/// i drugi raz go tu nie stawiamy.
fn to_sklep(kind: Option<PlaceKind>) -> bool {
    matches!(
        kind,
        Some(PlaceKind::Grocery | PlaceKind::Pharmacy | PlaceKind::Clothing)
    )
}

/// `SiteId` zakładu w przestrzeni gospodarki. Ta sama konwencja co w sklepach
/// (`SITE_KEY_BASE + indeks w `city.sites.sites`), bo indeks jest ten sam, a dwie
/// przestrzenie identyfikatorów dla jednego zakładu rozjechałyby ślad partii.
#[must_use]
pub fn site_id(i: usize) -> SiteId {
    SiteId(Entity::new(
        SITE_KEY_BASE + i as u32,
        std::num::NonZeroU32::MIN,
    ))
}

/// Nominalna przepustowość linii w gramach na godzinę.
fn przepustowosc(r: &magnat_supply::Recipe, scale: u16) -> Mass {
    let na_godzine = r.batch_mass.0.saturating_mul(60) / i64::from(r.duration_minutes.max(1));
    Mass((na_godzine.saturating_mul(i64::from(scale)) / SCALE_BASE).max(1))
}

/// Część zamienna dla linii: pierwszy towar domeny `part_` w katalogu.
///
/// `ponytail:` jedna część na wszystkie maszyny. Sufit nazwany: właściwe przypisanie
/// części do klasy maszyny jest tabelą w `data/`, której właścicielem jest M7 razem
/// z konserwacją jako decyzją firmy. Dziś liczy się to, że awaria **zjada towar
/// z magazynu** i bez niego linia stoi — a to działa z dowolną częścią.
fn czesc_zamienna(cat: &Catalog) -> Option<GoodId> {
    cat.goods
        .iter()
        .find(|g| g.key.starts_with("part_"))
        .map(|g| g.id)
}

/// Sumaryczna maska klas niebezpieczeństwa towarów, które przez zakład przechodzą.
/// Magazyn, który nie przyjmuje własnego wsadu, odrzucałby każdą dostawę z błędem
/// `HazardNotAllowed` — objaw wyglądający na brak dostawcy.
fn maska_hazardu(cat: &Catalog, recipes: &[RecipeId]) -> u8 {
    let mut m = 0u8;
    for r in recipes {
        let r = cat.recipe(*r);
        for i in &r.inputs {
            m |= cat.good(i.good).hazard.bit();
        }
        for o in &r.outputs {
            m |= cat.good(o.good).hazard.bit();
        }
    }
    m
}

/// Klasa przechowywania slotu — najczęstsza wśród towarów, które w nim staną.
fn klasa(cat: &Catalog, goods: &[GoodId]) -> StorageClass {
    goods
        .first()
        .map_or(StorageClass::Dry, |g| cat.good(*g).storage)
}

/// Wejścia i wyjścia wszystkich receptur zakładu, bez powtórzeń, w kolejności katalogu.
fn wejscia_wyjscia(cat: &Catalog, recipes: &[RecipeId]) -> (Vec<GoodId>, Vec<GoodId>) {
    let mut we: Vec<GoodId> = Vec::new();
    let mut wy: Vec<GoodId> = Vec::new();
    for r in recipes {
        let r = cat.recipe(*r);
        for i in &r.inputs {
            if !we.contains(&i.good) {
                we.push(i.good);
            }
        }
        for o in &r.outputs {
            if !wy.contains(&o.good) {
                wy.push(o.good);
            }
        }
    }
    we.sort_unstable_by_key(|g| g.0);
    wy.sort_unstable_by_key(|g| g.0);
    (we, wy)
}

/// Dobowe zapotrzebowanie zakładu na wejście `g`, w gramach.
fn dobowe_zuzycie(cat: &Catalog, recipes: &[RecipeId], g: GoodId, scale: u16) -> i64 {
    recipes
        .iter()
        .map(|r| cat.recipe(*r).daily_input(g))
        .sum::<i64>()
        .saturating_mul(i64::from(scale))
        / SCALE_BASE
}

/// Dobowa produkcja zakładu towaru `g`, w gramach.
fn dobowa_produkcja(cat: &Catalog, recipes: &[RecipeId], g: GoodId, scale: u16) -> i64 {
    recipes
        .iter()
        .map(|r| cat.recipe(*r).daily_yield(g))
        .sum::<i64>()
        .saturating_mul(i64::from(scale))
        / SCALE_BASE
}

/// Stawia zakłady produkcyjne miasta i zwraca raport.
///
/// Kolejność wewnątrz zakładu jest wymuszona typami, ale warto ją nazwać: sloty muszą
/// istnieć, zanim powstanie `PlantSite` (trzyma ich uchwyty), a `PlantSite` zanim
/// wejdzie zapas startowy (wchodzi do slotu wejściowego tego zakładu).
///
/// # Panics
/// Gdy zamek łańcucha jest zatruty.
pub fn obsadz_zaklady(
    city: &CityData,
    chain: &ChainHandle,
    market: &Market,
    books: &mut Books,
    rest: AccountId,
    cfg: &InitialStock,
) -> PlantsReport {
    let cat = chain.cat.clone();
    let mut rep = PlantsReport::default();
    let czesc = czesc_zamienna(&cat);

    for (i, s) in city.sites.sites.iter().enumerate() {
        if s.recipes.is_empty() || to_sklep(city.site_catalog.get(s.archetype).spec.place_kind) {
            continue;
        }
        postaw_zaklad(chain, market, books, rest, cfg, &cat, czesc, i, s, &mut rep);
    }
    rep
}

/// Jeden zakład: magazyny, linie, liczniki, konto i zapas startowy.
///
/// Osobno od pętli, bo pętla odpowiada na pytanie „które zakłady", a to na „jak wygląda
/// zakład" — dwa różne powody do zmiany (zasada `S` z SOLID, CLAUDE.md).
#[allow(clippy::too_many_arguments)]
fn postaw_zaklad(
    chain: &ChainHandle,
    market: &Market,
    books: &mut Books,
    rest: AccountId,
    cfg: &InitialStock,
    cat: &Catalog,
    czesc: Option<GoodId>,
    i: usize,
    seed: &magnat_world::SiteSeed,
    rep: &mut PlantsReport,
) {
    let site = site_id(i);
    let firm = seed.firm;
    let (we, wy) = wejscia_wyjscia(cat, &seed.recipes);
    let hazard = maska_hazardu(cat, &seed.recipes);

    // Pojemność z zapotrzebowania, nie z powierzchni parceli: parcela mówi, ile hali
    // się zmieści, a zapotrzebowanie — ile towaru ta hala musi pomieścić, żeby linia
    // nie stała. Rozbieżność między nimi jest tematem M7 (rozbudowa).
    let cap_we: i64 = we
        .iter()
        .map(|g| dobowe_zuzycie(cat, &seed.recipes, *g, seed.capacity_scale))
        .sum::<i64>()
        .saturating_mul(cfg.input_days * ZAPAS_POJEMNOSC)
        .max(1_000_000);
    let cap_wy: i64 = wy
        .iter()
        .map(|g| dobowa_produkcja(cat, &seed.recipes, *g, seed.capacity_scale))
        .sum::<i64>()
        .saturating_mul(cfg.output_days * ZAPAS_POJEMNOSC)
        .max(1_000_000);

    let (wejscie, wyjscie) = {
        let mut ch = chain.lock();
        let a = ch.store.add_slot(
            site,
            WarehouseRole::Input,
            klasa(cat, &we),
            Mass(cap_we),
            Volume(cap_we.saturating_mul(16)),
            hazard,
        );
        let b = ch.store.add_slot(
            site,
            WarehouseRole::Output,
            klasa(cat, &wy),
            Mass(cap_wy),
            Volume(cap_wy.saturating_mul(16)),
            hazard,
        );
        (a, b)
    };

    let mut zaklad = PlantSite::new(site, firm, Dock::new(2, 15, 1, OpenHours::ALWAYS));
    zaklad.inputs.push(wejscie);
    zaklad.outputs.push(wyjscie);
    liczniki(&mut zaklad, cat, &seed.recipes, chain.tuning.utility);
    rep.lines += linie(&mut zaklad, cat, seed, czesc);
    if zaklad.lines.is_empty() {
        return;
    }
    wydobycie(&mut zaklad, cat, seed, rep);

    let konto = books.open_account(
        AccountOwner::Firm(firm),
        AccountKind::Current,
        None,
        Money::ZERO,
    );
    if books
        .transfer(
            rest,
            konto,
            Money(KAPITAL_ZAKLADU),
            TxMemo::new(TxKind::Endowment, DecisionReason::Unspecified),
            magnat_core::Tick(0),
        )
        .is_err()
    {
        return;
    }
    market.register_plant(site, firm, konto);

    let mut ch = chain.lock();
    ch.plant.insert(zaklad);
    let reguly = zapas_startowy(&mut ch.store, cat, cfg, seed, (wejscie, wyjscie), firm, rep);
    ch.set_rules(site, reguly);
    rep.sites += 1;
}

/// Zapas otwarcia zakładu i reguły zapasu jego wejść.
///
/// Jedna funkcja na oba, bo obie liczą **to samo** dobowe zapotrzebowanie: rozdzielenie
/// ich znaczyłoby policzenie go dwa razy i dwie okazje do rozjechania się.
fn zapas_startowy(
    store: &mut Store,
    cat: &Catalog,
    cfg: &InitialStock,
    seed: &magnat_world::SiteSeed,
    sloty: (SlotId, SlotId),
    firm: FirmId,
    rep: &mut PlantsReport,
) -> Vec<InventoryRule> {
    let (wejscie, wyjscie) = sloty;
    let (we, wy) = wejscia_wyjscia(cat, &seed.recipes);
    let mut reguly = Vec::new();
    for g in &we {
        let doba = dobowe_zuzycie(cat, &seed.recipes, *g, seed.capacity_scale);
        if doba <= 0 {
            continue;
        }
        let koszt = zasil(
            store,
            cat,
            wejscie,
            *g,
            Mass(doba * cfg.input_days),
            firm,
            Q::new(cfg.quality),
        );
        rep.stock_value = Money(rep.stock_value.get() + koszt.get());
        reguly.push(InventoryRule::daily_shop(
            *g,
            Mass(doba * cfg.reorder_days),
            Mass(doba * cfg.target_days),
        ));
    }
    for g in &wy {
        let doba = dobowa_produkcja(cat, &seed.recipes, *g, seed.capacity_scale);
        if doba <= 0 {
            continue;
        }
        let koszt = zasil(
            store,
            cat,
            wyjscie,
            *g,
            Mass(doba * cfg.output_days),
            firm,
            Q::new(cfg.quality),
        );
        rep.stock_value = Money(rep.stock_value.get() + koszt.get());
    }
    reguly
}

/// Liczniki mediów: prąd, gdy któraś receptura bierze energię, woda, gdy bierze wodę.
///
/// Brak licznika znaczy „zakład tego nie potrzebuje", a licznik odcięty — „potrzebuje
/// i nie ma". Te dwie rzeczy nie mogą znaczyć tego samego, bo pierwsza jest normą,
/// a druga zatrzymuje produkcję.
fn liczniki(
    zaklad: &mut PlantSite,
    cat: &Catalog,
    recipes: &[RecipeId],
    taryfa: magnat_supply::tuning::UtilityTuning,
) {
    let media = FirmId(Entity::new(FIRMA_MEDIOW, std::num::NonZeroU32::MIN));
    if recipes.iter().any(|r| cat.recipe(*r).energy.0 > 0) {
        zaklad.meters.push(UtilityMeter::new(
            UtilityService::Electricity,
            media,
            Money(taryfa.power_gr_per_kwh),
        ));
    }
    if recipes.iter().any(|r| cat.recipe(*r).water.0 > 0) {
        zaklad.meters.push(UtilityMeter::new(
            UtilityService::Water,
            media,
            Money(taryfa.water_gr_per_m3),
        ));
    }
}

/// Jedna linia na recepturę archetypu. Zwraca, ile linii powstało.
///
/// Zakład o dwóch recepturach ma **dwie** linie, a nie jedną przezbrajaną: przezbrojenie
/// kosztuje czas i masę (§7.1: 50 minut i 30 kg), więc zakład robiący obie rzeczy
/// na stałe trzyma dwie maszyny.
fn linie(
    zaklad: &mut PlantSite,
    cat: &Catalog,
    seed: &magnat_world::SiteSeed,
    czesc: Option<GoodId>,
) -> usize {
    let Some(czesc) = czesc else { return 0 };
    let mut ile = 0;
    for r in &seed.recipes {
        let rec = cat.recipe(*r);
        let Some(klasa_maszyny) = cat.machine_class_of(rec) else {
            continue;
        };
        let mut l = ProductionLine::new(
            klasa_maszyny,
            przepustowosc(rec, seed.capacity_scale),
            Energy(rec.energy.0.max(0)),
            czesc,
        );
        l.recipe = Some(*r);
        // `AL-16`: `ProductionLine::new` zostawia termin przeglądu na „nigdy", więc
        // linia pracująca bez przerwy schodzi z `condition` do zera po ~166 dobach
        // i staje na awarii, której nie ma czym naprawić. Termin nadaje ten, kto zna
        // harmonogram zakładu — czyli to miejsce.
        l.next_maintenance =
            SimMinute(u64::from(zaklad.schedule.maintenance_interval_hours).saturating_mul(60));
        zaklad.lines.push(l);
        ile += 1;
    }
    ile
}

/// Przypisanie złoża zakładowi z recepturą `Extraction`.
///
/// Koncentracja i głębokość są **kopiowane raz**, przy założeniu zakładu — to parametry
/// ekonomiczne, nie stan terenu (§5.10). Zakład wydobywczy bez złoża stoi na `Starved`
/// i to jest właściwa odpowiedź, a nie błąd danych: wypełniacz strefy przemysłowej
/// naprawdę nie ma czego kopać.
fn wydobycie(
    zaklad: &mut PlantSite,
    cat: &Catalog,
    seed: &magnat_world::SiteSeed,
    rep: &mut PlantsReport,
) {
    let kopalnia = seed
        .recipes
        .iter()
        .any(|r| matches!(cat.recipe(*r).source, RecipeSource::Extraction(_)));
    if !kopalnia {
        return;
    }
    match seed.deposit {
        Some(d) => {
            zaklad.mining = Some(MiningSite {
                deposit: d,
                concentration_pct: 60,
                depth_m: 400,
            });
            rep.mines += 1;
        }
        None => rep.mines_without_deposit += 1,
    }
}

/// Wkłada zapas startowy do slotu i zwraca jego wartość.
///
/// `MassIn::Initial` — nie `Produced` ani `Imported`. To jest pozycja **otwarcia**
/// bilansu masy, a nie masa powstała w grze: `prop_mass_conservation` czyta ją jako
/// `stock_start` i gdyby weszła jako `produced`, bilans domknąłby się na papierze,
/// a świat miałby w magazynach towar, którego nikt nie zrobił.
fn zasil(
    store: &mut Store,
    cat: &Catalog,
    slot: SlotId,
    good: GoodId,
    mass: Mass,
    owner: FirmId,
    q: Q,
) -> Money {
    if mass.0 <= 0 {
        return Money::ZERO;
    }
    let g = cat.good(good);
    // Wycena po cenie zewnętrznej: zapas startowy jest tym, co poprzedni właściciel
    // kupił, zanim zaczęliśmy liczyć. Towar nieimportowalny (woda wodociągowa, odpady)
    // dostaje zero i to jest prawda, a nie luka — nikt za niego nie zapłacił.
    let cena = g.external_base_price.map_or(0, magnat_core::Money::get);
    let koszt = Money((i128::from(cena) * i128::from(mass.0) / 1_000_000) as i64);
    if store
        .put(
            cat,
            slot,
            BatchDraft {
                good,
                mass,
                quality: q,
                brand: None,
                producer: owner,
                produced_at: SimMinute(0),
                cost: koszt,
                origin: magnat_supply::BatchOrigin::default(),
                flags: magnat_supply::BatchFlags::default(),
            },
            MassIn::Initial,
        )
        .is_err()
    {
        return Money::ZERO;
    }
    koszt
}

/// Mapa pozycji ramp dla wtyczki towarowej M4 (`AO-4`).
///
/// `RoadFreight::new` bierze ją argumentem właśnie dlatego, że ma ją ten, kto składa
/// świat, a `sim/supply` nie ma i mieć nie może: `SiteId` jest dla łańcucha nieprzezroczysty.
#[must_use]
pub fn rampy(city: &CityData) -> BTreeMap<SiteId, magnat_core::WorldCoord> {
    let mut m = BTreeMap::new();
    for (i, s) in city.sites.sites.iter().enumerate() {
        let Some(b) = city.buildings.buildings.get(s.building.0.index() as usize) else {
            continue;
        };
        m.insert(
            site_id(i),
            magnat_core::WorldCoord::new(
                ((b.aabb.min.x + b.aabb.max.x) / 2.0) as i32,
                ((b.aabb.min.y + b.aabb.max.y) / 2.0) as i32,
                0,
            ),
        );
    }
    m
}
