//! Doba badań: wybór projektu, akumulacja punktów, przełom, odkrycie (M10c §5.4).
//!
//! **Wszystko tutaj jest funkcją nad rejestrem firm** — bez `World`, bez `Books`,
//! bez katalogu towarów. Pieniądz wychodzi listą obciążeń do zaksięgowania
//! ([`RndCharge`]), zmiany w zakładach produkcyjnych listą efektów
//! ([`PlantEffect`]), a wykonuje jedno i drugie `sim/economy` — ten sam podział,
//! którym M7 oddaje listę płac i pokrycie etatowe (`K-44`).
//!
//! **Element losowy jest dokładnie jeden**: przełom. Wybór projektu, tempo, patent
//! i wygaśnięcie są funkcjami stanu, więc firma o znanej obsadzie odkrywa węzeł
//! w dobie, którą da się policzyć ołówkiem — i to jest kryterium WP10.8.

use magnat_core::{
    rng, CitizenId, ContractId, GoodId, JobRoleId, Money, RecipeId, SimMinute, StreamId, TechId,
    Tick, Q,
};

use super::state::{ChargeKind, License, Patent, Project, RndCharge};
use super::tree::{TechEffect, TechNode};
use super::RndData;
use crate::hr::roles::RoleTable;
use crate::key::FirmKey;
use crate::registry::Firms;

/// Milipunkty na punkt badawczy.
pub const MRP: u64 = 1_000;

/// Doba kalendarza `K-1`.
const DAYS_PER_MONTH: u32 = 30;
const DAYS_PER_YEAR: u32 = 360;

/// Wejścia doby badań, których rejestr firm sam nie ma.
pub struct RndDay<'a> {
    pub day: u32,
    pub now: SimMinute,
    pub tick: Tick,
    pub world_seed: u64,
    pub roles: &'a RoleTable,
    /// Rola badacza z `data/jobs/roles.ron`. `None` w świecie bez tej roli —
    /// wtedy badania nie ruszają i to jest poprawny stan, nie błąd.
    pub researcher: Option<JobRoleId>,
    pub vitals:
        &'a dyn Fn(
            CitizenId,
        )
            -> Option<(magnat_agents::Vitals, Q, magnat_agents::DeprivationPressure)>,
}

/// Zmiana w zakładzie produkcyjnym, którą R&D zleca, a wykonuje `sim/economy`.
///
/// `sim/firms` nie sięga do `ChainHandle`: to ten sam podział, którym pokrycie
/// etatowe wraca listą par `(SiteId, promile)` (`K-44`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PlantEffect {
    pub site: magnat_core::SiteId,
    pub kind: PlantEffectKind,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PlantEffectKind {
    /// Zniżka na media dla zakładu, który prowadzi tę recepturę.
    UtilityForRecipe { recipe: RecipeId, bps: u16 },
    /// Ulepszenie linii tej klasy maszyn: przepustowość i MTBF w punktach bazowych.
    ///
    /// `ponytail:` ulepszenie stosuje się **raz, w chwili odkrycia**, do linii, które
    /// wtedy stały. Linia postawiona później nie dostanie go z mocą wsteczną. Sufit
    /// nazwany; droga wyjścia — przeliczanie mnożnika z listy znanych technologii
    /// przy każdym starcie szarży, kiedy ktoś zmierzy, że to robi różnicę.
    Machine {
        class: Box<str>,
        throughput_bps: u16,
        failure_bps: i16,
    },
}

/// Co doba badań zostawia wołającemu.
#[derive(Clone, Default, Debug)]
pub struct RndOutcome {
    pub charges: Vec<RndCharge>,
    pub plant_effects: Vec<PlantEffect>,
    /// Firmy, które w tej dobie zaczęły projekt.
    pub started: Vec<(FirmKey, TechId)>,
    /// Odkrycia: firma, węzeł, czy z patentem.
    pub discovered: Vec<(FirmKey, TechId, bool)>,
    /// Towary wpuszczone do obiegu w tej dobie, razem z tym, czyja technologia
    /// je wypuściła: **odkrywca, węzeł, towar**. Trójka, a nie sam `GoodId`, bo
    /// powód „nowy wyrób na półkach" należy do tego, kto go wymyślił, a nie do
    /// sklepu, który go wystawił.
    pub unlocked: Vec<(FirmKey, TechId, GoodId)>,
    /// Podpisane licencje: licencjobiorca, węzeł, licencjodawca, numer umowy.
    pub licensed: Vec<(FirmKey, TechId, FirmKey, ContractId)>,
}

