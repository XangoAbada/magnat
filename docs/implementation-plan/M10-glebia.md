# M10 — Głębia

Status: plan fazy. Podlega `00-konwencje-i-kontrakty.md` (typy bazowe, determinizm, LOD, dane, testy).
Źródło wymagań: `PRD_Magnat.md` §4.2 (Etap 9–10), §5.7, §6.5, §6.6, §7.6, §7.7, §7.9, §11.2–11.3, §12.3–12.4, §16.2, §17.4, §17.7, §19.

Właściciel crate'u: `sim/macro` (pełna postać). Rozszerza: `sim/firms`, `sim/economy`, `sim/agents` (pamięć), `sim/events`.

---

## 1. Cel fazy i artefakt końcowy

Po M9 istnieje żywe miasto z firmami AI, rynkiem pracy, miastem-aktorem i graczem-karierowiczem, ale
**świat startuje sterylny** (generator ustawia go „na zero") i **firmy są płaskie** — nie mają marki poza
liczbą, nie mają technologii, nie mają właścicieli innych niż jeden mieszkaniec, nie mają historii relacji.

M10 domyka trzy braki:

1. **Świat zaczyna się „zużyty".** `sim/macro` przeprowadza skróconą symulację 30–100 lat przed startem
   partii i ustala ceny wyjściowe, zapasy, zadłużenie firm, majątki rodzin, sieci stałych dostawców
   i historię dzielnic (upadek przemysłu, gentryfikacja). Ten sam kod obsługuje „co jeśli" firm AI (M7)
   i tryb 50× (M12) — to jest jeden model, nie trzy.
2. **Marka przestaje być liczbą.** Jest zbiorem afinitetów w pamięci konkretnych mieszkańców; reklama
   dociera do konkretnych ludzi, po konkretnych trasach i kanałach, a rozczarowanie niszczy markę
   szybciej, niż reklama ją buduje.
3. **Firmy zyskują warstwy wieloletnie:** R&D i epoki, giełda z wyceną z transakcji, media jako firmy
   kształtujące opinię, relacje międzyfirmowe (stali dostawcy, kartele, franczyza, JV), związki zawodowe
   i strajki, ubezpieczenia wyceniające ryzyko z historii zdarzeń.

### Artefakt końcowy (co da się uruchomić i zobaczyć)

**A. `tools/headless --dry-run --years 80 --seed X --size metropolia`** — generuje świat i wypisuje raport:
oś czasu 80 lat (kiedy powstała huta, kiedy upadła, kiedy Wzgórza Zachodnie przestały być robotnicze),
tabelę cen startowych, rozkład majątków, mapę relacji dostawców, listę 200 wpisów kronikarskich,
oraz **zielony raport Etapu 10** (żaden rynek w nierównowadze > 30%). Czas: ≤ 60 s jednowątkowo.

**B. `tools/headless --lod-consistency`** — ten sam scenariusz w mezo i w makro przez 12 miesięcy gry (360 dni, kalendarz K-1);
raport różnic agregatów pieniężnych per dzielnica z werdyktem PASS/FAIL przy tolerancji z §7.

**C. W grze:** gracz otwiera panel marketingu, kupuje billboard przy konkretnej ulicy i widzi na mapie
cieplnej, **którzy mieszkańcy** (ilu, z jakich dzielnic) zostali w tym tygodniu wyeksponowani, a po
miesiącu widzi w karcie inspekcji mieszkańca wpis „znam markę «Orlex» — z billboardu na Wołoskiej,
oczekiwana jakość 72, doświadczenie 58, afinitet −11 (rozczarowanie)".

**D. W grze:** firma gracza wchodzi na giełdę, notowana jest w codziennym fixingu, konkurent AI skupuje
akcje, przekracza 5% (komunikat → plotka → media), przekracza 50% i przejmuje kontrolę. Załoga fabryki
gracza formuje związek, przedstawia żądanie, negocjacje padają, produkcja staje, kary z kontraktów B2B
uruchamiają kaskadę u odbiorców, gazeta pisze o strajku, marka gracza traci afinitet.

---

## 2. Zakres — wchodzi / nie wchodzi

### Wchodzi

| # | Temat | PRD |
|---|---|---|
| 1 | `BrandAffinity` w pamięci mieszkańców, agregat „siła marki" tylko w UI | §7.6 |
| 2 | Kampanie i kanały: billboard (zasięg z tras), prasa/radio/TV (czytelnictwo dzielnic), ulotki, promocje, sponsoring, PR | §7.6 |
| 3 | Asymetria: reklama → znajomość + oczekiwana jakość; rozczarowanie → szybka utrata afinitetu | §7.6 |
| 4 | R&D: dział, punkty badawcze, `TechTree` per branża, `Patent`, licencjonowanie | §7.7 |
| 5 | Technologie „światowe" pojawiające się w czasie, epoki, nowe kategorie produktów zmieniające potrzeby i łańcuchy | §7.7, §11.3 |
| 6 | Giełda: IPO, `OrderBook` z fixingiem, `Valuation` z opóźnionych i zaszumionych wyników + plotek | §6.5 |
| 7 | Dywidendy, emisje, przejęcia wrogie i przyjazne, ład korporacyjny | §6.5, §7.9 |
| 8 | Ubezpieczenia: `InsurancePolicy`, wycena z historii zdarzeń, wypłaty, niewypłacalność ubezpieczyciela | §6.5, §11 |
| 9 | Media jako firmy: sprzedaż powierzchni reklamowej, `Story` → plotka i opinia | §7.2, §7.6, §11.2 |
| 10 | Relacje międzyfirmowe: stali dostawcy, ekskluzywność, `Cartel` z ryzykiem kary, franczyza, JV, integracja pionowa/pozioma | §7.9 |
| 11 | `Union`: formowanie żądania z rozdźwięku zysk/płace + sieci relacji, negocjacje, strajk | §6.6, §11.2 |
| 12 | `sim/macro` w pełnej postaci: `MacroCell`, `step`, `lift`/`lower` | §16.2, §17.4 |
| 13 | Historia „na sucho": `DryRunConfig`, etapy D0–D5, rozwinięcie do pełnego świata, weryfikacja Etapu 10 | §4.2 |
| 14 | Kroniki: nowe warianty zdarzeń z powyższych systemów + wpisy z dry-runu | §14, §19 |

### Nie wchodzi

| Temat | Gdzie |
|---|---|
| Podstawowa mechanika firmy, zakładu, produkcji, HR, polityk cenowych, osobowości firm AI | M7 — M10 **rozszerza**, nie redefiniuje |
| Rynek pracy jako taki (oferty, aplikacje, licytacja płac) | M7 — M10 dokłada wyłącznie warstwę związkową |
| Podatki, urzędy, regulator antymonopolowy jako organ, prawo, wybory | M8 — M10 **konsumuje** hook regulatora do wykrywania karteli |
| Kontrakty B2B, kary umowne, rynek spot | M6 — M10 konsumuje (franczyza i licencja to `ContractId`) |
| Panele UI, wykresy, tabele, edytor reguł | M9 — M10 zgłasza wymagania (§6) |
| Kronika jako podsystem (przechowywanie, indeks, wyszukiwarka) | M9 — M10 dopisuje warianty zdarzeń |
| Tryb 50× jako zagadnienie wydajnościowe, zapis w tle, profilowanie | M12 — M10 dostarcza mu `sim/macro` jako silnik |
| Bank centralny, stopa bazowa, kredyt | M5/M7 — M10 konsumuje stopę do wyceny |
| Zdarzenia losowe i pogoda | M8 — M10 konsumuje strumień zdarzeń do wyceny ubezpieczeń |

---

## 3. Mapowanie na PRD

| Sekcja PRD | Co z niej realizuje M10 |
|---|---|
| §4.2 Etap 9 | Historia „na sucho" — WP10.3 |
| §4.2 Etap 10 | Weryfikacja i naprawa świata startowego — WP10.3, §7 |
| §5.1 Pamięć | Rozszerzenie magazynu doświadczeń o sloty marek — WP10.5 |
| §5.7 Informacja i plotka | Reklama i media jako czwarte źródło wiedzy obok wizyt, relacji i tras — WP10.6, WP10.7 |
| §6.4 Decyzja zakupowa | Wypełnienie składników `w_marka · afinitet_do_marki` i `jakość_postrzegana` realnymi danymi — WP10.5 |
| §6.5 Giełda, ubezpieczenia | WP10.10–10.12 |
| §6.6 Związki zawodowe i strajki | WP10.14 |
| §7.4 Produkt | Cechy specjalne z R&D — WP10.8 |
| §7.6 Marketing i marka | WP10.5, WP10.6 |
| §7.7 Badania i rozwój | WP10.8 |
| §7.8 Finanse firmy | Emisja akcji jako realne źródło kapitału — WP10.11 |
| §7.9 Relacje międzyfirmowe | WP10.13 |
| §11.2 Społeczne, firmowe | Strajk, protest, skandal, moda, recenzja w mediach jako wejścia do marki i plotki — WP10.7 |
| §11.3 Postęp technologiczny epoki | WP10.9 |
| §12.3 Poziom strategiczny | `sim/macro` jako silnik „co jeśli" dla firm AI — WP10.1 |
| §12.4 Powstawanie i upadek | Przejęcia jako druga (obok bankructwa) ścieżka wyjścia firmy — WP10.11 |
| §16.2 crate `macro` | WP10.1 |
| §17.4 LOD makro | WP10.1, WP10.2, WP10.4 |
| §17.7 Pamięć | Budżet pamięci marki — §5.1 tego dokumentu |
| §19 M10 | Cały dokument |

---

## 4. Pakiety robocze

Ścieżka krytyczna: **WP10.2 → WP10.1 → WP10.3 → WP10.4**. Ona blokuje M12 (tryb 50×) i zamyka
Etap 9–10 generatora. Reszta jest równoległa i może iść w dowolnej kolejności po spełnieniu zależności.

### WP10.2 — Wydzielenie wspólnego jądra ekonomicznego (`econ_kernel`)

**Zależności:** M5 (rdzeń), M6 i M7 (wypełnienie slotów).
**Charakter:** **domknięcie, nie refaktor** — i to jest zmiana na lepsze względem pierwszej wersji
tego planu. M5 pisze `kernel` od razu jako rdzeń, bo te funkcje w M5 dopiero powstają; nie ma więc
czego wyciągać po fakcie. M10 dokłada wyłącznie brakujące funkcje i ustanawia regułę.

Sedno spójności makro↔mezo nie leży w „starannym kalibrowaniu dwóch modeli" — takie kalibrowanie
zawsze rozjeżdża się po trzech sprintach. Leży w tym, że **model jest jeden**. Dlatego przed napisaniem
`sim/macro` wyciągamy z `sim/economy` i `sim/firms` funkcje czyste, wołane z obu poziomów:

```rust
// sim/economy/src/kernel.rs — nowy moduł, przeniesione ciała funkcji
pub fn purchase_score(ctx: &ScoreCtx, opt: &OptionView) -> Score;     // §6.4, bez losowania
pub fn softmax_shares(scores: &[Score], beta: Q, out: &mut [u32]);    // udziały w permille, suma == 1000_000
pub fn next_price(cur: Money, s: &PricingState, p: &PricingPolicy) -> Money;  // §6.3
pub fn wage_bid(role: JobRoleId, s: &LaborState, p: &WagePolicy) -> Money;    // §6.6
pub fn throughput(r: &Recipe, inputs: &[Qty], crew: &CrewStats, tech: TechLevel) -> Qty; // §7.4
pub fn interest_accrual(principal: Money, rate_bps: u32, days: u16) -> Money;
pub fn ledger_post(book: &mut Ledger, e: LedgerEntry);                // jedyna droga księgowania
```

**Reguła wiążąca, którą ten WP ustanawia:**
> `sim/macro` **nie zawiera żadnej logiki ekonomicznej.** Zawiera wyłącznie agregację, alokację
> i pętlę czasu. Każda decyzja o cenie, płacy, produkcji, użyteczności i każde zaksięgowanie pieniądza
> przechodzi przez `econ_kernel`. Różnica między mezo a makro to wyłącznie to, czy `softmax_shares`
> jest losowany (mezo: jeden agent wybiera jedną opcję) czy stosowany jako wagi (makro: komórka dzieli
> popyt proporcjonalnie).

To przenosi problem spójności z „testowania podobieństwa dwóch implementacji" na „testowania jednej
implementacji" — i to jest jedyny powód, dla którego test z WP10.4 ma szansę być zielony po roku.

**Podział pracy — uzgodniony i zamknięty.** `sim/economy` należy do M5 i to M5 buduje rdzeń
od pierwszego dnia:

| Funkcja | Kto pisze | Kiedy |
|---|---|---|
| `next_price`, `take_cogs`, `ledger_post`, `purchase_score`, `softmax_shares` | **M5** | w M5, jako rdzeń od razu |
| `wage_bid` | **M7** | wypełnia slot zadeklarowany przez M5 |
| `throughput` | **M6** | wypełnia slot zadeklarowany przez M5 |
| `interest_accrual`, `tax::apply` | M10 | WP10.2 |

**Zasada rdzenia, ustalona przez M5 i wiążąca dla wszystkich:** rdzeń **nie zna LOD, nie zna encji,
nie alokuje** — dostaje liczby i zwraca liczby, a identyfikatory wchodzą wyłącznie jako indeksy
katalogu danych. To jest mocniejsze sformułowanie mojej reguły i zastępuje ją.

Konsekwencja dla mnie jest dokładnie ta, po którą ten pakiet powstał: **makro woła ten sam kod co
mezo**, więc kryterium K2 (dok. 00 §4 / K-5) mierzy **błąd agregacji, a nie rozjazd dwóch
implementacji**. Bez tego cały §7.3 byłby testem podobieństwa dwóch modeli, czyli testem,
który zawsze można „naprawić" dostrajaniem.

**Kryterium ukończenia:** wszystkie testy M5–M7 zielone bez zmian; `cargo tree` pokazuje, że
`sim/macro` zależy od `sim/economy::kernel`, a nie odwrotnie; lint CI zabrania w `sim/macro`
literałów mnożników cenowych i stawek (skrypt grepowy — prymitywny, ale skuteczny).

**Rozmiar: L.**

---

### WP10.1 — `sim/macro`: stan, krok, `lift`/`lower`

**Zależności:** WP10.2.

Cztery rzeczy:
1. `MacroState` / `MacroCell` / `MacroFirm` — struktury z §5.6.
2. `step()` — jeden dzień makro, osiem faz z §5.6.
3. `lift()` / `lower()` / `lower_cell()` — most mezo↔makro z gwarancją zachowania pieniądza co do grosza.
4. `MacroLodPolicy` — progi przełączania LOD, histereza i warunki blokady przejścia (M12 dostarcza
   wywołanie z pętli gry i `SpeedGovernor`, matematykę daje M10).

**Kryterium ukończenia:**
- `lift(lower(s)) == s` na wszystkich agregatach pieniężnych — **tolerancja 0 groszy**, test własnościowy
  na 1000 losowych stanów (proptest).
- `step()` zachowuje sumę pieniądza (emisja − destrukcja) — test własnościowy.
- Dwa przebiegi tego samego seeda → identyczny hash `MacroState` co 100 kroków.
- 7500 kroków makro dla metropolii ≤ 60 s jednowątkowo (criterion).
- `MacroState` metropolii ≤ 8 MB (test asercyjny na `size_of` + pojemnościach) — musi się dać
  sklonować kilkanaście razy dla równoległych „co jeśli" M7.

**Rozmiar: XL.**

---

### WP10.3 — Historia „na sucho" (`DryRunConfig`, etapy D0–D5)

**Zależności:** WP10.1, generator M1–M2 (Etapy 1–8).

Pełny opis etapów w §5.7. Kryterium ukończenia:
- `--dry-run --years 80` na 64 seedach: 100% kończy się światem przechodzącym Etap 10
  (po ≤ 3 rundach `rebalance`), **> 80% bez żadnej rundy naprawczej**.
- Raport zawiera ≥ 50 wpisów kronikarskich z `provenance: DryRun` dla 80 lat.
- Rozwinięcie 400 tys. mieszkańców z makro do pełnego ECS ≤ 20 s.
- Determinizm: ten sam seed → identyczny hash świata po rozwinięciu.

**Rozmiar: L.**

---

### WP10.4 — Test spójności LOD i rozszerzenie balansatora

**Zależności:** WP10.1–10.3.
Opis w §7.3. **Rozmiar: M.**

---

### WP10.5 — Marka: `BrandAffinity` w pamięci mieszkańców

**Zależności:** M3 (magazyn doświadczeń), M5 (decyzja zakupowa), M7 (firma ma produkt).

Sloty marek w zimnym magazynie doświadczeń, leniwy zanik, aktualizacja z doświadczeń, podpięcie
pod `purchase_score` (składniki `w_marka` i `jakość_postrzegana` przestają być stałymi).
Rachunek pamięci: §5.1.

**Kryterium ukończenia:**
- Test asymetrii: kampania podnosząca oczekiwaną jakość o +20 Q wymaga ≥ 7 ekspozycji; jedno
  rozczarowanie o −20 Q kosztuje ≥ 14 pkt afinitetu; odbudowa wymaga ≥ 3 pozytywnych doświadczeń
  o tej samej sile. To jest test jednostkowy na liczbach, nie „obserwacja w symulacji".
- Pamięć: 400 tys. mieszkańców × 16 slotów ≤ 55 MB mierzone (nie szacowane).
- Zanik leniwy: brak systemu iterującego po slotach częściej niż `EveryMonth`.

**Rozmiar: M.**

---

### WP10.6 — Kanały i kampanie (`AdCampaign`, `Reach`)

**Zależności:** WP10.5, M4 (zdarzenia przejazdu po krawędziach), M2 (indeks przestrzenny).

Osiem kanałów z §5.2. Kluczowa decyzja projektowa: **`Reach` nie jest zbiorem, tylko strumieniem
zdarzeń ekspozycji**. Nie budujemy indeksu „krawędź → mieszkańcy, którzy nią jeżdżą" (400 tys. × ~60
krawędzi to 96 MB indeksu, który trzeba utrzymywać przy każdej zmianie trasy). Zamiast tego billboard
podpina się pod istniejący strumień przejazdów mezo z M4 i próbkuje go deterministycznie.

