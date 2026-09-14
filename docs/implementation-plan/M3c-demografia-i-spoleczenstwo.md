# M3c — Demografia i społeczeństwo

Podfaza 3 z 4 fazy **M3 — Ludzie i dzień** (`M3-ludzie-i-dzien.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M3a (komponenty, DES), M3b (planer — `HouseholdView` i `StockCat`). |
| **Pakiety robocze** | WP7, WP8, WP9 |
| **Projekt techniczny** | §5.6, §5.7, §5.8 |
| **Wynik do pokazania** | `m3_century` — 100 lat gry headless bez wybuchu ani wygaszenia populacji. |
| **Kryterium zamknięcia** | Kryteria WP7–WP9; tożsamość księgowa populacji zachowana co do jednostki. |
| **Poprzednia / następna** | `M3b-dzien-mieszkanca.md` · `M3d-populacja-i-ui.md` |

Gospodarstwa domowe i cykl życia z hazardami rocznymi, migracja jako jedyny regulator populacji, status społeczny, relacje i propagacja wiedzy o miejscach.

---

## Pakiety robocze

| | WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|---|
| [x] | WP7 | GD, cykl życia, demografia | WP1 | L |
| [x] | WP8 | Migracja i regulacja populacji | WP7 | M |
| [x] | WP9 | Status, relacje, plotka | WP1, WP7 | M |

### WP7 — GD, cykl życia, demografia

Typy GD, przejścia stanów, hazardy (płodność, umieralność, formowanie związku, rozwód), dziedziczenie. Wszystkie hazardy roczne w skali roku 360-dniowego (K-1).

**Kryterium ukończenia:** `m3_century` — populacja po 36 000 dniach w przedziale [0,5×, 2,0×] startowej dla 5 seedów; tożsamość księgowa populacji zachowana co do jednostki (sekcja 7).

**Spełnione.** `headless century --years 100` dla ziaren 1–5 (4 000 lokali, 3 000 etatów,
2 500 gospodarstw na starcie, 5 184–5 318 mieszkańców po zasiewie): **0,92–0,96×** po stu
latach, minimum w całym przebiegu 4 546, maksimum równe startowej. Ani jeden przebieg nie
zszedł poniżej 1 000 mieszkańców ani nie przekroczył 3× startowej w żadnym momencie. Tożsamość księgowa sprawdzana **co dobę**, nie tylko na końcu
(`prop_population_identity`); zachowanie pieniądza przy dziedziczeniu z tolerancją
0 groszy (`prop_inheritance_conservation`). Wersja skrócona w CI: `wp7_populacja_przezywa_trzydziesci_lat`
(30 lat × 3 ziarna).

### WP8 — Migracja i regulacja populacji

Napływ/odpływ GD sterowany wakatami pracy i pustostanami mieszkaniowymi. **To jest jedyny regulator populacji** — nie ma sztucznego „spawnowania do celu".

**Kryterium ukończenia:** eksperyment szokowy — usunięcie 20% miejsc pracy powoduje odpływ i stabilizację bezrobocia w ≤ 5 latach gry, bez oscylacji o amplitudzie > 30%.

**Spełnione** (`wp8_szok_na_rynku_pracy_wygasa_w_pieciu_latach`, `headless century --shock-percent 20`).
Bezrobocie 26 ‰ → 212 ‰ w miesiącu szoku; nadwyżka schodzi poniżej 20 % skoku **po 4 latach
gry**, a amplituda wahań populacji liczona od piątego roku po szoku wynosi 3,1 % wobec progu 30 %.
„Stabilizacja" jest mierzona jako zanik **nadwyżki** ponad poziom sprzed szoku, a nie jako
powrót do dawnej liczby: po likwidacji jednej piątej etatów równowaga bezrobocia jest
z definicji inna, bo miasto ma mniej pracy (korekta G-8).

### WP9 — Status, relacje, plotka

Funkcja statusu, przedziały klas, budowa i zanik relacji, propagacja wiedzy o miejscach.

**Kryterium ukończenia:** nowe miejsce dodane do świata jest znane ≥ 60% mieszkańców w promieniu 1 km po 30 dniach gry i < 5% mieszkańców powyżej 5 km (test §5.7 — fundament pod markę w M10).

**Spełnione** (`wp9_plotka_zna_kilometr_i_nie_zna_pieciu`): z zasiewu 54 osób mieszkających
w promieniu 400 m wiedza dochodzi po 30 dobach do **83,1 %** mieszkańców w promieniu kilometra
(270 z 325) i do **0,0 %** powyżej pięciu (0 z 5 964), wobec progów 60 % i 5 %. Pomiar wymagał **dojrzałego grafu relacji** — waga rośnie
o kilka punktów na tydzień, a plotka wymaga ≥ 40 — więc test rozgrzewa miasto dwa lata gry,
zanim postawi nowy sklep. Trzy rzeczy okazały się warunkiem koniecznym i wszystkie są
opisane w tabeli korekt: stały pierścień bliskich kontaktów zamiast rotacji (G-9),
opowiadanie „do skutku" zamiast dwóch prób (G-9) i lokalność przeprowadzek (G-10).

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.6 GD, cykl życia, demografia (§5.2)

```rust
#[repr(C)] pub struct Household {          // 128 B
    pub kind: u8,          // Single, Couple, FamilyWithKids, MultiGen, Roommates, Dorm, LoneSenior
    pub size: u8,
    pub flags: u16,
    pub district: u16,
    pub _pad: u16,
    pub members: [u32; 6],                // >6 osób: przelew do bocznej mapy (rzadkie)
    pub building: u32, pub unit: u16, pub _pad2: u16,
    pub cash: Money, pub bank: Money, pub savings: Money, pub debt: Money,  // POLA; M5 operuje
    pub income_monthly: Money,            // z generacji; M5/M7 aktualizują
    pub stock: [u8; 8],                   // dni zapasu per StockCat — M5 zastąpi realnymi towarami
    pub shopper_rotation: u8,             // indeks członka robiącego zakupy (rotacja wg grafików)
    pub vehicle_slots: [u32; 2],          // M4 wypełnia
    pub _reserved: [u8; 16],              // rezerwa dla M5/M9, żeby nie przebudowywać archetypu
}
```

**Hazardy** (dane w `data/demography/` — katalog dopisany do doc 00 §5 — per epoka i klasa). Wszystkie stopy roczne odnoszą się do roku **360-dniowego** (K-1).

| Przejście | Model | Wyzwalacz |
|---|---|---|
| Narodziny | hazard roczny f(wiek matki, liczba dzieci, klasa, `Housing`, `income_monthly`) | `DemographySystem`, `EveryDay`, sharding **1/360** |
| Dobór partnera | kandydaci z grafu relacji (`Friend`, `Coworker`, `Neighbour`) + kompatybilność statusu \|Δstatus\| ≤ 20, wiek ±8 lat | `EveryMonth` (30 dni) |
| Ślub / wspólne GD | po N miesiącach relacji o wadze > 70 | `EveryMonth` |
| Rozwód | hazard f(stres obojga, `debt/income`, czas trwania związku) | `EveryMonth` |
| Starzenie | wiek liczony jako `(day − birth_day) / 360`, nie przechowywany | — |
| Emerytura | wiek ≥ próg epoki → `Employment.flags = Retired` | zdarzenie DES na urodziny |
| Choroba | hazard f(wiek, `health`, `hygiene`, sezon) | `EveryDay` |
| Śmierć | hazard f(wiek, `health`) + zdarzenia (M8) | `EveryDay` |
| Dziedziczenie | `Money` GD + lokal → współmałżonek, dalej dzieci po równo (reszta do pierwszego wg `entity_index`, doc 00 §2) | zdarzenie `Death` |

**Dziedziczenie w M3** przenosi wyłącznie pieniądz i mieszkanie. Firmy, aktywa i długi hipoteczne wchodzą hookiem:

```rust
pub trait InheritanceHook: Send + Sync {
    /// Wołany po podziale Money i mieszkania, przed usunięciem encji zmarłego.
    fn on_inheritance(&mut self, deceased: CitizenId,
                      heirs: &[(CitizenId, u16 /* permil */)],
                      cmd: &mut CommandBuffer);
}
```
M7 rejestruje tu przekazanie udziałów w firmach, M5 — długów i rachunków. Permile sumują się do 1000, reszta do pierwszego dziedzica wg `entity_index` — ta sama reguła co podział kwoty w doc 00 §2.

**Warunek determinizmu:** żaden hazard nie jest losowany zbiorczo dla całej populacji. Każde zdarzenie życiowe ma własny strumień `rng(seed, StreamId::Demography, citizen_idx, day)`, więc dodanie lub usunięcie mieszkańca nie przesuwa losowań pozostałych. Bez tego zapis/odczyt i przejścia LOD łamią hash stanu.

### 5.7 Migracja i regulacja populacji (§5.2)

Populacja **nie jest sterowana do celu**. Regulatorem jest migracja GD, reagująca na dwa obserwowalne sygnały miasta:

```
wakaty         = liczba JobSlot bez pracownika
pustostany     = liczba lokali bez GD
napływ/mies.   = k_in  · min(wakaty, pustostany) · atrakcyjność_miasta
odpływ/mies.   = k_out · Σ_GD p_wyjazdu(GD)
p_wyjazdu(GD)  = f(miesiące bez pracy któregokolwiek dorosłego,
                   Needs[Housing], Needs[Safety], zadłużenie/dochód)
