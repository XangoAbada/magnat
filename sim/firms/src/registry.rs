//! Rejestr firm i zakładów — zasób świata (M7a WP1).
//!
//! `BTreeMap`, a nie `HashMap`: iteracja po firmach wyznacza kolejność decyzji
//! i kolejność wypłat, a te wchodzą do hasha stanu (dokument 00 §3.2).

use magnat_core::hash::{HashState, StateHasher};
use magnat_core::{CitizenId, DecisionReason, SimCalendar, SimMinute, SiteId, Tick};
use std::collections::BTreeMap;

use crate::firm::{Firm, FirmStatus};
use crate::hr::employment::{payday, PayrollItem, PayrollRun};
use crate::key::{due, firm_id, slots, FirmKey, KeyCounter, Scheduler, Tier};
use crate::manager::Manager;
use crate::site::{Site, SitePnlMonth};

/// Kubełki slotów decyzyjnych — **indeks pochodny**, nie stan.
///
/// Bez niego przydział slotów jest skanem po wszystkich firmach w każdym ticku:
/// 10 tys. firm × 1440 minut × 3 poziomy to 43 mln sprawdzeń na dobę gry, czyli
/// budżet z §7.6 zjedzony w całości przez samą pętlę „czy to już". Z kubełkami
/// tick operacyjny dotyka ~7 firm.
///
/// **Do hasha nie wchodzi** i wchodzić nie może: jest funkcją zbioru kluczy,
/// odtwarzalną z niego w całości (`slots` jest czyste). Wchodzi za to kolejka
/// przepełnienia, bo ta jest stanem — przesunięcie decyzji na następny tick
/// zmienia to, kiedy firma zdecyduje.
#[derive(Default)]
struct SlotIndex {
    /// 1440 kubełków po minucie doby.
    ops: Vec<Vec<FirmKey>>,
    /// 30 kubełków po dniu miesiąca.
    tac: Vec<Vec<FirmKey>>,
    /// 90 kubełków po dniu kwartału.
    str_: Vec<Vec<FirmKey>>,
}

impl SlotIndex {
    fn new() -> SlotIndex {
        SlotIndex {
            ops: vec![Vec::new(); 1440],
            tac: vec![Vec::new(); 30],
            str_: vec![Vec::new(); 90],
        }
    }

    /// Klucze rosną monotonicznie, więc `push` zachowuje porządek rosnący
    /// w każdym kubełku — a na tym porządku stoi determinizm kolejności decyzji.
    fn add(&mut self, key: FirmKey) {
        let s = slots(key);
        self.ops[s.ops_minute_of_day as usize].push(key);
        self.tac[s.tac_day_of_month as usize].push(key);
        self.str_[s.str_day_of_quarter as usize].push(key);
    }
}

/// Firmy świata wraz z zakładami.
pub struct Firms {
    pub(crate) firms: BTreeMap<FirmKey, Firm>,
    pub(crate) sites: BTreeMap<SiteId, Site>,
    /// Menedżerowie świata, kluczowani mieszkańcem. `BTreeMap`, bo po tej kolejności
    /// idzie przeliczanie jakości zarządzania, a ono wchodzi do hasha stanu.
    pub(crate) managers: BTreeMap<CitizenId, Manager>,
    counter: KeyCounter,
    scheduler: Scheduler,
    slot_index: SlotIndex,
    /// Ile powodów decyzji zapisano od początku przebiegu.
    ///
    /// **Poza hashem stanu** i to jest świadome: dziennik firmy jest pierścieniem
    /// trzydziestu dwóch wpisów i on wchodzi do hasha, a to jest licznik obok niego.
    /// Wskaźnik wyjaśnialności z §7.8 (licznik decyzji == licznik powodów) potrzebuje
    /// **sumy od początku**, a suma z pierścienia jest nie do odczytania: trzydziesta
    /// trzecia decyzja nadpisuje pierwszą.
    reasons_logged: u64,
    /// Wiedza, projekty, patenty, licencje i towary w obiegu (M10c §5.4).
    ///
    /// Tutaj, a nie w osobnym zasobie, z dwóch powodów naraz: pisze to jeden krok
    /// nad tym samym rejestrem, a doczepienie do `Firms` daje hash stanu bez
    /// drugiego haka. Katalog technologii **nie** jest tutaj — jest wejściem
    /// (`rnd::RndData`) i nie wchodzi do hasha.
    rnd: crate::rnd::RndState,
}

