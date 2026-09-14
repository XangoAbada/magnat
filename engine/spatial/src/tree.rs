//! Quadtree nad AABB parcel (M2 §5.1).
//!
//! Parcele różnią się powierzchnią o trzy rzędy wielkości (działka R2 ~400 m²,
//! pole rolne ~20 ha), więc siatka jednorodna albo tonie w kandydatach, albo
//! w pustych komórkach. Drzewo jest **regionowe ze straddlerami**: element trafia
//! do najgłębszego węzła, który go w całości zawiera, a jeśli leży na granicy
//! ćwiartek — zostaje w rodzicu. Dzięki temu wielki obszar nie schodzi do liści
//! i nie jest duplikowany w czterech gałęziach.

use crate::geom::{Aabb2, Vec2};
use magnat_core::ParcelId;

/// Parametry z §5.1. Brak wstawiania i usuwania — parcele M2 są statyczne;
/// podziały i scalenia działek dokłada M5.
const MAX_DEPTH: u32 = 12;
const BUCKET: usize = 16;
const NO_NODE: u32 = u32::MAX;

#[derive(Clone, Copy, Debug)]
struct QNode {
    bounds: Aabb2,
    /// Zakres elementów własnych (straddlerów albo zawartości liścia).
    first: u32,
    len: u32,
    children: [u32; 4],
}

pub struct ParcelTree {
    nodes: Vec<QNode>,
    aabbs: Vec<Aabb2>,
    ids: Vec<ParcelId>,
}

impl ParcelTree {
    /// Budowa wsadowa. Kolejność elementów w węźle = kolejność wejściowa,
    /// więc wynik zapytania nie zależy od niczego poza danymi.
    #[must_use]
    pub fn build(items: &[(Aabb2, ParcelId)]) -> ParcelTree {
        let mut t = ParcelTree {
            nodes: Vec::new(),
            aabbs: Vec::with_capacity(items.len()),
            ids: Vec::with_capacity(items.len()),
        };
        if items.is_empty() {
            t.nodes.push(QNode {
                bounds: Aabb2::EMPTY,
                first: 0,
                len: 0,
                children: [NO_NODE; 4],
            });
            return t;
        }
        // Kwadrat opisany na wszystkim: ćwiartki dzielą się wtedy równo w obu osiach,
        // a płaska mapa nie robi z drzewa listy.
        let mut b = Aabb2::EMPTY;
        for (a, _) in items {
            b = b.union(*a);
        }
        let bok = f32::max(b.size().x, b.size().y).max(f32::MIN_POSITIVE);
        let root = Aabb2::new(b.min, b.min + Vec2::splat(bok));
        let idx: Vec<u32> = (0..items.len() as u32).collect();
        t.split(root, &idx, 0, items);
        t
    }

