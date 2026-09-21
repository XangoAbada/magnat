//! P6 — erozja: stream-power, dyfuzja zboczowa, erozja termiczna (M1 §5.7).
//!
//! Najdroższy przebieg potoku i jedyny, w którym determinizm `f32` naprawdę jest zagrożony
//! (ryzyko R1). Zabezpieczenia są trzy i wszystkie są widoczne w kodzie:
//!
//! 1. **Kolejność jest ustalona przez dane, nie przez harmonogram.** Erozja rzeczna idzie po
//!    stosie odbiorników z P5, który jest tą samą permutacją niezależnie od liczby wątków.
//! 2. **Równoległość idzie po zlewniach**, a zlewnie są rozłączne: dwie komórki z różnych
//!    zlewni nigdy na siebie nie wpływają w kroku stream-power, więc podział pracy nie może
//!    zmienić wyniku.
//! 3. **Tylko `+ − × ÷` i `sqrt`.** Żadnego `powf`, żadnego `mul_add` (00 §K-6, M0 §2).
//!
//! ### Dlaczego praca idzie w przestrzeni stosu, a nie siatki
//!
//! Przejście po stosie w przestrzeni siatki to dostęp losowy do 67 MB tablicy — przy 16,8 mln
//! komórek każdy odczyt to chybienie w pamięci podręcznej i realny koszt rośnie do ~30 ns
//! na komórkę, czyli ~40 s na 80 iteracji. Po przenumerowaniu komórek na kolejność stosu
//! dostęp staje się sekwencyjny, a odbiornik ma **zawsze niższy indeks niż donor w obrębie
//! zlewni** — co jednocześnie daje rozłączne podzakresy do zrównoleglenia.

use crate::fields::NO_RECEIVER;
use crate::grid::{neighbor_dist_m, Grid2, NEIGHBORS_8};
use crate::params::WORK_CELL_M;
use crate::pipeline::GenCtx;
use serde::Deserialize;

/// Parametry z `data/geology/erosion.ron`.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct ErosionParams {
    pub schema_version: u32,
    pub stream_power_m: f32,
    pub stream_power_n: f32,
    pub dt_years: f32,
    pub hillslope_diffusion: f32,
    pub slope_every: u32,
    pub repose_angle_deg: f32,
    pub repose_relaxation: f32,
    /// Ile razy w trakcie erozji przeliczyć topologię odwodnienia, równomiernie po przebiegu.
    /// `0` = ani razu, czyli sieć rzeczna zostaje ta, którą wyznaczył szum przed erozją.
    ///
    /// Liczba przetrasowań, a nie „co N iteracji": iteracji erozji jest 40 na mapie 4 i 8 km,
    /// a 80 na 12 i 16 km, więc ta sama wartość N dawałaby różną liczbę przetrasowań zależnie
    /// od rozmiaru świata — a przy N = 40 nawet zero na mapie 4 km i jedno na 16 km, czyli
    /// dokładnie odwrotnie, niż każe budżet czasu. Patrz [`erode`].
    pub reroutes: u32,
}

pub const EROSION_SCHEMA_VERSION: u32 = 2;

/// Ile metrów wcina się **próg odpływowy** misy bezodpływowej przez modelowany czas.
///
/// To jest mechanizm, przez który jeziora znikają z krajobrazu: nie zasypuje ich muł
/// (to trwa dłużej), tylko rzeka przecina próg i misa się opróżnia. Konsekwencja jest
/// istotna dla kształtu mapy, nie tylko dla głębokości: misa płytsza niż próg **znika
/// w całości**, a z głębszej zostaje tylko część poniżej progu — czyli maleje też jej
/// powierzchnia. Samo mnożenie głębokości przez ułamek zmniejsza głębokość, ale zostawia
/// zasięg nietknięty, a to dawało w regionie górskim jeziora na 23–32 % mapy.
const OUTLET_INCISION_M: f32 = 6.0;
/// Ułamek pozostałej głębokości, który przetrwał zasypanie materiałem ze zlewni.
const SEDIMENT_INFILL: f32 = 0.6;

