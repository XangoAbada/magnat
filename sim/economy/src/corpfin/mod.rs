//! Finanse firmy i upadłość (M7d, WP8 + WP9, PRD §7.8).
//!
//! ## Dlaczego w `sim/economy`, a nie w `sim/firms`
//!
//! Plan fazy (§6 „Dostarczam") wypisywał `Loan`, `Lease`, `Bond`, `Bankruptcy`
//! i `Claim` jako typy `sim/firms`. Tak się nie da, i to nie jest kwestia gustu:
//! **`sim/firms` nie zna `Books`** i znać nie może, bo zależność idzie
//! `economy → firms → policy` i odwrócenie jej zamyka cykl, którego Cargo nie
//! zbuduje. Finanse bez ksiąg to same nazwy pól — kredyt, który niczego nie
//! przelewa, i syndyk, który niczego nie wypłaca.
//!
//! Podział jest więc ten sam, który `D2` narzucił rynkowi pracy, a `AY-3` polityce
//! cenowej: **mechanizm jest tam, gdzie dane, a decyzja tam, gdzie firma.**
//! `sim/economy` prowadzi instrumenty i postępowanie; `sim/firms` zostaje przy
//! regułach czystych i przy tym, kto jaką decyzję podjął.
//!
//! ## Co tu jest
//!
//! - [`arrears`] — zaległości: jedna pozycja, dwie strony. Bez niej nieudany przelew
//!   nie zostawiał długu, a trzy mechanizmy M7d stały na pustym miejscu.
//! - [`instruments`] — leasing i obligacja. Kredyt ma własny moduł od M5d
//!   ([`crate::credit`]) i M7d dokłada mu wyłącznie trzeci produkt.
//! - [`bankruptcy`] — postępowanie: roszczenia, masa, podział.
//!
//! ## Czego tu nie ma i dlaczego
//!
//! **Nikt tu sam z siebie nie decyduje.** `CorpFinance` wie, jak podpisać leasing,
//! wypuścić obligację i sprzedać należność, ale nie wie, **kiedy** to zrobić —
//! to jest pytanie do tiera taktycznego AI firmy (M7e) albo do gracza (M9).
//! Ten sam podział, który M5 zastosował do `Books::inject_external_capital`:
//! kanał istnieje od fazy, która go zbudowała, pokrywa go test własnościowy,
//! a pierwszy wołający przychodzi później i nie zmienia przy tym podpisu.

pub mod arrears;
pub mod bankruptcy;
pub mod instruments;
pub mod ops;
pub mod params;
pub mod proceedings;
pub mod system;

use std::collections::BTreeMap;

use magnat_core::{CitizenId, DecisionReason, FirmId, HashState, HouseholdId, Money, StateHasher};

use crate::books::AccountOwner;

pub use arrears::{Arrear, ArrearId, Arrears, ClaimOrigin};
pub use bankruptcy::{
    Bankruptcy, BankruptcyId, BankruptcyStage, Claim, ClaimId, ClaimRejected, Distribution, LotFate,
};
pub use instruments::{AssetKind, AssetRef, Bond, BondHolder, BondId, Lease, LeaseId};
pub use params::{
    EstateParams, InsolvencyError, InsolvencyParams, TriggerParams, ValuationParams,
    INSOLVENCY_SCHEMA_VERSION,
};

// ── wypłaty do sektora gospodarstw ───────────────────────────────────────────────

/// Kwota, która wyszła z ksiąg i musi trafić do komponentu mieszkańca.
///
/// Gospodarstwo domowe **nie ma konta w `Books`** (M5b): saldo siedzi w komponencie
/// `Household`, a granica między księgami a komponentami jest kanałem
/// `household_sector_in/out`. Operacja finansowa, która płaci człowiekowi, nie może
/// więc domknąć się sama — zdejmuje kwotę z ksiąg i **oddaje ją listą** temu, kto
/// ma `&mut World`. Ten sam wzorzec, którym M6 oddaje rynkowi fakty do zaksięgowania.
///
/// Niedopisanie tej listy do komponentów znaczy pieniądz zgubiony między księgami
/// a światem — pilnuje tego test `society::total_money + Books::total_balance()`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SectorPayout {
    pub to: CitizenId,
    pub amount: Money,
    pub reason: DecisionReason,
}

// ── rejestr ──────────────────────────────────────────────────────────────────────

/// Finanse firm i postępowania upadłościowe całego miasta.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CorpFinance {
    params: InsolvencyParams,
    arrears: Arrears,
    leases: Vec<Lease>,
    bonds: Vec<Bond>,
    cases: Vec<Bankruptcy>,
    /// Otwarte postępowanie firmy. `BTreeMap`, bo iteracja po nim wyznacza
    /// kolejność kroków postępowań w dobie (00 §3.2).
    open_case: BTreeMap<u32, BankruptcyId>,
    /// Ile dób z rzędu firma ma zaległość powyżej progu. Klucz to indeks encji firmy.
    illiquid_days: BTreeMap<u32, u16>,
}

