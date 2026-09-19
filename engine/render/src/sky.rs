//! Słońce i niebo (PRD §15.3, M1 WP-R4).
//!
//! Pozycja słońca jest wyprowadzana z `SimMinute` przez **kalendarz 360-dniowy** (00 §K-1):
//! dzień roku daje deklinację, minuta doby kąt godzinny, a szerokość geograficzna regionu
//! zamienia oba na azymut i wysokość. To jest jedyne miejsce w renderze, które musi znać
//! kalendarz — i dlatego bierze go z `SimCalendar`, a nie liczy z minut po swojemu.
//!
//! Barwa nieba i słońca pochodzi z **tabeli 64 wpisów po wysokości słońca**, interpolowanej
//! liniowo. Nie ma tu modelu Preethama ani Hoseka i nie będzie: tabela wygląda dobrze,
//! jest darmowa i — co ważniejsze — da się ją stroić, oglądając wynik, a nie przestawiając
//! współczynniki rozpraszania (M1 §5.8).

use glam::Vec3;
use magnat_core::{SimCalendar, SimMinute};

/// Nachylenie osi obrotu w stopniach.
const AXIAL_TILT_DEG: f32 = 23.44;
/// Dzień roku, w którym deklinacja przechodzi przez zero rosnąco (równonoc wiosenna).
/// W kalendarzu 360-dniowym wypada na dzień 80 — tak samo jak w gregoriańskim,
/// bo miesiące są równe, a nie dlatego, że tak wyszło.
const EQUINOX_DAY: f32 = 80.0;

/// Kierunek do słońca i jego wysokość nad horyzontem.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SunState {
    /// Wektor **do** słońca, znormalizowany. Oś Z jest pionem świata.
    pub direction: Vec3,
    /// Wysokość nad horyzontem w stopniach; ujemna po zachodzie.
    pub elevation_deg: f32,
    pub azimuth_deg: f32,
}

impl SunState {
    #[must_use]
    pub fn is_day(&self) -> bool {
        self.elevation_deg > 0.0
    }
}

/// Pozycja słońca dla danej minuty symulacji i szerokości geograficznej.
///
/// `latitude_ddeg` w dziesiątych częściach stopnia — ta sama jednostka, w której trzyma ją
/// `Region` w `sim/world`, żeby nie było dwóch konwencji na jedną wielkość.
#[must_use]
pub fn sun_state(minute: SimMinute, latitude_ddeg: i16) -> SunState {
    let cal = SimCalendar::from_minute(minute);
    let day = cal.day_of_year() as f32;
    let minute_of_day = cal.minute_of_day() as f32;

    // Deklinacja: δ = 23,44° · sin(2π · (d − 80) / 360).
    let decl =
        AXIAL_TILT_DEG.to_radians() * ((day - EQUINOX_DAY) / 360.0 * std::f32::consts::TAU).sin();
    // Kąt godzinny: 0 w południe, ±180° o północy.
    let hour_angle = (minute_of_day / 1440.0 * std::f32::consts::TAU) - std::f32::consts::PI;
    let lat = (f32::from(latitude_ddeg) / 10.0).to_radians();

    let sin_elev = lat.sin() * decl.sin() + lat.cos() * decl.cos() * hour_angle.cos();
    let elev = sin_elev.clamp(-1.0, 1.0).asin();

    let cos_elev = elev.cos().max(1e-4);
    let cos_az = (decl.sin() - lat.sin() * sin_elev) / (lat.cos().max(1e-4) * cos_elev);
    let az = cos_az.clamp(-1.0, 1.0).acos();
    // Przed południem słońce jest na wschodzie, po południu na zachodzie.
    let az = if hour_angle > 0.0 {
        std::f32::consts::TAU - az
    } else {
        az
    };

    // Układ świata: X na wschód, Y na północ, Z w górę. Azymut liczony od północy.
    let direction =
        Vec3::new(cos_elev * az.sin(), cos_elev * az.cos(), elev.sin()).normalize_or_zero();

    SunState {
        direction,
        elevation_deg: elev.to_degrees(),
        azimuth_deg: az.to_degrees(),
    }
}

