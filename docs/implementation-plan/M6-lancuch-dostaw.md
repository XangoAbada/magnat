# M6 — Łańcuch dostaw

Crate własny: `sim/supply`. Crate'y rozszerzane: `sim/economy` (rynek B2B), `sim/firms` (produkcja), `engine/ui` (panel łańcucha), `data/` (towary, receptury), `tools/` (walidator grafu).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md` — typy, determinizm, LOD, format danych i konwencje testów **nie są tu redefiniowane**.

---

## 1. Cel fazy i artefakt końcowy

Po M6 towar w mieście **istnieje fizycznie**: ma masę, objętość, miejsce, właściciela, datę przydatności i udokumentowane pochodzenie. Nic nie powstaje z niczego i nic nie znika bez zaksięgowanej straty. Sklepy przestają być nieskończonym źródłem — od tej fazy półka jest pusta, jeśli ciężarówka nie dojechała.

**Artefakt końcowy** — scenariusz headless `scenarios/m6_lancuch.ron`: miasto 40 tys. mieszkańców, 2 pola pszenicy, elewator, młyn, 3 piekarnie, złoże ropy z szybem, rurociąg, rafineria, terminal paliwowy, 4 stacje paliw, 1 centrum dystrybucyjne, 12 sklepów, 1 firma transportowa, 1 węzeł graniczny (kolej). Przebieg 2 lat gry w ≤ 8 minut zegarowych na headless runnerze.

Co widać po uruchomieniu:

1. **Panel łańcucha dostaw** (`engine/ui`): graf dostawców i odbiorców wybranej firmy z przepływami masy/miesiąc, oznaczeniem ryzyka (jeden dostawca = czerwona krawędź), listą kontraktów i pokryciem zapasu w godzinach.
2. **Śledzenie partii „od pola do półki"**: wybrany bochenek chleba na półce sklepu rozwija się w oś czasu z etapami (pole → elewator → młyn → piekarnia → sklep), z czasem, masą, jakością i **kosztem narastającym** na każdym etapie. To samo dla litra diesla w baku mieszkańca: aż do konkretnego złoża.
3. **Realny niedobór**: wyłączenie młyna na 5 dni widocznie kaskaduje — piekarnie zjadają bufor, ograniczają produkcję, szukają spotu, importują, sięgają po substytut, w końcu stają; ceny chleba w sklepach rosną; część mieszkańców kupuje substytut albo wraca z pustymi rękami. Po powrocie młyna system stabilizuje się bez narastającej oscylacji.
4. **Walidator grafu produktów jako test CI**: `cargo test -p goods-graph` — każdy towar w `data/goods/` ma źródło osiągalne ze złóż lub importu, każda receptura zachowuje masę.
5. **Zielone testy własnościowe**: bilans masy per towar w oknie 30 dni domyka się do zera gramów.

---

## 2. Zakres — wchodzi / nie wchodzi

### Wchodzi

| Obszar | Zakres |
|---|---|
| Katalog towarów | Struktura `Good`, ~60 kategorii potrzeb, proces napełniania w falach (fala A ~60 towarów w tej fazie), konwencja kluczy, walidator |
| Partia | `Batch` z pełnym zestawem właściwości §8.2 + pochodzenie (`BatchOrigin`, `BatchLedger`) |
| Receptury | `Recipe`: wejścia z jakością minimalną → proces → wyjścia + produkty uboczne + odpady; jawny, całkowitoliczbowy model jakości wyjścia |
| Zakład fizyczny | Linie/maszyny (wydajność, awaryjność, wiek, konserwacja, części), magazyny wejściowe i wyjściowe, warunki przechowywania, media z licznikami, rampy z przepustowością, emisje |
| Harmonogram | Zmiany 1–3, przezbrojenia, konserwacja planowa i awaryjna |
| Logistyka | `TransportOrder`, realizacja flotą własną lub wynajętą, magazyny i centra dystrybucyjne, konsolidacja (milk-run) |
| Polityki zapasów | min/max, JIT, bufor sezonowy; przegląd ciągły i okresowy |
| Rynek B2B | Spot (`Rfq`/`Quote`) i kontrakt (cena stała/indeksowana/widełkowa, kary, wypowiedzenie) |
| Import/eksport | `TradeNode` z przepustowością, cłem, czasem dostawy, wzrostem ceny przy dużych zakupach; eksport jako drenaż lokalnej podaży |
| Niedobory | Pełna kaskada §8.4 jako maszyna stanów z wyjaśnialnością |
| Wydobycie | Złoże z realnym wyczerpywaniem, rosnącym kosztem i spadającą koncentracją |
| UI | Panel łańcucha dostaw, tryb „śledź partię" |
| Migracja | Likwidacja tymczasowego „zewnętrznego dostawcy" sklepów z M5 |

### Nie wchodzi

| Obszar | Faza |
|---|---|
| Decyzje AI firm: budowa zakładu, ekspansja, wybór branży, bankructwo | M7 |
| HR, pensje, rotacja, menedżerowie i ich wpływ na produktywność | M7 (M6 konsumuje `skill` jako liczbę) |
| Wycena strategiczna, integracja pionowa, kartele, ekskluzywność | M7 / M10 |
| Symulacja jazdy pojazdu, pathfinding, korki, parkingi | M4 (M6 konsumuje `route_cost` i zdarzenia przybycia) |
| Podatki, akcyza, VAT, stawki celne jako polityka | M8 (M6 ma stub `TariffTable` z danych) |
| Sieci przesyłowe jako fizyka (blackout, przeciążenia) | M8 (M6 ma licznik i fakturę) |
| Strefy zakazu ruchu ciężkiego, godziny dostaw jako przepis miejski | M8 (M6 czyta z danych) |
| R&D, nowe produkty, cechy z technologii, epoki | M10 |
| Marka jako pamięć agentów (M6 tylko przenosi `BrandId` w partii) | M10 |
| Giełda, przejęcia, finansowanie inwestycji | M7 / M10 |
| Emisje jako wpływ na wartość gruntu i zdrowie | M8 (M6 publikuje liczby) |

---

## 3. Mapowanie na PRD

| Sekcja PRD | Pokrycie w tej fazie |
|---|---|
| §8.1 Drzewo produktów | WP1 (katalog, walidator), WP14 (łańcuchy paliwa i chleba jako testy E2E) |
| §8.2 Właściwości towaru | WP2 (`Batch`, `StorageCondition`, `HazardClass`, pochodzenie) |
| §8.3 Logistyka | WP5, WP12 (`TransportOrder`, floty, DC, rampy, milk-run) |
| §8.4 Niedobory i substytucja | WP6 (`ShortageCascade`) |
| §8.5 Import/eksport | WP9 (`TradeNode`) |
| §7.3 Zakład: model fizyczny | WP4 (linie, magazyny, media, rampa, emisje) |
| §7.4 Produkcja | WP3 (receptury, jakość, odpady), WP4 (harmonogram, przezbrojenia) |
| §6.2 pkt 2–3 Rynki hurtowy i zewnętrzny | WP7, WP8, WP9 |
| §6.3 Surowce pierwotne | WP10 (koszt wydobycia z parametrów złoża) |
| §6.9 Księgowość (wycena zapasów po koszcie) | WP2 (`cost_total`, FEFO, warstwowy koszt) |
| §9.5 Paliwo i energia w transporcie | WP5, WP14 (stacja ze zbiornikami, dostawy cysterną, `draw_fuel` dla M4) |
| §9.6 Media (zakład bez prądu stoi) | WP4 (licznik, `LineState::Broken{NoPower}`) |
| §14.3 Panel łańcucha dostaw | WP13 |
| §14.4 Śledzenie partii | WP13 (`trace_batch`) |
| §16.5 Edytor danych z walidacją grafu | WP1 (`tools/goods-graph`) |
| §17.4 LOD produkcji (per maszyna / linia / zakład) | WP4 (jedna funkcja `advance_production`, test spójności) |
| §17.7 Pamięć | WP15 (agregacja partii) |
| §19 M6 | całość |

---

## 4. Pakiety robocze i podfazy

Kolejność: WP1 → WP2 → WP3 → WP4 → (WP5 ∥ WP6) → WP7 → WP8 → (WP9 ∥ WP10) → WP11 → WP12 → WP13 → WP15. WP14 rośnie równolegle od WP2 i domyka fazę.

Faza jest rozbita na **5 podfaz**. Podfaza to porcja, którą da się zacząć i zamknąć
bez trzymania w głowie całej fazy: własny zestaw WP, własny sprawdzalny wynik i własny
wycinek projektu technicznego. Opis pakietów i sekcje §5 mieszkają teraz w dokumentach
podfaz — poniższa tabela mówi, gdzie co jest. Bramki 1–7 z `00-postep.md` zamykają się
dopiero po ostatniej podfazie; podfaza zamyka się własnym kryterium ze swojego dokumentu.

| Podfaza | WP | §5 | Wynik do pokazania | Dokument |
|---|---|---|---|---|
| **M6a — Katalog i partia** | WP1, WP2, WP3 | 5.1, 5.2, 5.3, 5.4 | `cargo test -p goods-graph` zielony razem z testem negatywnym odziedziczonym po M2; `prop_mass_conservation` na scenariuszu z samym magazynem. | `M6a-katalog-i-partia.md` |
| **M6b — Zakład i transport** | WP4, WP5, WP6 | 5.5, 5.6, 5.7 | Linia bez prądu stoi; partia zmienia lokację wyłącznie przez zlecenie transportowe. | `M6b-zaklad-i-transport.md` |
| **M6c — Rynek B2B** | WP7, WP8, WP9 | 5.8, 5.9 | Niedobór lokalny domyka się kontraktem albo importem, a nie znikającym zapotrzebowaniem. | `M6c-rynek-b2b.md` |
| **M6d — Złoża i koniec dostawcy zewnętrznego** | WP10, WP11, WP12 | 5.10 | Paliwo od złoża do baku bez ani jednego punktu, w którym towar bierze się znikąd; sklepy przestają być nieskończone. | `M6d-zloza-i-koniec-dostawcy-zewnetrznego.md` |
| **M6e — Panel, testy, pamięć** | WP13, WP14, WP15 | 5.11 | Pełny artefakt fazy z §1 dokumentu fazy: paliwo od złoża do baku, sklepy przestają być nieskończone. | `M6e-panel-testy-pamiec.md` |

---

## 5. Projekt techniczny

