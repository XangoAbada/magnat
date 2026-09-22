# E9 — Oprawa interfejsu

**Po co.** Interfejs wygląda jak domyślne `egui`: brak własnego kroju, brak ikon, płaskie
przyciski, dok bez charakteru. Trzy rzeczy nie mają wykonawcy od M11: `docs/ui-design.md` §8
przypisywał ikony i dźwięk UI fazie M11, która zamknęła się bez nich, a krój odsyłały do siebie
nawzajem M9 i M11 (`M11-prezentacja.md:84`, komentarz w `engine/ui/src/theme.rs:308-318`).
Skutek widać w grze: strzałki `↑↓←→` wycięto z tekstów (`DE-16` w M9b), bo domyślny atlas
rysuje je jako prostokąty.

Etap nie pochodzi z audytu. Powstał z makiet z 22 września 2026 (`docs/ui-inspiracje/`,
gpt-image-2, dwa warianty każdego ekranu) i był krótko podfazą M12f planu implementacyjnego.
Trafił tutaj, bo plan implementacyjny stoi do zamknięcia E8, a ta praca nie dotyka niczego,
co naprawiają E2–E7.

**Kierunek.** Właściciel produktu wybrał: HUD `hud-b.png`, panel `panel-dane-b.png`, menu
`menu-glowne-b.png`, ustawienia `opcje-a.png`. Kontrakt `docs/ui-design.md` zostaje: ciemny
motyw, gęstość arkusza, kolor tylko ze znaczeniem. **Makieta to kierunek, nie specyfikacja.**
Tam, gdzie łamie kontrakt, wygrywa kontrakt (lista niżej).

**Wejście.** E1 zamknięty, bo złote testy wydruku paneli i `kontrast_spelnia_wcag` muszą
biec w CI, zanim cokolwiek o nich udowodnią. Poza tym **E9 nie zależy od E2–E8** i idzie
równolegle z nimi. Jedna kolizja: `N3.9` dotyka `screens/newgame.rs` i `screens/character.rs`,
a `N9.4` tych samych ekranów — `N3.9` idzie pierwszy.

**Pomiar zamknięcia.** Zrzuty `headless` czterech ekranów (HUD, Finanse, menu, ustawienia)
położone obok wybranych makiet w `docs/ui-inspiracje/po-e9/`. Test glifów, `kazdy_panel_ma_ikone`,
`kontrast_spelnia_wcag` i złote testy paneli zielone w CI. Liczba plików i linii zmienionych
w `engine/ui` i `game/src/screens` wpisana tutaj.

## Gdzie makieta łamie kontrakt

- `panel-dane-b.png` pokazuje Finanse jako duże okno na środku. `ui-design.md` §5 pozwala na
  panel pełnoekranowy tylko dla kroniki i edytora reguł. **Bierzemy wygląd, nie położenie.**
- `hud-b.png` pokazuje kartę mieszkańca pływającą nad sceną; jej miejsce opisuje §5 kontraktu.
- `hud-b.png` ma jednocześnie pasek ikon i siatkę „Więcej paneli”. Siatka znika, zostaje pasek.
- `menu-glowne-b.png` nie ma trybu przeglądu, przełącznika języka ani formatu zapisu. Te
  elementy zostają (§6.1 kontraktu).

## Punkty

- [ ] **N9.1** krój pisma — `nowe`, `ui-design.md` §3.2
  - IBM Plex Sans (tekst), IBM Plex Mono (liczby), IBM Plex Sans Condensed SemiBold wyłącznie
    dla `text.hero`. Licencja OFL 1.1, pliki `.ttf` i `OFL.txt` w `data/ui/fonts/`.
  - *Naprawa:* ładowanie w `Theme::apply` przez `egui::FontDefinitions`. Nowa zależność nie
    jest potrzebna, bo `ab_glyph` już jest w drzewie. Strzałki wracają do `pl.ron` i `en.ron`
    oraz do nagłówka §6.0.
  - *Test:* `Fonts::has_glyph` dla `ąćęłńóśźż ĄĆĘŁŃÓŚŹŻ ↑↓←→ − ×` w każdej z sześciu ról
    tekstu. Na kodzie sprzed naprawy pada na strzałkach.

