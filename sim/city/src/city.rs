//! Zasób świata `City` — wszystko, co miasto wie o swoich pieniądzach (M8a).
//!
//! Zwykły zasób ECS, a nie encja-singleton z komponentami: decyzja otwarta `D4`
//! fazy uzasadniała encję tym, żeby miasto „mieściło się w haszu stanu i snapshotcie
//! bez wyjątków" — a `World::register_resource_hash` (`K-29`) daje dokładnie to
//! samo i robi to już dla `Books`, `Market`, `Firms` i `CorpFinance`. Encja
//! z jednym egzemplarzem kosztowałaby archetyp, bufor komend i pytanie „którą",
//! nie kupując niczego.

use std::collections::BTreeMap;
use std::sync::Arc;

use magnat_core::{HashState, Money, SiteId, StateHasher, Tick};
use magnat_economy::AccountId;

use crate::budget::{BudgetPolicy, CityBudget};
use crate::charge::ChargeRegistry;
use crate::code::{TaxCode, VatTable};
use crate::engine::{CityTaxEngine, Withholding};
use crate::law::Enforcement;
use crate::permits::PermitRegistry;
use crate::services::{DistrictPopulation, PublicServices};

/// Wpis katastru: zakład i wartość katastralna parceli, na której stoi.
///
/// **Zamrożona na 1 stycznia roku podatkowego** (`D12`). Wycena gruntu zmienia się
/// w ciągu roku razem z dostępnością i sąsiedztwem; podatek liczony z bieżącej
/// wyceny znaczyłby, że rata rośnie w środku roku bez żadnej uchwały, a gracz
/// nie ma jak tego przewidzieć.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CadastreEntry {
    pub site: SiteId,
    pub value: Money,
}

/// Stan fiskalny miasta.
pub struct City {
    pub code: Arc<TaxCode>,
    pub rates: Arc<VatTable>,
    pub policy: BudgetPolicy,
    pub budget: CityBudget,
    pub charges: ChargeRegistry,
    pub withholding: Withholding,
    /// Podstawa podatku od nieruchomości, po `SiteId` rosnąco.
    pub cadastre: Vec<CadastreEntry>,
    /// Opłata koncesyjna po rodzaju zakładu (`SiteTypeId` jako liczba).
    pub licenses: BTreeMap<u16, Money>,
    /// Strata do rozliczenia w CIT, per zakład. Wpis znika, gdy zostanie zjedzona.
    pub loss_carry: BTreeMap<SiteId, Money>,
    /// Wpływy bieżącego miesiąca — podstawa okna dwunastu miesięcy w budżecie.
    pub month_revenue: Money,
    /// Ostatni rok, za który naliczono CIT i koncesje. Bez tego dwa domknięcia
    /// w tym samym styczniu naliczyłyby daninę dwa razy.
    pub last_year_assessed: u16,
    // ── M8d ──
    /// Kalibracja usług, urzędów i egzekucji (`data/tuning/city.ron`).
    pub tuning: Arc<TuningRef>,
    /// Placówki publiczne i pokrycie obwodowe (WP7).
    pub services: PublicServices,
    /// Wnioski o pozwolenie i urzędy, które je przerabiają (WP7).
    pub permits: PermitRegistry,
    /// Urzędy kontrolne i sprawy (WP8).
    pub enforcement: Enforcement,
    /// Emisja pyłu zakładów, odświeżana raz na dobę przez system miasta.
    ///
    /// Migawka, a nie źródło: pył liczy `sim/supply`, a miasto tylko na niego
    /// patrzy. Kopia istnieje, bo krok urzędów nie ma jak trzymać `ChainHandle`
    /// naraz z `&mut City` — zasób jest na czas kroku wyjęty ze świata.
    pub emissions: Vec<(SiteId, magnat_core::FirmId, i64)>,
    /// Ludność per dzielnica — wejście miesięcznego przeliczenia jakości.
    pub district_population: DistrictPopulation,
}

/// Kalibracja M8d albo jej brak.
///
/// `Option` w środku, a nie `Option<Arc<…>>` na zewnątrz, żeby `City::default()`
/// (świat sprzed M8d) nie musiał czytać pliku, a kod usług miał jedno pytanie
/// zamiast dwóch. Brak kalibracji znaczy „miasto nie prowadzi usług" i jest
/// poprawnym stanem scenariusza sprzed tej podfazy.
#[derive(Debug, Default)]
pub struct TuningRef(pub Option<crate::tuning::CityTuning>);

impl Default for City {
    /// Miasto bez kodeksu i bez konta — stan, w którym nic się nie nalicza.
    ///
    /// Istnieje wyłącznie po to, żeby system wyłączny mógł wyjąć zasób ze świata
    /// na czas kroku (`std::mem::take`), tak samo jak robi to `economy.Market`
    /// z finansami firm. Nigdy nie jest stanem, w którym miasto realnie działa.
    fn default() -> City {
        City::new(
            Arc::new(TaxCode::empty()),
            Arc::new(VatTable::default()),
            BudgetPolicy::default(),
            AccountId(u32::MAX),
        )
    }
}

