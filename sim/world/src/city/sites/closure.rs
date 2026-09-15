//! Domknięcie łańcuchów produktowych: osiągalność, naprawa, bilans przepustowości
//! (M2 §5.8, WP14). **Właścicielem docelowym jest M6d** — koniec dostawcy zewnętrznego.
//!
//! Wydzielone z `city/sites.rs` w R-WP5 bez zmiany zachowania.

use super::*;

/// Wynik domknięcia łańcuchów produktowych (§5.8, WP14).
#[derive(Clone, Default, PartialEq, Debug)]
pub struct ClosureReport {
    /// Towary, do których nie prowadzi żadna ścieżka — warunek akceptacji: pusta lista.
    pub missing: Vec<String>,
    /// Towary sprowadzane z zewnątrz i dobowy wolumen w kilogramach.
    pub imported: Vec<(String, i64)>,
    /// Zakłady dostawione przez naprawę (KROK 4b).
    pub added_sites: u32,
    /// Archetypy, którym trzeba było rozluźnić wymaganie strefy (KROK 4c po korekcie I-3).
    pub relaxed_zone: u32,
    /// `supply/demand` dla każdego towaru z popytem — rosnąco po `GoodId`.
    pub ratios: Vec<(String, f32)>,
    /// Najgorszy stosunek i towar, którego dotyczy.
    pub worst: (String, f32),
    /// Nadwyżki produktów ubocznych — patrz `wiodace_wyjscia` (korekta I-4). To nie jest
    /// błąd bilansu, tylko towar, który z miasta wyjeżdża; wypisany, żeby nie znikał.
    pub byproduct_surplus: Vec<(String, f32)>,
    /// Pozycje, przy których generacja się nie domknęła (KROK 4d).
    pub errors: Vec<String>,
}

impl ClosureReport {
    /// Warunek akceptacji z §5.8 i test T11.
    ///
    /// Liczony **tą samą regułą co `errors`**: nadwyżka produktu ubocznego nie jest
    /// błędem bilansu (korekta I-4), więc nie może wywracać akceptacji. Dwie reguły
    /// w dwóch miejscach dawały raport bez ani jednego błędu i domknięcie odrzucone.
    #[must_use]
    pub fn accepted(&self) -> bool {
        self.missing.is_empty() && self.errors.is_empty()
    }
}

// ── Domknięcie łańcuchów produktowych (WP14, §5.8) ───────────────────────────────────

/// Dobowa przepustowość towarowa bramy, w gramach.
///
/// `GateKind::capacity()` jest w `Qty` (milisztuki) i opisuje przepustowość **ruchu**,
/// nie masy — przeliczanie jednego na drugie byłoby myleniem jednostek. Import liczy się
/// więc w tonach na dobę, a właścicielem docelowego, dynamicznego limitu jest M6.
const fn brama_t_na_dobe(k: GateKind) -> i64 {
    match k {
        GateKind::Highway => 4_000,
        GateKind::RailFreight => 20_000,
        GateKind::Port => 40_000,
        GateKind::RailPassenger | GateKind::Airport => 0,
    }
}

