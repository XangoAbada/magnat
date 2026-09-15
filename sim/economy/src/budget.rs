//! Budżet gospodarstwa domowego — budżetowanie kopertowe (M5d §5.9, PRD §5.2).
//!
//! **Saldo gospodarstwa nie jest kontem w `Books`.** Mieszka w komponencie
//! `magnat_agents::Household` (własność M3), a księgi widzą je przez kanał
//! `MoneySupplyLedger.household_sector_in/out` (`U-17`). Budżet dokłada do tego
//! koperty, koszty stałe, kredyt i zaległości — a pola pieniężne **czyta i pisze
//! w komponencie**, bo dwa źródła salda rozjeżdżają się przy pierwszej transakcji.
//!
//! Koperta jest dwiema rzeczami naraz:
//!
//! - **limitem wydatku** — wyczerpana podnosi próg odłożenia zakupu przez człon
//!   `k_envelope` z `choice.ron` (`purchase_threshold`), czyli odróżnia „nie stać
//!   mnie" od „nie warto";
//! - **mianownikiem członu ceny** — `f(cena / budget_ref)` nie ma bez niej definicji.
//!
//! Rząd wielkości tego mianownika **decyduje o tym, czy cena w ogóle waży**
//! (korekta `V-9` z M5b): koperta miesięczna zamiast kwoty „na jedno wyjście po
//! zakupy" spłaszcza człon ceny do zera i rynek staje się losowaniem. Dlatego
//! dzielnikiem jest **liczba zakupów w kategorii w miesiącu**, a nie liczba wyjść:
//! gospodarstwo kupuje każdy towar kategorii osobno, więc `towary × 30/purchase_days`.

use magnat_agents::HouseholdKind;
use magnat_core::{
    split_proportional, FixedCost, HashState, Money, StateHasher, StockCat, FIXED_COST_COUNT, Q,
    STOCK_CAT_COUNT,
};

use crate::books::LoanId;
use crate::data::EconomyData;
use crate::kernel::BP;

/// Jedna koperta wydatkowa.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Envelope {
    pub allocated: Money,
    pub spent: Money,
    /// Ile zakupów w tej kategorii gospodarstwo planuje w miesiącu. Zero znaczy
    /// „kategoria bez asortymentu w tym mieście" i włącza wartość zapasową z danych.
    pub expected_purchases: u16,
}

impl Envelope {
    /// Mianownik członu ceny: koperta na jeden zakup.
    #[must_use]
    pub fn per_purchase(&self) -> Money {
        if self.expected_purchases == 0 {
            return Money::ZERO;
        }
        self.allocated
            .div_round_half_up(i64::from(self.expected_purchases))
    }

    /// Przekroczenie koperty w punktach bazowych; ujemne, dopóki są środki.
    /// To jest wejście członu `k_envelope` w progu odłożenia zakupu (§5.4).
    #[must_use]
    pub fn overspend_bp(&self) -> i32 {
        if self.allocated.get() <= 0 {
            // Koperta zerowa jest wyczerpana z definicji — inaczej brak przydziału
            // czytałby się jak nieograniczony budżet.
            return if self.spent.get() > 0 { BP as i32 } else { 0 };
        }
        let d = (self.spent.get() - self.allocated.get()) * BP / self.allocated.get();
        i32::try_from(d).unwrap_or(i32::MAX)
    }
}

/// Budżet jednego gospodarstwa.
///
/// `account` i `cash` z planu §5.9 **nie powstały jako `AccountId`** (`U-17`):
/// saldo jest w komponencie. `loans: Vec<LoanId>` zwęziło się do jednego kredytu
/// konsumpcyjnego — patrz korekta `Y-2` w dokumencie podfazy.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct HouseholdBudget {
    pub income_monthly: Money,
    pub fixed: [Money; FIXED_COST_COUNT],
    pub envelopes: [Envelope; STOCK_CAT_COUNT],
    pub savings_target_bp: i32,
    /// Zaległości; `> 0` psuje scoring i podnosi `arrears_months`.
    pub arrears: Money,
    /// Miesiące, w których czegoś zabrakło — historia kredytowa M5.
    pub arrears_months: u8,
    pub loan: Option<LoanId>,
    /// `false`, dopóki gospodarstwo nie przeżyło pierwszej granicy miesiąca.
    pub planned: bool,
}

impl HouseholdBudget {
    /// Suma kosztów stałych.
    #[must_use]
    pub fn fixed_total(&self) -> Money {
        Money(self.fixed.iter().map(|m| m.get()).sum())
    }

