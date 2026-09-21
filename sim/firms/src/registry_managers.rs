//! Menedżerowie w rejestrze firm: przypisanie, odejście i przeliczanie jakości
//! (M7c WP7, M7 §5.4).
//!
//! Osobno od [`crate::registry`], bo to są dwa tematy nad jednym zasobem. Tam mieszka
//! **rejestr firm i zakładów** — kto istnieje, kto komu płaci, kiedy wypada wypłata.
//! Tutaj **kto czym kieruje** i co się dzieje z zakładem, gdy kierujący odchodzi.
//! Podział jest przeniesieniem bloku, nie zmianą kształtu: `Firms` zostaje jednym
//! typem i jednym zasobem świata.

use magnat_core::{CitizenId, DecisionReason, FirmReason, SiteId, Tick, Q};

use crate::hr::productivity::ManagementQuality;
use crate::hr::tuning::ManagerTuning;
use crate::key::FirmKey;
use crate::manager::{
    management_quality, replacement_quality, Manager, ManagerStyle, SiteDelegation,
};
use crate::registry::Firms;

impl Firms {
    #[must_use]
    pub fn manager(&self, c: CitizenId) -> Option<&Manager> {
        self.managers.get(&c)
    }

    #[must_use]
    pub fn manager_count(&self) -> usize {
        self.managers.len()
    }

    pub fn managers(&self) -> impl Iterator<Item = (CitizenId, &Manager)> {
        self.managers.iter().map(|(c, m)| (*c, m))
    }

    /// Styl menedżera prowadzącego ten zakład — wejście licytacji płacowej,
    /// wyboru kandydata i kolejności świadczeń (`AV-6`).
    ///
    /// `None` znaczy „zakładem nie kieruje nikt imiennie": zastępstwo po odejściu
    /// menedżera też wraca `None`, bo zastępstwo nie ma stylu, tylko procedurę.
    #[must_use]
    pub fn style_of(&self, site: SiteId) -> Option<ManagerStyle> {
        let c = self.sites.get(&site)?.delegation.as_ref()?.manager?;
        self.managers.get(&c).map(|m| m.style)
    }

    /// Menedżer prowadzący ten zakład, jeśli jakiś jest.
    #[must_use]
    pub fn manager_of_site(&self, site: SiteId) -> Option<&Manager> {
        let c = self.sites.get(&site)?.delegation.as_ref()?.manager?;
        self.managers.get(&c)
    }

    /// Mediana jakości zarządzania firmy — podstawa jakości zastępstwa (§5.4).
    ///
    /// Mediana, a nie średnia: firma z jednym świetnym zakładem i pięcioma miernymi
    /// nie ma świetnego zastępstwa, tylko mierne.
    #[must_use]
    pub fn median_mgmt(&self, firm: FirmKey) -> ManagementQuality {
        let Some(f) = self.firms.get(&firm) else {
            return ManagementQuality::NEUTRAL;
        };
        let mut q: Vec<u8> = f
            .sites
            .iter()
            .filter_map(|id| self.sites.get(id))
            .map(|s| s.mgmt.0)
            .collect();
        if q.is_empty() {
            return ManagementQuality::NEUTRAL;
        }
        q.sort_unstable();
        ManagementQuality(q[q.len() / 2])
    }

    /// Przypisuje menedżera do zakładu wraz z polityką i autonomią.
    ///
    /// Zwraca `false`, gdy zakładu nie ma. Jakość zarządzania przelicza się **od razu
    /// i dla wszystkich zakładów tego menedżera**: rozpiętość kierowania właśnie
    /// urosła, więc pozostałe zakłady też dostają gorszą obsługę — i to jest cała
    /// treść zdania „dobry menedżer jest zasobem rzadkim".
    ///
    /// Zakład, który miał już menedżera, jest **najpierw od niego odpinany**. Bez tego
    /// poprzedni zachowywałby go na swojej liście: rozpiętość zawyżona (gorsza jakość
    /// na zakładach, których nikt nie dotknął), a `refresh_management` zapisywałaby
    /// `Site::mgmt` z dwóch rekordów naraz — wygrywałby ten, który wypadnie później
    /// w `BTreeMap`. `reconcile_managers` maskowało to, bo zwalnia przed przypisaniem;
    /// edytor M9 i testy nie mają powodu tego powtarzać.
    ///
    /// Powód decyzji ląduje w dzienniku firmy (00 §7), z jakością **sprzed** zmiany.
    pub fn assign_manager(
        &mut self,
        site: SiteId,
        m: Manager,
        delegation: SiteDelegation,
        morale: impl Fn(SiteId) -> Q,
        t: &ManagerTuning,
        now: Tick,
    ) -> bool {
        if !self.sites.contains_key(&site) {
            return false;
        }
        self.detach_site(site, &morale, t);
        let Some(s) = self.sites.get_mut(&site) else {
            return false;
        };
        let poprzednia = s.mgmt;
        let firma = s.firm;
        let c = m.citizen;
        s.delegation = Some(delegation);
        let wpis = self.managers.entry(c).or_insert(m);
        wpis.take_site(site);
        let dotkniete: Vec<SiteId> = wpis.sites.to_vec();
        let skill = wpis.skill_mgmt;
        self.przelicz(&dotkniete, c, &morale, t);
        if let Some(f) = self.firms.get_mut(&firma) {
            f.log_decision(
                now,
                DecisionReason::Firm(FirmReason::ManagerAssigned {
                    site,
                    skill_mgmt: skill,
                    prev: poprzednia.0,
                }),
            );
        }
        true
    }

