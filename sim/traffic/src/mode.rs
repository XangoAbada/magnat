//! Wybór środka transportu — koszt uogólniony (M4c/WP6, §5.3, PRD §9.4).
//!
//! **Wszystko liczy się w groszach.** Wygoda jest zmonetyzowana, nie ważona: nie ma
//! tu ani jednej wagi bez jednostki, więc karta inspekcji mówi „rower kosztowałby
//! o 3,40 zł więcej, bo pada", a nie „waga 0,7". To jest cały powód, dla którego
//! decyzja jest wyjaśnialna — nie da się wyjaśnić liczby, która nic nie znaczy.
//!
//! **Argmin z nawykiem, nie model logitowy.** Logit wymagałby losowania przy każdej
//! podróży i dawałby inny wynik przy tej samej sytuacji; argmin daje jeden wybór,
//! jedną ścieżkę i jedno uzasadnienie. Losowość wchodzi wyłącznie tam, gdzie modeluje
//! **niewiedzę**: wybór stacji paliw i wybór miejsca parkingowego (§5.3). Remisy
//! rozstrzyga kolejność w [`TravelOption`], a nie kolejność iteracji.
//!
//! **Podział pracy.** Ten moduł jest **czystą funkcją**: dostaje gotowe oferty —
//! czasy, kwoty, dostępność — i wybiera. Zdobycie ofert (routing, rezerwacja parkingu,
//! rozkład komunikacji) należy do [`crate::oracle`], bo tam jest router i stan miasta.
//! Dzięki temu cały rachunek kosztu da się przetestować bez grafu, bez świata
//! i bez pojazdu.

use crate::parking::ParkingSlotRef;
use crate::trip::TripPurpose;
use magnat_agents::ArrayVec;
use magnat_core::{data_path, DecisionReason, Money, TransportMode, Weather};
use serde::Deserialize;
use std::path::Path;

pub const MODE_CHOICE_SCHEMA_VERSION: u32 = 1;

/// Opcja transportowa rozważana przy podróży (§9.4).
///
/// To **nie jest** `TransportMode` z `core`: „auto własne" i „auto rodzinne" jadą
/// tym samym środkiem, a różnią się wykonalnością, i to ta różnica jest treścią
/// decyzji. Konwersja w jedną stronę jest w [`TravelOption::mode`].
///
/// Kolejność wariantów jest **regułą rozstrzygania remisów** (§5.3) i wchodzi do
/// indeksu tablic w `data/roads/mode_choice.ron` — przestawienie jej zmienia i jedno,
/// i drugie.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
#[repr(u8)]
pub enum TravelOption {
    #[default]
    Walk = 0,
    Bike = 1,
    Transit = 2,
    CarOwn = 3,
    CarHousehold = 4,
    Taxi = 5,
}

impl TravelOption {
    pub const ALL: &'static [TravelOption] = &[
        TravelOption::Walk,
        TravelOption::Bike,
        TravelOption::Transit,
        TravelOption::CarOwn,
        TravelOption::CarHousehold,
        TravelOption::Taxi,
    ];

    #[must_use]
    pub const fn as_index(self) -> usize {
        self as usize
    }

    /// Środek transportu w słowniku `core` — to on trafia do `TripHandle`,
    /// `PlanSlot.mode` i do `TravelTimeMatrix`.
    #[must_use]
    pub const fn mode(self) -> TransportMode {
        match self {
            TravelOption::Walk => TransportMode::Walk,
            TravelOption::Bike => TransportMode::Bicycle,
            TravelOption::Transit => TransportMode::Bus,
            TravelOption::CarOwn | TravelOption::CarHousehold | TravelOption::Taxi => {
                TransportMode::Car
            }
        }
    }

    /// Czy opcja wymaga miejsca postojowego u celu. Taksówka nie — wysadza i odjeżdża,
    /// i to jest dokładnie ta przewaga, którą ma w zatłoczonym centrum.
    #[must_use]
    pub const fn needs_parking(self) -> bool {
        matches!(self, TravelOption::CarOwn | TravelOption::CarHousehold)
    }

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            TravelOption::Walk => "walk",
            TravelOption::Bike => "bike",
            TravelOption::Transit => "transit",
            TravelOption::CarOwn => "car_own",
            TravelOption::CarHousehold => "car_household",
            TravelOption::Taxi => "taxi",
        }
    }
}

