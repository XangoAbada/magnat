//! Argumenty wiersza poleceń klienta i ich parsowanie.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "magnat",
    about = "Symulator miasta i gospodarki — podgląd świata (M1)"
)]
pub(crate) struct Args {
    /// Ziarno świata, dziesiętnie albo `0x…`.
    #[arg(long, default_value = "0xC0FFEE")]
    pub(crate) seed: String,

    /// `4km` | `8km` | `12km` | `16km`.
    #[arg(long, default_value = "8km")]
    pub(crate) size: String,

    /// `coastal`/`nadmorski`, `mountain`/`gorski`, `lowland`/`nizinny`,
    /// `river`/`rzeczny`, `desert`/`pustynny`.
    #[arg(long, default_value = "river")]
    pub(crate) region: String,

    #[arg(long, default_value = "1990")]
    pub(crate) epoch: String,

    #[arg(long, default_value = "mixed")]
    pub(crate) profile: String,

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

    /// Język interfejsu: `pl` albo `en`. Tekst w UI zawsze pochodzi z `data/locale/`
    /// w obu wersjach (CLAUDE.md), więc przełącznik nie ma prawa czegokolwiek zgubić.
    #[arg(long, default_value = "pl")]
    pub(crate) locale: String,
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
