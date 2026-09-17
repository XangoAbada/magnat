# M8 — Miasto jako aktor

Status: plan fazy. Podlega `00-konwencje-i-kontrakty.md` (typy bazowe, determinizm, LOD, dane, testy).
Crate'y tworzone: `sim/city`, `sim/events`. Crate rozszerzany: `sim/traffic` (sieci przesyłowe).

---

## 1. Cel fazy i artefakt końcowy

Po M8 miasto przestaje być scenografią i staje się **drugim graczem**: ma własny budżet,
własne cele, własne decyzje i własne narzędzia nacisku. Świat przestaje być gładki:
pada deszcz albo go nie ma, elektrownia się psuje, ludzie strajkują, a urząd ma kolejkę.

Artefakt uruchamialny (headless + nakładki debug):

1. **Miasto pobiera podatki i wydaje pieniądze.** Każda transakcja detaliczna nalicza VAT,
   każda wypłata nalicza PIT, każdy rok obrotowy firmy nalicza CIT, każda parcela płaci
   podatek od nieruchomości. Gracz widzi każde obciążenie w swojej księdze z nazwą, stawką
   i okresem. Budżet miasta domyka się do grosza.
2. **Miasto rządzi.** Burmistrz AI z preferencjami (prorozwojowy / socjalny / ekologiczny /
   populistyczny) co miesiąc podejmuje decyzje: zmienia stawki, ogłasza przetarg, wydaje
   pozwolenia, wprowadza regulację, przyznaje dotację — każda decyzja z `DecisionReason`.
3. **Wybory działają.** Co kadencję mieszkańcy głosują (frekwencja i preferencje zależne od
   statusu, nastroju, dzielnicy i jakości usług w ich okolicy). Wynik zmienia politykę.
   Gracz może wspierać kandydata — legalnie i nielegalnie, z ryzykiem.
4. **Usługi publiczne mają skutki.** Szkoła podnosi umiejętności dzieci w obwodzie, szpital
   skraca absencję, policja obniża straty w sklepach (widoczne w rachunku wyników gracza),
   urząd wydaje pozwolenie po **realnym czasie** wynikającym z obsady i kolejki.
5. **Sieci przesyłowe żyją.** Elektrownia → linie → transformatory → odbiorcy. Awaria źródła
   albo przeciążenie linii prowadzi do **blackoutu**: zakład bez prądu stoi, a przestój widać
   w kosztach i w niedostarczonych kontraktach. Woda, gaz, ciepło, telekomunikacja — ten sam
   solver, inne jednostki, każda sieć jako firma z taryfą.
6. **Zdarzenia wynikają ze stanu świata.** Susza nie jest wpisem w kalendarzu — jest skutkiem
   deficytu opadów. Awaria elektrowni jest skutkiem wieku bloku, zaległej konserwacji i
   miesięcy pracy na 98% mocy. Strajk jest skutkiem rozjazdu płac i zysku. Zdarzenie zmienia
   **parametry**; ceny zmieniają się same, w M5/M6/M7.
7. **Pogoda i pory roku pracują cały czas** — na popyt na ciepło, na plony, na prędkość ruchu,
   na obciążenie sieci.

**Demo fazy (`tools/headless --scenario m8-demo`):** rok gry na mieście 20 tys. mieszkańców.
Log pokazuje: mroźny tydzień → skok poboru ciepła i prądu → przeciążenie linii do dzielnicy
przemysłowej → wyłączenie → dwie fabryki stoją 4 godziny → kara umowna u odbiorcy → wzrost
taryfy w kolejnym okresie rozliczeniowym → spadek poparcia dla burmistrza w tej dzielnicy →
przegrana w wyborach w tym obwodzie. Ani jeden z tych kroków nie jest zaprogramowany jako skutek.

---

## 2. Zakres — wchodzi / nie wchodzi

### Wchodzi

| Obszar | Zawartość | PRD |
|---|---|---|
| Władza miejska AI | burmistrz i rada, cele, preferencje, pętla decyzyjna, regulacje, dotacje, inwestycje | §10.1 |
| Budżet i przepływy publiczne | `CityBudget`, księga publiczna, dług miejski, inwestycje kapitałowe | §6.8, §10.1 |
| Podatki | CIT, PIT, VAT (detal), od nieruchomości, akcyza, cła, koncesje — z pełnym śladem w księgowości | §6.8, §6.9 |
| Pozwolenia i przetargi | `Permit` z kolejką urzędu, `Tender` z kryteriami i ofertami | §10.1, §10.3 |
| Wybory | frekwencja i preferencje per mieszkaniec, kandydaci, wsparcie gracza (legalne i nie) | §10.2 |
| Usługi publiczne | szkoły, szpitale, policja, straż, odpady, parki, urzędy — z mierzalnym skutkiem | §10.3 |
| Prawo i egzekucja | antymonopol, inspekcja pracy, sanepid, ochrona środowiska, skarbówka, szara strefa | §10.4, §7.3 |
| Sieci przesyłowe | prąd (przepływ, przeciążenie, blackout, kaskada), gaz, woda/kanalizacja, ciepło, telekom; taryfy | §9.6, §7.3 |
| Generator zdarzeń | hazard zależny od stanu świata, `ParamPatch`, katalog 6 kategorii jako dane | §11.1, §11.2 |
| Systemy dynamiczne stałe | pory roku i pogoda, zegar epoki, wskaźniki cyklu koniunkturalnego, parametry demografii | §11.3 |

### Nie wchodzi

| Czego nie robię | Kto robi | Co zamiast tego |
|---|---|---|
| Reakcje firm na podatki, ceny, regulacje, blackout | **M7** | emituję parametry i obciążenia, firma decyduje sama |
| Księga główna firmy (konta, RZiS, bilans) | **M5** (`sim/economy`) / **M7** | emituję `TaxCharge` do bufora; księgowanie robi właściciel księgi |
| Media jako firmy, kształtowanie opinii | **M10** | `MediaExposure` to pole wejściowe wyborcy; do M10 zasilane stubem (zasięg = f(dzielnica, nakład publiczny)) |
| Giełda, przejęcia, R&D, marka | **M10** | `EpochClock` publikuje dostępność technologii, nic więcej |
| Pełne panele UI miasta | **M9** | tylko inspektory `devtools` (tabela budżetu, graf sieci, log hazardów) |
| Symulacja ruchu drogowego | **M4** | konsumuję czasy przejazdu i krawędzie; modyfikuję ich prędkość parametrem |
| Model makro / historia „na sucho" | **M10** | publikuję `MacroIndicators` jako agregat obserwowalny, nie modeluję cyklu |
| Bankructwo firmy z tytułu zaległości | **M7** | wystawiam `Remedy::BackTax` i wniosek egzekucyjny; skutek majątkowy — M7 |
| Generacja terenu, klimat bazowy | **M1** | pogoda M8 to odchyłka od norm klimatycznych M1 |

---

## 3. Mapowanie na PRD

| Sekcja PRD | Pokrycie w M8 |
|---|---|
| §6.8 Podatki i przepływy publiczne | WP2 w całości (7 danin), WP1 (budżet) |
| §6.9 Księgowość — ślad obciążeń | WP2, kontrakt `TaxCharge` → księga firmy |
| §6.7 Podatek od nieruchomości | WP2 (`property_tax_month`), podstawa z wyceny M5 |
| §7.3 Media zakładu, emisje | WP3 (przyłącza, zużycie, faktury), WP8 (kary za emisje) |
| §9.6 Sieci przesyłowe | WP3 w całości |
| §10.1 Władza miejska | WP9 (pętla decyzyjna), WP7 (pozwolenia), WP9 (przetargi, dotacje, regulacje) |
| §10.2 Wybory | WP10 |
| §10.3 Usługi publiczne | WP7 |
| §10.4 Prawo i egzekucja | WP8 |
| §11.1 Zasady zdarzeń | WP4 (hazard ze stanu, `ParamPatch`, zakaz zadawania cen) |
| §11.2 Katalog kategorii | WP5 (`data/events/`, 6 kategorii) |
| §11.3 Systemy dynamiczne | WP6 (pogoda, sezon, epoka, demografia, wskaźniki cyklu) |
| §14.3 Panel „Miasto", Kronika | WP11 (model danych + inspektor; pełny panel — M9) |
| §17.4 LOD | WP2, WP3 — mezo-odpowiedniki podatków i mediów, test salda 0 |
| §18.2 Determinizm | WP4 (arytmetyka całkowita hazardu), WP11 (hash, replay) |
| §19 M8 | zakres kamienia milowego w całości |