impl Default for ErosionParams {
    /// Wartości awaryjne, gdyby pliku nie było. Świadomie **identyczne** z plikiem:
    /// rozjazd między domyślną a plikową kalibracją oznaczałby, że świat wygląda inaczej
    /// zależnie od tego, czy `data/` jest na miejscu — i nikt by tego nie zauważył.
    fn default() -> Self {
        ErosionParams {
            schema_version: EROSION_SCHEMA_VERSION,
            stream_power_m: 0.5,
            stream_power_n: 1.0,
            dt_years: 150.0,
            hillslope_diffusion: 0.001,
            slope_every: 4,
            repose_angle_deg: 42.0,
            repose_relaxation: 0.5,
            reroutes: 0,
        }
    }
}

impl ErosionParams {
    pub fn load(path: &std::path::Path) -> Result<ErosionParams, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let p: ErosionParams = ron::from_str(&text).map_err(|e| e.to_string())?;
        if p.schema_version != EROSION_SCHEMA_VERSION {
            return Err(format!(
                "erosion.ron: schema_version {}, oczekiwano {EROSION_SCHEMA_VERSION}",
                p.schema_version
            ));
        }
        p.validate()?;
        Ok(p)
    }

    /// Warunek stabilności schematu jawnego dyfuzji. Naruszenie nie objawia się błędem,
    /// tylko terenem, który w kilka iteracji zamienia się w szachownicę — dlatego jest
    /// sprawdzane przy ładowaniu, a nie zostawione czujności strojącego.
    pub fn validate(&self) -> Result<(), String> {
        let dx = WORK_CELL_M as f32;
        let courant = self.hillslope_diffusion * self.dt_years / (dx * dx);
        if courant > 0.25 {
            return Err(format!(
                "dyfuzja niestabilna: D·dt/dx² = {courant:.3} > 0,25 \
                 (D = {}, dt = {})",
                self.hillslope_diffusion, self.dt_years
            ));
        }
        if self.stream_power_n != 1.0 {
            return Err("n ≠ 1 wymaga iteracji Newtona w kroku — patrz M1 §5.7".into());
        }
        Ok(())
    }
}

pub fn run(ctx: &mut GenCtx) {
    let params =
        ErosionParams::load(std::path::Path::new("data/geology/erosion.ron")).unwrap_or_default();
    erode(ctx, params);
}

/// Stan erozji w przestrzeni stosu. Wszystko tutaj jest pochodną topologii odwodnienia,
/// więc przetrasowanie unieważnia to naraz — stąd jedna struktura, a nie sześć wektorów
/// wożonych osobno.
struct Stos {
    /// Kolejność komórek od ujść w górę zlewni; `stack[k]` to indeks komórki w siatce.
    stack: Vec<u32>,
    /// Głębokość misy bezodpływowej per komórka siatki, **surowa** (`filled − height`).
    /// Wcięcie progu odpływowego nakłada dopiero [`rozsyp`], i tylko ten ostatni.
    depression: Vec<f32>,
    h: Vec<f32>,
    u: Vec<f32>,
    coef: Vec<f32>,
    /// Indeks odbiornika w przestrzeni stosu; `u32::MAX` = ujście.
    recv: Vec<u32>,
    ranges: Vec<(u32, u32)>,
}