**Kryterium ukończenia:**
- Billboard przy krawędzi o dobowym potoku 8000 przejazdów i `notice_rate = 12%` dostarcza
  960 ± 30 ekspozycji dziennie, powtarzalnie dla seeda.
- Ekspozycje trafiają do **mieszkańców faktycznie tamtędy jeżdżących** — test: przeniesienie
  billboardu na krawędź w innej dzielnicy zmienia rozkład dzielnic odbiorców zgodnie z macierzą dojazdów.
- Zero nowych indeksów przestrzennych — audyt kodu.
- Koszt kampanii księgowany przez `ledger_post`, nie bezpośrednio.

**Rozmiar: L.**

---

### WP10.7 — Media jako firmy

**Zależności:** WP10.6, M3 (graf relacji i plotka), M8 (strumień zdarzeń).
**Kryterium ukończenia:** gazeta o czytelnictwie 35% w Śródmieściu i 8% na Wzgórzach publikuje tekst
o strajku; po 3 dniach gry znajomość zdarzenia wśród mieszkańców Śródmieścia > 40%, na Wzgórzach < 15%,
a rozkład jest zgodny z czytelnictwem ± 5 pkt. Wiarygodność tytułu moduluje siłę aktualizacji opinii.
**Rozmiar: M.**

---

### WP10.8 — R&D: `TechTree`, punkty badawcze, `Patent`, licencje

**Zależności:** M7 (dział, stanowiska, budżet), dane `data/tech/`.
**Kryterium ukończenia:** firma z 4 badaczami (umiejętność 60) i budżetem 50 tys./mies. odkrywa węzeł
o koszcie 1200 RP w 9 ± 1 miesiącu gry, deterministycznie. Odkrycie przed rokiem „światowym" daje patent;
po roku „światowym" węzeł jest dostępny bez patentu po obniżonym koszcie. Licencja jest `ContractId`
z M6, nie nowym mechanizmem. Walidator grafu `data/tech/` w CI (każdy prereq istnieje, brak cykli,
każdy `NewGood` istnieje w `data/goods/`).
**Rozmiar: L.**

---

### WP10.9 — Nowe kategorie produktów i zmiana potrzeb

**Zależności:** WP10.8, M8 (`sim/events`), M3 (potrzeby).
**Kryterium ukończenia:** odblokowanie kategorii „telefon komórkowy" w roku epoki rzeczywiście zmienia
koszyk potrzeby „kontakt społeczny" i tworzy nowy łańcuch dostaw (walidator grafu towarów zielony
po zmianie). Mieszkańcy z wysoką otwartością kupują pierwsi — mierzalne w rozkładzie.
**Rozmiar: M.**

---

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

### WP10.13 — Relacje międzyfirmowe

**Zależności:** M6 (kontrakty), M7 (firmy AI), M8 (regulator).
**Kryterium ukończenia:** `SupplierRelation.trust` rośnie z historii terminowych dostaw i realnie
zmienia wybór dostawcy (firma płaci +3% stałemu dostawcy zamiast szukać taniej — widoczne w
`DecisionReason`). Kartel obniża wolumen i podnosi cenę; hazard wykrycia rośnie z odchyleniem ceny
od benchmarku; po wykryciu kara i uderzenie w markę wszystkich członków.
**Rozmiar: L.**

---

### WP10.14 — Związki zawodowe i strajki

**Zależności:** M7 (HR, płace, księgowość), M3 (graf relacji).
**Kryterium ukończenia:** zakład o marży 35% płacący 20% poniżej mediany miejskiej dla roli, z gęstą
siecią relacji między pracownikami, formuje związek w ciągu 3–9 miesięcy gry. Strajk zatrzymuje
produkcję zakładu (nie firmy), uruchamia kary z kontraktów M6 u odbiorców i jest widoczny w mediach.
Fundusz strajkowy wyczerpuje się i wymusza rozstrzygnięcie — brak strajków wiecznych (test: 100
strajków w balansatorze, mediana czasu trwania 4–21 dni, maksimum < 90 dni).
**Rozmiar: M.**

---

### WP10.15 — Kroniki i wyjaśnialność

**Zależności:** M9 (podsystem kroniki), wszystkie WP tej fazy.
Przekrojowy. Każda nowa decyzja ma `DecisionReason` (dok. 00 §7). Nowe warianty `ChronicleEvent` z §5.9.
**Kryterium ukończenia:** 100% nowych decyzji ma czytelny powód w karcie inspekcji; audyt ręczny
20 losowych wpisów kronikarskich pod kątem zrozumiałości dla gracza.
**Rozmiar: S.**

---

### WP10.16 — Determinizm i hash stanu

Przekrojowy, Definition of Done fazy. Nowe warianty `StreamId`, dopisanie nowych komponentów do
funkcji haszującej, testy dwóch przebiegów. **Rozmiar: S.**

---

## 5. Projekt techniczny

### 5.1 Marka — struktury i rachunek pamięci

**Problem.** Marka ma być zbiorem afinitetów w pamięci konkretnych mieszkańców (§7.6). Naiwna
implementacja to macierz `mieszkańcy × marki`:

| Wariant | Rachunek | Wynik | Udział w budżecie 6 GB (§17.7) |
|---|---|---|---|
| A — macierz gęsta | 400 000 × 4096 marek × 2 B | **3,28 GB** | 55% — odrzucony |
| B — mapa rzadka per mieszkaniec (`BTreeMap`) | 400 000 × ~14 wpisów × (8 B danych + ~40 B węzła) | ~270 MB + fragmentacja | 4,5% — odrzucony (alokacje, brak lokalności) |
| **C — tablica K slotów o stałym rozmiarze** | **400 000 × 16 × 8 B** | **51,2 MB** | **0,85% — przyjęty** |

**Wariant C — układ pola:**

```rust
/// Jeden slot pamięci marki. Dokładnie 8 B, bez wyrównania, SoA-friendly.
#[repr(C)]
pub struct BrandAffinity {
    pub brand: BrandId,          // u16 — marka (firma może mieć do 4 marek)
    pub affinity: i8,            // -100..=100 — sympatia; to jest „marka"
    pub expected_quality: Q,     // u8 0..=100 — czego się spodziewam
    pub awareness: u8,           // 0..=100 — jak mocno ją znam (§5.7)
    pub last_touch_day: u16,     // dzień gry; bazuje na epoce świata, ~179 lat zakresu
    pub source: TouchSource,     // u8: Experience | Ad | Rumor | Media | Owned — dla wyjaśnialności
}

pub const BRAND_SLOTS: usize = 16;
pub struct BrandSlots(pub [BrandAffinity; BRAND_SLOTS]); // 128 B
```

**Gdzie to mieszka.** §17.7 daje mieszkańcowi ~400 B stanu gorącego plus **osobny, kompresowany
magazyn doświadczeń (ostatnie 32 wpisy)**. `BrandSlots` idzie do tego drugiego, nie do stanu gorącego —
bo jest czytany przy decyzji zakupowej i ekspozycji (kilkanaście razy na dobę gry na mieszkańca,
przez DES), a nie co tick. Stan gorący pozostaje 400 B. Dostęp: ten sam mechanizm stronicowania,
który M3 zbudował dla doświadczeń; M10 dokłada tylko drugą sekcję rekordu.

**Sumarycznie:**

```
sloty marek:              400 000 × 128 B          =  51,2 MB
prior dzielnicowy:        40 dzielnic × 4096 × 1 B =   0,16 MB   (fallback „coś słyszałem")
agregat „siła marki":     4096 marek × 16 B        =   0,07 MB   (tylko dla UI, EveryDay)
indeks kampanii:          ≤ 2000 kampanii × 64 B   =   0,13 MB
──────────────────────────────────────────────────────────────
razem                                              ≈  51,6 MB  =  0,86% budżetu 6 GB
```

**Wypieranie slotów.** Przy 17. marce wypadamy slot o najmniejszej istotności:
`salience = |affinity| × 4 + awareness`, przy remisie niższy `BrandId` (determinizm). Marka
pracodawcy mieszkańca i marka sklepu, w którym był ≥ 8 razy, są przypięte (`source: Owned`)
i nie podlegają wypieraniu — bez tego lojalność z §5.1 znika w szumie reklamowym.

**Zanik jest leniwy.** Nie ma systemu iterującego po 6,4 mln slotów. Przy odczycie liczymy
`decay(days_since_touch)` z tablicy stałych (wyszukanie, nie `exp()`), afinitet i znajomość
zbiegają do zera i do priora dzielnicowego. Jedyny przebieg iterujący to `EveryMonth` kompaktowanie
slotów wygasłych do zera — i on tylko zwalnia miejsce.
> *ponytail: zanik liczony przy odczycie, O(1). Gdyby profil kiedyś pokazał, że odczyty dominują,
> przenieść na przebieg wsadowy `EveryWeek` po chunkach — ale nie wcześniej.*

**Aktualizacja — asymetria wymagana przez §7.6.** Arytmetyka całkowita, stałe w 1/256:

```rust
// Ekspozycja reklamowa (kanał c, przekaz twierdzi jakość `claim`)
awareness  = min(100, awareness + AWARENESS_GAIN[c]);              // 1..12 pkt
expected_quality += ((claim - expected_quality) * AD_LEARN) >> 8;  // AD_LEARN = 24  (~9%)
// afinitet NIE zmienia się od samej reklamy — reklama tworzy oczekiwanie, nie sympatię

// Doświadczenie (zakup, faktyczna jakość `actual`)
let d = actual as i16 - expected_quality as i16;
affinity += if d >= 0 { (K_UP   * d) >> 8 }     // K_UP   =  64  (0,25)
            else       { (K_DOWN * d) >> 8 };   // K_DOWN = 192  (0,75) → asymetria 3:1
expected_quality += ((actual - expected_quality) * EXP_LEARN) >> 8; // EXP_LEARN = 96 (~37%)
```

