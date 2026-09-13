# M7a — Firma jako dane

Podfaza 1 z 6 fazy **M7 — Firmy AI i rynek pracy** (`M7-firmy-ai-i-rynek-pracy.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | Szkic `sim/firms` z M5/M6, M3 (mieszkańcy), M6 (produkcja). |
| **Pakiety robocze** | WP1, WP2, WP3 |
| **Projekt techniczny** | §5.1, §5.2, §5.3 |
| **Wynik do pokazania** | 10 000 firm w świecie, każda z przypisanymi trzema slotami decyzyjnymi; dodanie typu zakładu nie dotyka kodu. |
| **Kryterium zamknięcia** | Kryteria WP1–WP3; dwa przebiegi dają identyczną sekwencję `(tick, FirmKey, tier)`. |
| **Poprzednia / następna** | — (pierwsza w fazie) · `M7b-rynek-pracy.md` |

Model firmy i zakładu, stabilny `FirmKey`, scheduler trzech poziomów decyzji, katalog typów zakładów w danych oraz stanowiska, zatrudnienie i lista płac.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| WP1 | Model firmy, `FirmKey`, scheduler decyzji i budżet shardingu | szkic `sim/firms` z M5/M6 | M |
| WP2 | Katalog typów zakładów w `data/site_types/` + walidator | WP1 | S |
| WP3 | Stanowiska, zatrudnienie, lista płac, produktywność | WP1, WP2, M3, M6 | M |

### WP1 — Model firmy, `FirmKey`, scheduler decyzji

Komponenty ECS firmy i zakładu, stabilny klucz `FirmKey(u64)` (monotoniczny licznik świata,
zapisywany w save; **nie** indeks encji — indeksy bywają recyklingowane, a klucz steruje
shardingiem i strumieniem RNG), `DecisionLog` jako pierścień 32 wpisów.
Scheduler: przydział firmy do slotu decyzyjnego każdego z trzech poziomów (§5.6 tego dokumentu).

*Kryterium ukończenia:* 10 000 firm w świecie, każda ma przypisane trzy sloty; test determinizmu —
dwa przebiegi dają identyczną sekwencję `(tick, FirmKey, tier)`; hash stanu ECS obejmuje wszystkie
komponenty M7.

### WP2 — Katalog typów zakładów w danych

`data/site_types/*.ron` — po jednym pliku na branżę (`extraction.ron`, `processing.ron`,
`manufacturing.ron`, `logistics.ron`, `retail.ron`, `consumer_services.ron`,
`business_services.ron`, `finance.ron`, `media.ron`, `real_estate.ron`).
**Dodanie nowego typu zakładu nie dotyka kodu** — to jest kryterium.

*Kryterium ukończenia:* walidator w CI — każdy `SiteType` ma domknięty zestaw `JobRoleId`
(każda rola istnieje w `data/jobs/`), każda receptura wskazuje istniejące `GoodId`, każdy typ ma
co najmniej jedno stanowisko menedżerskie; dodanie nowego pliku RON nie wymaga rekompilacji poza
ładowaniem danych.

### WP3 — Stanowiska, zatrudnienie, lista płac, produktywność

`Position`, `Employment`, `Payroll`. Produktywność jako funkcja czysta w milijednostkach
(`i32`, nie float — wynik wpływa na stan trwały). Wypłata miesięczna z shardingiem po dniach.

*Kryterium ukończenia:* zakład z załogą wytwarza w M6 wynik proporcjonalny do `effective_labor`;
test LOD — przebieg mikro i mezo tego samego zakładu daje identyczną sumę wypłat (tolerancja 0).

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.1 Struktura crate'a

```
sim/firms/
  src/
    lib.rs
    key.rs            // FirmKey, sharding, sloty decyzyjne
    firm.rs           // Firm, Ownership, FirmBooks, DecisionLog
    site.rs           // Site, SiteTypeId, SiteDelegation
    catalog.rs        // ładowanie data/site_types/
    hr/
      position.rs     // Position, JobRoleId <- data/jobs/
      employment.rs   // Employment, Payroll, Severance
      productivity.rs // effective_labor()
      manager.rs      // Manager, management_quality()
      training.rs     // szkolenia, premie, benefity
      turnover.rs     // rotacja
    labor_policy.rs   // scoring kandydata i eskalacja stawek — REGUŁY firmy;
                      // sam rynek pracy mieszka w sim/economy::labor (D2)
    finance/
      loan.rs  lease.rs  factoring.rs  bond.rs
      liquidity.rs    // kolejka płatności, wykrycie niewypłacalności
      bankruptcy.rs   // syndyk, masa, zaspokojenie
    view/
      firm_view.rs    // FirmView — JEDYNE wejście danych do ai/
      board.rs        // PublicMarketBoard, RumorFeed
    ai/
      personality.rs  // FirmPersonality, FirmStrategy
      policy.rs       // FirmPolicy = zestaw reguł sim/policy + presety per strategia
      operational.rs  // tier 1
      tactical.rs     // tier 2
      strategic.rs    // tier 3 (+ macro)
      reaction.rs     // reakcja na gracza
    lifecycle/
      founding.rs  closure.rs  external_chain.rs
    reason.rs         // DecisionReason (wariant Firm), Decided<T>
    systems.rs        // rejestracja systemów i częstotliwości
sim/macro/            // WŁAŚCICIEL: M10. M7 buduje podzbiór wg M10-glebia.md §6:
  src/                //   MacroState/MacroCell/MacroFirm/MacroStock, lift(), step() fazy 2-6,
                      //   what_if(). NIE: lower(), dry_run(), fast_forward(), MacroLodPolicy

sim/policy/           // K-11: JEDEN silnik reguł dla AI i gracza. Język projektuje M9.
  src/
    ast.rs            // re-eksport/odwzorowanie AST M9: ConditionExpr, Expr, Metric,
                      // Action, PolicyScope — NIE własny język
    eval.rs           // ewaluator: arytmetyka całkowita, bez alokacji na ścieżce gorącej
    metrics.rs        // rejestr Metric -> odczyt WYŁĄCZNIE przez FirmView
    actions.rs        // rejestr Action -> OpsAction / TacAction
    scope.rs          // PolicyScope: firma / zakład / sklep / produkt + konflikty

sim/economy/          // WŁAŚCICIEL: M5. M7 dokłada moduł labor (D2):
  src/labor/
    offer.rs          // JobOffer, Application — na maszynerii ofert M5
    matching.rs       // wybór kandydata na indeksie przestrzennym M5
    bidding.rs        // eskalacja stawek, headhunting
    stats.rs          // LaborMarketStats, ShortageIndex
```

### 5.2 Firma, zakład, własność (§7.1)

```rust
/// Stabilny klucz firmy: monotoniczny licznik świata, trwały w zapisie gry.
/// Steruje shardingiem decyzji i strumieniem RNG. NIE jest indeksem ECS.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FirmKey(pub u64);

pub struct Firm {
    pub key: FirmKey,
    pub name: NameId,                 // z data/names/ + gramatyki
    pub founded: SimMinute,
    pub director: Option<CitizenId>,  // None => firma zewnętrzna albo zarząd tymczasowy
    pub owners: SmallVec<[OwnerShare; 4]>,
    pub hq_district: DistrictId,
    pub sites: SmallVec<[SiteId; 8]>,
    pub products: SmallVec<[GoodId; 16]>,   // portfel produktów
    pub contracts: Vec<ContractId>,         // B2B z M6, sortowane po ContractId
    pub personality: FirmPersonality,
    pub strategy: FirmStrategy,
    pub policy: FirmPolicy,
    pub status: FirmStatus,           // Active | Restructuring | Bankrupt(..) | Closed
    pub log: RingBuf<LoggedDecision, 32>,   // historia decyzji (§5.11)
}

pub struct OwnerShare { pub owner: Owner, pub bp: u16 }   // bp: 1/10000, suma == 10_000
pub enum Owner { Citizen(CitizenId), Firm(FirmId), Player, City, External(ChainId) }

/// Pieniądz firmy. Pełny RZiS/bilans/CF per zakład — kontrakt z M5 (§6.9).
pub struct FirmBooks {
    pub cash: Money,
    pub receivables: Vec<Receivable>,  // sortowane po (due, ContractId)
    pub payables:    Vec<Payable>,
    pub loans:  SmallVec<[Loan; 4]>,
    pub leases: SmallVec<[Lease; 4]>,
    pub bonds:  SmallVec<[Bond; 2]>,
    pub equity_history: RingBuf<MonthlyEquity, 60>,  // 5 lat — trend i wycena
}

pub struct Site {
    pub firm: FirmId,
    pub site_type: SiteTypeId,          // indeks do data/site_types/
    pub building: BuildingId,           // fizyka, maszyny, magazyn — M6
    pub district: DistrictId,
    pub positions: Vec<Position>,       // sortowane po JobRoleId
    pub manager: Option<Entity>,        // Manager
    pub delegation: Option<SiteDelegation>,
    pub shift_plan: ShiftPlan,          // 1–3 zmiany (§7.4)
    pub fixed_cost_month: Money,        // czynsz, amortyzacja, media bazowe
    pub pnl: RingBuf<SitePnlMonth, 36>, // wejście decyzji taktycznej
    pub opened: SimMinute,
}
```

**Katalog typów zakładów (§7.2) — struktura danych, nie lista w kodzie.**
`data/site_types/retail.ron`, fragment ilustracyjny:

```ron
SiteType(
    key: "supermarket",
    schema_version: 1,
    category: Retail,
    footprint_m2: (min: 800, max: 3500),
    lines: None,                                  // handel nie ma linii produkcyjnych
    shelf_capacity_m3_per_100m2: 45,
    staffing: [
        ( role: "cashier",      per_100m2: 0.9, min: 2 ),
        ( role: "stocker",      per_100m2: 0.5, min: 1 ),
        ( role: "shop_manager", per_site: 1,    managerial: true ),
    ],
    utilities: ( power_wh_per_m2_day: 380, water_ml_per_m2_day: 120, heat: Required ),
    dock_trucks_per_hour: 4,
    emissions: ( noise: 15, air: 0, water: 0 ),
    capex: ( build: 1_850_000_00, equip_per_100m2: 42_000_00 ),
    permitted_zones: [ Commercial, MixedUse ],
    epoch_from: "1960",
)
```

Cała lista z §7.2 (wydobycie, przetwórstwo pierwotne, produkcja, logistyka, handel detaliczny,
usługi dla ludności, usługi dla biznesu, finanse, media, nieruchomości) to **treść danych**,
nie kodu. Kod zna wyłącznie `SiteTypeCategory` i pola powyżej. Nowy typ zakładu = nowy rekord RON
+ role w `data/jobs/` + receptury w `data/recipes/` (M6). Walidator CI sprawdza domknięcie grafu.

### 5.3 Stanowiska, zatrudnienie, produktywność (§7.5, §6.6)

```rust
pub struct Position {
    pub role: JobRoleId,                 // data/jobs/: profil umiejętności, wykształcenie
    pub slots: u16,
    pub filled: SmallVec<[Employment; 8]>,
    pub managerial: bool,
    pub wage_band: (Money, Money),       // widełki z polityki firmy, miesięcznie
}

pub struct Employment {
    pub citizen: CitizenId,
    pub firm: FirmId,
    pub site: SiteId,
    pub role: JobRoleId,
    pub wage_month: Money,               // brutto; potrącenia = hook M8
    pub since: SimMinute,
    pub shift: ShiftId,
    pub benefits: BenefitSet,            // bitflagi: CompanyCar | Health | Meals | Training
    pub bonus_policy: BonusPolicy,
    pub perf_ema: u16,                   // wygładzona ocena 0..=1000
    pub warnings: u8,
}

/// Produktywność (§6.6). Wszystko w milijednostkach i32 — wynik wpływa na stan trwały,
/// więc żadnych f32 (dokument 00 §2).
pub fn effective_labor(
    emp:   &Employment,
    body:  &CitizenVitals,      // energia, nastrój, zdrowie — z sim/agents (M3)
    skill: Q,                   // umiejętność w zawodzie
    tech:  TechLevel,           // poziom wyposażenia zakładu (M6)
    mgmt:  ManagementQuality,   // jakość zarządzania zakładem
    w:     &RoleWeights,        // wagi z data/jobs/<role>.ron
) -> Qty;
```

Formuła (milijednostki, składane w ustalonej kolejności):

```
base = w.skill*skill + w.energy*energy + w.mood*mood01 + w.health*health   // Σw = 1000
out  = base * tech_mult(tech) / 1000 * mgmt_mult(mgmt) / 1000
       // tech_mult: 800..1400,  mgmt_mult: 850..1150
```

`mood01` to `Mood(-100..=100)` przeskalowany jawnie do 0..100 — żeby zły nastrój nie wytwarzał
ujemnej pracy. Wagi są per zawód: dla `researcher` liczy się umiejętność, dla `truck_driver`
energia i zdrowie. Zmiana wag = zmiana danych, nie kodu.
