//! Walidator grafu produktów (M6a WP1, reguły 1–5).
//!
//! Reguły 1–4 są **błędami** — katalog, który ich nie spełnia, daje miasto niedomykające
//! Etapu 7 albo magazyn, który się zapycha. Reguła 5 to **ostrzeżenia**: opisują kruchość,
//! nie błąd, i decyzja, czy kruchość jest zamierzona, należy do człowieka.
//!
//! Zapas startowy **nie jest** źródłem. `data/scenarios/initial_stock.ron` skraca rozruch
//! świata, ale łańcuch domknięty zapasem wystartuje raz i nigdy się nie odtworzy —
//! czyli dokładnie ta klasa błędu, którą walidator ma łapać. Punkty wejścia to wyłącznie
//! `RecipeSource::{Extraction, Agriculture}` oraz `Good::external_base_price.is_some()`.

use magnat_core::{GoodId, RecipeId, StockCat};

use super::load::CatalogError;
use super::recipe::OutputKind;
use super::Catalog;

/// Ostrzeżenie reguły 5 — kruchość katalogu, nie jego błąd.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphWarning {
    pub kind: WarningKind,
    /// Klucz towaru albo receptury, której ostrzeżenie dotyczy.
    pub subject: String,
    /// Kontekst: klucz źródła dla `SingleSource`, liczba receptur dla
    /// `CriticalInputWithoutSubstitute`.
    pub context: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WarningKind {
    /// Dokładnie jedno źródło — awaria tego jednego zatrzymuje cały łańcuch w dół.
    SingleSource,
    /// Krytyczne wejście bez substytutu w kategorii żywnościowej albo energetycznej.
    CriticalInputWithoutSubstitute,
    /// Bez ceny importowej — w kryzysie nie da się go sprowadzić.
    NotImportable,
}

impl std::fmt::Display for GraphWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            WarningKind::SingleSource => {
                write!(f, "`{}`: jedno źródło ({})", self.subject, self.context)
            }
            WarningKind::CriticalInputWithoutSubstitute => {
                // Jedna receptura to „w recepturze", reszta „w recepturach". Odmiana
                // w komunikacie narzędzia nie jest lokalizacją UI (nie ma `LocKey`),
                // ale nieodmieniona liczebnikiem wygląda na błąd programu.
                let forma = if self.context == "1" {
                    "recepturze"
                } else {
                    "recepturach"
                };
                write!(
                    f,
                    "`{}`: krytyczne wejście bez substytutu w {} {forma}",
                    self.subject, self.context
                )
            }
            WarningKind::NotImportable => {
                write!(f, "`{}`: bez ceny importowej", self.subject)
            }
        }
    }
}

impl Catalog {
    /// **Reguła 1 — osiągalność.** Czy z samych punktów wejścia (wydobycie, rolnictwo,
    /// import) da się dojść do każdego towaru.
    ///
    /// Miasto sprawdza to jeszcze raz, ale na zbiorze receptur **zainstancjonowanych** —
    /// patrz `supply_closure_check` po stronie `sim/world`.
    pub fn validate_reachability(&self) -> Result<(), CatalogError> {
        let seed: Vec<GoodId> = self
            .goods
            .iter()
            .filter(|g| g.has_external_price())
            .map(|g| g.id)
            .collect();
        let dostepne: Vec<RecipeId> = self.recipes.iter().map(|r| r.id).collect();
        let osiagalne = self.reachable(&seed, &dostepne);
        let brak: Vec<String> = self
            .goods
            .iter()
            .filter(|g| !osiagalne[g.id.0 as usize])
            .map(|g| g.key.to_string())
            .collect();
        if brak.is_empty() {
            Ok(())
        } else {
            Err(CatalogError::Unreachable(brak))
        }
    }

