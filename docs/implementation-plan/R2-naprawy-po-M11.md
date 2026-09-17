# R2 — Naprawy po M11

Dokument wykonawczy spoza numeracji faz, drugi po `R1-refaktor-po-M5.md`. Wykonuje się go
**po zamknięciu M11e, przed startem M12a**. Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.

To nie jest faza. Nie ma bramek 1–7, nie wnosi do gry ani jednej nowej zdolności i nie ma sekcji
„dostarczam" — wszystko, co R2 robi, jest naprawą czegoś, co już zostało zadeklarowane jako zrobione.
Ma za to własne pakiety robocze, własne kryterium zamknięcia i własny wpis w dzienniku, tak samo
jak R1.

R1 mierzył jedną rzecz — długość plików — i naprawiał ją jednym ruchem. R2 mierzy co innego:
**rozjazd między tym, co dokumenty faz uznały za zamknięte, a tym, co robi kod**. Wykaz w §11 ma
43 wiersze: 42 z przeglądu repozytorium po M7f i jeden dopisany przy weryfikacji. Jedna pozycja
miała pakiet, jedna okazała się rozstrzygnięta i wypadła, jedenaście stało zapisanych w tabelach
korekt albo w rejestrze długu — **każda bez wykonawcy** — a dwadzieścia dziewięć nie było znanych
planowi w żadnej postaci.

| | |
|---|---|
| **Wejście** | M11e zamknięte (wszystkie bramki fazy M11), `master` zielony, `cargo check --workspace --all-targets` czysty. |
| **Pakiety robocze** | R2-WP1…R2-WP26, rozdzielone na sześć podfaz `R2a`…`R2f` |
| **Wynik do pokazania** | `headless m7-miasto --days 3600` z sekcją „Wykaz R2": dla każdego wiersza wykazu §11 status `zamknięta / przeniesiona / odrzucona / nie dotyczy` z liczbą, która to potwierdza. |
| **Kryterium zamknięcia** | Kryteria R2-WP1…R2-WP26 (§4) plus siedem kryteriów akceptacji z §7. Twarde: **żadna pozycja wykazu §11 nie kończy R2 bez statusu** — zamknięta z testem albo przeniesiona z imiennym adresatem i powodem. |
| **Poprzednia / następna** | `M11e-budzet-klatki.md` / `M12a-pamiec.md` |

---

## 1. Po co to jest i dlaczego akurat tutaj

### Co zmierzono

Jeden przegląd repozytorium na stanie `d2ad731` (po M7f) plus dwa przeglądy celowane — rodzina
i cykl życia, obieg dochodu gospodarstwa. Metoda: odczyt kodu i `data/`, bez zaglądania do planu,
a potem konfrontacja z planem. Wynik:

| Kategoria | Pozycji | Uwaga |
|---|---|---|
| Mechanizm jest napisany, przetestowany i **nie uruchamia się w żadnym wygenerowanym mieście** | 4 | wyczerpywanie złoża, substytucja w kaskadzie niedoboru, zamykanie nierentownej fabryki, cykl szkolny |
| Stan denormalizowany **przestaje być aktualizowany** po zdarzeniu, które go zmienia | 3 | wartość czasu, dochód gospodarstwa po zgonie, majątek przy rozwiązaniu gospodarstwa |
| Ta sama wielkość jest **podana dwa razy i różnie** | 2 | wiek produkcyjny, podstawa liczenia relacji współpracowników |
| Wariant typu istnieje i **nikt go nie konstruuje ani nie obsługuje** | 5 | `Carrier::Pipeline`, `TripPurpose::Escort`, `shopper_rotation`, `vehicle_slots`, `MachineClassId` |
| Model rodziny ma **dziurę, którą widać na karcie inspekcji** | 13 | od braku relacji rodzeństwa po brak sieroctwa |
| Dług strukturalny **bez fazy-właściciela** | 4 | pozycje 24, 33–35, 37, 38 rejestru R1 |
| Bramka albo test, który **nie mierzy tego, co obiecuje** | 4 | filtry bramki G11, widełki G4, testy miasta poza CI, brak testu dochodu |
| Dokumentacja opisuje **stan sprzed sześciu faz** | 1 | `README.md` |

### Dlaczego po M11, a nie wcześniej

Bo M8–M11 są fazami, które **produkują konsumentów** tych mechanizmów, a M12 jest fazą, która
**wszystko mierzy**. Naprawa cyklu szkolnego przed M8d byłaby naprawą w próżnię: szkoła jako
instytucja z pojemnością i rejonizacją powstaje dopiero tam. Naprawa relacji rodzinnych przed
M10e byłaby zgadywaniem, czego związki zawodowe i relacje partnerskie od nich potrzebują.

Z drugiej strony M12 mierzy sesję stuletnią, budżet pamięci metropolii, determinizm odtworzenia
i tryb 50×. **Każdy z tych pomiarów wykonany na świecie z wykazu §11 mierzy fikcję** — sesja
stuletnia na populacji, w której żadne urodzone dziecko nie chodzi do szkoły, mierzy coś innego
niż sto lat życia miasta.

