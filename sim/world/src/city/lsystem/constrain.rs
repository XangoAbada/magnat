//! Ograniczenia: co odsiewa propozycję i w jakiej kolejności.
//!
//! Drugi z trzech tematów, które do R2e mieszkały w jednym pliku (`R2-WP21`).
//! Reguła produkcji **proponuje** odcinek; tutaj rozstrzyga się, czy on powstanie,
//! a jeśli tak — jak zostanie doklejony do sieci.
//!
//! **Kolejność odsiewu jest kontraktem** i to jest jedyna rzecz, której przy tym
//! podziale trzeba było pilnować: strefy zakazane, limit spadku z sześcioma próbami
//! ratunku, wybór konstrukcji, doklejenie do węzła, przecięcie z istniejącym
//! segmentem, kontrola kąta minimalnego. Zamiana dwóch kroków miejscami zmienia
//! **każde miasto z każdego ziarna** — nie dlatego, że któryś jest ważniejszy,
//! tylko dlatego, że wcześniejszy zjada budżet prób późniejszego.
//!
//! Symbole przeniesione tu **co do linii**, bez przepisywania: podział, który
//! przy okazji poprawia kod, nie jest podziałem, tylko zmianą zachowania bez testu.

use super::*;

impl Builder<'_> {
    fn push_segment(
        &mut self,
        a: NodeId,
        b: NodeId,
        class: RoadClass,
        structure: RoadStructure,
        extra: RoadFlags,
    ) -> SegmentId {
        let spec = crate::city::road::spec(class);
        let (pa, pb) = (self.nodes[a.0 as usize].pos, self.nodes[b.0 as usize].pos);
        let geom = self.geom.push(&[pa, pb]);
        let mut flags = if crate::city::road::has_sidewalk(class) {
            extra.with(RoadFlags::SIDEWALK)
        } else {
            extra
        };
        if spec.lanes_bwd == 0 {
            flags = flags.with(RoadFlags::ONEWAY);
        }
        if class.is_rail() {
            flags = RoadFlags::RAIL.with(extra);
        }
        if crate::city::road::forbids_heavy(class) {
            flags = flags.with(RoadFlags::NO_HEAVY);
        }
        let id = SegmentId(self.segments.len() as u32);
        self.segments.push(RoadSegment {
            a,
            b,
            class,
            geom,
            structure,
            lanes_fwd: spec.lanes_fwd,
            lanes_bwd: spec.lanes_bwd,
            row_m: spec.row_m,
            speed_kph: spec.speed_kph,
            max_tonnage_t: spec.max_tonnage_t,
            length_dm: ((pb - pa).length() * 10.0) as u32,
            district: UNASSIGNED_DISTRICT,
            flags,
        });
        self.nodes[a.0 as usize].degree = self.nodes[a.0 as usize].degree.saturating_add(1);
        self.nodes[b.0 as usize].degree = self.nodes[b.0 as usize].degree.saturating_add(1);
        let r = class.rank();
        self.node_rank[a.0 as usize] = self.node_rank[a.0 as usize].max(r);
        self.node_rank[b.0 as usize] = self.node_rank[b.0 as usize].max(r);
        self.index_segment(id, pa, pb);
        match structure {
            RoadStructure::Bridge { .. } => self.stats.bridges += 1,
            RoadStructure::Tunnel { .. } => self.stats.tunnels += 1,
            RoadStructure::Embankment { .. } => self.stats.embankments += 1,
            RoadStructure::AtGrade => {}
        }
        id
    }

    /// Podział istniejącego segmentu w punkcie `p` (ograniczenie lokalne 4).
    ///
    /// Stare wpisy w indeksie zostają: pokrywają **nadzbiór** nowego przebiegu, więc
    /// zapytanie zwróci ten segment jako kandydata, a dokładny test i tak go odrzuci.
    /// Kasowanie ich kosztowałoby przeszukanie komórek bez żadnej korzyści.
    fn split_segment(&mut self, s: SegmentId, p: Vec2) -> NodeId {
        let seg = self.segments[s.0 as usize];
        let (na, nb) = (self.nodes[seg.a.0 as usize], self.nodes[seg.b.0 as usize]);
        let dl = (nb.pos - na.pos).length().max(1.0);
        let t = ((p - na.pos).length() / dl).clamp(0.0, 1.0);
        let z = na.z_dm + ((nb.z_dm - na.z_dm) as f32 * t) as i32;
        let n = self.add_node_z(p, z, NodeFlags::JUNCTION);
        let pa = na.pos;
        self.segments[s.0 as usize].b = n;
        self.segments[s.0 as usize].geom = self.geom.push(&[pa, p]);
        self.segments[s.0 as usize].length_dm = ((p - pa).length() * 10.0) as u32;
        self.nodes[n.0 as usize].degree += 1;
        self.node_rank[n.0 as usize] = seg.class.rank();
        // Druga połowa dziedziczy wszystko poza geometrią.
        let pb = self.nodes[seg.b.0 as usize].pos;
        let id = SegmentId(self.segments.len() as u32);
        let mut half = seg;
        half.a = n;
        half.geom = self.geom.push(&[p, pb]);
        half.length_dm = ((pb - p).length() * 10.0) as u32;
        self.segments.push(half);
        self.nodes[n.0 as usize].degree += 1;
        self.index_segment(id, p, pb);
        self.stats.split += 1;
        n
    }

    // ── ograniczenia lokalne ─────────────────────────────────────────────────────────

    /// Ograniczenie 6 (w planie numerowane 7): strefa zakazana.
    ///
    /// „Na wodzie bez mostu" znaczy: **węzeł** nie ma prawa stanąć w wodzie. Sam odcinek
    /// wolno poprowadzić nad wodą, bo od tego jest most — ale bez tego warunku miasto
    /// portowe rozrastało się mostami **w morze**: każdy 200-metrowy odcinek arterii
    /// mieścił się w limicie rozpiętości 300 m, więc L-system budował po tafli w nieskończoność.
    fn forbidden(&self, p: Vec2) -> bool {
        if p.x < 8.0 || p.y < 8.0 || p.x > self.map_m - 8.0 || p.y > self.map_m - 8.0 {
            return true;
        }
        if crate::city::gates::is_water(self.t, p) {
            return true;
        }
        self.airport.is_some_and(|fp| fp.contains(p))
    }

    /// Ograniczenie 1: **niweleta**, czyli podłużne nachylenie samej drogi.
    ///
    /// **Korekta wobec planu.** Plan każe próbkować `slope_at` co 8 m i porównywać
    /// z `max_slope[class]`. To jest inna wielkość: `slope_at` zwraca moduł gradientu
    /// terenu, więc droga biegnąca poziomo po zboczu ma niweletę 0%, a `slope_at` 20%.
    /// Przy kryterium z planu autostrada (5%) była odrzucana już na pierwszym segmencie
    /// w każdym regionie — zmierzone: 100% odrzuceń. Liczymy więc spadek między końcami
    /// odcinka; za profil pośredni (wykop, nasyp) odpowiada ograniczenie 2, które ma
    /// od M1 gotową odpowiedź w `crossing_cost`.
    ///
    /// Jednostka wyniku jest ta sama co `slope_at` (tan α × 64), żeby próg z tabeli klas
    /// dało się porównać bez konwersji w dwie strony.
    fn grade(&self, a: Vec2, b: Vec2) -> u8 {
        let ha = self.t.height_at(a.x as i32, a.y as i32) * 5;
        let hb = self.t.height_at(b.x as i32, b.y as i32) * 5;
        grade_units(a, ha, b, hb)
    }

    /// Ograniczenie 2: przeszkoda i wybór struktury. Reguła mieszka w `road`,
    /// bo używa jej także WP5b (tory) — jedna reguła, jedna implementacja.
    fn structure_for(&self, a: Vec2, b: Vec2, spec: &ClassSpec) -> Option<RoadStructure> {
        crate::city::road::structure_for(self.t, a, b, spec)
    }

    /// Rzędna niwelety segmentu `s` w punkcie `p` leżącym na nim.
    fn interp_z(&self, s: SegmentId, p: Vec2) -> i32 {
        let seg = &self.segments[s.0 as usize];
        let (na, nb) = (self.nodes[seg.a.0 as usize], self.nodes[seg.b.0 as usize]);
        let dl = (nb.pos - na.pos).length().max(1.0);
        let t = ((p - na.pos).length() / dl).clamp(0.0, 1.0);
        na.z_dm + ((nb.z_dm - na.z_dm) as f32 * t) as i32
    }

    /// Ograniczenie 5: minimalny kąt do krawędzi incydentnych w węźle docelowym.
    fn angle_ok(&self, node: NodeId, dir_in: Vec2, class: RoadClass) -> bool {
        // cos 30° = 0,866; dla `Local` próg to 22° → cos = 0,927.
        let limit = if matches!(class, RoadClass::Local | RoadClass::Service) {
            0.927_f32
        } else {
            0.866_f32
        };
        let p = self.nodes[node.0 as usize].pos;
        let incoming = -dir_in.normalize_or_zero();
        for &s in self.segments_at_node(node).iter() {
            let seg = &self.segments[s as usize];
            let other = if seg.a == node { seg.b } else { seg.a };
            let d = (self.nodes[other.0 as usize].pos - p).normalize_or_zero();
            if d.dot(incoming) > limit {
                return false;
            }
        }
        true
    }

    /// Segmenty incydentne z węzłem — liniowo po komórce indeksu, bo na tym etapie
    /// nie ma jeszcze CSR (ten powstaje przy domykaniu).
    pub(super) fn segments_at_node(&self, n: NodeId) -> Vec<u32> {
        let p = self.nodes[n.0 as usize].pos;
        let c = self.cell(p);
        self.seg_grid[c]
            .iter()
            .copied()
            .filter(|&s| {
                let seg = &self.segments[s as usize];
                seg.a == n || seg.b == n
            })
            .collect()
    }

    // ── wzrost ───────────────────────────────────────────────────────────────────────

    /// Próba wyprowadzenia segmentu z propozycji. Zwraca węzeł końcowy i to,
    /// czy kontynuacja ma sens (scalenie z istniejącym węzłem ją wygasza).
    ///
    /// `ponytail:` 261 linii przy progu 250 i **zostaje tak po R2-WP21**. Sufit
    /// nazwany: to jest jeden algorytm — drabina ograniczeń, w której kolejność
    /// kroków jest kontraktem generacji miasta, bo wcześniejszy krok zjada budżet
    /// prób późniejszego. Podział na dwie funkcje przeniósłby połowę drabiny piętro
    /// niżej i nie dodał ani jednej granicy tematycznej, a zamiana dwóch kroków
    /// miejscami przy okazji przenosin przestawiłaby każde miasto z każdego ziarna.
    /// Ścieżka wyjścia: gdy któryś krok przestanie być odsiewem i zacznie być
    /// decyzją (np. wybór konstrukcji z kosztem), wyjdzie stąd **jako temat**,
    /// a nie jako połowa linii.
    pub(super) fn grow(
        &mut self,
        prop: &Proposal,
        near: &mut Vec<u32>,
    ) -> Option<(NodeId, Vec2, bool)> {
        let spec = crate::city::road::spec(prop.class);
        let from = self.nodes[prop.from.0 as usize].pos;
        let mut r = rng(self.plan.seed, StreamId::RoadsL, prop.seq, Tick(0));

        // Cel jawny (reguła P4) omija wzorzec: łącznik ma trafić w konkretny węzeł.
        let (dir0, len0) = match prop.target {
            Some(t) => {
                let d = self.nodes[t.0 as usize].pos - from;
                (d.normalize_or_zero(), d.length())
            }
            None => {
                // Blokowisko: kolektor prowadzi trzykrotnie dłuższy odcinek,
                // bo superblok jest z definicji kwartałem bez ulic w środku.
                let mult = if matches!(self.pattern_at(from), Pattern::Superblock(_))
                    && matches!(prop.class, RoadClass::Collector)
                {
                    3.0
                } else {
                    1.0
                };
                (
                    self.steer(prop, from, &mut r),
                    f32::from(spec.seg_len_m) * mult,
                )
            }
        };
        if dir0.length_squared() < 0.5 {
            return None;
        }

        // Ograniczenie 1 z próbami ratunkowymi: obrót ±15°, ±30°, potem skrócenie do 50%.
        let proby: [(f32, f32); 6] = [
            (0.0, 1.0),
            (15.0, 1.0),
            (-15.0, 1.0),
            (30.0, 1.0),
            (-30.0, 1.0),
            (0.0, 0.5),
        ];
        let limit = spec.max_slope_units();
        let mut wybrane: Option<(Vec2, Vec2, RoadStructure)> = None;
        let mut powod_slope = false;
        let mut powod_cross = false;

        for (deg, skala) in proby {
            if prop.target.is_some() && (deg != 0.0 || skala != 1.0) {
                break; // łącznik nie negocjuje przebiegu — albo trafia, albo nie
            }
            let dir = rotate(dir0, deg);
            let end = from + dir * (len0 * skala);
            if self.forbidden(end) {
                self.stats.rejected_zone += 1;
                continue;
            }
            let Some(structure) = self.structure_for(from, end, &spec) else {
                powod_cross = true;
                continue;
            };
            // Nachylenie dotyczy przebiegu po gruncie; most i tunel go nie widzą.
            let po_gruncie = matches!(
                structure,
                RoadStructure::AtGrade | RoadStructure::Embankment { .. }
            );
            if po_gruncie && self.grade(from, end) > limit {
                powod_slope = true;
                continue;
            }
            wybrane = Some((dir, end, structure));
            break;
        }

        let Some((dir, mut end, structure)) = wybrane else {
            if powod_slope {
                self.stats.rejected_slope += 1;
            } else if powod_cross {
                self.stats.rejected_crossing += 1;
            }
            return None;
        };

        // Ograniczenie 3 (w planie 4): snapowanie do istniejącego węzła.
        let snap_r = len0 * 0.25;
        let mut scalony = false;
        let mut target = match prop.target {
            Some(t) => Some(t),
            None => self.find_node(end, snap_r, prop.class.rank(), &[prop.from]),
        };
        if let Some(t) = target {
            end = self.nodes[t.0 as usize].pos;
            scalony = true;
        }

        // Ograniczenie 4 (w planie 5): przecięcie z istniejącym segmentem.
        //
        // Sonda jest **przedłużona o 25%**, gdy koniec nie trafił w żaden węzeł.
        // To jest to samo, co robi snapowanie, tylko wobec krawędzi zamiast wierzchołka:
        // ulica kończąca się 20 m przed równoległą do niej ulicą ma się do niej podłączyć,
        // a nie zostać ślepym zaułkiem, który i tak wypadnie przy domykaniu sieci.
        // Bez tego sieć zostaje drzewem: zmierzone 161 ścian na 923 segmenty.
        let probe_end = if scalony {
            end
        } else {
            from + dir * (end - from).length() * 1.25
        };
        self.segments_near(from, probe_end, near);
        // Trafienie bliżej niż to od węzła jest **stykiem**, nie skrzyżowaniem.
        // Próg metrowy, nie ułamkowy: przy ułamkowym przecięcie 40 cm od końca
        // czterdziestometrowej dojazdówki wypadało z wykrywania, nie dostawało węzła
        // i po cichu łamało planarność grafu, na której stoi wyznaczanie kwartałów.
        const STYK_M: f32 = 1.0;
        let mut najblizsze: Option<(f32, SegmentId, Vec2)> = None;
        let mut do_wezla: Option<(f32, NodeId)> = None;
        let mut wiadukt: Option<SegmentId> = None;
        for &s in near.iter() {
            let seg = &self.segments[s as usize];
            let (pa, pb) = (
                self.nodes[seg.a.0 as usize].pos,
                self.nodes[seg.b.0 as usize].pos,
            );
            let Some((p, t_param)) = segment_intersection(from, probe_end, pa, pb) else {
                continue;
            };
            // Styk w węźle początkowym kandydata — to nie jest przecięcie, tylko wyjście
            // z tego samego skrzyżowania.
            if (p - from).length() < STYK_M {
                continue;
            }
            // Bezkolizyjny jest **most i tunel**, nie nasyp: droga na nasypie nadal
            // krzyżuje się z innymi w poziomie (plan §5.2 mówi wprost o `Bridge × AtGrade`).
            let bezkolizyjny = |st: RoadStructure| {
                matches!(
                    st,
                    RoadStructure::Bridge { .. } | RoadStructure::Tunnel { .. }
                )
            };
            if bezkolizyjny(structure) || bezkolizyjny(seg.structure) {
                if wiadukt.is_none() {
                    wiadukt = Some(SegmentId(s));
                }
                continue;
            }
            // Trafienie tuż przy końcu istniejącego segmentu: kandydat ma się podłączyć
            // do **tego węzła**, a nie przejść obok niego bez połączenia.
            let blisko = if (p - pa).length() < STYK_M {
                Some(seg.a)
            } else if (p - pb).length() < STYK_M {
                Some(seg.b)
            } else {
                None
            };
            if let Some(n) = blisko {
                if n != prop.from && do_wezla.is_none_or(|(b, _)| t_param < b) {
                    do_wezla = Some((t_param, n));
                }
                continue;
            }
            if self.forbidden(p) {
                continue;
            }
            if najblizsze
                .is_none_or(|(b, bs, _)| t_param < b || (t_param == b && SegmentId(s) < bs))
            {
                najblizsze = Some((t_param, SegmentId(s), p));
            }
        }
        // Bliżej jest to, co napotkamy pierwsze — węzeł albo środek segmentu.
        if let Some((tw, n)) = do_wezla {
            if najblizsze.is_none_or(|(t, _, _)| tw < t) {
                najblizsze = None;
                target = Some(n);
                end = self.nodes[n.0 as usize].pos;
                scalony = true;
            }
        }

        let mut structure = structure;
        // Podział istniejącego segmentu jest **ostatnią** rzeczą, jaką robimy: mutuje sieć,
        // więc nie może się wydarzyć przed sprawdzeniem, czy segment w ogóle powstanie.
        let mut do_podzialu: Option<(SegmentId, Vec2)> = None;
        if let Some((_, s, p)) = najblizsze {
            end = p;
            do_podzialu = Some((s, p));
            scalony = true;
        }

        // Snapowanie, przedłużenie i podział — każde z nich zmienia koniec odcinka,
        // a więc i jego długość. Struktura musi zostać przeliczona na **ostateczny**
        // przebieg, inaczej most rósłby ponad limit rozpiętości swojej klasy.
        if scalony {
            let Some(st) = self.structure_for(from, end, &spec) else {
                self.stats.rejected_crossing += 1;
                return None;
            };
            structure = st;
        }

        // **Niweleta ostateczna.** Pierwsze sprawdzenie (ograniczenie 1) dotyczyło końca
        // sprzed snapowania i przedłużenia; po nich odcinek ma inną długość i inny koniec,
        // a limit klasy obowiązuje ten, który naprawdę powstanie.
        let z_from = self.nodes[prop.from.0 as usize].z_dm;
        let z_end = match (target, do_podzialu) {
            (_, Some((s, p))) => self.interp_z(s, p),
            (Some(t), None) => self.nodes[t.0 as usize].z_dm,
            (None, None) => self.t.height_at(end.x as i32, end.y as i32) * 5,
        };
        if matches!(
            structure,
            RoadStructure::AtGrade | RoadStructure::Embankment { .. }
        ) && grade_units(from, z_from, end, z_end) > limit
        {
            self.stats.rejected_slope += 1;
            return None;
        }

        if let Some((s, p)) = do_podzialu {
            target = Some(self.split_segment(s, p));
        }

        // Ograniczenie 5 (w planie 6): minimalny kąt — **także w węźle początkowym**.
        // Bez tego dwie propozycje wychodzące z tego samego węzła w niemal tym samym
        // kierunku dawały dwie **nakładające się** krawędzie. Geometrycznie one się nie
        // „przecinają" (wyznacznik zeruje się na współliniowości), więc test planarności
        // ich nie widzi, ale porządek kątowy w węźle przestaje odpowiadać rysunkowi
        // i obchód półkrawędzi scala sąsiednie ściany: zmierzone 154 orbity wobec 164
        // wynikających ze wzoru Eulera.
        if !self.angle_ok(prop.from, from - end, prop.class) {
            self.stats.rejected_angle += 1;
            return None;
        }

        let node = match target {
            Some(t) => {
                if !self.angle_ok(t, end - from, prop.class) {
                    self.stats.rejected_angle += 1;
                    return None;
                }
                self.stats.snapped += 1;
                t
            }
            None => self.add_node_z(end, z_end, NodeFlags::NONE),
        };
        if node == prop.from {
            return None;
        }
        let sid = self.push_segment(prop.from, node, prop.class, structure, RoadFlags::NONE);
        if let Some(inny) = wiadukt {
            self.segments[sid.0 as usize].flags = self.segments[sid.0 as usize]
                .flags
                .with(RoadFlags::GRADE_SEPARATED);
            self.segments[inny.0 as usize].flags = self.segments[inny.0 as usize]
                .flags
                .with(RoadFlags::GRADE_SEPARATED);
        }
        self.stats.accepted += 1;
        Some((node, dir, !scalony))
    }
}

