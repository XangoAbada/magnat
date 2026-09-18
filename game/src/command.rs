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
}

/// Komenda zmieniająca **widok**, nie świat. Nie wchodzi do hasha stanu.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum ViewCommand {
    SetTimeScale(magnat_core::SimSpeed),
    SaveGame { slot: u8 },
}

/// Wpis strumienia widoku: co i **kiedy naprawdę** — znacznik czasu rzeczywistego
/// jest tu dozwolony, bo ten strumień nie wpływa na symulację (§5.5).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ViewRecord {
    pub wall_ms: u64,
    pub tick: Tick,
    pub cmd: ViewCommand,
}

/// Dlaczego komenda się nie wykonała. Enum z parametrami, nie napis — renderuje
/// go i18n, a nie `format!` w miejscu odrzucenia (§5.5).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum CommandError {
    /// Gospodarka wyłączona (`--no-economy`): nie ma rynku, do którego mówić.
    NoMarket,
    SiteNotFound {
        site: SiteId,
    },
    UnknownGood {
        key: String,
    },
    /// Sklep nie ma tego towaru na półce.
    NotOnShelf {
        site: SiteId,
        key: String,
    },
    /// Cena ujemna albo zero. Pieniądz jest `i64` w groszach, więc jedno i drugie
    /// da się wpisać, i jedno i drugie znaczy „oddaję towar za darmo".
    PriceNotPositive {
        price: Money,
    },
    /// Wskazany mieszkaniec nie istnieje albo nie żyje.
    CitizenNotFound {
        citizen: CitizenId,
    },
    /// Gra nie ma jeszcze postaci — nie ma komu ustawić autonomii.
    NoCharacter,
    /// Postać już jest. Drugi wybór dawałby drugi kapitał startowy, więc jest błędem,
    /// a nie przeprowadzką; dziedziczenie po śmierci to osobna komenda (`M9e`).
    CharacterAlreadySet,
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandError::NoMarket => write!(f, "gospodarka jest wyłączona"),
            CommandError::SiteNotFound { site } => write!(f, "nie ma zakładu {site:?}"),
            CommandError::UnknownGood { key } => write!(f, "nie ma towaru o kluczu {key}"),
            CommandError::NotOnShelf { site, key } => {
                write!(f, "zakład {site:?} nie ma na półce towaru {key}")
            }
            CommandError::PriceNotPositive { price } => {
                write!(f, "cena {} gr nie jest dodatnia", price.get())
            }
            CommandError::CitizenNotFound { citizen } => {
                write!(f, "nie ma mieszkańca {citizen:?}")
            }
            CommandError::NoCharacter => write!(f, "gra nie ma jeszcze postaci"),
            CommandError::CharacterAlreadySet => write!(f, "postać jest już wybrana"),
        }
    }
}

impl std::error::Error for CommandError {}

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

/// Sprawdza, czy komendę wolno wykonać. **Nie zmienia niczego.**
///
/// # Errors
/// [`CommandError`] opisujący, czego brakuje.
pub fn precheck(view: &CommandView<'_>, cmd: &PlayerCommand) -> Result<(), CommandError> {
    match cmd {
        // Koperta bootstrapowa: świat powstaje **przed** sesją, więc nie ma tu czego
        // sprawdzać ani czego wykonać. Jej treścią są parametry, z których replay
        // odtwarza grę (§5.13).
        PlayerCommand::StartGame { .. } => Ok(()),
        PlayerCommand::SetPrice { site, good, price } => {
            if price.get() <= 0 {
                return Err(CommandError::PriceNotPositive { price: *price });
            }
            let market = view.market.ok_or(CommandError::NoMarket)?;
            let g = resolve_good(market, good)?;
            if market.price_at(*site, g).is_none() {
                return Err(zgub_sklep(market, *site, good));
            }
            Ok(())
        }
        PlayerCommand::SetCharacter { citizen } => {
            if view.has_character {
                return Err(CommandError::CharacterAlreadySet);
            }
            let zyje = view.world.is_some_and(|w| {
                w.get::<magnat_agents::Identity>(citizen.entity())
                    .is_some_and(magnat_agents::Identity::is_alive)
            });
            if zyje {
                Ok(())
            } else {
                Err(CommandError::CitizenNotFound { citizen: *citizen })
            }
        }
        PlayerCommand::SetAutonomy { .. } => {
            if view.has_character {
                Ok(())
            } else {
                Err(CommandError::NoCharacter)
            }
        }
    }
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
        // Obie komendy postaci zmieniają świat i sesję, więc wykonuje je
        // `Session::apply_due` — tu jest tylko walidacja, wspólna dla obu stron.
        PlayerCommand::SetCharacter { .. } | PlayerCommand::SetAutonomy { .. } => Ok(()),
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

fn resolve_good(market: &Market, key: &str) -> Result<GoodId, CommandError> {
    market
        .good_of_key(key)
        .ok_or_else(|| CommandError::UnknownGood {
            key: key.to_string(),
        })
}

/// Rozróżnia „nie ma takiego zakładu" od „ma, ale nie handluje tym towarem".
/// Bez tego gracz dostawałby jeden komunikat na dwa różne błędy.
fn zgub_sklep(market: &Market, site: SiteId, key: &str) -> CommandError {
    if market.sites().contains(&site) {
        CommandError::NotOnShelf {
            site,
            key: key.to_string(),
        }
    } else {
        CommandError::SiteNotFound { site }
    }
}