/// Domknięcie osiągalności + naprawa + bilans przepustowości (§5.8, KROKI 1–5).
///
/// Zwraca raport; `sites` są mutowane, bo KROK 5 zmienia `capacity_scale`.
pub fn supply_closure_check(
    cat: &Catalog,
    sites: &mut [SiteSeed],
    popyt_bazowy: &[i64],
    roads: &crate::city::road::RoadNetwork,
) -> ClosureReport {
    let mut rep = ClosureReport::default();
    let n = cat.goods.len();

    // Budżet importu: suma przepustowości bram towarowych, w gramach na dobę.
    let mut budzet_importu: i64 = roads
        .gates
        .iter()
        .map(|g| brama_t_na_dobe(g.kind).saturating_mul(1_000_000))
        .sum();
    let importowalny: Vec<bool> = cat
        .goods
        .iter()
        .map(|g| {
            g.has_external_price()
                && g.gates()
                    .iter()
                    .any(|k| roads.gates.iter().any(|x| x.kind == *k))
        })
        .collect();

    // KROK 1–2: osiągalność na recepturach **zainstancjonowanych**, nie na całym katalogu.
    let mut zainstalowane: Vec<RecipeId> = sites.iter().flat_map(|s| s.recipes.clone()).collect();
    zainstalowane.sort_by_key(|r| r.0);
    zainstalowane.dedup();

    let mut import = vec![0i64; n];
    let seed: Vec<GoodId> = (0..n as u16)
        .map(GoodId)
        .filter(|g| importowalny[g.0 as usize])
        .collect();
    let osiagalne = cat.reachable(&seed, &zainstalowane);

    // KROK 3: czego brakuje. `demanded` wg def(3) z §5.8: koszyk, wejścia receptur
    // zainstancjonowanych i materiały eksploatacyjne archetypów (te siedzą już
    // w `popyt_bazowy`).
    let mut demanded = vec![false; n];
    for (g, d) in demanded.iter_mut().enumerate() {
        if popyt_bazowy[g] > 0 {
            *d = true;
        }
    }
    for r in &zainstalowane {
        for (g, _) in &cat.recipe(*r).inputs {
            demanded[g.0 as usize] = true;
        }
    }

    // KROK 4: naprawa. 4a — import, jeśli towar ma cenę zewnętrzną i zgodną bramę.
    // 4b (dostawienie zakładu) jest w tym generatorze **wykonane wcześniej**: liczba
    // zakładów wynika z popytu (korekta I-2), więc brak zakładu znaczy „nie było popytu",
    // a nie „zabrakło miejsca". 4c (przestrefowanie kwartału) nie jest wykonalne po
    // podziale na parcele i po zabudowie — jego rolę przejmuje wielostrefowość archetypów
    // w `data/buildings/` (korekta I-3).
    for g in 0..n {
        if demanded[g] && !osiagalne[g] {
            if importowalny[g] {
                import[g] = import[g].max(1);
            } else {
                rep.missing.push(cat.goods[g].key.clone());
            }
        }
    }

    // KROK 5: bilans przepustowości. Sześć przebiegów o **stałym budżecie** — nie pętla
    // do zbieżności (ryzyko R3): skala zakładów wyszła już z popytu, więc te przebiegi
    // domykają zaokrąglenia, a nie szukają rozwiązania.
    //
    // Import liczy się **od zera w każdym przebiegu**, a nie narastająco. Kumulowanie go
    // było błędem: przywóz uzupełniony w przebiegu drugim zostawał w mocy po tym, jak
    // przebieg trzeci zmniejszył popyt, i zboże wychodziło z bilansu z nadwyżką 4,7×,
    // choć nikt go nie zamawiał.
    let mut import = vec![0i64; n];
    for _ in 0..6 {
        let (supply, demand) = bilans(cat, sites, popyt_bazowy, &pusty(n));
        let wiodacy = wiodace_wyjscia(cat, &zainstalowane, &demand);
        let mut zmiana = false;
        for g in 0..n {
            if !demanded[g] || demand[g] <= 0 {
                continue;
            }
            let r = supply[g] as f64 / demand[g] as f64;
            let za_malo = r < RATIO_MIN && !importowalny[g];
            let za_duzo = r > RATIO_MAX && wiodacy[g];
            if !za_malo && !za_duzo {
                continue;
            }
            let cel = if za_malo { 0.98 } else { 1.10 };
            let mnoznik = cel / r.max(1e-9);
            for s in sites.iter_mut() {
                let dotyczy = s.recipes.iter().any(|x| {
                    let rec = cat.recipe(*x);
                    rec.outputs.iter().any(|(o, _)| o.0 as usize == g)
                        // Przy nadmiarze ruszamy **tylko** zakłady, dla których ten towar
                        // jest wyjściem wiodącym — inaczej ścięcie nadmiaru skóry
                        // zabrałoby miastu mięso.
                        && (za_malo || wiodace_dla(cat, *x, &demand) == Some(GoodId(g as u16)))
                });
                if !dotyczy {
                    continue;
                }
                let nowa = ((f64::from(s.capacity_scale) * mnoznik).round() as i64)
                    .clamp(i64::from(SCALE_MIN), i64::from(SCALE_MAX))
                    as u16;
                if nowa != s.capacity_scale {
                    s.capacity_scale = nowa;
                    zmiana = true;
                }
            }
            // Kiedy nadmiaru nie da się już zdjąć skalą, bo wszystkie zakłady stoją
            // na dolnej granicy, **gasimy** nadmiarowe: zostają budynkiem i etatami,
            // tracą receptury. To jest dźwignia, której §5.8 nie przewidział (korekta I-2)
            // — bez niej ostatni niepodzielny zakład trzyma bilans poza widełkami.
            if za_duzo && !zmiana {
                zmiana |= zgas_nadmiarowe(
                    cat,
                    sites,
                    GoodId(g as u16),
                    supply[g],
                    demand[g],
                    importowalny[g],
                );
            }
        }
        if !zmiana {
            break;
        }
    }

    // Czego po skalowaniu wciąż brakuje, a da się sprowadzić — dowozi się, w kolejności
    // `GoodId` i do wyczerpania przepustowości bram. Zawsze od zera, więc przywóz jest
    // funkcją stanu końcowego, a nie historii przebiegów.
    {
        let (supply, demand) = bilans(cat, sites, popyt_bazowy, &pusty(n));
        for g in 0..n {
            if !demanded[g] || !importowalny[g] || demand[g] <= 0 {
                continue;
            }
            let brak = demand[g] - supply[g];
            if brak <= 0 {
                continue;
            }
            let ile = brak.min(budzet_importu.max(0));
            if ile > 0 {
                import[g] = ile;
                budzet_importu -= ile;
            }
        }
    }

    let (supply, demand) = bilans(cat, sites, popyt_bazowy, &import);
    let wiodacy = wiodace_wyjscia(cat, &zainstalowane, &demand);
    let stosunek = |g: usize| supply[g] as f64 / demand[g] as f64;
    rep.ratios = (0..n)
        .filter(|g| demanded[*g] && demand[*g] > 0)
        .map(|g| (cat.goods[g].key.clone(), stosunek(g) as f32))
        .collect();
    rep.worst = rep
        .ratios
        .iter()
        .map(|(k, r)| (k.clone(), *r))
        .min_by(|a, b| {
            let da = (f64::from(a.1) - 1.0).abs();
            let db = (f64::from(b.1) - 1.0).abs();
            db.total_cmp(&da).then(a.0.cmp(&b.0))
        })
        .unwrap_or_default();
    rep.imported = (0..n)
        .filter(|g| import[*g] > 0)
        .map(|g| (cat.goods[g].key.clone(), import[g] / 1000))
        .collect();
    rep.byproduct_surplus = (0..n)
        .filter(|g| demanded[*g] && demand[*g] > 0 && !wiodacy[*g] && stosunek(*g) > RATIO_MAX)
        .map(|g| (cat.goods[g].key.clone(), stosunek(g) as f32))
        .collect();
    for g in 0..n {
        if !demanded[g] || demand[g] <= 0 {
            continue;
        }
        let r = stosunek(g);
        if r < RATIO_MIN || (r > RATIO_MAX && wiodacy[g]) {
            rep.errors.push(format!(
                "podaż/popyt dla `{}` wynosi {r:.2}, poza [{RATIO_MIN}; {RATIO_MAX}]",
                cat.goods[g].key
            ));
        }
    }
    rep
}

