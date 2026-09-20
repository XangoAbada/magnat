# M10b — Marka i media

Podfaza 2 z 6 fazy **M10 — Głębia** (`M10-glebia.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M3 (pamięć mieszkańca), M5 (transakcje), M7 (firmy). |
| **Pakiety robocze** | WP10.5, WP10.6, WP10.7 |
| **Projekt techniczny** | §5.1, §5.2, §5.3 |
| **Wynik do pokazania** | Marka rośnie z faktycznych doświadczeń zakupowych, a kampania przesuwa udział w rynku w mierzalny sposób. |
| **Kryterium zamknięcia** | Kryteria WP10.5–WP10.7. |
| **Poprzednia / następna** | `M10a-jadro-makro-historia.md` · `M10c-rd-i-nowe-produkty.md` |

`BrandAffinity` w pamięci mieszkańców, kampanie reklamowe z zasięgiem i kanałami, media jako zwykłe firmy sprzedające zasięg.

---

## Pakiety robocze

### WP10.5 `[x]` — Marka: `BrandAffinity` w pamięci mieszkańców

**Zależności:** M3 (magazyn doświadczeń), M5 (decyzja zakupowa), M7 (firma ma produkt).

Sloty marek w zimnym magazynie doświadczeń, leniwy zanik, aktualizacja z doświadczeń, podpięcie
pod `purchase_score` (składniki `w_marka` i `jakość_postrzegana` przestają być stałymi).
Rachunek pamięci: §5.1.

**Kryterium ukończenia:**
- Test asymetrii: kampania podnosząca oczekiwaną jakość o +20 Q wymaga ≥ 7 ekspozycji; jedno
  rozczarowanie o −20 Q kosztuje ≥ 14 pkt afinitetu; odbudowa wymaga ≥ 3 pozytywnych doświadczeń
  o tej samej sile. To jest test jednostkowy na liczbach, nie „obserwacja w symulacji".
- Pamięć: 400 tys. mieszkańców × 16 slotów ≤ 55 MB mierzone (nie szacowane).
- Zanik leniwy: brak systemu iterującego po slotach częściej niż `EveryMonth`.

**Rozmiar: M.**

---

### WP10.6 `[x]` — Kanały i kampanie (`AdCampaign`, `Reach`)

**Zależności:** WP10.5, M4 (zdarzenia przejazdu po krawędziach), M2 (indeks przestrzenny).

Osiem kanałów z §5.2. Kluczowa decyzja projektowa: **`Reach` nie jest zbiorem, tylko strumieniem
zdarzeń ekspozycji**. Nie budujemy indeksu „krawędź → mieszkańcy, którzy nią jeżdżą" (400 tys. × ~60
krawędzi to 96 MB indeksu, który trzeba utrzymywać przy każdej zmianie trasy). Zamiast tego billboard
podpina się pod istniejący strumień przejazdów mezo z M4 i próbkuje go deterministycznie.

**Kryterium ukończenia:**
- Billboard przy krawędzi o dobowym potoku 8000 przejazdów i `notice_rate = 12%` dostarcza
  960 ± 30 ekspozycji dziennie, powtarzalnie dla seeda.
- Ekspozycje trafiają do **mieszkańców faktycznie tamtędy jeżdżących** — test: przeniesienie
  billboardu na krawędź w innej dzielnicy zmienia rozkład dzielnic odbiorców zgodnie z macierzą dojazdów.
- Zero nowych indeksów przestrzennych — audyt kodu.
- Koszt kampanii księgowany przez `ledger_post`, nie bezpośrednio.

**Rozmiar: L.**

---

### WP10.7 `[x]` — Media jako firmy

**Zależności:** WP10.6, M3 (graf relacji i plotka), M8 (strumień zdarzeń).
**Kryterium ukończenia:** gazeta o czytelnictwie 35% w Śródmieściu i 8% na Wzgórzach publikuje tekst
o strajku; po 3 dniach gry znajomość zdarzenia wśród mieszkańców Śródmieścia > 40%, na Wzgórzach < 15%,
a rozkład jest zgodny z czytelnictwem ± 5 pkt. Wiarygodność tytułu moduluje siłę aktualizacji opinii.
**Rozmiar: M.**

---

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.1 Marka — struktury i rachunek pamięci

**Problem.** Marka ma być zbiorem afinitetów w pamięci konkretnych mieszkańców (§7.6). Naiwna
implementacja to macierz `mieszkańcy × marki`:

| Wariant | Rachunek | Wynik | Udział w budżecie 6 GB (§17.7) |
|---|---|---|---|
| A — macierz gęsta | 400 000 × 4096 marek × 2 B | **3,28 GB** | 55% — odrzucony |
| B — mapa rzadka per mieszkaniec (`BTreeMap`) | 400 000 × ~14 wpisów × (8 B danych + ~40 B węzła) | ~270 MB + fragmentacja | 4,5% — odrzucony (alokacje, brak lokalności) |
| **C — tablica K slotów o stałym rozmiarze** | **400 000 × 16 × 8 B** | **51,2 MB** | **0,85% — przyjęty** |

**Wariant C — układ pola:**

```rust
/// Jeden slot pamięci marki. Dokładnie 8 B, bez wyrównania, SoA-friendly.
#[repr(C)]
pub struct BrandAffinity {
    pub brand: BrandId,          // u16 — marka (firma może mieć do 4 marek)
    pub affinity: i8,            // -100..=100 — sympatia; to jest „marka"
    pub expected_quality: Q,     // u8 0..=100 — czego się spodziewam
    pub awareness: u8,           // 0..=100 — jak mocno ją znam (§5.7)
    pub last_touch_day: u16,     // dzień gry; bazuje na epoce świata, ~179 lat zakresu
    pub source: TouchSource,     // u8: Experience | Ad | Rumor | Media | Owned — dla wyjaśnialności
}

pub const BRAND_SLOTS: usize = 16;
pub struct BrandSlots(pub [BrandAffinity; BRAND_SLOTS]); // 128 B
```

**Gdzie to mieszka.** §17.7 daje mieszkańcowi ~400 B stanu gorącego plus **osobny, kompresowany
magazyn doświadczeń (ostatnie 32 wpisy)**. `BrandSlots` idzie do tego drugiego, nie do stanu gorącego —
bo jest czytany przy decyzji zakupowej i ekspozycji (kilkanaście razy na dobę gry na mieszkańca,
przez DES), a nie co tick. Stan gorący pozostaje 400 B. Dostęp: ten sam mechanizm stronicowania,
który M3 zbudował dla doświadczeń; M10 dokłada tylko drugą sekcję rekordu.

**Sumarycznie:**

```
sloty marek:              400 000 × 128 B          =  51,2 MB
prior dzielnicowy:        40 dzielnic × 4096 × 1 B =   0,16 MB   (fallback „coś słyszałem")
agregat „siła marki":     4096 marek × 16 B        =   0,07 MB   (tylko dla UI, EveryDay)
indeks kampanii:          ≤ 2000 kampanii × 64 B   =   0,13 MB
──────────────────────────────────────────────────────────────
razem                                              ≈  51,6 MB  =  0,86% budżetu 6 GB
```

**Wypieranie slotów.** Przy 17. marce wypadamy slot o najmniejszej istotności:
`salience = |affinity| × 4 + awareness`, przy remisie niższy `BrandId` (determinizm). Marka
pracodawcy mieszkańca i marka sklepu, w którym był ≥ 8 razy, są przypięte (`source: Owned`)
i nie podlegają wypieraniu — bez tego lojalność z §5.1 znika w szumie reklamowym.

**Zanik jest leniwy.** Nie ma systemu iterującego po 6,4 mln slotów. Przy odczycie liczymy
`decay(days_since_touch)` z tablicy stałych (wyszukanie, nie `exp()`), afinitet i znajomość
zbiegają do zera i do priora dzielnicowego. Jedyny przebieg iterujący to `EveryMonth` kompaktowanie
slotów wygasłych do zera — i on tylko zwalnia miejsce.
> *ponytail: zanik liczony przy odczycie, O(1). Gdyby profil kiedyś pokazał, że odczyty dominują,
> przenieść na przebieg wsadowy `EveryWeek` po chunkach — ale nie wcześniej.*

**Aktualizacja — asymetria wymagana przez §7.6.** Arytmetyka całkowita, stałe w 1/256:

```rust
// Ekspozycja reklamowa (kanał c, przekaz twierdzi jakość `claim`)
awareness  = min(100, awareness + AWARENESS_GAIN[c]);              // 1..12 pkt
expected_quality += ((claim - expected_quality) * AD_LEARN) >> 8;  // AD_LEARN = 24  (~9%)
// afinitet NIE zmienia się od samej reklamy — reklama tworzy oczekiwanie, nie sympatię

// Doświadczenie (zakup, faktyczna jakość `actual`)
let d = actual as i16 - expected_quality as i16;
affinity += if d >= 0 { (K_UP   * d) >> 8 }     // K_UP   =  64  (0,25)
            else       { (K_DOWN * d) >> 8 };   // K_DOWN = 192  (0,75) → asymetria 3:1
expected_quality += ((actual - expected_quality) * EXP_LEARN) >> 8; // EXP_LEARN = 96 (~37%)
```

Konsekwencja liczbowa (to jest treść testu z WP10.5): podniesienie oczekiwań o +20 Q wymaga
~8 ekspozycji; jedno rozczarowanie o −20 Q zabiera 15 pkt afinitetu; odbudowa tych 15 pkt wymaga
trzech doświadczeń o tej samej sile in plus. **Nadmiarowa obietnica w reklamie jest samokarząca** —
podnosi `expected_quality`, a więc powiększa `d < 0` przy każdym kolejnym zakupie. Gracz, który
reklamuje tandetę, niszczy sobie markę własną kampanią, i widzi to w karcie mieszkańca.

**Plotka (§5.7).** Przekaz przez graf relacji nie kopiuje afinitetu, tylko przybliża:

```
odbiorca.affinity += (nadawca.affinity - odbiorca.affinity) * waga_relacji * wiarygodność / 65536
odbiorca.awareness = max(odbiorca.awareness, nadawca.awareness / 2)
```
Plotka nigdy nie ustawia `expected_quality` powyżej wartości nadawcy — nie da się „nakręcić"
oczekiwań pętlą plotek.

### 5.2 Kampanie i zasięg

```rust
pub struct AdCampaign {
    pub id: CampaignId,
    pub advertiser: FirmId,
    pub brand: BrandId,
    pub channel: AdChannel,
    pub claim: Q,                 // deklarowana jakość — podnosi expected_quality odbiorców
    pub budget: Money,            // i64, grosze
    pub spent: Money,
    pub window: (SimMinute, SimMinute),
    pub audience: AudienceFilter, // filtr (dzielnice, klasy, wiek) — NIE lista odbiorców
    pub metrics: CampaignMetrics, // ekspozycje, przyrost znajomości, sprzedaż przypisana — dla UI M9
}

pub enum AdChannel {
    Billboard   { edge: EdgeId, side: u8, notice_rate_bps: u16 },
    Press       { outlet: FirmId, slot: AdSlot },
    Radio       { outlet: FirmId, daypart: Daypart },
    Tv          { outlet: FirmId, daypart: Daypart },
    Leaflet     { origin: SiteId, radius_m: u16 },
    InStorePromo{ site: SiteId, discount_bps: u16 },
    Sponsorship { event: EventId },
    Pr          { topic: PrTopic },     // reakcja na zdarzenie — wchodzi w plotkę, nie w ekspozycję
}

/// Zasięg jest PLANEM i POMIAREM, nie zbiorem odbiorców.
pub struct Reach {
    pub exposures_planned_daily: u32,
    pub exposures_actual_daily: u32,
    pub cost_per_mille: Money,
}
```

**Skąd bierze się zasięg billboardu (§7.6: „zasięg to mieszkańcy, którzy tamtędy jeżdżą").**
M4 w mezo już produkuje przejazdy po krawędziach grafu. System reklamy **subskrybuje ten strumień**
i próbkuje go rezerwuarowo:

```rust
// EveryHour, tylko dla krawędzi z aktywnym billboardem (typowo < 200 krawędzi)
for traversal in traffic.edge_traversals(edge) {
    if rng(seed, StreamId::AdNotice, campaign.index(), tick).next_bps() < notice_rate_bps {
        expose(traversal.citizen, campaign);
    }
}
```

Żadnego indeksu „krawędź → mieszkańcy". Żadnego utrzymywania go przy przeplanowaniu tras.
Koszt: O(przejazdy po krawędziach z billboardami), czyli ~200 krawędzi × ~500 przejazdów/h = 100 tys.
testów na godzinę gry — poniżej progu zauważalności.
> *ponytail: próbkowanie strumienia zamiast indeksu odwrotnego. Sufit: przy > 2000 billboardów
> w mieście koszt rośnie liniowo — wtedy agregować per krawędź (liczba ekspozycji zamiast listy)
> i losować odbiorców z potoku. Nie wcześniej.*

Prasa/radio/TV: `MediaOutlet.readership[district]` daje prawdopodobieństwo kontaktu; próbkujemy
mieszkańców dzielnicy bez materializowania listy (odwrotna dystrybuanta po kohorcie).
Ulotki: indeks przestrzenny z M2 (`engine/spatial`), promień, bez nowych struktur.
Promocje w sklepie: ekspozycja przy transakcji — darmowa, bo mieszkaniec i tak tam jest.
Sponsoring: uczestnicy zdarzenia z M8.
PR: nie tworzy ekspozycji — wstrzykuje `Rumor` do grafu relacji z §5.7.

### 5.3 Media

```rust
pub struct MediaOutlet {
    pub firm: FirmId,
    pub kind: MediaKind,                   // Gazeta | Radio | Tv | Portal
    pub readership: Vec<(DistrictId, Q)>,  // udział czytelnictwa per dzielnica
    pub credibility: Q,                    // moduluje siłę aktualizacji opinii u odbiorcy
    pub bias: EditorialBias,               // prorynkowy | prospołeczny | sensacyjny | lokalny
    pub inventory_daily: u16,              // slotów reklamowych na dobę
    pub slot_price: Money,
}
```

Media są **firmami z M7** z dodatkowym komponentem. Sprzedaż powierzchni idzie przez istniejący
mechanizm ofert z M5/M6 (`OfferId`) — nie budujemy drugiego rynku. Redakcja co dobę wybiera
`n = 3..8` zdarzeń ze strumienia M8 wg wartości informacyjnej (skala zdarzenia × zgodność z `bias`)
i publikuje `Story`, która wchodzi do grafu plotki z fan-outem ważonym czytelnictwem.

Odbiorca aktualizuje opinię proporcjonalnie do `credibility × jego zaufanie do tytułu`. Tytuł, który
publikował rzeczy niezgodne z obserwacją odbiorcy, traci u niego zaufanie — wiarygodność jest
per-para, tak jak marka. Reużywamy do tego ten sam slot `BrandAffinity` (tytuł ma `BrandId`) —
zero nowych struktur.

---

## Zmiany wpisane po M3c

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M3c.

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **Świadomość marki stoi na gotowym mechanizmie: `social::awareness_of(world, target)` zwraca „ilu wie z ilu", a `social::learn_place` jest jedynym wejściem wiedzy** — używają go plotka (§5.8), widoczność z trasy i zasiew Etapu 8. Reklama to **czwarte źródło tego samego wpisu**: `KnowledgeKind::Ad` istnieje w `store::KnowledgeKind` od M3a i nie wymaga nowej struktury | Jedno wejście znaczy jeden limit 32 wpisów na mieszkańca i jedna reguła wypychania najsłabszego (ocena × świeżość). Kampania reklamowa, która dopisywałaby wiedzę obok, obchodziłaby limit pamięci z §17.7 i psuła rangowanie |
| ★ | **Zasięg plotki jest skalibrowany i zmierzony w M3c**: nowe miejsce zna 89 % mieszkańców w promieniu kilometra po 30 dobach i poniżej 5 % powyżej pięciu. To jest **linia bazowa**, wobec której mierzy się skuteczność reklamy | Kryterium WP9 fazy M3 powstało dokładnie po to, żeby M10 miał na czym stanąć. Kampania, która daje mniej niż darmowa plotka, jest błędem kalibracji, a nie „słabą kampanią" — i bez tej liczby nie dałoby się tego odróżnić |
| | **Wiedza zdobyta z reklamy nie ma prawa wygrywać z „byłem tam".** `learn_place` nadpisuje rodzaj istniejącego wpisu tylko wtedy, gdy nowe źródło jest **mocniejsze**, a mocniejsze znaczy **niższa dyskryminanta** `KnowledgeKind`: `Visited` (0), `Heard` (1), `SeenOnRoute` (2), `Ad` (3). Ocena wpisu bierze przy tym maksimum, więc reklama może podnieść ocenę, ale nie zamieni „byłem tam" na „słyszałem" | Inaczej kampania kasowałaby własne doświadczenie klienta, a marka przestałaby być nadbudową nad jakością — co jest wprost wbrew PRD §6 |


---

## Tabela korekt planu (`F-n`)

Gwiazdka = zmiana zakresu albo kryterium. Rozstrzygnięcia dotykające kontraktu
z `00-konwencje-i-kontrakty.md` mają własny wpis `K-n` w §4a tamtego dokumentu
i są tu tylko wymienione.

| # | Korekta | Dlaczego |
|---|---|---|
| F-1 ★ | **Sloty marek są slabem, nie tablicą w komponencie.** §5.1 (wariant C) zapisywał `BrandSlots(pub [BrandAffinity; 16])`; jest `Slab<BrandAffinity>` obok slabu wiedzy i relacji, a w komponencie stoi 8-bajtowy uchwyt `BrandsRef` | Ten sam rachunek pamięci, tylko płacony za wpisy, które istnieją: tablica kosztowałaby 128 B każdego mieszkańca, także tego, który nie zna ani jednej marki. `Slab` jest już w tym module, ma `HashState`, wolne listy i regułę wypychania — druga struktura o tym samym zadaniu rozjechałaby się z pierwszą. **Zmierzone: 50 MB przy budżecie 55 MB**, stan gorący 429 B/mieszkańca przy sufitcie 430 |
| F-2 ★ | **`BrandId` przeprowadza się do `engine/core`, marka jest firmą, rejestru marek nie ma** (`K-79`). §5.1 pisało „firma może mieć do 4 marek"; jedna firma ma jedną markę i jest nią jej encja | Linie produktowe nie mają drugiego konsumenta i należą do M10c (`F-19`). Rejestr `FirmKey → BrandId` byłby stanem do wpięcia w hash i przeprowadzenia przez zapis — za wygodę, której nikt nie potrzebuje. Sufit jest jawny: powyżej 65 535 firm marki po prostu nie ma, zamiast po cichu dzielić ją z inną |
| F-3 ★ | **Powstaje crate `sim/media`.** Plan nie mówił, gdzie mieszkają kampanie | Kampania jest jedynym bytem w projekcie, który widzi naraz ruch (billboard), pamięć mieszkańca, firmę z pieniądzem i zdarzenia. Żaden istniejący crate nie stoi nad całą tą czwórką: `sim/events` stoi, ale jest własnością M8 (00 §1). Pamięć marki zostaje przy tym w `sim/agents`, bo piszą do niej trzy crate'y **pod** mediami |
| F-4 ★ | **`traffic.edge_traversals(edge)` z §5.2 nie istnieje — powstaje `EdgeWatch`** (`K-81`). Podsłuch notuje `(podróżny, krawędź)` przy wpuszczaniu podróży do sieci | `TrafficEvent` niesie `Arrived`, `Refuelled` i `Failed`; `MezoState` zna potoki bez tożsamości. Pełna trasa **z tożsamością** jest widoczna dokładnie w jednym miejscu i w jednej chwili — w `dispatch`. Kryterium „zero nowych indeksów przestrzennych" zostaje spełnione, bo podsłuch nie jest indeksem, tylko filtrem na istniejącej pętli |
| F-5 | **`ScoreCtx` nie istnieje.** §6 dokumentu fazy mówiło „zmienia się tylko źródło danych w `ScoreCtx`"; kontekstem kupującego jest `BuyerState`, a ofertą `Candidate`, oba w `sim/economy::choice` | Sygnatura `purchase_score` faktycznie się nie zmieniła i to była istota obietnicy. Doszła droga, którą pamięć mieszkańca dojeżdża do decyzji: `BrandView` w `CitizenView` i w `FulfilRequest` — ten sam wzorzec, którym jedzie `KnowledgeView`, bo `PlaceProvider` dostaje `&self`, a nie `&World` |
| F-6 | **`AudienceFilter` nie powstaje.** §5.2 zapisywał filtr dzielnic, klas i wieku | Nie ma drugiego konsumenta: prasa dobiera odbiorców czytelnictwem, ulotka promieniem, billboard trasą, promocja listą bywalców. Filtr byłby trzecim opisem tego samego wyboru — i tym, który rozjedzie się z dwoma pozostałymi |
| F-7 | **`Daypart`, `AdSlot` i `PrTopic` nie powstają.** Radio i telewizja różnią się przyrostem znajomości i CPM, a nie porą dnia | Pora emisji bez modelu oglądalności godzinowej jest mnożnikiem bez źródła. Mnożnik „z sufitu" wygląda w danych tak samo jak zmierzony i to jest cały powód, dla którego go nie ma |
| F-8 ★ | **`InStorePromo` nie niesie `discount_bps`.** Kanał daje ekspozycję bywalcom sklepu i nic poza tym | Zniżka jest zmianą ceny, a ceny prowadzi sterownik `PriceController` w `sim/economy` (M5c). Drugie wejście do ceny — z kampanii, z pominięciem sterownika — rozjechałoby marżę z polityką cenową firmy, i to w miejscu, w którym nikt by tego nie szukał. Rabat promocyjny należy do polityki cenowej, nie do marketingu |
| F-9 ★ | **Sponsoring dociera do dzielnicy zdarzenia, nie do jego uczestników.** §5.2 pisało „uczestnicy zdarzenia z M8" | Zdarzenie M8 nie ma listy uczestników i mieć nie będzie — ma `ScopeInstance` (świat, dzielnica, zakład, firma, sieć). To jest cała informacja, którą `sim/events` trzyma; zmyślenie listy byłoby dopisaniem danych, których nikt nie ma |
| F-10 ★ | **Znajomość zdarzenia jest licznikiem per dzielnica, a nie wpisem per mieszkaniec.** `Story.known: Vec<(DistrictId, u32)>`, a rozejście się tekstu to przyrost logistyczny na agregacie | Wpis per mieszkaniec wymagałby albo 400-tysięcznej tablicy bitów na tekst (1,2 MB na trzy doby), albo wpuszczenia zdarzeń do magazynu `Knowledge` — a ten trzyma **miejsca** i ma twardy limit 32 wpisów, więc strajk wypchnąłby mieszkańcowi sklep, do którego chodzi. Sufit jest nazwany w kodzie: dopóki nikt nie pyta „czy **ten** mieszkaniec wie o strajku", licznik wystarcza |
| F-11 ★ | **Kryterium WP10.7 „rozkład zgodny z czytelnictwem ± 5 pkt" jest wewnętrznie ciasne i test mierzy je tam, gdzie da się je spełnić.** Pierwsza część kryterium żąda znajomości **powyżej 40 %** przy czytelnictwie 35 %, czyli sama wymusza odchylenie większe niż pięć punktów | Obie granice naraz zamykają wynik w przedziale pustym. Test sprawdza więc to, co kryterium naprawdę mierzy: próg powyżej 40 % w dzielnicy czytającej, poniżej 15 % w dzielnicy nieczytającej, ± 5 pkt wobec czytelnictwa **dla tej drugiej**, oraz że plotka nie ucieka poza pasmo. **Zmierzone: 43,7 % i 11,0 %** |
| F-12 | **Prior dzielnicowy z §5.1 nie powstaje** (0,16 MB w rachunku pamięci) | Odpowiadałby na pytanie „co mieszkaniec sądzi o marce, której nie zna", a odpowiedź „nic" jest poprawna i jest zarazem stanem sprzed M10b, zachowanym co do bitu. Tablica 40 × 4096 bajtów bez czytelnika wygląda w pamięci tak samo jak działająca |
| F-13 | **Reguła przypięcia stoi na znajomości, nie na liczniku wizyt.** §5.1 mówiło „sklep odwiedzony ≥ 8 razy"; slot przypina się, gdy znajomość dobije do stu **z doświadczenia**, a przyrost per zakup jest ustawiony tak, żeby zajęło to `pin_visits` zakupów | Licznika wizyt nie ma i nie powstaje: `Knowledge` trzyma **jeden** wpis na cel i nie ma w nim ani bitu wolnego (8 B, `#[repr(C)]`, test rozmiaru). Ta sama informacja siedzi już w znajomości marki, a drugi licznik rozjechałby się z pierwszym |
| F-14 ★ | **`MacroFirm.brand_stock` przestaje być zerem.** `lift()` wypełnia go z pamięci mieszkańców, `lower()` z niego zasiewa (`D7`) | Pole stało w modelu od M7f i było zerem w każdym przebiegu — nikt go nie wypełniał i nikt nie czytał. Zasiew pamięci wymagał udziału rynkowego **marki**, więc obie strony domykają się jednym modułem (`sim/macro::brandseed`). `StreamId::MacroSeedMemory = 294` przestaje być zarezerwowany i niezajęty |
| F-15 ★ | **`OverlayField::BrandAwareness` dostaje parametr `brand` i dane** (`DK-4`). Nakładek jest dziesięć, nie dziewięć, a `is_reserved()` zwraca teraz `false` dla wszystkich | „Znajomość marki" bez wskazania marki nie jest pytaniem. Wpis w `data/ui/overlays.ron` i klucz w obu językach dołożone razem z danymi — paleta bez treści rysowałaby zera, a zero znaczyłoby „nikt nie zna" zamiast „nie ma czego mierzyć" |
| F-16 | **Kto kupuje reklamę: `magnat_media::ai`, nie `sim/firms`.** Raz na miesiąc firma z kursem innym niż `Cautious`, z saldem powyżej progu i bez żywej kampanii otwiera nową; kanał z zasobności, obietnica z kursu i agresji | Repertuar akcji firmy mieszka w `sim/firms`, ale `sim/firms` stoi **pod** mediami i o kampaniach nie wie — ta sama droga, którą `K-54` przeprowadził tier strategiczny. Bez tego cały mechanizm czekałby na pierwszy panel gracza, czyli na M10f, i przez trzy podfazy wyglądałby na działający |
| F-17 | **`data/site_types/media.ron` dostaje trzy tytuły** (`newspaper`, `radio_station`, `tv_station`) obok drukarni | Bez nich rejestr tytułów byłby pusty w każdym wygenerowanym mieście, a redakcja — mechanizmem, którego nikt nigdy nie uruchomi. Kolejność wpisów **nie** jest kontraktem zapisu: `magnat_media` szuka tytułu po kluczu tekstowym, a nie po `SiteTypeId` |
| F-18 | **Karta mieszkańca dostaje zakładkę „Marki"** — sympatia, oczekiwana jakość, znajomość i **źródło** kontaktu, każdy wiersz z odnośnikiem do firmy (`K-62`) | To jest główny dowód z M10 §6 pkt 6, że marka nie jest liczbą. Przy okazji: karta mieszkańca ma teraz **siedem** zakładek, czyli dokładnie sufit `MAX_CARD_TABS`. Kolejna zakładka mieszkańca wymaga decyzji, którą z obecnych złożyć — nie jest to problem tej podfazy, ale jest to fakt, o którym M10c–M10f muszą wiedzieć |
| F-19 | **Znaleziona luka spoza zakresu podfazy: siedem powodów bloku M8e (616–622) nigdy nie było sprawdzonych w obu językach.** Ramiona w `describe` istniały od M8e, ale wpisu w liście `wszystkie()` nie było, więc test `kazdy_powod_ma_tekst_w_obu_jezykach` ich nie widział. Dopisane razem z blokiem M10b — lista ma 101 pozycji | To ta sama luka, którą komentarz w tym samym pliku opisuje dla M8c („powinno być 78 i nie było"), i ten sam wniosek: **ramię w `describe` wymusza kompilator, wpisu w liście nie wymusza nikt**. Wniosek dla faz następnych stoi w §4 poniżej |
| F-20 ★ | **`BrandsRef` nie trafiał do dwóch produkcyjnych spawnerów mieszkańca** — narodzin (`demography::day`) i napływu migracyjnego (`migration::spawn_citizen`). Dokładały go wyłącznie światy testowe, a `World::get` jest archetypowe, więc cała pamięć marki milczałaby w grającym mieście i **nie złamałaby ani jednego testu** | Znalezione recenzją przed commitem, nie testem — bo testy stawiały własne encje. Naprawa ma własny test (`kazdy_mieszkaniec_ma_komplet_komponentow`), który sprawdza komplet komponentów u mieszkańców powstałych **po** zasiewie i który **puszczono na kodzie sprzed naprawy**: pierwsza wersja przechodziła również z błędem, bo świat testowy nie dożywał narodzin. Reguła, którą ten test od tej chwili egzekwuje, jest ogólniejsza niż marka: komponent z `register_components` ma być u każdego mieszkańca |
| F-21 ★ | **Zanik zapisywał się do slotu, nie ruszając doby kontaktu** — więc następny odczyt zanikał go drugi raz od tego samego punktu i marka gasła wykładniczo zamiast liniowo. Przy okazji `>>` na ujemnym afinitecie zaokrągla **w dół**, więc `-1` zostawało `-1` w nieskończoność: slot z niechęcią nie zwalniał miejsca nigdy, a komplet szesnastu zapychał się na stałe | Dwie usterki jednego rodzaju: arytmetyka, która na dodatnich liczbach robi to, co się wydaje, a na ujemnych coś innego. Kompaktowanie **tylko zwalnia miejsce**, a wszystkie kroki idą przez dzielenie całkowite obcinające ku zeru. Obie mają test |
| F-22 ★ | **`data/tuning/brand.ron` nie miał ani jednego czytelnika**: `register_resources` wstawiało `BrandData::default()`, więc plik, walidacja `schema_version` i walidacja asymetrii były martwe od chwili powstania. Po podpięciu wczytywania w `game::world::full` okazało się, że **plik się nie parsuje** — tablice o stałym rozmiarze były zapisane jako listy `[...]`, a RON czyta takie jak krotki | Najciekawsze znalezisko całej podfazy i jedyne, które ujawniło się **kaskadą**: martwy loader ukrywał niepoprawny plik. Gdyby wczytywanie podpięto dopiero w M10c, błąd w danych czekałby tam ze śladem prowadzącym do cudzej zmiany. Loader ma od tej chwili własny test (`plik_strojenia_marki_sie_wczytuje`) |
| F-23 | **Przelew i księga mogły się rozjechać.** Wynik `post_marketing_expense` był ignorowany, więc odrzucony zapis w księdze zostawiał pieniądz przesunięty i koszt nieudokumentowany — rozjazd, którego `check_conservation` nie widzi, bo suma sald się zgadza. Kolejność jest teraz **księga przed przelewem**: zapis niczego nie przesuwa, więc jego odmowa jest tania | Ta sama zasada, którą `K-57` postawił przy daninie: księga zakładu mówi, **na co** poszło, dziennik transakcji — **do kogo**. Kampania, której nie udało się opłacić, **kończy się**, zamiast emitować dalej za darmo: darmowa emisja nie tworzy pieniądza, ale tworzy zasięg bez kosztu, czyli dokładnie to, czego balansator nie ma jak zobaczyć |
| F-24 | **Redakcja opisywałaby ten sam strajk od nowa co cztery doby.** Deduplikacja publikacji patrzyła na **żywe** teksty, a te schodzą z obiegu po trzech dobach; wpis w kronice zdarzeń zostawał dłużej. Redakcja bierze teraz wyłącznie zdarzenia z ostatniej doby — nowina jest nowa | Okno dobowe jest przy tym tym samym oknem, którym kronika gracza zbiera dzienniki `sim/*`, więc nie wprowadza drugiego rytmu |
| F-25 | **Plotka nie ruszała oczekiwań.** Wyrażenie z §5.1 („nigdy powyżej wartości nadawcy") redukowało się do tożsamości, bo `expected_quality` nie było jeszcze w tym ramieniu zmieniane. Test przechodził **trywialnie** — sprawdzał wyłącznie sufit, którego nic nie naruszało | Poprawione razem z testem, który teraz wymaga, żeby plotka faktycznie przesunęła oczekiwania. Wniosek, który wraca z M11e: test sprawdzający wyłącznie górną granicę przechodzi także wtedy, gdy mechanizm nie robi nic |
| F-26 | Drobne z recenzji: sponsoring losował **ten sam punkt startu dla każdej dzielnicy** (klucz bez indeksu dzielnicy); billboard mieszał numer kampanii z tickiem przez dodawanie, więc pary `(tick, kampania)` się zlewały; `promocja` ustawiała dobę kontaktu na zero; `stories_max - stories_min` na `u8` mogło się zawinąć; `brand_stock` obcinał dwa razy i dawał zero, gdy `\|suma\| < ilu`; cztery pętle kopiowały całą populację raz na kampanię na dobę | Żadne z tych sześciu nie łamało testu i żadne nie łamało kompilacji |

| F-27 ★ | **Budżet §7.5 („systemy reklamowe ≤ 1,5 % ticku przy 2000 kampanii") nie jest zweryfikowany dla dwóch tysięcy kampanii, ale przy trzydziestu pięciu udział mediów mieści się w szumie pomiaru.** Zmierzone w `m7miasto` (28,5 tys. mieszkańców, 4 km, 40 dób): **14,83 s/dobę** przed zawężeniem kanału ulotkowego i **15,29 s/dobę** po nim — przy trzykrotnym spadku pracy mediów (1,61 mln → 482 tys. ekspozycji). Gdyby reklama była kosztem dominującym, ścięcie 70 % jej pracy byłoby widać | Pierwsze podejrzenie było fałszywe i warto zapisać dlaczego: doba kosztuje 1,15 s w piątej dobie i ~15 s w czterdziestej, ale **to nie jest koszt mediów** — to miasto, które się rozgrzało (968 tys. wizyt wobec 10,1 mln, zero odmów wobec 945 tys.). Porównywanie dwóch różnych dób jako „przed i po" mierzy wzrost gospodarki, nie zmianę kodu. Zawężenie ulotek zostaje mimo to, bo skanowanie całego miasta na każdą kampanię i każdą dobę jest kosztem rosnącym z liczbą kampanii razy liczba mieszkańców — czyli dokładnie tym, czego §7.5 dotyczy przy dwóch tysiącach. Rzetelny pomiar tamtej skali wymaga przebiegu balansatora z profilem; adres: **M10f** |

---

## Co M10b zostawia następnym podfazom

Zgodnie z `K-18` — wpisane jest tylko to, co wiadomo na pewno.

1. **`DecisionReason::BrandLearned` i `BrandExperience` nie mają trwałego czytelnika.**
   `magnat_agents::touch` je zwraca, `reason::describe` je rysuje, ale nikt ich nie
   zapisuje: pełny log decyzji dla 400 tys. mieszkańców to 14 GB na rok gry (M3,
   decyzja 9.16), więc karta pokazuje **stan** slotu, a nie historię kontaktów.
   Trwałym czytelnikiem ma być panel marketingu (M10f) i lejek „znajomość → próba →
   afinitet" z M10 §6 pkt 1, liczony z `CampaignMetrics`. Adres: **M10f**.
2. **Gracz nie ma jak kupić reklamy.** Kampanie otwiera wyłącznie `magnat_media::ai`.
   Komenda i panel należą do M9/M10f (`DK-3`: panel dokłada się przez
   `PanelRegistry::register`, a `PanelId::Brand` już istnieje i `is_reserved()` mówi
   o nim prawdę). Adres: **M10f**.
3. **Tytuł medialny nie ma karty ani `Subject`.** Jest zakładem, więc otwiera się
   jako `Subject::Site`, ale czytelnictwo, wiarygodność i linia redakcyjna nie mają
   gdzie się pokazać. Adres: **M10f**.
4. **Czytelnictwo tytułu nie jest strojone i nie zmienia się w czasie.** Tytuł czyta
   się najlepiej w swojej dzielnicy i dwa razy słabiej poza nią — jedna reguła bez
   rozrzutu. Numer `StreamId` na rozrzut **nie jest zarezerwowany**, bo dopóki nikt
   go nie stroi, zapisałby na wieczność liczbę bez właściciela. Adres: **M10f**.
5. **Wiarygodność tytułu nie spada.** §5.3 obiecuje, że tytuł piszący rzeczy
   niezgodne z obserwacją czytelnika traci u niego zaufanie; mechanizm ma nośnik
   (slot marki tytułu), ale nie ma wyzwalacza — trzeba porównać treść tekstu
   z tym, co mieszkaniec sam zaobserwował, a tekst niesie dziś `EventId`, nie tezę.
   Adres: **M10f**, razem z kroniką.
