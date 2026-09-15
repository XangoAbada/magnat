//! Karta inspekcji podróży i karta pojazdu (M4d/WP11, PRD §14.1 i §14.4).
//!
//! **Karta nie liczy niczego sama** — tak samo jak karta mieszkańca. Dostaje decyzję
//! (`ModeDecision`) i rejestr przejazdu (`TripLedger`) wyprodukowane przez `sim/traffic`
//! i zamienia je w tekst. Gdyby liczyła koszt uogólniony u siebie, gracz widziałby
//! drugi rachunek obok tego, na którym stoi symulacja (00 §7).
//!
//! **Powody renderuje wyłącznie [`crate::describe`]** (`K-12`). `ModeCompared`,
//! `NoParkingAtDestination` i `LeftBehind` mają tam gotowe zdania od M4c (`R-7`) —
//! tu się ich używa, a nie pisze drugi raz. Wyjątkiem jest [`Infeasible`], które jest
//! osobnym enumem z `sim/traffic`, a nie `DecisionReason`: ma własny wyczerpujący
//! `match` niżej, zbudowany na tej samej zasadzie (nowy wariant nie kompiluje UI).
//!
//! **Wejście przychodzi z zewnątrz, nie ze świata.** `TripLedger` nie jest komponentem
//! ECS i nigdzie nie jest przechowywany po zakończeniu podróży — kto chce pokazać kartę,
//! ten trzyma rejestr. Dlatego nie ma tu `InspectorPanel`: nie miałby skąd wziąć danych.

use crate::inspect::reason;
use crate::loc::{Catalog, Locale};
use magnat_agents::components::Identity;
use magnat_core::{Money, PlaceRef};
use magnat_traffic::mezo::HEADWAY_CS;
use magnat_traffic::{
    Candidate, FuelTank, GeneralizedCost, Infeasible, LocationKind, ModeDecision, OwnerKind,
    TravelOption, TripLedger, VehicleCatalog, VehicleClass, VehicleCondition, VehicleLocation,
    VehicleOwner, CS_PER_MINUTE, UL_PER_ML,
};

/// Wszystko, czego karta podróży potrzebuje, a czego nie ma w rejestrze przejazdu.
///
/// Końce podróży i czas planowany niesie `PendingTrip`, decyzję — `mode_choice`,
/// a rejestr — sieć. Karta łączy te trzy rzeczy i niczego nie dolicza.
pub struct TripView<'a> {
    pub traveller: &'a Identity,
    pub origin: PlaceRef,
    pub dest: PlaceRef,
    /// Czas obiecany przez planera. Różnica wobec faktycznego to spóźnienie.
    pub planned_minutes: u16,
    pub decision: &'a ModeDecision,
    pub ledger: &'a TripLedger,
}

/// Jeden rozważony środek transportu z pełnym rozbiciem kosztu albo powodem odrzucenia.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CandidateRow {
    pub label: String,
    pub chosen: bool,
    pub runner_up: bool,
    /// `None` = opcja odpadła przy planowaniu; wtedy wyjaśnia to `note`.
    pub cost: Option<GeneralizedCost>,
    /// Powód odrzucenia. Pusty dla opcji wykonalnych.
    pub note: String,
}

/// Rozbiór faktycznego czasu przejazdu (`N-6`).
///
/// Cztery kubełki, a nie trzy, i **żaden z nich nie jest resztą z odejmowania** poza
/// ostatnim: `LedgerEntry` niesie od M4d własne `node_delay_cs` i `blocked_cs`.
/// Miernik dryfu B1–B4 porównujący wyłącznie czasy całkowite nie odróżniłby złej
/// kalibracji modelu węzła od złego modelu kolejki, a to są dwie różne naprawy —
/// pierwsza jest w `settle_node`, druga w routingu bez sprzężenia z obciążeniem.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct TimeSplit {
    /// Sama jazda po krawędziach.
    pub travel_cs: u64,
    /// Skrzyżowania: zatrzymania razy odstęp ruszania **plus** opóźnienie modelu węzła
    /// (sygnalizacja, pierwszeństwo, kolejka ruchu skrętnego).
    pub node_cs: u64,
    /// Kolejki przed przewężeniami: czas czekania na wjazd na zapełnioną krawędź.
    pub queue_cs: u64,
    /// Reszta czasu podróży, której rejestr krawędzi nie tłumaczy: postój na stacji
    /// paliw, dojście do pojazdu i z powrotem, zaokrąglenie do minut.
    pub other_cs: u64,
}

