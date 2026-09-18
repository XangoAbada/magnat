//! Kronika: dziennik świata i gracza (M9e WP11, §5.11, PRD §14.3).
//!
//! # Kronika **czyta** dzienniki, a nie zbiera zgłoszenia
//!
//! §6 dokumentu fazy obiecywał `chronicle::record()` wołane przez „każdy system".
//! Tak się nie da i nie chodzi o wygodę: `game/` stoi **nad** wszystkimi `sim/*`,
//! więc żaden system symulacji nie może do niego sięgnąć — zależność szłaby
//! w drugą stronę i Cargo by tego nie zbudował.
//!
//! Wpisy i tak już istnieją, tylko w swoich crate'ach i w swoich formatach:
//! `magnat_events::Events::chronicle()` niesie zdarzenia świata, `Firm::log` —
//! trzydzieści dwie ostatnie decyzje firmy, dziennik wejść — to, co zrobił gracz.
//! Kronika **zbiera je raz na dobę** i zamienia na jeden typ. Jest więc widokiem
//! pochodnym, a nie drugim źródłem prawdy, i nie wchodzi do hasha stanu.
//!
//! # Skąd i skąd nie
//!
//! Zbieramy zdarzenia świata (wszystkie — jest ich rzędu setek na rok) oraz decyzje
//! **firm gracza** i jego własne komendy. Decyzji dwóch tysięcy firm AI nie zbieramy
//! i to jest `ponytail:` sufit nazwany: pierścień trzydziestu dwóch wpisów na firmę
//! to sześćdziesiąt cztery tysiące odczytów na dobę za treść, której nikt nie czyta.
//! Ścieżka wyjścia: kronika firmy otwiera się z jej karty i czyta jej pierścień
//! wprost, bez przepisywania go do siebie.
//!
//! # Tekst powstaje przy wyświetleniu
//!
//! W rekordzie siedzą **dane typowane**, nigdy gotowy napis. Inaczej kronika nie
//! dałaby się przetłumaczyć, a zmiana języka w trakcie gry pokazałaby historię
//! w dwóch językach naraz.

use magnat_core::{DecisionReason, DistrictId, FirmId, SimMinute, Subject};

use crate::Session;

/// Numer wpisu. Monotoniczny w obrębie gry i nigdy nie wraca.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ChronicleId(pub u32);

/// Rodzaj wpisu — po nim filtruje panel.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum ChronicleKind {
    /// Zdarzenie świata się zaczęło.
    EventStarted,
    /// Zdarzenie świata się skończyło.
    EventEnded,
    /// Decyzja firmy gracza.
    FirmDecision,
    /// Komenda gracza.
    PlayerAction,
}

impl ChronicleKind {
    pub const ALL: [ChronicleKind; 4] = [
        ChronicleKind::EventStarted,
        ChronicleKind::EventEnded,
        ChronicleKind::FirmDecision,
        ChronicleKind::PlayerAction,
    ];

    /// Klucz tekstu: `ui.chronicle.kind.<key>`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            ChronicleKind::EventStarted => "event_started",
            ChronicleKind::EventEnded => "event_ended",
            ChronicleKind::FirmDecision => "firm_decision",
            ChronicleKind::PlayerAction => "player_action",
        }
    }
}

/// Czyja to sprawa. Wpisy `Player` **nigdy nie są usuwane** przy decymacji.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChronicleScope {
    World,
    Player,
    Firm(FirmId),
    District(DistrictId),
}

/// Treść wpisu — **dane typowane, nigdy gotowy napis**.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ChroniclePayload {
    /// Zdarzenie świata: indeks definicji w katalogu i jego siła.
    Event { def: u16, severity_bps: u16 },
    /// Decyzja z powodem — renderuje ją `magnat_ui::describe`, ta sama funkcja,
    /// która rysuje kartę inspekcji.
    Reason(DecisionReason),
    /// Komenda gracza — klucz tekstu opisującego, co zrobił.
    Action { key: &'static str },
}

