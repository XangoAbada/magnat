//! Tier operacyjny — doba firmy (M7e WP11, M7 §5.6, PRD §12.3).
//!
//! Dwie dźwignie: **cel marży** i **cel zapasu**. Nie cena i nie zamówienie — te
//! powstają codziennie w `sim/economy` ze sterownika i z reguły zapasu, a firma
//! przestawia im cel. Różnica jest istotna i jest tą samą, którą M5c zapisał przy
//! `PricePolicy`: jedno miejsce składa cenę, reszta świata mówi mu, wokół czego.
//!
//! # Dlaczego kotwica, a nie krok
//!
//! Naiwna wersja („widzę tańszego → obniż o 1,5%") całkuje: po dwudziestu dobach
//! wojny cenowej marża jest pod podłogą, a po dwudziestu dobach spokoju pod sufitem,
//! i firma nie ma dokąd wrócić. Tutaj cel liczy się **od kotwicy kursu** i korekt
//! bieżących, więc ustanie bodźca przywraca kotwicę samo — a rozpiętość osobowości
//! widać w tym, gdzie kotwica leży i jak duże są korekty.
//!
//! Zakład **zdelegowany** menedżerowi jest pomijany: tam cenę prowadzi polityka
//! (M7c WP7), a dwóch sterowników na jedną półkę to dwie prawdy o tej samej cenie.

use magnat_core::{DecisionReason, FirmStrategy, GoodId, SiteId};
use magnat_policy::Decided;
use smallvec::SmallVec;

use crate::view::{FirmView, GoodFacts};

/// Krok korekty marży w punktach bazowych przy agresji neutralnej.
const BASE_STEP_BP: i32 = 150;

/// Poniżej tej zmiany firma nie rusza celu. Bez progu tier operacyjny produkowałby
/// wpis w dzienniku każdej firmy każdej doby i karta inspekcji przestałaby cokolwiek
/// znaczyć — trzydzieści dwie ostatnie decyzje to byłoby trzydzieści dwa razy
/// „przesunąłem marżę o 3 punkty bazowe".
const MIN_MOVE_BP: i32 = 100;

/// Poniżej tej zmiany firma nie rusza celu zapasu (w dobach).
const MIN_MOVE_DAYS: i32 = 2;

/// Co tier operacyjny może zrobić w ciągu doby (M7 §5.9).
///
/// Lista jest krótsza od tej w planie fazy i to jest świadome: publikacja i eskalacja
/// ofert pracy dzieją się **same** od M7b (`labor::matching`, `labor::bidding`),
/// premie i świadczenia rozlicza `labor::hr` raz w miesiącu, a grafik zmian należy
/// do `supply::PlantSite::schedule` z M6b. Dokładanie im drugiego, równoległego
/// sterowania nie dałoby firmie ani jednej nowej decyzji — dałoby dwa mechanizmy
/// na jedną rzecz i rozjazd przy pierwszej zmianie któregoś.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OpsAction {
    /// Nowy cel marży sterownika ceny, w punktach bazowych kosztu.
    SetMarginTarget { site: SiteId, good: GoodId, bp: i32 },
    /// Nowy cel pokrycia zapasu, w dobach sprzedaży.
    SetRestockDays {
        site: SiteId,
        good: GoodId,
        days: u16,
    },
}

/// Doba firmy: przegląd celów cenowych i zapasowych na wszystkich własnych półkach.
///
/// Zwraca co najwyżej osiem decyzji — tyle, ile mieści `SmallVec` bez alokacji.
/// Firma z dwudziestoma towarami przestawi dziś osiem, a resztę jutro; sufit jest
/// tu tą samą odpowiedzią co kolejka przepełnienia w schedulerze i z tego samego
/// powodu: budżet ma być liczony, a nie mierzony zegarem (00 §3.5).
#[must_use]
pub fn decide_operational(v: &FirmView) -> SmallVec<[Decided<OpsAction>; 8]> {
    let mut out: SmallVec<[Decided<OpsAction>; 8]> = SmallVec::new();
    for g in v.goods {
        if out.len() >= 8 {
            break;
        }
        if v.site(g.site).is_some_and(|s| s.delegated) {
            continue;
        }
        if let Some(d) = marza(v, g) {
            out.push(d);
        }
        if out.len() >= 8 {
            break;
        }
        if let Some(d) = zapas(v, g) {
            out.push(d);
        }
    }
    out
}

