# R2 — Naprawy po M11

Dokument wykonawczy spoza numeracji faz, drugi po `R1-refaktor-po-M5.md`. Wykonuje się go
**po zamknięciu M11e, przed startem M12a**. Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.

To nie jest faza. Nie ma bramek 1–7, nie wnosi do gry ani jednej nowej zdolności i nie ma sekcji
„dostarczam" — wszystko, co R2 robi, jest naprawą czegoś, co już zostało zadeklarowane jako zrobione.
Ma za to własne pakiety robocze, własne kryterium zamknięcia i własny wpis w dzienniku, tak samo
jak R1.

R1 mierzył jedną rzecz — długość plików — i naprawiał ją jednym ruchem. R2 mierzy co innego:
**rozjazd między tym, co dokumenty faz uznały za zamknięte, a tym, co robi kod**. Wykaz w §11 ma
**86 pozycji** i powstał w ośmiu rzutach — ósmy jest inny od siedmiu poprzednich, bo nie czytał ani kodu, ani zapisu sesji: **wszystkie sześć pozycji R2f wyszło z uruchomienia bramek, które przestały kłamać** (81, 82, 83, 84, 85 i pomiar przy 57). Powstał (dwa wiersze wyszły z wykonania pozycji 1, 2 i 21 przed R2, dwa dopisała M11c, trzy — R2b): 42 z przeglądu repozytorium po M7f, jeden dopisany
przy weryfikacji, trzy z przeglądów w trakcie M8, **dziesięć z recenzji przed commitem M9d**
(pozycje 47–56), jeden z M9e (57), **osiem z przeglądu sesji po M9e** (pozycje 58–65), jeden z gry uruchomionej
po M9e (68) i **dwa z M11b** (70–71) — obie wyszły dopiero wtedy, gdy pieszy przestał być plamką
kilku pikseli i dostał sylwetkę.
Z pierwszego rzutu jedna pozycja miała pakiet, jedna okazała się rozstrzygnięta i wypadła,
jedenaście stało zapisanych w tabelach korekt albo w rejestrze długu — **każda bez wykonawcy** —
a dwadzieścia dziewięć nie było znanych planowi w żadnej postaci.

**Rzut z M9d różni się od poprzednich i dlatego warto go opisać osobno.** Nie wyszedł z przeglądu
repozytorium, tylko z **recenzji jednej podfazy przed jej commitem** — i znalazł dziesięć pozycji,
z których żadna nie miała wykonawcy. Osiem z nich dotyczy rzeczy, które stały w kodzie od M6–M7c
i przeżyły po kilka podfaz: wariant akcji bez wykonawcy, pole bez czytelnika, typ bez arytmetyki,
konwencja nazw łamana w każdym crate'cie. To jest miara skuteczności recenzji przedcommitowej
wobec przeglądu okresowego: **recenzja jednej podfazy dała jedną czwartą wykazu R2**. Wniosek
dla R2-WP26 (egzekutor rejestru długu): próg wychwytu leży w kadencji przeglądu, a nie w jego
głębokości.

**Rzut czwarty (58–65) nie czytał w ogóle kodu — czytał zapis sesji.** Metoda: wszystkie prompty
i końcowe podsumowania 62 sesji od 13 do 18 września, zestawione z planem. Pytanie brzmiało
„co zostało powiedziane i nigdzie nie zapisane", a nie „co jest zepsute". Wynik jest inny
jakościowo od trzech poprzednich rzutów: **z ośmiu pozycji sześć było w dokumentach zapisanych,
tylko pod adresem, który nie istnieje albo już minął**. Dwie z nich miały jawny wiersz „idzie do
R2" w tabeli korekt fazy, której R2 nigdy nie zobaczyło (poz. 58, 59), a dwie były otwartą
decyzją w dokumencie, który sam się zamknął (poz. 60, 61). To jest ta sama klasa co cztery
pozycje rejestru długu z §5.13 `R2e` — z tą różnicą, że rejestr długu ma bramkę, a tabele korekt
nie mają żadnej. Stąd poz. 64 i rozszerzenie R2-WP26.

| | |
|---|---|
| **Wejście** | M11e zamknięte (wszystkie bramki fazy M11), `master` zielony, `cargo check --workspace --all-targets` czysty. |
| **Pakiety robocze** | R2-WP1…R2-WP34, rozdzielone na sześć podfaz `R2a`…`R2f` |
| **Wynik do pokazania** | `headless m7-miasto --days 3600` z sekcją „Wykaz R2": dla każdego wiersza wykazu §11 status `zamknięta / przeniesiona / odrzucona / nie dotyczy` z liczbą, która to potwierdza. |
| **Kryterium zamknięcia** | Kryteria R2-WP1…R2-WP34 (§4) plus siedem kryteriów akceptacji z §7. Twarde: **żadna pozycja wykazu §11 nie kończy R2 bez statusu** — zamknięta z testem albo przeniesiona z imiennym adresatem i powodem. |
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
| 6, 7 | Kalibracja hazardów zdarzeń na sondach `UnemploymentBps` i `WageGapBps`; wymóg „każda definicja z katalogu zachodzi ≥ 1 raz w 20-letnim przebiegu" | `M8c-zdarzenia.md` §5.5, `M8` R2 · **poz. 7 wykonana w M8c** (`K-60`), poz. 6 zostaje w R2 |
| 11 | Warunek powstania związku zawodowego: spójna składowa grafu relacji **wśród pracowników zakładu** ≥ max(8, 25 % załogi) — przy uczniach w indeksie miejsc pracy pierwszą kandydatką jest szkoła | `M10e-relacje-i-zwiazki.md` §5.9 |
| 2 | Skutek strajku w rachunku wyniku zakładu — niewidoczny, dopóki zakład produkcyjny nie ma utargu | `M10e` §5.9, `M7e` `BC-8` · **adres M8a wyczerpany** |

**Adres pozycji 2 właśnie się wyczerpał.** `M7e` `BC-8` wskazywał na M8; M8a i M8b zamknęły się
tego samego dnia, obie bez niej. To jest szósta przeprowadzka tej pozycji (`AR-7` → `AV-2` →
`BB-7` → `BD-6` → `BF-7` → `BC-8`) i najlepszy dowód, że mechanizm „pozycja ma adresata" nie działa
bez egzekutora — czyli dokładnie to, co naprawia R2-WP26.

**Wykonane 2026-09-18: pozycje 1, 2 i 21.** Cykl szkolny z placówką, wykształcenie jako stan
zmienny (`R2-WP1`, `R2-WP5`, `K-74`) i utarg zakładu produkcyjnego (`R2-WP7`, `K-75`).
Test T4a fazy M8 ma od tej chwili co mierzyć: osiemnastolatek urodzony w grze kończy szkołę
z `edu_level` z tabeli, a nie z zerem. **Adres wygasł po raz siódmy i nikt tego nie zauważył**
— M8d zamknęło się bez nich, a wykonano je dopiero na polecenie właściciela produktu, po M9e.
To jest dokładnie ten wzorzec, który `R2-WP26` ma złapać, i trzeci jego przypadek w wykazie.
Zostaje **jedna** pozycja wyprzedzająca: 11, adres `M10e`. Po `R2-WP1` jest częściowo
rozbrojona — uczeń wypadł z indeksu miejsc pracy, więc szkoła przestała być pierwszą
kandydatką do związku zawodowego; `M10e` zostaje z samym warunkiem.

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
| `R2a` | `R2a-rodzina-i-cykl-zycia.md` | R2-WP1…R2-WP6, R2-WP35, R2-WP37 | Cykl szkolny, graf rodziny, gospodarstwo, sieroctwo, wykształcenie, tożsamość, grafik zmianowy |
| `R2b` | `R2b-pieniadz-gospodarstwa.md` | R2-WP7…R2-WP11, R2-WP30, R2-WP32 | Utarg zakładu, majątek przy rozwiązaniu, dochód po zdarzeniu, dziedziczenie, skala ekwiwalentna, lista płac, konta ruchu |
| `R2c` | `R2c-rozjazdy-danych-i-kodu.md` | R2-WP12…R2-WP16 | Wiek produkcyjny, wartość czasu, substytucja, chodniki, martwe potrzeby |
| `R2d` | `R2d-domkniecie-swiata.md` | R2-WP17…R2-WP19 | Kopalnie na złożach, gęstość firm, przechwytywanie rzek |
| `R2e` | `R2e-dlug-i-martwy-kod.md` | R2-WP20…R2-WP23, R2-WP27, R2-WP28, R2-WP31 | `DecisionReason`, generator dróg, martwe warianty, dokumentacja, język identyfikatorów, liczba i tekst dla gracza, rozdzielenie urzędu |
| `R2f` | `R2f-pomiar-i-bramki.md` | R2-WP24…R2-WP26, R2-WP29, R2-WP33, R2-WP34 | Filtry bramek G4/G11, testy miasta, egzekutor rejestru długu **i poprawek wędrujących w przód**, budżety grafu i Gantta z M9e, linia bazowa benchmarków, scenariusz eksportu |

Tabela pakietów z rozmiarami i statusem stoi w dokumencie każdej podfazy. Zbiorczo:

