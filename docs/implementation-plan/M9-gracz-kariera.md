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

1. **Uruchomić `magnat` bez jednego argumentu** i z menu głównego założyć świat: ziarno, rozmiar,
   region, epoka, profil, trudność, scenariusz i wariant startu — wszystko na ekranie, z podglądem
   wygenerowanego miasta i możliwością wylosowania innego (§14.7).
2. Wybrać (lub wylosować) mieszkańca jako postać — z jego domem, rodziną, pracą, oszczędnościami
   i znajomymi, wygenerowanymi przez M2/M3, nie doklejonymi.
3. Przeżyć dzień jako pracownik: iść do pracy, zrobić zakupy, obejrzeć własną kartę inspekcji
   i kartę inspekcji sąsiada — zrozumieć, jak działa miasto.
4. Otworzyć pierwszy biznes (kiosk / food truck / sklep / warsztat / furgonetka), ustalić ceny
   ręcznie, obsłużyć go osobiście (realne godziny postaci), zobaczyć klientów i — przede wszystkim —
   **tych, którzy nie kupili, i dlaczego**.
5. Urosnąć: zatrudnić ludzi, postawić menedżera, otworzyć drugi punkt, kupić dostawę własną,
   wejść w produkcję, zbudować grupę — bez żadnej blokady poza kapitałem, ludźmi, informacją i czasem.
6. **Zautomatyzować**: napisać politykę cenową w edytorze reguł (bez pisania kodu), przypiąć ją
   do 40 sklepów, zobaczyć w karcie inspekcji, którą regułę menedżer zastosował i jak bardzo ją
   spartaczył, bo ma niską umiejętność.
7. Zbankrutować osobiście — i grać dalej jako pracownik z długiem i popsutą reputacją.
8. Umrzeć — i grać dalej jako dziedzic, który odziedziczył firmy, ale nie odziedziczył znajomości.
9. Obejrzeć kronikę stulecia i wyeksportować replay (seed + wejścia), który u kogoś innego
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
| **Ekrany poza rozgrywką**: menu główne, kreator świata, generacja z postępem i podglądem, sloty zapisu, ustawienia, pauza | §14.7 | logika — WP13 (`M9a`), rysowanie — WP14 (`M9b`). **Gra przestaje wymagać wiersza poleceń do założenia świata** |
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
| §14.7 Ekrany poza rozgrywką | `ShellScreen`, `NewGameParams`, `WorldGenJob`, `WorldPreview`, `SaveSlot`, `Settings` (WP13); ekrany i motyw `data/ui/theme.ron` (WP14); język wizualny w `docs/ui-design.md` |
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
| **M9a — Szkielet gry i komendy** | WP1, WP2, WP13 | 5.1, 5.2, 5.5, 5.13 | Zapis sesji roku gry i jej odtworzenie z łańcuchem hashy zgodnym co 1000 ticków; nowa gra zakładana z `NewGameParams` bez argumentów CLI. | `M9a-szkielet-gry-i-komendy.md` |
| **M9b — Rdzeń UI** | WP3, WP6, WP14 | 5.8, 5.14 | Panel testowy: brak zmian danych → 0 alokacji i 0 ms przebudowy; tabela 100 tys. wierszy sortuje i filtruje poza klatką; droga z menu głównego do grającego świata bez wiersza poleceń. | `M9b-rdzen-ui.md` |
| **M9c — Gracz, inspekcja, nakładki** | WP4, WP5, WP7 | 5.3, 5.7, 5.10 | Odpowiedź na „dlaczego Anna nie kupiła u mnie?” w PL i EN, z nazwanym konkurentem i klikalnym odnośnikiem. | `M9c-gracz-inspekcja-nakladki.md` |
| **M9d — Język reguł** | WP8, WP9 | 5.6 | 6 przykładowych polityk zbudowanych wyłącznie klikaniem; dry-run „30 dni” zgodny z późniejszym wykonaniem. | `M9d-jezyk-regul.md` |
| **M9e — Panele, czas, kariera** | WP10, WP11, WP12 | 5.4, 5.9, 5.11, 5.12 | Pełny artefakt fazy z §1 dokumentu fazy: pełna ścieżka kariery, panele biznesowe, automatyzacja polityk. | `M9e-panele-czas-kariera.md` |

Ścieżka krytyczna: WP1 → WP2 → WP3 → WP6 → WP8 → WP9 → WP10 → WP12.
WP4/WP5/WP7/WP11 są równoległe względem siebie po WP3.
WP13 idzie po WP2 (jest logiką sesji), WP14 po WP3 i WP13 — **oba poza ścieżką krytyczną**, ale
WP14 jest pierwszym konsumentem rdzenia UI i dlatego warto go zrobić wcześnie: kreator świata
sprawdza układ, fokus, DPI i i18n bez żadnych danych symulacji.

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
| 5.13 | Powłoka sesji — nowa gra, generacja, sloty (§14.7) | `M9a-szkielet-gry-i-komendy.md` |
| 5.14 | Ekrany powłoki i motyw (§14.7) | `M9b-rdzen-ui.md` |

---

## 6. Kontrakty międzyfazowe

### Dostarczam

