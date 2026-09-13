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

## 4. Pakiety robocze

Kolejność: WP1 → WP2 → WP3 → WP4 → (WP5 ∥ WP6) → WP7 → WP8 → (WP9 ∥ WP10) → WP11 → WP12 → WP13 → WP15. WP14 rośnie równolegle od WP2 i domyka fazę.

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

### WP4 — Zakład jako model fizyczny
**Zależy od:** WP3.
**Opis.** `ProductionLine` (wydajność, wiek, `condition`, MTBF, `LineState`), `ProductionSchedule` (zmiany, plan szarż, przezbrojenia, konserwacja), `UtilityMeter` (prąd/gaz/woda/ciepło z fakturą miesięczną), `Dock` (rampa jako kolejka M/D/c z godzinami dostaw), `Emissions`. Jedna funkcja `advance_production(site, minutes)` używana przez wszystkie trzy poziomy LOD.
**Kryterium ukończenia:** linia bez prądu stoi; awaria wymaga części z magazynu lub zamówienia; przezbrojenie kosztuje czas i masę; test spójności LOD (mikro vs mezo) daje identyczne salda.
**Rozmiar: XL**

### WP5 — Zlecenia transportowe
**Zależy od:** WP2, M4 (`nav`, `traffic`).
**Opis.** `TransportOrder` i jego maszyna stanów, `VehicleRequirements` (nadwozie, chłodnia, cysterna, ADR, tonaż), flota własna vs. przewoźnik wynajęty, załadunek i rozładunek przez rampę, zdarzenia przybycia z M4, obsługa nieudanej dostawy. Rurociąg jako tryb bez pojazdu (przepustowość dobowa + awarie).
**Kryterium ukończenia:** `prop_no_teleport` zielony — partia zmienia lokację wyłącznie przez zlecenie albo ruch wewnątrz tego samego `SiteId`.
**Rozmiar: L**

### WP6 — Polityki zapasów i kaskada niedoboru
**Zależy od:** WP2, WP5.
**Opis.** `InventoryPolicy` (min/max, JIT, sezonowy), przegląd ciągły i okresowy, `ShortageCascade` jako maszyna stanów per `(SiteId, GoodId)` z `DecisionReason` na każdym przejściu.
**Kryterium ukończenia:** scenariusz „młyn stoi 5 dni" przechodzi przez wszystkie stopnie kaskady w udokumentowanej kolejności; każde przejście ma zapisany powód czytelny w karcie inspekcji.
**Rozmiar: M**

### WP7 — Rynek B2B: spot
**Zależy od:** WP5, M5 (`Offer`).
**Opis.** `Rfq`, `Quote`, okno zbierania ofert, indeks przestrzenny dostawców per `GoodId`, deterministyczne rozstrzygnięcie po funkcji celu. Koszt transportu wliczany w ocenę oferty.
**Kryterium ukończenia:** ten sam seed daje tę samą wybraną ofertę; zero iteracji po `HashMap`.
**Rozmiar: M**

### WP8 — Rynek B2B: kontrakty
**Zależy od:** WP7.
**Opis.** `SupplyContract`, `ContractPricing` (`Fixed` / `Indexed` / `Collar`), harmonogram dostaw z oknem godzinowym, kary za niedostarczenie i za nieodebranie, wypowiedzenie z okresem, statystyki realizacji zasilające reputację dostawcy.
**Kryterium ukończenia:** `prop_contract_penalty` — suma kar naliczonych równa sumie zapłaconych; indeksacja przelicza się całkowitoliczbowo bez dryfu.
**Rozmiar: M**

### WP9 — Import / eksport
**Zależy od:** WP8.
**Opis.** `TradeNode` (port, kolej, autostrada, rurociąg) z dobową przepustowością, kolejką i lead time; cena zewnętrzna rosnąca z wolumenem zakupów w oknie 30 dni; stub `TariffTable`; eksport jako zwykłe zlecenie transportowe **do** węzła (zajmuje ciężarówki i rampę) z ceną skupu niższą od importowej.
**Kryterium ukończenia:** import nie jest darmowym zaworem — test „zakup 10× dziennej przepustowości" kończy się kolejką i wzrostem ceny, nie natychmiastową dostawą; eksport przy wysokiej cenie zewnętrznej mierzalnie podnosi ceny lokalne.
**Rozmiar: M**

