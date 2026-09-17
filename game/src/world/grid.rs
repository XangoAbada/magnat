//! Most „sieci przesyłowe": elektrownia, ujęcie wody i przyłącza zakładów
//! w świecie, który ma już miasto i gospodarkę (M8b WP3).
//!
//! # Czego tu nie ma
//!
//! Ani jednej reguły przepływu. Wszystko, co liczy bilans, zrzut i kaskadę,
//! mieszka w `magnat_traffic::utility`; ten plik wyłącznie **wiąże** zakład
//! gospodarki z przyłączem, dzielnicę generatora ze stacją i taryfę z licznikiem.
//!
//! Funkcja stoi tutaj, a nie w `sim/traffic`, z tego samego powodu co kataster
//! w `city.rs`: sieć wiąże dzielnicę generatora (`sim/world`) z zakładem
//! gospodarki (`sim/supply`), czyli dwie strony, których żadna nie widzi drugiej.
//! Most widzi obie — po to jest.
//!
//! # Które sieci powstają
//!
//! **Prąd, woda, gaz i ciepło.** Pierwsze dwie postawiła M8b, dwie kolejne —
//! M8c razem z popytem grzewczym, bo dopiero pogoda daje im odbiorcę: ciepło
//! i gaz są funkcją mrozu, a nie kalendarza, więc sieć bez pogody wiozłaby
//! stałą liczbę i wyglądałaby w raporcie tak samo jak sieć, która działa (`R2`).
//! Odpady i telekomunikacja czekają na usługi miejskie z M8d — z tego samego powodu.
//!
//! Sieci grzewcze są **bez przyłączy przemysłowych**: zakład grzeje halę tak samo
//! w lipcu i w styczniu, a jego pobór gazu technologicznego liczy już licznik M6b.
//! Odbiorcą jest gospodarstwo domowe, jeden węzeł na dzielnicę — i to ono zapala
//! zrzut obciążenia w mroźny tydzień.

use std::error::Error;

use magnat_core::{SiteId, UtilityService};
use magnat_ecs::World;
use magnat_supply::ChainHandle;
use magnat_traffic::utility::{
    register_grids, GridTuning, LoadProfile, UtilityEdge, UtilityGrids, UtilityNetwork, UtilityNode,
};
use magnat_world::CityData;

/// Co most postawił — liczby do raportu scenariusza.
pub struct GridSetup {
    pub nets: usize,
    pub nodes: usize,
    pub edges: usize,
    /// Szczytowa moc przyłączona sieci energetycznej, w watach.
    pub peak_w: i64,
    /// Moc elektrowni, w watach.
    pub source_w: i64,
    /// Ile zakładów ma przyłącze prądu, a ile wody.
    pub powered_sites: usize,
    pub watered_sites: usize,
    /// Moc grzewcza przyłączona w sieciach ciepła i gazu, w watach przy pełnym
    /// obciążeniu (200 stopniodni). W lipcu sieć wiezie kilka procent tej liczby.
    pub heating_w: i64,
}

