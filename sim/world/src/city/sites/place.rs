//! Etap 7 — obsada parcel: normatywy, wypełniacze, zakłady produkcyjne, materializacja
//! encji i przepięcie stanowisk pracy (M2 §5.8, WP13).
//!
//! Wydzielone z `city/sites.rs` w R-WP5 bez zmiany zachowania.

use super::*;

// ── Etap 7 ───────────────────────────────────────────────────────────────────────────

/// Decyzja obsady dla jednej parceli, zanim cokolwiek powstanie.
#[derive(Clone, Copy)]
struct Przydzial {
    parcel: u32,
    archetype: SiteArchetypeId,
    /// Zakłady jednego szablonu w jednym klastrze tworzą jedną firmę.
    firma_grupa: u32,
    capacity_scale: u16,
}

/// Kandydat na zakład: zabudowana (albo zabudowywalna) parcela w strefie niemieszkalnej.
struct Kand {
    idx: u32,
    zone: ZoneKind,
    area: u32,
    block: u32,
    pos: Vec2,
    built: bool,
    zajeta: bool,
}

/// Cała obsada Etapu 7 plus domknięcie łańcuchów.
///
/// Mutuje `parcels` (właściciel, status, budynek) i `buildings` (nowe bryły na zieleni
/// i w wydobyciu, przepięcie stanowisk pracy na zakłady — korekta F1).
#[allow(clippy::too_many_arguments)]
pub fn populate(
    bi: &BuildInput,
    cat: &Catalog,
    sc: &SiteCatalog,
    epoch_key: &str,
    geom: &mut PolyArena,
    parcels: &mut ParcelSet,
    buildings: &mut BuildingSet,
    q: &EditQueue,
) -> SiteSet {
    let plan = bi.plan;
    let districts = bi.districts;
    let mut kandydaci = zbierz_kandydatow(parcels, geom);
    let mut przydzialy: Vec<Przydzial> = Vec::new();
    let mut grupa = 0u32;
    let mut rep = SiteReport::default();

    let pop_pole = pole_ludnosci(plan, bi.blocks, parcels, buildings);

    // ── 1. Normatywy ludnościowe: handel, usługi publiczne, zieleń ──────────────────
    for (i, a) in sc.archetypes.iter().enumerate() {
        let Some(per_pop) = a.spec.per_pop else {
            continue;
        };
        if !a.pasuje_do_epoki(epoch_key) {
            continue;
        }
        let cel = (plan.target_pop / per_pop.max(1)).max(1);
        rep.norm_target += cel;
        let wybrane = rozmiesc_normatyw(&mut kandydaci, a, cel, &pop_pole, plan);
        rep.norm_placed += wybrane.len() as u32;
        for p in wybrane {
            przydzialy.push(Przydzial {
                parcel: p,
                archetype: SiteArchetypeId(i as u16),
                firma_grupa: {
                    grupa += 1;
                    grupa
                },
                capacity_scale: SCALE_BASE,
            });
        }
    }

    // ── 2. Wypełniacze stref nieprodukcyjnych ──────────────────────────────────────
    // Przed bilansem, bo ich `consumes` jest częścią popytu (def(3) z §5.8).
    const NIEPRODUKCYJNE: [ZoneKind; 4] = [
        ZoneKind::Commercial,
        ZoneKind::Office,
        ZoneKind::Institutional,
        ZoneKind::Logistics,
    ];
    wypelnij(
        plan,
        &mut kandydaci,
        sc,
        epoch_key,
        &NIEPRODUKCYJNE,
        &mut przydzialy,
        &mut grupa,
    );

    // ── 3. Zakłady produkcyjne: liczba z popytu, miejsce z klastrów ────────────────
    let popyt = popyt_bazowy(plan, cat, sc, &przydzialy);
    let potrzeby = zapotrzebowanie_zakladow(cat, sc, &popyt, epoch_key);
    posadz_produkcje(
        bi,
        sc,
        cat,
        &potrzeby,
        &mut kandydaci,
        &mut przydzialy,
        &mut grupa,
        &mut rep,
        epoch_key,
    );

    // ── 4. Wypełniacze stref przemysłowych i reszty ────────────────────────────────
    const RESZTA: [ZoneKind; 4] = [
        ZoneKind::IndustryLight,
        ZoneKind::IndustryHeavy,
        ZoneKind::Extraction,
        ZoneKind::Agriculture,
    ];
    wypelnij(
        plan,
        &mut kandydaci,
        sc,
        epoch_key,
        &RESZTA,
        &mut przydzialy,
        &mut grupa,
    );

    // ── 5. Materializacja: brakujące bryły, potem encje ────────────────────────────
    przydzialy.sort_by_key(|p| p.parcel);
    let zlecenia: Vec<(u32, GrammarId)> = przydzialy
        .iter()
        .filter(|p| parcels.parcels[p.parcel as usize].building.is_none())
        .filter_map(|p| sc.get(p.archetype).grammar.map(|g| (p.parcel, g)))
        .collect();
    let wynik = build::build_for_sites(bi, geom, parcels, buildings, q, &zlecenia);
    rep.buildings_added = wynik.iter().filter(|x| x.is_some()).count() as u32;
    rep.buildings_failed = wynik.iter().filter(|x| x.is_none()).count() as u32;

    // Przydział, któremu bryła się nie zmieściła, **przenosi się na działkę zabudowaną**,
    // zamiast zniknąć. Bez tego jedyne ujęcie wody w mieście turystycznym trafiało na
    // parcelę zieleni, gramatyka się na niej nie mieściła i całe miasto zostawało bez
    // wody — a raport mówił tylko „domknięcie nie przeszło", trzy kroki dalej.
    for p in &mut przydzialy {
        if parcels.parcels[p.parcel as usize].building.is_some() {
            continue;
        }
        let a = sc.get(p.archetype);
        let Some(i) = kandydaci.iter().position(|c| {
            !c.zajeta && c.built && niemieszkalna(c.zone) && c.area >= a.spec.min_parcel_m2
        }) else {
            continue;
        };
        kandydaci[i].zajeta = true;
        p.parcel = kandydaci[i].idx;
        rep.relaxed_zone += 1;
    }

    let mut set = utworz_zaklady(plan, sc, districts, parcels, buildings, &przydzialy, rep);

    // ── 6. Domknięcie łańcuchów i bilans przepustowości (WP14) ─────────────────────
    // Popyt liczony **jeszcze raz**, na komplecie przydziałów: wypełniacze z kroku 4
    // i zakłady z kroku 3 też coś zużywają, a ten z kroku 3 był policzony przed nimi.
    let popyt_koncowy = popyt_bazowy(plan, cat, sc, &przydzialy);
    set.closure = supply_closure_check(cat, &mut set.sites, &popyt_koncowy, bi.roads);

    // ── 7. Stanowiska pracy: przepięcie na zakłady i korekta liczby (korekta F1) ───
    rebind_workplaces(bi, sc, districts, parcels, buildings, &mut set, epoch_key);

    set.report.parcels_without_site = parcels
        .parcels
        .iter()
        .enumerate()
        .filter(|(_, p)| p.building.is_some() && niemieszkalna(p.zone))
        .filter(|(i, _)| {
            let b = parcels.parcels[*i].building.expect("sprawdzone wyżej");
            set.site_of_building(b).is_none()
        })
        .count() as u32;
    set
}

