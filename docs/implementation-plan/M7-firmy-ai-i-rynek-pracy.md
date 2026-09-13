# M7 — Firmy AI i rynek pracy

Status: plan fazy. Właściciel crate'ów: `sim/firms` (pełny), `sim/policy` (silnik reguł — K-11).
Rozszerzam, nie posiadam: `sim/economy` (właściciel M5) — moduł `labor` i wyciągnięcie `kernel`;
`sim/macro` (właściciel M10) — `MacroState`, `lift()`, fazy 2–6 `step()`, `what_if()`.
Dokument nadrzędny: `00-konwencje-i-kontrakty.md` — pieniądz `Money(i64)`, determinizm §3,
wyjaśnialność §7, szablon §8, kalendarz §4a. Ten plan **nie redefiniuje** niczego z dokumentu 00.

Rozstrzygnięcia koordynatora wbudowane w ten plan: **K-1** (kalendarz 12 × 30 = 360 dni),
**K-4** (`StreamId` 240–259), **K-7** (`Offer.price_basis`: brutto w detalu, netto w hurcie —
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

## 4. Pakiety robocze

Kolejność jest istotna: WP1–WP3 to fundament danych, WP4–WP7 rynek pracy i zarządzanie
(najwięcej ryzyka balansowego), WP8–WP9 finanse, WP10–WP14 AI, WP15–WP17 domknięcie.

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| WP1 | Model firmy, `FirmKey`, scheduler decyzji i budżet shardingu | szkic `sim/firms` z M5/M6 | M |
| WP2 | Katalog typów zakładów w `data/site_types/` + walidator | WP1 | S |
| WP3 | Stanowiska, zatrudnienie, lista płac, produktywność | WP1, WP2, M3, M6 | M |
| WP4 | Rynek pracy w `sim/economy`: oferty, aplikacje, wybór kandydata | WP3, M5 (maszyneria ofert) | L |
| WP5 | Licytacja płac, indeks niedoboru, headhunting | WP4 | M |
| WP6 | HR: szkolenia, premie, benefity, zwolnienia, rotacja | WP3, WP4 | M |
| WP6b | `sim/policy`: ewaluator języka reguł M9, zakresy stosowania | WP1, AST od M9 | M |
| WP7 | Menedżerowie, delegowanie, `FirmPolicy` jako zestaw reguł | WP3, WP6, WP6b | L |
| WP8 | Finanse firmy: kredyt, leasing, faktoring, obligacje | WP1, M5 (`BaseRate`, księga) | M |
| WP9 | Bankructwo: syndyk, kolejność zaspokojenia, wyprzedaż | WP8, WP3 | M |
| WP10 | `FirmView` + `PublicMarketBoard` — asymetria informacji | WP1 | M |
| WP11 | AI operacyjne (tier 1, dzienne) | WP10, WP4, M5, M6 | M |
| WP12 | AI taktyczne (tier 2, miesięczne) | WP11, WP7, WP8 | M |
| WP12b | Wyciągnięcie `sim/economy::kernel` (funkcje czyste: `next_price`, `wage_bid`, `throughput`, `ledger_post`) — wymóg M10, zero zmian zachowania | WP11, WP12 | M |
| WP13 | `sim/macro`: `MacroState`/`lift`/`step` fazy 2–6/`what_if` + AI strategiczne (tier 3) | WP12b, kontrakt M10 | L |
| WP14 | Reakcja na wejście gracza | WP11–WP13 | M |
| WP15 | Powstawanie firm (§5.6), upadek dobrowolny, sieci zewnętrzne | WP9, WP12 | M |
| WP16 | Inspektor devtools, panel ludzi, drzewo organizacyjne | WP1–WP15 | M |
| WP17 | Scenariusze balansatora, testy własnościowe, benchmarki | wszystkie | M |

### WP1 — Model firmy, `FirmKey`, scheduler decyzji

Komponenty ECS firmy i zakładu, stabilny klucz `FirmKey(u64)` (monotoniczny licznik świata,
zapisywany w save; **nie** indeks encji — indeksy bywają recyklingowane, a klucz steruje
shardingiem i strumieniem RNG), `DecisionLog` jako pierścień 32 wpisów.
Scheduler: przydział firmy do slotu decyzyjnego każdego z trzech poziomów (§5.6 tego dokumentu).

*Kryterium ukończenia:* 10 000 firm w świecie, każda ma przypisane trzy sloty; test determinizmu —
dwa przebiegi dają identyczną sekwencję `(tick, FirmKey, tier)`; hash stanu ECS obejmuje wszystkie
komponenty M7.

### WP2 — Katalog typów zakładów w danych

`data/site_types/*.ron` — po jednym pliku na branżę (`extraction.ron`, `processing.ron`,
`manufacturing.ron`, `logistics.ron`, `retail.ron`, `consumer_services.ron`,
`business_services.ron`, `finance.ron`, `media.ron`, `real_estate.ron`).
**Dodanie nowego typu zakładu nie dotyka kodu** — to jest kryterium.

*Kryterium ukończenia:* walidator w CI — każdy `SiteType` ma domknięty zestaw `JobRoleId`
(każda rola istnieje w `data/jobs/`), każda receptura wskazuje istniejące `GoodId`, każdy typ ma
co najmniej jedno stanowisko menedżerskie; dodanie nowego pliku RON nie wymaga rekompilacji poza
ładowaniem danych.

### WP3 — Stanowiska, zatrudnienie, lista płac, produktywność

`Position`, `Employment`, `Payroll`. Produktywność jako funkcja czysta w milijednostkach
(`i32`, nie float — wynik wpływa na stan trwały). Wypłata miesięczna z shardingiem po dniach.

*Kryterium ukończenia:* zakład z załogą wytwarza w M6 wynik proporcjonalny do `effective_labor`;
test LOD — przebieg mikro i mezo tego samego zakładu daje identyczną sumę wypłat (tolerancja 0).

### WP4 — Rynek pracy: oferty, aplikacje, wybór

**Lokalizacja: `sim/economy::labor`** — rozszerzenie crate'a M5, nie nowy crate (D2).
Rynek pracy jest rynkiem i korzysta z **tej samej** maszynerii ofert, tego samego indeksu
przestrzennego i tego samego mechanizmu dopasowania co rynek towarowy. Drugi, równoległy
mechanizm dopasowania jest jawnie zakazany. M7 jest autorem tego rozszerzenia, M5 właścicielem
crate'a — zmiany idą przez M5.

Firma publikuje `JobOffer` jako `Offer` w kategorii pracy. Mieszkaniec (hook w `sim/agents`)
składa `Application`. Firma wybiera wg scoringu. Indeks ofert per (`JobRoleId`, `DistrictId`)
zgodnie z §17.5 — kandydat rozważa 3–15 ofert, nie wszystkie.

*Kryterium ukończenia:* w scenariuszu z 5 tys. mieszkańców i 300 firmami bezrobocie zbiega do
pasma 3–9% w 90 dni gry; każde zatrudnienie ma `DecisionReason::Hire` z wynikiem kandydata
i wynikiem drugiego w kolejce.

### WP5 — Licytacja płac i niedobór

`LaborMarketStats` — indeks niedoboru per (`JobRoleId`, `DistrictId`), liczony dziennie z wakatów,
aplikacji i czasu wiszenia oferty. Eskalacja stawki przy braku kandydatów, headhunting (oferta
bezpośrednia do zatrudnionego) przy wysokim niedoborze, degresja stawek przy nadmiarze (nowe
oferty poniżej mediany, malejąca płaca progowa bezrobotnego).
**Nigdzie nie ma tabeli płac** — mediana w UI to agregat ofert i zaakceptowanych umów.

*Kryterium ukończenia:* test `wage_reacts_to_shortage` (§7) zielony w CI; brak spirali płacowej
w 5-letnim przebiegu (górne ograniczenie: firma nie licytuje powyżej płacy, przy której marża
zakładu spada poniżej progu z polityki).

### WP6 — HR

Szkolenia (koszt + czas + przyrost umiejętności z sufitem od talentu), premie (funkcja wyniku
zakładu i polityki), benefity (auto służbowe, opieka medyczna — realne zaspokojenie potrzeb
z §5.3, nie liczba dodawana do nastroju), zwolnienia z odprawą i kosztem reputacyjnym, rotacja
dobrowolna i przymusowa.

*Kryterium ukończenia:* benefit „opieka medyczna" mierzalnie podnosi poziom potrzeby *Zdrowie*
u pracownika w symulacji (nie tylko nastrój); odejście pracownika zawsze ma `DecisionReason`
po stronie odchodzącego (lepsza oferta / nastrój / stres / zwolnienie).

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

### WP8 — Finanse firmy

Kredyt obrotowy (linia odnawialna) i inwestycyjny (harmonogram rat), leasing (aktywo należy do
leasingodawcy do wykupu — kluczowe dla WP9), faktoring (sprzedaż należności z dyskontem),
obligacje (prosty zapis: nabywcy to mieszkańcy z oszczędnościami i inne firmy; rynek wtórny — M10).
Bank jest zwykłą firmą z polityką kredytową; kreację pieniądza księguje `sim/economy` (M5).

*Kryterium ukończenia:* test zachowania pieniądza przechodzi z włączonymi wszystkimi czterema
instrumentami; harmonogram rat sumuje się dokładnie do kwoty kredytu + odsetek (i64, reszta do
pierwszej raty wg reguły z dokumentu 00 §2).

### WP9 — Bankructwo i syndyk

Maszyna stanów `Filed → Valuation → Auction(rundy) → Distribution → Closed`.
Wyprzedaż majątku to **zwykłe oferty** na istniejącym rynku (M5/M6) z harmonogramem obniżek
i terminem — okazja dla gracza. Niesprzedane po ostatniej rundzie → wartość złomu.
Aktywa leasingowane wracają do leasingodawcy i nie wchodzą do masy.
Kolejność zaspokojenia: wynagrodzenia i odprawy → wierzyciele zabezpieczeni (do wartości
zabezpieczenia) → **zobowiązania publiczne: miasto jako wierzyciel podatkowy (M8 zgłasza
roszczenie, M7 je zaspokaja — K-10)** → wierzyciele niezabezpieczeni proporcjonalnie → właściciele.
Postępowanie przyjmuje wierzycieli z zewnątrz przez `file_claim` — M7 jest właścicielem
całej upadłości, żadna faza nie ma własnej ścieżki egzekucji.

*Kryterium ukończenia:* property-test `bankruptcy_conserves_money_and_assets` (§7) zielony na
10 000 losowych konfiguracji.

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

## 5. Projekt techniczny

### 5.1 Struktura crate'a

```
sim/firms/
  src/
    lib.rs
    key.rs            // FirmKey, sharding, sloty decyzyjne
    firm.rs           // Firm, Ownership, FirmBooks, DecisionLog
    site.rs           // Site, SiteTypeId, SiteDelegation
    catalog.rs        // ładowanie data/site_types/
    hr/
      position.rs     // Position, JobRoleId <- data/jobs/
      employment.rs   // Employment, Payroll, Severance
      productivity.rs // effective_labor()
      manager.rs      // Manager, management_quality()
      training.rs     // szkolenia, premie, benefity
      turnover.rs     // rotacja
    labor_policy.rs   // scoring kandydata i eskalacja stawek — REGUŁY firmy;
                      // sam rynek pracy mieszka w sim/economy::labor (D2)
    finance/
      loan.rs  lease.rs  factoring.rs  bond.rs
      liquidity.rs    // kolejka płatności, wykrycie niewypłacalności
      bankruptcy.rs   // syndyk, masa, zaspokojenie
    view/
      firm_view.rs    // FirmView — JEDYNE wejście danych do ai/
      board.rs        // PublicMarketBoard, RumorFeed
    ai/
      personality.rs  // FirmPersonality, FirmStrategy
      policy.rs       // FirmPolicy = zestaw reguł sim/policy + presety per strategia
      operational.rs  // tier 1
      tactical.rs     // tier 2
      strategic.rs    // tier 3 (+ macro)
      reaction.rs     // reakcja na gracza
    lifecycle/
      founding.rs  closure.rs  external_chain.rs
    reason.rs         // DecisionReason (wariant Firm), Decided<T>
    systems.rs        // rejestracja systemów i częstotliwości
sim/macro/            // WŁAŚCICIEL: M10. M7 buduje podzbiór wg M10-glebia.md §6:
  src/                //   MacroState/MacroCell/MacroFirm/MacroStock, lift(), step() fazy 2-6,
                      //   what_if(). NIE: lower(), dry_run(), fast_forward(), MacroLodPolicy

sim/policy/           // K-11: JEDEN silnik reguł dla AI i gracza. Język projektuje M9.
  src/
    ast.rs            // re-eksport/odwzorowanie AST M9: ConditionExpr, Expr, Metric,
                      // Action, PolicyScope — NIE własny język
    eval.rs           // ewaluator: arytmetyka całkowita, bez alokacji na ścieżce gorącej
    metrics.rs        // rejestr Metric -> odczyt WYŁĄCZNIE przez FirmView
    actions.rs        // rejestr Action -> OpsAction / TacAction
    scope.rs          // PolicyScope: firma / zakład / sklep / produkt + konflikty

sim/economy/          // WŁAŚCICIEL: M5. M7 dokłada moduł labor (D2):
  src/labor/
    offer.rs          // JobOffer, Application — na maszynerii ofert M5
    matching.rs       // wybór kandydata na indeksie przestrzennym M5
    bidding.rs        // eskalacja stawek, headhunting
    stats.rs          // LaborMarketStats, ShortageIndex
```

### 5.2 Firma, zakład, własność (§7.1)

```rust
/// Stabilny klucz firmy: monotoniczny licznik świata, trwały w zapisie gry.
/// Steruje shardingiem decyzji i strumieniem RNG. NIE jest indeksem ECS.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FirmKey(pub u64);

pub struct Firm {
    pub key: FirmKey,
    pub name: NameId,                 // z data/names/ + gramatyki
    pub founded: SimMinute,
    pub director: Option<CitizenId>,  // None => firma zewnętrzna albo zarząd tymczasowy
    pub owners: SmallVec<[OwnerShare; 4]>,
    pub hq_district: DistrictId,
    pub sites: SmallVec<[SiteId; 8]>,
    pub products: SmallVec<[GoodId; 16]>,   // portfel produktów
    pub contracts: Vec<ContractId>,         // B2B z M6, sortowane po ContractId
    pub personality: FirmPersonality,
    pub strategy: FirmStrategy,
    pub policy: FirmPolicy,
    pub status: FirmStatus,           // Active | Restructuring | Bankrupt(..) | Closed
    pub log: RingBuf<LoggedDecision, 32>,   // historia decyzji (§5.11)
}

pub struct OwnerShare { pub owner: Owner, pub bp: u16 }   // bp: 1/10000, suma == 10_000
pub enum Owner { Citizen(CitizenId), Firm(FirmId), Player, City, External(ChainId) }

/// Pieniądz firmy. Pełny RZiS/bilans/CF per zakład — kontrakt z M5 (§6.9).
pub struct FirmBooks {
    pub cash: Money,
    pub receivables: Vec<Receivable>,  // sortowane po (due, ContractId)
    pub payables:    Vec<Payable>,
    pub loans:  SmallVec<[Loan; 4]>,
    pub leases: SmallVec<[Lease; 4]>,
    pub bonds:  SmallVec<[Bond; 2]>,
    pub equity_history: RingBuf<MonthlyEquity, 60>,  // 5 lat — trend i wycena
}

pub struct Site {
    pub firm: FirmId,
    pub site_type: SiteTypeId,          // indeks do data/site_types/
    pub building: BuildingId,           // fizyka, maszyny, magazyn — M6
    pub district: DistrictId,
    pub positions: Vec<Position>,       // sortowane po JobRoleId
    pub manager: Option<Entity>,        // Manager
    pub delegation: Option<SiteDelegation>,
    pub shift_plan: ShiftPlan,          // 1–3 zmiany (§7.4)
    pub fixed_cost_month: Money,        // czynsz, amortyzacja, media bazowe
    pub pnl: RingBuf<SitePnlMonth, 36>, // wejście decyzji taktycznej
    pub opened: SimMinute,
}
```

**Katalog typów zakładów (§7.2) — struktura danych, nie lista w kodzie.**
`data/site_types/retail.ron`, fragment ilustracyjny:

```ron
SiteType(
    key: "supermarket",
    schema_version: 1,
    category: Retail,
    footprint_m2: (min: 800, max: 3500),
    lines: None,                                  // handel nie ma linii produkcyjnych
    shelf_capacity_m3_per_100m2: 45,
    staffing: [
        ( role: "cashier",      per_100m2: 0.9, min: 2 ),
        ( role: "stocker",      per_100m2: 0.5, min: 1 ),
        ( role: "shop_manager", per_site: 1,    managerial: true ),
    ],
    utilities: ( power_wh_per_m2_day: 380, water_ml_per_m2_day: 120, heat: Required ),
    dock_trucks_per_hour: 4,
    emissions: ( noise: 15, air: 0, water: 0 ),
    capex: ( build: 1_850_000_00, equip_per_100m2: 42_000_00 ),
    permitted_zones: [ Commercial, MixedUse ],
    epoch_from: "1960",
)
```

Cała lista z §7.2 (wydobycie, przetwórstwo pierwotne, produkcja, logistyka, handel detaliczny,
usługi dla ludności, usługi dla biznesu, finanse, media, nieruchomości) to **treść danych**,
nie kodu. Kod zna wyłącznie `SiteTypeCategory` i pola powyżej. Nowy typ zakładu = nowy rekord RON
+ role w `data/jobs/` + receptury w `data/recipes/` (M6). Walidator CI sprawdza domknięcie grafu.

### 5.3 Stanowiska, zatrudnienie, produktywność (§7.5, §6.6)

```rust
pub struct Position {
    pub role: JobRoleId,                 // data/jobs/: profil umiejętności, wykształcenie
    pub slots: u16,
    pub filled: SmallVec<[Employment; 8]>,
    pub managerial: bool,
    pub wage_band: (Money, Money),       // widełki z polityki firmy, miesięcznie
}

pub struct Employment {
    pub citizen: CitizenId,
    pub firm: FirmId,
    pub site: SiteId,
    pub role: JobRoleId,
    pub wage_month: Money,               // brutto; potrącenia = hook M8
    pub since: SimMinute,
    pub shift: ShiftId,
    pub benefits: BenefitSet,            // bitflagi: CompanyCar | Health | Meals | Training
    pub bonus_policy: BonusPolicy,
    pub perf_ema: u16,                   // wygładzona ocena 0..=1000
    pub warnings: u8,
}

/// Produktywność (§6.6). Wszystko w milijednostkach i32 — wynik wpływa na stan trwały,
/// więc żadnych f32 (dokument 00 §2).
pub fn effective_labor(
    emp:   &Employment,
    body:  &CitizenVitals,      // energia, nastrój, zdrowie — z sim/agents (M3)
    skill: Q,                   // umiejętność w zawodzie
    tech:  TechLevel,           // poziom wyposażenia zakładu (M6)
    mgmt:  ManagementQuality,   // jakość zarządzania zakładem
    w:     &RoleWeights,        // wagi z data/jobs/<role>.ron
) -> Qty;
```

Formuła (milijednostki, składane w ustalonej kolejności):

```
base = w.skill*skill + w.energy*energy + w.mood*mood01 + w.health*health   // Σw = 1000
out  = base * tech_mult(tech) / 1000 * mgmt_mult(mgmt) / 1000
       // tech_mult: 800..1400,  mgmt_mult: 850..1150
```

`mood01` to `Mood(-100..=100)` przeskalowany jawnie do 0..100 — żeby zły nastrój nie wytwarzał
ujemnej pracy. Wagi są per zawód: dla `researcher` liczy się umiejętność, dla `truck_driver`
energia i zdrowie. Zmiana wag = zmiana danych, nie kodu.

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

### 5.5 Rynek pracy jako rynek ofert (§6.6) — w `sim/economy::labor`

**Podział własności (D2):** typy i logika poniżej żyją w `sim/economy` (właściciel M5),
bo rynek pracy jest rynkiem i musi używać tej samej maszynerii ofert, tego samego indeksu
przestrzennego i tego samego dopasowania co rynek towarowy. M7 jest **autorem** tego rozszerzenia,
nie właścicielem crate'a. W `sim/firms` zostaje wyłącznie to, co jest decyzją *firmy*:
scoring kandydata, reguły eskalacji stawki, polityka headhuntingu — czyli `labor_policy.rs`.
Zakaz budowania drugiego, równoległego mechanizmu dopasowania jest wiążący.

```rust
pub struct JobOffer {
    pub id: OfferId,
    pub firm: FirmId, pub site: SiteId, pub role: JobRoleId,
    pub district: DistrictId,
    pub wage_month: Money,             // to jest licytowana zmienna
    pub slots: u16,
    pub shift: ShiftId,
    pub requirements: SkillReq,        // min. umiejętność, wykształcenie, doświadczenie
    pub benefits: BenefitSet,
    pub targeted: Option<CitizenId>,   // Some => headhunting, oferta bezpośrednia
    pub posted: SimMinute,
    pub expires: SimMinute,
    pub raises: u8,                    // ile razy podbito — do DecisionReason i do limitu
}

pub struct Application {
    pub offer: OfferId,
    pub citizen: CitizenId,
    pub wage_expectation: Money,       // płaca progowa kandydata (§5.6)
    pub skill: Q,
    pub referral: Option<CitizenId>,   // z grafu relacji §5.1
    pub submitted: SimMinute,
}
```

**Wybór kandydata** — scoring, nie argmax po jednej cesze:

```rust
pub fn score_application(a: &Application, o: &JobOffer, p: &FirmPolicy,
                         refs: &ReferralStrength, mem: &FirmMemory) -> i32;
// skill_fit   — dopasowanie do profilu; KARA za przekwalifikowanie (odejdzie szybko)
// wage_fit    — oczekiwanie <= oferta; zbyt niskie = sygnał ryzyka, nie okazja
// referral    — waga relacji z obecnym pracownikiem (§5.1)
// history     — pamięć firmy o kandydacie (był zwolniony? odszedł po miesiącu?)
// modyfikatory osobowości: quality_focus podnosi wagę skill_fit, price_focus — wage_fit
```

**Licytacja płac — rdzeń emergentności.** Nie ma tabeli płac. Są trzy sprzężenia:

1. **Eskalacja przy niedoborze.** Oferta wisi `d` dni bez akceptowalnego kandydata →
   `wage_month += step`, gdzie `step = base_step * (1 + aggression/100) * (1 + shortage_index)`,
   ograniczone twardo przez `wage_ceiling` z polityki: płaca, przy której marża zakładu spada
   poniżej `min_margin_bp`. To ograniczenie jest **jedynym** hamulcem spirali i musi być liczone
   z własnych kosztów firmy — czyli z danych prywatnych, zgodnie z `FirmView`.
2. **Headhunting.** Przy `shortage_index > próg` firma wysyła `JobOffer { targeted: Some(..) }`
   do pracownika konkurenta ze stawką `obecna * (1 + premia)`. Pracownik ocenia ją zwykłą regułą
   z §5.6 (użyteczność > próg; ambicja obniża próg, lojalność podwyższa).
3. **Degresja przy nadmiarze.** Płaca progowa bezrobotnego spada z czasem bezrobocia (funkcja po
   stronie `sim/agents`), więc oferty poniżej mediany znajdują kandydatów. Firma widzi to
   w `LaborMarketStats` i przestaje licytować.

```rust
pub struct LaborMarketStats {          // liczone EveryDay, dane PUBLICZNE
    // BTreeMap, nie HashMap (dokument 00 §3.2)
    pub per_role: BTreeMap<(JobRoleId, DistrictId), RoleStats>,
}
pub struct RoleStats {
    pub vacancies: u32,
    pub applicants_30d: u32,
    pub median_wage_posted: Money,
    pub median_wage_accepted: Money,
    pub median_days_to_fill: u16,
    pub shortage_index: u16,           // 0..=1000 z wakatów, aplikacji i czasu wakatu
}
```

`LaborMarketStats` widzą i gracz, i AI — to zamierzone: **płace w ofertach są jawne, koszty
jednostkowe nie są**.

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

### 5.12 Finanse firmy (§7.8)

```rust
pub struct Loan {
    pub id: LoanId, pub lender: FirmId,         // bank jest zwykłą firmą
    pub kind: LoanKind,                         // Working (linia odnawialna) | Investment
    pub principal: Money, pub outstanding: Money,
    pub rate_bp: u16,                           // marża + BaseRate z sim/economy (M5)
    pub schedule: SmallVec<[Installment; 12]>,  // suma rat == kapitał + odsetki, co do grosza
    pub collateral: Option<AssetRef>,           // zabezpieczenie -> priorytet w bankructwie
    pub missed: u8,
}
pub struct Lease {                              // aktywo NALEŻY do leasingodawcy do wykupu
    pub lessor: FirmId, pub asset: AssetRef,
    pub monthly: Money, pub months_left: u16, pub buyout: Money,
}
pub struct Bond { pub face: Money, pub coupon_bp: u16, pub maturity: SimMinute,
                  pub holders: Vec<(Owner, Money)> }   // rynek wtórny: M10
```

Faktoring: sprzedaż `Receivable` (z §6.9) za `wartość × (1 − discount_bp)` — natychmiastowa gotówka
kosztem marży. Typowe wyjście z chwilowej utraty płynności i typowy sygnał kłopotów dla obserwatora.

Ocena zdolności kredytowej przez bank: historia spłat, przepływy z `equity_history`, zabezpieczenie,
`BaseRate` i polityka kredytowa banku. Bank może odmówić (`CreditDenial`) — i to jest częsty
mechanizm wpychający firmę w bankructwo, a nie awaryjny wyjątek.

### 5.13 Bankructwo z syndykiem (§7.8) — postępowanie prowadzi M7 (K-10)

**Własność (K-10):** `Bankruptcy` należy w całości do M7. Inne fazy są wyłącznie **wierzycielami
zgłaszającymi roszczenie** do postępowania, które prowadzi M7 — w szczególności M8 (urząd
skarbowy z zaległościami podatkowymi) nie ma własnej ścieżki egzekucji.
Dlatego postępowanie jest zaprojektowane jako **otwarte na wierzycieli z zewnątrz**.

```rust
pub struct Bankruptcy {
    pub firm: FirmId,
    pub opened: SimMinute,
    pub trigger: BankruptcyTrigger,  // IlliquidDays{n} | NegativeEquityAndDefault | CourtOrder
    pub stage: BankruptcyStage,      // Filed | Valuation | Auction{round} | Distribution | Closed
    pub estate: Vec<AssetLot>,       // sortowane po (kind, id) — determinizm
    pub claims: Vec<Claim>,          // sortowane po (priority, creditor) — determinizm
    pub proceeds: Money,
    pub trustee_fee_bp: u16,
}

pub struct Claim {
    pub creditor: Creditor,
    pub priority: ClaimPriority,
    pub amount: Money,
    pub collateral: Option<AssetRef>,   // Some => Secured do wartości zabezpieczenia,
                                        // nadwyżka spada do Unsecured (jawnie, nie po cichu)
    pub filed: SimMinute,
    pub origin: ClaimOrigin,            // do karty inspekcji: skąd wziął się dług
}

/// Wierzyciele z ZEWNĄTRZ postępowania — lista jest zamknięta w kodzie, ale rozszerzalna per faza.
pub enum Creditor {
    Employee(CitizenId),      // zaległe pensje i odprawy — M7
    Bank(FirmId),             // kredyty, leasing — M7
    Supplier(FirmId),         // niezapłacone faktury B2B — M6/M7
    BondHolder(Owner),        // obligacje — M7
    City(ClaimKind),          // ZALEGŁOŚCI PODATKOWE, opłaty, kary — M8 zgłasza, M7 zaspokaja
    Landlord(Owner),          // zaległy czynsz za parcelę/budynek — M10
    Other(FirmId),            // kary umowne z kontraktów (§6.2)
}

pub enum ClaimPriority {
    Wages     = 0,  // zaległe wynagrodzenia
    Severance = 1,  // odprawy
    Secured   = 2,  // wierzyciele zabezpieczeni, do wartości zabezpieczenia
    Public    = 3,  // MIASTO: podatki, składki, opłaty, kary administracyjne (M8)
    Unsecured = 4,  // dostawcy, obligatariusze, kary umowne, nadwyżki ponad zabezpieczenie
    Owners    = 5,  // właściciele — to, co zostanie (zwykle nic)
}

/// JEDYNA droga zgłoszenia roszczenia. Woła ją M8 (podatki), M6 (dostawcy), M10 (czynsz).
/// Po przejściu w stage Distribution zgłoszenia są odrzucane (ClaimTooLate) — termin jest twardy,
/// żeby postępowanie było deterministyczne i skończone.
pub fn file_claim(b: &mut Bankruptcy, c: Claim) -> Result<ClaimId, ClaimRejected>;
```

Kolejność zaspokojenia jest **jedną stałą tablicą** `ClaimPriority` — zmiana układu (np. D7)
nie wymaga zmiany logiki podziału. W obrębie priorytetu podział proporcjonalny z regułą reszty
z dokumentu 00 §2, przy remisach sortowanie po `(Creditor, ClaimId)`.

Do czasu M8 miasto po prostu nie zgłasza roszczeń (`Creditor::City` istnieje, nikt go nie używa) —
wpięcie M8 nie zmienia ani struktury, ani testu §7.2, tylko dokłada wpisy do `claims`.

Przebieg:

1. **Filed** — natychmiast: wszystkie `Employment` firmy kończą się
   (`QuitCause::EmployerBankrupt`), naliczane są odprawy jako `Claim { Severance }`;
   zakłady wstrzymują produkcję (M6). **Aktywa leasingowane wracają do leasingodawcy**
   i nie wchodzą do masy.
2. **Valuation** — wycena lotów: budynki i maszyny wg wartości księgowej po amortyzacji,
   zapasy wg kosztu, należności wg dyskonta jakości dłużnika.
3. **Auction** — loty trafiają **na zwykły rynek ofert M5/M6** jako `Offer` z ceną
   `wycena × (1 − dyskont_rundy)`; 3 rundy po 15 dni gry (pół miesiąca w siatce 30-dniowej),
   dyskont 0% / 25% / 50%; całe postępowanie mieści się w 1,5 miesiąca.
   Gracz widzi je w panelu Rynek jak każdą inną ofertę. To jest cała „okazja dla gracza" —
   bez osobnego podsystemu aukcyjnego, bez osobnego UI, bez drugiego mechanizmu wyceny.
4. **Distribution** — zaspokojenie wg `ClaimPriority`; w obrębie priorytetu proporcjonalnie,
   z regułą reszty z dokumentu 00 §2 (reszta do pierwszego wg ustalonego porządku).
5. **Closed** — loty niesprzedane po rundzie 3 idą po wartości złomu; jeśli i to zawiedzie,
   są **jawnie spisywane** rekordem `AssetWriteOff` — nie znikają po cichu (test §7.2).
   Firma → `FirmStatus::Closed`; dyrektor wraca na rynek pracy z piętnem w pamięci firm.

Skutki w dzielnicy (§7.8: „skutki w dzielnicy") są emergentne: fala bezrobocia → spadek popytu
w okolicznych sklepach → spadek wartości gruntu. Żadnych modyfikatorów „na sztywno".

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
firms::hr::effective_labor(..) -> Qty                   // -> M6: wykonanie receptury
firms::hr::loss_multiplier(site) -> Milli               // -> M6: straty magazynowe i produkcyjne
firms::labor_policy::score_application(..) -> i32       // decyzja FIRMY o kandydacie
firms::labor_policy::wage_escalation_step(..) -> Money  // decyzja FIRMY o podbiciu stawki
firms::finance::request_loan(firm, kind, amount) -> Result<LoanId, CreditDenial>
firms::finance::open_bankruptcy(firm, trigger) -> BankruptcyId
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
| M0 `core`/`ecs`/`jobs` | `Money`, `SimMinute`, `Tick`, `Qty`, `Q`, `Mood`, identyfikatory, `StreamId` (zakres M7: 240–259), kalendarz 12 × 30 dni = 360 (K-1), bufory komend, fork-join, hash stanu |
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
               == Σ gotówka PO + opłata syndyka
               (gotówka nie powstaje ani nie ginie; opłata syndyka to jawny transfer)
2. Aktywa:     każdy AssetLot kończy w DOKŁADNIE jednym ze stanów: sprzedany (nowy właściciel)
               | zwrócony leasingodawcy | AssetWriteOff — suma liczności == suma początkowa,
               przecięcia zbiorów puste
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