R2 stoi więc dokładnie między ostatnią fazą, która dokłada mechanikę, a pierwszą, która ją wycenia.

### Czego to nie jest

To nie jest przeprojektowanie. Żaden pakiet R2 nie wprowadza mechaniki, której nie ma w PRD ani
w dokumencie jakiejś fazy — R2 domyka to, co już zostało obiecane. Jeśli w trakcie pracy okaże
się, że domknięcie wymaga decyzji projektowej, decyzja ląduje w §9 tego dokumentu z propozycją
domyślną, a nie w kodzie jako nowy wariant.

To nie jest też refaktor. R1 miał zasadę nadrzędną „żaden hash nie zmienił się o bit"; R2 ma
zasadę odwrotną — **prawie każdy pakiet zmienia hash stanu i to jest jego celem**. Stąd inna
procedura (§3).

---

## 2. Zakres — wchodzi / nie wchodzi

### Wchodzi

- Wszystkie wiersze wykazu §11, każdy z jawnym statusem na koniec R2.
- Testy, których brak pozwolił usterce przeżyć. Pakiet bez testu nie jest zamknięty — to jest
  główna różnica wobec faz, gdzie kryterium bywa metryką z przebiegu.
- Poprawki dokumentów faz **wcześniejszych**, których kryterium okazało się niemierzalne (§2b).
- Wpisy `K-n` w `00-konwencje-i-kontrakty.md` §4a dla czterech zmian dotykających kontraktów (§10).
- Rejestr długu w `R1-refaktor-po-M5.md` — pozycje zamknięte przez R2 dostają przekreślenie
  i wiersz w „Zmiany wpisane po R2", numeracja ciągła od **42**.

### Nie wchodzi

- Nic z zakresu M12. Budżet pamięci, replay, tryb 50×, modding i lokalizacja zostają tam, gdzie są.
- Nowe mechaniki rodzinne ponad to, czego wymaga domknięcie: **nie ma tu rozwodu z podziałem
  majątku jako mechaniki, alimentów, adopcji ani opieki zastępczej**. Jest opiekun prawny jako
  minimum, bez którego gospodarstwo z samymi dziećmi jest stanem nieopisanym.
- Przeprojektowanie modelu makro pod dziedziczenie cech. `M10a` §5.8 opiera most makro↔mezo na
  tym, że cechy są funkcją `(world_seed, birth_index)` i nigdy nie są przechowywane — R2 tego
  **nie rusza** i zamyka pozycje 25 i 26 wykazu jako świadomie odrzucone, z powodem (§9, `D-N4`).
- Kalibracja. R2 naprawia mechanizmy; przestrojenie liczb, które po naprawie przestaną pasować,
  należy do balansatora i do M12 §7.4.

### 2b. Czego nie da się odłożyć do R2

To jest najważniejszy akapit tego dokumentu i został napisany przed pierwszym pakietem.

Sześć pozycji wykazu **blokuje kryterium akceptacji fazy wcześniejszej niż R2**. Jeśli zostaną
w R2, tamte fazy zamkną się na bramce, która świeci na zielono, bo nie ma czego zmierzyć:

| Poz. | Co blokuje | Gdzie |
|---|---|---|
| 1, 21 | Test skutku szkoły: mediana umiejętności 18-latków w obwodzie po 10 latach gry, pary A/B | `M8-miasto-jako-aktor.md` §7 T4a · `M8d` otwarte |
| 6, 7 | Kalibracja hazardów zdarzeń na sondach `UnemploymentBps` i `WageGapBps`; wymóg „każda definicja z katalogu zachodzi ≥ 1 raz w 20-letnim przebiegu" | `M8c-zdarzenia.md` §5.5, `M8` R2 · `M8c` otwarte |
| 11 | Warunek powstania związku zawodowego: spójna składowa grafu relacji **wśród pracowników zakładu** ≥ max(8, 25 % załogi) — przy uczniach w indeksie miejsc pracy pierwszą kandydatką jest szkoła | `M10e-relacje-i-zwiazki.md` §5.9 |
| 2 | Skutek strajku w rachunku wyniku zakładu — niewidoczny, dopóki zakład produkcyjny nie ma utargu | `M10e` §5.9, `M7e` `BC-8` · **adres M8a wyczerpany** |

**Adres pozycji 2 właśnie się wyczerpał.** `M7e` `BC-8` wskazywał na M8; M8a i M8b zamknęły się
tego samego dnia, obie bez niej. To jest szósta przeprowadzka tej pozycji (`AR-7` → `AV-2` →
`BB-7` → `BD-6` → `BF-7` → `BC-8`) i najlepszy dowód, że mechanizm „pozycja ma adresata" nie działa
bez egzekutora — czyli dokładnie to, co naprawia R2-WP26.