| WP | Nazwa | Podfaza | Zależy od | Rozmiar | Status |
|---|---|---|---|---|---|
| R2-WP1 ⇧ | Cykl szkolny w trakcie gry | R2a | — | M | `[x]` **przed R2** (`K-74`) |
| R2-WP2 | Graf rodziny: rodzeństwo, dziadkowie, ochrona wpisu | R2a | — | M | `[x]` (`K-59`) |
| R2-WP3 | Gospodarstwo bez cichego przepełnienia | R2a | — | S | `[x]` (`K-91`) |
| R2-WP4 | Opiekun prawny i gospodarstwo osierocone | R2a | R2-WP3 | M | `[x]` (`K-91`) |
| R2-WP5 | Wykształcenie jako stan zmienny | R2a | R2-WP1 | M | `[x]` **przed R2** (`K-74`) |
| R2-WP6 | Tożsamość rodzinna: nazwisko i cechy | R2a | R2-WP2 | S | `[x]` (`D-N4` — odrzucone z powodem) |
| R2-WP7 ⇧ | Utarg zakładu produkcyjnego | R2b | — | M | `[x]` **przed R2** (`K-75`) |
| R2-WP8 | Majątek gospodarstwa przy rozwiązaniu i podziale | R2b | — | M | `[x]` (`K-61`) |
| R2-WP9 | Dochód gospodarstwa po zdarzeniu życiowym | R2b | — | S | `[x]` (`K-93`) |
| R2-WP10 | Dziedziczenie ponad gotówkę osobistą | R2b | R2-WP8 | M | `[x]` (`K-95`) |
| R2-WP11 | Skala ekwiwalentna gospodarstwa | R2b | — | S | `[x]` (`K-94`) |
| R2-WP12 ⇧ | Wiek produkcyjny w jednym miejscu | R2c | — | S | `[ ]` |
| R2-WP13 | Wartość czasu idzie za dochodem | R2c | — | M | `[x]` (`K-98`; motoryzacja → `R2-WP38`) |
| R2-WP14 | Szczebel substytucji dostaje wykonawcę | R2c | — | M | `[x]` (`K-97`) |
| R2-WP15 | Chodniki: warstwa piesza bez dróg szybkiego ruchu | R2c → **M11c** | — | S | `[x]` |
| R2-WP16 | Potrzeby bez martwych slotów | R2c | — | M | `[x]` (`K-96`; z pozycją 43) |
| R2-WP17 | Kopalnia staje na złożu | R2d → **M11c** | `D-N13` (przyjęta) | M | `[x]` |
| R2-WP18 | Gęstość firm i pasmo bezrobocia | R2d → M11c → **R3** | `D-N20` | L | `[~]` |
| R2-WP19 | Przechwytywanie rzek w erozji | R2d | — | M | `[x]` **zakończenie drugie** (pomiar, `M1` §5.7a; warunek włączenia — poz. 80) |
| R2-WP20 | Podział `DecisionReason` | R2e | wszystkie pozostałe | L | `[ ]` |
| R2-WP21 | Generator dróg: rozcięcie `lsystem.rs` | R2e | — | M | `[ ]` |
| R2-WP22 | Martwe warianty i nieużywane pola | R2e | — | M | `[ ]` |
| R2-WP23 | Dokumentacja wejściowa i zakresy strumieni | R2e | — | S | `[ ]` |
| R2-WP24 | Bramka bezrobocia naprawdę mierzy bezrobocie | R2f | — | M | `[x]` (`Verdict` z czterema stanami; G11 mierzy zawsze, G4 i G12 przestają być doradcze, pominięcie wywraca bieg nocny — `D-N17`) |
| R2-WP25 | Testy miasta wychodzą z `#[ignore]` | R2f | — | M | `[x]` (`WorldSize::Km2`, 27 testów automatycznie; `D-19` i `D-22` z R1 wykonane po stronie testu) |
| R2-WP26 | Egzekutor rejestru długu i poprawek wędrujących w przód | R2f | — | M | `[x]` (55 pozycji z 68 bez żywego adresata; trzy niespełnione obietnice wobec M8e; powstał dokument `R3`) |
| R2-WP27 | Jeden język w kodzie: identyfikatory i komunikaty | R2e | — | zależny od `D-N19` | `[ ]` |
| R2-WP28 | Liczba i tekst dla gracza bez niespodzianek | R2e | — | S | `[ ]` |
| R2-WP29 | Budżety grafu, Gantta i panelu zmierzone | R2f | M9e | S | `[x]` (`game/benches/ui_budgets.rs`, dane z przebiegu, rozmiar z kontraktu) |
| R2-WP30 | Lista płac obciąża pracodawcę | R2b | R2-WP7 | M | `[~]` (`K-93`; miasto jako pracodawca — poz. 76) |
| R2-WP31 | Wpłata poza rejestrem to nie praktyka monopolistyczna | R2e | — | M | `[ ]` |
| R2-WP32 | Konta stacji, przewoźnika, taksówki i parkingu | R2b | — | L | `[x]` (`K-72`) |
| R2-WP33 | Linia bazowa benchmarków mierzy wszystkie | R2f | — | S | `[x]` (brak wpisu = kod wyjścia 1, `--self-test`, odnowienie osobnym commitem; reguła odnawiania w `D-N21`) |
| R2-WP34 | Scenariusz `export_drains` i druga połowa kryterium WP9 M6 | R2f | R2-WP32 | M | `[x]` (`tools/headless/src/export_drains.rs`, pierwszy wołający `B2b::try_export` poza testami) |
| R2-WP35 | Opieka nad dzieckiem poniżej wieku szkolnego | R2a | R2-WP1 | M | `[ ]` |
| R2-WP36 | Utarg eksportowy zakładu produkcyjnego | R2b | R2-WP7 | S | `[x]` |
| R2-WP37 | Zmiana robocza idzie za rodzajem zakładu | R2a | — | M | `[x]` (`K-92`) |
| R2-WP38 | Motoryzacja idzie za dochodem | R2c | R2-WP13 | M | `[ ]` (`D-N12`) |
| R2-WP39 | Bramka wyjaśnialności mierzy to, co obiecuje | R2f | R2-WP32 | S | `[x]` (poz. 81; 61 357 decyzji bez powodu → **zero**) |

`⇧` = kandydat do wyprzedzenia przed R2 zgodnie z §2b.

Pakiety R2-WP30…R2-WP34 pochodzą z rzutu czwartego i mają wspólną cechę: **każdy był już komuś
przypisany i adres wygasł**. R2-WP30 był przenoszony sześć razy (M7b → M7f → M8a → M8d → M8e →
`CJ-9`), R2-WP32 jest decyzją otwartą nr 16 fazy M5, która trzyma bramkę 7 tamtej fazy,
R2-WP33 to `D-R8` z R1, R2-WP34 to `AH-12`/`AI-8` z M6c wskazujące na M6e, które zamknęło się
bez tego scenariusza.

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
do niego dojdzie. R2 wymaga jedenastu wpisów w dokumentach wcześniejszych i **żaden z nich nie jest
przeprojektowaniem tamtej fazy**. Cztery ostatnie wiersze pochodzą z rzutu czwartego i mają jeden
kształt: dokument fazy ma tam otwartą pozycję z adresem, którego nie ma — wpis nadaje jej adres,
a nie zmienia jej treści:

| Dokument | Wpis |
|---|---|
| `M8-miasto-jako-aktor.md` | Test T4a jest niemierzalny, dopóki nie zamknie się poz. 1 i 21 wykazu. Do §7 wiersz z warunkiem wejścia |
| `M8d-uslugi-i-prawo.md` | Trzy pakiety `S` przejęte z R2: poz. 1 (cykl szkolny), 21 (wykształcenie jako stan) i 2 (utarg zakładu produkcyjnego) |
| `M8a-pieniadz-publiczny.md` | Poz. 2 (`BC-8` z M7e) miała tu adres i M8a zamknęło się bez niej. Wiersz w tabeli korekt: adres przeniesiony na M8d, z liczbą przeprowadzek |
| `M8c-zdarzenia.md` | Kalibracja hazardów społecznych na bezrobociu 0,2 % opisze fikcję. Do §5.5 warunek: sondy kalibrowane po zamknięciu poz. 6; pakiet `S` na poz. 7 (wiek produkcyjny z danych) |
| `M10e-relacje-i-zwiazki.md` | Warunek składowej grafu wśród pracowników zakładu liczy dziś uczniów. Do §5.9 wiersz o poz. 11 |
| `M6-lancuch-dostaw.md` | `AH-3` twierdzi, że `Carrier::Pipeline` wozi ropę wewnątrz miasta — ścieżki wykonania nie ma. Sprostowanie w tabeli korekt |
| `R1-refaktor-po-M5.md` | Pozycje 24, 33, 34, 35, 37, 38 rejestru dostają adresata `R2` zamiast pustego pola albo „żadna z M6–M12" |
| `M5-gospodarka-detaliczna.md` | Decyzja otwarta nr 16 (konta stacji paliw, przewoźnika, taksówki i parkingu) dostaje adresata: R2-WP32. Do §9.16 zdanie zamykające i wiersz w tabeli korekt — bramka 7 fazy M5 zostaje otwarta do R2, ale **z nazwanym wykonawcą**, a nie bez niego |
| `M8-miasto-jako-aktor.md` (`CJ-9`) | Obie pozycje obiecane wykazowi R2 istnieją teraz jako 58 i 59. Wiersz w tabeli korekt z datą wpisania — bo między obietnicą a wpisem minęły dwie podfazy M9 |
| `R1-refaktor-po-M5.md` (`D-R8`) | Decyzja „odnowić linię bazową w pierwszym commicie po R1" nie miała adresata i R1 zamknęło się bez niej. Adresat: R2-WP33. Wiersz przy `D-R8` |
| `M6-lancuch-dostaw.md` (`AI-8`) | Scenariusz `export_drains` z §7.7 miał powstać w M6e i nie powstał; kryterium WP9 zostało zawężone, a pomiar przeniesiony donikąd. Adresat: R2-WP34 |
| `M1-swiat-statyczny.md` (dopisane w R2d) | Komentarz `ponytail:` o braku przechwyceń rzecznych podawał **oszacowanie** kosztu, którego nikt nie sprawdził przez sześć faz. Nowa §5.7a z pomiarem i warunkiem włączenia; pierwsza tabela „Zmiany wpisane po" w tym dokumencie — M1 zamknęło się przed wprowadzeniem `K-18` |

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
6. **Rejestr długu strukturalnego nie ma pozycji bez adresata, a tabela korekt nie ma obietnicy
   bez pokrycia.** Pozycja bez fazy-właściciela jest błędem bramki `struct_guard`, nie wpisem
   w tabeli; wiersz korekty wskazujący dokument, w którym nie ma jej treści, jest błędem
   `plan_guard`. Obie bramki muszą **zaświecić na czerwono na stanie sprzed R2** — inaczej
   sprawdzają co innego, niż myślą.
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

**`D-N20` — Czy zakład jest budynkiem, czy lokalem.** Otwarta przez pomiar `R2-WP18`
wykonany w M11c; **sufit `D-N6` zadziałał**. Propozycja: **lokalem** — Etap 7 dostaje pass
rozstawiający zakłady po `Unit`, a nie po parceli, i wtedy `SiteSet.by_building` przestaje być
odwzorowaniem jeden-do-jednego. Pomiar (świat 4 km, `industrial`, ziarno 1): 24 800 mieszkańców,
**199 firm**, 202 zakłady — jedna firma na 125 osób wobec obiecanych 1 : 15…25. Lokali użytkowych
jest przy tym **4 186**, z czego 3 209 należy już do jakiegoś zakładu: premises **są**, brakuje
mechanizmu, który wsadzi do nich osobne firmy. Drugi objaw z tej samej przyczyny: `it_office`
ma 19 zakładów i **3 483 etaty**, czyli 183 osoby na biuro w mieście 25-tysięcznym — obsada
liczy się z powierzchni **całego budynku**, bo zakład bierze cały budynek. Rozdrobnienie leczy
oba naraz i nie rusza normatywu mocy produkcyjnej: ta sama powierzchnia, ten sam sumaryczny
etat, więcej podmiotów. Cena jest jednak wyższa niż „pakiet naprawczy": zmiana dotyka
`utworz_zaklady`, `rebind_workplaces`, `by_building` i wszystkich pięciu jego czytelników,
a firm w mieście robi się pięć razy więcej — czyli dotyka wydajności i bilansu pieniądza.
**To jest przeprojektowanie Etapu 7, więc należy do własnej fazy (R3), a nie do dokumentu
napraw ani do podfazy prezentacyjnej.** *Blokująca dla R2-WP18; pakiet przechodzi do R3.*

**`D-N19` — Czy identyfikatory prywatne przechodzą na angielski, czy reguła się zmienia.**
`00` §6 i `CLAUDE.md` mówią: „kod i identyfikatory po angielsku, bez wyjątków — również w nowych
fazach". Kod mówi co innego i mówi to konsekwentnie od M5: **publiczne API jest angielskie,
a prywatne nazwy są polskie** w `sim/economy`, `sim/world`, `sim/agents`, `game/` i `tools/magnat`
(`zbierz_fakty`, `rozstrzygnij`, `zastosuj`, `wykonaj`, `warunek`, `klauzula`, `Wynik`, `Slownik`,
dziesiątki innych). To nie jest niedbalstwo jednej podfazy — to jest **druga, niezapisana
konwencja**, którą każda faza przejmowała z pliku, który rozszerzała.

Propozycja: **reguła się zmienia i zaczyna opisywać to, co jest.** Nowe brzmienie: angielski
obowiązuje wszędzie, gdzie nazwa przekracza granicę crate'u (typy publiczne, funkcje publiczne,
pola publiczne, klucze danych, warianty enumów) — i tam jest bez wyjątków. Prywatna nazwa wewnątrz
modułu idzie w języku komentarzy tego modułu, czyli po polsku, bo czyta ją ten sam człowiek i w tym
samym zdaniu co komentarz nad nią.