Konsekwencja liczbowa (to jest treść testu z WP10.5): podniesienie oczekiwań o +20 Q wymaga
~8 ekspozycji; jedno rozczarowanie o −20 Q zabiera 15 pkt afinitetu; odbudowa tych 15 pkt wymaga
trzech doświadczeń o tej samej sile in plus. **Nadmiarowa obietnica w reklamie jest samokarząca** —
podnosi `expected_quality`, a więc powiększa `d < 0` przy każdym kolejnym zakupie. Gracz, który
reklamuje tandetę, niszczy sobie markę własną kampanią, i widzi to w karcie mieszkańca.

**Plotka (§5.7).** Przekaz przez graf relacji nie kopiuje afinitetu, tylko przybliża:

```
odbiorca.affinity += (nadawca.affinity - odbiorca.affinity) * waga_relacji * wiarygodność / 65536
odbiorca.awareness = max(odbiorca.awareness, nadawca.awareness / 2)
```
Plotka nigdy nie ustawia `expected_quality` powyżej wartości nadawcy — nie da się „nakręcić"
oczekiwań pętlą plotek.

### 5.2 Kampanie i zasięg

```rust
pub struct AdCampaign {
    pub id: CampaignId,
    pub advertiser: FirmId,
    pub brand: BrandId,
    pub channel: AdChannel,
    pub claim: Q,                 // deklarowana jakość — podnosi expected_quality odbiorców
    pub budget: Money,            // i64, grosze
    pub spent: Money,
    pub window: (SimMinute, SimMinute),
    pub audience: AudienceFilter, // filtr (dzielnice, klasy, wiek) — NIE lista odbiorców
    pub metrics: CampaignMetrics, // ekspozycje, przyrost znajomości, sprzedaż przypisana — dla UI M9
}

pub enum AdChannel {
    Billboard   { edge: EdgeId, side: u8, notice_rate_bps: u16 },
    Press       { outlet: FirmId, slot: AdSlot },
    Radio       { outlet: FirmId, daypart: Daypart },
    Tv          { outlet: FirmId, daypart: Daypart },
    Leaflet     { origin: SiteId, radius_m: u16 },
    InStorePromo{ site: SiteId, discount_bps: u16 },
    Sponsorship { event: EventId },
    Pr          { topic: PrTopic },     // reakcja na zdarzenie — wchodzi w plotkę, nie w ekspozycję
}

/// Zasięg jest PLANEM i POMIAREM, nie zbiorem odbiorców.
pub struct Reach {
    pub exposures_planned_daily: u32,
    pub exposures_actual_daily: u32,
    pub cost_per_mille: Money,
}
```

