//! Kto i dlaczego składa zlecenie (M10d WP10.10, PRD §6.5).
//!
//! Inwestorów są dwa rodzaje i oba już istnieją w świecie:
//!
//! - **zamożne gospodarstwo** — kupuje, gdy uważa firmę za wartą więcej, niż kosztuje,
//!   i sprzedaje w odwrotnej sytuacji. Skłonność do ryzyka bierze z cechy `Risk`
//!   (M3 §5.1), a nie z nowego parametru;
//! - **firma** — kupuje konkurenta, gdy jej kurs (`FirmStrategy`) każe rosnąć przez
//!   przejęcie, a nie przez budowę. To jest wezwanie: limit z premią ponad kurs,
//!   bo inaczej nikt nie sprzeda pakietu, którym rządzi.
//!
//! Decyzja zapada **raz w miesiącu** dla gospodarstwa i **raz w tygodniu** dla firmy,
//! a dzień wypada z indeksu encji — nie dlatego, że tak jest realistycznie, tylko
//! dlatego, że inaczej cały rynek podejmowałby decyzję w tej samej dobie i fixing
//! byłby raz w miesiącu z wolumenem całego miasta.

use magnat_core::FirmStrategy;
use magnat_core::{Money, Tick};
use magnat_firms::Owner;

use super::book::Side;

/// Czy w tej dobie gospodarstwo o tym indeksie podejmuje decyzję inwestycyjną.
#[must_use]
pub fn household_decides(day: u32, citizen_index: u32) -> bool {
    day % 30 == citizen_index % 30
}

/// Czy w tej dobie firma o tym kluczu rozgląda się za przejęciem.
#[must_use]
pub fn firm_decides(day: u32, key: u64) -> bool {
    day % 7 == (key % 7) as u32
}

/// Czy kurs firmy każe jej rosnąć **szybciej, niż pozwala własny zysk**.
///
/// Jeden predykat na dwie decyzje i to jest rozmyślne: firma, która chce rosnąć
/// cudzym kapitałem, jest tą samą firmą, która chce rosnąć cudzym zakładem.
/// Wchodzi więc na giełdę **i** rozgląda się za przejęciem.
///
/// Trzy z sześciu kursów, i to nie jest dobór z gustu: konsolidator kupuje z definicji,
/// ekspansja szuka najtańszej drogi do skali, a innowator kupuje wtedy, gdy taniej
/// jest przejąć cudze laboratorium niż zbudować własne (M10c). Ostrożny, dyskontowy
/// i niszowy nie robią ani jednego, ani drugiego — rosną półką.
#[must_use]
pub fn growth_minded(s: FirmStrategy) -> bool {
    matches!(
        s,
        FirmStrategy::Consolidator | FirmStrategy::AggressiveExpansion | FirmStrategy::Innovative
    )
}

/// Zamiar inwestora: co, po ile i ile.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Intent {
    pub holder: Owner,
    pub side: Side,
    pub limit: Money,
    pub bp: u16,
}

/// Zamiar zamożnego gospodarstwa wobec jednej spółki.
///
/// `None` znaczy „nic nie robi" i tak jest w większości przypadków — inwestor,
/// którego przekonanie mieści się w widełkach kursu, nie ma po co składać zlecenia.
/// Widełki są konieczne: bez nich każdy rzut kostką dawałby transakcję, a wolumen
/// mierzyłby szum, a nie różnicę zdań.
#[must_use]
pub fn household_intent(
    holder: Owner,
    belief: Money,
    kurs: Money,
    ma_bp: u16,
    budzet: Money,
    ryzyko: u8,
) -> Option<Intent> {
    if belief.get() <= 0 || kurs.get() <= 0 {
        return None;
    }
    // Próg opłacalności maleje ze skłonnością do ryzyka: ostrożny chce 20 % zapasu,
    // ryzykant wchodzi przy 5 %.
    let prog_bp = 2_000 - i64::from(ryzyko.min(100)) * 15;
    let gora = kurs.get() + kurs.get() * prog_bp / 10_000;
    let dol = kurs.get() - kurs.get() * prog_bp / 10_000;
    if belief.get() > gora {
        let ile = (budzet.get() / belief.get().max(1)).clamp(0, i64::from(u16::MAX)) as u16;
        let ile = ile.min(1_000);
        if ile == 0 {
            return None;
        }
        return Some(Intent {
            holder,
            side: Side::Buy,
            limit: belief,
            bp: ile,
        });
    }
    if belief.get() < dol && ma_bp > 0 {
        return Some(Intent {
            holder,
            side: Side::Sell,
            limit: belief,
            bp: ma_bp.clamp(1, 1_000),
        });
    }
    None
}

