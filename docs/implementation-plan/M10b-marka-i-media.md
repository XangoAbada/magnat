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

### WP10.5 — Marka: `BrandAffinity` w pamięci mieszkańców

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

### WP10.6 — Kanały i kampanie (`AdCampaign`, `Reach`)

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

### WP10.7 — Media jako firmy

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
