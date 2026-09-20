//! Żal załogi i warunki powstania związku — funkcje czyste (M10e §5.9).
//!
//! Wszystko tutaj jest arytmetyką całkowitą nad liczbami, które zakład i tak ma:
//! rachunkiem wyniku miesiąca, medianą zawodu w mieście i stanem załogi. Żadna
//! z tych funkcji nie widzi świata i nie losuje — dlatego to one, a nie system,
//! są treścią testów kryterium WP10.14.

use magnat_core::Money;

use super::data::UnionParams;

/// Trzy człony żalu, każdy w skali 0..=100. Rozpisane osobno, bo panel pracowniczy
/// ma odpowiadać na pytanie „czego oni właściwie chcą", a suma tego nie mówi.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct GrievanceTerms {
    /// Ile zakład zarabia ponad koszty — załoga czyta to jako „jest z czego dać".
    pub profit: u8,
    /// O ile płacą tu mniej niż w mieście za tę samą robotę.
    pub wage_gap: u8,
    /// Średni stres załogi.
    pub stress: u8,
}

impl GrievanceTerms {
    /// Ważona suma w skali 0..=100. Wagi sumują się do stu i pilnuje tego walidator
    /// danych, więc dzielenie przez sto jest tu tożsamością, a nie normalizacją.
    #[must_use]
    pub fn total(self, p: &UnionParams) -> u8 {
        let s = u32::from(self.profit) * u32::from(p.w_profit)
            + u32::from(self.wage_gap) * u32::from(p.w_wage)
            + u32::from(self.stress) * u32::from(p.w_stress);
        u8::try_from((s / 100).min(100)).unwrap_or(100)
    }
}

/// Człon „jest z czego dać": marża zakładu w punktach bazowych na skalę 0..=100.
///
/// Pięćdziesiąt punktów bazowych marży na jeden punkt żalu, czyli 35 % marży daje
/// 70 — i to jest liczba wprost z kryterium WP10.14. Marża ujemna daje zero:
/// załoga firmy, która dokłada do interesu, nie ma o co prosić i wie o tym.
#[must_use]
pub fn profit_term(margin_bp: Option<i32>) -> u8 {
    let Some(m) = margin_bp else { return 0 };
    u8::try_from((m / 50).clamp(0, 100)).unwrap_or(0)
}

/// Człon luki płacowej: o ile promili płaca tutaj odstaje **w dół** od mediany
/// miejskiej zawodu, przez dwa. Dwadzieścia procent poniżej (200 ‰) daje sto —
/// znów liczba wprost z kryterium.
#[must_use]
pub fn wage_gap_term(here: Money, city_median: Option<Money>) -> u8 {
    let Some(med) = city_median else { return 0 };
    if med.get() <= 0 || here.get() >= med.get() {
        return 0;
    }
    let luka = (med.get() - here.get()).saturating_mul(1_000) / med.get();
    u8::try_from((luka / 2).clamp(0, 100)).unwrap_or(100)
}

/// Żal zakładu z trzech członów.
///
/// **Czwartego członu z planu §5.9 — `w4 × wypadki_12m` — nie ma i to jest
/// rozstrzygnięcie, nie przeoczenie.** Licznika wypadków przy pracy nie ma dziś
/// nigdzie w projekcie: `DeprivationEffect::AccidentRisk` istnieje w słowniku
/// i wpada w pustą gałąź `sim/agents::needs`. Człon nad liczbą, której nikt nie
/// liczy, byłby zerem udającym wagę (`R2`) — wraca razem z wypadkami, kiedy te
/// dostaną źródło.
#[must_use]
pub fn grievance(
    margin_bp: Option<i32>,
    wage_here: Money,
    city_median: Option<Money>,
    mean_stress: u8,
) -> GrievanceTerms {
    GrievanceTerms {
        profit: profit_term(margin_bp),
        wage_gap: wage_gap_term(wage_here, city_median),
        stress: mean_stress.min(100),
    }
}

