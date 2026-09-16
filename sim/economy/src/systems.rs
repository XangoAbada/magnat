//! Systemy ECS gospodarki detalicznej (M5b §5.5).
//!
//! Jeden system, bo wszystkie te kroki potrzebują **całego świata naraz**: sald
//! w `Books`, komponentów `Household` i stanu `Market`. Rozpisanie go na deklaracje
//! dostępu nie kupiłoby ani jednej krawędzi DAG — dałoby za to fałszywą deklarację,
//! gdyby ktoś czegoś nie wypisał. To jest dokładnie przypadek z `K-21`.
//!
//! Od M5c doba sklepu ma własną kolejność kroków (odpis → obserwacja → przecena →
//! zaopatrzenie) i ona też jest kontraktem — powód przy wywołaniach niżej.
//!
//! **Kolejność wobec pętli doby jest kontraktem.** System stoi **przed**
//! `agents.DayLoop`, bo to on ustawia rynkowi bieżący tick i sprząta po poprzedniej
//! minucie. Dzięki temu `fulfil` wołany w tej samej minucie widzi właściwy czas
//! i pustą listę rezerwacji budżetu, a intencje z minuty `t−1` są już rozliczone.

use magnat_agents::{Household, Population};
use magnat_core::{
    Cadence, DecisionReason, Money, NeedKind, SimCalendar, Tick, Q, STOCK_CAT_COUNT,
};
use magnat_ecs::{System, SystemCtx, SystemDesc, SystemId, World};

use crate::books::{Books, TxKind, TxMemo};
use crate::budget::HouseholdProfile;
use crate::market::{HouseholdMonth, HouseholdMonthReport, Market, PurchaseIntent};

/// Rozliczenie, uzupełnianie półek i zaopatrzenie.
pub struct MarketSystem {
    desc: SystemDesc,
    intents: Vec<PurchaseIntent>,
    households: Vec<(u32, u8, [u8; STOCK_CAT_COUNT])>,
    /// Bufor miesięcznego rozliczenia gospodarstw — raz na miesiąc, ale dla
    /// wszystkich naraz, więc alokacja per miesiąc byłaby alokacją na 80 tys. wierszy.
    months: Vec<HouseholdMonth>,
}

impl MarketSystem {
    #[must_use]
    pub fn new(_world: &World) -> MarketSystem {
        MarketSystem {
            desc: SystemDesc::new("economy.Market", Cadence::EveryMinute)
                .exclusive()
                .before(SystemId::from_name("agents.DayLoop")),
            intents: Vec::new(),
            households: Vec::new(),
            months: Vec::new(),
        }
    }
}

impl System for MarketSystem {
    fn desc(&self) -> &SystemDesc {
        &self.desc
    }

    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let t = ctx.tick;
        let Some(market) = ctx.world().get_resource::<Market>().cloned() else {
            return;
        };
        market.set_tick(t);

        // 0. Odbiór minuty łańcucha dostaw (M6).
        //
        //    **Kadencję prowadzi od M6e `supply.Chain`**, nie ten system (`AP-4`):
        //    łańcuch ma własny `SystemId` i stoi w DAG **przed** rynkiem, więc
        //    kontrakt `D12` (psucie przed detalem) jest krawędzią grafu, a nie
        //    komentarzem. Tutaj zostaje samo księgowanie tego, co łańcuch zostawił
        //    w skrzynce: odpisy terminu, rozliczenia B2B i faktury za media.
        //    Fakty wychodzą listą, księguje ten, kto ma księgę (`AI-1`).
        let chain = ctx.world().get_resource::<magnat_supply::ChainHandle>().cloned();
        let lancuch = chain.as_ref().map(magnat_supply::ChainHandle::take_tick);
        if let Some(w) = &lancuch {
            if !w.spoiled.is_empty() {
                market.absorb_spoilage(&w.spoiled, t);
            }
        }

        // 0a. Rozliczenia B2B i faktury za media — **przed** rozliczeniem intencji,
        //     bo obie pozycje dotyczą minuty, która się właśnie skończyła.
        if let Some(w) = &lancuch {
            if !w.settlements.is_empty() || !w.bills.is_empty() {
                if let Some(books) = ctx.world_mut().get_resource_mut::<Books>() {
                    market.absorb_settlements(&w.settlements, books, t);
                    market.absorb_utility_bills(&w.bills, books, t);
                }
            }
        }

