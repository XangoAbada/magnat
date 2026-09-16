# M7c — Polityki i menedżerowie

Podfaza 3 z 6 fazy **M7 — Firmy AI i rynek pracy** (`M7-firmy-ai-i-rynek-pracy.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M7a (model firmy), AST języka reguł od M9 (K-11). |
| **Pakiety robocze** | WP6b, WP7 |
| **Projekt techniczny** | §5.4, §5.11 |
| **Wynik do pokazania** | Reguła gracza i polityka firmy AI wykonują się tym samym kodem; różnica leży w jakości menedżera. |
| **Kryterium zamknięcia** | Kryteria WP6b i WP7. |
| **Poprzednia / następna** | `M7b-rynek-pracy.md` · `M7d-finanse-i-upadlosc.md` |

Crate `sim/policy` — ewaluator języka reguł M9 z zakresami stosowania — oraz menedżerowie, delegowanie i `FirmPolicy` jako zestaw reguł.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| [x] WP6b | `sim/policy`: ewaluator języka reguł M9, zakresy stosowania | WP1, AST od M9 | M |
| [x] WP7 | Menedżerowie, delegowanie, `FirmPolicy` jako zestaw reguł | WP3, WP6, WP6b | L |

### WP6b — `sim/policy`: ewaluator języka reguł (K-11)

**Język projektuje M9** — jego AST (`ConditionExpr`, `Expr`, `Metric`, `Action`, `PolicyScope`)
jest dla M7 wiążącą specyfikacją. M7 buduje **ewaluator** i rejestr metryk/akcji, nie drugi język.

Powód istnienia crate'a: PRD §6.3 obiecuje graczowi „ten sam zestaw narzędzi co AI". Gdyby
`FirmPolicy` była zaszyta w Ruście dla AI, a M9 zbudowała osobny interpreter reguł dla gracza,
ta sama reguła („−2% względem najtańszego konkurenta w promieniu 3 km") zachowywałaby się
inaczej u gracza i u konkurenta. Jeden ewaluator usuwa tę klasę błędów z definicji — i jest
drogą do moddowalnej AI w M12 (polityka staje się plikiem danych, nie kodem).

Zakres pracy M7: ewaluacja `ConditionExpr`/`Expr` w arytmetyce całkowitej (pieniądz i64,
reszta w milijednostkach — bez f32 na ścieżce wpływającej na stan trwały), rejestr `Metric`
(odczyty **wyłącznie** przez `FirmView` — reguła nie może czytać tego, czego nie widzi firma),
rejestr `Action` mapowany na `OpsAction`/`TacAction`, zakresy stosowania `PolicyScope`
(firma / zakład / sklep / produkt) z rozstrzyganiem konfliktów: najbardziej szczegółowy zakres
wygrywa, przy równej szczegółowości — niższy indeks reguły w liście.

*Kryterium ukończenia:* ta sama reguła zastosowana do zakładu gracza i do identycznego zakładu
AI daje identyczną akcję (test równoważności — §7.10); ewaluator jest deterministyczny i nie
alokuje na ścieżce gorącej; każda ewaluacja kończąca się akcją produkuje `DecisionReason`
wskazujący regułę, która się wyzwoliła.

### WP7 — Menedżerowie i delegowanie

`Manager`, `SiteDelegation`, `FirmPolicy`. **Jeden silnik polityk dla AI i dla gracza** (`sim/policy`,
WP6b) — różni się wyłącznie **źródłem** zestawu reguł (AI: tier taktyczny generuje reguły;
gracz: edytor z §14.6) i jakością wykonania (menedżer).
Jakość zarządzania modyfikuje produktywność, rotację i straty.

*Kryterium ukończenia:* test monotoniczności menedżera (§7); zakład zdelegowany menedżerowi działa
bez ingerencji gracza przez rok gry i nie degeneruje się (magazyn nie pustoszeje, ceny nie uciekają).

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.4 Menedżerowie i delegowanie (§7.5)

```rust
pub struct Manager {
    pub citizen: CitizenId,
    pub skill_mgmt: Q,                 // umiejętność „management" z §5.1
    pub style: ManagerStyle,           // Taskmaster | Coach | Bureaucrat | Dealmaker
    pub sites: SmallVec<[SiteId; 4]>,  // rozpiętość kierowania
    pub tenure: SimMinute,
}

pub struct SiteDelegation {
    pub manager: Entity,
    pub policy: FirmPolicy,            // TEN SAM typ, którego używa AI i gracz (§14.6)
    pub autonomy: Autonomy,            // PricesOnly | PricesAndStaff | Full
    pub report_freq: Freq,
}

/// Jakość zarządzania: rzadki zasób. Rozpiętość ponad optimum degraduje liniowo.
pub fn management_quality(m: &Manager, site: &Site, morale: Q) -> ManagementQuality;
```

Wpływ `ManagementQuality` — trzy niezależne kanały, wszystkie realne i mierzalne w balansatorze:

| Kanał | Efekt | Konsument |
|---|---|---|
| Produktywność | `mgmt_mult` 0,85–1,15 | `effective_labor` → M6 |
| Rotacja | mnożnik prawdopodobieństwa odejścia 1,4–0,7 | `hr::turnover` |
| Straty | mnożnik ubytku, psucia, braków 1,5–0,6 | hook strat w M6 (magazyn, produkcja) |

Dobry menedżer jest podkupywany: `bidding::headhunt` traktuje role menedżerskie z podwyższonym
priorytetem. Odejście menedżera zakładu natychmiast obniża `ManagementQuality` do wartości
zastępstwa (`Autonomy::PricesOnly`, jakość = mediana firmy − 20) — stąd „dobry menedżer to zasób
rzadki" ma konsekwencje, a nie tylko opis.

**Delegowanie jest jedynym sposobem skalowania gracza.** Gracz z 200 sklepami nie klika 200 razy:
ustawia `FirmPolicy` i przypisuje menedżera, a jakość wykonania polityki zależy od menedżera.
Ten sam kod realizuje decyzje firm AI — różni się tylko źródło polityki.

### 5.11 `DecisionReason` — jak konkretnie (dokument 00 §7)

Enum z parametrami, nie string. M7 dokłada wariant przestrzeni nazw firm:

```rust
// w engine/core
pub enum DecisionReason { Citizen(CitizenReason), Firm(FirmReason), City(CityReason) /* ... */ }

#[derive(Copy, Clone)]                 // <= 24 B — mieści się w pierścieniu
pub enum FirmReason {
    PriceCut   { good: GoodId, from: Money, to: Money, cause: PriceCause },
    PriceRaise { good: GoodId, from: Money, to: Money, cause: PriceCause },
    WageRaise  { role: JobRoleId, from: Money, to: Money, cause: WageCause },
    Hired      { applicant: CitizenId, score: i32, runner_up: i32 },
    Rejected   { applicant: CitizenId, score: i32, threshold: i32 },
    Poached    { from_firm: FirmId, role: JobRoleId, premium_bp: u16 },
    LaidOff    { role: JobRoleId, count: u16, cause: LayoffCause },
    QuitLost   { citizen: CitizenId, to_firm: Option<FirmId>, cause: QuitCause },
    TrainingStarted { role: JobRoleId, budget: Money, skill_gap: u8 },
    ManagerAssigned { site: SiteId, skill_mgmt: Q, prev_quality: u16 },
    // UWAGA: żadnej „prognozy zysku" jako kwoty — wynik makro jest obarczony 3–12% błędu.
    // Zapisujemy POZYCJĘ wariantu i margines, nie wartość (patrz §5.10).
    SiteOpened { site_type: SiteTypeId, variants: u8, margin_bp: u16, trend: Trend },
    SiteClosed { site: SiteId, months_negative: u8, roi_bp: i32 },  // roi z WŁASNYCH ksiąg — twarde
    LoanTaken  { kind: LoanKind, amount: Money, liquidity_months_before: u8 },
    Factored   { amount: Money, discount_bp: u16, liquidity_months_before: u8 },
    PriceWar   { target: FirmId, observed_price: Money, share_lost_bp: u16 },
    SupplierLocked { supplier: FirmId, premium_bp: u16, rival: FirmId },
    Bankruptcy { trigger: BankruptcyTrigger, days_illiquid: u16 },
    Founded    { niche: GoodId, district: DistrictId, expected_margin_bp: i32 },
    ChainEntry { chain: ChainId, threshold: ChainThreshold },
}
pub enum WageCause  { VacancyDays(u16), ShortageIndex(u16), Counteroffer(FirmId), Retention }
pub enum PriceCause { StockHigh(u16), StockLow(u16), Rival(FirmId), Spoilage(u16), CostUp(u16) }
```

Zapis jest **wymuszony typem**. Jedyna droga wykonania akcji to

```rust
pub fn apply_decision<T: FirmAction>(cmds: &mut CommandBuffer, firm: FirmId, d: Decided<T>);
// zawsze: cmds.push(d.action) ORAZ log.push(LoggedDecision { tick, reason: d.reason })
```

Nie istnieje przeciążenie bez `reason`, więc nie da się podjąć decyzji bez powodu — to jest
warunek Definition of Done fazy, sprawdzany testem 7.8, a nie przeglądem kodu.
Pierścień 32 wpisów per firma (wzorzec pamięci z §17.7); starsze wpisy idą do kroniki na dysku (M9).
Koszt: 24 B × 32 × 10 000 firm ≈ 7,7 MB — w budżecie §17.7.
Inspektor (WP16) tłumaczy wariant enuma na zdanie po polsku — tłumaczenie jest w warstwie UI,
w symulacji nie ma żadnych stringów.

---

## Zmiany wpisane po R1

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po refaktorze R1.

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **Przebudowa `DecisionReason` na `Citizen(CitizenReason) \| Firm(FirmReason) \| City(CityReason)` z §5.11 rozwiązuje przy okazji problem, którego dokument nie nazywa:** `engine/ui/src/inspect/reason.rs::describe` ma **319 linii** i jest jedynym miejscem, w którym `DecisionReason` staje się tekstem dla gracza. Po podziale enuma `describe` **rozpada się z konstrukcji** na trzy funkcje, a każdy podzbiór zachowuje własną wyczerpywalność sprawdzaną przez kompilator | To jest jedyna ścieżka wyjścia dla pozycji 37 rejestru długu w `R1-refaktor-po-M5.md`. R1 tego nie ruszył, bo `DecisionReason` jest dziś enumem **płaskim**: podział `describe` daje albo powtórzenie listy wariantów we wzorcach (dwa miejsca do zapomnienia zamiast jednego), albo pomocnicze funkcje zwracające `Option<String>` — a to **kasuje wyczerpywalność**, czyli jedyną rzecz, która gwarantuje, że nowy wariant nie przejdzie bez tekstu. `K-12` mówi o tym wprost: brak ramienia ma łamać kompilację, a nie po cichu wyświetlać pustą kartę |
| | **Zanim M7c to zrobi, `describe` urośnie.** M6 dopisuje sześć wariantów (`Shortage`, `SupplierChosen`, `ContractSigned`, `ExportChosen`, `SubstituteUsed`, `ProductionHalted`), czyli ~60 linii — plus wpisy w `data/locale/pl.ron` i `en.ron`, bo każdy tekst widoczny dla gracza powstaje w obu wersjach w tej samej zmianie | Odnotowane, żeby M7c nie zdziwił się rozmiarem funkcji, którą ma rozciąć, i żeby M6 wiedziało, że dokłada do pozycji, która już jest nad progiem błędu |

---

## Zmiany wpisane po M7b

Poprawki wpisane przez podfazę **M7b** (`K-18`). Wpisane jest tylko to, co wiadomo
na pewno po jej zamknięciu.

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **`Site::mgmt` przestał być liczbą bez skutku.** Od M7b jakość zarządzania wchodzi w produktywność każdego pracownika, a przez nią w `PlantSite::labor_pct`, w ocenę wyniku (`Employment::perf_ema`), w premię i w zwolnienie za wynik. Menedżer, którego M7c przypisze zakładowi, zmienia więc **cztery** mierzalne rzeczy naraz, a nie jedną | Test monotoniczności z §7.4 fazy ma po M7b na czym stanąć: da się porównać dwa identyczne zakłady różniące się wyłącznie `skill_mgmt`. Do M7b `mgmt` było neutralne wszędzie i test przechodziłby tożsamościowo |
| ★ | **Trzy decyzje, które M7b podejmuje stałą, czekają na politykę:** agresja licytacyjna (`bidding::AGGRESSION`, dziś 0), wagi wyboru kandydata (`HiringPolicy::NEUTRAL`) i przyznanie świadczenia pozapłacowego (dziś: sztywna kolejność „posiłki → opieka → szkolenia → auto", gdy stawka stoi na suficie). Wszystkie trzy są w kodzie w **jednym** miejscu każda i podmienia się je bez zmiany kształtu reguły | Każda z nich jest parametrem osobowości firmy, a osobowość powstaje w M7e — ale **wpięcie** ich w politykę należy do M7c, bo to polityka jest nośnikiem. Wpisane teraz, żeby M7c nie szukał ich po kodzie |
| ★ | **Premia jest funkcją oceny wyniku i stałej z `data/tuning/labor.ron`, a nie polityki firmy.** `Employment::bonus_policy` z §5.3 nie powstał — pole bez pisarza (`AR-6`) | M7c jest pierwszą podfazą, w której polityka firmy w ogóle istnieje. Wtedy `bonus` dostaje drugi mnożnik (wynik zakładu) i staje się tym, czym miał być: decyzją, a nie tabelą |
| | **`describe` w `engine/ui` ma po M7b 451 linii** wobec 397 przed nią, a prognoza z rejestru długu R1 (pozycja 37) mówi, że rozpada się z konstrukcji dopiero po przebudowie `DecisionReason` na `Citizen \| Firm \| City` w §5.11 | Liczba jest zamrożona w `struct_guard.py`, więc dopisanie jednego ramienia zapala bramkę. M7c dokłada własny blok powodów i podnosi ją jeszcze raz — to jest przewidziane, nie zaskoczenie |

---

## Zmiany wpisane po M7c

Korekty tej podfazy (`K-18`). Poprawki dokumentu **fazy** są w tabeli „Zmiany wpisane
po M7c" w `M7-firmy-ai-i-rynek-pracy.md`, poprawki języka — w `M9d-jezyk-regul.md`.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Co | Dlaczego |
|---|---|---|
| `AX-1`* | **AST dostaje `GoodRef { This, Id(GoodId) }`** w miejsce gołego `GoodId` w metrykach i akcjach towarowych | M9d §5.6 ma w AST `good: GoodId`, a **wszystkie sześć przykładowych polityk w tym samym paragrafie** pisze `TEN_TOWAR`. Polityka o zakresie kategorii albo grupy wykonuje się raz na towar i nie może znać jego identyfikatora w chwili zapisu; bez tego wariantu presetu w `data/policies/` nie da się zapisać inaczej niż jako lista czterystu kopii. Podstawienie robi ewaluator, zanim widok cokolwiek zobaczy |
| `AX-2` | **`Alert` i `AskPlayer` niosą `msg: u16`, a nie `LocKey`** | `LocKey` mieszka w `engine/ui`, a symulacja od interfejsu nie zależy i zależeć nie będzie. `msg` jest indeksem komunikatu polityki, który warstwa UI odwzorowuje na klucz lokalizacji. Przypadek (2) z `K-18`: API z przykładu nie istnieje po tej stronie grafu zależności |
| `AX-3` | **Typ nazywa się `Policy`, a `FirmPolicy` jest jego aliasem** | M9d nazywa go `Policy` (bo sterują nim i firma, i gracz), §6 dokumentu fazy — `FirmPolicy`. To jeden typ o dwóch nazwach w dwóch dokumentach, a nie dwa typy; alias kosztuje jedną linię i utrzymuje obie nazwy prawdziwymi |
| `AX-4`* | **`policy::register_metric` i `register_action` nie powstają** | §6 dokumentu fazy je zapowiada, ale `D18` mówi wprost, że drugiego konsumenta (polityki miasta M8, gospodarstw M3) **nikt nie potwierdził**. Rejestr z jedną implementacją jest abstrakcją, której zakazuje YAGNI, a metryka jest tutaj **wariantem enuma**: rozszerza się ją tak samo tanio i z kontrolą kompilatora. Wraca do rozważenia, gdy M8 albo M3 zgłosi, że na tym buduje |
| `AX-5` | **`management_quality(m, morale, tuning)` zamiast `management_quality(m, site, morale)`** | Zakład nie wnosi do tej funkcji ani jednej liczby: rozpiętość kierowania jest cechą menedżera, a nastrój i tak trzeba podać osobno, bo `Site` go nie zna (morale mieszka w `Vitals` mieszkańców). Przekazanie `&Site` sugerowałoby, że funkcja czyta coś jeszcze — ten sam błąd, który `AS-3` usunął z `loss_multiplier` |
| `AX-6`* | **Walidator odrzuca dziedziny bez wykonawcy** (`DomainNotAvailable`): dziś wolno przypiąć wyłącznie politykę `Pricing` i `Stock` | `Hr`, `Production` i `Logistics` są w słowniku, bo tak stanowi M9d, ale nikt ich nie wykonuje do M7e. Polityka kadrowa przypięta do zakładu wyglądałaby jak działająca i nie robiłaby nic — a to jest gorsze niż komunikat o błędzie, bo gracz zbudowałby na niej plan |
| `AX-7`* | **Przebudowa `DecisionReason` na `Citizen \| Firm \| City` z §5.11 nie weszła.** `fn describe` rośnie dalej: 451 → **499** linii, a adres podziału przesuwa się z M7c na osobną zmianę | Przebudowa dotyka **370 miejsc w dziesięciu crate'ach**, zmienia reprezentację w zapisie gry i pakowanie `PlanSlot.reason` do bajtu — i nie wnosi nic do żadnego z dwóch kryteriów tej podfazy. Wpuszczona do commita M7c zatopiłaby recenzję podfazy dokładnie tak, jak `cargo fmt` całego repo zatopiłby recenzję M7a (`AS-6`). Sam podział `describe` **nadal jest właściwym rozwiązaniem** i nadal wymaga tamtej przebudowy; zmienia się wyłącznie to, kto ją niesie. Wpisane w `R1-refaktor-po-M5.md` poz. 37 i zamrożone w `struct_guard.py` |
| `AX-8` | **`Metric::StockDays` jest liczbą całkowitą, nie `Option`**: pusta półka daje 0, zapas bez sprzedaży — 999 (`MAX_COVER`), reszta zapas przez obrót dobowy | „Nie wiem" było odpowiedzią złą dla obu krańców. Sklep z pełną półką i zerową sprzedażą ma pokrycie **nieskończone**, a nie nieznane — i to na nim ma się wyzwolić reguła „martwy zapas". Sklep z pustą półką ma pokrycie **zero** niezależnie od tego, czy cokolwiek się sprzedawało — i to na nim ma się wyzwolić zamówienie. Przy `Option` żadna z tych dwóch reguł nie wyzwoliłaby się nigdy |
| `AX-9` | **`evaluate` zwraca akcje przez referencję do polityki, nie kopią** | Kopia byłaby alokacją na ścieżce gorącej, bo `Action` niesie `Expr` za wskaźnikiem — zmierzone: 6 897 alokacji na 1 000 ewaluacji. Kryterium WP6b mówi „bez alokacji", więc jest to kryterium, a nie preferencja; pilnuje go test z licznikiem alokacji w **osobnej binarce**, bo licznik jest globalny, a testy z jednego pliku idą równolegle |
| `AX-10` | **Promień metryki konkurencyjnej jest dziś ignorowany** przez widok sklepu; ma `ponytail:` z nazwanym sufitem | Obraz konkurencji (`CompetitorSnapshot`, M5c) powstaje jednym promieniem obserwacji dla całego sklepu, bo pełny cennik każdego sąsiada to 1,2 mln wpisów na metropolię (`W-2`). Reguła z promieniem 3 km i reguła z 5 km dostają więc tę samą liczbę. Dopóki polityka ma limit dwóch metryk konkurencyjnych, jest to różnica w zapisie, a nie w wyniku |
| `AX-11`* | **Menedżer nie powstaje z przypisania przez gracza, tylko z obsadzonego stanowiska kierowniczego.** Rynek pracy domyka to raz na dobę (`labor::hr::reconcile_managers`) | Bez tego kroku menedżerowie istnieliby wyłącznie w testach i u gracza, a miasto stawiane przez most M7a miałoby dziesięć tysięcy zakładów o zarządzaniu dokładnie przeciętnym — czyli cała warstwa z §5.4 byłaby kodem bez ani jednego użytkownika. Katalog typów zakładów wypisuje stanowiska kierownicze, rynek pracy je obsadza; tutaj domyka się pętla. Styl kierowania wychodzi z **ambicji i lojalności** z `Personality` M3, a nie z losowania: losowanie dałoby ten sam rozkład i zero wyjaśnienia |
| `AX-12` | **`ManagerExecution` z M9d (opóźnienie reakcji, błąd wykonania, pominięty cykl) nie powstaje w M7c** | M9d przypisuje ją wprost do WP9 fazy M9 i tam zostaje. M7c dostarcza **jakość zarządzania**, która wchodzi w produktywność, rotację i straty; jakość **wykonania polityki** to osobna rzecz i osobny właściciel. Dopisanie jej tutaj znaczyłoby, że M9 musi ją najpierw stąd usunąć |
| `AX-13` | **Premia nie dostała drugiego mnożnika z wyniku zakładu**, choć zapowiadała to notatka po M7b | `SitePnlMonth` **nie ma przychodu** do M7e (`AR-7`), więc wyniku zakładu nie ma z czego policzyć — drugi mnożnik byłby liczbą wziętą znikąd. Wpisane wprost zamiast cichego pominięcia; adres: **M7e**, razem z przychodem w rachunku wyniku. Pozostałe trzy stałe z tamtej notatki (agresja licytacyjna, wagi wyboru kandydata, kolejność świadczeń) **mają pisarza** i jest nim styl menedżera |
| `AX-14` | **`registry.rs` podzielony od razu**: menedżerowie poszli do `registry_managers.rs` (blok `impl` zszedł z 404 na 208 linii) | Przegląd strukturalny po zamkniętym pakiecie. To są dwa tematy nad jednym zasobem: kto istnieje i kto komu płaci wobec tego, kto czym kieruje. Podział jest przeniesieniem bloku — `Firms` zostaje jednym typem i jednym zasobem świata |
| `AX-15`* | **Ogranicznik ceny liczy się na stanie bieżącym, a nie na migawce sprzed reguły** — i to jest najpoważniejszy błąd, który znalazła recenzja przed commitem. Wykonawca rozdziela „policz skutek" od „zastosuj", a zastosowanie **aktualizuje tablicę faktów** | `ClampPrice` przycinało cenę **sprzed** `SetPrice` i zapisywało ją z powrotem, więc druga akcja reguły cofała pierwszą. Zmierzone na presecie `retail_discount`, czyli na regule z PRD §6.3 wprost: po dobie sterownik stał dokładnie na cenie startowej, a po usunięciu ogranicznika z reguły — na cenie −2 % wobec najtańszego sąsiada. Sztandarowa polityka fazy nie robiła nic, a test równoważności tego nie widział, bo **oba** zakłady cofały tak samo. Łapie to teraz `dyskont_faktycznie_schodzi_ponizej_najtanszego_konkurenta` |
| `AX-16` | **Wykonawca liczy wyrażenia akcji `magnat_policy::eval`, a nie własną arytmetyką** | Druga implementacja tej samej arytmetyki **rozjechała się z pierwszą**, zanim ktokolwiek zdążył ją napisać do końca: `Money + Days` dawało wynik po stronie akcji i `None` po stronie warunku, a `Money × Money` przechodziło wyłącznie w akcji. To jest dokładnie ten rozjazd interpretera, przed którym `sim/policy` ma chronić (`K-11`) — tylko po stronie, która **pisze do świata** |
| `AX-17` | **Akcja uszanowała swoje pole `good`**: `SetPrice { good: Id(7) }` dotyczy siódmego towaru, a nie tego, który akurat wypadł w pętli | Każde ramię wykonawcy odrzucało to pole (`..`). Działało wyłącznie `GoodRef::This`, czyli akurat to, czego używają wszystkie trzy presety — więc żaden test tego nie widział, a pierwsza polityka gracza wskazująca towar wprost ustawiałaby cenę losowej linii półki |
| `AX-18`* | **Język dostaje `Expr::Convert { to, of }` — jawne `brutto(x)` / `netto(x)` z gramatyki M9d** wraz z `PolicyView::vat_bp` | Bez niej `K-7` **zamykał język**: koszt własny jest netto, cena półkowa brutto, a walidator odrzuca porównanie mieszające podstawy — więc ogranicznika „105 % … 160 % kosztu" z PRD §6.3 nie dało się zapisać. Preset obchodził to ogranicznikiem odniesionym do **własnej ceny**, a taki nie ogranicza niczego: przycina wartość do przedziału wyprowadzonego z niej samej. Gramatyka M9d ma te dwie funkcje od początku; brakowało ich w AST |
| `AX-19` | **`Firms::detach_site` odpina **jeden** zakład; `release_manager` zostaje dla „ten człowiek przestał pracować"** | Rynek pracy wołał `release_manager` przy każdej zmianie kierownika, a ta zdejmuje menedżerowi **wszystkie** zakłady: prowadzący A i B tracił oba, gdy zmieniał się kierownik w A. B spadał do jakości zastępstwa, tracił autonomię i odzyskiwał menedżera dopiero nazajutrz — z wyzerowanym stażem. Przy okazji `assign_manager` odpina poprzedniego menedżera sam, bo bez tego zachowywał zakład na swojej liście i `refresh_management` zapisywała `Site::mgmt` z dwóch rekordów naraz |
| `AX-20` | **Cała polityka wchodzi do hasha stanu**, nie tylko jej numer i liczba reguł | Gracz zmienia politykę w trakcie gry: wyłącza regułę, przestawia martwą strefę, poprawia próg w warunku. Żadna z tych zmian nie rusza ani numeru, ani długości listy — dwa rozjechane światy dawały więc identyczny hash, czyli dokładnie ten przypadek, po który hash istnieje. Poza hashem zostaje jedno pole: notatka gracza, bo nie wpływa na wykonanie |
| `AX-21`* | **`Cadence` ma czytelnika**: polityka dobowa wykonuje się na granicy doby, godzinowa co godzinę | `Cadence` nie było czytane **nigdzie w całym workspace**, a `run_policies` biegło wyłącznie na granicy doby — więc preset przecen deklarujący `Hourly` wykonywał się raz dziennie, a **każde** `cooldown_h` poniżej 24 h było polem bez skutku, bo `ready()` nikt nie pytał częściej. Granica doby jest też granicą godziny, więc wywołanie godzinowe pomija północ — inaczej polityka godzinowa wykonałaby się tam dwa razy |
| `AX-22` | **Premia headhuntingu za stanowisko kierownicze jest `max`, a nie podstawieniem**; `site_morale` liczy się raz na przebieg, a mediana zastępstwa raz na odejście | Trzy drobne, wszystkie z tego samego przeglądu. Podstawienie premii znaczyłoby, że podniesienie zwykłej stawki w balansatorze ponad menedżerską czyni menedżerów **najtańszymi** do podkupienia — czyli odwraca zdanie, które ta gałąź ma wypowiadać. `site_morale` w pętli po planie to przy 10 tys. zakładów ~10⁸ operacji w jednym ticku pierwszej doby. Mediana czytana w pętli osuwała się z każdą iteracją: drugi zakład tego samego menedżera dostawał gorsze zastępstwo niż pierwszy, bez powodu poza kolejnością |

### Co zostaje otwarte po M7c

| # | Co | Adres |
|---|---|---|
| `AZ-1` | **Polityki firm AI nie mają źródła.** Menedżer dostaje delegację z polityką **pustą**, bo zestaw reguł firmy generuje tier taktyczny. Jakość zarządzania działa od pierwszej doby, wykonywanie reguł czeka na to, kto te reguły napisze | **M7e** (AI taktyczne) |
| `AZ-2` | **Akcje `RemoveFromShelf`, `Alert` i `AskPlayer` nie mają wykonawcy.** Pierwsza wymaga zwolnienia oferty w arenie razem z linią półki — to ta sama ścieżka, którą zamyka zakład w M7d, i powstanie tam raz, a nie dwa razy. Dwie pozostałe nie zmieniają świata i czekają na pulpit firmy | **M7d** (wycofanie), **M9** (alerty i eskalacja) |
| `AZ-3` | **Metryki `MachineUtilization`, `StaffTurnover12m`, `MedianMarketWage` i `Receivables` zwracają „nie wiem".** Reguła oparta na nich się nie wyzwala i to jest właściwe zachowanie, a nie dziura — nie ma dziś skąd wziąć tych liczb dla **zakładu**, a nie dla miasta | **M7d** (należności), **M7e** (obłożenie maszyn), **M7f** (panel rynku pracy) |
| `AZ-4` | **`OrderUpTo` wiąże cel zamówienia z obrotem tygodniowym, a `restock.rs` celowo tego nie robi** dla sklepów niedelegowanych: presja zapasu jest w M5 jedynym kanałem, którym pieniądz dochodzi do cen, i związanie celu z obrotem **odwróciło tam znak bramek G1–G3**. Dziś to bezpieczne, bo dotyczy wyłącznie zakładów, którym ktoś politykę świadomie przypiął | Faza, która zdeleguje **wszystkie** sklepy miasta, musi przemierzyć bramki G1–G3. Adres: **M7f** (domknięcie fazy) razem z balansatorem |
| `AZ-5` | **Dry-run „przetestuj na ostatnich 30 dniach" nie powstaje.** `explain` i `inputs` dostarczają to, na czym dry-run stoi — odczytane wartości metryk i drzewo warunku z wynikiem — ale samo odtworzenie polityki na zapisanych seriach należy do edytora | **M9d** (WP8) |
