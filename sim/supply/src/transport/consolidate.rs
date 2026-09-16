//! Konsolidacja dostaw — milk-run (M6b §5.6, wykonanie w WP12 / M6d).
//!
//! Heurystyka zachłanna **Clarke–Wrighta**, twardy limit dwunastu punktów na trasę.
//! Optymalizator VRP jest niepotrzebny i to jest decyzja, nie brak czasu: trasa jest
//! **jedną funkcją do podmiany**, a różnica między rozwiązaniem zachłannym a optymalnym
//! przy dwunastu punktach jest mniejsza niż szum stawki przewozowej, którą i tak stroi
//! balansator (`R7`).
//!
//! **Dlaczego konsolidacja w ogóle coś oszczędza.** Kilometr ciężarówki kosztuje tyle
//! samo z ładunkiem i bez, a podstawienie płaci się raz za pojazd — więc dziesięć
//! kursów po jednym sklepie płaci dziesięć razy za dojazd do dzielnicy i dziesięć razy
//! za podstawienie, a jeden kurs po dziesięciu sklepach płaci raz za jedno i raz
//! za drugie. Przy stawce **tonokilometrowej** tej różnicy by nie było, bo koszt byłby
//! liniowy w masie niezależnie od liczby kursów — i dlatego stawka przeniosła się
//! do katalogu pojazdów jako grosze za kilometr (`AL-2`).

use magnat_core::{Mass, Money, SiteId};

use super::{FreightOracle, Transport, TransportOrderId};

/// Trasa objazdowa: jeden pojazd, jeden nadawca, kilka punktów rozładunku.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct MilkRun {
    pub from: SiteId,
    /// Zlecenia w kolejności rozładunku.
    pub stops: Vec<TransportOrderId>,
    pub mass: Mass,
    pub distance_m: u32,
    pub minutes: u32,
    pub cost: Money,
}

impl MilkRun {
    /// Koszt na tonę — liczba, którą porównuje kryterium `dc_beats_direct`.
    #[must_use]
    pub fn cost_per_tonne(&self) -> Money {
        if self.mass.0 <= 0 {
            return Money::ZERO;
        }
        Money((i128::from(self.cost.0) * 1_000_000 / i128::from(self.mass.0)) as i64)
    }
}

/// Parametry konsolidacji. Wszystkie są **ograniczeniami floty i okna dostaw**,
/// a nie strojeniem: pojazd o danej ładowności nie zabierze więcej, a przekroczenie
/// dwunastu punktów przestaje być trasą, a zaczyna dniówką kierowcy.
#[derive(Clone, Copy, Debug)]
pub struct ConsolidationLimits {
    pub capacity: Mass,
    pub max_stops: usize,
    /// Promień, w którym cele wolno łączyć — 4 km z §5.6. Mierzony odległością
    /// trasy między celami, nie w linii prostej: dwa sklepy po dwóch stronach rzeki
    /// są blisko na mapie i daleko na drodze.
    pub radius_m: u32,
}

impl Default for ConsolidationLimits {
    fn default() -> ConsolidationLimits {
        ConsolidationLimits {
            capacity: Mass(24_000_000),
            max_stops: 12,
            radius_m: 4_000,
        }
    }
}

