//! Doba i miesiąc usług, urzędów i egzekucji (M8d WP7/WP8).
//!
//! Osobny plik od [`crate::systems`] z tego samego powodu, dla którego osobny jest
//! `assess` od `settle`: system ECS odpowiada za **kiedy**, a to jest **co**.
//! Krok miasta miał po M8a pięć punktów; M8d dokłada dwa i robi się z tego funkcja
//! na sto linii w środku implementacji traitu, czyli dokładnie to, przed czym
//! broni reguła przeglądu strukturalnego.

use magnat_core::{DayOfWeek, DecisionReason, PermitKind, ServiceCoverage, SiteId, Tick};
use magnat_ecs::World;
use magnat_economy::Market;

use crate::city::City;
use crate::permits::Applicant;
use crate::services::DistrictPopulation;
use crate::{law, permits, services};

/// Klucz definicji zdarzenia kontroli skarbowej w `data/events/political.ron`.
///
/// Klucz tekstowy, a nie indeks: kolejność definicji w katalogu **nie jest**
/// kontraktem (`EventDef::key` jest), więc indeks rozjechałby się przy pierwszym
/// dopisanym zdarzeniu politycznym.
const AUDIT_KEY: &str = "political/tax_audit";

/// Dobowy krok M8d: emisje, urzędy, kolejka pozwoleń.
///
/// Zwraca powody do dziennika zakładów — wołający zapisuje je przez
/// `Market::log_firm_decision`, bo to on trzyma dziennik.
pub fn dobowy(
    city: &mut City,
    market: &Market,
    world: &mut World,
    t: Tick,
) -> Vec<(SiteId, DecisionReason)> {
    let Some(tuning) = city.tuning.0.as_ref() else {
        return Vec::new();
    };
    let tuning = tuning.clone();

    // 1. Migawka emisji. Kopia, bo krok urzędów trzyma `&mut City`, a `ChainHandle`
    //    jest zasobem świata — dwóch pożyczek naraz nie ma.
    city.emissions.clear();
    if let Some(chain) = world.get_resource::<magnat_supply::ChainHandle>().cloned() {
        // Dwa kroki, a nie jeden, i to jest decyzja o **kolejności zamków**:
        // `Market::shrinkage` bierze najpierw rynek, potem łańcuch, więc pytanie
        // o firmę zadane **w środku** blokady łańcucha ustawiałoby te same dwa
        // zamki w odwrotnej kolejności. Dziś oba wołania idzą z jednego systemu
        // wyłącznego i zakleszczyć się nie mają jak — ale to jest własność
        // harmonogramu, nie tego kodu, a takie założenia starzeją się cicho.
        let pyly: Vec<(SiteId, i64)> = {
            let ch = chain.lock();
            ch.plant
                .iter()
                .map(|(site, plant)| (site, plant.emissions.pm_g_last_minute))
                .filter(|(_, pyl)| *pyl > 0)
                .collect()
        };
        for (site, pyl) in pyly {
            let firma = market.firm_of(site).unwrap_or(magnat_core::FirmId(site.0));
            city.emissions.push((site, firma, pyl));
        }
    }
    city.emissions.sort_by_key(|(s, _, _)| s.0.to_bits());

    // 2. Kto dostał w tej dobie kontrolę skarbową. Hazard liczy `sim/events`
    //    (`CF-1`); miasto odbiera stąd wyłącznie fakt i zamienia go na sprawę.
    let skontrolowane = swiezo_skontrolowane(world, market, t);

    // 3. Urzędy: otwieranie spraw, dowody, rozstrzygnięcia.
    //
    //    Rejestr firm wyjmuje się ze świata na czas kroku (`std::mem::take`), bo
    //    `Firms` nie jest kopiowalny, a krok potrzebuje go obok `&mut City`.
    //    Ta sama droga, którą `economy.Market` wyjmuje finanse firm.
    let firms = world
        .get_resource_mut::<magnat_firms::Firms>()
        .map(std::mem::take);
    let mut powody = law::step_day(city, market, firms.as_ref(), &skontrolowane, &tuning, t);
    if let (Some(f), Some(slot)) = (firms, world.get_resource_mut::<magnat_firms::Firms>()) {
        *slot = f;
    }

    // 4. Nowy zakład składa wniosek o pozwolenie na budowę i płaci za niego.
    //    To jest jedyny dzisiejszy wnioskodawca i jedyna droga, którą kolejka
    //    urzędu w ogóle się napełnia.
    powody.extend(zloz_wnioski_nowych_zakladow(city, market, &tuning, t));

    // 5. Doba urzędu. Dzień tygodnia z kalendarza — urząd ma weekend (`K-15`).
    let dzien = DayOfWeek::from_day_index(t.get() / 1_440);
    for (kto, powod) in permits::process_queue(&mut city.permits, &tuning, dzien, t) {
        if let Applicant::Site(s) = kto {
            powody.push((s, powod));
        }
    }
    powody
}

