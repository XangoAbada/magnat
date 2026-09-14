# 00 — Konwencje i kontrakty wspólne (dokument nadrzędny)

Status: obowiązujący dla wszystkich faz M0–M12.
Źródło: `PRD_Magnat.md` §16, §17, §18.

Ten dokument ustala decyzje, które **nie mogą być podejmowane osobno w każdej fazie**.
Jeśli plan fazy potrzebuje czegoś, co jest tu zdefiniowane — odwołuje się do tego dokumentu,
nie redefiniuje. Jeśli faza potrzebuje zmiany kontraktu — zgłasza to w sekcji
„Decyzje otwarte" swojego dokumentu, a zmiana ląduje tutaj.

---

## 1. Indeks faz i własność modułów

Każdy crate ma **jednego właściciela** (fazę, która go tworzy). Inne fazy mogą go rozszerzać,
ale nie projektują go od nowa.

| Faza | Dokument | Crate'y tworzone (właściciel) | Crate'y rozszerzane |
|---|---|---|---|
| M0 | `M0-fundament-silnika.md` | `engine/core`, `engine/ecs`, `engine/jobs`, `engine/io` (minimalny), `engine/devtools` (szkielet), `tools/headless` | — |
| M1 | `M1-swiat-statyczny.md` | `engine/voxel`, `engine/render`, `sim/world` (teren, hydrologia, klimat) | `core`, `ecs`, `devtools` |
| M2 | `M2-miasto-statyczne.md` | `engine/spatial` | `sim/world` (drogi, strefy, parcele, budynki), `render` (nakładki) |
| M3 | `M3-ludzie-i-dzien.md` | `sim/agents`, `engine/ui` (szkielet + karta inspekcji) | `sim/world` (populacja) |
| M4 | `M4-ruch.md` | `engine/nav`, `sim/traffic` | `sim/agents` (wybór środka transportu) |
| M5 | `M5-gospodarka-detaliczna.md` | `sim/economy`, `tools/balansator` | `sim/agents` (decyzja zakupowa), `sim/firms` (szkic sklepu) |
| M6 | `M6-lancuch-dostaw.md` | `sim/supply` | `sim/economy` (rynek B2B), `sim/firms` (produkcja) |
| M7 | `M7-firmy-ai-i-rynek-pracy.md` | `sim/firms` (pełny), **`sim/policy`** (silnik reguł — K-11; język projektuje M9) | `sim/economy` (moduł `labor`), konsumuje `sim/macro` |
| M8 | `M8-miasto-jako-aktor.md` | `sim/city`, `sim/events` | `sim/traffic` (sieci przesyłowe) |
| M9 | `M9-gracz-kariera.md` | `game/`, `engine/ui` (panele biznesowe, automatyzacja polityk) | wszystkie `sim/*` (hooki gracza) |
| M10 | `M10-glebia.md` | `sim/macro` (pełny, historia „na sucho") | `sim/firms` (marka, R&D, giełda, media), `sim/economy` |
| M11 | `M11-prezentacja.md` | `engine/audio`, **`sim-snapshot`** (jedyny kanał sim → render; osobny crate, żeby zakaz mutacji był egzekwowany grafem zależności, a nie regulaminem) | `engine/render`, `engine/voxel` (LOD, animacje, wnętrza) |
| M12 | `M12-skala-i-jakosc.md` | `engine/script` (modding) | `engine/io` (pełny), wszystkie (profilowanie, pamięć, lokalizacja) |

`engine/io` w wersji minimalnej (snapshot ECS + hash stanu) powstaje w M0; pełne wersjonowanie
schematu, migracje i zapis w tle — M12.
`engine/devtools` rośnie przyrostowo: właścicielem szkieletu jest M0, każda faza dodaje własny inspektor.

---

## 2. Typy bazowe (`engine/core`) — kontrakt nienegocjowalny

```rust
// Pieniądz: zawsze i64 w groszach. Nigdy f32/f64, nigdy w komponentach jako float.
pub struct Money(pub i64);              // 1 = 1 grosz

// Czas symulacji
pub struct SimMinute(pub u64);          // minuty od startu świata (tick ekonomiczny = 1)
pub struct SimInstant(pub u64);         // milisekundy czasu gry (tick ruchu mikro = 100 ms)
pub struct Tick(pub u64);               // licznik ticków ekonomicznych, monotoniczny

// Wielkości fizyczne — całkowitoliczbowe
pub struct Mass(pub i64);               // gramy
pub struct Volume(pub i64);             // mililitry
pub struct Energy(pub i64);             // watogodziny
pub struct Qty(pub i64);                // milisztuki (sztuki × 1000)

// Skale
pub struct Q(pub u8);                   // jakość, zaspokojenie, umiejętność: 0..=100
pub struct Mood(pub i8);                // -100..=100

// Identyfikatory: typowane newtype'y nad indeksem ECS z generacją
pub struct CitizenId(pub Entity);
pub struct HouseholdId(pub Entity);
pub struct FirmId(pub Entity);
pub struct SiteId(pub Entity);          // zakład
pub struct BuildingId(pub Entity);
pub struct ParcelId(pub Entity);
pub struct VehicleId(pub Entity);
// UWAGA (K-16): partie i oferty NIE są encjami ECS — to uchwyty do dedykowanych aren.
pub struct BatchId { index: u32, generation: NonZeroU32 }   // arena partii (sim/supply)
pub struct OfferId { index: u32, generation: NonZeroU32 }   // arena ofert  (sim/economy)
pub struct ContractId(pub Entity);
pub struct DistrictId(pub u16);
pub struct GoodId(pub u16);             // indeks do katalogu danych, stabilny w obrębie wersji danych
pub struct RecipeId(pub u16);
pub struct JobRoleId(pub u16);
```

Zasady:
- Arytmetyka pieniądza: `checked_add`/`checked_mul` + jawne zaokrąglanie (`div_round_half_up`),
  nigdy niejawne obcinanie. Podział kwoty między N stron musi sumować się do oryginału
  (reszta trafia do pierwszego wg ustalonego porządku).
- Floaty **wolno** stosować w geometrii, renderze, fizyce ruchu i funkcjach użyteczności.
  **Nie wolno** w: pieniądzu, księgowości, stanach magazynowych, podatkach.
- Każdy float, którego wynik wpływa na stan trwały, musi być sumowany w ustalonej kolejności
  (iteracja po posortowanym kluczu, nigdy po `HashMap`).

---

## 3. Determinizm — reguły wiążące wszystkie fazy (PRD §18.2)

1. **RNG:** `xoshiro256++`, strumień wyprowadzany funkcją czystą:
   `rng(world_seed, StreamId, entity_index, tick)`. Brak globalnego stanu RNG.
   `StreamId` to enum w `core` — faza dopisuje własne warianty, nigdy nie zmienia istniejących wartości.
2. **Kolekcje:** zakaz iterowania po `std::collections::HashMap`/`HashSet` w kodzie symulacji.
   Dozwolone: `Vec`, `BTreeMap`, własna `SeededMap` o deterministycznej kolejności.
3. **Równoległość:** wynik nie może zależeć od kolejności ukończenia jobów.
   Redukcje — deterministyczne fork-join (składanie po indeksie chunka, nie po kolejności zakończenia).
4. **Mutacje strukturalne:** wyłącznie przez bufory komend, aplikowane w punktach synchronizacji,
   posortowane po `(SystemId, entity_index)`.
5. **Brak zależności od czasu rzeczywistego** w kodzie symulacji (żadnego `Instant::now()`).
6. **Test kontraktowy:** hash stanu ECS co 1000 ticków; dwa przebiegi tego samego seeda =
   identyczny ciąg hashy. Każda faza dopisuje swoje komponenty do funkcji haszującej —
   to element Definition of Done.

---

## 4. Architektura czasu i LOD (PRD §17.1, §17.4)

- Tick ekonomiczny: **1 minuta gry**. System deklaruje częstotliwość:
  `EveryMinute`, `EveryHour`, `EveryDay`, `EveryMonth`.
- Tick ruchu mikro: **100 ms gry**, tylko dla encji w LOD Mikro.
- Render: niezależny, interpolacja z podwójnie buforowanego snapshotu.
- **Zasada LOD** (doprecyzowana po analizie M4 — poprzednia wersja była niewykonalna dla makro):

  **Mikro ↔ mezo: tolerancja 0.** Wynik ekonomiczny nie zależy od poziomu. Każdy system działający
  w mikro ma mezo-odpowiednik dający ten sam czas, to samo zużycie i **ten sam koszt pieniężny**.
  Mezo jest źródłem prawdy; mikro jest wizualizatorem bez prawa zapisu do stanu ekonomicznego.
  Uzasadnienie: zbiór encji w mikro zależy od kamery gracza, a kamera nie wchodzi do hasha stanu —
  gdyby mikro wpływało na ekonomię, obrót kamerą zmieniałby saldo gospodarstwa domowego
  i determinizm (§3) upadałby.

  **Makro ↔ mezo: kontrakt słabszy, ale z twardym rdzeniem.** Makro agreguje (dzielnica × klasa),
  więc równość co do grosza jest z definicji nieosiągalna. Obowiązuje za to:
  1. **Zachowanie pieniądza i masy — tolerancja 0.** Makro nie tworzy ani nie niszczy
     ani grosza, ani grama. To nie podlega negocjacji.
  2. Odchylenie agregatów (ceny, wolumeny, zatrudnienie) ≤ 0,5% w skali miesiąca gry —
     **wyłącznie dla agregatów o liczebności n ≥ 500**. Dla mniejszych zbiorów, a w szczególności
     dla pojedynczej firmy, błąd rzędu 3–12% jest nieusuwalny (wariancja rozkładu wielomianowego)
     i nie jest kwestią kalibracji. Konsekwencja wiążąca dla konsumentów makro: wynik `what_if()`
     wolno używać **porównawczo** (uporządkowanie wariantów z marginesem), nigdy jako liczby
     bezwzględnej w regule progowej ani jako wartości pokazanej graczowi. Model deklaruje własny
     błąd razem z wynikiem i ma test na uczciwość tej deklaracji w obie strony.
  3. **Brak dryfu systematycznego:** błąd nie może się kumulować w jedną stronę —
     test 100 lat gry porównuje trajektorie makro i mezo.
  4. Przejście makro→mezo odtwarza stany indywidualne deterministycznie z seeda i tożsamości.

  Właściciele: M4 (mikro↔mezo), M10 (makro, `sim/macro`), M12 (tryb 50× i test długich sesji).

---

## 4a. Rozstrzygnięcia koordynacyjne (wiążące, dopisywane w trakcie)

Decyzje, których pojedyncza faza nie mogła podjąć sama. Numeracja `K-n`.

| # | Sprawa | Rozstrzygnięcie | Dotyczy |
|---|---|---|---|
| K-1 | Kalendarz gry: 360 dni (12×30) vs gregoriański | **360 dni, 12 miesięcy × 30.** Miesiąc jest jednorodną jednostką tickową, co upraszcza podatki, wybory, sezony i amortyzację. Brak lat przestępnych. Data wyświetlana graczowi może być kosmetycznie mapowana na kalendarz realny, ale symulacja liczy w 360 | M0 (właściciel `SimCalendar`), M1, M8, M10 |
| K-2 | Właściciel grafu pieszego | **`engine/nav` należy do M4.** M3 tworzy minimalny graf pieszy (odległości i czasy dojścia) za tym samym interfejsem, M4 zastępuje go pełną implementacją i dokłada routing multimodalny. M3 nie projektuje własnego, równoległego API | M3, M4 |
| K-3 | Bootstrap GPU | M0 buduje `wgpu` w `tools/voxelview` jako rusztowanie do wyrzucenia; **M1 przenosi `init_gpu()` do `engine/render`** i od tego momentu jest właścicielem urządzenia | M0, M1 |
| K-4 | Zakresy `StreamId` | Rezerwacja blokami po 20 per faza: M0 0–19, M1 100–119, M2 120–139, M3 140–159, M4 160–179, M5 180–199, M6 200–219, M7 220–239, M8 240–259, M9 260–279, M10 280–299, M11 300–319, M12 320–339. Fazie, której 20 wartości nie wystarcza, przysługuje **blok nadmiarowy `baza + 1000`** (M2: 1120–1219) — bez przenumerowywania czegokolwiek. Wartości raz nadane są niezmienne, bo zmiana numeru strumienia zmienia każdy świat wygenerowany wcześniej z tego samego seeda | wszystkie |
| K-6 | Funkcje przestępne w symulacji | **Podstawowa arytmetyka `f64` (`+ − × ÷`, `sqrt`) jest deterministyczna międzyplatformowo** — IEEE-754 wymaga poprawnego zaokrąglenia, a Rust nie stosuje fast-math ani automatycznej kontrakcji do FMA. Używać jej wolno (z zastrzeżeniem §2: nie w pieniądzu) i nie ma powodu uciekać od niej do fixed-point. Niedeterminizm wnosi wyłącznie **libm w funkcjach przestępnych**, dlatego: **zakaz `f64::ln`/`exp`/`powf`/`tanh`/`atan2` i pokrewnych ze std w kodzie symulacji**; `engine/core` dostarcza `core::det_math` z własnymi implementacjami o udokumentowanej dokładności (≤2 ULP), sumowaniem w kolejności indeksów i bez `mul_add`. Egzekwowane lintem `clippy.toml` + `-D warnings` w CI. Faza potrzebująca nowej funkcji dopisuje ją **do `det_math`** z wektorem testowym, nigdy lokalnie przez `#[allow]`. Dozwolone bez ograniczeń w renderze, UI i audio. Właściciel: M0 | wszystkie fazy z funkcjami użyteczności: M5, M7, M9, M10 |
| K-7 | Cena w ofercie: brutto czy netto | **Cena w `Offer` to zawsze kwota, którą faktycznie płaci kupujący.** Dla detalu (B2C) jest to cena brutto — mieszkaniec porównuje w funkcji użyteczności to, co realnie wyjmie z portfela, i tak też widzi ją w sklepie. Dla hurtu (B2B) jest to cena netto, bo VAT jest dla firmy przelotowy. Rozróżnienie niesie pole `Offer.price_basis: PriceBasis{GrossRetail, NetB2B}`, a nie domysł z kontekstu. VAT wyodrębnia się przy rozliczeniu transakcji do `Transaction.tax` (funkcja `vat_from_gross` M8). Konsekwencja dla gracza jest zamierzona: podwyżka VAT-u uderza w popyt natychmiast, bo zmienia cenę widzianą przez klienta | M5 (właściciel `Offer`), M6, M8, M9 |
| K-8 | Wspólne słowniki domenowe | **Enumy używane przez więcej niż jedną fazę mieszkają w `engine/core`, nie w crate'cie tej fazy, która ich użyła pierwsza.** Dotyczy co najmniej: `UtilityKind` (M5 używa w `TxKind::Utility`, zanim M8 zbuduje sieci), `TransportMode`, `PlaceRef`, `NeedKind`, `ActivityKind`, `DecisionReason` (K-12). Powód jest twardy: bez tego powstaje cykl zależności `sim/agents` → `sim/traffic` → `sim/economy` → `sim/agents`, którego Cargo nie skompiluje, a obejście przez duplikat enuma rozjeżdża się przy pierwszej zmianie. `core` nie dostaje przy tym logiki — wyłącznie słownik i konwersje | M0 (właściciel), M3, M4, M5, M8 |
| K-9 | Strajki i związki zawodowe | Mechanika należy do **M10** (związki, negocjacje, formowanie żądania). M8 dostarcza wyłącznie wyzwalacz zdarzeniowy i skutki miejskie. M7 dostarcza dane o płacach i zyskach. Żadna z tych faz nie implementuje negocjacji po swojemu | M7, M8, M10 |
| K-10 | Upadłość z tytułu zaległości podatkowych | Postępowanie upadłościowe prowadzi **M7** (jest właścicielem `Bankruptcy`). M8 jest tylko wierzycielem zgłaszającym roszczenie — nie ma własnej ścieżki likwidacji firmy | M7, M8 |
| K-11 | Silnik reguł: jeden dla gracza i dla AI | **Korekta macierzy z §1.** PRD §6.3 obiecuje graczowi „ten sam zestaw narzędzi co AI", więc nie mogą istnieć dwa silniki reguł. Powstaje crate **`sim/policy`** (AST, ewaluator, zakresy stosowania), którego właścicielem jest **M7** — bo M7 jest wcześniej i i tak potrzebuje polityk dla tysięcy firm. **M9 dokłada na wierzchu edytor UI, diagnostykę, dry-run i podpowiedzi** oraz pozostaje autorem projektu języka: specyfikacja AST z dokumentu M9 jest wiążąca dla M7, tylko crate zmienia adres z `game::policy` na `sim/policy`. Konsekwencja wymagana przez PRD: reguła gracza i polityka firmy AI wykonują się tym samym kodem, a różnica leży w jakości menedżera, nie w silniku | M7 (właściciel), M9 (język + edytor), M10 |
| K-12 | `DecisionReason` | **Jeden centralny enum w `engine/core`**, bez `#[non_exhaustive]` — brak ramienia renderującego ma łamać kompilację `game/`, a nie po cichu wyświetlać pustą kartę inspekcji. Każda faza dopisuje własne warianty; wyjaśnialność z §7 jest wtedy egzekwowana przez kompilator, a nie przez dobre chęci | wszystkie |
| K-13 | Jednostki i autorytet terenu | **Kontrakt `TerrainQuery` opublikowany przez M1 jest jedynym źródłem prawdy o terenie i jego jednostkach** (wysokość w jednostkach 0,5 m jako `i32`, nachylenie `u8`, współrzędne w metrach). Żadna faza nie wprowadza własnej reprezentacji ani konwersji „u siebie". Faza, której czegoś brakuje w tym interfejsie (np. szerokości przekroju doliny), **zgłasza to do M1 jako rozszerzenie `TerrainQuery`**, a nie liczy tego samodzielnie z surowych danych | M1 (właściciel), M2, M4, M6 |
| K-14 | Geometria pasów: `RoadNetwork` (M2) vs `RoadGraph` (M4) | M2 zapisuje **oś drogi, klasę, liczbę pasów, tonaż i struktury** (most/tunel/nasyp) — czyli to, co wynika z planowania miasta. **M4 wyprowadza z tego geometrię pasów, skrzyżowania i kierunki ruchu** przy budowie `RoadGraph`. M2 nie modeluje pasów, M4 nie modeluje urbanistyki. Granica: jeśli dana jest potrzebna do narysowania miasta — M2; jeśli do przejechania nim — M4 | M2, M4 |
| K-15 | Tydzień w kalendarzu 360-dniowym | **Tydzień 7-dniowy istnieje i dryfuje względem miesiąca.** 30 nie dzieli się przez 7, więc tydzień nie jest podwielokrotnością miesiąca — i dobrze, bo tak samo jest w rzeczywistości. `DayOfWeek` wyprowadza się z absolutnego numeru doby modulo 7 i mieszka w `SimCalendar` (`engine/core`), nie w `game/`. Rytm tygodniowy jest za mocny w detalu, w grafikach zmianowych (PRD §5.5: zmiana weekendowa) i w godzinach handlu, żeby go poświęcić dla elegancji podziału. Konsekwencja przyjęta świadomie: pierwszy dzień roku wypada co roku w inny dzień tygodnia. Agregacja wykresów używa dekad (3 × 10 dni), nie tygodni — dekada dzieli miesiąc bez reszty i sumy są bezstratne | M0 (właściciel `SimCalendar`), M3, M8, M9 |
| K-16 | Partie i oferty poza ECS | **Korekta §2.** Pierwotnie `BatchId` i `OfferId` były `Entity`. To był błąd skali: partii jest rzędu 600 tys., ofert podobnie, obie kategorie mają skrajnie wysoką rotację (powstają i giną w każdym ticku), a **żadna z nich nie jest nigdy odpytywana przekrojowo po archetypach** — do partii dociera się zawsze przez magazyn, pojazd lub półkę, do oferty przez indeks przestrzenny kategorii. Płacenie za nie mutacją strukturalną ECS (bufor komend, przenoszenie archetypu) to koszt bez korzyści. Obie dostają **dedykowane areny z uchwytem `{index: u32, generation: NonZeroU32}`**. Wymagania nienegocjowalne: arena wchodzi do funkcji haszującej stan (§3.6) w kolejności indeksów, uchwyt po zwolnieniu nigdy nie jest ponownie ważny (stąd generacja), a snapshot i zapis traktują arenę jak sekcję ECS. Właściciel mechanizmu areny: M0; instancje: M6 (partie), M5 (oferty) | M0, M5, M6, M12 |
| K-18 | Co zrobić z błędem znalezionym w planie **następnej** fazy | **Poprawka wędruje w przód w tej samej zmianie, w której powstała.** Jeśli praca nad fazą X pokazuje, że dokument fazy Y (Y > X) jest błędny, nieścisły albo niedopowiedziany — poprawiamy dokument Y **teraz**, a nie „jak dojdziemy do Y". Dotyczy pięciu sytuacji, wszystkich zaobserwowanych w praktyce: (1) kontrakt, na którym Y stoi, wygląda inaczej, niż Y zakłada; (2) API, którego Y używa w przykładzie, nie istnieje albo nazywa się inaczej; (3) kryterium ukończenia Y jest niemierzalne albo spełnione tożsamościowo; (4) kolejność pakietów w Y jest niewykonalna, bo któryś potrzebuje danych, które powstają po nim; (5) pakiet Y obiecuje coś, czego żaden pakiet nie jest właścicielem. Zasady: **(a)** poprawka idzie do dokumentu, którego dotyczy — dziennik `00-postep.md` odnotowuje wyłącznie, że powstała; **(b)** każdy dokument zbiera je w tabeli **„Zmiany wpisane po MX"** na swoim końcu, w formacie tabeli korekt (`D-n`, gwiazdka = zmiana zakresu albo kryterium); **(c)** wpisujemy **tylko to, co wiemy na pewno** — faza następna nie jest przy okazji przeprojektowywana; **(d)** poprawka, która wymaga decyzji nie do podjęcia teraz, ląduje w §9.2 dokumentu **fazy** jako decyzja otwarta z propozycją domyślną, nigdy jako komentarz `TODO` w kodzie; **(e)** zmiana dotykająca kontraktu z tego dokumentu nadal wymaga własnego `K-n` — K-18 nie jest obejściem §4a. Uzasadnienie: plan jest wart tyle, ile jest prawdziwy w chwili, gdy ktoś go otwiera. Poprawka odłożona do startu Y kosztuje tyle samo pracy plus ryzyko, że ten, kto zaczyna Y, o niej nie wie — a wiedzę ma ten, kto ją właśnie zdobył, i nikt inny | wszystkie |
| K-5 | Tolerancja LOD | Patrz §4 — mikro↔mezo tolerancja 0, makro z kontraktem słabszym, ale z zachowaniem pieniądza i masy co do grosza i grama | M4, M10, M12 |
| K-19 | Katalog `data/ui/` | **Dopisany do §5.** Nakładki danych na terenie (`TerrainOverlay` z M1) potrzebują palety, progów i jednostki, a te są danymi, nie kodem: klient graficzny i podgląd `headless` mają rysować **tę samą** mapę, więc nie mogą mieć dwóch tabel barw. M2 tworzy `data/ui/overlays.ron` z jednym wpisem (`land_value`), każda następna faza dopisuje swój. Nazwa wyświetlana siedzi pod `loc_key` i podłączy się do `data/locale/`, kiedy `engine/ui` powstanie w M3 — do tego czasu klient wypisuje ją z tabeli zapasowej w kodzie. Lista katalogów w §5 deklaruje się jako kompletna, więc brak wpisu był jej błędem, nie luką | M2 (właściciel), M8, M9, M10, M11 |
| K-20 | Słowniki, które M3 wnosi do `engine/core` | **Rozszerzenie listy z K-8 o cztery pozycje, wpisane po M3a.** `core` dostaje: **`PlaceKind`** (rodzaj miejsca zaspokajającego potrzebę — M3 wybiera cel, M5 stawia tam sklep z ofertą, M8 instytucję z pojemnością), **`TraitId`** (indeks cechy w `Personality` — M3 planuje czas wolny, M5 czyta `PriceSensitivity` i `Loyalty` w §6.4, M7 `Ambition` przy zmianie pracy), **`DeprivationEffect`** (ładunek `DecisionReason::Deprivation`; ładunek centralnego enuma **musi** mieszkać w `core`, inaczej `core` zależałby od `sim/agents`) i **`MinuteOfDay`** (minuta doby 0..1439 — wymieniają się nią planer M3, podróże M4 i godziny handlu M5). Przy okazji: **`NeedKind` dostaje dwanaście wariantów z M3a §5.5 w miejsce dziesięciu wpisanych roboczo w M0** — kolejność jest kontraktem, bo `Needs.level: [u8; 12]` indeksuje się `as_index()`; nic tych dziesięciu jeszcze nie używało. Do `core` przenosi się też **`assets::{data_dir, data_path}`** z `sim/world` (katalog `data/` jest kontraktem globalnym z §5, a `sim/agents` nie może zależeć od `sim/world` — zależność idzie w drugą stronę); `magnat_world` re-eksportuje, więc nazwy z M1 §6 zostają. **Uzupełnienie po M3b:** dochodzą **`CommitmentKind`** (rodzaj zobowiązania w planie dnia — ładunek `DecisionReason::Commitment`; M4 czyta przy dojazdach, M7 przy grafikach zmianowych) i **`StockCat`** z `STOCK_CAT_COUNT` oraz `StockCat::need()` (kategoria zapasu gospodarstwa — ładunek `DecisionReason::StockBelowThreshold`, indeks `Household.stock` w M3c, odwzorowanie na `GoodId` w M5). Oba spełniają to samo kryterium co poprzednie cztery: ładunek centralnego enuma **nie może** pochodzić z crate'u, który od `core` zależy — ta sama reguła, która wypchnęła `ReplanCause` z `DecisionReason` do `sim/agents` | M0 (właściciel `core`), M3 (wnosi), M4, M5, M7, M8 |
| K-17 | Podfazy jako jednostka pracy | **Fazy M2–M12 są rozbite na podfazy** (`MXa`, `MXb`, …), każda we własnym pliku `MX<litera>-<nazwa>.md`. Podfaza to porcja, którą da się zacząć i zamknąć bez trzymania w głowie całej fazy: własny zestaw WP, własny sprawdzalny wynik i własny wycinek §5. Konsekwencje wiążące: (1) opisy pakietów roboczych i sekcje §5 mieszkają w dokumentach podfaz, a §4 i §5 dokumentu fazy są tabelami kierującymi; (2) **numeracja `5.x` jest zachowana** — odesłanie „patrz §5.4" wskazuje tę samą treść, zmienił się tylko plik; (3) zakres (§2), kontrakty międzyfazowe (§6), testy i kryteria akceptacji (§7), ryzyka (§8) i decyzje otwarte (§9) **zostają w dokumencie fazy** i nie są dzielone — kontrakt jest jeden na fazę; (4) **bramki 1–7 z §8 zamykają się na poziomie fazy, nie podfazy** — podfaza zamyka się własnym kryterium. M0 i M1 zostają nierozbite: M0 jest zamknięty, M1 jest w trakcie i przenumerowanie go w locie kosztowałoby więcej, niż daje. **Uzupełnienie (po M2d):** litera zapisuje kolejność **powstania** dokumentu, a kolejność **wykonania** podaje tabela w §4 dokumentu fazy. Podfaza dopisana później wchodzi tam, gdzie jest jej miejsce w zależnościach, i dostaje kolejną wolną literę — nie przenumerowujemy istniejących, bo ich nazwy są cytowane w dzienniku i w tabelach korekt, a zysk byłby wyłącznie estetyczny. Pierwszy przypadek: `M2f` wykonywana między `M2d` a `M2e` | M2–M12 |

---

## 5. Dane (`data/`)

- Format: **RON**. Ładowane przy starcie, walidowane przed użyciem.
- Katalogi: `data/goods/`, `data/recipes/`, `data/jobs/`, `data/buildings/`, `data/grammar/`,
  `data/names/`, `data/epochs/`, `data/events/`, `data/needs/`, `data/demography/`,
  `data/materials/`, `data/geology/`, `data/climate/`, `data/zoning/`, `data/districts/`,
  `data/chains/`, `data/site_types/`, `data/scenarios/`, `data/locale/`, `data/policies/`,
  `data/palettes/`, `data/models/`, `data/audio/`, `data/tech/` (drzewa technologii, M10),
  `data/ui/` (palety i progi nakładek danych — `K-19`, właściciel M2).
  Faza dokładająca katalog dopisuje go tutaj — to jest lista kompletna, nie przykładowa.
- Każdy plik ma `schema_version`.
- **Walidator grafu produktów** (każdy towar ma źródło) wchodzi do CI **już w M2**, nie w M6 —
  Etap 7 generacji (obsadzenie budynków firmami) musi sprawdzić domknięcie łańcuchów, zanim
  powstanie jakakolwiek produkcja. M2 tworzy minimalny katalog `data/goods/` i `data/recipes/`
  oraz walidator domknięcia osiągalności; **M6 jest właścicielem docelowego schematu i pełnego
  katalogu (~400 towarów)** i rozszerza jedno i drugie. M2 nie projektuje schematu pod siebie —
  uzgadnia go z M6.
- `GoodId`/`RecipeId` nadawane przy ładowaniu (stabilna kolejność alfabetyczna klucza tekstowego);
  w zapisie gry trzymamy klucz tekstowy, nie indeks.

---

## 6. Konwencje kodu i testów

- Rust 2021, `#![forbid(unsafe_code)]` domyślnie; `unsafe` tylko w `ecs`/`voxel`,
  z uzasadnieniem w komentarzu i testem pod Miri.
- Każdy crate: `cargo test` zielony, `clippy -D warnings`, `criterion` dla ścieżek gorących.
- **Headless-first:** każdy system symulacji musi dać się uruchomić i przetestować bez GPU.
- Testy własnościowe obowiązkowe dla każdej fazy dotykającej pieniądza lub masy:
  - suma pieniądza = emisja − destrukcja,
  - suma masy towaru = produkcja − konsumpcja − straty.
- Nazewnictwo: kod i identyfikatory po angielsku, dokumentacja i komentarze domenowe po polsku.

---

## 7. Wymóg wyjaśnialności (PRD §14.1, §20.1)

Każda decyzja agenta lub firmy zapisuje **powód** w formie strukturalnej
(`DecisionReason` — enum z parametrami, nie string), dostępny w karcie inspekcji.
System bez wyjaśnialności nie spełnia Definition of Done swojej fazy.

---

## 8. Szablon dokumentu fazy

Każdy plik `MX-*.md` ma dokładnie te sekcje:

1. Cel fazy i artefakt końcowy (co da się uruchomić i zobaczyć po zakończeniu)
2. Zakres — wchodzi / nie wchodzi (z odesłaniem do fazy, która to robi)
3. Mapowanie na PRD (numery sekcji)
4. Pakiety robocze (WP) — kolejność, zależności, opis, kryterium ukończenia
5. Projekt techniczny — struktury danych, sygnatury API, systemy ECS i ich częstotliwość
6. Kontrakty międzyfazowe — „dostarczam" / „konsumuję" (jawne nazwy typów i funkcji)
7. Testy i kryteria akceptacji (w tym determinizm i wydajność)
8. Ryzyka fazy i mitygacje
9. Decyzje otwarte (do rozstrzygnięcia przed startem fazy)
10. Szacunek wielkości (rozmiar względny WP: S/M/L/XL, bez dat)

Od M2 sekcje 4 i 5 są **rozbite na dokumenty podfaz** (`K-17`): w dokumencie fazy zostają tabele
kierujące, a opis pakietów i treść §5 mieszkają w plikach `MX<litera>-<nazwa>.md`. Numeracja `5.x`
nie zmienia się przy przenosinach. Pozostałe sekcje zostają w dokumencie fazy w całości.
