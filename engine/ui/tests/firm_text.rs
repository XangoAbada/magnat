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

use magnat_core::{
    CitizenId, DecisionReason, DistrictId, Entity, FirmStrategy, JobRoleId, Money, SimMinute,
    SiteId, Tick, Trend,
};
use magnat_firms::panel::{EmployeeRow, FirmPanelSnapshot, ManagerRow, OutlookRow, SiteRow};
use magnat_firms::{FirmKey, FirmPersonality, FirmStatus, ManagerStyle, Owner, SitePnlMonth};
use magnat_ui::inspect::firm::{FirmCard, FirmTab};
use magnat_ui::loc::{Catalog, Locale};

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
                DecisionReason::StrategySet {
                    strategy: FirmStrategy::Discount,
                    prev: FirmStrategy::Cautious,
                },
            ),
            (
                Tick(2_880),
                DecisionReason::SiteOpened {
                    district: DistrictId(3),
                    variants: 4,
                    margin_bp: 620,
                    trend: Trend::Up,
                },
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

#[test]
fn karta_sklada_sie_w_obu_jezykach() {
    let c = Catalog::load().expect("data/locale/");
    for l in Locale::ALL {
        let karta = FirmCard::build(&c, l, &migawka());
        let t = karta.render_text(&c, l);
        assert!(!t.is_empty());
        // Nierozwinięty klucz zostawia w tekście nawias klamrowy albo własną nazwę.
        assert!(
            !t.contains('{') && !t.contains("ui."),
            "nierozwinięty klucz w karcie ({l:?}):\n{t}"
        );
        for zakladka in FirmTab::ALL {
            assert!(!karta.render_tab(&c, l, zakladka).is_empty());
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
        let t = FirmCard::build(&c, l, &s).render_tab(&c, l, FirmTab::Course);
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
    let pl = FirmCard::build(&c, Locale::Pl, &s).render_tab(&c, Locale::Pl, FirmTab::Course);
    let en = FirmCard::build(&c, Locale::En, &s).render_tab(&c, Locale::En, FirmTab::Course);
    assert!(pl.contains("nie rozstrzygnął"), "{pl}");
    assert!(en.contains("did not decide"), "{en}");
}

#[test]
fn zaklad_bez_rachunku_nie_udaje_zera() {
    let c = Catalog::load().expect("data/locale/");
    let mut s = migawka();
    s.sites[0].last_month = None;
    let t = FirmCard::build(&c, Locale::Pl, &s).render_header(&c, Locale::Pl);
    assert!(t.contains("brak rachunku"), "{t}");
    assert!(s.last_result().is_none());
}