---

## 4. Pakiety robocze i podfazy

Kolejność wynika z zależności: pieniądz publiczny → media → zdarzenia → reszta.

Faza jest rozbita na **5 podfaz**. Podfaza to porcja, którą da się zacząć i zamknąć
bez trzymania w głowie całej fazy: własny zestaw WP, własny sprawdzalny wynik i własny
wycinek projektu technicznego. Opis pakietów i sekcje §5 mieszkają teraz w dokumentach
podfaz — poniższa tabela mówi, gdzie co jest. Bramki 1–7 z `00-postep.md` zamykają się
dopiero po ostatniej podfazie; podfaza zamyka się własnym kryterium ze swojego dokumentu.

| Podfaza | WP | §5 | Wynik do pokazania | Dokument |
|---|---|---|---|---|
| **M8a — Pieniądz publiczny** | WP1, WP2 | 5.0, 5.1 | Test T1 (domknięcie podatkowe) zielony na scenariuszu rocznym; każde obciążenie widoczne w karcie inspekcji firmy z nazwą, stawką i podstawą. | `M8a-pieniadz-publiczny.md` |
| **M8b — Sieci przesyłowe** | WP3 | 5.4 | Test T2: blackout zatrzymuje produkcję i jest widoczny w kosztach. | `M8b-sieci-przesylowe.md` |
| **M8c — Zdarzenia** | WP4, WP5, WP6 | 5.5, 5.6 | Susza, awaria elektrowni i strajk odpalają się z danych, deterministycznie, z czytelnym logiem „dlaczego jeszcze nie”. | `M8c-zdarzenia.md` |
| **M8d — Usługi publiczne i prawo** | WP7, WP8 | 5.3 | Test T4 oraz scenariusz „firma ukrywa 30 % obrotu” kończący się kontrolą w medianie < 3 lat gry. | `M8d-uslugi-i-prawo.md` |
| **M8e — Władza i wybory** | WP9, WP10, WP11 | 5.2, 5.7, 5.8 | Pełny artefakt fazy z §1 dokumentu fazy: podatki, wybory, sieci przesyłowe, zdarzenia i pogoda. | `M8e-wladza-i-wybory.md` |

---

## 5. Projekt techniczny

