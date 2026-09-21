//! Produkcja: pięć reguł L-systemu i kierowanie odcinkiem.
//!
//! Pierwszy z trzech tematów, które do R2e mieszkały w jednym pliku (`R2-WP21`).
//! Reguła **proponuje** odcinek i nie wie, czy on powstanie — o tym rozstrzyga
//! [`super::constrain`]. Podział przebiega dokładnie tam: propozycja wychodzi
//! stąd, odsiew jest tam, kolejka jest w [`super`].
//!
//! Do R2e wszystkie pięć były blokami **w środku** pętli `grow_network`, każdy
//! czytający osiem zmiennych wolnych i mutujący kopiec przez domknięcie — i to jest
//! powód, dla którego rejestr długu nosił je od R1 bez wykonawcy. Wyjęcie ich
//! wymagało najpierw [`Krok`]: kontekstu liczonego raz na propozycję zdjętą
//! z kolejki. Sam podział jest **mechaniczny**: kolejność wpychania, kolejność
//! losowań i wartości nie zmieniają się o bit, bo `seq` nadaje się w miejscu
//! wywołania, a hash miasta jest funkcją tej kolejności.

use super::*;

impl Builder<'_> {
    pub(super) fn pattern_at(&self, p: Vec2) -> Pattern {
        let r = (p - self.center).length() / self.urban_r.max(1.0);
        let slope = self.t.slope_at(p.x as i32, p.y as i32);
        self.rings.pattern_at(r, slope, self.grid_theta)
    }

    /// Kierunek kolejnego segmentu wg wzorca globalnego.
    pub(super) fn steer(&self, prop: &Proposal, from: Vec2, r: &mut Rng) -> Vec2 {
        if prop.ring {
            // Obwodnica: styczna do okręgu wokół środka, zawsze w tę samą stronę.
            let radial = (from - self.center).normalize_or_zero();
            return Vec2::new(-radial.y, radial.x);
        }
        let d = prop.dir.normalize_or_zero();
        match self.pattern_at(from) {
            Pattern::Radial => {
                let to_c = (self.center - from).normalize_or_zero();
                let base = if d.dot(to_c) > 0.0 { to_c } else { -to_c };
                rotate(base, jitter(r, 8.0))
            }
            Pattern::Grid(theta) => {
                let snapped = snap_to_grid(d, theta);
                rotate(snapped, jitter(r, 3.0))
            }
            Pattern::Organic => rotate(d, jitter(r, 25.0)),
            Pattern::Contour => {
                // Pięciu kandydatów co 12°, wygrywa najmniejsze nachylenie.
                let mut best = (u8::MAX, d);
                for i in -2..=2 {
                    let c = rotate(d, (i * 12) as f32);
                    let probe = from + c * 40.0;
                    let s = self.t.slope_at(probe.x as i32, probe.y as i32);
                    if s < best.0 {
                        best = (s, c);
                    }
                }
                best.1
            }
            Pattern::Superblock(theta) => snap_to_grid(d, theta),
        }
    }
}

/// Szum kierunku w zakresie ±`amp` stopni.
fn jitter(r: &mut Rng, amp: f32) -> f32 {
    let v = r.gen_range_u32(2001) as f32 / 1000.0 - 1.0;
    v * amp
}

/// Snapowanie do najbliższej z czterech osi siatki obróconej o `theta` stopni.
fn snap_to_grid(d: Vec2, theta: f32) -> Vec2 {
    let base = rotate(Vec2::new(1.0, 0.0), theta);
    let perp = Vec2::new(-base.y, base.x);
    let mut best = (f32::MIN, base);
    for c in [base, -base, perp, -perp] {
        let dot = d.dot(c);
        if dot > best.0 {
            best = (dot, c);
        }
    }
    best.1
}

/// Kontekst jednej propozycji zdjętej z kolejki — to, co pięć reguł czyta wspólnie.
///
/// Liczony **raz**, zaraz po `grow`, a nie w każdej regule z osobna. Powód jest ten
/// sam, dla którego reguły dało się w ogóle wyjąć z pętli: wszystkie pięć pracuje
/// na tych samych liczbach, a policzenie ich po raz drugi byłoby drugą definicją
/// tego, czym jest „koniec łańcucha".
pub(super) struct Krok {
    pub prop: Proposal,
    /// Węzeł końcowy odcinka, który właśnie powstał.
    pub node: NodeId,
    /// Kierunek wyjścia z tego węzła.
    pub dir: Vec2,
    /// Czy kontynuacja ma sens — scalenie z istniejącym węzłem ją wygasza.
    pub kontynuuj: bool,
    pub p_end: Vec2,
    pub spec: ClassSpec,
    pub limit_generacji: bool,
    /// Czy koniec odcinka leży w obszarze zurbanizowanym (z zapasem 15 %).
    pub w_miescie: bool,
    pub kontynuuj_dalej: bool,
    pub klasa_dalej: RoadClass,
    pub gen_dalej: u16,
}

