# M7f — Makro i domknięcie

Podfaza 6 z 6 fazy **M7 — Firmy AI i rynek pracy** (`M7-firmy-ai-i-rynek-pracy.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M7e (AI, kernel), kontrakt M10. |
| **Pakiety robocze** | WP13, WP15, WP16, WP17 |
| **Projekt techniczny** | §5.10, §5.14, §5.16 |
| **Wynik do pokazania** | Pełny artefakt fazy z §1 dokumentu fazy: konkurencja reaguje na gracza, pensje emergentne. |
| **Kryterium zamknięcia** | Kryteria WP13, WP15–WP17 oraz bramki 1–7 fazy M7 w `00-postep.md`. |
| **Poprzednia / następna** | `M7e-ai-firm.md` · — (ostatnia w fazie) |

Podzbiór `sim/macro` z AI strategicznym i `what_if`, powstawanie i dobrowolny upadek firm, sieci zewnętrzne, inspektory oraz scenariusze balansatora.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| WP13 | `sim/macro`: `MacroState`/`lift`/`step` fazy 2–6/`what_if` + AI strategiczne (tier 3) | WP12b, kontrakt M10 | L |
| WP15 | Powstawanie firm (§5.6), upadek dobrowolny, sieci zewnętrzne | WP9, WP12 | M |
| WP16 | Inspektor devtools, panel ludzi, drzewo organizacyjne | WP1–WP15 | M |
| WP17 | Scenariusze balansatora, testy własnościowe, benchmarki | wszystkie | M |

### WP15 — Powstawanie, upadek, sieci zewnętrzne

Skan przedsiębiorczych mieszkańców (dzienny, limitowany), dobrowolne zamknięcie zakładu, wejście
sieci zewnętrznej po przekroczeniu progów z `data/chains/*.ron`.

*Kryterium ukończenia:* 5-letni przebieg bez gracza — liczba firm nie eksploduje ani nie wymiera
(pasmo z balansatora); kapitał sieci zewnętrznej jest zarejestrowanym punktem emisji pieniądza.

### WP16 — Inspektor i panel ludzi

Karta firmy, karta zakładu, karta pracownika, drzewo organizacyjne, lista kandydatów, oś ostatnich
decyzji z `DecisionReason` przetłumaczonym na zdanie po polsku.

*Kryterium ukończenia:* wskaźnik wyjaśnialności = 100% (licznik decyzji == licznik zapisanych
powodów, sprawdzany testem, nie okiem) **oraz zero kwot pochodzenia makro w UI** — decyzje
strategiczne prezentowane jako ranking wariantów, przedział lub strzałka kierunku, nigdy jako
„prognozowany zysk: X zł" (§5.10, wymóg M10).

### WP17 — Testy, balansator, benchmarki

Scenariusze balansatora, testy własnościowe, `criterion` na ścieżkach gorących.

---

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.10 `sim/macro` — podzbiór budowany w M7 pod „co jeśli"

**Własność i podział (dokument 00 §1 + kontrakt M10 z `M10-glebia.md` §6):**
crate należy do **M10**. M7 **nie projektuje własnego modelu makro** — buduje na API M10
ten podzbiór, bez którego nie da się zrobić strategii kwartalnej z PRD §12.3.
Kontrakt M10 jest wiążący; wcześniejszy szkic M7 (`snapshot`/`apply`/`Perturbation`) został
**wycofany na rzecz poniższego**. Nie ma dwóch modeli makro.

**Co M7 buduje** (bo bez tego nie ma tieru strategicznego):
`MacroState`, `MacroCell`, `MacroFirm`, `MacroStock`, `lift()`, fazy 2–6 `step()`, `what_if()`.

**Czego M7 nie pisze** (M10, faza M10): własnej agregacji dzielnic (wołamy `lift()`),
`lower()`/`lower_cell()` — „co jeśli" **nigdy** nie jest rozwijane z powrotem do świata,
czytamy wyłącznie agregaty; `dry_run()`, `fast_forward()`, `MacroLodPolicy`;
fazy 1, 7, 8 kroku (demografia, relacje, zdarzenia) — dla horyzontu kwartalnego nieistotne.

```rust
pub struct MacroState { tick, day, cells: Vec<MacroCell>, firms: Vec<MacroFirm>,
                        commute: CommuteMatrix, epoch: EpochState, ledger: Ledger }

/// Komórka = (DistrictId x ClassId). Metropolia: 40 dzielnic x 6 klas = 240 komórek.
/// Ziarno skaluje się z populacją (`cell_grain()`): przy małym mieście klasy łączą się 6 -> 3.
pub struct MacroCell {
    key: (DistrictId, ClassId),
    citizens: Vec<CitizenSeed>,   // TOŻSAMOŚĆ zachowana, 8 B/osoba (§17.4)
    age_hist: [u32; 18], cash: Money, deposits: Money, debt: Money,
    wealth_q: [Money; 4],
    labor: [u32; N_ROLES], skill_sum: [u32; N_ROLES],
    employed: u32, unemployed: u32, need_sat: [Q; N_NEEDS],
    demand: MacroStock,
}

/// FIRMY NIE SĄ AGREGOWANE — zachowują FirmId także w makro i w „co jeśli".
/// Dla M7 to kluczowe: strategia dotyczy konkretnego konkurenta, nie „sektora piekarniczego".
pub struct MacroFirm {
    id: FirmId, district: DistrictId, branch: BranchId,
    capital: Money, debt: Money, stock: MacroStock,
    capacity_daily: Qty, utilization_bps: u16, employees: u32, wage_bill: Money,
    price: SparseVec<GoodId, Money>, brand_stock: i32, tech: TechLevel,
    suppliers: SmallVec<[(GoodId, FirmId); 8]>,
}

pub fn lift(world: &World) -> MacroState;
pub fn step(state: &mut MacroState, params: &MacroParams);   // M7 implementuje fazy 2–6
pub fn what_if(base: &MacroState, sc: &Scenario, horizon_days: u16) -> MacroOutcome;
```

**Pola-surogaty — ograniczenie zgłoszone przez M10.** `brand_stock` i `tech` w `MacroFirm`
to **skalarne surogaty, nie prawda**: `brand_stock` jest wartością oczekiwaną agregatu afinitetów
z pamięci mieszkańców (indywidualne historie marek w makro giną), a `tech` jest **poziomem**,
nie zbiorem odblokowanych węzłów — nie zna `TechId`. M7 oba **czyta i żadnego nie zmienia**
(marka §7.6 i R&D §7.7 to M10), a do porównywania wariantów w „co jeśli" oba wystarczają.
Czego na nich **nie da się** policzyć: pytań o konkretną technologię — „czy opłaca się
licencjonować technologię X od konkurenta". Gdyby tier strategiczny kiedyś takiego pytania
potrzebował, wymaga to dołożenia węzła do `MacroFirm` po stronie M10 (ustalone: M10 doda
na żądanie). W M7 pytanie nie powstaje, bo nie ma jeszcze drzewa technologii.

`MacroStock` = `SparseVec<GoodId, i64>` w jednostce **natywnej** towaru z `data/goods/`
(ustalenie M6/M10): **bez konwersji jednostek na granicy LOD**, bo konwersja to zaokrąglenie,
a zaokrąglenie łamie zachowanie masy — i test §7.2 razem z nim.

**Zasada M10, której M7 przestrzega:** `sim/macro` **nie zawiera logiki ekonomicznej**.
Zawiera agregację, alokację i pętlę czasu. Każda decyzja o cenie, płacy, produkcji i użyteczności
oraz każde zaksięgowanie pieniądza przechodzi przez wspólne jądro `sim/economy::kernel` (§5.16).
Różnica mezo vs. makro to **wyłącznie** to, czy `softmax_shares` jest losowany (mezo: jeden agent
wybiera jedną opcję), czy stosowany jako wagi (makro: komórka dzieli popyt proporcjonalnie).

**Budżet:** `lift()` wołany **raz na kwartał**, wynik współdzielony przez wszystkie firmy klas
S2/S3. Firma robi tylko `what_if()` na klonie. `MacroState` metropolii ≤ 8 MB, więc 10 równoległych
scenariuszy to 80 MB — mieści się. Bez współdzielenia byłoby 10 tys. `lift()` na kwartał.

#### Tryb wyłącznie porównawczy — ograniczenie wiążące (rozstrzygnięcie M10)

Model makro operuje na agregatach dzielnica × klasa. **Odchylenie rzędu 3–12% wynika z wariancji
rozkładu wielomianowego i jest nieusuwalne** — to nie jest kwestia kalibracji. `what_if()` nadaje
się do **porównywania wariantów strategii między sobą** i do oceny stanu dzielnicy, a **nie** do
zdania „zysk tej firmy za rok wyniesie X".

Dlatego pętla strategiczna M7 **nie wolno jej** opierać na bezwzględnych progach. Konkretnie:

`macro::what_if(base, sc, horizon_days) -> MacroOutcome` (kontrakt M10) zwraca wynik **jednego**
scenariusza. M7 nie woła go bezpośrednio z logiki decyzyjnej — owija w cienki, własny ranker
w `firms::ai::strategic`, który jest **jedynym** wejściem AI do makro:

```rust
/// Woła macro::what_if raz na wariant, porządkuje, dokłada margines błędu modelu.
/// Nie udostępnia dalej surowych wartości MacroOutcome.
pub fn rank_variants(base: &MacroState, variants: &[Scenario], horizon_days: u16)
    -> RankedVariants;

pub struct RankedVariants {
    ranked: SmallVec<[(usize, MacroOutcome); 5]>,  // prywatne — malejąco wg wyniku
    pub error_margin_bp: u16,                      // deklarowany przez M10
}
impl RankedVariants {
    /// Zwraca zwycięzcę TYLKO jeśli jego przewaga nad drugim przekracza margines błędu.
    /// Inaczej None => firma zostaje przy obecnym kursie (KeepCourse).
    pub fn decisive_winner(&self) -> Option<usize>;
    /// Kierunek i znak zmiany — bez wielkości. To wszystko, co idzie do UI i do uzasadnień.
    pub fn direction(&self, i: usize) -> Trend;    // Up | Flat | Down
}
```

Pole `ranked` jest **prywatne celowo**: gdyby `MacroOutcome` wyciekło do reguł albo do UI,
ktoś prędzej czy później porównałby je z progiem albo wyświetlił jako kwotę.

| Zakazane | Dozwolone |
|---|---|
| „wejdź na rynek, jeśli prognozowany zysk > 200 tys." | „wybierz wariant najlepszy, jeśli przewaga nad drugim > margines błędu" |
| „zamknij zakład, bo makro przewiduje stratę 40 tys." | „zamknij zakład, bo wariant *bez niego* wypada lepiej niż wariant *z nim*, ponad margines" |
| próg absolutny wyliczony z `what_if()` | próg absolutny wyliczony z **własnych ksiąg** (`SitePnlMonth`, §6.9) — to dane twarde, nie prognoza |

Powód jest praktyczny: reguła progowa na wielkości obarczonej 3–12% błędem daje skokowe,
nieintuicyjne zachowania konkurencji — z perspektywy gracza „AI zachowuje się losowo".
Reguła porównawcza z marginesem jest na ten sam błąd odporna: wynik zmienia się dopiero wtedy,
gdy różnica między wariantami jest realna. **Zawsze istnieje wariant `KeepCourse`** i to on
wygrywa domyślnie, gdy model nie rozstrzyga — dzięki temu niepewność modelu przekłada się na
bezwładność firmy, a nie na losowe miotanie się.

**Zakaz fałszywej precyzji w UI (wymóg M10, obowiązuje WP16).** Nigdzie — ani w karcie inspekcji,
ani w uzasadnieniu decyzji, ani w panelu gracza — nie pojawia się zdanie w rodzaju
„prognozowany zysk: 240 000 zł" pochodzące z makro. Dozwolone są wyłącznie: **ranking wariantów**,
**przedział** i **strzałka kierunku** (`Trend`). Fałszywa precyzja jest gorsza od braku prognozy,
bo gracz w nią uwierzy i zbuduje na niej plan.
Kwoty pokazywane z dokładnością do grosza pochodzą **wyłącznie** z ksiąg firmy (§6.9) —
makro nigdy nie jest źródłem kwoty trafiającej do księgi, do rozliczenia kontraktu, do wypłaty
ani do podatku.
`DecisionReason::SiteOpened` niesie liczbę wariantów, margines i kierunek — gracz widzi,
że konkurent wybrał wariant A nad B i o ile pewnie, a nie że „wyliczył 240 tys.".

**Asymetria obowiązuje też w makro:** `MacroState` buduje się wyłącznie z agregatów publicznych
(statystyki miasta, oferty pracy, obserwowane ceny półkowe). Firma nie „symuluje sobie" kosztów
gracza tylnymi drzwiami.

### 5.14 Powstawanie firm (§5.6) i sieci zewnętrzne (§12.4)

```rust
pub fn founding_score(c: &CitizenSnapshot, known: &CitizenKnowledge) -> Option<FoundingIntent>;
// warunki: ambicja > A, ryzyko > R, kapitał (oszczędności + zdolność kredytowa) >= capex_min
//          typu zakładu, wykryta nisza w ZNANEJ mu okolicy (§5.7 — nie wyrocznia globalna)
```

Skan dzienny, **limitowany**: `MAX_FOUNDINGS_PER_DAY` skalowane populacją; wybór wg
`founding_score` malejąco, remisy po `CitizenId` — deterministycznie. Limit chroni przed
eksplozją firm po każdym szoku popytowym (R5).

Upadek dobrowolny: właściciel jednozakładowej firmy z trwale ujemnym ROI i niskim buforem
płynności wybiera `RequestVoluntaryClosure` — wyprzedaje majątek bez syndyka, spłaca zobowiązania
i wraca na rynek pracy. Tańsze niż bankructwo i dla symulacji, i dla niego.

Sieci zewnętrzne: `data/chains/*.ron` z progami (populacja, mediana dochodu, wartość gruntu
w dzielnicy, liczba istniejących konkurentów, dostęp drogowy, epoka), sprawdzane `EveryMonth`.
Kapitał sieci to **punkt emisji pieniądza** — musi być zarejestrowany jako
`MoneyEmission::ExternalCapital` w `sim/economy`, inaczej pęknie globalny test zachowania
pieniądza (§9, D10).

### 5.16 Systemy ECS i ich częstotliwość

| System | Częstotliwość | Czyta | Pisze | Uwagi |
|---|---|---|---|---|
| `firms::view::update_board` | `EveryDay` | ceny ofert (M5) | `PublicMarketBoard` | jedna tablica na świat |
| `economy::labor::index_offers` | `EveryHour` | `JobOffer` | indeks rola × dzielnica | §17.5, crate M5 |
| `economy::labor::intake_applications` | `EveryHour` | `Application` (z M3) | kolejki ofert | crate M5 |
| `economy::labor::match_and_hire` | `EveryDay`, shard | aplikacje, scoring z `firms` | `Employment`, log | crate M5 |
| `economy::labor::stats` | `EveryDay` | oferty, umowy | `LaborMarketStats` | crate M5, dane publiczne |
| `policy::evaluate` | wołane z `ai::*`, nie własny system | `FirmView`, `FirmPolicy` | akcje + `DecisionReason` | jeden ewaluator dla AI i gracza |
| `firms::hr::productivity` | `EveryHour` | `Employment`, vitals, `Manager` | `EffectiveLabor` per zakład | konsumuje M6 |
| `firms::hr::turnover` | `EveryDay` | nastrój, stres, oferty | odejścia | |
| `firms::hr::training` | `EveryDay` | polityka | umiejętności (M3) | |
| `firms::hr::payroll` | `EveryMonth`, shard po dniach | `Employment` | `FirmBooks`, gotówka mieszkańca | i64, bez strat |
| `firms::ai::operational` | `EveryMinute`, shard | `FirmView` | `OpsAction` do bufora komend | ~7 firm/tick |
| `firms::ai::tactical` | `EveryHour`, shard | `FirmView`, `SitePnlMonth` | `TacAction` | ~15 firm/h |
| `firms::ai::strategic` | `EveryHour`, shard | `FirmView`, `MacroState` | `StrAction` | ~5 firm/h, klasy S1–S3 |
| `firms::ai::reaction` | `EveryDay` | udział rynkowy, `board` | wyzwolenie polityk reakcji | tylko firmy tracące udział |
| `firms::finance::servicing` | `EveryDay` | raty, leasingi, kupony | `FirmBooks` | |
| `firms::finance::liquidity` | `EveryDay` | `FirmBooks` | licznik dni niewypłacalności | wyzwala bankructwo |
| `firms::finance::bankruptcy` | `EveryDay` | `Bankruptcy` | maszyna stanów, oferty masy | |
| `firms::lifecycle::founding` | `EveryDay` | mieszkańcy, ich wiedza | nowe `Firm` | limit dzienny |
| `firms::lifecycle::chains` | `EveryMonth` | `CityStats` | nowe `Firm` zewnętrzne | punkt emisji |
| `macro::lift` | `EveryMonth` (1. miesiąc kwartału) | świat | `MacroState` | jeden na świat, współdzielony przez klasy S2/S3 |

Wszystkie mutacje strukturalne (powstanie i likwidacja firmy, zatrudnienie, otwarcie zakładu)
idą przez bufory komend sortowane po `(SystemId, FirmKey)` — dokument 00 §3.4.

**Strumienie RNG.** Zakres `StreamId` przydzielony fazie M7 to **240–259** (dokument 00, K-4).
Wartości są przypisane na stałe i nigdy nie zmieniają numeru (dokument 00 §3.1):

| Wartość | Wariant | Do czego |
|---|---|---|
| 240 | `FirmHiring` | remisy w scoringu kandydatów, szum oceny |
| 241 | `FirmPricing` | eksperymenty cenowe, szum obserwacji konkurencji |
| 242 | `FirmTurnover` | losowanie odejść dobrowolnych |
| 243 | `FirmFounding` | remisy i szum w `founding_score` |
| 244 | `FirmMacroWhatIf` | rollouty makro (tier strategiczny) |
| 245 | `FirmRumor` | szum i świeżość plotki w `RumorFeed` |
| 246 | `FirmBankruptcy` | kolejność i wynik rund wyprzedaży masy |
| 247 | `FirmManager` | dobór stylu menedżera, szum jakości zarządzania |
| 248–259 | rezerwa M7 | wolne dla rozszerzeń fazy (nie przydzielać poza M7) |

---

## Zmiany wpisane po M7e

Poprawki wpisane przez podfazę **M7e** (`K-18`).
Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **Tabela strumieni RNG wyżej podaje cudzy blok i nieaktualne nazwy.** Fazie M7 przysługuje **220–239**, nie 240–259 (`K-4`, `AS-1`); 240–259 należy do M8. Zajęte po M7e: `LaborSearch = 220`, `LaborQuit = 221`, `FirmPersonality = 222`. Wolne dla M7f: **223–239**. Z nazw w tabeli `FirmPricing` jest już zajęta przez M5 (`StreamId::FirmPricing = 186`) i drugiej nie będzie; `FirmHiring`, `FirmTurnover` i `FirmManager` nie powstały, bo M7b i M7c nie potrzebowały losowania tam, gdzie tabela je przewidywała | Wartości `StreamId` są wieczne, więc tabela podająca cudzy blok jest miną: pierwsze losowanie pod numerem 244 unieważniłoby każdy świat, który M8 wygeneruje. Przypadek (1) z `K-18` — kontrakt, na którym podfaza stoi, wygląda inaczej, niż ona zakłada |
| ★ | **`StrAction` powstaje dopiero tutaj i M7e go nie zostawił.** M7e wnosi `firms::ai::reaction::Campaign` — trzy odpowiedzi konkurencyjne z kosztem i terminem ważności, wyzwalane **zmierzoną** utratą udziału, bez rolloutu makro. WP13 dokłada enum `StrAction` razem z `decide_strategic` i `RankedVariants` | Sześć z ośmiu wariantów `StrAction` (`OpenSite`, `Restructure`, `RequestVoluntaryClosure`, `SwitchStrategy`, `KeepCourse`) nie ma w M7e ani wykonawcy, ani pytania, na które odpowiadają. Enum z dwoma żywymi wariantami i sześcioma zaślepkami byłby abstrakcją bez drugiego konsumenta (`BC-6`) |
| ★ | **WP17 dostaje zadanie, którego §1 fazy wymaga, a którego nikt jeszcze nie ma: scenariusz stawiający `Firms` i `Market` naraz.** Dziś `m5shop` i balansator budują rynek **bez** rejestru firm, a `m7labor` rejestr **bez** rynku — więc ani polityki (M7c), ani AI firm (M7e) nie wykonują się w żadnym przebiegu headless, tylko w testach | To jest ta sama obserwacja co trzeci wiersz tabeli „po M7b", ale z ceną nazwaną wprost: zszycie zmienia świat, na którym stoją **skalibrowane bramki G1–G9**, więc razem z nim trzeba przemierzyć bramki. To jest praca WP17, a nie dopisanie linijki do `retail::setup` |
| ★ | **`PayrollOutbox` czeka na M7f po raz trzeci, ale blokada jest już nazwana.** Nie brakuje konsumenta — brakuje **przypisania utargu hurtowego do zakładu**: `supply::Settlement` niesie sprzedawcę jako `FirmId`, nie `SiteId`, więc `SitePnlMonth.revenue` ma dziś tylko zakład handlowy (`BC-8`) | Wypłata realnej listy płac z konta zakładu produkcyjnego, na które nic nie wpływa, wywraca saldo każdej firmy przemysłowej w pierwszym miesiącu. Kolejność jest więc wymuszona: najpierw utarg zakładu, potem lista płac. Pole po stronie M6 albo — jeśli M7f go nie ruszy — adres **M8**, gdzie tego samego przypisania potrzebuje podatek od zakładu |
| | **Dwa domknięcia drobne dla WP16:** metryka `MachineUtilization` w ewaluatorze polityk nadal zwraca „nie wiem" (`AZ-3`), a dziedzina `Hr` w walidatorze polityk nadal jest zablokowana (`AX-6`, M9d), bo wykonawcy akcji `Hire`/`RaiseWage` M7e nie napisał | Tier operacyjny nie dostał akcji kadrowych z rozmysłu — publikacja ofert i licytacja dzieją się same od M7b (`BC-3`). Ale wykonawca **polityki** kadrowej to inna rzecz niż decyzja tieru: gracz ma móc napisać regułę „podnieś stawkę spawaczom o 5 %", a tego nadal nie wykonuje nikt |
| | **`FirmView` jest gotowym wejściem dla tieru strategicznego i nie wymaga przebudowy.** Niesie `SiteFacts` (wynik, obsada, delegacja), `GoodFacts` (cena własna, cena rywala z opóźnieniem, zapas, sprzedaż dwóch tygodni), gotówkę, cechy i kurs. WP13 dokłada do niego **jedno** pole: wynik `what_if` w postaci uporządkowania wariantów | Widok jest strukturą faktów, nie zbiorem referencji (`BC-1`), więc dołożenie pola jest dopisaniem liczby w `ai_run::facts::zbierz`, a nie zmianą kształtu. Ograniczenie zostaje wiążące: **wolno dopisać tylko to, co firma widzi** — margines błędu makro jest własnością firmy, ranking wariantów też, cudzy koszt nie |

---

---

## Zmiany wpisane po M7b

Poprawki wpisane przez podfazę **M7b** (`K-18`). Wszystkie trzy pierwsze to **jedna
liczba widziana z trzech stron** i domykają się razem z `AT-1`.

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **Miasto 4 km ma 23 912 etatów przy 18,5 tys. siły roboczej** — o jedną trzecią za dużo miejsc pracy. Rynek pracy odpowiada na to poprawnie i widać to w przebiegu `m7labor`: chroniczny niedobór podbija stawki **do sufitu widełek w prawie każdym zawodzie**, a bezrobocie utrzymuje się na 6,8 % wyłącznie dzięki tarciu wyszukiwania | To jest ta sama rozbieżność co `AT-1` („215 firm zamiast 6–10 tys.") i `AT-4`, tylko zmierzona od strony skutku: nie brakuje firm, brakuje **ludzi na etatach, które już stoją**. Domknięcie przez powstawanie firm samo w sobie nie wystarczy — trzeba albo gęstszej populacji, albo rzadszej obsady zabudowy |
| ★ | **Most stawiający firmy nadaje jedne widełki wszystkim rolom spoza `Workplace` M2** (2 800–5 200 zł). W przebiegu widać skutek: mediana kilkunastu zawodów dochodzi do **dokładnie tej samej kwoty**, bo dochodzi do tego samego sufitu | Zawód bez własnych widełek nie ma czym się różnić od innego zawodu bez własnych widełek — a `data/jobs/roles.ron` ma `wage_base` dla **wszystkich** 46 ról. Brakuje wyłącznie przeliczenia przez epokę i zamożność dzielnicy, które M2 robi dla ról ze swojego podziału lokali |
| ★ | **Scenariusz `m7labor` nie stawia rynku detalicznego ani produkcji**, więc nastrój i stres pracownika stoją w nim zamrożone na wartościach z generacji, a rotacja liczy się ze stanu, który się nie zmienia. Pełne miasto ze wszystkim naraz stawia `m5shop` | Artefakt fazy z §1 wymaga jednego przebiegu, w którym **wszystko** biegnie razem: doba mieszkańca, produkcja, detal i rynek pracy. Złożenie `m5shop` z `labor::setup` to jedno wywołanie, ale przebieg pięcioletni trzeba wtedy liczyć w profilu `release` i zmierzyć jego koszt — to jest zadanie WP17 |
| | **`LaborDay` niesie już komplet metryk bramki rynku pracy**: zatrudnienia, odejścia, zwolnienia, wyjścia z rynku pracy, podwyżki, oferty zamrożone na suficie, oferty bezpośrednie, szkolenia, wakaty, siła robocza i bezrobocie | Bramka balansatora z §1 pkt 4 („mediana płacy per `JobRoleId` × dzielnica × czas, rotacja, czas wakatu") czyta te liczby z zasobu, a nie parsuje wydruku. `LaborMarketStats::per_role` daje medianę i czas wakatu per klucz |
