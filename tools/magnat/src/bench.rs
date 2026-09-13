//! Scena pomiarowa i raport wydajności renderu (M1 §7.4, WP-X1).
//!
//! Przelot jest **ustalony**: te same etapy, ten sam czas, ten sam seed. Dopiero wtedy
//! dwa uruchomienia da się porównać, a §7.4 wymaga porównywania między milestone'ami,
//! nie oglądania jednej liczby.
//!
//! Metryką jest czas klatki, nie „FPS": średnia z FPS-ów kłamie (jest średnią odwrotności),
//! a przy ocenie płynności i tak liczy się ogon rozkładu. Stąd mediana i **1 % low** —
//! czyli percentyl 99 czasu klatki wyrażony w klatkach na sekundę. To jest ta sama metryka,
//! którą podaje §20.2, i dlatego jest tu liczona dokładnie tak.

use std::time::Duration;

/// Etap przelotu: dokąd patrzy kamera i jak długo trwa.
#[derive(Clone, Copy)]
pub struct Etap {
    pub nazwa: &'static str,
    /// Wysokość orbity w metrach.
    pub dist_m: f32,
    /// Ile metrów kamera przesuwa się w poziomie przez cały etap.
    pub przesuniecie_m: f64,
    /// O ile radianów obraca się orbita przez cały etap.
    pub obrot_rad: f32,
    /// Próg mediany dla tego etapu w klatkach na sekundę (§20.2); `None` = etap przejściowy.
    pub prog_mediana_fps: Option<f32>,
    /// Próg 1 % low w klatkach na sekundę.
    pub prog_low_fps: Option<f32>,
}

/// Pięć etapów z §7.4: orbita miasta → dzielnica → poziom ulicy → przelot 2 km → orbita.
pub const ETAPY: [Etap; 5] = [
    Etap {
        nazwa: "widok miasta",
        dist_m: 2200.0,
        przesuniecie_m: 0.0,
        obrot_rad: 0.8,
        prog_mediana_fps: Some(30.0),
        prog_low_fps: Some(24.0),
    },
    Etap {
        nazwa: "widok dzielnicy",
        dist_m: 450.0,
        przesuniecie_m: 200.0,
        obrot_rad: 0.5,
        prog_mediana_fps: Some(60.0),
        prog_low_fps: Some(45.0),
    },
    Etap {
        nazwa: "poziom ulicy",
        dist_m: 35.0,
        przesuniecie_m: 120.0,
        obrot_rad: 0.5,
        prog_mediana_fps: Some(60.0),
        prog_low_fps: Some(45.0),
    },
    Etap {
        nazwa: "przelot 2 km",
        dist_m: 140.0,
        przesuniecie_m: 2000.0,
        obrot_rad: 0.0,
        // Przelot jest etapem przejściowym: jego kryterium to brak zacięć (§7.4),
        // a nie próg FPS — kamera przechodzi tu przez wszystkie poziomy szczegółowości
        // i strumieniowanie dokłada materializację do każdej klatki.
        prog_mediana_fps: None,
        prog_low_fps: None,
    },
    Etap {
        nazwa: "powrót na orbitę",
        dist_m: 2200.0,
        przesuniecie_m: 0.0,
        obrot_rad: 0.6,
        prog_mediana_fps: Some(30.0),
        prog_low_fps: Some(24.0),
    },
];

/// Zebrane czasy klatek jednego etapu.
#[derive(Default)]
pub struct Pomiar {
    pub ms: Vec<f32>,
    pub chunki: usize,
    pub trojkaty: usize,
}

impl Pomiar {
    pub fn dodaj(&mut self, dt: Duration, chunki: usize, trojkaty: usize) {
        self.ms.push(dt.as_secs_f32() * 1000.0);
        self.chunki = self.chunki.max(chunki);
        self.trojkaty = self.trojkaty.max(trojkaty);
    }

