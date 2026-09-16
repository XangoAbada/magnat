use super::*;

// ── plan dnia ───────────────────────────────────────────────────────────────────

/// Wolny przedział doby.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(super) struct Gap {
    pub(super) start: u16,
    pub(super) end: u16,
}

impl Gap {
    #[inline]
    pub(super) fn len(self) -> u16 {
        self.end.saturating_sub(self.start)
    }
}

/// Plan doby: sloty posortowane po `start_min`, bez nakładania, bez przechodzenia
/// przez północ (doba kończy się snem i zaczyna nowym planem).
///
/// `ponytail:` wstawianie liniowe zamiast drzewa przedziałów — przy 24 slotach całość
/// to najwyżej ~300 porównań na kilku liniach cache. Drzewo dopiero, gdyby limit slotów
/// przekroczył ~64.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DayCanvas {
    slots: ArrayVec<PlanSlot, MAX_SLOTS>,
    /// Cel slotu jako `PlaceRef`, w lockstepie ze `slots`. W zapisie zostaje sam
    /// `PlanSlot.target` (indeks encji) — to jest kopia robocza planera, żeby faza 3
    /// wiedziała, **gdzie** mieszkaniec jest na początku luki.
    places: ArrayVec<PlaceRef, MAX_SLOTS>,
}

impl Default for DayCanvas {
    fn default() -> Self {
        DayCanvas::new()
    }
}

impl DayCanvas {
    #[must_use]
    pub fn new() -> DayCanvas {
        DayCanvas {
            slots: ArrayVec::new(),
            places: ArrayVec::new(),
        }
    }

    #[must_use]
    pub fn slots(&self) -> &[PlanSlot] {
        self.slots.as_slice()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    #[must_use]
    pub fn is_full(&self) -> bool {
        self.slots.is_full()
    }

    /// Wczytuje zapisany plan z areny z powrotem na kanwę — wejście przeplanowania
    /// i karty inspekcji. Cele slotów odtwarza się z ich kluczy, bo arena trzyma
    /// sam indeks encji (§5.1).
    pub fn load(&mut self, slots: &[PlanSlot]) {
        self.slots.clear();
        self.places.clear();
        for s in slots.iter().take(MAX_SLOTS) {
            self.slots.push(*s);
            self.places
                .push(crate::places::place_from_key(s.target).unwrap_or_default());
        }
    }

    pub fn clear(&mut self) {
        self.slots.clear();
        self.places.clear();
    }

    /// Miejsce slotu o podanym indeksie.
    #[must_use]
    pub fn place_of(&self, i: usize) -> PlaceRef {
        self.places[i]
    }

    /// Wstawia slot z zachowaniem porządku. Zwraca `false`, gdy plan jest pełny albo
    /// slot nachodziłby na już wstawiony — **cisza zamiast paniki jest tu celowa**:
    /// „nie zmieściło się" to normalny wynik planowania, a nie błąd programu.
    pub(super) fn insert(&mut self, s: PlanSlot, at: PlaceRef) -> Option<usize> {
        if s.dur_min == 0 || u32::from(s.start_min) + u32::from(s.dur_min) > 1440 {
            return None;
        }
        if self.slots.is_full() {
            return None;
        }
        let koniec = s.end_min();
        let mut poz = self.slots.len();
        for (i, inny) in self.slots.iter().enumerate() {
            if inny.start_min < koniec && s.start_min < inny.end_min() {
                return None; // nakładanie
            }
            if inny.start_min >= koniec {
                poz = i;
                break;
            }
        }
        self.slots.insert(poz, s);
        self.places.insert(poz, at);
        Some(poz)
    }

    pub(super) fn remove(&mut self, i: usize) {
        self.slots.remove(i);
        self.places.remove(i);
    }

    /// Skąd mieszkaniec wyrusza w slocie `i`: z celu slotu poprzedniego albo z domu.
    pub(super) fn origin_of(&self, i: usize, home: PlaceRef) -> PlaceRef {
        if i == 0 {
            home
        } else {
            self.places[i - 1]
        }
    }

    /// Wolne przedziały doby od minuty `from`, w kolejności czasu.
    pub(super) fn gaps(&self, from: u16, out: &mut ArrayVec<Gap, 32>) {
        out.clear();
        let mut kursor = from;
        for s in self.slots.iter() {
            if s.end_min() <= kursor {
                continue;
            }
            if s.start_min > kursor {
                out.push(Gap {
                    start: kursor,
                    end: s.start_min,
                });
            }
            kursor = kursor.max(s.end_min());
        }
        if kursor < 1440 {
            out.push(Gap {
                start: kursor,
                end: 1440,
            });
        }
    }

    /// Gdzie mieszkaniec jest w minucie `m`: cel slotu trwającego w tej minucie albo
    /// ostatniego zakończonego przed nią. Brak takiego slotu = dom.
    pub(super) fn place_at(&self, m: u16, home: PlaceRef) -> PlaceRef {
        let mut gdzie = home;
        for (i, s) in self.slots.iter().enumerate() {
            if s.start_min > m {
                break;
            }
            // Slot dojścia kończy się tam, dokąd prowadzi — po nim mieszkaniec jest
            // w celu, a nie w punkcie startowym.
            gdzie = self.places[i];
        }
        gdzie
    }

    /// Slot zaczynający się dokładnie w minucie `m` (cel, do którego trzeba zdążyć).
    pub(super) fn slot_starting_at(&self, m: u16) -> Option<usize> {
        self.slots.iter().position(|s| s.start_min == m)
    }

    /// Gdzie mieszkaniec **musi być** w minucie `m`, żeby wykonać to, co się wtedy
    /// zaczyna.
    ///
    /// Dla dojazdu to jego **punkt wyjścia**, nie cel: `place_of` slotu dojazdu mówi,
    /// dokąd on prowadzi, a przed nim trzeba stać na jego początku. Pomylenie tych
    /// dwóch rzeczy kazało planerowi wysyłać mieszkańca po zakupy z powrotem do pracy,
    /// a potem liczyć dojazd do domu tak, jakby wychodził z domu — czas dojścia
    /// w planie przestawał się zgadzać z czasem policzonym przy wyruszeniu.
    pub(super) fn required_place_at(&self, m: u16, home: PlaceRef) -> Option<PlaceRef> {
        let j = self.slot_starting_at(m)?;
        if self.slots[j].kind == ActivityKind::Commute as u8 {
            Some(self.origin_of(j, home))
        } else {
            Some(self.places[j])
        }
    }

    pub(super) fn first_start(&self) -> Option<u16> {
        self.slots.first().map(|s| s.start_min)
    }
}

/// Statystyka planu — wejście do histogramu wykorzystania doby i do testów.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct PlanStats {
    pub slots: u8,
    pub commitments: u8,
    pub tasks_placed: u8,
    pub tasks_dropped: u8,
    pub travel_min: u16,
    pub free_min: u16,
}