Treść przeniesiona do dokumentów podfaz. **Numeracja `5.x` jest zachowana**, więc
odesłania w tekście („patrz §5.4") nadal wskazują tę samą sekcję — zmienił się tylko plik.

| § | Temat | Dokument |
|---|---|---|
| 5.1 | Katalog towarów | `M6a-katalog-i-partia.md` |
| 5.2 | Partia | `M6a-katalog-i-partia.md` |
| 5.3 | Warunki przechowywania i klasa niebezpieczeństwa | `M6a-katalog-i-partia.md` |
| 5.4 | Receptury | `M6a-katalog-i-partia.md` |
| 5.5 | Zakład fizyczny | `M6b-zaklad-i-transport.md` |
| 5.6 | Logistyka | `M6b-zaklad-i-transport.md` |
| 5.7 | Polityki zapasów i kaskada niedoboru | `M6b-zaklad-i-transport.md` |
| 5.8 | Rynek B2B | `M6c-rynek-b2b.md` |
| 5.9 | Import, eksport, węzły graniczne | `M6c-rynek-b2b.md` |
| 5.10 | Wydobycie | `M6d-zloza-i-koniec-dostawcy-zewnetrznego.md` |
| 5.11 | Systemy ECS i częstotliwości | `M6e-panel-testy-pamiec.md` |

---

## 6. Kontrakty międzyfazowe

### 6.1 Dostarczam

```rust
// --- katalog ---
pub fn goods() -> &'static GoodCatalog;
pub fn recipes() -> &'static RecipeCatalog;
impl GoodCatalog {
    pub fn by_key(&self, key: &str) -> Option<GoodId>;
    pub fn get(&self, id: GoodId) -> &Good;
    pub fn in_category(&self, c: NeedCategoryId) -> &[GoodId];
    pub fn mass_of(&self, id: GoodId, qty: Qty) -> Mass;
    pub fn volume_of(&self, id: GoodId, mass: Mass) -> Volume;
}

// --- magazyn ---
pub fn stock_of(w: &World, site: SiteId, good: GoodId) -> Mass;
pub fn coverage_minutes(w: &World, site: SiteId, good: GoodId) -> Option<u32>;
pub fn reserve(w: &mut World, site: SiteId, good: GoodId, mass: Mass, min_q: Q) -> Option<Reservation>;
pub fn take(w: &mut World, r: Reservation) -> BatchSlice;      // FEFO; masa, jakość, koszt, marka
pub fn put(w: &mut World, site: SiteId, d: BatchDraft) -> Result<BatchId, StoreError>;
pub fn free_capacity(w: &World, site: SiteId, class: StorageClass) -> (Mass, Volume);

// --- detal (dla M5) ---
pub fn shelf_pick(w: &mut World, store: SiteId, good: GoodId, qty: Qty) -> Option<SoldUnits>;
pub fn backroom_to_shelf(w: &mut World, store: SiteId, good: GoodId, mass: Mass) -> Mass;
pub fn shelf_state(w: &World, store: SiteId, good: GoodId) -> ShelfState;  // masa, najstarsza data, jakość, marka

// --- paliwo (dla M4) ---
pub fn draw_fuel(w: &mut World, station: SiteId, good: GoodId, v: Volume) -> Option<FuelDraw>;
pub fn tank_level(w: &World, station: SiteId, good: GoodId) -> (Volume, Volume);   // (aktualnie, pojemność)

// --- transport ---
pub fn order_transport(w: &mut World, req: TransportRequest) -> TransportOrderId;
pub fn transport_quote(w: &World, from: SiteId, to: SiteId, mass: Mass,
                       req: VehicleRequirements) -> Option<TransportQuote>;
pub fn cancel_transport(w: &mut World, id: TransportOrderId) -> Result<(), TransportError>;

// --- B2B ---
pub fn open_rfq(w: &mut World, d: RfqDraft) -> RfqId;
pub fn submit_quote(w: &mut World, d: QuoteDraft) -> Result<QuoteId, QuoteError>;
pub fn sign_contract(w: &mut World, d: SupplyContractDraft) -> Result<ContractId, ContractError>;
pub fn terminate_contract(w: &mut World, id: ContractId, by: FirmId) -> Result<Money, ContractError>;
pub fn best_price_for(w: &World, good: GoodId, at: SiteId, mass: Mass) -> Option<Money>;

// --- import / eksport ---
pub fn import_quote(w: &World, good: GoodId, mass: Mass, to: SiteId) -> Option<ImportQuote>;
pub fn place_import(w: &mut World, q: ImportQuote, buyer: FirmId) -> Result<ContractId, TradeError>;
pub fn export_price(w: &World, good: GoodId, from: SiteId, mass: Mass) -> Option<Money>;

// --- produkcja ---
pub fn advance_production(w: &mut World, site: SiteId, minutes: u32);   // wspólna dla mikro/mezo/makro
pub fn line_state(w: &World, line: LineId) -> LineState;
pub fn site_emissions(w: &World, site: SiteId) -> Emissions;
pub fn site_utilities(w: &World, site: SiteId) -> &[UtilityMeter];

// --- rampa: kontrakt z M4 (§5.5). release_at wchodzi do księgi, musi być deterministyczne
//     i niezależne od LOD; strażnikiem jest test M4 `ramp_wait_lod_invariant`.
pub fn site_dwell(w: &mut World, ev: VehicleArrivedAtSite) -> SiteDwellResponse;

// --- wyjaśnialność i UI ---
pub fn trace_batch(w: &World, b: BatchId) -> BatchTrace;                // §14.4, koszt narastający
pub fn supply_graph(w: &World, firm: FirmId) -> SupplyGraphView;        // §14.3
pub fn shortage_stage(w: &World, site: SiteId, good: GoodId) -> ShortageStage;
```

Do funkcji haszującej stan (dok. 00 §3 pkt 6) wchodzą komponenty ECS: `StorageSlot`, `ProductionLine`, `ProductionSchedule`, `Dock`, `UtilityMeter`, `TransportOrder`, `SupplyContract`, `Rfq`, `Quote`, `TradeNode`, `MiningSite`, `InventoryRule`, `ShortageStage` — **oraz arena partii**, iterowana w kolejności indeksów (K-16, §5.2).

**`StreamId` — blok M6 to 200–219** (K-4; rezerwa 1200–1219, gdyby zabrakło). Przydział wstępny: 200 `SupplyBreakdown`, 201 `SupplySpoilage`, 202 `SupplyQuality`, 203 `SupplyQuoteNoise`, 204 `SupplyYield`, 205 `SupplyTransitDelay`, 206 `SupplyImportLead`. Wartości raz nadane nigdy się nie zmieniają.

**Słowniki wspólne mieszkają w `engine/core`, nie w `sim/supply` (K-8, K-12).** M6 **konsumuje i rozszerza**, nigdy nie definiuje od nowa: `UtilityKind` (prąd, gaz, woda, ciepło — mój `UtilityMeter` używa wariantu z core), `TransportMode` (mój `Carrier::Pipeline` i `BodyType` odwołują się do niego), `PlaceRef`, `NeedKind`, `ActivityKind`, `DecisionReason`. Warianty, które M6 dopisuje do wspólnych enumów:

- `DecisionReason`: `Shortage{..}`, `SupplierChosen{..}`, `ContractSigned{..}`, `ExportChosen{..}`, `SubstituteUsed{..}`, `ProductionHalted{..}`.
- `LossKind` (nowy enum, właściciel M6, ale mieszka w core, bo księguje go M5 i M7): `Drying`, `Evaporation`, `Spoilage`, `Expired`, `Spillage`, `ProcessWaste`, `Setup`, `TransportDamage`, `Theft`, `Storage`.

### 6.2 Konsumuję

| Faza | Co |
|---|---|
| M0 | `Mass`, `Volume`, `Qty`, `Energy`, `Money`, `Q`, `SimMinute`, `Tick`, `GoodId`, `RecipeId`, `BatchId`, `SiteId`, `FirmId`, `ContractId`, `OfferId`; RNG ze strumieniami; bufory komend; hash stanu; ładowarka RON |
| M1 | Złoża z generatora geologii (`world::geology::deposits`), klimat i sezon (krzywe plonów, `Seasonal`) |
| M2 | `SiteId` → parcela i budynek, powierzchnia m² → pojemność magazynu, lokalizacja rampy przy drodze |
| M3 | `JobRoleId`, dostępność obsady zmiany, `skill` (wejście do `QualityModel`), uprawnienia kierowcy (`Licence::C`, `Licence::ADR`) |
| M4 | `RouteQuery { from, to, total_mass, profile: Passenger \| HeavyDay \| HeavyNight } -> Option<Route>` — **ograniczenia przejazdu rozstrzygane przy PLANOWANIU, nie w trakcie**: trasa łamiąca tonaż mostu albo zakaz ruchu ciężkiego zwraca `None` już na etapie planowania zamówienia, a nie porażkę w połowie przejazdu. Dzięki temu `ReplenishmentSystem` wie od razu, czy dostawa jest w ogóle wykonalna, i może szukać innego dostawcy zamiast wysyłać ciężarówkę w ślepy zaułek. Dalej: czas, koszt i spalone paliwo z trasy, `traffic::dispatch(order) -> VehicleArrivedAtSite` |
| M5 | `NeedCategoryId`, `Offer`, `Transaction`, `economy::book(FirmId, LedgerEntry)`, polityka cenowa sklepu, `OutOfStock` |
| M7 | `FirmPersonality` i decyzje inwestycyjne — **w M6 stub**: jedna, neutralna polityka i brak budowy nowych zakładów |
| M8 | `TariffTable`, akcyza, strefy zakazu ruchu ciężkiego, godziny dostaw — **w M6 stub z plików danych** |
| M10 | `BrandId` — M6 tylko przenosi wartość w partii, nie interpretuje |

### 6.3 Migracja: koniec tymczasowego „zewnętrznego dostawcy" z M5

W M5 sklep miał `ShelfStock` napełniany przez stub `economy::supply_stub::refill_shelf()` — funkcję **tworzącą masę z niczego** po cenie `Good::external_base_price` przemnożonej przez stałą marżę. To był jedyny w projekcie sankcjonowany wyjątek od zasady zachowania masy. Ten punkt znika w M6 w siedmiu krokach:

1. **Sklep dostaje realną strukturę magazynową:** `Warehouse{role: Backroom}` (zaplecze) + `Warehouse{role: Shelf}` (półka) + `Dock` (rampa; dla sklepu osiedlowego `bays: 1, fixed_minutes: 10`) + `InventoryRule` per `GoodId` z polityką `Periodic` dobową.
2. **`refill_shelf` znika ze ścieżki produkcyjnej.** Zostaje za feature flagą `infinite_supply`, domyślnie **wyłączoną**, wyłącznie do izolowanych testów jednostkowych M5 i do scenariuszy balansatora badających samą warstwę detaliczną.
3. **Systemowy odpowiednik:** dotychczasowy `ReplenishShelf` (M5, EveryHour) przestaje tworzyć towar, a zaczyna **przenosić** masę zaplecze → półka przez `supply::backroom_to_shelf(store, good, mass) -> Mass` (zwraca faktycznie przeniesioną masę, może być mniejsza od żądanej).
4. **Zaplecze napełnia M6:** `ReplenishmentSystem` czyta `InventoryRule`, wystawia `TransportOrder` z DC, hurtowni albo wprost od producenta (wg `PreferredSource`), i to ta ciężarówka fizycznie przywozi towar na rampę.
5. **Sprzedaż:** M5 przestaje odejmować liczbę z `ShelfStock`. Woła `supply::shelf_pick(store, good, qty) -> Option<SoldUnits>`, gdzie `SoldUnits { mass, quality, brand, cost_total, expires_at }` — i to stąd transakcja bierze jakość i markę do oceny przez mieszkańca oraz koszt własny do księgi. `None` = pusta półka → istniejąca w M5 ścieżka `OutOfStock` (substytut / konkurencja / rezygnacja + pamięć „u nich nie było").
6. **Ceny:** `PriceRule` M5 dostaje realny `unit_cost` z partii zamiast stałej z katalogu, więc marża wreszcie liczy się od czegoś prawdziwego. Przecena towaru krótkoterminowego (§6.3 PRD) działa na `expires_at` partii, nie na licznik.
7. **Bilans masy:** od M6 test `prop_mass_conservation` jest włączony **globalnie**, bez listy wyjątków. Do M5 sklepowy stub był na tej liście; wykreślenie go jest formalnym kryterium ukończenia WP11.

**Test migracyjny (obowiązkowy, WP11).** Scenariusz „miasto 20 tys., 1 piekarnia, 3 sklepy", 90 dni gry, ten sam seed, dwa przebiegi: z `infinite_supply` i bez.

- Saldo sklepu różni się wyłącznie o zaksięgowany koszt transportu i o wartość towaru przeterminowanego (oba równe zeru w wariancie ze stubem) — żadna inna pozycja księgi nie może się rozjechać.
- Liczba klientów odchodzących bez zakupu rośnie ponad 0 tylko w dniach, w których dostawa faktycznie nie dotarła (korelacja 1:1 z `TransportOrderState::Failed` lub `ShortageStage::Halted`).
- Bilans masy chleba domyka się do 0 g.

### 6.4 Rozstrzygnięcia dla faz oczekujących

Cztery fazy zadały M6 pytania blokujące ich pakiety robocze. Odpowiedzi są wiążące.

#### 6.4.1 M2 — schemat katalogu i cykle w grafie

**Podział `data/recipes/` ↔ `data/buildings/` — zgoda, argument M2 i M11 jest słuszny.** Receptura zostaje **czysto ekonomiczna**. Wymagania lokalizacyjne zakładu (`SiteArchetypeId`, wymagana strefa, minimalna powierzchnia parceli, `m2_per_workplace`, obsada etatowa `JobRoleId` + `wage_band`) idą do `data/buildings/` przy archetypie. Jedna receptura może działać w kilku archetypach o różnych wymaganiach przestrzennych, więc wiązanie ich w recepturze i tak byłoby ciasne.

Dwa punkty styku, których nie wolno zlać, bo wyglądają podobnie i znaczą co innego:

- **`Recipe::machine_class: MachineClassId`** zostaje w recepturze. To **abstrakcyjna zdolność** („potrzebuję młyna walcowego"), nie obiekt fizyczny. Archetyp budynku deklaruje, ile slotów maszynowych której klasy mieści hala. Render czyta archetyp, nie recepturę.
- **`Recipe::labour: Vec<(JobRoleId, u32)>`** to **pracochłonność szarży w osobominutach** — wielkość ekonomiczna, wchodzi do kosztu wytworzenia. To **nie jest** obsada etatowa. Obsada (`wage_band`, liczba etatów na zmianę) należy do archetypu budynku i do M7. Te dwie liczby muszą się zgadzać w balansie, ale mieszkają osobno i mają osobnych właścicieli.

**Cykle w grafie — nie, M6 nie potrzebuje cyklu bez wejścia z importu lub wydobycia. Nie przechodź na rozwiązywanie punktu stałego; proste domknięcie Dowlinga–Galliera wystarczy i zostaje.** Twoja reguła „każdy cykl musi mieć co najmniej jedno wejście z importu lub z wydobycia" jest dokładnie tym, czego M6 chce, i M6 będzie jej bronić w danych. Cykle są w tym projekcie normalne i pożądane — rafineria potrzebuje prądu, elektrownia paliwa, huta potrzebuje maszyn, fabryka maszyn stali (§7.2) — ale każdy taki cykl daje się rozciąć importem albo złożem, i to jest warunek, który dane mają spełniać, a nie przeszkoda do obejścia w algorytmie.

Jedno **zaostrzenie** twojej reguły, o które proszę: **zapas startowy nie jest źródłem w grafie.** `data/scenarios/initial_stock.ron` pokrywa pierwsze ~14 dni świata (R2 w §8), ale walidator nie może go traktować jako punktu wejścia domknięcia. Inaczej przeszedłby w CI łańcuch, który raz wystartuje i nigdy się nie odtworzy po wyczerpaniu zapasu — czyli dokładnie ta klasa błędu, którą walidator ma łapać. Punkty wejścia to wyłącznie `RecipeSource::Extraction` / `Agriculture` oraz `Good::external_base_price.is_some()`.

**Flaga „pierwotna", której potrzebujesz, jest już w schemacie:** `Recipe::source: RecipeSource { Manufacturing, Extraction(ResourceKind), Agriculture }` (§5.4). Receptury `Extraction` i `Agriculture` są punktami wejścia domknięcia — ich wejścia materiałowe nie muszą być osiągalne, bo masa pochodzi ze złoża albo z gleby.

**Korekty do twojej listy „minimum do odczytu":**

| Ty prosiłeś | M6 daje | Uwaga |
|---|---|---|
| jednostka `Qty` / `Mass` / `Volume` | `GoodUnit { Grams, Milliunits }` (§5.1) | **`Volume` nigdy nie jest jednostką natywną** — jest pochodną masy przez `density_g_per_l`. Dwie jednostki, nie trzy. To pole jest publiczne i stabilne, czyta je też `sim/macro` |
| flaga importowalności | `external_base_price: Option<Money>` | `None` = nieimportowalny. Jedno pole zamiast flagi i ceny osobno — nie da się mieć niespójnego stanu „importowalny bez ceny" |
| epoki dostępności importu | odroczone do M10 | Nie dokładaj pola, którego nie umiesz wypełnić. M10 jest właścicielem epok i dopisze je do `TradeGood`, nie do `Good` |
| yield receptury per output | `RecipeOutput::mass` + `Recipe::batch_mass` + `duration_minutes` | Przepustowość liczy się z tych trzech: `daily_yield(output) = outputs[o].mass * 1440 / duration_minutes`. Nie trzymam yieldu jako osobnego pola, bo rozjechałby się z masami — a to jest dokładnie ta liczba, którą walidator sprawdza w regule bilansu masy |

**`schema_version` per plik** — tak, zgodnie z dok. 00 §5. Katalog rośnie plikami (~60 plików towarów), a nie jednym blokiem; wspólna wersja zmuszałaby do bumpowania wszystkiego przy dodaniu jednej kategorii.

#### 6.4.2 M9 — głębokość `BatchProvenance`

Panel „od pola do półki" (PRD §14.4) **jest wykonalny w pełnej głębokości**, ale nie dla każdej partii — i to jest świadomy kompromis, nie ograniczenie implementacji.

- **Partie `TRACED`** (wszystko, co przechodzi przez firmę gracza, partie objęte aferą z §11, partie wskazane ręcznie w UI) mają **pełny łańcuch** w `BatchLedger`: `BatchTrace { stages: Vec<TraceStage> }` z czasem, miejscem, masą, jakością i kosztem narastającym na każdym etapie. Typowa głębokość to **5–8 etapów**, twardy limit **16** (`BatchOrigin::depth: u8`). Łańcuch paliwa kończy się na `DepositId`, łańcuch chleba na polu. To jest ten panel, który obiecuje PRD.
- **Partie nieoznaczone** mają tylko poziom lekki: `BatchOrigin { site, recipe, depth, deposit }` — 16 B. Odpowiadają na „skąd to jest" na poziomie zakładu i złoża, ale nie na „którędy szło".

Powód jest arytmetyczny: pełne drzewo dla 600 tys. aktywnych partii (§7.4) to setki MB ledgera rosnące przez 100 lat gry. Flaga `TRACED` płaci koszt śledzenia tylko tam, gdzie ktoś patrzy.

**Trzecie ograniczenie, o którym M9 musi wiedzieć, bo rysuje ten panel:** po przejściu przez LOD makro ślad **rwie się i nie da się go odtworzyć**. M10 potwierdził, że `lower()` nie odtwarza partii, tylko generuje je na nowo z agregatu komórki. Trace kończy się wtedy na `provenance: FromAggregate { district, last_supplier }`. **UI ma to powiedzieć wprost — „odtworzone z agregatu dzielnicy" — a nie domalować wiarygodny łańcuch do złoża.** M6 i M10 są w tej sprawie zgodni: przyznanie się do braku danych jest tańsze i uczciwsze niż fabrykowanie, a gracz, który raz przyłapie panel na zmyślaniu, przestanie mu wierzyć także tam, gdzie panel ma rację.

#### 6.4.3 M11 — skala `activity`, `emission`, `stock_fill`

Wszystkie trzy są polami `u8` 0..255 w `SiteRenderRec`, liczonymi po stronie M6 i wystawianymi jednokierunkowo przez snapshot.

**1. `activity` — obłożenie, normalizowane do własnej nominalnej wydajności zakładu.**

```
activity = 255 * Σ(line.nominal_throughput × line.utilization) / Σ(line.nominal_throughput)
```

Twoje założenie jest poprawne i **nie potrzebujesz dodatkowego bitu na ciszę**: `activity == 0` znaczy „nic się nie rusza", a nie „brak danych". Wkład stanów linii: `Running` → pełne obłożenie, `Setup` (przezbrojenie) → **0,4** (maszyna pracuje, ale nie produkuje — i słychać ją), `Idle` / `Starved` / `Blocked` / `Broken` / `Maintenance` → **0**. Stojąca linia daje ciszę, zgodnie z PRD §15.5.

Natomiast **awaria to nie to samo co bezczynność** i wizualnie powinny się różnić, więc zamiast psuć skalę `activity` dołożę osobny bit w `flags`: `SITE_FAULT` (jest `Broken` albo `cut_off` na liczniku mediów). Zakład w awarii jest cichy, ale powinien dać się odróżnić od zakładu, który po prostu nie ma zmiany — to jest informacja, po którą gracz sięga.

**Układ bajtu `flags` w `SiteRenderRec` (uzgodniony z M11, rekord zostaje 24 B — M11 zużył na to bajt `_pad`):**

| Bity | Pole | Źródło po stronie M6 |
|---|---|---|
| 0 | `SITE_FAULT` | dowolna linia w `Broken` albo `UtilityMeter::cut_off` |
| 1–2 | `smoke_kind` | `PlumeKind` aktualnie uruchomionej receptury |
| 3–7 | rezerwa | — |

`smoke_kind` to `PlumeKind { None = 0, Steam = 1, Soot = 2, Chemical = 3 }` z §5.4. Kluczowa decyzja: **jest deklarowany w `data/recipes/`, a nie wyliczany ze stosunku `pm_g` do `co2_g`.** Czy z komina leci biała para, czy ciemna sadza, wynika z tego, co się w środku dzieje, i nie da się tego wiarygodnie odgadnąć z dwóch liczb — chłodnia kominowa i kotłownia węglowa potrafią mieć zbliżone `co2_g` przy zupełnie różnym pióropuszu. Wartość bierze się z receptury **aktualnie uruchomionej** na linii; gdy zakład stoi, z domyślnej receptury archetypu, żeby komin nie zmieniał barwy przy każdym przezbrojeniu.

M11 wystarcza jedna liczba na wybór emitera cząstek i tempo rozpraszania — osobne pole `u8` z `co2_g` dałoby precyzję, której render i tak nie pokaże, więc go nie wystawiam.

**2. `emission` — skala absolutna, wspólna, z saturacją. Twoja preferencja jest właściwa.**

```
emission = min(255, 255 * pm_g_per_min / EMISSION_REF_PM_G_PER_MIN)
```

Normalizacja per typ zakładu jest odrzucona z dokładnie tego powodu, który podałeś: mała kotłownia na pełnej mocy dymiłaby jak huta, a mapa zanieczyszczeń kłamałaby w najgorszym możliwym miejscu — tam, gdzie gracz podejmuje decyzję, gdzie postawić dom.

Jedno doprecyzowanie względem twojej propozycji: **stała odniesienia jest stała, a nie „najbrudniejszy zakład epoki"**. Ruchomy mianownik znaczyłby, że ten sam komin dymi inaczej po wejściu nowej epoki, a dwa zrzuty ekranu z różnych lat przestają być porównywalne. `EMISSION_REF_PM_G_PER_MIN` siedzi w `data/tuning/supply.ron` (nie w kodzie), kalibrowane balansatorem na najbrudniejszy zakład epoki przemysłowej. Zakłady czystsze saturują się nisko — i to jest prawda o nich, nie wada skali.

**3. `stock_fill` — jedna liczba, wąskie gardło pojemności.**

```
stock_fill = 255 * max(used_volume/cap_volume, used_mass/cap_mass)
```

agregowane po wszystkich slotach zakładu **z wyłączeniem `WarehouseRole::Shelf`** (półka ma własny widok w panelu sklepu; mieszanie jej z zapleczem dałoby regały, które wyglądają na pełne, gdy sklepowi kończy się towar — czyli odwrotnie niż chcesz).

`max` z dwóch stosunków, a nie sam wolumen, bo wąskie gardło jest różne dla różnych towarów: skrzynki z chipsami wypełniają objętość przy śmiesznej masie, a silos zboża odwrotnie. Interesuje cię „jak pełno tu wygląda", a to jest ten z dwóch limitów, który akurat gryzie.

#### 6.4.4 M5 — `trait Wholesale` i `ExternalSupplier`

Potwierdzone: **M6 podmienia implementację, nie interfejs.** `trait Wholesale` zostaje dokładnie taki, jaki zostawiłeś; `ExternalSupplier` (tworzący masę z niczego) przestaje być domyślną implementacją i ląduje za feature flagą `infinite_supply`, a jego miejsce zajmuje implementacja oparta na `sim/supply`. Pełny opis migracji w siedmiu krokach wraz z testem porównawczym jest w §6.3. Jeśli którakolwiek sygnatura w `trait Wholesale` okaże się niewystarczająca (podejrzewam, że zabraknie zwrotu jakości i marki przy sprzedaży — `SoldUnits` z §6.1 to pokrywa), zgłoszę to jako rozszerzenie traitu, nie jako jego przepisanie.

#### 6.4.5 M10 — `SupplierRelation`

**M6 nie ma własnego typu relacji dostawcy i nie zamierza go tworzyć.** `SupplyContract` (§5.8) trzyma wyłącznie twarde fakty realizacji: `fulfilled_mass`, `missed_mass`, `late_deliveries`. Relacja — zaufanie, rabat stałego klienta, priorytet w niedoborze, ekskluzywność — to PRD §7.9, czyli zakres M7/M10, i M6 jawnie wypisał to w tabeli „nie wchodzi" (§2).

Więc `SupplierRelation` jest twój (albo M7, do ustalenia między wami) — M6 go **konsumuje** w dwóch konkretnych miejscach i tylko te dwa pola go obchodzą:

- **`discount_bps`** → korekta `Quote::price` przy rozstrzyganiu RFQ (§5.8),
- **`priority`** → kolejność przydziału masy, gdy dostawca nie ma dość towaru dla wszystkich odbiorców; to jest mechanizm „stały dostawca daje priorytet w niedoborze" z §7.9 i wpływa wprost na to, kto pierwszy wejdzie w kaskadę `ShortageStage`.

Reszta pól (`trust`, `since`, `volume_cum`, `exclusivity`) M6 nie czyta. Dopóki typu nie ma, RFQ liczy się bez rabatu i bez priorytetu — degradacja jest łagodna i nie blokuje M6.

**Pułapka, którą M10 zgłosił i którą trzeba mieć w kodzie od pierwszej wersji:** `trust` podnoszony jest także w fazie 7 kroku makro, czyli **podczas historii „na sucho"** — stamtąd biorą się „stali dostawcy" wymagani przez PRD §4.2 Etap 9. Znaczy to, że **świat startowy ma gotowe `discount_bps` i `priority` na relacjach, których kontrakty nigdy nie istniały jako encje w ECS**. Relacje są starsze niż jakikolwiek `SupplyContract`. Kod RFQ nie może więc zakładać, że istnienie relacji implikuje istnienie kontraktu ani że da się z relacji dojść do `ContractId` — to jest dokładnie ten rodzaj cichego założenia, które działa w każdym teście jednostkowym i wywraca się przy pierwszym wczytaniu wygenerowanego świata.

Odwrotnie też: `trust` karze za dostawy spóźnione **ponad `grace_minutes`**, a nie za spóźnione w ogóle. To dwie różne liczby, które przez większość czasu wyglądają tak samo, więc rozjazd ujawniłby się dopiero przy pierwszej karze umownej i wyglądał na problem z balansem, a nie z arytmetyką. M6 wystawia obie i nie pozwala liczyć ich osobno po stronie M10.

---

## 7. Testy i kryteria akceptacji

### 7.1 Łańcuch referencyjny A — chleb (§8.1)

Wszystkie liczby są **wartościami startowymi do kalibracji balansatorem**, nie prawdami objawionymi. Test E2E sprawdza je z tolerancją ±2% dla kosztów i **±0 g dla mas**.

**Etap 1 — pole (50 ha pszenicy, zbiór 1 sierpnia, 6 dni).**

| Pozycja | Ilość | Uwaga |
|---|---|---|
| Nasiona | 9 000 kg | 180 kg/ha, siew jesienny |
| Nawóz | 12 500 kg | 250 kg/ha, w trzech dawkach |
| Diesel | 4 500 l = 3 762 kg | 90 l/ha, ciągnik + kombajn |
| Praca | 400 osobogodzin | w tym 260 h sezonowych w żniwa |
| **Wyjście** | **275 000 kg zboża** | plon 5,5 t/ha, jakość q64 (zależna od pogody z M8) |

Zboże po zbiorze ma wilgotność 16%, `StorageClass::Silo`, `shelf_life` 18 miesięcy przy zachowanych warunkach.

**Etap 2 — transport pole → elewator (18 km).** Wywrotka 24 t, 12 kursów. Czas: 45 min jazdy w jedną stronę + 25 min rozładunku na rampie elewatora (2 stanowiska) → 14 h przy jednym pojeździe, 3,5 h przy czterech. Koszt 4,20 zł/km × 36 km × 12 = **1 814 zł** = 6,60 zł/t.

**Etap 3 — elewator.** Suszenie 16% → 14%: **strata masy 2,3%** → 275 000 → 268 675 kg, energia 32 kWh/t = 8 592 kWh. Przechowanie 12 miesięcy: ubytek 0,5% → **267 332 kg**. Straty księgowane jako `LossKind::Drying` i `LossKind::Storage` — legalne w bilansie, bo zaksięgowane. Cena zbytu do młyna: **780 zł/t** loco elewator, kontrakt `Indexed` do `raw_wheat`.

**Etap 4 — transport elewator → młyn (22 km).** 24 t, koszt 4,20 zł/km × 44 km = 185 zł / 24 t = **7,70 zł/t**. Wsad w młynie: 787,70 zł/t.

**Etap 5 — młyn, receptura `milling_wheat_t550`.**

| | Towar | Masa | Jakość |
|---|---|---|---|
| Wejście | `raw_wheat` | 1 000 kg | min q60 |
| Wyjście (Main) | `food_flour_t550` | 760 kg | q = f(...) |
| Wyjście (ByProduct) | `feed_bran` | 220 kg | — |
| Wyjście (Waste) | `waste_grain_screenings` | 20 kg | — |
| **Suma** | | **1 000 kg** | ✅ |

Proces: szarża 40 min, 55 kWh, 36 osobominut (1 operator na 4 linie), maszyna `mill_roller` 3 t/h, przezbrojenie na mąkę żytnią 50 min + 30 kg strat.

`QualityModel { base: 50, w_input: 40, w_skill: 20, w_tech: 15, w_machine: 10, cap_by_worst_input: true, bonus_qc: 3 }`. Przy zbożu q64, obsadzie skill 60, tech 50, maszynie condition 85: `q_raw = 50 + (40*64 + 20*60 + 15*50 + 10*85)/100 = 50 + 47 = 97` → cap: `min(97, 64+3) = 67`. **Mąka q67** — kluczowe, że dobra mąka nie powstanie ze słabego zboża.

Koszt: wsad 787,70 + energia 55 kWh × 0,65 = 35,75 + praca 22,80 + amortyzacja i media stałe 18,00 = **864,25 zł/t wsadu**. Odliczenie produktu ubocznego: 220 kg otrąb × 0,28 zł/kg = 61,60 zł (sprzedane wytwórni pasz — osobny łańcuch). Koszt przypisany 760 kg mąki: 802,65 zł → **1,056 zł/kg**. Młyn sprzedaje z marżą 18% → **1,246 zł/kg**, kontrakt `Indexed{index: raw_wheat, pass_through_pct: 85}`.

**Etap 6 — transport młyn → piekarnia (11 km).** 5 000 kg co 3 dni, furgonetka 3,5 t → 2 kursy, 22 min jazdy + 15 min rozładunku. Koszt 2 × 22 km × 2,10 = **92 zł** / 5 t = 0,018 zł/kg. Mąka loco piekarnia: **1,264 zł/kg**.

**Etap 7 — piekarnia, receptura `bakery_bread_wheat`.**

| | Towar | Masa |
|---|---|---|
| Wejście | `food_flour_t550` (min q55) | 100 kg |
| Wejście | `util_water` | 62 l = 62 kg |
| Wejście | `food_yeast` | 2 kg |
| Wejście | `food_salt` | 1,8 kg |
| Wyjście (Main) | `food_bread_wheat` | 158 kg ≈ 198 bochenków × 800 g |
| Strata | `LossKind::Evaporation` (wypiek) | 7,8 kg |
| **Suma** | | **165,8 kg = 165,8 kg** ✅ |

Proces: szarża **190 min** (mieszanie 20, fermentacja 90, wypiek 45, studzenie 35), 18 kWh, 84 osobominuty, maszyna `bakery_oven_deck`. Przezbrojenie na chleb żytni: 35 min + 4 kg strat. Chleb: `expires_at = produced_at + 2 880 min` (2 doby), `StorageClass::Ambient`, jakość ~**q72** przy mące q67 i obsadzie skill 65.

Koszt szarży: mąka 126,40 + drożdże 13,00 + sól 2,16 + woda 0,56 + prąd 11,70 + praca 44,80 + stałe (najem, amortyzacja pieca) 22,00 = **220,62 zł** na 158 kg = 1,396 zł/kg = **1,117 zł/bochenek**. Piekarnia sprzedaje sklepowi z marżą 30% → **1,452 zł/bochenek**.

**Etap 8 — transport piekarnia → sklep osiedlowy (6 km), milk-run po 6 sklepach.** Wyjazd 04:40, chłodnia niepotrzebna (`Ambient`), 150 bochenków (120 kg) na sklep. Trasa 38 km, 74 min jazdy + 6 × 8 min rampy = 122 min, koszt 38 × 2,10 = 80 zł / 6 sklepów = **13,30 zł/sklep** = 0,089 zł/bochenek. Chleb loco sklep: **1,54 zł**.

**Etap 9 — półka.** FEFO. Cena detaliczna = 1,54 × marża sklepu 28% × VAT 5% (M8) = **2,07 zł**. Od 36. godziny życia `SpoilageSystem` oznacza partię flagą `MARKDOWN` (przecena wg polityki M5), o 48. godzinie usuwa jako `LossKind::Expired` — **bochenek po dacie nigdy nie trafia do transakcji**, czego pilnuje `prop_no_expired_on_shelf`.

**Podsumowanie łańcucha A:** 9 etapów, 5 firm, czas od siewu do półki ~340 dni (z czego 11 dni od młyna). Narastający koszt na bochenek 800 g: 0,52 → 0,53 → 0,63 (mąka) → 0,64 → 1,12 (wypiek) → 1,45 → 1,54 → **2,07 zł**. Masa: 664 g zboża → 505 g mąki → 800 g chleba (różnicę robi woda). Test E2E sprawdza wszystkie te liczby i domyka bilans masy każdego z siedmiu towarów do 0 g.

### 7.2 Łańcuch referencyjny B — paliwo (§8.1, §9.5)

**Etap 1 — złoże i szyb.** `Deposit { good: raw_crude_oil, initial: 2 400 000 t, concentration_pct: 82, depth_m: 1 800 }`. Szyb: 320 t/dobę przy 100%, 120 kWh/t, 4 osobogodziny/dobę, `spare_part: part_pump_seal` (awaria co ~1 400 h pracy). Koszt startowy **1 620 zł/t**, cena zbytu 1 850 zł/t (`Indexed` do ceny importowej ropy). Po zużyciu 60% złoża koszt rośnie do ~2 480 zł/t — to samo z siebie czyni import opłacalnym i ostatecznie zamyka szyb.

**Etap 2 — rurociąg szyb → rafineria (42 km).** `Carrier::Pipeline`, przepustowość 900 t/dobę, czas przepływu 6 h, koszt **12 zł/t**, awaria co ~9 miesięcy na 3–14 dni (wtedy cysterny kolejowe, 5× drożej — realny test kaskady). Rurociąg nie zajmuje rampy ani kierowcy.

**Etap 3 — rafineria, receptura `refinery_crude_fractionation`.**

| | Towar | Masa | Rodzaj |
|---|---|---|---|
| Wejście | `raw_crude_oil` (min q55) | 1 000 kg | |
| Wyjście | `fuel_petrol_95` | 260 kg | Main |
| Wyjście | `fuel_diesel_b7` | 340 kg | Main |
| Wyjście | `fuel_lpg` | 45 kg | Main |
| Wyjście | `fuel_heating_oil` | 160 kg | Main |
| Wyjście | `mat_bitumen` | 95 kg | ByProduct |
| Wyjście | `chem_lubricant_base` | 40 kg | ByProduct |
| Wyjście | `fuel_refinery_gas` | 42 kg | SelfConsumed (spalany na miejscu) |
| Wyjście | `waste_refinery_residue` | 18 kg | Waste (utylizacja 340 zł/t) |
| **Suma** | | **1 000 kg** | ✅ |

Proces: instalacja ciągła modelowana jako szarża 60 min o wsadzie 12 t (288 t/dobę), 145 kWh/t, 15 osobominut/t, `machine_class: refinery_cdu`. Przezbrojenie sezonowe (zmiana proporcji benzyna/olej opałowy, lato↔zima): **8 h + 40 t strat** — dwa razy w roku, dobrze widoczne w kosztach. `power_draw` 4 200 kW; `cut_off` licznika prądu → `LineState::Broken{NoPower}` i cała rafineria stoi (§9.6).

`CostAllocation::ByMarketValue` — obowiązkowo. Przy alokacji wg masy asfalt kosztowałby tyle co benzyna i cały rynek materiałów budowlanych by się rozjechał. Diesel przy 34% masy bierze **41% kosztu**.

Koszt: wsad 1 850 + rurociąg 12 = 1 862 zł/t; przerób 145 kWh × 0,65 = 94,25 + praca 8,00 + stałe 65,00 = 167,25; razem **2 029,25 zł/t wsadu**. Diesel: 41% × 2 029,25 = 831,99 zł na 340 kg = 2,447 zł/kg. Gęstość 836 g/l → **2,046 zł/l** koszt wytworzenia. Marża rafinerii 12% → **2,29 zł/l**.

**Etap 4 — kolej rafineria → terminal paliwowy (28 km).** Cysterna kolejowa 60 t, 2 kursy/dobę, koszt **28 zł/t** = 0,023 zł/l. Terminal: 2,32 zł/l.

**Etap 5 — terminal paliwowy.** Zbiorniki `StorageClass::Tank`, `HazardClass::Flammable`, pojemność 4 000 t (ok. 4,8 mln l). Rampa nalewcza: `bays: 6, fixed_minutes: 12, minutes_per_tonne: 1` → 38 min na cysternę 26 t, przepustowość 9 cystern/h. Marża terminalu 4% → **2,41 zł/l**.

**Etap 6 — cysterna drogowa terminal → stacja (14 km).** 26 t = 31 100 l diesla. Wymagania: `BodyType::Tanker`, `adr: true` → **kierowca z uprawnieniami ADR** (M3 dostarcza; brak kierowcy = `FailReason::NoDriver`, realny problem kadrowy). 32 min jazdy + 40 min rozładunku. Koszt 6,80 zł/km × 28 km = 190 zł / 26 t = 7,30 zł/t = 0,0061 zł/l → **2,42 zł/l loco stacja**.

**Etap 7 — stacja paliw.** Zbiorniki: diesel 30 000 l, benzyna 2 × 25 000 l. `InventoryPolicy::MinMax { reorder_point: 25%, target: 92% }`, lead 3–8 h. Sprzedaż 9 000 l diesla/dobę → pokrycie 3,3 doby przy pełnym zbiorniku, 0,8 doby w punkcie zamówienia. Marża stacji 9% → 2,64 zł/l netto; + akcyza 1,16 + opłata paliwowa 0,33 (M8) + VAT 23% → **cena na dystrybutorze 6,55 zł/l**.

**Etap 8 — bak.** M4 woła `supply::draw_fuel(station, fuel_diesel_b7, volume)`. Zwraca `FuelDraw { mass, cost, batch }`. Jeśli zbiornik pusty → `None`, mieszkaniec jedzie na inną stację (decyzja i pamięć w M3/M4). `trace_batch` na tym `batch` prowadzi przez stację → cysternę → terminal → kolej → rafinerię → rurociąg → szyb → **`DepositId` konkretnego złoża**.

**Zależności zwrotne, które ten łańcuch domyka (i których pilnuje walidator grafu):** rafineria potrzebuje prądu (elektrownia potrzebuje węgla lub gazu), części zamiennych (fabryka maszyn potrzebuje stali) i chemikaliów (zakład chemiczny potrzebuje ropy). Cysterny palą diesel z tej samej rafinerii. Ten cykl jest **dozwolony** — walidator wymaga jedynie, by dało się go uruchomić z zapasu startowego lub importu, i to sprawdza algorytmem punktu stałego z WP1.

### 7.3 Testy własnościowe

Wszystkie na `proptest` plus scenariusze headless, w CI.

1. **`prop_mass_conservation`** — dla każdego `GoodId` w oknie 30 dni gry:
   `Σ produced + Σ imported + stock_start == Σ consumed + Σ exported + Σ losses + stock_end`, **tolerancja 0 g**. Straty muszą być sklasyfikowane (`LossKind`) — masa „znikająca" bez kategorii to błąd testu, nie zaokrąglenie. Uwaga metodologiczna: receptury **dodają** masę (woda w chlebie), dlatego bilans jest liczony per towar, a wewnętrzna spójność receptury sprawdzana jest statycznie przy ładowaniu danych (`Σ inputs == Σ outputs + process_loss`).
2. **`prop_no_negative_stock`** — po każdym punkcie synchronizacji: `0 <= slot.used_mass <= slot.cap_mass`, `0 <= slot.used_volume <= slot.cap_volume`, `batch.mass > 0` (partia o masie 0 jest usuwana w tym samym ticku, nie zostaje jako duch), `deposit.remaining >= 0`.
3. **`prop_no_teleport`** — partia nigdy nie zmienia lokacji bez zlecenia. W trybie testowym `AuditSystem` loguje każdą zmianę `Batch::location` jako `(batch, from, to, order: Option<TransportOrderId>)`. Warunek: `order.is_some() || site_of(from) == site_of(to)`. Ruch wewnątrz zakładu (magazyn → linia → magazyn wyjściowy, zaplecze → półka) jest dozwolony bez zlecenia; wszystko inne wymaga ciężarówki, pociągu albo rurociągu.
4. **`prop_no_expired_on_shelf`** — dla każdego ticku: żaden `Batch` w `WarehouseRole::Shelf` nie ma `expires_at < now`, i żadna transakcja M5 nie odwołuje się do partii przeterminowanej. Gwarantowane kolejnością w DAG systemów: `SpoilageSystem` przed `RetailSystem` — **ta kolejność jest kontraktem międzyfazowym**, nie przypadkiem (decyzja D12).
5. **`prop_cost_vs_mass`** — `Σ batch.cost_total` we wszystkich magazynach równa się `Σ` zapłacone za zakupy i wytworzenie − `Σ` zaksięgowany COGS − `Σ` odpisy strat. Wiąże bilans masy z bilansem pieniądza z dok. 00 §6 i wykrywa gubienie groszy przy podziale partii.
6. **`prop_dock_capacity`** — rampa nigdy nie obsługuje więcej pojazdów niż `bays`; suma czasów obsługi równa sumie czasów zajętości stanowisk; kolejka rozładowuje się wyłącznie w `OpeningHours`; pojazdy ponad `yard_capacity` trafiają na krawędź dostępową, a nie znikają. Test siostrzany po stronie M4 — **`ramp_wait_lod_invariant`** (300 ciężarówek, 12 ramp, mikro vs mezo, tolerancja 0 na `release_at`, paliwo postojowe i koszt) — jest twardym strażnikiem tego, że `SiteDwellResponse` nie zależy od LOD. Obie strony muszą być zielone; to nasz wspólny test, nie mój i nie M4.
7. **`prop_deposit_monotone`** — `remaining` nigdy nie rośnie, koszt wydobycia nigdy nie maleje wraz z wyczerpaniem, `concentration_eff` monotonicznie nierosnąca.
8. **`prop_contract_penalty`** — suma kar naliczonych równa sumie zapłaconych; cena `Collar` zawsze w `[floor, cap]`; `fulfilled_mass + missed_mass` równe masie wynikającej z harmonogramu.

### 7.4 Wydajność

**Skala docelowa (metropolia 400 tys. mieszkańców, PRD §17.7, M12):**

| Wielkość | Szacunek | Twardy limit |
|---|---|---|
| Zakłady (`SiteId`) z magazynem | ~9 000 (4 500 sklepów, 2 000 usług z zapasem, 800 produkcyjnych, 150 logistycznych, 90 stacji paliw, reszta) | — |
| Linie produkcyjne | ~2 400 | — |
| **Aktywne partie** | **≤ 600 000** | 1 000 000 (tryb awaryjny) |
| Aktywne zlecenia transportowe | 4 000 – 8 000 | 20 000 |
| Nowe zlecenia na dobę gry | ~12 000 (≈ 8 na tick) | — |
| Aktywne kontrakty | ~25 000 | — |
| Otwarte RFQ jednocześnie | ~600 | — |
| Pojazdy dostawcze w obiegu w szczycie | ~1 500, z czego **realnie ~900 na sieci** (pojazd na rampie nie jest w ruchu) | potwierdzone przez M4 |
| Zapytania o trasę B2B | ~6 000 przejazdów/dobę ≈ 15 zapytań/min | osobny budżet 0,3 ms po stronie M4 |

**Skąd 600 tys. partii.** 4 500 sklepów × ~80 partii po agregacji (300 pozycji asortymentu, ale większość scala się do jednej partii na pozycję) = 360k; 800 zakładów × (200 wejście + 100 wyjście) = 80k; 150 magazynów i DC × 600 = 90k; w transporcie ~25k; zbiorniki i silosy ~5k; bufor na partie śledzone ~40k. Gospodarstwa domowe **nie trzymają partii** — konsumpcja jest natychmiastowa (kontrakt z M5); to jedna decyzja, która oszczędza ~170 tys. encji.

**Pamięć.** `Batch` w układzie SoA: `good` u16, `mass`/`qty`/`volume` 3×i64, `quality` u8, `brand` u16, `producer` u32, `produced_at`/`expires_at` 2×u32 (u32 minut = 8 171 lat, wystarczy), `cost_total` i64, `flags` u8, `location` u64, `origin` 16 B ≈ **88 B**. 600 000 × 88 B = **53 MB**. Mieści się w budżecie 6 GB z §17.7.

**Strategia agregacji partii — bez niej byłyby miliony encji.** `BatchCoalesceSystem` (EveryDay, staggered) scala partie w tym samym slocie, gdy zgadza się **klucz agregacji**: `(good, quality / 5, brand, producer, expires_at / 1440)` — jakość kubełkowana co 5 punktów, data przydatności co dobę. Scalenie sumuje `mass`, `qty`, `volume` i `cost_total` (pieniądz dokładny, bez zaokrągleń), `quality` bierze jako średnią ważoną masą, `expires_at` jako **minimum** (konserwatywnie — nigdy nie przedłużamy przydatności), `origin.depth` jako maksimum, a `origin.recipe` / `origin.site` zachowuje, jeśli wspólne, w przeciwnym razie zeruje. Partie z flagą `TRACED` nie są scalane nigdy.

Efekt: sklep osiedlowy z 300 pozycjami asortymentu trzyma 300–600 partii zamiast 5 000+; magazyn hurtowni 600 zamiast 40 000.

**Tryb awaryjny przy przekroczeniu twardego limitu:** dla zakładów firm AI poza kadrem klucz agregacji redukuje się do `(good, quality / 20, expires_at / 10080)` — traci się markę i tygodniową precyzję daty, zyskuje rząd wielkości. Przełączenie jest deterministyczne (próg na liczbie partii) i odwracalne przy zejściu poniżej progu.

**Budżet czasu** (8 rdzeni, tick ekonomiczny = 1 minuta gry):

| Częstotliwość | Budżet `sim/supply` | Rozbicie |
|---|---|---|
| EveryMinute | **2,0 ms** p99 | produkcja 2 400 linii 0,5 ms; rampy 1 200 aktywnych kolejek 0,2 ms; przybycia (zdarzenia, nie skan) 0,2 ms; psucie (kopiec, amortyzowane) 0,1 ms; systemy godzinowe i dobowe rozproszone po minutach 0,7 ms; rezerwa 0,3 ms |
| EveryHour (zsumowane, przed rozproszeniem) | 12 ms | uzupełnianie 9 000 zakładów, kaskada niedoboru, RFQ, dostawy kontraktowe, dyspozycja transportu |
| EveryDay (zsumowane) | 40 ms | konserwacja, import/eksport, złoża, scalanie partii |
| EveryMonth | 25 ms | faktury za media |

Kluczowa technika: **rozpraszanie po indeksie encji** (`i % 60`, `i % 1440`) zamiast przeliczania wszystkiego w jednym ticku. Deterministyczne, bo indeks encji jest stabilny i nie zależy od zegara.

Benchmarki `criterion`: `bench_production_2400_lines`, `bench_dock_queue_1200`, `bench_replenish_9000_sites`, `bench_rfq_600_open`, `bench_coalesce_600k_batches`, `bench_trace_batch_depth12`.

### 7.5 Determinizm

- Komponenty M6 dopisane do funkcji haszującej stan ECS (lista w §6.1) — element Definition of Done fazy.
- 2 przebiegi × 200 000 ticków scenariusza `m6_lancuch` → identyczny ciąg hashy co 1 000 ticków.
- Zero iteracji po `HashMap`/`HashSet` w `sim/supply` — wymuszone lintem i przeglądem kodu. Kolejki ramp, listy ofert, listy partii w slocie: `Vec` / `VecDeque` sortowane po jawnym kluczu z tie-breakiem na `entity_index`.
- Losowość wyłącznie przez `rng(world_seed, StreamId::Supply*, entity_index, tick)`: awarie maszyn, szum plonu, drobny szum wyceny ofert.

### 7.6 Spójność LOD

Wszystkie trzy poziomy (mikro: per maszyna, mezo: per linia, makro: per zakład) wołają tę samą `advance_production(site, minutes)`; różnią się wyłącznie granulacją wywołania i tym, czy powstają wizualne encje. Test akceptacji: ten sam scenariusz 90-dniowy w mikro i w mezo → **identyczne salda pieniężne i identyczne masy, tolerancja 0** (dok. 00 §4).

Zastrzeżenie dla poziomu makro: tolerancja 0 obowiązuje wyłącznie na parze mikro↔mezo. Na parze mezo↔makro dok. 00 §4 dopuszcza odchylenie agregatów ≤ 0,5% miesięcznie i **tylko dla agregatów dzielnicowych i wyżej** — dla pojedynczego zakładu odchylenie rzędu kilku procent jest własnością rozkładu, nie błędem implementacji. `advance_production` jest w makro wywoływana z grubszym krokiem i na uśrednionych wejściach; M6 nie obiecuje na tym poziomie zgodności co do grama.

### 7.7 Scenariusze akceptacyjne

| Scenariusz | Kryterium |
|---|---|
| `e2e_bread` | 9 etapów, liczby z §7.1, bilans masy 7 towarów = 0 g, `trace_batch` ≥ 5 etapów |
| `e2e_fuel` | 8 etapów, liczby z §7.2, `trace_batch` ≥ 6 etapów kończących się `DepositId` |
| `shortage_mill_down_5d` | Kaskada przechodzi Buffer → Throttled → SpotSearch → Importing → Substituted → Halted w tej kolejności; ceny chleba +15…40%; **antybullwhip**: po powrocie młyna amplituda drugiego szczytu zamówień < 50% pierwszego i powrót do `Ok` w ≤ 7 dni |
| `dc_beats_direct` | 10 sklepów + DC ma niższy koszt dostawy na tonę niż 10 sklepów zaopatrywanych bezpośrednio — bez reguły to wymuszającej |
| `import_not_free` | Zakup 10× dobowej przepustowości węzła → kolejka i wzrost ceny, nie natychmiastowa dostawa |
| `export_drains` | Wzrost ceny zewnętrznej o 40% → mierzalny odpływ masy i wzrost cen lokalnych, bez zaprogramowanej reguły. **Zawężone na stałe w `AS-1` (R2f)**: scenariusz istnieje i mierzy, a wywożona masa jest zerem, bo żaden zakład nie trzyma wyrobu w slocie wyjściowym |
| `deposit_50y` | Złoże wyczerpane w 50 lat, koszt rośnie monotonicznie, zakład zamknięty, kaskada w dół łańcucha |
| `m5_migration` | Opis w §6.3 |
| `perf_400k` | `sim/supply` ≤ 2,0 ms/tick p99, ≤ 600 tys. partii, ≤ 64 MB |
| `determinism_200k` | Identyczne hashe |

---

## 8. Ryzyka fazy i mitygacje

| # | Ryzyko | Mitygacja |
|---|---|---|
| R1 | **Eksplozja liczby partii** — naiwna implementacja da 5–10 mln encji i rozjedzie pamięć oraz scheduler | Agregacja z WP15 jako część projektu, nie optymalizacja „na później"; twardy limit z trybem awaryjnym; GD bez partii; benchmark w CI od WP2 |
| R2 | **Zakleszczenie bootstrapu grafu** — rafineria potrzebuje prądu, elektrownia paliwa, świat nie startuje | Zapas startowy w `data/scenarios/initial_stock.ron` pokrywający 14 dni konsumpcji + import jako zawór; walidator WP1 (algorytm punktu stałego) wykrywa nierozwiązywalne cykle **w CI**, a nie przy pierwszym uruchomieniu |
| R3 | **Efekt byczego bicza (bullwhip)** — polityki min/max wzmacniają wahania i produkują oscylacje zamiast równowagi | Wygładzanie prognozy zużycia (średnia ruchoma 7 dni, nie ostatnia doba); przegląd okresowy zamiast ciągłego u detalistów; test antybullwhip w `shortage_mill_down_5d` jako kryterium akceptacji |
| R4 | **Zbyt tani import zabija lokalną produkcję** — deflacja i „miasto-magazyn" | Przepustowość węzła + elastyczność ceny (§5.9); rozszerzenie balansatora M5 o metrykę „udział importu w podaży < 40% po 5 latach" |
| R5 | **Niedeterminizm w kolejkach i aukcjach** — najłatwiejsze miejsce na przypadkowy `HashMap` | Zakaz z dok. 00 §3 egzekwowany lintem; wszystkie sortowania z jawnym tie-breakiem na `entity_index`; test hashy jako bramka |
| R6 | **Sztywne łańcuchy bez substytutów** — pierwsza awaria zabija miasto | Walidator ostrzega o krytycznym wejściu bez substytutu w kategoriach żywnościowych i energetycznych; reguła „≥ 3 towary na kategorię potrzeby" w fali B |
| R7 | **Koszt CPU planowania tras** — pokusa pełnego VRP | Heurystyka zachłanna z limitem 12 punktów; trasa jest jedną funkcją do podmiany, gdyby pomiar kiedyś tego wymagał |
| R8 | **Rozjazd LOD** (produkcja per maszyna vs per zakład) | Jedna funkcja `advance_production` dla wszystkich poziomów; test spójności z tolerancją 0 |
| R9 | **Model psucia się w transporcie** rozrasta się w symulację termodynamiki | Dwa stany: warunki spełnione / niespełnione, mnożnik ×8. Żadnych krzywych temperatury. Gdyby kiedyś zabrakło — to jeden mnożnik do zamiany na tablicę |
| R10 | **Alokacja kosztu produkcji łącznej** źle dobrana wywraca ceny całych branż (asfalt droższy od benzyny) | `CostAllocation::ByMarketValue` obowiązkowe dla receptur wieloproduktowych; test `e2e_fuel` sprawdza rozkład kosztu; decyzja otwarta D3 o źródle cen referencyjnych |
| R11 | **Obciążenie M4 pojazdami dostawczymi** (~1 500 w szczycie) nie zmieści się w budżecie M4 | Uzgodnienie liczby z M4 przed WP5 (decyzja D11); plan B: dostawy poza kadrem obsługiwane w mezo z czasem z tabeli dzielnicowej, bez encji pojazdu |
| R12 | **`BatchLedger` rośnie bez ograniczeń** przez 100 lat gry | Ledger wyłącznie dla partii `TRACED`; retencja i moment odcięcia — decyzja D8 |

---

## 9. Decyzje otwarte

Uwaga: dokumenty faz M0–M5 i M7–M12 powstawały równolegle z tym, więc poniższe uzgodnienia nie zostały jeszcze potwierdzone przez właścicieli tamtych faz. Każdy wiersz zawiera propozycję M6 — do zatwierdzenia albo odrzucenia przed startem fazy.

| # | Decyzja | Kontekst i propozycja | Z kim |
|---|---|---|---|
| **D1** | Budżet ms/tick dla `sim/supply` | Proponuję 2,0 ms EveryMinute, 12 ms EveryHour, 40 ms EveryDay przy 8 rdzeniach. Wymaga zatwierdzenia w globalnym budżecie ticku | M0, M12 |
| ~~D2~~ | ~~Czy `Batch` jest encją ECS~~ | **ROZSTRZYGNIĘTE (K-16, dok. 00 §2).** `BatchId` przestaje być `Entity`; partie dostają dedykowaną arenę z uchwytem `{index: u32, generation: NonZeroU32}`, `Arena<T>` dostarcza M0. Cztery warunki nienegocjowalne (hash w kolejności indeksów, brak reanimacji uchwytu, snapshot jak sekcja ECS, stabilność indeksów w obrębie ticku) opisane w §5.2 | zamknięte |
| ~~D3~~ | ~~Źródło cen referencyjnych do `CostAllocation::ByMarketValue`~~ | **ZAMKNIĘTE w M6c (`AI-6`), zgodnie z propozycją i z rozróżnieniem, którego propozycja nie miała.** Alokacja kosztu bierze **statyczne** `external_base_price` — musi być stabilna, bo inaczej udział diesla w koszcie przerobu zmieniałby się co dobę i rachunek rafinerii przestałby być porównywalny z samym sobą. Indeksacja kontraktu bierze natomiast **kroczącą średnią siedmiodobową z rynku spot**, ważoną masą — bo po to się kontrakt indeksowany podpisuje. Sprzężenia koszt→cena→koszt, którego bała się propozycja, nie ma: indeks wchodzi do **ceny**, nie do kosztu wytworzenia | zamknięte |
| **D4** | Właściciel `Dock` i decyzji o rozbudowie rampy | Proponuję: dane i kolejka w `sim/supply`, decyzja inwestycyjna w `sim/firms` (M7) | M7 |
| **D5** | `HazardClass` a przepisy miejskie (ADR przez centrum, godziny dostaw) | M6 czyta z `data/`, M8 podmienia na politykę miasta. Trzeba uzgodnić kształt `OpeningHours` i stref, żeby M8 nie musiał przepisywać `Dock` | M8 |
| ~~D6~~ | ~~Czy eksport zajmuje realne pojazdy i rampę~~ | **ZAMKNIĘTE w M6c (`AI-5`, `AH-8`), zgodnie z propozycją: tak.** `try_export` wystawia i wysyła `TransportOrder` do węzła, a masa schodzi z bilansu jako `exported` dopiero po rozładunku w porcie (`B2b::absorb_exports`) — dwa kroki, bo między decyzją a wyjazdem stoi przejazd, który może się nie udać, a towar, który nie dojechał, ma wrócić do podaży. Koszt CPU nadal do zmierzenia w `perf_400k` (M6e); plan B z propozycji — eksport w mezo — zostaje wykonalny, bo podział na decyzję i wchłonięcie jest już w kodzie | zamknięte |
| **D7** | Właściciel pliku kalibracji progów kaskady (8 h / 4 h / 2 h) | To kalibracja, nie projekt. `tools/balansator` (M5) czy `data/tuning/`? Proponuję `data/tuning/supply.ron` pod kontrolą balansatora | M5 |
| **D8** | Retencja `BatchLedger` i moment odcięcia historii partii | Wpływa na kroniki (M9) i budżet pamięci oraz dysku (M11, M12). Proponuję: pełna historia partii `TRACED` do 2 lat gry, potem kompaktowanie do 5 etapów kluczowych | M9, M12 |
| **D9** | Czy woda wodociągowa jest towarem z partiami | Proponuję: **medium licznikowe** (`UtilityMeter`), partia tylko dla wody butelkowanej. Wymaga potwierdzenia, bo M8 buduje sieci przesyłowe | M8 |
| **D10** | `Qty` w łańcuchu dostaw jako wielokrotność 1000 (całe sztuki) | Konflikt, jeśli M5 sprzedaje ułamek opakowania (np. na wagę). Propozycja: sprzedaż na wagę to towar `Bulk`, nie ułamek sztuki | M5 |
| ~~D11~~ | ~~Limit pojazdów dostawczych jednocześnie w ruchu~~ | **ZAMKNIĘTE.** 1 500 mieści się bez osobnej ścieżki — M4 odmówił drugiego modelu ruchu dla ciężarówek (drugi model to drugie miejsce, w którym pęka tolerancja 0 mikro↔mezo) i różnicuje je wyłącznie klasą pojazdu oraz profilem trasy. Szczyt 12 000 → 13 500 pojazdów, mezo 6,0 → 6,8 ms. Pojazd na rampie nie jest w ruchu, więc realnie ~900 na sieci. Kontrakt rampy (`VehicleArrivedAtSite` → `SiteDwellResponse`) z czterema warunkami brzegowymi w §5.5 | zamknięte |
| **D12** | Kolejność `SpoilageSystem` przed `RetailSystem` w DAG systemów | To kontrakt, od którego zależy `prop_no_expired_on_shelf`. Wymaga jawnego zapisu w grafie zależności systemów po stronie M5 | M5 |
| **D13** | Kto emituje `TransportOrder` dla dostaw do gospodarstw domowych (e-commerce, dowóz) | Poza zakresem M6 (brak e-commerce do M10), ale API `order_transport` powinno to udźwignąć bez zmiany kontraktu | M10 |
| ~~D14~~ | ~~Podział własności katalogu `data/goods/` z M2~~ | **ZAMKNIĘTE.** M2 przyjął schemat z §5.1 i §5.4 w całości wraz z czterema korektami i zaostrzeniem o zapasie startowym (§6.4.1); bierze nazwy stąd i zgłosi uwagę, zamiast obchodzić po swojemu. Minimalny katalog M2 (~60 towarów, ~45 receptur epoki startowej) ładuje się pod docelowym schematem bez migracji | zamknięte |

---

## 10. Szacunek wielkości

| WP | Zakres | Rozmiar |
|---|---|---|
| WP1 | Katalog towarów, format danych, walidator grafu w CI, fala A (~60 towarów) | **M** |
| WP2 | Partia, magazyn, warunki przechowywania, FEFO, wycena, psucie, pochodzenie | **L** |
| WP3 | Receptury, model jakości, produkty uboczne, odpady, alokacja kosztu | **M** |
| WP4 | Zakład fizyczny: linie, awarie, harmonogram, przezbrojenia, media, rampa, emisje | **XL** |
| WP5 | Zlecenia transportowe, floty, przewoźnicy, rurociąg, integracja z M4 | **L** |
| WP6 | Polityki zapasów, kaskada niedoboru, substytucja | **M** |
| WP7 | Rynek B2B spot: RFQ, oferty, rozstrzyganie | **M** |
| WP8 | Kontrakty: cennik indeksowany i widełkowy, harmonogram, kary | **M** |
| WP9 | Import/eksport, węzły graniczne, cła (stub), drenaż podaży | **M** |
| WP10 | Wydobycie, wyczerpywanie złóż, koszt z parametrów złoża | **S** |
| WP11 | Migracja M5 — koniec nieskończonych sklepów | **M** |
| WP12 | Magazyny, centra dystrybucyjne, konsolidacja milk-run | **M** |
| WP13 | Panel łańcucha dostaw, śledzenie partii, nakładka przepływów | **M** |
| WP14 | Testy E2E (chleb, paliwo), własnościowe, wydajnościowe, determinizm | **L** |
| WP15 | Agregacja partii, budżet pamięci, tryb awaryjny | **M** |

Rozkład: 1 × XL, 3 × L, 10 × M, 1 × S. Największe skupisko ryzyka to WP4 (model fizyczny zakładu) i WP2 (partia) — te dwa warto zamknąć i obłożyć testami, zanim ruszy cokolwiek powyżej. WP14 rośnie równolegle od WP2 i jest bramką zamykającą fazę.

---

## Zmiany wpisane po M5e

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu fazy M5;
M6 nie jest przy okazji przeprojektowywana. Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| AC-1 ★ | **`ExternalSupplier` ma od M5e stan, którego plan nie zakładał: mnożnik szoku ceny hurtowej** (`ExternalSupplier::set_shock(good, factor_bp)`, dostępny przez `Market::set_supply_shock`). WP11 („koniec nieskończonych sklepów") podmienia dostawcę na rynek B2B i **musi zachować ten punkt wejścia**, inaczej scenariusz `supply-shock` balansatora przestaje mieć czym szokować | Bramka **G4** (reaktywność szoku podaży: widoczny w 2–7 dni, wygaszony w 14–56) jest jedną z dziewięciu bramek CI i jest **jedyną**, która potrzebuje zdarzenia zewnętrznego. Po podmianie dostawcy szok przestaje być mnożnikiem na cenniku, a staje się zdarzeniem na rynku B2B — ale **nazwa wejścia ma zostać**, żeby balansator nie musiał wiedzieć, która faza akurat stoi pod spodem |
| AC-2 ★ | **Most „zakłady Etapu 7 → rynek" mieszka w `magnat_headless::retail`**, nie w scenariuszu, i ma trzech konsumentów: `headless m5shop`, klient graficzny i balansator. M6, stawiając zakłady produkcyjne, dokłada się **do niego**, a nie obok | Trzy kopie tej wiedzy rozjeżdżają się przy pierwszej zmianie, a rozjazd widać dopiero jako inny wynik bramki — czyli w miejscu, w którym najtrudniej go powiązać z przyczyną. Nazwa crate'u jest długiem przypisanym do R1 (patrz tabela po M5e w `R1-refaktor-po-M5.md`); M6 ma używać tego, co zastanie, i nie zakładać drugiego mostu |
| AC-3 | **Decyzja otwarta nr 5 fazy M5 (migracja wyceny WAC → FIFO) zostaje otwarta i jest teraz konkretniejsza.** Wycena średnią ważoną siedzi w **jednej** funkcji rdzenia: `magnat_economy::kernel::take_cogs`, z gałęzią „zmiatania reszty" chroniącą niezmiennik P5. Propozycja M5 pozostaje: wariant (a) — przełącznik od daty wejścia M6, linie historyczne dojeżdżają na średniej | Punkt był opisany jako „do uzgodnienia z M6" i nadal nim jest, ale adres jest już znany co do funkcji, a nie co do modułu. Ważniejsze: **`take_cogs` jest wołane także przez `Market::balance_sample`** (mediana marży dla bramki G3), więc zmiana wyceny przestawi bramkę razem z raportami — i to jest efekt do przewidzenia, a nie do odkrycia |
| AC-4 | **Katalog detaliczny M5 ma 18 towarów i `GoodTable::key_of` jako drogę powrotną `GoodId → klucz`.** Przeszukanie jest **liniowe** i to jest świadomy sufit nazwany w kodzie | M6 podnosi katalog do rzędu 400 pozycji. Przy tej wielkości `key_of` wołane w pętli po ofertach przestaje być darmowe, a wołającym jest panel sklepu (M5e) i wszystko, co po nim przyjdzie. Właściwą odpowiedzią jest odwrotny indeks w `GoodTable`, **nie** cache po stronie wołającego — ten rozjeżdża się przy przeładowaniu danych |
| AC-5 | **`Offer` ma od M5 pole `price_rev`, a zmiana ceny nie brudzi indeksu przestrzennego.** M6, dokładając `NetB2B`, dokłada wariant `PriceBasis`, a nie drugi indeks | Zapisane, bo to jest ta klasa optymalizacji, którą łatwo cofnąć przez nieuwagę: indeks trzyma uchwyty, a cena czyta się z areny na żywo. Drugi indeks „dla hurtu" kosztowałby przebudowę przy każdej przecenie |

---

## Zmiany wpisane po M6a

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu podfazy M6a;
reszta fazy nie jest przy okazji przeprojektowywana. Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| AD-1 ★ | **`Good::category` to `NeedCategoryId` z nowej tabeli `data/needs/categories.ron`, której właścicielem jest M6** — a nie „~60 kategorii potrzeb z M5" (§5.1, §6.2) | M5 takich kategorii nie stworzył. To, co M5 ma, to `StockCat`: **osiem** wariantów w `core::vocab`, indeksujących `Household.stock: [u8; 8]`. To jest gruby podział spiżarni, a nie odpowiedź na pytanie „czym to podmienić" z PRD §8.4. Kategoria potrzeby jest drobna i wiąże się z `StockCat` przez pole `stock_cat` **w danych**, nie w kodzie (`K-34`). Kategorie przemysłowe mają `None` i nie udają, że gospodarstwo je trzyma |
| AD-2 ★ | **`GoodUnit` przestaje być polem danych i jest pochodną `GoodForm`** (`GoodForm::unit()`) | `§5.1` trzymał oba obok siebie, przez co dawało się zadeklarować „sypki, liczony w sztukach" i walidator musiał tego pilnować regułą 4. Wyprowadzenie jednostki z postaci sprawia, że ten stan **nie istnieje**. Zobowiązanie wobec M10 z §5.1 („pole publiczne i stabilne, czyta je `lift()`") stoi bez zmian: metoda jest równie publiczna i równie stabilna, a `lift()` nie przelicza niczego |
| AD-3 ★ | **`Good::name: LocKey` nie powstaje.** Nazwę widzianą przez gracza daje klucz: `ui.good.<key>` | `LocKey` mieszka w `engine/ui`, a rozwiązanie klucza wymaga katalogu lokalizacji, czyli realnej zależności `sim/supply → engine/ui`. Żaden crate `sim/*` od UI nie zależy i zależeć nie może. M5 już robi to przez klucz i nie potrzebował pola; M6 idzie za nim, zamiast zakładać drugą konwencję (CLAUDE.md, „spójność między fazami") |
| AD-4 ★ | **`Good::tariff_class` i `Good::pack` nie powstają w M6a.** `tariff_class` dokłada **M6c** razem z `TradeNode`, `pack` — **M6d** razem z WP11 | Zero konsumentów w M6a, a pole bez konsumenta to zgadywanie kształtu. Przy `pack` zgadywanie jest szczególnie kosztowne: §5.1 nie definiuje `RetailPack` ani jednym zdaniem, a wie, czego potrzebuje, dopiero ten, kto podłącza półkę sklepu do magazynu — czyli WP11 |
| AD-5 ★ | **`Recipe::labour` i `Recipe::machine_class` zostają kluczami tekstowymi**, a nie `Vec<(JobRoleId, u32)>` i `MachineClassId` (§5.4) | `JobRoleId` nadaje katalog etatów, który mieszka po stronie miasta (`data/jobs/`, M2/M7) — `sim/supply` nie ma jak go rozwiązać, nie zależąc od `sim/world`. `MachineClassId` indeksowałby katalog klas maszyn, który powstaje dopiero w **M6b** razem z linią produkcyjną. Rozwiązanie obu kluczy należy do M6b, gdzie jest i obsada, i maszyna |
| AD-6 ★ | **`Recipe` dostaje `process_loss: Mass` razem z `loss_kind: LossKind`** | §5.4 nie miał pola ubytku procesowego w ogóle, a §7.3 pkt 1 wymaga, żeby **każda** masa schodząca z bilansu miała kategorię: „masa znikająca bez kategorii to błąd testu, nie zaokrąglenie". Chleb traci 7,8 kg na `Evaporation`, a przemiał nie traci nic — jego 20 kg to wyjście `Waste`, nie ubytek. Bez tego pola nie dało się tego odróżnić |
| AD-7 ★ | **Arena partii i sloty magazynowe mieszkają w jednym zasobie `Store`**, a nie jako `Arena<Batch>`-zasób plus komponent ECS `StorageSlot` (§6.1) | `World::resource_mut` pożycza **cały** świat, więc komponentu i zasobu nie da się zmutować naraz — a każda operacja magazynowa (`put`, `take`, `spoil`, `merge`) dotyka i slotu, i partii. `K-29` dopuszcza to wprost i nie wymaga nowego rozstrzygnięcia: arena wchodzi do hasha przez `Arena::hash_state` (kolejność indeksów, sloty i generacje), a `Store` przez `register_resource_hash`. Sekcja hasha zmienia się z „areny wg `ArenaKind`" na „zasoby wg nazwy typu". `ArenaKind::Batches` zostaje zarezerwowane i niezmienne. **Konsekwencja dla M6b:** `ProductionLine`, `Dock` i `UtilityMeter` mogą być komponentami, ale muszą sięgać do partii przez uchwyty `SlotId`, a nie przez własną arenę |
| AD-8 | **Rachunek pośredni w §7.1 (etap 5) jest arytmetycznie błędny, wynik końcowy nie.** `50 + (40·64 + 20·60 + 15·50 + 10·85)/100` to `50 + 53,6 = 103,6`, a nie `50 + 47 = 97` | Liczba w nawiasie to 5 360, nie 4 700. Wynik końcowy — **mąka q67** — jest poprawny, bo sufit najgorszego wejścia (`min(q_out, 64 + 3)`) przycina jedno i drugie tak samo. Zapisane, bo ktoś, kto będzie kalibrował `QualityModel` balansatorem, odtworzy ten rachunek i będzie szukał błędu u siebie. Test `mlyn_daje_q67_bo_sufit_wejscia_przycina` pilnuje wyniku, nie rachunku pośredniego |
| AD-9 ★ | **Walidator dostaje dwie reguły błędu ponad listę z §4 WP1:** wagi `QualityModel` muszą sumować się do ≤ 100, a `CostAllocation::ByMarketValue` wymaga `external_base_price` dla **każdego** wyjścia biorącego koszt | Pierwsza: zakład o średniej obsadzie i średniej maszynie wypuszczałby towar lepszy od wszystkiego, co do niego weszło — i nie byłoby tego widać inaczej niż jako dziwna inflacja jakości przez dziesięć lat gry. Druga: podział wg wartości bez ceny odniesienia jest **niezdefiniowany**, a nie „domyślny wg masy" — cicha zamiana trybu dałaby asfalt w cenie benzyny, czyli dokładnie to, przed czym §5.4 ostrzega |
| AD-10 | **`AC-4` jest nieaktualne: `key_of` nie potrzebuje odwrotnego indeksu.** `GoodId` **jest** indeksem w `Catalog::goods`, więc droga powrotna `GoodId → klucz` jest w czasie stałym | M5 miał przeszukanie liniowe, bo jego `GoodTable` trzymała klucze w `BTreeMap`. Katalog M6 trzyma towary w wektorze posortowanym po kluczu i nadaje `GoodId` jako indeks w tym wektorze — ta sama decyzja, która daje stabilność identyfikatorów, daje i tani powrót. `GoodTable` po stronie M5 zostaje bez zmian aż do WP11 |
| AD-11 | **`cargo test -p goods-graph` istnieje: crate `tools/goods-graph` z walidatorem i CLI.** `supply_closure_check` po stronie M2 zostaje bez zmian i dalej woła `Catalog::reachable` | Walidator M2 pytał o **miasto** (receptury zainstancjonowane, bramy, bilans przepustowości), a reguła z dok. 00 §5 pyta o **katalog**. To są dwa różne pytania i oba są potrzebne; M6a rozdzielił je na dwa miejsca, zamiast rozbudowywać jedno. Kolejność reguł 1–5 i treść testu negatywnego odziedziczonego po M2 nie uległy osłabieniu |
| AD-12 ★ | **Klucze towarów przeszły na konwencję `<domena>_<nazwa>[_<wariant>]` z zamkniętą listą dwunastu domen** (`raw_ agri_ food_ feed_ fuel_ mat_ chem_ part_ pack_ util_ waste_ cons_`), pilnowaną testem | §5.1 podawał konwencję, ale katalog M2 jej nie trzymał (`bread`, `flour`, `crude_oil`). Mieszanka dwóch konwencji zostałaby na zawsze, bo `key` idzie do zapisu gry. Przemianowanie przy 70 towarach kosztuje jeden commit; przy 400 kosztowałoby migrację zapisów. Lista domen jest **zamknięta** z rozmysłu: nowa domena ma być decyzją, a nie literówką w kluczu |

---

## Zmiany wpisane po M6b

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu podfazy M6b;
reszta fazy nie jest przy okazji przeprojektowywana. Gwiazdka = zmiana zakresu albo kryterium.
Poprawki dotyczące samego §5.5–§5.7 są w tabeli `AF` dokumentu `M6b-zaklad-i-transport.md`;
tutaj jest to, co dotyczy **fazy**: kontraktów z §6, decyzji z §9 i testów z §7.

| # | Zmiana | Dlaczego |
|---|---|---|
| AG-1 ★ | **§6.2, wiersz M4: cały ten kontrakt jest do zbudowania, nie do skonsumowania.** Po stronie M4 nie ma `traffic::dispatch(order) -> VehicleArrivedAtSite`, nie ma ciężarówek w `data/vehicles/classes.ron`, nie ma `BodyType`, nie ma `Licence::C` ani `Licence::ADR`. `RouteQuery` istnieje i ma profile `HeavyDay`/`HeavyNight`, ale nazywa pola `origin`/`dest`/`gross_mass` i `TrafficOracle` woła go wyłącznie na profilu `Passenger`. M6b wstawił w to miejsce trait `FreightOracle` z atrapą; **właścicielem podłączenia prawdziwego routera jest M6d** (WP11 i WP12 pierwsze potrzebują prawdziwych kilometrów) | M4 dowiózł ruch pasażerski i to jest zgodne z jego zakresem — ładunki nigdy nie były jego pakietem roboczym, a §6.2 opisywał kontrakt **uzgodniony**, nie zastany. Zapisane tutaj, a nie tylko w dokumencie podfazy, bo to jest wiersz w tabeli „konsumuję" i ktoś, kto ją czyta przed startem M6c albo M6d, ma prawo sądzić, że ten kontrakt stoi. Cena jest konkretna i dotyczy dwóch scenariuszy akceptacyjnych z §7.7: `e2e_fuel` (cysterna wymaga `BodyType::Tanker` i kierowcy ADR — „brak kierowcy = `FailReason::NoDriver`, realny problem kadrowy") oraz `dc_beats_direct` (porównanie kosztu na tonę wymaga prawdziwych odległości, bo przy stawce ryczałtowej centrum dystrybucyjne wygrywa albo przegrywa z arytmetyki, a nie z geografii) |
| AG-2 ★ | **§6.1: `ProductionLine`, `ProductionSchedule`, `Dock` i `UtilityMeter` nie są komponentami ECS.** Lista komponentów wchodzących do funkcji haszującej zmienia się: te cztery wchodzą przez zasób `Plant` (`register_resource_hash`), tak samo jak partie i sloty przez `Store` (`AD-7`). `TransportOrder` — analogicznie, przez zasób `Transport` | Rozszerzenie `AD-7` na zakład. `World::resource_mut` pożycza cały świat, a `advance_production` w każdej minucie dotyka i linii, i areny partii; komponentu z zasobem nie da się zmutować naraz. Rozbicie tego na dwa systemy z buforem komend znaczyłoby rozstanie się z **jedną** funkcją produkcji dla trzech poziomów LOD, czyli z mitygacją ryzyka `R8` i z kryterium §7.6. `K-29` dopuszcza zasób wprost i nie wymaga nowego rozstrzygnięcia |
| AG-3 ★ | **`UtilityMeter::kind` to `UtilityService`, nie `UtilityKind`** (§6.1 mówi „mój `UtilityMeter` używa wariantu z core" i wskazuje zły wariant) | `UtilityKind` w `core::vocab` jest wymiarem oceny **kupującego** z funkcji użyteczności M5 (`Price`, `Quality`, `Distance`, `Convenience`…). Rodzaj mediów to `UtilityService` (`Electricity`, `Water`, `Sewage`, `Gas`, `Heat`, `Waste`, `Internet`) i on istnieje w `core` od M5. Podstawienie zgodne z literą §6.1 dałoby licznik prądu o jednostce „wygoda" |
| AG-4 | **Decyzja otwarta `D7` zamknięta zgodnie z propozycją:** progi kaskady i reszta kalibracji łańcucha siedzą w `data/tuning/supply.ron` pod kontrolą balansatora. Katalog `data/tuning/` wchodzi do §5 dokumentu 00 jako **`K-35`** | Lista katalogów w 00 §5 deklaruje się jako kompletna, więc nowy katalog wymaga własnego `K-n`, a nie samego dopisania wiersza. Przy okazji rozstrzygnięty zakres: `data/tuning/` jest **wspólny dla faz** i trzyma liczby, które wolno przestawić bez zmiany znaczenia modelu; parametry zmieniające kształt rozwiązania zostają w katalogach dziedzinowych, a `data/economy/` należy do M5 i nie ma być workiem na cudze liczby |
| AG-5 ★ | **Decyzja otwarta `D9` zamknięta zgodnie z propozycją, ale z jednym doprecyzowaniem, którego propozycja nie miała: woda jest medium licznikowym w **dostawie**, a nie w **bilansie masy**.** `Recipe::water_ml` zasila `UtilityMeter{Water}`, a masa tej wody wchodzi do `Recipe::input_mass()` i do reguły 3 walidatora katalogu | Propozycja `D9` brzmiała „medium licznikowe, partia tylko dla wody butelkowanej" i jest słuszna: piekarni nikt nie dowozi wody ciężarówką. Ale woda **waży** i to jej masa wychodzi z pieca jako chleb — 62 kg ze 165,8 kg szarży. Gdyby licznikowość wyjmowała ją z bilansu, reguła `Σ wejść == Σ wyjść + ubytek` przestałaby domykać się dla dwudziestu jeden receptur, a `piekarnia.input_mass() == 165_800` z testu łańcucha chleba stałoby się nieprawdą. To jest dokładnie ta klasa różnicy, którą łatwo przeoczyć, bo obie strony brzmią jak to samo zdanie |
| AG-6 ★ | **`R6` („sztywne łańcuchy bez substytutów") jest zmaterializowane, nie zmitygowane: fala A ma dokładnie jeden substytut w całym katalogu**, dopisany w M6b (`feed_bran` zamiast `food_flour_t550` w piekarni). Reguła „≥ 3 towary na kategorię potrzeby" nie jest spełniona dla **żadnej** kategorii | Ostrzeżenie `CriticalInputWithoutSubstitute` z reguły 5 walidatora zapala się dziś na wszystkim, więc nie niesie informacji — ostrzeżenie, które dotyczy każdego przypadku, jest tłem, nie sygnałem. Konsekwencja mechaniczna jest twarda: szczebel `Substituted` kaskady niedoboru był **nieosiągalny dla każdego towaru**, czyli jedna szósta maszyny stanów z PRD §8.4 nie miała jak się wydarzyć. Wypełnienie należy do fali B; poprawka wpisana do `M6c` |
| AG-7 | **Budżet z `D1` nie jest w M6b zmierzony i nie mógł być.** Benchmarki `bench_production_2400_lines`, `bench_dock_queue_1200` i `bench_replenish_9000_sites` z §7.4 należą do **M6e** (WP14), bo dopiero tam jest miasto, na którym da się je uruchomić. Zapisany jest natomiast **sufit, który trzeba będzie zmierzyć**: `advance_production` chodzi pętlą minuta po minucie, O(minutes) | Przy kroku minutowym (mikro i mezo) to jest jedna iteracja i nie ma czego optymalizować; przy makro z krokiem dobowym to jest 1 440 obrotów na zakład. Sufit jest nazwany w kodzie komentarzem `ponytail:` razem z drogą wyjścia (skok do najbliższego zdarzenia), ale droga wyjścia wymaga dowodu, że rozkład awarii się nie zmienił — a to jest robota dla benchmarku z M6e, nie dla domysłu z M6b |

---

## Zmiany wpisane po M6c

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu podfazy M6c;
reszta fazy nie jest przy okazji przeprojektowywana. Gwiazdka = zmiana zakresu albo kryterium.
Poprawki dotyczące samego §5.8–§5.9 są w tabeli `AH` dokumentu `M6c-rynek-b2b.md`;
tutaj jest to, co dotyczy **fazy**: kontraktów z §6, decyzji z §9 i testów z §7.

| # | Zmiana | Dlaczego |
|---|---|---|
| AI-1 ★ | **§6.1: cały rynek B2B mieszka w `sim/supply`, nie w `sim/economy`.** Macierz własności w 00 §1 mówi, że M6 „rozszerza `sim/economy` (rynek B2B)" — to jest nieprawda co do adresu, prawda co do konsumenta. `Rfq`, `Quote`, `SupplyContract`, `TradeNode` i zasób `B2b` są w `sim/supply`; do `sim/economy` idzie z nich **lista faktów do zaksięgowania** (`Settlement`, `SellerRef`), tym samym wzorcem co `Plant::bill_utilities` | Wycena oferty potrzebuje magazynu, zakładu, trasy i katalogu — wszystkiego, co jest w `sim/supply`. Zależność idzie `economy → supply` i odwrócić się nie da, więc rynek po tamtej stronie sięgałby po cudze wnętrze przy każdej ofercie. Pełne rozstrzygnięcie razem z losem `PriceBasis::NetB2B` jest w **`K-36`** |
| AI-2 ★ | **§6.1: `B2b` dochodzi do listy komponentów wchodzących do funkcji haszującej — jako zasób (`register_resource_hash`), nie komponent.** Rozszerzenie `AD-7` i `AG-2` na rynek: `Rfq`, `Quote`, `SupplyContract`, `TradeNode` i okno cen spot haszują się **przez `B2b`**, a nie każde z osobna | Ten sam argument co przy `Store` i `Plant`, i `K-29` dopuszcza go wprost: rozstrzygnięcie zapytania podpisuje kontrakt, wystawia zlecenie transportowe i sięga do areny partii w jednym kroku, a `World::resource_mut` pożycza **cały** świat. Uwaga wiążąca dla M6e: `sim/supply` **do dziś nie zależy od `magnat-ecs` i nie rejestruje ani jednego systemu** — wpięcie wszystkich czterech zasobów i ich kadencji jest robotą, której nikt jeszcze nie wykonał, i trzeba jej dać właściciela |
| AI-3 ★ | **§6.1: sygnatury rynku różnią się od wypisanych, bo `sim/supply` nie ma `&World`.** `open_rfq(w: &mut World, d: RfqDraft)` jest w rzeczywistości `B2b::open_rfq(&mut self, d, cat, store, plant, oracle, tuning, now)`; tak samo `sign_contract`, `import_quote`, `place_import`, `export_price`. `submit_quote` **nie powstaje jako funkcja publiczna** — oferty zbiera `collect_quotes` przy otwarciu zapytania | §6.1 pisano, zanim `AD-7` przeniósł stan do zasobów, więc wszystkie sygnatury zakładały świat ECS jako pierwszy argument. Kształt jest ten sam, adres inny. `submit_quote` odpada, bo do M7 **nie ma kto** złożyć oferty z zewnątrz: wycena dostawcy jest czystą funkcją stanu jego magazynu, a osobowość cenowa firmy przychodzi z `FirmPersonality` w M7 — i wtedy to ona dostanie ten punkt wejścia, wypełniony czymś więcej niż zaślepką |
| AI-4 ★ | **§6.2, wiersz M5: `economy::book(FirmId, LedgerEntry)` nie istnieje pod tą nazwą.** Odpowiedniki to `ledger::post(&mut Ledger, JournalEntry)` (księga zakładu) i `Books::transfer(from, to, amount, TxMemo, tick)` (pieniądz między kontami). `SupplierRef` w `sim/economy` ma **jeden** wariant, `External` — WP11 musi dołożyć `Firm(FirmId)`, inaczej zakup u lokalnego dostawcy nie ma jak się zaksięgować | Zapisane tutaj, a nie tylko w dokumencie podfazy, bo to jest wiersz w tabeli „konsumuję" i ktoś, kto ją czyta przed startem M6d, ma prawo sądzić, że ten kontrakt stoi. `Settlement` z M6c jest **gotowym wejściem** dla obu tych funkcji i niesie dokładnie to, czego potrzebują: strony, towar, masę, kwotę netto, cło osobno i powód |
| AI-5 | **Decyzja otwarta `D6` zamknięta zgodnie z propozycją: eksport zajmuje realny pojazd i rampę.** `try_export` wystawia `TransportOrder` do węzła, masa schodzi z bilansu dopiero po rozładunku (`AH-8`) | Propozycja brzmiała „tak — bez tego drenaż podaży jest fikcją księgową" i jest słuszna. Koszt CPU nadal do zmierzenia w `perf_400k` (M6e); plan B z `D6` — eksport w mezo — zostaje nietknięty i wykonalny, bo podział na decyzję i wchłonięcie jest już w kodzie |
| AI-6 ★ | **Decyzja otwarta `D3` zamknięta zgodnie z propozycją, i przy okazji rozszerzona o cenę indeksową.** Alokacja `ByMarketValue` bierze statyczne `external_base_price` z katalogu (M6a); **`index_now` kontraktów indeksowanych to natomiast krocząca średnia siedmiodobowa z rynku spot**, ważona masą | `D3` pytało o źródło cen odniesienia i bało się sprzężenia koszt→cena→koszt. Rozróżnienie, które je rozbraja: **alokacja kosztu** musi być stabilna, bo inaczej udział diesla w koszcie przerobu zmieniałby się co dobę i rachunek rafinerii przestałby być porównywalny z samym sobą; **indeksacja kontraktu** ma być ruchoma, bo po to się ją podpisuje. Sprzężenia nie ma, bo indeks wchodzi do **ceny**, nie do kosztu wytworzenia |
| AI-7 ★ | **§7.3 pkt 8 (`prop_contract_penalty`) rozbity na trzy sprawdzalne zdania i wszystkie trzy mają testy**, ale „cena `Collar` zawsze w `[floor, cap]`" jest testem **jednostkowym na cenniku**, a nie własnościowym na przebiegu | Klamra jest własnością czystej funkcji `ContractPricing::price_at` i test na przebiegu sprawdzałby ją tysiąc razy na tych samych trzech gałęziach — kosztowałby czas przebiegu i nie dodałby ani jednego przypadku, którego test jednostkowy nie widzi. Dwie pozostałe własności (`Σ naliczonych == Σ zapłaconych`, `fulfilled + missed == scheduled`) **wymagają** przebiegu, bo dotyczą akumulacji, i tam są sprawdzane |
| AI-8 ★ | **§7.7, `export_drains`: kryterium „wzrost cen lokalnych" przesuwa się za WP11, a nie jest spełnione w M6c.** Sam drenaż masy działa i jest sprawdzony; ceny nie drgną, dopóki półka kupuje u `ExternalSupplier` o nieskończonej podaży | Pełne uzasadnienie w `AH-12` dokumentu podfazy. Krótko: w M6c cena dostawcy jest kosztowa i nie ma członu reagującego na zapas, bo presja zapasu jest mechanizmem M5 — a to jest ta sama obserwacja, którą M5e zapisał jako `AD-3`. Kryterium zostawione tak, jak było, spełniłoby się **pozornie** albo wymusiło dopisanie do wyceny członu, którego model nie ma |
| AI-9 | **`R6` („sztywne łańcuchy bez substytutów") nie została w M6c zamknięta i dostaje adres: M7, razem z falą B katalogu.** `AG-1` w dokumencie M6c mówiło, że wypełnienie należy do M6c — to było sprzeczne z `K-34` i zostało poprawione (`AH-11`) | M6c nie ma pakietu roboczego będącego właścicielem katalogu: WP7–WP9 to rynek, katalog to WP1 zamknięty w M6a, a `K-34` przypisuje falę B do M7. Ryzyko nie zostało przy tym bez odpowiedzi: szczebel `Importing` przestał być zaślepką i jest realną alternatywą dla każdego towaru z `external_base_price`, więc kaskada ma dwa działające wyjścia zamiast jednego |

---

## Zmiany wpisane po M6d

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu podfazy M6d;
reszta fazy nie jest przy okazji przeprojektowywana. Gwiazdka = zmiana zakresu albo kryterium.
Poprawki dotyczące samego §5.10 są w tabeli `AL` dokumentu `M6d-zloza-i-koniec-dostawcy-zewnetrznego.md`;
tutaj jest to, co dotyczy **fazy**: kontraktów z §6, migracji z §6.3 i testów z §7.

| # | Zmiana | Dlaczego |
|---|---|---|
| AM-1 ★ | **§6.3 krok 1: sklep dostaje magazyn i rampę, ale nie `InventoryRule`.** Zaplecze i półka są slotami `Store` (`Backroom`, `Shelf`), sklep jest `PlantSite` bez ani jednej linii, rampa ma jedno stanowisko. Reguła zapasu **nie powstaje** | Sklep ma już politykę zamówień — `docelowy_zapas`, wiążącą cel z obrotem tygodniowym i z terminem ważności, kalibrowaną balansatorem od M5e. Druga polityka nad tym samym magazynem nie jest nadmiarem, tylko sprzecznością: obie zamawiają, żadna nie widzi zamówień drugiej i zaplecze rośnie do sumy obu celów. Pełne uzasadnienie w `AL-12` |
| AM-2 ★ | **§6.3 krok 4: sklep płaci przy odbiorze, nie przy zamówieniu.** Dostawa, na którą zabrakło środków, i tak wchodzi na stan — jako zobowiązanie (`TradePayable`) | Zapytanie ofertowe **może nie znaleźć dostawcy**, a sklep, który zapłacił za towar, którego nikt nie przywiózł, ma dziurę w kasie bez zdarzenia, które by ją tłumaczyło. To jest różnica między dostawcą zewnętrznym a rynkiem i nie da się jej wyrazić przy zapłacie z góry. Szczegóły i konsekwencja dla P5 w `AL-13` |
| AM-3 ★ | **§6.1: `Settlement` niesie `deliver_to: SiteId`.** Sam `buyer: FirmId` nie mówi, z którego konta zapłacić, bo firma może mieć wiele zakładów, a księga i rachunek bieżący są **per zakład** (M5 §5.8) | `AK-3` nazwało `Settlement` „gotowym wejściem księgowania" i było blisko: brakowało dokładnie jednego pola, i to takiego, które rynek zna (jest w `Rfq::deliver_to` i w kontrakcie), tylko go nie przekazywał |
| AM-4 ★ | **§6.1: `Store::spoil` zwraca listę odpisów, a nie ich liczbę.** `Vec<Spoiled>` z slotem, towarem, masą i kosztem własnym | Odpis dotyka pieniądza, a `sim/supply` księgi nie widzi i widzieć nie może (kierunek zależności). Liczba partii nie pozwalała zaksięgować niczego, więc detal musiał prowadzić **własny** rejestr terminów — czyli drugą kopię tego, co wie magazyn. Ten sam wzorzec, którym rynek oddaje `Settlement`, a zakład fakturę za media: fakty wychodzą listą, księguje ten, kto ma księgę |
| AM-5 ★ | **§6.1: `BatchSlice` niesie producenta**, a `FreightQuote` — stałą opłatę za podstawienie pojazdu (`call_out`) | Producent: bez niego zwrot nieudanej sprzedaży oddawał partię z zaślepką zamiast z autorem, a `trace_batch` gubił jeden etap. `call_out`: bez wyodrębnienia części płaconej raz za pojazd konsolidacja dostaw nie oszczędza nic, czyli kryterium `dc_beats_direct` spełnia się tożsamościowo (`AL-7`, `AL-8`) |
| AM-6 ★ | **§7.3 pkt 1: `prop_mass_conservation` obejmuje od tej chwili sklepy i nie ma listy wyjątków.** Test `sklepy_przestaly_byc_nieskonczone` (100 dób, trzy sklepy) sprawdza bilans **każdego** towaru, jaki przez nie przeszedł, oraz że odmowa z braku towaru ma pokrycie w pustej półce | To jest formalne kryterium ukończenia WP11 i dopiero teraz ma sens: do M6c sklepowy stub był na liście wyjątków, bo tworzył masę z niczego. Wariantu porównawczego „z `infinite_supply` i bez" **nie da się uruchomić w jednym przebiegu** — feature jest przełącznikiem kompilacji — więc porównaniem jest ten sam zestaw testów zbudowany dwa razy, i oba budowania muszą być zielone |
| AM-7 ★ | **Decyzja otwarta `D10` zamknięta, ale inaczej, niż brzmiała propozycja.** `Qty` w detalu **nie jest** wielokrotnością tysiąca: dla towarów sypkich i ciekłych jednostką natywną jest gram i `Qty` **jest** masą w gramach; dla sztukowych jest w milisztukach. Przelicznik wynika z `GoodForm` i `unit_mass`, bez nowego pola (`AL-10`) | `D10` pytało, co zrobić ze sprzedażą ułamka opakowania, i proponowało „towar `Bulk`, nie ułamek sztuki". Odpowiedź okazała się już wpisana w `GoodUnit` z M6a: postać towaru rozstrzyga, czym jest jednostka, i dokładnie ten rozdział ratuje sprzedaż na wagę. `Good::pack` z `AD-4` nie powstaje, bo trzecia liczba obok dwóch, które się zgadzają, byłaby trzecią do rozjechania |
| AM-8 ★ | **Decyzja otwarta `D4` (właściciel `Dock`) rozstrzygnięta praktyką, zgodnie z propozycją:** dane i kolejka rampy są w `sim/supply`, a rampę sklepu zakłada ten, kto zakłada sklep (`Market::open_shop`). Decyzji inwestycyjnej nie ma i nie będzie do M7 | Propozycja brzmiała „dane i kolejka w `sim/supply`, decyzja inwestycyjna w `sim/firms`" i nic w M6d jej nie podważyło. Zapisane, bo `D4` figurowało jako otwarte, a od WP11 ma pierwszego konsumenta, który nie jest testem |
| AM-9 | **`AC-1` wykonane, ale wejście zmieniło adres.** `Market::set_supply_shock` żyje i bramka **G4** ma czym szokować; szok mnoży teraz cenę **węzła granicznego**, a nie wycenę u dostawcy | Cena z `Wholesale::quote` służy do decyzji „stać mnie czy nie", a płaci się to, co wyjdzie z rozliczenia rynku — szok postawiony na wycenie mierzyłby własny parametr zamiast skutku. Zmierzone po przenosinach: odpowiedź cen w 2–4 dobach, czyli w widełkach 2–7 z kryterium G4 |
| AM-10 ★ | **`R2` („zakleszczenie bootstrapu grafu") nie jest zamknięte i dostaje adres: M6e.** Do WP11 sklepy biorą towar **przez bramę graniczną**, bo zakłady produkcyjne Etapu 7 stoją w danych miasta, ale nie mają jeszcze ani linii, ani obsady, ani zapasu startowego | To nie jest obejście, tylko właściwa odpowiedź na stan pośredni: import jest prawdziwym źródłem — z przepustowością, kolejką, ceną rosnącą z wolumenem i cłem — więc bilans masy domyka się bez ani jednego grama z powietrza, a `import_not_free` ma czego pilnować. Czego brakuje: `data/scenarios/initial_stock.ron` z §8 i `PlantSite` budowany z `SiteSeed`. Kiedy powstaną, rynek lokalny wygra ceną sam, bo transport zza granicy kosztuje i kolejka na bramie rośnie |

---

## Zmiany wpisane po M6e

Zgodnie z `K-18`. To jest ostatnia podfaza fazy, więc tabela zamyka też rachunek
z §9: co zostało rozstrzygnięte, a co idzie dalej i do kogo. Poprawki dotyczące
samego §5.11 są w tabeli `AP` dokumentu `M6e-panel-testy-pamiec.md`; tutaj jest to,
co dotyczy **fazy**. Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| AQ-1 ★ | **§5.11 rozpisuje łańcuch na piętnaście systemów ECS; powstaje jeden.** `supply.Chain` (`EveryMinute`, wyłączny, przed `economy.Market`) wykonuje z §5.11 to, co naprawdę decyduje o §7.4: **częstotliwości i rozpraszanie po indeksie encji**. Pełne uzasadnienie w `AP-4` | Cały stan łańcucha siedzi w **jednym** zasobie (`AO-1`), więc piętnaście systemów byłoby piętnastoma systemami wyłącznymi (`K-21`) nad jednym `Mutex`em: piętnaście poziomów harmonogramu, zero krawędzi DAG, zero zrównoleglenia. Tabela §5.11 zostaje jako **opis kadencji** i w tej roli jest prawdziwa co do wiersza; przestaje być listą systemów do zarejestrowania |
| AQ-2 ★ | **§6.1: `bill_utilities` zwraca `Vec<UtilityBill>`, a nie trójki.** Rodzaj medium musi dojść do księgującego, bo niesie go `TxKind::Utility` (`AP-5`) | Bez rodzaju rachunek zakładu pokazywałby prąd i wodę jako jedną pozycję „media" — tę samą informację, po którą gracz otwiera kartę zakładu |
| AQ-3 ★ | **§6.1: `trace_batch` i `supply_graph` istnieją, ale w sygnaturach bez `&World`** — tak samo jak cały rynek B2B po `AI-3`. `trace_batch(&Store, BatchId) -> BatchTrace`, `supply_graph(&Chain, &Catalog, FirmId) -> SupplyGraphView`. Dochodzą `batches_in_role` (punkt wejścia „śledź partię": gracz wskazuje bochenek, nie uchwyt areny) i `Store::mark_traced` | `sim/supply` nie zależy od `magnat-ecs` po to, żeby mieć `&World` — zależy po to, żeby zarejestrować **jeden** system (`AQ-1`). Reszta crate'u pozostaje biblioteką czystych funkcji nad zasobami i ma nią zostać: to ona jest testowalna bez świata i to w niej stoi dowód spójności LOD |
| AQ-4 ★ | **§6.4.2 zyskuje czwarty przypadek, którego nie miało: „partia nie była śledzona".** `TraceOrigin` ma cztery warianty — `Deposit`, `Imported`, `InitialStock`, `NotTraced` — i panel mówi każdy z nich osobnym zdaniem | §6.4.2 rozróżniało partie `TRACED` i nieoznaczone, ale opisywało ślad urwany **wyłącznie** dla przypadku makro („odtworzone z agregatu dzielnicy"). W praktyce urywa się na trzy inne sposoby i wszystkie trzy są uczciwe: import (świata poza miastem nie symulujemy), zapas otwarcia świata i brak śladu w ogóle. Jedno zdanie na wszystkie znaczyłoby, że panel nie odróżnia „nie wiem, bo nie patrzyłem" od „wiem i to jest granica" — a to jest różnica, na której stoi zaufanie do panelu |
| AQ-5 ★ | **§7.3 pkt 5 (`prop_cost_vs_mass`) znalazł błąd w chwili, w której powstał** — i to jest odpowiedź na pytanie, po co pisze się testy własnościowe po fakcie. Sprzedaż lokalna nie przeszacowywała kosztu własnego partii na cenę zapłaconą (`AP-7`) | Osiem testów własnościowych z §7.3 miało do M6e cztery implementacje. Dopisanie pozostałych czterech nie było odhaczaniem listy: dwa z nich (`prop_cost_vs_mass`, `prop_no_expired_on_shelf`) dotyczą własności, które da się złamać **wyłącznie** przez ciąg operacji, a nie przez jedną. Drugi z nich wymusił przy okazji rozróżnienie, którego magazyn nie miał: wydanie na linię, sprzedaż z półki i odpis to trzy różne rzeczy, a `Store::take` traktowało je jednakowo |
| AQ-6 ★ | **§7.7, `perf_400k`: zmierzone i w budżecie, ale liczba pamięci jest inna niż szacunek.** 600 tys. partii × **104 B** = 59,5 MB wobec budżetu 64 MB; §7.4 szacowało 88 B. Zapas wynosi siedem bajtów na partię | `Batch` w układzie SoA z §7.4 to plan, a nie pomiar — realna struktura ma wyrównanie i `Option`y. Konsekwencja jest twarda i ma test: **dopisanie jednego pola `u64` do `Batch` wychodzi poza budżet**. Dlatego sufit pilnuje `size_of`, a nie przebieg: test ma pękać w tej samej zmianie, która pole wnosi, a nie pół roku później przy profilowaniu |
| AQ-7 ★ | **§7.7, `e2e_bread` i `e2e_fuel` biegną na scenariuszu testowym, nie na `scenarios/m6_lancuch.ron`.** Zakłady stawia test, towar jedzie zleceniem wystawionym wprost, a rynek B2B, kaskada i wybór dostawcy mają własne testy | Pytanie tych dwóch testów brzmi „czy masa, jakość i koszt przeżywają całą drogę", a nie „czy rynek wybiera dobrze". Zmieszanie obu dałoby test pękający z sześciu powodów naraz i niemówiący, z którego. Scenariusz `m6_lancuch.ron` jako plik danych **nie powstaje** — `data/scenarios/` dostaje w tej fazie `initial_stock.ron`, a format scenariusza opisanego danymi projektuje M9a (`Scenario`) i dopiero wtedy będzie do czego go dopisać |
| AQ-8 ★ | **§7.4: w mieście 4 km z 40 tys. mieszkańców staje 39 zakładów produkcyjnych, z czego 0 wydobywczych ze złożem.** Łańcuch chleba i łańcuch paliwa **nie domykają się w mieście** — domykają się w testach | Skala się zgadza (800 zakładów przy 400 tys. to 80 przy 40 tys.), ale kopalnie nie: archetypy wydobywcze trafiają w tym mieście do wypełniacza strefy, który złóż nie sprawdza (`AP-12`). Rynek lokalny działa i zakłady sprzedają sobie nawzajem; brakującym ogniwem jest **surowiec pierwotny**, który wciąż przychodzi bramą graniczną. To nie jest regres wobec M6d — to jest to samo miejsce, tylko teraz widać dokładnie, ilu zakładów dotyczy. Adres: **M7**, razem z decyzjami lokalizacyjnymi firm i falą B katalogu (`R6`, `AI-9`) |
| AQ-9 | **Decyzja otwarta `D1` (budżet ms/tick) zamknięta pomiarem, zgodnie z propozycją.** 2,0 ms `EveryMinute`, 12 ms `EveryHour`, 40 ms `EveryDay` — wszystkie składniki zmierzone i wszystkie w budżecie z zapasem (`AP-10`) | `D1` prosiło o zatwierdzenie budżetu w globalnym rachunku ticku. Liczby są, więc zatwierdzenie przestało być decyzją, a stało się odczytem. Największy pojedynczy składnik to produkcja (117 µs), największy sufit — `advance_production` przy kroku dobowym (3,78 ms na 100 zakładów), i ten drugi jest ceną tolerancji 0 spójności LOD, a nie kosztem do zoptymalizowania |
| AQ-10 | **Decyzja otwarta `D12` (kolejność `SpoilageSystem` przed `RetailSystem`) zamknięta: jest **krawędzią w DAG**, a nie komentarzem.** `supply.Chain.before(economy.Market)`; żeby ją dało się wyrazić, `engine/ecs` musiał przestać nadpisywać deklaracje autora krawędzią z konfliktu (`AP-6`, `K-42`) | `D12` prosiło o „jawny zapis w grafie zależności systemów po stronie M5". Do M6e był to komentarz w środku `MarketSystem`, bo jawny zapis **nie kompilował się** — dawał cykl. Teraz jest zapisem i pilnuje go `prop_no_expired_on_shelf` |
| AQ-11 | **Decyzje otwarte `D5`, `D8` i `D13` przechodzą dalej z adresatem — świadomie, nie przez przeoczenie.** `D5` (`HazardClass` a przepisy miejskie) → **M8**: `Dock::hours` i maska hazardu istnieją i są czytane z danych zakładu, więc M8 podmienia wartości, a nie kształt. `D8` (retencja `BatchLedger`) → **M9/M12**: dziennik rośnie dziś w pamięci i nie udaje, że umie się zarchiwizować; kompaktowanie do pięciu etapów kluczowych ma sens dopiero razem z zapisem w tle. `D13` (kto emituje `TransportOrder` dla dostaw do gospodarstw) → **M10**: `order_transport` udźwignie to bez zmiany kontraktu, bo nadawcą jest slot, a nie firma | Treść bramki 7 brzmi „zamknięte **albo świadomie przeniesione dalej z adresatem**". Każda z tych trzech ma dziś działający mechanizm i brakującą **politykę**, a polityka należy do fazy, która ją projektuje. Przenoszenie ich w ciemno byłoby odkładaniem decyzji; przenoszenie z nazwanym mechanizmem jest przekazaniem roboty |

---

## Zmiany wpisane po M9e

Zgodnie z `K-18`. Wpis powstał przy przeglądzie zapisu sesji po M9e — nie z pracy nad M6.
Prefiks `AR-n`. Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| AR-1 ★ | **Scenariusz `export_drains` z §7.7 nie powstał i dostaje wykonawcę: `R2-WP34` w `R2f-pomiar-i-bramki.md`.** Nazwa występuje dziś wyłącznie w tym dokumencie i w `M6c`; nie ma jej ani w `data/scenarios/`, ani nigdzie w kodzie. Druga połowa kryterium WP9 — „eksport mierzalnie podnosi ceny lokalne" — **nie została zmierzona**, a zawężenie z `AH-12` miało być tymczasowe | `AH-12` zawęziło kryterium słusznie (ceny nie drgną, dopóki półka nie kupuje z rynku B2B), a `AI-8` przeniosło pomiar za WP11, czyli do M6e. M6e zamknęło się bez niego i nikt tego nie zauważył, bo **zawężone kryterium przechodzi**. Zawężenie z adresatem jest dobrą praktyką dokładnie tak długo, jak długo ktoś sprawdza adresata — czego do R2-WP26 nie robi nic |
| AR-2 | **Pozycje 3 i 5 wykazu `R2` cytowały `AG-6` i `AQ-8` jako pochodzące z `M6c` i `M6e`; oba kody są w tym dokumencie.** Poprawione w `R2-naprawy-po-M11.md` i w `R2c-rozjazdy-danych-i-kodu.md` | Drobiazg, ale tego samego rodzaju co reszta: odesłanie, które prowadzi do pliku bez szukanego kodu, kosztuje kwadrans przy każdym czytaniu i uczy nie ufać odesłaniom |

---

## Zmiany wpisane po R2f

Zgodnie z `K-18`. `R2-WP34` wykonał obietnicę `AR-1` i przy okazji zmierzył coś, czego
żaden z trzech poprzednich wpisów o `export_drains` nie przewidywał.

| # | Zmiana | Dlaczego |
|---|---|---|
| AS-1 ★ | **Kryterium WP9 zostaje zawężone na stałe, a zawężenie ma odtąd liczbę.** Scenariusz `export-drains` istnieje (`tools/headless/src/export_drains.rs`) i jest **pierwszym wołającym `B2b::try_export` poza testami**. Zmierzone na świecie odniesienia (4 km, `industrial`, ziarno 1, 40 dób, szok +40 % w dobie 20): wywieziono **0 kg**, a powód nie jest cenowy. Pięć towarów w obrocie granicznym o największym zapasie — `raw_crude_oil` 489 t, `raw_wheat` 209 t, `chem_plastic_granule` 165 t, `raw_milk` 140 t, `mat_leather` 130 t — ma **zero kilogramów w slocie wyjściowym któregokolwiek zakładu**. Eksportuje się wyrób, czyli zawartość slotu wyjściowego; zapas leżący w slocie wejściowym cudzego zakładu albo na placu bramy granicznej nie jest niczym, co da się wywieźć | `AH-12` i `AI-8` zawęziły kryterium do samego drenażu masy, bo „ceny nie drgną, dopóki półka nie kupuje z rynku B2B". Pomiar pokazuje, że brakujące ogniwo leży **wcześniej**: nie ma czego wywieźć, więc nie ma też czym ruszyć ceny. To jest inny brak niż ten, który zapisały obie tamte korekty, i **mocniejszy**: cenę dałoby się zmierzyć, gdyby masa wyszła; masa nie wychodzi z powodu, który nie ma nic wspólnego z ceną |
| AS-2 | **Drugi towar nie jest wyborem, tylko konsekwencją.** Kryterium pyta o dwie krzywe **tego samego** towaru. W mieście odniesienia zbiór „ma zapas" i zbiór „stoi na półce" są **rozłączne**: wyrób gotowy (`food_milk`) idzie na półkę tego samego dnia i u producenta zostaje zero, a surowiec i półprodukt mają zapas i nie mają ceny detalicznej. Scenariusz wybiera towar o największym zapasie i mówi wprost, gdy nie stoi on na półce | Pierwszy przebieg wybrał `food_milk` — 37 t w dobie zero i zero od doby pierwszej — i mierzył przez to własny filtr. Wpisane, bo to samo założenie („znajdzie się towar o wysokim udziale eksportu") stoi w §7.7 od M6 i nie jest prawdziwe w żadnym wygenerowanym mieście |
| AS-3 | **Pola „udział eksportu" nie ma w `data/goods/` i nie było go nigdy.** Zakres `R2-WP34` mówi „towar z wysokim udziałem eksportu"; eksportowalność wynika z dwóch warunków naraz (`external_base_price` obecne i brama w `import_via`), a udziału nie mierzy nic | Wpisane, żeby następny czytelnik §7.7 nie szukał pola, którego nie ma |
