//! Menedżerowie i delegowanie (M7c WP7, M7 §5.4, PRD §7.5).
//!
//! # Po co to jest, jednym zdaniem
//!
//! **Delegowanie jest jedynym sposobem skalowania gracza.** Gracz z dwustoma sklepami
//! nie klika dwustu razy: ustawia politykę ([`magnat_policy::FirmPolicy`]) i przypisuje
//! menedżera, a jakość wykonania zależy od menedżera. Ten sam kod realizuje decyzje
//! firm AI — różni się wyłącznie **źródło** polityki (edytor M9 albo tier taktyczny M7e).
//!
//! # Trzy kanały wpływu i dlaczego akurat trzy
//!
//! §7.5 PRD obiecuje „realny wpływ", a ryzyko `R6` fazy nazywa wprost, co się stanie,
//! jeśli tej obietnicy nie dotrzymać: mnożnik 0,98–1,02 to warstwa HR jako dekoracja.
//! Dlatego jakość zarządzania wchodzi w **trzy niezależne miejsca**, a każde ma zakres,
//! który gracz zobaczy w balansatorze:
//!
//! | Kanał | Skrajne wartości | Gdzie |
//! |---|---|---|
//! | Produktywność | 0,85–1,15 | [`crate::hr::productivity::mgmt_mult`] → `effective_labor` → M6 |
//! | Straty | 1,15–0,85 | [`crate::hr::productivity::loss_multiplier`] → M6 |
//! | Rotacja | 1,40–0,70 | [`crate::hr::turnover::turnover_mult`] → `quit_pressure` |
//!
//! Dwa pierwsze istniały od M7a i do M7c nie miały pisarza innego niż wartość neutralna;
//! trzeci powstaje tutaj, bo bez niego tabela z §5.4 obiecywała kanał, którego nie było.
//!
//! # Jakość zarządzania jest rzadkim zasobem
//!
//! Nie dlatego, że tak napisano w dokumencie, tylko dlatego, że **rozpiętość kierowania
//! degraduje**: ten sam człowiek na sześciu zakładach jest wyraźnie gorszy niż na dwóch.
//! Stąd przeciąganie menedżerów ma sens ekonomiczny i stąd odejście menedżera boli —
//! zakład spada do jakości zastępstwa, a autonomia wraca do samych cen.

use magnat_core::hash::{HashState, StateHasher};
use magnat_core::{CitizenId, SimMinute, SiteId, Q};
use magnat_policy::FirmPolicy;
use smallvec::SmallVec;

use crate::hr::productivity::ManagementQuality;
use crate::hr::tuning::ManagerTuning;
use crate::labor_policy::HiringPolicy;

/// Styl kierowania. **Nie jest ozdobą karty inspekcji** — z niego wychodzą trzy liczby,
/// które M7b podejmował stałą: agresja licytacyjna, wagi wyboru kandydata i kolejność
/// świadczeń pozapłacowych.
///
/// Kolejność wariantów jest kontraktem zapisu gry: styl siedzi w rekordzie menedżera.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[repr(u8)]
pub enum ManagerStyle {
    /// Wyciska wynik. Patrzy na umiejętność, nie na cenę; świadczeń nie rozdaje.
    Taskmaster = 0,
    /// Buduje ludzi. Szkolenia i opieka przed autem służbowym, licytuje spokojnie.
    Coach = 1,
    /// Trzyma procedurę. Nigdzie nie wystaje i nigdzie nie przoduje.
    #[default]
    Bureaucrat = 2,
    /// Targuje się. Licytuje agresywnie i szuka taniego kandydata.
    Dealmaker = 3,
}

