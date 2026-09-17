//! Wiek produkcyjny jest **daną**, nie stałą w kodzie (`K-60`, pozycja 7 wykazu `R2`,
//! wyprzedzona przed M8c decyzją `D-N1`).
//!
//! Test padał przed poprawką z jednego powodu: pola `ages.labour_force` nie było,
//! a obie statystyki — stopa bezrobocia w `sim/economy::labor` i `MacroCell::in_labour_force`
//! w `sim/macro` — nosiły własną kopię `16..=74` w kodzie, w dwóch różnych typach
//! całkowitych i bez związku z `demography.ron`.

use magnat_agents::DemographyTable;

#[test]
fn granice_wieku_produkcyjnego_sa_w_danych() {
    let t = DemographyTable::load_default().expect("data/demography/demography.ron");
    let a = t.ages();
    assert!(a.labour_force.min < a.labour_force.max);
    // Statystyka liczy **szerzej** niż kadry: szesnastolatek bez pracy i bez szkoły
    // jest bezrobotny, a siedemdziesięciolatek bez emerytury wciąż szuka.
    assert!(
        a.labour_force.min <= a.work_start && a.labour_force.max >= a.retirement,
        "wiek produkcyjny statystyki nie obejmuje wieku pracy z tej samej tabeli"
    );
}

#[test]
fn odwrocone_granice_sa_bledem_danych() {
    let plik = std::env::temp_dir().join("magnat_labour_force_test.ron");
    let dobry = std::fs::read_to_string(magnat_core::data_path("demography/demography.ron"))
        .expect("wzorzec");
    let zly = dobry.replace(
        "labour_force: (min: 16, max: 74)",
        "labour_force: (min: 74, max: 16)",
    );
    assert_ne!(
        dobry, zly,
        "wzorzec nie zawiera pola, o które chodzi testowi"
    );
    std::fs::write(&plik, zly).expect("zapis");
    let wynik = DemographyTable::load(&plik);
    let _ = std::fs::remove_file(&plik);
    assert!(wynik.is_err(), "walidator przepuścił odwrócone granice");
}