/// Dlaczego opcja odpadła. Niewykonalność rozstrzyga się **przy planowaniu** —
/// nigdy w trakcie podróży (`M-4`, M4 §7.1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Infeasible {
    NoCarInHousehold,
    /// Auto jest w innej podróży. `by` to indeks encji mieszkańca, który nim jedzie,
    /// albo `u32::MAX`, gdy tego nie wiadomo.
    CarInUseBy(u32),
    NoParkingWithinRadius {
        lots_searched: u16,
    },
    InsufficientFuelRange {
        range_m: u32,
        needed_m: u32,
    },
    NoTransitConnection,
    /// Dla tego środka nie ma trasy między końcami podróży — 3,5 % węzłów sieci M2
    /// to pułapki jednokierunkowe (`Y-1`, `J-18`), a 2,8 % par nie ma połączenia
    /// i **to jest prawda o mieście, nie błąd routera**.
    ///
    /// Wariantu nie ma w §5.3; dopisany po pomiarze M4a, bo bez niego opcja bez trasy
    /// musiałaby udawać jedną z pozostałych niewykonalności.
    NoRoute,
    DistanceOverPersonalLimit {
        minutes: u16,
    },
    BelowMinimumAge,
    VehicleBroken,
    /// Opcja jest **za droga dla tego gospodarstwa**: kwota przekracza dopuszczalny
    /// ułamek dziennego dochodu. Dziś dotyczy wyłącznie taksówki (`D5`).
    ///
    /// Wariantu nie ma w §5.3; dopisany po pomiarze metropolii, gdzie taksówka bez
    /// żadnego ograniczenia zbierała 15 % podróży. Sama cena jej nie hamuje, bo dla
    /// mieszkańca bez auta i bez zasięgu komunikacji jest jedyną szybką opcją —
    /// a w roku 1990 nie była opcją wcale, jeśli kosztowała dniówkę.
    BeyondBudget {
        fare_gr: u32,
    },
}

/// Rozbicie dyskomfortu na składniki, każdy w groszach (§5.3).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct DiscomfortBreakdown {
    /// Deszcz i mróz razy ekspozycja środka.
    pub weather: Money,
    /// Masa bagażu razy ekspozycja.
    pub luggage: Money,
    /// Obłożenie pojazdu komunikacji.
    pub crowding: Money,
    /// Stała kara za przesiadkę.
    pub transfers: Money,
    /// Dysonans statusu — ujemny dla auta u osoby o wysokim statusie.
    pub status: Money,
    /// Dojście do przystanku albo parkingu ponad próg.
    pub walk_access: Money,
    /// Stała niedogodność środka — **siódmy składnik, nie ma go w §5.3**.
    ///
    /// Dopisany po pomiarze scenariusza odniesienia: bez niego rower zbierał 92 %
    /// podróży, bo jest darmowy i trzykrotnie szybszy od marszu, a model czasu
    /// i pieniądza nie widzi ani kradzieży, ani jazdy po jezdni, ani tego, że do
    /// pracy nie przychodzi się spoconym. Wartości są w `data/`, nie w kodzie.
    pub mode_penalty: Money,
}

impl DiscomfortBreakdown {
    #[must_use]
    pub fn total(&self) -> Money {
        Money(
            self.weather.0
                + self.luggage.0
                + self.crowding.0
                + self.transfers.0
                + self.status.0
                + self.walk_access.0
                + self.mode_penalty.0,
        )
    }
}

