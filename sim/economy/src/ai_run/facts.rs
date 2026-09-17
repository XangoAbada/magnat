//! Zbieranie faktów dla AI firm i tablica publiczna cen (M7e WP10).
//!
//! Tutaj kończy się świat, a zaczyna [`magnat_firms::view::FirmView`]. Wszystko,
//! co firma wie, powstaje w tym pliku — i tylko to, co tu wpiszemy, firma zobaczy.
//! Granica jest wąska z rozmysłu: dopisanie pola do widoku ma być decyzją
//! o asymetrii informacji (§5.8), a nie odruchem.

use std::collections::BTreeMap;

use magnat_core::{DistrictId, GoodId, Money, SiteId, Tick};
use magnat_firms::view::{FirmView, GoodFacts, SiteFacts};
use magnat_firms::{FirmKey, Firms};

use crate::board::{ObservedPrice, PublicMarketBoard};
use crate::market::Market;
use crate::pricing::PricePolicy;
use crate::shop::ReorderPolicy;

use super::MAX_COVER;

impl Market {
    /// Dobowy zapis do tablicy publicznej (§5.8).
    ///
    /// Jedno przejście po półkach miasta, grupowanie po `(dzielnica, towar)`
    /// i jeden wpis na parę. Kolejność jest kolejnością sklepów w wektorze, czyli
    /// kolejnością zakładania — a wynik i tak nie zależy od niej, bo przed
    /// policzeniem mediany lista jest sortowana (00 §3.2).
    ///
    /// Zwraca liczbę zapisanych par.
    pub fn refresh_board(&self, board: &mut PublicMarketBoard, t: Tick) -> usize {
        let m = self.lock();
        let doba = u32::try_from(t.get() / magnat_core::time::MINUTES_PER_DAY).unwrap_or(0);
        // (dzielnica, towar, cena netto, zakład) — czwarty składnik rozstrzyga remis
        // cenowy, żeby „najtańszy" nie zależał od kolejności sklepów w wektorze.
        let mut wpisy: Vec<(u16, u16, i64, SiteId)> = Vec::new();
        for shop in &m.shops {
            if shop.closed {
                continue;
            }
            for linia in &shop.shelf.lines {
                let Some(pc) = shop.controllers.get(&linia.good) else {
                    continue;
                };
                let netto = m.tax.net_from_gross(linia.good, pc.current);
                if netto.get() <= 0 {
                    continue;
                }
                wpisy.push((shop.district, linia.good.get(), netto.get(), shop.site));
            }
        }
        wpisy.sort_unstable();
        let mut zapisane = 0usize;
        let mut i = 0usize;
        while i < wpisy.len() {
            let (d, g, _, _) = wpisy[i];
            let mut j = i;
            while j < wpisy.len() && wpisy[j].0 == d && wpisy[j].1 == g {
                j += 1;
            }
            let n = j - i;
            let najtanszy = wpisy[i];
            board.record(
                DistrictId(d),
                GoodId(g),
                doba,
                ObservedPrice {
                    cheapest_net: Money(najtanszy.2),
                    median_net: Money(wpisy[i + n / 2].2),
                    cheapest_site: Some(najtanszy.3),
                    offers: u16::try_from(n).unwrap_or(u16::MAX),
                },
            );
            zapisane += 1;
            i = j;
        }
        zapisane
    }

    /// Wymusza okno sprzedaży pary (zakład, towar) — wejście testów i scenariuszy
    /// balansatora (M7e WP14).
    ///
    /// Utrata udziału w rynku mierzy się porównaniem dwóch tygodni sprzedaży, a dwa
    /// tygodnie zakupów w mieście testowym to kilkaset tysięcy ticków na jedno
    /// zdanie, które test chce wypowiedzieć. Scenariusz `player-price-war` z M5e
    /// korzysta z tego samego wejścia i z tego samego powodu.
    ///
    /// Zwraca `false`, gdy takiego zakładu albo towaru nie ma.
    pub fn set_weekly_sales(&self, site: SiteId, good: GoodId, week: i32, prev: i32) -> bool {
        let mut m = self.lock();
        let Some(i) = m.by_site.get(&site).copied().map(|i| i as usize) else {
            return false;
        };
        let Some(pc) = m.shops[i].controllers.get_mut(&good) else {
            return false;
        };
        pc.week = [0; 7];
        pc.week[0] = week;
        pc.prev_7d = prev;
        true
    }

