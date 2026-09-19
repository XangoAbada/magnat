//! Argumenty wiersza poleceń klienta i ich parsowanie.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "magnat",
    about = "Symulator miasta i gospodarki. Bez argumentów: menu główne i kreator świata."
)]
pub(crate) struct Args {
    /// Ziarno świata, dziesiętnie albo `0x…`.
    ///
    /// **Podanie któregokolwiek parametru świata omija powłokę** i stawia miasto
    /// od razu (M9b/WP14). Bez nich `magnat` zaczyna od menu głównego, a parametry
    /// wybiera się w kreatorze — to jest droga dla gracza, ta niżej dla nas.
    #[arg(long)]
    pub(crate) seed: Option<String>,

    /// `4km` | `8km` | `12km` | `16km`.
    #[arg(long)]
    pub(crate) size: Option<String>,

    /// `coastal`/`nadmorski`, `mountain`/`gorski`, `lowland`/`nizinny`,
    /// `river`/`rzeczny`, `desert`/`pustynny`.
    #[arg(long)]
    pub(crate) region: Option<String>,

    /// `1950` | `1970` | `1990` | `2010` | `2020`.
    #[arg(long)]
    pub(crate) epoch: Option<String>,

    /// `industrial` | `port` | `university` | `tourist` | `agricultural` | `mixed`.
    #[arg(long)]
    pub(crate) profile: Option<String>,

    /// `easy` | `normal` | `hard` | `brutal`.
    #[arg(long)]
    pub(crate) difficulty: Option<String>,

    /// Liczba wątków generacji i meshingu; 0 = liczba rdzeni.
    #[arg(long, default_value_t = 0)]
    pub(crate) threads: usize,

    /// Zapisuje jedną klatkę do pliku PNG i kończy. Bez tego renderu nie da się
    /// zweryfikować inaczej niż patrząc na ekran — a §7.4 wymaga raportu.
    #[arg(long)]
    pub(crate) screenshot: Option<std::path::PathBuf>,

    /// Ile klatek odczekać przed zrzutem, żeby strumieniowanie zdążyło wypełnić kadr.
    #[arg(long, default_value_t = 120)]
    pub(crate) screenshot_after: u32,

    /// Godzina dnia na starcie: `10` albo `8:15`.
    ///
    /// Minuty są tu potrzebne, a nie ozdobne: szczyt poranny w mieście z medianą dojazdu
    /// 18 minut trwa kilkanaście minut, a o pełnej ósmej ulice są jeszcze albo już puste.
    #[arg(long, default_value = "10")]
    pub(crate) hour: String,

    /// Wysokość orbity kamery w metrach nad celem.
    #[arg(long, default_value_t = 900.0)]
    pub(crate) dist: f32,

    /// Punkt, nad którym stoi kamera: `x,y` w metrach. Bez tego kamera zawsze celuje
    /// w środek mapy, a obejrzenie konkretnego miejsca (jeziora, doliny) wymaga latania
    /// myszą — czego zrzut z CI zrobić nie może.
    #[arg(long)]
    pub(crate) target: Option<String>,

    /// Scena pomiarowa z §7.4: ustalony przelot przez pięć etapów (orbita miasta →
    /// dzielnica → poziom ulicy → przelot 2 km → orbita) przez zadaną liczbę sekund,
    /// a na końcu raport wydajności. Bez okna nie da się tego zmierzyć uczciwie —
    /// czas GPU zależy od tego, co naprawdę trafiło na ekran.
    #[arg(long)]
    pub(crate) bench: Option<f32>,

    /// Wypisuje kartę inspekcji punktu `x,y` zaraz po generacji i kończy — ta sama karta,
    /// którą w oknie pokazuje klik prawym przyciskiem. Istnieje, bo kontrakt `TerrainQuery`
    /// da się wtedy sprawdzić bez GPU i bez rąk (§7.5).
    #[arg(long)]
    pub(crate) inspect: Option<String>,

    /// Nakładka debug na starcie: `height`, `flow`, `water`, `biome`, `temp-jan`,
    /// `temp-jul`, `precip`, `geology`, `deposits`, `land-value`. W oknie przełącza je `F3`.
    #[arg(long)]
    pub(crate) overlay: Option<String>,

    /// Scena pomiarowa WP-R4: tyle świateł punktowych rozrzuconych wokół celu kamery.
    /// Kryterium mówi o 4096 — tyle właśnie mieści snapshot M11 (`MAX_LIGHTS`).
    #[arg(long, default_value_t = 0)]
    pub(crate) lights: usize,

