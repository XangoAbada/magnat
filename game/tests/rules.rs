//! Sześć przykładowych polityk z M9d §5.6 zbudowanych **wyłącznie klikaniem** (WP8).
//!
//! Test nie dotyka `Policy`, `Rule`, `Expr` ani `Action` bezpośrednio: wszystko idzie
//! przez [`Edit`], czyli przez to samo wejście, które ma gracz. Gdyby składał drzewo
//! ręcznie, sprawdzałby budowanie drzewa — a pytanie brzmi, czy **formularz** to umie.
//!
//! Trzy z sześciu polityk z dokumentu **nie dają się dziś przypiąć** i to jest wynik
//! testu, a nie jego awaria. Powody są wypisane przy każdej z nich; zbiorczo siedzą
//! w tabeli korekt `M9d` (`DF-1`, `DF-2`).

use magnat_core::{Entity, FirmId, GoodId, Money, PolicyId, PriceBasis, SiteId};
use magnat_game::policy::{ActionDraft, Base, Clause, Edit, EditError, Note, RuleEditor, Slot};
use magnat_policy::{
    Bp, Cadence, CmpOp, Diagnostic, GoodRef, Metric, PolicyDomain, PolicyScope, Severity, TagId,
    Value,
};
use magnat_ui::{Catalog, Locale};

fn encja(i: u32) -> Entity {
    Entity::new(i, std::num::NonZeroU32::MIN)
}

fn cena_brutto() -> Metric {
    Metric::Price {
        good: GoodRef::This,
        basis: PriceBasis::GrossRetail,
    }
}

fn klauzula(lhs: Slot, op: CmpOp, rhs: Slot) -> Clause {
    Clause { lhs, op, rhs }
}

fn lit(v: Value) -> Slot {
    Slot::lit(v)
}

fn met(m: Metric) -> Slot {
    Slot::metric(m)
}

/// Czy edytor ma same ostrzeżenia, czyli czy politykę wolno przypiąć.
fn bez_bledow(e: &RuleEditor) -> bool {
    !e.notes().iter().any(Note::is_error)
}

/// „Dyskont dzielnicowy" — sztandarowa polityka z PRD §6.3.
///
/// Cena dwa procent pod najtańszym konkurentem w promieniu trzech kilometrów,
/// przycięta do widełek liczonych **z kosztu własnego**; marża 25 % jako zapasowa.
/// Ogranicznik przechodzi przez jawne `brutto(...)`, bo koszt jest netto, a cena
/// półkowa brutto (`K-7`).
fn dyskont() -> RuleEditor {
    let mut e = RuleEditor::new(
        PolicyId(1),
        "Dyskont dzielnicowy",
        PolicyDomain::Pricing,
        PolicyScope::Group(TagId(1)),
    );
    e.apply(Edit::Cadence(Cadence::Daily)).unwrap();
    e.apply(Edit::Cooldown(12)).unwrap();
    e.apply(Edit::AddRule).unwrap();
    e.apply(Edit::AddClause(
        0,
        klauzula(
            met(Metric::CompetitorCount { radius_m: 3_000 }),
            CmpOp::Gt,
            lit(Value::Count(0)),
        ),
    ))
    .unwrap();
    e.apply(Edit::AddAction(
        0,
        ActionDraft::set_price(
            GoodRef::This,
            met(Metric::CheapestCompetitorPrice {
                good: GoodRef::This,
                radius_m: 3_000,
                basis: PriceBasis::GrossRetail,
            })
            .scaled(Bp(9_800)),
        ),
    ))
    .unwrap();
    let z_kosztu = |bp: i32| {
        met(Metric::UnitCost(GoodRef::This))
            .scaled(Bp(bp))
            .converted(PriceBasis::GrossRetail)
    };
    e.apply(Edit::AddAction(
        0,
        ActionDraft::clamp(GoodRef::This, z_kosztu(10_500), z_kosztu(16_000)),
    ))
    .unwrap();
    e.apply(Edit::Fallback(Some(ActionDraft::margin(
        GoodRef::This,
        Bp(2_500),
    ))))
    .unwrap();
    e
}

