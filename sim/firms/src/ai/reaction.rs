//! Reakcja na wejście rywala (M7e WP14, M7 §5.9, PRD §12.2).
//!
//! # Kiedy firma reaguje
//!
//! **Przy zmierzonej utracie udziału, nigdy przy samym pojawieniu się konkurenta.**
//! To jest mitygacja ryzyka `R12` („reakcja jako prześladowanie"): gracz, który otworzy
//! sklep i nikomu nie zabierze klientów, nie zostanie zaatakowany, bo nie ma za co.
//! Warunek jest koniunkcją dwóch faktów, z których każdy jest **jawny**: sprzedaż
//! spadła wobec poprzedniego tygodnia **i** w okolicy przybyło rywali. Kosztu gracza
//! ani jego marży w tym warunku nie ma i być nie może (§5.8).
//!
//! # Trzy odpowiedzi i ich cena
//!
//! Każda kosztuje **reagującego**, i to jest cała mechanika, bo bez kosztu reakcja
//! byłaby karą wymierzaną graczowi przez system, a nie decyzją firmy:
//! wojna cenowa oddaje marżę, wyłączność płaci dostawcy premię, przeciąganie ludzi
//! płaci nadwyżkę ponad stawkę rywala. Wybór między nimi robi osobowość, nie losowanie.
//!
//! # Czego tu nie ma
//!
//! Zejścia **poniżej kosztu**. Plan fazy pisał „ujemna marża płacona z gotówki
//! reagującego"; wojna cenowa schodzi tu do **dolnego krańca własnych widełek marży**
//! i nie niżej. Powód jest dwojaki i oba są twarde: sprzedaż poniżej kosztu jest
//! praktyką wykluczającą, a ta należy do M8 razem z UOKiK (M7 §2, „nie wchodzi");
//! a ogranicznik marży jest mechanizmem, którego pilnuje bramka G3 balansatora
//! (`min_margin_bp` z `shop.ron`) — wyjątek od niego wywróciłby bramkę, nie firmę.
//! Zejście z 25% marży na 5% boli dostatecznie, żeby decyzja miała ciężar.

use magnat_core::hash::{HashState, StateHasher};
use magnat_core::{DecisionReason, GoodId, JobRoleId, ReactionKind, SiteId, Tick};
use magnat_policy::Decided;

use crate::view::FirmView;

/// Spadek sprzedaży, od którego firma uznaje utratę udziału za realną (bp).
const LOSS_TRIGGER_BP: i32 = 2_000;

/// Indeks niedoboru, od którego przeciąganie ludzi staje się sensowniejsze
/// od wojny cenowej: rywal, który nie ma kogo zatrudnić, nie otworzy drugiej zmiany.
const POACH_SHORTAGE: u16 = 500;

/// Agresja, od której firma woli uderzyć ceną niż zabezpieczyć sobie dostawy.
const PRICE_WAR_AGGRESSION: u8 = 60;

/// Odpowiedź konkurencyjna w toku — jedno pole firmy, nie osobny podsystem (§5.9).
///
/// Jedna kampania na firmę. Nie dlatego, że tak wygodniej, tylko dlatego, że reakcja
/// ma być **decyzją kwartalną o ciężarze**, a nie listą zadań: firma prowadząca trzy
/// wojny naraz nie płaci za żadną z nich osobno i `R12` wraca tylnymi drzwiami.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Campaign {
    pub kind: ReactionKind,
    /// **Zakład** rywala uznany za przyczynę. Bywa nim zakład gracza i to jest ten
    /// sam kod. Zakład, a nie firma: `SiteId` jest jedyną tożsamością, która ma
    /// w tej grze jedną numerację po obu stronach granicy crate'u (`K-46`).
    pub target: SiteId,
    /// Towar, o który idzie gra — przy wojnie cenowej i przy wyłączności.
    pub good: GoodId,
    /// Rola, o którą idzie gra — przy przeciąganiu ludzi.
    pub role: JobRoleId,
    /// Ile to kosztuje reagującego, w punktach bazowych. Znaczy co innego w każdym
    /// wariancie i to jest zamierzone — patrz `DecisionReason::CompetitiveResponse`.
    pub depth_bp: u16,
    /// Do kiedy kampania obowiązuje. Po tym ticku firma wraca do swojego kursu
    /// **sama**, bez osobnej decyzji: kampania bez końca byłaby nowym stanem firmy,
    /// a nie odpowiedzią na zdarzenie.
    pub until: Tick,
}

