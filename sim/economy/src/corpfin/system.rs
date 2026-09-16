//! Wpięcie finansów firmy w świat: zasób, doba postępowań, wypłaty (M7d).
//!
//! # Dlaczego osobny system, a nie krok w `economy.Market`
//!
//! Ten sam powód, dla którego rynek pracy dostał własny (`economy.Labor`, M7b):
//! niewypłacalność rozstrzyga się **raz na dobę**, a `MarketSystem` chodzi co minutę
//! i jest najgorętszym systemem symulacji. Kadencja `EveryDay` kosztuje jeden poziom
//! harmonogramu raz na dobę i nic poza tym.
//!
//! Kolejność jest zadeklarowana jawnie (`K-42`), a nie wzięta z hasza nazwy:
//! **po** rynku pracy, bo zwolnienie całej załogi upadłego ma zastać obsadę, którą
//! rynek pracy już na tę dobę ustalił, a nie wczorajszą.
//!
//! # Co ten system robi, a czego nie
//!
//! Robi trzy rzeczy i wszystkie są konsekwencją, a nie decyzją: sprawdza, czy firma
//! przekroczyła próg niewypłacalności; posuwa otwarte postępowania o dobę; wypłaca
//! to, co postępowanie zasądziło. **Nie decyduje**, czy firma ma wziąć kredyt,
//! wypuścić obligację albo sprzedać należność — to jest pytanie do tiera taktycznego
//! AI (M7e) i do gracza (M9), a `CorpFinance` udostępnia im gotowe wejścia.

use magnat_agents::{demography, Household, Identity};
use magnat_core::{Cadence, CitizenId, DecisionReason, FirmId, Money, SimMinute, SiteId, Tick};
use magnat_ecs::{System, SystemCtx, SystemDesc, SystemId, World};
use magnat_firms::Firms;

use crate::books::{AccountOwner, Books};
use crate::corpfin::{AssetKind, AssetRef, BankruptcyId, ClaimOrigin, CorpFinance, SectorPayout};
use crate::labor::system::{LaborHandle, WorldWorkforce};
use crate::market::Market;

/// Wpina finanse firm do świata i do funkcji haszującej stan (00 §3.6).
pub fn register_corpfin(world: &mut World, fin: CorpFinance) {
    world.insert_resource(fin);
    world.register_resource_hash::<CorpFinance>();
}

/// Doba postępowań upadłościowych.
pub struct InsolvencySystem {
    desc: SystemDesc,
    /// Bufor wypłat do sektora gospodarstw — raz na dobę, ale dla wszystkich naraz.
    payouts: Vec<SectorPayout>,
}

impl InsolvencySystem {
    #[must_use]
    pub fn new() -> InsolvencySystem {
        InsolvencySystem {
            desc: SystemDesc::new("economy.Insolvency", Cadence::EveryDay)
                .exclusive()
                .after(SystemId::from_name("economy.Labor")),
            payouts: Vec::new(),
        }
    }
}

impl Default for InsolvencySystem {
    fn default() -> InsolvencySystem {
        InsolvencySystem::new()
    }
}

impl System for InsolvencySystem {
    fn desc(&self) -> &SystemDesc {
        &self.desc
    }

    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let t = ctx.tick;
        step_day(ctx.world_mut(), t, &mut self.payouts);
    }
}

/// Jedna doba finansów firm. Wystawione jako wolna funkcja, bo woła ją i system,
/// i scenariusz headless, i test — a budowanie harmonogramu po to, żeby przesunąć
/// jedną dobę, jest kosztem bez korzyści.
pub fn step_day(world: &mut World, t: Tick, payouts: &mut Vec<SectorPayout>) {
    payouts.clear();
    let Some(market) = world.get_resource::<Market>().cloned() else {
        return;
    };
    let Some(mut fin) = world.get_resource_mut::<CorpFinance>().map(std::mem::take) else {
        return;
    };

    otwieraj_postepowania(world, &market, &mut fin, t);
    posuwaj_postepowania(world, &market, &mut fin, t, payouts);

    *world.resource_mut::<CorpFinance>() = fin;
    wyplac_sektorowi(world, payouts);
}

/// Sprawdza wszystkie zakłady pod kątem niewypłacalności i otwiera postępowania.
///
/// Iteracja idzie po `shop_accounts()`, czyli po zakładach posortowanych po `SiteId`
/// — kolejność otwierania postępowań wchodzi do stanu przez identyfikatory, więc
/// nie wolno jej brać z mapy mieszającej (00 §3.2).
fn otwieraj_postepowania(world: &mut World, market: &Market, fin: &mut CorpFinance, t: Tick) {
    let mut do_otwarcia: Vec<(SiteId, FirmId, magnat_core::BankruptcyTrigger, u16)> = Vec::new();
    for (site, konto) in market.shop_accounts() {
        let _ = konto;
        let Some(firm) = market.firm_of(site) else {
            continue;
        };
        if let Some((trigger, dni)) = fin.check_insolvency(
            firm,
            market.equity_of(site),
            market.loan_arrears_of(site),
            t,
        ) {
            do_otwarcia.push((site, firm, trigger, dni));
        }
    }
    for (site, firm, trigger, dni) in do_otwarcia {
        let Some(konto) = market.account_of(site) else {
            continue;
        };
        let id =
            market.with_loans(|loans| fin.open_bankruptcy(firm, konto, trigger, dni, loans, t));

        // Zgłoszenie (`Filed`) znaczy trzy rzeczy naraz i wszystkie dzieją się teraz,
        // a nie „kiedyś w trakcie postępowania": załoga odchodzi, rzeczy leasingowane
        // wracają do właścicieli (zrobił to `open_bankruptcy`), a majątek wchodzi
        // do masy.
        zwolnij_zaloge(world, fin, id, site, firm, t);
        wstaw_majatek(market, fin, id, site);
        let powod = fin.case(id).map_or(
            DecisionReason::Unspecified,
            crate::corpfin::Bankruptcy::reason,
        );
        market.log_firm_decision(site, powod);
    }
}