    /// **Reguła 2 — ujście.** Każdy towar musi mieć co najmniej jednego odbiorcę:
    /// recepturę, koszyk mieszkańca, eksport albo utylizację. Towar bez odbiorcy
    /// zapcha magazyny i nikt nigdy nie dowie się dlaczego.
    pub fn validate_consumers(&self) -> Result<(), CatalogError> {
        let mut ma_odbiorce = vec![false; self.goods.len()];
        for r in &self.recipes {
            for w in &r.inputs {
                ma_odbiorce[w.good.0 as usize] = true;
            }
        }
        for (g, _) in &self.basket {
            ma_odbiorce[g.0 as usize] = true;
        }
        for g in &self.goods {
            // Eksport jest odbiorcą: towar z ceną zewnętrzną da się sprzedać na zewnątrz.
            // Utylizacja też — odpad z kosztem utylizacji ma dokąd pójść.
            if g.has_external_price() || g.disposal_cost.0 > 0 {
                ma_odbiorce[g.id.0 as usize] = true;
            }
        }
        let brak: Vec<String> = self
            .goods
            .iter()
            .filter(|g| !ma_odbiorce[g.id.0 as usize])
            .map(|g| g.key.to_string())
            .collect();
        if brak.is_empty() {
            Ok(())
        } else {
            Err(CatalogError::NoConsumer(brak))
        }
    }

    /// **Reguła 5 — ostrzeżenia.** Zwraca listę posortowaną po kluczu, żeby wynik
    /// walidatora dał się porównać między przebiegami.
    #[must_use]
    pub fn warnings(&self) -> Vec<GraphWarning> {
        let mut out = Vec::new();

        for g in &self.goods {
            let mut zrodla: Vec<&str> = self
                .recipes
                .iter()
                .filter(|r| r.outputs.iter().any(|o| o.good == g.id))
                .map(|r| &*r.key)
                .collect();
            if g.has_external_price() {
                zrodla.push("import");
            } else {
                out.push(GraphWarning {
                    kind: WarningKind::NotImportable,
                    subject: g.key.to_string(),
                    context: String::new(),
                });
            }
            if zrodla.len() == 1 {
                out.push(GraphWarning {
                    kind: WarningKind::SingleSource,
                    subject: g.key.to_string(),
                    context: zrodla[0].to_string(),
                });
            }
        }

        // Jedno ostrzeżenie na **towar**, nie na parę (towar, receptura). Woda bez
        // substytutu jest jednym faktem o świecie, a nie dwudziestoma faktami o dwudziestu
        // recepturach — a raport, w którym dwadzieścia linii mówi to samo, przestaje być
        // czytany, więc przestaje działać.
        let mut ile_receptur = vec![0u32; self.goods.len()];
        for r in &self.recipes {
            for w in &r.inputs {
                if !w.critical || !w.substitutes.is_empty() {
                    continue;
                }
                let g = self.good(w.good);
                if !g.substitutes.is_empty() || !self.jest_wrazliwa(g.category) {
                    continue;
                }
                ile_receptur[w.good.0 as usize] += 1;
            }
        }
        for (i, n) in ile_receptur.iter().enumerate() {
            if *n == 0 {
                continue;
            }
            out.push(GraphWarning {
                kind: WarningKind::CriticalInputWithoutSubstitute,
                subject: self.goods[i].key.to_string(),
                context: n.to_string(),
            });
        }

        out.sort_by(|a, b| {
            (a.kind as u8, &a.subject, &a.context).cmp(&(b.kind as u8, &b.subject, &b.context))
        });
        out
    }

    /// Kategoria żywnościowa albo energetyczna — te, w których brak substytutu boli
    /// najbardziej, bo nie da się ich odłożyć na później.
    fn jest_wrazliwa(&self, c: magnat_core::NeedCategoryId) -> bool {
        matches!(
            self.category(c).stock_cat,
            Some(StockCat::Food | StockCat::Drink | StockCat::Fuel)
        )
    }

