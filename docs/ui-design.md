# Design interfejsu użytkownika

Status: **kontrakt wiążący** dla każdej fazy dokładającej UI. Podlega `PRD_Magnat.md` §14 i §16.4
oraz `docs/implementation-plan/00-konwencje-i-kontrakty.md`.
Właściciel dokumentu: M9 (faza, w której powstaje całe UI gry); M3 założył szkielet i pierwsze
widgety, M11 dokłada warstwę prezentacji, M12 lokalizację i modding.

PRD §16.4 mówi, **co** interfejs musi umieć. Ten dokument mówi, **jak** ma wyglądać i dlaczego
tak — żeby dwanaście faz dokładających panele nie wyprodukowało dwunastu wyglądów.

---

## 1. Zasada nadrzędna

**Interfejs jest przyrządem pomiarowym, nie dekoracją.** Gra sprzedaje jedną obietnicę: gracz
rozumie, *dlaczego* miasto zachowało się tak, jak się zachowało (PRD §14.1). Wszystko, co utrudnia
odczyt liczby albo powodu, jest błędem projektowym — także wtedy, gdy ładnie wygląda.

Trzy konsekwencje, z których wynika reszta dokumentu:

1. **Gęstość ponad przestronność.** Panel Finanse ma pokazać księgę, a nie trzy kafelki z dużym
   fontem. Domyślna gęstość jest bliższa arkuszowi kalkulacyjnemu niż stronie marketingowej.
2. **Kolor niesie znaczenie i nic poza tym.** Jeśli coś jest czerwone, to znaczy „strata" albo
   „awaria" — nigdy „to jest nagłówek".
3. **Każde ograniczenie mówi, skąd się wzięło.** Wygaszony przycisk bez powodu jest ślepym
   zaułkiem; mamy `precheck()` zwracający `CommandError` (M9a §5.5), więc powód istnieje zawsze
   i zawsze jest pokazywany.

---

## 2. Fundament techniczny (co jest dane, a nie do wyboru)

- Rdzeń: **`egui` + `egui-wgpu`** (decyzja M3 9.2, PRD §16.4 korekta). `engine/ui` nie zna `wgpu`
  ani `winit` — dzięki temu każdy ekran da się narysować w teście CI bez karty graficznej.
- Warstwa modeli: widget dostaje **model** (`CitizenModel`, `TimeControlsWidget`, …) i zamienia go
  na piksele. **W kodzie rysującym nie ma ani jednej liczby, której nie policzył model** — inaczej
  złoty test wydruku przestaje bronić tego, co widzi gracz.
- Wszystkie napisy to `LocKey` + wpis w `data/locale/{pl,en}.ron`. Literał w kodzie UI jest błędem
  (CLAUDE.md). Przestrzenie kluczy: `ui.shell.*` (powłoka), `ui.newgame.*` (kreator),
  `ui.panel.<id>.*`, `ui.overlay.*`, `ui.reason.*` (powody decyzji), `ui.error.*` (`CommandError`).
- Wartości z §3 i §4 tego dokumentu są **danymi**, nie stałymi w kodzie: `data/ui/theme.ron`
  (właściciel: M9b, katalog `data/ui/` z `K-19`). Motyw ma `schema_version` jak każdy plik w `data/`.

---

## 3. Tokeny

### 3.1 Kolor

Motyw domyślny jest **ciemny**, bo interfejs leży nad sceną 3D z pełnym zakresem jasności —
jasny HUD nad nocnym miastem oślepia, ciemny nad południowym placem nadal się czyta. Motyw jasny
jest dopuszczalny jako drugi zestaw tokenów (M12), nie jako drugi projekt.

