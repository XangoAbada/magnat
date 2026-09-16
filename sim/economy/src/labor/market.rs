//! Rynek pracy miasta — stan i doba (M7b, M7 §5.5).
//!
//! Osobno od [`super`], bo to są dwa tematy: tam jest **kontrakt z otoczeniem**
//! (port do mieszkańców, fakty o człowieku, podsumowanie doby), tutaj **rynek**
//! — arena ofert, ewidencja szukających, statystyki i kolejność kroków doby.

use magnat_core::{
    Arena, CitizenId, DistrictId, HashState, JobRoleId, Money, SimMinute, StateHasher, Tick, Q,
};
use magnat_firms::{Firms, LaborTuning, RoleTable};
use std::collections::BTreeMap;

use super::offer::{Application, JobIndex, JobOffer, JobOfferId};
use super::stats::LaborMarketStats;
use super::{bidding, hr, matching, LaborDay, Seeker, Workforce};

/// Rynek pracy miasta.
pub struct LaborMarket {
    pub(crate) tuning: LaborTuning,
    pub(crate) roles: RoleTable,
    pub(crate) offers: Arena<JobOffer>,
    pub(crate) index: JobIndex,
    /// Zgłoszenia doby. Czyszczone po rozstrzygnięciu — aplikacja, której nikt nie
    /// rozpatrzył, jest aplikacją odrzuconą, a nie kolejką rosnącą w nieskończoność.
    pub(crate) apps: Vec<Application>,
    pub(crate) seekers: BTreeMap<CitizenId, Seeker>,
    pub(crate) stats: LaborMarketStats,
    pub(crate) day: u32,
    pub(crate) last: LaborDay,
}

impl LaborMarket {
    #[must_use]
    pub fn new(tuning: LaborTuning, roles: RoleTable) -> LaborMarket {
        LaborMarket {
            tuning,
            roles,
            offers: Arena::new(),
            index: JobIndex::new(),
            apps: Vec::new(),
            seekers: BTreeMap::new(),
            stats: LaborMarketStats::default(),
            day: 0,
            last: LaborDay::default(),
        }
    }

    #[must_use]
    pub fn stats(&self) -> &LaborMarketStats {
        &self.stats
    }

    #[must_use]
    pub fn last_day(&self) -> LaborDay {
        self.last
    }

    /// Strojenie kadr — potrzebne postępowaniu upadłościowemu, żeby policzyć odprawę
    /// tą samą regułą co zwykłe zwolnienie (M7d). `Copy`, więc to jest odczyt,
    /// a nie wypożyczenie rynku.
    #[must_use]
    pub fn hr_tuning(&self) -> magnat_firms::HrTuning {
        self.tuning.hr
    }

    #[must_use]
    pub fn roles(&self) -> &RoleTable {
        &self.roles
    }

    #[must_use]
    pub fn open_offers(&self) -> usize {
        self.offers.len()
    }

    #[must_use]
    pub fn seekers(&self) -> usize {
        self.seekers.len()
    }

    /// Indeks niedoboru zawodu w dzielnicy — kontrakt §6 do M8 i M10.
    #[must_use]
    pub fn shortage_index(&self, role: JobRoleId, district: DistrictId) -> u16 {
        self.stats.shortage_index(role, district)
    }

    /// Publikacja oferty. Wejście gracza (M9) i mostu stawiającego miasto.
    pub fn post_offer(&mut self, draft: JobOffer) -> JobOfferId {
        let id = self.offers.insert(draft);
        self.index.mark_dirty();
        id
    }

    /// Zgłoszenie kandydata. Zwraca `false`, gdy oferta już nie istnieje albo
    /// gdy jest ofertą bezpośrednią do kogoś innego.
    pub fn apply_for(
        &mut self,
        offer: JobOfferId,
        citizen: CitizenId,
        expectation: Money,
        skill: Q,
        education: u8,
        now: SimMinute,
    ) -> bool {
        let Some(o) = self.offers.get_mut(offer) else {
            return false;
        };
        if o.targeted.is_some_and(|t| t != citizen) {
            return false;
        }
        o.applicants = o.applicants.saturating_add(1);
        self.apps.push(Application {
            offer,
            citizen,
            wage_expectation: expectation,
            skill,
            education,
            submitted: now,
        });
        true
    }

    /// Płaca progowa kandydata (§5.5 pkt 3).
    ///
    /// Punktem wyjścia jest ostatnia płaca, a gdy człowiek nigdy nie pracował —
    /// mediana zawartych umów w zawodzie, a gdy i tej nie ma — mediana ofert.
    /// Gdy rynek milczy w każdy z tych trzech sposobów, oczekiwania nie ma:
    /// pierwszy pracownik w zawodzie przyjmie to, co dadzą, i **to on** ustawi medianę.
    #[must_use]
    pub fn reservation_wage(&self, s: &Seeker, role: JobRoleId, district: DistrictId) -> Money {
        let w = &self.tuning.wage;
        // Dwa punkty wyjścia i dwie różne kotwice. Kto pracował, chce **trochę więcej**
        // niż ostatnio. Kto nie pracował nigdy, kotwiczy **poniżej** mediany zawodu:
        // nie ma czym uzasadnić stawki powyżej tego, co rynek płaci sprawdzonym.
        let (baza, start_bp) = if s.last_wage.get() > 0 {
            (s.last_wage, w.reservation_start_bp)
        } else if let Some(m) = self.stats.median_accepted(role, district) {
            (m, w.reservation_newcomer_bp)
        } else if let Some(m) = self.stats.median_posted(role, district) {
            (m, w.reservation_newcomer_bp)
        } else {
            return Money::ZERO;
        };
        let dni = i32::try_from(self.day.saturating_sub(s.since_day)).unwrap_or(i32::MAX);
        let bp = start_bp
            .saturating_sub(w.reservation_decay_bp_per_day.saturating_mul(dni))
            .max(w.reservation_floor_bp);
        Money(baza.get().saturating_mul(i64::from(bp)) / 10_000)
    }

