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

# Scenariusze `tools/headless` to bramki CI z CLI zamiast `#[test]` — `ci.yml`
# uruchamia `m3day` i `m5shop` z `--out`/`--expect` i porównuje ciągi hashy.
# Obowiązuje ten sam powód, którym R1 §2 zwolniło `tests/`: w `m5shop::run`
# kolejność wydruku **jest** raportem, a abstrakcja nad nią pogarsza jedyną
# własność, jaką ten kod ma. Granicę rysuje sam crate — `tools/headless/src/lib.rs`
# wystawia dokładnie to, co ma więcej niż jednego konsumenta (`population`,
# `retail`), a scenariusze zostają modułami binarki. Mierzymy więc to, co
# deklaruje `lib.rs`, i nic poza tym; lista nie może się zestarzeć, bo powstaje
# z odczytu tego pliku.
#
# Nie dotyczy `tools/magnat` ani `tools/balansator`: klient graficzny jest kodem
# produktu (dzielił go R-WP2), a balansator narodził się z granicami.
HEADLESS = "tools/headless/src/"

# Przekroczenia progu błędu przyjęte świadomie przy domknięciu R1 (`D-31`: kryterium
# akceptacji nr 5 dostało klauzulę wyjątków, symetryczną do nr 4). Każda pozycja ma
# wiersz w rejestrze długu strukturalnego na końcu `R1-refaktor-po-M5.md` — z nazwanym
# sufitem, powodem, dla którego podział nie jest przeniesieniem bloku, i fazą-właścicielem.
#
# Klucz niesie **wartość**, nie tylko nazwę: dopisanie choćby jednej linii do którejkolwiek
# z tych funkcji przestawia liczbę i bramka zapala się z powrotem. Wyjątek jest więc
# zamrożeniem stanu, nie zwolnieniem symbolu — i to jest jedyny powód, dla którego job
# `struct-guard` w CI może przestać być czerwony na stałe. Bramka, która zawsze świeci
# na czerwono, zostanie wyłączona (ryzyko R-5), a wtedy nie łapie już niczego.
REJESTR = {
    # **Pozycje 33 i 34 zamknięte w R2e** (`R2-WP21`): `lsystem.rs` rozpadł się na
    # `lsystem/{mod,rules,constrain}.rs`, blok `impl` zszedł z 609 na 438 (ostrzeżenie,
    # nie błąd), a `grow_network` z 317 na 66. Macierz 32 hashy miasta wyszła identyczna
    # bit w bit (`sim/world/tests/lsystem_split.rs`).
    #
    # **Pozycja 35 zostaje i to jest decyzja, nie zaniedbanie.** `Builder::grow` ma
    # 261 linii (257 przed `cargo fmt`, który rozwinął sygnaturę), bo jest **jednym algorytmem**: drabiną ograniczeń, w której kolejność
    # kroków jest kontraktem generacji miasta. Podział na „grow_a" i „grow_b" przeniósłby
    # połowę drabiny do drugiego pliku i nie dodał ani jednej granicy tematycznej —
    # a CLAUDE.md mówi wprost, że dzieli się pliki z dwoma tematami, nie pliki długie.
    # Sufit nazwany w kodzie komentarzem `ponytail:`.
    ("sim/world/src/city/lsystem/constrain.rs", "fn", 261): 35,
    # 365 do M6d, 368 po M6e: trzy linie za bilans otwarcia złóż (`AP-1`) —
    # jedno wywołanie `bilans_zloz`, jedno pole w `CityData` i pusta linia.
    # **370 po M11c** — dwie linie, których M11c nie zamroziło; patrz komentarz
    # przy `lsystem.rs` wyżej, to ten sam przypadek i ta sama data.
    ("sim/world/src/city/mod.rs", "fn", 370): 24,
    ("sim/world/src/city/zoning.rs", "fn", 322): 36,
    # Rośnie o jedno ramię na wariant `DecisionReason`. 319 przed M6b, 355 po bloku
    # M6b (400–402), 397 po M6c (403–405), 451 po M7b (500–502, przy czym `WageRaise`
    # ma dwa ramiona: podwyżka i sufit), 493 po M7c (503–504, przy czym `PolicyApplied`
    # ma dwa ramiona: reguła zwykła i zapasowa; 499 po sformatowaniu pliku).
    #
    # **Prognoza z rejestru się nie sprawdziła i to jest wpisane wprost**: R1 poz. 37
    # zakładał, że M7c rozetnie tę funkcję przebudową `DecisionReason` na
    # `Citizen | Firm | City`. Przebudowa nie weszła (patrz `AX-7` w `M7c-…md`),
    # więc funkcja rośnie dalej, a adres podziału przesuwa się na osobną zmianę.
    # 566 po M7d: siedem ramion bloku finansowego (505–510, przy czym
    # `BankruptcyOpened` ma dwa — brak płynności mierzy się dobami, ujemny kapitał nie).
    # 618 po M7e: pięć ramion AI firm (511–515).
    # 660 po M7f: cztery ramiona cyklu życia firm (516–519). Przyrost +42 jest
    # najmniejszy w całym bloku M7 i ma powód wart zapisania: żaden z czterech
    # wariantów nie rozgałęził się na dwa klucze lokalizacji, a `SiteOpened` mimo
    # trzech wartości `Trend` ma **jedno** ramię — kierunek wchodzi podstawieniem
    # do zdania, a nie wyborem klucza.
    # 730 po M8a: siedem ramion bloku M8 (600–606), po jednym na wariant. Przyrost
    # +70 wobec prognozy „~20 na podfazę" i powód jest ten sam, co przy M7d: żadne
    # z siedmiu ramion nie rozgałęziło się na drugi klucz lokalizacji, ale **każde
    # ma po trzy podstawienia** (danina, kwota, stawka/powód/kierunek), a formatowanie
    # trzech argumentów to dziesięć linii na ramię, nie pięć. Siedem danin nie robi
    # siedmiu zdań o naliczeniu — robi jedno zdanie z siedmioma podstawieniami,
    # i to jest właśnie ten podział, który utrzymuje przyrost liniowym.
    # 754 po M8b: dwa ramiona sieci przesyłowych (607–608), +24. Przyrost wraca
    # do prognozy „~20 na podfazę" i pokazuje tę samą regułę z drugiej strony:
    # `LoadShed` ma trzy podstawienia i dwanaście linii, `GridTripped` dwa
    # i dziesięć. Siedem mediów nie robi czternastu zdań — rodzaj medium wchodzi
    # podstawieniem `{medium}`, tak samo jak danina w bloku M8a.
    # 780 po M8c: dwa ramiona zdarzeń świata (609–610), +26. `EventStarted`
    # ma trzy podstawienia (kategoria, siła, numer) i trzynaście linii,
    # `EventEnded` też trzy i trzynaście — a kategorii jest sześć i wchodzą
    # jednym `{kategoria}`, tak samo jak siedem danin w M8a i siedem mediów w M8b.
    # Blok M8 ma po M8c zajęte 600–610 i trzy podfazy przed sobą.
    # 855 po M8d, +75 — i to jest **największy przyrost w całym bloku M8**,
    # z dwóch powodów naraz. Pierwszy: `ServiceQuality` ma **sześć** podstawień
    # (usługa, dzielnica, jakość, pieniądze, obsada, obłożenie), bo jakość placówki
    # bez rozbicia na czynniki jest liczbą bez odpowiedzi na „dlaczego tyle" —
    # to samo rozstrzygnięcie, które w M8c dało kartę zdarzenia z czynnikami
    # hazardu zamiast wyniku. Drugi: `RemedyImposed` jest **pierwszym ramieniem
    # bloku M8, które rozgałęzia się na dwa klucze lokalizacji** — kara z kwotą
    # i kara bez kwoty to dwa różne zdania, bo „0 zł" przy zawieszeniu działalności
    # mówiłoby graczowi, że nic go to nie kosztowało.
    # Przy okazji doszły dwa ramiona **zaległe z M8c**: `EventStarted` i `EventEnded`
    # miały ramiona w `describe` od M8c, ale nie miały wpisu na liście testu
    # `kazdy_powod_ma_tekst_w_obu_jezykach`, więc przez całą podfazę nikt nie
    # sprawdził, czy ich zdanie składa się w obu językach.
    # 972 po M8e: siedem ramion władzy i wyborów (616–622), +117 — i **drugi
    # w kolejności przyrost całego bloku M8**, z powodu, który wraca po raz trzeci.
    # Trzy z siedmiu ramion rozgałęziają się na dwa klucze lokalizacji, bo niosą
    # zdania o przeciwnym znaczeniu: podwyżka stawki i obniżka („budżet rozjechał
    # się z celem" wobec „budżet ma zapas"), przetarg rozstrzygnięty i przetarg
    # bez ofert, wybory utrzymujące burmistrza i wybory zmieniające władzę.
    # W każdym z tych trzech wspólne zdanie z podstawieniem byłoby zdaniem
    # mówiącym mniej niż liczba, którą niesie.
    #
    # **Plik przekracza po M8e również próg pliku (1329 > 1200) i to jest pierwsze
    # takie przekroczenie.** Odpowiedź jest ta sama i ten sam adres: pozycja 37
    # rejestru długu w `R1-refaktor-po-M5.md`. Podziału nie robi się teraz, bo
    # podział, który tu pomaga, to **nie** rozcięcie pliku na dwa — plik ma jeden
    # temat (powód → zdanie) i rozcięcie go dałoby dwa pliki o jednym temacie.
    # Pomaga dopiero `K-58` (R2e): rozbicie `DecisionReason` na trzy enumy po
    # aktorze rozcina razem z nim ten `match`, bo to jest ten sam podział widziany
    # z drugiej strony. Do tego czasu funkcja rośnie liniowo z liczbą wariantów
    # i jest to wzrost, który da się przewidzieć co do rzędu wielkości.
    # 1047 po M10b, **1106 po M10c** (+59): cztery powody R&D (804–807), z czego
    # jeden rozgałęziony na dwa klucze — odkrycie z patentem i odkrycie po roku
    # „światowym" to dwa różne zdania o tym samym wyniku, bo różnicę robi
    # wyprzedzenie świata, a nie znak liczby. Plus jedna funkcja pomocnicza
    # nazywająca technologię. Przyrost trafia w regułę tej pozycji: jedno
    # rozgałęzienie plus dziewięć podstawień.
    # 1190 po M10d, **1276 po M10e** (+86): osiem powodów relacji, zmów i związków
    # (817–824), z czego **jeden rozgałęziony na dwa klucze** — strajk wygrany
    # i strajk przegrany to dwa różne zdania o tej samej liczbie dób, bo zero
    # podwyżki znaczy kapitulację, a nie brak pomiaru. Przyrost trafia w regułę
    # tej pozycji piąty raz z rzędu: liczy się liczba rozgałęzień, nie wariantów.
    # Zostaje jedna podfaza fazy M10.
    # **1290 po R2a** (+14): dwa powody cyklu życia gospodarstwa (120–121), z czego
    # `GuardianAppointed` rozgałęziony na dwa klucze — kuratela krewnego i kuratela
    # obcego to dwa różne zdania o tej samej liczbie podopiecznych, bo różnicę robi
    # pokrewieństwo, a nie waga relacji. Przyrost trafia w regułę tej pozycji szósty
    # raz z rzędu: liczy się liczba rozgałęzień, nie wariantów. Adres podziału bez
    # zmian — `K-58` w `R2e`, i to ona rozetnie tę funkcję razem z enumem.
    # **Pozycja 37 po R2-WP20: rozcięta, ale nie zamknięta — i to jest wynik pomiaru,
    # a nie porażka.** `describe` (1290 linii, plik 1761) rozpadła się na trzy renderery
    # po aktorze: 376 linii mieszkaniec, 634 firma, 287 miasto. Kryterium R2e mówiło
    # „poniżej 300 linii każda" i **było nieosiągalne z arytmetyki**: 1290 linii ramion
    # podzielone na trzech aktorów to średnio 430, a firma ma 54 ramiona ze 115.
    #
    # Dalszy podział **po temacie wewnątrz aktora** nie składa się w nic mniejszego:
    # dyspozytor musiałby wymienić te same 54 warianty, żeby wiedzieć, do którego pliku
    # je posłać. Aktor jest jedyną osią, wzdłuż której ten `match` da się rozciąć raz.
    #
    # Co się przez to zmieniło i dlaczego pozycja zostaje mimo wszystko otwarta:
    # funkcja **przestała rosnąć z liczbą faz** i rośnie z liczbą decyzji **swojego**
    # aktora. M11 dokłada powody firmie i mieszkańcowi, a plik miasta nie drgnie.
    # Adresat: faza, która dołoży aktora albo zmieni kształt karty inspekcji.
    ("engine/ui/src/inspect/reason/citizen.rs", "fn", 376): 37,
    ("engine/ui/src/inspect/reason/firm.rs", "fn", 634): 37,
    ("engine/ui/src/inspect/reason/city.rs", "fn", 287): 37,
    # 269 po M7d: jedna linia za `finance: bf.finance` w budowie `BankParams`.
    # **Cztery pozycje zamrożone w R2e, wszystkie zastane.** Każda ma wiersz w rejestrze
    # długu z powodem i adresatem (62–65), i każda przeszła przez R2a–R2c bez wpisu tutaj —
    # więc job `struct-guard` świecił na czerwono od R2a i przestał cokolwiek znaczyć.
    # Bramka, która zawsze świeci na czerwono, zostaje wyłączona (ryzyko R-5), a wtedy
    # nie łapie już niczego. Zamrożenie **nie jest zgodą na wzrost**: klucz niesie wartość,
    # więc dopisanie jednej linii zapala bramkę z powrotem.
    ("sim/agents/src/migration.rs", "plik", 1265): 62,
    ("sim/agents/src/demography/mod.rs", "mod.rs", 641): 64,
    ("sim/economy/src/market/api.rs", "impl", 541): 63,
    # 269 przed R2c, 270 po niej — `R2-WP16` ubrało `envelopes.ron` o wiersz, a nie
    # dodało czytnika, więc wyzwalacz z pozycji 38 dalej nie zadziałał.
    ("sim/economy/src/data.rs", "fn", 270): 65,
    # 600 przed R2f, 603 po — `R2-WP25` dołożyło ramię `WorldSize::Km2` do `target_pop`
    # razem z dwuwierszowym komentarzem. Próg błędu `mod.rs` to dokładnie 600, więc trzy
    # linie przesunęły metrykę z ostrzeżenia na błąd. Pozycja 68 rejestru; podziału nie
    # robimy tutaj, bo `city/mod.rs` jest orkiestratorem generacji, a jego rozcięcie jest
    # przeprojektowaniem Etapu 7 — czyli `R3`.
    ("sim/world/src/city/mod.rs", "mod.rs", 603): 68,
}

