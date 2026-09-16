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
| [x] WP1 | Model firmy, `FirmKey`, scheduler decyzji i budżet shardingu | szkic `sim/firms` z M5/M6 | M |
| [x] WP2 | Katalog typów zakładów w `data/site_types/` + walidator | WP1 | S |
| [x] WP3 | Stanowiska, zatrudnienie, lista płac, produktywność | WP1, WP2, M3, M6 | M |

### WP1 — Model firmy, `FirmKey`, scheduler decyzji

Komponenty ECS firmy i zakładu, stabilny klucz `FirmKey(u64)` (monotoniczny licznik świata,
zapisywany w save; **nie** indeks encji — indeksy bywają recyklingowane, a klucz steruje
shardingiem i strumieniem RNG), `DecisionLog` jako pierścień 32 wpisów.
Scheduler: przydział firmy do slotu decyzyjnego każdego z trzech poziomów (§5.6 tego dokumentu).

*Kryterium ukończenia:* 10 000 firm w świecie, każda ma przypisane trzy sloty; test determinizmu —
dwa przebiegi dają identyczną sekwencję `(tick, FirmKey, tier)`; hash stanu ECS obejmuje wszystkie
komponenty M7.

**Spełnione** (`sim/firms/tests/scheduling.rs`, `state_hash.rs`): 10 000 firm, każda z trzema
slotami w kalendarzu 12 × 30, dwa przebiegi dają identyczną sekwencję, każda firma decyduje
operacyjnie **dokładnie raz** na dobę, a kolejność zakładania firm nie zmienia hasha. Stan
wchodzi do hasha świata przez zasób `Firms` (`AR-8`), razem z kolejką przepełnienia slotów;
kubełki indeksu pochodnego do hasha nie wchodzą i ma to własny test.

### WP2 — Katalog typów zakładów w danych

`data/site_types/*.ron` — po jednym pliku na branżę (`extraction.ron`, `processing.ron`,
`manufacturing.ron`, `logistics.ron`, `retail.ron`, `consumer_services.ron`,
`business_services.ron`, `finance.ron`, `media.ron`, `real_estate.ron`).
**Dodanie nowego typu zakładu nie dotyka kodu** — to jest kryterium.

*Kryterium ukończenia:* walidator w CI — każdy `SiteType` ma domknięty zestaw `JobRoleId`
(każda rola istnieje w `data/jobs/`), każda receptura wskazuje istniejące `GoodId`, każdy typ ma
co najmniej jedno stanowisko menedżerskie; **zbiory kluczy `data/site_types/` i `data/buildings/`
pokrywają się** (`AR-1`); dodanie nowego pliku RON nie wymaga rekompilacji poza ładowaniem danych.

**Spełnione** (`sim/firms/tests/catalog.rs` — dziewięć asercji na prawdziwych danych,
`tools/headless/tests/site_types.rs` — trzy reguły krzyżowe): 96 typów zakładów w dziewięciu
plikach branżowych, 96 z 96 archetypów firmowych pokrytych, zero dziur w obie strony. Każda
z czterech reguł ma test negatywny, czyli test tego, że walidator faktycznie odrzuca.

### WP3 — Stanowiska, zatrudnienie, lista płac, produktywność

`Position`, `Employment`, `Payroll`. Produktywność jako funkcja czysta w milijednostkach
(`i32`, nie float — wynik wpływa na stan trwały). Wypłata miesięczna z shardingiem po dniach.

*Kryterium ukończenia:* zakład z załogą wytwarza w M6 wynik proporcjonalny do `effective_labor`;
test LOD — przebieg mikro i mezo tego samego zakładu daje identyczną sumę wypłat (tolerancja 0).

**Spełnione, każde zdanie osobno.** Pierwsze: `sim/supply/tests/plant.rs`
(`obsada_zakladu_jest_drugim_ogranicznikiem_szarzy`) — ten sam młyn, ta sama doba, ten sam wsad,
pełna obsada miele pełną produkcję, połowa obsady około połowy, zakład bez ludzi stoi.
Wymagało to dołożenia `PlantSite::labor_pct` po stronie M6 (`AR-10`, `K-44`), bo do M7a praca
nie była wejściem produkcji. Drugie: `sim/firms/tests/payroll_and_labor.rs`
(`suma_wyplat_nie_zalezy_od_gestosci_liczenia_czasu`) — miesiąc liczony dobami i minutami daje
identyczną sumę wypłat, tolerancja 0.

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