/// Przenumerowuje komórki na kolejność stosu i liczy współczynniki kroku stream-power.
///
/// Wydzielone z [`erode`], bo przetrasowanie w trakcie erozji musi to powtórzyć: nowe
/// odbiorniki znaczą nowy stos, nowe zakresy zlewni i nowe pola akumulacji w `coef`.
fn przygotuj(ctx: &GenCtx, p: ErosionParams) -> Stos {
    let dim = ctx.dim();
    let n = dim * dim;

    // ── Zagłębienia jako osobna warstwa ──────────────────────────────────────────────
    //
    // Erozja rzeźbi **powierzchnię wypełnioną** z P4, nie surowy teren. Powód jest twardy:
    // wewnątrz misy bezodpływowej teren leży poniżej powierzchni trasowania, więc odbiornik
    // wyznaczony na powierzchni wypełnionej bywa wyżej od komórki — a schemat stream-power
    // nie ma wtedy czego zdejmować i misa zostaje nietknięta. Ponieważ ridged multifractal
    // produkuje setki takich mis, erozja „działała" na ułamku mapy (objaw: koryta pogłębiały
    // się o centymetry zamiast o metry).
    //
    // Głębokość misy zdejmujemy przed erozją i nakładamy po niej z powrotem. Dzięki temu
    // erozja pracuje na całej mapie, a jeziora nie znikają — stają się misami w **nowym**,
    // wyrzeźbionym terenie.
    let depression: Vec<f32> = (0..n)
        .map(|i| ctx.work.filled_m[i] - ctx.work.height_m[i])
        .collect();

    // ── Przenumerowanie na kolejność stosu ───────────────────────────────────────────
    let stack = ctx.work.stack.clone();
    let mut pos = vec![0u32; n];
    for (k, c) in stack.iter().enumerate() {
        pos[*c as usize] = k as u32;
    }

    let dist_lut: [f32; 8] =
        std::array::from_fn(|k| neighbor_dist_m(k, f64::from(WORK_CELL_M)) as f32);
    let mut h = vec![0.0f32; n];
    let mut u = vec![0.0f32; n];
    // Współczynnik erozji komórki: K · dt · A^m / dist. Liczony raz — w pętli zostaje
    // wyłącznie dzielenie i dwa mnożenia.
    let mut coef = vec![0.0f32; n];
    // Indeks odbiornika w przestrzeni stosu; `u32::MAX` = ujście, czyli poziom odniesienia.
    let mut recv = vec![u32::MAX; n];

    for (k, c) in stack.iter().enumerate() {
        let c = *c as usize;
        h[k] = ctx.work.filled_m[c];
        u[k] = ctx.work.uplift[c] * p.dt_years;
        let r = ctx.work.receiver[c];
        if r == NO_RECEIVER {
            continue;
        }
        recv[k] = pos[r as usize];
        // Odległość do odbiornika: osiowa albo po przekątnej, rozpoznana z różnicy indeksów.
        let (cx, cy) = ((c % dim) as i64, (c / dim) as i64);
        let (rx, ry) = ((r as usize % dim) as i64, (r as usize / dim) as i64);
        let po_ukosie = cx != rx && cy != ry;
        let d = dist_lut[usize::from(po_ukosie)];
        // A^m dla m = 0,5 to sqrt — jedyna funkcja nieelementarna w gorącej pętli,
        // i akurat ta jest dokładnie zaokrąglana w IEEE-754 (00 §K-6).
        let area_m = ctx.work.flow_acc[c].sqrt();
        coef[k] = ctx.work.erodibility[c] * p.dt_years * area_m / d;
    }

    Stos {
        stack,
        depression,
        h,
        u,
        coef,
        recv,
        ranges: ctx.work.basin_ranges.clone(),
    }
}

/// Odkłada wyerodowaną powierzchnię do `work.height_m`, zdejmując z niej misy.
///
/// `wciecie` nakłada wcięcie progu odpływowego i zasypanie osadem (patrz
/// [`OUTLET_INCISION_M`]) — robi to **wyłącznie ostatni** rozsyp. Przy przetrasowaniu
/// w trakcie erozji misa ma wrócić taka, jaka była: inaczej każde powtórzenie ścinałoby
/// jeziora o kolejne sześć metrów i liczba przetrasowań zmieniałaby powierzchnię jezior
/// mocniej niż sama erozja. Bez tego ścięcia mapa 16 km w regionie górskim wychodzi
/// z jeziorami na jednej trzeciej powierzchni i z 80 MB stanu trwałego wobec 60 MB
/// budżetu (M1 §5.9).
fn rozsyp(ctx: &mut GenCtx, s: &Stos, wciecie: bool) {
    for (k, c) in s.stack.iter().enumerate() {
        let c = *c as usize;
        let d = if wciecie {
            (s.depression[c] - OUTLET_INCISION_M).max(0.0) * SEDIMENT_INFILL
        } else {
            s.depression[c]
        };
        ctx.work.height_m[c] = s.h[k] - d;
    }
}

