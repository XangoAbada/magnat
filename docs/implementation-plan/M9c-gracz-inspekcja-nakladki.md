# M9c — Gracz, inspekcja, nakładki

Podfaza 3 z 5 fazy **M9 — Gracz: kariera** (`M9-gracz-kariera.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M9b (rdzeń UI), M9a (komendy). |
| **Pakiety robocze** | WP4, WP5, WP7 |
| **Projekt techniczny** | §5.3, §5.7, §5.10 |
| **Wynik do pokazania** | Odpowiedź na „dlaczego Anna nie kupiła u mnie?” w PL i EN, z nazwanym konkurentem i klikalnym odnośnikiem. |
| **Kryterium zamknięcia** | Kryteria WP4, WP5 i WP7; 9 nakładek z PRD §14.2 działa. |
| **Poprzednia / następna** | `M9b-rdzen-ui.md` · `M9d-jezyk-regul.md` |

Postać gracza i warianty startu, karta inspekcji w pełnej postaci z wyczerpującym renderem `DecisionReason` i `LostSale`, nakładki danych z filtrami.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Opis | Kryterium ukończenia |
|---|---|---|---|---|
| ✅ **WP4** | Postać gracza i start | WP2 | `PlayerCharacter`, `PlayerAutonomy`, `StartVariant` jako filtr+łatka na populacji, ekran wyboru postaci, przypięcie do LOD Mikro | Da się wybrać mieszkańca, zobaczyć jego rodzinę/pracę/znajomych i przeżyć dzień; 5 wariantów startu działa |
| ✅ **WP5** | Karta inspekcji w pełnej postaci | WP3, WP4 | `InspectionCard` z nagłówkiem i `CardTab` (układ zakładek per typ encji — tabela w §5.7), wyczerpujący render `DecisionReason`, `LostSale` (bufor cykliczny + histogram dobowy), odnośnik przy **każdej** nazwie podmiotu, `InspectionNav` ze stosem wstecz/dalej | Odpowiedź na „dlaczego Anna nie kupiła u mnie?" w PL i EN, z nazwanym konkurentem i klikalnym odnośnikiem; **z karty mieszkanki da się dojść kliknięciami do jej domu, pracodawcy, męża i samochodu, i wrócić do niej przyciskiem wstecz**; test przechodzi każdy wariant `Subject` i sprawdza, że cel nieistniejący renderuje się bez odnośnika |
| ✅ **WP7** | Nakładki danych i filtry | WP3, WP6 | `OverlaySpec`, `EntityFilter`, legenda, przełącznik warstw, filtry „tylko moi klienci / moi pracownicy / cysterny z paliwem" | 9 nakładek z §14.2 działa (bez `BrandAwareness` — zarezerwowana dla M10) |

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.3 Postać gracza

Kluczowa decyzja projektowa: **gracz nie jest osobnym bytem**. Gracz to `CitizenId` — ten sam,
którego obsługują systemy M3 (potrzeby, planer dnia, relacje, pamięć, zdrowie, demografia).
`PlayerCharacter` to cienki komponent sterujący nad zwykłym mieszkańcem.

```rust
pub struct PlayerCharacter {
    pub citizen: CitizenId,              // gracz JEST mieszkańcem
    pub household: HouseholdId,
    pub owned_firms: Vec<FirmId>,
    pub personal_account: AccountId,
    pub reputation: Reputation,          // przeżywa bankructwo (§13.4)
    pub debts: Vec<DebtId>,
    pub heir: Option<CitizenId>,
    pub autonomy: PlayerAutonomy,
    pub start_variant: StartVariant,
    pub dynasty: DynastyId,              // łączy kolejne postacie jednej gry
}

/// Które decyzje mieszkańca przejmuje gracz, a które dalej robi autopilot M3.
pub struct PlayerAutonomy {
    pub job:      Control,   // zmiana pracy
    pub shopping: Control,   // zakupy GD
    pub housing:  Control,   // mieszkanie
    pub vehicle:  Control,   // samochód
    pub leisure:  Control,   // czas wolny — domyślnie Auto, inaczej gra jest nudna
    pub schedule: Control,   // godziny pracy w swoim sklepie (§13.2)
}
pub enum Control { Manual, Auto }
```

Konsekwencja, którą trzeba zapewnić: **dom, rodzina, praca i znajomi przychodzą za darmo**
z generatora M2/M3. Nie tworzymy syntetycznego mieszkańca. Wariant startu to *predykat wyboru
+ łatka*:

```rust
pub enum StartVariant {
    /// absolwent bez kapitału, pożyczka od rodziny
    Graduate       { family_loan: Money },
    /// doświadczony pracownik z oszczędnościami
    ExperiencedWorker { savings: Money },
    /// spadkobierca małej firmy
    Heir           { firm_kind: SiteKind, condition: FirmCondition },
    /// inwestor z zewnątrz: kapitał jest, sieci relacji brak
    OutsideInvestor{ capital: Money, wipe_relations: bool },
    /// sandbox: dowolny kapitał
    Sandbox        { capital: Money },
}

impl StartVariant {
    /// Predykat na wygenerowanej populacji — kto w ogóle może być tą postacią.
    pub fn candidate_filter(&self) -> ConditionExpr;
    /// Łatka na wybranego mieszkańca, stosowana jako komenda w ticku 0.
    pub fn patch(&self, pick: CitizenId) -> Vec<PlayerCommand>;
}
```

`OutsideInvestor { wipe_relations: true }` realizuje „kapitał, brak sieci" przez wyzerowanie
relacji — a nie przez sztuczny modyfikator. Skutek jest emergentny: nikt o graczu nie wie,
więc pierwsza rekrutacja i pierwsi klienci są trudni (§5.7 PRD).

**Przypięcie LOD:** postać gracza i cele trybu „śledź" mają `LodPin` — muszą zostać w LOD Mikro
także przy 10× i 50×. To kontrakt do M3/M4 (§6, „konsumuję").

### 5.7 Karta inspekcji i „dlaczego Anna nie kupiła u mnie?"

```rust
pub struct InspectionCard {
    pub subject: Subject,
    pub header:  Vec<Span>,          // tożsamość — zawsze widoczna, nigdy w zakładce
    pub tabs:    Vec<CardTab>,       // pusta zakładka jest pomijana, nie wyszarzana
}
pub struct CardTab { pub title: LocKey, pub sections: Vec<CardSection> }

/// `Subject` mieszka w `engine/core`, nie tutaj (`K-62`) — niesie go `DecisionReason`.
pub enum Subject { Citizen(CitizenId), Household(HouseholdId), Firm(FirmId), Site(SiteId),
                   Building(BuildingId), Parcel(ParcelId), Vehicle(VehicleId), Batch(BatchId),
                   Offer(OfferId), Contract(ContractId), District(DistrictId),
                   // encje miejskie — dostarcza M8c/M8d/M8e (§6 tamtych dokumentów)
                   Government, Tender(TenderId), Case(CaseId),
                   Event(EventId), Permit(PermitId) }
// Usługa publiczna NIE ma tu wariantu: `PublicService.site` jest `SiteId` (M8d §5.3),
// więc szkoła i przychodnia otwierają się jako `Site` z innym układem zakładek.
// Wybory też nie: `Election` jest jedna naraz i jest zakładką w karcie `Government`.
pub enum CardSection {
    State(Vec<(LocKey, Value)>),
    History(SeriesKey),
    Decisions(Vec<DecisionRecord>),
    Relations(Vec<(LocKey, Subject)>),
    Actions(Vec<PlayerCommandTemplate>),     // z karty da się działać, nie tylko patrzeć
}
pub struct DecisionRecord {
    pub at: SimMinute,
    pub reason: DecisionReason,
    pub alternatives: SmallVec<[Alternative; 3]>,   // co odrzucone i o ile przegrało
}
```

#### Zakładki, nie sekcje jedna pod drugą

Decyzja właściciela produktu z 2026-09-17: **tożsamość w nagłówku, reszta w zakładkach**.
`docs/ui-design.md` §4 zapisuje to jako komponent, tutaj jest konsekwencja dla kodu.
Zakładki są stałe per typ encji — gracz, który nauczył się, że „Dlaczego" jest ostatnie
u mieszkanki, ma je znaleźć na tym samym miejscu u firmy.

| Encja | Zakładki |
|---|---|
| Mieszkaniec | Stan (potrzeby, gotówka, zdrowie) · Dzień (plan vs realizacja) · Rodzina (gospodarstwo, krewni, znajomi) · Majątek (mieszkanie, pojazdy, konta) · Praca (zakład, stanowisko, płaca, historia) · Dlaczego (`DecisionReason`) |
| Gospodarstwo | Skład · Budżet · Majątek · Mieszkanie · Dlaczego |
| Budynek | Kondygnacje · **Lokatorzy** (gospodarstwa) i **najemcy** (zakłady) · Stan techniczny · Media · Własność |
| Pojazd | Stan (przebieg, zużycie, paliwo) · Właściciel · Użycie (trasy, kierowca) · Koszty |
| Firma | Pulpit · Właściciele · Zakłady · Pracownicy · Finanse · Kontrakty · Dlaczego |
| Zakład | Półki/Produkcja · Klienci · Załoga · Dostawy · Konkurencja · Dlaczego |
| Parcela | Teren · Zabudowa · Strefa i prawo · Wartość |
| Dzielnica | Ludzie · Gospodarka · Usługi · Wartości gruntu |

| Rada miasta (`Government`) | Skład · Polityka i podatki · Budżet · **Wybory** (kandydaci, sondaże, wynik) · Przetargi · Dlaczego |

Zakładka bez treści znika: pojazd nieużywany od roku nie ma „Użycia", a pusta zakładka jest
gorsza od jej braku, bo obiecuje treść. Reszta encji (`Batch`, `Offer`, `Contract`, `Tender`,
`Case`, `Event`, `Permit`) ma jedną zakładkę i zachowuje się jak dzisiejsza karta.

Szkoła, przychodnia i posterunek to `Site` — ta sama karta co sklep, inny układ zakładek
(Obsada · Zasięg · Finansowanie · Jakość zamiast Półki · Klienci · Konkurencja). Nie ma
powodu robić dla nich osobnego wariantu `Subject`, skoro `PublicService` i tak trzyma `SiteId`.

#### Każda nazwa jest odnośnikiem

Reguła jest prosta i nie ma wyjątków: **jeśli karta wymienia podmiot, który ma własną kartę,
to jest to odnośnik**. Adres mieszkania w karcie Anny prowadzi do budynku, nazwa pracodawcy
do zakładu, nazwisko męża do jego karty, konkurent z powodu decyzji do jego zakładu.
Technicznie niesie to `Span { link: Option<Subject> }` z `M9b` §5.8 — więc `CardSection`
przestaje być jedynym miejscem, gdzie link może się pojawić, a `Relations` zostaje dla
powiązań wymienianych **z nazwy** („żona", „właściciel", „dostawca").

```rust
pub struct InspectionNav {
    pub current: Subject,
    back:    ArrayDeque<Subject, 32>,   // najstarsze wypada
    forward: ArrayDeque<Subject, 32>,   // czyszczone przy nowym skoku
}
```
`ViewCommand` (`M9a` §5.5) dostaje `NavigateBack` i `NavigateForward`. Bez tego karta
z kilkunastoma odnośnikami jest pułapką: trzy skoki i gracz nie ma jak wrócić do pytania,
które zadawał. Stos jest stanem widoku — nie wchodzi do zapisu świata ani do hasha.

**Cel, który przestał istnieć** (firma upadła, mieszkaniec zmarł, partia została sprzedana),
renderuje się jako nazwa bez odnośnika plus jedno zdanie, co się stało — `Subject::resolve`
zwraca `Option`, a `None` nie jest błędem, tylko normalnym stanem świata, który się zmienia.
Odnośnik prowadzący w pustkę jest gorszy od jego braku.

Render: `fn render_reason(&DecisionReason, &Locale) -> Vec<Span>` — **jedno ramię `match` na
wariant**, bez `_ =>`. `DecisionReason` to jeden centralny enum w `engine/core` i **nie może być
`#[non_exhaustive]`** (doc 00, K-12), żeby kompilator łapał
brakujące ramię. To jest egzekucja doc 00 §7: faza, która dodaje wariant, dodaje też render
i klucz lokalizacyjny, inaczej `game/` się nie kompiluje.

#### Pytanie „dlaczego Anna nie kupiła u mnie?" — warianty odpowiedzi i źródła

| Wariant `DecisionReason` | Renderowana odpowiedź | Źródło danych |
|---|---|---|
| `NotInChoiceSet { cause: Unknown }` | „Anna nie zna Twojego sklepu — nie była, nikt jej nie polecił, nie ma go na jej trasie" | M3, §5.7 (zbiór wiedzy mieszkańca) |
| `NotInChoiceSet { cause: TooFar { travel_cost, threshold } }` | „Za daleko: 18 min dojazdu przy jej progu 12 min" | M4 (trasa), M3 (próg osobisty) |
| `NotInChoiceSet { cause: Closed { at } }` | „O 16:40 Twój sklep był zamknięty" | M9 (`SetOpeningHours`) + M5 |
| `NotInChoiceSet { cause: OutOfStock { good } }` | „Brak chleba na półce o 16:40" | M5/M6 (stan półki) |
| `NotInChoiceSet { cause: NoAssortment { good } }` | „Nie masz w asortymencie mleka 3,2%" | M9 (`SetShelfAssortment`) |
| `LostToCompetitor { winner, u_gap_bp, dominant: Price }` | „Wybrała »Dobry Koszyk« — Twoja cena o 12% wyższa (18,90 vs 16,80)" | M5 §6.4 (rozkład użyteczności) |
| `LostToCompetitor { dominant: Distance }` | „»Dobry Koszyk« jest po jej drodze z pracy — Twój to 9 min objazdu" | M4 + M3 (planer dnia) |
| `LostToCompetitor { dominant: Loyalty }` | „Kupuje tam od 3 lat; przewaga lojalności wyceniona na 4,10 zł" | M3 (pamięć, historia zakupów) |
| `LostToCompetitor { dominant: Quality }` | „Ocenia jakość ich pieczywa na 78 vs Twoje 54" | M5/M6 (jakość partii) |
| `LostToCompetitor { dominant: Brand }` | „Zna ich markę, Twojej nie" (rezerwacja) | **M10** — do czasu M10 wariant nieaktywny |
| `LostToCompetitor { dominant: Status }` | „Twój sklep jest poniżej jej statusu — nie robi tam zakupów spożywczych" | M3 (status, osobowość) |
| `AccessBarrier { kind: NoParking }` | „Przyjechałaby autem — nie masz parkingu w promieniu 150 m" | M4 (parkingi), M2 (parcela) |
| `AccessBarrier { kind: Queue { minutes } }` | „Kolejka 11 min przy jednej kasie; jej budżet czasu to 6 min" | M5 (obsługa) + M9 (obsada) |
| `PurchaseDeferred { best_u, threshold }` | „Odłożyła zakup — budżet rozrywki wyczerpany do 20." | M3 (budżet GD), M5 (próg użyteczności) |
| `SubstituteChosen { wanted, taken }` | „Kupiła margarynę zamiast masła — masło przekroczyło jej próg ceny" | M5 §6.4 |
| `SoftmaxDraw { p_chosen_bp, p_yours_bp }` | „Twój sklep miał 31% szans, wylosowała inny — to nie błąd, to rozkład" | M5 (softmax, §6.4) |
| `PolicyApplied { .. }` | „Twoja cena wynika z polityki »X«, reguła 2" (kontekst do powyższych) | M9 |

Ostatni wiersz jest ważny mechanicznie: gracz pytający „dlaczego Anna nie kupiła" i widzący
„cena o 12% wyższa" musi mieć jednym kliknięciem odpowiedź, **skąd wzięła się jego cena** —
inaczej diagnoza nie prowadzi do decyzji.

#### Rejestracja utraconej sprzedaży (`LostSale`)

Karty inspekcji nie da się zbudować, jeśli nikt nie zapisał, że Anna *rozważała* mój sklep.
Zapisywanie tego dla 400 tys. agentów jest nie do przyjęcia kosztowo, więc:

```rust
pub struct LostSale {
    pub at: SimMinute, pub citizen: CitizenId, pub good: GoodId,
    pub reason: DecisionReason, pub competitor: Option<SiteId>, pub u_gap_bp: i32,
}
```

- **Zawsze, dla zakładów gracza**: dobowy histogram powodów (≤ 16 kubełków, 2 słowa kodu na
  kubełek) — to jest wersja *użyteczna operacyjnie*: „Utracone wizyty dziś: 312 — 41% nie zna
  sklepu, 28% cena, 19% za daleko, 12% brak towaru".
- **Bufor cykliczny 256 wpisów** z nazwiskami i szczegółami, wyłącznie dla zakładów oznaczonych
  `observed_by_player` — to jest wersja *narracyjna*, ta, w której pojawia się Anna.
- Dla zakładów AI: nic. Koszt przy graczu z 200 sklepami: 200 × 256 × ~48 B ≈ 2,5 MB. Do przyjęcia.

To jest wymaganie do M5 (`sim/economy`), zgłoszone w §6 i §9.

### 5.10 Nakładki, filtry, śledzenie, czas

```rust
pub struct OverlaySpec { pub field: OverlayField, pub filter: Option<EntityFilter>,
                         pub palette: PaletteId, pub range: RangeMode }
pub enum OverlayField {
    LandValue, HouseholdIncome, ShopCatchment { site: SiteId }, Traffic,
    ProductPrice { good: GoodId }, Unemployment, Health, Pollution,
    GoodFlow { good: GoodId },                 // animowane strumienie
    BrandAwareness,                            // zarezerwowane dla M10
}
pub type EntityFilter = ConditionExpr;         // „tylko klienci mojego sklepu" itd.
```
`game/` wybiera i filtruje; rysowanie pól skalarnych i strumieni należy do `engine/render`
(M1/M2). M9 dostarcza `OverlaySpec` + widget legendy + skalę.

```rust
pub enum FollowTarget { Citizen(CitizenId), Vehicle(VehicleId), Batch(BatchId) }
```
Tryb „śledź": kamera podąża, na dole oś czasu doby **plan vs realizacja** (plan z planera dnia
M3, realizacja z faktycznych zdarzeń), panel potrzeb, budżet, ostatnie decyzje z uzasadnieniem.
Śledzenie partii: łańcuch pochodzenia od pola do półki z czasem i kosztem każdego etapu —
wymaga `BatchProvenance` z M6 (§9, decyzja otwarta). Cel śledzenia dostaje `LodPin` na Mikro.

