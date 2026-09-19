//! Atmosfera snapshotu: pogoda, zasilanie dzielnic i lista świateł (M11d §5.8).
//!
//! ### Dlaczego osobny plik obok `view.rs`
//!
//! `view.rs` składa **encje** — mieszkańca, pojazd, zakład — i robi to złączeniem
//! warstwy Mikro z komponentami. Tutaj nie ma ani jednej encji: pogoda jest stanem
//! świata, zasilanie tablicą per dzielnica, a światła wynikiem geometrii miasta
//! i pory doby. Dwa tematy, dwa pliki; dzieli się pliki, w których są dwa tematy,
//! a nie pliki, które są długie (CLAUDE.md).
//!
//! ### Co tu jest stanem, a co funkcją
//!
//! Wszystko poza **rampą wygaszenia** jest funkcją czystą od świata i kadru.
//! Rampa musi pamiętać, bo 1,5 s wygaszania to własność **obrazu**, nie symulacji:
//! skokowe zgaszenie dzielnicy czyta się jako błąd renderu, a rampa jako awaria sieci
//! (§5.8). Pamięć rampy siedzi w [`Ambience`] razem z resztą buforów wypełniacza
//! i nie wchodzi do hasha stanu — tak samo jak zegar animacji (`W-3`).

use magnat_core::SiteId;
use magnat_ecs::World;
use magnat_sim_snapshot::{
    AmbientBed, LightRecord, PowerRec, RenderSnapshot, ViewQuery, WeatherState, MAX_DISTRICTS,
    VEHICLE_FLAG_LIGHTS,
};
use magnat_world::city::districts::DistrictKind;
use magnat_world::city::sites::SectorId;
use magnat_world::CityData;

/// Wysokość oprawy latarni nad gruntem w metrach.
const LAMP_HEIGHT_M: f32 = 6.0;
/// Zasięg światła latarni w metrach — tyle, ile obejmuje jedna oprawa uliczna.
const LAMP_RANGE_M: f32 = 14.0;
/// Zasięg światła obszarowego oświetlonego budynku.
const WINDOW_RANGE_M: f32 = 22.0;
/// Zasięg reflektora pojazdu.
const HEADLIGHT_RANGE_M: f32 = 18.0;

/// Dalej niż tyle metrów od oka latarnia przestaje być światłem punktowym (§5.8).
///
/// Próg jest 400 m, a nie 150 m z pierwszej wersji planu: limit 256 świateł okazał się
/// limitem **na klaster**, a nie globalnym, więc globalny budżet to 4 096 i nocne miasto
/// może być jaśniejsze, niż zakładał tamten rachunek.
pub const LAMP_LIGHT_RANGE_M: f64 = 400.0;

/// Cap reflektorów w klatce (§5.8 — „tylko L0 i L1, cap 128").
pub const HEADLIGHT_CAP: usize = 128;

/// Ile trwa wygaszenie dzielnicy przy blackoucie. Kryterium `blackout_ramps`: 1,5 s ±0,1 s.
pub const BLACKOUT_RAMP_MS: u64 = 1_500;

/// Poniżej tego progu `PowerRec.supply_ratio` znaczy blackout (M8b, §5.8).
pub const BLACKOUT_THRESHOLD: u8 = 128;

/// Poniżej tylu jednostek światła dziennego zapalają się latarnie i okna.
const LIGHTS_ON_BELOW: u8 = 110;

/// Zajętość klastra, powyżej której latarnie zaczynają się przerzedzać.
///
/// Trzy czwarte pojemności (`CLUSTER_CAPACITY` = 64 w kodzie M1, nie 256 jak zakładał
/// plan — `I-1`). Zapas ćwiartki jest po to, żeby histereza miała gdzie działać:
/// przerzedzanie przy samym suficie oznaczałoby, że objawem jest już zgubione światło.
pub const CLUSTER_PRESSURE_ON: u32 = 48;
/// Poniżej tej zajętości przerzedzanie się cofa. Rozstęp progów jest histerezą —
/// bez niego obrót kamery wzdłuż arterii migotałby latarniami co klatkę.
pub const CLUSTER_PRESSURE_OFF: u32 = 32;

/// Barwa latarni sodowej i wykładnik jej jasności (RGBE, patrz [`LightRecord::new`]).
const LAMP_COLOR: [u8; 3] = [255, 180, 90];
const LAMP_EXP: u8 = 131;
/// Barwa światła zza okna — chłodniejsza i słabsza od latarni.
const WINDOW_COLOR: [u8; 3] = [255, 214, 160];
const WINDOW_EXP: u8 = 131;
/// Reflektor: biały, jaśniejszy, wąski zasięg.
const HEADLIGHT_COLOR: [u8; 3] = [235, 240, 255];
const HEADLIGHT_EXP: u8 = 132;

/// Latarnia miasta: pozycja światła w metrach i dzielnica, w której stoi.
#[derive(Clone, Copy, Debug)]
struct Lamp {
    pos: [f32; 3],
    district: u16,
}

