"""Bramka kontroli strukturalnej (R1 §6, decyzje D-R1 i D-R2).

Mierzy cztery metryki na **kodzie produkcyjnym** i porównuje z progami, które są
kwartylami tego repozytorium zaokrąglonymi w górę (R1 §6, `D-R1`):

    plik                  800 / 1200 linii     (p90 = 910, p95 = 1181)
    blok `impl`           300 / 500            (p95 = 181, p99 = 421)
    funkcja               150 / 250            (p95 = 77,  p99 = 158)
    `mod.rs` własny kod   300 / 600            (oba dzisiejsze przypadki to R-WP4 i R-WP9)

Przekroczony próg **nie jest błędem**. Jest pytaniem, na które reguła „przegląd
strukturalny po zamkniętym pakiecie" z `CLAUDE.md` każe odpowiedzieć na jeden
z trzech sposobów: podziel teraz, zaplanuj podział (rejestr długu w
`R1-refaktor-po-M5.md`), albo zostaw świadomie z komentarzem `ponytail:`.

    python scripts/struct_guard.py --changed   # pliki zmienione względem HEAD (hook)
    python scripts/struct_guard.py --all       # całe repo (CI)
    python scripts/struct_guard.py --self-test # test wykrywacza

Kod wyjścia: 0 przy czystym przebiegu i przy samych ostrzeżeniach, 1 przy
przekroczeniu progu błędu w trybie `--all`. W trybie `--changed` **zawsze 0** —
hook informuje, nie blokuje (R1 §6, „czego kontrola nie robi").

Wzorem jest `scripts/bench_guard.py`: Python, zero zależności, dwa progi.
"""

import argparse
import io
import json
import pathlib
import re
import subprocess
import sys
import tempfile

PROGI = {
    "plik": (800, 1200),
    "impl": (300, 500),
    "fn": (150, 250),
    "mod.rs": (300, 600),
}

# Jawne wyjątki z R1 §5. Nie są przeoczeniem — są decyzją z powodem.
# Zwolnienie dotyczy **wyłącznie** metryki „plik": długi blok `impl` albo długa
# funkcja w tych plikach nadal się zapala, bo kryterium akceptacji nr 5 nie ma
# dla nich wyjątku.
WYJATKI_PLIK = {
    # Jeden algorytm: `contract`, `customize` i `query` dzielą niezmienniki
    # struktury łuków skrótowych. Rozbicie po plikach rozerwie je bez zysku.
    "engine/nav/src/cch.rs": "jeden algorytm (CCH); R1 §5",
    # Słownik funkcji deterministycznych. Podzielony słownik to dwa słowniki,
    # a `K-6` wiąże plik ze złotym odciskiem, więc dotknięcie jest kosztowne.
    "engine/core/src/det_math.rs": "słownik det_math związany złotym odciskiem (K-6); R1 §5",
    # Spójny: typy grafu, builder, walidacja. Odstaje tylko `synthetic_*` (`D-R5`).
    "engine/nav/src/graph.rs": "spójny moduł grafu; R1 §5 (D-R5 dotyczy tylko `synthetic_*`)",
}

# Katalogi, których kontrola nie mierzy: długi plik testów nie jest długiem
# (R1 §6), a benchmarki są przyrządem pomiarowym, nie kodem gry.
KATALOGI_POMIJANE = ("tests/", "benches/", "target/")

POCZATEK_ITEMU = re.compile(
    r"""(?P<test>\#\[cfg\(test\)\])
      | (?P<impl>\bimpl\b)
      | (?P<fn>\bfn\s+\w+)""",
    re.VERBOSE,
)

DEKLARACJA = re.compile(r"^\s*(pub(\s*\([^)]*\))?\s+)?(mod|use)\b")