---

## Zmiany wpisane po M7a

Korekty naniesione w trakcie wykonania tej podfazy. Gwiazdka = zmiana zakresu
albo kryterium. Format jak w tabelach korekt pozostałych faz (`K-18` pkt b).

| # | Co | Dlaczego |
|---|---|---|
| `AR-1`* | **Kryterium WP2 dostaje czwartą regułę: zbiory kluczy `data/site_types/` i `data/buildings/` muszą się pokrywać.** Egzekwuje ją `SiteTypeCatalog::load` (`CatalogError::NoArchetype`) w jedną stronę, a test `tools/headless/tests/site_types.rs` w drugą | Katalog typów zakładów powstał jako osobny plik obok istniejącego katalogu archetypów M2 (decyzja właściciela produktu przy starcie podfazy), a oba opisują **ten sam** zakład tym samym kluczem tekstowym. To jest jedyny realny koszt tego podziału i bez walidatora byłby cichy: archetyp zmienia nazwę, zakład traci typ, obsada wychodzi pusta, a objawem jest zakład bez ani jednego pracownika zauważony kiedyś w panelu |
| `AR-2`* | **`SiteType` przycięty do pól, które nie mają właściciela gdzie indziej.** Nie powstają: `footprint_m2` i `permitted_zones` (są w archetypie M2 jako `min_parcel_m2` i `zones`), `lines` (wyprowadzane z receptur, M6b), `utilities` (liczniki `UtilityMeter`, M6b), `dock_trucks_per_hour` (`Dock`, M6b), `emissions` (`PlantSite::emissions`, M6b), `shelf_capacity_m3_per_100m2` (`Shelf`, M5b), `epoch_from` (pole `epochs` w archetypie). Dochodzi `fixed_cost` — czynsz za m² i ryczałt administracyjny | Każde z tych pól ma już pisarza i czytelnika w innej fazie. Powtórzenie któregokolwiek dałoby drugą prawdę o tej samej liczbie — dokładnie to, przed czym broni `AR-1`, tylko wewnątrz jednego rekordu |
| `AR-3`* | **`staffing.per_100m2: f32` zastąpione przez `per_10000m2: u16`.** Wiersz obsady liczy się w całkowitych stanowiskach na 10 000 m² | Dwa powody, każdy osobno wystarczający. **(1)** Liczba stanowisk wchodzi do stanu trwałego, a tam floata nie ma (00 §2). **(2)** Mianownik 1 000 był za gruby: zakłady w `data/buildings/` mają od 22 m² na stanowisko (biuro) do 1 920 m² (las, pole), więc gospodarstwo rolne wymagałoby połowy stanowiska i w `u16` wychodziłoby 1 — cała gałąź rolna miasta byłaby **dwukrotnie** przeobsadzona, razem z dwukrotnie zawyżonym kosztem pracy. Przy 10 000 biuro ma 4 500, pole 5, i obie liczby są dokładne |
| `AR-4`* | **`tech_mult` przesunięty z 800..1400 na 700..1300**, przy zachowanej rozpiętości 600. Punkt neutralny wypada przy `Q(50)` | Znalazł to test, który miał sprawdzić rzecz oczywistą: że wzorcowy pracownik daje pełny etat. `Q::new(50)` jest wartością domyślną `PlantSite::tech` (M6) i `Site::tech`, czyli znaczy „wyposażenie przeciętne" — a w skali 800..1400 przeciętne wyposażenie dawało **ciche +10%** do przepustowości każdego zakładu w mieście. Mnożnik bez punktu neutralnego nie jest mnożnikiem, tylko przesunięciem skali ukrytym w kalibracji |
| `AR-5` | **`FirmBooks` nie powstaje w M7a.** `Firm` nie ma pola `cash` ani historii kapitału | Konto firmy prowadzi `Books` z M5 pod `AccountOwner::Firm(FirmId)` i to jest jedyne saldo; druga kopia gotówki byłaby drugą prawdą. Kredyty, leasingi, obligacje i należności to §5.12, czyli **M7d** — w M7a byłyby pustą strukturą |
| `AR-6` | **`Firm` bez pól `contracts`, `personality`, `strategy` i `policy`** wymienionych w §5.2 | Każde z nich ma właściciela w dalszej podfazie (`policy` — M7c, `personality` i `strategy` — M7e, `contracts` — M6 i M7d). Pole bez pisarza jest kosztem razy dziesięć tysięcy firm i zerem wartości; `Firm` dostanie je wtedy, gdy ktoś zacznie je zapisywać |
| `AR-7` | **`Site` bez `shift_plan` i bez `manager`; `SitePnlMonth` ma dwie pozycje, a nie pełny rachunek wyniku** | Harmonogram zmian już istnieje jako `PlantSite::schedule` (M6b) i drugi byłby sprzecznością, nie nadmiarem. Menedżer należy do M7c. Rachunek wyniku dostaje `labor` i `fixed`, bo tylko te dwie pozycje mają w M7a pisarza; przychody dokłada M7e, kiedy zacznie je czytać — zera udające pomiar są gorsze od braku pola |
| `AR-8` | **Firma i zakład nie są komponentami ECS**, wbrew brzmieniu WP1. Cały stan siedzi w zasobie `Firms` (`BTreeMap`), wpiętym do hasha przez `register_resource_hash` | Do firmy dociera się zawsze przez klucz albo przez zakład, nigdy przekrojowo po archetypach — czyli dokładnie ten argument, który wypchnął partie i oferty do aren (`K-16`). Wzorzec jest w projekcie ustalony: `Market` (M5) i `Plant` (M6) są zasobami z `BTreeMap`. Kryterium WP1 („hash stanu ECS obejmuje wszystkie komponenty M7") jest spełnione, bo zasób wchodzi do hasha na tych samych prawach co komponent |
| `AR-9` | **Scheduler dostaje kubełki slotów** — indeks pochodny per minuta doby, dzień miesiąca i dzień kwartału, **poza hashem** | Bez niego przydział slotów jest skanem po wszystkich firmach w każdym ticku: 10 tys. firm razy 1440 minut razy 3 poziomy to 43 mln sprawdzeń na dobę gry, czyli budżet z §7.6 zjedzony w całości przez samą pętlę „czy to już". Kubełki nie wchodzą do hasha, bo są funkcją zbioru kluczy i odtwarzają się z niego w całości; kolejka przepełnienia wchodzi, bo **jest** stanem |
| `AR-10`* | **`PlantSite` (M6) dostaje `labor_pct`** — patrz `K-44`. Pisarzem jest most M7a, czytelnikiem `sprobuj_start` | Do M7a praca nie była wejściem produkcji: młyn mielił tyle samo z pełną obsadą, z połową i bez nikogo, a płace były kosztem stojącym obok wyniku, nie jego przyczyną. Bez tego pola cała warstwa HR z PRD §7.5 nie miałaby jak objawić się w gospodarce, a kryterium WP3 („wynik proporcjonalny do `effective_labor`") byłoby niesprawdzalne |
| `AR-11` | **`D16` rozstrzygnięte jako rozszerzenie schematu `data/jobs/roles.ron`** (wersja 1 na 2, pole `weights`), a nie osobny plik. `magnat_firms::RoleTable` jest **drugim widokiem** na ten sam plik — patrz `K-43` | Osobny plik musiałby powtarzać kolejność ról, a dwie listy o wymuszonej wspólnej kolejności rozjeżdżają się przy pierwszej zmianie. Przejęcie `JobTable` przez `sim/firms` ciągnęłoby za sobą `UnitClass`, czyli gramatykę budynków M2 |
| `AR-12` | **Katalog ról rośnie z 4 do 46.** Cztery role M2 zostają na swoich miejscach | Obsada 96 typów zakładów potrzebuje zawodów, a nie jednej roli na rodzaj lokalu. Cztery pierwsze wpisy są nieruchome, bo `JobTable::role_for` bierze **pierwszą** rolę danej klasy lokalu — przestawienie ich przebudowałoby obsadę lokali M2 |
| `AR-13` | **Blok `StreamId` fazy M7 to 220–239, nie 240–259** | `K-4` przypisuje 220–239 fazie M7, a 240–259 fazie M8. Dokument fazy podawał drugi z tych zakresów w §1 i w §6 — poprawione po obu stronach (`AS-1`). M7a i tak nie zajęła ani jednego numeru, bo niczego nie losuje |
| `AR-14` | **Blok `DecisionReason` M7 (500–599) zostaje po M7a w całości wolny** | M7a buduje mechanizm dziennika decyzji (pierścień 32 wpisów, w hashu), ale **żadnej decyzji nie podejmuje** — pierwszym pisarzem jest M7b. Precedens jest w dzienniku: M5a zostawiła swój blok wolny z tego samego powodu. Wariant bez pisarza łamałby przy okazji regułę „wariant, którego nie da się pokazać graczowi jednym zdaniem, jest źle zaprojektowany", bo nie dałoby się napisać, kiedy powstaje |
| `AR-15` | **`Owner::External` bez ładunku** zamiast `External(ChainId)` | `ChainId` powstaje razem z sieciami zewnętrznymi w M7f. Do tego czasu wariant niesie sam fakt zewnętrzności i to wystarcza mostowi, który stawia firmy zastane |
| `AR-16` | **Most „miasto → firmy" (`tools/headless::firms`) czyta obsadę z komponentów ECS, a nie z `Workplace.occupant`** | `Workplace.occupant` jest **zawsze** `None`, także po Etapie 8 — sprawdzone w kodzie, nie założone. Zatrudnienie Etapu 8 żyje w komponencie `magnat_agents::Employment` (`site = SITE_KEY_BASE + indeks zakładu`). Most zbudowany na `occupant` dałby wszystkie firmy bez ani jednego pracownika. Druga pułapka tej samej klasy: `SiteSeed.units` obejmuje lokale **całego budynku**, więc powierzchnia zakładu liczy się z filtrem `UnitOccupant::Site` — inaczej sklep na parterze kamienicy dostaje powierzchnię kamienicy razem z mieszkaniami |

### Co zostaje otwarte po M7a

| # | Co | Adres |
|---|---|---|
| `AT-1` | **W mieście 4 km staje 215 firm i 217 zakładów**, przy obietnicy §1 dokumentu fazy, że „miasto 150 tys. startuje z ~6–10 tys. firm AI". Proporcjonalnie wychodzi rząd 800, czyli dziesięciokrotnie za mało. To nie jest brak M7a — tyle zakładów stawia Etap 7 generatora — ale obietnica fazy stoi i ktoś musi ją domknąć: albo powstawaniem firm, albo gęstszym obsadzeniem zabudowy | **M7f** (powstawanie firm, §5.14), przy udziale M2 |
| `AT-2` | **`labor_pct` jest wpisywane raz, przy stawianiu miasta.** Obsada z Etapu 8 jest zdjęciem, a nie liczbą prowadzoną: nikt jej nie aktualizuje przy odejściu, śmierci ani zatrudnieniu, więc wpięcie jej w produkcję na stałe dałoby wartość, która zamarza | **M7b** — rynek pracy jest pierwszą podfazą, która tę obsadę prowadzi, i to on przenosi zapis `labor_pct` z mostu do systemu |
| `AT-3` | **Trzy role z katalogu nie mają ani jednego zakładu**: `retail_clerk` (rola generyczna M2, w zakładach zastąpiona przez `cashier` i `sales_assistant`), `journalist` i `leasing_agent`. Te dwie należą do redakcji i do nieruchomości, a takich archetypów w `data/buildings/` nie ma — `SiteTypeCategory::Media` ma jeden typ, `RealEstate` zero | **M10** (media, §7.6) i **M10** (rynek nieruchomości, §6.7) |
| `AT-4` | **Obsada z Etapu 8 nie zgadza się co do roli z obsadą z katalogu M7.** M2 przypisuje jedną rolę na klasę lokalu, katalog M7 rozbija ją na zawody, więc most dopisuje stanowiska spoza planu katalogu. W mieście 4 km obsadzonych jest 14 405 z 23 912 etatów | **M7b** — to jest dokładnie ta rozbieżność, którą rynek pracy ma zamykać zatrudnieniem i zwolnieniem |