        // 1. Rozliczenie intencji z poprzedniej minuty.
        settle_transactions(ctx.world_mut(), &market, t, &mut self.intents);

        let cal = SimCalendar::new(t);
        if cal.is_hour_boundary() {
            // 2. Migawka gospodarstw dla decyzji zakupowej.
            refresh_households(ctx.world(), &mut self.households);
            market.refresh_households(&self.households);
            // 3a. Półki z zaplecza. Rozliczenia rynku B2B są już zaksięgowane
            //     (krok 0a), więc dostawa, która właśnie dojechała, trafia na półkę
            //     w tej samej godzinie, a nie w następnej.
            market.restock_shelves();
        }
        if cal.is_month_boundary() {
            // Dochód gospodarstw (§9 pkt 2 dokumentu fazy).
            pay_incomes(ctx.world_mut(), &market, t);
            // 3a. Budżet miesiąca: koperty, koszty stałe, rata, wniosek kredytowy
            //     przy niedoborze (M5d §5.9). **Po** wypłacie, bo plan dzieli to,
            //     co wpłynęło — odwrotna kolejność planowałaby zeszłomiesięczne saldo.
            settle_household_month(ctx.world_mut(), &market, t, &mut self.months);
        }
        if cal.is_day_boundary() {
            // 4. Doba sklepu (M5c). Kolejność jest kontraktem, nie wygodą:
            //    odpis → obserwacja → przecena → zaopatrzenie.
            //
            //    `observe` przed `reprice`, bo przecena konkurenta z doby `D` ma być
            //    widoczna najwcześniej w dobie `D+1` — inaczej reakcja mieści się
            //    w 2..=8 dobach zamiast 1..=7 z kryterium WP6.
            //    Odpis przed obserwacją, żeby cena nie opierała się na zapasie,
            //    którego już nie ma.
            // Odpis terminu robi magazyn (`Store::spoil`, kadencja minutowa łańcucha)
            // i oddaje listę; tutaj zostaje samo księgowanie tego, co zeszło ze slotów
            // tego sklepu. Do M6c była to własna pętla po liniach zapasu.
            market.observe_competitors(t);
            market.reprice_all(t);
            // 5. Dostawy i zamówienia u dostawcy zewnętrznego.
            if let Some(books) = ctx.world_mut().get_resource_mut::<Books>() {
                market.reorder_and_receive(books, t);
            }
            // 5a. Doba koszyka CPI — po zaopatrzeniu, bo wtedy wszystkie transakcje
            //     doby są już zaksięgowane przez `record_sale` (M5d §5.10).
            market.roll_cpi_day();
        }
        if cal.is_month_boundary() {
            // 6. Koszty stałe, amortyzacja, domknięcie okresu (M5c §5.8).
            //    Po zaopatrzeniu, bo miesiąc ma się domknąć na stanie, który
            //    ta doba zostawiła.
            if let Some(books) = ctx.world_mut().get_resource_mut::<Books>() {
                market.close_month(books, t);
            }
            // 6a. Domknięcie miesiąca CPI i stopa bazowa banku centralnego.
            //     Ostatnie, bo czyta indeks policzony z pełnego miesiąca dób.
            market.close_cpi_month(t);
        }
        // 7. Indeks ofert — przebudowa tylko brudnych warstw.
        market.rebuild_index(ctx.pool);
    }
}