impl TimeSplit {
    /// Rozbiór z rejestru. `None`, gdy podróż nie była śledzona — wpisy krawędzi
    /// powstają tylko dla obserwowanych mieszkańców, a rozbiór bez nich byłby
    /// zgadywaniem, nie pomiarem.
    #[must_use]
    pub fn of(ledger: &TripLedger) -> Option<TimeSplit> {
        if ledger.entries.is_empty() {
            return None;
        }
        let travel_cs: u64 = ledger.entries.iter().map(|e| u64::from(e.travel_cs)).sum();
        // Skrzyżowania i kolejki **mają własne pola** w rejestrze od M4d (`N-6`).
        // Wcześniej czas kolejki był resztą z odejmowania, więc opóźnienie sygnalizacji
        // lądowało w kubełku „kolejki" — a to są dwie różne naprawy: pierwsza to
        // kalibracja modelu węzła, druga to routing bez sprzężenia z obciążeniem.
        let node_cs: u64 = ledger
            .entries
            .iter()
            .map(|e| u64::from(e.stops) * u64::from(HEADWAY_CS) + u64::from(e.node_delay_cs))
            .sum();
        let queue_cs: u64 = ledger.entries.iter().map(|e| u64::from(e.blocked_cs)).sum();
        let total_cs = u64::from(ledger.minutes()) * CS_PER_MINUTE;
        Some(TimeSplit {
            travel_cs,
            node_cs,
            queue_cs,
            other_cs: total_cs.saturating_sub(travel_cs + node_cs + queue_cs),
        })
    }
}

/// Karta podróży — model, z którego powstaje i wydruk tekstowy, i widget M11.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TripCard {
    /// Imię i nazwisko z `data/names/`; **nie jest** lokalizacją UI (CLAUDE.md).
    pub traveller: String,
    pub origin: String,
    pub dest: String,
    pub depart_min: u16,
    pub arrive_min: u16,
    pub planned_minutes: u16,
    pub actual_minutes: u16,
    /// Dodatnie = spóźnienie wobec planu.
    pub late_minutes: i32,
    /// Dlaczego ten środek — `ModeChosen`, `ModeCompared` albo `NoParkingAtDestination`.
    pub choice_reason: String,
    /// Co się stało po drodze — `TripDelayed`, `StationChosen`, `RefuelNeeded`.
    /// Pusty, gdy nie wnosi nic ponad `choice_reason`.
    pub trip_reason: String,
    pub candidates: Vec<CandidateRow>,
    pub split: Option<TimeSplit>,
    pub edges: u32,
    pub stops: u32,
    pub distance_m: u64,
    /// Spalone paliwo w mililitrach.
    pub fuel_ml: i64,
    pub money: Money,
}

impl TripCard {
    #[must_use]
    pub fn build(c: &Catalog, l: Locale, v: &TripView<'_>) -> TripCard {
        let drugi = v.decision.runner_up().map(|(o, _)| o);
        let candidates = v
            .decision
            .candidates
            .as_slice()
            .iter()
            .map(|k| wiersz(c, l, k, v.decision.chosen, drugi))
            .collect();

        let choice_reason = reason::describe(c, l, v.decision.reason);
        let trip_reason = reason::describe(c, l, v.ledger.reason);
        let planowane = v.planned_minutes;
        let faktyczne = v.ledger.minutes();

        TripCard {
            traveller: crate::full_name(v.traveller),
            origin: place(c, l, v.origin),
            dest: place(c, l, v.dest),
            depart_min: (v.ledger.depart.0 % 1440) as u16,
            arrive_min: (v.ledger.arrive.0 % 1440) as u16,
            planned_minutes: planowane,
            actual_minutes: faktyczne,
            late_minutes: i32::from(faktyczne) - i32::from(planowane),
            trip_reason: if trip_reason == choice_reason {
                String::new()
            } else {
                trip_reason
            },
            choice_reason,
            candidates,
            split: TimeSplit::of(v.ledger),
            edges: v.ledger.edges,
            stops: v.ledger.stops,
            distance_m: v.ledger.distance_cm / 100,
            fuel_ml: v.ledger.total_fuel_ul / UL_PER_ML,
            money: v.ledger.total_money,
        }
    }

