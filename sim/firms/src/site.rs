//! Zakład jako **jednostka zarządcza** (M7a WP1/WP3, M7 §5.2, PRD §7.3).
//!
//! Fizyka zakładu ma już właściciela i nie przeprowadza się tutaj: linie, maszyny,
//! liczniki mediów i rampa siedzą w `magnat_supply::PlantSite` (M6b), a półka, zaplecze
//! i sterownik ceny w `magnat_economy::Shop` (M5b). Łącznikiem jest `SiteId`, tak samo
//! jak było przed M7 — trzeci opis tego samego budynku byłby trzecią prawdą.
//!
//! Tu siedzi to, czego żaden z tamtych nie ma i mieć nie powinien: **kto tu pracuje,
//! na jakich stanowiskach, za ile i pod czyim kierownictwem**.

use magnat_core::hash::{HashState, StateHasher};
use magnat_core::{BuildingId, CitizenId, DistrictId, Money, Qty, SimMinute, SiteId, Q};

use crate::catalog::{SiteType, SiteTypeId};
use crate::hr::employment::Position;
use crate::hr::productivity::{effective_labor, ManagementQuality};
use crate::hr::roles::RoleTable;
use crate::key::FirmKey;
use crate::ring::Ring;

/// Rachunek wyniku zakładu za jeden miesiąc — wejście decyzji taktycznej (M7e §5.9).
///
/// M7a miał dwie pozycje, bo dwie miały pisarza: koszt pracy liczy lista płac, koszt
/// stały katalog typów zakładów. **M7e dokłada przychód i koszt własny** — bez nich
/// nie ma marży, a bez marży sufit licytacji o pracownika zostaje przy krańcu widełek
/// roli i firma nie licytuje „na tyle, na ile ją stać", tylko „na tyle, ile ta praca
/// jest tu warta" (`AU-4`, `AV-2`).
///
/// **Pisarzem przychodu jest księga sklepu**, domykana raz w miesiącu razem z okresem
/// (`Market::close_month`). Zakład produkcyjny księgi nie ma i dlatego ma tu zero —
/// rozliczenie B2B niesie sprzedawcę jako `FirmId`, a nie `SiteId`, więc przypisanie
/// utargu hurtowego do konkretnej linii wymagałoby nowego pola w `supply::Settlement`.
/// Zero jest tu **brakiem pomiaru, a nie pomiarem zera**, i [`SitePnlMonth::margin_bp`]
/// mówi to wprost, zwracając `None`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct SitePnlMonth {
    /// Numer miesiąca od startu świata.
    pub month: u32,
    /// Utarg netto miesiąca. Zero znaczy „nie wiem", nie „nic nie sprzedał".
    pub revenue: Money,
    /// Koszt własny sprzedanego towaru.
    pub cogs: Money,
    pub labor: Money,
    pub fixed: Money,
}

impl SitePnlMonth {
    /// Koszty **operacyjne** zakładu: praca i koszt stały. Bez kosztu własnego —
    /// ten jest po stronie towaru, nie zakładu, i odejmuje się go od utargu osobno.
    #[must_use]
    pub fn cost(&self) -> Money {
        Money(self.labor.get().saturating_add(self.fixed.get()))
    }

    /// Wynik miesiąca: utarg minus koszt własny minus koszty operacyjne.
    #[must_use]
    pub fn result(&self) -> Money {
        Money(
            self.revenue
                .get()
                .saturating_sub(self.cogs.get())
                .saturating_sub(self.cost().get()),
        )
    }

    /// Marża zakładu w punktach bazowych utargu. `None` znaczy **„nie ma z czego
    /// policzyć"** — zakład bez księgi albo miesiąc bez sprzedaży.
    ///
    /// Rozróżnienie jest konieczne, bo konsument (sufit licytacji, decyzja taktyczna)
    /// ma na `None` zachować się jak przed M7e, a nie uznać zakład za nierentowny.
    /// Zero utargu i marża −100% to dwa różne zdania o zakładzie.
    #[must_use]
    pub fn margin_bp(&self) -> Option<i32> {
        let utarg = self.revenue.get();
        if utarg <= 0 {
            return None;
        }
        let bp = self.result().get().saturating_mul(10_000) / utarg;
        Some(bp.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32)
    }
}