pub(super) fn podsumuj(canvas: &DayCanvas, stats: &mut PlanStats) {
    stats.slots = canvas.len() as u8;
    stats.travel_min = canvas
        .slots()
        .iter()
        .filter(|s| s.kind == ActivityKind::Commute as u8)
        .map(|s| s.dur_min)
        .sum();
    let zajete: u32 = canvas.slots().iter().map(|s| u32::from(s.dur_min)).sum();
    stats.free_min = (1440 - zajete.min(1440)) as u16;
}

// ── wstawianie slotu ────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub(super) fn wstaw(
    canvas: &mut DayCanvas,
    log: &mut Log<'_>,
    kind: ActivityKind,
    start: u16,
    dur: u16,
    cel: PlaceRef,
    powod: DecisionReason,
    param: u8,
) -> bool {
    let tag = powod.discriminant();
    debug_assert!(
        tag <= 255,
        "PlanSlot.reason pakuje tag w bajt (M3a §5.1); powód {tag} tego nie mieści"
    );
    let slot = PlanSlot {
        start_min: start,
        dur_min: dur,
        target: knowledge_key(cel).unwrap_or(PlanSlot::NO_TARGET),
        kind: kind as u8,
        mode: if kind == ActivityKind::Commute {
            TransportMode::Walk as u8
        } else {
            u8::MAX
        },
        reason: PlanSlot::pack_reason(tag as u8, param),
    };
    let Some(i) = canvas.insert(slot, cel) else {
        return false;
    };
    log.shift_from(i as u8);
    log.add(i as u8, powod);
    true
}

// ── wydruk diagnostyczny ────────────────────────────────────────────────────────