impl Campaign {
    #[must_use]
    pub const fn active(&self, now: Tick) -> bool {
        now.0 < self.until.0
    }
}

impl HashState for Campaign {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(self.kind.as_index() as u8);
        self.target.0.hash_state(h);
        self.good.hash_state(h);
        h.write_u16(self.role.0);
        h.write_u16(self.depth_bp);
        h.write_u64(self.until.0);
    }
}

/// Kwartał firmy: czy ktoś zabrał nam klientów i czy warto coś z tym zrobić.
///
/// `already` to kampania trwająca — firma, która już odpowiada, nie zaczyna drugiej.
#[must_use]
pub fn decide_reaction(v: &FirmView, already: Option<Campaign>) -> Option<Decided<Campaign>> {
    if already.is_some_and(|c| c.active(v.tick)) {
        return None;
    }
    // Największa zmierzona strata udziału wśród własnych towarów. Kolejność wejścia
    // jest kolejnością `goods`, czyli deterministyczna; remis rozstrzyga pierwszy wpis.
    let (fakty, spadek) = v
        .goods
        .iter()
        .filter(|g| g.rivals > g.rivals_before)
        .filter_map(|g| g.sales_drop_bp().map(|d| (g, d)))
        .filter(|(_, d)| *d >= LOSS_TRIGGER_BP)
        .max_by_key(|(g, d)| (*d, g.good.get()))?;
    let target = fakty.rival_cheapest_site?;

    let p = &v.personality;
    let zaklad = v.site(fakty.site);
    let brak_ludzi = zaklad
        .and_then(|s| s.scarcest_role.filter(|_| s.vacancies > 0))
        .filter(|(_, indeks)| *indeks >= POACH_SHORTAGE);

    // Miesiące kampanii: cierpliwy ciągnie dłużej, niecierpliwy uderza krótko i mocno.
    let miesiace = 1 + u64::from(p.patience) / 34;
    let until = Tick(v.tick.0 + miesiace * 30 * 1440);

    let (kind, role, depth_bp) = match brak_ludzi {
        // Rywal, który nie ma kogo zatrudnić, nie otworzy drugiej zmiany — a to
        // jest tańsze niż oddawanie własnej marży.
        Some((rola, _)) => (ReactionKind::Poach, rola, premia(p.aggression, 300, 1_500)),
        None if p.aggression >= PRICE_WAR_AGGRESSION => (
            ReactionKind::PriceWar,
            JobRoleId(0),
            premia(p.aggression, 500, 2_500),
        ),
        None => (
            ReactionKind::SupplierLock,
            JobRoleId(0),
            premia(p.price_focus, 200, 1_000),
        ),
    };

    let kampania = Campaign {
        kind,
        target,
        good: fakty.good,
        role,
        depth_bp,
        until,
    };
    let _ = spadek;
    Some(Decided::new(
        kampania,
        DecisionReason::CompetitiveResponse {
            kind,
            target,
            depth_bp,
        },
    ))
}

