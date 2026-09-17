//! Tier taktyczny — miesiąc firmy (M7e WP12, M7 §5.6, PRD §12.3).
//!
//! Trzy pytania raz w miesiącu: **czy ten zakład ma sens**, **czy kurs firmy ma sens**
//! i **czy zdelegowany zakład ma w ogóle jakieś reguły**. Ostatnie z nich domyka `AZ-1`:
//! do M7e menedżer dostawał delegację z polityką **pustą**, bo zestaw reguł miała
//! generować taktyka, a taktyki nie było.
//!
//! # Zamknięcie zakładu
//!
//! Kryterium WP12: zakład trwale nierentowny zamyka się w **≤ 3 miesiące gry**.
//! „Trwale" znaczy nieprzerwany ciąg miesięcy zamkniętych stratą, liczony od ostatniego
//! wstecz i przerywany pierwszym miesiącem **bez pomiaru** — zakład bez księgi nie jest
//! zakładem nierentownym, tylko zakładem, o którego wyniku nic nie wiadomo, a to są
//! dwa różne zdania i tylko jedno z nich jest podstawą do zamknięcia.
//!
//! Cierpliwość osobowości przesuwa próg między dwoma a trzema miesiącami i nie dalej —
//! sufit jest kryterium fazy, nie kalibracją ([`crate::FirmPersonality::loss_patience_months`]).

use magnat_core::{DecisionReason, FirmStrategy, SiteId};
use magnat_policy::Decided;
use smallvec::SmallVec;

use crate::view::FirmView;

/// Marża, powyżej której firma ostrożna uznaje, że stać ją na ryzyko (bp utargu).
const EXPANSION_MARGIN_BP: i32 = 1_500;

/// Tolerancja ryzyka, od której firma w ogóle rozważa zmianę kursu na ekspansję.
const EXPANSION_RISK: u8 = 60;

/// Co tier taktyczny może zrobić w ciągu miesiąca (M7 §5.9).
///
/// Lista jest krótsza od planu fazy i to jest świadome. `TakeLoan`, `Repay` i `Factor`
/// mają od M7d własnego wykonawcę, który sam sięga po kredyt obrotowy przy niedoborze
/// (`maybe_borrow_working_capital`) — drugi decydent nad tym samym rachunkiem
/// zaciągałby kredyt dwa razy. `AssignManager` i `StartTraining` mają wykonawcę
/// w `labor::hr` od M7b i chodzą tam **co dobę**, czyli częściej, niż zdążyłby
/// je obejrzeć tier miesięczny. `HireRole`/`LayOff` wynikają z obsady stanowisk,
/// a obsada zmienia się razem z zakładem — zamknięcie zakładu jest tu jedyną
/// decyzją kadrową, bo jedyną, której nikt inny nie podejmuje.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TacAction {
    /// Zamknij zakład: zwolnij załogę, zdejmij go z listy firmy.
    CloseSite { site: SiteId },
    /// Postaw firmę na innym kursie i przepnij presety polityk zdelegowanych zakładów.
    SetStrategy(FirmStrategy),
    /// Przypnij zdelegowanemu zakładowi preset odpowiadający dzisiejszemu kursowi.
    /// Kurs się nie zmienia — zmienia się to, że menedżer ma wreszcie czym kierować.
    AdoptPolicy { site: SiteId },
}

/// Miesiąc firmy.
#[must_use]
pub fn decide_tactical(v: &FirmView) -> SmallVec<[Decided<TacAction>; 6]> {
    let mut out: SmallVec<[Decided<TacAction>; 6]> = SmallVec::new();
    let prog = v.personality.loss_patience_months();

    for s in v.sites {
        if out.len() >= 6 {
            return out;
        }
        if s.months_in_loss >= prog {
            out.push(Decided::new(
                TacAction::CloseSite { site: s.site },
                DecisionReason::SiteClosed {
                    months: s.months_in_loss,
                    // Zakład w ciągu strat ma zmierzoną marżę z definicji — ciąg
                    // przerywa się na pierwszym miesiącu bez pomiaru.
                    margin_bp: s.last_margin_bp.unwrap_or(0),
                },
            ));
        }
    }

    if let Some(kurs) = nowy_kurs(v) {
        if out.len() < 6 {
            out.push(Decided::new(
                TacAction::SetStrategy(kurs),
                DecisionReason::StrategySet {
                    strategy: kurs,
                    prev: v.strategy,
                },
            ));
        }
        // Kurs zmieniony — presety przepina jego wykonanie, więc osobnych
        // `AdoptPolicy` już nie trzeba.
        return out;
    }

    for s in v.sites {
        if out.len() >= 6 {
            break;
        }
        // Zakład zdelegowany bez reguł: menedżer jest, kierować nie ma czym (`AZ-1`).
        if s.delegated && s.needs_policy {
            out.push(Decided::new(
                TacAction::AdoptPolicy { site: s.site },
                DecisionReason::StrategySet {
                    strategy: v.strategy,
                    prev: v.strategy,
                },
            ));
        }
    }
    out
}

