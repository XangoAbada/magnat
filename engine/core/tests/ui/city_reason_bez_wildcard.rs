//! `K-12` po podziale (`K-58`): reguła obowiązuje na **każdym z trzech enumów
//! osobno**, a nie tylko na sumie.
//!
//! To jest cały warunek, pod którym `R2-WP20` wolno było zrobić. Gdyby wyczerpujący
//! `match` wymagany był wyłącznie na sumie, wystarczyłoby jedno ramię
//! `DecisionReason::Citizen(_) => …` i gwarancja wyjaśnialności zniknęłaby po cichu:
//! kompilacja przechodziłaby, a gracz dostawałby pustą kartę. Test trzyma ten
//! warunek dla miasta; bliźniacze pliki trzymają go dla pozostałych dwóch aktorów.
use magnat_core::CityReason;

fn opisz(powod: CityReason) -> &'static str {
    match powod {
        // Celowo brak obsługi jakiegokolwiek wariantu i brak ramienia `_`.
    }
}

fn main() {
    let _ = opisz(CityReason::TaxSettled {
        kind: magnat_core::TaxKind::Vat,
        amount: magnat_core::Money::ZERO,
    });
}
