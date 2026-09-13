//! Odnalezienie katalogu `data/` (00 §5).
//!
//! Ten sam kod uruchamia się z trzech miejsc o różnym katalogu bieżącym: z korzenia
//! workspace'u (`cargo run`), z katalogu crate'u (`cargo test`) i z katalogu instalacji
//! (gotowa gra). Zamiast powtarzać `../../data` w każdym wywołaniu, katalog rozstrzyga
//! się raz, w ustalonej kolejności prób.
//!
//! Wynik jest **zapamiętywany**: ścieżka nie zmienia się w trakcie procesu, a przeszukiwanie
//! systemu plików przy każdym otwarciu pliku danych byłoby kosztem bez powodu.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Zmienna środowiskowa nadpisująca wyszukiwanie — używa jej narzędzie uruchamiane
/// spoza drzewa projektu i modding (M12).
pub const DATA_DIR_ENV: &str = "MAGNAT_DATA";

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Katalog `data/`. Panika z czytelnym komunikatem, jeśli go nie ma: świat bez danych
/// nie jest stanem do obsłużenia, tylko błędem instalacji.
pub fn data_dir() -> &'static Path {
    DATA_DIR.get_or_init(|| {
        if let Some(p) = std::env::var_os(DATA_DIR_ENV) {
            let p = PathBuf::from(p);
            assert!(
                p.is_dir(),
                "{DATA_DIR_ENV} wskazuje na {p:?}, a tam nie ma katalogu"
            );
            return p;
        }

        let mut kandydaci: Vec<PathBuf> = vec![
            PathBuf::from("data"),
            PathBuf::from("../data"),
            PathBuf::from("../../data"),
        ];
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                kandydaci.push(dir.join("data"));
                kandydaci.push(dir.join("../data"));
            }
        }
        for k in &kandydaci {
            if k.is_dir() {
                return k.clone();
            }
        }
        panic!(
            "nie znaleziono katalogu data/ (próbowano: {kandydaci:?}); \
             ustaw {DATA_DIR_ENV}"
        );
    })
}

/// Ścieżka do pliku wewnątrz `data/`.
#[must_use]
pub fn data_path(rel: &str) -> PathBuf {
    data_dir().join(rel)
}

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