```rust
pub enum TimeScale { Paused, X1, X3, X10, X50 }
pub struct StopCondition { pub id: StopConditionId, pub expr: ConditionExpr, pub once: bool }
```
Gotowy zestaw warunków („zatrzymaj przy zdarzeniu X" bez budowania wyrażenia):
brak towaru w dowolnym moim sklepie · saldo poniżej progu · nieudana dostawa · strajk ·
awaria maszyny · konkurent otworzył punkt w promieniu R · kontrakt wygasa za N dni ·
wojna cenowa na towarze X · cel scenariusza osiągnięty · eskalacja z polityki ·
zdarzenie w mieście (podatki, przetarg, wybory).

Ewaluacja `EveryHour` po stronie `game/` na snapshocie; trafienie ustawia `TimeScale::Paused`
na granicy klatki, podnosi alert i podświetla źródło w kronice. Ponieważ to strona widoku,
pauzowanie z definicji nie może zmienić wyniku symulacji.

Przy `X50` (tryb makro, ruch w mezo): panele przechodzą na odświeżanie `EveryHour`, `GanttView`
i tryb śledzenia są wyłączone (albo wymuszają Mikro na jednej encji, co jest kosztem zgłoszonym
graczowi). Bez tego UI staje się wąskim gardłem trybu 50×.

---

## Zmiany wpisane po decyzji właściciela produktu (2026-09-17)

Zgodnie z `K-18`. Gwiazdka = zmiana zakresu albo kryterium.

| # | Zmiana | Dlaczego |
|---|---|---|
| Z-1 ★ | **Karta ma nagłówek i zakładki, nie listę sekcji** (§5.7). Układ zakładek jest stały per typ encji i wypisany w tabeli | `docs/ui-design.md` §4 projektował kartę na sekcje jedna pod drugą — to działa dla mieszkanki z czterema polami i przestaje działać dla firmy z sześcioma obszarami. Wymaganie właściciela produktu: zakładki jako komponent UI. Widget `TabStrip` dostarcza `M9b` (Z-8 tamtego dokumentu) |
| Z-2 ★ | **Odnośnikiem jest każda nazwa podmiotu, nie tylko `Relations` i powód** | Pierwotny plan obiecywał „głębokie linki między kartami", ale nie powiedział gdzie — a jedyny nośnik (`CardSection::Relations`) unosiłby wyłącznie powiązania wymienione z nazwy. Adres w polu „mieszka" też ma być odnośnikiem, a to jest `State`, nie `Relations` |
| Z-3 ★ | **`InspectionNav` — stos wstecz/dalej po 32 pozycje; `ViewCommand` dostaje `NavigateBack`/`NavigateForward`** | Nie było tego w żadnym dokumencie planu. Przy karcie, w której wszystko jest linkiem, brak powrotu zamienia nawigację w błądzenie: trzy skoki i gracz zgubił pytanie, od którego zaczął |
| Z-4 | **`Subject` rośnie o pięć encji miejskich** (`Government`, `Tender`, `Case`, `Event`, `Permit`; usługa publiczna jedzie jako `Site`, wybory jako zakładka rady) i przenosi się do `engine/core` (`K-62`) | M8c/M8d/M8e produkują te byty i zapisują dla nich `DecisionReason`, ale żaden nie miał jak trafić do inspekcji — lista `Subject` kończyła się na `District`. Rada, przetarg i sprawa urzędowa to rzeczy, o które gracz będzie pytał „dlaczego", a bramka 5 wymaga odpowiedzi |
| Z-5 | **Cel, który przestał istnieć, renderuje się jako nazwa bez odnośnika plus powod** | `Subject::resolve` zwraca `Option` i `None` nie jest błędem — firmy upadają, ludzie umierają, partie się zużywają. Bez tej reguły pierwsza karta sprzed roku prowadzi w pustkę albo w panikę |

---

## Zmiany wpisane po M9a

Zgodnie z `K-18`. Pełne uzasadnienia — tabela `DA-n` w `M9a-szkielet-gry-i-komendy.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| DC-1 ★ | **Komendy postaci wchodzą razem z WP4, a nie są zastane.** `PlayerCommand` ma po M9a dwa warianty (`StartGame`, `SetPrice`); `ApplyForJob`, `AcceptJobOffer`, `RentHome`, `SetOwnShift` i reszta listy z `M9a` §5.5 dokłada się **na końcu enuma**, każdy ze swoim wykonawcą | Wariant bez wykonawcy przechodzi każdy test i wygląda tak samo jak działający (`K-67`). Kolejność wariantów jest kontraktem dziennika wejść, więc dopisywać wolno wyłącznie na końcu |
| DC-2 ★ | **Własność zakładu nie jest jeszcze sprawdzana w `precheck`** — `CommandError` nie ma wariantu `SiteNotOwned`, bo do WP4 nie ma czyjej własności sprawdzać. Wnosi go WP4 razem z `PlayerCharacter` | Sprawdzenie własności wobec nieistniejącej postaci byłoby gałęzią zawsze prawdziwą, czyli zaślepką udającą regułę |
| DC-3 | **`StartVariant` mieszka dziś w `game::shell`**, bo niesie go koperta `StartGame` od M9a (format koperty ma być stabilny). WP4 przenosi go do `game::player` razem z resztą postaci; `pub use` zostawia stary adres | Pole w kopercie dopisane po nagraniu pierwszych dzienników unieważniłoby je — dlatego wartość jedzie od początku, choć skutku nabiera dopiero tutaj |
| DC-4 | **Dane do karty i nakładek są w `Session`:** `session.market` (rynek, półki, utracone sprzedaże), `session.built.city` (miasto, parcele, zakłady), `session.built.terrain` (teren). `Selection` w `engine/ui` nadal ma trzy warianty i nadal nie ma `Subject` (`K-62`) — to zadanie WP5 | Zapisane, żeby WP5 nie zaczął od szukania, którędy dane docierają do panelu |


## Zmiany wpisane po M9b

Zgodnie z `K-18`. Pełne uzasadnienia — tabela `DE-n` w `M9b-rdzen-ui.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| DC-5 ★ | **`Subject` już jest** — w `engine/core::subject`, z jedenastoma wariantami (`Citizen`, `Household`, `Firm`, `Site`, `Building`, `Parcel`, `Vehicle`, `Contract`, `District`, `Event`, `Government`) plus `SubjectKind` do wyboru układu zakładek. WP5 dokłada **pięć brakujących** (`Batch`, `Offer`, `Tender`, `Case`, `Permit`) i podejmuje przy tym decyzję, gdzie mają mieszkać `TenderId`, `CaseId` i `PermitId` — dziś siedzą w `sim/city`, a `core` od nich nie zależy | `Span.link` musiał mieć typ już w M9b. Pozostałe pięć wariantów wymaga ruchu w cudzych crate'ach albo uchwytu areny (`K-16`) i jest decyzją karty, nie rdzenia UI (`DE-9`) |
| DC-6 ★ | **`Span`/`Rich` działają, ale odnośników jeszcze nie ma ani jednego.** `InspectorPanel::build`, `ShopCard::render_tab/render_header`, `SupplyCard` i `FirmCard` zwracają `Rich`; ziarnistość kawałka to dziś **jedna linia**, a `link` jest wszędzie `None`. `magnat_ui::widgets::rich` rysuje `Rich` i zwraca `Option<Subject>` — kliknięty odnośnik | Przypinanie linków to WP5, nie WP3: cięcie na kawałki drobniejsze niż linia bez ani jednego linku dałoby trzy razy więcej kawałków i zero wartości. Złoty wydruk nie drgnął — `RichExt::to_plain` składa je z powrotem bajt w bajt |
| DC-7 ★ | **`magnat_ui::Selection` nadal ma trzy warianty i nadal nie jest `Option<Subject>`.** Wymiana należy do WP5 i jest wykonaniem `K-62` pkt (1) | `Selection` ma dziś jednego pisarza (klik w pieszego) i jednego czytelnika (karta mieszkańca); przestawienie go bez karty, która korzysta z pozostałych wariantów, byłoby zmianą typu bez zmiany zachowania |
| DC-8 | **`TabStrip` jest gotowy** (`magnat_ui::tab_strip`): sufit siedmiu zakładek, strzałki lewo/prawo, wybór trzymany przez wołającego. Karta inspekcji z §5.7 układa nim swoje zakładki, zamiast pisać czwartą pętlę `selectable_label` | Wykonanie `Z-8` |
| DC-9 | **Nakładki mają już miniaturę** (`magnat_ui::HeatmapThumb`) czytającą **indeksy palety** z `data/ui/overlays.ron`, a nie wartości surowe. WP7 dokłada wybór pola i legendę, nie rysowanie | Jedna tabela barw dla klienta i dla podglądu bezgłowego (`K-19`) |
| DC-10 | **Klient ma już `GameState`, a `Citizens` przestało być właścicielem sesji.** Panel bierze `&Session` w argumencie, a stan gry (powłoka, generacja, podgląd, rozgrywka) trzyma `tools/magnat::App`. `GameState::CharacterSelect` nadal nie istnieje — dokłada go WP4 | Powłoka przechodzi przez stany, w których sesji **nie ma**, więc świat musiał dostać jednego właściciela ponad panelem |


## Zmiany wpisane w M9c

Zgodnie z `K-18`. Gwiazdka = zmiana zakresu albo kryterium. To są rzeczy, które wyszły
przy pisaniu kodu obok §5.3, §5.7 i §5.10 — każda zmienia coś, co dokument obiecywał
inaczej, niż dało się dotrzymać.

| # | Zmiana | Dlaczego |
|---|---|---|
| DG-1 ★ | **Rodzina powodów „dlaczego Anna nie kupiła" z tabeli §5.7 nie powstaje.** `NotInChoiceSet`, `LostToCompetitor`, `AccessBarrier`, `SubstituteChosen` i `SoftmaxDraw` **nie są nowymi wariantami `DecisionReason`**; blok M9 (700–799) zostaje w całości wolny. Odpowiedź składa się z tego, co M5e już zapisuje: `LostSale { cause: RejectCause, went_to: Option<SiteId> }` | Osiem wariantów `RejectCause` pokrywa wszystkie wiersze tamtej tabeli, które **mają dziś skutek w symulacji** (nie zna sklepu, za daleko, brak towaru, cena, jakość wobec statusu, wyczerpany budżet, poniżej progu), a `went_to` niesie zwycięzcę. Pozostałe wiersze — zamknięty sklep, brak parkingu, kolejka do kasy — opisują mechaniki, których **nie ma**: dopisanie dla nich wariantów dałoby powody, których nikt nigdy nie zapisze, czyli dokładnie to, przed czym broni `K-67`. Wiersze wrócą razem ze swoimi mechanikami i wtedy dostaną numery |
| DG-2 ★ | **`InspectionCard` mieszka w `game::inspect`, a jego kształt (`CardTab`, `CardTabKind`, widget) w `engine/ui`.** §5.7 rysował jedną strukturę bez adresu | Karta budynku, parceli i dzielnicy czyta `CityData` z `sim/world`, a `engine/ui` od `sim/world` **nie zależy**. Złożenie całej karty w interfejsie wymagałoby nowej krawędzi w grafie crate'ów tylko po to, żeby pokazać liczbę kondygnacji. §6 dokumentu fazy wymienia `InspectionCard` pod adresem `game::inspect` — i to ten adres jest prawdziwy |
| DG-3 ★ | **`CardSection` nie powstaje.** Zakładka jest `Rich`, czyli tekstem z odnośnikami, a nie listą typowanych sekcji (`State`/`History`/`Decisions`/`Relations`/`Actions`) | Z pięciu wariantów dwa nie mają dziś czym być wypełnione: `History` potrzebuje `Series` per encja (M9e), `Actions` — szablonów komend z paneli biznesowych (M9e). Zostałyby trzy, z których każdy i tak renderuje się do `Rich`, bo odnośnik siedzi **w kawałku tekstu**, a nie w sekcji (`Z-2`). Typ, który natychmiast po zbudowaniu zamienia się w `Rich`, jest formą zapisu, nie strukturą |
| DG-4 ★ | **`EntityFilter` nie jest `ConditionExpr` i ma dwa warianty, nie trzy.** `type EntityFilter = ConditionExpr` z §5.10 zastępuje enum `MyCustomers { site }` / `MyEmployees { site }`. Filtr „cysterny z paliwem" **nie powstaje** | Dwa powody, każdy osobno wystarczający. Metryki języka reguł (`sim/policy`, M7c) opisują cenę, zapas i kadry **firmy** — pytania „czy ten pieszy kupił u mnie" nie da się w nich zadać, więc drzewo składniowe byłoby pustą ramą. A cysterny nie mają czego filtrować: klient rysuje **pieszych**, pojazdów w kadrze nie ma (`R-2`, warstwa Mikro pojazdów czeka na M11). Filtr, którego skutku nie widać, przechodzi każdy test i wygląda jak działający |
| DG-5 ★ | **Predykat wyboru postaci też nie jest `ConditionExpr`** (`StartVariant::accepts(wiek, praca, oszczędności)`) | Ten sam powód co `DG-4` i ta sama granica: język reguł nie ma metryk demograficznych i mieć ich nie będzie, bo opisuje firmę, a nie mieszkańca. Predykat nad trzema liczbami nie potrzebuje drzewa |
| DG-6 ★ | **Łatka wariantu startu wykonuje się w komendzie `SetCharacter`, a nie jako `Vec<PlayerCommand>` w ticku 0.** Dochodzą **dwie** komendy: `SetCharacter { citizen }` i `SetAutonomy { field, control }` | `StartVariant::patch` z §5.3 miał zwracać listę komend — ale każda z nich musiałaby mieć wykonawcę, a wykonawców kapitału startowego i zerowania relacji nie ma i nie będzie: to jest **jedna** rzecz, która dzieje się raz. Wybór postaci jest przy tym osobną komendą, a nie polem `StartGame`, bo lista kandydatów powstaje **z postawionego świata**, czyli po tym, jak koperta startowa jest już w dzienniku |
| DG-7 | **Kapitał startowy wchodzi kanałem emisji** (`TxKind::Endowment`, `Books::household_receive`), tak samo jak kapitał firm stawianych przez generator | Test własnościowy „suma pieniądza = emisja − destrukcja" (00 §6) pokazałby inaczej brak pokrycia dokładnie o kapitał gracza. Łatka dzieje się w pierwszym ticku i jest częścią inicjalizacji świata, a nie dochodem z niczego |
| DG-8 | **`LodPin` powstaje w `sim/traffic`, a nie w `sim/agents`** — `MicroLayer::set_pinned` plus metoda `TravelOracle::set_micro_pins`, sufit `MAX_PINNED = 8`. To jest wykonanie decyzji otwartej §9 pkt 2 dokumentu fazy, której M3 ani M4 nie wykonały | Bramka kadru stoi w `MicroLayer::enter` i to ona decyduje, kto wchodzi do warstwy Mikro. Przypięcie po stronie `sim/agents` musiałoby ominąć tę bramkę albo ją zduplikować — a to jest ta sama bramka, o której `K-5` mówi, że nie ma prawa wpłynąć na wynik. `Lod` w komponencie agenta zostaje bez zmian: jest etykietą dla renderera, a nie wejściem symulacji |
| DG-9 | **Nakładki danych mają jeden wspólny raster po działkach** (`magnat_world::parcel_raster`, uogólnienie `land_value_raster` z M2) | Sześć z dziewięciu nakładek §14.2 różni się **wyłącznie liczbą**, którą stempluje: dochód, bezrobocie, zdrowie, cena, zasięg i wartość gruntu idą po tych samych wielokątach. Druga kopia tej pętli rozjechałaby się z pierwszą przy pierwszej zmianie rozdzielczości rastra |
| DG-10 | **Agregaty dzielnicowe liczą się przy przebudowie nakładki, jednym przejściem po mieszkańcach**, a nie w kroku symulacji | Nakładka jest widokiem i nie ma prawa zostawić po sobie stanu (00 §4). Koszt jest liniowy i płaci go ten, kto nakładkę otworzył — a `MacroCell` z `sim/macro`, który miałby te liczby gotowe, jest modelem „co jeśli" (M10), nie rejestrem bieżącego miasta |
| DG-11 | **Przepływ towaru rysuje się prostą między zakładami, nie trasą po drogach**, i nie niesie kierunku | Trasa wymagałaby przeliczenia routingu każdego zlecenia przy każdym przerysowaniu nakładki, a animowane strumienie z §14.2 należą do prezentacji (M11). `ponytail:` sufit nazwany w kodzie |
| DG-12 | **`ShopCustomers` dostaje pierścień 256 ostatnich kupujących** (`Market::is_customer`), bo filtr „tylko moi klienci" pyta o **osobę**, a rozkład po dzielnicach odpowiada o zbiorze | Koszt jest ten sam co przy pierścieniu utraconych sprzedaży i płacą go wyłącznie zakłady śledzone: 200 sklepów × 256 × 4 B ≈ 200 kB |
| DG-13 | **`magnat_ui::Selection` jest od tej chwili `Option<Subject>`** (wykonanie `K-62` pkt 1), a klient trzyma zaznaczenie w `InspectionNav` | Dwa równoległe słowniki „co da się zaznaczyć" rozjechałyby się przy pierwszej nowej encji, a rozjazd wyglądałby jak brak karty, nie jak błąd |
| DG-14 | **Karta sklepu przestaje być osobnym oknem.** Zakład otwiera się jako `Subject::Site` w tej samej karcie co mieszkaniec, z zakładkami Półki · Klienci · Konkurencja z migawki M5e plus „Dlaczego" z powodami przecen | `ui-design.md` §5: dok prawy ma **jedną** kartę i historię, nie stos okien. Dwa okna znaczyłyby, że odnośnik z karty mieszkanki do sklepu otwiera trzecie |
| DG-15 | **Majątek w wierszu slotu zapisu przestaje być zerem** — liczy się z gospodarstwa postaci gracza (`W-6` z M9b domknięte) | Postać gracza istnieje od tej podfazy, więc liczba ma z czego powstać |