fn zbierz_kandydatow(parcels: &ParcelSet, geom: &PolyArena) -> Vec<Kand> {
    parcels
        .parcels
        .iter()
        .enumerate()
        .filter(|(_, p)| niemieszkalna(p.zone))
        .map(|(i, p)| Kand {
            idx: i as u32,
            zone: p.zone,
            area: p.area_m2,
            block: p.block.0,
            pos: poly::centroid(geom.get(p.poly)),
            built: p.building.is_some(),
            zajeta: false,
        })
        .collect()
}

/// Siatka ludności: mieszkania × 2,4, zsumowane po komórkach 256 m.
///
/// Potrzebna do rozmieszczenia normatywów „maksymalizującego pokrycie ludności" (§5.8).
/// Liczona z **mieszkań**, nie z gęstości strefy — po Etapie 6 jest z czego, a gęstość
/// strefy mówi o zamiarze planisty, nie o tym, ile mieszkań faktycznie stanęło.
struct PoleLudnosci {
    dim: usize,
    cell_m: f32,
    v: Vec<f32>,
}

const POP_CELL_M: f32 = 256.0;

impl PoleLudnosci {
    fn suma_w_promieniu(&self, p: Vec2, r_m: f32) -> f32 {
        let k = (r_m / self.cell_m).ceil() as i32;
        let (cx, cy) = ((p.x / self.cell_m) as i32, (p.y / self.cell_m) as i32);
        let mut s = 0.0;
        for dy in -k..=k {
            for dx in -k..=k {
                if dx * dx + dy * dy > k * k {
                    continue;
                }
                let (x, y) = (cx + dx, cy + dy);
                if x < 0 || y < 0 || x as usize >= self.dim || y as usize >= self.dim {
                    continue;
                }
                s += self.v[y as usize * self.dim + x as usize];
            }
        }
        s
    }
}

