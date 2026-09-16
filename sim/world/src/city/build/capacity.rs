//! Pojemność mieszkaniowa: stałe kalibracyjne, `District.pop_capacity`
//! i przeskalowanie podziału na mieszkania (M2 §5.6, test T10).
//!
//! Wydzielone z `city/build.rs` w R-WP8 bez zmiany zachowania.

use super::*;

/// Przelicza `District.pop_capacity` z **faktycznej liczby mieszkań** (korekta C9 z M2c).
///
/// M2c liczył tę wartość z gęstości strefy w osobach na hektar, bo lokali jeszcze nie było,
/// i zostawił ją do przeliczenia tutaj. Wielkość gospodarstwa domowego jest na razie stałą
/// z budżetu M2 §7 („167 000 mieszkań × 2,4 os."); model demograficzny należy do M3
/// i on tę liczbę zastąpi rozkładem, a nie średnią.
pub fn recompute_pop_capacity(districts: &mut DistrictSet, parcels: &ParcelSet, set: &BuildingSet) {
    let mut mieszkan = vec![0u32; districts.districts.len()];
    for u in set.units.iter().filter(|u| u.kind.is_dwelling()) {
        let b = &set.buildings[u.building.0.index() as usize];
        let d = parcels.parcels[b.parcel.0.index() as usize].district.0 as usize;
        if d < mieszkan.len() {
            mieszkan[d] += 1;
        }
    }
    for (d, n) in districts.districts.iter_mut().zip(&mieszkan) {
        d.pop_capacity = (*n as f32 * OSOB_NA_MIESZKANIE) as u32;
    }
}

/// Osób na mieszkanie w epoce startowej. Wejście dla M3, nie prawda o demografii.
pub const OSOB_NA_MIESZKANIE: f32 = 2.4;

/// Etatów na mieszkańca — z budżetu §7 fazy: 212,5 tys. stanowisk na 400 tys. ludzi.
/// To jest liczba, wobec której mierzy się drugą połowę testu T10.
pub const ETATOW_NA_MIESZKANCA: f32 = 0.531_25;

/// Dolna i górna granica zagęszczenia mieszkań. Poza nimi kalibracja przestaje być
/// kalibracją, a zaczyna produkować kawalerki 30-metrowe albo apartamenty w blokowisku.
const GESTOSC_MIN: f32 = 0.55;
const GESTOSC_MAX: f32 = 3.20;

/// Widełki przelicznika „m² lokalu na stanowisko" przy kalibracji drugiej połowy T10.
/// Szersze niż mieszkaniowe i to jest uzasadnione: 0,30 × przelicznik bazowy daje biuro
/// o 8 m² na urzędnika, czyli dokładnie to, czym było biuro w 1990 roku, a 3,0 — halę,
/// w której pracuje trzech ludzi przy dwóch maszynach. Obie skrajności istnieją.
///
/// Głębsza przyczyna rozrzutu zostaje nienaprawiona i jest tego świadoma: stanowiska
/// liczą się z **powierzchni lokalu**, więc gospodarstwo rolne zatrudnia tylu ludzi,
/// ilu mieści się w chałupie, a nie ilu potrzeba na dwudziestu hektarach. Praca na roli
/// i w wyrobisku dzieje się poza budynkiem i jej model należy do rynku pracy (M7).
pub const ETATY_SCALE_MIN: f32 = 0.18;
pub const ETATY_SCALE_MAX: f32 = 3.00;

/// **Kalibracja pojemności mieszkaniowej do `target_pop`** (test T10, korekta I-6).
///
/// Dlaczego to w ogóle istnieje. §9.1/7 zobowiązuje M2 do zagwarantowania pojemności
/// w widełkach T10, a M3 ma populację tylko skalować, nie dostawiać budynków. Tymczasem
/// liczba mieszkań wychodząca z Etapu 6 zależy od tego, ile płaskiego, suchego terenu
/// da generator M1 — a to zmienia się z ziarnem, nie z planem. Pomiar na 12 światach
/// (3 ziarna × 4 rozmiary, `lowland`) dał rozrzut **0,61–1,08** wobec `target_pop`.
/// Żadne ustawienie `max_depth_m` ani powierzchni lokali nie zamknie tego w oknie
/// szerokim na 11 punktów procentowych, bo rozrzut nie bierze się z parametrów.
///
/// Co robi ta funkcja. Powierzchnia mieszkalna budynku zostaje bez zmian; zmienia się
/// **podział na lokale**: przy niedoborze mieszkań te same metry dzielą się na więcej
/// mniejszych, przy nadmiarze — na mniej większych. To jest zależność prawdziwa również
/// w rzeczywistości (presja mieszkaniowa przekłada się na metraż), a sylwetka miasta,
/// za którą odpowiada gramatyka, nie drgnie.
///
/// Zwraca zastosowany współczynnik; 1,0 znaczy „nie było czego kalibrować".
pub fn rescale_dwellings(set: &mut BuildingSet, target_pop: u32) -> f32 {
    let jest = set.units.iter().filter(|u| u.kind.is_dwelling()).count() as f32;
    if jest < 1.0 {
        return 1.0;
    }
    // Celujemy w środek okna T10 (0,97–1,08), nie w jego brzeg.
    let cel = target_pop as f32 * 1.025 / OSOB_NA_MIESZKANIE;
    let k = (cel / jest).clamp(GESTOSC_MIN, GESTOSC_MAX);
    if (k - 1.0).abs() < 0.02 {
        return 1.0;
    }

    let mut nowe: Vec<Unit> = Vec::with_capacity(set.units.len());
    let mut mieszkan = 0u32;
    for b in &mut set.buildings {
        let od = nowe.len() as u32;
        let stare = &set.units[b.units.start as usize..b.units.end as usize];
        let mut i = 0usize;
        while i < stare.len() {
            // Mieszkania jednej kondygnacji idą w tablicy obok siebie — dzielimy je
            // grupami, bo to na kondygnacji jest do rozdania konkretny metraż.
            if !stare[i].kind.is_dwelling() {
                nowe.push(stare[i].clone());
                i += 1;
                continue;
            }
            let pietro = stare[i].floor;
            let mut j = i;
            let mut pole = 0f32;
            while j < stare.len() && stare[j].kind.is_dwelling() && stare[j].floor == pietro {
                pole += f32::from(stare[j].area_m2);
                j += 1;
            }
            let n = (((j - i) as f32) * k).round().max(1.0) as u32;
            let na_lokal = (pole / n as f32).max(12.0);
            let wzor = &stare[i];
            for _ in 0..n {
                nowe.push(Unit {
                    building: wzor.building,
                    floor: pietro,
                    kind: UnitKind::Dwelling {
                        rooms: (na_lokal / 28.0).round().clamp(1.0, 8.0) as u8,
                    },
                    area_m2: na_lokal.clamp(1.0, f32::from(u16::MAX)) as u16,
                    occupant: UnitOccupant::Vacant,
                    // Czynsz jest proporcjonalny do metrażu, więc skaluje się razem z nim.
                    rent_hint: wzor
                        .rent_hint
                        .mul_ratio(i64::from(na_lokal as u16), i64::from(wzor.area_m2.max(1))),
                    workplaces: 0..0,
                });
                mieszkan += 1;
            }
            i = j;
        }
        b.units = od..nowe.len() as u32;
    }
    set.units = nowe;
    set.report.units = set.units.len() as u32;
    set.report.dwellings = mieszkan;
    k
}
