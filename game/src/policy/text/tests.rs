//! Testy postaci tekstowej (M9 §7 „Język reguł").

use super::*;
use magnat_core::{rng, FirmId, Money, PolicyId, SiteId, StreamId, Tick};
use magnat_policy::{Action, ConditionExpr, Expr, GoodRef, OrderSource, Rule};

fn katalog() -> Catalog {
    Catalog::load().expect("data/locale/")
}

fn towary() -> GoodKeys {
    GoodKeys::new([
        (GoodId(1), "food_bread".to_string()),
        (GoodId(2), "food_milk".to_string()),
        (GoodId(7), "fuel_oil_crude".to_string()),
    ])
}

fn encja(i: u32) -> magnat_core::Entity {
    magnat_core::Entity::new(i, std::num::NonZeroU32::MIN)
}

/// Generator losowych, **poprawnych** polityk. Deterministyczny: ten sam numer daje
/// tę samą politykę, więc pęknięcie round-tripu da się powtórzyć.
struct Losowa {
    r: magnat_core::Rng,
}

impl Losowa {
    fn new(n: u64) -> Losowa {
        Losowa {
            r: rng(0xA11CE, StreamId::PolicyExecution, n as u32, Tick(n)),
        }
    }

    fn do_n(&mut self, n: u32) -> u32 {
        self.r.gen_range_u32(n)
    }

    fn towar(&mut self) -> GoodRef {
        match self.do_n(3) {
            0 => GoodRef::This,
            1 => GoodRef::Id(GoodId(1)),
            _ => GoodRef::Id(GoodId(7)),
        }
    }

    fn podstawa(&mut self) -> PriceBasis {
        if self.do_n(2) == 0 {
            PriceBasis::GrossRetail
        } else {
            PriceBasis::NetB2B
        }
    }

    fn bp(&mut self) -> Bp {
        // Także wartości nierówne setce punktów bazowych — zapis procentowy ma
        // trzymać setne części procenta, a nie zaokrąglać je po cichu.
        Bp(self.do_n(30_000) as i32 - 5_000)
    }

    /// Metryka i literał o tej samej jednostce — para, która **na pewno** przejdzie
    /// walidator, bo o to chodzi w round-tripie: sprawdzamy zapis, nie walidację.
    fn klauzula(&mut self) -> (Expr, CmpOp, Expr) {
        let t = self.towar();
        let (lhs, rhs) = match self.do_n(8) {
            0 => {
                let b = self.podstawa();
                (
                    Expr::Metric(Metric::Price { good: t, basis: b }),
                    Expr::Convert {
                        to: b,
                        of: Box::new(Expr::Metric(Metric::UnitCost(t))),
                    },
                )
            }
            1 => (
                Expr::Metric(Metric::StockDays(t)),
                Expr::Lit(Value::Days(self.do_n(60) as i32)),
            ),
            2 => (
                Expr::Metric(Metric::Stock(t)),
                Expr::Lit(Value::Qty(i64::from(self.do_n(9_000)))),
            ),
            3 => (
                Expr::Metric(Metric::CompetitorCount {
                    radius_m: self.do_n(10_000),
                }),
                Expr::Lit(Value::Count(self.do_n(9) as i32)),
            ),
            4 => (
                Expr::Metric(Metric::Season),
                Expr::Lit(Value::Enum(self.do_n(4) as u8)),
            ),
            5 => (
                Expr::Metric(Metric::DayOfWeek),
                Expr::Lit(Value::Enum(self.do_n(7) as u8)),
            ),
            6 => (
                Expr::Metric(Metric::CashBalance),
                Expr::Lit(Value::Money(Money(i64::from(self.do_n(1_000_000))))),
            ),
            _ => {
                let b = self.podstawa();
                let r = self.do_n(10_000);
                (
                    Expr::Metric(Metric::Price { good: t, basis: b }),
                    Expr::Bin {
                        lhs: Box::new(Expr::Metric(Metric::CheapestCompetitorPrice {
                            good: t,
                            radius_m: r,
                            basis: b,
                        })),
                        op: magnat_policy::ArithOp::Pct,
                        rhs: Box::new(Expr::Lit(Value::Bp(self.bp()))),
                    },
                )
            }
        };
        let op = [
            CmpOp::Lt,
            CmpOp::Le,
            CmpOp::Eq,
            CmpOp::Ne,
            CmpOp::Ge,
            CmpOp::Gt,
        ][self.do_n(6) as usize];
        (lhs, op, rhs)
    }

