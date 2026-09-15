//! Kroki 8 i 9: zasiew wiedzy o miejscach i relacje startowe.
//!
//! Wydzielone z `population/mod.rs` w R-WP4 bez zmiany zachowania.

use super::*;

// ── kroki 8 i 9: wiedza i relacje ───────────────────────────────────────────────

/// Krok 8: zasiew wiedzy (§5.7, korekta E-2).
///
/// Bez tego kroku **nic się w mieście nie wydarzy**: `candidates` zwraca wyłącznie
/// miejsca znane mieszkańcowi, więc mieszkaniec bez wpisów dostaje `PlaceUnknown`
/// i nigdzie nie idzie. To jest warunek działania scenariusza, a nie ozdoba.
pub(super) fn zasiej_wiedze(
    world: &mut World,
    mieszkancy: &[Entity],
    places: &PlaceTable,
    seed: &table::KnowledgeSeed,
) -> u64 {
    /// Rodzaje miejsc, które w ogóle zaspokajają potrzeby (`data/needs/needs.ron`).
    /// `Home` i `Workplace` mieszkaniec zna z definicji — one nie wymagają zasiewu.
    const POTRZEBNE: [PlaceKind; 8] = [
        PlaceKind::Grocery,
        PlaceKind::Eatery,
        PlaceKind::Clothing,
        PlaceKind::Doctor,
        PlaceKind::Pharmacy,
        PlaceKind::Hospital,
        PlaceKind::Leisure,
        PlaceKind::Social,
    ];
    let mut wpisow = 0u64;
    let mut na_trasie: ArrayVec<PlaceRef, MAX_ON_ROUTE> = ArrayVec::new();
    // Sąsiedztwo jest cechą **adresu**, nie mieszkańca: w jednym budynku mieszka
    // kilkanaście osób i wszystkie mają te same sklepy za rogiem. Zapytanie
    // przestrzenne robi się więc raz na budynek, a nie raz na człowieka —
    // przy metropolii to różnica między 19 tys. a 274 tys. zapytań.
    let mut okolica: BTreeMap<u32, Vec<u32>> = BTreeMap::new();

    for c in mieszkancy {
        let Some(dom) = world.get::<Residence>(*c).and_then(home_place) else {
            continue;
        };
        let Some(at) = places.coord_of(dom) else {
            continue;
        };
        let budynek = magnat_agents::knowledge_key(dom).unwrap_or(u32::MAX);

        let blisko = okolica.entry(budynek).or_insert_with(|| {
            // Po najbliższych, nie po wszystkich: magazyn wiedzy ma 32 wpisy,
            // a w centrum w promieniu 400 m bywa ich kilkaset.
            let mut v: Vec<(i64, u32)> = Vec::new();
            for kind in POTRZEBNE {
                places.for_each_near(kind, at, f32::from(seed.home_radius_m), |e| {
                    if let Some(k) = magnat_agents::knowledge_key(e.place) {
                        v.push((e.at.distance_sq_xy(at), k));
                    }
                });
            }
            v.sort_unstable();
            v.truncate(usize::from(seed.max_near_home));
            v.into_iter().map(|(_, k)| k).collect()
        });
        let blisko = blisko.clone();
        for k in &blisko {
            social::learn_place(world, *c, *k, KnowledgeKind::Visited, seed.home_score, 0);
            wpisow += 1;
        }

        // Miejsce pracy i miejsca widoczne z trasy dom↔praca.
        let praca = world.get::<Employment>(*c).and_then(|e| site_place(e.site));
        if let Some(p) = praca {
            if let Some(k) = magnat_agents::knowledge_key(p) {
                social::learn_place(world, *c, k, KnowledgeKind::Visited, seed.work_score, 0);
                wpisow += 1;
            }
            for k in korytarz(places, at, p, &mut na_trasie) {
                social::learn_place(
                    world,
                    *c,
                    k,
                    KnowledgeKind::SeenOnRoute,
                    seed.route_score,
                    0,
                );
                wpisow += 1;
            }
        }
    }
    wpisow
}