fn pole_ludnosci(
    plan: &CityPlan,
    _blocks: &BlockSet,
    parcels: &ParcelSet,
    buildings: &BuildingSet,
) -> PoleLudnosci {
    let dim = ((plan.map_size_m() as f32 / POP_CELL_M).ceil() as usize).max(1);
    let mut v = vec![0.0f32; dim * dim];
    for u in buildings.units.iter().filter(|u| u.kind.is_dwelling()) {
        let b = &buildings.buildings[u.building.0.index() as usize];
        let p = &parcels.parcels[b.parcel.0.index() as usize];
        let _ = p;
        let c = Vec2::new(
            (b.aabb.min.x + b.aabb.max.x) * 0.5,
            (b.aabb.min.y + b.aabb.max.y) * 0.5,
        );
        let (x, y) = ((c.x / POP_CELL_M) as i32, (c.y / POP_CELL_M) as i32);
        if x < 0 || y < 0 || x as usize >= dim || y as usize >= dim {
            continue;
        }
        v[y as usize * dim + x as usize] += 2.4;
    }
    PoleLudnosci {
        dim,
        cell_m: POP_CELL_M,
        v,
    }
}

/// Zachłanne rozmieszczenie normatywu: bierz działkę o największym pokryciu ludności,
/// potem tłum pokrycie w jej sąsiedztwie i powtarzaj. To jest k-center po ludności
/// z §5.8, wyrażony tak, żeby nie wymagał macierzy odległości 4 000 × 4 000.
fn rozmiesc_normatyw(
    kand: &mut [Kand],
    a: &Archetype,
    cel: u32,
    pop: &PoleLudnosci,
    plan: &CityPlan,
) -> Vec<u32> {
    let promien = (plan.urban_radius_m() / (cel as f32).sqrt()).clamp(150.0, 2500.0);
    let mut punkty: Vec<(usize, f32)> = kand
        .iter()
        .enumerate()
        .filter(|(_, k)| pasuje_dzialka(k, a))
        .map(|(i, k)| (i, pop.suma_w_promieniu(k.pos, promien)))
        .collect();
    let mut out = Vec::new();
    for _ in 0..cel {
        // Remisy po indeksie parceli — nigdy po kolejności iteracji (00 §3.2).
        let Some(&(best, _)) = punkty
            .iter()
            .filter(|(i, _)| !kand[*i].zajeta)
            .max_by(|a, b| a.1.total_cmp(&b.1).then(kand[b.0].idx.cmp(&kand[a.0].idx)))
        else {
            break;
        };
        let p = kand[best].pos;
        kand[best].zajeta = true;
        out.push(kand[best].idx);
        for (i, s) in &mut punkty {
            if (kand[*i].pos - p).length() < promien {
                *s *= 0.2;
            }
        }
    }
    out.sort_unstable();
    out
}

fn pasuje_dzialka(k: &Kand, a: &Archetype) -> bool {
    !k.zajeta
        && a.zones.contains(&k.zone)
        && k.area >= a.spec.min_parcel_m2
        // Działka bez budynku nadaje się tylko wtedy, gdy archetyp umie go postawić.
        && (k.built || a.grammar.is_some())
}

/// Wypełniacze: każda pozostała zabudowana działka we wskazanych strefach dostaje
/// archetyp losowany wagą. Kryterium WP13 mówi „każda zabudowana parcela niemieszkalna
/// ma `SiteSeed`" — to jest to miejsce, w którym ta obietnica się spełnia.
fn wypelnij(
    plan: &CityPlan,
    kand: &mut [Kand],
    sc: &SiteCatalog,
    epoch_key: &str,
    strefy: &[ZoneKind],
    out: &mut Vec<Przydzial>,
    grupa: &mut u32,
) {
    for k in kand.iter_mut() {
        if k.zajeta || !strefy.contains(&k.zone) {
            continue;
        }
        let mut r = rng(plan.seed, StreamId::SitePlacement, k.idx, Tick(0));
        let kandydaci: Vec<usize> = sc
            .archetypes
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                a.spec.weight > 0 && a.pasuje_do_epoki(epoch_key) && pasuje_dzialka(k, a)
            })
            .map(|(j, _)| j)
            .collect();
        let suma: u32 = kandydaci
            .iter()
            .map(|j| u32::from(sc.archetypes[*j].spec.weight))
            .sum();
        if suma == 0 {
            // Działka mniejsza od najmniejszego archetypu strefy. Kryterium WP13 mówi
            // „każda zabudowana parcela niemieszkalna ma `SiteSeed`" bez wyjątku dla
            // małych, więc bierzemy najmniejszy pasujący archetyp i ignorujemy metraż —
            // zakład na 60 m² jest mniej nieprawdziwy niż budynek bez właściciela.
            let Some(j) = sc
                .archetypes
                .iter()
                .enumerate()
                .filter(|(_, a)| {
                    a.spec.weight > 0
                        && a.pasuje_do_epoki(epoch_key)
                        && a.zones.contains(&k.zone)
                        && (k.built || a.grammar.is_some())
                })
                .min_by_key(|(j, a)| (a.spec.min_parcel_m2, *j))
                .map(|(j, _)| j)
            else {
                continue;
            };
            k.zajeta = true;
            *grupa += 1;
            out.push(Przydzial {
                parcel: k.idx,
                archetype: SiteArchetypeId(j as u16),
                firma_grupa: *grupa,
                capacity_scale: SCALE_BASE,
            });
            continue;
        }
        let mut los = r.next_u32() % suma;
        for j in kandydaci {
            let w = u32::from(sc.archetypes[j].spec.weight);
            if los < w {
                k.zajeta = true;
                *grupa += 1;
                out.push(Przydzial {
                    parcel: k.idx,
                    archetype: SiteArchetypeId(j as u16),
                    firma_grupa: *grupa,
                    capacity_scale: SCALE_BASE,
                });
                break;
            }
            los -= w;
        }
    }
}

