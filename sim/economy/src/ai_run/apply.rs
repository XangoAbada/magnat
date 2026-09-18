//! Wykonanie decyzji AI firm (M7e WP11, WP12, WP14).
//!
//! `sim/firms::ai` powiedział **co zrobić**; tutaj dzieje się to na sterowniku ceny,
//! regule zapasu, rejestrze firm i rynku B2B. Każda wykonana akcja zapisuje powód
//! do dziennika firmy — i to jest jedyna droga wykonania, tak samo jak
//! `policy_run::zastosuj` jest jedyną drogą wykonania reguły gracza.

use magnat_core::Qty;
use magnat_core::{SiteId, Tick};
use magnat_firms::{Campaign, FirmKey, Firms, OpsAction, TacAction};
use magnat_policy::{Decided, PolicyCatalog, PolicyDomain};

use crate::market::Market;
use crate::pricing::PricePolicy;
use crate::shop::ReorderPolicy;

use super::FirmAiDay;

impl Market {
    pub(super) fn wykonaj_ops(
        &self,
        firms: &mut Firms,
        key: FirmKey,
        akcja: &Decided<OpsAction>,
        t: Tick,
        d: &mut FirmAiDay,
    ) {
        let zrobione = match *akcja.action() {
            OpsAction::SetMarginTarget { site, good, bp } => {
                let mut m = self.lock();
                let Some(i) = m.by_site.get(&site).copied().map(|i| i as usize) else {
                    return;
                };
                let (dol, gora) = (
                    m.shops[i].pricing.min_margin_bp,
                    m.shops[i].pricing.max_margin_bp,
                );
                let Some(pc) = m.shops[i].controllers.get_mut(&good) else {
                    return;
                };
                // **Zawsze `Dynamic`, nigdy `Fixed`.** Cel marży jest celem, a nie
                // ceną: ogranicznik `min_margin_bp` ma obowiązywać każdą ścieżkę
                // (bramka G3), a `Fixed` omijałby go do następnej decyzji firmy.
                pc.policy = PricePolicy::Dynamic {
                    target_margin_bp: bp,
                    floor_margin_bp: dol,
                    ceil_margin_bp: gora,
                };
                d.margin_moves += 1;
                true
            }
            OpsAction::SetRestockDays { site, good, days } => {
                let mut m = self.lock();
                let Some(i) = m.by_site.get(&site).copied().map(|i| i as usize) else {
                    return;
                };
                let obrot = m.shops[i]
                    .controllers
                    .get(&good)
                    .map_or(Qty::ZERO, crate::pricing::PriceController::turnover_7d);
                // **Ta sama funkcja, którą woła panel gracza.** Druga kopia wzoru
                // rozjechałaby się przy pierwszej zmianie, a rozjazd widać dopiero
                // jako inny wynik bramki (`K-11`).
                let (cel, punkt) = crate::market::restock_from_days(obrot, days);
                let stare = m.shops[i]
                    .inventory
                    .reorder
                    .get(&good)
                    .map_or(1, |r| r.lead_time_days);
                m.shops[i].inventory.reorder.insert(
                    good,
                    ReorderPolicy {
                        point: punkt,
                        target: cel,
                        lead_time_days: stare,
                    },
                );
                d.restock_moves += 1;
                true
            }
        };
        if zrobione {
            firms.log(key, t, akcja.reason());
        }
    }

    pub(super) fn wykonaj_tac(
        &self,
        firms: &mut Firms,
        catalog: &PolicyCatalog,
        key: FirmKey,
        akcja: &Decided<TacAction>,
        t: Tick,
        d: &mut FirmAiDay,
    ) {
        match *akcja.action() {
            TacAction::CloseSite { site } => {
                // Samo zamknięcie domyka wołający: rozwiązanie umów dotyka
                // komponentów mieszkańców, a rynek ich nie widzi.
                d.to_close.push(site);
                firms.log(key, t, akcja.reason());
            }
            TacAction::SetStrategy(kurs) => {
                let Some(f) = firms.get_mut(key) else { return };
                f.strategy = kurs;
                let zaklady: Vec<SiteId> = f.sites.to_vec();
                d.strategy_changes += 1;
                firms.log(key, t, akcja.reason());
                for s in zaklady {
                    if przypnij_preset(firms, catalog, s, kurs) {
                        d.policies_adopted += 1;
                    }
                }
            }
            TacAction::AdoptPolicy { site } => {
                let Some(kurs) = firms.get(key).map(|f| f.strategy) else {
                    return;
                };
                if przypnij_preset(firms, catalog, site, kurs) {
                    d.policies_adopted += 1;
                    firms.log(key, t, akcja.reason());
                }
            }
        }
    }

