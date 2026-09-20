//! Ład korporacyjny: progi, kontrola, dywidenda, emisja (M10d WP10.11, PRD §7.8–§7.9).
//!
//! Wszystko tutaj jest **konsekwencją zmiany akcjonariatu**, a nie decyzją: kto
//! przekroczył próg, musi to ujawnić; kto ma ponad połowę, rządzi; ile wypłacić
//! w dywidendzie, rozstrzyga kurs firmy, a nie ten moduł. Decyzje zapadają
//! w [`super::invest`] (inwestor) i w tierze taktycznym M7e (firma).

use magnat_core::{DecisionReason, Money, Tick};
use magnat_firms::{Firm, FirmKey, Firms, Owner};

use super::{
    move_stake, owner_subject, stake_of, Disclosure, Equity, CONTROL_BP, DISCLOSURE_BP, WHOLE_BP,
};

/// Sprawdza progi po sesji i zgłasza przekroczenia.
///
/// **Zgłasza wyłącznie przekroczenie w górę** i tylko raz na próg: bez pamięci
/// poprzedniego stanu ujawnienie powtarzałoby się każdej doby, w której pakiet stoi
/// powyżej 5 %, a kronika zamieniłaby się w licznik dób.
///
/// Zwraca nowego kontrolującego, gdy zmieniła się kontrola nad firmą.
///
/// **Cech mieszkańca ten moduł nie zna** i nie może: `Personality` jest komponentem,
/// a tu nie ma `&World`. Zwrócony właściciel jest po to, żeby wołający — który świat
/// widzi — dokończył podmianę. Bez tego firma przejęta przez człowieka nigdy nie
/// zmieniałaby kursu, bo `refresh_personalities` woła się **wyłącznie przy stawianiu
/// świata** i w symulacji nie ma wołającego (znalezisko recenzji M10d).
pub fn check_thresholds(
    eq: &mut Equity,
    firms: &mut Firms,
    key: FirmKey,
    dotknieci: &[Owner],
    day: u32,
    t: Tick,
) -> Option<Owner> {
    let firm = firms.get(key)?;
    let mut zmiana_kontroli = None;
    let mut zgloszenia: Vec<Disclosure> = Vec::new();
    for o in dotknieci {
        let teraz = stake_of(firm, *o);
        let przed = eq.known_stake(key, *o);
        if u32::from(teraz) >= DISCLOSURE_BP && u32::from(przed) < DISCLOSURE_BP {
            if let Some(s) = owner_subject(*o) {
                zgloszenia.push(Disclosure {
                    firm: key,
                    holder: s,
                    bp: teraz,
                    control: false,
                    day,
                });
            }
        }
        if u32::from(teraz) > CONTROL_BP && u32::from(przed) <= CONTROL_BP {
            zmiana_kontroli = Some(*o);
            if let Some(s) = owner_subject(*o) {
                zgloszenia.push(Disclosure {
                    firm: key,
                    holder: s,
                    bp: teraz,
                    control: true,
                    day,
                });
            }
        }
    }
    for o in dotknieci {
        let teraz = stake_of(firms.get(key).expect("firma jest"), *o);
        eq.remember_stake(key, *o, teraz);
    }
    for z in &zgloszenia {
        let powod = if z.control {
            DecisionReason::ControlAcquired { holder: z.holder }
        } else {
            DecisionReason::StakeDisclosed { holder: z.holder }
        };
        firms.log(key, t, powod);
        eq.push_disclosure(*z);
    }
    let nowy = zmiana_kontroli?;
    przejmij_zarzad(firms, key, nowy);
    Some(nowy)
}

