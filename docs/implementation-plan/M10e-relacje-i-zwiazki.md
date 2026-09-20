# M10e — Relacje i związki

Podfaza 5 z 6 fazy **M10 — Głębia** (`M10-glebia.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M7 (firmy, płace), M8 (wyzwalacz zdarzeniowy — K-9). |
| **Pakiety robocze** | WP10.13, WP10.14 |
| **Projekt techniczny** | §5.9 |
| **Wynik do pokazania** | Strajk powstaje z żądania płacowego wyliczonego z danych M7 i skutkuje po stronie miasta przez wyzwalacz M8. |
| **Kryterium zamknięcia** | Kryteria WP10.13 i WP10.14. |
| **Poprzednia / następna** | `M10d-gielda-przejecia-ubezpieczenia.md` · `M10f-kroniki-i-domkniecie.md` |

Relacje międzyfirmowe oraz związki zawodowe i strajki — mechanika negocjacji należy tutaj, nie do M7 ani M8.

---

## Pakiety robocze

### WP10.13 — Relacje międzyfirmowe

**Zależności:** M6 (kontrakty), M7 (firmy AI), M8 (regulator).
**Kryterium ukończenia:** `SupplierRelation.trust` rośnie z historii terminowych dostaw i realnie
zmienia wybór dostawcy (firma płaci +3% stałemu dostawcy zamiast szukać taniej — widoczne w
`DecisionReason`). Kartel obniża wolumen i podnosi cenę; hazard wykrycia rośnie z odchyleniem ceny
od benchmarku; po wykryciu kara i uderzenie w markę wszystkich członków.
**Rozmiar: L.**

---

### WP10.14 — Związki zawodowe i strajki

**Zależności:** M7 (HR, płace, księgowość), M3 (graf relacji).
**Kryterium ukończenia:** zakład o marży 35% płacący 20% poniżej mediany miejskiej dla roli, z gęstą
siecią relacji między pracownikami, formuje związek w ciągu 3–9 miesięcy gry. Strajk zatrzymuje
produkcję zakładu (nie firmy), uruchamia kary z kontraktów M6 u odbiorców i jest widoczny w mediach.
Fundusz strajkowy wyczerpuje się i wymusza rozstrzygnięcie — brak strajków wiecznych (test: 100
strajków w balansatorze, mediana czasu trwania 4–21 dni, maksimum < 90 dni).
**Rozmiar: M.**

---

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.9 Relacje międzyfirmowe, związki — struktury

```rust
pub struct SupplierRelation {
    pub buyer: FirmId, pub supplier: FirmId, pub good: GoodId,
    pub since: SimMinute, pub volume_cum: Qty,
    pub trust: Q,                       // z historii terminowości i jakości
    pub discount_bps: u16, pub priority: u8,
    pub exclusivity: Option<Exclusivity>,
}

pub struct Cartel {
    pub id: CartelId, pub members: SmallVec<[FirmId; 8]>, pub good: GoodId,
    pub floor_price: Money,
    pub quota: SmallVec<[(FirmId, Qty); 8]>,
    pub formed: SimMinute, pub secrecy: Q,
}

pub struct Union {
    pub id: UnionId, pub scope: UnionScope,   // Site | Firm | Branch
    pub members: Vec<CitizenId>, pub density: Q,
    pub militancy: Q, pub strike_fund: Money,
    pub leader: CitizenId,
    pub demand: Option<UnionDemand>,
    pub state: UnionState,                    // Uśpiony | Żądanie | Negocjacje{runda} | Strajk{od} | Ugoda
}
```

**`SupplierRelation` jest mój, ale liczby karmiące `trust` są M6.** M6 jawnie odmówił własności
tego typu (relacja to §7.9, czyli mój zakres) i konsumuje z niego **dokładnie dwa pola**:
`discount_bps` (korekta ceny przy rozstrzyganiu RFQ) i `priority` (kolejność przydziału masy,
gdy dostawca nie ma dość dla wszystkich — to jest mechanizm „stały dostawca ma priorytet
w niedoborze" z §7.9). `trust`, `since`, `volume_cum` i `exclusivity` są wyłącznie moje.

**Nie buduję własnego licznika opóźnień.** `trust` liczę z `SupplyContract` M6:
`late_deliveries` i `missed_mass`. Uwaga, która kosztowałaby inaczej dzień debugowania: kontrakt
M6 ma `grace_minutes`, więc „spóźnione" i „spóźnione **ponad tolerancję**" to dwie różne liczby —
`trust` karze za tę drugą. Własny licznik obok licznika M6 rozjechałby się przy karach umownych.

Degradacja jest łagodna w obie strony: dopóki `SupplierRelation` nie istnieje, M6 liczy RFQ bez
rabatu i bez priorytetu, więc M10 nie blokuje M6, a M6 nie blokuje M10.

**Hazard wykrycia kartelu (miesięczny).** Kartel nie ginie od rzutu kostką, tylko od własnej chciwości:

```
h = 0,5%                                       // baza
  + 0,3% × (liczba_członków − 3).max(0)        // każdy dodatkowy członek to dodatkowe usta
  + 2,0% × (odchylenie_ceny_od_benchmarku% / 10)   // im więcej kradniesz, tym bardziej widać
  + 0,2% × liczba_niezadowolonych_wtajemniczonych  // menedżer z nastrojem < −40 lub zwolniony
  × aktywność_regulatora                       // z M8, 0,5..2,0
```

Kara: 10% obrotu 12-miesięcznego per członek (ograniczone wypłacalnością), rozwiązanie kartelu,
**uderzenie w markę: afinitet −20 u każdego mieszkańca posiadającego slot tej marki** (to jest
miejsce, gdzie dwa systemy tej fazy spotykają się i dają emergencję), 24 miesiące karencji.
Kartel trafia do kroniki **dopiero po wykryciu** — inaczej kronika spoileruje graczowi tajemnicę.

**Formowanie związku.** Trzy warunki naraz, wszystkie z §6.6:

```
grievance = clamp( w1 × (zysk_na_pracownika vs udział_płac_docelowy)
                 + w2 × (mediana_płacy_miejskiej_dla_roli − płaca_tutaj) / mediana
                 + w3 × średni_stres
                 + w4 × wypadki_12m, 0, 100)
```
1. `grievance ≥ 55` przez ≥ 60 kolejnych dni,
2. największa spójna składowa grafu relacji **wśród pracowników zakładu** ≥ max(8, 25% załogi)
   (przeszukiwanie ograniczone do załogi — kilkadziesiąt wierzchołków, koszt pomijalny),
3. gęstość potencjalnego członkostwa ≥ 30%.

Żądanie jest zakotwiczone na **znanych** płacach porównywalnych firm — czyli na tym, co pracownicy
wiedzą z grafu relacji i plotki (§5.7), a nie na prawdziwej medianie. Związek może żądać za dużo albo
za mało, bo ma niepełną informację. To jest realizm, który wychodzi z systemu, nie z parametru.

**Negocjacje i strajk.** Rundy tygodniowe; firma kontruje na podstawie swojej sytuacji finansowej
z ksiąg M7; próg akceptacji związku maleje wraz z wyczerpywaniem funduszu strajkowego. Strajk
zatrzymuje produkcję **zakładu** (nie całej firmy), uruchamia kary z kontraktów B2B M6 u odbiorców
(kaskada!), jest publikowany przez media, uderza w markę pracodawcy. Fundusz się kończy — nie ma
strajków wiecznych.


---

## Zmiany wpisane po M10b

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M10b.
Szczegóły — tabela `F-n` w `M10b-marka-i-media.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| FE-1 | **Graf relacji ma już wyjście na zewnątrz `sim/agents`: `social::relations_of(world, citizen) -> Vec<(Entity, u8)>`.** Zwraca drugą stronę relacji i jej wagę, w kolejności slabu | Powstało dla kampanii PR, która przechodzi po relacjach z góry (`magnat_media`). Związki zawodowe robią to samo z tego samego miejsca — **nie ma potrzeby pisać drugiego przejścia po slabie**, a dwa przejścia o tej samej regule rozjechałyby się przy pierwszej zmianie wagi relacji |
| FE-2 | **`SocialIndex::coworkers(site)` mierzy wreszcie to, co obiecuje** — ale to zasługa `K-74`, nie M10b; tutaj tylko potwierdzenie, że nic tego nie cofnęło | §5.9 liczy warunek powstania związku na spójnej składowej grafu relacji **wśród pracowników zakładu**. Uczeń wypadł z `by_site` i ma własny indeks (`DK-8`) |
| FE-3 | **Strajk ma już nośnik po stronie opinii: marka firmy.** Kampania PR, publikacja o strajku i rozczarowanie klienta piszą do tego samego slotu (`BrandAffinity`) | M10 §1 obiecuje kaskadę „strajk → gazeta pisze → marka gracza traci afinitet". Dwa z trzech ogniw są gotowe: publikacja (`Story`, M10b) i afinitet (`Touch::Media`). Brakuje wyłącznie zdarzenia strajku jako wejścia do redakcji — czyli tego, co robi ta podfaza |
| FE-4 | **Blok `StreamId` M10: zajęte 280–284 i 292–295; wolne 285–291 i 296–299.** `CartelDetection = 289`, `UnionFormation = 290`, `StrikeResolve = 291` są nadal wolne i zarezerwowane imiennie | — |