    /// Wykonanie decyzji kwartalnej (M7f WP13).
    ///
    /// Żadna z tych akcji nie daje się wykonać tutaj w całości i to nie jest brak:
    /// otwarcie zakładu stawia budynek, zamknięcie rozwiązuje umowy, zwinięcie firmy
    /// wyprzedaje majątek — wszystkie trzy dotykają świata, którego rynek nie widzi.
    /// Wychodzą więc listą, tak samo jak `to_close` tieru taktycznego (`AI-1`).
    /// Powód zapisuje się **tutaj**, w chwili decyzji, a nie u wykonawcy — inaczej
    /// decyzja odrzucona przez wykonawcę zniknęłaby bez śladu w dzienniku firmy.
    pub(super) fn wykonaj_str(
        &self,
        firms: &mut Firms,
        key: FirmKey,
        akcja: &Decided<magnat_firms::StrAction>,
        t: Tick,
        d: &mut FirmAiDay,
    ) {
        match *akcja.action() {
            magnat_firms::StrAction::KeepCourse => return,
            magnat_firms::StrAction::OpenSite {
                district,
                slots,
                capex,
            } => d.to_open.push((key, district, slots, capex)),
            magnat_firms::StrAction::CloseSite { site, .. } => d.to_close.push(site),
            magnat_firms::StrAction::RequestVoluntaryClosure => d.to_wind_down.push(key),
        }
        firms.log(key, t, akcja.reason());
    }

    pub(super) fn wykonaj_reakcje(
        &self,
        firms: &mut Firms,
        key: FirmKey,
        akcja: &Decided<Campaign>,
        t: Tick,
        d: &mut FirmAiDay,
    ) {
        let kampania = *akcja.action();
        if let Some(f) = firms.get_mut(key) {
            f.campaign = Some(kampania);
        }
        let zaklady: Vec<SiteId> = firms.get(key).map_or(Vec::new(), |f| f.sites.to_vec());
        match kampania.kind {
            // Wojna cenowa zaczyna boleć **natychmiast**: cel marży schodzi do dolnego
            // krańca własnych widełek na wszystkich półkach z tym towarem. Niżej nie —
            // sprzedaż poniżej kosztu jest praktyką wykluczającą i należy do M8
            // razem z UOKiK, a `min_margin_bp` jest mechanizmem bramki G3.
            magnat_core::ReactionKind::PriceWar => {
                let mut m = self.lock();
                for site in zaklady {
                    let Some(i) = m.by_site.get(&site).copied().map(|i| i as usize) else {
                        continue;
                    };
                    let (dol, gora) = (
                        m.shops[i].pricing.min_margin_bp,
                        m.shops[i].pricing.max_margin_bp,
                    );
                    if let Some(pc) = m.shops[i].controllers.get_mut(&kampania.good) {
                        pc.policy = PricePolicy::Dynamic {
                            target_margin_bp: dol,
                            floor_margin_bp: dol,
                            ceil_margin_bp: gora,
                        };
                    }
                }
            }
            // Wyłączność: pierwszy dostawca tego towaru w mieście sprzedaje odtąd
            // wyłącznie nam, za premię. „Pierwszy" znaczy pierwszy w indeksie
            // sprzedawców, czyli w kolejności zakładów — deterministycznie i bez
            // rankingu, bo rankingu jakości dostawcy firma nie widzi (§5.8).
            magnat_core::ReactionKind::SupplierLock => {
                let m = self.lock();
                let chain = m.chain.clone();
                drop(m);
                let mut ch = chain.lock();
                let Some(dostawca) = ch.b2b.sellers_of(kampania.good).first().copied() else {
                    return;
                };
                ch.b2b.exclusives_mut().lock(
                    dostawca,
                    kampania.good,
                    magnat_supply::Lock {
                        holder: magnat_firms::firm_id(key),
                        until: magnat_core::SimMinute(kampania.until.0),
                        premium_bp: kampania.depth_bp,
                    },
                );
            }
            // Przeciąganie ludzi wykonuje rynek pracy: kampania siedzi w firmie,
            // a `bidding::headhunt` czyta ją przy wyborze adresata oferty
            // bezpośredniej. Tutaj nie ma czego robić — i to jest właściwy podział,
            // bo oferta pracy nie jest ceną na półce.
            magnat_core::ReactionKind::Poach => {}
        }
        d.campaigns_started += 1;
        firms.log(key, t, akcja.reason());
    }
}

/// Przypina zakładowi preset odpowiadający kursowi firmy. Zwraca `false`, gdy zakład
/// nie jest zdelegowany albo katalog nie ma takiego presetu.
fn przypnij_preset(
    firms: &mut Firms,
    catalog: &PolicyCatalog,
    site: SiteId,
    kurs: magnat_core::FirmStrategy,
) -> bool {
    let Some(polityka) =
        crate::policy_run::preset_for(catalog, preset_key(kurs), PolicyDomain::Pricing)
    else {
        return false;
    };
    let Some(s) = firms.site_mut(site) else {
        return false;
    };
    let Some(deleg) = s.delegation.as_mut() else {
        return false;
    };
    deleg.policy = polityka;
    true
}

/// Preset polityki cenowej odpowiadający kursowi firmy (`AZ-1`).
///
/// Sześć kursów, trzy presety — i to nie jest niedoróbka, tylko obserwacja: różnica
/// między „ostrożny" a „konsolidacja" jest różnicą **inwestycyjną**, a nie cenową,
/// i objawi się dopiero w M7f, kiedy firma zacznie otwierać i kupować zakłady.
/// Trzeci preset dla nich obu byłby kopią drugiego.
const fn preset_key(kurs: magnat_core::FirmStrategy) -> &'static str {
    use magnat_core::FirmStrategy as S;
    match kurs {
        S::Discount | S::AggressiveExpansion => "retail_discount",
        S::NicheQuality | S::Innovative => "retail_premium",
        S::Cautious | S::Consolidator => "retail_steady",
    }
}
