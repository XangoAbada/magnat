# M9 — Gracz: kariera

Status: plan fazy. Podlega `00-konwencje-i-kontrakty.md` (typy bazowe, determinizm, wyjaśnialność).
Źródło: `PRD_Magnat.md` §13, §14, §16.4, §18.2, §19, §20.3.
Crate'y tworzone: `game/`. Crate'y rozszerzane: `engine/ui` (do pełnej postaci), `sim/policy`
(język reguł — właścicielem crate'a jest M7, K-11), wszystkie `sim/*` (hooki gracza).

---

## 1. Cel fazy i artefakt końcowy

Po M8 istnieje żywe miasto, które działa bez gracza. M9 wsadza w nie gracza — nie jako
bezcielesnego „inwestora", lecz jako **jednego z mieszkańców**, i daje mu narzędzia, którymi
da się prowadzić 200 sklepów bez mikrozarządzania.

**Artefakt końcowy:** uruchamialna gra. Konkretnie, w jednej sesji da się:

1. Wybrać (lub wylosować) mieszkańca jako postać — z jego domem, rodziną, pracą, oszczędnościami
   i znajomymi, wygenerowanymi przez M2/M3, nie doklejonymi.
2. Przeżyć dzień jako pracownik: iść do pracy, zrobić zakupy, obejrzeć własną kartę inspekcji
   i kartę inspekcji sąsiada — zrozumieć, jak działa miasto.
3. Otworzyć pierwszy biznes (kiosk / food truck / sklep / warsztat / furgonetka), ustalić ceny
   ręcznie, obsłużyć go osobiście (realne godziny postaci), zobaczyć klientów i — przede wszystkim —
   **tych, którzy nie kupili, i dlaczego**.
4. Urosnąć: zatrudnić ludzi, postawić menedżera, otworzyć drugi punkt, kupić dostawę własną,
   wejść w produkcję, zbudować grupę — bez żadnej blokady poza kapitałem, ludźmi, informacją i czasem.
5. **Zautomatyzować**: napisać politykę cenową w edytorze reguł (bez pisania kodu), przypiąć ją
   do 40 sklepów, zobaczyć w karcie inspekcji, którą regułę menedżer zastosował i jak bardzo ją
   spartaczył, bo ma niską umiejętność.
6. Zbankrutować osobiście — i grać dalej jako pracownik z długiem i popsutą reputacją.
7. Umrzeć — i grać dalej jako dziedzic, który odziedziczył firmy, ale nie odziedziczył znajomości.
8. Obejrzeć kronikę stulecia i wyeksportować replay (seed + wejścia), który u kogoś innego
   odtworzy tę samą grę co do grosza.

**Warunek grywalności fazy** (bez tego faza jest nieukończona): gracz z 200 sklepami nie musi
dotknąć ani jednej ceny ręcznie, a mimo to rozumie, dlaczego jego ceny są takie, jakie są.

**Twarde kryterium onboardingu (§20.3):** nowy gracz podejmuje pierwszą sensowną decyzję
(`SetPrice` / `OpenSite` / `AcceptJobOffer`) w **poniżej 15 minut** czasu rzeczywistego,
w ≤ 12 interakcjach i przy ≤ 3 otwartych panelach.

---

## 2. Zakres — wchodzi / nie wchodzi

### Wchodzi

| Obszar | PRD | Uwaga |
|---|---|---|
| `game/`: pętla gry, stany, sesja, zapis/wczytanie sesji, integracja sim+engine | §16.2 | crate tworzony w tej fazie |
| Postać gracza jako zwykły mieszkaniec, warianty startu | §13.1 | |
| Ścieżka kariery bez sztucznych blokad, tier jako etykieta | §13.2 | |
| Cele, scenariusze, osiągnięcia emergentne z kronik | §13.3 | |
| Bankructwo osobiste, śmierć postaci, dziedziczenie | §13.4 | |
| `PlayerCommand` — pełny zestaw wejść gracza + dziennik replay | §18.2 | |
| Karta inspekcji w pełnej postaci (renderowanie `DecisionReason`) | §14.1 | szkielet z M3, tu dopełnienie |
| Warstwy widoku, nakładki danych, filtrowanie | §14.2 | wybór i filtr — tu; rysowanie — M1/M2 |
| Komplet paneli biznesowych | §14.3 | 9 paneli |
| Tryb śledzenia mieszkańca / pojazdu / partii | §14.4 | |
| Sterowanie czasem + „zatrzymaj przy zdarzeniu X" | §14.5 | |
| **Język reguł: projektuję** (gramatyka, warunki, akcje, metryki, zakresy) | §14.6, §6.3 | K-11: AST i ewaluator mieszkają w `sim/policy`, którego **właścicielem jest M7** — firmy AI używają tego samego języka. Mój projekt języka jest dla M7 wiążący |
| **Edytor reguł, diagnostyka, dry-run „30 dni", podpowiedzi — mój zakres** | §14.6 | warstwa nad `sim/policy`, w `game/` |
| Wykonanie polityk przez menedżera gracza (`ManagerExecution`, eskalacje) | §14.6, §7 | odwzorowanie umiejętność → jakość wykonania |
| `engine/ui` w pełnej postaci: retained-mode, layout, `Table<T>` 100k, wykresy, grafy, Gantt, mapy cieplne mini, edytor reguł, DPI, i18n PL/EN z pluralizacją | §16.4 | |
| Onboarding i pomiar metryk gracza | §20.3 | |
| Punkt rozszerzenia dla paneli M10 (`PanelRegistry`) | — | tylko rejestr, bez treści |

### Nie wchodzi

| Obszar | Faza | Relacja M9 |
|---|---|---|
| Mechaniki symulacji, które panele wyświetlają: ceny, oferty, produkcja, HR, ruch, podatki | M3–M8 | konsumuję przez snapshot; wymagania na dane spisane w §6 |
| Marka, R&D, giełda, media, przejęcia + ich panele | M10 | rejestruję `PanelId`/`OverlayField`/`Goal` jako zarezerwowane, treści nie piszę |
| Detale graficzne, animacje, wnętrza, audio | M11 | UI rysuje się na własnych listach rysowania, niezależnie |
| Modding UI, panele z modów, pełne wersjonowanie zapisu | M12 | `PanelRegistry` jest projektowany tak, by M12 mógł go zasilić ze skryptu |
| Tryb 50× po stronie symulacji (mezo, makro) | M9 tylko sterowanie | przełącznik tu, implementacja mezo w M4/M9→`sim/macro` |
| Multiplayer lockstep | poza planem | koperta komendy ma pole `actor`, reszta nie |

---

## 3. Mapowanie na PRD

| Sekcja PRD | Co z niej realizuje M9 |
|---|---|
| §13.1 Start | `StartVariant`, wybór mieszkańca, `CharacterSelect` |
| §13.2 Ścieżka | `CareerTier` wyprowadzany, zero bram; `PlayerCommand::FoundFirm`/`OpenSite`/`AssignManager` |
| §13.3 Cele i scenariusze | `Scenario`, `Objective`, `Goal`, osiągnięcia jako zapytania do kroniki |
| §13.4 Porażka | `PersonalBankruptcy`, `Succession`, reputacja i dług przeżywające restart |
| §14.1 Karta inspekcji | `InspectionCard`, render `DecisionReason`, `LostSale` |
| §14.2 Warstwy widoku | `OverlaySpec`, `EntityFilter`, legenda |
| §14.3 Panele | 9 modułów w `game/panels/` |
| §14.4 Śledzenie | `FollowTarget`, oś czasu dnia, ślad partii |
| §14.5 Czas | `TimeScale`, `StopCondition` |
| §14.6 Automatyzacja | projekt języka (`Policy`, `Rule`, `ConditionExpr`, `Action`) → `sim/policy`/M7 (K-11); `RuleEditor`, diagnostyka, dry-run, `ManagerExecution` → `game/` |
| §6.3 Ceny gracza | polityki cenowe — ten sam zestaw narzędzi co AI (K-11); podstawa ceny jawna (K-7) |
| §16.4 UI gry | `engine/ui`: `Widget`, `Layout`, `Table<T>`, wykresy, grafy, Gantt, i18n, DPI |
| §18.2 Determinizm/replay | `CommandEnvelope`, dziennik wejść, test odtworzenia |
| §19 M9 | zakres kamienia milowego |
| §20.3 Metryki gracza | onboarding, pomiar z dziennika replay |

---

## 4. Pakiety robocze i podfazy

Kolejność wymuszona jednym faktem: **panele są klientem widgetów, a nie odwrotnie**. Widgety
budujemy w takiej kolejności, w jakiej żąda ich pierwszy panel, który ich naprawdę potrzebuje.
Nie budujemy biblioteki widgetów „na zapas".

Faza jest rozbita na **5 podfaz**. Podfaza to porcja, którą da się zacząć i zamknąć
bez trzymania w głowie całej fazy: własny zestaw WP, własny sprawdzalny wynik i własny
wycinek projektu technicznego. Opis pakietów i sekcje §5 mieszkają teraz w dokumentach
podfaz — poniższa tabela mówi, gdzie co jest. Bramki 1–7 z `00-postep.md` zamykają się
dopiero po ostatniej podfazie; podfaza zamyka się własnym kryterium ze swojego dokumentu.

| Podfaza | WP | §5 | Wynik do pokazania | Dokument |
|---|---|---|---|---|
| **M9a — Szkielet gry i komendy** | WP1, WP2 | 5.1, 5.2, 5.5 | Zapis sesji roku gry i jej odtworzenie z łańcuchem hashy zgodnym co 1000 ticków. | `M9a-szkielet-gry-i-komendy.md` |
| **M9b — Rdzeń UI** | WP3, WP6 | 5.8 | Panel testowy: brak zmian danych → 0 alokacji i 0 ms przebudowy; tabela 100 tys. wierszy sortuje i filtruje poza klatką. | `M9b-rdzen-ui.md` |
| **M9c — Gracz, inspekcja, nakładki** | WP4, WP5, WP7 | 5.3, 5.7, 5.10 | Odpowiedź na „dlaczego Anna nie kupiła u mnie?” w PL i EN, z nazwanym konkurentem i klikalnym odnośnikiem. | `M9c-gracz-inspekcja-nakladki.md` |
| **M9d — Język reguł** | WP8, WP9 | 5.6 | 6 przykładowych polityk zbudowanych wyłącznie klikaniem; dry-run „30 dni” zgodny z późniejszym wykonaniem. | `M9d-jezyk-regul.md` |
| **M9e — Panele, czas, kariera** | WP10, WP11, WP12 | 5.4, 5.9, 5.11, 5.12 | Pełny artefakt fazy z §1 dokumentu fazy: pełna ścieżka kariery, panele biznesowe, automatyzacja polityk. | `M9e-panele-czas-kariera.md` |

Ścieżka krytyczna: WP1 → WP2 → WP3 → WP6 → WP8 → WP9 → WP10 → WP12.
WP4/WP5/WP7/WP11 są równoległe względem siebie po WP3.

---

## 5. Projekt techniczny

Treść przeniesiona do dokumentów podfaz. **Numeracja `5.x` jest zachowana**, więc
odesłania w tekście („patrz §5.4") nadal wskazują tę samą sekcję — zmienił się tylko plik.

| § | Temat | Dokument |
|---|---|---|
| 5.1 | Struktura `game/` | `M9a-szkielet-gry-i-komendy.md` |
| 5.2 | Pętla gry i granica czytania stanu | `M9a-szkielet-gry-i-komendy.md` |
| 5.3 | Postać gracza | `M9c-gracz-inspekcja-nakladki.md` |
| 5.4 | Kariera bez blokad | `M9e-panele-czas-kariera.md` |
| 5.5 | Komendy gracza — wejścia replayu | `M9a-szkielet-gry-i-komendy.md` |
| 5.6 | Język reguł (§14.6) | `M9d-jezyk-regul.md` |
| 5.7 | Karta inspekcji i „dlaczego Anna nie kupiła u mnie?" | `M9c-gracz-inspekcja-nakladki.md` |
| 5.8 | `engine/ui` — system UI (§16.4) | `M9b-rdzen-ui.md` |
| 5.9 | Panele biznesowe (§14.3) | `M9e-panele-czas-kariera.md` |
| 5.10 | Nakładki, filtry, śledzenie, czas | `M9c-gracz-inspekcja-nakladki.md` |
| 5.11 | Scenariusze, cele, kronika, porażka | `M9e-panele-czas-kariera.md` |
| 5.12 | Onboarding — przełożenie §20.3 na wymagania | `M9e-panele-czas-kariera.md` |

---

## 6. Kontrakty międzyfazowe

### Dostarczam

| Typ / funkcja | Crate | Dla kogo |
|---|---|---|
| `PlayerCommand`, `ViewCommand`, `CommandEnvelope`, `CommandError` | `game::command` | M10 (nowe komendy: marka, R&D, giełda), M12 (mody) |
| `fn precheck(&Snapshot, &PlayerCommand) -> Result<(), CommandError>` | `game::command` | wszystkie panele, M10 |
| `ReplayLog` (nagłówek, strumień autorytatywny, strumień widoku) + odtwarzacz | `game::session` | `engine/devtools` (§16.5), M12 (zgłoszenia błędów) |
| `PlayerCharacter`, `PlayerAutonomy`, `CareerTier::derive`, `StartVariant` | `game::player` | M10 (progresja), M12 |
| **Projekt** języka: `Policy`, `Rule`, `ConditionExpr`, `Expr`, `Metric`, `Action`, `PolicyScope`, `PriceBasis` w wyrażeniach | `sim/policy` (**właściciel crate'a: M7**, K-11; autor języka: M9) | M7, M10, M12 |
| `ManagerExecution::from_skill`, eskalacje, `DecisionReason::PolicyApplied` | `game::policy` | M7, M10 |
| Wymagania na `PolicyRunner` (kadencja, rozłożenie w dobie, budżet ms) | spec dla `sim/policy` | **M7** (implementuje) |
| `RuleEditor` + `Diagnostic` + dry-run „30 dni" + serializator tekstowy | `game::policy` | M10, M12 |
| `Scenario`, `Objective`, `Goal`, format `data/scenarios/*.ron` | `game::scenario` | M10 (cele marki/R&D), M12 |
| `ChronicleEntry`, `ChronicleKind`, `chronicle::record()`, `chronicle::query()` | `game::chronicle` | **wszystkie fazy** — każdy system zgłasza swoje zdarzenia |
| `InspectionCard`, `render_reason(&DecisionReason, &Locale)` | `game::inspect` | wszystkie fazy (każda dodaje ramię) |
| `PanelRegistry`, `PanelDesc`, `PanelId` | `game::panels` | **M10** (Brand/Rnd/Stock), M12 (panele z modów) |
| `OverlaySpec`, `OverlayField`, `EntityFilter` | `game::overlays` | M1/M2 (`engine/render` konsumuje spec), M10 |
| `Widget`, `LayoutNode`, `Layout`, `Table<T>`, `Series`, `GraphView`, `GanttView`, `HeatmapThumb`, `DrawList` | `engine/ui` | M10, M11, M12 |
| `LocKey`, katalogi `data/locale/*.ron`, `plural(locale, n)` | `engine/ui` | wszystkie fazy z tekstem |
| `TimeScale`, `StopCondition` | `game::timectl` | M11 (LOD wizualne wg skali), M12 (tryb 50×) |
| `Series` + `MetricsRecorder` (historia metryk do wykresów i dry-runu) | `game::metrics` | M10, `tools/balansator` |

### Konsumuję

| Czego potrzebuję | Od kogo | Uwaga / ryzyko |
|---|---|---|
| `Snapshot` podwójnie buforowany, spójny na granicy ticku, z widokami SoA | M0 (`ecs`, `io`) | bez tego panele czytają rwany stan |
| `DecisionReason` — **jeden centralny enum w `engine/core`, bez `#[non_exhaustive]`** (K-12); każda faza dopisuje wariant + ramię renderujące + `LocKey` | M0, M3–M8 | rozstrzygnięte; brak ramienia = `game/` się nie kompiluje |
| Mieszkaniec z domem, rodziną, pracą, relacjami, pamięcią, planerem dnia, demografią (zgon) | M3 | postać gracza to zwykły `CitizenId` |
| `LodPin` — gwarancja LOD Mikro dla wskazanych encji także przy 10×/50× | M3, M4 | postać gracza i cel trybu „śledź" |
| Zbiór wiedzy mieszkańca o sklepach (§5.7) i rozkład użyteczności zakupu (§6.4) z rozbiciem na składniki | M3, M5 | źródło większości wariantów „dlaczego Anna nie kupiła" |
| `LostSale`: histogram dobowy dla zakładów gracza + bufor 256 wpisów dla oznaczonych | **M5** | do uzgodnienia — §9 |
| Stan półki, transakcje, oferty, księgowość sklepu, banki i ocena zdolności z reputacją | M5 | panel Sklep, Rynek, Finanse, bankructwo |
| `Offer.price_basis` (brutto w detalu, netto w hurcie) oraz rozdzielone przychód netto / VAT należny w księgach | **M5** (K-7), M8 (stawki) | bez tego panele mieszają podstawy, a edytor reguł nie ma czego pokazać w slocie |
| Partie towaru + `BatchProvenance` (etapy z czasem i kosztem) | **M6** | śledzenie partii „od pola do półki" — §9 |
| Kontrakty B2B, zlecenia transportowe, topologia dostawca-odbiorca | M6 | panel Łańcuch dostaw (graf) |
| Zlecenia produkcyjne z oknami czasu, maszyny, przeglądy | M6 | Gantt w panelu Zakład |
| Umiejętności menedżera, HR, rynek pracy, mediana płac, rotacja, strajki | M7 | `ManagerExecution`, panel Ludzie |
| `sim/policy`: AST + ewaluator + `PolicyRunner` wg mojego projektu języka | **M7** (K-11) | rozstrzygnięte; M9 dokłada edytor, diagnostykę, dry-run i `ManagerExecution` |
| Trasy, czasy przejazdu, parkingi, pojazdy | M4 | odległość i bariera „brak parkingu" w karcie inspekcji |
| Podatki, pozwolenia, przetargi, wybory, prawo spadkowe | M8 | panel Miasto, sukcesja |
| Zdarzenia świata (awarie, pogoda, recesje) jako `ChronicleKind` | M8, M11 | kronika, warunki zatrzymania |
| Renderowanie pól skalarnych i strumieni na terenie | M1, M2 | nakładki danych — `game/` daje tylko spec |
| Zapis/wczytanie snapshotu + strumieni pobocznych (kronika, serie, dziennik replay) | M0 (min.), M12 (pełny) | §9 — format plików pobocznych |
| Blok `StreamId` **260–279** (K-4) | M0 | M9 używa `PolicyExecution = 260`; 261–279 wolne na przyszłe strumienie gracza |

---

## 7. Testy i kryteria akceptacji

### Determinizm i replay

| Test | Kryterium |
|---|---|
| Odtworzenie sesji | Nagrany dziennik 10 lat gry (headless, skryptowane komendy) odtworzony daje identyczny łańcuch hashy stanu co 1000 ticków |
| Komendy odrzucone | Dziennik z komendami nieprawidłowymi odtwarza się identycznie — odrzucenie z tym samym `CommandError` |
| Polityki | 200 sklepów z politykami, dwa przebiegi tego samego seeda → identyczne ceny i zamówienia |
| Menedżer | Błąd wykonania i pominięcia pochodzą wyłącznie z `rng(seed, StreamId::PolicyExecution, ..)` — brak `Instant::now()` w `game/` (test lintujący) |
| Strumień widoku | Zmiana `TimeScale`, pauzy, układu paneli i sortowań **nie zmienia** łańcucha hashy |
| Sukcesja i bankructwo | Deterministyczny wybór dziedzica; suma pieniądza zachowana (majątek = spadek + podatek spadkowy + spłacone długi) |

### Wydajność UI — budżety na klatkę

Budżet klatki 16,6 ms przy 60 FPS. **UI dostaje 4,0 ms CPU i 2,0 ms GPU**, gdy panele są otwarte.

| Scenariusz | Budżet | Jak osiągnięty |
|---|---|---|
| Klatka bez zmian danych i bez wejścia | **0,00 ms, 0 alokacji** | dirty-flagging po `DataVersion`; test z licznikiem alokacji |
| Przebudowa pojedynczego panelu (zmiana danych) | ≤ 2,0 ms | przebudowa tylko brudnego poddrzewa |
| `Table<T>` 100k wierszy — budowa i rysowanie widoku | ≤ 0,5 ms | wirtualizacja: ~60 wierszy + 8 overscan, stała wysokość |
| `Table<T>` 100k — przewijanie | ≤ 0,2 ms | brak ponownego sortowania i filtrowania |
| `Table<T>` 100k — zmiana sortowania | ≤ 12 ms **poza klatką** (`engine/jobs`) | klucze `u64` + pdqsort; do czasu gotowości widoczny stary porządek |
| `Table<T>` 100k — zmiana filtra | ≤ 8 ms poza klatką | wynik jako bitset, przyrostowo |
| Wykres 10 lat danych dziennych × 8 serii (3600 próbek/serię, K-1) | ≤ 0,3 ms CPU / 0,2 ms GPU | piramida mip (dzień/dekada/miesiąc/kwartał), ≤ 2000 odcinków niezależnie od zakresu |
| Graf łańcucha dostaw, 500 węzłów / 2000 krawędzi — rysowanie | ≤ 0,8 ms | instancjonowanie węzłów i krawędzi, cache geometrii |
| Ten sam graf — przeliczenie układu | ≤ 50 ms **poza klatką**, tylko przy zmianie topologii | Sugiyama warstwowy, cache po hashu topologii |
| Gantt, 500 pasków w oknie | ≤ 0,4 ms | wirtualizacja po oknie czasu |
| Miniatura mapy cieplnej 256×256 | ≤ 0,3 ms, odświeżanie `EveryHour` | pole skalarne z nakładki, nie przeliczane per klatka |
| `PolicyRunner`, 200 zakładów, doba gry | ≤ 1,0 ms sumarycznie na wątku symulacji | `EveryDay` rozłożone: zakład `i` w minucie `(i*37) % 1440` |
| Tryb 50× z otwartymi panelami | UI ≤ 1,0 ms | panele na `EveryHour`, Gantt i śledzenie wyłączone |

Wszystkie mierzone w `criterion` na bezgłowym uprzęży UI (bez GPU) — zgodnie z zasadą
headless-first z doc 00 §6.

### Wyjaśnialność

- Test wyczerpującego `match`: dodanie wariantu `DecisionReason` bez ramienia renderującego
  **nie kompiluje się**.
- Fuzz: 1000 losowych `DecisionReason` renderuje niepusty tekst w PL i EN; żaden nie zwraca
  klucza zamiast tłumaczenia.
- Scenariusz akceptacyjny „Anna": w sklepie gracza z celowo zawyżoną ceną, w ciągu 1 doby gry
  panel Sklep pokazuje ≥ 3 nazwane utracone wizyty, każda z powodem i — tam, gdzie dotyczy —
  z nazwanym konkurentem i różnicą ceny.
- Każde zastosowanie polityki ma `DecisionReason::PolicyApplied` z wejściami i odchyleniem
  menedżera (test: 100% zastosowań, nie próbka).

### Język reguł

- 6 polityk z §5.6 zbudowanych **wyłącznie przez interakcje edytora** (test skryptowy na
  `UiIntent`), bez wpisywania tekstu.
- Round-trip: AST → tekst → AST identyczne dla 10 tys. losowych poprawnych polityk.
- **Podstawa ceny (K-7):** nie da się złożyć w edytorze porównania ani akcji mieszającej brutto
  z netto; import tekstu z taką mieszanką zwraca `PriceBasisMismatch`. Reguła „−2% względem
  najtańszego konkurenta w promieniu 3 km" wykonana na sklepie daje cenę brutto równą
  `0,98 × cena_brutto_konkurenta` co do grosza.
- Walidator łapie: niezgodność jednostek, oscylację bez martwej strefy, regułę nieosiągalną,
  konflikt zakresów, przekroczony budżet kosztu.
- Własność: żadna polityka nie tworzy pieniądza — zamówienia ograniczone limitem kredytowym,
  ceny ≥ 0; przekroczenie jest przycinane i raportowane jako alert, nigdy nie panikuje.
- Dry-run na 30 dniach zgodny z późniejszym rzeczywistym wykonaniem przy `ManagerExecution`
  o `skill = 100` (tolerancja 0).

### Kariera i porażka

- **Brak sztucznych blokad**: w `Sandbox` gracz wydaje `FoundFirm` w ticku 1 i komenda przechodzi;
  test przeszukuje kod pod kątem sprawdzania `CareerTier` w ścieżce walidacji komend (musi być puste).
- Bankructwo osobiste: `GameState` pozostaje `Playing`, `CareerTier::derive == Employee`,
  harmonogram długu aktywny, ocena zdolności w banku pogorszona.
- Śmierć: sukcesja przenosi własność i zobowiązania, **nie przenosi relacji ani umiejętności**;
  brak dziedzica → ekran spuścizny z pełną kroniką dynastii.
- Scenariusz „Zbuduj sieć 50 sklepów" przechodzi w headless w skryptowanym przebiegu.

### UI, i18n, DPI

- Pseudo-lokalizacja (napisy ×1,4) — brak przepełnień układu w żadnym panelu.
- Każdy `LocKey` istnieje w `pl` i `en`; brak literałów tekstowych w konstruktorach widgetów.
- Pluralizacja PL: `1 sklep / 2 sklepy / 5 sklepów / 1,5 sklepu` — tablica przypadków w teście.
- `ui_scale` 0,75 / 1,0 / 1,5 / 2,0 / 3,0 — brak rozmyć i przycięć, przyciąganie do pikseli.
- **Kalendarz (K-1):** oś czasu wykresu na 10 latach daje dokładnie 120 podziałek miesięcznych
  i 40 kwartalnych; agregacja mip dzień→dekada→miesiąc→kwartał jest bezstratna dla sum
  (`sum` poziomu wyższego = suma poziomu niższego, tolerancja 0).
- **VAT (K-7):** w panelu Finanse suma wiersza „przychód netto" nigdy nie zawiera VAT-u;
  test na wygenerowanych księgach: `obrót_brutto = przychód_netto + VAT_należny` i VAT
  występuje wyłącznie po stronie zobowiązań.

### Onboarding

- Skryptowy przebieg samouczka: ≤ 12 interakcji i ≤ 3 panele do pierwszej `SetPrice` (test CI).
- Z dzienników playtestów liczony `t_first_meaningful_decision`; cel: mediana < 15 min,
  percentyl 90 < 25 min. Raport generowany z replayów, nie z osobnej telemetrii.

---

## 8. Ryzyka fazy i mitygacje

| Ryzyko | Skutek | Mitygacja |
|---|---|---|
| **`engine/ui` to największa pojedyncza masa kodu w projekcie i nie ma zapasowego planu** (egui wolno tylko w devtools, §16.1) | Faza się rozjeżdża, panele czekają na widgety | Widgety budowane wyłącznie pod konkretny panel, który ich żąda (kolejność WP). Dopuszczamy **tymczasowe** panele na egui za flagą `dev-panels` w buildach deweloperskich, usuwane do końca fazy — to pozwala testować mechaniki, zanim widget powstanie |
| **Język reguł puchnie w język programowania** | Nieskończona faza, nieuczalne UI | Twarde limity w §5.6 (bez zmiennych, pętli, funkcji; ≤ 8 reguł, głębokość ≤ 3). Czego zabraknie — idzie do M12/modding, nie do M9 |
| **Dług wyjaśnialności z M3–M8**: warianty `DecisionReason` okażą się ubogie i karta inspekcji nie odpowie na pytania gracza | Główna obietnica gry (§14.1) niespełniona, metryka §20.3 „rozumiem, dlaczego przegrałem" nie do osiągnięcia | Audyt na starcie fazy: lista pytań z §5.7 skonfrontowana z istniejącymi wariantami; braki zgłoszone jako wymagania do faz M3–M8 **przed** WP5, nie po |
| **`LostSale` zbyt drogie** przy 400 tys. agentów | Panel Sklep bez odpowiedzi „kto nie kupił" | Dwa poziomy: histogram (zawsze, zakłady gracza) + bufor 256 (tylko oznaczone). Dla zakładów AI zero kosztu |
| **Postać gracza i cele śledzenia przypięte do Mikro przy 50×** | Tryb makro traci wydajność | `LodPin` ograniczony do ≤ 8 encji; przy `X50` tryb śledzenia domyślnie wyłączony, włączenie jest świadomym kosztem pokazanym graczowi |
| **Kronika rośnie w nieskończoność** (100 lat gry, §19 M12) | Zapis puchnie, wyszukiwanie wolne | Rekord ≤ 48 B, dane typowane zamiast stringów, decymacja po ważności powyżej 5 lat, wpisy gracza nietykalne |
| **Panele czytają stan w połowie ticku** | Niedeterminizm tego, co gracz widzi; komenda wydana na nieaktualnym stanie | Typ: panele dostają wyłącznie `&Snapshot`; brak dostępu do `&World` w `game::panels` (egzekwowane widocznością modułów) |
| **Rozjazd języka reguł z M7** — `sim/policy` należy do M7, a język projektuję ja (K-11); M7 może dodać metrykę lub akcję poza gramatyką | Edytor nie umie pokazać czegoś, co AI już robi; gracz i AI przestają mieć „ten sam zestaw narzędzi" (§6.3) | Gramatyka z §5.6 jest wiążąca: rozszerzenie języka wymaga dopisania slotu w edytorze, więc każda nowa metryka/akcja M7 to zmiana uzgodniona. Test: enum `Metric` i `Action` mają wyczerpujące pokrycie w edytorze — nowy wariant bez slotu łamie build `game/` |
| **Onboarding przegrywa z bogactwem UI** | Metryka < 15 min nieosiągalna | Domyślny układ 2 paneli, `min_tier` sterujący przypinaniem, test CI na liczbę interakcji od pierwszego dnia WP12, nie na końcu |
| **Sortowanie/filtrowanie 100k wierszy w klatce** | Zacięcia przy każdym kliknięciu nagłówka | Cała praca poza klatką w `engine/jobs`; widoczny stary porządek do czasu gotowości; brak jakiejkolwiek pracy O(n) w ścieżce przewijania |
| **Eksplozja liczby komend** (`PlayerCommand` ma ~70 wariantów) | Trudny replay, trudna walidacja | Jedna funkcja `precheck` użyta w obu miejscach; test pokrycia: każdy wariant ma co najmniej jeden test walidacji i jeden wpis w dzienniku replayu |

---

## 9. Decyzje otwarte

Rozstrzygnięte przez koordynatora w trakcie planowania i **usunięte z tej listy**:
język reguł i własność `sim/policy` (**K-11** — M7 właścicielem crate'a, M9 autorem języka),
`DecisionReason` jako jeden centralny enum bez `#[non_exhaustive]` (**K-12**),
podstawa ceny `Offer.price_basis` i rozdział przychód netto / VAT (**K-7**),
blok `StreamId` 260–279 (**K-4**), kalendarz 360 dni = 12 × 30 (**K-1**).

| # | Decyzja | Kontekst | Propozycja M9 | Z kim | Status |
|---|---|---|---|---|---|
| 1 | **Kto zapisuje `LostSale`** i jakim kosztem | Bez tego nie ma odpowiedzi „dlaczego Anna nie kupiła" — sedno §14.1 | `sim/economy` zapisuje, ale wyłącznie dla zakładów z flagą `observed_by_player`: histogram dobowy zawsze, bufor 256 wpisów dla oznaczonych. Flagę ustawia `game/` przy zmianie własności | **M5** | przekazane właścicielowi (M5) |
| 2 | **`LodPin`** — czy M3/M4 gwarantują Mikro dla wskazanych encji przy 10× i 50× | Postać gracza i tryb „śledź" nie mają sensu w mezo | ≤ 8 przypiętych encji; przy `X50` przypięcie kosztuje i jest komunikowane | **M3, M4** | przekazane właścicielowi (M3/M4) |
| 3 | **Głębokość `BatchProvenance`** | §14.4: „od pola do półki, z czasem i kosztem na każdym etapie". Pełny łańcuch dla milionów partii jest drogi | Pełny łańcuch tylko dla partii dotkniętych przez zakłady gracza; dla reszty ostatnie 3 etapy. Jeśli M6 nie da rady — panel degraduje się do „ostatnie 3 etapy" i trzeba to przyznać w PRD | **M6** | przekazane właścicielowi (M6) |
| 4 | **Czy w kalendarzu 12 × 30 istnieje tydzień 7-dniowy** | K-1 daje 360 dni = 12 × 30, ale 30 nie dzieli się przez 7. Dotyczy `Metric::DayOfWeek`, `WeekSchedule` w `SetOpeningHours`/`SetOwnShift` i rytmu „weekendowego" popytu | Albo tydzień 7-dniowy dryfujący względem miesiąca (realizm, ale brzydka arytmetyka osi), albo dekada 10-dniowa z „wolnym" co 10. dzień. **Rekomendacja: tydzień 7-dniowy dryfujący** — rytm tygodniowy jest mocno widoczny w handlu detalicznym i szkoda go stracić; piramida mip wykresów i tak używa dekad, więc nic nie traci. Typ `DayOfWeek` musi pochodzić z kalendarza w `engine/core`, nie z `game/` | **M0**, M3, M8 | otwarte |
| 5 | **Odwzorowanie umiejętności menedżera na jakość wykonania** | Kto jest właścicielem krzywej `from_skill` | M7 jest właścicielem umiejętności, M9 odwzorowania. Krzywa w `data/` (moddowalna), nie w kodzie | M7 | otwarte |
| 6 | **Format plików pobocznych zapisu**: kronika, serie metryk, dziennik replay | M0 daje snapshot minimalny, pełne wersjonowanie dopiero M12 | Trzy pliki obok zapisu, każdy z `schema_version`; M12 wciąga je w migracje | M0, M12 | otwarte |
| 7 | **Czy strumień `ViewCommand` jest obowiązkową częścią zapisu** | Potrzebny do zgłoszeń błędów i metryk §20.3, ale to megabajty | Nie w zapisie gry; osobny plik, domyślnie włączony, wyłączalny w ustawieniach; zawsze dołączany do zgłoszenia błędu | M12 | otwarte |
| 8 | **Podatek spadkowy i prawo spadkowe** przy sukcesji | §13.4 wymaga przejścia majątku; stawka to prawo miejskie | M8 dostarcza stawkę i tryb; M9 wykonuje transfer i sprawdza własność pieniądza | **M8** | otwarte |
| 9 | **`WorldPatch` dla scenariuszy** („Uratuj upadającą hutę") | Scenariusz musi deterministycznie zmodyfikować wygenerowany świat, nie łamiąc kontraktu hasha | Łatki stosowane jako komendy w ticku 0, po generacji, przed pierwszym systemem — wtedy hash pozostaje funkcją `(seed, lista łatek)` | **M1, M2** | otwarte |
| 10 | **Czy `actor: PlayerId` zostaje w kopercie komendy** | §18.2 wskazuje lockstep jako możliwość; koszt 2 bajty na komendę | Zostaje. Dwa bajty teraz są tańsze niż migracja formatu replayu później | — | otwarte |
| 11 | **Rezerwacje dla M10** | `OverlayField::BrandAwareness`, `Goal::ProductLaunched`, `LostToCompetitor { dominant: Brand }`, `PanelId::{Brand, Rnd, Stock}` | Warianty istnieją od M9 jako nieaktywne, M10 je zasila bez zmiany typów | **M10** | otwarte |
| 12 | **Kto jest właścicielem `Series`/`MetricsRecorder`** | Balansator (M5) też chce historii metryk | `game/` zapisuje serie dla gracza; `tools/balansator` ma własny zapis headless. Wspólny jest tylko typ `Series` w `engine/ui` | M5, M12 | otwarte |

---

## 10. Szacunek wielkości

| WP | Zakres | Rozmiar |
|---|---|---|
| WP1 | Szkielet `game/`, pętla, stany, sesja, integracja sim+engine | **M** |
| WP2 | `PlayerCommand` (~70 wariantów), koperta, walidacja, dziennik i odtwarzacz replayu | **L** |
| WP3 | Rdzeń `engine/ui`: retained-mode, layout, dirty-flagging, DPI, atlas fontów, i18n z pluralizacją | **XL** |
| WP4 | Postać gracza, autonomia, 5 wariantów startu, ekran wyboru | **M** |
| WP5 | Karta inspekcji, render `DecisionReason`, `LostSale` | **M** |
| WP6 | `Table<T>` 100k, wykresy z piramidą mip, miniatury map cieplnych | **L** |
| WP7 | Nakładki danych i filtry encji | **M** |
| WP8 | AST języka reguł, serializator tekstowy, walidator, `RuleEditor`, dry-run | **L** |
| WP9 | `PolicyRunner`, `ManagerExecution`, eskalacje, powody decyzji | **M** |
| WP10 | 9 paneli biznesowych + `GraphView` + `GanttView` + `PanelRegistry` | **XL** |
| WP11 | Sterowanie czasem, warunki zatrzymania, tryb śledzenia, magazyn i wyszukiwarka kroniki | **L** |
| WP12 | Kariera, 5 scenariuszy, cele, bankructwo, sukcesja, samouczek, metryki | **L** |

Rozkład masy: **WP3 i WP10 to razem około połowy fazy.** To nie jest przypadek — M9 jest fazą,
w której powstaje całe UI gry, a nie tylko warstwa gracza. Jeśli faza ma się rozjechać, rozjedzie
się tam, dlatego WP3 startuje najwcześniej jak to możliwe (zaraz po WP1) i dlatego dopuszczamy
tymczasowe panele na egui za flagą deweloperską, żeby mechaniki z WP8/WP9/WP12 dało się testować
niezależnie od postępu widgetów.
