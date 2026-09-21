# M8d — Usługi publiczne i prawo

Podfaza 4 z 5 fazy **M8 — Miasto jako aktor** (`M8-miasto-jako-aktor.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M8a (finansowanie, podstawa opodatkowania), M8c (mechanizm hazardu — reużyty), M3, M7. |
| **Pakiety robocze** | WP7, WP8 |
| **Projekt techniczny** | §5.3 |
| **Wynik do pokazania** | Test T4 oraz scenariusz „firma ukrywa 30 % obrotu” kończący się kontrolą w medianie < 3 lat gry. |
| **Kryterium zamknięcia** | Kryteria WP7 i WP8; domiar trafia do budżetu bez naruszenia testu T1. |
| **Poprzednia / następna** | `M8c-zdarzenia.md` · `M8e-wladza-i-wybory.md` |

Placówki publiczne z obsadą rekrutowaną na zwykłym rynku pracy, urząd jako kolejka z emergentnym czasem oczekiwania, pięć urzędów kontrolnych, szara strefa i antymonopol.

---

## Pakiety robocze

### WP7 — Usługi publiczne i urzędy
**Zależy od:** WP1 (finansowanie), WP2 (koszty osobowe → PIT), M3 (mieszkańcy), M7 (rynek pracy).
Placówki jako encje z obsadą rekrutowaną **na zwykłym rynku pracy** (nie magiczny etat),
jakość z finansowania i obsady, zasięg obwodowy, skutki w M3/M5/M7. Urząd jako kolejka:
`Permit` czeka, czas oczekiwania jest emergentny.
**Kryterium ukończenia:** test T4 (skutki usług mierzalne) + test kolejki: podwojenie obsady
urzędu skraca medianę czasu wydania pozwolenia co najmniej o 40%.
**Rozmiar:** L

### WP8 — Prawo i egzekucja, szara strefa
**Zależy od:** WP2 (podstawa opodatkowania), WP4 (mechanizm hazardu — **reużyty**, nie drugi system).
Pięć urzędów, sprawy, dowody, kary i środki zaradcze. Szara strefa jako parametr firmy
(`UnreportedShareBps`) podnoszący hazard kontroli. Kontrola skarbowa domykająca zaległość
z odsetkami. Antymonopol z progiem udziału rynkowego i przymusowym podziałem.
**Kryterium ukończenia:** scenariusz „firma ukrywa 30% obrotu" kończy się kontrolą w medianie
< 3 lat gry, a domiar trafia do budżetu bez naruszenia testu T1.
**Rozmiar:** M

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.3 Usługi publiczne i egzekucja

```rust
pub struct PublicService {
    pub kind: ServiceKind,           // School(Level) | Hospital | Clinic | Police | Fire
                                     // | WasteCollection | Park | Office(OfficeKind) | Transit
    pub site: SiteId,                // zwykły budynek na parceli (M2)
    pub capacity: u32,               // uczniów / łóżek / rewirów / ton na tydzień
    pub staff_target: u32,
    pub staff: Vec<CitizenId>,       // rekrutowani na ZWYKŁYM rynku pracy (M7)
    pub funding_per_month: Money,
    pub condition: Q,                // stan budynku, degraduje bez konserwacji
    pub quality: Q,                  // emergentna, aktualizowana EveryMonth
    pub utilization_bps: u32,        // przeciążenie obniża jakość
    pub catchment: DistrictId,
    pub open_days: WeekMask,         // K-15: urząd i szkoła mają weekend, szpital i policja nie
}

/// EveryMonth. quality = f(funding/obsługiwaną osobę, obsada/etaty, condition, przeciążenie)
fn sys_update_service_quality(services: &mut [PublicService], budget: &CityBudget);

/// EveryMonth. Publikuje odczyt dla innych faz: zanik z odległością po czasach przejazdu (M4).
fn sys_publish_service_coverage(services: &[PublicService], out: &mut ServiceCoverage);

pub struct ServiceCoverage {                 // per dzielnica, czytane przez M3/M5/M7
    pub education: Vec<Q>, pub health: Vec<Q>, pub safety: Vec<Q>,
    pub fire_response: Vec<Q>, pub sanitation: Vec<Q>, pub leisure: Vec<Q>,
}
```

Kanały skutków (emitowane jako parametry, nie jako liczby narzucone innym fazom):

| Usługa | Parametr wyjściowy | Kto konsumuje |
|---|---|---|
| Szkoła | `SkillGrowthMulBps{district, age_band}` | M3 (rozwój dzieci) |
| Szpital / przychodnia | `RecoveryRateMulBps`, `AbsenteeismBps{district}` | M3, M7 (produktywność) |
| Policja | `ShrinkageBps{district}` → straty w sklepach | M5/M7 (koszt w RZiS!) |
| Straż | `FireHazardMulBps{district}` → mnożnik hazardu zdarzenia pożaru | M8 (`sim/events`) |
| Odpady | `PollutionDelta{district}` przy zaległościach w wywozie | M8, M5 (wartość gruntu) |
| Parki | `LeisureSatisfaction`, `LandValueMulBps` | M3, M5 |
| Urząd | czas wydania pozwolenia (emergentny) | gracz, M7 |

```rust
pub struct Agency {
    pub kind: AgencyKind,            // Antitrust | LaborInspection | Sanitary | Environment | TaxOffice
    pub budget: Money, pub inspectors: u32,
    pub open_cases: Vec<CaseId>,
}

pub struct Case {
    pub id: CaseId, pub subject: FirmId, pub agency: AgencyKind,
    pub opened_at: Tick, pub evidence: Q,
    pub finding: Option<Finding>, pub remedy: Option<Remedy>,
}

pub enum Remedy {
    Fine(Money),
    Closure { until: Tick },                         // sanepid zamyka restaurację
    LicenseRevoked(LicenseClass),
    ForcedDivestiture { share_bps: u32 },            // antymonopol
    BackTax { amount: Money, interest: Money },      // skarbówka → nowy TaxCharge
    Injunction(Policy),                              // nakaz dostosowania (np. filtr)
}
```

**Szara strefa** to parametr firmy `UnreportedShareBps` (ustawiany przez gracza albo przez AI
firmy z M7). Obniża podstawę VAT/CIT/PIT i **podnosi hazard kontroli skarbowej** przez sondę
`DeclaredVsExpectedGapBps`. Kontrola używa **tego samego mechanizmu hazardu co zdarzenia**
(WP4) — nie budujemy drugiego losowania.


---

## Zmiany wpisane po M8a

Zgodnie z `K-18`.

| # | Zmiana | Dlaczego |
|---|---|---|
| | **`Remedy::BackTax` ma gotowe wejście i nie tworzy „nowego `TaxCharge`" ręcznie.** Domiar to należność jak każda inna: `magnat_city::ChargeRegistry::accrue(TaxPayer::Site(site), kind, okres, podstawa, masa, stawka_bp, kwota, teraz, termin)`. Odsetki liczy `late_interest(kwota, bp_rocznie, doby)` na kalendarzu 360-dniowym, a rejestr sam pilnuje, że domiar wchodzi do domknięcia `Σ Assessed = Σ Settled + Σ Overdue + Σ Abated` | Przypadek (2) z `K-18`: API, którego podfaza używa w przykładzie, istnieje i nazywa się inaczej |
| | **Umorzenie po nieskutecznej egzekucji też ma wejście: `magnat_city::abate_bankrupt(city, payer, tick)`.** Zamyka wszystkie otwarte należności płatnika powodem `AbateReason::Bankruptcy` i utrzymuje domknięcie. Odpowiada przy okazji na pytanie, które §5.3 zostawiało otwarte: **nikt nie zdejmuje z budżetu należności firmy, która upadła** — przesuwa ją do czwartego stanu, żeby różnica między „zapłacono" a „odpisano" była widoczna w raporcie | To jest ta sama reguła, którą `K-10` ustala dla postępowania: M8 jest wierzycielem, nie organem egzekucyjnym. Miasto zamyka swoją stronę księgi i nic poza tym |
| | **Podstawy VAT/CIT/PIT obniżanej przez `UnreportedShareBps` nie ma gdzie wpiąć „u siebie".** Wszystkie trzy naliczają się w `sim/city::assess` z faktów, które wystawia gospodarka: VAT z kolejki `Market::take_tax_accrued`, PIT z licznika `Withholding`, CIT z `Market::closed_result`. Szara strefa musi więc obniżyć **fakt**, a nie naliczenie — czyli zmniejszyć to, co zakład zgłasza, zanim danina powstanie. Inaczej powstałyby dwie prawdy o obrocie: jedna w księdze zakładu i druga w rejestrze miasta | Przypadek (5) z `K-18`. Obniżanie kwoty **po** naliczeniu dałoby rejestr, którego nie da się uzgodnić z księgą płatnika — a to jest dokładnie ta para liczb, którą test T1 porównuje |

## Zmiany wpisane po decyzji właściciela produktu (2026-09-17)

Zgodnie z `K-18`. Gwiazdka = zmiana zakresu albo kryterium. Wymaganie: **każdy obiekt
w świecie jest klikalny i ma kartę inspekcji z zakładkami, a każda nazwa w karcie jest
odnośnikiem**. Konsekwencja dla tej podfazy jest jedna i mała, ale nie było jej nigdzie.

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **Usługa publiczna jest klikalna jako `Site`, a nie przez nowy wariant `Subject`** — `PublicService.site` jest `SiteId` (§5.3), więc szkoła otwiera się tą samą kartą co sklep, z układem zakładek Obsada · Zasięg · Finansowanie · Jakość. Podfaza dostarcza dane pod te cztery zakładki | Gracz, który klika w budynek szkoły, oczekuje karty — i dostanie ją za darmo, o ile `PublicService` da się znaleźć po `SiteId`. To jest wymaganie wsteczne do tej podfazy: potrzebny jest indeks `SiteId → PublicService`, nie tylko lista usług |
| | **`Case` wchodzi do `Subject` jako `Subject::Case(CaseId)`** (`M9c` §5.7). Karta sprawy: czego dotyczy, kto ją prowadzi, na jakim etapie, jaki środek (`Remedy`) grozi — z odnośnikiem do firmy i do agencji | Sprawa urzędowa przeciwko firmie gracza jest rzeczą, o którą gracz zapyta „dlaczego" natychmiast. `Case` ma `id: CaseId` i `subject: FirmId` (§5.x), więc odnośnik w obie strony jest już w danych — brakowało tylko wariantu |

---

## Zmiany wpisane po M8d

Co podfaza pokazała o **własnym** planie. Gwiazdka = zmiana zakresu albo kryterium.
Poprawki wysłane w przód (do M8e, M9 i wykazu `R2`) są w tabeli „Zmiany wpisane po M8d"
w dokumencie fazy.

| # | Zmiana | Dlaczego |
|---|---|---|
| `CG-1` ★ | **`ServiceKind`, `AgencyKind`, `RemedyKind`, `PermitKind` i `ServiceCoverage` idą do `engine/core`, nie do `sim/city`** (`K-64`). `CF-2` przewidywało to warunkowo („jeśli okaże się, że czyta go więcej niż jedna faza") — okazało się, i to nie przy słowniku, tylko przy **strukturze**. Pokrycie usług czytają `sim/agents` (tempo nauki, długość choroby) i `sim/economy` (ubytki w sklepie), a oba crate'y stoją **pod** `sim/city` w grafie zależności | Warunek z `CF-2` spełniony w pierwszym pakiecie. Tablica trzymana u piszącego byłaby dla obu czytelników niewidoczna, a duplikat rozjechałby się przy pierwszej zmianie |
| `CG-2` ★ | **`PublicService.staff` jest liczbą, nie `Vec<CitizenId>`.** §5.3 zapisywało wektor identyfikatorów; obsada liczy się z komponentów `Employment` raz w miesiącu i nie jest przechowywana | Wektor byłby **drugą prawdą o zatrudnieniu** obok `Employment.site`, a rozjazd między nimi objawiłby się jako szkoła z nauczycielami, którzy pracują gdzie indziej. To jest ta sama klasa błędu co dwie numeracje `SiteId` w `K-46` |
| `CG-3` ★ | **Kanałów skutków jest trzy, nie siedem.** Powstają: szkoła → tempo nauki dziecka, opieka zdrowotna → długość zwolnienia, policja → ubytki inwentaryzacyjne sklepu. **Nie powstają** `FireHazardMulBps` (straż), `PollutionDelta` (odpady) i `LandValueMulBps` (parki) | `R2` zastosowane przed napisaniem kodu, a nie po. Wartość gruntu **nie zmienia się w trakcie gry**: jedyne dwa zapisy `Parcel.land_value_per_m2` w całym repozytorium to `value::pass_1` i `pass_2`, obie wołane raz przy generacji. Mnożnik hazardu pożaru wymagałby zmiany cudzej definicji zdarzenia i drugiego czytelnika `ServiceCoverage` po stronie `sim/events`. Oba mają adres w tabeli poprawek w przód |
| `CG-4` ★ | **`ServiceKind::Waste` nie ma ani jednego archetypu w `data/buildings/`.** Wariant zostaje w słowniku (PRD §10.3 wymienia odpady, a `SpendCategory::Waste` istnieje od M8a), ale w mieście nie stoi ani jedna placówka tego rodzaju i pokrycie wywozu odpadów jest zerowe w każdej dzielnicy | Zapisane wprost, bo rodzaj usługi bez placówki wygląda w raporcie tak samo jak rodzaj, którego nikt nie sfinansował. Raport scenariusza wypisuje „brak placówek w tym mieście" osobnym wierszem właśnie po to |
| `CG-5` ★ | **`Remedy::Injunction(Policy)` nie powstaje.** `Policy` jest typem M8e; wariant, którego nic nie ustawia, przechodzi każdy test | `R2` zastosowane do wariantu enuma — ta sama decyzja, którą `CD-6` podjęło wobec `EdgeState::UnderMaintenance` |
| `CG-6` ★ | **Inspekcja pracy nie stoi na pokryciu etatowym, tylko na płacy poniżej widełek roli.** Pierwsza wersja progu („obsada poniżej 55 % etatów") otworzyła w przebiegu 216 spraw na 217 zakładów | Próg mierzył **znaną fikcję**: po M7f w mieście stoi kilkanaście tysięcy nieobsadzonych etatów, więc „zakład pracujący obsadą, której nie ma" to w tym świecie każdy zakład. To jest ta sama klasa błędu co stopa bezrobocia 6 ‰ z pozycji 6 wykazu `R2` — i tak samo nie jest kwestią kalibracji progu. Widełki roli z `data/jobs/roles.ron` są za to prawdziwe |
| `CG-7` | **`Case.subject` to `SiteId`, nie `FirmId`.** §5.3 zapisywało `subject: FirmId`; płatnikiem daniny jest zakład (`CA-4`), a domiar musi trafić w to samo miejsce co reszta należności | Sprawa przeciwko firmie, której domiar ląduje na innym kluczu niż jej księga, nie domknęłaby się w T1. `Case` niesie obok `firm: FirmId` — do odnośnika w karcie, nie do księgowania |
| `CG-8` ★ | **Urząd prowadzi naraz tyle spraw, ilu ma inspektorów.** Bez tego limitu sanepid otwierał sprawę przeciwko co drugiemu sklepowi w mieście, dowody dzieliły się na dwieście spraw i **żadna nigdy się nie kończyła** | Pomiar, nie przegląd: 191 spraw otwartych i zero zamkniętych po 120 dobach gry. Mechanizm wyglądał na działający i nie robił nic — `R2` w najtrudniejszej do zauważenia postaci, bo licznik spraw rósł |
| `CG-9` | **Kartoteka sanitarna zeruje się razem z zamknięciem sprawy.** Masa odpisana z powodu terminu jest licznikiem narastającym od otwarcia zakładu, więc sklep, który raz przekroczył próg, przekraczałby go już zawsze | Pomiar: 109 zawieszeń na 60 sklepów w 200 dobach, czyli kara zamieniła się w stan, a sanepid w podatek obrotowy płacony czasem |
| `CG-10` ★ | **Szara strefa obniża VAT i CIT, nie PIT.** Ukryta część utargu nie wchodzi do kolejki VAT-u i nie staje się przychodem w księdze — idzie na kapitał właściciela, więc podstawa CIT-u spada razem z nią. Wynagrodzenia „pod stołem" nie powstają | `PayrollOutbox::take()` nie ma konsumenta od M7b (pozycja przenoszona pięć razy), więc wypłaty nie ruszają pieniądza i nie ma czego ukryć. Ukrywanie PIT-u wymaga najpierw domknięcia tamtego odcinka |
| `CG-11` ★ | **Strata musi być dotkliwa, a nie tylko ujemna** (`shadow.distress_bp`). Pierwsza wersja reguły wpychała w szarą strefę każdy zakład, który zamknął miesiąc choćby złotówkę pod kreską | Pomiar po 200 dobach: 70 zakładów na 70, średni udział 31 % — czyli dokładnie to, przed czym broni ryzyko `R10` („albo wszyscy oszukują, albo nikt"). Próg jest w `data/tuning/city.ron`, bo to kalibracja, i stroi go balansator M8e |
| `CG-12` ★ | **Antymonopol wykonuje przymusowy podział zamknięciem zakładu.** `Remedy::ForcedDivestiture` jest prawdziwy w skutku (udział firmy spada), ale zakład znika zamiast zmienić właściciela | `ponytail:` sufit nazwany w kodzie — sprzedaż wymaga rynku kontroli nad firmą, czyli giełdy i przejęć z M10. Skutek dla udziału rynkowego jest ten sam, koszt dla gospodarki większy |
| `CG-13` ★ | **Zasięg usługi jest dwustopniowy, nie liczony po czasie przejazdu.** §5.3 zapowiadało „zanik z odległością po czasach przejazdu (M4)"; `sim/city` nie widzi `TravelOracle` — ten mieszka po stronie mostu | Decyzja `D6` fazy dopuszczała granulację per dzielnica z zanikiem po czasie między centroidami. Macierzy odległości między dzielnicami nie ma po żadnej stronie granicy, a zbudowanie jej w moście jest pakietem samym w sobie. Dziś: własna dzielnica bierze najlepszą placówkę, reszta miasta ułamek z `spillover_bp` |
| `CG-14` | **Kolejka pozwoleń jest w M8d, mimo że `Permit` opisuje §5.2 dokumentu M8e.** Podział §5 poszedł za tematem, podział pakietów za zależnościami — i w tym jednym miejscu się rozjechały. WP7 ma kolejkę w kryterium ukończenia, więc kolejka jest tutaj; uchwały, przetargi i wybory zostają w M8e | Przypadek (5) z `K-18` w wersji łagodnej: pakiet obiecuje coś, czego właścicielem jest inny dokument. Numeracja `5.x` nie drgnęła, więc odesłania działają |
| `CG-15` | **Pozwolenie nie warunkuje otwarcia zakładu.** Wniosek składa się, czeka w kolejce, kosztuje opłatę i kończy decyzją — ale zakład, który go złożył, działa od pierwszej doby | Warunkowanie wymaga, żeby stawianie zakładu przechodziło przez miasto, a przechodzi przez M7 (powstawanie firm) i M10 (rynek nieruchomości). `ponytail:` sufit nazwany w kodzie mostu |
| `CG-16` ★ | **Sonda `FirmUnreportedBps` nie liczyła się per firma i przez dzień nie działała.** `ProbeCache` pomija klucz zakresu dla sond, których nie ma na liście `scope_dependent` — a nowa sonda tam nie trafiła, więc cache oddawał wszystkim firmom wartość **pierwszej ocenianej**. Kontrola skarbowa nie zachodziła ani razu, a diagnostyka pokazywała hazard 90 ppm zamiast 2 700 | Znalezione **pomiarem**, nie przeglądem: raport scenariusza wypisuje dla tej jednej definicji „ilu kandydatów, ile odsianych, jaki hazard", więc „90 ppm przy 59 zakładach w szarej strefie" rzuca się w oczy. Lista została **odwrócona**: wymienia teraz sondy **niezależne** od zakresu (pogoda, kalendarz, wskaźniki miasta — zbiór zamknięty), więc sonda dopisana i pominięta kosztuje wydajność, a nie poprawność |
| `CG-17` | **Bramka kontroli skarbowej stoi na wieku firmy, nie na obsadzie.** `MinFirmHeadcount(1)` odsiewał dwie trzecie firm miasta | Ta sama przyczyna co przy `CG-6`: obsada mierzy dziś znaną fikcję. Miesiąc od założenia znaczy za to dokładnie to, co ma znaczyć — firma bez domkniętego miesiąca nie ma czego ukryć |
| `CG-18` ★ | **Test T4 mierzy się na przebiegach krótszych niż dziesięcioletnie i mierzy tempo, nie sufit.** Kryterium mówi „10 lat gry"; tempo nauki z `data/tuning/city.ron` daje przy pełnym pokryciu ~13 punktów `Q` na rok, więc próg 8 punktów pęka w drugim roku, a dziesięcioletni przebieg mierzyłby wyłącznie sufit skali (100) po obu stronach | Ta sama korekta co `BF-3` w M7f: kryterium zapisane jako długość przebiegu, a mierzalne jako tempo. Przebiegi: szkoła 720 dób, absencja 540, domiar 90 dób pełnej gospodarki |
| `CG-19` | **`Agency.budget` jest polem bez pisarza.** Urzędy finansuje `SpendCategory::Administration` bez rozbicia per urząd, więc budżet urzędu kontrolnego nie ma skąd wziąć kwoty | Zapisane, żeby M8e nie założyło, że rozbicie już jest. Liczba inspektorów bierze się dziś z obsady urzędów miejskich i to ona jest realnym ograniczeniem |
| `CG-20` ★ | **`cooldown_days` jest przerwą definicji, a nie instancji — i dla zdarzenia o zakresie `Firm` znaczy to coś zupełnie innego, niż wygląda.** `Events::in_cooldown` patrzy na `last_end_day` **definicji**, więc 240 dób przerwy przy kontroli skarbowej znaczyło „całe miasto ma spokój przez dwie trzecie roku, bo jedna firma dostała kontrolę". Pomiar: 3 kontrole na 200 dób gry przy hazardzie 1 404 ppm i 213 kandydatach, czyli dwudziesta część tego, co mówił hazard. Po poprawce `cooldown_days: 0`, `max_concurrent: 12`, a powtórce u tej samej firmy zapobiega `Events::busy(def, scope)` | Pomiar, nie przegląd — i to samo miejsce, w którym raport pokazuje hazard obok liczby wystąpień. Wniosek jest szerszy niż ta jedna definicja i dotyczy każdej następnej o zakresie `Firm` albo `Site`: przerwa per definicja ma sens dla zdarzenia miejskiego (susza, embargo), a dla zdarzenia per podmiot jest globalnym licznikiem udającym lokalny |
| `CG-21` | **Recenzja przed commitem znalazła czternaście rzeczy i wszystkie weszły.** Najważniejsze cztery: (1) sprawa urzędowa **nie sprawdzała, czy jej podstawa nadal istnieje** — nagłówek modułu obiecywał „sprawa, która straciła podstawę, umarza się sama", a zakład, który przestał ukrywać obrót, i tak dostawał karę, bo dowody rosną same; (2) odsetki od domiaru liczyły się **od początku świata**, więc po roku gry każdy domiar dostawał ten sam, przycięty do sufitu nalicz; (3) mianownik udziału rynkowego w antymonopolu sumował utarg **od początku świata i razem z zakładami zamkniętymi**, więc próg stawał się z wiekiem świata nieosiągalny — teraz jest to okno dwunastu domkniętych miesięcy; (4) placówka liczyła jako obsługiwanych **wyłącznie mieszkańców swojej dzielnicy**, więc dziewięć dzielnic bez szpitala nie było obsługiwanych przez nikogo, a obłożenie wychodziło dziesięciokrotnie za niskie | Reszta to: `CaseId(0)`/`PermitId(0)` panikujące na odejmowaniu przed rzutowaniem, wniosek `Expired` blokujący złożenie kolejnego na zawsze, `applicant` i `office` poza hashem pozwolenia, brak walidacji `spillover_bp ≤ 10 000` w loaderze, przepełnienia `u32` przy obłożeniu, dzielenie przez zero w domiarze chronione wyłącznie sufitem szarej strefy, sufit `.max(1)` dający inspektorów miastu bez administracji, flaga ucznia stawiana pracującemu nastolatkowi oraz cztery martwe byty: `Remedy::LicenseRevoked`, `Agency.budget`, `Market::is_suspended` i `SiteEnforcementRow.district` |
