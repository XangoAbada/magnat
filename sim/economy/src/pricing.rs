//! Polityki cenowe sklepu (M5c §5.6, PRD §6.3).
//!
//! # Jeden mechanizm, dwóch właścicieli
//!
//! PRD §6.3 obiecuje graczowi „ten sam zestaw narzędzi co AI", więc [`PricePolicy`]
//! i [`reprice`] są **te same** dla sklepu gracza i dla sklepu AI. Różnica leży
//! wyłącznie w tym, kto ustawia wariant i parametry: [`PriceController::delegated`]
//! mówi, że sterowanie oddano polityce, a nie że polityka jest inna. Osobna ścieżka
//! kodu dla gracza byłaby złamaniem tej obietnicy w sposób, którego nikt by nie
//! zauważył, dopóki obie ścieżki nie rozjechałyby się przy pierwszej zmianie.
//!
//! To ta sama linia, którą §6 dokumentu fazy prowadzi przez rynek pracy: mechanizm
//! (`reprice`) jest jednakowy dla każdego, parametry ([`FirmPricing`]) zależą
//! od osobowości firmy.
//!
//! # Gdzie są floaty, a gdzie ich nie ma
//!
//! Składanie ceny jest **w całości całkowitoliczbowe** i mieszka w
//! [`crate::kernel::next_price`] — to jest warunek P9 i powód, dla którego rdzeń
//! jest wydzielony. Float pojawia się w jednym miejscu: przy liczeniu elastyczności
//! `e = ln(q1/q0) / ln(p1/p0)`, której wynik steruje **parametrem polityki**,
//! a nie kwotą (§5.6, decyzja otwarta nr 11). Do stanu trwałego wchodzi
//! kwantyzowany do `i32` w punktach bazowych, więc hash nie widzi floata.
//! Logarytm idzie przez `core::det_math` (`K-6`), nie przez std.

use magnat_core::{
    det_math, rng, DecisionReason, GoodId, HashState, Money, PriceDriver, Qty, SiteId, StateHasher,
    StreamId, Tick,
};

use crate::data::PricingParams;
use crate::kernel::{clamp_to_margin, next_price_full, PriceInput, BP};
use crate::tax::TaxEngine;

/// Ile ticków ma doba — okresem polityki cenowej jest doba, nie minuta.
const MIN_PER_DAY: u64 = magnat_core::time::MINUTES_PER_DAY;

// ── polityka ─────────────────────────────────────────────────────────────────────

/// Do czyjej ceny odnosi się [`PricePolicy::MatchCompetitor`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CompetitorRef {
    Cheapest,
    Median,
    Named(SiteId),
}

/// Polityka cenowa. **Ten sam typ dla gracza i dla AI** (WP11).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PricePolicy {
    /// Cena ustawiona ręcznie; `reprice` jej nie rusza poza ogranicznikiem marży.
    Fixed { price: Money },
    /// Stały narzut nad kosztem własnym, bez reakcji na otoczenie.
    Markup { target_margin_bp: i32 },
    /// „−2 % względem najtańszego w 3 km" — kryterium WP11 wprost.
    MatchCompetitor {
        delta_bp: i32,
        radius_m: u32,
        reference: CompetitorRef,
    },
    /// Pełne składanie z §5.6: zapas, konkurencja, eksperyment, przecena.
    Dynamic {
        target_margin_bp: i32,
        floor_margin_bp: i32,
        ceil_margin_bp: i32,
    },
}

// ── osobowość cenowa firmy ───────────────────────────────────────────────────────

/// Cztery parametry, które w M5 zastępują osobowość właściciela z PRD §12, plus
/// skłonność do ryzyka i czujność na konkurencję.
///
/// Losowane **raz na firmę** strumieniem `StreamId::FirmPricing` z kluczem indeksu
/// firmy i tickiem 0, więc nie zależą od momentu postawienia sklepu ani od kolejności
/// stawiania. M7 podmienia źródło na osobowość właściciela; kształt zostaje.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FirmPricing {
    pub target_margin_bp: i32,
    pub min_margin_bp: i32,
    pub max_margin_bp: i32,
    pub k_stock: i32,
    pub k_comp: i32,
    /// 0..=100.
    pub risk: i32,
    /// Co ile dni firma odświeża obraz cen konkurencji — 1..=7. To jest jej
    /// „czujność" i źródło realnych błędów decyzyjnych AI: sklep działa na starej
    /// cenie konkurenta, a gracz ma dzięki temu pole manewru (przecena na trzy dni,
    /// zanim konkurencja zauważy).
    pub delay_days: u8,
}

