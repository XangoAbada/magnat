#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Bramka R2-WP27: granicą języka jest crate, nie plik.

`00` §6 i `CLAUDE.md` mówią od R2e (`D-N19`): **po angielsku wszystko, co
przekracza granicę crate'u**; nazwa prywatna wewnątrz modułu i komunikat
deweloperski idą w języku komentarzy tego modułu, czyli po polsku. Reguła
w dokumencie nie broni się sama — dlatego przez siedem faz żyły w repozytorium
dwie konwencje naraz, z których jedna nie była nigdzie zapisana.

**Co ta bramka sprawdza i czego nie sprawdza — to jest tu najważniejsze.**

Sprawdza jedną rzecz: **każdy identyfikator w repozytorium jest w ASCII**.
Dzięki temu „po polsku" znaczy w tym projekcie zawsze `zbierz_fakty`, nigdy
`zbierz_faktę`, a nazwa czyta się tak samo w każdym terminalu, edytorze i diffie.
Reguła jest dziś spełniona w całości — **zero identyfikatorów spoza ASCII** — więc
bramka **zamraża konwencję, a nie znajduje usterkę**, i to jest jej cała rola.
Ta sama rola co `REJESTR` w `struct_guard.py`.

Nie sprawdza, czy nazwa publiczna jest **angielska**, i nie da się tego sprawdzić
tanio. Kryterium `R2e` zapowiadało „listę symboli publicznych bez znaku spoza
ASCII" — ta lista jest pusta od pierwszego dnia, więc test na nią byłby spełniony
tożsamościowo. Wykrywacz słownikowy, który napisano zamiast niego, **przepuścił
podłożone `pub fn pomnoz_ulamek`**: słownik łapie wyłącznie słowa, które w nim są,
a nazwa nie ma cechy odróżniającej polski od angielskiego, której da się poszukać
wyrażeniem regularnym. Skan morfemowy zostaje więc pod `--list` jako **pomoc do
przeglądu**, z nazwanym sufitem, a nie jako bramka udająca dowód.

Użycie:
    python scripts/lang_guard.py              # bramka: identyfikatory w ASCII
    python scripts/lang_guard.py --self-test
    python scripts/lang_guard.py --list       # skan morfemowy (pomoc, nie bramka)
