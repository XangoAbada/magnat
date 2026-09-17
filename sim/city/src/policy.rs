//! Uchwały rady: co miasto może postanowić i od kiedy to obowiązuje (M8e §5.2).
//!
//! Trzy reguły, na których stoi ten moduł:
//!
//! 1. **Uchwała ma zawsze czytelnika.** Wariant [`Policy`], którego skutku nikt
//!    nie widzi, przechodzi każdy test i w histogramie wygląda tak samo jak
//!    wariant działający (`R2`). Przy każdym wariancie stoi nazwa miejsca, które
//!    go czyta — żeby dopisanie ósmego zaczynało się od pytania „kto to
//!    przeczyta", a nie kończyło na nim. Dlatego wariantów jest siedem, a nie
//!    jedenaście: strefowanie, opłata parkingowa i ograniczenie ruchowe mają
//!    powód nieobecności zapisany w `PolicyKind` (`engine/core`) i w tabeli
//!    korekt (`CI-1`, `CI-2`, `CI-4`).
//! 2. **Między uchwaleniem a wejściem w życie mija czas.** `effective_from` nigdy
//!    nie równa się `enacted_at`: gracz ma mieć kwartał na reakcję na podwyżkę
//!    VAT-u, a nie dowiedzieć się o niej z rachunku. To jest też jedyna obrona
//!    przed burmistrzem, który zmienia zdanie co miesiąc (T5).
//! 3. **Uchwała o tym samym przedmiocie wypiera poprzednią**, a nie dokłada się
//!    do niej. Bez tego dwie uchwały o stawce VAT obowiązywałyby naraz i wygrywała
//!    ta, którą kod zobaczył ostatnią — czyli kolejność w wektorze zamieniłaby się
//!    w regułę prawa.

use magnat_core::{
    AgencyKind, DecisionReason, DistrictId, Money, OpenHours, PolicyKind, SpendCategory,
    StateHasher, TaxKind, Tick, UtilityService, SPEND_CATEGORY_COUNT,
};

/// Co rada może uchwalić.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Policy {
    /// Stawka daniny. Czyta: [`crate::rule::nalozenie`], które przebudowuje
    /// `TaxCode` i wstawia nowy silnik podatkowy do rynku.
    TaxRate { kind: TaxKind, bps: u32 },
    /// Udział kierunku wydatku w planie miesięcznym, w punktach bazowych.
    ///
    /// **Jeden wariant na finansowanie usług i na dotacje**, a nie dwa, i to jest
    /// wykonanie §5.2 w postaci, w której pieniądz rzeczywiście się rusza:
    /// udział wchodzi do `budget::close_month` (czyli do przelewu) **i** do
    /// `services::update_quality` (czyli do jakości placówki) przez jedną
    /// funkcję [`PolicySet::shares`]. Osobna kwota „na placówkę" wyglądałaby
    /// w karcie rady tak samo, ale podnosiłaby jakość, nie wydając ani grosza.
    SpendShare { category: SpendCategory, bp: u32 },
    /// Płaca minimalna. Czyta: `law::inspekcja_pracy` (`CH-5`).
    MinWage(Money),
    /// Limit emisji pyłu na zakład, w **gramach na minutę** — w tych samych
    /// jednostkach, w których liczy go `sim/supply`. Czyta: `law::ochrona_srodowiska`.
    EmissionLimit { max_g_per_min: i64 },
    /// Sufit taryfy operatora sieci. Czyta: [`crate::rule::nalozenie`], które
    /// przycina `UtilityNetwork.tariff.per_unit` i pamięta wartość sprzed uchwały.
    TariffCap {
        service: UtilityService,
        max_per_unit: Money,
    },
    /// Obsada urzędu kontrolnego. Czyta: `Enforcement::set_inspectors` (`CH-6`).
    AgencyStaffing { agency: AgencyKind, inspectors: u32 },
    /// Godziny handlu w dzielnicy. Czyta: `Market::set_trading_hours`.
    ///
    /// **Niesie `OpenHours` z `engine/core`, a nie własną siatkę tygodniową**,
    /// i to jest wybór, nie niedokładność: sklep, mieszkaniec i rampa magazynu
    /// mówią o godzinach otwarcia tym typem od M3 (`K-34`), więc uchwała wyrażona
    /// drugim typem wymagałaby konwersji, która gubi to, czego `OpenHours` nie
    /// umie wyrazić — a wtedy uchwała znaczyłaby co innego niż to, co widać
    /// w sklepie.
    TradingHours {
        district: DistrictId,
        hours: OpenHours,
    },
}