/// Jeden wpis kroniki.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ChronicleEntry {
    pub id: ChronicleId,
    pub at: SimMinute,
    pub kind: ChronicleKind,
    /// Podmiot, o którym wpis mówi — kliknięcie otwiera jego kartę. `None`, gdy
    /// wpis mówi o mieście.
    pub actor: Option<Subject>,
    pub payload: ChroniclePayload,
    /// 0..=100. Poniżej [`PROG_DECYMACJI`] wpis starszy niż pięć lat gry znika.
    pub importance: u8,
    pub scope: ChronicleScope,
}

/// Poniżej tej ważności wpis nie przeżywa pięciu lat gry (§5.11).
pub const PROG_DECYMACJI: u8 = 30;

/// Ile lat gry wpis o niskiej ważności przeżywa.
const LATA_DO_DECYMACJI: u64 = 5;

/// Czego szuka gracz w kronice.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Query {
    pub kind: Option<ChronicleKind>,
    /// Tylko wpisy o tym podmiocie.
    pub actor: Option<Subject>,
    /// Minimalna ważność.
    pub min_importance: u8,
    /// Zakres dób gry, `None` = bez ograniczenia.
    pub from_day: Option<u64>,
    pub to_day: Option<u64>,
}

/// Magazyn kroniki.
#[derive(Default)]
pub struct Chronicle {
    entries: Vec<ChronicleEntry>,
    next: u32,
    /// Ile wpisów kroniki zdarzeń już przepisano.
    seen_events: usize,
    /// Numer ostatniej przepisanej komendy gracza.
    seen_cmd: u64,
    /// Ostatni tick decyzji firmy, który już wszedł do kroniki.
    seen_firm_tick: u64,
    /// Ostatnia doba, w której zbierano.
    last_day: Option<u64>,
}

impl Chronicle {
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[must_use]
    pub fn entries(&self) -> &[ChronicleEntry] {
        &self.entries
    }

    #[must_use]
    pub fn get(&self, id: ChronicleId) -> Option<&ChronicleEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Zbiera nowe wpisy, jeśli minęła doba. Woła się z pętli gry co klatkę —
    /// sprawdzenie numeru doby jest tańsze niż osobny harmonogram.
    pub fn harvest(&mut self, session: &Session) {
        let doba = session.tick().get() / magnat_core::time::MINUTES_PER_DAY;
        if self.last_day == Some(doba) {
            return;
        }
        self.last_day = Some(doba);
        self.zbierz_zdarzenia(session);
        self.zbierz_decyzje_firm(session);
        self.zbierz_komendy(session);
        self.decimate(SimMinute(session.tick().get()));
    }

    fn push(&mut self, e: ChronicleEntry) {
        self.entries.push(e);
    }

    fn nowy_id(&mut self) -> ChronicleId {
        let id = ChronicleId(self.next);
        self.next = self.next.wrapping_add(1);
        id
    }

    /// Zdarzenia świata. Ważność wprost z siły zdarzenia — zdarzenie słabe jest
    /// mniej ważne i to jest cała reguła, bez tabeli wyjątków.
    fn zbierz_zdarzenia(&mut self, session: &Session) {
        let Some(ev) = session.app.world.get_resource::<magnat_events::Events>() else {
            return;
        };
        let wpisy: Vec<magnat_events::ChronicleEntry> =
            ev.chronicle().iter().skip(self.seen_events).copied().collect();
        self.seen_events += wpisy.len();
        for w in wpisy {
            let id = self.nowy_id();
            self.push(ChronicleEntry {
                id,
                at: SimMinute(w.at.get()),
                kind: if w.opened {
                    ChronicleKind::EventStarted
                } else {
                    ChronicleKind::EventEnded
                },
                actor: Some(Subject::Event(w.event)),
                payload: ChroniclePayload::Event {
                    def: w.def,
                    severity_bps: w.severity_bps,
                },
                importance: u8::try_from(w.severity_bps / 100).unwrap_or(100).min(100),
                scope: ChronicleScope::World,
            });
        }
    }

