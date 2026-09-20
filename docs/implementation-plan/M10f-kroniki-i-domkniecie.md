# M10f — Kroniki i domknięcie

Podfaza 6 z 7 fazy **M10 — Głębia** (`M10-glebia.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M10a–M10e. |
| **Pakiety robocze** | WP10.15, WP10.16, WP10.17, WP10.18 |
| **Projekt techniczny** | §5.10 |
| **Wynik do pokazania** | Świat, którego historię widać: kronika niesie osiemdziesiąt lat sprzed partii i uchwały rady, bramki Etapu 10 są zielone, a bramka balansatora porównuje przebieg mezo z makro. |
| **Kryterium zamknięcia** | Kryteria WP10.15–WP10.18. Bramki 1–7 fazy M10 zamyka dopiero `M10g`. |
| **Poprzednia / następna** | `M10e-relacje-i-zwiazki.md` · `M10g-panele-i-komendy.md` |

Kroniki, determinizm i domknięcie pomiarów fazy.

## Dlaczego podfaz jest siedem, a nie sześć

Plan przewidywał dla M10f dwa pakiety rozmiaru S — kroniki i hash. Tabele
„Zmiany wpisane po M10b/c/d/e" poniżej dołożyły do tego adresu jeszcze
**trzy panele z komendami gracza, cztery karty inspekcji i przebieg pomiarowy
całej fazy**: `FF-1`, `FF-8`, `FF-9`, `FF-11`, `FF-14`, `FF-15`, `FF-17`,
`FF-18`, `FF-21`, `FF-22`, `FF-23`, `FF-24`. Do tego doszły dwie pozycje
z M10a, które nie miały gdzie indziej trafić: bramki Etapu 10 (`E-12`)
i bramka balansatora makro↔mezo (`E-13`).

To jest praca czterech pakietów, dwóch rozmiaru L — a nie dwóch pakietów
rozmiaru S. `K-17` dopuszcza poprawienie podziału w dokumencie fazy, więc
podział jest poprawiony: **M10f zamyka symulację** (kronika, determinizm,
bramki, pomiary), **M10g zamyka wejście gracza** (panele, komendy, karty).
Granica jest ostra i wynika z kierunku zależności: M10f nie dotyka `game/`
poza kroniką, M10g nie dotyka `sim/*` poza jednym punktem podstawienia
w negocjacjach.

---

## Pakiety robocze

Kolejność jest kolejnością zależności: kronika i hash są niezależne, bramki
potrzebują obu, a przebieg pomiarowy potrzebuje bramek, żeby wiedzieć,
czy mierzy świat zdrowy.

### WP10.15 — Kroniki i wyjaśnialność

**Zależności:** M9 (podsystem kroniki), wszystkie WP tej fazy.
Przekrojowy. Każda nowa decyzja ma `DecisionReason` (dok. 00 §7).

**Co jest już zrobione po M10e i czego nie trzeba powtarzać:** `reason::describe`
ma ramię dla wszystkich dwudziestu pięciu powodów 800–824, a `data/locale/pl.ron`
i `en.ron` mają komplet kluczy po obu stronach. Dziennik redakcji jest zbierany
(`FF-2`), a decyzje giełdy, ubezpieczeń, zmów i związków idą przez `Firm.log`
i tą drogą trafiają do kroniki **firm gracza** (`FF-20`).

**Co zostaje do zrobienia — dwie rzeczy:**

1. **Historia „na sucho" wchodzi do kroniki** (M10 §2 pkt 14, §6 pkt 7, ryzyko `R9`).
   `DryRunResult.chronicle` jest dziś czytany wyłącznie przez `tools/headless`
   i ginie razem z procesem. Kronika gracza dostaje **pochodzenie** wpisu
   (`Provenance::{Live, DryRun}`) i most jednokierunkowy z `DK-2`: wpis sprzed
   partii niesie własny rok, bo `SimMinute` nie umie liczyć wstecz od zera świata.
2. **Uchwały rady i wybory wchodzą do kroniki.** `City.reasons` jest pierścieniem
   na 256 wpisów, który dziś nikt nie czyta; są w nim decyzje widoczne dla
   **każdego** gracza niezależnie od tego, co posiada — podwyżka VAT-u, wynik
   wyborów, uchwalona polityka. Decyzji dwóch tysięcy firm AI nadal nie zbieramy
   i to zostaje bez zmian (sufit nazwany w nagłówku `game::chronicle`).

**Kryterium ukończenia:** kronika po przebiegu `dry-run` niesie wpisy sprzed
startu partii i da się je odfiltrować po pochodzeniu; uchwała rady zmieniająca
stawkę podatku pojawia się w kronice w dobie, w której zapadła; audyt ręczny
20 wpisów pod kątem zrozumiałości dla gracza.
**Rozmiar: S.**

---

### WP10.16 — Determinizm i hash stanu

Przekrojowy, Definition of Done fazy.

**Co jest już zrobione po M10e:** wszystkie zasoby M10 mają `impl HashState`
i są wpięte — `MacroHandle`, `BrandSlab`, `Campaigns`, `Outlets`, `EdgeWatch`
(przez `TrafficNetwork`), `RndState` (przez `Firms`), `Equity`, `Insurers`,
`Unions`, `Cartels`, relacje B2B (przez `ChainHandle`). Bloki `StreamId`
280–295 i `DecisionReason` 800–824 są rozpisane i wieczne.

**Co zostaje:** **testu dwóch przebiegów obejmującego M10 nie ma.**
`sim/agents/tests/determinism.rs` pilnuje mieszkańców, `sim/economy` i `sim/firms`
mają własne testy hasha, `macro_lod.rs` porównuje dwa dry-runy — ale żaden
nie stawia świata z mediami, giełdą, ubezpieczeniami i związkami naraz
i nie porównuje **ciągu** hashy dwóch przebiegów. Bez niego zdanie „stan M10
jest w hashu" opiera się na przeglądzie kodu, a nie na pomiarze.

**Kryterium ukończenia:** test stawiający pełny świat (`game::world::stand_up`
z gospodarką) i przepuszczający go przez co najmniej trzy doby daje identyczny
ciąg hashy w dwóch przebiegach tego samego ziarna, przy jednym i przy wielu
wątkach; test jest w CI.
**Rozmiar: S.**

---

### WP10.17 — Bramki Etapu 10 i bramka balansatora makro↔mezo

**Zależności:** WP10.16. Przejmuje `E-12` i `E-13` z `M10a-jadro-makro-historia.md`,
bo to jedyne dwie pozycje WP10.3 i WP10.4, które tamta podfaza zostawiła otwarte.

Trzy rzeczy:

1. **Bramka 7 rozdziela się na dwie** — wykonanie decyzji `D9` fazy zgodnie
   z propozycją domyślną, rozstrzygniętej przez właściciela produktu 2026-09-20.
   Bramka 7 mierzy **Gini majątku** w paśmie 550–850 ‰ (bo to jest pasmo realnego
   współczynnika Giniego majątku i mierzy Etap 8 generatora), a nowa **bramka 9**
   mierzy **Gini dochodu** w paśmie 250–450 ‰ (bo to mierzy rynek pracy). Mylenie
   ich ukrywało obie: bramka świeciła na czerwono od pierwszego pomiaru i nikt nie
   wiedział, czy to wada generatora, czy wada progu.
2. **Bramki 1, 3 i 5 mierzy się ponownie**, bo `M10e` zmieniło wejście, na którym
   stały: `CommuteMatrix` wypełnia się od `GE-12` geometrią miasta, a rekrutacja
   w makro jest ważona gotowością do dojazdu. `E-12` przypisywało czerwień bramek
   1 i 3 **płaskiej macierzy** — pomiar rozstrzyga, czy to była cała przyczyna.
3. **Bramka `G12` balansatora porównuje przebieg mezo z makro.** `E-13` odkładało
   ją jako „bieg nocny z własnym budżetem", bo rok gry mezo kosztuje ~10 min na
   ziarno. Droga tańsza i wystarczająca: balansator **i tak** prowadzi przebieg
   mezo, więc `lift()` w dobie zero i krok makro obok kosztują 8 ms na dobę —
   dwie trajektorie z jednego przebiegu. Bramka porównuje agregaty miesięczne
   i stosuje kryterium `K3` z §7.3 (nachylenie regresji, nie odchylenie
   pojedynczego miesiąca), bo to ono jest prawdziwym testem jednego modelu.

**Kryterium ukończenia:** `headless dry-run` na trzech rozmiarach miasta wypisuje
dziewięć bramek i wszystkie są zmierzone; `balansator gate --profile nightly`
zna `G12` i wydaje werdykt z nachyleniem regresji.
**Rozmiar: M.**

---

### WP10.18 — Przebieg pomiarowy fazy

**Zależności:** WP10.17. Zbiera osiem pomiarów, które podfazy M10b–M10e
zostawiły z adresem „M10f" — każdy jest liczbą, której nikt nie zmierzył,
a od której zależy, czy mechanika w mieście z generatora w ogóle zachodzi.

| Skąd | Co zmierzyć | Próg z planu |
|---|---|---|
| `FF-8` | udział systemów reklamowych w budżecie ticku przy 2000 kampanii | ≤ 1,5 % (§7.5) |
| `FF-11` | badacze na etatach, projekty w toku, odkrycia, patenty | 4 badaczy → węzeł 1200 RP w 9 ± 1 mies. (§7.4) |
| `FF-17` | ile oddziałów ubezpieczeniowych stawia generator | ≥ 1; zero znaczy miasto bez polis |
| `FF-18` | firmy spełniające warunek debiutu, gospodarstwa nad progiem inwestora, przecięcia w arkuszu | pierwszy debiut nie przed dobą 255 |
| `FF-21` | szkody na rok gry uśrednione po ziarnach (powódź, pożar magazynu) | `base_ppm` zgadnięte, do weryfikacji |
| `FF-24` | zakłady nad progiem żalu, zmowy na rok, odsetek firm rocznie w strajku | strajk < 5 % firm rocznie (`R6`), 20–50 % zmów wykrytych w 5 lat (`R8`) |
| `FF-25` | ile marek rocznie obrywa od prasy | decyduje, czy `SCANDAL_MAX_DROP` idzie do `data/tuning/brand.ron` |
| `D4` | mediana liczby marek, z którymi mieszkaniec ma realny kontakt | > 13 znaczy podnieść `BRAND_SLOTS` z 16 na 24 |

Pomiar jest **wynikiem**, nie kodem: pakiet jest zamknięty, gdy liczby są
zmierzone i zapisane w tabeli na końcu tego dokumentu, a każda z nich ma
werdykt „mieści się w progu" albo „nie mieści się i oto adres naprawy".
Pułapka nazwana w `FF-8` obowiązuje wszystkie wiersze: doba `m7miasto`
kosztuje 1,15 s w piątej dobie i ~15 s w czterdziestej **bez udziału nowego
kodu**, więc porównanie dwóch różnych dób mierzy wzrost gospodarki.

**Kryterium ukończenia:** tabela pomiarów jest wypełniona, a każda liczba
poza progiem ma wpisany adres — pakiet naprawczy albo decyzję otwartą.
**Rozmiar: M.**

---

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.10 Systemy ECS i ich częstotliwość

| System | Częstotliwość | LOD | Odczyt/zapis |
|---|---|---|---|
| `brand_decay_compact` | `EveryMonth` | mezo | W: `BrandSlots` |
| `ad_expose_billboard` | `EveryHour` | mezo | R: przejazdy M4; W: `BrandSlots`, `CampaignMetrics` |
| `ad_expose_media` | `EveryDay` | mezo | R: `MediaOutlet`; W: `BrandSlots` |
| `ad_expose_leaflet` | `EveryDay` | mezo | R: indeks przestrzenny M2; W: `BrandSlots` |
| `ad_budget_burn` | `EveryDay` | — | W: `Ledger` przez `ledger_post` |
| `brand_strength_aggregate` | `EveryDay` | — | R: `BrandSlots`; W: agregat UI |
| `media_editorial` | `EveryDay` | — | R: `sim/events`; W: `Rumor` |
| `rnd_progress` | `EveryDay` | — | R: badacze, budżet; W: `ResearchProject` |
| `rnd_unlock` | `EveryDay` | — | W: `Patent`, efekty technologii |
| `epoch_advance` | `EveryMonth` | — | W: `EpochState`, koszyki potrzeb |
| `investor_decide_funds` | `EveryWeek` | — | W: `Order` |
| `investor_decide_citizens` | `EveryMonth` | — | W: `Order` |
| `stock_fixing` | `EveryDay` (17:00) | — | R/W: `OrderBook`, `Holding`, `Share` |
| `earnings_publish` | `EveryDay` (sprawdza harmonogram T+45) | — | W: `PublishedReport` |
| `takeover_check` | `EveryDay` | — | W: `FirmPersonality` (M7), `Rumor` |
| `dividend_pay` | `EveryMonth` | — | W: `Ledger` |
| `insurance_underwrite` | `EveryMonth` | — | R: `PerilStats`; W: `InsurancePolicy` |
| `insurance_claims` | `EveryDay` | — | R: `sim/events`; W: `Ledger`, `PerilStats` |
| `supplier_trust_update` | `EveryDay` | mezo+makro | W: `SupplierRelation` |
| `cartel_detect` | `EveryMonth` | — | W: kary, `BrandSlots`, kronika |
| `union_grievance` | `EveryDay` | — | R: księgi M7, graf relacji M3 |
| `union_negotiate` | `EveryWeek` | — | W: `Union`, płace |
| `strike_tick` | `EveryDay` | — | W: produkcja zakładu, `Ledger` |
| `macro_step` | `EveryDay` (tylko w trybie makro) | makro | R/W: `MacroState` |

---

## Wyniki przebiegu pomiarowego (WP10.18)

Osiem liczb, których podfazy M10b–M10e nie zmierzyły. **Przyrząd dobrany do
pytania, nie do przyzwyczajenia** — i to jest główny wniosek tego pakietu.

### Dlaczego nie jeden długi przebieg miasta

Pierwsza wersja tego pakietu uruchamiała trzy przebiegi po 400 dób i **dwa
z ośmiu pomiarów mierzyłaby źle**:

- **`FF-21`** („szkód na rok gry uśrednione po ziarnach") jest własnością
  katalogu zdarzeń, a nie miasta. Przebieg 400 dób to **jedna** próbka przy
  oczekiwanych 0,65 powodzi na rok — zero niczego nie dowodzi i jedna też nie.
  Arytmetyka katalogu daje rozkład i liczy się w milisekundach.
- **`FF-8`** („≤ 1,5 % budżetu ticku przy **2000** kampanii") nie zmierzyłby się
  nigdy: firmy AI otwierają ich kilkadziesiąt. Kampanie trzeba **postawić**,
  a nie doczekać.

Do tego `FF-11` było już zmierzone przez M10c, a M10f nie dotknął R&D, i `FF-17`
widać w nagłówku przebiegu. Zostały cztery pomiary, którym wystarczył **jeden**
przebieg 300 dób przy mniejszej populacji startowej — 801 s zamiast godzin.

### Tabela

Przebieg: `m7miasto --seed 1 --size 4km --citizens 6000 --days 300`
(populacja rośnie migracją do 22 731), 215 → 236 firm.

| # | Pomiar | Zmierzone | Próg | Werdykt |
|---|---|---|---|---|
| `D4` | mediana marek na znającego markę | **6** (maks. 16, 457 ‰ mieszkańców zna jakąkolwiek) | podnieść `BRAND_SLOTS` przy > 13 | **zielone — zostaje 16**, decyzja `D4` zamknięta |
| `FF-17` | biura ubezpieczeniowe z generatora | **5**, polis czynnych 118, 987 opłaconych polisomiesięcy | ≥ 1 | **zielone** |
| `FF-11` | badacze / projekty / patenty | 9 / 25 / 0, opłacony budżet badań **760 ‰** | węzeł 1200 RP w 9 ± 1 mies. | **czerwone** — ta sama przyczyna co `FF-18`, adres niżej |
| `FF-18` | notowania / gospodarstwa nad progiem inwestora | **0** / **202** | pierwszy debiut ≥ doba 255 | **czerwone** — warunek debiutu wymaga dodatniego **opublikowanego** wyniku |
| `FF-24` | zakłady nad progiem żalu / zmowy / strajki | **0 z 123** mierzonych / **17 czynnych** / **0** | strajk < 5 % firm rocznie (`R6`), 20–50 % zmów wykrytych w 5 lat (`R8`) | `R6` **zielone** (0 %), `R8` **niezmierzone** — 0,83 roku to za mało na wykrycie |
| `FF-21` | szkody na rok gry | 4 km: powódź **0,65**, pożar **0,79**; metropolia: **1,94** i **2,00** | mechanizm widoczny dla gracza | **zielone**, z jednym znaleziskiem — niżej |
| `FF-8` | koszt 2000 kampanii na dobę gry | **12 514 ms** | ≤ 225 ms (1,5 % doby) | **czerwone 55×**, przyczyna zlokalizowana co do funkcji |
| `FF-25` | marki obrywające od prasy | **0 na rok** — redakcja nie opublikowała ani jednego tekstu | liczba, która rozstrzyga, czy `SCANDAL_MAX_DROP` idzie do danych | **niezmierzone**, adres niżej |

### Znaleziska, których żaden z tych pomiarów nie szukał

**1. Karencja zdarzenia wiąże powyżej 556 zakładów, i to zmienia model.**
`cooldown_days` jest przerwą **definicji**, nie podmiotu (`CH-11`), więc jeden
pożar magazynu ucisza wszystkie zakłady w mieście na 180 dób. Hazard rośnie
z liczbą zakładów, realizowana częstość ma sufit `360 / cooldown_days`. Przy
4 km (220 zakładów) hazard 0,79/rok mieści się pod sufitem 2,00; przy metropolii
(2200 zakładów) hazard 7,92/rok jest **przycięty do 2,00**. Konsekwencja, której
nikt nie zapisał: **duże miasto ma pożarów na zakład czterokrotnie mniej niż
małe**, a dołożenie magazynów ich nie dokłada. Mierzy to
`sim/events/tests/peril.rs::karencja_wiaze_powyzej_progu_liczby_zakladow`.

**2. Cały nadmiar `FF-8` siedzi w dwóch kanałach z ośmiu.** Rozbicie (250 kampanii
na kanał, 20 tys. mieszkańców, doba gry): Billboard 2 ms, Press 1 ms, Radio 1 ms,
Tv 2 ms, Leaflet 1 ms, Sponsorship 10 ms — i **InStorePromo 6079 ms** oraz
**Pr 6479 ms**. Przyczyna jest jedna i ta sama: `promocja` (`sim/media/src/system.rs`)
zbiera mieszkańców **wszystkich dzielnic** i pyta każdego `knows_place`, a `pr`
robi to samo z `affinity_of` i dodatkowo klonuje tabelę pamięci marki na kampanię.
To jest przebieg po całej populacji **na kampanię, na dobę**. W świecie pomiaru
oba kanały nie docierają do **nikogo** (relacje puste, sklepów nikt nie zna), więc
te 12,5 s to koszt samego szukania — w prawdziwym mieście będzie wyższy, nie niższy.
Adres naprawy: indeks „marka → mieszkańcy, którzy ją znają" albo jeden przebieg
po populacji na dobę zamiast na kampanię. **Należy do `M10g` albo `R2`** — jest to
przebudowa wydajnościowa, a nie domknięcie pomiaru.

**3. Bramka `G12` w pierwszym przebiegu znalazła dwie różne rzeczy, i to jest
argument za tym, żeby w ogóle istniała.** Obie serie są czerwone, ale z **innych
powodów**, a rozróżnienie wyszło dopiero po zmierzeniu bramek Etapu 10 (niżej).

*Pieniądz gospodarstw — mediana odchylenia **879 ‰**.* Przyczyna jest po stronie
mezo: pieniądz wycieka z ksiąg firm do gospodarstw (**395 → 229 mln zł**
w księgach wobec **6 → 169 mln zł** u ludzi przez 300 dób), a makro tego nie
odtwarza, bo w nim płace idą z ksiąg firm do gospodarstw i tyle. Otwarty kanał
`FF-29`: `PayrollOutbox` nie ma konsumenta od M7b, więc dochód gospodarstwa jest
egzogeniczny i **pieniądz firm ubywa drugą drogą**.

*Bezrobocie — mediana odchylenia **863 ‰**.* Tu przyczyna jest po stronie
**makra i należy do M10**: bramka 4 Etapu 10 pokazuje bezrobocie makro równe
**zeru** na obu zmierzonych rozmiarach miasta. Rekrutacja ważona gotowością
do dojazdu (`GE-12`, decyzja `D10` wykonana w M10e) zatrudnia praktycznie
wszystkich — czyli zaszło dokładnie to, przed czym `E-12` ostrzegało przy
wcześniejszej próbie otwarcia puli na całe miasto („bezrobocie schodzi wtedy
do zera"). Odsetek firm bez obsady spadł przy tym z 291 ‰ do **167 ‰**, więc
poprawka pomogła w połowie i w połowie zaszkodziła.

**`G12` jest doradcza**, bo jedna z jej dwóch serii jest zablokowana kanałem,
którego jeszcze nie ma — tą samą drogą co `G4` do czasu M6. Seria bezrobocia
**nie ma tego usprawiedliwienia** i powinna zzielenieć po naprawie rekrutacji
makro, niezależnie od `FF-29`. Gdyby nie ta bramka, o zerowym bezrobociu
w makrze nie dowiedziałby się nikt: mezo go nie widzi, a `what_if()` zwraca
uporządkowanie wariantów, nie poziom.

**5. Bramki Etapu 10 po decyzji `D9` i po `GE-12`.** Przebieg
`headless dry-run --years 30`, dwa rozmiary miasta:

| Bramka | 4 km | 8 km | Pasmo | Uwaga |
|---|---|---|---|---|
| 7 Gini **majątku** | **704** ‰ ✅ | 872 ‰ | 550–850 | **decyzja `D9` działa** — bramka, która świeciła na czerwono od pierwszego pomiaru, jest zielona na 4 km; 8 km wychodzi 22 ‰ nad krawędź |
| 10 Gini **dochodu** (nowa) | 613 ‰ | 507 ‰ | 250–450 | czerwona; mierzy dyspersję płac **między pracodawcami**, a ta jest w tym modelu bardzo szeroka |
| 4 bezrobocie | **0** ‰ | **0** ‰ | 30–150 | **regresja po `GE-12`** — patrz znalezisko 3 |
| 3 firmy bez obsady | 167 ‰ | 139 ‰ | 0 | poprawa z 291 ‰ (`E-12`), ale nie do zera |
| 1 nierównowaga | 1000 ‰ | 1000 ‰ | ≤ 300 | bez zmian wobec `E-12` |
| 5 mediana dźwigni | 0 ‰ | 0 ‰ | 100–600 | bez zmian |
| 8 koszyk/dochód | 0 ‰ | 0 ‰ | 250–550 | niemierzalny przy zerowym bezrobociu i zerowej dźwigni |
| 2, 6 | ✅ | ✅ | — | zielone |

Kryterium WP10.3 „100 % ziaren przechodzi Etap 10" **nadal nie jest spełnione**
i to jest stan zapisany, nie przemilczany — ale po raz pierwszy wiadomo, że
jedna z czerwonych bramek jest skutkiem poprawki z poprzedniej podfazy,
a nie stanu zastanego. Adres: `M10g` razem z resztą rekrutacji makro.

**4. Trzy czerwone pomiary mają jedną przyczynę.** `FF-11` (badania pełzną),
`FF-18` (zero debiutów) i niski udział kampanii (6 zamiast 35 z M10b) to
skutki tego samego wycieku co w znalezisku 3: firma bez gotówki nie opłaca
badań, nie publikuje dodatniego wyniku i schodzi z reklamą na najtańszy kanał.
M10c nazwał to przy `FF-11`; ten przebieg pokazuje, że dotyczy trzech mechanik
naraz, a nie jednej. **Adres jest wspólny i jest nim `FF-29`.**

**4a. A najtańszy kanał nie dociera do nikogo — i to jest osobna usterka.**
Histogram kanałów żywych kampanii, dopisany do raportu właśnie po to, żeby
nie zgadywać: **`Leaflet` 6 kampanii, 0 ekspozycji** — i żadnego innego kanału.
Zubożałe firmy schodzą na ulotki (`ai::monthly` wybiera kanał z zasobności),
a ulotki milczą. `ulotki` (`sim/media/src/system.rs`) ma **dokładnie jedno**
wyjście, które kończy się zerem bez ani jednej próby doręczenia: brak
współrzędnej zakładu w `PlaceCatalog` (`coord_of(PlaceRef::Site(origin))`
zwraca `None`). Kandydatem na kampanię jest **każdy zakład z firmą**, a katalog
miejsc zna te, które mieszkaniec odwiedza — więc zakład produkcyjny wypada
z niego z definicji. To tłumaczy, dlaczego M10b mierzyło 482 tys. ekspozycji
przy 40 dobach (bogate firmy kupowały prasę i telewizję), a ten przebieg zero
przy 300 (biedne kupują ulotki). Mechanizm wyglądał na działający przez dwie
podfazy, bo nikt nie patrzył na kanał, tylko na sumę. Adres: `GF-2` w `M10g`.

---


---

## Zmiany wpisane po M10b

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M10b.
Gwiazdka = zmiana zakresu albo kryterium. Szczegóły — tabela `F-n` i sekcja
„Co M10b zostawia następnym podfazom" w `M10b-marka-i-media.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| FF-1 ★ | **Panel marketingu i komenda „kup reklamę" należą do tej podfazy.** M10b otwiera kampanie wyłącznie przez `magnat_media::ai` — raz na miesiąc, dla firm AI. Gracz nie ma jak kupić billboardu, więc artefakt C z §1 dokumentu fazy nie jest jeszcze osiągalny | `DK-3` opisuje drogę: panel dokłada się przez `PanelRegistry::register`, a `PanelId::Brand` już istnieje i `is_reserved()` mówi o nim prawdę. Dane dla panelu są gotowe: `CampaignMetrics` niesie ekspozycje i **pierwsze kontakty**, czyli lejek „znajomość → próba → afinitet" z M10 §6 pkt 1 |
| FF-2 | **Dziennik redakcji jest gotowy do zebrania: `Outlets::reasons() -> &[(Tick, DecisionReason)]`**, pierścień 128 wpisów, w hashu stanu | Wykonanie `DK-1`: zdarzenie M10 zapisuje powód **u siebie**, a `game::chronicle::Chronicle::harvest` dokłada dla niego źródło. Wzorzec jest ten sam co przy `Events::reasons()` |
| FF-3 ★ | **`DecisionReason::BrandLearned` i `BrandExperience` nie mają trwałego czytelnika.** Są zwracane przez `magnat_agents::touch` i rysowane przez `reason::describe`, ale nikt ich nie zapisuje | Pełny log decyzji dla 400 tys. mieszkańców to 14 GB na rok gry (M3, decyzja 9.16), więc karta pokazuje **stan** slotu (zakładka „Marki"), a nie historię kontaktów. Trwałym czytelnikiem ma być panel marketingu i lejek z `CampaignMetrics` — czyli `FF-1` |
| FF-4 ★ | **Tytuł medialny nie ma karty.** Otwiera się jako `Subject::Site`, bo jest zakładem, ale czytelnictwo, wiarygodność i linia redakcyjna nie mają gdzie się pokazać | Faza dokładająca byt z kartą dokłada wariant `Subject` **i** ramię w `game::inspect::card` (`K-69`). Tu wariantu nie trzeba: tytuł **jest** zakładem, więc wystarczy rozgałęzienie w karcie zakładu — zakład z wpisem w `Outlets` dostaje dodatkową sekcję |
| FF-5 ★ | **Wiarygodność tytułu nie spada i to jest jedyna obietnica §5.3, której M10b nie dowiózł.** Mechanizm ma nośnik (slot marki tytułu w pamięci czytelnika), ale nie ma wyzwalacza: żeby czytelnik stracił zaufanie, trzeba porównać **tezę** tekstu z jego własną obserwacją, a `Story` niesie dziś `EventId`, nie tezę | Rozstrzygnięcie, którego M10b nie umiał podjąć: teza tekstu to albo nowe pole w `Story` (kierunek i siła oceny), albo wyprowadzenie z kategorii i skali zdarzenia. Pierwsze jest uczciwsze, drugie darmowe. **Propozycja domyślna: wyprowadzenie**, bo redakcja i tak nie ma z czego zbudować tezy innej niż „to zdarzenie jest takie a takie" |
| FF-6 | **Czytelnictwo tytułu jest regułą bez rozrzutu**: najlepiej w swojej dzielnicy, dwa razy słabiej poza nią. Numer `StreamId` na rozrzut **nie jest zarezerwowany** | Dopóki nikt czytelnictwa nie stroi, numer zapisałby na wieczność liczbę bez właściciela — ta sama reguła, którą `K-63` zastosował do `EventHazard`, a `K-67` do `CityPolicy` |
| FF-7 | **Karta mieszkańca ma siedem zakładek, czyli sufit `MAX_CARD_TABS`** (`F-18`) | Ósma wymaga decyzji, którą z obecnych złożyć. Kronika mieszkańca, gdyby miała być zakładką, wchodzi za którąś z siedmiu — a nie obok |
| FF-8 ★ | **Budżet §7.5 czeka na pomiar przy pełnej skali.** Przy trzydziestu pięciu kampaniach udział mediów w dobie `m7miasto` mieści się w szumie: ścięcie 70 % ich pracy (zawężenie ulotek do dzielnicy) zmieniło dobę z 14,83 s na 15,29 s. Kryterium „≤ 1,5 % budżetu ticku przy **2000** aktywnych kampanii" wymaga jednak przebiegu z profilem, bo dwa tysiące to pięćdziesiąt razy więcej | Adres jest naturalny: M10f i tak stawia przebieg balansatora dla całej fazy. Znane wejście: kanał ulotkowy był jedynym, który skalował się z **liczbą mieszkańców razy liczba kampanii**, i został zawężony (`F-27` w M10b). Pułapka do uniknięcia przy mierzeniu: doba `m7miasto` kosztuje 1,15 s w piątej dobie i ~15 s w czterdziestej **bez udziału mediów** — porównanie dwóch różnych dób mierzy wzrost gospodarki, nie zmianę kodu |

---

## Zmiany wpisane po M10c

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu
M10c. Gwiazdka = zmiana zakresu albo kryterium. Szczegóły — tabela `FD-n`
w `M10c-rd-i-nowe-produkty.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| FF-9 ★ | **Panel R&D ma mechanizm, ale nie ma wejścia.** `PanelId::Rnd` istnieje i jest zarezerwowany; drzewo, projekty, patenty i licencje są w `Firms::rnd()` i `RndData`. Brakuje `PanelDesc`, komendy gracza „badaj X” i karty technologii | Do M10c projekt wybiera reguła „najtańszy osiągalny węzeł”, a licencję firma kupuje sama, kiedy cudzy patent blokuje najtańszy węzeł. Gracz nie ma żadnego wpływu na własne badania — to jest ta sama luka, którą M10b zostawiło przy kampaniach reklamowych, i domyka się tą samą drogą (`DK-3`) |
| FF-10 ★ | **Technologia renderuje się w karcie inspekcji jako `#N`.** `reason::describe` dostaje katalog tekstów i `Locale`, a drzewo mieszka w `sim/firms` i `data/tech/` — ta sama granica, którą `GoodId` ma od M6 | Nazwę podmienia panel, który drzewo trzyma. Dopóki panelu nie ma, cztery powody R&D mówią graczowi numer węzła, a nie jego nazwę |
| FF-11 ★ | **Gęstość badaczy i płynność firm nie są zmierzone przebiegiem balansatora, a to od nich zależy, czy R&D w ogóle cokolwiek odkrywa.** Przebieg `m7miasto` (4 km, 23 183 mieszkańców po 300 dobach, 234 firmy) daje **3 badaczy na etatach, 7 projektów w toku, średni opłacony budżet badań 428 ‰, 0 odkryć, 0 patentów** | Dwie przyczyny naraz i obie są liczbami do strojenia, nie usterkami. **Pierwsza:** rola `researcher` jest w katalogu i 53 typy zakładów mają jej stanowisko, ale rynek pracy obsadza je ostatnie — przy 11 834 wakatach w mieście. **Druga, ważniejsza:** firmy opłacają mniej niż połowę budżetu materiałowego, bo przez 300 dób pieniądz przechodzi z ich ksiąg do gospodarstw (398 → 221 mln zł wobec 3 → 179 mln zł), a tempo badań spada proporcjonalnie do opłaconej części. Do tego miasto z 1990 zastaje jedenaście z dwunastu węzłów jako wiedzę powszechną, więc pierwszy patent wymaga przejścia całego łańcucha elektronicznego. Czy to jest właściwy rozkład, rozstrzyga pomiar, a nie przegląd |
| FF-12 | **Trzy efekty technologii z planu §5.4 nie powstały i mają tu adres**: `NewRecipe` (wymaga archetypu budynku dla montowni), `ProductFeature` razem z `FeatureId` (wymaga karty towaru) i własna montownia telefonów. Szczegóły — `FD-1`, `FD-3`, `FD-5` w `M10c-rd-i-nowe-produkty.md` | Każdy z nich potrzebuje czytelnika, którego dziś nie ma: linii produkcyjnej, karty towaru albo archetypu w `data/buildings/`. Wariant bez wyzwalacza przechodzi każdy test i wygląda tak samo jak działający (`K-67`) |
| FF-13 | **Blok `DecisionReason` M10: zajęte 800–807.** Wolne: **808–899** | M10c dołożył `ResearchStarted`, `TechDiscovered`, `LicenseSigned` i `ProductLaunched` |

---

## Zmiany wpisane po M10d

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu
M10d. Gwiazdka = zmiana zakresu albo kryterium. Szczegóły — tabela `GD-n`
w `M10d-gielda-przejecia-ubezpieczenia.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| FF-14 ★ | **Panel giełdy ma mechanizm, ale nie ma wejścia — trzeci raz ta sama luka.** `PanelId::Stock` istnieje i jest zarezerwowany; notowania, arkusz zleceń, akcjonariat z progami i kalendarz publikacji są w `Equity`. Brakuje `PanelDesc`, komendy „złóż zlecenie" i karty spółki | Ta sama luka, którą M10b zostawił przy kampaniach i M10c przy badaniach, i ta sama droga wyjścia (`DK-3`). Kanał po stronie symulacji jest otwarty: `Equity::place_order` przyjmuje `Owner::Player`, a `pay::player_citizen` znajduje jego gospodarstwo, więc komenda gracza nie wymaga ani jednej zmiany w `sim/economy` |
| FF-15 ★ | **Polisa i notowanie nie mają karty inspekcji.** `Subject` nie dostał w M10d ani jednego wariantu: spółka ma kartę firmy, polisa **nie ma żadnej**, a odszkodowanie widać wyłącznie jako powód w dzienniku firmy | Wariant `Subject` bez ramienia w `game::inspect::card` łamie kompilację (`K-69`), a ramię bez panelu pokazuje pustą stronę. Adres jest tutaj razem z panelem: `Subject::Cover(CoverId)` dokłada się wtedy, kiedy jest co na tej karcie pokazać |
| FF-16 ★ | **Kurs w karcie inspekcji jest liczbą za 0,01 % udziału i to wymaga tekstu, a nie liczby.** Powody `StockListed` i `StockFixing` pokazują obie liczby naraz (cenę bp i wycenę firmy), bo sama cena bp jest dla gracza nieczytelna | Alternatywą było liczenie kursu w „akcjach" o umownej wielkości — czyli drugi przelicznik obok `Firm.owners` i dokładnie ta druga prawda, którą `GD-1` usunął. Tekst jest tańszy od przelicznika |
| FF-17 | **Ubezpieczyciel powstaje jak tytuł medialny: przez `data/site_types/finance.ron` i archetyp `insurance_office` w `data/buildings/commerce.ron`.** Rejestr znajduje go po **kluczu tekstowym**, nie po `SiteTypeId` | Ta sama droga, którą M10b postawił gazety (`KLUCZE_TYTULOW`), i ten sam powód: `SiteTypeId` jest indeksem w katalogu i zmienia się razem z nim. Do zmierzenia w przebiegu balansatora: **ile oddziałów ubezpieczeniowych stawia generator** — waga archetypu to 14 wobec 30 dla banku, a rejestr bez ani jednego ubezpieczyciela znaczy miasto, w którym nikt nie wystawia polis |
| FF-18 ★ | **Trzy liczby M10d nie są zmierzone przebiegiem i to od nich zależy, czy giełda w ogóle istnieje w mieście z generatora**: ile firm spełnia warunek debiutu (dodatni **opublikowany** wynik i kurs każący rosnąć), ile gospodarstw przekracza próg majątku inwestora (50 000 zł) i czy w arkuszu w ogóle powstaje przecięcie | Wszystkie trzy mają ten sam kształt co `FF-11` przy badaniach: mechanizm jest, kryterium ma test jednostkowy, a to, czy zachodzi w mieście, rozstrzyga pomiar. Znane wejście: publikacja wyniku miesiąca `m` wypada w dobie `(m + 1) × 30 + 45`, więc **pierwszy debiut nie może zajść przed dobą 255** (sześć miesięcy życia firmy plus opóźnienie publikacji) — przebieg krótszy niż rok gry pokaże zero notowań i będzie miał rację |
| FF-21 ★ | **Hazard powodzi jest zgadnięty i nie został zweryfikowany przebiegiem.** `natural/flood` dostał w M10d `base_ppm: 300` z rachunku „osiem dzielnic razy 270 dób sezonu daje 0,65 powodzi na rok gry”, ale trzysta dób przebiegu `m7miasto` nie pokazało ani jednej. Pożar magazynu (`base_ppm: 10`, zakres zakładowy) też nie zaszedł ani razu | Liczba, której nikt nie zmierzył, ma tę samą szansę być błędna co każda inna zgadnięta (`R11` fazy). Adres jest naturalny: M10f i tak stawia przebieg balansatora dla całej fazy, a właściwym pomiarem jest **liczba szkód na rok gry uśredniona po ziarnach**, nie pojedynczy przebieg. Pułapka do uniknięcia: `cooldown_days` jest przerwą **definicji**, nie podmiotu (`CH-11`), więc jeden pożar w mieście ucisza wszystkie zakłady na sto osiemdziesiąt dób — przy zakresie zakładowym to zmienia rząd wielkości oczekiwanej liczby zdarzeń |
| FF-19 | **Blok `DecisionReason` M10: zajęte 800–816. Wolne: 817–899.** `StreamId` M10: zajęte 280–287 i 292–295, wolne 288–291 i 296–299 | M10d dołożył dziewięć powodów i dwa strumienie |
| FF-20 | **Kronika ma po M10d cztery nowe klasy wpisów bez własnego dziennika**: debiut, przejęcie, szkoda i wypłata odszkodowania. Wszystkie zapisują się w dzienniku decyzji firmy (`Firm.log`, pierścień 32) i tą drogą trafią do `Chronicle::harvest` (`DK-1`) | Pułapka rozbrojona w M10d, ale warto o niej wiedzieć: do dziennika idzie **wyłącznie sesja, która ruszyła kurs**. Wpis co dobę wyparłby z pierścienia wszystko inne w miesiąc, bo notowana spółka ma fixing codziennie — a dziennik decyzji ma pokazywać decyzje, nie stan rynku. Kurs, także niezmieniony, czyta się z `Equity::listing` |

---

## Zmiany wpisane po M10e

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu
M10e. Gwiazdka = zmiana zakresu albo kryterium. Szczegóły — tabela `GE-n`
w `M10e-relacje-i-zwiazki.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| FF-22 ★ | **Związek, zmowa i relacja z dostawcą nie mają karty inspekcji — czwarty raz ta sama luka.** `Subject` nie dostał w M10e ani jednego wariantu: spór widać wyłącznie jako powody w dzienniku decyzji firmy (`UnionFormed`, `WageDemandMade`, `StrikeStarted`, `StrikeEnded`), a zmowę dopiero po wykryciu | Ta sama droga wyjścia co przy polisie (`FF-15`): wariant `Subject` bez ramienia w `game::inspect::card` łamie kompilację (`K-69`), a ramię bez panelu pokazuje pustą stronę. Dane są gotowe i nie wymagają nowego przebiegu: `Unions::iter` niesie gęstość, wojowniczość, stan sporu i fundusz, `Cartels::iter` — skład, cenę minimalną i odchylenie od benchmarku, a `B2b::relations` zaufanie i obrót każdej pary. **Karta zakładu jest naturalnym miejscem dla związku** (tak jak tytuł medialny nie potrzebuje własnego wariantu, `FF-4`), a zmowa jest sekcją karty firmy |
| FF-23 ★ | **Gracz nie ma jak negocjować — piąty raz ta sama luka.** Firma gracza dostaje żądanie i odpowiada na nie **regułą**, tą samą co firma AI: ustępstwo jest funkcją marży i numeru rundy. Komendy „przyjmij żądanie", „kontruj", „przeczekaj" nie ma | Kanał po stronie symulacji jest otwarty i nie wymaga zmian: ustępstwo przechodzi przez `talks::rundy`, a tam wystarczy, żeby gracz mógł podstawić własną liczbę zamiast wyliczonej — dokładnie tak, jak `PricePolicy` z M5c podstawia politykę gracza w miejsce polityki AI. Panel pracowniczy z §6 pkt 5 dokumentu fazy (`grievance` per zakład jako ostrzeżenie wyprzedzające, przebieg negocjacji, licznik funduszu) jest tu jedynym brakującym elementem |
| FF-24 ★ | **Trzy liczby M10e nie są zmierzone przebiegiem balansatora**: ile zakładów w mieście z generatora przekracza próg żalu, ile zmów zawiązuje się na rok i jaki odsetek firm bywa rocznie w strajku (ryzyko `R6` fazy stawia próg alarmowy **5 %**) | Ten sam kształt co `FF-11` przy badaniach i `FF-18` przy giełdzie: mechanizm jest, kryteria mają testy, a to, czy zachodzi w mieście z generatora, rozstrzyga pomiar. Znane wejścia: żal mierzy się **miesięcznie**, a warunek „≥ 60 dób" znaczy dwa pomiary z rzędu, więc przed dobą 60 nie powstanie ani jeden związek; zmowa wymaga **trzech** sprzedawców tego samego towaru w jednej dzielnicy, więc małe miasto może nie mieć ani jednej pary spełniającej warunek. Ryzyko `R8` fazy (kartel zawsze albo nigdy opłacalny) ma w tym samym przebiegu swój cel: **20–50 % zmów wykrytych w ciągu pięciu lat gry** |
| FF-25 ★ | **Uderzenie prasy w markę ma stałą w kodzie, nie w danych.** `SCANDAL_MAX_DROP` w `sim/media` mówi, o ile najwyżej spada sympatia po najgorszym możliwym tekście; zmowa cenowa zabiera dwadzieścia punktów i ma swoją liczbę w `data/tuning/relations.ron` | `ponytail:` z nazwanym sufitem i drogą wyjścia (`GE-10`). Liczba w danych bez przebiegu, który ją stroi, byłaby parametrem bez pytania, na które odpowiada — a przebieg mierzący, ile marek rocznie obrywa od prasy, i tak należy do M10f razem z `FF-24` |
| FF-26 | **Przymusowy podział nadal wykonuje się zamknięciem zakładu** (`sim/city::law`, `ponytail:` z M8d) — mimo że rynek kontroli nad firmą istnieje od M10d | Komentarz w kodzie mówi „należy do M10", a M10d zbudował przejęcia i emisje. Zmiana jest teraz wykonalna: `ForcedDivestiture` mógłby wystawić pakiet kontrolny na fixing zamiast zamykać sklep. M10e tego **nie robi**, bo dotyczy innego urzędu i innego kryterium — ale adres przestał być pusty i to jest cała treść tego wpisu |
| FF-27 | **Blok `DecisionReason` M10: zajęte 800–824. Wolne: 825–899.** `StreamId` M10: zajęte 280–287 i 289–295, wolne **288** (zarezerwowany `PerilDraw`, `K-85`) i 296–299 | M10e dołożył osiem powodów (`TrustedSupplier`, `CartelFormed`, `CartelDetected`, `BrandScandal`, `UnionFormed`, `WageDemandMade`, `StrikeStarted`, `StrikeEnded`) i trzy strumienie (`CartelDetection`, `UnionFormation`, `StrikeResolve`) |
| FF-28 | **Strajk jest pierwszym zdarzeniem w katalogu, którego nie losuje hazard** (`K-89`), i lista takich definicji jest zamknięta: `magnat_events::CALLED_EVENTS` | Test katalogu pyta o tę listę, więc `base_ppm: 0` wpisane przez pomyłkę nadal łamie build. Faza dopisująca zdarzenie wywoływane przez świat dopisuje je **i tam** — inaczej wygląda w katalogu jak definicja martwa (`R2`) |
| FF-29 ★ | **Lista płac nadal nie dochodzi do gospodarstw, a strajk pokazał, ile to kosztuje.** `Firms::run_payroll` zwraca fakty, `PayrollOutbox` je przyjmuje i **nikt jej nie opróżnia** od M7b; dochód gospodarstwa jest do dziś egzogeniczny (decyzja nr 2 fazy M5: płaci go abstrakcyjny pracodawca spoza miasta). Skutek dla M10e jest konkretny: strajk zabiera realny pieniądz **firmie** — jej rachunek wyniku nie księguje płac za dni postoju — a po stronie załogi zostaje **liczbą**, bo salda gospodarstw nie drgną, choć ludzie nie dostali wypłaty | To nie jest usterka M10e i M10e jej nie naprawia: konsument `PayrollOutbox` zmienia źródło dochodu całego miasta, więc przestawia kalibrację kopert, kredytu, CPI i bramek G1–G3 naraz. Adres jest tu, bo to M10e jest pierwszą mechaniką, dla której **różnica jest widoczna**: `Union::strike_fund` jest dziś jawnym przybliżeniem wytrzymałości i dopiero po domknięciu tego kanału stanie się odczytem prawdziwych sald. Pułapka do uniknięcia przy naprawie: `pay_incomes` wypłaca **netto po potrąceniu u źródła** i robi to raz w miesiącu dla wszystkich; lista płac ma dzień wypłaty **per firma** (`payday(FirmKey)`), więc podmiana źródła rozkłada dochód miasta na trzydzieści dób i to ona, a nie kwota, jest tu prawdziwą zmianą |
