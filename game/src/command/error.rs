//! Czego komendzie zabrakło — [`CommandError`] i jego zdania.
//!
//! Osobny plik, bo to jest **inny temat** niż komenda: tam jest to, o co gracz
//! prosi, tutaj to, czego brakuje. Jeden i drugi enum rośnie z każdą fazą i rośnie
//! z innego powodu — komenda razem ze swoim wykonawcą, błąd razem ze swoim
//! sprawdzeniem.
//!
//! Enum z parametrami, nigdy napis: tekst dla gracza składa lokalizacja, a nie
//! `format!` w miejscu odrzucenia (§5.5). `Display` jest tu **dla dewelopera**
//! i dla dziennika, nie dla okna.

use magnat_core::{CitizenId, Money, SiteId};
use serde::{Deserialize, Serialize};

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
    /// Świat nie ma rejestru firm — scenariusz postawił sam rynek detaliczny.
    NoFirms,
    /// Zakład należy do kogoś innego. Gracz przypina reguły **swoim** zakładom;
    /// cudzą politykę wolno obejrzeć, a nie podmienić.
    NotYourSite {
        site: SiteId,
    },
    /// Polityka nie przeszła walidatora. Liczba uwag, nie ich lista: pełną
    /// diagnostykę pokazuje edytor **przed** kliknięciem, a koperta komendy jedzie
    /// do dziennika wejść i ma być mała.
    PolicyInvalid {
        notes: u16,
    },
    /// Zakład nie ma przypiętej polityki, więc nie ma czego zdejmować.
    NoPolicy {
        site: SiteId,
    },

    // ── M9e ──────────────────────────────────────────────────────────────────
    /// Gracz nie prowadzi jeszcze żadnej firmy.
    NoFirm,
    /// Gracz już ma firmę. Druga jest przejęciem, a przejęcia należą do M10.
    AlreadyHasFirm,
    /// Kwota zero albo ujemna. Osobno od ceny, bo „za darmo" i „zero kapitału"
    /// to dwa różne błędy gracza.
    AmountNotPositive {
        amount: Money,
    },
    /// W tej dzielnicy nie ma lokalu, do którego dałoby się wejść.
    NoSeedInDistrict {
        district: u16,
    },
    /// Gospodarstwo nie ma tyle pieniędzy.
    NotEnoughCash {
        need: Money,
        have: Money,
    },
    /// Zakład nie ma takiego stanowiska albo wszystkie etaty są obsadzone.
    NoVacancy {
        site: SiteId,
        role: u16,
    },
    /// Świat nie ma rynku pracy — scenariusz postawił sam rynek detaliczny.
    NoLabor,
    /// Takiej oferty pracy nie ma albo już wygasła. Numer to bity uchwytu areny —
    /// oferty są poza ECS (`K-16`), więc nie ma tu encji.
    NoJobOffer {
        offer: u64,
    },
    /// Świat nie ma strony publicznej.
    NoCity,
    /// Nie trwa żadna kampania wyborcza.
    NoElection,
    /// Takiego kandydata nie ma na liście.
    NoCandidate {
        candidate: u8,
    },
    /// Takiego rodzaju pozwolenia nie ma w słowniku.
    UnknownPermit {
        kind: u8,
    },
    /// Bank odmówił kredytu.
    CreditRefused {
        cause: magnat_core::RejectCredit,
    },
    /// Ten mieszkaniec już gdzieś pracuje i nie da się go postawić nad zakładem
    /// bez zwolnienia go tam, gdzie jest.
    AlreadyEmployed {
        citizen: CitizenId,
    },
    /// Postać żyje — sukcesja nie ma po kim nastąpić.
    PlayerAlive,
    /// Ten mieszkaniec nie jest w gospodarstwie zmarłego, więc nie jest dziedzicem.
    NotAnHeir {
        citizen: CitizenId,
    },
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
            CommandError::NoFirms => write!(f, "świat nie ma rejestru firm"),
            CommandError::NotYourSite { site } => write!(f, "zakład {site:?} nie jest twój"),
            CommandError::PolicyInvalid { notes } => {
                write!(f, "polityka ma {notes} uwag walidatora")
            }
            CommandError::NoPolicy { site } => {
                write!(f, "zakład {site:?} nie ma przypiętej polityki")
            }
            CommandError::NoFirm => write!(f, "gracz nie prowadzi żadnej firmy"),
            CommandError::AlreadyHasFirm => write!(f, "gracz ma już firmę"),
            CommandError::AmountNotPositive { amount } => {
                write!(f, "kwota {} gr nie jest dodatnia", amount.get())
            }
            CommandError::NoSeedInDistrict { district } => {
                write!(f, "w dzielnicy {district} nie ma wolnego lokalu")
            }
            CommandError::NotEnoughCash { need, have } => write!(
                f,
                "brakuje {} gr — potrzeba {}, jest {}",
                need.get() - have.get(),
                need.get(),
                have.get()
            ),
            CommandError::NoVacancy { site, role } => {
                write!(
                    f,
                    "zakład {site:?} nie ma wolnego etatu na stanowisku {role}"
                )
            }
            CommandError::NoLabor => write!(f, "świat nie ma rynku pracy"),
            CommandError::NoJobOffer { offer } => write!(f, "nie ma oferty pracy {offer}"),
            CommandError::NoCity => write!(f, "świat nie ma strony publicznej"),
            CommandError::NoElection => write!(f, "nie trwa kampania wyborcza"),
            CommandError::NoCandidate { candidate } => {
                write!(f, "nie ma kandydata o numerze {candidate}")
            }
            CommandError::UnknownPermit { kind } => {
                write!(f, "nie ma pozwolenia rodzaju {kind}")
            }
            CommandError::CreditRefused { cause } => {
                write!(f, "bank odmówił kredytu: {cause:?}")
            }
            CommandError::AlreadyEmployed { citizen } => {
                write!(f, "mieszkaniec {citizen:?} już pracuje")
            }
            CommandError::PlayerAlive => write!(f, "postać gracza żyje"),
            CommandError::NotAnHeir { citizen } => {
                write!(
                    f,
                    "mieszkaniec {citizen:?} nie dziedziczy po postaci gracza"
                )
            }
        }
    }
}

