//! Pogoda i pory roku (WP6, §5.6, PRD §11.3).
//!
//! **Pogoda jest odchyłką od normy klimatycznej M1, nigdy jej podmianą.** Reguła
//! stoi wprost w nagłówku `sim/world/src/climate.rs`: klimat jest wyprowadzalny
//! z ziarna i nie wolno go mutować, bo świat przestałby być odtwarzalny. Normy
//! przychodzą tu **raz**, przy stawianiu świata — dzięki temu ten crate nie zależy
//! od `sim/world` i nie ciągnie za sobą generatora miasta.
//!
//! **Dlaczego proces ze stanem, skoro `weather_at` w `core` jest funkcją czystą.**
//! Bo susza jest zjawiskiem o pamięci. Przy losowaniu niezależnym doba po dobie
//! suma opadu z trzydziestu dób trzyma się średniej tak ciasno, że sonda
//! `PrecipDeficit30dChm` nigdy nie doszłaby do wartości, przy których krzywa suszy
//! w ogóle zaczyna rosnąć — czyli zdarzenie byłoby martwe (ryzyko `R2`). Wilgotne
//! i suche okresy muszą się kleić, a kleją się przez pamięć. Cena jest jedna
//! i zapłacona: pogoda jest odtąd **stanem** i wchodzi do hasha (`K-26`).
//!
//! Cała arytmetyka jest całkowitoliczbowa — §5.0 fazy wymaga tego mocniej niż `K-6`.

use magnat_core::{rng, HashState, Season, StateHasher, StreamId, Tick, Weather, NO_ENTITY};

/// Ile dób pamięta pierścień opadu. Dziewięćdziesiąt, bo sonda sezonowa pyta
/// o kwartał, a kwartał w kalendarzu `K-1` to dokładnie 90 dób.
pub const PRECIP_RING: usize = 90;

/// Temperatura odniesienia dla stopniodni grzewczych, w dziesiątych °C.
const HDD_BASE_DC: i32 = 150;

/// Normy klimatyczne miasta — dwanaście liczb na temperaturę i dwanaście na opad.
///
/// Kopia, a nie referencja do `WorldData`: mieści się w 48 bajtach, nie zmienia się
/// nigdy i zdejmuje z tego crate'u zależność od `sim/world`. **Nie wchodzi do hasha
/// stanu** — to dana wejściowa miasta, tak samo jak `NeedTable` (M3a).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ClimateNorms {
    pub temp_monthly_dc: [i16; 12],
    pub precip_monthly_mm: [u16; 12],
}

impl Default for ClimateNorms {
    /// Klimat umiarkowany — ten sam kształt, który zaślepka `core::weather_at`
    /// zaszywała w stałych. Scenariusz bez miasta dostaje więc pogodę, a nie zero.
    fn default() -> ClimateNorms {
        ClimateNorms {
            temp_monthly_dc: [-20, -10, 30, 90, 140, 180, 200, 195, 145, 90, 40, 0],
            precip_monthly_mm: [35, 30, 35, 40, 60, 70, 80, 70, 50, 45, 45, 40],
        }
    }
}

impl ClimateNorms {
    /// Norma temperatury dnia roku, interpolowana między środkami miesięcy.
    ///
    /// Ten sam schemat co `magnat_world::ClimateCell::temp_at_day_dc` i **celowo
    /// ten sam**: pogoda ma krążyć wokół tej samej krzywej, po której M1 rozpisał
    /// klimat, a nie obok niej. Powtórzenie dwunastu linii interpolacji jest tu
    /// tańsze niż zależność `sim/events → sim/world`, która ciągnęłaby za sobą
    /// generator miasta; pilnuje go test zgodności sumy rocznej z normą.
    #[must_use]
    pub fn temp_at_day_dc(&self, day_of_year: u16) -> i32 {
        interp(&self.temp_monthly_dc.map(i32::from), day_of_year)
    }

    /// Norma opadu **dobowego** w setnych milimetra. Miesiąc dzieli się przez 30
    /// dopiero po pomnożeniu przez 100, żeby nie zgubić jednej trzeciej sumy
    /// rocznej na zaokrągleniu w tę samą stronę (ta sama pułapka co w `K-25`).
    #[must_use]
    pub fn precip_at_day_chm(&self, day_of_year: u16) -> i32 {
        let m = interp(&self.precip_monthly_mm.map(i32::from), day_of_year);
        m.max(0) * 100 / 30
    }
}

