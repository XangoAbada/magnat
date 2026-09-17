//! `FirmView` — jedyne wejście AI firmy do świata (M7e WP10, M7 §5.8, PRD §12.2).
//!
//! # To jest decyzja architektoniczna, nie deklaracja intencji
//!
//! Plan fazy pisze: „funkcje AI nie przyjmują `&World`". W tym projekcie wychodzi
//! jeszcze mocniej, i to za darmo: `sim/firms` **nie widzi `sim/economy`** — zależność
//! idzie `economy → firms → policy` i odwrócić się nie da. Kod AI nie może więc sięgnąć
//! po księgę, po arenę ofert ani po cudzą półkę, bo tych typów w tym crate'cie po prostu
//! nie ma. Wyciek nie jest tu pilnowany lintem ani przeglądem — nie kompiluje się.
//!
//! Konsekwencja dla kształtu: widok jest **strukturą faktów**, a nie zbiorem referencji
//! do cudzych zasobów. `sim/economy` zbiera fakty i woła regułę firmy — ten sam kierunek
//! wstrzyknięcia, którym M7b rozwiązał scoring kandydata (`CandidateFacts`, `D19`).
//!
//! # Czego tu nie ma i być nie może
//!
//! Kosztu jednostkowego konkurenta (w tym gracza), jego gotówki, receptur, kontraktów,
//! zapasów, marży i planów; stanu potrzeb konkretnego mieszkańca. Nie ma na to pól,
//! więc test asymetrii (§7.3) sprawdza zachowanie, a nie dobre chęci: mutacja ukrytych
//! danych gracza nie ma jak zmienić decyzji AI, bo nie ma jak do niej dotrzeć.
//!
//! Co **jest**, bo jest jawne: cena półkowa rywala i liczba rywali w okolicy (widać je
//! z ulicy), stawki w publikowanych ofertach pracy (`K-...` — oferta jest publiczna
//! z definicji, `D9`), agregaty miasta. Wszystko z **opóźnieniem** [`FirmView::lag_days`]
//! — firma pracuje na cenie sprzed kilku dni i to jest źródło jej realnych błędów.

use magnat_core::hash::{HashState, StateHasher};
use magnat_core::{DistrictId, FirmStrategy, GoodId, JobRoleId, Money, SiteId, Tick};

use crate::key::FirmKey;
use crate::personality::FirmPersonality;

/// Co firma wie o jednym swoim towarze na jednej swojej półce.
///
/// Pola „własne" (koszt, marża, zapas) są pełne — firma zna siebie. Pola „rywala"
/// pochodzą z tablicy publicznej i są **cenami półkowymi**, nigdy kosztami.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GoodFacts {
    pub site: SiteId,
    pub good: GoodId,
    /// Własna cena półkowa, netto.
    pub own_price_net: Money,
    /// Własny koszt jednostkowy, netto.
    pub unit_cost_net: Money,
    /// Dzisiejszy cel marży sterownika ceny, w punktach bazowych.
    pub margin_bp: i32,
    /// Pokrycie zapasu w dobach sprzedaży. `-1` znaczy „nie sprzedaje się wcale",
    /// czyli pokrycie nieskończone — a to jest inna diagnoza niż „pusto".
    pub stock_days: i32,
    /// Dzisiejszy cel zamówienia w dobach.
    pub restock_days: u16,
    /// Najtańsza cena rywala w okolicy, netto, **z opóźnieniem**. `None` = nikogo nie widać.
    pub rival_cheapest_net: Option<Money>,
    /// Który zakład ją oferuje — adres sklepu jest jawny, jego koszty nie.
    pub rival_cheapest_site: Option<SiteId>,
    /// Mediana cen rywali, netto, z tym samym opóźnieniem.
    pub rival_median_net: Option<Money>,
    /// Ilu rywali widać dziś i ilu widać było przed oknem obserwacji.
    pub rivals: u16,
    pub rivals_before: u16,
    /// Sprzedaż ostatnich siedmiu dób i siedmiu poprzedzających — z tego liczy się
    /// **realna** utrata udziału, a nie z domysłu, że ktoś obok otworzył sklep.
    pub sold_7d: i64,
    pub sold_7d_prev: i64,
}

