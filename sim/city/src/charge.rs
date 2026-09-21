//! Należność podatkowa i jej cykl życia (M8a §5.1, WP2).
//!
//! Cykl ma cztery stany i **żaden z nich nie znika po cichu**:
//! `Assessed → Settled`, `Assessed → Overdue → Settled`, albo `… → Abated`.
//! Domknięcie `Σ Assessed = Σ Settled + Σ Overdue + Σ Abated` (test T1, tolerancja
//! zero groszy) jest tu niezmiennikiem struktury, a nie wnioskiem z testu:
//! kwotę zmienia wyłącznie [`ChargeRegistry::accrue`], a stan wyłącznie trzy metody
//! przejścia, które przy okazji przepisują sumę z jednego kubełka do drugiego.
//!
//! **Ziarnistość: jedna należność na (płatnik, danina, okres).** Nie na transakcję —
//! ryzyko `R8` mówi wprost, że siedem danin naliczanych per transakcja daje graczowi
//! ścianę liczb, a rejestr rosnący jak dziennik transakcji przestałby się mieścić
//! w pamięci po pierwszym roku. Piekarnia widzi jeden wiersz „VAT, marzec", a nie
//! cztery tysiące wierszy po jednej bułce.

use std::collections::{BTreeMap, BTreeSet};

use magnat_core::{
    AbateReason, CityReason, HashState, HouseholdId, Mass, Money, SiteId, StateHasher, TaxKind,
    Tick, TAX_KIND_COUNT,
};

/// Uchwyt należności. Indeks w rejestrze; wartości nie są nigdy ponownie użyte.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct TaxChargeId(pub u32);

/// Okres rozliczeniowy. **Wyłącznie siatka miesięczna** (`K-15`, M8a §5.0): dnia,
/// miesiąca i roku, bo 30 i 360 dzielą się bez reszty. Tygodnia tu nie ma i nie
/// będzie — podatek liczony „co tydzień" nie domknąłby się do roku, bo tydzień
/// dryfuje względem miesiąca.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum FiscalPeriod {
    /// Doba absolutna — cło i akcyza rozliczane na bieżąco.
    Day(u32),
    /// Rok i miesiąc 1..=12 — VAT, PIT, nieruchomości.
    Month(u16, u8),
    /// Rok obrotowy — CIT i koncesje.
    Year(u16),
}

impl FiscalPeriod {
    /// Rok, do którego okres należy — klucz agregacji rocznej w budżecie.
    #[must_use]
    pub fn year(self) -> u16 {
        match self {
            FiscalPeriod::Day(d) => u16::try_from(d / 360).unwrap_or(u16::MAX),
            FiscalPeriod::Month(y, _) | FiscalPeriod::Year(y) => y,
        }
    }
}

impl HashState for FiscalPeriod {
    fn hash_state(&self, h: &mut StateHasher) {
        match self {
            FiscalPeriod::Day(d) => {
                h.write_u8(0);
                h.write_u32(*d);
            }
            FiscalPeriod::Month(y, m) => {
                h.write_u8(1);
                h.write_u16(*y);
                h.write_u8(*m);
            }
            FiscalPeriod::Year(y) => {
                h.write_u8(2);
                h.write_u16(*y);
            }
        }
    }
}

/// Kto płaci.
///
/// **Zakład, a nie firma** — bo księga, konto i rachunek wyniku są w tej symulacji
/// per zakład (`Ledger::new(site, firm, …)`), a danina bez konta, z którego da się
/// ją ściągnąć, byłaby liczbą w raporcie, a nie pieniądzem w budżecie. Firma widzi
/// swoje obciążenia jako sumę po swoich zakładach; rejestr firm zna tę listę.
///
/// `External` to abstrakcyjny pracodawca spoza miasta — płatnik zaliczek PIT
/// dopóki lista płac nie rusza pieniądza z konta zakładu (patrz `M8a` §5.1,
/// domknięcie `BF-7`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum TaxPayer {
    Site(SiteId),
    Household(HouseholdId),
    External,
}