/// Pusty rejestr z zerowymi parametrami.
///
/// Istnieje **wyłącznie** po to, żeby system mógł wyjąć rejestr ze świata
/// (`std::mem::take`) na czas kroku — dwóch pożyczek `&mut World` naraz nie ma.
/// Ten sam wzorzec, którym `LaborSystem` wyjmuje rynek pracy. Rejestr z zerowymi
/// progami nigdy nie trafia do świata na dłużej niż jeden krok, a gdyby trafił,
/// znaczyłby miasto, w którym nikt nie upada — nie miasto, w którym upada każdy.
impl Default for CorpFinance {
    fn default() -> CorpFinance {
        CorpFinance::new(InsolvencyParams {
            trigger: TriggerParams {
                illiquid_days: u16::MAX,
                illiquid_min_gr: i64::MAX,
                negative_equity_arrears_months: u8::MAX,
            },
            estate: EstateParams {
                trustee_fee_bp: 0,
                rounds: 1,
                round_days: 1,
                round_discount_bp: vec![0],
                scrap_bp: 0,
            },
            valuation: ValuationParams {
                equipment_bp: 0,
                inventory_bp: 0,
            },
        })
    }
}

impl CorpFinance {
    #[must_use]
    pub fn new(params: InsolvencyParams) -> CorpFinance {
        CorpFinance {
            params,
            arrears: Arrears::new(),
            leases: Vec::new(),
            bonds: Vec::new(),
            cases: Vec::new(),
            open_case: BTreeMap::new(),
            illiquid_days: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn params(&self) -> &InsolvencyParams {
        &self.params
    }

    /// Hasz całego rejestru. Osobno od `impl HashState`, bo kolejność sekcji jest
    /// kontraktem determinizmu (00 §3.2) i ma stać obok struktury, której dotyczy,
    /// a nie obok traitu, który ją tylko wywołuje.
    fn hash_all(&self, h: &mut StateHasher) {
        self.arrears.hash_state(h);
        h.write_u64(self.leases.len() as u64);
        for l in &self.leases {
            l.hash_state(h);
        }
        h.write_u64(self.bonds.len() as u64);
        for b in &self.bonds {
            b.hash_state(h);
        }
        h.write_u64(self.cases.len() as u64);
        for c in &self.cases {
            c.hash_state(h);
        }
        h.write_u64(self.illiquid_days.len() as u64);
        for (k, v) in &self.illiquid_days {
            h.write_u32(*k);
            h.write_u16(*v);
        }
    }

    #[must_use]
    pub fn arrears(&self) -> &Arrears {
        &self.arrears
    }

    pub fn arrears_mut(&mut self) -> &mut Arrears {
        &mut self.arrears
    }

    #[must_use]
    pub fn lease(&self, id: LeaseId) -> Option<&Lease> {
        self.leases.get(id.0 as usize)
    }

    #[must_use]
    pub fn bond(&self, id: BondId) -> Option<&Bond> {
        self.bonds.get(id.0 as usize)
    }

    #[must_use]
    pub fn case(&self, id: BankruptcyId) -> Option<&Bankruptcy> {
        self.cases.get(id.0 as usize)
    }

    #[must_use]
    pub fn case_of(&self, firm: FirmId) -> Option<&Bankruptcy> {
        self.open_case
            .get(&firm.entity().index())
            .and_then(|id| self.case(*id))
    }

    /// Czy rzecz należy do leasingodawcy — czyli czy do masy **nie** wchodzi.
    ///
    /// Pyta o własność, a nie o to, czy umowa biegnie: upadłość kończy wszystkie
    /// umowy firmy, a rzecz i tak nie staje się przez to jej majątkiem.
    #[must_use]
    pub fn is_leased(&self, asset: AssetRef) -> bool {
        self.leases
            .iter()
            .any(|l| l.lessor_owns() && l.asset == asset)
    }
}

/// Właściciel konta odpowiadający nabywcy obligacji.
fn owner_of(who: BondHolder) -> AccountOwner {
    match who {
        BondHolder::Firm(f, _) => AccountOwner::Firm(f),
        BondHolder::Household(hh) => AccountOwner::Household(hh),
    }
}

/// Konwersja identyfikatora gospodarstwa na mieszkańca — potrzebna, bo wypłata
/// trafia do komponentu, a komponent nosi mieszkaniec.
#[must_use]
pub fn household_as_citizen(hh: HouseholdId) -> CitizenId {
    CitizenId(hh.entity())
}

/// Finanse firm wchodzą do hasha stanu jako **zasób**, bo są stanem symulacji:
/// kto komu jest winien i na jakim etapie stoi postępowanie zmienia to, co zdarzy
/// się w następnej minucie. Parametry z `data/tuning/insolvency.ron` do hasha
/// **nie wchodzą** — są tym samym dla obu przebiegów tego samego świata, a gdyby
/// nie były, rozjazd i tak wyszedłby na pierwszej zaległości.
impl HashState for CorpFinance {
    fn hash_state(&self, h: &mut StateHasher) {
        self.hash_all(h);
    }
}