# ── egzekutor rejestru długu (R2-WP26) ───────────────────────────────────────────
#
# Reguła z R1 brzmi: **pozycja rejestru ma adresata** — fazę, która ją otworzy
# z powodu innego niż liczba linii. W trzydziestu przypadkach zadziałała, w kilku
# nie, i każdy zawiódł inaczej: adresat skreślony bez zastąpienia, adresat, którego
# nigdy nie było, wyzwalacz, który nie strzelił raz, i wyzwalacz, który nie strzelił
# trzy razy. Wspólne dla wszystkich: **nic nie sprawdzało, czy adresat istnieje
# i czy jeszcze nie minął.** Rejestr jest tabelą w markdownie, a ta bramka mierzyła
# linie w kodzie — dwie rzeczy, które nigdy się nie spotykały.

REJESTR_PLANU = "docs/implementation-plan/R1-refaktor-po-M5.md"
NAGLOWEK_REJESTRU = "## Rejestr długu strukturalnego"
POSTEP = "docs/implementation-plan/00-postep.md"
PLAN_DIR = "docs/implementation-plan"

# Identyfikator fazy: `M0`…`M12` z opcjonalną literą podfazy, `R1`…`R9` tak samo.
# Negatywne spojrzenie w przód na `-WP` jest konieczne, a nie ostrożnościowe: `R2-WP26`
# to nazwa **pakietu**, a nie adres fazy, i bez tego wyjątku wiersz opisujący własną
# poprawkę zapalałby bramkę po zamknięciu R2. Złapane przy pierwszym odhaczeniu R2.
FAZA = re.compile(r"\b(M(?:1[0-2]|[0-9])[a-g]?|R[1-9][a-f]?)\b(?!-WP)")
# Wykreślenie w markdownie: `~~M8e~~`. Skreślony adresat **nie liczy się** —
# to jest dokładnie ten przypadek, który przeżył sześć faz (pozycja 37).
SKRESLONE = re.compile(r"~~.*?~~", re.S)
# Data przeglądu przy pozycji świadomie bez adresata fazowego.
DATA_PRZEGLADU = re.compile(r"\b20\d\d-\d\d-\d\d\b")
# Pozycja zamknięta: znacznik w kolumnie numeru. Jeden znak, jedna reguła —
# rozpoznawanie zamknięcia po prozie („zrobione", „wykonane") kończy się
# wyłączeniem bramki po trzecim fałszywym alarmie, a bramka wyłączona nie łapie nic.
ZAMKNIETA = "✅"


