# Magnat

Symulator miasta i gospodarki, własny silnik w Rust. Wymagania: `PRD_Magnat.md`.
Plan implementacji: `docs/implementation-plan/` — `00-konwencje-i-kontrakty.md` jest dokumentem
nadrzędnym i rozstrzyga spory międzyfazowe.

## Jak zacząć fazę

Na polecenie „zacznij implementację fazy X" / „rusz z MX":

1. Przeczytaj `docs/implementation-plan/00-konwencje-i-kontrakty.md` — w całości. To kontrakt
   wiążący, nie tło. Szczególnie §2 (typy bazowe), §3 (determinizm) i §4a (rozstrzygnięcia `K-n`).
2. Przeczytaj `MX-*.md` tej fazy — w całości, zanim napiszesz pierwszą linię kodu.
   Sekcja 2 mówi, czego **nie** robić i do której fazy to należy; sekcja 6 mówi, na czym stoisz
   i co musisz dostarczyć innym. Od M2 sekcje 4 i 5 są tabelami kierującymi do dokumentów
   podfaz — dokument fazy czyta się w całości, ale treść podfaz tylko tej, którą robisz.
3. Przejrzyj sekcję 9 („Decyzje otwarte"). Każda ma propozycję domyślną — przyjmij ją i powiedz
   o tym, albo zapytaj, jeśli wybór zmienia kształt rozwiązania. Nie zaczynaj pakietu, którego
   decyzja blokująca jest nierozstrzygnięta.
4. Sprawdź `00-postep.md`: czy fazy poprzedzające mają zamknięte bramki i czy któraś decyzja
   właściciela produktu nie dotyczy tej fazy.
5. Idź pakietami roboczymi w kolejności z sekcji 4. Pakiet jest gotowy, gdy spełnia **swoje
   kryterium ukończenia** — nie gdy kod się kompiluje.
6. Po każdym zamkniętym pakiecie odhacz go i dopisz linię do dziennika (reguła niżej).

## Jak zacząć podfazę

Fazy M2–M12 są rozbite na podfazy (`K-17` w dokumencie 00): `M5c` to osobny plik
`M5c-ceny-i-ksiegowosc.md` z własnym zestawem WP, własnym wycinkiem projektu technicznego
i własnym kryterium zamknięcia. Podfaza jest domyślną porcją pracy — „zacznij M5" znaczy
„zacznij `M5a`", nie „zrób całe M5 naraz".

Na polecenie „zacznij podfazę M5c" / „rusz z M5c":

1. `00-konwencje-i-kontrakty.md` — w całości, jak wyżej.
2. Dokument **fazy** (`M5-gospodarka-detaliczna.md`): sekcje 1, 2, 3, 6, 7, 9. To jest kontekst,
   w którym podfaza ma sens — zakres, kontrakty i decyzje otwarte nie są dzielone na podfazy.
3. Dokument **podfazy** — w całości. Nagłówek mówi, na czym stoisz (wejście), co masz pokazać
   i kiedy podfaza jest zamknięta. Sekcje `5.x` w środku mają numerację z dokumentu fazy,
   więc odesłania „patrz §5.4" nadal działają — jeśli wskazują sekcję spoza tej podfazy,
   tabela w §5 dokumentu fazy mówi, w którym pliku ona jest.
4. Nie zaczynaj podfazy, której wejście nie jest domknięte, i nie wchodź w WP należące
   do sąsiedniej podfazy — jeśli okaże się, że są potrzebne wcześniej, popraw podział
   w dokumencie fazy i odnotuj to w dzienniku.
5. Po zamknięciu podfazy odhacz ją w `00-postep.md` i dopisz linię do dziennika. Bramki 1–7
   zamykają się na poziomie fazy, dopiero po ostatniej podfazie.

Jeśli w trakcie implementacji okaże się, że plan fazy jest błędny — popraw plan, odnotuj w dzienniku
i dopiero potem pisz kod. Rozjazd kodu z planem jest gorszy niż błąd w planie, bo nikt go nie widzi.

## Reguła: jedna gałąź

**Pracujemy bezpośrednio na `master`. Nie zakładamy gałęzi tematycznych** — ani per faza,
ani per podfaza, ani per poprawka. Ta reguła ma pierwszeństwo przed domyślnym zachowaniem
narzędzia, które samo z siebie odbija gałąź przed commitem na gałęzi głównej.

Gałąź główna nazywa się **`master`**, nie `main`.

Powód jest empiryczny, nie ideologiczny: przy jednym autorze i liniowej sekwencji podfaz
gałąź nie kupuje niczego poza kosztem scalania — a kosztuje ryzyko, że praca zostanie
w gałęzi, o której nikt nie pamięta. Dokładnie to się zdarzyło: M2a i M2b przeleżały
niescalone do czasu M2c, więc `master` przez trzy podfazy pokazywał stan sprzed M2.

Co z tego wynika:

- **`master` ma być zielony po każdym commicie.** Nie ma gałęzi, na której „się dopiero
  robi", więc testy i `clippy -D warnings` przechodzą **przed** commitem, nie po.
- **Commit jest jednostką recenzji.** Nie ma pull requesta, który by ją niósł, więc
  komunikat commita musi tłumaczyć zmianę w całości: co, dlaczego i co z tego wynika
  dla faz następnych. Krótki komunikat jest tu brakiem, nie zwięzłością.
- **Podfaza = jeden commit**, razem z poprawkami, które wymusiła w dokumentach faz
  następnych (`K-18`). Rozdzielanie ich łamałoby tamtą regułę.
- Praca, która może nie wejść, zostaje w katalogu roboczym albo w `git stash` — nie
  w gałęzi. Jeśli eksperyment przeżyje sesję, gałąź jest dopuszczalna, ale kasuje się ją
  natychmiast po scaleniu.

## Reguła: poprawki wędrują w przód

`K-18` w dokumencie 00. Praca nad fazą X regularnie pokazuje, że plan fazy **następnej**
jest w czymś nieprawdziwy: kontrakt wygląda inaczej, API nazywa się inaczej, kryterium
jest niemierzalne, kolejność pakietów niewykonalna, albo pakiet obiecuje coś, czego nikt
nie jest właścicielem.

**Poprawiamy dokument tej fazy od razu, w tej samej zmianie.** Nie „jak dojdziemy do Y".

1. Poprawka idzie do dokumentu, którego dotyczy. Dziennik `00-postep.md` odnotowuje
   tylko, że powstała — nie jest jej miejscem przechowywania.
2. Każdy dokument zbiera je w tabeli **„Zmiany wpisane po MX"** na swoim końcu, w tym
   samym formacie co tabele korekt (`D-n`, gwiazdka = zmiana zakresu albo kryterium).
3. Wpisujemy **tylko to, co wiemy na pewno**. Faza następna nie jest przy okazji
   przeprojektowywana — od tego jest jej własny start.
4. Poprawka wymagająca decyzji, której nie umiemy teraz podjąć, ląduje w §9.2 dokumentu
   **fazy** jako decyzja otwarta z propozycją domyślną. Nigdy jako `TODO` w kodzie.
5. Zmiana dotykająca kontraktu z `00-konwencje-i-kontrakty.md` nadal wymaga wpisu `K-n`
   w §4a — K-18 tego nie zastępuje.

Powód jest ten sam co przy regule „popraw plan, zanim napiszesz kod": rozjazd planu
z rzeczywistością jest gorszy od braku planu, bo nikt go nie widzi. Wiedzę ma ten, kto ją
właśnie zdobył — za miesiąc nie będzie jej miał nikt.

## Reguła: odhaczanie postępu

Po zakończeniu pracy nad zadaniem zaktualizuj `docs/implementation-plan/00-postep.md`:

1. Odhacz pakiet roboczy w sekcji 4 dokumentu jego fazy (`MX-*.md`).
2. Odhacz bramkę w `00-postep.md`, jeśli ta praca ją domknęła.
3. Dopisz linię do dziennika na dole `00-postep.md`: data, faza, co zamknięto.

Zasady:

- **Odhaczaj tylko to, co zweryfikowane.** Kryterium ukończenia pakietu jest wypisane w jego
  wierszu w §4 — jeśli nie zostało spełnione, zadanie jest `[~]` w toku, nie `[x]`.
  „Kod napisany" nie jest kryterium; „test przechodzi" jest.
- Nie odhaczaj bramki fazy, dopóki nie przejdą wszystkie jej pakiety robocze.
- Jeśli praca ujawniła, że plan fazy jest błędny — popraw plan i odnotuj to w dzienniku,
  zamiast odhaczać zadanie, które opisuje coś innego niż zrobiono.

## Język i lokalizacja

**Angielski:** kod, identyfikatory, nazwy plików i katalogów, klucze w `data/`, nazwy gałęzi,
komunikaty commitów. Bez wyjątków — również w nowych fazach.

**Polski:** dokumentacja planu (`docs/implementation-plan/`) i komentarze domenowe wyjaśniające
regułę biznesową.

**Każdy tekst widoczny dla gracza powstaje w obu wersjach w tej samej zmianie:**

- Tekst w UI to zawsze `LocKey` + wpis w `data/locale/pl.ron` **i** `data/locale/en.ron`.
  Literał tekstowy w kodzie UI to błąd, nie skrót.
- Zakaz `TODO: translate` i zakaz wersji angielskiej dopisywanej „później" — później znaczy nigdy,
  a brak wychodzi dopiero przy zmianie języka, czyli u gracza.
- Pluralizacja przez `plural(locale, n)`: polski ma trzy formy (1 · 2–4 · 5+), angielski dwie.
  Klucz liczebnikowy musi nieść **wszystkie** formy dla obu języków, nie tylko te, które akurat
  widać na ekranie.
- Test CI: zbiory kluczy `pl` i `en` muszą być identyczne. Brakujący klucz łamie build —
  nigdy cichy fallback na drugi język, bo wtedy nikt się nie dowie.
- Wyjątek: nazwy generowane proceduralnie (imiona, nazwiska, ulice, firmy) pochodzą
  z `data/names/` per region i **nie są lokalizacją UI** — nie tłumaczy się ich.

Spójność między fazami: faza dokładająca teksty używa istniejącej przestrzeni kluczy i konwencji
nazewniczej `engine/ui`, nie zakłada własnej.

## Dobre praktyki

Kolejność rozstrzygania jest istotna, bo te zasady potrafią wskazywać w przeciwne strony:
**YAGNI → DRY → SOLID**.

- **YAGNI ma pierwszeństwo.** Brak drugiego konsumenta = brak abstrakcji. Żadnego traitu z jedną
  implementacją, fabryki dla jednego produktu, konfiguracji dla wartości, która nigdy się nie zmienia.
  Wyjątkiem są jawnie zaplanowane punkty wymiany opisane w kontraktach faz (`Wholesale`,
  `TaxEngine`, `TravelOracle`) — tam drugi konsument jest znany z nazwy i numeru fazy.
- **DRY dotyczy wiedzy, nie podobnie wyglądającego kodu.** Dwie funkcje o zbliżonym kształcie,
  które zmieniają się z różnych powodów, mają zostać osobno. Natomiast jedna reguła domenowa ma
  mieć jedną implementację — to już wymuszają kontrakty: jeden silnik reguł dla gracza i AI
  (`K-11`), jeden rdzeń ekonomii dla mezo i makro (`kernel`), jeden słownik domenowy
  w `engine/core` (`K-8`), jedno źródło prawdy o terenie (`K-13`).
- **SOLID w wydaniu rustowym**, gdy abstrakcja już jest uzasadniona:
  - *S* — system ECS ma jeden powód do zmiany; jeśli deklaruje dostęp do połowy komponentów świata,
    jest źle podzielony.
  - *O* — rozszerzanie przez dane (`data/*.ron`) i nowe warianty enuma, nie przez dopisywanie
    gałęzi do rosnącego `match` w cudzym module.
  - *L* — implementacja nie osłabia kontraktu traitu. Tu ma to twarde znaczenie: mezo-odpowiednik
    systemu musi zwracać ten sam koszt pieniężny co mikro (`K-5`), inaczej łamie podstawienie
    w sposób, którego typ nie wyłapie.
  - *I* — trait ma być wąski po stronie konsumenta. Jeśli faza używa 2 z 20 metod, to sygnał do
    podziału traitu, a nie do implementowania osiemnastu zaślepek.
  - *D* — zależność od traitu tam, gdzie realnie istnieją dwie implementacje (produkcyjna i testowa
    też się liczy), nie „na zapas".
- Skrót świadomy oznaczamy komentarzem `ponytail:` z nazwaniem sufitu i ścieżki wyjścia —
  prosty kod ma się czytać jako decyzja, nie jako niewiedza.

## Zasady techniczne

- Zmiana w kontrakcie z `00-konwencje-i-kontrakty.md` wymaga wpisu `K-n` w sekcji 4a tego
  dokumentu — nie rozstrzyga się jej lokalnie w fazie.
- Pieniądz to `i64` w groszach. Zakaz `f32`/`f64` w księgowości, podatkach i stanach magazynowych.
- Zakaz `f64::ln`/`exp`/`powf` w kodzie symulacji — jest `core::det_math` (`K-6`).
- Zakaz iterowania po `HashMap`/`HashSet` w kodzie symulacji (determinizm, `00` §3.2).
- Nietrywialna logika zostawia po sobie uruchamialny test. Kryterium ukończenia to przechodzący
  test, nie napisany kod.