/// Zdejmuje receptury z nadmiarowych zakładów produkujących `g`, od najwyższego indeksu,
/// zostawiając zawsze co najmniej jeden. Zakład bez receptur nadal ma budynek, etaty
/// i firmę — przestaje tylko być ogniwem łańcucha.
fn zgas_nadmiarowe(
    cat: &Catalog,
    sites: &mut [SiteSeed],
    g: GoodId,
    supply: i64,
    demand: i64,
    importowalny: bool,
) -> bool {
    let czynne: Vec<usize> = (0..sites.len())
        .filter(|i| {
            sites[*i]
                .recipes
                .iter()
                .any(|x| cat.recipe(*x).outputs.iter().any(|(o, _)| *o == g))
        })
        .collect();
    // Ostatni zakład wolno zgasić tylko wtedy, kiedy towar da się sprowadzić. Inaczej
    // miasto zostałoby bez wody albo bez betonu, a tego nie naprawi żaden przywóz.
    let minimum = usize::from(!importowalny);
    if czynne.len() <= minimum {
        return false;
    }
    let cel = (demand as f64 * RATIO_MAX) as i64;
    let na_zaklad = supply / czynne.len() as i64;
    if na_zaklad <= 0 {
        return false;
    }
    let zostawic = ((cel / na_zaklad).max(minimum as i64) as usize).min(czynne.len());
    if zostawic >= czynne.len() {
        return false;
    }
    for i in czynne.into_iter().skip(zostawic) {
        sites[i].recipes.clear();
    }
    true
}

/// Wektor zer o długości katalogu — bilans „bez importu".
fn pusty(n: usize) -> Vec<i64> {
    vec![0; n]
}