def wiersze_rejestru(tekst: str) -> list[tuple[str, str]]:
    """Pary `(numer, ścieżka wyjścia)` z tabeli rejestru długu.

    Tabela ciągnie się od swojego nagłówka do następnego nagłówka `## `.
    Wiersz nagłówka i separator odpadają po **kształcie**, a nie po pozycji:
    tabela jest w tym dokumencie przerwana poziomą kreską i druga połowa nie
    ma własnego nagłówka.
    """
    poczatek = tekst.find(NAGLOWEK_REJESTRU)
    if poczatek < 0:
        return []
    koniec = tekst.find("\n## ", poczatek + len(NAGLOWEK_REJESTRU))
    koniec = len(tekst) if koniec < 0 else koniec
    wyniki = []
    for linia in tekst[poczatek:koniec].split("\n"):
        if not linia.startswith("|"):
            continue
        # Kreska pionowa w treści jest w markdownie escapowana (`\|`) i **nie** dzieli
        # kolumny — bez tego rozbiór wiersza z `Citizen \| Firm \| City` gubi ścieżkę
        # wyjścia i bramka czyta cudzą komórkę.
        surowa = linia.replace("\|", "\u0001")
        kol = [c.strip().replace("\u0001", "\|") for c in surowa.strip().strip("|").split("|")]
        if len(kol) < 5 or kol[0] in ("#", "") or set(kol[0]) <= set("-: "):
            continue
        # **Ostatnia** kolumna, nie piąta: ścieżka wyjścia stoi na końcu wiersza,
        # a wiersz z doklejonym sprostowaniem bywał o kolumnę dłuższy.
        wyniki.append((kol[0], kol[-1]))
    return wyniki


