//! Rdzeń liczbowy produkcji — funkcje czyste bez ECS i bez świata (M7e WP12b, §5.15).
//!
//! # Dlaczego przerób stoi tutaj, a nie w `sim/economy::kernel`
//!
//! Plan M7e wymienia `throughput` wśród funkcji jądra i podaje jego adres:
//! `sim/economy::kernel`. Tego adresu **nie da się użyć**: przerób liczy `sim/supply`,
//! a `sim/supply` jest crate'em liściastym — zależy wyłącznie od `core` i `ecs`.
//! Zależność `supply → economy` zamknęłaby cykl (`economy → supply` istnieje od M6d)
//! i Cargo by tego nie zbudował.
//!
//! Rozstrzygnięcie jest takie, że **jądro jest regułą, a nie jednym plikiem**: decyzja
//! ekonomiczna ma być funkcją czystą o jawnych wejściach, mieszkać w crate'cie, który
//! ma dane, i mieć **jeden adres** dla `sim/macro`. Adres nadaje `economy::kernel`
//! przez reeksport — a implementacja jest jedna, więc rozjechać się nie ma z czym.
//! To jest ta sama treść, którą §5.15 miał na myśli („ta sama funkcja liczy tak samo
//! w mezo i w makro"), tylko wykonana zgodnie z grafem zależności.
//!
//! # Zero zmian zachowania
//!
//! Kryterium WP12b brzmi „wszystkie testy M7 przechodzą bez modyfikacji". Dlatego
//! [`throughput`] odtwarza **kolejność dzieleń** sprzed wydzielenia co do jednego
//! kroku, razem z pośrednim przycięciem do `i64` po dławieniu wsadu. Dwa dzielenia
//! dają inny wynik niż jedno przez iloczyn i to `to` jest wynik kontraktowy —
//! ta sama uwaga, którą M7a zapisał przy `effective_labor`.

use magnat_core::Mass;

/// Pokrycie etatowe odpowiadające pełnej obsadzie, w promilach (`K-44`).
pub const FULL_LABOR: u16 = 1_000;

/// Masa wsadu jednej szarży: nominał linii, czas trwania, dławienie wsadem
/// i pokrycie etatowe.
///
/// **Dwa niezależne ograniczniki i oba muszą się zmieścić**: wsad mówi, ile jest
/// z czego robić, obsada — ile jest komu. Mnożenie, a nie minimum: zakład z połową
/// ludzi i połową mąki robi ćwierć szarży, bo brakuje mu obu rzeczy.
///
/// - `nominal_per_hour` — nominalna przepustowość linii na godzinę,
/// - `duration_minutes` — czas trwania szarży z receptury,
/// - `throttle_pct` — dławienie z kaskady niedoboru (§5.7), 0..=100,
/// - `labor_pct` — pokrycie etatowe w promilach, [`FULL_LABOR`] = obsada kompletna.
#[must_use]
pub fn throughput(
    nominal_per_hour: Mass,
    duration_minutes: u32,
    throttle_pct: u8,
    labor_pct: u16,
) -> Mass {
    let pelna = i128::from(nominal_per_hour.0) * i128::from(duration_minutes) / 60;
    // Przycięcie do `i64` **tutaj**, przed pomnożeniem przez obsadę: tak liczył
    // `ProductionLine::charge_mass` przed wydzieleniem i tak ma liczyć dalej.
    //
    // ponytail: rzutowanie **zawija**, nie nasyca, i to jest zachowanie odziedziczone
    // razem ze wzorem. Sufit jest nazwany: nominał linii × czas szarży musi zmieścić
    // się w `i64` gramów, czyli ~9,2 mln ton na jedną szarżę. Najcięższa linia
    // w katalogu M6 robi rząd dziesiątek ton, więc zapas jest sześciorzędowy.
    // Droga wyjścia, gdyby kiedyś zabrakło: `i64::try_from(...).unwrap_or(i64::MAX)`
    // także tutaj — ale to **zmienia wynik** przy przepełnieniu, więc nie wolno
    // tego zrobić przy okazji, tylko osobną zmianą z własnym testem.
    let po_dlawieniu = (pelna * i128::from(throttle_pct) / 100) as i64;
    let z_obsada = i128::from(po_dlawieniu) * i128::from(labor_pct) / i128::from(FULL_LABOR);
    Mass(i64::try_from(z_obsada).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wersja sprzed wydzielenia, przepisana wprost z `charge_mass` i `sprobuj_start`.
    fn przed_wydzieleniem(nominal: Mass, minuty: u32, pct: u8, labor: u16) -> Mass {
        let pelna = i128::from(nominal.0) * i128::from(minuty) / 60;
        let szarza = Mass((pelna * i128::from(pct) / 100) as i64);
        Mass(
            i64::try_from(i128::from(szarza.0) * i128::from(labor) / i128::from(FULL_LABOR))
                .unwrap_or(i64::MAX),
        )
    }

    #[test]
    fn wydzielenie_nie_zmienilo_ani_jednego_grama() {
        // Kryterium WP12b jest „zero zmian zachowania", więc sprawdza się je
        // porównaniem z wersją sprzed wydzielenia, a nie oczekiwaną liczbą.
        for nominal in [0i64, 1, 997, 120_000, 5_000_000] {
            for minuty in [1u32, 7, 60, 90, 1_440] {
                for pct in [0u8, 1, 33, 99, 100] {
                    for labor in [0u16, 1, 333, 999, 1_000] {
                        let a = throughput(Mass(nominal), minuty, pct, labor);
                        let b = przed_wydzieleniem(Mass(nominal), minuty, pct, labor);
                        assert_eq!(a, b, "{nominal} {minuty} {pct} {labor}");
                    }
                }
            }
        }
    }

    #[test]
    fn pelna_obsada_jest_punktem_neutralnym() {
        let pelny = throughput(Mass(60_000), 60, 100, FULL_LABOR);
        assert_eq!(pelny, Mass(60_000));
        // Połowa ludzi i połowa wsadu to ćwierć szarży — mnożenie, nie minimum.
        assert_eq!(throughput(Mass(60_000), 60, 50, 500), Mass(15_000));
    }

    #[test]
    fn brak_obsady_zatrzymuje_linie() {
        assert_eq!(throughput(Mass(60_000), 60, 100, 0), Mass(0));
    }

    #[test]
    fn sufit_jest_ten_sam_co_przed_wydzieleniem() {
        // Nominał, przy którym wzór się przepełnia, **zawija tak samo jak zawijał** —
        // i to jest treść kryterium „zero zmian zachowania". Test pilnuje, żeby nikt
        // nie naprawił tego przy okazji: naprawa zmienia wynik i należy do osobnej
        // zmiany z własnym uzasadnieniem.
        assert_eq!(
            throughput(Mass(i64::MAX), 1_440, 100, FULL_LABOR),
            przed_wydzieleniem(Mass(i64::MAX), 1_440, 100, FULL_LABOR)
        );
    }
}