/// Ułamek światła dziennego 0..=255 — czy w mieście jest jasno (M11d §5.8).
///
/// Wyprowadzone z **tej samej** wysokości słońca, którą dostaje niebo i cienie, i to
/// jest cały powód, dla którego ta funkcja mieszka tutaj, a nie po stronie wypełniacza
/// snapshotu: druga kopia deklinacji i kąta godzinnego rozjechałaby się z pierwszą,
/// a objawem byłyby okna zapalające się w biały dzień.
///
/// Granice są zmierzchem **cywilnym**, nie geometrycznym: przy −6° da się jeszcze czytać
/// gazetę, więc latarnie mają się zapalać po tym progu, a nie w chwili, gdy tarcza
/// dotknie horyzontu. Powyżej +3° jest pełny dzień — między tymi dwiema wartościami
/// ramp jest liniowy, bo cała krzywa barwy i tak siedzi w tabeli nieba.
#[must_use]
pub fn daylight(minute: SimMinute, latitude_ddeg: i16) -> u8 {
    const NOC_DEG: f32 = -6.0;
    const DZIEN_DEG: f32 = 3.0;
    let e = sun_state(minute, latitude_ddeg).elevation_deg;
    let t = ((e - NOC_DEG) / (DZIEN_DEG - NOC_DEG)).clamp(0.0, 1.0);
    (t * 255.0) as u8
}

/// Wpis tabeli nieba.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SkySample {
    /// Barwa i natężenie światła słonecznego (liniowe, HDR).
    pub sun_color: Vec3,
    /// Barwa nieba w zenicie.
    pub zenith: Vec3,
    /// Barwa nieba przy horyzoncie.
    pub horizon: Vec3,
    /// Światło odbite od gruntu — druga strefa ambientu.
    pub ground: Vec3,
}

/// Tabela nieba: 64 wpisy po wysokości słońca od −18° (koniec zmierzchu żeglarskiego)
/// do +90°. Wartości są wyprowadzane z prostego modelu ekstynkcji — nie po to, żeby
/// były fizyczne, tylko żeby dało się je stroić jednym parametrem zamiast sześćdziesięcioma
/// czterema ręcznie wpisanymi trójkami.
pub const SKY_LUT_SIZE: usize = 64;
const SKY_ELEV_MIN: f32 = -18.0;
const SKY_ELEV_MAX: f32 = 90.0;

#[must_use]
pub fn sky_lut() -> [SkySample; SKY_LUT_SIZE] {
    std::array::from_fn(|i| {
        let elev =
            SKY_ELEV_MIN + (SKY_ELEV_MAX - SKY_ELEV_MIN) * i as f32 / (SKY_LUT_SIZE - 1) as f32;
        sky_sample(elev)
    })
}

/// Wygładzone przejście `0 → 1` na przedziale jednostkowym.
///
/// Nie zwykłe zaciśnięcie: `clamp` daje funkcję ciągłą, ale z **załamaniem** pochodnej
/// na obu końcach. Przy dobie skróconej do 30 sekund (§7) załamanie widać jako zmianę
/// tempa rozjaśniania w połowie świtu — obraz nie przeskakuje, ale „szarpie".
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Barwa nieba dla zadanej wysokości słońca.
fn sky_sample(elev_deg: f32) -> SkySample {
    // Noc: ciemny granat i zero słońca. Zmierzch cywilny (0…−6°) gaśnie płynnie.
    let dzien = smoothstep((elev_deg + 6.0) / 6.0);
    // Grubość masy atmosferycznej: przy słońcu nisko światło przechodzi dłuższą drogę,
    // więc traci niebieski — stąd czerwony zachód bez żadnego modelu rozpraszania.
    let nisko = smoothstep(1.0 - (elev_deg / 25.0).clamp(0.0, 1.0));

    let sun_base = Vec3::new(1.0, 0.96, 0.90);
    let sun_low = Vec3::new(1.0, 0.55, 0.25);
    let sun_color = sun_base.lerp(sun_low, nisko) * (dzien * (0.15 + 0.85 * dzien));

    let zenit_dzien = Vec3::new(0.18, 0.33, 0.62);
    let zenit_noc = Vec3::new(0.012, 0.018, 0.040);
    let horyzont_dzien = Vec3::new(0.52, 0.62, 0.78);
    let horyzont_zachod = Vec3::new(0.85, 0.45, 0.28);
    let horyzont_noc = Vec3::new(0.03, 0.04, 0.07);

    let zenith = zenit_noc.lerp(zenit_dzien, dzien);
    let horizon = horyzont_noc.lerp(horyzont_dzien.lerp(horyzont_zachod, nisko), dzien);
    // Grunt odbija to, co go oświetla, z przewagą ciepłych składowych.
    let ground = (horizon * 0.35 + sun_color * 0.15) * Vec3::new(1.0, 0.92, 0.80);

    SkySample {
        sun_color,
        zenith,
        horizon,
        ground,
    }
}