| Token | Wartość | Do czego |
|---|---|---|
| `bg.scene` | — (widok 3D) | pod HUD-em; UI nigdy nie zakrywa go bez powodu |
| `bg.window` | `#14171A` | tło ekranów powłoki i paneli pełnoekranowych |
| `bg.panel` | `#1E2226` | tło panelu zadokowanego |
| `bg.card` | `#232830` | karta inspekcji, wiersz wyróżniony, pole edycji |
| `bg.overlay` | `#14171A` @ 80% | podkład pod HUD nad sceną |
| `line.soft` | `#2E353C` | linie siatki tabeli, separatory |
| `line.strong` | `#3C444D` | obramowanie panelu, ramka pola |
| `text.primary` | `#E4E8EC` | liczby i treść |
| `text.secondary` | `#9AA4AE` | etykiety, jednostki, podpisy osi |
| `text.disabled` | `#626C76` | element niedostępny (zawsze z powodem obok) |
| `accent` | `#5078AA` | zaznaczenie, element aktywny, własność gracza |
| `accent.hi` | `#6FA0DC` | obwódka fokusu klawiatury, hover |
| `ok` | `#5AA064` | zysk, stan dobry, cel osiągnięty |
| `warn` | `#C8A03C` | stan graniczny, zapas poniżej progu, ostrzeżenie polityki |
| `danger` | `#C8463C` | strata, awaria, potrzeba krytyczna, komenda odrzucona |
| `info` | `#5078AA` | zdarzenie neutralne, podpowiedź |

`ok` / `warn` / `danger` to **dokładnie** trzy kolory, którymi M3 maluje już paski potrzeb
(`engine/ui::widgets::kolor_potrzeby`) — i taki był zamysł: trzy progi, nie gradient, bo gracz ma
odczytać „dobrze / słabo / źle" jednym spojrzeniem, a nie porównywać odcienie. `accent` to kolor
czynności `Work` z osi dnia, z tego samego powodu: to, co należy do gracza i jego pracy, ma jeden
kolor w całej grze.

**Palety kategorii** (kolor rozróżnia rodzaj, nie wartość) są ustalone raz i nie powtarzają się
między dziedzinami: czynności doby — `ActivityKind` w `engine/ui::widgets`; nakładki danych —
`data/ui/overlays.ron` (`K-19`); branże i towary — `data/goods/` (M6). Nowa dziedzina dopisuje
własną paletę do danych, nigdy nie pożycza cudzej.

**Reguła daltonizmu:** kolor nigdy nie jest jedynym nośnikiem informacji. Zysk ma znak `+`, strata
`−`, awaria ikonę, seria na wykresie kształt punktu, nakładka legendę z liczbami. Test: zrzut
w skali szarości musi pozostać czytelny.

### 3.2 Typografia

Jeden krój dla tekstu i jeden dla liczb. Oba **muszą** mieć pełny zestaw polskich diakrytyków —
sprawdzane testem renderującym `ąćęłńóśźż ĄĆĘŁŃÓŚŹŻ`, bo brak znaku w atlasie widać dopiero
u gracza.

| Rola | Rozmiar (px logiczne) | Grubość | Uwaga |
|---|---|---|---|
| `text.micro` | 11 | 400 | podpisy osi, jednostki |
| `text.body` | 13 | 400 | domyślny tekst i wiersz tabeli |
| `text.strong` | 13 | 600 | nagłówek kolumny, etykieta wyróżniona |
| `text.title` | 15 | 600 | tytuł panelu |
| `text.screen` | 20 | 600 | tytuł ekranu powłoki |
| `text.hero` | 28 | 600 | tytuł gry w menu głównym |

**Liczby są tabelaryczne i monospace** wszędzie, gdzie stoją jedna pod drugą: pieniądz, ilości,
czas, wiersze tabel, osie wykresów. Kolumna kwot wyrównana do prawej, przecinek dziesiętny i spacja
jako separator tysięcy w PL (`12 345,67 zł`) — formatowanie per `Locale`, nigdy ręcznie w kodzie
widgetu.

Rozmiar jest mnożony przez `ui_scale ∈ [0,75; 3,0]` (M9b §5.8) i przyciągany do pikseli fizycznych.

### 3.3 Przestrzeń i kształt

- **Siatka 4 px.** Wszystkie odstępy to `4 · n`. Zestaw: `4` (wewnątrz kontrolki), `8` (między
  kontrolkami), `12` (między grupami), `16` (margines panelu), `24` (między sekcjami ekranu).
- **Wysokość wiersza tabeli: 22 px** przy skali 1,0 — stała w obrębie tabeli, bo matematyka
  przewijania `Table<T>` jest O(1) tylko przy stałej wysokości (M9b §5.8).
