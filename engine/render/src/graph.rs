//! Frame graph — rejestracja passów z jawną deklaracją zasobów (M1 §5.8, WP-R1).
//!
//! `RenderGraph` **nie jest zamkniętą listą passów**. M3 dokłada instancing encji, M4 pojazdy,
//! M11 sześć własnych passów (cap przekroju, impostory, cząstki pogody, dym, tłum) — i robi to
//! przez `register`, bez dotykania passów M1. To jest zobowiązanie uzgodnione wprost
//! z planem M11 (M1 §6.1) i dlatego graf powstaje w R1, a nie „kiedyś, gdy będzie potrzebny".
//!
//! **Cykl i zapis do niezadeklarowanego zasobu są błędem wykrywanym przy budowie grafu,
//! nie w runtime.** Różnica jest praktyczna: błąd przy budowie widać na pierwszym uruchomieniu
//! i w teście, a błąd w runtime — dopiero wtedy, gdy ktoś włączy akurat ten wariant jakości.

use std::collections::{BTreeMap, BTreeSet};

/// Identyfikator passa nadawany przy rejestracji.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct PassId(pub u16);

/// Uchwyt zasobu ramki (tekstury albo bufora).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ResourceRef(pub u16);

/// Nazwany punkt wpięcia. Graf gwarantuje kolejność slotów względem passów M1,
/// więc faza dokładająca pass nie musi znać jego sąsiadów po nazwie.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum GraphSlot {
    PreDepth,
    PostDepth,
    PreOpaque,
    PostOpaque,
    PreWater,
    PostWater,
    PreOverlay,
    PrePost,
    PostPost,
    /// Poza łańcuchem klatki — cienie, regeneracja impostorów, prekomputacje.
    Offscreen,
}

impl GraphSlot {
    /// Kolejność slotów w klatce. `Offscreen` idzie **przed** wszystkim, bo jego wyniki
    /// (kaskady cieni) są wejściem passów widocznych.
    #[must_use]
    pub const fn order(self) -> u8 {
        match self {
            GraphSlot::Offscreen => 0,
            GraphSlot::PreDepth => 1,
            GraphSlot::PostDepth => 2,
            GraphSlot::PreOpaque => 3,
            GraphSlot::PostOpaque => 4,
            GraphSlot::PreWater => 5,
            GraphSlot::PostWater => 6,
            GraphSlot::PreOverlay => 7,
            GraphSlot::PrePost => 8,
            GraphSlot::PostPost => 9,
        }
    }
}

/// Opis zasobu ramki.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ResourceDesc {
    pub name: &'static str,
}

/// Deklaracja passa: co czyta, co zapisuje.
#[derive(Clone, Default, Debug)]
pub struct PassDecl {
    pub reads: Vec<ResourceRef>,
    pub writes: Vec<ResourceRef>,
}

impl PassDecl {
    pub fn read(&mut self, r: ResourceRef) -> &mut Self {
        self.reads.push(r);
        self
    }

    pub fn write(&mut self, r: ResourceRef) -> &mut Self {
        self.writes.push(r);
        self
    }
}

/// Pass renderu.
pub trait RenderPass: Send + Sync {
    fn name(&self) -> &'static str;
    fn slot(&self) -> GraphSlot;
    /// Deklaracja zasobów. Wołana **raz**, przy budowie grafu.
    fn declare(&self, d: &mut PassDecl);
}

/// Błąd budowy grafu.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum GraphError {
    /// Pass czyta zasób, którego nikt nie zapisuje.
    UnwrittenRead {
        pass: &'static str,
        resource: &'static str,
    },
    /// Dwa passy zapisują ten sam zasób — graf nie umie rozstrzygnąć, który jest źródłem.
    DoubleWrite {
        resource: &'static str,
        first: &'static str,
        second: &'static str,
    },
    /// Cykl: pass czyta zasób zapisywany przez pass, który stoi za nim w kolejności slotów.
    Cycle {
        pass: &'static str,
        resource: &'static str,
        producer: &'static str,
    },
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphError::UnwrittenRead { pass, resource } => write!(
                f,
                "pass `{pass}` czyta zasób `{resource}`, którego nikt nie zapisuje"
            ),
            GraphError::DoubleWrite {
                resource,
                first,
                second,
            } => write!(
                f,
                "zasób `{resource}` zapisują dwa passy: `{first}` i `{second}`"
            ),
            GraphError::Cycle {
                pass,
                resource,
                producer,
            } => write!(
                f,
                "cykl: `{pass}` czyta `{resource}`, który powstaje dopiero w `{producer}`"
            ),
        }
    }
}