impl Default for Firms {
    fn default() -> Firms {
        Firms {
            firms: BTreeMap::new(),
            sites: BTreeMap::new(),
            managers: BTreeMap::new(),
            counter: KeyCounter::default(),
            scheduler: Scheduler::new(),
            slot_index: SlotIndex::new(),
            reasons_logged: 0,
            rnd: crate::rnd::RndState::default(),
        }
    }
}

impl Firms {
    #[must_use]
    pub fn new() -> Firms {
        Firms::default()
    }

    /// Zakłada firmę i zwraca jej klucz. Klucz nadaje licznik świata, nie wywołujący —
    /// inaczej dwa mosty stawiające miasto mogłyby nadać ten sam.
    pub fn insert(&mut self, mut build: impl FnMut(FirmKey) -> Firm) -> FirmKey {
        let key = self.counter.issue();
        let firm = build(key);
        debug_assert_eq!(firm.key, key, "firma zbudowana pod innym kluczem");
        self.firms.insert(key, firm);
        self.slot_index.add(key);
        key
    }

    /// Wstawia zakład i dopisuje go do listy zakładów firmy.
    ///
    /// Zwraca `false`, jeśli firmy nie ma — zakład bez firmy nie ma komu płacić,
    /// więc cichy wpis byłby gorszy od odmowy.
    pub fn add_site(&mut self, site: Site) -> bool {
        let Some(firm) = self.firms.get_mut(&site.firm) else {
            return false;
        };
        if !firm.sites.contains(&site.id) {
            firm.sites.push(site.id);
        }
        self.sites.insert(site.id, site);
        true
    }

    #[must_use]
    pub fn get(&self, key: FirmKey) -> Option<&Firm> {
        self.firms.get(&key)
    }

    pub fn get_mut(&mut self, key: FirmKey) -> Option<&mut Firm> {
        self.firms.get_mut(&key)
    }

    #[must_use]
    pub fn site(&self, id: SiteId) -> Option<&Site> {
        self.sites.get(&id)
    }

    pub fn site_mut(&mut self, id: SiteId) -> Option<&mut Site> {
        self.sites.get_mut(&id)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.firms.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.firms.is_empty()
    }

    #[must_use]
    pub fn site_count(&self) -> usize {
        self.sites.len()
    }

    /// Stan badań i rozwoju (M10c).
    #[must_use]
    pub fn rnd(&self) -> &crate::rnd::RndState {
        &self.rnd
    }

    pub fn rnd_mut(&mut self) -> &mut crate::rnd::RndState {
        &mut self.rnd
    }

    /// Klucze firm czynnych, rosnąco. Kopia, a nie iterator: wołający zaraz
    /// sięgnie po `&mut self` i pożyczka po mapie by mu na to nie pozwoliła.
    #[must_use]
    pub fn active_keys(&self) -> Vec<FirmKey> {
        self.firms
            .iter()
            .filter(|(_, f)| f.status == FirmStatus::Active)
            .map(|(k, _)| *k)
            .collect()
    }

