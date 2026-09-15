//! `calibrate-vdf` — kalibracja diagramu podstawowego do parametrów IDM (M4d §5.4, WP9).
//!
//! **Jedyny kanał mikro → symulacja, i jest offline.** Warstwa Mikro nie ma prawa
//! zapisu do stanu gry (00 §4); jedyne, co wolno jej powiedzieć ekonomii, przechodzi
//! przez to narzędzie, ręcznie uruchomione, a wynik ląduje w `data/roads/vdf.ron`
//! i jest widoczny w diffie. Nie ma tu żadnej ścieżki „w czasie gry".
//!
//! **Domyślnie narzędzie nic nie zapisuje**, tylko porównuje i wydaje werdykt.
//! Powód jest konkretny, nie ostrożnościowy: `min_speed_dkmh` w `vdf.ron` ma oparcie
//! w pomiarze z M4b (`L-16`: jeden pas zjeżdża z kolejki ~30 pojazdów na minutę),
//! a parametry IDM go nie mają. Przepisanie tabeli zmienia **czasy przejazdu całego
//! miasta**, czyli pieniądze — i ma być decyzją człowieka, a nie skutkiem ubocznym
//! uruchomienia kalibratora. `--write` robi to na żądanie.
//!
//! `N-4`: kalibracja zaczyna się od **prędkości**, nie od przepustowości pasa —
//! `capacity_vph_per_lane` nie wchodzi do wzoru prędkości w ogóle.

use clap::Args as ClapArgs;
use magnat_core::RoadClass;
use magnat_traffic::{idm_speed_dkmh, IdmParams, VdfTable, CALIBRATION_VEHICLE_CM};
use std::process::ExitCode;

#[derive(ClapArgs, Debug)]
pub struct CalibrateArgs {
    /// Przepisz `min_speed_dkmh` w `data/roads/vdf.ron` wartościami z IDM.
    #[arg(long, default_value_t = false)]
    pub write: bool,
    /// Dopuszczalny rozjazd w procentach, powyżej którego narzędzie kończy błędem.
    #[arg(long, default_value_t = 25)]
    pub tolerance_pct: u32,
}

/// Prędkości swobodne klas — te same, które generator miasta wpisuje krawędziom
/// (`sim/world::city::road::SPECS`). Powtórzone tutaj, bo kalibracja dotyczy klasy
/// drogi, a nie konkretnego miasta: jednorodny odcinek nie ma rozkładu limitów.
const KLASY: [(RoadClass, u16); 5] = [
    (RoadClass::Highway, 1_200),
    (RoadClass::Arterial, 600),
    (RoadClass::Collector, 500),
    (RoadClass::Local, 300),
    (RoadClass::Service, 200),
];

/// Kubełki zapełnienia, w których pokazujemy obie krzywe. Ostatni jest tym, o który
/// toczy się gra: to on wyznacza tempo rozładowania korka.
const KUBELKI: [u32; 7] = [100, 200, 350, 500, 700, 850, 1000];

