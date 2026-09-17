//! Topologia sieci przesyłowej: wyspy, las rozpinający, kolejność sumowania
//! i mosty (M8b §5.4 krok 1).
//!
//! Przebudowuje się **wyłącznie przy zmianie topologii** — nowe przyłącze, awaria,
//! powrót krawędzi po naprawie — a nie 1440 razy na dobę. To jest cała różnica
//! między budżetem 0,3 ms a budżetem 30 ms: bilans wyspy i przepływy są jednym
//! przejściem po gotowej tablicy, a przygotowanie tej tablicy jest przejściem
//! po grafie.
//!
//! # Dlaczego most, a nie superwęzeł
//!
//! §5.4 zapowiadał kolapsowanie składowych 2-spójnych do superwęzłów: przepływ
//! liczy się na drzewie, a pętla na drzewie nie leży, więc trzeba ją ściągnąć
//! do jednego punktu. Kontrakcja nie jest jednak do niczego potrzebna, bo pytanie,
//! na które ona odpowiada, brzmi „czy tej krawędzi wolno wierzyć w przepływ
//! z poddrzewa" — a to jest dokładnie definicja mostu. Krawędź, która **jest**
//! mostem, niesie sumę popytu swojego poddrzewa co do wata, bo jej usunięcie
//! odcina dokładnie to poddrzewo. Krawędź, która mostem **nie jest**, leży
//! w pierścieniu i ma obok siebie drogę objazdową, więc jej przeciążenie na
//! drzewie jest artefaktem wyboru drzewa, a nie stanem sieci.
//!
//! Zysk: jedno przejście Tarjana zamiast kontrakcji grafu, i ani jednej struktury
//! pomocniczej więcej. Cena, nazwana wprost: **pierścień nie przeciąża się nigdy**,
//! bo żadna jego krawędź nie jest mostem i żadnej nie sprawdzamy.

use magnat_core::{HashState, StateHasher};

/// Brak rodzica w lesie rozpinającym — korzeń wyspy.
pub const NO_PARENT: u32 = u32::MAX;

/// Wynik przejścia po grafie. Wszystko indeksowane numerem węzła albo krawędzi,
/// wszystko w `Vec` — zakaz `HashMap` z 00 §3.2 obowiązuje także w cache'u.
#[derive(Clone, Debug, Default)]
pub struct TopologyCache {
    /// Numer wyspy: indeks węzła, od którego zaczęło się przejście. Dwa węzły
    /// w tej samej wyspie mają tę samą wartość.
    pub island: Vec<u32>,
    /// Rodzic w lesie rozpinającym albo [`NO_PARENT`].
    pub parent: Vec<u32>,
    /// Krawędź łącząca węzeł z rodzicem; przy korzeniu wartość nieokreślona.
    pub parent_edge: Vec<u32>,
    /// Węzły w kolejności postorder — dziecko zawsze przed rodzicem, więc suma
    /// poddrzewa liczy się jednym przejściem w przód.
    pub post_order: Vec<u32>,
    /// Czy krawędź jest mostem. Tylko mosty mają sprawdzaną przepustowość.
    pub is_bridge: Vec<bool>,
    /// Korzenie wysp: najpierw źródła w kolejności indeksu, potem reszta.
    /// Kolejność jest deterministyczna i to jest jedyne, czego od niej wymagamy.
    pub roots: Vec<u32>,
    buf: Buf,
}

/// Bufory przejścia. Mieszkają w cache'u, bo kaskada przebudowuje topologię
/// do ośmiu razy w jednym ticku i alokacja ośmiu tablic po 5 tys. elementów
/// razy pięć sieci jest **widoczna w budżecie** z testu T8b, a nie teoretyczna.
#[derive(Clone, Debug, Default)]
struct Buf {
    /// Początek listy sąsiedztwa węzła w układzie CSR.
    start: Vec<u32>,
    /// `(sąsiad, indeks krawędzi)`
    items: Vec<(u32, u32)>,
    cur: Vec<u32>,
    /// Dla ilu węzłów i krawędzi zbudowano CSR. Listy sąsiedztwa zależą wyłącznie
    /// od **zbioru** krawędzi, a ten w obrębie jednego kroku się nie zmienia —
    /// zmienia się tylko, które z nich są czynne. Kaskada przebudowywała je więc
    /// osiem razy po nic, a to była najdroższa część kroku.
    ksztalt: (usize, usize),
    disc: Vec<u32>,
    low: Vec<u32>,
    /// Pozycja w liście sąsiedztwa, na której stanęliśmy dla danego węzła.
    it: Vec<u32>,
    stos: Vec<u32>,
}