/// „Zapas min-max" — dwa progi: dolny zamawia, górny krzyczy.
fn zapas_min_max() -> RuleEditor {
    let mut e = RuleEditor::new(
        PolicyId(2),
        "Zapas min-max",
        PolicyDomain::Stock,
        PolicyScope::Group(TagId(1)),
    );
    e.apply(Edit::Cooldown(12)).unwrap();
    e.apply(Edit::AddRule).unwrap();
    e.apply(Edit::AddClause(
        0,
        klauzula(
            met(Metric::StockDays(GoodRef::This)),
            CmpOp::Lt,
            lit(Value::Days(3)),
        ),
    ))
    .unwrap();
    e.apply(Edit::AddAction(
        0,
        ActionDraft::order_up_to(GoodRef::This, lit(Value::Days(10))),
    ))
    .unwrap();
    e.apply(Edit::AddRule).unwrap();
    e.apply(Edit::AddClause(
        1,
        klauzula(
            met(Metric::StockDays(GoodRef::This)),
            CmpOp::Gt,
            lit(Value::Days(21)),
        ),
    ))
    .unwrap();
    e.apply(Edit::AddClause(
        1,
        klauzula(
            met(Metric::Turnover7d(GoodRef::This)),
            CmpOp::Lt,
            lit(Value::Qty(5_000)),
        ),
    ))
    .unwrap();
    e.apply(Edit::AddAction(1, ActionDraft::alert(1, Severity::Warning)))
        .unwrap();
    e
}

/// „Nie przepłacaj za ropę" — jedyna z sześciu, która używa `zapytaj gracza`.
fn ropa() -> RuleEditor {
    let ropa = GoodRef::Id(GoodId(7));
    let mut e = RuleEditor::new(
        PolicyId(3),
        "Nie przepłacaj za ropę",
        PolicyDomain::Stock,
        PolicyScope::Site(SiteId(encja(11))),
    );
    e.apply(Edit::Cooldown(24)).unwrap();
    e.apply(Edit::AddRule).unwrap();
    e.apply(Edit::AddClause(
        0,
        klauzula(met(Metric::StockDays(ropa)), CmpOp::Lt, lit(Value::Days(5))),
    ))
    .unwrap();
    // Obie strony netto: rafineria kupuje hurtem, więc porównanie idzie netto
    // do netto i żadnej konwersji nie potrzebuje.
    e.apply(Edit::AddClause(
        0,
        klauzula(
            met(Metric::AvgCompetitorPrice {
                good: ropa,
                radius_m: 10_000,
                basis: PriceBasis::NetB2B,
            }),
            CmpOp::Gt,
            met(Metric::UnitCost(ropa)).scaled(Bp(13_000)),
        ),
    ))
    .unwrap();
    e.apply(Edit::AddAction(0, ActionDraft::ask(0))).unwrap();
    e.apply(Edit::AddRule).unwrap();
    e.apply(Edit::AddClause(
        1,
        klauzula(met(Metric::StockDays(ropa)), CmpOp::Lt, lit(Value::Days(5))),
    ))
    .unwrap();
    e.apply(Edit::AddAction(
        1,
        ActionDraft::order_up_to(ropa, lit(Value::Days(20))),
    ))
    .unwrap();
    e
}

#[test]
fn trzy_polityki_z_dokumentu_da_sie_zbudowac_i_przypiac() {
    for e in [dyskont(), zapas_min_max(), ropa()] {
        assert!(bez_bledow(&e), "{}: {:?}", e.name(), e.notes());
        // Zbudowana polityka przechodzi walidator języka, czyli ten sam kod, który
        // sprawdza presety z `data/policies/` przy ładowaniu.
        magnat_policy::validate(&e.policy()).unwrap_or_else(|d| panic!("{}: {d:?}", e.name()));
    }
}