/// Ujście, do którego spływa każda komórka. Tożsamość zlewni odporna na przenumerowanie:
/// identyfikatory zlewni nadaje się kolejno rosnącym indeksom ujść, więc pojawienie się
/// jednego nowego ujścia przesunęłoby numery wszystkich zlewni za nim i każda komórka
/// wyglądałaby na przechwyconą.
fn ujscia_komorek(ctx: &GenCtx) -> Vec<u32> {
    let outlets = &ctx.work.outlets;
    ctx.work
        .basin
        .as_slice()
        .iter()
        .map(|b| outlets[*b as usize])
        .collect()
}

pub fn erode(ctx: &mut GenCtx, p: ErosionParams) {
    let dim = ctx.dim();
    let n = dim * dim;
    let iterations = ctx.params.size.erosion_iterations();
    if ctx.work.stack.len() != n {
        return; // brak trasowania — nic do zerodowania
    }

    // Odstęp między przetrasowaniami. `reroutes + 1` w mianowniku, bo ostatnie przypadłoby
    // na koniec przebiegu, gdzie i tak biegnie trasowanie końcowe — `r` przetrasowań dzieli
    // przebieg na `r + 1` odcinków.
    let reroute_step = match p.reroutes {
        0 => 0,
        r => (iterations / (r + 1)).max(1),
    };

    let mut s = przygotuj(ctx, p);
    let mut buf = vec![0.0f32; n];
    ctx.work.basin_captures = 0;

    for iter in 0..iterations {
        stream_power_step(ctx.pool, &mut s.h, &s.u, &s.coef, &s.recv, &s.ranges);

        if p.slope_every > 0 && (iter + 1).is_multiple_of(p.slope_every) {
            // Rozsypanie do siatki: dyfuzja i osuwanie działają na sąsiedztwie przestrzennym,
            // którego przestrzeń stosu nie zna.
            let mut grid = vec![0.0f32; n];
            for (k, c) in s.stack.iter().enumerate() {
                grid[*c as usize] = s.h[k];
            }
            let mut g = Grid2::from_vec(dim, grid);
            // Tyle podkroków, ile iteracji pominięto — ten sam całkowity czas dyfuzji
            // co przy liczeniu w każdej iteracji, za ułamek kosztu rozsypywania.
            for _ in 0..p.slope_every {
                diffusion_step(ctx.pool, &mut g, &mut buf, p);
                repose_step(ctx.pool, &mut g, &mut buf, p);
            }
            for (k, c) in s.stack.iter().enumerate() {
                s.h[k] = g[*c as usize];
            }
        }

        // ── Przechwytywanie rzeczne ──────────────────────────────────────────────────
        //
        // Bez tego kroku odbiorniki D8 są te, które wyznaczył szum **przed** erozją: doliny
        // pogłębiają się, ale rzeka nie może przeciąć niskiego działu wodnego i zabrać
        // sąsiedniej zlewni. W rzeczywistości przechwycenia kształtują większość dużych
        // dorzeczy, więc sieć bez nich jest siecią szumu, tylko głębiej wciętą.
        //
        // Przetrasowanie to pełne P4 + P5, więc idzie kilka razy na przebieg, a nie w każdej
        // iteracji — i to koszt, a nie algorytm, ustala ile razy (budżet generacji 16 km,
        // M1 §10; zmierzone w R2-WP19). Ostatnią iterację pomijamy: po niej nie ma już czego
        // erodować, a trasowanie i tak biegnie na końcu.
        if reroute_step > 0 && (iter + 1).is_multiple_of(reroute_step) && iter + 1 < iterations {
            rozsyp(ctx, &s, false);
            let przed = ujscia_komorek(ctx);
            crate::gen::flood::fill(ctx);
            crate::gen::flow::route(ctx);
            let outlets = &ctx.work.outlets;
            ctx.work.basin_captures += ctx
                .work
                .basin
                .as_slice()
                .iter()
                .zip(&przed)
                .filter(|(b, p)| outlets[**b as usize] != **p)
                .count() as u64;
            s = przygotuj(ctx, p);
        }
    }

    rozsyp(ctx, &s, true);
    normalize_relief(ctx);

    // Erozja potrafi wydrążyć nowe misy, a wszystko za nią zakłada teren bez zagłębień.
    //
    // To jest **jedyne** trasowanie przy `reroutes = 0`, czyli przy dzisiejszej kalibracji:
    // sieć rzeczna jest tą, którą wyznaczył szum przed erozją, tylko głębiej wciętą. Nie jest
    // to skrót bez pomiaru, tylko decyzja budżetowa z liczbami — M1 §5.7a i komentarz przy
    // `reroutes` w `data/geology/erosion.ron`.
    crate::gen::flood::fill(ctx);
    crate::gen::flow::route(ctx);
    crate::gen::commit_height(ctx);
}

