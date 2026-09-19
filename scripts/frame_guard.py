"""Bramka regresji budżetu klatki (M11e/WP10, §7.2 dokumentu fazy M11).

Czyta raporty scen odniesienia z `bench/frames/*.json`, porównuje je z linią bazową
i zwraca kod wyjścia: 0 przy zgodności, 1 przy regresji albo przekroczeniu progu.

**Dwie metryki, dwie role — korekta §7.2 (`H-11`), poparta pomiarem.** Plan mówił
„regresja p95 > 8 % zatrzymuje build", ale p95 czasu GPU **sam z siebie waha się o 10 %**
między czterema przebiegami tej samej sceny na tej samej maszynie (4,04–4,27 ms dla p50,
5,88–6,49 ms dla p95, `bench_district`). Bramka z progiem 8 % na p95 zapalałaby się
na szumie sterownika i stanu zegarów karty, czyli na niczym. Rozdzielenie:

* **próg bezwzględny** („czy scena mieści się w celu PRD §20.2") zostaje na **p95** —
  to on jest kryterium fazy i nie zmienia się;
* **regresja** („czy coś zwolniło") idzie po **p50** z progiem 10 %, bo mediana ma
  rozrzut 5,7 %, czyli mieści się w progu z zapasem.

Osobny skrypt od `bench_guard.py`, mimo bliźniaczego kształtu, i to jest decyzja:
tamten czyta katalog criteriona i pilnuje **mikrobenchmarków procesora** na wspólnych
runnerach CI, gdzie rozrzut sięga kilkunastu procent i próg musi być luźny. Ten czyta
raporty klatkowe z **maszyny referencyjnej**, gdzie rozrzut jest rzędu procenta,
a próg 8 % pochodzi wprost z §7.2. Wspólny skrypt musiałby nieść dwa zestawy progów
i dwa źródła danych po to, żeby oszczędzić czterdzieści linii.

Próg **bezwzględny** (czy scena mieści się w celu z PRD §20.2) sprawdza sam klient
i zgłasza go kodem wyjścia — tutaj chodzi wyłącznie o to, czy coś zwolniło względem
poprzedniego stanu tego samego repozytorium.

Aktualizacja linii bazowej (po świadomej zmianie wydajności):
    python scripts/frame_guard.py bench/frames --update
"""

import argparse
import json
import pathlib
import sys

REGRESJA = 0.10
BAZOWA = "baseline.json"

# Kanon scen z `tools/magnat/src/scenes.rs`. Powtórzony tutaj świadomie: klient i bramka
# to dwa procesy, a scena, która się nie uruchomiła, ma zapalić bramkę na czerwono —
# nie zniknąć z porównania. Rozjazd tej listy z katalogiem scen łapie test
# `katalog_pokrywa_sceny_odniesienia`, który pilnuje tych samych siedmiu nazw.
KANON = (
    "bench_street",
    "bench_district",
    "bench_city",
    "bench_night_rain",
    "bench_blackout",
    "bench_interiors",
    "bench_winter",
)


def zbierz(katalog: pathlib.Path) -> dict[str, dict]:
    """Raport każdej sceny z katalogu, bez pliku linii bazowej."""
    wyniki: dict[str, dict] = {}
    for plik in sorted(katalog.glob("*.json")):
        if plik.name == BAZOWA:
            continue
        with plik.open(encoding="utf-8") as f:
            dane = json.load(f)
        nazwa = dane.get("scene") or plik.stem
        wyniki[nazwa] = dane
    return wyniki


def p95(raport: dict) -> float:
    return float(raport.get("gpu_ms", {}).get("p95", 0.0))


