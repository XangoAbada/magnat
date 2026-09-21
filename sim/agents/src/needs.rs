//! Potrzeby, ich spadek i skutki deprywacji (M3a §5.5, WP2).
//!
//! **Spadek nie jest odejmowaniem.** Poziom w chwili `t` wynika z różnicy funkcji
//! czasu absolutnego: `spadek(t0, t1) = f(t1) − f(t0)`, gdzie `f(t) = t · tempo / 6000`.
//! Dzięki temu ten sam wynik daje przeliczenie co minutę, co godzinę i shardowane 1/60
//! (WP2, tolerancja 0) — a ułamkowe tempa (4,2 pkt/h) nie wymagają ani stanu na resztę,
//! ani floata, ani kalibracji. Odejmowanie przyrostowe dawałoby tu błąd zależny od tego,
//! kiedy akurat wypadł shard, czyli od momentu startu symulacji (ryzyko R12).
//!
//! Tempa, progi i skutki są w `data/needs/needs.ron`. Kod zna kształt, nie liczby.

use crate::components::{AgentState, Needs, Vitals};
use magnat_core::{Cadence, DecisionReason, DeprivationEffect, NeedKind, PlaceKind, NEED_COUNT, Q};
use magnat_ecs::{Entity, System, SystemCtx, SystemDesc, World};
use serde::Deserialize;
use std::path::Path;

pub const NEEDS_SCHEMA_VERSION: u32 = 2;

/// Ile mieszkańców przypada na jeden tick minutowy: 1/60 populacji, każdy z nich
/// dostaje spadek za pełną godzinę (§5.5).
pub const DECAY_SHARDS: u32 = 60;

/// Skutek deprywacji i jego siła.
///
/// Jedno pole, dwa odczyty — i to jest świadome. Skutki **rate'owe** (`EnergyLoss`,
/// `HealthLoss`, `MoodLoss`, `StressGain`) czyta się jako setne punktu na godzinę
/// i stosuje je [`DeprivationEffectsSystem`]. Skutki **progowe** czyta jako promile
/// ten, kto jest ich właścicielem: `AbsenceRisk` — planer doby, `ProductivityLoss`
/// i `AmbitionGain` — rynek pracy przez [`pressure`] i `PersonFacts` (`R2-WP16`),
/// `StatusLoss` — funkcja statusu, gdzie status jest **liczony**, a nie odejmowany.
/// `AccidentRisk` czytelnika nie ma i jest jedynym takim wariantem.
/// Dwa pola liczbowe zamiast jednego dałyby w każdym wpisie jedno puste.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub struct NeedEffect {
    pub effect: DeprivationEffect,
    pub magnitude: u32,
}

/// Parametry jednej potrzeby.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct NeedSpec {
    /// Klucz tekstowy = nazwa wariantu `NeedKind` (bez rozróżniania wielkości liter).
    pub key: String,
    /// Tempo spadku w setnych punktu na godzinę; 0 = potrzeba zdarzeniowa (Health).
    pub decay_centi_per_hour: u32,
    /// Poniżej tego poziomu zaczyna się deprywacja.
    pub critical: u8,
    /// Ile punktów daje jedna wizyta w M3 (`InfinitePlaces`). M5 zastąpi to wartością
    /// zależną od kupionego dobra — planer się nie zmieni.
    pub satisfaction: u8,
    /// Ile trwa jedna wizyta, w minutach. W danych, bo różnica między zakupami
    /// (25 min) a wizytą u lekarza (45 min) jest treścią domeny, nie kodu.
    pub visit_min: u16,
    /// Gdzie da się tę potrzebę zaspokoić. Pusta lista = potrzeba, której w M3 nie
    /// zaspokaja wizyta (Safety zależy od dzielnicy, Mobility od dostępu do trasy).
    pub places: Vec<PlaceKind>,
    pub effects: Vec<NeedEffect>,
    /// Faza, która wnosi mechanizm tej potrzeby — **wymagana**, gdy potrzeba ani nie
    /// spada, ani nie ma gdzie się zaspokoić (`R2-WP16`).
    ///
    /// Bez tego pola „pusto, bo cudza faza" i „pusto, bo martwe" wygląda w danych
    /// identycznie, a różnicę zna wyłącznie komentarz — czyli nikt, kto czyta kod
    /// trzy fazy później. Tak przeżył `NeedKind::Status`: właściciela miał w M5,
    /// M5 zamknęło się bez niego i nic tego nie zauważyło.
    #[serde(default)]
    pub owner_phase: Option<String>,
}