impl FirmPricing {
    #[must_use]
    pub fn draw(world_seed: u64, firm_index: u32, p: &PricingParams) -> FirmPricing {
        let mut r = rng(world_seed, StreamId::FirmPricing, firm_index, Tick(0));
        // Kolejność losowań jest kontraktem: przestawienie zmienia charakter
        // wszystkich firm w każdym świecie z tego samego seeda.
        let target = p.target_margin_bp.pick(&mut r);
        let min = p.min_margin_bp.pick(&mut r);
        let max = p.max_margin_bp.pick(&mut r).max(min + 100);
        FirmPricing {
            target_margin_bp: target.clamp(min, max),
            min_margin_bp: min,
            max_margin_bp: max,
            k_stock: p.k_stock.pick(&mut r),
            k_comp: p.k_comp.pick(&mut r),
            risk: p.risk.pick(&mut r),
            delay_days: p.observe_delay_days.pick(&mut r).clamp(1, 7) as u8,
        }
    }
}

// ── obserwacja konkurencji ───────────────────────────────────────────────────────

/// Obraz cen konkurencji dla jednego towaru, **z opóźnieniem** (§6.3).
///
/// Plan §5.6 opisywał wpis per (sklep, towar). Tu jest agregat per towar i to jest
/// świadoma korekta (`W-2`): polityka pyta o najtańszego, o medianę albo o wskazanego
/// konkurenta, więc pełny cennik każdego sąsiada byłby 1,2 mln wpisów na metropolię
/// za daną, z której czyta się trzy liczby. Wskazany konkurent rozwiązuje się
/// w chwili obserwacji i ląduje w `named`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CompetitorEntry {
    pub good: GoodId,
    /// Ceny **brutto**, tak jak stoją w ofertach (`K-7`). Sprowadzenie do netto
    /// robi `reprice` przez `TaxEngine`.
    pub cheapest: Money,
    pub cheapest_site: SiteId,
    pub median: Money,
    /// Cena wskazanego konkurenta, jeśli polityka na niego patrzy.
    pub named: Option<Money>,
    pub offers: u32,
    pub seen_at: Tick,
    /// Suma liczników `Offer.price_rev` obserwowanych ofert. Zmiana tej sumy znaczy
    /// „konkurencja ruszyła cennik" i jest tańsza niż trzymanie jego kopii — pole
    /// powstało w M5a właśnie na to i do M5c nie miało wołającego.
    pub rev: u32,
}

/// Obraz konkurencji całego sklepu, posortowany po `GoodId`.
#[derive(Clone, Default, Debug)]
pub struct CompetitorSnapshot {
    entries: Vec<CompetitorEntry>,
    pub refreshed: Tick,
    /// 1..=7, stałe dla firmy.
    pub delay_days: u8,
}

impl CompetitorSnapshot {
    /// Pusty obraz o zadanej czujności firmy.
    #[must_use]
    pub fn new(delay_days: u8) -> CompetitorSnapshot {
        CompetitorSnapshot {
            entries: Vec::new(),
            refreshed: Tick(0),
            delay_days: delay_days.clamp(1, 7),
        }
    }

    #[must_use]
    pub fn get(&self, good: GoodId) -> Option<&CompetitorEntry> {
        self.entries
            .binary_search_by_key(&good, |e| e.good)
            .ok()
            .map(|i| &self.entries[i])
    }

    #[must_use]
    pub fn entries(&self) -> &[CompetitorEntry] {
        &self.entries
    }

    /// Podmienia cały obraz. `next` musi być posortowane po `GoodId` — pilnuje tego
    /// wołający, bo to on iteruje po półce, która już jest posortowana.
    pub fn replace(&mut self, next: Vec<CompetitorEntry>, at: Tick) {
        debug_assert!(next.windows(2).all(|w| w[0].good < w[1].good));
        self.entries = next;
        self.refreshed = at;
    }

    /// Czy minęło tyle dni, ile wynosi czujność firmy.
    ///
    /// Warunek jest `>=`, nie `>`: przy `>` odświeżenie wypadałoby w dniu
    /// `delay_days + 1` i reakcja na przecenę konkurenta mogłaby sięgnąć ósmego dnia,
    /// łamiąc kryterium WP6 („nie później niż po 7 dniach").
    #[must_use]
    pub fn is_stale(&self, now: Tick) -> bool {
        if self.entries.is_empty() {
            return true;
        }
        let dni = (now.get().saturating_sub(self.refreshed.get())) / MIN_PER_DAY;
        dni >= u64::from(self.delay_days.max(1))
    }
}

