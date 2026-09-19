# Budżet klatki — sceny odniesienia

Raporty siedmiu scen z §7.2 dokumentu `docs/implementation-plan/M11-prezentacja.md`
plus linia bazowa, do której porównuje je `scripts/frame_guard.py`.

## Jak zmierzyć

```
cargo build --release -p magnat
for s in bench_street bench_district bench_city bench_night_rain \
         bench_blackout bench_interiors bench_winter; do
  ./target/release/magnat --bench-scene $s
done
python scripts/frame_guard.py bench/frames
```

Scena **definiuje cały przebieg**: ziarno, rozmiar świata, dobę, godzinę, kamerę, tłum
i pogodę. Argumenty, które scena posiada, są przy `--bench-scene` nadpisywane; zostają
`--threads`, `--no-audio`, `--bench-warmup` i `--bench-frames`, bo zmieniają przebieg,
a nie kadr. Kod wyjścia klienta mówi, czy scena zmieściła się w progu.

Symulacja stoi na pauzie (`--speed 0`), a **zegar prezentacji idzie**: sześćset klatek
mierzy ten sam świat, ale faza klipu, cząstki pogody i rampa wygaszenia dzielnicy
zachowują się tak jak w grze.

## Progi

Cele PRD §20.2 to 60 FPS dla widoku dzielnicy i ulicy oraz 30 FPS dla widoku miasta,
i dotyczą **„GPU średniej klasy 2024"**. Pomiar idzie na karcie wyraźnie szybszej, więc
próg jest zaostrzony mnożnikiem `ZAPAS = 0,55` (`tools/magnat/src/scenes.rs`):

| | maszyna | cel §20.2 | próg tutaj |
|---|---|---|---|
| pomiar | RTX 4070 Ti SUPER @ 1080p | — | — |
| obietnica | RTX 4060 @ 1080p | 16,6 / 33,3 ms | 9,13 / 18,32 ms |

Mnożnik jest **jawną stałą, a nie liczbą wtopioną w progi**: kiedy ktoś zmierzy te same
sceny na maszynie docelowej, zmienia się jedna linia i raport przestaje zgadywać.

## Powtarzalność — ile wolno uznać za zmianę

Cztery przebiegi `bench_district` pod rząd, ta sama maszyna, ten sam kod:

| | zakres | rozrzut |
|---|---|---|
| GPU p50 | 4,04 – 4,27 ms | **5,7 %** |
| GPU p95 | 5,88 – 6,49 ms | **10,3 %** |
| GPU p99 | 6,58 – 7,67 ms | 16,7 % |
| CPU p95 | 0,583 – 0,596 ms | 2,2 % |

Stąd dwie metryki i dwie role — **korekta §7.2, poparta tym pomiarem**:

* **próg bezwzględny** („czy scena mieści się w celu PRD §20.2") zostaje na **p95**;
  to on jest kryterium fazy i nie zmienia się;
* **regresja** („czy coś zwolniło") idzie po **p50** z progiem 10 %, bo przy p95
  próg 8 % zapalałby się na samym rozrzucie sterownika i stanu zegarów karty.

Ta sama przyczyna przestawiła kryterium `bench_blackout` ≤ `bench_night_rain`: obie
sceny to dwa osobne procesy, a między nimi karta stoi na innym zegarze, więc passy,
których blackout nie dotyka z konstrukcji (`depth_prepass`, `water`, `post`), potrafią
różnić się o 40–70 %. Porównanie idzie po **liczbie świateł i passie `clusters`** —
jedynym, który od listy świateł zależy.

## Linia bazowa

`baseline.json` trzyma p50 i p95 czasu GPU każdej sceny. Bramka zapala się przy
regresji p50 powyżej **10 %** i przy przekroczeniu progu bezwzględnego. Aktualizacja po
świadomej zmianie wydajności:

```
python scripts/frame_guard.py bench/frames --update
```

Linia bazowa jest związana z maszyną — plik zapisuje jej nazwę. Porównanie raportu
z jednej karty do linii bazowej z drugiej mierzy sprzęt, nie kod.

## Czego CI nie robi

Progów klatkowych **nie sprawdza CI**: wspólne runnery nie mają karty graficznej,
a czas GPU bez niej nie istnieje. CI weryfikuje poprawność (testy §7.1 bez GPU)
i mikrobenchmarki procesora (`scripts/bench_guard.py`); progi klatkowe należą do
nocnego biegu na maszynie referencyjnej — tak stanowi §7.2 dokumentu fazy.
