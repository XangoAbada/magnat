"""Bramka regresji benchmarków (M0 WP-14, decyzja D-8).

Czyta mediany z `target/criterion/**/new/estimates.json`, porównuje z linią bazową
i zwraca kod wyjścia: 0 przy zgodności, 0 z ostrzeżeniem powyżej 10 %, 1 powyżej 25 %.

Progi są **względne**, nie bezwzględne: współdzielone runnery CI mają rozrzut rzędu
kilkunastu procent między przebiegami, więc bezwzględne cele z M0 §7.3 pilnuje
człowiek na sprzęcie odniesienia, a CI pilnuje wyłącznie tego, czy coś nie zwolniło
względem poprzedniego stanu tego samego repozytorium.

Aktualizacja linii bazowej (po świadomej zmianie wydajności):
    python scripts/bench_guard.py benches/baseline.json --update
"""

import argparse
import json
import pathlib
import sys

OSTRZEZENIE = 0.10
BLAD = 0.25


def zbierz_mediany(katalog: pathlib.Path) -> dict[str, float]:
    """Mediana czasu w nanosekundach dla każdego benchmarku."""
    wyniki: dict[str, float] = {}
    for plik in katalog.glob("**/new/estimates.json"):
        nazwa = plik.parent.parent.relative_to(katalog).as_posix()
        if nazwa.endswith("/base") or "report" in nazwa:
            continue
        with plik.open(encoding="utf-8") as f:
            dane = json.load(f)
        mediana = dane.get("median", {}).get("point_estimate")
        if mediana is not None:
            wyniki[nazwa] = float(mediana)
    return wyniki


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("baseline", type=pathlib.Path)
    parser.add_argument("--criterion-dir", type=pathlib.Path, default=pathlib.Path("target/criterion"))
    parser.add_argument("--update", action="store_true", help="nadpisz linię bazową bieżącymi wynikami")
    args = parser.parse_args()

    biezace = zbierz_mediany(args.criterion_dir)
    if not biezace:
        print(f"brak wyników criterion w {args.criterion_dir} — czy `cargo bench` się wykonał?")
        return 1

    if args.update:
        args.baseline.parent.mkdir(parents=True, exist_ok=True)
        with args.baseline.open("w", encoding="utf-8") as f:
            json.dump(dict(sorted(biezace.items())), f, indent=2, ensure_ascii=False)
            f.write("\n")
        print(f"zapisano {len(biezace)} pomiarów do {args.baseline}")
        return 0

    if not args.baseline.exists():
        print(f"brak linii bazowej {args.baseline}; utwórz ją przez --update")
        return 1

    with args.baseline.open(encoding="utf-8") as f:
        bazowe = json.load(f)

    kod = 0
    for nazwa, wartosc in sorted(biezace.items()):
        odniesienie = bazowe.get(nazwa)
        if odniesienie is None:
            print(f"NOWY   {nazwa}: {wartosc / 1000:.1f} µs (brak w linii bazowej)")
            continue
        zmiana = (wartosc - odniesienie) / odniesienie
        etykieta = "OK    "
        if zmiana > BLAD:
            etykieta, kod = "BŁĄD  ", 1
        elif zmiana > OSTRZEZENIE:
            etykieta = "UWAGA "
        print(f"{etykieta} {nazwa}: {zmiana * 100:+.1f} % ({wartosc / 1000:.1f} µs)")

    brakujace = set(bazowe) - set(biezace)
    for nazwa in sorted(brakujace):
        print(f"BRAK   {nazwa}: benchmark zniknął z zestawu")

    return kod


if __name__ == "__main__":
    sys.exit(main())
