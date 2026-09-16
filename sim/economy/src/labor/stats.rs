//! Statystyki rynku pracy i indeks niedoboru (M7b WP5, M7 §5.5).
//!
//! **Te dane są jawne.** Widzą je i gracz, i firma AI, i to jest zamierzone: płace
//! w ofertach są publiczne, koszty jednostkowe nie są (M7 §5.5). Stąd bierze się
//! asymetria, na której stoi cała faza — konkurent wie, ile płacisz spawaczowi,
//! i nie wie, ile na nim zarabiasz.
//!
//! **Nigdzie tu nie ma tabeli płac.** Mediana jest agregatem po ofertach i po zawartych
//! umowach, liczonym z tego, co rynek zrobił — a nie liczbą, z której rynek się wywodzi.

use magnat_core::{DistrictId, HashState, JobRoleId, Money, StateHasher};
use magnat_firms::Ring;
use std::collections::BTreeMap;

/// Ile umów wstecz pamięta mediana zaakceptowanej płacy.
///
/// Szesnaście, bo mediana z czterech skacze przy każdej umowie, a mediana ze stu
/// przestaje reagować w skali, w której gracz obserwuje skutek swojej decyzji.
const HISTORY: usize = 16;

/// Ilu kandydatów na wakat rynek uznaje za „bez niedoboru".
///
/// Trzech: mniej znaczy, że firma nie ma z czego wybierać, więcej nie poprawia
/// już jej sytuacji. To jest mianownik indeksu niedoboru i jedyna arbitralna liczba
/// w tym pliku — dlatego stoi nazwana, a nie wpisana w środku wzoru.
const APPLICANTS_PER_SLOT: u32 = 3;

/// Ile punktów indeksu dokłada każda doba wiszącej oferty, i ile najwyżej razem.
///
/// Kalibrowane na zdanie z §1 dokumentu fazy: „oferta wisiała 14 dni bez kandydata,
/// a indeks niedoboru w dzielnicy = 0,82". Czternaście dób × 50 = 700, plus człon
/// braku aplikacji — i wychodzi dokładnie tamta liczba. **Czas waży więcej niż liczba
/// aplikacji** i to jest właściwa kolejność: aplikacja, po której nikogo nie zatrudniono,
/// nie jest dowodem, że rynek ma kogo dać.
const DAYS_WEIGHT: u32 = 50;
const DAYS_CAP: u32 = 700;

/// Ile najwyżej dokłada człon „nikt nie aplikuje".
const SCARCITY_CAP: u32 = 300;

/// Stan jednego zawodu w jednej dzielnicy.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct RoleStats {
    pub vacancies: u32,
    /// Suma aplikacji z ostatnich ~30 dób. Nie okno przesuwne, tylko **suma malejąca**:
    /// `s ← s − s/30 + dziś`. Jedna liczba zamiast trzydziestu na każdy z ~900 kluczy,
    /// a zachowanie jest to samo — stara aplikacja przestaje się liczyć po miesiącu.
    pub applicants_30d: u32,
    pub median_wage_posted: Money,
    pub median_wage_accepted: Money,
    pub median_days_to_fill: u16,
    /// 0..=1000. Zero = kandydatów w bród, tysiąc = nikt nie przychodzi i wakat wisi.
    pub shortage_index: u16,
    /// Ostatnie zawarte stawki — z nich liczy się mediana zaakceptowana.
    accepted: Ring<Money, HISTORY>,
    /// Ile dób wisiały ostatnio obsadzone wakaty.
    filled_in: Ring<u16, HISTORY>,
}

impl RoleStats {
    /// Zapisuje zawartą umowę: stawkę i czas, jaki wakat wisiał.
    pub fn record_hire(&mut self, wage: Money, days_open: u16) {
        self.accepted.push(wage);
        self.filled_in.push(days_open);
    }

    /// Przelicza mediany z pamięci i indeks niedoboru z wakatów, aplikacji i czasu.
    ///
    /// `posted` to stawki wszystkich wiszących ofert tego zawodu, `open_days` — suma
    /// dób, które przewisiały. Bufory podaje wołający, bo przeliczenie leci po wszystkich
    /// kluczach.
    pub fn recompute(&mut self, posted: &mut [Money], open_days: u32) {
        let ofert = posted.len() as u32;
        self.median_wage_posted = mediana(posted).unwrap_or(Money::ZERO);
        let mut buf: Vec<Money> = self.accepted.iter().copied().collect();
        self.median_wage_accepted = mediana(&mut buf).unwrap_or(Money::ZERO);
        let mut dni: Vec<u16> = self.filled_in.iter().copied().collect();
        dni.sort_unstable();
        self.median_days_to_fill = dni.get(dni.len() / 2).copied().unwrap_or(0);
        self.shortage_index = shortage(self.vacancies, self.applicants_30d, open_days, ofert);
    }

