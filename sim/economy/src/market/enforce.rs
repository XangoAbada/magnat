//! Co urząd i szara strefa robią zakładowi (M8d WP8, PRD §10.4).
//!
//! Osobny plik z tego samego powodu, dla którego osobny jest [`super::tax`]: to jest
//! **trzeci temat w tym samym typie**. `api.rs` odpowiada na pytania o rynek, `tax.rs`
//! na pytania o daninę, a ten plik na pytanie „co się dzieje z zakładem, kiedy ktoś
//! go przyciska" — nieważne, czy przyciska go własny wynik (szara strefa), sanepid
//! (zawieszenie), czy złodziej (ubytek inwentaryzacyjny).
//!
//! Reguła jest ta sama co przy daninie: **kwotę i próg zna `sim/city`, a rynek oddaje
//! fakty i przyjmuje zapisy**. Ten plik nie wie, jaki udział obrotu wolno ukryć ani
//! przy jakim pokryciu policyjnym kradnie się ile — wie tylko, jak wykonać decyzję.

use magnat_core::{LossKind, Mass, ServiceCoverage, ServiceKind};

use super::*;

/// Zakład widziany oczami urzędu (M8d WP8).
///
/// Jeden wiersz na zakład, kolejność po `SiteId` rosnąco — urzędy wybierają po nim
/// kandydatów do kontroli, a kolejność wyboru wchodzi do stanu świata (00 §3.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SiteEnforcementRow {
    pub site: SiteId,
    pub firm: FirmId,
    /// Utarg **zadeklarowany w ostatnich dwunastu domkniętych miesiącach**.
    ///
    /// Okno, a nie licznik od początku świata, i to jest różnica, która ma skutek:
    /// udział w obrocie **całego życia** miasta jest dla zakładu otwartego wczoraj
    /// nieosiągalny z definicji, więc próg antymonopolowy przestawałby działać
    /// razem z wiekiem świata. Ta sama podstawa jedzie do domiaru.
    pub declared_revenue: Money,
    pub unreported_bps: u16,
    /// Masa odpisana z powodu przekroczonego terminu — podstawa sprawy sanitarnej.
    pub expired_mass: Mass,
    pub suspended: bool,
    pub closed: bool,
}

impl Market {
    /// Ustawia udział obrotu poza deklaracją. Zwraca `false`, gdy zakładu nie ma.
    pub fn set_unreported_bps(&self, site: SiteId, bps: u16) -> bool {
        let mut m = self.lock();
        let Some(i) = m.by_site.get(&site).copied() else {
            return false;
        };
        m.shops[i as usize].unreported_bps = bps.min(10_000);
        true
    }

    #[must_use]
    pub fn unreported_bps_of(&self, site: SiteId) -> u16 {
        let m = self.lock();
        m.by_site
            .get(&site)
            .map_or(0, |i| m.shops[*i as usize].unreported_bps)
    }

    /// Zawiesza działalność zakładu do wskazanego ticku (`Remedy::Closure`).
    ///
    /// Zawieszenie **nie jest zamknięciem**: zakład wraca sam, gdy tick minie, i nie
    /// traci ani księgi, ani lokalu. Sanepid zamykający restaurację na zawsze byłby
    /// karą śmierci za brudną lodówkę.
    pub fn suspend_site(&self, site: SiteId, until: Tick) -> bool {
        let mut m = self.lock();
        let Some(i) = m.by_site.get(&site).copied() else {
            return false;
        };
        m.shops[i as usize].suspended_until = until;
        true
    }

    /// Zeruje kartotekę sanitarną zakładu po zamkniętej kontroli.
    ///
    /// Bez tego licznik jest **narastający od otwarcia zakładu**, więc sklep, który
    /// raz przekroczył próg, przekracza go już zawsze: sprawa kończy się zawieszeniem,
    /// zawieszenie mija, próg dalej przekroczony, sprawa otwiera się znowu. Pomiar
    /// po 120 dobach gry: 191 zamkniętych spraw na 60 sklepów, czyli po trzy
    /// zawieszenia na sklep — kara zamieniła się w stan.
    pub fn reset_expired_mass(&self, site: SiteId) -> bool {
        let mut m = self.lock();
        let Some(i) = m.by_site.get(&site).copied() else {
            return false;
        };
        m.shops[i as usize].expired_mass = Mass::ZERO;
        true
    }