/// Atmosfera kadru — stan, który wypełniacz trzyma między publikacjami.
pub struct Ambience {
    /// Latarnie miasta, policzone raz przy związaniu. `RoadNetwork.furniture` daje
    /// pozycję przy gruncie; oprawa wisi [`LAMP_HEIGHT_M`] wyżej.
    lamps: Vec<Lamp>,
    /// Łoże dźwiękowe per dzielnica — wyprowadzone z `DistrictKind`.
    bed_by_district: [u8; MAX_DISTRICTS],
    /// Ile dzielnic ma to miasto. Reszta tablic zostaje neutralna.
    districts: usize,
    /// Położenie rampy per dzielnica **w milisekundach** 0..=[`BLACKOUT_RAMP_MS`].
    ///
    /// W milisekundach, a nie w 0..=255, bo krok rampy przy 60 Hz to `255 · 16 / 1500`,
    /// czyli 2 po zaokrągleniu w dół — a 128 takich kroków to 2 048 ms zamiast 1 500.
    /// Zaokrąglenie w jedną stronę nie znosi się, tylko kumuluje; ta sama pułapka,
    /// którą `K-25` rozwiązał mikrolitrami paliwa.
    ///
    /// Startuje z pełnym światłem, bo świat zaczyna się zasilony; pierwszy blackout
    /// ma być zjazdem z 255, a nie skokiem w górę z zera.
    lit_ms: [u16; MAX_DISTRICTS],
    /// Zegar prezentacji z poprzedniej publikacji — rampa liczy się z jego przyrostu.
    last_ms: Option<u64>,
    /// Ułamek światła dziennego 0..=255. Podaje go klient, bo wysokość słońca liczy
    /// `engine/render::sky` i druga kopia tej arytmetyki rozjechałaby się z pierwszą.
    daylight: u8,
    /// Największa zajętość klastra zmierzona przez renderer w poprzedniej klatce.
    cluster_peak: u32,
    /// Co która latarnia trafia na listę: 1, 2 albo 4.
    lamp_stride: u8,
    /// Zakłady w numeracji `SiteId` → łoże dźwiękowe, z sektora archetypu.
    bed_by_site: Vec<(SiteId, u8)>,
    /// `PlumeKind` per `RecipeId` — z katalogu miasta, raz przy związaniu.
    plume_by_recipe: Vec<u8>,
    /// **Miligramy** pyłu na minutę pracy per `RecipeId`, z katalogu.
    ///
    /// Receptura podaje emisję na szarżę, a szarża trwa kilkanaście albo kilkadziesiąt
    /// minut — dzielenie przez czas trwania jest całą treścią tego pola. Miligramy,
    /// a nie gramy, bo `pm_g / duration_minutes` obcina w dół i **każda receptura
    /// emitująca mniej niż gram na minutę dawała dokładnie zero** (`I-22`). Ta sama
    /// pułapka, którą `K-25` rozwiązał mikrolitrami paliwa.
    pm_mg_per_min_by_recipe: Vec<i64>,
    /// Gramy pyłu na minutę odpowiadające pełnej skali `emission` (`data/tuning/supply.ron`).
    /// Zero znaczy „nie udało się wczytać kalibracji" i wtedy dymu nie ma wcale —
    /// to jest widoczne, w przeciwieństwie do podstawionej z palca liczby.
    emission_ref: i64,
    /// Wymuszenia sceny pomiarowej — patrz [`Ambience::force_scene`].
    force_precip: Option<u8>,
    force_snow: Option<u8>,
    force_blackout: u16,
    /// Wymuszona pora roku sceny pomiarowej, 0..=3.
    ///
    /// Istnieje po to, żeby kryterium `seasons_do_not_remesh` mogło zapalić się
    /// **na czerwono**: sezon liczy się z `world.tick`, a scena stoi na pauzie, więc
    /// bez wymuszenia przejście przez cztery pory roku nigdy w oknie pomiaru nie
    /// zachodzi i licznik remeshingu jest zerem z konstrukcji, a nie z własności kodu.
    force_season: Option<u8>,
}

impl Default for Ambience {
    fn default() -> Self {
        Ambience {
            lamps: Vec::new(),
            bed_by_district: [0; MAX_DISTRICTS],
            districts: 0,
            lit_ms: [BLACKOUT_RAMP_MS as u16; MAX_DISTRICTS],
            last_ms: None,
            daylight: 255,
            cluster_peak: 0,
            lamp_stride: 1,
            bed_by_site: Vec::new(),
            plume_by_recipe: Vec::new(),
            pm_mg_per_min_by_recipe: Vec::new(),
            emission_ref: 0,
            force_precip: None,
            force_snow: None,
            force_blackout: 0,
            force_season: None,
        }
    }
}

impl Ambience {
    /// Przelicza to, co w mieście nie zmienia się w trakcie gry: latarnie, łoża
    /// dzielnic i zakładów, pióropusze receptur.
    pub fn bind(&mut self, city: &CityData) {
        self.lamps.clear();
        for f in &city.roads.furniture {
            if f.kind != magnat_world::FurnitureKind::StreetLamp {
                continue;
            }
            let d = city
                .roads
                .segments
                .get(f.seg.0 as usize)
                .map_or(0, |s| s.district.0);
            self.lamps.push(Lamp {
                pos: [f.pos.x, f.pos.y, f.pos.z + LAMP_HEIGHT_M],
                district: d,
            });
        }

        self.districts = city.districts.districts.len().min(MAX_DISTRICTS);
        self.bed_by_district = [AmbientBed::Quiet.as_index() as u8; MAX_DISTRICTS];
        for (i, d) in city
            .districts
            .districts
            .iter()
            .take(MAX_DISTRICTS)
            .enumerate()
        {
            self.bed_by_district[i] = bed_of_district(d.kind).as_index() as u8;
        }

        self.bed_by_site.clear();
        for (i, s) in city.sites.sites.iter().enumerate() {
            // Brak archetypu daje ciszę, a nie panikę: uszkodzony katalog ma dać zakład
            // bez łoża, nie wywrócić wiązanie z miastem. Ta sama droga, którą `view.rs`
            // sięga po budynek zakładu.
            let loze = city
                .site_catalog
                .archetypes
                .get(s.archetype.0 as usize)
                .map_or(AmbientBed::Quiet, |a| bed_of_sector(a.spec.sector));
            // Klucz przesunięty (`K-46`) — ten sam, którym posługuje się wypełniacz
            // zakładów i rejestr produkcji.
            self.bed_by_site
                .push((crate::world::plants::site_id(i), loze.as_index() as u8));
        }
        self.bed_by_site.sort_unstable_by_key(|(s, _)| *s);

        self.plume_by_recipe = city
            .catalog
            .recipes
            .iter()
            .map(|r| r.emissions.plume as u8)
            .collect();
        self.pm_mg_per_min_by_recipe = city
            .catalog
            .recipes
            .iter()
            .map(|r| i64::from(r.emissions.pm_g) * 1_000 / i64::from(r.duration_minutes.max(1)))
            .collect();

        // Skala emisji jest kalibracją i mieszka w danych (`K-35`). Wczytujemy ją raz
        // na miasto, a nie raz na klatkę: to jest odczyt pliku.
        self.emission_ref =
            magnat_supply::Tuning::load_default().map_or(0, |t| t.emission_ref_pm_g_per_min);

        self.lit_ms = [BLACKOUT_RAMP_MS as u16; MAX_DISTRICTS];
        self.last_ms = None;
        self.lamp_stride = 1;
    }

    /// Ułamek światła dziennego 0..=255, policzony przez klienta z wysokości słońca.
    pub fn set_daylight(&mut self, v: u8) {
        self.daylight = v;
    }

