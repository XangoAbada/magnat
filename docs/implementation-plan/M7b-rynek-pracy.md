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
| WP4 | Rynek pracy w `sim/economy`: oferty, aplikacje, wybór kandydata | WP3, M5 (maszyneria ofert) | L |
| WP5 | Licytacja płac, indeks niedoboru, headhunting | WP4 | M |
| WP6 | HR: szkolenia, premie, benefity, zwolnienia, rotacja | WP3, WP4 | M |

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

### WP5 — Licytacja płac i niedobór

`LaborMarketStats` — indeks niedoboru per (`JobRoleId`, `DistrictId`), liczony dziennie z wakatów,
aplikacji i czasu wiszenia oferty. Eskalacja stawki przy braku kandydatów, headhunting (oferta
bezpośrednia do zatrudnionego) przy wysokim niedoborze, degresja stawek przy nadmiarze (nowe
oferty poniżej mediany, malejąca płaca progowa bezrobotnego).
**Nigdzie nie ma tabeli płac** — mediana w UI to agregat ofert i zaakceptowanych umów.

*Kryterium ukończenia:* test `wage_reacts_to_shortage` (§7) zielony w CI; brak spirali płacowej
w 5-letnim przebiegu (górne ograniczenie: firma nie licytuje powyżej płacy, przy której marża
zakładu spada poniżej progu z polityki).

### WP6 — HR

Szkolenia (koszt + czas + przyrost umiejętności z sufitem od talentu), premie (funkcja wyniku
zakładu i polityki), benefity (auto służbowe, opieka medyczna — realne zaspokojenie potrzeb
z §5.3, nie liczba dodawana do nastroju), zwolnienia z odprawą i kosztem reputacyjnym, rotacja
dobrowolna i przymusowa.

*Kryterium ukończenia:* benefit „opieka medyczna" mierzalnie podnosi poziom potrzeby *Zdrowie*
u pracownika w symulacji (nie tylko nastrój); odejście pracownika zawsze ma `DecisionReason`
po stronie odchodzącego (lepsza oferta / nastrój / stres / zwolnienie).

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