### WP10 — Wydobycie i wyczerpywanie złóż
**Zależy od:** WP4, M1 (geologia).
**Opis.** `Deposit` (masa pozostała, koncentracja, głębokość), rosnący koszt wydobycia, spadająca koncentracja (najlepsze żyły najpierw), sygnał wyczerpania do M7.
**Kryterium ukończenia:** `prop_deposit_monotone`; złoże wyczerpane w scenariuszu 50-letnim, zakład zamknięty, kaskada niedoboru odpala się w dół łańcucha.
**Rozmiar: S**

### WP11 — Migracja: koniec „zewnętrznego dostawcy" z M5
**Zależy od:** WP2, WP5, WP6. Szczegóły w §6.3.
**Kryterium ukończenia:** feature `infinite_supply` domyślnie wyłączony; test porównawczy przed/po migracji zielony; test zachowania masy włączony globalnie (do M5 sklepowy stub był jawnym wyjątkiem na liście).
**Rozmiar: M**

### WP12 — Magazyny, centra dystrybucyjne, konsolidacja
**Zależy od:** WP5, WP6.
**Opis.** `WarehouseRole::Distribution`, cross-docking, konsolidacja zleceń w trasę milk-run (heurystyka zachłanna Clarke–Wright ograniczona do 12 punktów — nie optymalizacja), koszt t·km i przepustowość rampy jako naturalna nagroda za DC.
**Kryterium ukończenia:** scenariusz „10 sklepów, 1 DC" ma niższy koszt dostawy na tonę niż „10 sklepów, dostawa bezpośrednia od producenta" — bez żadnej reguły to wymuszającej, wyłącznie z kosztów i kolejek.
**Rozmiar: M**

### WP13 — Panel łańcucha dostaw i śledzenie partii
**Zależy od:** WP8, WP12, `engine/ui`.
**Opis.** Graf dostawców i odbiorców z przepływami oraz oznaczeniem ryzyka, lista kontraktów z pokryciem, Gantt dostaw per zakład, tryb „śledź partię" z osią czasu i kosztem narastającym, nakładka „przepływ towaru Y" (dane do renderu z M1/M11).
**Kryterium ukończenia:** `trace_batch` na bochenku chleba zwraca ≥ 5 etapów z czasem, masą, jakością i kosztem; na litrze diesla ≥ 6 etapów aż do `Deposit`.
**Rozmiar: M**

### WP14 — Testy E2E, własnościowe, wydajnościowe, determinizm
**Zależy od:** rośnie od WP2.
**Opis.** Dwa łańcuchy referencyjne z liczbami (§7), osiem testów własnościowych, benchmarki criterion, dopisanie komponentów M6 do funkcji haszującej stan ECS.
**Kryterium ukończenia:** wszystko zielone przy budżecie wydajności z §7.4.
**Rozmiar: L**

### WP15 — Agregacja partii i budżet pamięci
**Zależy od:** WP2, WP14.
**Opis.** `BatchCoalesceSystem`, kubełkowanie jakości i daty przydatności, wyłączenie scalania dla partii śledzonych, kompaktowanie areny, twardy limit liczby partii z trybem awaryjnym.
**Kryterium ukończenia:** miasto 400 tys. mieści się w ≤ 600 tys. aktywnych partii i ≤ 64 MB pamięci gorącej.
**Rozmiar: M**

---

## 5. Projekt techniczny

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

### 5.5 Zakład fizyczny

```rust
pub struct ProductionLine {
    pub id: LineId, pub site: SiteId,
    pub machine_class: MachineClassId,
    pub recipe: Option<RecipeId>,          // aktualnie ustawiona
    pub nominal_throughput: Mass,          // masa wsadu na godzinę przy 100%
    pub age_minutes: u64,
    pub condition: Q,                      // spada z pracą, rośnie z konserwacją
    pub mtbf_hours: u32,                   // bazowe; efektywne = mtbf * condition/100
    pub power_draw: Energy,                // Wh/h przy pracy
    pub spare_part: GoodId,                // część potrzebna do naprawy — wpięcie w łańcuch dostaw
    pub state: LineState,
}

pub enum LineState {
    Idle,
    Setup      { until: SimMinute, to_recipe: RecipeId },
    Running    { recipe: RecipeId, started: SimMinute, ends: SimMinute, inputs: SmallVec<[BatchId;8]> },
    Broken     { since: SimMinute, cause: BreakCause },
    Maintenance{ until: SimMinute },
    Starved    { missing: GoodId },        // brak wejścia
    Blocked    { full: GoodId },           // pełny magazyn wyjściowy
}
pub enum BreakCause { Wear, NoPower, NoWater, NoStaff, Strike }
```