    /// Decyzje firm gracza z ich pierścieni. Bierze te, których tick jest nowszy
    /// od ostatnio widzianego — pierścień nie ma numerów, więc czas jest kursorem.
    fn zbierz_decyzje_firm(&mut self, session: &Session) {
        let Some(firms) = session.app.world.get_resource::<magnat_firms::Firms>() else {
            return;
        };
        let mut nowe: Vec<(u64, FirmId, DecisionReason)> = Vec::new();
        let mut najnowszy = self.seen_firm_tick;
        for (key, f) in firms.iter() {
            if !f
                .owners
                .iter()
                .any(|o| o.owner == magnat_firms::Owner::Player)
            {
                continue;
            }
            for d in f.log.iter() {
                if d.tick.get() > self.seen_firm_tick {
                    nowe.push((d.tick.get(), magnat_firms::firm_id(key), d.reason));
                    najnowszy = najnowszy.max(d.tick.get());
                }
            }
        }
        self.seen_firm_tick = najnowszy;
        // Kolejność po (tick, firma) — pierścienie czytają się w kolejności kluczy,
        // ale czasy się przeplatają i kronika ma być osią czasu, a nie listą firm.
        nowe.sort_unstable_by_key(|(t, f, _)| (*t, f.entity().index()));
        for (t, firma, powod) in nowe {
            let id = self.nowy_id();
            self.push(ChronicleEntry {
                id,
                at: SimMinute(t),
                kind: ChronicleKind::FirmDecision,
                actor: Some(Subject::Firm(firma)),
                payload: ChroniclePayload::Reason(powod),
                importance: 40,
                scope: ChronicleScope::Firm(firma),
            });
        }
    }