/// Wyjście, które **wyznacza skalę** receptury przy danym popycie: to, dla którego
/// `popyt / wydajność` jest największe. Pozostałe wyjścia są produktem ubocznym.
fn wiodace_dla(cat: &Catalog, r: RecipeId, demand: &[i64]) -> Option<GoodId> {
    let rec = cat.recipe(r);
    rec.outputs
        .iter()
        .map(|(g, _)| {
            let y = rec.daily_yield(*g).max(1);
            (*g, demand[g.0 as usize] as f64 / y as f64)
        })
        .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0 .0.cmp(&a.0 .0)))
        .map(|(g, _)| g)
}

/// Maska „ten towar jest dla którejś receptury wyjściem wiodącym".
///
/// **Korekta I-4 wobec §5.8.** Górna granica `ratio ≤ 1,30` z testu T11 jest nieosiągalna
/// dla produktu ubocznego: rzeźnia skalowana mięsem wyprodukuje tyle skóry, ile wyjdzie
/// z tuszy, i żadne skalowanie tego nie zmieni bez zepsucia bilansu mięsa. Algorytm
/// z §5.8 zna jedną dźwignię — `capacity_scale` całej receptury — więc nie ma jak
/// rozdzielić wyjść sprzężonych. Nadwyżka produktu ubocznego **wychodzi z miasta**
/// (odbiera ją dostawca zewnętrzny) i jest w raporcie wypisana osobno, zamiast udawać
/// błąd bilansu. Dolna granica 0,85 obowiązuje **bez wyjątku**: niedobór jest zawsze błędem.
fn wiodace_wyjscia(cat: &Catalog, zainstalowane: &[RecipeId], demand: &[i64]) -> Vec<bool> {
    let mut out = vec![false; cat.goods.len()];
    for r in zainstalowane {
        if let Some(g) = wiodace_dla(cat, *r, demand) {
            out[g.0 as usize] = true;
        }
    }
    out
}

