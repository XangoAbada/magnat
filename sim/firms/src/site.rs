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
/// W M7a ma dwie pozycje, bo dwie ma pisarza: koszt pracy liczy lista płac, koszt stały
/// katalog typów zakładów. Przychody i koszt własny dokłada M7e, kiedy zacznie je czytać —
/// pola bez pisarza byłyby zerami udającymi pomiar.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct SitePnlMonth {
    /// Numer miesiąca od startu świata.
    pub month: u32,
    pub labor: Money,
    pub fixed: Money,
}

impl SitePnlMonth {
    #[must_use]
    pub fn cost(&self) -> Money {
        Money(self.labor.get().saturating_add(self.fixed.get()))
    }
}

impl HashState for SitePnlMonth {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.month);
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
    pub pnl: Ring<SitePnlMonth, 36>,
    pub opened: SimMinute,
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
            pnl: Ring::new(),
            opened: at.opened,
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
        vitals: &impl Fn(CitizenId) -> Option<(magnat_agents::Vitals, Q)>,
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
    /// `vitals` zwraca formę i umiejętność mieszkańca; `None` znaczy, że mieszkańca
    /// już nie ma (zmarł, wyprowadził się) i jego etat nie pracuje. Domknięcie zamiast
    /// traitu, bo konsument jest jeden i zna swoje źródło danych.
    #[must_use]
    pub fn effective_labor(
        &self,
        roles: &RoleTable,
        vitals: &impl Fn(CitizenId) -> Option<(magnat_agents::Vitals, Q)>,
    ) -> Qty {
        let mut suma: i64 = 0;
        for p in &self.positions {
            let w = roles.weights(p.role);
            for e in &p.filled {
                if let Some((body, skill)) = vitals(e.citizen) {
                    suma += effective_labor(&body, skill, self.tech, self.mgmt, &w).0;
                }
            }
        }
        Qty(suma)
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
        self.pnl.hash_state(h);
        self.opened.hash_state(h);
    }
}