    /// Karta w formie tekstowej — ta sama treść co widget, do konsoli deweloperskiej
    /// i do złotego testu bez GPU.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn render_text(&self, c: &Catalog, l: Locale) -> String {
        use std::fmt::Write;
        let mut s = String::with_capacity(128 * (self.candidates.len() + 8));

        let _ = writeln!(
            s,
            "{}",
            c.fmt_key(
                l,
                "ui.trip.header",
                &[
                    ("imie", &self.traveller),
                    ("skad", &self.origin),
                    ("dokad", &self.dest),
                ],
            )
        );
        let _ = writeln!(
            s,
            "  {}",
            c.fmt_key(
                l,
                "ui.trip.when",
                &[
                    ("wyjazd", &reason::zegar(self.depart_min)),
                    ("przyjazd", &reason::zegar(self.arrive_min)),
                    ("czas", &reason::minutes(c, l, self.actual_minutes)),
                    ("plan", &reason::minutes(c, l, self.planned_minutes)),
                ],
            )
        );
        let _ = writeln!(
            s,
            "  {}",
            if self.late_minutes > 0 {
                c.fmt_key(
                    l,
                    "ui.trip.late",
                    &[(
                        "ile",
                        &reason::minutes(c, l, self.late_minutes.unsigned_abs() as u16),
                    )],
                )
            } else {
                c.fmt_key(l, "ui.trip.on_time", &[])
            }
        );
        let _ = writeln!(s, "  {}", self.choice_reason);
        if !self.trip_reason.is_empty() {
            let _ = writeln!(s, "  {}", self.trip_reason);
        }

        let _ = writeln!(s, "{}:", c.fmt_key(l, "ui.trip.candidates", &[]));
        for k in &self.candidates {
            let znacznik = if k.chosen {
                "→"
            } else if k.runner_up {
                "·"
            } else if k.cost.is_none() {
                "×"
            } else {
                " "
            };
            let tresc = match &k.cost {
                Some(g) => c.fmt_key(
                    l,
                    "ui.trip.cost",
                    &[
                        ("razem", &crate::zlotowki(g.total)),
                        ("czas", &reason::minutes(c, l, g.time_minutes)),
                        ("stawka", &g.vot_gr_per_min.to_string()),
                        ("pieniadze", &crate::zlotowki(g.money_cost)),
                        ("dyskomfort", &crate::zlotowki(g.discomfort.total())),
                    ],
                ),
                None => k.note.clone(),
            };
            let _ = writeln!(s, "  {znacznik} {:<22} {tresc}", k.label);
        }

        if let Some(t) = self.split {
            let _ = writeln!(
                s,
                "{}: {} · {} · {}",
                c.fmt_key(l, "ui.trip.split", &[]),
                c.fmt_key(
                    l,
                    "ui.trip.split.travel",
                    &[("czas", &reason::minutes(c, l, na_minuty(t.travel_cs)))],
                ),
                c.fmt_key(
                    l,
                    "ui.trip.split.nodes",
                    &[("czas", &reason::minutes(c, l, na_minuty(t.node_cs)))],
                ),
                c.fmt_key(
                    l,
                    "ui.trip.split.queues",
                    &[("czas", &reason::minutes(c, l, na_minuty(t.queue_cs)))],
                ),
            );
            if t.other_cs > 0 {
                let _ = writeln!(
                    s,
                    "  {}",
                    c.fmt_key(
                        l,
                        "ui.trip.split.other",
                        &[("czas", &reason::minutes(c, l, na_minuty(t.other_cs)))],
                    ),
                );
            }
        }

        let _ = writeln!(
            s,
            "{}",
            c.fmt_key(
                l,
                "ui.trip.route",
                &[
                    (
                        "odcinki",
                        &c.plural(l, c.must("ui.unit.segments"), u64::from(self.edges)),
                    ),
                    ("metry", &self.distance_m.to_string()),
                    (
                        "postoje",
                        &c.plural(l, c.must("ui.unit.stops"), u64::from(self.stops)),
                    ),
                ],
            )
        );
        let _ = writeln!(
            s,
            "{} · {}",
            c.fmt_key(l, "ui.trip.fuel", &[("ile", &litry(self.fuel_ml))]),
            c.fmt_key(
                l,
                "ui.trip.money",
                &[("kwota", &crate::zlotowki(self.money))]
            ),
        );
        s
    }
}