    /// Komendy gracza z dziennika wejść. To są wpisy, których decymacja nie rusza.
    fn zbierz_komendy(&mut self, session: &Session) {
        let nowe: Vec<(u64, u64, &'static str)> = session
            .log()
            .commands
            .iter()
            .filter(|e| e.seq >= self.seen_cmd)
            .map(|e| (e.seq, e.tick.get(), akcja_key(&e.cmd)))
            .collect();
        for (seq, tick, key) in nowe {
            self.seen_cmd = self.seen_cmd.max(seq + 1);
            let id = self.nowy_id();
            self.push(ChronicleEntry {
                id,
                at: SimMinute(tick),
                kind: ChronicleKind::PlayerAction,
                actor: session.player().map(|p| Subject::Citizen(p.citizen)),
                payload: ChroniclePayload::Action { key },
                importance: 80,
                scope: ChronicleScope::Player,
            });
        }
    }

    /// Wyrzuca wpisy o niskiej ważności starsze niż pięć lat gry.
    ///
    /// Wpisy gracza zostają **zawsze**, niezależnie od ważności i wieku: kronika
    /// dynastii jest treścią ekranu spuścizny, a skasowana historia własnej gry
    /// jest gorsza od pustej.
    pub fn decimate(&mut self, now: SimMinute) {
        let rok = 360 * magnat_core::time::MINUTES_PER_DAY;
        let granica = now.0.saturating_sub(LATA_DO_DECYMACJI * rok);
        self.entries.retain(|e| {
            e.scope == ChronicleScope::Player
                || e.importance >= PROG_DECYMACJI
                || e.at.0 >= granica
        });
    }

    /// Wpisy pasujące do zapytania, od najnowszego.
    #[must_use]
    pub fn query(&self, q: &Query) -> Vec<&ChronicleEntry> {
        let doba = |m: SimMinute| m.0 / magnat_core::time::MINUTES_PER_DAY;
        let mut out: Vec<&ChronicleEntry> = self
            .entries
            .iter()
            .filter(|e| q.kind.is_none_or(|k| k == e.kind))
            .filter(|e| q.actor.is_none_or(|a| Some(a) == e.actor))
            .filter(|e| e.importance >= q.min_importance)
            .filter(|e| q.from_day.is_none_or(|d| doba(e.at) >= d))
            .filter(|e| q.to_day.is_none_or(|d| doba(e.at) <= d))
            .collect();
        out.sort_unstable_by_key(|e| std::cmp::Reverse((e.at.0, e.id.0)));
        out
    }

    /// Ile razy zaszło zdarzenie tej definicji — wejście **osiągnięć emergentnych**
    /// (§5.11: „Twoja firma przetrwała 3 recesje" to zapytanie, nie osobny system).
    #[must_use]
    pub fn count_event(&self, def: u16) -> usize {
        self.entries
            .iter()
            .filter(|e| {
                e.kind == ChronicleKind::EventStarted
                    && matches!(e.payload, ChroniclePayload::Event { def: d, .. } if d == def)
            })
            .count()
    }
}

/// Klucz tekstu opisującego komendę gracza: `ui.chronicle.act.<key>`.
///
/// Wyczerpujący `match` z rozmysłu: komenda dopisana bez zdania w kronice byłaby
/// decyzją gracza, której jego własna historia nie pamięta.
const fn akcja_key(cmd: &crate::PlayerCommand) -> &'static str {
    use crate::PlayerCommand as C;
    match cmd {
        C::StartGame { .. } => "start",
        C::SetPrice { .. } => "set_price",
        C::SetCharacter { .. } => "set_character",
        C::SetAutonomy { .. } => "set_autonomy",
        C::AttachPolicy { .. } => "attach_policy",
        C::DetachPolicy { .. } => "detach_policy",
        C::FoundFirm { .. } => "found_firm",
        C::OpenSite { .. } => "open_site",
        C::CloseSite { .. } => "close_site",
        C::SetShelfAssortment { .. } => "set_assortment",
        C::HireCandidate { .. } => "hire",
        C::SetDelegationAutonomy { .. } => "set_delegation_autonomy",
        C::ApplyForJob { .. } => "apply_for_job",
        C::TakeLoan { .. } => "take_loan",
        C::BackCandidate { .. } => "back_candidate",
        C::ApplyForPermit { .. } => "apply_for_permit",
        C::SetRestockTarget { .. } => "set_restock",
        C::DeclarePersonalBankruptcy => "bankruptcy",
        C::SetHeir { .. } => "set_heir",
        C::Succeed { .. } => "succeed",
        C::ContinueAsNewCitizen { .. } => "new_dynasty",
    }
}

/// Zdanie wpisu w języku gracza.
///
/// Trzy źródła tekstu, po jednym na rodzaj ładunku: katalog zdarzeń niesie własny
/// klucz (`EventDef::chronicle`), powód decyzji renderuje `magnat_ui::describe`,
/// a komenda gracza ma klucz z [`akcja_key`]. Żaden napis nie powstaje tutaj.
#[must_use]
pub fn text(
    e: &ChronicleEntry,
    session: &Session,
    c: &magnat_ui::Catalog,
    l: magnat_ui::Locale,
) -> String {
    match e.payload {
        ChroniclePayload::Event { def, .. } => session
            .app
            .world
            .get_resource::<magnat_events::Events>()
            .and_then(|ev| ev.def(def).map(|d| d.chronicle.clone()))
            .map_or_else(
                || c.fmt_key(l, "ui.chronicle.unknown", &[]),
                |k| c.fmt_key(l, &k, &[]),
            ),
        ChroniclePayload::Reason(r) => magnat_ui::describe(c, l, r),
        ChroniclePayload::Action { key } => {
            c.fmt_key(l, &format!("ui.chronicle.act.{key}"), &[])
        }
    }
}
