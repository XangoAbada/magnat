//! Komendy gracza — wejścia replayu (M9a §5.5, PRD §18.2).
//!
//! # Dwa strumienie, bo to rozstrzyga determinizm
//!
//! - [`PlayerCommand`] — **autorytatywne**: wpływają na stan symulacji, wchodzą
//!   do hasha, są odtwarzane przy replayu.
//! - [`ViewCommand`] — **nieautorytatywne** (kamera, panele, skala czasu,
//!   sortowanie tabeli). Logowane osobno, z rzeczywistym znacznikiem czasu —
//!   do odtworzenia *widoku* w zgłoszeniu błędu i do pomiaru metryk §20.3.
//!
//! Pauza i skala czasu są w `ViewCommand` świadomie: symulacja daje ten sam wynik
//! niezależnie od tego, kiedy gracz ją zatrzymał — zatrzymanie zmienia tylko to,
//! na którym ticku przestały napływać komendy, a to jest już zapisane w polu
//! `tick` każdej koperty.
//!
//! # Ile wariantów i dlaczego tyle
//!
//! Plan fazy wypisuje ~70 wariantów `PlayerCommand` (M9a §5.5) i ta lista zostaje
//! **projektem** — ale wariant wchodzi tutaj razem ze swoim wykonawcą, a nie
//! przed nim. Powód jest ten sam, dla którego `PolicyKind` ma dziewięć wariantów,
//! a nie jedenaście (`K-67`): **wariant, którego skutku nikt nie widzi, przechodzi
//! każdy test i wygląda dokładnie tak samo jak działający.** Siedemdziesiąt
//! wariantów z ramieniem „to jeszcze nie istnieje" byłoby siedemdziesięcioma
//! `TODO` w typie, a `K-18` pkt 4 zabrania `TODO` w kodzie.
//!
//! Dopisywać wolno **wyłącznie na końcu** enuma — tak samo jak `TxKind` (`K-66`)
//! i z tego samego powodu: dziennik wejść jest artefaktem zapisu gry, a kolejność
//! wariantów jest w nim liczbą.

pub(crate) mod check;
pub(crate) mod error;
pub(crate) mod exec;

pub use check::precheck;
pub use error::CommandError;

use magnat_core::{CitizenId, GoodId, Money, SiteId, Tick};
use magnat_economy::Market;
use serde::{Deserialize, Serialize};

use crate::shell::{ScenarioId, StartVariant};

/// Kto wydał komendę. `0` w grze jednoosobowej; miejsce na lockstep (§18.2).
///
/// Zostaje mimo braku multiplayera — dwa bajty teraz są tańsze niż migracja
/// formatu dziennika później (decyzja otwarta nr 10 fazy, przyjęta domyślnie).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default, Serialize, Deserialize)]
pub struct PlayerId(pub u16);

/// Koperta komendy. Kolejność stosowania: wszystkie koperty dla ticku `T`
/// posortowane po `(actor, seq)` i stosowane w punkcie synchronizacji **przed**
/// systemami (00 §3.4).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct CommandEnvelope {
    /// Monotoniczny numer w sesji.
    pub seq: u64,
    /// Tick, na którego **początku** komenda jest stosowana.
    pub tick: Tick,
    pub actor: PlayerId,
    pub cmd: PlayerCommand,
}