/// Największa spójna składowa grafu relacji w zbiorze `crew`.
///
/// `edges` są parami indeksów encji; krawędź licząca się do składowej musi mieć
/// **oba** końce w załodze (§5.9). Zbiory rozłączne bez kompresji ścieżek —
/// przy kilkudziesięciu wierzchołkach koszt jest pomijalny, a kod bez sztuczek
/// czyta się jako to, czym jest.
#[must_use]
pub fn largest_component(crew: &[u32], edges: &[(u32, u32)]) -> u32 {
    if crew.is_empty() {
        return 0;
    }
    let poz = |x: u32| crew.binary_search(&x).ok();
    let mut ojciec: Vec<usize> = (0..crew.len()).collect();
    fn znajdz(o: &mut [usize], mut i: usize) -> usize {
        while o[i] != i {
            o[i] = o[o[i]];
            i = o[i];
        }
        i
    }
    for (a, b) in edges {
        let (Some(ia), Some(ib)) = (poz(*a), poz(*b)) else {
            continue;
        };
        let (ra, rb) = (znajdz(&mut ojciec, ia), znajdz(&mut ojciec, ib));
        if ra != rb {
            // Zawsze w stronę niższego indeksu — kolejność wejścia krawędzi nie
            // ma prawa zmienić wyniku (00 §3.2).
            let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
            ojciec[hi] = lo;
        }
    }
    let mut licznik = vec![0u32; crew.len()];
    for i in 0..crew.len() {
        let r = znajdz(&mut ojciec, i);
        licznik[r] += 1;
    }
    licznik.into_iter().max().unwrap_or(0)
}

/// Próg spójnej składowej: `max(min_component, component_share_bp × załoga)`.
#[must_use]
pub fn component_threshold(headcount: u32, p: &UnionParams) -> u32 {
    let udzial = headcount.saturating_mul(u32::from(p.component_share_bp)) / 10_000;
    udzial.max(u32::from(p.min_component))
}

/// Czy trzy warunki powstania związku są spełnione naraz (§5.9).
#[must_use]
pub fn may_form(
    days_above: u16,
    headcount: u32,
    component: u32,
    density_bp: u32,
    p: &UnionParams,
) -> bool {
    days_above >= p.grievance_days
        && component >= component_threshold(headcount, p)
        && density_bp >= u32::from(p.density_bp)
}

/// Żądanie płacowe: o ile procent więcej, względem tego, co załoga **wie**
/// o płacach gdzie indziej.
///
/// Zero znaczy „nie ma o co prosić" — kotwica poniżej dzisiejszej płacy nie daje
/// żądania ujemnego, bo nikt nie żąda obniżki.
#[must_use]
pub fn demand_raise_bp(here: Money, anchor: Money, p: &UnionParams) -> u16 {
    if here.get() <= 0 || anchor.get() <= here.get() {
        return 0;
    }
    let bp = (anchor.get() - here.get()).saturating_mul(10_000) / here.get();
    u16::try_from(bp.clamp(0, i64::from(p.demand_cap_bp))).unwrap_or(p.demand_cap_bp)
}

/// Ile firma jest gotowa dać w tej rundzie.
///
/// Rośnie z marżą (jest z czego) i z numerem rundy (przestój kosztuje). Sufit
/// z danych. Firma bez zmierzonej marży nie ustępuje: nie wie, czy ma z czego.
#[must_use]
pub fn concession_bp(margin_bp: Option<i32>, round: u8, striking: bool, p: &UnionParams) -> u16 {
    let Some(m) = margin_bp else { return 0 };
    if m <= 0 {
        return 0;
    }
    // Połowa marży rozłożona na rundy: firma zaczyna nisko i dochodzi do sufitu
    // dopiero wtedy, gdy spór trwa. Strajk przyspiesza to o jedną rundę, bo
    // przestój kosztuje więcej niż podwyżka.
    let rundy = u32::from(round) + u32::from(striking);
    let baza = (m as u32 / 2).min(u32::from(p.concession_cap_bp));
    let krok = baza * (rundy + 1) / (u32::from(p.max_rounds) + 1);
    u16::try_from(krok.min(u32::from(p.concession_cap_bp))).unwrap_or(p.concession_cap_bp)
}