impl HashState for TaxPayer {
    fn hash_state(&self, h: &mut StateHasher) {
        match self {
            TaxPayer::Site(s) => {
                h.write_u8(0);
                h.write_u64(s.0.to_bits());
            }
            TaxPayer::Household(hh) => {
                h.write_u8(1);
                h.write_u64(hh.0.to_bits());
            }
            TaxPayer::External => h.write_u8(2),
        }
    }
}

/// Stan należności. Przejścia robią wyłącznie metody [`ChargeRegistry`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChargeState {
    /// Naliczona, termin jeszcze nie minął.
    Assessed,
    /// Zapłacona — pieniądz jest w budżecie.
    Settled { at: Tick },
    /// Termin minął, kwota stoi. Odsetki rosną co dobę i są **osobną liczbą**,
    /// żeby kwota główna dalej znaczyła to samo w domknięciu T1.
    Overdue { since: Tick, interest: Money },
    /// Umorzona: przestała być wymagalna, choć nikt jej nie zapłacił.
    Abated { at: Tick, why: AbateReason },
}

impl ChargeState {
    #[must_use]
    pub fn is_open(self) -> bool {
        matches!(self, ChargeState::Assessed | ChargeState::Overdue { .. })
    }

    fn tag(self) -> u8 {
        match self {
            ChargeState::Assessed => 0,
            ChargeState::Settled { .. } => 1,
            ChargeState::Overdue { .. } => 2,
            ChargeState::Abated { .. } => 3,
        }
    }
}

impl HashState for ChargeState {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(self.tag());
        match self {
            ChargeState::Assessed => {}
            ChargeState::Settled { at } | ChargeState::Abated { at, .. } => h.write_u64(at.0),
            ChargeState::Overdue { since, interest } => {
                h.write_u64(since.0);
                h.write_i64(interest.get());
            }
        }
        if let ChargeState::Abated { why, .. } = self {
            h.write_u8(why.as_index() as u8);
        }
    }
}

/// Jedna należność. Niesie **stawkę użytą w chwili naliczenia**, nie aktualną:
/// stawka zmienia się uchwałą i nigdy wstecz, a karta inspekcji otwarta pół roku
/// później ma tłumaczyć kwotę, która wtedy powstała.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TaxCharge {
    pub id: TaxChargeId,
    pub payer: TaxPayer,
    pub kind: TaxKind,
    pub period: FiscalPeriod,
    /// Podstawa pieniężna. Dla akcyzy jest to wartość ekwiwalentna, a ilość niesie
    /// [`TaxCharge::base_mass`].
    pub base: Money,
    pub base_mass: Mass,
    pub rate_snapshot: u32,
    pub amount: Money,
    pub assessed_at: Tick,
    pub due_at: Tick,
    pub state: ChargeState,
}

impl TaxCharge {
    /// Powód w postaci strukturalnej (00 §7). Wyprowadzony z należności, a nie
    /// trzymany obok niej: drugie pole z tą samą treścią rozjechałoby się przy
    /// pierwszej korekcie kwoty.
    #[must_use]
    pub fn reason(&self) -> magnat_core::DecisionReason {
        magnat_core::DecisionReason::City(CityReason::TaxAssessed {
            kind: self.kind,
            rate_bp: u16::try_from(self.rate_snapshot).unwrap_or(u16::MAX),
            amount: self.amount,
        })
    }
}

impl HashState for TaxCharge {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.id.0);
        self.payer.hash_state(h);
        h.write_u8(self.kind.as_index() as u8);
        self.period.hash_state(h);
        h.write_i64(self.base.get());
        h.write_i64(self.base_mass.0);
        h.write_u32(self.rate_snapshot);
        h.write_i64(self.amount.get());
        h.write_u64(self.assessed_at.0);
        h.write_u64(self.due_at.0);
        self.state.hash_state(h);
    }
}

