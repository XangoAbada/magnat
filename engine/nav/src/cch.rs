//! Customizable Contraction Hierarchies — routing na `RoadGraph` (M4a §5.2, WP2).
//!
//! Podział pracy jest tu tym, co odróżnia CCH od zwykłego CH: **struktura** zależy
//! wyłącznie od topologii (kto z kim sąsiaduje), a **wagi** wchodzą dopiero przy
//! kustomizacji. Dzięki temu zmiana korka, zamknięcie mostu na remont (M8) czy inny
//! profil pojazdu kosztuje przebieg kustomizacji, a nie przebudowę hierarchii —
//! rekontrakcja jest potrzebna dopiero, gdy miasto dołoży ulicę.
//!
//! Trzy fazy, trzy koszty:
//!
//! | Faza | Wejście | Kiedy | Wynik |
//! |---|---|---|---|
//! | [`build_order`] | geometria węzłów | przy zmianie `topology_version` | [`ContractionOrder`] |
//! | [`contract`] | graf + kolejność | j.w. | [`ChGraph`] (same łuki, bez wag) |
//! | [`ChGraph::customize`] | wagi krawędzi | przy zmianie `weight_version` | wagi łuków |
//!
//! Kontrakcja jest **grą eliminacyjną bez poszukiwania świadka**: skrót powstaje dla
//! każdej pary sąsiadów eliminowanego węzła, nawet gdy przy bieżących wagach jest
//! bezużyteczny. To celowe — świadek zależy od wag, a struktura ma od nich nie zależeć.
//! Ceną jest więcej łuków niż w CH; zyskiem jest kustomizacja liczona w milisekundach.
//!
//! Determinizm (00 §3.2): kolejność kontrakcji wynika z geometrii i numerów węzłów,
//! remisy rozstrzyga rosnący `NodeId`, a kopce trzymają pary `(dystans, węzeł)`,
//! więc i tam remis rozstrzyga numer. Brak `HashMap`, brak wątków, brak `f64`.

use crate::graph::{Csr, EdgeId, NodeId, RoadGraph};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Próg, poniżej którego dysekcja przestaje dzielić i zwraca wycinek uporządkowany
/// po numerach węzłów. Poniżej ~32 węzłów separator kosztuje więcej, niż oszczędza:
/// blok jest i tak mniejszy niż separator otaczającego poziomu.
const LEAF: usize = 32;

// ---- Kolejność kontrakcji ----------------------------------------------------

/// Kolejność eliminacji węzłów — jedyny wynik fazy niezależnej od wag.
///
/// `order[0]` jest kontraktowany pierwszy (ranga 0) i trafia najniżej w hierarchii;
/// węzły separatorów lądują na końcu, bo przez nie idą trasy dalekie.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ContractionOrder {
    /// Węzły w kolejności kontrakcji.
    pub order: Vec<NodeId>,
    /// `rank[węzeł]` = pozycja w [`ContractionOrder::order`].
    pub rank: Vec<u32>,
    /// `topology_version` grafu, z którego kolejność policzono.
    pub built_for_topology: u32,
}

/// Liczy kolejność kontrakcji metodą **zagnieżdżonej dysekcji geometrycznej**.
///
/// Prawdziwa dysekcja grafowa (METIS, KaHIP) byłaby lepsza o kilkanaście procent
/// łuków, ale wymagałaby zewnętrznej biblioteki. Siatka uliczna jest prawie płaska
/// i prawie regularna, więc cięcie po medianie współrzędnej daje separatory rzędu
/// √n — tyle, ile wynosi dolne ograniczenie dla siatki.
///
/// Wynik nie zależy od żadnej wagi: ten sam graf daje tę samą kolejność zawsze.
#[must_use]
pub fn build_order(g: &RoadGraph) -> ContractionOrder {
    let n = g.node_count();
    let mut pairs: Vec<(u32, u32)> = Vec::with_capacity(g.edge_count() * 2);
    for e in &g.edges {
        if e.from == e.to {
            continue;
        }
        pairs.push((e.from.0, e.to.0));
        pairs.push((e.to.0, e.from.0));
    }
    let adj = Csr::from_pairs(n, &pairs);

    let mut d = Dissector {
        g,
        adj: &adj,
        stamp: vec![0; n],
        gen: 0,
        out: Vec::with_capacity(n),
    };
    d.dissect((0..n as u32).collect());
    let order = d.out;

    let mut rank = vec![0u32; n];
    for (i, &v) in order.iter().enumerate() {
        rank[v as usize] = i as u32;
    }
    ContractionOrder {
        order: order.into_iter().map(NodeId).collect(),
        rank,
        built_for_topology: g.topology_version,
    }
}

/// Stan rekurencyjnej dysekcji. Osobna struktura, bo `stamp` i `out` muszą przeżyć
/// wszystkie poziomy rekursji, a przekazywanie ich pięcioma argumentami nic nie wnosi.
struct Dissector<'a> {
    g: &'a RoadGraph,
    /// Sąsiedztwo **symetryzowane**: każda jezdnia jednokierunkowa liczy się w obie
    /// strony. Separator ma rozcinać geometrię, a nie kierunki ruchu.
    adj: &'a Csr,
    /// Znacznik przynależności do „drugiej połowy" bieżącego poziomu. Znacznik,
    /// a nie tablica bool, żeby nie czyścić n pozycji na każdym z ~2n wywołań.
    stamp: Vec<u32>,
    gen: u32,
    out: Vec<u32>,
}