    fn akcja(&mut self) -> Action {
        let t = self.towar();
        match self.do_n(10) {
            0 => Action::SetPrice {
                good: t,
                to: Expr::Lit(Value::Money(Money(i64::from(self.do_n(100_000))))),
            },
            1 => Action::AdjustPrice {
                good: t,
                by: Expr::Lit(Value::Money(Money(i64::from(self.do_n(2_000)) - 1_000))),
            },
            2 => Action::SetMargin {
                good: t,
                bp: self.bp(),
            },
            3 => Action::Markdown {
                good: t,
                bp: self.bp(),
            },
            4 => Action::ClampPrice {
                good: t,
                min: Expr::Convert {
                    to: PriceBasis::GrossRetail,
                    of: Box::new(Expr::Metric(Metric::UnitCost(t))),
                },
                max: Expr::Lit(Value::Money(Money(i64::from(self.do_n(100_000))))),
            },
            5 => Action::OrderUpTo {
                good: t,
                days: Expr::Lit(Value::Days(self.do_n(60) as i32)),
            },
            6 => Action::OrderQty {
                good: t,
                qty: Expr::Lit(Value::Qty(i64::from(self.do_n(9_000)))),
                source: if self.do_n(2) == 0 {
                    OrderSource::Market
                } else {
                    OrderSource::Preferred
                },
            },
            7 => Action::RemoveFromShelf(t),
            8 => Action::Alert {
                msg: self.do_n(4) as u16,
                severity: [Severity::Info, Severity::Warning, Severity::Critical]
                    [self.do_n(3) as usize],
            },
            _ => Action::AskPlayer {
                msg: self.do_n(4) as u16,
            },
        }
    }

    fn polityka(&mut self) -> (Policy, PolicyScope) {
        let ile = 1 + self.do_n(magnat_policy::MAX_RULES as u32 - 1);
        let mut rules = Vec::new();
        for _ in 0..ile {
            let klauzul = self.do_n(3);
            let mut when = None;
            for _ in 0..klauzul {
                let (lhs, op, rhs) = self.klauzula();
                let c = ConditionExpr::Cmp { lhs, op, rhs };
                when = Some(match when {
                    None => c,
                    Some(w) => ConditionExpr::And(Box::new(w), Box::new(c)),
                });
            }
            let mut akcje = Vec::new();
            for _ in 0..=self.do_n(2) {
                akcje.push(self.akcja());
            }
            rules.push(Rule {
                when: when.unwrap_or(ConditionExpr::Always),
                then: akcje.into_iter().collect(),
                enabled: self.do_n(5) > 0,
                note: if self.do_n(3) == 0 {
                    "notatka gracza".to_string()
                } else {
                    String::new()
                },
            });
        }
        let scope = match self.do_n(5) {
            0 => PolicyScope::Firm(FirmId(encja(self.do_n(1_000)))),
            1 => PolicyScope::Site(SiteId(encja(self.do_n(1_000)))),
            2 => PolicyScope::Group(magnat_policy::TagId(self.do_n(50) as u16)),
            3 => PolicyScope::Product {
                inner: Box::new(PolicyScope::Site(SiteId(encja(self.do_n(1_000))))),
                good: GoodId(1),
            },
            _ => PolicyScope::Category {
                inner: Box::new(PolicyScope::Firm(FirmId(encja(self.do_n(1_000))))),
                cat: magnat_core::NeedCategoryId(self.do_n(20) as u16),
            },
        };
        let p = Policy {
            id: PolicyId(0),
            name: "polityka testowa".to_string(),
            domain: [
                PolicyDomain::Pricing,
                PolicyDomain::Stock,
                PolicyDomain::Hr,
                PolicyDomain::Production,
                PolicyDomain::Logistics,
            ][self.do_n(5) as usize],
            rules,
            fallback: if self.do_n(2) == 0 {
                Some(self.akcja())
            } else {
                None
            },
            cadence: if self.do_n(2) == 0 {
                Cadence::Daily
            } else {
                Cadence::Hourly
            },
            cooldown_h: self.do_n(48) as u8,
        };
        (p, scope)
    }
}

#[test]
fn zaden_jezyk_nie_nadpisuje_slowa_drugiego() {
    let k = super::kolizje(&katalog());
    assert!(k.is_empty(), "{k:?}");
}