impl Krok {
    pub(super) fn nowy(
        b: &Builder<'_>,
        prop: Proposal,
        node: NodeId,
        dir: Vec2,
        kontynuuj: bool,
        center: Vec2,
    ) -> Krok {
        let p_end = b.nodes[node.0 as usize].pos;
        let r_end = (p_end - center).length();
        let spec = crate::city::road::spec(prop.class);
        let limit_generacji = prop.gen + 1 > spec.max_gen;
        let w_miescie = r_end < b.urban_r * 1.15;
        let kontynuuj_dalej = if matches!(prop.class, RoadClass::Highway) || prop.gate {
            // Trasa wylotowa i droga bramowa jadą do miasta nawet przez pustkę;
            // reszta rośnie tylko w obszarze zurbanizowanym.
            r_end > b.urban_r * 0.2
        } else {
            w_miescie
        };
        // **Trasa wylotowa wchodzi w miasto jako arteria.** Tak to działa w rzeczywistości
        // (droga krajowa staje się aleją) i tylko tak aksjomat z §5.2 rodzi szkielet
        // miasta: przy 15% szansy na zjazd dwie autostrady dają pół arterii na miasto,
        // czyli sieć, która po przycięciu wiszących końców znika w całości.
        let (klasa_dalej, gen_dalej) =
            if matches!(prop.class, RoadClass::Highway) && r_end < b.urban_r {
                (RoadClass::Arterial, 0)
            } else {
                (prop.class, prop.gen + 1)
            };
        Krok {
            prop,
            node,
            dir,
            kontynuuj,
            p_end,
            spec,
            limit_generacji,
            w_miescie,
            kontynuuj_dalej,
            klasa_dalej,
            gen_dalej,
        }
    }
}

/// Węzeł bieżący i jego bezpośredni sąsiedzi — zbiór wykluczeń przy szukaniu celu
/// łącznika. Wspólny dla P0 i P4, bo obie zadają to samo pytanie.
fn sasiedzi(b: &Builder<'_>, node: NodeId) -> Vec<NodeId> {
    b.segments_at_node(node)
        .iter()
        .map(|&s| {
            let seg = &b.segments[s as usize];
            if seg.a == node {
                seg.b
            } else {
                seg.a
            }
        })
        .chain(std::iter::once(node))
        .collect()
}

/// P1 — kontynuacja łańcucha.
pub(super) fn p1_kontynuacja(k: &Krok, out: &mut impl FnMut(Proposal)) {
    if k.kontynuuj_dalej && k.kontynuuj && !k.limit_generacji {
        out(Proposal {
            from: k.node,
            dir: k.dir,
            class: k.klasa_dalej,
            gen: k.gen_dalej,
            prio: k.prop.prio + 1,
            seq: 0,
            target: None,
            ring: k.prop.ring,
            gate: k.prop.gate,
        });
    }
}

/// P0 — **łącznik domykający**. Uogólnienie reguły P4 z arterii na każdą klasę:
/// łańcuch, który się kończy (limit generacji, wyjście poza obszar, dojazd do
/// środka), próbuje trafić w najbliższą istniejącą ulicę zamiast zostać ślepym
/// zaułkiem. Bez tego 35% powstałych segmentów wypadało przy domykaniu sieci,
/// a graf zostawał drzewem — wbrew celowi reguły P4 z §5.2.
pub(super) fn p0_lacznik(b: &Builder<'_>, k: &Krok, out: &mut impl FnMut(Proposal)) {
    if !(k.kontynuuj && (k.limit_generacji || !k.kontynuuj_dalej)) {
        return;
    }
    let wykluczone = sasiedzi(b, k.node);
    let (klasa, promien) = if matches!(k.prop.class, RoadClass::Highway) {
        (RoadClass::Arterial, 700.0)
    } else {
        (k.prop.class, f32::from(k.spec.seg_len_m) * 1.5)
    };
    if let Some(t) = b.find_node(k.p_end, promien, klasa.rank(), &wykluczone) {
        out(Proposal {
            from: k.node,
            dir: (b.nodes[t.0 as usize].pos - k.p_end).normalize_or_zero(),
            class: klasa,
            gen: k.prop.gen,
            prio: k.prop.prio + 1,
            seq: 0,
            target: Some(t),
            ring: false,
            gate: false,
        });
    }
}