#[derive(Deserialize)]
struct NeedsFile {
    schema_version: u32,
    /// Patrz [`NeedTable::mood_recovery_centi_per_hour`].
    mood_recovery_centi_per_hour: u32,
    needs: Vec<NeedSpec>,
}

#[derive(Debug)]
pub enum NeedTableError {
    Io(std::io::Error),
    Ron(String),
    Schema { found: u32, want: u32 },
    UnknownNeed(String),
    Missing(&'static str),
    Duplicate(&'static str),
    NoOwner(&'static str),
}

impl std::fmt::Display for NeedTableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NeedTableError::Io(e) => write!(f, "data/needs: {e}"),
            NeedTableError::Ron(m) => write!(f, "data/needs: {m}"),
            NeedTableError::Schema { found, want } => {
                write!(f, "data/needs: schema_version {found}, oczekiwano {want}")
            }
            NeedTableError::UnknownNeed(k) => write!(f, "data/needs: nieznana potrzeba {k:?}"),
            NeedTableError::Missing(k) => write!(f, "data/needs: brak potrzeby {k}"),
            NeedTableError::Duplicate(k) => write!(f, "data/needs: potrzeba {k} dwa razy"),
            NeedTableError::NoOwner(k) => write!(
                f,
                "data/needs: potrzeba {k} ani nie spada, ani nie ma gdzie się zaspokoić, a nie mówi, czyja jest — dopisz owner_phase"
            ),
        }
    }
}

impl std::error::Error for NeedTableError {}

impl From<std::io::Error> for NeedTableError {
    fn from(e: std::io::Error) -> Self {
        NeedTableError::Io(e)
    }
}

/// Tabela parametrów wszystkich potrzeb, indeksowana `NeedKind`.
#[derive(Clone, Debug)]
pub struct NeedTable {
    specs: Vec<NeedSpec>,
    mood_recovery_centi_per_hour: u32,
}

impl NeedTable {
    /// Tabela pusta — komplet potrzeb o zerowym tempie spadku i bez miejsc.
    ///
    /// Nie jest to stan produkcyjny: potrzeba, która nie spada, wygląda jak zaspokojona
    /// na zawsze. Istnieje dla ścieżek, które `PlanCtx` wymaga, a które tabeli nie
    /// czytają (podróż zna tylko widok mieszkańca) — i dla testów, które o potrzeby
    /// nie pytają.
    #[must_use]
    pub fn empty() -> NeedTable {
        NeedTable {
            specs: vec![NeedSpec::default(); NEED_COUNT],
            mood_recovery_centi_per_hour: 0,
        }
    }

    /// Wczytanie i **walidacja przed użyciem** (00 §5): plik musi opisywać każdą
    /// potrzebę dokładnie raz. Brak wpisu nie może dawać cichego zera — potrzeba,
    /// która nie spada, wygląda w symulacji jak zaspokojona na zawsze.
    pub fn load(path: &Path) -> Result<NeedTable, NeedTableError> {
        NeedTable::parse(&std::fs::read_to_string(path)?)
    }

    /// Sama walidacja, bez pliku — żeby dało się ją sprawdzić testem, a nie tylko
    /// zaufać, że przy ładowaniu zadziała.
    fn parse(txt: &str) -> Result<NeedTable, NeedTableError> {
        let file: NeedsFile = ron::from_str(txt).map_err(|e| NeedTableError::Ron(e.to_string()))?;
        if file.schema_version != NEEDS_SCHEMA_VERSION {
            return Err(NeedTableError::Schema {
                found: file.schema_version,
                want: NEEDS_SCHEMA_VERSION,
            });
        }

        let mut specs: Vec<Option<NeedSpec>> = vec![None; NEED_COUNT];
        for spec in file.needs {
            let idx = NeedKind::ALL
                .iter()
                .position(|n| n.name().eq_ignore_ascii_case(&spec.key))
                .ok_or_else(|| NeedTableError::UnknownNeed(spec.key.clone()))?;
            if specs[idx].is_some() {
                return Err(NeedTableError::Duplicate(NeedKind::ALL[idx].name()));
            }
            specs[idx] = Some(spec);
        }

        let mut out = Vec::with_capacity(NEED_COUNT);
        for (i, s) in specs.into_iter().enumerate() {
            let spec = s.ok_or(NeedTableError::Missing(NeedKind::ALL[i].name()))?;
            // Potrzeba, która ani nie spada, ani nie ma gdzie się zaspokoić, musi
            // powiedzieć, czyja jest (`R2-WP16`). `Health` tej reguły nie dotyczy:
            // tempo ma zerowe, ale spada skokiem przy chorobie i **ma** miejsca.
            if spec.decay_centi_per_hour == 0
                && spec.places.is_empty()
                && spec.owner_phase.is_none()
            {
                return Err(NeedTableError::NoOwner(NeedKind::ALL[i].name()));
            }
            out.push(spec);
        }
        Ok(NeedTable {
            specs: out,
            mood_recovery_centi_per_hour: file.mood_recovery_centi_per_hour,
        })
    }