/// Karta pojazdu (WP11): kto go ma, co to jest, w jakim jest stanie i gdzie stoi.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct VehicleCard {
    pub owner: String,
    /// Pusty, gdy pojazd nie ma kierowcy głównego.
    pub driver: String,
    pub class: String,
    pub fuel_kind: String,
    /// Zużycie 0..=100; 100 = wrak.
    pub wear: u8,
    pub odometer_km: u64,
    /// Ile kilometrów do przeglądu; ujemne = przegląd przeterminowany.
    pub service_in_km: i64,
    /// Minuta doby, do której pojazd stoi unieruchomiony. `None` = sprawny.
    pub broken_until_min: Option<u16>,
    pub fuel_ml: i64,
    pub tank_ml: i64,
    pub level_permille: u16,
    pub location: String,
}

/// Komplet komponentów jednego pojazdu. Osobna struktura z tego samego powodu co
/// [`TripView`]: karta czyta pięć komponentów ECS naraz i lista argumentów przestaje
/// się dać przeczytać, zanim przestanie się kompilować.
pub struct VehicleView<'a> {
    pub owner: &'a VehicleOwner,
    /// Kierowca główny, jeśli pojazd go ma — po to, żeby karta pokazała osobę,
    /// a nie indeks encji.
    pub driver: Option<&'a Identity>,
    pub class: VehicleClass,
    pub catalog: &'a VehicleCatalog,
    pub condition: &'a VehicleCondition,
    pub tank: &'a FuelTank,
    pub location: &'a VehicleLocation,
}

impl VehicleCard {
    #[must_use]
    pub fn build(c: &Catalog, l: Locale, v: &VehicleView<'_>) -> VehicleCard {
        let spec = v.catalog.spec(v.class.id);
        let cond = v.condition;
        VehicleCard {
            owner: wlasciciel(c, l, v.owner),
            driver: v.driver.map(crate::full_name).unwrap_or_default(),
            class: klasa(c, l, &spec.key),
            fuel_kind: c.fmt_key(
                l,
                &format!("ui.vehicle.fuel.{}", v.tank.fuel_kind().key()),
                &[],
            ),
            wear: cond.wear,
            odometer_km: cond.odometer_cm / 100_000,
            service_in_km: (cond.next_service_cm as i64 - cond.odometer_cm as i64) / 100_000,
            broken_until_min: (cond.broken_until != VehicleCondition::HEALTHY)
                .then_some((cond.broken_until % 1440) as u16),
            fuel_ml: v.tank.volume().0,
            tank_ml: v.tank.capacity / UL_PER_ML,
            level_permille: v.tank.level_permille(),
            location: polozenie(c, l, v.location),
        }
    }

    #[must_use]
    pub fn render_text(&self, c: &Catalog, l: Locale) -> String {
        use std::fmt::Write;
        let mut s = String::with_capacity(320);
        let _ = writeln!(
            s,
            "{}: {} ({})",
            c.fmt_key(l, "ui.card.vehicle", &[]),
            self.class,
            self.fuel_kind
        );
        let _ = writeln!(s, "  {}", self.owner);
        if !self.driver.is_empty() {
            let _ = writeln!(
                s,
                "  {}",
                c.fmt_key(l, "ui.vehicle.driver", &[("imie", &self.driver)])
            );
        }
        let _ = writeln!(
            s,
            "  {}",
            c.fmt_key(
                l,
                "ui.vehicle.condition",
                &[
                    ("zuzycie", &self.wear.to_string()),
                    ("przebieg", &self.odometer_km.to_string()),
                    ("przeglad", &self.service_in_km.to_string()),
                ],
            )
        );
        if let Some(m) = self.broken_until_min {
            let _ = writeln!(
                s,
                "  {}",
                c.fmt_key(l, "ui.vehicle.broken", &[("godzina", &reason::zegar(m))])
            );
        }
        let _ = writeln!(
            s,
            "  {}",
            c.fmt_key(
                l,
                "ui.vehicle.tank",
                &[
                    ("poziom", &litry(self.fuel_ml)),
                    ("pojemnosc", &litry(self.tank_ml)),
                    ("promile", &self.level_permille.to_string()),
                ],
            )
        );
        let _ = writeln!(s, "  {}", self.location);
        s
    }
}

