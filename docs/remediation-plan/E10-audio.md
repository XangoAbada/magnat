# E10 — Dźwięk: nagrania i głośność

**Po co.** Warstwa dźwiękowa z M11d (`engine/audio`, WP8) działa, ale gracz nie ma na nią
wpływu i słyszy mało:

- **Głośności nie da się ustawić.** `AudioEngine::set_volumes` (`engine/audio/src/lib.rs:255`)
  nie ma ani jednego wywołania. Klient zawsze startuje z `Volumes::default()`
  (`tools/magnat/src/sound.rs:73`). `Settings` (`game/src/shell.rs:348`) nie ma pól głośności.
- **Zakładka „Dźwięk” nie ma wykonawcy.** M9b `DE-11` i M9e `DF-5` odesłały ją do M11,
  a `ui-design.md` §6.5 wpisywał ją jako „Dźwięk (M11)”. M11 się zamknęło, a zakładka nie
  powstała. Komentarz `ponytail:` w `game/src/screens/settings.rs:8` dalej ją zapowiada.
- **Nie ma ani jednego nagrania.** Cały dźwięk powstaje w `synth.rs` z opisu
  w `data/audio/audio.ron`: 7 łóż dzielnic, 6 źródeł punktowych i 4 stemy muzyki.
  Muzyka to 2–3 czyste tony na nastrój. `kira` jest zbudowana bez dekoderów
  (`engine/audio/Cargo.toml:26`), więc crate nie umie przeczytać pliku.
- **Magistrala `Ui` jest pusta.** `Bus::Ui` nie ma żadnego odtworzenia w kodzie.
  `N9.6` odkłada dźwięk interfejsu, bo „gra nie ma miksera”, ale to nieprawda: mikser jest,
  brakuje tylko próbek i wywołań.

Etap nie pochodzi z audytu. Powstał na prośbę właściciela produktu z 22 września 2026: muzyka
i dźwięki mają być generowane lokalnie w ComfyUI, a w opcjach mają być suwaki głośności.

**Kierunek.** Właściciel produktu wybrał dwie rzeczy:

- **Nagrania z plików, synteza jako zapas.** Każdy wpis w `audio.ron` może wskazać plik `.ogg`.
  Jeśli pliku nie ma albo nie daje się go zdekodować, gra wraca do dzisiejszej syntezy.
  Dzięki temu CI i czyste repozytorium działają bez nagrań.
- **Pięć suwaków:** Główna, Muzyka, Efekty świata, Tło, Interfejs. Główna mnoży pozostałe.

**Wejście.** E1 zamknięty, bo testy `magnat-audio` i `game/tests/screens.rs` muszą biec w CI,
zanim cokolwiek o nich udowodnią. Poza tym **E10 nie zależy od E2–E9** i idzie równolegle
z nimi. Jedna kolizja: `N9.4` przerysowuje ekran ustawień. Jeśli `N10.2` zamknie się pierwszy,
`N9.4` stylizuje cztery zakładki. Jeśli `N9.4` pójdzie pierwszy, stylizuje trzy, a `N10.2`
dokłada czwartą w tym samym stylu.

**Pomiar zamknięcia.** Wszystkie cztery warunki muszą być spełnione:

1. W grze Ustawienia → Dźwięk → Główna = 0 daje ciszę od razu, bez restartu.
2. Po restarcie wartości zostają.
3. Stary `settings.ron` bez nowych pól wczytuje się z wartościami domyślnymi.
4. Liczba wpisów `audio.ron` z plikiem i bez pliku, rozmiar `data/audio/` w MB i długość
   pętli muzyki w sekundach są wpisane tutaj.

## Punkty

- [ ] **N10.1** głośność w profilu gracza — `nowe`
  - W `Settings` dochodzi pięć pól `u8` w procentach: `vol_master`, `vol_music`,
    `vol_world`, `vol_ambient`, `vol_ui`.
    - Typ `u8`, a nie `f32`, bo `Settings` ma `Eq`.
    - Wartości domyślne 100/50/90/80/100 to dzisiejsze `Volumes::default()` plus Główna.
  - *Naprawa:* `#[serde(default)]` na strukturze. Bez tego każdy istniejący `settings.ron`
    przestaje się wczytywać, a `Settings::load` zwraca błąd zamiast profilu.
  - *Test:* `settings.ron` w dzisiejszym kształcie (bez pól głośności) wczytuje się
    z domyślnymi. Na kodzie po dodaniu pól, ale bez `serde(default)`, pada.
    Literał w `game/tests/screens.rs:193` dostaje `..Settings::default()`.