/// Sprowadza ląd z powrotem do budżetu pionowego regionu.
///
/// Przez `iterations` kroków wypiętrzenie dokłada kilkanaście metrów na grzbietach, a erozja
/// zdejmuje je głównie w dolinach — netto krajobraz dryfuje w górę i najwyższe partie opierają
/// się o sufit świata. Samo zaciśnięcie ścięłoby im wierzchołki na płasko; skalowanie
/// liniowe zachowuje kształt, bo zakres pionowy świata to **budżet**, a nie wynik pomiaru.
///
/// Morze zostaje nietknięte: jego poziom jest odniesieniem dla wszystkiego innego (0 m),
/// więc przeskalowanie go przesunęłoby linię brzegową.
fn normalize_relief(ctx: &mut GenCtx) {
    use crate::gen::shape::{RegionShape, WORLD_MAX_M, WORLD_MIN_M};
    let shape = RegionShape::of(ctx.params.region);
    let target = (shape.base_m + shape.relief_m).min(WORLD_MAX_M);

    let mut max = 0.0f32;
    for h in ctx.work.height_m.as_slice() {
        if *h > max {
            max = *h;
        }
    }
    let scale = if max > target { target / max } else { 1.0 };

    for h in ctx.work.height_m.as_mut_slice() {
        if *h > 0.0 {
            *h *= scale;
        }
        *h = h.clamp(WORLD_MIN_M, WORLD_MAX_M);
    }
}

/// Krok stream-power w postaci niejawnej Brauna–Willetta dla `n = 1`:
///
/// `h_i' = (h_i + U·dt + C·h_r') / (1 + C)`, gdzie `C = K·dt·A^m / d`.
///
/// Odbiornik jest w tej samej zlewni i **wcześniej w stosie**, więc `h_r'` jest już policzone.
/// Stąd bierze się bezwarunkowa stabilność: wynik zawsze leży między `h_i + U·dt` a `h_r'`,
/// niezależnie od tego, jak duże jest `C`.
fn stream_power_step(
    pool: &magnat_jobs::JobPool,
    h: &mut [f32],
    u: &[f32],
    coef: &[f32],
    recv: &[u32],
    ranges: &[(u32, u32)],
) {
    // Rozłączne podzakresy stosu, jeden na zlewnię. `split_at_mut` w pętli daje wektor
    // wzajemnie niezachodzących wycinków — kompilator potwierdza rozłączność, której
    // w przestrzeni siatki musielibyśmy pilnować dyscypliną.
    let mut reszta = h;
    let mut kawalki: Vec<(usize, &mut [f32])> = Vec::with_capacity(ranges.len());
    let mut zjedzone = 0usize;
    for (s, e) in ranges {
        let (s, e) = (*s as usize, *e as usize);
        debug_assert_eq!(s, zjedzone, "zakresy zlewni nie są ciągłe");
        let (kawalek, dalej) = reszta.split_at_mut(e - s);
        kawalki.push((s, kawalek));
        reszta = dalej;
        zjedzone = e;
    }

    magnat_jobs::for_each_chunk_mut(pool, &mut kawalki, |_, (start, hh)| {
        let start = *start;
        for local in 0..hh.len() {
            let k = start + local;
            let r = recv[k];
            if r == u32::MAX {
                continue; // ujście: poziom odniesienia trzymany na miejscu
            }
            let hr = hh[(r as usize) - start];
            let c = coef[k];
            let podniesiona = hh[local] + u[k];
            let nowa = (podniesiona + c * hr) / (1.0 + c);
            // **Erozja nie ma prawa dosypać materiału.** Schemat niejawny jest średnią ważoną
            // między komórką a jej odbiornikiem, więc gdy komórka leży NIŻEJ od odbiornika,
            // ciągnie ją w górę. Na zwykłym zboczu to się nie zdarza, ale w misie wypełnionej
            // w P4 owszem: tam odbiornik wyznaczono na powierzchni wypełnionej, a rzeczywisty
            // teren jest pod nią. Bez tego ograniczenia stream-power zasypywałby jeziora skałą
            // — objaw wyglądał jak „erozja podnosi doliny o 3 m".
            hh[local] = if nowa < podniesiona {
                nowa
            } else {
                podniesiona
            };
        }
    });
}