Argument za: przemianowanie jest mechaniczne, ale dotyka **każdego crate'u w repozytorium**,
zatapia recenzję każdego commita, w którym wypadnie, i nie zmienia ani jednego zachowania.
Argument przeciw: reguła pisana pod istniejący kod przestaje być regułą, a następny rozjazd
uzasadni się tym precedensem. Jeśli decyzja pójdzie odwrotnie, R2-WP27 rośnie z `S` do `XL`
i staje się osobnym commitem bez żadnej innej zmiany — tak samo jak `cargo fmt` całego repo.
*Blokująca dla R2-WP27; nieblokująca dla reszty R2.*

---

## 10. Wpisy wymagane w `00-konwencje-i-kontrakty.md` §4a

Zmiany R2 dotykające kontraktów z dokumentu 00 wymagają wpisu `K-n` zgodnie z jego §4a.
Wpisy powstają w commicie pakietu, który zmianę wprowadza, a nie z góry.

**Numeracja sprawdzona po R2f (2026-09-22): R2f nie dopisuje ani jednego `K-n`.** Żadna zmiana tej podfazy nie dotyka kontraktu z dokumentu 00 — `Verdict` i `GateOutcome` należą do `tools/balansator`, `WorldSize::Km2` do `sim/world`, sygnatura `Books::inject_external_capital` i `restock_target` do `sim/economy`, a reguła „pozycja rejestru ma adresata” mieszka w `CLAUDE.md` od R1 §6. Pierwszym wolnym numerem jest `K-99`. **Zapowiedź z tej sekcji o sześciu wpisach `K-58`…`K-63` była nieprawdziwa od M8**: te numery zajęły M8 i M9, a `K-58` wykonała R2e — czyli tabela niżej opisuje rezerwację, której już nie ma.

**Numeracja sprawdzona po R2d (2026-09-21): R2d nie dopisało żadnego `K-n` i to jest wynik, a nie przeoczenie.** `R2-WP19` dotknął schematu `data/geology/erosion.ron` (`schema_version` 1 → 2) i struktury raportu generacji (`WorldStats`), a żadne z nich nie jest kontraktem z dokumentu 00 — oba należą w całości do M1 i tam są opisane (§5.7a). `K-58` i `K-73` są nadal wolne i zostają przy swoich pakietach.

**Numeracja sprawdzona po R2c (2026-09-21):** `K-96`, `K-97` i `K-98` dopisane — pierwszym wolnym numerem po R2b było `K-96`. `K-58` i `K-73` są nadal wolne i zostają przy swoich pakietach (`R2-WP20`, `R2-WP31`).

**Numeracja sprawdzona po R2b (2026-09-21):** `K-61` i `K-72` wykonane — dokładnie w treści, w której były zarezerwowane. Dodatkowo `K-93`, `K-94` i `K-95`, bo R2b zmieniło trzy kontrakty, których §10 nie przewidywał: umowę o pracę (gospodarstwo w `Employment`), `Household` (liczba dzieci i skala ekwiwalentna) oraz `InheritanceHook` (świat, `estate_charge`, wołanie bezwarunkowe). `K-58` i `K-73` są nadal wolne i zostają przy swoich pakietach.

**Numeracja sprawdzona po R2a (2026-09-21):** `K-59` wykonana, `K-91` i `K-92` dopisane — pierwszym wolnym numerem po M10g było `K-91`. `K-58`, `K-61`, `K-72` i `K-73` są nadal wolne i zostają przy swoich pakietach.

**Numeracja sprawdzona 2026-09-18.** R2 zarezerwowało `K-58`…`K-61`, kiedy pierwszym wolnym
numerem był `K-58`. W międzyczasie M8 i M9 zajęły `K-62`…`K-71`, a `K-60` **wykonała M8c** —
dokładnie w tej treści, w której był zarezerwowany (wiek produkcyjny jako dana). `K-58`, `K-59`
i `K-61` są nadal wolne i zostają przy swoich pakietach; dwa wpisy z rzutu czwartego dostają
**`K-72` i `K-73`**, bo pierwszy wolny numer jest teraz tam. To jest powód, dla którego rezerwuje
się numery w tabeli, a nie w głowie.

| Numer | Pakiet | Treść |
|---|---|---|
| `K-58` | R2-WP20 | `DecisionReason` dzieli się na `CitizenReason \| FirmReason \| CityReason` pod wspólnym enumem-sumą. Reguła `K-12` (wyczerpujący `match` bez `_`) obowiązuje na każdym z trzech osobno; jedno miejsce renderujące zostaje, ale rozpada się na trzy funkcje |
| `K-59` | R2-WP2 | `RelationKind` dostaje wariant `Grandparent`. Kolejność wariantów jest kontraktem zapisu gry — dopisywać wolno wyłącznie na końcu |
| `K-60` | R2-WP12 | Wiek produkcyjny jest **daną**, nie stałą w kodzie. Jedno źródło: `data/demography/demography.ron`, pole `ages.labour_force`. Statystyka bezrobocia i model makro czytają to samo pole |
| `K-61` ✔ | R2-WP8 | Rozwiązanie gospodarstwa domowego jest **operacją księgową**: salda przechodzą do spadkobierców albo na konto techniczne, nigdy nie znikają razem z encją. Niezmiennik świata obejmuje gospodarstwa — osobną funkcją `Books::check_world_conservation`, bo `P1` musi liczyć kanał sektora do podaży, a ten niezmiennik nie |
| `K-72` ✔ | R2-WP32 | Rejestry ruchu (`FuelLedger`, `FareLedger`, opłaty parkingowe) przestają być rejestrami i stają się **kontami w `Books`**. Niezmiennik świata `society::total_money + Books::total_balance() == const` obowiązuje wtedy bez wyłączeń i jest bramką scenariusza, nie pomiarem wypisywanym obok |
| `K-73` | R2-WP31 | `AgencyKind` dostaje wariant `Prosecution` **na końcu** listy — kolejność wariantów jest kontraktem indeksu zapisu gry. Przesłanka „wpłata poza rejestrem wpłat kampanijnych" przechodzi z urzędu antymonopolowego do niego |
| `K-91` | R2-WP3, R2-WP4 | `DecisionReason::{EscortUnavailable = 120, GuardianAppointed = 121}` przedłużają blok M3; `Household` dostaje `guardian` i `FLAG_OVERCROWDED` **z rezerwy M5/M9**, zostając przy 120 B; `add_member` jest `#[must_use]`; opiekun powstaje w dobowym przeglądzie, a nie w haku przy zgonie |
| `K-93` | R2-WP9, R2-WP30 | Umowa o pracę pamięta gospodarstwo (`Employment.household`), `Workforce::release` bierze je argumentem, `economy.Labor` idzie po `agents.Society`, a `PayrollOutbox` dostaje konsumenta: płaca schodzi z konta zakładu, `pay_incomes` staje się dopłatą |
| `K-94` | R2-WP11 | Skala ekwiwalentna gospodarstwa jest **daną** (`envelopes.ron`, `equivalence`) w promilach; `Household` dostaje `children` z rezerwy M5/M9, `ECONOMY_SCHEMA_VERSION` idzie z 3 na 4 |
| `K-95` | R2-WP10 | `InheritanceHook` dostaje `&mut World` i `estate_charge`, i jest wołany **zawsze** — także bez spadkobierców. Udziały dzielą się wagami gotówki, brak spadkobiercy znaczy `Owner::City` |
| `K-96` | R2-WP16 | `NeedKind` traci `Status` (`D-N11`) i ma jedenaście wariantów; potrzeba bez tempa i bez miejsc musi w danych podać `owner_phase`; skutki progowe deprywacji dostają czytelników (`DeprivationPressure`, szósty argument `effective_labor`, ambicja skuteczna w `PersonFacts`); nastrój wraca do zera, bo `MoodLoss` był jego jedynym pisarzem |
| `K-97` | R2-WP14 | Szósty szczebel kaskady ma wykonawcę w linii produkcyjnej, a nie na rynku; `ShortageAction::Substitute` znika jako wariant bez wykonawcy; kaskada dostaje sufit na `Substituted`, dopóki zamiennika starcza |
| `K-98` | R2-WP13 | Tablica dochodów w `TrafficOracle` przestaje być migawką z generacji: odświeża ją co dobę `TrafficSystem`, wchodzi do hasha stanu, a `household_incomes` jest jedną regułą o dwóch wołających. Przyrostowego `bump_income` nie ma — to wzorzec, który naprawiał `K-93` |
| `K-92` | R2-WP37 | Grafik zmianowy jest **jedną regułą o dwóch wołających** (`ShiftKind::schedule`): generator miasta rozdawał zmiany poprawnie, a rynek pracy wpisywał każdemu zmianę dzienną i dni robocze |

---

## 11. Wykaz — 86 pozycji (liczone, nie przepisywane)

Numeracja jest numeracją przeglądu i nie zmienia się. Kolumna „Plan" mówi, co wiedziały dokumenty
przed R2: `—` = nieznane planowi, `zapis` = zapisane jako znana usterka bez wykonawcy,
`WP` = miało pakiet. Kolumna „Status" wypełnia się w trakcie R2.

**Liczba w nagłówku była nieprawdziwa i to jest odnotowane, a nie po cichu poprawione.** Nagłówek
mówił 78, `00-postep.md` mówiło 73, a policzone pozycje dawały 79 (numer 38 stoi w dwóch wierszach).
Rozjazd wziął się stąd, że każdy rzut dopisywał wiersze, a liczbę w nagłówku poprawiał ten, kto
akurat pamiętał. Od R2d obie liczby są liczone, a nie przepisywane — to ta sama klasa usterki,
którą wykaz opisuje, tyle że we własnym nagłówku.

**Pięć pozycji ma adresata poza R2.** Decyzją właściciela produktu z 2026-09-19 pozycje
**5, 6, 12, 70 i 71** wykonuje **M11c**, razem z pakietami `R2-WP15`, `R2-WP17` i `R2-WP18`.
Powód stoi w tabeli `J-n` dokumentu `M11c-wnetrza-i-kamera.md`: podfaza prezentacyjna ma pokazać
ulicę, a ulica jest pusta — w kadrze kilkadziesiąt osób, zero samochodów i dziesięciokrotnie
za mało firm. To jest **zmiana adresu, nie zakresu**: opisy pakietów, ich kryteria i sufit
czasu `D-N6` przy `R2-WP18` zostają bez zmian, a status wraca tutaj po zamknięciu M11c.

