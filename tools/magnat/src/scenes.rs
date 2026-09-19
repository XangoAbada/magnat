//! Sceny odniesienia budżetu klatki (M11e/WP10, §7.2 dokumentu fazy).
//!
//! Siedem ustalonych kadrów, każdy z własnym progiem czasu klatki. Scena **definiuje
//! cały przebieg**: świat, dobę, godzinę, kamerę, tłum i pogodę. Argumenty wiersza
//! poleceń, które scena posiada, są przy `--bench-scene` nadpisywane — bez tego dwa
//! uruchomienia mierzyłyby dwa różne kadry i porównanie z linią bazową mówiłoby
//! o argumentach, a nie o kodzie.
//!
//! **Korekta §7.2 (`H-9`):** plan zapowiadał „zamrożone zapisy gry w repo
//! (`bench/scenes/*.mgsave`)". Takiego formatu nie ma i nie powstał: zapis gry to
//! ziarno plus dziennik wejść (`game/src/save.rs`), a zrzutu świata do pliku nie ma
//! nigdzie w repozytorium. Scena jest więc **presetem argumentów nad deterministycznym
//! generatorem** — ten sam seed daje ten sam świat, co pilnuje macierz hashy terenu
//! z M1. Zamrożony zrzut wymagałby schematu zapisu, którego właścicielem jest M12
//! (`M12b`), a zamrożenie go tutaj przesądzałoby cudzą decyzję przed czasem.

use magnat_render::{CameraMode, CameraState, RenderStats};
use std::fmt::Write as _;

/// Ziarno scen odniesienia (§7.2). „MAGNAT" w ASCII.
pub(crate) const SEED: u64 = 0x4D41_474E_4154;

/// Doba, w której stoją wszystkie sceny (§7.2).
pub(crate) const DZIEN: u64 = 400 % 360;

/// Maszyna, na której progi są mierzone — decyzja właściciela produktu z 2026-09-19.
pub(crate) const MASZYNA: &str = "RTX 4070 Ti SUPER @ 1080p";

/// Maszyna, której dotyczy obietnica z PRD §20.2 („GPU średniej klasy 2024").
pub(crate) const MASZYNA_DOCELOWA: &str = "RTX 4060 @ 1080p";

/// Zapas między maszyną pomiarową a docelową.
///
/// Mierzymy na karcie wyraźnie szybszej od tej, o której mówi PRD, więc próg
/// bezpośrednio z §20.2 byłby obietnicą niepokrytą niczym. Współczynnik jest **jawną
/// stałą, a nie wbudowaną w progi**: kiedy ktoś zmierzy te same sceny na RTX 4060,
/// zmienia się jedna liczba i raport przestaje zgadywać.
///
/// `ponytail:` sufit nazwany — 0,55 to stosunek przepustowości obu kart z materiałów
/// producenta, nie pomiar tej gry. Ścieżka wyjścia: przebieg na maszynie docelowej.
pub(crate) const ZAPAS: f32 = 0.55;

/// Klatki rozgrzewki: strumieniowanie chunków, wypalenie atlasów, rozkręcenie sterownika.
pub(crate) const ROZGRZEWKA: u32 = 120;

/// Klatki pomiaru (§7.2).
pub(crate) const POMIAR: u32 = 600;

/// Skąd bierze się kadr sceny.
#[derive(Clone, Copy)]
pub(crate) enum Kadr {
    /// Orbita nad celem: wysokość w metrach i pochylenie w stopniach.
    Orbita { dist_m: f32, pitch_deg: f32 },
    /// Poziom oczu pieszego nad celem.
    Ulica { wysokosc_m: f32 },
}

/// Jedna scena odniesienia.
#[derive(Clone, Copy)]
pub(crate) struct Scena {
    pub nazwa: &'static str,
    /// Cel czasu klatki z PRD §20.2, **przed** zapasem maszyny.
    pub target_ms: f32,
    pub godzina: &'static str,
    pub kadr: Kadr,
    /// Ilu syntetycznych pieszych dokłada scena (`H-3`) — warstwa Mikro oddaje
    /// kilkadziesiąt, a kryteria mówią o tysiącach.
    pub tlum: usize,
    pub opad: Option<u8>,
    pub snieg: Option<u8>,
    pub blackout: u16,
    pub ciecie: u8,
    /// Czy scena **przewija pory roku** w oknie pomiaru (kryterium `seasons_do_not_remesh`).
    ///
    /// Sezon liczy się z ticku świata, a scena stoi na pauzie — bez przewijania licznik
    /// remeshingu jest zerem z konstrukcji i kryterium nie może zapalić się na czerwono.
    pub cykl_por_roku: bool,
}