def wygas_literaly(tekst: str) -> str:
    """Zamienia treść komentarzy i literałów na spacje, zachowując numerację linii.

    Bez tego `{` w napisie albo w `//` przesuwa licznik zagnieżdżenia i cała
    reszta pomiaru jest zmyślona. Zachowujemy nowe linie, żeby numery się zgadzały.
    """
    wyj = []
    i, n = 0, len(tekst)
    while i < n:
        c = tekst[i]
        if c == "/" and i + 1 < n and tekst[i + 1] == "/":
            j = tekst.find("\n", i)
            j = n if j < 0 else j
            wyj.append(" " * (j - i))
            i = j
        elif c == "/" and i + 1 < n and tekst[i + 1] == "*":
            glebokosc, j = 1, i + 2
            while j < n and glebokosc:
                if tekst.startswith("/*", j):
                    glebokosc, j = glebokosc + 1, j + 2
                elif tekst.startswith("*/", j):
                    glebokosc, j = glebokosc - 1, j + 2
                else:
                    j += 1
            wyj.append("".join(ch if ch == "\n" else " " for ch in tekst[i:j]))
            i = j
        elif c == "r" and (m := re.match(r'r(#*)"', tekst[i:])):
            koniec = '"' + m.group(1)
            j = tekst.find(koniec, i + m.end())
            j = n if j < 0 else j + len(koniec)
            wyj.append("".join(ch if ch == "\n" else " " for ch in tekst[i:j]))
            i = j
        elif c == '"':
            j = i + 1
            while j < n and tekst[j] != '"':
                j += 2 if tekst[j] == "\\" else 1
            j = min(j + 1, n)
            wyj.append("".join(ch if ch == "\n" else " " for ch in tekst[i:j]))
            i = j
        elif c == "'" and re.match(r"'(\\.|[^\\'])'", tekst[i:]):
            wyj.append("   " if tekst[i + 1] != "\\" else "    ")
            i += 3 if tekst[i + 1] != "\\" else 4
        else:
            wyj.append(c)
            i += 1
    return "".join(wyj)


def bloki(tekst: str) -> list[tuple[str, int, int]]:
    """Zakresy `(rodzaj, pierwsza_linia, ostatnia_linia)` — numeracja od 1.

    Rodzaj to `test`, `impl` albo `fn`. Sygnatura może się ciągnąć przez kilka
    linii, więc rodzaj czeka w `oczekuje` na najbliższy `{`; średnik przed nim
    kasuje oczekiwanie (deklaracja w traicie, `#[cfg(test)] use ...`).
    """
    czysty = wygas_literaly(tekst)
    znalezione: list[tuple[str, int, int]] = []
    stos: list[tuple[tuple[str, int] | None, int]] = []
    oczekuje: tuple[str, int] | None = None
    linia = 1
    for wiersz in czysty.split("\n"):
        punkty = {m.start(): m.lastgroup for m in POCZATEK_ITEMU.finditer(wiersz)}
        for poz, ch in enumerate(wiersz):
            if (rodzaj := punkty.get(poz)) and oczekuje is None:
                oczekuje = (rodzaj, linia)
            if ch == "{":
                stos.append((oczekuje, linia))
                oczekuje = None
            elif ch == "}":
                if stos:
                    zapowiedz, _ = stos.pop()
                    if zapowiedz:
                        znalezione.append((zapowiedz[0], zapowiedz[1], linia))
            elif ch == ";":
                oczekuje = None
        linia += 1
    return znalezione


def zmierz(sciezka: pathlib.Path, tekst: str) -> list[tuple[str, str, int]]:
    """Lista `(metryka, opis, wartość)` dla jednego pliku."""
    wszystkie = bloki(tekst)
    testy = [(a, b) for rodzaj, a, b in wszystkie if rodzaj == "test"]

    def w_tescie(nr: int) -> bool:
        return any(a <= nr <= b for a, b in testy)

    wiersze = tekst.split("\n")
    if wiersze and wiersze[-1] == "":
        wiersze.pop()
    produkcyjne = [nr for nr in range(1, len(wiersze) + 1) if not w_tescie(nr)]

    wynik: list[tuple[str, str, int]] = [("plik", "", len(produkcyjne))]
    if sciezka.name == "mod.rs":
        wlasne = sum(1 for nr in produkcyjne if not DEKLARACJA.match(wiersze[nr - 1]))
        wynik.append(("mod.rs", "kod poza deklaracjami", wlasne))
    for rodzaj, a, b in wszystkie:
        if rodzaj in ("impl", "fn") and not w_tescie(a):
            wynik.append((rodzaj, f"linie {a}–{b}", b - a + 1))
    return wynik


