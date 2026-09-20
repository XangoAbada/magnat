//! Wykonanie doby badań: pieniądz, półki i zakłady (M10c WP10.8, WP10.9).
//!
//! **Reguły są w `sim/firms::rnd`, tutaj jest ich wykonanie** — ten sam podział, którym
//! M7 oddaje listę płac: `sim/firms` liczy, kto ile zebrał punktów i co z tego wynika,
//! a `sim/economy` przenosi pieniądz, dopisuje towar do asortymentu i przestawia linie
//! produkcyjne. Odwrotnie być nie może, bo `sim/firms` nie widzi `Books` i widzieć
//! nie będzie: zależność idzie `economy → firms`.
//!
//! Krok jest **dobowy** i woła go `LaborSystem`, a nie własny system. Powód jest ten
//! sam, dla którego kadry siedzą w `labor::hr`: badania prowadzą ludzie, a doba rynku
//! pracy jest jedynym miejscem, w którym naraz stoją rejestr firm, tablica ról
//! i świat z ich formą. Osobny system musiałby wyjąć rejestr z ECS-u drugi raz w tej
//! samej dobie.

use magnat_core::{DecisionReason, GoodId, Money, SimMinute, TechId, Tick};
use magnat_ecs::World;
use magnat_firms::rnd::{ChargeKind, PlantEffectKind, RndCharge, RndDay, RndOutcome};
use magnat_firms::{firm_id, Firms, RndData, RoleTable};

use crate::books::{Books, TxKind, TxMemo};

/// Klucz roli badacza w `data/jobs/roles.ron`.
pub const RESEARCHER_ROLE: &str = "researcher";

/// Obciążenie z rozwiązanymi kontami: skąd i dokąd. `None` po stronie odbiorcy
/// znaczy „reszta świata" — odczynniki przyjeżdżają spoza miasta.
type DoZaplaty = (
    RndCharge,
    Option<crate::books::AccountId>,
    Option<crate::books::AccountId>,
);

/// Jedna doba badań miasta — **cała treść kroku**, wołalna bez schedulera.
///
/// Nic nie zwraca i to jest świadome: licznik „ile dziś odkryto" liczyłby się
/// w każdej dobie po to, żeby wylądować w koszu, bo stan badań jest w `Firms::rnd()`
/// i tam go czyta i panel, i raport scenariusza. Wariant bez czytelnika przechodzi
/// każdy test i wygląda tak samo jak działający (`K-67`).
pub fn step_day(world: &mut World, firms: &mut Firms, roles: &RoleTable, tick: Tick) {
    let Some(data) = world.get_resource::<RndData>().cloned() else {
        return;
    };
    if data.tree.is_empty() {
        return;
    }
    let chain = world.get_resource::<magnat_supply::ChainHandle>().cloned();
    let dzien = RndDay {
        day: u32::try_from(tick.0 / 1_440).unwrap_or(u32::MAX),
        now: SimMinute(tick.0),
        tick,
        world_seed: world.seed,
        roles,
        researcher: roles.id(RESEARCHER_ROLE),
        vitals: &|c| {
            let v = *world.get::<magnat_agents::Vitals>(c.0)?;
            let emp = *world.get::<magnat_agents::Employment>(c.0)?;
            let skill = world
                .get::<magnat_agents::Skills>(c.0)
                .map_or(magnat_core::Q::MIN, |s| s.level_in(emp.role));
            Some((v, skill))
        },
    };
    // Numer umowy pochodzi **wyłącznie** z licznika `B2b` — drugi licznik dałby dwie
    // umowy o tym samym numerze. Świat bez łańcucha dostaw nie ma tego licznika
    // i **nie zawiera licencji wcale**: numer zastępczy byłby stały, więc druga
    // licencja po cichu nadpisałaby pierwszą, a opłata wstępna zostałaby pobrana
    // za obie.
    let mut mint = || chain.as_ref().map(|ch| ch.lock().b2b.mint_contract_id());

    let wynik = magnat_firms::rnd::step_day(firms, &data, &dzien, &mut mint);
    zapisz_powody(firms, &data, &wynik, dzien.now, tick);
    zaplac(world, firms, &wynik, tick);
    if let Some(ch) = &chain {
        zastosuj_w_zakladach(ch, &wynik);
    }
    wpusc_na_polki(world, firms, &wynik, tick);
}