// ── eksperyment i elastyczność ───────────────────────────────────────────────────

/// Trwający eksperyment cenowy (§6.3): sklep celowo rusza cenę, żeby **zmierzyć**
/// reakcję popytu. Gracz widzi wynik w raporcie — to jest ta elastyczność, którą
/// PRD §6.4 pozwala mu zmierzyć zamiast odczytać.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PriceExperiment {
    /// `+1` w górę, `−1` w dół.
    pub direction: i8,
    pub magnitude_bp: i32,
    pub started: Tick,
    pub len_days: u8,
    /// Sprzedaż dobowa sprzed eksperymentu — punkt odniesienia `q0`.
    pub baseline_units: Qty,
    /// Suma sprzedaży w trakcie okna.
    pub window_units: Qty,
    pub days_elapsed: u8,
}

/// Zmierzona elastyczność cenowa popytu. **`i32` w punktach bazowych**, nie float:
/// wartość wchodzi do stanu trwałego, więc do hasha i do zapisu gry
/// (decyzja otwarta nr 11, zgodnie z 00 §2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ObservedElasticity {
    /// `e × 10 000`. Ujemna dla normalnego towaru.
    pub e_bp: i32,
    pub measured_at: Tick,
    pub samples: u32,
}

// ── sterownik ────────────────────────────────────────────────────────────────────

/// Stan cenowy pary (zakład, towar).
pub struct PriceController {
    pub policy: PricePolicy,
    /// Cena **brutto**, czyli ta, która stoi w ofercie (`K-7`).
    pub current: Money,
    pub last_change: Tick,
    pub elasticity: Option<ObservedElasticity>,
    pub experiment: Option<PriceExperiment>,
    /// `true` = gracz oddał sterowanie polityce. `false` przy `Fixed` znaczy
    /// „cena ustawiona ręcznie" i to jest cała różnica między graczem a AI.
    pub delegated: bool,
    /// Sztuki sprzedane w bieżącej dobie; dobowy `reprice` przenosi je niżej.
    pub sold_today: Qty,
    /// Sprzedaż poprzedniej doby — odniesienie eksperymentu.
    pub sold_yesterday: Qty,
    /// `None` = firma jeszcze nigdy nie eksperymentowała. Odróżnione od `Tick(0)`,
    /// bo inaczej żaden sklep nie ruszyłby eksperymentu przez pierwsze 30 dni świata
    /// — a to jest dokładnie okres, w którym balansator mierzy rozbieg.
    pub last_experiment: Option<Tick>,
}

impl PriceController {
    #[must_use]
    pub fn new(policy: PricePolicy, price: Money, t: Tick) -> PriceController {
        PriceController {
            policy,
            current: price,
            last_change: t,
            elasticity: None,
            experiment: None,
            delegated: true,
            sold_today: Qty::ZERO,
            sold_yesterday: Qty::ZERO,
            last_experiment: None,
        }
    }
}

/// Wszystko, czego [`reprice`] potrzebuje o świecie. Rozwiązanie encji na liczby
/// robi wołający — sterownik nie widzi ani areny, ani indeksu.
pub struct PricingCtx<'a> {
    pub site: SiteId,
    pub good: GoodId,
    /// Firma jako indeks encji — klucz strumienia eksperymentu.
    pub firm_index: u32,
    pub world_seed: u64,
    /// Koszt własny za `PRICE_UNIT` jednostek, **netto**.
    pub unit_cost: Money,
    /// Zapas (zaplecze + półka) w punktach bazowych celu zamówienia.
    pub stock_bp_of_target: i32,
    /// Pozostałe dni ważności najkrótszej linii, `None` = towar się nie psuje.
    pub days_to_expiry: Option<u16>,
    pub firm: FirmPricing,
    pub observed: Option<CompetitorEntry>,
    pub params: &'a PricingParams,
    pub tax: &'a dyn TaxEngine,
}