- [ ] **N10.2** zakładka „Dźwięk” — `nowe`, `ui-design.md` §6.5
  - `SettingsTab::Audio` i trzecia zakładka w `game/src/screens/settings.rs`.
    - Pięć wierszy przez istniejący `segment_row` (`screens/controls.rs:99`).
    - Wartości `[0, 10, …, 100]`, etykieta `"{v}%"`.
    - Własna stała `WIERSZE_DZWIEKU = 5` obok `WIERSZE_GRY`.
  - Nowe klucze w `pl.ron` i `en.ron` w tej samej zmianie: `ui.settings.tab.audio`
    i `ui.settings.vol.{master,music,world,ambient,ui}`.
  - Komentarz `ponytail:` o dwóch zakładkach (`settings.rs:8-10`) znika. `ui-design.md` §6.5
    wskazuje już na `N10.2` (poprawione przy wpisaniu E10).
  - *Test:* nawigacja klawiaturą po zakładce (strzałki zmieniają wartość na wierszu pod
    kursorem, zmiana zwraca `ShellAction::SettingsChanged`). Test zgodności kluczy pl/en.

- [ ] **N10.3** podpięcie do silnika — `nowe`
  - W `tools/magnat/src/sound.rs` dochodzi `pub(crate) fn glosnosci(&Settings) -> Volumes`.
    Mnoży każdą magistralę przez Główną w kolejności `Bus` (Ambient, World, Ui, Music).
  - `uruchom(no_audio, &Settings)` zastępuje `Volumes::default()`. Wywołanie
    w `tools/magnat/src/main.rs:272` dostaje ustawienia wczytane wyżej.
  - `ShellAction::SettingsChanged` (`tools/magnat/src/session.rs:331`) woła
    `set_volumes(glosnosci(..))`. Zmiana działa od razu, tak jak język.
  - *Test:* Główna 0 daje ciszę na wszystkich magistralach, a pole `vol_music` trafia do
    `Volumes.0[Bus::Music as usize]`. Dziś tego testu nie da się napisać, bo `glosnosci`
    nie istnieje. Ręcznie: suwak słychać bez restartu.

- [ ] **N10.4** pliki zamiast syntezy — `nowe`
  - W `engine/audio/Cargo.toml` włączamy cechę `ogg` w `kira` (Vorbis przez `symphonia`).
    Komentarz „bez dekoderów” zmienia się na powód, dla którego dekoder jest.
  - W `catalog.rs` `Voice` dostaje `#[serde(default)] file: Option<String>` ze ścieżką
    względną do `data/audio/`. `validate` nie wymaga warstw syntezy przy wpisie z plikiem.
    `schema_version` rośnie do 2.
  - *Naprawa:* w `lib.rs` powstaje jedna funkcja `probka(v, sr, loop_s, seed)`.
    - Jeśli plik jest, woła `StaticSoundData::from_file`.
    - W przeciwnym razie woła dzisiejsze `sound(&synth::render_voice(..))`.
    - Brak pliku albo błąd dekodowania to `eprintln!` i powrót do syntezy, a nie błąd.
      To ta sama zasada „cicha gra, nie brak gry”, co przy `AudioError::Device`.
    - Korzystają z niej trzy miejsca, które dziś syntezują (`lib.rs:186-208`).
  - *Test:* mały `.ogg` w `engine/audio/tests/data/` wczytuje się i ma niezerową liczbę
    ramek. Wpis ze wskazanym, ale nieistniejącym plikiem daje syntezę. Test działa bez karty
    dźwiękowej, bo sprawdza samo dekodowanie, bez `AudioManager`.