/// Łączy zlecenia w trasy objazdowe. Zlecenia spoza jednego nadawcy są ignorowane —
/// wołający grupuje je wcześniej, bo to on wie, które okno czasowe rozpatruje.
///
/// Wynik jest deterministyczny: oszczędności sortuje się malejąco z remisem po parze
/// identyfikatorów zleceń, a te są nadawane rosnąco i nigdy nie wracają (00 §3.2).
#[must_use]
pub fn consolidate(
    t: &Transport,
    oracle: &dyn FreightOracle,
    drafts: &[TransportOrderId],
    limits: ConsolidationLimits,
) -> Vec<MilkRun> {
    let mut punkty: Vec<Punkt> = Vec::new();
    let mut depot = None;
    for id in drafts {
        let Some(o) = t.get(*id) else { continue };
        if depot.is_none() {
            depot = Some(o.from);
        }
        if depot != Some(o.from) {
            continue;
        }
        let Some(q) = oracle.quote(o.from, o.to, o.mass, &o.requires) else {
            continue;
        };
        punkty.push(Punkt {
            order: *id,
            site: o.to,
            mass: o.mass,
            do_depotu_m: q.distance_m,
            koszt_wprost: q.cost,
            minut_wprost: q.minutes,
        });
    }
    let Some(depot) = depot else { return Vec::new() };
    punkty.sort_unstable_by_key(|p| p.order.0);

    // Każdy punkt zaczyna jako własna trasa. Scalanie idzie po oszczędnościach
    // Clarke–Wrighta: `s(i,j) = d(0,i) + d(0,j) − d(i,j)`.
    let mut trasy: Vec<Vec<usize>> = (0..punkty.len()).map(|i| vec![i]).collect();
    let mut oszczednosci: Vec<(i64, usize, usize)> = Vec::new();
    for i in 0..punkty.len() {
        for j in (i + 1)..punkty.len() {
            let Some(q) = oracle.quote(
                punkty[i].site,
                punkty[j].site,
                punkty[j].mass,
                &t.get(punkty[j].order).expect("zlecenie").requires,
            ) else {
                continue;
            };
            if q.distance_m > limits.radius_m {
                continue;
            }
            let s = i64::from(punkty[i].do_depotu_m) + i64::from(punkty[j].do_depotu_m)
                - i64::from(q.distance_m);
            if s > 0 {
                oszczednosci.push((s, i, j));
            }
        }
    }
    // Malejąco po oszczędności, remis po parze indeksów — bez tego wynik zależałby
    // od kolejności wstawiania do kolekcji.
    oszczednosci.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));

    for (_, i, j) in oszczednosci {
        let (Some(ti), Some(tj)) = (trasa_z(&trasy, i), trasa_z(&trasy, j)) else {
            continue;
        };
        if ti == tj {
            continue;
        }
        // Scalać wolno wyłącznie **końce** tras: punkt w środku trasy jest już obsłużony
        // między dwoma sąsiadami i wpychanie się tam zmienia obie krawędzie naraz,
        // czego oszczędność Clarke–Wrighta nie wycenia.
        if *trasy[ti].last().expect("niepusta") != i || trasy[tj][0] != j {
            continue;
        }
        if trasy[ti].len() + trasy[tj].len() > limits.max_stops {
            continue;
        }
        let masa: i64 = trasy[ti]
            .iter()
            .chain(trasy[tj].iter())
            .map(|&k| punkty[k].mass.0)
            .sum();
        if masa > limits.capacity.0 {
            continue;
        }
        let ogon = std::mem::take(&mut trasy[tj]);
        trasy[ti].extend(ogon);
    }

    let mut wynik = Vec::new();
    for tr in trasy.into_iter().filter(|t| !t.is_empty()) {
        wynik.push(zbuduj(depot, &punkty, &tr, oracle, t));
    }
    wynik.sort_unstable_by_key(|r| r.stops[0].0);
    wynik
}

struct Punkt {
    order: TransportOrderId,
    site: SiteId,
    mass: Mass,
    do_depotu_m: u32,
    koszt_wprost: Money,
    minut_wprost: u32,
}

fn trasa_z(trasy: &[Vec<usize>], p: usize) -> Option<usize> {
    trasy.iter().position(|t| t.contains(&p))
}

/// Składa trasę z kolejnych odcinków. Ładunek maleje na każdym przystanku, więc
/// odcinki wycenia się masą, która **jeszcze** jedzie — inaczej ostatni kilometr
/// byłby wyceniony jak pierwszy.
fn zbuduj(
    depot: SiteId,
    punkty: &[Punkt],
    tr: &[usize],
    oracle: &dyn FreightOracle,
    t: &Transport,
) -> MilkRun {
    if tr.len() == 1 {
        let p = &punkty[tr[0]];
        return MilkRun {
            from: depot,
            stops: vec![p.order],
            mass: p.mass,
            distance_m: p.do_depotu_m,
            minutes: p.minut_wprost,
            cost: p.koszt_wprost,
        };
    }
    let calosc: i64 = tr.iter().map(|&k| punkty[k].mass.0).sum();
    let mut zostalo = calosc;
    let mut skad = depot;
    let (mut dystans, mut minuty, mut koszt) = (0u32, 0u32, 0i64);
    let mut podstawienie = Money::ZERO;
    for &k in tr {
        let p = &punkty[k];
        let req = &t.get(p.order).expect("zlecenie").requires;
        if let Some(q) = oracle.quote(skad, p.site, Mass(zostalo), req) {
            dystans = dystans.saturating_add(q.distance_m);
            minuty = minuty.saturating_add(q.minutes);
            koszt += q.cost.0;
            podstawienie = q.call_out;
        }
        zostalo -= p.mass.0;
        skad = p.site;
    }
    // Podstawienie płaci się **raz za pojazd**, a każdy odcinek doliczył je osobno.
    koszt -= podstawienie.0 * (tr.len() as i64 - 1);
    MilkRun {
        from: depot,
        stops: tr.iter().map(|&k| punkty[k].order).collect(),
        mass: Mass(calosc),
        distance_m: dystans,
        minutes: minuty,
        cost: Money(koszt.max(0)),
    }
}