    fn split(
        &mut self,
        bounds: Aabb2,
        idx: &[u32],
        depth: u32,
        items: &[(Aabb2, ParcelId)],
    ) -> u32 {
        let me = self.nodes.len() as u32;
        self.nodes.push(QNode {
            bounds,
            first: self.aabbs.len() as u32,
            len: 0,
            children: [NO_NODE; 4],
        });

        if idx.len() <= BUCKET || depth == MAX_DEPTH {
            for &i in idx {
                self.aabbs.push(items[i as usize].0);
                self.ids.push(items[i as usize].1);
            }
            self.nodes[me as usize].len = idx.len() as u32;
            return me;
        }

        let c = bounds.center();
        let cwiartki = [
            Aabb2::new(bounds.min, c),
            Aabb2::new(Vec2::new(c.x, bounds.min.y), Vec2::new(bounds.max.x, c.y)),
            Aabb2::new(Vec2::new(bounds.min.x, c.y), Vec2::new(c.x, bounds.max.y)),
            Aabb2::new(c, bounds.max),
        ];
        let mut dzieci: [Vec<u32>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
        let mut wlasne: Vec<u32> = Vec::new();
        for &i in idx {
            let a = items[i as usize].0;
            match cwiartki.iter().position(|q| zawiera(*q, a)) {
                Some(q) => dzieci[q].push(i),
                None => wlasne.push(i),
            }
        }

        for &i in &wlasne {
            self.aabbs.push(items[i as usize].0);
            self.ids.push(items[i as usize].1);
        }
        self.nodes[me as usize].len = wlasne.len() as u32;

        for (q, lista) in dzieci.iter().enumerate() {
            if lista.is_empty() {
                continue;
            }
            let dziecko = self.split(cwiartki[q], lista, depth + 1, items);
            self.nodes[me as usize].children[q] = dziecko;
        }
        me
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Parcela zawierająca punkt.
    ///
    /// **Odstępstwo od §5.1:** zamiast `&PolyArena` (typ `sim/world`, którego
    /// `engine/spatial` nie może zobaczyć bez odwrócenia zależności) bierze
    /// predykat dokładnego testu. AABB zawęża kandydatów do garstki, o wielokącie
    /// rozstrzyga wywołujący. Parcele się nie nakładają (test T5), więc pierwsze
    /// trafienie jest jedynym.
    #[must_use]
    pub fn at_point(&self, p: Vec2, contains: impl Fn(ParcelId) -> bool) -> Option<ParcelId> {
        self.szukaj(0, p, &contains)
    }

    /// Zejście w **każdą** ćwiartkę zawierającą punkt, nie w pierwszą lepszą:
    /// prostokąty są domknięte, więc punkt na miedzy należy do dwóch ćwiartek naraz,
    /// a szukana parcela siedzi tylko w jednej z nich.
    fn szukaj(&self, node: u32, p: Vec2, contains: &dyn Fn(ParcelId) -> bool) -> Option<ParcelId> {
        let n = self.nodes[node as usize];
        for i in n.first..n.first + n.len {
            let i = i as usize;
            if self.aabbs[i].contains(p) && contains(self.ids[i]) {
                return Some(self.ids[i]);
            }
        }
        for c in n.children {
            if c != NO_NODE && self.nodes[c as usize].bounds.contains(p) {
                if let Some(id) = self.szukaj(c, p, contains) {
                    return Some(id);
                }
            }
        }
        None
    }

    pub fn query_rect(&self, a: Aabb2, out: &mut Vec<ParcelId>) {
        out.clear();
        self.zbieraj(0, out, &|b| b.intersects(a));
    }

    pub fn query_segment(&self, a: Vec2, b: Vec2, out: &mut Vec<ParcelId>) {
        out.clear();
        self.zbieraj(0, out, &|box2| box2.intersects_segment(a, b));
    }

    fn zbieraj(&self, node: u32, out: &mut Vec<ParcelId>, test: &dyn Fn(Aabb2) -> bool) {
        let n = self.nodes[node as usize];
        if n.len == 0 && n.children.iter().all(|c| *c == NO_NODE) {
            return;
        }
        if !test(n.bounds) {
            return;
        }
        for i in n.first..n.first + n.len {
            let i = i as usize;
            if test(self.aabbs[i]) {
                out.push(self.ids[i]);
            }
        }
        for c in n.children {
            if c != NO_NODE {
                self.zbieraj(c, out, test);
            }
        }
    }
}

fn zawiera(zewn: Aabb2, wewn: Aabb2) -> bool {
    wewn.min.x >= zewn.min.x
        && wewn.min.y >= zewn.min.y
        && wewn.max.x <= zewn.max.x
        && wewn.max.y <= zewn.max.y
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::{rng, Entity, StreamId, Tick};
    use std::num::NonZeroU32;

    fn pid(i: u32) -> ParcelId {
        ParcelId(Entity::new(i, NonZeroU32::new(1).unwrap()))
    }

    /// Parcele o rozpiętości trzech rzędów wielkości — dokładnie przypadek,
    /// dla którego siatka jednorodna nie działa.
    fn dzialki(n: u32) -> Vec<(Aabb2, ParcelId)> {
        let mut r = rng(0xC0FFEE, StreamId::EngineSelfTest, 0, Tick(0));
        (0..n)
            .map(|i| {
                let x = r.gen_range_u32(8_000) as f32;
                let y = r.gen_range_u32(8_000) as f32;
                let bok = if i % 50 == 0 {
                    400.0 + r.gen_range_u32(100) as f32
                } else {
                    10.0 + r.gen_range_u32(30) as f32
                };
                (
                    Aabb2::new(Vec2::new(x, y), Vec2::new(x + bok, y + bok)),
                    pid(i + 1),
                )
            })
            .collect()
    }

    #[test]
    fn prostokat_zgadza_sie_z_przegladem_zupelnym() {
        let items = dzialki(2_000);
        let t = ParcelTree::build(&items);
        assert_eq!(t.len(), items.len());
        let mut r = rng(7, StreamId::EngineSelfTest, 0, Tick(0));
        let mut out = Vec::new();
        for _ in 0..200 {
            let x = r.gen_range_u32(8_000) as f32;
            let y = r.gen_range_u32(8_000) as f32;
            let q = Aabb2::new(Vec2::new(x, y), Vec2::new(x + 300.0, y + 200.0));
            t.query_rect(q, &mut out);
            let mut moje: Vec<u32> = out.iter().map(|p| p.0.index()).collect();
            let mut wzorzec: Vec<u32> = items
                .iter()
                .filter(|(a, _)| a.intersects(q))
                .map(|(_, p)| p.0.index())
                .collect();
            moje.sort_unstable();
            wzorzec.sort_unstable();
            assert_eq!(moje, wzorzec);
        }
    }

    #[test]
    fn odcinek_zgadza_sie_z_przegladem_zupelnym() {
        let items = dzialki(1_000);
        let t = ParcelTree::build(&items);
        let mut r = rng(11, StreamId::EngineSelfTest, 0, Tick(0));
        let mut out = Vec::new();
        for _ in 0..200 {
            let a = Vec2::new(r.gen_range_u32(8_000) as f32, r.gen_range_u32(8_000) as f32);
            let b = Vec2::new(r.gen_range_u32(8_000) as f32, r.gen_range_u32(8_000) as f32);
            t.query_segment(a, b, &mut out);
            let mut moje: Vec<u32> = out.iter().map(|p| p.0.index()).collect();
            let mut wzorzec: Vec<u32> = items
                .iter()
                .filter(|(x, _)| x.intersects_segment(a, b))
                .map(|(_, p)| p.0.index())
                .collect();
            moje.sort_unstable();
            wzorzec.sort_unstable();
            assert_eq!(moje, wzorzec);
        }
    }

    #[test]
    fn punkt_trafia_w_parcele_a_predykat_moze_odrzucic() {
        // Siatka rozłącznych działek 20×20 — brak nakładek, więc odpowiedź jest jedna.
        let items: Vec<(Aabb2, ParcelId)> = (0..400u32)
            .map(|i| {
                let (x, y) = ((i % 20) as f32 * 25.0, (i / 20) as f32 * 25.0);
                (
                    Aabb2::new(Vec2::new(x, y), Vec2::new(x + 20.0, y + 20.0)),
                    pid(i + 1),
                )
            })
            .collect();
        let t = ParcelTree::build(&items);
        for (a, id) in &items {
            assert_eq!(t.at_point(a.center(), |_| true), Some(*id));
        }
        // Przerwa między działkami: AABB nie trafia.
        assert_eq!(t.at_point(Vec2::new(22.0, 22.0), |_| true), None);
        // Predykat wielokąta odrzuca wszystko → brak odpowiedzi mimo trafienia w AABB.
        assert_eq!(t.at_point(items[0].0.center(), |_| false), None);
    }

    #[test]
    fn puste_drzewo_nie_wywraca_zapytan() {
        let t = ParcelTree::build(&[]);
        let mut out = Vec::new();
        t.query_rect(Aabb2::new(Vec2::ZERO, Vec2::splat(100.0)), &mut out);
        assert!(out.is_empty());
        assert_eq!(t.at_point(Vec2::ZERO, |_| true), None);
        assert!(t.is_empty());
    }
}
