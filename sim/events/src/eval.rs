//! Odczyt sond ze świata i pętla oceny definicji (M8c §5.5, WP4).
//!
//! Podział z rozmysłu: [`ProbeWorld`] **czyta**, a nie ocenia, i nie ma prawa
//! niczego zmienić — `&World`, nie `&mut`. Ocena jest funkcją czystą nad wartościami,
//! które ten moduł wyciągnął. Dzięki temu test hazardu nie potrzebuje świata,
//! a świat nie potrzebuje hazardu.

use crate::catalog::{EventScope, Precondition};
use crate::hazard::{self, HazardTrace};
use crate::probe::{Probe, ProbeCache};
use crate::registry::{Events, ScopeInstance};
use magnat_core::{Season, SiteId, Tick, UtilityService};
use magnat_ecs::World;
use magnat_firms::{FirmKey, Firms};
use magnat_supply::ChainHandle;
use magnat_traffic::utility::UtilityGrids;

/// Ile minut ma doba gry. Lokalnie, bo `core::time` nazywa to `MINUTES_PER_DAY`
/// i re-eksport w tym module byłby dłuższy od użycia.
const MINUTES_PER_DAY: u64 = 1440;

/// Czytelnik świata dla sond. Pożycza świat **niemutowalnie** i pamięta odczyty.
pub struct ProbeWorld<'a> {
    world: &'a World,
    ev: &'a Events,
    chain: Option<ChainHandle>,
    day: u64,
    now_minute: u64,
    pub cache: ProbeCache,
}

