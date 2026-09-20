//! Kroki 6 i 7: dochód ↔ wartość lokalu, a potem mediana czasu dojazdu.
//!
//! Wydzielone z `population/mod.rs` w R-WP4 bez zmiany zachowania.

use super::*;

// ── kroki 6 i 7: mieszkanie i dojazd ────────────────────────────────────────────

/// Pracujący członek gospodarstwa w lustrze kroków 6–7.
struct Pracownik {
    citizen: Entity,
    identity: Identity,
    vitals: Vitals,
    work: PlaceRef,
    commute: u16,
}

/// Wynik kroków 6 i 7.
pub(super) struct Mieszkania {
    pub(super) rho_centi: i16,
    pub(super) hist: [u32; COMMUTE_BINS],
    pub(super) median: u16,
    /// Cel po przycięciu do tego, co geometria miasta w ogóle dopuszcza.
    pub(super) cel: u16,
    /// Zmierzony przedział osiągalnych median.
    pub(super) osiagalne: (u16, u16),
}

/// Kroki 6 i 7: dochód ↔ wartość lokalu, a potem histogram czasu dojazdu.
///
/// `ponytail:` krok 7 to zwykła zachłanna wymiana sterowana χ², nie wyżarzanie ani
/// transport optymalny (§5.9). Sufit znany: przy bardzo nierównomiernym rozkładzie
/// miejsc pracy zbieżność może nie zejść poniżej 5 % błędu mediany — wtedy podmieniamy
/// na wyżarzanie. Nie wcześniej.
#[allow(clippy::too_many_arguments)]
pub(super) fn dopasuj_mieszkania(
    world: &mut World,
    gospodarstwa: &[Entity],
    domy: &[HomeSlot],
    oracle: &TrafficOracle,
    seed: u64,
    cel_min: u16,
    proby: u32,
) -> Mieszkania {
    let n = gospodarstwa.len();
    if n < 2 {
        return Mieszkania {
            rho_centi: 0,
            hist: [0; COMMUTE_BINS],
            median: 0,
            cel: cel_min,
            osiagalne: (0, 0),
        };
    }

    // Lustro: dla każdego gospodarstwa jego pracujący i ich kopie `Identity`/`Vitals`.
    // Prędkość marszu zależy od wieku i zdrowia, więc widok musi być **czyjś**, a nie
    // przeciętny — a 200 tys. prób zamiany nie ma prawa chodzić po archetypach ECS.
    let mut zaloga: Vec<Vec<Pracownik>> = Vec::with_capacity(n);
    for hh in gospodarstwa {
        let hh_c = world.get::<Household>(*hh).copied().unwrap_or_default();
        let sklad = household::members_of(hh.index(), &hh_c, world.resource::<HouseholdOverflow>());
        let mut v = Vec::new();
        for m in sklad.iter() {
            let Some(c) = demography::citizen_by_index(world, *m) else {
                continue;
            };
            let Some(work) = world
                .get::<Employment>(c)
                .filter(|e| e.flags & Employment::FLAG_PUPIL == 0)
                .and_then(|e| site_place(e.site))
            else {
                continue;
            };
            v.push(Pracownik {
                citizen: c,
                identity: world.get::<Identity>(c).copied().unwrap_or_default(),
                vitals: world.get::<Vitals>(c).copied().unwrap_or_default(),
                work,
                commute: 0,
            });
        }
        zaloga.push(v);
    }

    // Krok 6: rangi dochodu wobec rang wartości lokalu, z szumem kopuły.
    let mut po_dochodzie: Vec<(i64, u32, usize)> = gospodarstwa
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let h = world.get::<Household>(*e).copied().unwrap_or_default();
            (h.income_monthly.get(), e.index(), i)
        })
        .collect();
    po_dochodzie.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    // `domy` są posortowane rosnąco po wartości, a gospodarstwo `i` zajmuje `domy[i]`,
    // więc ranga lokalu to po prostu jego indeks.
    let mut lokal_dla_rangi: Vec<usize> = (0..n).collect();
    for i in 0..n {
        let mut r = rng(seed, StreamId::PopGen, i as u32, Tick(6));
        // p(przesunięcia o k) ∝ exp(−k): rozkład geometryczny z p = ½, czytany
        // z ciągu jedynek na końcu słowa losowego.
        let k = r.next_u32().trailing_ones().min(6) as usize;
        if k == 0 {
            continue;
        }
        // §5.9 mówi o przesunięciu **o k decyli** — i tak też się je liczy.
        let skok = (k * n / 10).max(1);
        let j = if r.gen_bool_permille(500) {
            (i + skok).min(n - 1)
        } else {
            i.saturating_sub(skok)
        };
        lokal_dla_rangi.swap(i, j);
    }

    // `adres[i]` — lokal, który ma gospodarstwo o indeksie `i` w `gospodarstwa`.
    let mut adres: Vec<usize> = vec![0; n];
    for (ranga, lokal) in lokal_dla_rangi.iter().enumerate() {
        adres[po_dochodzie[ranga].2] = *lokal;
    }
    // Ranga wartości lokalu przypisana gospodarstwu o danej randze dochodu — wejście
    // korelacji Spearmana.
    let rho = spearman(&lokal_dla_rangi);

    // Krok 7: mediana czasu dojazdu.
    //
    // **Poprawka do §5.9** (korekta H-2): pętla poprawkowa steruje **medianą**, a nie
    // całym histogramem. Powód jest mierzalny: w M3 wszyscy chodzą pieszo (K-2 —
    // środki transportu dokłada M4), a praca jest przydzielana w granicach dzielnicy
    // (E-15), więc kształt rozkładu jest własnością geometrii miasta i zamiana mieszkań
    // go nie zmienia. Minimalizowanie χ² całego histogramu przesuwało przy tym medianę
    // **w złą stronę**, bo nadrabiało ogon rozkładu kosztem środka — a kryterium
    // `gen_commute_hist` mierzy właśnie medianę. χ² zostaje w raporcie jako diagnostyka
    // kształtu; progu na niego §7.4 nigdy nie podało i przy n ≈ 20 tys. żaden
    // osiągalny fit nie przeszedłby testu p > 0,05.
    //
    // Cel też nie jest przyjmowany na wiarę: mierzymy przedział median, w którym miasto
    // w ogóle da się ustawić, i przycinamy do niego żądanie. Raport mówi, że przyciął.
    let mut hist = [0u32; COMMUTE_BINS];
    let mut razem = 0u32;
    for (i, lista) in zaloga.iter_mut().enumerate() {
        let dom = miejsce_domu(domy[adres[i]]);
        dojazdy_gospodarstwa(oracle, dom, lista);
        for p in lista.iter() {
            hist[kubelek(p.commute)] += 1;
            razem += 1;
        }
    }
    let dolna = mediana_minutowa(&zaloga);

    // Górny koniec: adresy przemieszane **wewnątrz decyli wartości**, czyli tak
    // rozrzucone, jak krok 6 w ogóle pozwala.
    //
    // `ponytail:` mierzone na **próbce** gospodarstw, nie na wszystkich. Sufit nazwany:
    // przy próbce 4 tys. błąd mediany to ułamek minuty, a pełny przebieg kosztuje dwa
    // dodatkowe przejścia po całej populacji — czyli sekundy z budżetu 30 s. Gdyby
    // kiedyś zaczęło zależeć na dokładności tego przedziału co do minuty, próbkę
    // podnosi się jedną stałą.
    const PROBKA_GRANIC: usize = 4_000;
    let krok_probki = (n / PROBKA_GRANIC.max(1)).max(1);
    let wymieszane = przemieszaj_w_decylach(&adres, seed, n);
    let mut probka: Vec<Vec<Pracownik>> = Vec::new();
    for i in (0..n).step_by(krok_probki) {
        let dom = miejsce_domu(domy[wymieszane[i]]);
        let mut v: Vec<Pracownik> = zaloga[i]
            .iter()
            .map(|p| Pracownik {
                citizen: p.citizen,
                identity: p.identity,
                vitals: p.vitals,
                work: p.work,
                commute: 0,
            })
            .collect();
        dojazdy_gospodarstwa(oracle, dom, &mut v);
        probka.push(v);
    }
    let gorna = mediana_minutowa(&probka);

    let (lo, hi) = (dolna.min(gorna), dolna.max(gorna));
    let cel_min = cel_min.clamp(lo, hi);

    // Mediana jest równa `cel_min` dokładnie wtedy, gdy połowa dojazdów jest krótsza —
    // więc sterujemy licznikiem „krótszych niż cel", a nie samą medianą. Aktualizacja
    // jest wtedy O(1) na zmieniony dojazd, a nie przejściem po całym rozkładzie.
    let mut krotszych = policz_krotsze(&zaloga, cel_min);
    let mut odchylenie = blad_mediany(krotszych, razem);
    for i in 0..proby {
        if razem == 0 || odchylenie == 0 {
            break;
        }
        let mut r = rng(seed, StreamId::PopGen, i, Tick(7));
        let a = r.gen_range_u32(n as u32) as usize;
        let b = r.gen_range_u32(n as u32) as usize;
        if a == b || (zaloga[a].is_empty() && zaloga[b].is_empty()) {
            continue;
        }
        // Zamiana wyłącznie **wewnątrz tego samego decyla wartości** — inaczej krok 7
        // zjadałby dopasowanie z kroku 6 (§5.9).
        if adres[a] * 10 / n != adres[b] * 10 / n {
            continue;
        }

        let stare: Vec<u16> = zaloga[a]
            .iter()
            .chain(zaloga[b].iter())
            .map(|p| p.commute)
            .collect();
        adres.swap(a, b);
        for k in [a, b] {
            let dom = miejsce_domu(domy[adres[k]]);
            for p in &zaloga[k] {
                hist[kubelek(p.commute)] -= 1;
                krotszych -= u32::from(p.commute <= cel_min);
            }
            dojazdy_gospodarstwa(oracle, dom, &mut zaloga[k]);
            for p in &zaloga[k] {
                hist[kubelek(p.commute)] += 1;
                krotszych += u32::from(p.commute <= cel_min);
            }
        }
        let nowe = blad_mediany(krotszych, razem);
        if nowe <= odchylenie {
            odchylenie = nowe;
        } else {
            adres.swap(a, b);
            let mut it = stare.into_iter();
            for k in [a, b] {
                for p in zaloga[k].iter_mut() {
                    hist[kubelek(p.commute)] -= 1;
                    krotszych -= u32::from(p.commute <= cel_min);
                    p.commute = it.next().unwrap_or(p.commute);
                    hist[kubelek(p.commute)] += 1;
                    krotszych += u32::from(p.commute <= cel_min);
                }
            }
        }
    }

    // Wynik wraca do świata: adres gospodarstwa, adresy członków i czas dojazdu
    // odniesienia, który §7.4 porównuje z histogramem.
    for (i, hh) in gospodarstwa.iter().enumerate() {
        let d = domy[adres[i]];
        przeprowadz(world, *hh, d);
        for p in &zaloga[i] {
            if let Some(e) = world.get_mut::<Employment>(p.citizen) {
                e.commute_baseline_min = p.commute;
            }
        }
    }

    let mediana = mediana_minutowa(&zaloga);
    Mieszkania {
        rho_centi: rho,
        hist,
        median: mediana,
        cel: cel_min,
        osiagalne: (lo, hi),
    }
}