/// Interpolacja liniowa po dwunastu wartościach miesięcznych, środek miesiąca
/// w dobie `30m + 15`.
fn interp(monthly: &[i32; 12], day_of_year: u16) -> i32 {
    let d = i32::from(day_of_year % 360);
    let przes = d - 15;
    let (m0, frac) = if przes < 0 {
        (11i32, przes + 30)
    } else {
        (przes / 30, przes % 30)
    };
    let a = monthly[(m0 % 12) as usize];
    let b = monthly[((m0 + 1) % 12) as usize];
    a + (b - a) * frac / 30
}

/// Stan pogody: bieżąca doba, jej odchyłki i pamięć opadu.
#[derive(Clone, Debug)]
pub struct WeatherState {
    norms: ClimateNorms,
    /// Odchyłka temperatury od normy, w dziesiątych °C. Proces z pamięcią.
    temp_anom_dc: i32,
    /// Odchyłka wilgotności w punktach bazowych wokół 10 000. Proces z pamięcią —
    /// i to on robi susze: 10 dób po 4 000 bps to deficyt, którego niezależne
    /// losowania nie ułożyłyby nigdy.
    wet_anom_bps: i32,
    /// Opad każdej z ostatnich 90 dób w setnych mm, indeksowany dobą modulo 90.
    precip_ring_chm: [u32; PRECIP_RING],
    /// Ile dób pierścień już zebrał — zanim się napełni, sondy liczą z tego, co jest.
    filled: u16,
    /// Pokrywa śnieżna w milimetrach słupa wody.
    snow_mm: u32,
    /// Doba, dla której policzone są `temp_anom_dc`, `wet_anom_bps` i wpis w pierścieniu.
    day: u64,
    /// Pogoda tej godziny — to ją widzi reszta świata.
    now: Weather,
}

impl Default for WeatherState {
    fn default() -> WeatherState {
        WeatherState::new(ClimateNorms::default())
    }
}

impl WeatherState {
    #[must_use]
    pub fn new(norms: ClimateNorms) -> WeatherState {
        WeatherState {
            norms,
            temp_anom_dc: 0,
            wet_anom_bps: 10_000,
            precip_ring_chm: [0; PRECIP_RING],
            filled: 0,
            snow_mm: 0,
            day: u64::MAX,
            now: Weather::default(),
        }
    }

    #[must_use]
    pub fn weather(&self) -> Weather {
        self.now
    }

    #[must_use]
    pub fn norms(&self) -> ClimateNorms {
        self.norms
    }

    /// Odchyłka temperatury od normy, w dziesiątych °C — do inspektora i do testu
    /// zgodności z klimatem. Średnia tej liczby po roku ma być bliska zeru,
    /// bo pogoda krąży wokół normy, a nie obok niej.
    #[must_use]
    pub fn temp_anomaly_dc(&self) -> i32 {
        self.temp_anom_dc
    }

    /// Odchyłka wilgotności w punktach bazowych wokół 10 000.
    #[must_use]
    pub fn wet_anomaly_bps(&self) -> i32 {
        self.wet_anom_bps
    }

    /// Krok godzinowy. Przy zmianie doby przelicza odchyłki i dopisuje opad
    /// do pierścienia; w każdej godzinie składa pogodę widzianą przez świat.
    pub fn step(&mut self, seed: u64, day: u64, hour: u32) {
        if self.day != day {
            self.roll_day(seed, day);
            self.day = day;
        }
        let doba = (day % 360) as u16;
        let norma = self.norms.temp_at_day_dc(doba);
        // Kształt doby: minimum o 5:00, maksimum o 17:00, trójkąt **symetryczny**.
        // Symetria nie jest estetyką — przy dwunastu godzinach w każdą stronę
        // średnia dobowa kształtu wynosi dokładnie zero, więc rok krąży wokół
        // normy klimatycznej, a nie obok niej. Kształt niesymetryczny zaniżał
        // średnią roczną o 0,8 °C i wywracał kryterium WP6 („±0,5 °C").
        let amp = 25 + i32::from(Season::of_day(doba) == Season::Summer) * 25;
        let h = hour as i32;
        let faza = if (5..17).contains(&h) {
            -amp + 2 * amp * (h - 5) / 12
        } else {
            let k = (h + 24 - 17) % 24;
            amp - 2 * amp * k / 12
        };
        let temp = norma + self.temp_anom_dc + faza;
        let dobowy = self.precip_ring_chm[(day % PRECIP_RING as u64) as usize];
        // Opad rozkłada się równo na dobę: 100 setnych mm na godzinę to ulewa
        // (24 mm na dobę), więc promile intensywności to setne mm razy dziesięć.
        let na_godzine = dobowy / 24;
        self.now = Weather {
            temp_dc: i16::try_from(temp.clamp(-600, 500)).unwrap_or(0),
            precip_permille: u16::try_from((na_godzine * 10).min(1_000)).unwrap_or(0),
            wind_kmh: self.wind_kmh(seed, day, hour),
            snow_cover_mm: u16::try_from(self.snow_mm.min(u32::from(u16::MAX))).unwrap_or(u16::MAX),
        };
    }