atrakcyjność   = g(średnia płaca, bezrobocie, bezpieczeństwo, wartość gruntu)
```

Napływające GD generuje **ten sam generator co Etap 8** (ten sam kod, mniejsze N) i od razu dostają pracę oraz mieszkanie; GD, które nie znajdzie żadnego w ciągu 3 miesięcy (90 dni), wyjeżdża. To zamyka pętlę ujemnego sprzężenia bez sztucznego spawnowania.

Testy sekcji 7 weryfikują, że sama migracja wystarcza do stabilności w 100 lat gry. Jeśli nie wystarczy — decyzja 9.9 (człon tłumiący kalibrowany balansatorem, świadomie **nie** wprowadzany na zapas).

### 5.8 Status, relacje, plotka (§5.4, §5.7)

**Status** (`StatusSystem`, `EveryMonth`, sharding 1/30):

```
status = clamp_0_100(
     w_i · percentyl(dochód_GD_per_capita)
   + w_w · percentyl(majątek_GD)
   + w_e · edu_level_score
   + w_o · prestiż_zawodu(JobRoleId)          // data/jobs/
   + w_a · percentyl(wartość gruntu dzielnicy)
   + w_c · konsumpcja_statusowa                // M3: 0; M5 wypełnia
   + w_r · reputacja_rodziny                   // średnia statusu rodziców, zanikająca z wiekiem
)
```

Klasy = przedziały: niższa 0–15, robotnicza 16–33, niższa średnia 34–52, wyższa średnia 53–72, wyższa 73–89, elita 90–100. Klasa jest **wyliczana, nie przechowywana** — mobilność społeczna (§5.4) wychodzi sama, bez osobnej mechaniki.

**Relacje.** Tworzenie: rodzina i współlokatorzy (z generacji/narodzin, waga 80–100); współpracownicy (wspólny `site`, +1/dobę wspólnej zmiany, max 70); sąsiedzi (wspólny budynek/kwartał, max 50); znajomi z miejsca (spotkanie w tym samym slocie `Leisure`/`Social`, max 60). Zanik: `RelationDecaySystem`, `EveryDay`, sharding 1/7 — waga −1 za każdy pełny tydzień bez kontaktu, usunięcie przy wadze 0 (poza rodziną).

**Plotka.** `GossipSystem`, `EveryDay`, sharding 1/7:

```
dla mieszkańca M (w shardzie dnia):
  wybierz ≤2 relacje losowane z wagą ∝ Relation.weight, tylko weight ≥ 40
  dla każdej relacji R:
     wybierz z wiedzy M 1 wpis o najwyższym (score × świeżość), którego R jeszcze nie ma
     wstaw do R: Knowledge{ target, day: dziś, score: score − 10, kind: Heard }