**Propozycja, którą przyjmuję jako domyślną:** pozycje 1, 2, 7, 11 i 21 są wyjęte z R2 i wykonane
jako **warunek wejścia** odpowiednio M8d (1, 2, 21), M8c (7) i M10e (11) — każda to jeden pakiet
wielkości `S`, dopisany do dokumentu tamtej podfazy zgodnie z `K-18`. M8d jest adresem pozycji 2,
bo to tam powstają urzędy kontrolne, które czytają księgi zakładu, a M8d stoi przed M10e.
Pozycja 6 zostaje w R2, bo jej naprawa jest droższa niż faza, która na nią czeka, i psuje
kalibrację, nie poprawność.

Wiersze w §4 i §11 są oznaczone `⇧` tam, gdzie pozycja jest kandydatem do wyprzedzenia. Decyzja
należy do właściciela produktu (`D-N1`).

---

## 3. Zasada nadrzędna: każda naprawa ma dowód, że usterka istniała

R1 pracował bez zmiany zachowania i sprawdzał to hashem. R2 zmienia zachowanie w każdym pakiecie,
więc hash nie jest dowodem niczego. Zamiast niego obowiązują trzy reguły.

1. **Najpierw test, który pada.** Każdy pakiet zaczyna się od testu odtwarzającego usterkę na
   dzisiejszym kodzie. Test, który przechodzi przed naprawą, opisuje coś innego niż usterka.
2. **Jedna naprawa, jeden powód.** Pakiet naprawia jedną przyczynę. Jeśli w trakcie wychodzi
   druga, dostaje własny wiersz w §11 i własny pakiet — nie dołącza się do trwającego.
3. **Zmiana hasha jest odnotowana.** Pakiet, który zmienia hash stanu, mówi w kryterium
   **dlaczego** i czego dotyczy zmiana. Pakiet, który hasha nie zmienia, też to mówi.

### Procedura dla każdego pakietu

1. Napisz test odtwarzający usterkę; uruchom; **musi paść**. Zapisz w commicie, jak padł.
2. Znajdź przyczynę i napraw ją w jednym miejscu. Jeśli miejsc jest więcej niż jedno, to znaczy,
   że w kodzie jest duplikat wiedzy — usuń duplikat, nie łataj obu kopii.
3. Uruchom test; musi przejść. Uruchom `cargo test --workspace`; musi przejść w całości.
4. Uruchom `python scripts/struct_guard.py --changed` i odpowiedz na progi wg reguły z `CLAUDE.md`.
5. Odhacz pakiet w §4 dokumentu podfazy, dopisz wiersz do §11, dopisz linię do dziennika.

---

## 4. Pakiety robocze

Kolejność jest kolejnością zależności, nie numeryczną. `R2a` i `R2c` można prowadzić równolegle —
nie dotykają tych samych plików. `R2e-WP20` (podział `DecisionReason`) stoi ostatni celowo:
dotyka 370 miejsc i każdy wcześniejszy pakiet, który dokłada powód, powiększyłby jego zakres.

| Podfaza | Dokument | Pakiety | Temat |
|---|---|---|---|
| `R2a` | `R2a-rodzina-i-cykl-zycia.md` | R2-WP1…R2-WP6 | Cykl szkolny, graf rodziny, gospodarstwo, sieroctwo, wykształcenie, tożsamość |
| `R2b` | `R2b-pieniadz-gospodarstwa.md` | R2-WP7…R2-WP11 | Utarg zakładu, majątek przy rozwiązaniu, dochód po zdarzeniu, dziedziczenie, skala ekwiwalentna |
| `R2c` | `R2c-rozjazdy-danych-i-kodu.md` | R2-WP12…R2-WP16 | Wiek produkcyjny, wartość czasu, substytucja, chodniki, martwe potrzeby |
| `R2d` | `R2d-domkniecie-swiata.md` | R2-WP17…R2-WP19 | Kopalnie na złożach, gęstość firm, przechwytywanie rzek |
| `R2e` | `R2e-dlug-i-martwy-kod.md` | R2-WP20…R2-WP23 | `DecisionReason`, generator dróg, martwe warianty, dokumentacja |
| `R2f` | `R2f-pomiar-i-bramki.md` | R2-WP24…R2-WP26 | Filtry bramek G4/G11, testy miasta, egzekutor rejestru długu |

Tabela pakietów z rozmiarami i statusem stoi w dokumencie każdej podfazy. Zbiorczo:

