# R2e — Dług strukturalny, martwy kod i dokumentacja

Podfaza dokumentu naprawczego `R2-naprawy-po-M11.md`. Numeracja sekcji `5.x` jest numeracją
dokumentu R2.

| | |
|---|---|
| **Wejście** | **Wszystkie pozostałe podfazy R2 zamknięte.** R2-WP20 dotyka 370 miejsc w dziesięciu crate'ach i każdy wcześniejszy pakiet, który dokłada powód decyzji, powiększyłby jego zakres. R2-WP21…R2-WP23 nie mają tego ograniczenia i mogą pójść wcześniej. |
| **Pakiety robocze** | R2-WP20…R2-WP23 |
| **Wynik do pokazania** | `python scripts/struct_guard.py --all` bez ani jednego przekroczenia progu błędu, oraz `engine/ui/src/inspect/reason.rs` rozpadnięty na trzy pliki, z których żaden nie przekracza 400 linii. |
| **Kryterium zamknięcia** | Kryteria R2-WP20…R2-WP23 plus: **rejestr długu strukturalnego nie ma pozycji bez adresata**, a każda pozycja zamknięta przez R2 ma przekreślenie i wiersz w „Zmiany wpisane po R2". |
| **Poprzednia / następna** | `R2d-domkniecie-swiata.md` / `R2f-pomiar-i-bramki.md` |

---

## 5.13 Dlaczego dług strukturalny wraca po drugiej podfazie naprawczej

R1 zbudował kontrolę strukturalną, ustalił progi i rozciął dwanaście plików. Potem zostawił rejestr
czterdziestu jeden pozycji z zasadą: pozycja zostaje w tabeli, ma adresata i znika, gdy adresat ją
zamknie.

Po sześciu fazach widać, jak ta zasada wytrzymała próbę. Wytrzymała w większości — trzydzieści
pozycji ma adresata i czeka. Nie wytrzymała w czterech przypadkach i każdy jest innym rodzajem
porażki:

- **Adresat został skreślony i nikt nie wpisał nowego.** Pozycja 37 (`reason.rs::describe`) miała
  adres M7c; adres znikł, pozycja została „bez właściciela fazowego". Funkcja urosła w tym czasie
  z 319 do 730 linii.
- **Adresata nigdy nie było i jest to zapisane wprost.** Pozycje 33–35 (`lsystem.rs`): „dziś nie
  planuje go żadna z M6–M12".