/// Wydruk planu doby w formie tekstowej — **artefakt diagnostyczny, nie interfejs
/// gracza.** Identyfikatory są angielskie, bo to kod, a nie tekst do przetłumaczenia.
///
/// Player-facing oś czasu po polsku i po angielsku buduje M3d
/// (`engine::ui::inspect::timeline`) nad **tymi samymi** danymi: `DayCanvas` plus
/// `ReasonLog`. Nie ma tu drugiego planera ani drugiego źródła uzasadnień — jest drugi
/// formatter, z których jeden idzie do złotego testu i do runnera headless, a drugi
/// przez `LocKey` do karty inspekcji.
///
/// Powód renderuje się przez `Debug` `DecisionReason`, a nie przez własny `match`:
/// wydruk ma być stabilny i wyczerpujący, a wariant dołożony przez M4 ma się w nim
/// pojawić bez dopisywania czegokolwiek tutaj.
#[must_use]
pub fn render_day_debug(ctx: &PlanCtx<'_>, canvas: &DayCanvas, log: &ReasonLog) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(128 * (canvas.len() + 4));
    let _ = writeln!(
        s,
        "citizen {} | day {} | {:?} | age {}",
        ctx.citizen.id.entity().index(),
        ctx.day,
        ctx.dow,
        ctx.citizen.identity.age_years(ctx.citizen.today),
    );
    for (i, slot) in canvas.slots().iter().enumerate() {
        let cel = match canvas.place_of(i) {
            p if p == ctx.home => "home".to_string(),
            p => match knowledge_key(p) {
                Some(k) => format!("#{k}"),
                None => "-".to_string(),
            },
        };
        let powody: Vec<String> = log.for_slot(i).map(|e| format!("{:?}", e.reason)).collect();
        let powod = if powody.is_empty() {
            format!("tag={} param={}", slot.reason_tag(), slot.reason_param())
        } else {
            powody.join(" | ")
        };
        let _ = writeln!(
            s,
            "{}-{} {:<8} {:<8} {}",
            MinuteOfDay::new(slot.start_min),
            MinuteOfDay::new(slot.end_min() % 1440),
            ActivityKind::from_index(slot.kind as usize).map_or("?", ActivityKind::name),
            cel,
            powod,
        );
    }
    for e in log.skipped() {
        let _ = writeln!(s, "skipped: {:?}", e.reason);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luki_wypelniaja_dobe_bez_dziur_i_bez_nakladania() {
        let mut c = DayCanvas::new();
        let slot = |start: u16, dur: u16| PlanSlot {
            start_min: start,
            dur_min: dur,
            ..PlanSlot::default()
        };
        assert_eq!(c.insert(slot(480, 60), PlaceRef::default()), Some(0));
        assert_eq!(c.insert(slot(0, 400), PlaceRef::default()), Some(0));
        assert_eq!(
            c.insert(slot(390, 100), PlaceRef::default()),
            None,
            "nakładanie przeszło"
        );
        assert_eq!(
            c.insert(slot(1400, 100), PlaceRef::default()),
            None,
            "slot za północ"
        );

        let mut luki: ArrayVec<Gap, 32> = ArrayVec::new();
        c.gaps(0, &mut luki);
        assert_eq!(
            luki.as_slice(),
            &[
                Gap {
                    start: 400,
                    end: 480
                },
                Gap {
                    start: 540,
                    end: 1440
                }
            ]
        );
        let suma: u32 = c.slots().iter().map(|s| u32::from(s.dur_min)).sum::<u32>()
            + luki.iter().map(|g| u32::from(g.len())).sum::<u32>();
        assert_eq!(suma, 1440, "sloty plus luki nie składają się na dobę");
    }

    #[test]
    fn dojazd_wymaga_stania_na_swoim_poczatku_a_nie_w_celu() {
        // Doc-komentarz `required_place_at` nazywa przebyty błąd: pomylenie punktu
        // wyjścia dojazdu z jego celem kazało planerowi liczyć następny dojazd
        // z niewłaściwego miejsca. Faza 3 pyta o to dla **końca** każdej luki, więc
        // pomyłka jest niewidoczna w kompilacji i widoczna dopiero w czasie dojścia.
        let miejsce = |i: i32| PlaceRef::Coord(magnat_core::WorldCoord::new(i, 0, 0));
        let (dom, praca, sklep) = (miejsce(1), miejsce(2), miejsce(3));
        let slot = |start: u16, dur: u16, kind: ActivityKind| PlanSlot {
            start_min: start,
            dur_min: dur,
            kind: kind as u8,
            ..PlanSlot::default()
        };
        let mut c = DayCanvas::new();
        c.insert(slot(480, 20, ActivityKind::Commute), praca);
        c.insert(slot(500, 480, ActivityKind::Work), praca);
        c.insert(slot(980, 15, ActivityKind::Commute), sklep);

        assert_eq!(
            c.required_place_at(480, dom),
            Some(dom),
            "poranny dojazd zaczyna się w domu, nie w pracy"
        );
        assert_eq!(
            c.required_place_at(980, dom),
            Some(praca),
            "dojazd po pracy zaczyna się w pracy, nie w sklepie"
        );
        assert_eq!(
            c.required_place_at(500, dom),
            Some(praca),
            "na pracę trzeba być w pracy"
        );
        assert_eq!(
            c.required_place_at(600, dom),
            None,
            "nic się wtedy nie zaczyna"
        );
    }
}