/// Ilu pracujących ma dojazd nie dłuższy niż `cel`.
fn policz_krotsze(zaloga: &[Vec<Pracownik>], cel: u16) -> u32 {
    zaloga.iter().flatten().filter(|p| p.commute <= cel).count() as u32
}

/// Odległość od stanu „mediana = cel". Mediana to pierwsza minuta, w której skumulowany
/// rozkład sięga połowy — więc trafia w `cel` dokładnie wtedy, gdy dojazdów **nie
/// dłuższych** niż `cel` jest dokładnie połowa.
fn blad_mediany(krotszych: u32, razem: u32) -> u32 {
    (i64::from(krotszych) * 2 - i64::from(razem)).unsigned_abs() as u32
}

/// Permutacja adresów przemieszana w obrębie decyli wartości — górny koniec rozrzutu
/// dojazdów, jaki krok 6 dopuszcza.
///
/// Pełne tasowanie wewnątrz każdego decyla, nie „losowa zamiana z losowym sąsiadem":
/// to drugie zostawia większość gospodarstw na miejscu, więc mierzyłoby nie górny
/// koniec przedziału, tylko punkt startowy.
fn przemieszaj_w_decylach(adres: &[usize], seed: u64, n: usize) -> Vec<usize> {
    let mut out = adres.to_vec();
    let mut grupy: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, a) in adres.iter().enumerate() {
        grupy.entry(a * 10 / n).or_default().push(i);
    }
    for (d, idx) in &grupy {
        let mut r = rng(seed, StreamId::PopGen, *d as u32, Tick(11));
        let mut wartosci: Vec<usize> = idx.iter().map(|i| adres[*i]).collect();
        r.shuffle(&mut wartosci);
        for (k, i) in idx.iter().enumerate() {
            out[*i] = wartosci[k];
        }
    }
    out
}