impl Buf {
    fn build_adjacency(&mut self, nodes: usize, edges: &[(u32, u32)]) {
        if self.ksztalt == (nodes, edges.len()) && !self.items.is_empty() {
            return;
        }
        self.ksztalt = (nodes, edges.len());
        self.start.clear();
        self.start.resize(nodes + 1, 0);
        for &(a, b) in edges {
            self.start[a as usize + 1] += 1;
            self.start[b as usize + 1] += 1;
        }
        for i in 1..self.start.len() {
            self.start[i] += self.start[i - 1];
        }
        self.items.clear();
        self.items.resize(self.start[nodes] as usize, (0, 0));
        self.cur.clear();
        self.cur.extend_from_slice(&self.start);
        for (i, &(a, b)) in edges.iter().enumerate() {
            self.items[self.cur[a as usize] as usize] = (b, i as u32);
            self.cur[a as usize] += 1;
            self.items[self.cur[b as usize] as usize] = (a, i as u32);
            self.cur[b as usize] += 1;
        }
    }

    fn range(&self, v: u32) -> &[(u32, u32)] {
        let a = self.start[v as usize] as usize;
        let b = self.start[v as usize + 1] as usize;
        &self.items[a..b]
    }
}

impl TopologyCache {
    /// Unieważnia cache list sąsiedztwa.
    ///
    /// CSR jest kluczowany parą `(węzły, krawędzie)`, bo w obrębie kroku zmienia
    /// się wyłącznie to, **które** krawędzie są czynne, a nie gdzie prowadzą.
    /// Przepięcie istniejącej krawędzi liczby nie zmienia, więc klucz by tego nie
    /// zauważył — i dlatego publiczna droga zmiany topologii przechodzi tędy.
    pub fn invalidate(&mut self) {
        self.buf.ksztalt = (usize::MAX, usize::MAX);
    }

    /// Jedno iteracyjne przejście DFS po krawędziach czynnych: wyspy, las,
    /// postorder i mosty naraz. Rekurencji nie ma z rozmysłu — sieć metropolii
    /// ma ~5 tys. węzłów, a głębokość drzewa promieniowego bywa bliska liczbie
    /// węzłów, więc stos wątku jest tu realnym ograniczeniem, nie teoretycznym.
    ///
    /// Determinizm jest własnością konstrukcji: sąsiedzi stoją w CSR w kolejności
    /// indeksu krawędzi, a korzenie w kolejności indeksu węzła w obrębie grupy.
    /// `prefer_root` wskazuje węzły, od których przejście ma zaczynać w pierwszej
    /// kolejności — w praktyce źródła. Ma to skutek, a nie jest kosmetyką:
    /// przepływ krawędzi liczy się jako suma popytu **poddrzewa**, a ta liczba
    /// jest przepływem tylko wtedy, gdy korzeniem wyspy jest jej źródło.
    /// Przy korzeniu w losowym odbiorcy moc „płynęłaby" od odbiorcy do elektrowni.
    pub fn rebuild(
        &mut self,
        nodes: usize,
        edges: &[(u32, u32)],
        active: &[bool],
        prefer_root: &[bool],
    ) {
        let mut buf = std::mem::take(&mut self.buf);
        buf.build_adjacency(nodes, edges);
        self.island.clear();
        self.island.resize(nodes, u32::MAX);
        self.parent.clear();
        self.parent.resize(nodes, NO_PARENT);
        self.parent_edge.clear();
        self.parent_edge.resize(nodes, u32::MAX);
        self.post_order.clear();
        self.post_order.reserve(nodes);
        self.is_bridge.clear();
        self.is_bridge.resize(edges.len(), false);
        self.roots.clear();

        buf.disc.clear();
        buf.disc.resize(nodes, u32::MAX);
        buf.low.clear();
        buf.low.resize(nodes, u32::MAX);
        buf.it.clear();
        buf.it.resize(nodes, 0);
        buf.stos.clear();
        let mut timer: u32 = 0;

        for start in (0..nodes as u32)
            .filter(|i| prefer_root.get(*i as usize).copied().unwrap_or(false))
            .chain(0..nodes as u32)
        {
            if buf.disc[start as usize] != u32::MAX {
                continue;
            }
            self.roots.push(start);
            buf.disc[start as usize] = timer;
            buf.low[start as usize] = timer;
            timer += 1;
            self.island[start as usize] = start;
            buf.stos.push(start);
            while let Some(&v) = buf.stos.last() {
                let i = buf.it[v as usize] as usize;
                let dalej = buf.range(v).get(i).copied();
                if let Some((w, e)) = dalej {
                    buf.it[v as usize] += 1;
                    // Krawędź wyłączona nie istnieje dla przejścia — CSR niesie
                    // wszystkie, bo jest cache'owany na zbiór, a nie na stan.
                    if !active[e as usize] {
                        continue;
                    }
                    // Krawędź, którą tu weszliśmy, nie jest krawędzią powrotną —
                    // i porównujemy **indeks krawędzi**, a nie numer rodzica,
                    // bo dwie linie między tą samą parą stacji to nie most.
                    if self.parent[v as usize] != NO_PARENT && self.parent_edge[v as usize] == e {
                        continue;
                    }
                    if buf.disc[w as usize] == u32::MAX {
                        self.parent[w as usize] = v;
                        self.parent_edge[w as usize] = e;
                        self.island[w as usize] = start;
                        buf.disc[w as usize] = timer;
                        buf.low[w as usize] = timer;
                        timer += 1;
                        buf.stos.push(w);
                    } else {
                        buf.low[v as usize] = buf.low[v as usize].min(buf.disc[w as usize]);
                    }
                } else {
                    buf.stos.pop();
                    self.post_order.push(v);
                    if let Some(&p) = buf.stos.last() {
                        buf.low[p as usize] = buf.low[p as usize].min(buf.low[v as usize]);
                        if buf.low[v as usize] > buf.disc[p as usize] {
                            self.is_bridge[self.parent_edge[v as usize] as usize] = true;
                        }
                    }
                }
            }
        }
        self.buf = buf;
    }
}

