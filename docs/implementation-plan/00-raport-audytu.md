# Raport z rozbicia PRD na fazy implementacyjne

Data: 2026-09-13
Źródło: `PRD_Magnat.md` (wersja 0.1)
Dokument nadrzędny planu: `00-konwencje-i-kontrakty.md`

---

## 1. Co powstało

`docs/implementation-plan/` — 14 dokumentów, ~17 000 linii.

| Plik | Zawartość |
|---|---|
| `00-konwencje-i-kontrakty.md` | Dokument nadrzędny: macierz własności crate'ów, typy bazowe, reguły determinizmu, kontrakt LOD, katalogi danych, szablon fazy, rozstrzygnięcia `K-1`…`K-16` |
| `M0-fundament-silnika.md` | Workspace, `core`, `ecs`, `jobs`, `io` minimalne, devtools, headless runner, determinizm |
| `M1-swiat-statyczny.md` | Teren, hydrologia, geologia i złoża, klimat i biomy, `engine/voxel`, `engine/render`, kamera |
| `M2-miasto-statyczne.md` | `engine/spatial`, drogi (L-system), strefy, parcele, gramatyka budynków, dzielnice, gospodarka bazowa |
| `M3-ludzie-i-dzien.md` | Mieszkańcy, gospodarstwa domowe, potrzeby, planer dnia, DES, demografia, karta inspekcji |
| `M4-ruch.md` | `engine/nav` (CCH), ruch mikro/mezo, komunikacja miejska, parkingi, wybór środka transportu |
| `M5-gospodarka-detaliczna.md` | Oferty, użyteczność zakupu, sklepy z magazynem, ceny AI, pieniądz, banki, księgowość, balansator |
| `M6-lancuch-dostaw.md` | Towary i partie, receptury, zakłady, magazyny, transport, rynek B2B, import/eksport, niedobory |
| `M7-firmy-ai-i-rynek-pracy.md` | Firmy, HR, menedżerowie, finanse, upadłość, rynek pracy, AI firm, `sim/policy` |
| `M8-miasto-jako-aktor.md` | Władza miejska, podatki, wybory, usługi publiczne, prawo, sieci przesyłowe, zdarzenia i pogoda |
| `M9-gracz-kariera.md` | Postać gracza, kariera, scenariusze, panele biznesowe, `engine/ui`, edytor reguł, kronika |
| `M10-glebia.md` | Marka z pamięci agentów, R&D i epoki, giełda, media, kartele, związki, `sim/macro`, historia „na sucho" |
| `M11-prezentacja.md` | Styl voxelowy, kamera, oświetlenie, animacje, LOD wizualne, `engine/audio`, `sim-snapshot` |
| `M12-skala-i-jakosc.md` | Metropolia 400 tys., tryb 50×, zapis w tle, replay, modding, lokalizacja, sesje 100-letnie |

Podział na fazy = kamienie milowe z PRD §19. Nie tworzono konkurencyjnej taksonomii; dokument
źródłowy sam definiuje kolejność tak, by po każdym etapie istniał działający, testowalny artefakt.

Każdy dokument fazy ma identyczną strukturę 10 sekcji (szablon: `00-konwencje-i-kontrakty.md` §8),
w tym jawną sekcję „nie wchodzi → faza X" i kontrakty „dostarczam / konsumuję".

---

## 2. Metoda

Jeden agent na fazę, 13 równolegle, każdy z rozłącznym zakresem wyłącznym. Przed startem powstał
dokument nadrzędny rozstrzygający decyzje wspólne — bez niego każdy agent zdefiniowałby po swojemu
typy bazowe, reguły determinizmu i jednostki, a rozjazd wyszedłby dopiero przy integracji.

Agenci mogli uzgadniać decyzje między sobą. Rozbieżności nierozstrzygnięte trafiały do sekcji 9
własnego dokumentu, a koordynator rozstrzygał je odgórnie i dopisywał do `00` jako `K-n`.

---

## 3. Przebieg audytu

