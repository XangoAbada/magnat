"""Bramka regresji benchmarków (M0 WP-14, decyzja D-8; próg „brak wpisu" — R2-WP33).

Czyta mediany z `target/criterion/**/new/estimates.json`, porównuje z linią bazową
i zwraca kod wyjścia: 0 przy zgodności, 0 z ostrzeżeniem powyżej 10 %, 1 powyżej 25 %.

Progi są **względne**, nie bezwzględne: współdzielone runnery CI mają rozrzut rzędu
kilkunastu procent między przebiegami, więc bezwzględne cele z M0 §7.3 pilnuje
człowiek na sprzęcie odniesienia, a CI pilnuje wyłącznie tego, czy coś nie zwolniło
względem poprzedniego stanu tego samego repozytorium.

**Brak wpisu w linii bazowej jest błędem, nie informacją** (`R2-WP33`). Do R2f
benchmark bez wpisu wypisywał się jako `NOWY` i bramka zwracała zero — czyli dla
**trzynastu z czterdziestu jeden** pozycji nie mierzyła niczego i robiła to cicho.
To jest ta sama klasa co filtry bramki G11: bramka raportuje zielono, nie sprawdzając
tego, co myśli, że sprawdza. Benchmark nowy w tym commicie jest dopuszczalny wyłącznie
**razem z dopisaniem go do linii bazowej w tym samym commicie** (`--update`).

Kiedy odnawia się linię bazową — `D-N21` (R2f), wykonanie `D-R8` z R1: **przy
zamknięciu każdej fazy**, jednym commitem bez innych zmian, z zapisem sprzętu
odniesienia. Wariant z `D-8` („przy świadomej zmianie wydajności") jest czystszy
teoretycznie i przegrał empirycznie: przez sześć faz nie zrobił tego nikt.

    python scripts/bench_guard.py benches/baseline.json
    python scripts/bench_guard.py benches/baseline.json --update
    python scripts/bench_guard.py --self-test
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


def porownaj(biezace: dict[str, float], bazowe: dict[str, float]) -> tuple[int, list[str]]:
    """Kod wyjścia i wiersze raportu.

    Osobno od `main`, bo bramka, której nie da się zawołać bez `target/criterion`,
    nie ma jak mieć testu samej siebie — a bramka bez testu nie wie, czy jeszcze
    świeci na czerwono.
    """
    kod = 0
    wiersze = []
    for nazwa, wartosc in sorted(biezace.items()):
        odniesienie = bazowe.get(nazwa)
        if odniesienie is None:
            kod = 1
            wiersze.append(f"BRAK W BAZIE {nazwa}: {wartosc / 1000:.1f} µs — dopisz go w tym commicie (--update)")
            continue
        zmiana = (wartosc - odniesienie) / odniesienie
        etykieta = "OK    "
        if zmiana > BLAD:
            etykieta, kod = "BŁĄD  ", 1
        elif zmiana > OSTRZEZENIE:
            etykieta = "UWAGA "
        wiersze.append(f"{etykieta} {nazwa}: {zmiana * 100:+.1f} % ({wartosc / 1000:.1f} µs)")

    for nazwa in sorted(set(bazowe) - set(biezace)):
        wiersze.append(f"BRAK   {nazwa}: benchmark zniknął z zestawu")
    return kod, wiersze


def self_test() -> int:
    """Bramka, która nigdy nie świeci na czerwono, nie jest bramką.

    Cztery przypadki, bo cztery rzeczy ta bramka ma rozróżniać — a jeden z nich
    (brak wpisu) przez sześć faz rozróżniała **źle**.
    """
    baza = {"a": 1000.0, "b": 1000.0, "c": 1000.0, "zniknal": 1000.0}
    biezace = {"a": 1000.0, "b": 1150.0, "c": 1400.0, "nowy": 500.0}
    kod, wiersze = porownaj(biezace, baza)
    raport = "\n".join(wiersze)

    przypadki = [
        ("zgodny przechodzi", "OK     a" in raport),
        ("+15 % to ostrzeżenie", "UWAGA  b" in raport),
        ("+40 % to błąd", "BŁĄD   c" in raport),
        ("brak wpisu w bazie to błąd", "BRAK W BAZIE nowy" in raport),
        ("benchmark, który zniknął, jest widoczny", "BRAK   zniknal" in raport),
        ("kod wyjścia 1", kod == 1),
    ]
    ok = True
    for opis, wynik in przypadki:
        ok &= wynik
        print(f"{'OK    ' if wynik else 'BŁĄD  '} {opis}")

    # Sam brak wpisu, bez żadnej regresji, **też** musi dać kod 1 — to jest cała
    # treść `R2-WP33` i najłatwiejsza rzecz do zepsucia przy następnej zmianie.
    kod2, _ = porownaj({"a": 1000.0, "nowy": 1.0}, {"a": 1000.0})
    ok &= kod2 == 1
    print(f"{'OK    ' if kod2 == 1 else 'BŁĄD  '} sam brak wpisu wywraca bramkę")

    # I w drugą stronę: komplet wpisów bez regresji ma dać zero.
    kod3, _ = porownaj({"a": 1000.0}, {"a": 1000.0})
    ok &= kod3 == 0
    print(f"{'OK    ' if kod3 == 0 else 'BŁĄD  '} komplet bez regresji przechodzi")
    print("bench_guard --self-test:", "ok" if ok else "BŁĄD")
    return 0 if ok else 1


def main() -> int:
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("baseline", type=pathlib.Path, nargs="?")
    parser.add_argument("--criterion-dir", type=pathlib.Path, default=pathlib.Path("target/criterion"))
    parser.add_argument("--update", action="store_true", help="nadpisz linię bazową bieżącymi wynikami")
    parser.add_argument("--self-test", action="store_true", help="sprawdź sam wykrywacz")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if args.baseline is None:
        parser.error("podaj plik linii bazowej albo --self-test")

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

    kod, wiersze = porownaj(biezace, bazowe)
    for w in wiersze:
        print(w)
    brakujace = sum(1 for w in wiersze if w.startswith("BRAK W BAZIE"))
    print(f"\nbench_guard: pozycji {len(biezace)}, bez wpisu w linii bazowej {brakujace}.")
    return kod


if __name__ == "__main__":
    sys.exit(main())