/// Jedna doba badań całego miasta.
///
/// Kolejność jest kolejnością `FirmKey`, bo po niej idzie losowanie przełomu
/// i nadanie patentu — a „kto był pierwszy" jest w tym mechanizmie całą nagrodą
/// (00 §3.2).
pub fn step_day(
    firms: &mut Firms,
    data: &RndData,
    d: &RndDay<'_>,
    mint: &mut dyn FnMut() -> Option<ContractId>,
) -> RndOutcome {
    let mut out = RndOutcome::default();
    if data.tree.is_empty() {
        return out;
    }
    let Some(researcher) = d.researcher else {
        return out;
    };
    let miesiac = d.day.is_multiple_of(DAYS_PER_MONTH) && d.day > 0;

    for key in firms.active_keys() {
        let Some((praca, badaczy, lead)) = zdolnosc(firms, key, researcher, d) else {
            continue;
        };
        if miesiac {
            miesieczne(firms, data, d, key, badaczy, lead, mint, &mut out);
        }
        if praca == 0 {
            continue;
        }
        wybierz_projekt(firms, data, d, key, &mut out);
        akumuluj(firms, data, d, key, praca, lead, &mut out);
    }
    if miesiac {
        royalty(firms, d, &mut out);
    }
    out
}

/// Opłaty licencyjne za miniony miesiąc — **osobny przebieg po licencjach**, a nie
/// krok w pętli firm badawczych.
///
/// Powód jest twardy i był pierwszą rzeczą, którą ta funkcja naprawiła: licencjobiorca
/// **nie musi mieć badaczy**. Kupił dostęp do cudzej technologii właśnie dlatego, że
/// sam jej nie zdobędzie — a pętla firm badawczych przechodzi wyłącznie po tych, które
/// mają kogo posadzić w laboratorium. Royalty naliczane tam byłoby zapisane w umowie
/// i nigdy nie zapłacone.
///
/// Podstawą jest **utarg zakładów licencjobiorcy z ostatniego domkniętego miesiąca**.
/// `ponytail:` utarg nie jest dzielony na wyroby, więc firma wytwarzająca dwie rzeczy
/// płaci royalty od obu. Sufit nazwany; droga wyjścia prowadzi przez utarg per towar
/// w `SitePnlMonth`, którego M7 nie ma i którego M10c nie dokłada dla jednej stawki.
fn royalty(firms: &mut Firms, d: &RndDay<'_>, out: &mut RndOutcome) {
    let czynne: Vec<(FirmKey, FirmKey, TechId, u16)> = firms
        .rnd()
        .licenses
        .values()
        .filter(|l| d.now.0 < l.expires.0)
        .map(|l| (l.licensee, l.licensor, l.tech, l.royalty_bp))
        .collect();
    for (licensee, licensor, tech, bp) in czynne {
        let Some(sites) = firms.firm(licensee).map(|f| f.sites.to_vec()) else {
            continue;
        };
        // **Ostatni domknięty miesiąc, a nie ostatni wpis pierścienia.** Lista płac
        // wkłada do `pnl` wpis z zerowym utargiem w dniu wypłaty firmy, a utarg
        // dopisuje mu dopiero domknięcie okresu — więc `last()` w dobie naliczenia
        // trafiał czasem w ten pusty wpis i royalty wychodziło systematycznie zerem.
        // Mechanizm byłby wtedy martwy w każdym grającym mieście i nie złamałby
        // ani jednego testu, bo testy stawiają `pnl` ręcznie.
        let biezacy = d.day / DAYS_PER_MONTH;
        let mut utarg: i64 = 0;
        for id in &sites {
            let Some(site) = firms.site(*id) else {
                continue;
            };
            if let Some(m) = site.pnl.iter().rev().find(|m| m.month < biezacy) {
                utarg = utarg.saturating_add(m.revenue.get());
            }
        }
        let kwota = Money(utarg.saturating_mul(i64::from(bp)) / 10_000);
        // Zakład o najniższym `SiteId` przyjmuje koszt — ten sam porządek, którym
        // wybiera się zakład prowadzący projekt, i z tego samego powodu.
        let Some(site) = sites.iter().min().copied() else {
            continue;
        };
        if kwota.get() > 0 {
            out.charges.push(RndCharge {
                firm: licensee,
                site,
                amount: kwota,
                kind: ChargeKind::Royalty { tech, licensor },
            });
        }
    }
}

