use super::*;

// ── dane (data/demography/demography.ron) ───────────────────────────────────────

/// Pasmo wieku z roczną stopą na 100 000. Zakres jest domknięty obustronnie.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct AgeRate {
    pub from: u8,
    pub to: u8,
    pub per_100k: u32,
}

/// Pasmo wieku z hazardem **miesięcznym** w promilach.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct AgeChance {
    pub from: u8,
    pub to: u8,
    pub permille: u16,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Ages {
    pub adult: u8,
    pub escort: u8,
    pub school_start: u8,
    pub school_end: u8,
    pub work_start: u8,
    pub retirement: u8,
    pub senior: u8,
    pub max: u8,
    pub leave_nest: u8,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct Range8 {
    pub min: u8,
    pub max: u8,
}

/// Wagi funkcji statusu (§5.8). Suma musi być 100 — inaczej status przestaje być
/// skalą 0..=100 i przedziały klas przestają cokolwiek znaczyć.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct StatusWeights {
    pub income: u16,
    pub wealth: u16,
    pub education: u16,
    pub occupation: u16,
    pub address: u16,
    pub consumption: u16,
    pub family: u16,
}

impl StatusWeights {
    #[must_use]
    pub const fn sum(&self) -> u16 {
        self.income
            + self.wealth
            + self.education
            + self.occupation
            + self.address
            + self.consumption
            + self.family
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct MigrationParams {
    pub k_in_permille: u16,
    pub jobless_months_to_leave: u8,
    pub settle_grace_days: u16,
    pub housing_need_threshold: u8,
    pub safety_need_threshold: u8,
    pub need_leave_permille: u16,
    pub jobless_leave_permille: u16,
    pub arrival_sizes: Vec<u8>,
    pub arrival_adult_age_min: u8,
    pub arrival_adult_age_spread: u8,
}

#[derive(Clone, Copy, Debug, Deserialize)]
pub struct SocialParams {
    pub relation_decay_per_week: u8,
    pub coworker_gain_per_week: u8,
    pub coworker_max: u8,
    pub neighbour_gain_per_week: u8,
    pub neighbour_max: u8,
    pub family_weight: u8,
    pub close_contacts: u8,
    pub gossip_targets: u8,
    pub gossip_min_weight: u8,
    pub gossip_score_penalty: u8,
    pub route_score: u8,
}

#[derive(Clone, Debug, Deserialize)]
pub(super) struct DemographyFile {
    schema_version: u32,
    ages: Ages,
    mortality: Vec<AgeRate>,
    mortality_sick_permille: u32,
    fertility: Vec<AgeRate>,
    fertility_parity_permille: Vec<u32>,
    fertility_single_permille: u32,
    fertility_housing_permille: u32,
    fertility_housing_threshold: u8,
    gestation_days: u16,
    illness: Vec<AgeRate>,
    pub(super) illness_health_drop: Range8,
    pub(super) illness_days: Range8,
    illness_hygiene_threshold: u8,
    partnering: Vec<AgeChance>,
    partner_max_status_delta: u8,
    partner_max_age_delta: u8,
    partner_min_relation_weight: u8,
    wedding_min_days: u16,
    wedding_min_weight: u8,
    divorce_base_permille: u16,
    divorce_stress_permille: u16,
    divorce_stress_threshold: u8,
    divorce_debt_permille: u16,
    divorce_debt_to_income: u16,
    divorce_grace_days: u16,
    status_weights: StatusWeights,
    migration: MigrationParams,
    social: SocialParams,
}

/// Tabela hazardów demograficznych — zasób świata, czytany, nigdy zmieniany w biegu.
#[derive(Clone, Debug)]
pub struct DemographyTable {
    pub(super) f: DemographyFile,
}

#[derive(Debug)]
pub enum DemographyError {
    Io(std::io::Error),
    Ron(String),
    Schema { found: u32, want: u32 },
    AgeGap { at: u8, table: &'static str },
    StatusWeights(u16),
}

impl std::fmt::Display for DemographyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DemographyError::Io(e) => write!(f, "data/demography: {e}"),
            DemographyError::Ron(m) => write!(f, "data/demography: {m}"),
            DemographyError::Schema { found, want } => {
                write!(
                    f,
                    "data/demography: schema_version {found}, oczekiwano {want}"
                )
            }
            DemographyError::AgeGap { at, table } => write!(
                f,
                "data/demography: tabela {table} nie pokrywa wieku {at} — hazard byłby \
                 po cichu zerowy"
            ),
            DemographyError::StatusWeights(s) => write!(
                f,
                "data/demography: wagi statusu sumują się do {s}, a mają do 100"
            ),
        }
    }
}

impl std::error::Error for DemographyError {}

impl From<std::io::Error> for DemographyError {
    fn from(e: std::io::Error) -> Self {
        DemographyError::Io(e)
    }
}

impl DemographyTable {
    /// Wczytanie i **walidacja przed użyciem** (00 §5).
    ///
    /// Dziura w pokryciu wieku jest błędem ładowania, nie stanem do obsłużenia:
    /// hazard zerowy dla pasma 45–54 wygląda w symulacji jak dekada nieśmiertelności
    /// i wychodzi dopiero po dziesięciu latach gry.
    pub fn load(path: &Path) -> Result<DemographyTable, DemographyError> {
        let txt = std::fs::read_to_string(path)?;
        let f: DemographyFile =
            ron::from_str(&txt).map_err(|e| DemographyError::Ron(e.to_string()))?;
        if f.schema_version != DEMOGRAPHY_SCHEMA_VERSION {
            return Err(DemographyError::Schema {
                found: f.schema_version,
                want: DEMOGRAPHY_SCHEMA_VERSION,
            });
        }
        if f.status_weights.sum() != 100 {
            return Err(DemographyError::StatusWeights(f.status_weights.sum()));
        }
        pokrycie(&f.mortality, 0, f.ages.max, "mortality")?;
        pokrycie(&f.illness, 0, f.ages.max, "illness")?;
        Ok(DemographyTable { f })
    }