fn miejsce_domu(h: HomeSlot) -> PlaceRef {
    PlaceRef::Building(BuildingId(encja(h.building)))
}

fn widok<'a>(p: &'a Pracownik, puste: &'a Puste) -> CitizenView<'a> {
    CitizenView {
        id: CitizenId(p.citizen),
        identity: &p.identity,
        vitals: &p.vitals,
        needs: &puste.needs,
        personality: &puste.personality,
        residence: &puste.residence,
        today: 0,
        brands: Default::default(),
    }
}

/// Komponenty, których estymator pieszy nie czyta, a `CitizenView` ich wymaga.
/// Jedna instancja na przebieg zamiast trzech konstrukcji na wywołanie.
#[derive(Default)]
struct Puste {
    needs: Needs,
    personality: Personality,
    residence: Residence,
}

/// Dojazdy całego gospodarstwa. Każdy pracownik osobno, bo prędkość marszu zależy
/// od wieku i zdrowia — i bo cache par miejsc w estymatorze i tak zbiera powtórzenia.
fn dojazdy_gospodarstwa(oracle: &TrafficOracle, dom: PlaceRef, lista: &mut [Pracownik]) {
    let puste = Puste::default();
    for p in lista.iter_mut() {
        let tempo = magnat_traffic::speed_pct(&widok(p, &puste));
        p.commute = oracle.network_walk_minutes(dom, p.work, tempo);
    }
}

