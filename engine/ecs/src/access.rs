//! Deklaracja dostępu systemu (M0 §5.6).
//!
//! Dostęp do komponentów **nie jest pisany ręcznie** — wynika z typów zapytania.
//! Ręczna deklaracja to gwarantowane źródło rozjazdu między tym, co system deklaruje,
//! a tym, co robi, a scheduler opiera na niej poprawność równoległości.

use crate::component::ComponentId;
use crate::resources::ResourceId;
use fixedbitset::FixedBitSet;

/// Zbiór dostępów jednego systemu: bitsety po `ComponentId` i po `ResourceId`.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct Access {
    reads_components: FixedBitSet,
    writes_components: FixedBitSet,
    reads_resources: FixedBitSet,
    writes_resources: FixedBitSet,
    /// Kolejne deklaracje w kolejności zgłoszenia — bitset gubi krotność,
    /// a bez krotności nie da się wykryć `Query<(&mut A, &A)>`.
    declared: Vec<(ComponentId, bool)>,
    /// System używa `CommandBuffer` ⇒ wymaga bariery po sobie.
    structural: bool,
}

impl Access {
    #[must_use]
    pub fn new() -> Access {
        Access::default()
    }

    pub fn read_component(&mut self, c: ComponentId) {
        grow_set(&mut self.reads_components, c.index());
        self.declared.push((c, false));
    }

    pub fn write_component(&mut self, c: ComponentId) {
        grow_set(&mut self.writes_components, c.index());
        // Zapis implikuje odczyt: system piszący do komponentu i tak go widzi,
        // a bez tego dwa zapisy do tego samego komponentu nie wyglądałyby na konflikt
        // przy porównaniu „write × read".
        grow_set(&mut self.reads_components, c.index());
        self.declared.push((c, true));
    }

    pub fn read_resource(&mut self, r: ResourceId) {
        grow_set(&mut self.reads_resources, r.index());
    }

    pub fn write_resource(&mut self, r: ResourceId) {
        grow_set(&mut self.writes_resources, r.index());
        grow_set(&mut self.reads_resources, r.index());
    }

    pub fn set_structural(&mut self, v: bool) {
        self.structural = v;
    }

    #[must_use]
    pub fn is_structural(&self) -> bool {
        self.structural
    }

    #[must_use]
    pub fn reads(&self, c: ComponentId) -> bool {
        self.reads_components.contains(c.index())
    }

    #[must_use]
    pub fn writes(&self, c: ComponentId) -> bool {
        self.writes_components.contains(c.index())
    }

    #[must_use]
    pub fn reads_resource(&self, r: ResourceId) -> bool {
        self.reads_resources.contains(r.index())
    }

    #[must_use]
    pub fn writes_resource(&self, r: ResourceId) -> bool {
        self.writes_resources.contains(r.index())
    }

    #[must_use]
    pub fn is_read_only(&self) -> bool {
        self.writes_components.count_ones(..) == 0
            && self.writes_resources.count_ones(..) == 0
            && !self.structural
    }

    /// Konflikt = `write × write` albo `read × write` na tym samym identyfikatorze.
    /// To jest **jedyna** podstawa krawędzi DAG (§5.7): jeśli dwa systemy nie
    /// konfliktują, nie widzą nawzajem swoich zapisów i mogą biec równolegle.
    #[must_use]
    pub fn conflicts_with(&self, other: &Access) -> bool {
        intersects(&self.writes_components, &other.reads_components)
            || intersects(&other.writes_components, &self.reads_components)
            || intersects(&self.writes_resources, &other.reads_resources)
            || intersects(&other.writes_resources, &self.reads_resources)
    }

    /// Sprzeczność wewnątrz jednego zapytania: ten sam komponent zadeklarowany
    /// dwa razy, w tym raz do zapisu. `Query<(&mut A, &A)>` dałoby wywołującemu
    /// `&mut A` i `&A` na ten sam wiersz — tego typ nie wyłapie, więc łapie to
    /// konstruktor zapytania.
    #[must_use]
    pub fn self_conflict(&self) -> Option<ComponentId> {
        for (i, (c, write)) in self.declared.iter().enumerate() {
            if !write {
                continue;
            }
            if self
                .declared
                .iter()
                .enumerate()
                .any(|(j, (other, _))| j != i && other == c)
            {
                return Some(*c);
            }
        }
        None
    }

    /// Wszystkie deklaracje w kolejności zgłoszenia — do komunikatów diagnostycznych.
    pub fn declarations(&self) -> impl Iterator<Item = (ComponentId, bool)> + '_ {
        self.declared.iter().copied()
    }
}

fn grow_set(set: &mut FixedBitSet, index: usize) {
    if set.len() <= index {
        set.grow(index + 1);
    }
    set.insert(index);
}

fn intersects(a: &FixedBitSet, b: &FixedBitSet) -> bool {
    a.ones().any(|i| b.contains(i))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cid(i: u16) -> ComponentId {
        ComponentId::from_raw(i)
    }

    #[test]
    fn odczyty_nie_konfliktuja() {
        let mut a = Access::new();
        a.read_component(cid(0));
        let mut b = Access::new();
        b.read_component(cid(0));
        assert!(!a.conflicts_with(&b));
        assert!(a.is_read_only());
    }

    #[test]
    fn zapis_konfliktuje_z_odczytem_i_z_zapisem() {
        let mut pisarz = Access::new();
        pisarz.write_component(cid(3));
        let mut czytelnik = Access::new();
        czytelnik.read_component(cid(3));
        assert!(pisarz.conflicts_with(&czytelnik));
        assert!(czytelnik.conflicts_with(&pisarz));
        assert!(pisarz.conflicts_with(&pisarz.clone()));
        assert!(!pisarz.is_read_only());
    }

    #[test]
    fn rozlaczne_komponenty_sa_zgodne() {
        let mut a = Access::new();
        a.write_component(cid(1));
        let mut b = Access::new();
        b.write_component(cid(2));
        assert!(!a.conflicts_with(&b));
    }

    #[test]
    fn zasoby_konfliktuja_tak_samo_jak_komponenty() {
        let mut a = Access::new();
        a.write_resource(ResourceId::from_index(0));
        let mut b = Access::new();
        b.read_resource(ResourceId::from_index(0));
        assert!(a.conflicts_with(&b));
        let mut c = Access::new();
        c.read_resource(ResourceId::from_index(1));
        assert!(!a.conflicts_with(&c));
    }

    #[test]
    fn sprzecznosc_w_jednym_zapytaniu_jest_wykrywalna() {
        let mut a = Access::new();
        a.write_component(cid(5));
        a.read_component(cid(5));
        assert_eq!(a.self_conflict(), Some(cid(5)));

        let mut b = Access::new();
        b.read_component(cid(5));
        b.read_component(cid(5));
        assert_eq!(b.self_conflict(), None, "dwa odczyty to nie konflikt");
    }

    #[test]
    fn system_strukturalny_nie_jest_tylko_do_odczytu() {
        let mut a = Access::new();
        a.set_structural(true);
        assert!(!a.is_read_only());
        assert!(a.is_structural());
    }
}