    /// Firma pod kluczem — alias [`Firms::get`] czytelniejszy w kodzie R&D.
    #[must_use]
    pub fn firm(&self, key: FirmKey) -> Option<&Firm> {
        self.firms.get(&key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (FirmKey, &Firm)> {
        self.firms.iter().map(|(k, f)| (*k, f))
    }

    pub fn sites(&self) -> impl Iterator<Item = (SiteId, &Site)> {
        self.sites.iter().map(|(k, s)| (*k, s))
    }

    /// Encja firmy — dla konsumentów, którzy znają tylko `FirmId` (księgi M5,
    /// partie M6, sklepy).
    #[must_use]
    pub fn id_of(&self, key: FirmKey) -> magnat_core::FirmId {
        firm_id(key)
    }

    /// Wpisuje do kolejek firmy, którym w tej minucie wypadł slot, i zdejmuje z nich
    /// tyle, ile wolno obsłużyć w tym ticku.
    ///
    /// Zwraca listę `(poziom, klucze)` w kolejności poziomów. To jest **jedyne**
    /// wejście do decyzji firm: kolejność wynika z `BTreeMap` i z kolejki FIFO,
    /// więc nie zależy ani od liczby wątków, ani od kolejności zakładania firm.
    pub fn schedule(&mut self, cal: SimCalendar) -> Vec<(Tier, Vec<FirmKey>)> {
        for tier in Tier::ALL {
            let kubelek: &[FirmKey] = match tier {
                Tier::Operational => &self.slot_index.ops[cal.minute_of_day() as usize],
                // Poziomy miesięczny i kwartalny wypadają tylko na pełnej godzinie,
                // więc przez 59 minut na 60 kubełek nie jest nawet dotykany.
                _ if cal.minute_of_hour() != 0 => &[],
                Tier::Tactical => &self.slot_index.tac[(cal.day_of_month() - 1) as usize],
                Tier::Strategic => {
                    let doba = usize::from((cal.month_of_year() - 1) % 3) * 30
                        + usize::from(cal.day_of_month() - 1);
                    &self.slot_index.str_[doba]
                }
            };
            let nalezne: Vec<FirmKey> = kubelek
                .iter()
                .copied()
                // Kubełek mówi „który dzień", `due` domyka „która godzina" i odsiewa
                // firmę zamkniętą. Jedno źródło prawdy o tym, kiedy firma decyduje.
                .filter(|k| due(slots(*k), tier, cal))
                .filter(|k| {
                    self.firms
                        .get(k)
                        .is_some_and(|f| f.status == FirmStatus::Active)
                })
                .collect();
            self.scheduler.enqueue(tier, nalezne);
        }
        Tier::ALL
            .into_iter()
            .map(|t| (t, self.scheduler.take(t)))
            .collect()
    }

    #[must_use]
    pub fn backlog(&self, tier: Tier) -> usize {
        self.scheduler.backlog(tier)
    }

    /// Lista płac za miniony miesiąc dla firm, których dzień wypłaty wypadł dziś.
    ///
    /// Zwraca **fakty do zaksięgowania**, niczego nie księgując: właścicielem pieniądza
    /// jest M5. Przy okazji domyka rachunek wyniku zakładu za ten miesiąc — koszt pracy
    /// i koszt stały są jedynymi pozycjami, które M7a zna.
    ///
    /// Wołać raz na dobę, na granicy doby. Kolejność pozycji jest ustalona
    /// przez `BTreeMap` firm, listę zakładów firmy i kolejność stanowisk.
    pub fn run_payroll(&mut self, cal: SimCalendar) -> PayrollRun {
        let dzis = cal.day_of_month() - 1;
        let miesiac = (cal.day_index() / 30) as u32;
        let mut run = PayrollRun::default();
        let do_wyplaty: Vec<FirmKey> = self
            .firms
            .iter()
            .filter(|(_, f)| f.status == FirmStatus::Active)
            .filter(|(k, _)| payday(**k) == dzis)
            .map(|(k, _)| *k)
            .collect();
        for key in do_wyplaty {
            let zaklady: Vec<SiteId> = self
                .firms
                .get(&key)
                .map(|f| f.sites.to_vec())
                .unwrap_or_default();
            for id in zaklady {
                let Some(site) = self.sites.get_mut(&id) else {
                    continue;
                };
                let mut labor: i64 = 0;
                for p in &site.positions {
                    for e in &p.filled {
                        labor += e.wage_month.get();
                        run.items.push(PayrollItem {
                            firm: key,
                            site: id,
                            citizen: e.citizen,
                            gross: e.wage_month,
                            // Potrącenia to hook M8 (`D6`).
                            deductions: magnat_core::Money::ZERO,
                        });
                    }
                }
                // Koszt kadrowy miesiąca (premie, odprawy, świadczenia, szkolenia)
                // wchodzi do kosztu pracy i zeruje się razem z zamknięciem miesiąca —
                // inaczej narastałby przez całą grę i rachunek wyniku kłamałby coraz
                // bardziej z każdym miesiącem.
                let kadry = std::mem::replace(&mut site.hr_accrued, magnat_core::Money::ZERO);
                if kadry.get() != 0 {
                    run.hr_costs.push((key, id, kadry));
                }
                // Przychód i koszt własny dopisuje później księga sklepu
                // (`Firms::post_revenue`), bo zna je dopiero przy domknięciu okresu.
                // Koszt badań domyka się razem z czynszem, a nie razem z płacami:
                // pensje badaczy są już w `labor`, a to są odczynniki i cudze patenty.
                let badania = std::mem::replace(&mut site.rnd_accrued, magnat_core::Money::ZERO);
                site.pnl.push(SitePnlMonth {
                    month: miesiac,
                    revenue: magnat_core::Money::ZERO,
                    cogs: magnat_core::Money::ZERO,
                    labor: magnat_core::Money(labor.saturating_add(kadry.get())),
                    fixed: magnat_core::Money(
                        site.fixed_cost_month.get().saturating_add(badania.get()),
                    ),
                });
            }
        }
        run
    }

    /// Dopisuje przychód i koszt własny do rachunku wyniku zakładu (M7e).
    ///
    /// Aktualizuje wpis tego miesiąca, jeśli lista płac już go założyła, albo zakłada
    /// go sama. Dwa pisarze jednego wpisu, bo koszty zna lista płac w dniu wypłaty,
    /// a przychód — księga sklepu przy domknięciu okresu, i te dwie chwile nie są
    /// tą samą dobą. Trzecim pisarzem nie zostanie nikt: wpis jest **sumą miesiąca**,
    /// nie dziennikiem, więc dopisywanie do niego w środku miesiąca byłoby
    /// przepisywaniem księgi po raz drugi.
    pub fn post_revenue(
        &mut self,
        site: SiteId,
        month: u32,
        revenue: magnat_core::Money,
        cogs: magnat_core::Money,
    ) -> bool {
        let Some(s) = self.sites.get_mut(&site) else {
            return false;
        };
        if let Some(wpis) = s.pnl.iter_mut().find(|p| p.month == month) {
            wpis.revenue = revenue;
            wpis.cogs = cogs;
            return true;
        }
        s.pnl.push(SitePnlMonth {
            month,
            revenue,
            cogs,
            labor: magnat_core::Money::ZERO,
            fixed: s.fixed_cost_month,
        });
        true
    }

    /// Nadaje firmie cechy: z dyrektora, jeśli firma go ma, z klucza — jeśli nie
    /// (M7e §5.7).
    ///
    /// Woła się przy stawianiu miasta i przy zmianie dyrektora. `director_facts`
    /// zwraca cechy i pozycję mieszkańca; `None` znaczy „tego człowieka już nie ma"
    /// i firma wraca wtedy do cech wylosowanych — zarząd tymczasowy z `D15` nie jest
    /// firmą bez charakteru, tylko firmą o charakterze nie swoim.
    pub fn refresh_personalities(
        &mut self,
        world_seed: u64,
        director_facts: impl Fn(CitizenId) -> Option<(magnat_agents::Personality, magnat_core::Q)>,
    ) {
        for (key, f) in &mut self.firms {
            let cechy = f.director.and_then(&director_facts).map_or_else(
                || crate::personality::FirmPersonality::draw(world_seed, *key),
                |(p, status)| crate::personality::personality_from_director(&p, status),
            );
            f.set_personality(cechy);
        }
    }

    /// Zamyka zakład: zwalnia załogę i zdejmuje go z listy zakładów firmy (M7e WP12).
    ///
    /// Zwraca listę zwolnionych razem z ich rolami — etaty trzeba domknąć po stronie
    /// mieszkańca i po stronie puli wakatów miasta, a tego `sim/firms` nie widzi
    /// i widzieć nie może. Zakład znika z rejestru; jego fizyka (linie, magazyn,
    /// półka) należy do M6 i M5 i to one decydują, co z nią zrobić.
    pub fn close_site(&mut self, site: SiteId) -> Vec<(CitizenId, magnat_core::JobRoleId)> {
        let Some(s) = self.sites.remove(&site) else {
            return Vec::new();
        };
        if let Some(f) = self.firms.get_mut(&s.firm) {
            f.sites.retain(|id| *id != site);
        }
        // Kolejność: po stanowiskach w porządku `JobRoleId`, wewnątrz po obsadzeniu —
        // ta sama, po której idzie lista płac, bo wchodzi do hasha stanu.
        s.positions
            .iter()
            .flat_map(|p| p.filled.iter().map(move |e| (e.citizen, p.role)))
            .collect()
    }

    /// Dopisuje powód decyzji do dziennika firmy (dokument 00 §7).
    pub fn log(&mut self, key: FirmKey, tick: Tick, reason: DecisionReason) {
        if let Some(f) = self.firms.get_mut(&key) {
            f.log_decision(tick, reason);
            self.reasons_logged += 1;
        }
    }

    /// Ile powodów zapisano od początku przebiegu — prawa strona równania z §7.8.
    #[must_use]
    pub fn reasons_logged(&self) -> u64 {
        self.reasons_logged
    }

    /// Ilu ludzi pracuje w mieście — wejście metryk balansatora.
    #[must_use]
    pub fn headcount(&self) -> usize {
        self.sites.values().map(Site::headcount).sum()
    }

    /// Gdzie pracuje ten mieszkaniec. Skan po zakładach, więc **nie na ścieżce gorącej** —
    /// szybkie zapytanie agenta idzie przez komponent `sim/agents::Employment` (`D1`).
    #[must_use]
    pub fn employer_of(&self, c: CitizenId) -> Option<(SiteId, FirmKey)> {
        self.sites.iter().find_map(|(id, s)| {
            s.positions
                .iter()
                .flat_map(|p| p.filled.iter())
                .any(|e| e.citizen == c)
                .then_some((*id, s.firm))
        })
    }

    /// Najstarsza firma świata — pierwszy nadany klucz. Skrót dla testów i panelu.
    #[must_use]
    pub fn first(&self) -> Option<FirmKey> {
        self.firms.keys().next().copied()
    }

    #[must_use]
    pub fn keys_issued(&self) -> u64 {
        self.counter.issued()
    }

    /// Data założenia najmłodszej firmy — wejście do metryk powstawania firm (M7f).
    #[must_use]
    pub fn youngest(&self) -> Option<SimMinute> {
        self.firms.values().map(|f| f.founded).max()
    }
}

impl HashState for Firms {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.firms.len() as u32);
        for (k, f) in &self.firms {
            k.hash_state(h);
            f.hash_state(h);
        }
        h.write_u32(self.sites.len() as u32);
        for (id, s) in &self.sites {
            id.0.hash_state(h);
            s.hash_state(h);
        }
        h.write_u32(self.managers.len() as u32);
        for (c, m) in &self.managers {
            c.0.hash_state(h);
            m.hash_state(h);
        }
        self.counter.hash_state(h);
        // Kolejka przepełnienia **musi** wejść do hasha (M7 §7.5): przesunięcie
        // decyzji na następny tick jest stanem, a nie szczegółem wykonania.
        self.scheduler.hash_state(h);
        // Badania są stanem tak samo jak zapas w magazynie: zmieniają to, co świat
        // zrobi w następnej dobie, i przeżywają zapis gry.
        self.rnd.hash_state(h);
    }
}