// ── Bilans popytu ────────────────────────────────────────────────────────────────────

/// Popyt niewynikający z produkcji: koszyk mieszkańców + materiały eksploatacyjne
/// archetypów już postawionych (def(3) z §5.8). Gramy na dobę.
fn popyt_bazowy(
    plan: &CityPlan,
    cat: &Catalog,
    sc: &SiteCatalog,
    przydzialy: &[Przydzial],
) -> Vec<i64> {
    let mut d = vec![0i64; cat.goods.len()];
    for (g, na_osobe) in &cat.basket {
        d[g.0 as usize] += na_osobe.saturating_mul(i64::from(plan.target_pop));
    }
    for p in przydzialy {
        for (g, m) in &sc.get(p.archetype).consumes {
            d[g.0 as usize] += *m;
        }
    }
    d
}

/// Ile zakładów każdego archetypu produkcyjnego trzeba, żeby pokryć popyt.
///
/// Propagacja wstecz po hipergrafie receptur: `required` startuje z popytu bazowego,
/// a każda runda dokłada wejścia potrzebne do jego pokrycia. Rund jest **dwanaście**,
/// nie „do zbieżności": łańcuch w katalogu M2 ma głębokość co najwyżej pięciu ogniw,
/// a stały budżet rund jest warunkiem determinizmu (ryzyko R3).
///
/// Receptura wielowyjściowa (rafineria → trzy paliwa) skalowana jest **raz**, po
/// najbardziej wymagającym wyjściu — inaczej ropa liczyłaby się trzykrotnie.
fn zapotrzebowanie_zakladow(
    cat: &Catalog,
    sc: &SiteCatalog,
    popyt: &[i64],
    epoch_key: &str,
) -> Vec<(SiteArchetypeId, RecipeId, f64)> {
    // Receptura krajowa dla towaru: pierwsza po `RecipeId`, której archetyp jest dostępny.
    let mut recepta_dla: Vec<Option<(RecipeId, SiteArchetypeId)>> = vec![None; cat.goods.len()];
    for (ai, a) in sc.archetypes.iter().enumerate() {
        if !a.pasuje_do_epoki(epoch_key) {
            continue;
        }
        for r in &a.recipes {
            for o in &cat.recipe(*r).outputs {
                let slot = &mut recepta_dla[o.good.0 as usize];
                if slot.is_none_or(|(stary, _)| r.0 < stary.0) {
                    *slot = Some((*r, SiteArchetypeId(ai as u16)));
                }
            }
        }
    }

    let mut required = popyt.to_vec();
    let mut skala: Vec<(RecipeId, SiteArchetypeId, f64)> = Vec::new();
    for _ in 0..12 {
        skala.clear();
        // Skala każdej używanej receptury — po najbardziej wymagającym wyjściu.
        for (gi, slot) in recepta_dla.iter().enumerate() {
            let Some((r, a)) = *slot else { continue };
            if required[gi] <= 0 {
                continue;
            }
            let y = cat.recipe(r).daily_yield(GoodId(gi as u16));
            if y <= 0 {
                continue;
            }
            let s = required[gi] as f64 / y as f64;
            match skala.iter_mut().find(|(x, _, _)| *x == r) {
                Some((_, _, v)) => *v = v.max(s),
                None => skala.push((r, a, s)),
            }
        }
        skala.sort_by_key(|(r, _, _)| r.0);
        let mut next = popyt.to_vec();
        for (r, _, s) in &skala {
            let rec = cat.recipe(*r);
            for w in &rec.inputs {
                let na_dobe = rec.daily_input(w.good) as f64 * s;
                next[w.good.0 as usize] =
                    next[w.good.0 as usize].saturating_add(na_dobe.round() as i64);
            }
        }
        required = next;
    }
    skala.iter().map(|(r, a, s)| (*a, *r, *s)).collect()
}

