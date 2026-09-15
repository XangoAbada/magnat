//! Oś czasu dnia: plan obok realizacji (M3d §5.11, PRD §5.5 i §14.4).
//!
//! **Jedno źródło etykiet.** Widget graficzny i wydruk tekstowy wołają te same funkcje
//! (`wiersz_planu`, `reason::describe`), więc złoty test w CI faktycznie broni tego,
//! co widzi gracz, a nie osobnej ścieżki (korekta E-8). Diagnostyczny
//! `planner::render_day_debug` z M3b zostaje osobno — jest po angielsku i bez
//! lokalizacji, bo nie jest interfejsem gracza.
//!
//! Plan bierze się z `DayCanvas` + `ReasonLog` (odtwarzanych na żądanie przez
//! `plan_day_explained`), realizacja — z bufora śledzenia zdarzeń DES (decyzja 9.16:
//! 64 wpisy dla najwyżej ośmiu obserwowanych mieszkańców).

use crate::inspect::reason::{self, zegar};
use crate::loc::{Catalog, Locale};
use magnat_agents::{DayCanvas, PlanSlot, ReasonLog, TraceEntry};
use magnat_core::{ActivityKind, DecisionReason};

/// Od ilu minut rozjazdu plan i realizacja są **podświetlane** jako rozbieżne (§5.11).
pub const DRIFT_HIGHLIGHT_MIN: i32 = 10;

/// Blok realizacji: co mieszkaniec faktycznie robił i kiedy.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ActualBlock {
    pub start_min: u16,
    pub end_min: u16,
    /// `ActivityKind`.
    pub kind: u8,
    /// Slot planu, z którego blok wyrósł.
    pub slot: u8,
}

/// Odtwarza ścieżkę „realizacja" z bufora śledzenia.
///
/// `StartActivity` otwiera blok, `EndActivity` albo `Arrive` go zamyka; blok bez
/// zamknięcia trwa do końca doby, bo mieszkaniec nadal go wykonuje.
#[must_use]
pub fn actual_from_trace(trace: &[TraceEntry]) -> Vec<ActualBlock> {
    let mut out: Vec<ActualBlock> = Vec::new();
    for e in trace {
        let minuta = (e.minute % 1440) as u16;
        if e.is_start() {
            if let Some(ostatni) = out.last_mut() {
                if ostatni.end_min == u16::MAX {
                    ostatni.end_min = minuta;
                }
            }
            out.push(ActualBlock {
                start_min: minuta,
                end_min: u16::MAX,
                kind: e.activity,
                slot: e.slot,
            });
        } else if let Some(ostatni) = out.last_mut() {
            if ostatni.slot == e.slot && ostatni.end_min == u16::MAX {
                ostatni.end_min = minuta;
            }
        }
    }
    for b in &mut out {
        if b.end_min == u16::MAX {
            b.end_min = 1440;
        }
    }
    out
}

/// Nagłówek karty. Imię i adres przychodzą **gotowe**: nazwy własne pochodzą
/// z `data/names/` i nie są lokalizacją UI (CLAUDE.md), a adres zna tylko ten, kto
/// widzi miasto.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct CitizenHeader {
    pub name: String,
    pub age_years: u32,
    pub occupation: String,
    pub address: String,
}

/// Jeden wiersz osi czasu — to, co rysuje widget i co wypisuje wydruk.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TimelineRow {
    pub slot: usize,
    pub start_min: u16,
    pub end_min: u16,
    pub kind: ActivityKind,
    /// Nazwa czynności w języku gracza.
    pub label: String,
    /// Pełne uzasadnienie wstawienia slotu.
    pub reason: String,
    /// Odrzucone alternatywy — PRD §5.5 chce ich w karcie razem z wyborem.
    pub alternatives: Vec<String>,
    /// Rozjazd planu z realizacją w minutach; dodatni = później niż w planie.
    pub drift_min: i32,
}

impl TimelineRow {
    /// Czy rozjazd jest na tyle duży, żeby go podświetlić (§5.11).
    #[must_use]
    pub fn drifted(&self) -> bool {
        self.drift_min.abs() >= DRIFT_HIGHLIGHT_MIN
    }
}