/// Dobowa podaż i popyt każdego towaru, w gramach.
fn bilans(
    cat: &Catalog,
    sites: &[SiteSeed],
    popyt_bazowy: &[i64],
    import: &[i64],
) -> (Vec<i64>, Vec<i64>) {
    let mut supply = import.to_vec();
    let mut demand = popyt_bazowy.to_vec();
    for s in sites {
        let k = i64::from(s.capacity_scale);
        for r in &s.recipes {
            let rec = cat.recipe(*r);
            for (g, _) in &rec.outputs {
                let v = rec.daily_yield(*g).saturating_mul(k) / i64::from(SCALE_BASE);
                supply[g.0 as usize] = supply[g.0 as usize].saturating_add(v);
            }
            // Wejścia rolnictwa **też** są popytem: obora naprawdę zjada paszę, choć
            // formalnie jest punktem wejścia łańcucha (`RecipeSource::Agriculture`).
            for (g, _) in &rec.inputs {
                let v = rec.daily_input(*g).saturating_mul(k) / i64::from(SCALE_BASE);
                demand[g.0 as usize] = demand[g.0 as usize].saturating_add(v);
            }
        }
    }
    (supply, demand)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::city::catalog::{Good, GoodUnit, Recipe, RecipeSource, RecipeSpec};

    /// Katalog dwutowarowy z jedną recepturą wytwórczą, której szarża domyka bilans masy:
    /// 500 g mąki → 400 g chleba + 100 g ubytku. Bez świata, bo `bilans` go nie potrzebuje.
    fn katalog() -> Catalog {
        let towar = |key: &str| Good {
            key: key.to_string(),
            unit: GoodUnit::Grams,
            density_g_per_l: 500,
            unit_mass_g: 0,
            external_base_price: None,
            import_via: Vec::new(),
        };
        let spec = RecipeSpec {
            key: "bakery".to_string(),
            source: RecipeSource::Manufacturing,
            inputs: vec![("flour".to_string(), 500)],
            outputs: vec![("bread".to_string(), 400)],
            process_loss_g: 100,
            duration_minutes: 60,
            labour: Vec::new(),
            machine_class: String::new(),
        };
        Catalog {
            goods: vec![towar("bread"), towar("flour")],
            recipes: vec![Recipe {
                spec,
                inputs: vec![(FLOUR, 500)],
                outputs: vec![(BREAD, 400)],
            }],
            basket: vec![(BREAD, 1)],
        }
    }

    const BREAD: GoodId = GoodId(0);
    const FLOUR: GoodId = GoodId(1);
    const PIEKARNIA: RecipeId = RecipeId(0);
    /// 400 g na szarżę × 1440 min / 60 min.
    const CHLEB_NA_DOBE: i64 = 9_600;
    /// 500 g na szarżę × 1440 min / 60 min.
    const MAKA_NA_DOBE: i64 = 12_000;
    /// 100 g na szarżę × 1440 min / 60 min — ubytek procesowy, ta sama arytmetyka.
    const STRATA_NA_DOBE: i64 = 2_400;

    fn zaklad(scale: u16) -> SiteSeed {
        SiteSeed {
            firm: firm_id(0),
            building: crate::city::build::building_id(0),
            units: 0..0,
            archetype: SiteArchetypeId(0),
            recipes: vec![PIEKARNIA],
            capacity_scale: scale,
            workplaces: 0..0,
            parcel: crate::city::parcels::parcel_id(0),
        }
    }

    /// Bilans masy: to, co zakłady wyprodukują, plus ubytek procesowy równa się temu,
    /// co zjedzą — co do grama, przy dowolnej skali. To jest własność, która pęka po
    /// cichu, gdy ktoś ruszy dzielenie przez `SCALE_BASE` albo pomyli wejścia z wyjściami.
    #[test]
    fn bilans_domyka_mase_przy_kazdej_skali() {
        let cat = katalog();
        for skala in [SCALE_MIN, 500, SCALE_BASE, SCALE_MAX] {
            let sites = [zaklad(skala)];
            let (supply, demand) = bilans(&cat, &sites, &pusty(2), &pusty(2));
            let k = i64::from(skala);
            let chleb = CHLEB_NA_DOBE * k / i64::from(SCALE_BASE);
            let maka = MAKA_NA_DOBE * k / i64::from(SCALE_BASE);
            let strata = STRATA_NA_DOBE * k / i64::from(SCALE_BASE);
            assert_eq!(
                (supply[0], supply[1]),
                (chleb, 0),
                "podaż przy skali {skala}"
            );
            assert_eq!(
                (demand[0], demand[1]),
                (0, maka),
                "popyt przy skali {skala}"
            );
            assert_eq!(
                chleb + strata,
                maka,
                "masa nie domyka się przy skali {skala}"
            );
        }
    }

    /// Popyt bazowy i import wchodzą do bilansu **obok** produkcji, nie zamiast niej:
    /// import dolicza się do podaży, koszyk mieszkańców do popytu.
    #[test]
    fn bilans_dolicza_import_i_popyt_bazowy() {
        let cat = katalog();
        let sites = [zaklad(SCALE_BASE)];
        let popyt = vec![7_000i64, 0];
        let import = vec![0i64, 5_000];
        let (supply, demand) = bilans(&cat, &sites, &popyt, &import);
        assert_eq!(supply, vec![CHLEB_NA_DOBE, 5_000]);
        assert_eq!(demand, vec![7_000, MAKA_NA_DOBE]);
    }

    /// Nadmiarowe zakłady gasną tak, że bilans **nadal się domyka**: podaż spada o całe
    /// zakłady, popyt na wejścia spada razem z nimi, a masa zostaje zamknięta. Bez tego
    /// gaszenie ścinałoby podaż, zostawiając zapotrzebowanie na mąkę dla zgaszonej piekarni.
    #[test]
    fn zgaszenie_nadmiaru_zostawia_bilans_domkniety() {
        let cat = katalog();
        let mut sites: Vec<SiteSeed> = (0..4).map(|_| zaklad(SCALE_BASE)).collect();
        let (supply, demand) = bilans(&cat, &sites, &[CHLEB_NA_DOBE, 0], &pusty(2));
        assert_eq!(supply[0], 4 * CHLEB_NA_DOBE);
        assert!(supply[0] as f64 / demand[0] as f64 > RATIO_MAX);

        // `importowalny = false`: chleba nie da się sprowadzić, więc ostatnia piekarnia zostaje.
        assert!(zgas_nadmiarowe(
            &cat, &mut sites, BREAD, supply[0], demand[0], false
        ));
        assert_eq!(
            sites.iter().filter(|s| !s.recipes.is_empty()).count(),
            1,
            "miała zostać jedna czynna piekarnia"
        );

        let (supply, demand) = bilans(&cat, &sites, &[CHLEB_NA_DOBE, 0], &pusty(2));
        let r = supply[0] as f64 / demand[0] as f64;
        assert!(
            (RATIO_MIN..=RATIO_MAX).contains(&r),
            "po zgaszeniu podaż/popyt chleba wynosi {r:.2}"
        );
        assert_eq!(
            supply[0] + STRATA_NA_DOBE,
            demand[1],
            "masa przestała się domykać po zgaszeniu"
        );
        // Zgaszony zakład traci receptury, ale zostaje budynkiem i firmą.
        assert_eq!(sites.len(), 4);
    }
}