impl Dissector<'_> {
    fn dissect(&mut self, mut part: Vec<u32>) {
        if part.len() <= LEAF {
            part.sort_unstable();
            self.out.extend_from_slice(&part);
            return;
        }

        // Dłuższy bok prostokąta otaczającego — cięcie w poprzek daje krótszy separator.
        let (mut min_x, mut max_x, mut min_y, mut max_y) = (i32::MAX, i32::MIN, i32::MAX, i32::MIN);
        for &v in &part {
            let p = self.g.nodes[v as usize].pos_cm;
            min_x = min_x.min(p.x);
            max_x = max_x.max(p.x);
            min_y = min_y.min(p.y);
            max_y = max_y.max(p.y);
        }
        let by_x = i64::from(max_x) - i64::from(min_x) >= i64::from(max_y) - i64::from(min_y);

        part.sort_unstable_by_key(|&v| {
            let p = self.g.nodes[v as usize].pos_cm;
            (if by_x { p.x } else { p.y }, v)
        });
        let cut = part.len() / 2;
        let (left, right) = part.split_at(cut);
        if left.is_empty() || right.is_empty() {
            self.fallback(part);
            return;
        }

        // Separator wycinamy z mniejszej połowy: mniej węzłów do sprawdzenia i mniejszy
        // separator, a rozcięcie jest tak samo poprawne (reszta mniejszej połowy nie ma
        // już sąsiada po drugiej stronie).
        let (small, large) = if left.len() <= right.len() {
            (left, right)
        } else {
            (right, left)
        };

        self.gen += 1;
        let gen = self.gen;
        for &v in large {
            self.stamp[v as usize] = gen;
        }
        let mut sep: Vec<u32> = Vec::new();
        let mut rest: Vec<u32> = Vec::with_capacity(small.len());
        for &v in small {
            if self
                .adj
                .range(v as usize)
                .iter()
                .any(|&w| self.stamp[w as usize] == gen)
            {
                sep.push(v);
            } else {
                rest.push(v);
            }
        }

        if rest.is_empty() {
            // Cała mniejsza połowa jest separatorem — dzielenie dalej nie robi postępu.
            self.fallback(part);
            return;
        }

        let large = large.to_vec();
        drop(part);
        self.dissect(rest);
        self.dissect(large);
        sep.sort_unstable();
        self.out.extend_from_slice(&sep);
    }

    /// Wyjście awaryjne, gdy podział nie robi postępu: wycinek idzie w całości,
    /// uporządkowany po numerach. Gorsza hierarchia, ale skończona rekursja.
    fn fallback(&mut self, mut part: Vec<u32>) {
        part.sort_unstable();
        self.out.extend_from_slice(&part);
    }
}

// ---- Struktura hierarchii ----------------------------------------------------

/// Łuk hierarchii — **nieskierowany** odcinek między dwoma węzłami, trzymany przy
/// tym z niższą rangą. Dwie wagi, bo ulica jednokierunkowa jest przejezdna tylko
/// w jedną stronę, a struktura i tak musi trzymać obie: wagi zmienia kustomizacja,
/// struktura się nie rusza.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Arc {
    /// Węzeł o **wyższej** randze.
    head: u32,
    /// Najkrótszy czas z ogona do głowy; `u32::MAX` = brak przejazdu.
    fwd: u32,
    /// Najkrótszy czas z głowy do ogona.
    bwd: u32,
    /// Węzeł pośredni dla `fwd`; `u32::MAX` = łuk jest oryginalną krawędzią.
    mid_fwd: u32,
    /// Węzeł pośredni dla `bwd`.
    mid_bwd: u32,
}

const NONE: u32 = u32::MAX;

/// Hierarchia kontrakcji: łuki „w górę" rangi, wagi wypełnia [`ChGraph::customize`].
///
/// Pola są prywatne, bo niosą niezmiennik, którego z zewnątrz nie da się utrzymać:
/// łuki jednego węzła leżą ciągiem i są posortowane po `head` (stąd wyszukiwanie
/// binarne), a chordalność gwarantuje, że każda para sąsiadów węzła też ma łuk.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ChGraph {
    rank: Vec<u32>,
    order: Vec<u32>,
    /// Węzeł → indeksy łuków wychodzących w górę.
    up: Csr,
    arcs: Vec<Arc>,
    /// `EdgeId.0` krawędzi realizującej `fwd`, albo `u32::MAX` gdy to skrót.
    orig_fwd: Vec<u32>,
    /// `EdgeId.0` krawędzi realizującej `bwd`, albo `u32::MAX` gdy to skrót.
    orig_bwd: Vec<u32>,
    /// **Drzewo eliminacji**: `parent[v]` = sąsiad `v` w górę o najniższej randze,
    /// albo `u32::MAX` dla korzenia. Zależy wyłącznie od struktury, więc powstaje
    /// razem z łukami i przeżywa każdą kustomizację.
    ///
    /// Niesie własność, która zastępuje kopiec w zapytaniu: w grafie chordalnym
    /// **wszyscy** sąsiedzi `v` w górę są jego przodkami w tym drzewie, więc zbiór
    /// węzłów osiągalnych w górę z `v` to dokładnie ścieżka `v → korzeń`. Graf
    /// niespójny ma kilka korzeni — stąd `u32::MAX`, a nie jeden wyróżniony węzeł.
    parent: Vec<u32>,
    built_for_topology: u32,
    customized_for_weights: u32,
}

/// Buduje hierarchię: **gra eliminacyjna** na grafie symetryzowanym, bez świadków.
///
/// Dla każdego węzła `v` w kolejności kontrakcji bierze jego sąsiadów o wyższej randze
/// i domyka ich w klikę. Wynikiem jest graf chordalny — nadgraf wejścia, w którym każda
/// trasa najkrótsza ma reprezentację „w górę, potem w dół".
///
/// # Panics
/// Gdy `order` nie opisuje tego grafu (inna liczba węzłów).
#[must_use]
pub fn contract(g: &RoadGraph, order: &ContractionOrder) -> ChGraph {
    let n = g.node_count();
    assert_eq!(order.order.len(), n, "kolejność kontrakcji z innego grafu");
    let rank = &order.rank;

    // up[v] = sąsiedzi o wyższej randze, posortowani po randze. Sortowanie po randze,
    // a nie po numerze, bo blok dokładany przy eliminacji jest posortowany właśnie tak
    // — scalanie dwóch posortowanych ciągów kosztuje sumę długości, nie iloczyn.
    let mut up: Vec<Vec<u32>> = vec![Vec::new(); n];
    for e in &g.edges {
        let (u, v) = (e.from.0, e.to.0);
        if u == v {
            continue;
        }
        if rank[u as usize] < rank[v as usize] {
            up[u as usize].push(v);
        } else {
            up[v as usize].push(u);
        }
    }
    for l in &mut up {
        l.sort_unstable_by_key(|&x| rank[x as usize]);
        l.dedup();
    }

    let mut buf: Vec<u32> = Vec::new();
    for &v in &order.order {
        let list = std::mem::take(&mut up[v.0 as usize]);
        for (i, &a) in list.iter().enumerate() {
            let block = &list[i + 1..];
            if block.is_empty() {
                break;
            }
            merge_by_rank(&mut up[a as usize], block, rank, &mut buf);
        }
        up[v.0 as usize] = list;
    }

    let mut arcs: Vec<Arc> = Vec::new();
    let mut pairs: Vec<(u32, u32)> = Vec::new();
    let mut parent: Vec<u32> = vec![NONE; n];
    for (v, l) in up.iter_mut().enumerate() {
        // Rodzic w drzewie eliminacji = sąsiad w górę o najniższej randze. Lista jest
        // w tym miejscu jeszcze posortowana po randze (tak ją utrzymuje `merge_by_rank`),
        // więc rodzicem jest jej pierwszy element — nic nie trzeba przeszukiwać.
        parent[v] = l.first().copied().unwrap_or(NONE);
        // Do przechowywania porządek po numerze węzła: pozwala szukać binarnie
        // i scalać listy dwóch węzłów jednym przejściem w kustomizacji.
        l.sort_unstable();
        for &h in l.iter() {
            pairs.push((v as u32, arcs.len() as u32));
            arcs.push(Arc {
                head: h,
                fwd: NONE,
                bwd: NONE,
                mid_fwd: NONE,
                mid_bwd: NONE,
            });
        }
    }
    let m = arcs.len();

    ChGraph {
        rank: rank.clone(),
        order: order.order.iter().map(|v| v.0).collect(),
        up: Csr::from_pairs(n, &pairs),
        arcs,
        orig_fwd: vec![NONE; m],
        orig_bwd: vec![NONE; m],
        parent,
        built_for_topology: order.built_for_topology,
        customized_for_weights: 0,
    }
}