/// Zmiana zarządu po przekroczeniu połowy udziałów.
///
/// **Podmienia cechy firmy na cechy przejmującego** (kryterium WP10.11): od tej
/// chwili polityka cenowa, gotowość do ekspansji i reakcje konkurencyjne liczą się
/// z czyjegoś innego charakteru, a zmianę widać w ciągu miesiąca gry. Przejmujący
/// będący firmą oddaje własne cechy; przejmujący będący mieszkańcem — cechy
/// wywiedzione z jego osobowości, tak samo jak przy zakładaniu firmy (M7e).
fn przejmij_zarzad(firms: &mut Firms, key: FirmKey, nowy: Owner) {
    let cechy = match nowy {
        Owner::Firm(k) => firms.get(k).map(|f| f.personality),
        // Mieszkaniec-właściciel staje się dyrektorem, a jego cechy przepisuje
        // wołający przez [`set_personality_from_citizen`] — tylko on widzi komponent.
        _ => None,
    };
    let Some(firm) = firms.get_mut(key) else {
        return;
    };
    if let Owner::Citizen(c) = nowy {
        firm.director = Some(c);
    }
    if let Some(p) = cechy {
        firm.set_personality(p);
    }
}

/// Przepisuje cechy mieszkańca na firmę, którą właśnie przejął.
///
/// Osobno od [`check_thresholds`], bo `Personality` jest komponentem świata, a tamta
/// funkcja dostaje sam rejestr firm. Ta sama droga, którą M7a wyprowadza cechy
/// z dyrektora przy zakładaniu firmy.
pub fn set_personality_from_citizen(
    firms: &mut Firms,
    key: FirmKey,
    p: magnat_firms::FirmPersonality,
) {
    if let Some(f) = firms.get_mut(key) {
        f.set_personality(p);
    }
}

/// Ile firma wypłaca w dywidendzie za miesiąc.
///
/// Z zysku **opublikowanego**, a nie z salda konta: firma z pustym kontem i dodatnim
/// wynikiem nie ma z czego wypłacić, więc kwota jest przycięta do tego, co realnie
/// leży na rachunku — a decyzję „czy w ogóle" podejmuje kurs firmy.
#[must_use]
pub fn dividend_amount(profit_month: Money, cash: Money, payout_bp: u16) -> Money {
    if profit_month.get() <= 0 || cash.get() <= 0 {
        return Money::ZERO;
    }
    let chciana = profit_month.get().saturating_mul(i64::from(payout_bp)) / i64::from(WHOLE_BP);
    Money(chciana.min(cash.get()).max(0))
}

/// Kto ma prawo poboru: największy dotychczasowy właściciel.
///
/// PRD §7.8 mówi o prawie poboru i to jest jego najtańsza uczciwa postać: nowa
/// emisja idzie najpierw do tego, kto ma najwięcej do stracenia na rozwodnieniu.
/// Dopiero gdy nie chce albo nie może zapłacić, pakiet trafia na rynek.
#[must_use]
pub fn preemptive_holder(firm: &Firm) -> Option<Owner> {
    firm.owners
        .iter()
        .max_by_key(|s| (s.bp, std::cmp::Reverse(super::owner_key(s.owner))))
        .map(|s| s.owner)
}

/// Ile bp trzeba wypuścić, żeby broniący **odzyskał kontrolę**.
///
/// Po emisji `x` bp na jego rzecz ma `d × (10 000 − x) / 10 000 + x`, bo rozwadnia
/// się razem z resztą, a potem obejmuje nowy pakiet w całości. Szuka się najmniejszego
/// `x`, przy którym to przekracza [`CONTROL_BP`].
///
/// Sufit 2 000 bp jest jawny i nie jest ostrożnością: emisja większa niż piąta część
/// firmy rozwadnia **wszystkich** dotychczasowych właścicieli mocniej, niż warta jest
/// obrona. Zero znaczy „obrona nic nie da" — broniący jest za daleko od połowy,
/// żeby dosypanie w widełkach cokolwiek zmieniło.
#[must_use]
pub fn defence_issue_bp(broniacy_bp: u16) -> u16 {
    let d = u64::from(broniacy_bp);
    for x in 1..=2_000u64 {
        if d * (u64::from(WHOLE_BP) - x) / u64::from(WHOLE_BP) + x > u64::from(CONTROL_BP) {
            return x as u16;
        }
    }
    0
}