impl HashState for TopologyCache {
    /// Topologia jest **wyliczalna z krawędzi**, więc do hasha stanu wchodzi
    /// tylko tyle, ile trzeba, żeby rozjazd w niej dało się zauważyć od razu,
    /// a nie dopiero jako inny rachunek za prąd trzy miesiące później.
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.roots.len() as u32);
        for r in &self.roots {
            h.write_u32(*r);
        }
        for p in &self.parent {
            h.write_u32(*p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ścieżka `0-1-2`: każda krawędź jest mostem, a postorder idzie od końca.
    #[test]
    fn sciezka_ma_same_mosty() {
        let mut t = TopologyCache::default();
        t.rebuild(3, &[(0, 1), (1, 2)], &[true, true], &[]);
        assert_eq!(t.is_bridge, vec![true, true]);
        assert_eq!(t.post_order, vec![2, 1, 0]);
        assert_eq!(t.roots, vec![0]);
        assert_eq!(t.island, vec![0, 0, 0]);
    }

    /// Pierścień `0-1-2-0`: ani jednego mostu, bo każdy węzeł ma objazd.
    #[test]
    fn pierscien_nie_ma_mostow() {
        let mut t = TopologyCache::default();
        t.rebuild(3, &[(0, 1), (1, 2), (2, 0)], &[true; 3], &[]);
        assert_eq!(t.is_bridge, vec![false, false, false]);
    }

    /// Pierścień z odgałęzieniem: most jest dokładnie jeden i jest nim odgałęzienie.
    #[test]
    fn odgalezienie_od_pierscienia_jest_mostem() {
        let mut t = TopologyCache::default();
        t.rebuild(4, &[(0, 1), (1, 2), (2, 0), (2, 3)], &[true; 4], &[]);
        assert_eq!(t.is_bridge, vec![false, false, false, true]);
    }

    /// Dwie linie między tą samą parą stacji nie są mostami — ta sama sytuacja
    /// co pierścień, tylko o długości dwa. Gdyby przejście porównywało **rodzica**
    /// zamiast **krawędzi**, obie wypadłyby jako mosty i sieć z rezerwą wyglądałaby
    /// tak samo krucho jak sieć bez niej.
    #[test]
    fn rownolegle_krawedzie_nie_sa_mostami() {
        let mut t = TopologyCache::default();
        t.rebuild(2, &[(0, 1), (0, 1)], &[true, true], &[]);
        assert_eq!(t.is_bridge, vec![false, false]);
    }

    /// Korzeń wskazany przez `prefer_root` wygrywa z kolejnością indeksu — bez
    /// tego przepływ na ścieżce `źródło(2)-1-0` liczyłby się od odbiorcy.
    #[test]
    fn zrodlo_zostaje_korzeniem_wyspy() {
        let mut t = TopologyCache::default();
        t.rebuild(3, &[(0, 1), (1, 2)], &[true, true], &[false, false, true]);
        assert_eq!(t.roots, vec![2]);
        assert_eq!(t.parent[0], 1);
        assert_eq!(t.parent[1], 2);
    }

    /// Krawędź wyłączona dzieli sieć na dwie wyspy — i to jest cały mechanizm
    /// rozpadu z kroku 5.
    #[test]
    fn wylaczona_krawedz_rozdziela_wyspy() {
        let mut t = TopologyCache::default();
        t.rebuild(3, &[(0, 1), (1, 2)], &[true, false], &[]);
        assert_eq!(t.roots, vec![0, 2]);
        assert_eq!(t.island, vec![0, 0, 2]);
    }
}
