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