    /// Scena pomiarowa WP-R2: wszystko w LOD0 do zadanego promienia w metrach.
    #[arg(long)]
    pub(crate) lod0_radius: Option<i32>,

    /// Cięcie poziomami na starcie: 0 = wyłączone, `n` = zdejmij `n` kondygnacji
    /// budynku pod celem kamery. W oknie przełącza je `C`.
    ///
    /// Istnieje z tego samego powodu co `--lights` i `--crowd`: kryterium WP5 mówi
    /// o przekroju centrum handlowego, a zrzut z CI nie ma jak nacisnąć klawisza.
    #[arg(long, default_value_t = 0)]
    pub(crate) cut: u8,

    /// Scena pomiarowa M11b: tylu syntetycznych pieszych rozstawionych wokół celu kamery,
    /// **obok** tych, których oddała symulacja. Ta sama konwencja co `--lights`.
    ///
    /// Istnieje, bo kryteria WP3 i WP4 mówią o dwudziestu tysiącach animowanych postaci
    /// i o przelocie bez przeskoku detalu, a warstwa Mikro w oknie 900 m oddaje ich
    /// kilkadziesiąt. Tłum wchodzi **do snapshotu po jego wypełnieniu**, więc nie dotyka
    /// symulacji ani hasha stanu — to jest scena, nie mieszkańcy.
    #[arg(long, default_value_t = 0)]
    pub(crate) crowd: usize,

    /// Odstęp między pieszymi sceny `--crowd` w metrach.
    ///
    /// Istnieje, bo pomiar dotyczy **konkretnego pasma detalu**: żeby zmierzyć koszt
    /// animacji dwudziestu tysięcy postaci, muszą się one zmieścić w paśmie, w którym
    /// animacja w ogóle działa. Przy domyślnym kroku 1,8 m siatka ma ćwierć kilometra
    /// boku i połowa tłumu jest już impostorami.
    #[arg(long, default_value_t = 1.8)]
    pub(crate) crowd_step: f64,

    /// Wyłącza animację: każda encja dostaje klip spoza katalogu, czyli pozę spoczynkową.
    ///
    /// Bez tego kryterium WP3 („< 0,3 ms dodatkowego czasu GPU **względem pozy bazowej**")
    /// jest niemierzalne — trzeba móc zmierzyć tę samą scenę dwa razy.
    #[arg(long, default_value_t = false)]
    pub(crate) no_anim: bool,

    /// Dzień roku (0–359) — wpływa na deklinację słońca, czyli na porę roku.
    /// 170 to przesilenie letnie w kalendarzu 360-dniowym (00 §K-1).
    #[arg(long, default_value_t = 170)]
    pub(crate) day: u64,

    /// Pomija generację miasta i pokazuje czysty teren M1. Istnieje po to, żeby
    /// regresję terenu dało się zdiagnozować bez zabudowy zasłaniającej widok (WP12b).
    #[arg(long, default_value_t = false)]
    pub(crate) no_city: bool,

    /// Pomija Etap 8 i pokazuje puste miasto (M3d). Istnieje z tego samego powodu co
    /// `--no-city`: zaludnienie metropolii to kilkadziesiąt sekund, a regresji renderu
    /// nie diagnozuje się, czekając na nią.
    #[arg(long, default_value_t = false)]
    pub(crate) no_citizens: bool,

    /// Wyłącza gospodarkę detaliczną (M5) i wraca do zachowania M3: miejsca dostarcza
    /// atrapa `InfinitePlaces`, w której wszystko jest i nic nie kosztuje.
    ///
    /// To jest **udokumentowana droga wyjścia** z kosztu klatki, nie wygoda (`AB-2`):
    /// każdy zakup to dwa wywołania routera M4, więc doba z gospodarką kosztuje
    /// wielokrotnie więcej niż bez niej. Przy `X10` na dużym mieście to jest różnica
    /// między płynną kamerą a pokazem slajdów — i lepiej, żeby dało się ją wyłączyć
    /// jednym przełącznikiem, niż żeby ktoś diagnozował „wolny render".
    #[arg(long, default_value_t = false)]
    pub(crate) no_economy: bool,

    /// Wchodzi do świata **bez postaci gracza** i bez ekranu wyboru: tryb przeglądu.
    ///
    /// To samo, co „Tylko oglądam" na ekranie wyboru postaci, tylko bez klikania.
    /// Istnieje dla tej samej drogi co `--no-city` i `--pick`: zrzut, pomiar i obejrzenie
    /// miasta nie potrzebują postaci, a ekran wyboru jest wtedy jednym naciśnięciem
    /// klawisza w środku skryptu.
    #[arg(long, default_value_t = false)]
    pub(crate) observe: bool,