/// Koszt uogólniony jednej opcji (§5.3).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct GeneralizedCost {
    pub time_minutes: u16,
    /// Wartość czasu: `f(dochód godzinowy GD, purpose)`.
    pub vot_gr_per_min: i64,
    pub time_cost: Money,
    /// Paliwo + bilet + parking + amortyzacja + taryfa.
    pub money_cost: Money,
    pub discomfort: DiscomfortBreakdown,
    pub total: Money,
}

/// Oferta opcji: to, co warstwa transportu wie o niej **przed** wyceną.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct OptionOffer {
    /// Pełny czas od drzwi do drzwi.
    pub minutes: u16,
    /// Kwota, którą mieszkaniec faktycznie wyjmie z portfela (bilet, taryfa, parking)
    /// plus paliwo i amortyzacja, jeśli jedzie własnym.
    pub money: Money,
    /// Ile z `minutes` spędza na dworze — dojście, oczekiwanie, jazda rowerem.
    pub exposed_minutes: u16,
    /// Obłożenie pojazdu w promilach; dotyczy wyłącznie komunikacji.
    pub crowding_permille: u16,
    pub transfers: u8,
    /// Dojście do przystanku albo od parkingu — wchodzi w `minutes`, ale ponad próg
    /// boli osobno.
    pub walk_access_min: u16,
    /// Zarezerwowane miejsce postojowe, jeśli opcja go wymagała.
    pub parking: Option<ParkingSlotRef>,
}

/// Mieszkaniec i okoliczności podróży — wszystko, czego wycena potrzebuje o człowieku.
#[derive(Clone, Copy, Debug)]
pub struct ModeContext {
    pub citizen: u32,
    pub age_years: u16,
    /// Percentyl statusu 0..=100 (M3d). Mediana to 50 i przy niej dysonans jest zerowy.
    pub status_percentile: u8,
    /// Dochód netto gospodarstwa w groszach na godzinę.
    pub hourly_net_income_gr: i64,
    pub purpose: TripPurpose,
    pub weather: Weather,
    /// Bagaż niesiony w tej podróży, w kilogramach.
    pub luggage_kg: u16,
    /// Środek wybrany poprzednio przez tego mieszkańca — podstawa premii nawyku.
    pub habit: Option<TravelOption>,
}

/// Wynik wyboru: co wybrano, ile to kosztuje i jak wypadli pozostali kandydaci.
#[derive(Clone, Debug)]
pub struct ModeDecision {
    pub chosen: TravelOption,
    pub minutes: u16,
    pub money: Money,
    pub parking: Option<ParkingSlotRef>,
    /// Pełna lista kandydatów z kosztem albo powodem odrzucenia — to ona idzie
    /// do karty inspekcji i to ona spełnia bramkę 5 z §7.4.
    pub candidates: ArrayVec<Candidate, { TravelOption::ALL.len() }>,
    /// Uzasadnienie w formie, która mieści się w ledgerze (00 §7).
    pub reason: DecisionReason,
}

/// Jeden kandydat na liście porównania.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Candidate {
    pub option: TravelOption,
    pub cost: GeneralizedCost,
    pub infeasible: Option<Infeasible>,
}

impl ModeDecision {
    /// Druga najtańsza wykonalna opcja i różnica kosztu — wejście `ModeCompared`.
    #[must_use]
    pub fn runner_up(&self) -> Option<(TravelOption, i64)> {
        let mut wykonalne: Vec<&Candidate> = self
            .candidates
            .as_slice()
            .iter()
            .filter(|c| c.infeasible.is_none())
            .collect();
        wykonalne.sort_by_key(|c| (c.cost.total.0, c.option));
        let pierwszy = wykonalne.first()?;
        let drugi = wykonalne.get(1)?;
        Some((drugi.option, drugi.cost.total.0 - pierwszy.cost.total.0))
    }
}

// ── dane ────────────────────────────────────────────────────────────────────────

