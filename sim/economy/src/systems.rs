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
//!
//! **Od M7e stoi też po `firms.Firm`**, i to też jest kontrakt (`K-42`): tamten
//! system przydziela sloty decyzyjne i zostawia je w skrzynce, a ten je wykonuje.
//! Odwrotna kolejność znaczyłaby, że firma decyduje w minucie następnej po tej,
//! w której wypadł jej slot — czyli że rozkład obciążenia z §5.6 przestaje opisywać
//! to, co się faktycznie dzieje.

use magnat_agents::{Household, Population};
use magnat_core::{
    Cadence, DecisionReason, Money, NeedKind, SimCalendar, Tick, Q, STOCK_CAT_COUNT,
};
use magnat_ecs::{System, SystemCtx, SystemDesc, SystemId, World};

use crate::books::{Books, TxKind, TxMemo};
use crate::budget::HouseholdProfile;
use crate::corpfin::CorpFinance;
use crate::market::{HouseholdMonth, HouseholdMonthReport, Market, PurchaseIntent};

/// Rozliczenie, uzupełnianie półek i zaopatrzenie.
pub struct MarketSystem {
    desc: SystemDesc,
    intents: Vec<PurchaseIntent>,
    households: Vec<(u32, u8, u8, [u8; STOCK_CAT_COUNT])>,
    /// Bufor miesięcznego rozliczenia gospodarstw — raz na miesiąc, ale dla
    /// wszystkich naraz, więc alokacja per miesiąc byłaby alokacją na 80 tys. wierszy.
    months: Vec<HouseholdMonth>,
    /// Ostatnia doba listy płac (`R2-WP30`) — do raportu scenariusza.
    payroll: crate::payroll::PayrollDay,
}

