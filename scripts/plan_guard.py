#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Bramka R2-WP23 i R2-WP26: opis nie rozjeżdża się z rzeczą, którą opisuje.

`README.md` jest jedynym plikiem, który ktoś z zewnątrz przeczyta pierwszy,
a jego nagłówek stanu nie był ruszany od fazy, w której powstał. Rozjazd nie
jest widoczny dla żadnego testu, bo opis nie ma z czym się nie zgadzać —
dopóki nie powie mu tego ta bramka.

Cztery reguły, wszystkie mechaniczne:

  1. **stan w `README.md` == ostatni odhaczony wiersz `00-postep.md`** —
     nagłówek `Stan:` musi nieść identyfikator ostatniej pozycji `[x]`
     (np. `R2d`, `M11e`). Kolejność wierszy w dokumencie postępu jest
     kolejnością wykonania, więc „ostatni" znaczy „najniżej w pliku";
  2. **jeden nagłówek tabel korekt** — `K-18` pkt (b) każe zbierać poprawki
     w tabeli „Zmiany wpisane po MX". Wariantów tego nagłówka było w repo
     **siedem** w 22 nagłówkach, a przegląd po jednym z nich gubi wpisy
     z pozostałych sześciu. Wpis, którego nie da się znaleźć, jest wpisem,
     którego nie ma.

  3. **obietnica z tabeli korekt ma pokrycie** (`R2-WP26`) — wiersz, którego
     druga kolumna jest nazwą dokumentu, deklaruje adresata; dokument musi
     istnieć i nieść kod korekty albo identyfikator z jej treści. Sprawdzenie
     dwudziestu takich wierszy przed napisaniem tej reguły dało **osiem bez
     pokrycia**, a dwa z nich były prawdziwą stratą (`M8` `CJ-9`).

  4. **workflow CI daje się wczytać** (`R2f`) — `.github/workflows/*.yml` musi
     przejść przez `yaml.safe_load`, a bez tego modułu przez wzorzec na klasę,
     która wywróciła plik w praktyce: niecytowaną wartość z dwukropkiem i spacją.
     Do R2f `ci.yml` nie wczytywał się **od M10a**, więc nie biegł ani jeden job.
     Ta reguła jest przez to jedyną, której miejscem jest przebieg **lokalny**:
     bramka w CI nie obroni pliku, od którego CI zależy.

  5. **workflow ma domyślną powłokę `bash`** (`N1.14`) — bez niej krok na Windows
     biegnie w pwsh, który liczy tylko ostatni kod wyjścia wieloliniowego `run:`,
     a `bash` bez jawnego `shell:` nie ma `pipefail`, więc `cargo test | tee` połyka błąd.

Czego ta bramka nie sprawdza i dlaczego: **czy rzecz naprawdę została opisana**
pod wskazanym adresem. To wymagałoby czytania ze zrozumieniem i skończyłoby się
wyłączeniem bramki po trzecim fałszywym alarmie.

Użycie:
    python scripts/plan_guard.py              # bramka
    python scripts/plan_guard.py --self-test
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

README = Path("README.md")
POSTEP = Path("docs/implementation-plan/00-postep.md")
PLAN = Path("docs/implementation-plan")

# Wiersz odhaczonej pozycji w dokumencie postępu: `- [x] **M11e** Budżet klatki — ...`.
# Identyfikator jest pogrubiony i jest pierwszym pogrubieniem w wierszu.
ODHACZONY = re.compile(r"^-\s*\[x\]\s*\*\*([A-Za-z0-9]+)\*\*", re.MULTILINE)

# Nagłówek stanu w README: jedna linia zaczynająca się od `Stan:`.
STAN = re.compile(r"^Stan:\s*(.+)$")

# Nagłówek tabeli korekt niekanoniczny. Szukamy **nagłówka**, nie zdania — dokument
# R2 opisuje warianty w prozie i to nie jest tabela. Numerowana sekcja (`## 4a. …`)
# też nie: to część własnej numeracji dokumentu, z żywymi odsyłaczami `M0 §4a`,
# a nie tabela doklejona na końcu wg `K-18` pkt (b).
NAGLOWEK_OBCY = re.compile(r"^#{1,6}\s+Korekty.*$", re.MULTILINE)