`Starved` i `Blocked` są osobnymi stanami, nie `Idle` — bo to one lądują w alertach pulpitu firmy i w wyjaśnieniu „dlaczego moja fabryka nie produkuje".

```rust
pub struct ProductionSchedule {
    pub shifts: [Option<Shift>; 3],
    pub plan: VecDeque<PlannedRun>,             // recipe, target_mass, earliest_start, priority
    pub maintenance_interval_hours: u32,
    pub next_maintenance: SimMinute,
}
pub struct Shift { pub from: SimMinute, pub to: SimMinute, pub headcount: u16, pub wage_multiplier_pct: u16 }
```

Przezbrojenie odpala się automatycznie, gdy `plan.front().recipe != line.recipe`: `LineState::Setup` na `recipe.setup.minutes` plus odpisanie `recipe.setup.scrap_mass` jako `LossKind::Setup`. Zmiana nocna kosztuje więcej (`wage_multiplier_pct`) — decyzję o jej uruchomieniu podejmuje M7, M6 tylko ją wykonuje.

```rust
pub struct Warehouse { pub site: SiteId, pub role: WarehouseRole, pub slots: Vec<SlotId> }
pub enum WarehouseRole { Input, Output, Distribution, Shelf, Backroom, Tank }

pub struct StorageSlot {
    pub id: SlotId, pub site: SiteId,
    pub class: StorageClass, pub hazard_mask: HazardMask,
    pub cap_mass: Mass, pub cap_volume: Volume,
    pub used_mass: Mass, pub used_volume: Volume,
    pub batches: Vec<BatchId>,             // utrzymywane posortowane po (expires_at, BatchId) — FEFO
}
```

Pojemność slotu wynika z powierzchni budynku (m² z M2) i klasy: `cap_volume = area_m2 * height_m * fill_factor(class)`. Silos i zbiornik mają `cap_volume` wprost z danych budynku.

```rust
pub struct Dock {                          // rampa — realne wąskie gardło (§7.3)
    pub site: SiteId,
    pub bays: u8,                          // stanowisk równolegle
    pub fixed_minutes: u16,                // podjazd, dokumenty
    pub minutes_per_tonne: u16,
    pub hours: OpeningHours,               // godziny dostaw (ograniczenie z §8.3)
    pub yard_capacity: u8,                 // ile pojazdów mieści plac poza stanowiskami
    pub queue: VecDeque<(SimMinute, u32)>, // klucz TOTALNY: (minuta przybycia, vehicle_entity_index)
    pub busy_until: [SimMinute; MAX_BAYS],
}
```

**Kontrakt z M4 — zakład trzyma kolejkę, M4 dowozi i odbiera czas zwolnienia.** M4 odmówił budowania osobnego modelu ruchu dla ciężarówek (drugi model to drugie miejsce, w którym może pęknąć tolerancja 0 mikro↔mezo) i słusznie — kolejka rampy jest moja, przejazd jest jego.

```rust
// M4 -> M6 przy dojeździe pojazdu na teren zakładu
pub struct VehicleArrivedAtSite { pub vehicle: VehicleId, pub site: SiteId,
                                  pub order: TransportOrderId, pub at: SimMinute }
// M6 -> M4 w odpowiedzi; M4 księguje oczekiwanie jako pozycję w rejestrze przejazdu
pub struct SiteDwellResponse { pub release_at: SimMinute, pub idle_fuel: Mass }
```

`release_at` wchodzi do księgi, czyli **dotyka pieniądza**, więc obowiązują cztery warunki brzegowe:

