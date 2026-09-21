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

---

## Wyniki przebiegu pomiarowego

Liczby z domknięcia podfazy. Przyrządem jest ten sam scenariusz, który mierzył
M10f — `m7miasto` na mieście 4 km i `dry-run --years 30` — żeby porównanie
było porównaniem, a nie dwoma różnymi światami.

| Co | Przed M10g | Po M10g | Uwaga |
|---|---|---|---|
| Koszt 2000 kampanii na dobę gry (`FF-8`, budżet 225 ms) | **12 514 ms** | **50 ms** | `GF-1` zamknięte; bramka `dwa_tysiace_kampanii_miesci_sie_w_budzecie_ticku` przeszła z czerwonej na zieloną |
| — w tym `InStorePromo`, 250 kampanii | 6 079 ms | **13 ms** | jeden przebieg po mieście dla wszystkich promocji, nie jeden na kampanię |
| — w tym `Pr`, 250 kampanii | 6 479 ms | **22 ms** | j.w., plus koniec klonowania tabeli pamięci marki na kampanię |
| Kanał ulotkowy z zakładu spoza katalogu miejsc | 0 ekspozycji | dostarcza | `GF-2`; test `ulotka_dociera_z_zakladu_spoza_katalogu_miejsc` |
| Bramka 8 Etapu 10 (koszyk do dochodu, 250–550 ‰) | **0 ‰** czerwona | **411 ‰** zielona | skutek uboczny naprawy licytacji płacowej |
| Bramka 10 Etapu 10 (Gini dochodu, 250–450 ‰) | **613 ‰** czerwona | **348 ‰** zielona | j.w. |
| Bramka 7 Etapu 10 (Gini majątku, 550–850 ‰) | 704 ‰ zielona | **665 ‰** zielona | bez zmiany werdyktu |
| Bramka 4 Etapu 10 (bezrobocie, 30–150 ‰) | **0 ‰** czerwona | **0 ‰** czerwona | `GF-7` **nie domknięte** — powód niżej, `GG-3` |
| Suma pieniądza po 30 latach historii „na sucho" | zachowana | zachowana | K1, tolerancja 0 groszy; po drodze **złamana i naprawiona**, patrz `GG-2` |
| Kliknięcie w panel wydaje komendę (M9e WP10) | 9 paneli | **11 paneli** | marka i badania w sweepie dobowym, giełda we własnym teście (`GG-7`) |

Panele i karty mierzy kompilator i testy, nie zegar: `PanelId::is_reserved`
zwraca od tej chwili `false` dla wszystkiego, `Subject` ma siedemnaście
wariantów i tyle samo ramion w `game::inspect::card`, a zbiory kluczy
`pl.ron` i `en.ron` są identyczne (`klucze_obu_jezykow_sa_identyczne`).

### Czego ten przebieg nie zmierzył

`FF-29` — konsument `PayrollOutbox` — **nie powstał w tej podfazie i nie miał
powstać**: `GF-3` nazywa go własnym pakietem, bo przestawia kalibrację kopert,
kredytu, CPI i bramek G1–G3 naraz. Trzy liczby, które na nim stoją (gęstość
badaczy, liczba debiutów giełdowych, budżety reklamowe firm), zostają więc
nieporuszone, a bramka `G12` zostaje **doradcza** zgodnie z `GF-4`. Panele tego
nie zasłaniają i nie miały zasłaniać: pokazują to, co jest, a jeśli firm nie stać
na badania, panel R&D pokaże pusty projekt i będzie miał rację.

---

