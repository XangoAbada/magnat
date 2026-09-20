//! Ubezpieczenia: polisa, szkodowość, składka (M10d WP10.12, PRD §6.5, §11).
//!
//! # Skąd bierze się składka — i czego w tym module nie ma
//!
//! Kryterium WP10.12 jest sformułowane przez **negację**: składka ma wyjść wyższa
//! tam, gdzie ryzyko jest większe, **bez żadnego parametru „ryzyko" w danych**.
//! Dlatego w `data/tuning/insurance.ron` nie ma i nie będzie ani jednej liczby
//! mówiącej, jak groźna jest dzielnica. Są tam wyłącznie liczby o **zakładzie
//! ubezpieczeń**: wiarygodność własnej statystyki, narzut, opłata i okno obserwacji.
//! Ryzyko wychodzi z [`PerilStats`] — z tego, co się w tej dzielnicy naprawdę stało.
//!
//! ```text
//! własna_stawka = loss_total / sum_total                       (szkodowość w bps)
//! stawka = (n × własna + K × prior_miejski) / (n + K)           (Bühlmann-lite)
//! składka = stawka × suma_ubezpieczenia × (1 + narzut) / 12 + opłata
//! ```
//!
//! Wygładzanie jest **konieczne, a nie ostrożne**: bez niego ubezpieczyciel po
//! pierwszej szkodzie wycenia składkę na poziomie sumy ubezpieczenia i wypada z rynku
//! w jednym kroku, a przy zerowej historii sprzedaje polisę za darmo.
//!
//! # Czego w grze nie było i dlatego to tutaj powstaje (`GD-4`)
//!
//! Plan fazy pisał „M8 (`sim/events` jako źródło szkód)". **Takiego źródła nie było.**
//! Zdarzenie do M10d zmieniało wyłącznie parametry, a `ParamOverlay` przywracał je
//! przy wygaśnięciu — sześć wariantów `Effect`, wszystkie odwracalne. Szkoda majątkowa
//! jest jednorazowa i nieodwracalna, więc siódmym wariantem `Effect` być nie może:
//! złamałaby kontrakt nakładki, która z definicji umie wszystko cofnąć.
//!
//! Rozstrzygnięcie: definicja zdarzenia dostaje pole `peril`, a rejestr zdarzeń
//! **zgłasza** wystąpienie do [`Insurers::report_peril`]. Kierunek jest wymuszony
//! grafem i jest ten sam, którym `sim/events` nakłada parametry: piszący stoi nad
//! czytelnikiem i sięga do jego pola (`K-64`).
//!
//! # Czego tu nie ma
//!
//! **Rynku reasekuracji.** Ubezpieczyciel, któremu zabraknie na wypłatę, płaci tyle,
//! ile ma, i idzie ścieżką niewypłacalności M7d jak każda inna firma — a nie cedują
//! ryzyka na drugiego ubezpieczyciela. `ponytail:` sufit nazwany; reasekuracja ma sens
//! dopiero wtedy, gdy gracz może założyć reasekuratora, i wtedy jest własnym pakietem.

pub mod data;
pub mod system;

use std::collections::BTreeMap;

use magnat_core::{
    CoverId, DistrictId, HashState, Money, PerilKind, SimMinute, SiteId, StateHasher,
    PERIL_KIND_COUNT,
};
use magnat_firms::FirmKey;

pub use data::{InsuranceData, InsuranceError, INSURANCE_SCHEMA_VERSION};

/// Polisa.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cover {
    pub id: CoverId,
    pub insurer: FirmKey,
    /// Ubezpieczony zakład. Polisy wystawia się **zakładowi**, a nie firmie: pożar
    /// niszczy magazyn w jednej dzielnicy, a nie przedsiębiorstwo w trzech.
    pub site: SiteId,
    pub district: DistrictId,
    pub peril: PerilKind,
    pub sum_insured: Money,
    pub deductible: Money,
    pub premium_monthly: Money,
    /// Stawka roczna od sumy ubezpieczenia — ta liczba jest całą wiedzą
    /// ubezpieczyciela o ryzyku i **nie pochodzi z żadnego pliku**.
    pub rate_bp: u16,
    pub from: SimMinute,
    pub until: SimMinute,
    pub ended: bool,
}

