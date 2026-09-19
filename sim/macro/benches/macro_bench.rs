//! Budżet kroku makro (M10a/WP10.1, M10 §7.5).
//!
//! Kryterium jest jedno i twarde: **7500 kroków metropolii ≤ 60 s jednowątkowo**,
//! czyli ≤ 8 ms na krok. 7500 to osiemdziesiąt lat historii „na sucho" przy kroku
//! jednodobowym — a dokładnie tyle liczy się przy starcie nowej partii, więc ta
//! liczba jest czasem, który gracz spędza na ekranie ładowania.
//!
//! Stan metropolii budujemy tu **syntetycznie**, a nie przez `lift()` prawdziwego
//! świata: benchmark ma mierzyć krok, a nie generator miasta, i ma dać tę samą
//! liczbę na maszynie bez `data/`.

use criterion::{criterion_group, criterion_main, Criterion};
use magnat_core::{DistrictId, FirmId, GoodId, Money, Qty};
use magnat_macro::{
    step, CitizenSeed, ClassGrain, ClassId, MacroCell, MacroFirm, MacroParams, MacroState,
    MacroStock, SparseVec, TechLevel,
};

/// Metropolia z §4.1: 400 tys. mieszkańców, 40 dzielnic, ~3 tys. firm.
const DZIELNICE: u16 = 40;
const MIESZKANCOW: u32 = 400_000;
const FIRM: u32 = 3_000;
const TOWAROW_NA_FIRME: u16 = 16;
/// Ile różnych towarów jest w obrocie detalicznym całego miasta. Liczba z §5.7:
/// rachunek budżetu kroku brzmi tam „240 komórek × ~60 towarów", a nie „× cały
/// katalog" — fala A `data/goods/` ma ~400 pozycji, ale większość z nich to wsad
/// przemysłowy, którego gospodarstwo nie kupuje.
const TOWAROW_W_OBROCIE: u16 = 60;
const ROL: usize = 46;

/// Generacja encji w stanie syntetycznym — dowolna, byle stała.
const GEN: std::num::NonZeroU32 = std::num::NonZeroU32::new(1).unwrap();

fn metropolia() -> MacroState {
    let grain = ClassGrain::Classes6;
    let mut st = MacroState::empty(grain, ROL, DZIELNICE);
    let klas = u16::from(grain.classes());
    let komorek = u32::from(DZIELNICE) * u32::from(klas);
    let na_komorke = MIESZKANCOW / komorek;

    let mut indeks = 0u32;
    for d in 0..DZIELNICE {
        for k in 0..klas {
            let mut c = MacroCell::new((DistrictId(d), ClassId(k as u8)), ROL);
            c.citizens = (0..na_komorke)
                .map(|_| {
                    indeks += 1;
                    CitizenSeed {
                        birth_index: indeks,
                        cell: 0,
                    }
                })
                .collect();
            for (i, slot) in c.age_hist.iter_mut().enumerate() {
                *slot = na_komorke / 18 + (i as u32 % 3);
            }
            c.cash = Money(i64::from(na_komorke) * 120_000);
            c.deposits = Money(i64::from(na_komorke) * 400_000);
            c.wealth_q = [
                Money(10_000),
                Money(80_000),
                Money(600_000),
                Money(9_000_000),
            ];
            c.employed = na_komorke * 55 / 100;
            c.unemployed = na_komorke * 5 / 100;
            for r in 0..ROL {
                c.labor[r] = na_komorke / 40;
                c.skill_sum[r] = c.labor[r] * 55;
            }
            st.cells.push(c);
        }
    }

    for f in 0..FIRM {
        let mut price: SparseVec<GoodId, Money> = SparseVec::new();
        let mut cost: SparseVec<GoodId, Money> = SparseVec::new();
        let mut stock = MacroStock::new();
        for g in 0..TOWAROW_NA_FIRME {
            let good = GoodId(((f as u16).wrapping_mul(7).wrapping_add(g * 3)) % TOWAROW_W_OBROCIE);
            price.set(good, Money(1_200 + i64::from(g) * 70));
            cost.set(good, Money(1_000 + i64::from(g) * 60));
            stock.add(good, 40_000);
        }
        st.firms.push(MacroFirm {
            id: FirmId(magnat_core::Entity::new(f + 1, GEN)),
            district: DistrictId((f % u32::from(DZIELNICE)) as u16),
            branch: magnat_macro::BranchId((f % 9) as u16),
            capital: Money(50_000_000),
            debt: Money(12_000_000),
            stock,
            capacity_daily: Qty(magnat_firms::hr::productivity::FULL_TIME * 25),
            utilization_bps: 0,
            employees: 20,
            wage_bill: Money(9_000_000),
            price,
            cost,
            brand_stock: 0,
            tech: TechLevel(1),
            suppliers: smallvec::SmallVec::new(),
        });
    }
    st.firms.sort_unstable_by_key(|f| f.id.0.to_bits());
    st.ledger.set(
        magnat_macro::MacroAccount::Households,
        Money(
            st.cells
                .iter()
                .map(|c| c.cash.get())
                .fold(0i64, i64::saturating_add),
        ),
    );
    st.ledger.set(
        magnat_macro::MacroAccount::Firms,
        Money(
            st.firms
                .iter()
                .map(|f| f.capital.get())
                .fold(0i64, i64::saturating_add),
        ),
    );
    st
}

/// Ile dób mierzy jeden przebieg. Miesiąc, a nie doba, i to jest **warunek
/// uczciwości pomiaru**: trzy z ośmiu faz nie chodzą codziennie — relacje raz
/// na trzydzieści dób, demografia raz na rok — więc pojedynczy krok mierzyłby
/// albo dobę z przeglądem dostawców, albo dobę bez niego, i żadna z tych liczb
/// nie byłaby średnią. Budżet przeliczony na krok: 8 ms (7500 kroków ≤ 60 s).
const DOB_W_PRZEBIEGU: u32 = 30;

fn krok_metropolii(c: &mut Criterion) {
    let wzorzec = metropolia();
    let params = MacroParams::default();
    let mut g = c.benchmark_group("m10-macro");
    g.throughput(criterion::Throughput::Elements(u64::from(DOB_W_PRZEBIEGU)));
    g.bench_function("m10-1-miesiac-metropolia", |b| {
        b.iter_batched_ref(
            || wzorzec.clone(),
            |st| {
                for _ in 0..DOB_W_PRZEBIEGU {
                    step(st, &params);
                }
            },
            criterion::BatchSize::LargeInput,
        );
    });
    g.finish();
}

criterion_group!(benches, krok_metropolii);
criterion_main!(benches);
