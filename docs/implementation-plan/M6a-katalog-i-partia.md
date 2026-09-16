# M6a — Katalog i partia

Podfaza 1 z 5 fazy **M6 — Łańcuch dostaw** (`M6-lancuch-dostaw.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M2 (minimalny katalog i walidator), M5 (`NeedCategoryId`). |
| **Pakiety robocze** | WP1, WP2, WP3 |
| **Projekt techniczny** | §5.1, §5.2, §5.3, §5.4 |
| **Wynik do pokazania** | `cargo test -p goods-graph` zielony razem z testem negatywnym odziedziczonym po M2; `prop_mass_conservation` na scenariuszu z samym magazynem. |
| **Kryterium zamknięcia** | Kryteria WP1–WP3; partia dzielona 1000 razy nie gubi ani grama, ani grosza. |
| **Poprzednia / następna** | — (pierwsza w fazie) · `M6b-zaklad-i-transport.md` |

Docelowy schemat katalogu towarów z falą A (~60 towarów), partia ze wszystkimi właściwościami, warunki przechowywania i psucie, receptury z modelem jakości.

---

## Pakiety robocze

### WP1 — Docelowy schemat katalogu i pełny katalog towarów
**Zależy od:** M0 (typy), **M2 (minimalny katalog i walidator już istnieją)**, M5 (`NeedCategoryId`).

**Podział własności z M2 (dok. 00 §5).** Walidator domknięcia grafu wchodzi do CI już w **M2**, bo Etap 7 generacji miasta musi sprawdzić domknięcie łańcuchów, zanim powstanie jakakolwiek produkcja. M2 tworzy **minimalny** `data/goods/` i `data/recipes/` oraz sam walidator. **Właścicielem docelowego schematu i pełnego katalogu ~400 towarów jest M6.**

Praktycznie znaczy to, że WP1 **nie pisze walidatora od zera** — rozszerza istniejący o reguły 2–5 z listy poniżej (M2 potrzebuje wyłącznie reguły 1, osiągalności) i migruje minimalny katalog na docelowy schemat. Schemat `Good` z §5.1 został wysłany M2 do uzgodnienia **przed** startem M2, żeby nie trzeba było przepisywać plików danych (decyzja D14); pola, których M2 nie użyje, mają mieć wartości domyślne, aby minimalny katalog ładował się pod docelowym schematem bez migracji.

**Opis.** Docelowa struktura `Good`, ładowanie RON, nadawanie `GoodId` w kolejności alfabetycznej klucza. Struktura plików: `data/goods/<kategoria>.ron` (~60 plików), `data/recipes/<branża>.ron`, `data/deposits/<typ>.ron`, `data/trade/<węzeł>.ron`. Konwencja klucza: `<domena>_<nazwa>[_<wariant>]` — `food_bread_wheat`, `fuel_diesel_b7`, `part_bearing_6204`, `raw_crude_oil`. Mapowanie `ResourceKind → GoodId` (K-13) jest częścią tego pakietu — to M6 decyduje, że `ResourceKind::Oil` to `raw_crude_oil`.

**Napełnianie katalogu — proces, nie jednorazowy zryw.** Trzy fale, każda domykana walidatorem:

- **Fala A (ta faza, ~60 towarów):** oba łańcuchy referencyjne (paliwo, chleb) w pełni + media (prąd, gaz, woda, ciepło) + części zamienne trzech klas + opakowania + nawóz, nasiona, pasza. To minimum, przy którym testy E2E przechodzą.
- **Fala B (M7, ~180 towarów):** pełna żywność, chemia gospodarcza, budowlanka, papier, tekstylia. Reguła wypełnienia: **każda kategoria potrzeby z M5 musi mieć ≥ 3 towary** różniące się jakością i ceną, inaczej nie ma czego substytuować i mechanika §8.4 jest martwa.
- **Fala C (M10, ~160 towarów):** dobra trwałe, elektronika, samochód, dobra luksusowe, produkty z R&D.

**Reguła porządkowa (wymuszona przez CI):** towar wchodzi do repo **razem** ze swoim źródłem — recepturą, złożem albo wpisem w `TradeNode`. Commit z sierotą nie przechodzi. Dzięki temu graf jest domknięty w każdej chwili, a nie „kiedyś na końcu".

**Walidator `tools/goods-graph`** (test CI od tej fazy, zgodnie z dok. 00 §5):

1. **Osiągalność (punkt stały):** start ze zbioru źródeł pierwotnych = {towary ze złóż} ∪ {towary z `TradeNode`}. Receptura „odpala się", gdy **wszystkie** jej wejścia są już osiągalne; jej wyjścia dołączają do zbioru. Iteruj do punktu stałego. Towar nieosiągalny → **błąd**. Ten algorytm wykrywa nierozwiązywalny bootstrap (rafineria potrzebuje prądu, elektrownia paliwa, i ani jednego nie da się zaimportować).
2. **Ujście:** każdy towar musi mieć ≥ 1 odbiorcę (receptura, konsumpcja mieszkańca, eksport, utylizacja). Inaczej zapcha magazyny → **błąd**.
3. **Bilans masy receptur:** `Σ inputs.mass == Σ outputs.mass + process_loss` dla każdej receptury → **błąd** przy rozjeździe (walidacja statyczna, nie runtime).
4. **Spójność jednostek:** `density > 0` dla `Bulk`/`Liquid`/`Gas`; `unit_mass > 0` i `unit_volume > 0` dla `Piece`.
5. **Ostrzeżenia:** towar z dokładnie jednym źródłem; krytyczne wejście receptury bez substytutu w kategorii żywnościowej lub energetycznej; towar bez `external_base_price` (nie da się go zaimportować w kryzysie).

**Test negatywny odziedziczony po M2 — nie wolno go osłabić.** M2 zapisał zaostrzenie o zapasie startowym (§6.4.1) jako jawny test negatywny w swoim WP14: katalog, w którym towar jest osiągalny **wyłącznie** z `initial_stock.ron`, **musi oblać walidację**. Powód, dla którego istnieje jako test, a nie jako reguła w komentarzu: reguła w komentarzu zgniłaby przy pierwszym refaktorze, a klasa błędu, którą łapie, ujawnia się dopiero jako „miasto po dwóch tygodniach przestaje produkować X" — czyli w najgorszym możliwym momencie, długo po commicie, który ją wprowadził.

Konsekwencja dla M6: każda z trzech fal napełniania katalogu musi przejść ten test. W praktyce znaczy to, że **zapas startowy wolno dodać dopiero wtedy, gdy łańcuch już się domyka bez niego** — `initial_stock.ron` skraca rozruch świata, nigdy nie zastępuje źródła.

**Kryterium ukończenia:** fala A w repo, `cargo test -p goods-graph` zielony (łącznie z testem negatywnym M2), CLI `goods-graph new <klucz>` generuje szkielet towaru z szablonu kategorii i od razu odpala walidację.
**Rozmiar: M**

### WP2 — Partia, magazyn, warunki przechowywania, psucie
**Zależy od:** WP1.
**Opis.** `Batch` ze wszystkimi właściwościami §8.2, `StorageSlot`, `StorageClass`, `HazardClass`, FEFO, wycena zapasów po koszcie (`cost_total`), `BatchOrigin` + `BatchLedger`, `SpoilageSystem` na kopcu po `expires_at`. Operacje `put` / `reserve` / `take` / `split` / `merge` z twardymi inwariantami masy i pieniądza.
**Kryterium ukończenia:** `prop_mass_conservation` i `prop_no_negative_stock` zielone na scenariuszu z samym magazynem; partia dzielona 1000 razy nie gubi ani grama, ani grosza.
**Rozmiar: L**

### WP3 — Receptury i model jakości
**Zależy od:** WP2.
**Opis.** `Recipe`, `RecipeInput` (jakość minimalna, lista substytutów z karą jakości), `RecipeOutput` z `OutputKind::{Main, ByProduct, Waste, SelfConsumed}`, `QualityModel` jako parametry (nie domknięcie — musi być serializowalny i deterministyczny), alokacja kosztu produkcji łącznej.
**Kryterium ukończenia:** przemiał zboża i frakcjonowanie ropy dają liczby z tabel w §7 tego dokumentu (±0 g); jakość wyjścia jest funkcją czystą — ten sam wsad daje tę samą jakość.
**Rozmiar: M**

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.1 Katalog towarów

```rust
pub struct Good {
    pub key: Box<str>,                    // "food_bread_wheat" — stabilny, to on idzie do zapisu gry
    pub id: GoodId,                       // nadawany przy ładowaniu, alfabetycznie po key
    pub name: LocKey,
    pub category: NeedCategoryId,         // z M5, ~60 kategorii potrzeb
    pub form: GoodForm,                   // Bulk | Liquid | Gas | Piece | Palletized
    pub unit: GoodUnit,                   // JEDNOSTKA NATYWNA — publiczna i stabilna (kontrakt z M10)
    pub density_g_per_l: u32,             // przeliczenie Mass <-> Volume dla Bulk/Liquid/Gas
    pub unit_mass: Mass,                  // Piece: masa 1 szt. (netto)
    pub unit_volume: Volume,              // Piece: objętość 1 szt. brutto (z opakowaniem)
    pub shelf_life_minutes: Option<u32>,  // None = nie psuje się
    pub storage: StorageClass,            // wymagana klasa magazynu
    pub hazard: HazardClass,
    pub has_quality: bool,                // towary homogeniczne (piasek, woda) mają quality = 50 i koniec
    pub substitutes: Vec<Substitute>,     // (GoodId, konwersja masy, kara jakości)
    pub external_base_price: Option<Money>,  // za tonę / 1000 szt.; None = NIEIMPORTOWALNY
    pub tariff_class: TariffClassId,      // stub do M8
    pub disposal_cost: Money,             // za tonę, dla odpadów
    pub pack: Option<RetailPack>,         // wielkość opakowania detalicznego (kontrakt z M5)
}

pub enum GoodForm { Bulk, Liquid, Gas, Piece, Palletized }

/// Jednostka natywna towaru. PUBLICZNA i STABILNA — nie jest szczegółem wewnętrznym
/// `sim/supply`. Czyta ją `sim/macro::lift()` (M10) oraz walidator katalogu.
pub enum GoodUnit {
    Grams,        // Bulk | Liquid | Gas — wartość i64 to gramy
    Milliunits,   // Piece | Palletized — wartość i64 to milisztuki, zawsze wielokrotność 1000
}

pub struct Substitute { pub good: GoodId, pub mass_ratio_permille: u16, pub quality_penalty: u8 }
```

**Jednostka wiodąca to zawsze `Mass`.** `Volume` i `Qty` są funkcjami masy (przez `density_g_per_l` / `unit_mass`) i istnieją dla czytelności oraz dla ograniczeń pojemnościowych. Bilans własnościowy liczy się wyłącznie w gramach — to usuwa całą klasę błędów zaokrągleń.

**Kontrakt z M10 (`sim/macro`).** `GoodUnit` jest jednostką, w której `MacroStock(SparseVec<GoodId, i64>)` trzyma **dokładnie tę samą liczbę**, którą trzyma `sim/supply` — `lift()` i `lower()` jej nie przeliczają. Właścicielem jednostki jest M6, przez `data/goods/`; M10 tylko sumuje. Brak konwersji na granicy LOD oznacza zero zaokrągleń, więc zachowanie masy co do grama wynika konstrukcyjnie, a nie z ostrożnej arytmetyki. Zobowiązanie M6: `GoodUnit` nigdy nie zmienia się dla istniejącego `key` w obrębie wersji danych — zmiana jednostki towaru to nowy `key`.

**Inwariant dla towarów sztukowych:** `Qty` w łańcuchu dostaw jest zawsze wielokrotnością 1000 (całe sztuki). Milisztuki rezerwujemy dla warstwy detalicznej M5 (por. decyzja otwarta D10).

### 5.2 Partia

```rust
pub struct Batch {
    pub good: GoodId,
    pub mass: Mass,                   // g — POLE WIODĄCE bilansu
    pub qty: Qty,                     // milisztuki; 0 dla Bulk/Liquid/Gas
    pub volume: Volume,               // ml brutto — do ograniczeń pojemności
    pub quality: Q,                   // 0..=100
    pub brand: Option<BrandId>,       // właścicielem semantyki jest M10, M6 tylko przenosi
    pub producer: FirmId,
    pub produced_at: SimMinute,
    pub expires_at: Option<SimMinute>,
    pub storage_req: StorageClass,
    pub hazard: HazardClass,
    pub cost_total: Money,            // koszt CAŁEJ partii, nie jednostkowy — patrz niżej
    pub origin: BatchOrigin,
    pub location: BatchLocation,
    pub flags: BatchFlags,            // TRACED | QUARANTINED | RESERVED | MARKDOWN
}

pub enum BatchLocation {
    Slot(SlotId),                     // magazyn wejściowy/wyjściowy/półka/zbiornik
    InTransit(TransportOrderId),
    OnLine(LineId),                   // w trakcie przetwarzania
}
```

**K-16 — partie żyją w dedykowanej arenie, nie w ECS (dok. 00 §2, rozstrzygnięcie po D2).** `BatchId` przestaje być `Entity`; uchwyt to `{ index: u32, generation: NonZeroU32 }`. Uzasadnienie: wysoka rotacja partii i brak zapytań przekrojowych po archetypach, więc mutacja strukturalna ECS byłaby kosztem bez korzyści. `Arena<T>` dostarcza M0 — M6 **nie pisze własnej**, a brakujące operacje zgłasza do M0.

Cztery warunki nienegocjowalne, które M6 musi spełnić:

1. **Arena wchodzi do funkcji haszującej stan**, iterowana **w kolejności indeksów**. To nie jest formalność: pominięcie areny w hashu daje dziurę w determinizmie, której żaden istniejący test nie wykryje, bo hash dalej byłby stabilny — tylko przestałby cokolwiek znaczyć dla największej struktury danych w grze.
2. **Uchwyt po zwolnieniu nigdy nie staje się ponownie ważny** — `generation` inkrementowana przy zwolnieniu slotu. Dotyczy to zwłaszcza `BatchOrigin::parents` i `TransportOrder::cargo`, które przechowują uchwyty do partii mogących zniknąć.
3. **Snapshot traktuje arenę jak sekcję ECS** — ten sam format wersjonowania i ta sama ścieżka zapisu w tle (`engine/io`).
4. Kompaktowanie areny (WP15) **nie może zmieniać indeksów żywych partii** w trakcie ticku — przenumerowanie, jeśli w ogóle, wyłącznie w punkcie synchronizacji i z przemapowaniem wszystkich uchwytów naraz.

**Koszt: pole `cost_total`, nie `unit_cost`.** PRD mówi „koszt jednostkowy do wyceny zapasów", ale przechowywanie kosztu na gram zaokrągla do zera dla towarów tanich i gubi grosze przy każdym podziale. Trzymamy koszt całej partii; podział dzieli go proporcjonalnie do masy z regułą reszty z dok. 00 §2 (reszta do pierwszej części wg ustalonego porządku). `unit_cost()` jest getterem dla UI i księgowości (Money za tonę albo za 1000 szt.). Dzięki temu suma `cost_total` po wszystkich partiach jest dokładnie równa sumie zapłaconych kwot pomniejszonej o rozliczony COGS — i da się to sprawdzić testem.

**Pochodzenie — dwa poziomy, bo pełne drzewo dla 600 tys. partii jest niewykonalne:**

```rust
pub struct BatchOrigin {
    pub site: SiteId,                 // gdzie powstała
    pub recipe: Option<RecipeId>,
    pub depth: u8,                    // ile etapów od surowca — do UI i do limitu
    pub deposit: Option<DepositId>,   // propagowane dla surowców pierwotnych (paliwo -> złoże)
}
```

- **Poziom lekki (domyślny, każda partia):** powyższa struktura, 16 B. Pozwala odpowiedzieć „skąd to jest" na poziomie zakładu i złoża.
- **Poziom pełny (`BatchFlags::TRACED`):** dodatkowo wpisy w `BatchLedger` — append-only strumieniu poza ECS, kompresowanym, trzymanym po stronie `engine/io`:

```rust
pub struct BatchEvent { pub batch: BatchId, pub at: SimMinute, pub site: SiteId,
                        pub kind: TraceKind, pub mass: Mass, pub cost_cumulative: Money, pub quality: Q }
pub enum TraceKind { Produced, Stored, Loaded, Departed, Arrived, Unloaded, Shelved,
                     Consumed, Sold, Lost(LossKind), Split, Merged }
```

Flagę `TRACED` dostają automatycznie: partie przechodzące przez firmę gracza, partie objęte aferą (§11 PRD), partie wybrane ręcznie w UI. Partie nieoznaczone nie trafiają do ledgera — koszt śledzenia płacimy tylko tam, gdzie ktoś patrzy. Partia `TRACED` **nie podlega scalaniu** (WP15).

### 5.3 Warunki przechowywania i klasa niebezpieczeństwa

```rust
pub enum StorageClass { Ambient, Dry, Chilled, Frozen, Silo, Tank, PressureTank, Yard, Secure }
pub enum HazardClass  { None, Flammable, Explosive, Toxic, Corrosive, Perishable, Oversize }

pub struct StorageCondition {          // opis dla UI i dla danych; logika działa na StorageClass
    pub class: StorageClass,
    pub temp_min_dc: i16, pub temp_max_dc: i16,   // dziesiąte stopnia Celsjusza
    pub humidity_max_pct: u8,
    pub pressurized: bool,
}
```

Sprawdzenie zgodności jest tanie: `slot.class.accepts(batch.storage_req)` oraz `slot.hazard_mask.allows(batch.hazard)`. Nie modelujemy ciągłej krzywej temperatury — dwa stany: warunki spełnione / niespełnione. Niespełnione = mnożnik psucia **×8** (partia w niewłaściwym slocie traci przydatność 8× szybciej), a dla `Flammable` / `Explosive` / `Toxic` w slocie bez maski operacja `put` po prostu się nie powiedzie (`StoreError::HazardNotAllowed`). To jest granica zaufania, nie miejsce na uproszczenie.

### 5.4 Receptury

```rust
pub struct Recipe {
    pub key: Box<str>, pub id: RecipeId,
    pub source: RecipeSource,             // PUNKT WEJŚCIA domknięcia grafu — wymagany przez M2
    pub inputs: Vec<RecipeInput>,
    pub outputs: Vec<RecipeOutput>,
    pub batch_mass: Mass,                 // nominalny wsad jednej szarży
    pub duration_minutes: u32,
    pub energy: Energy, pub water: Volume,
    pub labour: Vec<(JobRoleId, u32)>,    // osobominuty na szarżę
    pub machine_class: MachineClassId,
    pub setup: Setup,                     // przezbrojenie z innej receptury
    pub emissions: Emissions,             // na szarżę
    pub quality: QualityModel,
    pub cost_allocation: CostAllocation,
}

pub struct RecipeInput {
    pub good: GoodId, pub mass: Mass, pub min_quality: Q,
    pub substitutes: Vec<Substitute>,     // np. olej rzepakowy zamiast słonecznikowego
    pub critical: bool,                   // brak = stop; niekrytyczne obniża tylko jakość
}

pub struct RecipeOutput { pub good: GoodId, pub mass: Mass, pub kind: OutputKind }
pub enum OutputKind { Main, ByProduct, Waste, SelfConsumed }   // SelfConsumed: gaz opałowy spalany na miejscu

/// Punkt wejścia domknięcia grafu produktów. Receptury `Extraction` i `Agriculture`
/// są źródłami pierwotnymi — ich wejścia materiałowe nie muszą być osiągalne,
/// bo masa pochodzi ze złoża albo z gleby, a nie z innej receptury.
pub enum RecipeSource {
    Manufacturing,                 // zwykła: wszystkie wejścia muszą być osiągalne
    Extraction(ResourceKind),      // wymaga złoża pod parcelą (K-13, TerrainQuery)
    Agriculture,                   // wymaga gleby o wymaganej klasie
}

pub struct Setup { pub minutes: u32, pub scrap_mass: Mass, pub energy: Energy }

pub struct Emissions {
    pub noise_db: u8, pub pm_g: i32, pub co2_g: i32, pub wastewater: Volume,
    pub plume: PlumeKind,        // charakter pióropusza — deklarowany w danych, nie wyliczany
}

/// Co widać nad kominem. Mieści się na 2 bitach (kontrakt z M11, §6.4.3).
/// To WŁAŚCIWOŚĆ PROCESU deklarowana w `data/recipes/`, a nie funkcja stosunku pm_g do co2_g:
/// czy z komina leci para czy sadza, wynika z tego, co się w środku dzieje, i nie da się
/// tego wiarygodnie odgadnąć z dwóch liczb.
pub enum PlumeKind {
    None = 0,      // brak widocznej emisji (montownia, szwalnia, magazyn)
    Steam = 1,     // biała para: chłodnia kominowa, elektrownia, mleczarnia, browar
    Soot = 2,      // ciemna sadza: huta, koksownia, cementownia, kotłownia węglowa
    Chemical = 3,  // opary: rafineria, zakład chemiczny, papiernia
}
```

**Jakość wyjścia jako funkcja — parametry, nie domknięcie.** Domknięcie nie jest ani serializowalne, ani moddowalne z poziomu danych; parametry są jedno i drugie.

```rust
pub struct QualityModel {
    pub base: u8,
    pub w_input: u8, pub w_skill: u8, pub w_tech: u8, pub w_machine: u8,   // wagi, suma <= 100
    pub cap_by_worst_input: bool,
    pub bonus_qc: u8,                    // kontrola jakości: +q, kosztem osobominut
}
```

Wzór, w całości na `i32`, z jednym dzieleniem `div_round_half_up` na końcu:

```
q_in     = średnia ważona masą jakości wejść (substytuty: minus quality_penalty)
q_raw    = base + (w_input*q_in + w_skill*skill + w_tech*tech + w_machine*condition) / 100
q_out    = clamp(q_raw, 0, 100)
jeśli cap_by_worst_input:  q_out = min(q_out, q_worst_input + bonus_qc)
```

`skill` — średnia ważona umiejętności obsady zmiany (z M3/M7). `tech` — poziom technologii zakładu (do M10 stała 50). `condition` — stan maszyny.

**Alokacja kosztu produkcji łącznej.** Rafineria z jednej tony ropy robi benzynę, asfalt i odpad. Rozdzielenie kosztu **wg masy** dałoby asfalt droższy od benzyny — absurd, który wywróci wszystkie ceny w dół łańcucha.

```rust
pub enum CostAllocation {
    ByMass,             // domyślnie, gdy jest jedno wyjście Main
    ByMarketValue,      // wg external_base_price wyjść — dla produkcji łącznej
}
```

`Waste` nie dostaje żadnego kosztu i **generuje** koszt utylizacji (`Good::disposal_cost`) albo przychód, jeśli ma odbiorcę (złom, makulatura, otręby na paszę). `SelfConsumed` nie dostaje kosztu i nie tworzy partii — od razu zasila licznik energii zakładu.

---

## Stan po zamknięciu M6a

Kryteria trzech pakietów i kryterium zamknięcia podfazy, z liczbami.

| Pakiet | Kryterium | Stan |
|---|---|---|
| WP1 | fala A w repo, `cargo test -p goods-graph` zielony (łącznie z testem negatywnym M2), CLI `goods-graph new <klucz>` generuje szkielet z szablonu kategorii i odpala walidację | **Spełnione.** Katalog: **90 towarów, 74 receptury, 59 kategorii** (było 70 / 62 / 0). `tools/goods-graph`: dziesięć testów, w tym `zapas_startowy_nie_jest_zrodlem` przeniesiony z M2 bez osłabienia. Reguły 1–4 są błędami ładowania, reguła 5 daje **19 ostrzeżeń** — jedno na towar, nie na parę (towar, receptura) |
| WP2 | `prop_mass_conservation` i `prop_no_negative_stock` zielone na scenariuszu z samym magazynem; partia dzielona 1000 razy nie gubi ani grama, ani grosza | **Spełnione.** `sim/supply/tests/warehouse.rs`: strumień stu tysięcy operacji z pełnym sprawdzeniem co tysiąc kroków, dwadzieścia tysięcy operacji na ciasnym slocie i `proptest` na losowym przeplocie. Test podziału jest dosłowny: 1 000 000 g za 999 999 gr rozdzielone tysiąc razy wychodzi co do grama i co do grosza |
| WP3 | przemiał zboża i frakcjonowanie ropy dają liczby z tabel §7 **dokumentu fazy** (±0 g); jakość wyjścia jest funkcją czystą | **Spełnione.** `milling_wheat_t550` i `refinery_crude_fractionation` domykają się do 1 000 000 g, `bakery_bread_wheat` do 165 800 g z ubytkiem 7 800 g jako `Evaporation`. `QualityModel::quality` jest funkcją czystą na `i32` z jednym dzieleniem; młyn przy zbożu q64 daje **mąkę q67**, bo sufit najgorszego wejścia przycina wynik |
| Podfaza | kryteria WP1–WP3; partia dzielona 1000 razy nie gubi ani grama, ani grosza | **Zamknięte.** |

**Odesłanie „§7 tego dokumentu" w kryterium WP3 wskazuje §7.1 i §7.2 dokumentu fazy** —
podfaza nie ma własnej sekcji 7, bo testy i kryteria akceptacji nie są dzielone na podfazy
(`K-17` pkt 3). Poprawione przy zamknięciu.

Dwanaście korekt wpisanych w przód jest w tabeli „Zmiany wpisane po M6a" dokumentu fazy
(`AD-1`…`AD-12`), sześć w `M6b-zaklad-i-transport.md` (`AE-1`…`AE-6`) i cztery
w `M6d-zloza-i-koniec-dostawcy-zewnetrznego.md` (`AF-1`…`AF-4`). Dwa rozstrzygnięcia
kontraktowe weszły do dokumentu nadrzędnego jako `K-33` (`GateKind` do `core`)
i `K-34` (`LossKind` i `NeedCategoryId` do `core`).
