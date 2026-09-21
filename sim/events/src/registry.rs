//! Rejestr zdarzeń: co trwa, od kiedy, na czym i co z tego wynika (M8c §5.5).

use crate::catalog::{EventCatalog, EventDef, EventScope};
use crate::hazard::FactorTrace;
use crate::indicators::{CityIndicators, EpochClock};
use crate::param::{ParamOverlay, ParamPatch, SimParam};
use crate::probe::{SiteFilter, SiteRef};
use crate::weather::WeatherState;
use magnat_core::{
    CityReason, DecisionReason, EventCategory, EventId, GoodId, HashState, NeedKind, SiteId,
    StateHasher, TariffClassId, Tick,
};
use magnat_firms::FirmKey;

/// Ile wpisów kroniki i powodów trzyma rejestr. Kronika pełna należy do M8e —
/// tu jest pierścień, żeby karta inspekcji miała co pokazać, a przebieg stuletni
/// nie rósł w nieskończoność.
pub const LOG_RING: usize = 128;

/// Instancja zakresu: **na czym** zaszło zdarzenie.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScopeInstance {
    World,
    District(u16),
    Site(SiteId),
    Firm(FirmKey),
    Network(u8),
}

impl ScopeInstance {
    /// Klucz liczbowy instancji — do strumienia losowań i do porządkowania.
    /// Rozdzielony po rodzaju zakresu, żeby dzielnica 3 i sieć 3 nie dzieliły rzutu.
    #[must_use]
    pub fn key(self) -> u64 {
        match self {
            ScopeInstance::World => 0,
            ScopeInstance::District(d) => 1 << 56 | u64::from(d),
            ScopeInstance::Site(s) => 2 << 56 | u64::from(s.0.index()),
            ScopeInstance::Firm(f) => 3 << 56 | (f.0 & 0x00FF_FFFF_FFFF_FFFF),
            ScopeInstance::Network(n) => 4 << 56 | u64::from(n),
        }
    }

    fn hash_state(self, h: &mut StateHasher) {
        h.write_u64(self.key());
    }
}

/// Dlaczego zdarzenie powstało.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EventCause {
    /// Ze stanu świata przez hazard — droga normalna.
    Hazard,
    /// Wywołane ręcznie: scenariusz testowy, konsola, narzędzie balansu.
    Forced,
}

/// Zdarzenie, które trwa.
#[derive(Clone, Debug)]
pub struct WorldEvent {
    pub id: EventId,
    pub def: u16,
    pub scope: ScopeInstance,
    pub started_day: u64,
    /// Doba, po której zdarzenie gaśnie. `None` znaczy „dopóki stan świata je trzyma".
    pub ends_day: Option<u64>,
    /// Najwcześniejsza doba zakończenia dla wariantu `UntilBelow` — zdarzenie nie
    /// gaśnie w tej samej dobie, w której powstało.
    pub min_end_day: u64,
    pub severity_bps: u16,
    pub cause: EventCause,
    pub patches: Vec<ParamPatch>,
}

impl WorldEvent {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.id.0);
        h.write_u16(self.def);
        self.scope.hash_state(h);
        h.write_u64(self.started_day);
        h.write_u64(self.ends_day.unwrap_or(u64::MAX));
        h.write_u64(self.min_end_day);
        h.write_u16(self.severity_bps);
        h.write_u8(u8::from(self.cause == EventCause::Forced));
        h.write_u32(self.patches.len() as u32);
        for p in &self.patches {
            h.write_i64(p.value);
        }
    }
}

/// Wpis do kroniki miasta.
///
/// Niesie **podmiot**, a nie sam tekst: `EventId` jest uchwytem do zdarzenia
/// (`K-62`), więc kliknięcie w wiersz kroniki otwiera kartę tego, czego wpis
/// dotyczy. Gdyby wpis powstał tu jako napis, M9 musiałby wyciągać podmiot
/// z tekstu — a to jest droga, z której się nie wraca.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ChronicleEntry {
    pub at: Tick,
    pub event: EventId,
    pub def: u16,
    pub category: EventCategory,
    pub scope: ScopeInstance,
    pub severity_bps: u16,
    /// Czy to wpis o początku (`true`) czy o końcu zdarzenia.
    pub opened: bool,
}