/// Praca badawcza firmy, liczba badaczy i zakład prowadzący projekt.
///
/// Zakładem prowadzącym jest ten o **najniższym `SiteId`** spośród mających badaczy —
/// deterministycznie i bez losowania. To jego indeks encji jest kluczem rzutu na
/// przełom, żeby firma z trzema laboratoriami nie losowała dla nich jednego rzutu.
fn zdolnosc(
    firms: &Firms,
    key: FirmKey,
    researcher: JobRoleId,
    d: &RndDay<'_>,
) -> Option<(i64, u32, magnat_core::SiteId)> {
    let mut praca = 0i64;
    let mut badaczy = 0u32;
    let mut lead: Option<magnat_core::SiteId> = None;
    for id in firms.firm(key)?.sites.clone() {
        let Some(site) = firms.site(id) else { continue };
        let n = site.researchers(researcher);
        if n == 0 {
            continue;
        }
        badaczy += n;
        praca += site.research_labor(d.roles, researcher, &d.vitals).0;
        // **Najniższy `SiteId`, a nie pierwszy na liście firmy.** `Firm.sites` rośnie
        // w kolejności otwierania zakładów, więc „pierwszy" zależy od historii firmy,
        // a nie od jej stanu — i rozjeżdżałby się z regułą, którą `royalty` stosuje
        // do tej samej firmy. Jedna reguła „zakładu prowadzącego" na moduł.
        lead = Some(lead.map_or(id, |l| l.min(id)));
    }
    lead.map(|l| (praca, badaczy, l))
}

/// Wybór projektu: najtańszy osiągalny węzeł, przy remisie niższy `TechId`.
///
/// „Osiągalny" znaczy: firma go nie zna, zna wszystkie warunki wstępne i nikt inny
/// nie trzyma na nim czynnego patentu. Węzeł zablokowany cudzym patentem nie jest
/// kandydatem na badania — jest kandydatem na licencję, a tę rozstrzyga
/// [`miesieczne`].
fn wybierz_projekt(
    firms: &mut Firms,
    data: &RndData,
    d: &RndDay<'_>,
    key: FirmKey,
    out: &mut RndOutcome,
) {
    if firms.rnd().projects.contains_key(&key) {
        return;
    }
    let Some(t) = najtanszy_dostepny(firms, data, d, key) else {
        return;
    };
    let node = data.tree.node(t);
    let koszt = node.effective_cost_rp(u64::from(d.day), data.tuning.world_known_discount_bp);
    firms
        .rnd_mut()
        .projects
        .insert(key, Project::new(t, koszt, d.now));
    out.started.push((key, t));
}

/// Najtańszy węzeł, który firma może zacząć badać.
fn najtanszy_dostepny(
    firms: &Firms,
    data: &RndData,
    d: &RndDay<'_>,
    key: FirmKey,
) -> Option<TechId> {
    let st = firms.rnd();
    let mut best: Option<(u32, TechId)> = None;
    for n in &data.tree.nodes {
        if st.knows(key, n.id) || !n.prereqs.iter().all(|p| st.knows(key, *p)) {
            continue;
        }
        if st.blocked_by(key, n.id, d.now).is_some() {
            continue;
        }
        let koszt = n.effective_cost_rp(u64::from(d.day), data.tuning.world_known_discount_bp);
        if best.is_none_or(|(c, _)| koszt < c) {
            best = Some((koszt, n.id));
        }
    }
    best.map(|(_, t)| t)
}

/// Dobowa akumulacja punktów, rzut na przełom i ewentualne odkrycie.
fn akumuluj(
    firms: &mut Firms,
    data: &RndData,
    d: &RndDay<'_>,
    key: FirmKey,
    praca: i64,
    lead: magnat_core::SiteId,
    out: &mut RndOutcome,
) {
    let Some(p) = firms.rnd().projects.get(&key).copied() else {
        return;
    };
    let dzienne = mrp_per_day(praca, p.budget_permille, data.tuning.mrp_per_full_time_day);
    let mut p = p;
    p.done_mrp = p.done_mrp.saturating_add(dzienne);
    if let Some(ciecie) = przelom(data, d, lead) {
        let zostalo = p.remaining_mrp();
        p.done_mrp = p
            .done_mrp
            .saturating_add(zostalo * u64::from(ciecie) / 10_000);
        p.breakthroughs = p.breakthroughs.saturating_add(1);
    }
    if p.done_mrp < p.cost_mrp {
        firms.rnd_mut().projects.insert(key, p);
        return;
    }
    firms.rnd_mut().projects.remove(&key);
    odkryj(firms, data, d, key, p, out);
}

