# M12e — Lokalizacja i domknięcie

Podfaza 5 z 6 fazy **M12 — Skala i jakość** (`M12-skala-i-jakosc.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M12c (skala), M9/M11 (istniejące UI). |
| **Pakiety robocze** | WP13, WP14 |
| **Projekt techniczny** | §5.8, §5.9 |
| **Wynik do pokazania** | Pełny artefakt fazy z §1 dokumentu fazy — wszystkie pięć punktów zmierzone testem w CI. |
| **Kryterium zamknięcia** | Kryteria WP13 i WP14 oraz bramki 1–7 fazy M12 w `00-postep.md`. |
| **Poprzednia / następna** | `M12d-modding.md` · `M12f-oprawa-ui.md` (niezależna, może iść wcześniej) |

`LocaleCatalog` z pluralizacją CLDR i przypadkami gramatycznymi PL, testy pokrycia kluczy oraz sesja 100-letnia, detekcja dryfu ekonomicznego i benchmarki.

---

## Pakiety robocze

### WP13 — Lokalizacja PL/EN [M]
Zależności: M9/M11 (istniejące UI).

`LocaleCatalog`, pluralizacja CLDR (pl: one/few/many/other), przypadki gramatyczne dla nazw z `data/`, rodzaj gramatyczny w kronikach, przełączanie języka bez restartu.

**Kryterium ukończenia:** test `all_keys_present_in_all_locales()` i `no_unused_keys()` zielone; wektor pluralizacyjny CLDR dla pl i en przechodzi; zrzuty UI w pseudo-locale `x-long` bez przycięć tekstu.

### WP14 — Testy długich sesji, balansator, benchmarki [L]
Zależności: WP10.

Harness sesji 100-letniej, metryki degeneracji z progami (§7.4), balansator rozszerzony o regresję trendu w oknie 20-letnim, benchmarki `criterion` w CI z detekcją regresji.

**Kryterium ukończenia:** 5 seedów × 100 lat × miasto rosnące 150 tys. → 400 tys. bez przekroczenia żadnego progu alarmowego; raport HTML z przebiegu jako artefakt CI.

---

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.8 Lokalizacja

```rust
pub struct LocaleCatalog {
    locale: LocaleId,                       // "pl-PL" | "en-US"
    plural_rule: fn(i64) -> PluralCategory, // reguła CLDR
    entries: BTreeMap<TextKey, MessageTemplate>,
    fallback: Option<Arc<LocaleCatalog>>,   // brakujący klucz → en-US → sam klucz (widoczny w UI)
}

pub struct TextKey(pub u32);                // internowany, stabilny hash klucza tekstowego
pub enum PluralCategory { One, Few, Many, Other }   // pl używa wszystkich czterech, en dwóch
pub enum Case { Nom, Gen, Dat, Acc, Inst, Loc }     // ignorowane w en
pub enum Gender { Masc, Fem, Neut }
```

Trzy rzeczy, które odróżniają to od „przetłumaczymy stringi":

1. **Pluralizacja CLDR, nie `if n == 1`.** PL: `1 sklep` (one), `2–4 sklepy` (few), `5+ sklepów` (many), `1,5 sklepu` (other). EN: one/other. Szablon: `{count, plural, one {# sklep} few {# sklepy} many {# sklepów} other {# sklepu}}`.
2. **Przypadki dla nazw z `data/`.** PL wymaga odmiany: „Brak: chleb" vs „Kupiono chleba". Katalogi (`data/goods/*.ron`, `data/buildings/*.ron`) dostają pole `name` z formami przypadków; szablon deklaruje żądany: `{good:gen}`. Dla EN pola przypadków są ignorowane — jedna forma plus liczba mnoga. To rozszerzenie schematu `data/` i wymaga bumpa `schema_version` tych katalogów.
3. **Rodzaj gramatyczny w kronikach.** „Anna otworzyła piekarnię" vs „Jan otworzył piekarnię". `Gender` w katalogu imion (M2/M3), szablon: `{actor:gender, select, fem {otworzyła} other {otworzył}}`.

Przełączanie języka bez restartu: `LocaleCatalog` za `Arc`, zmiana unieważnia cache tekstu w UI (dirty-flag całego drzewa widgetów). Kroniki przechowują `TextKey` + argumenty, nie gotowy tekst — dlatego zapis z polskiej sesji otwarty po angielsku ma angielską kronikę. Wymaga, by M9 zapisywało kroniki strukturalnie, co i tak wynika z dok. 00 §7 (`DecisionReason` jako enum, nie string).

### 5.9 Systemy ECS dodawane przez M12

| System | Częstotliwość | Odczyt / zapis | Rola |
|---|---|---|---|
| `SaveBarrierSystem` | `EveryMinute` | cały świat (R), `save_epoch` (W) | Sprawdza żądanie zapisu, stawia barierę, buduje `SaveManifest` |
| `JournalAppendSystem` | `EveryMinute` | bufor wejść gracza (R), dziennik (W) | Dopisuje `JournalEntry` po aplikacji komend |
| `AutosaveTriggerSystem` | `EveryHour` | zegar gry, polityka autozapisu | Decyduje: snapshot bazowy czy przyrost dziennika |
| `ChronicleFlushSystem` | `EveryHour` | bufor kroniki (R/W) | Wypycha blok 4 MB na dysk, aktualizuje indeks |
| `ScriptHookDispatchSystem` | `EveryDay` (domyślnie) | wg deklaracji hooka | Wykonuje hooki modów w kolejności `(HookKind, load_order, mod_id)`, zbiera komendy |
| `MemoryProbeSystem` | `EveryDay` | liczniki alokatora, liczby encji | Metryki degeneracji; w `release` tylko RSS i liczby encji |
| `SpeedGovernorSystem` | poza tickiem (pętla gry) | historia czasów ticka | Watchdog budżetu, degradacja prędkości |

---
