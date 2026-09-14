//! Katalog `data/` — re-eksport z `magnat_core::assets` (00 §5).
//!
//! Funkcje przeniesiono do `core` w M3a: potrzebuje ich też `sim/agents`, a ten
//! nie zależy od `sim/world` (i nie będzie — to `sim/world` rozszerza się o populację).
//! Nazwy z kontraktu M1 §6 zostają na miejscu.

pub use magnat_core::assets::{data_dir, data_path, DATA_DIR_ENV};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn katalog_danych_zawiera_to_czego_uzywa_generator() {
        // Test jest zarazem sprawdzianem, że pliki wymagane przez M1 w ogóle istnieją —
        // brak któregoś objawiłby się dopiero paniką w środku generacji.
        for plik in [
            "materials/terrain.ron",
            "geology/layers.ron",
            "geology/erosion.ron",
            "geology/deposits.ron",
        ] {
            assert!(data_path(plik).is_file(), "brak data/{plik}");
        }
    }
}