impl Cover {
    #[must_use]
    pub fn is_active(&self, t: SimMinute) -> bool {
        !self.ended && self.from.0 <= t.0 && t.0 < self.until.0
    }

    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.id.0);
        h.write_u64(self.insurer.0);
        h.write_u32(self.site.0.index());
        h.write_u16(self.district.0);
        h.write_u8(self.peril.as_index() as u8);
        self.sum_insured.hash_state(h);
        self.deductible.hash_state(h);
        self.premium_monthly.hash_state(h);
        h.write_u16(self.rate_bp);
        h.write_u64(self.from.0);
        h.write_u64(self.until.0);
        h.write_u8(u8::from(self.ended));
    }
}

/// Szkodowość jednego ryzyka w jednym zakresie, w oknie obserwacji.
///
/// `exposures` liczy **polisomiesiące**, a nie polisy: ubezpieczyciel z jedną polisą
/// przez pięć lat wie o ryzyku tyle samo, co z sześćdziesięcioma przez miesiąc,
/// i to jest cała różnica między wiarygodnością a liczbą klientów.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct PerilStats {
    pub exposures: u32,
    pub claims: u32,
    pub loss_total: Money,
    pub sum_total: Money,
}

impl PerilStats {
    /// Szkodowość w punktach bazowych sumy ubezpieczenia.
    ///
    /// `None` znaczy „nie wiem", a nie „zero": ubezpieczyciel bez ekspozycji nie ma
    /// prawa twierdzić, że ryzyka nie ma — i dlatego składka liczy się wtedy
    /// z samego priora.
    #[must_use]
    pub fn loss_ratio_bp(&self) -> Option<u32> {
        if self.sum_total.get() <= 0 {
            return None;
        }
        let bp = self.loss_total.get().max(0) as i128 * 10_000 / i128::from(self.sum_total.get());
        Some(bp.clamp(0, i128::from(u32::MAX)) as u32)
    }

    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.exposures);
        h.write_u32(self.claims);
        self.loss_total.hash_state(h);
        self.sum_total.hash_state(h);
    }
}

/// Zgłoszenie wystąpienia ryzyka, wrzucone przez `sim/events`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PerilOccurrence {
    pub peril: PerilKind,
    /// `None` znaczy „całe miasto" — zakres `World` katalogu zdarzeń.
    pub district: Option<DistrictId>,
    /// `Some` dla zakresu `Site`: ucierpiał ten jeden zakład, a nie dzielnica.
    pub site: Option<SiteId>,
    pub severity_bps: u16,
    pub day: u32,
}

/// Stawka Bühlmanna: własna szkodowość zmieszana z priorem wg wiarygodności.
///
/// `n` to liczba polisomiesięcy, `k` — stała wiarygodności (ile ekspozycji trzeba,
/// żeby własna statystyka ważyła tyle co prior). Przy `n = 0` wychodzi czysty prior,
/// przy `n = k` — średnia arytmetyczna, przy `n → ∞` — własna szkodowość.
#[must_use]
pub fn credibility_rate_bp(own_bp: Option<u32>, prior_bp: u32, n: u32, k: u32) -> u32 {
    let Some(own) = own_bp else {
        return prior_bp;
    };
    let k = k.max(1);
    let licznik = u64::from(n) * u64::from(own) + u64::from(k) * u64::from(prior_bp);
    let mianownik = u64::from(n) + u64::from(k);
    (licznik / mianownik.max(1)).min(u64::from(u32::MAX)) as u32
}