/// Widełki udziału jednego środka w rozkładzie odniesienia.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub struct ShareRange {
    pub min: u16,
    pub max: u16,
}

impl ShareRange {
    #[must_use]
    pub fn contains(&self, permille: u16) -> bool {
        permille >= self.min && permille <= self.max
    }
}

/// Parametry wyboru środka — `data/roads/mode_choice.ron` (`K-24`).
#[derive(Clone, Debug, Deserialize)]
pub struct ModeChoiceParams {
    pub schema_version: u32,
    pub vot_floor_gr_per_min: i64,
    pub vot_cap_gr_per_min: i64,
    /// Indeks = `TripPurpose as usize`.
    pub purpose_multiplier_permille: Vec<i64>,
    /// Indeks = `TravelOption::as_index()`.
    pub weather_exposure_permille: Vec<i64>,
    pub transfer_penalty_gr: i64,
    pub crowding_gr_per_min_full: i64,
    pub status_gr_per_min_at_top: i64,
    pub luggage_gr_per_kg_min: i64,
    pub walk_access_free_min: u16,
    pub walk_access_gr_per_min: i64,
    /// Indeks = `TravelOption::as_index()`.
    pub option_penalty_gr: Vec<i64>,
    pub bike_ownership_permille: u16,
    pub habit_bonus_permille: i64,
    pub max_walk_min: u16,
    pub max_bike_min: u16,
    pub min_bike_age: u16,
    pub min_drive_age: u16,
    pub min_car_distance_m: u32,
    pub taxi_base_gr: i64,
    pub taxi_gr_per_km: i64,
    pub taxi_wait_min: u16,
    /// Ile promili dziennego dochodu gospodarstwa wolno wydać na jeden kurs taksówką.
    pub taxi_max_fare_permille_of_daily: i64,
    pub transit_fare_gr: i64,
    /// Od ilu mieszkańców widełki obowiązują (M4 §7.3: miasto 150 tys.). Poniżej tej
    /// liczby podróże są krótsze i rozkład jest inny — **legalnie**, bo to własność
    /// miasta, a nie modelu wyboru.
    pub reference_population: u32,
    /// Kolejność: Walk, Bike, Transit, Car (własne + rodzinne), Taxi.
    pub reference_share_permille: Vec<ShareRange>,
}

impl ModeChoiceParams {
    pub fn load_default() -> Result<ModeChoiceParams, crate::spec::DataError> {
        ModeChoiceParams::load(&data_path("roads/mode_choice.ron"))
    }

    pub fn load(path: &Path) -> Result<ModeChoiceParams, crate::spec::DataError> {
        let txt = std::fs::read_to_string(path)?;
        let p: ModeChoiceParams =
            ron::from_str(&txt).map_err(|e| crate::spec::DataError::Ron(e.to_string()))?;
        if p.schema_version != MODE_CHOICE_SCHEMA_VERSION {
            return Err(crate::spec::DataError::Schema {
                found: p.schema_version,
                want: MODE_CHOICE_SCHEMA_VERSION,
            });
        }
        // Tablice indeksowane enumem muszą mieć tyle wpisów, ile enum ma wariantów —
        // krótsza tablica nie jest brakiem danych, tylko cichym zerem w decyzji.
        if p.purpose_multiplier_permille.len() < 7 {
            return Err(crate::spec::DataError::Empty(
                "roads/mode_choice.ron: purpose_multiplier_permille",
            ));
        }
        if p.weather_exposure_permille.len() < TravelOption::ALL.len() {
            return Err(crate::spec::DataError::Empty(
                "roads/mode_choice.ron: weather_exposure_permille",
            ));
        }
        if p.option_penalty_gr.len() < TravelOption::ALL.len() {
            return Err(crate::spec::DataError::Empty(
                "roads/mode_choice.ron: option_penalty_gr",
            ));
        }
        if p.reference_share_permille.len() < 5 {
            return Err(crate::spec::DataError::Empty(
                "roads/mode_choice.ron: reference_share_permille",
            ));
        }
        Ok(p)
    }