/// Stawia sieci przesyłowe w gotowym świecie.
///
/// Kolejność jest wymuszona: **zakłady muszą już stać**, bo moc przyłączeniowa
/// bierze się z linii produkcyjnych, a przyłącze wiąże się kluczem przesuniętym
/// o `SITE_KEY_BASE` (`K-46`) — tym samym, który nadał most zakładów.
///
/// # Errors
/// Zwraca błąd, gdy `data/tuning/grid.ron` nie da się wczytać albo gdy w świecie
/// nie ma łańcucha dostaw, z którego bierze się lista zakładów.
pub fn setup(world: &mut World, city: &CityData) -> Result<GridSetup, Box<dyn Error>> {
    let tuning = GridTuning::load_default()?;
    let chain = world
        .get_resource::<ChainHandle>()
        .cloned()
        .ok_or("świat bez łańcucha dostaw — nie ma zakładów, do których doprowadzić prąd")?;

    // Zakład → dzielnica, przez parcelę. Kolejność rosnąca po `SiteId`, bo
    // kolejność węzłów jest kolejnością zrzutu obciążenia przy równym priorytecie
    // i nie wolno jej zostawić kolejności, w jakiej generator stawiał zakłady.
    let mut przylacza: Vec<(SiteId, u16, i64, bool)> = Vec::new();
    {
        let c = chain.lock();
        for (i, s) in city.sites.sites.iter().enumerate() {
            let site = crate::world::plants::site_id(i);
            let Some(z) = c.plant.get(site) else { continue };
            let dzielnica = city
                .parcels
                .parcels
                .get(s.parcel.0.index() as usize)
                .map_or(0, |p| p.district.0);
            let prad = z.meter(UtilityService::Electricity).is_some();
            let woda = z.meter(UtilityService::Water).is_some();
            if !prad && !woda {
                continue;
            }
            let moc = z.lines.iter().map(|l| l.power_draw.0).sum::<i64>();
            przylacza.push((site, dzielnica, if prad { moc } else { 0 }, woda));
        }
    }
    przylacza.sort_by_key(|(s, _, _, _)| s.0.index());

    let dzielnic = city.districts.districts.len().max(1);
    let mieszkancow = magnat_agents::society::population(world) as i64;
    let na_dzielnice = mieszkancow / dzielnic as i64;

    // Zakład prowadzący źródło: ten, który w recepturze **wytwarza** medium.
    // Bez tego wiązania sondy „blok ma 32 lata" i „90 dób zaległej konserwacji"
    // nie miałyby skąd wziąć liczby, bo źródło w grafie sieci jest węzłem,
    // a nie maszyną (`CD-2`).
    let wytworcy = wytworcy_mediow(&chain, city);

    let (prad, moc_szczyt, moc_zrodla, ile_prad) = siec_pradu(
        &tuning,
        &przylacza,
        dzielnic,
        na_dzielnice,
        wytworcy.get(&UtilityService::Electricity).copied(),
    );
    let (woda, ile_woda) = siec_wody(
        &tuning,
        &przylacza,
        dzielnic,
        na_dzielnice,
        wytworcy.get(&UtilityService::Water).copied(),
    );
    let (cieplo, moc_ciepla) = siec_grzewcza(
        &tuning,
        UtilityService::Heat,
        tuning.household_heat_w,
        dzielnic,
        na_dzielnice,
        wytworcy.get(&UtilityService::Heat).copied(),
    );
    let (gaz, moc_gazu) = siec_grzewcza(
        &tuning,
        UtilityService::Gas,
        tuning.household_gas_w,
        dzielnic,
        na_dzielnice,
        wytworcy.get(&UtilityService::Gas).copied(),
    );

    let mut grids = UtilityGrids::default();
    let raport = GridSetup {
        nets: 4,
        nodes: prad.nodes.len() + woda.nodes.len() + cieplo.nodes.len() + gaz.nodes.len(),
        edges: prad.edges.len() + woda.edges.len() + cieplo.edges.len() + gaz.edges.len(),
        peak_w: moc_szczyt,
        source_w: moc_zrodla,
        powered_sites: ile_prad,
        watered_sites: ile_woda,
        heating_w: moc_ciepla + moc_gazu,
    };
    grids.push(prad);
    grids.push(woda);
    grids.push(cieplo);
    grids.push(gaz);
    // Ujęcie wody stoi na prądzie: sieć 0, węzeł 1 (`WATERWORKS`) zasila źródło
    // sieci 1. To nie jest ozdoba — to jedyny powód, dla którego priorytet 0
    // istnieje, i jedyna droga, którą blackout sięga dalej niż do jednej sieci.
    grids.link_source(0, WATERWORKS, 1, 0);
    register_grids(world, grids);
    Ok(raport)
}

/// Węzeł ujęcia wody w sieci energetycznej. Stały indeks, bo wiąże dwie sieci.
const WATERWORKS: u32 = 1;