- **Wyzwalacz nie strzelił.** Pozycje 24 (`generate_city`, adres M6a) i 38 (`EconomyData::load`,
  adres „pierwsza faza dopisująca czytnik do `retail.ron`"). Fazy dopisywały; wyzwalacz nie
  zadziałał — raz przy pierwszej, trzy razy przy drugiej, z dwoma sprostowaniami w rejestrze.

Wniosek: reguła „pozycja ma adresata" jest dobra, ale **nie ma egzekutora**. Nic nie sprawdza,
czy adresat istnieje, czy nie został skreślony i czy faza, która go nosi, nie zamknęła się bez
niego. To jest zadanie R2-WP26 w następnej podfazie; tutaj zamykamy same pozycje.

---

## 5.14 Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar | Status |
|---|---|---|---|---|
| R2-WP21 | Generator dróg: rozcięcie `lsystem.rs` | — | M | `[ ]` |
| R2-WP22 | Martwe warianty i nieużywane pola | — | S | `[ ]` |
| R2-WP23 | Dokumentacja wejściowa i zakresy strumieni | — | S | `[ ]` |
| R2-WP20 | Podział `DecisionReason` | wszystkie pozostałe pakiety R2 | L | `[ ]` |

Kolejność w tabeli jest kolejnością wykonania, nie numeryczną — R2-WP20 idzie ostatni z powodu
opisanego w metadanych.

---

### R2-WP20 — Podział `DecisionReason`

**Pozycja wykazu:** 33. Rejestr długu poz. 37. Rozmiar `L` i największy pojedynczy pakiet w R2.

**Przyczyna.** `DecisionReason` jest jednym enumem dla całej gry i to jest dobra decyzja — na niej
stoi `K-12`: brak `#[non_exhaustive]`, `match` bez ramienia `_`, jedno miejsce renderujące.
Konsekwencja jest taka, że **nie da się dodać decyzji, której gracz nie zrozumie**, bo kompilator
nie wypuści powodu bez tekstu w obu językach.

Cena tej gwarancji rośnie liniowo z liczbą faz i płaci ją jedna funkcja. `describe`
w `engine/ui/src/inspect/reason.rs` miała 319 linii przed M6b, 566 po M7c, **730 dziś** — przy
progu błędu kontroli strukturalnej wynoszącym 250. Przyrost wyniósł 70 linii w ciągu jednej sesji
pracy nad M8a i został zmierzony bezpośrednio.

Plan pogłębia problem świadomie i słusznie: `M9c` §5.7 zamyka renderowanie powodu wyczerpującym
`match` bez `_`, a `M10f` WP10.15 wymaga powodu dla stu procent nowych decyzji ośmiu systemów.
M8e, M9c i M10f dokładają warianty **zanim ktokolwiek ten podział zaplanuje**.

**Szew.** Nie rezygnujemy z gwarancji — zmieniamy jej nośnik. `DecisionReason` staje się enumem
sumą trzech enumów po aktorze decyzji (`K-58`):

```
enum DecisionReason { Citizen(CitizenReason), Firm(FirmReason), City(CityReason) }
```

Reguła `K-12` obowiązuje na każdym z trzech osobno. Jedno miejsce renderujące zostaje jako pojęcie,
ale rozpada się na trzy funkcje w trzech plikach, każda poniżej progu ostrzeżenia. Dopisanie powodu
przez fazę dotykającą firm nie powiększa pliku, który obsługuje mieszkańców.

Podział po **aktorze**, a nie po fazie, jest tu kluczowy: faza jest własnością planu i zmienia się,
aktor jest własnością domeny i nie zmienia się. Powód „sklep odmówił, bo brak towaru" należy do
mieszkańca (to on stoi przed pustą półką), a nie do M5.

**Zakres.**

| Co | Gdzie |
|---|---|
| Trzy enumy plus enum suma, wszystkie bez `#[non_exhaustive]` | `engine/core/src/decision.rs` → katalog `decision/` |
| Podział `describe` na trzy funkcje w trzech plikach | `engine/ui/src/inspect/reason.rs` → `reason/{citizen,firm,city}.rs` |
| Przejście ~370 miejsc konstruujących powód | 10 crate'ów |
| Klucze lokalizacji bez zmian — `LocKey` jest interned i nie zależy od kształtu enumu | `data/locale/` |
| Wpis `K-58` w `00` §4a | — |

**Ostrzeżenie o determinizmie.** `DecisionReason` wchodzi do dzienników decyzji firm i do pierścieni
powodów, a te wchodzą do hasha stanu. Zmiana kształtu enumu **zmienia format zapisu gry**. R2 stoi
przed `M12b` (zapis i replay), które format i tak przebudowuje i ma w zakresie `Migration`
i `MigrationRegistry` — dlatego ten pakiet ma tu sens, a w M12 miałby go mniej.

**Kryterium:** `describe` nie istnieje jako jedna funkcja; trzy funkcje ją zastępujące mają poniżej
300 linii każda; `python scripts/struct_guard.py --all` nie zgłasza dla `engine/ui` przekroczenia
progu błędu. Test istniejący — każdy wariant powodu daje niepusty napis w obu językach — musi
przejść bez zmiany treści, tylko z przejściem na nową ścieżkę. Pozycja 37 rejestru dostaje
przekreślenie.

---

### R2-WP21 — Generator dróg: rozcięcie `lsystem.rs`

**Pozycja wykazu:** 34. Rejestr długu poz. 33 ★, 34, 35.

**Przyczyna.** `sim/world/src/city/lsystem.rs`: `impl Builder` ma 605 linii przy progu błędu 500,
`grow_network` 317 przy progu 250, `Builder::grow` 257. Rejestr mówi wprost, że **żadna faza
z M6–M12 tego nie planuje**, a pozycja 33 jest opisana jako „jedyna, która dzieli się dziś za
darmo" i wejście do decyzji o R2 (`D-33` w „Zmiany wpisane po R1").

Innymi słowy: ten pakiet jest powodem, dla którego R1 przewidział, że R2 kiedyś powstanie.

**Szew.** Trzy tematy w jednym pliku i granice między nimi są czyste:

1. **Produkcja** — reguły L-systemu: domknięcie ślepego końca, kontynuacja, odgałęzienie
   z degradacją klasy, rozwidlenie Y, domknięcie pierścienia.
2. **Ograniczenia** — odsiew propozycji w ustalonej kolejności: strefy zakazane, limit spadku
   z sześcioma próbami ratunku, wybór konstrukcji, doklejenie do węzła, przecięcie z istniejącym
   segmentem, kontrola kąta minimalnego.
