# Benchmarki i linia bazowa

`baseline.json` to mediany z `cargo bench --workspace`, w nanosekundach, kluczowane
nazwą benchmarku z criterion. Porównuje z nią `scripts/bench_guard.py` (D-8: 10 %
ostrzeżenie, 25 % błąd).

## Sprzęt odniesienia dla progów bezwzględnych z M0 §7.3

| | |
|---|---|
| CPU | 16 wątków, x86-64 |
| System | Windows 11 (MSVC) |
| Profil | `--release` (`lto = "thin"`, `codegen-units = 1`) |
| Toolchain | pinowany w `rust-toolchain.toml` |

**Progi bezwzględne z §7.3 są celami projektowymi na tym sprzęcie.** Bramką CI jest
regresja **względna**: współdzielone runnery mają rozrzut kilkunastu procent między
przebiegami, więc porównywanie ich z progiem bezwzględnym produkowałoby wyłącznie
fałszywe alarmy. Wyniki wobec celów są wypisane w `docs/implementation-plan/M0-fundament-silnika.md` §4a.

## Aktualizacja linii bazowej

Po **świadomej** zmianie wydajności (optymalizacja albo zaakceptowany koszt nowej
funkcji):

```
cargo bench --workspace
python scripts/bench_guard.py benches/baseline.json --update
```

Zmiana `baseline.json` bez wyjaśnienia w opisie commita jest sygnałem ostrzegawczym —
to jedyny plik w repozytorium, którego edycja potrafi uciszyć bramkę wydajności.
