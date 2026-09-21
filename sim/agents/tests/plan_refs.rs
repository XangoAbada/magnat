//! `R2-WP6`: komentarz nie odsyła do fazy, której nie ma.
//!
//! Test jest celowo mechaniczny — pakiet nie zmienia zachowania, więc nie ma czego
//! odtwarzać. Odesłanie do „fazy, która to zrobi" jest obietnicą, a obietnica bez
//! dokumentu jest tym samym, czym `TODO` w kodzie (`K-18` pkt 4): wygląda jak plan
//! i nikogo nie zobowiązuje.

use std::collections::BTreeSet;

/// Wszystkie identyfikatory faz i dokumentów naprawczych, jakie istnieją w planie.
fn istniejace() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let katalog = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/implementation-plan");
    for wpis in std::fs::read_dir(&katalog).expect("docs/implementation-plan") {
        let nazwa = wpis.expect("wpis").file_name().to_string_lossy().to_string();
        let Some(klucz) = nazwa.split('-').next() else {
            continue;
        };
        if klucz.len() >= 2 && (klucz.starts_with('M') || klucz.starts_with('R')) {
            out.insert(klucz.to_string());
            // `M10a` jest dokumentem, ale `M10` też jest prawidłowym odesłaniem.
            let baza: String = klucz
                .chars()
                .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                .collect();
            out.insert(baza);
        }
    }
    out
}

#[test]
fn zaden_komentarz_nie_odsyla_do_nieistniejacej_fazy() {
    let istnieja = istniejace();
    assert!(
        istnieja.contains("M10") && istnieja.contains("R2"),
        "skaner nie znalazł planu — sam test byłby wtedy spełniony tożsamościowo"
    );

    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut bledy: Vec<String> = Vec::new();
    let mut odwolan = 0usize;
    obejdz(&src, &mut |sciezka, tekst| {
        for (i, linia) in tekst.lines().enumerate() {
            let t = linia.trim_start();
            if !t.starts_with("//") {
                continue;
            }
            for slowo in t.split(|c: char| !c.is_ascii_alphanumeric()) {
                if !pasuje_do_fazy(slowo) {
                    continue;
                }
                odwolan += 1;
                if !istnieja.contains(slowo) {
                    bledy.push(format!("{}:{}: {slowo}", sciezka.display(), i + 1));
                }
            }
        }
    });

    assert!(odwolan > 50, "skaner znalazł tylko {odwolan} odesłań");
    assert!(
        bledy.is_empty(),
        "komentarze odsyłają do dokumentów, których nie ma w docs/implementation-plan:\n{}",
        bledy.join("\n")
    );
}

/// `M7`, `M10a`, `R2b` — litera, cyfry i opcjonalna litera podfazy.
///
/// **Samo `R<n>` nie jest odesłaniem do dokumentu i skaner go nie widzi.** Dokument
/// `R2` nazywa swoje pakiety `R2-WP1`, ale `R5` w komentarzu znaczy „ryzyko R5
/// z dokumentu tej fazy" — każdy dokument planu ma własną numerację ryzyk, więc
/// `R1`…`R13` są w komentarzach kilkanaście razy i żadne z nich nie jest adresem.
/// `R2a` i dalsze są jednoznaczne, bo ryzyka nie mają litery podfazy. Plan `R2a`
/// zapowiadał listę „`M*.md` i `R*.md`" — to jest poprawka do niego (`K-18`).
fn pasuje_do_fazy(s: &str) -> bool {
    let mut z = s.chars();
    let prefiks = z.next();
    if !matches!(prefiks, Some('M') | Some('R')) {
        return false;
    }
    let reszta: Vec<char> = z.collect();
    let cyfry = reszta.iter().take_while(|c| c.is_ascii_digit()).count();
    if cyfry == 0 || cyfry > 2 {
        return false;
    }
    match reszta.len() - cyfry {
        0 => prefiks == Some('M'),
        1 => reszta[cyfry].is_ascii_lowercase(),
        _ => false,
    }
}

fn obejdz(dir: &std::path::Path, f: &mut impl FnMut(&std::path::Path, &str)) {
    for wpis in std::fs::read_dir(dir).expect("katalog") {
        let p = wpis.expect("wpis").path();
        if p.is_dir() {
            obejdz(&p, f);
        } else if p.extension().is_some_and(|e| e == "rs") {
            let tekst = std::fs::read_to_string(&p).expect("plik");
            f(&p, &tekst);
        }
    }
}