/// Scala `add` (posortowane po randze) w `dst` (też posortowane), bez duplikatów.
fn merge_by_rank(dst: &mut Vec<u32>, add: &[u32], rank: &[u32], buf: &mut Vec<u32>) {
    if dst.is_empty() {
        dst.extend_from_slice(add);
        return;
    }
    buf.clear();
    buf.reserve(dst.len() + add.len());
    let (mut i, mut j) = (0usize, 0usize);
    while i < dst.len() && j < add.len() {
        let ra = rank[dst[i] as usize];
        let rb = rank[add[j] as usize];
        if ra < rb {
            buf.push(dst[i]);
            i += 1;
        } else if ra > rb {
            buf.push(add[j]);
            j += 1;
        } else {
            buf.push(dst[i]);
            i += 1;
            j += 1;
        }
    }
    buf.extend_from_slice(&dst[i..]);
    buf.extend_from_slice(&add[j..]);
    std::mem::swap(dst, buf);
}

impl ChGraph {
    /// Liczba węzłów — ta sama co w grafie źródłowym.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.rank.len()
    }

    /// Liczba łuków hierarchii (nieskierowanych, po jednym na parę).
    #[must_use]
    pub fn arc_count(&self) -> usize {
        self.arcs.len()
    }

    /// `topology_version`, dla której zbudowano strukturę. Różnica = rekontrakcja.
    #[must_use]
    pub fn built_for_topology(&self) -> u32 {
        self.built_for_topology
    }

    /// `weight_version` ostatniej kustomizacji. `0` = wagi jeszcze nie weszły.
    #[must_use]
    pub fn customized_for_weights(&self) -> u32 {
        self.customized_for_weights
    }

    /// Zajętość pamięci w bajtach — tylko tablice własne, bez grafu źródłowego.
    /// Istnieje dla bramki „≤ 30 MB na metropolię" z kryteriów M4a.
    #[must_use]
    pub fn memory_bytes(&self) -> usize {
        self.arcs.len() * size_of::<Arc>()
            + self.orig_fwd.len() * 4
            + self.orig_bwd.len() * 4
            + self.up.items().len() * 4
            + (self.up.keys() + 1) * 4
            + self.rank.len() * 4
            + self.order.len() * 4
            + self.parent.len() * 4
    }

    /// Początek i długość bloku łuków węzła w [`ChGraph::arcs`].
    fn arc_range(&self, v: u32) -> (usize, usize) {
        let r = self.up.range(v as usize);
        if r.is_empty() {
            (0, 0)
        } else {
            (r[0] as usize, r.len())
        }
    }

    /// Indeks łuku `tail → head` (`rank[tail] < rank[head]`), albo `None`.
    fn find_arc(&self, tail: u32, head: u32) -> Option<usize> {
        let (a0, len) = self.arc_range(tail);
        let slice = &self.arcs[a0..a0 + len];
        slice
            .binary_search_by_key(&head, |a| a.head)
            .ok()
            .map(|k| a0 + k)
    }

    // ---- Kustomizacja --------------------------------------------------------

    /// Wpisuje wagi w hierarchię. `weights[e]` indeksowane `EdgeId.0`, w setnych
    /// sekundy; `u32::MAX` oznacza krawędź **wyłączoną** z profilu (zamknięty most,
    /// zakaz dla ciężarówki, ulica nieprzejezdna dla warstwy).
    ///
    /// Kustomizacja bazowa (trójkątowa): węzły w rosnącej randze, dla każdej pary
    /// łuków w górę relaksacja łuku między ich głowami. Jeden wątek, jeden przebieg,
    /// bez świadków — deterministyczna z definicji.
    ///
    /// # Panics
    /// Gdy `weights` jest krótsze niż tablica krawędzi grafu.
    pub fn customize(&mut self, g: &RoadGraph, weights: &[u32], weight_version: u32) {
        assert!(
            weights.len() >= g.edge_count(),
            "wektor wag krótszy niż tablica krawędzi"
        );
        for a in &mut self.arcs {
            a.fwd = NONE;
            a.bwd = NONE;
            a.mid_fwd = NONE;
            a.mid_bwd = NONE;
        }
        self.orig_fwd.fill(NONE);
        self.orig_bwd.fill(NONE);

        // Krok 1: oryginalne krawędzie. Równoległe jezdnie tej samej pary węzłów
        // zlewają się w jeden łuk — zostaje najszybsza, bo tylko ona wchodzi do trasy.
        for (i, e) in g.edges.iter().enumerate() {
            let w = weights[i];
            if w == NONE || e.from == e.to {
                continue;
            }
            let (u, v) = (e.from.0, e.to.0);
            let forward = self.rank[u as usize] < self.rank[v as usize];
            let (tail, head) = if forward { (u, v) } else { (v, u) };
            let Some(k) = self.find_arc(tail, head) else {
                continue;
            };
            if forward {
                if w < self.arcs[k].fwd {
                    self.arcs[k].fwd = w;
                    self.arcs[k].mid_fwd = NONE;
                    self.orig_fwd[k] = i as u32;
                }
            } else if w < self.arcs[k].bwd {
                self.arcs[k].bwd = w;
                self.arcs[k].mid_bwd = NONE;
                self.orig_bwd[k] = i as u32;
            }
        }

        // Krok 2: trójkąty niższe. Gdy eliminujemy `v`, każda para jego sąsiadów w górę
        // `(a, b)` ma już łuk (chordalność) — wystarczy scalić listę `v` z listą `a`
        // po numerach głów, a trafienia są dokładnie parami do zrelaksowania. Lista `a`
        // zawiera wyłącznie węzły o randze wyższej niż `a`, więc kierunek pary wychodzi
        // z samego trafienia i nie trzeba porównywać rang.
        let mut vlist: Vec<Arc> = Vec::new();
        for oi in 0..self.order.len() {
            let v = self.order[oi];
            let (v0, vlen) = self.arc_range(v);
            if vlen < 2 {
                continue;
            }
            vlist.clear();
            vlist.extend_from_slice(&self.arcs[v0..v0 + vlen]);

            for va in &vlist {
                let a = va.head;
                let (a0, alen) = self.arc_range(a);
                if alen == 0 {
                    continue;
                }
                let (mut j, mut k) = (0usize, a0);
                while j < vlist.len() && k < a0 + alen {
                    let hv = vlist[j].head;
                    let ha = self.arcs[k].head;
                    if hv < ha {
                        j += 1;
                    } else if hv > ha {
                        k += 1;
                    } else {
                        // Trasa a → v → b i b → v → a przez eliminowany węzeł.
                        if va.bwd != NONE && vlist[j].fwd != NONE {
                            let c = va.bwd.saturating_add(vlist[j].fwd);
                            if c < self.arcs[k].fwd {
                                self.arcs[k].fwd = c;
                                self.arcs[k].mid_fwd = v;
                                self.orig_fwd[k] = NONE;
                            }
                        }
                        if vlist[j].bwd != NONE && va.fwd != NONE {
                            let c = vlist[j].bwd.saturating_add(va.fwd);
                            if c < self.arcs[k].bwd {
                                self.arcs[k].bwd = c;
                                self.arcs[k].mid_bwd = v;
                                self.orig_bwd[k] = NONE;
                            }
                        }
                        j += 1;
                        k += 1;
                    }
                }
            }
        }

        self.customized_for_weights = weight_version;
    }

    // ---- Zapytania -----------------------------------------------------------

    /// Najkrótszy czas przejazdu w setnych sekundy, albo `None` gdy trasy nie ma.
    pub fn query(&self, s: NodeId, t: NodeId, scratch: &mut ChScratch) -> Option<u32> {
        self.search(s, t, scratch).map(|(cost, _)| cost)
    }

    /// To samo zapytanie liczone **kopcem** — dwukierunkowa Dijkstra w górę rangi.
    ///
    /// Zostaje jako referencja: zapytanie po drzewie eliminacji jest szybsze, ale
    /// opiera się na własności chordalności, której typ nie pilnuje. Test różnicowy
    /// porównujący obie ścieżki jest jedynym miejscem, gdzie złamanie tej własności
    /// wyszłoby na jaw inaczej niż jako cicho gorsza trasa.
    pub fn query_dijkstra(&self, s: NodeId, t: NodeId, scratch: &mut ChScratch) -> Option<u32> {
        self.search_heap(s, t, scratch).map(|(cost, _)| cost)
    }

    /// Jak [`ChGraph::query`], ale rozpakowuje trasę do ciągu oryginalnych krawędzi
    /// w kolejności przejazdu od `s` do `t`.
    pub fn query_path(
        &self,
        s: NodeId,
        t: NodeId,
        scratch: &mut ChScratch,
    ) -> Option<(u32, Vec<EdgeId>)> {
        let (cost, meet) = self.search(s, t, scratch)?;

        // Odcinki hierarchii w kolejności przejazdu: najpierw w górę z `s` do punktu
        // spotkania, potem w dół do `t`. Każdy odcinek to jeden łuk — skrót albo
        // krawędź oryginalna.
        let mut seg: Vec<(u32, u32)> = Vec::new();
        let mut v = meet;
        while scratch.pf_arc[v as usize] != NONE {
            let u = scratch.pf_node[v as usize];
            seg.push((u, v));
            v = u;
        }
        seg.reverse();
        let mut v = meet;
        while scratch.pb_arc[v as usize] != NONE {
            let u = scratch.pb_node[v as usize];
            seg.push((v, u));
            v = u;
        }

        let mut out: Vec<EdgeId> = Vec::new();
        let mut stack: Vec<(u32, u32)> = seg;
        stack.reverse();
        while let Some((x, y)) = stack.pop() {
            let forward = self.rank[x as usize] < self.rank[y as usize];
            let (tail, head) = if forward { (x, y) } else { (y, x) };
            let k = self.find_arc(tail, head)?;
            let mid = if forward {
                self.arcs[k].mid_fwd
            } else {
                self.arcs[k].mid_bwd
            };
            if mid == NONE {
                let o = if forward {
                    self.orig_fwd[k]
                } else {
                    self.orig_bwd[k]
                };
                if o == NONE {
                    return None;
                }
                out.push(EdgeId(o));
            } else {
                stack.push((mid, y));
                stack.push((x, mid));
            }
        }
        Some((cost, out))
    }

    /// Wyszukiwanie po **drzewie eliminacji** — domyślne dla [`ChGraph::query`].
    ///
    /// Kopiec jest tu zbędny, bo zbiór węzłów osiągalnych w górę z `s` to ścieżka
    /// `s → korzeń`, a ranga rośnie wzdłuż niej monotonicznie. Idąc tą ścieżką
    /// w kolejności rang odwiedzamy każdego przodka **raz** i zawsze z ustalonym już
    /// dystansem: każda relaksacja do `v` przychodzi z węzła o niższej randze, czyli
    /// z odcinka ścieżki już przebytego.
    ///
    /// Dwie ścieżki (`s` i `t`) idą na przemian — zawsze ta, której bieżący węzeł ma
    /// **niższą** rangę. To jest warunek, dzięki któremu w chwili odwiedzenia `v`
    /// dystanse z obu stron są już ostateczne, więc spotkanie wystarczy sprawdzić
    /// w miejscu, bez odkładania na później.
    ///
    /// Zwraca koszt i węzeł spotkania; tablice rodziców w `sc` opisują wtedy obie
    /// połówki trasy — dokładnie tak samo jak po [`ChGraph::search_heap`].
    fn search(&self, s: NodeId, t: NodeId, sc: &mut ChScratch) -> Option<(u32, u32)> {
        let n = self.node_count();
        if s.0 as usize >= n || t.0 as usize >= n {
            return None;
        }
        sc.prepare(n);
        let g = sc.gen;
        if s == t {
            sc.sf[s.0 as usize] = g;
            sc.sb[s.0 as usize] = g;
            sc.pf_arc[s.0 as usize] = NONE;
            sc.pb_arc[s.0 as usize] = NONE;
            return Some((0, s.0));
        }

        sc.sf[s.0 as usize] = g;
        sc.df[s.0 as usize] = 0;
        sc.pf_arc[s.0 as usize] = NONE;
        sc.sb[t.0 as usize] = g;
        sc.db[t.0 as usize] = 0;
        sc.pb_arc[t.0 as usize] = NONE;

        let mut best = NONE;
        let mut meet = NONE;
        let (mut a, mut b) = (s.0, t.0);
        // Ranga korzenia jest `NONE`, czyli większa od każdej prawdziwej — ścieżka,
        // która doszła do końca, przestaje wygrywać porównanie i druga idzie sama.
        while a != NONE || b != NONE {
            let ra = if a == NONE {
                NONE
            } else {
                self.rank[a as usize]
            };
            let rb = if b == NONE {
                NONE
            } else {
                self.rank[b as usize]
            };
            // Rangi są unikalne, więc remis znaczy `a == b` — wtedy najpierw przód.
            let (v, forward) = if ra <= rb { (a, true) } else { (b, false) };
            let vi = v as usize;

            if forward {
                a = self.parent[vi];
            } else {
                b = self.parent[vi];
            }

            let stamp = if forward { sc.sf[vi] } else { sc.sb[vi] };
            if stamp != g {
                // Przodek nieosiągalny z tej strony; wyżej może być osiągalny, bo łuk
                // z `s` potrafi przeskoczyć węzeł pośredni. Idziemy dalej.
                continue;
            }
            let d = if forward { sc.df[vi] } else { sc.db[vi] };
            if (if forward { sc.sb[vi] } else { sc.sf[vi] }) == g {
                let other = if forward { sc.db[vi] } else { sc.df[vi] };
                let sum = d.saturating_add(other);
                if sum < best {
                    best = sum;
                    meet = v;
                }
            }
            if d >= best {
                // Każda trasa przez `v` kosztuje co najmniej `d` — relaksacja nic nie da.
                continue;
            }

            let (a0, alen) = self.arc_range(v);
            for k in a0..a0 + alen {
                let arc = self.arcs[k];
                let w = if forward { arc.fwd } else { arc.bwd };
                if w == NONE {
                    continue;
                }
                let nd = d.saturating_add(w);
                let h = arc.head as usize;
                if forward {
                    if sc.sf[h] != g || nd < sc.df[h] {
                        sc.sf[h] = g;
                        sc.df[h] = nd;
                        sc.pf_arc[h] = k as u32;
                        sc.pf_node[h] = v;
                    }
                } else if sc.sb[h] != g || nd < sc.db[h] {
                    sc.sb[h] = g;
                    sc.db[h] = nd;
                    sc.pb_arc[h] = k as u32;
                    sc.pb_node[h] = v;
                }
            }
        }

        if meet == NONE {
            None
        } else {
            Some((best, meet))
        }
    }

    /// Dwukierunkowe wyszukiwanie w górę rangi **kopcem**. Zwraca koszt i węzeł
    /// spotkania; tablice rodziców w `sc` opisują wtedy obie połówki trasy.
    fn search_heap(&self, s: NodeId, t: NodeId, sc: &mut ChScratch) -> Option<(u32, u32)> {
        let n = self.node_count();
        if s.0 as usize >= n || t.0 as usize >= n {
            return None;
        }
        sc.prepare(n);
        let g = sc.gen;
        if s == t {
            sc.sf[s.0 as usize] = g;
            sc.sb[s.0 as usize] = g;
            sc.pf_arc[s.0 as usize] = NONE;
            sc.pb_arc[s.0 as usize] = NONE;
            return Some((0, s.0));
        }

        sc.sf[s.0 as usize] = g;
        sc.df[s.0 as usize] = 0;
        sc.pf_arc[s.0 as usize] = NONE;
        sc.sb[t.0 as usize] = g;
        sc.db[t.0 as usize] = 0;
        sc.pb_arc[t.0 as usize] = NONE;
        sc.hf.push(Reverse((0, s.0)));
        sc.hb.push(Reverse((0, t.0)));

        let mut best = NONE;
        let mut meet = NONE;
        loop {
            let kf = sc.hf.peek().map(|&Reverse((d, _))| d);
            let kb = sc.hb.peek().map(|&Reverse((d, _))| d);
            let (forward, key) = match (kf, kb) {
                (None, None) => break,
                (Some(a), None) => (true, a),
                (None, Some(b)) => (false, b),
                (Some(a), Some(b)) => {
                    if a <= b {
                        (true, a)
                    } else {
                        (false, b)
                    }
                }
            };
            // Oba klucze są ≥ `key`, więc żadna nierozwinięta trasa nie będzie krótsza.
            if key >= best {
                break;
            }
            let popped = if forward { sc.hf.pop() } else { sc.hb.pop() };
            let Some(Reverse((d, v))) = popped else {
                break;
            };
            let vi = v as usize;
            let (a0, alen) = self.arc_range(v);

            if forward {
                if d > sc.df[vi] {
                    continue;
                }
                if sc.sb[vi] == g {
                    let sum = d.saturating_add(sc.db[vi]);
                    if sum < best {
                        best = sum;
                        meet = v;
                    }
                }
                for k in a0..a0 + alen {
                    let arc = self.arcs[k];
                    if arc.fwd == NONE {
                        continue;
                    }
                    let nd = d.saturating_add(arc.fwd);
                    let h = arc.head as usize;
                    if sc.sf[h] != g || nd < sc.df[h] {
                        sc.sf[h] = g;
                        sc.df[h] = nd;
                        sc.pf_arc[h] = k as u32;
                        sc.pf_node[h] = v;
                        sc.hf.push(Reverse((nd, arc.head)));
                    }
                }
            } else {
                if d > sc.db[vi] {
                    continue;
                }
                if sc.sf[vi] == g {
                    let sum = d.saturating_add(sc.df[vi]);
                    if sum < best {
                        best = sum;
                        meet = v;
                    }
                }
                for k in a0..a0 + alen {
                    let arc = self.arcs[k];
                    if arc.bwd == NONE {
                        continue;
                    }
                    let nd = d.saturating_add(arc.bwd);
                    let h = arc.head as usize;
                    if sc.sb[h] != g || nd < sc.db[h] {
                        sc.sb[h] = g;
                        sc.db[h] = nd;
                        sc.pb_arc[h] = k as u32;
                        sc.pb_node[h] = v;
                        sc.hb.push(Reverse((nd, arc.head)));
                    }
                }
            }
        }

        if meet == NONE {
            None
        } else {
            Some((best, meet))
        }
    }
}

