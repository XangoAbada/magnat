//! `deposit_ledger` — test własnościowy księgi złóż (M1 §7.1, 00 §6).
//!
//! Złoże jest jedynym miejscem w M1, w którym trzymana jest **masa**, a kontrakt §6 wymaga
//! od masy tego samego co od pieniądza: arytmetyki całkowitej i bilansu z tolerancją zero.
//! Dlatego niezmienniki sprawdza test własnościowy na dziesięciu tysiącach losowań, a nie
//! kilka ręcznie wpisanych przypadków — błąd przepełnienia albo ucieczki masy pojawia się
//! przy konkretnej kombinacji zasobów i żądań, a nie przy tej, którą akurat ktoś wymyślił.

use magnat_core::Mass;
use magnat_core::ResourceKind;
use magnat_core::{IVec3, Q};
use magnat_world::{Deposit, DepositId, DepositShape};
use proptest::prelude::*;

/// Złoże o zadanych zasobach. Kształt i geologia nie mają znaczenia dla księgi masy,
/// więc są ustalone — test pilnuje arytmetyki, nie geometrii.
fn zloze(reserves: i64) -> Deposit {
    Deposit {
        id: DepositId(1),
        resource: ResourceKind::Coal,
        shape: DepositShape::Ellipsoid {
            center: IVec3::new(0, 0, -100),
            radii: IVec3::new(50, 50, 20),
            yaw_deg: 0,
        },
        volume_m3: 1000,
        concentration: Q::new(50),
        reserves: Mass(reserves),
        extracted: Mass(0),
        depth_top_m: 10,
        depth_bottom_m: 30,
        discovered: true,
        quality: Q::new(70),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    /// Ciąg wydobyć nie może wyprodukować ani zgubić grama.
    #[test]
    fn ksiega_zloza_bilansuje_sie_do_grama(
        reserves in 1i64..i64::from(u32::MAX),
        zadania in prop::collection::vec(-1000i64..i64::MAX / 4, 1..12),
    ) {
        let mut d = zloze(reserves);
        let mut wydobyto = 0i64;
        for zadanie in zadania {
            let got = d.extract(Mass(zadanie)).0;
            // Żądanie ujemne albo zerowe nie ma prawa niczego dodać do złoża — inaczej
            // „wydobycie" ujemnej masy byłoby darmowym źródłem surowca.
            prop_assert!(got >= 0, "wydobycie ujemne: {got} przy żądaniu {zadanie}");
            prop_assert!(
                got <= zadanie.max(0),
                "wydobyto {got} przy żądaniu {zadanie}"
            );
            wydobyto += got;

            prop_assert!(d.extracted.0 >= 0, "ujemne `extracted`");
            prop_assert!(
                d.extracted.0 <= d.reserves.0,
                "extracted {} > reserves {}",
                d.extracted.0,
                d.reserves.0
            );
            prop_assert!(d.remaining().0 >= 0, "ujemna pozostałość");
            prop_assert_eq!(
                d.remaining().0 + d.extracted.0,
                d.reserves.0,
                "pozostałość i wydobycie nie sumują się do zasobów"
            );
        }
        prop_assert_eq!(wydobyto, d.extracted.0, "suma wydobyć ≠ `extracted`");
    }

    /// Zasoby policzone z objętości i koncentracji nigdy nie przepełniają `i64`
    /// i rosną monotonicznie z obiema wielkościami.
    #[test]
    fn zasoby_nie_przepelniaja_sie_i_rosna_z_objetoscia(
        v in 1i64..1_000_000,
        c in 1u8..=100,
        gestosc in 800u16..8000,
    ) {
        let a = Deposit::reserves_from(v, gestosc, Q::new(c));
        let b = Deposit::reserves_from(v * 2, gestosc, Q::new(c));
        prop_assert!(a.0 > 0, "zerowe zasoby przy objętości {v}");
        prop_assert!(b.0 >= a.0, "podwojenie objętości zmniejszyło zasoby");
    }
}