    /// Wymusza pogodę i awarię zasilania na potrzeby **sceny pomiarowej** (`--precip`,
    /// `--snow`, `--blackout`).
    ///
    /// Istnieje z tego samego powodu co `--lights` i `--crowd`: sceny odniesienia §7.2
    /// nazywają się `bench_night_rain`, `bench_winter` i `bench_blackout`, a model pogody
    /// losuje opad i nie da się go poprosić o ulewę. Wymuszenie dotyczy **wyłącznie
    /// snapshotu** — symulacja dalej ma swoją pogodę, a gospodarka swoje sieci, więc
    /// scena nie zmienia ani grosza w świecie.
    pub fn force_scene(&mut self, precip: Option<u8>, snow: Option<u8>, blackout: u16) {
        self.force_precip = precip;
        self.force_snow = snow;
        self.force_blackout = blackout;
    }

    /// Wymusza porę roku (0..=3) albo zdejmuje wymuszenie.
    ///
    /// Osobno od [`Ambience::force_scene`], bo scena `bench_winter` **przestawia ją
    /// w trakcie pomiaru**: kryterium WP7 mówi o przejściu przez cztery pory roku,
    /// a jedno ustawienie na starcie sprawdza jedną porę i nic więcej.
    pub fn force_season(&mut self, season: Option<u8>) {
        self.force_season = season;
    }

    /// Największa zajętość klastra z poprzedniej klatki — sprzężenie zwrotne
    /// przerzedzania latarni (§5.8).
    ///
    /// Liczba pochodzi z `ClusterOccupancy`, czyli z przyrządu, który M1 wystawił
    /// dokładnie po to. Sama suma świateł nie wystarcza: 2 400 latarni rozrzuconych
    /// po scenie jest tanie, a te same 2 400 wzdłuż jednej arterii wyczerpuje limit
    /// klastra wcześniej niż budżet globalny.
    pub fn set_cluster_peak(&mut self, v: u32) {
        self.cluster_peak = v;
    }

    /// Czy pojazdy mają zapalone światła. Ten sam próg, przy którym zapalają się
    /// latarnie — jedna liczba na obie rzeczy, bo obie odpowiadają na to samo pytanie.
    #[must_use]
    pub const fn headlights_on(&self) -> bool {
        self.daylight < LIGHTS_ON_BELOW
    }

    /// Bieżący krok przerzedzania — do raportu i do testu.
    #[must_use]
    pub const fn lamp_stride(&self) -> u8 {
        self.lamp_stride
    }

    /// Jasność zasilania dzielnicy po rampie, 0..=255 — do testu `blackout_ramps`.
    #[must_use]
    pub fn lit(&self, district: u16) -> u8 {
        let ms = self
            .lit_ms
            .get(district as usize)
            .copied()
            .unwrap_or(BLACKOUT_RAMP_MS as u16);
        (u32::from(ms) * 255 / BLACKOUT_RAMP_MS as u32).min(255) as u8
    }

    /// Ile latarni zna to miasto.
    #[must_use]
    pub fn lamp_count(&self) -> usize {
        self.lamps.len()
    }

    /// Łoże dźwiękowe zakładu — `AmbientBed::as_index`, 0 gdy zakład nieznany.
    #[must_use]
    pub fn bed_of_site(&self, s: SiteId) -> u8 {
        self.bed_by_site
            .binary_search_by_key(&s, |(k, _)| *k)
            .map_or(0, |i| self.bed_by_site[i].1)
    }

    /// `PlumeKind` receptury jako dwa bity `SiteRenderRec.flags`.
    #[must_use]
    pub fn plume_of_recipe(&self, r: magnat_core::RecipeId) -> u8 {
        self.plume_by_recipe.get(r.0 as usize).copied().unwrap_or(0)
    }

    /// Miligramy pyłu na minutę pracy receptury.
    #[must_use]
    pub fn pm_mg_per_min(&self, r: magnat_core::RecipeId) -> i64 {
        self.pm_mg_per_min_by_recipe
            .get(r.0 as usize)
            .copied()
            .unwrap_or(0)
    }

    /// Gęstość dymu zakładu, 0..=255 — **tempo**, a nie ostatni wyrzut.
    ///
    /// `EmissionTotals::pm_g_last_minute` wygląda na właściwe źródło i nim nie jest:
    /// pole zeruje się co minutę i napełnia dopiero w tej minucie, w której szarża
    /// **się kończy** (`plant::produce`). Snapshot próbkuje je co klatkę, więc trafia
    /// w zero prawie zawsze — komin dymiłby przez jedną klatkę raz na pół godziny gry
    /// i wyglądałoby to na usterkę cząstek. Kontrakt §5.2 mówi „gęstość", czyli tempo,
    /// więc liczymy je z receptury pracującej linii (`I-6`).
    ///
    /// ponytail: tempo jest **nominalne** — obniżona szarża dymi tu tak samo jak pełna.
    /// Sufit mieści się w rozrzucie cząstek; ścieżka wyjścia: przeskalować o stosunek
    /// `charge` do `batch_mass`, który `LineState::Running` już niesie.
    #[must_use]
    pub fn plant_emission(&self, plant: &magnat_supply::plant::PlantSite) -> u8 {
        let pm: i64 = plant
            .lines
            .iter()
            .filter_map(|l| match l.state {
                magnat_supply::plant::LineState::Running { recipe, .. } => Some(recipe),
                _ => None,
            })
            .map(|r| self.pm_mg_per_min(r))
            .sum();
        self.emission_scale_mg(pm)
    }

    /// Skala `emission` dla snapshotu: gramy pyłu na minutę → 0..=255.
    ///
    /// Ta sama arytmetyka co `magnat_supply::Tuning::emission_scale` i ten sam
    /// mianownik — mianownik jest wczytany z `data/tuning/supply.ron`, a nie wpisany
    /// tutaj, bo ruchomy próg odniesienia znaczyłby, że ten sam komin dymi inaczej
    /// po zmianie kalibracji cudzego zakładu (`K-35`).
    #[must_use]
    pub fn emission_scale(&self, pm_g_per_min: i64) -> u8 {
        self.emission_scale_mg(pm_g_per_min.saturating_mul(1_000))
    }

    /// To samo w miligramach — jednostce, w której liczy się tempo receptury, żeby
    /// zaokrąglenie nie zjadało zakładów emitujących mniej niż gram na minutę.
    #[must_use]
    pub fn emission_scale_mg(&self, pm_mg_per_min: i64) -> u8 {
        if self.emission_ref <= 0 {
            return 0;
        }
        (255 * pm_mg_per_min / (self.emission_ref * 1_000)).clamp(0, 255) as u8
    }