/// Próg akceptacji związku: schodzi wraz z wyczerpywaniem funduszu strajkowego.
///
/// Pełny fundusz znaczy „trzymamy się żądania", pusty — „bierzemy, co dają".
/// To jest mechanizm, który kończy strajki, i dlatego jest funkcją **pieniądza**,
/// a nie liczby dób.
#[must_use]
pub fn accept_bp(raise_bp: u16, fund: Money, fund_at_start: Money) -> u16 {
    if fund_at_start.get() <= 0 {
        return 0;
    }
    let zostalo = fund.get().clamp(0, fund_at_start.get());
    let bp = i64::from(raise_bp) * zostalo / fund_at_start.get();
    u16::try_from(bp.clamp(0, i64::from(u16::MAX))).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p() -> UnionParams {
        super::super::data::RelationsTuning::default().union
    }

    #[test]
    fn zaklad_z_kryterium_wp10_14_przekracza_prog_zalu() {
        // Marża 35 %, płaca 20 % poniżej mediany miejskiej, stres przeciętny.
        let g = grievance(Some(3_500), Money(400_000), Some(Money(500_000)), 30);
        assert_eq!(g.profit, 70);
        assert_eq!(g.wage_gap, 100);
        assert!(
            g.total(&p()) >= p().grievance_threshold,
            "żal {} nie sięga progu {}",
            g.total(&p()),
            p().grievance_threshold
        );
    }

    #[test]
    fn uczciwy_pracodawca_nie_dorabia_sie_zwiazku() {
        // Marża 10 %, płaca rynkowa, stres przeciętny.
        let g = grievance(Some(1_000), Money(500_000), Some(Money(500_000)), 30);
        assert!(
            g.total(&p()) < p().grievance_threshold,
            "żal {} przekroczył próg u pracodawcy płacącego jak rynek",
            g.total(&p())
        );
    }

    #[test]
    fn skladowa_liczy_sie_tylko_wsrod_zalogi() {
        let crew = vec![1, 2, 3, 10];
        // 1–2–3 są spójne, 10 jest sam, a krawędź 3–99 wychodzi poza załogę.
        let edges = [(1, 2), (2, 3), (3, 99)];
        assert_eq!(largest_component(&crew, &edges), 3);
        assert_eq!(largest_component(&[], &edges), 0);
    }

    #[test]
    fn prog_skladowej_to_wiekszy_z_dwoch_warunkow() {
        assert_eq!(
            component_threshold(20, &p()),
            8,
            "25 % z 20 to 5, więc sufit 8"
        );
        assert_eq!(
            component_threshold(100, &p()),
            25,
            "25 % ze stu bije ósemkę"
        );
    }

    #[test]
    fn prog_akceptacji_schodzi_z_funduszem() {
        let start = Money(1_000_000);
        assert_eq!(accept_bp(2_000, start, start), 2_000);
        assert_eq!(accept_bp(2_000, Money(500_000), start), 1_000);
        assert_eq!(accept_bp(2_000, Money::ZERO, start), 0);
    }

    #[test]
    fn firma_bez_zmierzonej_marzy_nie_ustepuje() {
        assert_eq!(concession_bp(None, 3, true, &p()), 0);
        assert_eq!(concession_bp(Some(-500), 3, true, &p()), 0);
        assert!(
            concession_bp(Some(3_500), 4, true, &p()) > concession_bp(Some(3_500), 0, false, &p())
        );
    }
}