    /// Wartość czasu mieszkańca w groszach na minutę (§5.3).
    ///
    /// Podłoga jest istotna: czas ma wartość także dla bezrobotnego. Bez niej cała
    /// ta grupa chodziłaby wszędzie pieszo bez względu na odległość, a rozkład
    /// udziałów rozjechałby się dokładnie tam, gdzie miasto ma najwięcej podróży.
    #[must_use]
    pub fn vot_gr_per_min(&self, hourly_net_income_gr: i64, purpose: TripPurpose) -> i64 {
        let mnoznik = self
            .purpose_multiplier_permille
            .get(purpose as usize)
            .copied()
            .unwrap_or(1_000);
        let base = hourly_net_income_gr.max(0) / 60;
        (base * mnoznik / 1_000).clamp(self.vot_floor_gr_per_min, self.vot_cap_gr_per_min)
    }

    /// Wycena jednej oferty. Czysta arytmetyka całkowita — wynik wchodzi do decyzji,
    /// więc nie ma tu ani jednego floata (00 §2).
    #[must_use]
    pub fn cost_of(
        &self,
        option: TravelOption,
        offer: &OptionOffer,
        ctx: &ModeContext,
    ) -> GeneralizedCost {
        let vot = self.vot_gr_per_min(ctx.hourly_net_income_gr, ctx.purpose);
        let time_cost = Money(i64::from(offer.minutes) * vot);

        let ekspozycja = self
            .weather_exposure_permille
            .get(option.as_index())
            .copied()
            .unwrap_or(0);
        let pogoda = i64::from(offer.exposed_minutes)
            * vot
            * i64::from(ctx.weather.harshness_permille())
            * ekspozycja
            / 1_000_000;

        let bagaz = if ekspozycja > 0 {
            i64::from(offer.exposed_minutes)
                * i64::from(ctx.luggage_kg)
                * self.luggage_gr_per_kg_min
                * ekspozycja
                / 1_000
        } else {
            0
        };

        let tlok = i64::from(offer.minutes)
            * i64::from(offer.crowding_permille)
            * self.crowding_gr_per_min_full
            / 1_000;

        let przesiadki = i64::from(offer.transfers) * self.transfer_penalty_gr;

        // Dysonans statusu jest **liniowy wokół mediany**: osoba o percentylu 50
        // nie płaci nic, o percentylu 100 płaci pełną stawkę za komunikację i tyleż
        // samo zyskuje na aucie. To nie jest snobizm dopisany do modelu, tylko
        // realizacja §5.4 — status jest w M3 i ma mieć konsekwencję.
        let status_skala = i64::from(ctx.status_percentile) - 50;
        let status = match option {
            TravelOption::Transit | TravelOption::Walk => {
                i64::from(offer.minutes) * self.status_gr_per_min_at_top * status_skala / 50
            }
            TravelOption::CarOwn | TravelOption::CarHousehold | TravelOption::Taxi => {
                -i64::from(offer.minutes) * self.status_gr_per_min_at_top * status_skala / 50
            }
            TravelOption::Bike => 0,
        };

        let ponad_prog = offer
            .walk_access_min
            .saturating_sub(self.walk_access_free_min);
        let dojscie = i64::from(ponad_prog) * self.walk_access_gr_per_min;

        let discomfort = DiscomfortBreakdown {
            mode_penalty: Money(
                self.option_penalty_gr
                    .get(option.as_index())
                    .copied()
                    .unwrap_or(0),
            ),
            weather: Money(pogoda),
            luggage: Money(bagaz),
            crowding: Money(tlok),
            transfers: Money(przesiadki),
            status: Money(status),
            walk_access: Money(dojscie),
        };
        GeneralizedCost {
            time_minutes: offer.minutes,
            vot_gr_per_min: vot,
            time_cost,
            money_cost: offer.money,
            discomfort,
            total: Money(time_cost.0 + offer.money.0 + discomfort.total().0),
        }
    }
}