/// Klastry przemysłowe: spójne grupy kwartałów o strefie przemysłowej albo logistycznej.
/// Ta sama definicja co w `rail.rs` i `districts.rs` — bocznica, dzielnica i firma mają
/// mówić o tym samym klastrze, a nie o trzech różnych.
fn klastry(blocks: &BlockSet, zones: &ZoneResult, segments: usize) -> Vec<Vec<u32>> {
    let przemysl = |z: ZoneKind| {
        matches!(
            z,
            ZoneKind::IndustryHeavy | ZoneKind::IndustryLight | ZoneKind::Logistics
        )
    };
    let n = blocks.blocks.len();
    let (start, items) = crate::city::blocks::block_adjacency(blocks, segments);
    let mut seen = vec![false; n];
    let mut out: Vec<Vec<u32>> = Vec::new();
    for s in 0..n {
        if seen[s] || !przemysl(zones.zone[s]) {
            continue;
        }
        let mut stos = vec![s];
        seen[s] = true;
        let mut grupa = Vec::new();
        while let Some(v) = stos.pop() {
            grupa.push(v as u32);
            for k in start[v]..start[v + 1] {
                let j = items[k as usize] as usize;
                if !seen[j] && przemysl(zones.zone[j]) {
                    seen[j] = true;
                    stos.push(j);
                }
            }
        }
        grupa.sort_unstable();
        out.push(grupa);
    }
    // Od największego: magistrala kolejowa powstaje tam samo (rail.rs), więc łańcuch
    // trafia do klastra, który faktycznie ma bocznicę.
    out.sort_by(|a, b| b.len().cmp(&a.len()).then(a[0].cmp(&b[0])));
    out
}

#[allow(clippy::too_many_arguments)]
fn posadz_produkcje(
    bi: &BuildInput,
    sc: &SiteCatalog,
    cat: &Catalog,
    potrzeby: &[(SiteArchetypeId, RecipeId, f64)],
    kand: &mut [Kand],
    out: &mut Vec<Przydzial>,
    grupa: &mut u32,
    rep: &mut SiteReport,
    epoch_key: &str,
) {
    let plan = bi.plan;
    // Ile zakładów każdego archetypu i z jaką skalą.
    let mut zostalo: Vec<(SiteArchetypeId, u32, u16)> = Vec::new();
    for (a, r, s) in potrzeby {
        if *s <= 0.0 {
            continue;
        }
        // Zakład jest niepodzielny, więc liczba zakładów to sufit skali, a `capacity_scale`
        // rozkłada resztę po równo. Poniżej `SCALE_MIN` zakład przestaje mieć sens:
        // jeśli wszystko, co produkuje, da się sprowadzić — miasto to sprowadza,
        // a jeśli nie (woda, beton) — stoi mimo wszystko, w najmniejszej skali.
        let n = s.ceil().clamp(1.0, 4096.0) as u32;
        let dokladna = s / f64::from(n) * f64::from(SCALE_BASE);
        if dokladna < f64::from(SCALE_MIN)
            && cat
                .recipe(*r)
                .outputs
                .iter()
                .all(|o| cat.good(o.good).has_external_price())
        {
            continue;
        }
        let skala =
            (dokladna.round() as i64).clamp(i64::from(SCALE_MIN), i64::from(SCALE_MAX)) as u16;
        zostalo.push((*a, n, skala));
    }
    // Najpierw zakłady produkujące to, czego **nie da się sprowadzić** (woda, beton).
    // Reszta może przegrać wyścig o działkę — miasto to wtedy kupi; wodociąg nie ma
    // takiej alternatywy, a przy zwykłej kolejności alfabetycznej `waterworks` trafiał
    // na koniec i zostawał bez miejsca w mieście o wąskim pasie przemysłowym.
    let musi_powstac = |a: SiteArchetypeId| {
        sc.get(a)
            .recipes
            .iter()
            .flat_map(|r| cat.recipe(*r).outputs.iter())
            .any(|o| !cat.good(o.good).has_external_price())
    };
    zostalo.sort_by_key(|(a, _, _)| (!musi_powstac(*a), a.0));

    let profil = plan.profile.key();
    let klastry = klastry(bi.blocks, bi.zones, bi.roads.segments.len());

    // ── 3a. Po klastrach, wg szablonów łańcuchów ───────────────────────────────────
    for (ki, k) in klastry.iter().enumerate() {
        let szablony: Vec<&ChainTemplate> = sc
            .chains
            .iter()
            .filter(|c| c.profiles.is_empty() || c.profiles.iter().any(|p| p == profil))
            .filter(|c| c.epochs.is_empty() || c.epochs.iter().any(|e| e == epoch_key))
            .collect();
        let suma: u32 = szablony.iter().map(|c| u32::from(c.weight)).sum();
        if suma == 0 {
            break;
        }
        let mut r = rng(plan.seed, StreamId::FirmSeed, ki as u32, Tick(0));
        let mut los = r.next_u32() % suma;
        let mut wybrany = szablony[0];
        for c in &szablony {
            let w = u32::from(c.weight);
            if los < w {
                wybrany = c;
                break;
            }
            los -= w;
        }
        *grupa += 1;
        let moja_grupa = *grupa;
        let mut cos_stanelo = false;
        for klucz in &wybrany.archetypes {
            let Some(aid) = sc.id_of(klucz) else { continue };
            let Some(slot) = zostalo.iter().position(|(a, n, _)| *a == aid && *n > 0) else {
                continue;
            };
            let skala = zostalo[slot].2;
            let a = sc.get(aid);
            let Some(i) = znajdz(kand, a, &|c: &Kand| {
                k.contains(&c.block) && zloze_ok(bi, c, a)
            }) else {
                continue;
            };
            kand[i].zajeta = true;
            zostalo[slot].1 -= 1;
            cos_stanelo = true;
            rep.from_chain += 1;
            out.push(Przydzial {
                parcel: kand[i].idx,
                archetype: aid,
                firma_grupa: moja_grupa,
                capacity_scale: skala,
            });
        }
        if !cos_stanelo {
            *grupa -= 1;
        }
    }

    // ── 3b. Reszta: gdziekolwiek się mieści, każdy zakład własną firmą ─────────────
    #[allow(
        clippy::needless_range_loop,
        reason = "pętla mutuje `zostalo[slot].1` i jednocześnie czyta `kand`; iterator po `zostalo` zablokowałby drugie pożyczenie"
    )]
    for slot in 0..zostalo.len() {
        let (aid, _, skala) = zostalo[slot];
        let a = sc.get(aid);
        let musi = musi_powstac(aid);
        while zostalo[slot].1 > 0 {
            // Zakład, który **musi** powstać, siada wyłącznie na działce już zabudowanej:
            // pusta działka daje szansę, że bryła się nie zmieści, a wtedy nie ma wody.
            let znaleziona = if musi {
                kand.iter()
                    .position(|c| c.built && pasuje_dzialka(c, a) && zloze_ok(bi, c, a))
            } else {
                znajdz(kand, a, &|c: &Kand| zloze_ok(bi, c, a))
            };
            let Some(i) = znaleziona else {
                break;
            };
            kand[i].zajeta = true;
            zostalo[slot].1 -= 1;
            *grupa += 1;
            out.push(Przydzial {
                parcel: kand[i].idx,
                archetype: aid,
                firma_grupa: *grupa,
                capacity_scale: skala,
            });
        }
        // ── 3c. Rozluźnienie strefy — zamiast przestrefowania kwartału (korekta I-3) ──
        // Dotyczy **wyłącznie** zakładów produkujących towar, którego nie da się
        // sprowadzić: wodociąg i betoniarnia muszą stanąć, bo importu wody nie ma.
        // Reszta może zostać nieposadzona — miasto to wtedy kupi.
        while musi && zostalo[slot].1 > 0 {
            let Some(i) = kand.iter().position(|c| {
                !c.zajeta
                    && c.built
                    && niemieszkalna(c.zone)
                    && c.area >= a.spec.min_parcel_m2
                    && zloze_ok(bi, c, a)
            }) else {
                break;
            };
            kand[i].zajeta = true;
            zostalo[slot].1 -= 1;
            rep.relaxed_zone += 1;
            *grupa += 1;
            out.push(Przydzial {
                parcel: kand[i].idx,
                archetype: aid,
                firma_grupa: *grupa,
                capacity_scale: skala,
            });
        }
        if zostalo[slot].1 > 0 {
            rep.unplaced.push((a.key().to_string(), zostalo[slot].1));
        }
    }
}

