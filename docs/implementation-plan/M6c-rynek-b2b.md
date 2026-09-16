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
| **Kryterium zamknięcia** | Kryteria WP7–WP9. **Spełnione** — `cargo test -p magnat-supply --test b2b`, 16 testów. |
| **Poprzednia / następna** | `M6b-zaklad-i-transport.md` · `M6d-zloza-i-koniec-dostawcy-zewnetrznego.md` |

Rynek spot, kontrakty terminowe i import/eksport przez węzły graniczne.

---

## Pakiety robocze

### WP7 — Rynek B2B: spot `[x]`
**Zależy od:** WP5, M5 (`Offer`).
**Opis.** `Rfq`, `Quote`, okno zbierania ofert, indeks przestrzenny dostawców per `GoodId`, deterministyczne rozstrzygnięcie po funkcji celu. Koszt transportu wliczany w ocenę oferty.
**Kryterium ukończenia:** ten sam seed daje tę samą wybraną ofertę; zero iteracji po `HashMap`.
**Rozmiar: M**

### WP8 — Rynek B2B: kontrakty `[x]`
**Zależy od:** WP7.
**Opis.** `SupplyContract`, `ContractPricing` (`Fixed` / `Indexed` / `Collar`), harmonogram dostaw z oknem godzinowym, kary za niedostarczenie i za nieodebranie, wypowiedzenie z okresem, statystyki realizacji zasilające reputację dostawcy.
**Kryterium ukończenia:** `prop_contract_penalty` — suma kar naliczonych równa sumie zapłaconych; indeksacja przelicza się całkowitoliczbowo bez dryfu.
**Rozmiar: M**

### WP9 — Import / eksport `[x]`
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

---

## Zmiany wpisane po M6c

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu podfazy M6c.
Gwiazdka = zmiana zakresu albo kryterium. Poprawki dotyczące **fazy** (kontrakty §6, decyzje
§9, testy §7) są w tabeli `AH` dokumentu `M6-lancuch-dostaw.md`; tutaj jest to, co dotyczy
samego §5.8 i §5.9.

