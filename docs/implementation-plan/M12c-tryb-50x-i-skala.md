# M12c — Tryb 50× i skala 400 tys.

Podfaza 3 z 6 fazy **M12 — Skala i jakość** (`M12-skala-i-jakosc.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M12a (pamięć), M12b (zapis), M10a (`sim/macro`), M4, M11 (przełączniki degradacji). |
| **Pakiety robocze** | WP9, WP10 |
| **Projekt techniczny** | §5.6 |
| **Wynik do pokazania** | Doba gry ≤ 3 s dla miasta 150 tys.; metropolia 400 tys. ładuje się i chodzi w ≤ 6 GB. |
| **Kryterium zamknięcia** | Kryteria WP9 i WP10. **Decyzje właściciela produktu** (`00-postep.md`): co poświęcamy w trybie 50× oraz akceptacja osłabienia kontraktu LOD dla makro (`K-5`) — obie przed startem WP9. |
| **Poprzednia / następna** | `M12b-zapis-i-replay.md` · `M12d-modding.md` |

Budżet 2,08 ms/tick rozpisany na systemy, przełączanie ruchu i agentów w makro, `SpeedGovernor`, profilowanie i domknięcie budżetu dla metropolii.

---

## Pakiety robocze

### WP9 — Tryb 50×: budżet czasu ticka [L]
Zależności: M10 (`sim/macro`), M4 (`sim/traffic` — kalibracja tablic czasów przejazdu), M11 (przełączniki degradacji renderu).

**To jest pakiet budżetowy, nie pakiet przełącznikowy.** Sam suwak prędkości to godzina pracy; L wynika z tego, że M4 zmierzył 11 ms/tick dla ruchu mezo (15,8 s/dobę — 5× ponad cel §20.2), więc mezo nie wystarcza i cały budżet trzeba rozpisać na systemy i wyegzekwować (§5.6). Zakres: rozpisanie budżetu 2,08 ms/tick, przełączanie ruchu i agentów w makro na progu 50×, degradacja grafiki, `SpeedGovernor`, dowód spójności LOD.

**Kryteria ukończenia (trzy niezależne):**
1. **Budżet:** 1 doba gry ≤ 3 s dla 150 tys. na maszynie referencyjnej CI (D10), z rozbiciem per system mieszczącym się w tabeli §5.6. Każdy system ma własną bramkę `criterion` — przekroczenie budżetu przez jeden system to czerwone CI, nawet gdy suma jeszcze się mieści.
2. **Zachowanie:** 30 dni w 1× vs 50× — suma pieniądza **co do grosza**, suma masy każdego towaru **co do grama** (dok. 00 §4 po rozdzieleniu tolerancji).
3. **Brak dryfu:** agregaty **komórkowe i wyżej** (ceny koszyka, bezrobocie, produkcja per branża) odchylone ≤ 0,5% miesięcznie; nachylenie regresji różnicy w oknie 12 miesięcy nieodróżnialne od zera **i** autokorelacja znaku różnicy (lag 1) ≤ 0,3. Odchylenie ograniczone jest tolerowalne, dryf **nie** — bo się kumuluje przez 100 lat. Wielkości o małym n (pojedyncza firma, cienki rynek, pojedyncza marka) są **poza** tym kryterium — D14.

### WP10 — Skala 400 tys.: profilowanie i domknięcie budżetu [XL]
Zależności: WP2, WP3, WP5, WP9.

Iteracyjna pętla: profil (`tracy`/`puffin`) → najgrubsza pozycja → optymalizacja → pomiar. Realizacja drabiny cięć (§5.2) w zakresie potrzebnym do zmieszczenia się w 6 GB.

**Kryterium ukończenia:** miasto 400 tys. działa przez 5 lat gry ≤ 6 GB RSS bez trendu wzrostowego; scenariusz `metropolis_400k.ron` w repo jako punkt odniesienia dla wszystkich późniejszych benchmarków.

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.6 Tryb 50×: budżet czasu ticka rozpisany na systemy

**Budżet.** Doba = 1440 ticków ekonomicznych. Cel §20.2: 1 doba ≤ 3 s dla 150 tys. → **480 ticków/s → 2,08 ms/tick na wszystko**.

**Punkt wyjścia: mezo nie wystarcza.** M4 zmierzył ruch mezo na **11 ms/tick**, czyli 15,8 s na dobę gry — sam ruch przekracza cały budżet 5×. To nie jest problem do rozwiązania optymalizacją stałego czynnika; to problem jednostki pracy. Przy 150 tys. mieszkańców każdy system, który przy 50× **iteruje per agent lub per pojazd**, kosztuje 1,5 ms nawet przy nierealistycznych 10 ns na encję — czyli 72% budżetu na jeden system. Wniosek jest jednoznaczny: **przy 50× jednostką pracy musi przestać być encja.**

Jednostki pracy przy 50× dla miasta 150 tys.:

| Jednostka | Liczność (150 tys.) | Zastępuje |
|---|---:|---|
| `MacroCell` (dzielnica × klasa) | **180** (30 dzielnic × 6 klas); zakres **30–240** w całym §4.1 | 150 000 mieszkańców |
| Podróż rozpoczęta w tym ticku | ~420 | ~60 000 pojazdów w ruchu |
| Zakład | ~11 000, ale na ticku `EveryHour` → ~180/tick amortyzowanych | ~120 000 maszyn |
| Para (kategoria × dzielnica) na rynku | ~4 000 | ~200 000 ofert |

**Ziarno agregacji jest podporządkowane tolerancji, nie budżetowi czasu — i nie jest stałą.** Droga do tej liczby jest pouczająca, więc zostaje udokumentowana. Pierwsza wersja tego dokumentu zakładała ~2 400 kohort (200 dzielnic × 12 klas) — obie liczby zgadywane. Było to błędne o dwa rzędy: przy 2 400 komórkach na komórkę przypada 62 osoby, a wariancja rozkładu wielomianowego przy n = 62 daje odchylenie kilku procent, więc kontrakt tolerancji 0,5% pękał ze statystyki, niezależnie od jakości implementacji.

M10 podał 240 (40 × 6) i miał rację dla metropolii. Po przyłożeniu tej samej reguły do **wszystkich** rozmiarów miast z PRD §4.1 okazało się jednak, że stałe ziarno `dzielnica × 6 klas` przechodzi na dużym mieście i **cicho pada na małym**:

| Rozmiar | Populacja | Dzielnic | Komórek (×6) | Osób/komórkę | Tolerancja 0,5%? |
|---|---:|---:|---:|---:|---|
| małe | 20 000 | 10 | 60 | **333** | **nie** |
| małe (górny) | 40 000 | 15 | 90 | **444** | **nie** |
| średnie | 60 000 | 20 | 120 | 500 | granica |
| duże | 150 000 | 30 | 180 | 833 | tak |
| metropolia | 400 000 | 40 | 240 | 1 667 | tak |

Kontrakt łamał się więc dokładnie tam, gdzie nikt nie patrzył — u gracza, który wybrał małe miasto. Rozwiązanie (M10): ziarno klas jest funkcją rozmiaru, z twardym progiem `MIN_CELL_POP = 500`:

```rust
pub const MIN_CELL_POP: u32 = 500;
pub fn cell_grain(pop: u32, districts: u16) -> ClassGrain;   // Classes6 | Classes3 | Classes2
```

Klasy łączą się parami po **sąsiadujących** przedziałach statusu, nigdy losowo (`Classes3` = niższa+robotnicza, niższa średnia+wyższa średnia, wyższa+elita). Dla 20 tys.: 10 × 3 = 30 komórek, 666 osób/komórkę — kontrakt wraca do zakresu. Odwzorowanie klasy na komórkę jest funkcją czystą, więc `lift()` i `lower_cell()` nie zmieniają się wcale.

**Konsekwencja dla M12, i jest to jedyna rzecz, o którą M10 poprosił:** liczba komórek **nigdzie nie może być stałą**. Wszystkie budżety, liczniki i asercje odwołują się do `state.cells.len()`, nie do 180 ani 240. Pułapka jest cicha — stała 240 działałaby poprawnie na wszystkich scenariuszach testowych M12 (które są duże) i złamałaby się dopiero przy pierwszym profilu na małym mieście. Pilnuje tego test `no_hardcoded_cell_count` (§7.3).

Spadek ze 150 000 do 180 jednostek pracy (833×) jest tym, co czyni cel osiągalnym z zapasem. Ruch: czas przejazdu z tablic dzielnica×dzielnica×godzina (PRD §17.6) — **wzór i kalibracja należą do M4 (`sim/traffic::travel_time`)**, a `sim/macro` trzyma tylko ich snapshot (`MacroState.commute: CommuteMatrix`) z chwili przejścia w makro. Nie ma kolejek na krawędziach, car-following ani pojazdów.

**Budżet per system (150 tys., prędkość 50×, maszyna referencyjna D10):**

| System | Budżet | Jednostka pracy przy 50× | Uzasadnienie liczby |
|---|---:|---|---|
| Ruch makro | 0,55 ms | ~420 podróży/tick | Lookup w `CommuteMatrix` + losowanie z rozkładu ≈ 1 µs/podróż → 0,42 ms, zapas 30%. Budżet uzgodniony z M10 |
| Agenci (DES makro) | 0,15 ms | **`cells.len()` = 180** | Zdarzenia per komórka, nie per agent; ~830 ns/komórka |
| Ekonomia (rynki, transakcje, ceny) | 0,45 ms | ~4 000 par kategoria × dzielnica | Transakcje agregowane per komórka × kategoria |
| Firmy i produkcja | 0,25 ms | ~180 zakładów/tick | Produkcja per zakład (§17.4), na ticku godzinowym |
| Miasto, zdarzenia, kroniki | 0,10 ms | dzielnice, zdarzenia aktywne | Częstotliwości `EveryDay`/`EveryMonth` |
| Scheduler, bufory komend, punkty synchronizacji | 0,10 ms | ~180 wpisów komend | Sortowanie po `(SystemId, entity_index)` |
| **Rezerwa** | **0,48 ms** | — | 23% budżetu |
| **RAZEM** | **2,08 ms** | | **= 3,0 s/dobę** |

Budżet jest liczony dla **180 komórek** (150 tys. — rozmiar, dla którego PRD §20.2 definiuje cel). Na metropolii komórek jest 240, więc wiersz „agenci" rośnie o 33% (0,15 → 0,20 ms); mieści się w rezerwie i nie dotyczy celu 3 s/dobę, który jest zdefiniowany dla 150 tys. Na małym mieście komórek jest 30 i koszt spada sześciokrotnie — skalowanie idzie **wyłącznie w dół**, nigdy w górę.

Rezerwa urosła z 0,08 do 0,48 ms wyłącznie dzięki korekcie ziarna. To nie jest zysk z optymalizacji — to zysk z tego, że poprzednia liczba była zgadnięta źle. Traktujemy ją jako margines na to, że pozostałe wiersze też są zgadnięte.

**Egzekwowanie.** Każdy wiersz tej tabeli dostaje własny benchmark `criterion` i własną bramkę CI. Przekroczenie budżetu przez pojedynczy system jest czerwone **nawet wtedy, gdy suma jeszcze się mieści** — inaczej pierwszy system, który przekroczy, zje rezerwę kolejnych i winowajcy nie da się wskazać. Budżety są własnością M12, ale realizacja każdego wiersza jest własnością fazy, która ten system napisała; M12 mierzy, raportuje i eskaluje.

**Przełączniki na progach prędkości:**

| Prędkość | Ruch | Agenci | Render |
|---|---|---|---|
| 1× / 3× | mikro w kadrze, mezo poza | DES pełny | pełny |
| 10× | mikro tylko dla śledzonych, reszta mezo | DES pełny | bez wnętrz, LOD +1 |
| **50×** | **makro wszędzie — tablice czasów przejazdu, zero pojazdów** | **makro: agregaty per dzielnica × klasa, tożsamości zachowane (stan uśpiony)** | **bez animacji ludzi, LOD +2, 1 kaskada cienia, bez cząstek, nakładki odświeżane co 1 s realnego** |

Przejście makro→mezo odtwarza indywidualne stany z zachowanych tożsamości, deterministycznie z seeda i tożsamości (PRD §17.4). Kontrakt uzgodniony z M10 (D11 zamknięte):

```rust
pub fn lower_cell(cell: &MacroCell, seed: u64, tick: Tick) -> CellExpansion;
```

Funkcja czysta: bez dostępu do `World`, bez stanu ukrytego. `MacroCell` niesie `Vec<CitizenSeed>` — nośnik tożsamości, bez którego „deterministycznie z seeda i tożsamości" ma tylko jeden człon. `lower()` jest wyłącznie sterownikiem wołającym ją per komórka. M12 nalegał na czystość tej sygnatury, bo to jedyny punkt, w którym replay i zapis mogą się rozjechać niezauważalnie — po obu stronach test jest osobny (K4 u M10, `replay_corpus` u M12).

**Kontrakt spójności po rozdzieleniu tolerancji (dok. 00 §4).** Tolerancja nie jest już jedna — i nie jest też jedna w obrębie makro:

| Przejście / wielkość | Tolerancja |
|---|---|
| mikro ↔ mezo | **0** — bit w bit, bez wyjątków |
| Suma pieniądza w systemie (każdy LOD) | **0 — co do grosza** |
| Suma masy każdego towaru (każdy LOD) | **0 — co do grama** |
| Agregat komórkowy i wyżej (`MIN_CELL_POP` ≥ 500 osób lub ≥ 10 firm) | ≤ 0,5% miesięcznie, **bez dryfu** |
| Przychód pojedynczej firmy | **3–12%** — poza kontraktem 0,5% |
| Cena towaru przy < 3 dostawcach | **2–8%** — poza kontraktem 0,5% |
| Udział rynkowy pojedynczej marki | **2–5%** — poza kontraktem 0,5% |

Trzy ostatnie wiersze to ustalenie M10 i **nie są usterką do naprawienia**: to wariancja rozkładu wielomianowego przy małym n (przy 200 klientach odchylenie udziału to ~3,5%), której żaden wspólny kernel ekonomiczny nie zdejmie. Konsekwencja dla M12 jest konkretna i wchodzi do obietnicy produktu: **tryb 50× przewiduje stan dzielnicy i miasta, nie los pojedynczej firmy.** Nigdzie w M12 nie wolno oprzeć się na założeniu, że po przebiegu w 50× konkretna firma jest w konkretnym stanie z dokładnością 0,5% — dotyczy to w szczególności „co jeśli" AI, autozapisu przed decyzją i wszelkich porównań przebiegów. Zapisane jako D14.

Rozróżnienie dokładności od zachowania nie jest złagodzeniem wymagania, tylko jego doprecyzowaniem, i M10 sformułował je czyściej, niż robił to ten dokument: **dokładność mówi, komu przypadł pieniądz; zachowanie mówi, ile go jest.** Makro zgaduje pierwsze, nie zgaduje drugiego — każdy przepływ idzie przez `ledger_post()` (zapis dwustronny, ten sam co w mezo), a podział agregatu domyka się korektą reszty do pierwszego wg posortowanego klucza (dok. 00 §2). To, a nie równość sald, jest prawdziwym kontraktem.

**Dryf jest groźniejszy od odchylenia.** 0,4% w losową stronę co miesiąc jest nieszkodliwe; 0,4% w *tę samą* stronę to po 100 latach czynnik ~120× i świat, który się rozpadł. Dlatego sam próg na nachylenie regresji nie wystarcza — da się go przypadkiem przejść na krótkiej próbce. Za M10 dokładamy **kryterium autokorelacji znaku różnicy (lag 1 ≤ 0,3)** do testu 100-letniego (§7.4).

**`SpeedGovernor`** (poza tickiem symulacji, w pętli gry): mierzy czas ticka w oknie kroczącym 30 ticków. Jeśli p50 przekroczy budżet prędkości, **automatycznie obniża prędkość o szczebel** i wyświetla komunikat („Miasto jest za duże na 50× — 10×"). Spójny wolniejszy bieg jest lepszy od szarpanego szybszego. Governor jest też mechanizmem degradacji przy wysyceniu areny cieni (§5.5). Wyłączalny w headless (tam liczymy czas, nie płynność).

Governor **nie decyduje sam o przejściu LOD** — pyta `MacroLodPolicy` (M10):

```rust
pub fn target_lod(&self, speed: GameSpeed, load: &LoadStats) -> Lod;
pub fn may_transition(&self, from: Lod, to: Lod, world: &World) -> Option<BlockReason>;
```

`may_transition` jest wiążące: wejście w makro musi być zablokowane w trakcie fixingu giełdowego, rundy negocjacji związkowych i trwającego strajku — te stany zginęłyby przy agregacji. Governor pokazuje wtedy graczowi powód („50× dostępne po zakończeniu fixingu"), zamiast cicho zignorować suwak. **Histereza progów jest po stronie M10**, żeby governor nie oscylował — M12 nie implementuje własnej.

**Jeśli budżet nie domknie się mimo makro** — drabina cięć celu 3 s/dobę (szczegóły: D12):

1. Ekonomia na ticku godzinowym zamiast minutowego w trybie makro. Zysk ~0,35 ms. Koszt: ceny reagują w oknie godziny; §20.1 wymaga reakcji na szok w 2–7 **dni**, więc mieści się.
2. Cel rozluźniony do **≤ 4 s/dobę** — zmiana §20.2, decyzja projektanta. 1 rok gry w 50× to wtedy 24 min zamiast 18.
3. Cel definiowany dla 100 tys., nie 150 tys. — najgorsza opcja, bo zmienia obietnicę PRD w miejscu, w którym jest ona mierzalna i konkretna.

**Ziarno jako dźwignia — ograniczona i asymetryczna.** Szczebel „kohorty grubsze", obecny w pierwszej wersji tej drabiny jako 12 → 6 klas, nie zniknął, ale skurczył się i zmienił charakter. Przy `MIN_CELL_POP = 500` miasto 150 tys. ma zapas: `Classes3` daje 90 komórek po 1 666 osób, więc wciąż spełnia tolerancję. Zysk jest jednak **marginalny (~0,08 ms)**, bo ziarno klas wpływa tylko na wiersz „agenci" (0,15 ms) — ekonomia liczy się per kategoria × dzielnica i jest na nie obojętna. To dźwignia warta odnotowania, nie warta planowania.

**W drugą stronę drogi nie ma wcale.** Zagęszczanie komórek w celu poprawy rozdzielczości łamie tolerancję, i to najpierw na małych miastach — tam zapasu nad progiem 500 nie ma żadnego. Ziarno jest ograniczone z dołu przez statystykę (`MIN_CELL_POP`) i z góry przez sens; między tymi granicami zapas zależy od rozmiaru miasta i na 20 tys. wynosi zero.

Realny margines: rezerwa 0,48 ms (23%) plus szczebel 1 — razem ~0,83 ms, czyli 40% budżetu.
