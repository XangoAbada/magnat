# M7 — Firmy AI i rynek pracy

Status: plan fazy. Właściciel crate'ów: `sim/firms` (pełny), `sim/policy` (silnik reguł — K-11).
Rozszerzam, nie posiadam: `sim/economy` (właściciel M5) — moduł `labor` i wyciągnięcie `kernel`;
`sim/macro` (właściciel M10) — `MacroState`, `lift()`, fazy 2–6 `step()`, `what_if()`.
Dokument nadrzędny: `00-konwencje-i-kontrakty.md` — pieniądz `Money(i64)`, determinizm §3,
wyjaśnialność §7, szablon §8, kalendarz §4a. Ten plan **nie redefiniuje** niczego z dokumentu 00.

Rozstrzygnięcia koordynatora wbudowane w ten plan: **K-1** (kalendarz 12 × 30 = 360 dni),
**K-4** (`StreamId` 220–239 — `AS-1`), **K-7** (`Offer.price_basis`: brutto w detalu, netto w hurcie —
marże zawsze na netto), **K-9** (związki i strajki w M10; M7 dostarcza dane, indywidualne
negocjacje płacowe zostają w M7), **K-10** (upadłość w całości u M7; M8 jest tylko wierzycielem),
**K-11** (`sim/policy` na własność M7, język reguł autorstwa M9),
**D2** (`LaborMarketStats` zostaje w `sim/economy`), **D3** (`sim/macro` należy do M10 —
M7 buduje w nim podzbiór wymagany przez tier strategiczny, wg kontraktu `M10-glebia.md` §6;
`what_if()` wyłącznie w trybie porównawczym, bez progów absolutnych i bez kwot w UI).

---

## 1. Cel fazy i artefakt końcowy

Po M7 świat ma **żywą stronę podażową i rynek pracy**. Firmy przestają być scenografią dla gracza:
mają dyrektora-mieszkańca, osobowość wywiedzioną z jego cech, własne pieniądze, kredyty,
pracowników i strategie. Powstają, rosną, przegrywają i bankrutują bez udziału gracza.

Artefakt uruchamialny (headless + inspektor w `devtools`):

1. `tools/headless --scenario m7_miasto --years 5` — miasto 150 tys. startuje z ~6–10 tys. firm AI;
   przez 5 lat gry bezrobocie oscyluje w zadanym paśmie, płace per zawód rozjeżdżają się zgodnie
   z niedoborami, część firm bankrutuje, przedsiębiorczy mieszkańcy zakładają nowe, sieć zewnętrzna
   wchodzi po przekroczeniu progu atrakcyjności. Bez ingerencji gracza, bez spirali.