def odhaczone_fazy(tekst: str) -> set[str]:
    """Identyfikatory pozycji `- [x] **X**` w dokumencie postępu."""
    return set(re.findall(r"^-\s*\[x\]\s*\*\*([A-Za-z0-9-]+)\*\*", tekst, re.M))


def faza_minela(faza: str, odhaczone: set[str], korzen: pathlib.Path) -> bool:
    """Czy adresat już się zamknął.

    Podfaza minęła, gdy jest odhaczona wprost. Faza bez litery minęła, gdy
    **wszystkie** jej podfazy są odhaczone — bo to one są porcją wykonawczą
    (`K-17`), a wiersz „Faza ukończona" nie niesie identyfikatora.
    """
    if faza in odhaczone:
        return True
    if not faza[-1].isdigit():
        return False
    podfazy = {f for f in odhaczone if f.startswith(faza) and len(f) == len(faza) + 1}
    wszystkie = {p.name.split("-")[0] for p in (korzen / PLAN_DIR).glob(f"{faza}[a-g]-*.md")}
    return bool(wszystkie) and wszystkie <= podfazy


def dokument_fazy(faza: str, korzen: pathlib.Path) -> bool:
    return any((korzen / PLAN_DIR).glob(f"{faza}-*.md"))


def sprawdz_rejestr(korzen: pathlib.Path) -> list[str]:
    """Błędy rejestru długu. Pusta lista znaczy: każda pozycja ma żywego adresata."""
    try:
        tekst = (korzen / REJESTR_PLANU).read_text(encoding="utf-8")
        postep = (korzen / POSTEP).read_text(encoding="utf-8")
    except OSError as e:
        return [f"nie da się przeczytać rejestru: {e}"]

    odhaczone = odhaczone_fazy(postep)
    bledy: list[str] = []
    widziane: set[str] = set()
    for numer, wyjscie in wiersze_rejestru(tekst):
        if ZAMKNIETA in numer:
            continue
        nr = numer.split()[0]
        if nr in widziane:
            bledy.append(f"pozycja {nr}: numer użyty drugi raz — wiersza nie da się wskazać")
        widziane.add(nr)

        zywe = SKRESLONE.sub("", wyjscie)
        kandydaci = FAZA.findall(zywe)
        if not kandydaci:
            if not DATA_PRZEGLADU.search(zywe):
                bledy.append(
                    f"pozycja {nr}: brak adresata fazowego i brak daty przeglądu"
                )
            continue
        nieistniejace = [f for f in kandydaci if not dokument_fazy(f, korzen)]
        zywi = [
            f
            for f in kandydaci
            if dokument_fazy(f, korzen) and not faza_minela(f, odhaczone, korzen)
        ]
        if zywi:
            continue
        if nieistniejace:
            bledy.append(f"pozycja {nr}: adresat {nieistniejace[0]} nie ma dokumentu w {PLAN_DIR}/")
        else:
            bledy.append(
                f"pozycja {nr}: adresat {'/'.join(kandydaci)} już się zamknął"
            )
    return bledy


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


