# M7e — AI firm

Podfaza 5 z 6 fazy **M7 — Firmy AI i rynek pracy** (`M7-firmy-ai-i-rynek-pracy.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M7b (rynek pracy), M7c (polityki), M7d (finanse), M5, M6. |
| **Pakiety robocze** | WP10, WP11, WP12, WP12b, WP14 |
| **Projekt techniczny** | §5.6, §5.7, §5.8, §5.9, §5.15 |
| **Wynik do pokazania** | Konkurencja reaguje na gracza: otwarcie sklepu obok zmienia ceny i asortyment sąsiadów w mierzalny sposób. |
| **Kryterium zamknięcia** | Kryteria WP10–WP12b i WP14; `kernel` bez zmiany zachowania względem wersji sprzed wydzielenia. |
| **Poprzednia / następna** | `M7d-finanse-i-upadlosc.md` · `M7f-makro-i-domkniecie.md` |

Asymetria informacji (`FirmView`, `PublicMarketBoard`), AI operacyjne i taktyczne, domknięcie wspólnego jądra `sim/economy::kernel` oraz reakcja konkurencji na wejście gracza.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| WP10 | `FirmView` + `PublicMarketBoard` — asymetria informacji | WP1 | M |
| WP11 | AI operacyjne (tier 1, dzienne) | WP10, WP4, M5, M6 | M |
| WP12 | AI taktyczne (tier 2, miesięczne) | WP11, WP7, WP8 | M |
| WP12b | Wyciągnięcie `sim/economy::kernel` (funkcje czyste: `next_price`, `wage_bid`, `throughput`, `ledger_post`) — wymóg M10, zero zmian zachowania | WP11, WP12 | M |
| WP14 | Reakcja na wejście gracza | WP11–WP13 | M |

### WP10 — `FirmView` i `PublicMarketBoard`

Warstwa obserwacji. Kod AI **nie dostaje `&World`** — dostaje `FirmView`. Obserwacje cen
konkurencji idą przez wspólną tablicę per (dzielnica × `GoodId` × dzień, okno 7 dni),
a nie przez materializowaną kopię na firmę.

*Kryterium ukończenia:* test asymetrii informacji (§7) zielony; moduł `ai/` nie ma w `use`
żadnego typu zapytań ECS (test lintowy na graf zależności modułów).

### WP11–WP13 — trzy poziomy AI

Projekt w §5.6–5.10 tego dokumentu.
*WP11:* 10 000 firm, pełny dzień gry mieści się w budżecie z §5.6.
*WP12:* firma z trwale nierentownym zakładem zamyka go w ≤ 3 miesiące gry i zapisuje powód z ROI.
*WP13:* uporządkowanie wariantów z `what_if()` zgadza się z przebiegiem mezo w ≥ 95% przypadków,
w których różnica przekracza margines błędu modelu (§7.7); decyzja strategiczna zapisuje liczbę
rozważanych wariantów, wybrany i margines; przy nierozstrzygalnej różnicy AI wybiera `KeepCourse`.

### WP14 — Reakcja na wejście gracza

Trzy wzorce z §12.2 jako polityki wyzwalane spadkiem udziału rynkowego: wojna cenowa,
przejęcie lub zablokowanie dostawcy (kontrakt na wyłączność — legalny; kartel to M8),
przeciąganie pracowników (headhunting skierowany).

*Kryterium ukończenia:* scenariusz demonstracyjny z §1 pkt 5, powtarzalny, z zapisanym powodem.

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.6 AI firm — trzy poziomy, sharding i budżet (§12.3)

**Zasada wiążąca:** budżet obliczeniowy wyraża się w **liczbie firm na tick**, nigdy w czasie
rzeczywistym. Zegar ścienny jest niedeterministyczny (dokument 00 §3.5), więc nie może sterować
tym, ile decyzji zapadnie. Czas mierzymy wyłącznie w benchmarkach i asercjach CI.

```rust
/// Slot decyzyjny firmy — funkcja czysta klucza, stała przez życie firmy.
pub fn slots(key: FirmKey) -> DecisionSlots {
    let h = splitmix64(key.0);
    DecisionSlots {
        ops_minute_of_day:  (h         % 1440) as u16,  // 1 x dziennie
        tac_day_of_month:  ((h >> 12)  % 30)   as u8,   // 1 x miesięcznie (kalendarz 12 x 30)
        tac_hour:          ((h >> 20)  % 24)   as u8,
        str_day_of_quarter:((h >> 28)  % 90)   as u8,   // 1 x kwartalnie (90 = 3 x 30)
        str_hour:          ((h >> 36)  % 24)   as u8,
    }
}
```

Kalendarz gry to **12 miesięcy po 30 dni = 360 dni** (dokument 00 §4a, rozstrzygnięcie K-1),
więc miesiąc i kwartał są jednorodnymi jednostkami tickowymi i sharding dzieli się równo:
30 slotów dziennych na miesiąc, 90 na kwartał. Żadnych przypadków 28/31.

Firma decyduje, gdy bieżąca minuta doby równa się `ops_minute_of_day` itd. Rozkład jest
równomierny i deterministyczny; firmy założone tego samego dnia nie zlepiają się w jeden pik,
bo klucz idzie przez `splitmix64`, a nie przez kolejny numer.

| Poziom | Częstotliwość na firmę | Wywołanie systemu | Firm/tick przy 10 tys. firm | Budżet na firmę | Co decyduje |
|---|---|---|---|---|---|
| Operacyjny | 1 × doba | `EveryMinute` | ~7 | ≤ 5 µs | ceny, zamówienia, publikacja i eskalacja ofert pracy, premie, zmiany |
| Taktyczny | 1 × miesiąc (30 dni) | `EveryHour` | ~14/h (≈0,23/tick) | ≤ 200 µs | rentowność zakładów, zmiana `FirmPolicy`, plan HR, kredyt, otwarcie/zamknięcie zakładu |
| Strategiczny | 1 × kwartał (90 dni) | `EveryHour` | ~5/h | wg klasy (niżej) | strategia, ekspansja, reakcja na gracza, restrukturyzacja |

**Klasy strategiczne** — pełne „co jeśli" dla 10 tys. firm jest nie do udźwignięcia:

| Klasa | Kryterium | Metoda | Budżet |
|---|---|---|---|
| S1 | ≤ 4 pracowników, 1 zakład (ok. 70% firm) | tablica reguł, bez makro | ≤ 50 µs |
| S2 | reszta poniżej progu S3 (ok. 28%) | 1 rollout makro, 4 kwartały, 1 scenariusz | ≤ 2 ms |
| S3 | top 200 wg aktywów + wszystkie sieci zewnętrzne + wszystkie konkurujące bezpośrednio z graczem | 3–5 scenariuszy × 4–8 kwartałów | ≤ 20 ms |

Rachunek na dobę gry (10 tys. firm, kalendarz 30/90):
operacyjny `10 000 × 5 µs = 50 ms`;
taktyczny `10 000/30 × 200 µs ≈ 67 ms`;
strategiczny `(200 × 20 ms + 2 800 × 2 ms) / 90 ≈ 107 ms`.
**Razem ≈ 224 ms na dobę gry** przy celu ≤ 3 s/doba w trybie 50× (§20.2) — ok. 7,5% budżetu.
Po zrównolegleniu po chunkach firm (fork-join deterministyczny, składanie po indeksie chunka)
schodzi poniżej 60 ms na 4 rdzeniach.

**Twardy limit awaryjny:** jeśli w danym ticku liczba firm w slocie przekroczy `MAX_PER_TICK`
(stałe: 32 ops, 4 tac, 1 str), nadmiar przechodzi do następnego ticku kolejką FIFO posortowaną
po `FirmKey`. Przesunięcie jest deterministyczne i wchodzi do hasha stanu.

### 5.7 Osobowość, strategia, polityka (§12.1)

```rust
pub struct FirmPersonality {   // wszystkie 0..=100
    pub aggression: u8, pub risk_tolerance: u8, pub quality_focus: u8,
    pub price_focus: u8, pub innovation: u8, pub patience: u8, pub staff_loyalty: u8,
}

/// §12.1: osobowość firmy JEST wywiedziona z dyrektora-mieszkańca.
/// Funkcja czysta — zmiana dyrektora (śmierć, dziedziczenie §5.2, sprzedaż) zmienia firmę.
pub fn personality_from_director(p: &CitizenPersonality, status: Status) -> FirmPersonality;
// ambicja + ryzyko        -> aggression, risk_tolerance
// oszczędność             -> price_focus, finance.max_leverage (odwrotnie)
// sumienność              -> quality_focus, patience
// otwartość               -> innovation
// lojalność               -> staff_loyalty (opór przed zwolnieniami, premia za staż)

/// Sieci zewnętrzne: preset korporacyjny z data/chains/*.ron, bez dyrektora-mieszkańca.
pub fn personality_from_chain(c: &ChainDef) -> FirmPersonality;

pub enum FirmStrategy {
    AggressiveExpansion, Cautious, NicheQuality, Discount, Innovative, Consolidator,
}

/// Polityka firmy = ZESTAW REGUŁ w języku sim/policy (K-11), nie struktura zaszyta w Ruście.
/// Ten sam typ, ten sam ewaluator i ten sam język dla AI i dla gracza (§6.3, §14.6).
pub struct FirmPolicy {
    pub rules: Vec<PolicyRule>,     // sortowane po (PolicyScope, indeks) — determinizm
    pub preset: Option<PolicyPreset>, // nazwa presetu, z którego wyszła — do UI i do diffów
}

/// Z sim/policy (AST autorstwa M9 — specyfikacja wiążąca):
pub struct PolicyRule {
    pub scope: PolicyScope,         // Firm | Site(SiteId) | Shop(SiteId) | Product(GoodId)
    pub when:  ConditionExpr,       // np. Cmp(Metric::StockDays, Lt, Expr::Const(3))
    pub then:  Action,              // np. Action::SetMarginBp(Expr::Const(2500))
}
```

Siedem obszarów polityki z poprzedniej wersji tego planu (`PricingPolicy`, `StockPolicy`,
`HiringPolicy`, `TrainingPolicy`, `CompPolicy`, `FinancePolicy`, `ExpansionPolicy`) **nie są
typami Rusta**, tylko **presetami reguł** w `data/policies/*.ron` — punktami startowymi,
które AI modyfikuje w tierze taktycznym, a gracz edytuje w UI. Przykładowo preset `discount`
dla `FirmStrategy::Discount`:

```ron
PolicyPreset( key: "discount", rules: [
    ( scope: Firm,        when: Always,
      then: SetMarginBp(Const(1200)) ),
    ( scope: Product(Any), when: Cmp(StockDays, Gt, Const(21)),
      then: AdjustPriceBp(Const(-300)) ),
    ( scope: Product(Any), when: Cmp(RivalPriceInRadius(3000), Lt, Metric::OwnPrice),
      then: MatchRivalBp(Const(-200)) ),        // „-2% względem najtańszego w 3 km" z §6.3
    ( scope: Firm,        when: Cmp(ShortageIndex(Role::Any), Gt, Const(600)),
      then: RaiseOfferWageBp(Const(400)) ),
] )
```

Dwie konsekwencje, dla których to jest warte osobnego crate'a:
**(a)** reguła gracza i reguła konkurenta przechodzą przez ten sam ewaluator, więc nie mogą
zachować się różnie (PRD §6.3: „ten sam zestaw narzędzi co AI");
**(b)** polityka AI staje się danymi, więc moddowanie zachowania firm w M12 nie wymaga kodu.

**Ograniczenie wiążące dla `Metric`:** każda metryka dostępna w regule czyta wyłącznie przez
`FirmView` (§5.8). Reguła nie może sięgnąć po to, czego firma nie widzi — inaczej język reguł
stałby się obejściem asymetrii informacji. Test §7.3 obejmuje również ścieżkę przez `sim/policy`.

**Podstawa ceny — `Offer.price_basis` (K-7).** Cena w `Offer` to kwota płacona przez kupującego:
**brutto w detalu, netto w hurcie**, a które z nich — mówi jawne pole `Offer.price_basis`.
Konsekwencje dla firm AI liczących marże, wiążące w całym M7:

1. po stronie **zakupowej B2B** firma operuje **netto** — koszt wejść do receptury, koszt
   materiałów, koszt mediów liczy się netto;
2. **marża jest zawsze liczona na netto** — również gdy firma sprzedaje w detalu; cena półkowa
   brutto jest wynikiem: `cena_brutto = netto_z_marży * (1 + stawka)`, nigdy odwrotnie;
3. `wage_ceiling` (§5.5) i progi rentowności zakładu z `SitePnlMonth` liczone są na netto —
   inaczej zmiana stawki podatkowej w M8 przesunęłaby próg licytacji płac bez żadnego powodu
   ekonomicznego;
4. `PublicMarketBoard` przechowuje obserwację **razem z `price_basis`** — porównanie ceny
   konkurenta wymaga sprowadzenia obu do tej samej podstawy, inaczej firma detaliczna
   „zobaczy" u hurtownika cenę niższą o stawkę VAT i wejdzie w wojnę cenową z powietrzem;
5. do M8 stawka podatkowa wynosi 0, więc netto == brutto, ale **kod nie zakłada równości** —
   podstawa jest w `Offer` od pierwszego dnia. Test §7.2 (zachowanie pieniądza) sprawdza
   sumy w jednej, jawnie zadeklarowanej podstawie.

### 5.8 Asymetria informacji — `FirmView` (§12.2)

To decyzja architektoniczna, nie deklaracja intencji: **funkcje AI nie przyjmują `&World`.**

```rust
pub struct FirmView<'a> {
    pub own_books: &'a FirmBooks,          // pełny wgląd we WŁASNE koszty
    pub own_sites: &'a [Site],
    pub board:     &'a PublicMarketBoard,  // ceny półkowe konkurencji: opóźnione i zaszumione
    pub labor:     &'a LaborMarketStats,   // publiczne — oferty pracy są jawne
    pub city:      &'a CityStats,          // populacja, mediana dochodu, bezrobocie — publiczne
    pub rumors:    &'a RumorFeed,          // z grafu relacji dyrektora (§5.7), z szumem
    pub lag_days:  u8,                     // 1..=7, z rozmiaru firmy i osobowości (§6.3)
    pub tick:      Tick,
}
```

`PublicMarketBoard`: pierścień `(DistrictId, GoodId, dzień) -> ObservedPrice`, okno 7 dni,
**jeden na świat**. Koszt pamięci: dzielnice × towary × 7 — rzędu setek kB.
Wariant „kopia obserwacji per firma" byłby O(firmy × konkurenci) i został odrzucony.
Opóźnienie realizuje się przy odczycie: `board.price(district, good, tick - lag_days)`.

Czego `FirmView` **nie zawiera i zawierać nie może**: kosztu jednostkowego konkurenta (w tym
gracza), jego gotówki, receptur, kontraktów, zapasów, marży, planów; stanu potrzeb konkretnego
mieszkańca.

Egzekucja dwutorowa: (a) `sim/firms::ai` nie ma dostępu do zapytań ECS — lint na graf zależności
modułów; (b) test zachowania z §7.3, który pęka, jeśli cokolwiek wycieknie.

### 5.9 Trzy poziomy — sygnatury (§12.2, §12.3)

```rust
/// Każda funkcja decyzyjna ZWRACA powód razem z akcją — typ wymusza wyjaśnialność.
pub struct Decided<T> { pub action: T, pub reason: DecisionReason }

pub fn decide_operational(v: &FirmView, pol: &FirmPolicy)
    -> SmallVec<[Decided<OpsAction>; 8]>;
pub enum OpsAction {
    SetPrice { good: GoodId, site: SiteId, price: Money },
    PlaceOrder { good: GoodId, qty: Qty, max_price: Money },
    PostJobOffer(JobOfferDraft),
    RaiseOfferWage { offer: OfferId, to: Money },
    ClosePosting(OfferId),
    PayBonus { site: SiteId, pool: Money },
    SetShifts { site: SiteId, plan: ShiftPlan },
}

pub fn decide_tactical(v: &FirmView, pol: &FirmPolicy, pnl: &[SitePnlMonth])
    -> SmallVec<[Decided<TacAction>; 6]>;
pub enum TacAction {
    AdjustPolicy(FirmPolicy),
    CloseSite { site: SiteId },
    HireRole { site: SiteId, role: JobRoleId, count: u16 },
    LayOff  { site: SiteId, role: JobRoleId, count: u16 },
    TakeLoan { kind: LoanKind, amount: Money },
    Repay { loan: LoanId, amount: Money },
    Factor { receivables: SmallVec<[ContractId; 8]> },
    AssignManager { site: SiteId, manager: Entity },
    StartTraining { site: SiteId, role: JobRoleId, budget: Money },
}

/// UWAGA (rozstrzygnięcie M10): decyzja powstaje z UPORZĄDKOWANIA wariantów, nigdy
/// z bezwzględnej wartości prognozy. Patrz §5.10 — tryb wyłącznie porównawczy.
pub fn decide_strategic(v: &FirmView, class: StrategicClass, macro_base: &MacroState)
    -> Decided<StrAction>;
pub enum StrAction {
    KeepCourse,
    SwitchStrategy(FirmStrategy),
    OpenSite { site_type: SiteTypeId, district: DistrictId, capex: Money },
    EnterPriceWar { target: FirmId, depth_bp: u16, months: u8 },
    LockSupplier { supplier: FirmId, months: u8, premium_bp: u16 },
    PoachCampaign { target: FirmId, role: JobRoleId, premium_bp: u16 },
    Restructure { close: SmallVec<[SiteId; 4]> },
    RequestVoluntaryClosure,
}
```

**Reakcja na gracza (§12.2)** to nie osobny podsystem, tylko trzy warianty `StrAction`
wyzwalane w `ai::reaction` przy realnej utracie udziału rynkowego:
`EnterPriceWar` (koszt: ujemna marża płacona z gotówki reagującego — musi boleć),
`LockSupplier` (kontrakt na wyłączność z premią — legalny; kartel to M8),
`PoachCampaign` (headhunting skierowany na role, których gracz potrzebuje).

### 5.15 Wspólne jądro `sim/economy::kernel` (wymóg M10, robione w M7)

M10 stawia całą swoją fazę na regule: **`sim/macro` nie zawiera logiki ekonomicznej**.
Żeby to było prawdą, decyzje o cenie, płacy, produkcji i użyteczności muszą być **czystymi
funkcjami** w `sim/economy::kernel`, wołanymi tak samo przez mezo i przez makro — bez dostępu
do ECS, bez zapytań do świata, bez stanu globalnego.

M7 wyciąga je **od razu**, zamiast zaszywać w systemach firm i pozwalać M10 wyciągać je potem.
Powód jest czysto praktyczny: refaktor cudzego kodu miesiąc później kosztuje więcej niż napisanie
go od razu w docelowym miejscu, a różnica w nakładzie teraz jest bliska zeru — to te same funkcje,
tylko zadeklarowane gdzie indziej.

| Funkcja jądra | Co robi | Kto wołał to w M7 |
|---|---|---|
| `purchase_score` | użyteczność pary (produkt, miejsce) — §6.4 | M5 (decyzja zakupowa) |
| `softmax_shares` | rozkład wyboru po użytecznościach | M5 mezo (losuje), makro (wagi) |
| `next_price` | kolejna cena z polityki, zapasu, cen rywali — §6.3 | `ai::operational` |
| `wage_bid` | kolejna stawka w ofercie pracy z niedoboru i pułapu marży — §6.6 | `labor_policy` |
| `throughput` | przerób z `effective_labor`, maszyn, technologii — §7.4 | `hr::productivity`, M6 |
| `ledger_post` | zaksięgowanie kwoty (i64, reguła reszty z dok. 00 §2) — §6.9 | wszystko, co dotyka pieniądza |

Zasady wiążące dla tych funkcji: bez ECS, bez `&World`, bez `HashMap` w iteracji, pieniądz `i64`,
wejścia jawne w sygnaturze. Dzięki temu ta sama funkcja liczy tak samo w mezo i w makro,
a różnica sprowadza się do jednego miejsca: `softmax_shares` jest w mezo **losowany** (jeden agent
wybiera jedną opcję), a w makro **stosowany jako wagi** (komórka dzieli popyt proporcjonalnie).

**Kryterium: zero zmian zachowania.** Wszystkie testy M7 (§7) mają przejść bez modyfikacji —
to jest definicja ukończenia tego refaktoru, a nie jego efekt uboczny. Jeśli którykolwiek test
wymaga zmiany, znaczy to, że przy okazji zmieniliśmy zachowanie, i trzeba to cofnąć.
Własność: `sim/economy` należy do M5, więc wyciągnięcie jądra idzie przez M5 tak samo jak
moduł `labor` (D2). Zakres zmian w M7: `ai::operational`, `labor_policy`, `hr::productivity`
wołają jądro zamiast liczyć u siebie.

---

## Zmiany wpisane po M7e

Korekty **tej** podfazy naniesione w trakcie jej wykonywania (`K-18`).
Gwiazdka = zmiana zakresu albo kryterium.

| # | Co | Dlaczego |
|---|---|---|
| `BC-1`* | **`FirmView` jest strukturą faktów, a nie zbiorem referencji do zasobów.** §5.8 wypisuje `own_books: &FirmBooks`, `board: &PublicMarketBoard`, `labor: &LaborMarketStats` — żadnego z tych typów `sim/firms` nie widzi i widzieć nie może. Widok niesie `SiteFacts`, `GoodFacts` i `CityFacts`, czyli gołe liczby, a składa je `sim/economy::ai_run::facts` | Zależność idzie `economy → firms` i odwrócić się nie da. **Wychodzi z tego gwarancja mocniejsza od zamierzonej:** `sim/firms::ai` nie może sięgnąć po cudzy koszt, bo tych typów w tym crate'cie nie ma — wyciek nie jest pilnowany lintem, tylko **nie kompiluje się**. Ten sam kierunek wstrzyknięcia, którym `D19` rozciął scoring kandydata |
| `BC-2`* | **`PublicMarketBoard` mieszka w `sim/economy`, nie w `sim/firms`** (§6 „Dostarczam" podawał drugi adres), a jego wpis niesie **`cheapest_site: Option<SiteId>`**, a nie `FirmId` | Tablicę wypełnia przelot po półkach miasta, a półki są w `sim/economy` — mechanizm jest tam, gdzie dane (`AY-3`). Tożsamość: `FirmId` sklepu pochodzi z **generatora miasta** (`SiteSeed::firm`), a `FirmKey` z rejestru M7, i `firm_id(key)` nie daje tej pierwszej liczby — to jest `K-46` w drugiej odsłonie. `SiteId` ma jedną numerację po obu stronach granicy i dlatego to on przenosi tożsamość rywala, także w `DecisionReason::CompetitiveResponse` |
| `BC-3`* | **Tier operacyjny ustawia *cele*, a nie ceny i zamówienia.** `OpsAction` ma dwa warianty (`SetMarginTarget`, `SetRestockDays`) zamiast siedmiu z §5.9. `PostJobOffer`, `RaiseOfferWage` i `ClosePosting` **już się dzieją** — publikację i eskalację prowadzi `labor::matching`/`labor::bidding` od M7b; `PayBonus` rozlicza `labor::hr` raz w miesiącu; `SetShifts` to `PlantSite::schedule` z M6b | Drugi mechanizm nad tą samą rzeczą nie jest nadmiarem, tylko sprzecznością — ta sama nauka, którą M5b zapisał przy `InventoryRule` sklepu (`AL-5`). Firma zyskuje decyzję tam, gdzie jej naprawdę nie było: wokół czego składa się cena i do ilu dób zamawia |
| `BC-4` | **Cel marży liczy się od kotwicy kursu, a nie krokiem od stanu bieżącego** | Wersja krokowa całkuje: po dwudziestu dobach wojny cenowej marża leży na podłodze, po dwudziestu dobach spokoju pod sufitem, a firma nie ma dokąd wrócić. Kotwica wraca sama po ustaniu bodźca i to ona nosi rozpiętość osobowości — test `brak_bodzca_wraca_do_kotwicy_zamiast_dryfowac` |
| `BC-5`* | **Wojna cenowa schodzi do dolnego krańca własnych widełek marży i nie niżej.** §5.9 pisał „ujemna marża płacona z gotówki reagującego" | Dwa powody, każdy osobno wystarczający: sprzedaż poniżej kosztu jest praktyką wykluczającą, a ta należy do M8 razem z UOKiK (§2, „nie wchodzi"); a `min_margin_bp` z `shop.ron` jest **mechanizmem bramki G3** balansatora — wyjątek od niego wywróciłby bramkę, nie firmę. Zejście z 25 % marży na 5 % boli dostatecznie, żeby decyzja miała ciężar |
| `BC-6`* | **`StrAction` z §5.9 nie powstaje jako enum; M7e wnosi `ai::reaction::Campaign`** — jedno pole firmy z rodzajem, celem, kosztem i terminem ważności | Z ośmiu wariantów `StrAction` sześć należy do M7f: `OpenSite`, `Restructure`, `RequestVoluntaryClosure` to WP15, `SwitchStrategy`/`KeepCourse` mają sens dopiero z rolloutem makro (WP13). Enum z dwoma żywymi wariantami i sześcioma zaślepkami byłby abstrakcją bez drugiego konsumenta. M7f dopisze `StrAction` razem z tierem, który go potrzebuje |
| `BC-7` | **Wyłączność dostawcy jest rejestrem w `sim/supply::b2b` (`Exclusives`), sprawdzanym przy zbieraniu ofert.** M7 rozszerza więc `sim/supply` o drugą rzecz po `labor_pct` (`K-44`) | Blokada musi działać tam, gdzie zapada wybór dostawcy, czyli w `collect_quotes`. Rywal nie dostaje odmowy — po prostu przestaje widzieć tego dostawcę, tak jak nie widzi zakładu z pustą wystawką. Kupujący płaci za to premię doliczaną do ceny jego własnych ofert: bez kosztu wyłączność byłaby darmowa, a reakcja ma boleć (`R12`) |
| `BC-8`* | **Przychód w `SitePnlMonth` ma dziś tylko zakład *handlowy*.** Pisze go domknięcie okresu księgowego sklepu (`close_month_with` → `Firms::post_revenue`); zakład produkcyjny zostaje z zerem, a `margin_bp()` zwraca dla niego `None` | `supply::Settlement` niesie sprzedawcę jako `FirmId`, nie `SiteId`, więc utargu hurtowego nie ma jak przypisać do linii. Zero jest tu **brakiem pomiaru, a nie pomiarem zera**, i konsument (sufit licytacji, decyzja taktyczna) ma się na `None` zachować jak przed M7e. Domknięcie wymaga pola po stronie M6 — adres: **M8** (razem z podatkiem od zakładu, który tego samego przypisania potrzebuje) |
| `BC-9` | **Sufit licytacji o pracownika przesuwa marża, ale najwyżej o ±2000 bp**, choć `wage_ceiling` przyjmuje ±5000 | Sufit płacowy jest **jedynym** hamulcem spirali (`R1`), więc marża może go przesunąć, ale nie może go znieść. To domyka `AU-4`/`AV-2` dla zakładów z księgą i zostawia je otwarte dla zakładów bez niej — patrz `BC-8` |
| `BC-10` | **Osobowość firmy bez dyrektora jest losowana z klucza** (`FirmPersonality::draw`, `StreamId::FirmPersonality`), a nie neutralna | Miasto stawiane przez generator ma dziś **same** firmy zewnętrzne (`Owner::External`), więc wersja „tylko z dyrektora" znaczyłaby, że w całym mieście nie ma ani jednej firmy o charakterze — a to jest jedyny widoczny w rozgrywce skutek §12.1. Rozkład jest trójkątny, nie płaski: firm skrajnych ma być mało |
| `BC-11` | **Katalog presetów rośnie o dwa: `retail_premium` i `retail_steady`** | Odwzorowanie kursu na preset ma sens tylko wtedy, gdy kursy różnią się presetem. Sześć kursów, trzy presety — różnica między „ostrożny" a „konsolidacja" jest **inwestycyjna**, a nie cenowa, i objawi się dopiero w M7f |
| `BC-13`* | **`economy.Market` stoi po `firms.Firm` i wymagało to nowego rozstrzygnięcia `K-51`** — `SystemDesc::after_if_present`, ograniczenia warunkowego, dla którego brak celu jest brakiem krawędzi, a nie błędem budowy | Kolejność jest kontraktem: `firms.Firm` przydziela sloty i zostawia je w skrzynce, `economy.Market` je wykonuje. Zwykłe `after` wywróciło `m5shop`, balansator i klienta graficznego — **żaden z nich nie stawia systemu firm**, a scenariusz stawia wycinek symulacji, nie całość. Przy okazji wyszło, że `economy.Labor.after(firms.Firm)` z M7b nie pękało wyłącznie dlatego, że jedyny scenariusz z rynkiem pracy dodawał oba systemy |
| `BC-12`* | **Wynik do pokazania jest pokazany w teście, a nie w mieście.** `sim/economy/tests/firm_ai.rs` prowadzi pełny scenariusz „rywal wchodzi obok, zabiera klientów, konkurent odpowiada" — powtarzalnie i z zapisanym powodem. Scenariusza headless nad **wygenerowanym** miastem nie ma | Żaden scenariusz nie stawia dziś `Firms` i `Market` naraz: `m5shop` i balansator budują rynek bez rejestru firm, `m7labor` rejestr bez rynku. Zszycie ich to nie dopisanie linijki — to zmiana świata, na którym stoją **skalibrowane bramki G1–G9**, a przestrojenie bramek jest pakietem **WP17**. Adres: **M7f** |

---

## Zmiany wpisane po M7b

Poprawki wpisane przez podfazę **M7b** (`K-18`).

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **`SitePnlMonth` potrzebuje przychodu i to M7e jest jego pisarzem** (`AR-7` zapowiadał to od M7a; M7b pokazał, co za tym stoi). Bez przychodu nie da się policzyć marży zakładu, a bez marży `wage_ceiling` nie ma czym przesunąć sufitu licytacji i zostaje przy krańcu widełek roli (`AU-4` w `M7b-rynek-pracy.md`) | To jest różnica między „firma nie licytuje powyżej tego, co ta praca jest warta w tej dzielnicy" a „firma nie licytuje powyżej tego, na co ją stać" — a §7.1 pkt 4 obiecuje to drugie. Wpięcie jest jedną liczbą przekazywaną do `next_bid`, nie zmianą kształtu reguły |
| ★ | **Osobowość firmy ma w rynku pracy dwa gotowe gniazda:** `bidding::AGGRESSION` (0..=100, mnoży krok licytacji) i `HiringPolicy { quality_focus, price_focus }` (przesuwa wagi scoringu kandydata). Oba stoją dziś na wartości neutralnej i oba są w kodzie w jednym miejscu | Dwa zakłady różniące się wyłącznie osobowością dyrektora mają się różnić **tym, kogo zatrudnią i za ile** — a to jest najbardziej widoczny w rozgrywce skutek §12.1. Gniazda są, więc M7e podłącza, zamiast przebudowywać |
| ★ | **`FirmView` zastanie rynek pracy już podzielony na jawne i ukryte.** Jawne: stawki w ofertach, `LaborMarketStats` (mediany, indeks niedoboru, czas wakatu). Ukryte: widełki stanowiska (`JobOffer::band`), koszt jednostkowy, marża | Test asymetrii §7.3 ma po M7b konkretną granicę do sprawdzenia, a nie deklarację: mutacja widełek gracza **nie może** zmienić decyzji konkurenta, mutacja jego stawki w ofercie **musi**. To jest wariant negatywny tamtego testu po stronie kadrowej |
| | **Pamięć firmy o kandydacie (`FirmMemory`) i siła polecenia z grafu relacji nie powstały w M7b** — `Application` nie ma pola `referral`, a `score_application` nie ma członu `history` | Obie dane są własnością M10 (relacje) i M7e (pamięć decyzji). Pole bez pisarza jest kosztem razy liczba aplikacji i zerem wartości (`AR-6`); scoring przyjmuje je jako dodatkowe pole `CandidateFacts`, bez zmiany reszty |