/// Jedna korekta ceny. Zwraca powód, jeśli cena się zmieniła — `None` znaczy
/// „polityka policzyła to samo, co stoi", a nie „nic nie policzono".
///
/// Kolejność kroków jest istotna: eksperyment domyka się **przed** złożeniem ceny,
/// bo jego wynik (elastyczność) wchodzi do tego samego składania.
pub fn reprice(pc: &mut PriceController, ctx: &PricingCtx<'_>, t: Tick) -> Option<DecisionReason> {
    // 1. Rytm dobowy sprzedaży — wejście eksperymentu.
    pc.sold_yesterday = pc.sold_today;
    pc.sold_today = Qty::ZERO;

    // 2. Eksperyment: domknięcie zakończonego, start nowego.
    let adj_elast = step_experiment(pc, ctx, t);

    // 3. Przecena psującego się (§5.6) — schodkowo wg pozostałego terminu.
    let adj_spoil = ctx.days_to_expiry.map_or(0, |d| {
        ctx.params
            .spoilage
            .iter()
            .find(|s| d <= s.days_left)
            .map_or(0, |s| s.adj_bp)
    });

    let stara = pc.current;
    let (nowa_netto, driver) = compose(pc, ctx, adj_elast, adj_spoil);
    // 4. **Ostatni krok**: netto → brutto (`K-7`). W M5 mnożnik 1.
    let nowa = ctx.tax.gross_from_net(ctx.good, nowa_netto);
    if nowa == stara {
        return None;
    }
    pc.current = nowa;
    pc.last_change = t;
    let delta_bp = if stara.get() > 0 {
        ((nowa.get() - stara.get()).saturating_mul(BP) / stara.get()).clamp(-32_000, 32_000) as i16
    } else {
        0
    };
    Some(DecisionReason::Repricing {
        site: ctx.site,
        good: ctx.good,
        driver,
        delta_bp,
    })
}

/// Cena, którą polityka policzyłaby **dziś**, bez ruszania stanu sterownika.
///
/// To jest podgląd „co by się stało z ceną" z WP11: gracz wybiera wariant, widzi
/// skutek, dopiero potem zatwierdza. Woła dokładnie to samo składanie co `reprice`,
/// więc podgląd nie może się rozjechać z wykonaniem.
#[must_use]
pub fn preview_price(policy: PricePolicy, pc: &PriceController, ctx: &PricingCtx<'_>) -> Money {
    let mut kopia = PriceController {
        policy,
        current: pc.current,
        last_change: pc.last_change,
        elasticity: pc.elasticity,
        experiment: None,
        delegated: pc.delegated,
        sold_today: pc.sold_today,
        sold_yesterday: pc.sold_yesterday,
        last_experiment: pc.last_experiment,
    };
    let adj_spoil = ctx.days_to_expiry.map_or(0, |d| {
        ctx.params
            .spoilage
            .iter()
            .find(|s| d <= s.days_left)
            .map_or(0, |s| s.adj_bp)
    });
    let (netto, _) = compose(&mut kopia, ctx, 0, adj_spoil);
    ctx.tax.gross_from_net(ctx.good, netto)
}