/// Liniowe przełożenie cechy 0..=100 na przedział premii.
const fn premia(cecha: u8, min_bp: u16, max_bp: u16) -> u16 {
    let rozpietosc = (max_bp - min_bp) as u32;
    min_bp + (cecha as u32 * rozpietosc / 100) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::personality::FirmPersonality;
    use crate::view::{CityFacts, GoodFacts, SiteFacts};
    use crate::FirmKey;
    use magnat_core::{DistrictId, Entity, FirmStrategy, Money};
    use std::num::NonZeroU32;

    fn zaklad() -> SiteId {
        SiteId(Entity::new(1, NonZeroU32::MIN))
    }

    fn rywal() -> SiteId {
        SiteId(Entity::new(9, NonZeroU32::MIN))
    }

    fn towar(spadek: bool, nowy_rywal: bool) -> GoodFacts {
        GoodFacts {
            site: zaklad(),
            good: GoodId(3),
            own_price_net: Money(500),
            unit_cost_net: Money(400),
            margin_bp: 2_500,
            stock_days: 9,
            restock_days: 7,
            rival_cheapest_net: Some(Money(450)),
            rival_cheapest_site: Some(rywal()),
            rival_median_net: Some(Money(480)),
            rivals: if nowy_rywal { 2 } else { 1 },
            rivals_before: 1,
            sold_7d: if spadek { 60 } else { 100 },
            sold_7d_prev: 100,
        }
    }

    fn fakty_zakladu(wakaty: u32, niedobor: u16) -> SiteFacts {
        SiteFacts {
            site: zaklad(),
            district: DistrictId(0),
            vacancies: wakaty,
            headcount: 4,
            months_in_loss: 0,
            last_margin_bp: Some(1_000),
            delegated: false,
            needs_policy: false,
            scarcest_role: (wakaty > 0).then_some((JobRoleId(7), niedobor)),
        }
    }

    fn widok<'a>(
        p: FirmPersonality,
        sites: &'a [SiteFacts],
        goods: &'a [GoodFacts],
    ) -> FirmView<'a> {
        FirmView {
            key: FirmKey(1),
            cash: Money(5_000_000),
            personality: p,
            strategy: FirmStrategy::Cautious,
            margin_floor_bp: 500,
            margin_ceiling_bp: 4_000,
            lag_days: 3,
            tick: Tick(1_000),
            sites,
            goods,
            city: CityFacts::default(),
        }
    }

    #[test]
    fn nowy_rywal_bez_utraty_klientow_nie_wyzwala_reakcji() {
        let s = [fakty_zakladu(0, 0)];
        let g = [towar(false, true)];
        assert!(decide_reaction(&widok(FirmPersonality::NEUTRAL, &s, &g), None).is_none());
    }

    #[test]
    fn spadek_bez_nowego_rywala_tez_nie_wyzwala() {
        // Sprzedaż spadła, ale konkurencja się nie zmieniła — to jest sezon albo
        // własny błąd, a nie cudze wejście. Firma ma to naprawić ceną, nie wojną.
        let s = [fakty_zakladu(0, 0)];
        let g = [towar(true, false)];
        assert!(decide_reaction(&widok(FirmPersonality::NEUTRAL, &s, &g), None).is_none());
    }

    #[test]
    fn agresywna_firma_wybiera_wojne_cenowa() {
        let mut p = FirmPersonality::NEUTRAL;
        p.aggression = 90;
        let s = [fakty_zakladu(0, 0)];
        let g = [towar(true, true)];
        let d = decide_reaction(&widok(p, &s, &g), None).expect("reakcja");
        assert_eq!(d.action().kind, ReactionKind::PriceWar);
        assert_eq!(d.action().target, rywal());
    }

    #[test]
    fn spokojna_firma_woli_zabezpieczyc_dostawy() {
        let mut p = FirmPersonality::NEUTRAL;
        p.aggression = 20;
        let s = [fakty_zakladu(0, 0)];
        let g = [towar(true, true)];
        let d = decide_reaction(&widok(p, &s, &g), None).expect("reakcja");
        assert_eq!(d.action().kind, ReactionKind::SupplierLock);
    }

    #[test]
    fn niedobor_ludzi_zmienia_odpowiedz_na_przeciaganie() {
        let mut p = FirmPersonality::NEUTRAL;
        p.aggression = 90;
        let s = [fakty_zakladu(2, 800)];
        let g = [towar(true, true)];
        let d = decide_reaction(&widok(p, &s, &g), None).expect("reakcja");
        assert_eq!(d.action().kind, ReactionKind::Poach);
        assert_eq!(d.action().role, JobRoleId(7));
    }

    #[test]
    fn firma_prowadzaca_kampanie_nie_zaczyna_drugiej() {
        let mut p = FirmPersonality::NEUTRAL;
        p.aggression = 90;
        let s = [fakty_zakladu(0, 0)];
        let g = [towar(true, true)];
        let v = widok(p, &s, &g);
        let trwa = *decide_reaction(&v, None).expect("reakcja").action();
        assert!(decide_reaction(&v, Some(trwa)).is_none());
        // Po wygaśnięciu wolno zacząć od nowa — kampania jest odpowiedzią, nie stanem.
        let wygasla = Campaign {
            until: Tick(500),
            ..trwa
        };
        assert!(decide_reaction(&v, Some(wygasla)).is_some());
    }

    #[test]
    fn reakcja_zawsze_niesie_powod_z_celem() {
        let mut p = FirmPersonality::NEUTRAL;
        p.aggression = 90;
        let s = [fakty_zakladu(0, 0)];
        let g = [towar(true, true)];
        let d = decide_reaction(&widok(p, &s, &g), None).expect("reakcja");
        match d.reason() {
            DecisionReason::CompetitiveResponse {
                kind,
                target,
                depth_bp,
            } => {
                assert_eq!(kind, ReactionKind::PriceWar);
                assert_eq!(target, rywal());
                assert!(depth_bp > 0, "reakcja bez kosztu nie jest reakcją");
            }
            inny => panic!("nie ten powód: {inny:?}"),
        }
    }
}