impl ManagerStyle {
    pub const ALL: [ManagerStyle; 4] = [
        ManagerStyle::Taskmaster,
        ManagerStyle::Coach,
        ManagerStyle::Bureaucrat,
        ManagerStyle::Dealmaker,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            ManagerStyle::Taskmaster => "Taskmaster",
            ManagerStyle::Coach => "Coach",
            ManagerStyle::Bureaucrat => "Bureaucrat",
            ManagerStyle::Dealmaker => "Dealmaker",
        }
    }

    /// Agresja licytacyjna, 0..=100 — mnożnik kroku podwyżki w wiszącej ofercie.
    ///
    /// Do M7c była stałą `AGGRESSION = 0` w `sim/economy::labor::bidding` i całe miasto
    /// licytowało tak samo (`AV-6`). Osobowość dyrektora dołoży do tego M7e; styl
    /// menedżera jest **pierwszym** pisarzem tej liczby i mieszka tu, bo to zakładem,
    /// a nie firmą, kieruje menedżer.
    #[must_use]
    pub const fn aggression(self) -> u8 {
        match self {
            ManagerStyle::Dealmaker => 60,
            ManagerStyle::Taskmaster => 30,
            ManagerStyle::Bureaucrat => 0,
            ManagerStyle::Coach => 10,
        }
    }

    /// Wagi wyboru kandydata — drugi z trzech znaczników z `AV-6`.
    #[must_use]
    pub const fn hiring(self) -> HiringPolicy {
        match self {
            ManagerStyle::Taskmaster => HiringPolicy {
                quality_focus: 40,
                price_focus: 0,
            },
            ManagerStyle::Dealmaker => HiringPolicy {
                quality_focus: 0,
                price_focus: 40,
            },
            ManagerStyle::Coach => HiringPolicy {
                quality_focus: 15,
                price_focus: 10,
            },
            ManagerStyle::Bureaucrat => HiringPolicy::NEUTRAL,
        }
    }

    /// Kolejność, w której menedżer dokłada świadczenia, gdy stawka stoi na suficie —
    /// trzeci znacznik z `AV-6`.
    ///
    /// Do M7c kolejność była sztywna („posiłki → opieka → szkolenia → auto") i była
    /// kolejnością **kosztu**. Teraz jest decyzją: trener zaczyna od szkoleń, nadzorca
    /// od tego, co najtańsze, a handlowiec od auta służbowego, bo ono robi wrażenie
    /// na kandydacie i o to mu chodzi.
    #[must_use]
    pub const fn benefit_order(self) -> [u8; 4] {
        use crate::hr::employment::BenefitSet as B;
        match self {
            ManagerStyle::Coach => [B::TRAINING, B::HEALTH, B::MEALS, B::COMPANY_CAR],
            ManagerStyle::Dealmaker => [B::COMPANY_CAR, B::MEALS, B::HEALTH, B::TRAINING],
            ManagerStyle::Taskmaster | ManagerStyle::Bureaucrat => {
                [B::MEALS, B::HEALTH, B::TRAINING, B::COMPANY_CAR]
            }
        }
    }
}

impl HashState for ManagerStyle {
    #[inline]
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(*self as u8);
    }
}

/// Ile menedżer może w zakładzie zmienić bez pytania właściciela.
///
/// Kolejność wariantów jest kontraktem zapisu gry i jest **rosnąca**: wyższa autonomia
/// obejmuje wszystko, co niższa.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
#[repr(u8)]
pub enum Autonomy {
    /// Wyłącznie ceny. To jest też autonomia zastępstwa po odejściu menedżera.
    #[default]
    PricesOnly = 0,
    /// Ceny i obsada.
    PricesAndStaff = 1,
    /// Wszystko, co polityka umie wykonać.
    Full = 2,
}

impl Autonomy {
    /// Czy ta autonomia obejmuje dziedzinę polityki.
    #[must_use]
    pub const fn covers(self, d: magnat_policy::PolicyDomain) -> bool {
        use magnat_policy::PolicyDomain as D;
        match d {
            D::Pricing => true,
            D::Hr => matches!(self, Autonomy::PricesAndStaff | Autonomy::Full),
            D::Stock | D::Production | D::Logistics => matches!(self, Autonomy::Full),
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Autonomy::PricesOnly => "PricesOnly",
            Autonomy::PricesAndStaff => "PricesAndStaff",
            Autonomy::Full => "Full",
        }
    }
}

impl HashState for Autonomy {
    #[inline]
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(*self as u8);
    }
}

/// Menedżer — mieszkaniec prowadzący zakłady firmy.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Manager {
    pub citizen: CitizenId,
    /// Umiejętność „zarządzanie" z `Skills` mieszkańca (M3 §5.1).
    pub skill_mgmt: Q,
    pub style: ManagerStyle,
    /// Rozpiętość kierowania. **Posortowana po `SiteId`** — po niej idzie przeliczanie
    /// jakości, a to wchodzi do hasha stanu.
    pub sites: SmallVec<[SiteId; 4]>,
    pub tenure: SimMinute,
}