/// Pierwsza pasująca działka, **z pierwszeństwem dla już zabudowanych**.
///
/// Kolejność ma znaczenie praktyczne: na działce z budynkiem zakład powstanie na pewno,
/// a na pustej dopiero, jeśli bryła się zmieści. Bez tego pierwszeństwa jedyne ujęcie
/// wody w mieście trafiało na parcelę, na której gramatyka nie miała jak stanąć, i całe
/// miasto zostawało bez wody — objaw widoczny dopiero w bilansie, o trzy kroki dalej.
fn znajdz(kand: &[Kand], a: &Archetype, extra: &dyn Fn(&Kand) -> bool) -> Option<usize> {
    kand.iter()
        .position(|c| c.built && pasuje_dzialka(c, a) && extra(c))
        .or_else(|| kand.iter().position(|c| pasuje_dzialka(c, a) && extra(c)))
}

/// Czy pod działką leży złoże, którego archetyp wymaga (M1 `deposit_at`, K-13).
fn zloze_ok(bi: &BuildInput, k: &Kand, a: &Archetype) -> bool {
    let Some(want) = a.spec.needs_deposit else {
        return true;
    };
    match bi.terrain.deposit_at(k.pos.x as i32, k.pos.y as i32) {
        Some(id) => bi.terrain.deposit(id).resource == want,
        None => false,
    }
}

// ── Materializacja encji ─────────────────────────────────────────────────────────────