/// Sieć energetyczna: elektrownia, ujęcie wody, pierścień stacji dzielnicowych
/// i promieniste przyłącza.
///
/// Pierścień, a nie gwiazda, i to jest decyzja o kształcie: magistrala w pierścieniu
/// **nie ma ani jednego mostu**, więc nie wypada z przeciążenia — dopóki ktoś jej
/// nie otworzy. Dopiero otwarty pierścień zamienia się w ścieżkę, jej odcinki stają
/// się mostami i zaczynają wypadać po kolei. To jest awaria N-1 i dokładnie tak
/// kaskadują prawdziwe sieci.
fn siec_pradu(
    t: &GridTuning,
    przylacza: &[(SiteId, u16, i64, bool)],
    dzielnic: usize,
    mieszkancow_na_dzielnice: i64,
    elektrownia: Option<SiteId>,
) -> (UtilityNetwork, i64, i64, usize) {
    let mut nodes = vec![
        UtilityNode::source(0),
        // Ujęcie wody: priorytet 0, odbiór ciągły. Gaśnie ostatnie, bo jego
        // zgaśnięcie zabiera wodę całemu miastu.
        UtilityNode::connection(
            None,
            t.priority_critical,
            t.household_w * mieszkancow_na_dzielnice.max(1) * dzielnic as i64 / 40,
            LoadProfile::Flat,
        ),
    ];
    let mut edges = Vec::new();
    let hub0 = nodes.len() as u32;
    for _ in 0..dzielnic {
        nodes.push(UtilityNode::hub());
    }
    pierscien(&mut edges, hub0, dzielnic, t.backbone_capacity_bps, t);
    // Przyłącze ujęcia wody wisi u pierwszej stacji i ma przepustowość magistrali,
    // a nie własnego popytu: zabezpieczenie, które wywala wodociąg z sieci, byłoby
    // drugą drogą do tego samego skutku co zrzut obciążenia — a tamta ma priorytet
    // i ma go z powodu, który tutaj też obowiązuje.
    edges.push(UtilityEdge::new(
        hub0,
        WATERWORKS,
        i64::from(t.backbone_capacity_bps) * 100_000,
        t.loss_bps_spur,
    ));

    let mut ile = 0;
    for &(site, dzielnica, moc, _) in przylacza {
        if moc <= 0 {
            continue;
        }
        let i = nodes.len() as u32;
        nodes.push(UtilityNode::connection(
            Some(site),
            t.priority_industry,
            moc,
            LoadProfile::Industry,
        ));
        edges.push(UtilityEdge::new(
            hub0 + u32::from(dzielnica).min(dzielnic as u32 - 1),
            i,
            moc * i64::from(t.spur_capacity_bps) / 10_000,
            t.loss_bps_spur,
        ));
        ile += 1;
    }
    // Gospodarstwa domowe: jeden węzeł na dzielnicę. Nie mają licznika (płacą
    // ryczałt w kopercie, M5d), ale **obciążają sieć** i podlegają zrzutowi —
    // bez nich przemysł byłby jedynym odbiorcą i priorytety nie miałyby czego
    // szeregować.
    for d in 0..dzielnic as u32 {
        let moc = t.household_w * mieszkancow_na_dzielnice.max(1);
        let i = nodes.len() as u32;
        nodes.push(UtilityNode::connection(
            None,
            t.priority_household,
            moc,
            LoadProfile::Household,
        ));
        edges.push(UtilityEdge::new(
            hub0 + d,
            i,
            moc.max(1) * i64::from(t.spur_capacity_bps) / 10_000,
            t.loss_bps_spur,
        ));
    }

    let szczyt: i64 = nodes.iter().map(|n| n.base_demand).sum();
    let moc = szczyt * i64::from(t.source_margin_bps) / 10_000;
    nodes[0] = zrodlo(elektrownia, moc);
    let net = UtilityNetwork::new(
        UtilityService::Electricity,
        operator(0),
        nodes,
        edges,
        t.tariff(UtilityService::Electricity),
    );
    (net, szczyt, moc, ile)
}

