# M12f — Oprawa interfejsu

Podfaza 6 z 6 fazy **M12 — Skala i jakość** (`M12-skala-i-jakosc.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`. Kontrakt wyglądu: `docs/ui-design.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M9b (motyw, `Theme::apply`, ekrany powłoki), M9e/M10g (panele, dok), M11 (scena). **Nie zależy od M12a–M12e** — może iść przed nimi albo równolegle. |
| **Pakiety robocze** | WP15, WP16, WP17, WP18 |
| **Projekt techniczny** | §5.10 |
| **Wynik do pokazania** | Gra wygląda jak makiety wybrane w `docs/ui-inspiracje/`: HUD `hud-b.png`, panel `panel-dane-b.png`, menu `menu-glowne-b.png`, ustawienia `opcje-a.png`. Własny krój z polskimi znakami i strzałkami, ikony paneli i potrzeb, pasek ikon po lewej, menu główne na żywej scenie. |
| **Kryterium zamknięcia** | Kryteria WP15–WP18. Bramki 1–7 fazy M12 zamykają się dopiero po całej fazie. |
| **Poprzednia / następna** | `M12e-lokalizacja-i-domkniecie.md` · — (numerem ostatnia, kolejnością dowolna) |

Skąd ta podfaza: `docs/ui-design.md` §8 przypisywał ikony M11, a M11 był już zamknięty bez nich;
własny krój odsyłały do siebie nawzajem M9 i M11 (`M11-prezentacja.md:84`, TODO w
`engine/ui/src/theme.rs`). Trzy rzeczy nie miały wykonawcy. Makiety powstały 2026-09-22
(gpt-image-2, osiem obrazów, po dwa warianty na ekran); właściciel produktu wybrał warianty
wymienione wyżej. **Makiety są kierunkiem, nie specyfikacją** — tam, gdzie obraz łamie
`ui-design.md`, wygrywa dokument (lista różnic w §5.10.5).

---

## Pakiety robocze

### WP15 — Krój pisma [S]
Zależności: —

IBM Plex Sans (tekst), IBM Plex Sans Condensed SemiBold (wyłącznie `text.hero`), IBM Plex Mono
(liczby) — wszystkie OFL 1.1, pliki `.ttf` w `data/ui/fonts/` razem z `OFL.txt`. Ładowanie
w `Theme::apply` przez `egui::FontDefinitions` (bez nowej zależności — `ab_glyph` już jest).
Strzałki `↑↓←→` wracają do tekstów, z których wyciął je `DE-16` (M9b), i do nagłówka §6.0.

**Kryterium ukończenia:** test renderujący `ąćęłńóśźż ĄĆĘŁŃÓŚŹŻ ↑↓←→ − ×` w każdej z sześciu ról
tekstu nie trafia na glif zastępczy (sprawdza `Fonts::has_glyph`, nie oko); zrzut powłoki
w `headless` używa nowego kroju; strzałki przywrócone w `pl.ron` i `en.ron`.

### WP16 — Ikony [M]
Zależności: —

Ikony jako **dane**: `data/ui/icons.ron`, każda to siatka 16×16 znaków (`#` piksel, `.` pusto)
pod kluczem (`panel.shop`, `need.hunger`, `ui.gear`, …). `engine/ui` zamienia siatkę na
`ColorImage` biały-na-przezroczystym i barwi ją tokenem przy rysowaniu — ikona nie ma własnego
koloru (§3.1 `ui-design.md`: kolor niesie znaczenie). Styl pikselowy jest z wyboru: pasuje do
świata voxelowego i nie potrzebuje dekodera PNG ani SVG. Zestaw startowy: 12 paneli
(`PanelId`), 4 potrzeby z karty mieszkańca, pauza, zębatka, zamknij, gotówka, mieszkańcy.

**Kryterium ukończenia:** test `kazdy_panel_ma_ikone` (każdy wariant `PanelId` ma klucz
w `icons.ron`); test wczytania odrzuca siatkę o złym rozmiarze albo nieznanym znaku z nazwą
klucza w błędzie; zrzut w skali szarości (reguła daltonizmu §3.1) czytelny.

### WP17 — HUD i panele [M]
Zależności: WP15, WP16.

- **Pasek ikon** po lewej (48 px) z ikoną i podpisem `text.micro` pod spodem, dla wszystkich
  paneli w stałej kolejności `PanelId`. Zastępuje wiersz „Więcej paneli" z `layout.rs`;
  akordeon przypiętych paneli w doku zostaje bez zmian (`Layout` i jego zapis w profilu —
  `DM-4` — też). Aktywny panel: tło `bg.card` + pasek `accent` 3 px po lewej.
- **Nagłówek panelu**: ikona + tytuł `text.title` + pasek `accent` 3 px — ten sam komponent
  w doku i w oknie pływającym.
- **Pasek czasu**: prędkości jako kafelki 24 px z ikoną pauzy; gotówka i liczba mieszkańców
  jako „chipy" z ikoną na `bg.card`, po prawej.
- **Karta inspekcji**: ikona przy każdej potrzebie (drugi nośnik obok koloru). Portret —
  `D15` w §9 fazy.
- **Ścięte narożniki** (`shape.chamfer`, §5.10.3) na oknach pływających.

**Kryterium ukończenia:** złote testy wydruku paneli (M9b) przechodzą po aktualizacji, a różnica
jest wyłącznie wizualna (te same liczby z tych samych modeli); test `kontrast_spelnia_wcag`
zielony dla każdego nowego tokenu; dok przy `ui_scale` 0,75 i 3,0 bez przycięć.

### WP18 — Ekrany powłoki [M]
Zależności: WP15, WP16.

- **Menu główne** (`menu-glowne-b.png`): tło to żywa scena świata z wolno orbitującą kamerą
  (z ostatniego zapisu, a bez zapisu — z ustalonego ziarna demonstracyjnego), gradient
  `bg.window` od lewej krawędzi pod menu, tytuł `text.hero` Condensed, pozycje jako kafelki
  32 px z `shape.chamfer`. Kolejność i zawartość z `ui-design.md` §6.1 bez zmian: Kontynuuj
  z opisem zapisu, Tryb przeglądu, przełącznik PL/EN, wersja gry i format zapisu.
- **Ustawienia** (`opcje-a.png`): panel na prawej połowie ekranu, scena (albo tło menu) widoczna
  po lewej; zakładki podkreślone. Cztery zakładki z §6.5 (także Sterowanie, którego makieta nie
  ma). Ten sam układ z menu i z pauzy.
- Pozostałe ekrany powłoki (kreator, wczytaj, wybór postaci) dostają nowy nagłówek i kafelki,
  bez zmiany układu.

**Kryterium ukończenia:** każdy ekran przechodzi się Tabem i Esc jak wcześniej (testy M9b);
tło menu bez zapisu jest deterministyczne (ten sam hash świata demonstracyjnego); przełączenie
języka i skali UI w ustawieniach działa bez restartu.

---

## 5.10 Oprawa interfejsu — projekt

### 5.10.1 Gdzie co mieszka

| Rzecz | Plik | Właściciel po M12f |
|---|---|---|
| Kroje | `data/ui/fonts/*.ttf`, `OFL.txt` | `engine/ui::theme` |
| Ikony | `data/ui/icons.ron` (`schema_version`, `K-19`) | `engine/ui` (nowy moduł `icons`) |
| Nowe tokeny | `data/ui/theme.ron` | bez zmiany — jedyne miejsce koloru i kształtu |
| Pasek ikon, nagłówek | `engine/ui::widgets` | wspólny komponent, `ui-design.md` §4 |
| Tło menu | `game/src/screens/menu.rs` + klient | powłoka |

### 5.10.2 Krój
Plex, bo ma pełny Latin Extended-A, strzałki, minus typograficzny i odmianę Condensed w tej
samej rodzinie — `text.hero` nie wprowadza trzeciego kroju. Mono jest z tej samej rodziny, więc
liczby w tabelach mają tę samą wysokość x co tekst obok.

### 5.10.3 Nowe tokeny
`shape.chamfer: 8` (px logiczne, długość ściętej przyprostokątnej),
`rail.width: 48`, `accent.bar: 3`. Promień 3 px zostaje dla kontrolek; ścięcie tylko na oknach
pływających i kafelkach powłoki. `egui::Frame` nie umie ścięcia — rysuje je `Shape::convex_polygon`.

### 5.10.4 Czego nie robimy
- Portretu mieszkańca w karcie — wymaga renderowania modelu `.mvox` do tekstury (`D15`).
- Dźwięku interfejsu — bez wykonawcy (`D16`).
- Motywu jasnego — pozostaje w M12 jako podmiana danych; nowe tokeny muszą mieć sens w obu.

### 5.10.5 Gdzie makieta łamie kontrakt (wygrywa kontrakt)
- `panel-dane-b.png` pokazuje Finanse jako duże okno na środku. `ui-design.md` §5: panel
  pełnoekranowy tylko dla kroniki i edytora reguł. **Bierzemy wygląd, nie położenie.**
- `hud-b.png` pokazuje kartę mieszkańca pływającą nad sceną; jej miejsce opisuje §5 kontraktu.
- `hud-b.png` ma jednocześnie pasek ikon i siatkę „Więcej paneli" — siatka znika, zostaje pasek.
- `menu-glowne-b.png` nie ma trybu przeglądu, przełącznika języka ani formatu zapisu — zostają.