2. **Karta inspekcji firmy**: bilans, zakłady, załoga, ostatnie 32 decyzje z `DecisionReason`
   („podniosłem stawkę spawacza z 5 400 na 5 900 zł, bo oferta wisiała 14 dni bez kandydata,
   a indeks niedoboru w dzielnicy = 0,82").
3. **Panel ludzi** (§14.3): lista pracowników, kandydatów, menedżerów, drzewo organizacyjne,
   delegowanie zakładu menedżerowi z wyborem polityki.
4. `tools/balansator` — raport rynku pracy: mediana płacy per `JobRoleId` × dzielnica × czas,
   rotacja, czas wakatu, udziały rynkowe, liczba bankructw, wskaźnik wyjaśnialności = 100%.
5. Scenariusz demonstracyjny **„konkurencja reaguje na gracza"**: gracz otwiera piekarnię
   przemysłową; w ciągu 2–8 tygodni gry lokalny konkurent obniża ceny (wojna cenowa),
   podbija stawki piekarzy (przeciąganie pracowników) albo blokuje młyn kontraktem na wyłączność —
   każda z tych reakcji z zapisanym uzasadnieniem i **bez dostępu do kosztów gracza**.

---

## 2. Zakres — wchodzi / nie wchodzi

### Wchodzi

| Obszar | PRD | Skrót |
|---|---|---|
| Firma jako osoba prawna | §7.1 | właściciele, kapitał, zakłady, pracownicy, portfel produktów, kontrakty, kredyty, historia |
| Katalog typów zakładów | §7.2 | struktura danych + opis w `data/site_types/`, nie lista w kodzie |
| Zakład jako jednostka zarządcza | §7.3 (część zarządcza) | delegowanie, polityki, koszty stałe; fizyka i maszyny konsumowane z M6 |
| Stanowiska i zatrudnienie | §7.5 | profile umiejętności, umowa, grafik zmian |
| Menedżerowie | §7.5 | realny wpływ na produktywność, rotację, straty; delegowanie zakładów |
| HR | §7.5 | rekrutacja, szkolenia, premie, benefity, zwolnienia, odprawy, rotacja |
| Rynek pracy jako rynek ofert | §6.6 | publikacja ofert, aplikowanie, wybór kandydata, licytacja płac przy niedoborze — **jako rozszerzenie `sim/economy`** (D2) |
| `sim/policy` — silnik reguł | §6.3, §14.6 | ewaluator języka reguł; **jeden silnik dla AI i gracza**; język projektuje M9 |
| Produktywność | §6.6 | f(umiejętność, energia, nastrój, zdrowie, technologia, zarządzanie) |
| Finanse firmy | §7.8 | kredyt obrotowy/inwestycyjny, leasing, faktoring, obligacje |
| Bankructwo | §7.8 | syndyk, kolejność zaspokojenia, wyprzedaż majątku, utrata pracy |
| AI firm — 3 poziomy | §12.1–12.3 | operacyjny / taktyczny / strategiczny, osobowość z dyrektora |
| Asymetria informacji | §12.2 | `FirmView` zamiast `&World`; konkurent nie zna kosztów gracza |
| Reakcja na gracza | §12.2 | wojna cenowa, przejęcie dostawcy, przeciąganie pracowników |
| Powstawanie i upadek firm | §5.6, §12.4 | przedsiębiorczy mieszkańcy, zamknięcie dobrowolne, sieci zewnętrzne |
| `sim/macro` — podzbiór | §12.3, §17.4 | `MacroState`, `lift()`, fazy 2–6 `step()`, `what_if()` wg kontraktu M10; crate należy do M10 |
| `sim/economy::kernel` — wyciągnięcie | §6.3, §6.6, §7.4 | funkcje czyste wspólne dla mezo i makro (wymóg M10); crate należy do M5 |
| Inspekcja i panel ludzi | §14.3 | karta firmy/zakładu/pracownika, drzewo organizacyjne |

### Nie wchodzi

| Obszar | PRD | Faza |
|---|---|---|
| Marketing, marka, reklama | §7.6 | M10 |
| R&D, patenty, drzewo technologii, epoki | §7.7 | M10 |
| Giełda, emisja akcji, przejęcia wrogie i przyjazne | §6.5, §7.9 | M10 |
| Związki zawodowe, negocjacje **zbiorowe**, żądania płacowe, strajki | §6.6 | **M10 (K-9).** Granica biegnie między indywidualnym a zbiorowym: **indywidualne** negocjacje płacowe i licytacja o rzadki zawód zostają w M7. M7 nie implementuje własnej mechaniki negocjacji zbiorowych, tylko **dostarcza M10 dane wejściowe** — patrz §6 |
| Kartele, ekskluzywność jako przestępstwo, UOKiK | §7.9 | M8 |
| Podatki (CIT/PIT/VAT/akcyza), płaca minimalna, prawo pracy | §6.8 | M8 |
| Historia „na sucho", pełny model makro | §17.4 | M10 |
| Produkcja, receptury, magazyny, partie, transport | §7.4, §6.2 | M6 — **konsumujemy** |
| Ceny detaliczne i decyzja zakupowa mieszkańca | §6.3, §6.4 | M5 — **konsumujemy** |
| **Projekt języka reguł** (AST: `ConditionExpr`, `Expr`, `Metric`, `Action`, `PolicyScope`) | §14.6 | **M9 — specyfikacja wiążąca dla M7.** M7 buduje ewaluator w `sim/policy`, nie projektuje konkurencyjnego języka |
| Edytor reguł w UI, pełne panele biznesowe | §14.3, §14.6 | M9 — M7 daje ewaluator i inspektor devtools |
| Rynek nieruchomości, deweloperzy | §6.7 | M10 (M7 tylko kupuje/najmuje parcele istniejącym API) |

**Granica z M6:** M7 nie dotyka receptur ani przepływu towaru. M7 dostarcza M6 jedną liczbę —
`effective_labor(site) -> Qty` — i jeden mnożnik strat. M6 konsumuje je w wykonaniu receptury.

---

## 3. Mapowanie na PRD

| Sekcja PRD | Co z niej realizuje M7 |
|---|---|
| §5.1 | cechy dyrektora jako źródło `FirmPersonality`; umiejętności, energia, nastrój, zdrowie jako wejścia produktywności |
| §5.4 | status jako wejście oczekiwań płacowych i dostępu do stanowisk |
| §5.6 | przeglądanie ofert pracy, zmiana pracy przy różnicy użyteczności > próg; **przedsiębiorczość** → zakładanie firm |
| §5.7 | wykrycie niszy opiera się na wiedzy mieszkańca, nie na wyroczni globalnej |
| §6.2 | oferty pracy i wyprzedaż majątku syndyka jako te same oferty co rynek towarowy |
| §6.3 | polityka cenowa z parametrami z osobowości; obserwacja cen konkurencji z opóźnieniem 1–7 dni |
| §6.5 | kredyt, stopa bazowa, ocena ryzyka przez bank |
| §6.6 | **rdzeń**: oferty pracy, aplikacje, wybór, emergentne płace, produktywność |
| §6.9 | rachunek wyników per zakład jako wejście decyzji taktycznej |
| §7.1 | model firmy |
| §7.2 | katalog typów zakładów w danych |
| §7.3 | zakład jako jednostka zarządcza z kosztami stałymi i mediami |
| §7.5 | **rdzeń**: stanowiska, menedżerowie, HR, rotacja |
| §7.8 | **rdzeń**: finansowanie i bankructwo |
| §7.9 | stali dostawcy, ekskluzywność, integracja pionowa jako *reakcja na gracza* (bez karteli — M8) |
| §12 | **rdzeń**: osobowość, zachowania, trzy poziomy sterowania, powstawanie i upadek |
| §14.1, §14.3 | karta inspekcji z powodami, panel ludzi |
| §17.1, §17.2, §17.4 | częstotliwości systemów, DAG, LOD, makro dla „co jeśli" |
| §17.5 | indeks ofert pracy per zawód × dzielnica zamiast O(n·m) |
| §18.2 | sharding decyzji deterministyczny, RNG ze strumienia |
| §20.1 | metryki: brak spirali płacowej, rotacja w paśmie, 100% wyjaśnialności |

---

## 4. Pakiety robocze i podfazy

Kolejność jest istotna: WP1–WP3 to fundament danych, WP4–WP7 rynek pracy i zarządzanie
(najwięcej ryzyka balansowego), WP8–WP9 finanse, WP10–WP14 AI, WP15–WP17 domknięcie.

Faza jest rozbita na **6 podfaz**. Podfaza to porcja, którą da się zacząć i zamknąć
bez trzymania w głowie całej fazy: własny zestaw WP, własny sprawdzalny wynik i własny
wycinek projektu technicznego. Opis pakietów i sekcje §5 mieszkają teraz w dokumentach
podfaz — poniższa tabela mówi, gdzie co jest. Bramki 1–7 z `00-postep.md` zamykają się
dopiero po ostatniej podfazie; podfaza zamyka się własnym kryterium ze swojego dokumentu.

| Podfaza | WP | §5 | Wynik do pokazania | Dokument |
|---|---|---|---|---|
| **M7a — Firma jako dane** ✅ | WP1, WP2, WP3 | 5.1, 5.2, 5.3 | 10 000 firm w świecie, każda z przypisanymi trzema slotami decyzyjnymi; dodanie typu zakładu nie dotyka kodu. | `M7a-firma-jako-dane.md` |
| **M7b — Rynek pracy** ✅ | WP4, WP5, WP6 | 5.5 | Pensje emergentne: niedobór roli podnosi ofertę bez żadnej tabeli płac w kodzie. | `M7b-rynek-pracy.md` |
| **M7c — Polityki i menedżerowie** ✅ | WP6b, WP7 | 5.4, 5.11 | Reguła gracza i polityka firmy AI wykonują się tym samym kodem; różnica leży w jakości menedżera. | `M7c-polityki-i-menedzerowie.md` |
| **M7d — Finanse i upadłość** | WP8, WP9 | 5.12, 5.13 | Firma bierze kredyt, przestaje go obsługiwać, bankrutuje, a wierzyciele są zaspokajani w udokumentowanej kolejności. | `M7d-finanse-i-upadlosc.md` |
| **M7e — AI firm** | WP10, WP11, WP12, WP12b, WP14 | 5.6, 5.7, 5.8, 5.9, 5.15 | Konkurencja reaguje na gracza: otwarcie sklepu obok zmienia ceny i asortyment sąsiadów w mierzalny sposób. | `M7e-ai-firm.md` |
| **M7f — Makro i domknięcie** | WP13, WP15, WP16, WP17 | 5.10, 5.14, 5.16 | Pełny artefakt fazy z §1 dokumentu fazy: konkurencja reaguje na gracza, pensje emergentne. | `M7f-makro-i-domkniecie.md` |

---

## 5. Projekt techniczny

Treść przeniesiona do dokumentów podfaz. **Numeracja `5.x` jest zachowana**, więc
odesłania w tekście („patrz §5.4") nadal wskazują tę samą sekcję — zmienił się tylko plik.

| § | Temat | Dokument |
|---|---|---|
| 5.1 | Struktura crate'a | `M7a-firma-jako-dane.md` |
| 5.2 | Firma, zakład, własność (§7.1) | `M7a-firma-jako-dane.md` |
| 5.3 | Stanowiska, zatrudnienie, produktywność (§7.5, §6.6) | `M7a-firma-jako-dane.md` |
| 5.4 | Menedżerowie i delegowanie (§7.5) | `M7c-polityki-i-menedzerowie.md` |
| 5.5 | Rynek pracy jako rynek ofert (§6.6) — w `sim/economy::labor` | `M7b-rynek-pracy.md` |
| 5.6 | AI firm — trzy poziomy, sharding i budżet (§12.3) | `M7e-ai-firm.md` |
| 5.7 | Osobowość, strategia, polityka (§12.1) | `M7e-ai-firm.md` |
| 5.8 | Asymetria informacji — `FirmView` (§12.2) | `M7e-ai-firm.md` |
| 5.9 | Trzy poziomy — sygnatury (§12.2, §12.3) | `M7e-ai-firm.md` |
| 5.10 | `sim/macro` — podzbiór budowany w M7 pod „co jeśli" | `M7f-makro-i-domkniecie.md` |
| 5.11 | `DecisionReason` — jak konkretnie (dokument 00 §7) | `M7c-polityki-i-menedzerowie.md` |
| 5.12 | Finanse firmy (§7.8) | `M7d-finanse-i-upadlosc.md` |
| 5.13 | Bankructwo z syndykiem (§7.8) — postępowanie prowadzi M7 (K-10) | `M7d-finanse-i-upadlosc.md` |
| 5.14 | Powstawanie firm (§5.6) i sieci zewnętrzne (§12.4) | `M7f-makro-i-domkniecie.md` |
| 5.15 | Wspólne jądro `sim/economy::kernel` (wymóg M10, robione w M7) | `M7e-ai-firm.md` |
| 5.16 | Systemy ECS i ich częstotliwość | `M7f-makro-i-domkniecie.md` |

---

## 6. Kontrakty międzyfazowe

### Dostarczam

**Typy** (`sim/firms`):
`FirmKey`, `Firm`, `Owner`, `OwnerShare`, `FirmStatus`, `FirmBooks`, `Site`, `SiteTypeId`,
`SiteType`, `ShiftPlan`, `Position`, `Employment`, `BenefitSet`, `Payroll`, `Manager`,
`ManagerStyle`, `ManagementQuality`, `SiteDelegation`, `Autonomy`, `FirmPersonality`,
`FirmStrategy`, `Loan`, `Lease`, `Bond`, `Receivable`, `Payable`, `Bankruptcy`,
`Claim`, `ClaimPriority`, `AssetLot`, `FirmView`, `PublicMarketBoard`, `Decided<T>`,
`FirmReason` (wariant `DecisionReason::Firm`).
`FirmPolicy` mieszka w `sim/policy`; `JobOffer`, `Application`, `SkillReq`, `LaborMarketStats`
i `RoleStats` w `sim/economy::labor` (D2) — obie listy niżej.

**Funkcje:**

```rust
firms::hr::effective_labor(..) -> Qty                   // -> M6: praca zakładu w MILIETATACH (AS-2)
firms::hr::loss_multiplier(ManagementQuality) -> i32    // -> M6: straty, w tysięcznych (AS-3)
firms::Site::labor_pct(..) -> u16                       // -> M6: pokrycie etatowe w promilach (K-44)
firms::labor_policy::score_application(..) -> i32       // decyzja FIRMY o kandydacie
firms::labor_policy::wage_escalation_step(..) -> Money  // decyzja FIRMY o podbiciu stawki
// UWAGA (BB-2): te dwie nie powstały pod tym adresem. Kredyt firmy prowadzi
// `sim/economy::credit` (M5d + produkt inwestycyjny M7d), postępowanie upadłościowe
// `sim/economy::corpfin` — patrz „Zmiany wpisane po M7d".
economy::corpfin::CorpFinance::open_bankruptcy(..) -> BankruptcyId
economy::corpfin::Bankruptcy::file_claim(..) -> Result<ClaimId, ClaimRejected>
firms::ai::{decide_operational, decide_tactical, decide_strategic}
firms::ai::apply_decision(cmds, firm, Decided<T>)       // jedyna droga wykonania akcji
firms::lifecycle::found_firm(citizen, intent) -> FirmId
firms::ai::strategic::rank_variants(..) -> RankedVariants  // jedyne wejście AI do makro
```

**W crate'ach innych faz, budowane przez M7** (własność zostaje u właściciela crate'a):

```rust
// sim/macro — crate M10, M7 buduje podzbiór wymagany przez tier strategiczny:
macro::{MacroState, MacroCell, MacroFirm, MacroStock, lift, step /*fazy 2–6*/, what_if}

// sim/economy::kernel — crate M5, wyciągnięcie wymagane przez M10 (§5.15):
kernel::{next_price, wage_bid, throughput, ledger_post}   // funkcje czyste, bez ECS
```

**`sim/policy` — crate M7, język autorstwa M9 (K-11).** Ten sam ewaluator obsługuje reguły AI
i reguły gracza; M7 nie projektuje języka, tylko go wykonuje.

```rust
policy::evaluate(rules: &FirmPolicy, scope: PolicyScope, v: &FirmView)
    -> SmallVec<[Decided<Action>; 8]>     // akcja ZAWSZE z powodem wskazującym regułę
policy::validate(rules: &FirmPolicy) -> Result<(), PolicyError>  // -> M9: walidacja w edytorze
policy::explain(rule: &PolicyRule) -> ExplainTree                // -> M9: „dlaczego się wyzwoliła"
policy::register_metric / register_action                        // rejestry rozszerzalne per faza
// PolicyRule, PolicyScope, PolicyPreset, FirmPolicy
```

**Rozszerzenie `sim/economy` (crate M5, autor M7 — D2):**

```rust
economy::labor::post_offer(firm, draft) -> OfferId      // -> M9: gracz publikuje ofertę
economy::labor::apply_for(offer, citizen, expectation)  // -> M3: mieszkaniec aplikuje
economy::labor::shortage_index(role, district) -> u16   // -> M8 (polityka), M10 (związki)
// JobOffer, Application, SkillReq, LaborMarketStats, RoleStats
```

**Zdarzenia** (do kroniki M9, zdarzeń M8, mediów M10):
`FirmFounded`, `FirmClosed`, `SiteOpened`, `SiteClosed`, `Hired`, `Fired`, `Quit`, `WageChanged`,
`ManagerAssigned`, `LoanGranted`, `LoanDefaulted`, `BankruptcyOpened`, `EstateAssetSold`,
`ChainEntered`, `PriceWarStarted`.

**Dane:** `data/site_types/*.ron` (schemat + walidator CI), `data/chains/*.ron`,
`data/policies/*.ron` (presety reguł per `FirmStrategy`),
rozszerzenie `data/jobs/*.ron` o `RoleWeights` (wagi produktywności per zawód).

**Wejścia dla M10 pod związki zawodowe i strajki (K-9).** M7 nie implementuje negocjacji
zbiorowych — dostarcza komplet danych, z których M10 je zbuduje:

```rust
// wszystko już istnieje w M7, wystawione jako jawny, stabilny odczyt dla M10:
firms::hr::site_wage_distribution(site) -> WageStats      // mediana, rozstęp, rozkład per rola
firms::site_pnl(site) -> &[SitePnlMonth]                  // zysk zakładu vs. koszt pracy
firms::hr::workforce_sentiment(site) -> SentimentStats    // nastrój, stres, staż, rotacja
firms::hr::roster(site) -> &[Employment]                  // kto z kim pracuje -> M3 daje graf relacji
economy::labor::shortage_index(role, district) -> u16     // siła przetargowa zawodu
```

Rozdźwięk „zysk firmy vs. płace" z §6.6 jest więc liczalny po stronie M10 z danych M7,
bez dublowania modelu. Przerwanie produkcji przez strajk M10 realizuje istniejącym mechanizmem
wstrzymania zakładu (ten sam, którego M7 używa w `BankruptcyStage::Filed`).

### Konsumuję

| Z fazy | Co |
|---|---|
| M0 `core`/`ecs`/`jobs` | `Money`, `SimMinute`, `Tick`, `Qty`, `Q`, `Mood`, identyfikatory, `StreamId` (zakres M7: **220–239**, `K-4`), kalendarz 12 × 30 dni = 360 (K-1), bufory komend, fork-join, hash stanu |
| M1/M2 `sim/world` | parcele, budynki, dzielnice, strefowanie (gdzie wolno postawić zakład) |
| M3 `sim/agents` | `CitizenPersonality`, umiejętności, energia/nastrój/zdrowie, graf relacji, pamięć, DES, hooki `job_search` i `entrepreneurship` |
| M4 `engine/nav` | tabele czasów dojazdu per dzielnica × godzina — **do decyzji kandydata**, nie firmy |
| M5 `sim/economy` | mechanizm ofert i indeks przestrzenny (na nim budujemy moduł `labor` — D2), **`Offer.price_basis`: brutto w detalu, netto w hurcie (K-7)**, księga (§6.9), `BaseRate`, depozyty, rejestr emisji pieniądza, `tools/balansator` |
| M6 `sim/supply` | receptury i wykonanie produkcji, magazyny, partie, kontrakty B2B, maszyny i ich zużycie, import |
| M8 (później) | płaca minimalna, prawo pracy, potrącenia płacowe, UOKiK — dziś hooki zerowe |
| M9 | **AST języka reguł** (`ConditionExpr`, `Expr`, `Metric`, `Action`, `PolicyScope`) — specyfikacja **wiążąca**, blokuje start WP6b; panele biznesowe i edytor reguł nad `sim/policy`; hooki gracza jako właściciela firmy |
| M8 (wierzyciel) | zgłoszenia `Creditor::City` do prowadzonego przez M7 postępowania upadłościowego (K-10) — M8 nie ma własnej ścieżki egzekucji |
| M10 | marka, R&D, giełda, **związki zawodowe i strajki (K-9 — M7 dostarcza im dane, nie mechanikę)**, pełny `sim/macro` wraz z `error_margin_bp` |
| M10 `sim/macro` | **kontrakt wiążący** (`M10-glebia.md` §6): `MacroState`, `MacroCell`, `MacroFirm`, `MacroStock` (jednostki natywne z `data/goods/`, bez konwersji na granicy LOD), `lift()`, `step()`, `what_if()`, `cell_grain()`, `error_margin_bp`. M7 buduje podzbiór (fazy 2–6 `step`, `what_if`), nie projektuje modelu obok. M7 **nie** pisze `lower()`, `dry_run()`, `fast_forward()`, `MacroLodPolicy` |
| M10 `macro::what_if` | **wyłącznie w trybie porównawczym.** Model ma nieusuwalne odchylenie 3–12% dla pojedynczej firmy (wariancja rozkładu wielomianowego przy ~200 klientach), więc M7 używa go **tylko do uporządkowania wariantów strategii** — przez `RankedVariants::decisive_winner`, z `KeepCourse` jako domyślnym. **Żadna decyzja AI nie porównuje wyniku `what_if()` z progiem absolutnym**; progi absolutne liczone są z własnych ksiąg firmy. **Żadna kwota z makro nie trafia do księgi, rozliczenia, wypłaty, podatku ani do UI w formie wyglądającej na precyzyjną** — do UI idzie ranking, przedział albo `Trend`. Egzekwowane testem §7.7 pkt 4. Szczegóły: §5.10 |

### Hooki zostawione jawnie (zero-koszt do właściwej fazy)

`PayrollDeductions` (M8), `BrandStrength` w `score_application` i w polityce cenowej (M10),
`ResearchLevel` w `tech_mult` (M10), `UnionPressure` w `bidding` (M10),
`TaxRate` w `SitePnlMonth` (M8), `MinWage` w `HiringPolicy` (M8).

---

## 7. Testy i kryteria akceptacji

### 7.1 Płace reagują na niedobór zawodu (scenariusz balansatora)

`tools/balansator --scenario wage_shortage_welders`:
start — 3 zakłady wymagające zawodu `welder` po 8 etatów (24 wakaty); w mieście 20 spawaczy,
z czego 16 już zatrudnionych; reszta rynku pracy w równowadze.

**Asercje:**

1. mediana **zaakceptowanej** płacy spawacza rośnie o ≥ 15% w 60 dni gry;
2. mediana płacy zawodu **kontrolnego** (`cashier`, brak niedoboru) zmienia się o < 3% —
   podwyżka jest lokalna dla zawodu, a nie inflacją całego rynku pracy;
3. po wstrzyknięciu 30 spawaczy (migracja) `shortage_index(welder)` spada, a płace
   **stabilizują się, nie walą w dół** — istniejące umowy nie są cięte (lepkość w dół);
4. brak spirali: w 5-letnim przebiegu płaca spawacza nie przekracza `wage_ceiling` wynikającego
   z marży, więc część wakatów zostaje nieobsadzona — i **to jest poprawny wynik**: firma rezygnuje
   z produkcji zamiast płacić poniżej progu rentowności;
5. 100% podwyżek ma `FirmReason::WageRaise` z konkretnym `WageCause`.

### 7.2 Bankructwo nie gubi pieniędzy ani aktywów (property-test)

`proptest`, 10 000 przypadków: losowa firma (0–8 zakładów, 0–200 pracowników, 0–4 kredyty,
0–4 leasingi, losowe należności i zobowiązania, losowe zabezpieczenia) przeprowadzona przez pełną
maszynę stanów do `Closed`. Generator losuje **wierzycieli wszystkich typów z `Creditor`**,
włącznie z `Creditor::City` (zaległości podatkowe, K-10) i roszczeniami zgłoszonymi późno —
tak, żeby wpięcie M8 nie wymagało pisania tego testu od nowa.

**Niezmienniki:**

```
1. Pieniądz:   Σ gotówka wszystkich uczestników PRZED + Σ wpływy ze sprzedaży masy
               == Σ gotówka PO + opłata syndyka + Σ wypłat do sektora gospodarstw
               (gotówka nie powstaje ani nie ginie; opłata syndyka to jawny transfer.
                Ostatni składnik dopisany po M7d — BB-4: gospodarstwa nie mają kont
                w `Books`, więc wypłata dla człowieka zdejmuje kwotę z ksiąg i domyka
                się dopiero po drugiej stronie granicy sektora)
2. Aktywa:     każdy AssetLot kończy w DOKŁADNIE jednym stanie `LotFate`: Sold (nowy
               właściciel) | ReturnedToLessor | WrittenOff — suma liczności == suma
               początkowa, przecięcia zbiorów puste, `InEstate` po domknięciu puste
3. Leasingi:   żadne aktywo leasingowane nie trafiło do masy ani nie zostało sprzedane
4. Ludzie:     każdy Employment zakończony dokładnie raz, dokładnie jedna odprawa naliczona
5. Roszczenia: Σ wypłat per priorytet <= Σ roszczeń per priorytet; żaden niższy priorytet nie
               dostał nic, dopóki wyższy nie jest zaspokojony w całości
6. Reszta:     podział proporcjonalny sumuje się co do grosza do kwoty dzielonej
7. Determinizm: ten sam przypadek + ten sam seed -> identyczny ciąg wypłat
```

### 7.3 Firma AI nie widzi tego, czego widzieć nie powinna (test asymetrii informacji)

Dwuprzebiegowy test **zachowania**, mocniejszy niż przegląd kodu:

```
1. Scenariusz „gracz + 40 konkurentów AI", 180 dni gry. Zapisz strumień
   hash(DecisionReason) wszystkich firm AI -> H1.
2. Ten sam seed, ten sam scenariusz, ale przed startem zmutuj WYŁĄCZNIE dane UKRYTE gracza:
   koszt jednostkowy (podmiana receptury na tańszą o 30%), gotówka x10, marża docelowa,
   zawartość magazynu wejściowego, treść kontraktów.
   Dane JAWNE zostają identyczne: ceny półkowe, publikowane oferty pracy, lokalizacje,
   asortyment, godziny otwarcia.
3. Asercja: H1 == H2 — decyzje AI są BIT-IDENTYCZNE.
```

Jeśli gdziekolwiek wycieknie `&World` do modułu `ai/`, ten test pęka natychmiast.
**Wariant negatywny (test testu):** zmiana danych **jawnych** (cena półkowa gracza) **musi**
zmienić H2 — inaczej test jest ślepy i przechodzi z błędnego powodu.
Dodatkowo test strukturalny: `sim::firms::ai::*` nie importuje żadnego typu zapytań ECS
ani `sim::economy::private::*`.

### 7.4 Menedżer działa monotonicznie

Property-test: dwa identyczne zakłady różniące się wyłącznie `skill_mgmt` menedżera (A < B) —
produktywność(B) ≥ produktywność(A), rotacja(B) ≤ rotacja(A), straty(B) ≤ straty(A),
na 1000 losowych konfiguracji. Łapie odwrócone znaki i przepełnienia w skalowaniu milijednostek.

### 7.5 Determinizm i LOD (dokument 00 §3, §4)

- Hash stanu ECS co 1000 ticków obejmuje **wszystkie** komponenty M7, w tym `DecisionLog`,
  `PublicMarketBoard` i kolejki przepełnienia slotów — dwa przebiegi = identyczny ciąg hashy.
- `slots(key)` jest funkcją czystą: permutacja kolejności tworzenia firm w tym samym ticku nie
  zmienia hasha.
- Zakaz iteracji po `HashMap`/`HashSet` — lint CI.
- LOD: przebieg mikro i mezo tego samego miesiąca daje **identyczną** sumę wypłat oraz identyczną
  liczbę zatrudnień i odejść (tolerancja 0).

### 7.6 Wydajność (`criterion` + asercja budżetu w CI)

| Miara | Cel |
|---|---|
| `ai::operational` | ≤ 5 µs/firma, 1 rdzeń |
| `ai::tactical` | ≤ 200 µs/firma |
| `ai::strategic` S2 / S3 | ≤ 2 ms / ≤ 20 ms na firmę |
| doba gry, 10 tys. firm, wszystkie systemy M7 | ≤ 300 ms na 1 rdzeniu, ≤ 80 ms na 4 |
| `match_and_hire`, 5 tys. otwartych ofert | ≤ 10 ms na dobę gry |
| pamięć M7 przy 10 tys. firm i 120 tys. zatrudnionych | ≤ 350 MB |

### 7.7 Zgodność makro — test **uporządkowania**, nie dokładności (kontrakt z M10)

Model makro ma nieusuwalne odchylenie 3–12% (rozstrzygnięcie M10), więc test dokładności
bezwzględnej byłby testem fikcyjnym: albo przechodziłby przypadkiem, albo wymuszałby kalibrację
czegoś, czego skalibrować się nie da. Testujemy to, na czym faktycznie stoi pętla strategiczna.

**Asercje:**

1. **Stabilność uporządkowania.** Dla 200 zestawów po 3–5 wariantów: jeśli faktyczny przebieg
   mezo pokazuje, że wariant A bije B o więcej niż `error_margin_bp`, to `what_if()` też stawia
   A przed B. Odsetek zgodnych uporządkowań ≥ 95%.
2. **Uczciwość marginesu.** Rozkład faktycznych odchyleń makro vs. mezo mieści się w deklarowanym
   `error_margin_bp` w ≥ 90% przypadków. Jeśli M10 deklaruje margines zbyt wąski, ten test go
   złapie — margines jest obietnicą, nie ozdobą.
3. **Brak fałszywej stanowczości.** Gdy dwa warianty różnią się mniej niż o margines,
   `decisive_winner()` zwraca `None` w 100% przypadków — AI zostaje przy `KeepCourse`.
4. **Test negatywny na regresję projektową:** żadna ścieżka w `ai::strategic` nie porównuje
   wartości z `MacroOutcome` ze stałą progową. Lint + przegląd: jedyne dozwolone wejście to
   `RankedVariants::decisive_winner`.

Czego ten test **nie** sprawdza i sprawdzać nie będzie: że prognoza zysku firmy zgadza się
co do procentu. Takiej obietnicy `sim/macro` nie składa i M7 na niej nie polega.

### 7.8 Wyjaśnialność (Definition of Done, dokument 00 §7)

Po 5-letnim przebiegu: licznik `decisions_applied` == licznik `reasons_logged`.
Nie „przejrzeliśmy i wygląda dobrze" — równość liczników, inaczej CI czerwone.

### 7.9 Równoważność reguł gracza i AI (K-11)

Test, dla którego istnieje `sim/policy`. Dwa identyczne zakłady w identycznym otoczeniu —
jeden należy do gracza i jest sterowany regułą z edytora, drugi do firmy AI i jest sterowany
tą samą regułą wygenerowaną przez tier taktyczny.

**Asercje:**

1. dla 1000 losowych stanów świata obie ścieżki produkują **identyczną** akcję i identyczny
   `DecisionReason` wskazujący tę samą regułę;
2. reguła z §6.3 w brzmieniu „−2% względem najtańszego konkurenta w promieniu 3 km" daje
   ten sam wynik po obu stronach, łącznie z przypadkami brzegowymi (brak konkurenta w promieniu,
   remis cenowy, konkurent poniżej kosztu);
3. **żadna `Metric` nie czyta poza `FirmView`** — ten sam test co §7.3, uruchomiony na ścieżce
   przez ewaluator reguł: mutacja ukrytych danych gracza nie zmienia wyniku ewaluacji reguł AI;
4. ewaluacja jest deterministyczna i bez alokacji na ścieżce gorącej (`criterion` + licznik alokacji).

### 7.10 Balans długoterminowy

5-letni przebieg bez gracza, 20 seedów: bezrobocie 3–12%, rotacja roczna 8–25%, mediana marży firm
3–15%, liczba firm w paśmie ±40% wartości startowej, ≥ 1 bankructwo i ≥ 1 nowa firma na każde
100 firm rocznie, brak zawodu z zerowym zatrudnieniem przez > 180 dni.

---

## 8. Ryzyka fazy i mitygacje

| # | Ryzyko | Dlaczego groźne | Mitygacja |
|---|---|---|---|
| R1 | **Spirala płacowo-cenowa** — firmy licytują płace, podnoszą ceny, mieszkańcy żądają więcej | Klasyczny sposób, w jaki symulacja otwarta wybucha; §20.4 wymienia to jako ryzyko główne | Twarde `wage_ceiling` liczone z własnej marży, nie z rynku; lepkość płac w dół; test 7.1 pkt 4 mówi wprost, że **nieobsadzony wakat jest poprawnym wynikiem** |
| R2 | **Wyciek informacji do AI** — ktoś „na chwilę" przekaże `&World` do `ai/` | Zabija wiarygodność konkurencji i psuje rozgrywkę bezpowrotnie | `FirmView` jako jedyne wejście + test zachowania 7.3 + lint grafu modułów; wykrywane automatycznie, nie przeglądem |
| R3 | **Koszt strategicznego „co jeśli"** — 10 tys. firm × rollout makro | Wywala budżet trybu 50× | Klasy S1/S2/S3; jeden wspólny `snapshot` na kwartał; budżet liczony w firmach na tick |
| R4 | **Niedeterminizm przez budżet czasowy** — pokusa „licz, póki starczy 2 ms" | Łamie kontrakt determinizmu i replay (dokument 00 §3.5) | Budżet **wyłącznie** w liczbie firm; kolejka przepełnienia FIFO po `FirmKey`, wchodząca do hasha |
| R5 | **Eksplozja lub wymarcie firm** | Miasto bez firm albo z 50 tys. firm — jedno i drugie kończy rozgrywkę | Limit foundingów na dobę; próg kapitałowy; test 7.9 (pasmo liczby firm) |
| R6 | **Menedżer jako kosmetyka** — mnożnik 0,98–1,02, którego nikt nie czuje | §7.5 wprost mówi „realny wpływ"; bez tego cała warstwa HR jest dekoracją | Trzy niezależne kanały, zakres ≥ ±15%, test monotoniczności 7.4, widoczność w panelu ludzi |
| R7 | **Utrata pieniędzy w bankructwie** — najłatwiejsze miejsce na zgubienie groszy | Psuje globalny test zachowania pieniądza, trudne do wyśledzenia po fakcie | Property-test 7.2 z siedmioma niezmiennikami; jawny `AssetWriteOff` zamiast cichego znikania |
| R8 | **Sprzężenie z M6 na produktywności** — zmiana wzoru wywraca balans produkcji | Dwie fazy strojone przeciwko sobie | Kontrakt to jedna funkcja `effective_labor -> Qty`; M6 nie zagląda do środka; zmiana wag tylko w `data/jobs/` |
| R9 | **Dwa modele makro** — M7 buduje własny szkic obok crate'a M10 | Dwa modele „co jeśli" = dwa różne światy i podwójny koszt utrzymania | Crate należy do M10, M7 buduje **w nim** podzbiór wg kontraktu `M10-glebia.md` §6; wcześniejszy szkic M7 wycofany; test 7.7 w CI od M7 |
| R14 | **Progi absolutne na wyniku makro** — „wejdź, jeśli prognozowany zysk > 200 tys." | Model ma nieusuwalne 3–12% błędu; reguła progowa daje skokowe decyzje, które gracz odczyta jako „AI działa losowo" | Wyłącznie tryb porównawczy: `decisive_winner` z marginesem, `KeepCourse` domyślnie; `MacroOutcome` prywatne w `RankedVariants`; test 7.7 pkt 4 jako lint projektowy |
| R15 | **Fałszywa precyzja w UI** — kwota z makro pokazana graczowi co do złotówki | Gracz uwierzy i zbuduje na tym plan; gorsze niż brak prognozy | Do UI wyłącznie ranking / przedział / `Trend`; kwoty do grosza tylko z ksiąg firmy; kryterium ukończenia WP16 |
| R16 | **Jądro `kernel` wyciągnięte za późno** (albo przez M10 po fakcie) | Refaktor cudzego kodu miesiąc później kosztuje wielokrotnie więcej; ryzyko cichej zmiany zachowania | WP12b w M7, kryterium „zero zmian zachowania": wszystkie testy §7 przechodzą bez modyfikacji |
| R10 | **Nieczytelność decyzji AI dla gracza** | „Dlaczego on obniżył ceny?" bez odpowiedzi = frustracja (§20.3) | `DecisionReason` wymuszony typem `Decided<T>` i jedyną drogą `apply_decision`; test równości liczników 7.8; tłumaczenie na polskie zdanie w inspektorze (WP16) |
| R11 | **Rotacja jako młynek** — pracownicy krążą między firmami co tydzień | Zabija wydajność i realizm jednocześnie | Koszt zmiany pracy po stronie mieszkańca (§5.6: próg + lojalność), okres wypowiedzenia, kara za „przekwalifikowanego" kandydata w scoringu |
| R12 | **Reakcja na gracza jako prześladowanie** — wszyscy konkurenci rzucają się naraz | Niegrywalne, a wygląda jak oszukujące AI | Reakcja tylko przy realnej utracie udziału, tylko w tierze strategicznym (kwartalnie), a koszt reakcji płacony z gotówki reagującego — wojna cenowa musi go boleć |
| R13 | **Zależność od niegotowych faz (M8: podatki, płaca minimalna)** | Balans strojony bez podatków rozjedzie się po M8 | Hooki zerowe zamiast wartości „na oko"; scenariusze balansatora M7 opisane jako *przed-podatkowe* i przewidziane do przestrojenia w M8 |

---

## 9. Decyzje otwarte

Do rozstrzygnięcia **przed startem fazy**. W chwili pisania tego planu dokumenty M0–M6 i M8–M12
jeszcze nie istnieją w repozytorium, a w tym środowisku nie było kanału uzgodnień z ich autorami —
poniższe są propozycjami M7 wymagającymi potwierdzenia.

| # | Kwestia | Z kim | Propozycja M7 |
|---|---|---|---|
| D1 | Kto jest właścicielem relacji pracownik ↔ firma | M3 | Lekki `EmploymentLink { firm, site, role }` jako komponent mieszkańca w `sim/agents`; pełny rekord `Employment` w `sim/firms`. Jedno źródło prawdy = `sim/firms`, link jest denormalizacją pod szybkie zapytanie agenta |
| D2 | ~~Gdzie żyje `LaborMarketStats`~~ | — | **ROZSTRZYGNIĘTE.** `LaborMarketStats` i cały moduł `labor` zostają w `sim/economy` (właściciel M5); M7 jest autorem rozszerzenia, nie właścicielem crate'a. Rynek pracy korzysta z tej samej maszynerii ofert co reszta — drugi, równoległy mechanizm dopasowania jest zakazany. Wbudowane w §5.5, §5.15, §6 |
| D3 | ~~Podpis `macro::step` i własność crate'a~~ | — | **ROZSTRZYGNIĘTE w wersji M7.** Interfejs i minimalna implementacja „co jeśli" u M7, pełny model i historia „na sucho" u M10. Wymagania pętli strategicznej przekazane do M10. Semantyka podpisów nie zmienia się bez zgłoszenia do dokumentu 00. Wbudowane w §5.10 |
| D4 | Czy banki są zwykłymi firmami | M5 | Tak: encja `Firm` + `LendingPolicy` w `sim/firms`; kreacja pieniądza kredytowego i rejestr emisji w `sim/economy`. Wymaga potwierdzenia, bo M5 jest właścicielem pieniądza |
| D5 | Płaca minimalna i prawo pracy | M8 | Do M8 stała z `data/`; potem `min_wage(role, epoch)` z `sim/city`. Ustalić, czy płaca minimalna wiąże **istniejące** umowy, czy tylko nowe |
| D6 | Potrącenia płacowe (PIT / składki) | M8 | M7 wypłaca brutto == netto, `PayrollDeductions` = 0; M8 wpina się bez zmiany podpisu `payroll` |
| D7 | Pierwszeństwo pracowników przed wierzycielem zabezpieczonym w bankructwie | **do decyzji właściciela produktu** (nie M7, nie koordynator) | **Wariant domyślny M7: wynagrodzenia i odprawy PRZED wierzycielami zabezpieczonymi.** Uzasadnienie: (a) zgodne z realiami większości porządków prawnych, więc nie wymaga tłumaczenia graczowi; (b) czyni upadłość **widoczną w dzielnicy** — ludzie dostają wypłatę i wydają ją lokalnie, zamiast zniknąć razem z firmą, co zasila emergentne historie z §5; (c) przenosi ryzyko na bank, czyli na stronę, która je wycenia i może żądać wyższej marży — to jest sprzężenie, nie kara; (d) gracz-bankier traci na tym realnie, co jest ciekawszą decyzją niż bezpieczny kredyt. Wariant przeciwny (zabezpieczeni pierwsi) daje twardszą symulację finansową kosztem czytelności i lokalnych skutków. Zmiana to jedna stała w `ClaimPriority` — decyzja jest odwracalna do końca WP9 |
| D8 | Progi klas strategicznych S1/S2/S3 | M12 (skala) + balansator | Start: S3 = top 200 aktywów + sieci + bezpośredni konkurenci gracza; kalibracja po pomiarze na metropolii 400 tys. |
| D9 | Czy oferty pracy gracza są w pełni jawne dla AI | M9 | Tak — oferta jest publiczna z definicji (§6.6), więc AI zna płace gracza, ale nie jego koszty. Potwierdzić, że to zamierzone i czytelne dla gracza (inaczej pojawi się pytanie „skąd on wiedział?") |
| D10 | Punkt emisji kapitału sieci zewnętrznej | M5 — **przekazane do M5 przez koordynatora** | `MoneyEmission::ExternalCapital`. Wymaga rozszerzenia testu zachowania pieniądza **po stronie M5**; bez tego test zacznie pękać dopiero w M7, czyli daleko od przyczyny. Oczekuję od M5 wariantu `MoneyEmission` i objęcia go testem własnościowym |
| D11 | Limit przeciągania menedżerów gracza przez AI | M9 (gameplay) | Bez limitu mechanicznego, ale z kosztem: AI płaci premię z własnej gotówki i ponosi ryzyko. Jeśli okaże się frustrujące — cooldown per menedżer |
| D12 | Czy `sim/firms` szkicowany w M5/M6 jest przepisywany, czy rozszerzany | M5, M6 | Rozszerzany. M7 dodaje `FirmKey`, `FirmPolicy`, `ai/`, `view/`, `hr/`, `finance/`; sklep z M5 i zakład produkcyjny z M6 stają się szczególnymi przypadkami `Site`. Wymaga potwierdzenia, że M5/M6 nie zadeklarowały sprzecznych struktur |
| D13 | Gdzie liczone są straty magazynowe modyfikowane przez menedżera | M6 | M7 dostarcza `loss_multiplier(site)`, M6 stosuje je w swoim modelu ubytków. Ustalić, czy M6 w ogóle przewiduje taki punkt wpięcia |
| D14 | Widoczność `MacroState` dla gracza | M9 | Propozycja: gracz dostaje własne „co jeśli" tym samym kodem — to narzędzie planowania, a nie przewaga AI. Ewentualnie odblokowywane przez zatrudnienie konsultanta (`business_services`) |
| D15 | Czy dyrektor-mieszkaniec jest wymagany | M3 | Firma bez dyrektora (śmierć bez spadkobiercy) dostaje zarząd tymczasowy o neutralnej osobowości i podwyższonym prawdopodobieństwie sprzedaży lub likwidacji. Wymaga hooka dziedziczenia z §5.2 |
| D16 | Kto jest właścicielem `RoleWeights` w `data/jobs/` | M3 | M3 tworzy `data/jobs/` na potrzeby umiejętności mieszkańców; M7 dokłada sekcję `RoleWeights`. Ustalić, czy to rozszerzenie schematu, czy osobny plik |
| D17 | **Termin dostarczenia AST języka reguł przez M9** | M9 — *blokujące* | WP6b (`sim/policy`) nie może wystartować bez zamrożonej specyfikacji `ConditionExpr`, `Expr`, `Metric`, `Action`, `PolicyScope`. Potrzebuję jej **przed** WP7, bo `FirmPolicy` jest wyrażona w tym języku. Jeśli AST nie będzie gotowe na czas, awaryjnie: WP7 na tymczasowym zestawie reguł o tej samej semantyce, z jawnym długiem migracji — ale to psuje gwarancję z §7.9 i wolałbym tego uniknąć |
| D18 | Zakres `sim/policy` poza firmami | M8, M9, M3 | M7 buduje ewaluator pod polityki firm. Jeśli M8 chce reguł dla polityki miasta, a M3 dla gospodarstw domowych, rejestry `Metric`/`Action` muszą być rozszerzalne per faza — przewidziałem to w `register_metric`/`register_action`, ale potrzebuję potwierdzenia, czy ktoś na tym buduje |
| D19 | Kto jest właścicielem scoringu kandydata | M5 | Granica przebiega tak: `sim/economy::labor` prowadzi rynek (oferty, indeks, dopasowanie), `sim/firms::labor_policy` decyduje, **kogo firma chce**. Potwierdzić, że M5 akceptuje wstrzyknięcie funkcji scoringu z `sim/firms` zamiast trzymania jej u siebie |
| D20 | **Kto wyciąga `sim/economy::kernel`** | M5 (właściciel crate'a) + M10 — prowadzone jako **D11 u M10**, adresaci M5 + M7 + M10 | Podział naniesiony po obu stronach: **M7** bierze `next_price`, `wage_bid`, `throughput`, `ledger_post` (WP12b); **M5** bierze `purchase_score` i `softmax_shares`, bo decyzja zakupowa §6.4 jest jego; **M10** resztę i regułę „zero logiki ekonomicznej w `sim/macro`". Pozostaje **zgoda M5** jako właściciela crate'a — dwie fazy przebudowujące ten sam cudzy moduł niezależnie to gorszy stan niż jedna faza czekająca tydzień |
| D21 | ~~Ziarno komórek makro a tożsamość firm~~ | — | **ROZSTRZYGNIĘTE.** `FirmId` w `MacroFirm` potwierdzone przez M10 jako **wiążące**: firmy nie są agregowane w żadnym trybie ziarna. Powód wpisany do papieru M10: bez tożsamości konkurenta cały tier strategiczny §12.3 i reakcja na wejście gracza §12.2 tracą sens — tego nie da się obejść projektowo. `cell_grain()` skaluje wyłącznie komórki mieszkańców (6 → 3 klasy przy małym mieście) |
| D22 | Pytania o **konkretną** technologię w tierze strategicznym | M10 | `MacroFirm.tech` to poziom, nie zbiór węzłów — nie zna `TechId`, więc „czy licencjonować technologię X od konkurenta" jest na nim nieobliczalne. W M7 pytanie nie powstaje (brak drzewa technologii — §7.7 to M10). M10 dołoży węzeł do `MacroFirm` **na żądanie**, gdy R&D zacznie istnieć. Nie robimy tego zawczasu |

---

## 10. Szacunek wielkości

| WP | Zakres | Rozmiar |
|---|---|---|
| WP1 | Model firmy, `FirmKey`, scheduler i budżet shardingu | **M** |
| WP2 | Katalog typów zakładów w danych + walidator | **S** |
| WP3 | Stanowiska, zatrudnienie, lista płac, produktywność | **M** |
| WP4 | Rynek pracy w `sim/economy`: oferty, aplikacje, wybór kandydata | **L** |
| WP5 | Licytacja płac, indeks niedoboru, headhunting | **M** |
| WP6 | HR: szkolenia, premie, benefity, zwolnienia, rotacja | **M** |
| WP6b | `sim/policy`: ewaluator języka reguł M9, zakresy stosowania | **M** |
| WP7 | Menedżerowie, delegowanie, silnik `FirmPolicy` | **L** |
| WP8 | Finanse: kredyt, leasing, faktoring, obligacje | **M** |
| WP9 | Bankructwo, syndyk, wyprzedaż majątku | **M** |
| WP10 | `FirmView`, `PublicMarketBoard`, asymetria informacji | **M** |
| WP11 | AI operacyjne (tier 1) | **M** |
| WP12 | AI taktyczne (tier 2) | **M** |
| WP12b | Wyciągnięcie `sim/economy::kernel` (wymóg M10, zero zmian zachowania) | **M** |
| WP13 | `sim/macro`: podzbiór wg kontraktu M10 + AI strategiczne (tier 3) | **L** |
| WP14 | Reakcja na wejście gracza | **M** |
| WP15 | Powstawanie firm, upadek, sieci zewnętrzne | **M** |
| WP16 | Inspektor devtools, panel ludzi, drzewo organizacyjne | **M** |
| WP17 | Scenariusze balansatora, testy własnościowe, benchmarki | **M** |

Rozkład: 3 × L, 15 × M, 1 × S. Suma względna ≈ **XL** — największa faza symulacyjna po M6,
z ciężarem przesuniętym na WP4, WP7 i WP13 (rynek pracy, delegowanie, strategiczne „co jeśli").

Ścieżka krytyczna: **WP1 → WP3 → WP4 → WP5 → WP11 → WP12 → WP12b → WP13**.
Zrównoleglalne: WP2, WP8, WP10, WP16. WP6b blokowane przez AST od M9 (D17), WP13 przez
kontrakt `sim/macro` od M10 (dostarczony).
WP17 rośnie razem z pozostałymi, nie na końcu — testy 7.1, 7.2 i 7.3 powstają odpowiednio razem
z WP5, WP9 i WP10, bo napisane po fakcie już niczego nie złapią.

---

## Zmiany wpisane po M7d

Poprawki dokumentu **fazy** naniesione w trakcie podfazy M7d (`K-18`). Korekty samej
podfazy są w tabeli „Zmiany wpisane po M7d" w `M7d-finanse-i-upadlosc.md`.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Co | Dlaczego |
|---|---|---|
| `BB-1`* | **§6 „Dostarczam" zmienia adres finansów: `Loan`, `Lease`, `Bond`, `Bankruptcy`, `Claim`, `AssetLot` i `ClaimPriority` **nie** są typami `sim/firms`.** Mieszkają w `sim/economy::corpfin` (`ClaimPriority` i `BankruptcyTrigger` w `engine/core`, `K-48`). Podpisy mają dziś postać `CorpFinance::{sign_lease, pay_lease, issue_bond, pay_coupon, redeem_bond, factor_receivable, check_insolvency, open_bankruptcy, add_lot, bid_lot, step_case, settle_scrap, distribute}` oraz `Bankruptcy::{file_claim, plan_distribution}` | `sim/firms` nie zna `Books` i nie może: zależność idzie `economy → firms → policy`. To ten sam podział, który `D2` narzucił rynkowi pracy, a `AY-3` wykonawcy polityki cenowej — mechanizm jest tam, gdzie dane, a decyzja tam, gdzie firma. Szczegóły i pozostałe cztery korekty projektu: `BA-1`…`BA-4` w dokumencie podfazy |
| `BB-2`* | **`firms::finance::request_loan` i `firms::finance::open_bankruptcy` z §6 nie powstały i nie powstaną pod tym adresem.** Kredyt firmy prowadzi `sim/economy::credit` od M5d (M7d dokłada mu produkt inwestycyjny i zaległość z niezapłaconej raty), postępowanie — `sim/economy::corpfin` | Przypadek (2) z `K-18`: API z przykładu nie istnieje i nazywa się inaczej. Drugi rejestr kredytów obok `LoanBook` byłby dokładnie tym błędem, który `K-8` opisuje przy słownikach: rozjazd przy pierwszej zmianie, widoczny dopiero jako inna suma podaży pieniądza |
| `BB-3`* | **Decyzja właściciela produktu z `D7` jest podjęta: pracownicy przed wierzycielem zabezpieczonym.** Układ siedzi w `ClaimPriority` w `engine/core` i jest tam **regułą podziału**, nie tylko kontraktem indeksu — podział idzie po `as_index()` rosnąco. Odhaczone w `00-postep.md` | Wariant domyślny M7 przyjęty bez zmian. Dokument podfazy zapowiadał, że zmiana układu to jedna stała i tak zostało: przestawienie wariantów przestawia wynik każdej upadłości, nie dotykając ani jednej linii logiki |
| `BB-4`* | **Niezmiennik 1 z §7.2 ma dopisek o granicy sektora gospodarstw.** Brzmi: „Σ gotówka uczestników PRZED + Σ wpływy ze sprzedaży masy == Σ gotówka PO + opłata syndyka **+ Σ wypłat do sektora gospodarstw**" | Gospodarstwa nie mają kont w `Books` (M5b) i wypłata dla człowieka przechodzi kanałem `household_sector_out` — suma sald w księgach **spada** o tę kwotę. Równanie bez tego składnika jest fałszywe, a fałszywy niezmiennik jest gorszy od żadnego, bo test pod niego napisany pęka z właściwego powodu i wygląda na błąd kodu |
| `BB-5` | **§7.2 pkt 2 mówi o `LotFate`, a nie o `AssetWriteOff` jako rekordzie.** Lot kończy w dokładnie jednym z czterech stanów: `InEstate` (nigdy po domknięciu), `Sold`, `ReturnedToLessor`, `WrittenOff` | Przypadek (2) z `K-18`. Osobny rekord odpisu byłby drugą listą o tej samej treści co masa i rozjechałby się z nią przy pierwszej zmianie; jawność, o którą chodziło, daje wariant stanu — nie da się go pominąć, bo `LotFate` nie ma wartości domyślnej |
| `BB-6` | **Blok `DecisionReason` fazy M7 rośnie o sześć: `LoanTaken = 505`, `LeaseSigned = 506`, `ReceivablesFactored = 507`, `BondIssued = 508`, `BankruptcyOpened = 509`, `ClaimSettled = 510`.** Wartości są od tej chwili wieczne | `AY-6` przewidywało, że blok rośnie dalej płasko, i tak się stało — przebudowa `DecisionReason` na `Citizen \| Firm \| City` nadal nie weszła i nadal należy do osobnej zmiany. `StreamId` M7d **nie zajmuje żadnego numeru**: postępowanie jest deterministyczne z kolejności identyfikatorów, a nie z losowania |
| `BB-7`* | **`PayrollOutbox` nadal nie ma konsumenta — adres przesuwa się z M7d na M7e.** Uzasadnienie i to, co M7d z tego domyka mimo wszystko, są w `BA-8` | Powód, dla którego M7b odłożył to do M7d, nie zniknął: `SitePnlMonth` nie ma przychodu do M7e (`AV-2`), a zakłady produkcyjne nie mają księgi wcale. Wypłata realnej listy płac z konta, na które nic nie wpływa, wywróciłaby saldo każdej firmy w pierwszym miesiącu |

---

## Zmiany wpisane po M7c

Poprawki dokumentu **fazy** naniesione w trakcie podfazy M7c (`K-18`). Korekty samej
podfazy są w tabeli „Zmiany wpisane po M7c" w `M7c-polityki-i-menedzerowie.md`.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Co | Dlaczego |
|---|---|---|
| `AY-1`* | **§6 „Dostarczam" traci `policy::register_metric` i `register_action`**, a `FirmPolicy` dostaje adres: jest aliasem `policy::Policy`. Podpisy mają dziś postać `evaluate(&Policy, &MetricCtx, &impl PolicyView) -> SmallVec<[Decided<&Action>; 8]>`, `validate(&Policy) -> Result<(), PolicyError>`, `explain(&ConditionExpr, &MetricCtx, &impl PolicyView) -> ExplainTree` | Rejestry: `D18` mówi wprost, że drugiego konsumenta nikt nie potwierdził, a rejestr z jedną implementacją jest abstrakcją, której zakazuje YAGNI (`AX-4`). Podpisy: ewaluator potrzebuje kontekstu towaru (`TEN_TOWAR`, `AX-1`) i **nie kopiuje akcji**, bo kopia jest alokacją na ścieżce gorącej (`AX-9`). `explain` bierze warunek, a nie regułę, bo wyjaśnia się warunek — akcja się wykonuje |
| `AY-2` | **`sim/policy` zależy od `engine/core` i od niczego więcej.** Do §6 „Konsumuję" dochodzi `PolicyView` jako jedyne wejście reguły do świata; `FirmView` z §5.8 będzie jego implementacją, a nie osobną drogą | Zależność `economy → firms → policy` jest jednostronna i zamyka cykl, jeśli ją odwrócić. Konsekwencja, której plan nie nazywał: `PriceBasis` musiał wyprowadzić się do `core` (`K-47`), bo metryka cenowa niesie podstawę, a `sim/policy` nie widzi `sim/economy` |
| `AY-3`* | **Wykonawca akcji polityki mieszka w `sim/economy` (`policy_run`), nie w `sim/firms` ani w `sim/policy`.** §6 tego adresu nie podawał | Półkę, cenę i zapas ma sklep, a sklep jest w `sim/economy`. To ten sam podział, który `D2` narzucił rynkowi pracy: mechanizm jest tam, gdzie dane, a decyzja tam, gdzie firma. `sim/policy` mówi **co zrobić** i nie wie, czym jest sklep |
| `AY-4`* | **Kryterium §7.9 pkt 4 („bez alokacji na ścieżce gorącej") jest mierzone, a nie deklarowane** — test z licznikiem alokacji w osobnej binarce testowej | Zmierzone przy pierwszym uruchomieniu: 6 897 alokacji na 1 000 ewaluacji, wszystkie z kopiowania akcji. Kryterium napisane bez pomiaru przeszłoby |
| `AY-5` | **Decyzje otwarte fazy zamknięte w M7c: `D13`, `D17`, `D18`.** `D13` (gdzie liczone są straty modyfikowane przez menedżera) — `loss_multiplier(ManagementQuality)` ma od M7c realnego pisarza po stronie M7 i M6 stosuje go u siebie bez zmian. `D17` (termin AST od M9) — specyfikacja jest w `M9d-jezyk-regul.md` §5.6, była gotowa i wystarczyła; awaryjny wariant „tymczasowy zestaw reguł" nie był potrzebny. `D18` (zakres `sim/policy` poza firmami) — rozstrzygnięte **przez niebudowanie**: rejestrów nie ma, dopóki M8 albo M3 nie zgłosi, że na nich buduje (`AX-4`) | Wszystkie trzy dało się rozstrzygnąć tym, co pokazał kod, a nie uzgodnieniem. `D11` (limit przeciągania menedżerów gracza) zostaje otwarta: przeciąganie menedżerów działa od M7c i ma wyższą premię, ale ocena, czy jest frustrujące, wymaga gracza — adres **M9** |
| `AY-6`* | **§5.11 obiecuje przebudowę `DecisionReason` na `Citizen \| Firm \| City` i ta przebudowa w M7c nie weszła.** Blok M7 rośnie dalej płasko: `PolicyApplied = 503`, `ManagerAssigned = 504` | Przebudowa dotyka 370 miejsc w dziesięciu crate'ach i nie wnosi nic do kryteriów M7c; jej właściwym miejscem jest osobna zmiana, nie commit podfazy (`AX-7`). Wpisane tutaj, żeby M7d i M7e nie zakładały, że `FirmReason` już istnieje: warianty z §5.11 dopisuje się **płasko do bloku M7**, tak jak zrobiły to M7b i M7c |
| `AY-7` | **`FirmReason` z §5.11 ma dziś trzy warianty mniej, niż wypisuje lista, i dwa więcej.** Powstały `Hired`, `WageRaise`, `JobLeft` (M7b) oraz `PolicyApplied` i `ManagerAssigned` (M7c); `ManagerAssigned` niesie `prev: u8`, a nie `prev_quality: u16`, bo `ManagementQuality` jest bajtem | Przypadek (2) z `K-18`: typ z przykładu nie istnieje w tej szerokości. Reszta listy (`PriceCut`, `LoanTaken`, `Bankruptcy`, `Founded`, …) czeka na podfazy, które są ich właścicielami |

---

## Zmiany wpisane po M7b

Poprawki dokumentu **fazy** naniesione w trakcie podfazy M7b (`K-18`). Korekty samej
podfazy są w tabeli „Zmiany wpisane po M7b" w `M7b-rynek-pracy.md`.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Co | Dlaczego |
|---|---|---|
| `AV-1` | **Powód decyzji nazywa się `DecisionReason::Hired`, a nie `Hire`** (§4, kryterium WP4), a blok M7 otwierają `Hired = 500`, `WageRaise = 501`, `JobLeft = 502` | Nazwa wariantu opisuje **fakt**, a nie czynność — tak jak `SupplierChosen` i `ContractSigned` w bloku M6. Wartości są od tej chwili wieczne (`K-45`) |
| `AV-2`* | **`§7.1 pkt 4` mierzy się dziś wobec widełek stanowiska, a nie wobec marży** — patrz `AU-4` w dokumencie podfazy. Treść kryterium („firma rezygnuje z produkcji zamiast płacić poniżej progu rentowności") zostaje bez zmian, zmienia się liczba, z którą się porównuje | `SitePnlMonth` nie ma przychodu do M7e (`AR-7`), więc marży nie ma z czego policzyć. Wpisane teraz, żeby M7e wiedział, że domknięcie tego kryterium należy do niego, a nie do §7.1 „w ogóle" |
| `AV-3` | **Decyzje otwarte zamknięte w M7b: `D2`, `D5`, `D6`, `D19`.** `D2` i `D19` — wykonaniem, nie negocjacją: moduł `labor` stoi w `sim/economy`, a reguły firmy w `sim/firms::labor_policy`, ze wstrzyknięciem w stronę wymuszoną kierunkiem zależności (`AU-5`). `D5` (płaca minimalna) — do M8 stałej nie ma wcale, bo dolnym ogranicznikiem jest widełka roli z `data/jobs/`, a ta pochodzi z danych; pytanie „czy płaca minimalna wiąże istniejące umowy" zostaje dla M8 i nic w M7b od niego nie zależy. `D6` — potrącenia są zerem i podpis listy płac się przez to nie zmienił | Wszystkie cztery dało się rozstrzygnąć tym, co pokazał kod, a nie uzgodnieniem |
| `AV-4`* | **§6 „Dostarczam" dostaje `economy::labor::LaborMarket` i `LaborSystem` jako jawne wejście**, a funkcje z listy mają dziś podpisy: `post_offer(&mut LaborMarket, JobOffer) -> JobOfferId`, `apply_for(&mut LaborMarket, JobOfferId, CitizenId, Money, Q, u8, SimMinute) -> bool`, `shortage_index(&LaborMarket, JobRoleId, DistrictId) -> u16` | Podpisy z §6 zakładały wolne funkcje nad ukrytym stanem globalnym; rynek jest zasobem świata i wchodzi do hasha, więc każda z nich musi go dostać jawnie. Ten sam wzorzec, którym M6c poprawił swoje sygnatury (`AI-3`) |
| `AV-5` | **Do §6 „Konsumuję" dochodzi `sim/agents::migration::{Vacancies, release_job_of}`** — pula etatów miasta jest wejściem regulatora napływu (M3c §5.7) i rynek pracy musi ją prowadzić: zajęty etat z niej schodzi, zwolniony wraca | Bez tego `min(wakaty, pustostany)` przestaje opisywać miasto już po pierwszym miesiącu gry, a regulator populacji dostaje wejście, które kłamie. Kontrakt był zapisany w dokumencie podfazy jako korekta po M3c i teraz jest wykonany |
| `AV-6` | **Do „hooków zostawionych jawnie" dochodzą dwa: `AGGRESSION` (agresja licytacyjna z osobowości dyrektora) i `HiringPolicy` (wagi wyboru kandydata)** — oba stoją dziś na wartości neutralnej z adresem M7e | Zgadywanie osobowości w M7b znaczyłoby, że M7e musi najpierw usunąć odgadnięcie. Wartość neutralna jest właściwym stanem przejściowym i widać ją w jednym miejscu, a nie w dziesięciu |

---

## Zmiany wpisane po M7a

Poprawki dokumentu **fazy** naniesione w trakcie podfazy M7a (`K-18`). Korekty samej
podfazy są w tabeli „Zmiany wpisane po M7a" w `M7a-firma-jako-dane.md`.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Co | Dlaczego |
|---|---|---|
| `AS-1` | **Blok `StreamId` fazy M7 poprawiony z 240–259 na 220–239** — w §1 i w §6 | `K-4` przypisuje 220–239 fazie M7, a 240–259 fazie M8; dokument fazy podawał cudzy blok. Wartości `StreamId` są wieczne, więc pomyłka wykryta po pierwszym losowaniu kosztowałaby każdy świat wygenerowany wcześniej. M7a nie zajęła żadnego numeru, więc poprawka jest jeszcze darmowa |
| `AS-2` | **`effective_labor` dostaje jednostkę w kontrakcie §6: milietat** (1000 = jeden pełny etat o sprawności odniesienia) | §6 obiecywał M6 „jedną liczbę" typu `Qty`, ale `Qty` to milisztuki — bez podanej jednostki M6 nie miał czym jej pomnożyć, a kryterium „wynik proporcjonalny do `effective_labor`" nie było mierzalne. To jest przypadek (3) z `K-18`: kryterium niemierzalne |
| `AS-3` | **`loss_multiplier` bierze `ManagementQuality`, nie `site`, i zwraca `i32` w tysięcznych, nie `Milli`** | Typ `Milli` w projekcie nie istnieje, a mnożnik strat zależy wyłącznie od jakości zarządzania — podanie mu zakładu sugerowałoby, że czyta coś jeszcze. Przypadek (2) z `K-18`: API z przykładu nie istnieje albo nazywa się inaczej |
| `AS-4`* | **Do §6 „Dostarczam" dochodzi `Site::labor_pct`**, a do macierzy własności w 00 §1 — rozszerzanie `sim/supply` przez M7 (`K-44`) | Kontrakt z M6 brzmiał „jedna funkcja `effective_labor`", ale M6 nie miał gdzie jej wyniku przyłożyć: przepustowość linii nie zależała od pracy w żaden sposób. Brakującym ogniwem jest pole `PlantSite::labor_pct` i normalizacja pracy zakładu do jego etatów — bez niej §7.5 PRD („menedżer ma realny wpływ") nie ma jak zadziałać |
| `AS-5` | **Decyzje otwarte fazy zamknięte w M7a:** `D1` (właściciel relacji pracownik ↔ firma — potwierdzony podziałem, który już istniał w kodzie M3), `D12` (`sim/firms` rozszerzany, nie przepisywany — z zastrzeżeniem, że crate'u nie było wcale, a szkicem okazały się `Shop` w M5 i `PlantSite` w M6, oba zostają na miejscu), `D15` (dyrektor nieobowiązkowy — `Option<CitizenId>`), `D16` (rozszerzenie schematu `data/jobs/`, `K-43`) | Wszystkie cztery były potrzebne do zamknięcia WP1–WP3 i wszystkie dało się rozstrzygnąć tym, co pokazał kod, a nie negocjacją. `D13` (gdzie liczone są straty modyfikowane przez menedżera) zostaje otwarta z adresem **M7c**, bo dopiero tam menedżer przestaje być wartością neutralną |
| `AS-6` | **Obietnica z §1 „miasto 150 tys. startuje z ~6–10 tys. firm AI" jest dziś niespełniona i dostaje adres**: w mieście 4 km staje 215 firm, proporcjonalnie rząd 800 przy 150 tys. Adres: **M7f** (`AT-1` w dokumencie M7a) | Lepiej zapisać rozbieżność teraz, niż odkryć ją przy domykaniu fazy. Liczba firm wynika z tego, ile zakładów stawia Etap 7 generatora — to nie jest brak w warstwie firm, ale artefakt fazy stoi na tej liczbie i ktoś musi ją domknąć |