    /// Zdejmuje menedżera z **jednego** zakładu, zostawiając mu pozostałe.
    ///
    /// To jest różnica, której [`Firms::release_manager`] zrobić nie może: tamta
    /// odpowiada na „ten człowiek przestał pracować", ta na „ten zakład zmienił
    /// kierownika". Mylenie ich kosztuje zakład, którego nikt nie dotknął: menedżer
    /// prowadzący A i B tracił oba, gdy zmieniał się kierownik w A, i odzyskiwał B
    /// dopiero nazajutrz — z wyzerowanym stażem.
    ///
    /// Menedżer bez ani jednego zakładu **znika z rejestru**: rekord bez zakładu nie
    /// opisuje nikogo, a wchodzi do hasha stanu i do zapisu gry.
    pub fn detach_site(&mut self, site: SiteId, morale: &impl Fn(SiteId) -> Q, t: &ManagerTuning) {
        let Some(c) = self
            .sites
            .get(&site)
            .and_then(|s| s.delegation.as_ref())
            .and_then(|d| d.manager)
        else {
            return;
        };
        let zostaly: Vec<SiteId> = match self.managers.get_mut(&c) {
            Some(m) => {
                m.drop_site(site);
                m.sites.to_vec()
            }
            None => Vec::new(),
        };
        if zostaly.is_empty() {
            self.managers.remove(&c);
        } else {
            self.przelicz(&zostaly, c, morale, t);
        }
        if let Some(s) = self.sites.get_mut(&site) {
            if let Some(d) = s.delegation.as_mut() {
                d.manager = None;
                d.autonomy = crate::manager::Autonomy::PricesOnly;
            }
        }
    }

    /// Menedżer przestaje pracować — odchodzi, umiera, zostaje podkupiony.
    ///
    /// Każdy jego zakład spada do **jakości zastępstwa** (mediana firmy minus próg)
    /// i do autonomii `PricesOnly`. Polityka zostaje: to ona jest tym, co zakład
    /// umie robić sam, a menedżer był tym, jak dobrze ją wykonywał.
    ///
    /// Zastępstwo liczy się z mediany **sprzed** odejścia i jest jedną liczbą dla
    /// wszystkich jego zakładów. Mediana czytana w pętli osuwałaby się z każdą
    /// iteracją: drugi zakład tego samego menedżera dostawałby gorsze zastępstwo
    /// niż pierwszy, bez żadnego powodu poza kolejnością.
    pub fn release_manager(&mut self, c: CitizenId, t: &ManagerTuning, now: Tick) -> usize {
        let Some(m) = self.managers.remove(&c) else {
            return 0;
        };
        let mediany: std::collections::BTreeMap<FirmKey, ManagementQuality> = m
            .sites
            .iter()
            .filter_map(|id| self.sites.get(id).map(|s| s.firm))
            .map(|firma| (firma, replacement_quality(self.median_mgmt(firma), t)))
            .collect();
        let mut ile = 0;
        for id in m.sites {
            let Some(s) = self.sites.get_mut(&id) else {
                continue;
            };
            let firma = s.firm;
            let poprzednia = s.mgmt;
            if let Some(d) = s.delegation.as_mut() {
                d.manager = None;
                d.autonomy = crate::manager::Autonomy::PricesOnly;
            }
            let zastepstwo = mediany
                .get(&firma)
                .copied()
                .unwrap_or(ManagementQuality::NEUTRAL);
            s.mgmt = zastepstwo;
            if let Some(f) = self.firms.get_mut(&firma) {
                f.log_decision(
                    now,
                    DecisionReason::Firm(FirmReason::ManagerAssigned {
                        site: id,
                        skill_mgmt: Q::new(zastepstwo.0),
                        prev: poprzednia.0,
                    }),
                );
            }
            ile += 1;
        }
        ile
    }

    /// Przelicza jakość zarządzania zakładów jednego menedżera.
    fn przelicz(
        &mut self,
        sites: &[SiteId],
        c: CitizenId,
        morale: &impl Fn(SiteId) -> Q,
        t: &ManagerTuning,
    ) {
        let Some(m) = self.managers.get(&c) else {
            return;
        };
        let nowe: Vec<(SiteId, ManagementQuality)> = sites
            .iter()
            .map(|id| (*id, management_quality(m, morale(*id), t)))
            .collect();
        for (id, q) in nowe {
            if let Some(s) = self.sites.get_mut(&id) {
                s.mgmt = q;
            }
        }
    }

    /// Odświeża jakość zarządzania wszystkich zdelegowanych zakładów — nastrój załogi
    /// chodzi z dnia na dzień, więc jakość też.
    ///
    /// Wołane raz na dobę przez system rynku pracy, w kolejności `BTreeMap` menedżerów.
    pub fn refresh_management(&mut self, morale: impl Fn(SiteId) -> Q, t: &ManagerTuning) {
        let ludzie: Vec<CitizenId> = self.managers.keys().copied().collect();
        for c in ludzie {
            let sites: Vec<SiteId> = self
                .managers
                .get(&c)
                .map(|m| m.sites.to_vec())
                .unwrap_or_default();
            self.przelicz(&sites, c, &morale, t);
        }
    }
}