/// Dyfuzja zboczowa: `∂h/∂t = D·∇²h`, schemat jawny na pięciopunktowym laplasjanie.
/// Podwójne buforowanie, bo aktualizacja w miejscu uzależniłaby wynik od kolejności komórek.
fn diffusion_step(
    pool: &magnat_jobs::JobPool,
    g: &mut Grid2<f32>,
    buf: &mut [f32],
    p: ErosionParams,
) {
    let dim = g.dim();
    let dx = WORK_CELL_M as f32;
    let k = p.hillslope_diffusion * p.dt_years / (dx * dx);
    let src = g.as_slice();

    let mut rows: Vec<&mut [f32]> = buf.chunks_mut(dim).collect();
    magnat_jobs::for_each_chunk_mut(pool, &mut rows, |y, row| {
        for (x, out) in row.iter_mut().enumerate() {
            let h = src[y * dim + x];
            let xm = x.saturating_sub(1);
            let xp = (x + 1).min(dim - 1);
            let ym = y.saturating_sub(1);
            let yp = (y + 1).min(dim - 1);
            let lap = src[y * dim + xm] + src[y * dim + xp] + src[ym * dim + x] + src[yp * dim + x]
                - 4.0 * h;
            *out = h + k * lap;
        }
    });
    g.as_mut_slice().copy_from_slice(buf);
}

