//! Doba giełdy: publikacja, zlecenia, fixing, rozliczenie, progi (M10d WP10.10–10.11).
//!
//! # Dlaczego osobny system, a nie krok w `economy.Market`
//!
//! Ten sam powód, dla którego dostały go rynek pracy (M7b) i niewypłacalność (M7d):
//! fixing jest **raz na dobę**, a `MarketSystem` chodzi co minutę i jest najgorętszym
//! systemem symulacji. Kolejność jest zadeklarowana jawnie (`K-42`): **po**
//! niewypłacalności, bo firma, która właśnie upadła, nie ma czego notować, a jej
//! udziały dzieli postępowanie, nie rynek.
//!
//! # Czym jest jedna sesja
//!
//! 1. **Publikacja.** Firmie, której wypada T+45, aktualizuje się [`Published`] —
//!    i to jest jedyny moment, w którym wycena fundamentalna w ogóle się rusza.
//! 2. **Zlecenia.** Gospodarstwa i firmy, którym wypadł dzień decyzji, składają je
//!    do arkusza ([`super::invest`]).
//! 3. **Fixing.** Jedna cena na spółkę, przydziały z [`super::book::fixing`].
//! 4. **Rozliczenie.** Pieniądz przez [`Books`], udział przez `Firm.owners`.
//! 5. **Progi.** 5 % → ujawnienie, ponad 50 % → zmiana zarządu.
//! 6. **Miesiąc.** Dywidenda i emisja.
//!
//! # Kto komu płaci
//!
//! Wszystkie przelewy sesji idą **przez konto reszty świata** i to jest świadome:
//! aukcja ma jedną cenę, więc suma wpłat równa się sumie wypłat co do grosza, a konto
//! przelotowe domyka się w tej samej dobie. Alternatywa — przelew wprost od kupującego
//! do sprzedającego — wymagałaby parowania zleceń, którego fixing z definicji nie robi,
//! a dla pary „gospodarstwo → gospodarstwo" nie zostawiłaby w dzienniku ani jednego
//! zapisu, bo gospodarstwo nie ma konta w księgach (M5b).

use magnat_agents::{Household, Identity, Personality, Population};
use magnat_core::{Cadence, CitizenId, DecisionReason, Entity, Money, SimMinute, Tick, TraitId};
use magnat_ecs::{System, SystemCtx, SystemDesc, SystemId, World};
use magnat_firms::{firm_id, FirmKey, FirmStatus, Firms, Owner};

use crate::credit::BaseRate;
use crate::market::Market;

use super::book::{fixing, Side, StockOrder};
use super::corp;
use super::invest;
use super::pay::{self, odbierz, player_citizen, saldo, zaplac};
use super::value::{self, Published};
use super::{day_of, month_of, Equity, WHOLE_BP};

/// Ile bp udziału trafia do wolnego obrotu przy debiucie.
pub const IPO_FLOAT_BP: u16 = 2_500;

/// Ile miesięcy firma musi mieć za sobą, zanim w ogóle rozważy debiut.
///
/// Trzy, a nie sześć, i to jest arytmetyka, nie gust: wynik miesiąca `m` publikuje
/// się w dobie `(m + 1) × 30 + 45` (`GD-3`), więc pierwszy **opublikowany** wynik
/// istnieje dopiero w dobie 75. Przy sześciu miesiącach pierwszy debiut wypadałby
/// w dobie 180, czyli pół roku gry po tym, jak warunek stał się spełnialny —
/// a przebieg krótszy niż to pokazywałby zero notowań i miałby rację.
pub const IPO_MIN_MONTHS: u32 = 3;

/// Wpina giełdę do świata i do funkcji haszującej stan (00 §3.6).
pub fn register_equity(world: &mut World, eq: Equity) {
    world.insert_resource(eq);
    world.register_resource_hash::<Equity>();
}

/// Co się w tej dobie stało — do raportu scenariusza i do testów.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct EquityDay {
    pub listed: u32,
    pub debuts: u32,
    pub orders: u32,
    pub fixings: u32,
    pub volume_bp: u32,
    pub turnover: Money,
    pub disclosures: u32,
    pub takeovers: u32,
    pub dividends: Money,
    pub issues: u32,
}

/// Dobowa sesja giełdowa.
pub struct EquitySystem {
    desc: SystemDesc,
    last: EquityDay,
}