    pub fn load_default() -> Result<NeedTable, NeedTableError> {
        NeedTable::load(&magnat_core::data_path("needs/needs.ron"))
    }

    #[inline]
    #[must_use]
    pub fn spec(&self, n: NeedKind) -> &NeedSpec {
        &self.specs[n.as_index()]
    }

    #[inline]
    #[must_use]
    pub fn places_for(&self, n: NeedKind) -> &[PlaceKind] {
        &self.spec(n).places
    }

    /// Spadek potrzeby między dwiema minutami świata, w punktach. Czysta funkcja
    /// czasu absolutnego — patrz nagłówek modułu.
    #[inline]
    #[must_use]
    pub fn decay_between(&self, n: NeedKind, from_min: u64, to_min: u64) -> u32 {
        decay_between(self.spec(n).decay_centi_per_hour, from_min, to_min)
    }

    /// Tempo powrotu nastroju do zera, w setnych punktu na godzinę (`R2-WP16`).
    ///
    /// Jedna liczba na cały plik, a nie pole potrzeby: odbudowa nie należy do żadnej
    /// z nich. Do `R2-WP16` `MoodLoss` był **jedynym** pisarzem `Vitals.mood` w całym
    /// repozytorium, więc po roku gry całe miasto siedziało na −100 — a nastrój wchodzi
    /// do produktywności pracownika i do decyzji zakupowej, czyli mierzył stałą.
    ///
    /// Zero jest sufitem i to jest jawne ograniczenie modelu: **nic nie podnosi
    /// nastroju ponad neutralny**. Dobry nastrój potrzebuje źródła — zdarzenia,
    /// relacji, marki — a żadna z tych faz do `Vitals.mood` nie pisze.
    #[inline]
    #[must_use]
    pub fn mood_recovery_centi_per_hour(&self) -> u32 {
        self.mood_recovery_centi_per_hour
    }

    /// Czy potrzeba jest w deprywacji.
    #[inline]
    #[must_use]
    pub fn is_deprived(&self, n: NeedKind, level: Q) -> bool {
        level.get() < self.spec(n).critical
    }
}

/// Skumulowany spadek od doby 0 do minuty `t`, w punktach.
#[inline]
#[must_use]
const fn decayed_at(centi_per_hour: u32, minute: u64) -> u64 {
    // tempo jest w setnych punktu na godzinę, czas w minutach: /100 /60 = /6000.
    minute * centi_per_hour as u64 / 6000
}

/// Spadek między dwiema minutami. Wydzielone z `NeedTable`, żeby dało się testować
/// samą arytmetykę bez wczytywania danych.
#[inline]
#[must_use]
pub const fn decay_between(centi_per_hour: u32, from_min: u64, to_min: u64) -> u32 {
    if to_min <= from_min || centi_per_hour == 0 {
        return 0;
    }
    (decayed_at(centi_per_hour, to_min) - decayed_at(centi_per_hour, from_min)) as u32
}