impl Scena {
    /// Próg czasu klatki na maszynie pomiarowej.
    #[must_use]
    pub(crate) fn prog_ms(&self) -> f32 {
        self.target_ms * ZAPAS
    }

    /// Kamera sceny nad zadanym punktem świata.
    #[must_use]
    pub(crate) fn kamera(&self, cel: glam::DVec3) -> CameraState {
        match self.kadr {
            Kadr::Orbita { dist_m, pitch_deg } => CameraState {
                mode: CameraMode::Orbit {
                    target: cel,
                    dist: dist_m,
                    // Azymut ustalony, nie domyślny: kadr sceny ma być ten sam
                    // w każdym przebiegu, a `startowa_kamera` wolno komuś przestawić.
                    yaw: 0.6,
                    pitch: pitch_deg.to_radians(),
                },
                fov_deg: 35.0,
                ..CameraState::default()
            },
            Kadr::Ulica { wysokosc_m } => CameraState {
                mode: CameraMode::FirstPerson {
                    pos: glam::DVec3::new(cel.x, cel.y, cel.z + f64::from(wysokosc_m)),
                    yaw: 0.6,
                    pitch: 0.0,
                    anchor: None,
                    eye_height_m: wysokosc_m,
                },
                fov_deg: 60.0,
                ..CameraState::default()
            },
        }
    }
}

/// Siedem scen z §7.2 dokumentu fazy.
pub(crate) const SCENY: [Scena; 7] = [
    Scena {
        nazwa: "bench_street",
        target_ms: 16.6,
        godzina: "8:15",
        kadr: Kadr::Ulica { wysokosc_m: 1.7 },
        tlum: 6_000,
        opad: None,
        snieg: None,
        blackout: 0,
        ciecie: 0,
        cykl_por_roku: false,
    },
    Scena {
        nazwa: "bench_district",
        target_ms: 16.6,
        godzina: "8:15",
        kadr: Kadr::Orbita {
            dist_m: 180.0,
            pitch_deg: 35.0,
        },
        tlum: 24_000,
        opad: None,
        snieg: None,
        blackout: 0,
        ciecie: 0,
        cykl_por_roku: false,
    },
    Scena {
        nazwa: "bench_city",
        target_ms: 33.3,
        godzina: "12",
        kadr: Kadr::Orbita {
            dist_m: 1_400.0,
            pitch_deg: 60.0,
        },
        tlum: 0,
        opad: None,
        snieg: None,
        blackout: 0,
        ciecie: 0,
        cykl_por_roku: false,
    },
    Scena {
        nazwa: "bench_night_rain",
        target_ms: 16.6,
        godzina: "23",
        kadr: Kadr::Orbita {
            dist_m: 180.0,
            pitch_deg: 35.0,
        },
        tlum: 24_000,
        opad: Some(200),
        snieg: None,
        blackout: 0,
        ciecie: 0,
        cykl_por_roku: false,
    },
    Scena {
        nazwa: "bench_blackout",
        // Kryterium jest **względne** („≤ `night_rain`"), ale próg bezwzględny musi
        // istnieć, inaczej scena szybsza od siebie samej przechodziłaby zawsze.
        target_ms: 16.6,
        godzina: "23",
        kadr: Kadr::Orbita {
            dist_m: 180.0,
            pitch_deg: 35.0,
        },
        tlum: 24_000,
        opad: Some(200),
        snieg: None,
        blackout: 3,
        ciecie: 0,
        cykl_por_roku: false,
    },
    Scena {
        nazwa: "bench_interiors",
        target_ms: 16.6,
        godzina: "12",
        kadr: Kadr::Orbita {
            dist_m: 120.0,
            pitch_deg: 30.0,
        },
        tlum: 12_000,
        opad: None,
        snieg: None,
        blackout: 0,
        ciecie: 2,
        cykl_por_roku: false,
    },
    Scena {
        nazwa: "bench_winter",
        target_ms: 16.6,
        godzina: "12",
        kadr: Kadr::Orbita {
            dist_m: 180.0,
            pitch_deg: 35.0,
        },
        tlum: 24_000,
        opad: None,
        snieg: Some(200),
        blackout: 0,
        ciecie: 0,
        // §7.3: „przejście przez 4 pory roku, `chunk_remesh_count == 0`".
        cykl_por_roku: true,
    },
];