**Wszystkie 13 faz wróciło do poprawki co najmniej raz.** W większości przypadków przyczyną był
kontrakt bazowy albo brak informacji z fazy sąsiedniej, nie jakość pracy agenta.

| Faza | Powód zwrotu | Źródło problemu |
|---|---|---|
| M0 | brak `det_math`, brak `DecisionReason` i słowników w `core`, chunki ECS jako API, areny | kontrakt bazowy (3 rundy) |
| M1 | `ChunkWriter`, 5 rozszerzeń `TerrainQuery`, granica z M11 | brak informacji z M2/M11 |
| M2 | kolizja `StreamId`, jednostki terenu, granica z M4, walidator grafu | rozstrzygnięcie po starcie fazy |
| M3 | tydzień w kalendarzu 360-dniowym, właściciel grafu pieszego | kontrakt bazowy |
| M4 | budżet pojazdów dostawczych, kontrakt rampy, ograniczenia tonażowe | brak informacji z M6 |
| M5 | `det_math`, podstawa ceny, rozszerzenia od M7/M8/M9 | kontrakt bazowy + brak informacji |
| M6 | ceny B2B netto, areny, odpowiedzi dla 4 faz | pomyłka adresowa koordynatora |
| M7 | `sim/policy` nie dotarło na czas, makro, upadłość, związki | pomyłka adresowa koordynatora |
| M8 | tydzień, regulacje ruchowe jako maski krawędzi | brak informacji z M4 |
| M9 | język reguł należy do wspólnego crate'a | podział własności koordynatora |
| M10 | tolerancja makro, adresy, jednostka towaru masowego | kontrakt bazowy |
| M11 | adresy M1/M6, `sim-snapshot`, brakujące katalogi danych | pomyłka adresowa koordynatora |
| M12 | tryb 50× jako pakiet budżetowy, nie przełącznik | brak informacji z M4 |

### 3.1 Błędy w kontrakcie bazowym wykryte przez agentów

1. **Tolerancja LOD była niewykonalna dla makro** (M4). Pierwotny zapis żądał tolerancji 0 na
   wszystkich poziomach; makro agreguje dzielnica × klasa, więc równość co do grosza nie istnieje.
   → `K-5`.
2. **Brak deterministycznych funkcji przestępnych** (M5). Funkcja użyteczności zakupu z PRD §6.4
   używa logarytmów i softmaxu; libm różni się między platformami, więc ten sam seed dałby inny
   świat na Windows i Linuksie. → `K-6`.
3. **Dwa silniki reguł** (M9). Automatyzacja polityk przypisana wyłącznie graczowi łamała obietnicę
   PRD §6.3, że gracz ma te same narzędzia co AI. → `K-11`, crate `sim/policy`.
4. **`BatchId`/`OfferId` jako encje ECS** (M6). 600 tys. obiektów o wysokiej rotacji, nigdy
   nieodpytywanych przekrojowo po archetypach — mutacja strukturalna ECS to koszt bez korzyści.
   → `K-16`, dedykowane areny.
5. **Sprostowanie do `K-6`** (M0): podstawowa arytmetyka `f64` jest deterministyczna
   międzyplatformowo (IEEE-754), niedeterminizm wnosi wyłącznie libm. Ucieczka do fixed-point
   kosztowałaby zakres dynamiczny bez powodu.

### 3.2 Błąd koordynatora

Pomylone adresy agentów: wiadomości kierowane do M7 trafiały do M6, jedna dla M6 — do M11.
Naprawione. Nie kosztowało treści, ponieważ M6, M11 i M10 rozpoznały cudzy zakres i odmówiły
rozstrzygania za inne fazy zamiast wykonać polecenie.

---

## 4. Decyzje wymagające właściciela produktu

1. **Odstępstwo od PRD §9.2.** Symulacja mikroskopowa działa wyłącznie w kadrze kamery, nie „także
   na drogach o wysokim obciążeniu". Uzasadnienie M4: kamera nie wchodzi do hasha stanu, więc
   gdyby mikro wpływało na ekonomię, obrót kamerą zmieniałby salda gospodarstw domowych.
   Dokument: `M4-ruch.md` §9/D7.
