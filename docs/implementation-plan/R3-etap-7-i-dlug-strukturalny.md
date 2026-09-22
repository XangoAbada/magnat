# R3 — Etap 7 i dług strukturalny bez właściciela

Dokument wykonawczy spoza numeracji faz, trzeci po `R1-refaktor-po-M5.md`
i `R2-naprawy-po-M11.md`. Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.

Powstał w `R2f` (`R2-WP26`) i **nie powstał z pomysłu, tylko z pomiaru**. Egzekutor rejestru
długu, który `R2-WP26` dołożył do `scripts/struct_guard.py`, zapytał po raz pierwszy o rzecz,
o którą przez siedem faz nie pytał nikt: czy adresat pozycji rejestru **istnieje i czy jeszcze
nie minął**. Odpowiedź brzmiała: na 68 pozycji rejestru **55 nie miało żywego adresata** —
dziewięć było zamkniętych i nikt tego nie odnotował, a czterdzieści dziewięć wskazywało fazę,
która zamknęła się bez nich.

Plan R2 przewidywał cztery takie pozycje. To jest różnica rzędu wielkości i jest ona
treścią tego dokumentu: reguła „pozycja rejestru ma adresata" działała dokładnie tak długo,
jak długo ktoś sprawdzał adresata — czyli nigdy.

| | |
|---|---|
| **Wejście** | R2 zamknięte (`R2f`), `master` zielony, `struct_guard --all` przechodzi |
| **Wynik do pokazania** | Etap 7 rozdrabnia zakłady po lokalach, a rejestr długu ma mniej pozycji niż na wejściu — obie liczby zmierzone, nie oszacowane |
| **Kryterium zamknięcia** | Kryteria pakietów §4 plus §7 |
| **Poprzednia / następna** | `R2f-pomiar-i-bramki.md` / `M12a-pamiec.md` (R3 **nie blokuje** M12) |

---

## 1. Po co to jest

Dwa tematy, jedna przyczyna. **Przyczyną jest pomiar wykonany po fakcie.**

**(1) Etap 7 generacji miasta stawia dziesięciokrotnie za mało firm.** Zmierzone w M11c
(`R2-WP18`, `D-N20`): świat 4 km, profil `industrial`, ziarno 1 — 24 800 mieszkańców,
**199 firm**, 202 zakłady, czyli jedna firma na 125 osób wobec obiecanych 1 : 15…25. Lokali
użytkowych jest przy tym **4 186**, z czego 3 209 należy już do jakiegoś zakładu: premises
**są**, brakuje mechanizmu, który wsadzi do nich osobne firmy. Drugi objaw tej samej przyczyny:
`it_office` ma 19 zakładów i 3 483 etaty, czyli 183 osoby na biuro w mieście 25-tysięcznym,
bo obsada liczy się z powierzchni **całego budynku** — zakład bierze cały budynek.

Sufit `D-N6` zadziałał zgodnie z planem: pakiet skończył się **pomiarem i decyzją**, a nie kodem.
`D-N20` rozstrzygnęło, że zakład jest **lokalem**, a nie budynkiem, i że jest to przeprojektowanie
Etapu 7, więc należy do własnej fazy. Ta faza to R3 i do R2f nie miała dokumentu — czyli była
adresatem dokładnie tej klasy, którą `R2-WP26` ma łapać.

**(2) Czterdzieści dziewięć pozycji rejestru długu strukturalnego straciło adresata.** Nie
dlatego, że ktoś je zignorował, tylko dlatego, że każda wskazywała fazę M2–M11 „przy okazji",
a wszystkie te fazy się zamknęły. Rejestr powstał w R1 (po M5) przy założeniu, że M6–M12 będą
te pliki otwierać z własnych powodów. M6–M11 otworzyły część i podzieliły to, co podzieliły —
a reszta została z adresem nieżyjącym.

Do M12 zostaje pięć podfaz o wąskim zakresie (pamięć, zapis, tryb 50×, modding, lokalizacja)
i żadna z nich nie jest fazą od dzielenia cudzych plików. Wpisanie ich tam byłoby siódmą
przeprowadzką tej samej pozycji.

---

## 2. Zakres — wchodzi / nie wchodzi

### Wchodzi

- Rozdrobnienie Etapu 7: zakład jako **lokal**, nie budynek (`D-N20`).
- Pozycje rejestru długu przeniesione tu przez `R2-WP26` — **z pomiarem, nie hurtem**:
  pozycja, której przekroczenia już nie ma, zamyka się znacznikiem `✅`, a nie pakietem.
- Pakiety R2, które zostały otwarte przy zamknięciu R2 (§4 dokumentu R2): `R2-WP35`
  (opieka nad dzieckiem poniżej wieku szkolnego) i `R2-WP38` (motoryzacja idzie za dochodem,
  `D-N12`).
- Pozycje wykazu R2, które R2 zamknęło jako przeniesione — ich lista z numerami stoi
  w §11 dokumentu R2.

### Nie wchodzi

- Nic z zakresu M12. R3 **nie jest warunkiem wejścia M12a** i obie mogą iść w dowolnej
  kolejności: budżet pamięci mierzy się na świecie, jaki jest, a nie na docelowym.
- Zmiana normatywu mocy produkcyjnej. Rozdrobnienie to **ta sama powierzchnia i ten sam
  sumaryczny etat w większej liczbie podmiotów** — inaczej byłoby przestrojeniem gospodarki,
  a nie naprawą generatora.
- Dzielenie plików, które są długie, bo jeden algorytm jest długi. Reguła z `CLAUDE.md`
  obowiązuje bez zmian: dzieli się pliki z dwoma tematami.