impl Manager {
    #[must_use]
    pub fn new(
        citizen: CitizenId,
        skill_mgmt: Q,
        style: ManagerStyle,
        since: SimMinute,
    ) -> Manager {
        Manager {
            citizen,
            skill_mgmt,
            style,
            sites: SmallVec::new(),
            tenure: since,
        }
    }

    /// Dopisuje zakład, zachowując porządek. Zwraca `false`, gdy już go prowadzi.
    pub fn take_site(&mut self, site: SiteId) -> bool {
        match self.sites.binary_search(&site) {
            Ok(_) => false,
            Err(i) => {
                self.sites.insert(i, site);
                true
            }
        }
    }

    pub fn drop_site(&mut self, site: SiteId) -> bool {
        match self.sites.binary_search(&site) {
            Ok(i) => {
                self.sites.remove(i);
                true
            }
            Err(_) => false,
        }
    }

    #[must_use]
    pub fn span(&self) -> usize {
        self.sites.len()
    }
}

impl HashState for Manager {
    fn hash_state(&self, h: &mut StateHasher) {
        self.citizen.0.hash_state(h);
        h.write_u8(self.skill_mgmt.get());
        self.style.hash_state(h);
        h.write_u32(self.sites.len() as u32);
        for s in &self.sites {
            s.0.hash_state(h);
        }
        self.tenure.hash_state(h);
    }
}

/// Zakład oddany menedżerowi wraz z polityką, którą ma prowadzić.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SiteDelegation {
    /// `None` znaczy **zastępstwo**: menedżer odszedł, polityka zostaje, jakość spada
    /// do mediany firmy minus `replacement_drop`, a autonomia do samych cen (§5.4).
    pub manager: Option<CitizenId>,
    /// **Ten sam typ, którego używa AI i gracz** (`K-11`).
    pub policy: FirmPolicy,
    pub autonomy: Autonomy,
    /// Jak często menedżer melduje właścicielowi. Nie wpływa na wykonanie — wpływa
    /// na to, kiedy gracz się o czymś dowie (panel M9).
    pub report_freq: magnat_core::Cadence,
    /// Tick ostatniego wykonania polityki — na nim stoi martwa strefa `cooldown_h`.
    pub last_run: magnat_core::Tick,
}

impl SiteDelegation {
    #[must_use]
    pub fn new(manager: CitizenId, policy: FirmPolicy, autonomy: Autonomy) -> SiteDelegation {
        SiteDelegation {
            manager: Some(manager),
            policy,
            autonomy,
            report_freq: magnat_core::Cadence::EveryMonth,
            last_run: magnat_core::Tick(0),
        }
    }

    /// Czy martwa strefa polityki już minęła.
    #[must_use]
    pub fn ready(&self, now: magnat_core::Tick) -> bool {
        self.ready_after(now, 0)
    }

    /// Jak [`SiteDelegation::ready`], ale z dodatkowymi godzinami zwłoki menedżera
    /// (M9d WP9). Słaby menedżer reaguje rzadziej, więc jego martwa strefa jest dłuższa
    /// niż ta, którą ustawił właściciel polityki — i to jest cała treść tego argumentu.
    #[must_use]
    pub fn ready_after(&self, now: magnat_core::Tick, extra_h: u8) -> bool {
        let strefa = (u64::from(self.policy.cooldown_h) + u64::from(extra_h)) * 60;
        // Zakład, który jeszcze nigdy nie wykonał polityki, wykonuje ją od razu —
        // inaczej pierwszy dzień delegowania byłby dniem bez zarządzania.
        self.last_run.0 == 0 || now.0.saturating_sub(self.last_run.0) >= strefa
    }
}

impl HashState for SiteDelegation {
    fn hash_state(&self, h: &mut StateHasher) {
        match self.manager {
            None => h.write_u8(0),
            Some(c) => {
                h.write_u8(1);
                c.0.hash_state(h);
            }
        }
        // Polityka wchodzi do hasha **w całości** (`magnat_policy::hash`). Numer
        // i liczba reguł nie wystarczą: gracz wyłącza regułę, przestawia martwą strefę
        // i poprawia próg w warunku, a żadna z tych zmian nie rusza ani numeru, ani
        // długości listy — dwa rozjechane światy dawałyby wtedy identyczny hash.
        self.policy.hash_state(h);
        self.autonomy.hash_state(h);
        h.write_u8(self.report_freq as u8);
        h.write_u64(self.last_run.0);
    }
}