impl HashState for SitePnlMonth {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.month);
        self.revenue.hash_state(h);
        self.cogs.hash_state(h);
        self.labor.hash_state(h);
        self.fixed.hash_state(h);
    }
}

/// Gdzie i kiedy stanął zakład — pięć pól, które przychodzą razem z budynku
/// i z generatora miasta, a nie z katalogu typów zakładów.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SitePlacement {
    pub id: SiteId,
    pub building: BuildingId,
    pub district: DistrictId,
    /// Powierzchnia użytkowa w m².
    pub floor_m2: u32,
    pub opened: SimMinute,
}

/// Zakład — strona zarządcza.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Site {
    pub id: SiteId,
    pub firm: FirmKey,
    pub site_type: SiteTypeId,
    pub building: BuildingId,
    pub district: DistrictId,
    /// Powierzchnia użytkowa w m² — z niej wychodzi obsada i czynsz.
    pub floor_m2: u32,
    /// Stanowiska, **posortowane po `JobRoleId`**. Kolejność jest kontraktem:
    /// po niej idzie lista płac, a ta wchodzi do hasha stanu.
    pub positions: Vec<Position>,
    /// Jakość zarządzania. M7a stawia neutralną; menedżera przypisuje M7c.
    pub mgmt: ManagementQuality,
    /// Poziom wyposażenia — ten sam `tech`, który M6 trzyma w `PlantSite`.
    /// Dla zakładu handlowego jest wyposażeniem sklepu.
    pub tech: Q,
    pub fixed_cost_month: Money,
    /// Koszt kadrowy narosły w tym miesiącu: premie, odprawy, świadczenia, szkolenia
    /// (M7b WP6). Lista płac dolicza go do kosztu pracy w rachunku wyniku i zeruje.
    ///
    /// Osobne pole, a nie doliczanie na bieżąco do `pnl`: rachunek wyniku zakładu
    /// domyka się raz w miesiącu i ma opisywać miesiąc, a nie rosnąć w środku doby.
    pub hr_accrued: Money,
    /// Koszt badań narosły w tym miesiącu: budżet materiałowy laboratorium
    /// i opłaty licencyjne (M10c WP10.8).
    ///
    /// Osobne pole od `hr_accrued` i osobna pozycja w rachunku wyniku: płace badaczy
    /// są już w koszcie pracy, a to są odczynniki, prototypy i cudzy patent — czyli
    /// koszt **operacyjny**, nie kadrowy. Domyka się do `SitePnlMonth::fixed` razem
    /// z czynszem i zeruje przy zamknięciu miesiąca, tak samo jak `hr_accrued`:
    /// pozycja, która narasta przez całą grę, kłamie coraz bardziej z każdym miesiącem.
    pub rnd_accrued: Money,
    pub pnl: Ring<SitePnlMonth, 36>,
    pub opened: SimMinute,
    /// Odsetek załogi, która **w tej chwili** strajkuje, w punktach bazowych
    /// (M10e WP10.14, `K-9`). Zero znaczy „pracują".
    ///
    /// Pole jest tu, a nie przy związku, z tego samego powodu co `labor_pct`
    /// w `PlantSite` (`K-44`): piszącym jest `sim/economy` (związki), a czytelnicy
    /// stoją **pod** nim — lista płac w tym crate'cie i sonda generatora zdarzeń,
    /// który zatrzymuje produkcję. Związek w `sim/economy` nie mógłby ich obsłużyć,
    /// bo oba stoją niżej w grafie zależności.
    pub strike_bps: u16,
    /// Suma `strike_bps` po dobach bieżącego miesiąca — mianownikiem jest
    /// `30 × 10 000`. Lista płac odejmuje z niej nieprzepracowane dni i zeruje.
    ///
    /// Osobne pole od `strike_bps`, bo to są dwie różne liczby: jedna mówi, co jest
    /// dziś, druga — za co firma ma nie zapłacić. Wypłata jest miesięczna, więc bez
    /// licznika strajk trwający dwa tygodnie kosztowałby dokładnie tyle samo co
    /// strajk trwający dobę, byle skończył się przed dniem wypłaty.
    pub strike_bp_days: u32,
    /// Menedżer i polityka, jeśli zakład jest zdelegowany (M7c WP7).
    ///
    /// `None` znaczy „prowadzi go właściciel" — dla firmy AI jest to zakład sterowany
    /// tierem operacyjnym M7e, dla gracza zakład, który klika sam. Jedno i drugie jest
    /// poprawnym stanem, a nie brakiem.
    pub delegation: Option<crate::manager::SiteDelegation>,
    /// Profil zmianowości — wejście reguły `ShiftKind::schedule` przy ogłaszaniu
    /// wakatu (`R2-WP37`). Wynika z branży rodzaju zakładu i nie zmienia się.
    ///
    /// Pole, a nie odpytanie katalogu: rynek pracy (`sim/economy::labor`) widzi
    /// rejestr firm, a katalogu rodzajów zakładów nie widzi — ten sam układ, przez
    /// który `labor_pct` siedzi w `PlantSite` (`K-44`).
    pub shift_profile: magnat_agents::ShiftProfile,
}