def p50(raport: dict) -> float:
    return float(raport.get("gpu_ms", {}).get("p50", 0.0))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("katalog", type=pathlib.Path, default=pathlib.Path("bench/frames"), nargs="?")
    parser.add_argument("--update", action="store_true", help="nadpisz linię bazową bieżącymi wynikami")
    args = parser.parse_args()

    biezace = zbierz(args.katalog)
    if not biezace:
        print(f"brak raportów scen w {args.katalog} — czy `magnat --bench-scene …` się wykonał?")
        return 1

    baseline = args.katalog / BAZOWA
    if args.update:
        brakujace = [n for n in KANON if n not in biezace]
        if brakujace:
            print("linia bazowa z niepełnego przebiegu byłaby gorsza od jej braku; brak: "
                  + ", ".join(brakujace))
            return 1
        skrot = {
            nazwa: {
                "gpu_p50_ms": p50(r),
                "gpu_p95_ms": p95(r),
                "threshold_ms": r.get("threshold_ms"),
                "machine": r.get("machine"),
            }
            for nazwa, r in sorted(biezace.items())
        }
        baseline.parent.mkdir(parents=True, exist_ok=True)
        with baseline.open("w", encoding="utf-8") as f:
            json.dump(skrot, f, indent=2, ensure_ascii=False)
            f.write("\n")
        print(f"zapisano {len(skrot)} scen do {baseline}")
        return 0

    if not baseline.exists():
        print(f"brak linii bazowej {baseline}; utwórz ją przez --update")
        return 1

    with baseline.open(encoding="utf-8") as f:
        bazowe = json.load(f)

    kod = 0
    for nazwa, raport in sorted(biezace.items()):
        teraz = p50(raport)
        prog = raport.get("threshold_ms")
        # Werdykt liczymy **sami**, a nie ufamy polu `verdict` z pliku: plik może być
        # z poprzedniego przebiegu albo z innej maszyny, a wtedy niesie cudzy werdykt.
        if not prog or p95(raport) <= 0.0 or p95(raport) > float(prog):
            print(f"PRÓG   {nazwa}: p95 {p95(raport):.2f} ms wobec progu {prog} ms")
            kod = 1
        odniesienie = (bazowe.get(nazwa) or {}).get("gpu_p50_ms")
        if odniesienie is None or odniesienie <= 0.0:
            print(f"NOWA   {nazwa}: p50 {teraz:.2f} ms (brak w linii bazowej)")
            continue
        zmiana = (teraz - odniesienie) / odniesienie
        etykieta = "OK    "
        if zmiana > REGRESJA:
            etykieta, kod = "BŁĄD  ", 1
        print(
            f"{etykieta} {nazwa}: {zmiana * 100:+.1f} % (p50 {teraz:.2f} ms, "
            f"p95 {p95(raport):.2f} / próg {prog} ms)"
        )

    # Kryterium §7.3: blackout **nie może kosztować więcej** niż ta sama scena
    # z pełnym oświetleniem — i to porównanie idzie po **passie klastrów**, a nie po
    # całej klatce. Korekta `H-12`, poparta pomiarem: obie sceny to dwa osobne procesy,
    # a między nimi karta stoi na innym zegarze, więc `depth_prepass`, `water` i `post`
    # — których blackout nie dotyka z konstrukcji — potrafią różnić się o 40–70 %.
    # Porównanie całej klatki mierzyłoby wtedy stan sprzętu. Passem, który zależy od
    # listy świateł, jest `clusters` i tylko on; reszta kryterium to sama lista.
    noc, ciemno = biezace.get("bench_night_rain"), biezace.get("bench_blackout")
    if noc and ciemno:
        sw_a = (noc.get("stats") or {}).get("lights", 0)
        sw_b = (ciemno.get("stats") or {}).get("lights", 0)
        kl_a = ((noc.get("pass_ms") or {}).get("clusters") or {}).get("p50", 0.0)
        kl_b = ((ciemno.get("pass_ms") or {}).get("clusters") or {}).get("p50", 0.0)
        if sw_b >= sw_a:
            # Scena, w której blackout nie zgasił ani jednego światła, porównuje
            # tę samą klatkę ze sobą i jest zielona z powodu, który nią nie jest.
            print(f"BŁĄD   bench_blackout: {sw_b} świateł wobec {sw_a} — dzielnice nie zgasły")
            kod = 1
        elif kl_a > 0 and kl_b > kl_a:
            print(f"BŁĄD   bench_blackout: pass klastrów {kl_b:.3f} ms > {kl_a:.3f} ms mimo krótszej listy")
            kod = 1
        else:
            print(
                f"OK     bench_blackout: {sw_b} świateł wobec {sw_a}, "
                f"pass klastrów {kl_b:.3f} wobec {kl_a:.3f} ms"
            )

    # Kryterium §7.3 `seasons_do_not_remesh`: pora roku wchodzi uniformem ramki,
    # a nie geometrią, więc w oknie pomiaru sceny zimowej nie ma prawa powstać
    # ani jeden nowy mesh chunka. Licznik w raporcie jest **przyrostem** w oknie.
    zima = biezace.get("bench_winter")
    if zima:
        n = zima.get("stats", {}).get("chunk_remesh_count", 0)
        if n:
            print(f"BŁĄD   bench_winter: {n} remeshingów chunków w oknie pomiaru (ma być 0)")
            kod = 1
        else:
            print("OK     bench_winter: zero remeshingów — sezon nie dotyka geometrii")

    # Braki liczymy wobec **kanonu**, a nie wobec linii bazowej: jedno i drugie pochodzi
    # z tych samych plików, więc scena, która nigdy się nie uruchomiła, nie znalazłaby się
    # po żadnej stronie różnicy. Kryterium, którego nikt nie mierzy, jest zielone
    # z tego samego powodu co kryterium spełnione.
    for nazwa in sorted(set(KANON) | set(bazowe)):
        if nazwa not in biezace:
            print(f"BRAK   {nazwa}: brak raportu — scena się nie uruchomiła albo padła")
            kod = 1

    return kod


if __name__ == "__main__":
    sys.exit(main())