impl GoodFacts {
    /// O ile procent (w punktach bazowych) sprzedaż spadła wobec poprzedniego okna.
    /// Dodatnia liczba znaczy spadek. `None`, gdy nie ma z czym porównać.
    #[must_use]
    pub fn sales_drop_bp(&self) -> Option<i32> {
        if self.sold_7d_prev <= 0 {
            return None;
        }
        let spadek = self.sold_7d_prev.saturating_sub(self.sold_7d);
        Some((spadek.saturating_mul(10_000) / self.sold_7d_prev).clamp(-10_000, 10_000) as i32)
    }

    /// Marża, którą dzisiejsza cena faktycznie daje — w punktach bazowych ceny.
    #[must_use]
    pub fn realised_margin_bp(&self) -> Option<i32> {
        let cena = self.own_price_net.get();
        if cena <= 0 {
            return None;
        }
        let zysk = cena.saturating_sub(self.unit_cost_net.get());
        Some((zysk.saturating_mul(10_000) / cena).clamp(-100_000, 100_000) as i32)
    }
}

/// Co firma wie o swoim zakładzie.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SiteFacts {
    pub site: SiteId,
    pub district: DistrictId,
    pub vacancies: u32,
    pub headcount: u32,
    /// Ile **kolejnych** ostatnich miesięcy zakład zamknął stratą. Liczone od
    /// najnowszego wstecz i przerywane pierwszym miesiącem bez pomiaru — miesiąc,
    /// o którym nic nie wiadomo, nie jest miesiącem straty.
    pub months_in_loss: u8,
    /// Marża ostatniego zmierzonego miesiąca. `None` = zakład bez księgi.
    pub last_margin_bp: Option<i32>,
    /// Zakład prowadzony przez menedżera z polityką — jego cenami steruje polityka,
    /// a nie tier operacyjny.
    pub delegated: bool,
    /// Zakład zdelegowany, któremu nikt nie dał reguł (`AZ-1`). Menedżer jest,
    /// kierować nie ma czym — i to jest stan do naprawienia przez tier taktyczny,
    /// a nie stan spoczynkowy.
    pub needs_policy: bool,
    /// Rola, której temu zakładowi najbardziej brakuje, wraz z indeksem niedoboru
    /// w jego dzielnicy. `None`, gdy nie ma wakatów.
    pub scarcest_role: Option<(JobRoleId, u16)>,
}

/// Agregaty miasta — jawne dla wszystkich, bo publikuje je urząd.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct CityFacts {
    pub population: u32,
    pub median_income: Money,
    pub unemployment_permille: u16,
}

/// Widok firmy na świat. **Jedyne** wejście `ai::*`.
#[derive(Clone, Copy, Debug)]
pub struct FirmView<'a> {
    pub key: FirmKey,
    /// Gotówka firmy — własna, więc pełna.
    pub cash: Money,
    pub personality: FirmPersonality,
    pub strategy: FirmStrategy,
    /// Widełki marży osobowości cenowej firmy (M5c `FirmPricing`). Tier operacyjny
    /// przesuwa cel **w nich**, a nie zamiast nich — drugiego opisu tej samej
    /// osobowości nie ma (`K-8` w małej skali).
    pub margin_floor_bp: i32,
    pub margin_ceiling_bp: i32,
    /// Opóźnienie obrazu konkurencji w dobach, 1..=7 (§5.8).
    pub lag_days: u8,
    pub tick: Tick,
    pub sites: &'a [SiteFacts],
    pub goods: &'a [GoodFacts],
    pub city: CityFacts,
}