impl MarketSystem {
    #[must_use]
    pub fn new(_world: &World) -> MarketSystem {
        MarketSystem {
            desc: SystemDesc::new("economy.Market", Cadence::EveryMinute)
                .exclusive()
                .after_if_present(SystemId::from_name("firms.Firm"))
                .before(SystemId::from_name("agents.DayLoop")),
            intents: Vec::new(),
            households: Vec::new(),
            months: Vec::new(),
            payroll: crate::payroll::PayrollDay::default(),
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
        let chain = ctx
            .world()
            .get_resource::<magnat_supply::ChainHandle>()
            .cloned();
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

        // 0b. Lista płac (`R2-WP30`). Obok rozliczeń B2B, bo to ten sam kształt:
        //     skrzynka pochodzi z `sim/firms`, a księguje `sim/economy`. `run_payroll`
        //     produkuje wyłącznie o północy, więc w pozostałych minutach kończy się
        //     na sprawdzeniu, czy skrzynka jest pusta.
        self.payroll = crate::payroll::absorb_payroll(ctx.world_mut(), &market, t);

        // 0c. Opłaty mobilne (`R2-WP32`). Ten sam kształt co wyżej: `sim/traffic`
        //     zdjął pieniądz z portfela i odłożył go w `MobilityDue`, a księguje ten,
        //     kto ma `Books`. Świat bez ruchu nie ma tego zasobu i kończy na `None`.
        crate::mobility::absorb_mobility(ctx.world_mut(), &market, t);

        // 1. Rozliczenie intencji z poprzedniej minuty.
        settle_transactions(ctx.world_mut(), &market, t, &mut self.intents);

        // 1a. Decyzje firm AI, którym w tej minucie wypadł slot (M7e WP11–WP14).
        //     Przed dobą sklepu, bo tier operacyjny ustawia **cel** marży i zapasu,
        //     a `reprice_all` i `reorder_and_receive` je wykonują — odwrotna
        //     kolejność znaczyłaby, że decyzja firmy działa dopiero nazajutrz.
        run_firm_ai(ctx.world_mut(), &market, t);

        let cal = SimCalendar::new(t);
        if cal.is_hour_boundary() {
            // 1a. Polityki o kadencji godzinowej (M9d §5.6). Dobowe wykonują się
            //     niżej, razem z resztą doby sklepu — `run_policies` odsiewa je samo
            //     po `Cadence`, więc kadencja jest jedną regułą w jednym miejscu,
            //     a nie dwoma listami zakładów.
            //
            //     Granica doby jest **też** granicą godziny, więc bez tego warunku
            //     polityka godzinowa wykonałaby się o północy dwa razy.
            if !cal.is_day_boundary() {
                run_policies(ctx.world_mut(), &market, t);
            }
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
            // 4a. Tablica publiczna cen (M7e WP10) — **przed** przeceną, bo zapisuje
            //     stan, na którym doba się zaczyna. Zapis po przecenie znaczyłby,
            //     że firma o opóźnieniu jednej doby widzi dzisiejszą cenę rywala,
            //     czyli że asymetria informacji ma dziurę wielkości jednego kroku.
            refresh_board(ctx.world_mut(), &market, t);
            market.observe_competitors(t);
            // 4b. Ślad doby zakładów śledzonych (M9d WP8) — **między obserwacją
            //     a wykonaniem**, czyli na dokładnie tych liczbach, które zobaczy
            //     polityka. Zapis w innym miejscu doby dałby dry-run mówiący o innym
            //     świecie niż ten, w którym reguła się wykona.
            market.record_policy_trace(t);
            // 4a. Polityki zdelegowanych zakładów (M7c WP7) — **między obserwacją
            //     a przeceną**. Reguła czyta świeży obraz konkurencji i ustawia
            //     sterownik ceny, a `reprice_all` go wykonuje razem z ogranicznikiem
            //     marży. Odwrotna kolejność znaczyłaby, że polityka pracuje na obrazie
            //     sprzed doby, a jej wynik i tak zostaje nadpisany tego samego dnia.
            run_policies(ctx.world_mut(), &market, t);
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
            // Domknięcie miesiąca dotyka naraz ksiąg i rejestru zaległości, a dwóch
            // pożyczek `&mut World` naraz nie ma — więc finanse firm wyjmuje się
            // ze świata na czas kroku, tak samo jak `LaborSystem` wyjmuje rynek pracy.
            let mut fin = ctx
                .world_mut()
                .get_resource_mut::<CorpFinance>()
                .map(std::mem::take);
            let mut wyniki: Vec<(magnat_core::SiteId, u32, Money, Money)> = Vec::new();
            if let Some(books) = ctx.world_mut().get_resource_mut::<Books>() {
                match fin.as_mut() {
                    Some(f) => {
                        wyniki = market.close_month_with(books, f, t).1;
                    }
                    // Świat bez finansów firm to scenariusz sprzed M7d (albo test
                    // samego detalu). Miesiąc domyka się wtedy na rejestrze na jedną
                    // chwilę, którego nikt potem nie czyta — koszt jest ten sam,
                    // a gałęzi „bez zaległości" w `close_month` nie ma, bo byłaby
                    // drugą, nietestowaną ścieżką księgowania.
                    None => {
                        let mut pusty = CorpFinance::default();
                        wyniki = market.close_month_with(books, &mut pusty, t).1;
                    }
                }
            }
            // Utarg i koszt własny do rachunku wyniku zakładu (M7e). **Drugi pisarz
            // jednego wpisu**: koszty zna lista płac w dniu wypłaty, przychód — księga
            // przy domknięciu okresu, a to nie jest ta sama doba.
            if let Some(firms) = ctx.world_mut().get_resource_mut::<magnat_firms::Firms>() {
                for (site, miesiac, utarg, koszt) in wyniki {
                    firms.post_revenue(site, miesiac, utarg, koszt);
                }
            }
            if let (Some(f), Some(slot)) = (fin, ctx.world_mut().get_resource_mut::<CorpFinance>())
            {
                *slot = f;
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
        // Encja **z generacją**, a nie `Entity::new(index, MIN)` (`R2-WP32`): indeks
        // zwolniony przez rozwiązane gospodarstwo wraca do puli i dostaje następną
        // generację, więc uchwyt sklejony z samego indeksu przestaje wskazywać cokolwiek.
        // Cicho pomijany zapis zwrotny znaczył wtedy, że pieniądz **zszedł z konta**
        // (`household_pay` w `household_month`), a **nie zszedł z komponentu** — czyli
        // świat go sobie dorabiał. To była druga strona rozjazdu niezmiennika świata.
        let Some(e) = magnat_agents::demography::household_by_index(world, row.index) else {
            continue;
        };
        let Some(h) = world.get_mut::<Household>(e) else {
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
        children: h.children,
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
    for (e, zadeklarowany) in plan {
        // **Dopłata, nie wypłata** (`R2-WP30`). Płacę wypłaca pracodawca w swoim dniu
        // wypłaty (`payroll::absorb_payroll`), a ta pętla domyka to, czego żaden
        // pracodawca nie pokrył: dochód spoza etatu i światy bez rejestru firm
        // (scenariusze M3 i M5, w których `income_monthly` pochodzi z generatora).
        // Bez licznika świat z rejestrem płaciłby dwa razy, a świat bez niego —
        // ani razu.
        let pokryte = market.take_wages_paid(e.index());
        let brutto = Money((zadeklarowany.get() - pokryte.get()).max(0));
        if brutto.get() <= 0 {
            continue;
        }
        // Zaliczka PIT potrącana **u źródła** (hak M8, do M8 zero). Gospodarstwo
        // dostaje netto, bo z tego, co dostanie, zaraz planuje koperty — potrącenie
        // doliczone później znaczyłoby, że planer dzieli dochód, którego nie ma.
        // Potrącony pieniądz zostaje u pracodawcy; przekazuje go miastu system
        // `city.Tax`, razem z deklaracją miesięczną.
        let zaliczka = market.withhold(e.index(), brutto);
        let kwota = Money(brutto.get() - zaliczka.get());
        if kwota.get() <= 0 {
            continue;
        }
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
    let mut memo = TxMemo::new(
        TxKind::RetailSale {
            offer: it.offer,
            good: it.good,
            qty: it.qty,
            buyer: it.household,
        },
        it.reason,
    );
    // VAT wyłuskuje się **z ceny, którą mieszkaniec właśnie zapłacił** (`K-7`),
    // a nie dolicza do niej: cena półkowa jest brutto, więc podatek już w niej siedzi.
    // Do M8 `NoTax` zwraca zero i to pole nic nie zmienia.
    memo.tax = market.vat_on_gross(it.good, it.agreed_price);
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
    // **Pętla zwrotna marki** (M10b WP10.5): mieszkaniec dowiaduje się, ile ten towar
    // naprawdę był wart, i porównuje to z tym, czego się spodziewał. Tu — a nie przy
    // decyzji — bo dopiero teraz partia zeszła z półki i zna się jej **faktyczną**
    // jakość. Rozczarowanie kosztuje trzy razy tyle, ile daje zachwyt (PRD §7.6).
    if let Some(slice) = it.taken {
        if let Some(brand) = slice.brand {
            let _ = magnat_agents::touch(
                world,
                it.buyer.entity(),
                brand,
                magnat_agents::Touch::Experience {
                    actual: slice.quality,
                },
                t.get() / 1_440,
            );
        }
    }
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

fn refresh_households(world: &World, out: &mut Vec<(u32, u8, u8, [u8; STOCK_CAT_COUNT])>) {
    out.clear();
    let Some(p) = world.get_resource::<Population>() else {
        return;
    };
    for e in p.households() {
        let Some(h) = world.get::<Household>(*e) else {
            continue;
        };
        out.push((e.index(), h.size.max(1), h.children, h.stock));
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

/// Doba polityk: rejestr firm, salda zakładów i wykonanie reguł (M7c WP7).
///
/// Rejestr firm **wyjmuje się** ze świata na czas kroku, bo salda prowadzi `Books`,
/// a dwóch pożyczek `&mut World` naraz nie ma. Ten sam wzorzec, którym `LaborSystem`
/// wyjmuje rynek pracy, a `sim/agents` — `AgentSources`.
fn run_policies(world: &mut World, market: &Market, t: Tick) {
    let konta = market.shop_accounts();
    let salda: std::collections::BTreeMap<magnat_core::SiteId, Money> = world
        .get_resource::<Books>()
        .map(|b| {
            konta
                .iter()
                .filter_map(|(site, acc)| b.balance(*acc).map(|m| (*site, m)))
                .collect()
        })
        .unwrap_or_default();
    let Some(mut firms) = world
        .get_resource_mut::<magnat_firms::Firms>()
        .map(std::mem::take)
    else {
        return;
    };
    // Kalibracja menedżera jest **zasobem świata**, a nie stałą: świat bez niej
    // wykonuje polityki dokładnie i natychmiast (`ManagerExecution::flawless`),
    // a plik ładuje `game::world` przy stawianiu gry. Scenariusz stawiający sam
    // wycinek gospodarki nie musi go mieć, żeby się uruchomić.
    let krzywa = world
        .get_resource::<crate::manager_exec::PolicyTuning>()
        .map(|t| t.manager);
    market.run_policies(&mut firms, &salda, krzywa.as_ref(), t);
    *world.resource_mut::<magnat_firms::Firms>() = firms;
}

/// Dobowy zapis tablicy publicznej cen (M7e WP10).
fn refresh_board(world: &mut World, market: &Market, t: Tick) {
    let Some(mut board) = world
        .get_resource_mut::<crate::board::PublicMarketBoard>()
        .map(std::mem::take)
    else {
        return;
    };
    market.refresh_board(&mut board, t);
    *world.resource_mut::<crate::board::PublicMarketBoard>() = board;
}

/// Minuta AI firm: wykonanie decyzji, którym `firms.Firm` przydzielił slot (M7e).
///
/// Rejestr firm **wyjmuje się** ze świata na czas kroku — ten sam wzorzec co przy
/// `run_policies` i z tego samego powodu: salda prowadzi `Books`, a dwóch pożyczek
/// `&mut World` naraz nie ma.
fn run_firm_ai(world: &mut World, market: &Market, t: Tick) {
    let Some(due) = world
        .get_resource_mut::<magnat_firms::DecisionOutbox>()
        .map(magnat_firms::DecisionOutbox::take)
        .filter(|d| d.iter().any(|(_, k)| !k.is_empty()))
    else {
        return;
    };
    let Some(board) = world
        .get_resource::<crate::board::PublicMarketBoard>()
        .cloned()
    else {
        return;
    };
    let Some(catalog) = world
        .get_resource::<magnat_policy::PolicyCatalog>()
        .cloned()
    else {
        return;
    };
    let konta = market.shop_accounts();
    let salda: std::collections::BTreeMap<magnat_core::SiteId, Money> = world
        .get_resource::<Books>()
        .map(|b| {
            konta
                .iter()
                .filter_map(|(site, acc)| b.balance(*acc).map(|m| (*site, m)))
                .collect()
        })
        .unwrap_or_default();
    let Some(mut firms) = world
        .get_resource_mut::<magnat_firms::Firms>()
        .map(std::mem::take)
    else {
        return;
    };
    let widoki = world
        .get_resource::<magnat_firms::StrategicOutlooks>()
        .cloned()
        .unwrap_or_default();
    let dzien = market.run_firm_ai(
        &mut firms,
        &crate::ai_run::AiInputs {
            board: &board,
            catalog: &catalog,
            outlooks: &widoki,
            cash: &salda,
        },
        &due,
        t,
    );
    *world.resource_mut::<magnat_firms::Firms>() = firms;
    for site in &dzien.to_close {
        close_site(world, market, *site, t);
    }
    // Powstawanie, ekspansja i zwijanie firm (M7f WP15). Stoi tutaj, a nie we własnym
    // systemie, bo jedynym wejściem są listy, które właśnie wróciły z decyzji firm —
    // osobny system musiałby je przenieść przez świat i wtedy `to_open` byłoby
    // wykonywane dobę później niż zapisany powód.
    let zycie = crate::firmlife::step_day(world, market, &dzien, t);
    if let Some(l) = world.get_resource_mut::<crate::firmlife::FirmLifeLog>() {
        l.record(&dzien, zycie);
    }
}

/// Domknięcie zamknięcia zakładu (M7e WP12).
///
/// Decyzję podjął tier taktyczny, ale jej wykonanie dotyka trzech rzeczy, których
/// rynek nie widzi: umów (komponent `Employment` mieszkańca), puli wakatów miasta
/// i odpraw. Idzie przez [`crate::labor::hr::dismiss_all`], czyli przez tę samą
/// jedyną drogę wyjścia z etatu, którą chodzi upadłość — inaczej niezmiennik
/// „każdy `Employment` zakończony dokładnie raz" miałby drugą, nietestowaną ścieżkę.
///
/// **Odprawa jest tu wypłacana, a nie naliczana**, i to jest cała różnica wobec
/// upadłości: firma zamykająca nierentowny zakład nadal istnieje i nadal ma konto.
/// Gdy na nim nie starcza, kwota zostaje zaległością — ta sama gałąź, którą
/// `close_month` obsługuje niezapłacony czynsz.
pub(crate) fn close_site(world: &mut World, market: &Market, site: magnat_core::SiteId, t: Tick) {
    let Some(mut firms) = world
        .get_resource_mut::<magnat_firms::Firms>()
        .map(std::mem::take)
    else {
        return;
    };
    let Some(hr) = world
        .get_resource::<crate::labor::LaborHandle>()
        .and_then(crate::labor::LaborHandle::get)
        .map(crate::labor::LaborMarket::hr_tuning)
    else {
        *world.resource_mut::<magnat_firms::Firms>() = firms;
        return;
    };
    let firma = firms.site(site).map(|s| s.firm);
    let odprawy = {
        let mut people = crate::labor::system::WorldWorkforce::new(world);
        crate::labor::hr::dismiss_all(
            &hr,
            &mut firms,
            &mut people,
            site,
            magnat_core::SimMinute(t.get()),
        )
    };
    firms.close_site(site);
    *world.resource_mut::<magnat_firms::Firms>() = firms;
    market.close_shop(site);
    let Some(konto) = market.shop_account(site) else {
        return;
    };
    let mut wyplaty: Vec<crate::corpfin::SectorPayout> = Vec::new();
    for (c, odprawa) in odprawy {
        if odprawa.get() <= 0 {
            continue;
        }
        let memo = TxMemo::new(
            TxKind::Wage { site },
            DecisionReason::JobLeft {
                role: magnat_core::JobRoleId(0),
                cause: magnat_core::LeaveCause::Redundancy,
                tenure_days: 0,
            },
        );
        let zaplacone = world
            .get_resource_mut::<Books>()
            .is_some_and(|b| b.household_receive(konto, odprawa, memo, t).is_ok());
        if zaplacone {
            wyplaty.push(crate::corpfin::SectorPayout {
                to: c,
                amount: odprawa,
                reason: memo.reason,
            });
            continue;
        }
        // Zakład, którego nie stać na odprawę, zostawia po sobie dług — ta sama
        // gałąź, którą `close_month` obsługuje niezapłacony czynsz (M7d `BA-5`).
        if let (Some(f), Some(fin)) = (firma, world.get_resource_mut::<CorpFinance>()) {
            fin.arrears_mut().accrue(
                crate::books::AccountOwner::Firm(magnat_firms::firm_id(f)),
                crate::books::AccountOwner::Citizen(c),
                odprawa,
                crate::corpfin::ClaimOrigin::Severance,
                t,
            );
        }
    }
    // Druga połowa kanału `household_sector_out`: kwota zeszła z ksiąg, więc musi
    // wejść do gospodarstwa — inaczej pieniądz ginie między księgą a światem.
    crate::corpfin::system::wyplac_sektorowi(world, &wyplaty);
}
