//! Reguła gracza i polityka firmy AI wykonują się tym samym kodem (M7 §7.9, `K-11`).
//!
//! To jest test, **dla którego istnieje `sim/policy`**. PRD §6.3 obiecuje graczowi
//! „ten sam zestaw narzędzi co AI", a obietnica ta jest sprawdzalna tylko w jeden
//! sposób: ta sama reguła, podana raz jako polityka z edytora i raz jako polityka
//! wygenerowana przez tier taktyczny, musi dać **identyczną akcję i identyczny powód** —
//! łącznie z przypadkami brzegowymi, bo to w nich rozjeżdżają się dwa interpretery.
//!
//! Przypadki brzegowe są wypisane w §7.9 pkt 2 wprost i wszystkie trzy są tutaj:
//! brak konkurenta w promieniu, remis cenowy, konkurent poniżej kosztu.

use magnat_core::{DecisionReason, GoodId, Money, PolicyId, PriceBasis};
use magnat_policy::{
    evaluate, validate, Action, ArithOp, Bp, CmpOp, ConditionExpr, Expr, GoodRef, Metric,
    MetricCtx, Policy, PolicyDomain, PolicyView, Rule, Value,
};

/// Ile sklep wie o sobie i o sąsiadach. `None` w cenie konkurenta znaczy „nie widzę
/// żadnego w promieniu" — i to jest inny stan niż „widzę darmowego".
#[derive(Clone, Copy)]
struct Widok {
    cena: i64,
    koszt: i64,
    konkurenci: Option<(i64, i32)>,
}

impl PolicyView for Widok {
    fn metric(&self, m: Metric, _ctx: &MetricCtx) -> Option<Value> {
        match m {
            Metric::Price { .. } => Some(Value::Money(Money(self.cena))),
            Metric::UnitCost(_) => Some(Value::Money(Money(self.koszt))),
            Metric::CheapestCompetitorPrice { .. } => {
                self.konkurenci.map(|(c, _)| Value::Money(Money(c)))
            }
            Metric::CompetitorCount { .. } => {
                Some(Value::Count(self.konkurenci.map_or(0, |(_, n)| n)))
            }
            _ => None,
        }
    }
}

fn cena() -> Expr {
    Expr::Metric(Metric::Price {
        good: GoodRef::This,
        basis: PriceBasis::GrossRetail,
    })
}

fn najtanszy() -> Expr {
    Expr::Metric(Metric::CheapestCompetitorPrice {
        good: GoodRef::This,
        radius_m: 3_000,
        basis: PriceBasis::GrossRetail,
    })
}

/// „−2 % względem najtańszego konkurenta w promieniu 3 km" (PRD §6.3, §7.9 pkt 2),
/// z ogranicznikiem chroniącym przed sprzedażą poniżej kosztu.
///
/// Ogranicznik przechodzi przez cenę półkową, a nie przez koszt własny, i to jest
/// wymóg `K-7`, nie wygoda: koszt jest netto, cena brutto, a walidator odrzuca
/// porównanie mieszające podstawy. W edytorze M9 gracz sięgnąłby po jawną konwersję
/// `brutto(...)`; tutaj ogranicznik jest wyrażony w samej cenie półkowej.
fn regula_dyskontu(id: PolicyId) -> Policy {
    let mut p = Policy::empty(id, "Dyskont dzielnicowy", PolicyDomain::Pricing);
    p.cooldown_h = 12;
    p.rules.push(Rule {
        when: ConditionExpr::Cmp {
            lhs: Expr::Metric(Metric::CompetitorCount { radius_m: 3_000 }),
            op: CmpOp::Gt,
            rhs: Expr::Lit(Value::Count(0)),
        },
        then: smallvec::smallvec![
            Action::SetPrice {
                good: GoodRef::This,
                to: Expr::Bin {
                    lhs: Box::new(najtanszy()),
                    op: ArithOp::Pct,
                    rhs: Box::new(Expr::Lit(Value::Bp(Bp(9_800)))),
                },
            },
            Action::ClampPrice {
                good: GoodRef::This,
                min: Expr::Bin {
                    lhs: Box::new(cena()),
                    op: ArithOp::Pct,
                    rhs: Box::new(Expr::Lit(Value::Bp(Bp(8_000)))),
                },
                max: Expr::Bin {
                    lhs: Box::new(cena()),
                    op: ArithOp::Pct,
                    rhs: Box::new(Expr::Lit(Value::Bp(Bp(12_500)))),
                },
            },
        ],
        enabled: true,
        note: String::new(),
    });
    p.fallback = Some(Action::SetMargin {
        good: GoodRef::This,
        bp: Bp(2_500),
    });
    p
}