/// Zamiar firmy szukającej przejęcia — czyli wezwanie.
///
/// Limit jest kursem powiększonym o premię (plan §5.5: zwykle 15–35 %), bo pakiet
/// dający kontrolę nie leży na rynku za tyle, co pakiet, który jej nie daje.
/// Wielkość zlecenia to dystans do progu kontroli, a nie „ile się da": przejęcie ma
/// być decyzją o firmie, a nie zbieractwem.
#[must_use]
pub fn takeover_intent(
    holder: Owner,
    kurs: Money,
    ma_bp: u16,
    stac_na: Money,
    premia_bp: u16,
) -> Option<Intent> {
    if kurs.get() <= 0 || u32::from(ma_bp) > super::CONTROL_BP {
        return None;
    }
    let limit = Money(kurs.get() + kurs.get() * i64::from(premia_bp) / 10_000);
    let brakuje = (super::CONTROL_BP + 1 - u32::from(ma_bp)) as u16;
    let stac = (stac_na.get() / limit.get().max(1)).clamp(0, i64::from(u16::MAX)) as u16;
    let bp = brakuje.min(stac);
    if bp == 0 {
        return None;
    }
    Some(Intent {
        holder,
        side: Side::Buy,
        limit,
        bp,
    })
}

/// Tick doby, w której inwestor podejmuje decyzję — klucz szumu przekonania.
#[must_use]
pub fn decision_tick(day: u32) -> Tick {
    Tick(u64::from(day) * 60 * 24)
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::{CitizenId, Entity};

    fn kto() -> Owner {
        Owner::Citizen(CitizenId(Entity::new(3, std::num::NonZeroU32::MIN)))
    }

    #[test]
    fn inwestor_kupuje_gdy_wierzy_w_wiecej_niz_kurs() {
        let i = household_intent(kto(), Money(150), Money(100), 0, Money(100_000), 50)
            .expect("zamiar jest");
        assert_eq!(i.side, Side::Buy);
        assert_eq!(i.limit, Money(150));
    }

    #[test]
    fn inwestor_nie_rusza_sie_w_widelkach() {
        assert!(household_intent(kto(), Money(101), Money(100), 10, Money(100_000), 50).is_none());
    }

    #[test]
    fn ryzykant_wchodzi_wczesniej_od_ostroznego() {
        let ostrozny = household_intent(kto(), Money(112), Money(100), 0, Money(100_000), 0);
        let ryzykant = household_intent(kto(), Money(112), Money(100), 0, Money(100_000), 100);
        assert!(ostrozny.is_none(), "przy progu 20 % nic nie robi");
        assert!(ryzykant.is_some(), "przy progu 5 % wchodzi");
    }

    #[test]
    fn inwestor_sprzedaje_tylko_to_co_ma() {
        assert!(household_intent(kto(), Money(50), Money(100), 0, Money(0), 50).is_none());
        let i = household_intent(kto(), Money(50), Money(100), 40, Money(0), 50).expect("zamiar");
        assert_eq!(i.side, Side::Sell);
        assert_eq!(i.bp, 40);
    }

    #[test]
    fn wezwanie_celuje_dokladnie_w_kontrole() {
        let i = takeover_intent(kto(), Money(100), 2_000, Money(100_000_000), 2_500)
            .expect("wezwanie jest");
        assert_eq!(i.limit, Money(125));
        assert_eq!(u32::from(i.bp) + 2_000, super::super::CONTROL_BP + 1);
    }

    #[test]
    fn kto_ma_kontrole_nie_wzywa() {
        assert!(takeover_intent(kto(), Money(100), 6_000, Money(100_000_000), 2_500).is_none());
    }

    #[test]
    fn dni_decyzji_rozkladaja_sie_po_indeksach() {
        let ile = (0..30u32).filter(|d| household_decides(*d, 7)).count();
        assert_eq!(ile, 1, "gospodarstwo decyduje raz na trzydzieści dób");
        let ile = (0..7u32).filter(|d| firm_decides(*d, 11)).count();
        assert_eq!(ile, 1, "firma raz na tydzień");
    }
}