/// Miesięczna składka ze stawki rocznej, narzutu i opłaty stałej.
#[must_use]
pub fn premium(sum_insured: Money, rate_bp: u32, loading_bp: u16, fee: Money) -> Money {
    if sum_insured.get() <= 0 {
        return Money::ZERO;
    }
    let roczna = sum_insured.get() as i128 * i128::from(rate_bp) / 10_000;
    let z_narzutem = roczna * i128::from(10_000 + u32::from(loading_bp)) / 10_000;
    // Rok gry ma dwanaście miesięcy po 30 dób (`K-1`), więc dzielenie przez 12
    // jest dokładne i nie ma konwencji dziennej.
    let miesieczna = (z_narzutem / 12).clamp(0, i128::from(i64::MAX)) as i64;
    Money(miesieczna.saturating_add(fee.get().max(0)))
}

/// Rejestr ubezpieczeń miasta.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Insurers {
    params: InsuranceData,
    covers: Vec<Cover>,
    next_cover: u32,
    /// Szkodowość per `(ryzyko, dzielnica)`. `BTreeMap`, bo kolejność iteracji
    /// wyznacza kolejność wyceny polis w dobie (00 §3.2).
    by_district: BTreeMap<(u8, u16), PerilStats>,
    /// Prior miejski: ta sama statystyka zebrana po całym mieście.
    by_city: [PerilStats; PERIL_KIND_COUNT],
    /// Zgłoszenia od `sim/events` czekające na realizację. Wchodzą do hasha stanu,
    /// bo między otwarciem zdarzenia a krokiem ubezpieczeń są jedynym śladem po
    /// szkodzie — ta sama zasada, co przy `EdgeWatch` (`K-81`).
    pending: Vec<PerilOccurrence>,
}

impl Insurers {
    #[must_use]
    pub fn new(params: InsuranceData) -> Insurers {
        Insurers {
            params,
            ..Insurers::default()
        }
    }

    #[must_use]
    pub fn params(&self) -> &InsuranceData {
        &self.params
    }

    #[must_use]
    pub fn cover_count(&self) -> usize {
        self.covers.iter().filter(|c| !c.ended).count()
    }

    #[must_use]
    pub fn cover(&self, id: CoverId) -> Option<&Cover> {
        self.covers.get(id.0 as usize)
    }

    /// Polisa zakładu na to ryzyko, jeśli jest czynna.
    #[must_use]
    pub fn cover_of(&self, site: SiteId, peril: PerilKind, t: SimMinute) -> Option<&Cover> {
        self.covers
            .iter()
            .find(|c| c.site == site && c.peril == peril && c.is_active(t))
    }

    pub fn covers(&self) -> impl Iterator<Item = &Cover> {
        self.covers.iter()
    }

    #[must_use]
    pub fn stats(&self, peril: PerilKind, district: DistrictId) -> PerilStats {
        self.by_district
            .get(&(peril.as_index() as u8, district.0))
            .copied()
            .unwrap_or_default()
    }

    #[must_use]
    pub fn city_stats(&self, peril: PerilKind) -> PerilStats {
        self.by_city[peril.as_index()]
    }

    /// Stawka, po jakiej ubezpieczyciel wystawiłby dziś polisę na to ryzyko w tej
    /// dzielnicy. **Jedyne** miejsce, w którym cena ryzyka powstaje.
    #[must_use]
    pub fn rate_bp(&self, peril: PerilKind, district: DistrictId) -> u32 {
        let d = self.stats(peril, district);
        let m = self.city_stats(peril);
        let prior = m
            .loss_ratio_bp()
            .unwrap_or(u32::from(self.params.prior_rate_bp));
        credibility_rate_bp(
            d.loss_ratio_bp(),
            prior,
            d.exposures,
            self.params.credibility_k,
        )
    }

    /// Zgłoszenie od `sim/events`. Jedyne wejście szkody do gospodarki.
    pub fn report_peril(&mut self, o: PerilOccurrence) {
        self.pending.push(o);
    }

    #[must_use]
    pub fn pending(&self) -> &[PerilOccurrence] {
        &self.pending
    }