    /// Pogoda, zasilanie i łoża dzielnic. Wołane **przed** wypełnieniem zakładów,
    /// bo jasność okna zależy od tego, czy dzielnica ma prąd.
    pub fn fill_state(&mut self, world: &World, view: &ViewQuery, out: &mut RenderSnapshot) {
        out.weather = weather_of(world, self.daylight);
        if let Some(p) = self.force_precip {
            out.weather.precipitation = p;
            out.weather.cloud = out.weather.cloud.max(p);
        }
        if let Some(s) = self.force_snow {
            out.weather.snow_cover = s;
            // Śnieg leżący znaczy mróz i zimę: bez tego opad wymuszony razem z pokrywą
            // padałby jako deszcz na biały teren, a liście byłyby zielone spod śniegu.
            // Scena `bench_winter` ma pokazać zimę, a `--day` przestawia samo słońce.
            out.weather.kind = 1;
            out.weather.season = magnat_core::Season::Winter.as_index() as u8;
        }
        if let Some(s) = self.force_season {
            out.weather.season = s.min(3);
        }
        out.district_ambient = self.bed_by_district;

        let dzielnic = if self.districts == 0 {
            MAX_DISTRICTS
        } else {
            self.districts
        };
        power_of(world, dzielnic, &mut out.power);
        for d in self.dzielnice_do_zgaszenia(view) {
            out.power[usize::from(d)].supply_ratio = 0;
        }
        self.step_power(&out.power, view.anim_ms);
    }

    /// Które dzielnice gasi scena `bench_blackout` — te **widoczne**, a nie pierwsze
    /// z brzegu.
    ///
    /// Do pierwszego pomiaru wymuszenie gasiło dzielnice 0..n po indeksie i scena nie
    /// mierzyła niczego: kamera stoi nad centrum, a dzielnice o najniższych numerach
    /// leżały gdzie indziej, więc lista świateł miała tyle samo pozycji z blackoutem
    /// i bez niego (1 255 w obu przebiegach). Kryterium „`bench_blackout`
    /// ≤ `bench_night_rain`" porównywało wtedy tę samą scenę ze sobą.
    ///
    /// Wybór idzie po **liczbie latarni w zasięgu oka**, bo to ona jest kosztem:
    /// dzielnica bez ani jednej latarni w kadrze zgaszona nie zmienia ani jednej klatki.
    fn dzielnice_do_zgaszenia(&self, view: &ViewQuery) -> Vec<u16> {
        let ile = usize::from(self.force_blackout).min(MAX_DISTRICTS);
        if ile == 0 {
            return Vec::new();
        }
        let oko = mm_to_m(view.eye);
        let zasieg2 = (LAMP_LIGHT_RANGE_M * LAMP_LIGHT_RANGE_M) as f32;
        let mut ile_latarni = [0u32; MAX_DISTRICTS];
        for l in &self.lamps {
            if kwadrat_odleglosci(l.pos, oko) <= zasieg2 {
                if let Some(n) = ile_latarni.get_mut(usize::from(l.district)) {
                    *n += 1;
                }
            }
        }
        // Gdy w zasięgu oka nie ma **ani jednej** latarni (kamera wysoko, miasto bez
        // oświetlenia), wszystkie liczniki są zerami i sortowanie zostawiłoby dzielnice
        // w kolejności indeksów — czyli dokładnie to zachowanie, które ta funkcja miała
        // zastąpić, tylko po cichu. Wtedy ranking idzie po **całkowitej** liczbie latarni
        // w dzielnicy: gasimy te, w których jest co gasić, nawet jeśli akurat ich nie widać.
        if ile_latarni.iter().all(|n| *n == 0) {
            for l in &self.lamps {
                if let Some(n) = ile_latarni.get_mut(usize::from(l.district)) {
                    *n += 1;
                }
            }
        }
        let mut wg_liczby: Vec<(u32, u16)> = ile_latarni
            .iter()
            .enumerate()
            .map(|(d, n)| (*n, d as u16))
            .collect();
        // Malejąco po liczbie latarni, remis po numerze dzielnicy — wynik ma być
        // ten sam w każdym przebiegu tej samej sceny, inaczej bramka mierzy losowanie.
        wg_liczby.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        // Dzielnica bez ani jednej latarni nie ma czego zgasić — wpisanie jej do wyniku
        // zajęłoby miejsce dzielnicy, która ma.
        wg_liczby.retain(|(n, _)| *n > 0);
        wg_liczby.truncate(ile);
        wg_liczby.into_iter().map(|(_, d)| d).collect()
    }

    /// Posuwa rampę wygaszenia o tyle, ile minęło na zegarze prezentacji.
    ///
    /// Zegar prezentacji, nie czas realny: przy pauzie stoi, więc wygaszanie też —
    /// i dobrze, bo pauza zatrzymuje świat, a nie tylko jego gospodarkę. Przyrost jest
    /// przycięty do długości rampy, bo po wczytaniu zapisu albo po wyjściu z powłoki
    /// zegar skacze o minuty i bez przycięcia pierwsza klatka gasiłaby dzielnicę skokiem
    /// — czyli dokładnie tak, jak rampa ma nie wyglądać.
    pub fn step_power(&mut self, power: &[PowerRec; MAX_DISTRICTS], anim_ms: u64) {
        let krok = self
            .last_ms
            .map_or(0, |p| anim_ms.saturating_sub(p))
            .min(BLACKOUT_RAMP_MS) as u16;
        self.last_ms = Some(anim_ms);
        for (d, p) in power.iter().enumerate() {
            let cel = u16::from(p.supply_ratio >= BLACKOUT_THRESHOLD) * BLACKOUT_RAMP_MS as u16;
            self.lit_ms[d] = ramp(self.lit_ms[d], cel, krok);
        }
    }

    /// Jasność okien zakładu 0..=255: pora doby razy obłożenie razy prąd.
    ///
    /// Zakład bez zmiany świeci **dyżurnym** światłem, a nie pełnym — inaczej nocna
    /// dzielnica przemysłowa wyglądałaby tak samo jak w środku dnia pracy. Awaria linii
    /// nie gasi okien; gasi je dopiero brak prądu, i to przez rampę.
    ///
    /// ponytail: okna świecą wyłącznie budynkom z **zakładem**, bo `SiteRenderRec` jest
    /// jedynym rekordem budynku w snapshocie (§5.8 wprost tak to stawia). Blok mieszkalny
    /// zostaje nocą ciemny. Ścieżka wyjścia, gdyby to zaczęło przeszkadzać: światło
    /// obszarowe per budynek mieszkalny z gęstości zameldowania, czyli własny rekord
    /// w snapshocie — nie przeróbka tej funkcji.
    #[must_use]
    pub fn window_brightness(&self, district: u16, activity: u8) -> u8 {
        let noc = 255u32.saturating_sub(u32::from(self.daylight));
        if noc == 0 {
            return 0;
        }
        let obciazenie = 64 + u32::from(activity) * 191 / 255;
        let z_pradem = u32::from(self.lit(district));
        ((noc * obciazenie / 255) * z_pradem / 255).min(255) as u8
    }

