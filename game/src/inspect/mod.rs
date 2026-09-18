//! Karta inspekcji: składanie treści dla każdego podmiotu (M9c §5.7, WP5).
//!
//! # Dlaczego tutaj, a nie w `engine/ui`
//!
//! Kształt karty — nagłówek plus zakładki — mieszka w `magnat_ui::InspectionCard`.
//! **Treść składa się tu**, bo dopiero z `game/` widać naraz świat ECS, miasto
//! (`CityData` z `sim/world`), rynek i rejestr firm. `engine/ui` nie zależy od
//! `sim/world` i nie ma powodu zaczynać: kartę parceli dałoby się przez to zbudować
//! wyłącznie kosztem nowej krawędzi w grafie crate'ów, a §6 dokumentu fazy wymienia
//! `InspectionCard` pod adresem `game::inspect` właśnie dlatego.
//!
//! # Reguła, z której bierze się reszta modułu
//!
//! **Jeśli karta wymienia podmiot, który ma własną kartę, to jest to odnośnik**
//! (`Z-2`) — adres mieszkania prowadzi do budynku, nazwa pracodawcy do zakładu,
//! nazwisko męża do jego karty, konkurent z powodu decyzji do jego zakładu.
//! Nośnikiem jest `Span { link: Option<Subject> }`, więc odnośnik wolno postawić
//! w środku zdania, a nie tylko w wydzielonej sekcji powiązań.
//!
//! **Cel, który przestał istnieć, renderuje się jako nazwa bez odnośnika** (`Z-5`):
//! [`resolve`] zwraca `false`, a `None` nie jest błędem, tylko normalnym stanem
//! świata, który się zmienia. Odnośnik prowadzący w pustkę jest gorszy od jego braku.

mod batch;
mod business;
mod citizen;
mod city;

pub use citizen::household_worth;

use crate::Session;
use magnat_core::{
    ArenaRef, BuildingId, CaseId, CitizenId, ContractId, DistrictId, EventId, FirmId, HouseholdId,
    ParcelId, PermitId, SiteId, Subject, SubjectKind, TenderId, VehicleId,
};
use magnat_ui::{gone_span, Catalog, InspectionCard, Locale, Rich, Span};

/// Wszystko, czego karta potrzebuje: teksty, język i świat.
///
/// `&Session`, a nie `&World`: budynek i parcela są w `CityData`, rynek w `Market`,
/// a jedno i drugie wisi obok świata ECS, nie w nim.
pub struct CardCtx<'a> {
    pub c: &'a Catalog,
    pub l: Locale,
    pub session: &'a Session,
}

