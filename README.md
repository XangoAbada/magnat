# Magnat

Symulator miasta i gospodarki. Własny silnik w Rust — ECS, generacja świata, renderer na `wgpu`.
Deterministyczny: ten sam seed daje ten sam świat i ten sam przebieg, niezależnie od liczby wątków
i platformy.

Stan: **M8 zamknięte, M9a zamknięte** (miasto, ludzie, ruch, gospodarka, łańcuch dostaw, firmy AI,
podatki, sieci przesyłowe, zdarzenia świata — oraz sesja gry z dziennikiem wejść i odtworzeniem).
Szczegóły postępu:
[`docs/implementation-plan/00-postep.md`](docs/implementation-plan/00-postep.md).

## Wymagania

- Rust **1.96.0** — wersja jest przypięta w `rust-toolchain.toml`, `rustup` pobierze ją sam.
- GPU z obsługą Vulkan / DX12 / Metal — tylko dla binarki `magnat`. `magnat-headless` nie
  dotyka GPU.
- Python 3 — tylko dla `scripts/bench_guard.py` (progi regresji benchmarków).

## Uruchomienie

```bash
git clone <repo> && cd magnat
cargo run --release -p magnat                 # gra: menu główne, kreator, miasto
cargo run --release -p magnat-headless -- generate   # generacja bez GPU
```

`--release` nie jest opcjonalny w praktyce: debug jest kilkadziesiąt razy wolniejszy.

### `magnat` — gra i podgląd świata

Bez argumentów gra prowadzi od menu głównego przez kreator do miasta. Menu ma dwie drogi
do świata: **Nowa gra** (gracz jest jednym z mieszkańców) i **Tryb przeglądu** (gracz
nikogo nie prowadzi, ogląda i klika). Podany parametr świata omija menu i stawia miasto
od razu — to jest droga dla zrzutów, pomiarów i testów, nie dla gracza.

```bash
cargo run --release -p magnat -- --seed 0xC0FFEE --size 8km --region river
```

| Flaga | Domyślnie | Znaczenie |
|---|---|---|
| `--seed` | `0xC0FFEE` | ziarno świata, dziesiętnie albo `0x…` |
| `--size` | `8km` | `4km` \| `8km` \| `12km` \| `16km` |
| `--region` | `river` | `coastal` \| `mountain` \| `lowland` \| `river` \| `desert` |
| `--epoch` | `1990` | `1950` \| `1970` \| `1990` \| `2010` \| `2020` |
| `--profile` | `mixed` | `industrial` \| `port` \| `university` \| `tourist` \| `agricultural` \| `mixed` |
| `--threads` | `0` | wątki generacji i meshingu; `0` = liczba rdzeni |
| `--hour`, `--day` | `10`, `170` | pora dnia (0–23) i dzień roku (0–359) — sterują słońcem |
| `--target x,y` | środek | punkt w metrach, nad którym staje kamera |
| `--dist` | `900` | wysokość orbity kamery w metrach |
| `--overlay` | — | nakładka na starcie: `height`, `flow`, `water`, `biome`, `temp-jan`, `temp-jul`, `precip`, `geology`, `deposits` |
| `--observe` | — | tryb przeglądu: świat bez postaci gracza i bez ekranu wyboru |
| `--inspect x,y` | — | wypisuje kartę inspekcji punktu i kończy, bez okna |
| `--screenshot plik.png` | — | zrzut jednej klatki do pliku i wyjście |
| `--screenshot-after` | `120` | ile klatek odczekać przed zrzutem (strumieniowanie) |
| `--bench <sekundy>` | — | ustalony przelot pomiarowy z raportem FPS |
| `--lights`, `--lod0-radius` | `0`, — | sceny obciążeniowe renderera |

Sterowanie w oknie:

| | |
|---|---|
| lewy przycisk + ruch | obrót kamery |
| kółko | zoom |
| prawy przycisk | inspekcja punktu — karta na konsolę |
| `1` / `2` / `3` | kamera orbitalna / swobodna / z poziomu oczu |
| `W` `S` `A` `D` `Q` `E` | ruch w kamerze swobodnej |
| `F3` | następna nakładka debug |
| `T` | czas ×1000 |
| `Esc` | w grze — menu pauzy; na ekranie powłoki — o jeden ekran wstecz; w menu głównym — wyjście |

### `magnat-headless` — symulacja bez GPU

```bash
# generacja świata: raport czasów, hash terenu, statystyki
cargo run --release -p magnat-headless -- generate --seed 1 --size 4km --region lowland --out swiat.mgw

# dwa przebiegi tego samego ziarna przy 1 i 8 wątkach → porównanie hashy
cargo run --release -p magnat-headless -- verify --seed 1 --threads 1,8

# podgląd pola generatora jako PNG (height | water | biome | temp | precip | fertility)
cargo run --release -p magnat-headless -- preview --field biome --max-px 1024

# sesja gry z pliku parametrów — bez ani jednego argumentu opisującego świat
cargo run --release -p magnat-headless -- new-game --from params.ron --days 2 \
    --price 250 --record sesja.log.ron --out sesja.hashes

# to samo jeszcze raz, z nagranego dziennika — ten sam łańcuch hashy co do bitu
cargo run --release -p magnat-headless -- new-game --replay sesja.log.ron --days 2 \
    --expect sesja.hashes

# bez podpolecenia: przebieg ticków świata syntetycznego (tryb M0)
cargo run --release -p magnat-headless -- --seed 42 --entities 100000 --ticks 20000 \
    --hash-every 1000 --threads 8 --out run.hashes
```

Przydatne flagi trybu bez podpolecenia: `--save`/`--load` (stan świata), `--expect plik.hashes`
(porównanie z zapisanym przebiegiem, rozjazd → kod wyjścia 1 i raport różnic encja po encji),
`--metrics plik.csv`, `--interactive`.

## Testy i bramki

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings   # egzekwuje zakaz libm i HashMap w symulacji
cargo test --workspace
```

Testy długie (macierz hashy terenu, budżety pamięci i czasu) są `#[ignore]` — biegną w CI
i lokalnie tylko na żądanie:

```bash
cargo test --release -p magnat-world --test determinism --test budgets -- --include-ignored --nocapture
```

Benchmarki i próg regresji (ostrzeżenie od 10 %, błąd od 25 %):

```bash
cargo bench -p magnat-core -p magnat-ecs -p magnat-io -p magnat-voxel -p magnat-world --benches
python3 scripts/bench_guard.py benches/baseline.json
```

FPS mierzy się osobno, na maszynie z GPU: `cargo run --release -p magnat -- --bench 60`.

## Układ repozytorium

```
engine/     core ecs jobs io devtools render voxel ui nav spatial
                                                    — silnik, bez wiedzy o domenie
sim/        world agents traffic economy supply firms policy city events macro snapshot
                                                    — świat i jego migawka dla renderera
game/       sesja, komendy gracza, dziennik replayu, powłoka — i **jedyne** miejsce,
            w którym świat wstaje (`game::world::stand_up`, K-68)
tools/      magnat (klient) · headless (scenariusze) · balansator · goods-graph
data/       climate geology materials                — dane wejściowe, RON
docs/implementation-plan/                            — plan faz M0…M12
benches/    baseline.json                            — linia bazowa benchmarków
```

Renderer widzi symulację **wyłącznie** przez `sim/snapshot`, a `magnat-headless` nie linkuje
`wgpu` — jedno i drugie pilnuje CI.

## Dokumentacja

- [`PRD_Magnat.md`](PRD_Magnat.md) — wymagania produktu.
- [`docs/implementation-plan/00-konwencje-i-kontrakty.md`](docs/implementation-plan/00-konwencje-i-kontrakty.md)
  — kontrakt nadrzędny: typy bazowe, determinizm, rozstrzygnięcia `K-n`.
- [`CLAUDE.md`](CLAUDE.md) — konwencje pracy nad kodem.