impl std::error::Error for CommandError {}

impl CommandError {
    /// Klucz zdania dla gracza: `ui.cmd_err.<key>`.
    ///
    /// **`Display` nie nadaje się do okna** i mówi to o sobie w nagłówku tego pliku:
    /// jest polskim literałem dla dewelopera i wypisuje `SiteId(Entity { … })`.
    /// Wygaszony przycisk wiesza przy sobie **powód**, a powód jest tekstem dla
    /// gracza — więc przechodzi przez katalog jak każdy inny (CLAUDE.md).
    ///
    /// Wyczerpujący `match` z rozmysłu: nowy wariant bez zdania nie skompiluje `game/`.
    #[must_use]
    pub const fn key(&self) -> &'static str {
        match self {
            CommandError::NoMarket => "no_market",
            CommandError::SiteNotFound { .. } => "site_not_found",
            CommandError::UnknownGood { .. } => "unknown_good",
            CommandError::NotOnShelf { .. } => "not_on_shelf",
            CommandError::PriceNotPositive { .. } => "price_not_positive",
            CommandError::CitizenNotFound { .. } => "citizen_not_found",
            CommandError::NoCharacter => "no_character",
            CommandError::CharacterAlreadySet => "character_already_set",
            CommandError::NoFirms => "no_firms",
            CommandError::NotYourSite { .. } => "not_your_site",
            CommandError::PolicyInvalid { .. } => "policy_invalid",
            CommandError::NoPolicy { .. } => "no_policy",
            CommandError::NoFirm => "no_firm",
            CommandError::AlreadyHasFirm => "already_has_firm",
            CommandError::AmountNotPositive { .. } => "amount_not_positive",
            CommandError::NoSeedInDistrict { .. } => "no_seed_in_district",
            CommandError::NotEnoughCash { .. } => "not_enough_cash",
            CommandError::NoVacancy { .. } => "no_vacancy",
            CommandError::NoLabor => "no_labor",
            CommandError::NoJobOffer { .. } => "no_job_offer",
            CommandError::NoCity => "no_city",
            CommandError::NoElection => "no_election",
            CommandError::NoCandidate { .. } => "no_candidate",
            CommandError::UnknownPermit { .. } => "unknown_permit",
            CommandError::CreditRefused { .. } => "credit_refused",
            CommandError::AlreadyEmployed { .. } => "already_employed",
            CommandError::PlayerAlive => "player_alive",
            CommandError::NotAnHeir { .. } => "not_an_heir",
        }
    }

    /// Zdanie dla gracza w jego języku.
    ///
    /// Liczby podstawiają się nazwane; identyfikatorów encji **nie pokazujemy** —
    /// gracz nie widzi `SiteId` i nie ma powodu zaczynać.
    #[must_use]
    pub fn text(&self, c: &magnat_ui::Catalog, l: magnat_ui::Locale) -> String {
        let klucz = format!("ui.cmd_err.{}", self.key());
        match self {
            CommandError::UnknownGood { key } | CommandError::NotOnShelf { key, .. } => {
                c.fmt_key(l, &klucz, &[("towar", key)])
            }
            CommandError::PriceNotPositive { price } => c.fmt_key(
                l,
                &klucz,
                &[("kwota", &magnat_ui::fmt::money(c, l, *price))],
            ),
            CommandError::AmountNotPositive { amount } => c.fmt_key(
                l,
                &klucz,
                &[("kwota", &magnat_ui::fmt::money(c, l, *amount))],
            ),
            CommandError::NotEnoughCash { need, have } => c.fmt_key(
                l,
                &klucz,
                &[
                    (
                        "brakuje",
                        &magnat_ui::fmt::money(c, l, Money(need.get() - have.get())),
                    ),
                    ("trzeba", &magnat_ui::fmt::money(c, l, *need)),
                ],
            ),
            CommandError::NoSeedInDistrict { district } => {
                c.fmt_key(l, &klucz, &[("nr", &district.to_string())])
            }
            CommandError::PolicyInvalid { notes } => {
                c.fmt_key(l, &klucz, &[("ile", &notes.to_string())])
            }
            CommandError::NoVacancy { role, .. } => {
                c.fmt_key(l, &klucz, &[("rola", &role.to_string())])
            }
            CommandError::CreditRefused { cause } => c.fmt_key(
                l,
                &klucz,
                &[(
                    "powod",
                    &c.fmt_key(l, &format!("ui.reject_credit.{}", cause.name()), &[]),
                )],
            ),
            _ => c.fmt_key(l, &klucz, &[]),
        }
    }
}
