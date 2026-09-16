# M7b — Rynek pracy

Podfaza 2 z 6 fazy **M7 — Firmy AI i rynek pracy** (`M7-firmy-ai-i-rynek-pracy.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M7a (stanowiska), M5 (maszyneria ofert). |
| **Pakiety robocze** | WP4, WP5, WP6 |
| **Projekt techniczny** | §5.5 |
| **Wynik do pokazania** | Pensje emergentne: niedobór roli podnosi ofertę bez żadnej tabeli płac w kodzie. |
| **Kryterium zamknięcia** | Kryteria WP4–WP6. |
| **Poprzednia / następna** | `M7a-firma-jako-dane.md` · `M7c-polityki-i-menedzerowie.md` |

Rynek pracy w `sim/economy`: oferty, aplikacje i wybór kandydata, licytacja płac z indeksem niedoboru i headhuntingiem, HR ze szkoleniami, premiami i rotacją.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| [x] WP4 | Rynek pracy w `sim/economy`: oferty, aplikacje, wybór kandydata | WP3, M5 (maszyneria ofert) | L |
| [x] WP5 | Licytacja płac, indeks niedoboru, headhunting | WP4 | M |
| [x] WP6 | HR: szkolenia, premie, benefity, zwolnienia, rotacja | WP3, WP4 | M |

### WP4 — Rynek pracy: oferty, aplikacje, wybór

**Lokalizacja: `sim/economy::labor`** — rozszerzenie crate'a M5, nie nowy crate (D2).
Rynek pracy jest rynkiem i korzysta z **tej samej** maszynerii ofert, tego samego indeksu
przestrzennego i tego samego mechanizmu dopasowania co rynek towarowy. Drugi, równoległy
mechanizm dopasowania jest jawnie zakazany. M7 jest autorem tego rozszerzenia, M5 właścicielem
crate'a — zmiany idą przez M5.

Firma publikuje `JobOffer` jako `Offer` w kategorii pracy. Mieszkaniec (hook w `sim/agents`)
składa `Application`. Firma wybiera wg scoringu. Indeks ofert per (`JobRoleId`, `DistrictId`)
zgodnie z §17.5 — kandydat rozważa 3–15 ofert, nie wszystkie.

*Kryterium ukończenia:* w scenariuszu z 5 tys. mieszkańców i 300 firmami bezrobocie zbiega do
pasma 3–9% w 90 dni gry; każde zatrudnienie ma `DecisionReason::Hire` z wynikiem kandydata
i wynikiem drugiego w kolejce.

**Spełnione, oba zdania osobno.** Pierwsze: `sim/economy/tests/labor.rs`
(`bezrobocie_zbiega_do_pasma_w_dziewiecdziesiat_dni`) — 5 tys. mieszkańców, 300 firm,
4 650 etatów; po 90 dobach bezrobocie ląduje w paśmie 30–90 ‰. To samo kryterium
na prawdziwym mieście: `tools/headless/tests/labor_city.rs`
(`rynek_pracy_obsadza_wakaty_i_podnosi_stawki`) — miasto 4 km, 18,5 tys. siły roboczej,
68 ‰ po 90 dobach. Drugie: `kazde_zatrudnienie_ma_wynik_i_drugiego_w_kolejce` —
powód nazywa się `Hired` (nie `Hire`), niesie wynik zatrudnionego i wynik drugiego,
a `i32::MIN` znaczy „nie było drugiego", a nie „drugi był fatalny".

### WP5 — Licytacja płac i niedobór

`LaborMarketStats` — indeks niedoboru per (`JobRoleId`, `DistrictId`), liczony dziennie z wakatów,
aplikacji i czasu wiszenia oferty. Eskalacja stawki przy braku kandydatów, headhunting (oferta
bezpośrednia do zatrudnionego) przy wysokim niedoborze, degresja stawek przy nadmiarze (nowe
oferty poniżej mediany, malejąca płaca progowa bezrobotnego).
**Nigdzie nie ma tabeli płac** — mediana w UI to agregat ofert i zaakceptowanych umów.

*Kryterium ukończenia:* test `wage_reacts_to_shortage` (§7) zielony w CI; brak spirali płacowej
w 5-letnim przebiegu (górne ograniczenie: firma nie licytuje powyżej płacy, przy której marża
zakładu spada poniżej progu z polityki).

**Spełnione** (`sim/economy/tests/labor.rs`): `wage_reacts_to_shortage` odtwarza scenariusz
z §7.1 co do liczb — trzy zakłady po osiem etatów spawacza, dwudziestu spawaczy, szesnastu
już zatrudnionych — i sprawdza wszystkie cztery asercje tamtego paragrafu: mediana
zaakceptowanej płacy spawacza rośnie o 30 % w 60 dób, zawód kontrolny drga o mniej niż 3 %,
wstrzyknięcie trzydziestu spawaczy obniża niedobór, a **żadnemu pracownikowi nie obcięto
stawki**. `brak_spirali_placowej_w_pieciu_latach` przechodzi 1800 dób i sprawdza sufit.
Sufitem marży jest w M7b górny kraniec widełek stanowiska — patrz `AU-4`.

### WP6 — HR

Szkolenia (koszt + czas + przyrost umiejętności z sufitem od talentu), premie (funkcja wyniku
zakładu i polityki), benefity (auto służbowe, opieka medyczna — realne zaspokojenie potrzeb
z §5.3, nie liczba dodawana do nastroju), zwolnienia z odprawą i kosztem reputacyjnym, rotacja
dobrowolna i przymusowa.

*Kryterium ukończenia:* benefit „opieka medyczna" mierzalnie podnosi poziom potrzeby *Zdrowie*
u pracownika w symulacji (nie tylko nastrój); odejście pracownika zawsze ma `DecisionReason`
po stronie odchodzącego (lepsza oferta / nastrój / stres / zwolnienie).

**Spełnione** (`sim/economy/tests/labor.rs`): `swiadczenie_zdrowotne_podnosi_potrzebe_pracownika`
sprawdza obie połowy zdania — potrzeba *Zdrowie* rośnie, a nastrój nie drga.
`odejscie_zawsze_ma_powod_po_stronie_odchodzacego` wymusza trzy przyczyny **stanem
człowieka**, a nie wpisaniem powodu z ręki: zniechęcenie, przeciążenie i zwolnienie
za wynik — przy czym to ostatnie wynika z realnej produktywności, bo ocena schodzi
poniżej progu i po trzech miesięcznych upomnieniach firma wypowiada umowę. Czwartą
przyczynę (lepsza oferta) sprawdza `zmiana_pracy_przechodzi_przez_zwolnienie_etatu`,
bo tam jest jej właściwe miejsce: zmiana pracy **musi** przejść przez zwolnienie etatu,
inaczej etat znika z miasta na zawsze.

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.5 Rynek pracy jako rynek ofert (§6.6) — w `sim/economy::labor`

**Podział własności (D2):** typy i logika poniżej żyją w `sim/economy` (właściciel M5),
bo rynek pracy jest rynkiem i musi używać tej samej maszynerii ofert, tego samego indeksu
przestrzennego i tego samego dopasowania co rynek towarowy. M7 jest **autorem** tego rozszerzenia,
nie właścicielem crate'a. W `sim/firms` zostaje wyłącznie to, co jest decyzją *firmy*:
scoring kandydata, reguły eskalacji stawki, polityka headhuntingu — czyli `labor_policy.rs`.
Zakaz budowania drugiego, równoległego mechanizmu dopasowania jest wiążący.

```rust
pub struct JobOffer {
    pub id: OfferId,
    pub firm: FirmId, pub site: SiteId, pub role: JobRoleId,
    pub district: DistrictId,
    pub wage_month: Money,             // to jest licytowana zmienna
    pub slots: u16,
    pub shift: ShiftId,
    pub requirements: SkillReq,        // min. umiejętność, wykształcenie, doświadczenie
    pub benefits: BenefitSet,
    pub targeted: Option<CitizenId>,   // Some => headhunting, oferta bezpośrednia
    pub posted: SimMinute,
    pub expires: SimMinute,
    pub raises: u8,                    // ile razy podbito — do DecisionReason i do limitu
}

pub struct Application {
    pub offer: OfferId,
    pub citizen: CitizenId,
    pub wage_expectation: Money,       // płaca progowa kandydata (§5.6)
    pub skill: Q,
    pub referral: Option<CitizenId>,   // z grafu relacji §5.1
    pub submitted: SimMinute,
}
```

**Wybór kandydata** — scoring, nie argmax po jednej cesze:

```rust
pub fn score_application(a: &Application, o: &JobOffer, p: &FirmPolicy,
                         refs: &ReferralStrength, mem: &FirmMemory) -> i32;
// skill_fit   — dopasowanie do profilu; KARA za przekwalifikowanie (odejdzie szybko)
// wage_fit    — oczekiwanie <= oferta; zbyt niskie = sygnał ryzyka, nie okazja
// referral    — waga relacji z obecnym pracownikiem (§5.1)
// history     — pamięć firmy o kandydacie (był zwolniony? odszedł po miesiącu?)
// modyfikatory osobowości: quality_focus podnosi wagę skill_fit, price_focus — wage_fit
```

**Licytacja płac — rdzeń emergentności.** Nie ma tabeli płac. Są trzy sprzężenia:

1. **Eskalacja przy niedoborze.** Oferta wisi `d` dni bez akceptowalnego kandydata →
   `wage_month += step`, gdzie `step = base_step * (1 + aggression/100) * (1 + shortage_index)`,
   ograniczone twardo przez `wage_ceiling` z polityki: płaca, przy której marża zakładu spada
   poniżej `min_margin_bp`. To ograniczenie jest **jedynym** hamulcem spirali i musi być liczone
   z własnych kosztów firmy — czyli z danych prywatnych, zgodnie z `FirmView`.
2. **Headhunting.** Przy `shortage_index > próg` firma wysyła `JobOffer { targeted: Some(..) }`
   do pracownika konkurenta ze stawką `obecna * (1 + premia)`. Pracownik ocenia ją zwykłą regułą
   z §5.6 (użyteczność > próg; ambicja obniża próg, lojalność podwyższa).
3. **Degresja przy nadmiarze.** Płaca progowa bezrobotnego spada z czasem bezrobocia (funkcja po
   stronie `sim/agents`), więc oferty poniżej mediany znajdują kandydatów. Firma widzi to
   w `LaborMarketStats` i przestaje licytować.

```rust
pub struct LaborMarketStats {          // liczone EveryDay, dane PUBLICZNE
    // BTreeMap, nie HashMap (dokument 00 §3.2)
    pub per_role: BTreeMap<(JobRoleId, DistrictId), RoleStats>,
}
pub struct RoleStats {
    pub vacancies: u32,
    pub applicants_30d: u32,
    pub median_wage_posted: Money,
    pub median_wage_accepted: Money,
    pub median_days_to_fill: u16,
    pub shortage_index: u16,           // 0..=1000 z wakatów, aplikacji i czasu wakatu
}
```

`LaborMarketStats` widzą i gracz, i AI — to zamierzone: **płace w ofertach są jawne, koszty
jednostkowe nie są**.

---

## Zmiany wpisane po M7b

Korekty naniesione w trakcie wykonania tej podfazy. Gwiazdka = zmiana zakresu
albo kryterium.

| # | Co | Dlaczego |
|---|---|---|
| `AU-1`* | **Most M7a nadawał zakładowi identyfikator z innej przestrzeni niż reszta gospodarki** — patrz `K-46`. `Site.id` szło z numeracji generatora (od zera), a `Employment.site`, sklepy M5 i zakłady M6 używają klucza przesuniętego o `SITE_KEY_BASE`. Poprawione w `tools/headless/src/firms.rs`; numeracja generatora została tylko tam, gdzie jest potrzebna — do wyliczenia powierzchni z lokali | Bez tego rynek pracy w pierwszej dobie **zwolniłby całą załogę miasta**: krok domknięcia po M3 sprawdza, czy pracownik nadal wskazuje ten zakład, a wskazywał liczbę większą o 16 777 216. Objaw byłby przy tym mylący — wyglądałby na błąd rotacji, a nie na błąd identyfikatora. Pilnuje tego `labor_city.rs::zaklad_i_jego_zaloga_mowia_o_sobie_tym_samym_identyfikatorem` |
| `AU-2`* | **Indeks ofert jest po `(JobRoleId, DistrictId)`, a nie przestrzenny.** §5.5 mówi „ten sam indeks przestrzenny", a WP4 w tym samym dokumencie — „indeks ofert per (`JobRoleId`, `DistrictId`) zgodnie z §17.5". Rozstrzygnięte na korzyść drugiego zapisu | To on odpowiada PRD §17.5 i tak brzmi pytanie kandydata: **szukam pracy w swoim zawodzie, niedaleko**. Siatka przestrzenna odpowiada na „co jest w promieniu 800 m" — pytanie kupującego chleb, a nie szukającego pracy, który pojedzie przez pół miasta, jeśli oferta jest w jego zawodzie. Zakaz „drugiego mechanizmu dopasowania" zostaje w mocy i jest spełniony: ta sama arena z generacjami, ta sama kolejność kroków, ta sama funkcja oceny |
| `AU-3` | **`JobOffer` jest własnym typem, a nie `Offer` z innym `price_basis`.** Pole `id: OfferId` z §5.5 nie powstaje — uchwyt nadaje arena | Ten sam rozstrzygnięty spór co przy ofercie B2B (`K-36`) i z tych samych trzech powodów: kształt (z dziewięciu pól wspólne jest jedno), indeks (`OfferIndex` warstwuje się po `StockCat`, a zawód nie ma kategorii spiżarni, w której mógłby stanąć) i koszt (osiem pól razy 10⁵ ofert detalicznych, płacone przez M5 za wygodę M7) |
| `AU-4`* | **Sufitem licytacji jest górny kraniec widełek stanowiska**, a nie płaca, przy której marża zakładu spada poniżej progu. `wage_ceiling(band, margin_headroom_bp)` ma marżę jako **hak zerowy** | Marży nie ma z czego policzyć: `SitePnlMonth` po M7a niesie koszt pracy i koszt stały, a **nie ma przychodu** (`AR-7`) — pierwszym jego pisarzem jest M7e. Widełki są przy tym sufitem prawdziwym, a nie zastępczym: są już przeliczone przez epokę i zamożność dzielnicy, czyli mówią, ile ta praca w tym miejscu może kosztować. Spiralę zatrzymują twardo i test pięcioletni to sprawdza. Adres domknięcia: **M7e** |
| `AU-5` | **`score_application` bierze `CandidateFacts` i `OpeningFacts`, a nie `&Application` i `&JobOffer`** | Sygnatura z §5.5 nie mogła istnieć: `Application` i `JobOffer` żyją w `sim/economy`, a zależność idzie `economy → firms` i odwrócić się nie da — byłby cykl, którego Cargo nie zbuduje. Wstrzyknięcie działa więc w drugą stronę, dokładnie tak, jak zapowiada `D19`. Przypadek (2) z `K-18`. Z tego samego powodu `Application` **nie ma pola `referral`** ani pamięci firmy o kandydacie: graf relacji jest własnością M10, a pole bez pisarza jest kosztem razy liczba aplikacji (`AR-6`) |
| `AU-6` | **Rynek pracy wchodzi do świata przez uchwyt `LaborHandle(Option<LaborMarket>)`**, a nie jako zwykły zasób | Krok doby dotyka naraz rejestru firm i komponentów mieszkańców, a dwóch pożyczek `&mut World` nie ma. Rynek trzeba więc na czas kroku **wyjąć** ze świata — ten sam wzorzec, którym `sim/agents` wyjmuje `AgentSources`, i ten sam powód |
| `AU-7`* | **Oferta publiczna na nadal otwarty wakat jest odnawiana, a nie wystawiana od nowa.** `offer_days_valid` znaczy „okres odnowienia", a nie „czas życia"; oferta bezpośrednia nadal ginie po dobie | Znalazł to przebieg, nie przegląd: przy wygasaniu co 14 dób licytacja **wracała na dolny kraniec widełek** i dwa tygodnie podbijania stawki przepadały. Mediana ofert rysowała piłę, a mediana zawartych umów rosła o 9 % zamiast o 15 % z kryterium — czyli mechanizm z §5.5 pkt 1 był na miejscu i nie działał |
| `AU-8`* | **Ciśnienie na odejście liczy się w punktach bazowych na dobę, a nie w promilach** (`QuitPressure::per_10k`) | §7.10 stawia rotację roczną w paśmie 8–25 %, czyli ryzyko dobowe rzędu 2–8 bp. W promilach **najmniejszą wyrażalną wartością jest 30 % rocznie** — sama jednostka wypychała rotację ponad górny kraniec pasma, zanim ktokolwiek cokolwiek skalibrował. Zmierzone w mieście 4 km: 8 961 odejść na 90 dób przy 14 tys. zatrudnionych i bezrobocie rosnące z 5,5 % do 23,3 % |
| `AU-9`* | **Headhunting jest drugim ruchem, a nie pierwszym**: wchodzi dopiero wtedy, gdy firma podniosła już stawkę w zwykłym ogłoszeniu, i najwyżej raz na firmę na dobę | Bez limitu firma z pięcioma wakatami rozsyła pięć propozycji dziennie i przeciąganie pracownika przestaje być decyzją, a staje się tłem: 15 278 ofert bezpośrednich na 90 dób, czyli jedna na każdego zatrudnionego w mieście. Ryzyko `R12` („reakcja jako prześladowanie") w wersji kadrowej |
| `AU-10` | **Kandydat, który nigdy nie pracował, kotwiczy poniżej mediany zawodu** (`reservation_newcomer_bp`), a nie powyżej niej jak bezrobotny z ostatnią płacą | Bez tego rozróżnienia trzydziestu spawaczy wstrzykniętych migracją **nie mogło podjąć żadnej pracy**: ich oczekiwanie startowało na 102 % mediany, a nowe oferty stoją z definicji poniżej mediany. Niedobór po migracji **rósł** zamiast spadać, wbrew asercji 3 z §7.1. To jest przy tym druga połowa degresji z §5.5 pkt 3 — pierwszą jest spadek oczekiwań z czasem bezrobocia |
| `AU-11` | **Świadczenie pozapłacowe jest tym, co firma daje, gdy skończyły się pieniądze**: oferta zatrzymana na suficie marży dostaje kolejno posiłki, opiekę medyczną, szkolenia i auto | Kryterium WP6 wymaga, żeby świadczenie było realnym zaspokojeniem potrzeby, więc musiało mieć pisarza — a jedynym miejscem, w którym firma **decyduje** o świadczeniu przed M7c (polityki), jest właśnie chwila, w której stawki nie wolno jej już podnieść. Kolejność jest kolejnością kosztu |
| `AU-12` | **Suma malejąca aplikacji zaokrągla ubytek w górę** (`div_ceil`) | Przy dzieleniu w dół suma poniżej trzydziestu przestaje maleć w ogóle (`29 − 29/30 = 29`), więc zawód, w którym ostatnia aplikacja wpłynęła rok temu, na zawsze zostaje z resztką mówiącą „ktoś tu jednak chciał" — a indeks niedoboru czyta ją jako prawdę |
| `AU-13` | **`PlantSite::labor_pct` ma wreszcie pisarza prowadzącego go w czasie** (`economy.Labor`, raz na dobę), a `Site` dostaje `hr_accrued` — koszt kadrowy miesiąca doliczany do kosztu pracy przez listę płac | Domknięcie `AT-2` z M7a: obsada z Etapu 8 była zdjęciem, a nie liczbą prowadzoną. Premie, odprawy, świadczenia i szkolenia muszą trafić do rachunku wyniku zakładu, bo to on jest wejściem decyzji taktycznej M7e; **zaksięgowanie** ich razem z listą płac należy do M7d, która jest właścicielem pieniądza firmy |

| `AU-14`* | **Obsada liczy się do wyczerpania wakatu, a nie do wyczerpania planu.** Trzy osobne drogi prowadziły do zakładu z liczbą pracowników większą od liczby etatów: oferta publiczna i oferta bezpośrednia na **to samo** stanowisko, odjęcie od oferty etatów **zaplanowanych** zamiast obsadzonych (pętla ma przerwania) i „zatrudnienie" własnego pracownika na jego własne stanowisko po tym, jak eskalacja przebiła jego próg zmiany pracy. Dodatkowo liczba etatów w wiszącej ofercie jest teraz codziennie synchronizowana z wakatami stanowiska | Każda z trzech dawała ten sam skutek księgowy: **zakład płaci dwie pensje za jeden etat do końca gry**, bo `vacancies()` nasyca się na zerze i nadmiar nigdy się nie ujawnia. Trzecia dawała przy tym zatrudnienie widmo — `hires` i `quits` rosną o jeden, obsada bez zmian, a mediana zawartych umów łapie stawkę, która nikogo nie przyniosła. Pilnuje tego `obsada_nigdy_nie_przekracza_liczby_etatow`: 300 dób, po każdej sprawdzane, że nikt nie stoi na dwóch listach płac |
| `AU-15` | **Ewidencja szukających pracy jest domykana razem z obsadą**, a `odejdz` zwalnia **ten** etat, a nie „jakikolwiek" | Bezrobotny, który umarł albo wyjechał, nigdy nie stał na liście płac, więc pętla domykająca obsadę go nie dotykała — a mapa szukających wchodzi do hasha stanu i do zapisu gry, więc rosłaby przez sto lat bez granicy. Drugie: `release` czyści komponent mieszkańca **bez pytania o zakład**, więc wywołane dla kogoś, kto pracuje gdzie indziej, skasowałoby mu prawdziwą pracę. Dziś taki rozjazd nie powstaje (pilnuje go `K-46`), ale to jedyna droga wyjścia z etatu i nie wolno jej wierzyć wołającemu na słowo |

### Co zostaje otwarte po M7b

| # | Co | Adres |
|---|---|---|
| `AW-1` | **Lista płac nadal nikogo nie obciąża.** `PayrollOutbox` (M7a) i dołożone do niej `PayrollRun::hr_costs` nie mają konsumenta: dochód gospodarstwa nadal płynie z konta „reszta świata" przez `Household.income_monthly`, a rynek pracy tę liczbę wyłącznie **przesuwa** przy zatrudnieniu i odejściu. Domknięcie obiegu wymaga, żeby firmy miały z czego płacić, czyli przychodu i kredytu | **M7d** (finanse firmy), przy udziale M7e (przychód w `SitePnlMonth`) |
| `AW-2` | **W mieście 4 km stoi 23 912 etatów przy 18,5 tys. siły roboczej.** Rynek pracy odpowiada na to poprawnie — chroniczny niedobór podbija stawki do sufitu widełek w prawie każdym zawodzie — ale jest to odpowiedź na miasto, które ma o jedną trzecią za dużo miejsc pracy. Ta sama rozbieżność, którą M7a zapisała jako `AT-4`, zmierzona teraz od strony skutku | **M7f** (powstawanie i upadek firm), przy udziale M2 (gęstość obsady zabudowy) |
| `AW-3` | **Widełki płacowe mostu są jedną liczbą dla wszystkich ról spoza `Workplace` M2** (`DOMYSLNE_WIDELKI`, 2 800–5 200 zł). Skutek widać w przebiegu: mediana kilkunastu zawodów dochodzi do dokładnie tej samej kwoty, bo dochodzi do tego samego sufitu | **M7f** razem z `AW-2`; docelowo widełki per rola z `data/jobs/roles.ron` × epoka × dzielnica, tak samo jak dla ról, które M2 zna |
| `AW-4` | **Wymagania stanowiska są wyprowadzone z wag produktywności zawodu**, bo `data/jobs/roles.ron` nie ma pola „wymagana umiejętność". Przybliżenie jest jawne i ma `ponytail:` z nazwanym sufitem | **M10** (R&D i profile kompetencji stanowisk) |
| `AW-5` | **Nastrój i stres pracownika nie zmieniają się w scenariuszu `m7labor`**, bo nie biegnie w nim doba mieszkańca M3 — rotacja jest liczona ze stanu zamrożonego przy generacji. Pełne miasto ze wszystkim naraz stawia `m5shop`, a złożenie obu należy do domknięcia fazy | **M7f** (artefakt fazy z §1) |
| `AW-6` | **Zwolnienie kosztuje odprawę, ale nie kosztuje reputacji.** WP6 mówi „zwolnienia z odprawą i kosztem reputacyjnym"; odprawa jest, reputacji nie ma — firma nie ma dziś żadnej wielkości, w której koszt reputacyjny mógłby się odłożyć | Reputacja pracodawcy jest wielkością **jawną** (kandydat musi ją widzieć, zanim złoży aplikację), więc jej miejscem jest `LaborMarketStats` albo `FirmView`; jedno i drugie powstaje w M7e. Do tego czasu scoring kandydata nie ma czego o firmie czytać. Adres: **M7e**, razem z marką i pamięcią firmy |

## Zmiany wpisane po M3c

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M3c.

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **M7 przejmuje pulę etatów `sim::agents::migration::Vacancies`** razem z `Employment`. Dziś pula jest płaską listą `JobSlot { site, role, shift, work_days, district, wage_monthly }` wypełnianą przez Etap 8 z `Workplace` M2 i jest **jedynym** miejscem, w którym „miasto ma pracę" cokolwiek znaczy | Do M7 nie ma rynku pracy: etat się bierze i oddaje, nie negocjuje. Pula jest za to wejściem regulatora populacji z M3c §5.7 (napływ ∝ `min(wakaty, pustostany)`), więc **nie wolno jej po prostu usunąć** — M7 ma ją zastąpić czymś, co nadal odpowiada na pytanie „ile jest wolnych etatów i gdzie" |
| ★ | **Każde wyjście z rynku pracy musi przejść przez `migration::release_job_of`.** Zgon, emerytura i wyjazd z miasta już tędy idą; M7 dokłada zwolnienie i zmianę pracy | Etat, który nie wraca do puli, znika z miasta na zawsze. Zmierzone w M3c (korekta G-7): bez tego `min(wakaty, pustostany)` schodzi do zera, napływ wygasa i po stu latach z 5 257 mieszkańców zostaje 105. To jest domknięcie księgowe pojemności miasta, nie szczegół implementacji |
| ★ | **`DeprivationEffect::ProductivityLoss` i `AmbitionGain` czekają w `data/needs/needs.ron` na M7** — `needs::deprivation_of` już je wystawia, nikt ich nie stosuje (rozstrzygnięcie D-6 z M3a, potwierdzone w M3c dla `StatusLoss`) | Ten sam wzorzec, którym M3c wziął `StatusLoss`: skutek progowy stosuje faza, która jest jego właścicielem, a wartości siedzą w danych od M3a, żeby nikt ich nie wymyślał od nowa |
| | **`Employment.role` jest indeksem do `data/jobs/`, a prestiż zawodu czyta `social::CityFacts.job_prestige`** — tablica wypełniana z zewnątrz, dziś przez Etap 8 | `sim/agents` nie zna katalogu zawodów. Gdy `job_prestige` jest pusta, funkcja statusu przybliża prestiż kompetencją (bezrobotny 10, emeryt 40, uczeń 45, pracujący 40 + połowa poziomu w roli) — jawny `ponytail:` w `social::prestiz`, który znika, gdy tablica jest wypełniona |