/// Dlaczego definicja jeszcze nie zaszła — odpowiedź dla inspektora (WP4).
#[derive(Clone, Debug, Default)]
pub struct Diagnosis {
    /// Ile instancji zakresu przeszło bramkę w ostatniej ocenie.
    pub candidates: u32,
    /// Ile instancji bramka odrzuciła.
    pub gated_out: u32,
    /// Najwyższy hazard policzony w ostatniej ocenie, w ppm.
    pub best_ppm: u32,
    /// Czynniki tego najwyższego — to jest właściwa odpowiedź na „dlaczego jeszcze nie".
    pub best_factors: Vec<FactorTrace>,
    /// Ile razy definicja zaszła od początku gry.
    pub fired: u32,
    /// Doba ostatniego zakończenia — z niej liczy się karencja.
    pub last_end_day: u64,
}

/// Efekt z rozwiązanymi kluczami tekstowymi. Rozwiązanie dzieje się **raz**,
/// przy stawianiu świata: klucz `res_crude_oil` zamienia się w `GoodId` tego
/// miasta, a `raw_materials` w `TariffClassId`. Gdyby rozwiązywać go przy każdym
/// zdarzeniu, nieistniejący klucz objawiłby się jako zdarzenie bez skutku —
/// czyli jako nic.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResolvedEffect {
    SiteOutput { mul_bps: u32, filter: SiteFilter },
    SourceOnline,
    SourceCapacity { mul_bps: u32 },
    ExternalPrice { good: GoodId, mul_bps: u32 },
    Duty { class: TariffClassId, bp: i64 },
    NeedDecay { need: NeedKind, mul_bps: u32 },
}

#[derive(Debug)]
pub enum ResolveError {
    UnknownGood { key: String, def: String },
    UnknownTariffClass { key: String, def: String },
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::UnknownGood { key, def } => write!(
                f,
                "data/events: zdarzenie {def} wskazuje towar {key}, którego nie ma w katalogu"
            ),
            ResolveError::UnknownTariffClass { key, def } => write!(
                f,
                "data/events: zdarzenie {def} wskazuje klasę taryfową {key}, \
                 której nie ma w data/trade/tariffs.ron"
            ),
        }
    }
}

impl std::error::Error for ResolveError {}

/// Zasób świata: katalog, wycinek miasta, stan zdarzeń, pogoda i wskaźniki.
///
/// Jeden zasób, a nie pięć, z jednego powodu: system zdarzeń jest wyłączny
/// i wyjmuje go ze świata na czas kroku (`mem::take`), żeby móc trzymać obok
/// `&mut World`. Pięć zasobów to pięć takich wyjęć i pięć haków hasha,
/// a wszystkie i tak zmieniają się w tym samym kroku.
#[derive(Default)]
pub struct Events {
    catalog: EventCatalog,
    /// Efekty z rozwiązanymi kluczami, równolegle do `catalog.defs`.
    resolved: Vec<Vec<ResolvedEffect>>,
    /// Zakłady miasta: dzielnica i rodzaj. Dane wejściowe, nie stan.
    sites: Vec<SiteRef>,
    districts: u16,
    /// Stan: co trwa.
    active: Vec<WorldEvent>,
    next_id: u32,
    diag: Vec<Diagnosis>,
    overlay: ParamOverlay,
    chronicle: Vec<ChronicleEntry>,
    reasons: Vec<(Tick, DecisionReason)>,
    weather: WeatherState,
    indicators: CityIndicators,
    epoch: EpochClock,
}

impl Events {
    /// Buduje rejestr wokół katalogu. Rozwiązanie kluczy i wycinek miasta
    /// dokłada most (`tools/headless/src/events.rs`).
    #[must_use]
    pub fn new(catalog: EventCatalog, weather: WeatherState, epoch: EpochClock) -> Events {
        let n = catalog.defs.len();
        Events {
            resolved: vec![Vec::new(); n],
            diag: vec![
                Diagnosis {
                    last_end_day: u64::MAX,
                    ..Diagnosis::default()
                };
                n
            ],
            catalog,
            sites: Vec::new(),
            districts: 1,
            active: Vec::new(),
            next_id: 1,
            overlay: ParamOverlay::default(),
            chronicle: Vec::new(),
            reasons: Vec::new(),
            weather,
            indicators: CityIndicators::default(),
            epoch,
        }
    }

