# M12d — Modding

Podfaza 4 z 5 fazy **M12 — Skala i jakość** (`M12-skala-i-jakosc.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | Stabilne API `sim/*` (po M10), M12b (`ModStamp` w nagłówku zapisu). |
| **Pakiety robocze** | WP11, WP12 |
| **Projekt techniczny** | §5.7 |
| **Wynik do pokazania** | `example-mod/` — nowy towar, typ zakładu, polityka AI, panel UI i zdarzenie — instaluje się i przechodzi walidator determinizmu. |
| **Kryterium zamknięcia** | Kryteria WP11 i WP12. |
| **Poprzednia / następna** | `M12c-tryb-50x-i-skala.md` · `M12e-lokalizacja-i-domkniecie.md` |

`engine/script`: host Lua z sandboxem i `ScriptHook`, `ModManifest`, menedżer modów w grze i walidator determinizmu moda.

---

## Pakiety robocze

### WP11 — `engine/script`: host Lua i sandbox [XL]
Zależności: brak twardych; wymaga stabilnego API `sim/*` (po M10).

Host `mlua` (Lua 5.4), budowa `_ENV` z białej listy, typowane API kontekstu, bufor komend moda, `ScriptHook`, budżet instrukcji.

**Kryterium ukończenia:** przykładowy mod (`example-mod/`) realizuje wszystkie cztery klasy hooków z §18.3 PRD (nowe zdarzenie, polityka AI, nowy typ zakładu, panel UI); test `sandbox_escapes()` — 30 prób ucieczki z piaskownicy (lista w §5.7) kończy się kontrolowanym błędem, nie efektem ubocznym.

### WP12 — `ModManifest` i menedżer modów [L]
Zależności: WP11, WP4 (`ModStamp` w nagłówku zapisu).

Manifest, rozwiązywanie zależności i konfliktów, deterministyczna kolejność ładowania, menedżer w grze, **walidator determinizmu moda**.

**Kryterium ukończenia:** mod celowo niedeterministyczny (w korpusie testowym: wywołuje `pairs` po tablicy z kluczami-stringami i zapisuje kolejność do stanu) zostaje **odrzucony** przez walidator; mod poprawny przechodzi; wczytanie zapisu z inną listą modów daje jawne ostrzeżenie i oznaczenie sesji `ModsChanged`.

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.7 Modding: `engine/script`

**Wybór hosta: Lua 5.4 przez `mlua`.** WASM/`wasmtime` jest **odroczony** — dwa hosty to dwie powierzchnie sandboxa do audytu i dwa zestawy dowodów determinizmu, przy jednej korzyści (wydajność modów, której nikt jeszcze nie potrzebuje). API kontekstu jest projektowane jako czysty zestaw funkcji hosta, więc dołożenie backendu `wasmtime` później nie wymaga zmiany kontraktu z modderem. Lua wygrywa progiem wejścia, a modding bez modderów jest martwy.

```rust
pub struct ModManifest {
    id: ModId,                     // odwrotna domena, np. "pl.kowalski.piekarnie"
    name: LocalizedText,
    version: Version,              // semver
    engine_api: VersionReq,        // kompatybilność z API skryptowym
    data_schema: u32,              // wersja schematu data/
    requires: Vec<(ModId, VersionReq)>,
    conflicts: Vec<ModId>,
    load_order_hint: i32,          // rozstrzyganie remisów; ostateczna kolejność jest deterministyczna
    data_dirs: Vec<RelPath>,       // wyłącznie względne do katalogu moda
    scripts: Vec<ScriptEntry>,     // plik + deklarowane hooki
    permissions: ModPermissions,   // deklaratywne, zawsze podzbiór sandboxa
    checksum: u64,                 // xxh3 wszystkich plików moda — trafia do SaveHeader.mods
    determinism_verified: bool,    // ustawiane przez walidator, nie przez autora
}

pub enum ScriptHook {
    OnTick(Frequency),              // EveryDay/EveryMonth domyślnie; EveryHour/EveryMinute za permisją
    OnEventFired(EventKindId),
    OnFirmPolicyDecision,           // zwraca DecisionOverride + DecisionReason (dok. 00 §7)
    OnSiteProduce,
    OnAgentPurchaseScore,           // modyfikator użyteczności zakupu
    OnCityPolicyVote,
    OnWorldgenPostPass,
    OnUiPanel(PanelSlot),           // wyłącznie wątek UI; NIE MOŻE dotknąć stanu symulacji
}
```

**Sandbox — operacje dozwolone:**

- odczyt plików **wyłącznie** z `mods/<id>/` przez wirtualny FS montowany read-only (ścieżki kanonizowane, `..` i ścieżki absolutne odrzucane przed dotknięciem systemu plików);
- zapis **wyłącznie** do `mods/<id>/state/` (ustawienia moda), limit 1 MB, format RON, jeden plik — i **nigdy z wnętrza hooka symulacyjnego**, tylko z hooka UI;
- odczyt stanu symulacji przez typowane API kontekstu (`ctx:firm(id):cash()`, `ctx:good_price(good, district)`), zawsze zwracające dane posortowane po id;
- mutacje **wyłącznie** przez bufor komend hosta (`ctx:cmd():set_price(site, good, money)`) — ten sam mechanizm, którym mutują systemy silnika, sortowany po `(SystemId, entity_index)` zgodnie z dok. 00 §3.4; mod dostaje własny, stały `SystemId` z pozycją wynikającą z kolejności ładowania;
- losowość **wyłącznie** przez `ctx:rng()` → `rng(world_seed, StreamId::Script, mod_slot << 24 | entity_index, tick)` — slot moda wchodzi do indeksu, żeby dwa mody nigdy nie dzieliły strumienia;
- czas **wyłącznie** gry: `ctx:sim_minute()`, `ctx:tick()`, `ctx:date()`;
- logowanie do konsoli gry, teksty przez `ctx:text(key, args)` (pełna lokalizacja, §5.8);
- alokacja w obrębie limitu heapu moda (domyślnie 16 MB, twardy sufit sumaryczny 128 MB).

**Sandbox — operacje zakazane.** Zakaz jest egzekwowany przez **brak funkcji w środowisku**, nie przez konwencję ani skanowanie źródeł:

| Zakazane | Mechanizm |
|---|---|
| Jakiekolwiek IO poza `mods/<id>/` | `io` i `os` nieobecne w `_ENV`; wirtualny FS kanonizuje i odrzuca |
| Sieć | Brak jakiegokolwiek API sieciowego w hoście. Kropka |
| Zegar rzeczywisty (`os.time`, `os.clock`, `os.date`) | `os` nieobecne; odpowiedniki w `ctx` zwracają czas **gry** |
| `os.execute`, `io.popen` | `os`, `io` nieobecne |
| `require`, `dofile`, `loadfile`, `load`, `loadstring` | Usunięte z `_ENV`; ładowanie modułów wyłącznie przez host, z listy w manifeście |
| `package.loadlib`, FFI, biblioteki natywne | `package` nieobecne |
| `debug.*` | Usunięte z `_ENV` **po** zainstalowaniu przez host licznika instrukcji |
| Wątki, wyjście z hooka przez `coroutine.yield` | `coroutine` dostępne wewnątrz hooka, ale yield przez granicę hosta = błąd |
| Iteracja po niedeterministycznej kolekcji | API nie eksponuje żadnej mapy hash; `pairs` podmienione na wariant sortujący klucze |
| Float na ścieżce pieniądza | API `Money`/`Qty` przyjmuje wyłącznie `math.type(x) == "integer"`, resztę odrzuca błędem |
| Mutacja stanu z hooka UI | `OnUiPanel` dostaje kontekst tylko do odczytu — `ctx:cmd()` nie istnieje w tym wariancie |

**Wymuszanie determinizmu — pięć mechanizmów.** Wymóg „mod nie może złamać determinizmu" nie jest spełniany jednym trikiem:

1. **Środowisko bez źródeł niedeterminizmu.** `_ENV` budowane z białej listy, nie przez usuwanie z domyślnego. `math.random` i `math.randomseed` zastąpione przekierowaniem do `ctx:rng()`. `pairs` zastąpione wariantem sortującym klucze — kolejność `pairs` w Lua zależy od hashowania stringów i **nie jest stabilna między uruchomieniami**; to najczęstsze źródło niedeterminizmu w modach i najłatwiejsze do przeoczenia. `table.sort` udokumentowany jako niestabilny — API dostarcza `ctx:sort_stable()`.
2. **Budżet instrukcji, nie czasu.** Limit to N wykonanych instrukcji VM (Lua: licznikowy `debug.sethook` instalowany przez host **zanim** `debug` zniknie z `_ENV`). Przekroczenie → hook przerwany, jego bufor komend odrzucony w całości, mod oznaczony `Faulted`, komunikat w konsoli. Kluczowe: liczba instrukcji jest **deterministyczna**, czas ścienny nie — limit czasowy złamałby determinizm dokładnie tam, gdzie miał go chronić. Domyślnie 200 tys. instrukcji na hook, 2 mln na tick sumarycznie.
3. **Pieniądz i ilości tylko całkowitoliczbowo.** Lua 5.4 ma podtyp integer; API pieniądza waliduje `math.type` i odrzuca float. Zgodne z dok. 00 §2 („nigdy f32/f64 w pieniądzu").
4. **Odcisk konfiguracji modów w zapisie.** `SaveHeader.mods: Vec<ModStamp { id, version, checksum }>`. Wczytanie zapisu przy innej liście lub innych checksumach → flaga `MODS_CHANGED`, ostrzeżenie dla gracza, unieważnienie replayu i wyników rankingowych. Bez tego determinizm jest nieweryfikowalny, bo nie wiadomo, co właściwie liczyło.
5. **Walidator determinizmu jako bramka instalacji.** Menedżer modów uruchamia headless: 2 przebiegi × 5 000 ticków na małym mieście z tym samym seedem → porównanie ciągów hashy. Wynik zapisuje `determinism_verified`. Mod, który nie przejdzie, **nie jest blokowany** — jest degradowany: działa w piaskownicy, jest niedostępny w kariery i w rankingach, i mówi o tym wprost w menedżerze. Blokada zniechęciłaby modderów; degradacja chroni graczy.

Kolejność wykonania hooków: sortowana po `(HookKind, load_order, mod_id)` — nigdy po kolejności zwróconej przez system plików.

**Menedżer modów w grze:** lista zainstalowanych z ich statusem determinizmu, rozwiązywanie zależności i konfliktów, przeciąganie kolejności ładowania (zapisywanej jawnie, nie wyliczanej), przycisk „sprawdź determinizm" uruchamiający walidator, ostrzeżenie przed wczytaniem zapisu z inną konfiguracją. Warsztat online — poza zakresem M12 (to infrastruktura, nie gra).
---

## Zmiany wpisane po M2d

Zgodnie z `K-18`: praca nad M2d/M2f pokazała, że ten dokument jest w trzech miejscach
niedopowiedziany wobec katalogu, który już istnieje. Gwiazdką oznaczone te, które
zmieniają **zakres albo kryterium**.

| # | Zmiana | Dlaczego |
|---|---|---|
| G1 ★ | **`data/grammar/` jest katalogiem moddowalnym i wchodzi do `ModManifest.data_dirs` imiennie.** `GrammarSet::load_dir(&Path, &MaterialRegistry)` dostaje wariant scalający katalog bazowy z katalogami modów w **deterministycznej kolejności ładowania** — ta sama, którą manifest rozstrzyga dla hooków (`load_order`, potem `mod_id`), nigdy kolejność z systemu plików | §5.7 wymienia `data_dirs` jako mechanizm, ale nie mówi, które katalogi realnie coś przyjmują. Gramatyka budynku jest do tego najlepszym kandydatem w całym `data/`: to czyste dane z walidatorem, zamkniętym zbiorem operatorów i bez dostępu do stanu symulacji (`If` czyta wyłącznie parametry wejściowe budynku), więc mod dokładający budynek nie ma jak złamać determinizmu. Dziś `load_dir` bierze **jedną** ścieżkę i jest to jedyne, co dzieli ten katalog od moddowalności. Ważne dla gracza: różnorodność zabudowy przestaje być wtedy ograniczona zawartością repo — a to jest najtańszy rodzaj modów, jaki ta gra może mieć |
| G2 ★ | **Walidator gramatyki (`grammar::validate`) jest częścią bramki instalacji moda**, obok walidatora determinizmu z WP12. Mod z nieznanym materiałem, cyklem `Ref`, przekroczonym budżetem węzłów albo złym `schema_version` jest odrzucany przy instalacji, nie przy generacji miasta | Walidator istnieje od M2d i odrzuca dokładnie te cztery klasy błędów. Bez podpięcia go do menedżera modów błąd w pliku moda wyjdzie dopiero przy `generate_city`, czyli po kliknięciu „nowa gra" — a wtedy nie wiadomo, który z zainstalowanych modów go wniósł. Walidator jest deterministyczny i tani (dwanaście plików ładuje się w czasie nieodróżnialnym od zera), więc nie ma powodu go odkładać |
| G3 | `ModManifest.data_schema` obejmuje `GRAMMAR_SCHEMA_VERSION` (dziś `1`), a nie tylko schemat towarów i receptur | `data_schema: u32` jest w manifeście jedną liczbą, a `data/` ma kilka niezależnych schematów. Gramatyka ma własną stałą w `sim/world/src/city/grammar.rs` i własny błąd `GrammarError::SchemaVersion` — jeśli manifest ma odpowiadać na pytanie „czy ten mod pasuje do tej wersji gry", musi ją widzieć |
| G4 | `example-mod/` z kryterium WP11 dokłada **także jedną gramatykę budynku** — piąty artefakt obok zdarzenia, polityki AI, typu zakładu i panelu UI | Przykładowy mod jest dokumentacją wykonywalną: modder kopiuje to, co w nim widzi. Nowy budynek jest najczęstszym pierwszym modem w grze miastotwórczej i jednocześnie jedynym, który nie wymaga ani linii Lua — pokazanie tego w `example-mod/` jest tańsze niż rozdział w dokumentacji. Nie zmienia kryterium „cztery klasy hooków z §18.3 PRD", bo gramatyka nie jest hookiem — jest danymi |

**Czego tu celowo nie ma:** operatorów gramatyki dostępnych dla modów jako skrypt.
Zbiór operatorów jest zamknięty od M2 (ryzyko R4) i modding tego nie otwiera — mod
dokłada **pliki** w istniejącym języku, nie nowe słowa w języku. Gdyby kiedyś miał je
dokładać, jest na to `ScriptHook`, a nie rozszerzalna gramatyka.