| Typ / funkcja | Crate | Dla kogo |
|---|---|---|
| `PlayerCommand`, `ViewCommand`, `CommandEnvelope`, `CommandError` | `game::command` | M10 (nowe komendy: marka, R&D, giełda), M12 (mody) |
| `fn precheck(&Snapshot, &PlayerCommand) -> Result<(), CommandError>` | `game::command` | wszystkie panele, M10 |
| `ReplayLog` (nagłówek, strumień autorytatywny, strumień widoku) + odtwarzacz | `game::session` | `engine/devtools` (§16.5), M12 (zgłoszenia błędów) |
| `PlayerCharacter`, `PlayerAutonomy`, `StartVariant`, `Candidate`, `candidates`, `take_role`, `pin_micro` | `game::player` | M10 (progresja), M12. `CareerTier::derive` dokłada `M9e` razem ze ścieżką kariery |
| `ShellScreen`, `NewGameParams`, `WorldGenJob`, `WorldPreview`, `SaveSlot`, `Settings` | `game::session` | M11 (ustawienia grafiki i dźwięku), M12 (wersjonowanie slotów, modding ekranów) |
| `Theme` + `data/ui/theme.ron` (tokeny z `docs/ui-design.md`) | `engine/ui` | M10, M11, **M12** (motyw jasny, wysoki kontrast, mody) |
| **Projekt** języka: `Policy`, `Rule`, `ConditionExpr`, `Expr`, `Metric`, `Action`, `PolicyScope`, `PriceBasis` w wyrażeniach | `sim/policy` (**właściciel crate'a: M7**, K-11; autor języka: M9) | M7, M10, M12 |
| `ManagerExecution::from_skill`, eskalacje (`PolicyAlert`), `DecisionReason::PolicyApplied` | **`sim/economy`** (`U-1`) — wykonawca polityk stoi w systemie ECS, a `game/` jest nad symulacją, nie w niej | M7, M10 |
| Wymagania na `PolicyRunner` (kadencja, rozłożenie w dobie, budżet ms) | spec dla `sim/policy` | **M7** (implementuje) |
| `RuleEditor` + `Edit` + `Note` + dry-run „30 dni" + serializator i parser tekstowy (`GoodKeys`), `RuleEditorView` | `game::policy` | M10, M12 |
| `Scenario`, `Objective`, `Goal`, format `data/scenarios/*.ron` | `game::scenario` | M10 (cele marki/R&D), M12 |
| `ChronicleEntry`, `ChronicleKind`, `chronicle::record()`, `chronicle::query()` | `game::chronicle` | **wszystkie fazy** — każdy system zgłasza swoje zdarzenia |
| `InspectionCard`, `CardTab`, `CardTabKind`, `InspectionNav`, widget `inspection_card` | kształt i widget w `engine/ui`, **treść w `game::inspect`** (`DG-2`) | wszystkie fazy (każda dodaje ramię w `card`) |
| `render_reason` = `magnat_ui::describe(&Catalog, Locale, DecisionReason)` | `engine/ui::inspect::reason` | wszystkie fazy (każda dodaje ramię) |
| `PanelRegistry`, `PanelDesc`, `PanelId` | `game::panels` | **M10** (Brand/Rnd/Stock), M12 (panele z modów) |
| `OverlayField`, `OverlayField2d`, `EntityFilter`, `overlays::build` | `game::overlays` (nazwa `OverlaySpec` należy od M2 do palety w `sim/world`, `K-19`) | M1/M2 (`engine/render` konsumuje pole), M10 |
| `Theme` + `ColorToken`/`TextRole`, `Span`/`Rich` + `RichExt`, `TabStrip`, `Cached`/`DataSource`/`Versions`, `Table<T>` + `RowSource`/`Filter`, `Series` + `MipLevel`, `HeatmapThumb`, `CalendarFmt`, `fmt::{integer, decimal, money}`, `testing::draw*` | `engine/ui` | M10, M11, M12 |
| `LayoutNode`, `Layout` (dokowanie), `GraphView`, `GanttView` | `engine/ui` — **powstają w `M9e`** razem z pierwszym panelem, który ich żąda (`W-2`) | M10, M11, M12 |
| `Subject`, `SubjectKind` | `engine/core` (`K-62`) — jedenaście wariantów od `M9b`, pięć dokłada `M9c` | wszystkie fazy dopisujące byt z kartą |
| `Shell`, `ShellAction`, ekrany powłoki | `game::screens` | M11 (ustawienia grafiki i dźwięku), M12 (modding ekranów) |
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
| `WorldGenParams`, `generate()`, `generate_city()`, Etap 8 populacji, `WorldGenReport`, `PASSES` z nazwami etapów | M1, M2, M3 | istnieją. **WP13 dokłada do `generate()` obserwatora postępu i flagę anulowania** — dziś raport jest dopiero po zakończeniu, a ekran ładowania potrzebuje go w trakcie (`M9a` Z-4) |
| Nagłówek pliku zapisu czytelny bez wczytania świata (nazwa miasta, data gry, majątek, `WorldGenParams`, `schema_version`) | M0 (min.), **M12** (pełny) | lista slotów nie ma prawa wczytać dziesięciu światów; niezgodna wersja musi dać opisany błąd, nie panikę |
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

### Powłoka i zakładanie gry (§14.7)

- **Bez wiersza poleceń:** `magnat` uruchomiony bez argumentów prowadzi od menu głównego do
  grającego świata w ≤ 6 interakcjach, wyłącznie z klawiatury. Test skryptowy na `UiIntent`.
- **Jedna droga do świata:** `new_game(NewGameParams)` i `magnat --seed … --size … --region …`
  dają **identyczny hash terenu i miasta** dla tych samych wartości. Gdyby kiedykolwiek rozjechały
  się o bit, znaczyłoby to, że klient ma własną, drugą ścieżkę generacji.
- **Postęp jest prawdziwy:** ekran generacji pokazuje nazwę etapu z `PASSES` i rośnie monotonicznie;
  test sprawdza, że liczba raportów = liczba passów, a nie że pasek się rusza.
- **Anulowanie:** przerwanie generacji 16 km w dowolnym momencie wraca do kreatora bez wycieku
  wątku i bez sesji w stanie połowicznym (test: 50 anulowań w pętli, stałe zużycie pamięci).
- **Sloty:** lista dziesięciu slotów czyta wyłącznie nagłówki (test: brak alokacji rzędu świata);
  slot w starszej wersji schematu zwraca `SaveError::SchemaTooOld` i zostaje widoczny na liście.
- **Ustawienia bez restartu:** zmiana `Locale` i `ui_scale` w trakcie gry przerysowuje UI w tej samej
  sesji; test na zrzucie tekstowym w obu językach i na trzech skalach.
- **Determinizm:** ustawienia i układ paneli **nie wchodzą** do hasha stanu (ten sam test co dla
  strumienia widoku), a `PlayerCommand::StartGame` niesie komplet `WorldGenParams` — replay
  odtwarza świat z samej koperty, bez pliku świata.

### Onboarding

- Skryptowy przebieg samouczka: ≤ 12 interakcji i ≤ 3 panele do pierwszej `SetPrice` (test CI).
- Z dzienników playtestów liczony `t_first_meaningful_decision`; cel: mediana < 15 min,
  percentyl 90 < 25 min. Raport generowany z replayów, nie z osobnej telemetrii.

---

## 8. Ryzyka fazy i mitygacje

| Ryzyko | Skutek | Mitygacja |
|---|---|---|
| **`engine/ui` to duża masa kodu, a panele są od niej zależne** | Faza się rozjeżdża, panele czekają na widgety | **Częściowo rozbrojone w M3 (decyzja 9.2):** rdzeniem jest `egui`, nie własny toolkit — nie piszemy układu, atlasu fontów ani obsługi wejścia (korekta PRD §16.4; zapis „egui tylko w devtools" z §16.1 już nie obowiązuje, więc flaga `dev-panels` jest zbędna). Zostaje reguła kolejności: widgety powstają wyłącznie pod ekran albo panel, który ich żąda — pierwszym takim klientem jest kreator świata z WP14, najtańszy z możliwych |
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
| 1 | **Kto zapisuje `LostSale`** i jakim kosztem | Bez tego nie ma odpowiedzi „dlaczego Anna nie kupiła" — sedno §14.1 | `sim/economy` zapisuje, ale wyłącznie dla zakładów z flagą `observed_by_player`: histogram dobowy zawsze, bufor 256 wpisów dla oznaczonych. Flagę ustawia `game/` przy zmianie własności | **M5** | **zamknięte w M5e i wykonane w M9c**: `LostSaleTracking::Full`, pierścień 256 z `went_to`, flagę ustawia `game/` przy wyborze postaci i przy otwarciu karty zakładu |
| 2 | **`LodPin`** — czy M3/M4 gwarantują Mikro dla wskazanych encji przy 10× i 50× | Postać gracza i tryb „śledź" nie mają sensu w mezo | ≤ 8 przypiętych encji; przy `X50` przypięcie kosztuje i jest komunikowane | **M3, M4** | **wykonane w M9c wg propozycji domyślnej** (`DG-8`): `MicroLayer::set_pinned`, `MAX_PINNED = 8`, bramka w `enter`. Ani M3, ani M4 tego nie zrobiły |
| 3 | **Głębokość `BatchProvenance`** | §14.4: „od pola do półki, z czasem i kosztem na każdym etapie". Pełny łańcuch dla milionów partii jest drogi | Pełny łańcuch tylko dla partii dotkniętych przez zakłady gracza; dla reszty ostatnie 3 etapy. Jeśli M6 nie da rady — panel degraduje się do „ostatnie 3 etapy" i trzeba to przyznać w PRD | **M6** | **zamknięte: M6 dał pełny łańcuch.** `magnat_supply::trace_batch` chodzi po krawędziach pokrewieństwa i składa etapy przodków przed własnymi, z kosztem narastającym; `game::inspect::batch` rysuje z tego kartę partii (M9e WP11). Degradacji nie było |
| 4 | **Czy w kalendarzu 12 × 30 istnieje tydzień 7-dniowy** | K-1 daje 360 dni = 12 × 30, ale 30 nie dzieli się przez 7. Dotyczy `Metric::DayOfWeek`, `WeekSchedule` w `SetOpeningHours`/`SetOwnShift` i rytmu „weekendowego" popytu | Albo tydzień 7-dniowy dryfujący względem miesiąca (realizm, ale brzydka arytmetyka osi), albo dekada 10-dniowa z „wolnym" co 10. dzień. **Rekomendacja: tydzień 7-dniowy dryfujący** — rytm tygodniowy jest mocno widoczny w handlu detalicznym i szkoda go stracić; piramida mip wykresów i tak używa dekad, więc nic nie traci. Typ `DayOfWeek` musi pochodzić z kalendarza w `engine/core`, nie z `game/` | **M0**, M3, M8 | otwarte |
| 5 | **Odwzorowanie umiejętności menedżera na jakość wykonania** | Kto jest właścicielem krzywej `from_skill` | M7 jest właścicielem umiejętności, M9 odwzorowania. Krzywa w `data/` (moddowalna), nie w kodzie | M7 | **zamknięte w M9d wg propozycji domyślnej** (`K-70`): krzywa siedzi w `data/tuning/policy.ron`, `ManagerExecution::from_skill(skill, &ManagerCurve)` bierze ją argumentem, a kod jej nie zna |
| 6 | **Format plików pobocznych zapisu**: kronika, serie metryk, dziennik replay | M0 daje snapshot minimalny, pełne wersjonowanie dopiero M12 | Trzy pliki obok zapisu, każdy z `schema_version`; M12 wciąga je w migracje | M0, M12 | **zawężone po M9e: pliki są dwa, nie trzy.** Kronika jest **widokiem pochodnym** (`DI-4`) — zbiera się ją z dzienników, które i tak siedzą w zapisie, więc własnego pliku nie potrzebuje i nie dostanie. Serie metryk też nie: `MetricsRecorder` odtwarza się z przewinięcia dziennika wejść. Zostaje dziennik replayu, który format już ma. Decyzja dla M12: **czy przewijanie przy wczytaniu jest dopuszczalnym kosztem** dla stuletniej gry, czy serie mimo wszystko idą na dysk |
| 7 | **Czy strumień `ViewCommand` jest obowiązkową częścią zapisu** | Potrzebny do zgłoszeń błędów i metryk §20.3, ale to megabajty | Nie w zapisie gry; osobny plik, domyślnie włączony, wyłączalny w ustawieniach; zawsze dołączany do zgłoszenia błędu | M12 | otwarte |
| 8 | **Podatek spadkowy i prawo spadkowe** przy sukcesji | §13.4 wymaga przejścia majątku; stawka to prawo miejskie | M8 dostarcza stawkę i tryb; M9 wykonuje transfer i sprawdza własność pieniądza | **M8** | **otwarte, z wykonaną połową.** Sukcesja działa i przenosi własność bez podatku (`DI-20`): kodeks M8 zna siedem danin (`K-55`) i żadna nie jest spadkowa, a naliczenie stawki, której nie ma w `data/city/`, byłoby wymyśleniem prawa. Do rozstrzygnięcia zostaje **jedno pytanie**: czy danina spadkowa dochodzi jako ósma pozycja `TaxKind` (kolejność jest wieczna, więc tylko na końcu), czy dziedziczenie zostaje wolne od podatku i to jest decyzja projektowa, a nie luka |
| 9 | **`WorldPatch` dla scenariuszy** („Uratuj upadającą hutę") | Scenariusz musi deterministycznie zmodyfikować wygenerowany świat, nie łamiąc kontraktu hasha | Łatki stosowane jako komendy w ticku 0, po generacji, przed pierwszym systemem — wtedy hash pozostaje funkcją `(seed, lista łatek)` | **M1, M2** | **zamknięte w M9e wg propozycji domyślnej.** Łatki idą przed pierwszym tickiem, więc hash został funkcją `(ziarno, lista łatek)`; ani M1, ani M2 nie musiały niczego dołożyć, bo obie istniejące łatki działają **przez rynek**, a nie przez generator. Powstały dwie (`ClearShopKind`, `IndebtSite`) i obie adresują świat **porządkowo**, bo numery dzielnic i zakładów zależą od ziarna (`DI-11`) |
| 10 | **Czy `actor: PlayerId` zostaje w kopercie komendy** | §18.2 wskazuje lockstep jako możliwość; koszt 2 bajty na komendę | Zostaje. Dwa bajty teraz są tańsze niż migracja formatu replayu później | — | otwarte |
| 11 | **Rezerwacje dla M10** | `OverlayField::BrandAwareness`, `Goal::ProductLaunched`, `PanelId::{Brand, Rnd, Stock}` | Warianty istnieją od M9 jako nieaktywne, M10 je zasila bez zmiany typów | **M10** | **domknięte w M9e, z jednym skreśleniem.** `PanelId::{Brand, Rnd, Stock}` istnieją i `is_reserved()` mówi o nich prawdę — nie ma ich w `PanelRegistry`, a M10 dokłada je przez `register`, bez dotykania kodu M9. `Goal::ProductLaunched` **nie powstaje** (`DI-3`): cel, którego nie da się osiągnąć, wygląda w panelu tak samo jak osiągalny. Wcześniej, w M9c: `OverlayField::BrandAwareness` istnieje i **nie ma wpisu w `data/ui/overlays.ron`** — `build` zwraca `None`, a nie raster zer. `LostToCompetitor { dominant: Brand }` skreślone razem z całą rodziną (`DG-1`); marka wejdzie jako składnik `UtilityKind`, który już jest |
| 13 | **Czy dziedzina jest granicą polityki, czy tylko granicą akcji** | `Action::domain()` z M7c przypisuje każdej akcji dziedzinę, a `Policy.domain` musi się z nią zgadzać. Skutek wyszedł w M9d: **dwie z sześciu polityk przykładowych z §5.6 nie dają się zapisać jako jedna** — „Nabiał — nie wyrzucamy" miesza przecenę z wycofaniem z półki, „Sezon grzewczy" zamówienie z przeceną | **Zostawić jak jest.** Dziedzina wybiera wykonawcę, a wykonawca ceny i wykonawca zapasu to dwa różne kroki doby sklepu; polityka o dwóch wykonawcach musiałaby mieć dwie kadencje i dwie martwe strefy. Edytor mówi wprost, czego nie wolno, i gracz robi z tego dwie polityki o tym samym zakresie — konfliktu zakresów to nie tworzy, bo `scope::resolve` rozstrzyga w obrębie dziedziny. Alternatywa (polityka trzyma akcje z wielu dziedzin, a wykonawca bierze swoje) jest tańsza dla gracza i droższa dla kadencji: reguła „przeceń **oraz** wycofaj" musiałaby wykonać się w dwóch różnych momentach doby, a wtedy „ORAZ" przestaje znaczyć „naraz" | **M7** (właściciel `sim/policy`) | otwarte |
| 12 | **Kto jest właścicielem `Series`/`MetricsRecorder`** | Balansator (M5) też chce historii metryk | `game/` zapisuje serie dla gracza; `tools/balansator` ma własny zapis headless. Wspólny jest tylko typ `Series` w `engine/ui` | M5, M12 | **zamknięte w M9e wg propozycji domyślnej.** `MetricsRecorder` mieszka w `Session` i pisze z kroku doby, więc przebieg bezgłowy prowadzi te same serie co klient (`DI-17`); balansator zostaje przy swoim zapisie. Wspólny jest `Series` z `engine/ui` i nic poza nim |

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
| WP13 | Powłoka sesji: `ShellScreen`, `NewGameParams`, generacja z postępem i anulowaniem, sloty, ustawienia | **M** |
| WP14 | Ekrany powłoki (menu, kreator, ładowanie, podgląd, sloty, ustawienia, pauza) + motyw `data/ui/theme.ron` | **M** |

Rozkład masy: **WP3 i WP10 to razem około połowy fazy.** WP13 i WP14 tej proporcji nie zmieniają —
to razem mniej niż jedno WP10, bo cała mechanika, na której stoją (`WorldGenParams`, `generate`,
`generate_city`, Etap 8, format zapisu), istnieje od M1–M3. To nie jest przypadek — M9 jest fazą,
w której powstaje całe UI gry, a nie tylko warstwa gracza. Jeśli faza ma się rozjechać, rozjedzie
się tam, dlatego WP3 startuje najwcześniej jak to możliwe (zaraz po WP1), a jego pierwszym
konsumentem jest WP14 — ekrany powłoki nie potrzebują żadnych danych symulacji, więc rdzeń UI
da się sprawdzić w całości, zanim powstanie pierwszy panel biznesowy.

---

## Zmiany wpisane po M3d

Zgodnie z `K-18`. Źródło: decyzja właściciela produktu z 2026-09-14 — gra ma zakładać świat
z poziomu gry, a nie z wiersza poleceń — plus stan kodu po M3d.

| # | Zmiana | Dlaczego |
|---|---|---|
| Z-1 ★ | **Ekrany poza rozgrywką wchodzą w zakres fazy** (§2, §3): PRD ma nową sekcję §14.7, M9 dostaje dwa pakiety — WP13 (logika, `M9a` §5.13) i WP14 (ekrany i motyw, `M9b` §5.14). Artefakt końcowy §1 zaczyna się od „uruchom `magnat` bez argumentu" | Droga od uruchomienia do grającego świata nie miała właściciela: `GameState::MainMenu` był wariantem enuma, którego nikt nie wypełniał, a komplet parametrów świata żył wyłącznie w `clap` w `tools/magnat`. Przypadek (5) z `K-18` |
| Z-2 ★ | **Zakładanie gry jest dwuetapowe:** teren + miasto → podgląd i decyzja gracza → zaludnienie (Etap 8) → wybór postaci | Pomiary M1 i M3d: metropolia to ~20 s terenu i **26,9 s** zaludnienia. Świat odrzucony po obejrzeniu mapy nie ma powodu być zaludniany. Konsekwencja wpisana wprost: podgląd pokazuje **pojemność** (mieszkania, miejsca pracy, firmy), nie populację, bo w tym momencie nie istnieje jeszcze ani jeden mieszkaniec |
| Z-3 ★ | **`PlayerCommand::StartGame` niesie `WorldGenParams`, nie `seed: u64`** (`M9a` §5.5) | Replay z samym ziarnem odtwarzałby inne miasto, bo rozmiar, region, epoka, profil i trudność zmieniają świat tak samo jak ziarno. To kontrakt determinizmu z §7, nie szczegół |
| Z-4 | **Nowy kontrakt „konsumuję": obserwator postępu i anulowanie w `sim/world::generate`** oraz nagłówek zapisu czytelny bez wczytania świata (§6) | Ekran ładowania i lista slotów bez tego albo kłamią (animowany pasek), albo wczytują dziesięć światów, żeby pokazać dziesięć wierszy |
| Z-5 | **Język wizualny wydzielony do `docs/ui-design.md`**, tokeny do `data/ui/theme.ron`; PRD §16.4 dostał korektę o `egui` (decyzja M3 9.2), która znosi ograniczenie „egui tylko w devtools" z §16.1 | Ryzyko z §8 („`engine/ui` to największa masa kodu bez planu zapasowego") zostało częściowo rozbrojone jeszcze w M3 — dokument fazy nadal mówił inaczej niż kod, który już stoi na `egui`. Przy okazji: tymczasowe panele „za flagą `dev-panels`" z §8 przestają być potrzebne jako furtka, bo `egui` jest teraz drogą główną, a nie awaryjną |

---

## Zmiany wpisane po M5d

Zgodnie z `K-18`.

| # | Zmiana | Dlaczego |
|---|---|---|
| Y-1 | **Gospodarka w kliencie jest zastana, nie budowana.** `tools/magnat` uruchamia `Books`, `Market` i `MarketSystem` oraz ma `Sources.places` podmienione na rynek od **M5e/WP12** (`AB-1`); M9 dokłada gracza, panele i edytor reguł **na działającej gospodarce** | Do M5d klient wstawiał do `AgentSources` atrapę `InfinitePlaces` z M3, a droga, którą gospodarka trafia do okna, nie miała właściciela w żadnym dokumencie — łącznie z tym. Zapisane tutaj, żeby M9c nie zaczął od budowania czegoś, co już stoi: kryterium „dlaczego Anna nie kupiła u mnie" w `M9c` dotyczy **karty inspekcji mieszkańca**, a panel sklepu i jego `ShopPanelSnapshot` przychodzą gotowe z M5e |
| Y-2 | **Koszt gospodarki w klatce będzie znany przed startem fazy.** Bramka benchmarkowa M5e/WP14 rozdziela decyzję zakupową od podróży, które ona generuje (`U-24`), a `M5e` ma kryterium na klatkę przy `X10` z `--no-economy` jako udokumentowaną drogą wyjścia (`AB-2`) | M9 planuje panele i automatyzację polityk przy założeniu, że świat się kręci. Gdyby koszt zakupów w klatce wyszedł dopiero tutaj, wyszedłby **pod panelami** — czyli w miejscu, w którym najtrudniej odróżnić „panel jest wolny" od „symulacja jest wolna" |

---

## Zmiany wpisane po M5e

Zgodnie z `K-18`. To są rzeczy, o których wiemy **na pewno** po zamknięciu fazy M5.

| # | Zmiana | Dlaczego |
|---|---|---|
| Z-1 ★ | **Panel sklepu jest zbudowany i działa w oknie**: `magnat_ui::{ShopCard, ShopTab, ShopView}` + `widgets::shop_card`, dane wyłącznie przez `magnat_economy::ShopPanelSnapshot`. Trzy zakładki (Półki / Klienci / Konkurencja), złoty wydruk tekstowy w CI w obu językach, klucze `ui.shop.*` i `ui.good.*` w `data/locale/`. M9 **dokłada na tym**, a nie buduje od nowa | Wykonanie `Y-1` z poprzedniej rundy. Zapisane z numerami, bo M9c ma kryterium „dlaczego Anna nie kupiła u mnie" i łatwo je pomylić z tym samym zdaniem z M5e: **M5e odpowiada po stronie sklepu** (histogram utraconych sprzedaży z oknem 7 dób i wpisem `went_to`), a **M9c po stronie mieszkańca** (karta inspekcji osoby) |
| Z-2 ★ | **Otwarcie sklepu idzie dziś przez raycast w teren, nie przez bufor identyfikatorów.** `Citizens::select_shop(x, y, promien)` szuka najbliższego zakładu w promieniu 25 m od punktu trafienia, bo bufor ID w `engine/render` niesie **wyłącznie pieszych** (korekta H-15 do M3d) | Dla M9 to jest **zadanie, nie ozdoba**: `Selection` (`engine/ui/src/selection.rs`) nie ma wariantu dla zakładu, a panele biznesowe M9 potrzebują go dla każdej klikalnej rzeczy, nie tylko dla sklepu. Ścieżka wyjścia jest jednolinijkowa po stronie panelu (`select_shop` zamienia się w odczyt `SiteId`), ale wymaga, żeby `engine/render` **dokładał budynki i zakłady do bufora ID** — czyli zmiany w cudzym crate'cie (właściciel M1/M11), którą lepiej zgłosić teraz niż w tygodniu domknięcia M9 |
| Z-3 ★ | **Poziom śledzenia zakładu ustawia klient i to on jest „graczem" w M5.** `Market::set_tracking(site, LostSaleTracking::Full)` wołane przy otwarciu panelu; wyłączenie kasuje rozkłady klientów. Poziom **nie wchodzi do hasha** (`U-22`) | M9 wnosi prawdziwą własność zakładu (gracz przejmuje sklep), więc to wywołanie przenosi się z „otworzyłem panel" na „to jest mój zakład". Zapisane, bo **kasowanie rozkładów przy wyłączeniu jest zamierzone**: po przejęciu sklepu gracz ma zobaczyć swój zasięg, a nie osad poprzedniego właściciela — i M9 ma tę decyzję odziedziczyć świadomie, a nie ją odkryć |
| Z-4 | **`--no-economy` w kliencie istnieje i ma zostać.** Wraca do zachowania M3 (atrapa `InfinitePlaces`) i jest udokumentowaną drogą wyjścia z kosztu klatki (`AB-2`) | Zmierzone przy zamknięciu M5e na mieście 28,2 tys. mieszkańców: **5 351 klatek w 8 s z gospodarką wobec 5 311 bez niej** przy `X10`, zero zacięć powyżej 33 ms. Różnica jest nieodróżnialna od szumu, więc obawa z `AB-2` **nie zmaterializowała się na tej wielkości** — ale przełącznik zostaje, bo M9 dokłada panele i automatyzację polityk, a wtedy pytanie „panel jest wolny czy symulacja" wraca |
| Z-5 | **`Market::preview_policy` istnieje i woła to samo składanie co `reprice`** — podgląd „co by się stało z ceną" jest gotowy dla edytora reguł | M9 §9 planuje dry-run polityk. Połowa tego, co dry-run obiecuje dla cen, już stoi i jest przetestowana (`podglad_ceny_zgadza_sie_z_tym_co_zrobi_przecena`). Osobna arytmetyka podglądu rozjechałaby się z wykonaniem przy pierwszej zmianie wzoru |

---

## Zmiany wpisane po M9a

Zgodnie z `K-18`. Szczegóły i uzasadnienia — tabela `DA-n` w `M9a-szkielet-gry-i-komendy.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| X-1 ★ | **`game/` jest właścicielem drogi, którą powstaje świat** (`game::world`), a `tools/headless` jej konsumentem. Mosty „Etap 7 → gospodarka" przeprowadziły się tam z `tools/headless`; nowy kontrakt `K-68`. §6 „dostarczam" rośnie o `game::world::{stand_up, SessionOpts, BuiltCity}` | Kryteria WP1 i WP13 żądają, żeby przebieg bezgłowy **uruchamiał sesję** — czyli `headless → game`. Zależność w drugą stronę (po mosty) zamykałaby cykl w Cargo |
| X-2 ★ | **`PlayerCommand` rośnie podfazami, nie naraz.** M9a wnosi `StartGame` i `SetPrice`; każdy następny wariant wchodzi razem ze swoim wykonawcą i ze swoim panelem. Lista z `M9a` §5.5 zostaje projektem języka komend. Dopisywać wolno wyłącznie **na końcu** enuma | Ryzyko §8 („eksplozja liczby komend") domaga się, żeby każdy wariant miał test walidacji i wpis w dzienniku — a wariant bez wykonawcy nie może mieć ani jednego. `K-67` rozstrzygnął ten sam spór dla `PolicyKind` |
| X-3 ★ | **Zapis gry do czasu M12 jest dziennikiem wejść, nie migawką stanu.** Slot = nagłówek + `ReplayLog`. Wiersz §6 „konsumuję: zapis/wczytanie snapshotu — M0 (min.), M12 (pełny)" zostaje, ale **M9 nie dostanie migawki od M0**: `save_world` nie widzi zasobów, a `CityData` nie jest serializowalne | Konsekwencja dla `M9b` i `M9e`: ekran slotów pokazuje prawdziwe nagłówki, ale „Wczytaj" przewija dziennik. Czas wczytania rośnie z długością rozgrywki i to jest znany sufit, nie niespodzianka |
| X-4 | **Podwójnie buforowanego `Snapshot` dla paneli nie ma i M0 go nie dostarczy** — `magnat-sim-snapshot` niesie POD-y renderu. Buduje go **WP3 w `M9b`**; szwem, w który wejdzie, jest `game::CommandView` | Wiersz §6 „konsumuję: `Snapshot` … M0 (`ecs`, `io`)" opisywał coś, czego nikt nie jest właścicielem — przypadek (5) z `K-18`. Bez tego zapisu `M9b` zacząłby od szukania typu, którego nie ma |
| X-5 | **Blok `StreamId` 260–279 jest nadal wolny w całości.** M9a nie losuje niczego: komenda gracza jest funkcją stanu, a nie losowaniem | `PolicyExecution = 260` zajmie `M9d` razem z odchyleniem menedżera — to tam jest pierwsze losowanie fazy |
| X-6 | **Blok `DecisionReason` 700–799 jest nadal wolny.** M9a nie zapisuje powodów: `StartGame` i `SetPrice` to decyzje **gracza**, a wyjaśnialność z `00` §7 dotyczy decyzji agentów i firm | Bramka 5 fazy zamknie się w `M9c` (karta inspekcji) i `M9d` (`PolicyApplied`), a nie tutaj |


## Zmiany wpisane po M9b

Zgodnie z `K-18`. Szczegóły i uzasadnienia — tabela `DE-n` w `M9b-rdzen-ui.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| W-1 ★ | **Drzewo retained z §5.8 nie powstało i nie powstanie.** Rdzeniem UI jest `egui` (decyzja M3 9.2, `Z-1`), a dirty-flagging dotyczy **modelu panelu**, nie drzewa widgetów. §6 „dostarczam" traci `Widget`, `LayoutNode`, `DrawList` i `Layout`, a zyskuje `Cached`, `DataSource`, `Versions`, `Theme`, `Span`/`Rich`, `TabStrip`, `RowSource` i `CalendarFmt` | §5.8 i `Z-1` mówiły dwie różne rzeczy o tym samym; postawienie drugiego stosu widgetów na `egui` byłoby dokładnie tym własnym toolkitem, którego tamta decyzja miała uniknąć (`DE-1`, `DE-2`) |
| W-2 ★ | **`Layout` z dokowaniem, `GraphView` i `GanttView` przenoszą się do `M9e`**, `RuleEditorView` do `M9d`. Budżety §7 dla grafu i Gantta zamykają się razem z ich panelami | Reguła kolejności z §4: widget powstaje pod ekran, który go żąda. Układ doków bez paneli to plik konfiguracyjny bez czytelnika (`DE-4`, `DE-5`) |
| W-3 | **Filtr `Table<T>` jest predykatem, nie `ConditionExpr`.** „Jedna gramatyka" zostaje obietnicą formy zapisu, a nie typu pola w tabeli; `M9e` podłącza ewaluator `sim/policy` pod `Filter` | `engine/ui` nie zależy od `sim/policy` i nie ma powodu zaczynać; pole AST wymusiłoby, żeby także lista dziesięciu slotów zapisu była opisana drzewem składniowym (`DE-6`) |
| W-4 ★ | **`Subject` mieszka w `engine/core` od M9b** (`K-62`), z jedenastoma wariantami. `Batch`, `Offer`, `Tender`, `Case` i `Permit` dokłada `M9c` razem z decyzją, gdzie mieszkają ich identyfikatory | `Span.link` jest `Option<Subject>`, więc typ musiał powstać w tej podfazie; pozostałe pięć wymaga przeniesienia `TenderId`/`CaseId`/`PermitId` z `sim/city` albo uchwytu areny, a to jest decyzja karty inspekcji (`DE-9`) |
| W-5 | **Polski ma cztery formy liczebnika** (`one` / `few` / `many` / `other`). Klucz liczebnikowy niesie cztery formy po stronie PL i dwie po stronie EN; `Catalog::plural_frac` obsługuje ułamki | §7 wymienia „1,5 sklepu" wprost, a kod M3 miał trzy formy. Kluczy liczebnikowych jest osiem, więc koszt był żaden (`DE-8`) |
| W-6 | **Zapis gry jest dostępny z menu pauzy**, a majątek w wierszu slotu to zero do czasu WP4 | Bez zapisu ekran „Wczytaj" byłby listą, do której nic nie trafia — czyli ekranem, którego skutku nikt nie widzi (`R2`, `K-67`). Zero z komentarzem jest uczciwsze niż liczba udająca majątek gracza (`DE-12`) |


## Zmiany wpisane po M9c

Zgodnie z `K-18`. Szczegóły i uzasadnienia — tabela `DG-n` w `M9c-gracz-inspekcja-nakladki.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| V-1 ★ | **Blok `DecisionReason` 700–799 zostaje wolny w całości.** Rodzina powodów „dlaczego Anna nie kupiła" (`NotInChoiceSet`, `LostToCompetitor`, `AccessBarrier`, `SubstituteChosen`, `SoftmaxDraw`) **nie powstaje**; odpowiedź składa `LostSale` z M5e (`RejectCause` + `went_to`) | Osiem wariantów `RejectCause` pokrywa każdy wiersz tamtej tabeli, który ma dziś skutek w symulacji. Warianty dla mechanik, których nie ma (kolejka do kasy, brak parkingu przy sklepie, zamknięcie), byłyby powodami, których nikt nie zapisze — `K-67` (`DG-1`) |
| V-2 ★ | **`Subject` ma szesnaście wariantów, a `TenderId`/`CaseId`/`PermitId` mieszkają w `engine/core`** (`K-69`). Partia i oferta jadą jako `ArenaRef` — uchwyt areny z zatartym typem | Wykonanie `K-62` i domknięcie `DE-9`. `core` nie może zależeć od `sim/city` ani znać `Batch`/`Offer` |
| V-3 ★ | **Karta inspekcji jest jedna dla wszystkich podmiotów**, a karta sklepu przestaje być osobnym oknem. Dok prawy ma jedną kartę i historię (`ui-design.md` §5), a nie stos okien | Odnośnik z karty mieszkanki do sklepu otwierałby inaczej trzecie okno (`DG-14`) |
| V-4 ★ | **`EntityFilter` i predykat wyboru postaci nie są `ConditionExpr`.** Filtr ma dwa warianty (`MyCustomers`, `MyEmployees`); „cysterny z paliwem" nie powstaje, bo klient rysuje pieszych, a nie pojazdy | Metryki języka reguł opisują firmę, nie mieszkańca — drzewo składniowe bez nich byłoby pustą ramą (`DG-4`, `DG-5`). Konsekwencja dla `M9d`: gramatyka nie musi obsłużyć ani filtra encji, ani predykatu kandydata |
| V-5 | **`PlayerCommand` ma cztery warianty**: `StartGame`, `SetPrice`, `SetCharacter`, `SetAutonomy`. Dwa ostatnie wykonuje `Session`, a nie `command::apply` — zmieniają świat **i** sesję naraz | `apply` dostaje widok, nie `&mut World`, i tak ma zostać (`DG-6`). `CommandView` rośnie o `world` i `has_character`, bo panel musi znać powód **przed** kliknięciem |
| V-6 | **`GameState` ma pięć wariantów**: doszedł `CharacterSelect(Box<Session>)`. Świat stoi i nie tyka, dopóki gracz nie wybierze postaci | Kandydaci powstają z postawionego świata, bo predykat pyta o wiek, pracę i oszczędności |
| V-7 | **Dziewięć nakładek z §14.2 liczy `game::overlays::build`**, a raster po działkach jest jeden (`magnat_world::parcel_raster`). Klient przełącza je klawiszem `F3` w tej samej pętli co nakładki terenu i rysuje legendę z `data/ui/overlays.ron` | Sześć z dziewięciu różni się wyłącznie stemplowaną liczbą (`DG-9`) |
| V-8 ★ | **Tryb przeglądu jest szóstą drogą wejścia do świata i nie jest wariantem startu** (`DG-16`, PRD §13.1). Mechanizmem jest **brak komendy `SetCharacter`**: sesja bez niej ma `player == None`, tyka i buduje karty. Koperta `StartGame` nie rośnie o ani jedno pole, `REPLAY_SCHEMA_VERSION` zostaje przy 1. Drogi wejścia są dwie i obie ustawiają tę samą flagę `Shell::observe`: pozycja „Tryb przeglądu" w menu głównym i flaga `--observe` klienta. Wiersz „Tylko oglądam — bez postaci" na ekranie wyboru postaci zostaje | Decyzja właściciela produktu z 2026-09-18. Korekta powstała w `M9c` i **nie doszła tutaj**, przez co zakres fazy w §2 i mapa PRD w §3 o trybie przeglądu nie wiedziały. Wejście wyłącznie z ekranu wyboru postaci okazało się w grze nieosiągalne: gracz dochodzi do niego dopiero po generacji świata, a po drodze Esc zamykał grę (`V-9`) |
| V-9 ★ | **Esc na ekranach powłoki należy wyłącznie do `egui`; pętla okna go nie dotyka.** `GameState::Shell` traci ładunek `ShellScreen` — który ekran jest na wierzchu, wie `Shell::screen` i tylko on. Dokąd prowadzi cofnięcie, mówi jedna tabela w `Shell::cofnij`, a ekran zgłasza samą intencję (`ShellAction::Back`). Wspólny nagłówek niesie klikalny przycisk „Wstecz" i ścieżkę, a menu pauzy otwiera `buduj_ui`, a nie pętla okna | Klawisz miał dwóch właścicieli i dwa źródła prawdy o bieżącym ekranie. `tools/magnat` wychodziło z gry, gdy `GameState` mówił „menu główne" — a mówił tak także w kreatorze, w ustawieniach i na liście slotów, bo wejście w nie zmieniało tylko `Shell::screen`. W menu pauzy oba właściciele widziały to samo naciśnięcie w jednej klatce i pauza zamykała się natychmiast po otwarciu |
| V-10 | **Znaki strzałek nie wchodzą do tekstów interfejsu, dopóki M11 nie wgra własnego kroju.** Domyślny atlas `egui` ich nie ma, więc `↑↓←→` wychodziły u gracza jako prostokąty — w podpowiedzi klawiszy powłoki, w nagłówku karty podróży i w znaczniku wybranej opcji. Pilnuje tego test `atlas_fontow_zna_wszystkie_znaki_z_lokalizacji` | Sprawdzenie atlasu istniało od M9b, ale obejmowało wyłącznie polskie diakrytyki z listy wpisanej do testu. Lista z góry nie mogła złapać znaku, którego nikt nie podejrzewał; przejście po wszystkich formach wszystkich kluczy łapie go i złapie następny |


## Zmiany wpisane po M9d

Zgodnie z `K-18`. Szczegóły i uzasadnienia — tabela `DF-n` w `M9d-jezyk-regul.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| U-1 ★ | **`ManagerExecution` mieszka w `sim/economy`, a nie w `game::policy`.** §6 „dostarczam" poprawione | Politykę wykonuje system ECS, a `sim/economy` nie może zależeć od `game/`. Jakość wykonania jest krokiem **wewnątrz** doby polityk, więc mieszka tam, gdzie doba. W `game/` zostaje edytor, diagnostyka, dry-run i tekst |
| U-2 ★ | **`PolicyRunner` jako osobny system ECS nie powstaje, a rozłożenie `(i*37) % 1440` z §7 nie jest potrzebne.** Polityki wykonuje `Market::run_policies` na granicy doby (i godziny dla `Cadence::Hourly`), w jednym przelocie po zakładach zdelegowanych | Budżet z §7 („≤ 1 ms sumarycznie na dobę, 200 zakładów") jest **spełniony bez rozproszenia** i zmierzony: 200 zakładów z polityką dyskontową liczy się poniżej milisekundy w wydaniu optymalizowanym. Rozproszenie po minutach doby kupowałoby wyrównanie kosztu w klatce, a płaciło regułą wykonywaną na stanie z przypadkowej godziny — czyli tym, co §5.6 nazywa kadencją. Wraca, jeśli budżet pęknie przy dziesięciu tysiącach zakładów |
| U-3 ★ | **Trzy z sześciu polityk przykładowych nie dają się dziś przypiąć** (`DF-2`): kadrowa nie ma wykonawcy, dwie mieszają dziedziny. Kryterium WP8 „sześć polityk zbudowanych wyłącznie klikaniem" jest spełnione — edytor buduje wszystkie sześć i przy trzech mówi, czego brakuje | Wariant, którego skutku nikt nie widzi, wygląda tak samo jak działający (`K-67`). Polityka, która „działa" i nic nie robi, jest gorsza od odmowy z powodem. Czy dziedzina ma zostać granicą polityki — decyzja otwarta nr 13 |
| U-4 | **`DecisionReason::PolicyApplied` rośnie o `lag_days` i `deviation_bp`** (`K-70`). Bramka 5 fazy (wyjaśnialność) domyka się w tej podfazie po stronie polityk: każde zastosowanie ma powód, a powód niesie odchyłkę menedżera | §5.6 obiecuje zdanie „cel 6,38 zł, menedżer ustawił 6,44 zł". Bez tych dwóch liczb nie ma go z czego złożyć, bo obu da się dowiedzieć wyłącznie w chwili wykonania |
| U-5 | **`PlayerCommand` ma sześć wariantów** (`AttachPolicy`, `DetachPolicy` na końcu enuma). Przypięcie polityki włącza śledzenie zakładu, bo bez śladu doby dry-run nie ma na czym pracować | `X-2`: wariant wchodzi razem ze swoim wykonawcą. Wykonawcą jest `Session`, bo komenda dotyka rejestru firm na mutowalnie |
| U-6 | **Edytor otwiera się w kliencie klawiszem `R` na zaznaczonym zakładzie**, a punktem wyjścia jest polityka, która na nim stoi — albo preset „Kurs stały" z `data/policies/`, czyli ten sam, od którego zaczyna firma AI | Ekran, do którego nie ma drogi, jest ekranem, którego skutku nikt nie widzi (`W-6`). Pusty formularz byłby przy tym uczciwy i bezużyteczny: gracz uczy się języka, patrząc na regułę, która działa |


## Zmiany wpisane po M9e

Zgodnie z `K-18`. Pełne uzasadnienia — tabela `DI-n` w `M9e-panele-czas-kariera.md`.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| DJ-1 ★ | **§6 „Dostarczam" traci `chronicle::record()`, a zyskuje `Chronicle::harvest`.** Kronika jest widokiem pochodnym: zbiera raz na dobę `Events::chronicle()`, pierścienie decyzji firm gracza i dziennik wejść | `game/` stoi nad wszystkimi `sim/*`, więc żaden system symulacji nie może do niego sięgnąć (`DI-4`) |
| DJ-2 ★ | **§6 traci `LayoutNode` i `Layout` (dokowanie) z listy `engine/ui`.** Dok jest `egui::Panel`, a `Layout` mieszka w `game::panels::layout` i jest listą przypiętych paneli | Ta sama decyzja, którą `M9b` podjął dla drzewa retained (`DE-4`): własny system układu nad `egui` byłby drugim toolkitem (`DI-5`) |
| DJ-3 ★ | **`GraphView` i `GanttView` powstały w `engine/ui`, zgodnie z `DF-2`** — razem z panelem Łańcuch dostaw i panelem Zakład. Graf układa się warstwowo z odciskiem topologii, Gantt wirtualizuje po oknie czasu | Budżety §7 mierzy się na widgecie rysującym prawdziwe dane, a nie dane zbudowane na potrzeby pomiaru (`DE-5`) |
| DJ-4 ★ | **Zbiór „sensownych decyzji" §20.3 ma pięć komend, nie osiem.** `SetPrice`, `AttachPolicy`, `OpenSite`, `ApplyForJob`, `HireCandidate` | `AcceptJobOffer` nie powstaje: w tej gospodarce etat wygrywa się **zgłoszeniem i doborem**, a nie kliknięciem „przyjmuję" — rynek pracy nie ma kroku akceptacji poza ofertą bezpośrednią, a ta i tak jedzie przez `apply_for` (`DH-2` domknięte) |
| DJ-5 | **`PlayerCommand` ma dwadzieścia wariantów**, `ViewCommand` sześć. Kolejność jest kontraktem dziennika wejść i dopisywać wolno wyłącznie na końcu | Lista z `M9a` §5.5 zapowiadała ~70 i zostaje projektem: wariant wchodzi razem ze swoim wykonawcą i ze swoim panelem (`X-2`) |
| DJ-6 | **`data/scenarios/scenarios.ron` jest katalogiem scenariuszy, a `data/scenarios/` ma od tej chwili dwóch właścicieli** (`K-71`) | Zapowiedziane w §5 dokumentu 00 słowami „format scenariusza opisanego danymi projektuje M9" |