fn utworz_zaklady(
    plan: &CityPlan,
    sc: &SiteCatalog,
    districts: &DistrictSet,
    parcels: &mut ParcelSet,
    buildings: &BuildingSet,
    przydzialy: &[Przydzial],
    mut rep: SiteReport,
) -> SiteSet {
    let mut firms: Vec<FirmSeed> = Vec::new();
    let mut sites: Vec<SiteSeed> = Vec::new();
    let mut by_building = vec![None; buildings.buildings.len()];
    // Grupa → indeks firmy. Wektor par, nie mapa: iterujemy po nim w kolejności wstawiania.
    let mut grupy: Vec<(u32, u32)> = Vec::new();

    for p in przydzialy {
        let parcel = &parcels.parcels[p.parcel as usize];
        let Some(bid) = parcel.building else { continue };
        let a = sc.get(p.archetype);
        let fidx = match grupy.iter().find(|(g, _)| *g == p.firma_grupa) {
            Some((_, i)) => *i,
            None => {
                let i = firms.len() as u32;
                let dzielnica = districts
                    .districts
                    .get(parcel.district.0 as usize)
                    .map_or("Miasto", |d| d.name.as_str());
                let mut r = rng(plan.seed, StreamId::FirmSeed, 1_000_000 + i, Tick(0));
                let czlon = sc.names.czlon(a.sector(), r.next_u32());
                let mut name = format!("{czlon} „{dzielnica}”");
                // Ta sama nazwa dwa razy w mieście jest błędem widocznym natychmiast
                // (ryzyko R10 dotyczy dzielnic, ale firma z tym samym szyldem po drugiej
                // stronie ulicy wygląda tak samo źle).
                let mut n = 2;
                while firms.iter().any(|f| f.name == name) {
                    name = format!("{czlon} „{dzielnica}” {}", rzymska(n));
                    n += 1;
                }
                firms.push(FirmSeed {
                    name,
                    sector: a.sector(),
                    sites: SmallVec::new(),
                });
                grupy.push((p.firma_grupa, i));
                i
            }
        };
        let b = &buildings.buildings[bid.0.index() as usize];
        let sid = site_id(sites.len() as u32);
        firms[fidx as usize].sites.push(sid);
        by_building[bid.0.index() as usize] = Some(sites.len() as u32);
        sites.push(SiteSeed {
            firm: firm_id(fidx),
            building: bid,
            units: b.units.clone(),
            archetype: p.archetype,
            recipes: a.recipes.clone(),
            capacity_scale: p.capacity_scale,
            workplaces: 0..0,
            parcel: crate::city::parcels::parcel_id(p.parcel),
        });
        rep.by_sector[a.sector() as usize] += 1;

        let parcel = &mut parcels.parcels[p.parcel as usize];
        parcel.status = ParcelStatus::Built;
        parcel.owner = if a.sector().is_municipal() {
            ParcelOwner::City
        } else {
            ParcelOwner::Firm(firm_id(fidx))
        };
    }

    rep.firms = firms.len() as u32;
    rep.sites = sites.len() as u32;
    SiteSet {
        firms,
        sites,
        by_building,
        report: rep,
        closure: ClosureReport::default(),
    }
}

/// Liczebnik porządkowy do odróżniania firm o tej samej nazwie. Do dziesiątej wystarczy
/// rzymski, dalej arabski — „Fabryka «Wola» XXIV" wygląda gorzej niż „… 24".
fn rzymska(n: u32) -> String {
    const R: [&str; 9] = ["II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X"];
    if (2..=10).contains(&n) {
        R[(n - 2) as usize].to_string()
    } else {
        n.to_string()
    }
}

// ── Stanowiska pracy (korekta F1) ────────────────────────────────────────────────────