/// Skutki deprywacji w chwili `now`, jako gotowe powody do karty inspekcji (00 §7).
///
/// Liczone na żądanie, nie przechowywane: 400 tys. mieszkańców × potrzeby × doba
/// to logi, których nikt nie przeczyta, a determinizm gwarantuje, że odtworzenie
/// da dokładnie to samo (ta sama zasada co przy `plan_day_explained` w M3b).
/// `out` to zwykły `Vec`, a nie `ArrayVec`: to ścieżka na żądanie (karta inspekcji),
/// wywoływana dla jednego mieszkańca naraz, więc twardy limit bez sterty nie kupuje
/// tu niczego — a `DecisionReason` **celowo nie ma** wartości domyślnej (00 §K-12),
/// której `ArrayVec` by wymagał.
pub fn deprivation_of(needs: &Needs, table: &NeedTable, out: &mut Vec<DecisionReason>) {
    out.clear();
    for n in NeedKind::ALL {
        let level = needs.get(*n);
        if !table.is_deprived(*n, level) {
            continue;
        }
        // Pierwszy skutek jest tym, który mieszkaniec czuje najmocniej — kolejność
        // w danych jest kolejnością ważności, bo karta inspekcji pokazuje jeden wiersz.
        if let Some(e) = table.spec(*n).effects.first() {
            out.push(DecisionReason::Deprivation {
                need: *n,
                effect: e.effect,
            });
        }
    }
}

/// Nacisk deprywacji na to, co dzieje się **poza** `Vitals` (`R2-WP16`).
///
/// Skutki rate'owe (`EnergyLoss`, `HealthLoss`, `MoodLoss`, `StressGain`) stosuje
/// [`DeprivationEffectsSystem`] wprost na formie mieszkańca. Skutki progowe formy
/// nie zmieniają — zmieniają to, **jak mieszkaniec pracuje i czego chce** — więc
/// mają innego czytelnika i muszą do niego dojechać. Do `R2-WP16` nie dojeżdżały
/// do nikogo: dane deklarowały je od M3a, a wszystkie trzy ramiona `match` były puste.
///
/// Liczy je `sim/agents`, bo tylko on widzi poziomy potrzeb; czyta rynek pracy
/// przez `PersonFacts`. Wartości są **promilami z danych**, bez interpretacji —
/// co znaczy „300 promili produktywności", wie ten, kto tę liczbę mnoży.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct DeprivationPressure {
    /// Suma magnitud `ProductivityLoss` zdeprywowanych potrzeb, w promilach.
    pub productivity_loss_permille: u32,
    /// Suma magnitud `AmbitionGain` zdeprywowanych potrzeb, w promilach.
    pub ambition_gain_permille: u32,
}

impl DeprivationPressure {
    /// Czy cokolwiek naciska. Mieszkaniec bez deprywacji to większość miasta.
    #[must_use]
    pub fn is_none(&self) -> bool {
        self.productivity_loss_permille == 0 && self.ambition_gain_permille == 0
    }
}

/// Nacisk skutków progowych w danej chwili — suma po potrzebach **w deprywacji**.
///
/// Sumowanie, a nie maksimum: człowiek głodny i niewyspany naraz pracuje gorzej niż
/// głodny sam. Sufit nakłada dopiero konsument, bo to on wie, czego ta liczba dotyczy.
#[must_use]
pub fn pressure(needs: &Needs, table: &NeedTable) -> DeprivationPressure {
    let mut out = DeprivationPressure::default();
    for n in NeedKind::ALL {
        if !table.is_deprived(*n, needs.get(*n)) {
            continue;
        }
        for e in &table.spec(*n).effects {
            match e.effect {
                DeprivationEffect::ProductivityLoss => {
                    // `saturating_add`, bo magnitudy pochodzą z pliku: plik, który
                    // dopisze ich dość, ma dać nasycony nacisk, a nie panikę
                    // przy ładowaniu świata.
                    out.productivity_loss_permille =
                        out.productivity_loss_permille.saturating_add(e.magnitude);
                }
                DeprivationEffect::AmbitionGain => {
                    out.ambition_gain_permille =
                        out.ambition_gain_permille.saturating_add(e.magnitude);
                }
                _ => {}
            }
        }
    }
    out
}

/// Spadek potrzeb, shardowany 1/60 (§5.5).
///
/// System działa co minutę, ale dotyka 1/60 populacji — shard wybiera **indeks encji**,
/// nigdy pozycja w archetypie (R12: archetyp przestawia się przy narodzinach i śmierci,
/// a wtedy shard przeskakiwałby mieszkańcom pod nogami).
pub struct NeedDecaySystem {
    desc: SystemDesc,
}