/// Rozwiązuje umowy zakładu i zamienia odprawy na roszczenia.
///
/// Odprawa jest **naliczona, nie wypłacona**: firma w upadłości z definicji nie ma
/// czym płacić, więc kwota staje się zaległością wobec konkretnego mieszkańca, a stąd
/// roszczeniem o priorytecie `Severance`. To jest jedyne miejsce, w którym wierzycielem
/// zostaje człowiek z imienia, i dlatego to ono domyka niezmiennik 4 z M7 §7.2.
fn zwolnij_zaloge(
    world: &mut World,
    fin: &mut CorpFinance,
    case: BankruptcyId,
    site: SiteId,
    firm: magnat_core::FirmId,
    t: Tick,
) {
    let Some(mut firms) = world.get_resource_mut::<Firms>().map(std::mem::take) else {
        return;
    };
    // Z rynku pracy bierzemy **samo strojenie odprawy**, a nie rynek. `HrTuning` jest
    // `Copy` i ma dwadzieścia bajtów; klonowanie całego `LaborMarket` po to, żeby
    // obliczyć jedną kwotę, kosztowałoby areną ofert za każdą upadłość w mieście.
    let Some(hr) = world
        .get_resource::<LaborHandle>()
        .and_then(LaborHandle::get)
        .map(crate::labor::LaborMarket::hr_tuning)
    else {
        *world.resource_mut::<Firms>() = firms;
        return;
    };
    let odprawy = {
        let mut people = WorldWorkforce::new(world);
        crate::labor::hr::dismiss_all(&hr, &mut firms, &mut people, site, SimMinute(t.get()))
    };
    *world.resource_mut::<Firms>() = firms;
    for (c, odprawa) in odprawy {
        if odprawa.get() <= 0 {
            continue;
        }
        let id = fin.arrears_mut().accrue(
            AccountOwner::Firm(firm),
            AccountOwner::Citizen(c),
            odprawa,
            ClaimOrigin::Severance,
            t,
        );
        fin.file_claim_in(
            case,
            AccountOwner::Citizen(c),
            odprawa,
            ClaimOrigin::Severance,
            id,
            t,
        );
    }
}

/// Wstawia majątek zakładu do masy: wyposażenie po wartości księgowej netto
/// i zapas po koszcie.
fn wstaw_majatek(market: &Market, fin: &mut CorpFinance, case: BankruptcyId, site: SiteId) {
    let (wyposazenie, zapas) = market.book_assets_of(site);
    fin.add_lot(case, AssetRef::new(site, AssetKind::Equipment), wyposazenie);
    fin.add_lot(case, AssetRef::new(site, AssetKind::Inventory), zapas);
}

/// Posuwa otwarte postępowania o dobę i domyka te, które doszły do podziału.
fn posuwaj_postepowania(
    world: &mut World,
    market: &Market,
    fin: &mut CorpFinance,
    t: Tick,
    payouts: &mut Vec<SectorPayout>,
) {
    let otwarte: Vec<BankruptcyId> = fin.open_cases().collect();
    let rest = market.rest_of_world();
    let Some(mut books) = world.get_resource_mut::<Books>().map(std::mem::take) else {
        return;
    };
    for id in otwarte {
        if !fin.step_case(id, t) {
            continue;
        }
        // Wejście w `Distribution`: najpierw pieniądz za złom, potem podział.
        // Odwrotna kolejność dzieliłaby masę, w której złomu jeszcze nie ma.
        fin.settle_scrap(id, rest, &mut books, t);
        fin.distribute(id, rest, &mut books, t, payouts);
    }
    *world.resource_mut::<Books>() = books;
}

/// Dopisuje wypłaty do komponentów mieszkańców.
///
/// Druga połowa kanału `household_sector_out`: kwota zeszła już z ksiąg, więc
/// pominięcie tej pętli znaczyłoby pieniądz zgubiony między księgami a światem.
/// Mieszkaniec bez gospodarstwa nie istnieje, więc wypłata trafia do gospodarstwa,
/// w którym mieszka — tak samo jak pensja w `pay_incomes`.
fn wyplac_sektorowi(world: &mut World, payouts: &[SectorPayout]) {
    for p in payouts {
        let Some(hh) = gospodarstwo(world, p.to) else {
            continue;
        };
        if let Some(h) = world.get_mut::<Household>(hh) {
            h.bank = Money(h.bank.get().saturating_add(p.amount.get()));
        }
    }
}

/// Gospodarstwo, w którym mieszka ten człowiek. Ta sama droga, którą chodzi
/// decyzja zakupowa: indeks w `Identity`, rozwiązany przez rejestr demografii.
fn gospodarstwo(world: &World, c: CitizenId) -> Option<magnat_core::Entity> {
    let idx = world.get::<Identity>(c.0)?.household;
    demography::household_by_index(world, idx)
}