    /// Rozwiązuje klucze tekstowe efektów na identyfikatory tego miasta.
    ///
    /// # Errors
    /// Zwraca błąd, gdy katalog zdarzeń wskazuje towar albo klasę taryfową,
    /// której w tym mieście nie ma. To **ma** być błąd, a nie ciche pominięcie:
    /// zdarzenie bez skutku wygląda w raporcie tak samo jak zdarzenie, które działa.
    pub fn resolve(
        &mut self,
        good_of: &dyn Fn(&str) -> Option<GoodId>,
        class_of: &dyn Fn(&str) -> Option<TariffClassId>,
    ) -> Result<(), ResolveError> {
        use crate::param::Effect;
        for (i, d) in self.catalog.defs.iter().enumerate() {
            let mut out = Vec::with_capacity(d.effects.len());
            for e in &d.effects {
                out.push(match e {
                    Effect::SiteOutput { mul_bps, filter } => ResolvedEffect::SiteOutput {
                        mul_bps: *mul_bps,
                        filter: *filter,
                    },
                    Effect::SourceOnline => ResolvedEffect::SourceOnline,
                    Effect::SourceCapacity { mul_bps } => {
                        ResolvedEffect::SourceCapacity { mul_bps: *mul_bps }
                    }
                    Effect::ExternalPrice { good, mul_bps } => ResolvedEffect::ExternalPrice {
                        good: good_of(good).ok_or_else(|| ResolveError::UnknownGood {
                            key: good.clone(),
                            def: d.key.clone(),
                        })?,
                        mul_bps: *mul_bps,
                    },
                    Effect::Duty { class, bp } => ResolvedEffect::Duty {
                        class: class_of(class).ok_or_else(|| ResolveError::UnknownTariffClass {
                            key: class.clone(),
                            def: d.key.clone(),
                        })?,
                        bp: *bp,
                    },
                    Effect::NeedDecay { need, mul_bps } => ResolvedEffect::NeedDecay {
                        need: *need,
                        mul_bps: *mul_bps,
                    },
                });
            }
            self.resolved[i] = out;
        }
        Ok(())
    }

    /// Wycinek miasta: zakłady z dzielnicą i rodzajem, posortowane po identyfikatorze.
    /// Kolejność jest kontraktem, bo po niej idą instancje zakresu `Site`, a po nich
    /// klucze strumienia losowań.
    pub fn set_sites(&mut self, mut sites: Vec<SiteRef>, districts: u16) {
        sites.sort_unstable_by_key(|s| s.site.0.index());
        self.sites = sites;
        self.districts = districts.max(1);
    }
}

/// Odczyty. Osobny blok od budowy i od mutacji — nie dla ozdoby: `impl Events`
/// jako jedna całość przekraczał próg ostrzegawczy przeglądu strukturalnego,
/// a podział na „co ustawia świat", „co czyta gracz" i „co zmienia tick"
/// jest tu podziałem tematycznym, nie cięciem po linijkach.
impl Events {
    #[must_use]
    pub fn catalog(&self) -> &EventCatalog {
        &self.catalog
    }

    #[must_use]
    pub fn def(&self, i: u16) -> Option<&EventDef> {
        self.catalog.defs.get(i as usize)
    }

    #[must_use]
    pub fn effects(&self, i: u16) -> &[ResolvedEffect] {
        self.resolved.get(i as usize).map_or(&[], Vec::as_slice)
    }

    #[must_use]
    pub fn sites(&self) -> &[SiteRef] {
        &self.sites
    }

    #[must_use]
    pub fn districts(&self) -> u16 {
        self.districts
    }

    #[must_use]
    pub fn active(&self) -> &[WorldEvent] {
        &self.active
    }

    #[must_use]
    pub fn diagnosis(&self, i: u16) -> Option<&Diagnosis> {
        self.diag.get(i as usize)
    }

    pub fn diagnosis_mut(&mut self, i: u16) -> Option<&mut Diagnosis> {
        self.diag.get_mut(i as usize)
    }

    #[must_use]
    pub fn chronicle(&self) -> &[ChronicleEntry] {
        &self.chronicle
    }

    #[must_use]
    pub fn reasons(&self) -> &[(Tick, DecisionReason)] {
        &self.reasons
    }

    #[must_use]
    pub fn weather(&self) -> &WeatherState {
        &self.weather
    }

    pub fn weather_mut(&mut self) -> &mut WeatherState {
        &mut self.weather
    }

    #[must_use]
    pub fn indicators(&self) -> &CityIndicators {
        &self.indicators
    }

    pub fn indicators_mut(&mut self) -> &mut CityIndicators {
        &mut self.indicators
    }

    #[must_use]
    pub fn epoch(&self) -> &EpochClock {
        &self.epoch
    }

    pub fn epoch_mut(&mut self) -> &mut EpochClock {
        &mut self.epoch
    }

    pub fn overlay_mut(&mut self) -> &mut ParamOverlay {
        &mut self.overlay
    }

