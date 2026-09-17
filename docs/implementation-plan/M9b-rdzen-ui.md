# M9b — Rdzeń UI

Podfaza 2 z 5 fazy **M9 — Gracz: kariera** (`M9-gracz-kariera.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M9a (pętla, powłoka sesji, `NewGameParams`). |
| **Pakiety robocze** | WP3, WP6, WP14 |
| **Projekt techniczny** | §5.8, §5.14 |
| **Wynik do pokazania** | Panel testowy: brak zmian danych → 0 alokacji i 0 ms przebudowy; tabela 100 tys. wierszy sortuje i filtruje poza klatką; **z menu głównego da się dojść do grającego świata bez wiersza poleceń**. |
| **Kryterium zamknięcia** | Kryteria WP3, WP6 i WP14; pseudo-lokalizacja ×1,4 nie rozwala układu. |
| **Poprzednia / następna** | `M9a-szkielet-gry-i-komendy.md` · `M9c-gracz-inspekcja-nakladki.md` |

Rdzeń `engine/ui`: drzewo retained, `measure/arrange/paint/event`, dirty-flagging przez `DataVersion`, dokowanie, DPI, atlas fontów, i18n PL/EN z pluralizacją — plus `Table<T>`, `Series` z piramidą mip i miniatura mapy cieplnej. Na wierzchu ekrany powłoki (menu, kreator świata, ładowanie, sloty, ustawienia) jako pierwszy prawdziwy klient tych widgetów i motyw `data/ui/theme.ron`.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Opis | Kryterium ukończenia |
|---|---|---|---|---|
| **WP3** | Rdzeń `engine/ui` | WP1 | `Widget`, drzewo retained, `measure/arrange/paint/event`, dirty-flagging przez `DataVersion`, `Layout` z dokowaniem, skalowanie DPI, atlas fontów, i18n PL/EN z pluralizacją, listy rysowania, **`Span`/`Rich` z polem `link`** i **`TabStrip`** | Panel testowy: brak zmian danych → 0 alokacji i 0 ms przebudowy; pseudo-lokalizacja ×1,4 nie rozwala układu; **`Rich` składa się w `String` bajt w bajt zgodny ze złotym wydrukiem sprzed zmiany — test porównujący starą i nową ścieżkę dla trzech istniejących kart** |
| **WP6** | `Table<T>`, wykresy, mapy cieplne mini | WP3 | Wirtualizowana tabela na 100k wierszy z sortowaniem/filtrem poza klatką, `Series` z piramidą mip (dzień/dekada/miesiąc/kwartał, kalendarz 12 × 30 wg K-1), miniatura mapy cieplnej | Budżety z §7 spełnione w criterion |
| **WP14** | Ekrany powłoki i motyw | WP3, WP13 (M9a) | Menu główne, kreator świata (`NewGameParams`), ekran generacji z postępem i anulowaniem, podgląd świata, lista slotów, ustawienia, menu pauzy; `data/ui/theme.ron` z tokenami z `docs/ui-design.md` | Świeża instalacja: od uruchomienia `magnat` **bez argumentów** do grającego świata w ≤ 6 interakcjach, wszystko klawiaturą; zmiana języka i `ui_scale` działa bez restartu; pseudo-lokalizacja ×1,4 i skale 0,75–3,0 nie rozwalają żadnego ekranu; test rysuje każdy ekran w CI bez GPU |

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
skalarnego nakładki, odświeżana `EveryHour`), `RuleEditorView`. **`TabStrip`** — rząd
zakładek nad treścią (≤ 7, stan wyboru trzymany przy `WidgetId`, więc przeżywa przebudowę
drzewa); żąda go karta inspekcji z `M9c` §5.7 i trzy istniejące karty (`ShopTab`, `SupplyTab`,
`FirmTab`), które dziś mają po własnej pętli `selectable_label` w `engine/ui/src/widgets.rs`.

#### Tekst, który da się kliknąć (`Span`)

```rust
pub struct Span { pub text: String, pub style: SpanStyle, pub link: Option<Subject> }
pub enum SpanStyle { Normal, Emphasis, Number, Link }
pub type Rich = Vec<Span>;
```

**To jest jedyna zmiana w M9b, która odwraca istniejącą decyzję, i dlatego stoi tutaj, a nie
w M9c.** Dziś `InspectorPanel::build` zwraca `String` (Z-3 niżej), a `render_tab` kart sklepu,
zakładu i firmy — też `String`. W `String` nie da się zakotwiczyć celu kliknięcia, więc
„klikalny odnośnik do podmiotu" z `docs/ui-design.md` §4 nie ma na czym stanąć. Podmiana
zwracanego typu na `Rich` **nie psuje złotego testu wydruku**, który był powodem tamtej decyzji:
`Rich` składa się z powrotem w `String` przez `spans.iter().map(|s| &s.text).collect()`, więc
test porównuje dokładnie ten sam napis co dziś, a link dochodzi obok niego, nie zamiast niego.
Konwersja jest jedną funkcją w `engine/ui` i to ona, a nie widget, jest wejściem złotego testu.

`Subject` w polu `link` pochodzi z `engine/core` (`K-62`), nie z `engine/ui` — inaczej
`DecisionReason` nie mógłby nieść celu odnośnika, a to on jest głównym źródłem linków
w karcie („wybrała »Dobry Koszyk«" musi wskazywać na ten zakład).

**DPI:** jedna skala `ui_scale ∈ [0,75; 3,0]`, wszystkie rozmiary w pikselach logicznych,
przyciąganie do siatki pikseli fizycznych przy rysowaniu, font rastrowany per skala do atlasu.

**i18n:** każdy napis to `LocKey` (interned `u32`); katalogi `data/locale/{pl,en}.ron`.
Pluralizacja: PL ma cztery formy (`one` / `few` / `many` / `other`), EN dwie (`one` / `other`) —
wybór formy z reguł CLDR, zaszyty jako funkcja czysta na `(locale, n)`. Formatowanie liczb,
pieniądza i dat per locale (PL: przecinek dziesiętny, spacja jako separator tysięcy, „zł" po
liczbie). Test CI: żadnego literału tekstowego w konstruktorach widgetów.

### 5.14 Ekrany powłoki i motyw (PRD §14.7)

Wygląd, układ i wymagania dostępności każdego z tych ekranów są w **`docs/ui-design.md`** §6 —
tu jest wyłącznie to, co z nich wynika dla kodu. Logika (`ShellScreen`, `NewGameParams`,
`WorldGenJob`, `SaveSlot`) należy do WP13 w `M9a`; WP14 jej nie powtarza, tylko rysuje.

**Dlaczego to jest tutaj, a nie w M9e razem z panelami.** Rdzeń UI potrzebuje pierwszego prawdziwego
konsumenta, a menu główne i kreator świata są najprostszym, jaki istnieje: kilkanaście kontrolek,
żadnych danych symulacji, żadnej wirtualizacji. Jeśli `Widget`, układ, fokus klawiatury, skala DPI
i i18n nie wystarczą do narysowania kreatora, to nie wystarczą też do panelu Finanse — a dowiemy się
o tym tydzień wcześniej i taniej. Zgodnie z regułą z §4 dokumentu fazy: widgety powstają
w kolejności, w jakiej żąda ich pierwszy ekran, który ich naprawdę potrzebuje.

```rust
pub struct Theme { pub colors: BTreeMap<TokenId, Rgba>, pub text: [TextStyle; 6], pub grid: u8 }
// data/ui/theme.ron — schema_version jak każdy plik w data/ (00 §5, K-19)
```

Motyw jest **danymi**, nie stałymi: tokeny z `docs/ui-design.md` §3 lądują w `data/ui/theme.ron`,
a `engine/ui` nie zna żadnego koloru z nazwy własnej. Powód jest praktyczny, nie estetyczny — motyw
jasny (M12) i tryb wysokiego kontrastu mają być zmianą pliku, a nie przeglądem dwudziestu paneli.
Trzy kolory stanu (`ok` / `warn` / `danger`) zastępują literały, które M3 ma dziś w `widgets.rs`.

Ekrany powłoki rysują się **bez snapshotu symulacji** — w menu głównym nie ma jeszcze świata.
To jedyne miejsce w UI, gdzie źródłem danych nie jest `&Snapshot`, więc granica z §5.2 nie jest
naruszona, tylko nieużywana: ekran czyta `&ShellModel` (sloty, ustawienia, draft kreatora), a ten
nie ma dostępu do `World`, bo świata jeszcze nie ma.

Trzy wymagania, które łatwo przeoczyć, a wszystkie są sprawdzalne:

1. **Ekran generacji odświeża się, gdy symulacja nie chodzi.** Pętla klatki z §5.2 zakłada tick
   symulacji; w `Generating` ticków nie ma, a pasek postępu i tak musi się ruszać. Stan ma własną,
   uproszczoną pętlę: odczyt `GenProgress` → `rebuild_dirty` → `draw`.
2. **Anulowanie działa z klawiatury i jest natychmiastowe w odbiorze** — flaga sprawdzana między
   passami, a ekran wraca do kreatora od razu po jej ustawieniu, nie po zakończeniu bieżącego passu.
3. **Miniatura podglądu świata to ta sama mapa co w podglądzie `headless`** — kolor z klasy wody
   i biomu klimatu, nie drugi renderer (`tools/magnat::mapa_dalekiego_terenu` liczy to już dziś).

---

## Zmiany wpisane po M3d

Zgodnie z `K-18`. To są rzeczy, o których M9 wie **na pewno** po zamknięciu M3d.

| # | Zmiana | Dlaczego |
|---|---|---|
| Z-1 ★ | **Biblioteka UI jest wybrana i wpięta: `egui` + `egui-wgpu`** (decyzja 9.2 M3). Warstwa siedzi w `engine/render::ui`, a widgety w `engine/ui::widgets`. M9 dokłada panele, nie integrację | Podział przebiega tak, że `engine/ui` **nie zna `wgpu` ani `winit`** — zna tylko `egui`, który jest czystym procesorem. Dzięki temu panel da się narysować w teście w CI bez karty graficznej (`egui::Context::run_ui` + zebranie kształtów tekstowych) i M9 ma zastać ten wzorzec, a nie panele testowalne wyłącznie okiem |
| Z-2 ★ | **Selekcja działa: `Renderer::pick(x, y) -> Option<u32>`** czyta bufor ID (decyzja 9.3). Odczyt pochodzi z **klatki poprzedniej** | To nie jest kompromis, tylko własność mechanizmu: czytanie GPU w chwili kliknięcia to `submit` + `map` + oczekiwanie, czyli zacięcie klatki na każdy klik. Dla M9 ma to konsekwencję na plus — podświetlenie encji pod kursorem jest już policzone i nie kosztuje nic więcej. Dziś w buforze są wyłącznie piesi; M9 dokłada firmy, pojazdy i sieci do **tego samego** przebiegu |
| Z-3 | **`InspectorPanel::build` zwraca tekst, a rysowanie stoi obok** (`widgets::citizen_card` nad `CitizenModel`) | Jedno źródło prawdy: złoty test wydruku broni tego, co widzi gracz (E-8). Panel M9 ma czytać model, a nie liczyć drugi raz — inaczej test przestaje cokolwiek gwarantować |
| Z-4 | **Zaznaczenie mieszkańca włącza bufor śledzenia** (`Trace::watch`, najwyżej ośmiu naraz — decyzja 9.16) | Bez tego karta pokazuje sam plan, bez realizacji. M9 musi o tym pamiętać przy każdym nowym sposobie otwierania karty (wyszukiwarka, lista, skok po relacji) |
| Z-5 ★ | **Nowy pakiet WP14 — ekrany powłoki i motyw** (§5.14), zależny od WP3 i od WP13 z `M9a`. Kryterium: od `magnat` bez argumentów do grającego świata w ≤ 6 interakcjach, wszystko klawiaturą | PRD §14.7 i decyzja właściciela produktu z 2026-09-14. Przy okazji rozwiązuje problem kolejności: WP3 dostaje pierwszego konsumenta, który nie wymaga snapshotu ani wirtualizacji, więc rdzeń UI da się sprawdzić, zanim powstanie pierwszy panel biznesowy |
| Z-6 | **Tokeny wyglądu są danymi w `data/ui/theme.ron`**, a język wizualny (paleta, typografia, siatka, komponenty, dostępność) mieszka w `docs/ui-design.md` — wiążąco dla każdej fazy dokładającej UI | Bez tego dwanaście faz dokładających panele wyprodukuje dwanaście wyglądów, a motyw jasny i tryb wysokiego kontrastu (M12) będą przeglądem wszystkich paneli zamiast podmianą pliku |

## Zmiany wpisane po decyzji właściciela produktu (2026-09-17)

Zgodnie z `K-18`. Gwiazdka = zmiana zakresu albo kryterium. Źródłem jest wymaganie produktowe:
każdy obiekt widoczny w świecie ma być klikalny, a każda informacja w karcie — odnośnikiem
do podmiotu, o którym mówi. Przegląd dwudziestu dokumentów planu pokazał, że trzy z pięciu
potrzebnych rzeczy nie mają właściciela.

| # | Zmiana | Dlaczego |
|---|---|---|
| Z-7 ★ | **`Span`/`Rich` wchodzą do WP3** (§5.8). `InspectorPanel::build` i `render_tab` kart przestają zwracać `String`, a zaczynają `Rich` | Z-3 poniżej ustalił `String` i to była słuszna decyzja przy jednym konsumencie — ale w `String` nie da się zakotwiczyć celu kliknięcia, więc „klikalny odnośnik" z `ui-design.md` §4 nie miał na czym stanąć. Złoty test nie traci nic: `Rich` składa się z powrotem w ten sam napis |
| Z-8 ★ | **`TabStrip` wchodzi do listy widgetów WP3** | Zakładki istnieją w kodzie trzy razy (`ShopTab`, `SupplyTab`, `FirmTab`), za każdym razem jako własna pętla `selectable_label`, a `widgets.rs:247` mówi wprost „osobnej abstrakcji zakładek nie ma i nie jest potrzebna". Przy czwartym konsumencie (karta inspekcji, `M9c` §5.7) to przestaje być prawdą — i to jest moment, w którym YAGNI każe abstrakcję zrobić, a nie wcześniej |
| Z-9 | **`Subject` musi mieszkać w `engine/core`, nie w `engine/ui`** (`K-62` w dokumencie 00) | `DecisionReason` jest w `core` i bez `#[non_exhaustive]` (`K-12`). Jeśli cel odnośnika ma jechać w ładunku powodu — a to jest główne źródło linków w karcie — to `Subject` musi być widoczny tam, gdzie powstaje powód, czyli w `sim/*`. Inaczej każda faza dopisująca wariant powodu musiałaby zależeć od `engine/ui` |

---

## Zmiany wpisane po M9a

Zgodnie z `K-18`. Pełne uzasadnienia — tabela `DA-n` w `M9a-szkielet-gry-i-komendy.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| DB-1 ★ | **WP3 buduje migawkę dla paneli od zera.** `Snapshot`, o którym §6 dokumentu fazy mówi „konsumuję od M0", **nie istnieje**: `magnat-sim-snapshot` niesie POD-y renderu (piesi, światła), a nie stan dla paneli. Szwem, w który migawka wejdzie, jest `game::CommandView` — funkcja walidująca komendę nie zagląda do `&World`, więc podmiana źródła jej nie dotknie | Przypadek (5) z `K-18`: pakiet obiecuje coś, czego żaden inny pakiet nie jest właścicielem. Bez tego wpisu WP3 zacząłby od szukania typu, którego nie ma |
| DB-2 | **Ekrany WP14 stoją na gotowych typach z `game::shell`:** `ShellScreen` (pięć wariantów), `NewGameParams` (czwarte pole `opts`), `WorldGenJob` (postęp 13 kroków, `cancel`, `join`), `WorldPreview` (miniatura 512×512 RGBA + pojemność miasta), `Settings` (język, `ui_scale`, zapis strumienia widoku, RON obok zapisu). `GameState` ma dziś cztery warianty — `CharacterSelect` dokłada `M9c` | Logika powłoki jest zamknięta i przetestowana bez GPU; WP14 dokłada rysowanie, a nie model |
| DB-3 | **Ekran slotów: nagłówek jest tani, wczytanie nie.** `save::list_slots` czyta wyłącznie `slot-N.meta.ron` (kilkaset bajtów), ale „Wczytaj" przewija dziennik wejść, więc kosztuje tyle, ile kosztowała rozgrywka. Migawka stanu należy do M12 (`DA-7`) | Ekran ma to **pokazać**, a nie udawać: pasek „odtwarzam dzień 128 z 360" jest prawdą, a kręciołek nie |
| DB-4 | `magnat_ui::Locale` jest od M9a `Serialize`/`Deserialize` — profil gracza to plik RON | Zmiana języka bez restartu (kryterium §7 fazy) wymaga, żeby język dało się zapisać poza światem |