2. **Tryb 50× nie mieści się w celu z PRD §20.2** — 17,0 s na dobę gry wobec wymaganych ≤ 3 s dla
   miasta 150 tys. `M12-skala-i-jakosc.md` zawiera drabinę cięć; wybór, co poświęcić, należy do
   właściciela produktu.
3. **Kolejność zaspokajania w upadłości** — czy pracownicy mają pierwszeństwo przed wierzycielem
   zabezpieczonym. Decyzja gameplayowa. `M7-firmy-ai-i-rynek-pracy.md` §9/D7, wariant domyślny
   z argumentacją.
4. **Maszyna referencyjna** dla „GPU klasy średniej (2024)" z PRD §20.2. Bez niej tabele budżetów
   wydajnościowych M11 i M12 są nieweryfikalne. Zgłoszone niezależnie przez obie fazy.

### 4.1 Świadome osłabienie obietnicy z PRD

PRD §17.4 deklaruje, że wynik ekonomiczny nie zależy od poziomu LOD. Zapis został rozdzielony
(`K-5`):

- **mikro ↔ mezo: tolerancja 0** — bez zmian wobec PRD;
- **makro ↔ mezo: kontrakt słabszy** — zachowanie pieniądza i masy co do grosza i grama pozostaje
  absolutne, ale agregaty mają limit odchylenia 0,5% miesięcznie i **wyłącznie dla zbiorów n ≥ 500**.
  Dla pojedynczej firmy błąd 3–12% jest wpisany w metodę (wariancja rozkładu wielomianowego)
  i nieusuwalny.

Konsekwencja wiążąca: `what_if()` wolno używać porównawczo (uporządkowanie wariantów z marginesem),
nigdy jako liczby bezwzględnej w regule progowej ani jako wartości pokazywanej graczowi.

Jeśli to osłabienie jest nie do przyjęcia, trzeba wrócić do projektu trybu 50× i historii „na sucho".

---

## 5. Ryzyko metodologiczne

Diagnoza postawiona przez agenta M10, warta zapamiętania przy dalszej pracy nad planem:

> Żadne z czterech znalezisk tej fazy nie wyszło z własnego przeglądu dokumentu — wszystkie
> wymagały pytania z zewnątrz. Dwa z nich były błędami we własnym kontrakcie dokładności i
> przetrwały każdy przegląd autora. Liczby, których nikt nie zakwestionował, mają tę samą szansę
> być skalibrowane od złej strony zakresu.

Mitygacja świadomie **nie** brzmi „czytać uważniej", bo to właśnie zawiodło. Zamiast tego:

1. każda stała ma test z progiem — stała bez testu jest niewykrywalna z definicji;
2. każda stała ma wyprowadzenie lub pomiar jako źródło zamiast intuicji;
3. przegląd krzyżowy z fazą sąsiednią przed zamknięciem planu — jedyny mechanizm, który
   w tej rundzie faktycznie wykrywał błędy.

`M10-glebia.md` §8/R11 zawiera imienną listę stałych podejrzanych o złą kalibrację.

---

## 6. Stan końcowy

- 13 dokumentów faz, każdy z dokładnie 10 sekcjami wg szablonu.
- 16 rozstrzygnięć koordynacyjnych w `00-konwencje-i-kontrakty.md` §4a.
- Zero decyzji blokujących start prac.
- Decyzje otwarte pozostają udokumentowane w sekcji 9 każdej fazy, każda z propozycją domyślną
  i wskazanym adresatem.

Kolejny krok: M0 (fundament silnika) nie ma zależności wejściowych i może ruszyć od razu.
Przed startem warto zamknąć cztery decyzje z sekcji 4 tego raportu — trzy z nich (tryb 50×,
maszyna referencyjna, odstępstwo §9.2) wpływają na kryteria akceptacji, a nie tylko na zakres.
