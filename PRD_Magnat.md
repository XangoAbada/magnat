# PRD — „Magnat" (nazwa robocza)
## Symulacja miasta i gospodarki z pełnym łańcuchem dostaw, grafika voxelowa, własny silnik

Wersja: 0.1 (draft do dyskusji)
Data: 2026-09-13
Autor: Roman (właściciel produktu) / Claude (redakcja)

---

## Spis treści

1. Wizja produktu
2. Inspiracje i co z nich bierzemy
3. Filary designu
4. Świat: generacja miasta
5. Mieszkańcy: agenci, rodziny, potrzeby, tryb dnia
6. Gospodarka: podaż, popyt, ceny, pieniądz
7. Przedsiębiorstwa: typy, budynki, zarządzanie
8. Łańcuch dostaw: zasoby, przetwórstwo, logistyka
9. Transport i ruch
10. Miasto jako aktor: władza, podatki, infrastruktura, usługi publiczne
11. Wydarzenia losowe i systemy dynamiczne
12. Konkurencja: firmy sterowane przez AI
13. Gracz: kariera, progresja, cele
14. Interfejs użytkownika i inspekcja świata
15. Prezentacja: voxele, kamera, dźwięk
16. Technologia: własny silnik
17. Architektura symulacji i skalowanie
18. Zapis stanu, determinizm, modding
19. Plan implementacji (kamienie milowe)
20. Metryki sukcesu i ryzyka projektowe
21. Glosariusz

---

## 1. Wizja produktu

**Jedno zdanie:** Symulator gospodarczy, w którym gracz zaczyna jako jeden z tysięcy mieszkańców proceduralnie wygenerowanego miasta i buduje imperium przedsiębiorstw w świecie, gdzie każdy mieszkaniec, każda firma i każda tona surowca istnieją naprawdę, a każda cena wynika z faktycznego spotkania podaży z popytem.

**Co odróżnia grę od istniejących tytułów:**

- *Capitalism* symuluje rynki, ale konsumenci są abstrakcyjną masą. U nas konsumentem jest konkretny Jan Kowalski, który o 7:15 idzie do piekarni przy ul. Lipowej, bo jest bliżej niż supermarket, a chleb tam jest o 20 groszy tańszy.
- *Cities: Skylines* symuluje mieszkańców i ruch, ale gospodarka jest fasadą. U nas sklep, do którego Jan poszedł, faktycznie ma stan magazynowy, zamówił mąkę u hurtownika, hurtownik u młyna, młyn kupuje zboże z pola pod miastem, a cena zboża zależy od tegorocznych zbiorów i ceny paliwa do kombajnu.
- *Dwarf Fortress* daje głębię symulacji jednostek i emergentne historie, ale w mikroskali. U nas ta głębia działa dla całego miasta: 50–200 tysięcy mieszkańców, tysiące firm, dziesiątki tysięcy pojazdów.

**Fantazja gracza:** „Zobaczyłem, że w dzielnicy robotniczej nie ma ani jednej stacji benzynowej, a ludzie tam dojeżdżają do fabryki 12 km. Otworzyłem stację. Potem dystrybucję paliw. Potem kupiłem rafinerię. Teraz kontroluję cenę paliwa w całym mieście, a burmistrz prosi mnie o rozmowę."

**Platforma:** PC (Windows, Linux; macOS w drugiej kolejności). Gra jednoosobowa; symulacja projektowana deterministycznie, co otwiera drogę do trybu wieloosobowego w przyszłości (patrz §18).

---

## 2. Inspiracje i co z nich bierzemy

### 2.1 Capitalism / Capitalism Lab

| Element | Co przejmujemy | Co zmieniamy |
|---|---|---|
| Łańcuch produkcji (surowiec → półprodukt → produkt) | Pełna sieć zależności; każdy produkt ma recepturę | Sieć jest fizyczna: towar musi przejechać drogą, ma masę, objętość, czas transportu |
| Jakość, marka, cena jako trzy osie konkurencji | Tak, plus „dostępność lokalna" jako czwarta oś | Marka buduje się przez faktyczne doświadczenia konkretnych klientów |
| Sklepy z działami i kanałami sprzedaży | Tak | Sklep ma fizyczną lokalizację; klienci to mieszkańcy w zasięgu dojazdu |
| Giełda, przejęcia, fuzje | Tak, w późnej fazie kariery | Firmy notowane mają realne wyniki wynikające z symulacji |
| Badania i rozwój, technologie | Tak | Powiązane z rynkiem pracy (potrzebujesz inżynierów, którzy gdzieś mieszkają) |

### 2.2 Cities: Skylines

| Element | Co przejmujemy | Co zmieniamy |
|---|---|---|
| Mieszkańcy z domem, pracą, trasą dojazdu | Tak, jako fundament | Mieszkaniec jest pełnym agentem ekonomicznym z budżetem i preferencjami |
| Ruch drogowy z pathfindingiem, korki | Tak | Korki mają koszt ekonomiczny (opóźnione dostawy, spóźnieni pracownicy, zużyte paliwo) |
| Strefy, dzielnice, wartość gruntu | Tak | Gracz nie jest burmistrzem; miasto zarządza AI z własnymi celami |
| Komunikacja miejska | Tak | Linie mają koszt, rozkład, pojemność; mieszkańcy wybierają między autem a komunikacją ekonomicznie |
| Usługi publiczne (szkoły, szpitale, policja) | Tak | Wpływają na produktywność i zdrowie konkretnych mieszkańców |

### 2.3 Dwarf Fortress

| Element | Co przejmujemy | Co zmieniamy |
|---|---|---|
| Każda jednostka ma osobowość, potrzeby, relacje, historię | Tak | Skalowane do setek tysięcy agentów przez LOD symulacji |
| Emergentne narracje | Tak — dziennik zdarzeń, kroniki miasta | Narracje budowane wokół gospodarki: bankructwa, strajki, fortuny |
| Głęboka symulacja materiałów i procesów | Tak — każdy towar ma właściwości fizyczne | Ograniczamy się do właściwości istotnych ekonomicznie (masa, objętość, psucie, temperatura) |
| Świat generowany z historią | Tak — miasto ma historię, stare rody, stare firmy | Historia generowana jest „na sucho" (skrócona symulacja 50–100 lat przed startem) |
| Legenda/przeglądarka świata | Tak — możliwość zajrzenia w każdy element | UI nowoczesne, nie ASCII |

---

## 3. Filary designu

