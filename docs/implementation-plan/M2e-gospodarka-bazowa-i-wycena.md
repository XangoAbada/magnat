# M2e — Gospodarka bazowa i wycena

Podfaza 5 z 5 fazy **M2 — Miasto statyczne** (`M2-miasto-statyczne.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M2d (budynki), M2c (dzielnice), M2a (pola skalarne). |
| **Pakiety robocze** | WP13, WP14, WP15, WP16, WP17 |
| **Projekt techniczny** | §5.7, §5.8 |
| **Wynik do pokazania** | Pełny artefakt fazy z §1 dokumentu fazy: `GenerationReport`, nakładka wartości gruntu z rozbiciem na czynniki, zielone `--test consistency`. |
| **Kryterium zamknięcia** | Kryteria WP13–WP17 oraz bramki 1–7 fazy M2 w `00-postep.md`. |
| **Poprzednia / następna** | `M2d-zabudowa.md` · — (ostatnia w fazie) |

Etap 7 i domknięcie fazy: archetypy zakładów, obsada budynków firmami-danymi, algorytm domknięcia łańcuchów produktowych, trzy przebiegi wyceny gruntu, nakładka UI i testy spójności Etapu 10.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Opis | Kryterium ukończenia |
|---|---|---|---|---|
| WP13 | Archetypy zakładów i szablony łańcuchów | WP12 | `data/buildings/` (typy z PRD §7.2), `data/chains/`, obsada stref | każda zabudowana parcela niemieszkalna ma przypisany `SiteSeed` lub jest `Institutional`/`Green` |
| WP14 | Domknięcie łańcuchów produktowych | WP13 | `supply_closure_check` + naprawa (import / dostawienie zakładu); walidator CI grafu produktów | `missing == []`; podaż/popyt w [0,85; 1,30] dla każdego towaru; **test negatywny: katalog, w którym towar jest osiągalny wyłącznie z `initial_stock.ron`, musi oblać walidację** (zapas startowy nie jest źródłem — 5.8) |
| WP15 | Wycena gruntu (3 przebiegi) | WP2, WP7, WP14 | `land_value_pass_0/1/2`, `LandValueBreakdown` | monotoniczność: średnia wartość w `OldTown` > `Suburb`; wartość przy przemyśle ciężkim < średniej dzielnicy; przebiegi deterministyczne |
| WP16 | Nakładka UI + karta inspekcji | WP15, M1 (`render`) | tekstura pola skalarnego na terenie, legenda, karta parceli/budynku | klik na parcelę pokazuje wartość i rozbicie na ≥ 8 czynników; 60 FPS z włączoną nakładką |
| WP17 | Testy spójności + raport generacji | WP14, WP15 | 12 testów z sekcji 7, `GenerationReport`, `world_hash_m2` | wszystkie testy zielone dla 32 seedów × 4 profile w CI (miasto małe) i 4 seedów (metropolia, nocne CI) |

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.7 Statyczna wycena gruntu (§6.7, część statyczna)

Problem kolejności: strefowanie potrzebuje wartości, wartość potrzebuje miejsc pracy,
miejsca pracy potrzebują budynków, budynki potrzebują wartości. Rozwiązanie: **trzy
przebiegi o ustalonym zakresie, bez iteracji do zbieżności** (determinizm + budżet czasu).

| Przebieg | Kiedy | Czynniki | Odbiorca |
|---|---|---|---|
| `pass_0` | przed Etapem 4 | `d_center`, `d_gate_*`, `amenity`, `slope`, `flood_risk` | punktacja stref (5.3) |
| `pass_1` | po Etapie 5 | + klasa drogi frontowej, długość frontu, `noise`, powierzchnia i kształt działki, `epoch_ring` | wybór gramatyki, liczba kondygnacji, `rent_hint` |
| `pass_2` | po Etapie 7 | + `job_access`, `retail_access`, `service_access`, prestiż dzielnicy, kara za sąsiedztwo `IndustryHeavy`/`Extraction` | UI, karta inspekcji, wejście dla M5/M8/M10 |

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