/// Sumy kontrolne jednej pary (danina, okres) — lewa i prawa strona domknięcia T1.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ChargeTotals {
    pub assessed: Money,
    pub settled: Money,
    pub overdue: Money,
    pub abated: Money,
}

impl ChargeTotals {
    /// Kwota wciąż otwarta: naliczona, ale ani niezapłacona, ani nieumorzona.
    #[must_use]
    pub fn open(&self) -> Money {
        Money(self.assessed.get() - self.settled.get() - self.abated.get())
    }
}

/// Rejestr należności. Jedyne miejsce, w którym danina powstaje i zmienia stan.
///
/// `ponytail:` sufit — rejestr trzyma **wszystkie** należności od początku świata,
/// także zapłacone. Przy ziarnistości „płatnik × danina × okres" daje to rząd
/// 50 tys. wierszy na rok gry przy 300 firmach, czyli kilka megabajtów; przy
/// stuletniej rozgrywce M10 przestanie być darmowe. Droga wyjścia: zwijać
/// należności starsze niż `time_bar_days` do `totals` i zostawiać w `charges`
/// wyłącznie okno widoczne w karcie inspekcji — `totals` już dziś niosą wszystko,
/// czego potrzebuje domknięcie T1.
#[derive(Clone, Default)]
pub struct ChargeRegistry {
    charges: Vec<TaxCharge>,
    /// Otwarta należność dla klucza (płatnik, danina, okres) — tu dopisuje `accrue`.
    open_by_key: BTreeMap<(TaxPayer, u8, FiscalPeriod), TaxChargeId>,
    /// Należności czekające na zapłatę, rosnąco. Kolejność rozliczania jest
    /// kolejnością identyfikatorów, czyli powstania — deterministyczna bez losowania.
    unsettled: BTreeSet<TaxChargeId>,
    totals: BTreeMap<(u8, FiscalPeriod), ChargeTotals>,
    /// Wpływy zrealizowane narastająco, per danina — druga strona równania
    /// `Σ Settled == Δ CityBudget.revenue_life[kind]`.
    settled_by_kind: [Money; TAX_KIND_COUNT],
}

impl ChargeRegistry {
    #[must_use]
    pub fn new() -> ChargeRegistry {
        ChargeRegistry::default()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.charges.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.charges.is_empty()
    }

    #[must_use]
    pub fn get(&self, id: TaxChargeId) -> Option<&TaxCharge> {
        self.charges.get(id.0 as usize)
    }

    pub fn iter(&self) -> impl Iterator<Item = &TaxCharge> {
        self.charges.iter()
    }

    /// Należności jednego płatnika — treść karty inspekcji firmy (kryterium WP2).
    pub fn of_payer(&self, payer: TaxPayer) -> impl Iterator<Item = &TaxCharge> {
        self.charges.iter().filter(move |c| c.payer == payer)
    }

    #[must_use]
    pub fn totals(&self, kind: TaxKind, period: FiscalPeriod) -> ChargeTotals {
        self.totals
            .get(&(kind.as_index() as u8, period))
            .copied()
            .unwrap_or_default()
    }

    #[must_use]
    pub fn settled_of(&self, kind: TaxKind) -> Money {
        self.settled_by_kind[kind.as_index()]
    }