def pliki_produkcyjne(sciezki) -> list[pathlib.Path]:
    wybrane = []
    for p in sciezki:
        s = p.as_posix()
        if p.suffix != ".rs" or any(k in s for k in KATALOGI_POMIJANE):
            continue
        wybrane.append(p)
    return sorted(wybrane)


def zmienione_wzgledem_head(korzen: pathlib.Path) -> list[pathlib.Path]:
    polecenia = [
        ["git", "diff", "--name-only", "--diff-filter=d", "HEAD"],
        ["git", "ls-files", "--others", "--exclude-standard"],
    ]
    nazwy: set[str] = set()
    for polecenie in polecenia:
        wynik = subprocess.run(polecenie, cwd=korzen, capture_output=True, text=True)
        if wynik.returncode:
            print(f"struct_guard: `{' '.join(polecenie)}` nie powiodło się — pomijam")
            continue
        nazwy.update(w for w in wynik.stdout.split("\n") if w.strip())
    return [korzen / n for n in nazwy if (korzen / n).is_file()]


def raport(korzen: pathlib.Path, pliki: list[pathlib.Path], jako_json: bool) -> int:
    pozycje = []
    for plik in pliki:
        wzgledna = plik.relative_to(korzen).as_posix()
        try:
            tekst = plik.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        for metryka, opis, wartosc in zmierz(plik, tekst):
            ostrzezenie, blad = PROGI[metryka]
            if wartosc <= ostrzezenie:
                continue
            zwolniony = metryka == "plik" and wzgledna in WYJATKI_PLIK
            pozycje.append(
                {
                    "plik": wzgledna,
                    "metryka": metryka,
                    "opis": opis,
                    "linie": wartosc,
                    "prog": "wyjątek" if zwolniony else ("błąd" if wartosc > blad else "ostrzeżenie"),
                    "powod": WYJATKI_PLIK.get(wzgledna) if zwolniony else None,
                }
            )

    if jako_json:
        print(json.dumps(pozycje, indent=2, ensure_ascii=False))
    else:
        etykiety = {"błąd": "BŁĄD  ", "ostrzeżenie": "UWAGA ", "wyjątek": "WYJĄTEK"}
        for p in sorted(pozycje, key=lambda p: (-p["linie"], p["plik"])):
            ogon = f" ({p['opis']})" if p["opis"] else ""
            ogon += f" — {p['powod']}" if p["powod"] else ""
            print(f"{etykiety[p['prog']]} {p['plik']}: {p['metryka']} {p['linie']} linii{ogon}")
        bledy = sum(1 for p in pozycje if p["prog"] == "błąd")
        uwagi = sum(1 for p in pozycje if p["prog"] == "ostrzeżenie")
        print(f"\nstruct_guard: sprawdzonych plików {len(pliki)}, przekroczeń progu błędu {bledy}, ostrzeżeń {uwagi}")
        if pozycje:
            print(
                "Reguła przeglądu strukturalnego po zamkniętym pakiecie (CLAUDE.md): "
                "podziel teraz, zaplanuj podział w rejestrze długu R1, "
                "albo zostaw świadomie z komentarzem `ponytail:`."
            )
    return 1 if any(p["prog"] == "błąd" for p in pozycje) else 0


ATRAPA = (
    "impl Atrapa {\n"
    + "    fn dluga() {\n"
    + "        let _ = 1;\n" * 300
    + "    }\n"
    + "    fn druga() {\n"
    + "        let _ = 2;\n" * 300
    + "    }\n"
    + "}\n"
    + "fn poza() {\n"
    + "    let _ = 3;\n" * 700
    + "}\n"
    + "#[cfg(test)]\n"
    + "mod tests {\n"
    + "    fn t() {\n"
    + "        let _ = 4;\n" * 3000
    + "    }\n"
    + "}\n"
)


