# M6c — Rynek B2B

Podfaza 3 z 5 fazy **M6 — Łańcuch dostaw** (`M6-lancuch-dostaw.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M6b (transport), M5 (`Offer`). |
| **Pakiety robocze** | WP7, WP8, WP9 |
| **Projekt techniczny** | §5.8, §5.9 |
| **Wynik do pokazania** | Niedobór lokalny domyka się kontraktem albo importem, a nie znikającym zapotrzebowaniem. |
| **Kryterium zamknięcia** | Kryteria WP7–WP9. |
| **Poprzednia / następna** | `M6b-zaklad-i-transport.md` · `M6d-zloza-i-koniec-dostawcy-zewnetrznego.md` |

Rynek spot, kontrakty terminowe i import/eksport przez węzły graniczne.

---

## Pakiety robocze

### WP7 — Rynek B2B: spot
**Zależy od:** WP5, M5 (`Offer`).
**Opis.** `Rfq`, `Quote`, okno zbierania ofert, indeks przestrzenny dostawców per `GoodId`, deterministyczne rozstrzygnięcie po funkcji celu. Koszt transportu wliczany w ocenę oferty.
**Kryterium ukończenia:** ten sam seed daje tę samą wybraną ofertę; zero iteracji po `HashMap`.
**Rozmiar: M**

### WP8 — Rynek B2B: kontrakty
**Zależy od:** WP7.
**Opis.** `SupplyContract`, `ContractPricing` (`Fixed` / `Indexed` / `Collar`), harmonogram dostaw z oknem godzinowym, kary za niedostarczenie i za nieodebranie, wypowiedzenie z okresem, statystyki realizacji zasilające reputację dostawcy.
**Kryterium ukończenia:** `prop_contract_penalty` — suma kar naliczonych równa sumie zapłaconych; indeksacja przelicza się całkowitoliczbowo bez dryfu.
**Rozmiar: M**

### WP9 — Import / eksport
**Zależy od:** WP8.
**Opis.** `TradeNode` (port, kolej, autostrada, rurociąg) z dobową przepustowością, kolejką i lead time; cena zewnętrzna rosnąca z wolumenem zakupów w oknie 30 dni; stub `TariffTable`; eksport jako zwykłe zlecenie transportowe **do** węzła (zajmuje ciężarówki i rampę) z ceną skupu niższą od importowej.
**Kryterium ukończenia:** import nie jest darmowym zaworem — test „zakup 10× dziennej przepustowości" kończy się kolejką i wzrostem ceny, nie natychmiastową dostawą; eksport przy wysokiej cenie zewnętrznej mierzalnie podnosi ceny lokalne.
**Rozmiar: M**

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.8 Rynek B2B

```rust
pub struct Rfq {
    pub id: RfqId, pub buyer: FirmId, pub deliver_to: SiteId,
    pub good: GoodId, pub mass: Mass, pub min_quality: Q,
    pub needed_by: SimMinute, pub radius_m: u32,
    pub opened_at: SimMinute, pub closes_at: SimMinute,   // domyślnie +60 min gry
    pub incoterm: WhoTransports,
    pub quotes: Vec<QuoteId>,
}
pub struct Quote {
    pub id: QuoteId, pub rfq: RfqId,
    pub seller: FirmId, pub from_site: SiteId,
    pub mass_available: Mass, pub quality: Q,
    pub price: Money,                    // za tonę / za 1000 szt., loco magazyn sprzedawcy
    pub transport: Option<Money>,        // gdy sprzedawca organizuje dostawę
    pub ready_at: SimMinute, pub eta: SimMinute,
    pub valid_until: SimMinute,
}
pub enum WhoTransports { Seller, Buyer }
```

Rozstrzygnięcie: minimalizacja `score = price*mass + transport + late_penalty*max(0, eta − needed_by) + quality_penalty*(min_quality − quality)`, remis rozstrzygany po `(seller.entity_index, quote.id)` — nigdy po kolejności wpisu do kolekcji. Dostawcy szukani przez indeks przestrzenny per `GoodId` (grid dzielnic, dok. 00 §3 i PRD §17.5) → typowo 3–12 kandydatów, nie wszystkie firmy w mieście.

```rust
pub struct SupplyContract {
    pub id: ContractId, pub buyer: FirmId, pub seller: FirmId,
    pub good: GoodId, pub min_quality: Q,
    pub deliver_to: SiteId, pub incoterm: WhoTransports,
    pub schedule: DeliverySchedule,       // co ile minut, jaka masa, okno godzinowe
    pub pricing: ContractPricing,
    pub penalty: Penalty,
    pub valid_from: SimMinute, pub valid_to: SimMinute,
    pub notice_minutes: u32,
    pub fulfilled_mass: Mass, pub missed_mass: Mass, pub late_deliveries: u32,
}

pub enum ContractPricing {
    Fixed   { price: Money },
    Indexed { base: Money, index: GoodId, ref_price: Money, pass_through_pct: u8 },
    Collar  { base: Money, index: GoodId, ref_price: Money, floor: Money, cap: Money },
}
pub struct Penalty { pub per_tonne_missed: Money, pub cap_pct: u8, pub grace_minutes: u32 }
```

**Baza cenowa (K-7, dok. 00 §4a) — wiążące.** Mechanizm ofert jest **jeden**, wspólny z M5; M6 **nie tworzy drugiego typu oferty dla B2B**. `Quote` jest widokiem na `Offer` z `price_basis: PriceBasis::NetB2B` — rozróżnienie robi jawne pole, nie osobny typ.

Cena w `Offer` to zawsze kwota płacona przez kupującego: w detalu brutto (`GrossRetail`), w hurcie netto (`NetB2B`, VAT jest dla firmy przelotowy). **Cały rynek B2B M6 — `Quote::price`, `ContractPricing` we wszystkich trzech wariantach, `import_quote`, `export_price`, `TransportOrder::price` — operuje netto.**

Konsekwencje dla łańcucha: `Batch::cost_total` jest kosztem netto i marże liczą się od netto. **Cła i akcyza nie modyfikują ceny w ofercie** — doliczają się przez `ChargeRegistry` po stronie M8. Znaczy to, że `TradeGood::tariff_class` jest wyłącznie etykietą przekazywaną do `ChargeRegistry`, a `import_quote` zwraca cenę netto plus osobno rozpisane obciążenia, nigdy kwotę „z cłem w środku". W łańcuchach referencyjnych (§7.1, §7.2) VAT i akcyza pojawiają się wyłącznie w ostatnim wierszu, na sprzedaży detalicznej.

Cena indeksowana przelicza się całkowitoliczbowo: `price = base + (index_now − ref_price) * pass_through_pct / 100`, a `Collar` dodatkowo klamruje do `[floor, cap]`. `index_now` to średnia cena spot towaru indeksowego w mieście z ostatnich 7 dni — liczona raz dziennie, cache'owana, iterowana po posortowanym kluczu.

Statystyki `fulfilled_mass` / `missed_mass` / `late_deliveries` są jedynym wyjściem M6 do reputacji dostawcy; jej interpretacja i pamięć relacji to M7/M10.

### 5.9 Import, eksport, węzły graniczne

```rust
pub struct TradeNode {
    pub id: TradeNodeId, pub kind: TradeNodeKind,   // Port | Rail | Highway | PipelineHead | Airport
    pub site: SiteId,                               // fizyczne miejsce w mieście
    pub capacity_per_day: Mass,
    pub used_today: Mass, pub backlog: Mass,
    pub base_lead_minutes: u32,                     // dni–tygodnie
    pub goods: Vec<TradeGood>,
}
pub struct TradeGood {
    pub good: GoodId,
    pub base_price: Money,                 // za tonę; trend epoki i zdarzenia globalne -> M8/M10
    pub elasticity_permille: u32,          // wzrost ceny przy dużych zakupach
    pub window_reference: Mass,            // wolumen odniesienia (30 dni)
    pub bought_window: Mass,
    pub export_spread_pct: u8,             // cena skupu jako % ceny importowej (typowo 78–90)
    pub tariff_class: TariffClassId,
}
```

- **Cena importu:** `p = base_price * (1000 + elasticity_permille * bought_window / window_reference) / 1000`. Kupno 10× wolumenu odniesienia przy `elasticity = 300` podnosi cenę o 300%. Import **nie jest** darmowym zaworem bezpieczeństwa.
- **Czas:** `lead = base_lead_minutes * (1 + backlog / capacity_per_day)`. Przeciążony węzeł wydłuża kolejkę wszystkim.
- **Cło:** `TariffTable` — w M6 stub z `data/trade/tariffs.ron`, M8 podmienia na politykę miasta i epoki.
- **Eksport:** producent porównuje `p_export = base_price * export_spread_pct/100 − koszt_transportu_do_węzła` z najlepszą ceną lokalną. Jeśli eksport wygrywa, powstaje zwykły `TransportOrder` **do** węzła — zajmuje ciężarówkę, kierowcę i rampę, i **zabiera masę z lokalnej podaży**. Wzrost cen w mieście jest emergentny, nie zaprogramowany. To jest cała mechanika „drenażu" z §8.5.

---

## Zmiany wpisane po M6b

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu podfazy M6b.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| AG-1 ★ | **Fala A ma dokładnie jeden substytut w całym katalogu** (`feed_bran` zamiast `food_flour_t550` w `bakery_bread_wheat`, dopisany w M6b). `Good::substitutes` jest puste dla **wszystkich** towarów, a `RecipeInput::substitutes` dla wszystkich wejść poza tym jednym. Wypełnienie należy do fali B, czyli tutaj | Konsekwencja nie jest kosmetyczna: szczebel `Substituted` kaskady niedoboru (§5.7) był nieosiągalny dla każdego towaru, czyli jedna szósta maszyny stanów z PRD §8.4 nie miała jak się wydarzyć. Reguła `R6` („≥ 3 towary na kategorię potrzeby") nie jest spełniona dla **żadnej** z 59 kategorii, więc ostrzeżenie `CriticalInputWithoutSubstitute` z reguły 5 walidatora zapala się na wszystkim — a ostrzeżenie dotyczące każdego przypadku jest tłem, nie sygnałem. **Gdzie wpisywać:** substytutu szuka się najpierw w `RecipeInput`, a dopiero potem przy towarze, bo to proces przyjmuje zamiennik, nie towar sam w sobie — i to tam jest `min_quality`, z którym zamiennik musi się zgadzać |
| AG-2 | **`RfqId` już istnieje: `magnat_supply::shortage::RfqId`.** Zadeklarował go M6b, bo kaskada niesie uchwyt zapytania ofertowego w `ShortageStage::SpotSearch`. WP7 **przejmuje ten typ**, a nie definiuje drugiego | Ta sama droga, którą M6a zadeklarował `LineId` i `TransportOrderId` na użytek M6b: typ mieszka tam, gdzie pierwszy konsument, a właściciel przejmuje go bez przenumerowania. Duplikat rozjechałby się przy pierwszej zmianie i dałby dwa nieporównywalne uchwyty do tego samego zapytania |
| AG-3 ★ | **Wejściem rynku jest lista `ShortageAction`, nie trait.** `shortage::review` zwraca `OpenRfq { site, good, mass }` i `Import { site, good, mass }`; WP7 i WP9 mają je konsumować. Dopóki nikt ich nie obsługuje, drabina wchodzi szczebel wyżej — czyli zachowuje się dokładnie tak, jak ma się zachować spot bez wyniku | Trait z jedną atrapą byłby abstrakcją bez drugiego konsumenta (YAGNI z `CLAUDE.md`), a przy okazji odwróciłby kierunek zależności: kaskada musiałaby znać rynek. Akcje idą w drugą stronę i to jest właściwa strona — zakład mówi, czego potrzebuje, rynek decyduje, skąd to wziąć |
| AG-4 ★ | **Import w kaskadzie ma na razie stałe ETA 8 h**, wpisane wprost w `shortage::zbuduj` z komentarzem `ponytail:`. WP9 podmienia to na `lead_minutes` z `TradeNode` razem z kolejką i ceną rosnącą z wolumenem | Zapisane, bo to jest liczba, która wygląda jak kalibracja, a nie jest nią: dopóki węzła granicznego nie ma, ETA importu nie ma z czego wynikać, a wpisanie jej do `data/tuning/supply.ron` sugerowałoby, że wolno ją stroić. Po WP9 ma **zniknąć**, a nie przenieść się do danych |
| AG-5 | **`Good::tariff_class` nadal nie istnieje** (`AD-4`), a `Good::import_via: Vec<GateKind>` — owszem, i jest wypełnione w katalogu fali A | Bez zmian wobec `AD-4`; zapisane, bo WP9 jest pierwszym pakietem, który obu tych pól dotyka, i warto wiedzieć, że jedno zastaje gotowe, a drugie zakłada |
