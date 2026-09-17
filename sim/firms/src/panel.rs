//! Migawka panelu firmy — jedyne wejście interfejsu do warstwy zarządczej
//! (M7f WP16, PRD §14.3).
//!
//! # Migawka, a nie referencje
//!
//! Ten sam wybór co przy `ShopPanelSnapshot` (M5e) i `FirmView` (M7e, `BC-1`):
//! panel dostaje **oderwaną od stanu strukturę**, którą wolno trzymać przez klatkę.
//! Referencja do rejestru firm żyłaby przez cały rysunek, a rejestr jest w tym czasie
//! wyjmowany ze świata przez systemy — panel widziałby wtedy raz firmę, raz jej brak.
//!
//! # Czego migawka nie niesie i dlaczego to jest istotne
//!
//! **Żadnej kwoty pochodzącej z modelu makro** (§5.10, `R15`). Tier strategiczny
//! zostawia [`OutlookRow`] — liczbę wariantów, kierunek i margines. Prognoza w formie
//! kwoty jest w tym typie **niewyrażalna**, bo nie ma w nim pola, w które dałoby się
//! ją włożyć. Kwoty do grosza pochodzą wyłącznie z ksiąg firmy.

use magnat_core::{
    CitizenId, DecisionReason, DistrictId, JobRoleId, Money, SimMinute, SiteId, Tick, Trend, Q,
};

use crate::ai::strategic::{Outlook, StrAction};
use crate::firm::{FirmStatus, Owner};
use crate::manager::{Autonomy, ManagerStyle};
use crate::registry::Firms;
use crate::site::SitePnlMonth;
use crate::{FirmKey, FirmPersonality};
use magnat_core::FirmStrategy;

/// Jeden zakład firmy — wiersz tabeli zakładów i węzeł drzewa organizacyjnego.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SiteRow {
    pub site: SiteId,
    pub district: DistrictId,
    pub site_type: String,
    pub headcount: u32,
    pub vacancies: u32,
    /// Miesięczny koszt pracy — z umów, nie z widełek.
    pub labor_cost: Money,
    pub fixed_cost: Money,
    /// Ostatni domknięty miesiąc; `None` znaczy „zakład jeszcze nie ma rachunku".
    pub last_month: Option<SitePnlMonth>,
    /// Ile ostatnich miesięcy zamknął stratą.
    pub months_in_loss: u8,
    /// Menedżer i zakres jego samodzielności; `None` = zakład prowadzi właściciel.
    pub manager: Option<ManagerRow>,
}

/// Menedżer zakładu — węzeł drzewa organizacyjnego.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManagerRow {
    /// `None` znaczy **zastępstwo**: menedżer odszedł, polityka została (§5.4).
    pub citizen: Option<CitizenId>,
    pub skill_mgmt: Q,
    pub style: ManagerStyle,
    pub autonomy: Autonomy,
    /// Ile zakładów prowadzi ten sam człowiek — rozpiętość kierowania.
    pub span: u8,
    pub tenure: SimMinute,
}

/// Jeden pracownik — wiersz listy ludzi.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmployeeRow {
    pub citizen: CitizenId,
    pub site: SiteId,
    pub role: JobRoleId,
    pub wage_month: Money,
    pub since: SimMinute,
    /// Wygładzona ocena wyniku, 0..=1000.
    pub perf: u16,
    pub warnings: u8,
}

/// Wynik ostatniego „co jeśli" — **bez kwoty i bez możliwości jej dodania**.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlookRow {
    /// Warianty, które model porównał.
    pub variants: Vec<StrAction>,
    /// Zwycięzca albo `None`, gdy przewaga nie przekroczyła marginesu.
    pub winner: Option<u8>,
    pub trend: Trend,
    pub error_margin_bp: u32,
    pub at: Tick,
}

impl From<&Outlook> for OutlookRow {
    fn from(o: &Outlook) -> OutlookRow {
        OutlookRow {
            variants: o.variants.to_vec(),
            winner: o.winner,
            trend: o.trend,
            error_margin_bp: o.error_margin_bp,
            at: o.at,
        }
    }
}

/// Wszystko, co karta firmy pokazuje.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FirmPanelSnapshot {
    pub key: FirmKey,
    pub name: String,
    pub founded: SimMinute,
    pub status: FirmStatus,
    pub hq_district: DistrictId,
    pub director: Option<CitizenId>,
    pub owners: Vec<(Owner, u16)>,
    pub personality: FirmPersonality,
    pub strategy: FirmStrategy,
    /// Saldo rachunku firmy — **z ksiąg**, podane przez wołającego. Rejestr firm
    /// pieniądza nie prowadzi i prowadzić nie będzie: właścicielem jest M5.
    pub cash: Money,
    pub sites: Vec<SiteRow>,
    pub staff: Vec<EmployeeRow>,
    /// Ostatnie decyzje z powodami — oś czasu karty (00 §7).
    pub decisions: Vec<(Tick, DecisionReason)>,
    /// Trwająca kampania konkurencyjna, jeśli jakaś trwa.
    pub campaign: Option<crate::ai::reaction::Campaign>,
    pub outlook: Option<OutlookRow>,
}