impl NeedDecaySystem {
    #[must_use]
    pub fn new(world: &World) -> NeedDecaySystem {
        NeedDecaySystem {
            desc: SystemDesc::new("agents.NeedDecay", Cadence::EveryMinute)
                .with_query::<(Entity, &mut Needs), ()>(world)
                .reads_resource::<NeedTable>(world)
                .reads_resource::<crate::worldparams::NeedModifiers>(world),
        }
    }
}

impl System for NeedDecaySystem {
    fn desc(&self) -> &SystemDesc {
        &self.desc
    }

    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let now = ctx.tick.0;
        let shard = (now % u64::from(DECAY_SHARDS)) as u32;
        // Kopia tabeli przez wskaźnik: `res` pożycza świat niemutowalnie, a zapytanie
        // potrzebuje go mutowalnie. Klonowanie tabeli co tick byłoby kosztem bez powodu,
        // więc bierzemy tempa do tablicy na stosie — jedna liczba na potrzebę.
        let tempa: [u32; NEED_COUNT] = {
            let t = ctx.res::<NeedTable>();
            // Mnożnik zdarzeniowy wchodzi **tutaj**, a nie w tabeli: epidemia
            // przyspiesza spadek zdrowia na czas swojego trwania, a po jej końcu
            // świat wraca do `data/needs/needs.ron`, a nie do liczby zapisanej
            // przez ostatnie zdarzenie (M8c §5.5).
            //
            // Odczyt idzie przez `world()`, a nie `res()`, bo zasób jest **opcjonalny**:
            // świat bez zdarzeń (scenariusze M3, M4) nigdy go nie dostaje, a wartość
            // domyślna jest jednością. Dostęp jest mimo to zadeklarowany w `desc`,
            // więc scheduler widzi krawędź wszędzie tam, gdzie zasób istnieje.
            let m = ctx
                .world()
                .get_resource::<crate::worldparams::NeedModifiers>()
                .copied()
                .unwrap_or_default();
            std::array::from_fn(|i| m.decay(i, t.specs[i].decay_centi_per_hour))
        };

        // Równolegle po chunkach (00 §3.3): każdy wiersz pisze wyłącznie do własnego
        // komponentu, więc wynik nie zależy ani od kolejności ukończenia, ani od liczby
        // wątków — pilnuje tego `dwa_przebiegi_tego_samego_ziarna_daja_ten_sam_hash`.
        //
        // `ponytail:` shard jest **filtrem w przebiegu**, a nie indeksem: archetypowy ECS
        // nie umie zaadresować „co sześćdziesiątej encji" bez bocznej tablicy, którą
        // trzeba by unieważniać przy każdych narodzinach i każdym zgonie. Sufit znany
        // i zmierzony — 342 µs na 400 tys. mieszkańców na jednym wątku, czyli 0,03 %
        // ticku przy prędkości 1× (korekta D-13). Indeks dopiero, gdyby pomiar w M12
        // pokazał, że w trybie 50× to boli.
        let pool = ctx.pool;
        ctx.query::<(Entity, &mut Needs), ()>()
            .par_for_each(pool, |(e, needs)| {
                if e.index() % DECAY_SHARDS != shard {
                    return;
                }
                let od = u64::from(needs.updated_at);
                if now <= od {
                    return;
                }
                for (i, tempo) in tempa.iter().enumerate() {
                    let spadek = decay_between(*tempo, od, now);
                    if spadek > 0 {
                        needs.level[i] = needs.level[i].saturating_sub(spadek.min(255) as u8);
                    }
                }
                needs.updated_at = now as u32;
            });
    }
}

/// Skutki deprywacji na `Vitals` (§5.5).
///
/// Ten system stosuje **wyłącznie skutki rate'owe dotykające `Vitals`**: energię,
/// zdrowie, nastrój i stres. Skutki progowe formy nie zmieniają i mają własnych
/// czytelników: `AbsenceRisk` czyta planer (M3b), `ProductivityLoss` i `AmbitionGain`
/// — rynek pracy przez [`pressure`] i `PersonFacts` (`R2-WP16`). `StatusLoss` należy
/// do funkcji statusu (M3c §5.8, gdzie status jest **liczony**, a nie odejmowany),
/// a `AccidentRisk` czeka na fazę, która modeluje wypadek — jedyny wariant bez
/// czytelnika i jedyny, który `R2e-WP22` ma jeszcze rozstrzygnąć.
pub struct DeprivationEffectsSystem {
    desc: SystemDesc,
}