/// Ta sama reguła zapisana jako tekst RON — czyli tak, jak trafia do `data/policies/`
/// i jak M9 będzie ją zapisywać przy dzieleniu się polityką.
fn regula_z_danych() -> Policy {
    let txt = r#"(
        id: (7),
        name: "Dyskont dzielnicowy",
        domain: Pricing,
        rules: [(
            when: Cmp(
                lhs: Metric(CompetitorCount(radius_m: 3000)),
                op: Gt,
                rhs: Lit(Count(0)),
            ),
            then: [
                SetPrice(good: This, to: Bin(
                    lhs: Metric(CheapestCompetitorPrice(good: This, radius_m: 3000, basis: GrossRetail)),
                    op: Pct,
                    rhs: Lit(Bp((9800))),
                )),
                ClampPrice(
                    good: This,
                    min: Bin(lhs: Metric(Price(good: This, basis: GrossRetail)), op: Pct, rhs: Lit(Bp((8000)))),
                    max: Bin(lhs: Metric(Price(good: This, basis: GrossRetail)), op: Pct, rhs: Lit(Bp((12500)))),
                ),
            ],
        )],
        fallback: Some(SetMargin(good: This, bp: (2500))),
        cadence: Daily,
        cooldown_h: 12,
    )"#;
    ron::from_str(txt).expect("polityka z tekstu")
}

#[test]
fn gracz_i_ai_wykonuja_te_sama_regule_tym_samym_kodem() {
    // Jedna polityka, dwa źródła: edytor gracza (zbudowana w kodzie) i katalog
    // danych, z którego bierze ją tier taktyczny. Numer jest ten sam, bo to **ta
    // sama polityka przypięta do dwóch zakładów**, a nie dwie polityki o tej treści.
    let gracz = regula_dyskontu(PolicyId(7));
    let ai = regula_z_danych();
    assert_eq!(gracz, ai, "dwa źródła tej samej reguły dały różne drzewa");

    let ctx = MetricCtx::for_good(GoodId(1));
    for widok in stany() {
        let a = evaluate(&gracz, &ctx, &widok);
        let b = evaluate(&ai, &ctx, &widok);
        assert_eq!(a, b, "ta sama reguła, dwa różne wyniki");
        // Każda akcja niesie powód wskazujący **tę samą** regułę.
        for d in &a {
            assert!(matches!(
                d.reason(),
                DecisionReason::PolicyApplied {
                    policy: PolicyId(7),
                    ..
                }
            ));
        }
    }
}

/// Stany świata, na których sprawdza się równoważność — łącznie z trzema
/// przypadkami brzegowymi z §7.9 pkt 2.
fn stany() -> Vec<Widok> {
    let mut v = Vec::new();
    // Brak konkurenta w promieniu.
    v.push(Widok {
        cena: 700,
        koszt: 400,
        konkurenci: None,
    });
    // Remis cenowy — konkurent ma dokładnie tyle samo.
    v.push(Widok {
        cena: 700,
        koszt: 400,
        konkurenci: Some((700, 3)),
    });
    // Konkurent poniżej kosztu własnego.
    v.push(Widok {
        cena: 700,
        koszt: 400,
        konkurenci: Some((350, 1)),
    });
    // Pasmo zwykłych stanów — 1 000 kombinacji ceny i ceny konkurenta.
    for i in 0..1_000i64 {
        v.push(Widok {
            cena: 400 + i,
            koszt: 300 + i / 3,
            konkurenci: Some((380 + (i * 7) % 900, (i % 5) as i32)),
        });
    }
    v
}

#[test]
fn brak_konkurenta_w_promieniu_nie_jest_konkurentem_za_darmo() {
    let p = regula_dyskontu(PolicyId(7));
    let ctx = MetricCtx::for_good(GoodId(1));
    let bez = Widok {
        cena: 700,
        koszt: 400,
        konkurenci: None,
    };
    let d = evaluate(&p, &ctx, &bez);
    // Reguła się nie wyzwala (zero konkurentów), więc wykonuje się akcja zapasowa —
    // marża 25 %, a nie przecena do ceny, której nikt nie podał.
    assert_eq!(d.len(), 1);
    assert_eq!(
        **d[0].action(),
        Action::SetMargin {
            good: GoodRef::This,
            bp: Bp(2_500)
        }
    );
}

#[test]
fn remis_cenowy_daje_wynik_deterministyczny_a_nie_losowy() {
    let p = regula_dyskontu(PolicyId(7));
    let ctx = MetricCtx::for_good(GoodId(1));
    let remis = Widok {
        cena: 700,
        koszt: 400,
        konkurenci: Some((700, 2)),
    };
    let a = evaluate(&p, &ctx, &remis);
    for _ in 0..100 {
        assert_eq!(evaluate(&p, &ctx, &remis), a);
    }
    assert_eq!(a.len(), 2, "cena i ogranicznik");
}

#[test]
fn polityka_z_danych_przechodzi_walidator() {
    // Ten sam walidator, którym edytor M9 sprawdza politykę gracza — gdyby reguła
    // z pliku go nie przechodziła, gracz i AI mieliby jednak dwa różne języki.
    validate(&regula_z_danych()).expect("reguła z danych musi być poprawna");
    validate(&regula_dyskontu(PolicyId(7))).expect("reguła gracza musi być poprawna");
}