/// P2 — rozgałęzienie boczne.
pub(super) fn p2_odgalezienie(
    b: &Builder<'_>,
    k: &Krok,
    r: &mut Rng,
    out: &mut impl FnMut(Proposal),
) {
    let side = Vec2::new(-k.dir.y, k.dir.x);
    let (dziecko, p_branch, dprio) = match k.prop.class {
        RoadClass::Highway => (Some(RoadClass::Arterial), 150, 12),
        RoadClass::Arterial => (Some(RoadClass::Collector), 550, 6),
        RoadClass::Collector => (Some(RoadClass::Local), 700, 3),
        // `ponytail:` reguła P2 dopuszcza `Local → Service` tylko w kwartale > 2 ha,
        // a kwartały zna dopiero M2c (WP8). Przybliżenie: od drugiej generacji ulicy
        // lokalnej i poza superblokiem. Sufit: w gęstej starówce powstanie sięgacz,
        // który urbanistycznie należałby do podziału kwartału. Wyjście: przenieść
        // ten wariant do WP8, gdy `max_block_area` będzie już policzone.
        // Bez tego równoległe ulice lokalne nigdy się nie przecinają i miasto
        // wychodzi grzebieniem — widać to na podglądzie natychmiast.
        RoadClass::Local if k.prop.gen >= 2 => (Some(RoadClass::Service), 200, 1),
        _ => (None, 0, 0),
    };
    // W superbloku nie ma ulic wewnętrznych — to jest cała jego definicja.
    let dziecko = match (dziecko, b.pattern_at(k.p_end)) {
        (Some(RoadClass::Local), Pattern::Superblock(_)) => None,
        (d, _) => d,
    };
    let Some(kl) = dziecko else { return };
    let co_czwarty = !matches!(k.prop.class, RoadClass::Highway) || k.prop.gen.is_multiple_of(4);
    for znak in [1.0_f32, -1.0] {
        if !co_czwarty || !r.gen_bool_permille(p_branch) {
            continue;
        }
        out(Proposal {
            from: k.node,
            dir: side * znak,
            class: kl,
            gen: 0,
            prio: k.prop.prio + dprio,
            seq: 0,
            target: None,
            ring: false,
            gate: false,
        });
    }
}

/// P3 — rozwidlenie Y (tylko arterie).
pub(super) fn p3_rozwidlenie(k: &Krok, r: &mut Rng, out: &mut impl FnMut(Proposal)) {
    if !(matches!(k.prop.class, RoadClass::Arterial) && !k.prop.ring && r.gen_bool_permille(80)) {
        return;
    }
    for znak in [1.0_f32, -1.0] {
        let kat = (25 + r.gen_range_u32(16)) as f32 * znak;
        out(Proposal {
            from: k.node,
            dir: rotate(k.dir, kat),
            class: RoadClass::Arterial,
            gen: k.prop.gen + 1,
            prio: k.prop.prio + 2,
            seq: 0,
            target: None,
            ring: false,
            gate: false,
        });
    }
}

/// P4 — domknięcie pierścienia: sieć arterii ma być grafem z cyklami, nie drzewem.
pub(super) fn p4_pierscien(b: &Builder<'_>, k: &Krok, out: &mut impl FnMut(Proposal)) {
    if !(matches!(k.prop.class, RoadClass::Arterial) && k.prop.gen >= 3) {
        return;
    }
    let wykluczone = sasiedzi(b, k.node);
    let promien = f32::from(k.spec.seg_len_m) * 1.5;
    // `ponytail:` pełna reguła mówi „węzeł nienależący do przodków"; tu wykluczamy
    // węzeł bieżący i jego bezpośrednich sąsiadów. Sufit: łącznik może zamknąć
    // trójkąt o boku 1,5·seg_len zamiast większego cyklu. Wyjście: pamiętać
    // identyfikator korzenia propozycji, gdyby trójkąty okazały się widoczne.
    if let Some(t) = b.find_node(k.p_end, promien, RoadClass::Arterial.rank(), &wykluczone) {
        out(Proposal {
            from: k.node,
            dir: (b.nodes[t.0 as usize].pos - k.p_end).normalize_or_zero(),
            class: RoadClass::Arterial,
            gen: k.prop.gen + 1,
            prio: k.prop.prio + 2,
            seq: 0,
            target: Some(t),
            ring: false,
            gate: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapowanie_bierze_najblizsza_os() {
        let d = snap_to_grid(Vec2::new(0.9, 0.1), 0.0);
        assert!((d - Vec2::new(1.0, 0.0)).length() < 1e-5);
        let d = snap_to_grid(Vec2::new(-0.1, -0.9), 0.0);
        assert!((d - Vec2::new(0.0, -1.0)).length() < 1e-5);
    }
}