1. **Deterministyczne i niezależne od LOD.** Mikro odgrywa stojącą ciężarówkę wizualnie, ale **nie decyduje, kiedy odjedzie** — decyduje kolejka, identycznie w mikro i w mezo.
2. **Klucz kolejki musi być totalny:** `(minuta_przybycia, vehicle_entity_index)`. Wcześniejszy szkic używał `TransportOrderId`, co jest błędem — jeden pojazd może wieźć kilka zleceń (milk-run, §5.6), więc porządek po zleceniu nie jest funkcją i przy konsolidacji dałby remis rozstrzygany przypadkowo.
3. **Przepełnienie placu wylewa się na ulicę.** Pojazdy ponad `yard_capacity` zajmują pojemność przyuliczną krawędzi dostępowej i **biorą udział w zatorze**. To jedyny punkt, w którym moja kolejka dotyka sieci drogowej, i jest pożądany: zastawiona ulica pod źle zaprojektowaną hurtownią to czytelny sygnał dla gracza, że rampa jest za mała — dokładnie ta emergencja, o którą chodzi w §8.3.
4. **Strażnikiem jest test M4 `ramp_wait_lod_invariant`** (300 ciężarówek, 12 ramp, mikro vs mezo, tolerancja 0 na czas zwolnienia, paliwo postojowe i koszt). Niedeterministyczny model rampy po stronie M6 wywali CI, zanim rozejdzie się po saldach — projektujemy pod to od pierwszej wersji, nie dokładamy determinizmu później.

```rust

pub struct UtilityMeter {
    pub kind: UtilityKind,                 // Power | Gas | Water | Heat
    pub consumed: i64,                     // Wh lub ml, kumulacja od ostatniej faktury
    pub supplier: FirmId,
    pub tariff: Money,                     // za kWh / m³
    pub billed_until: SimMinute,
    pub cut_off: bool,                     // brak płatności / awaria sieci (M8) -> Broken{NoPower}
}
```

Faktura miesięczna trafia do `economy::book`. Emisje są akumulowane per `SiteId` i wystawiane przez `site_emissions()` — M8 je konsumuje, M6 nie liczy ich skutków.

### 5.6 Logistyka

```rust
pub struct TransportOrder {
    pub id: TransportOrderId,
    pub from: SiteId, pub to: SiteId,
    pub cargo: SmallVec<[BatchId; 8]>,
    pub mass: Mass, pub volume: Volume,
    pub requires: VehicleRequirements,
    pub ready_at: SimMinute, pub due_at: SimMinute,     // "kiedy najpóźniej"
    pub carrier: Carrier,
    pub price: Money,
    pub state: TransportOrderState,
    pub reason: DecisionReason,
}

pub struct VehicleRequirements {
    pub body: BodyType,                    // Box | Reefer | Tanker | Tipper | Container | Flatbed
    pub min_payload: Mass, pub min_volume: Volume,
    pub adr: bool,                         // wymaga kierowcy z uprawnieniami
    pub storage_class: StorageClass,       // chłodnia w drodze
}

pub enum Carrier { OwnFleet(FirmId), Hired { firm: FirmId, quote: Money }, Pipeline(PipelineId), Unassigned }

pub enum TransportOrderState {
    Draft, Tendered { closes: SimMinute },
    Assigned { vehicle: VehicleId, pickup_eta: SimMinute },
    LoadingQueue, Loading { until: SimMinute },
    EnRoute { eta: SimMinute },
    UnloadingQueue, Unloading { until: SimMinute },
    Done, Failed(FailReason),
}
pub enum FailReason { NoCarrier, NoVehicle, NoDriver, Expired, Refused, RouteBlocked, Accident }
```

Cykl życia: `ReplenishmentSystem` albo `ContractDeliverySystem` tworzy `Draft` → `TransportDispatchSystem` konsoliduje i przypisuje flotę albo ogłasza przetarg (`Tendered`) → `Assigned` → kolejka rampy nadawcy → `EnRoute` (M4 liczy trasę i zwraca zdarzenie przybycia) → kolejka rampy odbiorcy → `Done`. **Zmiana `Batch::location` wolna jest wyłącznie w obrębie tego cyklu** — tego pilnuje test `prop_no_teleport`.

**Konsolidacja (milk-run).** Zlecenia z tego samego nadawcy, w tym samym oknie czasowym, do celów w promieniu 4 km, mieszczące się w jednym pojeździe, łączone są heurystyką zachłanną Clarke–Wright z twardym limitem 12 punktów na trasę. To wystarcza, żeby centrum dystrybucyjne opłacało się samo z siebie, i nie kosztuje zauważalnego CPU. Optymalizator VRP jest niepotrzebny — a gdyby kiedyś pomiar wykazał inaczej, trasa jest jedną funkcją do podmiany.