/// Sieć wodociągowa: ujęcie, pierścień stacji i przyłącza zakładów, które biorą
/// wodę do receptury.
///
/// Stan źródła przejmuje co krok zasilanie ujęcia (`UtilityGrids::link_source`):
/// ujęcie bez prądu wyłącza pompy, a miasto bez prądu jest miastem bez wody.
/// Startowe `online: true` trzyma się dokładnie do pierwszego kroku sieci.
fn siec_wody(
    t: &GridTuning,
    przylacza: &[(SiteId, u16, i64, bool)],
    dzielnic: usize,
    mieszkancow_na_dzielnice: i64,
    ujecie: Option<SiteId>,
) -> (UtilityNetwork, usize) {
    let mut nodes = vec![UtilityNode::source(0)];
    let mut edges = Vec::new();
    let hub0 = nodes.len() as u32;
    for _ in 0..dzielnic {
        nodes.push(UtilityNode::hub());
    }
    pierscien(&mut edges, hub0, dzielnic, t.backbone_capacity_bps, t);

    let mut ile = 0;
    for &(site, dzielnica, _, woda) in przylacza {
        if !woda {
            continue;
        }
        let i = nodes.len() as u32;
        let pobor = t.plant_water_ml_h;
        nodes.push(UtilityNode::connection(
            Some(site),
            t.priority_industry,
            pobor,
            LoadProfile::Industry,
        ));
        edges.push(UtilityEdge::new(
            hub0 + u32::from(dzielnica).min(dzielnic as u32 - 1),
            i,
            pobor * i64::from(t.spur_capacity_bps) / 10_000,
            t.loss_bps_spur,
        ));
        ile += 1;
    }
    for d in 0..dzielnic as u32 {
        let pobor = t.household_water_ml_h * mieszkancow_na_dzielnice.max(1);
        let i = nodes.len() as u32;
        nodes.push(UtilityNode::connection(
            None,
            t.priority_household,
            pobor,
            LoadProfile::Household,
        ));
        edges.push(UtilityEdge::new(
            hub0 + d,
            i,
            pobor.max(1) * i64::from(t.spur_capacity_bps) / 10_000,
            t.loss_bps_spur,
        ));
    }

    let szczyt: i64 = nodes.iter().map(|n| n.base_demand).sum();
    nodes[0] = zrodlo(ujecie, szczyt * i64::from(t.source_margin_bps) / 10_000);
    let net = UtilityNetwork::new(
        UtilityService::Water,
        operator(1),
        nodes,
        edges,
        t.tariff(UtilityService::Water),
    );
    (net, ile)
}

/// Źródło z właścicielem albo bez. Węzeł bez zakładu jest poprawnym stanem:
/// miasto, w którym nikt nie produkuje prądu, i tak go skądś ma — z systemu.
/// Traci za to sondy wieku i konserwacji, więc awaria bloku takiej elektrowni
/// zachodzi wyłącznie z szansy bazowej.
fn zrodlo(site: Option<SiteId>, moc: i64) -> UtilityNode {
    match site {
        Some(s) => UtilityNode::source_owned(s, moc),
        None => UtilityNode::source(moc),
    }
}

/// Sieć grzewcza — ciepłownicza albo gazowa. Jeden kształt na oba media, bo
/// różnią się wyłącznie stawką i mocą na mieszkańca; odbiorcą jest gospodarstwo
/// domowe, a poborem steruje mnożnik pogodowy sieci (`LoadProfile::Heating`).
///
/// Bez przyłączy przemysłowych i to jest decyzja: zakład grzeje halę tak samo
/// przez cały rok, a jego gaz technologiczny liczy już licznik M6b. Osobny węzeł
/// rozbiłby tę samą liczbę na dwie i nie zmieniłby ani chwili, w której zapala
/// się zrzut — ta sama odpowiedź, którą M8b dała sklepom (`CC-17`).
fn siec_grzewcza(
    t: &GridTuning,
    service: UtilityService,
    moc_na_mieszkanca: i64,
    dzielnic: usize,
    mieszkancow_na_dzielnice: i64,
    wytworca: Option<SiteId>,
) -> (UtilityNetwork, i64) {
    let mut nodes = vec![UtilityNode::source(0)];
    let mut edges = Vec::new();
    let hub0 = nodes.len() as u32;
    for _ in 0..dzielnic {
        nodes.push(UtilityNode::hub());
    }
    pierscien(&mut edges, hub0, dzielnic, t.backbone_capacity_bps, t);
    for d in 0..dzielnic as u32 {
        let moc = moc_na_mieszkanca * mieszkancow_na_dzielnice.max(1);
        let i = nodes.len() as u32;
        nodes.push(UtilityNode::connection(
            None,
            t.priority_household,
            moc,
            LoadProfile::Heating,
        ));
        edges.push(UtilityEdge::new(
            hub0 + d,
            i,
            moc.max(1) * i64::from(t.spur_capacity_bps) / 10_000,
            t.loss_bps_spur,
        ));
    }
    let szczyt: i64 = nodes.iter().map(|n| n.base_demand).sum();
    // Ciepłownia wymiaruje się na **szczyt**, czyli na mróz. Zapas jest ten sam
    // co w elektrowni i z tego samego powodu: bez niego pierwszy mroźny tydzień
    // gasiłby miasto co roku, a to nie jest zdarzenie, tylko błąd wymiarowania.
    nodes[0] = zrodlo(wytworca, szczyt * i64::from(t.source_margin_bps) / 10_000);
    let net = UtilityNetwork::new(
        service,
        operator(u32::try_from(service.as_index()).unwrap_or(0)),
        nodes,
        edges,
        t.tariff(service),
    );
    (net, szczyt)
}

