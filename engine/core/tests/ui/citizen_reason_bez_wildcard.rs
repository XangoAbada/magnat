//! `K-12` po podziale (`K-58`): reguła obowiązuje na **każdym z trzech enumów
//! osobno**, a nie tylko na sumie.
//!
//! To jest cały warunek, pod którym `R2-WP20` wolno było zrobić. Gdyby wyczerpujący
//! `match` wymagany był wyłącznie na sumie, wystarczyłoby jedno ramię
//! `DecisionReason::Citizen(_) => …` i gwarancja wyjaśnialności zniknęłaby po cichu:
//! kompilacja przechodziłaby, a gracz dostawałby pustą kartę. Test trzyma ten
//! warunek dla mieszkańca; bliźniacze pliki trzymają go dla firmy i miasta.
use magnat_core::CitizenReason;

fn opisz(powod: CitizenReason) -> &'static str {
    match powod {
        // Celowo brak obsługi jakiegokolwiek wariantu i brak ramienia `_`.
    }
}

fn main() {
    let _ = opisz(CitizenReason::Arrived {
        planned: magnat_core::MinuteOfDay::new(0),
        actual: magnat_core::MinuteOfDay::new(0),
    });
}