impl City {
    #[must_use]
    pub fn new(
        code: Arc<TaxCode>,
        rates: Arc<VatTable>,
        policy: BudgetPolicy,
        account: AccountId,
    ) -> City {
        City {
            code,
            rates,
            policy,
            budget: CityBudget::new(account),
            charges: ChargeRegistry::new(),
            withholding: Withholding::new(),
            cadastre: Vec::new(),
            licenses: BTreeMap::new(),
            loss_carry: BTreeMap::new(),
            month_revenue: Money::ZERO,
            last_year_assessed: u16::MAX,
            tuning: Arc::new(TuningRef(None)),
            services: PublicServices::default(),
            permits: PermitRegistry::default(),
            enforcement: Enforcement::default(),
            emissions: Vec::new(),
            district_population: DistrictPopulation::default(),
        }
    }

    /// Silnik podatkowy do wstawienia w rynek (`Market::set_tax_engine`).
    #[must_use]
    pub fn tax_engine(&self) -> CityTaxEngine {
        CityTaxEngine::new(
            Arc::clone(&self.rates),
            Arc::clone(&self.code),
            self.withholding.clone(),
        )
    }

    /// Czy miasto ma kodeks i konto. Miasto bez konta nie nalicza — i to nie jest
    /// awaria, tylko scenariusz sprzed M8.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.budget.account != AccountId(u32::MAX)
    }

    /// Wpis wpływu: budżet i licznik miesiąca naraz, żeby nie dało się podnieść
    /// jednego bez drugiego.
    pub fn credit_revenue(&mut self, kind: magnat_core::TaxKind, amount: Money) {
        self.budget.take_revenue(kind, amount);
        self.month_revenue = Money(self.month_revenue.get() + amount.get());
    }

    /// Strata zakładu do rozliczenia w latach następnych.
    #[must_use]
    pub fn loss_of(&self, site: SiteId) -> Money {
        self.loss_carry.get(&site).copied().unwrap_or(Money::ZERO)
    }

    pub fn set_loss(&mut self, site: SiteId, loss: Money) {
        if loss.get() <= 0 {
            self.loss_carry.remove(&site);
        } else {
            self.loss_carry.insert(site, loss);
        }
    }

    /// Obciążenia jednego zakładu — treść karty inspekcji firmy (kryterium WP2).
    #[must_use]
    pub fn charges_of_site(&self, site: SiteId) -> Vec<crate::charge::TaxCharge> {
        self.charges
            .of_payer(crate::charge::TaxPayer::Site(site))
            .copied()
            .collect()
    }
}

impl HashState for City {
    /// Kodeks, stawki i polityka **nie wchodzą**: to są dane wczytane z `data/`,
    /// takie same w obu przebiegach tego samego świata. Wchodzi wszystko, co się
    /// w trakcie zmienia — budżet, należności, zaliczki, straty i kataster.
    fn hash_state(&self, h: &mut StateHasher) {
        self.budget.hash_state(h);
        self.charges.hash_state(h);
        self.withholding.hash_state(h);
        h.write_u64(self.cadastre.len() as u64);
        for c in &self.cadastre {
            h.write_u64(c.site.0.to_bits());
            h.write_i64(c.value.get());
        }
        h.write_u64(self.loss_carry.len() as u64);
        for (s, m) in &self.loss_carry {
            h.write_u64(s.0.to_bits());
            h.write_i64(m.get());
        }
        h.write_i64(self.month_revenue.get());
        h.write_u16(self.last_year_assessed);
        // M8d: jakość placówek, kolejka urzędu i sprawy zmieniają się w trakcie
        // i zmieniają wynik gry, więc są stanem. Kalibracja i migawka emisji —
        // nie: pierwsza jest daną z `data/`, druga kopią cudzego pola.
        self.services.hash_state(h);
        self.permits.hash_state(h);
        self.enforcement.hash_state(h);
    }
}

/// Tick pierwszej minuty doby `day` (licząc od zera świata).
#[must_use]
pub fn tick_of_day(day: u64) -> Tick {
    Tick(day * 1_440)
}

/// Termin płatności: dzień `day_of_month` miesiąca, w którym wypada `from`,
/// albo miesiąca następnego, jeśli ten dzień już minął.
#[must_use]
pub fn due_on_day(from: Tick, day_of_month: u8) -> Tick {
    let cal = magnat_core::SimCalendar::new(from);
    let dzien_absolutny = cal.day_index();
    let dzien_miesiaca = u64::from(cal.day_of_month());
    let cel = u64::from(day_of_month.clamp(1, 30));
    let poczatek_miesiaca = dzien_absolutny - (dzien_miesiaca - 1);
    let w_tym = poczatek_miesiaca + cel - 1;
    if w_tym >= dzien_absolutny {
        tick_of_day(w_tym)
    } else {
        tick_of_day(poczatek_miesiaca + 30 + cel - 1)
    }
}
