# M5b — Sklep i zakup

Podfaza 2 z 5 fazy **M5 — Gospodarka detaliczna** (`M5-gospodarka-detaliczna.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M5a (pieniądz, oferta), M3 (mieszkańcy), M4 (dojazd). |
| **Pakiety robocze** | WP3, WP4, WP5 |
| **Projekt techniczny** | §5.3, §5.4, §5.5, §5.7 |
| **Wynik do pokazania** | Mieszkaniec wychodzi po chleb, wybiera ofertę i wraca; pieniądz i sztuki zgadzają się po obu stronach. |
| **Kryterium zamknięcia** | Kryteria WP3–WP5; brak sprzedaży poniżej stanu magazynowego przy współbieżnym zakupie. |
| **Poprzednia / następna** | `M5a-pieniadz-i-oferta.md` · `M5c-ceny-i-ksiegowosc.md` |

Sklep jako magazyn + półka + asortyment z zewnętrznym dostawcą jako jawnym punktem wymiany do M6, pełna funkcja użyteczności zakupu i rozliczanie transakcji z wyścigiem o ostatnią sztukę.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| WP3 | Sklep: magazyn, półka, asortyment, zewnętrzny dostawca | WP1, WP2 | L |
| WP4 | Funkcja użyteczności i wybór oferty | M3, M4, WP2 | L |
| WP5 | Rozliczanie transakcji | WP1, WP3, WP4 | M |

### WP3 — Sklep: magazyn, półka, asortyment, zewnętrzny dostawca
Zaplecze (`ShopInventory`) i półka (`Shelf`) to **dwa różne stany**; oferta odzwierciedla wyłącznie
półkę. Uzupełnianie półki z zaplecza co godzinę, zamówienie u zewnętrznego dostawcy wg polityki
zapasu (punkt ponownego zamówienia + czas dostawy z danych).
Kryterium: test własnościowy „brak ujemnych stanów" i „sklep nie sprzedaje towaru, którego nie ma"
zielony; scenariusz zerwania dostaw → półka pustoszeje w tempie sprzedaży, oferta znika z kandydatów,
ale **sklep pozostaje widoczny w inspekcji z powodem „brak towaru"**.

### WP4 — Funkcja użyteczności i wybór oferty
Pełna treść w sekcji 5.4. Kryterium: dwa przebiegi tego samego seeda dają identyczny ciąg wyborów;
test wrażliwości — podniesienie ceny w jednym sklepie o 10% przesuwa udział rynkowy monotonicznie
w dół dla każdej z 20 losowych populacji; 100% decyzji ma zapisany `DecisionReason`.

### WP5 — Rozliczanie transakcji
Rozdzielenie na fazę decyzji (równoległą, bez mutacji) i fazę rozliczenia (sekwencyjną,
deterministyczną). Rozwiązuje wyścig o ostatnią sztukę bez blokad.
Kryterium: 10 tys. agentów kierujących się do sklepu z 10 sztukami na półce → dokładnie 10 transakcji,
9990 zdarzeń `Stockout` z przeplanowaniem, suma pieniądza bez zmian.

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.3 Sklep: magazyn, półka, asortyment

```rust
pub struct ShopInventory {
    pub site: SiteId,
    pub backroom: BTreeMap<GoodId, StockLine>,
    pub capacity_m3: i64,                 // z budynku (§7.3)
    pub reorder: BTreeMap<GoodId, ReorderPolicy>,
}

pub struct StockLine {
    pub qty: Qty,                         // NIGDY < 0 — niezmiennik testowany
    pub cost_total: Money,                // łączny koszt nabycia tej linii → wycena średnią ważoną
    pub expires: Option<SimMinute>,       // M5: jedna data na linię. M6: zastąpione przez BatchId
}

pub struct Shelf {
    pub site: SiteId,
    pub slots: u16,                       // z pojemności budynku
    pub lines: Vec<ShelfLine>,            // len <= slots, posortowane po GoodId
}

pub struct ShelfLine {
    pub good: GoodId,
    pub qty: Qty,
    pub facings: u16,                     // ile miejsc na półce — limit ekspozycji
    pub offer: OfferId,
}

pub struct ReorderPolicy { pub point: Qty, pub target: Qty, pub lead_time_days: u8 }

pub enum AssortmentPolicy {
    Manual { goods: Vec<GoodId> },                    // gracz
    Auto   { max_lines: u16, min_margin_bp: i32 },    // AI: top-N wg marża × popyt w dzielnicy
}
```

Półka a zaplecze to dwa stany. `Offer.available` odzwierciedla **wyłącznie półkę** — towar
w zapleczu nie jest na sprzedaż. Przy wyczerpaniu półki oferta zostaje (z `available == 0`),
bo sklep ma pozostać widoczny w inspekcji jako „znany, ale bez towaru" — to jest odpowiedź
na pytanie z §14.1.

### 5.4 Funkcja użyteczności zakupu (§6.4) — pełna specyfikacja

#### Kandydaci (§17.5: 3–15, nie O(n·m))

```rust
pub struct Candidate {
    pub offer: OfferId,
    pub site: SiteId,
    pub good: GoodId,
    pub qty: Qty,                 // ile agent chce kupić
    pub price_total: Money,       // unit_price × qty
    pub travel: TravelCost,       // z M4: (SimMinute, Money)
    pub quality: Q,
    pub known: bool,
}

pub fn gather_candidates(
    ctx: &MarketCtx,
    buyer: CitizenId,
    need: NeedId,
    budget_ref: Money,
    out: &mut CandidateBuf,
) -> usize;
```

Algorytm (deterministyczny, bez alokacji):
1. `need → &[GoodId]` — koszyk substytutów z `data/goods/` (§5.3), posortowany po randze substytutu.
2. `query_offers` dla kategorii potrzeby, promień = `max_travel_m(mode, personality)` z M4.
3. Filtr twardy: `available >= qty` **i** `known(buyer, site)` (§5.7: odwiedzony ∪ komórka domu/pracy/trasy).
4. Wstępny ranking po całkowitoliczbowym `prescore = −(price_total + travel_money + travel_min·vot)`
   kwantyzowanym do `i32`; sortowanie po `(−prescore, SiteId, GoodId)` — remisy rozstrzygane po id.
5. Obcięcie do `K_MAX = 15`. Jeśli `< K_MIN = 3` → jednokrotne rozszerzenie promienia o jeden pierścień
   komórek. Jeśli nadal `0` → `DecisionReason::NoCandidates { cause }`.

#### Użyteczność

```rust
pub fn utility_of_offer(
    c: &Candidate,
    w: &UtilityWeights,
    st: &BuyerState,
    noise: f32,
) -> f32;
```

```
U = w_price   · f(price_total / budget_ref)
  + w_quality · (quality / 100)
  + w_brand   · brand_affinity                 // M5: ≡ 0.0, hook M10
  + w_dist    · f(travel_cost / budget_ref)
  + w_loyalty · loyalty(site)
  + w_status  · status_fit(quality, status)
  + w_novelty · novelty(site) · openness
  + noise
```

Konkretne postaci członów:

| Człon | Postać | Uzasadnienie |
|---|---|---|
| `f(x)` | `f(x) = −ln(1 + x)`, `x ≥ 0` | jedna funkcja dla ceny **i** odległości — obie wyrażone jako ułamek budżetu, więc porównywalne. `f(0)=0`, monotonicznie malejąca, malejąca wrażliwość przy dużym `x` (kto już wydał pół budżetu, nie rozróżnia drobnych różnic) |
| `g` (odległość) | `g ≡ f`, argumentem jest `travel_cost` | świadomie ta sama funkcja: PRD wymaga „kosztu dojazdu w czasie i pieniądzu" sprowadzonego do jednej wielkości |
| `travel_cost` | `travel_money + travel_min · vot`, gdzie `vot = dochód_netto_GD_miesięczny / (22·8·60) · vot_factor(status)` | wartość czasu wyprowadzona z dochodu, nie zgadnięta; bogatszy mieszkaniec realnie omija tani sklep na drugim końcu miasta |
| `budget_ref` | `envelopes[need] / max(1, oczekiwane_pozostałe_zakupy_w_miesiącu)` | mianownik ma definicję operacyjną z WP8, nie jest stałą |
| `loyalty(site)` | `(avg_rating(site) − 50)/50 · visits/(visits + 3)` ∈ ⟨−1, 1⟩ | z pamięci doświadczeń M3 (§5.1); skurcz `n/(n+3)` — jedna wizyta nie tworzy lojalności |
| `status_fit` | `1 − |tier(quality) − tier(status)| / 4`, tiery 0..4 | elita nie kupuje najtańszego, GD o niskim statusie nie kupuje luksusu — nawet gdy stać |
| `novelty(site)` | `1.0` jeśli `visits == 0`, inaczej `0.0` | jednorazowa premia za spróbowanie nowego sklepu — to jest mechanizm, dzięki któremu sklep gracza w ogóle ma pierwszego klienta (§5.7) |
| `openness` | `Personality.otwartość / 100` | |
| `noise` | `sigma · u`, `u ~ U(−1, 1)` | patrz niżej |

#### Wagi (§6.4: „wynikają z osobowości i statusu")

```rust
pub struct UtilityWeights { pub price: f32, pub quality: f32, pub brand: f32, pub dist: f32,
                            pub loyalty: f32, pub status: f32, pub novelty: f32 }

pub fn weights_for(p: &Personality, status: Q, need: NeedId, data: &EconomyData) -> UtilityWeights;
```

Konstrukcja: baza per potrzeba z `data/economy/weights.ron` (np. dla „głód" cena waży więcej niż
dla „status"), modulowana **multiplikatywnie** przez osobowość i status, potem normalizowana
tak, by `Σ|w| = 1` (żeby próg `U_threshold` miał wspólną skalę dla wszystkich potrzeb):

```
price   = base.price   · (0.5 + 1.0 · wrażliwość_cenowa/100) · (1.4 − 0.8 · status/100)
quality = base.quality · (0.6 + 0.8 · status/100)
dist    = base.dist    · (0.6 + 0.8 · (1 − mobilność_GD))
loyalty = base.loyalty · (lojalność/100)
novelty = base.novelty · (otwartość/100)
status  = base.status  · (status/100) · (ambicja/100 + 0.5)
brand   = base.brand                                     // M5: mnożone przez afinitet ≡ 0
```

Brak nowych parametrów osobowości — wszystkie z §5.1 (własność M3).
**Elastyczność cenowa nie jest tu parametrem** — jest własnością rozkładu `price` w populacji
i dostępności alternatyw (§6.4). Balansator ją mierzy, nie ustawia.

#### Szum — powtarzalny (dok. 00 §3 pkt 1)

```rust
// Szum musi być STAŁY dla danej trójki (kupujący, oferta, decyzja) w obrębie jednej decyzji,
// inaczej ponowna ewaluacja da inny wynik.
let key = mix64(citizen.index() as u64, offer.index() as u64);
let noise = rng_uniform_f32(world_seed, StreamId::PurchaseNoise, key, tick) * 2.0 - 1.0;
let noise = noise * sigma;     // sigma z data/economy, kalibrowana balansatorem
```

Żadnego globalnego stanu RNG. **K-4: blok `StreamId` przydzielony M5 to 180–199.**
Warianty dopisywane do enuma w `core`, nigdy nie zmieniamy istniejących wartości:

| Wartość | Wariant |
|---|---|
| 180 | `PurchaseNoise` |
| 181 | `PurchaseChoice` |
| 182 | `PriceExperiment` |
| 183 | `CompetitorDelay` |
| 184 | `ExternalPriceDrift` |
| 185 | `CreditScoringJitter` |
| 186–199 | wolne (rezerwa na rozszerzenia `sim/economy`, m.in. rynek pracy od M7) |

#### Wybór: softmax (§6.4 — nie argmax)

```rust
pub fn choose_offer(
    cands: &[Candidate],          // posortowane deterministycznie (krok 4 wyżej)
    utils: &[f32],
    temperature: f32,
    seed: RngKey,
) -> Choice;

pub enum Choice { Buy { idx: usize }, Defer { reason: DeferReason } }
```

1. `core::det_math::softmax(&utils, T, &mut probs)` — odejmuje maksimum (stabilność numeryczna)
   i sumuje **w kolejności indeksów wejściowych**, bez redukcji parami (K-6).
2. Dlatego `utils` musi już być w kolejności posortowanej tablicy kandydatów (krok 4 wyżej);
   nigdy po mapie (dok. 00 §2).
3. Losowanie: `r = rng_uniform_f32(seed, StreamId::PurchaseChoice, citizen.index(), tick)`,
   przejście po sumie skumulowanej `probs` w tej samej kolejności.
4. `T` (temperatura) z `data/economy/choice.ron`, jeden parametr globalny; kalibrowany balansatorem.
   `T → 0` degeneruje do argmax i produkuje monopole — bramka balansatora na koncentrację rynku
   (HHI) pilnuje, żeby kalibracja tam nie zjechała.

#### Próg odłożenia zakupu (§6.4)

```rust
pub fn purchase_threshold(need: NeedId, satisfaction: Q, budget: &HouseholdBudget, d: &EconomyData) -> f32;
```

```
U_threshold = thr0(need)
            − k_urgency · (100 − satisfaction)/100        // głodny kupi drożej
            + k_envelope · max(0, overspend_bp) / 10_000  // wyczerpana koperta podnosi próg
```

Ścieżka gdy `U_best < U_threshold` (kolejność z PRD §6.4):
1. **Substytut niższego rzędu** — następna ranga w koszyku potrzeby; jednokrotna ponowna ewaluacja
   z tym samym seedem decyzji (bez rekurencji).
2. Jeśli dalej poniżej progu — **odłożenie**: `Choice::Defer`, agent wraca do zadania później tego dnia.
3. Jeśli dzień się kończy — **ograniczenie konsumpcji**: zaspokojenie potrzeby spada,
   skutki obsługuje M3 (§5.3). `sim/economy` tylko raportuje zdarzenie.

#### Wyjaśnialność (§14.1, dok. 00 §7)

```rust
// K-12: JEDEN centralny enum w engine/core, bez #[non_exhaustive].
// M5 dopisuje poniższe warianty do istniejącego enuma — nie tworzy własnego.
pub enum DecisionReason {
    // ... warianty z innych faz
    Purchase {
        chosen: OfferId, runner_up: Option<OfferId>,
        price_delta_bp: i32,            // vs runner-up
        dominant_term: UtilityTerm,     // który człon przeważył
        u_chosen: f32, u_threshold: f32,
    },
    OfferRejected { site: SiteId, cause: RejectCause },
    PurchaseDeferred { need: NeedId, best_u: f32, threshold: f32, cause: DeferReason },
    Repricing { site: SiteId, good: GoodId, from: Money, to: Money, driver: PriceDriver },
    CreditDecision { applicant: AccountOwner, outcome: CreditOutcome, dsti_bp: i32 },
}

pub enum RejectCause {
    NotKnown, OutOfStock, TooFar { extra_min: u16 },
    PriceHigherBy { bp: i32 }, QualityBelowStatus, BudgetExhausted,
}
```

**Zapis utraconej sprzedaży (`LostSale`) — kontrakt uzgodniony z M9.** To jedyny sposób, żeby
odpowiedzieć na pytanie, które PRD §14.1 stawia jako sztandarowy przykład karty inspekcji.
Przyjmuję propozycję M9 w całości — trójstopniowa, bo pełny bufor na wszystkich sklepach AI
byłby kosztem bez odbiorcy:

```rust
pub enum LostSaleTracking { None, Histogram, Full }   // wybierane per SiteId

pub struct LostSaleHistogram {                 // ~120 B na sklep na dobę
    pub day: SimDay,
    pub by_cause: [u32; RejectCause::COUNT],   // ile razy który powód
    pub by_good:  SmallVec<[(GoodId, u32); 8]>,
}

pub struct LostSale {                          // 16 B, pierścień 256 wpisów
    pub citizen: CitizenId, pub good: GoodId,
    pub when: Tick, pub cause: RejectCause, pub went_to: Option<SiteId>,
}
```

| Zakład | Poziom | Koszt |
|---|---|---|
| zakłady gracza | `Histogram` **zawsze** + pierścień `Full` 256 wpisów | ~12,8 kB/sklep |
| zakłady oznaczone przez gracza („śledź") | `Full` | jw. |
| pozostałe zakłady AI | `None` | **0** |

Przy 200 sklepach gracza z pełnym śledzeniem i rokiem histogramów: ≈ 2,5 MB — mieści się
w budżecie pamięci §17.7. Zapis następuje w fazie decyzji, gdy kandydat został odrzucony
**i** jego `LostSaleTracking != None` — sprawdzenie to jeden odczyt bitu, więc gorąca ścieżka
dla rynku obsadzonego wyłącznie przez AI nie płaci nic.

#### Determinizm zmiennoprzecinkowy — rozstrzygnięte (K-6)

`ln`/`exp` z systemowego libm **nie są bit-identyczne między platformami**, a funkcja użyteczności
używa obu. M0 dostarczył `core::det_math` (`ln`, `ln1p`, `log2`, `exp`, `exp_m1`, `exp2`, `pow`,
`sqrt`, `softmax`) o dokładności ≤ 2 ULP, z lintem zakazującym libm w kodzie symulacji.

**`sim/economy` używa wyłącznie `core::det_math` — nie `libm`, nie `f32::ln`/`f32::exp`.**
Konkretnie: `f(x) = −det_math::ln1p(x)`, a wybór oferty woła `det_math::softmax`, nie własną pętlę.
`det_math::softmax` sumuje w kolejności indeksów wejściowych i nie stosuje redukcji parami —
dlatego tablica kandydatów musi być posortowana **przed** wywołaniem (krok 4 doboru kandydatów),
i to sortowanie jest jedynym miejscem, gdzie ustala się kolejność sumowania.

### 5.5 Rozliczanie transakcji — wyścig o ostatnią sztukę

Faza decyzji jest równoległa po chunkach mieszkańców i **nie mutuje** magazynów. Produkuje intencje:

```rust
pub struct PurchaseIntent {
    pub buyer: CitizenId, pub household: HouseholdId,
    pub offer: OfferId, pub good: GoodId, pub qty: Qty,
    pub agreed_price: Money,          // cena z momentu decyzji
    pub arrived: Tick,
    pub reason: DecisionReason,
}
```

Faza rozliczenia (`settle_transactions`, EveryMinute, sekwencyjna) sortuje intencje po
`(SiteId, GoodId, arrived, CitizenId)` i obsługuje do wyczerpania półki:

- towar jest → `Books::transfer(GD → sklep)`, `Shelf.qty -= qty`, zapis w dzienniku sklepu
  (Revenue + Cogs), wpis do pamięci doświadczeń agenta (M3), aktualizacja `Offer.available`;
- towaru brak → `RejectCause::OutOfStock`, zdarzenie DES „przeplanuj zakup" (§17.3);
- cena się zmieniła między decyzją a rozliczeniem o więcej niż `price_slippage_bp` →
  agent ponownie ocenia próg (jednokrotnie), inaczej rezygnuje.

To rozwiązanie daje niezmiennik „żaden sklep nie sprzedaje towaru, którego nie ma"
**konstrukcyjnie**, bez blokad i bez zależności od kolejności ukończenia jobów (dok. 00 §3 pkt 3).

### 5.7 Punkt wymiany M5 ↔ M6: zewnętrzny dostawca

To jedyne źródło towaru w M5 i **jedyne miejsce, które M6 musi wymienić**. Sygnatura i księgowanie
są zaprojektowane tak, żeby M6 podmienił implementację, nie interfejs.

```rust
/// Kontrakt zaopatrzenia sklepu. W M5: jedna implementacja (ExternalSupplier).
/// W M6: zastąpiona przez rynek B2B (spot + kontrakty) — sygnatura bez zmian.
pub trait Wholesale {
    fn quote(&self, good: GoodId, qty: Qty, at: SiteId, t: Tick) -> Option<PurchaseQuote>;
    fn place_order(&mut self, q: &PurchaseQuote, buyer: FirmId, t: Tick) -> Result<OrderId, SupplyError>;
    fn poll_deliveries(&mut self, t: Tick, out: &mut Vec<Delivery>);
}

pub struct PurchaseQuote {
    pub good: GoodId, pub qty: Qty,
    pub unit_price: Money,          // cena hurtowa
    pub delivery_at: Tick,          // teraz + lead_time_days z data/goods
    pub quality: Q,
    pub shelf_life: Option<SimMinute>,
}

/// M5: cena hurtowa = wholesale_base(good) · sezonowość(month) · dryf(t) — z data/goods/*.ron.
/// Dryf deterministyczny: rng(world_seed, StreamId::ExternalPriceDrift, good, day).
/// Pieniądz idzie na konto AccountOwner::RestOfWorld — czyli NIE WYPADA z systemu.
pub struct ExternalSupplier { /* ... */ }
```

Trzy jawne konsekwencje, które M6 musi znać:
1. `RestOfWorld` jest kontem w `Books`, nie ujściem — dzięki temu niezmiennik pieniądza jest
   sprawdzany bez ewidencji przepływów zewnętrznych.
2. Zapas jest wyceniany **średnią ważoną** (`StockLine.cost_total / qty`), bo nie ma partii.
   M6 wprowadza `BatchId` i FIFO — to **zmienia COGS**; migracja opisana w sekcji 9 pkt 5.
3. Dostawa zewnętrzna nie zajmuje pojazdu ani rampy (§7.3) — tylko czas. M6 to urealnia.