// ── pomocnicze ──────────────────────────────────────────────────────────────────

fn wiersz(
    c: &Catalog,
    l: Locale,
    k: &Candidate,
    chosen: TravelOption,
    runner_up: Option<TravelOption>,
) -> CandidateRow {
    CandidateRow {
        label: option_name(c, l, k.option),
        chosen: k.option == chosen,
        runner_up: runner_up == Some(k.option) && k.option != chosen,
        cost: k.infeasible.is_none().then_some(k.cost),
        note: k
            .infeasible
            .map(|i| infeasible(c, l, i))
            .unwrap_or_default(),
    }
}

/// Nazwa opcji transportowej. To **nie** jest `TransportMode`: „auto własne"
/// i „auto rodzinne" jadą tym samym środkiem, a karta ma pokazać, którym z nich.
#[must_use]
pub fn option_name(c: &Catalog, l: Locale, o: TravelOption) -> String {
    c.fmt_key(l, &format!("ui.trip.option.{}", o.key()), &[])
}

/// Dlaczego opcja odpadła. Drugi — obok [`crate::describe`] — wyczerpujący `match`
/// w interfejsie: wariant dopisany w `sim/traffic` nie skompiluje UI, dopóki nikt
/// nie napisze, jak go pokazać graczowi (ta sama zasada co `K-12`).
#[must_use]
pub fn infeasible(c: &Catalog, l: Locale, i: Infeasible) -> String {
    match i {
        Infeasible::NoCarInHousehold => c.fmt_key(l, "ui.trip.no.NoCarInHousehold", &[]),
        Infeasible::CarInUseBy(by) => c.fmt_key(
            l,
            "ui.trip.no.CarInUseBy",
            &[(
                "kto",
                &if by == u32::MAX {
                    "?".to_string()
                } else {
                    by.to_string()
                },
            )],
        ),
        Infeasible::NoParkingWithinRadius { lots_searched } => c.fmt_key(
            l,
            "ui.trip.no.NoParkingWithinRadius",
            &[("ile", &lots_searched.to_string())],
        ),
        Infeasible::InsufficientFuelRange { range_m, needed_m } => c.fmt_key(
            l,
            "ui.trip.no.InsufficientFuelRange",
            &[
                ("zasieg", &(range_m / 1000).to_string()),
                ("trzeba", &(needed_m / 1000).to_string()),
            ],
        ),
        Infeasible::NoTransitConnection => c.fmt_key(l, "ui.trip.no.NoTransitConnection", &[]),
        Infeasible::NoRoute => c.fmt_key(l, "ui.trip.no.NoRoute", &[]),
        Infeasible::DistanceOverPersonalLimit { minutes } => c.fmt_key(
            l,
            "ui.trip.no.DistanceOverPersonalLimit",
            &[("czas", &reason::minutes(c, l, minutes))],
        ),
        Infeasible::BelowMinimumAge => c.fmt_key(l, "ui.trip.no.BelowMinimumAge", &[]),
        Infeasible::VehicleBroken => c.fmt_key(l, "ui.trip.no.VehicleBroken", &[]),
        Infeasible::BeyondBudget { fare_gr } => c.fmt_key(
            l,
            "ui.trip.no.BeyondBudget",
            &[("kwota", &crate::zlotowki(Money(i64::from(fare_gr))))],
        ),
    }
}

/// Miejsce jako tekst. Nazw budynków i ulic gra nie ma (M4d §5.10, „sufit"), więc
/// karta pokazuje numer — tak samo jak adres w karcie mieszkańca.
#[must_use]
pub fn place(c: &Catalog, l: Locale, p: PlaceRef) -> String {
    match p {
        PlaceRef::Building(e) => c.fmt_key(
            l,
            "ui.trip.place.Building",
            &[("nr", &e.0.index().to_string())],
        ),
        PlaceRef::Parcel(e) => c.fmt_key(
            l,
            "ui.trip.place.Parcel",
            &[("nr", &e.0.index().to_string())],
        ),
        PlaceRef::Site(e) => {
            c.fmt_key(l, "ui.trip.place.Site", &[("nr", &e.0.index().to_string())])
        }
        PlaceRef::District(d) => {
            c.fmt_key(l, "ui.trip.place.District", &[("nr", &d.0.to_string())])
        }
        PlaceRef::Coord(w) => c.fmt_key(
            l,
            "ui.trip.place.Coord",
            &[
                ("x", &(w.x / 100).to_string()),
                ("y", &(w.y / 100).to_string()),
            ],
        ),
    }
}

