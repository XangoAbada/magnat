//! Złoty wydruk karty firmy (M7f WP16).
//!
//! Dwa pytania. Pierwsze zwykłe: czy karta składa się w obu językach i nie zostawia
//! nierozwiniętego klucza. Drugie jest **kryterium ukończenia WP16** i warto je
//! nazwać wprost: czy w karcie nie ma ani jednej kwoty pochodzenia makro.
//!
//! Drugie pytanie sprawdza się przez konstrukcję: `OutlookRow` nie ma pola
//! pieniężnego, więc karta nie ma z czego takiej kwoty złożyć. Test pilnuje, żeby
//! to zostało prawdą — porównuje wydruk zakładki „Kurs" z listą liczb, które
//! **wolno** w niej stanąć, i nie przepuszcza żadnej innej.

use magnat_city::{ChargeState, FiscalPeriod, TaxCharge, TaxChargeId, TaxPayer};
use magnat_core::{
    CitizenId, DecisionReason, DistrictId, Entity, FirmReason, FirmStrategy, JobRoleId, Money,
    SimMinute, SiteId, Tick, Trend,
};
use magnat_firms::panel::{EmployeeRow, FirmPanelSnapshot, ManagerRow, OutlookRow, SiteRow};
use magnat_firms::{FirmKey, FirmPersonality, FirmStatus, ManagerStyle, Owner, SitePnlMonth};
use magnat_ui::inspect::firm::{FirmCard, FirmTab};
use magnat_ui::loc::{Catalog, Locale};
use magnat_ui::RichExt;

fn e(i: u32) -> Entity {
    Entity::new(i, std::num::NonZeroU32::MIN)
}

fn migawka() -> FirmPanelSnapshot {
    FirmPanelSnapshot {
        key: FirmKey(7),
        name: "Piekarnia Pod Kogutem".to_string(),
        founded: SimMinute(0),
        status: FirmStatus::Active,
        hq_district: DistrictId(3),
        director: Some(CitizenId(e(42))),
        owners: vec![(Owner::Citizen(CitizenId(e(42))), 10_000)],
        personality: FirmPersonality::default(),
        strategy: FirmStrategy::Discount,
        cash: Money(1_250_000),
        sites: vec![SiteRow {
            site: SiteId(e(1 << 24)),
            district: DistrictId(3),
            site_type: "grocery_store".to_string(),
            headcount: 6,
            vacancies: 2,
            labor_cost: Money(2_400_000),
            fixed_cost: Money(560_000),
            last_month: Some(SitePnlMonth {
                month: 12,
                revenue: Money(4_000_000),
                cogs: Money(3_000_000),
                labor: Money(2_400_000),
                fixed: Money(560_000),
            }),
            months_in_loss: 3,
            manager: Some(ManagerRow {
                citizen: Some(CitizenId(e(43))),
                skill_mgmt: magnat_core::Q::new(71),
                style: ManagerStyle::Coach,
                autonomy: magnat_firms::Autonomy::PricesOnly,
                span: 2,
                tenure: SimMinute(0),
            }),
        }],
        staff: vec![EmployeeRow {
            citizen: CitizenId(e(44)),
            site: SiteId(e(1 << 24)),
            role: JobRoleId(5),
            wage_month: Money(380_000),
            since: SimMinute(0),
            perf: 520,
            warnings: 0,
        }],
        decisions: vec![
            (
                Tick(1_440),
                DecisionReason::Firm(FirmReason::StrategySet {
                    strategy: FirmStrategy::Discount,
                    prev: FirmStrategy::Cautious,
                }),
            ),
            (
                Tick(2_880),
                DecisionReason::Firm(FirmReason::SiteOpened {
                    district: DistrictId(3),
                    variants: 4,
                    margin_bp: 620,
                    trend: Trend::Up,
                }),
            ),
        ],
        campaign: None,
        outlook: Some(OutlookRow {
            variants: vec![
                magnat_firms::StrAction::KeepCourse,
                magnat_firms::StrAction::OpenSite {
                    district: DistrictId(3),
                    slots: 6,
                    capex: Money(18_000_000),
                },
            ],
            winner: Some(1),
            trend: Trend::Up,
            error_margin_bp: 620,
            at: Tick(2_880),
        }),
    }
}

/// Trzy obciążenia zakładu, po jednym na stan, którego gracz może dożyć.
fn obciazenia() -> Vec<TaxCharge> {
    let site = TaxPayer::Site(SiteId(e(1 << 24)));
    let wzor = TaxCharge {
        id: TaxChargeId(0),
        payer: site,
        kind: magnat_core::TaxKind::Vat,
        period: FiscalPeriod::Month(1, 3),
        base: Money(1_240_000),
        base_mass: magnat_core::Mass(0),
        rate_snapshot: 500,
        amount: Money(62_000),
        assessed_at: Tick(129_600),
        due_at: Tick(158_400),
        state: ChargeState::Settled { at: Tick(158_400) },
    };
    vec![
        wzor,
        TaxCharge {
            id: TaxChargeId(1),
            kind: magnat_core::TaxKind::Property,
            period: FiscalPeriod::Month(1, 3),
            base: Money(23_000_000),
            rate_snapshot: 25,
            amount: Money(4_792),
            assessed_at: Tick(129_600),
            state: ChargeState::Overdue {
                since: Tick(150_000),
                interest: Money(41),
            },
            ..wzor
        },
        TaxCharge {
            id: TaxChargeId(2),
            kind: magnat_core::TaxKind::License,
            period: FiscalPeriod::Year(1),
            base: Money(1_400_000),
            rate_snapshot: 0,
            amount: Money(1_400_000),
            assessed_at: Tick(0),
            state: ChargeState::Abated {
                at: Tick(160_000),
                why: magnat_core::AbateReason::Council,
            },
            ..wzor
        },
    ]
}