/// Powody decyzji w dzienniku firmy — wyjaśnialność z 00 §7.
fn zapisz_powody(firms: &mut Firms, data: &RndData, w: &RndOutcome, now: SimMinute, tick: Tick) {
    for (key, tech) in &w.started {
        let Some(p) = firms.rnd().projects.get(key).copied() else {
            continue;
        };
        let node = data.tree.node(*tech);
        // Prognoza liczona z tempa **w chwili decyzji**: pełny budżet, dzisiejsza obsada.
        let miesiecy = p.months_left(
            u64::from(data.tuning.mrp_per_full_time_day) * u64::from(node.gain.max(1)) / 10,
        );
        if let Some(f) = firms.get_mut(*key) {
            f.log_decision(
                tick,
                DecisionReason::ResearchStarted {
                    tech: *tech,
                    cost_rp: node.cost_rp,
                    months_est: miesiecy,
                },
            );
        }
    }
    for (key, tech, patent) in &w.discovered {
        let node = data.tree.node(*tech);
        let miesiecy = u16::try_from((now.0 / 1_440 / 30).max(1)).unwrap_or(u16::MAX);
        if let Some(f) = firms.get_mut(*key) {
            f.log_decision(
                tick,
                DecisionReason::TechDiscovered {
                    tech: *tech,
                    patented: *patent,
                    rp_spent: node.cost_rp,
                    months: miesiecy,
                },
            );
        }
    }
    for (key, tech, licensor, _) in &w.licensed {
        let royalty = firms
            .rnd()
            .licenses
            .values()
            .find(|l| l.tech == *tech && l.licensee == *key)
            .map_or(0, |l| l.royalty_bp);
        if let Some(f) = firms.get_mut(*key) {
            f.log_decision(
                tick,
                DecisionReason::LicenseSigned {
                    tech: *tech,
                    licensor: firm_id(*licensor),
                    royalty_bp: royalty,
                },
            );
        }
    }
}

/// Obciążenia badawcze: przelew i koszt w rachunku wyniku zakładu.
///
/// **Przelew jest pierwszy, a nie ostatni**, i to jest różnica wobec kampanii
/// reklamowej z M10b (`F-23`): tam księga zakładu mogła zapis odrzucić, więc trzeba
/// było zapytać ją przed ruszeniem pieniądza. Tu koszt idzie do `Site::accrue_rnd`,
/// które odmówić nie może — więc porządek odwraca się i pieniądz rozstrzyga.
///
/// Firma płaci **tyle, ile ma**. Udział opłaconej kwoty wraca do projektu jako tempo
/// następnego miesiąca: laboratorium bez materiałów nie stoi, tylko zwalnia.
fn zaplac(world: &mut World, firms: &mut Firms, w: &RndOutcome, tick: Tick) {
    if w.charges.is_empty() {
        return;
    }
    let (reszta_swiata, konta): (crate::books::AccountId, Vec<DoZaplaty>) = {
        let Some(market) = world.get_resource::<crate::Market>() else {
            return;
        };
        let lista = w
            .charges
            .iter()
            .map(|c| {
                let od = market.account_of_firm(firm_id(c.firm));
                let do_kogo = match c.kind {
                    ChargeKind::Materials { .. } => None,
                    ChargeKind::LicenseUpfront { licensor, .. }
                    | ChargeKind::Royalty { licensor, .. } => {
                        market.account_of_firm(firm_id(licensor))
                    }
                };
                (*c, od, do_kogo)
            })
            .collect();
        (market.rest_of_world(), lista)
    };
    for (c, od, do_kogo) in konta {
        // **Zapłacono, a nie „miało być zapłacone".** Tempo następnego miesiąca liczy
        // się z tej liczby, więc firma bez konta rynkowego i firma, której przelew
        // odrzucono, mają tu zero — a nie pełny budżet. Pierwsza wersja liczyła udział
        // z kwoty **żądanej**, czyli dawała darmowe badania każdemu, komu przelew
        // nie przeszedł.
        let mut zaplacono = Money::ZERO;
        if let Some(od) = od {
            if let Some(books) = world.get_resource_mut::<Books>() {
                let stan = books.balance(od).map_or(0, magnat_core::Money::get);
                let kwota = Money(c.amount.get().min(stan.max(0)));
                if kwota.get() > 0 {
                    let cel = do_kogo.unwrap_or(reszta_swiata);
                    let memo = TxMemo::new(tx_kind(&c), powod(&c));
                    if books.transfer(od, cel, kwota, memo, tick).is_ok() {
                        zaplacono = kwota;
                        if let Some(s) = firms.site_mut(c.site) {
                            s.accrue_rnd(kwota);
                        }
                    }
                }
            }
        }
        if let ChargeKind::Materials { .. } = c.kind {
            let udzial = if c.amount.get() > 0 {
                u16::try_from(zaplacono.get().saturating_mul(1_000) / c.amount.get())
                    .unwrap_or(1_000)
            } else {
                1_000
            };
            if let Some(p) = firms.rnd_mut().projects.get_mut(&c.firm) {
                p.budget_permille = udzial.min(1_000);
            }
        }
    }
}