fn wlasciciel(c: &Catalog, l: Locale, o: &VehicleOwner) -> String {
    let klucz = match o.kind {
        x if x == OwnerKind::Firm as u8 => "ui.vehicle.owner.firm",
        x if x == OwnerKind::Transit as u8 => "ui.vehicle.owner.transit",
        _ => "ui.vehicle.owner.household",
    };
    c.fmt_key(l, klucz, &[("nr", &o.owner.to_string())])
}

fn polozenie(c: &Catalog, l: Locale, v: &VehicleLocation) -> String {
    match v.kind {
        x if x == LocationKind::OnEdge as u8 => c.fmt_key(
            l,
            "ui.vehicle.loc.on_edge",
            &[("odcinek", &v.edge.to_string())],
        ),
        x if x == LocationKind::Depot as u8 => {
            c.fmt_key(l, "ui.vehicle.loc.depot", &[("nr", &v.edge.to_string())])
        }
        _ => c.fmt_key(
            l,
            "ui.vehicle.loc.parked",
            &[("miejsce", &place(c, l, v.at))],
        ),
    }
}

/// Nazwa klasy pojazdu. Klucze klas pochodzą z `data/vehicles/classes.ron`, więc
/// klasa dołożona moddem nie ma wpisu w katalogu tekstów — wtedy lepiej pokazać
/// jej surowy klucz niż wywrócić interfejs.
fn klasa(c: &Catalog, l: Locale, key: &str) -> String {
    c.key(&format!("ui.vehicle.class.{key}"))
        .map_or_else(|| key.to_string(), |k| c.text(l, k).to_string())
}

/// Mililitry jako litry z dwoma miejscami. Osobno od [`crate::zlotowki`], bo to inna
/// wielkość i zmienia się z innego powodu (M12 dołoży walucie symbol i separator,
/// litrom nie). Obcięcie do centylitra jest świadome: karta nie udaje dokładności
/// mililitra, którą i tak zjadłoby zaokrąglenie ceny na stacji.
fn litry(ml: i64) -> String {
    let znak = if ml < 0 { "-" } else { "" };
    let cl = ml.unsigned_abs() / 10;
    format!("{znak}{}.{:02}", cl / 100, cl % 100)
}

/// Setne sekundy na pełne minuty, w dół — kubełki rozbioru mają się sumować do czasu
/// podróży, a nie przekraczać go przez trzy zaokrąglenia w górę.
fn na_minuty(cs: u64) -> u16 {
    (cs / CS_PER_MINUTE).min(u64::from(u16::MAX)) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn litry_licza_sie_z_mililitrow() {
        assert_eq!(litry(45_000), "45.00");
        assert_eq!(litry(367), "0.36");
        assert_eq!(litry(9), "0.00");
    }

    #[test]
    fn kazda_niewykonalnosc_ma_tekst_w_obu_jezykach() {
        let c = Catalog::load().expect("data/locale/");
        let wszystkie = [
            Infeasible::NoCarInHousehold,
            Infeasible::CarInUseBy(7),
            Infeasible::CarInUseBy(u32::MAX),
            Infeasible::NoParkingWithinRadius { lots_searched: 11 },
            Infeasible::InsufficientFuelRange {
                range_m: 4_000,
                needed_m: 12_000,
            },
            Infeasible::NoTransitConnection,
            Infeasible::NoRoute,
            Infeasible::DistanceOverPersonalLimit { minutes: 75 },
            Infeasible::BelowMinimumAge,
            Infeasible::VehicleBroken,
            Infeasible::BeyondBudget { fare_gr: 3_400 },
        ];
        for i in wszystkie {
            for l in Locale::ALL {
                let t = infeasible(&c, l, i);
                assert!(!t.is_empty(), "{i:?} w {} jest puste", l.code());
                assert!(!t.contains('{'), "{i:?} w {}: `{t}`", l.code());
            }
        }
    }

    #[test]
    fn kazda_opcja_transportowa_ma_nazwe() {
        let c = Catalog::load().expect("data/locale/");
        for o in TravelOption::ALL {
            for l in Locale::ALL {
                assert!(!option_name(&c, l, *o).is_empty(), "{o:?}");
            }
        }
    }
}