/// Kotwica marży wynikająca z kursu firmy — ułamek widełek jej osobowości cenowej.
///
/// Dyskont siedzi przy dolnej krawędzi, nisza jakościowa przy górnej, ekspansja
/// niżej niż środek (bo kupuje udział), reszta w środku. Widełki są **cudze**
/// (`FirmPricing` z M5c) i tu się ich nie podważa — kurs mówi, gdzie w nich stanąć.
fn kotwica(v: &FirmView) -> i32 {
    let dol = v.margin_floor_bp;
    let rozpietosc = (v.margin_ceiling_bp - dol).max(0);
    let udzial_pct = match v.strategy {
        FirmStrategy::Discount => 15,
        FirmStrategy::AggressiveExpansion => 35,
        FirmStrategy::Cautious | FirmStrategy::Consolidator => 50,
        FirmStrategy::Innovative => 60,
        FirmStrategy::NicheQuality => 85,
    };
    dol + rozpietosc * udzial_pct / 100
}

/// Ile firma dokłada albo ujmuje w jednym ruchu. Agresja mnoży, nie zastępuje —
/// ta sama konwencja co w licytacji płac (`wage_escalation_step`).
fn krok(v: &FirmView) -> i32 {
    BASE_STEP_BP * (100 + i32::from(v.personality.aggression)) / 100
}

/// O ile firma pozwala sobie być droższa od najtańszego rywala, zanim zareaguje.
///
/// Nacisk na cenę zwęża tolerancję: firma tania reaguje na przewagę 1%, firma
/// jakościowa dopiero na 5%. To jest miejsce, w którym dwa sklepy w tej samej ulicy
/// zaczynają zachowywać się inaczej, mając ten sam koszt.
fn tolerancja_bp(v: &FirmView) -> i32 {
    (600 - i32::from(v.personality.price_focus) * 5).clamp(100, 600)
}

fn marza(v: &FirmView, g: &GoodFacts) -> Option<Decided<OpsAction>> {
    let k = krok(v);
    let mut cel = kotwica(v);

    // (1) Rywale. Porównujemy **cenę do ceny**, nigdy marżę do marży — marży
    //     konkurenta firma nie zna i znać nie może (§5.8).
    if let Some(rywal) = g.rival_cheapest_net {
        let ich = rywal.get().max(1);
        let nasza = g.own_price_net.get();
        let roznica_bp =
            ((nasza - ich).saturating_mul(10_000) / ich).clamp(-100_000, 100_000) as i32;
        if roznica_bp > tolerancja_bp(v) {
            cel -= k;
        } else if roznica_bp < -tolerancja_bp(v) && g.stock_days >= 0 && g.stock_days <= 3 {
            // Jesteśmy wyraźnie najtańsi i schodzi nam z półki — to jest zaproszenie
            // do podniesienia ceny, a nie powód do dumy.
            cel += k;
        }
    }

    // (2) Zapas. Leży — taniej; znika — drożej. Próg jest wyrażony w **celu
    //     zamówienia**, a nie w stałej liczbie dób: zakład zamawiający na dziesięć
    //     dni i zakład zamawiający na trzy mają inny sens słowa „dużo".
    let cel_zapasu = i32::from(g.restock_days.max(1));
    if g.stock_days < 0 || g.stock_days > cel_zapasu * 2 {
        cel -= k;
    } else if g.stock_days * 3 < cel_zapasu {
        cel += k;
    }

    let nowy = cel.clamp(v.margin_floor_bp, v.margin_ceiling_bp);
    if (nowy - g.margin_bp).abs() < MIN_MOVE_BP {
        return None;
    }
    Some(Decided::new(
        OpsAction::SetMarginTarget {
            site: g.site,
            good: g.good,
            bp: nowy,
        },
        DecisionReason::MarginTargetSet {
            good: g.good,
            margin_bp: nowy,
            prev_bp: g.margin_bp,
        },
    ))
}

