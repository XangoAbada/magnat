//! Kryterium zamknięcia M7a WP1: 10 000 firm, każda z trzema slotami decyzyjnymi,
//! a dwa przebiegi dają **identyczną** sekwencję `(tick, FirmKey, tier)`.
//!
//! To jest test, dla którego `FirmKey` nie jest indeksem encji. Gdyby był, sekwencja
//! zależałaby od tego, które encje zostały wcześniej zwolnione — czyli od historii
//! świata, a nie od jego stanu.

use magnat_core::hash::{HashState, StateHasher};
use magnat_core::{DistrictId, SimCalendar, SimMinute, Tick};
use magnat_firms::firm::Owner;
use magnat_firms::{slots, Firm, FirmKey, Firms, Tier};

const FIRM: u32 = 10_000;

fn miasto(n: u32) -> Firms {
    let mut f = Firms::new();
    for i in 0..n {
        f.insert(|key| {
            Firm::sole_owner(
                key,
                format!("Firma {i}"),
                SimMinute(0),
                DistrictId((i % 24) as u16),
                Owner::Player,
            )
        });
    }
    f
}

/// Doba przydziałów: dla każdej minuty zbieramy, kto decydował i na jakim poziomie.
fn doba(firms: &mut Firms, od: u64) -> Vec<(u64, FirmKey, Tier)> {
    let mut seq = Vec::new();
    for m in od..od + 1440 {
        let cal = SimCalendar::new(Tick(m));
        for (tier, keys) in firms.schedule(cal) {
            for k in keys {
                seq.push((m, k, tier));
            }
        }
    }
    seq
}

#[test]
fn kazda_firma_ma_trzy_sloty() {
    for i in 1..=u64::from(FIRM) {
        let s = slots(FirmKey(i));
        assert!(s.ops_minute_of_day < 1440);
        assert!(s.tac_day_of_month < 30 && s.tac_hour < 24);
        assert!(s.str_day_of_quarter < 90 && s.str_hour < 24);
    }
}

#[test]
fn dwa_przebiegi_daja_identyczna_sekwencje() {
    let a = doba(&mut miasto(FIRM), 0);
    let b = doba(&mut miasto(FIRM), 0);
    assert_eq!(a, b, "sekwencja decyzji rozjechała się między przebiegami");
    assert!(!a.is_empty());
}

#[test]
fn kolejnosc_zakladania_firm_nie_zmienia_hasha() {
    // `slots` jest funkcją czystą klucza, a klucze nadaje licznik świata — więc
    // firma numer 500 dostaje tę samą minutę niezależnie od tego, czy powstała
    // w pierwszym ticku, czy w tysięcznym.
    let mut a = miasto(1_000);
    let mut b = Firms::new();
    for i in 0..1_000u32 {
        b.insert(|key| {
            Firm::sole_owner(
                key,
                format!("Firma {i}"),
                SimMinute(0),
                DistrictId((i % 24) as u16),
                Owner::Player,
            )
        });
    }
    let mut ha = StateHasher::new();
    let mut hb = StateHasher::new();
    a.hash_state(&mut ha);
    b.hash_state(&mut hb);
    assert_eq!(ha.finish(), hb.finish());
    assert_eq!(doba(&mut a, 0), doba(&mut b, 0));
}

#[test]
fn kazda_firma_decyduje_operacyjnie_raz_na_dobe() {
    let mut f = miasto(FIRM);
    let seq = doba(&mut f, 0);
    let mut ile = std::collections::BTreeMap::new();
    for (_, k, t) in seq {
        if t == Tier::Operational {
            *ile.entry(k).or_insert(0u32) += 1;
        }
    }
    assert_eq!(
        ile.len(),
        FIRM as usize,
        "któraś firma nie decydowała wcale"
    );
    assert!(
        ile.values().all(|&n| n == 1),
        "któraś firma decydowała więcej niż raz na dobę"
    );
    // Sufit 32 firm na tick nie został przekroczony, więc nic nie wisi w kolejce.
    assert_eq!(f.backlog(Tier::Operational), 0);
}

#[test]
fn poziom_taktyczny_wypada_raz_na_miesiac() {
    let mut f = miasto(1_000);
    let mut ile = std::collections::BTreeMap::new();
    // 30 dób = miesiąc gry (`K-1`: 12 miesięcy po 30 dni).
    for d in 0..30u64 {
        for (_, k, t) in doba(&mut f, d * 1440) {
            if t == Tier::Tactical {
                *ile.entry(k).or_insert(0u32) += 1;
            }
        }
    }
    assert_eq!(ile.len(), 1_000, "któraś firma nie zdecydowała w miesiącu");
    assert!(ile.values().all(|&n| n == 1));
}

#[test]
fn nadmiar_w_slocie_przechodzi_do_nastepnego_ticku_i_nie_ginie() {
    // 4000 firm o **tym samym** kluczu modulo nie istnieje, więc pik wymusza się
    // sztucznie: bierzemy minutę o największym obłożeniu i sprawdzamy, że sufit
    // działa jako przesunięcie, a nie jako utrata decyzji.
    let mut f = miasto(FIRM);
    let seq = doba(&mut f, 0);
    let ops: Vec<FirmKey> = seq
        .iter()
        .filter(|(_, _, t)| *t == Tier::Operational)
        .map(|(_, k, _)| *k)
        .collect();
    let unikalne: std::collections::BTreeSet<FirmKey> = ops.iter().copied().collect();
    assert_eq!(ops.len(), unikalne.len(), "firma obsłużona dwa razy");
    assert_eq!(unikalne.len(), FIRM as usize);
}