| # | Usterka | Plan | Pakiet | Status |
|---|---|---|---|---|
| 1 ⇧ | Dziecko urodzone w grze nie dostaje flagi ucznia ani szkoły | — | R2-WP1 | `[x]` **wykonane przed R2** (`K-74`) |
| 2 ⇧ | Zakład produkcyjny nigdy nie ma utargu; tier taktyczny go nie zamknie | zapis `M7e` `BC-8` | R2-WP7 | `[x]` **wykonane przed R2** (`K-75`) |
| 3 | Szczebel `Substituted` kaskady ma puste ramię `match` | zapis `M6` `AG-6` | R2-WP14 | `[x]` **zamknięte** (`K-97`, test `piekarnia_bez_maki_piecze_na_otrebach_i_nie_staje`; przed naprawą zakład z pełnym magazynem otrąb dochodził do `Halted` w tej samej dobie co zakład z pustym. Puste ramię było przy tym w `B2b::serve` **słusznie** — podmiany się nie kupuje; brakowało wykonawcy w linii, a nie na rynku) |
| 4 | Wartość czasu zamrożona na stanie z generacji świata | — | R2-WP13 | `[x]` **zamknięte** (`K-98`, testy `awans_podnosi_wartosc_czasu_w_ciagu_doby` i `utrata_pracy_obniza_wartosc_czasu`; przed naprawą stawka nie drgnęła ani o grosz: 37 → 37 po trzykrotnym awansie) |
| 5 | Zero zakładów wydobywczych ze złożem w mieście 4 km | zapis `M6` `AQ-8` | R2-WP17 → **M11c** | `[x]` **zamknięte w M11c** (`J-14`, `J-15`) |
| 6 | Gęstość firm ~10× za niska; bezrobocie 0,2 % przy 12 032 wakatach | zapis `00-postep` `BF-4`/`BF-10`, pomiar `M11c` | R2-WP18 → M11c → **R3** | `[→]` **przeniesiona** do `R3-etap-7-i-dlug-strukturalny.md` (`R3-WP1`, `D-N20`): sufit `D-N6` zadziałał, pomiar jest, a przeprojektowanie Etapu 7 ma własną fazę |
| 7 ⇧ | `WORKING_AGE` w kodzie (2 miejsca) vs `work_start`/`retirement` w danych | — | R2-WP12 | `[x]` **wykonane w M8c** (`K-60`) |
| 8 | Rodzeństwo z zasiedlenia i napływu bez relacji `Sibling` | — | R2-WP2 | `[x]` **zamknięte** (`K-59`, test `rodzenstwo_z_zasiedlenia_ma_relacje`) |
| 9 | Babcia dostaje z wnukiem relację `Sibling`; brak `Grandparent` | — | R2-WP2 | `[x]` **zamknięte** (`K-59`; wariant jest **symetryczny**, nie odwracany na `Child` — inaczej wnuk wchodziłby do `spadkobiercy`) |
| 10 | Dziecko urodzone w pełnym gospodarstwie nie wchodzi do listy członków | — | R2-WP3 | `[x]` **zamknięte** (`K-91`, test `porod_do_pelnego_gospodarstwa_nie_gubi_dziecka`; przed naprawą 80 osób w składach wobec 81 żywych) |
| 11 ⇧ | Uczeń wchodzi do indeksu miejsc pracy i dostaje relacje `Colleague` | zapis `M3d` `E-19` (przyczyna) | R2-WP1 | `[x]` wykonane w `K-74` (2026-09-18); M10e policzył warunek uzwiązkowienia na `coworkers` bez ani jednego filtra |
| 12 | Warstwa piesza dopuszcza drogi szybkiego ruchu | — | R2-WP15 → **M11c** | `[x]` **zamknięte w M11c** (`J-13`) |
| 13 | Motoryzacja to płaska stawka 430 ‰ bez związku z dochodem | zapis `M4b` `L-12` | **R2-WP38** | `[→]` **przeniesiona** do `R3-etap-7-i-dlug-strukturalny.md` (`R3-WP5`, `D-N12`): `R2-WP38` został opisany i nie dostał wykonawcy w żadnej podfazie R2 |
| 14 | Potrzeba `Status`: tempo 0, brak miejsc, `StatusLoss` pusty | zapis `M3a` `D-15` | R2-WP16 | `[x]` **zamknięte** (`K-96`, `D-N11`; wariant usunięty, `NEED_COUNT` 12 → 11, a jego jedyny skutek `AmbitionGain` ma drugiego producenta w `Development` i od tej chwili czytelnika. `StatusLoss` zostaje bez czytelnika **świadomie**: status jest liczony, a nie odejmowany — M3c §5.8) |
| 15 | `ProductivityLoss` i `AmbitionGain` jawnie puste | zapis `M7b` ★ | R2-WP16 | `[x]` **zamknięte** (`K-96`, testy `glod_obniza_produktywnosc_pracownika` i `deprywacja_rozwoju_podnosi_ambicje`; przed naprawą ocena głodnego i najedzonego wynosiła 496 w obu przypadkach, a ambicja 0 w obu) |
| 16 | Majątek gospodarstwa przepada przy rozwiązaniu | zapis `M3c` `G-7` (tylko lokal i etat) | R2-WP8 | `[x]` **zamknięte** (`K-61`, test `rozwiazane_gospodarstwo_nie_gubi_pieniedzy`; przed naprawą znikało dokładnie 100 000 gr, a `society::total_money` schodziło do zera) |
| 17 | Śmierć pracownika może zostawić płacę w `income_monthly` | zapis `M7b` ★ (tylko etat) | R2-WP9 | `[x]` **zamknięte** (`K-93`). Usterka zachodzi na dzisiejszym harmonogramie i ma **dwie** przyczyny: `release` szukało gospodarstwa przez `Identity` zmarłego, a `odejdz` pomijało `release` w ogóle dla emeryta, któremu `sim/agents` wyczyściło już komponent |
| 18 | `InheritanceHook` to zaślepka; dziedziczona tylko gotówka osobista | decyzja otwarta `M3` §9.10 | R2-WP10 | `[x]` **zamknięte** (`K-95`, testy w `sim/economy/tests/inherit.rs`; hak stał przy tym **wewnątrz** gałęzi „są spadkobiercy”, więc majątek bez spadkobiercy nie był nawet zgłaszany) |
| 19 | Wyprowadzka z gniazda i rozstanie nie przenoszą środków | — | R2-WP8 | `[x]` **zamknięte** (`K-61`, testy `usamodzielnienie_zabiera_udzial_w_majatku` i `rozstanie_dzieli_majatek_i_dlug_na_pol` plus `proptest` na 10 000 konfiguracji; ułamek podaje wołający — `1/n` z gniazda, `1/2` przy rozstaniu) |
| 20 | Brak opiekuna, kurateli i sieroctwa | — | R2-WP4 | `[x]` **zamknięte** (`K-91`, testy `smierc_ostatniego_doroslego_daje_dzieciom_opiekuna` i `prop_dziecko_nigdy_bez_doroslego_i_bez_opiekuna`) |
| 21 | Nikt nie zdobywa wykształcenia w trakcie gry | — | R2-WP5 | `[x]` **wykonane przed R2** (`K-74`) |
| 22 | Brak żłobka i przedszkola dla dzieci 0–6 lat | zakres `M3` §2 → M8 | R2-WP5 | `[→]` **przeniesiona** do `R3-etap-7-i-dlug-strukturalny.md` (`R3-WP4`): temat wyszedł z `R2-WP5` do `R2-WP35`, a `R2-WP35` nie dostał wykonawcy |
| 23 | Dziecko konsumuje tyle co dorosły | — | R2-WP11 | `[x]` **zamknięte** (`K-94`; rodzina dwoje dorosłych + czworo dzieci ma 2,7 osoby ekwiwalentnej zamiast 6, skala w promilach z `envelopes.ron`) |
| 24 | Relacja rodzinna nie chroniona przed wypchnięciem z slabu | — | R2-WP2 | `[x]` **zamknięte** (`K-59`, test `relacja_rodzinna_nie_wypada_przy_przepelnieniu`; podłoga wagi przeszła do `social.family_floor`) |
| 25 | Osobowość noworodka nie jest dziedziczona | — | odrzucone `D-N4` | `[x]` **odrzucone z powodem** (`R2-WP6`; komentarz w `day.rs::uroda` nazywa rozstrzygnięcie, skaner `plan_refs` pilnuje, że nie odsyła do fazy, której nie ma) |
| 26 | Brak zmiany nazwiska po ślubie | — | odrzucone `D-N4` | `[x]` **odrzucone z powodem** (`R2-WP6`; komentarz w `migration.rs` przestał odsyłać do „fazy, która ślub modeluje" — takiej fazy nie ma i nie będzie) |
| 27 | Maksymalnie czworo dzieci odprowadzanych, piąte pomijane | — | R2-WP3 | `[x]` **zamknięte** (`K-91`, test `piate_dziecko_zostawia_slad`; limit zostaje, znika milczenie — `HouseholdRoles::unescorted` i `DecisionReason::EscortUnavailable`) |
| 28 | Brak opieki nad starszymi | — | R2-WP4 | `[x]` **zamknięte** (`K-91`; gospodarstwo samych seniorów o zdrowiu poniżej `care_health_threshold` dostaje opiekuna tą samą regułą co sierota) |
| 29 | `Carrier::Pipeline` bez ścieżki wykonania | zapis `M6c` `AH-3` (błędny) | R2-WP22 | `[x]` **zamknięte w R2e** (`R2-WP22`, `D-N5`): `Carrier::Pipeline`, `PipelineId` i taryfa znikają z typu; zostaje komentarz-nagrobek w `b2b/trade.rs` |
| 30 | `TripPurpose::Escort` nigdzie nie konstruowany | — | R2-WP22 | `[x]` **zamknięte w R2e** (`R2-WP22`, `E-12`): `TripPurpose` przeniesiony do `engine/core`, test `plan_doby_niesie_cel_kazdej_podrozy_a_odprowadzenie_wazy_wiecej_niz_dojazd` |
| 31 | `shopper_rotation` i `vehicle_slots` nieużywane | — | R2-WP22 | `[x]` **zamknięte w R2e** (`R2-WP22`, `E-15`): `shopper_rotation` i `vehicle_slots` usunięte, `Household` schudł ze 120 B do 112 B |
| 32 | `MachineClassId` bez katalogu danych | zapis `M6b` `AF-4` | R2-WP22 | `[x]` **zamknięte w R2e** (`R2-WP22`, `D-N16`): zostaje świadomie jako `ponytail:`, a katalog `classes.ron` dostaje adresata M12d |
| 33 | `reason.rs::describe` 730 linii przy progu 250 | zapis rejestr poz. 37 | R2-WP20 | `[x]` **zamknięte w R2e** (`R2-WP20`, `K-58`, `E-8`): `describe` 1290 linii rozpadło się na trzy renderery 376 · 634 · 287 |
| 34 | Generator dróg ponad progami, bez fazy otwierającej | zapis rejestr poz. 33–35 | R2-WP21 | `[x]` **zamknięte w R2e** (`R2-WP21`, `E-9`, `E-10`, test `macierz_hashy_miasta_nie_drgnela_po_podziale`) |
| 35 | Wyzwalacze długu nie zadziałały (`generate_city`, `EconomyData::load`) | zapis rejestr poz. 24, 38 | R2-WP26 | `[x]` **zamknięte w R2f** (`R2-WP26`): `struct_guard --all` czyta rejestr długu i pyta o adresata; pierwszy przebieg złapał **55 pozycji z 68** bez żywego adresata, w tym oba wyzwalacze z tej pozycji |
| 36 | `README.md` opisuje stan „M1 zamknięte" | — | R2-WP23 | `[x]` **zamknięte w R2e** (`R2-WP23`, `E-1`): bramka `scripts/plan_guard.py` porównuje `Stan:` w `README.md` z ostatnim odhaczonym wierszem `00-postep.md` |
| 37 | M8 bez wariantów `StreamId` w przydzielonym bloku | WP `M8a` §5.0 (`K-4`) | R2-WP23 (weryfikacja) | `[x]` **zamknięte w R2e** (`R2-WP23`, `E-5`) weryfikacją: blok 240–259 był kompletny, doszedł test `engine/core/tests/stream_ids.rs` |
| 38 | ~~Profil `ci` balansatora nie mieści się w 10 minutach~~ — **nie jest usterką** | rozstrzygnięte `M5e` `AD-6 ★` | — | `nie dotyczy` |
| 38b | Bramka G11 (bezrobocie 3–12 %) chodzi tylko nocą i nie zapala się przy 0,2 % | — | R2-WP24 | `[x]` **zamknięte w R2f** (`R2-WP24`, testy `bezrobocie_dwa_promile_czerwieni_bramke_g11` i `profil_ci_pomija_bramki_bez_usuwania_ich_z_raportu`): filtr ciasnego rynku pracy znika z werdyktu i zostaje w opisie. Przed naprawą przebieg z bezrobociem 2 ‰ dawał „doradcze”, po naprawie **czerwone** — zmierzone także na prawdziwym przebiegu (200 dób, 2,8 % przy 12 536 wakatach na 14 780 siły roboczej) |
| 39 | Bramka G4 doradcza — 5 dób wobec widełek 14–56 | zapis `00-postep` M7 | R2-WP24 | `[→]` **przeniesiona** do `R3-etap-7-i-dlug-strukturalny.md` jako **poz. 82**: G4 przestała być doradcza w `R2-WP24` i okazała się czerwona z innego powodu niż widełki — `t_response` i `t_settle` są `None`, czyli mediana ceny szokowanego towaru **nie drgnęła wcale** |
| 40 | Większość testów miasta `#[ignore]` | zapis `R1` `D-R7` | R2-WP25 | `[x]` **zamknięte w R2f** (`R2-WP25`): 27 testów `sim/world/tests/` wychodzi z `#[ignore]` na świecie 2 km, reszta ma wiersz w `D-R7` z nazwą joba nocnego. Macierz hashy terenu i miasta identyczna bit w bit |
| 41 | Brak testu `income_monthly` po zdarzeniu życiowym | — | R2-WP9 | `[x]` **zamknięte** (`K-93`; trzy testy w `sim/economy/tests/labor.rs` — zgon, emerytura, wyjazd z miasta — na prawdziwym świecie, bo atrapa `TestPeople` gospodarstw nie zna) |
| 42 | Erozja nie przechwytuje rzek | — | R2-WP19 | `[x]` **zakończenie drugie** — mechanizm jest, ma test (`erozja_przechwytuje_zlewnie_dopiero_po_przetrasowaniu`) i stoi w danych na zerze, bo budżet go nie przyjął. Jedno przetrasowanie przechwytuje 478 395 komórek na mapie 16 km, ale kosztuje **1,38 s** wobec 1,2 s zapasu: 8,43 s → 10,27 s przy celu 10 s. Skrót przestał być skrótem bez pomiaru — decyzja z liczbami stoi w `M1` §5.7a. Warunek włączenia: poz. 80 |
| 43 | **Nastrój mieszkańca tylko spada i nic go nie odbudowuje** — `DeprivationEffect::MoodLoss` jest jedynym pisarzem `Vitals.mood` w całym repozytorium (`sim/agents/src/needs.rs`, `saturating_sub`). Po roku gry cała populacja siedzi na −100, a przebieg 400-dobowy `m8miasto` mierzy średnią **−99** | — | R2-WP16 (ten sam pakiet co `StatusLoss` i `ProductivityLoss`) | `[x]` **zamknięte** (`K-96`, testy w `sim/agents/tests/needs_mood.rs`; odbudowa jest bezwarunkowa i konkuruje z karami, a zero jest sufitem — nic nie podnosi nastroju ponad neutralny, bo żadna faza do niego nie pisze) |
| 44 | **`README.md` opisywał stan „M1 zamknięte"** przez siedem faz — poprawione w M8c na „M8c zamknięte". Pozycja 36 zostaje, bo jej treścią jest **test CI pilnujący opisu**, a nie jednorazowa poprawka | — | R2-WP23 (test) | `[x]` **zamknięte w R2e** (`R2-WP23`): brakującym testem CI jest ta sama bramka `plan_guard`, o którą chodzi w pozycji 36 |
| 45 | **Wartość gruntu nie zmienia się w trakcie gry.** Jedyne dwa zapisy `Parcel.land_value_per_m2` w całym repozytorium to `city::value::pass_1` i `pass_2`, obie wołane raz przy generacji miasta (`city/mod.rs`). Każdy kanał skutku kończący się na wartości gruntu — parki, zaległy wywóz odpadów, hałas — jest przez to **niewykonalny**, a nie tylko odłożony. Znalezione w M8d przy kanałach skutków usług publicznych (`CG-3`) | — | **bez pakietu** — kandydat na R2-WP27, bo żaden istniejący go nie obejmuje | `[→]` **przeniesiona** do `R3-etap-7-i-dlug-strukturalny.md`: `R2-WP27` okazał się pakietem językowym i tej pozycji nie objął; wartość gruntu nadal nie zmienia się w trakcie gry |
| 46 | **`ServiceKind::Waste` nie ma żadnego archetypu w `data/buildings/public.ron`.** `SpendCategory::Waste` ma udział w planie wydatków od M8a, więc miasto wydaje pieniądze na usługę, której w mieście nie ma — pokrycie wywozu odpadów jest zerowe w każdej dzielnicy i takie zostanie, dopóki ktoś nie dopisze archetypu. Znalezione w M8d (`CG-4`) | — | R2-WP22 (martwe warianty i nieużywane pola) | `[→]` **przeniesiona** do `R3-etap-7-i-dlug-strukturalny.md`: pozycja miała przypisany `R2-WP22`, ale lista pozycji tego pakietu w `R2e` jej nie wymienia — **wypadła po cichu**, czyli tym samym wzorcem, który łapie `R2-WP26` |
| 47 | **Identyfikatory prywatne są po polsku w całym repozytorium**, wbrew `00` §6 i `CLAUDE.md` („kod i identyfikatory po angielsku, bez wyjątków"). Publiczne API jest angielskie, prywatne nazwy polskie — w `sim/economy`, `sim/world`, `sim/agents`, `game/` i `tools/magnat`. Nie jest to usterka jednej podfazy, tylko **druga, niezapisana konwencja** przejmowana z pliku do pliku od M5 | — | R2-WP27 (`D-N19`) | `[x]` **zamknięte w R2e** (`R2-WP27`, `D-N19`, `E-6`): granicą języka jest crate, pilnuje tego `scripts/lang_guard.py` |
| 48 | **Komunikaty deweloperskie `eprintln!` są po polsku** w kliencie i w scenariuszach (`tools/magnat/src/session.rs`, `tools/headless`). Ta sama nieuzgodniona konwencja co pozycja 47, ale inna reguła: to nie jest identyfikator ani tekst gracza, tylko trzecia kategoria, której `00` §6 nie nazywa | — | R2-WP27 | `[x]` **zamknięte w R2e** (`R2-WP27`, `E-7`): komunikaty deweloperskie idą po polsku i reguła to zapisuje — 550 komunikatów opisywało stan, którego reguła nie znała |
| 49 | **Postać tekstowa polityki drukuje separator dziesiętny `.` niezależnie od języka** (`game::policy::text::procent`), a ta liczba trafia na ekran w zakładkach „reguły" i „tekst" edytora. Polski gracz czyta „98.55 %" zamiast „98,55 %" | — | R2-WP28 | `[x]` **zamknięte w R2e** (`R2-WP28`, test `procent_na_ekranie_ma_separator_jezyka_a_w_zapisie_kropke`) |
| 50 | **Brak klucza lokalizacji panikuje w ścieżce rysowania.** `Catalog::must` rozwija `Option` przez `expect`, a woła go kod budujący kartę inspekcji i ekran edytora reguł. Literówka w nazwie klucza wywraca klatkę zamiast pokazać pusty napis — a `Catalog::load` sprawdza **równość zbiorów** `pl`/`en`, nie obecność konkretnego klucza | — | R2-WP28 | `[x]` **zamknięte w R2e** (`R2-WP28`, test `liczebnik_o_zlej_liczbie_form_lamie_wczytanie_a_brak_klucza_nie_lamie_klatki`) |
| 51 | **Komunikat polityki spoza katalogu nie ma tekstu.** `Action::Alert`/`AskPlayer` niosą numer (`msg: u16`, `AX-2`), a `data/locale/` ma trzy wpisy `ui.policy.msg.*`. Edytor pokazuje wtedy regułę bez zdania, a skrzynka eskalacji (M9e) pokaże wpis bez treści. Brakuje walidacji: numer komunikatu spoza katalogu ma być błędem walidatora, a nie pustym miejscem | — | R2-WP28 | `[x]` **zamknięte w R2e** (`R2-WP28`, testy `komunikat_spoza_katalogu_jest_bledem_edytora` i `liczba_komunikatow_zgadza_sie_z_katalogiem`) |
| 52 | **`Qty` nie ma arytmetyki z punktami bazowymi.** `Money` ma `mul_ratio` przez `i128`, `Qty` nie ma nic — więc wykonawca polityki opakowuje ilość w `Money`, żeby przemnożyć ją przez odchyłkę menedżera (`sim/economy/src/policy_run/decide.rs`). Wynik jest poprawny, typ kłamie | — | R2-WP22 | `[x]` **zamknięte w R2e** (`R2-WP22`, test `kazda_wielkosc_calkowita_zaokragla_tak_samo_jak_pieniadz`): `Qty`, `Mass`, `Volume` i `Energy` dostały `mul_ratio` |
| 53 | **`Action::RemoveFromShelf` nie ma wykonawcy.** Polityka „wycofaj z półki" przechodzi walidator (akcja jest w dziedzinie `Stock`), wykonuje się i **nie robi nic** — wykonawca zwraca `PolicyOutcome::Blind`, bo zwolnienie oferty w arenie razem z linią półki nie ma jeszcze ścieżki. To jest dokładnie ten przypadek, przed którym broni `K-67`: reguła, która wygląda na działającą | — | R2-WP22 | `[x]` **zamknięte w R2e** (`R2-WP22`, test `wycofanie_z_polki_zdejmuje_linie_i_zwalnia_oferte`) |
| 54 | **Promień metryki konkurencyjnej jest ignorowany.** `radius_m` jest polem `Metric::CheapestCompetitorPrice`, `AvgCompetitorPrice` i `CompetitorCount`, wchodzi do walidatora (limit 10 km) i **nie wchodzi do odczytu**: obraz konkurencji sklepu powstaje jednym promieniem obserwacji dla całego sklepu. Reguła z 3 km i reguła z 5 km dostają tę samą liczbę, a gracz widzi dwie różne reguły | — | R2-WP22 | `[x]` **zamknięte w R2e** (`R2-WP22`, `E-13`, test `promien_metryki_zaweza_obraz_konkurencji`) |
| 55 | **Trzy z pięciu dziedzin polityki nie mają wykonawcy** (`Hr`, `Production`, `Logistics`). Walidator odrzuca je jawnie (`DomainNotAvailable`), więc cichej polityki nie ma — ale `Action::domain()` rozcina przy okazji **dwie z sześciu polityk przykładowych z `M9d` §5.6** na dwie każdą, bo przecena jest cenowa, a wycofanie z półki i zamówienie zapasowe. Czy dziedzina ma zostać granicą polityki, czy tylko granicą akcji — decyzja otwarta nr 13 fazy M9 | zapis `M9d` `DF-2` | R2-WP22 (weryfikacja po decyzji) | `[x]` **zamknięte w R2e** (`R2-WP22`, `E-14`) weryfikacją: odmowa `DomainNotAvailable` ma test, a `Logistics` bez akcji dostaje adres M12b |
| 56 ⇧ | **Dry-run nie zna salda, nastroju załogi ani wakatów.** Ślad doby zakładu (`PolicyTrace`) niesie półkę, bo o niej mówi polityka cenowa i zapasowa. Reguła oparta na `saldo`, `nastrój_załogi` albo `wolne_etaty` wychodzi w podglądzie jako „nie wiem" (`DrySummary.blind`), choć w wykonaniu się odpala. Sufit nazwany w kodzie, adresat wskazany: panel finansów `M9e` | zapis `M9d` `DF-6` | ⇧ `M9e` (WP10) | `[→]` **przeniesiona** do `R3-etap-7-i-dlug-strukturalny.md`: adresat `M9e` WP10 zamknął się bez tego, a R2 nie nadało pozycji pakietu — dokładnie ta klasa, którą łapie `R2-WP26` |
| 57 | **Cel zapasu firmy AI dla towaru bez historii sprzedaży wychodzi w milisztukach.** `ai_run::apply` przy `OpsAction::SetRestockDays` liczy zapas dobowy jako `obrót_7d / 7` z podłogą **jednej milisztuki**; dla towaru, który jeszcze się nie sprzedawał, cel „na N dni" wychodzi N tysięcznych sztuki. Sklep przestaje ten towar zamawiać, a sterownik ceny — liczący zapełnienie jako `ilość / cel` — widzi magazyn przepełniony i schodzi do podłogi marży | `sim/economy/src/ai_run/apply.rs`, ramię `OpsAction::SetRestockDays`; ścieżka gracza (`Market::set_restock_days`) dostała w M9e wyjście `restock_without_history`, ścieżka AI **nie** | R2-WP22 | `[→]` **przeniesiona z pomiarem** do `R3-etap-7-i-dlug-strukturalny.md`. Naprawa jest jednolinijkowa (gałąź zerowa do wspólnego `restock_target`, **jedna reguła o dwóch wołających**, `K-11`) i **została w R2f napisana, a potem cofnięta**: wywraca negatywny test `sim/economy/tests/firm_ai.rs::ale_widzi_cene_polkowa_gracza`. Ten test sprawdza, że decyzje firmy AI **zmieniają się** po obniżce ceny półkowej gracza — a po naprawie odciski obu przebiegów są identyczne, czyli jedynym kanałem, którym cena gracza docierała do decyzji konkurenta, była **ta degeneracja**. Rozstrzygnięcie „pada test czy pada model” jest decyzją o modelu, nie domknięciem, i dlatego wychodzi z R2 |
| 58 | **Wypłaty płyną z „reszty świata", a lista płac firm rośnie w skrzynce, której nikt nie opróżnia.** `PayrollOutbox::take()` nie ma w repozytorium ani jednego wołającego; pensje trafiają do gospodarstwa przez `pay_incomes` z konta `rest_of_world`, więc płaca **nie obciąża pracodawcy**. Blokuje łańcuch „budżet miasta → pensja nauczyciela → jakość szkoły" (`CH-4`), bo miasto nie może być pracodawcą, dopóki wypłata nie ma odbiorcy. Pozycja przenoszona **sześć razy** (M7b → M7f → M8a → M8d → M8e → `CJ-9`) | zapis `M8` `CJ-9` — wiersz kazał dopisać ją do tego wykazu i nie została dopisana | R2-WP30 | `[→]` **przeniesiona** do `R3-etap-7-i-dlug-strukturalny.md` w drugiej połowie (poz. 76): skrzynka ma konsumenta od `K-93`, ale miasto nadal nie jest pracodawcą |
| 59 | **Urząd antymonopolowy ściga przestępstwo.** Od M8e prowadzi sprawy z dwóch przesłanek: udziału rynkowego i wpłaty poza rejestrem wpłat kampanijnych. Druga nie jest praktyką ograniczającą konkurencję, tylko czynem karalnym, i należy do innego urzędu. Rozdzielenie wymaga szóstego wariantu `AgencyKind` — dopisywanego **na końcu**, bo kolejność wariantów jest kontraktem indeksu | zapis `M8` `CJ-9` — jak wyżej | R2-WP31 (`K-73`) | `[x]` **zamknięte w R2e** (`R2-WP31`, `K-73`, test `wplata_poza_rejestrem_nie_jest_praktyka_monopolistyczna`) |
| 60 | **Niezmiennik pieniądza świata nie domyka się, kiedy w scenariuszu jeździ ruch.** Mieszkaniec płaci za paliwo, bilet i taryfę z komponentu `Wealth`, a drugą stroną jest `FuelLedger`/`FareLedger` z `sim/traffic` — **rejestr, nie konto**. Kanałów bez pary jest co najmniej dwa i działają w przeciwne strony; taryfa taksówkowa jest podejrzanym numer jeden. Rozjazd rośnie razem z wydatkami na dojazdy: +63,2 tys. zł na 40 dób po M5c, **+163,0 tys. zł po M5d** — czyli jest **mnożnikowy, nie addytywny** | decyzja otwarta nr 16 `M5` §9 — trzyma **bramkę 7 fazy M5** i nie należała do żadnego pakietu M5…M9 | R2-WP32 (`K-72`) | `[→]` **przeniesiona** do `R3-etap-7-i-dlug-strukturalny.md` w drugiej połowie (poz. 78): kanały ruchu są kontami (`K-72`), a niezmiennik świata rozjeżdża się na granicy miesiąca |
| 61 | **Trzynaście z czterdziestu jeden benchmarków nie ma wpisu w linii bazowej**, więc `bench_guard` nie mierzy dla nich niczego — i robi to cicho, bo brak wpisu jest informacją, nie błędem. Dotyczy całego planera (`plan_day`, `plan_day_explained`, `replan`), mikro pieszych, `estimate` z cache, trzech pozycji demografii, Etapu 8 i czterech pozycji indeksu parcel. Ta sama klasa co poz. 38b: bramka raportuje zielono, nie sprawdzając tego, co myśli, że sprawdza | `R1` `D-R8` — opisane w `00-postep.md` jako „otwarte z adresem", **adresu nigdy nie wpisano** | R2-WP33 | `[x]` **zamknięte w R2f** (`R2-WP33`): brak wpisu w linii bazowej daje kod wyjścia 1 z nazwą benchmarku, `bench_guard --self-test` ma sześć przypadków, a `D-N21` mówi, kiedy linię bazową się odnawia |
| 62 | **Scenariusz `export_drains` nie istnieje.** `M6` §7.7 wymienia go jako test kryterium WP9 („eksport mierzalnie podnosi ceny lokalne"); `AH-12` zawęziło kryterium do samego drenażu masy, a `AI-8` przeniosło pomiar cen za WP11. M6e zamknęło się bez niego i nie ma go ani w `data/scenarios/`, ani nigdzie w kodzie. Druga połowa kryterium fazy M6 nie została zmierzona i nic tego nie pilnuje | zapis `M6c` `AH-12`, `M6` `AI-8` — adresat `M6e` wyczerpany | R2-WP34 | `[x]` **zamknięte w R2f** (`R2-WP34`): scenariusz `export-drains` jest w `tools/headless`, wpięty do biegu nocnego i **pierwszy woła `B2b::try_export` poza testami**. Druga połowa kryterium WP9 zamyka się **pomiarem**, zgodnie z dopuszczeniem w zakresie pakietu: wywieziono 0 kg, a powód nie jest cenowy — patrz poz. 83 i `AS-1` w `M6-lancuch-dostaw.md` |
| 63 | **`UtilityKind` i `UtilityService` żyją w kodzie obie naraz.** `CB-1` w M8 zarządziło zmianę nazwy; wykonana jest w połowie — oba identyfikatory są w `engine/core` (`vocab.rs`, `decision.rs`, `lib.rs`) i oba mają czytelników. Dwie nazwy na jedno pojęcie to dokładnie ten rodzaj długu, który R2-WP22 zbiera | zapis `M8` `CB-1` (zarządzone, niedokończone) | R2-WP22 | `[x]` **zamknięte w R2e** (`R2-WP22`, `E-11`) sprostowaniem: to dwa różne pojęcia, oba żywe, a różnica jest nazwana w `vocab.rs` |
| 64 ★ | **Poprawka wędrująca w przód nie ma egzekutora po stronie adresata.** Tabela korekt fazy potrafi wskazać dokument docelowy, a rzeczy w nim nie ma — sprawdzenie dwudziestu wierszy wskazujących inny plik dało **osiem trafień bez pokrycia pod wskazanym adresem**, w tym obie pozycje `CJ-9`. Dla rejestru długu tę samą chorobę leczy R2-WP26; dla tabel korekt nie ma nic. To jest przyczyna, dla której pozycje 58–62 w ogóle powstały | — | R2-WP26 (rozszerzenie zakresu) | `[x]` **zamknięte w R2f** (`R2-WP26`, reguła 3 `plan_guard`): wiersz tabeli korekt wskazujący dokument musi mieć w nim pokrycie. Pierwszy przebieg złapał **trzy niespełnione obietnice** fazy M8 wobec M8e (`CD-6`, `CH-5`, `CH-6`) |
| 65 | **`CLAUDE.md` mówi o trzech formach liczebnika polskiego, kod ma cztery.** Reguła lokalizacji brzmi „polski ma trzy formy (1 · 2–4 · 5+)"; `Locale::plural_forms` zwraca dla polskiego **4**, bo `DE-8` w M9b dołożyło CLDR-owe `other` dla wartości ułamkowych („1,5 sklepu"). Kod ma rację, reguła jest nieaktualna — a to jest dokument, który każda nowa sesja czyta jako wiążący | zapis `M9b` `DE-8` (zmiana wykonana, reguła nietknięta) | R2-WP23 | `[x]` **zamknięte w R2e** (`R2-WP23`, `E-4`, test `engine/ui/tests/loc_arity.rs`) |

**Bilans wejściowy:** 1 pozycja miała pakiet (37), 1 okazała się rozstrzygnięta i wypadła
z wykazu (38), 11 było zapisanych jako znana usterka **bez wykonawcy**, a 29 nie było znanych
planowi w żadnej postaci. Doszła jedna pozycja znaleziona przy weryfikacji sprostowania (38b).

**Rzut M9d (47–56):** dwie pozycje były zapisane w tabeli korekt tej samej podfazy, w której
powstały (55, 56 — obie z adresatem, ale bez pakietu), osiem nie było znanych planowi. Siedem
z dziesięciu to kod starszy niż M9d: konwencja nazw ciągnie się od M5, `RemoveFromShelf`
i promień metryki od M7c, `Catalog::must` od M3. Nowe są trzy i wszystkie dotyczą liczby albo
tekstu na granicy interfejsu (49, 51, 52).

**Rzut przeglądu sesji (58–65)** ma inny rozkład niż wszystkie poprzednie i to jest jego jedyny
ciekawy wynik. Trzy wcześniejsze rzuty czytały kod i znajdowały rzeczy, o których plan nie
wiedział — tu jest odwrotnie: **sześć z ośmiu pozycji plan znał i zapisał**, a mimo to nie miały
wykonawcy. Dwie były wprost zaadresowane do tego wykazu i nie dojechały (58, 59), dwie były
decyzją otwartą w dokumencie, który zamknął się bez niej (60 — bramka 7 fazy M5, 61 — `D-R8`
z R1), jedna miała adresata, który zamknął się wcześniej (62), jedna była zarządzona i wykonana
w połowie (63). Tylko 64 i 65 są nowe, a 64 opisuje mechanizm, który wyprodukował pozostałe.

Wniosek jest wąski i dlatego wart zapisania: **liczba pozycji, które plan „zna", nie mówi nic
o tym, ile z nich ktoś zrobi.** Adres bez egzekutora jest dokładnie tak samo skuteczny jak brak
adresu, tylko dłużej wygląda na rozwiązany.

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
| 66 | Dziecko poniżej wieku szkolnego nie blokuje dorosłego w gospodarstwie | zakres `R2-WP5` | R2-WP35 | `[→]` **przeniesiona** do `R3-etap-7-i-dlug-strukturalny.md` (`R3-WP4`): `R2-WP35` był opisany w R2a i **jawnie wyłączony z jej kryterium zamknięcia** (`A-7`), a potem nie dostał wykonawcy |
| 67 | Zakład sprzedający na eksport nie ma utargu w rachunku wyniku | — | R2-WP36 | `[x]` **zamknięte** (test `eksport_zabiera_mase_z_lokalnej_podazy` padał na `seller_site == None`; koszt własny bierze się z ładunku zlecenia, bo `dispatch` zdejmuje partie ze slotu sprzedawcy od razu) |
| 68 | **Strzałki `↑↓←→` wypadły z interfejsu, bo domyślny atlas `egui` ich nie ma** — do czasu M11 gra nie wgrywa własnego kroju (`Theme::font`). Napisy, które ich używały (podpowiedź klawiszy powłoki, nagłówek karty podróży, znacznik wybranej opcji), mówią to samo znakami z atlasu. Po wgraniu kroju w M11 sprawdzić, czy strzałki wracają: test `atlas_fontow_zna_wszystkie_znaki_z_lokalizacji` odpowie na to w jednym przebiegu. Sufit testu jest nazwany: chodzi po **katalogu**, więc nie widzi znaków zaszytych w kodzie rysującym (`>`, `·`, `×` w `engine/ui/src/inspect/trip.rs`, `−` w `fmt.rs`) | zapis `M9b` `DE-16` | M11 (`M11d` albo gdziekolwiek wchodzą fonty) | `[→]` **przeniesiona** do `R3-etap-7-i-dlug-strukturalny.md`: adresatem było M11 (`M11d`), własny krój nie został wgrany, a komentarz w `inspect/trip.rs` odsyła do fazy, która się zamknęła |
| 69 | **Ekran rozgrywki ma dwie z trzech rzeczy, które rysuje `ui-design.md` §5.** Inspekcja jest przeciągalnym oknem `egui` (`tools/magnat/src/citizens.rs`), a nie **dokiem prawym** — więc „lewy prowadzi, prawy pokazuje klikniętego" jest regułą dokumentu, nie ekranu. Pasek czasu nie niesie gotówki ani jej zmiany, choć §5 rysuje je po jego prawej stronie: gracz widzi stan konta tylko po otwarciu pulpitu. Oba są brakiem treści, a nie usterką — układ po naprawie `DI-39` jest już taki, że dok prawy ma gdzie stanąć | zapis `M9e` `DI-39` | **M11c** | `[x]` **zamknięte** (`WP12`, test `pieszy_idzie_ulica_a_nie_przez_kwartal`) |
| 70 | **Trasa pieszego w warstwie Mikro jest odcinkiem prostym między środkami budynków.** `journey.rs::enter_micro_inner` podaje `MicroLayer::enter` dwa punkty (`coord_of(from)`, `coord_of(to)`) i nic więcej — żadnego routingu geometrycznego, żadnego próbkowania terenu. Pieszy idzie więc przez kwartały, a w połowie drogi bywa pod ziemią albo nad nią, bo interpolacja liniowa nie zna niwelety. Węzły grafu pieszego **mają** poprawne `z_cm` (łańcuch `TerrainQuery::height_at` → `lsystem` → `nav_build`) i nikt ich w tej ścieżce nie czyta. Objaw stał się widoczny w M11b, gdy pieszy przestał być plamką i dostał sylwetkę; poprawka `G-13` wyprostowała **końce** trasy (rzędna wejścia zamiast dna fundamentu), środek zostaje | zapis `M11b` `G-13` | **M11c** | `[x]` **zamknięte** (`WP12`, test `pieszy_idzie_ulica_a_nie_przez_kwartal`) |
| 71 ⇧ | **Kadr gry jest pusty, bo okno warstwy Mikro i promień rysowania są zaczepione w oku kamery, a nie w tym, na co gracz patrzy.** Zdiagnozowane po M11b, trzy przyczyny naraz. **(1)** `citizens.rs::okno_mikro` podaje `set_micro_window(camera.eye())`, a przy orbicie z 900 m oko stoi 767 m w poziomie od celu — dysk o promieniu 900 m jest przesunięty o tyle samo, więc połowa okna leży za plecami kamery. **(2)** `DRAW_RADIUS_M = 600` mierzy się **od oka**, a `ViewQuery.aabb` to 720 m wokół oka: przy orbicie 900 m punkt, na który gracz patrzy, jest z definicji poza jednym i drugim, więc w domyślnym widoku dzielnicy nie widać **żadnej** encji. **(3)** `MicroLayer::enter` wpuszcza pieszego tylko w minucie wyruszenia i tylko wtedy, gdy początek albo koniec jego trasy trafia w okno — kto idzie przez kadr, ale mieszka i pracuje poza nim, nie pojawia się nigdy, a po przeskoku kamery nowe okno napełnia się przez kilkanaście minut symulacji. Liczba samych pieszych (24–46) jest przy tym **prawdopodobnie poprawna**: `data/roads/mode_choice.ron` daje dla miasta 28 tys. udział pieszy ~70 % i udział samochodowy ~10 %, a `min_car_distance_m: 800` odcina krótkie dojazdy autem. Rekordy pojazdów mają czytelnika (`view.rs::fill_vehicles`) — pusty jest bufor Mikro, nie kanał | zapis `M11b` `G-12` | **M11c** (przejęte z R2 decyzją właściciela produktu) | `[x]` **zamknięte** (`WP12`; czwarta przyczyna — pojazdy w `[0,0,0]` — znaleziona przy okazji, `J-11`) |
| 72 | **Poza kwadransami szczytu ulica jest pusta.** Plan doby wysyła wszystkich w tej samej minucie, więc miasto ma trzy piki i dwadzieścia godzin ciszy. Zmierzone w M11c na świecie odniesienia (`--seed 7 --size 4km`, mieszkańcy w snapshocie): 7:50 → 4 644, 8:00 → 831, 8:15 → 74, **10:00 → 0**, 14:00 → 2 031, 16:00 → 1 919, 16:30 → 129, **17:00 → 0**, 20:00 → 0. To nie jest usterka prezentacji: warstwa Mikro oddaje dokładnie tych, którzy są w drodze. Rozkład wyruszeń należy do planera doby (M3), a `WP12` fazy M11c jawnie go nie rusza | zapis `M11c` `J-12` | **R2-WP37** (pierwsza połowa) | `[→]` **przeniesiona** do `R3-etap-7-i-dlug-strukturalny.md` w drugiej połowie: `K-92` naprawiło grafik zmianowy, a rozkład wyjść z domu w ciągu doby zostaje bez zmian |
| 73 | **`debug_assert` w kolejce zdarzeń wywraca każdy przebieg pętli doby w profilu testowym.** `des.rs` sprawdza, że klucz porządku zdarzeń jest **totalny**, i w mieście 4 km z czterema tysiącami mieszkańców trafia na duplikat w minucie 469: „dwa zdarzenia o identycznym kluczu — porządek przestał być totalny, a wynik zaczął zależeć od kolejności wstawiania". Skutkiem jest to, że **wszystkie testy `tools/headless/tests/full_city.rs` są czerwone pod `cargo test`** i zielone dopiero pod `cargo test --release`, gdzie asercja nie istnieje. Zarzut jest przy tym prawdziwy: jeśli klucz nie jest totalny, kolejność dwóch zdarzeń zależy od tego, które wstawiono pierwsze, a to jest wprost naruszenie 00 §3 | zapis `M11c` (znalezione przy `R2-WP17`) | — | `[→]` **przeniesiona** do `R3-etap-7-i-dlug-strukturalny.md`: `debug_assert` stoi nietknięty w `sim/agents/src/des.rs`, a pozycja nigdy nie dostała pakietu. Do R3 test `doba_przez_systemy_ecs_planuje_dowozi_i_zaspokaja` jest pomijany w CI **imiennie, po nazwie**, a nie przez wyciszenie całego kroku |
| 77 ★ | **W profilu `release` żadne gospodarstwo nie dostawało członków.** `spawn_household_aged` dopisywało mieszkańca do składu **wewnątrz** `debug_assert!`, a `debug_assert!` nie oblicza swojego argumentu w release. Skutek: miasto z samych pustych gospodarstw — nikt nie dostawał etatu, nikt nie miał dochodu, nikt niczego nie kupował, a `m5shop --days 40` kończył się „przez 40 dób nikt nic nie kupił”. Debug pokazywał świat zdrowy, więc `cargo test` był zielony i nic tego nie łapało — a **CI mierzy release**, więc od `R2-WP3` każdy nocny przebieg mierzył miasto widmo. Wprowadzone przez `K-91` razem z `#[must_use]` na `add_member`, znalezione przy pomiarze `R2-WP32` | — | **`R2-WP8`** (naprawione: wywołanie stoi poza asercją w trzech miejscach `migration.rs`) | `[x]` **zamknięte** |
| 78 | **Niezmiennik pieniądza świata rozjeżdża się na granicy miesiąca.** Po zamknięciu kanałów ruchu (`K-72`) rozjazd zmienił znak z **+107 812 zł** na **−49 539 zł** na 31 dób (4 km, ziarno 1) — i dopiero wtedy dało się go zlokalizować: przebieg 3-dobowy domyka się **co do grosza**, 29-dobowy rozjeżdża o **+2 430 gr**, a 31-dobowy o **−4 953 885 gr**. Cała kwota wchodzi więc w **dobie 30**, czyli w `pay_incomes`, `settle_household_month`, `close_month_with` albo w miesiącu demograficznym — a nie w ruchu. Pieniądz **znika**, a nie powstaje, więc szukać należy komponentu, z którego kwota schodzi bez `household_pay`. Jeden podejrzany został przy okazji odrzucony: zapis zwrotny po `Entity::new(index, MIN)` w `settle_household_month` (poprawiony na `household_by_index`, rozjazd bez zmian) | — | **bez pakietu** — kandydat do R2f (bramka niezmiennika) albo R3 | `[→]` **przeniesiona** do `R3-etap-7-i-dlug-strukturalny.md`: rozjazd jest zlokalizowany co do doby i znaku, przyczyna nie |
| 74 | **Eksportu nie woła w grze nikt.** `B2b::try_export` nie ma w repozytorium ani jednego wołającego poza testami, więc mechanizm drenażu podaży — łącznie z utargiem eksportowym z `R2-WP36` — nie uruchamia się w żadnym wygenerowanym mieście. To ta sama klasa co pozycje 3, 5 i 21: kod napisany, przetestowany i martwy. Znalezione przy `R2-WP36` | — | **R2-WP34** (scenariusz `export_drains` i druga połowa kryterium WP9 fazy M6) | `[x]` **zamknięte w R2f** (`R2-WP34`): scenariusz `export-drains` jest pierwszym wołającym `B2b::try_export` poza testami |
| 75 | **Dochód nie idzie za mieszkańcem, który zmienia gospodarstwo.** `Household.income_monthly` prowadzą zatrudnienie, zwolnienie i podwyżka, a **nie** wyprowadzka z gniazda ani rozstanie: dwudziestopięciolatek zakłada dom z dochodem zero, a jego płaca zostaje w dochodzie rodziców. Od `K-93` nie jest to już cicha niespójność — `+wage` i `−wage` trafiają zawsze po tej samej stronie, bo umowa pamięta gospodarstwo — ale liczba nadal opisuje nieprawdę. Znalezione przy `R2-WP9` | — | **bez pakietu** — kandydat do R3, bo domknięcie wymaga przeniesienia płacy razem z umową, czyli dotknięcia rejestru firm z `sim/agents` | `[→]` **przeniesiona** do `R3-etap-7-i-dlug-strukturalny.md`: domknięcie wymaga przeniesienia płacy razem z umową, czyli dotknięcia rejestru firm z `sim/agents` — to mechanika, której `R2` §2 zabrania |
| 76 | **Miasto nie jest pracodawcą** (`CH-4`). Most `game::world::firms` **pomija** zakłady municypalne (`skipped_municipal`, „ich firmy stawia M8”), więc nauczyciela nie ma w rejestrze firm w ogóle, a plan wydatków dalej wychodzi na konto reszty świata. Łańcuch „budżet miasta → pensja nauczyciela → jakość szkoły” zostaje przerwany, choć `R2-WP30` domknęło jego drugą połowę: skrzynka płac ma konsumenta i pieniądz wychodzi od pracodawcy — tam, gdzie pracodawca istnieje | zapis `M8` `CH-4`, zakres `R2-WP30` | **bez pakietu** — postawienie firmy miasta, zakładów dla placówek i realnej listy płac jest **mechaniką**, której `R2` §2 zabrania; adres: M8 albo R3 | `[→]` **przeniesiona** do `R3-etap-7-i-dlug-strukturalny.md`: postawienie firmy miasta i realnej listy płac jest **mechaniką**, a nie domknięciem |
| 79 | **Zamiennika nikt nie zamawia.** Od `R2-WP14` szósty szczebel kaskady faktycznie karmi linię (`K-97`), ale kaskada, zapytania ofertowe i punkty zamówieniowe chodzą wyłącznie po `RecipeInput.good` (`shortage::wejscia_zakladu`), a towar-zamiennik wejściem receptury nie jest. Podmiana zadziała więc tyle razy, ile zamiennika stoi w magazynie **przypadkiem**: w łańcuchu odniesienia otręby są produktem ubocznym młyna, więc zwykle stoją, ale reguły na to nie ma i w innym łańcuchu nie będzie. Sufit nazwany w `plant::produce::podmiana`. Znalezione przy recenzji przed commitem `R2c` | — | **bez pakietu** — kandydat do R3 albo do fazy dotykającej zaopatrzenia: zapytanie ofertowe na zamiennik w chwili wejścia na `Substituted` zmienia to, **co firma kupuje**, czyli jest mechaniką, nie domknięciem | `[→]` **przeniesiona** do `R3-etap-7-i-dlug-strukturalny.md`: zapytanie ofertowe na zamiennik zmienia **to, co firma kupuje**, czyli jest mechaniką |
| 80 | **Priority-flood (P4) na kopcu binarnym kosztuje 1,10 s na mapie 16 km i to on blokuje przechwytywanie rzeczne.** `R2-WP19` zbudował mechanizm przetrasowania w trakcie erozji, zmierzył go i musiał zostawić wyłączony: przetrasowanie kosztuje P4 + P5 = 1,38 s, a do celu 10 s zostaje 1,2 s. Rzadziej się nie da — jedno przetrasowanie na przebieg to minimum. Kolejka jest przy tym **wprost wymienialna**: `flood::fill` pracuje w milimetrach całkowitych (`i32`), więc klucz jest ograniczonym intem i kolejka kubełkowa daje ten sam wynik bit w bit, bez zmiany macierzy hashy. Znalezione pomiarem w `R2-WP19` | — | **bez pakietu** — kandydat do R3 albo do M12 (profilowanie); dopiero po nim `reroutes` w `data/geology/erosion.ron` ma prawo być większe od zera | `[→]` **przeniesiona** do `R3-etap-7-i-dlug-strukturalny.md`: przyspieszenie priority-flood jest optymalizacją, a nie naprawą; dopiero po nim `reroutes` ma prawo być większe od zera |
| 81 ★ | **Bramka G9 („wyjaśnialność") świeci na czerwono od `R2b` i nikt tego nie zobaczył.** Zmierzone w R2f na profilu `ci` (4 ziarna × 120 dób, scenariusz `base`): **61 357 decyzji bez powodu na 1 820 626 próbkowanych**, czyli 3,4 % wobec progu **zero**. Przyczyna jest jedna i pochodzi z jednego commita: `R2-WP32` (`K-72`) wpisało `TxKind::Mobility` po stronie „wybór" w `jest_decyzja` i w tym samym podejściu utworzyło `sim/economy/src/mobility.rs`, który księguje **dobowy agregat per kanał** z `DecisionReason::Unspecified`. Powodu pojedynczego przejazdu nie da się do agregatu przypiąć, więc bramka wymagała powodu, którego z konstrukcji nie ma. Druga, mniejsza ścieżka jest zastana: `Books::inject_external_capital` wpisywało `Unspecified` **w środku funkcji**, a wołający nie miał czym tego naprawić | — | **R2-WP39** | `[x]` **zamknięte** (`Mobility` przechodzi na stronę zobowiązań z pomiarem w komentarzu, `inject_external_capital` przyjmuje powód i dostaje `FirmReason::ChainEntered`; przed naprawą 61 357/1 820 626, po naprawie zero — pomiar w dzienniku) |
| 82 | **Bramka G4 nie umie zmierzyć szoku, a G12 rozjeżdża się o 886 ‰.** Zmierzone w R2f na przebiegu `supply-shock` (200 dób, ziarno 1), pierwszym po zdjęciu z obu bramek doradczości: G4 zwraca `t_response: None` i `t_settle: None`, czyli mediana ceny szokowanego towaru **nie drgnęła w ogóle** — inaczej niż w M5e, gdzie było 2 doby i 5 dób. G12 daje medianę odchylenia pieniądza gospodarstw **886 ‰** i autokorelację znaku 100/100, mimo że `PayrollOutbox` dostała konsumenta w `R2b` (`K-93`) — czyli warunek, pod którym miała przestać być doradcza, spełnił się, a liczba nie drgnęła | — | **R3** | `[→]` **przeniesiona z pomiarem** do `R3-etap-7-i-dlug-strukturalny.md`: obie bramki mają od R2f werdykt blokujący i obie są czerwone. To jest **wynik**, a nie usterka R2f — bramka bez werdyktu była wykresem, a wykres nie mówił, że model makro i kanał szoku są zepsute |
| 83 ★ | **Żaden zakład nie trzyma wyrobu w slocie wyjściowym, więc eksport nie ma czego wywieźć.** Zmierzone w `R2-WP34` na świecie odniesienia (4 km, `industrial`, ziarno 1, 40 dób): pięć towarów w obrocie granicznym o największym zapasie — `raw_crude_oil` 489 t, `raw_wheat` 209 t, `chem_plastic_granule` 165 t, `raw_milk` 140 t, `mat_leather` 130 t — ma **zero kilogramów** w slocie wyjściowym któregokolwiek zakładu. Zapas w mieście jest, ale leży w slotach **wejściowych** cudzych zakładów i na placu bramy granicznej. Drugi objaw tej samej przyczyny: wyrób gotowy (`food_milk`) ma 37 t w dobie zero i **zero od doby pierwszej**, bo idzie na półkę tego samego dnia | — | **R3** | `[→]` **przeniesiona z pomiarem**: to nie jest usterka eksportu ani cen, tylko kształt przepływu masy przez zakład — wyrób nie leży u producenta ani minuty. Nazwanie tego usterką wymaga rozstrzygnięcia, czy zakład **ma** trzymać zapas wyrobu, a to jest decyzja o modelu, nie domknięcie |
| 84 | **Bramka `macro-kernel` w CI jest czerwona i była czerwona przed R2f.** `python scripts/macro_kernel_guard.py` zwraca kod 1 na dwóch liniach `sim/macro/src/step/labor.rs` (190 i 229): skala punktu bazowego w działaniu zamiast `kernel::apply_bp`. Sprawdzone na `HEAD` przez `git stash` — to nie jest skutek żadnej zmiany R2f. Reguła `K-50` („`sim/macro` woła jądro, nie liczy sam”) jest jedyną obroną przed ryzykiem `R1` fazy M10, czyli dwoma modelami gospodarki, które rozjeżdżają się i których rozjazdu nie widać w żadnym teście | — | **R3** | `[→]` **przeniesiona z pomiarem**: R2f zajrzało we wszystkie bramki CI i to jest trzecia, która świeciła na czerwono bez czytelnika — po G9 (poz. 81) i po `struct_guard` z rejestrem długu (poz. 35). Naprawa dotyka **kroku makro**, czyli modelu, a nie pomiaru |
| 85 ★★ | **`.github/workflows/ci.yml` nie jest poprawnym YAML-em i nie był nim od M10a, więc GitHub Actions nie wczytywał tego pliku — czyli nie biegł ani jeden job.** Nazwa kroku „M10a — artefakt A fazy M10: raport historii „na sucho”” niosła **dwukropek ze spacją** bez cudzysłowu, a to w YAML-u jest separator klucza od wartości. Sprawdzone na `HEAD` przez `yaml.safe_load`: `mapping values are not allowed here, line 245`. Skutek obejmuje **wszystko**: `cargo test`, `clippy`, determinizm, Miri, obie macierze hashy, bramki balansatora, `struct_guard`, `plan_guard`, `lang_guard`, `bench_guard` i raport budżetów | — | **R2f** | `[x]` **zamknięte** (jedna para cudzysłowów; `plan_guard` dostaje regułę 4 — workflow musi się wczytywać, sprawdzane przez `yaml.safe_load`, a bez tego modułu wzorcem na tę jedną klasę). Bramka stoi **lokalnie i w jobie**, bo CI jest dokładnie tym, co się zepsuło: pierwsze zadziała zawsze, drugie dopiero, gdy plik znów się wczyta |

## 12. Szacunek wielkości

| Podfaza | Pakiety | Pliki dotknięte | Testy nowe | Rozmiar |
|---|---|---|---|---|
| `R2a` | 8 | ~20 w `sim/agents`, `sim/world/population`, `sim/economy/labor`, `sim/firms` | 13 | L |
| `R2b` | 7 | ~14 w `sim/economy`, `sim/agents`, `sim/firms`, `sim/traffic`, `sim/city` | 13 | L |
| `R2c` | 5 | ~11 w `sim/traffic`, `sim/supply`, `sim/agents`, `data/` | 9 | M |
| `R2d` | 3 | ~7 w `sim/world` | 5 | L |
| `R2e` | 7 | ~370 miejsc w 10 crate'ach (sam R2-WP20); R2-WP27 zależny od `D-N19` | 10 | L |
| `R2f` | 6 | `tools/balansator`, `sim/world/tests`, `scripts/`, `benches/`, `data/scenarios/` | 7 | L |
| **Razem** | **36** | — | **57** | — |

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