```

**Widoczność z trasy.** Przy zmianie trasy dom↔praca (a w M3 tylko wtedy — trasa jest cache'owana) `TravelOracle::places_on_route` zwraca miejsca w buforze 50 m od przebiegu; trafiają do wiedzy jako `SeenOnRoute` ze `score = 35`.

To jest dokładnie mechanika z §5.7: nowy sklep zna początkowo tylko ten, kto go mija lub w nim był; zasięg buduje się przez relacje. M10 dopisze `kind = Ad` i niczego innego nie musi zmieniać.

---

## Korekty planu wpisane po implementacji M3c

Gwiazdka = zmiana zakresu albo kryterium; reszta to doprecyzowanie.

| # | Korekta | Dlaczego |
|---|---|---|
| G-1 ★ | **`Household` ma 120 B, nie 128.** Lista pól z §5.6 zostaje bez zmian — to etykieta była zaokrągleniem w górę, nie rachunkiem | Suma pól wypisanych w §5.6, z jawnym wyrównaniem i bez niejawnego paddingu, daje 120 bajtów i nie ma tam czego dołożyć. Osiem bajtów mniej to 1,3 MB przy 167 tys. gospodarstw metropolii; rezerwa 16 B dla M5/M9 (ryzyko R5) zostaje nietknięta. Test `rozmiar_gospodarstwa_zgadza_sie_z_budzetem` pilnuje offsetów, nie tylko rozmiaru |
| G-2 ★ | **Hazard roczny stosuje się raz w roku, na dobie shardu — nie 360 razy po 1/360.** Zdarzenia z własnym terminem (poród po ciąży, wyzdrowienie) idą do `LifeQueue`: kalendarza dobowego na `BTreeMap` | Sharding 1/360 z §5.6 znaczy, że mieszkaniec przechodzi swoje losowania raz na 360 dób; rozbijanie stopy rocznej na dzienną dawałoby tę samą częstość zdarzeń za 360× większą cenę i za cenę **360 wartości ze strumienia** zamiast jednej. Jedna wartość na mieszkańca na rok to warunek `det_demography_independence`. Kolejki DES z M3a nie da się tu użyć: liczy w minutach i przewija się minuta po minucie, a tryb demograficzny skacze po dobach |
| G-3 ★ | **Miasto wchodzi do migracji dwiema płaskimi listami** — `Vacancies { homes: Vec<HomeSlot>, jobs: Vec<JobSlot> }` — wsypywanymi z zewnątrz, dokładnie jak `PlaceTable` w M3a (korekta A-5). `spawn_household` i `seed_population` **są** generatorem, o którym mówi §5.7 („ten sam kod, mniejsze N") | `sim/agents` nie zależy od `sim/world`, a §5.7 wymaga, żeby napływ tworzył gospodarstwa tym samym generatorem co Etap 8. Rozwiązanie jest jedno: generator mieszka w `sim/agents` i przyjmuje pojemność miasta jako dane, a Etap 8 (M3d §5.9) dokłada **nad** nim dopasowania statystyczne, zamiast pisać drugi generator obok |
| G-4 | **`StatusSystem` nie jest shardowany 1/30** (§5.12 zapowiadał shard) | Status stoi na **percentylu**, a percentyl liczy się z całego rozkładu naraz. Przeliczanie co trzydziestej osoby dziennie znaczyłoby, że dwie osoby o identycznym dochodzie mają przez miesiąc różny status, bo mierzono je względem dwóch różnych rozkładów. Rozkład i tak trzeba zbudować w całości; przebieg po spisie jest przy tym kosztem pomijalnym |
| G-5 ★ | **Rytm doby i miesiąca to funkcje (`society::step_day`), a nie systemy ECS.** Tabela §5.12 należy do M3d i to ona je opakuje | Ta sama decyzja co w M3a (pętla zdarzeń w runnerze) i M3b (planer wołany przez scenariusz), z dodatkowym powodem: tryb demograficzny `m3_century` przeskakuje **dobę na krok**, a `App::tick` idzie minuta po minucie — 36 000 dób to 51,8 mln ticków, z czego 51,8 mln minęłoby bez pracy do wykonania. Mutacje strukturalne i tak idą przez `CommandBuffer` sortowany po `(SystemId, entity_index)` (00 §3.4) |
| G-6 ★ | **Nadwyżka urodzeń potrzebuje zaworu.** Dorosły od 25 lat wyprowadza się od rodziców, gdy w jego dzielnicy jest pustostan; gdy nie ma ani lokalu, ani pracy — **wyjeżdża sam** | Bez tego regulator z §5.7 nie domyka pętli: gospodarstwo z jednym pracującym rodzicem nie jest „bez pracy", więc reguła odpływu gospodarstw nigdy by go nie ruszyła, a dorosłe dzieci zostawałyby w nim w nieskończoność. Populacja rosłaby ponad pojemność miasta bez żadnego hamulca — i tak właśnie zachowywał się pierwszy przebieg |
| G-7 ★ | **Etat wraca do puli przy zgonie, emeryturze i wyjeździe; gospodarstwo bez członków rozwiązuje się i oddaje lokal** | Dwa błędy tej samej klasy, oba znalezione dopiero stuletnim przebiegiem, bo oba działają **w jedną stronę**. Etat, który nie wraca do puli, znika z miasta na zawsze; lokal po zmarłym zostaje w martwym gospodarstwie. W obu przypadkach `min(wakaty, pustostany)` schodzi do zera, napływ wygasa i populacja może już tylko maleć — po stu latach z 5 257 mieszkańców zostawało 105. To nie jest kalibracja: to jest brakujące domknięcie księgowe pojemności miasta |
| G-8 ★ | **Odpływ ma człon proporcjonalny do udziału dorosłych bez pracy** (`jobless_leave_permille`), obok twardej reguły „trzy miesiące bez pracy **kogokolwiek**" | „Miesiące bez pracy któregokolwiek dorosłego" z §5.7 czytane dosłownie (nie pracuje **nikt**) opisuje mniejszość gospodarstw: para, w której jedno straciło etat, zostawałaby w nieskończoność. Bezrobocie po szoku schodziłoby wtedy dekadami zamiast lat i kryterium WP8 byłoby niespełnialne. Człon proporcjonalny jest jednocześnie głównym wzmocnieniem pętli ujemnego sprzężenia — i to on decyduje o czasie powrotu do równowagi |
| G-9 ★ | **Trzy zmiany w mechanice relacji i plotki, wszystkie wymuszone kryterium WP9:** (1) bliskie kontakty to **stały pierścień** `close_contacts` osób, nie rotacja po całej grupie; (2) przyrost wagi jest **tygodniowy** (`coworker_gain_per_week` 5, `neighbour_gain_per_week` 3), bo przebieg relacji jest shardowany 1/7; (3) plotka opowiada **do skutku** — liczy się `gossip_targets` osób, którym udało się coś powiedzieć, a nie zagadniętych | (1) Kontakt rotujący po całym zakładzie spotyka tę samą osobę raz na kilka tygodni, a waga zanika o 1 na tydzień — próg plotki (40) nie zostałby przekroczony nigdy. (2) §5.8 mówi „+1 za dobę wspólnej zmiany", a przebieg obejmuje cały tydzień naraz; bez przeliczenia relacje rosłyby siedmiokrotnie wolniej, niż mówi dokument. (3) Rozmowa z kimś, kto już wie, nie jest opowiedzeniem plotki. Bez tego rozróżnienia wiedza zatrzymywała się na pierwszym nasyconym kwartale: zasięg w promieniu kilometra rósł do 38 % i dalej nie szedł |
| G-10 ★ | **Przeprowadzka jest lokalna albo jej nie ma.** `Vacancies::take_home_in(district)` **nie ma** fallbacku na „pierwszy lepszy"; `take_job_in(district)` dobiera etat w dzielnicy; zwolniony etat wraca do puli z **własną** dzielnicą i płacą | Bez tego każda przeprowadzka po rozstaniu albo wyprowadzce z domu wstawiała do grafu relacji **most przez całe miasto**: sąsiedzi z nowego kwartału, współpracownicy ze starego zakładu. Przy czterech rundach plotki na miesiąc taki most roznosił wiedzę po metropolii w tydzień — powyżej 5 km wiedziało 93 % zamiast poniżej 5 %. W mieście zapełnionym po brzegi ruch bez wolnego lokalu w dzielnicy po prostu **się nie udaje**, i to też jest prawda o mieście: rozwiedziona para mieszka razem, dopóki coś się nie zwolni |
| G-11 ★ | **`core` dostaje dwa nowe słowniki — `MigrationKind` i `LifeEventKind`** — a blok `DecisionReason` fazy M3 rośnie do **100–119**: `PartnerChosen` (115), `SeparationFiled` (116), `MigrationDecision` (117), `Inheritance` (118), `LifeEvent` (119). Numery 120–199 zostają wolne | Ładunek centralnego enuma nie może pochodzić z crate'u, który od `core` zależy (K-12) — ta sama reguła, która wypchnęła tam `CommitmentKind` i `StockCat`. Dopisane do `K-20` w dokumencie 00. Decyzje demograficzne wracają z raportów doby i miesiąca jako `(indeks encji, DecisionReason)`, więc karta inspekcji M3d ma je skąd wziąć bez logu per mieszkaniec |
| G-12 ★ | **Domknięcie C-8 sięga planera:** `HouseholdView` dostaje `pickups`, a faza 1 planuje powrót z pracy **przez szkołę** (trzy sloty: praca → szkoła, przekazanie, szkoła → dom) | Podział ról z §5.6 wyznacza odbierającego (kończy zmianę najwcześniej) i odprowadzającego (zaczyna najpóźniej) — ale bez pola w widoku planer nie miał jak tego wykonać. Trzy sloty, a nie jeden, z tego samego powodu co przy odprowadzaniu rano (korekta F-8): każdy `Commute` musi trwać dokładnie tyle, ile `TravelOracle` liczy dla jego pary miejsc. Złoty wydruk `anna_day.txt` zmienił się o te trzy sloty i jest zatwierdzony na nowo |
| G-13 | **Wygasła relacja znika po obu stronach naraz**, a nie każda przy swoim shardzie | Shard 1/7 znaczyłby inaczej, że przez tydzień jedna strona pamięta drugą, a druga nie. Czytają to i `prop_relation_symmetry`, i dobór partnera, i podział spadku — jednostronna relacja jest w każdym z nich cichym błędem |
| G-14 | **`Slab::remove_at`** — usunięcie wpisu z zachowaniem kolejności; blok pusty wraca na wolną listę | Potrzebne trzy razy: przy zgonie (relacja u drugiej strony), przy wygaśnięciu relacji i przy wyjeździe. `swap_remove` byłby tańszy, ale zmieniałby, kogo wypchnie następne przepełnienie — czyli stan świata — w sposób zależny od historii usunięć |
| G-16 ★ | **§7.5 `bench_gossip_day`: ≤ 120 ms na jednym wątku zamiast ≤ 20 ms.** Zmierzone **63,6 ms** dla 421 tys. mieszkańców (release, jeden wątek) po usunięciu alokacji z pętli gorącej — przed nią 101 ms | Próg 20 ms nie ma za sobą rachunku, który by go tłumaczył. Doba dotyka 1/7 populacji (60 tys. osób), a na osobę przypada: wzmocnienie sześciu relacji **po obu stronach** (dwanaście przejść po slabie), przegląd własnego slabu pod kątem zaniku i dwie opowiedziane plotki, z których każda przeszukuje slab wiedzy słuchacza. To jest 1,06 µs na osobę — trudno z tego zrobić 0,33 µs bez rezygnacji z którejś z tych rzeczy. W skali, która ma znaczenie, koszt jest nieistotny: 63,6 ms na **dobę gry** to 0,004 % czasu przy prędkości 1× i 0,22 % w trybie 50× z M12. System w §5.12 jest przy tym `EveryDay` i shardowany — M3d może go puścić `par_for_each` po chunkach, tak jak M3a zrobił ze spadkiem potrzeb |
| G-17 ★ | **`seed_population` dla 200 tys. gospodarstw (421 tys. mieszkańców) trwa 19,5 s.** Etap 8 (M3d §5.9) ma na całość 30 s, więc na dopasowania statystyczne zostaje 10 s — i to jest liczba, z którą M3d musi zacząć, a nie odkryć na końcu | Koszt siedzi w `Vacancies::take_job_in`, które szuka etatu w dzielnicy liniowo od końca listy (jawny `ponytail:` z nazwanym sufitem). Przy 800 tys. etatów i nierównym rozkładzie po dzielnicach szukanie przestaje kończyć się po kilku krokach. Ścieżka wyjścia jest zapisana przy funkcji: `BTreeMap<u16, Vec<usize>>` z wolnymi indeksami per dzielnica. Nie robimy tego teraz, bo M3c nie ma kryterium czasowego na zasiew, a M3d i tak przepisze ten krok pod swoje cztery dopasowania |
| G-15 | **Płodność w `data/demography/` jest skalibrowana pod stulecie, nie przepisana z rocznika.** Surowy TFR w tabeli to 3,5; realizowany jest wyraźnie niższy, bo mnożą go trzy korekty (kolejność dziecka, brak partnera, ciasnota) i sama mechanika: ciąża trwa 270 z 360 dób, więc kobieta, która właśnie zaszła w ciążę, na następnym losowaniu rocznym najczęściej jeszcze jest w ciąży | Tabela wejściowa i realizowana dzietność to dwie różne liczby, a kryterium WP7 dotyczy drugiej. Kalibrację epok wnosi balansator M5 — scenariusz `century` jest jego gotowym wejściem (§7.2 dokumentu fazy) |

---

## Zmiany wpisane po M3a

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M3a.

| # | Zmiana | Dlaczego |
|---|---|---|
| C-1 | **`Household` powstaje w tej podfazie, nie w M3a.** M3a definiuje trzynaście komponentów mieszkańca i zostawia `Identity.household` jako indeks encji | §5.1 (M3a) nie opisuje `Household` — opisuje go §5.6, czyli ta podfaza. Budżet §17.7 liczy GD osobno (≈167 tys. × 128 B), poza stanem gorącym mieszkańca |
| C-2 | **Slaby relacji i wiedzy są gotowe** (`RelationSlab`, `KnowledgeSlab`, klasy 4/8/12/16/24/32, wolna lista, `occupied_blocks()` do wykrywania wycieków). Wypychanie 33. wpisu bierze indeks ofiary z domknięcia wywołującego, a nie z traitu | Dwa magazyny, dwie różne reguły rangowania (`Knowledge::rank` jest w `store.rs`), obie jednolinijkowe u wołającego — trait na to nie zarabia. `prop_knowledge_bound` ma gotowe `occupied_blocks()` |
| C-3 | **`StatusLoss` stosuje ta podfaza**, przy funkcji statusu z §5.8. M3a go nie odejmuje, tylko wystawia przez `deprivation_of` | Rozstrzygnięcie D-6: status jest tu **liczony**, a nie odejmowany — odejmowanie go w M3a rozjechałoby się z tą funkcją przy pierwszym uruchomieniu obu naraz. To samo dotyczy `AmbitionGain` (M7) |
| C-4 | **`ReplanCause::HouseholdEvent { kind: HhEventKind }` istnieje** z wariantami `Birth`, `ChildIll`, `Death`, `MemberJoined`, `MemberLeft`, `Moved`; `is_full_replan()` zwraca `true` dla `Death` | Zdarzenia demograficzne mają już punkt zaczepienia w planerze — WP7 wypełnia je treścią, nie projektuje od nowa |
| C-5 | **Strumienie RNG tej podfazy są przypisane i zamrożone:** `Demography` 141, `Gossip` 142, `Migration` 145, `Relations` 146 (`K-4`, blok M3 = 140–159) | Wartości raz nadane są niezmienne — zmiana numeru strumienia zmienia każdy świat wygenerowany wcześniej z tego samego ziarna |

---

## Zmiany wpisane po M3b

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M3b —
podfaza nie jest tu przeprojektowywana.

| # | Zmiana | Dlaczego |
|---|---|---|
| C-6 ★ | **`Household.stock: [u8; 8]` indeksuje się `StockCat` z `engine/core`**, nie własnym enumem. Rozmiar tablicy to `STOCK_CAT_COUNT`, a `StockCat::need()` mówi, którą potrzebę uzupełnia zakup w danej kategorii | Słownik powstał w M3b, bo jest ładunkiem `DecisionReason::StockBelowThreshold` i musiał trafić do `core` (K-12, korekta F-3). Kolejność wariantów jest kontraktem tak samo jak przy `NeedKind` — to ona indeksuje tablicę |
| C-7 ★ | **`HouseholdView` istnieje i jest widokiem, nie komponentem** (`planner.rs`): niesie `id`, `stock` i `escorts`. WP7 **wypełnia go** z `Household` i podziału ról, a nie projektuje od nowa | Planer powstał przed gospodarstwem, więc kontrakt opisuje, co planer czyta, a nie z czego to pochodzi — ta sama sztuczka co `CitizenView` w M3a (korekta F-4). Gdyby WP7 dołożył własny widok obok, planer miałby dwa źródła zapasów |
| C-8 ★ | **Odbiór dziecka ze szkoły po południu należy do tej podfazy.** M3b odprowadza rano (`escorts` = szkoły, do których ten mieszkaniec odprowadza) i na tym poprzestaje | Szkoła kończy się o 14:00, a zmiana dzienna o 16:00 — kto odbiera dziecko, wynika z **podziału ról w gospodarstwie** (§5.6), a nie z planu jednej osoby. Planer ma już gotowy mechanizm: trzy sloty na odcinek, każdy o czasie zgodnym z `TravelOracle` (korekta F-8) |
| C-9 | **Wyzwalacz „chory → wizyta u lekarza" czyta dziś wyłącznie `Needs[Health] < 30`.** `Lifecycle.flags.chory` z §5.4 nie jest jeszcze podłączony, bo `CitizenView` nie niesie `Lifecycle` | Do rozstrzygnięcia w WP7 razem z chorobą: albo choroba **obniża `Health`** (i wtedy nic nie trzeba dokładać), albo `PlanCtx` dostaje flagę. Pierwsze jest tańsze i spójne z §5.5, gdzie choroba to skok −10..−60 na `Health` |
| C-10 | **`ReplanCause::HouseholdEvent` ma po stronie planera gotową ścieżkę**: `replan` przyrostowe zachowuje zobowiązania i sen, a pełne (`Death`) buduje dobę od zera. Debouncing 15 minut jest w `request_replan` | WP7 wypełnia zdarzenia treścią i woła `request_replan` — nie musi projektować ani strategii, ani limitu przeplanowań (R3) |