impl<'a> CardCtx<'a> {
    #[must_use]
    pub fn new(c: &'a Catalog, l: Locale, session: &'a Session) -> CardCtx<'a> {
        CardCtx { c, l, session }
    }

    /// Doba świata — karta mieszkańca odtwarza z niej plan dnia.
    #[must_use]
    pub fn day(&self) -> u64 {
        self.session.tick().get() / 1440
    }

    #[must_use]
    pub fn text(&self, key: &str) -> String {
        self.c.fmt_key(self.l, key, &[])
    }

    #[must_use]
    pub fn fmt(&self, key: &str, args: &[(&str, &str)]) -> String {
        self.c.fmt_key(self.l, key, args)
    }

    /// Kwota w języku gracza — jeden przelicznik dla wszystkich kart.
    #[must_use]
    pub fn money_str(&self, m: magnat_core::Money) -> String {
        magnat_ui::fmt::money(self.c, self.l, m)
    }

    /// Wiersz tekstu bez odnośnika.
    pub fn line(&self, out: &mut Rich, key: &str, args: &[(&str, &str)]) {
        out.push(Span::plain(format!("{}\n", self.fmt(key, args))));
    }

    /// Wiersz zakończony odnośnikiem: `etykieta: <nazwa>`, gdzie nazwa prowadzi
    /// do karty podmiotu — albo jest samą nazwą, gdy podmiotu już nie ma (`Z-5`).
    pub fn link_line(&self, out: &mut Rich, label: &str, to: Subject) {
        out.push(Span::plain(format!("{}: ", self.text(label))));
        out.push(self.subject_span(to));
        out.push(Span::plain("\n".to_string()));
    }

    /// Nazwa podmiotu jako odnośnik albo — gdy podmiotu już nie ma — sama nazwa
    /// z jednym zdaniem, co się stało.
    #[must_use]
    pub fn subject_span(&self, to: Subject) -> Span {
        let nazwa = self.subject_name(to);
        if resolve(self.session, to) {
            Span::link(nazwa, to)
        } else {
            gone_span(self.c, self.l, &nazwa)
        }
    }

    /// Nazwa podmiotu w języku gracza. Nazwy własne (mieszkańcy, firmy, dzielnice)
    /// pochodzą z `data/names/` i **nie są lokalizacją UI** (CLAUDE.md) — ta sama
    /// nazwa pada w obu wersjach językowych.
    #[must_use]
    pub fn subject_name(&self, to: Subject) -> String {
        match to {
            Subject::Citizen(c) => citizen::name_of(self, c),
            Subject::Household(h) => self.fmt(
                "ui.subject.household",
                &[("nr", &h.entity().index().to_string())],
            ),
            Subject::Firm(f) => business::firm_name(self, f),
            Subject::Site(s) => business::site_name(self, s),
            Subject::Building(b) => city::building_name(self, b),
            Subject::Parcel(p) => self.fmt(
                "ui.subject.parcel",
                &[("nr", &p.entity().index().to_string())],
            ),
            Subject::Vehicle(v) => business::vehicle_name(self, v),
            Subject::Contract(k) => self.fmt(
                "ui.subject.contract",
                &[("nr", &k.entity().index().to_string())],
            ),
            Subject::District(d) => city::district_name(self, d),
            Subject::Event(e) => self.fmt("ui.subject.event", &[("nr", &e.get().to_string())]),
            Subject::Government => self.text("ui.subject.government"),
            Subject::Batch(b) => self.fmt("ui.subject.batch", &[("nr", &b.index().to_string())]),
            Subject::Offer(o) => self.fmt("ui.subject.offer", &[("nr", &o.index().to_string())]),
            Subject::Tender(t) => self.fmt("ui.subject.tender", &[("nr", &t.get().to_string())]),
            Subject::Case(k) => self.fmt("ui.subject.case", &[("nr", &k.get().to_string())]),
            Subject::Permit(p) => self.fmt("ui.subject.permit", &[("nr", &p.get().to_string())]),
        }
    }
}

/// Czy podmiot nadal istnieje w świecie.
///
/// To jest `Subject::resolve` z `K-62` pkt (3) po stronie, która ma czym sprawdzić:
/// `core` zna typ uchwytu, ale nie zna świata. `false` **nie jest błędem** — firma
/// upadła, mieszkaniec zmarł, partia została sprzedana.
///
/// `ponytail:` podmioty, których rejestru karta nie otwiera (umowa, oferta, przetarg,
/// sprawa, pozwolenie, zdarzenie), odpowiadają `true`: mamy ich identyfikator i nic
/// poza nim. **Partia od `M9e` odpowiada prawdą** — jej ślad czyta `batch::card`.
/// Dla pozostałych sufit zostaje: rejestry przetargów i spraw są w `sim/city`,
/// a pytanie „czy ten przetarg jeszcze trwa" nie ma dziś czytelnika poza kroniką.
#[must_use]
pub fn resolve(session: &Session, subject: Subject) -> bool {
    match subject {
        Subject::Citizen(c) => citizen::is_alive(session, c),
        Subject::Household(h) => citizen::household_exists(session, h),
        Subject::Firm(f) => business::firm_exists(session, f),
        Subject::Site(s) => business::site_exists(session, s),
        Subject::Building(b) => city::building_exists(session, b),
        Subject::Parcel(p) => city::parcel_exists(session, p),
        Subject::District(d) => city::district_exists(session, d),
        Subject::Vehicle(v) => business::vehicle_exists(session, v),
        Subject::Government => true,
        Subject::Contract(_)
        | Subject::Event(_)
        | Subject::Batch(_)
        | Subject::Offer(_)
        | Subject::Tender(_)
        | Subject::Case(_)
        | Subject::Permit(_) => true,
    }
}