/// Zmiana adresu gospodarstwa razem z adresami wszystkich jego członków.
fn przeprowadz(world: &mut World, hh: Entity, home: HomeSlot) {
    let Some(h) = world.get_mut::<Household>(hh) else {
        return;
    };
    h.building = home.building;
    h.unit = home.unit;
    h.district = home.district;
    let hh_c = world.get::<Household>(hh).copied().unwrap_or_default();
    let sklad = household::members_of(hh.index(), &hh_c, world.resource::<HouseholdOverflow>());
    for m in sklad.iter() {
        let Some(c) = demography::citizen_by_index(world, *m) else {
            continue;
        };
        if let Some(r) = world.get_mut::<Residence>(c) {
            r.building = home.building;
            r.unit = home.unit;
            r.district = home.district;
        }
    }
}

fn kubelek(min: u16) -> usize {
    ((min / COMMUTE_BIN_MIN) as usize).min(COMMUTE_BINS - 1)
}

/// Mediana czasu dojazdu liczona **co minutę**, nie z kubełków histogramu.
///
/// Kubełek ma pięć minut, a kryterium `gen_commute_hist` mówi o odchyleniu mediany
/// ≤ 5 % — czyli przy medianie 20 minut o jednej minucie. Mediana odczytana z kubełka
/// miałaby rozdzielczość gorszą od kryterium, które ma mierzyć.
fn mediana_minutowa(zaloga: &[Vec<Pracownik>]) -> u16 {
    let mut licznik = [0u32; 512];
    let mut n = 0u32;
    for lista in zaloga {
        for p in lista {
            licznik[usize::from(p.commute).min(511)] += 1;
            n += 1;
        }
    }
    let polowa = n / 2;
    let mut suma = 0u32;
    for (m, k) in licznik.iter().enumerate() {
        suma += k;
        if suma >= polowa {
            return m as u16;
        }
    }
    0
}

/// Docelowy histogram czasu dojazdu: rozkład logarytmiczno-normalny w promilach.
pub(super) fn histogram_docelowy(median: u16, sigma_centi: u16) -> [u32; COMMUTE_BINS] {
    let mu = det_math::ln(f64::from(median.max(1)));
    let sigma = f64::from(sigma_centi.max(1)) / 100.0;
    let mut gestosc = [0f64; COMMUTE_BINS];
    let mut suma = 0f64;
    for (i, g) in gestosc.iter_mut().enumerate() {
        let x = (i as f64 + 0.5) * f64::from(COMMUTE_BIN_MIN);
        let z = (det_math::ln(x) - mu) / sigma;
        *g = det_math::exp(-0.5 * z * z) / x;
        suma += *g;
    }
    let mut out = [0u32; COMMUTE_BINS];
    for (i, g) in gestosc.iter().enumerate() {
        out[i] = (g / suma * 1000.0) as u32;
    }
    out
}

/// Korelacja rang Spearmana ×100 dla permutacji „ranga dochodu → ranga wartości".
fn spearman(perm: &[usize]) -> i16 {
    let n = perm.len() as i128;
    if n < 2 {
        return 0;
    }
    let suma_d2: i128 = perm
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let d = i as i128 - *p as i128;
            d * d
        })
        .sum();
    let rho = 100 - (600 * suma_d2) / (n * (n * n - 1));
    rho.clamp(-100, 100) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spearman_liczy_zgodnie_z_definicja() {
        assert_eq!(spearman(&[0, 1, 2, 3, 4]), 100);
        assert_eq!(spearman(&[4, 3, 2, 1, 0]), -100);
    }

    #[test]
    fn histogram_docelowy_jest_rozkladem_z_maksimum_przy_medianie() {
        let h = histogram_docelowy(24, 62);
        let suma: u32 = h.iter().sum();
        assert!((980..=1000).contains(&suma), "{suma}");
        let szczyt = h
            .iter()
            .enumerate()
            .max_by_key(|(_, v)| **v)
            .map(|(i, _)| i);
        // Moda rozkładu log-normalnego leży poniżej mediany — kubełek 2 lub 3 (10–20 min).
        assert!(matches!(szczyt, Some(2..=3)), "{szczyt:?}");
    }
}