fn tx_kind(c: &RndCharge) -> TxKind {
    match c.kind {
        ChargeKind::Materials { tech } => TxKind::RndSpend { tech },
        ChargeKind::LicenseUpfront { tech, .. } | ChargeKind::Royalty { tech, .. } => {
            TxKind::LicenseFee { tech }
        }
    }
}

fn powod(c: &RndCharge) -> DecisionReason {
    match c.kind {
        ChargeKind::Materials { tech } => DecisionReason::ResearchStarted {
            tech,
            cost_rp: 0,
            months_est: 0,
        },
        ChargeKind::LicenseUpfront { tech, licensor } | ChargeKind::Royalty { tech, licensor } => {
            DecisionReason::LicenseSigned {
                tech,
                licensor: firm_id(licensor),
                royalty_bp: 0,
            }
        }
    }
}

/// Efekty technologii w zakładach produkcyjnych (`K-44`: M6 czyta gotową liczbę).
fn zastosuj_w_zakladach(chain: &magnat_supply::ChainHandle, w: &RndOutcome) {
    if w.plant_effects.is_empty() {
        return;
    }
    let cat = chain.cat.clone();
    let mut ch = chain.lock();
    for e in &w.plant_effects {
        let Some(p) = ch.plant.get_mut(e.site) else {
            continue;
        };
        match &e.kind {
            PlantEffectKind::UtilityForRecipe { recipe, bps } => {
                if p.lines.iter().any(|l| l.recipe == Some(*recipe)) {
                    p.utility_bonus_bp = p
                        .utility_bonus_bp
                        .saturating_add(*bps)
                        .min(magnat_supply::PlantSite::MAX_UTILITY_BONUS_BP);
                }
            }
            PlantEffectKind::Machine {
                class,
                throughput_bps,
                failure_bps,
            } => {
                let Some(id) = cat.machine_class_id(class) else {
                    continue;
                };
                for l in p.lines.iter_mut().filter(|l| l.machine_class == id) {
                    l.nominal_throughput = magnat_core::Mass(
                        l.nominal_throughput.0
                            + l.nominal_throughput.0 * i64::from(*throughput_bps) / 10_000,
                    );
                    let mtbf = i64::from(l.mtbf_hours)
                        + i64::from(l.mtbf_hours) * i64::from(*failure_bps) / 10_000;
                    l.mtbf_hours = u32::try_from(mtbf.max(1)).unwrap_or(l.mtbf_hours);
                }
            }
        }
    }
}

/// Towar świeżo wpuszczony do obiegu trafia na półki sklepów swojej kategorii.
///
/// To jest widoczna strona `TechEffect::NewGood`: przed odkryciem towaru nie ma
/// na żadnej półce i nie ma go w żadnym koszyku wydatków, po odkryciu jest w obu.
/// Powód „nowy wyrób na półkach" dostaje **odkrywca**, nie sklep: sklep niczego
/// nie postanowił, tylko wystawił to, co się pojawiło.
fn wpusc_na_polki(world: &mut World, firms: &mut Firms, w: &RndOutcome, tick: Tick) {
    if w.unlocked.is_empty() {
        return;
    }
    let wystawione: Vec<(magnat_firms::FirmKey, TechId, GoodId, u16)> = {
        let Some(market) = world.get_resource::<crate::Market>() else {
            return;
        };
        w.unlocked
            .iter()
            // Odsianie powtórzenia jest po stronie rynku, bo to on trzyma stan:
            // drugi odkrywca tego samego węzła dostaje zero i nie ma czym się chwalić.
            .filter(|(_, _, g)| market.is_good_locked(*g))
            .map(|(key, tech, g)| {
                let n = market.stock_new_good(*g, tick);
                (*key, *tech, *g, n)
            })
            .collect()
    };
    for (key, tech, good, shops) in wystawione {
        if let Some(f) = firms.get_mut(key) {
            f.log_decision(tick, DecisionReason::ProductLaunched { good, tech, shops });
        }
    }
}