/// Złożenie ceny wg wariantu polityki. Zwraca kwotę **netto** i człon, który przeważył.
fn compose(
    pc: &mut PriceController,
    ctx: &PricingCtx<'_>,
    adj_elast: i32,
    adj_spoil: i32,
) -> (Money, PriceDriver) {
    let f = ctx.firm;
    match pc.policy {
        PricePolicy::Fixed { price } => {
            // Nawet cena ręczna nie schodzi pod koszt: ogranicznik jest mechanizmem
            // rynku, nie wyborem właściciela. Gracz, który chce sprzedawać ze stratą,
            // robi to obniżając marżę minimalną, a nie omijając regułę.
            let netto = ctx.tax.net_from_gross(ctx.good, price);
            let (p, pod, nad) = clamp_to_margin(netto, ctx.unit_cost, f.min_margin_bp, f.max_margin_bp);
            (p, driver_of_clamp(pod, nad, PriceDriver::Policy))
        }
        PricePolicy::Markup { target_margin_bp } => {
            let b = next_price_full(PriceInput {
                unit_cost: ctx.unit_cost,
                target_margin_bp,
                min_margin_bp: f.min_margin_bp,
                max_margin_bp: f.max_margin_bp,
                stock_bp_of_target: BP as i32,
                k_stock: 0,
                competitor_net: None,
                k_comp: 0,
                adj_elast_bp: 0,
                adj_spoil_bp: adj_spoil,
            });
            let d = if adj_spoil != 0 {
                PriceDriver::Spoilage
            } else {
                driver_of_clamp(b.at_floor, b.at_ceil, PriceDriver::Cost)
            };
            (b.price, d)
        }
        PricePolicy::MatchCompetitor {
            delta_bp,
            radius_m: _,
            reference,
        } => match reference_price(ctx, reference) {
            Some(ref_gross) => {
                let netto = ctx.tax.net_from_gross(ctx.good, ref_gross);
                let cel = netto.mul_ratio(BP + i64::from(delta_bp), BP);
                let (p, pod, nad) =
                    clamp_to_margin(cel, ctx.unit_cost, f.min_margin_bp, f.max_margin_bp);
                (p, driver_of_clamp(pod, nad, PriceDriver::Competitor))
            }
            // Nikogo nie widać — spadamy na własną marżę docelową. Cena nie może
            // zależeć od tego, czy zapytanie coś zwróciło.
            None => {
                let b = next_price_full(PriceInput {
                    unit_cost: ctx.unit_cost,
                    target_margin_bp: f.target_margin_bp,
                    min_margin_bp: f.min_margin_bp,
                    max_margin_bp: f.max_margin_bp,
                    stock_bp_of_target: BP as i32,
                    k_stock: 0,
                    competitor_net: None,
                    k_comp: 0,
                    adj_elast_bp: 0,
                    adj_spoil_bp: adj_spoil,
                });
                (b.price, PriceDriver::Cost)
            }
        },
        PricePolicy::Dynamic {
            target_margin_bp,
            floor_margin_bp,
            ceil_margin_bp,
        } => {
            let konkurent = ctx
                .observed
                .as_ref()
                .map(|e| ctx.tax.net_from_gross(ctx.good, e.cheapest));
            let b = next_price_full(PriceInput {
                unit_cost: ctx.unit_cost,
                target_margin_bp,
                min_margin_bp: floor_margin_bp,
                max_margin_bp: ceil_margin_bp,
                stock_bp_of_target: ctx.stock_bp_of_target,
                k_stock: f.k_stock,
                competitor_net: konkurent,
                k_comp: f.k_comp,
                adj_elast_bp: adj_elast,
                adj_spoil_bp: adj_spoil,
            });
            let (_, kod) = b.dominant_adj_bp();
            let d = match kod {
                0 => PriceDriver::Stock,
                1 => PriceDriver::Competitor,
                2 => PriceDriver::Experiment,
                3 => PriceDriver::Spoilage,
                _ => PriceDriver::Cost,
            };
            (b.price, driver_of_clamp(b.at_floor, b.at_ceil, d))
        }
    }
}

fn driver_of_clamp(at_floor: bool, at_ceil: bool, inaczej: PriceDriver) -> PriceDriver {
    if at_floor {
        PriceDriver::Floor
    } else if at_ceil {
        PriceDriver::Ceiling
    } else {
        inaczej
    }
}

fn reference_price(ctx: &PricingCtx<'_>, r: CompetitorRef) -> Option<Money> {
    let e = ctx.observed.as_ref()?;
    if e.offers == 0 {
        return None;
    }
    match r {
        CompetitorRef::Cheapest => Some(e.cheapest),
        CompetitorRef::Median => Some(e.median),
        CompetitorRef::Named(_) => e.named,
    }
}

// ── eksperymenty (§5.6) ──────────────────────────────────────────────────────────

