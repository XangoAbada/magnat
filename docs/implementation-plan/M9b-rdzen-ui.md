# M9b — Rdzeń UI

Podfaza 2 z 5 fazy **M9 — Gracz: kariera** (`M9-gracz-kariera.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M9a (pętla). |
| **Pakiety robocze** | WP3, WP6 |
| **Projekt techniczny** | §5.8 |
| **Wynik do pokazania** | Panel testowy: brak zmian danych → 0 alokacji i 0 ms przebudowy; tabela 100 tys. wierszy sortuje i filtruje poza klatką. |
| **Kryterium zamknięcia** | Kryteria WP3 i WP6; pseudo-lokalizacja ×1,4 nie rozwala układu. |
| **Poprzednia / następna** | `M9a-szkielet-gry-i-komendy.md` · `M9c-gracz-inspekcja-nakladki.md` |

Rdzeń `engine/ui`: drzewo retained, `measure/arrange/paint/event`, dirty-flagging przez `DataVersion`, dokowanie, DPI, atlas fontów, i18n PL/EN z pluralizacją — plus `Table<T>`, `Series` z piramidą mip i miniatura mapy cieplnej.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Opis | Kryterium ukończenia |
|---|---|---|---|---|
| **WP3** | Rdzeń `engine/ui` | WP1 | `Widget`, drzewo retained, `measure/arrange/paint/event`, dirty-flagging przez `DataVersion`, `Layout` z dokowaniem, skalowanie DPI, atlas fontów, i18n PL/EN z pluralizacją, listy rysowania | Panel testowy: brak zmian danych → 0 alokacji i 0 ms przebudowy; pseudo-lokalizacja ×1,4 nie rozwala układu |
| **WP6** | `Table<T>`, wykresy, mapy cieplne mini | WP3 | Wirtualizowana tabela na 100k wierszy z sortowaniem/filtrem poza klatką, `Series` z piramidą mip (dzień/dekada/miesiąc/kwartał, kalendarz 12 × 30 wg K-1), miniatura mapy cieplnej | Budżety z §7 spełnione w criterion |

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.8 `engine/ui` — system UI (§16.4)

```rust
pub trait Widget {
    fn id(&self) -> WidgetId;                                  // stabilne: hierarchia + klucz danych
    fn deps(&self) -> &[DataSourceId];                         // co czytam — podstawa dirty-flagging
    fn measure(&mut self, ctx: &mut LayoutCtx, avail: Size) -> Size;
    fn arrange(&mut self, ctx: &mut LayoutCtx, rect: Rect);
    fn paint(&self, out: &mut DrawList);
    fn event(&mut self, ev: &InputEvent, out: &mut Vec<UiIntent>) -> EventFlow;
}
```

**Retained-mode z dirty-flagging.** Drzewo widgetów żyje między klatkami. Każdy widget deklaruje
`deps()`; po podmianie snapshotu `game/` podbija `DataVersion` tylko tych `DataSourceId`, które
faktycznie się zmieniły. Widget, którego zależności i wejście się nie zmieniły, **nie jest
dotykany** — zero pracy, zero alokacji. Deklaratywność (PRD §16.4 „drzewo budowane z danych")
realizujemy przez budowniczych paneli zwracających opis drzewa; różnicowanie jest po
`WidgetId`, więc stan widgetu (pozycja przewijania, zaznaczenie, rozwinięcie) przeżywa przebudowę.

```rust
pub enum LayoutNode {
    Split { axis: Axis, ratio: u16, a: Box<LayoutNode>, b: Box<LayoutNode> },
    Tabs(Vec<PanelId>),
    Panel(PanelId),
    Float { rect: Rect, panel: PanelId },
}
pub struct Layout { pub root: LayoutNode, pub docks: [Option<LayoutNode>; 4], pub name: String }
```
Układ jest preferencją widoku — zapisywany w profilu gracza, nie w zapisie świata (ale
`ViewCommand::SetLayout` idzie do strumienia widoku replayu).

```rust
pub struct Table<T: Row> {
    source:  Arc<dyn RowSource<T>>,   // widok na SoA snapshotu — brak kopiowania danych
    order:   Vec<u32>,                // permutacja po sort+filtr, liczona poza klatką
    order_version: u64,
    columns: Vec<Column<T>>,
    sort:    Option<(ColumnId, SortDir)>,
    filter:  ConditionExpr,           // ten sam AST co reguły
    viewport: Range<u32>,             // wirtualizacja
    selection: IndexSet,
}
```

Trzy razy ten sam AST, i to jest świadome: **warunki reguł, filtry tabel, warunki celów,
warunki „zatrzymaj przy zdarzeniu X" i filtry nakładek to jedna gramatyka**. Gracz uczy się
jednego mechanizmu, a my utrzymujemy jeden walidator i jeden ewaluator.

- Sortowanie 100k wierszy: wyciągnięcie tablicy kluczy `u64` i `pdqsort` w `engine/jobs`,
  poza klatką; tabela pokazuje poprzedni porządek do czasu gotowości. Przewijanie nigdy nie
  sortuje ponownie.
- Filtrowanie: przyrostowe, wynik jako bitset, przeliczane przy zmianie `DataVersion` źródła.
- Wysokość wiersza stała w obrębie tabeli → matematyka przewijania O(1).

```rust
pub struct Series { pub key: SeriesKey, pub mips: [Vec<Sample>; 4] }  // dzień, dekada, miesiąc, kwartał
pub struct Sample { pub min: i64, pub max: i64, pub sum: i64, pub n: u32 }
```
**Kalendarz: 360 dni, 12 × 30 (doc 00, K-1)** — i to jest powód, dla którego poziomy piramidy to
dzień / dekada (10 dni) / miesiąc (30) / kwartał (90), a nie tydzień: przy 12 × 30 każdy poziom
dzieli się bez reszty (30 = 3 × 10, 90 = 3 × 30, 360 = 4 × 90). Agregacja jest wtedy dokładna,
a oś czasu wykresu ma równe podziałki miesięczne bez dryfu — to samo dotyczy skali czasu
w `GanttView` dostaw i osi doby w trybie „śledź".

Wykres 10 lat danych dziennych = 3600 próbek na serię; przy 8 seriach ~29k próbek. Rysujemy
z poziomu mip dobranego do szerokości w pikselach — **nigdy więcej niż ~2000 odcinków**,
niezależnie od zakresu. `MetricsRecorder` (`EveryDay`, w `game/`, nie w `sim/`) zapisuje serie;
10 lat × 8 serii × 16 B ≈ 460 kB na poziomie dziennym (+ ~15% na pozostałe poziomy) — idzie do
zapisu gry, bo historia wykresów musi przeżyć wczytanie (i bo z niej działa dry-run polityk).

Formatowanie dat (`CalendarFmt` w `engine/ui`) zna wyłącznie kalendarz 12 × 30 — nie ma w nim
lat przestępnych ani miesięcy o różnej długości, więc arytmetyka osi czasu jest całkowitoliczbowa.

Pozostałe widgety: `GraphView` (układ warstwowy Sugiyama — łańcuch dostaw to przepływ, więc
warstwy są naturalne; liczony w jobie, cache'owany po hashu topologii), `GanttView` (wiersze =
maszyny/pojazdy, wirtualizacja po oknie czasu), `HeatmapThumb` (tekstura 256×256 z pola
skalarnego nakładki, odświeżana `EveryHour`), `RuleEditorView`.

**DPI:** jedna skala `ui_scale ∈ [0,75; 3,0]`, wszystkie rozmiary w pikselach logicznych,
przyciąganie do siatki pikseli fizycznych przy rysowaniu, font rastrowany per skala do atlasu.

**i18n:** każdy napis to `LocKey` (interned `u32`); katalogi `data/locale/{pl,en}.ron`.
Pluralizacja: PL ma cztery formy (`one` / `few` / `many` / `other`), EN dwie (`one` / `other`) —
wybór formy z reguł CLDR, zaszyty jako funkcja czysta na `(locale, n)`. Formatowanie liczb,
pieniądza i dat per locale (PL: przecinek dziesiętny, spacja jako separator tysięcy, „zł" po
liczbie). Test CI: żadnego literału tekstowego w konstruktorach widgetów.

---

## Zmiany wpisane po M3d

Zgodnie z `K-18`. To są rzeczy, o których M9 wie **na pewno** po zamknięciu M3d.

| # | Zmiana | Dlaczego |
|---|---|---|
| Z-1 ★ | **Biblioteka UI jest wybrana i wpięta: `egui` + `egui-wgpu`** (decyzja 9.2 M3). Warstwa siedzi w `engine/render::ui`, a widgety w `engine/ui::widgets`. M9 dokłada panele, nie integrację | Podział przebiega tak, że `engine/ui` **nie zna `wgpu` ani `winit`** — zna tylko `egui`, który jest czystym procesorem. Dzięki temu panel da się narysować w teście w CI bez karty graficznej (`egui::Context::run_ui` + zebranie kształtów tekstowych) i M9 ma zastać ten wzorzec, a nie panele testowalne wyłącznie okiem |
| Z-2 ★ | **Selekcja działa: `Renderer::pick(x, y) -> Option<u32>`** czyta bufor ID (decyzja 9.3). Odczyt pochodzi z **klatki poprzedniej** | To nie jest kompromis, tylko własność mechanizmu: czytanie GPU w chwili kliknięcia to `submit` + `map` + oczekiwanie, czyli zacięcie klatki na każdy klik. Dla M9 ma to konsekwencję na plus — podświetlenie encji pod kursorem jest już policzone i nie kosztuje nic więcej. Dziś w buforze są wyłącznie piesi; M9 dokłada firmy, pojazdy i sieci do **tego samego** przebiegu |
| Z-3 | **`InspectorPanel::build` zwraca tekst, a rysowanie stoi obok** (`widgets::citizen_card` nad `CitizenModel`) | Jedno źródło prawdy: złoty test wydruku broni tego, co widzi gracz (E-8). Panel M9 ma czytać model, a nie liczyć drugi raz — inaczej test przestaje cokolwiek gwarantować |
| Z-4 | **Zaznaczenie mieszkańca włącza bufor śledzenia** (`Trace::watch`, najwyżej ośmiu naraz — decyzja 9.16) | Bez tego karta pokazuje sam plan, bez realizacji. M9 musi o tym pamiętać przy każdym nowym sposobie otwierania karty (wyszukiwarka, lista, skok po relacji) |