    #[must_use]
    pub fn overlay(&self) -> &ParamOverlay {
        &self.overlay
    }

    /// Ile instancji tej definicji trwa teraz.
    #[must_use]
    pub fn concurrent(&self, def: u16) -> u8 {
        u8::try_from(self.active.iter().filter(|e| e.def == def).count()).unwrap_or(u8::MAX)
    }

    /// Czy ta instancja zakresu jest już zajęta przez tę definicję.
    #[must_use]
    pub fn busy(&self, def: u16, scope: ScopeInstance) -> bool {
        self.active
            .iter()
            .any(|e| e.def == def && e.scope.key() == scope.key())
    }

    /// Czy karencja definicji jeszcze trwa.
    #[must_use]
    pub fn in_cooldown(&self, def: u16, day: u64) -> bool {
        let Some(d) = self.catalog.defs.get(def as usize) else {
            return true;
        };
        let Some(diag) = self.diag.get(def as usize) else {
            return true;
        };
        diag.last_end_day != u64::MAX && day < diag.last_end_day + u64::from(d.cooldown_days)
    }
}

/// Mutacje stanu: otwarcie zdarzenia, wygaszenie, złożenie patchy.
impl Events {
    /// Dopisuje zdarzenie i zwraca jego numer.
    pub fn open(&mut self, mut ev: WorldEvent, at: Tick) -> EventId {
        let id = EventId(self.next_id);
        self.next_id += 1;
        ev.id = id;
        let (kat, sev) = self
            .catalog
            .defs
            .get(ev.def as usize)
            .map_or((EventCategory::Natural, 0), |d| {
                (d.category, ev.severity_bps)
            });
        self.note_chronicle(ChronicleEntry {
            at,
            event: id,
            def: ev.def,
            category: kat,
            scope: ev.scope,
            severity_bps: sev,
            opened: true,
        });
        self.note_reason(
            at,
            DecisionReason::City(CityReason::EventStarted {
                event: id,
                category: kat,
                severity_bps: sev,
            }),
        );
        if let Some(d) = self.diag.get_mut(ev.def as usize) {
            d.fired += 1;
        }
        self.active.push(ev);
        id
    }

    /// Zamyka zdarzenia, dla których warunek końca jest spełniony, i zwraca ich liczbę.
    ///
    /// `still_holds` odpowiada na pytanie „czy stan świata wciąż trzyma to zdarzenie" —
    /// dla wariantu `UntilBelow`. Dla wariantów o znanym czasie rozstrzyga doba.
    pub fn close_expired<F: FnMut(&WorldEvent) -> bool>(
        &mut self,
        day: u64,
        at: Tick,
        mut still_holds: F,
    ) -> usize {
        let mut zamkniete = Vec::new();
        self.active.retain(|e| {
            let koniec = match e.ends_day {
                Some(d) => day > d,
                None => day >= e.min_end_day && !still_holds(e),
            };
            if koniec {
                zamkniete.push(e.clone());
            }
            !koniec
        });
        for e in &zamkniete {
            let kat = self
                .catalog
                .defs
                .get(e.def as usize)
                .map_or(EventCategory::Natural, |d| d.category);
            let dni = u16::try_from(day.saturating_sub(e.started_day)).unwrap_or(u16::MAX);
            self.note_chronicle(ChronicleEntry {
                at,
                event: e.id,
                def: e.def,
                category: kat,
                scope: e.scope,
                severity_bps: e.severity_bps,
                opened: false,
            });
            self.note_reason(
                at,
                DecisionReason::City(CityReason::EventEnded {
                    event: e.id,
                    category: kat,
                    days: dni,
                }),
            );
            if let Some(d) = self.diag.get_mut(e.def as usize) {
                d.last_end_day = day;
            }
        }
        zamkniete.len()
    }

    /// Składa wszystkie patche aktywnych zdarzeń w jeden zestaw wartości efektywnych.
    ///
    /// Kolejność: po `(SimParam, EventId)` — czyli po posortowanym kluczu, nigdy po
    /// kolejności wstawiania. Dwa zdarzenia na jeden parametr składają się tak samo
    /// niezależnie od tego, które powstało pierwsze w tej minucie.
    #[must_use]
    pub fn compose_patches(&self) -> Vec<ParamPatch> {
        let mut v: Vec<(SimParam, u32, i64)> = Vec::new();
        for e in &self.active {
            for p in &e.patches {
                v.push((p.param, e.id.0, p.value));
            }
        }
        v.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        v.into_iter()
            .map(|(param, _, value)| ParamPatch { param, value })
            .collect()
    }