/// Milipunkty na dobę: praca × tempo × opłacony budżet, każde dzielenie osobno.
///
/// Kolejność działań jest częścią wyniku i nie wolno jej „uprościć" — ta sama
/// zasada, co w [`crate::effective_labor`].
#[must_use]
pub fn mrp_per_day(praca_milietaty: i64, budget_permille: u16, mrp_per_ft: u32) -> u64 {
    if praca_milietaty <= 0 {
        return 0;
    }
    let mut v = praca_milietaty as u64 * u64::from(mrp_per_ft) / 1_000;
    v = v * u64::from(budget_permille) / 1_000;
    v
}

/// Rzut na przełom. `None` znaczy zwykłą dobę, czyli prawie każdą.
fn przelom(data: &RndData, d: &RndDay<'_>, lead: magnat_core::SiteId) -> Option<u16> {
    if data.tuning.breakthrough_per_10k_day == 0 {
        return None;
    }
    let mut r = rng(
        d.world_seed,
        StreamId::RnDBreakthrough,
        lead.entity().index(),
        d.tick,
    );
    if r.gen_range_u32(10_000) >= u32::from(data.tuning.breakthrough_per_10k_day) {
        return None;
    }
    let (lo, hi) = data.tuning.breakthrough_cut_bp;
    let rozpietosc = u32::from(hi - lo) + 1;
    Some(lo + u16::try_from(r.gen_range_u32(rozpietosc)).unwrap_or(0))
}

/// Odkrycie: wiedza, patent, poziom wyposażenia, odblokowania i efekty w zakładach.
fn odkryj(
    firms: &mut Firms,
    data: &RndData,
    d: &RndDay<'_>,
    key: FirmKey,
    p: Project,
    out: &mut RndOutcome,
) {
    let node = data.tree.node(p.tech);
    firms.rnd_mut().learn(key, p.tech);

    let patent = node.patentable(u64::from(d.day)) && !firms.rnd().patents.contains_key(&p.tech);
    if patent {
        let lata = u64::from(data.tuning.patent_years) * u64::from(DAYS_PER_YEAR) * 1_440;
        firms.rnd_mut().patents.insert(
            p.tech,
            Patent {
                tech: p.tech,
                owner: key,
                granted: d.now,
                expires: SimMinute(d.now.0.saturating_add(lata)),
            },
        );
    }
    zastosuj(firms, node, key, out);
    out.discovered.push((key, p.tech, patent));
}

/// Skutki odkrycia: wyposażenie zakładów firmy, towary w obiegu, zlecenia do M6.
fn zastosuj(firms: &mut Firms, node: &TechNode, key: FirmKey, out: &mut RndOutcome) {
    let sites: Vec<magnat_core::SiteId> = firms
        .firm(key)
        .map(|f| f.sites.to_vec())
        .unwrap_or_default();
    if node.gain > 0 {
        for id in &sites {
            if let Some(s) = firms.site_mut(*id) {
                s.raise_tech(node.gain);
            }
        }
    }
    for e in &node.effects {
        match e {
            TechEffect::NewGood(g) => {
                // Zgłoszenie, nie zapis: **kto jest w obiegu, wie rynek**, bo to on
                // ma półki i oferty. Druga tablica odblokowań po tej stronie byłaby
                // drugą prawdą o tej samej rzeczy — a odsiać powtórzenie (dwie firmy
                // odkrywają ten sam węzeł) umie ten, kto trzyma stan.
                out.unlocked.push((key, node.id, *g));
            }
            TechEffect::CostReduction { recipe, bps } => {
                for id in &sites {
                    out.plant_effects.push(PlantEffect {
                        site: *id,
                        kind: PlantEffectKind::UtilityForRecipe {
                            recipe: *recipe,
                            bps: *bps,
                        },
                    });
                }
            }
            TechEffect::MachineUpgrade {
                machine_class,
                throughput_bps,
                failure_bps,
            } => {
                for id in &sites {
                    out.plant_effects.push(PlantEffect {
                        site: *id,
                        kind: PlantEffectKind::Machine {
                            class: machine_class.clone(),
                            throughput_bps: *throughput_bps,
                            failure_bps: *failure_bps,
                        },
                    });
                }
            }
        }
    }
}