/// Interpolacja tabeli dla konkretnej wysokości słońca.
#[must_use]
pub fn sample_sky(lut: &[SkySample; SKY_LUT_SIZE], elev_deg: f32) -> SkySample {
    let t = ((elev_deg - SKY_ELEV_MIN) / (SKY_ELEV_MAX - SKY_ELEV_MIN)).clamp(0.0, 1.0)
        * (SKY_LUT_SIZE - 1) as f32;
    let i = t.floor() as usize;
    let j = (i + 1).min(SKY_LUT_SIZE - 1);
    let f = t - i as f32;
    let a = lut[i];
    let b = lut[j];
    SkySample {
        sun_color: a.sun_color.lerp(b.sun_color, f),
        zenith: a.zenith.lerp(b.zenith, f),
        horizon: a.horizon.lerp(b.horizon, f),
        ground: a.ground.lerp(b.ground, f),
    }
}

/// Ekspozycja z krzywej pory dnia (M1 §5.8: bez auto-ekspozycji — krzywa jest przewidywalna
/// i nie miga, a automat na wejściu do tunelu robi pompowanie jasności, którego nikt nie chce).
#[must_use]
pub fn exposure(elev_deg: f32) -> f32 {
    // W dzień 1,0; w nocy oko przywyka, więc podnosimy do ~4× — ale nie więcej, bo noc
    // ma zostać nocą.
    let dzien = smoothstep((elev_deg + 4.0) / 10.0);
    4.0 - 3.0 * dzien
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minuta odpowiadająca zadanemu dniu roku i godzinie.
    fn minute(day: u64, hour: u64) -> SimMinute {
        SimMinute(day * 1440 + hour * 60)
    }

    #[test]
    fn slonce_wschodzi_i_zachodzi_w_ciagu_doby() {
        let lat = 521; // 52,1°
        let mut nad = 0;
        let mut pod = 0;
        for h in 0..24 {
            let s = sun_state(minute(170, h), lat);
            if s.is_day() {
                nad += 1;
            } else {
                pod += 1;
            }
        }
        assert!(nad > 0 && pod > 0, "doba bez wschodu albo bez zachodu");
        // Czerwiec na 52° — dzień wyraźnie dłuższy od nocy.
        assert!(
            nad > pod,
            "letni dzień krótszy od nocy: {nad} h wobec {pod} h"
        );
    }

    #[test]
    fn poludnie_jest_najwyzszym_punktem_doby() {
        let lat = 521;
        let poludnie = sun_state(minute(170, 12), lat).elevation_deg;
        for h in [0u64, 3, 6, 9, 15, 18, 21] {
            let e = sun_state(minute(170, h), lat).elevation_deg;
            assert!(
                poludnie >= e,
                "o {h}:00 słońce jest wyżej ({e}°) niż w południe ({poludnie}°)"
            );
        }
    }

    #[test]
    fn lato_jest_wyzsze_od_zimy() {
        let lat = 521;
        let zima = sun_state(minute(355, 12), lat).elevation_deg;
        let lato = sun_state(minute(170, 12), lat).elevation_deg;
        assert!(
            lato > zima + 30.0,
            "przesilenia się nie różnią: zima {zima}°, lato {lato}°"
        );
        // Na 52° w południe letnie słońce nie sięga zenitu, a zimowe nie znika.
        assert!(lato < 65.0 && zima > 5.0, "zima {zima}°, lato {lato}°");
    }

    #[test]
    fn nizsza_szerokosc_daje_wyzsze_slonce() {
        let polnoc = sun_state(minute(170, 12), 600).elevation_deg;
        let poludnie_geo = sun_state(minute(170, 12), 315).elevation_deg;
        assert!(
            poludnie_geo > polnoc,
            "szerokość nie wpływa na wysokość słońca: {poludnie_geo}° wobec {polnoc}°"
        );
    }

    #[test]
    fn kierunek_jest_znormalizowany_i_zgodny_z_wysokoscia() {
        for h in 0..24u64 {
            let s = sun_state(minute(100, h), 521);
            assert!(
                (s.direction.length() - 1.0).abs() < 1e-4,
                "wektor nieznormalizowany"
            );
            assert!(
                (s.direction.z - s.elevation_deg.to_radians().sin()).abs() < 1e-4,
                "składowa pionowa nie zgadza się z wysokością"
            );
        }
    }

    /// Największa druga różnica barwy słońca przy zadanym kroku wysokości.
    fn max_druga_roznica(krok: f32) -> f32 {
        let n = (108.0 / krok) as i32;
        let mut poprzednia = sky_sample(-18.0).sun_color;
        let mut biezaca = sky_sample(-18.0 + krok).sun_color;
        let mut max = 0.0f32;
        for i in 2..n {
            let nastepna = sky_sample(-18.0 + i as f32 * krok).sun_color;
            max = max.max((nastepna - biezaca * 2.0 + poprzednia).length());
            poprzednia = biezaca;
            biezaca = nastepna;
        }
        max
    }

    #[test]
    fn model_nieba_nie_ma_zalaman() {
        // §7 M1: „doba w 30 s realnych — ciągła zmiana oświetlenia bez skoków".
        //
        // Testem na załamanie pochodnej nie jest wartość drugiej różnicy — ta jest niezerowa
        // dla każdej krzywej — tylko **jak się zachowuje przy zagęszczaniu próbkowania**.
        // Dla funkcji gładkiej druga różnica maleje jak kwadrat kroku (czterokrotnie przy
        // połowieniu), dla funkcji z załamaniem — tylko liniowo. To rozróżnienie jest ostre
        // i nie wymaga zgadywania progu.
        let a = max_druga_roznica(0.1);
        let b = max_druga_roznica(0.05);
        let stosunek = a / b.max(1e-9);
        assert!(
            stosunek > 3.0,
            "druga różnica maleje {stosunek:.2}× zamiast ~4× — model ma załamanie pochodnej"
        );
    }

    #[test]
    fn tabela_wiernie_przybliza_model() {
        // Tutaj: **tabela** ma wiernie oddawać model. 64 wpisy na 108° dają komórkę 1,7°,
        // więc interpolacja liniowa ma w środku komórki błąd rzędu krzywizny — i to jest
        // świadomy wybór z §5.8 (tabela zamiast modelu Preethama). Test pilnuje, żeby ten
        // błąd został tam, gdzie go zaakceptowaliśmy, a nie urósł po zmianie modelu.
        let lut = sky_lut();
        let mut najgorszy = 0.0f32;
        for i in 0..2160 {
            let e = -18.0 + i as f32 * 0.05;
            let blad = (sample_sky(&lut, e).sun_color - sky_sample(e).sun_color).length();
            najgorszy = najgorszy.max(blad);
        }
        // Zmierzone 0,065 przy wartościach barwy sięgających 1,4 — czyli ~4,6 % błędu
        // względnego, i to tylko w najbardziej stromym momencie wschodu. Próg 0,08 zostawia
        // margines na strojenie modelu, a złapie zmianę rzędu wielkości. Gdyby kiedyś było
        // to widać: zagęścić tabelę przy horyzoncie, gdzie dzieje się wszystko, zamiast
        // podnosić rozdzielczość równomiernie.
        assert!(
            najgorszy < 0.08,
            "tabela rozjeżdża się z modelem o {najgorszy}"
        );
    }

    #[test]
    fn tabela_nieba_jest_ciagla_na_granicach_komorek() {
        // Interpolacja liniowa jest ciągła z definicji, ale tylko jeśli próbkowanie
        // faktycznie trafia w tę samą komórkę z obu stron granicy — błąd o jeden indeks
        // objawiłby się migotaniem nieba przy powolnym zachodzie i niczym więcej.
        let lut = sky_lut();
        for i in 1..SKY_LUT_SIZE {
            let elev =
                SKY_ELEV_MIN + (SKY_ELEV_MAX - SKY_ELEV_MIN) * i as f32 / (SKY_LUT_SIZE - 1) as f32;
            let przed = sample_sky(&lut, elev - 1e-3);
            let po = sample_sky(&lut, elev + 1e-3);
            assert!(
                (przed.zenith - po.zenith).length() < 1e-3,
                "szew tabeli przy {elev}°"
            );
        }
    }

    #[test]
    fn doba_zmienia_oswietlenie_bez_skokow() {
        // Kryterium WP-R4: „doba w 30 s realnych — ciągła zmiana oświetlenia bez skoków".
        // Przy 1000× przyspieszeniu klatka to ~17 minut gry, więc sprawdzamy krok
        // minutowy z zapasem: żaden składnik oświetlenia nie ma prawa przeskoczyć.
        let lat = 521;
        let lut = sky_lut();
        let mut poprzednie: Option<(SkySample, f32, f32)> = None;
        for m in 0..1440 {
            let sun = sun_state(SimMinute(minute(170, 0).0 + m), lat);
            let sky = sample_sky(&lut, sun.elevation_deg);
            let e = exposure(sun.elevation_deg);
            if let Some((p_sky, p_exp, p_elev)) = poprzednie {
                assert!(
                    (sun.elevation_deg - p_elev).abs() < 0.5,
                    "skok wysokości słońca w minucie {m}"
                );
                // Ekspozycja przechodzi od 4× do 1× na dziesięciu stopniach wysokości,
                // a słońce wznosi się o ~0,25°/min — zmiana rzędu 0,08 na minutę jest
                // płynna, skok byłby wielokrotnie większy.
                assert!((e - p_exp).abs() < 0.15, "skok ekspozycji w minucie {m}");
                for (a, b) in [
                    (sky.zenith, p_sky.zenith),
                    (sky.horizon, p_sky.horizon),
                    (sky.ground, p_sky.ground),
                    (sky.sun_color, p_sky.sun_color),
                ] {
                    assert!(
                        (a - b).length() < 0.06,
                        "skok barwy nieba w minucie {m}: {a:?} po {b:?}"
                    );
                }
            }
            poprzednie = Some((sky, e, sun.elevation_deg));
        }
    }

    #[test]
    fn noc_jest_ciemna_a_dzien_jasny() {
        let lut = sky_lut();
        let noc = sample_sky(&lut, -15.0);
        let dzien = sample_sky(&lut, 50.0);
        assert!(noc.sun_color.length() < 0.01, "słońce świeci w nocy");
        assert!(dzien.sun_color.length() > 1.0, "dzień bez słońca");
        assert!(dzien.zenith.length() > noc.zenith.length() * 5.0);
        assert!(
            exposure(-15.0) > exposure(50.0),
            "ekspozycja nie reaguje na porę doby"
        );
    }

    #[test]
    fn zachod_jest_cieplejszy_od_poludnia() {
        let lut = sky_lut();
        let zachod = sample_sky(&lut, 2.0);
        let poludnie = sample_sky(&lut, 55.0);
        let cieplota = |c: Vec3| c.x / c.z.max(1e-4);
        assert!(
            cieplota(zachod.sun_color) > cieplota(poludnie.sun_color) * 1.5,
            "zachód nie jest cieplejszy od południa"
        );
    }
}