/// Zakład, który **wytwarza** dane medium — po kluczu towaru w wyjściu receptury.
///
/// Pierwszy po identyfikatorze, bo miasto z dwiema elektrowniami i tak ma w sieci
/// jedno źródło (`CC-15`), a wybór „którą z nich prowadzi to źródło" nie może
/// zależeć od kolejności iteracji areny.
fn wytworcy_mediow(
    chain: &ChainHandle,
    city: &CityData,
) -> std::collections::BTreeMap<UtilityService, SiteId> {
    let mut out = std::collections::BTreeMap::new();
    let c = chain.lock();
    for i in 0..city.sites.sites.len() {
        let site = crate::world::plants::site_id(i);
        let Some(z) = c.plant.get(site) else { continue };
        for l in &z.lines {
            let Some(r) = l.recipe else { continue };
            for o in &chain.cat.recipe(r).outputs {
                let usluga = match &*chain.cat.good(o.good).key {
                    "util_electricity" => UtilityService::Electricity,
                    "util_water" => UtilityService::Water,
                    "util_heat" => UtilityService::Heat,
                    "util_gas" => UtilityService::Gas,
                    _ => continue,
                };
                out.entry(usluga).or_insert(site);
            }
        }
    }
    out
}

/// Pierścień magistralny: źródło → stacja 0 → … → stacja n−1 → źródło.
fn pierscien(
    edges: &mut Vec<UtilityEdge>,
    hub0: u32,
    dzielnic: usize,
    capacity_bps: u32,
    t: &GridTuning,
) {
    let przepustowosc = i64::from(capacity_bps) * 100_000;
    edges.push(UtilityEdge::new(
        0,
        hub0,
        przepustowosc,
        t.loss_bps_backbone,
    ));
    for d in 1..dzielnic as u32 {
        edges.push(UtilityEdge::new(
            hub0 + d - 1,
            hub0 + d,
            przepustowosc,
            t.loss_bps_backbone,
        ));
    }
    if dzielnic > 1 {
        edges.push(UtilityEdge::new(
            hub0 + dzielnic as u32 - 1,
            0,
            przepustowosc,
            t.loss_bps_backbone,
        ));
    }
}

/// Operator sieci. `ponytail:` sufit — identyfikator bez księgi i bez konta,
/// tak samo jak `FIRMA_MEDIOW` w moście zakładów od M6b: pieniądz za media
/// wychodzi na konto reszty świata, bo operator jako firma z rachunkiem wyników
/// jest decyzją `D10` odłożoną do M9/M10 („przejęcie operatora przez gracza").
/// Droga wyjścia: założyć firmę w rejestrze M7 i skierować `absorb_utility_bills`
/// na jej konto — reszta modelu się nie zmienia, bo taryfa już dziś należy do sieci.
fn operator(i: u32) -> magnat_core::FirmId {
    magnat_core::FirmId(magnat_core::Entity::new(
        u32::MAX - 20 - i,
        std::num::NonZeroU32::MIN,
    ))
}