### 5.7 Polityki zapasów i kaskada niedoboru

```rust
pub enum InventoryPolicy {
    MinMax    { reorder_point: Mass, target: Mass },
    Jit       { lead_minutes: u32, safety_minutes: u32 },
    Seasonal  { base: MinMaxParams, curve: SeasonCurveId },
}
pub struct InventoryRule { pub good: GoodId, pub policy: InventoryPolicy,
                           pub review: Review, pub preferred: PreferredSource }
pub enum Review { Continuous, Periodic { every_minutes: u32, at_minute: u32 } }
pub enum PreferredSource { Contract(ContractId), Spot, Import, Any }
```

Sklepy używają `Periodic` dobowego (jedna dostawa dziennie jest tańsza niż pięć); zakłady ciągłe — `Continuous`. `Seasonal` istnieje dla żniw i zimy: krzywa mnoży `reorder_point` w oknie roku.

**Kalendarz (K-1, K-15) — wiążący.** Rok ma **360 dni (12 × 30)**, tydzień ma **7 dni i dryfuje względem miesiąca**; `DayOfWeek` pochodzi z `SimCalendar` w `engine/core`, nie jest liczony lokalnie z `SimMinute`. Dotyka to M6 w czterech miejscach i w każdym trzeba to uwzględnić od pierwszej linii kodu, bo późniejsza poprawka to migracja danych:

- **`SeasonCurveId`** — krzywa ma 12 punktów po 30 dni, nie 365 dni. Żniwa wypadają na stałym dniu roku.
- **`expires_at`** — liczone w minutach od `produced_at`, więc kalendarz nie wpływa na arytmetykę, ale wpływa na prezentację daty w UI i na `expires_at / 1440` w kluczu agregacji (§7.4).
- **Okno 30 dni** w `TradeGood::window_reference` to dokładnie jeden miesiąc kalendarzowy — wygodny zbieg, który warto zachować.
- **Dryf tygodnia** jest jedynym powodem, dla którego harmonogram dostaw `DeliverySchedule` musi trzymać okres w minutach, a nie „w dniach miesiąca": dostawa „w każdy wtorek" i dostawa „1. i 15. dnia miesiąca" to dwa różne rytmy, które się rozjeżdżają. Sklepy zamawiają tygodniowo, faktury za media idą miesięcznie — i te dwa cykle mają się nie synchronizować.

**Kaskada niedoboru (§8.4)** — maszyna stanów per `(SiteId, GoodId)`, przeliczana `EveryHour`, sterowana **pokryciem** `coverage = stock / consumption_rate` w godzinach:

```rust
pub enum ShortageStage {
    Ok,
    Buffer      { coverage_min: u32 },        // < 8 h: zużywaj rezerwę, alert w pulpicie
    Throttled   { pct: u8 },                  // < 4 h: produkcja proporcjonalnie obniżona
    SpotSearch  { rfq: RfqId },               // równolegle z Throttled: zapytanie ofertowe, drożej
    Importing   { eta: SimMinute },           // < 2 h i spot bez wyniku: import, wolniej
    Substituted { alt: GoodId, quality_loss: u8 },
    Halted      { since: SimMinute },         // 0: linia Starved, koszty stałe lecą dalej
}
```

Kolejność prób jest dokładnie ta z PRD: bufor → obniżenie produkcji → spot → import → substytut → postój. Substytut jest przed postojem, a nie przed importem, bo psuje jakość produktu i powinien być przedostatnią deską ratunku. Każde przejście zapisuje `DecisionReason::Shortage { good, from, to, coverage_minutes }` — widoczny w karcie inspekcji zakładu (dok. 00 §7).