/// Miesięczny krok M8d: obsada, jakość, pokrycie, ubytki i szara strefa.
pub fn miesieczny(
    city: &mut City,
    market: &Market,
    world: &mut World,
    t: Tick,
) -> Vec<(SiteId, DecisionReason)> {
    let Some(tuning) = city.tuning.0.as_ref() else {
        return Vec::new();
    };
    let tuning = tuning.clone();

    // 1. Obsada placówek i ludność dzielnic — jednym przebiegiem po mieszkańcach,
    //    bo dwa przebiegi po ćwierć miliona encji kosztują dwa razy tyle, a pytanie
    //    jest jedno: „kto gdzie mieszka i gdzie pracuje".
    policz_obsade_i_ludnosc(city, world);

    // 2. Jakość placówek i pokrycie obwodowe.
    let miesiac = magnat_core::SimCalendar::new(t).month_of_year();
    // Udziały planu **po uchwałach rady** (M8e) — ta sama tablica, którą dostał
    // przed chwilą `budget::close_month`. Bez władzy są to udziały z danych.
    let udzialy = crate::rule::shares(city, t);
    let mut powody = services::update_quality(
        &mut city.services,
        &city.budget,
        &city.district_population,
        &tuning,
        miesiac,
        &udzialy,
    );
    services::publish_coverage(&mut city.services, &tuning);

    // 3. Publikacja do świata — jedyne wejście, którym `sim/agents` i `sim/economy`
    //    widzą usługi publiczne. Kierunek wymuszony grafem zależności (`K-8`).
    let pokrycie = city.services.coverage().clone();
    if let Some(slot) = world.get_resource_mut::<ServiceCoverage>() {
        *slot = pokrycie.clone();
    } else {
        world.insert_resource(pokrycie.clone());
    }

    // 4. Obsada urzędów pozwoleń i inspektorów — z tych samych placówek, żeby nie
    //    powstała druga lista etatów. Urząd jest placówką `ServiceKind::Office`.
    przepisz_obsade_urzedow(city);

    // 5. Ubytki inwentaryzacyjne miesiąca — kanał skutku „posterunek → straty".
    market.shrinkage(&pokrycie, tuning.shrinkage_base_bp, t);

    // 6. Decyzja zakładów o szarej strefie. Po domknięciu miesiąca, bo stoi
    //    na wyniku, który właśnie się domknął.
    let s = tuning.shadow;
    for (site, udzial, wynik) in
        market.update_shadow_share(s.step_bp, s.relief_bp, s.ceiling_bp, s.distress_bp)
    {
        powody.push((
            site,
            DecisionReason::ShadowShareSet {
                share_bp: udzial,
                last_result: wynik,
            },
        ));
    }
    powody
}

/// Zakłady, których firma ma świeżo otwarte zdarzenie kontroli skarbowej.
///
/// Z listy zdarzeń czynnych, a nie z kroniki: kronika jest pierścieniem na 128
/// wpisów i przy ruchliwym mieście gubi dobę.
///
/// Okno jest **dwudobowe, nie jednodobowe**, i to nie jest zapas na wszelki wypadek:
/// zdarzenia oceniaą się co godzinę (`events.Event`), a miasto raz na dobę, więc
/// kontrola otwarta o siódmej rano doby `D` jest dla kroku miasta zdarzeniem doby
/// `D` albo `D+1` — zależnie od tego, w której minucie wypada jego kadencja.
/// Powtórki odsiewa `Enforcement::ma_otwarta`, a nie wąskie okno.
fn swiezo_skontrolowane(world: &World, market: &Market, t: Tick) -> Vec<SiteId> {
    let Some(ev) = world.get_resource::<magnat_events::Events>() else {
        return Vec::new();
    };
    let Some(firms) = world.get_resource::<magnat_firms::Firms>() else {
        return Vec::new();
    };
    let doba = t.get() / 1_440;
    let mut out = Vec::new();
    for e in ev.active() {
        if e.started_day + 1 < doba {
            continue;
        }
        if ev.def(e.def).is_none_or(|d| d.key != AUDIT_KEY) {
            continue;
        }
        let magnat_events::ScopeInstance::Firm(k) = e.scope else {
            continue;
        };
        let Some(f) = firms.get(k) else { continue };
        // Kontrola przychodzi po zakładzie, który ukrywa najwięcej — tak samo,
        // jak liczy to sonda, na której stoi hazard.
        if let Some(site) = f
            .sites
            .iter()
            .copied()
            .max_by_key(|s| (market.unreported_bps_of(*s), s.0.to_bits()))
        {
            out.push(site);
        }
    }
    out.sort_by_key(|s| s.0.to_bits());
    out.dedup();
    out
}