impl DeprivationEffectsSystem {
    #[must_use]
    pub fn new(world: &World) -> DeprivationEffectsSystem {
        DeprivationEffectsSystem {
            desc: SystemDesc::new("agents.DeprivationEffects", Cadence::EveryHour)
                .with_query::<(&Needs, &mut Vitals, &AgentState), ()>(world)
                .reads_resource::<NeedTable>(world),
        }
    }
}

impl System for DeprivationEffectsSystem {
    fn desc(&self) -> &SystemDesc {
        &self.desc
    }

    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let godzina = ctx.tick.0 / 60;
        // Ta sama sztuczka co przy spadku: ułamkowe tempo (zdrowie −0,2/h) rozdziela się
        // po numerze godziny absolutnej, więc nie potrzebuje stanu na resztę i nie zależy
        // od tego, w której godzinie zaczęła się deprywacja.
        let dawka = |centi: u32| -> u8 {
            let a = godzina * u64::from(centi) / 100;
            let b = (godzina + 1) * u64::from(centi) / 100;
            (b - a).min(255) as u8
        };

        let (tabela, odbudowa): (Vec<(u8, Vec<NeedEffect>)>, u32) = {
            let t = ctx.res::<NeedTable>();
            (
                NeedKind::ALL
                    .iter()
                    .map(|n| {
                        let s = t.spec(*n);
                        (s.critical, s.effects.clone())
                    })
                    .collect(),
                t.mood_recovery_centi_per_hour(),
            )
        };
        let powrot = dawka(odbudowa);