def moduly_biblioteczne_headless(korzen: pathlib.Path) -> set[str]:
    """Pliki `tools/headless/src/`, które wystawia `lib.rs` — te i tylko te mierzymy.

    Czytane z pliku, nie wypisane z listy: lista by się zestarzała przy pierwszym
    module dopisanym do `lib.rs`, a wtedy bramka przestałaby mierzyć most, na
    którym stoi klient graficzny i balansator.
    """
    lib = korzen / HEADLESS / "lib.rs"
    try:
        tresc = lib.read_text(encoding="utf-8")
    except OSError:
        return set()
    nazwy = {m.group(1) for m in re.finditer(r"^\s*pub mod (\w+)\s*;", tresc, re.M)}
    return {f"{HEADLESS}{n}.rs" for n in nazwy} | {f"{HEADLESS}lib.rs"}


def pliki_produkcyjne(sciezki, korzen: pathlib.Path | None = None) -> list[pathlib.Path]:
    headless = moduly_biblioteczne_headless(korzen) if korzen else None
    wybrane = []
    for p in sciezki:
        s = p.as_posix()
        if p.suffix != ".rs" or any(k in s for k in KATALOGI_POMIJANE):
            continue
        if headless is not None and HEADLESS in s and not any(s.endswith(h) for h in headless):
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
            powod = WYJATKI_PLIK.get(wzgledna) if zwolniony else None
            wpis = REJESTR.get((wzgledna, metryka, wartosc))
            if not zwolniony and wpis and wartosc > blad:
                zwolniony, powod = True, f"rejestr długu R1, pozycja {wpis}"
            pozycje.append(
                {
                    "plik": wzgledna,
                    "metryka": metryka,
                    "opis": opis,
                    "linie": wartosc,
                    "prog": "wyjątek" if zwolniony else ("błąd" if wartosc > blad else "ostrzeżenie"),
                    "powod": powod,
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


def test_rejestru() -> int:
    """Bramka rejestru na atrapie: każdy z czterech sposobów, na które adresat zawiódł.

    Cztery przypadki nie są wymyślone — każdy zdarzył się w tym repozytorium i każdy
    przeżył co najmniej trzy fazy: adresat skreślony bez zastąpienia, adresat, którego
    nigdy nie było, adresat, który się zamknął, i numer użyty dwa razy.
    """
    atrapa = (
        "## Rejestr długu strukturalnego\n\n"
        "| # | Plik | Metryka | Powód | Ścieżka wyjścia |\n|---|---|---|---|---|\n"
        "| 1 | `a.rs` | plik 900 | bo tak | **M12a** — pamięć dokłada tu pomiar |\n"
        "| 2 | `b.rs` | plik 900 | bo tak | **M11a** — zamknięta faza |\n"
        "| 3 | `c.rs` | plik 900 | bo tak | ~~**M12a**~~ → nikt |\n"
        "| 4 | `d.rs` | plik 900 | bo tak | **M99z** — fazy nie ma |\n"
        "| 5 | `e.rs` | plik 900 | bo tak | dziś nie planuje go nikt, przegląd 2026-09-22 |\n"
        "| 1 | `f.rs` | plik 900 | bo tak | **M12a** — numer użyty drugi raz |\n"
        "| 7 ✅ | `g.rs` | plik 900 | bo tak | **M11a** — zamknięta pozycja, adresat nieżywy |\n"
        "\n## Co innego\n"
    )
    postep = "- [x] **M11a** Format i snapshot\n- [ ] **M12a** Pamięć\n"
    with tempfile.TemporaryDirectory() as katalog:
        korzen = pathlib.Path(katalog)
        plan = korzen / PLAN_DIR
        plan.mkdir(parents=True)
        (plan / "R1-refaktor-po-M5.md").write_text(atrapa, encoding="utf-8")
        (plan / "00-postep.md").write_text(postep, encoding="utf-8")
        for f in ("M11a-format-i-snapshot.md", "M12a-pamiec.md"):
            (plan / f).write_text("x", encoding="utf-8")
        bledy = sprawdz_rejestr(korzen)

    oczekiwane = {
        "2": "adresat zamknięty",
        "3": "adresat skreślony bez zastąpienia",
        "4": "adresat bez dokumentu",
        "1": "numer użyty drugi raz",
    }
    znalezione = {b.split()[1].rstrip(":") for b in bledy}
    kod = 0
    for nr, opis in oczekiwane.items():
        ok = nr in znalezione
        kod |= 0 if ok else 1
        print(f"{'OK    ' if ok else 'BLAD  '} rejestr: {opis} (pozycja {nr})")
    # Pozycja 5 ma datę przeglądu, 7 jest zamknięta — żadna nie ma prawa się zapalić.
    for nr in ("5", "7"):
        ok = nr not in znalezione
        kod |= 0 if ok else 1
        print(f"{'OK    ' if ok else 'BLAD  '} rejestr: pozycja {nr} bez falszywego alarmu")
    return kod


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

    # Wykluczenie scenariuszy `tools/headless` jest filtrem, nie metryką — regres
    # w nim (np. wykluczenie całego `tools/`) byłby **cichy**, bo bramka świeciłaby
    # wtedy na zielono z mniejszą liczbą plików. Sprawdzamy więc na prawdziwym
    # repozytorium, że rusztowanie biblioteki nadal jest mierzone, a scenariusz już nie.
    #
    # **Do M10c ta asercja wskazywała `tools/headless/src/retail.rs`, którego nie ma
    # od M9a**: most detaliczny wyprowadził się do `game/src/world/retail.rs`
    # (`K-68`), a fikstura o tym nie wiedziała — więc `--self-test` świecił na
    # czerwono od tamtej pory i job `struct-guard` w CI nie przechodził wcale.
    # Plikiem, który zostaje po tej stronie i jest mierzony, jest `population.rs`.
    korzen = pathlib.Path(__file__).resolve().parent.parent
    mierzone = {p.as_posix() for p in pliki_produkcyjne(korzen.glob("**/*.rs"), korzen)}
    for wzgledna, ma_byc in (
        ("tools/headless/src/population.rs", True),
        ("tools/headless/src/m5shop.rs", False),
    ):
        jest = any(s.endswith(wzgledna) for s in mierzone)
        ok = jest == ma_byc
        kod |= 0 if ok else 1
        czy = "mierzony" if ma_byc else "pominiety"
        print(f"{'OK    ' if ok else 'BLAD  '} {wzgledna} {czy}")
    # `tools/magnat` to kod produktu, nie scenariusz — musi zostać mierzony.
    klient = any(s.endswith("tools/magnat/src/app.rs") for s in mierzone)
    kod |= 0 if klient else 1
    print(f"{'OK    ' if klient else 'BLAD  '} tools/magnat/src/app.rs mierzony")

    # Wpis w `REJESTR`, który przestał odpowiadać czemukolwiek w kodzie, jest martwy:
    # ktoś podzielił funkcję i zapomniał usunąć zwolnienie, więc następne przekroczenie
    # w tym samym miejscu przejdzie po cichu. Sprawdzamy, że każda pozycja nadal opisuje
    # realne przekroczenie — w drugą stronę bramka broni się sama, bo klucz niesie wartość.
    biezace = set()
    for plik in pliki_produkcyjne(korzen.glob("**/*.rs"), korzen):
        wzgledna = plik.relative_to(korzen).as_posix()
        if not any(wzgledna == k[0] for k in REJESTR):
            continue
        try:
            for metryka, _, wartosc in zmierz(plik, plik.read_text(encoding="utf-8")):
                biezace.add((wzgledna, metryka, wartosc))
        except (OSError, UnicodeDecodeError):
            continue
    martwe = [k for k in REJESTR if k not in biezace]
    kod |= 0 if not martwe else 1
    print(f"{'OK    ' if not martwe else 'BLAD  '} REJESTR bez martwych wpisow ({len(REJESTR)} pozycji)")
    for k in martwe:
        print(f"       martwy wpis: {k[0]} {k[1]} {k[2]} (pozycja {REJESTR[k]})")

    # Hook `PreToolUse` odpala się tylko na narzędziach z `matcher`. Agent commituje
    # z `Bash` albo z `PowerShell` — do N1.9 matcher znał tylko pierwsze, więc na
    # Windows raport przed commitem nie pojawiał się wcale.
    ustawienia = korzen / ".claude" / "settings.json"
    matchery = [
        wpis.get("matcher", "")
        for wpis in json.loads(ustawienia.read_text(encoding="utf-8")).get("hooks", {}).get("PreToolUse", [])
        if any("struct_guard.py" in h.get("command", "") for h in wpis.get("hooks", []))
    ] if ustawienia.exists() else []
    brak = [n for n in ("Bash", "PowerShell") if not any(re.fullmatch(m, n) for m in matchery)]
    kod |= 1 if brak else 0
    print(f"{'OK    ' if not brak else 'BLAD  '} hook przed commitem obejmuje Bash i PowerShell"
          + (f" (brak: {', '.join(brak)})" if brak else ""))
    return kod | test_rejestru()


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
    pliki = pliki_produkcyjne(zmienione_wzgledem_head(korzen), korzen)
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
        pliki = pliki_produkcyjne(zmienione_wzgledem_head(korzen), korzen)
        if not pliki:
            return 0
        return min(raport(korzen, pliki, args.json), 0)  # hook informuje, nie blokuje
    kod = raport(korzen, pliki_produkcyjne(korzen.glob("**/*.rs"), korzen), args.json)
    if args.json:
        return kod
    # Rejestr długu jest drugą połową tej bramki (`R2-WP26`). Pozycja, której adresat
    # zamknął się bez niej, jest **błędem bramki**, nie wpisem w tabeli — to jest
    # dokładnie ta klasa, która przeżyła w tym repozytorium siedem faz.
    bledy = sprawdz_rejestr(korzen)
    for b in bledy:
        print(f"BLAD   rejestr długu: {b}")
    print(f"struct_guard: rejestr długu — pozycji bez żywego adresata: {len(bledy)}")
    return kod | (1 if bledy else 0)


if __name__ == "__main__":
    sys.exit(main())