/// Miesięczne rozliczenie gospodarstw: plan kopert, koszty stałe, rata kredytu,
/// wniosek przy niedoborze i zaległość przy odmowie (M5d §5.9, WP8).
///
/// Pieniądz gospodarstwa mieszka w komponencie `Household` (`U-17`), więc krok ma
/// trzy fazy: zebranie sald z komponentów, rozliczenie w `Market` + `Books`, zapis
/// sald z powrotem. Dwa źródła salda rozjechałyby się przy pierwszej transakcji.
///
/// Zaległość uderza w zaspokojenie potrzeby `Housing` członków gospodarstwa — to jest
/// „spadek zaspokojenia" z kryterium WP8 i zarazem pierwsze tempo, jakie ta potrzeba
/// w ogóle dostaje (`Z-3` z dokumentu fazy mówi wprost, że wnosi je M5).
///
/// Zwraca zbiorczy raport miesiąca.
pub fn settle_household_month(
    world: &mut World,
    market: &Market,
    t: Tick,
    buf: &mut Vec<HouseholdMonth>,
) -> HouseholdMonthReport {
    buf.clear();
    let Some(p) = world.get_resource::<Population>() else {
        return HouseholdMonthReport::default();
    };
    // Kolejność z `Population::households()` jest deterministyczna i to ona ustala
    // kolejność wniosków kredytowych — a ta ma znaczenie, bo bank patrzy na stopę
    // bazową, nie na kolejkę, ale rozrzut scoringu bierze klucz z indeksu encji.
    for e in p.households() {
        let Some(h) = world.get::<Household>(*e) else {
            continue;
        };
        if h.flags & Household::FLAG_ACTIVE == 0 {
            continue;
        }
        buf.push(HouseholdMonth {
            index: e.index(),
            profile: profile_of(world, h),
            income: h.income_monthly,
            cash: h.cash,
            bank: h.bank,
            savings: h.savings,
            shortfall: Money::ZERO,
            credit: None,
            unpaid: None,
        });
    }
    let raport = match world.get_resource_mut::<Books>() {
        Some(books) => market.household_month(buf, books, t),
        None => return HouseholdMonthReport::default(),
    };
    for row in buf.iter() {
        let Some(h) = world.get_mut::<Household>(magnat_core::Entity::new(
            row.index,
            std::num::NonZeroU32::MIN,
        )) else {
            continue;
        };
        h.cash = row.cash;
        h.bank = row.bank;
        h.savings = row.savings;
        // Dług gospodarstwa to niespłacony kapitał plus zaległości — jedno pole,
        // dwie przyczyny, obie prawdziwe. `society::total_money` go nie sumuje
        // (dług nie jest pieniądzem), więc niezmiennik pieniądza stoi.
        let b = market.budget_of(row.index);
        let kapital = b
            .loan
            .and_then(|id| market.loan(id))
            .map_or(Money::ZERO, |l| l.outstanding);
        h.debt = Money(kapital.get().saturating_add(b.arrears.get()));
    }
    apply_shortfall_to_needs(world, buf);
    raport
}

/// Profil gospodarstwa: typ ze **składu**, cechy z pierwszego dorosłego.
///
/// `ponytail:` sufit nazwany — osobowość gospodarstwa jest osobowością pierwszego
/// członka na liście, bo „głowa gospodarstwa" jako pojęcie powstaje dopiero w M7
/// razem z rynkiem pracy. Do tego czasu jedna cecha z jednej osoby jest uczciwsza
/// niż średnia, której nikt nie umie obronić.
fn profile_of(world: &World, h: &Household) -> HouseholdProfile {
    let mut thrift = Q::new(50);
    let mut ambition = Q::new(50);
    for m in h.inline_members() {
        let e = magnat_core::Entity::new(m, std::num::NonZeroU32::MIN);
        if let Some(p) = world.get::<magnat_agents::Personality>(e) {
            thrift = p.get(magnat_core::TraitId::Thrift);
            ambition = p.get(magnat_core::TraitId::Ambition);
            break;
        }
    }
    HouseholdProfile {
        kind: h.household_kind(),
        size: h.size,
        thrift,
        ambition,
    }
}