/// Pora roku sceny przewijającej sezony: pełny obieg czterech pór w oknie pomiaru.
///
/// Zmiana co ćwierć okna, a nie co klatkę: remeshing chunka jest asynchroniczny
/// i licznik potrzebuje kilkudziesięciu klatek, żeby go zobaczyć — przewijanie
/// co klatkę mierzyłoby, czy kolejka nadąża, a nie czy sezon dotyka geometrii.
#[must_use]
pub(crate) fn pora_roku(klatka: u32, klatek: u32) -> u8 {
    ((u64::from(klatka) * 4 / u64::from(klatek.max(1))) % 4) as u8
}

/// Scena o zadanej nazwie.
#[must_use]
pub(crate) fn scena(nazwa: &str) -> Option<&'static Scena> {
    SCENY.iter().find(|s| s.nazwa == nazwa)
}

/// Nazwy wszystkich scen, do komunikatu o błędzie.
#[must_use]
pub(crate) fn nazwy() -> String {
    SCENY.iter().map(|s| s.nazwa).collect::<Vec<_>>().join(", ")
}

/// Zbierane klatki jednego przebiegu.
pub(crate) struct Przebieg {
    pub scena: &'static Scena,
    pub katalog: std::path::PathBuf,
    /// Czas ściany klatki — to, co widzi gracz.
    pub klatka_ms: Vec<f32>,
    /// Czas GPU klatki, suma passów. Wychodzi co druga klatka, więc próbek jest mniej.
    pub gpu_ms: Vec<f32>,
    pub cpu_ms: Vec<f32>,
    pub pass_ms: Vec<[f32; magnat_render::PASS_NAMES.len()]>,
    /// Ostatnia pełna statystyka klatki — do sekcji `stats` raportu.
    pub stats: RenderStats,
    /// Szczyty, bo to one są budżetem: „≤ 1 500 draw calli" znaczy w każdej klatce.
    pub max_draw_calls: u32,
    pub max_triangles: usize,
    pub max_instances: usize,
    pub max_select_ms: f32,
    pub max_budynki_ms: f32,
    /// Licznik remeshingu w pierwszej klatce pomiaru — raport podaje **przyrost**.
    ///
    /// `chunk_remesh_count` w kliencie liczy od startu, a kryterium WP7 („pory roku
    /// nie dotykają geometrii") mówi o zerze **w oknie pomiaru**. Suma od startu jest
    /// zawsze dodatnia, bo miasto trzeba było raz zmeshować, więc kryterium z niej
    /// nie zapali się na zielono nigdy.
    pub remesh_na_starcie: Option<u32>,
    pub zapisane: u32,
}

impl Przebieg {
    /// Nowy przebieg. **Kasuje raport z poprzedniego przebiegu tej sceny**, i to nie jest
    /// porządkowanie: raporty są zacommitowane, a przebieg przerwany przed końcem nie
    /// zapisuje nic. Bez skasowania bramka porównywałaby wczorajszy plik z linią bazową
    /// wygenerowaną z tego samego pliku i meldowała „brak regresji" o scenie, która
    /// w ogóle się nie uruchomiła.
    #[must_use]
    pub(crate) fn nowy(scena: &'static Scena, katalog: std::path::PathBuf) -> Przebieg {
        let _ = std::fs::remove_file(katalog.join(format!("{}.json", scena.nazwa)));
        Przebieg {
            scena,
            katalog,
            klatka_ms: Vec::with_capacity(POMIAR as usize),
            gpu_ms: Vec::with_capacity(POMIAR as usize),
            cpu_ms: Vec::with_capacity(POMIAR as usize),
            pass_ms: Vec::with_capacity(POMIAR as usize),
            stats: RenderStats::default(),
            max_draw_calls: 0,
            max_triangles: 0,
            max_instances: 0,
            max_select_ms: 0.0,
            max_budynki_ms: 0.0,
            remesh_na_starcie: None,
            zapisane: 0,
        }
    }

