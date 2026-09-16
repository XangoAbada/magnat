//! Alokacja kosztu produkcji łącznej (M6a §5.4, WP3).
//!
//! Rafineria z jednej tony ropy robi benzynę, asfalt i odpad. **Rozdzielenie kosztu wg
//! masy dałoby asfalt droższy od benzyny** — absurd, który wywraca ceny w całym łańcuchu
//! w dół, bo asfalt idzie do budowlanki, a ta wycenia z niego drogi i dachy.
//!
//! Stąd dwa tryby: [`CostAllocation::ByMass`] tam, gdzie wyjście jest jedno, i
//! [`CostAllocation::ByMarketValue`] przy produkcji łącznej. Wartością odniesienia jest
//! **statyczna** `external_base_price` z katalogu (decyzja otwarta `D3` fazy, wariant
//! prosty): krocząca średnia z rynku lokalnego byłaby realistyczniejsza, ale zamyka
//! sprzężenie koszt → cena → koszt, którego M6 nie umie jeszcze stłumić.
//!
//! `Waste` i `SelfConsumed` **nie dostają kosztu**: odpad go generuje (utylizacja),
//! a gaz opałowy spalany na miejscu nie tworzy partii, więc nie ma czego wyceniać.

use magnat_core::{split_proportional, GoodId, Mass, Money};

use crate::catalog::{Catalog, CostAllocation, OutputKind, Recipe};

/// Rozdziela koszt szarży między wyjścia, które koszt dostają.
///
/// Suma zwróconych kwot równa się `total` **zawsze** — podział idzie przez
/// [`split_proportional`], więc reszta trafia do pierwszej pozycji wg ustalonego
/// porządku (dok. 00 §2) i ani grosz nie ginie.
#[must_use]
pub fn allocate_cost(cat: &Catalog, r: &Recipe, total: Money) -> Vec<(GoodId, Money)> {
    let wyjscia: Vec<&crate::catalog::RecipeOutput> = r
        .outputs
        .iter()
        .filter(|o| matches!(o.kind, OutputKind::Main | OutputKind::ByProduct))
        .collect();
    if wyjscia.is_empty() {
        return Vec::new();
    }
    let wagi: Vec<u64> = wyjscia
        .iter()
        .map(|o| match r.cost_allocation {
            CostAllocation::ByMass => o.mass.0.unsigned_abs(),
            CostAllocation::ByMarketValue => {
                // Waga wartościowa: masa × cena odniesienia. Skala ceny (grosze za 1000
                // jednostek natywnych) skraca się w proporcji, więc nie trzeba jej
                // normalizować — i lepiej jej nie normalizować, bo dzielenie po drodze
                // to jedyne miejsce, w którym podział mógłby zgubić grosz.
                let cena = cat
                    .good(o.good)
                    .external_base_price
                    .map_or(0, |p| p.0.unsigned_abs());
                o.mass.0.unsigned_abs().saturating_mul(cena)
            }
        })
        .collect();
    split_proportional(total, &wagi)
        .into_iter()
        .zip(wyjscia)
        .map(|(kwota, o)| (o.good, kwota))
        .collect()
}

/// Koszt utylizacji odpadów jednej szarży, w groszach.
///
/// `Good::disposal_cost` jest podane **za tonę**, bo tak się go czyta i tak się go
/// negocjuje z odbiorcą odpadu.
#[must_use]
pub fn disposal_cost(cat: &Catalog, r: &Recipe) -> Money {
    let grosze: i128 = r
        .outputs
        .iter()
        .filter(|o| o.kind == OutputKind::Waste)
        .map(|o| i128::from(cat.good(o.good).disposal_cost.0) * i128::from(o.mass.0) / 1_000_000)
        .sum();
    Money(grosze as i64)
}