impl Site {
    /// Zakład obsadzony według katalogu: stanowiska wynikają z typu i powierzchni.
    ///
    /// Etaty są **puste** — obsadza je rynek pracy M7b albo most stawiający miasto.
    /// Zakład powstający z pustą załogą jest poprawnym stanem: tak wygląda otwarcie.
    #[must_use]
    pub fn from_type(
        at: SitePlacement,
        firm: FirmKey,
        site_type: SiteTypeId,
        spec: &SiteType,
        wage_band: impl Fn(magnat_core::JobRoleId) -> (Money, Money),
    ) -> Site {
        let mut positions: Vec<Position> = spec
            .staffing
            .iter()
            .map(|s| {
                Position::new(
                    s.role,
                    s.slots(at.floor_m2),
                    s.managerial,
                    wage_band(s.role),
                )
            })
            .collect();
        // Kolejność stanowisk jest kontraktem: po niej idzie lista płac, a ta wchodzi
        // do hasha. Katalog wolno zapisać w dowolnym porządku i to jest w porządku —
        // porządkuje się tutaj, raz.
        positions.sort_by_key(|p| p.role);
        Site {
            id: at.id,
            firm,
            site_type,
            building: at.building,
            district: at.district,
            floor_m2: at.floor_m2,
            positions,
            mgmt: ManagementQuality::NEUTRAL,
            tech: Q::new(50),
            fixed_cost_month: spec.fixed_cost_month(at.floor_m2),
            hr_accrued: Money::ZERO,
            rnd_accrued: Money::ZERO,
            pnl: Ring::new(),
            opened: at.opened,
            strike_bps: 0,
            strike_bp_days: 0,
            delegation: None,
            shift_profile: spec.category.shift_profile(),
        }
    }

    /// Etaty wymagane przez ten zakład — suma stanowisk, obsadzonych i pustych.
    #[must_use]
    pub fn required_slots(&self) -> u32 {
        self.positions.iter().map(|p| u32::from(p.slots)).sum()
    }