- [ ] **N9.2** ikony jako dane — `nowe`, `ui-design.md` §3.4
  - `data/ui/icons.ron` ze `schema_version` (katalog `data/ui/`, `K-19`). Każda ikona to
    siatka 16×16 znaków (`#` to piksel, `.` to puste pole) pod kluczem: `panel.shop`,
    `need.hunger`, `ui.gear` i tak dalej. Zestaw startowy obejmuje 12 paneli (`PanelId`),
    4 potrzeby z karty mieszkańca oraz ikony pauzy, zębatki, zamknięcia, gotówki i mieszkańców.
  - *Naprawa:* nowy moduł `engine/ui::icons`. Zamienia siatkę na biały `ColorImage`
    na przezroczystym tle, a barwi ją tokenem przy rysowaniu. Ikona nie ma własnego koloru.
    Styl pikselowy jest wybrany celowo: pasuje do świata voxelowego i nie potrzebuje dekodera
    PNG ani SVG.
  - *Test:* `kazdy_panel_ma_ikone` (każdy wariant `PanelId` ma klucz); wczytanie odrzuca siatkę
    o złym rozmiarze albo nieznanym znaku i podaje w błędzie nazwę klucza.

- [ ] **N9.3** HUD i panele — `nowe`, makiety `hud-b`, `panel-dane-b`
  - **Pasek ikon.** Stoi po lewej, ma 48 px (`rail.width`). Pod każdą ikoną jest podpis
    `text.micro`. Pokazuje wszystkie panele w stałej kolejności `PanelId` i zastępuje wiersz
    „Więcej paneli” z `game/src/panels/layout.rs`. Akordeon przypiętych paneli w doku zostaje,
    a `Layout` dalej zapisuje się w profilu gracza (`DM-4`). Aktywny panel ma tło `bg.card`
    i pasek `accent` o szerokości 3 px (`accent.bar`).
  - **Nagłówek panelu.** Ikona, tytuł `text.title` i pasek akcentu. Wygląda tak samo w doku
    i w oknie pływającym.
  - **Pasek czasu.** Prędkości to kafelki 24 px z ikoną pauzy. Po prawej stronie gotówka
    i liczba mieszkańców jako „chipy” z ikoną.
  - **Karta inspekcji.** Przy każdej potrzebie stoi ikona jako drugi nośnik informacji obok
    koloru.
  - **Ścięty narożnik** (`shape.chamfer: 8`) mają tylko okna pływające. `egui::Frame` nie umie
    ścinać narożników, więc rysuje je `Shape::convex_polygon`.
  - *Test:* złote testy wydruku paneli (M9b) po aktualizacji różnią się tylko wyglądem, liczby
    z modeli zostają te same. `kontrast_spelnia_wcag` przechodzi dla każdego nowego tokenu.
    Dok przy `ui_scale` 0,75 i 3,0 nie ma przycięć.

- [ ] **N9.4** ekrany powłoki — `nowe`, makiety `menu-glowne-b`, `opcje-a`
  - **Menu główne.** Tło to żywa scena z wolno krążącą kamerą. Scena pochodzi z ostatniego
    zapisu, a bez zapisu ze stałego świata demonstracyjnego. Od lewej krawędzi biegnie gradient
    `bg.window`. Tytuł `text.hero` jest w kroju Condensed. Pozycje menu to kafelki 32 px
    ze ściętym narożnikiem. Zawartość i kolejność zostają z §6.1 kontraktu.
  - **Ustawienia.** Panel zajmuje prawą połowę ekranu, a po lewej widać scenę albo tło menu.
    Zakładki są podkreślone. Zakładki są te, które istnieją: Gra, Sterowanie (którego makieta
    nie ma) i Dźwięk, jeśli `N10.2` już się zamknął. Grafika nie ma treści, więc jej nie ma. Układ jest ten sam z menu i z pauzy.
  - Kreator, wczytywanie i wybór postaci dostają nowy nagłówek i kafelki. Ich układ się
    nie zmienia.
  - *Test:* testy nawigacji M9b (Tab, Esc) przechodzą. Tło menu bez zapisu jest
    deterministyczne, czyli hash świata demonstracyjnego jest stały. Język i skala UI zmieniają
    się bez restartu.

- [ ] **N9.5** portret mieszkańca — `nowe`, makieta `hud-b`
  - **Decyzja N9.5-a:** *Domyślnie:* portretu nie robimy w E9. Prawdziwy portret to model
    `.mvox` wyrenderowany do tekstury, a takiej ścieżki w rendererze nie ma. Punkt dostaje
    `[-]` i wraca, gdy renderer zyska render do tekstury z innego powodu, na przykład
    do miniatur zapisów.

- [-] **N9.6** dźwięk interfejsu — `nowe`, `ui-design.md` §8
  - Przeniesiony do `N10.7`. Założenie „gra nie ma miksera” było nieprawdziwe: mikser jest
    od M11d (`engine/audio`, magistrala `Bus::Ui`), brakuje tylko próbek i wywołań.

## Znalezione po drodze