impl EquitySystem {
    #[must_use]
    pub fn new() -> EquitySystem {
        EquitySystem {
            desc: SystemDesc::new("economy.Equity", Cadence::EveryDay)
                .exclusive()
                .after_if_present(SystemId::from_name("economy.Insolvency"))
                // Po ubezpieczeniach, bo wypłata odszkodowania zmienia saldo firmy,
                // a z salda liczy się jej wartość księgowa.
                .after_if_present(SystemId::from_name("economy.Insurance")),
            last: EquityDay::default(),
        }
    }

    #[must_use]
    pub fn last(&self) -> EquityDay {
        self.last
    }
}

impl Default for EquitySystem {
    fn default() -> EquitySystem {
        EquitySystem::new()
    }
}

impl System for EquitySystem {
    fn desc(&self) -> &SystemDesc {
        &self.desc
    }

    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let t = ctx.tick;
        self.last = step_day(ctx.world_mut(), t);
    }
}

/// Jedna doba giełdy. Wolna funkcja, bo woła ją i system, i scenariusz, i test —
/// budowanie harmonogramu po to, żeby przesunąć jedną dobę, jest kosztem bez korzyści.
pub fn step_day(world: &mut World, t: Tick) -> EquityDay {
    let Some(market) = world.get_resource::<Market>().cloned() else {
        return EquityDay::default();
    };
    let Some(mut eq) = world.get_resource_mut::<Equity>().map(std::mem::take) else {
        return EquityDay::default();
    };
    let day = day_of(t);
    let mut raport = EquityDay::default();

    eq.drop_expired(SimMinute(t.0));
    wyrejestruj_upadle(world, &mut eq);
    publikuj(world, &market, &mut eq, day, t);
    raport.debuts = debiuty(world, &mut eq, day, t);
    raport.orders = zlecenia(world, &market, &mut eq, day, t);
    sesja(world, &market, &mut eq, day, t, &mut raport);
    if t.0.is_multiple_of(60 * 24 * 30) && day > 0 {
        raport.dividends = super::month::dywidendy(world, &market, &mut eq, t);
        raport.issues = super::month::emisje(world, &market, &mut eq, t);
    }
    raport.listed = eq.listed_count() as u32;

    *world.resource_mut::<Equity>() = eq;
    raport
}

/// Zdejmuje z notowań firmy, które przestały być czynne.
///
/// Upadłość prowadzi M7d i to ona zmienia `FirmStatus`; giełda tylko wyciąga z tego
/// wniosek. Bez tego kroku spółka po upadłości dalej publikowałaby wyniki i wypłacała
/// dywidendę z konta, którym rozporządza już postępowanie.
fn wyrejestruj_upadle(world: &World, eq: &mut Equity) {
    let Some(firms) = world.get_resource::<Firms>() else {
        return;
    };
    let padle: Vec<FirmKey> = eq
        .listings()
        .map(|l| l.firm)
        .filter(|k| firms.get(*k).is_none_or(|f| f.status != FirmStatus::Active))
        .collect();
    for k in padle {
        eq.delist(k);
    }
}

// ── 1. publikacja wyników ────────────────────────────────────────────────────────

