//! K-12: `match` po `DecisionReason` bez ramienia `_` MUSI przestać się kompilować,
//! gdy pojawi się nieobsłużony wariant. To jest cały mechanizm wyjaśnialności —
//! kompilacja `game/` ma paść, dopóki ktoś nie napisze, jak pokazać powód graczowi.
use magnat_core::DecisionReason;

fn opisz(powod: DecisionReason) -> &'static str {
    match powod {
        // Celowo brak obsługi jakiegokolwiek wariantu i brak ramienia `_`.
    }
}

fn main() {
    let _ = opisz(DecisionReason::Unspecified);
}
