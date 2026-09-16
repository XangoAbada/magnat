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

## Zmiany wpisane po M7b

Poprawki wpisane przez podfazę **M7b** (`K-18`).

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **`SitePnlMonth` potrzebuje przychodu i to M7e jest jego pisarzem** (`AR-7` zapowiadał to od M7a; M7b pokazał, co za tym stoi). Bez przychodu nie da się policzyć marży zakładu, a bez marży `wage_ceiling` nie ma czym przesunąć sufitu licytacji i zostaje przy krańcu widełek roli (`AU-4` w `M7b-rynek-pracy.md`) | To jest różnica między „firma nie licytuje powyżej tego, co ta praca jest warta w tej dzielnicy" a „firma nie licytuje powyżej tego, na co ją stać" — a §7.1 pkt 4 obiecuje to drugie. Wpięcie jest jedną liczbą przekazywaną do `next_bid`, nie zmianą kształtu reguły |
| ★ | **Osobowość firmy ma w rynku pracy dwa gotowe gniazda:** `bidding::AGGRESSION` (0..=100, mnoży krok licytacji) i `HiringPolicy { quality_focus, price_focus }` (przesuwa wagi scoringu kandydata). Oba stoją dziś na wartości neutralnej i oba są w kodzie w jednym miejscu | Dwa zakłady różniące się wyłącznie osobowością dyrektora mają się różnić **tym, kogo zatrudnią i za ile** — a to jest najbardziej widoczny w rozgrywce skutek §12.1. Gniazda są, więc M7e podłącza, zamiast przebudowywać |
| ★ | **`FirmView` zastanie rynek pracy już podzielony na jawne i ukryte.** Jawne: stawki w ofertach, `LaborMarketStats` (mediany, indeks niedoboru, czas wakatu). Ukryte: widełki stanowiska (`JobOffer::band`), koszt jednostkowy, marża | Test asymetrii §7.3 ma po M7b konkretną granicę do sprawdzenia, a nie deklarację: mutacja widełek gracza **nie może** zmienić decyzji konkurenta, mutacja jego stawki w ofercie **musi**. To jest wariant negatywny tamtego testu po stronie kadrowej |
| | **Pamięć firmy o kandydacie (`FirmMemory`) i siła polecenia z grafu relacji nie powstały w M7b** — `Application` nie ma pola `referral`, a `score_application` nie ma członu `history` | Obie dane są własnością M10 (relacje) i M7e (pamięć decyzji). Pole bez pisarza jest kosztem razy liczba aplikacji i zerem wartości (`AR-6`); scoring przyjmuje je jako dodatkowe pole `CandidateFacts`, bez zmiany reszty |