/// Aktualizuje opublikowany obraz firm, którym wypada T+45.
///
/// Wynik dwunastu miesięcy liczy się z pierścienia `Site.pnl` **wszystkich** zakładów
/// firmy, a nie z jednego: firma z trzema zakładami ma jeden zysk, a nie trzy.
/// Do M10c rachunek wyniku fabryki był zawsze zerem (`K-75`) i wycena firmy
/// produkcyjnej stanęłaby na liczbie, której nie ma.
fn publikuj(world: &mut World, market: &Market, eq: &mut Equity, day: u32, t: Tick) {
    let Some(month) = value::published_month(day) else {
        return;
    };
    let seed = world.seed;
    let szum = eq.params().earnings_noise_bp;
    let Some(firms) = world.get_resource::<Firms>() else {
        return;
    };
    let mut nowe: Vec<(FirmKey, Published)> = Vec::new();
    for (key, firm) in firms.iter() {
        if firm.status != FirmStatus::Active {
            continue;
        }
        let mut zysk = 0i64;
        let mut ksiegowa = 0i64;
        // `Option`, a nie zero: zero jest **prawdziwym** indeksem encji, więc
        // wartownik z niego robiłby jeden klucz szumu dla firm bez zakładów
        // i dla firmy o zakładzie numer zero (znalezisko recenzji M10d).
        let mut pierwszy_zaklad: Option<u32> = None;
        for site in &firm.sites {
            let Some(s) = firms.site(*site) else {
                continue;
            };
            pierwszy_zaklad.get_or_insert(site.0.index());
            for w in s.pnl.iter() {
                if w.month <= month && w.month + 12 > month {
                    zysk = zysk.saturating_add(w.result().get());
                }
            }
            ksiegowa = ksiegowa.saturating_add(market.equity_of(*site).get());
        }
        nowe.push((
            key,
            Published {
                month,
                profit_12m: value::reported(
                    Money(zysk),
                    szum,
                    seed,
                    // Firma bez zakładu nie ma czego zaszumić — jej wynik jest zerem
                    // i szum nic z nim nie zrobi; klucz jest wtedy kluczem firmy.
                    pierwszy_zaklad.unwrap_or_else(|| firm_id(key).0.index()),
                    t,
                ),
                book: Money(ksiegowa),
            },
        ));
    }
    // Saldo rachunku firmy dolicza się osobno, bo księga zakładu go nie widzi:
    // konto prowadzi `Books` pod `AccountOwner::Firm`, a nie księga sklepu.
    let mut salda: Vec<(FirmKey, Money)> = Vec::with_capacity(nowe.len());
    for (key, _) in &nowe {
        let konto = market.account_of_firm(firm_id(*key));
        salda.push((*key, saldo(world, konto)));
    }
    for ((key, mut p), (_, saldo)) in nowe.into_iter().zip(salda) {
        p.book = Money(p.book.get().saturating_add(saldo.get().max(0)));
        eq.set_published(key, p);
    }
}

// ── 2. debiuty ───────────────────────────────────────────────────────────────────

/// Kto wchodzi na giełdę.
///
/// Warunek jest jeden i wynika z tego, po co się na giełdę wchodzi: firma musi mieć
/// **opublikowany dodatni wynik** (inaczej nie ma czego wyceniać) i kurs, który każe
/// jej rosnąć szybciej, niż pozwala własny zysk. Debiut jest ofertą **dotychczasowych
/// właścicieli**, nie emisją: [`IPO_FLOAT_BP`] udziału idzie na rynek, a pieniądz do
/// nich. Emisja — czyli pieniądz do firmy — jest osobnym mechanizmem (WP10.11),
/// bo to są dwie różne decyzje o dwóch różnych skutkach dla akcjonariatu.
fn debiuty(world: &mut World, eq: &mut Equity, day: u32, t: Tick) -> u32 {
    if !day.is_multiple_of(30) {
        return 0;
    }
    if month_of(t) < IPO_MIN_MONTHS {
        return 0;
    }
    // Firmy, które **chcą** wejść. Warunek wykonalności sprawdza `debut` — jeden
    // dla AI i dla gracza (`K-11`), bo inaczej byłyby dwie giełdy z dwoma progami.
    let chetne: Vec<FirmKey> = {
        let Some(firms) = world.get_resource::<Firms>() else {
            return 0;
        };
        firms
            .iter()
            .filter(|(_, f)| f.status == FirmStatus::Active && invest::growth_minded(f.strategy))
            .map(|(k, _)| k)
            .collect()
    };
    let mut ile = 0;
    for key in chetne {
        if debut(world, eq, key, t) {
            ile += 1;
        }
    }
    ile
}