    fn note_chronicle(&mut self, e: ChronicleEntry) {
        if self.chronicle.len() >= LOG_RING {
            self.chronicle.remove(0);
        }
        self.chronicle.push(e);
    }

    fn note_reason(&mut self, at: Tick, r: DecisionReason) {
        if self.reasons.len() >= LOG_RING {
            self.reasons.remove(0);
        }
        self.reasons.push((at, r));
    }

    /// Instancje zakresu definicji — lista, po której idzie ocena.
    ///
    /// Zakres `Site` zawęża się **filtrem efektu**, a nie bramką: susza nie ma
    /// po co pytać o sondy sklepu, którego i tak nie dotknie. To jest wykonanie
    /// mitygacji ryzyka `R6` („zakresy `Site`/`Firm` oceniane tylko dla kandydatów").
    #[must_use]
    pub fn site_filter_of(&self, def: u16) -> SiteFilter {
        self.effects(def)
            .iter()
            .find_map(|e| match e {
                ResolvedEffect::SiteOutput { filter, .. } => Some(*filter),
                _ => None,
            })
            .unwrap_or(SiteFilter::Any)
    }

    /// Czy dzielnica ma zakłady rolne — bramka `HasFarms`.
    #[must_use]
    pub fn has_farms(&self, district: Option<u16>) -> bool {
        self.sites.iter().any(|s| {
            s.kind == crate::probe::SiteClass::Farm && district.is_none_or(|d| s.district == d)
        })
    }
}

impl HashState for Events {
    /// Do hasha wchodzi **stan**: co trwa, jaki jest następny numer, pogoda
    /// i wskaźniki. Katalog, wycinek miasta i rozwiązane klucze to dane wejściowe
    /// i są identyczne w obu przebiegach tego samego ziarna — ta sama zasada,
    /// co przy `NeedTable` (M3a) i `TaxCode` (M8a).
    ///
    /// Diagnostyka **nie wchodzi**: jest wyjaśnieniem, nie stanem. Wchodzi za to
    /// liczba wpisów w kronice i dyskryminanty powodów — rozjazd w tym, ile razy
    /// coś się w mieście wydarzyło, jest rozjazdem w symulacji, nawet gdy salda
    /// przypadkiem się zgadzają (ta sama reguła co w `UtilityGrids`).
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.next_id);
        h.write_u32(self.active.len() as u32);
        for e in &self.active {
            e.hash_state(h);
        }
        h.write_u32(self.chronicle.len() as u32);
        for c in &self.chronicle {
            h.write_u32(c.event.0);
            h.write_u16(c.def);
            h.write_u8(u8::from(c.opened));
        }
        h.write_u32(self.reasons.len() as u32);
        for (t, r) in &self.reasons {
            h.write_u64(t.0);
            h.write_u16(r.discriminant());
        }
        self.weather.hash_state(h);
        self.indicators.hash_state(h);
        self.epoch.hash_state(h);
        h.write_u32(self.overlay.len() as u32);
        for (k, v) in self.overlay.active() {
            h.write_u64(match k {
                SimParam::SiteOutputMulBps(s) => u64::from(*s),
                SimParam::SourceOnline(a, b) => u64::from(*a) << 32 | u64::from(*b),
                SimParam::SourceCapacityMulBps(a, b) => {
                    1 << 48 | u64::from(*a) << 32 | u64::from(*b)
                }
                SimParam::ExternalPriceMulBps(g) => 2 << 48 | u64::from(*g),
                SimParam::DutyBp(c) => 3 << 48 | u64::from(*c),
                SimParam::NeedDecayMulBps(n) => 4 << 48 | u64::from(*n),
            });
            h.write_i64(*v);
        }
    }
}

/// Instancje zakresu, po których idzie ocena definicji.
#[must_use]
pub fn scope_instances(
    ev: &Events,
    def: u16,
    scope: EventScope,
    networks: u8,
) -> Vec<ScopeInstance> {
    match scope {
        EventScope::World => vec![ScopeInstance::World],
        EventScope::District => (0..ev.districts()).map(ScopeInstance::District).collect(),
        EventScope::Site => {
            let f = ev.site_filter_of(def);
            ev.sites()
                .iter()
                .filter(|s| f.accepts(s.kind))
                .map(|s| ScopeInstance::Site(s.site))
                .collect()
        }
        EventScope::Firm => Vec::new(), // wypełnia wołający, bo tylko on widzi rejestr firm
        EventScope::Network => (0..networks).map(ScopeInstance::Network).collect(),
    }
}