/// Prowadzi eksperyment o jeden dzień i zwraca korektę `adj_elast` w bp.
///
/// Trzy stany: brak eksperymentu (może się zacząć), trwający (korekta = kierunek ×
/// magnituda), zakończony (liczy elastyczność, czyści się). Poza eksperymentem
/// korekta bierze się ze zmierzonej elastyczności: krok w stronę `p* = p · (1 + 1/(e+1))`
/// ograniczony do ±500 bp.
fn step_experiment(pc: &mut PriceController, ctx: &PricingCtx<'_>, t: Tick) -> i32 {
    if let Some(mut ex) = pc.experiment {
        ex.window_units = Qty(ex.window_units.get().saturating_add(pc.sold_yesterday.get()));
        ex.days_elapsed = ex.days_elapsed.saturating_add(1);
        if ex.days_elapsed < ex.len_days {
            pc.experiment = Some(ex);
            return i32::from(ex.direction) * ex.magnitude_bp;
        }
        // Okno zamknięte — mierzymy.
        pc.experiment = None;
        pc.last_experiment = Some(t);
        let q0 = ex.baseline_units.get().max(0) * i64::from(ex.len_days);
        let q1 = ex.window_units.get().max(0);
        if q0 > 0 && q1 > 0 {
            let dln_q = det_math::ln(q1 as f64 / q0 as f64);
            if (dln_q * 1_000.0).abs() >= f64::from(ctx.params.experiment_noise_permille) {
                let dln_p =
                    det_math::ln1p(f64::from(i32::from(ex.direction) * ex.magnitude_bp) / BP as f64);
                if dln_p.abs() > 1e-9 {
                    let e = dln_q / dln_p;
                    pc.elasticity = Some(ObservedElasticity {
                        e_bp: (e * BP as f64).clamp(-2_000_000.0, 2_000_000.0) as i32,
                        measured_at: t,
                        samples: u32::try_from(q1 / 1_000).unwrap_or(u32::MAX),
                    });
                }
            }
        }
        return elastic_step(pc);
    }

    // Start nowego: tylko firma skłonna do ryzyka, nie częściej niż co N dni
    // i tylko wtedy, gdy jest z czym porównywać.
    let cooldown = u64::from(ctx.params.experiment_cooldown_days) * MIN_PER_DAY;
    let gotowa = ctx.firm.risk >= ctx.params.experiment_risk_min
        && pc
            .last_experiment
            .is_none_or(|l| t.get() >= l.get().saturating_add(cooldown))
        && pc.sold_yesterday.get() > 0
        && matches!(pc.policy, PricePolicy::Dynamic { .. });
    if !gotowa {
        return elastic_step(pc);
    }
    let mut r = rng(
        ctx.world_seed,
        StreamId::PriceExperiment,
        ctx.firm_index,
        Tick(t.get() / MIN_PER_DAY),
    );
    let kierunek = if r.next_u32() & 1 == 0 { -1i8 } else { 1 };
    let magnituda = ctx.params.experiment_bp.pick(&mut r);
    pc.experiment = Some(PriceExperiment {
        direction: kierunek,
        magnitude_bp: magnituda,
        started: t,
        len_days: ctx.params.experiment_len_days.max(1),
        baseline_units: pc.sold_yesterday,
        window_units: Qty::ZERO,
        days_elapsed: 0,
    });
    i32::from(kierunek) * magnituda
}

/// Krok w stronę ceny optymalnej przy zmierzonej elastyczności: `p* = p · (1 + 1/(e+1))`.
///
/// Przy `e` bliskim `−1` wyrażenie wybucha (przychód przestaje zależeć od ceny),
/// więc wtedy korekty nie ma — to nie jest przypadek brzegowy do obsłużenia
/// „jakkolwiek", tylko obszar, w którym pomiar nic nie mówi.
fn elastic_step(pc: &PriceController) -> i32 {
    let Some(e) = pc.elasticity else { return 0 };
    let mianownik = e.e_bp + BP as i32;
    if mianownik.abs() < 1_000 {
        return 0;
    }
    let krok = (BP * BP / i64::from(mianownik)).clamp(-500, 500);
    krok as i32
}

// ── hash ─────────────────────────────────────────────────────────────────────────

impl HashState for PriceController {
    fn hash_state(&self, h: &mut StateHasher) {
        match self.policy {
            PricePolicy::Fixed { price } => {
                h.write_u8(0);
                price.hash_state(h);
            }
            PricePolicy::Markup { target_margin_bp } => {
                h.write_u8(1);
                h.write_i64(i64::from(target_margin_bp));
            }
            PricePolicy::MatchCompetitor {
                delta_bp,
                radius_m,
                reference,
            } => {
                h.write_u8(2);
                h.write_i64(i64::from(delta_bp));
                h.write_u32(radius_m);
                match reference {
                    CompetitorRef::Cheapest => h.write_u8(0),
                    CompetitorRef::Median => h.write_u8(1),
                    CompetitorRef::Named(s) => {
                        h.write_u8(2);
                        s.entity().hash_state(h);
                    }
                }
            }
            PricePolicy::Dynamic {
                target_margin_bp,
                floor_margin_bp,
                ceil_margin_bp,
            } => {
                h.write_u8(3);
                h.write_i64(i64::from(target_margin_bp));
                h.write_i64(i64::from(floor_margin_bp));
                h.write_i64(i64::from(ceil_margin_bp));
            }
        }
        self.current.hash_state(h);
        self.last_change.hash_state(h);
        match self.elasticity {
            Some(e) => {
                h.write_u8(1);
                h.write_i64(i64::from(e.e_bp));
                e.measured_at.hash_state(h);
            }
            None => h.write_u8(0),
        }
        match self.experiment {
            Some(e) => {
                h.write_u8(1);
                h.write_u8(e.direction as u8);
                h.write_i64(i64::from(e.magnitude_bp));
                e.started.hash_state(h);
                h.write_u8(e.days_elapsed);
                e.window_units.hash_state(h);
                e.baseline_units.hash_state(h);
            }
            None => h.write_u8(0),
        }
        h.write_u8(u8::from(self.delegated));
        self.sold_today.hash_state(h);
        self.sold_yesterday.hash_state(h);
        match self.last_experiment {
            Some(l) => {
                h.write_u8(1);
                l.hash_state(h);
            }
            None => h.write_u8(0),
        }
    }
}

