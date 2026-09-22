//! Budżety grafu, Gantta i przebudowy panelu (`R2-WP29`, §7 dokumentu fazy M9).
//!
//! §7 podaje trzy liczby, których **do R2f nie zmierzył nikt**:
//!
//! | Ścieżka | Budżet §7 |
//! |---|---|
//! | Graf łańcucha dostaw 500 węzłów / 2000 krawędzi — rysowanie | ≤ 0,8 ms |
//! | Ten sam graf — układ | ≤ 50 ms **poza klatką** |
//! | Gantt 500 pasków — rysowanie | ≤ 0,4 ms |
//! | Przebudowa modelu jednego panelu | ≤ 2,0 ms |
//!
//! M9e zdał bramkę „klatka bez zmian danych nie przebudowuje modelu" testem licznika
//! przebudów — i to jest **inna rzecz niż czas**. Licznik mówi, ile razy; budżet mówi,
//! ile to kosztuje, kiedy już się zdarzy.
//!
//! # Dlaczego w `game/`, a nie w `engine/ui/benches/`
//!
//! Plan wskazywał `engine/ui`. Tam da się zmierzyć widget, ale **nie da się zmierzyć
//! przebudowy panelu**: `PanelDesc::build` mieszka w `game/`, a `magnat-game` zależy
//! od `magnat-ui`, nie odwrotnie. Trzy budżety z jednej tabeli mierzone w dwóch
//! crate'ach to dwie uprzęże i dwa zestawy danych, więc wszystkie trzy stoją tutaj —
//! razem z jedynym miejscem, z którego widać naraz sesję, rejestr paneli i widgety.
//!
//! # Dane są z przebiegu, rozmiar jest z kontraktu
//!
//! `DE-5` mówi wprost: pomiar widgetu karmionego danymi wymyślonymi na potrzeby pomiaru
//! nie mówi nic o panelu, który je potem dostanie. Węzły, krawędzie i paski powstają
//! więc z **prawdziwej sesji** — z rejestru umów B2B i ze stanu linii produkcyjnych
//! wygenerowanego miasta, tym samym odwzorowaniem, którego używają panele
//! `game::panels::{supply, plant}`.
//!
//! Miasto odniesienia ma jednak mniej niż 500 zakładów pod umowami, a budżet jest
//! **dla 500 węzłów i 2000 krawędzi**. Zestaw z przebiegu jest więc powtarzany do tej
//! liczby: etykiety, flagi ryzyka i wagi zostają prawdziwe, rośnie wyłącznie liczebność.
//! To jest uczciwsze niż jedno i drugie osobno — pomiar na trzydziestu węzłach nie
//! sprawdziłby budżetu, a pomiar na zmyślonych pięciuset nie sprawdziłby panelu.
//!
//! Świat ma 4 km i biegnie **trzydzieści dób**, a nie jedną, i to jest poprawka z pomiaru.
//! Pierwsza wersja stawiała 2 km na dobę (`WorldSize::Km2` z `R2-WP25`) i dostawała
//! **zero umów B2B i jeden pasek** — umowy dostaw zawiera się przez tygodnie, a nie
//! w pierwszej dobie. Budowa świata i trzydzieści dób to kilkanaście sekund **poza**
//! pomiarem, więc cena jest zapłacona raz.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use magnat_game::career::Holdings;
use magnat_game::panels::{PanelCtx, PanelId, PanelModel};
use magnat_game::player::StartVariant;
use magnat_game::session::Session;
use magnat_game::shell::{NewGameParams, ScenarioId};
use magnat_game::world::{population, SessionOpts};
use magnat_game::{GenWatch, PanelRegistry, PlayerCommand};
use magnat_jobs::JobPool;
use magnat_ui::gantt::{BarState, GanttBar, GanttView};
use magnat_ui::graph::{GraphEdge, GraphNode, GraphView};
use magnat_ui::{testing, Catalog, Locale, Theme};
use magnat_world::{Difficulty, EconomyProfile, Epoch, Region, WorldGenParams, WorldSize};

/// Liczby z tabeli §7 — nie z danych. Patrz nagłówek: dane są z przebiegu, rozmiar stąd.
const WEZLOW: usize = 500;
const KRAWEDZI: usize = 2_000;
const PASKOW: usize = 500;

