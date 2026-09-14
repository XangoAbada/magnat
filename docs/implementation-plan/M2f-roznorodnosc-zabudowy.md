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
| WP18 | Operator `Protrude` i lukarny | M2d (WP11, WP12) | wariant `Rule::Protrude { face, m, rule }` — wysunięcie zakresu **poza jedną** ścianę; `dormers` w `RoofShape::{Gable, Hip}`; przycięcie wysunięcia do granicy parceli i pasa drogowego | balkon, wykusz i lukarna dają się zapisać w `.ron` bez ani jednej linii Rusta; żadne wysunięcie nie wychodzi poza wielokąt parceli ani nad pas drogowy (rozszerzony T6); budżet `MAX_NODES` trzymany na kamienicy 6-kondygnacyjnej z balkonami na każdym piętrze |
| WP19 | Katalog gramatyk: pokrycie i wariancja | WP18 | rozszerzenie `data/grammar/` z 12 do ~32 plików wg typologii z §5.6c; `Choice` wewnątrz gramatyk na materiał, rytm otworów i kształt dachu | macierz pokrycia (strefa × epoka × styl) **bez luk**: każda kombinacja występująca w mieście ma ≥ 1 gramatykę bez rozluźniania filtrów; fallback < 0,5 %; `relaxed` < 5 % |
| WP20 | Miara różnorodności + test T13 | WP19 | `BuildingSignature` (4 znaczniki), histogram gramatyk i kolizje sygnatur w `GenerationReport`; test T13 | T13 zielony: udział par identycznych sygnatur w promieniu 60 m < 15 %; entropia rozkładu gramatyk w dzielnicy mieszkaniowej o ≥ 100 budynkach ≥ 1,8 bita; oba progi mierzone na 32 ziarnach × 4 profile |

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