/// Przepina stanowiska pracy na zakłady i poprawia ich liczbę tam, gdzie archetyp mówi
/// co innego niż przelicznik roli.
///
/// M2d wypełnił `BuildingSet.workplaces` rolami z `data/jobs/roles.ron`, bo archetypy
/// powstają dopiero tutaj (korekta E5). Tablica jest płaska, a `Unit.workplaces` to
/// zakres w niej — więc zmiana liczby stanowisk w jednym lokalu przesuwa wszystkie
/// następne. Przebudowa całej tablicy raz jest tańsza i prostsza od wstawiania w środek.
fn rebind_workplaces(
    bi: &BuildInput,
    sc: &SiteCatalog,
    districts: &DistrictSet,
    parcels: &ParcelSet,
    buildings: &mut BuildingSet,
    set: &mut SiteSet,
    epoch_key: &str,
) {
    let _ = epoch_key;
    // Kalibracja drugiej połowy T10 (korekta I-6). Przelicznik „m² na stanowisko" jest
    // tą samą dźwignią co zagęszczenie mieszkań: liczba etatów wychodząca z Etapu 6
    // zależy od tego, ile powierzchni usługowej dał teren, a to zmienia się z ziarnem.
    // Najpierw liczymy stanowiska przy przeliczniku z danych, potem korygujemy go raz,
    // globalnie, żeby suma trafiła w środek okna T10.
    let cel = bi.plan.target_pop as f32 * build::ETATOW_NA_MIESZKANCA * 1.03;
    // Cztery przebiegi o **stałym budżecie**, nie do zbieżności. Jedno przeliczenie
    // nie wystarcza, bo liczba stanowisk w lokalu jest zaokrąglana w górę do jedynki:
    // przy małych lokalach — a takie ma wieś — suma nie jest liniowa względem
    // przelicznika i pierwsze podstawienie trafia obok o kilka procent.
    let mut wp_scale = 1.0f32;
    for _ in 0..4 {
        let ile = licz_stanowiska(bi, sc, set, buildings, wp_scale);
        if ile <= 0.0 || cel <= 0.0 {
            break;
        }
        let nowa = (wp_scale * ile / cel).clamp(
            build::capacity::ETATY_SCALE_MIN,
            build::capacity::ETATY_SCALE_MAX,
        );
        if (nowa - wp_scale).abs() < 0.005 {
            break;
        }
        wp_scale = nowa;
    }

    let mut nowe: Vec<Workplace> = Vec::with_capacity(buildings.workplaces.len());
    let mut zakres_zakladu: Vec<Range<u32>> = vec![0..0; set.sites.len()];

    for bi_idx in 0..buildings.buildings.len() {
        let (units, parcel) = {
            let b = &buildings.buildings[bi_idx];
            (b.units.clone(), b.parcel)
        };
        let site_idx = set.by_building[bi_idx];
        let p = &parcels.parcels[parcel.0.index() as usize];
        let block = &bi.blocks.blocks[p.block.0 as usize];
        let epoka = bi
            .zones
            .rings
            .get(usize::from(block.epoch_ring))
            .map_or("contemporary", |e| e.key.as_str());
        let epoch_mult = bi.jobs.epoch_mult(epoka);
        let tier = districts
            .districts
            .get(p.district.0 as usize)
            .map_or(2, |d| d.income_tier);
        let arch = site_idx.map(|i| sc.get(set.sites[i as usize].archetype));
        let od_budynku = nowe.len() as u32;

        for ui in units.start..units.end {
            let (kind, area) = {
                let u = &buildings.units[ui as usize];
                (u.kind, u.area_m2)
            };
            let wp_od = nowe.len() as u32;
            if let Some((role_id, role)) = bi.jobs.role_for(kind) {
                let na_stanowisko = match arch {
                    Some(a) if a.spec.m2_per_workplace > 0 => a.spec.m2_per_workplace,
                    _ => role.m2_per_workplace.max(1),
                };
                let ile = (f32::from(area) / (f32::from(na_stanowisko.max(1)) * wp_scale))
                    .round()
                    .max(1.0) as u32;
                let mult = epoch_mult * arch.map_or(1.0, |a| a.spec.wage_mult);
                let band = build::model::wage_band(role, mult, tier);
                for _ in 0..ile {
                    nowe.push(Workplace {
                        unit: UnitIdx(ui),
                        site: site_idx.map(site_id),
                        role: role_id,
                        shift: role.shift.into(),
                        wage_band: band,
                        occupant: None,
                    });
                }
            }
            let u = &mut buildings.units[ui as usize];
            u.workplaces = wp_od..nowe.len() as u32;
            // Lokal niemieszkalny w budynku zakładu należy do tego zakładu; mieszkanie
            // zostaje `Vacant` i czeka na gospodarstwo domowe z M3.
            if let Some(i) = site_idx {
                if !kind.is_dwelling() {
                    u.occupant = UnitOccupant::Site(site_id(i));
                }
            }
        }
        if let Some(i) = site_idx {
            zakres_zakladu[i as usize] = od_budynku..nowe.len() as u32;
        }
    }

    for (s, r) in set.sites.iter_mut().zip(zakres_zakladu) {
        s.workplaces = r;
    }
    buildings.report.workplaces = nowe.len() as u32;
    buildings.workplaces = nowe;
}

/// Liczba stanowisk, jaka wyszłaby przy zadanym mnożniku przelicznika. Przebieg suchy —
/// potrzebny raz, żeby wyznaczyć mnożnik, i nic poza liczbą nie zmienia.
fn licz_stanowiska(
    bi: &BuildInput,
    sc: &SiteCatalog,
    set: &SiteSet,
    buildings: &BuildingSet,
    scale: f32,
) -> f32 {
    let mut n = 0f32;
    for (i, b) in buildings.buildings.iter().enumerate() {
        let arch = set.by_building[i].map(|s| sc.get(set.sites[s as usize].archetype));
        for u in &buildings.units[b.units.start as usize..b.units.end as usize] {
            let Some((_, role)) = bi.jobs.role_for(u.kind) else {
                continue;
            };
            let na_stanowisko = match arch {
                Some(a) if a.spec.m2_per_workplace > 0 => a.spec.m2_per_workplace,
                _ => role.m2_per_workplace.max(1),
            };
            n += (f32::from(u.area_m2) / (f32::from(na_stanowisko.max(1)) * scale))
                .round()
                .max(1.0);
        }
    }
    n
}
