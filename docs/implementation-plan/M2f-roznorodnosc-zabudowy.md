# M2f — Różnorodność zabudowy

Podfaza 5 z 6 fazy **M2 — Miasto statyczne** (`M2-miasto-statyczne.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M2d: język gramatyki (`Rule`, walidator, `GrammarSet::load_dir`), silnik derywacji na drzewie zakresów, katalog 12 plików w `data/grammar/`, `EditQueue`, `Parcel.land_value_per_m2` (`pass_1`). M2c: `Parcel`, `Block.epoch_ring`, `District.{style, kind, income_tier}`. |
| **Pakiety robocze** | WP18, WP19, WP20 |
| **Projekt techniczny** | §5.6c |
| **Wynik do pokazania** | Przelot kamerą nad dzielnicą mieszkaniową, w której **żadne dwa sąsiadujące budynki nie są tym samym budynkiem**: różnią się typem, wysokością, materiałem, dachem albo detalem bryły. Macierz pokrycia (strefa × epoka × styl) bez ani jednej luki. |
| **Kryterium zamknięcia** | Kryteria WP18–WP20; udział gramatyki awaryjnej < 0,5 %, udział doboru z rozluźnionym filtrem (`relaxed`) < 5 %; test T13 zielony na 32 ziarnach × 4 profile. |
| **Stan** | **Podfaza zamknięta** — WP18, WP19 i WP20 (korekty H1–H15). |
| **Poprzednia / następna** | `M2d-zabudowa.md` · `M2e-gospodarka-bazowa-i-wycena.md` |

Domknięcie Etapu 6: jeden brakujący operator bryły (`Protrude`), lukarny w regule `Roof`,
katalog gramatyk rozszerzony do pokrycia macierzy bez luk i **mierzalna** definicja
różnorodności, żeby słowo „różnorodny" miało próg, a nie opinię.

---

## Dlaczego przed M2e, a nie po niej

Litera w nazwie zapisuje kolejność powstania dokumentu, nie kolejność wykonania.
M2f wykonuje się **między M2d a M2e**, a M2e zostaje ostatnią podfazą fazy i to ona
zamyka bramki 1–7. Powód jest jeden i jest twardy:

WP17 w M2e zamyka 12 testów spójności na 32 ziarnach × 4 profile, a WP15b liczy `pass_2`
wyceny. Obie rzeczy mierzą miasto, które zależy od katalogu gramatyk: T10 (bilans mieszkań
i stanowisk) jest wg korekty F6 **zadaniem kalibracyjnym**, a jedną z trzech jego dźwigni
jest `massing.max_depth_m` w gramatykach. Rozszerzenie katalogu po kalibracji znaczy
kalibrację drugi raz. Rozszerzenie przed nią nie kosztuje nic.

Przemianowania `M2e` → `M2g` nie robimy: nazwa pliku jest cytowana w dzienniku
`00-postep.md`, w tabelach korekt M2d i w §4 dokumentu fazy, a zysk byłby wyłącznie
estetyczny.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Opis | Kryterium ukończenia |
|---|---|---|---|---|
| ✅ WP18 | Operator `Protrude` i lukarny | M2d (WP11, WP12) | wariant `Rule::Protrude { face, m, rule }` — wysunięcie zakresu **poza jedną** ścianę; `dormers` w regule `Roof`; przycięcie wysunięcia do granicy parceli, z wyjątkiem wysięgu nad chodnikiem powyżej skrajni | balkon, wykusz i lukarna dają się zapisać w `.ron` bez ani jednej linii Rusta; **poniżej skrajni 3,5 m** żadne wysunięcie nie wychodzi poza wielokąt parceli, **powyżej** nie dalej niż 1,5 m; budżet `MAX_NODES` trzymany na kamienicy 6-kondygnacyjnej z balkonami na każdym piętrze (korekta H1) |
| ✅ WP19 | Katalog gramatyk: pokrycie i wariancja | WP18 | rozszerzenie `data/grammar/` z 12 do ~32 plików wg typologii z §5.6c; `Choice` wewnątrz gramatyk na materiał, rytm otworów i kształt dachu | macierz pokrycia (strefa × epoka × styl) **bez luk**: każda kombinacja występująca w mieście ma ≥ 1 gramatykę bez rozluźniania filtrów; fallback < 0,5 %; `relaxed` < 5 % |
| ✅ WP20 | Miara różnorodności + test T13 | WP19 | `BuildingSignature` (4 znaczniki), histogram gramatyk i kolizje sygnatur w `GenerationReport`; test T13 | T13 zielony: udział par identycznych sygnatur w promieniu 60 m < 15 %; entropia rozkładu gramatyk w dzielnicy mieszkaniowej o ≥ 100 budynkach ≥ 1,8 bita; oba progi mierzone na 32 ziarnach × 4 profile |

Ścieżka jest liniowa: WP18 → WP19 → WP20. WP20 wolno pisać równolegle z WP19 —
miara powstaje szybciej niż dane, które ma mierzyć, i lepiej, żeby powstała pierwsza.

---

## Projekt techniczny

### 5.6c Detal bryły, katalog i miara różnorodności

#### Dlaczego brakuje dokładnie jednego operatora

Pierwotny szkic §5.6 obiecywał blok `Details([Cornice, WindowBand, Balcony, Chimney,
Entrance])`. Po M2d widać, że cztery z pięciu **już się da** zapisać zestawem, który
jest w kodzie, a piąty nie da się wcale:

| Detal | Czym się zapisuje dziś | Werdykt |
|---|---|---|
| `WindowBand` | `Comp(Front) → Repeat(U) → Split(W) → Void` — dokładnie tak robi to `kamienica.ron` | jest |
| `Entrance` | `Void` w parterze + rekord `Entrance` liczony w `build::wejscia` | jest |
| `Cornice` | `Offset(m) → Extrude(m) → Fill` — poszerzenie w poziomie plus ustawienie wysokości | jest, ale patrz niżej o voxelu |
| `Chimney` | `Comp(Top) → Repeat(U) → Extrude → Fill` | jest |
| `Balcony`, wykusz, lukarna | **nie da się** — `Offset` poszerza zakres z **każdej** strony, a balkon wychodzi z jednej | brak |

Stąd zakres WP18 to jeden wariant enuma, nie blok pięciu reguł:

```rust
/// Wysunięcie zakresu poza **jedną** ścianę o `m`. Odwrotność `Comp`: `Comp` bierze płytę
/// przy ścianie od środka, `Protrude` dokłada bryłę na zewnątrz. Balkon, wykusz, ryzalit
/// i gzyms jednostronny to ten sam operator z innym `m` i innym wnętrzem.
Protrude { face: Face, m: f32, rule: Box<Rule> },
```

Balkon w gramatyce to wtedy dwie linie, bez nowego słowa kluczowego:

```ron
("pietro_z_balkonami", Seq([
    Fill("stucco"),
    Comp(thickness_m: 1.25, faces: [
        (Front, Repeat(axis: U, step_m: 4.6, pad_m: 1.3, rule: Seq([
            Split(axis: W, parts: [(Abs(0.9), Nothing), (Abs(1.4), Void), (Rel(1.0), Nothing)]),
            Protrude(face: Front, m: 1.2, rule: Extrude(m: 1.0, rule: Fill("concrete"))),
        ]))),
    ]),
])),
```

Lukarna nie jest operatorem, tylko parametrem dachu — `RoofShape::{Gable, Hip}` dostaje
`dormers: (u8, u8)` (widełki liczby, rozstaw wyliczany z długości kalenicy). Powód jest
ten sam, dla którego `Floors` jest regułą domenową, a nie `Split`: lukarna musi znać
połać, a połać powstaje wewnątrz reguły `Roof`.

#### Sufit, o który to wszystko uderza: voxel ma 1 m

Korekta E12 z M2d zmierzyła, że przy voxelu **1 m w poziomie** detal cieńszy niż ~2 m
rasteryzuje się na stopnie, które się mijają. Z tego wynika reguła, która rozstrzyga
połowę sporów o zakres i jest wpisana do §9.2 fazy jako decyzja 21:

> **Detal poniżej jednego voxela nie istnieje.** Gramatyka M2 wyraża detal od ~1 m w górę:
> balkon (1,0–1,4 m), wykusz (1,0–1,5 m), lukarna, komin, ryzalit, uskok bryły. Gzyms
> 0,4 m, opaska okienna, pilaster i boniowanie **nie są tu wyrażalne w ogóle** — nie
> dlatego, że brakuje operatora, tylko dlatego, że nie mają gdzie się zrasteryzować.
> To jest detal wizualny z §2 („nie wchodzi"), czyli M11: prefab przez `Place` albo mesh.

Ta granica jest zgodna z rozstrzygnięciem 9.1/10 („zbiór operatorów, bryła i otwory
zostają w M2, M11 dokłada warianty terminali") i dopiero ją uściśla: **bryła mierzona
w voxelach jest moja, detal mierzony pod voxelem jest M11**.

#### Przycięcie wysunięcia

`Protrude` jest jedynym operatorem, który wychodzi **poza** bryłę posadowioną w WP12,
więc jest jedynym, który może wyjść poza działkę. Obsługa jest twarda, nie miękka:

1. wysunięcie przycinane jest do wielokąta parceli pomniejszonego o `setback` z `massing`;
2. wysunięcie nad pasem drogowym jest dozwolone **wyłącznie** powyżej 3,5 m nad niweletą
   frontu (wykusz nad chodnikiem tak, balkon nad jezdnią na wysokości parteru nie);
3. wysunięcie, którego po przycięciu zostaje mniej niż 1 voxel, **nie jest kolejkowane** —
   nie ma półbalkonów grubości pół metra, bo one i tak by zniknęły w rasteryzacji;
4. każdy z trzech przypadków jest liczony w `GenerationReport`, nie pomijany po cichu.

Test T6 („budynek w parceli") rozszerza się o wysunięcia — dziś porównuje `Building.aabb`,
który po WP18 musi obejmować bryłę **z** detalem, inaczej selekcja do kadru w M11
utnie balkony przy krawędzi ekranu.

#### Typologia katalogu (WP19)

Dwanaście plików z M2d to jeden przedstawiciel na rodzaj zabudowy — wystarczyło, żeby
miasto stanęło, nie wystarcza, żeby wyglądało na miasto. Docelowo ~32 pliki. Rozkład
wynika z macierzy (strefa × epoka × styl), a nie z upodobań:

| Rodzaj | Warianty | Strefy | Epoki |
|---|---|---|---|
| Kamienica | czynszowa ceglana, secesyjna, modernistyczna międzywojenna, plombowa powojenna | R3, Commercial | medieval → interwar → postwar |
| Blok | płytowy klatkowy, punktowiec, galeriowiec, blok współczesny z parterem usługowym | R2, R3 | postwar, modern |
| Wieżowiec | biurowy z podcieniem, mieszkalny, z koroną schodkową | Commercial, R3 | modern |
| Dom jednorodzinny | kostka, willa międzywojenna, dom podmiejski z garażem, dworek | R1 | interwar → modern |
| **Bliźniak** | **dwa mieszkania, jedna bryła, lustrzany podział** | **R1, R2** | **interwar → modern** |
| Szeregowiec | wąski ceglany, szeregówka współczesna z garażem | R1, R2 | industrial → modern |
| Zagroda | z budynkiem gospodarczym, bez | Agriculture | wszystkie |
| Hala / magazyn | hala z suwnicą, hala lekka, magazyn wysokiego składowania, skład otwarty | IndustryHeavy, Logistics | industrial → modern |
| Pawilon / handel | pawilon wolnostojący, pasaż, market z parkingiem | Commercial, Logistics | postwar → modern |
| Gmach publiczny | szkoła, ratusz/urząd, budynek sakralny z wieżą | Institutional | wszystkie |

Bliźniak jest wytłuszczony, bo jest jedynym rodzajem, którego M2d nie miał **wcale**,
a nie tylko w jednym wariancie: działka o froncie 8–12 m w R1 dostawała albo dom
wolnostojący z zerowym odstępem bocznym, albo nic.

**Wariancja wewnątrz jednego pliku jest tańsza od nowego pliku i ma być wyczerpana
pierwsza.** `Choice` istnieje od M2d i jest dziś użyty w jednym miejscu w całym katalogu
(materiał ściany w `kamienica.ron`). Trzy kanały na gramatykę — materiał, rytm otworów,
kształt dachu — dają z trzech wariantów każdy 27 wyglądów z jednego pliku. Nowy plik
piszemy dopiero wtedy, gdy różni się **bryłą**, a nie ubraniem.

#### Miara różnorodności (WP20)

Bez miary „różnorodny" jest opinią, a kryterium ukończenia musi dać się obalić.
Sygnatura budynku to cztery znaczniki, które gracz rozróżnia z odległości ulicy:

```rust
/// Cztery znaczniki, po których dwa budynki tej samej gramatyki są albo nie są
/// tym samym budynkiem. Liczona w derywacji, trzymana w raporcie — nie w `Building`
/// (14,5 tys. budynków × 4 B to pamięć za coś, co jest miarą jakości, nie stanem gry).
pub struct BuildingSignature(u32);   // grammar_id | floors | wall_material | roof_shape
```

Test **T13** (dopisany do §7 fazy) ma dwa progi i oba biorą się z obejrzenia miasta,
nie z teorii:

- **Sąsiedztwo:** dla każdego budynku, wśród budynków w promieniu 60 m, udział tych
  o **identycznej** sygnaturze < 15 %. Promień 60 m to mniej więcej to, co widać
  z chodnika po obu stronach ulicy.
- **Dzielnica:** entropia Shannona rozkładu `GrammarId` w dzielnicy mieszkaniowej
  o ≥ 100 budynkach ≥ 1,8 bita (czyli efektywnie ≥ ~3,5 równoważnych gramatyk).
  Próg liczony **tylko** dla `DistrictKind` mieszkaniowych: osiedle płytowe ma prawo
  być monotonne, bo takie jest, a strefa przemysłowa ma trzy rodzaje hal i koniec.

`GenerationReport` dostaje: histogram `GrammarId`, histogram sygnatur, najgorszy promień
(ten, na którym udział identycznych jest największy) i jego współrzędne — żeby dało się
tam polecieć kamerą, zamiast czytać, że test upadł.

---

## Czego ta podfaza nie robi

- **Nie rusza doboru gramatyki.** `pick_grammar` z M2d (filtr `applies` → rozluźnianie
  etapami → losowanie ważone → `coverage`) zostaje. Jeśli T13 nie przechodzi, naprawia
  się katalog albo wagi w danych, nie algorytm — bo algorytm nie ma skąd wiedzieć,
  czego brakuje w katalogu.
- **Nie dodaje prefabrykatów.** `Instance`/`Place` wypadły z M2 korektą D5 i zostają
  u M11; decyzja 21 w §9.2 dociąga tę granicę do rozmiaru voxela.
- **Nie rusza wnętrz.** `Unit`/`Workplace` powstają z `interior:` i nowa gramatyka
  wypełnia ten sam blok. Zmiana liczby mieszkań idzie do kalibracji T10 w M2e — i to
  jest dokładnie powód, dla którego ta podfaza leży przed M2e.
- **Nie jest ostatnim słowem o katalogu.** Po M12d `data/grammar/` jest katalogiem
  moddowalnym (wpis G1 w `M12d-modding.md`), więc dalszy wzrost katalogu jest zawartością,
  nie fazą.

---

## Zmiany wpisane po M2d

Zgodnie z `K-18`. Gwiazdką oznaczone te, które zmieniają **zakres albo kryterium**.

| # | Zmiana | Dlaczego |
|---|---|---|
| G1 ★ | **Powstaje ta podfaza.** Etap 6 nie był domknięty: blok `Details` z §5.6 nie wszedł do enuma `Rule` w M2d i żaden pakiet nie był jego właścicielem | M2d zamknął swoje kryterium („budynki mają piętra, lokale i stanowiska"), ale §5.6 obiecywał też detal bryły, a `Rule` go nie ma. Pakiet bez właściciela to sytuacja (5) z `K-18` |
| G2 ★ | **Kolejność wykonania: M2d → M2f → M2e.** M2e zostaje ostatnia i nadal zamyka bramki 1–7 | T10 jest wg F6 zadaniem kalibracyjnym, a jedną z jego trzech dźwigni jest `massing` gramatyk. Katalog rozszerzony po kalibracji = kalibracja dwa razy |
| G3 ★ | Test **T13** dopisany do §7 dokumentu fazy | „Różnorodny" bez progu jest opinią; `K-18` pkt 3 zabrania kryteriów niemierzalnych |
| G4 | Zakres WP18 to **jeden** wariant enuma (`Protrude`), nie blok pięciu reguł `Details` | Cztery z pięciu detali z §5.6 są wyrażalne zestawem, który już jest w kodzie (tabela w §5.6c). Nowy operator dostaje tylko to, czego naprawdę nie da się złożyć — ryzyko R4 („gramatyka staje się drugim językiem programowania") rośnie z każdym wariantem |

---

## Zmiany wpisane po M2f

Numeracja `H-n`, jak `E-n` w M2d. Gwiazdką te, które zmieniają **zakres albo kryterium**.

| # | Korekta | Dlaczego |
|---|---|---|
| H1 ★ | **Kryterium WP18 mówiło co innego niż §5.6c.** Tabela żądała, żeby żadne wysunięcie nie wyszło nad pas drogowy; §5.6c dopuszczał wysięg nad chodnikiem powyżej 3,5 m. Obowiązuje §5.6c, tabela poprawiona | Przy zakazie bezwarunkowym kamienica w pierzei nie ma prawa do **żadnego** balkonu od ulicy, bo `setback_front_m` wynosi tam 0 z definicji zabudowy obrzeżnej. Kryterium byłoby spełnialne wyłącznie przez katalog bez balkonów frontowych — czyli mierzyłoby co innego, niż nazywa (`K-18` pkt 3). Granica 1,5 m jest **wyprowadzona, nie przyjęta**: jezdnia zajmuje 60 % pasa drogowego (`voxels.rs`), najwęższa klasa uliczna `Service` ma `row_m` 8 m, więc pobocze ma 1,6 m z każdej strony. Wysięg 1,5 m nie dosięga jezdni w żadnej klasie i przestaje być prawdziwy dopiero, gdyby doszła klasa węższa niż 8 m |
| H2 ★ | **`footprint_for` obraca oś `u` tak, żeby ulica była zawsze po stronie `−v`.** Błąd zastany z M2d, nie skutek tej podfazy | `u` brało się z kierunku odcinka drogi, a kierunek odcinka nie mówi, po której jego stronie leży działka. `Face::Front` w gramatyce wypadał więc na podwórzu mniej więcej co drugi budynek: witryna parteru usługowego patrzyła w oficynę, a `Comp(Front, …)` stawiał pas okien od tyłu. W liczbach nie było tego widać — okna były, tylko nie tam. Dla WP18 to warunek konieczny: bez tego `Protrude(Front)` wychodziłby w głąb kwartału i przycięcie kasowałoby go dokładnie tam, gdzie balkon miał powstać. Przy okazji znika rozgałęzienie `front_na_minusie` w cofnięciach i w trakcie — teraz front jest zawsze od `v0` |
| H3 | **Lukarny są parametrem reguły `Roof`, nie wariantów `RoofShape`** | `dormers` na `RoofShape` trzeba byłoby dopisać osobno do `Gable` i do `Hip`, a walidator i tak musi odrzucić lukarny na dachu płaskim (`DormersOnFlat`). Jedno pole z `#[serde(default)]` nie łamie żadnego pliku z katalogu i daje walidatorowi jedno miejsce do sprawdzenia |
| H4 | **Zapas na wysunięcie liczy się od lica bryły, nie od lica bieżącego zakresu** | Inaczej `Inset(0,4) → Protrude` na cofniętym poddaszu zjadałby 0,4 m balkonu, choć działka się nie zwęziła. Zakres wie, gdzie jest względem obrysu (`Ctx::base`), więc luz da się policzyć dokładnie zamiast zakładać najgorszy przypadek. Działa też w drugą stronę: zakres po `Offset` ma zapas **mniejszy** o tyle, o ile już wystaje |
| H5 | **`Building.aabb` obejmuje wysunięcia i lukarny, a nie tylko obrys** | `aabb` ma jednego odbiorcę — selekcję do kadru w M11 (kontrakt §6 fazy). Bryła licząca tylko obrys ucinałaby balkony przy krawędzi ekranu, i to dokładnie przy tej krawędzi, przy której gracz na nie patrzy |
| H6 | Zapas mierzony liniowo co 0,25 m do `MAX_PROTRUDE_M`, nie połowieniem, i **bez** zapamiętywania go w `GrammarSet` per gramatyka | 64 testy przynależności na kierunek przy 14,5 tys. budynków w kroku, który i tak ma budżet 28 s. Optymalizacja „licz zapas tylko dla gramatyk z `Protrude`" wymagałaby dodatkowego stanu w katalogu i nie ma czego kupić (YAGNI) |
| H7 ★ | **Odwołane.** Diagnoza „połać dachu faluje przez trzy nachodzące schodkowania" była błędna i zostaje wycofana razem ze zmianami, które na jej podstawie zrobiłem (próg stopnia dachu, rytm okien, wysokość witryny) | Sprawdzone liczbowo i wzrokowo: derywacja daje dokładnie tyle brył, ile opisuje plan, a zrzut z commitu poprzedzającego wygląda identycznie. Falowanie widać **tak samo na zieleni terenu**, na jezdni i na brzegu rzeki — to nie jest cecha dachu, tylko rasteryzacji obróconych brył przy voxelu 1 m, czyli wygląd całego silnika. Wniosek dla planu: takie zadanie nie należy do WP19 ani do żadnego pakietu M2 — należy do M11 (profil ciągły z prefabrykatu) albo do rozmowy o rozmiarze voxela, czyli do M1. Zapisane tutaj, żeby następny, kto to zobaczy, nie zaczął od tej samej hipotezy |
| H8 | `Applies::covers` jest **jednym** źródłem odpowiedzi na „czy ta gramatyka dotyczy tej kombinacji": używa jej i dobór dla parceli (`build::pasuje`), i test luk w katalogu (`GrammarSet::covered`) | Dwie kopie tej reguły rozjechałyby się przy pierwszym nowym filtrze, a wtedy test pokrycia mówiłby „katalog kompletny" o katalogu, z którego generator nie umie wybrać. `None` w miejscu epoki albo stylu znaczy „filtr pominięty" — pusty klucz **zaostrzałby** filtr zamiast go pomijać i przy pierwszym podejściu dokładnie to zrobił (udział awaryjnych skoczył z 1,1 % na 7,2 %) |
| H9 | `relaxed` rozbite na `relaxed_epoch`, `relaxed_style`, `relaxed_value` | Jedna liczba nie mówi, co zrobić. Brak epoki albo stylu łata się **plikiem** w `data/grammar/`, brak przedziału wartości gruntu — **liczbą** w pliku, który już jest. To są dwie różne prace i raport ma je rozróżniać |
| H10 | Test macierzy pokrycia pomija `Green` i `Extraction` | To są strefy, których `plan_building` nie zabudowuje z zamiaru (korekta E6 z M2d) — obiekty dostają w Etapie 7. Bez tego wyjątku test liczył 18 „luk", z których żadna nie była luką |
| H11 | Stary test `katalog_gramatyk_pokrywa_strefy_miasta` (próg 2 %) usunięty, jego dwie własne asercje przeniesione do nowego | Dwa testy pilnujące tego samego progu z różnymi liczbami to dwa miejsca do poprawienia przy następnej zmianie kryterium — a kryterium WP19 jest ostrzejsze i ma pierwszeństwo |

---

## Co WP18 zostawia dalszym pakietom

- `Rule::Protrude` i `Roof.dormers` są w języku i w walidatorze; `data/grammar/` używa ich
  w dwóch plikach (`blok.ron` — balkony frontowe, `kamienica.ron` — balkony od podwórza
  i lukarny). **WP19 rozstawia je po całym katalogu**, a nie dopisuje operatorów.
- `BuildReport.{protrusions, protrusions_clipped, protrusions_dropped}` są w raporcie
  generacji. Pomiar na mieście 8 km, ziarno 7: **8 283 wysunięcia, 19 przyciętych,
  154 odrzucone** — czyli przycinanie działa i prawie nigdy nie jest potrzebne,
  bo katalog prosi o wysięgi, które się mieszczą.
- `Face::Front` jest od tej pory **zawsze** licem od ulicy (korekta H2). Gramatyki pisane
  w WP19 mogą na tym polegać; wcześniejsze były pisane w świecie, w którym to nie było prawdą.

| H12 ★ | **Entropia liczona po sygnaturach, nie po `GrammarId`**, i dochodzi drugi próg — entropia całego miasta ≥ 4,0 bita. §5.6c mówił „entropia rozkładu `GrammarId`"; to było mierzenie złej rzeczy | Dzielnica „Bór" (ziarno 7, profil przemysłowy) dostała 0,29 bita, bo 108 ze 112 budynków to `chalupa`. Tyle że te chałupy różnią się materiałem, kształtem dachu i wysokością — z ulicy **są** różne. Entropia gramatyk mierzy, jak zorganizowany jest katalog; entropia sygnatur mierzy to, co widzi gracz. Po zamianie ta sama dzielnica ma 3,45 bita i jest to liczba prawdziwa. Próg miasta dochodzi, bo dzielnica ma prawo być jednorodna (osiedle płytowe takie jest i ma takie zostać), a miasto nie ma |
| H13 | Reguły **rozdzielające** (`Seq`, `Choice`, `If`, `Ref`) zwolnione ze strażnika zdegenerowanego zakresu w `derive::apply` | Bryła w momencie wyboru dachu ma zerową wysokość (dach dostaje ją dopiero od `Floors`), więc `Choice([Roof, Roof])` wypadał w całości i **nie było jak zapisać losowania kształtu dachu** — trzeci kanał wariancji z §5.6c był martwy. Strażnik ma pilnować reguł, które naprawdę pracują na zakresie; reguła, która tylko przekazuje go dalej, nie ma czego pilnować. Strażnik nadal zatrzymuje `Fill` i `Comp` na liściu |
| H14 | Kształt dachu jako trzeci kanał `Choice` w dziewięciu gramatykach | Sygnatura ma cztery znaczniki, a jeden z nich — dach — był w całym katalogu stały. Czwarta część miary leżała odłogiem. Efekt zmierzony na tym samym mieście: powtórki sygnatury w promieniu 60 m **14,2 % → 8,6 %**, bez dokładania ani jednego pliku |
| H15 | Dwie monokultury znalezione **przez miarę** i naprawione w danych: `kamienica_modernistyczna` miała próg wartości gruntu 300 000 gr/m², przez co na taniej ziemi w ogóle nie kandydowała i `kamienica_plombowa` brała 441 z 474 budynków dzielnicy; `blok_wspolczesny` wymagał 14 m frontu, czyli więcej, niż ma typowa wąska działka R3 | Obie były niewidoczne w pokryciu: macierz była pełna, fallback poniżej progu, a mimo to jedna gramatyka wygrywała 93 % losowań w dzielnicy. Dokładnie po to jest druga miara — pokrycie mówi „jest z czego wybierać", entropia mówi „i faktycznie wybiera" |
| H16 ★ | **Próg wartości gruntu w `applies` jest bezpieczny na typie prestiżowym i groźny na typie roboczym.** `blok_wspolczesny` i `kamienica_modernistyczna` tracą swoje progi; `wiezowiec`, `wiezowiec_mieszkalny`, `willa_miedzywojenna` i `kamienica_secesyjna` zostają ze swoimi | To samo znalezisko co H15, tylko na szerszej macierzy: T13 na 32 ziarnach złapał ziarno 1 / profil przemysłowy z 16,5 % powtórek, bo `blok_wspolczesny` miał próg 400 000 gr/m², w tanim mieście nie kandydował, a `kamienica_plombowa` brała 546 z 650 budynków dzielnicy. Reguła, którą z tego wyciągam: próg wartości gruntu wolno postawić typowi, **poniżej którego ktoś inny przejmuje** (wieżowiec ma pod sobą blok, willa ma pod sobą kostkę) — nigdy typowi, który jest dla swojej kombinacji jedynym kandydatem. Po zdjęciu progu: 10,5 % powtórek, entropia dzielnicy 3,31 → 4,09 bita |

---

## Co WP19 zostawia WP20

Katalog liczy **32 pliki** (31 gramatyk + awaryjna), z 12 po M2d. Pomiar na mieście 8 km,
ziarno 7, wobec stanu po WP18:

| Miara | Po WP18 | Po WP19 | Próg |
|---|---|---|---|
| gramatyka awaryjna | 55 (1,07 %) | **8 (0,15 %)** | < 0,5 % |
| dobór po rozluźnieniu filtru | 295 (5,7 %) | **35 (0,67 %)** | < 5 % |
| luki macierzy (strefa × epoka × styl) | 9 | **0** | 0 |
| mieszkania | 40 810 | **47 462** | T10 kalibruje M2e |
| wysunięcia bryły | 8 283 | 8 764 | — |
| czas etapu zabudowy | 41,8 ms | 40,6 ms | 28 s |

Nowe typy wobec M2d: bliźniak (nie było go **wcale**), chałupa, kostka, willa
międzywojenna, szeregówka ceglana, trzy warianty kamienicy (secesyjna, modernistyczna,
plombowa), kamienica biurowa, pasaż, market z parkingiem, punktowiec, galeriowiec,
apartamentowiec, wieżowiec mieszkalny, skład ceglany, hala lekka, magazyn wysokiego
składowania, szkoła, kościół z wieżą.

**Czego WP20 nie dostanie za darmo:** rdzeń miasta (R3 i Commercial w pierścieniach
sprzed 1918) nadal wygląda jednorodnie, mimo że ma tam teraz pięciu kandydatów zamiast
jednego. Wagi są tak dobrane, że `kamienica` wygrywa większość losowań, a pozostałe
warianty różnią się od niej detalem, nie bryłą. To jest dokładnie to, co ma zmierzyć
entropia w teście T13 — i jeśli T13 upadnie, upadnie właśnie tam, a nie na przedmieściach.

---

## Co zostaje po M2f

Podfaza zamknięta. Test **T13** stoi w `sim/world/tests/city_m2d.rs` i chodzi po
**32 ziarnach × 4 profile** na mieście 4 km. Trzy progi, każdy odpowiada na inne pytanie:

| Miara | Próg | Najgorsza wartość w próbce | Co łapie |
|---|---|---|---|
| powtórki sygnatury w promieniu 60 m | < 15 % | 10,5 % | pierzeja z jednej formy powielonej dwadzieścia razy |
| entropia sygnatur w dzielnicy (≥ 100 budynków, mieszkaniowa) | ≥ 1,8 bita | 3,45 bita | dzielnica z jednym rodzajem domu |
| entropia sygnatur w mieście | ≥ 4,0 bita | 5,91 bita | miasto z jednym rodzajem domu |

**Progi są podłogami przeciw regresji, nie celami.** Zostały skalibrowane pomiarem
i mają margines rzędu dwukrotności — zmiana, która je przebije, nie „lekko pogorszyła
różnorodność", tylko ją zawaliła. Wymyślanie progów przed pomiarem raz już się nie udało
(korekta H12), a 16-punktowa próbka okazała się za mała: pełny przebieg 32 × 4 znalazł
przypadek o 4,6 pp. gorszy od wszystkiego, co widziała próbka (korekta H16). **Kalibrować
próg wolno wyłącznie na tej macierzy, na której test potem chodzi.**

`GenerationReport` niesie komplet: histogram gramatyk, liczbę par sąsiedztwa i powtórek,
najgorsze sąsiedztwo z **współrzędnymi** (żeby dało się tam polecieć kamerą), entropię
miasta, najniższą entropię dzielnicy z jej numerem oraz gramatykę, która ją zdominowała,
i w jakiej proporcji.

`BuildingSignature` **nie jest trzymana w `Building`** — powstaje w generacji, wchodzi
do miar i ginie. Do raportu trafiają agregaty, nie tablica 14,5 tys. sygnatur.

Otwarte, świadomie nieruszone: rdzeń miasta (R3 i Commercial w pierścieniach sprzed 1918)
ma pięciu kandydatów, ale `kamienica` nadal wygrywa większość losowań. T13 tego nie łapie,
bo sygnatury **wewnątrz** kamienicy są zróżnicowane (trzy materiały × trzy dachy × zakres
kondygnacji). Jeśli kiedyś ma się to zmienić, dźwignią są wagi w `applies`, a nie nowy plik.