/// Granica miesiąca: budżet materiałowy, royalty i ewentualna licencja.
#[allow(clippy::too_many_arguments)]
fn miesieczne(
    firms: &mut Firms,
    data: &RndData,
    d: &RndDay<'_>,
    key: FirmKey,
    badaczy: u32,
    lead: magnat_core::SiteId,
    mint: &mut dyn FnMut() -> Option<ContractId>,
    out: &mut RndOutcome,
) {
    let kurs = firms
        .firm(key)
        .map_or(magnat_core::FirmStrategy::Cautious, |f| f.strategy);
    if let Some(p) = firms.rnd().projects.get(&key).copied() {
        let kwota = data.tuning.target_budget(badaczy, kurs);
        if kwota.0 > 0 {
            out.charges.push(RndCharge {
                firm: key,
                site: lead,
                amount: kwota,
                kind: ChargeKind::Materials { tech: p.tech },
            });
        }
    } else {
        licencja(firms, data, d, key, lead, mint, out);
    }
}

/// Licencja: firma, która nie ma czego badać, bo najtańszy sensowny węzeł trzyma
/// cudzy patent, kupuje do niego dostęp.
///
/// Stawka **nie jest losowana** — rośnie z tym, ile życia patentu zostało. Patent
/// świeży kosztuje górną krańcówkę widełek, wygasający za rok dolną.
fn licencja(
    firms: &mut Firms,
    data: &RndData,
    d: &RndDay<'_>,
    key: FirmKey,
    lead: magnat_core::SiteId,
    mint: &mut dyn FnMut() -> Option<ContractId>,
    out: &mut RndOutcome,
) {
    let Some((tech, licensor)) = zablokowany(firms, data, d, key) else {
        return;
    };
    let Some(p) = firms.rnd().patents.get(&tech).copied() else {
        return;
    };
    let (lo, hi) = data.tuning.royalty_bp;
    let zycie = p.life_left_permille(d.now);
    let royalty = lo + u16::try_from(u32::from(hi - lo) * zycie / 1000).unwrap_or(0);
    let node = data.tree.node(tech);
    let wstepna = Money(
        data.tuning
            .upfront_per_rp_gr
            .saturating_mul(i64::from(node.cost_rp)),
    );
    let Some(contract) = mint() else {
        // Świat bez rejestru umów nie zawiera licencji. Lepiej nie zawrzeć niż
        // nadać numer, który zderzy się z następnym.
        return;
    };
    firms.rnd_mut().licenses.insert(
        contract.entity().index(),
        License {
            contract,
            tech,
            licensor,
            licensee: key,
            royalty_bp: royalty,
            signed: d.now,
            expires: p.expires,
        },
    );
    firms.rnd_mut().learn(key, tech);
    zastosuj(firms, node, key, out);
    out.charges.push(RndCharge {
        firm: key,
        site: lead,
        amount: wstepna,
        kind: ChargeKind::LicenseUpfront { tech, licensor },
    });
    out.licensed.push((key, tech, licensor, contract));
}

/// Najtańszy węzeł, którego firma nie zna i który blokuje jej cudzy patent.
fn zablokowany(
    firms: &Firms,
    data: &RndData,
    d: &RndDay<'_>,
    key: FirmKey,
) -> Option<(TechId, FirmKey)> {
    let st = firms.rnd();
    let mut best: Option<(u32, TechId, FirmKey)> = None;
    for n in &data.tree.nodes {
        if st.knows(key, n.id) || !n.prereqs.iter().all(|p| st.knows(key, *p)) {
            continue;
        }
        let Some(wl) = st.blocked_by(key, n.id, d.now) else {
            continue;
        };
        let koszt = n.effective_cost_rp(u64::from(d.day), data.tuning.world_known_discount_bp);
        if best.is_none_or(|(c, _, _)| koszt < c) {
            best = Some((koszt, n.id, wl));
        }
    }
    best.map(|(_, t, w)| (t, w))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tempo_zgadza_sie_z_rachunkiem_z_pliku_strojenia() {
        // Czterech badaczy o umiejętności 60 w formie to 2960 milietatów
        // (rachunek w nagłówku `data/tuning/rnd.ron`).
        assert_eq!(mrp_per_day(2_960, 1_000, 1_590), 4_706);
        // Połowa budżetu to połowa tempa — laboratorium bez materiałów zwalnia,
        // a nie staje.
        assert_eq!(mrp_per_day(2_960, 500, 1_590), 2_353);
        assert_eq!(mrp_per_day(0, 1_000, 1_590), 0);
    }

    #[test]
    fn dziewiec_miesiecy_na_wezel_za_1200_rp() {
        // Kryterium WP10.8, policzone wprost: 1 200 000 mRP przy 4706 mRP na dobę.
        let dob = 1_200_000u64.div_ceil(4_706);
        assert_eq!(dob, 255);
        // Miesiąc ma 30 dób (`K-1`), więc doba 255 wypada w dziewiątym.
        assert_eq!((dob - 1) / 30 + 1, 9);
    }
}