/// Wprowadza **jedną** firmę na giełdę. `false` = nie spełnia warunku.
///
/// Wejście dla obu stron: raz na miesiąc woła je AI dla firm o kursie na wzrost,
/// a komenda gracza — dla jego własnej (M10g WP10.21). Druga ścieżka debiutu
/// rozjechałaby się z pierwszą przy pierwszej zmianie progu, a próg jest tu istotą:
/// firma musi mieć **opublikowany dodatni wynik**, inaczej nie ma czego wyceniać.
///
/// Debiut jest ofertą **dotychczasowych właścicieli**, nie emisją: [`IPO_FLOAT_BP`]
/// udziału idzie na rynek, a pieniądz do nich. Emisja — czyli pieniądz do firmy —
/// jest osobnym mechanizmem (WP10.11), bo to są dwie różne decyzje o dwóch różnych
/// skutkach dla akcjonariatu.
pub fn debut(world: &mut World, eq: &mut Equity, key: FirmKey, t: Tick) -> bool {
    if eq.listing(key).is_some() {
        return false;
    }
    if world
        .get_resource::<Firms>()
        .and_then(|f| f.get(key))
        .is_none_or(|f| f.status != FirmStatus::Active)
    {
        return false;
    }
    let pe = pe_of(world, eq);
    let Some(p) = eq.published(key) else {
        return false;
    };
    if p.profit_12m.get() <= 0 {
        return false;
    }
    let cena = Money(value::fundamental(p, pe).get() / i64::from(WHOLE_BP));
    if cena.get() <= 0 {
        return false;
    }
    if !eq.list(key, IPO_FLOAT_BP, cena, SimMinute(t.0)) {
        return false;
    }
    // Wolny obrót powstaje z pakietu największego właściciela — to on wystawia
    // zlecenie sprzedaży na pierwszą sesję.
    let sprzedajacy = world
        .get_resource::<Firms>()
        .and_then(|f| f.get(key))
        .and_then(|firm| corp::preemptive_holder(firm).map(|o| (o, super::stake_of(firm, o))));
    if let Some((wlasciciel, ma)) = sprzedajacy {
        let bp = IPO_FLOAT_BP.min(ma);
        if bp > 0 {
            eq.place_order(
                key,
                wlasciciel,
                Side::Sell,
                cena,
                bp,
                SimMinute(t.0 + 60 * 24 * 7),
            );
        }
    }
    if let Some(firms) = world.get_resource_mut::<Firms>() {
        firms.log(
            key,
            t,
            DecisionReason::StockListed {
                firm: firm_id(key),
                price: cena,
            },
        );
    }
    true
}

fn pe_of(world: &World, eq: &Equity) -> u32 {
    let stopa = world
        .get_resource::<BaseRate>()
        .map_or(500, |b| b.bp.max(1) as u32);
    value::price_earnings(stopa, eq.params().pe_min, eq.params().pe_max)
}

// ── 3. zlecenia ──────────────────────────────────────────────────────────────────