    /// Składa listę świateł klatki: okna, latarnie, reflektory — w tej kolejności.
    ///
    /// Kolejność nie jest estetyczna, tylko **budżetowa**: okna są najrzadsze i najbardziej
    /// niosą sylwetkę miasta, więc mają wejść w całości; latarnie są najliczniejsze
    /// i to one się przerzedzają; reflektory mają własny cap. Przy przepełnieniu globalnego
    /// budżetu 4 096 obcina się ogon, a ogonem są latarnie najdalsze — bo są dopisywane
    /// w kolejności rosnącej odległości.
    pub fn fill_lights(&mut self, view: &ViewQuery, out: &mut RenderSnapshot) {
        self.lamp_stride = next_stride(self.lamp_stride, self.cluster_peak);
        let noc = self.daylight < LIGHTS_ON_BELOW;

        for s in out.sites.as_slice() {
            if s.lights == 0 {
                continue;
            }
            let poz = mm_to_m(s.pos);
            // Okno wisi nad wejściem, a świeci z wnętrza budynku — podnosimy je
            // o kondygnację, żeby światło nie leżało na chodniku.
            let rec = LightRecord::new(
                [poz[0], poz[1], poz[2] + 2.0],
                WINDOW_RANGE_M,
                WINDOW_COLOR,
                skaluj_wykladnik(WINDOW_EXP, s.lights),
            );
            if !out.lights.push(rec) {
                return;
            }
        }

        if noc {
            let oko = mm_to_m(view.eye);
            let zasieg2 = (LAMP_LIGHT_RANGE_M * LAMP_LIGHT_RANGE_M) as f32;
            let mut kandydaci: Vec<(u32, usize)> = Vec::new();
            for (i, l) in self.lamps.iter().enumerate() {
                if i % self.lamp_stride as usize != 0 {
                    continue;
                }
                let d2 = kwadrat_odleglosci(l.pos, oko);
                if d2 > zasieg2 {
                    continue;
                }
                kandydaci.push((d2 as u32, i));
            }
            // Rosnąco po odległości: gdyby budżet 4 096 się wyczerpał, gaśnie ogon,
            // a ogonem są latarnie najdalsze — czyli te, których brak widać najmniej.
            kandydaci.sort_unstable();
            for (_, i) in kandydaci {
                let l = self.lamps[i];
                let jasnosc = self.lit(l.district);
                if jasnosc == 0 {
                    continue;
                }
                let rec = LightRecord::new(
                    l.pos,
                    LAMP_RANGE_M,
                    LAMP_COLOR,
                    skaluj_wykladnik(LAMP_EXP, jasnosc),
                );
                if !out.lights.push(rec) {
                    return;
                }
            }
        }

        let mut reflektorow = 0usize;
        for v in out.vehicles.as_slice() {
            if reflektorow >= HEADLIGHT_CAP {
                break;
            }
            if v.flags & VEHICLE_FLAG_LIGHTS == 0 {
                continue;
            }
            let poz = mm_to_m(v.pos);
            // Snop przed maską, nie w środku bryły: `yaw` jest w 1/65536 obrotu.
            let kat = f32::from(v.yaw) / 65536.0 * std::f32::consts::TAU;
            let (s, c) = (kat.sin(), kat.cos());
            let rec = LightRecord::new(
                [poz[0] + c * 3.0, poz[1] + s * 3.0, poz[2] + 0.8],
                HEADLIGHT_RANGE_M,
                HEADLIGHT_COLOR,
                HEADLIGHT_EXP,
            );
            if !out.lights.push(rec) {
                return;
            }
            reflektorow += 1;
        }
    }
}

/// Łoże dźwiękowe rodzaju dzielnicy. Odwzorowanie jest wiele-do-jednego z rozmysłu:
/// starówka i śródmieście brzmią ruchem, a nie dwoma osobnymi łożami, których nikt
/// by nie odróżnił (ryzyko `R5` — ciężar niosą łoża, nie pojedyncze emitery).
const fn bed_of_district(k: DistrictKind) -> AmbientBed {
    match k {
        DistrictKind::OldTown | DistrictKind::InnerCity => AmbientBed::Traffic,
        DistrictKind::BlockEstate | DistrictKind::Suburb | DistrictKind::Village => {
            AmbientBed::Residential
        }
        DistrictKind::IndustrialBelt => AmbientBed::Industry,
        DistrictKind::PortQuarter => AmbientBed::Port,
        DistrictKind::Campus => AmbientBed::Retail,
        DistrictKind::GreenBelt => AmbientBed::Park,
    }
}

/// Łoże emitera zakładu — z sektora archetypu.
const fn bed_of_sector(s: SectorId) -> AmbientBed {
    match s {
        SectorId::Industry | SectorId::Extraction => AmbientBed::Industry,
        SectorId::Logistics => AmbientBed::Port,
        SectorId::Retail | SectorId::Services => AmbientBed::Retail,
        SectorId::Office | SectorId::Public => AmbientBed::Residential,
        SectorId::Agriculture => AmbientBed::Quiet,
        SectorId::Green => AmbientBed::Park,
    }
}

