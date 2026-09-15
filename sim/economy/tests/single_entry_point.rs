//! Test statyczny WP1: **jedno wejście do zmiany salda**.
//!
//! Prywatność pola `Account::balance` załatwia zakaz z zewnątrz modułu `books`,
//! ale nie z jego wnętrza — a to właśnie wnętrze będzie rosło przez całą fazę M5
//! (banki w M5d, rozliczanie transakcji w M5b). Dlatego bramka jest tekstowa:
//! przypisanie do salda wolno mieć dokładnie jedno i ma stać w `Books::set_balance`.
//!
//! Test czyta źródła crate'u, więc jest bramką CI, a nie ozdobą — nowy moduł piszący
//! po saldzie „na skróty" zaczerwieni go w tym samym przebiegu, w którym powstanie.

use std::fs;
use std::path::{Path, PathBuf};

fn src_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in fs::read_dir(dir).expect("brak katalogu src") {
            let p = e.expect("nieczytelny wpis katalogu").path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut out);
    out.sort();
    out
}

/// Przypisania do pola `balance` — bez porównań (`==`, `!=`) i bez inicjalizacji
/// struktury (`balance: ...`).
fn assignments(text: &str) -> Vec<(usize, String)> {
    text.lines()
        .enumerate()
        .filter(|(_, l)| {
            let l = l.trim_start();
            if l.starts_with("//") || l.starts_with("///") {
                return false;
            }
            match l.find(".balance") {
                None => false,
                Some(i) => {
                    let rest = l[i + ".balance".len()..].trim_start();
                    rest.starts_with('=') && !rest.starts_with("==")
                }
            }
        })
        .map(|(i, l)| (i + 1, l.trim().to_string()))
        .collect()
}

#[test]
fn saldo_zmienia_sie_tylko_w_jednym_miejscu() {
    let mut found = Vec::new();
    for path in src_files() {
        let text = fs::read_to_string(&path).expect("nieczytelny plik źródłowy");
        for (line, code) in assignments(&text) {
            found.push(format!("{}:{line}: {code}", path.display()));
        }
    }
    assert_eq!(
        found.len(),
        1,
        "przypisań do salda ma być dokładnie jedno (Books::set_balance), a jest {}:\n{}",
        found.len(),
        found.join("\n")
    );
    assert!(
        found[0].contains("books.rs") && found[0].contains("self.accounts["),
        "jedyne przypisanie do salda nie jest tym z Books::set_balance: {}",
        found[0]
    );
}

#[test]
fn wykrywacz_lapie_podstawione_przypisanie() {
    // Bramka, która nigdy nie świeci na czerwono, nie jest bramką (M5 §7.4).
    let ile = assignments("    acc.balance = Money(0);\n    if a.balance == b.balance {}\n");
    assert_eq!(ile.len(), 1);
    assert_eq!(assignments("    balance: Money::ZERO,").len(), 0);
}