/// Komenda zmieniająca stan świata.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum PlayerCommand {
    /// Założenie gry. **Komplet parametrów świata, nie samo ziarno**: rozmiar,
    /// region, epoka, profil i trudność zmieniają wygenerowany świat tak samo jak
    /// ziarno, więc replay bez nich odtworzyłby inne miasto (`Z-3`).
    ///
    /// `pick` to wybrany mieszkaniec; `None` do czasu ekranu wyboru postaci
    /// (WP4, `M9c`) — świat wtedy stoi, ale gracz nie ma jeszcze ciała.
    StartGame {
        world: magnat_world::WorldGenParams,
        scenario: ScenarioId,
        variant: StartVariant,
        pick: Option<CitizenId>,
    },
    /// Cena towaru na półce, ustawiona ręcznie.
    ///
    /// Towar jedzie **kluczem tekstowym, nie indeksem** — `GoodId` nadaje się przy
    /// ładowaniu katalogu i przesuwa się przy każdym nowym towarze, a w zapisie gry
    /// trzymamy klucz (00 §5).
    ///
    /// Ustawienie ceny przestawia zarazem politykę cenową na `Fixed`. Bez tego
    /// najbliższa przecena nadpisałaby cenę gracza tego samego dnia, a gracz
    /// zobaczyłby, że jego decyzja nie ma skutku — i miałby rację.
    SetPrice {
        site: SiteId,
        good: String,
        price: Money,
    },
    /// Wybór postaci gracza (WP4). Osobna komenda, a nie pole `StartGame`, bo lista
    /// kandydatów powstaje **z postawionego świata** — w chwili, gdy koperta startowa
    /// już jest w dzienniku. Replay odtwarza wybór z tej komendy, nie z listy.
    SetCharacter { citizen: CitizenId },
    /// Które decyzje mieszkańca gracz przejmuje, a które zostają autopilotowi M3.
    SetAutonomy {
        field: crate::player::AutonomyField,
        control: crate::player::Control,
    },
    /// Przypnij politykę do zakładu (M9d WP8). Polityka jedzie **całym drzewem**,
    /// a nie kluczem presetu: gracz ją właśnie zbudował i nigdzie jeszcze nie stoi.
    ///
    /// Pełnomocnictwo jest zawsze `Autonomy::Full` i to nie jest uproszczenie:
    /// autonomia mówi, ile wolno **menedżerowi** bez pytania właściciela, a tu
    /// właściciel przypina regułę samemu sobie. Wybór autonomii wchodzi razem
    /// z zatrudnianiem menedżera, czyli w `M9e`.
    AttachPolicy {
        site: SiteId,
        policy: Box<magnat_policy::Policy>,
    },
    /// Zdejmij politykę z zakładu — od tej chwili ceny stoją tam, gdzie stanęły.
    DetachPolicy { site: SiteId },

    // ── M9e: komendy paneli biznesowych (WP10) ───────────────────────────────
    //
    // Każda z nich ma **wykonawcę i panel**, z którego się ją wydaje — to jest ta
    // sama reguła, którą `DC-1` postawił dla komend postaci. Lista paneli, z których
    // da się coś zrobić, stoi w `PanelId::is_operational`.
    /// Załóż firmę i pierwszy sklep w dzielnicy.
    ///
    /// Idzie **tą samą drogą co firma zakładana przez mieszkańca**
    /// (`magnat_economy::firmlife::found`): jeden rejestr, jedno konto, jeden lokal,
    /// jeden przelew kapitału. Druga ścieżka zakładania firm rozjechałaby się
    /// z pierwszą przy pierwszej zmianie w rejestrze (`K-11`).
    FoundFirm { district: u16, capital: Money },
    /// Otwórz kolejny punkt w dzielnicy. Nakład idzie z rachunku firmy — tyle,
    /// ile na nim stoi, bo zakład bez kapitału obrotowego jest poprawnym wynikiem.
    OpenSite { district: u16, capex: Money },
    /// Zamknij zakład: półka schodzi, załoga odchodzi, rejestr o tym wie.
    CloseSite { site: SiteId },
    /// Ustaw asortyment półki. Towary **kluczami tekstowymi** (00 §5), tak samo jak
    /// w `SetPrice`, i przycinane do liczby wyłożeń — gracz wybiera z listy,
    /// a ile się zmieści, wie półka.
    SetShelfAssortment { site: SiteId, goods: Vec<String> },
    /// Zatrudnij wskazanego mieszkańca na wskazane stanowisko.
    HireCandidate {
        site: SiteId,
        citizen: CitizenId,
        role: u16,
    },
    /// Ile wolno menedżerowi tego zakładu bez pytania właściciela.
    ///
    /// **To, a nie „przypisz menedżera", jest decyzją gracza.** Menedżerem zostaje
    /// najlepszy człowiek na stanowisku kierowniczym i wybiera go `reconcile_managers`
    /// codziennie — komenda „postaw tego" byłaby nadpisywana następnej doby, czyli
    /// byłaby wariantem bez skutku (`K-67`). Gracz stawia menedżera **zatrudniając
    /// go** na stanowisko kierownicze (`HireCandidate`), a tutaj mówi, ile mu wolno.
    /// To jest ta decyzja, której `AttachPolicy` świadomie nie podejmowało.
    SetDelegationAutonomy { site: SiteId, autonomy: Autonomy },
    /// Złóż podanie o pracę. **Podanie, nie przyjęcie oferty**: w tej gospodarce
    /// etat wygrywa się zgłoszeniem i doborem, a nie kliknięciem „przyjmuję"
    /// (`DH-2` — zbiór komend metryki §5.12 wymienia tę, która istnieje).
    ///
    /// Oferta jedzie **bitami uchwytu areny** (`ArenaHandle::to_bits`), bo oferty są
    /// poza ECS (`K-16`) i ich uchwyt nie jest encją. Uchwyt po wygaśnięciu oferty
    /// nigdy nie jest ponownie ważny, więc replay odrzuci podanie tak samo jak gra.
    ApplyForJob { offer: u64 },
    /// Poproś bank o kredyt obrotowy na zakład.
    TakeLoan { site: SiteId },
    /// Wpłać na kampanię kandydata (`K-66`). Pieniądz wychodzi z gospodarstwa
    /// gracza i jedzie kanałem `TxKind::CampaignDonation`.
    BackCandidate { candidate: u8, amount: Money },
    /// Złóż wniosek o pozwolenie. Urząd przerabia go w kolejce, tak samo jak wniosek
    /// firmy — `Applicant::Player` czekał w `sim/city` od M8d na kogoś, kto go wyda.
    ///
    /// Rodzaj jedzie **indeksem `PermitKind`**, którego kolejność jest kontraktem
    /// (`K-64`), a nie nazwą: dziennik wejść ma być mały.
    ApplyForPermit { kind: u8 },
    /// Przestaw cel zapasu towaru w zakładzie — na ile dób sklep ma się zatowarować.
    SetRestockTarget {
        site: SiteId,
        good: String,
        days: u16,
    },

    // ── M9e: porażka i dziedziczenie (WP12) ──────────────────────────────────
    /// Ogłoś upadłość osobistą. Zakłady idą do likwidacji, zobowiązania **zostają**
    /// razem z zaległościami — i to je widzi bank przy następnym wniosku.
    /// `GameState` pozostaje `Playing`: gra się nie kończy (§13.4).
    DeclarePersonalBankruptcy,
    /// Wskaż dziedzica. Bez tego wybiera go [`crate::legacy::heir_of`] po śmierci.
    SetHeir { citizen: CitizenId },
    /// Dziedzic przejmuje rolę. Własność firm przechodzi sama — `Owner::Player` jest
    /// rolą, a nie osobą. **Nie przechodzą relacje ani umiejętności**: należą do
    /// komponentu nowego mieszkańca i nikt ich nie przepisuje.
    Succeed { citizen: CitizenId },
    /// Nowa dynastia w tym samym świecie. Firm poprzedniej nie dziedziczy nikt,
    /// więc idą do likwidacji — inaczej miasto zapełniłoby się zakładami bez
    /// właściciela, którymi nikt nigdy nie pokieruje.
    ContinueAsNewCitizen { citizen: CitizenId },
}