## Zmiany wpisane po M10g

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu
podfazy. Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| GG-1 ★ | **Katalog miejsc nie odbudowuje się po założeniu zakładu — i to jest dług szerszy niż reklama.** `PlaceTable` powstaje **raz**, przy zaludnianiu miasta, z zakładów generatora (`sim/world::population::katalog_miejsc`). Zakład założony w trakcie gry — przez `firmlife::found`, przez `OpenSite` gracza — nigdy do niego nie trafia, więc `PlaceCatalog::coord_of(Site)` odpowiada dla niego `None` na zawsze | `GF-2` naprawiono **w miejscu objawu**: punkt nadania ulotki ma teraz trzy próby (zakład, jego budynek, środek ciężkości domów dzielnicy) i test, który tego pilnuje. Przyczyna zostaje: każde pytanie o położenie nowego zakładu dostaje `None`. Adres naprawy: przebudowa `PlaceTable` przy zmianie rejestru zakładów, razem z indeksem, który z niej korzysta — **własny pakiet z własnym przebiegiem**, nie dopisek do kanału reklamowego. Sufit nazwany `ponytail:` w `sim/media::punkt_nadania`. **Czego nie zmierzono:** histogram kanałów po trzystu dobach, czyli dokładnie ten pomiar, którym `GF-2` została znaleziona. Dowodem naprawy jest test stawiający ten sam układ (zakład spoza katalogu, mieszkańcy z domami w katalogu) i przebieg siedemdziesięciu dób, w którym kanał dostarcza 251 940 ekspozycji — ale to jest przebieg **sprzed** progu, na którym objaw się pojawiał |
| GG-2 ★ | **Licytacja płacowa w makrze nie miała sufitu i nikt tego nie widział przez dwie podfazy.** `wage_escalation_step` **nigdy nie zwraca zera** (M7: „krok zerowy zatrzymałby licytację przy stawkach groszowych"), a jedynym ogranicznikiem był `sufit = stawka × 3` — liczony z **dzisiejszej** stawki, więc przesuwający się razem z nią. Firma z trwale otwartym wakatem podnosiła płacę o dwa procent na dobę bez końca | Do M10g nie było tego widać, bo bez odejść dobrowolnych każdy wakat kiedyś się zamykał i licytacja milkła sama. Pierwsza próba domknięcia `GF-7` (odejścia w fazie 2) odsłoniła to natychmiast: po trzydziestu latach firmy pożyczały na listę płac **stukrotność** obrotu miasta, a suma pieniądza w mieście rosła z 40 mld do 4,5 bln groszy. **Naprawa ma dwie części, obie odtwarzają regułę mezo:** stawkę rusza dopiero **nieudana** rekrutacja (mezo: `search.escalate_after_days`), a widełki są zakotwiczone w płacy odniesienia z danych, nie w dzisiejszej stawce (mezo: widełki roli z `data/jobs/roles.ron`). Skutek uboczny jest duży i dodatni: bramki 8 i 10 Etapu 10 zzieleniały |
| GG-3 ★ | **`GF-7` nie domyka się rotacją i przyczyna jest inna, niż zakładał wpis.** Odejścia dobrowolne weszły do fazy 2 kroku makro tą samą liczbą, którą liczy je mezo (`hr.quit_base_per_10k`, `K-50`) — i bezrobocie dalej wynosi **0 ‰**. Powód jest arytmetyczny: miasto 4 km ma **24 645 etatów na ~20 tys. osób w wieku produkcyjnym**, więc popyt na pracę trwale przewyższa podaż i pula pustoszeje w każdym kroku niezależnie od tego, ilu ludzi z niej odejdzie. Przy rotacji 7 % rocznie zapas bezrobotnych w równowadze to ułamek promila, a nie 30–150 ‰ | Mezo pokazuje w tym samym mieście 3–12 %, bo **dopasowuje po rolach**: kandydat ma umiejętność, wykształcenie i próg płacowy, więc część ludzi nie pasuje do żadnego wakatu. Makro traktuje pracę jako jednorodną — `MacroCell` ma tablice `labor` i `skill_sum` per rola, ale faza 2 ich **nie czyta** i zsypuje wszystkich do jednej puli dzielnicowej. To jest prawdziwy adres `GF-7` i jest to **zmiana modelu fazy 2**, nie parametr: rekrutacja per rola, z progiem umiejętności. Właściciel: M10a (§5.7), wykonanie poza M10 — bramka 4 zostaje czerwona z **nazwaną** przyczyną, zamiast czerwonej z nieznaną |
| GG-6 ★ | **Panel giełdy dostał piątą komendę, której plan nie przewidywał: „wprowadź moją spółkę na giełdę".** Kryterium WP10.21 zaczyna się od „firma gracza **debiutuje** na giełdzie", a `WP10.21` wymieniał jako komendę wyłącznie „złóż zlecenie" — bo `FF-14` zakładał, że kanał symulacji jest otwarty i wystarczy go podłączyć. Debiut był **decyzją firmy AI**: prywatna `fn debiuty` przechodziła raz na miesiąc po firmach o kursie na wzrost i wprowadzała je same | Gracz nie miał jak wejść na giełdę i nie miałby nigdy, bo jego firma nie ma `FirmStrategy` ustawianej przez tier taktyczny. `equity::system::debut` jest od tej chwili **publiczna i jednofirmowa**, a `debiuty` woła ją w pętli — jeden próg dla AI i dla gracza (`K-11`), zamiast dwóch giełd. Próg zostaje ten sam i twardy: **opublikowany dodatni wynik**, czyli najwcześniej doba 75 |
| GG-7 | **Test „z każdego panelu operacyjnego da się wydać komendę" (M9e WP10) pomija giełdę i ma na to powód wpisany w kod.** W dobie pierwszej nie jest notowana ani jedna spółka i notowana być nie może; panel wystawia wtedy jeden przycisk, wygaszony z nazwanym powodem | To nie jest osłabienie kryterium, tylko jego doprecyzowanie: kryterium mówi o panelu, a nie o świecie, w którym panel akurat stoi. Giełda ma własny test (`panel_gieldy_sklada_zlecenie_gdy_jest_co_kupowac`), który **stawia notowanie ścieżką symulacji** i sprawdza, że kliknięcie w panel składa zlecenie, a zlecenie trafia do arkusza. Przewijanie świata o siedemdziesiąt pięć dób mierzyłoby przy okazji rentowność firm w pierwszym kwartale i pękałoby z powodu, który z panelem nie ma nic wspólnego |
| GG-4 | **Panel dokłada się czterema zmianami i to się sprawdziło co do liczby.** Trzy panele (`Brand`, `Rnd`, `Stock`) weszły przez `PanelRegistry::default()`, wariant `PanelModel`, plik w `game/src/panels/` i klucz tytułu — bez ani jednej zmiany w `M9e`. `PanelId::is_reserved` zwraca od tej chwili `false` dla wszystkiego i **tak ma zostać**: identyfikator bez ekranu jest wariantem bez skutku (`K-67`) | Wzorzec z `DI-6` i `DK-3` opisywał drogę, której nikt jeszcze nie przeszedł. Przeszedł ją M10g trzy razy i za każdym razem kosztowała cztery zmiany. Uwaga dla M12 (panele z modów): jedyną rzeczą, której mod nie zrobi tą drogą, jest wariant `PanelModel` — enum jest w `game/` i mod do niego nie dopisze |
| GG-5 | **Karty głębi nie dostały wariantów `Subject` i to było właściwe.** Tytuł medialny, związek zawodowy, zmowa i relacja z dostawcą weszły jako **sekcje** kart zakładu i firmy (`FF-4`, `FF-22`); własny podmiot dostała wyłącznie polisa (`FF-15`), bo jako jedyna nie jest stanem czegoś, co kartę już ma | Kryterium było jedno: czy gracz zapyta „pokaż mi ten byt", czy „co się dzieje w tej hali". Sufit siedmiu zakładek (`MAX_CARD_TABS`) nie drgnął — karta sklepu ma pięć, karta firmy dwie |