impl Policy {
    #[must_use]
    pub fn kind(&self) -> PolicyKind {
        match self {
            Policy::TaxRate { .. } => PolicyKind::TaxRate,
            Policy::SpendShare { .. } => PolicyKind::SpendShare,
            Policy::MinWage(_) => PolicyKind::MinWage,
            Policy::EmissionLimit { .. } => PolicyKind::EmissionLimit,
            Policy::TariffCap { .. } => PolicyKind::TariffCap,
            Policy::AgencyStaffing { .. } => PolicyKind::AgencyStaffing,
            Policy::TradingHours { .. } => PolicyKind::TradingHours,
        }
    }

    /// Klucz zastępowania: nowa uchwała o tym samym kluczu **wypiera** poprzednią.
    ///
    /// Klucz jest parą (rodzaj, przedmiot), bo dwie dzielnice mogą mieć różne
    /// godziny handlu, ale jedna dzielnica nie może mieć dwóch.
    #[must_use]
    pub fn slot(&self) -> (PolicyKind, u32) {
        let przedmiot = match self {
            Policy::TaxRate { kind, .. } => kind.as_index() as u32,
            Policy::SpendShare { category, .. } => category.as_index() as u32,
            Policy::AgencyStaffing { agency, .. } => agency.as_index() as u32,
            Policy::TariffCap { service, .. } => service.as_index() as u32,
            Policy::TradingHours { district, .. } => u32::from(district.0),
            // Płaca minimalna i limit emisji są jedne na miasto — przedmiotu nie mają.
            Policy::MinWage(_) | Policy::EmissionLimit { .. } => 0,
        };
        (self.kind(), przedmiot)
    }

    /// Liczbowa treść uchwały jako jedna kwota — do hasza i do raportu.
    ///
    /// Jedna liczba zamiast pięciu pól, bo hasz ma wykryć **zmianę**, a nie
    /// odtworzyć treść; treść odtwarza się z samej uchwały.
    #[must_use]
    pub fn amount(&self) -> Money {
        match self {
            Policy::TaxRate { bps: v, .. } | Policy::SpendShare { bp: v, .. } => {
                Money(i64::from(*v))
            }
            Policy::MinWage(m) => *m,
            Policy::EmissionLimit { max_g_per_min } => Money(*max_g_per_min),
            Policy::TariffCap { max_per_unit, .. } => *max_per_unit,
            Policy::AgencyStaffing { inspectors, .. } => Money(i64::from(*inspectors)),
            Policy::TradingHours { hours, .. } => Money(
                i64::from(hours.open.get()) * 100_000
                    + i64::from(hours.close.get()) * 10
                    + i64::from(hours.days),
            ),
        }
    }
}

/// Wynik głosowania rady.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct CouncilVote {
    /// Poparcie w punktach bazowych mandatów. `>= 5000` znaczy uchwalone.
    pub for_bp: u16,
}

impl CouncilVote {
    #[must_use]
    pub const fn passed(&self) -> bool {
        self.for_bp >= 5_000
    }
}

/// Uchwała wraz z datami i powodem.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PolicyRecord {
    pub policy: Policy,
    pub enacted_at: Tick,
    /// Zawsze `> enacted_at` — vacatio legis z `data/city/government.ron`.
    pub effective_from: Tick,
    pub sunset: Option<Tick>,
    pub vote: CouncilVote,
    pub reason: DecisionReason,
}

impl PolicyRecord {
    #[must_use]
    pub fn in_force(&self, t: Tick) -> bool {
        t.0 >= self.effective_from.0 && self.sunset.is_none_or(|s| t.0 < s.0)
    }
}

/// Zbiór uchwał miasta — pełna historia, nie tylko obowiązujące.
///
/// Historia zostaje, bo karta rady ma odpowiadać na „kiedy to weszło i kto to
/// przegłosował", a uchwała skasowana po wygaśnięciu zabiera ze sobą odpowiedź.
/// Koszt jest znikomy: dwadzieścia lat gry to rzędu dwustu wpisów.
#[derive(Clone, Default, Debug)]
pub struct PolicySet {
    records: Vec<PolicyRecord>,
}