/// Jakość zarządzania zakładem (§5.4).
///
/// Trzy wejścia i każde ma swoje zdanie:
/// 1. **umiejętność menedżera** — punkt wyjścia;
/// 2. **rozpiętość kierowania** — ponad optimum degraduje liniowo, bo człowiek
///    na sześciu zakładach jest na każdym z nich rzadziej;
/// 3. **nastrój załogi** — menedżer w zbuntowanym zakładzie zarządza gorzej,
///    a nie tylko „ma trudniej".
///
/// **Bierze menedżera i nastrój, nie zakład** — korekta `AX-5` wobec sygnatury
/// z §5.4. Zakład nie wnosi tu ani jednej liczby: rozpiętość jest cechą menedżera,
/// a nastrój i tak trzeba podać osobno, bo `Site` go nie zna (morale mieszka
/// w `Vitals` mieszkańców). Przekazanie `&Site` sugerowałoby, że funkcja czyta
/// coś jeszcze — ten sam błąd, który `AS-3` usunął z `loss_multiplier`.
#[must_use]
pub fn management_quality(m: &Manager, morale: Q, t: &ManagerTuning) -> ManagementQuality {
    let nadmiar = m.span().saturating_sub(t.span_optimum as usize);
    let kara = (nadmiar as i32).saturating_mul(i32::from(t.span_penalty));
    // Nastrój przesuwa jakość wokół środka skali: załoga w nastroju 50 nie zmienia nic,
    // 100 podnosi, 0 obniża. Bez odejmowania środka dobry nastrój byłby premią,
    // której zakład nigdy nie traci, czyli stałą ukrytą w kalibracji.
    let z_nastroju = (i32::from(morale.get()) - 50) * t.morale_weight_bp / 10_000;
    let q = i32::from(m.skill_mgmt.get()) - kara + z_nastroju;
    ManagementQuality(q.clamp(0, 100) as u8)
}