| WP | Nazwa | Podfaza | Zależy od | Rozmiar | Status |
|---|---|---|---|---|---|
| R2-WP1 ⇧ | Cykl szkolny w trakcie gry | R2a | — | M | `[ ]` |
| R2-WP2 | Graf rodziny: rodzeństwo, dziadkowie, ochrona wpisu | R2a | — | M | `[ ]` |
| R2-WP3 | Gospodarstwo bez cichego przepełnienia | R2a | — | S | `[ ]` |
| R2-WP4 | Opiekun prawny i gospodarstwo osierocone | R2a | R2-WP3 | M | `[ ]` |
| R2-WP5 | Wykształcenie jako stan zmienny | R2a | R2-WP1 | M | `[ ]` |
| R2-WP6 | Tożsamość rodzinna: nazwisko i cechy | R2a | R2-WP2 | S | `[ ]` |
| R2-WP7 ⇧ | Utarg zakładu produkcyjnego | R2b | — | M | `[ ]` |
| R2-WP8 | Majątek gospodarstwa przy rozwiązaniu i podziale | R2b | — | M | `[ ]` |
| R2-WP9 | Dochód gospodarstwa po zdarzeniu życiowym | R2b | — | S | `[ ]` |
| R2-WP10 | Dziedziczenie ponad gotówkę osobistą | R2b | R2-WP8 | M | `[ ]` |
| R2-WP11 | Skala ekwiwalentna gospodarstwa | R2b | — | S | `[ ]` |
| R2-WP12 ⇧ | Wiek produkcyjny w jednym miejscu | R2c | — | S | `[ ]` |
| R2-WP13 | Wartość czasu idzie za dochodem | R2c | — | M | `[ ]` |
| R2-WP14 | Szczebel substytucji dostaje wykonawcę | R2c | — | M | `[ ]` |
| R2-WP15 | Chodniki: warstwa piesza bez dróg szybkiego ruchu | R2c | — | S | `[ ]` |
| R2-WP16 | Potrzeby bez martwych slotów | R2c | — | M | `[ ]` |
| R2-WP17 | Kopalnia staje na złożu | R2d | — | M | `[ ]` |
| R2-WP18 | Gęstość firm i pasmo bezrobocia | R2d | R2-WP17, R2-WP12 | L | `[ ]` |
| R2-WP19 | Przechwytywanie rzek w erozji | R2d | — | M | `[ ]` |
| R2-WP20 | Podział `DecisionReason` | R2e | wszystkie pozostałe | L | `[ ]` |
| R2-WP21 | Generator dróg: rozcięcie `lsystem.rs` | R2e | — | M | `[ ]` |
| R2-WP22 | Martwe warianty i nieużywane pola | R2e | — | S | `[ ]` |
| R2-WP23 | Dokumentacja wejściowa i zakresy strumieni | R2e | — | S | `[ ]` |
| R2-WP24 | Bramka bezrobocia naprawdę mierzy bezrobocie | R2f | — | M | `[ ]` |
| R2-WP25 | Testy miasta wychodzą z `#[ignore]` | R2f | — | M | `[ ]` |
| R2-WP26 | Egzekutor rejestru długu | R2f | — | S | `[ ]` |

`⇧` = kandydat do wyprzedzenia przed R2 zgodnie z §2b.

---

## 5. Czego nie ruszamy i dlaczego

| Rzecz | Dlaczego zostaje |
|---|---|
| Cechy mieszkańca jako funkcja `(world_seed, birth_index)` | Na tym stoi most makro↔mezo z `M10a` §5.8 i budżet 8 B na mieszkańca w `MacroCell`. Dziedziczenie osobowości i zmiana nazwiska po ślubie wymagają stanu, którego ten model z założenia nie trzyma — pozycje 25 i 26 zamykamy jako odrzucone, nie jako naprawione |
| Limit 32 relacji na osobę | Sufit jest zmierzony i wchodzi do budżetu pamięci z `M12a`. R2 zmienia **regułę wyboru ofiary**, nie pojemność |
| Ciąża 270 dób, pasma płodności i umieralności | Liczby są skalibrowane i zdają test stuletni. R2 nie dotyka tabel demograficznych |
| Kaskada niedoboru jako drabina | Kształt jest świadomy i opisany. R2 dokłada wykonawcę szóstemu szczeblowi, nie przebudowuje drabiny |
| `WORKING_AGE` jako pojęcie | Wiek produkcyjny ma prawo różnić się od wieku pracy — statystyka publiczna liczy inaczej niż kadry. R2 sprawia, że jest **jedną wartością z danych**, a nie dwiema stałymi w kodzie |

Reguła: naprawiamy rozjazd między deklaracją a zachowaniem, a nie decyzję, która była świadoma
i zapisana.

---

## 6. Co to zmienia w dokumentach faz wcześniejszych

Zgodnie z `K-18` wiedza zdobyta teraz wraca do dokumentu, którego dotyczy — nie czeka, aż ktoś
do niego dojdzie. R2 wymaga sześciu wpisów w dokumentach wcześniejszych i **żaden z nich nie jest
przeprojektowaniem tamtej fazy**:

