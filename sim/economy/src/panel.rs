//! Migawka panelu sklepu (M5e §5.12) — **jedyne** wejście interfejsu do gospodarki.
//!
//! Panel nie dostaje `&Market` ani tym bardziej `&World`: dostaje gotową, oderwaną
//! od stanu strukturę, którą wolno trzymać przez klatkę i czytać bez zamka. To jest
//! ta sama zasada, którą `sim/snapshot` stosuje do renderu — warstwa prezentacji
//! nie ma prawa mutować symulacji, więc nie dostaje do niej uchwytu.
//!
//! **Niczego tu się nie liczy.** Każde pole jest przepisaniem z `Market`, `Ledger`
//! albo `PriceController`; jedyna arytmetyka to marża i dni pokrycia, obie z liczb,
//! które już są. Gdyby panel liczył cokolwiek u siebie, gracz widziałby drugi
//! rachunek obok tego, na którym stoi symulacja (00 §7).

use magnat_agents::SocialClass;
use magnat_core::{
    DecisionReason, DistrictId, FirmId, GoodId, Money, Qty, SimMinute, SiteId, Tick, UtilityKind,
};

use crate::ledger::{BalanceSheet, CashFlow, IncomeStatement};
use crate::pricing::PricePolicy;
use crate::shop::{LostSale, LostSaleHistogram, LostSaleTracking};

/// Wszystko, co pokazuje panel sklepu: trzy zakładki (Półki / Klienci / Konkurencja)
/// plus wspólny nagłówek finansowy.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ShopPanelSnapshot {
    pub site: SiteId,
    pub firm: FirmId,
    /// Rodzaj zakładu — nagłówek panelu. Z migawki, nie z domysłu wołającego.
    pub kind: magnat_core::PlaceKind,
    /// Chwila, z której pochodzi migawka — panel ma pokazywać, że patrzy na dane
    /// sprzed N minut, tak samo jak pokazuje wiek obrazu konkurencji.
    pub at: Tick,
    pub tracking: LostSaleTracking,
    pub shelves: Vec<ShelfRow>,
    pub customers: CustomerStats,
    pub lost_sales: LostSalesView,
    pub competition: Vec<CompetitorRow>,
    pub finance: FinanceSummary,
    /// Powody ostatnich przecen — „dlaczego wczoraj potaniało". Pusty dla zakładu
    /// nieśledzonego, bo pierścień prowadzą wyłącznie oznaczone (`W-7`).
    pub reprices: Vec<DecisionReason>,
    /// Klucze tekstowe towarów występujących w migawce, posortowane po `GoodId`.
    ///
    /// Migawka niesie je ze sobą, bo inaczej interfejs musiałby trzymać `GoodTable`
    /// obok — a wtedy przestałaby być **jedynym** wejściem do gospodarki i pierwsza
    /// rozbieżność katalogów wyszłaby jako pusta nazwa na ekranie.
    pub good_keys: Vec<(GoodId, String)>,
}

impl ShopPanelSnapshot {
    /// Klucz tekstowy towaru. Pusty łańcuch znaczy „katalog go nie zna" — interfejs
    /// pokazuje wtedy identyfikator, bo pusta nazwa nie mówi nic.
    #[must_use]
    pub fn good_key(&self, good: GoodId) -> &str {
        self.good_keys
            .binary_search_by_key(&good.0, |(g, _)| g.0)
            .map_or("", |i| self.good_keys[i].1.as_str())
    }
}

/// Jedna linia półki.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ShelfRow {
    pub good: GoodId,
    /// Cena brutto z oferty (`K-7`) — ta, którą płaci kupujący.
    pub price: Money,
    /// Koszt własny za jednostkę ceny, średnia ważona zapasu (§5.8).
    pub unit_cost: Money,
    /// Marża w punktach bazowych ponad koszt własny.
    pub margin_bp: i32,
    pub on_shelf: Qty,
    pub backroom: Qty,
    /// Na ile dób starczy zapasu przy obrocie z ostatniego tygodnia.
    /// `u16::MAX` = „nic się nie sprzedaje", a nie „starczy na zawsze".
    pub days_of_cover: u16,
    pub turnover_7d: Qty,
    /// Chwila, w której linia traci ważność — **bezwzględna**, nie „za ile".
    /// Ile zostało, liczy interfejs z `ShopPanelSnapshot::at`; migawka nie podaje
    /// różnicy, bo wtedy starzałaby się w kieszeni panelu.
    pub expires_at: Option<SimMinute>,
    pub policy: PricePolicy,
    /// `true` = cena stoi na polityce, `false` = gracz ustawił ją ręcznie.
    pub delegated: bool,
}

/// Karta „Klienci": skąd, kto i dlaczego (§5.12).
///
/// Rozkłady są kumulatywne od włączenia śledzenia, `daily` prowadzi okno siedmiu
/// dób — powód przy [`crate::ShopCustomers`].
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct CustomerStats {
    pub by_district: Vec<(DistrictId, u32)>,
    pub by_class: Vec<(SocialClass, u32)>,
    pub by_driver: Vec<(UtilityKind, u32)>,
    /// Zakupy w siedmiu ostatnich dobach, **od najstarszej do dzisiejszej**:
    /// `daily[6]` to doba `ShopPanelSnapshot::at`. Migawka obraca pierścień przy
    /// składaniu, żeby interfejs nie musiał znać jego kotwicy — inaczej pierwszy
    /// wykres narysowany bez tej wiedzy pokazywałby tydzień przesunięty o losowo
    /// wiele dób i nikt by tego nie zauważył.
    ///
    /// To jest ta liczba, na której widać skutek podwyżki po trzech dniach.
    pub daily: [u32; 7],
    pub total: u32,
}