    /// Wszystkie pary (danina, okres), dla których cokolwiek naliczono.
    pub fn periods(&self) -> impl Iterator<Item = (TaxKind, FiscalPeriod, ChargeTotals)> + '_ {
        self.totals.iter().filter_map(|((k, p), t)| {
            TaxKind::from_index(usize::from(*k)).map(|kind| (kind, *p, *t))
        })
    }

    /// Dolicza kwotę do otwartej należności tego płatnika za ten okres albo
    /// zakłada nową. **Jedyne wejście, którym kwota rośnie.**
    ///
    /// `rate_bp` idzie do migawki; przy drugim doliczeniu w tym samym okresie
    /// zostaje pierwsza stawka, bo stawka zmienia się od początku okresu, a nie
    /// w jego środku (`effective_from`).
    #[allow(clippy::too_many_arguments)]
    pub fn accrue(
        &mut self,
        payer: TaxPayer,
        kind: TaxKind,
        period: FiscalPeriod,
        base: Money,
        base_mass: Mass,
        rate_bp: u32,
        amount: Money,
        at: Tick,
        due_at: Tick,
    ) -> Option<TaxChargeId> {
        if amount.get() == 0 {
            return None;
        }
        let key = (payer, kind.as_index() as u8, period);
        let t = self.totals.entry((key.1, period)).or_default();
        t.assessed = Money(t.assessed.get() + amount.get());
        if let Some(id) = self.open_by_key.get(&key).copied() {
            let c = &mut self.charges[id.0 as usize];
            c.base = Money(c.base.get() + base.get());
            c.base_mass = Mass(c.base_mass.0 + base_mass.0);
            c.amount = Money(c.amount.get() + amount.get());
            return Some(id);
        }
        let id = TaxChargeId(u32::try_from(self.charges.len()).expect("za dużo należności"));
        self.charges.push(TaxCharge {
            id,
            payer,
            kind,
            period,
            base,
            base_mass,
            rate_snapshot: rate_bp,
            amount,
            assessed_at: at,
            due_at,
            state: ChargeState::Assessed,
        });
        self.open_by_key.insert(key, id);
        self.unsettled.insert(id);
        Some(id)
    }

    /// Należności zaległe: uchwyt, doba wejścia w zaległość i kwota główna.
    ///
    /// Po `unsettled`, a **nie** po wszystkich należnościach — zaległa jest z definicji
    /// nierozliczona, a rejestr trzyma wszystko od początku świata. Przelot po całości
    /// kosztowałby co dobę tyle, ile świat dotąd naliczył, czyli rósłby bez granicy
    /// przy pracy, która maleje.
    #[must_use]
    pub fn overdue_now(&self) -> Vec<(TaxChargeId, Tick, Money)> {
        self.unsettled
            .iter()
            .filter_map(|id| {
                let c = &self.charges[id.0 as usize];
                match c.state {
                    ChargeState::Overdue { since, .. } => Some((c.id, since, c.amount)),
                    _ => None,
                }
            })
            .collect()
    }

    /// Należności wymagalne w tym ticku, rosnąco po identyfikatorze.
    #[must_use]
    pub fn due_now(&self, t: Tick) -> Vec<TaxChargeId> {
        self.unsettled
            .iter()
            .copied()
            .filter(|id| self.charges[id.0 as usize].due_at.0 <= t.0)
            .collect()
    }

    /// Wszystkie otwarte należności płatnika — używa tego zgłoszenie roszczenia
    /// do postępowania upadłościowego (`K-10`).
    #[must_use]
    pub fn open_of_payer(&self, payer: TaxPayer) -> Vec<TaxChargeId> {
        self.unsettled
            .iter()
            .copied()
            .filter(|id| self.charges[id.0 as usize].payer == payer)
            .collect()
    }

    pub fn mark_settled(&mut self, id: TaxChargeId, at: Tick) {
        let Some(c) = self.charges.get_mut(id.0 as usize) else {
            return;
        };
        if !c.state.is_open() {
            return;
        }
        let (kind, period, amount) = (c.kind, c.period, c.amount);
        let byla_zalegla = matches!(c.state, ChargeState::Overdue { .. });
        c.state = ChargeState::Settled { at };
        let key = (c.payer, kind.as_index() as u8, period);
        self.open_by_key.remove(&key);
        self.unsettled.remove(&id);
        let t = self
            .totals
            .entry((kind.as_index() as u8, period))
            .or_default();
        t.settled = Money(t.settled.get() + amount.get());
        if byla_zalegla {
            t.overdue = Money(t.overdue.get() - amount.get());
        }
        let s = &mut self.settled_by_kind[kind.as_index()];
        *s = Money(s.get() + amount.get());
    }

    /// Przestawia należność w zaległość albo dopisuje jej odsetki. Zwraca `true`,
    /// jeśli to było **wejście** w zaległość (a nie kolejna doba w niej).
    pub fn mark_overdue(&mut self, id: TaxChargeId, at: Tick, interest_delta: Money) -> bool {
        let Some(c) = self.charges.get_mut(id.0 as usize) else {
            return false;
        };
        match c.state {
            ChargeState::Assessed => {
                let (kind, period, amount) = (c.kind, c.period, c.amount);
                c.state = ChargeState::Overdue {
                    since: at,
                    interest: interest_delta,
                };
                let t = self
                    .totals
                    .entry((kind.as_index() as u8, period))
                    .or_default();
                t.overdue = Money(t.overdue.get() + amount.get());
                true
            }
            ChargeState::Overdue { since, interest } => {
                c.state = ChargeState::Overdue {
                    since,
                    interest: Money(interest.get() + interest_delta.get()),
                };
                false
            }
            _ => false,
        }
    }

    pub fn abate(&mut self, id: TaxChargeId, at: Tick, why: AbateReason) {
        let Some(c) = self.charges.get_mut(id.0 as usize) else {
            return;
        };
        if !c.state.is_open() {
            return;
        }
        let (kind, period, amount) = (c.kind, c.period, c.amount);
        let byla_zalegla = matches!(c.state, ChargeState::Overdue { .. });
        c.state = ChargeState::Abated { at, why };
        let key = (c.payer, kind.as_index() as u8, period);
        self.open_by_key.remove(&key);
        self.unsettled.remove(&id);
        let t = self
            .totals
            .entry((kind.as_index() as u8, period))
            .or_default();
        t.abated = Money(t.abated.get() + amount.get());
        if byla_zalegla {
            t.overdue = Money(t.overdue.get() - amount.get());
        }
    }

    /// Domknięcie podatkowe (T1): dla każdej pary (danina, okres) kwota naliczona
    /// równa się sumie zapłaconej, zaległej i umorzonej — **z tolerancją zero**.
    ///
    /// `Err` niesie parę, na której pękło, i obie strony równania: przy czerwonym
    /// teście chce się znać liczby, a nie fakt ich nierówności.
    pub fn check_closure(&self) -> Result<(), (TaxKind, FiscalPeriod, Money, Money)> {
        for ((k, p), t) in &self.totals {
            let Some(kind) = TaxKind::from_index(usize::from(*k)) else {
                continue;
            };
            // Kwota jeszcze naliczona i nierozstrzygnięta: `Assessed` bez terminu
            // i `Overdue`. Pierwsza nie ma własnego licznika, bo jest resztą.
            let prawa = Money(t.settled.get() + t.abated.get() + t.open().get());
            if prawa != t.assessed {
                return Err((kind, *p, t.assessed, prawa));
            }
            if t.overdue.get() > t.open().get() {
                return Err((kind, *p, t.open(), t.overdue));
            }
        }
        Ok(())
    }
}