| Dokument | Wpis |
|---|---|
| `M8-miasto-jako-aktor.md` | Test T4a jest niemierzalny, dopóki nie zamknie się poz. 1 i 21 wykazu. Do §7 wiersz z warunkiem wejścia |
| `M8d-uslugi-i-prawo.md` | Trzy pakiety `S` przejęte z R2: poz. 1 (cykl szkolny), 21 (wykształcenie jako stan) i 2 (utarg zakładu produkcyjnego) |
| `M8a-pieniadz-publiczny.md` | Poz. 2 (`BC-8` z M7e) miała tu adres i M8a zamknęło się bez niej. Wiersz w tabeli korekt: adres przeniesiony na M8d, z liczbą przeprowadzek |
| `M8c-zdarzenia.md` | Kalibracja hazardów społecznych na bezrobociu 0,2 % opisze fikcję. Do §5.5 warunek: sondy kalibrowane po zamknięciu poz. 6; pakiet `S` na poz. 7 (wiek produkcyjny z danych) |
| `M10e-relacje-i-zwiazki.md` | Warunek składowej grafu wśród pracowników zakładu liczy dziś uczniów. Do §5.9 wiersz o poz. 11 |
| `M6-lancuch-dostaw.md` | `AH-3` twierdzi, że `Carrier::Pipeline` wozi ropę wewnątrz miasta — ścieżki wykonania nie ma. Sprostowanie w tabeli korekt |
| `R1-refaktor-po-M5.md` | Pozycje 24, 33, 34, 35, 37, 38 rejestru dostają adresata `R2` zamiast pustego pola albo „żadna z M6–M12" |

---

## 7. Kryteria akceptacji

1. **Każda pozycja wykazu §11 ma status.** Zamknięta z testem, przeniesiona z imiennym adresatem
   i powodem, albo odrzucona z powodem. Pozycja bez statusu blokuje zamknięcie R2.
2. **Każdy zamknięty pakiet zostawił test, który padał przed naprawą.** Commit pakietu zawiera
   zapis, jak test padł — sama treść testu nie wystarcza.
3. **`cargo test --workspace` i `clippy -D warnings` przechodzą po każdym commicie**, nie tylko
   na końcu. Reguła jednej gałęzi z `CLAUDE.md` obowiązuje bez zmian.
4. **Przebieg dziesięcioletni `headless m7-miasto --days 3600` domyka cztery niezmienniki**,
   które dziś nie są sprawdzane: populacja uczniów rośnie razem z populacją dzieci; suma sald
   gospodarstw plus rejestr emisji zgadza się co do grosza po tysiącu rozwiązanych gospodarstw;
   liczba zakładów produkcyjnych z niezerowym utargiem równa się liczbie zakładów produkcyjnych;
   żadna kopalnia nie stoi poza złożem.
5. **Bramka G11 świeci czerwono na dzisiejszym świecie i zielono po R2-WP18.** To jest test
   samej bramki: przy bezrobociu 0,2 % ma padać, a jeśli nie pada, to jej filtry ją wyłączają
   i trzeba je nazwać. G4 przestaje być doradcza albo dostaje nową widełkę z pomiarem.
6. **Rejestr długu strukturalnego nie ma pozycji bez adresata.** Pozycja bez fazy-właściciela jest
   błędem bramki `struct_guard`, nie wpisem w tabeli.
7. **`README.md` opisuje stan repozytorium**, a nie stan sprzed sześciu faz. Weryfikacja: test CI
   porównuje deklarowaną fazę z ostatnim odhaczonym wierszem `00-postep.md`.

---

## 8. Ryzyka i mitygacje

| # | Ryzyko | Mitygacja |
|---|---|---|
| N-1 | Naprawa cyklu szkolnego zmienia rozkład populacji i przewraca kalibrację M8–M10 | Pakiet R2-WP1 kończy się przebiegiem dziesięcioletnim z porównaniem przed/po; rozjazdy ponad 10 % idą do balansatora jako zadanie kalibracyjne, nie do kodu jako współczynnik |
| N-2 | Podział `DecisionReason` zmienia format zapisu gry i unieważnia zapisy z M11 | R2-WP20 stoi **przed** M12b (zapis i replay) i po ostatnim pakiecie dokładającym powód; migracja formatu jest zakresem M12b i tam ma swój wpis |
| N-3 | Gęstość firm (R2-WP18) okaże się nie do naprawienia bez przeprojektowania Etapu 7 | Pakiet ma jawny sufit: jeśli po dwóch dniach pracy stosunek nie schodzi poniżej 1 : 40, pakiet kończy się **decyzją otwartą z pomiarem**, nie kodem. Wtedy przenosi się do R3 z adresatem |
| N-4 | Sześć pozycji z §2b zostanie w R2, a fazy M8/M10 zamkną się na bramkach, których nie da się zmierzyć | Decyzja `D-N1` jest blokująca dla startu M8a i musi zapaść przed nim, nie przed R2 |
| N-5 | Testy miasta wyjęte z `#[ignore]` wydłużą CI ponad okno | R2-WP25 buduje świat 2 km, nie 4 km; jeśli to nie wystarczy, testy idą do joba nocnego z własnym progiem, a nie wracają do `#[ignore]` |
| N-6 | Naprawa wartości czasu zmieni udziały środków transportu poza bramkę metropolii | R2-WP13 kończy się pomiarem udziałów wobec widełek z `data/roads/mode_choice.ron`; wyjście poza widełki jest zadaniem kalibracyjnym z własnym wierszem, nie powodem do cofnięcia naprawy |
| N-7 | R2 rozrośnie się o rzeczy znalezione w trakcie | Reguła 2 z §3: znalezisko dostaje wiersz w §11 i własny pakiet, a pakiet powstaje tylko wtedy, gdy mieści się w kryterium zamknięcia R2. Inaczej idzie do R3 |

