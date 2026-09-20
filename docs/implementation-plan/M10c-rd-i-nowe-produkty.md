# M10c — R&D i nowe produkty

Podfaza 3 z 6 fazy **M10 — Głębia** (`M10-glebia.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M6 (receptury), M7 (firmy), M8 (`EpochClock`). |
| **Pakiety robocze** | WP10.8, WP10.9 |
| **Projekt techniczny** | §5.4 |
| **Wynik do pokazania** | Technologia odblokowuje kategorię towaru, patent daje wyłączność, licencja ją sprzedaje. |
| **Kryterium zamknięcia** | Kryteria WP10.8 i WP10.9; fala C katalogu towarów w repo i zielona w walidatorze. |
| **Poprzednia / następna** | `M10b-marka-i-media.md` · `M10d-gielda-przejecia-ubezpieczenia.md` |

`TechTree`, punkty badawcze, patenty i licencje oraz nowe kategorie produktów wraz ze zmianą struktury potrzeb w czasie.

---

## Pakiety robocze

### `[x]` WP10.8 — R&D: `TechTree`, punkty badawcze, `Patent`, licencje

**Zależności:** M7 (dział, stanowiska, budżet), dane `data/tech/`.
**Kryterium ukończenia:** firma z 4 badaczami (umiejętność 60) i budżetem 50 tys./mies. odkrywa węzeł
o koszcie 1200 RP w 9 ± 1 miesiącu gry, deterministycznie. Odkrycie przed rokiem „światowym" daje patent;
po roku „światowym" węzeł jest dostępny bez patentu po obniżonym koszcie. Licencja jest `ContractId`
z M6, nie nowym mechanizmem. Walidator grafu `data/tech/` w CI (każdy prereq istnieje, brak cykli,
każdy `NewGood` istnieje w `data/goods/`).
**Rozmiar: L.**
**Wykonane.** Tempo trafia w kryterium co do doby: czterech badaczy o umiejętności 60 zbiera 1200 RP w **dobie 254**, czyli w dziewiątym miesiącu (`sim/firms/tests/rnd.rs`). Patent, zniżka za rok „światowy” i licencja jako `ContractId` mają po teście; walidator drzewa chodzi w CI zwykłym `cargo test` (`sim/firms/tests/tech_graph.rs`, siedem przypadków, w tym trzy negatywne).

---

### `[x]` WP10.9 — Nowe kategorie produktów i zmiana potrzeb

**Zależności:** WP10.8, M8 (`sim/events`), M3 (potrzeby).
**Kryterium ukończenia:** odblokowanie kategorii „telefon komórkowy" w roku epoki rzeczywiście zmienia
koszyk potrzeby „kontakt społeczny" i tworzy nowy łańcuch dostaw (walidator grafu towarów zielony
po zmianie). Mieszkańcy z wysoką otwartością kupują pierwsi — mierzalne w rozkładzie.
**Rozmiar: M.**
**Wykonane z jedną korektą pomiaru** (`FD-6`): efekt otwartości mierzy się na **ocenie decyzji**, a nie na liczbie zakupów — w świecie testowym o zakupie rozstrzyga najpierw próg budżetu, który jest twardy, więc liczba kupujących skacze z zera na sto przy jednej wartości i nie pokazuje niczego o osobowości. Premia za nowość rośnie monotonicznie dziesiątkami otwartości i wygasa po roku gry (`sim/economy/tests/rnd_new_product.rs`).

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.4 R&D, technologie, epoki

```rust
pub struct TechTree { pub nodes: Vec<TechNode>, pub by_branch: Vec<Vec<TechId>> }

pub struct TechNode {
    pub id: TechId,
    pub branch: BranchId,
    pub prereqs: SmallVec<[TechId; 4]>,
    pub cost_rp: u32,                 // punkty badawcze, milipunkty w akumulacji
    pub world_year: i32,              // rok, od którego technologia jest „światowa"
    pub effects: SmallVec<[TechEffect; 4]>,
}

pub enum TechEffect {
    QualityBonus   { good: GoodId, delta_q: i8 },
    CostReduction  { recipe: RecipeId, bps: u16 },
    NewRecipe      (RecipeId),
    NewGood        (GoodId),           // §11.3 — nowa kategoria, zmienia koszyk potrzeb
    MachineUpgrade { machine: MachineId, throughput_bps: u16, failure_bps: i16 },
    ProductFeature { good: GoodId, feature: FeatureId },
}

pub struct Patent {
    pub tech: TechId,
    pub owner: FirmId,
    pub granted: SimMinute,
    pub expires: SimMinute,            // 20 lat gry
    pub license_policy: LicensePolicy, // Zamknięty | Otwarty{opłata} | Wybiórczy{lista}
}
```

**Postęp:** `rp_per_day = Σ_badacz f(umiejętność, energia, nastrój) × jakość_laboratorium ×
min(1, budżet_materiałowy / próg)` — wszystko całkowitoliczbowe, milipunkty. Akumulacja na wybranym
węźle. Element losowy jest **jeden**: przełom (`StreamId::RnDBreakthrough`) skraca pozostały koszt
o 10–40% z małym prawdopodobieństwem dziennym. Brak losowego „nie udało się" — frustrujące i nieczytelne.

**Trzy tryby dostępu do technologii:**
1. **Odkrycie przed `world_year`** → patent na 20 lat. Przewaga i źródło przychodu z licencji.
2. **Odkrycie po `world_year`** → bez patentu, koszt RP obniżony o 60% (wiedza jest w obiegu).
3. **Licencja** → `ContractId` z M6 (opłata wstępna + royalty w bps od przychodu z produktu).
   Zero nowego mechanizmu umów.

**Epoki (§11.3).** `data/epochs/*.ron` (istniejący katalog z dok. 00 §5) mapuje rok → zbiór
`world_year` dla węzłów. Nowa kategoria produktu (`NewGood`) uruchamia migrację koszyka potrzeb:
towar dopisuje się do koszyka wskazanej potrzeby z wagą początkową i **przesuwa wagi substytutów**.
To jest jedyne miejsce, w którym dane symulacji zmieniają się w trakcie partii — musi przejść przez
walidator grafu towarów (dok. 00 §5), inaczej odrzucamy zmianę i logujemy błąd danych.

**Nowa kategoria oznacza nowy łańcuch.** `NewGood` bez receptury i bez importu to błąd danych
wykrywany w CI, nie w runtime.


---

## Zmiany wpisane po M10c

Zgodnie z `K-18`. Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| FD-1 ★ | **`TechEffect::NewRecipe` nie powstaje.** Efektów jest trzy, nie sześć: `NewGood`, `CostReduction` i `MachineUpgrade`, a ciągły wpływ na jakość i produktywność niesie osobne pole węzła `gain`, które podnosi `Site.tech` | Recepturę uruchamia linia, linia stoi w zakładzie, zakład wymaga archetypu budynku z `data/buildings/` — a archetypy stawia generator M2, który o technologii nie wie i wiedzieć nie ma. Wariant bez wyzwalacza przechodzi każdy test i wygląda tak samo jak działający (`K-67`). Adres: **M10f**, razem z własną montownią telefonów |
| FD-2 ★ | **`TechEffect::QualityBonus { good, delta_q }` nie powstaje**, a jego miejsce zajmuje `TechNode::gain` podnoszący `Site.tech` | Model jakości M6 (`QualityModel`) ma już wejście `w_tech`, a `PlantSite::tech` nosi komentarz „M10 podmieni je drzewem technologii, nie zmieniając kształtu”. Drugie wejście obok niego byłoby drugą prawdą o tej samej rzeczy, a przy okazji nowym wyszukiwaniem po `GoodId` w gorącej ścieżce produkcji. Przy okazji domyka to `FC-2`: `MacroFirm.tech` wychodzi z `Site.tech`, więc pole przestaje być zerem **bez ani jednej zmiany w `sim/macro`** |
| FD-3 ★ | **`TechEffect::ProductFeature { good, feature }` i `FeatureId` nie powstają** | `FeatureId` nie istnieje nigdzie w repozytorium i nie ma czytelnika: cecha produktu bez miejsca w karcie towaru, w decyzji zakupowej i w wycenie jest wariantem, którego skutku nikt nie zobaczy. `FC-5` mówi zresztą wprost, że cecha z R&D ma się pokazać w karcie **towaru albo firmy** — a tych kart nie ma. Adres: **M10f** |
| FD-4 ★ | **`CostReduction` obniża zużycie mediów szarży, a nie masę wsadu.** Nowe pole `PlantSite::utility_bonus_bp` (0 = bez zmian, sufit 5000), pisane przez R&D, czytane przez rozliczenie szarży | Obniżka masy wejściowej przy niezmienionym wyjściu **tworzyłaby masę z niczego** i łamała test własnościowy z 00 §6. Prąd i woda są jedynymi pozycjami kosztu szarży, które wolno obniżyć bez ruszania bilansu — i obniżają zarówno koszt własny wyrobu, jak i rachunek za media. `ponytail:` mnożnik jest per zakład, a `CostReduction` mówi o recepturze: zakład prowadzący dwie receptury dostaje zniżkę na obie |
| FD-5 ★ | **Telefon komórkowy jest towarem importowanym, bez receptury krajowej.** `cons_mobile_phone` ma `external_base_price` i nie ma wpisu w `data/recipes/` | To jest zgodne z regułą z §5.4 („`NewGood` bez receptury **i** bez importu to błąd danych”): jedno źródło wystarczy, a pierwsze telefony w każdym mieście przyjeżdżały zza granicy. Własna montownia wymaga archetypu budynku i typu zakładu, czyli zmiany w generatorze M2 — a wtedy generator stawiałby ją **przed** odkryciem technologii. Adres: **M10f**, razem z `FD-1` |
| FD-6 ★ | **„Mieszkańcy z wysoką otwartością kupują pierwsi" mierzy się na ocenie decyzji, nie na liczbie zakupów** | W świecie testowym o zakupie rozstrzyga najpierw próg budżetu i jest on **twardy**: przy budżecie 300 gr kupuje zero osób, przy 320 gr — sto. Liczba kupujących nie niesie żadnej informacji o osobowości, bo osobowość działa na użyteczność, a nie na stać/nie stać. Człon nowości wchodzi do oceny z wagą otwartości, więc premia „towar nowy minus ten sam towar po roku" jest zerem dla najzamkniętszej dziesiątki i rośnie monotonicznie do najotwartszej. Test ma kontrolę negatywną: po roku gry premia znika, a towar dalej stoi na półce |
| FD-7 ★ | **Nowość towaru jest osobnym powodem od nowości miejsca.** `Candidate` dostaje pole `fresh`, a `Market` — tablicę `fresh_goods` (towar → tick, do którego jest nowy), wpisywaną przy wypuszczeniu towaru i trwającą **rok gry** | Do M10c człon nowości dotyczył wyłącznie sklepu, w którym mieszkaniec nie był. Pierwszy telefon komórkowy jest nowy także w sklepie, do którego chodzi od lat — i to jest cała treść „nowej kategorii produktu”. Rok, a nie miesiąc: telefon kupuje się raz na kilka lat, więc miesięczne okno minęłoby, zanim ktokolwiek wyszedłby po niego do sklepu |
| FD-8 | **Rola `researcher` dopisana na końcu `data/jobs/roles.ron`** (47 ról), a stanowisko badacza dostały 53 typy zakładów przetwórczych i wytwórczych o co najmniej pięciu wierszach obsady | Kryterium WP10.8 mówi „firma z 4 badaczami", a w katalogu nie było roli badacza wcale — najbliższy `lab_technician` to kontrola jakości, nie badania. Pierwsza wersja dała stanowisko sześciu rzadkim typom zakładów i w mieście 1500 mieszkańców **nie było ani jednego badacza**, czyli cały mechanizm był martwy poza testami. Kolejność w pliku jest kontraktem zapisu gry (`K-43`), więc rola idzie wyłącznie na koniec |
| FD-9 | **Dedykowanego laboratorium jako typu zakładu nie ma.** Badania prowadzi się przy zakładzie, a jakość laboratorium jest jego `Site.tech` | Nowy typ zakładu wymaga archetypu budynku, wag generatora i godzin — czyli zmiany po stronie M2 — a kryterium mówi o czterech badaczach, nie o osobnym budynku. `ponytail:` sufit nazwany; adres wyjścia ten sam co przy `FD-1` |
| FD-10 | **Krok badań prowadzi `LaborSystem`, a nie własny system ECS.** Treść siedzi w `sim/economy::rnd::step_day`, reguły w `sim/firms::rnd` | Badania prowadzą ludzie, a doba rynku pracy jest jedynym miejscem, w którym naraz stoją wyjęty z ECS rejestr firm, tablica ról i świat z formą mieszkańców. Osobny system musiałby wyjąć rejestr drugi raz w tej samej dobie i dołożyć poziom harmonogramu — ta sama nauka, którą M6e zapisał przy `supply.Chain` (`AP-4`). Kolejność jest przy tym istotna i nazwana w kodzie: R&D jedzie **za** rynkiem pracy, bo tempo liczy się z dzisiejszej obsady |
| FD-11 | **Licencja mintuje `ContractId` z licznika `B2b`** (`magnat_supply::B2b::mint_contract_id`), a rejestr licencji trzyma `sim/firms` | Drugi licznik numerów umów dałby dwie umowy o tym samym numerze i `Subject::Contract` prowadziłby raz do dostawy, raz do licencji. Numer wydany tą drogą **nie ma wpisu** w rejestrze umów dostawy i to jest poprawny stan: rejestr prowadzi ten, kto umowę zawarł |
| FD-12 | **Stawka royalty nie jest losowana** — rośnie liniowo z tym, ile życia patentu jeszcze zostało | Losowanie zajęłoby numer `StreamId` na wieczność dla mechaniki, która ma sensowną regułę deterministyczną: patent z osiemnastoma latami ochrony jest wart górną krańcówkę widełek, wygasający za rok — dolną. Ta sama zasada, którą `K-67` zastosował do decyzji burmistrza |
| FD-13 | **`data/tuning/rnd.ron` trzyma `mrp_per_full_time_day: 1590`, a nie okrągłe 1500** | Przy 1500 węzeł za 1200 RP domyka się w dobie 271, czyli w **dziesiątym** miesiącu — mieści się w „9 ± 1", ale na krańcu przedziału. Przy 1590 wypada w dobie 254, w środku dziewiątego. Okrągła liczba na krawędzi kryterium jest gorsza od nieokrągłej w środku, a test asercją na dobę 254 zapali się, zanim tempo zdąży wypchnąć odkrycie poza przedział |
| FD-14 | **Przełom jest rzadki: 5 na dziesięć tysięcy na dobę** (mediana raz na kilkanaście lat badań) | Kryterium mówi „w 9 ± 1 miesiącu, deterministycznie", a rzut skracający pozostały koszt o 10–40 % potrafi z dziewięciu miesięcy zrobić siedem. Przy tej rzadkości przełom jest wydarzeniem w kronice, a nie składnikiem tempa — i test kalibracji wyłącza go jawnie, zamiast liczyć na to, że nie wypadnie |
| FD-15 | **Zmierzone w grającym mieście** (`m7miasto`, 4 km, 3000 mieszkańców z generatora → 23 183 po 300 dobach, 234 firmy): 12 węzłów w 5 gałęziach, **3 badaczy na etatach, 7 projektów w toku, średni opłacony budżet badań 428 ‰, 0 odkryć, 0 patentów**, telefon nadal poza obiegiem; najdalej zaawansowany projekt zebrał 17 % kosztu przez 300 dób | Mechanizm biegnie w prawdziwym mieście, ale **pełznie, i wiadomo dlaczego**: firmy są ubogie w gotówkę (przez 300 dób pieniądz przeszedł z ksiąg do ludzi, 398 → 221 mln zł wobec 3 → 179 mln zł), więc opłacają mniej niż połowę budżetu materiałowego, a tempo spada proporcjonalnie. Do tego miasto zaczyna w 1990, więc jedenaście z dwunastu węzłów jest już wiedzą powszechną (zniżka 60 %, patentu nie ma), a łańcucha elektronicznego (`float_glass` → `integrated_circuit` → `polymer_blends` → `mobile_telephony`) nie da się przejść trzema badaczami bez budżetu. **Przed naprawą `FD-19` te same 300 dób dawało dwa odkrycia i 90 % postępu u lidera — i to była miara błędu, nie gospodarki.** Czy rozkład jest właściwy, rozstrzyga przebieg balansatora z profilem gęstości badaczy i płynności firm; adres **M10f** |
| FD-16 | **Znalezione poza zakresem: job `struct-guard` w CI był czerwony od M9a i od M11c naraz.** `--self-test` sprawdzał obecność `tools/headless/src/retail.rs`, którego nie ma od `K-68` (most przeniósł się do `game/src/world/retail.rs`), a `--all` łapał dwa przekroczenia, których M11c nie zamroziło (`lsystem.rs` 605 → 609, `city/mod.rs` 368 → 370). Obie rzeczy naprawione w tej zmianie | Bramka, która świeci na czerwono niezależnie od zmiany, przestaje cokolwiek mówić — i dokładnie tak było przez trzy podfazy. Naprawa jest dwuliniowa po każdej stronie i nie jest zgodą na wzrost: pozycje rejestru długu i ich adresy zostają bez zmian |
| FD-18 ★ | **Royalty nalicza się osobnym przebiegiem po licencjach, a nie w pętli firm badawczych, i liczy się od utargu **wszystkich** zakładów licencjobiorcy z ostatniego domkniętego miesiąca** | Pierwsza wersja naliczała opłatę w tej samej pętli, która prowadzi badania — a ta przechodzi wyłącznie po firmach mających kogo posadzić w laboratorium. Licencjobiorca **z definicji** takiej obsady nie ma: kupił dostęp właśnie dlatego, że sam technologii nie zdobędzie. Stawka byłaby zapisana w umowie i nigdy niezapłacona, czyli `ChargeKind::Royalty` byłby wariantem bez producenta — dokładnie tym, przed czym broni `K-67`. Znalezione przeglądem przed commitem, nie testem. `ponytail:` utarg nie jest dzielony na wyroby, więc firma wytwarzająca dwie rzeczy płaci royalty od obu; sufit nazwany w kodzie, droga wyjścia prowadzi przez utarg per towar w `SitePnlMonth`, którego M7 nie ma |
| FD-19 ★ | **Recenzja przed commitem znalazła cztery rzeczy zmieniające wynik i wszystkie weszły.** (1) `budget_permille` liczył się z kwoty **żądanej**, a nie zapłaconej, więc firma, której przelew odrzucono — albo która nie miała konta rynkowego — dostawała **darmowe badania pełnym tempem**. (2) Podstawą royalty był `pnl.last()`, czyli ostatni wpis pierścienia, a lista płac wkłada tam wpis z zerowym utargiem w dniu wypłaty firmy — opłata licencyjna wychodziła systematycznie zerem. (3) „Zakład prowadzący projekt” był pierwszy na liście firmy, a przy royalty ten o najniższym `SiteId`: dwie reguły w jednym module, więc budżet i opłata tej samej firmy mogły obciążyć różne zakłady. (4) Zastępczy numer umowy w świecie bez łańcucha dostaw był **stały**, więc druga licencja nadpisywała pierwszą, choć opłata wstępna szła za obie | Żadnej z tych czterech nie złamał test i żadna nie złamała kompilacji. Dwie pierwsze są tej samej klasy co martwa ładowarka `brand.ron` z M10b: mechanizm istnieje, przechodzi, i po cichu nie robi tego, co obiecuje. Druga jest z nich groźniejsza, bo **testy stawiają `pnl` ręcznie**, więc royalty działało wszędzie poza grającym miastem. Numer umowy naprawia się teraz przez `Option`: świat bez rejestru umów **nie zawiera licencji wcale**, zamiast nadawać numer, który zderzy się z następnym |
| FD-20 | **Martwy kod usunięty w tej samej zmianie**: indeks „węzły tej gałęzi” (`by_branch`, `in_branch`, `branch_id`) i struktura raportu `RndReport`. `TechNode::world_year` i `Project::{started, breakthroughs}` **zostają** i dostały czytelnika w raporcie scenariusza | Indeks gałęzi budował się przy każdym ładowaniu dla nikogo, a raport liczył się w każdej dobie po to, żeby wylądować w koszu — obie rzeczy wróci dołożyć panel R&D razem z pierwszym czytelnikiem i obie są wtedy pętlą po dwunastu węzłach, nie mechanizmem. Pola, które zostały, opisują **stan w hashu**: usunięcie ich zmieniłoby hash, a nie usunięcie martwego kodu |
| FD-21 | **Opłata licencyjna trafia na konto licencjodawcy, ale nie do jego rachunku wyniku.** `Firms::post_revenue` ma jednego wołającego i `K-75` każe mu przy nim zostać, więc `SitePnlMonth::revenue` właściciela patentu nie rośnie | Pieniądz domyka się co do grosza i to jest niezmiennik, którego ta luka nie narusza. Narusza za to obraz firmy: wycena z marży (M10d) zobaczy licencje na koncie, a nie w wyniku, a podstawa royalty nigdy nie zawiera dochodu z innych licencji. Sufit nazwany w `books.rs`; droga wyjścia prowadzi przez drugie **wejście** do rachunku wyniku, a nie przez drugiego wołającego `post_revenue`. Adres: **M10d** (`FD-5` tamże) |
| FD-22 | **Koszt badań miesiąca może wpaść do rachunku wyniku miesiąca następnego.** Obciążenia wychodzą w dobie świata podzielnej przez 30, a `rnd_accrued` zeruje się w **dniu wypłaty firmy** | To jest ta sama właściwość, którą ma `hr_accrued` od M7b, i ta sama odpowiedź: suma za rok się zgadza, przypisanie do miesiąca bywa przesunięte o jeden. Wyrównanie wymagałoby wspólnej granicy miesiąca dla wszystkich firm, a ta jest rozsypana z rozmysłu — dziesięć tysięcy list płac tego samego dnia to szczyt, którego M7 unika |
| FD-23 | **Testy jednego pliku biegną równolegle i dwa z nich prosiły o to samo drzewo z tej samej ścieżki tymczasowej.** Test licencji przewracał się zależnie od kolejności wątków — sam przechodził, w komplecie nie | Znalezione dopiero pełnym przebiegiem `cargo test --workspace`, bo pojedynczy `--test rnd` też przechodził. Katalog jest od tej chwili unikatowy na wywołanie. Wniosek ogólniejszy niż ten test: **plik tymczasowy o nazwie wyprowadzonej z parametrów jest współdzielony przez wszystkie testy o tych samych parametrach** |
| FD-17 | **Znalezione poza zakresem, nienaprawione: dwa klucze lokalizacji stoją w `data/locale/` po dwa razy.** `ui.overlay.brand_awareness` (M10b) ma dwa **identyczne** wpisy, a `ui.legacy.new_dynasty` (M9e) dwa **różne** — „Zacznij nową dynastię w tym mieście” i „Zacznij od nowa, jako ktoś inny”. Widzi to wyłącznie człowiek: test CI sprawdza, czy **zbiory** kluczy `pl` i `en` są identyczne, a duplikat jest w obu plikach tak samo | Pierwszy jest nieszkodliwy i to on pokazał drugi. Drugi zmienia zdanie, które gracz widzi na ekranie spuścizny, a które z dwóch ma zostać — nie jest pytaniem, na które M10c ma odpowiedź. Adres: **R2**, razem z resztą rozjazdów danych i kodu |

---

## Co M10c zostawia następnym podfazom

Zgodnie z `K-18` — wpisane jest tylko to, co wiadomo na pewno.

1. **Gracz nie ma jak prowadzić badań.** Projekt wybiera reguła „najtańszy osiągalny
   węzeł", budżet wynika z kursu firmy, a licencję kupuje się automatycznie, kiedy
   cudzy patent blokuje najtańszy węzeł. Komenda gracza i panel należą do M10f
   (`DK-3`: `PanelId::Rnd` już istnieje, `is_reserved()` mówi o nim prawdę).
   Adres: **M10f**.
2. **Technologia nie ma karty.** `DecisionReason` renderuje węzeł jako `#N`, bo
   `engine/ui` nie widzi `data/tech/` — ta sama granica, którą `GoodId` ma od M6.
   Panel R&D trzyma drzewo i podmieni numer na nazwę. Adres: **M10f**.
3. **Patentów w typowym mieście nie ma.** Miasto startujące w 1990 zastaje jedenaście
   z dwunastu węzłów jako wiedzę powszechną, więc pierwszy patent wymaga przejścia
   łańcucha elektronicznego. Czy to jest właściwy rozkład, rozstrzyga przebieg
   balansatora — nie da się tego ocenić bez pomiaru gęstości badaczy.
   Adres: **M10f**.
4. **`MachineUpgrade` stosuje się raz, do linii stojących w chwili odkrycia.** Linia
   postawiona później nie dostanie ulepszenia z mocą wsteczną. Sufit nazwany
   w kodzie; droga wyjścia to przeliczanie mnożnika z listy znanych technologii
   przy starcie szarży. Adres: **M10f** albo **M12** (tryb 50×), zależnie od tego,
   kto pierwszy zmierzy, że to robi różnicę.
5. **Kategoria `Comms` ma jeden towar i jeden rodzaj sklepu.** Telefon stoi w sklepie
   specjalistycznym (`PlaceKind::Clothing`), bo to jedyne miejsce w generatorze,
   w którym towar sztukowy spoza spożywczaka ma gdzie stanąć. Własny `PlaceKind`
   dla elektroniki kosztuje archetyp budynku i wagi generatora — zmiana po stronie
   **M2**, nie M10.

---

## Zmiany wpisane po M10b

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M10b.
Gwiazdka = zmiana zakresu albo kryterium. Szczegóły — tabela `F-n` w `M10b-marka-i-media.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| FC-1 ★ | **Marka jest firmą i produkt jej nie ma** (`K-79`). `BrandId` to encja firmy, a `magnat_supply::brand_of` jest funkcją czystą bez rejestru. Nowy towar z R&D dziedziczy więc markę **wytwórcy**, a nie dostaje własnej | To jest rozstrzygnięcie M10b, nie przeoczenie: linia produktowa nie miała drugiego konsumenta, a rejestr marek byłby stanem do wpięcia w hash i przeprowadzenia przez zapis. **Jeśli M10c chce marek per produkt** — bo „nowa kategoria produktu" z §7.7 bywa osobną marką — to jest to decyzja otwarta z propozycją domyślną: **zostawić markę przy firmie**. Rozdzielenie kosztuje rejestr, `Batch.brand` przestaje być wyprowadzalne z producenta, a `firm_of(brand)` przestaje być bijekcją, więc karta mieszkańca traci odnośnik do firmy |
| FC-2 ★ | **`MacroFirm.tech` jest tym samym, czym był `brand_stock` do M10b: polem zawsze zerowym.** `lift()` wypełnia je `TechLevel(tech)` z rejestru firm, ale poziom technologii nikt nie podnosi, więc w każdym przebiegu jest to ta sama liczba | M10b pokazał, ile kosztuje takie pole: `brand_stock` stał w modelu od M7f, przechodził każdy test i wyglądał tak samo jak działający. Wzór domknięcia jest gotowy do przepisania — `sim/macro::brandseed` robi dokładnie dwie rzeczy, których `tech` potrzebuje: agregat przy `lift()` i wpływ przy `lower()` |
| FC-3 | **Blok `StreamId` M10: zajęte 280–284 (reklama i media) oraz 292–295 (makro).** Wolne dla M10c–M10e: **285–291 i 296–299** | `StreamId::RnDBreakthrough = 285` z tabeli M10 §7.1 jest nadal wolny i nadal zarezerwowany imiennie |
| FC-4 | **Blok `DecisionReason` M10: zajęte 800–803.** Wolne: **804–899** | Blok M9 (700–799) zostaje w całości wolny (`K-71`) |
| FC-5 | **Karta mieszkańca ma siedem zakładek, czyli sufit `MAX_CARD_TABS`.** Doszła „Marki" (`F-18`) | Ósma zakładka mieszkańca wymaga decyzji, którą z obecnych złożyć. Dla M10c to znaczy: cecha produktu z R&D pokazuje się w karcie **towaru albo firmy**, nie mieszkańca |