    /// Doba rynku pracy. Kolejność kroków jest kontraktem — powód przy każdym z nich.
    pub fn step_day(
        &mut self,
        firms: &mut Firms,
        people: &mut impl Workforce,
        seed: u64,
        tick: Tick,
    ) -> LaborDay {
        let now = SimMinute(tick.0);
        self.day = (tick.0 / 1440) as u32;
        let mut d = LaborDay::default();

        // 1. Domknięcie po M3: kto zniknął z rynku pracy, ten nie zajmuje etatu.
        //    Przed wszystkim innym, bo zwolniony etat ma dziś szansę się obsadzić.
        hr::reconcile(self, firms, people, now, &mut d);
        // 2. Rotacja: odejścia dobrowolne i zwolnienia — też zwalniają etaty.
        hr::turnover(self, firms, people, seed, now, &mut d);
        // 3. Kadry: świadczenia (codziennie) oraz premie i szkolenia (raz w miesiącu).
        hr::personnel(self, firms, people, &mut d);
        // 3a. Kto kieruje zakładem: obsadzone stanowisko kierownicze staje się
        //     menedżerem (M7c WP7). Po rotacji, bo to ona zwalnia stanowiska.
        hr::reconcile_managers(self, firms, people, now);
        // 3b. Jakość zarządzania na dziś. **Przed** licytacją i przed rotacją jutra,
        //     bo z niej wychodzi i agresja podbicia, i ciśnienie na odejście —
        //     nastrój załogi chodzi z dnia na dzień, więc jakość też.
        let nastroje = hr::site_morale(firms, people);
        let mt = self.tuning.manager;
        firms.refresh_management(|id| nastroje.get(&id).copied().unwrap_or(Q::new(50)), &mt);
        // 4. Wygaśnięcia i licytacja — **przed** publikacją, żeby oferta, która
        //    właśnie wygasła, mogła się dziś ukazać na nowo.
        bidding::expire_and_escalate(self, firms, now, &mut d);
        // 5. Publikacja wakatów, które nie mają jeszcze oferty.
        matching::post_offers(self, firms, now, &mut d);
        // 6. Headhunting — po publikacji, bo korzysta z policzonego niedoboru.
        bidding::headhunt(self, firms, people, now, &mut d);
        self.index.rebuild(&self.offers);
        // 7. Zgłoszenia i rozstrzygnięcie.
        matching::collect_applications(self, firms, people, seed, now, &mut d);
        matching::hire(self, firms, people, now, &mut d);
        // 8. Statystyki na jutro — z tego, co rynek dziś zrobił.
        self.refresh_stats(firms, &mut d);

        let (sila, bez_pracy) = people.labour_force(self.day);
        d.labour_force = sila;
        d.unemployed = bez_pracy;
        d.employed = sila.saturating_sub(bez_pracy);
        self.last = d;
        d
    }

    /// Przeliczenie statystyk: wakaty, mediany, indeks niedoboru.
    fn refresh_stats(&mut self, firms: &Firms, d: &mut LaborDay) {
        // Wakaty liczy się z zakładów, a nie z ofert: etat bez wystawionej oferty
        // nadal jest wakatem i nadal ma wejść do mianownika niedoboru.
        for s in self.stats.per_role.values_mut() {
            s.vacancies = 0;
        }
        for (_, site) in firms.sites() {
            for p in &site.positions {
                let wolne = u32::from(p.vacancies());
                if wolne == 0 {
                    continue;
                }
                self.stats.entry(p.role, site.district).vacancies += wolne;
                d.vacancies += wolne;
            }
        }
        // Stawki wiszących ofert i czas ich wiszenia, per klucz.
        let mut posted: BTreeMap<(JobRoleId, DistrictId), (Vec<Money>, u32)> = BTreeMap::new();
        for (_, o) in self.offers.iter() {
            let e = posted.entry((o.role, o.district)).or_default();
            e.0.push(o.wage_month);
            e.1 += u32::from(o.days_open);
        }
        for ((role, district), st) in &mut self.stats.per_role {
            let mut pusty = (Vec::new(), 0);
            let (stawki, dni) = posted.get_mut(&(*role, *district)).unwrap_or(&mut pusty);
            st.recompute(stawki, *dni);
        }
    }
}

impl HashState for LaborMarket {
    /// Stan rynku pracy w hashu świata (00 §3.6).
    ///
    /// Arena ofert wchodzi w kolejności indeksów (`K-16`, `K-29`), razem ze slotami
    /// i generacjami. Indeks **nie wchodzi** — jest pochodną areny. Zgłoszenia wchodzą,
    /// bo między zebraniem a rozstrzygnięciem są jedynym śladem po decyzji kandydata;
    /// na granicy doby i tak są puste.
    fn hash_state(&self, h: &mut StateHasher) {
        self.offers.hash_state(h);
        h.write_u32(self.apps.len() as u32);
        for a in &self.apps {
            a.hash_state(h);
        }
        h.write_u32(self.seekers.len() as u32);
        for (c, s) in &self.seekers {
            c.0.hash_state(h);
            s.hash_state(h);
        }
        self.stats.hash_state(h);
        h.write_u32(self.day);
    }
}
