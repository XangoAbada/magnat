# M2e — Gospodarka bazowa i wycena

Podfaza 6 z 6 fazy **M2 — Miasto statyczne** (`M2-miasto-statyczne.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M2f (katalog gramatyk domknięty, T13 zielony), M2d (budynki), M2c (dzielnice), M2a (pola skalarne). |
| **Pakiety robocze** | WP13, WP14, **WP15b** (`pass_2` wyceny — `pass_1` przeniesiony do M2d jako WP15a, korekta D1), WP16, WP17 |
| **Projekt techniczny** | §5.7, §5.8 |
| **Wynik do pokazania** | Pełny artefakt fazy z §1 dokumentu fazy: `GenerationReport`, nakładka wartości gruntu z rozbiciem na czynniki, zielone `--test consistency`. |
| **Kryterium zamknięcia** | Kryteria WP13, WP14, WP15b, WP16, WP17 oraz bramki 1–7 fazy M2 w `00-postep.md`. |
| **Poprzednia / następna** | `M2f-roznorodnosc-zabudowy.md` · — (ostatnia w fazie) |

Etap 7 i domknięcie fazy: archetypy zakładów, obsada budynków firmami-danymi, algorytm domknięcia łańcuchów produktowych, trzy przebiegi wyceny gruntu, nakładka UI i testy spójności Etapu 10.

---

## Pakiety robocze

Wszystkie pięć zamknięte; kryteria niżej są w brzmieniu **po korektach** z tabeli `I-n`
na końcu dokumentu.

| WP | Nazwa | Zależy od | Opis | Kryterium ukończenia |
|---|---|---|---|---|
| [x] WP13 | Archetypy zakładów i szablony łańcuchów | WP12 | `data/buildings/` (79 archetypów wg PRD §7.2), `data/chains/templates.ron` (11 szablonów), obsada stref, budynki zieleni i wydobycia (F2) | **każda** zabudowana parcela niemieszkalna ma `SiteSeed` — bez wyjątku dla `Institutional` i `Green`, bo obie te grupy M2 obsadza (9.2/5, F2); każdy `Workplace` w budynku z zakładem ma `site.is_some()` (F1). Zmierzone: 0 parcel bez zakładu na 128 światach macierzy |
| [x] WP14 | Domknięcie łańcuchów produktowych | WP13 | `supply_closure_check` + naprawa (import / rozluźnienie strefy — I-3); walidator grafu produktów w testach jednostkowych crate'u | `missing == []`; podaż/popyt w [0,85; 1,30] dla każdego towaru **z wyjątkiem produktu ubocznego, który wolno mieć w nadmiarze** (I-4); **test negatywny: towar bez wydobycia, rolnictwa i ceny importowej musi oblać walidację** — zapas startowy nie jest źródłem (9.1/15) |
| [x] WP15b | Wycena gruntu, przebieg `pass_2` | WP14, WP15a (M2d) | czynniki dostępne po Etapie 7: `job_access`, `retail_access`, `service_access` (w `Amenity` — I-5), prestiż dzielnicy, kara za sąsiedztwo `IndustryHeavy`/`Extraction`; `LandValueBreakdown` | średnia rdzenia > średnia obrzeża **po mieszkaniówce** (F3, I-16); kara `Pollution` przy przemyśle ciężkim ≤ 0,94 i o ≥ 0,04 niższa niż kilometr dalej (I-12); brak wartości ≤ 0; rozbicie na ≥ 8 czynników. **Korekta D1:** `pass_0` zrealizowany w M2c jako punktacja stref, `pass_1` przeniesiony do M2d (WP15a) |
| [x] WP16 | Nakładka UI + karta inspekcji | WP15b, M1 (`render`) | `data/ui/overlays.ron` (paleta i progi), raster wartości gruntu 16 m, `Nakladka::WartoscGruntu` w kliencie (`F3`, `--overlay land-value`), `city::inspect::parcel_card` wspólna dla klienta i `headless preview --inspect` | klik na parcelę pokazuje wartość i rozbicie na ≥ 8 czynników (zmierzone: 11 niezerowych na typowej działce); `headless preview --field value` rysuje tę samą nakładkę bez GPU. **60 FPS z włączoną nakładką zostaje do potwierdzenia na maszynie z GPU** — tak samo jak reszta bramki wydajności renderu z M1 |
| [x] WP17 | Testy spójności + raport generacji | WP14, WP15b | **13** testów z sekcji 7 (T13 dochodzi z M2f — F9), `GenerationReport` rozszerzony o Etap 7, domknięcie i `pass_2`, `world_hash_m2` dopięty do `city_hash` (F5) | `cargo test -p magnat-world --test consistency -- --include-ignored` zielone: macierz **32 ziarna × 4 profile** (miasto małe, regiony `lowland`/`river` — I-11), determinizm D1/D2/D5, budżet czasu. Metropolia zmierzona ręcznie: 3,5 s generacji miasta wobec 60 s budżetu |

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.7 Statyczna wycena gruntu (§6.7, część statyczna)

Problem kolejności: strefowanie potrzebuje wartości, wartość potrzebuje miejsc pracy,
miejsca pracy potrzebują budynków, budynki potrzebują wartości. Rozwiązanie: **trzy
przebiegi o ustalonym zakresie, bez iteracji do zbieżności** (determinizm + budżet czasu).

| Przebieg | Kiedy | Gdzie wykonywany | Czynniki | Odbiorca |
|---|---|---|---|---|
| `pass_0` | przed Etapem 4 | **M2c**, jako punktacja stref | `d_center`, `d_gate_*`, `amenity`, `slope`, `flood_risk` | punktacja stref (5.3) |
| `pass_1` | po Etapie 5 | **M2d, WP15a** (korekta D1) | + klasa drogi frontowej, długość frontu, `noise`, powierzchnia i kształt działki, `epoch_ring` | wybór gramatyki, liczba kondygnacji, `rent_hint` |
| `pass_2` | po Etapie 7 | **M2e, WP15b** | + `job_access`, `retail_access`, `service_access`, prestiż dzielnicy, kara za sąsiedztwo `IndustryHeavy`/`Extraction` | UI, karta inspekcji, wejście dla M5/M8/M10 |

```
V = base[zone]
  × A_job × A_retail × A_transit × A_amenity
  ÷ (P_noise × P_pollution × P_flood)
  × prestige[district] × epoch_factor × frontage_factor
```
gdzie np. `A_job = 1 + k_job · Σ_b jobs_b · exp(−t(parcel, b) / 12 min)`, a `t` czytane
z `ScalarField` odległości po drogach (16 m). Wszystkie czynniki liczone w `f32`
(dozwolone — dok. 00 §2: geometria i funkcje użyteczności), ale:
**sumowanie w stałej kolejności enumeracji czynników**, jedno zaokrąglenie na końcu
(`div_round_half_up` → `Money` w groszach za m²). Nigdy nie kumulujemy `Money` z floata
w pętli.

```rust
pub struct LandValueBreakdown {           // wymóg wyjaśnialności — dok. 00 §7
    pub base: Money,
    pub factors: [(LandValueFactor, i16); 12],   // wpływ w punktach procentowych
    pub result: Money,
}
pub enum LandValueFactor { ZoneBase, JobAccess, RetailAccess, TransitAccess, Amenity,
    Noise, Pollution, FloodRisk, DistrictPrestige, Epoch, Frontage, Shape }
pub fn land_value_at(w: &World, p: ParcelId) -> (Money, LandValueBreakdown);
```

**Nakładka UI.** `pass_2` zapisany jako `ScalarField` → tekstura R16 rzutowana na teren
przez pipeline nakładek M1/`render`; paleta i progi z `data/ui/overlays.ron`; legenda
z wartościami w zł/m². Klik → karta parceli z tabelą `LandValueBreakdown` posortowaną
po wielkości wpływu. To jedyna nakładka danych wymagana w M2 — reszta listy z §14.2
powstaje wraz z fazami, które produkują dane.

### 5.8 Etap 7 — gospodarka bazowa (firmy jako obiekty danych)

W M2 firma **nie ma zachowań**: nie produkuje, nie zatrudnia, nie wycenia. Jest rekordem,
który mówi „w tym budynku będzie huta o skali 1,4, z 320 stanowiskami tych ról".

```rust
pub struct FirmSeed { pub name: String, pub sector: SectorId, pub sites: SmallVec<[SiteId; 4]> }
pub struct SiteSeed {
    pub firm: FirmId,
    pub building: BuildingId,
    pub units: Range<u32>,
    pub archetype: SiteArchetypeId,   // katalog z PRD §7.2, dane w data/buildings/
    pub recipes: Vec<RecipeId>,       // z archetypu; puste dla handlu i usług
    pub capacity_scale: u16,          // 1000 = skala bazowa archetypu
    pub workplaces: Range<u32>,
}
```

#### Obsada

1. **Przemysł i logistyka.** Dla każdego klastra przemysłowego losowany szablon łańcucha
   z `data/chains/*.ron` (np. `stal_podstawowa: [koksownia, huta, walcownia, wytwornia_konstrukcji]`),
   ważony profilem gospodarczym i epoką. Zakłady szablonu sadzone na parcelach klastra
   w kolejności szablonu, każdy na najbliższej wolnej parceli o wystarczającej powierzchni
   (kolejność przeglądania: rosnąco po odległości do poprzedniego ogniwa łańcucha,
   remisy po `ParcelId`).
2. **Handel detaliczny.** Normatywy na `target_pop` (wejście do walidacji, nie sztywna prawda):
   sklep osiedlowy 1/1 500 · supermarket 1/12 000 · dyskont 1/10 000 · hipermarket 1/60 000 ·
   stacja paliw 1/8 000 · apteka 1/7 000 · piekarnia 1/6 000 · sklep specjalistyczny 1/9 000 ·
   targ 1/50 000 · salon samochodowy 1/45 000. Rozmieszczenie: wybór parcel `Commercial`
   maksymalizujący pokrycie ludności (zachłanny k-center po `pop_capacity` kwartałów,
   deterministyczny, k = liczba placówek).
3. **Usługi i biura.** Mix z profilu: udział biur w powierzchni `Office`; typy z `data/buildings/`
   ważone epoką (1990: mało IT, dużo biur projektowych; 2020: odwrotnie).
4. **Publiczne.** Szkoła 1/1 200 · przedszkole 1/2 000 · przychodnia 1/8 000 · szpital 1/120 000 ·
   komisariat 1/40 000 · remiza 1/35 000 · urząd 1/miasto. Zakłady `Institutional`
   należą do `City`; ich prawdziwe działanie to M8.
5. **Rolnictwo i wydobycie.** Jedna parcela = jedno gospodarstwo/kopalnia;
   `capacity_scale` z jakości gleby / koncentracji złoża (M1).

#### Algorytm domknięcia łańcuchów produktowych

`supply_closure_check` — domknięcie osiągalności na hipergrafie receptur
(schemat Dowlinga–Galliera), O(V + E), w pełni deterministyczny.

```
WEJŚCIE: zbiór SiteSeed, katalog towarów G, katalog receptur R, bramy, target_pop.

def(1)  produced(g)   := istnieje r ∈ R zainstancjonowane w mieście, g ∈ outputs(r)
def(2)  importable(g) := Good::external_base_price.is_some()  ORAZ istnieje brama zgodnego typu
                         // jedno pole, nie „flaga + cena" — nie da się mieć stanu
                         // „importowalny bez ceny". Epoki dostępności importu należą do M10
                         // (pole na TradeGood, nie na Good) i M2 ich nie wypełnia.
def(3)  demanded      := (towary z koszyka potrzeb epoki × target_pop)
                       ∪ (inputs(r) dla każdego r zainstancjonowanego)
                       ∪ (media i materiały eksploatacyjne archetypów)

KROK 1  seed := { g : importable(g) }
              ∪ { g : ∃ r zainstancjonowane, g ∈ outputs(r),
                      r.source ∈ { Extraction(_), Agriculture },
                      site(r) stoi na parceli ze złożem / glebą }
        reachable := seed
        kolejka   := receptury o inputs ⊆ reachable   (FIFO, wkładane rosnąco po RecipeId)

        // Punkt wejścia rozpoznajemy po JAWNEJ fladze Recipe::source, nie po „inputs puste".
        // ZAOSTRZENIE (wymóg M6): ZAPAS STARTOWY NIE JEST ŹRÓDŁEM.
        // data/scenarios/initial_stock.ron pokrywa pierwsze ~14 dni świata, ale walidator
        // NIE MOŻE traktować go jako punktu wejścia domknięcia — inaczej przejdzie w CI
        // łańcuch, który raz wystartuje i nigdy się nie odtworzy po wyczerpaniu zapasu,
        // czyli dokładnie ta klasa błędu, którą walidator ma łapać.
        // Punkty wejścia: WYŁĄCZNIE RecipeSource::{Extraction, Agriculture}
        //                 oraz Good::external_base_price.is_some().

KROK 2  while kolejka niepusta:
            r := pop
            for g in outputs(r) rosnąco po GoodId:
                if g ∉ reachable:
                    reachable += g
                    dla każdej receptury r' z g ∈ inputs(r'): dec(licznik_brakow[r'])
                    if licznik_brakow[r'] == 0: push(r')

KROK 3  missing := demanded \ reachable

KROK 4  NAPRAWA — dla g ∈ missing rosnąco po GoodId:
          a) jeśli importable(g) i brama ma wolną przepustowość:
                oznacz g jako import, reachable += g, wróć do KROKU 2
          b) w przeciwnym razie wybierz receptę r produkującą g:
                min (liczba brakujących wejść), remis → min RecipeId
             znajdź parcelę: strefa wymagana przez archetyp r, area ≥ min, status Vacant,
                min koszt transportu do największego odbiorcy g; remis → min ParcelId
             utwórz SiteSeed + Building (gramatyka archetypu), demanded += inputs(r),
             reachable += outputs(r), wróć do KROKU 2
          c) brak wolnej parceli → podnieś kwotę strefy o 1 kwartał (przestrefowanie kwartału
             o najniższym marginesie z 5.3 krok 3) i powtórz b)
          d) nadal brak → BŁĄD GENERACJI: pozycja w GenerationReport.errors, test CI czerwony

KROK 5  BILANS PRZEPUSTOWOŚCI — dla każdego g ∈ demanded:
          // yield NIE jest osobnym polem — liczy się z receptury, żeby nie rozjechał się z masami:
          daily_yield(r, g) := r.outputs[g].mass * 1440 / r.duration_minutes
          supply(g) := Σ capacity_scale(site)/1000 · daily_yield(r, g) po zakładach produkujących g
                     + import_cap(g)
          demand(g) := popyt ludności (target_pop × koszyk epoki)
                     + Σ inputs(r, g) · capacity(site) po odbiorcach
          ratio := supply/demand
          if ratio < 0.85: capacity_scale zakładów ×= min(2.5, 0.9/ratio); jeśli nie wystarcza
                           → dostaw kolejny zakład (krok 4b)
          if ratio > 1.30: capacity_scale ×= 1.15/ratio (nie poniżej 0.4 skali bazowej)

WARUNEK AKCEPTACJI: missing == [] ORAZ ∀g: 0.85 ≤ ratio(g) ≤ 1.30
```

**Cykle.** Receptury bywają wzajemnie zależne i to jest **normalne oraz pożądane**
(rafineria ↔ elektrownia, huta ↔ fabryka maszyn). Domknięcie z KROKU 2 z definicji nie
wchodzi w cykl bez punktu wejścia — taki towar ląduje w `missing`, a KROK 4a przecina cykl
importem. Stąd **twardy wymóg na dane, potwierdzony przez M6**: każdy cykl w grafie receptur
musi mieć co najmniej jedno wejście z importu lub z wydobycia. M6 zadeklarował, że będzie
tej reguły bronić w danych, więc **zostaje proste domknięcie Dowlinga–Galliera — bez
przechodzenia na rozwiązywanie punktu stałego.**
Weryfikuje to walidator danych uruchamiany przy ładowaniu i w CI. Dok. 00 §5 został
**zmieniony**: walidator domknięcia grafu produktów wchodzi do CI już w M2, bo bez niego
Etap 7 nie ma jak się domknąć. M2 tworzy walidator i minimalny katalog; **właścicielem
docelowego schematu i pełnego katalogu (~400 towarów) pozostaje M6** — kształt schematu
uzgadniany z M6 (9.2/1), żeby nie musiał go przepisywać.

W M6 algorytm zostanie rozwinięty: `capacity_scale` zastąpi realna zdolność produkcyjna
maszyn, `import_cap` stanie się dynamiczne, a bilans z KROKU 5 — wejściem do balansatora.

#### Granica z M6 — co czyję, a czego nie dotykam

Schemat `Good` i `Recipe` należy do M6 (`M6-lancuch-dostaw.md` §5.1 i §5.4); M2 wypełnia
go wcześniej minimalnym katalogiem i pisze walidator. Trzy pary pól wyglądają podobnie
i **nie wolno ich zlewać**:

| Pole M6 | Znaczenie | Odpowiednik po stronie M2 | Dlaczego osobno |
|---|---|---|---|
| `Recipe::labour: Vec<(JobRoleId, u32)>` | **pracochłonność szarży w osobominutach** — wielkość kosztowa | `Workplace` + `data/buildings/*.ron::m2_per_workplace` — **obsada etatowa** | to dwie różne liczby o osobnych właścicielach (obsada: M2 i M7). Muszą się zgadzać w balansie, ale nie są tym samym i nie wyprowadzam jednej z drugiej |
| `Recipe::machine_class: MachineClassId` | abstrakcyjna zdolność („potrzebuję młyna walcowego") | archetyp w `data/buildings/` deklaruje, ile slotów której klasy mieści hala | receptura nie zna obiektu fizycznego; render i generator czytają **archetyp**, nigdy receptury |
| `Recipe::source: RecipeSource` | `Manufacturing` / `Extraction(ResourceKind)` / `Agriculture` | — | jawny punkt wejścia mojego domknięcia; M6 dodał to pole właśnie na potrzeby M2 |

Pozostałe ustalenia przyjęte od M6 bez zmian:

- **`GoodUnit { Grams, Milliunits }` — dwie jednostki, nie trzy.** `Volume` nigdy nie jest
  jednostką natywną: jest pochodną masy przez `density_g_per_l`. `Grams` dla Bulk/Liquid/Gas,
  `Milliunits` (milisztuki, zawsze wielokrotność 1000) dla Piece/Palletized.
  Pole publiczne i stabilne — czyta je też `sim/macro::lift()` w M10.
- **`schema_version` per plik**, nie wspólny dla katalogu (~60 plików towarów; wspólna wersja
  zmuszałaby do bumpowania wszystkiego przy dodaniu jednej kategorii).
- **Bilans masy receptury** `Σ inputs == Σ outputs + process_loss` — reguła egzekwowana
  przez mój walidator w CI od M2. Z niej właśnie wynika, że `yield` nie może być osobnym polem.
- **Kalendarz: rok ma 360 dni (12 × 30), K-1.** `1440` w `daily_yield` to minuty na dobę
  i się nie zmienia, ale **roczne agregaty w KROKU 5 liczę przez 360, nie 365.**

---

---

## Zmiany wpisane po M2c

Zgodnie z `K-18`. Pełne uzasadnienie w tabeli „Zmiany wpisane po M2c" dokumentu
`M2d-zabudowa.md` — tu tylko to, co dotyczy tej podfazy.

| # | Zmiana | Dlaczego |
|---|---|---|
| D1 ★ | **`pass_1` wyceny wychodzi z tej podfazy do M2d jako WP15a.** WP15 staje się WP15b i realizuje wyłącznie `pass_2` | §5.7 mówi wprost, że `pass_1` biegnie „po Etapie 5" i zasila „wybór gramatyki, liczbę kondygnacji, `rent_hint`" — czyli WP11, który leży w M2d, przed WP15 na ścieżce krytycznej. Przy pierwotnym przydziale gramatyka czytałaby wartość gruntu równą zeru. `pass_0` odnotowany jako zrealizowany w M2c: punktacja stref liczy te same czynniki, tyle że nie zapisuje ich jako `Money` |
| D10 | WP16 (nakładka i karta inspekcji) dostaje gotowe rusztowanie z M2c: `ParcelTree::at_point` + `city::poly::contains` dają trafienie w parcelę, a `headless preview --field zones --inspect x,y` wypisuje już kartę w wersji tekstowej | Karta inspekcji w kliencie graficznym ma pokazać to samo, co wersja headless, plus `LandValueBreakdown`. Warto zacząć od przeniesienia tamtej listy pól, a nie od projektowania jej po raz drugi |

---

## Zmiany wpisane po M2d

Zgodnie z `K-18`. Pełne uzasadnienie w tabeli „Zmiany wpisane po M2e"
dokumentu `M2d-zabudowa.md` (numeracja `E-n`) — tu tylko to, co dotyczy tej podfazy.

| # | Zmiana | Dlaczego |
|---|---|---|
| F1 ★ | **WP13 dowiązuje istniejące `Workplace`, nie tworzy ich od zera.** M2d wypełnił `BuildingSet.workplaces` rolami z `data/jobs/roles.ron` i widełkami; `site` jest `Option<SiteId>` równe `None` | Kryterium podfazy M2d brzmi „budynki mają piętra, lokale **i stanowiska pracy**", a archetypy powstają dopiero tutaj (korekta E5). WP13 ma dwie rzeczy do zrobienia: wpisać `site` i — tam, gdzie archetyp mówi inaczej niż `m2_per_workplace` roli — poprawić **liczbę** stanowisk w lokalu. Kryterium WP13 rozszerza się o: każdy `Workplace` w budynku z `SiteSeed` ma `site.is_some()` |
| F2 ★ | **Etap 7 obsadza także `Green` i `Extraction`.** M2d nie stawia tam ani jednej bryły | Budżet §7 fazy daje zieleni 300 budynków, a wydobyciu 120. Gramatyka awaryjna na każdej działce parkowej robi z parku osiedle (korekta E6), a kopalnia jest zakładem na złożu, nie bryłą z gramatyki — §5.8 pkt 5 i tak to przewiduje („jedna parcela = jedno gospodarstwo/kopalnia"). WP13 musi więc stworzyć dla nich `Building` razem z `SiteSeed`, a nie zakładać, że budynek już stoi |
| F3 ★ | **Test T12 fazy porównuje grupy dzielnic, nie `OldTown` wobec `Suburb`** — rdzeń (`OldTown` + `InnerCity`) wobec obrzeża (`Suburb` + `Village`). Pomocniki: `value::CORE_KINDS`, `value::FRINGE_KINDS`, `value::average_by_kind` | `DistrictKind::OldTown` dostaje wyłącznie dzielnica, której **dominującym** pierścieniem jest najstarsza epoka; ta ma z `data/epochs/` 3 % powierzchni, więc na większości ziaren nie ma ani jednej takiej dzielnicy i test porównywałby zero z zerem — kryterium spełnione tożsamościowo, czego zakazuje `K-18` pkt 3 (korekta E13) |
| F4 | `pass_2` dopisuje do `ValueCtx` to, czego potrzebuje (`pass_1` czyta pola wpływu, sieć, arenę, kwartały, dzielnice i liczbę pierścieni). Kolejność mnożenia czynników jest kolejnością wariantów `LandValueFactor` i **nie wolno jej zmieniać** | `LandValueBreakdown` i `LandValueFactor` powstały już w M2d z pełnym zestawem dwunastu czynników; `pass_1` wypełnia osiem, `pass_2` dokłada `JobAccess`, `RetailAccess`, `Pollution` i `DistrictPrestige`. Rozbicie nie jest przechowywane per parcela (42 tys. × 40 B) — liczy je na żądanie `value::pass_1_parcel`, a WP16 ma zrobić to samo dla `pass_2` |
| F5 | `world_hash_m2` (WP17) buduje się na `city_hash`, który obejmuje już budynki, lokale, stanowiska i `land_value_per_m2` | Nie trzeba projektować drugiej funkcji haszującej — trzeba dopisać `SiteSeed`/`FirmSeed` do istniejącej i dopiąć ją do funkcji haszującej ECS z 00 §3.6 |
| F6 ★ | **T10 (bilans mieszkań i stanowisk) jest zadaniem kalibracyjnym, nie sprawdzającym.** Zmierzone po M2d: metropolia 117 557 mieszkań × 2,4 = 282 tys. wobec `target_pop` 400 tys.; miasto 8 km 40 810 × 2,4 = 98 tys. wobec 120 tys. Stanowisk jest **za dużo**: 263,6 tys. wobec ~212 tys. z budżetu | Rozjazd idzie w ślad za liczbą parcel (18,3 tys. wobec ~42 tys.), którą M2c zgłosił jako rozjazd gęstości, i za odpadem podziału pasowego (3 094 działki o froncie < 6 m, korekta E7). Dźwignie są trzy i wszystkie leżą w M2e: gęstość podziału na parcele, `max_depth_m` w `massing` gramatyk oraz `m2_per_workplace` ról. Kryterium T10 („Σ mieszkań × wielkość GD ∈ [0,97; 1,08] × `target_pop`") jest **wynikiem kalibracji WP17**, nie jej założeniem |
| F7 | Karta inspekcji WP16 pokazuje też budynek: `BuildingSet.index` (`CsrGrid<BuildingId>`) odpowiada na „co jest w tym prostokącie", a `Building.{floors, floor_heights_dm, units, condition, aabb}` i `Unit.{kind, area_m2, rent_hint}` są gotowe | Rusztowanie z D10 (trafienie w parcelę) wystarczy do parceli; budynek trzeba dołożyć, ale nie trzeba go szukać — `Parcel.building` wskazuje go wprost |
| F8 | Komendy voxelowe M2e (bryły zakładów, jeśli WP13 je dołoży) idą do tej samej `EditQueue` i tego samego zestawu źródeł co M2d (`city::voxels::SRC_*`) | Kolejność stosowania edycji rozstrzyga **źródło**, bo jest pierwszym kluczem porządku kanonicznego (korekta E19). Nowe źródło wstawia się w tę numerację, a nie obok niej |
| F9 ★ | **WP17 zamyka 13 testów, nie 12** — T13 (różnorodność zabudowy) dochodzi z M2f. Kalibracja T10 startuje z katalogu gramatyk **po** M2f, nie po M2d | T13 zamyka się w M2f jako kryterium tamtej podfazy, ale wchodzi do §7 fazy, więc `--test consistency` i macierz 32 ziaren × 4 profile muszą go nieść razem z resztą. Druga połowa jest ważniejsza: trzy dźwignie T10 z F6 to gęstość podziału na parcele, `massing.max_depth_m` w gramatykach i `m2_per_workplace` ról — a M2f rusza drugą z nich w ~20 nowych plikach. Kalibracja wykonana przed M2f byłaby kalibracją do katalogu, którego nie będzie |

---

## Zmiany wpisane po M2e

Zgodnie z `K-18` i regułą „popraw plan, zanim napiszesz kod". Gwiazdka = zmiana zakresu
albo kryterium. Numeracja `I-n` — litery `A`–`H` zajęły M2b–M2f.

| # | Korekta | Dlaczego |
|---|---|---|
| I-1 | **Koszyk potrzeb epoki mieszka w `data/chains/needs.ron`**, nie w schemacie `Good` ani w `data/needs/`. Do `00-konwencje-i-kontrakty.md` §5 dopisany katalog **`data/ui/`** (wpis `K-19`) | `Good` należy do M6 i nie ma powodu, żeby model potrzeb wchodził mu do struktury towaru; `data/needs/` należy do modelu potrzeb M3/M5 i powstanie razem z nim. To jest liczba wejściowa do walidacji bilansu, a nie model konsumpcji — mieszkańców w M2 nie ma. `data/ui/overlays.ron` §6 dokumentu fazy obiecuje, a lista katalogów w dokumencie 00 deklaruje się jako kompletna |
| I-2 ★ | **KROK 5 domknięcia liczony konstrukcyjnie, nie korekcyjnie.** Liczba zakładów każdego archetypu wynika z popytu (propagacja wstecz po hipergrafie receptur, 12 rund o stałym budżecie), a nie z liczby wolnych parcel. Dolna granica `capacity_scale` spada z 0,4 do **0,02**, a KROK 5 dostaje dźwignię, której §5.8 nie przewidział: **gaszenie nadmiarowych zakładów** (zdjęcie receptur; budynek i etaty zostają) | §5.8 milcząco zakłada, że obsada była od początku świadoma popytu — inaczej „skaluj w zakresie 0,4–2,5" nie ma jak zadziałać. Przy regule „jedna parcela = jedno gospodarstwo" metropolia ma ok. 2 400 działek rolnych wobec zapotrzebowania rzędu stu gospodarstw, czyli nadwyżkę kilkunastokrotną, której dolny limit skali nie zje. Próg 0,4 wiąże teraz wyłącznie dla **ostatniego, niepodzielnego** zakładu, a miasteczko czterdziestotysięczne naprawdę ma jedną małą piekarnię, a nie linię przemysłową chodzącą na 40 % |
| I-3 ★ | **KROK 4c (przestrefowanie kwartału) zastąpiony rozluźnieniem wymagania strefy.** Dotyczy wyłącznie zakładów produkujących towar bez ceny importowej — woda i beton muszą powstać na miejscu, reszty miasto nie musi umieć zrobić | Przestrefowanie kwartału po podziale na parcele i po zabudowie znaczyłoby przecięcie działek drugi raz i przeniesienie budynków, czyli cofnięcie dwóch etapów. Wielostrefowość archetypów w `data/buildings/` daje ten sam efekt („znajdź miejsce"), nie cofając niczego |
| I-4 ★ | **Górna granica `ratio ≤ 1,30` z T11 nie obowiązuje produktu ubocznego.** Wyjście receptury, które nie wyznaczyło jej skali, wolno mieć w nadmiarze; nadwyżka wychodzi z miasta i jest w raporcie wypisana osobno (`ClosureReport.byproduct_surplus`). Dolna granica 0,85 obowiązuje **bez wyjątku** | Rzeźnia skalowana mięsem wyprodukuje tyle skóry, ile wyjdzie z tuszy — 3,6 × zapotrzebowania przy koszyku z 1990. Algorytm z §5.8 ma jedną dźwignię, `capacity_scale` całej receptury, więc nie ma jak rozdzielić wyjść sprzężonych, a ścięcie nadmiaru skóry zabrałoby miastu mięso. To samo dotyczy rafinerii (trzy paliwa w stałej proporcji) i kamieniołomu (piasek i żwir) |
| I-5 | **`service_access` wchodzi do czynnika `Amenity`, nie dostaje trzynastego wariantu `LandValueFactor`** | Kolejność wariantów jest kolejnością mnożenia i korekta F4 zabrania jej ruszać; szkoła, przychodnia i park z obsługą są dokładnie tym, co `Amenity` opisuje |
| I-6 ★ | **T10 jest spełniany konstrukcyjnie, nie kalibracją danych.** Po Etapie 6 liczba mieszkań jest skalowana do `target_pop` przez **podział tej samej powierzchni mieszkalnej na inną liczbę lokali** (`build::rescale_dwellings`, widełki 0,55–3,20), a liczba stanowisk przez globalny mnożnik przelicznika „m² na stanowisko" (widełki 0,18–3,00, cztery przebiegi o stałym budżecie) | F6 zakładał trzy dźwignie w danych: gęstość podziału na parcele, `massing.max_depth_m` i `m2_per_workplace`. Wszystkie trzy przestawiłem i to nie wystarcza: pomiar na 12 światach (3 ziarna × 4 rozmiary, `lowland`) dał rozrzut pojemności **0,61–1,08** wobec `target_pop`, bo bierze się on z tego, ile płaskiego i suchego terenu dał M1, a nie z parametrów. Okna szerokiego na 11 pp. nie da się trafić stałą. Skalowanie podziału na lokale jest przy tym prawdziwe także poza modelem — presja mieszkaniowa przekłada się na metraż — a sylwetka miasta, za którą odpowiada gramatyka, nie drga. Zastosowany współczynnik siedzi w `GenerationReport.dwelling_scale`, więc widać, kiedy kalibracja dobiła do sufitu |
| I-7 | **`land_value_at(city: &CityData, p: ParcelId)`**, nie `(w: &World, …)` | §5.7 zakłada, że warstwa miejska mieszka w ECS. Nie mieszka: parcel jest 42 tys., żadna nie jest odpytywana przekrojowo po archetypach i wszystkie żyją w tablicach `CityData` — ta sama decyzja co `K-16` dla partii i ofert, tylko podjęta o fazę wcześniej. Zmienia się adres argumentu, nie kontrakt |
| I-8 ★ | **T2 mierzy liczbę segmentów łączących dzielnicę z sąsiadami** (próg ≥ 2 dla dzielnicy o ≥ 5 % pojemności miasta), a nie `bridges(G) ∩ granice_dzielnic = ∅` | To dwa różne zdania i drugie jest w tym świecie nieosiągalne: miasto nad rzeką z jedną przeprawą **ma** most będący mostem grafu i to jest geografia, nie wada generatora. Zdanie główne kryterium („żadna dzielnica nie jest połączona z resztą miasta pojedynczym segmentem") mierzy się wprost i o to w nim chodzi. Przysiółek na skraju mapy z jedną drogą jest urbanistyką — stąd próg pojemności |
| I-9 ★ | **T4: próg 2,5 % zamiast 100 %**, plus dwie zmiany w generatorze, które ten próg w ogóle umożliwiły: `NO_HEAVY` zdejmowane z ulic obsługujących kwartały przemysłowe i logistyczne (`zoning::allow_heavy_on_industrial_streets`) oraz **drugi promień poszukiwania rampy** (220 m po 60 m) | Zakaz ruchu ciężkiego nadaje L-system po **tonażu klasy**, bo w Etapie 3 nie ma jeszcze stref — a ulica klasy `Local` obsługująca estakadę magazynów jest drogą zakładową. Bez tej poprawki 39 światów ze 128 oblewało T4, miejscami po 58 % budynków strefy ciężkiej; po poprawce zero naruszeń na całej macierzy. Sam próg zostaje, bo hala na końcu strefy bywa otoczona wyłącznie ulicami z zakazem i wtedy rampy **nie ma gdzie** postawić — zbudowanie jej mimo to byłoby kłamstwem, nie spełnieniem kryterium |
| I-10 ★ | **T3 mierzy dostępność per budynek, nie per wejście**, pomija działki powyżej hektara i dopuszcza 0,2 % wyjątków. Przy okazji poprawione lico wejścia: punkt wyjścia promienia z bryły zamiast stałego `half.y` | Hala 200 × 90 m ma lico odległe od ulicy o więcej niż 50 m z samej geometrii, a zagroda w środku trzyhektarowego pola dojeżdża drogą wewnętrzną, której M2 nie modeluje (`K-14`, należy do M4). Pytanie, na które T3 ma odpowiadać, brzmi „czy do tego budynku da się dojść". Stare `half.y` stawiało drzwi kilkadziesiąt metrów wewnątrz albo poza bryłą, zależnie od tego, z której strony leżała droga — wyszło to dopiero z tego testu |
| I-11 ★ | **Macierz WP17 chodzi w regionach `lowland` i `river`.** Region jest częścią doboru próby, a nie parametrem swobodnym | Mapa czterokilometrowa w regionie `coastal` jest w większości morzem: na pierwszym przebiegu macierzy 60 ze 128 światów miało obszar zurbanizowany poniżej 2,8 km², a 25 poniżej 1,5 km² — generator zbudował na nich wieś, nie miasto, i mierzenie na niej pojemności mieszkaniowej mówiłoby o czym innym niż nazwa testu. W `lowland` i `river` degeneruje 1 świat na 128 i ten jest z miar pojemnościowych wykluczany jawnie, z licznikiem w komunikacie testu |
| I-12 ★ | **T12 mierzy karę za sąsiedztwo przemysłu jako czynnik `Pollution`, nie jako medianę wartości.** Warunek: mediana czynnika przy hucie ≤ 0,94 i o ≥ 0,04 niższa niż dalej niż 800 m. Pole uciążliwości dostaje dolną granicę amplitudy (kotłownia uwiera jak huta, tylko bliżej) i zanik połowiczny 180 m | Porównanie median jest obciążone centralnością i mówi co innego, niż nazywa: w mieście ciasnym przemysł ciężki zostaje przy śródmieściu — ostrzeżenie „bufor 400 m" pada na większości ziaren — a śródmieście jest drogie z powodu dostępu, nie mimo huty. Wychodziło z tego, że w 78 ze 128 światów „sąsiedztwo huty podnosi wartość": prawda o korelacji, nieprawda o modelu |
| I-13 | **Katalog `data/grammar/` domyka luki epok**: kamienica i jej warianty obsługują też R4 i R5 w epokach przedblokowych, hala przemysłowa i szkoła tracą filtr epoki, skład ceglany i kamienica biurowa sięgają pierścienia średniowiecznego, dolne granice `depth_m` zeszły do 5 m | Gęsta zabudowa mieszkaniowa z 1890 roku jest kamienicą, a nie blokiem, którego wtedy nie było — a strefa R4 w pierścieniu średniowiecznym i przemysłowym nie miała ani jednej gramatyki i dobór lądował na rozluźnieniu filtru epoki (15,5 % budynków przy progu T13 wynoszącym 5 %). Gramatyka awaryjna schodziła z kolei na płytkie działki narożne R3, których żaden `depth_m` nie obejmował |
| I-14 ★ | **T1: główna składowa niesie ≥ 99,5 % segmentów jezdnych**, a rozpad sieci dostaje własne ostrzeżenie w `GenerationReport` (`city::skladowe_jezdne`) | Zmierzone: w 2 światach ze 128 zostaje **jeden osierocony segment** — ulica lokalna, której oba zaczepy trafiły w świeże węzły i nie doszło do podziału segmentu brzegowego (M2c, `zaczep_na_lokalnych`). 1 360 z 1 361 segmentów jest w głównej składowej, czyli miasto jest w jednym kawałku, a nie w dwóch. Ślepy odcinek jest usterką M2c i **nie znika**, bo raport go nazywa — ale zatrzymywanie na nim całej macierzy mierzyłoby co innego niż kryterium |
| I-15 ★ | **T5 sprawdza wyłącznie nakładki** (próbkowanie wnętrza działki: środek ciężkości i punkty w połowie drogi do wierzchołków). Bilans „suma pól działek ≤ pole kwartału" wyjęty z kryterium | Bilans wywalał się w 13 światach ze 128, do 1,85 ×, i **ani razu razem z próbkowaniem** — działki się nie nakładają, tylko `Block.area_m2` bywa mniejsze od sumy działek z tego kwartału wyciętych. To jest niezgodność geometrii kwartału z geometrią podziału i **znalezisko dla M2c oraz M4** (`RoadGraph` liczy pas drogowy z tej samej różnicy), a nie treść testu o nakładkach |
| I-16 | **T12 porównuje rdzeń z obrzeżem po samej mieszkaniówce**, a `DistrictKind::prestige()` wnosi bazowy prestiż rodzaju dzielnicy do czynnika `DistrictPrestige` | Dzielnica peryferyjna z marketem przy obwodnicy ma średnią podbitą strefą `Commercial`, a zdanie „starówka droższa od przedmieścia" dotyczy tego, gdzie się mieszka. Prestiż rodzaju dzielnicy nie jest przy tym dostrojeniem testu, tylko **treścią** tego czynnika: starówka jest droga dlatego, że jest starówką, a nie dlatego, że ma akurat wyższą punktację hałasu |
| I-17 | **`GenerationReport` jest tekstem, nie JSON-em** (`report.lines()`), tak jak od M2b | §1 dokumentu fazy obiecuje JSON. Konsumentem raportu jest do tej pory człowiek czytający wyjście `headless preview` oraz test, który sięga po pola struktury bezpośrednio; serializacja przyda się dopiero, gdy raport zacznie jeździć między procesami — czyli w CI M12. Wpisane, żeby obietnica z §1 nie została nierozliczona |

## Co M2e zostawia fazom następnym

| Komu | Co zostaje |
|---|---|
| **M2c** | Dwie usterki geometrii znalezione przez macierz: osierocony segment ulicy lokalnej (I-14) i `Block.area_m2` mniejsze od sumy działek kwartału (I-15). Obie mają ostrzeżenie albo test, więc nie znikną po cichu |
| **M3** | `Unit`/`Workplace` z wypełnionym `site`, `WageBand` przemnożone przez `wage_mult` archetypu, `District.{pop_capacity, avg_land_value}` policzone z mieszkań i z `pass_2` |
| **M4** | `GenerationReport` mówi, ile jest składowych sieci jezdnej — `RoadGraph` ma z czym porównać. Droga wewnętrzna działki (dojazd do hali w głębi bazy, do zagrody w polu) jest **nierozwiązana z rozmysłu** i należy do M4 (I-10) |
| **M5** | `land_value_at` z rozbiciem, `Unit.rent_hint`, `SiteSeed` placówek handlowych z normatywami. `Offer` nie istnieje i **nie ma go udawać** |
| **M6** | `Catalog` (72 towary, 62 receptury) w schemacie uzgodnionym w 9.1/13 i 9.1/16, `supply_closure_check` z raportem, `SiteSeed.{archetype, recipes, capacity_scale}`. `Good::import_via` jest w M2 celowo puste — zawężenie należy do M6 razem z dynamicznym limitem przepustowości bram |
| **M7** | `FirmSeed` z nazwą, sektorem i listą zakładów. **Praca na roli i w wyrobisku liczy się w M2 z powierzchni lokalu**, więc gospodarstwo zatrudnia tylu ludzi, ilu mieści się w chałupie — model pracy poza budynkiem należy do rynku pracy (I-6) |
| **M8** | `water_raw` siedzi w koszyku potrzeb, choć woda jest **mediem**; kiedy M8 zbuduje wodociąg, ta pozycja z `data/chains/needs.ron` znika. `SectorId::is_municipal` wskazuje zakłady miejskie |
| **M11** | Nakładka `land_value` jako wzorzec wpisu w `data/ui/overlays.ron` i `loc_key`, pod który podłączy się `engine/ui` z M3 |