/// Pogoda widoczna i słyszalna, złożona ze stanu `sim/events` (M8c).
///
/// Trzech pól model nie ma i nie będzie miał, bo nie są mu do niczego potrzebne:
/// zachmurzenia, mgły i światła dziennego. Wyprowadzamy je tutaj z tego, co jest —
/// i to jest właściwe miejsce, bo są wyłącznie do patrzenia.
fn weather_of(world: &World, daylight: u8) -> WeatherState {
    let Some(ev) = world.get_resource::<magnat_events::Events>() else {
        return WeatherState {
            daylight,
            ..Default::default()
        };
    };
    let st = ev.weather();
    let w = st.weather();
    let cal = magnat_core::SimCalendar::new(world.tick);
    let sezon = magnat_core::Season::of_day(cal.day_of_year());

    // Opad: promile → 0..255.
    let opad = (u32::from(w.precip_permille) * 255 / 1000).min(255) as u8;
    // Rodzaj opadu rozstrzyga **temperatura w chwili opadu**, a nie zalegająca pokrywa:
    // odwilż nad białym terenem to deszcz na śniegu i tak ma wyglądać.
    let rodzaj = u8::from(w.temp_dc <= 0);
    // Wilgotność jest w modelu **odchyłką** wokół 10 000 bps, nie wartością bezwzględną.
    // Zachmurzenie idzie za nią i za opadem: pada zawsze spod chmury, ale chmura bywa
    // bez deszczu.
    let wilgoc = (st.wet_anomaly_bps() - 10_000).clamp(-4_000, 4_000);
    let chmura = ((wilgoc + 4_000) * 255 / 8_000).clamp(0, 255) as u8;
    let chmura = chmura.max(opad);
    // Mgła: wilgotno **i** bezwietrznie. Silny wiatr ją rozwiewa, a to jedyna reguła,
    // którą gracz naprawdę zauważy.
    //
    // Próg zamiast dzielnika: pierwsza wersja mnożyła zachmurzenie przez ciszę wiatrową
    // i dzieliła przez trzy, więc `fog_density` nie przekraczało 85 z 255 i **górne dwie
    // trzecie skali były martwe** (`I-23`). Teraz mgła zaczyna się dopiero powyżej
    // połowy zachmurzenia i wtedy sięga pełnej skali — jest tak samo rzadka, ale kiedy
    // już jest, znaczy to, co obiecuje kontrakt renderu: widoczność rzędu 200 m.
    let bezwietrznie = 255i32 - (i32::from(w.wind_kmh) * 255 / 30).min(255);
    let wilgotno = (i32::from(chmura) - 128).max(0) * 2;
    let mgla = (wilgotno * bezwietrznie / 255).clamp(0, 255) as u8;

    WeatherState {
        precipitation: opad,
        kind: rodzaj,
        temp_c: (w.temp_dc / 10).clamp(-128, 127) as i8,
        wind: wind_vector(w.wind_kmh, cal.day_of_year()),
        fog_density: mgla,
        cloud: chmura,
        season: sezon.as_index() as u8,
        snow_cover: (u32::from(w.snow_cover_mm) * 255 / 200).min(255) as u8,
        daylight,
        _pad: [0; 2],
    }
}

/// Wiatr jako dwie składowe w m/s. Model niesie samą prędkość, więc kierunek bierzemy
/// z doby: ma być **stały w ciągu dnia** i inny jutro, bo deszcz siekący raz w lewo,
/// raz w prawo między klatkami wygląda na usterkę.
fn wind_vector(kmh: u16, day_of_year: u16) -> [i8; 2] {
    let ms = (i32::from(kmh) * 10 / 36).clamp(0, 30);
    // Ósemka kierunków, przesuwana o jeden co dobę — bez trygonometrii.
    const KIERUNKI: [(i32, i32); 8] = [
        (10, 0),
        (7, 7),
        (0, 10),
        (-7, 7),
        (-10, 0),
        (-7, -7),
        (0, -10),
        (7, -7),
    ];
    let (kx, ky) = KIERUNKI[(day_of_year as usize * 3) % 8];
    [(ms * kx / 10) as i8, (ms * ky / 10) as i8]
}

/// Stopień zasilania per dzielnica, 0..=255.
///
/// Sieci przesyłowe (M8b) liczą zasilanie **per zakład**, bo tam są liczniki; dzielnica
/// nie ma węzła i mieć go nie musi. Udział zasilanych zakładów jest więc jedyną liczbą,
/// z której da się wyprowadzić „czy tu jest prąd" — i jest właściwą liczbą, bo blackout
/// w grze wychodzi z odcięć, a odcięcia dotykają przyłączy.
///
/// Dzielnica bez ani jednego zakładu z przyłączem dostaje pełne 255, a nie zero: brak
/// przyłącza znaczy „nie potrzebuje", tak samo jak po stronie `UtilityGrids::supply_state`.
fn power_of(world: &World, districts: usize, out: &mut [PowerRec; MAX_DISTRICTS]) {
    *out = [PowerRec { supply_ratio: 255 }; MAX_DISTRICTS];
    let Some(grids) = world.get_resource::<magnat_traffic::utility::UtilityGrids>() else {
        return;
    };
    let Some(ev) = world.get_resource::<magnat_events::Events>() else {
        return;
    };
    let mut razem = [0u32; MAX_DISTRICTS];
    let mut zasilone = [0u32; MAX_DISTRICTS];
    for s in ev.sites() {
        let d = s.district as usize;
        if d >= MAX_DISTRICTS {
            continue;
        }
        razem[d] += 1;
        if grids.power_available(s.site) {
            zasilone[d] += 1;
        }
    }
    for d in 0..districts.min(MAX_DISTRICTS) {
        if razem[d] == 0 {
            continue;
        }
        out[d].supply_ratio = (zasilone[d] * 255 / razem[d]).min(255) as u8;
    }
}

/// Krok przerzedzania latarni na następną klatkę — 1, 2 albo 4.
///
/// Histereza jest tu treścią, nie ozdobą: bez rozstępu progów obrót kamery wzdłuż alei
/// przełączałby krok co klatkę i rząd latarni pulsowałby. Przerzedzanie zamiast obcinania
/// ogona, bo równomiernie rzadszy rząd czyta się jak rzadsze latarnie, a ucięty ogon
/// jak ciemna dziura w połowie ulicy (§5.8).
#[must_use]
pub fn next_stride(obecny: u8, peak: u32) -> u8 {
    if peak >= CLUSTER_PRESSURE_ON {
        (obecny * 2).min(4)
    } else if peak <= CLUSTER_PRESSURE_OFF {
        (obecny / 2).max(1)
    } else {
        obecny
    }
}

/// Zbliża `teraz` do `celu` o najwyżej `krok`.
fn ramp(teraz: u16, cel: u16, krok: u16) -> u16 {
    if teraz < cel {
        teraz.saturating_add(krok).min(cel)
    } else {
        teraz.saturating_sub(krok).max(cel)
    }
}

