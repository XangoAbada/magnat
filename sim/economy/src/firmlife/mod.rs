//! Powstawanie i upadek firm oraz wejście sieci zewnętrznych (M7f WP15, §5.14).
//!
//! # Dlaczego to mieszka w `sim/economy`
//!
//! Bo założenie firmy to trzy rzeczy naraz: wpis do rejestru firm (`sim/firms`),
//! rachunek w księgach (`Books`) i lokal z półką (`Market`). Dwa ostatnie są tutaj
//! i wyprowadzić się nie mogą — `sim/firms` nie widzi `sim/economy` i widzieć nie
//! może (`AY-3`). To ten sam podział, którym `D2` rozciął rynek pracy: mechanizm
//! jest tam, gdzie dane, a decyzja tam, gdzie firma.
//!
//! # Trzy wejścia i jedno wyjście
//!
//! - **skan dzienny** — mieszkaniec z ambicją i oszczędnościami zakłada firmę
//!   ([`founding`]), limitowany, deterministyczny;
//! - **kwartał firmy** — tier strategiczny każe otworzyć albo zwinąć zakład,
//!   a jego decyzja przychodzi listą w `FirmAiDay` ([`expand`]);
//! - **miesiąc miasta** — sieć zewnętrzna przekracza próg i wchodzi ([`chains`]).
//!
//! Wyjściem jest [`FirmLifeDay`] — liczby dla panelu, balansatora i testu pasma.

use magnat_core::{Money, Tick};
use magnat_ecs::World;

use crate::ai_run::FirmAiDay;
use crate::market::Market;

pub mod chains;
pub mod expand;
pub mod founding;

pub use chains::{ChainCatalog, ChainSpec, CHAINS_SCHEMA_VERSION};
pub use founding::{
    districts_with_seed, found, founding_score, retail_site_type, Found, FoundingIntent,
    FoundingParams,
};

/// Co doba zrobiła z populacją firm.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FirmLifeDay {
    /// Firmy założone przez mieszkańców.
    pub founded: u32,
    /// Zakłady otwarte przez tier strategiczny istniejących firm.
    pub opened: u32,
    /// Firmy zwinięte dobrowolnie — **nie** upadłości; te prowadzi `corpfin`.
    pub wound_down: u32,
    /// Wejścia sieci zewnętrznych.
    pub chain_entries: u32,
    /// Kapitał wniesiony z zewnątrz miasta — zarejestrowany punkt emisji (`D10`).
    pub capital_in: Money,
    /// Kapitał wniesiony przez mieszkańców. To **nie jest** emisja: pieniądz
    /// przechodzi z komponentu gospodarstwa na rachunek firmy i sumy się zgadzają.
    pub capital_own: Money,
}

/// Zapis tego, co cykl życia firm zrobił — zasób świata dla panelu i balansatora.
///
/// Nie wchodzi do hasha stanu: to są **liczniki**, a nie stan. Stanem jest rejestr
/// firm, rynek i księgi, i to one się haszują. Licznik w hashu znaczyłby, że
/// dopisanie metryki do panelu unieważnia każdy zapis gry.
#[derive(Clone, Copy, Debug, Default)]
pub struct FirmLifeLog {
    /// Ostatnia doba.
    pub last: FirmLifeDay,
    /// Od początku przebiegu.
    pub total: FirmLifeDay,
    /// Decyzje AI firm od początku przebiegu — wejście wskaźnika wyjaśnialności
    /// (§7.8: licznik decyzji == licznik zapisanych powodów).
    pub ai: AiTotals,
}

/// Ile decyzji firm AI zapadło i jakich.
#[derive(Clone, Copy, Debug, Default)]
pub struct AiTotals {
    pub ops_firms: u64,
    pub tactical_firms: u64,
    pub strategic_firms: u64,
    pub margin_moves: u64,
    pub restock_moves: u64,
    pub strategy_changes: u64,
    pub policies_adopted: u64,
    pub campaigns_started: u64,
    pub sites_closed: u64,
    pub sites_opened: u64,
    pub wind_downs: u64,
}

impl AiTotals {
    /// Ile **akcji** firmy AI wykonały. To jest lewa strona równania z §7.8 —
    /// prawą jest liczba powodów zapisanych w dziennikach firm.
    #[must_use]
    pub fn actions(&self) -> u64 {
        self.margin_moves
            + self.restock_moves
            + self.strategy_changes
            + self.policies_adopted
            + self.campaigns_started
            + self.sites_closed
            + self.sites_opened
            + self.wind_downs
    }

    fn merge(&mut self, d: &FirmAiDay) {
        self.ops_firms += u64::from(d.firms_ops);
        self.tactical_firms += u64::from(d.firms_tactical);
        self.strategic_firms += u64::from(d.firms_strategic);
        self.margin_moves += u64::from(d.margin_moves);
        self.restock_moves += u64::from(d.restock_moves);
        self.strategy_changes += u64::from(d.strategy_changes);
        self.policies_adopted += u64::from(d.policies_adopted);
        self.campaigns_started += u64::from(d.campaigns_started);
        self.sites_closed += d.to_close.len() as u64;
        self.sites_opened += d.to_open.len() as u64;
        self.wind_downs += d.to_wind_down.len() as u64;
    }
}

impl FirmLifeLog {
    /// Zapisuje dobę: co zrobiła AI i co z tego wyszło w populacji firm.
    pub fn record(&mut self, ai: &FirmAiDay, zycie: FirmLifeDay) {
        self.ai.merge(ai);
        self.last = zycie;
        self.total.merge(zycie);
    }
}

impl FirmLifeDay {
    /// Dokłada wynik innej doby. Publiczne, bo sumuje je też scenariusz.
    pub fn merge(&mut self, other: FirmLifeDay) {
        self.add(other);
    }

    fn add(&mut self, other: FirmLifeDay) {
        self.founded += other.founded;
        self.opened += other.opened;
        self.wound_down += other.wound_down;
        self.chain_entries += other.chain_entries;
        self.capital_in = Money(self.capital_in.get() + other.capital_in.get());
        self.capital_own = Money(self.capital_own.get() + other.capital_own.get());
    }
}

/// Doba cyklu życia firm: najpierw wykonanie decyzji kwartalnych, potem skan
/// przedsiębiorczych mieszkańców, a na początku miesiąca — sieci zewnętrzne.
///
/// Kolejność ma znaczenie i jest kontraktem: **najpierw znikają firmy, potem
/// powstają.** Odwrotnie limit dzienny zjadałaby firma, która i tak tego samego dnia
/// się zwinie, a pasmo liczby firm z bramki mierzyłoby wtedy kolejność wywołań,
/// a nie gospodarkę.
pub fn step_day(world: &mut World, market: &Market, ai: &FirmAiDay, t: Tick) -> FirmLifeDay {
    let mut d = FirmLifeDay::default();
    d.add(expand::wind_down_all(world, market, &ai.to_wind_down, t));
    d.add(expand::open_all(world, market, &ai.to_open, t));
    let doba = t.get() / magnat_core::time::MINUTES_PER_DAY;
    if magnat_agents::is_month_start(doba) {
        d.add(chains::step_month(world, market, t));
    }
    d.add(founding::scan_day(world, market, t));
    d
}