/// Masa, którą szarża wypuszcza jako odpad wymagający utylizacji.
#[must_use]
pub fn waste_mass(r: &Recipe) -> Mass {
    Mass(
        r.outputs
            .iter()
            .filter(|o| o.kind == OutputKind::Waste)
            .map(|o| o.mass.0)
            .sum(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{
        Emissions, GoodForm, HazardClass, NeedCategory, QualityModel, RecipeOutput, RecipeSource,
        Setup, StorageClass,
    };
    use magnat_core::{Energy, LossKind, NeedCategoryId, RecipeId, Volume};

    fn towar(i: u16, key: &str, cena: Option<i64>) -> crate::catalog::Good {
        crate::catalog::Good {
            key: key.into(),
            id: GoodId(i),
            category: NeedCategoryId(0),
            form: GoodForm::Liquid,
            density_g_per_l: 800,
            unit_mass: Mass::ZERO,
            unit_volume: Volume::ZERO,
            shelf_life_minutes: None,
            storage: StorageClass::Tank,
            hazard: HazardClass::None,
            has_quality: true,
            substitutes: Vec::new(),
            external_base_price: cena.map(Money),
            import_via: Vec::new(),
            disposal_cost: Money::ZERO,
        }
    }

    fn receptura(alokacja: CostAllocation, wyjscia: Vec<RecipeOutput>) -> Recipe {
        Recipe {
            key: "test".into(),
            id: RecipeId(0),
            source: RecipeSource::Manufacturing,
            inputs: Vec::new(),
            outputs: wyjscia,
            batch_mass: Mass(1_000_000),
            duration_minutes: 60,
            process_loss: Mass::ZERO,
            loss_kind: LossKind::ProcessWaste,
            energy: Energy::ZERO,
            water: Volume::ZERO,
            labour: Vec::new(),
            machine_class: "".into(),
            setup: Setup::default(),
            emissions: Emissions::default(),
            quality: QualityModel::default(),
            cost_allocation: alokacja,
        }
    }

    fn wyjscie(g: u16, masa: i64, kind: OutputKind) -> RecipeOutput {
        RecipeOutput {
            good: GoodId(g),
            mass: Mass(masa),
            kind,
        }
    }

    /// Sedno §5.4: przy alokacji wg masy asfalt kosztowałby na kilogram tyle co benzyna.
    /// Wg wartości bierze mniej — i to jest różnica, która decyduje o cenach w dół łańcucha.
    #[test]
    fn asfalt_nie_kosztuje_tyle_co_benzyna() {
        let cat = Catalog::from_parts(
            vec![
                towar(0, "fuel_petrol_95", Some(190)),
                towar(1, "mat_bitumen", Some(30)),
            ],
            Vec::new(),
            vec![NeedCategory {
                key: "test".to_string(),
                stock_cat: None,
            }],
            Vec::new(),
        );
        let wyjscia = vec![
            wyjscie(0, 500_000, OutputKind::Main),
            wyjscie(1, 500_000, OutputKind::ByProduct),
        ];

        let wg_masy = allocate_cost(
            &cat,
            &receptura(CostAllocation::ByMass, wyjscia.clone()),
            Money(1_000_000),
        );
        assert_eq!(wg_masy[0].1, wg_masy[1].1, "równa masa, równy koszt");

        let wg_wartosci = allocate_cost(
            &cat,
            &receptura(CostAllocation::ByMarketValue, wyjscia),
            Money(1_000_000),
        );
        // 190 : 30 przy równej masie. Reszta z dzielenia (1 grosz) idzie do pierwszej
        // pozycji wg wagi — dok. 00 §2 — więc benzyna ma 863 637, a nie 863 636.
        assert_eq!(wg_wartosci[0].1, Money(863_637));
        assert_eq!(wg_wartosci[1].1, Money(136_363));
        assert_eq!(wg_wartosci[0].1 .0 + wg_wartosci[1].1 .0, 1_000_000);
        assert!(wg_wartosci[0].1 > wg_wartosci[1].1);
    }

    /// Podział nie gubi ani grosza przy dowolnej kwocie — także niepodzielnej.
    #[test]
    fn podzial_kosztu_sumuje_sie_do_calosci() {
        let cat = Catalog::from_parts(
            vec![
                towar(0, "fuel_petrol_95", Some(190)),
                towar(1, "fuel_diesel_b7", Some(175)),
                towar(2, "mat_bitumen", Some(30)),
            ],
            Vec::new(),
            vec![NeedCategory {
                key: "test".to_string(),
                stock_cat: None,
            }],
            Vec::new(),
        );
        let r = receptura(
            CostAllocation::ByMarketValue,
            vec![
                wyjscie(0, 260_000, OutputKind::Main),
                wyjscie(1, 340_000, OutputKind::Main),
                wyjscie(2, 95_000, OutputKind::ByProduct),
            ],
        );
        for kwota in [1i64, 7, 999_999, 2_029_250, i64::from(u32::MAX)] {
            let v = allocate_cost(&cat, &r, Money(kwota));
            assert_eq!(
                v.iter().map(|(_, m)| m.0).sum::<i64>(),
                kwota,
                "kwota {kwota}"
            );
        }
    }

    /// Odpad i to, co zakład spala u siebie, **nie dostają kosztu**.
    #[test]
    fn odpad_i_samozuzycie_nie_biora_kosztu() {
        let cat = Catalog::from_parts(
            vec![
                towar(0, "fuel_petrol_95", Some(190)),
                towar(1, "waste_x", None),
            ],
            Vec::new(),
            vec![NeedCategory {
                key: "test".to_string(),
                stock_cat: None,
            }],
            Vec::new(),
        );
        let r = receptura(
            CostAllocation::ByMass,
            vec![
                wyjscie(0, 900_000, OutputKind::Main),
                wyjscie(1, 60_000, OutputKind::Waste),
                wyjscie(1, 40_000, OutputKind::SelfConsumed),
            ],
        );
        let v = allocate_cost(&cat, &r, Money(500_000));
        assert_eq!(v.len(), 1, "koszt dostaje tylko wyjście główne");
        assert_eq!(v[0], (GoodId(0), Money(500_000)));
        assert_eq!(waste_mass(&r), Mass(60_000));
    }
}