impl std::error::Error for GraphError {}

struct Entry {
    pass: Box<dyn RenderPass>,
    decl: PassDecl,
}

/// Graf klatki.
#[derive(Default)]
pub struct RenderGraph {
    resources: Vec<ResourceDesc>,
    passes: Vec<Entry>,
    /// Kolejność wyliczona przez `build`; pusta, dopóki graf nie został zbudowany.
    order: Vec<PassId>,
}

impl RenderGraph {
    #[must_use]
    pub fn new() -> RenderGraph {
        RenderGraph::default()
    }

    pub fn resource(&mut self, desc: ResourceDesc) -> ResourceRef {
        self.resources.push(desc);
        ResourceRef((self.resources.len() - 1) as u16)
    }

    pub fn register(&mut self, pass: Box<dyn RenderPass>) -> PassId {
        let mut decl = PassDecl::default();
        pass.declare(&mut decl);
        self.passes.push(Entry { pass, decl });
        self.order.clear();
        PassId((self.passes.len() - 1) as u16)
    }

    #[must_use]
    pub fn pass_count(&self) -> usize {
        self.passes.len()
    }

    #[must_use]
    pub fn resource_name(&self, r: ResourceRef) -> &'static str {
        self.resources
            .get(r.0 as usize)
            .map_or("<nieznany>", |d| d.name)
    }

    /// Wyznacza kolejność wykonania i sprawdza spójność.
    ///
    /// Kolejność jest **stabilna**: sloty, a w obrębie slotu kolejność rejestracji.
    /// Sortowanie topologiczne dałoby to samo dla poprawnego grafu, a dla niepoprawnego
    /// ukryłoby błąd, przestawiając passy — wolimy, żeby cykl był błędem, a nie przetasowaniem.
    pub fn build(&mut self) -> Result<&[PassId], GraphError> {
        // Kto zapisuje który zasób.
        let mut producent: BTreeMap<u16, (usize, &'static str)> = BTreeMap::new();
        for (i, e) in self.passes.iter().enumerate() {
            for w in &e.decl.writes {
                if let Some((_, first)) = producent.get(&w.0) {
                    return Err(GraphError::DoubleWrite {
                        resource: self.resources[w.0 as usize].name,
                        first,
                        second: e.pass.name(),
                    });
                }
                producent.insert(w.0, (i, e.pass.name()));
            }
        }

        let mut kolejnosc: Vec<PassId> = (0..self.passes.len() as u16).map(PassId).collect();
        kolejnosc.sort_by_key(|p| {
            let e = &self.passes[p.0 as usize];
            (e.pass.slot().order(), p.0)
        });

        // Pozycja passa w wyliczonej kolejności — po niej sprawdzamy cykle.
        let mut pozycja: BTreeMap<u16, usize> = BTreeMap::new();
        for (i, p) in kolejnosc.iter().enumerate() {
            pozycja.insert(p.0, i);
        }

        let mut czytane: BTreeSet<u16> = BTreeSet::new();
        for p in &kolejnosc {
            let e = &self.passes[p.0 as usize];
            for r in &e.decl.reads {
                czytane.insert(r.0);
                let Some((prod_idx, prod_name)) = producent.get(&r.0) else {
                    return Err(GraphError::UnwrittenRead {
                        pass: e.pass.name(),
                        resource: self.resources[r.0 as usize].name,
                    });
                };
                let prod_pos = pozycja[&(*prod_idx as u16)];
                if prod_pos > pozycja[&p.0] {
                    return Err(GraphError::Cycle {
                        pass: e.pass.name(),
                        resource: self.resources[r.0 as usize].name,
                        producer: prod_name,
                    });
                }
            }
        }

        self.order = kolejnosc;
        Ok(&self.order)
    }

    /// Kolejność wykonania po udanym `build`.
    #[must_use]
    pub fn order(&self) -> &[PassId] {
        &self.order
    }

    #[must_use]
    pub fn pass_name(&self, id: PassId) -> &'static str {
        self.passes[id.0 as usize].pass.name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Testowy {
        name: &'static str,
        slot: GraphSlot,
        reads: Vec<ResourceRef>,
        writes: Vec<ResourceRef>,
    }

    impl RenderPass for Testowy {
        fn name(&self) -> &'static str {
            self.name
        }
        fn slot(&self) -> GraphSlot {
            self.slot
        }
        fn declare(&self, d: &mut PassDecl) {
            for r in &self.reads {
                d.read(*r);
            }
            for w in &self.writes {
                d.write(*w);
            }
        }
    }

    fn pass(
        name: &'static str,
        slot: GraphSlot,
        reads: &[ResourceRef],
        writes: &[ResourceRef],
    ) -> Box<dyn RenderPass> {
        Box::new(Testowy {
            name,
            slot,
            reads: reads.to_vec(),
            writes: writes.to_vec(),
        })
    }

    #[test]
    fn kolejnosc_idzie_po_slotach_a_potem_po_rejestracji() {
        let mut g = RenderGraph::new();
        let depth = g.resource(ResourceDesc { name: "depth" });
        let color = g.resource(ResourceDesc { name: "color" });
        // Rejestracja celowo w złej kolejności — graf ma ją uporządkować.
        g.register(pass("post", GraphSlot::PrePost, &[color], &[]));
        g.register(pass("opaque", GraphSlot::PreOpaque, &[depth], &[color]));
        g.register(pass("prepass", GraphSlot::PreDepth, &[], &[depth]));

        let order = g.build().expect("graf ma być poprawny").to_vec();
        let nazwy: Vec<&str> = order.iter().map(|p| g.pass_name(*p)).collect();
        assert_eq!(nazwy, ["prepass", "opaque", "post"]);
    }

    #[test]
    fn czytanie_niezapisanego_zasobu_to_blad_budowy() {
        let mut g = RenderGraph::new();
        let cienie = g.resource(ResourceDesc { name: "shadow" });
        g.register(pass("opaque", GraphSlot::PreOpaque, &[cienie], &[]));
        let e = g.build().unwrap_err();
        assert!(
            matches!(
                e,
                GraphError::UnwrittenRead {
                    resource: "shadow",
                    ..
                }
            ),
            "{e}"
        );
    }

    #[test]
    fn dwa_zapisy_tego_samego_zasobu_to_blad() {
        let mut g = RenderGraph::new();
        let color = g.resource(ResourceDesc { name: "color" });
        g.register(pass("a", GraphSlot::PreOpaque, &[], &[color]));
        g.register(pass("b", GraphSlot::PostOpaque, &[], &[color]));
        let e = g.build().unwrap_err();
        assert!(matches!(e, GraphError::DoubleWrite { .. }), "{e}");
    }

    #[test]
    fn cykl_jest_wykrywany_przy_budowie_a_nie_w_runtime() {
        // M1 §7.3 `render_graph_acyclic`.
        let mut g = RenderGraph::new();
        let a = g.resource(ResourceDesc { name: "a" });
        // Pass wcześniejszy czyta zasób powstający później.
        g.register(pass("wczesny", GraphSlot::PreDepth, &[a], &[]));
        g.register(pass("pozny", GraphSlot::PostPost, &[], &[a]));
        let e = g.build().unwrap_err();
        assert!(
            matches!(
                e,
                GraphError::Cycle {
                    pass: "wczesny",
                    ..
                }
            ),
            "{e}"
        );
    }

    #[test]
    fn offscreen_wyprzedza_cala_klatke() {
        // Kaskady cieni powstają przed passem, który je czyta — i to jest powód,
        // dla którego `Offscreen` ma numer 0, a nie numer na końcu.
        let mut g = RenderGraph::new();
        let shadow = g.resource(ResourceDesc { name: "shadow" });
        g.register(pass("opaque", GraphSlot::PreOpaque, &[shadow], &[]));
        g.register(pass("shadow", GraphSlot::Offscreen, &[], &[shadow]));
        let order = g.build().expect("cienie mają wyprzedzać opaque").to_vec();
        assert_eq!(g.pass_name(order[0]), "shadow");
    }

    #[test]
    fn rejestracja_po_zbudowaniu_uniewaznia_kolejnosc() {
        let mut g = RenderGraph::new();
        let a = g.resource(ResourceDesc { name: "a" });
        g.register(pass("x", GraphSlot::PreOpaque, &[], &[a]));
        g.build().unwrap();
        assert_eq!(g.order().len(), 1);
        g.register(pass("y", GraphSlot::PostOpaque, &[a], &[]));
        assert!(
            g.order().is_empty(),
            "stara kolejność przetrwała rejestrację nowego passa"
        );
        assert_eq!(g.build().unwrap().len(), 2);
    }
}