# Kanoniczny nagłówek tabeli korekt (`K-18` pkt b).
NAGLOWEK_KANONICZNY = "Zmiany wpisane po"


# ── reguła 3: obietnica z tabeli korekt ma pokrycie pod wskazanym adresem ─────────
#
# Druga połowa `R2-WP26`. Rejestr długu ma przynajmniej regułę, którą da się
# egzekwować; **tabele korekt nie miały żadnej** — a niosą ten sam rodzaj
# zobowiązania: wiersz mówi „to idzie do `M10-glebia.md`", i nikt nigdy nie
# sprawdza, czy dojechało. Sprawdzenie wykonane ręcznie przed napisaniem tej
# reguły dało osiem wierszy bez pokrycia pod wskazanym adresem, a dwa z nich
# były prawdziwą stratą.
#
# Sprawdzenie jest **słabe i takie ma być**: czy plik docelowy istnieje i czy
# niesie kod korekty **albo** charakterystyczny identyfikator z jej treści.
# Mocniejsze (czy rzecz naprawdę została opisana) wymagałoby czytania ze
# zrozumieniem i skończyłoby się wyłączeniem bramki po trzecim fałszywym
# alarmie — dokładnie tak, jak `AD-6` opisuje los bramki, która przeszkadza.

# Kod korekty w pierwszej kolumnie: `CJ-9`, `D-4`, `E20`, z gwiazdką albo bez.
KOD_KOREKTY = re.compile(r"^`?([A-Z]{1,3}-?\d+)`?")
# Adresat: **cała** druga kolumna jest nazwą dokumentu. Nie „gdzieś w prozie" —
# tabele o kształcie `| kod | treść | dlaczego |` wymieniają cudze pliki w treści
# i nie deklarują tym adresata.
KOLUMNA_ADRESATA = re.compile(r"^`([A-Za-z0-9_.-]+\.md)`(\s*[(§].*)?$")
# Identyfikator z treści: cokolwiek w grawisach, co nie jest nazwą dokumentu.
IDENTYFIKATOR = re.compile(r"`([^`]{3,60})`")


def dokument_celu(nazwa: str) -> Path | None:
    """Dokument planu, dokument `docs/` albo plik w korzeniu — w tej kolejności."""
    for kandydat in (PLAN / nazwa, Path("docs") / nazwa, Path(nazwa)):
        if kandydat.exists():
            return kandydat
    return None


def obietnice(tekst: str) -> list[tuple[str, str, list[str]]]:
    """Wiersze tabel „Zmiany wpisane po", które deklarują adresata.

    Zwraca `(kod, dokument docelowy, identyfikatory z treści)`.
    """
    wyniki = []
    w_tabeli = False
    for linia in tekst.split("\n"):
        if linia.startswith("## "):
            w_tabeli = NAGLOWEK_KANONICZNY in linia
            continue
        if not w_tabeli or not linia.startswith("|"):
            continue
        kol = [c.strip() for c in linia.strip().strip("|").split("|")]
        if len(kol) < 3 or set(kol[0]) <= set("-: "):
            continue
        cel = KOLUMNA_ADRESATA.match(kol[1])
        if not cel:
            continue
        tresc = " ".join(kol[2:])
        idy = [
            t
            for t in IDENTYFIKATOR.findall(tresc)
            if len(t.strip()) >= 4 and not t.strip().endswith(".md") and re.search(r"[A-Za-z]", t)
        ]
        wyniki.append((kol[0], cel.group(1), idy))
    return wyniki


def bez_pokrycia() -> list[str]:
    """Obietnice, których pod wskazanym adresem nie widać."""
    bledy = []
    for plik in sorted(PLAN.glob("*.md")):
        for kod, nazwa, idy in obietnice(plik.read_text(encoding="utf-8")):
            cel = dokument_celu(nazwa)
            if cel is None:
                bledy.append(f"{plik.name}: {kod} wskazuje {nazwa} — takiego pliku nie ma")
                continue
            tresc = cel.read_text(encoding="utf-8")
            m = KOD_KOREKTY.match(kod)
            if m and m.group(1) in tresc:
                continue
            if any(i in tresc for i in idy):
                continue
            bledy.append(
                f"{plik.name}: {kod} obiecuje coś dokumentowi {nazwa}, "
                f"a nie ma tam ani kodu korekty, ani żadnego z {len(idy)} identyfikatorów jej treści"
            )
    return bledy


