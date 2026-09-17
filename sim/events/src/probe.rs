//! Sondy — **jedyne** wejście generatora zdarzeń do świata (M8c §5.5).
//!
//! Trzy reguły, wszystkie wynikające z 00 §3 i z ryzyka `R9` fazy:
//!
//! 1. **Tylko do odczytu.** Sonda nie zmienia ani jednego bajtu stanu. Gdyby
//!    zmieniała, kolejność oceny definicji stałaby się częścią wyniku.
//! 2. **Wyłącznie `i64`.** Ani jednego floata na ścieżce hazardu — to jest wymóg
//!    silniejszy niż `K-6` i domyka test T3 („żaden typ zmiennoprzecinkowy
//!    w module hazardu").
//! 3. **Cache per (sonda, instancja zakresu) na tick.** Sonda `UnemploymentPermille`
//!    używana przez dwanaście definicji liczy się raz. Cache jest `BTreeMap`,
//!    nie `HashMap` — 00 §3.2.
//!
//! Czego tu **nie ma i dlaczego**: sondy z §5.5, których dzisiejszy świat nie umie
//! policzyć, nie powstały jako warianty zwracające zero. Wariant, który zawsze
//! oddaje tę samą liczbę, przechodzi każdy test i w katalogu wygląda tak samo jak
//! sonda działająca (`R2`) — a krzywa nad nim jest wtedy ozdobą. Lista nieobecnych
//! i ich adresów jest w tabeli korekt dokumentu `M8c`.

use magnat_core::SiteId;
use serde::Deserialize;
use std::collections::BTreeMap;

/// Co sonda mierzy. Sondy zakresowe (`Site*`, `Firm*`) pytają o **instancję
/// zakresu, dla której liczy się hazard** — dlatego nie noszą identyfikatora
/// w wariancie: definicja `social/strike` o zakresie `Firm` pyta o „tę firmę",
/// a nie o firmę wypisaną w danych.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Deserialize)]
pub enum Probe {
    // ── pogoda i kalendarz (WP6) ────────────────────────────────────────────
    /// Temperatura powietrza w dziesiątych °C.
    AirTempDc,
    /// Niedobór opadu z ostatnich 30 dób wobec normy klimatycznej, w setnych mm.
    /// Dodatni znaczy „mniej, niż powinno spaść".
    PrecipDeficit30dChm,
    /// Niedobór opadu z ostatnich 90 dób — susza sezonowa, nie tygodniowa.
    PrecipDeficit90dChm,
    /// Pokrywa śnieżna w milimetrach słupa wody.
    SnowCoverMm,
    /// Prędkość wiatru w km/h.
    WindKmh,
    /// Stopniodoby grzewcze doby: `max(0, 15 °C − temperatura)` w dziesiątych.
    HeatingDegreeDc,
    /// Pora roku jako indeks 0..=3 (`Season::as_index`).
    SeasonIndex,
    // ── sieci przesyłowe (M8b) ──────────────────────────────────────────────
    /// Obciążenie sieci w punktach bazowych: popyt / moc osiągalna.
    GridLoadFactorBps,
    /// Zapas mocy sieci w punktach bazowych; ujemny znaczy deficyt.
    GridReserveMarginBps,
    /// Moc, której odbiorcy nie dostali w ostatnim kroku.
    GridUnserved,
    /// Wiek najstarszej linii zakładu prowadzącego źródło, w dobach.
    SourceAgeDays,
    /// Zaległość konserwacyjna źródła w dobach; ujemna znaczy „przed terminem".
    SourceMaintenanceOverdueDays,
    /// Kondycja najgorszej linii źródła, 0..=100.
    SourceConditionQ,
    // ── zakład (M6) ─────────────────────────────────────────────────────────
    /// Kondycja najgorszej linii zakładu, 0..=100.
    SiteConditionQ,
    /// Zaległość konserwacyjna zakładu w dobach.
    SiteMaintenanceOverdueDays,
    /// Pokrycie etatowe zakładu w promilach.
    SiteLaborPct,
    // ── firma (M7) ──────────────────────────────────────────────────────────
    /// Liczba zatrudnionych we wszystkich zakładach firmy.
    FirmHeadcount,
    /// Wiek firmy w dobach.
    FirmAgeDays,
    /// Średnie morale załogi firmy, 0..=100.
    FirmMoraleQ,
    /// Luka płacowa: o ile promili płaca firmy odstaje **w dół** od mediany
    /// przyjętej w zawodzie i dzielnicy. Zero znaczy „płaci jak rynek".
    FirmWageGapPermille,
    /// Udział obrotu firmy **poza deklaracją**, w punktach bazowych (M8d WP8).
    ///
    /// Największy z udziałów jej zakładów, a nie średnia: kontrola skarbowa
    /// przychodzi po firmie, w której coś nie gra, a nie po firmie, której średnia
    /// wygląda spokojnie. Zero znaczy „deklaruje wszystko".
    FirmUnreportedBps,
    // ── miasto (WP6) ────────────────────────────────────────────────────────
    /// Stopa bezrobocia w promilach.
    UnemploymentPermille,
    /// Inflacja rok do roku w punktach bazowych.
    CpiYoyBp,
    /// Średni nastrój mieszkańców, −100..=100.
    MoodMean,
}