    /// Suma przydziałów kopert.
    #[must_use]
    pub fn allocated_total(&self) -> Money {
        Money(self.envelopes.iter().map(|e| e.allocated.get()).sum())
    }

    /// Ile gospodarstwo wydało w tym miesiącu ponad przydział, w groszach.
    #[must_use]
    pub fn envelope(&self, cat: StockCat) -> Envelope {
        self.envelopes[cat.as_index()]
    }

    /// Dopisuje wydatek do koperty. Woła to rozliczenie transakcji, a nie decyzja —
    /// koperta ma śledzić pieniądze, które **wyszły**, a nie te, które ktoś rozważył.
    pub fn charge(&mut self, cat: StockCat, amount: Money) {
        let e = &mut self.envelopes[cat.as_index()];
        e.spent = Money(e.spent.get().saturating_add(amount.get()));
    }

    pub(crate) fn hash_state(&self, h: &mut StateHasher) {
        self.income_monthly.hash_state(h);
        for m in &self.fixed {
            m.hash_state(h);
        }
        for e in &self.envelopes {
            e.allocated.hash_state(h);
            e.spent.hash_state(h);
            h.write_u16(e.expected_purchases);
        }
        h.write_u32(self.savings_target_bp as u32);
        self.arrears.hash_state(h);
        h.write_u8(self.arrears_months);
        h.write_u32(self.loan.map_or(u32::MAX, |l| l.0));
        h.write_u8(u8::from(self.planned));
    }
}

/// Wszystko, co plan budżetu wie o gospodarstwie poza pieniędzmi.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HouseholdProfile {
    pub kind: HouseholdKind,
    pub size: u8,
    /// Cecha `Thrift` — steruje celem oszczędności.
    pub thrift: Q,
    /// Cecha `Ambition` — przesuwa wagę kopert na konsumpcję statusową (PRD §5.4).
    pub ambition: Q,
}

/// Wynik planowania miesiąca.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct BudgetPlan {
    /// Kwota rozdzielona na koperty.
    pub disposable: Money,
    /// Ile zabrakło na koszty stałe i oszczędności; `0`, kiedy budżet się spina.
    pub shortfall: Money,
    pub savings: Money,
}

/// Ile zakupów w kategorii gospodarstwo robi w miesiącu.
///
/// `towary w kategorii × 30 / purchase_days`, bo gospodarstwo kupuje **każdy towar
/// kategorii osobno** — jedno wyjście po zakupy to jedna pozycja, nie koszyk.
/// Tabeli na to nie ma: asortyment kategorii jest w `retail.ron`, rytm wyjść
/// w `choice.ron`, a trzecia kopia rozjechałaby się z pierwszą zmianą.
#[must_use]
pub fn expected_purchases(cat: StockCat, d: &EconomyData) -> u16 {
    let towarow = d.retail.goods.iter().filter(|g| g.cat == cat).count();
    if towarow == 0 {
        return 0;
    }
    let wyjsc = 30 / u32::from(d.purchase_days.max(1)).max(1);
    u16::try_from(towarow as u32 * wyjsc.max(1)).unwrap_or(u16::MAX)
}

/// Plan miesięczny: koszty stałe, oszczędności, reszta na koperty.
///
/// `disposable = dochód − Σ koszty stałe − dochód · savings_target_bp / 10 000`.
/// Podział na koperty idzie przez [`split_proportional`], więc **suma kopert równa
/// się `disposable` co do grosza**, a reszta z dzielenia trafia do pierwszej koperty
/// wg kolejności `StockCat` (00 §2).
///
/// `installment` to rata kredytów tego miesiąca — wchodzi do kosztów stałych jako
/// `FixedCost::LoanService` i tym samym do mianownika DSTI przy kolejnym wniosku.
pub fn plan_budget(
    b: &mut HouseholdBudget,
    p: &HouseholdProfile,
    income: Money,
    installment: Money,
    d: &EconomyData,
    _t: magnat_core::Tick,
) -> BudgetPlan {
    let f = &d.budget.fixed;
    b.income_monthly = income;
    b.fixed[FixedCost::Housing.as_index()] = Money(
        income
            .mul_ratio(i64::from(f.housing_bp_of_income), BP)
            .get()
            .max(f.housing_min_gr),
    );
    b.fixed[FixedCost::Utilities.as_index()] =
        Money(f.utilities_gr_per_person * i64::from(p.size.max(1)));
    b.fixed[FixedCost::Insurance.as_index()] = Money(f.insurance_gr);
    b.fixed[FixedCost::LoanService.as_index()] = installment;

    b.savings_target_bp = d.budget.savings_base_bp
        + d.budget.savings_thrift_gain_bp * i32::from(p.thrift.get()) / 100;
    let savings = income.mul_ratio(i64::from(b.savings_target_bp), BP);

    let reszta = income.get() - b.fixed_total().get();
    let (disposable, shortfall, savings) = if reszta < 0 {
        // Nie stać nawet na koszty stałe: oszczędności odpadają pierwsze, koperty
        // zostają puste, a brakującą kwotę obsługuje ścieżka kredyt → zaległość.
        (Money::ZERO, Money(-reszta), Money::ZERO)
    } else if reszta < savings.get() {
        // Stać na koszty stałe, nie stać na pełną oszczędność — odkładamy tyle, ile jest.
        (Money::ZERO, Money::ZERO, Money(reszta))
    } else {
        (Money(reszta - savings.get()), Money::ZERO, savings)
    };

    let wagi = envelope_weights(p, d);
    let kwoty = split_proportional(disposable, &wagi);
    for (i, kwota) in kwoty.iter().enumerate() {
        let cat = StockCat::ALL[i];
        b.envelopes[i] = Envelope {
            allocated: *kwota,
            spent: Money::ZERO,
            expected_purchases: expected_purchases(cat, d),
        };
    }
    b.planned = true;

    BudgetPlan {
        disposable,
        shortfall,
        savings,
    }
}