    /// Widełki marży osobowości cenowej zakładu. Zakład spoza rynku detalicznego
    /// (zakład produkcyjny, biuro) dostaje widełki neutralne — nie ma półki,
    /// więc nie ma czego przycinać.
    pub(super) fn widelki(&self, site: Option<SiteId>) -> (i32, i32) {
        let m = self.lock();
        site.and_then(|s| m.by_site.get(&s).copied())
            .map(|i| {
                let p = m.shops[i as usize].pricing;
                (p.min_margin_bp, p.max_margin_bp)
            })
            .unwrap_or((500, 4_000))
    }

    /// Fakty firmy: zakłady i towary na ich półkach. Zwraca gotówkę firmy albo
    /// `None`, jeśli firmy nie ma w rejestrze.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn zbierz(
        &self,
        firms: &Firms,
        board: &PublicMarketBoard,
        cash: &BTreeMap<SiteId, Money>,
        key: FirmKey,
        t: Tick,
        sites: &mut Vec<SiteFacts>,
        goods: &mut Vec<GoodFacts>,
    ) -> Option<Money> {
        sites.clear();
        goods.clear();
        let firma = firms.get(key)?;
        let lag = FirmView::lag_for(firma.sites.len(), &firma.personality);
        let mut gotowka = Money::ZERO;
        let m = self.lock();
        let chain = m.chain.clone();
        let ch = chain.lock();
        for id in &firma.sites {
            let Some(s) = firms.site(*id) else { continue };
            gotowka = Money(
                gotowka
                    .get()
                    .saturating_add(cash.get(id).copied().unwrap_or(Money::ZERO).get()),
            );
            let deleg = s.delegation.as_ref();
            sites.push(SiteFacts {
                site: *id,
                district: s.district,
                vacancies: s.vacancies(),
                headcount: u32::try_from(s.headcount()).unwrap_or(u32::MAX),
                months_in_loss: miesiace_straty(s),
                last_margin_bp: s.pnl.last().and_then(magnat_firms::SitePnlMonth::margin_bp),
                delegated: deleg.is_some(),
                needs_policy: deleg.is_some_and(|d| d.policy.rules.is_empty()),
                // Najrzadszą rolę zakładu wypełnia dopiero rynek pracy — tu zostaje
                // sam fakt wakatu, bo indeksu niedoboru rynek detaliczny nie widzi.
                scarcest_role: None,
            });
            let Some(i) = m.by_site.get(id).copied().map(|i| i as usize) else {
                continue;
            };
            if m.shops[i].closed {
                continue;
            }
            for linia in &m.shops[i].shelf.lines.clone() {
                let g = linia.good;
                let Some(pc) = m.shops[i].controllers.get(&g) else {
                    continue;
                };
                let zaplecze = ch.store.shelf_state(m.shops[i].backroom, g);
                let polka = ch.store.shelf_state(m.shops[i].shelf_slot, g);
                let ilosc = chain
                    .cat
                    .good(g)
                    .units_of_mass(magnat_core::Mass(zaplecze.mass.0 + polka.mass.0))
                    .get();
                let koszt = zaplecze.cost_total.get() + polka.cost_total.get();
                let unit_cost = if ilosc > 0 && koszt > 0 {
                    Money(koszt).mul_ratio(crate::supply::PRICE_UNIT, ilosc)
                } else {
                    m.goods.spec(g).map_or(Money::ZERO, |s| s.wholesale_base)
                };
                let tydzien = pc.turnover_7d().get();
                let dziennie = (tydzien / 7).max(0);
                let obs = board.observed(DistrictId(m.shops[i].district), g, lag);
                // Rywalem jest **cudza** oferta: własnej ceny nie liczymy jako
                // konkurencji, bo firma ścigałaby samą siebie w dół.
                let cudzy = obs.filter(|o| o.cheapest_site != Some(*id));
                goods.push(GoodFacts {
                    site: *id,
                    good: g,
                    own_price_net: m.tax.net_from_gross(g, pc.current),
                    unit_cost_net: unit_cost,
                    margin_bp: cel_marzy(pc, unit_cost, m.tax.net_from_gross(g, pc.current)),
                    stock_days: pokrycie(ilosc, dziennie),
                    restock_days: cel_zapasu_dni(m.shops[i].inventory.reorder.get(&g), dziennie),
                    rival_cheapest_net: cudzy.map(|o| o.cheapest_net),
                    rival_cheapest_site: cudzy.and_then(|o| o.cheapest_site),
                    rival_median_net: cudzy.map(|o| o.median_net),
                    rivals: cudzy.map_or(0, |o| o.offers.saturating_sub(1)),
                    rivals_before: obs_wczesniej(board, m.shops[i].district, g, lag),
                    sold_7d: tydzien,
                    sold_7d_prev: i64::from(pc.prev_7d),
                });
            }
        }
        let _ = t;
        Some(gotowka)
    }
}

