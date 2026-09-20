# M10d — Giełda, przejęcia, ubezpieczenia

Podfaza 4 z 6 fazy **M10 — Głębia** (`M10-glebia.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M7 (finanse firmy), M5 (banki). |
| **Pakiety robocze** | WP10.10, WP10.11, WP10.12 |
| **Projekt techniczny** | §5.5 |
| **Wynik do pokazania** | Notowanie powstaje z arkusza zleceń, a wrogie przejęcie da się przeprowadzić i obronić. |
| **Kryterium zamknięcia** | Kryteria WP10.10–WP10.12. |
| **Poprzednia / następna** | `M10c-rd-i-nowe-produkty.md` · `M10e-relacje-i-zwiazki.md` |

`Share`, `OrderBook` z fixingiem i `Valuation`, przejęcia, dywidendy, emisje i ład korporacyjny oraz ubezpieczenia.

---

## Pakiety robocze

### WP10.10 — Giełda: `Share`, `OrderBook`, fixing, `Valuation`

**Zależności:** M7 (księgowość firmy, raporty kwartalne), M5 (majątek GD).
**Kryterium ukończenia:**
- Fixing dzienny wyznacza jedną cenę maksymalizującą wolumen; przy remisie — cena najbliższa
  poprzedniemu fixingowi, przy dalszej remisie — niższa. Test na 10 tys. losowych ksiąg zleceń.
- Racjonowanie na marginesie pro rata; reszta do pierwszego wg posortowanego `OrderId` (dok. 00 §2).
  Test: suma przydzielonych akcji == wolumen, zawsze.
- Wycena reaguje na publikację wyników z opóźnieniem T+45 dni, nie wcześniej — test, że kurs nie
  porusza się przed publikacją, jeśli nie ma plotki.
- Suma akcji w obiegu == `Share.total_shares` po każdym fixingu, emisji i przejęciu (test własnościowy).
**Rozmiar: L.**

---

### WP10.11 — Przejęcia, dywidendy, emisje, ład korporacyjny

**Zależności:** WP10.10.
**Kryterium ukończenia:** przekroczenie 5% publikowane (→ plotka), przekroczenie 50% zmienia zarząd
i **podmienia osobowość firmy z M7 na osobowość przejmującego** — obserwowalna zmiana polityki
cenowej w ciągu miesiąca gry. Dywidenda dzieli kwotę bez utraty ani jednego grosza (test własnościowy
na 1000 losowych struktur akcjonariatu). Emisja rozwadnia proporcjonalnie, prawo poboru działa.
**Rozmiar: M.**

---

### WP10.12 — Ubezpieczenia

**Zależności:** M8 (`sim/events` jako źródło szkód), M7 (firma ubezpieczeniowa jako typ).
**Kryterium ukończenia:** składka za ubezpieczenie od powodzi w dzielnicy nadrzecznej po dwóch
powodziach w oknie 60 miesięcy jest ≥ 2× wyższa niż w dzielnicy wyżej położonej, **bez żadnego
parametru „ryzyko powodzi" w danych** — wyłącznie z historii zdarzeń. Ubezpieczyciel z ekspozycją
< 100 polis używa priora miejskiego (wygładzanie Bühlmanna) — test na małej próbce, że składka nie
skacze o rząd wielkości po jednej szkodzie.
**Rozmiar: M.**

---

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.5 Giełda, ubezpieczenia

```rust
pub struct Share {                    // klasa akcji firmy, nie pojedyncza akcja
    pub firm: FirmId,
    pub total_shares: u32,
    pub float: u32,                   // w wolnym obrocie
    pub listed_since: Option<SimMinute>,
    pub last_fixing: Money,           // cena za akcję
}
pub struct Holding { pub holder: HolderId, pub firm: FirmId, pub shares: u32 } // rzadkie

pub struct Order {
    pub id: OrderId, pub holder: HolderId, pub firm: FirmId,
    pub side: Side, pub limit: Money, pub qty: u32, pub expires: SimMinute,
}
pub struct OrderBook { pub firm: FirmId, pub buys: Vec<Order>, pub sells: Vec<Order> }

pub struct Valuation {
    pub fundamental: Money,   // z opublikowanych wyników
    pub belief: Money,        // fundamental × szum inwestora × korekta z plotek
    pub confidence: Q,
    pub reason: DecisionReason,
}

pub struct InsurancePolicy {
    pub id: PolicyId, pub insurer: FirmId, pub insured: PolicyHolder,
    pub peril: PerilKind, pub sum_insured: Money, pub deductible: Money,
    pub premium_monthly: Money, pub window: (SimMinute, SimMinute),
}
```

**Fixing zamiast notowania ciągłego.** Jeden fixing na dobę gry, o 17:00: cena maksymalizująca
wolumen dopasowany; remis → cena najbliższa poprzedniemu fixingowi; dalszy remis → niższa.
Racjonowanie na marginesie pro rata, reszta do pierwszego wg posortowanego `OrderId` (dok. 00 §2).
> *ponytail: notowanie ciągłe z ciągłą podwójną aukcją to O(zlecenia) mutacji stanu na tick i koszmar
> determinizmu przy równoległości. Fixing dzienny daje ten sam efekt gospodarczy dla miasta
> z ~3000 firm i ~5000 inwestorów, przy jednej deterministycznej redukcji na dobę.*

**Skąd bierze się cena (§6.5).** Wyłącznie z transakcji, nigdy z formuły wpisanej do stanu:

```
fundamental = max(wartość_księgowa, zysk_12m_opublikowany × mnożnik(branża, stopa, wzrost))
belief      = fundamental × (1 + szum(inwestor, firma, miesiąc)) × (1 + korekta_z_plotek)
```
Wyniki publikowane **kwartalnie z opóźnieniem T+45 dni i szumem** zależnym od jakości raportowania
firmy. Inwestor nie zna prawdziwego zysku — zna opublikowany. Szum `σ` zależy od wyrafinowania
inwestora: fundusz AI 5%, zamożny mieszkaniec 15–25%. Plotki (§5.7) przesuwają `belief` zanim
wyniki wyjdą — i to jest cała mechanika „ktoś wiedział wcześniej".

**Inwestorzy:** (a) mieszkańcy z majątkiem > progu (~1–2% populacji, decyzja `EveryMonth`),
(b) fundusze AI (5–15 encji, `EveryWeek`), (c) gracz. Skłonność do ryzyka bierze się z cechy
„Ryzyko" z §5.1 — bez nowego parametru.

**Przejęcia.** Próg 5% → obowiązek publikacji → `Rumor` + `Story`. Próg 50% → zmiana zarządu →
**podmiana `FirmPersonality` z M7 na osobowość przejmującego**. Wrogie: wezwanie z premią
(zwykle 15–35%), akcjonariusze porównują z `belief`. Przyjazne: negocjacje z zarządem i głównymi
akcjonariuszami, premia niższa, ale wyższa skuteczność.

**Ubezpieczenia — wycena wyłącznie z historii (§6.5).** Ubezpieczyciel prowadzi okno 60 miesięcy:

```rust
pub struct PerilStats { pub peril: PerilKind, pub scope: Scope, // District | City
                        pub exposures: u32, pub claims: u32,
                        pub loss_total: Money, pub sum_total: Money }
```

```
własna_stawka = loss_total / max(sum_total, 1)                       // szkodowość w bps
stawka  = (n × własna_stawka + K × prior_miejski) / (n + K)          // K = 100 polis
składka = div_round_half_up(stawka × sum_insured × (10000 + narzut_bps), 10000 × 12) + opłata
```

Wygładzanie (Bühlmann-lite, jedna linijka) jest konieczne, bo bez niego nowy ubezpieczyciel
po pierwszej szkodzie wycenia składkę na poziomie sumy ubezpieczenia i wypada z rynku w jednym kroku.
Po katastrofie ubezpieczyciel może być niewypłacalny; cesja X% ryzyka do „reszty świata" (§6.2 warstwa 3)
jest jedyną reasekuracją, jaką modelujemy.
> *ponytail: brak modelu reasekuracji jako rynku. Dodać, gdy gracz będzie mógł założyć reasekuratora —
> nie wcześniej.*


---

## Korekty planu wpisane w trakcie M10d

Zgodnie z `K-18` i regułą „popraw plan, zanim napiszesz kod". Gwiazdka = zmiana
zakresu albo kryterium. Numeracja `GD-n`.

| # | Co było w planie | Co jest i dlaczego |
|---|---|---|
| GD-1 ★ | `Share { total_shares, float }` i `Holding { holder, firm, shares }` jako nowe struktury (§5.5) | **Nie powstają** i to nie jest cięcie zakresu, tylko usunięcie drugiej prawdy. Własność firmy liczy od M7a `Firm.owners` w **punktach bazowych**, suma wynosi dokładnie 10 000 i pilnuje jej `Firm::owners_sum_ok`. Giełda obraca punktami bazowymi: kurs jest ceną **jednej bp**, próg ujawnienia to 500 bp, próg kontroli 5 000 bp, a emisja rozwadnia przez przeskalowanie udziałów. Kryterium WP10.10 „suma akcji w obiegu == `total_shares`, 0 akcji" **jest** tym istniejącym niezmiennikiem i ma test od M7a. `Listing` niesie wyłącznie to, czego `Firm` nie ma: kiedy notowana, ostatni fixing, ile bp w wolnym obrocie. Rozstrzygnięcie w 00 §4a jako `K-85` |
| GD-2 | `OrderId` jako uchwyt zlecenia giełdowego (§5.5) | **`StockOrderId`.** `OrderId` jest zajęty od M5 przez zamówienie zakupowe (`economy::supply::OrderId`), a dwa typy o jednej nazwie w jednym crate'cie nie mogą istnieć |
| GD-3 ★ | „Wyniki publikowane **kwartalnie** z opóźnieniem T+45 dni" (§5.5) | **Miesięcznie z opóźnieniem T+45 dni.** Raportów kwartalnych w grze nie ma i nie było: rozliczenie jest miesięczne (`SitePnlMonth` w pierścieniu 36 miesięcy), a kwartał nie ma w kodzie żadnego nośnika. Opóźnienie zostaje bez zmian i jest dokładnie tym, czym miało być: wynik miesiąca `m` publikuje się w dobie `(m + 1) × 30 + 45`, więc kryterium „kurs nie porusza się przed publikacją" jest mierzalne |
| GD-4 ★ | „Zależności: M8 (`sim/events` **jako źródło szkód**)" (WP10.12) | **Takiego źródła nie było.** Sześć wariantów `Effect` to odwracalne mnożniki parametrów — `ParamOverlay` pamięta wartość zastaną i przywraca ją przy wygaśnięciu zdarzenia. Szkoda majątkowa jest jednorazowa i nieodwracalna, więc siódmym wariantem `Effect` być nie może. `EventDef` dostaje osobne pole `peril: Option<PerilKind>` (schema 1 → 2), a rejestr zdarzeń **zgłasza** wystąpienie do `Insurers::report_peril` — piszący stoi nad czytelnikiem i sięga do jego pola, tak jak przy `ServiceCoverage` (`K-64`). Rozstrzygnięcie w 00 §4a jako `K-86` |
| GD-5 ★ | „Składka za ubezpieczenie **od powodzi** w dzielnicy nadrzecznej…" (kryterium WP10.12) | **Powodzi w katalogu zdarzeń nie było**, więc M10d ją dopisuje: `natural/flood`, zakres dzielnicowy, hazard z **nadmiaru** opadu (sonda `PrecipDeficit30dChm` czytana po ujemnej stronie — osobnej sondy „opad" nie ma i nie jest potrzebna). Czego **nie ma i nie będzie**: liczby mówiącej, która dzielnica jest nadrzeczna. To jest istota kryterium: dzielnice różnicuje **historia szkód**, a nie tabela ryzyka, i pilnuje tego test porównujący dwie dzielnice różniące się wyłącznie przebiegiem zdarzeń |
| GD-6 | Szkoda jako ubytek majątku — plan nie mówił, czego | **Zapasu sklepu**, tą samą drogą co kradzież z M8d (`Store::write_off` + `WriteOffExpense`), więc bilans masy (00 §6) domyka się sam: towar schodzi do `LossKind::Disaster`, a nie znika. `ponytail:` sufit nazwany w kodzie — szkoda nie dotyka wyposażenia ani magazynów zakładów produkcyjnych, bo slotów fabryki `Market` nie zna. Droga wyjścia prowadzi przez rejestr zakładów M6 |
| GD-7 | `PolicyId` jako uchwyt polisy (§5.5) | **`CoverId`.** `PolicyId` jest zajęty od `K-47` przez politykę silnika reguł i znaczy coś zupełnie innego; przemianowanie tamtego przepisałoby edytor reguł M9 |
| GD-8 ★ | „Inwestorzy: (a) mieszkańcy, (b) **fundusze AI (5–15 encji)**, (c) gracz" (§5.5) | **Funduszy nie ma.** Inwestorami są zamożne gospodarstwa, firmy kupujące konkurenta i gracz — i to jest pełna lista, bo fundusz bez zakładu, bez załogi i bez księgi byłby czwartym rodzajem `Owner`, którego nikt poza modułem giełdy by nie widział. `ponytail:` sufit nazwany: gdy panel giełdy (M10f) pokaże, że płynność jest za mała, fundusz dopisuje się jako wariant `Owner` razem ze swoim kontem, a nie jako wyjątek w giełdzie |
| GD-9 ★ | „Wrogie: **wezwanie z premią**; przyjazne: negocjacje z zarządem" (§5.5) | **Wezwanie jest zleceniem z limitem powyżej kursu** i przechodzi tym samym fixingiem co każde inne — osobnej ścieżki nie ma. Akcjonariusze porównują je z własnym przekonaniem dokładnie tak, jak plan opisywał, tylko dzieje się to w arkuszu, a nie w osobnym mechanizmie. Negocjacji z zarządem **nie ma**: zarząd w tej grze nie jest bytem, z którym da się rozmawiać — jest polem `director` i zestawem cech, więc „przyjazne przejęcie" byłoby wezwaniem z niższą premią i niczym więcej |
| GD-10 ★ | Obrona przed przejęciem — „wynik do pokazania" podfazy, bez mechanizmu w §5.5 | **Emisja obronna**: notowana firma, w której ktoś obcy przekroczył ćwierć udziałów, a największy właściciel nie ma jeszcze kontroli, dosypuje mu udziału po kursie ostatniego fixingu. Pieniądz idzie **do spółki** i to jest cała różnica wobec debiutu, gdzie trafia do sprzedających. To zarazem jedyny wołający emisji w symulacji — kanał gracza dokłada M10f |
| GD-11 | Plan nie mówił, skąd bierze się plotka ruszająca kurs przed publikacją | **Z ujawnienia pakietu.** Przekroczenie 5 % podnosi przekonanie inwestorów o firmie na siedem dób i gaśnie liniowo. Podpięcie do grafu plotki M3 wymagałoby, żeby `sim/economy` widziało pamięć mieszkańca po stronie zapisu, a widzi ją po stronie odczytu; kanał do M10f zostaje otwarty skrzynką `Equity::take_disclosures`, z której czyta `sim/media` |
| GD-12 ★ | Znalezione poza zakresem: **`LedgerAccount::MarketingExpense` nie było kontem wynikowym** | M10b dopisał konto reklamy, ale nie dopisał go do `is_profit_and_loss`, więc **nie domykało się na koniec miesiąca** i `IncomeStatement::net_result()` przestało równać się zmianie `RetainedEarnings` w każdym zakładzie, który się reklamował — czyli kryterium WP7 fazy M5 było fałszywe od M10b i nie złamało żadnego testu, bo `Σ sald == 0` trzymało się dalej. Naprawione razem z dołożeniem `InsuranceExpense`: oba konta wchodzą do rachunku wyników, obie pozycje są w `IncomeStatement` i obie widać w panelu sklepu — liczba, która nie zgadza się z pozycjami nad nią, wygląda dla gracza jak błąd programu |
| GD-13 | „Ubezpieczyciel prowadzi **okno 60 miesięcy**" (§5.5) | **Okno jest wykładnicze, nie prostokątne**: każdy miesiąc mnoży szkodowość przez `1 − 1/60`, więc średni wiek obserwacji wynosi sześćdziesiąt miesięcy. Prostokątne wymagałoby sześćdziesięciu kubełków na parę (ryzyko, dzielnica) — tablicy rosnącej z liczbą dzielnic razy sześćdziesiąt, w całości w hashu stanu — za różnicę, której gracz nie odróżni od wygładzenia. Skutek jest ten, o który chodziło, i ma test: **powódź sprzed dekady nie trzyma składki w górze** |
| GD-14 ★ | Plan nie mówił, co się dzieje ze zleceniem bez pokrycia | **Przycina się przed fixingiem**: sprzedający do pakietu, który ma, kupujący do kwoty, którą ma na koncie. Bez tego zlecenie sprzedaży większe od pakietu dawałoby wolumen, za który kupujący **zapłacił**, a którego nie ma z czego przenieść — pieniądz domykałby się co do grosza, a akcjonariat nie. Taki jest też porządek prawdziwej giełdy: pokrycia sprawdza się przy przyjęciu zlecenia, nie przy rozliczeniu |
| GD-15 | Plan nie mówił, jak często fixing trafia do dziennika decyzji firmy | **Tylko wtedy, gdy ruszył kurs.** `Firm.log` ma 32 wpisy i pokazuje decyzje; dobowy fixing jest stanem rynku, więc dopisywany co dobę wyparłby z pierścienia wszystko inne w miesiąc i karta firmy przestałaby odpowiadać na „dlaczego". Kurs, także niezmieniony, czyta się z `Equity::listing` |
| GD-16 ★ | Znalezione przez recenzję: **ubezpieczyciel nie miał konta w księgach** | Konta otwierają dziś dwie drogi — sklepom (`world::retail`) i zakładom z linią produkcyjną (`world::plants`) — a biuro nie jest ani jednym, ani drugim. `Market::account_of_firm` zwracał `None`, więc składka nie szła, ekspozycja zostawała zerem i stawka na zawsze równała się priorowi: **polisy powstawały, a nie ruszał się ani grosz**. Żaden test tego nie łamał, bo testy dopinały konto ręcznie — ta sama klasa błędu co martwa ładowarka `brand.ron` w M10b i nienadpisany `excise_on` w M8b. Naprawa: `stand_up_insurers` w moście stawiającym świat, wołane raz, z kapitałem otwarcia równym kapitałowi zakładu produkcyjnego |
| GD-17 ★ | Znalezione przez recenzję: **pieniądz ginął na granicy sektora gospodarstw** | Trzy miejsca, jeden wzorzec: zdjęcie z komponentu `Household` już się stało, a odmowa po stronie ksiąg nie zwracała kwoty; i odwrotnie — `household_receive` wypuszczał pieniądz z ksiąg, a `dodaj_do_gospodarstwa` milczało dla mieszkańca bez gospodarstwa. **`Books::check_conservation` tego nie widzi**, bo suma sald się zgadza; widzi to dopiero test liczący księgi **plus** komponenty. Naprawa: zwrot dokładnie tam, skąd wzięto (osobno oszczędności, osobno rachunek), i sprawdzenie gospodarstwa **przed** przelewem |
| GD-18 ★ | Znalezione przez recenzję: **emisja obronna psuła pamięć progów** | `dilute` przestawia bp **wszystkim** właścicielom, a pamięć progów aktualizowała się tylko dla stron sesji. Po obronie zapamiętany pakiet napastnika zostawał trwale zawyżony, więc próg 5 % i 50 % nie odpalał się dla niego **już nigdy** — ujawnienie i zmiana zarządu przestawały dla tej pary istnieć. Naprawa: `odswiez_progi` przepisuje pamięć całego akcjonariatu po każdej zmianie spoza sesji |
| GD-19 ★ | Znalezione przez recenzję: **przejęcie przez mieszkańca nie zmieniało cech firmy** | `check_thresholds` nie widzi komponentu `Personality`, więc zostawiało to `refresh_personalities` — a ta funkcja ma w całym repozytorium dwóch wołających i obaj stawiają świat, nie grają w nim. Kryterium WP10.11 („podmienia osobowość firmy na osobowość przejmującego") było więc spełnione **tylko** wtedy, gdy przejmującym była firma. Naprawa: funkcja zwraca nowego kontrolującego, a cechy mieszkańca dopisuje wołający, który świat widzi |
| GD-20 | Znalezione przez recenzję: **sprzedający bez kanału wypłaty oddawał udział za darmo** | Firma bez konta w `Market` przechodziła przez fixing, traciła pakiet, a pieniądz zostawał na koncie przelotowym. Naprawa: pokrycie sprawdza się po **obu** stronach — sprzedający musi mieć i pakiet, i kanał wypłaty. `Owner::External` i `City` konta nie mają i mieć nie będą, ale tam pieniądz zostaje w **reszcie świata**, czyli na prawdziwym koncie, więc sprzedawać im wolno — tą drogą wychodzi na rynek pakiet założycielski przy debiucie |

---

## Zmiany wpisane po M10b

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M10b.
Szczegóły — tabela `F-n` w `M10b-marka-i-media.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| FD-1 | **Nowy przepływ pieniądza = nowy wariant `TxKind`, dopisany na końcu** (`K-80`). M10b dołożył `AdSpend`; obrót akcjami, dywidenda, składka i wypłata ubezpieczeniowa idą tą samą drogą | `Books::transfer` wymaga rodzaju zapisu, a wciśnięcie obrotu akcjami w `WholesalePurchase` zafałszowałoby bramkę G9 balansatora, która dzieli zapisy na wybory i zobowiązania. Przy okazji: **`jest_decyzja` w `tools/balansator` jest wyczerpujący i nie ma gałęzi `_`**, więc nowy wariant nie skompiluje się bez rozstrzygnięcia, po której stronie stoi |
| FD-2 | **Plan kont `LedgerAccount` też rośnie na końcu.** M10b dołożył `MarketingExpense` | Kolejność wariantów indeksuje tablicę sald, która wchodzi do hasha stanu i do zapisu gry. Rozdział ról zostaje bez zmian: **księga zakładu mówi, na co poszło, dziennik transakcji — do kogo** (`K-57`, `K-80`) |
| FD-3 ★ | **Wycena firmy ma już jedno wejście, którego wcześniej nie miała: `MacroFirm.brand_stock`** przestał być zerem (`F-14`) i niesie renomę marki ważoną znajomością, w skali −100..100 | §6.5 PRD chce wyceny „z opóźnionych i zaszumionych wyników plus plotek". Marka jest tym składnikiem wyceny, który **nie** jest wynikiem finansowym, i od M10b jest realną liczbą, a nie polem w strukturze |
| FD-4 | **Blok `StreamId` M10: zajęte 280–284 i 292–295; wolne 285–291 i 296–299.** `InvestorNoise = 286`, `EarningsNoise = 287`, `PerilDraw = 288` z tabeli M10 §7.1 są nadal wolne i zarezerwowane imiennie | — |

---

## Zmiany wpisane po M10c

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu
M10c. Gwiazdka = zmiana zakresu albo kryterium. Szczegóły — tabela `FD-n`
w `M10c-rd-i-nowe-produkty.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| FD-5 ★ | **`SitePnlMonth.fixed` niesie od M10c także koszt badań** (`Site::rnd_accrued`, domykany razem z czynszem przy liście płac). Wycena firmy liczona z marży zakładu widzi więc pełny koszt operacyjny, a nie sam czynsz z katalogu typów | Budżet materiałowy laboratorium i opłaty licencyjne są kosztem **operacyjnym**, nie kadrowym: pensje badaczy siedzą już w `labor`. Dla M10d znaczy to jedno i warto o tym wiedzieć przed wyceną: firma prowadząca badania ma niższą marżę **dziś** i wyższą wartość **jutro**, a `margin_bp()` widzi tylko pierwszą połowę |
| FD-6 | **`ContractId` mintuje wyłącznie `magnat_supply::B2b::mint_contract_id`** — także dla umów, których `sim/supply` nie prowadzi. M10c wydaje tą drogą numery licencjom patentowym i trzyma ich rejestr u siebie | Drugi licznik dałby dwie umowy o tym samym numerze i `Subject::Contract` prowadziłby raz tu, raz tam. Jeśli M10d zechce umowy inwestorskiej z własnym numerem, bierze go stamtąd — a rejestr prowadzi u siebie |
| FD-7 | **Blok `DecisionReason` M10: zajęte 800–807.** Wolne: **808–899**. `StreamId` M10: zajęte 280–285 i 292–295; wolne **286–291** i **296–299** | M10c wziął jeden numer strumienia (`RnDBreakthrough = 285`) i cztery powody (804–807) |
| FD-8 ★ | **Opłata licencyjna trafia na konto licencjodawcy, ale nie do jego rachunku wyniku** (`FD-21` w M10c). `SitePnlMonth::revenue` właściciela patentu nie rośnie, bo `Firms::post_revenue` ma jednego wołającego i `K-75` każe mu przy nim zostać | Dla wyceny firmy to jest różnica widoczna gołym okiem: firma żyjąca z patentów ma pieniądze na koncie i **zero** w marży, więc wycena licząca z `margin_bp()` policzy ją jako nierentowną. Droga wyjścia prowadzi przez drugie **wejście** do rachunku wyniku, a nie przez drugiego wołającego `post_revenue` |
