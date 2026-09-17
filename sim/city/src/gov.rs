//! Władza miejska: kim jest burmistrz, co waży i jak mierzy poparcie (M8e WP9).
//!
//! **Sam wybór działania jest w [`crate::mayor`]** i to jest podział na dwa
//! tematy, a nie na dwa pliki: tutaj mieszka stan władzy (kto rządzi, z jakim
//! programem, z jakim poparciem i z jaką kalibracją), tam — reguła decyzji.
//! Stan czyta karta rady, wybory i raport; regułę wyłącznie krok miesięczny.
//!
//! **Histereza jest mechanizmem, nie ostrożnością.** Bez niej AI oscyluje: stawka
//! w górę, bo miesiąc wyszedł na minusie, w dół, bo następny na plusie, i tak co
//! trzydzieści dób. Trzy zabezpieczenia, każde na inną postać tej samej choroby:
//!
//! 1. **Nacisk musi się utrzymać** `hysteresis_months` miesięcy w tę samą stronę,
//!    zanim cokolwiek się stanie. Jeden zły miesiąc nie jest sygnałem.
//! 2. **Ta sama danina ma karencję** `min_months_between_changes` — miasto, które
//!    właśnie podniosło VAT, nie podnosi go znowu w kwartale.
//! 3. **Zwrot kierunku kosztuje** `min_months_for_reversal` miesięcy. To jest
//!    dosłowna treść testu T5 i dlatego nie jest progiem miękkim: zmiana kierunku
//!    przed upływem tego czasu **nie jest brana pod uwagę w menu**, a nie tylko
//!    nisko punktowana.
//!
//! Zero floatów, tak jak w całej fazie: sygnały w punktach bazowych, kwoty
//! w groszach, punktacja menu w `i64`.

use magnat_core::{
    AgencyKind, DistrictId, PolicyKind, StateHasher, TaxKind, Tick, AGENCY_KIND_COUNT,
    TAX_KIND_COUNT,
};
use serde::Deserialize;

use crate::policy::{Policy, PolicySet};

pub const GOVERNMENT_SCHEMA_VERSION: u32 = 1;

/// Cztery osie preferencji z PRD §10.1. Sumują się do 10 000 punktów bazowych.
///
/// To nie jest osobowość — to **wagi celów**. Burmistrz prorozwojowy i socjalny
/// widzą ten sam deficyt i wybierają inne działanie, bo inaczej ważą jego skutki.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub struct Preference {
    pub growth_bps: u32,
    pub social_bps: u32,
    pub green_bps: u32,
    pub populist_bps: u32,
}

impl Preference {
    /// Normalizuje do 10 000 bp. Reszta z dzielenia trafia do pierwszej osi —
    /// ta sama reguła co przy podziale kwoty między N stron (`00` §2).
    #[must_use]
    pub fn normalized(self) -> Preference {
        let suma = i64::from(self.growth_bps)
            + i64::from(self.social_bps)
            + i64::from(self.green_bps)
            + i64::from(self.populist_bps);
        if suma <= 0 {
            return Preference {
                growth_bps: 2_500,
                social_bps: 2_500,
                green_bps: 2_500,
                populist_bps: 2_500,
            };
        }
        let s = |v: u32| u32::try_from(i64::from(v) * 10_000 / suma).unwrap_or(0);
        let (g, so, gr, p) = (
            s(self.growth_bps),
            s(self.social_bps),
            s(self.green_bps),
            s(self.populist_bps),
        );
        Preference {
            growth_bps: g + (10_000 - g - so - gr - p),
            social_bps: so,
            green_bps: gr,
            populist_bps: p,
        }
    }

    /// Waga osi, po której punktuje się dane działanie.
    #[must_use]
    pub fn axis(&self, a: Axis) -> u32 {
        match a {
            Axis::Growth => self.growth_bps,
            Axis::Social => self.social_bps,
            Axis::Green => self.green_bps,
            Axis::Populist => self.populist_bps,
        }
    }

