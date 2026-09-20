# M10c — R&D i nowe produkty

Podfaza 3 z 6 fazy **M10 — Głębia** (`M10-glebia.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M6 (receptury), M7 (firmy), M8 (`EpochClock`). |
| **Pakiety robocze** | WP10.8, WP10.9 |
| **Projekt techniczny** | §5.4 |
| **Wynik do pokazania** | Technologia odblokowuje kategorię towaru, patent daje wyłączność, licencja ją sprzedaje. |
| **Kryterium zamknięcia** | Kryteria WP10.8 i WP10.9; fala C katalogu towarów w repo i zielona w walidatorze. |
| **Poprzednia / następna** | `M10b-marka-i-media.md` · `M10d-gielda-przejecia-ubezpieczenia.md` |

`TechTree`, punkty badawcze, patenty i licencje oraz nowe kategorie produktów wraz ze zmianą struktury potrzeb w czasie.

---

## Pakiety robocze

### WP10.8 — R&D: `TechTree`, punkty badawcze, `Patent`, licencje

**Zależności:** M7 (dział, stanowiska, budżet), dane `data/tech/`.
**Kryterium ukończenia:** firma z 4 badaczami (umiejętność 60) i budżetem 50 tys./mies. odkrywa węzeł
o koszcie 1200 RP w 9 ± 1 miesiącu gry, deterministycznie. Odkrycie przed rokiem „światowym" daje patent;
po roku „światowym" węzeł jest dostępny bez patentu po obniżonym koszcie. Licencja jest `ContractId`
z M6, nie nowym mechanizmem. Walidator grafu `data/tech/` w CI (każdy prereq istnieje, brak cykli,
każdy `NewGood` istnieje w `data/goods/`).
**Rozmiar: L.**

---

### WP10.9 — Nowe kategorie produktów i zmiana potrzeb

**Zależności:** WP10.8, M8 (`sim/events`), M3 (potrzeby).
**Kryterium ukończenia:** odblokowanie kategorii „telefon komórkowy" w roku epoki rzeczywiście zmienia
koszyk potrzeby „kontakt społeczny" i tworzy nowy łańcuch dostaw (walidator grafu towarów zielony
po zmianie). Mieszkańcy z wysoką otwartością kupują pierwsi — mierzalne w rozkładzie.
**Rozmiar: M.**

---

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.4 R&D, technologie, epoki

```rust
pub struct TechTree { pub nodes: Vec<TechNode>, pub by_branch: Vec<Vec<TechId>> }

pub struct TechNode {
    pub id: TechId,
    pub branch: BranchId,
    pub prereqs: SmallVec<[TechId; 4]>,
    pub cost_rp: u32,                 // punkty badawcze, milipunkty w akumulacji
    pub world_year: i32,              // rok, od którego technologia jest „światowa"
    pub effects: SmallVec<[TechEffect; 4]>,
}

pub enum TechEffect {
    QualityBonus   { good: GoodId, delta_q: i8 },
    CostReduction  { recipe: RecipeId, bps: u16 },
    NewRecipe      (RecipeId),
    NewGood        (GoodId),           // §11.3 — nowa kategoria, zmienia koszyk potrzeb
    MachineUpgrade { machine: MachineId, throughput_bps: u16, failure_bps: i16 },
    ProductFeature { good: GoodId, feature: FeatureId },
}

pub struct Patent {
    pub tech: TechId,
    pub owner: FirmId,
    pub granted: SimMinute,
    pub expires: SimMinute,            // 20 lat gry
    pub license_policy: LicensePolicy, // Zamknięty | Otwarty{opłata} | Wybiórczy{lista}
}
```

**Postęp:** `rp_per_day = Σ_badacz f(umiejętność, energia, nastrój) × jakość_laboratorium ×
min(1, budżet_materiałowy / próg)` — wszystko całkowitoliczbowe, milipunkty. Akumulacja na wybranym
węźle. Element losowy jest **jeden**: przełom (`StreamId::RnDBreakthrough`) skraca pozostały koszt
o 10–40% z małym prawdopodobieństwem dziennym. Brak losowego „nie udało się" — frustrujące i nieczytelne.

**Trzy tryby dostępu do technologii:**
1. **Odkrycie przed `world_year`** → patent na 20 lat. Przewaga i źródło przychodu z licencji.
2. **Odkrycie po `world_year`** → bez patentu, koszt RP obniżony o 60% (wiedza jest w obiegu).
3. **Licencja** → `ContractId` z M6 (opłata wstępna + royalty w bps od przychodu z produktu).
   Zero nowego mechanizmu umów.

**Epoki (§11.3).** `data/epochs/*.ron` (istniejący katalog z dok. 00 §5) mapuje rok → zbiór
`world_year` dla węzłów. Nowa kategoria produktu (`NewGood`) uruchamia migrację koszyka potrzeb:
towar dopisuje się do koszyka wskazanej potrzeby z wagą początkową i **przesuwa wagi substytutów**.
To jest jedyne miejsce, w którym dane symulacji zmieniają się w trakcie partii — musi przejść przez
walidator grafu towarów (dok. 00 §5), inaczej odrzucamy zmianę i logujemy błąd danych.

**Nowa kategoria oznacza nowy łańcuch.** `NewGood` bez receptury i bez importu to błąd danych
wykrywany w CI, nie w runtime.


---

## Zmiany wpisane po M10b

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M10b.
Gwiazdka = zmiana zakresu albo kryterium. Szczegóły — tabela `F-n` w `M10b-marka-i-media.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| FC-1 ★ | **Marka jest firmą i produkt jej nie ma** (`K-79`). `BrandId` to encja firmy, a `magnat_supply::brand_of` jest funkcją czystą bez rejestru. Nowy towar z R&D dziedziczy więc markę **wytwórcy**, a nie dostaje własnej | To jest rozstrzygnięcie M10b, nie przeoczenie: linia produktowa nie miała drugiego konsumenta, a rejestr marek byłby stanem do wpięcia w hash i przeprowadzenia przez zapis. **Jeśli M10c chce marek per produkt** — bo „nowa kategoria produktu" z §7.7 bywa osobną marką — to jest to decyzja otwarta z propozycją domyślną: **zostawić markę przy firmie**. Rozdzielenie kosztuje rejestr, `Batch.brand` przestaje być wyprowadzalne z producenta, a `firm_of(brand)` przestaje być bijekcją, więc karta mieszkańca traci odnośnik do firmy |
| FC-2 ★ | **`MacroFirm.tech` jest tym samym, czym był `brand_stock` do M10b: polem zawsze zerowym.** `lift()` wypełnia je `TechLevel(tech)` z rejestru firm, ale poziom technologii nikt nie podnosi, więc w każdym przebiegu jest to ta sama liczba | M10b pokazał, ile kosztuje takie pole: `brand_stock` stał w modelu od M7f, przechodził każdy test i wyglądał tak samo jak działający. Wzór domknięcia jest gotowy do przepisania — `sim/macro::brandseed` robi dokładnie dwie rzeczy, których `tech` potrzebuje: agregat przy `lift()` i wpływ przy `lower()` |
| FC-3 | **Blok `StreamId` M10: zajęte 280–284 (reklama i media) oraz 292–295 (makro).** Wolne dla M10c–M10e: **285–291 i 296–299** | `StreamId::RnDBreakthrough = 285` z tabeli M10 §7.1 jest nadal wolny i nadal zarezerwowany imiennie |
| FC-4 | **Blok `DecisionReason` M10: zajęte 800–803.** Wolne: **804–899** | Blok M9 (700–799) zostaje w całości wolny (`K-71`) |
| FC-5 | **Karta mieszkańca ma siedem zakładek, czyli sufit `MAX_CARD_TABS`.** Doszła „Marki" (`F-18`) | Ósma zakładka mieszkańca wymaga decyzji, którą z obecnych złożyć. Dla M10c to znaczy: cecha produktu z R&D pokazuje się w karcie **towaru albo firmy**, nie mieszkańca |