**Skąd bierze się zasięg billboardu (§7.6: „zasięg to mieszkańcy, którzy tamtędy jeżdżą").**
M4 w mezo już produkuje przejazdy po krawędziach grafu. System reklamy **subskrybuje ten strumień**
i próbkuje go rezerwuarowo:

```rust
// EveryHour, tylko dla krawędzi z aktywnym billboardem (typowo < 200 krawędzi)
for traversal in traffic.edge_traversals(edge) {
    if rng(seed, StreamId::AdNotice, campaign.index(), tick).next_bps() < notice_rate_bps {
        expose(traversal.citizen, campaign);
    }
}
```

Żadnego indeksu „krawędź → mieszkańcy". Żadnego utrzymywania go przy przeplanowaniu tras.
Koszt: O(przejazdy po krawędziach z billboardami), czyli ~200 krawędzi × ~500 przejazdów/h = 100 tys.
testów na godzinę gry — poniżej progu zauważalności.
> *ponytail: próbkowanie strumienia zamiast indeksu odwrotnego. Sufit: przy > 2000 billboardów
> w mieście koszt rośnie liniowo — wtedy agregować per krawędź (liczba ekspozycji zamiast listy)
> i losować odbiorców z potoku. Nie wcześniej.*

Prasa/radio/TV: `MediaOutlet.readership[district]` daje prawdopodobieństwo kontaktu; próbkujemy
mieszkańców dzielnicy bez materializowania listy (odwrotna dystrybuanta po kohorcie).
Ulotki: indeks przestrzenny z M2 (`engine/spatial`), promień, bez nowych struktur.
Promocje w sklepie: ekspozycja przy transakcji — darmowa, bo mieszkaniec i tak tam jest.
Sponsoring: uczestnicy zdarzenia z M8.
PR: nie tworzy ekspozycji — wstrzykuje `Rumor` do grafu relacji z §5.7.

### 5.3 Media

```rust
pub struct MediaOutlet {
    pub firm: FirmId,
    pub kind: MediaKind,                   // Gazeta | Radio | Tv | Portal
    pub readership: Vec<(DistrictId, Q)>,  // udział czytelnictwa per dzielnica
    pub credibility: Q,                    // moduluje siłę aktualizacji opinii u odbiorcy
    pub bias: EditorialBias,               // prorynkowy | prospołeczny | sensacyjny | lokalny
    pub inventory_daily: u16,              // slotów reklamowych na dobę
    pub slot_price: Money,
}
```

Media są **firmami z M7** z dodatkowym komponentem. Sprzedaż powierzchni idzie przez istniejący
mechanizm ofert z M5/M6 (`OfferId`) — nie budujemy drugiego rynku. Redakcja co dobę wybiera
`n = 3..8` zdarzeń ze strumienia M8 wg wartości informacyjnej (skala zdarzenia × zgodność z `bias`)
i publikuje `Story`, która wchodzi do grafu plotki z fan-outem ważonym czytelnictwem.

Odbiorca aktualizuje opinię proporcjonalnie do `credibility × jego zaufanie do tytułu`. Tytuł, który
publikował rzeczy niezgodne z obserwacją odbiorcy, traci u niego zaufanie — wiarygodność jest
per-para, tak jak marka. Reużywamy do tego ten sam slot `BrandAffinity` (tytuł ma `BrandId`) —
zero nowych struktur.

### 5.4 R&D, technologie, epoki

```rust
pub struct TechTree { pub nodes: Vec<TechNode>, pub by_branch: Vec<Vec<TechId>> }

pub struct TechNode {
    pub id: TechId,
    pub branch: BranchId,
    pub prereqs: SmallVec<[TechId; 4]>,
    pub cost_rp: u32,                 // punkty badawcze, milipunkty w akumulacji
    pub world_year: i32,              // rok, od którego technologia jest „światowa"
    pub effects: SmallVec<[TechEffect; 4]>,
}

pub enum TechEffect {
    QualityBonus   { good: GoodId, delta_q: i8 },
    CostReduction  { recipe: RecipeId, bps: u16 },
    NewRecipe      (RecipeId),
    NewGood        (GoodId),           // §11.3 — nowa kategoria, zmienia koszyk potrzeb
    MachineUpgrade { machine: MachineId, throughput_bps: u16, failure_bps: i16 },
    ProductFeature { good: GoodId, feature: FeatureId },
}

pub struct Patent {
    pub tech: TechId,
    pub owner: FirmId,
    pub granted: SimMinute,
    pub expires: SimMinute,            // 20 lat gry
    pub license_policy: LicensePolicy, // Zamknięty | Otwarty{opłata} | Wybiórczy{lista}
}
```

**Postęp:** `rp_per_day = Σ_badacz f(umiejętność, energia, nastrój) × jakość_laboratorium ×
min(1, budżet_materiałowy / próg)` — wszystko całkowitoliczbowe, milipunkty. Akumulacja na wybranym
węźle. Element losowy jest **jeden**: przełom (`StreamId::RnDBreakthrough`) skraca pozostały koszt
o 10–40% z małym prawdopodobieństwem dziennym. Brak losowego „nie udało się" — frustrujące i nieczytelne.

**Trzy tryby dostępu do technologii:**
1. **Odkrycie przed `world_year`** → patent na 20 lat. Przewaga i źródło przychodu z licencji.
2. **Odkrycie po `world_year`** → bez patentu, koszt RP obniżony o 60% (wiedza jest w obiegu).
3. **Licencja** → `ContractId` z M6 (opłata wstępna + royalty w bps od przychodu z produktu).
   Zero nowego mechanizmu umów.

**Epoki (§11.3).** `data/epochs/*.ron` (istniejący katalog z dok. 00 §5) mapuje rok → zbiór
`world_year` dla węzłów. Nowa kategoria produktu (`NewGood`) uruchamia migrację koszyka potrzeb:
towar dopisuje się do koszyka wskazanej potrzeby z wagą początkową i **przesuwa wagi substytutów**.
To jest jedyne miejsce, w którym dane symulacji zmieniają się w trakcie partii — musi przejść przez
walidator grafu towarów (dok. 00 §5), inaczej odrzucamy zmianę i logujemy błąd danych.

**Nowa kategoria oznacza nowy łańcuch.** `NewGood` bez receptury i bez importu to błąd danych
wykrywany w CI, nie w runtime.

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

### 5.6 `sim/macro` — stan i krok

```rust
/// Komórka = (dzielnica × klasa społeczna). 40 dzielnic × 6 klas = 240 komórek.
pub struct MacroCell {
    pub key: (DistrictId, ClassId),
    pub citizens: Vec<CitizenSeed>,     // TOŻSAMOŚĆ — patrz §5.8
    pub age_hist: [u32; 18],            // kohorty 5-letnie
    pub cash: Money, pub deposits: Money, pub debt: Money,
    pub wealth_q: [Money; 4],           // min, q1, q3, max — do rozwinięcia rozkładu
    pub labor: [u32; N_ROLES],          // liczba zdolnych do roli
    pub skill_sum: [u32; N_ROLES],      // suma umiejętności (średnia = skill_sum/labor)
    pub employed: u32, pub unemployed: u32,
    pub need_sat: [Q; N_NEEDS],
    pub demand: MacroStock,             // popyt zrealizowany w ostatnim kroku
}

/// Firmy NIE są agregowane — zachowują tożsamość i są symulowane indywidualnie także w makro.
pub struct MacroFirm {
    pub id: FirmId, pub district: DistrictId, pub branch: BranchId,
    pub capital: Money, pub debt: Money,
    pub stock: MacroStock,              // jednostka natywna towaru — patrz niżej
    pub capacity_daily: Qty, pub utilization_bps: u16,
    pub employees: u32, pub wage_bill: Money,
    pub price: SparseVec<GoodId, Money>,
    pub brand_stock: i32,               // skalarny surogat marki (patrz niżej)
    pub tech: TechLevel,
    pub suppliers: SmallVec<[(GoodId, FirmId); 8]>,   // wynik dry-runu: „stali dostawcy"
}

pub struct MacroState {
    pub tick: Tick, pub day: u32,
    pub cells: Vec<MacroCell>,          // posortowane po kluczu — determinizm iteracji
    pub firms: Vec<MacroFirm>,          // posortowane po FirmId
    pub commute: CommuteMatrix,         // dzielnica × dzielnica × koszt (z §17.6, statyczna w makro)
    pub epoch: EpochState,
    pub ledger: Ledger,                 // suma pieniądza, ten sam typ co mezo
}
```

**Ziarno agregacji jest zamknięte z obu stron — i musi skalować się z populacją.**

Ziarna nie wolno **zagęścić**, bo poniżej ~500 osób na komórkę błąd zastąpienia losowania wartością
oczekiwaną przestaje się znosić i kryterium K2 (≤ 0,5%/miesiąc, §7.3) staje się nieosiągalne
ze statystyki, nie z implementacji. Nie wolno go też **zgrubić**, bo komórka przestaje odpowiadać
czemukolwiek, co gracz widzi na mapie. To jest ograniczenie, o którym łatwo zapomnieć przy pierwszej
prośbie o „większą rozdzielczość makro" — nie ma jej i nie będzie.

Stąd wniosek, którego stałe ziarno 40 × 6 nie spełnia. PRD §4.1 dopuszcza miasta od 20 tys.,
a §4.3 daje 10–40 dzielnic. Rachunek osób na komórkę przy 6 klasach:

| Rozmiar (§4.1) | Populacja | Dzielnic | Komórek (×6 klas) | Osób/komórkę | K2 osiągalne? |
|---|---|---|---|---|---|
| małe | 20 000 | 10 | 60 | 333 | **nie** |
| małe (górny) | 40 000 | 15 | 90 | 444 | **nie** |
| średnie | 60 000 | 20 | 120 | 500 | granica |
| średnie (górny) | 120 000 | 25 | 150 | 800 | tak |
| duże | 150 000 | 30 | 180 | 833 | tak |
| duże (górny) | 300 000 | 35 | 210 | 1 428 | tak |
| metropolia | 400 000 | 40 | 240 | 1 667 | tak |

Dwa pierwsze wiersze łamią własny kontrakt fazy. Dlatego ziarno **nie jest stałą**:

```rust
/// Liczba klas w ziarnie dobierana tak, by na komórkę przypadało ≥ MIN_CELL_POP osób.
pub const MIN_CELL_POP: u32 = 500;
pub fn cell_grain(pop: u32, districts: u16) -> ClassGrain;   // Classes6 | Classes3 | Classes2
```

Klasy łączymy parami po sąsiadujących przedziałach statusu (§5.4), nigdy losowo:
`Classes3` = (niższa + robotnicza), (niższa średnia + wyższa średnia), (wyższa + elita).
Dla miasta 20 tys. daje to 10 × 3 = 30 komórek i 666 osób na komórkę — kontrakt wraca do zakresu.
Odwzorowanie klasy na komórkę jest funkcją czystą, więc `lift()` i `lower()` działają bez zmian.

Test `macro_grain_meets_min_cell_pop` sprawdza to dla wszystkich siedmiu rozmiarów z §4.1 —
inaczej K2 przechodzi w CI na metropolii i cicho pada u gracza, który wybrał małe miasto.
> *ponytail: `ClassGrain` ma trzy warianty, nie parametr ciągły. Ziarno ciągłe wymagałoby
> przemapowania klas przy każdej zmianie populacji; trzy progi wystarczą na zakres 20 tys.–400 tys.*

**Jednostka towaru w makro: natywna, nigdy przeliczana.**

```rust
/// Ilość towaru w jednostce NATYWNEJ danego `GoodId`, zadeklarowanej w `data/goods/`.
/// Jeden i64, nie para — dwie liczby mogłyby się rozjechać, a wtedy zachowanie masy (K1) pada.
pub struct MacroStock(pub SparseVec<GoodId, i64>);
```

M6 (`sim/supply`) prowadzi towary masowe (ropa, zboże, cement) w `Mass` (gramy), a sztukowe w `Qty`
(milisztuki), i to `data/goods/` rozstrzyga, która jednostka jest wiodąca dla danego towaru.
`MacroStock` trzyma **dokładnie tę liczbę, którą trzyma M6**, i ani `lift()`, ani `lower()` jej nie
przelicza. Konsekwencja jest celowa: skoro nie ma konwersji, nie ma zaokrąglenia, a zachowanie masy
co do grama (K1, §7.3) wynika z tego konstrukcyjnie, a nie z ostrożnej arytmetyki.
> *ponytail: jedna liczba w jednostce z katalogu zamiast pary `(Mass, Qty)`. Para wymagałaby
> współczynnika przeliczeniowego per towar, utrzymywania go w zgodzie z M6 i testu na rozjazd —
> czyli trzech rzeczy zamiast zera.*

**Czego makro nie odtworzy: łańcucha pochodzenia partii.** `MacroStock` zna ilość, nie zna partii.
Gubi to, co M6 uważa w towarze za istotne: jakość, datę przydatności, koszt nabycia i markę.
Przy `lower()` partie **nie są odtwarzane — są generowane na nowo** z agregatu, z jakością i datą
przydatności wylosowanymi z rozkładu komórki (ten sam mechanizm kwantylowy co w §5.8).

Skutek dla „od pola do półki" (PRD §14.4): **ślad pochodzenia rwie się na granicy makro.**
Partia wygenerowana przez `lower()` dostaje `provenance: FromAggregate { district, last_supplier }`
i trace kończy się tam, a nie na złożu. Trzy rzeczy czynią to akceptowalnym:
1. `MacroFirm.suppliers` zachowuje **jeden skok wstecz** — a to jest właśnie ten wynik dry-runu,
   o który chodzi w §4.2 Etap 9 („relacje między firmami — stali dostawcy").
2. Partie z historii „na sucho" dostają `provenance: DryRun` i UI pokazuje je jako historię,
   a nie jako sfabrykowany łańcuch do złoża. Ten sam chwyt co przy wpisach kronikarskich —
   **lepiej przyznać się do braku danych niż zmyślić wiarygodny łańcuch.**
3. W trybie 50× (M12) ślad rwie się tylko dla towarów, które przeżyły całe okno przyspieszenia;
   wszystko wyprodukowane po powrocie do mezo ma pełny łańcuch.

To jest realna utrata funkcjonalności i jest wpisana jako D10 w §9 — nie da się jej naprawić
bez trzymania partii w makro, co przekreśla sens agregacji.

**Firmy zachowują tożsamość również w makro.** 3000 firm × ~400 B to 1,2 MB i kilkanaście operacji
dziennie każda — agregowanie ich byłoby oszczędnością bez pokrycia, a kosztowałoby dokładnie te
rzeczy, po które robimy dry-run: relacje dostawców, zadłużenie konkretnych firm, historię dzielnic.
Agregowani są **wyłącznie mieszkańcy**.

**Marka w makro.** Sloty per mieszkaniec nie istnieją w stanie uśpionym. `MacroFirm.brand_stock`
to skalarna wartość oczekiwana agregatu afinitetów; przy `lower` rozwijana do slotów proporcjonalnie
do znajomości w dzielnicy. Utrata informacji jest akceptowana i **jawnie udokumentowana**:
przejście mezo→makro→mezo gubi indywidualne historie marek, zachowuje rozkład.

**Krok makro — jeden dzień, osiem faz, wszystkie przez `econ_kernel`:**

| # | Faza | Częstotliwość | Co robi |
|---|---|---|---|
| 1 | Demografia | co 30 kroków | starzenie kohort, urodzenia, zgony, migracja z tablic zależnych od `need_sat` i bezrobocia |
| 2 | Rynek pracy | co krok | podaż roli w komórce × dostępność z `CommuteMatrix` vs popyt firm; płaca z `wage_kernel`, zmiana ograniczona do ±2%/dzień |
| 3 | Produkcja | co krok | `throughput()` per firma, ograniczone wejściami i załogą |
| 4 | Rynek dóbr | co krok | popyt komórki z potrzeb; alokacja do firm przez `softmax_shares(purchase_score(...))` — **ten sam kernel co §6.4** |
| 5 | Ceny | co krok | `next_price()` per firma per towar — **ten sam kernel co §6.3** |
| 6 | Finanse | co krok | odsetki, raty, podatki (stawki z M8), dywidendy, test wypłacalności, bankructwa |
| 7 | Relacje | co krok | dostawca wybrany w fazie 4 dla B2B podnosi `SupplierRelation.trust` |
| 8 | Zdarzenia | co krok | zredukowany katalog §11.2 z tymi samymi hazardami co w mezo |

`step()` jest jednowątkowy. 240 komórek × ~60 towarów + 3000 firm to ~10⁵ operacji na krok —
równoległość byłaby tu tylko źródłem niedeterminizmu bez zysku.

### 5.7 Historia „na sucho" — etapy

```rust
pub struct DryRunConfig {
    pub seed: u64,
    pub years: u16,                    // 30..=100
    pub start_year: i32,               // rok_startu_gry − years
    pub epoch_track: EpochTrackId,
    pub profile: EconProfile,          // §4.1
    pub target: PopTarget,             // §4.1 — docelowa wielkość NA KONIEC dry-runu
    pub step_days_early: u8,           // 6  — lata 1..N−5 (60 kroków/rok przy kalendarzu 360 dni)
    pub step_days_late: u8,            // 1  — ostatnie 5 lat
    pub max_rebalance_rounds: u8,      // 3
    pub chronicle: bool,
}
```

| Etap | Nazwa | Co jest symulowane | Co jest agregowane | Wynik |
|---|---|---|---|---|
| **D0** | Zasiew | — | — | Z Etapów 4–8 generatora budujemy `MacroState` w `start_year`: miasto **mniejsze** (populacja skalowana krzywą wzrostu epoki, typowo 30–50% docelowej), mniej firm, mniej dzielnic zabudowanych. Parcele i drogi z Etapów 3–5 istnieją już w pełni — teren nie jest symulowany. |
| **D3** | Kronika | — | — | Każde zdarzenie o skali > progu → `ChronicleEvent { provenance: DryRun }`: upadek huty, gentryfikacja Starego Portu, fortuna rodziny Nowaków, trzy recesje. To jest tło narracyjne, które gracz czyta w M9. |
| **D4** | Rozwinięcie (`lower`) | — | — | `MacroState` → pełny świat ECS. Mechanizm w §5.8. |
| **D5** | Weryfikacja i naprawa | — | — | Etap 10 PRD. Kryteria i procedura naprawy niżej. |

**D5 — weryfikacja (§4.2 Etap 10: „żaden rynek nie jest w stanie nierównowagi > 30%").**

Nierównowagę definiujemy jako **średnią z ostatnich 30 dni dry-runu**, nie z jednego dnia — pojedynczy
dzień jest zaszumiony i dawałby fałszywe alarmy:

```
imbalance(g) = |podaż_dzienna(g) − popyt_dzienny(g)| / max(podaż_dzienna(g), popyt_dzienny(g))
```

Bramki Etapu 10 (wszystkie muszą przejść):

| # | Kryterium | Próg |
|---|---|---|
| 1 | `imbalance(g)` dla każdego `GoodId` | ≤ 30% |
| 2 | Pokrycie zapasem każdego konsumowanego towaru | ≥ 3 dni |
| 3 | Każda firma: ≥ 1 pracownik i ≥ 1 dostawca dla każdego wejścia receptury | 100% |
| 4 | Bezrobocie | 3%–15% |
| 5 | Mediana `dług/aktywa` firm | 0,10–0,60 |
| 6 | Odsetek firm niewypłacalnych w dowolnej dzielnicy | < 40% |
| 7 | Gini majątku GD | 0,25–0,45 |
| 8 | Koszyk podstawowy / mediana dochodu GD | 0,25–0,55 |
| 9 | Każdy mieszkaniec ma dom; grafy dróg spójne | 100% (z Etapów 4–6, nie z makro) |

**Naprawa, nie odrzucenie seeda.** Odrzucenie 40% seedów byłoby porażką generatora i wściekłością
gracza, który wybrał seed. Do 3 rund `rebalance()`:

1. **Zapasy:** dosypanie do minimum pokrycia po koszcie z księgowaniem (nie z powietrza — jako import
   zaciągnięty w ostatnim miesiącu, z długiem).
2. **Ceny:** przesunięcie do punktu równowagi logitowej wyliczonego z `softmax_shares` — analitycznie,
   jednym krokiem Newtona, nie iteracyjnie.
3. **Brakujący dostawca:** dopisanie importera z §6.2 warstwa 3 zamiast tworzenia firmy z niczego.
4. **Zadłużenie:** redukcja poprzez zdarzenie restrukturyzacji, zapisane w kronice.

Po 3 nieudanych rundach: regeneracja z `seed + 1` i jawny komunikat w logu. Cel w CI (balansator,
64 seedy): **100% seedów przechodzi, > 80% bez żadnej rundy naprawczej.** Spadek poniżej 80% jest
sygnałem, że parametry gospodarki się rozjechały — to jest właściwy wczesny alarm dla całego projektu,
nie tylko dla M10.

### 5.8 Tożsamość w stanie uśpionym i rozwinięcie makro→mezo

To jest sedno techniczne fazy. §17.4 wymaga: „agregaty per dzielnica × klasa, **z zachowaniem
tożsamości** (tylko stan uśpiony)" oraz „przejście makro→mezo odtwarza indywidualne stany
z zachowanych tożsamości (deterministycznie z seedu + agregatu)".

**Czym jest uśpiony mieszkaniec.** Trzema rzeczami i niczym więcej:

```rust
pub struct CitizenSeed {
    pub birth_index: u32,   // globalnie unikalny, monotoniczny numer narodzin/przybycia
    pub cell: u16,          // indeks MacroCell, do której należy
}
```

8 bajtów. 400 tys. mieszkańców = **3,2 MB**. Czyli: tożsamość jest zachowana **dosłownie i w całości** —
wiemy dokładnie, kto istnieje i do której komórki należy. Nie jest zachowany **stan indywidualny**
(gotówka, umiejętność w roli, nastrój, pamięć, sloty marek) — on jest odtwarzany.

Trwały identyfikator i cała osobowość mieszkańca wywodzą się czysto:
`personality(world_seed, birth_index)` → wszystkie cechy stałe z §5.1 (ambicja, oszczędność, lojalność…),
imię, nazwisko, płeć, data urodzenia. **Cechy stałe nigdy nie muszą być przechowywane w makro** —
są funkcją seeda. To jest jedyne miejsce, gdzie determinizm z dok. 00 §3 daje nam oszczędność
zamiast kosztu.

**`lower()` — rozwinięcie agregatu do jednostek, krok po kroku.** Dla każdej komórki i każdej
wielkości X (gotówka, majątek, umiejętność w roli r, zaspokojenie potrzeby n):

1. Weź z komórki: `suma` (i64) oraz kwantyle `wealth_q` (min, q1, q3, max).
2. Ustal **porządek rangowy**: posortuj `citizens` po `hash(world_seed, StreamId::MacroLower,
   birth_index, quantity_id)`. Porządek jest funkcją czystą — niezależny od kolejności iteracji,
   od wątków, od historii wstawień do wektora.
3. Przypisz wartości z **kwantylowej funkcji odwrotnej** dopasowanej do (suma, kwantyle):
   log-normalna dla pieniądza i majątku, beta dla skal `Q`. Dopasowanie dwuparametrowe,
   rozwiązywane analitycznie z q1/q3 — bez iteracji.
4. **Korekta reszty:** po zaokrągleniu do i64 licz `r = suma − Σ przypisanych` i dodaj `r`
   do pierwszego wg posortowanego klucza (dok. 00 §2, zasada podziału kwoty).

Punkt 4 daje twardą gwarancję: **`lower()` zachowuje pieniądz co do grosza**. Nie „w przybliżeniu",
nie „z tolerancją" — dokładnie. Odwrotnie `lift()` sumuje po posortowanym kluczu i wyznacza kwantyle.
Stąd kontrakt testowalny z tolerancją 0:

```
lift(lower(s)) == s     na wszystkich polach Money
```

**Czego `lower` nie odtwarza i dlaczego to jest w porządku.** `lower` odtwarza **rozkład**, nie
konkretne życiorysy. Anna Wiśniewska po rozwinięciu ma tę samą tożsamość, osobowość, wiek i rodzinę,
ale jej saldo bankowe jest losowaniem z rozkładu jej komórki, a nie sumą jej faktycznych 80 lat
transakcji — bo tych transakcji nie było. To jest cena makro i jest zamierzona.

**Graf relacji w uśpieniu: wyrzucany, nie przechowywany.** Graf z §5.1 nie jest stanem niezależnym —
jest funkcją struktury: rodzina (jawna, mało encji), sąsiedztwo (parcela), współpraca (zakład),
plus „stare przyjaźnie" generowane z seeda pary. Przy `lower` odtwarzamy go z tych czterech
generatorów. Oszczędność w trybie 50× (M12) jest duża, a informacja utracona — nieobserwowalna,
bo w makro nikt po tym grafie nie chodzi.

**Rodziny i gospodarstwa domowe zachowują tożsamość jawnie.** GD jest jednostką ekonomiczną (§5.2),
jest ich ~150 tys. przy 400 tys. mieszkańców, a ich majątek i mieszkanie to rzeczy, których rozkład
nie wystarcza (mieszkanie jest konkretną parcelą). `HouseholdId`, skład, adres i majątek GD są trzymane
jawnie także w makro: 150 tys. × 48 B = 7,2 MB. **To jest jedyne odstępstwo od agregacji mieszkańców
i jest świadome** — bez niego „majątki rodzin" z §4.2 Etap 9 nie mają nośnika.

### 5.9 Relacje międzyfirmowe, związki — struktury

```rust
pub struct SupplierRelation {
    pub buyer: FirmId, pub supplier: FirmId, pub good: GoodId,
    pub since: SimMinute, pub volume_cum: Qty,
    pub trust: Q,                       // z historii terminowości i jakości
    pub discount_bps: u16, pub priority: u8,
    pub exclusivity: Option<Exclusivity>,
}

pub struct Cartel {
    pub id: CartelId, pub members: SmallVec<[FirmId; 8]>, pub good: GoodId,
    pub floor_price: Money,
    pub quota: SmallVec<[(FirmId, Qty); 8]>,
    pub formed: SimMinute, pub secrecy: Q,
}

pub struct Union {
    pub id: UnionId, pub scope: UnionScope,   // Site | Firm | Branch
    pub members: Vec<CitizenId>, pub density: Q,
    pub militancy: Q, pub strike_fund: Money,
    pub leader: CitizenId,
    pub demand: Option<UnionDemand>,
    pub state: UnionState,                    // Uśpiony | Żądanie | Negocjacje{runda} | Strajk{od} | Ugoda
}
```

**`SupplierRelation` jest mój, ale liczby karmiące `trust` są M6.** M6 jawnie odmówił własności
tego typu (relacja to §7.9, czyli mój zakres) i konsumuje z niego **dokładnie dwa pola**:
`discount_bps` (korekta ceny przy rozstrzyganiu RFQ) i `priority` (kolejność przydziału masy,
gdy dostawca nie ma dość dla wszystkich — to jest mechanizm „stały dostawca ma priorytet
w niedoborze" z §7.9). `trust`, `since`, `volume_cum` i `exclusivity` są wyłącznie moje.

**Nie buduję własnego licznika opóźnień.** `trust` liczę z `SupplyContract` M6:
`late_deliveries` i `missed_mass`. Uwaga, która kosztowałaby inaczej dzień debugowania: kontrakt
M6 ma `grace_minutes`, więc „spóźnione" i „spóźnione **ponad tolerancję**" to dwie różne liczby —
`trust` karze za tę drugą. Własny licznik obok licznika M6 rozjechałby się przy karach umownych.

Degradacja jest łagodna w obie strony: dopóki `SupplierRelation` nie istnieje, M6 liczy RFQ bez
rabatu i bez priorytetu, więc M10 nie blokuje M6, a M6 nie blokuje M10.

**Hazard wykrycia kartelu (miesięczny).** Kartel nie ginie od rzutu kostką, tylko od własnej chciwości:

```
h = 0,5%                                       // baza
  + 0,3% × (liczba_członków − 3).max(0)        // każdy dodatkowy członek to dodatkowe usta
  + 2,0% × (odchylenie_ceny_od_benchmarku% / 10)   // im więcej kradniesz, tym bardziej widać
  + 0,2% × liczba_niezadowolonych_wtajemniczonych  // menedżer z nastrojem < −40 lub zwolniony
  × aktywność_regulatora                       // z M8, 0,5..2,0
```

Kara: 10% obrotu 12-miesięcznego per członek (ograniczone wypłacalnością), rozwiązanie kartelu,
**uderzenie w markę: afinitet −20 u każdego mieszkańca posiadającego slot tej marki** (to jest
miejsce, gdzie dwa systemy tej fazy spotykają się i dają emergencję), 24 miesiące karencji.
Kartel trafia do kroniki **dopiero po wykryciu** — inaczej kronika spoileruje graczowi tajemnicę.

**Formowanie związku.** Trzy warunki naraz, wszystkie z §6.6:

```
grievance = clamp( w1 × (zysk_na_pracownika vs udział_płac_docelowy)
                 + w2 × (mediana_płacy_miejskiej_dla_roli − płaca_tutaj) / mediana
                 + w3 × średni_stres
                 + w4 × wypadki_12m, 0, 100)
```
1. `grievance ≥ 55` przez ≥ 60 kolejnych dni,
2. największa spójna składowa grafu relacji **wśród pracowników zakładu** ≥ max(8, 25% załogi)
   (przeszukiwanie ograniczone do załogi — kilkadziesiąt wierzchołków, koszt pomijalny),
3. gęstość potencjalnego członkostwa ≥ 30%.

Żądanie jest zakotwiczone na **znanych** płacach porównywalnych firm — czyli na tym, co pracownicy
wiedzą z grafu relacji i plotki (§5.7), a nie na prawdziwej medianie. Związek może żądać za dużo albo
za mało, bo ma niepełną informację. To jest realizm, który wychodzi z systemu, nie z parametru.

**Negocjacje i strajk.** Rundy tygodniowe; firma kontruje na podstawie swojej sytuacji finansowej
z ksiąg M7; próg akceptacji związku maleje wraz z wyczerpywaniem funduszu strajkowego. Strajk
zatrzymuje produkcję **zakładu** (nie całej firmy), uruchamia kary z kontraktów B2B M6 u odbiorców
(kaskada!), jest publikowany przez media, uderza w markę pracodawcy. Fundusz się kończy — nie ma
strajków wiecznych.

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

## 6. Kontrakty międzyfazowe

### Dostarczam

#### API crate'u `sim/macro` — kontrakt wiążący

Dok. 00 §1 przyznaje własność crate'u `sim/macro` fazie M10. **Poniższe sygnatury są kontraktem, nie
propozycją.** M7 tworzy wyłącznie warstwę „co jeśli" **na tym API** i nie definiuje własnego stanu
makro ani własnej agregacji dzielnic.

```rust
// ── rdzeń: stan i krok ────────────────────────────────────────────────
pub fn step(state: &mut MacroState, params: &MacroParams);

// ── most mezo↔makro ───────────────────────────────────────────────────
pub fn lift(world: &World) -> MacroState;
pub fn lower(state: &MacroState, world: &mut World, seed: u64) -> Result<(), LowerError>;

/// Czysty rdzeń rozwinięcia: bez stanu ukrytego, bez dostępu do World.
/// To jest miejsce, w którym replay i zapis mogą się rozjechać — dlatego jest funkcją czystą
/// i jest testowane osobno. `lower()` jest tylko sterownikiem wołającym to per komórka.
pub fn lower_cell(cell: &MacroCell, seed: u64, tick: Tick) -> CellExpansion;

// ── tryby (ten sam `step`, różne wejścia) ─────────────────────────────
pub fn dry_run(cfg: &DryRunConfig, seed_world: &SeedWorld) -> DryRunResult;       // M10
pub fn what_if(base: &MacroState, sc: &Scenario, horizon_days: u16) -> MacroOutcome; // M7
pub fn fast_forward(world: &mut World, cfg: &FastForwardConfig) -> FastForwardReport; // M12

// ── polityka LOD (M12 dostarcza wywołanie i watchdog, M10 — matematykę) ──
pub struct MacroLodPolicy { /* progi prędkości gry, histereza, warunki blokady przejścia */ }
impl MacroLodPolicy {
    pub fn target_lod(&self, speed: GameSpeed, load: &LoadStats) -> Lod;
    /// Czy wolno teraz przejść na `target`? Blokuje w środku fixingu, negocjacji, strajku.
    pub fn may_transition(&self, from: Lod, to: Lod, world: &World) -> Option<BlockReason>;
}
```

**Podzbiór, który musi być gotowy już w M7** (reszta może powstać w M10):

| Element | Wymagany w M7 | Uzasadnienie |
|---|---|---|
| `MacroState`, `MacroCell`, `MacroFirm` | **tak** — pełne definicje pól | M7 nie może zbudować „co jeśli" bez stanu |
| `lift()` | **tak** | M7 musi umieć zrobić zdjęcie świata; **M7 nie pisze własnej agregacji** |
| `step()` | **tak**, fazy 2–6 (praca, produkcja, dobra, ceny, finanse) | to wystarcza dla horyzontu 4–12 kwartałów |
| `what_if()` | **tak** | to jest właśnie szkic M7 z dok. 00 §1 |
| `step()` fazy 1, 7, 8 (demografia, relacje, zdarzenia) | nie — M10 | nieistotne dla horyzontu kwartalnego |
| `lower()`, `lower_cell()` | nie — M10 | M7 nigdy nie rozwija „co jeśli" do świata, tylko czyta agregaty |
| `dry_run()`, `fast_forward()`, `MacroLodPolicy` | nie — M10 | |

`MacroFirm` **zachowuje `FirmId` także w trybie „co jeśli"** — M7 musi móc zadać pytanie
„co jeśli konkurent X obniży cenę o 8%", a to wymaga wskazania konkretnej firmy.

#### Ograniczenie kontraktu — co `what_if()` i `fast_forward()` gwarantują, a czego nie

Rozstrzygnięte, nie otwarte. To ostrzeżenie jest częścią kontraktu i obowiązuje każdego, kto sięga
po to API — w szczególności M7 przy AI strategicznym i M9 przy panelach gracza.

Model makro operuje na agregatach dzielnica × klasa, więc **prognoza losu pojedynczej firmy
z dokładnością procentową jest z niego nieosiągalna.** Nie jest to usterka do naprawienia
w implementacji: wariancja rozkładu wielomianowego przy ~200 klientach daje odchylenie udziału
rynkowego rzędu 3,5% i żaden wspólny kernel tego nie zdejmie (analiza w §7.3/K2).

**Do czego `what_if()` służy:**
- porównywanie wariantów strategii **między sobą** („czy ekspansja do dzielnicy B bije obniżkę ceny"),
- ocena stanu dzielnicy i miasta w horyzoncie 4–12 kwartałów,
- wykrywanie kierunku i znaku zmiany, nie jej wielkości.

**Czego z niego nie wolno zrobić:**
- **decyzji, które muszą być spójne co do grosza** — księgowanie, rozliczenie kontraktu, wypłata,
  podatek. Makro nigdy nie jest źródłem kwoty trafiającej do księgi;
- **liczb wyświetlanych graczowi w formie wyglądającej na precyzyjną.** Panel nie pokazuje
  „prognozowany zysk: 1 284 300 zł". Pokazuje przedział, ranking wariantów albo strzałkę kierunku.
  Fałszywa precyzja jest tu gorsza od braku prognozy, bo gracz jej uwierzy;
- **zdania „mój zysk za rok wyniesie X"** w żadnej formie, w UI ani w uzasadnieniu decyzji AI.

To samo ograniczenie dotyczy `fast_forward()` (M12): tryb 50× przewiduje stan dzielnicy i miasta,
**nie los pojedynczej firmy**. M12 potwierdził i zdjął to założenie z trzech miejsc u siebie.

**Wzorzec konsumpcji — kształt przyjęty przez M7, do naśladowania przez M9.** Zamiast wystawiać
`MacroOutcome` wprost, M7 trzyma go w polu prywatnym i udostępnia dwie rzeczy:

```rust
fn decisive_winner(&self) -> Option<VariantId>;  // zwycięzca TYLKO gdy przewaga > error_margin_bp
fn direction(&self) -> Trend;                    // kierunek, nie wielkość
```

`None` znaczy „nierozstrzygalne" i przekłada się na `KeepCourse` — a nie na wybór wariantu, który
przypadkiem wyszedł o 0,3% lepiej. To jest właściwa odpowiedź na pytanie „co pokazać zamiast
prognozy punktowej": ranking z jawnym marginesem i uczciwe „nie wiem", gdy margines go pochłania.
M9 powinien zrobić to samo w panelu — przedział albo strzałka, nigdy kwota.

Konsekwencja dla testów po stronie konsumenta: testuje się **uporządkowanie wariantów**, nie
dokładność bezwzględną. M7 sprawdza zgodność rankingu makro z przebiegiem mezo ≥ 95% tam, gdzie
różnica przekracza margines, uczciwość samego marginesu (≥ 90% odchyleń w deklarowanym paśmie)
oraz `decisive_winner() == None` w 100% przypadków nierozstrzygalnych.

**Margines nie jest parametrem konsumenta — jest obietnicą `sim/macro` i podaję go razem z wynikiem.**

```rust
pub struct MacroOutcome {
    /* ... */
    /// Deklarowany błąd TEGO wyniku: funkcja n komórki, horyzontu i liczby firm w porównaniu.
    /// Konsument nie zgaduje tej liczby i nie wolno mu jej nadpisać.
    pub error_margin_bp: u32,
}
```

Powód jest taki, jak ujął to M7: `decisive_winner()` jest tylko tak dobre, jak liczba, którą mu się
poda, a **zbyt wąski margines po cichu przywraca skokowe decyzje**, które to rozwiązanie miało
wyeliminować. Gdyby margines wybierał konsument, dobrałby go na oko albo — co gorsza — dostroił tak,
żeby jego AI wreszcie „podejmowało decyzje". Ja mam dane, żeby go policzyć: znam `n` komórki,
horyzont i liczbę porównywanych firm, czyli dokładnie te trzy rzeczy, od których zależy błąd (§7.3/K2).

Stąd nowe kryterium akceptacji **po mojej stronie**, nie tylko po stronie M7:

| Test | Próg |
|---|---|
| `macro_error_margin_is_honest`: odsetek faktycznych odchyleń makro vs mezo mieszczących się w deklarowanym `error_margin_bp` | **≥ 90%** |
| Margines nie jest zaniżony przez zawężenie: mediana `|odchylenie| / error_margin_bp` | **0,3–0,8** (poniżej 0,3 margines jest rozdęty i wszystko staje się nierozstrzygalne; powyżej 0,8 — zaniżony) |

Margines jest obietnicą, więc jest testowany jak obietnica. Rozdęty margines nie jest bezpieczną
stroną błędu — to `decisive_winner()`, które zawsze zwraca `None`, czyli API udające ostrożność
i bezużyteczne.

Konsekwencja dla testów: `lod_macro_aggregate_accuracy` **celowo nie obejmuje** wielkości o małym n —
zielone CI nie może sugerować gwarancji, której nie ma.
`MacroState` metropolii ≤ 8 MB — klonowanie pod równoległe scenariusze jest tanie (10 wariantów = 80 MB).

**Granica własności wobec M4 (`sim/traffic`).** Ta sama zasada co przy `econ_kernel` z WP10.2:
`sim/macro` nie zawiera logiki dziedzinowej. Podział:

| Co | Właściciel | Forma |
|---|---|---|
| Agregat kohortowy (dzielnica × klasa), pętla czasu, alokacja podróży do kohort | **M10 / `sim/macro`** | `MacroCell`, faza 2 i 4 kroku |
| **Model czasu przejazdu** i jego kalibracja z obserwacji mezo (§17.6) | **M4 / `sim/traffic`** — potwierdzone | `TravelTimeMatrix::lookup` (dzielnica × godzina, aktualizowana z obserwacji) |
| `CommuteMatrix` w `MacroState` | M10 trzyma, M4 wypełnia | snapshot `TravelTimeMatrix` z chwili `lift()` |

M4 potwierdził: `travel_time()` zostaje w `sim/traffic`, a `TravelTimeMatrix::lookup` jest moim
jedynym źródłem czasów przejazdu. **Nie buduję własnego modelu** — ani w makro, ani w dry-runie.

Czyli: makro ruchu **nie jest ani w całości moje, ani w całości M4** — jednostka pracy (kohorta,
podróż rozpoczęta w ticku) jest moja, wzór na czas przejazdu jest M4. Gdyby było inaczej, mielibyśmy
dwa modele czasu przejazdu i te same rozjazdy, którym zapobiega WP10.2 po stronie ekonomii.

**Dla M12 (tryb 50× i długie sesje):**
```rust
pub struct FastForwardConfig { pub days: u32, pub keep_identities: bool, pub events: bool }
```
Gwarancja: `fast_forward` **nie tworzy ani nie niszczy pieniądza i masy** (§7.3, punkt 1) —
to jest własność konstrukcyjna, niezależna od dokładności agregatów. M12 dostarcza wywołanie
z pętli gry i `SpeedGovernor`; matematykę przełączania daje `MacroLodPolicy` powyżej.
M12 nie potrzebuje własnej agregacji ruchu — dostaje ją z `MacroCell` i `travel_time()` M4.

**Dla M1/M2 (generator) i dla `game/` (start partii):**
```rust
pub fn dry_run(cfg: &DryRunConfig, seed_world: &SeedWorld) -> DryRunResult;
pub struct DryRunResult { pub state: MacroState, pub chronicle: Vec<ChronicleEvent>,
                          pub verification: VerificationReport, pub rebalance_rounds: u8 }
```

**Dla M3 (agenci) — rozszerzenie pamięci:**
```rust
pub fn brand_affinity(c: CitizenId, b: BrandId) -> Option<BrandAffinity>;  // z zanikiem leniwym
pub fn touch_brand(c: CitizenId, b: BrandId, t: Touch);                    // Ad|Experience|Rumor|Media
```

**Dla M5 (decyzja zakupowa) — wypełnienie §6.4:**
`purchase_score` dostaje realne `afinitet_do_marki` i `jakość_postrzegana` zamiast stałych.
Sygnatura `purchase_score` **nie zmienia się** — zmienia się tylko źródło danych w `ScoreCtx`.

**Dla M6 (łańcuch):** `SupplierRelation` jako wejście do wyboru dostawcy; licencja i franczyza
jako `ContractId`, bez nowego typu umowy.

**Dla M8 (miasto):** `Cartel` i `CartelDetection` jako klient regulatora; `PerilStats` jako konsument
strumienia zdarzeń.

**Dla M9 (UI) — wymagania, nie implementacja:**
1. Panel marketingu: mapa cieplna ekspozycji per dzielnica, lejek znajomość → próba → afinitet,
   wykres `expected_quality` vs `actual_quality` w czasie (to jedno miejsce, gdzie gracz zobaczy,
   że przereklamował produkt).
2. Panel R&D: drzewo technologii per branża z postępem i rokiem „światowym"; lista patentów własnych
   i cudzych z ofertami licencji.
3. Panel giełdy: księga zleceń przed fixingiem, historia kursu, akcjonariat z progami 5/25/50%,
   kalendarz publikacji wyników.
4. Panel relacji: graf dostawców z `trust` i ekskluzywnością; ostrzeżenie o ryzyku kartelu
   (gracz musi wiedzieć, że to nielegalne, zanim wejdzie).
5. Panel pracowniczy: `grievance` per zakład jako ostrzeżenie wyprzedzające, przebieg negocjacji,
   licznik funduszu strajkowego.
6. Karta inspekcji mieszkańca: sekcja „marki, które znam" — 16 slotów z afinitetem, oczekiwaną
   jakością i **źródłem** kontaktu. To jest główny dowód, że marka nie jest liczbą.
7. Kronika: filtr `provenance: DryRun` i oś czasu sprzed startu partii.

### Konsumuję

| Od | Co |
|---|---|
| M0 | `Money`, `Q`, `Tick`, `StreamId`, RNG, bufory komend, hash stanu |
| M2 | `engine/spatial` (ulotki), `DistrictId`, parcele |
| M3 | graf relacji, magazyn doświadczeń, plotka (§5.7), cechy osobowości, potrzeby |
| M4 | strumień przejazdów po krawędziach (zasięg billboardów), `CommuteMatrix` z §17.6 |
| M5 | `purchase_score`, oferty, budżety GD, majątek GD |
| M6 | `ContractId`, kary umowne, importer z warstwy 3; `GoodUnit { Grams, Milliunits }` z `data/goods/` (stabilny dla `key` w obrębie wersji danych); `SupplyContract::{late_deliveries, missed_mass}` jako jedyne źródło `trust` |
| M7 | `FirmId`, księgowość, raporty kwartalne, `FirmPersonality`, HR, płace, polityki cenowe |
| M8 | strumień zdarzeń (`sim/events`), stawki podatkowe, regulator, stopa bazowa |
| M9 | podsystem kroniki, `DecisionReason` w karcie inspekcji |

---

## 7. Testy i kryteria akceptacji

### 7.1 Determinizm (dok. 00 §3, DoD fazy)

- Nowe warianty `StreamId`, **zakres przydzielony M10: 280–299** (dok. 00 §3 — wartości istniejących
  wariantów nietykalne, M10 nie wychodzi poza swój zakres):

  | Wartość | Wariant | Użycie |
  |---|---|---|
  | 280 | `AdNotice` | czy mieszkaniec zauważył billboard |
  | 281 | `AdMediaPick` | dobór odbiorców reklamy prasowej/radiowej/TV z czytelnictwa |
  | 282 | `AdLeaflet` | dobór odbiorców ulotek w promieniu |
  | 283 | `Rumor` | propagacja plotki i PR przez graf relacji |
  | 284 | `MediaEditorial` | dobór zdarzeń do publikacji przez redakcję |
  | 285 | `RnDBreakthrough` | przełom skracający koszt węzła technologii |
  | 286 | `InvestorNoise` | szum wyceny per (inwestor, firma, miesiąc) |
  | 287 | `EarningsNoise` | szum publikowanych wyników kwartalnych |
  | 288 | `PerilDraw` | realizacja szkody ubezpieczeniowej |
  | 289 | `CartelDetection` | hazard wykrycia kartelu |
  | 290 | `UnionFormation` | formowanie związku i treść żądania |
  | 291 | `StrikeResolve` | rozstrzygnięcie rundy negocjacyjnej |
  | 292 | `MacroStep` | losowość wewnątrz kroku makro |
  | 293 | `MacroLower` | porządek rangowy przy rozwijaniu komórki |
  | 294 | `MacroSeedMemory` | zasiew pamięci doświadczeń po `lower` (D9) |
  | 295 | `DryRunEvent` | zdarzenia w historii „na sucho" |
  | 296–299 | rezerwa M10 | |

- **Kalendarz K-1: 360 dni (12 × 30).** Obowiązuje w epokach (`data/epochs/`), harmonogramie
  publikacji wyników (T+45 dni = 1,5 miesiąca), oknie statystyk ubezpieczeniowych (60 miesięcy
  = 1800 dni), karencji kartelowej (24 miesiące) i w liczbie kroków dry-runu (§5.7).
  Żadnego `365` w kodzie M10 — lint CI.
- Wszystkie nowe komponenty dopisane do funkcji haszującej stan ECS.
- Dwa przebiegi tego samego seeda przez 100 tys. ticków → identyczny ciąg hashy.
- Dwa przebiegi dry-runu 80 lat → identyczny hash `MacroState` i identyczna kronika.
- `lower()` dwukrotnie z tego samego stanu → identyczny świat (test na hash po rozwinięciu).
- Audyt: zero iteracji po `HashMap` w nowym kodzie (lint CI).

### 7.2 Własnościowe (dok. 00 §6)

| Test | Tolerancja |
|---|---|
| Suma pieniądza = emisja − destrukcja, po każdym kroku makro | **0 groszy** |
| `lift(lower(s)) == s` na polach `Money` | **0 groszy** |
| Suma akcji w obiegu == `Share.total_shares` po fixingu/emisji/przejęciu | **0 akcji** |
| Suma wypłaconej dywidendy == kwota uchwalona | **0 groszy** |
| Racjonowanie na fixingu: Σ przydziałów == wolumen | **0 akcji** |
| Suma składek i wypłat ubezpieczeniowych zgodna z księgą | **0 groszy** |
| Suma masy towaru w makro = produkcja − konsumpcja − straty | **0** |

### 7.3 Kontrakt LOD makro↔mezo — cztery kryteria z dok. 00 §4

Dok. 00 §4 został doprecyzowany: dla pary **mikro↔mezo** obowiązuje tolerancja 0 (zakres M4);
dla pary **makro↔mezo** obowiązuje kontrakt słabszy, ale z twardym rdzeniem. M10 przyjmuje te cztery
punkty jako kryteria akceptacji fazy w miejsce własnych tolerancji.

**Scenariusz referencyjny** (deterministyczny, zamknięty, bez zdarzeń losowych): 1 dzielnica,
2000 mieszkańców, 12 firm, 8 towarów, 2 banki. Przebieg (a) w pełnym mezo, przebieg (b) w makro
po `lift` stanu początkowego. Horyzont: 12 miesięcy (360 dni) per-commit, 100 lat nocnie.

Scenariusz **przechodzi przez `cell_grain()` jak każdy inny świat** — przy 2000 mieszkańcach
i 1 dzielnicy daje to `Classes3`, czyli 3 komórki po 666 osób. Gdyby wymusić na nim stałe
6 klas, wyszłoby 333 osoby na komórkę i **test referencyjny łamałby próg, który sam weryfikuje.**
Zapisuję to jawnie, bo jest to dokładnie ten błąd, który zrobiłem w pierwszej wersji dokumentu.

**Scenariusz nie jest jeden.** Testy kontraktowe LOD i dry-runu przebiegają na **pełnym zakresie
rozmiarów z §4.1** (20 tys. → 400 tys.), nie na jednym referencyjnym. Powód jest ten sam, który M12
zapisał u siebie jako R1d: faza, której scenariusze mają jedną skalę, testuje swoje skrzywienie
zamiast swojego kontraktu. U mnie najciaśniej jest **na małym mieście** — tam zapasu nad progiem
`MIN_CELL_POP` nie ma żadnego — a wszystkie moje intuicje liczbowe pochodzą z metropolii
(400 tys., 240 komórek, 51,2 MB slotów, 8 MB `MacroState`). Metropolia jest w tej fazie przypadkiem
wygodnym, nie brzegowym.

Reguła, z furtką — bez niej byłaby dogmatem i kosztowałaby czas na testach, które rozmiaru nie widzą:

> Test kontraktowy M10 na jednym rozmiarze miasta jest błędem projektu testu — **chyba że da się
> wskazać powód, dla którego weryfikowany kontrakt jest niewrażliwy na rozmiar.**

Który test czego wymaga:

| Test | Rozrzut rozmiarów | Powód |
|---|---|---|
| K2, K3 (dokładność, dryf) | **wymagany** | próg n zależy wprost od populacji na komórkę |
| Dry-run, Etap 10, ziarno | **wymagany** | bramki i ziarno skalują się z populacją |
| Budżety pamięci i czasu | **wymagany** | to są liczby per mieszkaniec i per komórka |
| K1 (zachowanie pieniądza i masy) | **nie** | niezmiennik księgowy, nie ma progu populacyjnego |
| K4 (`lift(lower(s)) == s`, czystość `lower_cell`) | **nie** | transformacja, prawdziwa dla każdej komórki osobno |
| Determinizm (hash, dwa przebiegi) | **nie** | własność RNG i kolejności, nie skali |
| Asymetria marki, fixing, podział dywidendy | **nie** | testy jednostkowe na liczbach, świat nieistotny |

Trzy pierwsze wiersze to dokładnie te, w których mój pierwszy dokument był skalibrowany od złej
strony zakresu. Cztery ostatnie mają powód i dlatego zostają małe.

#### K1 — Zachowanie pieniądza i masy co do grosza i grama, bez wyjątków

**Przyjęte bez zastrzeżeń. To jest własność konstrukcyjna, nie statystyczna** — i dlatego jest
osiągalne mimo przybliżonych agregatów. Dwie rzeczy są tu ortogonalne:
- **dokładność** mówi, *komu* przypadł pieniądz (makro zgaduje z rozkładu — stąd ≤ 0,5%),
- **zachowanie** mówi, *ile go jest w sumie* (makro nie zgaduje — księguje).

Każdy przepływ w makro przechodzi przez `ledger_post()` z WP10.2, czyli przez ten sam zapis
dwustronny co mezo: pieniądz zawsze przechodzi **między kontami**, nigdy nie powstaje przy alokacji.
Dzielenie kwoty agregatu na jednostki zawsze domyka się korektą reszty do pierwszego wg posortowanego
klucza (dok. 00 §2, mechanizm w §5.8). Ten sam schemat dla masy: `Qty` jest i64, a każda alokacja
towaru między odbiorców kończy się przypisaniem reszty.

Testy własnościowe (§7.2): suma pieniądza = emisja − destrukcja i suma masy = produkcja − konsumpcja
− straty, sprawdzane **po każdym kroku makro**, tolerancja **0**. Plus `lift(lower(s)) == s` na polach
`Money` i `Qty`, tolerancja **0**.

#### K2 — Odchylenie agregatów ≤ 0,5% w skali miesiąca gry

**Przyjęte, ale z jawnym ograniczeniem zakresu — i to jest miejsce, w którym proszę o świadomą decyzję.**

Próg 0,5%/miesiąc jest osiągalny dla agregatów, w których uśrednia się dostatecznie wiele decyzji.
Błąd zastąpienia losowania wartością oczekiwaną ma rząd O(1/√n) na pojedynczym udziale rynkowym;
przy sumowaniu po komórkach i dniach błędy o przeciwnych znakach znoszą się. Konkretnie:

| Agregat | n | Oczekiwane odchylenie miesięczne | ≤ 0,5%? |
|---|---|---|---|
| Suma pieniądza w systemie | — | 0 (K1) | **tak, z zapasem** |
| Gotówka + depozyty GD per dzielnica | 2000+ osób | 0,15–0,3% | **tak** |
| Kapitał i dług firm per dzielnica | 12+ firm | 0,2–0,4% | **tak** |
| Przychód i podatki skumulowane per dzielnica | — | 0,2–0,4% | **tak** |
| Obrót w jednej branży w dzielnicy | ≥ 5 firm | 0,4–0,8% | **na granicy** |
| **Przychód pojedynczej firmy** | 1 | **3–12%** | **nie** |
| **Cena towaru przy < 3 dostawcach w dzielnicy** | 1–2 | **2–8%** | **nie** |
| **Udział rynkowy pojedynczej marki** | 1 | **2–5%** | **nie** |

Trzy ostatnie wiersze są **fizycznie nieosiągalne** i nie da się tego naprawić inżynierią. Udział
rynkowy jednej firmy to realizacja rozkładu wielomianowego; makro zna jego wartość oczekiwaną,
a mezo losuje jedną realizację. Przy 200 klientach odchylenie standardowe udziału to ~3,5% i żaden
wspólny kernel tego nie zdejmie — zdjęłoby to dopiero symulowanie pojedynczych klientów, czyli
rezygnacja z makro.

**Rozstrzygnięte — dok. 00 §4 / K-5.** Próg 0,5%/miesiąc obowiązuje **wyłącznie dla agregatów
o liczebności n ≥ 500** (osób lub ≥ 10 firm). Ten sam próg n ≥ 500 wymusza skalowanie ziarna
komórek z populacją — rachunek i reguła w §5.6 („Ziarno agregacji jest zamknięte z obu stron"). Poniżej tego poziomu obowiązują wyłącznie K1 (zachowanie)
i K3 (brak dryfu) — bez progu na odchylenie pojedynczego kroku. Praktyczna konsekwencja jest
akceptowalna: tryb 50× i „co jeśli" **nie służą do przewidywania losu jednej firmy** i nie wolno
budować na nich takich funkcji w UI. Służą do przewidywania stanu dzielnicy i miasta.

Dla mniejszych zbiorów błąd 3–12% jest **wpisany w metodę i nie jest kwestią kalibracji** —
dok. 00 §4 mówi to wprost, więc nie jest to już moje zastrzeżenie, tylko kontrakt projektu.
Wiążąca konsekwencja dla konsumentów (M7, M9, M12): wynik `what_if()` wolno używać **porównawczo**,
nigdy jako liczby bezwzględnej w regule progowej ani jako wartości pokazanej graczowi.

#### K3 — Brak dryfu systematycznego (test trajektorii 100 lat)

To jest **ważniejsze kryterium niż K2** i jest prawdziwym testem tego, czy `sim/macro` i mezo dzielą
jeden model. Odchylenie 0,4% w każdym miesiącu w losową stronę jest nieszkodliwe. Odchylenie 0,4%
w każdym miesiącu **w tę samą stronę** to po 100 latach (1200 miesięcy) czynnik ~120× — świat, który
się rozpadł.

Test: trajektorie makro i mezo tego samego scenariusza przez 100 lat (36 000 dni przy kalendarzu 360),
próbkowane miesięcznie. Dla każdego agregatu liczymy różnicę względną `d(t) = (makro − mezo) / mezo`
i regresję liniową `d(t)` po czasie. Kryteria:

| Wielkość | Próg |
|---|---|
| Nachylenie regresji `|slope|` | **≤ 0,02 pp na rok gry** (czyli ≤ 2 pp po 100 latach) |
| Nachylenie istotne statystycznie (t-test) | **nie** — dryf musi być nieodróżnialny od zera |
| Skumulowana różnica po 100 latach, agregaty dzielnicowe | **≤ 5%** |
| Autokorelacja znaku `d(t)` (lag 1) | **≤ 0,3** — systematyczne odchylenie w jedną stronę jest wykrywane, nawet gdy mieści się w K2 |

Ostatni wiersz jest celowo ostry: dodaję go, bo sam próg na nachylenie da się przypadkiem przejść
na krótkiej próbce, a autokorelacja znaku łapie „makro zawsze odrobinę na plus" natychmiast.

Test 100-letni jest **nocny**, nie per-commit (mezo × 36 000 dni × 2000 agentów to minuty w headless,
nie sekundy). Per-commit działa wersja 12-miesięczna (K1 + K2). Regresja dryfu blokuje release, nie merge.

#### K4 — Przejście makro→mezo odtwarza stany indywidualne deterministycznie

Mechanizm w §5.8. Kryteria:
- `lower_cell()` jest funkcją czystą — dwa wywołania z tym samym `(cell, seed, tick)` dają bitowo
  identyczny wynik; brak dostępu do `World`, brak stanu ukrytego (to jest wymaganie M12: tu replay
  i zapis mogą się rozjechać).
- `lower()` wykonany dwukrotnie z tego samego `MacroState` → identyczny hash świata.
- `lift(lower(s)) == s` na polach `Money` i `Qty` — tolerancja **0** (test własnościowy, 1000 stanów).
- Tożsamość zachowana w całości: zbiór `CitizenSeed` po `lower()` jest identyczny co do elementu
  ze zbiorem przed; żaden mieszkaniec nie znika i żaden nie powstaje bez zdarzenia demograficznego.
- Sekwencja `lower → 1 miesiąc mezo → lift → 1 miesiąc makro` nie daje wyniku gorszego niż
  `lift → 2 miesiące makro` (test antyhisterezy — przełączanie LOD nie może samo w sobie generować błędu).

**Co to znaczy dla WP10.2.** Przekroczenie K2 lub K3 jest w praktyce zawsze tym samym błędem:
ktoś dopisał logikę ekonomiczną do `sim/macro` zamiast wołać `econ_kernel`. To jest prawdziwy cel
tych testów — nie mierzą jakości przybliżenia, tylko pilnują, że model jest jeden.

### 7.4 Funkcjonalne — progi liczbowe

| Obszar | Kryterium |
|---|---|
| Marka | +20 Q oczekiwań wymaga ≥ 7 ekspozycji; −20 Q rozczarowania kosztuje ≥ 14 pkt afinitetu; odbudowa ≥ 3 pozytywne doświadczenia |
| Marka | 400 tys. mieszkańców × 16 slotów ≤ 55 MB **mierzone** |
| Dry-run | Przebieg 64 seedów powtórzony dla **każdego rozmiaru z §4.1**, nie tylko metropolii |
| Ziarno | `macro_grain_meets_min_cell_pop`: wszystkie 7 rozmiarów z §4.1 daje ≥ 500 osób na komórkę |
| Ziarno | Zero stałych liczb komórek w kodzie — wszędzie `state.cells.len()` (lint + test na 20 tys. i 400 tys.) |
| Zasięg | Billboard: 8000 przejazdów/dobę × 12% → 960 ± 30 ekspozycji; przeniesienie na inną krawędź zmienia rozkład dzielnic zgodnie z macierzą dojazdów |
| Media | Publikacja o czytelnictwie 35%/8% daje znajomość > 40% / < 15% po 3 dniach (± 5 pkt) |
| R&D | 4 badaczy (skill 60) + 50 tys./mies. → węzeł 1200 RP w 9 ± 1 miesiącu |
| Giełda | Kurs nie porusza się przed publikacją wyników, jeśli nie ma plotki |
| Ubezpieczenia | Po 2 powodziach w oknie 60 mies. składka w dzielnicy nadrzecznej ≥ 2× wyższa, **bez parametru ryzyka w danych** |
| Ubezpieczenia | Przy < 100 polisach jedna szkoda nie zmienia składki o rząd wielkości (wygładzanie) |
| Kartel | Hazard rośnie monotonicznie z liczbą członków i odchyleniem ceny; po wykryciu afinitet marki −20 u posiadaczy slotu |
| Związki | Zakład z marżą 35% i płacą 20% poniżej mediany formuje związek w 3–9 miesięcy |
| Strajk | 100 strajków w balansatorze: mediana 4–21 dni, maksimum < 90 dni |
| Dry-run | 64 seedy: 100% przechodzi Etap 10, > 80% bez rundy naprawczej |
| Dry-run | 80 lat ≤ 60 s jednowątkowo; rozwinięcie 400 tys. mieszkańców ≤ 20 s |
| Wyjaśnialność | 100% nowych decyzji ma `DecisionReason` w karcie inspekcji (dok. 00 §7) |

### 7.5 Wydajność

- `macro_step` metropolii: ≤ 8 ms/krok jednowątkowo (7500 kroków ≤ 60 s).
- `MacroState` metropolii ≤ 8 MB (asercja, nie szacunek).
- Systemy reklamowe: ≤ 1,5% budżetu ticku przy 2000 aktywnych kampanii.
- `stock_fixing`: ≤ 5 ms przy 500 notowanych firmach i 50 tys. zleceń.
- **Ruch w makro (budżet uzgodniony z M12): ≤ 0,55 ms/tick.** Osiągalne tylko przy jednostce
  pracy = kohorta (dzielnica × klasa, ~2400 dla 150 tys.) i podróż rozpoczęta w ticku (~420/tick).
  **Zero iteracji per pojazd i per agent** — to jest warunek konieczny, nie cel optymalizacyjny.
  Czas przejazdu z `travel_time()` M4 (tablice §17.6), bez własnego modelu.
- Odczyt `brand_affinity` z zanikiem: ≤ 80 ns (criterion) — leży na ścieżce decyzji zakupowej.

---

## 8. Ryzyka fazy i mitygacje

| # | Ryzyko | Skutek | Mitygacja |
|---|---|---|---|
| R1 | **Makro rozjeżdża się z mezo mimo testów.** Ktoś dopisuje logikę ekonomiczną do `sim/macro`, bo „tam było wygodniej". | Świat startowy fałszywy; „co jeśli" M7 kłamie; tryb 50× psuje partię. | WP10.2 **przed** WP10.1: `sim/macro` fizycznie nie zawiera logiki — tylko agregację i pętlę. Lint CI przeciw literałom stawek w `sim/macro`. Test 7.3 per-commit. To jest ryzyko nr 1 fazy i cała jej architektura jest wokół niego zbudowana. |
| R2 | **Dry-run generuje świat, który wygląda dobrze w agregacie i absurdalnie w szczególe** (dzielnica bez sklepu spożywczego, firma z 200 pracownikami i jednym klientem). | Gracz widzi absurd w pierwszej minucie partii. | Bramki Etapu 10 obejmują kryteria **strukturalne** (każda firma ma dostawcę i pracownika, każdy mieszkaniec ma dom), nie tylko agregatowe. Plus ręczny przegląd 10 seedów przed zamknięciem fazy — czytanie raportu, nie tylko zielonego CI. |
| R3 | **Dry-run 80 lat trwa 10 minut.** | Start nowej partii nie do zniesienia. | Budżet 60 s jest kryterium ukończenia WP10.1, nie celem. Zmienny krok (7 dni wcześnie, 1 dzień w ostatnich 5 latach) daje 3× oszczędność przy zachowaniu dokładności tam, gdzie ona działa na stan startowy. Awaryjnie: `--dry-run-years 30` jako domyślne dla dużych miast, 80+ jako opcja. |
| R4 | **Sloty marek wypychają lojalność.** 16 slotów, agresywna kampania zalewa pamięć, mieszkaniec zapomina sklep, do którego chodzi od 10 lat. | Lojalność z §5.1 przestaje istnieć; marka staje się funkcją budżetu reklamowego — dokładnie to, czego §7.6 zabrania. | Przypinanie: pracodawca i sklep odwiedzony ≥ 8 razy nie podlegają wypieraniu. Wypieranie po `salience`, nie po czasie. Test regresji: mieszkaniec z 10-letnią lojalnością po kampanii konkurenta nadal ma slot swojego sklepu. |
| R5 | **Giełda staje się jedyną grą.** Wycena z opóźnionych danych + plotki = arbitraż, który dominuje nad prowadzeniem firmy. | Gracz przestaje symulować miasto, zaczyna klikać fixingi. | Fixing dzienny (nie ciągły) ogranicza częstotliwość. Koszty transakcyjne i podatek od zysków (M8). Płynność jest realnie mała — 500 firm i 5000 inwestorów to nie NYSE; duże zlecenie rusza kurs przeciw sobie. Monitorowane w balansatorze jako „udział zysków gracza z obrotu akcjami" — próg alarmowy 30%. |
| R6 | **Strajki jako spirala śmierci.** Strajk → kary z kontraktów → firma traci płynność → nie może podnieść płac → strajk trwa → bankructwo. Emergentne i realistyczne, ale jeśli zdarza się w 40% firm, gospodarka wymiera. | Balansator pokazuje wymieranie sektora. | Fundusz strajkowy jest skończony i to on wymusza rozstrzygnięcie. Test 7.4 (mediana 4–21 dni, max < 90). Jeśli balansator pokaże > 5% firm rocznie w strajku — parametry `grievance` do korekty, nie mechanizm. |
| R7 | **Nowa kategoria produktu psuje graf towarów w trakcie partii.** `NewGood` bez receptury i bez importu. | Towar bez źródła, popyt niezaspokajalny, spirala cenowa. | Walidator grafu towarów (dok. 00 §5) uruchamiany **także po odblokowaniu technologii**, nie tylko przy ładowaniu. Odblokowanie, które nie przechodzi walidacji, jest odrzucane i logowane jako błąd danych. |
| R8 | **Kartel jest zawsze nieopłacalny albo zawsze opłacalny.** | Mechanizm martwy albo dominujący. | Hazard jest funkcją odchylenia ceny — kartel umiarkowany jest opłacalny, chciwy się wykrywa. Kalibracja w balansatorze: cel to 20–50% karteli wykrytych w ciągu 5 lat gry. |
| R9 | **Kronika zalana wpisami z dry-runu.** 80 lat × wszystkie zdarzenia = nieczytelne. | Funkcja narracyjna martwa. | Próg skali dla wpisu z dry-runu jest wyższy niż dla zdarzenia w partii (tylko rzeczy widoczne w skali dzielnicy). Cel: 50–200 wpisów na 80 lat, nie 50 000. Filtr `provenance` w UI (M9). |
| R10 | **Zakres fazy jest największy w projekcie.** Osiem niezależnych systemów plus najtrudniejszy technicznie (`sim/macro`). | Faza się nie kończy. | Ścieżka krytyczna (10.2→10.1→10.3→10.4) jest wydzielona i **sama w sobie jest dostarczalnym artefaktem** — świat „zużyty" na starcie. Pozostałe WP są niezależne i każdy da się wyciąć bez naruszenia reszty. Kolejność cięcia przy presji: 10.9, 10.12, 10.11, 10.7. |
| R11 | **Liczby w tym dokumencie mogą być skalibrowane od złej strony zakresu — i nie wykryje tego mój własny przegląd.** Cztery realne błędy tej fazy (ziarno komórek łamiące próg n na małym mieście; scenariusz referencyjny stojący poniżej progu, który sam weryfikuje; `Qty` gubiące jednostkę towarów masowych; `error_margin_bp` jako parametr konsumenta zamiast obietnicy modelu) wyszły **wyłącznie z pytań innych faz**. Żadnego nie znalazł przegląd własny, mimo że wszystkie były w dokumencie od pierwszej wersji. | Liczby, których nikt nie zakwestionował, mają tę samą szansę być błędne co te cztery. Dotyczy to w szczególności wielkości, które postawiłem „z rozsądku": `BRAND_SLOTS = 16`, `MIN_CELL_POP = 500`, `K_UP/K_DOWN = 64/192`, okno 60 miesięcy w ubezpieczeniach, próg `grievance ≥ 55`, T+45 dni publikacji, hazard bazowy kartelu 0,5%. | Trzy rzeczy, żadna nie jest „uważniejszym czytaniem": (1) **każda z tych stałych ma w dokumencie jawny test z progiem liczbowym** (§7.4) — stała bez testu jest niewykrywalna z definicji; (2) **wszystkie mają wyliczenie lub pomiar jako źródło, nie intuicję** — `MIN_CELL_POP` ma wyprowadzenie z O(1/√n), `BRAND_SLOTS` jest jawnie oznaczony jako „start z 16, podnieść po pomiarze w balansatorze" (D4), a nie jako wartość docelowa; (3) **przegląd krzyżowy z fazą sąsiednią przed zamknięciem planu** — to jedyny mechanizm, który w tej rundzie faktycznie zadziałał, i jest tańszy niż znalezienie tego samego w implementacji. Ryzyko zostaje otwarte świadomie: nie znam sposobu, by je zamknąć w obrębie jednej fazy. |

---

## 9. Decyzje otwarte

| # | Decyzja | Kontekst | Kto rozstrzyga | Propozycja M10 |
|---|---|---|---|---|
> **Rozstrzygnięte przed startem fazy** (zapisane dla historii, nie wymagają już działania):
> - *Własność `sim/macro`*: M10 jest właścicielem crate'u (dok. 00 §1). API w §6 jest **kontraktem
>   wiążącym**, nie propozycją do zatwierdzenia; M7 buduje `what_if()` na tym API. Podzbiór wymagany
>   już w M7 jest wypisany w §6.
> - *Tolerancja LOD*: dok. 00 §4 doprecyzowany — mikro↔mezo 0, makro↔mezo cztery kryteria K1–K4.
>   M10 przyjmuje je jako kryteria akceptacji (§7.3) i rezygnuje z własnych progów.
> - *Kalendarz*: K-1, 360 dni (12 × 30). Naniesione w §5.7, §7.1.
> - *Zakres `StreamId`*: 280–299, rozpisany w §7.1.
> - *Zakres progu 0,5%* (dawne D1): dok. 00 §4 / **K-5** — próg obowiązuje **wyłącznie dla agregatów
>   o liczebności n ≥ 500**; dla mniejszych zbiorów, w szczególności dla pojedynczej firmy, błąd
>   3–12% jest wpisany w metodę i nie jest kwestią kalibracji. Wiążąca konsekwencja dla konsumentów:
>   wynik `what_if()` wolno używać **porównawczo**, nigdy jako liczby bezwzględnej w regule progowej
>   ani jako wartości pokazanej graczowi. Deklaracja błędu razem z wynikiem (`error_margin_bp`)
>   i test na jej uczciwość **w obie strony** są częścią kontraktu (§6, §7.3/K2).
> - *Refaktor `sim/economy`* (dawne D11): **nie będzie refaktoru** — M5 pisze `kernel` od razu jako
>   rdzeń. Podział i zasada rdzenia w WP10.2.
> - *`travel_time()`* (dawne D2): zostaje w `sim/traffic`; M4 dostarcza `TravelTimeMatrix::lookup`.

| **D1** | **Nowy katalog danych `data/tech/`.** | Dok. 00 §5 wylicza katalogi `data/`; `tech` nie ma na liście. | **Właściciel dok. 00** (dopisek). | Dopisać `data/tech/` do listy w dok. 00 §5, z walidatorem grafu (prereq istnieje, brak cykli, `NewGood` ma wpis w `data/goods/`) jako testem CI od M10. |
| **D2** | **Jawne trzymanie GD w makro (150 tys. × 48 B = 7,2 MB).** | §17.4 mówi „agregaty per dzielnica × klasa"; M10 robi wyjątek dla GD, bo mieszkanie to konkretna parcela, a §4.2 Etap 9 wymaga „majątków rodzin". | **M10 + M12** (M12 płaci za to w trybie 50×). | Utrzymać wyjątek. 7,2 MB przy budżecie 6 GB to 0,12%, a alternatywa (rozkład zamiast adresów) psuje rynek nieruchomości po `lower`. |
| **D3** | **Czy gracz może posiadać media?** | §7.2 wymienia media jako typ firmy; PRD nie rozstrzyga konfliktu interesów. | **M9 (gracz) + M8 (regulator).** | Tak, z konsekwencją: posiadanie tytułu daje przewagę PR, ale regulator M8 może nałożyć ograniczenia koncentracji, a wiarygodność tytułu spada u czytelników, którzy zauważą stronniczość (mechanizm już jest — wiarygodność jest per-para). Nie wymaga nowego kodu, wymaga decyzji projektowej. |
| **D4** | **`BRAND_SLOTS = 16` — czy to wystarczy?** | 16 slotów × 8 B = 128 B/mieszkańca. Przy 24 slotach: 76,8 MB (1,28% budżetu). | **M10, po pomiarze w balansatorze.** | Start z 16. Jeśli balansator pokaże, że mediana liczby marek, z którymi mieszkaniec ma realny kontakt, przekracza 13 — podnieść do 24 (stała, jedna zmiana). Nie robić tego przed pomiarem. |
| **D5** | **Kiedy dry-run działa: przy generacji świata czy przy pierwszym uruchomieniu partii?** | 60 s to dużo dla ekranu ładowania, mało dla generatora, który i tak liczy teren. | **M9 (`game/`, przepływ startu partii).** | W tle, równolegle z Etapami 1–8 generatora (teren i dry-run są niezależne do momentu D0 — a D0 potrzebuje tylko stref i parcel z Etapu 5). Realny narzut na ekranie ładowania: ~15 s. |
| **D6** | **Czy strajk zatrzymuje zakład, czy firmę?** | §6.6 mówi „produkcja stoi" — niejednoznacznie. | **M10 + M7.** | Zakład. Związek formuje się na poziomie zakładu (graf relacji jest lokalny — ludzie znają współpracowników, nie całą korporację). Strajk ogólnofirmowy jest możliwy jako eskalacja (`UnionScope::Firm`), ale nie domyślny. |
| **D7** | **Czy `lower()` musi odtwarzać pamięć doświadczeń mieszkańców?** | Po dry-runie mieszkańcy nie mają historii zakupów — wszystkie sklepy są im obce, więc pierwszy tydzień partii to chaos wyborów. | **M10 + M3.** | Tak, minimalnie: `lower()` zasiewa 3–5 wpisów doświadczeń i 2–4 sloty marek per mieszkaniec, wybierając sklepy z jego dzielnicy proporcjonalnie do ich udziału rynkowego w makro. Bez tego świat startowy jest „zużyty" w liczbach i sterylny w zachowaniu. Koszt: jeden przebieg przy `lower`. |
| **D8** | **Zerwanie śladu pochodzenia partii na granicy makro** (zgłoszone przez M6). `MacroStock` zna ilość, nie zna partii — przy `lower()` partie są generowane na nowo, więc trace „od pola do półki" (§14.4) kończy się na `FromAggregate`, nie na złożu. | Dotyczy każdego świata po dry-runie i każdego powrotu z trybu 50×. Naprawa wymagałaby trzymania partii w makro — co przekreśla sens agregacji (partie to setki tysięcy encji). | **M6 + M10 + M9** (M9 pokazuje trace w UI). | Zaakceptować i **pokazać jawnie**: partia z agregatu ma `provenance: FromAggregate { district, last_supplier }`, partia z historii — `provenance: DryRun`; UI kończy ślad komunikatem „odtworzone z agregatu dzielnicy", nigdy zmyślonym łańcuchem. Jeden skok wstecz zachowany w `MacroFirm.suppliers`. Alternatywa (trzymanie partii w makro) do odrzucenia. |

---

## 10. Szacunek wielkości

| WP | Temat | Rozmiar | Zależności |
|---|---|---|---|
| WP10.2 | Domknięcie `econ_kernel` (M5 buduje rdzeń natywnie) | **M** | M5, M6, M7 |
| WP10.1 | `sim/macro`: stan, `step`, `lift`/`lower` | **XL** | WP10.2 |
| WP10.3 | Historia „na sucho" (D0–D5, rebalance, kronika) | **L** | WP10.1, M1–M2 |
| WP10.4 | Test spójności LOD + balansator | **M** | WP10.1–10.3 |
| WP10.5 | `BrandAffinity`, sloty, zanik, asymetria | **M** | M3, M5, M7 |
| WP10.6 | `AdCampaign`, `Reach`, osiem kanałów | **L** | WP10.5, M4, M2 |
| WP10.7 | Media jako firmy, `Story` → plotka | **M** | WP10.6, M3, M8 |
| WP10.8 | `TechTree`, RP, `Patent`, licencje | **L** | M7, dane |
| WP10.9 | Nowe kategorie produktów, zmiana potrzeb | **M** | WP10.8, M8, M3 |
| WP10.10 | `Share`, `OrderBook`, fixing, `Valuation` | **L** | M7, M5 |
| WP10.11 | Przejęcia, dywidendy, emisje, ład korporacyjny | **M** | WP10.10 |
| WP10.12 | `InsurancePolicy`, `PerilStats`, wypłaty | **M** | M8, M7 |
| WP10.13 | `SupplierRelation`, ekskluzywność, `Cartel`, franczyza, JV | **L** | M6, M7, M8 |
| WP10.14 | `Union`, żądanie, negocjacje, strajk | **M** | M7, M3 |
| WP10.15 | Kroniki i wyjaśnialność (przekrojowy) | **S** | M9, wszystkie |
| WP10.16 | Determinizm, `StreamId`, hash stanu (przekrojowy, DoD) | **S** | wszystkie |

**Rozkład:** 1 × XL, 4 × L, 8 × M, 2 × S. **Najcięższa faza projektu po M7.**

WP10.2 zmalał z L do M, odkąd M5 buduje `kernel` jako rdzeń od pierwszego dnia zamiast oddawać go
do wyciągnięcia po fakcie — jedyna pozycja tego planu, która stanieje wskutek uzgodnień, a nie
wskutek cięcia zakresu.

**Ścieżka krytyczna:** WP10.2 → WP10.1 → WP10.3 → WP10.4 (M + XL + L + M). Ona jedna jest
dostarczalnym artefaktem (świat „zużyty" na starcie) i ona jedna blokuje M12.

**Kolejność cięcia przy presji na zakres** (od pierwszego do ostatniego): WP10.9 (nowe kategorie),
WP10.12 (ubezpieczenia), WP10.11 (przejęcia — giełda działa bez nich), WP10.7 (media — kanały
reklamowe działają bez redakcji). **Nie do wycięcia:** WP10.1–10.4 (świat startowy i spójność LOD),
WP10.5 (bez niej §7.6 nie jest zrealizowane w ogóle).