    /// Ile zakładów ukrywa część obrotu i jaki jest średni udział wśród nich.
    ///
    /// Do raportu balansatora — ryzyko `R10` mówi „cel 5–20 % populacji firm AI",
    /// a udział, którego nikt nie mierzy, jest tak samo martwy jak danina,
    /// której nikt nie naliczył.
    #[must_use]
    pub fn shadow_stats(&self) -> (u32, u32) {
        let m = self.lock();
        let (mut ile, mut suma) = (0u32, 0u32);
        for s in &m.shops {
            if s.unreported_bps > 0 && !s.closed {
                ile += 1;
                suma += u32::from(s.unreported_bps);
            }
        }
        (ile, suma.checked_div(ile).unwrap_or(0))
    }

    /// Wiersze dla urzędów, po `SiteId` rosnąco.
    #[must_use]
    pub fn enforcement_view(&self, t: Tick) -> Vec<SiteEnforcementRow> {
        let m = self.lock();
        let mut out: Vec<SiteEnforcementRow> = m
            .shops
            .iter()
            .map(|s| SiteEnforcementRow {
                site: s.site,
                firm: s.firm,
                declared_revenue: Money(
                    s.ledger
                        .periods
                        .iter()
                        .rev()
                        .take(12)
                        .map(|p| p.statement.revenue.get())
                        .sum(),
                ),
                unreported_bps: s.unreported_bps,
                expired_mass: s.expired_mass,
                suspended: s.suspended_until.get() > t.get(),
                closed: s.closed,
            })
            .collect();
        out.sort_by_key(|r| r.site.0.to_bits());
        out
    }

    /// Ubytki inwentaryzacyjne miesiąca — kradzież, której skala zależy od policji.
    ///
    /// To jest kanał skutku „posterunek → straty w sklepach" z §5.3 i zarazem jedyne
    /// miejsce w grze, w którym `LossKind::Theft` w ogóle powstaje. Odpis idzie przez
    /// [`magnat_supply::Store::write_off`], więc bilans masy (00 §6) domyka się sam:
    /// towar schodzi z `consumed` do `losses[Theft]`, a nie znika.
    ///
    /// Zwraca `(liczba zakładów z odpisem, wartość odpisu)`.
    pub fn shrinkage(&self, coverage: &ServiceCoverage, base_bp: u32, t: Tick) -> (u32, Money) {
        let mut m = self.lock();
        let chain = m.chain.clone();
        let mut ch = chain.lock();
        let (mut zakladow, mut razem) = (0u32, Money::ZERO);
        for i in 0..m.shops.len() {
            if m.shops[i].closed {
                continue;
            }
            // Pokrycie 100 zdejmuje kradzież do zera, pokrycie 0 zostawia stawkę
            // bazową. Liniowo, bo krzywa bez danych, które by ją uzasadniły, jest
            // ozdobą — a stawkę bazową stroi balansator (`data/tuning/city.ron`).
            let bezpieczenstwo =
                u32::from(coverage.at(DistrictId(m.shops[i].district), ServiceKind::Police).get());
            let bp = base_bp * (100 - bezpieczenstwo.min(100)) / 100;
            if bp == 0 {
                continue;
            }
            let (backroom, shelf_slot) = (m.shops[i].backroom, m.shops[i].shelf_slot);
            let linie: Vec<GoodId> = m.shops[i].shelf.lines.iter().map(|l| l.good).collect();
            let przed = ch.store.write_offs_total();
            for good in linie {
                for slot in [shelf_slot, backroom] {
                    let jest = ch.store.shelf_state(slot, good).mass;
                    let ile = Mass(jest.0 * i64::from(bp) / 10_000);
                    if ile.0 <= 0 {
                        continue;
                    }
                    ch.store.write_off(slot, good, ile, LossKind::Theft);
                }
            }
            let odpis = Money(ch.store.write_offs_total().get() - przed.get());
            if odpis.get() <= 0 {
                continue;
            }
            let _ = ledger::post(
                &mut m.shops[i].ledger,
                JournalEntry::new(
                    t,
                    DecisionReason::Unspecified,
                    &[
                        (LedgerAccount::WriteOffExpense, odpis),
                        (LedgerAccount::InventoryGoods, Money(-odpis.get())),
                    ],
                ),
            );
            m.stats.write_offs += 1;
            m.stats.write_off_value = Money(m.stats.write_off_value.get() + odpis.get());
            zakladow += 1;
            razem = Money(razem.get() + odpis.get());
        }
        (zakladow, razem)
    }
}

