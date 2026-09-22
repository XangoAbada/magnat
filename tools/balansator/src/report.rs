//! Raport Markdown profilu nocnego.
//!
//! **Generuje go Rust, nie Python.** §7.4 dokumentu fazy wspomina o analizie
//! w Pythonie „z wykresami"; raport bez wykresów, złożony z tabel, odpowiada na
//! te same pytania, kosztuje jedną funkcję i nie wnosi do bramki CI zależności
//! od drugiego środowiska uruchomieniowego. Jeśli kiedyś wykresy okażą się
//! potrzebne, wejściem dla Pythona jest plik metryk, a nie ten raport.

use std::collections::BTreeMap;

use crate::gates::{mediana, unikalne, GateOutcome, Verdict};
use crate::metrics::RunFile;

/// Raport: tabela bramek plus tabela metryk per scenariusz (mediana/min/maks
/// po ziarnach).
#[must_use]
pub fn markdown(runs: &[RunFile], gates: &[GateOutcome], profil: &str) -> String {
    let mut s = String::new();
    s.push_str("# Raport balansatora\n\n");
    s.push_str(&format!(
        "Profil: **{profil}**. Przebiegów: {}, ziaren: {}.\n\n",
        runs.len(),
        unikalne(runs).len()
    ));

    // **Każda bramka ma wiersz, także ta, której nie policzono** (`R2-WP24`).
    // Do R2f bramka spoza profilu znikała z raportu bez śladu, więc czytelnik
    // nie miał jak odróżnić „zmierzone i zielone" od „nie zmierzono wcale".
    let bez_werdyktu = gates
        .iter()
        .filter(|g| g.verdict == Verdict::Skipped)
        .count();
    s.push_str(&format!(
        "## Bramki\n\nWierszy: {}, w tym bez werdyktu: {bez_werdyktu}.\n\n\
         | Bramka | Werdykt | Zmierzono | Próg |\n|---|---|---|---|\n",
        gates.len()
    ));
    for g in gates {
        s.push_str(&format!(
            "| {} {} | {} | {} | {} |\n",
            g.gate,
            g.name,
            match g.verdict {
                Verdict::Green => "ZIELONE",
                Verdict::Red => "**CZERWONE**",
                Verdict::Advisory => "DORADCZE",
                Verdict::Skipped => "**POMINIĘTE**",
            },
            g.value,
            g.threshold
        ));
    }

    s.push_str("\n## Metryki per scenariusz\n\n");
    s.push_str(
        "| Scenariusz | Ziaren | CPI koniec (bp) | r/r ostatnia (bp) | Marża mediana (bp) \
         | Odłożenia (‰) | Braki (‰) | HHI | Odpisy/obrót (‰) |\n\
         |---|---|---|---|---|---|---|---|---|\n",
    );
    let mut per_scenariusz: BTreeMap<&str, Vec<&RunFile>> = BTreeMap::new();
    for r in unikalne(runs) {
        per_scenariusz
            .entry(r.scenario.as_str())
            .or_default()
            .push(r);
    }
    for (nazwa, lista) in &per_scenariusz {
        let kol = |f: &dyn Fn(&RunFile) -> i32| -> String {
            let mut v: Vec<i32> = lista.iter().map(|r| f(r)).collect();
            v.sort_unstable();
            format!(
                "{} ({}…{})",
                mediana(&v),
                v.first().copied().unwrap_or(0),
                v.last().copied().unwrap_or(0)
            )
        };
        s.push_str(&format!(
            "| {nazwa} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            lista.len(),
            kol(&|r| r.days_data.last().map_or(0, |d| d.cpi_index_bp)),
            kol(&|r| r
                .days_data
                .iter()
                .rev()
                .find_map(|d| d.cpi_yoy_bp)
                .unwrap_or(0)),
            kol(&|r| mediana(
                &r.days_data
                    .iter()
                    .map(|d| d.margin_median_bp)
                    .collect::<Vec<_>>()
            )),
            kol(&|r| mediana(
                &r.days_data
                    .iter()
                    .map(|d| d.deferral_permille)
                    .collect::<Vec<_>>()
            )),
            kol(&|r| mediana(
                &r.days_data
                    .iter()
                    .map(|d| d.stockout_permille)
                    .collect::<Vec<_>>()
            )),
            kol(&|r| mediana(
                &r.days_data
                    .iter()
                    .filter(|d| d.hhi_pairs > 0)
                    .map(|d| d.hhi_median)
                    .collect::<Vec<_>>()
            )),
            kol(&odpisy_do_obrotu),
        ));
    }
    s.push_str(
        "\nOdpisy do obrotu są w tabeli z korekty ★ po M5c: to **pierwszy** objaw złej \
         kalibracji zamówień i widać go, zanim ruszy rozkład marż.\n",
    );
    s
}

/// Stosunek odpisów towaru przeterminowanego do obrotu, w promilach.
fn odpisy_do_obrotu(r: &RunFile) -> i32 {
    let obrot: i64 = r.days_data.iter().map(|d| d.revenue_gr).sum();
    let odpisy: i64 = r.days_data.iter().map(|d| d.write_off_gr).sum();
    if obrot <= 0 {
        return 0;
    }
    i32::try_from(odpisy.saturating_mul(1_000) / obrot).unwrap_or(i32::MAX)
}