/// Ile wolno menedżerowi bez pytania właściciela.
///
/// Powtórzenie `magnat_firms::Autonomy` w kopercie komendy, a nie jego reeksport:
/// koperta jedzie do dziennika wejść i jej kształt jest **kontraktem zapisu**,
/// a `sim/firms` ma prawo swój enum przestawić. Dyskryminanty są tu wieczne.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Autonomy {
    /// Wyłącznie ceny — tyle, ile dostaje zastępstwo po odejściu menedżera.
    #[default]
    PricesOnly,
    /// Ceny i obsada.
    PricesAndStaff,
    /// Wszystko, co polityka umie wykonać.
    Full,
}

impl Autonomy {
    pub const ALL: [Autonomy; 3] = [
        Autonomy::PricesOnly,
        Autonomy::PricesAndStaff,
        Autonomy::Full,
    ];

    #[must_use]
    pub const fn to_firms(self) -> magnat_firms::Autonomy {
        match self {
            Autonomy::PricesOnly => magnat_firms::Autonomy::PricesOnly,
            Autonomy::PricesAndStaff => magnat_firms::Autonomy::PricesAndStaff,
            Autonomy::Full => magnat_firms::Autonomy::Full,
        }
    }

    #[must_use]
    pub const fn from_firms(a: magnat_firms::Autonomy) -> Autonomy {
        match a {
            magnat_firms::Autonomy::PricesOnly => Autonomy::PricesOnly,
            magnat_firms::Autonomy::PricesAndStaff => Autonomy::PricesAndStaff,
            magnat_firms::Autonomy::Full => Autonomy::Full,
        }
    }

    /// Klucz tekstu: `ui.autonomy.<key>`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Autonomy::PricesOnly => "prices_only",
            Autonomy::PricesAndStaff => "prices_and_staff",
            Autonomy::Full => "full",
        }
    }
}

/// Komenda zmieniająca **widok**, nie świat. Nie wchodzi do hasha stanu.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum ViewCommand {
    SetTimeScale(magnat_core::SimSpeed),
    SaveGame { slot: u8 },
    /// Gracz otworzył panel. **To jest nośnik metryki onboardingu** (§5.12): liczba
    /// otwartych paneli do pierwszej sensownej decyzji liczy się z tego strumienia,
    /// a nie z osobnej telemetrii.
    OpenPanel(crate::panels::PanelId),
    ClosePanel(crate::panels::PanelId),
    /// Warunek zatrzymania włączony albo wyłączony (§5.10).
    SetStopCondition {
        id: crate::timectl::StopConditionId,
        on: bool,
    },
    /// Tryb „śledź". `None` kończy śledzenie.
    Follow(Option<crate::timectl::FollowTarget>),
}

