# M10g — Panele, komendy i karty

Podfaza 7 z 7 fazy **M10 — Głębia** (`M10-glebia.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M10f (kronika, hash, bramki, pomiary). |
| **Pakiety robocze** | WP10.19, WP10.20, WP10.21, WP10.22 |
| **Projekt techniczny** | §5.11 (nowa sekcja, poniżej) |
| **Wynik do pokazania** | Artefakty C i D z §1 dokumentu fazy: gracz kupuje billboard i widzi, kogo dosięgnął; wchodzi na giełdę, prowadzi badania i negocjuje z załogą. |
| **Kryterium zamknięcia** | Kryteria WP10.19–WP10.22 **oraz bramki 1–7 fazy M10** w `00-postep.md`. |
| **Poprzednia / następna** | `M10f-kroniki-i-domkniecie.md` · — (ostatnia w fazie) |

Podfaza istnieje, bo przez cztery podfazy z rzędu ta sama luka wracała pod innym
numerem: `FF-1` (kampanie), `FF-9` (badania), `FF-14` (giełda), `FF-23`
(negocjacje). Za każdym razem mechanika była gotowa po stronie symulacji,
a gracz nie miał jak jej dotknąć. Wspólna droga wyjścia jest jedna i opisuje
ją `DK-3`: panel dokłada się przez `PanelRegistry::register`, a `PanelId::{Brand,
Rnd, Stock}` już istnieją i `is_reserved()` mówi o nich prawdę.

## Granica wobec M10f

M10f nie dotyka `game/` poza kroniką; M10g nie dotyka `sim/*` **poza jednym
punktem podstawienia** — ustępstwem gracza w `talks::rundy` (`FF-23`). Reguła
jest ta sama, którą `PricePolicy` z M5c postawiło dla cen: gracz podstawia
własną liczbę w miejsce wyliczonej, a nie dostaje drugiego silnika.

---

## Pakiety robocze

### WP10.19 — Panel marki i kampanie gracza

**Zależności:** M10b, M9e (`PanelRegistry`). Wykonuje `FF-1` i `FF-3`.

Panel `PanelId::Brand`: mapa cieplna ekspozycji per dzielnica, lejek
„znajomość → próba → afinitet" z `CampaignMetrics`, wykres `expected_quality`
vs `actual_quality` (M10 §6 pkt 1 — to jedyne miejsce, w którym gracz zobaczy,
że przereklamował produkt). Komenda gracza otwierająca kampanię na wskazanym
kanale, z budżetem i czasem trwania; walidacja i wykonanie tą samą drogą co
`SetPrice` (`check.rs` → `exec.rs` → `apply`).

`FF-3` domyka się przy okazji i bez nowego kodu w `sim/*`: `BrandLearned`
i `BrandExperience` dostają trwałego czytelnika w lejku panelu — pełnego logu
kontaktów dla 400 tys. mieszkańców nie było i nie będzie (14 GB na rok gry).

`FF-4`: zakład z wpisem w `Outlets` dostaje w karcie zakładu sekcję tytułu
medialnego — czytelnictwo, wiarygodność, linia redakcyjna. Bez wariantu
`Subject`, bo tytuł **jest** zakładem.

`OverlayField::BrandAwareness` ma wpis w `data/ui/overlays.ron` od M10b —
`DK-4` jest wykonane i nie ma tu nic do zrobienia. Komentarz przy `PanelId`
mówiący co innego jest nieaktualny i znika razem z rejestracją panelu.

**Kryterium ukończenia:** gracz kupuje billboard przy wskazanej ulicy i po
tygodniu gry widzi w panelu, ilu mieszkańców i z jakich dzielnic zostało
wyeksponowanych; po miesiącu karta mieszkańca pokazuje slot tej marki
ze źródłem kontaktu.
**Rozmiar: L.**

---

### WP10.20 — Panel R&D i karta technologii

**Zależności:** M10c. Wykonuje `FF-9` i `FF-10`.

Panel `PanelId::Rnd`: drzewo technologii per branża z postępem i rokiem
„światowym", lista patentów własnych i cudzych z ofertami licencji
(M10 §6 pkt 2). Komenda „badaj X" — do M10c projekt wybiera reguła
„najtańszy osiągalny węzeł" i gracz nie ma na to żadnego wpływu.

`FF-10`: cztery powody R&D renderują dziś numer węzła (`#N`), bo
`reason::describe` nie zna drzewa — drzewo mieszka w `sim/firms` i `data/tech/`.
Nazwę podmienia panel, który drzewo trzyma; to jest ta sama granica, którą
`GoodId` ma od M6.

`FF-12` **nie wchodzi**: trzy efekty technologii (`NewRecipe`, `ProductFeature`,
własna montownia telefonów) potrzebują archetypu budynku i karty towaru,
czyli czytelników, których nie ma ani tu, ani w M10. Adres zostaje w `FD-1`,
`FD-3`, `FD-5`.

**Kryterium ukończenia:** gracz wskazuje węzeł do badania i widzi postęp
w miesiącach; powód `TechDiscovered` w karcie inspekcji pokazuje **nazwę**
węzła w obu językach, nie numer.
**Rozmiar: M.**

---

### WP10.21 — Panel giełdy, zlecenia i karta polisy

**Zależności:** M10d. Wykonuje `FF-14`, `FF-15` i `FF-16`.

Panel `PanelId::Stock`: księga zleceń przed fixingiem, historia kursu,
akcjonariat z progami 5/25/50 %, kalendarz publikacji wyników (M10 §6 pkt 3).
Komenda „złóż zlecenie" — kanał po stronie symulacji jest otwarty i nie wymaga
zmian: `Equity::place_order` przyjmuje `Owner::Player`, a `pay::player_citizen`
znajduje jego gospodarstwo.

`Subject::Cover(CoverId)` dokłada się **razem z kartą polisy**, nie przed nią:
wariant `Subject` bez ramienia w `game::inspect::card` łamie kompilację
(`K-69`), a ramię bez treści pokazuje pustą stronę. Karta niesie przedmiot
ubezpieczenia, sumę, składkę, udział własny i historię szkód.

`FF-16`: kurs jest ceną **jednego punktu bazowego** i sama ta liczba jest dla
gracza nieczytelna. Panel pokazuje obok niej wycenę firmy — tekst, nie drugi
przelicznik (drugi przelicznik byłby tą samą drugą prawdą, którą `GD-1` usunął).

**Kryterium ukończenia:** firma gracza debiutuje na giełdzie, gracz składa
zlecenie kupna i widzi jego los po fixingu; polisa otwiera się jako karta
z własną historią szkód.
**Rozmiar: L.**

---

### WP10.22 — Karty relacji i negocjacje gracza

**Zależności:** M10e. Wykonuje `FF-22` i `FF-23`, domyka `DK-6`.

Cztery rzeczy:

1. **Panel pracowniczy** (M10 §6 pkt 5): `grievance` per zakład jako ostrzeżenie
   wyprzedzające, przebieg negocjacji, licznik funduszu strajkowego.
2. **Negocjacje gracza** (`FF-23`): komendy „przyjmij żądanie", „kontruj",
   „przeczekaj". Po stronie `sim/economy` zmiana jest jedna — `talks::rundy`
   przyjmuje ustępstwo podstawione przez gracza w miejsce wyliczonego z marży.
3. **Karty bez własnego wariantu `Subject`** (`FF-22`): związek zawodowy jest
   sekcją karty **zakładu** (tak jak tytuł medialny), zmowa — sekcją karty
   **firmy**, relacja z dostawcą — sekcją karty firmy. Dane są gotowe:
   `Unions::iter`, `Cartels::iter`, `B2b::relations`.
4. **`DK-6`**: `game::timectl::StopCondition` dostaje wariant „strajk",
   a `Goal::ProductLaunched` — swoje zgłoszenie. Oba enumy są w `game/`,
   więc to jest wariant, ramię i klucz tekstu w obu językach.

**Kryterium ukończenia:** załoga zakładu gracza przedstawia żądanie, gracz
kontruje własną liczbą i negocjacje kończą się inaczej niż przy regule AI;
zatrzymanie czasu na strajku działa; zmowa widoczna w karcie firmy
**po wykryciu**, nie wcześniej.
**Rozmiar: L.**

---

## Projekt techniczny

### 5.11 Wzorzec panelu M10

Numeracja `5.x` jest ciągiem z dokumentu fazy; `5.10` (systemy ECS) mieszka
w `M10f-kroniki-i-domkniecie.md`.

Panel dokłada się **czterema zmianami i ani jedną więcej** (`DI-6`):

1. nowy plik `game/src/panels/<nazwa>.rs` ze stałą `DESC: PanelDesc`, funkcją
   `build(&PanelCtx) -> PanelModel` i `render(…) -> PanelAction`,
2. `mod <nazwa>;` w `game/src/panels/mod.rs`,
3. wariant w `enum PanelModel`,
4. wpis w `PanelRegistry::default()` **albo** wywołanie `register()` — pierwsze
   dla paneli gry, drugie zostaje dla modów M12.

`PanelId::is_reserved` traci przy tym trzy warianty i po M10g zwraca `false`
dla wszystkiego; `is_operational` zostaje bez zmian przy jednym wyjątku
(kronika jest dziennikiem i komendy mieć nie może, `DH-3`).

Wszystkie teksty idą przez `ctx.text`/`ctx.fmt` i klucze w `data/locale/pl.ron`
**oraz** `en.ron` w tej samej zmianie (CLAUDE.md). Panel czyta `&Session`,
nigdy `&mut` — komendę składa i oddaje, a wykonuje ją sesja w punkcie
synchronizacji.

### 5.12 Sufit siedmiu zakładek

`MAX_CARD_TABS = 7` jest twardym `assert!`, a karta mieszkańca już go dotyka
(`FF-7`). Trzy sekcje z tej podfazy — tytuł medialny, związek, zmowa — wchodzą
więc jako **sekcje istniejących zakładek**, a nie nowe zakładki. Karta polisy
jest nowym podmiotem, więc zaczyna od jednej zakładki i ma zapas.

---

## Zmiany wpisane po M10f

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu
M10f. Gwiazdka = zmiana zakresu albo kryterium. Szczegóły — sekcja „Wyniki
przebiegu pomiarowego" w `M10f-kroniki-i-domkniecie.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| GF-1 ★ | **Dwa kanały reklamowe przebiegają po całej populacji na kampanię, na dobę, i to jest pakiet dla tej podfazy.** `promocja` (`sim/media/src/system.rs`) zbiera mieszkańców wszystkich dzielnic i pyta każdego `knows_place`; `pr` robi to samo z `affinity_of` i klonuje przy tym tabelę pamięci marki. Zmierzone przy 250 kampaniach i 20 tys. mieszkańców: **InStorePromo 6079 ms i Pr 6479 ms na dobę gry**, wobec 1–10 ms każdego z pozostałych sześciu kanałów | Kryterium §7.5 („systemy reklamowe ≤ 1,5 % budżetu ticku przy 2000 kampanii") jest przekroczone **55×** i zamyka bramkę 4 fazy, a bramki zamykają się dopiero z M10g. Kierunek naprawy wynika z pomiaru: przebieg po populacji ma być **raz na dobę dla wszystkich kampanii**, a nie raz na kampanię — albo marka ma dostać indeks „kto mnie zna". Bramką jest `sim/media/tests/reach.rs::dwa_tysiace_kampanii_miesci_sie_w_budzecie_ticku`, dziś czerwona i `#[ignore]` |
| GF-2 ★ | **Kanał ulotkowy nie dociera do nikogo, a to na niego schodzą zubożałe firmy.** Histogram kanałów po 300 dobach: **`Leaflet` 6 kampanii, 0 ekspozycji**, i żadnego innego kanału (M10b mierzyło 35 kampanii i 482 tys. ekspozycji po 40 dobach, bo bogate firmy kupowały prasę i telewizję) | `ulotki` (`sim/media/src/system.rs`) ma **jedno** wyjście kończące się zerem bez próby doręczenia: `PlaceCatalog::coord_of(PlaceRef::Site(origin))` zwraca `None`. Kandydatem na kampanię jest **każdy zakład z firmą**, a katalog miejsc zna te, które mieszkaniec odwiedza — zakład produkcyjny wypada z niego z definicji. Mechanizm wyglądał na działający przez dwie podfazy, bo raport pokazywał sumę ekspozycji, a nie rozbicie na kanały. **Panel marketingu zbudowany na tym stanie pokaże pustą tabelę i będzie miał rację**, więc naprawa idzie przed panelem |
| GF-3 | **Trzy mechaniki M10 są zagłodzone jedną przyczyną: `FF-29`.** Badania (`FF-11`: 25 projektów, 0 odkryć), giełda (`FF-18`: 0 debiutów po 300 dobach) i reklama (`GF-2`) stoją na gotówce firm, a ta przez 300 dób spada z 395 do 229 mln zł, podczas gdy u gospodarstw rośnie z 6 do 169 mln | `PayrollOutbox` nie ma konsumenta od M7b, więc dochód gospodarstwa jest egzogeniczny i **pieniądz firm wycieka drugą drogą**. To samo widzi bramka `G12` (mediana odchylenia makro od mezo **879 ‰** przy zgodności w dobie zero). Domknięcie tego kanału przestawia kalibrację kopert, kredytu, CPI i bramek G1–G3 naraz, więc jest własnym pakietem, nie dopiskiem — ale **trzy pakiety M10g stoją na jego wyniku** |
| GF-4 | **`G12` jest bramką doradczą i ma nazwany warunek, kiedy przestaje nią być.** Świeci i wypisuje liczbę co noc, ale nie wywraca profilu | Ta sama droga, którą `G4` jest doradcza do czasu M6: bramka czerwona z powodu nieistniejącego kanału uczy wyłącznie ignorowania bramek. `advisory: true` znika razem z konsumentem `PayrollOutbox` |
| GF-7 ★ | **Bezrobocie w makrze wynosi zero i to jest regresja po `GE-12`, a nie stan zastany.** Bramka 4 Etapu 10 daje **0 ‰** na 4 km i na 8 km wobec pasma 30–150; `G12` widzi to samo jako 863 ‰ odchylenia od mezo. Odsetek firm bez obsady spadł przy tym z 291 ‰ do 167 ‰ | Rekrutacja ważona gotowością do dojazdu (decyzja `D10`, wykonana w M10e) zatrudnia praktycznie wszystkich — czyli zaszło dokładnie to, przed czym `E-12` ostrzegało przy wcześniejszej próbie otwarcia puli na całe miasto. **To jest defekt M10, nie brakujący kanał**, więc w odróżnieniu od `GF-3` nie zasłania się `FF-29` i seria bezrobocia w `G12` ma zzielenieć po naprawie. Bez bramki `G12` nie dowiedziałby się o tym nikt: mezo zera nie widzi, a `what_if()` zwraca uporządkowanie, nie poziom |
| GF-8 | **Bramka 7 Etapu 10 jest po `D9` zielona na mieście 4 km (704 ‰), a na 8 km wychodzi 872 ‰ — 22 ‰ nad krawędzią pasma 550–850.** Nowa bramka 10 (Gini dochodu) daje 613 i 507 ‰ wobec 250–450 | Rozdzielenie bramki zadziałało: liczba, która świeciła na czerwono od pierwszego pomiaru i nie wiadomo było, czy z winy generatora, czy progu, ma teraz dwa osobne pomiary i dwa osobne pytania. Otwarte zostaje, czy górna krawędź 850 ‰ jest za ciasna dla większych miast — to jest pytanie o **pomiar na pełnym zakresie rozmiarów** (M10 §7.3), nie o próg z sufitu |
| GF-5 | **Karencja zdarzenia wiąże powyżej 556 zakładów**, więc metropolia ma pożarów na zakład czterokrotnie mniej niż miasto 4 km. `cooldown_days` jest przerwą definicji, nie podmiotu | Nie jest to usterka i nie należy do M10g — jest to **własność modelu, której nikt nie zapisał**, a która zmienia wycenę polisy w dużym mieście. Adres, gdyby miała się zmienić: karencja per podmiot zamiast per definicja, w `sim/events` (crate M8) |
| GF-6 | **`BRAND_SLOTS` zostaje 16 i decyzja `D4` fazy jest zamknięta.** Mediana liczby marek u mieszkańca, który zna jakąkolwiek, wynosi **6** przy sufcie 16; próg podniesienia to 13 | Pomiar, na który `D4` czekała od M10b. Panel marketingu nie musi zakładać 24 slotów |