    /// Wiatr: funkcja doby i godziny, bez pamięci. Wichura trwa godziny, nie tygodnie,
    /// więc proces z pamięcią nic by tu nie kupił — a sonda `WindKmh` służy jednemu
    /// zdarzeniu, które ma zachodzić rzadko i krótko.
    fn wind_kmh(&self, seed: u64, day: u64, hour: u32) -> u16 {
        // Kluczem jest **godzina w miejscu indeksu encji**, a doba zostaje tickiem.
        // Sklejenie obu w jeden tick (`day * 24 + hour`) dawałoby wiatr godziny
        // zerowej doby 1 z dokładnie tego samego stanu generatora co przejście
        // doby 24 — bo `rng` jest funkcją czystą czterech argumentów, a te dwa
        // wywołania miałyby trzy z nich identyczne.
        let mut r = rng(seed, StreamId::Weather, hour, Tick(day));
        // Rozkład skośny: 0–30 km/h przez większość czasu, ogon do ~120.
        let x = r.gen_range_u32(1_000);
        let v = 5 + x * x / 9_000;
        u16::try_from(v.min(140)).unwrap_or(0)
    }

    /// Przejście doby: odchyłki, opad, śnieg.
    fn roll_day(&mut self, seed: u64, day: u64) {
        let mut r = rng(seed, StreamId::Weather, NO_ENTITY, Tick(day));
        // Proces z pamięcią, całkowitoliczbowy: część wczorajszej odchyłki plus szum.
        // Dla temperatury 0,7 na dobie daje czas korelacji rzędu trzech dób —
        // tyle, ile trwa fala ciepła. Dla wilgotności **0,93**, czyli rzędu dwóch
        // tygodni, i to nie jest przesada: przy krótszej pamięci najgłębszy
        // niedobór trzydziestodobowy roku zatrzymuje się na 28 mm, czyli poniżej
        // punktu, w którym krzywa suszy w `data/events/natural.ron` zaczyna rosnąć.
        // Susza jest zjawiskiem o pamięci i albo się ją modeluje, albo zdarzenia
        // nie ma (`R2`).
        let szum_t = i32::try_from(r.gen_range_u32(81)).unwrap_or(0) - 40;
        self.temp_anom_dc = (self.temp_anom_dc * 7 / 10 + szum_t).clamp(-120, 120);
        let szum_w = i32::try_from(r.gen_range_u32(7_001)).unwrap_or(0) - 3_500;
        self.wet_anom_bps =
            ((self.wet_anom_bps - 10_000) * 93 / 100 + szum_w + 10_000).clamp(0, 30_000);

        let norma = self.norms.precip_at_day_chm((day % 360) as u16);
        // Ile spadło: norma × wilgotność, a potem rzut, czy w ogóle padało.
        // Opad jest zdarzeniem rzadkim i obfitym, nie mżawką rozsmarowaną na rok:
        // pada mniej więcej co trzecią dobę, więc w dobie deszczowej spada trzykrotność.
        let oczekiwany = norma * self.wet_anom_bps / 10_000;
        let pada = r.gen_range_u32(1_000) < 333;
        let dzis = if pada {
            u32::try_from((oczekiwany * 3).max(0)).unwrap_or(0)
        } else {
            0
        };
        self.precip_ring_chm[(day % PRECIP_RING as u64) as usize] = dzis;
        self.filled = self.filled.saturating_add(1).min(PRECIP_RING as u16);

        // Śnieg: przy mrozie opad zalega, przy odwilży topnieje proporcjonalnie
        // do temperatury. Milimetr słupa wody to dziesiątki setnych milimetra opadu.
        let temp = self.norms.temp_at_day_dc((day % 360) as u16) + self.temp_anom_dc;
        if temp < 0 {
            self.snow_mm = self.snow_mm.saturating_add(dzis / 100);
        } else {
            let topi = u32::try_from(temp).unwrap_or(0) * 2 / 10;
            self.snow_mm = self.snow_mm.saturating_sub(topi.max(1));
        }
    }