/// Ile **kolejnych** ostatnich miesięcy zakład zamknął stratą.
///
/// Liczone od najnowszego wstecz i przerywane **pierwszym miesiącem bez pomiaru**:
/// zakład bez księgi nie jest zakładem nierentownym, tylko zakładem, o którym nic
/// nie wiadomo (`SitePnlMonth::margin_bp` zwraca wtedy `None`).
fn miesiace_straty(s: &magnat_firms::Site) -> u8 {
    let mut n = 0u8;
    for p in s.pnl.iter().rev() {
        match p.margin_bp() {
            Some(m) if m < 0 => n = n.saturating_add(1),
            _ => break,
        }
    }
    n
}

/// Pokrycie zapasu w dobach sprzedaży, przycięte do [`MAX_COVER`].
fn pokrycie(ilosc: i64, dziennie: i64) -> i32 {
    if ilosc <= 0 {
        0
    } else if dziennie > 0 {
        i32::try_from(ilosc / dziennie)
            .unwrap_or(MAX_COVER)
            .min(MAX_COVER)
    } else {
        MAX_COVER
    }
}

/// Cel zamówienia wyrażony w dobach — odwrotność przeliczenia, którym wykonawca
/// polityk zamienia `OrderUpTo(dni)` na masę. Brak reguły znaczy „siedem dób",
/// bo tyle wynosi domyślne pokrycie sklepu osiedlowego z M6.
fn cel_zapasu_dni(r: Option<&ReorderPolicy>, dziennie: i64) -> u16 {
    let Some(r) = r else { return 7 };
    let dzielnik = dziennie.max(1);
    u16::try_from((r.target.get() / dzielnik).clamp(0, i64::from(u16::MAX))).unwrap_or(7)
}

/// Cel marży, wokół którego składa się dzisiejsza cena.
///
/// Sterownik trzymający `Dynamic` albo `Markup` mówi go wprost; sterownik na cenie
/// stałej albo dopasowany do konkurenta **nie ma celu** i wtedy celem jest to, co
/// dzisiejsza cena faktycznie daje. Bez tego firma sterowana ręcznie wyglądałaby
/// dla własnej taktyki jak firma z marżą zero.
fn cel_marzy(pc: &crate::pricing::PriceController, unit_cost: Money, price_net: Money) -> i32 {
    match pc.policy {
        PricePolicy::Markup { target_margin_bp }
        | PricePolicy::Dynamic {
            target_margin_bp, ..
        } => target_margin_bp,
        PricePolicy::Fixed { .. } | PricePolicy::MatchCompetitor { .. } => {
            let koszt = unit_cost.get().max(1);
            (((price_net.get() - koszt).saturating_mul(10_000) / koszt).clamp(-100_000, 100_000))
                as i32
        }
    }
}

/// Ilu rywali widać było **przed** oknem obserwacji — drugi składnik warunku
/// „przybyło konkurencji" z WP14.
fn obs_wczesniej(board: &PublicMarketBoard, district: u16, good: GoodId, lag: u8) -> u16 {
    let starszy = lag
        .saturating_add(3)
        .min(crate::board::WINDOW_DAYS as u8 - 1);
    board
        .observed(DistrictId(district), good, starszy)
        .map_or(0, |o| o.offers.saturating_sub(1))
}
