//! Doba cenowa: obserwacja konkurencji, przecena, polityki cenowe (szew (c)).

use super::*;

/// Ile powodów przecen pamięta zakład śledzony. Tyle, ile mieści panel — pierścień
/// jest tu po to, żeby gracz zobaczył „czemu wczoraj potaniało", a nie po to, żeby
/// prowadzić historię cen. Historię prowadzą miesięczne domknięcia księgi.
const REPRICE_LOG: usize = 64;

// ── M5c: ceny i księgowość ───────────────────────────────────────────────────────

impl Market {
    /// Odświeżenie obrazu cen konkurencji (§6.3). Zwraca liczbę sklepów, które
    /// coś zobaczyły.
    ///
    /// Odświeżają się **tylko** sklepy, którym minęła własna czujność `delay_days`,
    /// więc sklep zwykle działa na starej cenie konkurenta — i to jest zamierzone:
    /// stąd biorą się realne błędy decyzyjne AI i pole manewru gracza (przecena
    /// na trzy dni, zanim konkurencja zauważy).
    ///
    /// Jedno zapytanie na **kategorię**, nie na towar: kategorii jest osiem, towarów
    /// czterdzieści, a `query_offers` i tak zwraca całą warstwę w promieniu.
    pub fn observe_competitors(&self, t: Tick) -> u64 {
        let mut m = self.lock();
        let promien_bazowy = m.data.pricing.observe_radius_m;
        let mut obs = std::mem::take(&mut m.obs_buf);
        let mut ofr = std::mem::take(&mut m.offer_buf);
        let mut wpisy = std::mem::take(&mut m.entry_buf);
        let mut ile = 0u64;

        for i in 0..m.shops.len() {
            if !m.shops[i].observed.is_stale(t) {
                continue;
            }
            let mut promien = promien_bazowy;
            // Promienie, o które pyta polityka zakładu. Obserwacja musi sięgnąć
            // najdalszego z nich, inaczej reguła „najtańszy w 8 km" widziałaby
            // wyłącznie sąsiadów z promienia obserwacji (`R2-WP22`).
            let pytane = m.shops[i].asked_radii;
            for r in pytane {
                promien = promien.max(r);
            }
            let mut nazwani: Vec<(GoodId, SiteId)> = Vec::new();
            for (g, pc) in &m.shops[i].controllers {
                if let PricePolicy::MatchCompetitor {
                    radius_m,
                    reference,
                    ..
                } = pc.policy
                {
                    promien = promien.max(radius_m);
                    if let CompetitorRef::Named(s) = reference {
                        nazwani.push((*g, s));
                    }
                }
            }
            let (pos, site) = (m.shops[i].pos, m.shops[i].site);

            // 1. Ceny konkurentów w promieniu, tylko dla towarów z naszej półki.
            obs.clear();
            let mut kategorie = [StockCat::Food; STOCK_CAT_COUNT];
            let mut n_kat = 0usize;
            for l in &m.shops[i].shelf.lines {
                if let Some(spec) = m.goods.spec(l.good) {
                    if !kategorie[..n_kat].contains(&spec.cat) && n_kat < STOCK_CAT_COUNT {
                        kategorie[n_kat] = spec.cat;
                        n_kat += 1;
                    }
                }
            }
            for c in &kategorie[..n_kat] {
                query_offers(&m.index, CategoryId::Stock(*c), pos, promien, &mut ofr);
                for id in &ofr {
                    let Some(o) = m.offers.get(*id) else { continue };
                    if o.site == site || m.shops[i].shelf.line(o.good).is_none() {
                        continue;
                    }
                    // Odległość do konkurenta — bez niej nie da się zawęzić obrazu
                    // do promienia, o który pyta reguła. Pozycję ma sklep, nie oferta.
                    let d = m
                        .by_site
                        .get(&o.site)
                        .map_or(u32::MAX, |j| odleglosc_m(pos, m.shops[*j as usize].pos));
                    obs.push((o.good, o.unit_price, o.price_rev, o.site, d));
                }
            }
            // Porządek `(towar, cena, sklep)` — najtańszy i mediana czytają się wprost,
            // a wynik nie zależy od kolejności zwracanej przez indeks.
            obs.sort_unstable_by_key(|(g, p, _, s, _)| (g.get(), p.get(), s.entity().index()));

            // 2. Zbicie do jednego wpisu na towar.
            wpisy.clear();
            let mut k = 0usize;
            while k < obs.len() {
                let good = obs[k].0;
                let mut j = k;
                let mut rev = 0u32;
                while j < obs.len() && obs[j].0 == good {
                    rev = rev.wrapping_add(obs[j].2);
                    j += 1;
                }
                let n = j - k;
                let named = nazwani
                    .iter()
                    .find(|(g, _)| *g == good)
                    .and_then(|(_, s)| price_of(&m, *s, good));
                wpisy.push(CompetitorEntry {
                    good,
                    cheapest: obs[k].1,
                    cheapest_site: obs[k].3,
                    median: obs[k + n / 2].1,
                    named,
                    offers: n as u32,
                    seen_at: t,
                    rev,
                    // Grupa `obs[k..j]` jest już posortowana po cenie, więc zawężenie
                    // do promienia to jedno przejście: pierwszy mieszczący się jest
                    // najtańszy, a środkowy z mieszczących się jest medianą.
                    near: zaw(&obs[k..j], pytane),
                });
                k = j;
            }
            if !wpisy.is_empty() {
                ile += 1;
            }
            let kopia = wpisy.clone();
            m.shops[i].observed.replace(kopia, t);
        }
        m.stats.observations += ile;
        obs.clear();
        ofr.clear();
        wpisy.clear();
        m.obs_buf = obs;
        m.offer_buf = ofr;
        m.entry_buf = wpisy;
        ile
    }