    /// Dobowy krok sumy malejącej aplikacji.
    ///
    /// Ubytek zaokrągla się **w górę** i to nie jest kosmetyka: przy dzieleniu w dół
    /// suma poniżej trzydziestu przestaje maleć w ogóle (`29 − 29/30 = 29`) i zawód,
    /// w którym ostatnia aplikacja wpłynęła rok temu, na zawsze zostaje z resztką
    /// mówiącą „ktoś tu jednak chciał". Indeks niedoboru czytałby ją jako prawdę.
    pub fn tick_applicants(&mut self, today: u32) {
        let ubytek = self.applicants_30d.div_ceil(30);
        self.applicants_30d = self.applicants_30d - ubytek + today;
    }
}

impl HashState for RoleStats {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.vacancies);
        h.write_u32(self.applicants_30d);
        self.median_wage_posted.hash_state(h);
        self.median_wage_accepted.hash_state(h);
        h.write_u16(self.median_days_to_fill);
        h.write_u16(self.shortage_index);
        h.write_u32(self.accepted.len() as u32);
        for m in self.accepted.iter() {
            m.hash_state(h);
        }
        h.write_u32(self.filled_in.len() as u32);
        for d in self.filled_in.iter() {
            h.write_u16(*d);
        }
    }
}

/// Mediana jako element **górny** przy parzystej liczności — ta sama zasada
/// co w [`crate::price_stats`]: mediana ma być stawką z rynku, a nie średnią
/// dwóch stawek, której nikt nikomu nie zaproponował.
fn mediana(v: &mut [Money]) -> Option<Money> {
    if v.is_empty() {
        return None;
    }
    v.sort_unstable();
    Some(v[v.len() / 2])
}

/// Indeks niedoboru: 0..=1000 z wakatów, aplikacji i czasu wiszenia (M7 §5.5).
///
/// Dwa niezależne człony, bo mówią o dwóch różnych rzeczach: brak aplikacji znaczy
/// „nikt nie chce", a długi wakat — „ci, którzy chcieli, nie byli do przyjęcia".
/// Zawód bez wakatów ma niedobór zero i nie ma innej sensownej odpowiedzi.
#[must_use]
pub fn shortage(vacancies: u32, applicants_30d: u32, open_days: u32, offers: u32) -> u16 {
    if vacancies == 0 {
        return 0;
    }
    let potrzeba = vacancies.saturating_mul(APPLICANTS_PER_SLOT).max(1);
    let pokrycie = (applicants_30d.saturating_mul(1_000) / potrzeba).min(1_000);
    let z_braku = SCARCITY_CAP - pokrycie * SCARCITY_CAP / 1_000;
    // Średnia doba wiszenia **na ofertę**, a nie na etat: oferta na osiem etatów,
    // która wisi dwa tygodnie, jest tym samym sygnałem co osiem ofert na jeden etat.
    let z_czasu = (open_days / offers.max(1) * DAYS_WEIGHT).min(DAYS_CAP);
    (z_braku.saturating_add(z_czasu)).min(1_000) as u16
}

/// Rynek pracy widziany z zewnątrz (M7 §5.5). `BTreeMap`, nie `HashMap` — iteracja
/// po tych kluczach wyznacza kolejność decyzji firm (00 §3.2).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct LaborMarketStats {
    pub per_role: BTreeMap<(JobRoleId, DistrictId), RoleStats>,
}

impl LaborMarketStats {
    #[must_use]
    pub fn get(&self, role: JobRoleId, district: DistrictId) -> Option<&RoleStats> {
        self.per_role.get(&(role, district))
    }

    pub fn entry(&mut self, role: JobRoleId, district: DistrictId) -> &mut RoleStats {
        self.per_role.entry((role, district)).or_default()
    }

    /// Indeks niedoboru zawodu w dzielnicy — kontrakt do M8 (polityka) i M10 (związki).
    #[must_use]
    pub fn shortage_index(&self, role: JobRoleId, district: DistrictId) -> u16 {
        self.get(role, district).map_or(0, |s| s.shortage_index)
    }