#[test]
fn nabial_nie_wyrzucamy_konczy_sie_na_wycofaniu_z_polki() {
    // §5.6 pisze tę politykę jako jedną: trzy stopnie przeceny, ostatni wycofuje
    // towar z półki. Przecena jest **cenowa**, a wycofanie **zapasowe** — i to nie
    // jest szczegół zapisu, tylko dwaj różni wykonawcy. Edytor odmawia dołożenia
    // drugiej akcji i mówi dlaczego; gracz robi z tego dwie polityki.
    let mut e = RuleEditor::new(
        PolicyId(4),
        "Nabiał — nie wyrzucamy",
        PolicyDomain::Pricing,
        PolicyScope::Category {
            inner: Box::new(PolicyScope::Firm(FirmId(encja(5)))),
            cat: magnat_core::NeedCategoryId(3),
        },
    );
    e.apply(Edit::Cadence(Cadence::Hourly)).unwrap();
    e.apply(Edit::Cooldown(6)).unwrap();
    for (i, (prog, przecena)) in [(2, 4_000), (1, 7_000)].into_iter().enumerate() {
        e.apply(Edit::AddRule).unwrap();
        e.apply(Edit::AddClause(
            i,
            klauzula(
                met(Metric::DaysToExpiry(GoodRef::This)),
                CmpOp::Lt,
                lit(Value::Days(prog)),
            ),
        ))
        .unwrap();
        e.apply(Edit::AddAction(
            i,
            ActionDraft::markdown(GoodRef::This, Bp(przecena)),
        ))
        .unwrap();
    }
    e.apply(Edit::AddRule).unwrap();
    e.apply(Edit::AddClause(
        2,
        klauzula(
            met(Metric::DaysToExpiry(GoodRef::This)),
            CmpOp::Eq,
            lit(Value::Days(0)),
        ),
    ))
    .unwrap();
    // Alert wolno — nie zmienia świata, więc nie należy do żadnej dziedziny.
    e.apply(Edit::AddAction(
        2,
        ActionDraft::alert(2, Severity::Critical),
    ))
    .unwrap();
    // Wycofanie z półki — nie wolno.
    let wycofaj = ActionDraft {
        kind: magnat_core::ActionKind::RemoveFromShelf,
        good: GoodRef::This,
        ..ActionDraft::default()
    };
    assert_eq!(
        e.apply(Edit::AddAction(2, wycofaj)),
        Err(EditError::ActionOutOfDomain {
            expected: PolicyDomain::Pricing
        })
    );
    // Reszta polityki jest poprawna i da się ją przypiąć.
    assert!(bez_bledow(&e), "{:?}", e.notes());
}

#[test]
fn sezon_grzewczy_takze_rozpada_sie_na_dwie_dziedziny() {
    // „zamów do 60 dni" jest zapasowe, „przeceń o 15 %" cenowe. Ta sama korekta
    // co przy nabiale i z tego samego powodu.
    let wegiel = GoodRef::Id(GoodId(9));
    let mut e = RuleEditor::new(
        PolicyId(5),
        "Sezon grzewczy",
        PolicyDomain::Stock,
        PolicyScope::Product {
            inner: Box::new(PolicyScope::Firm(FirmId(encja(6)))),
            good: GoodId(9),
        },
    );
    e.apply(Edit::Cooldown(24)).unwrap();
    e.apply(Edit::AddRule).unwrap();
    e.apply(Edit::AddClause(
        0,
        klauzula(met(Metric::Season), CmpOp::Eq, lit(Value::Enum(3))),
    ))
    .unwrap();
    e.apply(Edit::AddClause(
        0,
        klauzula(
            met(Metric::StockDays(wegiel)),
            CmpOp::Lt,
            lit(Value::Days(30)),
        ),
    ))
    .unwrap();
    e.apply(Edit::AddAction(
        0,
        ActionDraft::order_up_to(wegiel, lit(Value::Days(60))),
    ))
    .unwrap();
    e.apply(Edit::AddRule).unwrap();
    e.apply(Edit::AddClause(
        1,
        klauzula(met(Metric::Season), CmpOp::Eq, lit(Value::Enum(1))),
    ))
    .unwrap();
    assert_eq!(
        e.apply(Edit::AddAction(1, ActionDraft::markdown(wegiel, Bp(1_500)))),
        Err(EditError::ActionOutOfDomain {
            expected: PolicyDomain::Stock
        })
    );
}