/// Zbiera zamiary inwestorów i wpisuje je do arkuszy.
///
/// `ponytail:` 152 linie przy progu ostrzeżenia 150. Sufit nazwany: dwie pętle po
/// dwóch rodzajach inwestora, każda ze swoim pokryciem i swoim kluczem szumu.
/// Wydzielenie ich to dwie funkcje o siedmiu wspólnych argumentach — droga wyjścia,
/// gdy dojdzie trzeci rodzaj inwestora, a nie wcześniej.
fn zlecenia(world: &mut World, market: &Market, eq: &mut Equity, day: u32, t: Tick) -> u32 {
    let notowane: Vec<(FirmKey, Money, i16)> = eq
        .listings()
        .map(|l| (l.firm, l.last_fixing, l.rumor_bp))
        .collect();
    if notowane.is_empty() {
        return 0;
    }
    let seed = world.seed;
    let pe = pe_of(world, eq);
    let params = *eq.params();
    let mut zamiary: Vec<(FirmKey, invest::Intent)> = Vec::new();

    // ── gospodarstwa ──
    let mieszkancy: Vec<Entity> = world
        .get_resource::<Population>()
        .map(|p| {
            p.citizens()
                .iter()
                .copied()
                .filter(|e| invest::household_decides(day, e.index()))
                .collect()
        })
        .unwrap_or_default();
    for e in mieszkancy {
        let Some(id) = world.get::<Identity>(e).copied() else {
            continue;
        };
        if id.flags & Identity::FLAG_ALIVE == 0 {
            continue;
        }
        let Some(h) = magnat_agents::demography::household_by_index(world, id.household) else {
            continue;
        };
        let Some(gd) = world.get::<Household>(h).copied() else {
            continue;
        };
        let majatek = Money(gd.savings.get().max(0) + gd.bank.get().max(0));
        if majatek < params.investor_wealth_min {
            continue;
        }
        let budzet = Money(
            (majatek.get() - params.investor_wealth_min.get()).max(0)
                * i64::from(params.investor_stake_bp)
                / i64::from(WHOLE_BP),
        );
        let ryzyko = world
            .get::<Personality>(e)
            .map_or(50, |p| p.get(TraitId::Risk).get());
        let kto = Owner::Citizen(CitizenId(e));
        for (key, kurs, plotka) in &notowane {
            let Some(p) = eq.published(*key) else {
                continue;
            };
            let f = value::fundamental(p, pe);
            let wiara = value::belief(
                f,
                params.noise_household_bp,
                *plotka,
                seed,
                e.index(),
                key.0,
                invest::decision_tick(day),
            );
            let ma = world
                .get_resource::<Firms>()
                .and_then(|fs| fs.get(*key))
                .map_or(0, |fm| super::stake_of(fm, kto));
            if let Some(i) = invest::household_intent(kto, wiara, *kurs, ma, budzet, ryzyko) {
                zamiary.push((*key, i));
            }
        }
    }

    // ── firmy szukające przejęcia ──
    let kandydaci: Vec<(FirmKey, FirmKey, u16)> = {
        let Some(firms) = world.get_resource::<Firms>() else {
            return 0;
        };
        let mut v = Vec::new();
        for (key, firm) in firms.iter() {
            if firm.status != FirmStatus::Active || !invest::firm_decides(day, key.0) {
                continue;
            }
            if !invest::growth_minded(firm.strategy) {
                continue;
            }
            for (cel, _, _) in &notowane {
                if *cel == key {
                    continue;
                }
                let Some(c) = firms.get(*cel) else { continue };
                v.push((key, *cel, super::stake_of(c, Owner::Firm(key))));
            }
        }
        v
    };
    for (key, cel, ma) in kandydaci {
        let konto = market.account_of_firm(firm_id(key));
        let stac = saldo(world, konto);
        if stac.get() <= 0 {
            continue;
        }
        let Some(l) = eq.listing(cel).copied() else {
            continue;
        };
        // Wezwanie jest przycięte do **własnego przekonania** przejmującego: raider,
        // który uważa spółkę za wartą mniej, niż musiałby zapłacić z premią, nie
        // składa wezwania. Bez tego `noise_firm_bp` byłby liczbą w pliku, której
        // nikt nie czyta, a firma płaciłaby każdą cenę (znalezisko recenzji M10d).
        let wiara = eq.published(cel).map_or(Money::ZERO, |p| {
            value::belief(
                value::fundamental(p, pe),
                params.noise_firm_bp,
                l.rumor_bp,
                seed,
                firm_id(key).0.index(),
                cel.0,
                invest::decision_tick(day),
            )
        });
        if let Some(i) = invest::takeover_intent(
            Owner::Firm(key),
            l.last_fixing,
            ma,
            stac,
            params.tender_premium_bp,
        ) {
            if wiara >= i.limit {
                zamiary.push((cel, i));
            }
        }
    }

    let mut ile = 0;
    for (key, i) in zamiary {
        if eq
            .place_order(
                key,
                i.holder,
                i.side,
                i.limit,
                i.bp,
                SimMinute(t.0 + 60 * 24),
            )
            .is_some()
        {
            ile += 1;
        }
    }
    ile
}

// ── 4. sesja ─────────────────────────────────────────────────────────────────────