        let pool = ctx.pool;
        ctx.query::<(&Needs, &mut Vitals, &AgentState), ()>()
            .par_for_each(pool, |(needs, vitals, _state)| {
                // Odbudowa **przed** karami i bezwarunkowo: nastrój jest wypadkową
                // dwóch sił, a nie stanem włączanym przy braku deprywacji. Gdyby
                // wracał tylko przy zerowej deprywacji, nie wracałby nigdy — zawsze
                // któraś potrzeba jest pod progiem, a wtedy mieszkaniec tkwiłby
                // na dnie tak samo jak przed naprawą, tylko mniej widocznie.
                if vitals.mood < 0 {
                    vitals.mood = vitals.mood.saturating_add(powrot.min(127) as i8).min(0);
                }
                for (i, (prog, skutki)) in tabela.iter().enumerate() {
                    if needs.level[i] >= *prog {
                        continue;
                    }
                    for s in skutki {
                        let d = dawka(s.magnitude);
                        if d == 0 {
                            continue;
                        }
                        match s.effect {
                            DeprivationEffect::EnergyLoss => {
                                vitals.energy = vitals.energy.saturating_sub(d);
                            }
                            DeprivationEffect::HealthLoss => {
                                vitals.health = vitals.health.saturating_sub(d);
                            }
                            DeprivationEffect::MoodLoss => {
                                vitals.mood =
                                    vitals.mood.saturating_sub(d.min(127) as i8).max(-100);
                            }
                            DeprivationEffect::StressGain => {
                                vitals.stress = vitals.stress.saturating_add(d).min(100);
                            }
                            // Progowe — czyta je kto inny, patrz nagłówek systemu.
                            DeprivationEffect::StatusLoss
                            | DeprivationEffect::AbsenceRisk
                            | DeprivationEffect::AccidentRisk
                            | DeprivationEffect::ProductivityLoss
                            | DeprivationEffect::AmbitionGain => {}
                        }
                    }
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spadek_jest_dokladny_mimo_ulamkowego_tempa() {
        // 4,2 pkt/h przez dobę to 100,8 pkt — a nie 24 × floor(4,2) = 96.
        assert_eq!(decay_between(420, 0, 1440), 100);
        // Suma spadków godzina po godzinie == spadek policzony raz za całą dobę.
        let po_godzinie: u32 = (0..24)
            .map(|h| decay_between(420, h * 60, (h + 1) * 60))
            .sum();
        assert_eq!(po_godzinie, decay_between(420, 0, 1440));
    }

    #[test]
    fn shardowanie_nie_zmienia_wyniku() {
        // WP2: shard 1/60 daje ten sam poziom co przeliczanie co minutę (tolerancja 0).
        // To jest własność funkcji czasu absolutnego, nie kwestia kalibracji.
        for tempo in [15, 50, 300, 420, 600] {
            let co_minute: u32 = (0..1440).map(|m| decay_between(tempo, m, m + 1)).sum();
            let co_godzine: u32 = (0..24)
                .map(|h| decay_between(tempo, h * 60, (h + 1) * 60))
                .sum();
            let raz = decay_between(tempo, 0, 1440);
            assert_eq!(co_minute, raz, "tempo {tempo}");
            assert_eq!(co_godzine, raz, "tempo {tempo}");
        }
    }

    #[test]
    fn potrzeba_zdarzeniowa_nie_spada_sama() {
        assert_eq!(decay_between(0, 0, 100_000), 0);
    }

    #[test]
    fn tabela_z_danych_opisuje_wszystkie_potrzeby() {
        let t = NeedTable::load_default().expect("data/needs/needs.ron");
        for n in NeedKind::ALL {
            let s = t.spec(*n);
            assert!(
                s.key.eq_ignore_ascii_case(n.name()),
                "potrzeba {} trafiła pod indeks {}",
                s.key,
                n.as_index()
            );
            assert!(s.critical > 0 && s.critical <= 100);
            assert!(s.satisfaction <= 100);
            assert!(s.visit_min > 0 && s.visit_min <= 12 * 60);
        }
        // Health jest zdarzeniowa (§5.5) — jej tempo musi być zerowe, inaczej
        // każdy mieszkaniec umierałby z upływu czasu.
        assert_eq!(t.spec(NeedKind::Health).decay_centi_per_hour, 0);
    }

    /// Szkielet pliku potrzeb: wszystkie wpisy poprawne poza tym jednym, który
    /// test podmienia. Bez tego każdy przypadek musiałby powtarzać jedenaście wpisów.
    fn plik(nadpisz: &str) -> String {
        let mut wpisy: Vec<String> = NeedKind::ALL
            .iter()
            .filter(|n| !nadpisz.contains(&format!("key: \"{}\"", n.name())))
            .map(|n| {
                format!(
                    "(key: \"{}\", visit_min: 30, decay_centi_per_hour: 100, critical: 25,                      satisfaction: 40, places: [Home], effects: []),",
                    n.name()
                )
            })
            .collect();
        wpisy.push(nadpisz.to_owned());
        format!(
            "(schema_version: {NEEDS_SCHEMA_VERSION}, mood_recovery_centi_per_hour: 150,              needs: [{}])",
            wpisy.join("")
        )
    }

    #[test]
    fn potrzeba_bez_tempa_i_bez_miejsc_musi_powiedziec_czyja_jest() {
        // `R2-WP16`. Bez tej reguły „pusto, bo cudza faza" i „pusto, bo martwe"
        // wygląda w danych identycznie — i tak przeżył `NeedKind::Status`.
        let sierota = "(key: \"Safety\", visit_min: 15, decay_centi_per_hour: 0, critical: 25,                        satisfaction: 0, places: [], effects: []),";
        assert!(matches!(
            NeedTable::parse(&plik(sierota)),
            Err(NeedTableError::NoOwner("Safety"))
        ));

        let z_wlascicielem = "(key: \"Safety\", visit_min: 15, decay_centi_per_hour: 0,                               critical: 25, satisfaction: 0, places: [],                               owner_phase: Some(\"M8\"), effects: []),";
        assert!(NeedTable::parse(&plik(z_wlascicielem)).is_ok());

        // Potrzeba zdarzeniowa **z** miejscami reguły nie dotyczy: `Health` nie spada
        // z upływu czasu, ale przychodnia ją podnosi, więc ma się gdzie zaspokoić.
        let zdarzeniowa = "(key: \"Health\", visit_min: 45, decay_centi_per_hour: 0,                            critical: 30, satisfaction: 50, places: [Doctor], effects: []),";
        assert!(NeedTable::parse(&plik(zdarzeniowa)).is_ok());
    }

    #[test]
    fn nacisk_deprywacji_sumuje_sie_tylko_z_potrzeb_ponizej_progu() {
        // `R2-WP16`. Nacisk liczy się z danych, nie ze stałych w kodzie — a potrzeba
        // zaspokojona nie naciska wcale.
        let t = NeedTable::load_default().expect("data/needs/needs.ron");
        assert!(pressure(&Needs::default(), &t).is_none());

        let mut n = Needs::default();
        n.set(NeedKind::Hunger, Q::new(0));
        let glod = pressure(&n, &t);
        assert_eq!(
            glod.productivity_loss_permille,
            t.spec(NeedKind::Hunger)
                .effects
                .iter()
                .find(|e| e.effect == DeprivationEffect::ProductivityLoss)
                .map_or(0, |e| e.magnitude)
        );
        assert_eq!(glod.ambition_gain_permille, 0);

        n.set(NeedKind::Development, Q::new(0));
        assert!(pressure(&n, &t).ambition_gain_permille > 0);

        // Sumowanie, a nie maksimum: człowiek głodny i niewyspany naraz pracuje gorzej
        // niż głodny sam — gdyby nacisk brał maksimum, druga deprywacja byłaby darmowa.
        // Tabela jest tu **syntetyczna**, bo w dzisiejszych danych `ProductivityLoss`
        // niesie jedna potrzeba; testujemy regułę, a nie zawartość katalogu.
        let kara = |klucz: &str, ile: u32| {
            format!(
                "(key: \"{klucz}\", visit_min: 30, decay_centi_per_hour: 100, critical: 25, satisfaction: 40, places: [Home], effects: [(effect: ProductivityLoss, magnitude: {ile})]),"
            )
        };
        let dwie_kary = NeedTable::parse(&plik(&format!(
            "{}{}",
            kara("Hunger", 300),
            kara("Sleep", 250)
        )))
        .expect("tabela syntetyczna");
        let mut sam_glod = Needs::default();
        sam_glod.set(NeedKind::Hunger, Q::new(0));
        assert_eq!(
            pressure(&sam_glod, &dwie_kary).productivity_loss_permille,
            300
        );
        let mut dwie = sam_glod;
        dwie.set(NeedKind::Sleep, Q::new(0));
        assert_eq!(
            pressure(&dwie, &dwie_kary).productivity_loss_permille,
            550,
            "nacisk bierze maksimum zamiast sumy — druga deprywacja byłaby darmowa"
        );
    }

    #[test]
    fn deprywacja_daje_powod_dla_karty_inspekcji() {
        let t = NeedTable::load_default().expect("data/needs/needs.ron");
        let mut n = Needs::default();
        n.set(NeedKind::Hunger, Q::new(5));
        n.set(NeedKind::Leisure, Q::new(1));
        let mut out = Vec::new();
        deprivation_of(&n, &t, &mut out);
        assert_eq!(out.len(), 2, "powody: {out:?}");
        assert!(out.iter().any(|r| matches!(
            r,
            DecisionReason::Deprivation {
                need: NeedKind::Hunger,
                ..
            }
        )));

        // Potrzeby zaspokojone nie produkują powodów — karta inspekcji ma pokazywać
        // to, co boli, a nie wiersz na każdą potrzebę z napisem „w porządku".
        let mut pusto = Vec::new();
        deprivation_of(&Needs::default(), &t, &mut pusto);
        assert!(pusto.is_empty());
    }

    #[test]
    fn potrzeba_bez_sposobu_zaspokojenia_nie_spada() {
        // Korekta H-3 (M3d). Potrzeba, ktorej nic nie podnosi, a ktora spada, dochodzi
        // do zera u **wszystkich** mieszkancow — i wtedy jej skutki przestaja cokolwiek
        // roznicowac. Mobilnosc z `AbsenceRisk` 1000 zatrzymala w ten sposob cale miasto
        // w pracy w drugiej dobie przebiegu `m3day`. Tempo spadku wpisuje faza, ktora
        // wnosi mechanizm: M4 (trasa), M8 (przestepczosc), M5/M9 (lokal, konsumpcja).
        let t = NeedTable::load_default().expect("data/needs/needs.ron");
        for n in NeedKind::ALL {
            let s = t.spec(*n);
            if !s.places.is_empty() || s.satisfaction > 0 {
                continue;
            }
            assert_eq!(
                s.decay_centi_per_hour, 0,
                "potrzeba {n:?} spada ({} setnych/h), a nic jej nie podnosi",
                s.decay_centi_per_hour
            );
        }
    }
}