impl HashState for CompetitorSnapshot {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.entries.len() as u32);
        for e in &self.entries {
            e.good.hash_state(h);
            e.cheapest.hash_state(h);
            e.median.hash_state(h);
            h.write_u32(e.offers);
            h.write_u32(e.rev);
            e.seen_at.hash_state(h);
        }
        self.refreshed.hash_state(h);
        h.write_u8(self.delay_days);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::EconomyData;
    use crate::tax::NoTax;
    use magnat_core::Entity;
    use std::num::NonZeroU32;

    fn site(i: u32) -> SiteId {
        SiteId(Entity::new(i, NonZeroU32::MIN))
    }

    struct Fix {
        data: EconomyData,
    }

    fn fix() -> Fix {
        Fix {
            data: EconomyData::load_default().expect("data/economy/"),
        }
    }

    impl Fix {
        fn ctx<'a>(&'a self, f: FirmPricing, obs: Option<CompetitorEntry>) -> PricingCtx<'a> {
            PricingCtx {
                site: site(1),
                good: GoodId(0),
                firm_index: 1,
                world_seed: 7,
                unit_cost: Money(200),
                stock_bp_of_target: 10_000,
                days_to_expiry: None,
                firm: f,
                observed: obs,
                params: &self.data.pricing,
                tax: &NoTax,
            }
        }
    }

    fn firma() -> FirmPricing {
        FirmPricing {
            target_margin_bp: 3_000,
            min_margin_bp: 500,
            max_margin_bp: 12_000,
            k_stock: 4_000,
            k_comp: 5_000,
            risk: 0,
            delay_days: 3,
        }
    }

    #[test]
    fn osobowosc_jest_stala_dla_firmy_i_rozna_miedzy_firmami() {
        let d = fix();
        let a = FirmPricing::draw(9, 11, &d.data.pricing);
        assert_eq!(a, FirmPricing::draw(9, 11, &d.data.pricing));
        let rozne = (0..50u32)
            .map(|i| FirmPricing::draw(9, i, &d.data.pricing).target_margin_bp)
            .collect::<std::collections::BTreeSet<_>>();
        assert!(rozne.len() > 5, "marże firm się nie różnicują: {rozne:?}");
        for i in 0..200u32 {
            let f = FirmPricing::draw(9, i, &d.data.pricing);
            assert!((1..=7).contains(&f.delay_days));
            assert!(f.min_margin_bp < f.max_margin_bp);
        }
    }

    #[test]
    fn zaleganie_obniza_a_braki_podnosza_w_ciagu_trzech_dni() {
        // Kryterium WP6: sklep z nadmiarem zapasu obniża cenę w ciągu 3 dni.
        let d = fix();
        let polityka = PricePolicy::Dynamic {
            target_margin_bp: 3_000,
            floor_margin_bp: 500,
            ceil_margin_bp: 12_000,
        };
        let mut nad = PriceController::new(polityka, Money(260), Tick(0));
        let mut brak = PriceController::new(polityka, Money(260), Tick(0));
        for dzien in 1..=3u64 {
            let t = Tick(dzien * MIN_PER_DAY);
            let mut c = d.ctx(firma(), None);
            c.stock_bp_of_target = 25_000;
            reprice(&mut nad, &c, t);
            let mut c = d.ctx(firma(), None);
            c.stock_bp_of_target = 1_000;
            reprice(&mut brak, &c, t);
        }
        assert!(nad.current < Money(260), "nadmiar: {:?}", nad.current);
        assert!(brak.current > Money(260), "braki: {:?}", brak.current);
    }

    #[test]
    fn polityka_gracza_i_ai_daja_te_sama_cene() {
        // Kryterium WP11: „−2 % względem najtańszego konkurenta w promieniu 3 km"
        // ustawiona przez gracza i przez AI daje **identyczną** cenę przy identycznym
        // stanie. Różnica jest wyłącznie we fladze `delegated`.
        let d = fix();
        let polityka = PricePolicy::MatchCompetitor {
            delta_bp: -200,
            radius_m: 3_000,
            reference: CompetitorRef::Cheapest,
        };
        let obs = CompetitorEntry {
            good: GoodId(0),
            cheapest: Money(250),
            cheapest_site: site(2),
            median: Money(270),
            named: None,
            offers: 4,
            seen_at: Tick(0),
            rev: 0,
        };
        let mut gracz = PriceController::new(polityka, Money(300), Tick(0));
        gracz.delegated = true;
        let mut ai = PriceController::new(polityka, Money(300), Tick(0));
        ai.delegated = false;
        let c = d.ctx(firma(), Some(obs));
        reprice(&mut gracz, &c, Tick(MIN_PER_DAY));
        reprice(&mut ai, &c, Tick(MIN_PER_DAY));
        assert_eq!(gracz.current, ai.current);
        assert_eq!(gracz.current, Money(245), "250 gr − 2 % = 245 gr");
    }

    #[test]
    fn cena_nie_schodzi_pod_koszt_nawet_gdy_konkurent_rozdaje() {
        let d = fix();
        let obs = CompetitorEntry {
            good: GoodId(0),
            cheapest: Money(1),
            cheapest_site: site(2),
            median: Money(1),
            named: None,
            offers: 1,
            seen_at: Tick(0),
            rev: 0,
        };
        let mut pc = PriceController::new(
            PricePolicy::MatchCompetitor {
                delta_bp: -200,
                radius_m: 3_000,
                reference: CompetitorRef::Cheapest,
            },
            Money(300),
            Tick(0),
        );
        reprice(&mut pc, &d.ctx(firma(), Some(obs)), Tick(MIN_PER_DAY));
        assert_eq!(pc.current, Money(210), "podłoga: koszt 200 + 5 %");
    }

    #[test]
    fn przecena_psujacego_sie_schodzi_schodkami() {
        let d = fix();
        let polityka = PricePolicy::Markup {
            target_margin_bp: 3_000,
        };
        let ceny: Vec<i64> = [None, Some(2u16), Some(1), Some(0)]
            .into_iter()
            .map(|dni| {
                let mut pc = PriceController::new(polityka, Money(0), Tick(0));
                let mut c = d.ctx(firma(), None);
                c.days_to_expiry = dni;
                reprice(&mut pc, &c, Tick(MIN_PER_DAY));
                pc.current.get()
            })
            .collect();
        assert!(
            ceny.windows(2).all(|w| w[0] > w[1]),
            "kolejne progi mają obniżać cenę: {ceny:?}"
        );
        assert!(
            ceny[3] < 200,
            "w dniu ważności towar schodzi poniżej kosztu: {ceny:?}"
        );
    }

    #[test]
    fn eksperyment_rusza_cene_i_konczy_sie_pomiarem() {
        let d = fix();
        let mut f = firma();
        f.risk = 100;
        let polityka = PricePolicy::Dynamic {
            target_margin_bp: 3_000,
            floor_margin_bp: 500,
            ceil_margin_bp: 12_000,
        };
        let mut pc = PriceController::new(polityka, Money(260), Tick(0));
        pc.sold_today = Qty(10_000);
        // Dzień 1 — start eksperymentu, cena odchodzi od 260.
        reprice(&mut pc, &d.ctx(f, None), Tick(MIN_PER_DAY));
        assert!(pc.experiment.is_some(), "eksperyment miał ruszyć");
        assert_ne!(pc.current, Money(260));
        // Sprzedaż w oknie spada o połowę — popyt reaguje.
        for dzien in 2..=8u64 {
            pc.sold_today = Qty(5_000);
            reprice(&mut pc, &d.ctx(f, None), Tick(dzien * MIN_PER_DAY));
        }
        assert!(pc.experiment.is_none(), "okno miało się domknąć");
        let e = pc.elasticity.expect("elastyczność zmierzona");
        assert!(e.e_bp != 0);
        // Spadek sprzedaży przy zmianie ceny → elastyczność ujemna dla podwyżki
        // i dodatnia dla obniżki; znak zależy od wylosowanego kierunku, więc
        // sprawdzamy to, co jest niezależne: pomiar w ogóle zaszedł i ma próbki.
        assert!(e.samples > 0);
    }
}