---

## 2.1 Trzy obietnice, które nie dojechały do M8e

Znalezione przez regułę 3 `scripts/plan_guard.py` przy jej pierwszym uruchomieniu
(`R2-WP26`, 2026-09-22). Wszystkie trzy stały w tabeli korekt fazy M8 z adresem `M8e`,
a M8e zamknęło się bez nich. Żadna nie jest duża; wszystkie trzy są tą samą klasą co
pozycje rejestru długu — zobowiązanie zapisane pod adresem, którego nikt nie odwiedził.

| Kod | Co zostało obiecane | Stan |
|---|---|---|
| `CD-6` | **Planowy remont linii przesyłowej.** `EdgeState` ma dwa warianty (`Ok`, `Tripped { until }`); trzeci — `UnderMaintenance` — miał powstać razem z harmonogramem konserwacji operatora | Wariantu nadal nie ma i to jest **dobrze**: wariant, którego nic nie ustawia, przechodzi każdy test i w raporcie wygląda jak działający (`R2-WP22`). Powstaje razem z pisarzem albo wcale |
| `CH-5` | **`Policy::MinWage` ma konsumenta i jest nim inspekcja pracy.** Uchwała o płacy minimalnej miała podmieniać **próg**, a nie mechanizm | Zapis informacyjny, którego M8e nie przejęło. Do sprawdzenia w R3: czy uchwała faktycznie przestawia próg, na którym stoi sprawa inspekcji pracy, czy tylko go deklaruje |
| `CH-6` ★ | **`Agency.budget` nie ma pisarza.** Urzędy finansuje `SpendCategory::Administration` bez rozbicia per urząd; realnym ogranicznikiem jest obsada `ServiceKind::Office` | Pole bez pisarza, czyli ta sama klasa co `CD-6`. Albo dostaje pisarza (rozbicie budżetu na pięć urzędów jako decyzja burmistrza), albo znika — trzeciej odpowiedzi nie ma |

Dlaczego to jest w R3, a nie w M12: żadna z pięciu podfaz M12 nie dotyka sieci przesyłowych,
urzędów kontrolnych ani prawa pracy. Wpisanie ich tam byłoby siódmą przeprowadzką tej samej
pozycji — dokładnie tym, co `R2-WP26` miało przerwać.

---

## 3. Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar | Status |
|---|---|---|---|---|
| R3-WP1 | Zakład jako lokal: Etap 7 rozstawia po `Unit` | — | L | `[ ]` |
| R3-WP2 | Obsada z powierzchni lokalu, nie budynku | R3-WP1 | M | `[ ]` |
| R3-WP3 | Rejestr długu: pozycje zamknięte pomiarem, reszta z tematem | — | M | `[ ]` |
| R3-WP4 | `R2-WP35` — opieka nad dzieckiem poniżej wieku szkolnego | — | M | `[ ]` |
| R3-WP5 | `R2-WP38` — motoryzacja idzie za dochodem (`D-N12`) | — | M | `[ ]` |

**R3-WP1.** `utworz_zaklady` rozstawia zakłady po lokalach użytkowych, a nie po parceli;
`SiteSet.by_building` przestaje być odwzorowaniem jeden-do-jednego, co dotyka `rebind_workplaces`
i pięciu czytelników `by_building`. Firm w mieście robi się kilka razy więcej, więc pakiet
dotyka wydajności i bilansu pieniądza — kryterium musi objąć jedno i drugie.
**Kryterium:** stosunek mieszkańców do firm schodzi poniżej 1 : 40 na świecie odniesienia
(4 km, `industrial`, ziarno 1), niezmiennik pieniądza świata domyka się co do grosza,
a czas Etapu 7 nie rośnie więcej niż dwukrotnie.

**R3-WP2.** Obsada liczy się z powierzchni **lokalu**. **Kryterium:** żaden rodzaj zakładu
nie ma mediany obsady większej niż jego normatyw z `data/site_types/`; `it_office` schodzi
z 183 osób na biuro do wartości z tabeli.

**R3-WP3.** Czterdzieści dziewięć pozycji przeniesionych przez `R2-WP26`. Każda kończy się
jednym z trzech: podziałem, znacznikiem `✅` (przekroczenia już nie ma) albo wierszem
„zostaje świadomie" z **datą przeglądu** — trzeciej możliwości pilnuje bramka, więc pozycja
bez żadnej z nich nie przejdzie.
**Kryterium:** `python scripts/struct_guard.py --all` przechodzi, a liczba pozycji rejestru
jest **mniejsza** niż na wejściu R3. Obie liczby w dzienniku.

---

## 4. Kryteria akceptacji

1. Stosunek mieszkańców do firm na świecie odniesienia poniżej 1 : 40, zmierzony, nie oszacowany.
2. Niezmiennik pieniądza świata domyka się co do grosza po rozdrobnieniu.
3. `struct_guard --all` i `plan_guard` przechodzą; rejestr ma mniej pozycji niż na wejściu.
4. `cargo test --workspace` i `clippy -D warnings` po każdym commicie.

---

## 5. Decyzje otwarte

**`D-R3-1` — Czy lokal użytkowy dostaje własną encję, czy zostaje indeksem w budynku.**
Propozycja: **indeksem** — `Unit` już istnieje w danych budynku i niesie powierzchnię,
a encja na lokal to ~4 tys. encji więcej na świat 4 km bez drugiego konsumenta.
*Blokująca dla R3-WP1.*

---

## Zmiany wpisane po R3

Zgodnie z `K-18`.

| # | Zmiana | Dlaczego |
|---|---|---|
| | *(tabela wypełnia się w trakcie R3)* | |