impl<'a> ProbeWorld<'a> {
    #[must_use]
    pub fn new(world: &'a World, ev: &'a Events, t: Tick) -> ProbeWorld<'a> {
        ProbeWorld {
            chain: world.get_resource::<ChainHandle>().cloned(),
            world,
            ev,
            day: t.0 / MINUTES_PER_DAY,
            now_minute: t.0,
            cache: ProbeCache::default(),
        }
    }

    fn firms(&self) -> Option<&Firms> {
        self.world.get_resource::<Firms>()
    }

    fn grids(&self) -> Option<&UtilityGrids> {
        self.world.get_resource::<UtilityGrids>()
    }

    /// Wartość sondy dla instancji zakresu. Wynik jest zapamiętany na ten tick.
    pub fn value(&mut self, p: Probe, scope: ScopeInstance) -> i64 {
        // Sonda miejska ma klucz zerowy, więc `UnemploymentPermille` używana
        // przez dwanaście definicji liczy się raz na cały przebieg oceny.
        let klucz = if scope_dependent(p) { scope.key() } else { 0 };
        if let Some(v) = self.cache.peek(p, klucz) {
            return v;
        }
        let v = self.compute(p, scope);
        self.cache.put(p, klucz, v);
        v
    }

    #[allow(clippy::too_many_lines)]
    fn compute(&mut self, p: Probe, scope: ScopeInstance) -> i64 {
        let w = self.ev.weather();
        match p {
            Probe::AirTempDc => i64::from(w.weather().temp_dc),
            Probe::PrecipDeficit30dChm => w.precip_deficit_chm(30),
            Probe::PrecipDeficit90dChm => w.precip_deficit_chm(90),
            Probe::SnowCoverMm => i64::from(w.weather().snow_cover_mm),
            Probe::WindKmh => i64::from(w.weather().wind_kmh),
            Probe::HeatingDegreeDc => i64::from(w.heating_degree_dc()),
            Probe::SeasonIndex => Season::of_day((self.day % 360) as u16).as_index() as i64,

            Probe::GridLoadFactorBps => self.siec(scope, |g, s| i64::from(g.load_factor_bps(s))),
            Probe::GridReserveMarginBps => {
                self.siec(scope, |g, s| i64::from(g.reserve_margin_bps(s)))
            }
            Probe::GridUnserved => self.siec(scope, |g, s| {
                g.nets()
                    .iter()
                    .position(|n| n.service == s)
                    .map_or(0, |i| g.unserved(i))
            }),
            Probe::SourceAgeDays => self.zrodlo_linie(scope, Miara::WiekDob),
            Probe::SourceMaintenanceOverdueDays => {
                self.zrodlo_linie(scope, Miara::ZaleglaKonserwacja)
            }
            Probe::SourceConditionQ => self.zrodlo_linie(scope, Miara::Kondycja),

            Probe::SiteConditionQ => self.zaklad_linie(scope, Miara::Kondycja),
            Probe::SiteMaintenanceOverdueDays => {
                self.zaklad_linie(scope, Miara::ZaleglaKonserwacja)
            }
            Probe::SiteLaborPct => {
                let ScopeInstance::Site(s) = scope else {
                    return 0;
                };
                self.chain.as_ref().map_or(0, |c| {
                    c.lock()
                        .plant
                        .get(s)
                        .map_or(0, |z| i64::from(z.effective_labor_pct()))
                })
            }

            Probe::FirmHeadcount => self.firma(scope, |f, firms| {
                f.sites
                    .iter()
                    .filter_map(|s| firms.site(*s))
                    .map(|s| i64::try_from(s.headcount()).unwrap_or(0))
                    .sum()
            }),
            Probe::FirmAgeDays => self.firma(scope, |f, _| {
                i64::try_from(self.now_minute.saturating_sub(f.founded.get()) / MINUTES_PER_DAY)
                    .unwrap_or(0)
            }),
            Probe::FirmMoraleQ => self.morale(scope),
            Probe::FirmWageGapPermille => self.luka_placowa(scope),

            Probe::UnemploymentPermille => i64::from(self.ev.indicators().unemployment_permille),
            Probe::CpiYoyBp => i64::from(self.ev.indicators().cpi_yoy_bp),
            Probe::MoodMean => i64::from(self.ev.indicators().mood_mean.get()),
        }
    }

    fn siec<F: Fn(&UtilityGrids, UtilityService) -> i64>(&self, scope: ScopeInstance, f: F) -> i64 {
        let (ScopeInstance::Network(n), Some(g)) = (scope, self.grids()) else {
            return 0;
        };
        g.nets().get(n as usize).map_or(0, |net| f(g, net.service))
    }

    /// Linie zakładu prowadzącego źródło sieci.
    fn zrodlo_linie(&self, scope: ScopeInstance, m: Miara) -> i64 {
        let (ScopeInstance::Network(n), Some(g)) = (scope, self.grids()) else {
            return 0;
        };
        let Some(net) = g.nets().get(n as usize) else {
            return 0;
        };
        let Some(site) = net
            .sources()
            .into_iter()
            .find_map(|i| net.source_info(i).and_then(|(_, _, s)| s))
        else {
            return 0;
        };
        self.linie(site, m)
    }

    fn zaklad_linie(&self, scope: ScopeInstance, m: Miara) -> i64 {
        let ScopeInstance::Site(s) = scope else {
            return 0;
        };
        self.linie(s, m)
    }

    /// Miara po liniach zakładu — najgorsza linia, bo to ona się psuje.
    fn linie(&self, site: SiteId, m: Miara) -> i64 {
        let Some(c) = self.chain.as_ref() else {
            return 0;
        };
        let g = c.lock();
        let Some(z) = g.plant.get(site) else {
            return 0;
        };
        let mut wynik: Option<i64> = None;
        for l in &z.lines {
            let v = match m {
                Miara::WiekDob => i64::try_from(l.age_minutes / MINUTES_PER_DAY).unwrap_or(0),
                Miara::Kondycja => i64::from(l.condition.get()),
                Miara::ZaleglaKonserwacja => i64::try_from(
                    self.now_minute.saturating_sub(l.next_maintenance.get()) / MINUTES_PER_DAY,
                )
                .unwrap_or(0),
            };
            wynik = Some(match (wynik, m) {
                // Kondycja: najgorsza. Wiek i zaległość: największe. W obu wypadkach
                // pytamy o **najsłabsze ogniwo**, bo awaria zaczyna się w nim.
                (None, _) => v,
                (Some(a), Miara::Kondycja) => a.min(v),
                (Some(a), _) => a.max(v),
            });
        }
        wynik.unwrap_or(0)
    }

    fn firma<F: Fn(&magnat_firms::Firm, &Firms) -> i64>(&self, scope: ScopeInstance, f: F) -> i64 {
        let (ScopeInstance::Firm(k), Some(firms)) = (scope, self.firms()) else {
            return 0;
        };
        firms.get(k).map_or(0, |fi| f(fi, firms))
    }

    /// Średni nastrój załogi firmy, 0..=100.
    ///
    /// Liczony ze spisu pracowników, a nie z gotowej mapy morale rynku pracy:
    /// tamta powstaje raz na dobę dla **wszystkich** zakładów miasta, a hazard
    /// pyta o kilka firm, które przeszły bramkę (ryzyko `R6`).
    fn morale(&self, scope: ScopeInstance) -> i64 {
        let (ScopeInstance::Firm(k), Some(firms)) = (scope, self.firms()) else {
            return 0;
        };
        let Some(f) = firms.get(k) else { return 0 };
        let mut suma: i64 = 0;
        let mut ile: i64 = 0;
        for s in &f.sites {
            let Some(site) = firms.site(*s) else { continue };
            for p in &site.positions {
                for e in &p.filled {
                    if let Some(v) = self.world.get::<magnat_agents::Vitals>(e.citizen.0) {
                        // Nastrój −100..=100 na skalę 0..=100, bo krzywe w katalogu
                        // czyta się łatwiej na skali bez znaku.
                        suma += i64::from(v.mood) + 100;
                        ile += 2;
                    }
                }
            }
        }
        if ile == 0 {
            50
        } else {
            suma / ile
        }
    }

    /// O ile promili firma płaci **poniżej** mediany zawartych umów w zawodzie
    /// i dzielnicy. Zero znaczy „płaci jak rynek albo lepiej".
    ///
    /// To jest sonda strajku z §5.5 i jedyna, która ma dwa źródła naraz: płace
    /// z rejestru firm i medianę z rynku pracy. Bez mediany „luka płacowa" byłaby
    /// liczbą bez odniesienia.
    fn luka_placowa(&self, scope: ScopeInstance) -> i64 {
        let (ScopeInstance::Firm(k), Some(firms)) = (scope, self.firms()) else {
            return 0;
        };
        let Some(lh) = self.world.get_resource::<magnat_economy::LaborHandle>() else {
            return 0;
        };
        let Some(rynek) = lh.get() else { return 0 };
        let Some(f) = firms.get(k) else { return 0 };
        let mut suma: i64 = 0;
        let mut ile: i64 = 0;
        for s in &f.sites {
            let Some(site) = firms.site(*s) else { continue };
            for p in &site.positions {
                let Some(med) = rynek.stats().median_accepted(p.role, site.district) else {
                    continue;
                };
                if med.get() <= 0 {
                    continue;
                }
                for e in &p.filled {
                    let luka = (med.get() - e.wage_month.get()).max(0) * 1_000 / med.get();
                    suma += luka;
                    ile += 1;
                }
            }
        }
        if ile == 0 {
            0
        } else {
            suma / ile
        }
    }
}

