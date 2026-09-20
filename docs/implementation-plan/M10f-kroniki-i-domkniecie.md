# M10f — Kroniki i domknięcie

Podfaza 6 z 6 fazy **M10 — Głębia** (`M10-glebia.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M10a–M10e. |
| **Pakiety robocze** | WP10.15, WP10.16 |
| **Projekt techniczny** | §5.10 |
| **Wynik do pokazania** | Pełny artefakt fazy z §1 dokumentu fazy: marka z pamięci agentów, R&D, giełda, historia „na sucho”. |
| **Kryterium zamknięcia** | Kryteria WP10.15 i WP10.16 oraz bramki 1–7 fazy M10 w `00-postep.md`. |
| **Poprzednia / następna** | `M10e-relacje-i-zwiazki.md` · — (ostatnia w fazie) |

Kroniki i wyjaśnialność dla wszystkich nowych mechanik oraz dopisanie stanu M10 do funkcji haszującej.

---

## Pakiety robocze

### WP10.15 — Kroniki i wyjaśnialność

**Zależności:** M9 (podsystem kroniki), wszystkie WP tej fazy.
Przekrojowy. Każda nowa decyzja ma `DecisionReason` (dok. 00 §7). Nowe warianty `ChronicleEvent` z §5.9.
**Kryterium ukończenia:** 100% nowych decyzji ma czytelny powód w karcie inspekcji; audyt ręczny
20 losowych wpisów kronikarskich pod kątem zrozumiałości dla gracza.
**Rozmiar: S.**

---

### WP10.16 — Determinizm i hash stanu

Przekrojowy, Definition of Done fazy. Nowe warianty `StreamId`, dopisanie nowych komponentów do
funkcji haszującej, testy dwóch przebiegów. **Rozmiar: S.**

---

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.10 Systemy ECS i ich częstotliwość

| System | Częstotliwość | LOD | Odczyt/zapis |
|---|---|---|---|
| `brand_decay_compact` | `EveryMonth` | mezo | W: `BrandSlots` |
| `ad_expose_billboard` | `EveryHour` | mezo | R: przejazdy M4; W: `BrandSlots`, `CampaignMetrics` |
| `ad_expose_media` | `EveryDay` | mezo | R: `MediaOutlet`; W: `BrandSlots` |
| `ad_expose_leaflet` | `EveryDay` | mezo | R: indeks przestrzenny M2; W: `BrandSlots` |
| `ad_budget_burn` | `EveryDay` | — | W: `Ledger` przez `ledger_post` |
| `brand_strength_aggregate` | `EveryDay` | — | R: `BrandSlots`; W: agregat UI |
| `media_editorial` | `EveryDay` | — | R: `sim/events`; W: `Rumor` |
| `rnd_progress` | `EveryDay` | — | R: badacze, budżet; W: `ResearchProject` |
| `rnd_unlock` | `EveryDay` | — | W: `Patent`, efekty technologii |
| `epoch_advance` | `EveryMonth` | — | W: `EpochState`, koszyki potrzeb |
| `investor_decide_funds` | `EveryWeek` | — | W: `Order` |
| `investor_decide_citizens` | `EveryMonth` | — | W: `Order` |
| `stock_fixing` | `EveryDay` (17:00) | — | R/W: `OrderBook`, `Holding`, `Share` |
| `earnings_publish` | `EveryDay` (sprawdza harmonogram T+45) | — | W: `PublishedReport` |
| `takeover_check` | `EveryDay` | — | W: `FirmPersonality` (M7), `Rumor` |
| `dividend_pay` | `EveryMonth` | — | W: `Ledger` |
| `insurance_underwrite` | `EveryMonth` | — | R: `PerilStats`; W: `InsurancePolicy` |
| `insurance_claims` | `EveryDay` | — | R: `sim/events`; W: `Ledger`, `PerilStats` |
| `supplier_trust_update` | `EveryDay` | mezo+makro | W: `SupplierRelation` |
| `cartel_detect` | `EveryMonth` | — | W: kary, `BrandSlots`, kronika |
| `union_grievance` | `EveryDay` | — | R: księgi M7, graf relacji M3 |
| `union_negotiate` | `EveryWeek` | — | W: `Union`, płace |
| `strike_tick` | `EveryDay` | — | W: produkcja zakładu, `Ledger` |
| `macro_step` | `EveryDay` (tylko w trybie makro) | makro | R/W: `MacroState` |

---


---

## Zmiany wpisane po M10b

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M10b.
Gwiazdka = zmiana zakresu albo kryterium. Szczegóły — tabela `F-n` i sekcja
„Co M10b zostawia następnym podfazom" w `M10b-marka-i-media.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| FF-1 ★ | **Panel marketingu i komenda „kup reklamę" należą do tej podfazy.** M10b otwiera kampanie wyłącznie przez `magnat_media::ai` — raz na miesiąc, dla firm AI. Gracz nie ma jak kupić billboardu, więc artefakt C z §1 dokumentu fazy nie jest jeszcze osiągalny | `DK-3` opisuje drogę: panel dokłada się przez `PanelRegistry::register`, a `PanelId::Brand` już istnieje i `is_reserved()` mówi o nim prawdę. Dane dla panelu są gotowe: `CampaignMetrics` niesie ekspozycje i **pierwsze kontakty**, czyli lejek „znajomość → próba → afinitet" z M10 §6 pkt 1 |
| FF-2 | **Dziennik redakcji jest gotowy do zebrania: `Outlets::reasons() -> &[(Tick, DecisionReason)]`**, pierścień 128 wpisów, w hashu stanu | Wykonanie `DK-1`: zdarzenie M10 zapisuje powód **u siebie**, a `game::chronicle::Chronicle::harvest` dokłada dla niego źródło. Wzorzec jest ten sam co przy `Events::reasons()` |
| FF-3 ★ | **`DecisionReason::BrandLearned` i `BrandExperience` nie mają trwałego czytelnika.** Są zwracane przez `magnat_agents::touch` i rysowane przez `reason::describe`, ale nikt ich nie zapisuje | Pełny log decyzji dla 400 tys. mieszkańców to 14 GB na rok gry (M3, decyzja 9.16), więc karta pokazuje **stan** slotu (zakładka „Marki"), a nie historię kontaktów. Trwałym czytelnikiem ma być panel marketingu i lejek z `CampaignMetrics` — czyli `FF-1` |
| FF-4 ★ | **Tytuł medialny nie ma karty.** Otwiera się jako `Subject::Site`, bo jest zakładem, ale czytelnictwo, wiarygodność i linia redakcyjna nie mają gdzie się pokazać | Faza dokładająca byt z kartą dokłada wariant `Subject` **i** ramię w `game::inspect::card` (`K-69`). Tu wariantu nie trzeba: tytuł **jest** zakładem, więc wystarczy rozgałęzienie w karcie zakładu — zakład z wpisem w `Outlets` dostaje dodatkową sekcję |
| FF-5 ★ | **Wiarygodność tytułu nie spada i to jest jedyna obietnica §5.3, której M10b nie dowiózł.** Mechanizm ma nośnik (slot marki tytułu w pamięci czytelnika), ale nie ma wyzwalacza: żeby czytelnik stracił zaufanie, trzeba porównać **tezę** tekstu z jego własną obserwacją, a `Story` niesie dziś `EventId`, nie tezę | Rozstrzygnięcie, którego M10b nie umiał podjąć: teza tekstu to albo nowe pole w `Story` (kierunek i siła oceny), albo wyprowadzenie z kategorii i skali zdarzenia. Pierwsze jest uczciwsze, drugie darmowe. **Propozycja domyślna: wyprowadzenie**, bo redakcja i tak nie ma z czego zbudować tezy innej niż „to zdarzenie jest takie a takie" |
| FF-6 | **Czytelnictwo tytułu jest regułą bez rozrzutu**: najlepiej w swojej dzielnicy, dwa razy słabiej poza nią. Numer `StreamId` na rozrzut **nie jest zarezerwowany** | Dopóki nikt czytelnictwa nie stroi, numer zapisałby na wieczność liczbę bez właściciela — ta sama reguła, którą `K-63` zastosował do `EventHazard`, a `K-67` do `CityPolicy` |
| FF-7 | **Karta mieszkańca ma siedem zakładek, czyli sufit `MAX_CARD_TABS`** (`F-18`) | Ósma wymaga decyzji, którą z obecnych złożyć. Kronika mieszkańca, gdyby miała być zakładką, wchodzi za którąś z siedmiu — a nie obok |
| FF-8 ★ | **Budżet §7.5 czeka na pomiar przy pełnej skali.** Przy trzydziestu pięciu kampaniach udział mediów w dobie `m7miasto` mieści się w szumie: ścięcie 70 % ich pracy (zawężenie ulotek do dzielnicy) zmieniło dobę z 14,83 s na 15,29 s. Kryterium „≤ 1,5 % budżetu ticku przy **2000** aktywnych kampanii" wymaga jednak przebiegu z profilem, bo dwa tysiące to pięćdziesiąt razy więcej | Adres jest naturalny: M10f i tak stawia przebieg balansatora dla całej fazy. Znane wejście: kanał ulotkowy był jedynym, który skalował się z **liczbą mieszkańców razy liczba kampanii**, i został zawężony (`F-27` w M10b). Pułapka do uniknięcia przy mierzeniu: doba `m7miasto` kosztuje 1,15 s w piątej dobie i ~15 s w czterdziestej **bez udziału mediów** — porównanie dwóch różnych dób mierzy wzrost gospodarki, nie zmianę kodu |

---

## Zmiany wpisane po M10c

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu
M10c. Gwiazdka = zmiana zakresu albo kryterium. Szczegóły — tabela `FD-n`
w `M10c-rd-i-nowe-produkty.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| FF-9 ★ | **Panel R&D ma mechanizm, ale nie ma wejścia.** `PanelId::Rnd` istnieje i jest zarezerwowany; drzewo, projekty, patenty i licencje są w `Firms::rnd()` i `RndData`. Brakuje `PanelDesc`, komendy gracza „badaj X” i karty technologii | Do M10c projekt wybiera reguła „najtańszy osiągalny węzeł”, a licencję firma kupuje sama, kiedy cudzy patent blokuje najtańszy węzeł. Gracz nie ma żadnego wpływu na własne badania — to jest ta sama luka, którą M10b zostawiło przy kampaniach reklamowych, i domyka się tą samą drogą (`DK-3`) |
| FF-10 ★ | **Technologia renderuje się w karcie inspekcji jako `#N`.** `reason::describe` dostaje katalog tekstów i `Locale`, a drzewo mieszka w `sim/firms` i `data/tech/` — ta sama granica, którą `GoodId` ma od M6 | Nazwę podmienia panel, który drzewo trzyma. Dopóki panelu nie ma, cztery powody R&D mówią graczowi numer węzła, a nie jego nazwę |
| FF-11 ★ | **Gęstość badaczy i płynność firm nie są zmierzone przebiegiem balansatora, a to od nich zależy, czy R&D w ogóle cokolwiek odkrywa.** Przebieg `m7miasto` (4 km, 23 183 mieszkańców po 300 dobach, 234 firmy) daje **3 badaczy na etatach, 7 projektów w toku, średni opłacony budżet badań 428 ‰, 0 odkryć, 0 patentów** | Dwie przyczyny naraz i obie są liczbami do strojenia, nie usterkami. **Pierwsza:** rola `researcher` jest w katalogu i 53 typy zakładów mają jej stanowisko, ale rynek pracy obsadza je ostatnie — przy 11 834 wakatach w mieście. **Druga, ważniejsza:** firmy opłacają mniej niż połowę budżetu materiałowego, bo przez 300 dób pieniądz przechodzi z ich ksiąg do gospodarstw (398 → 221 mln zł wobec 3 → 179 mln zł), a tempo badań spada proporcjonalnie do opłaconej części. Do tego miasto z 1990 zastaje jedenaście z dwunastu węzłów jako wiedzę powszechną, więc pierwszy patent wymaga przejścia całego łańcucha elektronicznego. Czy to jest właściwy rozkład, rozstrzyga pomiar, a nie przegląd |
| FF-12 | **Trzy efekty technologii z planu §5.4 nie powstały i mają tu adres**: `NewRecipe` (wymaga archetypu budynku dla montowni), `ProductFeature` razem z `FeatureId` (wymaga karty towaru) i własna montownia telefonów. Szczegóły — `FD-1`, `FD-3`, `FD-5` w `M10c-rd-i-nowe-produkty.md` | Każdy z nich potrzebuje czytelnika, którego dziś nie ma: linii produkcyjnej, karty towaru albo archetypu w `data/buildings/`. Wariant bez wyzwalacza przechodzi każdy test i wygląda tak samo jak działający (`K-67`) |
| FF-13 | **Blok `DecisionReason` M10: zajęte 800–807.** Wolne: **808–899** | M10c dołożył `ResearchStarted`, `TechDiscovered`, `LicenseSigned` i `ProductLaunched` |

---

## Zmiany wpisane po M10d

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu
M10d. Gwiazdka = zmiana zakresu albo kryterium. Szczegóły — tabela `GD-n`
w `M10d-gielda-przejecia-ubezpieczenia.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| FF-14 ★ | **Panel giełdy ma mechanizm, ale nie ma wejścia — trzeci raz ta sama luka.** `PanelId::Stock` istnieje i jest zarezerwowany; notowania, arkusz zleceń, akcjonariat z progami i kalendarz publikacji są w `Equity`. Brakuje `PanelDesc`, komendy „złóż zlecenie" i karty spółki | Ta sama luka, którą M10b zostawił przy kampaniach i M10c przy badaniach, i ta sama droga wyjścia (`DK-3`). Kanał po stronie symulacji jest otwarty: `Equity::place_order` przyjmuje `Owner::Player`, a `pay::player_citizen` znajduje jego gospodarstwo, więc komenda gracza nie wymaga ani jednej zmiany w `sim/economy` |
| FF-15 ★ | **Polisa i notowanie nie mają karty inspekcji.** `Subject` nie dostał w M10d ani jednego wariantu: spółka ma kartę firmy, polisa **nie ma żadnej**, a odszkodowanie widać wyłącznie jako powód w dzienniku firmy | Wariant `Subject` bez ramienia w `game::inspect::card` łamie kompilację (`K-69`), a ramię bez panelu pokazuje pustą stronę. Adres jest tutaj razem z panelem: `Subject::Cover(CoverId)` dokłada się wtedy, kiedy jest co na tej karcie pokazać |
| FF-16 ★ | **Kurs w karcie inspekcji jest liczbą za 0,01 % udziału i to wymaga tekstu, a nie liczby.** Powody `StockListed` i `StockFixing` pokazują obie liczby naraz (cenę bp i wycenę firmy), bo sama cena bp jest dla gracza nieczytelna | Alternatywą było liczenie kursu w „akcjach" o umownej wielkości — czyli drugi przelicznik obok `Firm.owners` i dokładnie ta druga prawda, którą `GD-1` usunął. Tekst jest tańszy od przelicznika |
| FF-17 | **Ubezpieczyciel powstaje jak tytuł medialny: przez `data/site_types/finance.ron` i archetyp `insurance_office` w `data/buildings/commerce.ron`.** Rejestr znajduje go po **kluczu tekstowym**, nie po `SiteTypeId` | Ta sama droga, którą M10b postawił gazety (`KLUCZE_TYTULOW`), i ten sam powód: `SiteTypeId` jest indeksem w katalogu i zmienia się razem z nim. Do zmierzenia w przebiegu balansatora: **ile oddziałów ubezpieczeniowych stawia generator** — waga archetypu to 14 wobec 30 dla banku, a rejestr bez ani jednego ubezpieczyciela znaczy miasto, w którym nikt nie wystawia polis |
| FF-18 ★ | **Trzy liczby M10d nie są zmierzone przebiegiem i to od nich zależy, czy giełda w ogóle istnieje w mieście z generatora**: ile firm spełnia warunek debiutu (dodatni **opublikowany** wynik i kurs każący rosnąć), ile gospodarstw przekracza próg majątku inwestora (50 000 zł) i czy w arkuszu w ogóle powstaje przecięcie | Wszystkie trzy mają ten sam kształt co `FF-11` przy badaniach: mechanizm jest, kryterium ma test jednostkowy, a to, czy zachodzi w mieście, rozstrzyga pomiar. Znane wejście: publikacja wyniku miesiąca `m` wypada w dobie `(m + 1) × 30 + 45`, więc **pierwszy debiut nie może zajść przed dobą 255** (sześć miesięcy życia firmy plus opóźnienie publikacji) — przebieg krótszy niż rok gry pokaże zero notowań i będzie miał rację |
| FF-21 ★ | **Hazard powodzi jest zgadnięty i nie został zweryfikowany przebiegiem.** `natural/flood` dostał w M10d `base_ppm: 300` z rachunku „osiem dzielnic razy 270 dób sezonu daje 0,65 powodzi na rok gry”, ale trzysta dób przebiegu `m7miasto` nie pokazało ani jednej. Pożar magazynu (`base_ppm: 10`, zakres zakładowy) też nie zaszedł ani razu | Liczba, której nikt nie zmierzył, ma tę samą szansę być błędna co każda inna zgadnięta (`R11` fazy). Adres jest naturalny: M10f i tak stawia przebieg balansatora dla całej fazy, a właściwym pomiarem jest **liczba szkód na rok gry uśredniona po ziarnach**, nie pojedynczy przebieg. Pułapka do uniknięcia: `cooldown_days` jest przerwą **definicji**, nie podmiotu (`CH-11`), więc jeden pożar w mieście ucisza wszystkie zakłady na sto osiemdziesiąt dób — przy zakresie zakładowym to zmienia rząd wielkości oczekiwanej liczby zdarzeń |
| FF-19 | **Blok `DecisionReason` M10: zajęte 800–816. Wolne: 817–899.** `StreamId` M10: zajęte 280–287 i 292–295, wolne 288–291 i 296–299 | M10d dołożył dziewięć powodów i dwa strumienie |
| FF-20 | **Kronika ma po M10d cztery nowe klasy wpisów bez własnego dziennika**: debiut, przejęcie, szkoda i wypłata odszkodowania. Wszystkie zapisują się w dzienniku decyzji firmy (`Firm.log`, pierścień 32) i tą drogą trafią do `Chronicle::harvest` (`DK-1`) | Pułapka rozbrojona w M10d, ale warto o niej wiedzieć: do dziennika idzie **wyłącznie sesja, która ruszyła kurs**. Wpis co dobę wyparłby z pierścienia wszystko inne w miesiąc, bo notowana spółka ma fixing codziennie — a dziennik decyzji ma pokazywać decyzje, nie stan rynku. Kurs, także niezmieniony, czyta się z `Equity::listing` |

---

## Zmiany wpisane po M10e

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu
M10e. Gwiazdka = zmiana zakresu albo kryterium. Szczegóły — tabela `GE-n`
w `M10e-relacje-i-zwiazki.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| FF-22 ★ | **Związek, zmowa i relacja z dostawcą nie mają karty inspekcji — czwarty raz ta sama luka.** `Subject` nie dostał w M10e ani jednego wariantu: spór widać wyłącznie jako powody w dzienniku decyzji firmy (`UnionFormed`, `WageDemandMade`, `StrikeStarted`, `StrikeEnded`), a zmowę dopiero po wykryciu | Ta sama droga wyjścia co przy polisie (`FF-15`): wariant `Subject` bez ramienia w `game::inspect::card` łamie kompilację (`K-69`), a ramię bez panelu pokazuje pustą stronę. Dane są gotowe i nie wymagają nowego przebiegu: `Unions::iter` niesie gęstość, wojowniczość, stan sporu i fundusz, `Cartels::iter` — skład, cenę minimalną i odchylenie od benchmarku, a `B2b::relations` zaufanie i obrót każdej pary. **Karta zakładu jest naturalnym miejscem dla związku** (tak jak tytuł medialny nie potrzebuje własnego wariantu, `FF-4`), a zmowa jest sekcją karty firmy |
| FF-23 ★ | **Gracz nie ma jak negocjować — piąty raz ta sama luka.** Firma gracza dostaje żądanie i odpowiada na nie **regułą**, tą samą co firma AI: ustępstwo jest funkcją marży i numeru rundy. Komendy „przyjmij żądanie", „kontruj", „przeczekaj" nie ma | Kanał po stronie symulacji jest otwarty i nie wymaga zmian: ustępstwo przechodzi przez `talks::rundy`, a tam wystarczy, żeby gracz mógł podstawić własną liczbę zamiast wyliczonej — dokładnie tak, jak `PricePolicy` z M5c podstawia politykę gracza w miejsce polityki AI. Panel pracowniczy z §6 pkt 5 dokumentu fazy (`grievance` per zakład jako ostrzeżenie wyprzedzające, przebieg negocjacji, licznik funduszu) jest tu jedynym brakującym elementem |
| FF-24 ★ | **Trzy liczby M10e nie są zmierzone przebiegiem balansatora**: ile zakładów w mieście z generatora przekracza próg żalu, ile zmów zawiązuje się na rok i jaki odsetek firm bywa rocznie w strajku (ryzyko `R6` fazy stawia próg alarmowy **5 %**) | Ten sam kształt co `FF-11` przy badaniach i `FF-18` przy giełdzie: mechanizm jest, kryteria mają testy, a to, czy zachodzi w mieście z generatora, rozstrzyga pomiar. Znane wejścia: żal mierzy się **miesięcznie**, a warunek „≥ 60 dób" znaczy dwa pomiary z rzędu, więc przed dobą 60 nie powstanie ani jeden związek; zmowa wymaga **trzech** sprzedawców tego samego towaru w jednej dzielnicy, więc małe miasto może nie mieć ani jednej pary spełniającej warunek. Ryzyko `R8` fazy (kartel zawsze albo nigdy opłacalny) ma w tym samym przebiegu swój cel: **20–50 % zmów wykrytych w ciągu pięciu lat gry** |
| FF-25 ★ | **Uderzenie prasy w markę ma stałą w kodzie, nie w danych.** `SCANDAL_MAX_DROP` w `sim/media` mówi, o ile najwyżej spada sympatia po najgorszym możliwym tekście; zmowa cenowa zabiera dwadzieścia punktów i ma swoją liczbę w `data/tuning/relations.ron` | `ponytail:` z nazwanym sufitem i drogą wyjścia (`GE-10`). Liczba w danych bez przebiegu, który ją stroi, byłaby parametrem bez pytania, na które odpowiada — a przebieg mierzący, ile marek rocznie obrywa od prasy, i tak należy do M10f razem z `FF-24` |
| FF-26 | **Przymusowy podział nadal wykonuje się zamknięciem zakładu** (`sim/city::law`, `ponytail:` z M8d) — mimo że rynek kontroli nad firmą istnieje od M10d | Komentarz w kodzie mówi „należy do M10", a M10d zbudował przejęcia i emisje. Zmiana jest teraz wykonalna: `ForcedDivestiture` mógłby wystawić pakiet kontrolny na fixing zamiast zamykać sklep. M10e tego **nie robi**, bo dotyczy innego urzędu i innego kryterium — ale adres przestał być pusty i to jest cała treść tego wpisu |
| FF-27 | **Blok `DecisionReason` M10: zajęte 800–824. Wolne: 825–899.** `StreamId` M10: zajęte 280–287 i 289–295, wolne **288** (zarezerwowany `PerilDraw`, `K-85`) i 296–299 | M10e dołożył osiem powodów (`TrustedSupplier`, `CartelFormed`, `CartelDetected`, `BrandScandal`, `UnionFormed`, `WageDemandMade`, `StrikeStarted`, `StrikeEnded`) i trzy strumienie (`CartelDetection`, `UnionFormation`, `StrikeResolve`) |
| FF-28 | **Strajk jest pierwszym zdarzeniem w katalogu, którego nie losuje hazard** (`K-89`), i lista takich definicji jest zamknięta: `magnat_events::CALLED_EVENTS` | Test katalogu pyta o tę listę, więc `base_ppm: 0` wpisane przez pomyłkę nadal łamie build. Faza dopisująca zdarzenie wywoływane przez świat dopisuje je **i tam** — inaczej wygląda w katalogu jak definicja martwa (`R2`) |
| FF-29 ★ | **Lista płac nadal nie dochodzi do gospodarstw, a strajk pokazał, ile to kosztuje.** `Firms::run_payroll` zwraca fakty, `PayrollOutbox` je przyjmuje i **nikt jej nie opróżnia** od M7b; dochód gospodarstwa jest do dziś egzogeniczny (decyzja nr 2 fazy M5: płaci go abstrakcyjny pracodawca spoza miasta). Skutek dla M10e jest konkretny: strajk zabiera realny pieniądz **firmie** — jej rachunek wyniku nie księguje płac za dni postoju — a po stronie załogi zostaje **liczbą**, bo salda gospodarstw nie drgną, choć ludzie nie dostali wypłaty | To nie jest usterka M10e i M10e jej nie naprawia: konsument `PayrollOutbox` zmienia źródło dochodu całego miasta, więc przestawia kalibrację kopert, kredytu, CPI i bramek G1–G3 naraz. Adres jest tu, bo to M10e jest pierwszą mechaniką, dla której **różnica jest widoczna**: `Union::strike_fund` jest dziś jawnym przybliżeniem wytrzymałości i dopiero po domknięciu tego kanału stanie się odczytem prawdziwych sald. Pułapka do uniknięcia przy naprawie: `pay_incomes` wypłaca **netto po potrąceniu u źródła** i robi to raz w miesiącu dla wszystkich; lista płac ma dzień wypłaty **per firma** (`payday(FirmKey)`), więc podmiana źródła rozkłada dochód miasta na trzydzieści dób i to ona, a nie kwota, jest tu prawdziwą zmianą |