Po stronie sklepu (M5) pusta półka to `OutOfStock` — mieszkaniec kupuje substytut, idzie do konkurencji albo rezygnuje, i **zapamiętuje** („u nich nie było"). M6 tylko dostarcza sygnał; pamięć jest w M3/M5.

### 5.8 Rynek B2B

```rust
pub struct Rfq {
    pub id: RfqId, pub buyer: FirmId, pub deliver_to: SiteId,
    pub good: GoodId, pub mass: Mass, pub min_quality: Q,
    pub needed_by: SimMinute, pub radius_m: u32,
    pub opened_at: SimMinute, pub closes_at: SimMinute,   // domyślnie +60 min gry
    pub incoterm: WhoTransports,
    pub quotes: Vec<QuoteId>,
}
pub struct Quote {
    pub id: QuoteId, pub rfq: RfqId,
    pub seller: FirmId, pub from_site: SiteId,
    pub mass_available: Mass, pub quality: Q,
    pub price: Money,                    // za tonę / za 1000 szt., loco magazyn sprzedawcy
    pub transport: Option<Money>,        // gdy sprzedawca organizuje dostawę
    pub ready_at: SimMinute, pub eta: SimMinute,
    pub valid_until: SimMinute,
}
pub enum WhoTransports { Seller, Buyer }
```

Rozstrzygnięcie: minimalizacja `score = price*mass + transport + late_penalty*max(0, eta − needed_by) + quality_penalty*(min_quality − quality)`, remis rozstrzygany po `(seller.entity_index, quote.id)` — nigdy po kolejności wpisu do kolekcji. Dostawcy szukani przez indeks przestrzenny per `GoodId` (grid dzielnic, dok. 00 §3 i PRD §17.5) → typowo 3–12 kandydatów, nie wszystkie firmy w mieście.

```rust
pub struct SupplyContract {
    pub id: ContractId, pub buyer: FirmId, pub seller: FirmId,
    pub good: GoodId, pub min_quality: Q,
    pub deliver_to: SiteId, pub incoterm: WhoTransports,
    pub schedule: DeliverySchedule,       // co ile minut, jaka masa, okno godzinowe
    pub pricing: ContractPricing,
    pub penalty: Penalty,
    pub valid_from: SimMinute, pub valid_to: SimMinute,
    pub notice_minutes: u32,
    pub fulfilled_mass: Mass, pub missed_mass: Mass, pub late_deliveries: u32,
}

pub enum ContractPricing {
    Fixed   { price: Money },
    Indexed { base: Money, index: GoodId, ref_price: Money, pass_through_pct: u8 },
    Collar  { base: Money, index: GoodId, ref_price: Money, floor: Money, cap: Money },
}
pub struct Penalty { pub per_tonne_missed: Money, pub cap_pct: u8, pub grace_minutes: u32 }
```

**Baza cenowa (K-7, dok. 00 §4a) — wiążące.** Mechanizm ofert jest **jeden**, wspólny z M5; M6 **nie tworzy drugiego typu oferty dla B2B**. `Quote` jest widokiem na `Offer` z `price_basis: PriceBasis::NetB2B` — rozróżnienie robi jawne pole, nie osobny typ.

Cena w `Offer` to zawsze kwota płacona przez kupującego: w detalu brutto (`GrossRetail`), w hurcie netto (`NetB2B`, VAT jest dla firmy przelotowy). **Cały rynek B2B M6 — `Quote::price`, `ContractPricing` we wszystkich trzech wariantach, `import_quote`, `export_price`, `TransportOrder::price` — operuje netto.**

Konsekwencje dla łańcucha: `Batch::cost_total` jest kosztem netto i marże liczą się od netto. **Cła i akcyza nie modyfikują ceny w ofercie** — doliczają się przez `ChargeRegistry` po stronie M8. Znaczy to, że `TradeGood::tariff_class` jest wyłącznie etykietą przekazywaną do `ChargeRegistry`, a `import_quote` zwraca cenę netto plus osobno rozpisane obciążenia, nigdy kwotę „z cłem w środku". W łańcuchach referencyjnych (§7.1, §7.2) VAT i akcyza pojawiają się wyłącznie w ostatnim wierszu, na sprzedaży detalicznej.

Cena indeksowana przelicza się całkowitoliczbowo: `price = base + (index_now − ref_price) * pass_through_pct / 100`, a `Collar` dodatkowo klamruje do `[floor, cap]`. `index_now` to średnia cena spot towaru indeksowego w mieście z ostatnich 7 dni — liczona raz dziennie, cache'owana, iterowana po posortowanym kluczu.

Statystyki `fulfilled_mass` / `missed_mass` / `late_deliveries` są jedynym wyjściem M6 do reputacji dostawcy; jej interpretacja i pamięć relacji to M7/M10.

### 5.9 Import, eksport, węzły graniczne

```rust
pub struct TradeNode {
    pub id: TradeNodeId, pub kind: TradeNodeKind,   // Port | Rail | Highway | PipelineHead | Airport
    pub site: SiteId,                               // fizyczne miejsce w mieście
    pub capacity_per_day: Mass,
    pub used_today: Mass, pub backlog: Mass,
    pub base_lead_minutes: u32,                     // dni–tygodnie
    pub goods: Vec<TradeGood>,
}
pub struct TradeGood {
    pub good: GoodId,
    pub base_price: Money,                 // za tonę; trend epoki i zdarzenia globalne -> M8/M10
    pub elasticity_permille: u32,          // wzrost ceny przy dużych zakupach
    pub window_reference: Mass,            // wolumen odniesienia (30 dni)
    pub bought_window: Mass,
    pub export_spread_pct: u8,             // cena skupu jako % ceny importowej (typowo 78–90)
    pub tariff_class: TariffClassId,
}
```

- **Cena importu:** `p = base_price * (1000 + elasticity_permille * bought_window / window_reference) / 1000`. Kupno 10× wolumenu odniesienia przy `elasticity = 300` podnosi cenę o 300%. Import **nie jest** darmowym zaworem bezpieczeństwa.
- **Czas:** `lead = base_lead_minutes * (1 + backlog / capacity_per_day)`. Przeciążony węzeł wydłuża kolejkę wszystkim.
- **Cło:** `TariffTable` — w M6 stub z `data/trade/tariffs.ron`, M8 podmienia na politykę miasta i epoki.
- **Eksport:** producent porównuje `p_export = base_price * export_spread_pct/100 − koszt_transportu_do_węzła` z najlepszą ceną lokalną. Jeśli eksport wygrywa, powstaje zwykły `TransportOrder` **do** węzła — zajmuje ciężarówkę, kierowcę i rampę, i **zabiera masę z lokalnej podaży**. Wzrost cen w mieście jest emergentny, nie zaprogramowany. To jest cała mechanika „drenażu" z §8.5.

### 5.10 Wydobycie

**K-13 — `TerrainQuery` z M1 jest jedynym źródłem prawdy o terenie.** M6 **nie trzyma własnej kopii geologii** i nie odejmuje voxeli. Złoże jest własnością M1; M6 konsumuje trzy metody i wnosi jedną rzecz własną — mapowanie `ResourceKind → GoodId`.

```rust
// Kontrakt M1 (K-13). Bilans złoża prowadzony jest w GRAMACH, w arytmetyce całkowitej.
// Voxele są POCHODNĄ WIZUALNĄ — nigdy nie są źródłem prawdy o pozostałej masie.
impl Deposit {
    pub fn extract(&mut self, want: Mass) -> Mass;   // zwraca faktycznie wydobyte (może być < want)
    pub fn remaining(&self) -> Mass;
    pub fn voxels_for(&self, m: Mass) -> VoxelDelta; // do renderu wyrobiska, M1/M11
}

// Własność M6: co znaczy dany surowiec w katalogu towarów.
pub fn good_for(kind: ResourceKind) -> GoodId;       // ResourceKind::Oil -> raw_crude_oil
```

Strona M6 to wyłącznie ekonomia wydobycia — koszt, koncentracja, decyzja o eksploatacji:

```rust
pub struct MiningSite {
    pub site: SiteId, pub deposit: DepositId, pub good: GoodId,
    pub concentration_pct: u8,             // ile masy użytecznej na tonę urobku
    pub depth_m: u16,
    pub daily_capacity: Mass,
}
```

`concentration_pct` i `depth_m` pochodzą z `TerrainQuery` przy założeniu zakładu i są kopiowane raz — to parametry ekonomiczne, nie stan terenu. Stan terenu żyje wyłącznie po stronie M1 i jest odpytywany przez `remaining()`.

Koszt wydobycia, przeliczany `EveryDay`:

```
depletion  = 1000 − 1000*remaining/initial                      // promile wyczerpania
conc_eff   = concentration_pct * cbrt_lut(remaining/initial)    // najlepsze żyły najpierw
cost/tonę  = base * (100 + depth_m/20) / 100 * 100/conc_eff * (1000 + 2*depletion²/1000) / 1000
```

`cbrt_lut` to tablica 64 wartości — deterministyczna i tania, bez floatów w ścieżce kosztowej. **K-6** zakazuje `f64::ln` / `exp` / `powf` ze std w kodzie symulacji; gdyby gdziekolwiek w `sim/supply` potrzebna była funkcja przestępna (krzywa sezonowa, wygładzanie prognozy), pochodzi z `core::det_math`, nigdy ze `std`. W ścieżkach pieniądza, masy i stanów magazynowych i tak obowiązuje arytmetyka całkowita (dok. 00 §2).

Gdy `remaining() == 0`: zakład dostaje `DepositExhausted`, linie przechodzą w `Starved`, a decyzję (zamknięcie, przebranżowienie, bankructwo) podejmuje M7.

### 5.11 Systemy ECS i częstotliwości

| System | Częstotliwość | Zadanie |
|---|---|---|
| `ProductionTickSystem` | EveryMinute | postęp szarż, pobór mediów, losowanie awarii, zakończenie szarży → nowe partie |
| `DockQueueSystem` | EveryMinute | kolejki ramp, załadunek i rozładunek |
| `TransportArrivalSystem` | EveryMinute | odbiór zdarzeń przybycia z `sim/traffic`, przejście stanu zlecenia |
| `SpoilageSystem` | EveryMinute (kopiec) | wygaszanie partii wg `expires_at`, przeceny, `LossKind::Expired` |
| `ShortageCascadeSystem` | EveryHour (staggered) | pokrycie zapasu, przejścia `ShortageStage` |
| `ReplenishmentSystem` | EveryHour (staggered) | polityki zapasów → `TransportOrder::Draft` |
| `RfqSystem` | EveryHour | otwieranie RFQ, zbieranie i rozstrzyganie ofert |
| `ContractDeliverySystem` | EveryHour | harmonogram dostaw kontraktowych, naliczanie kar |
| `TransportDispatchSystem` | EveryHour | konsolidacja milk-run, przydział flot, przetargi przewozowe |
| `ScheduleSystem` | EveryHour | zmiany, kolejka szarż, przezbrojenia |
| `MaintenanceSystem` | EveryDay | konserwacja, zużycie maszyn, zamawianie części |
| `ImportExportSystem` | EveryDay | przepustowość węzłów, ceny zewnętrzne, decyzje eksportowe |
| `DepositDepletionSystem` | EveryDay | wyczerpywanie złóż, przeliczenie kosztu wydobycia |
| `UtilityBillingSystem` | EveryMonth | faktury za media → `economy::book` |
| `BatchCoalesceSystem` | EveryDay (staggered) | scalanie partii, kompaktowanie areny |

**Rozpraszanie obciążenia (staggering):** systemy godzinowe i dobowe nie liczą wszystkiego naraz — zakład o indeksie `i` obsługiwany jest w minucie `i % 60` (godzinowe) i `i % 1440` (dobowe). Deterministyczne, bo po indeksie encji, a nie po zegarze. To spłaszcza szczyt CPU z ~12 ms raz na godzinę do ~0,2 ms co minutę.

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
| `export_drains` | Wzrost ceny zewnętrznej o 40% → mierzalny odpływ masy i wzrost cen lokalnych, bez zaprogramowanej reguły |
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
| **D3** | Źródło cen referencyjnych do `CostAllocation::ByMarketValue` | Statyczne `external_base_price` z katalogu (proste, stabilne) czy krocząca średnia z rynku lokalnego (realistyczne, ale ze sprzężeniem koszt→cena→koszt)? Proponuję statyczne w M6, przegląd w M7 | M5, M7 |
| **D4** | Właściciel `Dock` i decyzji o rozbudowie rampy | Proponuję: dane i kolejka w `sim/supply`, decyzja inwestycyjna w `sim/firms` (M7) | M7 |
| **D5** | `HazardClass` a przepisy miejskie (ADR przez centrum, godziny dostaw) | M6 czyta z `data/`, M8 podmienia na politykę miasta. Trzeba uzgodnić kształt `OpeningHours` i stref, żeby M8 nie musiał przepisywać `Dock` | M8 |
| **D6** | Czy eksport zajmuje realne pojazdy i rampę | Proponuję **tak** — bez tego „drenaż podaży" jest fikcją księgową. Koszt CPU do zmierzenia w `perf_400k`; plan B: eksport w mezo | M4, M7 |
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