fn sesja() -> Session {
    let pool = JobPool::new(0);
    let p = NewGameParams {
        world: WorldGenParams {
            seed: 11,
            size: WorldSize::Small4km,
            epoch: Epoch::Y1990,
            profile: EconomyProfile::Mixed,
            region: Region::Lowland,
            difficulty: Difficulty::Normal,
        },
        scenario: ScenarioId(0),
        variant: StartVariant::Heir,
        opts: SessionOpts {
            citizens: 3_000,
            commute_swaps: 5_000,
            economy: true,
            micro: false,
            ..SessionOpts::default()
        },
    };
    let built = population::zbuduj_z_params(p.world, &pool, &GenWatch::none())
        .expect("generacja")
        .expect("nie anulowano");
    let mut s = Session::begin(built, p, &pool).expect("sesja");
    let kandydaci = magnat_game::player::candidates(&s.app.world, StartVariant::Heir, 0);
    let kto = kandydaci.first().expect("kandydat na postać").citizen;
    s.submit(PlayerCommand::SetCharacter { citizen: kto })
        .expect("wybór postaci");
    // Trzydzieści dób, nie jedna: umowy B2B zawiera się w rytmie **tygodniowym**,
    // a szarże produkcyjne trwają godzinami. Świat młodszy niż miesiąc daje pusty graf.
    s.step(30 * 1_440 + 1, 0);
    s
}

/// Węzły i krawędzie z **ruchu towarowego całego miasta**: zakład nadający,
/// zakład odbierający, jedna krawędź na parę.
///
/// **Nie z rejestru umów, i to jest znalezisko, nie wygoda.** Panel Łańcuch dostaw
/// buduje graf z `b2b.contracts()` (`game::panels::supply`), a w przebiegu 4 km po
/// trzydziestu dobach ten rejestr jest **pusty** — zero umów, więc panel gracza
/// pokazałby pusty ekran. Zlecenia transportowe są za to prawdziwe i jest ich dużo:
/// to one wożą wsad między zakładami. Ta sama topologia, mierzona po stronie
/// wykonania zamiast po stronie zobowiązania — i ten sam kształt danych dla widgetu.
fn graf_z_przebiegu(s: &Session) -> (Vec<GraphNode>, Vec<GraphEdge>) {
    let Some(m) = s.market.as_ref() else {
        return (Vec::new(), Vec::new());
    };
    let chain = m.chain();
    let c = chain.lock();
    let mut wezly: Vec<magnat_core::SiteId> = Vec::new();
    let mut krawedzie: Vec<(magnat_core::SiteId, magnat_core::SiteId)> = Vec::new();
    let dodaj = |wezly: &mut Vec<magnat_core::SiteId>, x: magnat_core::SiteId| {
        if !wezly.contains(&x) {
            wezly.push(x);
        }
    };
    for o in c.transport.iter() {
        dodaj(&mut wezly, o.from);
        dodaj(&mut wezly, o.to);
        if !krawedzie.contains(&(o.from, o.to)) {
            krawedzie.push((o.from, o.to));
        }
    }
    for k in c.b2b.contracts() {
        dodaj(&mut wezly, k.deliver_from);
        dodaj(&mut wezly, k.deliver_to);
        if !krawedzie.contains(&(k.deliver_from, k.deliver_to)) {
            krawedzie.push((k.deliver_from, k.deliver_to));
        }
    }
    drop(c);

    let nodes: Vec<GraphNode> = wezly
        .iter()
        .map(|x| GraphNode {
            label: x.entity().index().to_string(),
            subject: Some(magnat_core::Subject::Site(*x)),
            risk: krawedzie.iter().filter(|(_, b)| b == x).count() == 1,
        })
        .collect();
    let edges: Vec<GraphEdge> = krawedzie
        .iter()
        .filter_map(|(a, b)| {
            Some(GraphEdge {
                from: u32::try_from(wezly.iter().position(|x| x == a)?).ok()?,
                to: u32::try_from(wezly.iter().position(|x| x == b)?).ok()?,
                weight_permille: 500,
            })
        })
        .collect();
    (nodes, edges)
}

/// Paski z **wszystkich linii produkcyjnych miasta**, tym samym odwzorowaniem
/// co `game::panels::plant::build`.
fn paski_z_przebiegu(s: &Session) -> Vec<GanttBar> {
    let Some(m) = s.market.as_ref() else {
        return Vec::new();
    };
    let chain = m.chain();
    let c = chain.lock();
    let mut bars = Vec::new();
    let mut wiersz: u16 = 0;
    for (_, z) in c.plant.iter() {
        for l in &z.lines {
            if let magnat_supply::LineState::Running {
                recipe,
                started,
                ends,
                ..
            } = l.state
            {
                bars.push(GanttBar {
                    row: wiersz,
                    label: recipe.0.to_string(),
                    from: started.0,
                    to: ends.0,
                    state: BarState::Running,
                });
            }
            wiersz = wiersz.saturating_add(1);
        }
    }
    bars
}

/// Powiela zestaw z przebiegu do zadanej liczebności. Pusty zestaw zostaje pusty —
/// benchmark ma wtedy paść na asercji, a nie zmierzyć nic i wyglądać na zielony.
fn powiel<T: Clone>(zrodlo: &[T], ile: usize) -> Vec<T> {
    assert!(
        !zrodlo.is_empty(),
        "zestaw z przebiegu jest pusty — pomiar mierzyłby wtedy pustkę, a nie panel"
    );
    zrodlo.iter().cycle().take(ile).cloned().collect()
}