1. **Wszystko jest prawdziwe.** Nie ma abstrakcyjnych „punktów popytu". Jeśli sklep sprzedał 340 bochenków, to 340 konkretnych mieszkańców przyszło i kupiło. Jeśli fabryka stoi, to dlatego, że ciężarówka z blachą stoi w korku na moście.
2. **Cena wynika z symulacji, nigdy z tabeli.** Każda cena to wynik negocjacji między sprzedającym a kupującym na tle stanów magazynowych, kosztów i alternatyw. Jedyne wartości „zadane" to koszty wydobycia pierwotnego i parametry fizyczne świata.
3. **Gracz jest jednym z wielu.** Miasto działa bez gracza. Gracz nie ma przycisku „zbuduj drogę" — musi przekonać (lobbować, opłacić) miasto. Konkurenci AI mają te same narzędzia co gracz.
4. **Czytelność przez inspekcję.** Głębia symulacji jest bezużyteczna, jeśli gracz jej nie widzi. Każdy element (mieszkaniec, pojazd, paleta towaru, budynek) można zaznaczyć i zobaczyć jego stan, historię, powody decyzji.
5. **Determinizm.** Ten sam seed + te same akcje gracza = ten sam świat. Umożliwia replay, debugowanie, multiplayer lockstep, i buduje zaufanie gracza („to nie był rzut kostką, to ja zawaliłem").
6. **Skala jest treścią.** Emergencja pojawia się dopiero przy dziesiątkach tysięcy agentów. Nie skalujemy w dół wizji; skalujemy w górę technologię.

---

## 4. Świat: generacja miasta

### 4.1 Parametry generacji

Gracz wybiera przed startem:

- **Seed** (64-bit) — pełna determinizacja.
- **Wielkość miasta:** małe (20–40 tys. mieszkańców), średnie (60–120 tys.), duże (150–300 tys.), metropolia (400 tys.+, wymagania sprzętowe wyższe).
- **Epoka startowa:** określa dostępne technologie, strukturę gospodarki, mix transportu (np. 1990: mniej aut, więcej przemysłu ciężkiego; 2020: usługi, e-commerce, elektromobilność w zalążku).
- **Profil gospodarczy:** przemysłowe, portowe, uniwersyteckie, turystyczne, rolnicze zaplecze, mieszane. Profil ustala wagi w generacji sektorów.
- **Region geograficzny:** nadmorski, górski, nizinny, rzeczny, pustynny. Wpływa na surowce, klimat, pory roku, ryzyko katastrof.
- **Poziom trudności:** wpływa na kapitał startowy, agresywność konkurentów, częstotliwość zdarzeń, stopy procentowe.

### 4.2 Etapy generacji

**Etap 1 — Teren.** Mapa 4×4 km do 16×16 km, voxelowa, rozdzielczość 1 m w poziomie, 0,5 m w pionie. Generator: szum wielooktawowy + symulacja hydrologiczna (rzeki spływają, tworzą doliny, delty). Warstwy geologiczne: gleba, glina, piasek, skała, złoża (węgiel, ruda żelaza, ropa, gaz, kruszywa, wody gruntowe). Złoża mają objętość i koncentrację — kopalnia faktycznie je wyczerpuje.

**Etap 2 — Klimat i biomy.** Mapa temperatury i opadów (roczny cykl), lasy, pola, mokradła. Determinuje rolnictwo (co rośnie, plon/ha), zapotrzebowanie na ogrzewanie, sezonowość popytu.

**Etap 3 — Szkielet transportu.** Punkty wejścia do miasta (autostrada, linia kolejowa, port, lotnisko — zależnie od profilu). Główne arterie generowane algorytmem L-systemu ograniczonego terenem (wzniesienia, rzeki → mosty, tunele). Sieć kolejowa towarowa łączy strefy przemysłowe z węzłem zewnętrznym.

**Etap 4 — Strefowanie.** Przypisanie stref do parceli na podstawie: odległości od centrum, wartości gruntu, dostępu do transportu, hałasu, historii (starówka vs. blokowisko vs. przedmieścia). Strefy: mieszkalna (5 klas gęstości), handlowa, usługowa/biurowa, przemysłowa lekka/ciężka, logistyczna, rolnicza, publiczna, zielona, wydobywcza.

**Etap 5 — Sieć lokalna i parcele.** Ulice osiedlowe, podział na działki o realistycznych wymiarach dla typu strefy. Każda parcela ma właściciela (mieszkaniec, firma, miasto, deweloper), wartość i status (zabudowana, pusta, w budowie).

**Etap 6 — Budynki.** Proceduralna architektura voxelowa: budynek definiowany gramatyką (fundament → kondygnacje → dach → detale) parametryzowaną przez strefę, epokę budowy, wartość gruntu, styl dzielnicy. Wnętrza istnieją logicznie (piętra, lokale, mieszkania, stanowiska pracy), a wizualnie jako uproszczone bryły z możliwością „ścięcia" widoku.

**Etap 7 — Gospodarka bazowa.** Generator obsadza budynki firmami: dla każdej strefy przemysłowej losuje łańcuch (np. huta + walcownia + wytwórnia konstrukcji), dla handlowej — sieć sklepów, dla usługowej — biura, banki, restauracje. Gwarantuje spójność: każdy produkt konsumowany w mieście ma co najmniej jedno źródło (lokalne lub import).

**Etap 8 — Populacja.** Generacja rodzin (§5) tak, by: liczba stanowisk pracy ≈ liczba aktywnych zawodowo × (1 + bezrobocie docelowe); struktura wieku odpowiada piramidzie epoki; dochody rodzin odpowiadają wartości mieszkań; miejsca zamieszkania są w rozsądnej odległości od pracy (z zadanym rozkładem czasu dojazdu).

**Etap 9 — Historia „na sucho".** Symulacja 30–100 lat w trybie makro (bez ruchu, bez voxeli, wyłącznie ekonomia i demografia) w celu: ustalenia realistycznych cen wyjściowych, stanów magazynowych, zadłużenia firm, majątków rodzin, relacji między firmami (stali dostawcy), historii dzielnic (upadek przemysłu, gentryfikacja). Wynik: świat „zużyty", nie sterylny.

**Etap 10 — Weryfikacja.** Testy spójności: każdy mieszkaniec ma dom; każda firma ma pracowników i dostawcę; grafy dróg są spójne; żaden rynek nie jest w stanie nierównowagi > 30% na starcie.

### 4.3 Struktura przestrzenna

- **Świat** → **Dzielnice** (10–40) → **Kwartały** → **Parcele** → **Budynki** → **Lokale/Piętra** → **Stanowiska/Miejsca**.
- Dzielnice mają tożsamość: nazwę, historię, reputację, dominującą klasę społeczną, poziom przestępczości, średnią wartość gruntu, ceny lokalne. Mieszkańcy identyfikują się z dzielnicą.

---

## 5. Mieszkańcy: agenci, rodziny, potrzeby, tryb dnia

### 5.1 Model mieszkańca

Każdy mieszkaniec jest odrębnym agentem. Struktura danych (zorientowana na dane, komponenty ECS — patrz §17):

**Tożsamość:** id, imię, nazwisko (generowane z puli regionalnej), płeć, data urodzenia, miejsce urodzenia (dzielnica lub „przyjezdny"), rodzina (id), gospodarstwo domowe (id).

**Cechy stałe (osobowość, 0–100):**
- Ambicja — skłonność do zmiany pracy, awansu, zakładania firmy.
- Oszczędność — skłonność do oszczędzania vs. konsumpcji.
- Wrażliwość cenowa — jak mocno cena wpływa na wybór sklepu/produktu.
- Lojalność — przywiązanie do marek, sklepów, pracodawcy.
- Towarzyskość — potrzeba rozrywki, restauracji, kontaktów.
- Otwartość — skłonność do próbowania nowych produktów.
- Ryzyko — zakłady, inwestycje, przedsiębiorczość.
- Sumienność — produktywność, absencja.

**Cechy dynamiczne:**
- Zdrowie (0–100), Energia (0–100), Nastrój (−100..100), Stres.
- Wykształcenie: poziom (podstawowe → wyższe) + kierunek (techniczny, humanistyczny, medyczny, ekonomiczny, artystyczny).
- Umiejętności: słownik zawód → poziom (0–100), rośnie przez pracę, spada przez nieużywanie.
- Status społeczny (§5.4).
- Majątek: gotówka, konto, oszczędności, kredyty, aktywa (mieszkanie, samochód, akcje).

**Relacje:** graf ważony do innych mieszkańców (rodzina, przyjaciele, współpracownicy, sąsiedzi, znajomi ze sklepu). Wartość relacji wpływa na przepływ informacji (plotka o dobrym sklepie, o zwolnieniach) i decyzje (praca u znajomego).

**Pamięć:** ostatnie N doświadczeń z produktami, sklepami, pracodawcami, usługami (ocena 0–100, data). To baza pod lojalność i markę.

### 5.2 Rodziny i gospodarstwa domowe

- **Gospodarstwo domowe (GD)** to jednostka ekonomiczna: wspólny budżet, wspólne mieszkanie, wspólny samochód (lub kilka), wspólne zakupy spożywcze.
- Typy GD: singiel, para, rodzina z dziećmi (1–4), rodzina wielopokoleniowa, współlokatorzy, student w akademiku, senior samotny.
- **Budżet GD:** dochody (pensje, zasiłki, emerytury, dywidendy, czynsz z wynajmu) − wydatki stałe (czynsz/rata, media, ubezpieczenia, raty) − wydatki zmienne (żywność, paliwo, odzież, rozrywka) → oszczędności lub dług.
- Podział ról: kto robi zakupy (rotacja zależna od grafików pracy), kto odwozi dzieci, kto używa samochodu.
- **Cykl życia:** narodziny, przedszkole/szkoła, studia/praca, związek (dobór partnera z grafu relacji + kompatybilność statusu), ślub, dzieci, rozwód (prawdopodobieństwo zależne od stresu i finansów), starzenie, emerytura, choroba, śmierć, dziedziczenie (majątek przechodzi na rodzinę — w tym firmy).
- **Migracja:** GD wyprowadza się z miasta, jeśli długotrwały brak pracy/mieszkania; nowe GD wprowadzają się, gdy jest praca i mieszkania. Ruch ten reguluje populację bez „spawnowania".

### 5.3 Potrzeby

Model hierarchiczny z ciągłymi poziomami zaspokojenia (0–100) i tempem spadku:

| Potrzeba | Tempo spadku | Zaspokajana przez | Skutek deprywacji |
|---|---|---|---|
| Głód | godziny | żywność (dom, restauracja, bar) | spadek energii, zdrowia, produktywności |
| Sen | dobowe | dom, hotel | energia, wypadki, absencja |
| Higiena | dobowe | dom (woda, środki czystości) | zdrowie, status |
| Zdrowie | zdarzeniowe | leki, lekarz, szpital | absencja, śmierć |
| Bezpieczeństwo | wolne | dzielnica, policja, ubezpieczenie | stres, migracja |
| Mieszkanie | wolne | odpowiednie mieszkanie | stres, poszukiwanie nowego |
| Mobilność | dobowe | samochód/komunikacja/rower | dostęp do pracy i sklepów |
| Odzież | tygodniowe/sezonowe | sklepy odzieżowe | status, komfort termiczny |
| Rozrywka | dobowe | kino, restauracja, park, TV, gry, sport | nastrój, produktywność |
| Kontakt społeczny | dobowe | rodzina, znajomi, lokale | nastrój |
| Status | wolne | dobra luksusowe, adres, auto, marka | ambicja, satysfakcja |
| Rozwój | wolne | edukacja, kursy, awans | ambicja, zmiana pracy |

Każda potrzeba mapuje się na **koszyk dóbr i usług** z substytutami (głód: chleb, makaron, ryż, gotowe dania, restauracja...). Wybór konkretnego produktu — §6.4.

### 5.4 Status społeczny i klasy

- Status = f(dochód, majątek, wykształcenie, zawód, adres, konsumpcja statusowa, reputacja rodziny).
- Klasy (dla czytelności i generowania): niższa, robotnicza, niższa średnia, wyższa średnia, wyższa, elita. Klasa nie jest etykietą stałą, lecz przedziałem statusu.
- Wpływ statusu: dostęp do pracy (sieć znajomych), preferencje zakupowe (marka/jakość vs. cena), miejsce zamieszkania, szkoła dzieci, kompatybilność partnerska, wpływ polityczny (lobbing).
- **Mobilność społeczna** jest realna: dziecko robotnika, które skończy studia i awansuje, przesuwa się w górę; to podstawa emergentnych historii.

### 5.5 Tryb dnia (harmonogram)

Każdy mieszkaniec ma **plan dnia** generowany co wieczór na następną dobę (i przeplanowywany przy zdarzeniach). Planer to system celów z priorytetami:

1. **Zobowiązania stałe:** praca (grafik: zmiana 6–14, 8–16, 14–22, 22–6, elastyczna, weekendowa), szkoła, odwożenie dzieci.
2. **Potrzeby krytyczne:** jedzenie, sen — wstawiane jako okna.
3. **Zadania:** zakupy (gdy zapasy GD spadną poniżej progu), tankowanie (poziom paliwa < próg), wizyta u lekarza, urząd, załatwianie mieszkania/pracy.
4. **Czas wolny:** wypełniany wg osobowości i budżetu (restauracja, kino, park, wizyta u znajomych, dom).

Dla każdego zadania planer wybiera **miejsce** (sklep, stacja, kino) na podstawie funkcji użyteczności (§6.4) i **środek transportu** (§9.4), a następnie układa trasy tak, by minimalizować całkowity koszt (czas + pieniądze) — np. łączy tankowanie z powrotem z pracy, zakupy z odbiorem dzieci.

Przykład (widoczny po zaznaczeniu mieszkańca):

```
Anna Wiśniewska, 34 l., księgowa, Dąbrowa Górna 12/4
06:30  Pobudka (sen 92%)
06:45  Śniadanie w domu (zapasy: pieczywo 1 dzień, nabiał 3 dni)
07:20  Wyjście — samochód (paliwo 31%, planowane tankowanie)
07:35  Stacja „Orlex" Wołoska — tankowanie 35 l @ 6,42 zł (wybrana: 
       najtańsza na trasie; alternatywa „PetroMax" +0,18 zł/l)
07:55  Praca: Biuro Rachunkowe „Bilans" (zmiana 8–16)
16:10  Wyjście z pracy
16:40  Market „Dobry Koszyk" Lipowa — zakupy spożywcze, budżet 140 zł
       (wybrany: 2. najtańszy, ale po drodze; brak chleba w „Piekarni u Kazi")
17:20  Dom — kolacja, dzieci
19:00  Czas wolny: TV (budżet rozrywki wyczerpany do 20.)
22:30  Sen
```

### 5.6 Decyzje długoterminowe

- **Praca:** mieszkaniec przegląda oferty (zasięg: dojazd ≤ próg osobisty), ocenia: pensja netto − koszt dojazdu, dopasowanie umiejętności, reputacja pracodawcy (z pamięci i plotek), warunki (zmiany, stres). Zmienia pracę, gdy różnica użyteczności > próg (ambicja obniża próg, lojalność podwyższa).
- **Mieszkanie:** zmiana przy zmianie wielkości GD, zmianie dochodu, przeprowadzce pracy, spadku bezpieczeństwa dzielnicy. Rynek nieruchomości: kupno/wynajem, kredyty hipoteczne (bank ocenia zdolność), aukcje.
- **Samochód:** kupno, gdy koszt komunikacji + czas > koszt posiadania; wybór klasy wg statusu i budżetu; auto zużywa się, wymaga serwisu, ubezpieczenia.
- **Edukacja:** dziecko → szkoła najbliższa lub lepsza (jeśli rodzice mogą dowozić); dorosły → kursy, studia zaoczne (ambicja + budżet).
- **Przedsiębiorczość:** mieszkaniec z wysoką ambicją, ryzykiem, kapitałem i wykrytą niszą (widzi, że w jego dzielnicy brakuje X) zakłada firmę → staje się konkurentem AI (§12).

### 5.7 Informacja i plotka

Mieszkańcy nie mają wiedzy doskonałej. Wiedzą o: sklepach, w których byli; sklepach, o których słyszeli od relacji (waga relacji × świeżość); sklepach widocznych z trasy dojazdu; reklamie (§7.6). Nowy sklep gracza na początku ma klientów tylko z przechodniów — kampania i rekomendacje budują zasięg. To realizuje „markę" bez abstrakcji.

---

## 6. Gospodarka: podaż, popyt, ceny, pieniądz

### 6.1 Zasada nadrzędna

Nie istnieje globalna „cena rynkowa" produktu. Istnieją **oferty**: konkretny sprzedawca, konkretny produkt, konkretna lokalizacja, konkretna cena, konkretna ilość. Cena obserwowana w UI to agregat ofert. Każda transakcja to spotkanie oferty z konkretnym kupującym.

### 6.2 Rynki

Trzy warstwy rynków, wszystkie oparte na tym samym mechanizmie ofert:

1. **Detaliczny (B2C):** mieszkaniec ↔ sklep/usługa. Ceny ustala sprzedawca (gracz ręcznie, AI algorytmicznie). Kupujący wybiera wg użyteczności (§6.4).
2. **Hurtowy (B2B):** firma ↔ firma. Dwa tryby:
   - *Spot:* kupujący wysyła zapytanie do dostawców w zasięgu, otrzymuje oferty (cena, ilość, termin, koszt transportu), wybiera.
   - *Kontrakt:* umowa na okres (tygodnie–lata): ilość, cena (stała, indeksowana do surowca, lub z widełkami), kary za niedostarczenie. Kontrakty stabilizują łańcuchy i są głównym narzędziem gracza-producenta.
3. **Zewnętrzny (import/eksport):** „reszta świata" jako duży, ale nie nieskończony partner z cenami bazowymi podlegającymi trendom i zdarzeniom globalnym, plus koszt transportu, cło, czas dostawy (dni–tygodnie). Import nie jest darmowym zaworem: ograniczona przepustowość węzła (port, kolej, autostrada), a duże zakupy podnoszą cenę zewnętrzną.

### 6.3 Ustalanie cen

**Sprzedawca AI** używa polityki cenowej z parametrami zależnymi od osobowości firmy (§12): cena bazowa = koszt jednostkowy × (1 + marża docelowa); korekty codzienne wg: stanu magazynu vs. docelowego (nadmiar → obniżka, brak → podwyżka), obserwowanych cen konkurencji w zasięgu (firma „widzi" ceny konkurentów z opóźnieniem 1–7 dni), elastyczności obserwowanej (eksperymenty cenowe), sezonu, psucia się towaru (przeceny).

**Gracz** ustala ceny ręcznie lub deleguje polityki (np. „−2% względem najtańszego konkurenta w promieniu 3 km", „marża 25%", „cena dynamiczna z limitem"), z tym samym zestawem narzędzi co AI.

**Surowce pierwotne:** koszt wydobycia wynika z parametrów złoża (koncentracja, głębokość), zużycia maszyn, płac, energii. Kopalnia ustala cenę jak każda firma. Nie ma „ceny światowej ropy" jako zmiennej — jest cena importu ropy (trend + zdarzenia) i cena lokalnych producentów.

### 6.4 Decyzja zakupowa mieszkańca

Dla potrzeby P mieszkaniec M rozważa koszyk produktów zaspokajających P i dostępne miejsca ich zakupu (znane mu — §5.7). Dla każdej pary (produkt, miejsce) liczy użyteczność:

```
U = w_cena · f(cena / budżet_P)
  + w_jakość · jakość_postrzegana
  + w_marka · afinitet_do_marki
  + w_odległość · g(koszt_dojazdu w czasie i pieniądzu)
  + w_lojalność · historia_z_tym_sklepem
  + w_status · dopasowanie_do_statusu
  + w_nowość · (nieznany produkt) · otwartość
  + szum
```

Wagi `w_*` wynikają z osobowości i statusu. Wybór: softmax po użyteczności (nie deterministyczne argmax — daje rozkład, ale seedowany, więc powtarzalny). Jeśli najlepsza opcja ma U < próg — mieszkaniec odkłada zakup, kupuje substytut niższego rzędu lub ogranicza konsumpcję (spadek zaspokojenia → skutki z §5.3).

**Elastyczność cenowa** nie jest parametrem — jest własnością emergentną rozkładu wrażliwości cenowej i alternatyw w populacji. Gracz może ją zmierzyć eksperymentem (i widzi to w raporcie).

### 6.5 Pieniądz, banki, kredyt

- Jedna waluta. Podaż pieniądza modelowana przez system bankowy: banki przyjmują depozyty (GD, firmy), udzielają kredytów (hipoteczne, konsumpcyjne, inwestycyjne, obrotowe), oceniają ryzyko (historia, zabezpieczenie, przepływy).
- **Bank centralny** (poza miastem, abstrakcja) ustala stopę bazową reagując na inflację mierzoną koszykiem miejskim. Stopy wpływają na koszt kredytu, skłonność do oszczędzania, wyceny.
- **Inflacja** jest emergentna: więcej pieniądza (kredyt) przy stałej podaży dóbr → wzrost cen w symulacji ofert.
- **Giełda** (późna faza): firmy mogą wejść na giełdę; cena akcji wynika z transakcji między inwestorami (mieszkańcy z majątkiem, fundusze AI, gracz), którzy wyceniają na podstawie publikowanych wyników (kwartalnych — z opóźnieniem i szumem) i plotek. Przejęcia wrogie i przyjazne, dywidendy, emisje.
- **Ubezpieczenia:** firmy ubezpieczeniowe wyceniają ryzyko na podstawie historii zdarzeń; zdarzenia losowe (§11) generują wypłaty.

### 6.6 Rynek pracy

- Każde stanowisko to oferta: zawód, wymagane umiejętności, pensja, grafik, lokalizacja. Firmy publikują oferty, mieszkańcy aplikują (§5.6), firma wybiera (dopasowanie, oczekiwania płacowe, referencje z grafu relacji).
- Pensje ustalają się w symulacji: gdy brakuje spawaczy, firmy licytują pensje; gdy jest nadmiar, mieszkańcy akceptują mniej.
- **Związki zawodowe i strajki:** przy dużym rozdźwięku między zyskiem firmy a płacami i przy sieci relacji między pracownikami — formuje się żądanie; odmowa → strajk (produkcja stoi).
- **Produktywność** pracownika = f(umiejętność, energia, nastrój, zdrowie, narzędzia/technologia, zarządzanie).

### 6.7 Rynek nieruchomości i gruntów

- Wartość gruntu = f(dostęp do pracy, sklepów, usług publicznych, hałas, zanieczyszczenie, prestiż dzielnicy, popyt). Aktualizowana z transakcji (sprzedaże parcel i mieszkań).
- Deweloperzy AI kupują grunt, budują, sprzedają/wynajmują. Gracz może zostać deweloperem.
- Czynsze wynikają z ofert wynajmujących i wyborów najemców.
- Miasto pobiera podatek od nieruchomości od wartości.

### 6.8 Podatki i przepływy publiczne

CIT, PIT, VAT (na etapie sprzedaży detalicznej), podatek od nieruchomości, akcyza (paliwo, alkohol, tytoń), cła, opłaty koncesyjne. Trafiają do budżetu miasta (§10). Gracz widzi w księgowości każde obciążenie.

### 6.9 Księgowość

Każda firma prowadzi pełną księgę: rachunek zysków i strat, bilans, przepływy pieniężne, per zakład/sklep/produkt. Amortyzacja majątku, zapasy wyceniane po koszcie, należności/zobowiązania z terminami (kontrahenci płacą z opóźnieniem; ryzyko niewypłacalności partnera).

---

## 7. Przedsiębiorstwa: typy, budynki, zarządzanie

### 7.1 Model firmy

Firma = osoba prawna z: właścicielem/właścicielami (mieszkańcy, inne firmy, gracz, miasto), kapitałem, zakładami (fizycznymi budynkami), pracownikami, portfelem produktów, kontraktami, kredytami, marką, historią.

Struktura: **Firma** → **Zakłady** (fabryka, sklep, magazyn, biuro, kopalnia...) → **Działy/Linie** → **Stanowiska**.

### 7.2 Katalog typów zakładów

**Wydobycie i pozyskanie:** kopalnia odkrywkowa, kopalnia głębinowa, szyb naftowy, gazowy, kamieniołom, żwirownia, ujęcie wody, tartak/leśnictwo, gospodarstwo rolne (zboża, warzywa, sady), hodowla (bydło, trzoda, drób), rybołówstwo/port rybacki, farma wiatrowa/słoneczna, elektrownia (węgiel, gaz, wodna, atomowa), ciepłownia.

**Przetwórstwo pierwotne:** rafineria, huta, walcownia, koksownia, młyn, mleczarnia, rzeźnia/masarnia, tartak, cementownia, zakład chemiczny (nawozy, tworzywa), papiernia, wytwórnia szkła, elektrownia.

**Produkcja:** piekarnia przemysłowa, zakład spożywczy (konserwy, napoje, słodycze, mrożonki), browar/gorzelnia, szwalnia, fabryka obuwia, fabryka mebli, fabryka AGD, fabryka elektroniki, montownia samochodów, fabryka części, fabryka maszyn, zakład farmaceutyczny, kosmetyczny, drukarnia, wytwórnia materiałów budowlanych, prefabrykaty.

**Logistyka:** magazyn, centrum dystrybucyjne, hurtownia (branżowa), firma transportowa (ciężarówki, tabor kolejowy), spedycja, terminal paliwowy, port/terminal kontenerowy, firma kurierska.

**Handel detaliczny:** sklep osiedlowy, supermarket, hipermarket, dyskont, sklep specjalistyczny (odzież, elektronika, meble, budowlany, apteka, księgarnia), stacja paliw, salon samochodowy, targ/bazar, e-commerce (magazyn + kurier).

**Usługi dla ludności:** restauracja, bar, kawiarnia, fast food, hotel, fryzjer, siłownia, kino, klub, warsztat samochodowy, myjnia, przychodnia prywatna, szkoła prywatna, przedszkole, pralnia, salon urody.

**Usługi dla biznesu:** biuro rachunkowe, kancelaria, agencja reklamowa, firma IT/software, ochrona, sprzątanie, HR/agencja pracy, konsulting, laboratorium badawcze, biuro projektowe, firma budowlana.

**Finanse:** bank, kasa pożyczkowa, ubezpieczyciel, dom maklerski, fundusz inwestycyjny, leasing.

**Media:** gazeta, radio, telewizja lokalna, portal — sprzedają reklamę, kształtują plotkę i opinię (§7.6, §11).

**Nieruchomości:** deweloper, zarządca, agencja.

### 7.3 Zakład: model fizyczny

- **Budynek** na parceli, z pojemnością (m²) determinującą liczbę linii/stanowisk/regałów.
- **Wyposażenie:** maszyny/linie o zadanej wydajności, zużyciu energii, awaryjności, wieku; wymagają konserwacji i części (z łańcucha dostaw!).
- **Magazyn wejściowy/wyjściowy:** pojemność w m³ i tonach, warunki (chłodnia, silos, zbiornik), towary psujące się mają datę przydatności.
- **Media:** przyłącza prądu, gazu, wody, ciepła — zużycie liczone, faktury od dostawców (którymi mogą być gracz lub AI).
- **Rampa/parking:** przepustowość dostaw (ciężarówek/godz.) — wąskie gardło realne.
- **Emisje:** hałas, zanieczyszczenie powietrza/wody — wpływają na wartość gruntu i zdrowie okolicy, generują konflikty z mieszkańcami i miastem.

### 7.4 Produkcja

- **Receptura:** wejścia (ilość, jakość min.) → proces (czas, energia, praca, maszyna) → wyjścia (produkt, odpady, produkt uboczny). Jakość wyjścia = f(jakość wejść, poziom umiejętności załogi, technologia, kontrola jakości).
- **Harmonogram:** zmiany (1–3), przestoje, konserwacja, przezbrojenia przy zmianie produktu.
- **Odpady:** trzeba je wywieźć (koszt) lub sprzedać (złom, makulatura) — kolejny łańcuch.
- **Produkt:** ma cechy: jakość (0–100), cena, marka, cechy specjalne z R&D (np. energooszczędny), masa, objętość, trwałość, kategoria potrzeby.

### 7.5 Pracownicy i zarządzanie

- **Stanowiska:** robotnik, operator, technik, inżynier, sprzedawca, kierowca, magazynier, księgowy, menedżer, dyrektor, badacz, specjalista marketingu... każde z profilem umiejętności.
- **Menedżerowie** mają realny wpływ: jakość zarządzania zakładem (z umiejętności menedżera) modyfikuje produktywność, rotację, straty. Gracz nie może „mikrozarządzać" wszystkiego — deleguje zakłady menedżerom, którzy stosują polityki. Dobry menedżer to zasób rzadki i podkupywany.
- **HR:** rekrutacja (ogłoszenia, agencje, headhunting), szkolenia (podnoszą umiejętności, kosztują czas), premie, benefity (auto służbowe, opieka medyczna — realne zaspokojenie potrzeb pracownika), zwolnienia (odprawy, reputacja).
- **Rotacja:** pracownik odchodzi, gdy dostanie lepszą ofertę (§5.6) lub gdy nastrój/stres przekroczą próg. Odejście kluczowego inżyniera obniża jakość.

### 7.6 Marketing i marka

- **Marka** to zbiór afinitetów w pamięci mieszkańców, nie liczba. Wskaźnik „siła marki" w UI = agregat.
- Kanały: reklama zewnętrzna (billboardy przy konkretnych ulicach — zasięg to mieszkańcy, którzy tamtędy jeżdżą), gazeta/radio/TV (zasięg wg czytelnictwa dzielnic), ulotki, promocje w sklepie, sponsoring (drużyna, festyn), PR (reakcja na zdarzenia).
- Reklama zwiększa **znajomość** produktu/sklepu (§5.7) i **oczekiwaną jakość**; jeśli rzeczywista jakość jest niższa — doświadczenie psuje markę szybciej niż reklama ją buduje.

### 7.7 Badania i rozwój

- Dział R&D (badacze, laboratorium, budżet) generuje: ulepszenia jakości, obniżki kosztów (proces), nowe cechy produktów, nowe produkty w kategorii (np. napój energetyczny w kategorii napojów), ulepszenia maszyn.
- Drzewo technologii per branża; postęp zależny od epoki (technologie „światowe" pojawiają się w czasie, firma może je licencjonować lub odkryć wcześniej).
- Patenty: ochrona czasowa; licencjonowanie jako źródło przychodu.

### 7.8 Finanse firmy

Kredyty obrotowe i inwestycyjne, leasing maszyn/pojazdów, faktoring, emisja akcji, obligacje. Bankructwo: gdy brak płynności → wierzyciele, syndyk, wyprzedaż majątku (okazja dla gracza), pracownicy tracą pracę (skutki w dzielnicy).

### 7.9 Relacje międzyfirmowe

Stali dostawcy (rabaty, priorytet w niedoborze), ekskluzywność, kartele (nielegalne — ryzyko UOKiK-podobnego urzędu), franczyza, joint venture, integracja pionowa (kupno dostawcy) i pozioma (kupno konkurenta).

---

## 8. Łańcuch dostaw: zasoby, przetwórstwo, logistyka

### 8.1 Drzewo produktów (fragment)

Pełny katalog: ~400 towarów w ~60 kategoriach potrzeb. Przykładowe łańcuchy:

**Paliwo:**
złoże ropy → szyb naftowy (ropa surowa) → rurociąg/cysterna kolejowa → rafineria (benzyna, diesel, LPG, asfalt, oleje) → terminal paliwowy → cysterna drogowa → stacja paliw → bak samochodu mieszkańca / firmy transportowej / kombajnu rolnika.
Zależności zwrotne: rafineria potrzebuje prądu (elektrownia potrzebuje węgla/gazu), części zamiennych (fabryka maszyn), chemikaliów (zakład chemiczny potrzebuje ropy...). Cysterny palą diesel.

**Chleb:**
pole (nasiona, nawóz, paliwo, praca sezonowa) → zboże → elewator → młyn (mąka, otręby → pasza) → hurtownia spożywcza / bezpośrednio → piekarnia (mąka + drożdże + sól + woda + energia + praca) → chleb (przydatność 2 dni) → sklep osiedlowy / supermarket / restauracja → mieszkaniec.

**Samochód:**
ruda żelaza + węgiel → huta (stal) → walcownia (blacha) → tłocznia (karoseria); ropa → tworzywa → części; miedź → wiązki; guma → opony; elektronika → sterowniki → montownia → salon → mieszkaniec (kredyt z banku, ubezpieczenie, potem serwis, paliwo, części).

**Budynek:**
kamieniołom → cementownia (cement) + żwirownia + stal + tartak (drewno) + szkło + instalacje → firma budowlana (praca, maszyny) → budynek na parceli dewelopera → mieszkanie/lokal → najemca.

### 8.2 Właściwości towaru

Każda jednostka towaru (partia) niesie: rodzaj, ilość, masę, objętość, jakość, markę, producenta, datę produkcji, datę przydatności, wymagania przechowywania (temp., suchość, ciśnienie), klasę niebezpieczeństwa (paliwo, chemia — wymogi transportu), koszt jednostkowy (do wyceny zapasów). Partie są śledzone od źródła — możliwa jest kontrola pochodzenia i afery (§11).

### 8.3 Logistyka

- **Środki transportu:** furgonetka, ciężarówka (skrzynia, chłodnia, cysterna, wywrotka, kontenerowa), pociąg towarowy (wagony typowane), rurociąg, statek (jeśli port), transport wewnętrzny (wózki).
- Pojazd ma: ładowność (t, m³), typ ładunku, spalanie, zużycie, koszt/km, kierowcę (mieszkaniec z uprawnieniami!), stan techniczny.
- **Zlecenie transportowe:** skąd, dokąd, co, kiedy najpóźniej. Realizowane przez flotę własną firmy lub firmę transportową (rynek spot/kontrakt). Trasa planowana na sieci dróg z uwzględnieniem ograniczeń (tonaż mostów, zakaz ruchu ciężkiego w centrum, godziny dostaw).
- **Czas** jest realny: dostawa z hurtowni po drugiej stronie miasta trwa 40 minut + korki + rozładunek. Zakład planuje zamówienia z wyprzedzeniem (polityka zapasów: min/max, just-in-time, bufor sezonowy).
- **Magazyny i dystrybucja:** gracz-detalista z 10 sklepami będzie chciał centrum dystrybucyjnego — jedna dostawa dużą ciężarówką od producenta, rozwózka furgonetkami. Symulacja to nagradza naturalnie (koszt/t·km, przepustowość ramp).

### 8.4 Niedobory i substytucja

Gdy dostawca nie dostarcza: zakład zużywa bufor → obniża produkcję → szuka spot (drożej) → import (wolniej) → zmienia recepturę na substytut (jeśli istnieje, np. olej rzepakowy zamiast słonecznikowego) → staje. Sklep: brak towaru na półce → klienci kupują substytut lub idą do konkurencji (pamięć: „u nich nie było"). Niedobór propaguje się w dół łańcucha z opóźnieniem — to źródło dynamiki i okazji.

### 8.5 Import/eksport

Węzły graniczne mają przepustowość i czas. Import obsługują firmy spedycyjne (AI lub gracz). Cła i przepisy epoki. Eksport: gdy cena lokalna < zewnętrzna − koszt transportu, producent może sprzedać na zewnątrz (drenaż lokalnej podaży → wzrost cen w mieście — kolejna emergencja).

---

## 9. Transport i ruch

### 9.1 Sieć

Graf dróg z pasami, skrzyżowaniami (sygnalizacja, ronda, pierwszeństwo), ograniczeniami prędkości, klasami (autostrada, arteria, lokalna, osiedlowa, gruntowa), tonażem, parkingami (pojemność!). Sieć kolejowa (pasażerska, towarowa), tramwajowa, ścieżki rowerowe, chodniki. Rzeki i mosty (przepustowość, remonty).

### 9.2 Symulacja ruchu

Model hybrydowy:
- **Mikroskopowy** (pojazd po pojazdzie, model car-following + zmiana pasa) w obszarze widocznym dla gracza i na drogach o wysokim obciążeniu.
- **Mezoskopowy** (kolejki na krawędziach grafu, przepływy) poza widokiem.
Oba spójne w wynikach (czas przejazdu, zużycie paliwa) — przełączenie LOD nie zmienia rezultatu ekonomicznego (§17.4).

### 9.3 Komunikacja miejska

Linie autobusowe, tramwajowe, kolej podmiejska, metro (metropolia). Każda linia: trasa, przystanki, rozkład, tabor (pojemność, spalanie/prąd, koszt), kierowcy (mieszkańcy). Operator: miasto (dotowane) lub prywatny (gracz może wygrać przetarg). Mieszkaniec zna rozkład ze swojego przystanku i oblicza czas dojścia + oczekiwania + przejazdu + przesiadki. Przepełnione autobusy zostawiają pasażerów (spóźnienie do pracy → produktywność).

### 9.4 Wybór środka transportu przez mieszkańca

Dla każdej podróży: koszt uogólniony = czas × wartość czasu (zależna od dochodu i celu) + koszt pieniężny (paliwo, bilet, parking, amortyzacja) + wygoda (pogoda, bagaż, status). Opcje: pieszo, rower, komunikacja, samochód własny, samochód rodziny (jeśli wolny), taxi, carpooling z relacji. Auto wymaga miejsca parkingowego u celu — brak parkingu przy sklepie zmniejsza jego zasięg dla kierowców.

### 9.5 Paliwo i energia w transporcie

Każdy pojazd ma bak/baterię; zużycie liczone per przejazd (prędkość, korki, masa). Tankowanie to zadanie w planie dnia z wyborem stacji wg użyteczności (cena, trasa, marka, kolejka). Stacja ma zbiorniki (pojemność, poziom), dostawy cysterną, ceny per paliwo. Elektromobilność (epokowo): ładowarki, obciążenie sieci energetycznej.

### 9.6 Sieci przesyłowe

Prąd (elektrownie → linie → transformatory → odbiorcy; awarie, przeciążenia, blackout), gaz, woda/kanalizacja, ciepło sieciowe, telekomunikacja. Każda sieć to firma/firmy z taryfami. Zakład bez prądu stoi.

---

## 10. Miasto jako aktor

### 10.1 Władza miejska (AI)

Burmistrz i rada z celami (poparcie, budżet, rozwój) i preferencjami (prorozwojowa, socjalna, ekologiczna, populistyczna). Decyzje: stawki podatków, inwestycje (drogi, szkoły, komunikacja), strefowanie i wydawanie pozwoleń, przetargi (komunikacja, wywóz śmieci, budowa), regulacje (zakaz ruchu ciężkiego, normy emisji, godziny handlu), dotacje.

### 10.2 Wybory

Co N lat mieszkańcy głosują (frekwencja i preferencje wg statusu, nastroju, dzielnicy, mediów). Gracz może wspierać kandydatów (finansowo, medialnie) — legalnie i nie; wpływ na politykę wobec jego branży.

### 10.3 Usługi publiczne

Szkoły (jakość → wykształcenie dzieci), szpitale (zdrowie, absencja), policja (przestępczość → bezpieczeństwo, kradzieże w sklepach, straty), straż pożarna (pożary zakładów), wywóz odpadów, parki (rozrywka, wartość gruntu), urzędy (czas załatwiania pozwoleń — realny koszt dla gracza).

### 10.4 Prawo i egzekucja

Urząd ochrony konkurencji (kartele, monopole → kary, przymusowy podział), inspekcja pracy, sanepid (zamknięcie restauracji), ochrona środowiska (kary za emisje), urząd skarbowy (kontrole; szara strefa jako opcja z ryzykiem).

---

## 11. Wydarzenia losowe i systemy dynamiczne

### 11.1 Zasady

Zdarzenia losowane z seedowanego generatora, z prawdopodobieństwami zależnymi od stanu świata (nie z listy „raz na miesiąc coś się dzieje"). Zdarzenie zmienia parametry symulacji — skutki cenowe są emergentne, nie zadane („susza obniża plon o 40%" — a nie „susza podnosi cenę chleba o 15%").

### 11.2 Katalog (kategorie)

**Naturalne:** susza, powódź, mróz, upał, burza, pożar lasu, epidemia (grypa → absencja, popyt na leki), plaga szkodników, wyczerpanie złoża, trzęsienie (region górski).

**Infrastrukturalne:** awaria elektrowni, pęknięcie rurociągu, zamknięcie mostu na remont, awaria sygnalizacji, wypadek blokujący arterię, awaria linii tramwajowej.

**Ekonomiczne zewnętrzne:** szok cenowy ropy (embargo, wojna „w świecie"), recesja globalna, boom eksportowy, zmiana kursu, nowe cło, nowa technologia dostępna do licencji, pojawienie się zewnętrznej sieci handlowej wchodzącej do miasta.

**Społeczne:** strajk, protest przeciw fabryce, moda na produkt (viral), skandal (zatrucie partią towaru — śledzone do producenta), przestępczość zorganizowana (wymuszenia), festyn/koncert (popyt lokalny), fala migracji.

**Firmowe:** awaria maszyny, wypadek w pracy (inspekcja), odejście kluczowej osoby, kradzież, pożar magazynu, błąd w partii, proces sądowy, pozytywna recenzja w mediach.

**Polityczne:** zmiana podatków, nowa strefa, decyzja o obwodnicy (zmienia wartość gruntów — kto wiedział wcześniej?), przetarg, dotacja unijna-podobna.

### 11.3 Systemy dynamiczne stałe

- **Pory roku i pogoda:** popyt na ogrzewanie, napoje, odzież, sezonowość rolnictwa, ruch (śnieg spowalnia), budownictwo.
- **Cykl koniunkturalny:** emergentny z kredytu, inwestycji i nastroju; wzmacniany zdarzeniami zewnętrznymi.
- **Postęp technologiczny epoki:** stopniowe pojawianie się nowych kategorii produktów (telefon komórkowy, internet, e-commerce, EV), które zmieniają potrzeby i łańcuchy.
- **Demografia:** starzenie, dzietność zależna od warunków, migracje.

---

## 12. Konkurencja: firmy sterowane przez AI

### 12.1 Osobowość firmy

Każda firma AI ma dyrektora (mieszkaniec!) i strategię wynikającą z jego osobowości i sytuacji: agresywna ekspansja, ostrożna, niszowa jakość, cenowa (dyskont), innowacyjna, konsolidacyjna. Duże firmy zewnętrzne (sieci) mają osobowość korporacyjną.

### 12.2 Zachowania

Wycena, zaopatrzenie, zatrudnianie, otwieranie/zamykanie zakładów, R&D, marketing, kredyty, ekspansja do nowych dzielnic, reakcja na wejście gracza (wojna cenowa, przejęcie dostawcy, przeciągnięcie pracowników). Decyzje oparte na tych samych danych, które widzi gracz (z ograniczeniami informacji — konkurent nie zna cen kosztów gracza).

### 12.3 Poziomy sterowania

- **Operacyjny (codzienny):** reguły + heurystyki (ceny, zamówienia) — tanie obliczeniowo, dla tysięcy firm.
- **Taktyczny (miesięczny):** ocena rentowności zakładów, zmiany polityki.
- **Strategiczny (kwartalny/roczny):** planowanie z wykorzystaniem symulacji „co jeśli" na uproszczonym modelu (ten sam kod makro co historia „na sucho").

### 12.4 Powstawanie i upadek

Mieszkańcy zakładają firmy (§5.6); firmy bankrutują; sieci zewnętrzne wchodzą, gdy miasto osiąga progi atrakcyjności. Ekosystem konkurencji jest otwarty — gracz nigdy nie „wyczyści" rynku na stałe.

---

## 13. Gracz: kariera, progresja, cele

### 13.1 Start

Gracz wybiera (lub losuje) mieszkańca jako postać: z jego domem, rodziną, pracą, oszczędnościami, znajomymi. Warianty startu: absolwent bez kapitału (pożyczka od rodziny), doświadczony pracownik z oszczędnościami, spadkobierca małej firmy, inwestor z zewnątrz (kapitał, brak sieci), tryb sandbox (dowolny kapitał).

**Tryb przeglądu** jest szóstą drogą i nie jest wariantem startu, tylko jego brakiem: gracz wchodzi do gotowego miasta **bez postaci** i ogląda je — klika mieszkańców, sklepy, zakłady i budynki, przewija czas, włącza nakładki danych. Nikogo nie prowadzi, więc nie ma czym wydać komendy, a majątek w wierszu zapisu jest zerem. Istnieje po to, żeby miasto dało się poznać, zanim zdecyduje się, kim w nim być — i dlatego wyjście z niego prowadzi przez nową grę, a nie przez wybór postaci w locie (to jest `SetCharacter` w środku rozgrywki i należy do sukcesji, §13.4).

### 13.2 Ścieżka

1. **Pracownik:** gracz może pracować (dochód, umiejętności, relacje, obserwacja branży od środka). Zdobywa wiedzę o mieście przez życie w nim.
2. **Pierwszy biznes:** kiosk, food truck, sklep, warsztat, przewóz furgonetką. Osobiste zaangażowanie (gracz-postać pracuje w sklepie — realne godziny).
3. **Firma:** zatrudnienie, menedżer, drugi punkt, dostawy własne.
4. **Grupa:** wiele branż, integracja pionowa, marka, R&D, kredyty.
5. **Magnat:** giełda, przejęcia, wpływ na miasto, media.

Nie ma sztucznych blokad — tylko kapitał, ludzie, informacja i czas.

### 13.3 Cele i scenariusze

Tryb otwarty (bez końca) + scenariusze: „Zmonopolizuj paliwo w 10 lat", „Uratuj upadającą hutę", „Zbuduj sieć 50 sklepów", „Wprowadź własną markę samochodów", „Zostań największym pracodawcą". Osiągnięcia z kronik (emergentne: „Twoja firma przetrwała 3 recesje").

### 13.4 Porażka

Bankructwo osobiste nie kończy gry — gracz wraca do bycia pracownikiem z długiem i reputacją. Śmierć postaci → dziedzic (dziecko/małżonek) przejmuje.

---

## 14. Interfejs użytkownika i inspekcja świata

### 14.1 Zasada

Każdy obiekt symulacji ma **kartę inspekcji**: stan, historia, powody ostatnich decyzji („dlaczego Anna nie kupiła u mnie?" → „nie zna Twojego sklepu" / „cena 12% wyższa niż w Dobrym Koszyku" / „brak parkingu"). To narzędzie nauki gry i jednocześnie główny kanał feedbacku dla projektanta.

### 14.2 Warstwy widoku

Widok 3D voxelowy + nakładki danych (mapy cieplne): wartość gruntu, dochód GD, zasięg sklepów, ruch, ceny produktu X per sklep, bezrobocie, zdrowie, zanieczyszczenie, znajomość marki gracza, przepływ towaru Y (animowane strumienie). Filtrowanie: pokaż tylko klientów mojego sklepu, tylko pracowników mojej fabryki, tylko ciężarówki z paliwem.

### 14.3 Panele biznesowe

- **Pulpit firmy:** przepływy pieniężne, alerty (niedobór, strajk, awaria), KPI.
- **Zakład:** produkcja, magazyn, załoga, maszyny, dostawy (Gantt), koszty.
- **Sklep:** półki (asortyment, ceny, rotacja), klienci (skąd, kto, dlaczego), konkurencja w zasięgu.
- **Łańcuch dostaw:** graf dostawców/odbiorców gracza z przepływami, kontrakty, ryzyka (jeden dostawca = czerwone).
- **Rynek:** dla produktu — wszystkie oferty w mieście, historia cen, wolumeny, udziały.
- **Ludzie:** lista pracowników, kandydatów, menedżerów; drzewo organizacyjne.
- **Finanse:** księgi, kredyty, giełda, wycena.
- **Miasto:** polityka, budżet, przetargi, wybory, media.
- **Kronika:** dziennik zdarzeń świata i gracza (w stylu DF Legends), przeszukiwalny.

### 14.4 Śledzenie mieszkańca / pojazdu / partii

Tryb „śledź": kamera podąża, na dole oś czasu dnia z planem i realizacją, panel potrzeb, budżet, ostatnie decyzje z uzasadnieniem. Śledzenie partii towaru: od pola do półki, z czasem i kosztem na każdym etapie.

### 14.5 Czas

Pauza, 1×, 3×, 10×, 50× (tryb makro — ruch przełącza się w mezo, grafika upraszcza). Kalendarz z sezonami. Możliwość ustawienia „zatrzymaj przy zdarzeniu X".

### 14.6 Automatyzacja

Polityki i reguły (cenowe, zapasów, HR) edytowane w prostym języku reguł z UI (bez pisania kodu), by gracz z 200 sklepami nie tonął w mikrozarządzaniu. Menedżerowie realizują polityki z jakością zależną od ich umiejętności.

### 14.7 Ekrany poza rozgrywką

**Zasada: gra uruchamia się bez argumentów wiersza poleceń.** Wszystko, co dziś ustawiają przełączniki `magnat --seed --size --region --epoch --profile --difficulty`, gracz ustawia na ekranie. Wiersz poleceń zostaje narzędziem deweloperskim i CI (§16.5) — nie jedyną drogą do nowego świata.

Powłoka gry to stany aplikacji, nie „okienka nad grą": każdy ekran jest wariantem stanu sesji, więc nie da się być jednocześnie w menu i w rozgrywce.

- **Menu główne.** Kontynuuj (ostatni zapis), Nowa gra, Wczytaj, Ustawienia, Wyjdź. Widoczna wersja gry i wersja formatu zapisu — zgłoszenie błędu zaczyna się od tych dwóch liczb.
- **Nowa gra — kreator świata.** Komplet parametrów generacji (§4.1) w jednym ekranie: ziarno (wpisane lub losowane), rozmiar mapy (4/8/12/16 km), region, epoka startowa, profil gospodarczy, trudność, scenariusz (§13.3) i wariant startu (§13.1). Każdy parametr niesie zdanie o tym, **co zmienia w grze**, a nie samą nazwę. Ziarno jest jawne i przepisywalne: dwóch graczy z tym samym ziarnem i tymi samymi parametrami dostaje ten sam świat co do metra (§18.2).
- **Generacja i podgląd.** Po zatwierdzeniu parametrów generacja idzie w tle, z nazwanymi etapami (maska lądu, wysokości, erozja, hydrologia, klimat, złoża, drogi, strefy, zabudowa, firmy, populacja) i możliwością anulowania — świat 16 km to kilkanaście sekund i ekran nie ma prawa udawać, że zawiesił się na zawsze. Po generacji: **podgląd mapy z kluczowymi liczbami** (populacja, powierzchnia miasta, dominujące branże, złoża) i decyzja gracza — zaczynam albo losuję ponownie. Odrzucenie świata przed rozpoczęciem gry jest tańsze niż odkrycie po godzinie, że miasto nie ma portu.
- **Wybór postaci** (§13.1): kandydaci z wygenerowanej populacji, z domem, rodziną, pracą i oszczędnościami; losowanie i przewijanie do skutku.
- **Wczytaj i zapisz.** Lista slotów z metadanymi: nazwa miasta, data w grze, majątek, godziny rozgrywki, wersja zapisu, ziarno. Autozapis rotacyjny. Zapis niezgodny wersją jest widoczny i opisany, nie ukryty.
- **Ustawienia** (wspólne dla menu i pauzy): język interfejsu PL/EN, skala UI, grafika (rozdzielczość, tryb okna, synchronizacja pionowa, zasięg widzenia, jakość cieni), dźwięk (§15.5), sterowanie z podglądem skrótów, zapis dziennika widoku do zgłoszeń błędów (włączalny). Zmiana języka i skali działa natychmiast, bez restartu.
- **Menu pauzy:** wróć do gry, zapisz, wczytaj, ustawienia, wyjdź do menu głównego. Wyjście z niezapisanym postępem zawsze pyta.
- **Ekrany domknięcia:** koniec scenariusza z rozliczeniem celów (§13.3) i ekran spuścizny po śmierci postaci bez dziedzica — z kroniką dynastii (§13.4). Oba prowadzą z powrotem do gry albo do menu; żaden nie jest ślepym zaułkiem.

Wymagania wspólne dla wszystkich ekranów: pełna obsługa z klawiatury, teksty wyłącznie z katalogów lokalizacji (PL i EN równolegle), układ wytrzymujący napisy dłuższe o 40% i skalę UI od 0,75 do 3,0, oraz ten sam system widgetów co w rozgrywce (§16.4) — powłoka nie jest osobnym interfejsem.

---

## 15. Prezentacja: voxele, kamera, dźwięk

### 15.1 Styl

Voxele o rozdzielczości 1 m (teren, budynki), 0,25 m dla detali (pojazdy, ludzie, wyposażenie, szyldy). Paleta ograniczona per dzielnica/epoka dla spójności. Cel: czytelność z góry, urok w zbliżeniu (mieszkańcy z ubraniem odpowiadającym statusowi i zawodowi, samochody klas i marek, szyldy z nazwami firm gracza).

### 15.2 Kamera

Swobodna izometryczno-perspektywiczna (obrót, pochylenie, zoom od orbity nad miastem do poziomu ulicy), przełączenie na widok pierwszoosobowy postaci gracza (spacer po własnym sklepie). Cięcie budynków poziomami (podgląd wnętrz: regały, linie, biura).

### 15.3 Oświetlenie i pogoda

Cykl dobowy, cienie, oświetlenie wnętrz i ulic nocą (zasilane z sieci — blackout gasi miasto), pory roku (śnieg, liście), deszcz, mgła, dym z kominów (proporcjonalny do emisji).

### 15.4 Animacja

Mieszkańcy chodzą, wsiadają, pracują (animacje wg zawodu), noszą zakupy; pojazdy jadą, parkują, tankują; wózki widłowe, dźwigi na budowach; towar na rampach. Poziomy detalu: w oddali — impostory/instancje, z bliska — pełne modele.

### 15.5 Dźwięk

Ambient dzielnicy (przemysł, ruch, park), dźwięki zakładów gracza (linia stoi = cisza), muzyka adaptacyjna do stanu finansów.

---

## 16. Technologia: własny silnik

### 16.1 Wybór języka i fundamentów

**Decyzja: Rust.**

Uzasadnienie w kontekście wymagań:

| Wymaganie | Dlaczego Rust |
|---|---|
| 100–400 tys. agentów, tysiące firm, dziesiątki tysięcy pojazdów w jednym ticku | Kontrola układu pamięci (SoA, arena), brak garbage collectora → brak pauz, przewidywalna wydajność; ownership wymusza projekt data-oriented |
| Wielowątkowość symulacji (systemy równoległe, job system) | Kompilator wyklucza wyścigi danych; `Send/Sync` czyni schedulera ECS bezpiecznym bez runtime'owych blokad |
| Determinizm | Brak ukrytej alokacji/GC, kontrola nad float (można wymusić fixed-point tam, gdzie trzeba), deterministyczne kolekcje (bez losowej kolejności iteracji hash-map — użyjemy własnych map z seedem) |
| Własny renderer voxelowy multiplatformowy | `wgpu` daje Vulkan/DX12/Metal jednym API bez pisania trzech backendów; shadery w WGSL |
| Długi cykl życia projektu, refaktory, solo/mały zespół | Typy i borrow checker to sieć bezpieczeństwa przy refaktorach dużych systemów; cargo jako jednolity build/test/bench |
| Modding | Bezpieczne osadzenie Lua (mlua) lub WASM (wasmtime) z izolacją |
| Zapis/serializacja | `serde` + własny format binarny; łatwe snapshoty stanu ECS |

**Alternatywy odrzucone:**
- *C++* — równa wydajność, ale brak bezpieczeństwa wątków i pamięci na poziomie kompilatora; w projekcie, w którym równoległość jest kluczowa, koszt debugowania wyścigów byłby dominujący.
- *C# (własny silnik bez Unity)* — GC przy setkach tysięcy obiektów wymusza walkę z alokacjami; możliwe, ale wbrew naturze języka.
- *TypeScript/WebGPU* — znany stos (por. dotychczasowe projekty gier), ale jednowątkowy model (Workers z kopiowaniem lub SharedArrayBuffer) i brak kontroli pamięci wykluczają skalę docelową.
- *Zig* — dobre dopasowanie, ale ekosystem (grafika, serializacja, narzędzia) zbyt młody na wieloletni projekt.

**Ustalenie granicy „własnego silnika":** piszemy silnik, nie sterowniki. Korzystamy z bibliotek warstwy systemowej i nie budujemy na cudzym silniku gry:

- `wgpu` (abstrakcja GPU), `winit` (okno, wejście), `naga` (shadery),
- `rayon` (work-stealing thread pool) — lub własny job system, jeśli potrzebna pełniejsza kontrola nad kolejnością (determinizm),
- `serde` + `bincode`/własny format (serializacja), `zstd` (kompresja zapisów),
- `glam` (matematyka), `parking_lot`, `crossbeam` (kanały),
- `egui` — rdzeń UI gry **i** narzędzi deweloperskich (korekta po M3, decyzja 9.2; pierwotnie było tu „tylko dla narzędzi deweloperskich, UI gry własne" — uzasadnienie zmiany w §16.4),
- `mlua` lub `wasmtime` (modding), `kira` lub `cpal` (audio),
- `tracy-client`/`puffin` (profilowanie).

Wszystko powyżej tej warstwy — ECS, scheduler, renderer voxelowy, symulacja, UI, format danych, narzędzia — jest własne.

### 16.2 Moduły silnika (crate'y workspace)

```
magnat/
├── engine/
│   ├── core        — typy bazowe, id, czas, RNG (xoshiro, strumienie per system), fixed-point
│   ├── ecs         — archetypowy ECS (SoA), scheduler systemów z grafem zależności, komendy
│   ├── jobs        — job system, deterministyczne fork-join, work stealing
│   ├── spatial     — siatka chunków, quadtree parcel, indeksy przestrzenne agentów/pojazdów
│   ├── nav         — grafy transportu (drogi, tory, piesze), CH/A* hierarchiczny, cache tras
│   ├── voxel       — chunki 32³, paleta, greedy meshing, LOD, edycja, streaming
│   ├── render      — wgpu: pipeline voxelowy, instancing, cienie kaskadowe, oświetlenie dobowe, 
│   │                 nakładki danych, post-processing, impostory
│   ├── ui          — retained-mode UI gry: layout, widgety, wykresy, tabele, style, i18n (PL/EN)
│   ├── audio       — miksowanie, ambient przestrzenny
│   ├── io          — zapis/odczyt, snapshoty ECS, dziennik zdarzeń, kompresja, wersjonowanie
│   ├── script      — host modów (Lua/WASM), API sandboxowane
│   └── devtools    — inspektor ECS, profiler, replay, edytor danych, konsola
├── sim/
│   ├── world       — generator (teren, hydrologia, strefy, drogi, budynki, historia na sucho)
│   ├── agents      — mieszkańcy, GD, potrzeby, planer dnia, decyzje, relacje, pamięć
│   ├── economy     — oferty, transakcje, rynki, ceny, banki, giełda, księgowość
│   ├── firms       — firmy, zakłady, produkcja, HR, marketing, R&D, AI firm
│   ├── supply      — towary, partie, receptury, magazyny, zlecenia transportowe
│   ├── traffic     — mikro/mezo ruch, komunikacja miejska, parkingi, sieci przesyłowe
│   ├── city        — władza, podatki, usługi publiczne, prawo, wybory
│   ├── events      — generator zdarzeń, pogoda, sezony, epoka
│   └── macro       — uproszczony model makro (historia na sucho, „co jeśli" dla AI, tryb 50×)
├── data/           — katalogi: towary, receptury, zawody, budynki, gramatyki, nazwy, epoki (RON/TOML)
├── game/           — pętla gry, stany, sesja, kariera, scenariusze, integracja sim+engine
└── tools/          — generator podglądu miasta, balansator ekonomii (headless), edytor danych
```

### 16.3 Renderer voxelowy

- Świat w chunkach 32×32×32 z paletą (≤256 materiałów/chunk) → 1 B/voxel + RLE dla pustych.
- Greedy meshing na CPU (jobs) do buforów GPU; chunki odległe: LOD 2×/4×/8× przez agregację; horyzont: impostory dzielnic.
- Encje dynamiczne (ludzie, pojazdy): instancing z per-instance transform + wariant modelu + paleta (kolor ubrań/auta z danych symulacji — auto należy do konkretnego GD i ma swój kolor).
- Oświetlenie: kierunkowe słońce z cieniami kaskadowymi + światła punktowe (latarnie, okna, reflektory) przez clustered shading; ambient occlusion voxelowe (obliczane per wierzchołek podczas meshingu); dzień/noc.
- Nakładki danych: tekstury pól skalarnych renderowane na terenie (mapy cieplne) + linie/strumienie przepływów jako geometria instancjonowana.
- Cel wydajnościowy: 60 FPS na GPU klasy średniej (2024) przy widoku całej dzielnicy, 30 FPS przy widoku całego miasta.

### 16.4 UI gry

Własny system retained-mode z deklaratywnym opisem (drzewo widgetów budowane z danych, dirty-flagging). Wymagane widgety: tabele z sortowaniem/filtrami na 100k wierszy (wirtualizacja), wykresy czasowe, grafy (łańcuch dostaw), Gantt, mapy cieplne w miniaturze, edytor reguł. Skalowanie DPI, lokalizacja PL/EN z pluralizacją.

**Korekta po M3 (decyzja 9.2, wiążąca):** rdzeniem UI gry jest **`egui` + `egui-wgpu`**, a nie własny toolkit — §16.1 dopuszczał `egui` tylko w narzędziach deweloperskich i to ograniczenie zostaje zniesione. Powód jest policzalny: M9 ma zbudować kilkanaście paneli, nie framework, a pisanie własnego układu, atlasu fontów i obsługi wejścia to największa pojedyncza masa kodu w projekcie bez planu zapasowego. Wymagania powyżej nie znikają — retained-mode, dirty-flagging po wersji danych, wirtualizacja i budżet klatki obowiązują dalej, tylko realizuje je warstwa modeli widgetów w `engine/ui` nad `egui`, który pozostaje czystym procesorem bez `wgpu` i `winit`. Konsekwencja wykorzystywana od M3: panel da się narysować w teście CI bez karty graficznej.

**Język wizualny** (paleta, typografia, siatka, komponenty, układ ekranów, dostępność) jest kontraktem osobnym od tego rozdziału i mieszka w `docs/ui-design.md`. Rozdział mówi, **co** interfejs musi umieć; tamten dokument — **jak** ma wyglądać i dlaczego tak.

### 16.5 Narzędzia deweloperskie

Krytyczne dla projektu tej skali — budowane równolegle z grą:
- **Headless runner:** symulacja bez grafiki, 1000× szybciej, do balansowania i testów regresji (porównanie hashy stanu między wersjami — determinizm jako test).
- **Inspektor ECS** i wykresy dowolnej metryki w czasie.
- **Replay:** zapis wejść gracza + seed → odtworzenie; kluczowe przy zgłoszeniach błędów.
- **Balansator:** uruchamia N miast z różnymi seedami i raportuje: rozkład cen, bezrobocie, bankructwa, stabilność — wykrywa spirale (deflacja, hiperinflacja, wymieranie populacji).
- **Edytor danych** (towary, receptury, gramatyki budynków) z walidacją grafu (każdy produkt ma źródło).

---

## 17. Architektura symulacji i skalowanie

### 17.1 Czas

- **Tick ekonomiczny:** 1 minuta czasu gry. Wszystkie decyzje agentów, transakcje, produkcja, księgowość — na tym ticku lub grubszym (godzina, dzień, miesiąc — systemy deklarują częstotliwość).
- **Tick ruchu:** 100 ms czasu gry w obszarze mikro; mezo — na ticku minutowym.
- **Tick renderu:** niezależny; interpolacja stanów między tickami symulacji.
- Symulacja w osobnym wątku (grupie wątków) od renderu; komunikacja przez podwójnie buforowany snapshot „widocznych" danych.

### 17.2 ECS i scheduler

- Archetypowy ECS, komponenty SoA, encje: mieszkaniec, GD, firma, zakład, budynek, parcela, pojazd, partia towaru, oferta, kontrakt, linia komunikacji, zdarzenie.
- Systemy deklarują odczyt/zapis komponentów → scheduler buduje DAG i uruchamia niezależne systemy równolegle; wewnątrz systemów — równoległość po chunkach encji (job system).
- Mutacje strukturalne (tworzenie/usuwanie encji) przez bufory komend aplikowane w ustalonych punktach synchronizacji → determinizm.

### 17.3 Symulacja sterowana zdarzeniami (DES) dla agentów

Agenci nie są aktualizowani co tick. Planer dnia zamienia plan na **zdarzenia w kolejce priorytetowej czasu** („o 07:20 Anna wychodzi z domu"). System obsługuje tylko zdarzenia zapadające w bieżącym ticku. 200 tys. agentów × ~20 zdarzeń/dobę = 4 mln zdarzeń/dobę = ~2800/minutę gry — trywialne. Podróże między zdarzeniami to zadania dla systemu ruchu (mezo: tylko czas przybycia; mikro: pojazd na drodze).

Przeplanowanie (replanning) wyzwalane zdarzeniami: korek, brak towaru, wezwanie do szkoły, zmiana pogody.

### 17.4 LOD symulacji (spójność wyników)

Trzy poziomy, ze ścisłą zasadą: **wynik ekonomiczny nie zależy od poziomu** (tylko wizualny).

| Poziom | Kiedy | Agenci | Ruch | Produkcja |
|---|---|---|---|---|
| Mikro | w kadrze / śledzone | pełne pozycje, animacje | car-following | per maszyna |
| Mezo | poza kadrem, ≤10× | zdarzenia DES, pozycja jako krawędź grafu | kolejki na krawędziach | per linia |
| Makro | 50×, historia na sucho, „co jeśli" AI | agregaty per dzielnica × klasa, z zachowaniem tożsamości (tylko stan uśpiony) | czasy z modelu statystycznego kalibrowanego z mezo | per zakład |

Gwarancja spójności: mezo i mikro używają tego samego modelu kosztu przejazdu (mikro jest tylko wizualizacją rozkładu, którego średnia równa się mezo). Przejście makro→mezo odtwarza indywidualne stany z zachowanych tożsamości (deterministycznie z seedu + agregatu).

### 17.5 Rynki — wydajność

Dopasowanie kupujących do ofert bez O(n·m): indeks przestrzenny ofert per kategoria (grid dzielnic), agent rozważa tylko oferty w zasięgu i znane (§5.7) → typowo 3–15 kandydatów. Ceny agregowane per dzielnica cache'owane per tick.

### 17.6 Pathfinding

Contraction Hierarchies na grafie dróg (przebudowa przy zmianach sieci — rzadkie, w tle), A* z heurystyką dla grafu pieszego, tabele czasów przejazdu między dzielnicami per godzina (aktualizowane z obserwacji) dla planowania mezo. Cache tras dom↔praca (stabilne miesiącami).

### 17.7 Pamięć

Cel: metropolia 400 tys. mieszkańców ≤ 6 GB RAM. Mieszkaniec ≈ 400 B stanu gorącego + pamięć doświadczeń w osobnym, kompresowanym magazynie (ostatnie 32 wpisy). Historia świata (kroniki) na dysku, indeksowana.

### 17.8 Testy

- Testy jednostkowe systemów; testy własnościowe (zasada zachowania: suma pieniądza w systemie = emisja − zniszczenie; suma masy towarów).
- Testy determinizmu: dwa przebiegi = identyczny hash stanu co 1000 ticków.
- Testy regresji balansu (balansator) w CI.
- Benchmarki (criterion) dla systemów gorących.

---

## 18. Zapis stanu, determinizm, modding

### 18.1 Zapis

Snapshot pełnego stanu ECS (kompresja zstd, ~50–300 MB dla dużego miasta) + dziennik zdarzeń od snapshotu (dla szybkich autozapisów i replay). Wersjonowanie schematu z migracjami. Zapis w tle bez zatrzymania symulacji (copy-on-write chunków ECS).

### 18.2 Determinizm

Wymagania: brak zależności od czasu rzeczywistego i kolejności wątków; RNG ze strumieniami per system i per encja (hash(seed, id, tick)); operacje zmiennoprzecinkowe w ustalonej kolejności lub fixed-point w księgowości (pieniądze zawsze jako i64 w groszach). Replay: seed + wejścia gracza = ten sam świat. Otwiera: multiplayer lockstep (kilku graczy w jednym mieście), dzielenie się „miastami" i wyzwaniami, odtwarzanie błędów.

### 18.3 Modding

Warstwa danych (towary, receptury, budynki, nazwy, epoki, zdarzenia) w plikach tekstowych — moddowalna bez kodu. Warstwa logiki: API skryptowe (Lua/WASM) z hookami (nowe zdarzenia, polityki AI, nowe typy zakładów, UI-panele). Sandbox: brak dostępu do IO poza katalogiem moda. Warsztat/menedżer modów w grze.

---

## 19. Plan implementacji (kamienie milowe)

Kolejność zaprojektowana tak, by po każdym milestonie istniał działający, testowalny artefakt. Bez dat — zakresy.

**M0 — Fundament silnika.** Workspace, `core`, `ecs`, `jobs`, headless runner, testy determinizmu na pustym świecie. Okno z `wgpu` renderujące jeden chunk voxeli.

**M1 — Świat statyczny.** Generator terenu + hydrologii, chunki, streaming, kamera, oświetlenie dobowe. Widoczny „krajobraz" z seedu.

**M2 — Miasto statyczne.** Sieć dróg, strefy, parcele, gramatyka budynków, dzielnice. Miasto do oglądania, nakładka wartości gruntu.

**M3 — Ludzie i dzień.** Generacja populacji i GD, potrzeby, planer dnia, DES, pieszy ruch, inspekcja mieszkańca z osią czasu. Mieszkańcy chodzą do pracy i sklepu (sklepy jako proste „nieskończone" źródła — tymczasowo).

**M4 — Ruch.** Graf dróg, CH, pojazdy, mikro/mezo, parkingi, komunikacja miejska, paliwo w baku (stacje jeszcze nieskończone). Korki widoczne i mierzone.

**M5 — Gospodarka detaliczna.** Oferty, użyteczność zakupu, sklepy z magazynem, ceny AI, pieniądz, budżety GD, księgowość sklepu. Pierwsza pętla gracza: otwórz sklep, ustal ceny, obserwuj klientów. Balansator uruchomiony.

**M6 — Łańcuch dostaw.** Towary z partiami, receptury, zakłady produkcyjne, magazyny, zlecenia transportowe, rynek B2B (spot + kontrakty), import. Paliwo od złoża do baku. Sklepy przestają być nieskończone.

**M7 — Firmy AI i rynek pracy.** Osobowości firm, polityki, HR, pensje emergentne, rotacja, menedżerowie, powstawanie/bankructwo firm. Konkurencja reaguje na gracza.

**M8 — Miasto jako aktor.** Władza, podatki, usługi publiczne, sieci przesyłowe, prawo, wybory. Zdarzenia losowe i pogoda/sezony.

**M9 — Gracz: kariera.** Postać gracza, start jako pracownik, ścieżka, scenariusze, kroniki, porażka/dziedziczenie. Pełne UI paneli biznesowych, automatyzacja polityk.

**M10 — Głębia.** Marketing i marka z pamięci agentów, R&D i epoki, giełda, przejęcia, media, relacje międzyfirmowe, związki zawodowe, historia „na sucho".

**M11 — Prezentacja.** Detale voxelowe, animacje, wnętrza, pogoda wizualna, audio, LOD wizualne, wydajność do celów.

**M12 — Skala i jakość.** Metropolia 400 tys., profilowanie, pamięć, tryb 50×, zapis w tle, modding API, lokalizacja, testy długich sesji (100 lat gry bez degeneracji).

---

## 20. Metryki sukcesu i ryzyka projektowe

### 20.1 Metryki symulacji (mierzone balansatorem)

- Stabilność: brak spirali cenowych (inflacja roczna w przedziale −5…+15% w 95% seedów bez ingerencji).
- Realizm: rozkład czasu dojazdu, bezrobocia, marż i rotacji w zakresach referencyjnych dla epoki.
- Reaktywność: szok podaży (zamknięcie rafinerii) widoczny w cenach stacji w ciągu 2–7 dni gry, wygaszony w 2–8 tygodni.
- Wyjaśnialność: 100% decyzji agentów ma czytelne uzasadnienie w karcie inspekcji.

### 20.2 Metryki techniczne

60 FPS widok dzielnicy / 30 FPS widok miasta na GPU średniej klasy; 1 doba gry ≤ 3 s w trybie 50× dla miasta 150 tys.; determinizm 100% w CI; zapis < 5 s bez pauzy.

### 20.3 Metryki gracza

Czas do pierwszej sensownej decyzji (< 15 min), odsetek graczy rozumiejących „dlaczego przegrałem" (ankieta > 80%), długość sesji, liczba emergentnych historii dzielonych przez graczy.

### 20.4 Ryzyka (do świadomego zarządzania, nie do redukowania wizji)

- Spirale ekonomiczne w symulacji otwartej — mitygacja: balansator w CI od M5, testy 100-letnie.
- Nieczytelność złożoności — mitygacja: inspekcja i uzasadnienia jako wymóg każdego systemu od pierwszego dnia.
- Koszt własnego silnika — mitygacja: ścisła granica bibliotek (§16.1), devtools równolegle z grą, headless-first.
- Krzywa uczenia Rusta przy dotychczasowym stosie Python/TypeScript — mitygacja: M0–M2 jako okres nauki na prostych, dobrze zdefiniowanych modułach; Python pozostaje w narzędziach (analiza wyników balansatora, generowanie danych).

---

## 21. Glosariusz

- **Agent** — mieszkaniec jako jednostka decyzyjna.
- **GD** — gospodarstwo domowe, jednostka budżetowa.
- **Oferta** — konkretna propozycja sprzedaży (kto, co, gdzie, ile, za ile).
- **Partia** — śledzona jednostka towaru z pochodzeniem i właściwościami.
- **Receptura** — definicja procesu produkcji (wejścia → wyjścia).
- **DES** — symulacja sterowana zdarzeniami (discrete event simulation).
- **LOD symulacji** — poziom szczegółowości obliczeń (mikro/mezo/makro) o gwarantowanej spójności wyników.
- **Historia na sucho** — przyspieszona symulacja makro przed startem gry, tworząca „zużyty" świat.
- **Kronika** — dziennik zdarzeń świata, przeszukiwalny, źródło emergentnych narracji.
- **Polityka** — reguła automatyzująca decyzje (cenowe, zapasów, HR) dla gracza i AI.
- **Balansator** — narzędzie headless uruchamiające wiele miast i mierzące stabilność ekonomii.

---

*Otwarte pytania do kolejnej wersji:* nazwa produktu; docelowy zakres epok (jedna epoka startowa vs. przejście przez dekady); multiplayer w zakresie 1.0 czy po; zakres pierwszego prototypu (M5 jako „vertical slice" do pokazania).
