# M10f — Kroniki i domknięcie

Podfaza 6 z 6 fazy **M10 — Głębia** (`M10-glebia.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M10a–M10e. |
| **Pakiety robocze** | WP10.15, WP10.16 |
| **Projekt techniczny** | §5.10 |
| **Wynik do pokazania** | Pełny artefakt fazy z §1 dokumentu fazy: marka z pamięci agentów, R&D, giełda, historia „na sucho”. |
| **Kryterium zamknięcia** | Kryteria WP10.15 i WP10.16 oraz bramki 1–7 fazy M10 w `00-postep.md`. |
| **Poprzednia / następna** | `M10e-relacje-i-zwiazki.md` · — (ostatnia w fazie) |

Kroniki i wyjaśnialność dla wszystkich nowych mechanik oraz dopisanie stanu M10 do funkcji haszującej.

---

## Pakiety robocze

### WP10.15 — Kroniki i wyjaśnialność

**Zależności:** M9 (podsystem kroniki), wszystkie WP tej fazy.
Przekrojowy. Każda nowa decyzja ma `DecisionReason` (dok. 00 §7). Nowe warianty `ChronicleEvent` z §5.9.
**Kryterium ukończenia:** 100% nowych decyzji ma czytelny powód w karcie inspekcji; audyt ręczny
20 losowych wpisów kronikarskich pod kątem zrozumiałości dla gracza.
**Rozmiar: S.**

---

### WP10.16 — Determinizm i hash stanu

Przekrojowy, Definition of Done fazy. Nowe warianty `StreamId`, dopisanie nowych komponentów do
funkcji haszującej, testy dwóch przebiegów. **Rozmiar: S.**

---

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.10 Systemy ECS i ich częstotliwość

| System | Częstotliwość | LOD | Odczyt/zapis |
|---|---|---|---|
| `brand_decay_compact` | `EveryMonth` | mezo | W: `BrandSlots` |
| `ad_expose_billboard` | `EveryHour` | mezo | R: przejazdy M4; W: `BrandSlots`, `CampaignMetrics` |
| `ad_expose_media` | `EveryDay` | mezo | R: `MediaOutlet`; W: `BrandSlots` |
| `ad_expose_leaflet` | `EveryDay` | mezo | R: indeks przestrzenny M2; W: `BrandSlots` |
| `ad_budget_burn` | `EveryDay` | — | W: `Ledger` przez `ledger_post` |
| `brand_strength_aggregate` | `EveryDay` | — | R: `BrandSlots`; W: agregat UI |
| `media_editorial` | `EveryDay` | — | R: `sim/events`; W: `Rumor` |
| `rnd_progress` | `EveryDay` | — | R: badacze, budżet; W: `ResearchProject` |
| `rnd_unlock` | `EveryDay` | — | W: `Patent`, efekty technologii |
| `epoch_advance` | `EveryMonth` | — | W: `EpochState`, koszyki potrzeb |
| `investor_decide_funds` | `EveryWeek` | — | W: `Order` |
| `investor_decide_citizens` | `EveryMonth` | — | W: `Order` |
| `stock_fixing` | `EveryDay` (17:00) | — | R/W: `OrderBook`, `Holding`, `Share` |
| `earnings_publish` | `EveryDay` (sprawdza harmonogram T+45) | — | W: `PublishedReport` |
| `takeover_check` | `EveryDay` | — | W: `FirmPersonality` (M7), `Rumor` |
| `dividend_pay` | `EveryMonth` | — | W: `Ledger` |
| `insurance_underwrite` | `EveryMonth` | — | R: `PerilStats`; W: `InsurancePolicy` |
| `insurance_claims` | `EveryDay` | — | R: `sim/events`; W: `Ledger`, `PerilStats` |
| `supplier_trust_update` | `EveryDay` | mezo+makro | W: `SupplierRelation` |
| `cartel_detect` | `EveryMonth` | — | W: kary, `BrandSlots`, kronika |
| `union_grievance` | `EveryDay` | — | R: księgi M7, graf relacji M3 |
| `union_negotiate` | `EveryWeek` | — | W: `Union`, płace |
| `strike_tick` | `EveryDay` | — | W: produkcja zakładu, `Ledger` |
| `macro_step` | `EveryDay` (tylko w trybie makro) | makro | R/W: `MacroState` |

---


---

## Zmiany wpisane po M10b

Zgodnie z `K-18`. Wpisane jest **tylko to, co wiadomo na pewno** po zamknięciu M10b.
Gwiazdka = zmiana zakresu albo kryterium. Szczegóły — tabela `F-n` i sekcja
„Co M10b zostawia następnym podfazom" w `M10b-marka-i-media.md`.

| # | Zmiana | Dlaczego |
|---|---|---|
| FF-1 ★ | **Panel marketingu i komenda „kup reklamę" należą do tej podfazy.** M10b otwiera kampanie wyłącznie przez `magnat_media::ai` — raz na miesiąc, dla firm AI. Gracz nie ma jak kupić billboardu, więc artefakt C z §1 dokumentu fazy nie jest jeszcze osiągalny | `DK-3` opisuje drogę: panel dokłada się przez `PanelRegistry::register`, a `PanelId::Brand` już istnieje i `is_reserved()` mówi o nim prawdę. Dane dla panelu są gotowe: `CampaignMetrics` niesie ekspozycje i **pierwsze kontakty**, czyli lejek „znajomość → próba → afinitet" z M10 §6 pkt 1 |
| FF-2 | **Dziennik redakcji jest gotowy do zebrania: `Outlets::reasons() -> &[(Tick, DecisionReason)]`**, pierścień 128 wpisów, w hashu stanu | Wykonanie `DK-1`: zdarzenie M10 zapisuje powód **u siebie**, a `game::chronicle::Chronicle::harvest` dokłada dla niego źródło. Wzorzec jest ten sam co przy `Events::reasons()` |
| FF-3 ★ | **`DecisionReason::BrandLearned` i `BrandExperience` nie mają trwałego czytelnika.** Są zwracane przez `magnat_agents::touch` i rysowane przez `reason::describe`, ale nikt ich nie zapisuje | Pełny log decyzji dla 400 tys. mieszkańców to 14 GB na rok gry (M3, decyzja 9.16), więc karta pokazuje **stan** slotu (zakładka „Marki"), a nie historię kontaktów. Trwałym czytelnikiem ma być panel marketingu i lejek z `CampaignMetrics` — czyli `FF-1` |
| FF-4 ★ | **Tytuł medialny nie ma karty.** Otwiera się jako `Subject::Site`, bo jest zakładem, ale czytelnictwo, wiarygodność i linia redakcyjna nie mają gdzie się pokazać | Faza dokładająca byt z kartą dokłada wariant `Subject` **i** ramię w `game::inspect::card` (`K-69`). Tu wariantu nie trzeba: tytuł **jest** zakładem, więc wystarczy rozgałęzienie w karcie zakładu — zakład z wpisem w `Outlets` dostaje dodatkową sekcję |
| FF-5 ★ | **Wiarygodność tytułu nie spada i to jest jedyna obietnica §5.3, której M10b nie dowiózł.** Mechanizm ma nośnik (slot marki tytułu w pamięci czytelnika), ale nie ma wyzwalacza: żeby czytelnik stracił zaufanie, trzeba porównać **tezę** tekstu z jego własną obserwacją, a `Story` niesie dziś `EventId`, nie tezę | Rozstrzygnięcie, którego M10b nie umiał podjąć: teza tekstu to albo nowe pole w `Story` (kierunek i siła oceny), albo wyprowadzenie z kategorii i skali zdarzenia. Pierwsze jest uczciwsze, drugie darmowe. **Propozycja domyślna: wyprowadzenie**, bo redakcja i tak nie ma z czego zbudować tezy innej niż „to zdarzenie jest takie a takie" |
| FF-6 | **Czytelnictwo tytułu jest regułą bez rozrzutu**: najlepiej w swojej dzielnicy, dwa razy słabiej poza nią. Numer `StreamId` na rozrzut **nie jest zarezerwowany** | Dopóki nikt czytelnictwa nie stroi, numer zapisałby na wieczność liczbę bez właściciela — ta sama reguła, którą `K-63` zastosował do `EventHazard`, a `K-67` do `CityPolicy` |
| FF-7 | **Karta mieszkańca ma siedem zakładek, czyli sufit `MAX_CARD_TABS`** (`F-18`) | Ósma wymaga decyzji, którą z obecnych złożyć. Kronika mieszkańca, gdyby miała być zakładką, wchodzi za którąś z siedmiu — a nie obok |
| FF-8 ★ | **Budżet §7.5 czeka na pomiar przy pełnej skali.** Przy trzydziestu pięciu kampaniach udział mediów w dobie `m7miasto` mieści się w szumie: ścięcie 70 % ich pracy (zawężenie ulotek do dzielnicy) zmieniło dobę z 14,83 s na 15,29 s. Kryterium „≤ 1,5 % budżetu ticku przy **2000** aktywnych kampanii" wymaga jednak przebiegu z profilem, bo dwa tysiące to pięćdziesiąt razy więcej | Adres jest naturalny: M10f i tak stawia przebieg balansatora dla całej fazy. Znane wejście: kanał ulotkowy był jedynym, który skalował się z **liczbą mieszkańców razy liczba kampanii**, i został zawężony (`F-27` w M10b). Pułapka do uniknięcia przy mierzeniu: doba `m7miasto` kosztuje 1,15 s w piątej dobie i ~15 s w czterdziestej **bez udziału mediów** — porównanie dwóch różnych dób mierzy wzrost gospodarki, nie zmianę kodu |