    /// **Pokrycie etatowe w promilach** — to, co M6 mnoży przez przepustowość linii
    /// (`PlantSite::labor_pct`). 1000 znaczy „obsada kompletna i w formie".
    ///
    /// Sufit jest przy 1000 i to jest decyzja, nie zaokrąglenie: wąskim gardłem jest
    /// maszyna, a nie człowiek, więc doskonała załoga pracuje na nominale linii,
    /// a nie ponad nim. Nadwyżka produktywności jest zapasem — widać ją dopiero wtedy,
    /// gdy załoga słabnie, choruje albo się przerzedza.
    //
    // ponytail: gdyby kiedyś miała istnieć nadgodzina albo druga zmiana na tej samej
    // linii, sufit podnosi się tutaj i tylko tutaj — M6 mnoży przez to, co dostanie.
    #[must_use]
    pub fn labor_pct(
        &self,
        roles: &RoleTable,
        vitals: &impl Fn(
            CitizenId,
        )
            -> Option<(magnat_agents::Vitals, Q, magnat_agents::DeprivationPressure)>,
    ) -> u16 {
        let wymagane = self.required_slots();
        if wymagane == 0 {
            // Zakład, który nie potrzebuje nikogo (w pełni zautomatyzowany albo
            // opisany bez obsady), nie jest zakładem stojącym.
            return magnat_supply::PlantSite::FULL_LABOR;
        }
        let mianownik = i64::from(wymagane) * crate::hr::productivity::FULL_TIME;
        let praca = self.effective_labor(roles, vitals).0;
        let promile = praca.saturating_mul(1000) / mianownik;
        u16::try_from(promile.clamp(0, i64::from(magnat_supply::PlantSite::FULL_LABOR)))
            .unwrap_or(magnat_supply::PlantSite::FULL_LABOR)
    }

    /// Dopisuje koszt kadrowy do miesiąca. Jedyna droga zmiany `hr_accrued` —
    /// zerowanie należy do listy płac.
    pub fn accrue_hr(&mut self, kwota: Money) {
        self.hr_accrued = Money(self.hr_accrued.get().saturating_add(kwota.get()));
    }

    /// Dopisuje koszt badań do miesiąca. Jedyna droga zmiany `rnd_accrued` —
    /// zerowanie należy do zamknięcia miesiąca, tak samo jak przy kadrach.
    pub fn accrue_rnd(&mut self, kwota: Money) {
        self.rnd_accrued = Money(self.rnd_accrued.get().saturating_add(kwota.get()));
    }

    /// Liczba zatrudnionych.
    #[must_use]
    pub fn headcount(&self) -> usize {
        self.positions.iter().map(|p| p.filled.len()).sum()
    }

    /// Nieobsadzone etaty razem.
    #[must_use]
    pub fn vacancies(&self) -> u32 {
        self.positions
            .iter()
            .map(|p| u32::from(p.vacancies()))
            .sum()
    }

    /// Miesięczny koszt pracy — suma stawek brutto.
    #[must_use]
    pub fn labor_cost_month(&self) -> Money {
        Money(
            self.positions
                .iter()
                .flat_map(|p| p.filled.iter())
                .map(|e| e.wage_month.get())
                .sum(),
        )
    }

    /// **Kontrakt do M6** (M7 §6): efektywna praca zakładu w milietatach.
    ///
    /// M6 mnoży przez to przepustowość linii i nie zagląda do środka. Suma idzie
    /// po stanowiskach w kolejności `JobRoleId`, a wewnątrz stanowiska w kolejności
    /// obsadzenia — nigdy po mapie (dokument 00 §3.2).
    ///
    /// `vitals` zwraca formę, umiejętność i nacisk deprywacji mieszkańca; `None`
    /// znaczy, że mieszkańca już nie ma (zmarł, wyprowadził się) i jego etat nie
    /// pracuje. Domknięcie zamiast traitu, bo konsument jest jeden i zna swoje
    /// źródło danych.
    #[must_use]
    pub fn effective_labor(
        &self,
        roles: &RoleTable,
        vitals: &impl Fn(
            CitizenId,
        )
            -> Option<(magnat_agents::Vitals, Q, magnat_agents::DeprivationPressure)>,
    ) -> Qty {
        let mut suma: i64 = 0;
        for p in &self.positions {
            let w = roles.weights(p.role);
            for e in &p.filled {
                if let Some((body, skill, dep)) = vitals(e.citizen) {
                    suma += effective_labor(&body, skill, self.tech, self.mgmt, &w, dep).0;
                }
            }
        }
        Qty(suma)
    }

