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
| **WP4** | Postać gracza i start | WP2 | `PlayerCharacter`, `PlayerAutonomy`, `StartVariant` jako filtr+łatka na populacji, ekran wyboru postaci, przypięcie do LOD Mikro | Da się wybrać mieszkańca, zobaczyć jego rodzinę/pracę/znajomych i przeżyć dzień; 5 wariantów startu działa |
| **WP5** | Karta inspekcji w pełnej postaci | WP3, WP4 | `InspectionCard`, wyczerpujący render `DecisionReason`, `LostSale` (bufor cykliczny + histogram dobowy), głębokie linki między kartami | Odpowiedź na „dlaczego Anna nie kupiła u mnie?" w PL i EN, z nazwanym konkurentem i klikalnym odnośnikiem |
| **WP7** | Nakładki danych i filtry | WP3, WP6 | `OverlaySpec`, `EntityFilter`, legenda, przełącznik warstw, filtry „tylko moi klienci / moi pracownicy / cysterny z paliwem" | 9 nakładek z §14.2 działa (bez `BrandAwareness` — zarezerwowana dla M10) |

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
pub struct InspectionCard { pub subject: Subject, pub sections: Vec<CardSection> }
pub enum Subject { Citizen(CitizenId), Household(HouseholdId), Firm(FirmId), Site(SiteId),
                   Building(BuildingId), Parcel(ParcelId), Vehicle(VehicleId), Batch(BatchId),
                   Offer(OfferId), Contract(ContractId), District(DistrictId) }
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