/// Oś czasu jednego dnia jednego mieszkańca.
///
/// **Dwa plany, nie jeden** (`N-9`). `canvas` jest planem **odtworzonym z ziarna**
/// przez `plan_day_explained` — tylko on niesie uzasadnienia, bo pełnych logów decyzji
/// nikt nie przechowuje. `stored` jest planem **zapisanym**, czyli tym po wszystkich
/// przeplanowaniach — i to w niego indeksują numery slotów z bufora śledzenia.
/// Karta pokazuje `stored`, bo to on rządził dniem; uzasadnienia dobiera do niego
/// z `canvas`, a nie odwrotnie.
pub struct DayTimeline<'a> {
    pub canvas: &'a DayCanvas,
    pub log: &'a ReasonLog,
    /// Pusty, gdy mieszkaniec nie jest śledzony — wtedy karta pokazuje sam plan.
    pub actual: &'a [ActualBlock],
    /// Plan zapisany w slabie. Pusty = nie ma go (plan z innej doby albo świat bez
    /// slabu) i wtedy oś pokazuje plan odtworzony, jak przed `N-9`.
    pub stored: &'a [PlanSlot],
}

impl DayTimeline<'_> {
    /// Plan, który karta pokazuje: zapisany, gdy jest, odtworzony w przeciwnym razie.
    #[must_use]
    pub fn plan(&self) -> &[PlanSlot] {
        if self.stored.is_empty() {
            self.canvas.slots()
        } else {
            self.stored
        }
    }

    /// Slot planu **odtworzonego**, z którego biorą się uzasadnienia dla `i`-tego
    /// slotu planu pokazywanego. `None` = tego slotu w odtworzonym planie nie ma.
    ///
    /// Przeplanowanie przyrostowe zachowuje wszystko, co zaczęło się przed jego
    /// minutą, więc prefiks obu planów jest identyczny i trafia w pierwszą gałąź.
    /// Ogon dopisany przez przeplanowanie w odtworzonym planie **nie istnieje** —
    /// wtedy lepiej nie powiedzieć nic, niż podstawić uzasadnienie cudzego slotu.
    fn reason_slot(&self, i: usize, s: &PlanSlot) -> Option<usize> {
        if self.stored.is_empty() {
            return Some(i);
        }
        let sloty = self.canvas.slots();
        let ten_sam = |k: &PlanSlot| k.kind == s.kind && k.start_min == s.start_min;
        if sloty.get(i).is_some_and(ten_sam) {
            return Some(i);
        }
        if let Some(j) = sloty.iter().position(ten_sam) {
            return Some(j);
        }
        // Ta sama czynność w tym samym miejscu, przesunięta w czasie: powód („kupić
        // chleb, bo zapas na dobę") jest nadal ten sam, zmieniła się tylko godzina.
        if s.target == PlanSlot::NO_TARGET {
            return None;
        }
        sloty
            .iter()
            .position(|k| k.kind == s.kind && k.target == s.target)
    }

    /// Wiersze planu z uzasadnieniami i rozjazdem wobec realizacji.
    #[must_use]
    pub fn rows(&self, c: &Catalog, l: Locale) -> Vec<TimelineRow> {
        self.plan()
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let mut powody = self
                    .reason_slot(i, s)
                    .map(|j| self.log.for_slot(j))
                    .into_iter()
                    .flatten()
                    .map(|e| e.reason);
                let pierwszy = powody.next().unwrap_or(DecisionReason::Unspecified);
                let kind = ActivityKind::from_index(s.kind as usize).unwrap_or(ActivityKind::Idle);
                TimelineRow {
                    slot: i,
                    start_min: s.start_min,
                    end_min: s.end_min(),
                    kind,
                    label: reason::activity(c, l, kind),
                    reason: reason::describe(c, l, pierwszy),
                    alternatives: powody.map(|r| reason::describe(c, l, r)).collect(),
                    drift_min: self.drift(i as u8),
                }
            })
            .collect()
    }

    /// Rozjazd startu slotu wobec realizacji, w minutach. `0`, gdy nie ma czego porównać.
    ///
    /// **Numer slotu jest tożsamością czynności tylko w planie zapisanym** (`N-9`).
    /// Bufor śledzenia niesie numery slotów tego planu, więc porównujemy z nim —
    /// wcześniej karta porównywała z planem **odtworzonym z ziarna** i po każdym
    /// przeplanowaniu przypisywała wieczorne przybycie do porannego slotu, wypisując
    /// kilkugodzinne „spóźnienie", którego nie było.
    ///
    /// Numery pozostają prawdziwe także po przeplanowaniu: przeplanowanie przyrostowe
    /// zachowuje wszystkie sloty rozpoczęte przed jego minutą, a przepisuje wyłącznie
    /// przyszłość — czyli to, czego bufor jeszcze nie dotknął.
    ///
    /// Strażnik zgodności rodzaju czynności z M4b zostaje: świat bez zapisanego planu
    /// (`stored` puste) nadal porównuje z planem odtworzonym i tam pomyłka jest możliwa.
    #[must_use]
    pub fn drift(&self, slot: u8) -> i32 {
        let Some(s) = self.plan().get(slot as usize) else {
            return 0;
        };
        let Some(b) = self
            .actual
            .iter()
            .find(|b| b.slot == slot && b.kind == s.kind)
        else {
            return 0;
        };
        i32::from(b.start_min) - i32::from(s.start_min)
    }

    /// Decyzje, które **nie** wytworzyły slotu: pominięte zadania, brak wiedzy, absencja.
    /// PRD §5.5 chce w karcie obu rzeczy — i tego, co mieszkaniec zrobił, i tego,
    /// czego nie zrobił (korekta E-11).
    #[must_use]
    pub fn skipped(&self, c: &Catalog, l: Locale) -> Vec<String> {
        self.log
            .skipped()
            .map(|e| reason::describe(c, l, e.reason))
            .collect()
    }
}