    /// Dopisuje klatkę pomiaru.
    pub(crate) fn dodaj(&mut self, dt_s: f64, s: RenderStats) {
        self.klatka_ms.push((dt_s * 1000.0) as f32);
        self.cpu_ms.push(s.cpu_ms());
        // Do próbki wchodzą **wyłącznie klatki ze świeżym odczytem**. `pass_ms`
        // odbudowuje się co klatkę z ostatniego udanego odczytu, a ten wychodzi co drugą
        // klatkę — więc sam warunek „większe od zera" przepuszczał każdą klatkę i każdy
        // pomiar liczył się dwa razy. Percentylom to nie szkodziło, ale `gpu_samples`
        // w raporcie było zawyżone dwukrotnie i nie mówiło, czy pomiar w ogóle szedł.
        let gpu = s.gpu_ms();
        if s.frame.gpu_fresh && gpu > 0.0 {
            self.gpu_ms.push(gpu);
            self.pass_ms.push(s.frame.pass_ms);
        }
        self.max_draw_calls = self.max_draw_calls.max(s.draw_calls());
        self.max_triangles = self.max_triangles.max(s.frame.triangles);
        self.max_instances = self.max_instances.max(s.frame.instances);
        self.max_select_ms = self.max_select_ms.max(s.snapshot_select_ms);
        self.max_budynki_ms = self.max_budynki_ms.max(s.building_query_ms);
        self.remesh_na_starcie.get_or_insert(s.chunk_remesh_count);
        self.stats = s;
        self.zapisane += 1;
    }

    /// Czy scena zmieściła się w progu. Metryką jest **p95 czasu GPU**, bo to ona
    /// stoi w §7.2 — czas ściany niesie też wsync i koszt symulacji.
    #[must_use]
    pub(crate) fn p95_gpu_ms(&self) -> f32 {
        crate::bench::percentyl_ms(&self.gpu_ms, 0.95)
    }

    #[must_use]
    pub(crate) fn miesci_sie(&self) -> bool {
        let p95 = self.p95_gpu_ms();
        p95 > 0.0 && p95 <= self.scena.prog_ms()
    }