    /// Dobowy przelot polityk cenowych (§5.6). Zwraca liczbę zmienionych ofert.
    ///
    /// Woła się **po** [`Market::observe_competitors`] i to jest kontrakt kolejności:
    /// przecena konkurenta z doby `D` wchodzi do obrazu najwcześniej w dobie `D+1`,
    /// więc reakcja mieści się w 1..=7 dobach, tak jak żąda kryterium WP6. Odwrotna
    /// kolejność dopuszczałaby ósmą dobę.
    pub fn reprice_all(&self, t: Tick) -> u64 {
        let mut m = self.lock();
        let seed = m.seed;
        let chain = m.chain.clone();
        let ch = chain.lock();
        let cat = &chain.cat;
        let MarketInner {
            shops,
            offers,
            data,
            goods,
            tax,
            stats,
            ..
        } = &mut *m;
        let mut zmian = 0u64;
        for shop in shops.iter_mut() {
            let sledzony = shop.tracking != LostSaleTracking::None;
            let firm_index = shop.firm.entity().index();
            let (site, osobowosc) = (shop.site, shop.pricing);
            for li in 0..shop.shelf.lines.len() {
                let linia = shop.shelf.lines[li];
                let good = linia.good;
                let Some(spec) = goods.spec(good).copied() else {
                    continue;
                };
                // Zapas czyta się z magazynu: zaplecze i półka to od WP11 dwa sloty
                // tego samego zakładu, a nie dwa pola sklepu.
                let zaplecze = ch.store.shelf_state(shop.backroom, good);
                let polka = ch.store.shelf_state(shop.shelf_slot, good);
                let ilosc = cat
                    .good(good)
                    .units_of_mass(magnat_core::Mass(zaplecze.mass.0 + polka.mass.0))
                    .get();
                let koszt = zaplecze.cost_total.get() + polka.cost_total.get();
                // Koszt własny: średnia ważona zapasu, a przy pustym magazynie cena
                // hurtowa. Cena nie może zależeć od tego, czy akurat jest towar —
                // zależy od tego, ile kosztuje go zdobyć.
                let unit_cost = if ilosc > 0 && koszt > 0 {
                    Money(koszt).mul_ratio(PRICE_UNIT, ilosc)
                } else {
                    spec.wholesale_base
                };
                let cel = shop
                    .inventory
                    .reorder
                    .get(&good)
                    .map_or(SHELF_UNITS_PER_FACING, |r| r.target.get().max(1));
                let stock_bp = (ilosc.saturating_mul(BP) / cel).clamp(0, 200_000) as i32;
                let termin = match (zaplecze.expires_at, polka.expires_at) {
                    (Some(a), Some(b)) => Some(a.get().min(b.get())),
                    (a, b) => a.or(b).map(magnat_core::SimMinute::get),
                }
                .map(|e| {
                    u16::try_from(e.saturating_sub(t.get()) / magnat_core::time::MINUTES_PER_DAY)
                        .unwrap_or(u16::MAX)
                });
                let obserwacja = shop.observed.get(good).copied();
                let Some(pc) = shop.controllers.get_mut(&good) else {
                    continue;
                };
                let ctx = PricingCtx {
                    site,
                    good,
                    firm_index,
                    world_seed: seed,
                    unit_cost,
                    stock_bp_of_target: stock_bp,
                    days_to_expiry: termin,
                    firm: osobowosc,
                    observed: obserwacja,
                    params: &data.pricing,
                    tax: &**tax,
                };
                let Some(powod) = reprice(pc, &ctx, t) else {
                    continue;
                };
                let nowa = pc.current;
                if let Some(o) = offers.get_mut(linia.offer) {
                    // Zmiana ceny **nie brudzi indeksu** — indeks trzyma uchwyty,
                    // a cena czyta się z areny na żywo (§5.2, bez zmian od M5a).
                    o.set_price(nowa);
                }
                zmian += 1;
                if sledzony {
                    if shop.reprice_log.len() >= REPRICE_LOG {
                        shop.reprice_log.remove(0);
                    }
                    shop.reprice_log.push(powod);
                }
            }
        }
        stats.reprices += zmian;
        zmian
    }