/// Wagi kopert po modulacji osobowością.
///
/// Cecha `Ambition` przesuwa do `status_shift_permille` promili z kategorii
/// codziennych na statusowe (`Clothing`, `Other`) — to jest konsumpcja statusowa
/// z PRD §5.4 i jedyna modulacja wag w M5. Suma zostaje 1000, bo zabrane promile
/// idą w całości tam, gdzie mają iść.
fn envelope_weights(p: &HouseholdProfile, d: &EconomyData) -> [u64; STOCK_CAT_COUNT] {
    let base = d.budget.weights(p.kind);
    let mut w = [0u64; STOCK_CAT_COUNT];
    for (i, v) in base.iter().enumerate() {
        w[i] = u64::from(*v);
    }
    let shift = i64::from(d.budget.status_shift_permille) * i64::from(p.ambition.get()) / 100;
    if shift <= 0 {
        return w;
    }
    let status = [StockCat::Clothing.as_index(), StockCat::Other.as_index()];
    let zrodla: u64 = w
        .iter()
        .enumerate()
        .filter(|(i, _)| !status.contains(i))
        .map(|(_, v)| *v)
        .sum();
    if zrodla == 0 {
        return w;
    }
    let mut zabrane = 0u64;
    for (i, v) in w.iter_mut().enumerate() {
        if status.contains(&i) {
            continue;
        }
        let ile = (*v * shift as u64) / zrodla;
        *v -= ile;
        zabrane += ile;
    }
    // Zabrane promile dzielimy po równo między dwie kategorie statusowe; reszta
    // do pierwszej wg kolejności `StockCat`, tak samo jak przy podziale kwoty.
    w[status[0]] += zabrane - zabrane / 2;
    w[status[1]] += zabrane / 2;
    w
}