    /// Raport sceny w JSON. Pisany ręcznie, bo to czterdzieści linii, a `serde_json`
    /// byłby nową zależnością workspace'u dla jednego pliku (PRD §16.1: granica
    /// bibliotek zewnętrznych jest decyzją, nie odruchem).
    #[must_use]
    pub(crate) fn json(&self, swiat: &str) -> String {
        let mut o = String::with_capacity(4096);
        let kw = |v: &[f32]| {
            (
                crate::bench::percentyl_ms(v, 0.5),
                crate::bench::percentyl_ms(v, 0.95),
                crate::bench::percentyl_ms(v, 0.99),
            )
        };
        let (g50, g95, g99) = kw(&self.gpu_ms);
        let (c50, c95, c99) = kw(&self.cpu_ms);
        let (f50, f95, f99) = kw(&self.klatka_ms);
        let _ = writeln!(o, "{{");
        let _ = writeln!(o, "  \"scene\": \"{}\",", self.scena.nazwa);
        let _ = writeln!(o, "  \"machine\": \"{MASZYNA}\",");
        let _ = writeln!(o, "  \"target_machine\": \"{MASZYNA_DOCELOWA}\",");
        let _ = writeln!(o, "  \"headroom_factor\": {ZAPAS},");
        let _ = writeln!(o, "  \"target_ms\": {:.2},", self.scena.target_ms);
        let _ = writeln!(o, "  \"threshold_ms\": {:.2},", self.scena.prog_ms());
        let _ = writeln!(o, "  \"frames\": {},", self.zapisane);
        let _ = writeln!(o, "  \"gpu_samples\": {},", self.gpu_ms.len());
        let _ = writeln!(
            o,
            "  \"gpu_ms\": {{ \"p50\": {g50:.3}, \"p95\": {g95:.3}, \"p99\": {g99:.3} }},"
        );
        let _ = writeln!(
            o,
            "  \"cpu_ms\": {{ \"p50\": {c50:.3}, \"p95\": {c95:.3}, \"p99\": {c99:.3} }},"
        );
        let _ = writeln!(
            o,
            "  \"frame_ms\": {{ \"p50\": {f50:.3}, \"p95\": {f95:.3}, \"p99\": {f99:.3} }},"
        );
        let _ = writeln!(o, "  \"pass_ms\": {{");
        for (i, nazwa) in magnat_render::PASS_NAMES.iter().enumerate() {
            let v: Vec<f32> = self.pass_ms.iter().map(|p| p[i]).collect();
            let (p50, p95, _) = kw(&v);
            let przecinek = if i + 1 == magnat_render::PASS_NAMES.len() {
                ""
            } else {
                ","
            };
            let _ = writeln!(
                o,
                "    \"{nazwa}\": {{ \"p50\": {p50:.3}, \"p95\": {p95:.3} }}{przecinek}"
            );
        }
        let _ = writeln!(o, "  }},");
        let s = &self.stats;
        let _ = writeln!(o, "  \"stats\": {{");
        let _ = writeln!(o, "    \"draw_calls_max\": {},", self.max_draw_calls);
        let _ = writeln!(o, "    \"triangles_max\": {},", self.max_triangles);
        let _ = writeln!(o, "    \"instances_max\": {},", self.max_instances);
        let l = s.instances_by_lod();
        let _ = writeln!(
            o,
            "    \"instances_by_lod\": [{}, {}, {}, {}],",
            l[0], l[1], l[2], l[3]
        );
        let _ = writeln!(o, "    \"instance_batches\": {},", s.frame.instance_batches);
        let _ = writeln!(o, "    \"chunks_drawn\": {},", s.frame.chunks_drawn);
        let _ = writeln!(
            o,
            "    \"weather_particles\": {},",
            s.frame.weather_particles
        );
        let _ = writeln!(
            o,
            "    \"chunk_remesh_count\": {},",
            s.chunk_remesh_count
                .saturating_sub(self.remesh_na_starcie.unwrap_or(0))
        );
        let _ = writeln!(o, "    \"voices_active\": {},", s.voices_active);
        let _ = writeln!(o, "    \"lights\": {},", s.lights);
        let _ = writeln!(
            o,
            "    \"snapshot_select_ms_max\": {:.4},",
            self.max_select_ms
        );
        let _ = writeln!(
            o,
            "    \"building_query_ms_max\": {:.4},",
            self.max_budynki_ms
        );
        // `impostor_resident` i `impostor_regen` **nie wchodzą do raportu**, dopóki
        // nie mają pisarza: impostory dzielnic są w `M12a`/WP4 (`H-10`), więc byłyby
        // parą zer wyglądających jak pomiar. Pola zostają w `RenderStats`, bo to ich
        // przyszły adres; wiersz w JSON-ie dopisze faza, która je wypełni.
        let _ = writeln!(o, "    \"lod_scale\": {:.3}", s.lod_scale);
        let _ = writeln!(o, "  }},");
        let _ = writeln!(o, "  \"world\": {swiat},");
        let _ = writeln!(
            o,
            "  \"verdict\": \"{}\"",
            if self.miesci_sie() { "ok" } else { "over" }
        );
        let _ = writeln!(o, "}}");
        o
    }