pub fn run(a: &CalibrateArgs) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let vdf = VdfTable::load_default()?;
    let idm = IdmParams::load_default()?;
    let jam = vdf.jam_spacing_cm();

    println!("kalibracja diagramu podstawowego (M4d §5.4, WP9)");
    println!(
        "  IDM: s0 {} cm, T {:.1} s, a {} cm/s², b {} cm/s²",
        idm.s0_cm,
        f64::from(idm.headway_ds) / 10.0,
        idm.accel_cms2,
        idm.decel_cms2
    );
    println!(
        "  odstęp korkowy {jam} cm, pojazd odniesienia {CALIBRATION_VEHICLE_CM} cm, \
         luka przy zapełnieniu {} cm",
        jam.saturating_sub(CALIBRATION_VEHICLE_CM)
    );
    println!();
    println!("zapełnienie ‰ →   100   200   350   500   700   850  1000   (dkmh: VDF / IDM)");

    let mut najgorszy = 0.0f64;
    let mut najgorsza_klasa = "";
    let mut nowe: Vec<(RoadClass, u16)> = Vec::new();

    for (c, free) in KLASY {
        let mut wiersz_vdf = String::new();
        let mut wiersz_idm = String::new();
        for d in KUBELKI {
            // `speed_dkmh` bierze obłożenie i pojemność; kubełek promilowy odwzorowuje
            // się na parę (obłożenie, pojemność) = (d, 1000).
            let v = vdf.speed_dkmh(c, free, d as u16, 1000);
            let i = idm_speed_dkmh(d, jam, CALIBRATION_VEHICLE_CM, free, &idm);
            wiersz_vdf.push_str(&format!("{v:>6}"));
            wiersz_idm.push_str(&format!("{i:>6}"));
        }
        let v_jam = vdf.speed_dkmh(c, free, 1000, 1000);
        let i_jam = idm_speed_dkmh(1000, jam, CALIBRATION_VEHICLE_CM, free, &idm);
        let blad = (f64::from(i_jam) - f64::from(v_jam)).abs() / f64::from(v_jam.max(1)) * 100.0;
        if blad > najgorszy {
            najgorszy = blad;
            najgorsza_klasa = c.key();
        }
        nowe.push((c, i_jam));
        println!("{:>12} VDF {wiersz_vdf}", c.key());
        println!("{:>12} IDM {wiersz_idm}   rozjazd przy zapełnieniu {blad:.0} %", "");
    }

    println!();
    println!("największy rozjazd: {najgorszy:.0} % (klasa {najgorsza_klasa})");

    if a.write {
        let sciezka = magnat_core::data_path("roads/vdf.ron");
        let tekst = std::fs::read_to_string(&sciezka)?;
        let mut wynik = tekst.clone();
        let mut zmian = 0u32;
        for (c, v) in &nowe {
            // Podmieniamy **wyłącznie** jedną liczbę w wierszu klasy, żeby komentarze
            // w pliku przeżyły: to one tłumaczą, skąd wzięły się pozostałe kolumny.
            let igla = format!("(class: \"{}\",", c.key());
            let Some(start) = wynik.find(&igla) else {
                continue;
            };
            let Some(koniec) = wynik[start..].find(')') else {
                continue;
            };
            let wiersz = &wynik[start..start + koniec];
            let Some(poz) = wiersz.find("min_speed_dkmh:") else {
                continue;
            };
            let od = start + poz + "min_speed_dkmh:".len();
            let dlugosc = wynik[od..start + koniec]
                .find(|ch: char| !ch.is_ascii_digit() && !ch.is_whitespace())
                .unwrap_or(koniec - poz - "min_speed_dkmh:".len());
            let nowy = format!(" {v}");
            wynik.replace_range(od..od + dlugosc, &nowy);
            zmian += 1;
        }
        std::fs::write(&sciezka, wynik)?;
        println!("zapisano {zmian} wierszy do {}", sciezka.display());
        println!(
            "UWAGA: to zmienia czasy przejazdu całego miasta, czyli pieniądze. \
             Przelicz scenariusz odniesienia (`headless m3day`) przed commitem."
        );
        return Ok(ExitCode::SUCCESS);
    }

    if najgorszy > f64::from(a.tolerance_pct) {
        eprintln!(
            "rozjazd {najgorszy:.0} % przekracza tolerancję {} % — animacja przestała \
             pasować do księgowości (ryzyko R2). Albo popraw `data/roads/idm.ron`, albo \
             uruchom to samo z `--write` i przelicz scenariusz odniesienia.",
            a.tolerance_pct
        );
        return Ok(ExitCode::FAILURE);
    }
    println!("werdykt: w tolerancji {} %", a.tolerance_pct);
    Ok(ExitCode::SUCCESS)
}