/// Miejsca widoczne po drodze dom↔praca, liczone z **korytarza wokół odcinka**,
/// a nie z odtworzonej trasy.
///
/// `ponytail:` `WalkOracle::places_on_route` daje prawdziwą trasę po ulicach, ale kosztuje
/// jedną Dijkstrę na mieszkańca — 90 µs × 270 tys. to 25 s z budżetu 30 s na cały Etap 8
/// (§7.4 `gen_perf`). Sufit nazwany: korytarz nie widzi objazdu wokół rzeki ani wiaduktu,
/// więc mieszkaniec zza mostu pozna czasem sklep, którego nie mija. W **rozgrywce** trasą
/// zajmuje się `places_on_route` i to ona rozdaje wiedzę z codziennych dojazdów; tu chodzi
/// o zasiew startowy, którego i tak nikt nie widział. Prawdziwa trasa wejdzie, kiedy M4
/// zastąpi ten moduł `engine/nav` z routingiem wielokrotnego użytku (K-2).
fn korytarz(
    places: &PlaceTable,
    dom: WorldCoord,
    praca: PlaceRef,
    bufor: &mut ArrayVec<PlaceRef, MAX_ON_ROUTE>,
) -> Vec<u32> {
    /// Ile punktów na odcinku dom↔praca próbkować.
    const PROBEK: i32 = 4;
    /// Promień korytarza w metrach — dwie pierzeje.
    const PROMIEN_M: f32 = 150.0;
    bufor.clear();
    let Some(cel) = places.coord_of(praca) else {
        return Vec::new();
    };
    let mut out: Vec<u32> = Vec::new();
    for k in 1..=PROBEK {
        let at = WorldCoord::new(
            dom.x + (cel.x - dom.x) / PROBEK * k,
            dom.y + (cel.y - dom.y) / PROBEK * k,
            0,
        );
        for kind in [PlaceKind::Grocery, PlaceKind::Eatery, PlaceKind::Pharmacy] {
            places.for_each_near(kind, at, PROMIEN_M, |e| {
                if let Some(key) = magnat_agents::knowledge_key(e.place) {
                    out.push(key);
                }
            });
        }
    }
    out.sort_unstable();
    out.dedup();
    out.truncate(MAX_ON_ROUTE);
    out
}

/// Krok 9: relacje startowe — współpracownicy z tego samego zakładu i sąsiedzi
/// z tego samego kwartału.
///
/// Rodzina powstała już przy spawnie (`spawn_household_aged`). Tu dochodzą dwa
/// pozostałe źródła z §5.9: bez nich graf relacji ma same wyspy rodzinne i plotka
/// nie ma po czym chodzić.
pub(super) fn relacje_startowe(
    world: &mut World,
    mieszkancy: &[Entity],
    s: &table::StartingRelations,
    seed: u64,
) -> u64 {
    let waga_wsp = world
        .resource::<DemographyTable>()
        .social()
        .coworker_max
        .min(60);
    let waga_sas = world
        .resource::<DemographyTable>()
        .social()
        .neighbour_max
        .min(45);

    let mut w_zakladzie: BTreeMap<u32, Vec<Entity>> = BTreeMap::new();
    let mut w_kwartale: BTreeMap<u32, Vec<Entity>> = BTreeMap::new();
    for c in mieszkancy {
        if let Some(e) = world.get::<Employment>(*c) {
            if e.has_job() && e.flags & Employment::FLAG_PUPIL == 0 {
                w_zakladzie.entry(e.site).or_default().push(*c);
            }
        }
        if let Some(r) = world.get::<Residence>(*c) {
            let blok = world.resource::<CityFacts>().block(r.building);
            w_kwartale.entry(blok).or_default().push(*c);
        }
    }

    let mut krawedzi = 0u64;
    for (grupa, kind, waga, lo, hi, tick) in [
        (
            &w_zakladzie,
            RelationKind::Colleague,
            waga_wsp,
            s.coworkers_min,
            s.coworkers_max,
            8u64,
        ),
        (
            &w_kwartale,
            RelationKind::Neighbour,
            waga_sas,
            s.neighbours_min,
            s.neighbours_max,
            9u64,
        ),
    ] {
        for lista in grupa.values() {
            if lista.len() < 2 {
                continue;
            }
            for (i, a) in lista.iter().enumerate() {
                let mut r = rng(seed, StreamId::Relations, a.index(), Tick(tick));
                let ile = u32::from(lo) + r.gen_range_u32(u32::from(hi.saturating_sub(lo)) + 1);
                // Sąsiadami w liście są ci, którzy siedzą obok — kolejność listy jest
                // kolejnością indeksów encji, więc wybór jest deterministyczny i lokalny.
                for k in 1..=ile as usize {
                    let j = (i + k) % lista.len();
                    if j == i {
                        break;
                    }
                    let b = lista[j];
                    if a.index() < b.index() {
                        demography::powiaz(world, *a, b, kind, waga, 0);
                        krawedzi += 1;
                    }
                }
            }
        }
    }
    krawedzi
}