impl FirmView<'_> {
    /// Opóźnienie obrazu konkurencji: od rozmiaru firmy i od jej otwartości na nowe
    /// (§5.8, §6.3).
    ///
    /// Duża i ciekawska patrzy częściej, mała i zachowawcza rzadziej. Sufit siedmiu dób
    /// jest kontraktem tablicy publicznej — okno ma siedem dni i ósmej nie będzie.
    #[must_use]
    pub fn lag_for(sites: usize, p: &FirmPersonality) -> u8 {
        let rozmiar = sites.min(3) as i32;
        let ciekawosc = i32::from(p.innovation) / 34;
        (7 - rozmiar - ciekawosc).clamp(1, 7) as u8
    }

    /// Fakty o towarze na wskazanej półce.
    #[must_use]
    pub fn good(&self, site: SiteId, good: GoodId) -> Option<&GoodFacts> {
        self.goods.iter().find(|g| g.site == site && g.good == good)
    }

    #[must_use]
    pub fn site(&self, site: SiteId) -> Option<&SiteFacts> {
        self.sites.iter().find(|s| s.site == site)
    }
}

impl HashState for SiteFacts {
    fn hash_state(&self, h: &mut StateHasher) {
        self.site.0.hash_state(h);
        self.district.hash_state(h);
        h.write_u32(self.vacancies);
        h.write_u32(self.headcount);
        h.write_u8(self.months_in_loss);
        h.write_u32(self.last_margin_bp.unwrap_or(i32::MIN) as u32);
        h.write_u8(u8::from(self.delegated));
        h.write_u8(u8::from(self.needs_policy));
        match self.scarcest_role {
            None => h.write_u8(0),
            Some((r, i)) => {
                h.write_u8(1);
                h.write_u16(r.0);
                h.write_u16(i);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fakty() -> GoodFacts {
        GoodFacts {
            site: SiteId(magnat_core::Entity::new(1, std::num::NonZeroU32::MIN)),
            good: GoodId(3),
            own_price_net: Money(500),
            unit_cost_net: Money(400),
            margin_bp: 2_500,
            stock_days: 7,
            restock_days: 10,
            rival_cheapest_net: None,
            rival_cheapest_site: None,
            rival_median_net: None,
            rivals: 0,
            rivals_before: 0,
            sold_7d: 100,
            sold_7d_prev: 100,
        }
    }

    #[test]
    fn spadek_sprzedazy_liczy_sie_wzgledem_poprzedniego_okna() {
        let mut g = fakty();
        g.sold_7d = 60;
        assert_eq!(g.sales_drop_bp(), Some(4_000));
        g.sold_7d = 120;
        assert_eq!(g.sales_drop_bp(), Some(-2_000));
        g.sold_7d_prev = 0;
        assert_eq!(g.sales_drop_bp(), None, "nie ma z czym porównać");
    }

    #[test]
    fn marza_zrealizowana_liczy_sie_od_ceny() {
        let g = fakty();
        assert_eq!(g.realised_margin_bp(), Some(2_000));
    }

    #[test]
    fn opoznienie_miesci_sie_w_oknie_tablicy() {
        for sites in 0..10usize {
            for innowacja in 0..=100u8 {
                let mut p = FirmPersonality::NEUTRAL;
                p.innovation = innowacja;
                let l = FirmView::lag_for(sites, &p);
                assert!((1..=7).contains(&l), "{sites} zakładów, {innowacja} → {l}");
            }
        }
        // Duża i ciekawska patrzy częściej niż mała i zachowawcza.
        let mut ciekawska = FirmPersonality::NEUTRAL;
        ciekawska.innovation = 100;
        let mut zachowawcza = FirmPersonality::NEUTRAL;
        zachowawcza.innovation = 0;
        assert!(FirmView::lag_for(5, &ciekawska) < FirmView::lag_for(0, &zachowawcza));
    }
}