/// Karta „utracone sprzedaże" — trójstopniowa, zgodnie z `LostSaleTracking`.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct LostSalesView {
    /// Siedem ostatnich dób łącznie — okno, o które pyta kryterium WP12.
    pub histogram: LostSaleHistogram,
    /// Do 256 ostatnich zdarzeń. Niosą `CitizenId`, nie imię: rozwiązanie
    /// tożsamości wymaga `&World`, którego interfejs nie dostaje — kartę osoby
    /// otwiera się kliknięciem w mieszkańca, nie w wiersz utraconej sprzedaży.
    /// Pusty poza poziomem `Full`.
    pub recent: Vec<LostSale>,
}

/// Konkurent w zasięgu obserwacji, z **jawnym wiekiem** obrazu.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CompetitorRow {
    pub site: SiteId,
    pub distance_m: u32,
    pub prices: Vec<(GoodId, Money)>,
    /// Ile dób temu widziano te ceny. Pokazywane wprost: gracz ma widzieć to samo
    /// opóźnienie, z którym gra AI (§6.3).
    pub observed_age_days: u8,
}

/// Nagłówek finansowy: RZiS bieżącego miesiąca, bilans, przepływy i zapas.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FinanceSummary {
    pub statement: IncomeStatement,
    pub balance: BalanceSheet,
    /// `complete == false` znaczy „okno dziennika nie objęło całego okresu" —
    /// panel **musi** to pokazać, bo liczba obcięta oknem wygląda jak prawdziwa.
    pub cash: CashFlow,
    /// Wycena zapasu (zaplecze + półka) — lewa strona niezmiennika P5.
    pub inventory_value: Money,
    pub loan: Option<crate::books::LoanId>,
}

/// Rozkład cen jednego towaru po wszystkich ofertach miasta (§7.4).
///
/// **To jest jedyna postać, w jakiej istnieje „cena mleka w mieście"** (PRD §6.1):
/// agregat po ofertach, nie zmienna. Percentyle liczą się metodą najbliższej rangi
/// na posortowanej tablicy — bez interpolacji, bo interpolacja wprowadziłaby float
/// do liczby, którą bramka porównuje.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct PriceDist {
    pub good: GoodId,
    pub min: Money,
    pub p10: Money,
    pub p50: Money,
    pub p90: Money,
    pub max: Money,
    pub offers: u32,
}

/// Zbiorczy odczyt stanu rynku dla balansatora — **jeden zamek, nie sto tysięcy**.
///
/// Balansator próbkuje to raz na dobę gry; składanie go z pojedynczych akcesorów
/// brałoby zamek raz na sklep i mogłoby złapać dwa różne stany świata w jednym
/// wierszu raportu. Bramki porównują liczby między dobami, więc spójność wewnątrz
/// doby jest warunkiem, żeby w ogóle coś znaczyły.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct BalanceSample {
    pub prices: Vec<PriceDist>,
    /// Mediana marży zakładów w punktach bazowych ponad koszt własny.
    pub margin_median_bp: i32,
    /// Zakłady z ujemnym saldem księgi — w M5 to jest całe „bankructwo",
    /// bo postępowanie upadłościowe prowadzi M7 (`K-10`).
    pub insolvent: u32,
    pub shops: u32,
    /// Udział ofert z pustą półką w ogólnej liczbie ofert, w promilach.
    pub stockout_permille: i32,
    /// Koncentracja Herfindahla-Hirschmana × 10 000, mediana po parach
    /// (kategoria, dzielnica) mających co najmniej dwa zakłady.
    pub hhi_median: i32,
    /// Ile par (kategoria, dzielnica) w ogóle dało się zmierzyć.
    pub hhi_pairs: u32,
    /// Kategorie zapasu, w których miasto ma choć jedną ofertę z towarem.
    pub live_categories: u32,
}

impl ShelfRow {
    /// Marża ponad koszt własny w punktach bazowych.
    #[must_use]
    pub fn margin_of(price: Money, unit_cost: Money) -> i32 {
        if unit_cost.get() <= 0 {
            return 0;
        }
        i32::try_from((price.get() - unit_cost.get()).saturating_mul(10_000) / unit_cost.get())
            .unwrap_or(i32::MAX)
    }

    /// Dni pokrycia przy obrocie tygodniowym. Zero sprzedaży = `u16::MAX`,
    /// czyli „nie wiadomo", a nie „nieskończenie długo".
    #[must_use]
    pub fn cover_of(on_hand: Qty, turnover_7d: Qty) -> u16 {
        let dobowy = turnover_7d.get() / 7;
        if dobowy <= 0 {
            return u16::MAX;
        }
        u16::try_from(on_hand.get() / dobowy).unwrap_or(u16::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marza_i_pokrycie_licza_sie_z_liczb_ktore_juz_sa() {
        // 30 % marży: koszt 200, cena 260.
        assert_eq!(ShelfRow::margin_of(Money(260), Money(200)), 3_000);
        // Koszt zero (towar, którego nigdy nie kupiono) nie dzieli przez zero.
        assert_eq!(ShelfRow::margin_of(Money(260), Money(0)), 0);
        // 7 000 milisztuk tygodniowo = 1 000 na dobę; 3 000 na stanie = 3 doby.
        assert_eq!(ShelfRow::cover_of(Qty(3_000), Qty(7_000)), 3);
        // Brak sprzedaży to brak odpowiedzi, nie wieczność.
        assert_eq!(ShelfRow::cover_of(Qty(3_000), Qty(0)), u16::MAX);
    }
}
