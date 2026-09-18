//! Graf warstwowy: dostawcy i odbiorcy z przepływami (M9e, `DF-2`).
//!
//! # Dlaczego dopiero tutaj
//!
//! `M9b` miał go zbudować razem z rdzeniem UI i zmierzyć budżet. Zmierzenie widgetu
//! rysującego dane wymyślone na potrzeby pomiaru nie mówi nic o panelu, który go
//! potem dostanie (`DE-5`), więc graf powstaje **razem z pierwszym panelem, który
//! go żąda** — panelem Łańcuch dostaw.
//!
//! # Układ liczy się poza klatką i tylko przy zmianie topologii
//!
//! Warstwy wychodzą z najdłuższej ścieżki do węzła (Sugiyama w najprostszym
//! wydaniu: przypisanie warstw plus jedno przejście porządkujące). To jest jedyna
//! droga, którą koszt układu przestaje zależeć od liczby klatek: `hash` topologii
//! decyduje, czy przeliczać, a nie kamera i nie przewijanie.
//!
//! `ponytail:` bez minimalizacji przecięć krawędzi. Sufit nazwany: przy 500 węzłach
//! i 2000 krawędzi przecięcia bolą wzrokowo, ale barycentryczne porządkowanie to
//! drugi algorytm i drugi zestaw testów. Ścieżka wyjścia: warstwy i porządek są
//! rozdzielone, więc dołożenie przejścia barycentrycznego zmienia jedną funkcję.

use crate::theme::{ColorToken, TextRole, Theme};

/// Węzeł grafu: co pokazać i co otworzyć po kliknięciu.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GraphNode {
    pub label: String,
    /// Podmiot, którego kartę otwiera kliknięcie.
    pub subject: Option<magnat_core::Subject>,
    /// Czy węzeł jest **ryzykiem** — jedyny dostawca towaru. Czerwony w palecie,
    /// a obok tego napis: kolor nigdy nie niesie znaczenia sam (`ui-design.md` §7).
    pub risk: bool,
}

/// Krawędź: od kogo, do kogo i jak gruba.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GraphEdge {
    pub from: u32,
    pub to: u32,
    /// Wolumen w promilach największego przepływu — grubość linii.
    pub weight_permille: u16,
}

/// Graf z policzonym układem.
#[derive(Default)]
pub struct GraphView {
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    /// `(warstwa, pozycja w warstwie)` dla każdego węzła.
    layout: Vec<(u32, u32)>,
    layers: u32,
    /// Odcisk topologii, przy którym policzono układ.
    stamp: u128,
}

