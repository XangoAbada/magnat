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

## Reguła: przegląd strukturalny po zamkniętym pakiecie

Przed commitem zamykającym pakiet roboczy sprawdź pliki, które ta praca zmieniła, pod cztery
progi (`python scripts/struct_guard.py --changed`): plik 800/1200 linii kodu, blok `impl` 300/500,
funkcja 150/250, `mod.rs` z własnym kodem 300/600.

Przekroczony próg nie jest błędem i nie blokuje commita. Jest pytaniem, na które trzeba
odpowiedzieć w jeden z trzech sposobów:

1. **Podziel teraz**, jeśli podział jest mechaniczny (przeniesienie symboli bez zmiany
   zachowania) i mieści się w tym samym commicie.
2. **Zaplanuj podział**, jeśli wymaga decyzji albo dotyka determinizmu — wiersz w rejestrze
   długu strukturalnego w `R1-refaktor-po-M5.md`, z powodem.
3. **Zostaw świadomie**, jeśli plik jest długi, bo jeden algorytm jest długi — komentarz
   `ponytail:` nazywający sufit, plus wpis na liście wyjątków w `R1-refaktor-po-M5.md` §5
   i w `WYJATKI_PLIK` w skrypcie.

Czego nie wolno: zostawić bez odpowiedzi i zostawić `TODO` w kodzie (`K-18` pkt 4).
Dzieli się pliki, w których są dwa tematy, a nie pliki, które są długie.

Powód, dla którego ta reguła w ogóle jest: przez sześć faz kryterium ukończenia pakietu brzmiało
„test przechodzi" i to jest właściwe kryterium — ale przechodzący test nie odróżnia czterystu
linii dopisanych do modułu od czterystu linii dopisanych do worka. Pomiar jest w `R1` §1.

## Reguła: buildy testowe się sprząta

Katalog `target/` urósł do **106 GB**, bo każda podfaza zostawiała po sobie pełne artefakty,
a część przebiegów szła do własnych katalogów (`target/m10f`, `target/tests`). Nikt tego nie
widział, bo `target/` jest w `.gitignore` — brak w gicie znaczy „poza zasięgiem wzroku", nie
„nie istnieje".

- **Jeden katalog artefaktów: domyślny `target/`.** Nie ustawiamy `CARGO_TARGET_DIR` ani
  `--target-dir` per faza, per podfaza, per eksperyment. Katalog nazwany od podfazy nikomu
  nie przypomni o sobie, gdy podfaza się skończy.
- **Po zamkniętej podfazie — `cargo clean`**, w tym samym kroku co commit zamykający. Wyjątek:
  zostaje, jeśli następna podfaza rusza od razu i czekanie na pełny rebuild kosztuje więcej
  niż miejsce.
- **Eksperyment, który chodził we własnym katalogu, kasuje się razem z eksperymentem.**
  Ta sama zasada co przy gałęziach: praca, która może nie wejść, nie zostawia po sobie śmieci
  przeżywających sesję.
- **Profil `release` buduje się tylko wtedy, gdy mierzymy wydajność.** Testy i `clippy` chodzą
  na `debug`; `release` obok `debug` to drugie 12 GB za nic.
- Przy sprzątaniu warto najpierw zobaczyć, co zajmuje miejsce (`du -sh target/*` w Git Bash),
  a potem skasować punktowo: `cargo clean -p <crate>` albo `cargo clean --release`.

Powód jest ten sam co przy regule o przeglądzie strukturalnym: przechodzący test nie odróżnia
projektu od wysypiska. Miejsce na dysku jest zasobem tej samej klasy co kontekst i czas —
kończy się nagle i zawsze w najgorszym momencie.

## Reguła: subagenci oszczędzają kontekst

**Pracę, której wynikiem jest wniosek, a nie treść plików, oddajemy subagentowi** (`Task`).
Główna sesja ma trzymać kontrakty, plan fazy i pisany kod — nie surowe wyniki przeszukiwań.

Do subagenta idzie:

- rozpoznanie w kodzie („gdzie liczony jest podatek", „kto woła `Wholesale::quote`") — wraca
  lista `plik:linia` z jednym zdaniem, nie wklejone pliki;
- przeglądy i audyty wielu plików: recenzja przed commitem, sprawdzenie zgodności z kontraktem,
  szukanie literałów tekstowych w UI, `TODO` w kodzie;
- zadania masowe i mechaniczne: zmiana nazwy w wielu plikach, dopisanie tego samego wzorca,
  generowanie powtarzalnych testów i danych;
- czytanie długich dokumentów planu **nie swojej** fazy, gdy potrzebny jest jeden fakt.

Zostaje w głównej sesji:

- dokument `00-konwencje-i-kontrakty.md` i dokument robionej właśnie (pod)fazy — te czyta się
  w całości samemu, bo są kontraktem, a nie materiałem do streszczenia;
- decyzje dotykające kontraktów, determinizmu i `K-n`;
- kod pisany w ramach pakietu roboczego, jego testy i commit.

Zasady:

- **Zleca się wniosek, nie przeszukanie.** Prompt mówi, czego szukamy i w jakiej formie ma wrócić
  odpowiedź. Subagent zwracający dwieście linii kodu nie oszczędził niczego.