    /// Sprawdza bufor ID bez rąk: ustawia kursor na piksel `x,y`, przewija kilka klatek
    /// i wypisuje, w kogo trafiono (kryterium WP11: „kliknięcie w pieszego daje
    /// `CitizenId`"). Bez tego jedynym sposobem sprawdzenia selekcji jest mysz.
    #[arg(long)]
    pub(crate) pick: Option<String>,

    /// Prędkość gry na starcie: `0` (pauza), `1`, `3`, `10`. Ta sama czwórka co
    /// w widgecie sterowania czasem — i ta sama, na której stoi wymóg „prędkość nie
    /// wpływa na wynik" (§5.11, PRD §14.5).
    #[arg(long, default_value_t = 1)]
    pub(crate) speed: u32,

    /// Język interfejsu: `pl` albo `en`. Bez tego argumentu bierze się go z profilu
    /// gracza, czyli z tego, co wybrano w ustawieniach ostatnim razem.
    #[arg(long)]
    pub(crate) locale: Option<String>,
}

impl Args {
    /// Parametry świata z wiersza poleceń albo `None`, jeśli żadnego nie podano.
    ///
    /// `None` znaczy „idź do menu głównego". Wystarczy **jeden** parametr, żeby ominąć
    /// powłokę: `magnat --seed 7` ma dalej stawiać świat od razu, bo tą drogą chodzą
    /// zrzuty, przeloty pomiarowe i test bufora identyfikatorów.
    ///
    /// # Errors
    /// Nieznana wartość któregokolwiek parametru.
    pub(crate) fn world_params(
        &self,
    ) -> Result<Option<magnat_world::WorldGenParams>, Box<dyn std::error::Error>> {
        let podano = self.seed.is_some()
            || self.size.is_some()
            || self.region.is_some()
            || self.epoch.is_some()
            || self.profile.is_some()
            || self.difficulty.is_some()
            // `--no-city` bez ziarna też znaczy „pokaż mi teren", a nie „pokaż menu".
            || self.no_city;
        if !podano {
            return Ok(None);
        }
        let d = magnat_world::WorldGenParams::default();
        Ok(Some(magnat_world::WorldGenParams {
            seed: match &self.seed {
                Some(s) => parse_seed(s)?,
                None => 0x00C0_FFEE,
            },
            size: match &self.size {
                Some(s) => s.parse()?,
                None => magnat_world::WorldSize::Medium8km,
            },
            region: match &self.region {
                Some(s) => s.parse()?,
                None => magnat_world::Region::River,
            },
            epoch: match &self.epoch {
                Some(s) => s.parse()?,
                None => d.epoch,
            },
            profile: match &self.profile {
                Some(s) => s.parse()?,
                None => d.profile,
            },
            difficulty: match &self.difficulty {
                Some(s) => s.parse()?,
                None => d.difficulty,
            },
        }))
    }
}

/// `HH` albo `HH:MM` na minutę doby.
pub(crate) fn parse_hour(s: &str) -> Result<u32, Box<dyn std::error::Error>> {
    let (h, m) = match s.split_once(':') {
        Some((h, m)) => (h.trim().parse::<u32>()?, m.trim().parse::<u32>()?),
        None => (s.trim().parse::<u32>()?, 0),
    };
    if h > 23 || m > 59 {
        return Err(format!("--hour {s}: poza dobą").into());
    }
    Ok(h * 60 + m)
}

pub(crate) fn parse_seed(s: &str) -> Result<u64, Box<dyn std::error::Error>> {
    let t = s.trim();
    Ok(
        match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
            Some(hex) => u64::from_str_radix(&hex.replace('_', ""), 16)?,
            None => t.replace('_', "").parse::<u64>()?,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn godzina_z_tekstu() {
        assert_eq!(parse_hour("10").unwrap(), 600);
        assert_eq!(parse_hour("8:15").unwrap(), 495);
        assert!(parse_hour("24").is_err());
        assert!(parse_hour("9:60").is_err());
    }

    #[test]
    fn ziarno_dziesietnie_i_szesnastkowo() {
        assert_eq!(parse_seed("0xC0FFEE").unwrap(), 0x00C0_FFEE);
        assert_eq!(parse_seed("0XC0FFEE").unwrap(), 0x00C0_FFEE);
        assert_eq!(parse_seed("1_000").unwrap(), 1000);
        assert!(parse_seed("0xzz").is_err());
    }
}