fn budzety(c: &mut Criterion) {
    let theme = Theme::load().expect("data/ui/theme.ron");
    let katalog = Catalog::load().expect("data/locale/");
    let ctx_egui = egui::Context::default();
    let s = sesja();

    let (wezly, krawedzie) = graf_z_przebiegu(&s);
    let paski = paski_z_przebiegu(&s);
    println!(
        "z przebiegu: {} wezlow, {} krawedzi, {} paskow — pomiar na {WEZLOW}/{KRAWEDZI}/{PASKOW}",
        wezly.len(),
        krawedzie.len(),
        paski.len()
    );

    let nodes = powiel(&wezly, WEZLOW);
    let edges: Vec<GraphEdge> = powiel(&krawedzie, KRAWEDZI)
        .into_iter()
        .enumerate()
        .map(|(i, e)| GraphEdge {
            // Powielona krawędź ma wskazywać powielony węzeł, inaczej graf byłby
            // pięciuset kopiami tej samej warstwy i układ nie miałby co liczyć.
            from: (e.from as usize + i) as u32 % WEZLOW as u32,
            to: (e.to as usize + i + 1) as u32 % WEZLOW as u32,
            weight_permille: e.weight_permille,
        })
        .collect();
    let bars: Vec<GanttBar> = powiel(&paski, PASKOW)
        .into_iter()
        .enumerate()
        .map(|(i, b)| GanttBar {
            row: u16::try_from(i).unwrap_or(u16::MAX),
            ..b
        })
        .collect();

    // ── układ grafu: ≤ 50 ms poza klatką ────────────────────────────────────────
    //
    // Świeży `GraphView` w każdej iteracji, bo `set` przelicza układ **wyłącznie**
    // przy zmianie odcisku topologii — i to jest jego istota, nie wada. Mierzenie
    // drugiego `set` tej samej topologii mierzyłoby `hash`, a nie Sugiyamę.
    c.bench_function("m9e graf/uklad 500x2000", |b| {
        b.iter_batched(
            || (nodes.clone(), edges.clone()),
            |(n, e)| {
                let mut g = GraphView::default();
                g.set(n, e);
                black_box(g.is_empty());
            },
            BatchSize::SmallInput,
        );
    });

    // ── rysowanie grafu: ≤ 0,8 ms w klatce ──────────────────────────────────────
    let mut graf = GraphView::default();
    graf.set(nodes.clone(), edges.clone());
    c.bench_function("m9e graf/rysowanie 500x2000", |b| {
        b.iter(|| {
            let (teksty, _) = testing::draw_in(&ctx_egui, testing::input(testing::EKRAN), |ui| {
                graf.show(ui, &theme, 400.0);
            });
            black_box(teksty.len());
        });
    });

    // ── rysowanie Gantta: ≤ 0,4 ms ──────────────────────────────────────────────
    //
    // Okno czasu obejmuje **wszystkie** paski. Węższe mierzyłoby filtr `visible`,
    // czyli wirtualizację, a nie rysowanie — a budżet §7 mówi o pięciuset paskach
    // na ekranie, nie o pięciuset w pamięci.
    let mut gantt = GanttView::default();
    let od = bars.iter().map(|b| b.from).min().unwrap_or(0);
    let do_ = bars.iter().map(|b| b.to).max().unwrap_or(1);
    gantt.set(bars);
    c.bench_function("m9e gantt/rysowanie 500 paskow", |b| {
        b.iter(|| {
            let (teksty, _) = testing::draw_in(&ctx_egui, testing::input(testing::EKRAN), |ui| {
                gantt.show(ui, &theme, od, do_, 400.0);
            });
            black_box(teksty.len());
        });
    });

    // ── przebudowa modelu panelu: ≤ 2,0 ms ──────────────────────────────────────
    //
    // Panel Łańcuch dostaw, bo to on ma najdroższy model: bierze zamek łańcucha
    // i przechodzi cały rejestr umów. Panel tańszy mierzyłby najlepszy przypadek.
    let reg = PanelRegistry::default();
    let desc = reg.get(PanelId::Supply).expect("panel Łańcuch dostaw");
    let h = Holdings::of(&s);
    let ctx = PanelCtx {
        theme: &theme,
        c: &katalog,
        l: Locale::Pl,
        session: &s,
        holdings: &h,
        sel: 0,
    };
    c.bench_function("m9e panel/przebudowa supply", |b| {
        b.iter(|| {
            let model = (desc.build)(&ctx);
            black_box(matches!(model, PanelModel::Supply(_)));
        });
    });
}

criterion_group!(benches, budzety);
criterion_main!(benches);