#[test]
fn round_trip_dziesieciu_tysiecy_polityk() {
    let c = katalog();
    let g = towary();
    for l in Locale::ALL {
        for n in 0..5_000u64 {
            let (p, s) = Losowa::new(n).polityka();
            let txt = write(&p, &s, &g, &c, l);
            let (p2, s2) =
                parse(&txt, &g, &c).unwrap_or_else(|e| panic!("polityka {n} ({l:?}): {e}\n{txt}"));
            assert_eq!(p, p2, "polityka {n} ({l:?})\n{txt}");
            assert_eq!(s, s2, "zakres {n} ({l:?})\n{txt}");
        }
    }
}

#[test]
fn tekst_zapisany_po_polsku_czyta_sie_u_gracza_z_angielskim() {
    let c = katalog();
    let g = towary();
    let (p, s) = Losowa::new(42).polityka();
    let pl = write(&p, &s, &g, &c, Locale::Pl);
    let (p2, _) = parse(&pl, &g, &c).expect("polityka po polsku");
    assert_eq!(p, p2);
    // I odwrotnie — bo dzielenie się politykami działa w obie strony albo wcale.
    let en = write(&p, &s, &g, &c, Locale::En);
    assert_eq!(parse(&en, &g, &c).expect("polityka po angielsku").0, p);
    assert_ne!(pl, en, "oba języki dały ten sam tekst");
}

#[test]
fn import_mieszajacy_brutto_z_netto_jest_odrzucony() {
    // `K-7`: formularz takiej klauzuli nie złoży, więc import jest jedyną drogą,
    // którą ona wchodzi — i tam musi zostać zatrzymana.
    let c = katalog();
    let g = towary();
    let p = Policy {
        id: PolicyId(0),
        name: "mieszanka".to_string(),
        domain: PolicyDomain::Pricing,
        rules: vec![Rule {
            when: ConditionExpr::Cmp {
                lhs: Expr::Metric(Metric::Price {
                    good: GoodRef::This,
                    basis: PriceBasis::GrossRetail,
                }),
                op: CmpOp::Gt,
                rhs: Expr::Metric(Metric::UnitCost(GoodRef::This)),
            },
            then: Vec::new().into_iter().collect(),
            enabled: true,
            note: String::new(),
        }],
        fallback: None,
        cadence: Cadence::Daily,
        cooldown_h: 12,
    };
    let s = PolicyScope::Site(SiteId(encja(1)));
    let txt = write(&p, &s, &g, &c, Locale::Pl);
    assert!(matches!(
        parse(&txt, &g, &c),
        Err(TextError::PriceBasisMismatch {
            lhs: PriceBasis::GrossRetail,
            rhs: PriceBasis::NetB2B
        })
    ));
}

#[test]
fn nieznany_towar_jest_bledem_a_nie_cichym_zerem() {
    let c = katalog();
    let pusty = GoodKeys::default();
    let p = Policy {
        id: PolicyId(0),
        name: "obcy towar".to_string(),
        domain: PolicyDomain::Stock,
        rules: vec![Rule {
            when: ConditionExpr::Always,
            then: [Action::RemoveFromShelf(GoodRef::Id(GoodId(1)))]
                .into_iter()
                .collect(),
            enabled: true,
            note: String::new(),
        }],
        fallback: None,
        cadence: Cadence::Daily,
        cooldown_h: 0,
    };
    let s = PolicyScope::Site(SiteId(encja(1)));
    let txt = write(&p, &s, &towary(), &c, Locale::Pl);
    assert!(matches!(
        parse(&txt, &pusty, &c),
        Err(TextError::UnknownGood(_))
    ));
}

#[test]
fn preset_z_repozytorium_zapisuje_sie_i_wraca() {
    // Sztandarowa polityka z PRD §6.3 przechodzi przez tekst bez straty — razem
    // z jawną konwersją `brutto(koszt × 105 %)`, na której `K-7` stoi.
    let c = katalog();
    let g = towary();
    let kat = magnat_policy::PolicyCatalog::load_default().expect("data/policies");
    let s = PolicyScope::Group(magnat_policy::TagId(1));
    for pr in kat.iter() {
        let txt = write(&pr.policy, &s, &g, &c, Locale::Pl);
        let (p2, s2) = parse(&txt, &g, &c).unwrap_or_else(|e| panic!("{}: {e}\n{txt}", pr.key));
        assert_eq!(pr.policy, p2, "preset {}\n{txt}", pr.key);
        assert_eq!(s, s2);
    }
}