| # | Zmiana | Dlaczego |
|---|---|---|
| AH-1 ★ | **`Quote` jest własnym typem, nie widokiem na `Offer`.** §5.8 mówiło „mechanizm ofert jest jeden, wspólny z M5; M6 **nie tworzy drugiego typu oferty dla B2B**" — i w tym samym akapicie wypisywało `struct Quote` z dziewięcioma własnymi polami. Rozstrzygnięte na korzyść struktury jako **`K-36`** w dokumencie 00 | Zdanie i struktura pod nim nie mogły być prawdziwe naraz. Trzy powody, każdy osobno wystarczający: `Offer` mieszka w `sim/economy`, a zależność idzie `economy → supply` i **nigdy odwrotnie**; `OfferIndex` warstwuje się po `StockCat`, a ruda żelaza nie ma `StockCat`, w którym mogłaby stanąć; `Offer` jest **stojącą** ceną półkową, a `Quote` odpowiedzią na **jedno** zapytanie, która umiera razem z nim. **Substancja `K-7` zostaje w całości:** cały rynek B2B liczy netto, rozróżnienie niesie jawna deklaracja, cła doliczają się osobno |
| AH-2 ★ | **Oferty siedzą w `Rfq.quotes: Vec<Quote>`, a nie w osobnej arenie z `Vec<QuoteId>`** (§5.8) | Drugiego właściciela oferty nie ma: nikt nie sięga po ofertę inaczej niż przez zapytanie, któremu odpowiada. Arena kosztowałaby uchwyt, generację i wejście do hasha bez ani jednego konsumenta. `QuoteId` **zostaje** — jest tie-breakiem rozstrzygnięcia i tożsamością w panelu, który pokazuje, kto przegrał i o ile |
| AH-3 ★ | **`TradeNodeKind` nie powstaje — rodzajem węzła jest `GateKind` z `core`.** Głowica rurociągu (`PipelineHead`) z §5.9 odpada razem z nim | `K-33` przeniósł `GateKind` do `core` **dokładnie po to**, żeby `Good::import_via` i węzeł graniczny mówiły o tym samym; drugi enum rozjechałby się przy pierwszej zmianie i unieważnił `import_via` w całym katalogu fali A. `PipelineHead` odpada, bo w fali A żaden rurociąg nie przechodzi granicy, a `Carrier::Pipeline` z M6b wozi ropę **wewnątrz** miasta i nie jest bramą |
| AH-4 ★ | **`TradeGood` nie ma pliku per towar; `base_price` podaje wołający przy rejestracji węzła, a `tariff_class` wyprowadza się z domeny klucza** (`data/trade/tariffs.ron`, `K-37`). `Good::tariff_class` nadal nie istnieje i nie powstanie (`AD-4`, `AG-5`) | §5.9 stawiało `tariff_class` przy `TradeGood` i tam zostaje — ale tabela czterystu wierszy „towar → klasa" starzeje się przy każdym nowym towarze i starzeje się **po cichu**: brakujący wiersz nie jest błędem ładowania, tylko towarem, którego nagle nie da się zaimportować. Domen jest dwanaście i lista jest zamknięta (`AD-12`), więc przypisanie przez domenę daje pokrycie **z definicji**, a testowi zostaje sprawdzenie, że żadna domena nie została bez klasy |
| AH-5 ★ | **Kara w funkcji celu RFQ i kara umowna to dwie różne liczby i nie wolno ich zlać.** `late_penalty` oraz `quality_penalty` z §5.8 są **wagami wyboru** w `data/tuning/supply.ron` — nikt ich nie płaci; `Penalty { per_tonne_missed, cap_pct, grace_minutes }` jest pieniądzem i wchodzi do księgi | Obie nazywają się karą i obie mają jednostkę pieniężną, więc pomylenie ich jest kwestią czasu, a skutek byłby taki, że kupujący płaciłby za to, że wybrał dostawcę odrobinę wolniejszego. Zapisane, bo `prop_contract_penalty` pyta **tylko** o tę drugą i przeszedłby bez zmian, gdyby pierwsza zaczęła być księgowana |
| AH-6 ★ | **`cap_pct` liczy się od wartości całego kontraktu, nie jednej dostawy** (§5.8 nie mówiło, od czego) | Sufit od dostawy nie jest sufitem: kontrakt roczny byłby dwunastokrotnie droższy do zerwania niż miesięczny o tych samych warunkach, choć `cap_pct` w obu wynosiłby tyle samo. Test `kara_nie_przekracza_sufitu_kontraktu` sprawdza właśnie tę interpretację |
| AH-7 | **`valid_to` kontraktu jest wyłączne**: dostawa wypadająca dokładnie w minucie wygaśnięcia już się nie odbywa | Warunek brzegowy, który cicho przesuwa harmonogram o jedną dostawę, a widać go dopiero w rozliczeniu kary — kontrakt na „dziesięć dób po jednej dostawie" wystawia ich dziewięć. Wpisany wprost w test, żeby nie został odkryty przy pierwszym sporze o karę umowną |
| AH-8 ★ | **Eksport jedzie ciężarówką — wykonanie decyzji `D6` zgodnie z propozycją.** `try_export` wystawia i wysyła `TransportOrder` do węzła, a masa schodzi z bilansu jako `exported` dopiero w `B2b::absorb_exports`, po rozładunku w porcie | Odpisanie masy wprost z magazynu byłoby o czterdzieści linii krótsze i czyniłoby z drenażu podaży **fikcję księgową**: towar znikałby bez zajęcia pojazdu, kierowcy i rampy, więc eksport nie konkurowałby z dostawami o tę samą flotę — czyli dokładnie to, przed czym `D6` ostrzegało. Dwa kroki zamiast jednego, bo między decyzją a wyjazdem stoi przejazd, który może się nie udać: towar, który nie dojechał do portu, ma wrócić do podaży, a nie zniknąć |
| AH-9 | **`Store::export` jest trzecim wyjściem masy z magazynu**, obok `take` (zużycie w łańcuchu) i `write_off` (strata) | Bilans z 00 §6 rozróżnia `consumed`, `exported` i `losses`, a do M6c pole `exported` **nie miało ani jednego pisarza** — czyli jedna trzecia prawej strony niezmiennika była martwa i nikt by się nie dowiedział, gdyby liczyła źle. Zlanie eksportu ze zużyciem domknęłoby bilans i skłamało w rachunku, bo za eksport ktoś zapłacił |
| AH-10 ★ | **Zaślepki szczebli kaskady wypełnia rynek, nie kaskada.** `PlantSite::set_stage` wpisuje prawdziwy `RfqId` w `SpotSearch` i prawdziwe ETA w `Importing`; `ponytail:` ze stałą ośmiu godzin z M6b (`AG-4`) **zniknął**, a nie przeniósł się do danych | `AG-4` żądało dokładnie tego i to jest wykonane. Wzorzec jest ten sam, którym M6b zostawił `RfqId::default()`: kaskada buduje szczebel, rynek wypełnia ładunek w tej samej minucie. Strażnikiem jest test `rynek_wypelnia_zaslepki_kaskady_prawdziwymi_uchwytami`, który sprawdza obie zaślepki naraz — w tym jawnie, że ETA **nie** wynosi `now + 480` |
| AH-11 ★ | **`R6` zostaje niespełniona i przechodzi dalej z adresem. `AG-1` mówiło „wypełnienie należy do fali B, **czyli tutaj**" — to jest nieprawda i została poprawiona** | `K-34` w dokumencie 00 przypisuje falę B katalogu do **M7** („`NeedCategoryId` czyta M5 przy substytucji i M7 przy wypełnianiu fali B katalogu"), a M6c nie ma ani jednego pakietu roboczego, który byłby właścicielem katalogu — WP7–WP9 to rynek, a katalog to WP1, zamknięty w M6a. Podfaza, której kryterium brzmi „niedobór domyka się kontraktem albo importem", nie jest miejscem na czterysta towarów. **Co się przy okazji zmieniło na lepsze bez dotykania katalogu:** szczebel `Importing` przestał być zaślepką i jest realną alternatywą dla **każdego** towaru z `external_base_price`, więc kaskada ma teraz dwa działające wyjścia zamiast jednego. Adresat `R6`: M7 razem z falą B |
| AH-12 ★ | **Druga połowa kryterium WP9 — „eksport mierzalnie podnosi ceny lokalne" — nie jest w M6c mierzalna i została zawężona do tego, co da się sprawdzić: eksport zabiera masę z lokalnej podaży, a bilans domyka się z pozycją `exported`.** Pomiar ceny przechodzi do scenariusza `export_drains` w **M6e** (§7.7 dokumentu fazy), po WP11 | To nie jest ustępstwo, tylko fakt o kanale, którym cena rośnie. W M6c cena dostawcy jest **kosztowa**: koszt wytworzenia razy jedna marża z `data/tuning/supply.ron`. Nie ma w niej członu reagującego na zapas, bo presja zapasu jest mechanizmem **M5** (`reprice`), a M5 do WP11 kupuje u `ExternalSupplier` o nieskończonej podaży i stałej cenie — to jest dokładnie ta sama obserwacja, którą M5e zapisał jako `AD-3` („sterownik zapasu należy do M6 razem z realnym dostawcą"). Eksport ma więc dziś dokąd zabrać masę, ale nie ma jeszcze przez co podnieść ceny; ta pętla domyka się w chwili, gdy półka zaczyna brać towar z magazynu. Kryterium sformułowane tak, jak było, spełniłoby się w M6c **tylko pozornie** — albo przez dopisanie do wyceny członu, którego model nie ma |

