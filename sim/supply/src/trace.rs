//! Ślad partii i graf dostawców — dane dla panelu łańcucha dostaw (WP13, §14.3–§14.4).
//!
//! Dwie funkcje i dwie struktury, żadnego formatowania: `engine/ui` dostaje liczby
//! i sam decyduje, jak je pokazać. To jest ta sama granica, którą M5 postawił przy
//! `ShopPanelSnapshot` — panel **nie liczy nic sam**, a symulacja nie zna ani jednego
//! `LocKey`.
//!
//! **Czego ten ślad nie robi i dlaczego to jest ważniejsze od tego, co robi.**
//! Łańcuch kończy się tam, gdzie kończą się dane, i mówi o tym wprost
//! ([`TraceOrigin`]): na złożu, na granicy albo na zapasie startowym świata.
//! Panel „od pola do półki" z PRD §14.4 ma powiedzieć „import" zamiast domalować
//! wiarygodny łańcuch do żyły, której nie ma — gracz, który raz przyłapie panel na
//! zmyślaniu, przestanie mu wierzyć także tam, gdzie panel ma rację (M6 §6.4.2).

use magnat_core::{DepositId, FirmId, GoodId, Mass, Money, SimMinute, SiteId, Q};

use crate::batch::{BatchId, TraceKind};
use crate::store::WarehouseRole;
use crate::{Chain, Store};

/// Jeden etap w historii partii.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TraceStage {
    pub batch: BatchId,
    pub at: SimMinute,
    pub site: Option<SiteId>,
    pub kind: TraceKind,
    pub mass: Mass,
    pub quality: Q,
    /// Koszt **narastający**: ile ta masa kosztowała w chwili tego etapu (§14.4).
    pub cost_cumulative: Money,
}

/// Gdzie ślad się kończy i dlaczego.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TraceOrigin {
    /// Doszedł do konkretnej żyły — to jest pełny łańcuch obiecany przez PRD §14.4.
    Deposit(DepositId),
    /// Kończy się na granicy: świata zewnętrznego nie symulujemy.
    Imported,
    /// Kończy się na zapasie, od którego świat wystartował.
    InitialStock,
    /// Partia nie była śledzona, więc historii po prostu nie ma — to jest brak danych,
    /// a nie brak pochodzenia, i panel ma je rozróżniać.
    NotTraced,
}

/// Pełny ślad partii — wejście trybu „śledź partię" (§14.4).
#[derive(Clone, Debug)]
pub struct BatchTrace {
    pub batch: BatchId,
    pub good: GoodId,
    /// Etapy od najstarszego przodka do stanu bieżącego.
    pub stages: Vec<TraceStage>,
    pub origin: TraceOrigin,
    /// Ile przetworzeń dzieli tę partię od surowca (`BatchOrigin::depth`).
    pub depth: u8,
}

impl BatchTrace {
    /// Czy ślad prowadzi do złoża — kryterium ukończenia WP13 dla łańcucha paliwa.
    #[must_use]
    pub fn reaches_deposit(&self) -> bool {
        matches!(self.origin, TraceOrigin::Deposit(_))
    }
}

/// Historia partii: etapy przodków, potem własne, w kolejności czasu.
///
/// **Przodkowie idą pierwsi i to jest cała różnica wobec `BatchLedger::trace`.**
/// Dziennik zna historię jednej partii — bochenek od wyjęcia z pieca. Łańcuch
/// „od pola do półki" zaczyna się na polu, czyli w partii, której ten bochenek jest
/// wnukiem, i bez przejścia po krawędziach pokrewieństwa urwałby się na pierwszym
/// przetworzeniu.
///
/// Wybór gałęzi przy wielu rodzicach: **najgłębsza**, a przy remisie najwcześniejsza.
/// Chleb ma czterech rodziców (mąka, woda, drożdże, sól) i pokazanie wszystkich
/// czterech drzew dałoby panel, którego nikt nie przeczyta; interesująca jest ta
/// gałąź, która sięga najdalej wstecz. Pozostałe są w `BatchLedger` i panel może
/// po nie sięgnąć, gdy gracz rozwinie etap.
#[must_use]
pub fn trace_batch(store: &Store, id: BatchId) -> BatchTrace {
    let mut stages = Vec::new();
    odwiedz(store, id, &mut stages, 0);
    stages.sort_by_key(|s| (s.at.0, s.batch.index()));

    let b = store.batch(id);
    let good = b.map_or(GoodId(0), |b| b.good);
    let depth = b.map_or(0, |b| b.origin.depth);
    let origin = if stages.is_empty() {
        TraceOrigin::NotTraced
    } else if let Some(d) = b.and_then(|b| b.origin.deposit) {
        TraceOrigin::Deposit(d)
    } else {
        // Pierwszy etap w czasie mówi, skąd masa weszła do świata: wyrób ma `Produced`,
        // dostawa zza granicy `Arrived`, zapas otwarcia `Stored`.
        match stages.first().map(|s| s.kind) {
            Some(TraceKind::Arrived) => TraceOrigin::Imported,
            Some(TraceKind::Stored) => TraceOrigin::InitialStock,
            _ => TraceOrigin::NotTraced,
        }
    };
    BatchTrace {
        batch: id,
        good,
        stages,
        origin,
        depth,
    }
}