/// Bufory wyszukiwania, żywe między zapytaniami.
///
/// Przy 200 tys. węzłów samo wyzerowanie tablic dystansów kosztowałoby więcej niż
/// całe zapytanie, więc ważność wpisu niesie znacznik `gen`, a nie czyszczenie.
#[derive(Default)]
pub struct ChScratch {
    gen: u32,
    df: Vec<u32>,
    sf: Vec<u32>,
    pf_arc: Vec<u32>,
    pf_node: Vec<u32>,
    db: Vec<u32>,
    sb: Vec<u32>,
    pb_arc: Vec<u32>,
    pb_node: Vec<u32>,
    hf: BinaryHeap<Reverse<(u32, u32)>>,
    hb: BinaryHeap<Reverse<(u32, u32)>>,
}

impl ChScratch {
    /// Pusty zestaw buforów; rozmiar dobierze się przy pierwszym zapytaniu.
    #[must_use]
    pub fn new() -> ChScratch {
        ChScratch::default()
    }

    fn prepare(&mut self, n: usize) {
        if self.df.len() != n {
            self.df = vec![0; n];
            self.sf = vec![0; n];
            self.pf_arc = vec![0; n];
            self.pf_node = vec![0; n];
            self.db = vec![0; n];
            self.sb = vec![0; n];
            self.pb_arc = vec![0; n];
            self.pb_node = vec![0; n];
            self.gen = 0;
        }
        self.gen = self.gen.wrapping_add(1);
        if self.gen == 0 {
            // Przewinięcie licznika po 4 mld zapytań — jedyny moment, gdy trzeba zerować.
            self.sf.fill(0);
            self.sb.fill(0);
            self.gen = 1;
        }
        self.hf.clear();
        self.hb.clear();
    }
}