# ── reguła 4: workflow CI daje się wczytać (R2f, pozycja 85 wykazu `R2`) ──────────
#
# Najdroższa usterka, jaką znalazła ta podfaza, i najtańsza do naprawienia: jedna
# nazwa kroku niosła **dwukropek ze spacją** bez cudzysłowu, czyli separator klucza
# od wartości. GitHub Actions nie wczytywał przez to **całego** pliku, więc od M10a
# nie biegł ani jeden job — ani `cargo test`, ani `clippy`, ani żadna z bramek,
# które R2 przez sześć podfaz naprawiało.
#
# Bramka nie ma jak być w CI, bo CI jest dokładnie tym, co się zepsuło. Stoi więc
# w skrypcie, który chodzi lokalnie przed commitem i w jobie — i to jest jedyny
# sensowny układ: pierwsze zadziała zawsze, drugie dopiero, gdy plik znów się wczyta.

WORKFLOWS = Path(".github/workflows")
# Wartość skalarna z dwukropkiem i spacją, niecytowana. Wąsko i celowo: to jest
# ta jedna klasa, która wywróciła plik, a szerszy wykrywacz bez parsera YAML-a
# skończyłby się fałszywymi alarmami na blokach `run: |`.
KLUCZ_Z_DWUKROPKIEM = re.compile(r"^\s*-?\s*(name|if):\s+(?![\"'|>])(.*:\s.*)$")


def workflow_sie_wczytuje() -> list[str]:
    """Błędy wczytania plików `.github/workflows/*.yml`.

    Woła `yaml`, gdy jest — to jedyny sposób na pełną odpowiedź. Gdy go nie ma,
    zostaje sprawdzenie tej jednej klasy, która wywróciła plik w praktyce.
    """
    if not WORKFLOWS.is_dir():
        return []
    bledy = []
    try:
        import yaml  # noqa: PLC0415
    except ImportError:
        yaml = None
    for plik in sorted(WORKFLOWS.glob("*.yml")) + sorted(WORKFLOWS.glob("*.yaml")):
        tresc = plik.read_text(encoding="utf-8")
        if yaml is not None:
            try:
                yaml.safe_load(tresc)
            except yaml.YAMLError as e:
                bledy.append(f"{plik.as_posix()}: nie wczytuje się jako YAML — {e}")
            continue
        for nr, linia in enumerate(tresc.split("\n"), 1):
            if KLUCZ_Z_DWUKROPKIEM.match(linia):
                bledy.append(
                    f"{plik.as_posix()}:{nr}: wartość z dwukropkiem i spacją bez cudzysłowu "
                    f"— YAML czyta to jako drugi klucz i wywraca cały plik"
                )
    return bledy


# ── reguła 5: domyślna powłoka `bash` (N1.14) ─────────────────────────────────────
#
# GitHub Actions woła `run:` na Windows przez pwsh, a ten zwraca kod **ostatniej**
# komendy — cztery `cargo test` w jednym kroku M1 padały po cichu, jeśli padł któryś
# poza ostatnim. Na Linuksie bez jawnego `shell:` jest `bash -e` bez `pipefail`,
# więc `cargo test | tee raport` jest zielony zawsze. Jawne `shell: bash` daje
# `bash -eo pipefail` na obu systemach — wystarczy raz, w `defaults` całego pliku.
DOMYSLNA_POWLOKA = re.compile(r"^defaults:\s*\n\s+run:\s*\n\s+shell:\s*bash\s*$", re.MULTILINE)


def workflow_ma_bash() -> list[str]:
    if not WORKFLOWS.is_dir():
        return []
    return [
        f"{plik.as_posix()}: brak `defaults: run: shell: bash` — pwsh na Windows połyka "
        f"błędy wcześniejszych komend kroku, a `bash` bez `pipefail` błąd przed `| tee`"
        for plik in sorted(WORKFLOWS.glob("*.yml")) + sorted(WORKFLOWS.glob("*.yaml"))
        if not DOMYSLNA_POWLOKA.search(plik.read_text(encoding="utf-8"))
    ]