#[test]
fn zatrzymac_ludzi_buduje_sie_ale_nikt_jej_nie_wykona() {
    // Dziedzina kadrowa nie ma wykonawcy (`AX-5` w M7c): publikacja ofert i licytacja
    // płac dzieją się same od M7b, ale **polityki** kadrowej nie wykonuje nikt.
    // Edytor pozwala ją złożyć i mówi wprost, że jej nie przypnie — cicha polityka,
    // która wygląda na działającą, byłaby gorsza.
    let kierowca = magnat_core::JobRoleId(4);
    let mut e = RuleEditor::new(
        PolicyId(6),
        "Zatrzymać ludzi",
        PolicyDomain::Hr,
        PolicyScope::Firm(FirmId(encja(7))),
    );
    e.apply(Edit::Cooldown(24)).unwrap();
    e.apply(Edit::AddRule).unwrap();
    e.apply(Edit::AddClause(
        0,
        klauzula(
            met(Metric::StaffTurnover12m),
            CmpOp::Gt,
            lit(Value::Bp(Bp(3_000))),
        ),
    ))
    .unwrap();
    e.apply(Edit::AddClause(
        0,
        klauzula(met(Metric::StaffMood), CmpOp::Lt, lit(Value::Count(40))),
    ))
    .unwrap();
    let podwyzka = ActionDraft {
        kind: magnat_core::ActionKind::RaiseWage,
        role: kierowca,
        bp: Bp(500),
        b: met(Metric::MedianMarketWage(kierowca)).scaled(Bp(11_500)),
        has_b: true,
        ..ActionDraft::default()
    };
    e.apply(Edit::AddAction(0, podwyzka)).unwrap();
    let uwagi = e.notes();
    assert!(
        uwagi.iter().any(|n| matches!(
            n,
            Note::Language(Diagnostic::DomainNotAvailable {
                domain: PolicyDomain::Hr
            })
        )),
        "{uwagi:?}"
    );
}

#[test]
fn edytor_nie_pozwala_zmieszac_brutto_z_netto() {
    // `K-7` po stronie formularza: „ustaw cenę brutto = koszt netto" nie da się
    // złożyć, a nie „da się złożyć i walidator to potem odrzuci".
    let mut e = dyskont();
    let zle = klauzula(
        met(cena_brutto()),
        CmpOp::Gt,
        met(Metric::UnitCost(GoodRef::This)),
    );
    assert_eq!(
        e.apply(Edit::AddClause(0, zle)),
        Err(EditError::PriceBasisMismatch {
            lhs: PriceBasis::GrossRetail,
            rhs: PriceBasis::NetB2B
        })
    );
    // Ta sama para po jawnej konwersji przechodzi — bo o to chodzi w konwersji.
    let dobre = klauzula(
        met(cena_brutto()),
        CmpOp::Gt,
        met(Metric::UnitCost(GoodRef::This)).converted(PriceBasis::GrossRetail),
    );
    assert!(e.apply(Edit::AddClause(0, dobre)).is_ok());
}

#[test]
fn edytor_nie_pozwala_porownac_dni_ze_zlotowkami() {
    let mut e = zapas_min_max();
    let zle = klauzula(
        met(Metric::StockDays(GoodRef::This)),
        CmpOp::Lt,
        lit(Value::Money(Money(300))),
    );
    assert!(matches!(
        e.apply(Edit::AddClause(0, zle)),
        Err(EditError::UnitMismatch { .. })
    ));
}

#[test]
fn lista_slotu_nie_zawiera_pozycji_ktora_tworzy_blad() {
    // To jest mechanizm, na którym stoi całe zdanie „gracz nie może napisać błędu":
    // lista podpowiedzi jest **wynikiem** sprawdzenia zgodności, a nie kompletem
    // metryk z filtrem po stronie rysowania.
    let e = dyskont();
    let lewa = met(cena_brutto());
    for b in e.options_for(Some(&lewa)) {
        let s = Slot {
            base: b,
            scale: None,
            convert: None,
        };
        assert_eq!(s.unit(), lewa.unit(), "{b:?}");
        if let (Some(a), Some(c)) = (lewa.basis(), s.basis()) {
            assert_eq!(a, c, "{b:?}");
        }
    }
    // Koszt własny jest netto, więc w slocie po cenie brutto nie ma go wcale.
    assert!(!e
        .options_for(Some(&lewa))
        .contains(&Base::Metric(Metric::UnitCost(GoodRef::This))));
}

#[test]
fn polityka_wraca_z_tekstu_w_obu_jezykach() {
    let c = Catalog::load().expect("data/locale/");
    let g = magnat_game::policy::GoodKeys::new([(GoodId(7), "fuel_oil_crude".to_string())]);
    let e = ropa();
    for l in Locale::ALL {
        let txt = magnat_game::policy::write(&e.policy(), e.scope(), &g, &c, l);
        assert!(!txt.is_empty());
        let (p, s) = magnat_game::policy::parse(&txt, &g, &c)
            .unwrap_or_else(|err| panic!("{l:?}: {err}\n{txt}"));
        assert_eq!(p.rules, e.policy().rules, "{l:?}\n{txt}");
        assert_eq!(&s, e.scope());
    }
}