/// Wydruk dnia dla gracza w układzie z PRD §5.5.
///
/// To jest ten sam tekst, który widzi gracz w karcie inspekcji — ta sama funkcja
/// buduje etykiety i uzasadnienia. Złoty test w CI porównuje wynik bajt w bajt.
#[must_use]
pub fn render_day_text(
    c: &Catalog,
    l: Locale,
    header: &CitizenHeader,
    timeline: &DayTimeline<'_>,
) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(128 * (timeline.canvas.len() + 6));

    let _ = writeln!(
        s,
        "{}",
        c.fmt_key(
            l,
            "ui.card.header",
            &[
                ("imie", &header.name),
                ("wiek", &reason::years(c, l, header.age_years)),
                ("zawod", &header.occupation),
                ("adres", &header.address),
            ],
        )
    );

    for r in timeline.rows(c, l) {
        let _ = write!(
            s,
            "{}–{}  {:<12} ({})",
            zegar(r.start_min),
            zegar(r.end_min % 1440),
            r.label,
            r.reason
        );
        for a in &r.alternatives {
            let _ = write!(s, " · {a}");
        }
        if r.drifted() {
            let _ = write!(
                s,
                " · {}",
                c.fmt_key(
                    l,
                    "ui.reason.Arrived",
                    &[
                        ("faktycznie", &zegar(
                            (i32::from(r.start_min) + r.drift_min).clamp(0, 1439) as u16
                        )),
                        ("plan", &zegar(r.start_min)),
                    ],
                )
            );
        }
        let _ = writeln!(s);
    }

    let pominiete = timeline.skipped(c, l);
    if !pominiete.is_empty() {
        let _ = writeln!(s, "{}:", c.fmt_key(l, "ui.card.skipped", &[]));
        for p in pominiete {
            let _ = writeln!(s, "  {p}");
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_agents::EventKind;

    fn wpis(minute: u32, kind: EventKind, slot: u8, activity: u8) -> TraceEntry {
        TraceEntry {
            minute,
            kind: kind as u8,
            slot,
            activity,
        }
    }

    #[test]
    fn realizacja_sklada_sie_z_par_start_koniec() {
        let t = vec![
            wpis(400, EventKind::StartActivity, 0, ActivityKind::Sleep as u8),
            wpis(410, EventKind::EndActivity, 0, ActivityKind::Sleep as u8),
            wpis(410, EventKind::StartActivity, 1, ActivityKind::Eat as u8),
            wpis(440, EventKind::EndActivity, 1, ActivityKind::Eat as u8),
            // Ostatni blok bez zamknięcia — mieszkaniec nadal go wykonuje.
            wpis(500, EventKind::StartActivity, 2, ActivityKind::Work as u8),
        ];
        let b = actual_from_trace(&t);
        assert_eq!(b.len(), 3);
        assert_eq!((b[0].start_min, b[0].end_min), (400, 410));
        assert_eq!((b[1].start_min, b[1].end_min), (410, 440));
        assert_eq!((b[2].start_min, b[2].end_min), (500, 1440));
    }

    fn slot(start_min: u16, dur_min: u16, kind: ActivityKind, target: u32) -> magnat_agents::PlanSlot {
        magnat_agents::PlanSlot {
            start_min,
            dur_min,
            target,
            kind: kind as u8,
            mode: u8::MAX,
            reason: 0,
        }
    }

    /// `N-9`: przeplanowanie przyrostowe wyrzuca poranne zakupy, więc wieczorne
    /// przesuwają się w planie **zapisanym** na numer, pod którym w planie
    /// **odtworzonym z ziarna** stoją poranne. Dopasowanie po numerze slotu wypisywało
    /// wtedy dziesięciogodzinne „spóźnienie", którego nie było — i to jest dokładnie
    /// ten przypadek, którego strażnik rodzaju czynności z M4b nie łapie, bo rodzaj
    /// się zgadza.
    #[test]
    fn przeplanowanie_nie_wymysla_spoznienia() {
        let odtworzony = [
            slot(0, 360, ActivityKind::Sleep, 0),
            slot(420, 30, ActivityKind::Shop, 11),
            slot(1020, 30, ActivityKind::Shop, 11),
        ];
        // Po przeplanowaniu zostały tylko wieczorne zakupy — pod numerem 1.
        let zapisany = [
            slot(0, 360, ActivityKind::Sleep, 0),
            slot(1020, 30, ActivityKind::Shop, 11),
        ];
        let realizacja = [ActualBlock {
            start_min: 1020,
            end_min: 1050,
            kind: ActivityKind::Shop as u8,
            slot: 1,
        }];

        let mut canvas = DayCanvas::new();
        canvas.load(&odtworzony);
        let log = ReasonLog::new();

        // Tak wyglądała karta przed naprawą: porównanie z planem odtworzonym.
        let przed = DayTimeline {
            canvas: &canvas,
            log: &log,
            actual: &realizacja,
            stored: &[],
        };
        assert_eq!(
            przed.drift(1),
            600,
            "scena nie odtwarza błędu, którego dotyczy N-9"
        );

        let po = DayTimeline {
            canvas: &canvas,
            log: &log,
            actual: &realizacja,
            stored: &zapisany,
        };
        assert_eq!(po.drift(1), 0, "karta nadal wymyśla spóźnienie");
        let c = Catalog::load().expect("data/locale/");
        let rows = po.rows(&c, Locale::Pl);
        assert_eq!(rows.len(), 2, "karta pokazuje plan zapisany, nie odtworzony");
        assert!(!rows[1].drifted());
        assert_eq!(rows[1].start_min, 1020);
    }

    #[test]
    fn rozjazd_ponizej_progu_nie_jest_podswietlany() {
        let r = TimelineRow {
            slot: 0,
            start_min: 480,
            end_min: 540,
            kind: ActivityKind::Work,
            label: String::new(),
            reason: String::new(),
            alternatives: Vec::new(),
            drift_min: 9,
        };
        assert!(!r.drifted());
        let r = TimelineRow {
            drift_min: -10,
            ..r
        };
        assert!(r.drifted());
    }
}
