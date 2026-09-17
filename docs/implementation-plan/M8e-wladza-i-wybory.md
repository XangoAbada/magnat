# M8e — Władza i wybory

Podfaza 5 z 5 fazy **M8 — Miasto jako aktor** (`M8-miasto-jako-aktor.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M8a–M8d w całości. |
| **Pakiety robocze** | WP9, WP10, WP11 |
| **Projekt techniczny** | §5.2, §5.7, §5.8 |
| **Wynik do pokazania** | Pełny artefakt fazy z §1 dokumentu fazy: podatki, wybory, sieci przesyłowe, zdarzenia i pogoda. |
| **Kryterium zamknięcia** | Kryteria WP9–WP11, testy T1–T8 zielone oraz bramki 1–7 fazy M8 w `00-postep.md`. |
| **Poprzednia / następna** | `M8d-uslugi-i-prawo.md` · — (ostatnia w fazie) |

Władza miejska AI z celami, histerezą i menu działań, wybory z kadencjami i programami, inspektory, kronika i domknięcie fazy.

---

## Pakiety robocze

### WP9 — Władza miejska AI
**Zależy od:** WP1–WP3, WP7.
Cele (poparcie, saldo budżetu, rozwój), preferencje jako wagi, menu działań z heurystyką
oczekiwanego skutku, pętla miesięczna z histerezą (ochrona przed oscylacją stawek).
Inwestycje (drogi, szkoły, komunikacja), strefowanie, przetargi z kryteriami i rozstrzygnięciem,
regulacje, dotacje z limitem budżetowym.
**Kryterium ukończenia:** test T5 (brak oscylacji): w 20 latach gry bez zdarzeń zewnętrznych
żadna stawka podatkowa nie zmienia kierunku częściej niż raz na 24 miesiące.
Każda decyzja ma `DecisionReason` czytelny w inspektorze.
**Rozmiar:** L

### WP10 — Wybory
**Zależy od:** WP9, WP7 (jakość usług jako wejście), M3 (status, nastrój, dzielnica).
Kalendarz kadencji, kandydaci z programem, frekwencja i głos per mieszkaniec, podział mandatów,
wpływ na politykę. Wsparcie kandydata przez gracza: legalne (darowizna, wykup reklamy)
i nielegalne (łapówka) z ryzykiem ujawnienia jako zdarzenia politycznego.
**Kryterium ukończenia:** test T6 (determinizm wyborów) + test wrażliwości: pogorszenie jakości
usług w jednej dzielnicy o 30% obniża tam poparcie inkumbenta o co najmniej 5 pkt proc.
**Rozmiar:** M

### WP11 — Inspektory, kronika, domknięcie fazy
**Zależy od:** wszystkie.
Inspektor budżetu, inspektor sieci (graf z przepływami i przeciążeniami), log hazardów
(„dlaczego jeszcze nie ma suszy" — wartości sond i wynikowy hazard), wpisy kroniki dla zdarzeń.
Dopisanie komponentów M8 do funkcji hasza stanu ECS. Scenariusz `m8-demo`.
**Kryterium ukończenia:** Definition of Done fazy: wszystkie testy T1–T8 zielone,
`clippy -D warnings`, benchmarki w budżecie, scenariusz demo odtwarzalny z seeda.
**Rozmiar:** M

---

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.2 Polityka, pozwolenia, przetargi, władza

```rust
pub struct Government {
    pub mayor: CitizenId,
    pub mayor_pref: Preference,
    pub council: Vec<(CitizenId, Preference)>,     // mandaty
    pub term_start: Tick,
    pub term_ticks: u64,
    pub approval_bps_by_district: Vec<u32>,        // aktualizowane miesięcznie
    pub goals: Goals,                              // { approval_w, balance_w, growth_w } — wagi z preferencji
}

pub struct Preference {                            // 4 osie z §10.1, sumują się do 10000 bps
    pub growth_bps: u32, pub social_bps: u32,
    pub green_bps: u32,  pub populist_bps: u32,
}

pub enum Policy {
    TaxRate { kind: TaxKind, bps: u32 },
    Zoning { parcel: ParcelId, use_: ZoneUse },
    TrafficRestriction(EdgeRestriction),          // patrz niżej — JEDEN wariant na wszystkie
                                                  // ograniczenia ruchowe, nie jeden na rodzaj
    EmissionLimit { pollutant: Pollutant, per_day: i64, penalty_per_unit: Money },
    TradingHours { district: DistrictId, hours: WeekMask },   // w tym zakaz handlu w niedziele
    Subsidy { target: SubsidyTarget, per_period: Money, cap: Money, sunset: Tick },
    TariffCap { kind: UtilityKind, max_per_unit: Money },
    MinWage(Money),
    ParkingFee { district: DistrictId, per_hour: Money },
    ServiceFunding { kind: ServiceKind, per_month: Money },
}

/// Siatka tygodniowa (K-15): 7 dni × 24 godziny = 168 bitów.
/// Indeksowane `DayOfWeek` z `SimCalendar` (engine/core) — M8 nie liczy dnia tygodnia sam.
pub struct WeekMask([u64; 3]);
impl WeekMask { pub fn allows(&self, d: DayOfWeek, hour: u8) -> bool; }
```

**Regulacje ruchowe — predykat na krawędziach, nie nowy profil trasy.**
Ograniczenie M4: trasy liczą się na Contraction Hierarchies z ustaloną, małą liczbą profili
(`Passenger`, `HeavyDay`, `HeavyNight`) — każdy profil to osobny zestaw wag, ~450 ms
kustomizacji i do 30 MB. **Liczba profili jest zasobem limitowanym i M8 go nie powiększa.**
Gdyby każda uchwała rady tworzyła własny profil, koszt routingu rósłby liniowo z aktywnością
polityczną miasta — czyli rósłby dokładnie wtedy, gdy rozgrywka robi się ciekawa.

Dlatego **wszystkie** ograniczenia ruchowe — zakaz ruchu ciężkiego w centrum, tonaż na moście,
strefa czystego transportu, okna dostaw w śródmieściu — to jeden typ, wyrażony jako zbiór
wyłączonych krawędzi plus okno czasowe:

```rust
pub struct EdgeRestriction {
    pub edges: EdgeSet,                     // krawędzie grafu M4, nie geometria ani dzielnica
    pub applies_to: VehicleClass,           // Heavy | Delivery | Combustion | Transit | All
    pub window: WeekMask,                   // K-15 — kiedy obowiązuje
    pub exemption: Option<PermitKind>,      // np. OversizeTransport zwalnia z zakazu
    pub fine: Money,                        // egzekucja: mandat, nie bariera fizyczna
}
```

Kompilacja do maski (`EveryHour`, tylko przy zmianie zbioru obowiązujących regulacji):

```rust
/// Wszystkie aktywne regulacje dla danej klasy pojazdu i godziny składają się
/// SUMĄ BITOWĄ w JEDNĄ maskę na istniejący profil. N regulacji = 1 maska,
/// więc koszt jest stały w liczbie uchwał, nie liniowy.
pub fn compile_edge_mask(policies: &PolicySet, class: VehicleClass,
                         day: DayOfWeek, hour: u8) -> EdgeMask;

pub struct PolicyRecord {
    pub policy: Policy,
    pub enacted_at: Tick,
    pub effective_from: Tick,        // zawsze ≥ enacted_at + okres vacatio legis
    pub sunset: Option<Tick>,
    pub vote: CouncilVote,
    pub reason: DecisionReason,      // wyjaśnialność (§7 dokumentu 00)
}

// ---------- pozwolenia: czas jest kosztem ----------

pub struct Permit {
    pub id: PermitId,
    pub applicant: Applicant,
    pub kind: PermitKind,            // Build | ChangeOfUse | Demolition | EnvClearance
                                     // | AlcoholLicense | FoodService | RoadAccess | OversizeTransport
    pub parcel: Option<ParcelId>,
    pub filed_at: Tick,
    pub status: PermitStatus,        // Queued | UnderReview | RequestForInfo{deadline}
                                     // | Approved{at} | Rejected(RejectReason) | Expired
    pub fee: Money,
    pub office: OfficeId,
    pub priority: PermitPriority,    // Standard | Expedited(opłata) | Political(interwencja)
}
```

Czas wydania pozwolenia **nie jest parametrem** — wynika z obsady i kolejki:

```rust
/// EveryDay. throughput = Σ po urzędnikach floor(skill/10) * digitization_mul_bps/10000,
/// pomnożone przez 0 w dni wolne urzędu (K-15: `PermitOffice.open_days: WeekMask`).
/// Kolejka FIFO w obrębie klasy priorytetu; sprawy złożone (EnvClearance) kosztują 5 jednostek.
/// Skutek uboczny zamierzony: wniosek złożony w piątek leży dwa dni — czas gracza to koszt.
fn sys_process_permit_queue(offices: &mut [PermitOffice], permits: &mut PermitRegistry,
                            day: DayOfWeek, tick: Tick);
```

```rust
pub struct Tender {
    pub id: TenderId,
    pub subject: TenderSubject,      // BusLine(LineId) | WasteCollection(DistrictId)
                                     // | Construction(ProjectId) | Maintenance(AssetId)
    pub published_at: Tick,
    pub bid_deadline: Tick,
    pub criteria: BidCriteria,       // wagi bps: cena / jakość / termin / lokalność
    pub bids: Vec<Bid>,
    pub outcome: Option<TenderOutcome>,   // { winner: FirmId, contract: ContractId }
}

pub struct Bid {
    pub bidder: FirmId, pub price: Money, pub quality_promise: Q,
    pub delivery_ticks: u64,
    pub kickback: Option<Money>,     // nielegalne; podnosi hazard kontroli i skandalu
}
```

Pętla decyzyjna burmistrza (`EveryMonth`): ocena celów → wybór działania z menu
z heurystyką oczekiwanego skutku (nie optymalizacja, nie solver) → uchwalenie z terminem
wejścia w życie. **Histereza obowiązkowa:** zmiana stawki wymaga odchylenia od celu
powyżej progu utrzymującego się 3 miesiące — inaczej AI oscyluje.

### 5.7 Wybory

```rust
pub struct Election {
    pub scheduled_at: Tick, pub term_ticks: u64,
    pub candidates: Vec<Candidate>,
    pub result: Option<ElectionResult>,
}

pub struct Candidate {
    pub citizen: CitizenId,
    pub platform: Preference,                       // 4 osie, te same co u burmistrza
    pub tax_stance: Vec<(TaxKind, i32)>,            // deklarowana zmiana w bps
    pub funding: Money,
    pub media_reach: Q,                             // do M10 stub: f(funding, dzielnica)
    pub backers: Vec<(Backer, Money, Legality)>,    // gracz tu wchodzi
}

pub struct ElectionResult {
    pub turnout_bps: u32,
    pub per_district: Vec<DistrictTally>,
    pub council: Vec<(u8 /*idx kandydata*/, u8 /*mandaty*/)>,
    pub mayor: u8,
}

/// EveryDay w dniu wyborów. Iteracja po CitizenId rosnąco — determinizm.
/// turnout   = f(status, wiek, nastrój, staż w dzielnicy, ekspozycja medialna) → próg z RNG
/// preferencja = softmax po użyteczności kandydata: zgodność programu z interesem wyborcy
///               (podatki wobec jego dochodu), ocena usług w JEGO dzielnicy za kadencję,
///               nastrój, ekspozycja medialna, więzi osobiste (graf relacji M3)
fn sys_run_election(e: &mut Election, citizens: &CitizenView, cov: &ServiceCoverage, seed: u64);
```

Wsparcie nielegalne (`Legality::Illegal`) tworzy ukryte powiązanie, które podnosi hazard
zdarzenia `political/scandal` przez sondę — a ujawnienie uderza w kandydata i w gracza
(sprawa w `Agency::Antitrust`/prokuratura, kara, utrata koncesji).

### 5.8 Systemy ECS i ich częstotliwość

| System | Crate | Częstotliwość | Odczyt / zapis |
|---|---|---|---|
| `sys_solve_power_grid` | `traffic` | EveryMinute | R: węzły, popyt / W: `SupplyState`, `EdgeState` |
| `sys_solve_other_grids` | `traffic` | EveryHour | jw. dla gazu, wody, ciepła, telekomu |
| `sys_meter_utilities` | `traffic` | EveryMinute | W: `metered` |
| `sys_assess_vat` | `city` | na transakcji (hook M5) | W: bufor `TaxCharge` |
| `sys_assess_payroll_pit` | `city` | na wypłacie (hook M7) | W: bufor `TaxCharge` |
| `sys_assess_property_tax` | `city` | EveryMonth | R: wycena parcel / W: bufor |
| `sys_assess_excise_duty` | `city` | na zdarzeniu produkcji/importu | W: bufor |
| `sys_assess_cit` | `city` | EveryMonth (domknięcie roku firmy) | R: RZiS / W: bufor |
| `sys_settle_tax_charges` | `city` | EveryDay | W: `CityBudget`, `PaymentCmd` |
| `sys_process_permit_queue` | `city` | EveryDay | W: `Permit.status` |
| `sys_update_service_quality` | `city` | EveryMonth | W: `PublicService.quality` |
| `sys_publish_service_coverage` | `city` | EveryMonth | W: `ServiceCoverage` |
| `sys_agency_inspections` | `city` | EveryDay | W: `Case`, bufor `TaxCharge` |
| `sys_government_decide` | `city` | EveryMonth | W: `PolicyRecord`, `Tender` |
| `sys_run_election` | `city` | EveryDay (gate: dzień wyborów) | W: `Government` |
| `sys_budget_close` | `city` | EveryMonth / EveryMonth (gate: grudzień) | W: `CityBudget` |
| `sys_weather_step` | `events` | EveryHour | W: `Weather` |
| `sys_eval_hazards_slow` | `events` | EveryDay | R: sondy / W: bufor `WorldEvent` |
| `sys_eval_hazards_fast` | `events` | EveryHour | jw. |
| `sys_apply_events` | `events` | EveryHour | W: `ParamOverlay`, Kronika |
| `sys_expire_events` | `events` | EveryHour | W: `ParamOverlay` |
| `sys_macro_indicators` | `events` | EveryMonth | W: `MacroIndicators`, `DemographyParams` |

---

## Korekty wpisane w trakcie M8e

Poprawki do **tego** dokumentu, które implementacja pokazała jako nieprawdziwe
albo niedopowiedziane. Gwiazdka = zmiana zakresu albo kryterium.

| # | Korekta | Dlaczego |
|---|---|---|
| `CI-1` ★ | **`Policy::Zoning` nie powstaje.** §5.2 wymieniał go jako jeden z dziesięciu wariantów; nie ma go w kodzie i nie ma go z powodu, który da się sprawdzić `grep`-em: `Parcel.zone` czyta w całym repozytorium **wyłącznie generator miasta** (`city/build.rs`, `city/districts.rs`), a generator biegnie raz, przed pierwszym tickiem. Uchwała o przekwalifikowaniu parceli zmieniałaby pole, którego nikt już nie przeczyta, i wyglądałaby w karcie rady dokładnie tak samo jak uchwała działająca (`R2`). Wraz z nią odpada zapowiedziane w „Zmianach wpisanych po R1" rozcięcie `assign_zones` — **M8e tej funkcji nie dotyka**, a pozycja 36 rejestru długu w `R1-refaktor-po-M5.md` zostaje bez adresata M8e. Wejściem będzie rynek nieruchomości (M10) albo przebudowa w trakcie gry (M12) | Przypadek (5) z `K-18`: pakiet obiecuje coś, czego żaden mechanizm nie jest właścicielem. Sprawdzone przed napisaniem wariantu, nie po |
| `CI-2` ★ | **`Policy::ParkingFee` nie powstaje.** Cennik parkingu (`ParkingLot.price_gr_per_hour`) jest dziś wszędzie zerem i **ma czytelnika** — wchodzi do porównania środków transportu (`oracle/offers.rs`) — ale siedzi w `ParkingRegistry` **wewnątrz** `TrafficOracle`, do którego jedyna droga zapisu to `set_parking(&mut self, …)` wołane przy budowie świata. Oracle żyje za `Arc` w `AgentSources`, więc uchwała nie ma jak go dotknąć bez punktu podmiany po stronie M4. Wariant bez drogi do skutku to ten sam `R2` co przy strefowaniu | Przypadek (1) z `K-18`: kontrakt, na którym wariant stoi, wygląda inaczej, niż §5.2 zakłada |
| `CI-3` | **`Policy::TradingHours` niesie `OpenHours`, nie `WeekMask`.** §5.2 zapisywał okno tygodniowe; sklep, mieszkaniec i rampa magazynu mówią o godzinach otwarcia typem `OpenHours` z `engine/core` od M3 (`K-34`), więc uchwała wyrażona drugim typem wymagałaby konwersji gubiącej to, czego `OpenHours` nie umie wyrazić — a wtedy uchwała znaczyłaby co innego niż to, co widać w sklepie. `WeekMask` zostaje i jest potrzebny przy `EdgeRestriction`, gdzie okno godzina po godzinie jest treścią, a nie zaokrągleniem | Przypadek (2) z `K-18`, plus druga rung drabiny: typ, który już jest w repozytorium i który rozumieją trzy fazy |
| `CI-4` ★ | **Regulacje ruchowe nie powstają — cały blok.** §5.2 projektuje `EdgeRestriction`, `WeekMask`, `VehicleClass` i `compile_edge_mask`, a §6 dokumentu fazy podnosi to do rangi kontraktu z M4. Kontrakt jest **dobry** i jego kształt zostaje w §6 bez zmian: jedna maska składana sumą bitową, koszt stały w liczbie uchwał, router nakłada ją na istniejący profil. Nie powstaje **implementacja**, i to z tego samego powodu co przy opłacie parkingowej (`CI-2`): punkt wejścia po stronie M4 to `NavRouter::customize(profile, banned, …)` schowany w `TrafficOracle` za `&mut self` przy budowie świata, a oracle żyje za `Arc` w `AgentSources`. Uchwała nie ma jak tam dojść. Napisanie samych typów i maski bez czytelnika dałoby cztery publiczne nazwy, sto pięćdziesiąt linii i test mierzący tablicę `bool`, której nikt nie nakłada — czyli dokładnie mechanizm, który przechodzi każdy test i w raporcie wygląda jak działający (`R2`). Przy okazji odpada `fine`: mandat wymaga sprawdzenia, czy pojazd wjechał w zakazaną krawędź, a to jest mikro-ruch i kierowca wybierający trasę wbrew routerowi — jedno i drugie należy do M4 i M9 | Przypadek (5) z `K-18` w najczystszej postaci, dwa razy w jednej fazie. Adres wyjścia jest ten sam co dla `ParkingFee` i zapisany razem z nim (`CJ-4`): metoda z `&self` po stronie `sim/traffic`, tak samo jak `set_weather` (`K-26`) |
| `CI-5` ★ | **Tabela systemów z §5.8 opisuje jeden system, a nie dwadzieścia jeden.** Rozszerzenie `CB-2` (po M8a) i `CH-3` (po M8d): `sys_government_decide`, `sys_run_election`, `sys_process_permit_queue`, `sys_update_service_quality`, `sys_publish_service_coverage` i `sys_agency_inspections` to **kroki** `city.Tax` (`Cadence::EveryDay`, wyłączny, po `economy.Market`), a nie systemy ECS. Po M8e ten system wykonuje dziewięć kroków: daniny hurtu, deklaracja miesięczna, rozliczenie, starzenie zaległości, budżet, **skutki uchwał**, usługi i urzędy, **władza dobowa** (przetargi, wybory) i **władza miesięczna** (poparcie, decyzja). Wierszami tabeli zostają nazwy kroków. Systemami spoza `city` zostają `traffic.Utility` (`CD-7`) i `events.Event` | Przypadek (4) z `K-18`: kolejność kroków fazy jest widoczna naraz wyłącznie w tej tabeli, a ta przestała opisywać to, co się dzieje |
| `CI-6` ★ | **`Election::back_candidate` nie przelewa pieniędzy i to jest kontrakt, nie przeoczenie.** `sim/city` nie ma `&mut Books` w chwili, w której kandydat przyjmuje wpłatę — przelew robi **wołający**: dziś `rule::finansuj_kampanie`, od M9 komenda gracza. Rodzaj zapisu jest nowy i wymagał wpisu w dokumencie nadrzędnym: `TxKind::CampaignDonation` (`K-66`) | Przypadek (2) z `K-18`. Zapisane, bo z nazwy `back_candidate` naturalnie wynika, że metoda robi całość |
| `CI-7` | **`TaxCode` dostaje `excise_scale_bp`, bo akcyza jest kwotowa.** §5.2 zakłada, że `Policy::TaxRate { kind, bps }` pokrywa wszystkie siedem danin; akcyza nie ma stawki procentowej, tylko grosze za kilogram i za kilowatogodzinę. Uchwała niesie więc **mnożnik** (10 000 bp = tyle, ile w danych), a kwoty w `data/city/tax.ron` zostają nietknięte — dzięki temu dziesiąta zmiana stawki liczy się od danych, a nie od dziewiątego zaokrąglenia. Cło i koncesje burmistrz pomija jawnie: pierwsze jest polityką handlową w cudzym pliku (`K-37`), drugie opłatą za wpis do rejestru | Przypadek (3) z `K-18`: kryterium „uchwała zmienia stawkę" jest dla dwóch danin z siedmiu niewykonalne w zapowiedzianej postaci |
| `CI-8` | **Histereza obowiązuje też obietnicę wyborczą.** Trzy karencje z §5.2 były opisane jako zabezpieczenie pętli burmistrza; nowy burmistrz wchodzi jednak z programem podatkowym, który omija menu. Stawka podniesiona w czterdziestym szóstym miesiącu i obcięta w czterdziestym ósmym to zwrot kierunku po dwóch miesiącach — i **żadna z tych decyzji z osobna reguły nie łamie**. Obietnica, której nie wolno dziś spełnić, przepada i nie odkłada się na później: wyborca głosował na program, a nie na kolejkę zadań | Przypadek (4) z `K-18`, znaleziony przy pisaniu testu T5, nie przy przeglądzie. Bez tego test pękałby raz na kadencję, czyli najgorzej, jak się da |
| `CI-9` | **`political/scandal` jako zdarzenie nie powstaje.** §5.7 zapowiada, że wsparcie nielegalne podnosi hazard zdarzenia przez sondę. Wykonane jest to samo, tylko drogą, która **już istnieje**: wpłata poza rejestrem otwiera sprawę w urzędzie antymonopolowym (`Enforcement::otworz`), dowody rosną, kara przychodzi na końcu, budżet ją księguje. Druga ścieżka do tego samego skutku — sonda, definicja zdarzenia, `ParamPatch` — byłaby dokładnie tym, przed czym bronią `K-11` i `K-13`. Przy okazji: sonda musiałaby czytać `City`, a `sim/events` stoi **pod** `sim/city` w grafie i czytać go nie może | Przypadek (2) i (5) z `K-18` naraz: API istnieje i nazywa się konkretnie, a drugiego właściciela tego skutku nie ma |
| `CI-10` ★ | **`Policy::ServiceFunding` i `Policy::Subsidy` łączą się w jeden wariant `SpendShare { category, bp }`** — i to jest poprawka **zmieniająca wynik**, a nie porządkująca. Pierwsza wersja miała osobną kwotę „na placówkę", którą czytała wyłącznie `services::update_quality`: uchwała o dofinansowaniu oświaty podnosiła jakość szkół i **nie wydawała ani grosza**, bo do `budget::close_month` ta liczba nie docierała. Po poprawce uchwała przestawia **udział kierunku wydatku w planie miesięcznym**, a udział czyta i przelew, i jakość — przez jedną funkcję `PolicySet::shares`. Dotacja nie jest przy tym osobnym wariantem, tylko udziałem `SpendCategory::Subsidies`, więc z dwóch wariantów robi się jeden i oba stają się prawdziwe. Ta sama zmiana usuwa drugi błąd: menu miało zaszytą kwotę 120 zł na placówkę, czyli o rzędy wielkości niższą niż udział z planu — „uchwała o dofinansowaniu" faktycznie tnie finansowanie | Znalezione **recenzją przed commitem**, nie testem: oba testy przechodziły, bo żaden nie pytał, czy pieniądz się rusza razem z jakością. To jest ta sama klasa błędu co martwa akcyza z `CC-9` — mechanizm liczy, tylko nikt nie zapłacił |
| `CI-11` ★ | **Limit emisji jest w gramach na minutę i uchwała musi mówić w tych samych jednostkach.** `Policy::EmissionLimit` niósł `per_day_g`, a trafiał do `Enforcement::emission_limit_g_per_min` — pierwsza uchwała ekologicznego burmistrza rozluźniała limit tysiąc czterysta czterdzieści razy i ochrona środowiska przestawała otwierać sprawy. Sygnał `emission_bp`, na którym stoi decyzja o **kolejnej** uchwale, porównywał przy tym gramy na minutę z gramami na dobę. Pole nazywa się od tej chwili `max_g_per_min`, a nowy limit liczy się z **obowiązującego** (cztery piąte), nie ze stałej: stała byłaby drugim źródłem liczby, którą miasto już zna, i w innych jednostkach niż pierwsze | Błąd jednostek, który wygląda identycznie jak działający mechanizm: limit istnieje, jest liczbą i rośnie. Ta sama klasa co `K-25` (mikrolitry paliwa) |
| `CI-12` | **Przetarg bez ofert kończy się i przedmiot wraca na rynek.** Pierwsza wersja nie zamykała takiego postępowania, tylko przesuwała jego termin o trzydzieści dób — a `publish` odmawia ogłoszenia, gdy dla tego przedmiotu wisi nierozstrzygnięte postępowanie. Skutek: dzielnica, w której w pierwszym przetargu nikt nie stanął, nie dostawała już nigdy żadnego, a powód „nie przyszedł nikt" leciał do dziennika co miesiąc bez końca. `Tender` dostaje pole `closed` — bo „wygrał ten" i „nie przyszedł nikt" to dwie różne odpowiedzi, a nie brak odpowiedzi | Pętla, która nie jest awarią: nic się nie zawiesza, nic nie pęka, tylko jeden przedmiot cicho wypada z mechanizmu na zawsze |
| `CI-13` ★ | **Miasto płaciło za odbiór odpadów dwa razy.** `budget::close_month` przelewa co miesiąc pełny udział `SpendCategory::Waste` na konto reszty świata, a `rule::zaplac_umowy` płaci jeszcze umowy przetargowe z tego samego udziału — razem około stu siedemdziesięciu procent planu. Budżet dostaje od tej chwili `SpendPlan` z dwiema tablicami: udziałami po uchwałach i **rezerwą na to, co już zakontraktowano**. Obie chodzą razem i obie pochodzą z jednego miejsca, więc struktura, a nie dwa argumenty — osobne argumenty znaczyłyby, że da się podać jeden bez drugiego | Znalezione recenzją. `check_conservation` tego nie łapie i nie ma jak: pieniądz nie ginie, tylko wychodzi dwa razy tam, gdzie miał wyjść raz |
| `CI-14` ★ | **Wybory powtarzały się co dobę od pierwszego rozstrzygnięcia.** Warunek wejścia pytał „czy w zasobie są jakieś wybory", a wynik **zostaje** w zasobie, bo pokazuje go karta rady — więc od dnia pierwszych wyborów miasto głosowało codziennie. Pytanie brzmi od tej chwili „czy trwa kampania", czyli czy wybory nie mają jeszcze wyniku. Przy okazji: `run_election` zwracające `None` (miasto bez dorosłych mieszkańców) nie kasuje już kampanii razem z wpłatami, tylko odkłada ją z powrotem | Znalezione recenzją, a nie przebiegiem — 150-dobowy test integracyjny ma dwie kadencje po dwa miesiące i przechodził, bo powtórka daje ten sam wynik przy tym samym stanie |
| `CI-15` | **Kryterium „złapany przestaje ukrywać" mierzy skutek kary, a nie stan po kolejnych miesiącach.** Test M8d żądał dokładnego zera na końcu dziewięćdziesięciodobowego przebiegu, czyli pytał „i nigdy więcej nie ukrywał" — a to jest inne pytanie niż to, które stoi w kryterium WP8. Domiar dalej zeruje udział ukrywany; zakład pod presją zaczyna go odbudowywać co miesiąc i to jest zamierzone (`R10`). Test pyta od tej chwili, czy zakład **wrócił do poziomu sprzed kontroli**. Wyzwalaczem korekty była `CH-4`: jakość placówek liczy się z planu **po cięciu**, więc zakłady częściej wpadają w kłopoty | Przypadek (3) z `K-18`: kryterium spełnione tożsamościowo dopóty, dopóki żaden złapany zakład nie miał gorszego miesiąca. Ta sama korekta co `CH-1` (T4 mierzy tempo, nie stan) |


## Zmiany wpisane po R1

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po refaktorze R1.

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **`Policy::Zoning { parcel: ParcelId, use_: ZoneUse }` z §5.2 nazywa typ, którego nie ma.** W kodzie od M2c jest **`ZoneKind`** (`sim/world/src/city/zoning.rs`), z wariantami `Residential(ResDensity)`, `Green`, `Extraction`, `Water`, `Undevelopable` i pozostałymi. Jedno z dwóch trzeba poprawić i **tańsza jest poprawka w dokumencie**: `ZoneKind` jest w kodzie od trzech faz, czyta go generator, zabudowa i wycena | Rozjazd znaleziony przy R1, przy czytaniu `assign_zones` do rejestru długu strukturalnego. Nie kosztuje dziś nic, a przy starcie M8e kosztowałby pół dnia szukania typu, którego nie ma |
| | **`assign_zones` ma 322 linie i jest jedną funkcją — M8e będzie musiał ją rozciąć, i to jest powód, dla którego R1 tego nie zrobił.** Dziś strefa powstaje **raz, dla całego miasta**, w sześciu ponumerowanych krokach na wspólnej macierzy `score[n][16]`. `Policy::Zoning` potrzebuje ścieżki „przekwalifikuj **jedną** parcelę w trakcie gry", czyli rozdzielenia *policz punktację* od *przydziel* — a to jest zmiana kształtu, nie przeniesienie bloku. Pozycja 36 rejestru długu w `R1-refaktor-po-M5.md` | Najczystszy szew, gdyby M8e szukał punktu zaczepienia: **krok 6** (bufor przemysłu ciężkiego, linie ~1010–1080) czyta `score`, `area` i sąsiedztwo, mutuje `zone` i zwraca dwa ostrzeżenia — wychodzi przeniesieniem bloku z sygnaturą na pięć argumentów. Kroki 1–4 są splecione: krok 4 czyta macierz z kroku 1 i kwoty z kroku 3. **Rng w tym pliku nie ma wcale**; determinizm stoi na `sort_by(total_cmp)` z jawnymi remisami po `BlockId` i na kolejności kroków |

---

## Zmiany wpisane po M8a

Zgodnie z `K-18`. Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **Tabeli systemów podatkowych z §5.8 nie ma i nie będzie w tym kształcie.** `sys_assess_vat` („na transakcji"), `sys_assess_payroll_pit` („na wypłacie"), `sys_settle_tax_charges` (EveryDay) i `sys_budget_close` (EveryMonth) to **cztery wiersze opisujące jeden system**: `city.Tax`, `Cadence::EveryDay`, wyłączny (`K-21`), stojący po `economy.Market` przez `after_if_present` (`K-51`). Naliczenie na transakcji i na wypłacie **nie jest systemem** — jest hakiem w `TaxEngine` (`K-57`), bo system ECS nie ma jak wpiąć się w środek przelewu. Domknięcie budżetu też nie jest osobnym systemem: jest krokiem piątym tego samego, bo musi stać **po** rozliczeniu należności, a dwa systemy wyłączne o wymuszonej kolejności to `K-53` bez potrzeby. Wiersze zostają jako **nazwy kroków**, nie systemów | Przypadek (5) z `K-18`: pakiet obiecuje coś, czego żaden pakiet nie jest właścicielem. Wpisane po M8a, gdzie te cztery wiersze powstały jako jeden `CitySystem` i inaczej powstać nie mogły |
| | **Uchwała zmieniająca stawkę podmienia `TaxCode`, a nie pole w nim.** `City.code` jest `Arc<TaxCode>`, a `City.rates` — `Arc<VatTable>` rozwiązaną z katalogu towarów przy budowie miasta. Zmiana stawki VAT wymaga więc **obu**: nowego kodeksu i nowego rozwiązania (`TaxCode::resolve`), a potem `Market::set_tax_engine(Box::new(city.tax_engine()))`, żeby rynek zaczął liczyć cenę brutto po nowemu. Stawki już naliczonych należności nie drgną, bo `TaxCharge` niesie `rate_snapshot` | `effective_from` w `TaxCode` istnieje i czeka na pierwszego pisarza — jest nim uchwała rady, czyli ta podfaza. M8a wczytuje kodeks z `effective_from: Tick(0)` i nigdy go nie zmienia |
| | **Wydatek publiczny idzie dziś do „reszty świata" i to jest `ponytail:` sufit z nazwanym wyjściem.** `budget::wydaj` przelewa kwotę z konta miasta na `rest_of_world`, bo usługi publiczne, ich obsada i ich zakłady powstają w M8d. Kwota, kierunek (`SpendCategory`) i powód (`DecisionReason::PublicSpend`) są już prawdziwe; adresat nie. Wyjście: konto zakładu usługowego w miejsce `rest`, bez zmiany sygnatury | Wpisane po M8a, żeby M8d nie szukał, gdzie wpiąć odbiorcę — jest jedna linia i ma komentarz |

## Zmiany wpisane po decyzji właściciela produktu (2026-09-17)

Zgodnie z `K-18`. Gwiazdka = zmiana zakresu albo kryterium. Wymaganie: **każdy obiekt
w świecie jest klikalny i ma kartę inspekcji z zakładkami, a każda nazwa w karcie jest
odnośnikiem**. Konsekwencja dla tej podfazy jest jedna i mała, ale nie było jej nigdzie.

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **Rada miasta jest klikalna: `Subject::Government`** (`M9c` §5.7, `K-62`) — singleton z zakładkami Skład · Polityka i podatki · Budżet · Wybory · Przetargi · Dlaczego. **`Election` nie dostaje własnego wariantu** — jest jedna naraz i jest zakładką tej karty | WP11 buduje dziś „inspektor budżetu" i „inspektor sieci" jako narzędzia deweloperskie, niepodpięte pod `InspectionCard`. To są dwie ścieżki do tych samych liczb, a `K-11`/`K-13` mówią jasno, co sądzimy o dwóch ścieżkach do jednej prawdy. Inspektor deweloperski zostaje jako wyjście tekstowe dla `headless`, ale **czyta ten sam model, co karta** |
| | **`Tender` i `Permit` wchodzą do `Subject`** (`Tender(TenderId)`, `Permit(PermitId)`). Karta przetargu: przedmiot, termin, oferty z odnośnikami do firm, rozstrzygnięcie i powód | Oba typy mają już `id` (§5.x), a `Bid` wskazuje na firmę — więc graf odnośników jest kompletny w danych. Przetarg, który gracz przegrał, bez karty z powodem jest dokładnie tym, przed czym broni PRD §14.1 |
| | **Kandydat w wyborach linkuje do mieszkańca, a `backers` — do firm** | `Candidate.citizen` to `CitizenId`, a `backers: Vec<(Backer, Money, Legality)>` niesie fundatora. Jeśli gracz ma zobaczyć, kto komu płacił, to musi móc w to kliknąć — inaczej `Legality::Illegal` jest liczbą w tabeli, a nie zarzutem wobec konkretnej firmy |