/// Jakość, z jaką pracuje zakład po odejściu menedżera: mediana firmy minus próg.
///
/// Zastępstwo nie jest zerem i nie jest wartością neutralną — jest **gorsze niż to,
/// co było**, i to jest cała treść zdania „dobry menedżer to zasób rzadki".
#[must_use]
pub fn replacement_quality(
    mediana_firmy: ManagementQuality,
    t: &ManagerTuning,
) -> ManagementQuality {
    ManagementQuality(mediana_firmy.0.saturating_sub(t.replacement_drop))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hr::tuning::LaborTuning;
    use magnat_core::Entity;
    use std::num::NonZeroU32;

    fn tuning() -> ManagerTuning {
        LaborTuning::load_default()
            .expect("data/tuning/labor.ron")
            .manager
    }

    fn menedzer(skill: u8, zaklady: u32) -> Manager {
        let mut m = Manager::new(
            CitizenId(Entity::new(1, NonZeroU32::MIN)),
            Q::new(skill),
            ManagerStyle::Bureaucrat,
            SimMinute(0),
        );
        for i in 0..zaklady {
            m.take_site(SiteId(Entity::new(100 + i, NonZeroU32::MIN)));
        }
        m
    }

    #[test]
    fn rozpietosc_ponad_optimum_degraduje_liniowo() {
        let t = tuning();
        let dwa = management_quality(&menedzer(80, 2), Q::new(50), &t);
        let cztery = management_quality(&menedzer(80, 4), Q::new(50), &t);
        let szesc = management_quality(&menedzer(80, 6), Q::new(50), &t);
        assert_eq!(dwa.0, 80, "w optimum menedżer pracuje pełną umiejętnością");
        assert!(cztery.0 < dwa.0 && szesc.0 < cztery.0);
        // Liniowo: każdy nadmiarowy zakład kosztuje tyle samo.
        assert_eq!(dwa.0 - cztery.0, cztery.0 - szesc.0);
    }

    #[test]
    fn nastroj_zalogi_przesuwa_jakosc_w_obie_strony() {
        let t = tuning();
        let obojetny = management_quality(&menedzer(60, 1), Q::new(50), &t);
        let zbuntowany = management_quality(&menedzer(60, 1), Q::new(0), &t);
        let oddany = management_quality(&menedzer(60, 1), Q::new(100), &t);
        assert_eq!(obojetny.0, 60, "nastrój 50 jest punktem neutralnym");
        assert!(zbuntowany.0 < obojetny.0);
        assert!(oddany.0 > obojetny.0);
    }

    #[test]
    fn zastepstwo_jest_gorsze_od_tego_co_bylo() {
        let t = tuning();
        let z = replacement_quality(ManagementQuality(70), &t);
        assert!(z.0 < 70);
        // Nawet z fatalnej mediany zastępstwo nie schodzi poniżej skali.
        assert_eq!(replacement_quality(ManagementQuality(5), &t).0, 0);
    }

    #[test]
    fn styl_nadaje_trzy_liczby_ktore_m7b_podejmowal_stala() {
        // `AV-6`: agresja, wagi wyboru kandydata i kolejność świadczeń.
        assert!(ManagerStyle::Dealmaker.aggression() > ManagerStyle::Bureaucrat.aggression());
        assert!(
            ManagerStyle::Taskmaster.hiring().quality_focus
                > ManagerStyle::Dealmaker.hiring().quality_focus
        );
        assert!(
            ManagerStyle::Dealmaker.hiring().price_focus
                > ManagerStyle::Taskmaster.hiring().price_focus
        );
        assert_eq!(ManagerStyle::Bureaucrat.hiring(), HiringPolicy::NEUTRAL);
        // Każdy styl rozdaje ten sam komplet świadczeń, tylko w innej kolejności —
        // inaczej styl odbierałby pracownikowi świadczenie, a nie przestawiał priorytet.
        for s in ManagerStyle::ALL {
            let mut k = s.benefit_order();
            k.sort_unstable();
            assert_eq!(k, [1, 2, 4, 8], "styl {} gubi świadczenie", s.name());
        }
    }

    #[test]
    fn autonomia_rosnie_i_obejmuje_nizsze_szczeble() {
        use magnat_policy::PolicyDomain as D;
        assert!(Autonomy::PricesOnly.covers(D::Pricing));
        assert!(!Autonomy::PricesOnly.covers(D::Stock));
        assert!(Autonomy::Full.covers(D::Stock));
        assert!(Autonomy::PricesAndStaff.covers(D::Hr));
        assert!(!Autonomy::PricesAndStaff.covers(D::Production));
    }

    /// Rejestr z dwoma zakładami jednej firmy — tyle, ile potrzeba, żeby zobaczyć
    /// różnicę między „ten człowiek odszedł" a „ten zakład zmienił kierownika".
    fn rejestr() -> (crate::Firms, magnat_core::SiteId, magnat_core::SiteId) {
        use crate::{Firm, Owner, Site, SitePlacement, Staffing};
        let mut f = crate::Firms::new();
        let key = f.insert(|k| {
            Firm::sole_owner(
                k,
                "Dwa zakłady".to_owned(),
                SimMinute(0),
                magnat_core::DistrictId(0),
                Owner::Player,
            )
        });
        let spec = crate::SiteType {
            key: "t".to_owned(),
            category: crate::SiteTypeCategory::Retail,
            staffing: Vec::<Staffing>::new(),
            capex: crate::catalog::Capex {
                build: 0,
                equip_per_100m2: 0,
            },
            fixed_cost: crate::catalog::FixedCostSpec {
                rent_per_m2: 0,
                admin: 0,
            },
        };
        let mut id = |i: u32| {
            let s = SiteId(Entity::new(600 + i, NonZeroU32::MIN));
            assert!(f.add_site(Site::from_type(
                SitePlacement {
                    id: s,
                    building: magnat_core::BuildingId(Entity::new(700 + i, NonZeroU32::MIN)),
                    district: magnat_core::DistrictId(0),
                    floor_m2: 200,
                    opened: SimMinute(0),
                },
                key,
                crate::SiteTypeId(0),
                &spec,
                |_| (magnat_core::Money(1), magnat_core::Money(2)),
            )));
            s
        };
        let (a, b) = (id(0), id(1));
        (f, a, b)
    }

    #[test]
    fn zmiana_kierownika_w_jednym_zakladzie_nie_zabiera_menedzerowi_drugiego() {
        // Menedżer prowadzący A i B tracił **oba**, gdy zmieniał się kierownik w A:
        // rynek pracy wołał `release_manager` zamiast odpiąć jeden zakład. B spadał
        // do jakości zastępstwa, tracił autonomię i odzyskiwał menedżera dopiero
        // nazajutrz — z wyzerowanym stażem.
        let t = tuning();
        let (mut f, a, b) = rejestr();
        let stary = CitizenId(Entity::new(800, NonZeroU32::MIN));
        for site in [a, b] {
            let m = Manager::new(stary, Q::new(70), ManagerStyle::Coach, SimMinute(0));
            let d = SiteDelegation::new(
                stary,
                magnat_policy::Policy::empty(
                    magnat_core::PolicyId(1),
                    "t",
                    magnat_policy::PolicyDomain::Pricing,
                ),
                Autonomy::Full,
            );
            assert!(f.assign_manager(site, m, d, |_| Q::new(50), &t, magnat_core::Tick(0)));
        }
        assert_eq!(f.manager(stary).expect("menedżer").span(), 2);

        // A dostaje nowego kierownika. B ma zostać przy starym.
        let nowy = CitizenId(Entity::new(801, NonZeroU32::MIN));
        let m = Manager::new(nowy, Q::new(60), ManagerStyle::Taskmaster, SimMinute(0));
        let d = SiteDelegation::new(
            nowy,
            magnat_policy::Policy::empty(
                magnat_core::PolicyId(1),
                "t",
                magnat_policy::PolicyDomain::Pricing,
            ),
            Autonomy::Full,
        );
        assert!(f.assign_manager(a, m, d, |_| Q::new(50), &t, magnat_core::Tick(10)));

        assert_eq!(
            f.manager_of_site(b).map(|m| m.citizen),
            Some(stary),
            "zakład B stracił menedżera, choć nikt go nie dotknął"
        );
        assert_eq!(
            f.manager(stary).expect("stary menedżer").span(),
            1,
            "stary menedżer zachował zakład, którego już nie prowadzi"
        );
        assert_eq!(f.manager_of_site(a).map(|m| m.citizen), Some(nowy));
        // Rozpiętość spadła do jednego zakładu, więc jakość B **wzrosła**: to jest
        // ta sama liczba, która przedtem karała za prowadzenie dwóch naraz.
        assert_eq!(f.site(b).expect("zakład").mgmt.0, 70);
    }

    #[test]
    fn menedzer_bez_zakladu_znika_z_rejestru() {
        // Rekord bez ani jednego zakładu nikogo nie opisuje, a wchodzi do hasha stanu
        // i do zapisu gry — zostawiony, rósłby przez całą grę.
        let t = tuning();
        let (mut f, a, _) = rejestr();
        let c = CitizenId(Entity::new(800, NonZeroU32::MIN));
        let m = Manager::new(c, Q::new(70), ManagerStyle::Coach, SimMinute(0));
        let d = SiteDelegation::new(
            c,
            magnat_policy::Policy::empty(
                magnat_core::PolicyId(1),
                "t",
                magnat_policy::PolicyDomain::Pricing,
            ),
            Autonomy::Full,
        );
        assert!(f.assign_manager(a, m, d, |_| Q::new(50), &t, magnat_core::Tick(0)));
        assert_eq!(f.manager_count(), 1);
        f.detach_site(a, &|_| Q::new(50), &t);
        assert_eq!(f.manager_count(), 0);
        assert!(f.manager_of_site(a).is_none());
        assert_eq!(
            f.site(a)
                .expect("zakład")
                .delegation
                .as_ref()
                .expect("polityka zostaje")
                .autonomy,
            Autonomy::PricesOnly
        );
    }

    #[test]
    fn martwa_strefa_wstrzymuje_powtorne_wykonanie() {
        let mut d = SiteDelegation::new(
            CitizenId(Entity::new(1, NonZeroU32::MIN)),
            magnat_policy::Policy::empty(
                magnat_core::PolicyId(1),
                "t",
                magnat_policy::PolicyDomain::Pricing,
            ),
            Autonomy::Full,
        );
        d.policy.cooldown_h = 12;
        // Pierwsze wykonanie zawsze przechodzi.
        assert!(d.ready(magnat_core::Tick(500)));
        d.last_run = magnat_core::Tick(500);
        assert!(!d.ready(magnat_core::Tick(1_000)));
        assert!(d.ready(magnat_core::Tick(500 + 12 * 60)));
    }
}