// ---- Referencja --------------------------------------------------------------

/// Dijkstra na oryginalnym `RoadGraph` — referencja dla testu równoważności
/// i awaryjne wyszukiwanie w trakcie rekontrakcji.
///
/// Krawędź o wadze `u32::MAX` jest wyłączona i nie jest relaksowana.
///
/// # Panics
/// Gdy `weights` jest krótsze niż tablica krawędzi grafu.
#[must_use]
pub fn dijkstra(
    g: &RoadGraph,
    weights: &[u32],
    s: NodeId,
    t: NodeId,
) -> Option<(u32, Vec<EdgeId>)> {
    assert!(
        weights.len() >= g.edge_count(),
        "wektor wag krótszy niż tablica krawędzi"
    );
    let n = g.node_count();
    if s.0 as usize >= n || t.0 as usize >= n {
        return None;
    }
    let mut dist = vec![NONE; n];
    let mut parent = vec![NONE; n];
    let mut heap: BinaryHeap<Reverse<(u32, u32)>> = BinaryHeap::new();
    dist[s.0 as usize] = 0;
    heap.push(Reverse((0, s.0)));

    while let Some(Reverse((d, v))) = heap.pop() {
        if d > dist[v as usize] {
            continue;
        }
        if v == t.0 {
            break;
        }
        for &e in g.out(NodeId(v)) {
            let w = weights[e as usize];
            if w == NONE {
                continue;
            }
            let h = g.edges[e as usize].to.0 as usize;
            let nd = d.saturating_add(w);
            if nd < dist[h] {
                dist[h] = nd;
                parent[h] = e;
                heap.push(Reverse((nd, h as u32)));
            }
        }
    }

    if dist[t.0 as usize] == NONE {
        return None;
    }
    let mut path: Vec<EdgeId> = Vec::new();
    let mut v = t.0 as usize;
    while parent[v] != NONE {
        let e = parent[v];
        path.push(EdgeId(e));
        v = g.edges[e as usize].from.0 as usize;
    }
    path.reverse();
    Some((dist[t.0 as usize], path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{
        synthetic_grid, synthetic_road_network, EdgeSpec, GeomRef, Modality, RoadGraphBuilder,
    };
    use magnat_core::{DistrictId, IVec2, Mass, RoadClass};

    /// Deterministyczny generator par (s, t) — LCG, bo `rand` nie wchodzi do symulacji.
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> u32 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 33) as u32
        }
    }

    fn zbuduj(g: &RoadGraph, weights: &[u32]) -> ChGraph {
        let order = build_order(g);
        let mut ch = contract(g, &order);
        ch.customize(g, weights, 1);
        ch
    }

    #[test]
    fn kolejnosc_kontrakcji_jest_permutacja() {
        let g = synthetic_grid(20, 20, 10_000);
        let o = build_order(&g);
        assert_eq!(o.order.len(), g.node_count());
        assert_eq!(o.rank.len(), g.node_count());
        let mut seen = vec![false; g.node_count()];
        for v in &o.order {
            assert!(!seen[v.0 as usize], "węzeł {} dwa razy", v.0);
            seen[v.0 as usize] = true;
        }
        assert!(seen.iter().all(|&s| s));
        for (i, v) in o.order.iter().enumerate() {
            assert_eq!(o.rank[v.0 as usize], i as u32);
        }
        assert_eq!(o.built_for_topology, g.topology_version);
    }

    #[test]
    fn kolejnosc_nie_zalezy_od_wag() {
        let g = synthetic_grid(23, 17, 10_000);
        let a = build_order(&g);
        let b = build_order(&g);
        assert_eq!(a.order, b.order);
        assert_eq!(a.rank, b.rank);
    }

    #[test]
    fn cch_zgadza_sie_z_dijkstra() {
        let g = synthetic_grid(40, 40, 10_000);
        let w = g.free_flow_weights();
        let ch = zbuduj(&g, &w);
        let mut sc = ChScratch::new();
        let n = g.node_count() as u32;
        let mut rng = Lcg(0x1234_5678_9abc_def0);
        for _ in 0..1000 {
            let s = NodeId(rng.next() % n);
            let t = NodeId(rng.next() % n);
            let got = ch.query(s, t, &mut sc);
            let want = dijkstra(&g, &w, s, t).map(|(c, _)| c);
            assert_eq!(got, want, "para ({}, {})", s.0, t.0);
        }
    }

    #[test]
    fn rozpakowana_trasa_ma_koszt_zapytania() {
        let g = synthetic_grid(30, 30, 10_000);
        let w = g.free_flow_weights();
        let ch = zbuduj(&g, &w);
        let mut sc = ChScratch::new();
        let n = g.node_count() as u32;
        let mut rng = Lcg(0xdead_beef_cafe_0001);
        for _ in 0..200 {
            let s = NodeId(rng.next() % n);
            let t = NodeId(rng.next() % n);
            let Some((cost, path)) = ch.query_path(s, t, &mut sc) else {
                assert_eq!(ch.query(s, t, &mut sc), None);
                continue;
            };
            let suma: u64 = path.iter().map(|e| u64::from(w[e.0 as usize])).sum();
            assert_eq!(suma, u64::from(cost), "koszt trasy ({}, {})", s.0, t.0);
            // Ciągłość: koniec krawędzi jest początkiem następnej, start to `s`, meta `t`.
            let mut v = s;
            for e in &path {
                assert_eq!(g.edge(*e).from, v, "przerwa w trasie ({}, {})", s.0, t.0);
                v = g.edge(*e).to;
            }
            assert_eq!(v, t);
        }
    }

    #[test]
    fn krawedz_wylaczona_nie_wchodzi_do_trasy() {
        let g = synthetic_grid(24, 24, 10_000);
        let mut w = g.free_flow_weights();
        let mut wylaczone: Vec<u32> = Vec::new();
        for (i, x) in w.iter_mut().enumerate() {
            if i % 7 == 3 {
                *x = u32::MAX;
                wylaczone.push(i as u32);
            }
        }
        let ch = zbuduj(&g, &w);
        let mut sc = ChScratch::new();
        let n = g.node_count() as u32;
        let mut rng = Lcg(0x0bad_f00d_0000_0007);
        for _ in 0..200 {
            let s = NodeId(rng.next() % n);
            let t = NodeId(rng.next() % n);
            let got = ch.query_path(s, t, &mut sc);
            let want = dijkstra(&g, &w, s, t);
            assert_eq!(
                got.as_ref().map(|(c, _)| *c),
                want.as_ref().map(|(c, _)| *c),
                "para ({}, {})",
                s.0,
                t.0
            );
            if let Some((_, path)) = got {
                for e in &path {
                    assert!(
                        !wylaczone.contains(&e.0),
                        "trasa ({}, {}) używa wyłączonej krawędzi {}",
                        s.0,
                        t.0,
                        e.0
                    );
                }
            }
        }
    }

    #[test]
    fn brak_trasy_daje_none() {
        // Dwie rozłączne składowe: 0–1 i 2–3.
        let mut b = RoadGraphBuilder::new(Modality::Road).with_turns(false);
        for i in 0..4 {
            b.add_node(IVec2::new(i * 10_000, 0), 0);
        }
        let spec = |from, to| EdgeSpec {
            from,
            to,
            geometry_ref: GeomRef(0),
            length_cm: 10_000,
            lanes: 1,
            class: RoadClass::Local,
            speed_limit_dkmh: 300,
            max_mass: Mass::ZERO,
            grade_permille: 0,
            bridge: None,
            curb_parking: 0,
            district: DistrictId(0),
        };
        b.add_edge_pair(spec(NodeId(0), NodeId(1)));
        b.add_edge_pair(spec(NodeId(2), NodeId(3)));
        let g = b.finish();
        let w = g.free_flow_weights();
        let ch = zbuduj(&g, &w);
        let mut sc = ChScratch::new();
        assert_eq!(ch.query(NodeId(0), NodeId(3), &mut sc), None);
        assert_eq!(ch.query_path(NodeId(0), NodeId(3), &mut sc), None);
        assert_eq!(ch.query(NodeId(0), NodeId(1), &mut sc), Some(1200));
        assert_eq!(ch.query(NodeId(2), NodeId(2), &mut sc), Some(0));
        assert_eq!(dijkstra(&g, &w, NodeId(1), NodeId(2)), None);
    }

    /// Zapytanie po drzewie eliminacji ma dawać **ten sam koszt** co kopiec i co
    /// Dijkstra na oryginale — na siatce jednorodnej i na sieci ulic z węzłami
    /// stopnia 2, gdzie drzewo jest głębokie i ścieżki przodków najdłuższe.
    #[test]
    fn drzewo_eliminacji_zgadza_sie_z_dijkstra() {
        let instancje = [
            synthetic_grid(40, 40, 10_000),
            synthetic_road_network(20, 20, 5, 10_000),
        ];
        for (i, g) in instancje.iter().enumerate() {
            let w = g.free_flow_weights();
            let ch = zbuduj(g, &w);
            let mut sc = ChScratch::new();
            let n = g.node_count() as u32;
            let mut rng = Lcg(0x0e11_0000_0000_0001 ^ i as u64);
            for _ in 0..1000 {
                let s = NodeId(rng.next() % n);
                let t = NodeId(rng.next() % n);
                let drzewo = ch.query(s, t, &mut sc);
                let kopiec = ch.query_dijkstra(s, t, &mut sc);
                let odniesienie = dijkstra(g, &w, s, t).map(|(c, _)| c);
                assert_eq!(drzewo, kopiec, "instancja {i}, para ({}, {})", s.0, t.0);
                assert_eq!(
                    drzewo, odniesienie,
                    "instancja {i}, para ({}, {})",
                    s.0, t.0
                );
            }
        }
    }

    #[test]
    fn rozpakowana_trasa_z_drzewa_ma_koszt_zapytania() {
        let g = synthetic_road_network(20, 20, 5, 10_000);
        let w = g.free_flow_weights();
        let ch = zbuduj(&g, &w);
        let mut sc = ChScratch::new();
        let n = g.node_count() as u32;
        let mut rng = Lcg(0x0e11_0000_0000_0002);
        for _ in 0..200 {
            let s = NodeId(rng.next() % n);
            let t = NodeId(rng.next() % n);
            let Some((cost, path)) = ch.query_path(s, t, &mut sc) else {
                assert_eq!(ch.query(s, t, &mut sc), None);
                continue;
            };
            let suma: u64 = path.iter().map(|e| u64::from(w[e.0 as usize])).sum();
            assert_eq!(suma, u64::from(cost), "koszt trasy ({}, {})", s.0, t.0);
            let mut v = s;
            for e in &path {
                assert_eq!(g.edge(*e).from, v, "przerwa w trasie ({}, {})", s.0, t.0);
                v = g.edge(*e).to;
            }
            assert_eq!(v, t);
        }
    }

    #[test]
    fn drzewo_eliminacji_respektuje_wylaczone_krawedzie() {
        let g = synthetic_road_network(20, 20, 5, 10_000);
        let mut w = g.free_flow_weights();
        for (i, x) in w.iter_mut().enumerate() {
            if i % 5 == 2 {
                *x = u32::MAX;
            }
        }
        let ch = zbuduj(&g, &w);
        let mut sc = ChScratch::new();
        let n = g.node_count() as u32;
        let mut rng = Lcg(0x0e11_0000_0000_0003);
        let mut bez_trasy = 0u32;
        for _ in 0..500 {
            let s = NodeId(rng.next() % n);
            let t = NodeId(rng.next() % n);
            let got = ch.query(s, t, &mut sc);
            let want = dijkstra(&g, &w, s, t).map(|(c, _)| c);
            assert_eq!(got, want, "para ({}, {})", s.0, t.0);
            if want.is_none() {
                bez_trasy += 1;
            }
        }
        // Wyłączenie co piątej krawędzi ma naprawdę rozspójnić graf — inaczej test
        // sprawdzałby tylko, że `u32::MAX` nie psuje arytmetyki.
        assert!(bez_trasy > 0, "żadna para nie została odcięta");
    }

    #[test]
    fn rodzic_w_drzewie_ma_wyzsza_range() {
        let g = synthetic_road_network(20, 20, 5, 10_000);
        let w = g.free_flow_weights();
        let ch = zbuduj(&g, &w);
        let mut korzenie = 0u32;
        for v in 0..ch.node_count() as u32 {
            let (a0, alen) = ch.arc_range(v);
            let p = ch.parent[v as usize];
            if alen == 0 {
                assert_eq!(p, NONE, "węzeł {v} bez łuków w górę nie jest korzeniem");
                korzenie += 1;
                continue;
            }
            assert_ne!(p, NONE, "węzeł {v} ma łuki w górę, ale nie ma rodzica");
            assert!(
                ch.rank[p as usize] > ch.rank[v as usize],
                "rodzic {p} węzła {v} nie ma wyższej rangi"
            );
            // Rodzic to sąsiad o **najniższej** randze, a każdy sąsiad w górę
            // jest przodkiem — to jest własność, na której stoi zapytanie.
            for k in a0..a0 + alen {
                let h = ch.arcs[k].head;
                assert!(ch.rank[h as usize] > ch.rank[v as usize]);
                assert!(ch.rank[h as usize] >= ch.rank[p as usize]);
            }
        }
        assert!(korzenie > 0, "drzewo eliminacji bez korzenia");
        // Ścieżka przodków musi się kończyć — rosnąca ranga to gwarantuje, ale
        // niezmiennik wolno sprawdzić wprost, bo zapytanie idzie po niej pętlą.
        let mut v = 0u32;
        let mut krokow = 0usize;
        while v != NONE {
            v = ch.parent[v as usize];
            krokow += 1;
            assert!(krokow <= ch.node_count(), "cykl w drzewie eliminacji");
        }
    }

    #[test]
    fn kustomizacja_jest_idempotentna() {
        let g = synthetic_grid(18, 22, 10_000);
        let w = g.free_flow_weights();
        let order = build_order(&g);
        let mut ch = contract(&g, &order);
        ch.customize(&g, &w, 1);
        let arcs = ch.arcs.clone();
        let of = ch.orig_fwd.clone();
        let ob = ch.orig_bwd.clone();
        ch.customize(&g, &w, 2);
        assert_eq!(ch.arcs, arcs);
        assert_eq!(ch.orig_fwd, of);
        assert_eq!(ch.orig_bwd, ob);
        assert_eq!(ch.customized_for_weights(), 2);
    }

    /// Pomiar, nie bramka. Wyłączony domyślnie, bo na siatce 450×450 kontrakcja
    /// i kustomizacja w profilu `dev` zajmują ~55 s — to jest ten przypadek, o którym
    /// mówi reguła „nie odhaczaj testu, który blokuje pętlę zwrotną".
    /// Uruchomienie: `cargo test -p magnat-nav --release -- --ignored --nocapture pomiar_p95`.
    #[test]
    #[ignore = "N1.2: pomiar wydajności: ~5 s w release, ~55 s w debug"]
    fn pomiar_p95_zapytania_cch() {
        use std::time::Instant;

        let g = synthetic_grid(450, 450, 10_000);
        let w = g.free_flow_weights();

        let t0 = Instant::now();
        let order = build_order(&g);
        let t_order = t0.elapsed();

        let t0 = Instant::now();
        let mut ch = contract(&g, &order);
        let t_contract = t0.elapsed();

        let t0 = Instant::now();
        ch.customize(&g, &w, 1);
        let t_custom = t0.elapsed();

        let mut sc = ChScratch::new();
        let n = g.node_count() as u32;
        let mut rng = Lcg(0x5eed_0000_0000_2024);
        // Rozgrzewka: pierwsze zapytanie alokuje bufory scratcha.
        let _ = ch.query(NodeId(0), NodeId(n - 1), &mut sc);

        let mut czasy: Vec<u128> = Vec::with_capacity(2000);
        for _ in 0..2000 {
            let s = NodeId(rng.next() % n);
            let t = NodeId(rng.next() % n);
            let t0 = Instant::now();
            let r = ch.query(s, t, &mut sc);
            czasy.push(t0.elapsed().as_micros());
            assert!(r.is_some(), "siatka jest spójna, para ({}, {})", s.0, t.0);
        }
        czasy.sort_unstable();
        let mediana = czasy[czasy.len() / 2];
        let p95 = czasy[czasy.len() * 95 / 100];

        // Ta sama pula par kopcem — liczba, wobec której drzewo eliminacji ma sens.
        let mut rng = Lcg(0x5eed_0000_0000_2024);
        let mut kopiec: Vec<u128> = Vec::with_capacity(2000);
        for _ in 0..2000 {
            let s = NodeId(rng.next() % n);
            let t = NodeId(rng.next() % n);
            let t0 = Instant::now();
            let r = ch.query_dijkstra(s, t, &mut sc);
            kopiec.push(t0.elapsed().as_micros());
            assert!(r.is_some());
        }
        kopiec.sort_unstable();

        println!(
            "CCH na siatce 450x450 ({n} węzłów, {} krawędzi):",
            g.edge_count()
        );
        println!("  kolejność:    {t_order:?}");
        println!("  kontrakcja:   {t_contract:?}");
        println!("  kustomizacja: {t_custom:?}");
        println!("  łuki:         {}", ch.arc_count());
        println!(
            "  pamięć:       {} B ({} MB)",
            ch.memory_bytes(),
            ch.memory_bytes() / 1_048_576
        );
        println!("  zapytanie:    mediana {mediana} µs, p95 {p95} µs  (drzewo eliminacji)");
        println!(
            "  kopcem:       mediana {} µs, p95 {} µs  (referencja)",
            kopiec[kopiec.len() / 2],
            kopiec[kopiec.len() * 95 / 100]
        );

        assert!(
            p95 < 5000,
            "p95 zapytania {p95} µs — regresja rzędu wielkości"
        );
    }
}