/// Zaległość psuje zaspokojenie potrzeby mieszkaniowej członków gospodarstwa.
///
/// Skala jest wprost proporcjonalna do nieopłaconej części kosztów stałych i przycięta
/// do 25 punktów na miesiąc: gospodarstwo, które nie zapłaciło raz, ma problem;
/// gospodarstwo, które nie płaci od pół roku, dochodzi do zera i wypada przez progi
/// migracji (M3c) — i to jest właściwy skutek, a nie natychmiastowa katastrofa.
fn apply_shortfall_to_needs(world: &mut World, rows: &[HouseholdMonth]) {
    for row in rows {
        if row.shortfall.get() <= 0 {
            continue;
        }
        let e = magnat_core::Entity::new(row.index, std::num::NonZeroU32::MIN);
        let Some(h) = world.get::<Household>(e).copied() else {
            continue;
        };
        let odniesienie = row.income.get().max(1);
        let spadek = (row.shortfall.get() * 100 / odniesienie).clamp(1, 25) as u8;
        for m in h.inline_members() {
            let c = magnat_core::Entity::new(m, std::num::NonZeroU32::MIN);
            if let Some(n) = world.get_mut::<magnat_agents::Needs>(c) {
                let v = n.get(NeedKind::Housing).get().saturating_sub(spadek);
                n.set(NeedKind::Housing, Q::new(v));
            }
        }
    }
}

/// Wypłata miesięcznego dochodu gospodarstw z konta `RestOfWorld`.
///
/// Decyzja otwarta nr 2 dokumentu fazy, propozycja M5 przyjęta: dochód jest
/// **egzogeniczny** i wypłaca go abstrakcyjny pracodawca spoza miasta, a kwota
/// stoi w `Household.income_monthly`, które wypełnia generacja populacji (M3).
/// M7 podmienia **źródło** kwoty na pensję emergentną, nie strukturę wypłaty;
/// M5d/WP8 dokłada podział tej kwoty na koperty.
///
/// Bez tego kroku pętla fazy jest pusta: gospodarstwa z generacji mają saldo zero,
/// więc każda decyzja zakupowa kończy się `RejectCause::BudgetExhausted`. To nie
/// jest teoria — pierwszy przebieg scenariusza `m5shop` dał dokładnie 170 tys. takich
/// odmów i ani jednej transakcji.
///
/// Zwraca `(ile gospodarstw, łączna kwota)`.
pub fn pay_incomes(world: &mut World, market: &Market, t: Tick) -> (u64, Money) {
    let rest = market.rest_of_world();
    let Some(p) = world.get_resource::<Population>() else {
        return (0, Money::ZERO);
    };
    let plan: Vec<(magnat_core::Entity, Money)> = p
        .households()
        .iter()
        .filter_map(|e| {
            let h = world.get::<Household>(*e)?;
            (h.flags & Household::FLAG_ACTIVE != 0 && h.income_monthly.get() > 0)
                .then_some((*e, h.income_monthly))
        })
        .collect();
    let mut ile = 0u64;
    let mut suma = Money::ZERO;
    for (e, kwota) in plan {
        let memo = TxMemo::new(
            TxKind::Wage {
                site: magnat_core::SiteId(e),
            },
            DecisionReason::Unspecified,
        );
        let ok = world
            .get_resource_mut::<Books>()
            .map(|b| b.household_receive(rest, kwota, memo, t).is_ok())
            .unwrap_or(false);
        if !ok {
            continue;
        }
        if let Some(h) = world.get_mut::<Household>(e) {
            h.bank = Money(h.bank.get().saturating_add(kwota.get()));
            ile += 1;
            suma = Money(suma.get().saturating_add(kwota.get()));
        }
    }
    (ile, suma)
}

/// Rozliczenie wszystkiego, co `fulfil` zaklepał (§5.5).
///
/// Wyjmuje intencje w kolejności `(SiteId, GoodId, arrived, CitizenId)` i domyka
/// każdą z nich: przelew, zapas gospodarstwa, wpis w dzienniku. `buf` jest buforem
/// wielokrotnego użytku wołającego — rozliczenie dzieje się co minutę.
///
/// Zwraca liczbę rozliczonych transakcji.
pub fn settle_transactions(
    world: &mut World,
    market: &Market,
    t: Tick,
    buf: &mut Vec<PurchaseIntent>,
) -> usize {
    *buf = market.take_intents();
    let intents = std::mem::take(buf);
    let mut n = 0;
    for it in &intents {
        if settle_one(world, market, it, t) {
            n += 1;
        }
    }
    *buf = intents;
    buf.clear();
    n
}