    pub(crate) fn take_pending(&mut self) -> Vec<PerilOccurrence> {
        std::mem::take(&mut self.pending)
    }

    /// Wystawia polisę. `pub`, bo wołają to trzy strony: miesięczny krok systemu,
    /// scenariusz headless i komenda gracza (M10f) — a każda z nich podaje już
    /// policzoną stawkę.
    pub fn push_cover(&mut self, mut c: Cover) -> CoverId {
        // **Odnowienie zamiast nowej pozycji.** Polisa jest roczna, więc zakład, który
        // ubezpiecza się przez sto lat, dorobiłby się stu wpisów — w całości w hashu
        // stanu i w całości przeglądanych liniowo przy każdej szkodzie. Jeden wpis
        // na parę (zakład, ryzyko) trzyma rejestr ograniczony liczbą zakładów razy
        // dwa ryzyka. Historia szkód mieszka w [`PerilStats`], nie w polisach.
        if let Some(stara) = self
            .covers
            .iter_mut()
            .find(|x| x.site == c.site && x.peril == c.peril)
        {
            let id = stara.id;
            *stara = Cover { id, ..c };
            return id;
        }
        let id = CoverId(self.next_cover);
        self.next_cover += 1;
        c.id = id;
        self.covers.push(c);
        id
    }

    /// Dopisuje polisomiesiąc do szkodowości. `pub` z tego samego powodu co
    /// [`Books::endow`](crate::Books::endow): kanał musi dać się otworzyć z zewnątrz,
    /// żeby scenariusz mógł postawić świat z historią, a nie z zerem.
    pub fn note_exposure(&mut self, peril: PerilKind, district: DistrictId, sum: Money) {
        let w = self
            .by_district
            .entry((peril.as_index() as u8, district.0))
            .or_default();
        w.exposures += 1;
        w.sum_total = Money(w.sum_total.get().saturating_add(sum.get()));
        let m = &mut self.by_city[peril.as_index()];
        m.exposures += 1;
        m.sum_total = Money(m.sum_total.get().saturating_add(sum.get()));
    }

    /// Postarza szkodowość o jeden miesiąc.
    ///
    /// **Okno obserwacji jest wykładnicze, nie prostokątne**, i to jest decyzja:
    /// każdy miesiąc mnoży całą statystykę przez `1 − 1/window_months`, więc średni
    /// wiek obserwacji równa się `window_months` — pięć lat gry przy dzisiejszej
    /// kalibracji. Okno prostokątne wymagałoby sześćdziesięciu kubełków na parę
    /// (ryzyko, dzielnica), czyli tablicy, która rośnie z liczbą dzielnic razy
    /// sześćdziesiąt i wchodzi w całości do hasha stanu — za różnicę, której gracz
    /// nie odróżni od wygładzenia. Skutek jest ten sam i o niego chodzi: **powódź
    /// sprzed dziesięciu lat nie trzyma składki w górze**.
    pub fn age_one_month(&mut self) {
        let w = i64::from(self.params.window_months.max(1));
        let starzej = |s: &mut PerilStats| {
            s.exposures = (u64::from(s.exposures) * (w as u64 - 1) / w as u64) as u32;
            s.claims = (u64::from(s.claims) * (w as u64 - 1) / w as u64) as u32;
            // `i128` w mnożeniu: akumulatory rosną przez `saturating_add`, więc
            // po nasyceniu do `i64::MAX` zwykłe `* (w - 1)` panikowałoby w debug
            // i zawijało w release (znalezisko recenzji M10d).
            s.loss_total =
                Money((i128::from(s.loss_total.get()) * i128::from(w - 1) / i128::from(w)) as i64);
            s.sum_total =
                Money((i128::from(s.sum_total.get()) * i128::from(w - 1) / i128::from(w)) as i64);
        };
        for s in self.by_district.values_mut() {
            starzej(s);
        }
        for s in self.by_city.iter_mut() {
            starzej(s);
        }
    }