3. **Kolejka** — `Builder` jako maszyna: priorytet, stan, licznik odrzutów, raport.

Podział mechaniczny, bez zmiany zachowania — czyli w tym jednym pakiecie obowiązuje reguła R1,
a nie R2: **żaden hash nie zmienia się o bit**.

**Zakres.**

| Nowy plik | Zawartość | ~linii |
|---|---|---|
| `lsystem/rules.rs` | pięć reguł produkcji, `Proposal` | ~260 |
| `lsystem/constrain.rs` | odsiew propozycji, próby ratunku, wybór konstrukcji | ~320 |
| `lsystem/mod.rs` | `Builder`, kolejka priorytetowa, `grow_network`, raport | ~290 |

**Ostrzeżenie o determinizmie.** Kolejność ograniczeń jest kontraktem — zmiana kolejności odsiewu
zmienia **każde miasto z każdego ziarna**. Przy przenoszeniu symboli kolejność wywołań musi zostać
zachowana co do kroku, a macierz hashy miasta (32 ziarna × 4 profile) musi wyjść identyczna.

**Kryterium:** macierz hashy miasta identyczna bit w bit przed i po podziale; żaden z trzech plików
nie przekracza progu ostrzeżenia dla pliku ani dla bloku `impl`; `grow_network` poniżej 250 linii.
Pozycje 33–35 rejestru dostają przekreślenie.

---

### R2-WP22 — Martwe warianty i nieużywane pola

**Pozycje wykazu:** 29, 30, 31, 32.

**Przyczyna.** Cztery miejsca, w których typ deklaruje coś, czego nikt nie konstruuje, nie czyta
albo nie obsługuje. Reguła YAGNI z `CLAUDE.md` mówi o tym wprost: brak drugiego konsumenta = brak
abstrakcji. Tutaj bywa gorzej — nie ma pierwszego.

| Co | Stan | Rozstrzygnięcie |
|---|---|---|
| `Carrier::Pipeline`, `PipelineId`, `pipeline_gr_per_tonne` | Zadeklarowane w typie i w strojeniu, bez gałęzi wykonania. `M6c` `AH-3` **twierdzi**, że rurociąg wozi ropę wewnątrz miasta — czyli plan zakłada istnienie ścieżki, której nie ma | Znika z typu (`D-N5`). `M8b` §5.4 buduje rurociągi jako sieci przesyłowe z taryfą i fakturą, co jest innym mechanizmem; drugi, martwy, jest kosztem bez konsumenta |
| `TripPurpose::Escort` | Wariant zadeklarowany, nigdzie nie konstruowany — odprowadzenie jeździ jako zwykły dojazd | Zostaje i **dostaje konstruktora**: odprowadzenie ma inny mnożnik celu podróży (1,50) niż dojazd do pracy (1,30), więc wariant zmienia wynik, tylko nikt go nie użył |
| `Household.shopper_rotation` | Pole w komponencie, czytane wyłącznie przez funkcję haszującą. Rotacja kupującego liczy się z numeru doby | Znika z komponentu. Zmniejsza `Household` i upraszcza hash |
| `Household.vehicle_slots` | Jak wyżej. Flota jest w `sim/traffic`, powiązanie idzie przez `VehicleOwner` | Znika z komponentu |
| `MachineClassId` bez `data/machines/classes.ron` | Świadomy `ponytail:` z nazwaną ścieżką wyjścia; zapisany jako `AF-4 ★` w `M6b`, skutek w `M7d` `BA-4` | Zostaje jako skrót, ale **wyjście dostaje adresata**: katalog powstaje w M12d (modding) razem z pozostałymi katalogami rozszerzalnymi, albo nie powstaje nigdy i wtedy `ponytail:` zmienia się w decyzję |

**Ostrzeżenie o determinizmie.** Usunięcie dwóch pól z `Household` zmienia rozmiar komponentu
i jego reprezentację w hashu — tak samo jak usunięcie wariantu potrzeby w R2-WP16. Oba pakiety
zmieniają format zapisu gry i oba stoją przed M12b.

**Kryterium:** cztery testy odtwarzające, po jednym na temat. Dla `TripPurpose::Escort` —
odprowadzenie dziecka wycenia czas mnożnikiem 1,50, a nie 1,30; przed naprawą oba są równe.
Dla usuwanych pól i wariantów — test statyczny: w kodzie symulacji nie ma wariantu enumu ani pola
publicznego, którego nikt nie konstruuje i nie czyta poza funkcją haszującą. Ten test zostaje
w repozytorium i chroni przed powtórką.