/// Wpis strumienia widoku: co i **kiedy naprawdę** — znacznik czasu rzeczywistego
/// jest tu dozwolony, bo ten strumień nie wpływa na symulację (§5.5).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ViewRecord {
    pub wall_ms: u64,
    pub tick: Tick,
    pub cmd: ViewCommand,
}


/// To, co komenda widzi ze świata.
///
/// Jeden typ dla obu stopni walidacji: panel sprawdza nim komendę **zanim** gracz
/// kliknie (wygaszony przycisk z powodem), a sesja **autorytatywnie** w punkcie
/// synchronizacji. Ta sama funkcja, dwa wywołania — inaczej UI i świat mogłyby
/// mieć różne zdanie o tym, co wolno.
///
/// `ponytail:` do czasu, aż `M9b` postawi podwójnie buforowany `Snapshot`, oba
/// wywołania idą po tym samym `Market`. Panel dostanie wtedy widok na migawkę,
/// a ta funkcja nie drgnie — bo nie zagląda do `&World`.
pub struct CommandView<'a> {
    pub market: Option<&'a Market>,
    /// Świat ECS — komendy dotyczące mieszkańców sprawdzają w nim, czy podmiot
    /// istnieje. `Option`, bo panel może pytać, zanim świat stanie.
    pub world: Option<&'a magnat_ecs::World>,
    /// Czy gra ma już postać. Panel wygasza „wybierz postać" po jej wyborze i musi
    /// znać powód **przed** kliknięciem, a nie po odrzuceniu komendy.
    pub has_character: bool,
}

/// Wykonuje komendę. Wołane **wyłącznie** po udanym [`precheck`], w punkcie
/// synchronizacji przed systemami ticku.
///
/// # Errors
/// To samo co [`precheck`] — świat mógł się zmienić między sprawdzeniem a tickiem.
pub fn apply(view: &CommandView<'_>, cmd: &PlayerCommand) -> Result<(), CommandError> {
    precheck(view, cmd)?;
    match cmd {
        PlayerCommand::StartGame { .. } => Ok(()),
        // Polityki zmieniają rejestr firm, czyli zasób świata na mutowalnie —
        // wykonuje je `Session::apply_due` z tego samego powodu co komendy postaci.
        PlayerCommand::AttachPolicy { .. } | PlayerCommand::DetachPolicy { .. } => Ok(()),
        // Obie komendy postaci zmieniają świat i sesję, więc wykonuje je
        // `Session::apply_due` — tu jest tylko walidacja, wspólna dla obu stron.
        PlayerCommand::SetCharacter { .. } | PlayerCommand::SetAutonomy { .. } => Ok(()),
        // Komendy paneli biznesowych sięgają do `&mut World` — rejestru firm, ksiąg,
        // rynku pracy i strony publicznej. Wykonuje je `exec::run` z sesji; tutaj
        // zostaje walidacja, ta sama, którą panel woła przy wygaszaniu przycisku.
        PlayerCommand::FoundFirm { .. }
        | PlayerCommand::OpenSite { .. }
        | PlayerCommand::CloseSite { .. }
        | PlayerCommand::HireCandidate { .. }
        | PlayerCommand::SetDelegationAutonomy { .. }
        | PlayerCommand::ApplyForJob { .. }
        | PlayerCommand::TakeLoan { .. }
        | PlayerCommand::BackCandidate { .. }
        | PlayerCommand::ApplyForPermit { .. }
        | PlayerCommand::SetShelfAssortment { .. }
        | PlayerCommand::SetRestockTarget { .. }
        | PlayerCommand::DeclarePersonalBankruptcy
        | PlayerCommand::SetHeir { .. }
        | PlayerCommand::Succeed { .. }
        | PlayerCommand::ContinueAsNewCitizen { .. } => Ok(()),
        PlayerCommand::SetPrice { site, good, price } => {
            let market = view.market.ok_or(CommandError::NoMarket)?;
            let g = resolve_good(market, good)?;
            if !market.set_price(*site, g, *price) {
                return Err(zgub_sklep(market, *site, good));
            }
            market.set_policy(
                *site,
                g,
                magnat_economy::PricePolicy::Fixed { price: *price },
                false,
            );
            Ok(())
        }
    }
}

pub(crate) fn resolve_good(market: &Market, key: &str) -> Result<GoodId, CommandError> {
    market
        .good_of_key(key)
        .ok_or_else(|| CommandError::UnknownGood {
            key: key.to_string(),
        })
}

/// Rozróżnia „nie ma takiego zakładu" od „ma, ale nie handluje tym towarem".
/// Bez tego gracz dostawałby jeden komunikat na dwa różne błędy.
pub(crate) fn zgub_sklep(market: &Market, site: SiteId, key: &str) -> CommandError {
    if market.sites().contains(&site) {
        CommandError::NotOnShelf {
            site,
            key: key.to_string(),
        }
    } else {
        CommandError::SiteNotFound { site }
    }
}