    /// Oś dominująca — po niej nazywa się burmistrza w raporcie.
    #[must_use]
    pub fn dominant(&self) -> Axis {
        let mut best = (Axis::Growth, self.growth_bps);
        for (a, v) in [
            (Axis::Social, self.social_bps),
            (Axis::Green, self.green_bps),
            (Axis::Populist, self.populist_bps),
        ] {
            if v > best.1 {
                best = (a, v);
            }
        }
        best.0
    }

    /// Odległość między programami w punktach bazowych (suma modułów różnic).
    /// Wyborca porównuje nią kandydata ze swoim interesem.
    #[must_use]
    pub fn distance_bp(&self, other: &Preference) -> u32 {
        self.growth_bps.abs_diff(other.growth_bps)
            + self.social_bps.abs_diff(other.social_bps)
            + self.green_bps.abs_diff(other.green_bps)
            + self.populist_bps.abs_diff(other.populist_bps)
    }
}

impl Default for Preference {
    fn default() -> Preference {
        Preference {
            growth_bps: 2_500,
            social_bps: 2_500,
            green_bps: 2_500,
            populist_bps: 2_500,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Axis {
    Growth,
    Social,
    Green,
    Populist,
}

impl Axis {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Axis::Growth => "prorozwojowy",
            Axis::Social => "socjalny",
            Axis::Green => "ekologiczny",
            Axis::Populist => "populistyczny",
        }
    }
}

/// Wagi celów wyprowadzone z preferencji: poparcie, saldo, rozwój.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Goals {
    pub approval_w: u32,
    pub balance_w: u32,
    pub growth_w: u32,
}

impl Goals {
    /// Populista waży poparcie, prorozwojowy rozwój, a saldo waży każdy —
    /// bo miasto bez pieniędzy nie realizuje żadnego programu.
    #[must_use]
    pub fn from_preference(p: &Preference) -> Goals {
        Goals {
            approval_w: 2_000 + p.populist_bps / 2,
            balance_w: 3_000 + p.growth_bps / 4,
            growth_w: 2_000 + p.growth_bps / 2,
        }
    }
}

/// Widełki stawki jednej daniny — sufit i podłoga uchwał.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct RateBand {
    pub kind: TaxKind,
    pub min_bp: u32,
    pub max_bp: u32,
}

/// Kalibracja władzy i wyborów (`data/city/government.ron`).
///
/// **W `data/city/`, a nie w `data/tuning/`** (`K-56`): długość kadencji i widełki
/// stawek zmieniają to, co gracz widzi i czego może się spodziewać po mieście —
/// są kształtem, a nie kalibracją. Progi histerezy stoją obok nich z rozmysłu:
/// rozdzielenie jednej mechaniki na dwa pliki kosztowałoby więcej niż daje.
#[derive(Clone, Debug, Deserialize)]
pub struct GovTuning {
    pub schema_version: u32,
    /// Kadencja w miesiącach (`K-1`: rok = 12 miesięcy po 30 dób).
    pub term_months: u32,
    /// Ile dób mija od uchwalenia do wejścia w życie.
    pub vacatio_legis_days: u16,
    /// Ile miesięcy nacisk musi się utrzymać, zanim burmistrz zareaguje.
    pub hysteresis_months: u8,
    /// Karencja tej samej daniny.
    pub min_months_between_changes: u8,
    /// Karencja zwrotu kierunku — dosłowna treść testu T5.
    pub min_months_for_reversal: u8,
    /// Krok zmiany stawki w punktach bazowych.
    pub tax_step_bp: u32,
    pub rate_bands: Vec<RateBand>,
    /// Cięcie planu, powyżej którego miasto uznaje miesiąc za deficytowy.
    pub deficit_cut_bp: u32,
    /// Zapas ponad rezerwę (w miesiącach planu), powyżej którego jest nadwyżka.
    pub surplus_months: u32,
    /// Zadłużenie do wpływów rocznych, powyżej którego nie rusza się wydatków w górę.
    pub debt_alarm_bp: u32,
    /// Minimalna punktacja działania, poniżej której burmistrz nic nie robi.
    pub action_threshold: i64,
    /// Wagi poparcia: usługi, obciążenie podatkowe, nastrój, bezrobocie.
    pub approval_w_services: u32,
    pub approval_w_tax: u32,
    pub approval_w_mood: u32,
    pub approval_w_unemployment: u32,
    /// Bezwładność poparcia: ile promili nowej wartości wchodzi co miesiąc.
    pub approval_inertia_bp: u32,
    /// Ilu kandydatów staje do wyborów.
    pub candidates: u8,
    /// Frekwencja: baza i wagi w punktach bazowych.
    pub turnout_base_bp: u32,
    pub turnout_status_bp: u32,
    pub turnout_age_bp: u32,
    pub turnout_mood_bp: u32,
    pub turnout_min_bp: u32,
    pub turnout_max_bp: u32,
    /// Ile mandatów ma rada.
    pub council_seats: u8,
    /// Temperatura wyboru kandydata: im wyżej, tym bardziej wyrównany rozkład.
    pub vote_temperature_bp: u32,
    /// Ile punktów bazowych użyteczności daje kandydatowi złotówka kampanii
    /// na jednego wyborcę dzielnicy. Wsparcie nielegalne liczy się mnożnikiem.
    pub media_reach_bp_per_zl: u32,
    pub illegal_reach_mul_bp: u32,
    /// Szum oferty przetargowej w punktach bazowych ceny.
    pub bid_noise_bp: u32,
    /// Udział planu wydatków na odbiór odpadów, który idzie do przetargu.
    pub tender_waste_bp: u32,
}