    /// Domknięcie osiągalności na hipergrafie receptur — schemat Dowlinga–Galliera,
    /// O(V + E), bez przechodzenia na rozwiązywanie punktu stałego.
    ///
    /// `seed` to towary dane z zewnątrz (import), `available` to receptury, które wolno
    /// odpalić. Punkty wejścia rozpoznaje **jawna flaga** `RecipeSource`, nie puste
    /// `inputs` — receptura bez wejść, która nie jest wydobyciem, byłaby błędem danych,
    /// a nie kopalnią.
    ///
    /// Cykle są dozwolone i pożądane (rafineria ↔ elektrownia, huta ↔ fabryka maszyn);
    /// warunek na dane brzmi: **każdy cykl ma co najmniej jedno wejście z importu albo
    /// z wydobycia**. Cykl bez takiego wejścia po prostu nie zostanie osiągnięty i wyjdzie
    /// jako `Unreachable` — i to jest właściwa odpowiedź, nie porażka algorytmu.
    #[must_use]
    pub fn reachable(&self, seed: &[GoodId], available: &[RecipeId]) -> Vec<bool> {
        let mut osiagalne = vec![false; self.goods.len()];
        // Ile wejść receptury jeszcze brakuje; `u32::MAX` = receptura niedostępna.
        let mut brakow = vec![u32::MAX; self.recipes.len()];
        let mut kolejka: std::collections::VecDeque<RecipeId> = std::collections::VecDeque::new();

        let mut lista: Vec<RecipeId> = available.to_vec();
        lista.sort_by_key(|r| r.0);
        lista.dedup();
        for r in &lista {
            brakow[r.0 as usize] = self.recipe(*r).inputs.len() as u32;
        }
        // KROK 1: punktami wejścia są wyłącznie wydobycie, rolnictwo i import.
        for g in seed {
            osiagalne[g.0 as usize] = true;
        }
        for r in &lista {
            if self.recipe(*r).source.is_entry() {
                brakow[r.0 as usize] = 0;
            }
        }
        // Receptury gotowe od razu — rosnąco po `RecipeId`, żeby kolejność była danymi,
        // a nie przypadkiem (00 §3.2).
        for r in &lista {
            if brakow[r.0 as usize] == 0 {
                kolejka.push_back(*r);
            }
        }
        // Wejścia już osiągalne z importu odliczamy przed startem.
        for r in &lista {
            if brakow[r.0 as usize] == 0 || brakow[r.0 as usize] == u32::MAX {
                continue;
            }
            let brak = self
                .recipe(*r)
                .inputs
                .iter()
                .filter(|w| !osiagalne[w.good.0 as usize])
                .count() as u32;
            brakow[r.0 as usize] = brak;
            if brak == 0 {
                kolejka.push_back(*r);
            }
        }

        // KROK 2.
        while let Some(r) = kolejka.pop_front() {
            let rec = self.recipe(r);
            // `SelfConsumed` nie tworzy partii, ale **jest** towarem osiągalnym: zakład
            // go wytwarza i zużywa u siebie, więc graf ma prawo się na nim domknąć.
            let mut wyjscia: Vec<GoodId> = rec.outputs.iter().map(|o| o.good).collect();
            wyjscia.sort_by_key(|g| g.0);
            for g in wyjscia {
                if osiagalne[g.0 as usize] {
                    continue;
                }
                osiagalne[g.0 as usize] = true;
                for r2 in &lista {
                    let licznik = brakow[r2.0 as usize];
                    if licznik == 0 || licznik == u32::MAX {
                        continue;
                    }
                    if self.recipe(*r2).inputs.iter().any(|w| w.good == g) {
                        brakow[r2.0 as usize] = licznik - 1;
                        if brakow[r2.0 as usize] == 0 {
                            kolejka.push_back(*r2);
                        }
                    }
                }
            }
        }
        osiagalne
    }

    /// Wyjścia receptury, które dostają koszt — `Waste` i `SelfConsumed` go nie dostają.
    #[must_use]
    pub fn costed_outputs(&self, r: RecipeId) -> Vec<GoodId> {
        self.recipe(r)
            .outputs
            .iter()
            .filter(|o| matches!(o.kind, OutputKind::Main | OutputKind::ByProduct))
            .map(|o| o.good)
            .collect()
    }
}