impl HashState for ChargeRegistry {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.charges.len() as u64);
        for c in &self.charges {
            c.hash_state(h);
        }
        for m in &self.settled_by_kind {
            h.write_i64(m.get());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(i: u32) -> TaxPayer {
        TaxPayer::Site(SiteId(magnat_core::Entity::new(
            i,
            std::num::NonZeroU32::MIN,
        )))
    }

    fn dolicz(r: &mut ChargeRegistry, p: TaxPayer, kwota: i64) -> Option<TaxChargeId> {
        r.accrue(
            p,
            TaxKind::Vat,
            FiscalPeriod::Month(1, 3),
            Money(kwota * 5),
            Mass(0),
            2300,
            Money(kwota),
            Tick(10),
            Tick(100),
        )
    }

    #[test]
    fn druga_kwota_w_tym_samym_okresie_dopisuje_sie_do_tej_samej_naleznosci() {
        let mut r = ChargeRegistry::new();
        let a = dolicz(&mut r, site(1), 1000).unwrap();
        let b = dolicz(&mut r, site(1), 500).unwrap();
        assert_eq!(a, b);
        assert_eq!(r.len(), 1);
        assert_eq!(r.get(a).unwrap().amount, Money(1500));
        // Inny płatnik to inna należność, mimo tego samego okresu.
        let c = dolicz(&mut r, site(2), 700).unwrap();
        assert_ne!(a, c);
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn kwota_zero_nie_zaklada_naleznosci() {
        let mut r = ChargeRegistry::new();
        assert!(dolicz(&mut r, site(1), 0).is_none());
        assert!(r.is_empty());
    }

    #[test]
    fn cykl_zycia_domyka_sie_w_kazdej_sciezce() {
        let mut r = ChargeRegistry::new();
        let zaplacona = dolicz(&mut r, site(1), 1000).unwrap();
        let zalegla = dolicz(&mut r, site(2), 400).unwrap();
        let umorzona = dolicz(&mut r, site(3), 250).unwrap();
        r.mark_settled(zaplacona, Tick(120));
        r.mark_overdue(zalegla, Tick(120), Money(3));
        r.abate(umorzona, Tick(130), AbateReason::Bankruptcy);
        let t = r.totals(TaxKind::Vat, FiscalPeriod::Month(1, 3));
        assert_eq!(t.assessed, Money(1650));
        assert_eq!(t.settled, Money(1000));
        assert_eq!(t.overdue, Money(400));
        assert_eq!(t.abated, Money(250));
        assert_eq!(t.open(), Money(400));
        r.check_closure().expect("domknięcie po trzech ścieżkach");
        // Zaległa zapłacona później wychodzi z kubełka zaległości i nie liczy się dwa razy.
        r.mark_settled(zalegla, Tick(200));
        let t = r.totals(TaxKind::Vat, FiscalPeriod::Month(1, 3));
        assert_eq!(t.settled, Money(1400));
        assert_eq!(t.overdue, Money::ZERO);
        assert_eq!(t.open(), Money::ZERO);
        r.check_closure()
            .expect("domknięcie po zapłacie zaległości");
        assert_eq!(r.settled_of(TaxKind::Vat), Money(1400));
    }

    #[test]
    fn nalezosc_rozliczona_nie_zmienia_juz_stanu() {
        let mut r = ChargeRegistry::new();
        let id = dolicz(&mut r, site(1), 1000).unwrap();
        r.mark_settled(id, Tick(120));
        r.mark_settled(id, Tick(130));
        r.abate(id, Tick(140), AbateReason::Council);
        let t = r.totals(TaxKind::Vat, FiscalPeriod::Month(1, 3));
        assert_eq!(t.settled, Money(1000));
        assert_eq!(t.abated, Money::ZERO);
        r.check_closure()
            .expect("podwójne rozliczenie nie dubluje kwoty");
    }

    #[test]
    fn wymagalne_ida_w_kolejnosci_powstania() {
        let mut r = ChargeRegistry::new();
        let a = dolicz(&mut r, site(1), 100).unwrap();
        let b = dolicz(&mut r, site(2), 100).unwrap();
        assert_eq!(r.due_now(Tick(99)), Vec::new());
        assert_eq!(r.due_now(Tick(100)), vec![a, b]);
        r.mark_settled(a, Tick(100));
        assert_eq!(r.due_now(Tick(100)), vec![b]);
    }
}