Treść przeniesiona do dokumentów podfaz. **Numeracja `5.x` jest zachowana**, więc
odesłania w tekście („patrz §5.4") nadal wskazują tę samą sekcję — zmienił się tylko plik.

| § | Temat | Dokument |
|---|---|---|
| 5.0 | Konwencje fazy | `M8a-pieniadz-publiczny.md` |
| 5.1 | Budżet miasta i podatki | `M8a-pieniadz-publiczny.md` |
| 5.2 | Polityka, pozwolenia, przetargi, władza | `M8e-wladza-i-wybory.md` |
| 5.3 | Usługi publiczne i egzekucja | `M8d-uslugi-i-prawo.md` |
| 5.4 | Sieci przesyłowe (`sim/traffic`) | `M8b-sieci-przesylowe.md` |
| 5.5 | Generator zdarzeń (`sim/events`) | `M8c-zdarzenia.md` |
| 5.6 | Systemy dynamiczne stałe | `M8c-zdarzenia.md` |
| 5.7 | Wybory | `M8e-wladza-i-wybory.md` |
| 5.8 | Systemy ECS i ich częstotliwość | `M8e-wladza-i-wybory.md` |

---

## 6. Kontrakty międzyfazowe

### Dostarczam

| Typ / funkcja | Crate | Konsument |
|---|---|---|
| `CityTaxEngine: TaxEngine` — jedyna prawdziwa implementacja haka M5 | `city` | **M5** (zastępuje `NoTax`) |
| `TaxCharge`, `TaxChargeId`, `ChargeRegistry`, `ChargeState`, `TaxReason` | `city` | M5 (księga), M7 (decyzje), M9 (UI) |
| `TaxKind`, `TaxCode`, `FiscalPeriod` | `city` | M5, M6 (cło), M7 |
| `vat_from_gross`, `vat_add_to_net`, `cit_due`, `pit_withheld`, `property_tax_month`, `excise_due`, `duty_due`, `license_fee` | `city` | M5, M6, M7 |
| `CityBudget`, `SpendCategory`, `MunicipalBond` | `city` | M9 (panel), M10 (makro) |
| `Policy`, `PolicyRecord`, `PolicySet`, `fn policy_in_force(&PolicySet, Tick) -> &[Policy]` | `city` | M2 (strefowanie), M7 (regulacje), M9 |
| `EdgeRestriction`, `VehicleClass`, `fn compile_edge_mask(..) -> EdgeMask` | `city` | **M4 — kontrakt o ustalonym kształcie, patrz niżej** |
| `Permit`, `PermitKind`, `PermitStatus`, `fn file_permit(..) -> PermitId` | `city` | M7 (budowa zakładu), M9 (gracz) |
| `Tender`, `Bid`, `BidCriteria`, `fn submit_bid(..)` | `city` | M7 (firmy AI), M9 (gracz) |
| `Election`, `Candidate`, `ElectionResult`, `fn back_candidate(..)` | `city` | M9, M10 (media) |
| `PublicService`, `ServiceKind`, `ServiceCoverage` | `city` | M3 (edukacja, zdrowie), M5 (straty, wartość gruntu), M7 (absencja) |
| `Agency`, `Case`, `Remedy`, `UnreportedShareBps` | `city` | M7 (ryzyko), M9 (decyzja gracza) |
| `UtilityGrids`, `UtilityNetwork`, `UtilityNode`, `UtilityEdge`, `Tariff`, `SupplyState`, `GridTuning`, `UtilitySystem` (moduł `traffic::utility`) | `traffic` | M7 (operatorzy, zakłady), M9 |
| `UtilityGrids::power_available(SiteId) -> bool`, `UtilityGrids::supply_state(SiteId, UtilityService) -> SupplyState` | `traffic` | **M7 — kontrakt krytyczny.** Metody zasobu świata, nie funkcje wolne (`CC-7`): `world.resource::<UtilityGrids>().power_available(site)`. Rodzajem mediów jest `UtilityService` z `core::vocab` (`CB-1`), a nie `UtilityKind` — ta nazwa jest zajęta przez wymiar oceny w funkcji użyteczności zakupu |
| `WorldEvent`, `EventDef`, `EventCategory`, `EventId`, `EventCause` | `events` | wszystkie `sim/*`, M9 (Kronika) |
| `SimParam`, `ParamPatch`, `ParamOverlay`, `fn effective(..)` | `events` | M1, M3, M4, M5, M6, M7 |
| `Probe`, `ProbeCtx`, `EventTrigger`, `hazard_ppm` | `events` | M8 (WP8 — kontrole), M10 (zdarzenia rynkowe) |
| `Weather`, `SeasonCalendar`, `EpochClock` | `events` | M1/M11 (wizualizacja), M3 (potrzeby), M4 (prędkość), M6 (rolnictwo) |
| `MacroIndicators`, `DemographyParams` | `events` | M3 (demografia), M10 (makro) |
| `data/events/*.ron` — schemat i walidator | `events` | M12 (modding) |

#### Kontrakt szczególny z M4 — regulacje ruchowe

**Liczba profili tras (CH) jest zasobem limitowanym. M8 go nie powiększa.**
M4 utrzymuje ustalony, mały zestaw profili (`Passenger`, `HeavyDay`, `HeavyNight`) na jednej
kolejności kontrakcji. Każda regulacja ruchowa M8 wyraża się **wyłącznie** jako predykat na
krawędziach (`EdgeRestriction`: zbiór krawędzi + klasa pojazdu + okno `WeekMask`), który
M4 nakłada jako **maskę wyłączonych krawędzi na istniejący profil**. Zasady wiążące:

1. Żadna uchwała rady, żadna decyzja burmistrza i żadne działanie gracza **nie tworzy nowego
   profilu trasy.** Jeśli regulacja nie daje się wyrazić maską, nie wchodzi do `Policy`.
2. Regulacje składają się **sumą bitową w jedną maskę** per (klasa pojazdu, godzina).
   Dwadzieścia uchwał kosztuje tyle samo co jedna — koszt jest stały w liczbie regulacji.
3. Dla rzadkiego, nietypowego ograniczenia, którego maska nie obsłuży, M4 udostępnia
   **fallback: A\* z predykatem** — wolniejszy, ale bez kosztu stałego. M8 używa go świadomie
   i oznacza taką regulację jako kosztowną (widoczne w inspektorze).
4. `compile_edge_mask` przelicza się wyłącznie przy zmianie zbioru obowiązujących regulacji
   (uchwała, wejście w życie, wygaśnięcie), nie co tick.

Konsekwencja projektowa dla M8: regulacja jest zawsze **egzekwowana mandatem, nie barierą**.
Ciężarówka fizycznie może wjechać w zakazaną ulicę; router jej tam nie poprowadzi, a jeśli
kierowca (albo gracz) wybierze tę trasę mimo wszystko — jest mandat i sprawa dla inspekcji.
To utrzymuje regulację po stronie kosztu ekonomicznego, gdzie ma być, a nie po stronie fizyki.

### Konsumuję

| Czego potrzebuję | Skąd | Uwagi |
|---|---|---|
| `Money`, `Tick`, `SimMinute`, `Qty`, `Q`, `Mood`, `StreamId`, `rng()` | M0 `core` | dopisuję warianty `StreamId` |
| bufory komend, scheduler, hash stanu | M0 `ecs` | dopisuję komponenty M8 do hasza |
| normy klimatyczne per dzień roku i dzielnicę | M1 `world` | kotwica dla pogody |
| parcele, budynki, dzielnice, strefy | M2 `world` | podstawa podatku od nieruchomości, obwody |
| `CitizenId`, status, nastrój, dzielnica zamieszkania, graf relacji | M3 `agents` | wyborcy, obsada usług |
| czasy przejazdu, krawędzie drogowe | M4 `nav`/`traffic` | zasięg usług, `TravelSpeedMulBps` |
| `Books`, `Books::transfer`, `AccountId`, `AccountOwner`, `MoneySupplyLedger` | M5 `economy` | jedyna droga ruchu pieniądza; miasto jako `AccountOwner::City` |
| `trait TaxEngine` (+ `NoTax` jako baza), `Transaction.tax`, `TxKind`, `LedgerAccount::TaxExpense` | M5 `economy` | **haki zostawione dla M8** — M8 dostarcza `CityTaxEngine` |
| `Ledger`, `JournalEntry`, `post`, `income_statement` | M5 `economy` | podstawa CIT; ślad obciążeń w księdze gracza |
| transakcja detaliczna (`TxKind::RetailSale`), wycena parceli | M5 `economy` | punkt naliczenia VAT, podstawa podatku od nieruchomości |
| `SimCalendar` (K-1: 360 dni, 12 × 30), `DayOfWeek` (K-15) | M0 `core` | okresy podatkowe i kadencje na siatce miesięcznej; godziny handlu i grafiki urzędów na tygodniowej |
| `UtilityKind`, `DecisionReason`, `NeedKind`, `PlaceRef`, `TransportMode`, `ActivityKind` | M0 `core` (K-8, K-12) | M8 tylko używa, nie definiuje — `UtilityKind` dzielony z `TxKind::Utility` (M5) i M4 |
| węzeł importowy, wartość celna, partie towaru, receptury | M6 `supply` | cło, akcyza, naprawa (części) |
| `FirmId`, RZiS, payroll, decyzje firmy, rynek pracy | M7 `firms` | CIT, PIT, reakcje na blackout i podatki |
| `MediaExposure` wyborcy | M10 | **do M10 stub**: f(nakład publiczny kandydata, dzielnica) |

---

## 7. Testy i kryteria akceptacji

**T1 — Domknięcie podatkowe (tolerancja 0 groszy).** Scenariusz roczny, 5 tys. mieszkańców,
300 firm, wszystkie siedem danin aktywnych. Dla każdej pary `(TaxKind, FiscalPeriod)`:
```
Σ amount(Assessed) == Σ amount(Settled) + Σ amount(Overdue) + Σ amount(Abated)
Σ amount(Settled) == Δ CityBudget.revenue_ytd[kind]
```
oraz dla każdej firmy: suma obciążeń w jej księdze == suma `TaxCharge` wystawionych na nią.
Plus test zachowania pieniądza z dokumentu 00: `Σ firmy + Σ mieszkańcy + Σ banki + miasto = const`.
**Tolerancja: 0.** Test własnościowy (proptest) na 10⁶ losowych transakcji sprawdza dodatkowo,
że suma VAT-u naliczonego per transakcja równa się dokładnie VAT-owi w deklaracji miesięcznej
(nie ma dryfu zaokrągleń) i że `Σ property_tax_month(v, bps, 1..=12) == annual(v, bps)`.
Strażnik siatek czasu (K-15): test strukturalny sprawdza, że `FiscalPeriod` nie ma wariantu
tygodniowego i że żadna daninowa ścieżka nie czyta `DayOfWeek`. Przebieg 3-letni domyka się
do roku niezależnie od tego, w jaki dzień tygodnia wypadł 1 stycznia (dryf tygodnia nie
przesuwa ani jednego grosza).

**T2 — Blackout zatrzymuje produkcję i widać to w kosztach.** Scenariusz: jedna elektrownia,
jedna fabryka, 6 godzin gry. W ticku T wymuszona awaria źródła.
Asercje: (a) `power_available(factory) == false` w ticku T (Power liczony `EveryMinute`);
(b) wolumen produkcji w oknie awarii **dokładnie 0**; (c) w RZiS firmy za ten dzień koszty stałe
i płace **nie zerowe**, koszt energii zmiennej zerowy, wynik gorszy niż w przebiegu kontrolnym;
(d) karta inspekcji zakładu pokazuje `SiteHalted{ reason: NoUtility(Power) }`;
(e) po naprawie produkcja wraca do poziomu sprzed awarii w ≤ 2 ticki.
Wariant kaskadowy: sieć z 3 wyspami, wymuszone przeciążenie linii — asercja, że kaskada
kończy się w ≤ 8 rundach i że każda wyspa po ustabilizowaniu ma `supply ≥ demand`.

**T3 — Pełny determinizm zdarzeń.** Dwa przebiegi tego samego seeda, 5 lat gry:
identyczny, uporządkowany ciąg krotek `(tick, event_key, scope_instance, severity_bps, duration)`
oraz identyczny hash stanu ECS co 1000 ticków (z komponentami M8 w funkcji haszującej).
Trzeci przebieg: te same 5 lat z zapisem i wczytaniem stanu w losowym momencie — ciąg zdarzeń
po wznowieniu identyczny. Test statyczny: w module hazardu i losowania zdarzeń **nie występuje
żaden typ zmiennoprzecinkowy** (lint + test kompilacyjny na sygnaturach).

**T4 — Usługi mają mierzalny skutek.** Trzy pary przebiegów A/B, 10 lat gry:
(a) szkoła z pełnym finansowaniem vs. połowa — mediana umiejętności 18-latków w obwodzie
różni się o ≥ 8 punktów `Q`; (b) posterunek vs. brak — straty inwentaryzacyjne sklepów
w dzielnicy różnią się o ≥ 25% i widać to jako pozycję w RZiS; (c) szpital vs. brak —
absencja chorobowa w dzielnicy różni się o ≥ 15%.

**T5 — Władza nie oscyluje.** 20 lat gry bez zdarzeń zewnętrznych: żadna stawka podatkowa
nie zmienia kierunku częściej niż raz na 24 miesiące; budżet nie wchodzi w spiralę zadłużenia
(dług/przychody roczne < 200% w każdym punkcie). Każda decyzja ma niepusty `DecisionReason`.

**T6 — Wybory deterministyczne i wrażliwe.** Ten sam seed → identyczny wynik co do głosu.
Test wrażliwości: obniżenie `ServiceQuality` w jednej dzielnicy o 30% przez kadencję obniża
tam poparcie inkumbenta o ≥ 5 pkt proc. wobec przebiegu kontrolnego. Frekwencja mieści się
w widełkach 35–75% i jest wyższa wśród wyższego statusu i starszych roczników.

**T7 — Zdarzenia nie zadają cen.** Test strukturalny: pełne wyliczenie wariantów `SimParam`
przechodzi przez listę dozwoloną; żaden wariant nie odnosi się do `Offer`, `price`, `margin`
ani `demand_qty`. Test dynamiczny: przebieg 3 lat ze wszystkimi zdarzeniami — snapshot
wszystkich `Offer.price` przed i po zastosowaniu każdego patcha w tym samym ticku jest
identyczny (ceny zmieniają się dopiero w ticku decyzji cenowej firmy, nie w ticku zdarzenia).

**T8 — LOD i wydajność.**
(a) Ten sam scenariusz w mikro i w mezo → identyczne sumy podatków i identyczne salda
gotówkowe (tolerancja 0, wymóg z dokumentu 00 §4).
(b) `criterion`: `solve_network` dla wszystkich pięciu sieci metropolii 400 tys.
**< 0,3 ms/tick** (typowo, 1 runda) i **< 2 ms** w kaskadzie 8-rundowej.
(c) `sys_eval_hazards_slow` dla pełnego katalogu zdarzeń i 400 tys. mieszkańców
**< 3 ms/dobę gry**, z cache sond.
(d) `sys_settle_tax_charges` **< 1 ms/dobę gry** przy 20 tys. firm.

**Definition of Done fazy:** T1–T8 zielone, `clippy -D warnings`, komponenty M8 w haszu stanu,
walidator `data/events/` w CI, scenariusz `m8-demo` odtwarzalny z seeda, każda decyzja
władzy i każde zdarzenie z powodem widocznym w inspektorze (§7 dokumentu 00).

---

## 8. Ryzyka fazy i mitygacje

| # | Ryzyko | Skutek | Mitygacja |
|---|---|---|---|
| R1 | **Burza zdarzeń** — hazardy mnożą się i świat staje się festiwalem katastrof | gra nieprzewidywalna, balans niemożliwy | `max_concurrent` i `cooldown_days` per definicja; globalny limit „budżetu chaosu" (suma severity aktywnych zdarzeń per kategoria); raport balansatora: liczba zdarzeń na rok per kategoria z widełkami w CI |
| R2 | **Martwe hazardy** — zdarzenie nigdy nie zachodzi, bo krzywa jest źle dobrana | katalog danych jest fikcją | inspektor hazardów pokazuje dla każdej definicji aktualny hazard i wartości sond; test CI: w 20-letnim przebiegu każda definicja z katalogu musi zajść ≥ 1 raz |
| R3 | **Model sieci zbyt prosty** — przeciążenia nie występują albo występują zawsze | blackout jest nudny albo gra nie działa | ścieżka wyjścia opisana w 5.4 (rozpływ DC za tym samym interfejsem); scenariusz testowy z wymuszonym przeciążeniem od WP3 |
| R4 | **Sprzężenie z M7** — firmy nie reagują na blackout ani na podatki | M8 wygląda jak dekoracja | kontrakt `power_available` i `TaxCharge` uzgodniony **przed** WP2/WP3; test integracyjny T2 wymaga rzeczywistej reakcji M7 — jeśli M7 nie gotowe, stub firmy w testach fazy |
| R5 | **Dryf zaokrągleń w podatkach** | test T1 pęka po miesiącach, trudny do zdiagnozowania | wszystkie funkcje podatkowe czyste i pokryte proptestami od pierwszego dnia; PIT liczony narastająco, podatek od nieruchomości metodą różnicy skumulowanej |
| R6 | **Koszt oceny hazardów** — sondy liczone per obiekt per definicja | O(defs × obiektów) co dobę | `gate` odsiewa przed policzeniem sond; cache sond per (Probe, tick); zakresy `Site`/`Firm` oceniane tylko dla kandydatów; budżet w T8c |
| R7 | **Oscylacja AI władzy** — stawki skaczą co miesiąc | miasto wygląda na szalone, gracz nie może planować | histereza + minimalny okres między zmianami + vacatio legis; test T5 |
| R8 | **Podatki przytłaczają gracza** | siedem danin w księdze = ściana liczb | jedna linia zbiorcza „podatki" z rozwinięciem; naliczenia grupowane per okres, nie per transakcja; UI — M9, ale model danych już to umożliwia (`FiscalPeriod` jako klucz agregacji) |
| R9 | **Determinizm złamany przez sondę** — sonda liczona z `HashMap` albo z floata | cichy rozjazd przebiegów | sondy wyłącznie `i64`, iteracja po `Vec`/`BTreeMap`; test T3 z zapisem i wczytaniem w środku |
| R10 | **Szara strefa zbyt opłacalna albo bezużyteczna** | albo wszyscy oszukują, albo nikt | hazard kontroli rośnie nadliniowo z `UnreportedShareBps`; kara = domiar + odsetki + mnożnik; balansator raportuje odsetek szarej strefy w populacji firm AI (cel: 5–20%) |
| R11 | **Kaskada sieci nie zbiega** | pętla w ticku | twardy limit 8 rund + fallback: zrzuć wszystko niezbilansowane; test kaskadowy w T2 |
| R12 | **Inflacja regulacji ruchowych** — aktywne politycznie miasto mnoży uchwały, a każda dokłada koszt routingu | routing przestaje się mieścić w budżecie M4 dokładnie wtedy, gdy gra robi się ciekawa | `EdgeRestriction` jako predykat składany sumą bitową w jedną maskę (kontrakt z M4 w §6) — koszt stały w liczbie uchwał; zakaz tworzenia nowych profili CH wpisany w kontrakt; test: 50 aktywnych regulacji daje ten sam czas routingu co 1 (±5%) |

---

## 9. Decyzje otwarte

Kanał uzgodnień międzyagentowych był w tej sesji niedostępny (`ListAgents` nieobecny), ale
dokumenty M0/M1/M4/M5 i rozstrzygnięcia `K-*` z dokumentu 00 pojawiły się w trakcie pisania
i zostały **uwzględnione**. Poniżej najpierw to, co już rozstrzygnięte, potem to, co otwarte.

### Rozstrzygnięte na podstawie dokumentów, które powstały równolegle

| # | Sprawa | Rozstrzygnięcie przyjęte przez M8 |
|---|---|---|
| Z1 | Kalendarz | **K-1**: 360 dni, 12 × 30. Okresy podatkowe, kadencje i sezony liczą w tej jednostce; `annual/12` bez reszty |
| Z2 | Zakres `StreamId` | **K-4**: blok 240–259, przydział wyliczony w 5.0 |
| Z3 | Funkcje przestępne | **K-6**: M8 spełnia z nadmiarem — hazard i podatki w całości całkowitoliczbowe, `det_math` niepotrzebny w tej ścieżce |
| Z4 | Właściciel księgi i planu kont | **M5**: `Ledger`, `LedgerAccount`, `JournalEntry`, `post`. M8 nie projektuje księgowości, tylko emituje `TaxCharge` i wypełnia hak `TaxEngine` |
| Z5 | Hak podatkowy (pytanie otwarte M5 nr 8) | **odpowiedź w 5.1**: `Transaction.tax` zostaje dla VAT (jedna stawka na transakcję), reszta danin w `ChargeRegistry` M8. M5 nie migruje dziennika |
| Z6 | Ruch pieniądza | **M5**: wyłącznie `Books::transfer`; miasto jako `AccountOwner::City`, objęte niezmiennikiem `MoneySupplyLedger` |
| Z7 | Gdzie mieszka `UtilityKind` (było D2b) | **K-8**: `engine/core`, zgodnie z propozycją M8. Razem z `TransportMode`, `PlaceRef`, `NeedKind`, `ActivityKind` i `DecisionReason` (K-12) |
| Z8 | Tydzień w kalendarzu 12×30 | **K-15**: tydzień 7-dniowy istnieje i dryfuje; `DayOfWeek` z `SimCalendar`. M8 rozdziela siatki: regulacje handlowe i grafiki usług na tygodniowej (`WeekMask`), podatki, budżet i kadencje na miesięcznej/kwartalnej. Żaden okres rozliczeniowy nie jest liczony w tygodniach |

### Otwarte — do rozstrzygnięcia przed startem fazy

| # | Decyzja | Założenie M8 | Z kim uzgodnić |
|---|---|---|---|
| D1 | ~~Rozszerzenia, o które M8 prosi M5~~ | **ZAMKNIĘTE w M8a: M5 dostarczył wszystko z wyprzedzeniem.** `AccountOwner::City`, `TxKind::TaxPayment { charge: ChargeKind }` i `TxKind::PublicSpend { program: ProgramId }` stoją w `books.rs` od M5 z komentarzem „wariant zamówiony przez M8". Kont `*Payable` **nie ma i nie będzie**: `LedgerAccount` ma jedno `TaxPayable` na wszystkie daniny, a rozbicie per danina niesie `ChargeRegistry` — pięć kont z jedną treścią kosztowałoby pięć martwych wierszy w bilansie każdego zakładu | **M5** |
| D2 | ~~Czy cena detaliczna w ofercie jest brutto czy netto~~ | **ZAMKNIĘTE przez `K-7` przed startem fazy: brutto.** M8a wykonuje to bez odstępstwa — `CityTaxEngine::gross_from_net` jest ostatnim krokiem składania ceny półkowej, a `vat_on_gross` wyłuskuje podatek z kwoty, którą mieszkaniec właśnie zapłacił | **M5** |
| D3 | ~~Kto nalicza cło w węźle importowym~~ | **ZAMKNIĘTE w M8a, inaczej niż zakładano.** M6 liczy cło **u siebie** (`ImportQuote.duty`, tabela `data/trade/tariffs.ron`) i wsadza je w koszt nabycia partii — tak jest od M6c i `K-36` na tym stoi. M8 nie wywołuje `duty_due` w cudzym kodzie: odbiera fakt przez kolejkę `Market::take_b2b_tax`, zapisuje należność i zabiera pieniądz z kanału importowego. Stawki są od tej chwili polityką miasta, mimo że plik ma cudzy adres (`CA-7`) | **M6** |
| D4 | ~~Czy `City` jest encją ECS czy zasobem~~ | **ZAMKNIĘTE w M8a odwrotnie do propozycji: zasób.** Uzasadnienie propozycji („żeby mieściła się w haszu stanu i snapshotcie bez wyjątków") pokrywa `World::register_resource_hash` (`K-29`), z którego korzystają już `Books`, `Market`, `Firms` i `CorpFinance`. Encja z jednym egzemplarzem kosztowałaby archetyp, bufor komend i pytanie „którą" (`CA-10`) | **M0** |
| D5 | Bankructwo z tytułu zaległości podatkowych | M8 wystawia `Remedy::BackTax` i wniosek egzekucyjny; postępowanie upadłościowe i wyprzedaż majątku — M7 | **M7** |
| D6 | Granulacja zasięgu usług publicznych | per dzielnica (`DistrictId`), z zanikiem po czasie przejazdu między centroidami. `ponytail:` sufit — szkoła obwodowa może być zbyt zgrubna; wyjście: promień w metrach na siatce M2 | **M2, M3** |
| D7 | `MediaExposure` wyborcy przed M10 | stub `f(nakład publiczny kandydata, dzielnica)`; M10 podmienia na zasięg realnych tytułów bez zmiany sygnatury | **M10** |
| D8 | Płaca minimalna — polityka miasta czy parametr rynku pracy | `Policy::MinWage` uchwalana przez M8, egzekwowana przez M7 przy ustalaniu ofert pracy | **M7** |
| D10 | Czy operatorzy sieci są przejmowalni przez gracza w M8 | w M8 wyłącznie firmy AI z taryfą; przejęcie/przetarg na operatora — M9/M10 | **M9, M10** |
| D11 | Kto stosuje `DemographyParams` | M8 liczy parametry (dzietność, migracja), M3 je stosuje do populacji | **M3** |
| D12 | ~~Podstawa podatku od nieruchomości~~ | **ZAMKNIĘTE w M8a: `land_value_per_m2 × area_m2` parceli z M2, zamrażane przy budowie miasta.** Wyceny aktualizowanej z transakcji nie ma, bo nie ma rynku nieruchomości — `land_value` liczy `city::value::pass_2` przy generacji i po niej nie drga. Zamrożenie na 1 stycznia będzie miało treść dopiero wtedy, gdy wycena zacznie się zmieniać w trakcie gry; do tego czasu jest tożsamością. Płatnikiem jest zakład stojący na parceli (`CA-11`) | **M5** |
| D13 | Czy strajk jest zdarzeniem (`sim/events`) czy mechaniką związków (M10 §6.6) | M8 dostarcza zdarzenie `social/strike` z hazardem; M10 może podmienić wyzwalanie na model związków zawodowych, zachowując te same `SimParam` | **M10, M7** |
| D14 | Czy zdarzenia mogą wyzwalać zdarzenia (`EventCause::Chained`) | tak, ale wyłącznie przez zmianę sondy (pośrednio) — bezpośrednie łańcuchy tylko tam, gdzie to fizyka (awaria linii → rozpad wyspy) | — (decyzja M8, do rewizji po balansie) |
| D15 | ~~Częstotliwość `sys_solve_power_grid`~~ | **ZAMKNIĘTE w M8b pomiarem: `EveryMinute` i zostaje.** Benchmark `m8b-1` daje 186 µs na tick **pięciu** sieci metropolii wobec budżetu 300 µs, więc wariant awaryjny („`EveryHour` z wyzwalaniem zdarzeniowym") nie jest potrzebny. System nazywa się `traffic.Utility`, jest wyłączny i otwiera tick (`K-53`); prąd liczy się co minutę, reszta mediów co godzinę — zgodnie z tabelą częstotliwości z §5.4 | **M12** (profilowanie) |

---

## 10. Szacunek wielkości

| WP | Zakres | Rozmiar |
|---|---|---|
| WP1 | Szkielet `sim/city`, `CityBudget`, księga publiczna, dług | **M** |
| WP2 | `TaxCode`, siedem danin, cykl należności, integracja z księgą | **L** |
| WP3 | Sieci przesyłowe: graf, solver, blackout, kaskada, taryfy, faktury | **L** |
| WP4 | Rdzeń `sim/events`: hazard, sondy, `ParamOverlay`, rejestr | **L** |
| WP5 | Katalog zdarzeń w `data/events/` + walidator CI | **M** |
| WP6 | Pogoda, sezony, epoka, wskaźniki makro, parametry demografii | **M** |
| WP7 | Usługi publiczne, obwody, urzędy z kolejką pozwoleń | **L** |
| WP8 | Pięć urzędów, kontrole, kary, szara strefa | **M** |
| WP9 | Władza miejska AI: cele, decyzje, przetargi, dotacje, regulacje | **L** |
| WP10 | Wybory: frekwencja, preferencje, mandaty, wsparcie gracza | **M** |
| WP11 | Inspektory, Kronika, hash stanu, scenariusz demo | **M** |

Rozkład: 4 × L, 6 × M, 1 × M (WP1). Najcięższe i najbardziej ryzykowne są WP2 (precyzja
do grosza), WP3 (jedyny nowy solver numeryczny w fazie) i WP4 (fundament, od którego zależą
WP5 i WP8). Kolejność startu: WP1 → WP2 równolegle z WP3 → WP4 → WP5/WP6 równolegle →
WP7 → WP8/WP9 → WP10 → WP11.

## Zmiany wpisane po M3

Zgodnie z `K-18`.

| # | Zmiana | Dlaczego |
|---|---|---|
| Z-1 ★ | **`Safety` ma dziś tempo spadku 0 i to M8 je wpisuje** (`data/needs/needs.ron`, korekta H-3) | Bezpieczeństwo nie ma w M3 ani miejsca zaspokojenia, ani mechanizmu odbudowy, a migracja czyta jego próg (`safety_need_threshold`). Zostawione ze spadkiem 0,15 pkt/h dochodziło u wszystkich do zera w ciągu miesiąca gry i zaczynało wypychać z miasta każdego po kolei. M8 wnosi przestępczość i usługi publiczne — i razem z nimi tempo oraz sposób odbudowy |
| Z-2 | **Instytucje publiczne są dziś zwykłymi miejscami w katalogu** (`PlaceKind::Education`, `Doctor`, `Hospital`, `Leisure`, `Social`), wskazanymi w `data/buildings/*.ron` polem `place_kind` (korekta H-16) | Decyzja 9.15 fazy M3: w M3 instytucje są nieskończone, M8 podmienia implementację `PlaceProvider` i dokłada pojemność oraz kolejki. Odwzorowanie „archetyp zakładu → rodzaj miejsca" jest już w danych, więc M8 zmienia zachowanie, a nie katalog |
| Z-3 | **Uczeń ma przypisaną szkołę z Etapu 8** (`Employment.site` ucznia wskazuje najbliższą placówkę `Education`), a nie „szkołę w ogóle" | M8 dokłada pojemność szkoły i rejonizację. Dziś przypisanie jest po najbliższej w promieniu 800 m → 2,5 km → 8 km i zawsze się udaje; liczba uczniów bez placówki jest w raporcie Etapu 8 i wynosi zero |


## Zmiany wpisane po M8a

Zgodnie z `K-18`. Poprawki do dokumentów **podfaz następnych**, które M8a pokazała
jako nieprawdziwe. Gwiazdka = zmiana zakresu albo kryterium.

| # | Dokument | Zmiana | Dlaczego |
|---|---|---|---|
| `CB-1` ★ | `M8b-sieci-przesylowe.md`, `M8c`, `M8e` | **`UtilityKind` z §5.4 nie powstaje — rodzajem mediów jest `UtilityService` i on już jest w `engine/core`.** `M8b` §5.4 deklaruje `pub enum UtilityKind { Power, Gas, Water, Sewage, Heat, Telecom }`; nazwa `UtilityKind` jest **zajęta** i znaczy co innego: to wymiar oceny w funkcji użyteczności zakupu (`Price`, `Quality`, `Distance`, …), wniesiony przez M5. Rodzaj mediów nazywa się `UtilityService` (`Electricity`, `Water`, `Sewage`, `Gas`, `Heat`, `Waste`, `Internet`), siedzi w `core::vocab` od M5 i jest ładunkiem `TxKind::Utility`, czyli **już dziś** niesie każdą fakturę za media w księgach. Drugi enum o tej samej treści rozjechałby się przy pierwszej zmianie, a pierwsza zmiana jest tu pewna, bo M8b dokłada sieci | Przypadek (1) i (2) z `K-18` naraz: kontrakt wygląda inaczej, niż podfaza zakłada, i API nazywa się inaczej. Do wykrycia było przy pierwszym `use magnat_core::UtilityKind` w M8b — czyli po napisaniu połowy solvera |
| `CB-2` ★ | `M8e-wladza-i-wybory.md` §5.8 | **Tabeli systemów podatkowych z §5.8 nie ma i nie będzie w tym kształcie.** `sys_assess_vat` („na transakcji"), `sys_assess_payroll_pit` („na wypłacie"), `sys_settle_tax_charges` i `sys_budget_close` to **cztery wiersze opisujące jeden system**: `city.Tax`, `Cadence::EveryDay`, wyłączny (`K-21`), stojący po `economy.Market` (`K-51`). Naliczenie na transakcji i na wypłacie nie jest systemem, tylko **hakiem** w `TaxEngine` (`K-57`) — system ECS nie ma jak wpiąć się w środek przelewu. Domknięcie budżetu też nie jest osobnym systemem: jest krokiem piątym tego samego, bo musi stać **po** rozliczeniu należności, a dwa systemy wyłączne o wymuszonej kolejności to `K-53` bez potrzeby | Przypadek (5) z `K-18`: pakiet obiecuje coś, czego żaden pakiet nie jest właścicielem. Wiersze zostają jako **nazwy kroków**, nie systemów |
| `CB-3` | `M8d-uslugi-i-prawo.md` §5.3 | **`Remedy::BackTax` ma gotowe wejście: `ChargeRegistry::accrue`, a nie „nowy `TaxCharge`".** Domiar to należność jak każda inna — z płatnikiem, okresem, stawką w migawce i terminem. Umorzenie po nieskutecznej egzekucji ma też gotowe wejście: `magnat_city::abate_bankrupt(city, payer, t)` zamyka wszystkie otwarte należności płatnika z powodem `AbateReason::Bankruptcy` i utrzymuje domknięcie T1 | Przypadek (2) z `K-18`. Przy okazji znika pytanie, kto zdejmuje z budżetu należność firmy, która upadła: nikt jej nie zdejmuje, tylko przesuwa do czwartego stanu |
| `CB-4` ★ | `M8b-sieci-przesylowe.md` | **Akcyza od energii ma adres i to jest M8b.** W M8a akcyza jest wpięta w dwa prawdziwe punkty (sprzedaż detaliczna, rozliczenie hurtowe) i nie ma czego obłożyć: paliwo kupują pojazdy przez `FuelLedger`, który nie ma konta w księgach, piwo przegrywa z sokiem w rangach substytutu, papierosów nie ma w asortymencie. `ExciseClass::Energy` z PRD §6.8 jest jedyną z czterech klas, która ma w tej grze codzienny wolumen — i przechodzi dokładnie przez to, co M8b buduje: rachunek za media | `R2` („martwe hazardy") zastosowane do daniny. Danina, której nikt nigdy nie naliczył, przechodzi **każdy** test domknięcia i wygląda tak samo jak danina, której nikt nie zapłacił |
| `CB-5` | wszystkie podfazy M8 | **Blok `StreamId` 240–259 jest w całości wolny po M8a.** Naliczenie daniny jest funkcją stanu, nie losowaniem, więc przydział z §5.0 (`244 Weather`, `245 Election`, `246 Audit`, …) zostaje bez zmian i bez ani jednej wartości zajętej. Pierwszy numer zajmie M8c | Wartości `StreamId` są wieczne, więc informacja „nic nie zajęte" jest tak samo wiążąca jak lista zajętych |


## Zmiany wpisane po M8b

Zgodnie z `K-18`. Poprawki do dokumentów **podfaz następnych**, które M8b pokazała
jako nieprawdziwe albo niedopowiedziane. Gwiazdka = zmiana zakresu albo kryterium.

| # | Dokument | Zmiana | Dlaczego |
|---|---|---|---|
| `CD-1` ★ | `M8c-zdarzenia.md` | **Sieci gazu i ciepła jeszcze nie ma i to M8c je stawia — razem z popytem, nie zamiast niego.** M8b zbudowała solver obojętny na medium i most, który stawia sieć wtedy, gdy ma ona **odbiorcę z licznikiem**. Gaz i ciepło odbiorcy dziś nie mają: zapotrzebowanie na ciepło jest funkcją pogody, a pogoda przychodzi w M8c (WP6). Pakiet pogodowy dokłada więc dwa węzły popytu per dzielnica i dwa wywołania `UtilityGrids::push` w `tools/headless/src/grid.rs` — nie nowy solver i nie nowy typ | `R2`: sieć bez ani jednego odbiorcy przechodzi każdy test i wygląda w raporcie tak samo jak sieć, która działa. Kolejność „najpierw popyt, potem sieć" jest jedyną, w której da się to sprawdzić |
| `CD-2` | `M8c-zdarzenia.md` | **Awaria bloku energetycznego ma gotowe wejście: `UtilityNetwork::set_source_online(node, false)`.** M8b wystawia je jako jedyną drogę, którą awaria wchodzi do sieci, i **nie** ma własnego hazardu — wiek bloku, zaległa konserwacja i miesiące pracy na 98 % mocy są sondami M8c. Tak samo `EdgeState::Tripped { until }` jest już ustawiany przez solver, więc zdarzenie „zerwana linia" ma gdzie usiąść | Przypadek (5) z `K-18` odwrócony: pakiet, którego właściciela warto nazwać, **zanim** ktoś zbuduje drugi. Hazard w M8b byłby loterią zamiast skutku stanu świata — a §1 pkt 6 dokumentu fazy obiecuje odwrotnie |
| `CD-3` | `M8c-zdarzenia.md` | **`StreamId::GridFault = 248` jest zajęty i wieczny.** Losuje **czas naprawy** krawędzi, nie samo zadziałanie zabezpieczenia — to drugie wynika z bilansu wyspy i jest funkcją stanu. Blok M8 (240–259) ma zajęty wyłącznie ten jeden numer; przydział z M8a §5.0 zostaje bez zmian | Wartości `StreamId` są wieczne (`K-4`), więc informacja „zajęty jeden, ten" jest tak samo wiążąca jak lista wolnych. `CB-5` po M8a mówiło „blok w całości wolny" i to zdanie przestało być prawdziwe |
| `CD-4` | `M8d-uslugi-i-prawo.md` | **Priorytet 0 zrzutu obciążenia ma dziś jednego odbiorcę — ujęcie wody — i M8d dopisuje resztę.** `data/tuning/grid.ron` niesie `priority_critical: 0`, a most przypina go jedynemu węzłowi, który dziś zasługuje: przyłączu wodociągu, bo jego zgaśnięcie zabiera wodę całemu miastu (`UtilityGrids::link_source`). Szpital, straż i posterunek dostaną ten sam priorytet wtedy, gdy będą **instytucjami z pojemnością**, czyli w WP7 | Przypadek (1) z `K-18`: §5.4 mówi „szpital i wodociąg mają priorytet 0", a w M8b istnieje wyłącznie wodociąg. Zapisane, żeby WP7 nie zbudował drugiej tabeli priorytetów obok istniejącej |
| `CD-5` ★ | `M8e-wladza-i-wybory.md` | **`Policy::TariffCap` ma gotowe wejście i gotowy skutek, ale nie ma gotowej pętli.** Taryfa jest polem sieci (`UtilityNetwork.tariff`, wartości z `data/tuning/grid.ron`) i to ona ląduje w liczniku zakładu co krok — uchwała ograniczająca ją działa więc od razu po zmianie jednej liczby. Czego **nie ma**: sondy `MaintenanceBacklogDays` i związku „operator tnie konserwację → rośnie hazard awarii" z §5.4, bo operator nie ma w M8b rachunku wyników (`CC-14`), więc nie ma czego ciąć. Pętla emergentna z §5.4 jest pakietem M8e, a nie stanem zastanym | Przypadek (5) z `K-18`: §5.4 opisuje pętlę, której żaden pakiet M8b nie jest właścicielem. Wpisane wprost, bo z samego istnienia `TariffCap` łatwo wnieść, że skutek też już jest |
| `CD-6` | `M8e-wladza-i-wybory.md` | **Planowy remont linii (`EdgeState::UnderMaintenance`) należy do M8e i wariantu jeszcze nie ma.** M8b ma dwa stany krawędzi: `Ok` i `Tripped { until }`. Trzeci dopisuje się razem z tym, kto go ustawi — czyli razem z harmonogramem konserwacji operatora | `R2` zastosowane do wariantu enuma (`CC-4`). Wariant, którego nic nie ustawia, przechodzi każdy test i w raporcie wygląda tak samo jak wariant działający |
| `CD-7` ★ | `M8e-wladza-i-wybory.md` §5.8 | **`traffic.Utility` otwiera tick (`opens_tick`, `K-53`) i jest systemem wyłącznym.** Tabela systemów w §5.8 musi go uwzględnić: stan zasilania jest warunkiem wstępnym dla produkcji, więc system ustalający go po `supply.Chain` spóźniałby blackout o minutę — a T2 mierzy „wolumen produkcji w oknie awarii dokładnie 0". Po M8b dwa systemy otwierają tick: `firms.Firm` (M7f) i `traffic.Utility`; szeregują się między sobą kanonicznie i to jest dopuszczalne wprost (`K-53` pkt c) | Przypadek (4) z `K-18`: kolejność pakietów w tabeli systemów jest niewykonalna bez tej deklaracji. Zapisane teraz, bo §5.8 jest jedynym miejscem, w którym kolejność systemów fazy jest widoczna naraz |


## Zmiany wpisane po M8c

Zgodnie z `K-18`. Poprawki do dokumentów **podfaz następnych** i faz dalszych,
które M8c pokazała jako nieprawdziwe albo niedopowiedziane.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Dokument | Zmiana | Dlaczego |
|---|---|---|---|
| `CF-1` | `M8d-uslugi-i-prawo.md` | **Mechanizm hazardu jest gotowy do reużycia i WP8 nie buduje drugiego.** `Probe`, `Precondition`, `Curve`, `hazard_ppm`, `roll`, `severity_bps` i rejestr `Events` stoją w `sim/events` i są publiczne. Kontrola skarbowa dokłada **sondę i wiersz w `data/events/`**, a nie system: `Probe::FirmUnreportedBps` obok istniejących sond firmowych, definicja w nowej kategorii albo w `firm.ron`, a kara przez `ChargeRegistry::accrue` (`CB-3`). Dokładnie to zapowiada WP8 („reużyty, nie drugi system") i po M8c jest to wykonalne dosłownie | Przypadek (2) z `K-18`: API istnieje i nazywa się konkretnie. Zapisane, żeby WP8 nie zaczął od własnego `roll` — a zaczyna się od niego wtedy, gdy nie wiadomo, że cudzy jest publiczny |
| `CF-2` ★ | `M8d-uslugi-i-prawo.md` | **`ServiceQuality(ServiceKind, DistrictId)` z §5.5 nie istnieje jako sonda i to M8d ją wnosi.** Nie powstała w M8c, bo placówek publicznych nie ma (`CE-1`). Wraz z nią M8d dokłada `ServiceKind` — a ten trafia do `engine/core::vocab`, jeśli okaże się, że czyta go więcej niż jedna faza (`K-8`), bo kolejność wariantów będzie indeksować pokrycie obwodowe | Przypadek (5) z `K-18`: §5.5 obiecuje sondę, której żaden pakiet M8c nie jest właścicielem |
| `CF-3` | `M8d-uslugi-i-prawo.md` | **Priorytet 0 zrzutu obciążenia dotyczy teraz czterech sieci, nie jednej.** `CD-4` mówiło „dziś jednego odbiorcę — ujęcie wody"; po M8c miasto ma prąd, wodę, gaz i ciepło, więc szpital przyłączony w WP7 dostanie węzeł w **każdej** sieci, którą bierze. Tabela priorytetów zostaje jedna (`data/tuning/grid.ron`) i nie powstaje druga obok | Przypadek (1) z `K-18`: stan, na którym stoi WP7, wygląda inaczej niż w chwili pisania `CD-4` |
| `CF-4` | `M8e-wladza-i-wybory.md` | **`EventCause` dostaje `Policy` i `Player` dopiero w M8e** (`CE-6`). Uchwała rady wywołująca zdarzenie i komenda gracza to dwa warianty do dopisania **na końcu** enuma, razem z tym, kto je ustawia. Wejście istnieje: `Events::open` przyjmuje gotowy `WorldEvent`, więc zdarzenie z uchwały jest tym samym zdarzeniem, tylko bez rzutu | `R2`: wariant, którego nic nie ustawia, przechodzi każdy test. Zapisane, żeby M8e nie szukał osobnej drogi dla zdarzeń politycznych |
| `CF-5` ★ | `M8e-wladza-i-wybory.md` | **`Policy::TariffCap` dotyczy czterech sieci, a `data/tuning/grid.ron` ma już cztery taryfy.** Taryfa jest polem sieci (`UtilityNetwork.tariff`) i ląduje w liczniku zakładu co krok, więc uchwała ograniczająca ją działa po zmianie jednej liczby — ale liczb jest teraz cztery, nie dwie. Czego dalej nie ma (bez zmian wobec `CD-5`): rachunku wyników operatora, więc pętla „operator tnie konserwację → rośnie hazard awarii" nadal czeka na M8e. **Sonda jest za to gotowa**: `SourceMaintenanceOverdueDays` liczy zaległość konserwacyjną zakładu prowadzącego źródło i jest już czynnikiem hazardu awarii bloku | Przypadek (1) z `K-18`. Połowa pętli z §5.4 powstała w M8c; brakuje strony kosztowej, a ta jest decyzją `D10` odłożoną do M9/M10 |
| `CF-6` | `M8e-wladza-i-wybory.md` | **Kronika ma już model danych i niesie podmiot, nie tekst.** `ChronicleEntry` (`sim/events`) trzyma `EventId`, definicję, kategorię, zakres, siłę i to, czy wpis mówi o początku czy o końcu; klucz tekstu stoi w `EventDef::chronicle` i jest **kluczem lokalizacji**, a nie napisem. WP11 buduje więc panel i historię, a nie model — i nie musi parsować podmiotu z tekstu. Pierścień ma dziś 128 wpisów; kronika pełna (z zapisem gry) jest zakresem WP11 | Wykonanie wymagania z tabeli „Zmiany wpisane po decyzji właściciela produktu": wpis kroniki niesie `Subject`, a `EventId` jest jego uchwytem (`K-62`) |
| `CF-7` | `M9c-gracz-inspekcja-nakladki.md` | **Karta zdarzenia ma gotowe wejście i jest nim `Events::diagnosis`.** Odpowiedź na „dlaczego" przy zdarzeniu to nie jeden powód, tylko **rozbicie hazardu na czynniki**: `Diagnosis::best_factors` niesie listę `(sonda, wartość, mnożnik)` z ostatniej oceny, a `Diagnosis::{candidates, gated_out, best_ppm, fired}` mówi, ile instancji w ogóle rozważano. `Events::active()` daje „co trwa i jak długo jeszcze". `EventId` jest w `engine/core` (`K-63`), więc `Subject::Event(EventId)` nie wymaga nowego typu | Przypadek (2) z `K-18`: API, którego M9c używa w opisie, istnieje i nazywa się konkretnie |
| `CF-8` | `M10e-relacje-i-zwiazki.md` | **Strajk istnieje i ma sondę luki płacowej — związki podmieniają wyzwalanie, nie parametry.** Decyzja `D13` fazy mówi: „M10 może podmienić wyzwalanie na model związków zawodowych, zachowując te same `SimParam`". Po M8c to zdanie ma treść: `social/strike` ma zakres `Firm`, kończy się warunkiem `UntilBelow` nad `FirmWageGapPermille` (czyli gdy firma podniesie płace) i zmienia dokładnie jeden parametr — `SiteOutputMulBps`. Sonda `UnionDensityBps` **nie powstała** (`CE-1`) i to M10 ją wnosi razem z uzwiązkowieniem, jako czwarty czynnik tej samej definicji | Przypadek (2) i (5) z `K-18` naraz. Zapisane, bo najłatwiejszą pomyłką jest zbudowanie w M10 drugiego strajku obok tego |
| `CF-9` | `M10-glebia.md` | **`EpochClock` czeka na `TechId` i `data/tech/`** (`CE-7`). Zegar ma rok gry i lata od startu; odblokowania technologii nie ma, bo nie ma czego odblokowywać. M10 dokłada `TechId`, katalog i pole — do istniejącego zegara, a nie do nowego | `R2` zastosowane do pola struktury. Pusty zbiór wygląda tak samo jak zbiór, którego nikt nie wypełnił |
| `CF-10` ★ | `R2-naprawy-po-M11.md`, `M8e-wladza-i-wybory.md` | **Sonda `MoodMean` mierzy dziś −99 i jest w praktyce stałą.** Nastrój mieszkańca **tylko spada**: `DeprivationEffect::MoodLoss` (`sim/agents/src/needs.rs`) jest jedynym pisarzem `Vitals.mood` w całym repozytorium i robi `saturating_sub`, a odbudowy nie ma nigdzie. Po roku gry cała populacja siedzi na −100. Skutek dla M8c jest konkretny: krzywe protestu (×4,0), fali przestępczości (×2,5) i embarga stoją **na końcu krzywej od pierwszego roku**, więc mnożnik przestaje cokolwiek różnicować. Zdarzenia nadal zachodzą i bramka kategorii jest zielona, ale kalibracja społeczna czeka. Dopisane do wykazu `R2` jako pozycja 43, do pakietu `R2-WP16` — tam, gdzie leżą już `StatusLoss` i `ProductivityLoss`, czyli dwa inne puste ramiona tego samego `match`. **Drugi warunek tej kalibracji, pozycja 6 wykazu (bezrobocie 6 ‰), świadomie został w R2** — i po M8c widać, po co: sonda `UnemploymentPermille` też mierzy liczbę, o której wiadomo, że jest fikcją | Znalezione pomiarem, nie przeglądem: raport `m8miasto --days 400` pokazuje wartości sond obok mnożników, więc „×4,00 przy MoodMean=−99" rzuca się w oczy przy pierwszym czytaniu. To jest zysk z tego, że diagnostyka pokazuje **czynniki**, a nie liczbę wynikową (WP4) — hazard 2 800 ppm niczego by nie zdradził |