/// Wybiera środek transportu spośród ofert (§5.3).
///
/// **Argmin z premią nawyku.** Opcja wybrana poprzednio dostaje zniżkę
/// [`ModeChoiceParams::habit_bonus_permille`] liczoną od kosztu najtańszej opcji —
/// dzięki temu decyzja nie przeskakuje przy każdym drgnięciu korka, a jednocześnie
/// nawyk nie broni się przed różnicą, która naprawdę ma znaczenie.
///
/// Gdy nie ma **ani jednej** wykonalnej opcji, wraca `None`. Wołający musi wtedy
/// dać mieszkańcowi cokolwiek, co się kończy — podróż bez zakończenia zawiesza go
/// do końca gry (`M-4`).
#[must_use]
pub fn evaluate_modes(
    params: &ModeChoiceParams,
    ctx: &ModeContext,
    offers: &[(TravelOption, Result<OptionOffer, Infeasible>)],
) -> Option<ModeDecision> {
    let mut candidates: ArrayVec<Candidate, { TravelOption::ALL.len() }> = ArrayVec::new();
    let mut best: Option<(i64, TravelOption, OptionOffer, GeneralizedCost)> = None;
    let mut brak_parkingu: Option<u16> = None;

    for (option, offer) in offers {
        if candidates.is_full() {
            break;
        }
        match offer {
            Err(powod) => {
                if let Infeasible::NoParkingWithinRadius { lots_searched } = powod {
                    brak_parkingu = Some(*lots_searched);
                }
                let _ = candidates.push(Candidate {
                    option: *option,
                    cost: GeneralizedCost::default(),
                    infeasible: Some(*powod),
                });
            }
            Ok(o) => {
                let cost = params.cost_of(*option, o, ctx);
                let _ = candidates.push(Candidate {
                    option: *option,
                    cost,
                    infeasible: None,
                });
                // Klucz porównania to `(koszt, opcja)` — remisy rozstrzyga kolejność
                // w enumie, a nie kolejność ofert (§5.3).
                if best
                    .as_ref()
                    .is_none_or(|(c, opt, _, _)| (cost.total.0, *option) < (*c, *opt))
                {
                    best = Some((cost.total.0, *option, *o, cost));
                }
            }
        }
    }

    let (najtanszy, _, _, _) = best?;
    // Premia nawyku: liczona od kosztu najtańszej opcji, żeby była proporcjonalna
    // do skali podróży — stała kwota znaczyłaby wszystko przy dojściu do sklepu
    // i nic przy dojeździe przez miasto.
    let premia = najtanszy.abs() * params.habit_bonus_permille / 1_000;
    let mut wybrany = best.expect("sprawdzone wyżej");
    if let Some(h) = ctx.habit {
        for c in candidates.as_slice() {
            if c.option != h || c.infeasible.is_some() {
                continue;
            }
            if c.cost.total.0 - premia < wybrany.0 {
                let oferta = offers
                    .iter()
                    .find(|(o, _)| *o == h)
                    .and_then(|(_, r)| r.as_ref().ok())
                    .copied();
                if let Some(o) = oferta {
                    wybrany = (c.cost.total.0, h, o, c.cost);
                }
            }
        }
    }

    let (_, chosen, offer, _) = wybrany;
    let mut decision = ModeDecision {
        chosen,
        minutes: offer.minutes.max(1),
        money: offer.money,
        parking: offer.parking,
        candidates,
        reason: DecisionReason::ModeChosen {
            mode: chosen.mode(),
            minutes: offer.minutes.max(1),
        },
    };
    // Kolejność ważności uzasadnienia: **brak parkingu przesłania porównanie**.
    // To jest odpowiedź na pytanie z PRD §14.1 — „dlaczego Anna nie kupiła u mnie?"
    // → „bo nie miała gdzie stanąć" — i traci sens, gdy utonie w liście kandydatów.
    decision.reason = match (brak_parkingu, decision.runner_up()) {
        (Some(lots_searched), _) if !chosen.needs_parking() => {
            DecisionReason::NoParkingAtDestination { lots_searched }
        }
        (_, Some((runner_up, delta))) => DecisionReason::ModeCompared {
            chosen: chosen.mode(),
            runner_up: runner_up.mode(),
            delta_gr: delta.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        },
        _ => decision.reason,
    };
    Some(decision)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> ModeChoiceParams {
        ModeChoiceParams::load_default().expect("data/roads/mode_choice.ron")
    }

    fn ctx() -> ModeContext {
        ModeContext {
            citizen: 1,
            age_years: 35,
            status_percentile: 50,
            // 4000 zł netto miesięcznie na ~170 h → ~2350 gr/h.
            hourly_net_income_gr: 2_350,
            purpose: TripPurpose::Work,
            weather: Weather {
                temp_dc: 180,
                precip_permille: 0,
            },
            luggage_kg: 0,
            habit: None,
        }
    }

    fn oferta(minutes: u16, money: i64) -> OptionOffer {
        OptionOffer {
            minutes,
            money: Money(money),
            ..OptionOffer::default()
        }
    }

    #[test]
    fn wartosc_czasu_ma_podloge_i_sufit() {
        let p = params();
        assert_eq!(
            p.vot_gr_per_min(0, TripPurpose::Work),
            p.vot_floor_gr_per_min,
            "czas bezrobotnego nie ma wartości"
        );
        assert_eq!(
            p.vot_gr_per_min(10_000_000, TripPurpose::Work),
            p.vot_cap_gr_per_min
        );
        // Praca jest droższa od zakupów.
        assert!(
            p.vot_gr_per_min(3_000, TripPurpose::Work)
                > p.vot_gr_per_min(3_000, TripPurpose::Shopping)
        );
    }

    #[test]
    fn szybsza_i_tansza_opcja_wygrywa() {
        let p = params();
        let d = evaluate_modes(
            &p,
            &ctx(),
            &[
                (TravelOption::Walk, Ok(oferta(50, 0))),
                (TravelOption::Transit, Ok(oferta(20, 240))),
            ],
        )
        .expect("jest wykonalna opcja");
        assert_eq!(d.chosen, TravelOption::Transit);
        assert!(matches!(
            d.reason,
            DecisionReason::ModeCompared {
                chosen: TransportMode::Bus,
                runner_up: TransportMode::Walk,
                ..
            }
        ));
        assert_eq!(
            d.candidates.len(),
            2,
            "karta inspekcji nie widzi wszystkich"
        );
    }

    #[test]
    fn deszcz_przesuwa_wybor_z_roweru_na_komunikacje() {
        let p = params();
        let sucho = ctx();
        let mut leje = ctx();
        leje.weather = Weather {
            temp_dc: 40,
            precip_permille: 900,
        };
        let oferty = |ctx: &ModeContext| {
            let _ = ctx;
            [
                (
                    TravelOption::Bike,
                    Ok(OptionOffer {
                        minutes: 22,
                        exposed_minutes: 22,
                        ..OptionOffer::default()
                    }),
                ),
                (
                    TravelOption::Transit,
                    Ok(OptionOffer {
                        minutes: 26,
                        money: Money(240),
                        exposed_minutes: 7,
                        ..OptionOffer::default()
                    }),
                ),
            ]
        };
        let a = evaluate_modes(&p, &sucho, &oferty(&sucho)).expect("sucho");
        let b = evaluate_modes(&p, &leje, &oferty(&leje)).expect("deszcz");
        assert_eq!(a.chosen, TravelOption::Bike, "rower przegrał przy pogodzie");
        assert_eq!(
            b.chosen,
            TravelOption::Transit,
            "rowerzysta jedzie w ulewie"
        );
    }

    #[test]
    fn brak_parkingu_odbiera_auto_i_jest_powodem_w_karcie() {
        let p = params();
        let d = evaluate_modes(
            &p,
            &ctx(),
            &[
                (TravelOption::Transit, Ok(oferta(35, 240))),
                (
                    TravelOption::CarOwn,
                    Err(Infeasible::NoParkingWithinRadius { lots_searched: 11 }),
                ),
            ],
        )
        .expect("komunikacja zostaje");
        assert_eq!(d.chosen, TravelOption::Transit);
        assert_eq!(
            d.reason,
            DecisionReason::NoParkingAtDestination { lots_searched: 11 },
            "karta inspekcji nie powie, dlaczego Anna nie przyjechała"
        );
        let auto = d
            .candidates
            .as_slice()
            .iter()
            .find(|c| c.option == TravelOption::CarOwn)
            .expect("auto jest na liście");
        assert!(auto.infeasible.is_some());
    }

    #[test]
    fn brak_wykonalnej_opcji_daje_none() {
        let p = params();
        assert!(evaluate_modes(
            &p,
            &ctx(),
            &[
                (TravelOption::CarOwn, Err(Infeasible::NoCarInHousehold)),
                (TravelOption::Transit, Err(Infeasible::NoTransitConnection)),
            ]
        )
        .is_none());
    }

    #[test]
    fn nawyk_broni_poprzedniego_wyboru_tylko_do_progu() {
        let p = params();
        let mut z_nawykiem = ctx();
        z_nawykiem.habit = Some(TravelOption::Walk);
        // Różnica ~4 % kosztu — nawyk ją przykrywa.
        let blisko = [
            (TravelOption::Walk, Ok(oferta(26, 0))),
            (TravelOption::Transit, Ok(oferta(25, 0))),
        ];
        let d = evaluate_modes(&p, &z_nawykiem, &blisko).expect("wybór");
        assert_eq!(d.chosen, TravelOption::Walk, "nawyk nic nie robi");

        // Różnica dwukrotna — nawyk nie ma prawa jej przykryć.
        let daleko = [
            (TravelOption::Walk, Ok(oferta(50, 0))),
            (TravelOption::Transit, Ok(oferta(25, 0))),
        ];
        let d2 = evaluate_modes(&p, &z_nawykiem, &daleko).expect("wybór");
        assert_eq!(
            d2.chosen,
            TravelOption::Transit,
            "nawyk zwyciężył zdrowy rozsądek"
        );
    }

    #[test]
    fn remis_rozstrzyga_kolejnosc_w_enumie() {
        let p = params();
        let d = evaluate_modes(
            &p,
            &ctx(),
            &[
                (TravelOption::Transit, Ok(oferta(30, 0))),
                (TravelOption::Walk, Ok(oferta(30, 0))),
            ],
        )
        .expect("wybór");
        assert_eq!(
            d.chosen,
            TravelOption::Walk,
            "remis rozstrzygnięty kolejnością ofert"
        );
    }

    #[test]
    fn status_odpycha_od_komunikacji_i_przyciaga_do_auta() {
        let p = params();
        let mut elita = ctx();
        elita.status_percentile = 100;
        // Mieszkaniec z dolnego percentyla statusu i minimalnym dochodem.
        let mut bieda = ctx();
        bieda.status_percentile = 5;
        bieda.hourly_net_income_gr = 900;
        let oferty = [
            (TravelOption::Transit, Ok(oferta(30, 240))),
            (TravelOption::Taxi, Ok(oferta(20, 1_400))),
        ];
        let a = evaluate_modes(&p, &elita, &oferty).expect("wybór");
        let b = evaluate_modes(&p, &bieda, &oferty).expect("wybór");
        assert_eq!(a.chosen, TravelOption::Taxi);
        assert_eq!(b.chosen, TravelOption::Transit);
    }
}