#[derive(Debug)]
pub enum GovError {
    Io(String),
    Parse(String),
    Schema { found: u32, want: u32 },
    Missing(&'static str),
}

impl std::fmt::Display for GovError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GovError::Io(e) | GovError::Parse(e) => write!(f, "city/government.ron: {e}"),
            GovError::Schema { found, want } => write!(
                f,
                "city/government.ron: schema_version {found}, oczekiwano {want}"
            ),
            GovError::Missing(w) => write!(f, "city/government.ron: brak widełek dla `{w}`"),
        }
    }
}

impl std::error::Error for GovError {}

impl GovTuning {
    pub fn load(path: &std::path::Path) -> Result<GovTuning, GovError> {
        let tekst = std::fs::read_to_string(path).map_err(|e| GovError::Io(e.to_string()))?;
        let t: GovTuning = ron::from_str(&tekst).map_err(|e| GovError::Parse(e.to_string()))?;
        if t.schema_version != GOVERNMENT_SCHEMA_VERSION {
            return Err(GovError::Schema {
                found: t.schema_version,
                want: GOVERNMENT_SCHEMA_VERSION,
            });
        }
        // Kompletność przed użyciem: danina bez widełek dałaby stawkę przyciętą
        // do zera, czyli daninę cicho zniesioną uchwałą, której nikt nie podjął.
        for k in TaxKind::ALL {
            if !t.rate_bands.iter().any(|b| b.kind == *k) {
                return Err(GovError::Missing(k.name()));
            }
        }
        Ok(t)
    }

    pub fn load_default() -> Result<GovTuning, GovError> {
        GovTuning::load(&magnat_core::data_path("city/government.ron"))
    }

    #[must_use]
    pub fn band(&self, kind: TaxKind) -> (u32, u32) {
        self.rate_bands
            .iter()
            .find(|b| b.kind == kind)
            .map_or((0, 10_000), |b| (b.min_bp, b.max_bp))
    }
}

/// Sygnały, na które patrzy burmistrz. Wszystkie w punktach bazowych i wszystkie
/// **zmierzone**, żeby karta rady mogła pokazać liczbę obok decyzji.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Signals {
    /// Ujemny = deficyt (cięcie planu), dodatni = nadwyżka ponad rezerwę.
    pub fiscal_bp: i32,
    /// Zadłużenie do wpływów rocznych.
    pub debt_bp: u32,
    /// Średnie poparcie dla władzy.
    pub approval_bp: u32,
    /// Luka w pokryciu usługami (10 000 = miasto bez usług).
    pub service_gap_bp: u32,
    /// Indeks rodzaju usługi o **najgorszym** pokryciu — ten, który burmistrz
    /// dofinansuje, gdy zdecyduje się dofinansować cokolwiek.
    ///
    /// Osobne pole, a nie wyliczenie z luki: pierwsza wersja brała
    /// `ServiceKind::ALL[service_gap_bp as u8 % 8]`, czyli wybierała rodzaj usługi
    /// po reszcie z dzielenia liczby, która z nim nie ma nic wspólnego. Komentarz
    /// mówił „tam, gdzie pokrycie najgorsze", kod robił coś innego i nic tego nie
    /// łapało, bo obie liczby są indeksami i obie mieszczą się w zakresie.
    pub worst_service: u8,
    pub unemployment_permille: u32,
    /// Najbrudniejszy zakład wobec obowiązującego limitu emisji.
    pub emission_bp: u32,
    /// Udział zakładów w szarej strefie.
    pub shadow_bp: u32,
}

