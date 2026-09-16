//! Ewaluacja nie alokuje na ścieżce gorącej (kryterium WP6b, M7 §7.9 pkt 4).
//!
//! **Osobny plik, czyli osobna binarka testowa, i to nie jest kosmetyka.** Licznik
//! alokacji jest globalny, a `cargo test` puszcza testy z jednego pliku równolegle —
//! pomiar w pliku z innymi testami mierzyłby ich alokacje razem ze swoimi. Tu jest
//! jeden test i nikt mu nie przeszkadza.
//!
//! Dlaczego to jest kryterium, a nie ozdoba: politykę wykonuje się raz na towar
//! na dobę, czyli rzędu miliona razy na dobę gry przy dwustu sklepach gracza i firmach
//! AI obok. Alokacja na tej ścieżce nie objawia się jako błąd, tylko jako budżet
//! klatki, którego nie da się później odzyskać bez przepisania ewaluatora.

use magnat_core::{GoodId, Money, PolicyId, PriceBasis};
use magnat_policy::{
    evaluate, Action, ArithOp, Bp, CmpOp, ConditionExpr, Expr, GoodRef, Metric, MetricCtx, Policy,
    PolicyDomain, PolicyView, Rule, Value,
};

struct Widok;

impl PolicyView for Widok {
    fn metric(&self, m: Metric, _ctx: &MetricCtx) -> Option<Value> {
        match m {
            Metric::Price { .. } => Some(Value::Money(Money(700))),
            Metric::CheapestCompetitorPrice { .. } => Some(Value::Money(Money(651))),
            Metric::CompetitorCount { .. } => Some(Value::Count(3)),
            _ => None,
        }
    }
}

fn polityka() -> Policy {
    let mut p = Policy::empty(PolicyId(7), "Dyskont", PolicyDomain::Pricing);
    p.cooldown_h = 12;
    p.rules.push(Rule {
        when: ConditionExpr::Cmp {
            lhs: Expr::Metric(Metric::CompetitorCount { radius_m: 3_000 }),
            op: CmpOp::Gt,
            rhs: Expr::Lit(Value::Count(0)),
        },
        then: smallvec::smallvec![Action::SetPrice {
            good: GoodRef::This,
            to: Expr::Bin {
                lhs: Box::new(Expr::Metric(Metric::CheapestCompetitorPrice {
                    good: GoodRef::This,
                    radius_m: 3_000,
                    basis: PriceBasis::GrossRetail,
                })),
                op: ArithOp::Pct,
                rhs: Box::new(Expr::Lit(Value::Bp(Bp(9_800)))),
            },
        }],
        enabled: true,
        note: String::new(),
    });
    p
}

#[test]
fn tysiac_ewaluacji_nie_alokuje_ani_razu() {
    let p = polityka();
    let ctx = MetricCtx::for_good(GoodId(1));
    // Rozgrzewka poza pomiarem: licznik ma startować od stanu ustalonego.
    std::hint::black_box(evaluate(&p, &ctx, &Widok));
    let przed = ALOKACJE.load(Ordering::Relaxed);
    for _ in 0..1_000 {
        std::hint::black_box(evaluate(&p, &ctx, &Widok));
    }
    let po = ALOKACJE.load(Ordering::Relaxed);
    assert_eq!(
        po - przed,
        0,
        "ewaluacja zaalokowała {} razy na 1 000 przebiegów",
        po - przed
    );
}

// ── licznik alokacji ─────────────────────────────────────────────────────────────

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

static ALOKACJE: AtomicUsize = AtomicUsize::new(0);

/// Alokator zliczający: opakowanie nad systemowym, które nic poza liczeniem nie robi.
struct Liczacy;

// SAFETY: przekazuje oba wywołania do `System` bez zmiany argumentów i wyniku,
// więc kontrakt `GlobalAlloc` jest ten sam co u niego. Jedyny efekt uboczny to
// inkrementacja licznika atomowego, która nie dotyka pamięci alokatora.
unsafe impl GlobalAlloc for Liczacy {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALOKACJE.fetch_add(1, Ordering::Relaxed);
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
    }
}

#[global_allocator]
static ALOC: Liczacy = Liczacy;