- **Niezależne zlecenia idą równolegle** — kilka wywołań w jednej wiadomości.
- Wynik subagenta jest raportem, nie prawdą. Zanim wejdzie do kodu albo do planu, sprawdzamy
  wskazane miejsce — zwłaszcza gdy dotyczy determinizmu albo księgowości.
- Subagent nie odhacza postępu i nie commituje. `00-postep.md` i dziennik prowadzi główna sesja.

Powód: kontekst główny jest zasobem tej samej klasy co czas — wypełniony wynikami `grep`
przestaje mieścić kontrakt fazy, a wtedy błędy zaczynają wyglądać jak niewiedza.

## Reguła: podsumowanie po commicie mówi po ludzku

Podsumowanie, które piszesz po ostatnim commicie podfazy, czyta człowiek — nie recenzent
i nie następna sesja. Ma tłumaczyć, **co się zmieniło i dlaczego to ma znaczenie**, prostymi
zdaniami.

- Krótkie zdania, konkrety, strona czynna. „Piekarnia zużywa teraz wodę i mąkę, a gdy ich
  zabraknie — przestaje piec", nie „zaimplementowano mechanizm konsumpcji surowców
  z obsługą niedoborów".
- Żargonu używaj tylko wtedy, gdy nazywa rzecz, której nie da się nazwać inaczej
  (nazwa typu, systemu, pliku). Nie ozdabiaj nim zdań.
- Bez marketingu i bez nadęcia: żadnych „kompleksowy", „solidny", „w pełni zintegrowany",
  „znacząco usprawnia", żadnych emoji i list z pogrubionymi hasłami bez treści.
- Powiedz też, czego **nie** zrobiono i co z tego wynika dla następnej podfazy — to jest
  najważniejsza część, a zwykle wypada pierwsza.
- Liczby zamiast przymiotników: ile testów przechodzi, ile plików, jaki jest sufit.

Ta reguła dotyczy tekstu do użytkownika, nie komunikatu commita — commit nadal jest
jednostką recenzji i opisuje zmianę technicznie.

## Język i lokalizacja

**Angielski:** wszystko, co **przekracza granicę crate'u** — typy, funkcje, pola i warianty
enumów publiczne (`pub` bez zawężenia), klucze w `data/`, nazwy plików i katalogów, nazwy gałęzi,
komunikaty commitów. Tam bez wyjątków, również w nowych fazach.

**Polski:** dokumentacja planu (`docs/implementation-plan/`), komentarze domenowe wyjaśniające
regułę biznesową, **nazwy prywatne wewnątrz modułu** (`zbierz_fakty`, `rozstrzygnij`, `Slownik`,
także `pub(crate)` i `pub(super)`) oraz **komunikaty deweloperskie** — `panic!`, `expect`,
`assert!`, `eprintln!`, `debug_assert`. Jedno i drugie czyta ten sam człowiek i w tym samym
zdaniu co komentarz nad nim, więc idzie w języku tego komentarza.

**Nazwa polska pisze się bez diakrytyków:** `zbierz_fakty`, nie `zbierz_faktę`. Powód jest
praktyczny — identyfikator czyta się w terminalu, w diffie i w komunikacie kompilatora, a nie
tylko w edytorze. Pilnuje tego `scripts/lang_guard.py` w CI; przy wpisaniu tej reguły bramka
znalazła siedem nazw, które ją łamały, i żadna nie była widoczna dla niczego innego.

To brzmienie powstało w `R2e` (`D-N19`) i **opisuje kod, który jest**: publiczne API było
angielskie od M0, prywatne nazwy polskie od M5, a komunikatów deweloperskich po polsku było 550.
Do R2e była to druga konwencja, niezapisana; wybór padł na zapisanie jej zamiast przemianowania
kilkuset symboli i komunikatów bez zmiany zachowania. Granica jest twarda i przebiega tam, gdzie
nazwę widzi ktoś spoza crate'u.

**Czego bramka nie sprawdza:** czy nazwa publiczna jest angielska. Tego nie da się sprawdzić
tanio — wykrywacz słownikowy przepuszcza każde słowo, którego w słowniku nie ma (podłożone
`pub fn pomnoz_ulamek` przeszło). `lang_guard --list` robi skan morfemowy jako pomoc do
przeglądu, z nazwanym sufitem, i nie ma kodu błędu. Granicy pilnuje recenzja commita.

**Każdy tekst widoczny dla gracza powstaje w obu wersjach w tej samej zmianie:**

- Tekst w UI to zawsze `LocKey` + wpis w `data/locale/pl.ron` **i** `data/locale/en.ron`.
  Literał tekstowy w kodzie UI to błąd, nie skrót.
- Zakaz `TODO: translate` i zakaz wersji angielskiej dopisywanej „później" — później znaczy nigdy,
  a brak wychodzi dopiero przy zmianie języka, czyli u gracza.
- Pluralizacja przez `plural(locale, n)`: polski ma **cztery** formy (1 · 2–4 · 5+ · ułamek),
  angielski dwie. Czwarta jest CLDR-owym `other` i dotyczy wartości ułamkowych — „1,5 sklepu",
  a nie „1,5 sklep" ani „1,5 sklepy" (`DE-8` w M9b). Klucz liczebnikowy musi nieść **wszystkie**
  formy dla obu języków, nie tylko te, które akurat widać na ekranie; wpis o innej liczbie form
  łamie wczytanie katalogu (`LocError::PluralArity`), a nie gubi formę po cichu.
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
