# M10 — Głębia

Status: plan fazy. Podlega `00-konwencje-i-kontrakty.md` (typy bazowe, determinizm, LOD, dane, testy).
Źródło wymagań: `PRD_Magnat.md` §4.2 (Etap 9–10), §5.7, §6.5, §6.6, §7.6, §7.7, §7.9, §11.2–11.3, §12.3–12.4, §16.2, §17.4, §17.7, §19.

Właściciel crate'u: `sim/macro` (pełna postać). Rozszerza: `sim/firms`, `sim/economy`, `sim/agents` (pamięć), `sim/events`.

---

## 1. Cel fazy i artefakt końcowy

Po M9 istnieje żywe miasto z firmami AI, rynkiem pracy, miastem-aktorem i graczem-karierowiczem, ale
**świat startuje sterylny** (generator ustawia go „na zero") i **firmy są płaskie** — nie mają marki poza
liczbą, nie mają technologii, nie mają właścicieli innych niż jeden mieszkaniec, nie mają historii relacji.

M10 domyka trzy braki:

1. **Świat zaczyna się „zużyty".** `sim/macro` przeprowadza skróconą symulację 30–100 lat przed startem
   partii i ustala ceny wyjściowe, zapasy, zadłużenie firm, majątki rodzin, sieci stałych dostawców
   i historię dzielnic (upadek przemysłu, gentryfikacja). Ten sam kod obsługuje „co jeśli" firm AI (M7)
   i tryb 50× (M12) — to jest jeden model, nie trzy.
2. **Marka przestaje być liczbą.** Jest zbiorem afinitetów w pamięci konkretnych mieszkańców; reklama
   dociera do konkretnych ludzi, po konkretnych trasach i kanałach, a rozczarowanie niszczy markę
   szybciej, niż reklama ją buduje.
3. **Firmy zyskują warstwy wieloletnie:** R&D i epoki, giełda z wyceną z transakcji, media jako firmy
   kształtujące opinię, relacje międzyfirmowe (stali dostawcy, kartele, franczyza, JV), związki zawodowe
   i strajki, ubezpieczenia wyceniające ryzyko z historii zdarzeń.

### Artefakt końcowy (co da się uruchomić i zobaczyć)

**A. `tools/headless --dry-run --years 80 --seed X --size metropolia`** — generuje świat i wypisuje raport:
oś czasu 80 lat (kiedy powstała huta, kiedy upadła, kiedy Wzgórza Zachodnie przestały być robotnicze),
tabelę cen startowych, rozkład majątków, mapę relacji dostawców, listę 200 wpisów kronikarskich,
oraz **zielony raport Etapu 10** (żaden rynek w nierównowadze > 30%). Czas: ≤ 60 s jednowątkowo.

**B. `tools/headless --lod-consistency`** — ten sam scenariusz w mezo i w makro przez 12 miesięcy gry (360 dni, kalendarz K-1);
raport różnic agregatów pieniężnych per dzielnica z werdyktem PASS/FAIL przy tolerancji z §7.

**C. W grze:** gracz otwiera panel marketingu, kupuje billboard przy konkretnej ulicy i widzi na mapie
cieplnej, **którzy mieszkańcy** (ilu, z jakich dzielnic) zostali w tym tygodniu wyeksponowani, a po
miesiącu widzi w karcie inspekcji mieszkańca wpis „znam markę «Orlex» — z billboardu na Wołoskiej,
oczekiwana jakość 72, doświadczenie 58, afinitet −11 (rozczarowanie)".

**D. W grze:** firma gracza wchodzi na giełdę, notowana jest w codziennym fixingu, konkurent AI skupuje
akcje, przekracza 5% (komunikat → plotka → media), przekracza 50% i przejmuje kontrolę. Załoga fabryki
gracza formuje związek, przedstawia żądanie, negocjacje padają, produkcja staje, kary z kontraktów B2B
uruchamiają kaskadę u odbiorców, gazeta pisze o strajku, marka gracza traci afinitet.

---

## 2. Zakres — wchodzi / nie wchodzi

### Wchodzi

| # | Temat | PRD |
|---|---|---|
| 1 | `BrandAffinity` w pamięci mieszkańców, agregat „siła marki" tylko w UI | §7.6 |
| 2 | Kampanie i kanały: billboard (zasięg z tras), prasa/radio/TV (czytelnictwo dzielnic), ulotki, promocje, sponsoring, PR | §7.6 |
| 3 | Asymetria: reklama → znajomość + oczekiwana jakość; rozczarowanie → szybka utrata afinitetu | §7.6 |
| 4 | R&D: dział, punkty badawcze, `TechTree` per branża, `Patent`, licencjonowanie | §7.7 |
| 5 | Technologie „światowe" pojawiające się w czasie, epoki, nowe kategorie produktów zmieniające potrzeby i łańcuchy | §7.7, §11.3 |
| 6 | Giełda: IPO, `OrderBook` z fixingiem, `Valuation` z opóźnionych i zaszumionych wyników + plotek | §6.5 |
| 7 | Dywidendy, emisje, przejęcia wrogie i przyjazne, ład korporacyjny | §6.5, §7.9 |
| 8 | Ubezpieczenia: `InsurancePolicy`, wycena z historii zdarzeń, wypłaty, niewypłacalność ubezpieczyciela | §6.5, §11 |
| 9 | Media jako firmy: sprzedaż powierzchni reklamowej, `Story` → plotka i opinia | §7.2, §7.6, §11.2 |
| 10 | Relacje międzyfirmowe: stali dostawcy, ekskluzywność, `Cartel` z ryzykiem kary, franczyza, JV, integracja pionowa/pozioma | §7.9 |
| 11 | `Union`: formowanie żądania z rozdźwięku zysk/płace + sieci relacji, negocjacje, strajk | §6.6, §11.2 |
| 12 | `sim/macro` w pełnej postaci: `MacroCell`, `step`, `lift`/`lower` | §16.2, §17.4 |
| 13 | Historia „na sucho": `DryRunConfig`, etapy D0–D5, rozwinięcie do pełnego świata, weryfikacja Etapu 10 | §4.2 |
| 14 | Kroniki: nowe warianty zdarzeń z powyższych systemów + wpisy z dry-runu | §14, §19 |

### Nie wchodzi

| Temat | Gdzie |
|---|---|
| Podstawowa mechanika firmy, zakładu, produkcji, HR, polityk cenowych, osobowości firm AI | M7 — M10 **rozszerza**, nie redefiniuje |
| Rynek pracy jako taki (oferty, aplikacje, licytacja płac) | M7 — M10 dokłada wyłącznie warstwę związkową |
| Podatki, urzędy, regulator antymonopolowy jako organ, prawo, wybory | M8 — M10 **konsumuje** hook regulatora do wykrywania karteli |
| Kontrakty B2B, kary umowne, rynek spot | M6 — M10 konsumuje (franczyza i licencja to `ContractId`) |
| Panele UI, wykresy, tabele, edytor reguł | M9 — M10 zgłasza wymagania (§6) |
| Kronika jako podsystem (przechowywanie, indeks, wyszukiwarka) | M9 — M10 dopisuje warianty zdarzeń |
| Tryb 50× jako zagadnienie wydajnościowe, zapis w tle, profilowanie | M12 — M10 dostarcza mu `sim/macro` jako silnik |
| Bank centralny, stopa bazowa, kredyt | M5/M7 — M10 konsumuje stopę do wyceny |
| Zdarzenia losowe i pogoda | M8 — M10 konsumuje strumień zdarzeń do wyceny ubezpieczeń |

---

## 3. Mapowanie na PRD

| Sekcja PRD | Co z niej realizuje M10 |
|---|---|
| §4.2 Etap 9 | Historia „na sucho" — WP10.3 |
| §4.2 Etap 10 | Weryfikacja i naprawa świata startowego — WP10.3, §7 |
| §5.1 Pamięć | Rozszerzenie magazynu doświadczeń o sloty marek — WP10.5 |
| §5.7 Informacja i plotka | Reklama i media jako czwarte źródło wiedzy obok wizyt, relacji i tras — WP10.6, WP10.7 |
| §6.4 Decyzja zakupowa | Wypełnienie składników `w_marka · afinitet_do_marki` i `jakość_postrzegana` realnymi danymi — WP10.5 |
| §6.5 Giełda, ubezpieczenia | WP10.10–10.12 |
| §6.6 Związki zawodowe i strajki | WP10.14 |
| §7.4 Produkt | Cechy specjalne z R&D — WP10.8 |
| §7.6 Marketing i marka | WP10.5, WP10.6 |
| §7.7 Badania i rozwój | WP10.8 |
| §7.8 Finanse firmy | Emisja akcji jako realne źródło kapitału — WP10.11 |
| §7.9 Relacje międzyfirmowe | WP10.13 |
| §11.2 Społeczne, firmowe | Strajk, protest, skandal, moda, recenzja w mediach jako wejścia do marki i plotki — WP10.7 |
| §11.3 Postęp technologiczny epoki | WP10.9 |
| §12.3 Poziom strategiczny | `sim/macro` jako silnik „co jeśli" dla firm AI — WP10.1 |
| §12.4 Powstawanie i upadek | Przejęcia jako druga (obok bankructwa) ścieżka wyjścia firmy — WP10.11 |
| §16.2 crate `macro` | WP10.1 |
| §17.4 LOD makro | WP10.1, WP10.2, WP10.4 |
| §17.7 Pamięć | Budżet pamięci marki — §5.1 tego dokumentu |
| §19 M10 | Cały dokument |

---

## 4. Pakiety robocze i podfazy

Ścieżka krytyczna: **WP10.2 → WP10.1 → WP10.3 → WP10.4**. Ona blokuje M12 (tryb 50×) i zamyka
Etap 9–10 generatora. Reszta jest równoległa i może iść w dowolnej kolejności po spełnieniu zależności.

Faza jest rozbita na **6 podfaz**. Podfaza to porcja, którą da się zacząć i zamknąć
bez trzymania w głowie całej fazy: własny zestaw WP, własny sprawdzalny wynik i własny
wycinek projektu technicznego. Opis pakietów i sekcje §5 mieszkają teraz w dokumentach
podfaz — poniższa tabela mówi, gdzie co jest. Bramki 1–7 z `00-postep.md` zamykają się
dopiero po ostatniej podfazie; podfaza zamyka się własnym kryterium ze swojego dokumentu.

| Podfaza | WP | §5 | Wynik do pokazania | Dokument |
|---|---|---|---|---|
| **M10a — Jądro, makro, historia na sucho** | WP10.2, WP10.1, WP10.3, WP10.4 | 5.6, 5.7, 5.8 | Świat startuje z 30-letnią historią policzoną w makro; test spójności makro↔mezo zielony. | `M10a-jadro-makro-historia.md` |
| **M10b — Marka i media** | WP10.5, WP10.6, WP10.7 | 5.1, 5.2, 5.3 | Marka rośnie z faktycznych doświadczeń zakupowych, a kampania przesuwa udział w rynku w mierzalny sposób. | `M10b-marka-i-media.md` |
| **M10c — R&D i nowe produkty** | WP10.8, WP10.9 | 5.4 | Technologia odblokowuje kategorię towaru, patent daje wyłączność, licencja ją sprzedaje. | `M10c-rd-i-nowe-produkty.md` |
| **M10d — Giełda, przejęcia, ubezpieczenia** | WP10.10, WP10.11, WP10.12 | 5.5 | Notowanie powstaje z arkusza zleceń, a wrogie przejęcie da się przeprowadzić i obronić. | `M10d-gielda-przejecia-ubezpieczenia.md` |
| **M10e — Relacje i związki** | WP10.13, WP10.14 | 5.9 | Strajk powstaje z żądania płacowego wyliczonego z danych M7 i skutkuje po stronie miasta przez wyzwalacz M8. | `M10e-relacje-i-zwiazki.md` |
| **M10f — Kroniki i domknięcie** | WP10.15, WP10.16 | 5.10 | Pełny artefakt fazy z §1 dokumentu fazy: marka z pamięci agentów, R&D, giełda, historia „na sucho”. | `M10f-kroniki-i-domkniecie.md` |

---

## 5. Projekt techniczny

Treść przeniesiona do dokumentów podfaz. **Numeracja `5.x` jest zachowana**, więc
odesłania w tekście („patrz §5.4") nadal wskazują tę samą sekcję — zmienił się tylko plik.

| § | Temat | Dokument |
|---|---|---|
| 5.1 | Marka — struktury i rachunek pamięci | `M10b-marka-i-media.md` |
| 5.2 | Kampanie i zasięg | `M10b-marka-i-media.md` |
| 5.3 | Media | `M10b-marka-i-media.md` |
| 5.4 | R&D, technologie, epoki | `M10c-rd-i-nowe-produkty.md` |
| 5.5 | Giełda, ubezpieczenia | `M10d-gielda-przejecia-ubezpieczenia.md` |
| 5.6 | `sim/macro` — stan i krok | `M10a-jadro-makro-historia.md` |
| 5.7 | Historia „na sucho" — etapy | `M10a-jadro-makro-historia.md` |
| 5.8 | Tożsamość w stanie uśpionym i rozwinięcie makro→mezo | `M10a-jadro-makro-historia.md` |
| 5.9 | Relacje międzyfirmowe, związki — struktury | `M10e-relacje-i-zwiazki.md` |
| 5.10 | Systemy ECS i ich częstotliwość | `M10f-kroniki-i-domkniecie.md` |

---

## 6. Kontrakty międzyfazowe

### Dostarczam

#### API crate'u `sim/macro` — kontrakt wiążący

Dok. 00 §1 przyznaje własność crate'u `sim/macro` fazie M10. **Poniższe sygnatury są kontraktem, nie
propozycją.** M7 tworzy wyłącznie warstwę „co jeśli" **na tym API** i nie definiuje własnego stanu
makro ani własnej agregacji dzielnic.

```rust
// ── rdzeń: stan i krok ────────────────────────────────────────────────
pub fn step(state: &mut MacroState, params: &MacroParams);

// ── most mezo↔makro ───────────────────────────────────────────────────
pub fn lift(world: &World) -> MacroState;
pub fn lower(state: &MacroState, world: &mut World, seed: u64) -> Result<(), LowerError>;

/// Czysty rdzeń rozwinięcia: bez stanu ukrytego, bez dostępu do World.
/// To jest miejsce, w którym replay i zapis mogą się rozjechać — dlatego jest funkcją czystą
/// i jest testowane osobno. `lower()` jest tylko sterownikiem wołającym to per komórka.
pub fn lower_cell(cell: &MacroCell, seed: u64, tick: Tick) -> CellExpansion;

// ── tryby (ten sam `step`, różne wejścia) ─────────────────────────────
pub fn dry_run(cfg: &DryRunConfig, seed_world: &SeedWorld) -> DryRunResult;       // M10
pub fn what_if(base: &MacroState, sc: &Scenario, horizon_days: u16) -> MacroOutcome; // M7
pub fn fast_forward(world: &mut World, cfg: &FastForwardConfig) -> FastForwardReport; // M12

// ── polityka LOD (M12 dostarcza wywołanie i watchdog, M10 — matematykę) ──
pub struct MacroLodPolicy { /* progi prędkości gry, histereza, warunki blokady przejścia */ }
impl MacroLodPolicy {
    pub fn target_lod(&self, speed: GameSpeed, load: &LoadStats) -> Lod;
    /// Czy wolno teraz przejść na `target`? Blokuje w środku fixingu, negocjacji, strajku.
    pub fn may_transition(&self, from: Lod, to: Lod, world: &World) -> Option<BlockReason>;
}
```

**Podzbiór, który musi być gotowy już w M7** (reszta może powstać w M10):

| Element | Wymagany w M7 | Uzasadnienie |
|---|---|---|
| `MacroState`, `MacroCell`, `MacroFirm` | **tak** — pełne definicje pól | M7 nie może zbudować „co jeśli" bez stanu |
| `lift()` | **tak** | M7 musi umieć zrobić zdjęcie świata; **M7 nie pisze własnej agregacji** |
| `step()` | **tak**, fazy 2–6 (praca, produkcja, dobra, ceny, finanse) | to wystarcza dla horyzontu 4–12 kwartałów |
| `what_if()` | **tak** | to jest właśnie szkic M7 z dok. 00 §1 |
| `step()` fazy 1, 7, 8 (demografia, relacje, zdarzenia) | nie — M10 | nieistotne dla horyzontu kwartalnego |
| `lower()`, `lower_cell()` | nie — M10 | M7 nigdy nie rozwija „co jeśli" do świata, tylko czyta agregaty |
| `dry_run()`, `fast_forward()`, `MacroLodPolicy` | nie — M10 | |

`MacroFirm` **zachowuje `FirmId` także w trybie „co jeśli"** — M7 musi móc zadać pytanie
„co jeśli konkurent X obniży cenę o 8%", a to wymaga wskazania konkretnej firmy.

#### Ograniczenie kontraktu — co `what_if()` i `fast_forward()` gwarantują, a czego nie

Rozstrzygnięte, nie otwarte. To ostrzeżenie jest częścią kontraktu i obowiązuje każdego, kto sięga
po to API — w szczególności M7 przy AI strategicznym i M9 przy panelach gracza.

Model makro operuje na agregatach dzielnica × klasa, więc **prognoza losu pojedynczej firmy
z dokładnością procentową jest z niego nieosiągalna.** Nie jest to usterka do naprawienia
w implementacji: wariancja rozkładu wielomianowego przy ~200 klientach daje odchylenie udziału
rynkowego rzędu 3,5% i żaden wspólny kernel tego nie zdejmie (analiza w §7.3/K2).

**Do czego `what_if()` służy:**
- porównywanie wariantów strategii **między sobą** („czy ekspansja do dzielnicy B bije obniżkę ceny"),
- ocena stanu dzielnicy i miasta w horyzoncie 4–12 kwartałów,
- wykrywanie kierunku i znaku zmiany, nie jej wielkości.

**Czego z niego nie wolno zrobić:**
- **decyzji, które muszą być spójne co do grosza** — księgowanie, rozliczenie kontraktu, wypłata,
  podatek. Makro nigdy nie jest źródłem kwoty trafiającej do księgi;
- **liczb wyświetlanych graczowi w formie wyglądającej na precyzyjną.** Panel nie pokazuje
  „prognozowany zysk: 1 284 300 zł". Pokazuje przedział, ranking wariantów albo strzałkę kierunku.
  Fałszywa precyzja jest tu gorsza od braku prognozy, bo gracz jej uwierzy;
- **zdania „mój zysk za rok wyniesie X"** w żadnej formie, w UI ani w uzasadnieniu decyzji AI.

To samo ograniczenie dotyczy `fast_forward()` (M12): tryb 50× przewiduje stan dzielnicy i miasta,
**nie los pojedynczej firmy**. M12 potwierdził i zdjął to założenie z trzech miejsc u siebie.

**Wzorzec konsumpcji — kształt przyjęty przez M7, do naśladowania przez M9.** Zamiast wystawiać
`MacroOutcome` wprost, M7 trzyma go w polu prywatnym i udostępnia dwie rzeczy:

```rust
fn decisive_winner(&self) -> Option<VariantId>;  // zwycięzca TYLKO gdy przewaga > error_margin_bp
fn direction(&self) -> Trend;                    // kierunek, nie wielkość
```

`None` znaczy „nierozstrzygalne" i przekłada się na `KeepCourse` — a nie na wybór wariantu, który
przypadkiem wyszedł o 0,3% lepiej. To jest właściwa odpowiedź na pytanie „co pokazać zamiast
prognozy punktowej": ranking z jawnym marginesem i uczciwe „nie wiem", gdy margines go pochłania.
M9 powinien zrobić to samo w panelu — przedział albo strzałka, nigdy kwota.

Konsekwencja dla testów po stronie konsumenta: testuje się **uporządkowanie wariantów**, nie
dokładność bezwzględną. M7 sprawdza zgodność rankingu makro z przebiegiem mezo ≥ 95% tam, gdzie
różnica przekracza margines, uczciwość samego marginesu (≥ 90% odchyleń w deklarowanym paśmie)
oraz `decisive_winner() == None` w 100% przypadków nierozstrzygalnych.

**Margines nie jest parametrem konsumenta — jest obietnicą `sim/macro` i podaję go razem z wynikiem.**

```rust
pub struct MacroOutcome {
    /* ... */
    /// Deklarowany błąd TEGO wyniku: funkcja n komórki, horyzontu i liczby firm w porównaniu.
    /// Konsument nie zgaduje tej liczby i nie wolno mu jej nadpisać.
    pub error_margin_bp: u32,
}
```

Powód jest taki, jak ujął to M7: `decisive_winner()` jest tylko tak dobre, jak liczba, którą mu się
poda, a **zbyt wąski margines po cichu przywraca skokowe decyzje**, które to rozwiązanie miało
wyeliminować. Gdyby margines wybierał konsument, dobrałby go na oko albo — co gorsza — dostroił tak,
żeby jego AI wreszcie „podejmowało decyzje". Ja mam dane, żeby go policzyć: znam `n` komórki,
horyzont i liczbę porównywanych firm, czyli dokładnie te trzy rzeczy, od których zależy błąd (§7.3/K2).

Stąd nowe kryterium akceptacji **po mojej stronie**, nie tylko po stronie M7:

| Test | Próg |
|---|---|
| `macro_error_margin_is_honest`: odsetek faktycznych odchyleń makro vs mezo mieszczących się w deklarowanym `error_margin_bp` | **≥ 90%** |
| Margines nie jest zaniżony przez zawężenie: mediana `|odchylenie| / error_margin_bp` | **0,3–0,8** (poniżej 0,3 margines jest rozdęty i wszystko staje się nierozstrzygalne; powyżej 0,8 — zaniżony) |

Margines jest obietnicą, więc jest testowany jak obietnica. Rozdęty margines nie jest bezpieczną
stroną błędu — to `decisive_winner()`, które zawsze zwraca `None`, czyli API udające ostrożność
i bezużyteczne.

Konsekwencja dla testów: `lod_macro_aggregate_accuracy` **celowo nie obejmuje** wielkości o małym n —
zielone CI nie może sugerować gwarancji, której nie ma.
`MacroState` metropolii ≤ 8 MB — klonowanie pod równoległe scenariusze jest tanie (10 wariantów = 80 MB).

**Granica własności wobec M4 (`sim/traffic`).** Ta sama zasada co przy `econ_kernel` z WP10.2:
`sim/macro` nie zawiera logiki dziedzinowej. Podział:

| Co | Właściciel | Forma |
|---|---|---|
| Agregat kohortowy (dzielnica × klasa), pętla czasu, alokacja podróży do kohort | **M10 / `sim/macro`** | `MacroCell`, faza 2 i 4 kroku |
| **Model czasu przejazdu** i jego kalibracja z obserwacji mezo (§17.6) | **M4 / `sim/traffic`** — potwierdzone | `TravelTimeMatrix::lookup` (dzielnica × godzina, aktualizowana z obserwacji) |
| `CommuteMatrix` w `MacroState` | M10 trzyma, M4 wypełnia | snapshot `TravelTimeMatrix` z chwili `lift()` |

M4 potwierdził: `travel_time()` zostaje w `sim/traffic`, a `TravelTimeMatrix::lookup` jest moim
jedynym źródłem czasów przejazdu. **Nie buduję własnego modelu** — ani w makro, ani w dry-runie.

Czyli: makro ruchu **nie jest ani w całości moje, ani w całości M4** — jednostka pracy (kohorta,
podróż rozpoczęta w ticku) jest moja, wzór na czas przejazdu jest M4. Gdyby było inaczej, mielibyśmy
dwa modele czasu przejazdu i te same rozjazdy, którym zapobiega WP10.2 po stronie ekonomii.

**Dla M12 (tryb 50× i długie sesje):**
```rust
pub struct FastForwardConfig { pub days: u32, pub keep_identities: bool, pub events: bool }
```
Gwarancja: `fast_forward` **nie tworzy ani nie niszczy pieniądza i masy** (§7.3, punkt 1) —
to jest własność konstrukcyjna, niezależna od dokładności agregatów. M12 dostarcza wywołanie
z pętli gry i `SpeedGovernor`; matematykę przełączania daje `MacroLodPolicy` powyżej.
M12 nie potrzebuje własnej agregacji ruchu — dostaje ją z `MacroCell` i `travel_time()` M4.

**Dla M1/M2 (generator) i dla `game/` (start partii):**
```rust
pub fn dry_run(cfg: &DryRunConfig, seed_world: &SeedWorld) -> DryRunResult;
pub struct DryRunResult { pub state: MacroState, pub chronicle: Vec<ChronicleEvent>,
                          pub verification: VerificationReport, pub rebalance_rounds: u8 }
```

**Dla M3 (agenci) — rozszerzenie pamięci:**
```rust
pub fn brand_affinity(c: CitizenId, b: BrandId) -> Option<BrandAffinity>;  // z zanikiem leniwym
pub fn touch_brand(c: CitizenId, b: BrandId, t: Touch);                    // Ad|Experience|Rumor|Media
```

**Dla M5 (decyzja zakupowa) — wypełnienie §6.4:**
`purchase_score` dostaje realne `afinitet_do_marki` i `jakość_postrzegana` zamiast stałych.
Sygnatura `purchase_score` **nie zmienia się** — zmienia się tylko źródło danych w `ScoreCtx`.

**Dla M6 (łańcuch):** `SupplierRelation` jako wejście do wyboru dostawcy; licencja i franczyza
jako `ContractId`, bez nowego typu umowy.

**Dla M8 (miasto):** `Cartel` i `CartelDetection` jako klient regulatora; `PerilStats` jako konsument
strumienia zdarzeń.

**Dla M9 (UI) — wymagania, nie implementacja:**
1. Panel marketingu: mapa cieplna ekspozycji per dzielnica, lejek znajomość → próba → afinitet,
   wykres `expected_quality` vs `actual_quality` w czasie (to jedno miejsce, gdzie gracz zobaczy,
   że przereklamował produkt).
2. Panel R&D: drzewo technologii per branża z postępem i rokiem „światowym"; lista patentów własnych
   i cudzych z ofertami licencji.
3. Panel giełdy: księga zleceń przed fixingiem, historia kursu, akcjonariat z progami 5/25/50%,
   kalendarz publikacji wyników.
4. Panel relacji: graf dostawców z `trust` i ekskluzywnością; ostrzeżenie o ryzyku kartelu
   (gracz musi wiedzieć, że to nielegalne, zanim wejdzie).
5. Panel pracowniczy: `grievance` per zakład jako ostrzeżenie wyprzedzające, przebieg negocjacji,
   licznik funduszu strajkowego.
6. Karta inspekcji mieszkańca: sekcja „marki, które znam" — 16 slotów z afinitetem, oczekiwaną
   jakością i **źródłem** kontaktu. To jest główny dowód, że marka nie jest liczbą.
7. Kronika: filtr `provenance: DryRun` i oś czasu sprzed startu partii.

### Konsumuję

| Od | Co |
|---|---|
| M0 | `Money`, `Q`, `Tick`, `StreamId`, RNG, bufory komend, hash stanu |
| M2 | `engine/spatial` (ulotki), `DistrictId`, parcele |
| M3 | graf relacji, magazyn doświadczeń, plotka (§5.7), cechy osobowości, potrzeby |
| M4 | strumień przejazdów po krawędziach (zasięg billboardów), `CommuteMatrix` z §17.6 |
| M5 | `purchase_score`, oferty, budżety GD, majątek GD |
| M6 | `ContractId`, kary umowne, importer z warstwy 3; `GoodUnit { Grams, Milliunits }` z `data/goods/` (stabilny dla `key` w obrębie wersji danych); `SupplyContract::{late_deliveries, missed_mass}` jako jedyne źródło `trust` |
| M7 | `FirmId`, księgowość, raporty kwartalne, `FirmPersonality`, HR, płace, polityki cenowe |
| M8 | strumień zdarzeń (`sim/events`), stawki podatkowe, regulator, stopa bazowa |
| M9 | podsystem kroniki, `DecisionReason` w karcie inspekcji |

---

## 7. Testy i kryteria akceptacji

### 7.1 Determinizm (dok. 00 §3, DoD fazy)

- Nowe warianty `StreamId`, **zakres przydzielony M10: 280–299** (dok. 00 §3 — wartości istniejących
  wariantów nietykalne, M10 nie wychodzi poza swój zakres):

  | Wartość | Wariant | Użycie |
  |---|---|---|
  | 280 | `AdNotice` | czy mieszkaniec zauważył billboard |
  | 281 | `AdMediaPick` | dobór odbiorców reklamy prasowej/radiowej/TV z czytelnictwa |
  | 282 | `AdLeaflet` | dobór odbiorców ulotek w promieniu |
  | 283 | `Rumor` | propagacja plotki i PR przez graf relacji |
  | 284 | `MediaEditorial` | dobór zdarzeń do publikacji przez redakcję |
  | 285 | `RnDBreakthrough` | przełom skracający koszt węzła technologii |
  | 286 | `InvestorNoise` | szum wyceny per (inwestor, firma, miesiąc) |
  | 287 | `EarningsNoise` | szum publikowanych wyników kwartalnych |
  | 288 | `PerilDraw` | realizacja szkody ubezpieczeniowej |
  | 289 | `CartelDetection` | hazard wykrycia kartelu |
  | 290 | `UnionFormation` | formowanie związku i treść żądania |
  | 291 | `StrikeResolve` | rozstrzygnięcie rundy negocjacyjnej |
  | 292 | `MacroStep` | losowość wewnątrz kroku makro |
  | 293 | `MacroLower` | porządek rangowy przy rozwijaniu komórki |
  | 294 | `MacroSeedMemory` | zasiew pamięci doświadczeń po `lower` (D9) |
  | 295 | `DryRunEvent` | zdarzenia w historii „na sucho" |
  | 296–299 | rezerwa M10 | |

- **Kalendarz K-1: 360 dni (12 × 30).** Obowiązuje w epokach (`data/epochs/`), harmonogramie
  publikacji wyników (T+45 dni = 1,5 miesiąca), oknie statystyk ubezpieczeniowych (60 miesięcy
  = 1800 dni), karencji kartelowej (24 miesiące) i w liczbie kroków dry-runu (§5.7).
  Żadnego `365` w kodzie M10 — lint CI.
- Wszystkie nowe komponenty dopisane do funkcji haszującej stan ECS.
- Dwa przebiegi tego samego seeda przez 100 tys. ticków → identyczny ciąg hashy.
- Dwa przebiegi dry-runu 80 lat → identyczny hash `MacroState` i identyczna kronika.
- `lower()` dwukrotnie z tego samego stanu → identyczny świat (test na hash po rozwinięciu).
- Audyt: zero iteracji po `HashMap` w nowym kodzie (lint CI).

### 7.2 Własnościowe (dok. 00 §6)

| Test | Tolerancja |
|---|---|
| Suma pieniądza = emisja − destrukcja, po każdym kroku makro | **0 groszy** |
| `lift(lower(s)) == s` na polach `Money` | **0 groszy** |
| Suma akcji w obiegu == `Share.total_shares` po fixingu/emisji/przejęciu | **0 akcji** |
| Suma wypłaconej dywidendy == kwota uchwalona | **0 groszy** |
| Racjonowanie na fixingu: Σ przydziałów == wolumen | **0 akcji** |
| Suma składek i wypłat ubezpieczeniowych zgodna z księgą | **0 groszy** |
| Suma masy towaru w makro = produkcja − konsumpcja − straty | **0** |

### 7.3 Kontrakt LOD makro↔mezo — cztery kryteria z dok. 00 §4

Dok. 00 §4 został doprecyzowany: dla pary **mikro↔mezo** obowiązuje tolerancja 0 (zakres M4);
dla pary **makro↔mezo** obowiązuje kontrakt słabszy, ale z twardym rdzeniem. M10 przyjmuje te cztery
punkty jako kryteria akceptacji fazy w miejsce własnych tolerancji.

**Scenariusz referencyjny** (deterministyczny, zamknięty, bez zdarzeń losowych): 1 dzielnica,
2000 mieszkańców, 12 firm, 8 towarów, 2 banki. Przebieg (a) w pełnym mezo, przebieg (b) w makro
po `lift` stanu początkowego. Horyzont: 12 miesięcy (360 dni) per-commit, 100 lat nocnie.

Scenariusz **przechodzi przez `cell_grain()` jak każdy inny świat** — przy 2000 mieszkańcach
i 1 dzielnicy daje to `Classes3`, czyli 3 komórki po 666 osób. Gdyby wymusić na nim stałe
6 klas, wyszłoby 333 osoby na komórkę i **test referencyjny łamałby próg, który sam weryfikuje.**
Zapisuję to jawnie, bo jest to dokładnie ten błąd, który zrobiłem w pierwszej wersji dokumentu.

**Scenariusz nie jest jeden.** Testy kontraktowe LOD i dry-runu przebiegają na **pełnym zakresie
rozmiarów z §4.1** (20 tys. → 400 tys.), nie na jednym referencyjnym. Powód jest ten sam, który M12
zapisał u siebie jako R1d: faza, której scenariusze mają jedną skalę, testuje swoje skrzywienie
zamiast swojego kontraktu. U mnie najciaśniej jest **na małym mieście** — tam zapasu nad progiem
`MIN_CELL_POP` nie ma żadnego — a wszystkie moje intuicje liczbowe pochodzą z metropolii
(400 tys., 240 komórek, 51,2 MB slotów, 8 MB `MacroState`). Metropolia jest w tej fazie przypadkiem
wygodnym, nie brzegowym.

Reguła, z furtką — bez niej byłaby dogmatem i kosztowałaby czas na testach, które rozmiaru nie widzą:

> Test kontraktowy M10 na jednym rozmiarze miasta jest błędem projektu testu — **chyba że da się
> wskazać powód, dla którego weryfikowany kontrakt jest niewrażliwy na rozmiar.**

Który test czego wymaga:

| Test | Rozrzut rozmiarów | Powód |
|---|---|---|
| K2, K3 (dokładność, dryf) | **wymagany** | próg n zależy wprost od populacji na komórkę |
| Dry-run, Etap 10, ziarno | **wymagany** | bramki i ziarno skalują się z populacją |
| Budżety pamięci i czasu | **wymagany** | to są liczby per mieszkaniec i per komórka |
| K1 (zachowanie pieniądza i masy) | **nie** | niezmiennik księgowy, nie ma progu populacyjnego |
| K4 (`lift(lower(s)) == s`, czystość `lower_cell`) | **nie** | transformacja, prawdziwa dla każdej komórki osobno |
| Determinizm (hash, dwa przebiegi) | **nie** | własność RNG i kolejności, nie skali |
| Asymetria marki, fixing, podział dywidendy | **nie** | testy jednostkowe na liczbach, świat nieistotny |

Trzy pierwsze wiersze to dokładnie te, w których mój pierwszy dokument był skalibrowany od złej
strony zakresu. Cztery ostatnie mają powód i dlatego zostają małe.

#### K1 — Zachowanie pieniądza i masy co do grosza i grama, bez wyjątków

**Przyjęte bez zastrzeżeń. To jest własność konstrukcyjna, nie statystyczna** — i dlatego jest
osiągalne mimo przybliżonych agregatów. Dwie rzeczy są tu ortogonalne:
- **dokładność** mówi, *komu* przypadł pieniądz (makro zgaduje z rozkładu — stąd ≤ 0,5%),
- **zachowanie** mówi, *ile go jest w sumie* (makro nie zgaduje — księguje).

Każdy przepływ w makro przechodzi przez `ledger_post()` z WP10.2, czyli przez ten sam zapis
dwustronny co mezo: pieniądz zawsze przechodzi **między kontami**, nigdy nie powstaje przy alokacji.
Dzielenie kwoty agregatu na jednostki zawsze domyka się korektą reszty do pierwszego wg posortowanego
klucza (dok. 00 §2, mechanizm w §5.8). Ten sam schemat dla masy: `Qty` jest i64, a każda alokacja
towaru między odbiorców kończy się przypisaniem reszty.

Testy własnościowe (§7.2): suma pieniądza = emisja − destrukcja i suma masy = produkcja − konsumpcja
− straty, sprawdzane **po każdym kroku makro**, tolerancja **0**. Plus `lift(lower(s)) == s` na polach
`Money` i `Qty`, tolerancja **0**.

#### K2 — Odchylenie agregatów ≤ 0,5% w skali miesiąca gry

**Przyjęte, ale z jawnym ograniczeniem zakresu — i to jest miejsce, w którym proszę o świadomą decyzję.**

Próg 0,5%/miesiąc jest osiągalny dla agregatów, w których uśrednia się dostatecznie wiele decyzji.
Błąd zastąpienia losowania wartością oczekiwaną ma rząd O(1/√n) na pojedynczym udziale rynkowym;
przy sumowaniu po komórkach i dniach błędy o przeciwnych znakach znoszą się. Konkretnie:

| Agregat | n | Oczekiwane odchylenie miesięczne | ≤ 0,5%? |
|---|---|---|---|
| Suma pieniądza w systemie | — | 0 (K1) | **tak, z zapasem** |
| Gotówka + depozyty GD per dzielnica | 2000+ osób | 0,15–0,3% | **tak** |
| Kapitał i dług firm per dzielnica | 12+ firm | 0,2–0,4% | **tak** |
| Przychód i podatki skumulowane per dzielnica | — | 0,2–0,4% | **tak** |
| Obrót w jednej branży w dzielnicy | ≥ 5 firm | 0,4–0,8% | **na granicy** |
| **Przychód pojedynczej firmy** | 1 | **3–12%** | **nie** |
| **Cena towaru przy < 3 dostawcach w dzielnicy** | 1–2 | **2–8%** | **nie** |
| **Udział rynkowy pojedynczej marki** | 1 | **2–5%** | **nie** |

Trzy ostatnie wiersze są **fizycznie nieosiągalne** i nie da się tego naprawić inżynierią. Udział
rynkowy jednej firmy to realizacja rozkładu wielomianowego; makro zna jego wartość oczekiwaną,
a mezo losuje jedną realizację. Przy 200 klientach odchylenie standardowe udziału to ~3,5% i żaden
wspólny kernel tego nie zdejmie — zdjęłoby to dopiero symulowanie pojedynczych klientów, czyli
rezygnacja z makro.

**Rozstrzygnięte — dok. 00 §4 / K-5.** Próg 0,5%/miesiąc obowiązuje **wyłącznie dla agregatów
o liczebności n ≥ 500** (osób lub ≥ 10 firm). Ten sam próg n ≥ 500 wymusza skalowanie ziarna
komórek z populacją — rachunek i reguła w §5.6 („Ziarno agregacji jest zamknięte z obu stron"). Poniżej tego poziomu obowiązują wyłącznie K1 (zachowanie)
i K3 (brak dryfu) — bez progu na odchylenie pojedynczego kroku. Praktyczna konsekwencja jest
akceptowalna: tryb 50× i „co jeśli" **nie służą do przewidywania losu jednej firmy** i nie wolno
budować na nich takich funkcji w UI. Służą do przewidywania stanu dzielnicy i miasta.

Dla mniejszych zbiorów błąd 3–12% jest **wpisany w metodę i nie jest kwestią kalibracji** —
dok. 00 §4 mówi to wprost, więc nie jest to już moje zastrzeżenie, tylko kontrakt projektu.
Wiążąca konsekwencja dla konsumentów (M7, M9, M12): wynik `what_if()` wolno używać **porównawczo**,
nigdy jako liczby bezwzględnej w regule progowej ani jako wartości pokazanej graczowi.

#### K3 — Brak dryfu systematycznego (test trajektorii 100 lat)

To jest **ważniejsze kryterium niż K2** i jest prawdziwym testem tego, czy `sim/macro` i mezo dzielą
jeden model. Odchylenie 0,4% w każdym miesiącu w losową stronę jest nieszkodliwe. Odchylenie 0,4%
w każdym miesiącu **w tę samą stronę** to po 100 latach (1200 miesięcy) czynnik ~120× — świat, który
się rozpadł.

Test: trajektorie makro i mezo tego samego scenariusza przez 100 lat (36 000 dni przy kalendarzu 360),
próbkowane miesięcznie. Dla każdego agregatu liczymy różnicę względną `d(t) = (makro − mezo) / mezo`
i regresję liniową `d(t)` po czasie. Kryteria:

| Wielkość | Próg |
|---|---|
| Nachylenie regresji `|slope|` | **≤ 0,02 pp na rok gry** (czyli ≤ 2 pp po 100 latach) |
| Nachylenie istotne statystycznie (t-test) | **nie** — dryf musi być nieodróżnialny od zera |
| Skumulowana różnica po 100 latach, agregaty dzielnicowe | **≤ 5%** |
| Autokorelacja znaku `d(t)` (lag 1) | **≤ 0,3** — systematyczne odchylenie w jedną stronę jest wykrywane, nawet gdy mieści się w K2 |

Ostatni wiersz jest celowo ostry: dodaję go, bo sam próg na nachylenie da się przypadkiem przejść
na krótkiej próbce, a autokorelacja znaku łapie „makro zawsze odrobinę na plus" natychmiast.

Test 100-letni jest **nocny**, nie per-commit (mezo × 36 000 dni × 2000 agentów to minuty w headless,
nie sekundy). Per-commit działa wersja 12-miesięczna (K1 + K2). Regresja dryfu blokuje release, nie merge.

#### K4 — Przejście makro→mezo odtwarza stany indywidualne deterministycznie

Mechanizm w §5.8. Kryteria:
- `lower_cell()` jest funkcją czystą — dwa wywołania z tym samym `(cell, seed, tick)` dają bitowo
  identyczny wynik; brak dostępu do `World`, brak stanu ukrytego (to jest wymaganie M12: tu replay
  i zapis mogą się rozjechać).
- `lower()` wykonany dwukrotnie z tego samego `MacroState` → identyczny hash świata.
- `lift(lower(s)) == s` na polach `Money` i `Qty` — tolerancja **0** (test własnościowy, 1000 stanów).
- Tożsamość zachowana w całości: zbiór `CitizenSeed` po `lower()` jest identyczny co do elementu
  ze zbiorem przed; żaden mieszkaniec nie znika i żaden nie powstaje bez zdarzenia demograficznego.
- Sekwencja `lower → 1 miesiąc mezo → lift → 1 miesiąc makro` nie daje wyniku gorszego niż
  `lift → 2 miesiące makro` (test antyhisterezy — przełączanie LOD nie może samo w sobie generować błędu).

**Co to znaczy dla WP10.2.** Przekroczenie K2 lub K3 jest w praktyce zawsze tym samym błędem:
ktoś dopisał logikę ekonomiczną do `sim/macro` zamiast wołać `econ_kernel`. To jest prawdziwy cel
tych testów — nie mierzą jakości przybliżenia, tylko pilnują, że model jest jeden.

### 7.4 Funkcjonalne — progi liczbowe

| Obszar | Kryterium |
|---|---|
| Marka | +20 Q oczekiwań wymaga ≥ 7 ekspozycji; −20 Q rozczarowania kosztuje ≥ 14 pkt afinitetu; odbudowa ≥ 3 pozytywne doświadczenia |
| Marka | 400 tys. mieszkańców × 16 slotów ≤ 55 MB **mierzone** |
| Dry-run | Przebieg 64 seedów powtórzony dla **każdego rozmiaru z §4.1**, nie tylko metropolii |
| Ziarno | `macro_grain_meets_min_cell_pop`: wszystkie 7 rozmiarów z §4.1 daje ≥ 500 osób na komórkę |
| Ziarno | Zero stałych liczb komórek w kodzie — wszędzie `state.cells.len()` (lint + test na 20 tys. i 400 tys.) |
| Zasięg | Billboard: 8000 przejazdów/dobę × 12% → 960 ± 30 ekspozycji; przeniesienie na inną krawędź zmienia rozkład dzielnic zgodnie z macierzą dojazdów |
| Media | Publikacja o czytelnictwie 35%/8% daje znajomość > 40% / < 15% po 3 dniach (± 5 pkt) |
| R&D | 4 badaczy (skill 60) + 50 tys./mies. → węzeł 1200 RP w 9 ± 1 miesiącu |
| Giełda | Kurs nie porusza się przed publikacją wyników, jeśli nie ma plotki |
| Ubezpieczenia | Po 2 powodziach w oknie 60 mies. składka w dzielnicy nadrzecznej ≥ 2× wyższa, **bez parametru ryzyka w danych** |
| Ubezpieczenia | Przy < 100 polisach jedna szkoda nie zmienia składki o rząd wielkości (wygładzanie) |
| Kartel | Hazard rośnie monotonicznie z liczbą członków i odchyleniem ceny; po wykryciu afinitet marki −20 u posiadaczy slotu |
| Związki | Zakład z marżą 35% i płacą 20% poniżej mediany formuje związek w 3–9 miesięcy |
| Strajk | 100 strajków w balansatorze: mediana 4–21 dni, maksimum < 90 dni |
| Dry-run | 64 seedy: 100% przechodzi Etap 10, > 80% bez rundy naprawczej |
| Dry-run | 80 lat ≤ 60 s jednowątkowo; rozwinięcie 400 tys. mieszkańców ≤ 20 s |
| Wyjaśnialność | 100% nowych decyzji ma `DecisionReason` w karcie inspekcji (dok. 00 §7) |

### 7.5 Wydajność

- `macro_step` metropolii: ≤ 8 ms/krok jednowątkowo (7500 kroków ≤ 60 s).
- `MacroState` metropolii ≤ 8 MB (asercja, nie szacunek).
- Systemy reklamowe: ≤ 1,5% budżetu ticku przy 2000 aktywnych kampanii.
- `stock_fixing`: ≤ 5 ms przy 500 notowanych firmach i 50 tys. zleceń.
- **Ruch w makro (budżet uzgodniony z M12): ≤ 0,55 ms/tick.** Osiągalne tylko przy jednostce
  pracy = kohorta (dzielnica × klasa, ~2400 dla 150 tys.) i podróż rozpoczęta w ticku (~420/tick).
  **Zero iteracji per pojazd i per agent** — to jest warunek konieczny, nie cel optymalizacyjny.
  Czas przejazdu z `travel_time()` M4 (tablice §17.6), bez własnego modelu.
- Odczyt `brand_affinity` z zanikiem: ≤ 80 ns (criterion) — leży na ścieżce decyzji zakupowej.

---

## 8. Ryzyka fazy i mitygacje

| # | Ryzyko | Skutek | Mitygacja |
|---|---|---|---|
| R1 | **Makro rozjeżdża się z mezo mimo testów.** Ktoś dopisuje logikę ekonomiczną do `sim/macro`, bo „tam było wygodniej". | Świat startowy fałszywy; „co jeśli" M7 kłamie; tryb 50× psuje partię. | WP10.2 **przed** WP10.1: `sim/macro` fizycznie nie zawiera logiki — tylko agregację i pętlę. Lint CI przeciw literałom stawek w `sim/macro`. Test 7.3 per-commit. To jest ryzyko nr 1 fazy i cała jej architektura jest wokół niego zbudowana. |
| R2 | **Dry-run generuje świat, który wygląda dobrze w agregacie i absurdalnie w szczególe** (dzielnica bez sklepu spożywczego, firma z 200 pracownikami i jednym klientem). | Gracz widzi absurd w pierwszej minucie partii. | Bramki Etapu 10 obejmują kryteria **strukturalne** (każda firma ma dostawcę i pracownika, każdy mieszkaniec ma dom), nie tylko agregatowe. Plus ręczny przegląd 10 seedów przed zamknięciem fazy — czytanie raportu, nie tylko zielonego CI. |
| R3 | **Dry-run 80 lat trwa 10 minut.** | Start nowej partii nie do zniesienia. | Budżet 60 s jest kryterium ukończenia WP10.1, nie celem. Zmienny krok (7 dni wcześnie, 1 dzień w ostatnich 5 latach) daje 3× oszczędność przy zachowaniu dokładności tam, gdzie ona działa na stan startowy. Awaryjnie: `--dry-run-years 30` jako domyślne dla dużych miast, 80+ jako opcja. |
| R4 | **Sloty marek wypychają lojalność.** 16 slotów, agresywna kampania zalewa pamięć, mieszkaniec zapomina sklep, do którego chodzi od 10 lat. | Lojalność z §5.1 przestaje istnieć; marka staje się funkcją budżetu reklamowego — dokładnie to, czego §7.6 zabrania. | Przypinanie: pracodawca i sklep odwiedzony ≥ 8 razy nie podlegają wypieraniu. Wypieranie po `salience`, nie po czasie. Test regresji: mieszkaniec z 10-letnią lojalnością po kampanii konkurenta nadal ma slot swojego sklepu. |
| R5 | **Giełda staje się jedyną grą.** Wycena z opóźnionych danych + plotki = arbitraż, który dominuje nad prowadzeniem firmy. | Gracz przestaje symulować miasto, zaczyna klikać fixingi. | Fixing dzienny (nie ciągły) ogranicza częstotliwość. Koszty transakcyjne i podatek od zysków (M8). Płynność jest realnie mała — 500 firm i 5000 inwestorów to nie NYSE; duże zlecenie rusza kurs przeciw sobie. Monitorowane w balansatorze jako „udział zysków gracza z obrotu akcjami" — próg alarmowy 30%. |
| R6 | **Strajki jako spirala śmierci.** Strajk → kary z kontraktów → firma traci płynność → nie może podnieść płac → strajk trwa → bankructwo. Emergentne i realistyczne, ale jeśli zdarza się w 40% firm, gospodarka wymiera. | Balansator pokazuje wymieranie sektora. | Fundusz strajkowy jest skończony i to on wymusza rozstrzygnięcie. Test 7.4 (mediana 4–21 dni, max < 90). Jeśli balansator pokaże > 5% firm rocznie w strajku — parametry `grievance` do korekty, nie mechanizm. |
| R7 | **Nowa kategoria produktu psuje graf towarów w trakcie partii.** `NewGood` bez receptury i bez importu. | Towar bez źródła, popyt niezaspokajalny, spirala cenowa. | Walidator grafu towarów (dok. 00 §5) uruchamiany **także po odblokowaniu technologii**, nie tylko przy ładowaniu. Odblokowanie, które nie przechodzi walidacji, jest odrzucane i logowane jako błąd danych. |
| R8 | **Kartel jest zawsze nieopłacalny albo zawsze opłacalny.** | Mechanizm martwy albo dominujący. | Hazard jest funkcją odchylenia ceny — kartel umiarkowany jest opłacalny, chciwy się wykrywa. Kalibracja w balansatorze: cel to 20–50% karteli wykrytych w ciągu 5 lat gry. |
| R9 | **Kronika zalana wpisami z dry-runu.** 80 lat × wszystkie zdarzenia = nieczytelne. | Funkcja narracyjna martwa. | Próg skali dla wpisu z dry-runu jest wyższy niż dla zdarzenia w partii (tylko rzeczy widoczne w skali dzielnicy). Cel: 50–200 wpisów na 80 lat, nie 50 000. Filtr `provenance` w UI (M9). |
| R10 | **Zakres fazy jest największy w projekcie.** Osiem niezależnych systemów plus najtrudniejszy technicznie (`sim/macro`). | Faza się nie kończy. | Ścieżka krytyczna (10.2→10.1→10.3→10.4) jest wydzielona i **sama w sobie jest dostarczalnym artefaktem** — świat „zużyty" na starcie. Pozostałe WP są niezależne i każdy da się wyciąć bez naruszenia reszty. Kolejność cięcia przy presji: 10.9, 10.12, 10.11, 10.7. |
| R11 | **Liczby w tym dokumencie mogą być skalibrowane od złej strony zakresu — i nie wykryje tego mój własny przegląd.** Cztery realne błędy tej fazy (ziarno komórek łamiące próg n na małym mieście; scenariusz referencyjny stojący poniżej progu, który sam weryfikuje; `Qty` gubiące jednostkę towarów masowych; `error_margin_bp` jako parametr konsumenta zamiast obietnicy modelu) wyszły **wyłącznie z pytań innych faz**. Żadnego nie znalazł przegląd własny, mimo że wszystkie były w dokumencie od pierwszej wersji. | Liczby, których nikt nie zakwestionował, mają tę samą szansę być błędne co te cztery. Dotyczy to w szczególności wielkości, które postawiłem „z rozsądku": `BRAND_SLOTS = 16`, `MIN_CELL_POP = 500`, `K_UP/K_DOWN = 64/192`, okno 60 miesięcy w ubezpieczeniach, próg `grievance ≥ 55`, T+45 dni publikacji, hazard bazowy kartelu 0,5%. | Trzy rzeczy, żadna nie jest „uważniejszym czytaniem": (1) **każda z tych stałych ma w dokumencie jawny test z progiem liczbowym** (§7.4) — stała bez testu jest niewykrywalna z definicji; (2) **wszystkie mają wyliczenie lub pomiar jako źródło, nie intuicję** — `MIN_CELL_POP` ma wyprowadzenie z O(1/√n), `BRAND_SLOTS` jest jawnie oznaczony jako „start z 16, podnieść po pomiarze w balansatorze" (D4), a nie jako wartość docelowa; (3) **przegląd krzyżowy z fazą sąsiednią przed zamknięciem planu** — to jedyny mechanizm, który w tej rundzie faktycznie zadziałał, i jest tańszy niż znalezienie tego samego w implementacji. Ryzyko zostaje otwarte świadomie: nie znam sposobu, by je zamknąć w obrębie jednej fazy. |

---

## 9. Decyzje otwarte

| # | Decyzja | Kontekst | Kto rozstrzyga | Propozycja M10 |
|---|---|---|---|---|
> **Rozstrzygnięte przed startem fazy** (zapisane dla historii, nie wymagają już działania):
> - *Własność `sim/macro`*: M10 jest właścicielem crate'u (dok. 00 §1). API w §6 jest **kontraktem
>   wiążącym**, nie propozycją do zatwierdzenia; M7 buduje `what_if()` na tym API. Podzbiór wymagany
>   już w M7 jest wypisany w §6.
> - *Tolerancja LOD*: dok. 00 §4 doprecyzowany — mikro↔mezo 0, makro↔mezo cztery kryteria K1–K4.
>   M10 przyjmuje je jako kryteria akceptacji (§7.3) i rezygnuje z własnych progów.
> - *Kalendarz*: K-1, 360 dni (12 × 30). Naniesione w §5.7, §7.1.
> - *Zakres `StreamId`*: 280–299, rozpisany w §7.1.
> - *Zakres progu 0,5%* (dawne D1): dok. 00 §4 / **K-5** — próg obowiązuje **wyłącznie dla agregatów
>   o liczebności n ≥ 500**; dla mniejszych zbiorów, w szczególności dla pojedynczej firmy, błąd
>   3–12% jest wpisany w metodę i nie jest kwestią kalibracji. Wiążąca konsekwencja dla konsumentów:
>   wynik `what_if()` wolno używać **porównawczo**, nigdy jako liczby bezwzględnej w regule progowej
>   ani jako wartości pokazanej graczowi. Deklaracja błędu razem z wynikiem (`error_margin_bp`)
>   i test na jej uczciwość **w obie strony** są częścią kontraktu (§6, §7.3/K2).
> - *Refaktor `sim/economy`* (dawne D11): **nie będzie refaktoru** — M5 pisze `kernel` od razu jako
>   rdzeń. Podział i zasada rdzenia w WP10.2.
> - *`travel_time()`* (dawne D2): zostaje w `sim/traffic`; M4 dostarcza `TravelTimeMatrix::lookup`.

| **D1** | **Nowy katalog danych `data/tech/`.** | Dok. 00 §5 wylicza katalogi `data/`; `tech` nie ma na liście. | **Właściciel dok. 00** (dopisek). | Dopisać `data/tech/` do listy w dok. 00 §5, z walidatorem grafu (prereq istnieje, brak cykli, `NewGood` ma wpis w `data/goods/`) jako testem CI od M10. |
| **D2** | **Jawne trzymanie GD w makro (150 tys. × 48 B = 7,2 MB).** | §17.4 mówi „agregaty per dzielnica × klasa"; M10 robi wyjątek dla GD, bo mieszkanie to konkretna parcela, a §4.2 Etap 9 wymaga „majątków rodzin". | **M10 + M12** (M12 płaci za to w trybie 50×). | Utrzymać wyjątek. 7,2 MB przy budżecie 6 GB to 0,12%, a alternatywa (rozkład zamiast adresów) psuje rynek nieruchomości po `lower`. |
| **D3** | **Czy gracz może posiadać media?** | §7.2 wymienia media jako typ firmy; PRD nie rozstrzyga konfliktu interesów. | **M9 (gracz) + M8 (regulator).** | Tak, z konsekwencją: posiadanie tytułu daje przewagę PR, ale regulator M8 może nałożyć ograniczenia koncentracji, a wiarygodność tytułu spada u czytelników, którzy zauważą stronniczość (mechanizm już jest — wiarygodność jest per-para). Nie wymaga nowego kodu, wymaga decyzji projektowej. |
| **D4** | **`BRAND_SLOTS = 16` — czy to wystarczy?** | 16 slotów × 8 B = 128 B/mieszkańca. Przy 24 slotach: 76,8 MB (1,28% budżetu). | **M10, po pomiarze w balansatorze.** | Start z 16. Jeśli balansator pokaże, że mediana liczby marek, z którymi mieszkaniec ma realny kontakt, przekracza 13 — podnieść do 24 (stała, jedna zmiana). Nie robić tego przed pomiarem. |
| **D5** | **Kiedy dry-run działa: przy generacji świata czy przy pierwszym uruchomieniu partii?** | 60 s to dużo dla ekranu ładowania, mało dla generatora, który i tak liczy teren. | **M9 (`game/`, przepływ startu partii).** | W tle, równolegle z Etapami 1–8 generatora (teren i dry-run są niezależne do momentu D0 — a D0 potrzebuje tylko stref i parcel z Etapu 5). Realny narzut na ekranie ładowania: ~15 s. |
| **D6** | **Czy strajk zatrzymuje zakład, czy firmę?** | §6.6 mówi „produkcja stoi" — niejednoznacznie. | **M10 + M7.** | Zakład. Związek formuje się na poziomie zakładu (graf relacji jest lokalny — ludzie znają współpracowników, nie całą korporację). Strajk ogólnofirmowy jest możliwy jako eskalacja (`UnionScope::Firm`), ale nie domyślny. |
| **D7** | **Czy `lower()` musi odtwarzać pamięć doświadczeń mieszkańców?** | Po dry-runie mieszkańcy nie mają historii zakupów — wszystkie sklepy są im obce, więc pierwszy tydzień partii to chaos wyborów. | **M10 + M3.** | Tak, minimalnie: `lower()` zasiewa 3–5 wpisów doświadczeń i 2–4 sloty marek per mieszkaniec, wybierając sklepy z jego dzielnicy proporcjonalnie do ich udziału rynkowego w makro. Bez tego świat startowy jest „zużyty" w liczbach i sterylny w zachowaniu. Koszt: jeden przebieg przy `lower`. |
| **D8** | **Zerwanie śladu pochodzenia partii na granicy makro** (zgłoszone przez M6). `MacroStock` zna ilość, nie zna partii — przy `lower()` partie są generowane na nowo, więc trace „od pola do półki" (§14.4) kończy się na `FromAggregate`, nie na złożu. | Dotyczy każdego świata po dry-runie i każdego powrotu z trybu 50×. Naprawa wymagałaby trzymania partii w makro — co przekreśla sens agregacji (partie to setki tysięcy encji). | **M6 + M10 + M9** (M9 pokazuje trace w UI). | Zaakceptować i **pokazać jawnie**: partia z agregatu ma `provenance: FromAggregate { district, last_supplier }`, partia z historii — `provenance: DryRun`; UI kończy ślad komunikatem „odtworzone z agregatu dzielnicy", nigdy zmyślonym łańcuchem. Jeden skok wstecz zachowany w `MacroFirm.suppliers`. Alternatywa (trzymanie partii w makro) do odrzucenia. |

### 9.2 Stan decyzji po M10a

Podfaza M10a zamknęła cztery decyzje i otworzyła jedną. Szczegóły i uzasadnienia —
tabela `E-n` w `M10a-jadro-makro-historia.md`.

| # | Stan po M10a |
|---|---|
| **D2** | **Przyjęta inaczej, niż brzmiała propozycja, i to jest korzyść.** Gospodarstwa nie są trzymane jawnie w `MacroState` i nie kosztują 7,2 MB — trzyma je `World`, bo `lower()` jest **nakładką na stojący świat**, a nie jego generatorem (`K-78`, `E-1`). „Majątki rodzin" z §4.2 Etap 9 mają nośnik: `Household.cash/bank/debt` zapisane przez `lower()`. |
| **D5** | **Bez zmian, mierzone.** Dry-run trzydziestu lat metropolii to 3300 kroków i ~0,2 s po wygenerowaniu miasta; osiemdziesiąt lat mieści się w budżecie 60 s z dużym zapasem (7500 kroków ≈ 47 s dla metropolii, `criterion`). Przepływ startu partii pozostaje do zaprojektowania przez M9 — ale narzut, o który tam chodziło, okazał się rzędu sekund, nie kwadransa. |
| **D7** | **Nie wykonana w M10a i przeniesiona do M10b z imieniem.** `lower()` nie zasiewa pamięci doświadczeń: sklepy, którymi się ją zasiewa, wskazuje udział rynkowy marki, a marka to WP10.5. `StreamId::MacroSeedMemory = 294` jest zarezerwowany imiennie i **niezajęty** — zajmie go M10b. Konsekwencja przyjęta świadomie: pierwszy tydzień partii po dry-runie jest pod względem wyboru sklepu taki sam, jak przed nim. |
| **D8** | **Przyjęta w całości.** Partie nie są odtwarzane, a `MacroFirm.suppliers` niesie jeden skok wstecz i jest wypełniany co miesiąc przez fazę 7. Przebieg `headless dry-run` wypisuje, ile firm ma stałego dostawcę i ile jest par (firma, towar). |
| **D9 (nowa)** | **Rozkład majątku gospodarstw po Etapie 8 nie mieści się w bramce 7 Etapu 10.** Zmierzone: Gini 0,50–0,84 zależnie od ziarna i wielkości miasta; plan (§5.7, bramka 7) oczekuje 0,25–0,45. Obie liczby są obronne — realny współczynnik Giniego **majątku** bywa rzędu 0,7–0,8, a 0,25–0,45 to pasmo typowe dla **dochodu**. Rozstrzygnąć trzeba, którą wielkość mierzy ta bramka, bo od tego zależy, czy naprawiać próg, czy generator. **Kto rozstrzyga: właściciel produktu.** Propozycja M10: rozdzielić bramkę na dwie — Gini majątku 0,55–0,85 i Gini dochodu 0,25–0,45 — bo pierwsza mierzy Etap 8, a druga rynek pracy, i mylenie ich ukrywa obie. |
| **D10 (nowa)** | **Płaska `CommuteMatrix` blokuje dwie bramki Etapu 10 i to jest dług kontraktu, nie modelu.** Rekrutacja zamknięta w granicach dzielnicy zostawia ~29 % firm bez obsady (dzielnice przemysłowe nie mają mieszkań), więc nie produkują i bramki 1 oraz 3 świecą na czerwono. Otwarcie puli na całe miasto zostało spróbowane i cofnięte — bez **kosztu** dojazdu rekrutacja jest albo zakazana, albo darmowa, a żadne z tych dwojga nie jest rynkiem pracy. Kontrakt M10 §6 mówi „M10 trzyma, M4 wypełnia"; do wypełnienia potrzebny jest snapshot `TravelTimeMatrix::lookup`. **Kto rozstrzyga: M4 + M10.** Propozycja M10: wypełnić macierz przy okazji M10e, bo związki zawodowe i tak czytają sieć relacji w zakładzie, a ta zależy od tego, kto skąd dojeżdża. |

---

## 10. Szacunek wielkości

| WP | Temat | Rozmiar | Zależności |
|---|---|---|---|
| WP10.2 | Domknięcie `econ_kernel` (M5 buduje rdzeń natywnie) | **M** | M5, M6, M7 |
| WP10.1 | `sim/macro`: stan, `step`, `lift`/`lower` | **XL** | WP10.2 |
| WP10.3 | Historia „na sucho" (D0–D5, rebalance, kronika) | **L** | WP10.1, M1–M2 |
| WP10.4 | Test spójności LOD + balansator | **M** | WP10.1–10.3 |
| WP10.5 | `BrandAffinity`, sloty, zanik, asymetria | **M** | M3, M5, M7 |
| WP10.6 | `AdCampaign`, `Reach`, osiem kanałów | **L** | WP10.5, M4, M2 |
| WP10.7 | Media jako firmy, `Story` → plotka | **M** | WP10.6, M3, M8 |
| WP10.8 | `TechTree`, RP, `Patent`, licencje | **L** | M7, dane |
| WP10.9 | Nowe kategorie produktów, zmiana potrzeb | **M** | WP10.8, M8, M3 |
| WP10.10 | `Share`, `OrderBook`, fixing, `Valuation` | **L** | M7, M5 |
| WP10.11 | Przejęcia, dywidendy, emisje, ład korporacyjny | **M** | WP10.10 |
| WP10.12 | `InsurancePolicy`, `PerilStats`, wypłaty | **M** | M8, M7 |
| WP10.13 | `SupplierRelation`, ekskluzywność, `Cartel`, franczyza, JV | **L** | M6, M7, M8 |
| WP10.14 | `Union`, żądanie, negocjacje, strajk | **M** | M7, M3 |
| WP10.15 | Kroniki i wyjaśnialność (przekrojowy) | **S** | M9, wszystkie |
| WP10.16 | Determinizm, `StreamId`, hash stanu (przekrojowy, DoD) | **S** | wszystkie |

**Rozkład:** 1 × XL, 4 × L, 8 × M, 2 × S. **Najcięższa faza projektu po M7.**

WP10.2 zmalał z L do M, odkąd M5 buduje `kernel` jako rdzeń od pierwszego dnia zamiast oddawać go
do wyciągnięcia po fakcie — jedyna pozycja tego planu, która stanieje wskutek uzgodnień, a nie
wskutek cięcia zakresu.

**Ścieżka krytyczna:** WP10.2 → WP10.1 → WP10.3 → WP10.4 (M + XL + L + M). Ona jedna jest
dostarczalnym artefaktem (świat „zużyty" na starcie) i ona jedna blokuje M12.

**Kolejność cięcia przy presji na zakres** (od pierwszego do ostatniego): WP10.9 (nowe kategorie),
WP10.12 (ubezpieczenia), WP10.11 (przejęcia — giełda działa bez nich), WP10.7 (media — kanały
reklamowe działają bez redakcji). **Nie do wycięcia:** WP10.1–10.4 (świat startowy i spójność LOD),
WP10.5 (bez niej §7.6 nie jest zrealizowane w ogóle).


## Zmiany wpisane po M9e

Zgodnie z `K-18`. Szczegóły — tabela `DI-n` w `M9e-panele-czas-kariera.md`.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| DK-1 ★ | **Kronika gracza nie przyjmuje zgłoszeń — czyta dzienniki.** `M10f` („Kroniki i domknięcie") ma w zależnościach „M9 (podsystem kroniki)", ale tego podsystemu nie wolno wołać: `game/` stoi **nad** wszystkimi `sim/*`. Zdarzenie M10, które ma trafić do kroniki gracza, zapisuje się **u siebie** — w `Events::chronicle()`, w pierścieniu decyzji firmy albo w nowym dzienniku `sim/macro` — a `game::chronicle::Chronicle::harvest` dokłada dla niego źródło | Zależność w drugą stronę zamknęłaby cykl, którego Cargo nie zbuduje (`DI-4`). Praktycznie to jest **mniej** pracy, nie więcej: zdarzenie i tak musi zostawić ślad po stronie symulacji, żeby przeżyć zapis gry |
| DK-2 ★ | **`DryRunResult.chronicle: Vec<ChronicleEvent>` zostaje typem M10 i nie jest tym samym co `game::chronicle::ChronicleEntry`.** Historia „na sucho" opisuje osiemdziesiąt lat, których nikt nie rozegrał, i ma własne `provenance: DryRun`; kronika gracza opisuje grę, która się toczy | Dwa typy o jednej nazwie rozjechałyby się przy pierwszej zmianie, a łączenie ich dałoby kronikę, w której zdarzenie wymyślone i przeżyte wyglądają tak samo. Most między nimi jest jednokierunkowy: `harvest` może dołożyć źródło „historia sprzed gry", odwrotnie nie |
| DK-3 | **Panele marki, R&D i giełdy dokłada się przez `PanelRegistry::register`, bez dotykania kodu M9.** `PanelId::{Brand, Rnd, Stock}` istnieją i `is_reserved()` mówi o nich prawdę — nie ma ich w rejestrze. `PanelDesc` niesie klucz tytułu, listę źródeł danych, funkcję budującą model i funkcję rysującą | Wykonanie decyzji otwartej nr 11 dokumentu M9. Uwaga praktyczna: `PanelModel` jest **enumem**, więc panel M10 dokłada do niego wariant — to jedyne miejsce, w którym M10 dotyka `game::panels` (`DI-6`) |
| DK-4 | **`OverlayField::BrandAwareness` nadal nie ma wpisu w `data/ui/overlays.ron`** i `overlays::build` zwraca dla niego `None`, a nie raster zer. M10 dokłada wpis razem z marką | Bez zmian od `M9c` (`DG-1`); powtórzone tutaj, bo M10 jest pierwszą fazą, która to naprawi |
| DK-5 | **`Trend` z `K-52` ma już czytelnika po stronie gracza**: karta firmy i pulpit rysują z niego kierunek. Kwoty z prognozy do UI nie idą i nie pójdą (`R15`) | Kanał jest gotowy — M10 wypełnia `StrategicOutlooks`, a nie buduje drogi do okna |
| DK-6 | **Dwa byty M9e czekają na M10 z nazwy.** (1) Warunek zatrzymania „strajk" **nie powstał** — związki i negocjacje należą do M10 (`K-9`), więc wariant dokłada się do `game::timectl::StopCondition` razem z mechaniką, a nie przed nią. (2) Cel scenariusza `Goal::ProductLaunched` **nie powstał** z tego samego powodu: własna marka i receptury to M10 | Wariant, którego nikt nigdy nie zgłosi, przechodzi każdy test i wygląda tak samo jak działający (`K-67`). Oba enumy są w `game/`, więc M10 dokłada do nich wariant tak samo, jak dokłada wariant `DecisionReason` — plus jedno ramię i jeden klucz tekstu |
| DK-7 | **Dziedziczenie nie rozróżnia pokrewieństwa.** `legacy::heir_of` bierze dorosłego domownika o najniższym indeksie encji, bo `Household` niesie listę członków, a nie drzewo rodziny. W gospodarstwie dwojga dziedziczy małżonek, w gospodarstwie z dorosłym dzieckiem — ten, kto urodził się wcześniej | Kolejność z §5.11 („dorosłe dziecko → małżonek → rodzeństwo") wymaga stopnia pokrewieństwa, a pytania o rolę nie ma dziś komu zadać. M10e (relacje i związki) i M10f (kroniki i rody) budują dokładnie tę wiedzę — wtedy `heir_of` dostaje prawdziwą kolejność zamiast `ponytail:` w kodzie |
| DK-8 ★ | **Uczeń wypadł z indeksu miejsc pracy, więc `M10e` §5.9 mierzy wreszcie to, co obiecuje.** `SocialIndex::coworkers(site)` zwracał do `R2-WP1` również uczniów, bo `Employment.site` niesie **szkołę** ucznia, a `has_job()` nie odróżniało szkoły od zakładu (`K-74`). Warunek powstania związku zawodowego — spójna składowa grafu relacji **wśród pracowników zakładu** ≥ max(8, 25 % załogi) — miał więc jako pierwszą kandydatkę szkołę podstawową. Od tej chwili `by_site` bierze `is_employed()`, a klasa szkolna ma własny indeks (`classmates`) i relacje `Acquaintance`, nie `Colleague` (`D-N7`). **Co z tego wynika dla M10e:** pozycja 11 wykazu `R2` jest częściowo rozbrojona i zostaje z samym warunkiem — nie trzeba już filtrować uczniów po stronie związków | Wariant `RelationKind::Classmate` **nie powstaje** (`D-N7`): kolejność wariantów jest kontraktem zapisu gry, a nikt nie pyta, czy znajomy jest kolegą z klasy |
| DK-9 | **Zakład produkcyjny ma wreszcie rachunek wyniku** (`K-75`), więc `M10d` (przejęcia) i `M10a` (historia „na sucho”) mogą pytać o jego marżę. Do `R2-WP7` `SitePnlMonth.revenue` fabryki był zawsze zerem, a `margin_bp()` zwracało `None`, czyli „nie wiem” — wycena firmy produkcyjnej stanęłaby na liczbie, której nie ma | Hash stanu zmienił się w każdym świecie z produkcją; złote odciski sprzed 2026-09-18 nie porównują się z późniejszymi |