/// Stan władzy miejskiej.
#[derive(Clone, Debug)]
pub struct Government {
    /// Indeks encji mieszkańca-burmistrza; `NO_MAYOR` = wakat (świat bez wyborów).
    pub mayor: u32,
    pub mayor_pref: Preference,
    /// Mandaty: mieszkaniec i jego program.
    pub council: Vec<(u32, Preference)>,
    pub term_start: Tick,
    pub term_ticks: u64,
    /// Poparcie per dzielnica, aktualizowane co miesiąc z bezwładnością.
    pub approval_bps_by_district: Vec<u32>,
    pub goals: Goals,
    pub tuning: Option<GovTuning>,
    /// Ostatnie zmierzone sygnały — treść zakładki „Dlaczego" karty rady.
    pub signals: Signals,
    /// Absolutny numer miesiąca świata; rośnie w kroku miesięcznym.
    pub month: u32,
    /// Ile miesięcy z rzędu nacisk fiskalny ma ten sam znak.
    ///
    /// `pub(crate)`, bo pisze go [`crate::mayor::decide_month`] — rejestr histerezy
    /// jest stanem władzy, a reguła, która go rusza, mieszka obok. Poza crate'em
    /// nikt go nie widzi i widzieć nie ma po co.
    pub(crate) fiscal_streak: i16,
    /// Kierunek ostatniej zmiany stawki: −1, 0, +1.
    pub(crate) tax_last_dir: [i8; TAX_KIND_COUNT],
    /// Miesiąc ostatniej zmiany stawki; `u32::MAX` = nigdy.
    pub(crate) tax_last_month: [u32; TAX_KIND_COUNT],
    /// Ile uchwał każdego rodzaju zapadło — histogram karty rady.
    pub enacted: [u32; magnat_core::POLICY_KIND_COUNT],
}

impl Government {
    pub const NO_MAYOR: u32 = u32::MAX;

    #[must_use]
    pub fn new(pref: Preference, districts: usize, tuning: Option<GovTuning>) -> Government {
        let pref = pref.normalized();
        let term = tuning.as_ref().map_or(48, |t| t.term_months);
        Government {
            mayor: Government::NO_MAYOR,
            mayor_pref: pref,
            council: Vec::new(),
            term_start: Tick(0),
            term_ticks: u64::from(term) * 43_200,
            approval_bps_by_district: vec![5_000; districts],
            goals: Goals::from_preference(&pref),
            tuning,
            signals: Signals::default(),
            month: 0,
            fiscal_streak: 0,
            tax_last_dir: [0; TAX_KIND_COUNT],
            tax_last_month: [u32::MAX; TAX_KIND_COUNT],
            enacted: [0; magnat_core::POLICY_KIND_COUNT],
        }
    }