    /// Zapisuje raport i wypisuje werdykt. Zwraca `true`, gdy scena mieści się w progu.
    pub(crate) fn zapisz(&self, swiat: &str) -> bool {
        let sciezka = self.katalog.join(format!("{}.json", self.scena.nazwa));
        if let Some(rodzic) = sciezka.parent() {
            let _ = std::fs::create_dir_all(rodzic);
        }
        match std::fs::write(&sciezka, self.json(swiat)) {
            Ok(()) => eprintln!("raport: {}", sciezka.display()),
            Err(e) => eprintln!("zapis raportu nieudany: {e}"),
        }
        let p95 = self.p95_gpu_ms();
        let ok = self.miesci_sie();
        if p95 <= 0.0 {
            eprintln!(
                "{}: brak pomiaru GPU (sterownik bez TIMESTAMP_QUERY) — progu nie da się ocenić",
                self.scena.nazwa
            );
            return false;
        }
        eprintln!(
            "{}: GPU p95 {p95:.2} ms wobec progu {:.2} ms ({} na {MASZYNA}) — {}",
            self.scena.nazwa,
            self.scena.prog_ms(),
            if self.scena.target_ms < 20.0 {
                "60 FPS"
            } else {
                "30 FPS"
            },
            if ok { "ok" } else { "PRZEKROCZONY" }
        );
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn katalog_pokrywa_sceny_odniesienia() {
        // §7.2 wymienia siedem scen. Brakująca scena znaczy kryterium, które nie
        // ma jak zapalić się ani na zielono, ani na czerwono.
        for wymagana in [
            "bench_street",
            "bench_district",
            "bench_city",
            "bench_night_rain",
            "bench_blackout",
            "bench_interiors",
            "bench_winter",
        ] {
            assert!(scena(wymagana).is_some(), "brak sceny {wymagana}");
        }
    }

    #[test]
    fn progi_sa_zaostrzone_wobec_prd() {
        // Mierzymy na karcie szybszej niż ta z §20.2, więc próg musi być ostrzejszy
        // od celu — inaczej „60 FPS na średniej klasie" nie jest pokryte niczym.
        for s in &SCENY {
            assert!(s.prog_ms() < s.target_ms, "{}", s.nazwa);
        }
        assert!((scena("bench_district").unwrap().prog_ms() - 9.13).abs() < 0.01);
        assert!((scena("bench_city").unwrap().prog_ms() - 18.31).abs() < 0.01);
    }

    #[test]
    fn scena_zimowa_przewija_wszystkie_cztery_pory_roku() {
        // Kryterium §7.3 mówi o **przejściu przez cztery pory roku**. Gdyby przewijanie
        // oddawało trzy albo tę samą przez cały czas, licznik remeshingu byłby zerem
        // z konstrukcji i bramka świeciłaby na zielono bez powodu.
        let widziane: std::collections::BTreeSet<u8> =
            (0..POMIAR).map(|k| pora_roku(k, POMIAR)).collect();
        assert_eq!(widziane.len(), 4, "{widziane:?}");
        assert!(scena("bench_winter").unwrap().cykl_por_roku);
        assert!(!scena("bench_district").unwrap().cykl_por_roku);
    }

    #[test]
    fn werdykt_wymaga_pomiaru_a_nie_jego_braku() {
        // Scena bez ani jednej próbki GPU **nie przechodzi**. Zero jako „zmieściło się"
        // byłoby zielonym wynikiem na maszynie, która niczego nie zmierzyła.
        let p = Przebieg::nowy(scena("bench_city").unwrap(), std::path::PathBuf::new());
        assert!(!p.miesci_sie());
    }

    #[test]
    fn raport_jest_poprawnym_jsonem_i_niesie_werdykt() {
        let mut p = Przebieg::nowy(scena("bench_district").unwrap(), std::path::PathBuf::new());
        let mut s = RenderStats::default();
        s.frame.pass_ms[3] = 5.0;
        s.frame.draw_calls = 1200;
        // Bez tego klatka jest kopią poprzedniego odczytu i do próbki nie wchodzi —
        // raport wyszedłby bez ani jednego pomiaru, czyli z werdyktem „over".
        s.frame.gpu_fresh = true;
        for _ in 0..10 {
            p.dodaj(0.016, s);
        }
        let j = p.json("{}");
        assert!(j.contains("\"scene\": \"bench_district\""));
        assert!(j.contains("\"verdict\": \"ok\""), "{j}");
        assert!(j.contains("\"pick_id\""), "raport musi mieć ósmy pass");
        // Nawiasy klamrowe muszą się bilansować — raport czyta skrypt bramki.
        let otw = j.chars().filter(|c| *c == '{').count();
        let zam = j.chars().filter(|c| *c == '}').count();
        assert_eq!(otw, zam, "{j}");
    }
}
