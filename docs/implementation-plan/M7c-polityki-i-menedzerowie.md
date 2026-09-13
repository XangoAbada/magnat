# M7c — Polityki i menedżerowie

Podfaza 3 z 6 fazy **M7 — Firmy AI i rynek pracy** (`M7-firmy-ai-i-rynek-pracy.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M7a (model firmy), AST języka reguł od M9 (K-11). |
| **Pakiety robocze** | WP6b, WP7 |
| **Projekt techniczny** | §5.4, §5.11 |
| **Wynik do pokazania** | Reguła gracza i polityka firmy AI wykonują się tym samym kodem; różnica leży w jakości menedżera. |
| **Kryterium zamknięcia** | Kryteria WP6b i WP7. |
| **Poprzednia / następna** | `M7b-rynek-pracy.md` · `M7d-finanse-i-upadlosc.md` |

Crate `sim/policy` — ewaluator języka reguł M9 z zakresami stosowania — oraz menedżerowie, delegowanie i `FirmPolicy` jako zestaw reguł.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| WP6b | `sim/policy`: ewaluator języka reguł M9, zakresy stosowania | WP1, AST od M9 | M |
| WP7 | Menedżerowie, delegowanie, `FirmPolicy` jako zestaw reguł | WP3, WP6, WP6b | L |

### WP6b — `sim/policy`: ewaluator języka reguł (K-11)

**Język projektuje M9** — jego AST (`ConditionExpr`, `Expr`, `Metric`, `Action`, `PolicyScope`)
jest dla M7 wiążącą specyfikacją. M7 buduje **ewaluator** i rejestr metryk/akcji, nie drugi język.

Powód istnienia crate'a: PRD §6.3 obiecuje graczowi „ten sam zestaw narzędzi co AI". Gdyby
`FirmPolicy` była zaszyta w Ruście dla AI, a M9 zbudowała osobny interpreter reguł dla gracza,
ta sama reguła („−2% względem najtańszego konkurenta w promieniu 3 km") zachowywałaby się
inaczej u gracza i u konkurenta. Jeden ewaluator usuwa tę klasę błędów z definicji — i jest
drogą do moddowalnej AI w M12 (polityka staje się plikiem danych, nie kodem).

Zakres pracy M7: ewaluacja `ConditionExpr`/`Expr` w arytmetyce całkowitej (pieniądz i64,
reszta w milijednostkach — bez f32 na ścieżce wpływającej na stan trwały), rejestr `Metric`
(odczyty **wyłącznie** przez `FirmView` — reguła nie może czytać tego, czego nie widzi firma),
rejestr `Action` mapowany na `OpsAction`/`TacAction`, zakresy stosowania `PolicyScope`
(firma / zakład / sklep / produkt) z rozstrzyganiem konfliktów: najbardziej szczegółowy zakres
wygrywa, przy równej szczegółowości — niższy indeks reguły w liście.

*Kryterium ukończenia:* ta sama reguła zastosowana do zakładu gracza i do identycznego zakładu
AI daje identyczną akcję (test równoważności — §7.10); ewaluator jest deterministyczny i nie
alokuje na ścieżce gorącej; każda ewaluacja kończąca się akcją produkuje `DecisionReason`
wskazujący regułę, która się wyzwoliła.

### WP7 — Menedżerowie i delegowanie

`Manager`, `SiteDelegation`, `FirmPolicy`. **Jeden silnik polityk dla AI i dla gracza** (`sim/policy`,
WP6b) — różni się wyłącznie **źródłem** zestawu reguł (AI: tier taktyczny generuje reguły;
gracz: edytor z §14.6) i jakością wykonania (menedżer).
Jakość zarządzania modyfikuje produktywność, rotację i straty.

*Kryterium ukończenia:* test monotoniczności menedżera (§7); zakład zdelegowany menedżerowi działa
bez ingerencji gracza przez rok gry i nie degeneruje się (magazyn nie pustoszeje, ceny nie uciekają).

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.4 Menedżerowie i delegowanie (§7.5)

```rust
pub struct Manager {
    pub citizen: CitizenId,
    pub skill_mgmt: Q,                 // umiejętność „management" z §5.1
    pub style: ManagerStyle,           // Taskmaster | Coach | Bureaucrat | Dealmaker
    pub sites: SmallVec<[SiteId; 4]>,  // rozpiętość kierowania
    pub tenure: SimMinute,
}

pub struct SiteDelegation {
    pub manager: Entity,
    pub policy: FirmPolicy,            // TEN SAM typ, którego używa AI i gracz (§14.6)
    pub autonomy: Autonomy,            // PricesOnly | PricesAndStaff | Full
    pub report_freq: Freq,
}

/// Jakość zarządzania: rzadki zasób. Rozpiętość ponad optimum degraduje liniowo.
pub fn management_quality(m: &Manager, site: &Site, morale: Q) -> ManagementQuality;
```

Wpływ `ManagementQuality` — trzy niezależne kanały, wszystkie realne i mierzalne w balansatorze:

| Kanał | Efekt | Konsument |
|---|---|---|
| Produktywność | `mgmt_mult` 0,85–1,15 | `effective_labor` → M6 |
| Rotacja | mnożnik prawdopodobieństwa odejścia 1,4–0,7 | `hr::turnover` |
| Straty | mnożnik ubytku, psucia, braków 1,5–0,6 | hook strat w M6 (magazyn, produkcja) |

Dobry menedżer jest podkupywany: `bidding::headhunt` traktuje role menedżerskie z podwyższonym
priorytetem. Odejście menedżera zakładu natychmiast obniża `ManagementQuality` do wartości
zastępstwa (`Autonomy::PricesOnly`, jakość = mediana firmy − 20) — stąd „dobry menedżer to zasób
rzadki" ma konsekwencje, a nie tylko opis.

**Delegowanie jest jedynym sposobem skalowania gracza.** Gracz z 200 sklepami nie klika 200 razy:
ustawia `FirmPolicy` i przypisuje menedżera, a jakość wykonania polityki zależy od menedżera.
Ten sam kod realizuje decyzje firm AI — różni się tylko źródło polityki.

### 5.11 `DecisionReason` — jak konkretnie (dokument 00 §7)

Enum z parametrami, nie string. M7 dokłada wariant przestrzeni nazw firm:

```rust
// w engine/core
pub enum DecisionReason { Citizen(CitizenReason), Firm(FirmReason), City(CityReason) /* ... */ }

#[derive(Copy, Clone)]                 // <= 24 B — mieści się w pierścieniu
pub enum FirmReason {
    PriceCut   { good: GoodId, from: Money, to: Money, cause: PriceCause },
    PriceRaise { good: GoodId, from: Money, to: Money, cause: PriceCause },
    WageRaise  { role: JobRoleId, from: Money, to: Money, cause: WageCause },
    Hired      { applicant: CitizenId, score: i32, runner_up: i32 },
    Rejected   { applicant: CitizenId, score: i32, threshold: i32 },
    Poached    { from_firm: FirmId, role: JobRoleId, premium_bp: u16 },
    LaidOff    { role: JobRoleId, count: u16, cause: LayoffCause },
    QuitLost   { citizen: CitizenId, to_firm: Option<FirmId>, cause: QuitCause },
    TrainingStarted { role: JobRoleId, budget: Money, skill_gap: u8 },
    ManagerAssigned { site: SiteId, skill_mgmt: Q, prev_quality: u16 },
    // UWAGA: żadnej „prognozy zysku" jako kwoty — wynik makro jest obarczony 3–12% błędu.
    // Zapisujemy POZYCJĘ wariantu i margines, nie wartość (patrz §5.10).
    SiteOpened { site_type: SiteTypeId, variants: u8, margin_bp: u16, trend: Trend },
    SiteClosed { site: SiteId, months_negative: u8, roi_bp: i32 },  // roi z WŁASNYCH ksiąg — twarde
    LoanTaken  { kind: LoanKind, amount: Money, liquidity_months_before: u8 },
    Factored   { amount: Money, discount_bp: u16, liquidity_months_before: u8 },
    PriceWar   { target: FirmId, observed_price: Money, share_lost_bp: u16 },
    SupplierLocked { supplier: FirmId, premium_bp: u16, rival: FirmId },
    Bankruptcy { trigger: BankruptcyTrigger, days_illiquid: u16 },
    Founded    { niche: GoodId, district: DistrictId, expected_margin_bp: i32 },
    ChainEntry { chain: ChainId, threshold: ChainThreshold },
}
pub enum WageCause  { VacancyDays(u16), ShortageIndex(u16), Counteroffer(FirmId), Retention }
pub enum PriceCause { StockHigh(u16), StockLow(u16), Rival(FirmId), Spoilage(u16), CostUp(u16) }
```

Zapis jest **wymuszony typem**. Jedyna droga wykonania akcji to

```rust
pub fn apply_decision<T: FirmAction>(cmds: &mut CommandBuffer, firm: FirmId, d: Decided<T>);
// zawsze: cmds.push(d.action) ORAZ log.push(LoggedDecision { tick, reason: d.reason })
```

Nie istnieje przeciążenie bez `reason`, więc nie da się podjąć decyzji bez powodu — to jest
warunek Definition of Done fazy, sprawdzany testem 7.8, a nie przeglądem kodu.
Pierścień 32 wpisów per firma (wzorzec pamięci z §17.7); starsze wpisy idą do kroniki na dysku (M9).
Koszt: 24 B × 32 × 10 000 firm ≈ 7,7 MB — w budżecie §17.7.
Inspektor (WP16) tłumaczy wariant enuma na zdanie po polsku — tłumaczenie jest w warstwie UI,
w symulacji nie ma żadnych stringów.