/// Twardy sufit zagłębienia — [`crate::BatchOrigin::MAX_DEPTH`] razy dwa, bo podział
/// partii też jest krawędzią pokrewieństwa i nie zwiększa głębokości pochodzenia.
const MAX_KROKOW: u8 = 32;

fn odwiedz(store: &Store, id: BatchId, out: &mut Vec<TraceStage>, krok: u8) {
    if krok > MAX_KROKOW {
        return;
    }
    // Najgłębsza gałąź rodziców — patrz komentarz przy `trace_batch`.
    let rodzice = store.ledger().parents_of(id);
    if let Some(p) = rodzice
        .iter()
        .max_by_key(|p| store.batch(**p).map_or(0, |b| b.origin.depth))
    {
        odwiedz(store, *p, out, krok + 1);
    }
    for e in store.ledger().trace(id) {
        out.push(TraceStage {
            batch: e.batch,
            at: e.at,
            site: e.site,
            kind: e.kind,
            mass: e.mass,
            quality: e.quality,
            cost_cumulative: e.cost_cumulative,
        });
    }
}

// ── Graf dostawców ───────────────────────────────────────────────────────────────────

/// Krawędź przepływu towaru między zakładami.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SupplyEdge {
    pub from: SiteId,
    pub to: SiteId,
    pub good: GoodId,
    /// Masa objęta **obowiązującymi** zleceniami i kontraktami, nie historia.
    pub mass: Mass,
    /// Czy to jedyne źródło tego towaru dla odbiorcy — czerwona krawędź z §14.3.
    pub sole_source: bool,
}

/// Wiersz listy kontraktów z pokryciem zapasu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SupplyCoverage {
    pub site: SiteId,
    pub good: GoodId,
    pub stock: Mass,
    /// Na ile godzin starczy przy bieżącym zużyciu. `None` = zakład tego nie zużywa,
    /// więc pytanie nie ma odpowiedzi — a nie „starczy na zawsze".
    pub hours: Option<u32>,
    pub stage: crate::ShortageStage,
}

/// Graf dostawców i odbiorców jednej firmy (§14.3).
#[derive(Clone, Debug)]
pub struct SupplyGraphView {
    pub firm: FirmId,
    /// Zakłady tej firmy, w kolejności identyfikatorów.
    pub sites: Vec<SiteId>,
    pub inbound: Vec<SupplyEdge>,
    pub outbound: Vec<SupplyEdge>,
    pub coverage: Vec<SupplyCoverage>,
    /// Ile kontraktów firma ma podpisanych — jako kupujący i jako sprzedawca.
    pub contracts: (usize, usize),
}