/// `ponytail:` 167 linii przy progu ostrzeżenia 150. Sufit nazwany: to jest **jedna
/// sesja** i jej kroki są ze sobą związane kolejnością (zlecenia → fixing → pieniądz
/// → udział → progi → plotka). Rozcięcie ich na funkcje znaczyłoby przekazywanie
/// sześciu wektorów między nimi, czyli ten sam kod plus sygnatury. Droga wyjścia,
/// gdyby doszedł siódmy krok: wydzielić rozliczenie pieniądza razem z przenoszeniem
/// udziału, bo to jedyna para, która nie potrzebuje reszty.
fn sesja(
    world: &mut World,
    market: &Market,
    eq: &mut Equity,
    day: u32,
    t: Tick,
    raport: &mut EquityDay,
) {
    let klucze: Vec<FirmKey> = eq.listings().map(|l| l.firm).collect();
    for key in klucze {
        let prev = eq.listing(key).map_or(Money::ZERO, |l| l.last_fixing);
        let zlecenia = waliduj(world, market, key, eq.take_orders(key));
        gasnij_plotke(eq, key);
        let kupno: Vec<StockOrder> = zlecenia
            .iter()
            .copied()
            .filter(|o| o.side == Side::Buy)
            .collect();
        let sprzedaz: Vec<StockOrder> = zlecenia
            .iter()
            .copied()
            .filter(|o| o.side == Side::Sell)
            .collect();
        let Some(f) = fixing(&kupno, &sprzedaz, prev) else {
            // Sesja bez przecięcia nie kasuje arkusza: zlecenia wracają i czekają
            // do terminu ważności.
            eq.restore_orders(key, zlecenia);
            continue;
        };
        raport.fixings += 1;
        raport.volume_bp += f.volume_bp;

        // Pieniądz: najpierw wszyscy płacą, potem wszyscy dostają. Odwrotna kolejność
        // wpuściłaby konto przelotowe pod kreskę w środku sesji.
        let mut obrot = Money::ZERO;
        let mut dotknieci: Vec<Owner> = Vec::new();
        let row = market.rest_of_world();
        let rozliczenie = pay::Rozliczenie {
            market,
            row,
            firm: key,
            t,
        };
        // Kupujący płacą pierwsi, a zapłacony wolumen jest **jedyną** podstawą
        // wszystkiego, co dalej: sprzedający dostają tyle, ile wpłynęło, i tyle
        // udziału oddają. Po walidacji pokrycia odmowa zapłaty nie powinna zajść,
        // ale konsekwencja jej zajścia nie może być cudzym pieniądzem — dlatego
        // liczy się fakt, a nie wolumen z fixingu.
        let mut oplaceni: Vec<(Owner, u16)> = Vec::new();
        for (id, bp) in &f.fills {
            let Some(o) = zlecenia.iter().find(|o| o.id == *id) else {
                continue;
            };
            if o.side != Side::Buy {
                continue;
            }
            let kwota = Money(f.price.get().saturating_mul(i64::from(*bp)));
            if !zaplac(world, rozliczenie, o.holder, kwota, *bp) {
                continue;
            }
            obrot = Money(obrot.get() + kwota.get());
            oplaceni.push((o.holder, *bp));
            dotknieci.push(o.holder);
        }
        let mut zostalo: u32 = oplaceni.iter().map(|(_, bp)| u32::from(*bp)).sum();
        let mut sprzedali: Vec<(Owner, u16)> = Vec::new();
        for (id, bp) in &f.fills {
            let Some(o) = zlecenia.iter().find(|o| o.id == *id) else {
                continue;
            };
            if o.side != Side::Sell || zostalo == 0 {
                continue;
            }
            let ile = zostalo.min(u32::from(*bp)) as u16;
            zostalo -= u32::from(ile);
            let kwota = Money(f.price.get().saturating_mul(i64::from(ile)));
            odbierz(world, rozliczenie, o.holder, kwota, ile);
            sprzedali.push((o.holder, ile));
            dotknieci.push(o.holder);
        }
        raport.turnover = Money(raport.turnover.get() + obrot.get());

        // Udział: sprzedający oddają do puli, kupujący biorą z niej. Pula jest
        // domknięta, bo obie listy powstały z **tego samego** zapłaconego wolumenu.
        przenies_udzialy(world, key, &sprzedali, &oplaceni);

        // Niezrealizowana reszta wraca do arkusza — termin ważności jest jej
        // jedynym końcem.
        let reszta: Vec<StockOrder> = zlecenia
            .iter()
            .filter_map(|o| {
                let wziete: u32 = f
                    .fills
                    .iter()
                    .filter(|(id, _)| *id == o.id)
                    .map(|(_, bp)| u32::from(*bp))
                    .sum();
                let zostalo = u32::from(o.bp).saturating_sub(wziete);
                (zostalo > 0).then_some(StockOrder {
                    bp: zostalo as u16,
                    ..*o
                })
            })
            .collect();
        eq.restore_orders(key, reszta);

        let ruszyl = eq.listing(key).is_some_and(|l| l.last_fixing != f.price);
        if let Some(l) = eq.listing_mut(key) {
            l.prev_fixing = l.last_fixing;
            l.last_fixing = f.price;
            l.last_volume_bp = f.volume_bp.min(u32::from(u16::MAX)) as u16;
        }
        // Do dziennika decyzji firmy idzie **wyłącznie sesja, która ruszyła kurs**.
        // Dziennik ma 32 wpisy i pokazuje decyzje; fixing dobowy jest stanem rynku,
        // a nie decyzją spółki — dopisywany co dobę wyparłby z pierścienia wszystko
        // inne w miesiąc i karta firmy przestałaby odpowiadać na „dlaczego".
        // Sam kurs, także ten niezmieniony, zostaje w `Listing` i tam czyta go panel.
        if ruszyl {
            if let Some(firms) = world.get_resource_mut::<Firms>() {
                firms.log(key, t, super::fixing_reason(firm_id(key), f.price));
            }
        }

        dotknieci.sort_by_key(|o| super::owner_key(*o));
        dotknieci.dedup();
        let przed = eq.take_disclosures();
        let zmiana = {
            let Some(firms) = world.get_resource_mut::<Firms>() else {
                continue;
            };
            corp::check_thresholds(eq, firms, key, &dotknieci, day, t)
        };
        let nowe = eq.take_disclosures();
        raport.disclosures += nowe.len() as u32;
        if let Some(nowy) = zmiana {
            raport.takeovers += 1;
            // Cechy przejmującego-mieszkańca dopisuje wołający, bo tylko on widzi
            // komponent `Personality`. Firma-przejmujący ma to już zrobione
            // w `check_thresholds`, która widzi rejestr firm.
            if let Owner::Citizen(c) = nowy {
                // `ponytail:` status społeczny neutralny. Sufit nazwany i taki sam
                // jak w całym projekcie: `refresh_personalities` woła się dziś
                // z `|_| None`, więc **żadna** firma nie ma cech z dyrektora, a status
                // wchodzi do wzoru wyłącznie jako poduszka przy skłonności do ryzyka.
                // Droga wyjścia: `director_facts` wypełnione majątkiem gospodarstwa —
                // jedna zmiana w moście, nie tutaj.
                let p = world
                    .get::<magnat_agents::Personality>(c.0)
                    .map(|os| magnat_firms::personality_from_director(os, magnat_core::Q::new(50)));
                if let (Some(p), Some(firms)) = (p, world.get_resource_mut::<Firms>()) {
                    corp::set_personality_from_citizen(firms, key, p);
                }
            }
        }
        // Plotka z ujawnienia podnosi przekonanie na tydzień — i to jest jedyna
        // droga, którą kurs rusza się **przed** publikacją wyników.
        if !nowe.is_empty() {
            if let Some(l) = eq.listing_mut(key) {
                l.rumor_bp = l.rumor_bp.saturating_add(500);
                l.rumor_days = super::RUMOR_DAYS;
            }
        }
        for d in przed.into_iter().chain(nowe) {
            eq.push_disclosure(d);
        }
    }
}