#[test]
fn edytor_otwiera_polityke_zapisana_i_oddaje_ja_bez_zmiany() {
    // Droga powrotna: preset z `data/policies/` wchodzi do formularza i wychodzi
    // z niego identyczny. Bez tego „otwórz politykę firmy AI i zobacz, co robi"
    // byłoby obietnicą bez pokrycia.
    let kat = magnat_policy::PolicyCatalog::load_default().expect("data/policies");
    for pr in kat.iter() {
        let e = RuleEditor::from_policy(&pr.policy, PolicyScope::Group(TagId(0)))
            .unwrap_or_else(|| panic!("preset {} nie mieści się w formularzu", pr.key));
        assert_eq!(e.policy(), pr.policy, "preset {}", pr.key);
    }
}

#[test]
fn ekran_edytora_rysuje_sie_w_obu_jezykach_bez_gpu() {
    use magnat_game::policy::{EditorAction, RuleEditorView, Tab};
    let theme = magnat_ui::Theme::load().expect("data/ui/theme.ron");
    let g = magnat_game::policy::GoodKeys::new([(GoodId(7), "fuel_oil_crude".to_string())]);
    for l in Locale::ALL {
        let c = Catalog::load().expect("data/locale/");
        for zakladka in [Tab::Rules, Tab::Notes, Tab::DryRun, Tab::Text] {
            let mut v = RuleEditorView::new(ropa());
            v.tab = zakladka;
            let mut akcja = EditorAction::None;
            let teksty = magnat_ui::testing::draw(|ui| {
                akcja = v.draw(ui, &theme, &c, l, &g);
            });
            assert_eq!(akcja, EditorAction::None);
            assert!(
                teksty.iter().any(|t| t.contains("Nie przepłacaj")),
                "{l:?} {zakladka:?}: {teksty:?}"
            );
            // Żaden napis nie jest kluczem lokalizacji — brak wpisu w `data/locale/`
            // objawiłby się właśnie tak i przeszedłby niezauważony.
            assert!(
                !teksty.iter().any(|t| t.starts_with("ui.policy.")),
                "{l:?} {zakladka:?}: {teksty:?}"
            );
        }
    }
}

#[test]
fn edytor_z_bledem_nie_daje_sie_przypiac() {
    use magnat_game::policy::RuleEditorView;
    // Polityka kadrowa buduje się, ale nikt jej nie wykona — ekran ma to powiedzieć
    // **przed** kliknięciem, a nie odrzucić komendę po nim.
    let mut e = RuleEditor::new(
        PolicyId(8),
        "Kadry",
        PolicyDomain::Hr,
        PolicyScope::Firm(FirmId(encja(3))),
    );
    e.apply(Edit::AddRule).unwrap();
    let v = RuleEditorView::new(e);
    assert!(!v.can_attach());
    assert!(RuleEditorView::new(dyskont()).can_attach());
}

#[test]
fn kazda_uwaga_walidatora_ma_zdanie_w_obu_jezykach() {
    use magnat_game::policy::view::note_text;
    use magnat_policy::NodePath;
    use magnat_policy::Unit;
    let c = Catalog::load().expect("data/locale/");
    let wszystkie = [
        Note::BelowCost { rule: 0 },
        Note::Language(Diagnostic::UnitMismatch {
            at: NodePath::rule(0),
            expected: Unit::Money,
            got: Unit::Days,
        }),
        Note::Language(Diagnostic::PriceBasisMismatch {
            at: NodePath::rule(0),
            lhs: PriceBasis::GrossRetail,
            rhs: PriceBasis::NetB2B,
        }),
        Note::Language(Diagnostic::ActionOutOfDomain {
            at: NodePath::rule(0),
            expected: PolicyDomain::Pricing,
            got: PolicyDomain::Stock,
        }),
        Note::Language(Diagnostic::DomainNotAvailable {
            domain: PolicyDomain::Hr,
        }),
        Note::Language(Diagnostic::TooDeep {
            at: NodePath::rule(0),
            depth: 4,
        }),
        Note::Language(Diagnostic::BudgetExceeded {
            what: "rules",
            limit: 8,
        }),
        Note::Language(Diagnostic::UnreachableRule {
            rule: 1,
            shadowed_by: 0,
        }),
        Note::Language(Diagnostic::PossibleOscillation { rule: 0 }),
    ];
    for l in Locale::ALL {
        for n in &wszystkie {
            let t = note_text(n, &c, l);
            assert!(
                !t.is_empty() && !t.starts_with("ui.policy."),
                "{l:?} {n:?}: {t}"
            );
        }
    }
}