- Wysokość kontrolki interaktywnej: **24 px** (przycisk, pole, pozycja listy), **32 px** w powłoce,
  gdzie klikalność jest ważniejsza od gęstości.
- Promień zaokrąglenia: **3 px**. Cienie: wyłącznie pod elementem pływającym (okno niedokowane,
  menu kontekstowe, podpowiedź). Zadokowany panel oddziela `line.strong`, nie cień.
- Szerokość doku bocznego: 320 px domyślnie, 260–560 px w zakresie regulacji.

---

## 4. Komponenty

Lista jest zamknięta w tym sensie, że **nowy panel składa się z tych elementów**; element spoza
listy wymaga dopisania go tutaj, nie wymyślenia go lokalnie.

| Komponent | Zasada |
|---|---|
| Przycisk | Trzy wagi: podstawowy (`accent`), zwykły (obrys), tekstowy. Wyłączony **zawsze** z powodem: podpowiedź renderuje `CommandError` (np. „brakuje 12 400 zł") |
| Pole liczbowe | Jednostka w polu, nie w etykiecie; walidacja przy każdej zmianie; wartość spoza zakresu podświetla `danger` i mówi, jaki zakres obowiązuje |
| Przełącznik / wybór | Do pięciu wariantów — segmenty w rzędzie; powyżej — lista rozwijana z filtrem |
| Tabela (`Table<T>`) | Nagłówek przyklejony, sortowanie kliknięciem, filtr w nagłówku kolumny, wirtualizacja; kolumny liczbowe do prawej, tekstowe do lewej |
| Karta inspekcji | Nagłówek z tożsamością **zawsze widoczny**, reszta w zakładkach o stałej kolejności: stan → historia → powiązania → **powody** (`DecisionReason`). Zakładka pusta dla danej encji jest ukryta, nie wyszarzona. Każda nazwa innego podmiotu w karcie jest odnośnikiem |
| Zakładki | Rząd etykiet nad treścią; wybrana ma pełny kontrast, reszta `text.dim`. Do siedmiu zakładek w rzędzie — powyżej dziel panel, nie zwijaj etykiet. Wybór przeżywa przebudowę drzewa i zmianę zaznaczenia na encję **tego samego typu**; przy zmianie typu wraca na pierwszą. Sterowanie klawiaturą: strzałki lewo/prawo w obrębie rzędu |
| Odnośnik | Nazwa innego podmiotu (mieszkaniec, firma, zakład, budynek, pojazd, parcela, dzielnica) jest klikalna i otwiera jego kartę. Wygląd: kolor `accent`, podkreślenie dopiero pod kursorem — tekst karty ma zostać czytelny, gdy odnośników jest kilkanaście. Odnośnik do celu, który już nie istnieje (firma upadła, mieszkaniec zmarł), pokazuje nazwę bez odnośnika i powiada, co się stało — nigdy nie prowadzi w pustkę. Nawigacja ma **wstecz** i **dalej** (myszka: przyciski boczne, klawiatura: Alt+strzałki) |
| Wykres (`Series`) | Oś czasu w kalendarzu 12 × 30 (`K-1`); poziom mip dobrany do szerokości; maksimum 2000 odcinków niezależnie od zakresu; bez animacji przy zmianie zakresu |
| Pasek czasu | Data, zegar, cztery prędkości (pauza / 1× / 3× / 10×). Jedyny element UI zawsze widoczny w rozgrywce |
| Alert | Jedna linia: waga (`warn`/`danger`), czego dotyczy, co z tym zrobić. Kliknięcie otwiera podmiot. Alert bez możliwej akcji jest wpisem kroniki, nie alertem |
| Nakładka (legenda) | Nazwa pola, skala z liczbami i jednostką, wartość pod kursorem. Paleta z `data/ui/overlays.ron`, ta sama w kliencie i w podglądzie `headless` |
| Podpowiedź | Pojawia się po 400 ms, znika natychmiast; nigdy nie niesie informacji, której nie ma nigdzie indziej |

**Stany interakcji** są wspólne dla wszystkich komponentów: normalny → hover (`accent.hi` 15%) →
wciśnięty (`accent` 25%) → wyłączony (`text.disabled`, brak hovera) → fokus klawiatury (obwódka
2 px `accent.hi`, zawsze widoczna, nigdy usuwana dla urody).

---

## 5. Układ ekranu rozgrywki

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ [pauza 1x 3x 10x]  Rok 3, 14 marca, wt  08:42  │  1 284 300,00 zł  ▲ +12 400 │  ← pasek czasu i stanu
├───────────────┬─────────────────────────────────────────────┬────────────────┤
│               │                                             │                │
│  PANELE       │                                             │  INSPEKCJA     │
│  (dok lewy)   │                                             │  (dok prawy)   │
│  320 px       │             WIDOK 3D MIASTA                 │  320 px        │
│               │                                             │                │
│  Pulpit       │        nakładka danych + legenda            │  Anna Kowalska │
│  Sklep        │        w prawym dolnym rogu                 │  32 l., kasjer │
│  Finanse      │                                             │ [stan][dzień]  │
│  Ludzie       │                                             │ [rodzina][…]   │
│  …            │                                             │  Głód   ███░░  │
│               │                                             │                │
├───────────────┴─────────────────────────────────────────────┴────────────────┤
│ ⚠ Brak mleka w „Kiosk nr 2" od 2 dni — zamów albo zmień dostawcę    [pokaż]  │  ← pas alertów (maks. 3)
└──────────────────────────────────────────────────────────────────────────────┘
```

Reguły układu:

- **Widok 3D jest zawsze widoczny.** Panel pełnoekranowy istnieje wyłącznie dla kroniki i edytora
  reguł; wszystko inne dokuje się po bokach. Gracz ma widzieć skutek swojej decyzji w mieście.
- **Domyślny układ to dwa panele, nie osiem** — wymóg onboardingu z PRD §20.3 (pierwsza sensowna
  decyzja w ≤ 12 interakcjach i ≤ 3 panelach).
- Dok prawy należy do **inspekcji** (to, co gracz kliknął), lewy do **paneli biznesowych** (to, co
  gracz prowadzi). Ta różnica jest stała — zamiana miejscami psuje nawyk.
- **Dok prawy ma jedną kartę i historię**, nie stos okien. Odnośnik podmienia zawartość doku
  i odkłada poprzednią kartę na stos wstecz (32 pozycje, najstarsze wypadają). Bez tego karta
  z kilkunastoma odnośnikami jest ślepą uliczką: gracz skacze od Anny do konkurenta, do jego
  dostawcy — i nie ma jak wrócić do pytania, które zadawał.
- **Każdy obiekt widoczny w świecie jest klikalny** — mieszkaniec, budynek, pojazd, zakład,
  parcela. Obiekt narysowany, którego nie da się kliknąć, jest dekoracją, a ta łamie zasadę z §1.
- Układ jest preferencją widoku: zapisuje się w profilu gracza, nie w zapisie świata (M9b §5.8).

---

## 6. Ekrany poza rozgrywką (PRD §14.7)

### 6.1 Menu główne

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                                                                              │
│                              M A G N A T                                     │  ← text.hero
│                     symulator miasta i gospodarki                            │
│                                                                              │
│                     ┌──────────────────────────────┐                         │
│                     │  Kontynuuj                   │  ← przycisk podstawowy, │
│                     │  Rzeszów · rok 4 · 12,3 h    │    ukryty gdy brak      │
│                     ├──────────────────────────────┤    zapisu               │
│                     │  Nowa gra                    │                         │
│                     │  Wczytaj                     │                         │
│                     │  Ustawienia                  │                         │
│                     │  Wyjdź                       │                         │
│                     └──────────────────────────────┘                         │
│                                                                              │
│  wersja 0.1.0 · format zapisu 1                          [PL] [EN]           │
└──────────────────────────────────────────────────────────────────────────────┘
```

Wersja gry i wersja formatu zapisu są widoczne, bo od nich zaczyna się każde zgłoszenie błędu.
Przełącznik języka jest tutaj, a nie tylko w ustawieniach — gracz, który nie zna polskiego, musi
umieć go zmienić bez czytania polskiego menu.

### 6.2 Nowa gra — kreator świata

To jest ekran, który zdejmuje z gracza wiersz poleceń. Każdy parametr ma jedno zdanie o tym, **co
zmienia w grze** — nie definicję słownikową.

```
┌──────────────────────────────────────────────────────────────────────────────┐
│  Nowa gra                                                        [Wstecz]    │
├────────────────────────────────────────────────┬─────────────────────────────┤
│  ZIARNO   [ 0xC0FFEE            ] [Losuj]      │                             │
│           To samo ziarno i te same parametry   │    podgląd pojawia się      │
│           dają ten sam świat co do metra.      │    po wygenerowaniu         │
│                                                │    (§6.3)                   │
│  ROZMIAR  (4 km)(8 km)(12 km)(16 km)           │                             │
│           8 km ≈ 120 tys. mieszkańców.         │                             │
│           16 km to metropolia i ~15 s          │                             │
│           generacji.                           │                             │
│                                                │                             │
│  REGION   (nadmorski)(górski)(nizinny)         │                             │
│           (rzeczny)(pustynny)                  │                             │
│           Rzeczny: port rzeczny i mosty,       │                             │
│           tania woda dla przemysłu.            │                             │
│                                                │                             │
│  EPOKA    (1950)(1970)(1990)(2010)(2020)       │                             │
│  PROFIL   (przemysłowy)(portowy)               │                             │
│           (uniwersytecki)(turystyczny)         │                             │
│           (rolniczy)(mieszany)                 │                             │
│  TRUDNOŚĆ (łatwo)(normalnie)(trudno)(brutal)   │                             │
│                                                │                             │
│  SCENARIUSZ  [ Tryb otwarty              ▾ ]   │                             │
│  START       [ Absolwent bez kapitału    ▾ ]   │                             │
│              Pożyczka od rodziny, brak         │                             │
│              oszczędności, dużo czasu.         │                             │
├────────────────────────────────────────────────┴─────────────────────────────┤
│                                                     [Generuj świat]          │
└──────────────────────────────────────────────────────────────────────────────┘
```

Komplet parametrów to dokładnie `WorldGenParams` (`sim/world::params`) plus scenariusz i wariant
startu. Nic tu nie jest „ustawieniem zaawansowanym" ukrytym za rozwijaczem: to sześć pól, które
w całości opisują świat, a ukrycie ich zmusza do wiersza poleceń.

### 6.3 Generacja i podgląd

```
┌──────────────────────────────────────────────────────────────────────────────┐
│  Tworzę świat — ziarno 0xC0FFEE, 8 km, rzeczny, 1990, mieszany               │
│                                                                              │
│  ████████████████████████████░░░░░░░░░░░░░░░░  63 %                          │
│  Strefy i parcele…                                     8,4 s                 │
│                                                                              │
│  ✓ maska lądu ✓ wysokości ✓ wypiętrzenie ✓ erozja ✓ hydrologia ✓ klimat      │
│  ✓ złoża ✓ drogi ▸ strefy · zabudowa · firmy · populacja                     │
│                                                                              │
│                                                        [Anuluj]              │
└──────────────────────────────────────────────────────────────────────────────┘
                                    ↓ po zakończeniu
┌──────────────────────────────────────────────────────────────────────────────┐
│  Świat gotowy                                                                │
├─────────────────────────────────────────┬────────────────────────────────────┤
│                                         │  Mieszkań              47 100      │
│      mapa z góry: wody, drogi,          │  Miejsc pracy          52 400      │
│      zabudowa, granice dzielnic         │  Powierzchnia miasta    31 km²     │
│                                         │  Dzielnic                  14      │
│                                         │  Firm startowych        3 820      │
│                                         │  Dominują: przemysł spożywczy,     │
│                                         │            transport, handel       │
│                                         │  Złoża: żwir, wapień, glina        │
├─────────────────────────────────────────┴────────────────────────────────────┤
│  [Losuj inny świat]  [Zmień parametry]                    [Gram tutaj →]     │
└──────────────────────────────────────────────────────────────────────────────┘
```

Etapy nie są ozdobą: generator już dziś raportuje je z nazwą i czasem (`WorldGenReport`, `PASSES`
w `sim/world::pipeline`), więc ekran pokazuje prawdziwy postęp, a nie animowany pasek. Anulowanie
jest wymagane — metropolia 16 km to około 20 sekund samego terenu i gracz ma prawo się rozmyślić.

Podgląd przed rozpoczęciem gry istnieje z jednego powodu: odrzucenie miasta bez portu kosztuje
dwadzieścia sekund teraz albo godzinę później. **Ekran pokazuje pojemność, nie populację** —
zaludnienie (Etap 8, kolejne 27 s dla metropolii) rusza dopiero po „Gram tutaj", więc w tym
momencie nie istnieje ani jeden mieszkaniec i podanie ich liczby byłoby zmyśleniem (M9a §5.13).
Po akceptacji ekran ładowania wraca z drugim etapem: zaludnienie, gospodarstwa, praca, szkoły.

### 6.4 Wczytaj / zapisz

```
┌──────────────────────────────────────────────────────────────────────────────┐
│  Wczytaj                                                         [Wstecz]    │
├──────────────────────────────────────────────────────────────────────────────┤
│  ▸ Rzeszów           rok 4, 12 maja     1 284 300 zł   12,3 h   zapis 1      │
│    Autozapis         rok 4, 11 maja     1 190 050 zł   12,1 h   zapis 1      │
│    Gdynia            rok 1,  3 lutego      42 800 zł    1,8 h   zapis 1      │
│    Kraków            rok 9, 28 grudnia  8 412 900 zł   61,0 h   zapis 0  ⚠   │
│                                          starszy format — nie można wczytać  │
├──────────────────────────────────────────────────────────────────────────────┤
│  ziarno 0xC0FFEE · 8 km · rzeczny · 1990        [Usuń]         [Wczytaj]     │
└──────────────────────────────────────────────────────────────────────────────┘
```

Zapis niezgodny wersją jest **widoczny i opisany**, nie ukryty — inaczej gracz sądzi, że stracił
grę.

### 6.5 Ustawienia

Cztery zakładki, te same w menu i w pauzie: **Gra** (język, skala UI, autozapis, dziennik widoku do
zgłoszeń błędów), **Grafika** (rozdzielczość, tryb okna, synchronizacja pionowa, zasięg widzenia,
jakość cieni), **Dźwięk** (M11), **Sterowanie** (lista skrótów z podglądem). Język i skala UI
działają natychmiast, bez restartu — to jest kryterium akceptacyjne, nie życzenie.

### 6.6 Pauza i ekrany domknięcia

Menu pauzy to ta sama lista co menu główne minus „Nowa gra", plus „Wróć do gry". Wyjście
z niezapisanym postępem zawsze pyta i mówi, ile czasu gry przepadnie.

Ekran końca scenariusza rozlicza cele (osiągnięty / nieosiągnięty / o ile chybiony), ekran spuścizny
po śmierci bez dziedzica pokazuje kronikę dynastii. Oba mają wyjście do gry albo do menu —
**żaden ekran w tej grze nie jest ślepym zaułkiem.**

---

## 7. Dostępność i lokalizacja

Wymagania są twarde, bo wszystkie dają się sprawdzić testem:

- **Kontrast** tekstu do tła ≥ 4,5:1 (`text.primary` na `bg.panel` ≈ 12:1, `text.secondary` ≈ 6:1).
  Element interaktywny do tła ≥ 3:1.
- **Daltonizm:** zrzut w skali szarości pozostaje czytelny — kolor zawsze z drugim nośnikiem (§3.1).
- **Klawiatura:** każdy ekran przechodzi się Tabem, `Enter` zatwierdza, `Esc` cofa. Fokus jest
  zawsze widoczny.
- **Skala UI 0,75–3,0** bez przycięć i rozmyć; przyciąganie do pikseli fizycznych.
- **Pseudo-lokalizacja ×1,4** nie rozwala żadnego układu — napisy PL bywają o tyle dłuższe od EN.
- **Zbiory kluczy `pl` i `en` identyczne** (test CI łamie build przy braku).

---

## 8. Czego ten dokument nie ustala

- Treści paneli biznesowych — PRD §14.3 i M9e §5.9.
- Gramatyki edytora reguł — M9d §5.6.
- Wyglądu świata 3D (voxele, paleta dzielnic, oświetlenie) — PRD §15, M11.
- Ikon i dźwięku interfejsu — M11.
- Motywu jasnego i dodatkowych języków — M12; tokeny z §3 są tak pomyślane, żeby oba były zmianą
  danych, nie kodu.