    /// Czy władza w ogóle działa. Świat bez wczytanej kalibracji nie rządzi —
    /// i to jest poprawny stan scenariusza sprzed M8e, nie awaria.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.tuning.is_some()
    }

    #[must_use]
    pub fn approval_mean_bp(&self) -> u32 {
        if self.approval_bps_by_district.is_empty() {
            return 5_000;
        }
        let s: u64 = self
            .approval_bps_by_district
            .iter()
            .map(|v| u64::from(*v))
            .sum();
        u32::try_from(s / self.approval_bps_by_district.len() as u64).unwrap_or(5_000)
    }

    #[must_use]
    pub fn approval_of(&self, d: DistrictId) -> u32 {
        self.approval_bps_by_district
            .get(usize::from(d.0))
            .copied()
            .unwrap_or(5_000)
    }

    /// Czy wolno dziś ruszyć stawkę tej daniny w tym kierunku (`+1` w górę).
    ///
    /// Publiczne, bo **drugim wołającym jest obietnica wyborcza**: nowy burmistrz
    /// wchodzi z programem, ale trzy karencje obowiązują jego tak samo jak
    /// poprzednika. Bez tego test T5 pękałby raz na kadencję — stawka podniesiona
    /// w czterdziestym szóstym miesiącu i obcięta w czterdziestym ósmym to zwrot
    /// kierunku po dwóch miesiącach, choć żadna z tych decyzji z osobna reguły
    /// nie łamała.
    #[must_use]
    pub fn may_move_rate(&self, kind: TaxKind, dir: i8) -> bool {
        let Some(tun) = self.tuning.as_ref() else {
            return false;
        };
        crate::mayor::wolno_ruszyc(self, tun, kind.as_index(), dir)
    }

    /// Odnotowuje zmianę stawki w rejestrze histerezy. Woła się **zawsze**, gdy
    /// stawka faktycznie drgnęła — niezależnie od tego, czy ruszył ją burmistrz,
    /// czy wynik wyborów.
    pub fn record_rate_change(&mut self, kind: TaxKind, dir: i8) {
        let i = kind.as_index();
        self.tax_last_dir[i] = dir;
        self.tax_last_month[i] = self.month;
    }

    /// Czy kadencja dobiegła końca w chwili `t`.
    #[must_use]
    pub fn term_over(&self, t: Tick) -> bool {
        self.term_ticks > 0 && t.0 >= self.term_start.0 + self.term_ticks
    }

    pub fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.mayor);
        h.write_u32(self.mayor_pref.growth_bps);
        h.write_u32(self.mayor_pref.social_bps);
        h.write_u32(self.mayor_pref.green_bps);
        h.write_u32(self.mayor_pref.populist_bps);
        h.write_u64(self.council.len() as u64);
        for (c, p) in &self.council {
            h.write_u32(*c);
            // Wszystkie cztery osie, nie dwie: rada głosuje osią uchwały, więc
            // dwie rady różniące się wyłącznie zielonym i populistycznym
            // uchwalałyby co innego, a hasz mówiłby, że są tym samym.
            h.write_u32(p.growth_bps);
            h.write_u32(p.social_bps);
            h.write_u32(p.green_bps);
            h.write_u32(p.populist_bps);
        }
        h.write_u64(self.term_start.0);
        h.write_u64(self.term_ticks);
        for a in &self.approval_bps_by_district {
            h.write_u32(*a);
        }
        h.write_u32(self.month);
        h.write_u16(self.fiscal_streak as u16);
        for d in &self.tax_last_dir {
            h.write_u8(*d as u8);
        }
        for m in &self.tax_last_month {
            h.write_u32(*m);
        }
        for e in &self.enacted {
            h.write_u32(*e);
        }
    }
}

/// Wejście comiesięcznego poparcia: pokrycie usługami per dzielnica plus
/// trzy liczby globalne. Osobna struktura, bo krok miesięczny nie ma jak
/// trzymać `&City` i `&mut Government` naraz — zasób jest wyjęty ze świata.
#[derive(Clone, Copy, Debug, Default)]
pub struct ApprovalInput {
    /// Średnie pokrycie usługami w dzielnicy, 0..=100.
    pub coverage_q: u32,
    /// Łączne obciążenie podatkowe miasta w bp (suma stawek VAT, CIT, PIT).
    pub tax_burden_bp: u32,
    /// Nastrój mieszkańców, −100..=100.
    pub mood: i32,
    pub unemployment_permille: u32,
}