def test_wykrywacza() -> int:
    """Bramka, która nigdy nie świeci na czerwono, nie jest bramką (M5a, `single_entry_point`).

    Atrapa przekracza każdy z czterech progów; każda z czterech metryk musi się
    zapalić. Dodatkowo sprawdzamy to, co najłatwiej zepsuć przy zmianie parsowania:
    3000 linii w bloku `#[cfg(test)]` **nie** wchodzi do żadnej metryki.
    """
    with tempfile.TemporaryDirectory() as katalog:
        plik = pathlib.Path(katalog) / "mod.rs"
        plik.write_text(ATRAPA, encoding="utf-8")
        zmierzone = zmierz(plik, ATRAPA)

    najwieksze: dict[str, int] = {}
    for metryka, _, wartosc in zmierzone:
        najwieksze[metryka] = max(najwieksze.get(metryka, 0), wartosc)

    kod = 0
    for metryka, (_, blad) in PROGI.items():
        wartosc = najwieksze.get(metryka, 0)
        ok = wartosc > blad
        kod |= 0 if ok else 1
        print(f"{'OK    ' if ok else 'BŁĄD  '} {metryka}: {wartosc} linii (próg błędu {blad})")

    # Blok testowy ma 3005 linii — gdyby wchodził do metryki „plik", byłoby ich
    # ~4300 zamiast ~1310. Ten warunek łapie regres parsowania `#[cfg(test)]`.
    bez_testow = najwieksze.get("plik", 0) < 2000
    kod |= 0 if bez_testow else 1
    print(f"{'OK    ' if bez_testow else 'BLAD  '} blok #[cfg(test)] poza metryka pliku")
    return kod


def hook() -> int:
    """Tryb `PreToolUse`: raport trafia do kontekstu agenta tuż przed `git commit`.

    Moment jest wybrany dosłownie tak, jak mówi reguła: **przed commitem, nie po**.
    Hook typu `Stop` z R1 §6 odpalałby się po zakończeniu tury, czyli po commicie —
    korekta zapisana w tabeli „Zmiany wpisane po R1".

    Nie rozstrzyga uprawnień (brak `permissionDecision`), więc zwykła zgoda na
    `git commit` przebiega bez zmian. Wyjście jest zawsze 0: hook informuje, nie blokuje.
    """
    try:
        zdarzenie = json.loads(sys.stdin.read() or "{}")
    except json.JSONDecodeError:
        return 0
    polecenie = str(zdarzenie.get("tool_input", {}).get("command", ""))
    if "git commit" not in polecenie:
        return 0

    korzen = pathlib.Path(__file__).resolve().parent.parent
    pliki = pliki_produkcyjne(zmienione_wzgledem_head(korzen))
    if not pliki:
        return 0

    bufor = io.StringIO()
    poprzedni, sys.stdout = sys.stdout, bufor
    try:
        raport(korzen, pliki, False)
    finally:
        sys.stdout = poprzedni
    tresc = bufor.getvalue().strip()
    if "BŁĄD" not in tresc and "UWAGA" not in tresc:
        return 0

    print(
        json.dumps(
            {
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "additionalContext": "Kontrola strukturalna — reguła przeglądu "
                    "strukturalnego po zamkniętym pakiecie (CLAUDE.md):\n" + tresc,
                }
            },
            ensure_ascii=False,
        )
    )
    return 0


def main() -> int:
    # Wynik czyta agent i log CI, a konsola Windows domyślnie nie jest UTF-8 —
    # bez tego polskie znaki w raporcie są nieczytelne dokładnie tam, gdzie mają działać.
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    parser = argparse.ArgumentParser(description=__doc__)
    grupa = parser.add_mutually_exclusive_group()
    grupa.add_argument("--changed", action="store_true", help="tylko pliki zmienione względem HEAD")
    grupa.add_argument("--all", action="store_true", help="całe repozytorium (tryb CI)")
    grupa.add_argument("--self-test", action="store_true", help="test wykrywacza na pliku-atrapie")
    grupa.add_argument("--hook", action="store_true", help="tryb hooka `PreToolUse` (JSON na wejściu i wyjściu)")
    parser.add_argument("--json", action="store_true", help="wynik maszynowy")
    args = parser.parse_args()

    if args.self_test:
        return test_wykrywacza()
    if args.hook:
        return hook()

    korzen = pathlib.Path(__file__).resolve().parent.parent
    if args.changed:
        pliki = pliki_produkcyjne(zmienione_wzgledem_head(korzen))
        if not pliki:
            return 0
        return min(raport(korzen, pliki, args.json), 0)  # hook informuje, nie blokuje
    return raport(korzen, pliki_produkcyjne(korzen.glob("**/*.rs")), args.json)


if __name__ == "__main__":
    sys.exit(main())