"""

from __future__ import annotations

import argparse
import re
import sys
import unicodedata
from pathlib import Path

KORZENIE = ("engine", "sim", "game", "tools")

# Deklaracja nazwy — publiczna albo nie, bo reguła ASCII dotyczy wszystkich.
# `[^\s(<:;=]+` zamiast `\w+`, żeby złapać też nazwę ze znakiem spoza ASCII:
# `\w` w trybie Unicode dopasowałby ją i bramka przepuściłaby to, czego szuka.
DEKLARACJA_DOWOLNA = re.compile(
    r"^\s*(?:pub(?:\([^)]*\))?\s+)?"
    r"(?:async\s+|unsafe\s+|const\s+|extern\s+\"[^\"]*\"\s+)*"
    r"(fn|struct|enum|trait|type|mod|static|union)\s+([^\s(<:;={]+)"
)

# Deklaracja **niezawężona**: `pub` bez nawiasu. Tylko taka przekracza granicę crate'u.
DEKLARACJA_PUBLICZNA = re.compile(
    r"^\s*pub\s+(?:async\s+|unsafe\s+|const\s+|extern\s+\"[^\"]*\"\s+)*"
    r"(fn|struct|enum|trait|type|mod|const|static)\s+([A-Za-z_][A-Za-z0-9_]*)"
)

PUB_MOD = re.compile(r"^\s*pub\s+mod\s+([a-z_][a-z0-9_]*)\s*;")
PUB_USE = re.compile(r"^\s*pub\s+use\s+(.+?);\s*$")

# Morfemy polskie do skanu przeglądowego (`--list`). Lista jest **niepełna z definicji**
# i to jest powód, dla którego nie stoi za nią kod błędu.
PL_MORFEMY = (
    "zbierz sprawdz policz oblicz utworz usun dodaj pobierz ustaw zmien przelicz "
    "odswiez rozstrzygnij zastosuj wykonaj wybierz przeklasyfikuj rozwiaz zwiaz "
    "odejdz sprobuj probuj wypisz narysuj pomnoz podziel odejmij warunek klauzula "
    "wynik slownik powod przyczyna liczba wiersz kolumna nazwa numer rozmiar "
    "dlugosc szerokosc wysokosc odleglosc kierunek promien srodek granica obszar "
    "strefa dzielnica parcela budynek lokal mieszkanie rodzina dziecko dorosly "
    "senior uczen pracownik klient dostawca towar partia oferta umowa zlecenie "
    "dostawa magazyn polka kasa paragon faktura przelew saldo budzet regula "
    "polityka decyzja swiat miasto dom osoba firma zaklad sklep cena placa praca "
    "dzien doba krok wezel droga ulica szkola pojazd podroz zapas masa koszt "
    "zysk strata podatek kredyt konto ksiega smierc opieka ulamek"
).split()
PL = re.compile(r"(?:^|_)(?:" + "|".join(PL_MORFEMY) + r")(?:_|[0-9]|$)", re.IGNORECASE)


def pliki_rust() -> list[Path]:
    out: list[Path] = []
    for korzen in KORZENIE:
        k = Path(korzen)
        if k.is_dir():
            out.extend(sorted(k.rglob("*.rs")))
    return [p for p in out if "target" not in p.parts]


def nie_ascii(nazwa: str) -> str | None:
    """Pierwszy znak spoza ASCII w nazwie, opisany po ludzku."""
    for ch in nazwa:
        if ord(ch) > 127:
            return f"U+{ord(ch):04X} `{ch}` ({unicodedata.name(ch, 'bez nazwy')})"
    return None


def identyfikatory_spoza_ascii() -> list[str]:
    trafienia: list[str] = []
    for plik in pliki_rust():
        try:
            linie = plik.read_text(encoding="utf-8").splitlines()
        except OSError:
            continue
        for i, l in enumerate(linie, 1):
            m = DEKLARACJA_DOWOLNA.match(l)
            if not m:
                continue
            znak = nie_ascii(m.group(2))
            if znak:
                trafienia.append(f"{plik.as_posix()}:{i} {m.group(1)} {m.group(2)} — {znak}")
    return trafienia


# ── skan przeglądowy (`--list`), bez kodu błędu ────────────────────────────────


def crate_dirs() -> list[Path]:
    out = []
    for korzen in KORZENIE:
        k = Path(korzen)
        if not k.is_dir():
            continue
        for cargo in k.glob("*/Cargo.toml"):
            if (cargo.parent / "src").is_dir():
                out.append(cargo.parent)
        if (k / "Cargo.toml").exists() and (k / "src").is_dir():
            out.append(k)
    return sorted(set(out))


def pliki_publiczne(crate: Path) -> set[Path]:
    """Pliki, do których prowadzi ścieżka złożona z samych `pub mod`."""
    src = crate / "src"
    korzen = src / "lib.rs"
    if not korzen.exists():
        korzen = src / "main.rs"
    if not korzen.exists():
        return set()
    publiczne = {korzen}
    do_obejscia = [(korzen, src)]
    while do_obejscia:
        plik, kat = do_obejscia.pop()
        try:
            tekst = plik.read_text(encoding="utf-8")
        except OSError:
            continue
        for nazwa in PUB_MOD.findall(tekst):
            for dziecko, dalej in ((kat / f"{nazwa}.rs", kat / nazwa), (kat / nazwa / "mod.rs", kat / nazwa)):
                if dziecko.exists() and dziecko not in publiczne:
                    publiczne.add(dziecko)
                    do_obejscia.append((dziecko, dalej))
                    break
    return publiczne


def reeksportowane(crate: Path) -> set[str]:
    out: set[str] = set()
    for plik in (crate / "src").rglob("*.rs"):
        try:
            tekst = plik.read_text(encoding="utf-8")
        except OSError:
            continue
        for sciezka in PUB_USE.findall(tekst):
            out.update(re.findall(r"[A-Za-z_][A-Za-z0-9_]*", sciezka))
    return out


def skan_morfemowy() -> list[str]:
    trafienia: list[str] = []
    for crate in crate_dirs():
        publiczne = pliki_publiczne(crate)
        reeks = reeksportowane(crate)
        for plik in sorted((crate / "src").rglob("*.rs")):
            try:
                linie = plik.read_text(encoding="utf-8").splitlines()
            except OSError:
                continue
            for i, l in enumerate(linie, 1):
                m = DEKLARACJA_PUBLICZNA.match(l)
                if not m:
                    continue
                nazwa = m.group(2)
                if (plik in publiczne or nazwa in reeks) and PL.search(nazwa):
                    trafienia.append(f"{plik.as_posix()}:{i} pub {m.group(1)} {nazwa}")
    return trafienia


def bramka() -> int:
    t = identyfikatory_spoza_ascii()
    for s in t:
        print(f"lang_guard: {s}")
    if t:
        print(
            f"\nlang_guard: {len(t)} identyfikator(ów) ze znakiem spoza ASCII. "
            f"Nazwa polska pisze się bez diakrytyków (`zbierz_fakty`), bo czyta się ją "
            f"w terminalu, w diffie i w komunikacie kompilatora (`00` §6)."
        )
        return 1
    print("lang_guard: ok — każdy identyfikator w ASCII.")
    return 0


def lista() -> int:
    t = skan_morfemowy()
    for s in t:
        print(s)
    print(
        f"\nlang_guard --list: {len(t)} nazw publicznych z morfemem z listy. "
        f"To jest **pomoc do przeglądu, nie bramka**: lista morfemów jest niepełna "
        f"z definicji i przepuszcza każde słowo, którego w niej nie ma."
    )
    return 0


def self_test() -> int:
    ok = True

    # Znak spoza ASCII w nazwie — wykrywany niezależnie od tego, czy nazwa jest publiczna.
    for l in ("pub fn oblicz_cenę() {}", "    fn długość() -> u32 { 0 }", "struct Słownik;"):
        m = DEKLARACJA_DOWOLNA.match(l)
        if not m or not nie_ascii(m.group(2)):
            ok = False
            print(f"self-test: nie wykryto znaku spoza ASCII w {l.strip()!r}")
    for l in ("pub fn zbierz_fakty() {}", "struct Slownik;", "    fn plan_day() {}"):
        m = DEKLARACJA_DOWOLNA.match(l)
        if not m or nie_ascii(m.group(2)):
            ok = False
            print(f"self-test: fałszywy alarm na {l.strip()!r}")

    # Deklaracja zawężona nie przekracza granicy crate'u — skan morfemowy ma ją pomijać.
    for l in ("    pub(crate) fn zbierz_fakty() {}", "    pub(super) struct Slownik;"):
        if DEKLARACJA_PUBLICZNA.match(l):
            ok = False
            print(f"self-test: `pub(…)` policzone jako publiczne: {l.strip()!r}")
    if not DEKLARACJA_PUBLICZNA.match("pub fn zbierz_fakty() {}"):
        ok = False
        print("self-test: nie rozpoznano zwykłej deklaracji publicznej")

    print("lang_guard --self-test:", "ok" if ok else "BŁĄD")
    return 0 if ok else 1


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--self-test", action="store_true", help="sprawdź sam wykrywacz")
    ap.add_argument("--list", action="store_true", help="skan morfemowy — pomoc do przeglądu")
    a = ap.parse_args()
    if a.self_test:
        return self_test()
    return lista() if a.list else bramka()


if __name__ == "__main__":
    sys.exit(main())