/// Aktualizacja poparcia w jednej dzielnicy (`EveryMonth`).
///
/// Bezwładność jest tu warunkiem sensu, nie wygładzaniem: poparcie skaczące
/// z miesiąca na miesiąc czyniłoby wybory loterią pogodową, a §1 dokumentu fazy
/// obiecuje, że spadek jakości usług **przez kadencję** przekłada się na wynik.
#[must_use]
pub fn approval_step(prev_bp: u32, inp: ApprovalInput, t: &GovTuning) -> u32 {
    let mut cel: i64 = 5_000;
    // Usługi: pokrycie 50 to punkt neutralny, 100 daje pełną wagę na plus.
    cel += i64::from(t.approval_w_services) * (i64::from(inp.coverage_q) - 50) / 50;
    // Podatki: 6000 bp łącznego obciążenia to punkt neutralny epoki.
    cel -= i64::from(t.approval_w_tax) * (i64::from(inp.tax_burden_bp) - 6_000) / 6_000;
    cel += i64::from(t.approval_w_mood) * i64::from(inp.mood) / 100;
    cel -= i64::from(t.approval_w_unemployment) * i64::from(inp.unemployment_permille) / 100;
    let cel = cel.clamp(0, 10_000);
    let bezwl = i64::from(t.approval_inertia_bp.min(10_000));
    let nowe = (i64::from(prev_bp) * (10_000 - bezwl) + cel * bezwl) / 10_000;
    u32::try_from(nowe.clamp(0, 10_000)).unwrap_or(5_000)
}

/// Łączne obciążenie podatkowe do sygnału poparcia: trzy daniny, które płacą
/// wszyscy. Akcyza i cło nie wchodzą, bo dotykają wąskich grup towarów, a podatek
/// od nieruchomości płaci zakład, nie mieszkaniec.
#[must_use]
pub fn tax_burden_bp(rates: &[u32; TAX_KIND_COUNT]) -> u32 {
    rates[TaxKind::Vat.as_index()] + rates[TaxKind::Cit.as_index()] + rates[TaxKind::Pit.as_index()]
}

/// Obsada urzędów wynikająca z obowiązujących uchwał (`CH-6`).
#[must_use]
pub fn agency_staffing(policies: &PolicySet, t: Tick) -> [Option<u32>; AGENCY_KIND_COUNT] {
    let mut out = [None; AGENCY_KIND_COUNT];
    for a in AgencyKind::ALL {
        if let Some(Policy::AgencyStaffing { inspectors, .. }) =
            policies.current(PolicyKind::AgencyStaffing, a.as_index() as u32, t)
        {
            out[a.as_index()] = Some(*inspectors);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tuning() -> GovTuning {
        GovTuning::load_default().expect("data/city/government.ron")
    }

    #[test]
    fn plik_domyslny_sie_laduje_i_ma_widelki_kazdej_daniny() {
        let t = tuning();
        for k in TaxKind::ALL {
            let (a, b) = t.band(*k);
            assert!(a <= b, "{}", k.name());
        }
        assert!(t.min_months_for_reversal >= 24, "T5 wymaga 24 miesięcy");
    }

    #[test]
    fn preferencja_normalizuje_sie_do_dziesieciu_tysiecy() {
        let p = Preference {
            growth_bps: 7,
            social_bps: 1,
            green_bps: 1,
            populist_bps: 1,
        }
        .normalized();
        assert_eq!(
            p.growth_bps + p.social_bps + p.green_bps + p.populist_bps,
            10_000
        );
        assert_eq!(p.dominant(), Axis::Growth);
    }

    #[test]
    fn poparcie_reaguje_na_uslugi_i_ma_bezwladnosc() {
        let t = tuning();
        let dobre = ApprovalInput {
            coverage_q: 90,
            tax_burden_bp: 6_000,
            mood: 0,
            unemployment_permille: 50,
        };
        let zle = ApprovalInput {
            coverage_q: 20,
            ..dobre
        };
        let a = approval_step(5_000, dobre, &t);
        let b = approval_step(5_000, zle, &t);
        assert!(
            a > b,
            "lepsze usługi mają dawać wyższe poparcie: {a} vs {b}"
        );
        // Bezwładność: jeden miesiąc nie przenosi poparcia na cel.
        assert!(a < 9_000, "poparcie przeskoczyło na cel w jednym miesiącu");
    }
}