    /// Mediana czasu klatki w klatkach na sekundę.
    #[must_use]
    pub fn mediana_fps(&self) -> f32 {
        self.percentyl_fps(0.5)
    }

    /// 1 % low: percentyl 99 czasu klatki, czyli jedna najgorsza klatka na sto.
    #[must_use]
    pub fn low_fps(&self) -> f32 {
        self.percentyl_fps(0.99)
    }

    fn percentyl_fps(&self, q: f32) -> f32 {
        if self.ms.is_empty() {
            return 0.0;
        }
        let mut v = self.ms.clone();
        v.sort_by(f32::total_cmp);
        // Zaokrąglenie **w górę**: przy stu próbkach 1 % low ma pokazać tę jedną najgorszą
        // klatkę, a nie przedostatnią. Obcięcie w dół wypadało na indeksie 98 i zgłaszało
        // wynik o rząd wielkości lepszy niż rzeczywisty.
        let i = ((((v.len() - 1) as f32) * q).ceil() as usize).min(v.len() - 1);
        1000.0 / v[i].max(0.001)
    }

    #[must_use]
    pub fn zaciecia(&self) -> usize {
        self.ms.iter().filter(|ms| **ms > 33.0).count()
    }
}

/// Wiersz raportu dla jednego etapu wraz z werdyktem wobec progów §20.2.
#[must_use]
pub fn wiersz(etap: &Etap, p: &Pomiar) -> String {
    let werdykt = |prog: Option<f32>, wartosc: f32| -> String {
        match prog {
            Some(pr) if wartosc + 0.5 < pr => format!(" (próg {pr:.0} — NIESPEŁNIONY)"),
            Some(pr) => format!(" (próg {pr:.0} — ok)"),
            None => String::new(),
        }
    };
    format!(
        "{:<18} mediana {:6.1} FPS{}, 1 % low {:6.1} FPS{}, zacięć > 33 ms: {}, szczyt {} chunków / {} tys. trójkątów",
        etap.nazwa,
        p.mediana_fps(),
        werdykt(etap.prog_mediana_fps, p.mediana_fps()),
        p.low_fps(),
        werdykt(etap.prog_low_fps, p.low_fps()),
        p.zaciecia(),
        p.chunki,
        p.trojkaty / 1000,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentyle_licza_sie_z_czasu_klatki_a_nie_ze_sredniej_fps() {
        // Dziewięćdziesiąt dziewięć klatek po 10 ms i jedna po 100 ms: średnia FPS
        // wyszłaby ~99, a 1 % low ma pokazać tę jedną złą klatkę, czyli 10 FPS.
        let mut p = Pomiar::default();
        for _ in 0..99 {
            p.dodaj(Duration::from_micros(10_000), 0, 0);
        }
        p.dodaj(Duration::from_micros(100_000), 0, 0);
        assert!((p.mediana_fps() - 100.0).abs() < 1.0, "{}", p.mediana_fps());
        assert!((p.low_fps() - 10.0).abs() < 1.0, "{}", p.low_fps());
        assert_eq!(p.zaciecia(), 1);
    }

    #[test]
    fn scenariusz_pokrywa_wymagane_widoki() {
        // §7.4 wymienia widok miasta, widok dzielnicy, poziom ulicy i przelot 2 km.
        // Gdyby któryś wypadł ze scenariusza, raport przestałby odpowiadać na pytanie,
        // które zadaje §20.2.
        let nazwy: Vec<&str> = ETAPY.iter().map(|e| e.nazwa).collect();
        for wymagany in [
            "widok miasta",
            "widok dzielnicy",
            "poziom ulicy",
            "przelot 2 km",
        ] {
            assert!(nazwy.contains(&wymagany), "brak etapu „{wymagany}”");
        }
        let droga: f64 = ETAPY.iter().map(|e| e.przesuniecie_m).sum();
        assert!(droga >= 2000.0, "przelot krótszy niż 2 km: {droga} m");
    }
}