    // ── polityki cenowe gracza (WP11) ────────────────────────────────────────────

    /// Ustawia politykę cenową towaru. **Ta sama funkcja dla gracza i dla AI** —
    /// różnica jest w `delegated`, nie w ścieżce kodu (§6.3 PRD).
    pub fn set_policy(
        &self,
        site: SiteId,
        good: GoodId,
        policy: PricePolicy,
        delegated: bool,
    ) -> bool {
        let mut m = self.lock();
        let Some(i) = m.by_site.get(&site).copied() else {
            return false;
        };
        let Some(pc) = m.shops[i as usize].controllers.get_mut(&good) else {
            return false;
        };
        pc.policy = policy;
        pc.delegated = delegated;
        true
    }

    /// „Co by się stało z ceną dziś" — podgląd polityki bez jej zatwierdzania (WP11).
    ///
    /// Woła dokładnie to samo składanie co [`Market::reprice_all`], więc podgląd nie
    /// ma jak rozjechać się z wykonaniem.
    #[must_use]
    pub fn preview_policy(&self, site: SiteId, good: GoodId, policy: PricePolicy) -> Option<Money> {
        let m = self.lock();
        let i = m.by_site.get(&site).copied()?;
        let shop = &m.shops[i as usize];
        let pc = shop.controllers.get(&good)?;
        let ilosc =
            m.backroom_units(i as usize, good).get() + m.shelf_units(i as usize, good).get();
        let unit_cost = m.unit_cost(i as usize, good);
        let cel = shop
            .inventory
            .reorder
            .get(&good)
            .map_or(SHELF_UNITS_PER_FACING, |r| r.target.get().max(1));
        let ctx = PricingCtx {
            site,
            good,
            firm_index: shop.firm.entity().index(),
            world_seed: m.seed,
            unit_cost,
            stock_bp_of_target: (ilosc.saturating_mul(BP) / cel).clamp(0, 200_000) as i32,
            days_to_expiry: None,
            firm: shop.pricing,
            observed: shop.observed.get(good).copied(),
            params: &m.data.pricing,
            tax: &*m.tax,
        };
        Some(preview_price(policy, pc, &ctx))
    }
}

