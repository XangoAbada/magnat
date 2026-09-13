# M10d — Giełda, przejęcia, ubezpieczenia

Podfaza 4 z 6 fazy **M10 — Głębia** (`M10-glebia.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M7 (finanse firmy), M5 (banki). |
| **Pakiety robocze** | WP10.10, WP10.11, WP10.12 |
| **Projekt techniczny** | §5.5 |
| **Wynik do pokazania** | Notowanie powstaje z arkusza zleceń, a wrogie przejęcie da się przeprowadzić i obronić. |
| **Kryterium zamknięcia** | Kryteria WP10.10–WP10.12. |
| **Poprzednia / następna** | `M10c-rd-i-nowe-produkty.md` · `M10e-relacje-i-zwiazki.md` |

`Share`, `OrderBook` z fixingiem i `Valuation`, przejęcia, dywidendy, emisje i ład korporacyjny oraz ubezpieczenia.

---

## Pakiety robocze

### WP10.10 — Giełda: `Share`, `OrderBook`, fixing, `Valuation`

**Zależności:** M7 (księgowość firmy, raporty kwartalne), M5 (majątek GD).
**Kryterium ukończenia:**
- Fixing dzienny wyznacza jedną cenę maksymalizującą wolumen; przy remisie — cena najbliższa
  poprzedniemu fixingowi, przy dalszej remisie — niższa. Test na 10 tys. losowych ksiąg zleceń.
- Racjonowanie na marginesie pro rata; reszta do pierwszego wg posortowanego `OrderId` (dok. 00 §2).
  Test: suma przydzielonych akcji == wolumen, zawsze.
- Wycena reaguje na publikację wyników z opóźnieniem T+45 dni, nie wcześniej — test, że kurs nie
  porusza się przed publikacją, jeśli nie ma plotki.
- Suma akcji w obiegu == `Share.total_shares` po każdym fixingu, emisji i przejęciu (test własnościowy).
**Rozmiar: L.**

---

### WP10.11 — Przejęcia, dywidendy, emisje, ład korporacyjny

**Zależności:** WP10.10.
**Kryterium ukończenia:** przekroczenie 5% publikowane (→ plotka), przekroczenie 50% zmienia zarząd
i **podmienia osobowość firmy z M7 na osobowość przejmującego** — obserwowalna zmiana polityki
cenowej w ciągu miesiąca gry. Dywidenda dzieli kwotę bez utraty ani jednego grosza (test własnościowy
na 1000 losowych struktur akcjonariatu). Emisja rozwadnia proporcjonalnie, prawo poboru działa.
**Rozmiar: M.**

---

### WP10.12 — Ubezpieczenia

**Zależności:** M8 (`sim/events` jako źródło szkód), M7 (firma ubezpieczeniowa jako typ).
**Kryterium ukończenia:** składka za ubezpieczenie od powodzi w dzielnicy nadrzecznej po dwóch
powodziach w oknie 60 miesięcy jest ≥ 2× wyższa niż w dzielnicy wyżej położonej, **bez żadnego
parametru „ryzyko powodzi" w danych** — wyłącznie z historii zdarzeń. Ubezpieczyciel z ekspozycją
< 100 polis używa priora miejskiego (wygładzanie Bühlmanna) — test na małej próbce, że składka nie
skacze o rząd wielkości po jednej szkodzie.
**Rozmiar: M.**

---

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.5 Giełda, ubezpieczenia

```rust
pub struct Share {                    // klasa akcji firmy, nie pojedyncza akcja
    pub firm: FirmId,
    pub total_shares: u32,
    pub float: u32,                   // w wolnym obrocie
    pub listed_since: Option<SimMinute>,
    pub last_fixing: Money,           // cena za akcję
}
pub struct Holding { pub holder: HolderId, pub firm: FirmId, pub shares: u32 } // rzadkie

pub struct Order {
    pub id: OrderId, pub holder: HolderId, pub firm: FirmId,
    pub side: Side, pub limit: Money, pub qty: u32, pub expires: SimMinute,
}
pub struct OrderBook { pub firm: FirmId, pub buys: Vec<Order>, pub sells: Vec<Order> }

pub struct Valuation {
    pub fundamental: Money,   // z opublikowanych wyników
    pub belief: Money,        // fundamental × szum inwestora × korekta z plotek
    pub confidence: Q,
    pub reason: DecisionReason,
}

pub struct InsurancePolicy {
    pub id: PolicyId, pub insurer: FirmId, pub insured: PolicyHolder,
    pub peril: PerilKind, pub sum_insured: Money, pub deductible: Money,
    pub premium_monthly: Money, pub window: (SimMinute, SimMinute),
}
```

**Fixing zamiast notowania ciągłego.** Jeden fixing na dobę gry, o 17:00: cena maksymalizująca
wolumen dopasowany; remis → cena najbliższa poprzedniemu fixingowi; dalszy remis → niższa.
Racjonowanie na marginesie pro rata, reszta do pierwszego wg posortowanego `OrderId` (dok. 00 §2).
> *ponytail: notowanie ciągłe z ciągłą podwójną aukcją to O(zlecenia) mutacji stanu na tick i koszmar
> determinizmu przy równoległości. Fixing dzienny daje ten sam efekt gospodarczy dla miasta
> z ~3000 firm i ~5000 inwestorów, przy jednej deterministycznej redukcji na dobę.*

**Skąd bierze się cena (§6.5).** Wyłącznie z transakcji, nigdy z formuły wpisanej do stanu:

```
fundamental = max(wartość_księgowa, zysk_12m_opublikowany × mnożnik(branża, stopa, wzrost))
belief      = fundamental × (1 + szum(inwestor, firma, miesiąc)) × (1 + korekta_z_plotek)
```
Wyniki publikowane **kwartalnie z opóźnieniem T+45 dni i szumem** zależnym od jakości raportowania
firmy. Inwestor nie zna prawdziwego zysku — zna opublikowany. Szum `σ` zależy od wyrafinowania
inwestora: fundusz AI 5%, zamożny mieszkaniec 15–25%. Plotki (§5.7) przesuwają `belief` zanim
wyniki wyjdą — i to jest cała mechanika „ktoś wiedział wcześniej".

**Inwestorzy:** (a) mieszkańcy z majątkiem > progu (~1–2% populacji, decyzja `EveryMonth`),
(b) fundusze AI (5–15 encji, `EveryWeek`), (c) gracz. Skłonność do ryzyka bierze się z cechy
„Ryzyko" z §5.1 — bez nowego parametru.

**Przejęcia.** Próg 5% → obowiązek publikacji → `Rumor` + `Story`. Próg 50% → zmiana zarządu →
**podmiana `FirmPersonality` z M7 na osobowość przejmującego**. Wrogie: wezwanie z premią
(zwykle 15–35%), akcjonariusze porównują z `belief`. Przyjazne: negocjacje z zarządem i głównymi
akcjonariuszami, premia niższa, ale wyższa skuteczność.

**Ubezpieczenia — wycena wyłącznie z historii (§6.5).** Ubezpieczyciel prowadzi okno 60 miesięcy:

```rust
pub struct PerilStats { pub peril: PerilKind, pub scope: Scope, // District | City
                        pub exposures: u32, pub claims: u32,
                        pub loss_total: Money, pub sum_total: Money }
```

```
własna_stawka = loss_total / max(sum_total, 1)                       // szkodowość w bps
stawka  = (n × własna_stawka + K × prior_miejski) / (n + K)          // K = 100 polis
składka = div_round_half_up(stawka × sum_insured × (10000 + narzut_bps), 10000 × 12) + opłata
```

Wygładzanie (Bühlmann-lite, jedna linijka) jest konieczne, bo bez niego nowy ubezpieczyciel
po pierwszej szkodzie wycenia składkę na poziomie sumy ubezpieczenia i wypada z rynku w jednym kroku.
Po katastrofie ubezpieczyciel może być niewypłacalny; cesja X% ryzyka do „reszty świata" (§6.2 warstwa 3)
jest jedyną reasekuracją, jaką modelujemy.
> *ponytail: brak modelu reasekuracji jako rynku. Dodać, gdy gracz będzie mógł założyć reasekuratora —
> nie wcześniej.*