#[test]
fn caly_wjazd_edytora_ma_skutek() {
    use magnat_game::policy::Join;
    use magnat_policy::Cadence;
    // Każdy wariant `Edit` jest kliknięciem, które gracz może wykonać — więc każdy
    // musi mieć skutek i musi być sprawdzony. Wariant bez testu wygląda tak samo
    // jak działający (`K-67`), a połowa wjazdu edytora bez pokrycia znaczy, że
    // formularz da się zepsuć w miejscu, którego nikt nie dotyka.
    let mut e = dyskont();
    e.apply(Edit::Name("Nowa nazwa".into())).unwrap();
    assert_eq!(e.name(), "Nowa nazwa");

    e.apply(Edit::Scope(PolicyScope::Site(SiteId(encja(42)))))
        .unwrap();
    assert_eq!(e.scope(), &PolicyScope::Site(SiteId(encja(42))));

    e.apply(Edit::Cadence(Cadence::Hourly)).unwrap();
    e.apply(Edit::Cooldown(6)).unwrap();
    assert_eq!(e.policy().cadence, Cadence::Hourly);
    assert_eq!(e.policy().cooldown_h, 6);

    e.apply(Edit::Note(0, "notatka".into())).unwrap();
    assert_eq!(e.rules()[0].note, "notatka");
    e.apply(Edit::Enable(0, false)).unwrap();
    assert!(!e.rules()[0].enabled);
    e.apply(Edit::Join(0, Join::Or)).unwrap();
    assert_eq!(e.rules()[0].join, Join::Or);

    // Druga reguła, przestawienie kolejności, kasowanie warunku i akcji.
    e.apply(Edit::AddRule).unwrap();
    e.apply(Edit::AddClause(
        1,
        klauzula(
            met(Metric::StockDays(GoodRef::This)),
            CmpOp::Gt,
            lit(Value::Days(30)),
        ),
    ))
    .unwrap();
    e.apply(Edit::SetClause(
        1,
        0,
        klauzula(
            met(Metric::StockDays(GoodRef::This)),
            CmpOp::Gt,
            lit(Value::Days(45)),
        ),
    ))
    .unwrap();
    assert_eq!(e.rules()[1].clauses[0].rhs, lit(Value::Days(45)));
    e.apply(Edit::AddAction(
        1,
        ActionDraft::markdown(GoodRef::This, Bp(1_000)),
    ))
    .unwrap();
    e.apply(Edit::SetAction(
        1,
        0,
        ActionDraft::markdown(GoodRef::This, Bp(2_000)),
    ))
    .unwrap();
    assert_eq!(e.rules()[1].actions[0].bp, Bp(2_000));
    e.apply(Edit::MoveRule { from: 1, to: 0 }).unwrap();
    assert_eq!(e.rules()[0].clauses.len(), 1);
    e.apply(Edit::DelAction(0, 0)).unwrap();
    assert!(e.rules()[0].actions.is_empty());
    e.apply(Edit::DelClause(0, 0)).unwrap();
    assert!(e.rules()[0].clauses.is_empty());
    e.apply(Edit::DelRule(0)).unwrap();
    assert_eq!(e.rules().len(), 1);

    e.apply(Edit::Fallback(None)).unwrap();
    assert!(e.fallback().is_none());
    e.apply(Edit::Domain(PolicyDomain::Stock)).unwrap();
    assert_eq!(e.domain(), PolicyDomain::Stock);

    // Odmowa nie zostawia po sobie połowy zmiany.
    let przed = e.rules().len();
    assert!(e.apply(Edit::DelRule(9)).is_err());
    assert_eq!(e.rules().len(), przed);
}

#[test]
fn limit_osmiu_regul_jest_twardy() {
    let mut e = RuleEditor::new(
        PolicyId(7),
        "Za dużo",
        PolicyDomain::Stock,
        PolicyScope::Site(SiteId(encja(1))),
    );
    for _ in 0..magnat_policy::MAX_RULES {
        e.apply(Edit::AddRule).unwrap();
    }
    assert_eq!(e.apply(Edit::AddRule), Err(EditError::TooManyRules));
}