- [ ] **N10.5** stemy muzyki trzymają takt — `nowe`
  - Crossfade nastrojów trafia w granicę taktu tylko dlatego, że wszystkie stemy grają
    w jednej fazie i mają długość `music.loop_s()`. Nagranie z generatora ma dowolną długość.
  - *Naprawa:* stem z pliku jest po dekodowaniu przycinany albo dopełniany ciszą do dokładnej
    liczby ramek pętli. Źródła punktowe i łoża nie są przycinane, bo grają niezależnie.
  - **Decyzja N10.5-a:** *Domyślnie:* `bars_per_loop` w `audio.ron` rośnie z 4 do 16.
    Przy 84 BPM pętla trwa 45,7 s zamiast 11,4 s. Czterotaktowa pętla nagranej muzyki
    powtarzałaby się co 11 sekund, a 45 sekund mieści się w oknie ACE-Step.
  - *Test:* stem dłuższy i krótszy od pętli kończą z tą samą liczbą ramek co
    `loop_s() * sample_rate`. Istniejący test `katalog_z_repozytorium_sie_wczytuje`
    (`catalog.rs`, całkowita liczba taktów) przechodzi dla 16.

- [ ] **N10.6** nagrania z ComfyUI — `nowe`
  - **Decyzja N10.6-a:** *Domyślnie:* modele lokalne z natywną obsługą w ComfyUI.

    | Model | Do czego | Licencja | Pamięć GPU |
    |---|---|---|---|
    | ACE-Step 1.5 | 4 stemy muzyki, instrumentalne | Apache-2.0, bez ograniczeń komercyjnych | poniżej 4 GB wariant lekki, 12+ GB pełny |
    | Stable Audio Open 1.0 | 7 łóż dzielnic, 6 źródeł punktowych, dźwięki UI | Stability Community License, darmowa do 1 mln USD przychodu rocznie | ok. 8 GB |

    Stable Audio 3.0 też ma obsługę w ComfyUI, ale jego licencji nie sprawdzono.
    Przed użyciem trzeba ją przeczytać i wpisać tutaj.
  - *Przebieg:*
    1. Przez `comfy-mcp`: `server_info`, `search_templates`, `fetch_template`,
       sprawdzenie `local_check`, `set_workflow_slot` (prompt, długość, seed), `run_workflow`.
    2. Prompty:
       - muzyka: instrumental, 84 BPM, wspólna tonacja G, nastrój według `MusicMood`
         (rozwój jasny dur, stabilność spokojnie, napięcie dysonans, kryzys nisko i z pulsem),
         długość 45,7 s;
       - łoża: pętla 4–8 s na dzielnicę;
       - źródła: długość według `loop_s` z katalogu.
    3. ComfyUI zapisuje FLAC. Konwertujemy przez `ffmpeg -c:a libvorbis -q:a 4` do
       `data/audio/{music,beds,sources,ui}/*.ogg`.
       - Ogg, nie MP3, bo koder MP3 dokleja ciszę na początku i pętla przestaje być bezszwowa.
       - Ogg jest też ok. 10× mniejszy od FLAC.
  - Prompt, seed i model każdego pliku idą jako komentarz przy jego wpisie `file` w `audio.ron`.
    Dzięki temu generację da się powtórzyć. `ponytail:` bez skryptu generującego, bo generacja
    jest jednorazowa. Skrypt powstaje, gdy trzeba ją powtórzyć drugi raz.
  - *Test:* odsłuch właściciela produktu, a w CI wczytanie katalogu z wszystkimi plikami
    (`AudioCatalog::load_default` i `probka` bez powrotu do syntezy dla żadnego wpisu z plikiem).
  - Osobny commit z samymi nagraniami i komentarzami w `audio.ron`, po `N10.4`.

- [ ] **N10.7** dźwięk interfejsu — `nowe`, `ui-design.md` §8, przejmuje `N9.6`
  - **Decyzja N10.7-a:** *Domyślnie:* trzy dźwięki: klik, potwierdzenie i odmowa.
    Grają na magistrali `Ui` przez `AudioEngine::play_ui(UiSound)`. `Shell` zwraca, co
    zabrzmiało, a klient to odtwarza, bo `game` nie zależy od `magnat-audio` i tak zostaje.
    Wolno przyjąć wariant „bez dźwięków UI”, wtedy punkt dostaje `[-]` i adres.
  - *Test:* wybór pozycji menu i zmiana wartości w `segment_row` zwracają zdarzenie dźwięku.
    Suwak Interfejs = 0 wycisza je bez ruszania pozostałych magistral.

## Znalezione po drodze