/// Mianownik członu ceny dla potrzeby, liczony **z koperty** (§5.4).
///
/// To jest wywołanie, które M5d podmienia w decyzji zakupowej za stałą
/// `choice::budget_ref_for`. Gospodarstwo bez zaplanowanego budżetu (pierwsze
/// doby świata, zanim wypadnie granica miesiąca) i kategoria bez asortymentu
/// wracają do wartości z `choice.ron` — inaczej mianownik byłby zerem, a człon
/// ceny nieskończonością.
#[must_use]
pub fn budget_ref_for_need(b: &HouseholdBudget, cat: StockCat, d: &EconomyData) -> Money {
    if !b.planned {
        return crate::choice::budget_ref_for(cat, d);
    }
    let v = b.envelope(cat).per_purchase();
    if v.get() <= 0 {
        return crate::choice::budget_ref_for(cat, d);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::Tick;

    fn dane() -> EconomyData {
        EconomyData::load_default().expect("data/economy/")
    }

    fn profil(kind: HouseholdKind, size: u8) -> HouseholdProfile {
        HouseholdProfile {
            kind,
            size,
            thrift: Q::new(50),
            ambition: Q::new(0),
        }
    }

    #[test]
    fn koperty_sumuja_sie_do_kwoty_do_podzialu_co_do_grosza() {
        // P7 z §7.1: podział kwoty między N stron sumuje się do oryginału.
        let d = dane();
        let mut r = magnat_core::Rng::from_state([0x1234_5678_9ABC_DEF0, 11, 13, 17]);
        for _ in 0..5_000 {
            let dochod = Money(i64::from(r.gen_range_u32(2_000_000)));
            let mut b = HouseholdBudget::default();
            let p = profil(HouseholdKind::FamilyWithKids, 3);
            let plan = plan_budget(&mut b, &p, dochod, Money::ZERO, &d, Tick(0));
            assert_eq!(b.allocated_total(), plan.disposable, "dochód {dochod:?}");
        }
    }

    #[test]
    fn dochod_ponizej_kosztow_stalych_daje_niedobor_a_nie_ujemne_koperty() {
        let d = dane();
        let mut b = HouseholdBudget::default();
        let p = profil(HouseholdKind::LoneSenior, 1);
        let plan = plan_budget(&mut b, &p, Money(20_000), Money::ZERO, &d, Tick(0));
        assert!(plan.shortfall.get() > 0);
        assert_eq!(plan.disposable, Money::ZERO);
        assert!(b.envelopes.iter().all(|e| e.allocated.get() == 0));
    }

    #[test]
    fn mianownik_ceny_zostaje_w_rzedzie_wielkosci_stalej_z_choice_ron() {
        // Korekta `V-9`: rząd wielkości tej liczby decyduje o tym, czy cena waży.
        // Koperta miesięczna zamiast kwoty „na jedno wyjście" spłaszczyłaby człon
        // ceny do zera — test pilnuje, że dzielnik jest liczbą zakupów, nie wyjść.
        let d = dane();
        let mut b = HouseholdBudget::default();
        let p = profil(HouseholdKind::FamilyWithKids, 3);
        plan_budget(&mut b, &p, Money(450_000), Money::ZERO, &d, Tick(0));
        for cat in [StockCat::Food, StockCat::Drink, StockCat::Hygiene] {
            let z_koperty = budget_ref_for_need(&b, cat, &d).get();
            let stala = crate::choice::budget_ref_for(cat, &d).get();
            assert!(
                z_koperty * 4 >= stala && z_koperty <= stala * 4,
                "{cat:?}: koperta {z_koperty} gr wobec stałej {stala} gr — \
                 poza rzędem wielkości, kalibracja softmaxu przestanie pasować"
            );
        }
    }

    #[test]
    fn kategoria_bez_asortymentu_wraca_do_stalej_z_danych() {
        let d = dane();
        let mut b = HouseholdBudget::default();
        plan_budget(
            &mut b,
            &profil(HouseholdKind::Single, 1),
            Money(450_000),
            Money::ZERO,
            &d,
            Tick(0),
        );
        // Paliwo i „inne" nie mają w M5 ani jednego towaru detalicznego.
        assert_eq!(expected_purchases(StockCat::Fuel, &d), 0);
        assert_eq!(
            budget_ref_for_need(&b, StockCat::Fuel, &d),
            crate::choice::budget_ref_for(StockCat::Fuel, &d)
        );
    }

    #[test]
    fn ambicja_przesuwa_wage_na_konsumpcje_statusowa_bez_zmiany_sumy() {
        let d = dane();
        let spokojny = profil(HouseholdKind::Couple, 2);
        let mut ambitny = spokojny;
        ambitny.ambition = Q::new(100);
        let a = envelope_weights(&spokojny, &d);
        let b = envelope_weights(&ambitny, &d);
        assert_eq!(a.iter().sum::<u64>(), b.iter().sum::<u64>());
        assert!(b[StockCat::Clothing.as_index()] > a[StockCat::Clothing.as_index()]);
        assert!(b[StockCat::Food.as_index()] < a[StockCat::Food.as_index()]);
    }

    #[test]
    fn wyczerpana_koperta_podnosi_prog_a_pelna_go_nie_rusza() {
        let d = dane();
        let mut b = HouseholdBudget::default();
        plan_budget(
            &mut b,
            &profil(HouseholdKind::Single, 1),
            Money(450_000),
            Money::ZERO,
            &d,
            Tick(0),
        );
        let e = b.envelope(StockCat::Food);
        assert!(e.overspend_bp() < 0);
        b.charge(StockCat::Food, Money(e.allocated.get() * 2));
        assert!(b.envelope(StockCat::Food).overspend_bp() > 0);
        let prog_pusty = crate::choice::purchase_threshold(
            magnat_core::NeedKind::Hunger,
            Q::new(50),
            b.envelope(StockCat::Food).overspend_bp(),
            &d,
        );
        let prog_pelny =
            crate::choice::purchase_threshold(magnat_core::NeedKind::Hunger, Q::new(50), 0, &d);
        assert!(prog_pusty > prog_pelny);
    }
}