/// Buduje graf dostawców firmy z **bieżących** zleceń, kontraktów i zapasów.
///
/// Historia przepływów tu nie wchodzi i to jest decyzja, nie brak: panel odpowiada
/// na pytanie „na czym ta firma stoi **teraz**", a strumień historyczny miesiąc po
/// miesiącu jest osobnym widokiem i osobnym kosztem pamięci. Ryzyko jednego dostawcy
/// liczy się z tego samego zbioru krawędzi, więc jest zawsze zgodne z tym, co widać.
#[must_use]
pub fn supply_graph(chain: &Chain, cat: &crate::Catalog, firm: FirmId) -> SupplyGraphView {
    let mut v = SupplyGraphView {
        firm,
        sites: Vec::new(),
        inbound: Vec::new(),
        outbound: Vec::new(),
        coverage: Vec::new(),
        contracts: (0, 0),
    };
    v.sites = chain
        .plant
        .iter()
        .filter(|(_, p)| p.owner == firm)
        .map(|(s, _)| s)
        .collect();
    v.sites.sort_unstable_by_key(|s| s.entity().index());
    if v.sites.is_empty() {
        return v;
    }

    for o in chain.transport.iter() {
        if o.state.is_final() {
            continue;
        }
        let moj_nadawca = v.sites.contains(&o.from);
        let moj_odbiorca = v.sites.contains(&o.to);
        if !moj_nadawca && !moj_odbiorca {
            continue;
        }
        let e = SupplyEdge {
            from: o.from,
            to: o.to,
            good: o.good,
            mass: o.mass,
            sole_source: false,
        };
        if moj_odbiorca {
            dolacz(&mut v.inbound, e);
        }
        if moj_nadawca {
            dolacz(&mut v.outbound, e);
        }
    }

    // Jeden dostawca = czerwona krawędź. Liczone per `(odbiorca, towar)`, bo ryzyko
    // dotyczy towaru, nie firmy: młyn z dwoma dostawcami zboża i jednym dostawcą
    // worków stoi tak samo, gdy zabraknie worków.
    for i in 0..v.inbound.len() {
        let (to, good) = (v.inbound[i].to, v.inbound[i].good);
        let ilu = v
            .inbound
            .iter()
            .filter(|e| e.to == to && e.good == good)
            .count();
        v.inbound[i].sole_source = ilu == 1;
    }

    for site in &v.sites {
        let Some(p) = chain.plant.get(*site) else {
            continue;
        };
        let mut towary: Vec<GoodId> = Vec::new();
        for l in &p.lines {
            let Some(r) = l.recipe else { continue };
            for we in &cat.recipe(r).inputs {
                if !towary.contains(&we.good) {
                    towary.push(we.good);
                }
            }
        }
        towary.sort_unstable_by_key(|g| g.0);
        for g in towary {
            let stock = crate::plant::input_stock(&chain.store, p, g);
            v.coverage.push(SupplyCoverage {
                site: *site,
                good: g,
                stock,
                hours: pokrycie_godzin(cat, p, g, stock),
                stage: p.stage(g),
            });
        }
    }

    v.contracts = (
        chain.b2b.contracts().filter(|c| c.buyer == firm).count(),
        chain.b2b.contracts().filter(|c| c.seller == firm).count(),
    );
    v
}

fn dolacz(v: &mut Vec<SupplyEdge>, e: SupplyEdge) {
    if let Some(x) = v
        .iter_mut()
        .find(|x| x.from == e.from && x.to == e.to && x.good == e.good)
    {
        x.mass = Mass(x.mass.0 + e.mass.0);
    } else {
        v.push(e);
    }
}

/// Pokrycie zapasu w godzinach przy nominalnym zużyciu linii zakładu.
fn pokrycie_godzin(
    cat: &crate::Catalog,
    p: &crate::PlantSite,
    g: GoodId,
    stock: Mass,
) -> Option<u32> {
    let na_dobe: i64 = p
        .lines
        .iter()
        .filter_map(|l| l.recipe)
        .map(|r| cat.recipe(r).daily_input(g))
        .sum();
    if na_dobe <= 0 {
        return None;
    }
    Some((stock.0.saturating_mul(24) / na_dobe).clamp(0, i64::from(u32::MAX)) as u32)
}

/// Partie stojące w slotach danej roli — wejście listy „co leży na półce" w panelu
/// i punkt wejścia trybu „śledź partię": gracz wskazuje bochenek, a nie uchwyt areny.
#[must_use]
pub fn batches_in_role(store: &Store, site: SiteId, role: WarehouseRole) -> Vec<BatchId> {
    let mut out = Vec::new();
    for s in store.slots_of(site) {
        if s.role == role {
            out.extend_from_slice(s.batches());
        }
    }
    out
}