def ostatnia_odhaczona(tekst: str) -> str | None:
    """Identyfikator ostatniej pozycji `[x]` — najniższej w pliku."""
    trafienia = ODHACZONY.findall(tekst)
    return trafienia[-1] if trafienia else None


def stan_z_readme(tekst: str) -> str | None:
    for linia in tekst.splitlines():
        m = STAN.match(linia)
        if m:
            return m.group(1)
    return None


def bramka() -> int:
    bledy: list[str] = []

    # Reguła 1 — stan README wobec dokumentu postępu.
    if not README.exists() or not POSTEP.exists():
        print("plan_guard: brak README.md albo 00-postep.md", file=sys.stderr)
        return 2
    faza = ostatnia_odhaczona(POSTEP.read_text(encoding="utf-8"))
    stan = stan_z_readme(README.read_text(encoding="utf-8"))
    if faza is None:
        bledy.append("00-postep.md nie ma ani jednego wiersza `- [x] **X**`")
    elif stan is None:
        bledy.append("README.md nie ma linii zaczynającej się od `Stan:`")
    elif not re.search(rf"\b{re.escape(faza)}\b", stan):
        bledy.append(
            f"README.md deklaruje stan {stan.split('(')[0].strip()!r}, "
            f"a ostatnią odhaczoną pozycją 00-postep.md jest {faza}"
        )

    # Reguła 2 — jeden nagłówek tabel korekt w całym planie.
    for plik in sorted(PLAN.glob("*.md")):
        tekst = plik.read_text(encoding="utf-8")
        for obcy in NAGLOWEK_OBCY.findall(tekst):
            bledy.append(
                f"{plik.as_posix()}: nagłówek tabeli korekt {obcy.strip()!r} — "
                f"kanoniczny to '## {NAGLOWEK_KANONICZNY} <faza>' (`K-18` pkt b)"
            )

    # Reguła 4 — workflow CI daje się wczytać.
    bledy.extend(workflow_sie_wczytuje())

    # Reguła 5 — domyślna powłoka `bash`.
    bledy.extend(workflow_ma_bash())

    # Reguła 3 — obietnica z tabeli korekt ma pokrycie pod wskazanym adresem.
    obietnic = sum(len(obietnice(q.read_text(encoding="utf-8"))) for q in PLAN.glob("*.md"))
    bledy.extend(bez_pokrycia())

    for b in bledy:
        print(f"plan_guard: {b}")
    if bledy:
        print(f"\nplan_guard: {len(bledy)} rozjazd(ów) opisu z rzeczą.")
        return 1
    print(
        f"plan_guard: ok — README.md mówi {faza}, jeden nagłówek tabel korekt, "
        f"{obietnic} obietnic z adresatem i każda ma pokrycie."
    )
    return 0