/// Nowy zakład handlowy składa wniosek o pozwolenie i płaci opłatę.
///
/// `ponytail:` sufit nazwany — pozwolenie **nie warunkuje** dziś otwarcia zakładu.
/// Warunkowanie wymaga, żeby to miasto stawiało zakład, a stawia go M7 (powstawanie
/// firm) i M10 (rynek nieruchomości); gdy tamta strona dostanie hak, wniosek stanie
/// się warunkiem, a nie fakturą. Do tego czasu kolejka jest prawdziwa, czas
/// oczekiwania emergentny, a opłata realna — brakuje wyłącznie skutku odmowy.
fn zloz_wnioski_nowych_zakladow(
    city: &mut City,
    market: &Market,
    tuning: &crate::tuning::CityTuning,
    t: Tick,
) -> Vec<(SiteId, DecisionReason)> {
    let mut out = Vec::new();
    for site in market.sites() {
        if city.permits.has_permit_for(site) {
            continue;
        }
        let Some(id) = city
            .permits
            .file(Applicant::Site(site), PermitKind::Build, tuning, t)
        else {
            break; // miasto bez urzędu — nie ma komu rozpatrzyć
        };
        let oplata = city.permits.get(id).map_or(magnat_core::Money::ZERO, |p| p.fee);
        if oplata.get() > 0 {
            city.charges.accrue(
                crate::charge::TaxPayer::Site(site),
                magnat_core::TaxKind::License,
                crate::assess::poprzedni_miesiac(t),
                oplata,
                magnat_core::Mass::ZERO,
                0,
                oplata,
                t,
                crate::city::due_on_day(t, city.code.vat_due_day),
            );
        }
        out.push((
            site,
            DecisionReason::TaxAssessed {
                kind: magnat_core::TaxKind::License,
                rate_bp: 0,
                amount: oplata,
            },
        ));
    }
    out
}

/// Jeden przebieg po mieszkańcach: obsada placówek i ludność dzielnic.
fn policz_obsade_i_ludnosc(city: &mut City, world: &mut World) {
    use magnat_agents::{Employment, Identity, Residence};

    let dzielnic = city.services.coverage().districts().max(1);
    let mut ludnosc = vec![0u32; dzielnic];
    let mut obsada: std::collections::BTreeMap<u64, u32> = std::collections::BTreeMap::new();
    for (id, emp, res) in world
        .query::<(&Identity, &Employment, &Residence), ()>()
        .iter()
    {
        if !id.is_alive() {
            continue;
        }
        if let Some(l) = ludnosc.get_mut(res.district as usize) {
            *l += 1;
        }
        if emp.has_job() {
            *obsada.entry(u64::from(emp.site)).or_insert(0) += 1;
        }
    }
    city.district_population = DistrictPopulation(ludnosc);
    for s in city.services.all_mut() {
        // `Employment.site` niesie klucz przesunięty o `SITE_KEY_BASE` (`K-46`),
        // a `PublicService.site` jest tym samym kluczem — most stawia placówki
        // po tej samej stronie granicy co sklepy.
        s.staff = obsada
            .get(&u64::from(s.site.entity().index()))
            .copied()
            .unwrap_or(0);
    }
}

/// Obsada urzędów pozwoleń i inspektorów — z placówek, nie z drugiej listy.
fn przepisz_obsade_urzedow(city: &mut City) {
    let mut po_zakladzie: std::collections::BTreeMap<u64, u32> =
        std::collections::BTreeMap::new();
    for s in city.services.all() {
        if s.kind == magnat_core::ServiceKind::Office {
            po_zakladzie.insert(s.site.0.to_bits(), s.staff);
        }
    }
    let mut urzednikow = 0u32;
    for o in city.permits.offices_mut() {
        o.staff = po_zakladzie.get(&o.site.0.to_bits()).copied().unwrap_or(0);
        urzednikow += o.staff;
    }
    // Inspektorzy: piąta część urzędników miasta na każdy z pięciu urzędów.
    // Podział po równo, bo żaden dokument fazy nie mówi inaczej, a różnicowanie
    // bez uzasadnienia byłoby liczbą wziętą znikąd.
    //
    // **Bez sufitu na jedynce**: miasto, które nie obsadziło ani jednego urzędu,
    // nie prowadzi kontroli. Zero inspektorów znaczy zero spraw naraz (`ma_miejsce`),
    // więc brak administracji jest tu widoczny, a nie zamaskowany.
    let na_urzad = urzednikow / 5;
    for k in magnat_core::AgencyKind::ALL {
        city.enforcement.set_inspectors(*k, na_urzad);
    }
}

/// Wpisuje do świata pusty zasób pokrycia usług, żeby czytelnicy mieli co czytać
/// od pierwszej minuty, a nie dopiero po pierwszym domknięciu miesiąca.
pub fn register_coverage(world: &mut World, districts: usize) {
    if world.get_resource::<ServiceCoverage>().is_none() {
        world.insert_resource(ServiceCoverage::new(districts));
    }
}