#[derive(Clone, Copy)]
enum Miara {
    WiekDob,
    Kondycja,
    ZaleglaKonserwacja,
}

/// Czy wartość sondy zależy od instancji zakresu. Miejskie wskaźniki nie zależą,
/// więc liczą się raz na wszystkie definicje i wszystkie instancje.
fn scope_dependent(p: Probe) -> bool {
    matches!(
        p,
        Probe::GridLoadFactorBps
            | Probe::GridReserveMarginBps
            | Probe::GridUnserved
            | Probe::SourceAgeDays
            | Probe::SourceMaintenanceOverdueDays
            | Probe::SourceConditionQ
            | Probe::SiteConditionQ
            | Probe::SiteMaintenanceOverdueDays
            | Probe::SiteLaborPct
            | Probe::FirmHeadcount
            | Probe::FirmAgeDays
            | Probe::FirmMoraleQ
            | Probe::FirmWageGapPermille
    )
}

/// Czy instancja przechodzi bramkę definicji.
pub fn gate_ok(pw: &mut ProbeWorld<'_>, gate: &[Precondition], scope: ScopeInstance) -> bool {
    for g in gate {
        let ok = match g {
            Precondition::Season(s) => {
                let sez = Season::of_day((pw.day % 360) as u16);
                s.contains(&sez)
            }
            Precondition::MinAirTempDc(v) => pw.value(Probe::AirTempDc, scope) >= i64::from(*v),
            Precondition::MaxAirTempDc(v) => pw.value(Probe::AirTempDc, scope) <= i64::from(*v),
            Precondition::HasFarms => pw.ev.has_farms(match scope {
                ScopeInstance::District(d) => Some(d),
                _ => None,
            }),
            Precondition::SourceOnline => {
                // Sieć bez czynnego źródła nie ma czego zgasić. Sprawdzenie idzie
                // po stanie węzła, a nie po bilansie — wyspa bez prądu to nie to
                // samo co blok, który stoi.
                let (ScopeInstance::Network(n), Some(g)) = (scope, pw.grids()) else {
                    return false;
                };
                g.nets().get(n as usize).is_some_and(|net| {
                    net.sources()
                        .into_iter()
                        .any(|i| net.source_info(i).is_some_and(|(_, on, _)| on))
                })
            }
            Precondition::Service(lista) => {
                let (ScopeInstance::Network(n), Some(g)) = (scope, pw.grids()) else {
                    return false;
                };
                g.nets()
                    .get(n as usize)
                    .is_some_and(|net| lista.contains(&net.service))
            }
            Precondition::MinFirmHeadcount(v) => {
                pw.value(Probe::FirmHeadcount, scope) >= i64::from(*v)
            }
            Precondition::MinFirmAgeDays(v) => pw.value(Probe::FirmAgeDays, scope) >= i64::from(*v),
            Precondition::MinSiteLines(v) => {
                let ScopeInstance::Site(s) = scope else {
                    return false;
                };
                pw.chain.as_ref().is_some_and(|c| {
                    c.lock()
                        .plant
                        .get(s)
                        .is_some_and(|z| z.lines.len() as u32 >= *v)
                })
            }
        };
        if !ok {
            return false;
        }
    }
    true
}