/// Rozwadnia firmę o `bp` na rzecz `to` i zwraca kwotę, którą trzeba zapłacić.
///
/// Cena jest kursem ostatniego fixingu: emisja po cenie wyższej nie znalazłaby
/// nabywcy, a po niższej byłaby darowizną kosztem pozostałych właścicieli.
/// Sam przelew robi wołający — ten moduł nie zna ksiąg.
#[must_use]
pub fn issue_price(bp: u16, last_fixing: Money) -> Money {
    Money(last_fixing.get().saturating_mul(i64::from(bp)))
}

/// Wykonuje emisję w rejestrze firm. Zwraca `false`, gdy emisja jest niemożliwa.
pub fn issue(firms: &mut Firms, key: FirmKey, to: Owner, bp: u16, price: Money, t: Tick) -> bool {
    let Some(firm) = firms.get_mut(key) else {
        return false;
    };
    if !super::dilute(firm, to, bp) {
        return false;
    }
    firms.log(key, t, DecisionReason::SharesIssued { bp, price });
    true
}

/// Przenosi pakiet i pilnuje, żeby suma została nienaruszona.
///
/// Cienka nakładka na [`move_stake`] — istnieje po to, żeby fixing i wezwanie
/// chodziły **jedną** drogą do akcjonariatu, a nie dwiema.
pub fn settle_stake(firms: &mut Firms, key: FirmKey, from: Owner, to: Owner, bp: u16) -> u16 {
    let Some(firm) = firms.get_mut(key) else {
        return 0;
    };
    let ile = move_stake(firm, from, to, bp);
    debug_assert!(firm.owners_sum_ok(), "emisja albo obrót zgubiły udział");
    ile
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::{CitizenId, Entity, SimMinute};
    use smallvec::smallvec;

    fn obywatel(i: u32) -> Owner {
        Owner::Citizen(CitizenId(Entity::new(i, std::num::NonZeroU32::MIN)))
    }

    fn firma() -> Firm {
        Firm::sole_owner(
            FirmKey(1),
            "Test".to_string(),
            SimMinute(0),
            magnat_core::DistrictId(0),
            obywatel(1),
        )
    }

    #[test]
    fn prawo_poboru_ma_najwiekszy_wlasciciel() {
        let mut f = firma();
        f.owners = smallvec![
            magnat_firms::OwnerShare {
                owner: obywatel(1),
                bp: 4_000
            },
            magnat_firms::OwnerShare {
                owner: obywatel(2),
                bp: 6_000
            },
        ];
        assert_eq!(preemptive_holder(&f), Some(obywatel(2)));
    }

    #[test]
    fn emisja_obronna_oddaje_kontrole_broniacemu() {
        // Broniący z 4 800 bp: dosypanie 205 bp daje 4 800 × 0,9795 + 205 = 5 106.
        let x = defence_issue_bp(4_800);
        assert!(x > 0 && x <= 2_000, "{x}");
        let po = |x: u64| 4_800u64 * (10_000 - x) / 10_000 + x;
        assert!(po(u64::from(x)) > 5_000);
        // Najmniejsza, jaka wystarcza.
        assert!(po(u64::from(x) - 1) <= 5_000);
        // Kto ma dokładnie połowę, potrzebuje dwóch bp: pierwsza tylko odrabia
        // własne rozwodnienie (5 000 → 4 999 + 1 = 5 000, czyli wciąż nie „ponad").
        assert_eq!(defence_issue_bp(5_000), 2);
        // Kto jest za daleko, nie obroni się emisją w widełkach.
        assert_eq!(defence_issue_bp(1_000), 0);
    }

    #[test]
    fn dywidenda_nie_przekracza_salda() {
        assert_eq!(
            dividend_amount(Money(1_000_000), Money(100_000), 3_000),
            Money(100_000)
        );
        assert_eq!(
            dividend_amount(Money(1_000_000), Money(10_000_000), 3_000),
            Money(300_000)
        );
        assert_eq!(
            dividend_amount(Money(-5), Money(10_000_000), 3_000),
            Money::ZERO
        );
    }
}
