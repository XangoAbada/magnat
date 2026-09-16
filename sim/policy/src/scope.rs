//! Zakres stosowania polityki i rozstrzyganie konfliktów (M9d §5.6, M7c WP6b).
//!
//! Porządek jest **od najbardziej szczegółowego**:
//! `Product@Site > Category@Site > Site > Product@Group > Group > Product@Firm > Firm`.
//!
//! Konflikt — dwie polityki o tej samej szczegółowości na tym samym celu — jest
//! **błędem walidacji**, a nie losowaniem. Gdyby mimo to trafił do wykonania (polityka
//! z zapisu gry sprzed zmiany danych), wygrywa niższy indeks na liście: deterministycznie
//! i tak samo w każdym przebiegu.

use magnat_core::{FirmId, GoodId, NeedCategoryId, SiteId};
use serde::{Deserialize, Serialize};

/// Etykieta grupy zakładów („sklepy w Śródmieściu"). Indeks w liście grup firmy;
/// grupy prowadzi M9 razem z edytorem, `sim/policy` zna wyłącznie numer.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize,
)]
#[repr(transparent)]
pub struct TagId(pub u16);

/// Zakres stosowania.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum PolicyScope {
    Firm(FirmId),
    Site(SiteId),
    Group(TagId),
    Product {
        inner: Box<PolicyScope>,
        good: GoodId,
    },
    Category {
        inner: Box<PolicyScope>,
        cat: NeedCategoryId,
    },
}

impl PolicyScope {
    /// Szczegółowość: im większa, tym zakres węższy. Liczba jest **porządkiem**,
    /// nie wagą — nigdzie się jej nie sumuje.
    ///
    /// Baza jest rozstawiona co trzy — firma 0, grupa 3, zakład 6 — a zawężenie
    /// dokłada 1 za kategorię i 2 za towar. Odstęp musi być większy od największego
    /// zawężenia, inaczej `Product@Group` zrównałoby się z `Site` i porządek z M9d
    /// przestałby być porządkiem. Wychodzi: 8 · 7 · 6 · 5 · 3 · 2 · 0.
    #[must_use]
    pub fn specificity(&self) -> u8 {
        match self {
            PolicyScope::Firm(_) => 0,
            PolicyScope::Group(_) => 3,
            PolicyScope::Site(_) => 6,
            PolicyScope::Category { inner, .. } => inner.specificity() + 1,
            PolicyScope::Product { inner, .. } => inner.specificity() + 2,
        }
    }

    /// Zakład, do którego zakres się odnosi — `None` dla zakresów szerszych niż zakład.
    #[must_use]
    pub fn site(&self) -> Option<SiteId> {
        match self {
            PolicyScope::Site(s) => Some(*s),
            PolicyScope::Product { inner, .. } | PolicyScope::Category { inner, .. } => {
                inner.site()
            }
            _ => None,
        }
    }

    /// Czy zakres obejmuje ten towar. Zakres bez zawężenia towarowego obejmuje każdy.
    #[must_use]
    pub fn covers_good(&self, good: GoodId, category: Option<NeedCategoryId>) -> bool {
        match self {
            PolicyScope::Product { inner, good: g } => {
                *g == good && inner.covers_good(good, category)
            }
            PolicyScope::Category { inner, cat } => {
                category == Some(*cat) && inner.covers_good(good, category)
            }
            _ => true,
        }
    }
}

/// Wybiera politykę, która ma wykonać się na tym celu.
///
/// Wejściem jest lista `(zakres, indeks)` w kolejności, w której polityki stoją
/// u właściciela. Wygrywa największa szczegółowość; przy remisie — niższy indeks.
/// Zwraca indeks, a nie referencję, bo wołający i tak trzyma listę i tak samo jak
/// walidator musi umieć powiedzieć, **która** to była pozycja.
#[must_use]
pub fn resolve(scopes: &[PolicyScope]) -> Option<usize> {
    scopes
        .iter()
        .enumerate()
        .max_by_key(|(i, s)| (s.specificity(), std::cmp::Reverse(*i)))
        .map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::Entity;
    use std::num::NonZeroU32;

    fn ent(i: u32) -> Entity {
        Entity::new(i, NonZeroU32::MIN)
    }

    fn produkt(inner: PolicyScope, g: u16) -> PolicyScope {
        PolicyScope::Product {
            inner: Box::new(inner),
            good: GoodId(g),
        }
    }

    #[test]
    fn porzadek_szczegolowosci_jest_ten_z_m9d() {
        let firma = PolicyScope::Firm(FirmId(ent(1)));
        let grupa = PolicyScope::Group(TagId(1));
        let zaklad = PolicyScope::Site(SiteId(ent(2)));
        let kat = |inner: PolicyScope| PolicyScope::Category {
            inner: Box::new(inner),
            cat: NeedCategoryId(1),
        };
        // Product@Site > Category@Site > Site > Product@Group > Group > Product@Firm > Firm
        let kolejnosc = [
            produkt(zaklad.clone(), 1),
            kat(zaklad.clone()),
            zaklad.clone(),
            produkt(grupa.clone(), 1),
            grupa.clone(),
            produkt(firma.clone(), 1),
            firma.clone(),
        ];
        for para in kolejnosc.windows(2) {
            assert!(
                para[0].specificity() > para[1].specificity(),
                "{:?} nie bije {:?}",
                para[0],
                para[1]
            );
        }
    }

    #[test]
    fn remis_rozstrzyga_nizszy_indeks_a_nie_losowanie() {
        let a = PolicyScope::Site(SiteId(ent(2)));
        let b = PolicyScope::Site(SiteId(ent(2)));
        assert_eq!(resolve(&[a, b]), Some(0));
    }

    #[test]
    fn zakres_towarowy_nie_obejmuje_cudzego_towaru() {
        let s = produkt(PolicyScope::Site(SiteId(ent(2))), 7);
        assert!(s.covers_good(GoodId(7), None));
        assert!(!s.covers_good(GoodId(8), None));
        // Zakres bez zawężenia obejmuje każdy towar.
        assert!(PolicyScope::Site(SiteId(ent(2))).covers_good(GoodId(8), None));
    }

    #[test]
    fn zakres_zna_swoj_zaklad_takze_przez_zawezenie() {
        let s = produkt(PolicyScope::Site(SiteId(ent(9))), 7);
        assert_eq!(s.site(), Some(SiteId(ent(9))));
        assert_eq!(PolicyScope::Group(TagId(0)).site(), None);
    }
}