/// Diagnoza jednej definicji z przebiegu oceny: `(definicja, kandydaci, odsiane,
/// ślad najwyższego hazardu)`. Krotka, nie struktura, bo żyje przez trzy linie
/// między oceną a zapisem do rejestru.
pub type DiagRow = (u16, u32, u32, HazardTrace);

/// Zdarzenie, które właśnie zaszło — wynik przebiegu oceny.
pub struct Fired {
    pub def: u16,
    pub scope: ScopeInstance,
    pub severity_bps: u16,
}

/// Jeden przebieg oceny: wszystkie definicje o zadanej częstotliwości.
///
/// Zwraca listę zdarzeń do otwarcia oraz — przez `&mut Events` po powrocie —
/// diagnostykę „dlaczego jeszcze nie". Kolejność wyniku idzie po `(definicja,
/// instancja)`, czyli po posortowanym kluczu, nigdy po kolejności iteracji mapy.
#[must_use]
pub fn evaluate(
    ev: &Events,
    world: &World,
    t: Tick,
    hourly: bool,
    seed: u64,
) -> (Vec<Fired>, Vec<DiagRow>) {
    let mut pw = ProbeWorld::new(world, ev, t);
    let day = t.0 / MINUTES_PER_DAY;
    let sieci = u8::try_from(
        world
            .get_resource::<UtilityGrids>()
            .map_or(0, |g| g.nets().len()),
    )
    .unwrap_or(u8::MAX);
    // Lista firm powstaje **leniwie**: zakres `Firm` mają wyłącznie definicje
    // oceniane raz na dobę, a ten przebieg wywołuje się też co godzinę. Zebranie
    // i posortowanie dwustu kluczy dwadzieścia cztery razy na dobę po to, żeby
    // ani razu ich nie użyć, jest kosztem, który rośnie razem z miastem.
    let mut firmy: Option<Vec<FirmKey>> = None;

    let mut wynik = Vec::new();
    let mut diag = Vec::new();
    for (i, d) in ev.catalog().defs.iter().enumerate() {
        let idx = u16::try_from(i).unwrap_or(u16::MAX);
        if d.hourly != hourly {
            continue;
        }
        if ev.concurrent(idx) >= d.max_concurrent || ev.in_cooldown(idx, day) {
            continue;
        }
        let instancje: Vec<ScopeInstance> = if d.scope == EventScope::Firm {
            firmy
                .get_or_insert_with(|| crate::apply::firm_instances(world.get_resource::<Firms>()))
                .iter()
                .copied()
                .map(ScopeInstance::Firm)
                .collect()
        } else {
            crate::registry::scope_instances(ev, idx, d.scope, sieci)
        };
        let mut kandydaci = 0u32;
        let mut odsiane = 0u32;
        let mut najlepszy = HazardTrace::default();
        let mut wolne = i32::from(d.max_concurrent) - i32::from(ev.concurrent(idx));
        for inst in instancje {
            if ev.busy(idx, inst) {
                continue;
            }
            if !gate_ok(&mut pw, &d.trigger.gate, inst) {
                odsiane += 1;
                continue;
            }
            kandydaci += 1;
            let mut slad = HazardTrace::default();
            let ppm = hazard::hazard_ppm(d, |p| pw.value(p, inst), Some(&mut slad));
            if ppm > najlepszy.ppm {
                najlepszy = slad;
            }
            if wolne > 0 && hazard::roll(seed, idx, inst.key(), t, ppm) {
                let sev = hazard::severity_bps(&d.severity, seed, idx, inst.key(), t, |p| {
                    pw.value(p, inst)
                });
                wynik.push(Fired {
                    def: idx,
                    scope: inst,
                    severity_bps: sev,
                });
                wolne -= 1;
            }
        }
        diag.push((idx, kandydaci, odsiane, najlepszy));
    }
    (wynik, diag)
}