/// Przycina zlecenia do **pokrycia**: sprzedający do pakietu, który ma, kupujący
/// do kwoty, którą ma na koncie.
///
/// To jest jedyne miejsce, w którym sesja może kogoś oszukać, i dlatego stoi przed
/// fixingiem, a nie po nim. Bez tego zlecenie sprzedaży większe od pakietu dawałoby
/// wolumen, za który kupujący **zapłacił**, a którego nie ma z czego przenieść —
/// pieniądz domykałby się co do grosza, a udział nie. Odwrotnie po stronie kupna:
/// nieopłacony przydział zostawiałby konto przelotowe pod kreską, bo sprzedającym
/// płaci się z wolumenu sesji, a nie z tego, ile faktycznie wpłynęło.
///
/// Zlecenie przycięte do zera wypada z arkusza. Taki jest też porządek prawdziwej
/// giełdy: pokrycia sprawdza się przy przyjęciu zlecenia, nie przy rozliczeniu.
fn waliduj(
    world: &World,
    market: &Market,
    key: FirmKey,
    mut zlecenia: Vec<StockOrder>,
) -> Vec<StockOrder> {
    let ma = |o: &StockOrder| -> u16 {
        match o.side {
            // Sprzedający musi mieć **i pakiet, i kanał wypłaty**. Firma bez konta
            // w `Books` oddałaby udział, a pieniądz za niego został na koncie
            // przelotowym — znalezisko recenzji M10d. `Owner::External` i `City`
            // konta nie mają i mieć nie będą, ale tam pieniądz **zostaje w resztcie
            // świata**, czyli na prawdziwym koncie, więc sprzedawać im wolno.
            Side::Sell if !umie_odebrac(market, o.holder) => 0,
            Side::Sell => world
                .get_resource::<Firms>()
                .and_then(|fs| fs.get(key))
                .map_or(0, |f| super::stake_of(f, o.holder)),
            Side::Buy => {
                let gotowka = pokrycie(world, market, o.holder);
                (gotowka.get() / o.limit.get().max(1)).clamp(0, i64::from(u16::MAX)) as u16
            }
        }
    };
    for o in zlecenia.iter_mut() {
        o.bp = o.bp.min(ma(o));
    }
    zlecenia.retain(|o| o.bp > 0);
    zlecenia
}