---

### R2-WP23 — Dokumentacja wejściowa i zakresy strumieni

**Pozycje wykazu:** 36, 37 (weryfikacja).

**Przyczyna.** Dwie rzeczy, obie o tym, że **opis rozjechał się z rzeczą**.

`README.md` deklaruje „Stan: **M1 zamknięte** (świat statyczny — teren, woda, biomy, renderer)".
Repozytorium jest po M7f i w trakcie M8a. Flagi klienta i sterowanie są opisane poprawnie;
nagłówek stanu nie był ruszany od pierwszej fazy. To jest jedyny plik, który ktoś z zewnątrz
przeczyta pierwszy.

`StreamId` dla M8: `K-4` przydzielił blok 240–259 z imiennym rozpisaniem (`240 EventHazard` …
`251 Demography`, 252–259 wolne) i `M8a` §5.0 potwierdza, że po M8a nic nie jest zajęte. Plan jest
więc w porządku — usterka, którą zgłosił przegląd, dotyczyła **braku wariantów w enumie**, czyli
niewykonania `K-4`, nie luki w planie. Pakiet sprowadza się do weryfikacji, że wykonanie nastąpiło,
i do dopisania wariantów, jeśli nie.

Trzecia rzecz, znaleziona przy weryfikacji sprostowania i dopisana tutaj: **tabele korekt mają
w repozytorium cztery różne nagłówki** — „Zmiany wpisane po MX" (98 wystąpień), „Korekty planu
wpisane po implementacji MX" (5), „Korekty projektu technicznego MX" (2), „Korekty wpisane
w trakcie MX" (1). Przegląd po samym pierwszym wariancie gubi wpisy; tak zniknęła z wykazu
pozycja 38. Koszt jest realny: wpis, którego nie da się znaleźć, jest wpisem, którego nie ma.

**Zakres.**

| Co | Gdzie |
|---|---|
| Nagłówek stanu w `README.md` z odesłaniem do `00-postep.md`, bez powtarzania listy faz | `README.md` |
| Test CI: deklarowana faza w `README.md` zgadza się z ostatnim odhaczonym wierszem `00-postep.md` | `.github/workflows/ci.yml` + skrypt |
| Weryfikacja wariantów `StreamId` w bloku 240–259; dopisanie brakujących | `engine/core/src/rng.rs` |
| Ujednolicenie nagłówków tabel korekt do „Zmiany wpisane po MX" we wszystkich dokumentach planu | `docs/implementation-plan/*.md` |
| Nazwa joba balansatora w CI mówi „G1–G11", nie „G1–G9" | `.github/workflows/ci.yml` |

**Kryterium:** test CI pada na dzisiejszym `README.md` i przechodzi po poprawce. `grep` po
nagłówkach tabel korekt w `docs/implementation-plan/` zwraca jeden wariant, nie cztery. Test
jednostkowy `StreamId` — żaden numer w bloku 240–259 nie jest użyty dwa razy i każdy wariant
zadeklarowany w `K-4` istnieje w enumie.

---

## 5.15 Decyzje otwarte tej podfazy

**`D-N15` — Czy podział `DecisionReason` idzie po aktorze, czy po fazie.** Propozycja: po aktorze
(`Citizen | Firm | City`). Faza jest własnością planu i zmienia się — powód „sklep odmówił, bo brak
towaru" powstał w M5, a należy do mieszkańca, który stoi przed pustą półką, i tam zostanie na
zawsze. Podział po fazie dałby dziś ten sam rezultat, a za trzy fazy wymagałby przenoszenia
wariantów między enumami. *Blokująca dla R2-WP20.*

**`D-N16` — Czy `MachineClassId` dostaje katalog w M12d, czy zostaje wyprowadzany z receptur.**
Propozycja: dostaje katalog, razem z pozostałymi katalogami rozszerzalnymi moddingu. Klasa maszyny
jest dokładnie tym rodzajem rzeczy, którą modder chce dodać, a dziś nie może — nie istnieje jako
plik. Wariant odwrotny jest tańszy i wymaga wtedy zamiany komentarza `ponytail:` na wiersz
w tabeli korekt `M6b`, żeby przestał obiecywać plik, który nie powstanie. *Nieblokująca.*

---

## Zmiany wpisane po R2e

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu R2e.

| # | Zmiana | Dlaczego |
|---|---|---|
| | *(tabela wypełnia się w trakcie R2e)* | |