def self_test() -> int:
    """Bramka, która nigdy nie świeci na czerwono, nie jest bramką."""
    ok = True

    przypadki_postep = [
        ("- [x] **M11e** Budżet klatki — `x.md`\n- [ ] **R2e** Dług\n", "M11e"),
        ("- [x] **M1** a\n- [x] **R2d** b\n", "R2d"),
        ("- [ ] **M1** a\n", None),
    ]
    for tekst, chciane in przypadki_postep:
        mam = ostatnia_odhaczona(tekst)
        if mam != chciane:
            ok = False
            print(f"self-test: ostatnia_odhaczona({tekst!r}) = {mam!r}, chciane {chciane!r}")

    przypadki_stan = [
        ("# T\n\nStan: **R2d zamknięte** (świat).\n", "**R2d zamknięte** (świat)."),
        ("# T\n\nbez stanu\n", None),
    ]
    for tekst, chciane in przypadki_stan:
        mam = stan_z_readme(tekst)
        if mam != chciane:
            ok = False
            print(f"self-test: stan_z_readme = {mam!r}, chciane {chciane!r}")

    # Rozpoznanie rozjazdu: stan mówi o innej fazie niż ostatni wiersz postępu.
    if re.search(r"\bR2d\b", "**M8 zamknięte, M9a zamknięte** (miasto)."):
        ok = False
        print("self-test: fałszywe dopasowanie fazy w nagłówku stanu")
    if not re.search(r"\bR2d\b", "**R2d zamknięte** (świat)."):
        ok = False
        print("self-test: nie rozpoznano zgodnego nagłówka stanu")

    # Obcy nagłówek wykrywany; wzmianka w prozie i numerowana sekcja — nie.
    if not NAGLOWEK_OBCY.findall("## Korekty projektu technicznego M5c\n"):
        ok = False
        print("self-test: nie wykryto obcego nagłówka tabeli korekt")
    if NAGLOWEK_OBCY.findall('tabela „Korekty wpisane w trakcie MX" (1)\n'):
        ok = False
        print("self-test: fałszywy alarm na wzmiance w prozie")
    if NAGLOWEK_OBCY.findall("## 4a. Korekty planu wprowadzone w trakcie implementacji\n"):
        ok = False
        print("self-test: fałszywy alarm na numerowanej sekcji")

    # Reguła 3: rozpoznanie adresata i pokrycia. Trzy przypadki, bo trzy różne
    # rzeczy da się tu zepsuć — i każdy z nich zdarzył się w tym repozytorium.
    tabela = (
        "## Zmiany wpisane po MX\n\n| # | Dokument | Zmiana |\n|---|---|---|\n"
        "| `AA-1` | `M12a-pamiec.md` | **Coś o `BudzetPamieci`** |\n"
        "| `AA-2` | `M12a-pamiec.md` (§5) | wariant z odesłaniem w kolumnie |\n"
        "| `AA-3` | Treść wymieniająca `M12a-pamiec.md` w prozie | nie deklaruje adresata |\n"
    )
    deklaracje = obietnice(tabela)
    if len(deklaracje) != 2:
        ok = False
        print(f"self-test: obietnic z adresatem {len(deklaracje)}, chciane 2")
    elif deklaracje[0][1] != "M12a-pamiec.md" or "BudzetPamieci" not in deklaracje[0][2]:
        ok = False
        print(f"self-test: zly rozbior obietnicy: {deklaracje[0]!r}")
    # Wiersz spoza tabeli „Zmiany wpisane po" nie jest obietnicą — inaczej bramka
    # zapalałaby się na tabeli indeksu faz w dokumencie 00.
    if obietnice("## Indeks\n\n| M0 | `M0-fundament-silnika.md` | crate'y |\n"):
        ok = False
        print("self-test: tabela spoza naglowka kanonicznego uznana za obietnice")

    # Reguła 4: workflow, którego GitHub nie wczyta. Wzorzec sprawdzamy wprost,
    # bo ścieżka z `yaml` zależy od tego, czy moduł jest zainstalowany — a bramka
    # ma świecić na czerwono w obu przypadkach.
    przypadki_yaml = [
        ('      - name: M10a — artefakt A: raport', True),
        ('      - name: "M10a — artefakt A: raport"', False),
        ("      - name: M10a — artefakt A", False),
        ("        run: cargo test --release -- --include-ignored", False),
    ]
    for linia, ma_paskac in przypadki_yaml:
        trafienie = KLUCZ_Z_DWUKROPKIEM.match(linia) is not None
        if trafienie != ma_paskac:
            ok = False
            print(f"self-test: workflow {linia!r} → {trafienie}, chciane {ma_paskac}")

    # Reguła 5: `defaults` na poziomie pliku, nie `shell:` w jednym kroku.
    przypadki_bash = [
        ("on: push\ndefaults:\n  run:\n    shell: bash\njobs: {}\n", True),
        ("on: push\njobs:\n  a:\n    steps:\n      - run: x\n        shell: bash\n", False),
        ("on: push\ndefaults:\n  run:\n    shell: pwsh\n", False),
    ]
    for tekst, chciane in przypadki_bash:
        if (DOMYSLNA_POWLOKA.search(tekst) is not None) != chciane:
            ok = False
            print(f"self-test: domyślna powłoka w {tekst!r} → {not chciane}, chciane {chciane}")

    print("plan_guard --self-test:", "ok" if ok else "BŁĄD")
    return 0 if ok else 1


def main() -> int:
    # Raport czyta log CI i konsola Windows, która domyślnie nie jest UTF-8 —
    # bez tego gwiazdka korekty wywraca bramkę zamiast ją wypisać.
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--self-test", action="store_true", help="sprawdź sam wykrywacz")
    a = ap.parse_args()
    return self_test() if a.self_test else bramka()


if __name__ == "__main__":
    sys.exit(main())