impl GraphView {
    /// Podmienia treść. Układ przelicza się **wyłącznie**, gdy zmieniła się topologia.
    pub fn set(&mut self, nodes: Vec<GraphNode>, edges: Vec<GraphEdge>) {
        let stamp = odcisk(&nodes, &edges);
        self.nodes = nodes;
        self.edges = edges;
        if stamp != self.stamp {
            self.stamp = stamp;
            self.relayout();
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    #[must_use]
    pub fn layers(&self) -> u32 {
        self.layers
    }

    /// Warstwa węzła — wejście testu „układ jest warstwowy".
    #[must_use]
    pub fn layer_of(&self, node: usize) -> Option<u32> {
        self.layout.get(node).map(|(l, _)| *l)
    }

    /// Przypisanie warstw: warstwa węzła to najdłuższa ścieżka prowadząca do niego.
    ///
    /// Cykl w grafie dostaw jest możliwy (A sprzedaje B, B sprzedaje A), więc pętla
    /// ma **twardy sufit** równy liczbie węzłów: bez niego cykl kręciłby się do
    /// przepełnienia, a graf dostaw jest danymi z gry, nie z założenia.
    fn relayout(&mut self) {
        let n = self.nodes.len();
        let mut warstwa = vec![0u32; n];
        for _ in 0..n {
            let mut zmiana = false;
            for e in &self.edges {
                let (a, b) = (e.from as usize, e.to as usize);
                if a >= n || b >= n || a == b {
                    continue;
                }
                if warstwa[b] <= warstwa[a] {
                    warstwa[b] = warstwa[a] + 1;
                    zmiana = true;
                }
            }
            if !zmiana {
                break;
            }
        }
        // Cykl podbija warstwy przy każdym przebiegu, więc bez przycięcia graf
        // z dwoma węzłami dostawał pięć warstw i zjeżdżał w lewą piątą płótna.
        // Warstw nie może być więcej niż węzłów — najdłuższa ścieżka prosta ma
        // tyle wierzchołków, ile graf.
        let n32 = u32::try_from(n).unwrap_or(u32::MAX).max(1);
        for w in &mut warstwa {
            *w = (*w).min(n32 - 1);
        }
        self.layers = warstwa.iter().copied().max().map_or(0, |m| m + 1);
        let mut licznik: Vec<u32> = vec![0; self.layers as usize + 1];
        self.layout = warstwa
            .iter()
            .map(|w| {
                let i = *w as usize;
                let p = licznik[i];
                licznik[i] += 1;
                (*w, p)
            })
            .collect();
    }

    /// Rysuje graf. Zwraca podmiot klikniętego węzła.
    pub fn show(
        &self,
        ui: &mut egui::Ui,
        theme: &Theme,
        height: f32,
    ) -> Option<magnat_core::Subject> {
        let (rect, odp) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), height),
            egui::Sense::click(),
        );
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0.0, theme.color(ColorToken::BgCard));
        if self.nodes.is_empty() {
            return None;
        }
        let w_warstwy = rect.width() / (self.layers.max(1) as f32);
        let szer_wezla = (w_warstwy * 0.8).min(theme.gap(30));
        let w_max = self
            .layout
            .iter()
            .map(|(_, poz)| *poz)
            .max()
            .map_or(1.0, |m| (m + 1) as f32);
        let h_wezla = (rect.height() / w_max).min(theme.gap(6));

        let srodek = |i: usize| -> egui::Pos2 {
            let (l, poz) = self.layout[i];
            egui::pos2(
                rect.left() + (l as f32 + 0.5) * w_warstwy,
                rect.top() + (poz as f32 + 0.5) * h_wezla,
            )
        };

        for e in &self.edges {
            let (a, b) = (e.from as usize, e.to as usize);
            if a >= self.nodes.len() || b >= self.nodes.len() {
                continue;
            }
            let grubosc = 1.0 + f32::from(e.weight_permille) / 1000.0 * 3.0;
            p.line_segment(
                [srodek(a), srodek(b)],
                egui::Stroke::new(grubosc, theme.color(ColorToken::LineStrong)),
            );
        }

        let mut klikniety = None;
        for (i, w) in self.nodes.iter().enumerate() {
            let c = srodek(i);
            let r = egui::Rect::from_center_size(c, egui::vec2(szer_wezla, h_wezla * 0.8));
            p.rect_filled(
                r,
                2.0,
                theme.color(if w.risk {
                    ColorToken::Danger
                } else {
                    ColorToken::BgPanel
                }),
            );
            p.text(
                c,
                egui::Align2::CENTER_CENTER,
                &w.label,
                theme.mono(TextRole::Micro),
                theme.color(ColorToken::TextPrimary),
            );
            if odp.clicked() && odp.interact_pointer_pos().is_some_and(|q| r.contains(q)) {
                klikniety = w.subject;
            }
        }
        klikniety
    }
}

/// Odcisk topologii: liczba węzłów i para końców każdej krawędzi.
///
/// **Bez wag** — zmiana wolumenu zmienia grubość linii, a nie położenie węzła,
/// więc przeliczanie układu po każdej dobie kosztowałoby za nic.
fn odcisk(nodes: &[GraphNode], edges: &[GraphEdge]) -> u128 {
    let mut h = magnat_core::hash::StateHasher::new();
    h.write_u32(u32::try_from(nodes.len()).unwrap_or(u32::MAX));
    for e in edges {
        h.write_u32(e.from);
        h.write_u32(e.to);
    }
    h.finish().0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wezel(n: &str) -> GraphNode {
        GraphNode {
            label: n.to_string(),
            subject: None,
            risk: false,
        }
    }

    #[test]
    fn lancuch_uklada_sie_w_warstwy() {
        let mut g = GraphView::default();
        g.set(
            vec![wezel("pole"), wezel("mlyn"), wezel("piekarnia")],
            vec![
                GraphEdge {
                    from: 0,
                    to: 1,
                    weight_permille: 1000,
                },
                GraphEdge {
                    from: 1,
                    to: 2,
                    weight_permille: 500,
                },
            ],
        );
        assert_eq!(g.layers(), 3, "trzy ogniwa to trzy warstwy");
        assert_eq!(g.layer_of(0), Some(0));
        assert_eq!(g.layer_of(2), Some(2));
    }

    #[test]
    fn cykl_nie_zapetla_ukladu() {
        let mut g = GraphView::default();
        g.set(
            vec![wezel("a"), wezel("b")],
            vec![
                GraphEdge {
                    from: 0,
                    to: 1,
                    weight_permille: 1,
                },
                GraphEdge {
                    from: 1,
                    to: 0,
                    weight_permille: 1,
                },
            ],
        );
        assert!(g.layers() >= 1, "cykl ma dostać jakikolwiek układ");
        assert!(
            g.layers() <= 2,
            "dwa węzły nie mogą dać więcej niż dwie warstwy, a dały {}",
            g.layers()
        );
    }
}