    /// Mediana zawartych umów; `None`, dopóki w tym zawodzie nikogo nie zatrudniono.
    /// To jest **jedyna** postać, w jakiej „płaca zawodu" w ogóle istnieje.
    #[must_use]
    pub fn median_accepted(&self, role: JobRoleId, district: DistrictId) -> Option<Money> {
        self.get(role, district)
            .map(|s| s.median_wage_accepted)
            .filter(|m| m.get() > 0)
    }

    /// Mediana stawek w wiszących ofertach — to, co kandydat widzi, zanim gdziekolwiek
    /// złoży aplikację.
    #[must_use]
    pub fn median_posted(&self, role: JobRoleId, district: DistrictId) -> Option<Money> {
        self.get(role, district)
            .map(|s| s.median_wage_posted)
            .filter(|m| m.get() > 0)
    }
}

impl HashState for LaborMarketStats {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.per_role.len() as u32);
        for ((r, d), s) in &self.per_role {
            r.hash_state(h);
            d.hash_state(h);
            s.hash_state(h);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zawod_bez_wakatow_nie_ma_niedoboru() {
        assert_eq!(shortage(0, 0, 0, 0), 0);
    }

    #[test]
    fn brak_kandydatow_podnosi_indeks() {
        assert_eq!(shortage(8, 0, 0, 1), SCARCITY_CAP as u16);
        // Komplet chętnych sprowadza człon „nikt nie chce" do zera.
        assert_eq!(shortage(8, 8 * APPLICANTS_PER_SLOT, 0, 1), 0);
    }

    #[test]
    fn oferta_wiszaca_dwa_tygodnie_daje_niedobor_z_dokumentu_fazy() {
        // §1 dokumentu fazy: „oferta wisiała 14 dni bez kandydata, a indeks niedoboru
        // w dzielnicy = 0,82". Jedna oferta, jeden etat, nikt nie aplikuje — skraja skali.
        assert_eq!(shortage(1, 0, 14, 1), 1_000);
        // Dwie aplikacje na trzy spodziewane i te same czternaście dób: 0,80.
        // Liczba z dokumentu mieści się w tej skali, a nie obok niej.
        assert_eq!(shortage(1, 2, 14, 1), 801);
    }

    #[test]
    fn dlugi_wakat_podnosi_indeks_nawet_przy_aplikacjach() {
        let komplet = 8 * APPLICANTS_PER_SLOT;
        let swiezy = shortage(8, komplet, 0, 1);
        let wiszacy = shortage(8, komplet, 30, 1);
        assert!(wiszacy > swiezy, "{swiezy} → {wiszacy}");
        // Ale człon czasu ma sufit — sam czas nie robi z rynku pustyni.
        assert!(shortage(8, komplet, 10_000, 1) <= DAYS_CAP as u16);
    }

    #[test]
    fn mediana_zaakceptowanej_placy_idzie_za_umowami_a_nie_za_tabela() {
        let mut s = RoleStats::default();
        let mut posted = vec![Money(400_000), Money(420_000), Money(380_000)];
        s.recompute(&mut posted, 0);
        assert_eq!(s.median_wage_posted, Money(400_000));
        assert_eq!(
            s.median_wage_accepted,
            Money::ZERO,
            "nikogo nie zatrudniono"
        );

        for w in [520_000, 540_000, 560_000] {
            s.record_hire(Money(w), 4);
        }
        let mut posted = vec![Money(400_000)];
        s.recompute(&mut posted, 0);
        assert_eq!(s.median_wage_accepted, Money(540_000));
        assert_eq!(s.median_days_to_fill, 4);
    }

    #[test]
    fn suma_aplikacji_zapomina_stare() {
        let mut s = RoleStats::default();
        for _ in 0..30 {
            s.tick_applicants(10);
        }
        // Suma malejąca **dochodzi** do 300, a nie skacze tam po miesiącu: po trzydziestu
        // dobach jest w dwóch trzecich drogi i to jest właściwe zachowanie wygładzania.
        let po_miesiacu = s.applicants_30d;
        assert!(
            (150..=250).contains(&po_miesiacu),
            "trzydzieści dób po dziesięć aplikacji → {po_miesiacu}"
        );
        // I schodzi do zera, a nie do resztki: dzielenie w dół zatrzymałoby ją na 29.
        for _ in 0..180 {
            s.tick_applicants(0);
        }
        assert_eq!(s.applicants_30d, 0);
    }
}
