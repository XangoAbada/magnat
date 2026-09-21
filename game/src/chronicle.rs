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
    /// Decyzja władzy miasta: uchwała, wybory, zmiana stawki.
    CityDecision,
    /// Wpis z historii „na sucho" — z lat sprzed startu partii.
    History,
}

impl ChronicleKind {
    pub const ALL: [ChronicleKind; 6] = [
        ChronicleKind::EventStarted,
        ChronicleKind::EventEnded,
        ChronicleKind::FirmDecision,
        ChronicleKind::PlayerAction,
        ChronicleKind::CityDecision,
        ChronicleKind::History,
    ];

    /// Klucz tekstu: `ui.chronicle.kind.<key>`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            ChronicleKind::EventStarted => "event_started",
            ChronicleKind::EventEnded => "event_ended",
            ChronicleKind::FirmDecision => "firm_decision",
            ChronicleKind::PlayerAction => "player_action",
            ChronicleKind::CityDecision => "city_decision",
            ChronicleKind::History => "history",
        }
    }
}

/// Skąd wpis pochodzi.
///
/// Wykonanie wymagania M10 §6 pkt 7 („filtr `provenance: DryRun` i oś czasu
/// sprzed startu partii") i mitygacja ryzyka `R9`: osiemdziesiąt lat historii
/// wchodzi do tej samej kroniki co rozgrywka, więc gracz musi umieć jedno
/// od drugiego oddzielić. Most jest jednokierunkowy (`DK-2`) — kronika dokłada
/// pochodzenie wpisowi z `sim/macro`, a `sim/macro` o kronice gracza nie wie.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Provenance {
    /// Zaszło w tej rozgrywce.
    #[default]
    Live,
    /// Policzone w historii „na sucho" przed startem partii.
    DryRun,
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
    /// Wpis z historii „na sucho": klucz zdania z `magnat_macro::ChronicleKind`,
    /// **rok świata** i skala w promilach.
    ///
    /// Rok siedzi w ładunku, a nie w `at`, bo `SimMinute` jest bez znaku i nie
    /// umie liczyć wstecz od zera świata — a historia dzieje się właśnie przed
    /// nim. Skala jest liczbą porządkową (ile ludności straciła dzielnica),
    /// nigdy pieniężną.
    History {
        key: &'static str,
        year: i32,
        magnitude: i32,
    },
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
    /// Rozgrywka czy historia sprzed niej. Decymacja rusza wyłącznie `Live`.
    pub provenance: Provenance,
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
    /// Tylko wpisy o tym pochodzeniu. `None` = jedno i drugie.
    pub provenance: Option<Provenance>,
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
    /// Ostatnia minuta publikacji, którą kronika już zebrała z dziennika redakcji.
    seen_story_tick: u64,
    /// Ostatnia minuta decyzji władzy, którą kronika już zebrała.
    seen_city_tick: u64,
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
        self.zbierz_publikacje(session);
        self.zbierz_wladze(session);
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
        let wpisy: Vec<magnat_events::ChronicleEntry> = ev
            .chronicle()
            .iter()
            .skip(self.seen_events)
            .copied()
            .collect();
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
                provenance: Provenance::Live,
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
                provenance: Provenance::Live,
            });
        }
    }

    /// Publikacje tytułów medialnych (M10b).
    ///
    /// Wykonanie `DK-1`: zdarzenie `sim/*` zapisuje powód **u siebie**, a kronika
    /// gracza dokłada dla niego źródło. `game/` stoi nad wszystkimi `sim/*`, więc
    /// zgłoszenie w drugą stronę zamknęłoby cykl, którego Cargo nie zbuduje.
    ///
    /// Aktorem jest **tytuł**, a nie zdarzenie, i to jest różnica warta zapisania:
    /// wpis odpowiada na pytanie „kto o tym napisał", a nie „co się stało" — to
    /// drugie ma już własny wiersz z `zbierz_zdarzenia`.
    fn zbierz_publikacje(&mut self, session: &Session) {
        let Some(o) = session.app.world.get_resource::<magnat_media::Outlets>() else {
            return;
        };
        let nowe: Vec<(u64, DecisionReason)> = o
            .reasons()
            .iter()
            .filter(|(t, _)| t.get() > self.seen_story_tick)
            .map(|(t, r)| (t.get(), *r))
            .collect();
        for (t, _) in &nowe {
            self.seen_story_tick = self.seen_story_tick.max(*t);
        }
        for (t, powod) in nowe {
            let tytul = match powod {
                DecisionReason::StoryPublished { outlet, .. } => {
                    Some(Subject::Firm(magnat_supply::firm_of(outlet)))
                }
                _ => None,
            };
            let id = self.nowy_id();
            self.push(ChronicleEntry {
                id,
                at: SimMinute(t),
                kind: ChronicleKind::FirmDecision,
                actor: tytul,
                payload: ChroniclePayload::Reason(powod),
                // Publikacja waży mniej niż decyzja firmy gracza i więcej niż nic:
                // tekst o strajku jest tłem, dopóki nie dotyczy jego zakładu.
                importance: 25,
                scope: ChronicleScope::World,
                provenance: Provenance::Live,
            });
        }
    }

    /// Decyzje władzy miasta (M8e): uchwały rady, wybory, zmiany stawek.
    ///
    /// To są wpisy, które widzi **każdy** gracz niezależnie od tego, co posiada —
    /// podwyżka VAT-u zmienia cenę na półce w całym mieście, a wynik wyborów
    /// zmienia to, czego można się spodziewać po radzie. Dlatego czytamy je,
    /// choć decyzji dwóch tysięcy firm AI nie czytamy: tamtych jest sześćdziesiąt
    /// cztery tysiące na dobę i dotyczą cudzych sklepów, tych jest kilka na
    /// miesiąc i dotyczą wszystkich.
    ///
    /// Źródłem jest pierścień `City.reasons` (256 wpisów) — zapisuje go
    /// `sim/city`, tak samo jak redakcja zapisuje swój (`DK-1`).
    fn zbierz_wladze(&mut self, session: &Session) {
        let Some(c) = session.app.world.get_resource::<magnat_city::City>() else {
            return;
        };
        let nowe: Vec<(u64, DecisionReason)> = c
            .reasons
            .iter()
            .filter(|(t, _)| t.get() > self.seen_city_tick)
            .map(|(t, r)| (t.get(), *r))
            .collect();
        for (t, _) in &nowe {
            self.seen_city_tick = self.seen_city_tick.max(*t);
        }
        for (t, powod) in nowe {
            let id = self.nowy_id();
            self.push(ChronicleEntry {
                id,
                at: SimMinute(t),
                kind: ChronicleKind::CityDecision,
                actor: Some(Subject::Government),
                payload: ChroniclePayload::Reason(powod),
                // Wyżej niż publikacja i niżej niż własna decyzja: uchwała rady
                // dotyczy gracza zawsze, ale nie jest jego wyborem.
                importance: 55,
                scope: ChronicleScope::World,
                provenance: Provenance::Live,
            });
        }
    }

    /// Wciąga historię „na sucho" — lata policzone w makro przed startem partii.
    ///
    /// Wołane **raz**, przy zakładaniu sesji, i to jest cała droga: `sim/macro`
    /// nie zna kroniki gracza i nie ma jak jej zgłosić wpisu (`DK-1`), więc to
    /// sesja przychodzi po gotowy `DryRunResult.chronicle`.
    ///
    /// Wpisy dostają `at = 0`, bo minuta świata zaczyna się od zera i nie umie
    /// liczyć wstecz; rok niosą **w ładunku**. Filtr po pochodzeniu jest tym,
    /// co oddziela osiemdziesiąt lat historii od pierwszej doby rozgrywki.
    /// Ile ich jest, rozstrzyga próg skali po stronie `sim/macro` — kronika
    /// nie filtruje drugi raz (ryzyko `R9`: cel to 50–200 wpisów na 80 lat).
    pub fn ingest_dry_run(&mut self, events: &[magnat_macro::ChronicleEvent]) {
        for e in events {
            let id = self.nowy_id();
            self.push(ChronicleEntry {
                id,
                at: SimMinute(0),
                kind: ChronicleKind::History,
                actor: e.district.map(Subject::District),
                payload: ChroniclePayload::History {
                    key: e.kind.key(),
                    year: e.year,
                    magnitude: e.magnitude,
                },
                // Skala jest w promilach, ważność w setnych — dzielenie przez
                // dziesięć jest całą regułą, tak samo jak przy sile zdarzenia.
                importance: u8::try_from(e.magnitude.unsigned_abs() / 10)
                    .unwrap_or(100)
                    .min(100),
                scope: e.district.map_or(ChronicleScope::World, ChronicleScope::District),
                provenance: Provenance::DryRun,
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
                provenance: Provenance::Live,
            });
        }
    }

    /// Wyrzuca wpisy o niskiej ważności starsze niż pięć lat gry.
    ///
    /// Wpisy gracza zostają **zawsze**, niezależnie od ważności i wieku: kronika
    /// dynastii jest treścią ekranu spuścizny, a skasowana historia własnej gry
    /// jest gorsza od pustej.
    ///
    /// Wpisy z historii „na sucho" zostają z tego samego powodu i z jednego
    /// dodatkowego: mają `at = 0`, więc kryterium wieku skasowałoby je co do
    /// jednego w piątym roku gry — a „dlaczego ta dzielnica jest taka, jaka
    /// jest" jest pytaniem, które gracz zadaje **później**, nie wcześniej.
    /// Ich liczbę ogranicza próg skali po stronie `sim/macro`, nie decymacja.
    pub fn decimate(&mut self, now: SimMinute) {
        let rok = 360 * magnat_core::time::MINUTES_PER_DAY;
        let granica = now.0.saturating_sub(LATA_DO_DECYMACJI * rok);
        self.entries.retain(|e| {
            e.scope == ChronicleScope::Player
                || e.provenance == Provenance::DryRun
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
            .filter(|e| q.provenance.is_none_or(|p| p == e.provenance))
            // Zakres dób dotyczy rozgrywki. Wpis z historii ma `at = 0` i swój
            // rok w ładunku, więc pytanie „co się działo w dobach 100–200"
            // nie ma dla niego sensu — odsiewa go zakres, a nie brak filtra.
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
        C::OpenCampaign { .. } => "open_campaign",
        C::StartResearch { .. } => "start_research",
        C::PlaceStockOrder { .. } => "place_stock_order",
        C::AnswerUnion { .. } => "answer_union",
        C::GoPublic { .. } => "go_public",
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
        ChroniclePayload::Action { key } => c.fmt_key(l, &format!("ui.chronicle.act.{key}"), &[]),
        // Dwa klucze, bo połowa wpisów historii nie ma skali: wstrząs miejski
        // i runda naprawcza dotyczą całego miasta i nie mierzą się w promilach
        // niczego. „(0 ‰)" przy każdym z nich było szumem, który audyt WP10.15
        // wyłapał od razu — i to jest dokładnie to, po co ten audyt jest.
        ChroniclePayload::History {
            key,
            year,
            magnitude: 0,
        } => c.fmt_key(
            l,
            "ui.chronicle.history_row_plain",
            &[("rok", &year.to_string()), ("co", &c.fmt_key(l, key, &[]))],
        ),
        ChroniclePayload::History {
            key,
            year,
            magnitude,
        } => c.fmt_key(
            l,
            "ui.chronicle.history_row",
            &[
                ("rok", &year.to_string()),
                ("co", &c.fmt_key(l, key, &[])),
                ("skala", &magnitude.abs().to_string()),
            ],
        ),
    }
}