impl PolicySet {
    #[must_use]
    pub fn new() -> PolicySet {
        PolicySet::default()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    #[must_use]
    pub fn all(&self) -> &[PolicyRecord] {
        &self.records
    }

    /// Wpisuje uchwałę. Poprzednia o tym samym slocie dostaje `sunset` na chwilę
    /// wejścia nowej — nie znika, bo historia ma zostać czytelna.
    pub fn enact(&mut self, record: PolicyRecord) {
        let slot = record.policy.slot();
        for r in &mut self.records {
            if r.policy.slot() == slot && r.sunset.is_none() {
                r.sunset = Some(record.effective_from);
            }
        }
        self.records.push(record);
    }

    /// Uchwały obowiązujące w chwili `t`, w kolejności uchwalenia.
    pub fn in_force(&self, t: Tick) -> impl Iterator<Item = &PolicyRecord> {
        self.records.iter().filter(move |r| r.in_force(t))
    }

    /// Ostatnia obowiązująca uchwała danego rodzaju o danym przedmiocie.
    #[must_use]
    pub fn current(&self, kind: PolicyKind, subject: u32, t: Tick) -> Option<&Policy> {
        self.in_force(t)
            .filter(|r| r.policy.slot() == (kind, subject))
            .map(|r| &r.policy)
            .last()
    }

    /// Udziały planu wydatków po nałożeniu obowiązujących uchwał.
    ///
    /// **Jedno miejsce dla pieniądza i dla jakości.** Ten sam wynik czyta
    /// `budget::close_month` (ile miasto przelewa) i `services::update_quality`
    /// (jak dobra jest przez to placówka), więc uchwała podnosząca jakość szkoły
    /// **zawsze** kosztuje tyle, ile podnosi. Druga droga do jednej z tych dwóch
    /// liczb byłaby drogą do jakości za darmo.
    #[must_use]
    pub fn shares(
        &self,
        base: [u32; SPEND_CATEGORY_COUNT],
        t: Tick,
    ) -> [u32; SPEND_CATEGORY_COUNT] {
        let mut out = base;
        for r in self.in_force(t) {
            if let Policy::SpendShare { category, bp } = &r.policy {
                out[category.as_index()] = *bp;
            }
        }
        out
    }

    pub fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.records.len() as u64);
        for r in &self.records {
            h.write_u16(r.policy.kind().as_index() as u16);
            h.write_u32(r.policy.slot().1);
            h.write_u64(r.enacted_at.0);
            h.write_u64(r.effective_from.0);
            h.write_u64(r.sunset.map_or(u64::MAX, |s| s.0));
            h.write_u16(r.vote.for_bp);
            // Kwota uchwały wchodzi do hasza, bo to ona zmienia wynik gry.
            h.write_i64(r.policy.amount().get());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uchwala(policy: Policy, at: u64) -> PolicyRecord {
        PolicyRecord {
            policy,
            enacted_at: Tick(at),
            effective_from: Tick(at + 100),
            sunset: None,
            vote: CouncilVote { for_bp: 6_000 },
            reason: DecisionReason::Unspecified,
        }
    }

    #[test]
    fn nowa_uchwala_wypiera_poprzednia_o_tym_samym_slocie() {
        let mut set = PolicySet::new();
        set.enact(uchwala(
            Policy::TaxRate {
                kind: TaxKind::Vat,
                bps: 2_300,
            },
            0,
        ));
        set.enact(uchwala(
            Policy::TaxRate {
                kind: TaxKind::Vat,
                bps: 1_800,
            },
            1_000,
        ));
        // Przed wejściem drugiej obowiązuje pierwsza.
        assert!(matches!(
            set.current(
                PolicyKind::TaxRate,
                TaxKind::Vat.as_index() as u32,
                Tick(500)
            ),
            Some(Policy::TaxRate { bps: 2_300, .. })
        ));
        // Po wejściu drugiej — tylko druga, mimo że obie są w historii.
        assert_eq!(set.in_force(Tick(2_000)).count(), 1);
        assert!(matches!(
            set.current(
                PolicyKind::TaxRate,
                TaxKind::Vat.as_index() as u32,
                Tick(2_000)
            ),
            Some(Policy::TaxRate { bps: 1_800, .. })
        ));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn dwie_dzielnice_moga_miec_rozne_godziny_handlu() {
        let mut set = PolicySet::new();
        set.enact(uchwala(
            Policy::TradingHours {
                district: DistrictId(0),
                hours: OpenHours::new(360, 1_200, OpenHours::ALL_DAYS),
            },
            0,
        ));
        set.enact(uchwala(
            Policy::TradingHours {
                district: DistrictId(1),
                hours: OpenHours::new(480, 1_080, 0b001_1111),
            },
            0,
        ));
        assert_eq!(set.in_force(Tick(200)).count(), 2);
    }

    /// Udział z uchwały wchodzi **do jednej tablicy**, którą czytają i przelew,
    /// i jakość placówki. Test pilnuje, że nie ma drugiej drogi do żadnej z nich.
    #[test]
    fn uchwala_o_wydatkach_przestawia_udzial_planu() {
        let mut set = PolicySet::new();
        let baza = [1_000u32; SPEND_CATEGORY_COUNT];
        assert_eq!(set.shares(baza, Tick(0)), baza);
        set.enact(uchwala(
            Policy::SpendShare {
                category: SpendCategory::Education,
                bp: 2_500,
            },
            0,
        ));
        // Przed wejściem w życie plan się nie rusza.
        assert_eq!(
            set.shares(baza, Tick(50))[SpendCategory::Education.as_index()],
            1_000
        );
        let po = set.shares(baza, Tick(200));
        assert_eq!(po[SpendCategory::Education.as_index()], 2_500);
        assert_eq!(po[SpendCategory::Health.as_index()], 1_000);
    }
}
