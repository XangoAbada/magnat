//! Czas dojazdu między dzielnicami — wypełnienie [`CommuteMatrix`] (decyzja `D10`).
//!
//! ## Dlaczego nie ze snapshotu `TravelTimeMatrix`, jak zapowiadał kontrakt
//!
//! Kontrakt M10 §6 mówi „M10 trzyma, M4 wypełnia" i wskazuje `TravelTimeMatrix`
//! jako źródło. Jest jedno: **ta macierz w świecie świeżo postawionym jest pusta**.
//! Wypełniają ją obserwacje realnych przejazdów mezo (`TravelTimeMatrix::observe`),
//! a historia „na sucho" żadnego przejazdu mezo nie wykonuje — o to w niej chodzi.
//! Zdjęcie pustej macierzy dałoby `None` w każdej komórce, czyli dokładnie tę samą
//! płaską stałą, którą miało zastąpić.
//!
//! Dlatego macierz wypełnia się **z geometrii miasta**: odległość między środkami
//! ciężkości dzielnic przez nominalną prędkość miejską. Geometria istnieje od
//! Etapu 5 generatora, czyli przed pierwszą dobą historii, i nie zmienia się
//! w grze. Obserwacje M4 **uściślają** ten obraz, kiedy już są — i to jest jedyna
//! korekta kontraktu: nie „M4 wypełnia", tylko „M10 wypełnia z geometrii, M4
//! uściśla obserwacjami".
//!
//! ## Środek ciężkości z domów, nie z granic
//!
//! Dzielnica jest wielokątem, ale dojeżdża się **do ludzi i do zakładów**, a nie
//! do środka wielokąta. Środek liczy się więc ze współrzędnych domów mieszkańców:
//! dzielnica, w której wszyscy mieszkają przy jednej ulicy, ma środek na tej ulicy,
//! a nie w środku pola po drugiej stronie. Dzielnica bez mieszkańców (przemysłowa)
//! dostaje środek z zakładów — a jeśli nie ma ani jednego, zostaje przy stałej
//! płaskiej, bo nie ma czego mierzyć.

use std::collections::BTreeMap;

use magnat_agents::{PlaceCatalog, Population, Residence};
use magnat_core::{PlaceRef, WorldCoord};
use magnat_ecs::World;
use magnat_firms::Firms;

use crate::types::CommuteMatrix;

/// Nominalna prędkość dojazdu w mieście, w metrach na minutę.
///
/// 400 m/min to 24 km/h — średnia prędkość podróży miejskiej razem z postojami
/// i przesiadkami, czyli mniej niż prędkość pojazdu. To jest **stała odniesienia**,
/// a nie kalibracja: zmienia ją obserwacja M4, nie przebieg balansatora.
pub const CITY_SPEED_M_PER_MIN: i64 = 400;

/// Ile minut kosztuje dojazd wewnątrz jednej dzielnicy. Zero znaczyłoby, że praca
/// za rogiem jest za darmo, a wtedy granica dzielnicy przestałaby cokolwiek ważyć
/// w drugą stronę.
pub const INTRA_DISTRICT_MIN: u16 = 8;

/// Środki ciężkości dzielnic w centymetrach świata; `None` dla dzielnic,
/// o których nic nie wiadomo.
#[must_use]
pub fn centroids(world: &World, districts: u16) -> Vec<Option<(i64, i64)>> {
    let mut sumy: BTreeMap<u16, (i64, i64, i64)> = BTreeMap::new();
    let places = world
        .get_resource::<PlaceCatalog>()
        .and_then(PlaceCatalog::get);

    if let (Some(pop), Some(places)) = (world.get_resource::<Population>(), places) {
        for e in pop.citizens() {
            let Some(r) = world.get::<Residence>(*e) else {
                continue;
            };
            let Some(at) = magnat_agents::home_of(r).and_then(|p| places.coord_of(p)) else {
                continue;
            };
            dodaj(&mut sumy, r.district, at);
        }
    }
    // Dzielnica bez mieszkańców — przemysłowa, portowa — ma środek w swoich
    // zakładach. Bez tego kroku byłaby nieosiągalna dla rekrutacji, a to jest
    // dokładnie ta dzielnica, w której stoją fabryki bez obsady (`D10`).
    if let (Some(firms), Some(places)) = (world.get_resource::<Firms>(), places) {
        for (id, s) in firms.sites() {
            if sumy.contains_key(&s.district.0) {
                continue;
            }
            if let Some(at) = places.coord_of(PlaceRef::Site(id)) {
                dodaj(&mut sumy, s.district.0, at);
            }
        }
    }

    (0..districts)
        .map(|d| {
            sumy.get(&d)
                .filter(|(_, _, n)| *n > 0)
                .map(|(x, y, n)| (x / n, y / n))
        })
        .collect()
}

fn dodaj(sumy: &mut BTreeMap<u16, (i64, i64, i64)>, district: u16, at: WorldCoord) {
    let e = sumy.entry(district).or_insert((0, 0, 0));
    e.0 += i64::from(at.x);
    e.1 += i64::from(at.y);
    e.2 += 1;
}

/// Macierz dojazdów z geometrii miasta.
///
/// `fallback` obowiązuje wszędzie tam, gdzie nie da się policzyć odległości —
/// i jest tą samą liczbą, którą model miał do M10e wszędzie.
#[must_use]
pub fn from_geometry(world: &World, districts: u16, fallback: u16) -> CommuteMatrix {
    let srodki = centroids(world, districts);
    let mut m = CommuteMatrix::flat(districts, fallback);
    for a in 0..districts {
        for b in 0..districts {
            let minuty = if a == b {
                INTRA_DISTRICT_MIN
            } else {
                match (srodki[usize::from(a)], srodki[usize::from(b)]) {
                    (Some(p), Some(q)) => minuty_miedzy(p, q),
                    _ => fallback,
                }
            };
            m.set(a, b, minuty);
        }
    }
    m
}

/// Odległość euklidesowa w centymetrach przez prędkość miejską.
///
/// `isqrt` na `i64`, a nie `f64::sqrt`: pierwiastek z `00` §K-6 jest wprawdzie
/// deterministyczny, ale wynik i tak wpada do `u16` minut, więc liczba
/// zmiennoprzecinkowa po drodze nie kupuje ani jednej cyfry znaczącej.
fn minuty_miedzy(a: (i64, i64), b: (i64, i64)) -> u16 {
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    let cm = dx.saturating_mul(dx).saturating_add(dy.saturating_mul(dy));
    let metry = cm.isqrt() / 100;
    let minuty = (metry + CITY_SPEED_M_PER_MIN - 1) / CITY_SPEED_M_PER_MIN;
    u16::try_from(minuty.clamp(i64::from(INTRA_DISTRICT_MIN), i64::from(u16::MAX - 1)))
        .unwrap_or(u16::MAX - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dojazd_rosnie_z_odlegloscia_i_nigdy_nie_jest_darmowy() {
        // 4 km przy 400 m/min to dziesięć minut.
        assert_eq!(minuty_miedzy((0, 0), (400_000, 0)), 10);
        assert_eq!(minuty_miedzy((0, 0), (800_000, 0)), 20);
        // Za rogiem nadal kosztuje — inaczej granica dzielnicy nic nie waży.
        assert_eq!(minuty_miedzy((0, 0), (0, 0)), INTRA_DISTRICT_MIN);
        // Symetria: dojazd tam i z powrotem to ta sama liczba.
        assert_eq!(
            minuty_miedzy((100, 200), (500_000, 300_000)),
            minuty_miedzy((500_000, 300_000), (100, 200))
        );
    }
}