    /// Opad dzisiejszej doby w setnych milimetra — suma, a nie chwilowa intensywność.
    ///
    /// Do pomiaru zgodności z normą klimatyczną służy **ta** liczba, a nie suma
    /// godzinowych `precip_permille`: tamta gubi resztę z dzielenia doby na
    /// dwadzieścia cztery i przycina ulewę do skali promili.
    #[must_use]
    pub fn precip_today_chm(&self) -> u32 {
        if self.day == u64::MAX {
            return 0;
        }
        self.precip_ring_chm[(self.day % PRECIP_RING as u64) as usize]
    }

    /// Niedobór opadu z ostatnich `n` dób wobec normy, w setnych milimetra.
    /// Dodatni znaczy „spadło mniej, niż powinno".
    #[must_use]
    pub fn precip_deficit_chm(&self, n: u16) -> i64 {
        if self.day == u64::MAX {
            return 0;
        }
        let ile = u64::from(n.min(self.filled).min(PRECIP_RING as u16)).max(1);
        let mut faktyczny: i64 = 0;
        let mut normalny: i64 = 0;
        for i in 0..ile {
            let d = self.day.saturating_sub(i);
            faktyczny += i64::from(self.precip_ring_chm[(d % PRECIP_RING as u64) as usize]);
            normalny += i64::from(self.norms.precip_at_day_chm((d % 360) as u16));
        }
        normalny - faktyczny
    }

    /// Stopniodoby grzewcze bieżącej godziny, w dziesiątych °C poniżej 15 °C.
    #[must_use]
    pub fn heating_degree_dc(&self) -> i32 {
        (HDD_BASE_DC - i32::from(self.now.temp_dc)).max(0)
    }

    /// Mnożnik popytu na medium, w punktach bazowych.
    ///
    /// Ciepło i gaz są **funkcją mrozu**, a nie stałą z korektą: w lipcu kaloryfery
    /// stoją i sieć ciepłownicza wozi prawie nic, a w styczniu przy −5 °C idzie
    /// pełną mocą — tak wymiaruje ją most. Prąd reaguje słabiej (część ogrzewania
    /// jest elektryczna) i dodatkowo rośnie w upał (chłodzenie). Woda rośnie
    /// wyłącznie w upał. Reszta mediów pogody nie widzi.
    #[must_use]
    pub fn demand_bps(&self, service: magnat_core::UtilityService) -> u32 {
        use magnat_core::UtilityService as U;
        let hdd = self.heating_degree_dc();
        let upal = (i32::from(self.now.temp_dc) - 250).max(0);
        let v = match service {
            U::Heat | U::Gas => (hdd * 50).clamp(500, 20_000),
            U::Electricity => (10_000 + hdd * 8 + upal * 6).min(20_000),
            U::Water => (10_000 + upal * 8).min(16_000),
            U::Sewage | U::Waste | U::Internet => 10_000,
        };
        u32::try_from(v).unwrap_or(10_000)
    }
}

impl HashState for WeatherState {
    /// Do hasha wchodzi **stan**, nie normy: klimat jest wejściem świata i jest
    /// identyczny w obu przebiegach tego samego ziarna.
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.temp_anom_dc as u32);
        h.write_u32(self.wet_anom_bps as u32);
        for p in self.precip_ring_chm {
            h.write_u32(p);
        }
        h.write_u16(self.filled);
        h.write_u32(self.snow_mm);
        h.write_u64(self.day);
        h.write_u16(self.now.temp_dc as u16);
        h.write_u16(self.now.precip_permille);
        h.write_u16(self.now.wind_kmh);
        h.write_u16(self.now.snow_cover_mm);
    }
}