    /// Praca **badawcza** zakładu w milietatach (M10c §5.4).
    ///
    /// To samo, co [`Site::effective_labor`], zawężone do jednego stanowiska.
    /// Osobna funkcja, a nie parametr tamtej, bo pytania są różne: tamto pyta
    /// „ile pracy stoi za tym zakładem" i odpowiada M6, to pyta „jak szybko tu
    /// idą badania" i odpowiada R&D. Wspólna funkcja z filtrem zmusiłaby M6
    /// do podawania roli, której nie zna.
    ///
    /// Zakład bez stanowiska badacza zwraca zero i to jest większość miasta.
    #[must_use]
    pub fn research_labor(
        &self,
        roles: &RoleTable,
        researcher: magnat_core::JobRoleId,
        vitals: &impl Fn(
            CitizenId,
        )
            -> Option<(magnat_agents::Vitals, Q, magnat_agents::DeprivationPressure)>,
    ) -> Qty {
        let mut suma: i64 = 0;
        for p in self.positions.iter().filter(|p| p.role == researcher) {
            let w = roles.weights(p.role);
            for e in &p.filled {
                if let Some((body, skill, dep)) = vitals(e.citizen) {
                    suma += effective_labor(&body, skill, self.tech, self.mgmt, &w, dep).0;
                }
            }
        }
        Qty(suma)
    }

    /// Ilu badaczy faktycznie tu pracuje — mianownik budżetu materiałowego.
    #[must_use]
    pub fn researchers(&self, researcher: magnat_core::JobRoleId) -> u32 {
        self.positions
            .iter()
            .filter(|p| p.role == researcher)
            .map(|p| p.filled.len() as u32)
            .sum()
    }

    /// Podnosi poziom wyposażenia o `delta`, z sufitem skali `Q`.
    ///
    /// Jedyna droga zmiany `tech` po postawieniu zakładu. Sufit jest twardy, bo
    /// `Q` przycina sam — ale zapisujemy go jawnie, żeby było widać, że komplet
    /// drzewa technologii nie robi z warsztatu maszyny doskonałej.
    pub fn raise_tech(&mut self, delta: u8) {
        self.tech = Q::new(self.tech.get().saturating_add(delta));
    }
}

impl HashState for Site {
    fn hash_state(&self, h: &mut StateHasher) {
        self.id.0.hash_state(h);
        self.firm.hash_state(h);
        self.site_type.hash_state(h);
        self.building.0.hash_state(h);
        self.district.hash_state(h);
        h.write_u32(self.floor_m2);
        h.write_u32(self.positions.len() as u32);
        for p in &self.positions {
            p.hash_state(h);
        }
        h.write_u8(self.mgmt.0);
        h.write_u8(self.tech.get());
        self.fixed_cost_month.hash_state(h);
        self.hr_accrued.hash_state(h);
        self.rnd_accrued.hash_state(h);
        self.pnl.hash_state(h);
        self.opened.hash_state(h);
        // Strajk zmienia to, ile zakład produkuje i komu firma płaci — czyli stan
        // gospodarki, a nie szczegół prezentacji.
        h.write_u16(self.strike_bps);
        h.write_u32(self.strike_bp_days);
        // Profil wychodzi z `site_type`, więc do hasha nic nie wnosi — wchodzi
        // mimo to, bo jest polem, a pole można zapisać wbrew wyprowadzeniu.
        h.write_u8(self.shift_profile as u8);
        match &self.delegation {
            None => h.write_u8(0),
            Some(d) => {
                h.write_u8(1);
                d.hash_state(h);
            }
        }
    }
}