/// Karta podmiotu. **Wyczerpujący `match`** — faza dokładająca byt z kartą dokłada
/// wariant `Subject` *i* ramię tutaj, inaczej `game/` się nie kompiluje (`K-62` pkt 2).
#[must_use]
pub fn card(ctx: &CardCtx<'_>, subject: Subject) -> InspectionCard {
    match subject {
        Subject::Citizen(c) => citizen::card(ctx, c),
        Subject::Household(h) => citizen::household_card(ctx, h),
        Subject::Firm(f) => business::firm_card(ctx, f),
        Subject::Site(s) => business::site_card(ctx, s),
        Subject::Vehicle(v) => business::vehicle_card(ctx, v),
        Subject::Building(b) => city::building_card(ctx, b),
        Subject::Parcel(p) => city::parcel_card(ctx, p),
        Subject::District(d) => city::district_card(ctx, d),
        Subject::Government => city::government_card(ctx),
        Subject::Batch(b) => batch::card(ctx, b),
        Subject::Contract(_)
        | Subject::Event(_)
        | Subject::Offer(_)
        | Subject::Tender(_)
        | Subject::Case(_)
        | Subject::Permit(_) => detail_card(ctx, subject),
    }
}

/// Karta podmiotu, o którym wiemy tyle, ile niesie jego identyfikator.
///
/// Jedna zakładka i jedno zdanie — dokładnie to, co zapowiada §5.7 („reszta encji
/// ma jedną zakładkę i zachowuje się jak dzisiejsza karta"). Treść dokładają panele
/// `M9e`, które czytają rejestry przetargów, spraw i kroniki.
fn detail_card(ctx: &CardCtx<'_>, subject: Subject) -> InspectionCard {
    let nazwa = ctx.subject_name(subject);
    let rodzaj = ctx.text(&format!("ui.subject.kind.{}", subject.kind().key()));
    let mut body: Rich = Vec::new();
    ctx.line(&mut body, "ui.card.only_id", &[("rodzaj", &rodzaj)]);
    InspectionCard::new(subject, magnat_ui::lines_titled(&format!("{nazwa}\n")))
        .tab(magnat_ui::CardTabKind::Detail, body)
}

/// Podmiot, który karta pokazuje po kliknięciu w mieszkańca z listy — wejście dla
/// klienta i dla testów.
#[must_use]
pub const fn citizen_subject(c: CitizenId) -> Subject {
    Subject::Citizen(c)
}

/// Wszystkie szesnaście rodzajów z przykładowym identyfikatorem — wejście testu
/// „każdy wariant `Subject` renderuje się w obu językach".
#[must_use]
pub fn sample_subjects(e: impl Fn(u32) -> magnat_core::Entity) -> Vec<Subject> {
    let arena = |i: u32| {
        ArenaRef::of(
            magnat_core::ArenaHandle::<()>::from_bits((1u64 << 32) | u64::from(i)).expect("uchwyt"),
        )
    };
    let v = vec![
        Subject::Citizen(CitizenId(e(1))),
        Subject::Household(HouseholdId(e(2))),
        Subject::Firm(FirmId(e(3))),
        Subject::Site(SiteId(e(4))),
        Subject::Building(BuildingId(e(5))),
        Subject::Parcel(ParcelId(e(6))),
        Subject::Vehicle(VehicleId(e(7))),
        Subject::Contract(ContractId(e(8))),
        Subject::District(DistrictId(9)),
        Subject::Event(EventId(10)),
        Subject::Government,
        Subject::Batch(arena(11)),
        Subject::Offer(arena(12)),
        Subject::Tender(TenderId(13)),
        Subject::Case(CaseId(14)),
        Subject::Permit(PermitId(15)),
    ];
    debug_assert_eq!(v.len(), SubjectKind::ALL.len());
    v
}