impl Market {
    /// Miesięczna decyzja zakładu o szarej strefie (M8d WP8, ryzyko `R10`).
    ///
    /// **Szara strefa jest odpowiedzią na przyciśnięcie, a nie cechą charakteru.**
    /// Zakład, który domknął miesiąc pod kreską, ukrywa o `step_bp` więcej; zakład
    /// z dodatnim wynikiem wraca do deklarowania o `relief_bp`. Dzięki temu udział
    /// szarej strefy w populacji firm jest **wynikiem koniunktury**, a nie liczbą
    /// wpisaną w dane — a to jest dokładnie to, czego wymaga `R10`: ani wszyscy,
    /// ani nikt.
    ///
    /// Progi przychodzą z `data/tuning/city.ron`, bo to kalibracja M8, a nie
    /// osobowość cenowa firmy z `data/economy/shop.ron`. Reguła mieszka tutaj, bo
    /// to jest decyzja **zakładu** — miasto jej nie podejmuje, tylko stroi świat,
    /// w którym ona zapada.
    ///
    /// `distress_bp` mówi, jak głęboka strata liczy się jako przyciśnięcie —
    /// w punktach bazowych utargu miesiąca. Zakład między progiem a zerem stoi
    /// w miejscu: nie pogłębia ukrywania, ale też z niego nie wychodzi.
    ///
    /// Zwraca zmienione zakłady: `(zakład, nowy udział, wynik, który go wywołał)`.
    pub fn update_shadow_share(
        &self,
        step_bp: u16,
        relief_bp: u16,
        ceiling_bp: u16,
        distress_bp: u32,
    ) -> Vec<(SiteId, u16, Money)> {
        let mut m = self.lock();
        let mut out = Vec::new();
        for i in 0..m.shops.len() {
            if m.shops[i].closed {
                continue;
            }
            let Some(okres) = m.shops[i].ledger.periods.last() else {
                continue;
            };
            let wynik = okres.statement.net_result();
            // **Strata musi być dotkliwa, a nie tylko ujemna.** Zakład, który
            // zamknął miesiąc złotówkę pod kreską, nie zaczyna oszukiwać; zakład,
            // który stracił trzy procent obrotu, zaczyna. Bez tego progu w szarej
            // strefie kończy **cała** populacja i mechanizm przestaje różnicować.
            let prog = -(okres.statement.revenue.get() * i64::from(distress_bp) / 10_000);
            let teraz = m.shops[i].unreported_bps;
            let nowy = if wynik.get() < prog {
                (teraz + step_bp).min(ceiling_bp)
            } else if wynik.get() > 0 {
                teraz.saturating_sub(relief_bp)
            } else {
                teraz
            };
            if nowy == teraz {
                continue;
            }
            m.shops[i].unreported_bps = nowy;
            out.push((m.shops[i].site, nowy, wynik));
        }
        out
    }
}

impl Market {
    /// Utarg **zadeklarowany** w ostatnich `months` domkniętych miesiącach.
    ///
    /// Z migawek domknięć, a nie z dziennika: dziennik zakładu jest pierścieniem
    /// i po roku gry nie pamięta stycznia, a podstawa domiaru musi. Ten sam powód,
    /// dla którego z nich liczy się podstawę CIT-u.
    #[must_use]
    pub fn declared_revenue_recent(&self, site: SiteId, months: usize) -> Money {
        let m = self.lock();
        let Some(i) = m.by_site.get(&site).copied() else {
            return Money::ZERO;
        };
        let okresy = &m.shops[i as usize].ledger.periods;
        let od = okresy.len().saturating_sub(months);
        Money(okresy[od..].iter().map(|p| p.statement.revenue.get()).sum())
    }
}