/// Erozja termiczna: nachylenie powyżej kąta usypu jest częściowo zdejmowane.
/// Bez tego zbocza wyerodowane przez stream-power stają pionowo i wyglądają jak błąd renderu.
fn repose_step(pool: &magnat_jobs::JobPool, g: &mut Grid2<f32>, buf: &mut [f32], p: ErosionParams) {
    let dim = g.dim();
    // `det_math::tan`, nie `f64::tan`: kąt usypu kształtuje teren, a teren jest stanem
    // trwałym (00 §K-6). Liczone raz na przebieg, więc koszt jest bez znaczenia.
    let tan_repose =
        magnat_core::det_math::tan(f64::from(p.repose_angle_deg) * std::f64::consts::PI / 180.0)
            as f32;
    let dist: [f32; 8] = std::array::from_fn(|k| neighbor_dist_m(k, f64::from(WORK_CELL_M)) as f32);
    let src = g.as_slice();

    let mut rows: Vec<&mut [f32]> = buf.chunks_mut(dim).collect();
    magnat_jobs::for_each_chunk_mut(pool, &mut rows, |y, row| {
        for (x, out) in row.iter_mut().enumerate() {
            let h = src[y * dim + x];
            let mut nadmiar = 0.0f32;
            for (k, (dx, dy)) in NEIGHBORS_8.iter().enumerate() {
                let (nx, ny) = (x as i64 + i64::from(*dx), y as i64 + i64::from(*dy));
                if nx < 0 || ny < 0 || nx >= dim as i64 || ny >= dim as i64 {
                    continue;
                }
                let dh = h - src[ny as usize * dim + nx as usize];
                let limit = tan_repose * dist[k];
                if dh > limit && dh - limit > nadmiar {
                    nadmiar = dh - limit;
                }
            }
            *out = h - nadmiar * p.repose_relaxation;
        }
    });
    g.as_mut_slice().copy_from_slice(buf);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{Region, WorldGenParams, WorldSize};
    use magnat_jobs::JobPool;

    fn swiat(region: Region, seed: u64, threads: usize) -> GenCtx<'static> {
        let pool: &'static JobPool = Box::leak(Box::new(JobPool::new(threads)));
        let params = WorldGenParams {
            seed,
            size: WorldSize::Small4km,
            region,
            ..WorldGenParams::default()
        };
        let mut ctx = GenCtx::new(params, pool);
        crate::gen::landmask::run(&mut ctx);
        crate::gen::height::run(&mut ctx);
        crate::gen::uplift::run(&mut ctx);
        crate::gen::flood::run(&mut ctx);
        crate::gen::flow::run(&mut ctx);
        ctx
    }

    #[test]
    fn parametry_z_pliku_sa_stabilne_i_zgodne_z_domyslnymi() {
        let p = ErosionParams::load(&crate::data_path("geology/erosion.ron"))
            .expect("data/geology/erosion.ron");
        p.validate().unwrap();
        let d = ErosionParams::default();
        assert_eq!(
            p.dt_years, d.dt_years,
            "plik i wartości awaryjne się rozjechały"
        );
        assert_eq!(p.hillslope_diffusion, d.hillslope_diffusion);
        assert_eq!(p.repose_angle_deg, d.repose_angle_deg);
        // Rozjazd akurat tutaj zmieniłby sieć rzeczną każdego świata z każdego ziarna,
        // a przy `unwrap_or_default()` w `run` nikt by się nie dowiedział, że do niego doszło.
        assert_eq!(p.reroutes, d.reroutes, "liczba przetrasowań się rozjechała");
    }

    #[test]
    fn niestabilna_dyfuzja_jest_odrzucana_przy_ladowaniu() {
        let p = ErosionParams {
            hillslope_diffusion: 1.0,
            ..ErosionParams::default()
        };
        assert!(p.validate().is_err(), "warunek stabilności nie zadziałał");
    }

    /// Wcięcie koryta: o ile komórka leży niżej od średniej swoich ośmiu sąsiadów.
    /// Miara **względna**, i to jest cały jej sens — bezwzględna zmiana wysokości w dolinie
    /// mierzy głównie to, jak daleko dana komórka leży od poziomu odniesienia, a nie to,
    /// czy erozja cokolwiek wyrzeźbiła.
    fn wciecie(g: &crate::grid::Grid2<f32>, x: usize, y: usize) -> f64 {
        let mut suma = 0.0f64;
        for (dx, dy) in NEIGHBORS_8 {
            suma += f64::from(*g.get_clamped(x as i64 + i64::from(dx), y as i64 + i64::from(dy)));
        }
        f64::from(*g.get(x, y)) - suma / 8.0
    }

    #[test]
    fn erozja_wcina_koryta_i_nie_wyjezdza_poza_zakres() {
        let mut ctx = swiat(Region::Mountain, 17, 2);
        let przed = ctx.work.height_m.clone();
        let acc = ctx.work.flow_acc.as_slice().to_vec();
        erode(&mut ctx, ErosionParams::default());

        // 0,1 km² zlewni to już ciek, a nie zbocze — i wciąż nie ujście, więc jest co rzeźbić.
        let prog = 100_000.0f32;
        let dim = ctx.dim();
        let (mut p_sum, mut po_sum, mut ile) = (0.0f64, 0.0f64, 0usize);
        for y in 1..dim - 1 {
            for x in 1..dim - 1 {
                if acc[y * dim + x] <= prog {
                    continue;
                }
                p_sum += wciecie(&przed, x, y);
                po_sum += wciecie(&ctx.work.height_m, x, y);
                ile += 1;
            }
        }
        assert!(ile > 500, "za mało komórek ciekowych: {ile}");
        let (p_sr, po_sr) = (p_sum / ile as f64, po_sum / ile as f64);
        assert!(
            po_sr < p_sr - 0.05,
            "koryta nie pogłębiły się: wcięcie {p_sr:.3} m → {po_sr:.3} m"
        );
        assert!(po_sr < 0.0, "koryta leżą wyżej niż otoczenie: {po_sr:.3} m");

        for h in ctx.work.height_m.as_slice() {
            assert!(
                (crate::gen::shape::WORLD_MIN_M..=crate::gen::shape::WORLD_MAX_M).contains(h),
                "wysokość {h} poza zakresem świata"
            );
        }
    }

    #[test]
    fn erozja_nie_zalezy_od_liczby_watkow() {
        // To jest test ryzyka R1 — najważniejszy test tego przebiegu.
        let mut a = swiat(Region::Mountain, 31, 1);
        let mut b = swiat(Region::Mountain, 31, 8);
        erode(&mut a, ErosionParams::default());
        erode(&mut b, ErosionParams::default());
        assert_eq!(
            a.work.height_m.as_slice(),
            b.work.height_m.as_slice(),
            "erozja dała inny teren przy 1 i 8 wątkach"
        );
    }

    /// R2-WP19. Przechwycenie rzeczne: rzeka przecina niski dział wodny i zabiera sąsiednią
    /// zlewnię, więc komórka spływa po erozji do **innego ujścia** niż przed nią.
    ///
    /// Przed naprawą ten test padał zawsze i dla każdej liczby przetrasowań, bo topologia
    /// odwodnienia była ustalana raz, przed pętlą: przechwycenie nie miało jak zajść.
    #[test]
    fn erozja_przechwytuje_zlewnie_dopiero_po_przetrasowaniu() {
        let bez = ErosionParams {
            reroutes: 0,
            ..ErosionParams::default()
        };
        let mut a = swiat(Region::Mountain, 17, 2);
        erode(&mut a, bez);
        assert_eq!(
            a.work.basin_captures, 0,
            "bez przetrasowania nie ma jak zmienić zlewni"
        );

        let mut b = swiat(Region::Mountain, 17, 2);
        erode(&mut b, ErosionParams { reroutes: 1, ..bez });
        assert!(
            b.work.basin_captures > 0,
            "erozja nie przechwyciła ani jednej komórki"
        );
        assert_ne!(
            a.work.height_m.as_slice(),
            b.work.height_m.as_slice(),
            "przechwycenie policzone, ale teren wyszedł ten sam — pomiar mierzy nie to, co trzeba"
        );

        // Ryzyko R1 na ścieżce przetrasowania: między iteracjami wchodzi tu P4 (szeregowe)
        // i P5 (po wierszach), więc `erozja_nie_zalezy_od_liczby_watkow` ich nie pokrywa —
        // ono chodzi po kalibracji z danych, czyli po `reroutes = 0`.
        let mut c = swiat(Region::Mountain, 17, 8);
        erode(&mut c, ErosionParams { reroutes: 1, ..bez });
        assert_eq!(
            b.work.height_m.as_slice(),
            c.work.height_m.as_slice(),
            "przetrasowanie dało inny teren przy 2 i 8 wątkach"
        );
        assert_eq!(b.work.basin_captures, c.work.basin_captures);
    }

    #[test]
    fn po_erozji_nadal_nie_ma_lokalnych_minimow() {
        let mut ctx = swiat(Region::Lowland, 23, 2);
        erode(&mut ctx, ErosionParams::default());
        let dim = ctx.dim();
        let g = &ctx.work.filled_m;
        let mut minima = 0;
        for y in 1..dim - 1 {
            for x in 1..dim - 1 {
                let h = *g.get(x, y);
                if !NEIGHBORS_8.iter().any(|(dx, dy)| {
                    *g.get_clamped(x as i64 + i64::from(*dx), y as i64 + i64::from(*dy)) < h
                }) {
                    minima += 1;
                }
            }
        }
        assert_eq!(minima, 0, "po erozji zostało {minima} zagłębień");
    }
}