impl Probe {
    /// Etykieta sondy w kluczu cache i w inspektorze. Kolejność wariantów nie jest
    /// kontraktem zapisu — sonda nie wchodzi ani do zapisu gry, ani do hasha stanu,
    /// bo jest **funkcją** stanu, nie jego częścią.
    #[must_use]
    pub const fn tag(self) -> u16 {
        self as u16
    }
}

/// Wartości sond w tym ticku, liczone leniwie i pamiętane.
///
/// `scope_key` to identyfikator instancji zakresu spakowany w `u64` — zakład,
/// firma, dzielnica albo medium. Dla sond niezależnych od zakresu jest zerem,
/// więc miejska stopa bezrobocia liczy się raz na wszystkie definicje.
#[derive(Default)]
pub struct ProbeCache {
    values: BTreeMap<(u16, u64), i64>,
    /// Ile razy odpowiedziano z pamięci — do raportu wydajności (T8c).
    pub hits: u32,
    pub misses: u32,
}

impl ProbeCache {
    pub fn clear(&mut self) {
        self.values.clear();
    }

    /// Zapamiętana wartość albo `None`.
    pub fn peek(&mut self, p: Probe, scope_key: u64) -> Option<i64> {
        let v = self.values.get(&(p.tag(), scope_key)).copied();
        if v.is_some() {
            self.hits += 1;
        } else {
            self.misses += 1;
        }
        v
    }

    /// Zapamiętuje policzoną wartość.
    pub fn put(&mut self, p: Probe, scope_key: u64, v: i64) {
        self.values.insert((p.tag(), scope_key), v);
    }
}

/// Zakład w przestrzeni generatora zdarzeń — wycinek, który most ustala **raz**,
/// przy stawianiu świata.
///
/// Dzielnica i rodzaj zakładu nie zmieniają się w trakcie gry, a policzenie ich
/// wymagałoby parcel, stref i katalogu receptur, czyli `sim/world` i całego
/// generatora miasta. Trzymanie ich tutaj jest tańsze **i** zdejmuje z tego crate'u
/// zależność od `sim/world` — ta sama droga, którą `AgentSources` zdejmuje ją z M3.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SiteRef {
    pub site: SiteId,
    pub district: u16,
    pub kind: SiteClass,
}

/// Do czego zakład służy — tyle, ile potrzebuje filtr celu zdarzenia.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub enum SiteClass {
    /// Pole, sad, hodowla — zakład o recepturze z gałęzi rolnej.
    Farm,
    /// Zakład przetwórczy albo wydobywczy.
    Industry,
    /// Sklep, usługa, biuro.
    Service,
}

/// Filtr celu efektu. `Any` znaczy „każdy zakład w zakresie".
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub enum SiteFilter {
    Any,
    Farm,
    Industry,
    Service,
}

impl SiteFilter {
    #[must_use]
    pub fn accepts(self, k: SiteClass) -> bool {
        match self {
            SiteFilter::Any => true,
            SiteFilter::Farm => k == SiteClass::Farm,
            SiteFilter::Industry => k == SiteClass::Industry,
            SiteFilter::Service => k == SiteClass::Service,
        }
    }
}