    /// Dopisuje szkodę do szkodowości.
    pub fn note_loss(&mut self, peril: PerilKind, district: DistrictId, loss: Money) {
        let w = self
            .by_district
            .entry((peril.as_index() as u8, district.0))
            .or_default();
        w.claims += 1;
        w.loss_total = Money(w.loss_total.get().saturating_add(loss.get()));
        let m = &mut self.by_city[peril.as_index()];
        m.claims += 1;
        m.loss_total = Money(m.loss_total.get().saturating_add(loss.get()));
    }

    pub(crate) fn end_cover(&mut self, id: CoverId) {
        if let Some(c) = self.covers.get_mut(id.0 as usize) {
            c.ended = true;
        }
    }
}

impl HashState for Insurers {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.covers.len() as u64);
        for c in &self.covers {
            c.hash_state(h);
        }
        h.write_u32(self.next_cover);
        h.write_u64(self.by_district.len() as u64);
        for ((p, d), s) in &self.by_district {
            h.write_u8(*p);
            h.write_u16(*d);
            s.hash_state(h);
        }
        for s in &self.by_city {
            s.hash_state(h);
        }
        h.write_u64(self.pending.len() as u64);
        for o in &self.pending {
            h.write_u8(o.peril.as_index() as u8);
            h.write_u16(o.district.map_or(u16::MAX, |d| d.0));
            h.write_u32(o.site.map_or(u32::MAX, |s| s.0.index()));
            h.write_u16(o.severity_bps);
            h.write_u32(o.day);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brak_historii_daje_czysty_prior() {
        assert_eq!(credibility_rate_bp(None, 300, 0, 100), 300);
        assert_eq!(credibility_rate_bp(Some(9_000), 300, 0, 100), 300);
    }

    #[test]
    fn wlasna_statystyka_przewaza_dopiero_przy_ekspozycji() {
        // Przy n == k własna waży tyle co prior.
        assert_eq!(credibility_rate_bp(Some(1_100), 300, 100, 100), 700);
        // Przy n dziesięć razy większym od k prior prawie znika.
        let r = credibility_rate_bp(Some(1_100), 300, 1_000, 100);
        assert!((1_000..=1_100).contains(&r), "{r}");
    }

    /// Kryterium WP10.12: przy małej ekspozycji jedna szkoda nie może podnieść
    /// składki o rząd wielkości.
    #[test]
    fn jedna_szkoda_przy_malej_probce_nie_wywraca_skladki() {
        let prior = 300;
        let bez = credibility_rate_bp(Some(0), prior, 12, 100);
        // Jedna szkoda na pełną sumę ubezpieczenia w dwunastu polisomiesiącach:
        // surowa szkodowość to 10 000 bp, czyli trzydziestokrotność priora.
        let po = credibility_rate_bp(Some(10_000), prior, 12, 100);
        assert!(
            po < bez * 10,
            "składka skoczyła o rząd wielkości: {bez} → {po}"
        );
    }

    #[test]
    fn skladka_jest_roczna_stawka_podzielona_na_dwanascie() {
        // 1 000 000 gr sumy, stawka 300 bp = 30 000 gr rocznie, bez narzutu → 2 500 gr.
        assert_eq!(premium(Money(1_000_000), 300, 0, Money::ZERO), Money(2_500));
        // Narzut 20 % podnosi do 3 000, plus opłata stała.
        assert_eq!(
            premium(Money(1_000_000), 300, 2_000, Money(100)),
            Money(3_100)
        );
    }

    #[test]
    fn szkodowosc_bez_ekspozycji_nie_klamie_zerem() {
        let s = PerilStats::default();
        assert_eq!(s.loss_ratio_bp(), None);
        let s = PerilStats {
            exposures: 10,
            claims: 1,
            loss_total: Money(500),
            sum_total: Money(10_000),
        };
        assert_eq!(s.loss_ratio_bp(), Some(500));
    }
}