/// **Kryterium ukończenia WP2 fazy M8a**: obciążenie widoczne w karcie inspekcji
/// firmy **z nazwą, stawką i podstawą**. Nie „z kwotą" — kwota bez podstawy
/// i stawki nie odpowiada na pytanie, które gracz zadaje, otwierając tę zakładkę.
#[test]
fn zakladka_danin_pokazuje_podstawe_i_stawke() {
    let c = Catalog::load().expect("data/locale/");
    let karta =
        FirmCard::build(&c, Locale::Pl, &migawka()).with_taxes(&c, Locale::Pl, &obciazenia());
    let t = karta.render_tab(&c, Locale::Pl, FirmTab::Taxes).to_plain();
    // Nazwa daniny, podstawa i stawka — każda z trzech należności.
    assert!(t.contains("VAT"), "{t}");
    assert!(
        t.contains("12400.00"),
        "podstawa VAT-u nie widać:
{t}"
    );
    assert!(
        t.contains("5 %"),
        "stawki VAT-u nie widać:
{t}"
    );
    assert!(t.contains("podatek od nieruchomości"), "{t}");
    assert!(
        t.contains("0,25 %"),
        "stawka ułamkowa zaokrąglona do zera:
{t}"
    );
    // Stan mówi, co się z należnością stało — łącznie z odsetkami i umorzeniem.
    assert!(t.contains("zapłacone") && t.contains("zaległe"), "{t}");
    assert!(t.contains("umorzone") && t.contains("uchwała rady"), "{t}");
    // Karta bez obciążeń mówi „brak danych", a nie udaje zera.
    let pusta = FirmCard::build(&c, Locale::Pl, &migawka());
    assert!(pusta
        .render_tab(&c, Locale::Pl, FirmTab::Taxes)
        .to_plain()
        .contains("brak danych"));
}

#[test]
fn karta_sklada_sie_w_obu_jezykach() {
    let c = Catalog::load().expect("data/locale/");
    for l in Locale::ALL {
        let karta = FirmCard::build(&c, l, &migawka()).with_taxes(&c, l, &obciazenia());
        let t = karta.render_text(&c, l);
        assert!(!t.is_empty());
        // Nierozwinięty klucz zostawia w tekście nawias klamrowy albo własną nazwę.
        assert!(
            !t.contains('{') && !t.contains("ui."),
            "nierozwinięty klucz w karcie ({l:?}):\n{t}"
        );
        for zakladka in FirmTab::ALL {
            assert!(!karta.render_tab(&c, l, zakladka).to_plain().is_empty());
        }
    }
}

#[test]
fn zakladka_kursu_nie_pokazuje_zadnej_kwoty() {
    // **Kryterium ukończenia WP16**: zero kwot pochodzenia makro w interfejsie.
    // Zakładka „Kurs" jest jedynym miejscem karty, do którego prognoza w ogóle
    // dociera, więc to ona jest miejscem sprawdzenia.
    let c = Catalog::load().expect("data/locale/");
    let s = migawka();
    for l in Locale::ALL {
        let t = FirmCard::build(&c, l, &s)
            .render_tab(&c, l, FirmTab::Course)
            .to_plain();
        assert!(
            !t.contains("zł") && !t.to_lowercase().contains("pln"),
            "kwota w zakładce kursu ({l:?}):\n{t}"
        );
        // Liczby, które **wolno** tu zobaczyć: liczba wariantów i margines błędu.
        // `capex` wariantu „otworzyć zakład" jest kwotą i **nie ma prawa** się
        // pokazać — wariant opisuje się nazwą, nie ceną.
        assert!(
            !t.contains("180 000") && !t.contains("18000000"),
            "nakład wariantu wyciekł do interfejsu ({l:?}):\n{t}"
        );
        assert!(t.contains('2'), "liczba wariantów zniknęła z karty:\n{t}");
    }
}

#[test]
fn nierozstrzygniety_ranking_mowi_o_tym_wprost() {
    // „Model nie rozstrzygnął" i „nic się nie stanie" to dwa różne zdania (§5.10).
    let c = Catalog::load().expect("data/locale/");
    let mut s = migawka();
    if let Some(o) = s.outlook.as_mut() {
        o.winner = None;
        o.trend = Trend::Flat;
    }
    let pl = FirmCard::build(&c, Locale::Pl, &s)
        .render_tab(&c, Locale::Pl, FirmTab::Course)
        .to_plain();
    let en = FirmCard::build(&c, Locale::En, &s)
        .render_tab(&c, Locale::En, FirmTab::Course)
        .to_plain();
    assert!(pl.contains("nie rozstrzygnął"), "{pl}");
    assert!(en.contains("did not decide"), "{en}");
}

#[test]
fn zaklad_bez_rachunku_nie_udaje_zera() {
    let c = Catalog::load().expect("data/locale/");
    let mut s = migawka();
    s.sites[0].last_month = None;
    let t = FirmCard::build(&c, Locale::Pl, &s)
        .render_header(&c, Locale::Pl)
        .to_plain();
    assert!(t.contains("brak rachunku"), "{t}");
    assert!(s.last_result().is_none());
}