/// Niweleta w jednostkach `slope_at` (tan α × 64), z rzędnych w decymetrach.
fn grade_units(a: Vec2, za_dm: i32, b: Vec2, zb_dm: i32) -> u8 {
    let dh = (zb_dm - za_dm).abs() as f32 * 0.1;
    let len = (b - a).length().max(1.0);
    ((dh / len) * 64.0).min(255.0) as u8
}

/// Przecięcie dwóch odcinków w ich domknięciu. Zwraca punkt i parametr wzdłuż pierwszego.
///
/// Bez epsilonów — o tym, czy trafienie jest skrzyżowaniem, stykiem w węźle, czy
/// niczym, rozstrzyga wywołujący, bo tylko on wie, które węzły są jego własne.
fn segment_intersection(a1: Vec2, a2: Vec2, b1: Vec2, b2: Vec2) -> Option<(Vec2, f32)> {
    let r = a2 - a1;
    let s = b2 - b1;
    let denom = r.x * s.y - r.y * s.x;
    if denom.abs() < 1e-6 {
        return None;
    }
    let q = b1 - a1;
    let t = (q.x * s.y - q.y * s.x) / denom;
    let u = (q.x * r.y - q.y * r.x) / denom;
    if !(0.0..=1.0).contains(&t) || !(0.0..=1.0).contains(&u) {
        return None;
    }
    Some((a1 + r * t, t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn przeciecie_zwraca_punkt_na_obu_odcinkach() {
        let (p, t) = segment_intersection(
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(5.0, -5.0),
            Vec2::new(5.0, 5.0),
        )
        .expect("odcinki się przecinają");
        assert!((p - Vec2::new(5.0, 0.0)).length() < 1e-5);
        assert!((t - 0.5).abs() < 1e-5);

        // Odcinki równoległe nie mają przecięcia.
        assert!(segment_intersection(
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(0.0, 5.0),
            Vec2::new(10.0, 5.0),
        )
        .is_none());

        // Styk w końcu **jest** zwracany: o tym, czy to skrzyżowanie, czy sąsiedztwo
        // w węźle, rozstrzyga wywołujący progiem metrowym (korekta B13).
        assert!(segment_intersection(
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 9.0),
        )
        .is_some());
    }
}