    pub fn load_default() -> Result<DemographyTable, DemographyError> {
        DemographyTable::load(&magnat_core::data_path("demography/demography.ron"))
    }

    #[inline]
    #[must_use]
    pub fn ages(&self) -> Ages {
        self.f.ages
    }

    #[inline]
    #[must_use]
    pub fn status_weights(&self) -> StatusWeights {
        self.f.status_weights
    }

    #[inline]
    #[must_use]
    pub fn migration(&self) -> &MigrationParams {
        &self.f.migration
    }

    #[inline]
    #[must_use]
    pub fn social(&self) -> SocialParams {
        self.f.social
    }

    #[inline]
    #[must_use]
    pub fn gestation_days(&self) -> u16 {
        self.f.gestation_days
    }

    /// Roczny hazard zgonu na 100 000, po korekcie o zdrowie.
    #[must_use]
    pub fn mortality_per_100k(&self, age_years: i32, health: Q) -> u32 {
        let baza = stopa(&self.f.mortality, age_years);
        // Zdrowie 100 → ×1,0, zdrowie 0 → ×`mortality_sick_permille`/1000. Liniowo.
        let mnoznik = 1000
            + (self.f.mortality_sick_permille.saturating_sub(1000))
                * u32::from(100 - health.get().min(100))
                / 100;
        (u64::from(baza) * u64::from(mnoznik) / 1000).min(100_000) as u32
    }

    /// Roczny hazard zachorowania na 100 000.
    #[must_use]
    pub fn illness_per_100k(&self, age_years: i32, hygiene: Q) -> u32 {
        let baza = stopa(&self.f.illness, age_years);
        if hygiene.get() < self.f.illness_hygiene_threshold {
            (baza + baza / 2).min(100_000)
        } else {
            baza.min(100_000)
        }
    }

    /// Roczny hazard urodzenia dziecka na 100 000 kobiet, po korektach (§5.6).
    #[must_use]
    pub fn fertility_per_100k(
        &self,
        age_years: i32,
        children: usize,
        partnered: bool,
        housing: Q,
    ) -> u32 {
        let mut v = u64::from(stopa(&self.f.fertility, age_years));
        if v == 0 {
            return 0;
        }
        let parity = self
            .f
            .fertility_parity_permille
            .get(children)
            .or_else(|| self.f.fertility_parity_permille.last())
            .copied()
            .unwrap_or(1000);
        v = v * u64::from(parity) / 1000;
        if !partnered {
            v = v * u64::from(self.f.fertility_single_permille) / 1000;
        }
        if housing.get() < self.f.fertility_housing_threshold {
            v = v * u64::from(self.f.fertility_housing_permille) / 1000;
        }
        v.min(100_000) as u32
    }

    /// Miesięczny hazard wejścia w związek, w promilach.
    #[must_use]
    pub fn partnering_permille(&self, age_years: i32) -> u16 {
        for b in &self.f.partnering {
            if age_years >= i32::from(b.from) && age_years <= i32::from(b.to) {
                return b.permille;
            }
        }
        0
    }

    /// Miesięczny hazard rozstania, w promilach (§5.6).
    #[must_use]
    pub fn divorce_permille(&self, stress: Q, debt: Money, income_monthly: Money) -> u16 {
        let mut p = self.f.divorce_base_permille;
        if stress.get() >= self.f.divorce_stress_threshold {
            p += self.f.divorce_stress_permille;
        }
        let prog = income_monthly
            .checked_mul_int(i64::from(self.f.divorce_debt_to_income))
            .unwrap_or(Money(i64::MAX));
        if income_monthly.get() > 0 && debt.get() > prog.get() {
            p += self.f.divorce_debt_permille;
        }
        p
    }

    #[must_use]
    pub fn partner_limits(&self) -> (u8, u8, u8) {
        (
            self.f.partner_max_status_delta,
            self.f.partner_max_age_delta,
            self.f.partner_min_relation_weight,
        )
    }

    #[must_use]
    pub fn wedding_rules(&self) -> (u16, u8) {
        (self.f.wedding_min_days, self.f.wedding_min_weight)
    }

    #[must_use]
    pub fn divorce_grace_days(&self) -> u16 {
        self.f.divorce_grace_days
    }
}

fn pokrycie(t: &[AgeRate], from: u8, to: u8, name: &'static str) -> Result<(), DemographyError> {
    for wiek in from..=to {
        if !t.iter().any(|b| wiek >= b.from && wiek <= b.to) {
            return Err(DemographyError::AgeGap {
                at: wiek,
                table: name,
            });
        }
    }
    Ok(())
}

fn stopa(t: &[AgeRate], age_years: i32) -> u32 {
    if age_years < 0 {
        return 0;
    }
    let a = age_years.min(255) as u8;
    for b in t {
        if a >= b.from && a <= b.to {
            return b.per_100k;
        }
    }
    0
}