/// Czy sprzedający ma gdzie odebrać pieniądz za pakiet.
fn umie_odebrac(market: &Market, kto: Owner) -> bool {
    match kto {
        Owner::Firm(k) => market.account_of_firm(firm_id(k)).is_some(),
        // Mieszkaniec odbiera przez granicę sektora, a nie przez konto; `pay::odbierz`
        // sprawdza samo gospodarstwo i milczy, gdy go nie ma.
        Owner::Citizen(_) | Owner::Player => true,
        // Pieniądz zostaje na koncie reszty świata, które jest prawdziwym kontem
        // w księgach. Tą drogą wychodzi na rynek pakiet założycielski przy debiucie.
        Owner::City | Owner::External => true,
    }
}

/// Ile pieniądza ten właściciel może dziś wyłożyć.
fn pokrycie(world: &World, market: &Market, kto: Owner) -> Money {
    match kto {
        Owner::Firm(k) => saldo(world, market.account_of_firm(firm_id(k))),
        Owner::Citizen(c) => majatek_plynny(world, c),
        Owner::Player => player_citizen(world).map_or(Money::ZERO, |c| majatek_plynny(world, c)),
        // Miasto i sieć zewnętrzna nie kupują udziałów; ich zlecenia kupna nie
        // powstają, a gdyby powstały, nie miałyby z czego zapłacić (`pay::zaplac`).
        Owner::City | Owner::External => Money::ZERO,
    }
}

fn majatek_plynny(world: &World, c: magnat_core::CitizenId) -> Money {
    let Some(h) = super::pay::gospodarstwo(world, c) else {
        return Money::ZERO;
    };
    world
        .get::<magnat_agents::Household>(h)
        .map_or(Money::ZERO, |gd| {
            Money(gd.savings.get().max(0) + gd.bank.get().max(0))
        })
}

/// Plotka gaśnie o jedną `RUMOR_DAYS`-tą na dobę.
fn gasnij_plotke(eq: &mut Equity, key: FirmKey) {
    let Some(l) = eq.listing_mut(key) else { return };
    if l.rumor_days == 0 {
        l.rumor_bp = 0;
        return;
    }
    l.rumor_days -= 1;
    if l.rumor_days == 0 {
        l.rumor_bp = 0;
    } else {
        l.rumor_bp -= l.rumor_bp / i16::from(l.rumor_days + 1);
    }
}

/// Przenosi udział: z listy sprzedających do listy kupujących, po kolei.
///
/// Obie listy mają tę samą sumę bp, bo obie powstały z zapłaconego wolumenu sesji.
/// Kolejność jest posortowana po kluczu właściciela, więc wynik nie zależy od tego,
/// w jakiej kolejności zlecenia trafiły do arkusza (00 §3.2).
fn przenies_udzialy(
    world: &mut World,
    key: FirmKey,
    sprzedali: &[(Owner, u16)],
    kupili: &[(Owner, u16)],
) {
    let mut sprzedajacy: Vec<(Owner, u16)> = sprzedali.to_vec();
    let mut kupujacy: Vec<(Owner, u16)> = kupili.to_vec();
    sprzedajacy.sort_by_key(|(o, _)| super::owner_key(*o));
    kupujacy.sort_by_key(|(o, _)| super::owner_key(*o));
    let Some(firms) = world.get_resource_mut::<Firms>() else {
        return;
    };
    let mut i = 0usize;
    let mut zostalo_u_sprzedajacego = sprzedajacy.first().map_or(0, |(_, bp)| *bp);
    for (kupiec, mut chce) in kupujacy {
        while chce > 0 && i < sprzedajacy.len() {
            let (sprzedawca, _) = sprzedajacy[i];
            let ile = chce.min(zostalo_u_sprzedajacego);
            if ile > 0 {
                let poszlo = corp::settle_stake(firms, key, sprzedawca, kupiec, ile);
                chce -= poszlo;
                zostalo_u_sprzedajacego -= poszlo;
                if poszlo < ile {
                    // Nie powinno zajść po walidacji pokrycia (`waliduj`), a gdyby
                    // zaszło, reszta zlecenia przepada zamiast tworzyć udział
                    // z niczego — suma 10 000 jest niezmiennikiem, nie życzeniem.
                    zostalo_u_sprzedajacego = 0;
                }
            }
            if zostalo_u_sprzedajacego == 0 {
                i += 1;
                zostalo_u_sprzedajacego = sprzedajacy.get(i).map_or(0, |(_, bp)| *bp);
            }
        }
    }
}