---

## 9. Decyzje otwarte

Konwencja numeracji: `D-Nn`, gdzie `N` znaczy „naprawy" — tak samo jak `D-Rn` w R1 znaczyło
„refaktor". Decyzje R2 nie mieszają się z korektami `K-18`, które mają własne tabele na końcu.

**`D-N1` — Czy sześć pozycji z §2b wyprzedza R2.** Propozycja: pozycje 1, 2, 7 i 11 wychodzą
z R2 i stają się warunkiem wejścia odpowiednio M8d, M8a, M8c i M10e, każda jako pakiet `S`
dopisany do dokumentu tamtej podfazy. Pozycje 6 i 21 zostają w R2. Argument za wyprzedzeniem:
bez nich cztery kryteria akceptacji faz M8 i M10 są tożsamościowo spełnione, czyli mierzą własny
brak. Argument przeciw: cztery pakiety wpadają do faz, których zakres jest już zamknięty, i każda
z nich urośnie o dzień pracy. *Blokująca dla startu M8a.*

**`D-N2` — Czy opiekun prawny jest encją, czy polem gospodarstwa.** Propozycja: polem — indeks
mieszkańca w `Household`, ustawiany przy śmierci ostatniego dorosłego, wybierany spośród dorosłych
o relacji `Parent`/`Sibling`/`Grandparent` z którymkolwiek z dzieci, a przy braku takiego — spośród
dorosłych w dzielnicy z najwyższą wagą relacji. Wariant z encją („instytucja opiekuńcza") daje
więcej, ale wymaga usługi publicznej, czyli wchodzi w zakres M8d i tam powinien powstać.
*Blokująca dla R2-WP4.*

**`D-N3` — Czy `edu_level` rośnie w trakcie gry, czy zostaje cechą startową.** Propozycja: rośnie,
ale wyłącznie w dwóch momentach — ukończenie szkoły w wieku `school_end` i ukończenie kursu
wykupionego komendą gracza (`EnrollCourse` z `M9a` §5.5). Wariant ciągły („wykształcenie rośnie
z każdą dobą w szkole") wymaga trzeciego pola na mieszkańca i nie ma konsumenta, który by go
odróżnił od skokowego. *Blokująca dla R2-WP5.*

**`D-N4` — Czy zamykamy pozycje 25 i 26 jako odrzucone.** Propozycja: tak. Dziedziczenie osobowości
po rodzicach i zmiana nazwiska po ślubie wymagają stanu per mieszkaniec, a `M10a` §5.8 opiera
most makro↔mezo na tym, że cechy są funkcją ziarna i nigdy nie są przechowywane. Koszt jest
realny (8 B na mieszkańca w `MacroCell` plus migracja formatu zapisu), zysk jest kosmetyczny.
Jeśli decyzja pójdzie odwrotnie, obie pozycje przenoszą się do M10a jako zmiana kontraktu, nie
do R2. *Nieblokująca — domyślne przyjęcie wystarczy.*

**`D-N5` — Czy `Carrier::Pipeline` dostaje ścieżkę wykonania, czy znika z typu.** Propozycja:
znika. `M8b` §5.4 buduje rurociągi jako **sieci przesyłowe z taryfą i fakturą**, co jest innym
mechanizmem niż przewóz partii towaru; utrzymywanie drugiego, martwego, jest kosztem bez
konsumenta. `M6c` `AH-3` wymaga wtedy sprostowania (§6). Jeśli decyzja pójdzie odwrotnie,
pakiet R2-WP22 rośnie z `S` do `M`. *Blokująca dla R2-WP22.*

**`D-N6` — Sufit pracy nad gęstością firm.** Propozycja: dwa dni pracy, próg akceptacji
1 : 40 mieszkańców na firmę wobec obiecanych 1 : 15…1 : 25. Po przekroczeniu sufitu pakiet
kończy się pomiarem i decyzją, nie kodem. Powód sufitu: Etap 7 jest jedynym miejscem w projekcie,
w którym naprawa może wymagać przeprojektowania, a nie domknięcia — a wtedy należy do własnej
fazy, nie do dokumentu napraw. *Blokująca dla R2-WP18.*

---

## 10. Wpisy wymagane w `00-konwencje-i-kontrakty.md` §4a

Cztery zmiany R2 dotykają kontraktów z dokumentu 00 i zgodnie z jego §4a wymagają wpisu `K-n`.
Pierwszy wolny numer to **`K-58`**. Wpisy powstają w commicie pakietu, który zmianę wprowadza,
a nie z góry.

| Numer | Pakiet | Treść |
|---|---|---|
| `K-58` | R2-WP20 | `DecisionReason` dzieli się na `CitizenReason \| FirmReason \| CityReason` pod wspólnym enumem-sumą. Reguła `K-12` (wyczerpujący `match` bez `_`) obowiązuje na każdym z trzech osobno; jedno miejsce renderujące zostaje, ale rozpada się na trzy funkcje |
| `K-59` | R2-WP2 | `RelationKind` dostaje wariant `Grandparent`. Kolejność wariantów jest kontraktem zapisu gry — dopisywać wolno wyłącznie na końcu |
| `K-60` | R2-WP12 | Wiek produkcyjny jest **daną**, nie stałą w kodzie. Jedno źródło: `data/demography/demography.ron`, pole `ages.labour_force`. Statystyka bezrobocia i model makro czytają to samo pole |
| `K-61` | R2-WP8 | Rozwiązanie gospodarstwa domowego jest **operacją księgową**: salda przechodzą do spadkobierców albo na konto techniczne, nigdy nie znikają razem z encją. Niezmiennik P1 obejmuje gospodarstwa |

---

## 11. Wykaz — 43 wiersze

Numeracja jest numeracją przeglądu i nie zmienia się. Kolumna „Plan" mówi, co wiedziały dokumenty
przed R2: `—` = nieznane planowi, `zapis` = zapisane jako znana usterka bez wykonawcy,
`WP` = miało pakiet. Kolumna „Status" wypełnia się w trakcie R2.

| # | Usterka | Plan | Pakiet | Status |
|---|---|---|---|---|
| 1 ⇧ | Dziecko urodzone w grze nie dostaje flagi ucznia ani szkoły | — | R2-WP1 | `[ ]` |
| 2 ⇧ | Zakład produkcyjny nigdy nie ma utargu; tier taktyczny go nie zamknie | zapis `M7e` `BC-8` | R2-WP7 | `[ ]` |
| 3 | Szczebel `Substituted` kaskady ma puste ramię `match` | zapis `M6c` `AG-6` | R2-WP14 | `[ ]` |
| 4 | Wartość czasu zamrożona na stanie z generacji świata | — | R2-WP13 | `[ ]` |
| 5 | Zero zakładów wydobywczych ze złożem w mieście 4 km | zapis `M6e` `AQ-8` | R2-WP17 | `[ ]` |
| 6 | Gęstość firm ~10× za niska; bezrobocie 0,2 % przy 12 032 wakatach | zapis `00-postep` `BF-4`/`BF-10` | R2-WP18 | `[ ]` |
| 7 ⇧ | `WORKING_AGE` w kodzie (2 miejsca) vs `work_start`/`retirement` w danych | — | R2-WP12 | `[ ]` |
| 8 | Rodzeństwo z zasiedlenia i napływu bez relacji `Sibling` | — | R2-WP2 | `[ ]` |
| 9 | Babcia dostaje z wnukiem relację `Sibling`; brak `Grandparent` | — | R2-WP2 | `[ ]` |
| 10 | Dziecko urodzone w pełnym gospodarstwie nie wchodzi do listy członków | — | R2-WP3 | `[ ]` |
| 11 ⇧ | Uczeń wchodzi do indeksu miejsc pracy i dostaje relacje `Colleague` | zapis `M3d` `E-19` (przyczyna) | R2-WP1 | `[ ]` |
| 12 | Warstwa piesza dopuszcza drogi szybkiego ruchu | — | R2-WP15 | `[ ]` |
| 13 | Motoryzacja to płaska stawka 430 ‰ bez związku z dochodem | zapis `M4b` `L-12` | R2-WP13 | `[ ]` |
| 14 | Potrzeba `Status`: tempo 0, brak miejsc, `StatusLoss` pusty | zapis `M3a` `D-15` | R2-WP16 | `[ ]` |
| 15 | `ProductivityLoss` i `AmbitionGain` jawnie puste | zapis `M7b` ★ | R2-WP16 | `[ ]` |
| 16 | Majątek gospodarstwa przepada przy rozwiązaniu | zapis `M3c` `G-7` (tylko lokal i etat) | R2-WP8 | `[ ]` |
| 17 | Śmierć pracownika może zostawić płacę w `income_monthly` | zapis `M7b` ★ (tylko etat) | R2-WP9 | `[ ]` |
| 18 | `InheritanceHook` to zaślepka; dziedziczona tylko gotówka osobista | decyzja otwarta `M3` §9.10 | R2-WP10 | `[ ]` |
| 19 | Wyprowadzka z gniazda i rozstanie nie przenoszą środków | — | R2-WP8 | `[ ]` |
| 20 | Brak opiekuna, kurateli i sieroctwa | — | R2-WP4 | `[ ]` |
| 21 | Nikt nie zdobywa wykształcenia w trakcie gry | — | R2-WP5 | `[ ]` |
| 22 | Brak żłobka i przedszkola dla dzieci 0–6 lat | zakres `M3` §2 → M8 | R2-WP5 | `[ ]` |
| 23 | Dziecko konsumuje tyle co dorosły | — | R2-WP11 | `[ ]` |
| 24 | Relacja rodzinna nie chroniona przed wypchnięciem z slabu | — | R2-WP2 | `[ ]` |
| 25 | Osobowość noworodka nie jest dziedziczona | — | odrzucone `D-N4` | `[ ]` |
| 26 | Brak zmiany nazwiska po ślubie | — | odrzucone `D-N4` | `[ ]` |
| 27 | Maksymalnie czworo dzieci odprowadzanych, piąte pomijane | — | R2-WP3 | `[ ]` |
| 28 | Brak opieki nad starszymi | — | R2-WP4 | `[ ]` |
| 29 | `Carrier::Pipeline` bez ścieżki wykonania | zapis `M6c` `AH-3` (błędny) | R2-WP22 | `[ ]` |
| 30 | `TripPurpose::Escort` nigdzie nie konstruowany | — | R2-WP22 | `[ ]` |
| 31 | `shopper_rotation` i `vehicle_slots` nieużywane | — | R2-WP22 | `[ ]` |
| 32 | `MachineClassId` bez katalogu danych | zapis `M6b` `AF-4` | R2-WP22 | `[ ]` |
| 33 | `reason.rs::describe` 730 linii przy progu 250 | zapis rejestr poz. 37 | R2-WP20 | `[ ]` |
| 34 | Generator dróg ponad progami, bez fazy otwierającej | zapis rejestr poz. 33–35 | R2-WP21 | `[ ]` |
| 35 | Wyzwalacze długu nie zadziałały (`generate_city`, `EconomyData::load`) | zapis rejestr poz. 24, 38 | R2-WP26 | `[ ]` |
| 36 | `README.md` opisuje stan „M1 zamknięte" | — | R2-WP23 | `[ ]` |
| 37 | M8 bez wariantów `StreamId` w przydzielonym bloku | WP `M8a` §5.0 (`K-4`) | R2-WP23 (weryfikacja) | `[ ]` |
| 38 | ~~Profil `ci` balansatora nie mieści się w 10 minutach~~ — **nie jest usterką** | rozstrzygnięte `M5e` `AD-6 ★` | — | `nie dotyczy` |
| 38b | Bramka G11 (bezrobocie 3–12 %) chodzi tylko nocą i nie zapala się przy 0,2 % | — | R2-WP24 | `[ ]` |
| 39 | Bramka G4 doradcza — 5 dób wobec widełek 14–56 | zapis `00-postep` M7 | R2-WP24 | `[ ]` |
| 40 | Większość testów miasta `#[ignore]` | zapis `R1` `D-R7` | R2-WP25 | `[ ]` |
| 41 | Brak testu `income_monthly` po zdarzeniu życiowym | — | R2-WP9 | `[ ]` |
| 42 | Erozja nie przechwytuje rzek | — | R2-WP19 | `[ ]` |

**Bilans wejściowy:** 1 pozycja miała pakiet (37), 1 okazała się rozstrzygnięta i wypadła
z wykazu (38), 11 było zapisanych jako znana usterka **bez wykonawcy**, a 29 nie było znanych
planowi w żadnej postaci. Doszła jedna pozycja znaleziona przy weryfikacji sprostowania (38b).

Dziura jest jednorodna, a nie zbieraniną: **18 z 29 nieznanych** to demografia, rodzina, cykl
życia i majątek gospodarstwa (1, 7–12, 16–28, 31). Reszta rozkłada się na higienę repozytorium
i testów (36, 41, 42) oraz ruch (4). To jest powód, dla którego `R2a` i `R2b` są najobszerniejszymi
podfazami, a `R2e` i `R2f` najkrótszymi.

**Uwaga metodyczna do przyszłych przeglądów.** Tabele korekt mają w tym repozytorium **cztery
różne nagłówki**: „Zmiany wpisane po MX" (98 wystąpień), „Korekty planu wpisane po implementacji
MX" (5), „Korekty projektu technicznego MX" (2) i „Korekty wpisane w trakcie MX" (1). Przegląd
po samym pierwszym gubi część wpisów — tak zniknęła pozycja 38, zapisana pod czwartym wariantem.
Ujednolicenie nagłówków jest zadaniem R2-WP23.

---

## 12. Szacunek wielkości

| Podfaza | Pakiety | Pliki dotknięte | Testy nowe | Rozmiar |
|---|---|---|---|---|
| `R2a` | 6 | ~14 w `sim/agents`, `sim/world/population` | 11 | L |
| `R2b` | 5 | ~9 w `sim/economy`, `sim/agents`, `sim/firms` | 8 | M |
| `R2c` | 5 | ~11 w `sim/traffic`, `sim/supply`, `sim/agents`, `data/` | 9 | M |
| `R2d` | 3 | ~7 w `sim/world` | 5 | L |
| `R2e` | 4 | ~370 miejsc w 10 crate'ach (sam R2-WP20) | 4 | L |
| `R2f` | 3 | `tools/balansator`, `sim/world/tests`, `scripts/` | 3 | M |
| **Razem** | **26** | — | **40** | — |

Szacunek liczby testów jest dolną granicą: kryterium akceptacji nr 2 wymaga testu, który padał,
dla **każdego** pakietu, a pakiety wielotematyczne (R2-WP2, R2-WP22) potrzebują go dla każdego
tematu osobno.

---

## Zmiany wpisane po R2

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu R2. Numeracja
`N-n`, gwiazdka `★` przy numerze = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| | *(tabela wypełnia się w trakcie R2)* | |