fn zapas(v: &FirmView, g: &GoodFacts) -> Option<Decided<OpsAction>> {
    // Kotwica: cierpliwy trzyma więcej, ryzykant mniej. Dwie cechy, dwa kierunki,
    // żadnego całkowania — po ustaniu bodźca cel wraca sam.
    let mut cel =
        7 + i32::from(v.personality.patience) / 25 - i32::from(v.personality.risk_tolerance) / 50;
    if g.stock_days >= 0 && g.stock_days <= 1 {
        // Półka świeci pustkami mimo dzisiejszego celu — cel jest za niski.
        cel += 3;
    } else if g.stock_days > 21 {
        cel -= 3;
    }
    let cel = cel.clamp(3, 21);
    if (cel - i32::from(g.restock_days)).abs() < MIN_MOVE_DAYS {
        return None;
    }
    let days = cel as u16;
    Some(Decided::new(
        OpsAction::SetRestockDays {
            site: g.site,
            good: g.good,
            days,
        },
        DecisionReason::RestockTargetSet {
            good: g.good,
            days,
            prev: g.restock_days,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::personality::FirmPersonality;
    use crate::view::{CityFacts, SiteFacts};
    use crate::FirmKey;
    use magnat_core::{DistrictId, Entity, Money, Tick};
    use std::num::NonZeroU32;

    fn zaklad() -> SiteId {
        SiteId(Entity::new(1, NonZeroU32::MIN))
    }

    fn fakty_towaru() -> GoodFacts {
        GoodFacts {
            site: zaklad(),
            good: GoodId(3),
            own_price_net: Money(500),
            unit_cost_net: Money(400),
            margin_bp: 2_500,
            stock_days: 7,
            restock_days: 7,
            rival_cheapest_net: None,
            rival_cheapest_site: None,
            rival_median_net: None,
            rivals: 0,
            rivals_before: 0,
            sold_7d: 100,
            sold_7d_prev: 100,
        }
    }

    fn fakty_zakladu() -> SiteFacts {
        SiteFacts {
            site: zaklad(),
            district: DistrictId(0),
            vacancies: 0,
            headcount: 4,
            months_in_loss: 0,
            last_margin_bp: Some(1_000),
            delegated: false,
            needs_policy: false,
            scarcest_role: None,
        }
    }

    fn widok<'a>(
        p: FirmPersonality,
        sites: &'a [SiteFacts],
        goods: &'a [GoodFacts],
    ) -> FirmView<'a> {
        FirmView {
            key: FirmKey(1),
            cash: Money(1_000_000),
            personality: p,
            strategy: p.strategy(),
            margin_floor_bp: 1_000,
            margin_ceiling_bp: 4_000,
            lag_days: 3,
            tick: Tick(0),
            sites,
            goods,
            city: CityFacts::default(),
            outlook: None,
        }
    }

    #[test]
    fn kurs_firmy_decyduje_gdzie_w_widelkach_stanie_cel() {
        let mut tani = FirmPersonality::NEUTRAL;
        tani.price_focus = 90;
        tani.quality_focus = 20;
        let mut niszowy = FirmPersonality::NEUTRAL;
        niszowy.quality_focus = 90;
        niszowy.price_focus = 20;

        let s = [fakty_zakladu()];
        let g = [fakty_towaru()];
        let a = widok(tani, &s, &g);
        let b = widok(niszowy, &s, &g);
        assert_eq!(a.strategy, FirmStrategy::Discount);
        assert_eq!(b.strategy, FirmStrategy::NicheQuality);
        assert!(kotwica(&a) < kotwica(&b), "dyskont nie stoi niżej od niszy");
    }

    #[test]
    fn drozszy_od_rywala_schodzi_z_marza() {
        let s = [fakty_zakladu()];
        let mut f = fakty_towaru();
        f.rival_cheapest_net = Some(Money(400)); // my 500, czyli +25%
        let g = [f];
        let v = widok(FirmPersonality::NEUTRAL, &s, &g);
        let d = decide_operational(&v);
        let OpsAction::SetMarginTarget { bp, .. } = *d
            .iter()
            .map(magnat_policy::Decided::action)
            .find(|a| matches!(a, OpsAction::SetMarginTarget { .. }))
            .expect("marża ruszona")
        else {
            unreachable!()
        };
        assert!(bp < kotwica(&v), "cel nie zszedł poniżej kotwicy: {bp}");
    }

    #[test]
    fn brak_bodzca_wraca_do_kotwicy_zamiast_dryfowac() {
        // Całkujący tier zsuwałby cel coraz niżej przy powtarzanym przeglądzie.
        // Ten ma wracać: dwa przebiegi z tym samym wejściem dają ten sam cel.
        let s = [fakty_zakladu()];
        let mut f = fakty_towaru();
        f.margin_bp = 4_000;
        let g = [f];
        let v = widok(FirmPersonality::NEUTRAL, &s, &g);
        let pierwszy = decide_operational(&v);
        let OpsAction::SetMarginTarget { bp, .. } = *pierwszy[0].action() else {
            unreachable!()
        };
        assert_eq!(bp, kotwica(&v));

        // Po wykonaniu cel jest na kotwicy — drugi przegląd nie ma już co ruszać.
        let mut f2 = fakty_towaru();
        f2.margin_bp = bp;
        let g2 = [f2];
        let v2 = widok(FirmPersonality::NEUTRAL, &s, &g2);
        assert!(
            !v2.goods.is_empty()
                && decide_operational(&v2)
                    .iter()
                    .all(|d| !matches!(d.action(), OpsAction::SetMarginTarget { .. })),
            "cel dryfuje mimo braku bodźca"
        );
    }

    #[test]
    fn zaklad_zdelegowany_nie_dostaje_drugiego_sterownika() {
        let mut s0 = fakty_zakladu();
        s0.delegated = true;
        let s = [s0];
        let mut f = fakty_towaru();
        f.margin_bp = 4_000;
        let g = [f];
        let v = widok(FirmPersonality::NEUTRAL, &s, &g);
        assert!(decide_operational(&v).is_empty());
    }

    #[test]
    fn pusta_polka_podnosi_cel_zapasu() {
        let s = [fakty_zakladu()];
        let mut f = fakty_towaru();
        f.stock_days = 0;
        f.restock_days = 7;
        let g = [f];
        let v = widok(FirmPersonality::NEUTRAL, &s, &g);
        let d = decide_operational(&v);
        let dni = d
            .iter()
            .find_map(|d| match *d.action() {
                OpsAction::SetRestockDays { days, .. } => Some(days),
                OpsAction::SetMarginTarget { .. } => None,
            })
            .expect("cel zapasu ruszony");
        assert!(dni > 7, "cel zapasu {dni}");
    }

    #[test]
    fn kazda_decyzja_niesie_powod() {
        let s = [fakty_zakladu()];
        let mut f = fakty_towaru();
        f.margin_bp = 4_000;
        f.stock_days = 0;
        let g = [f];
        let v = widok(FirmPersonality::NEUTRAL, &s, &g);
        let d = decide_operational(&v);
        assert!(!d.is_empty());
        for x in &d {
            assert_ne!(x.reason(), DecisionReason::Unspecified);
        }
    }

    #[test]
    fn budzet_osmiu_decyzji_jest_twardy() {
        let s = [fakty_zakladu()];
        let towary: Vec<GoodFacts> = (0..20)
            .map(|i| {
                let mut f = fakty_towaru();
                f.good = GoodId(i);
                f.margin_bp = 4_000;
                f.stock_days = 0;
                f
            })
            .collect();
        let v = widok(FirmPersonality::NEUTRAL, &s, &towary);
        assert_eq!(decide_operational(&v).len(), 8);
    }
}