/// Cena towaru we wskazanym sklepie — potrzebna wyłącznie przy `CompetitorRef::Named`.
fn price_of(m: &MarketInner, site: SiteId, good: GoodId) -> Option<Money> {
    let i = m.by_site.get(&site).copied()?;
    let line = m.shops[i as usize].shelf.line(good)?;
    m.offers.get(line.offer).map(|o| o.unit_price)
}

/// Odległość między dwiema pozycjami sklepów w metrach.
///
/// `sqrt` na `f64` jest tu dozwolone (`K-6`: podstawowa arytmetyka IEEE-754 jest
/// deterministyczna międzyplatformowo, zakazane są funkcje przestępne), a wynik idzie
/// przez zaokrąglenie do metra — więc dwa przebiegi tego samego ziarna dają tę samą
/// liczbę i to samo zawężenie obrazu.
fn odleglosc_m(a: magnat_spatial::Vec2, b: magnat_spatial::Vec2) -> u32 {
    let dx = f64::from(a.x - b.x);
    let dy = f64::from(a.y - b.y);
    let d = (dx * dx + dy * dy).sqrt();
    if d >= f64::from(u32::MAX) {
        u32::MAX
    } else {
        d as u32
    }
}

/// Zawęża obserwacje jednego towaru do promieni, o które pyta polityka.
///
/// Wejście jest **posortowane po cenie rosnąco** (tak je układa `observe_competitors`),
/// więc najtańszy mieszczący się w promieniu to pierwszy taki, a mediana to środkowy
/// z mieszczących się. Promień `0` znaczy „slot pusty" i zostaje pusty.
///
/// Zero ofert w promieniu daje slot z `offers == 0` i zerowymi kwotami — reguła czyta
/// to jako „nie wiem", a nie jako „najtańszy kosztuje zero". Rozróżnienie robi
/// `offers`, bo `Money::ZERO` jest poprawną ceną promocyjną.
fn zaw(
    grupa: &[(GoodId, Money, u32, SiteId, u32)],
    pytane: [u32; magnat_economy_near_radii::N],
) -> [crate::pricing::NearRadius; magnat_economy_near_radii::N] {
    let mut out = [crate::pricing::NearRadius::default(); magnat_economy_near_radii::N];
    for (slot, r) in pytane.into_iter().enumerate() {
        if r == 0 {
            continue;
        }
        let mut w_promieniu = grupa.iter().filter(|(_, _, _, _, d)| *d <= r);
        let Some(pierwszy) = w_promieniu.next() else {
            out[slot] = crate::pricing::NearRadius {
                radius_m: r,
                ..Default::default()
            };
            continue;
        };
        let ile = 1 + w_promieniu.count();
        let mediana = grupa
            .iter()
            .filter(|(_, _, _, _, d)| *d <= r)
            .nth(ile / 2)
            .map_or(pierwszy.1, |x| x.1);
        out[slot] = crate::pricing::NearRadius {
            radius_m: r,
            cheapest: pierwszy.1,
            median: mediana,
            offers: u32::try_from(ile).unwrap_or(u32::MAX),
        };
    }
    out
}

/// Skrót nazwy stałej — `MAX_NEAR_RADII` w postaci użytecznej w typie tablicy.
mod magnat_economy_near_radii {
    pub const N: usize = crate::pricing::MAX_NEAR_RADII;
}