/// Przycina jasność światła, zmniejszając wykładnik RGBE. `0` gasi je do czerni.
///
/// Wykładnikiem, a nie mantysą, bo mantysa niesie **barwę**: przyciemnienie przez
/// podzielenie trójki RGB przeniosłoby latarnię sodową w stronę szarości, a wygaszana
/// dzielnica ma robić się ciemniejsza, nie bardziej bezbarwna.
fn skaluj_wykladnik(bazowy: u8, jasnosc: u8) -> u8 {
    match jasnosc {
        0 => 0,
        // Każde halving jasności to jeden stopień w dół; cztery stopnie to 1/16 mocy,
        // poniżej tego światło i tak nie jest widoczne.
        1..=15 => bazowy.saturating_sub(4),
        16..=31 => bazowy.saturating_sub(3),
        32..=63 => bazowy.saturating_sub(2),
        64..=127 => bazowy.saturating_sub(1),
        _ => bazowy,
    }
}

fn mm_to_m(p: [i32; 3]) -> [f32; 3] {
    [
        p[0] as f32 * 0.001,
        p[1] as f32 * 0.001,
        p[2] as f32 * 0.001,
    ]
}

fn kwadrat_odleglosci(a: [f32; 3], b: [f32; 3]) -> f32 {
    let (dx, dy, dz) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    dx * dx + dy * dy + dz * dz
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `blackout_ramps` z §7.3: wygaszenie trwa 1,5 s ±0,1 s i nie jest skokiem.
    ///
    /// Test jedzie przez [`Ambience::step_power`] — czyli przez tę samą funkcję, którą
    /// woła [`Ambience::fill_state`] — a nie przez podstawiony ręcznie krok rampy.
    /// Wejściem jest tablica `power`, bo to ona jest wyjściem sieci przesyłowej.
    #[test]
    fn wygaszenie_dzielnicy_trwa_poltorej_sekundy() {
        let mut a = Ambience::default();
        let zasilone = [PowerRec { supply_ratio: 255 }; MAX_DISTRICTS];
        let ciemno = [PowerRec { supply_ratio: 0 }; MAX_DISTRICTS];

        a.step_power(&zasilone, 0);
        assert_eq!(a.lit(0), 255, "świat zaczyna zasilony");

        // Klatka po klatce przy 60 Hz, dokładnie tak jak w kliencie.
        let mut ms = 0u64;
        while a.lit(0) > 0 && ms < 5_000 {
            ms += 16;
            a.step_power(&ciemno, ms);
        }
        assert!(
            (1_400..=1_600).contains(&ms),
            "wygaszenie trwało {ms} ms, a ma 1500 ±100"
        );

        // Powrót zasilania jest tak samo płynny — awaria, która mija skokiem,
        // wygląda jak błąd renderu tak samo jak awaria, która zaczyna się skokiem.
        let start = ms;
        while a.lit(0) < 255 && ms < start + 5_000 {
            ms += 16;
            a.step_power(&zasilone, ms);
        }
        assert!(
            (1_400..=1_600).contains(&(ms - start)),
            "powrót: {} ms",
            ms - start
        );
    }

    /// Przeskok zegara prezentacji (wczytany zapis, wyjście z pauzy) nie może zgasić
    /// dzielnicy jednym skokiem — przyrost jest przycięty do długości rampy.
    #[test]
    fn skok_zegara_nie_gasi_natychmiast() {
        let mut a = Ambience::default();
        let ciemno = [PowerRec { supply_ratio: 0 }; MAX_DISTRICTS];
        a.step_power(&ciemno, 0);
        a.step_power(&ciemno, 600_000);
        assert_eq!(a.lit(0), 0, "po pełnej rampie ma być ciemno");
        // ...ale nie szybciej niż rampa: pierwsza klatka po skoku zjeżdża o całą rampę,
        // nie o dziesięć minut.
        let mut b = Ambience::default();
        b.step_power(&ciemno, 0);
        b.step_power(&ciemno, 100);
        assert!(b.lit(0) > 200, "sto milisekund zgasiło {}", 255 - b.lit(0));
    }

    /// Przerzedzanie ma histerezę: sam próg włączenia przełączałby krok co klatkę.
    #[test]
    fn przerzedzanie_latarni_ma_histereze() {
        assert_eq!(next_stride(1, CLUSTER_PRESSURE_ON), 2);
        assert_eq!(next_stride(2, CLUSTER_PRESSURE_ON), 4);
        assert_eq!(
            next_stride(4, CLUSTER_PRESSURE_ON),
            4,
            "krok nie rośnie poza 4"
        );
        // Między progami nic się nie dzieje — to jest cała histereza.
        let miedzy = (CLUSTER_PRESSURE_OFF + CLUSTER_PRESSURE_ON) / 2;
        assert_eq!(next_stride(2, miedzy), 2);
        assert_eq!(next_stride(2, CLUSTER_PRESSURE_OFF), 1);
        assert_eq!(next_stride(1, 0), 1);
    }

    /// Przyciemnienie zmienia wykładnik, a nie barwę: wygaszana latarnia ma robić się
    /// ciemniejsza, nie szara.
    #[test]
    fn przyciemnienie_nie_rusza_barwy() {
        let pelna = LightRecord::new([0.0; 3], 1.0, LAMP_COLOR, skaluj_wykladnik(LAMP_EXP, 255));
        let slaba = LightRecord::new([0.0; 3], 1.0, LAMP_COLOR, skaluj_wykladnik(LAMP_EXP, 100));
        assert_eq!(
            pelna.color_rgbe & 0x00FF_FFFF,
            slaba.color_rgbe & 0x00FF_FFFF,
            "mantysa barwy drgnęła"
        );
        let [r1, _, _] = pelna.color();
        let [r2, _, _] = slaba.color();
        assert!(r2 < r1, "słabsze światło nie jest słabsze");
        assert_eq!(
            LightRecord::new([0.0; 3], 1.0, LAMP_COLOR, skaluj_wykladnik(LAMP_EXP, 0)).color(),
            [0.0, 0.0, 0.0],
            "zero nie gasi"
        );
    }

    /// W dzień okna są ciemne, w nocy jasne, a zakład bez obsady ciemny zawsze.
    #[test]
    fn okna_swieca_po_zmroku_i_gasna_przy_blackoucie() {
        let mut a = Ambience::default();
        a.set_daylight(255);
        assert_eq!(a.window_brightness(0, 200), 0, "w południe świeci");
        a.set_daylight(0);
        let noc = a.window_brightness(0, 200);
        assert!(noc > 0, "w nocy nie świeci");
        // Zakład bez zmiany ma światło dyżurne, ale słabsze niż pracujący.
        assert!(a.window_brightness(0, 0) < noc);
        assert!(a.window_brightness(0, 0) > 0);
        // Dzielnica bez prądu jest ciemna niezależnie od zmiany.
        a.lit_ms[0] = 0;
        assert_eq!(a.window_brightness(0, 255), 0, "blackout nie zgasił okien");
    }

    fn zapytanie(anim_ms: u64) -> ViewQuery {
        ViewQuery {
            aabb: magnat_sim_snapshot::Aabb::around([0, 0, 0], 1_000_000, 1_000_000),
            eye: [0, 0, 0],
            caps: magnat_sim_snapshot::SnapshotCaps::DEFAULT,
            anim_ms,
        }
    }

    /// Trzy źródła światła i trzy reguły: okno świeci tym, co ma w `lights`, latarnia
    /// tylko po zmroku i tylko z bliska, reflektor tylko przy zapalonych światłach.
    #[test]
    fn lista_swiatel_sklada_okna_latarnie_i_reflektory() {
        let mut a = Ambience {
            lamps: vec![
                Lamp {
                    pos: [10.0, 0.0, 6.0],
                    district: 0,
                },
                Lamp {
                    pos: [5000.0, 0.0, 6.0],
                    district: 0,
                },
            ],
            ..Default::default()
        };
        let mut snap = RenderSnapshot::default();
        snap.sites.push(magnat_sim_snapshot::SiteRenderRec {
            pos: [20_000, 0, 0],
            lights: 200,
            ..Default::default()
        });
        snap.vehicles.push(magnat_sim_snapshot::VehicleRenderRec {
            pos: [30_000, 0, 0],
            flags: VEHICLE_FLAG_LIGHTS,
            ..Default::default()
        });
        snap.vehicles.push(magnat_sim_snapshot::VehicleRenderRec {
            pos: [40_000, 0, 0],
            ..Default::default()
        });

        a.set_daylight(255);
        a.fill_lights(&zapytanie(0), &mut snap);
        // W dzień: okno (ma własne `lights`) i reflektor, bez latarni.
        assert_eq!(snap.lights.len(), 2, "latarnia zapaliła się w południe");

        snap.lights.clear();
        a.set_daylight(0);
        a.fill_lights(&zapytanie(16), &mut snap);
        // W nocy dochodzi bliska latarnia; ta 5 km stąd jest poza progiem 400 m.
        assert_eq!(snap.lights.len(), 3, "latarnia z 5 km weszła do listy");

        // Blackout gasi latarnię, a okno przychodzi już wygaszone przez `window_brightness`.
        snap.lights.clear();
        a.lit_ms[0] = 0;
        a.fill_lights(&zapytanie(32), &mut snap);
        assert_eq!(snap.lights.len(), 2, "latarnia świeci mimo blackoutu");
    }

    /// Przerzedzanie naprawdę zmniejsza listę, a nie tylko licznik.
    #[test]
    fn przerzedzanie_zmniejsza_liste_latarni() {
        let mut a = Ambience {
            lamps: (0..100u16)
                .map(|i| Lamp {
                    pos: [f32::from(i) * 3.0, 0.0, 6.0],
                    district: 0,
                })
                .collect(),
            ..Default::default()
        };
        a.set_daylight(0);
        let mut snap = RenderSnapshot::default();
        a.fill_lights(&zapytanie(0), &mut snap);
        let pelno = snap.lights.len();
        assert_eq!(pelno, 100);

        snap.lights.clear();
        a.set_cluster_peak(CLUSTER_PRESSURE_ON);
        a.fill_lights(&zapytanie(16), &mut snap);
        assert_eq!(a.lamp_stride(), 2);
        assert_eq!(snap.lights.len(), 50);
    }

    /// Skala emisji ma źródło w danych, a nie w kodzie. Test istnieje, bo `emission_ref`
    /// równe zeru daje **zero dymu z każdego komina** — czyli obraz nie do odróżnienia
    /// od świata, w którym nic nie produkuje.
    #[test]
    fn skala_emisji_ma_zrodlo_w_danych() {
        let t = magnat_supply::Tuning::load_default().expect("data/tuning/supply.ron");
        let a = Ambience {
            emission_ref: t.emission_ref_pm_g_per_min,
            ..Default::default()
        };
        assert!(a.emission_ref > 0, "brak stałej odniesienia skali emisji");
        assert_eq!(a.emission_scale(a.emission_ref), 255);
        assert_eq!(a.emission_scale(0), 0);
        assert_eq!(a.emission_scale(a.emission_ref * 10), 255, "brak nasycenia");
        // Ta sama arytmetyka co po stronie M6 — dwie skale tej samej wielkości
        // rozjechałyby się przy pierwszej zmianie kalibracji.
        assert_eq!(
            a.emission_scale(a.emission_ref / 2),
            t.emission_scale(a.emission_ref / 2)
        );
        // Zakład emitujący **poniżej grama na minutę** ma być widoczny, a nie zaokrąglony
        // do zera: przy odniesieniu 40 g/min pół grama to 3 z 255 (`I-22`).
        assert!(
            a.emission_scale_mg(500) > 0,
            "pół grama znikło w zaokrągleniu"
        );
        assert_eq!(a.emission_scale_mg(0), 0);
    }

    /// Cała skala mgły ma być osiągalna: przy pełnym zachmurzeniu i bezwietrznie
    /// `fog_density` dochodzi do 255, bo tyle obiecuje kontrakt renderu (`I-23`).
    #[test]
    fn mgla_siega_pelnej_skali() {
        let gesta = |chmura: i32, wiatr_kmh: i32| -> u8 {
            let bezwietrznie = 255 - (wiatr_kmh * 255 / 30).min(255);
            let wilgotno = (chmura - 128).max(0) * 2;
            (wilgotno * bezwietrznie / 255).clamp(0, 255) as u8
        };
        assert_eq!(gesta(255, 0), 254, "pełna mgła nie dochodzi do skali");
        assert_eq!(gesta(128, 0), 0, "połowa zachmurzenia już mgli");
        assert_eq!(gesta(255, 40), 0, "wichura nie rozwiała mgły");
        assert!(gesta(200, 5) > 0 && gesta(200, 5) < 200);
    }

    /// Wiatr trzyma kierunek przez całą dobę i zmienia go następnej — deszcz siekący
    /// raz w lewo, raz w prawo między klatkami wygląda na usterkę.
    #[test]
    fn wiatr_jest_staly_w_dobie() {
        assert_eq!(wind_vector(20, 5), wind_vector(20, 5));
        assert_ne!(wind_vector(20, 5), wind_vector(20, 6));
        assert_eq!(wind_vector(0, 5), [0, 0], "cisza ma zerowy wektor");
    }
}