/// Kurs, na który firma powinna przejść — albo `None`, jeśli obecny jest w porządku.
///
/// Dwa przejścia i tylko dwa, bo tylko dwa da się dziś uzasadnić **zmierzoną** liczbą:
/// firma, która traci, przechodzi na ostrożny; firma, która zarabia i znosi ryzyko,
/// wychodzi z ostrożnego na ekspansję. Pozostałe kursy wynikają z cech dyrektora
/// i taktyka ich nie podważa — to byłaby osobowość zmieniana miesięcznie, czyli
/// nie osobowość.
fn nowy_kurs(v: &FirmView) -> Option<FirmStrategy> {
    let zmierzone: SmallVec<[i32; 8]> = v.sites.iter().filter_map(|s| s.last_margin_bp).collect();
    if zmierzone.is_empty() {
        return None;
    }
    let suma: i64 = zmierzone.iter().map(|m| i64::from(*m)).sum();
    let srednia = (suma / zmierzone.len() as i64) as i32;

    if srednia < 0 && v.strategy != FirmStrategy::Cautious {
        return Some(FirmStrategy::Cautious);
    }
    if srednia >= EXPANSION_MARGIN_BP
        && v.strategy == FirmStrategy::Cautious
        && v.personality.risk_tolerance >= EXPANSION_RISK
    {
        return Some(FirmStrategy::AggressiveExpansion);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::personality::FirmPersonality;
    use crate::view::{CityFacts, SiteFacts};
    use crate::FirmKey;
    use magnat_core::{DistrictId, Entity, Money, Tick};
    use std::num::NonZeroU32;

    fn zaklad(i: u32) -> SiteId {
        SiteId(Entity::new(i, NonZeroU32::MIN))
    }

    fn fakty(i: u32, strat: u8, marza: Option<i32>) -> SiteFacts {
        SiteFacts {
            site: zaklad(i),
            district: DistrictId(0),
            vacancies: 0,
            headcount: 3,
            months_in_loss: strat,
            last_margin_bp: marza,
            delegated: false,
            needs_policy: false,
            scarcest_role: None,
        }
    }

    fn widok<'a>(p: FirmPersonality, s: FirmStrategy, sites: &'a [SiteFacts]) -> FirmView<'a> {
        FirmView {
            key: FirmKey(1),
            cash: Money(1_000_000),
            personality: p,
            strategy: s,
            margin_floor_bp: 1_000,
            margin_ceiling_bp: 4_000,
            lag_days: 3,
            tick: Tick(0),
            sites,
            goods: &[],
            city: CityFacts::default(),
            outlook: None,
        }
    }

    #[test]
    fn trwale_nierentowny_zaklad_zamyka_sie_najpozniej_po_trzech_miesiacach() {
        for cierpliwosc in 0..=100u8 {
            let mut p = FirmPersonality::NEUTRAL;
            p.patience = cierpliwosc;
            let s = [fakty(1, 3, Some(-820))];
            let d = decide_tactical(&widok(p, FirmStrategy::Cautious, &s));
            assert!(
                d.iter()
                    .any(|x| matches!(x.action(), TacAction::CloseSite { .. })),
                "cierpliwość {cierpliwosc} nie zamknęła zakładu po trzech miesiącach"
            );
        }
    }

    #[test]
    fn jeden_zly_miesiac_nie_zamyka_zakladu() {
        let s = [fakty(1, 1, Some(-500))];
        let d = decide_tactical(&widok(FirmPersonality::NEUTRAL, FirmStrategy::Cautious, &s));
        assert!(!d
            .iter()
            .any(|x| matches!(x.action(), TacAction::CloseSite { .. })));
    }

    #[test]
    fn zaklad_bez_pomiaru_nie_jest_zakladem_nierentownym() {
        // `months_in_loss` liczy wyłącznie miesiące **zmierzone** — zakład bez księgi
        // ma zero i żadna cierpliwość go nie zamknie.
        let s = [fakty(1, 0, None)];
        let d = decide_tactical(&widok(FirmPersonality::NEUTRAL, FirmStrategy::Cautious, &s));
        assert!(d.is_empty());
    }

    #[test]
    fn zamkniecie_zakladu_niesie_powod_z_liczbami() {
        let s = [fakty(1, 3, Some(-820))];
        let d = decide_tactical(&widok(FirmPersonality::NEUTRAL, FirmStrategy::Cautious, &s));
        let r = d[0].reason();
        assert_eq!(
            r,
            DecisionReason::SiteClosed {
                months: 3,
                margin_bp: -820
            }
        );
    }

    #[test]
    fn strata_sprowadza_firme_na_kurs_ostrozny() {
        let s = [fakty(1, 0, Some(-300))];
        let d = decide_tactical(&widok(
            FirmPersonality::NEUTRAL,
            FirmStrategy::AggressiveExpansion,
            &s,
        ));
        assert!(d
            .iter()
            .any(|x| *x.action() == TacAction::SetStrategy(FirmStrategy::Cautious)));
    }

    #[test]
    fn zysk_wypuszcza_ryzykanta_z_kursu_ostroznego_a_ostroznego_nie() {
        let s = [fakty(1, 0, Some(2_000))];
        let mut ryzykant = FirmPersonality::NEUTRAL;
        ryzykant.risk_tolerance = 80;
        let d = decide_tactical(&widok(ryzykant, FirmStrategy::Cautious, &s));
        assert!(d
            .iter()
            .any(|x| *x.action() == TacAction::SetStrategy(FirmStrategy::AggressiveExpansion)));

        let mut bojazliwy = FirmPersonality::NEUTRAL;
        bojazliwy.risk_tolerance = 20;
        let d = decide_tactical(&widok(bojazliwy, FirmStrategy::Cautious, &s));
        assert!(d.is_empty(), "ostrożny nie miał ruszać z miejsca");
    }

    #[test]
    fn zdelegowany_zaklad_bez_regul_dostaje_preset() {
        let mut f = fakty(1, 0, Some(1_000));
        f.delegated = true;
        f.needs_policy = true;
        let s = [f];
        let d = decide_tactical(&widok(FirmPersonality::NEUTRAL, FirmStrategy::Cautious, &s));
        assert!(d
            .iter()
            .any(|x| matches!(x.action(), TacAction::AdoptPolicy { .. })));
    }
}