/// Jedno rozliczenie: pieniądz z komponentu do ksiąg, zapas do gospodarstwa.
///
/// Kolejność jest istotna i wynika z tego, że pieniądz nie może zniknąć ani się
/// rozmnożyć: najpierw **próba** zdjęcia kwoty z gospodarstwa, dopiero potem zapis
/// w księgach. Gdy gospodarstwa nie stać (a `fulfil` sprawdzał to godziny wcześniej),
/// towar **wraca na półkę** — inaczej sztuki zniknęłyby ze świata.
fn settle_one(world: &mut World, market: &Market, it: &PurchaseIntent, t: Tick) -> bool {
    let Some(account) = market.account_of(it.site) else {
        market.return_goods(it);
        return false;
    };
    let hh = it.household.entity();
    let zaplacone = match world.get_mut::<Household>(hh) {
        Some(h) => pay_from_household(h, it.agreed_price),
        None => false,
    };
    if !zaplacone {
        market.return_goods(it);
        return false;
    }
    let memo = TxMemo::new(
        TxKind::RetailSale {
            offer: it.offer,
            good: it.good,
            qty: it.qty,
            buyer: it.household,
        },
        it.reason,
    );
    let ok = world
        .get_resource_mut::<Books>()
        .map(|b| b.household_pay(account, it.agreed_price, memo, t).is_ok())
        .unwrap_or(false);
    if !ok {
        // Księgi odmówiły — oddajemy pieniądz gospodarstwu i towar sklepowi.
        // Nie ma ścieżki, w której świat traci jedno i drugie.
        if let Some(h) = world.get_mut::<Household>(hh) {
            h.bank = Money(h.bank.get().saturating_add(it.agreed_price.get()));
        }
        market.return_goods(it);
        return false;
    }
    if let Some(h) = world.get_mut::<Household>(hh) {
        let i = it.cat.as_index();
        h.stock[i] = h.stock[i].saturating_add(it.days);
    }
    market.record_sale(it);
    true
}

/// Zdejmuje kwotę z gospodarstwa: najpierw rachunek, potem gotówka.
///
/// Saldo gospodarstwa jest **własnością M3** (komponent `Household`), a nie kontem
/// w `Books` — dlatego druga strona przelewu idzie kanałem sektora gospodarstw
/// (`MoneySupplyLedger.household_sector_in`). Dwa źródła salda rozjechałyby się
/// przy pierwszej transakcji, a testu, który by to złapał, nie ma po żadnej ze stron.
fn pay_from_household(h: &mut Household, amount: Money) -> bool {
    let kwota = amount.get();
    if kwota <= 0 {
        return false;
    }
    if h.bank.get().saturating_add(h.cash.get()) < kwota {
        return false;
    }
    let z_banku = kwota.min(h.bank.get().max(0));
    h.bank = Money(h.bank.get() - z_banku);
    h.cash = Money(h.cash.get() - (kwota - z_banku));
    true
}

fn refresh_households(world: &World, out: &mut Vec<(u32, u8, [u8; STOCK_CAT_COUNT])>) {
    out.clear();
    let Some(p) = world.get_resource::<Population>() else {
        return;
    };
    for e in p.households() {
        let Some(h) = world.get::<Household>(*e) else {
            continue;
        };
        out.push((e.index(), h.size.max(1), h.stock));
    }
}

/// Rejestracja gospodarki w świecie (M5a rozszerzone o rynek).
///
/// `Market` wchodzi do hasha jako **zasób**, nie jako arena: arena ofert siedzi
/// w jego środku, bo `PlaceProvider` nie widzi świata. Uzasadnienie i zgodność
/// z `K-16` są opisane przy `impl HashState for Market`.
pub fn register_economy(world: &mut World, market: Market) {
    world.insert_resource(Books::new());
    world.insert_resource(market);
    world.register_resource_hash::<Books>();
    world.register_resource_hash::<Market>();
}

/// Sam `Books`, bez rynku — dla testów i scenariuszy, które badają wyłącznie pieniądz.
pub fn register_books(world: &mut World) {
    world.insert_resource(Books::new());
    world.register_resource_hash::<Books>();
}

/// Powód transakcji, gdy sprzedaży nie da się przypisać decyzji (import, korekta).
pub const UNSPECIFIED: DecisionReason = DecisionReason::Unspecified;