impl FirmPanelSnapshot {
    /// Suma zatrudnionych we wszystkich zakładach.
    #[must_use]
    pub fn headcount(&self) -> u32 {
        self.sites.iter().map(|s| s.headcount).sum()
    }

    /// Suma nieobsadzonych etatów.
    #[must_use]
    pub fn vacancies(&self) -> u32 {
        self.sites.iter().map(|s| s.vacancies).sum()
    }

    /// Miesięczny koszt firmy: płace plus koszty stałe zakładów.
    #[must_use]
    pub fn monthly_cost(&self) -> Money {
        Money(
            self.sites
                .iter()
                .map(|s| s.labor_cost.get().saturating_add(s.fixed_cost.get()))
                .fold(0i64, i64::saturating_add),
        )
    }

    /// Wynik ostatniego domkniętego miesiąca wszystkich zakładów, które go mają.
    /// `None`, gdy żaden nie ma rachunku — a to jest inne zdanie niż „zero".
    #[must_use]
    pub fn last_result(&self) -> Option<Money> {
        let mierzone: Vec<Money> = self
            .sites
            .iter()
            .filter_map(|s| s.last_month.map(|m| m.result()))
            .collect();
        if mierzone.is_empty() {
            return None;
        }
        Some(Money(
            mierzone
                .iter()
                .map(|m| m.get())
                .fold(0i64, i64::saturating_add),
        ))
    }
}

/// Buduje migawkę panelu firmy.
///
/// `cash` przychodzi z zewnątrz, bo saldo rachunku prowadzi `Books` w `sim/economy`,
/// a ten crate go nie widzi (`AY-3`). `types` zamienia `SiteTypeId` na klucz tekstowy —
/// katalog jest daną, a migawka ma być samowystarczalna, tak samo jak `good_keys`
/// w migawce sklepu.
#[must_use]
pub fn firm_panel(
    firms: &Firms,
    key: FirmKey,
    cash: Money,
    types: &crate::catalog::SiteTypeCatalog,
    outlook: Option<&Outlook>,
) -> Option<FirmPanelSnapshot> {
    let f = firms.get(key)?;
    let mut sites = Vec::new();
    let mut staff = Vec::new();
    for id in &f.sites {
        let Some(s) = firms.site(*id) else { continue };
        for p in &s.positions {
            for e in &p.filled {
                staff.push(EmployeeRow {
                    citizen: e.citizen,
                    site: *id,
                    role: p.role,
                    wage_month: e.wage_month,
                    since: e.since,
                    perf: e.perf_ema,
                    warnings: e.warnings,
                });
            }
        }
        sites.push(SiteRow {
            site: *id,
            district: s.district,
            site_type: types.get(s.site_type).key.clone(),
            headcount: s.headcount() as u32,
            vacancies: s.vacancies(),
            labor_cost: s.labor_cost_month(),
            fixed_cost: s.fixed_cost_month,
            last_month: s.pnl.last().copied(),
            months_in_loss: miesiace_straty(s),
            manager: s.delegation.as_ref().map(|d| ManagerRow {
                citizen: d.manager,
                skill_mgmt: d
                    .manager
                    .and_then(|c| firms.manager(c))
                    .map_or(Q::MIN, |m| m.skill_mgmt),
                style: d
                    .manager
                    .and_then(|c| firms.manager(c))
                    .map_or(ManagerStyle::default(), |m| m.style),
                autonomy: d.autonomy,
                span: d
                    .manager
                    .and_then(|c| firms.manager(c))
                    .map_or(0, |m| m.sites.len() as u8),
                tenure: d
                    .manager
                    .and_then(|c| firms.manager(c))
                    .map_or(SimMinute(0), |m| m.tenure),
            }),
        });
    }
    // Kolejność ludzi jest kolejnością stanowisk w zakładach, czyli deterministyczna;
    // sortowanie po mieszkańcu dałoby listę, w której współpracownicy z jednej zmiany
    // są rozrzuceni — a panel ludzi ma odpowiadać na „kto z kim pracuje" (§6, M10).
    Some(FirmPanelSnapshot {
        key,
        name: f.name.clone(),
        founded: f.founded,
        status: f.status,
        hq_district: f.hq_district,
        director: f.director,
        owners: f.owners.iter().map(|o| (o.owner, o.bp)).collect(),
        personality: f.personality,
        strategy: f.strategy,
        cash,
        sites,
        staff,
        decisions: f.log.iter().map(|d| (d.tick, d.reason)).collect(),
        campaign: f.campaign,
        outlook: outlook.map(OutlookRow::from),
    })
}

/// Ile ostatnich miesięcy zakład zamknął stratą. Liczy od końca i przerywa na
/// pierwszym miesiącu **bez pomiaru**: miesiąc bez przychodu nie jest miesiącem
/// straty, tylko miesiącem, o którym nic nie wiadomo (`SitePnlMonth::margin_bp`).
fn miesiace_straty(s: &crate::site::Site) -> u8 {
    let mut n = 0u8;
    for m in s.pnl.iter().rev() {
        match m.margin_bp() {
            Some(bp) if bp < 0 => n = n.saturating_add(1),
            _ => break,
        }
    }
    n
}
