//! Rządzenie: co miasto robi ze swoją władzą co dobę i co miesiąc (M8e WP9, WP10).
//!
//! Osobny plik od [`crate::step`] z tego samego powodu, dla którego tamten jest
//! osobny od [`crate::systems`]: system ECS odpowiada za **kiedy**, `step` za
//! usługi i urzędy, a to jest **władza**. Trzy rzeczy, w tej kolejności i nie innej:
//!
//! 1. **Skutki uchwał nakłada się co miesiąc od nowa, z obowiązującego zbioru.**
//!    Nie „przy uchwaleniu", bo uchwała wchodzi w życie po vacatio legis i wygasa
//!    po `sunset` — a stan nałożony jednorazowo przeżyłby jedno i drugie. Nakładanie
//!    idempotentne znaczy, że obowiązujący zbiór uchwał **jest** stanem regulacji,
//!    a nie jego historią.
//! 2. **Decyzja zapada po skutkach**, bo burmistrz patrzy na miasto takie, jakie
//!    jest po jego poprzednich uchwałach, a nie takie, jakim było przed nimi.
//! 3. **Wybory idą co dobę z bramką**, bo dzień wyborów jest jeden na kadencję
//!    i sprawdzenie go kosztuje porównanie dwóch liczb. Sam cykl wyborczy —
//!    kampania, wpłaty, głosowanie, objęcie urzędu — mieszka w [`crate::ballot`];
//!    tutaj zostaje jego wyzwalacz.
//!
//! Czego tu nie ma: **miasta jako pracodawcy**. Plan wydatków dalej wychodzi
//! na konto reszty świata (`ponytail:` sufit z M8a) — z jednym wyjątkiem, i to
//! jest cała nowość przepływu: **umowa z przetargu płaci prawdziwej firmie**.
//! Pensja nauczyciela wymaga `FirmKey` dla miasta i konsumenta `PayrollOutbox`,
//! którego nie ma od M7b; adres jest w tabeli korekt.

use magnat_core::{
    rng, AgencyKind, DecisionReason, DistrictId, Money, PolicyKind, ServiceKind, SiteId,
    SpendCategory, StreamId, TaxKind, TenderKind, Tick, Q, SPEND_CATEGORY_COUNT,
};
use magnat_economy::Market;
use magnat_ecs::World;

use crate::city::City;
use crate::gov::{self, ApprovalInput, Signals};
use crate::policy::Policy;
use crate::tender::{Bid, BidCriteria, TenderSubject};

/// Ile miesięcy trwa umowa z przetargu. Dwa lata: krócej niż kadencja, więc
/// każdy burmistrz rozstrzyga przynajmniej jeden przetarg i odpowiada za niego.
const CONTRACT_MONTHS: u16 = 24;
/// Ile dób firmy mają na złożenie oferty.
const BID_DAYS: u16 = 14;

/// Dobowy krok władzy: przetargi i wybory.
pub fn dobowy(
    city: &mut City,
    market: &Market,
    world: &mut World,
    t: Tick,
) -> Vec<(SiteId, DecisionReason)> {
    if !city.gov.is_active() {
        return Vec::new();
    }
    let mut powody = Vec::new();
    zbierz_oferty(city, market, world, t, &mut powody);
    for p in city.tenders.close_due(CONTRACT_MONTHS, t) {
        city.log_reason(t, p);
    }
    crate::ballot::cykl(city, market, world, t, &mut powody);
    powody
}

/// Miesięczny krok władzy: skutki uchwał, poparcie, decyzja, przetargi.
pub fn miesieczny(
    city: &mut City,
    market: &Market,
    world: &mut World,
    t: Tick,
) -> Vec<(SiteId, DecisionReason)> {
    if !city.gov.is_active() {
        return Vec::new();
    }
    city.gov.month = city.gov.month.saturating_add(1);

    // 1. Zapłata za usługi kupione na zewnątrz. **Jedyny wydatek publiczny, który
    //    trafia na konto z tego miasta** — reszta planu dalej wychodzi poza model.
    let powody = zaplac_umowy(city, market, world, t);

    // 2. Sygnały i poparcie — po skutkach, bo burmistrz ocenia miasto takie,
    //    jakim je zostawił.
    let sig = sygnaly(city, market, world);
    zaktualizuj_poparcie(city, world, sig);

    // 3. Decyzja. Jedna uchwała na miesiąc, nie wszystkie opłacalne.
    let rates = city.rates();
    let baza = city.policy.shares();
    let limit = city.enforcement.emission_limit_g_per_min();
    let tun_present = city.gov.tuning.is_some();
    if tun_present {
        if let Some(rec) =
            crate::mayor::decide_month(&mut city.gov, &city.policies, &rates, &baza, limit, sig, t)
        {
            let powod = rec.reason;
            let for_bp = rec.vote.for_bp;
            let kind = rec.policy.kind();
            city.policies.enact(rec);
            // Powód niesie **wynik głosowania**, a decyzja burmistrza znała
            // wyłącznie propozycję — stąd podmiana pola tuż przed zapisem.
            city.log_reason(
                t,
                match powod {
                    DecisionReason::PolicyEnacted { delay_days, .. } => {
                        DecisionReason::PolicyEnacted {
                            kind,
                            for_bp,
                            delay_days,
                        }
                    }
                    inny => inny,
                },
            );
        }
    }

    // 4. Przetarg na odbiór odpadów tam, gdzie umowa wygasła albo jej nie było.
    ogloz_przetargi(city, t);
    powody
}

// ── skutki uchwał ───────────────────────────────────────────────────────────────

/// Nakłada skutki wszystkich obowiązujących uchwał. Idempotentnie: wywołanie
/// dwa razy z rzędu daje ten sam stan, bo każdy skutek jest **przypisaniem**,
/// a nie przyrostem.
///
/// Wołane **co dobę i przed wszystkim innym**, bo uchwała wchodzi w życie
/// w konkretnym ticku, a nie na początku miesiąca: próg płacy minimalnej ma
/// obowiązywać inspekcję pracy tego samego dnia, w którym zaczął obowiązywać.
pub fn nalozenie(city: &mut City, market: &Market, world: &mut World, t: Tick) {
    if !city.gov.is_active() {
        return;
    }
    // Stawki: kodeks przebudowuje się dopiero, gdy uchwała różni się od tego,
    // co obowiązuje. Przebudowa kosztuje `resolve` po całym katalogu towarów,
    // więc robienie jej co miesiąc „na wszelki wypadek" byłoby czterystoma
    // wyszukiwaniami miesięcznie za nic.
    let mut kodeks = None;
    for k in TaxKind::ALL {
        let Some(Policy::TaxRate { bps, .. }) =
            city.policies
                .current(PolicyKind::TaxRate, k.as_index() as u32, t)
        else {
            continue;
        };
        if city.code.rate_of(*k) != *bps {
            let baza = kodeks.as_ref().unwrap_or(&*city.code);
            kodeks = Some(baza.with_rate(*k, *bps, t));
        }
    }
    if let Some(nowy) = kodeks {
        // Katalog towarów bierze się z łańcucha stojącego w świecie — ten sam
        // powód co w moście (`city_bridge::setup`): `GoodId` jest indeksem
        // w katalogu **tej epoki**, a nie liczbą globalną.
        let goods = world
            .get_resource::<magnat_supply::ChainHandle>()
            .map(|c| std::sync::Arc::clone(&c.cat));
        if let Some(cat) = goods {
            if let Ok(tabela) = nowy.resolve(&cat) {
                city.code = std::sync::Arc::new(nowy);
                city.rates = std::sync::Arc::new(tabela);
                // Rynek musi zacząć składać cenę brutto po nowemu — bez tego
                // uchwała zmieniłaby wyłącznie deklarację, a nie cenę na półce.
                market.set_tax_engine(Box::new(city.tax_engine()));
            }
        }
    }

    // Limit emisji: jedna liczba w rejestrze urzędów, w **gramach na minutę** —
    // w tych samych jednostkach, w których liczy pył `sim/supply`. Pierwsza wersja
    // brała „gramy na dobę" i wpisywała je jako minutowe, czyli rozluźniała limit
    // tysiąc czterysta czterdzieści razy; ochrona środowiska przestawała wtedy
    // otwierać sprawy, a sygnał `emission_bp` porównywał dwie różne wielkości.
    if let Some(Policy::EmissionLimit { max_g_per_min }) =
        city.policies.current(PolicyKind::EmissionLimit, 0, t)
    {
        city.enforcement.set_emission_limit(*max_g_per_min);
    }
    // Płaca minimalna: próg, poniżej którego inspekcja pracy otwiera sprawę (`CH-5`).
    if let Some(Policy::MinWage(m)) = city.policies.current(PolicyKind::MinWage, 0, t) {
        city.enforcement.set_min_wage(*m);
    }
    // Obsada urzędów kontrolnych (`CH-6`). Uchwała **zastępuje** podział po równo
    // z obsady urzędów miejskich — i to jest cała treść „budżetu urzędu": rada
    // decyduje, ilu inspektorów ma skarbówka, a nie ile ma pieniędzy.
    for (i, ilu) in gov::agency_staffing(&city.policies, t).iter().enumerate() {
        if let (Some(n), Some(a)) = (ilu, AgencyKind::from_index(i)) {
            city.enforcement.set_inspectors(a, *n);
        }
    }
    // Godziny handlu w dzielnicy.
    for r in city.policies.in_force(t) {
        if let Policy::TradingHours { district, hours } = &r.policy {
            let _ = market.set_trading_hours(district.0, *hours);
        }
    }
    // Sufit taryfy operatora sieci (`CD-5`, `CF-5`). Taryfa jest polem sieci
    // i ląduje w liczniku zakładu co krok, więc uchwała działa od razu — to
    // jest ten sam ruch, którym `sim/events` nakłada parametry na pola
    // właścicieli (`CE-4`): piszący sięga do pola czytelnika.
    //
    // **Sufit jest odwracalny**, więc miasto zapamiętuje taryfę sprzed pierwszej
    // uchwały: uchwała, która wygasła, ma przestać obowiązywać, a nie zostawić
    // po sobie cenę na zawsze. Bez tego `sunset` byłby polem bez skutku.
    let capy: Vec<(magnat_core::UtilityService, Option<Money>)> = magnat_core::UtilityService::ALL
        .iter()
        .map(|u| {
            let sufit = match city
                .policies
                .current(PolicyKind::TariffCap, u.as_index() as u32, t)
            {
                Some(Policy::TariffCap { max_per_unit, .. }) => Some(*max_per_unit),
                _ => None,
            };
            (*u, sufit)
        })
        .collect();
    if let Some(grids) = world.get_resource_mut::<magnat_traffic::utility::UtilityGrids>() {
        for (usluga, sufit) in capy {
            let Some(net) = grids.net_mut(usluga) else {
                continue;
            };
            let baza = *city
                .tariff_base
                .entry(usluga.as_index() as u8)
                .or_insert(net.tariff.per_unit);
            net.tariff.per_unit = match sufit {
                Some(s) if s.get() < baza.get() => s,
                _ => baza,
            };
        }
    }
}

/// Udziały planu wydatków po uchwałach rady — **jedno** wejście dla przelewu
/// (`budget::close_month`) i dla jakości placówki (`services::update_quality`).
#[must_use]
pub fn shares(city: &City, t: Tick) -> [u32; SPEND_CATEGORY_COUNT] {
    city.policies.shares(city.policy.shares(), t)
}

/// Ile miasto wyda w tym miesiącu na umowy z przetargów, per kierunek wydatku.
///
/// Budżet **rezerwuje** tę kwotę zamiast wydać ją z planu, bo usługa kupiona
/// na zewnątrz ma jedną cenę, a nie dwie.
#[must_use]
pub fn contracted(city: &City, t: Tick) -> [Money; SPEND_CATEGORY_COUNT] {
    let mut out = [Money::ZERO; SPEND_CATEGORY_COUNT];
    for (_, c) in city.tenders.contracts_in_force(t) {
        let i = SpendCategory::Waste.as_index();
        out[i] = Money(out[i].get() + c.price.get());
    }
    out
}

// ── sygnały ─────────────────────────────────────────────────────────────────────

/// Cztery liczby, na które patrzy burmistrz, plus trzy, po których sięga
/// po narzędzia regulacyjne. Wszystkie **zmierzone**, żeby karta rady mogła
/// pokazać liczbę obok decyzji, a nie samą decyzję.
fn sygnaly(city: &City, market: &Market, world: &World) -> Signals {
    let plan = city.budget.plan_base_month().get().max(1);
    let gotowka = world
        .get_resource::<magnat_economy::Books>()
        .and_then(|b| b.balance(city.budget.account))
        .map_or(0, |m| m.get());
    let rezerwa = plan * i64::from(city.policy.reserve_target_months);
    // Deficyt mierzy się **cięciem planu**, bo to ono jest faktem, który właśnie
    // zaszedł; nadwyżkę — zapasem ponad rezerwę, bo to on jest pieniądzem,
    // którego miasto nie musi trzymać.
    let fiscal_bp = if city.budget.cut_bp > 0 {
        -i32::try_from(city.budget.cut_bp.min(10_000)).unwrap_or(-10_000)
    } else {
        i32::try_from(((gotowka - rezerwa).max(0) * 10_000 / plan).min(10_000)).unwrap_or(0)
    };
    let roczne = plan * 12;
    let debt_bp =
        u32::try_from(city.budget.debt_outstanding().get() * 10_000 / roczne.max(1)).unwrap_or(0);

    // Luka w usługach: średnie pokrycie miasta po wszystkich rodzajach, odjęte
    // od pełni. Rodzaj bez ani jednej placówki liczy się jako zero i **ma** się
    // tak liczyć — miasto bez straży pożarnej ma lukę, a nie brak pomiaru.
    let pokrycie = city.services.coverage();
    let suma: u32 = ServiceKind::ALL
        .iter()
        .map(|k| u32::from(pokrycie.city_mean(*k).get()))
        .sum();
    let srednia = suma / u32::try_from(ServiceKind::ALL.len()).unwrap_or(8);
    let service_gap_bp = 10_000 - (srednia * 100).min(10_000);
    // Najgorzej pokryty rodzaj usługi. Remis rozstrzyga kolejność `ServiceKind`,
    // czyli ta sama, która indeksuje wiersz pokrycia — jawnie, nie przypadkiem.
    let worst_service = ServiceKind::ALL
        .iter()
        .enumerate()
        .min_by_key(|(i, k)| (pokrycie.city_mean(**k).get(), *i))
        .map_or(0, |(i, _)| u8::try_from(i).unwrap_or(0));

    let (unemployment_permille, _) = world
        .get_resource::<magnat_events::Events>()
        .map_or((0, 0), |e| (e.indicators().unemployment_permille, 0u32));

    // Emisje wobec obowiązującego limitu: 10 000 znaczy „najbrudniejszy zakład
    // stoi dokładnie na limicie", zero — „limit jest daleko i nic nie znaczy".
    let limit = city.enforcement.emission_limit_g_per_min().max(1);
    let najbrudniejszy = city.emissions.iter().map(|(_, _, g)| *g).max().unwrap_or(0);
    let emission_bp = u32::try_from((najbrudniejszy * 10_000 / limit).min(10_000)).unwrap_or(0);

    let (_, shadow_bp) = market.shadow_stats();

    Signals {
        fiscal_bp,
        debt_bp,
        approval_bp: city.gov.approval_mean_bp(),
        service_gap_bp,
        worst_service,
        unemployment_permille,
        emission_bp,
        shadow_bp,
    }
}

/// Poparcie per dzielnica. Usługi liczą się **lokalnie**, reszta globalnie —
/// i to jest jedyny powód, dla którego test wrażliwości T6 w ogóle mierzy
/// cokolwiek poza szumem.
fn zaktualizuj_poparcie(city: &mut City, world: &World, sig: Signals) {
    let Some(tun) = city.gov.tuning.as_ref() else {
        return;
    };
    let nastroj = world
        .get_resource::<magnat_events::Events>()
        .map_or(0, |e| i32::from(e.indicators().mood_mean.get()));
    let obciazenie = gov::tax_burden_bp(&{
        let mut r = [0u32; magnat_core::TAX_KIND_COUNT];
        for k in TaxKind::ALL {
            r[k.as_index()] = city.code.rate_of(*k);
        }
        r
    });
    let pokrycie = city.services.coverage();
    let dzielnic = city.gov.approval_bps_by_district.len();
    let mut nowe = Vec::with_capacity(dzielnic);
    for d in 0..dzielnic {
        let suma: u32 = ServiceKind::ALL
            .iter()
            .map(|k| u32::from(pokrycie.at(DistrictId(d as u16), *k).get()))
            .sum();
        let coverage_q = suma / u32::try_from(ServiceKind::ALL.len()).unwrap_or(8);
        nowe.push(gov::approval_step(
            city.gov.approval_bps_by_district[d],
            ApprovalInput {
                coverage_q,
                tax_burden_bp: obciazenie,
                mood: nastroj,
                unemployment_permille: sig.unemployment_permille,
            },
            tun,
        ));
    }
    city.gov.approval_bps_by_district = nowe;
}

// ── przetargi ───────────────────────────────────────────────────────────────────

/// Ogłasza przetarg na odbiór odpadów w każdej dzielnicy bez ważnej umowy.
///
/// Odpady, bo to jedyna usługa, za którą miasto płaci od M8a, a której **nie ma
/// w mieście**: `SpendCategory::Waste` ma udział w planie wydatków, a
/// `ServiceKind::Waste` nie ma ani jednego archetypu budynku. Przetarg zamyka
/// tę dziurę od strony, z której da się ją zamknąć teraz.
fn ogloz_przetargi(city: &mut City, t: Tick) {
    let Some(tun) = city.gov.tuning.as_ref() else {
        return;
    };
    let udzial = i64::from(tun.tender_waste_bp);
    let dzielnic = city.gov.approval_bps_by_district.len();
    if dzielnic == 0 {
        return;
    }
    let plan = city.budget.plan_base_month().get();
    let na_odpady = plan
        * i64::from(city.policy.shares()[magnat_core::SpendCategory::Waste.as_index()])
        / 10_000;
    let budzet = Money(na_odpady * udzial / 10_000 / dzielnic as i64);
    if budzet.get() <= 0 {
        return;
    }
    let mut wpisy = Vec::new();
    for d in 0..dzielnic {
        let subject = TenderSubject {
            kind: TenderKind::WasteCollection,
            id: d as u16,
        };
        if let Some((_, powod)) =
            city.tenders
                .publish(subject, budzet, BidCriteria::default(), BID_DAYS, t)
        {
            wpisy.push(powod);
        }
    }
    for p in wpisy {
        city.log_reason(t, p);
    }
}

/// Firmy składają oferty w trwających przetargach.
///
/// Kto licytuje: zakład z dzielnicy przedmiotu, a jeśli takich nie ma — zakład
/// spoza niej, z gorszą punktacją za brak lokalności. Cena bierze się z budżetu
/// przetargu i rozrzutu ze strumienia [`StreamId::TenderScoring`]: firma nie liczy
/// tu kosztu własnego, bo odbiór odpadów nie jest linią produkcyjną i nie ma
/// receptury. `ponytail:` sufit z nazwaną drogą wyjścia — kiedy M10 doda usługi
/// jako przedmiot działalności, cena wyjdzie z rachunku zakładu, a ten rozrzut
/// zostanie tym, czym miał być: miarą tego, komu zależy.
fn zbierz_oferty(
    city: &mut City,
    market: &Market,
    world: &World,
    t: Tick,
    powody: &mut Vec<(SiteId, DecisionReason)>,
) {
    // Oferty składa się **w przeddzień terminu**, a nie co dobę i nie w dobie
    // ogłoszenia. Trzy podejścia i każde następne poprawia konkretny błąd:
    //
    // 1. co dobę do terminu — każda doba nadpisywała ofertę z poprzedniej nowym
    //    rzutem, więc liczyła się wyłącznie ostatnia, a trzynaście dób szło w nic;
    // 2. w dobie ogłoszenia — **nie zbierała się ani jedna oferta**, bo przetargi
    //    ogłasza krok miesięczny, a ten stoi w kroku miasta **za** dobowym: gdy
    //    oferenci patrzyli na listę, przetargu jeszcze nie było, a nazajutrz
    //    `published_at` już się nie zgadzał. Objaw: pięćset ogłoszonych postępowań
    //    i zero rozstrzygniętych w przebiegu na 1500 dób — mechanizm, który
    //    przechodzi każdy test, bo żaden nie pytał, czy ktokolwiek staje;
    // 3. w przeddzień terminu — jedno przejście, termin składania ofert ma treść,
    //    a kolejność kroków w obrębie doby przestaje cokolwiek znaczyć.
    let otwarte: Vec<(magnat_core::TenderId, TenderSubject, Money)> = city
        .tenders
        .all()
        .iter()
        .filter(|x| x.outcome.is_none() && !x.closed && x.bid_deadline.0 == t.0 + 1_440)
        .map(|x| (x.id, x.subject, x.budget))
        .collect();
    if otwarte.is_empty() {
        return;
    }
    let Some(firms) = world.get_resource::<magnat_firms::Firms>() else {
        return;
    };
    // Jeden przebieg po zakładach na dobę, nie jeden na przetarg.
    let zaklady: Vec<(SiteId, magnat_firms::FirmKey, DistrictId)> = firms
        .sites()
        .filter(|(_, s)| s.positions.len() >= 2)
        .map(|(site, s)| (site, s.firm, s.district))
        .collect();
    if zaklady.is_empty() {
        return;
    }
    let seed = seed_of(world);
    for (id, subject, budzet) in otwarte {
        let dzielnica = crate::tender::subject_district(subject);
        // Lokalni pierwsi, potem reszta; w obrębie grupy po `SiteId` rosnąco —
        // remis rozstrzygnięty jawnie, nie kolejnością w rejestrze.
        let mut kandydaci: Vec<(SiteId, magnat_firms::FirmKey, bool)> = zaklady
            .iter()
            .map(|(site, firma, d)| (*site, *firma, dzielnica.is_none_or(|x| *d == x)))
            .collect();
        kandydaci.sort_unstable_by_key(|(s, _, lokalny)| (!*lokalny, s.0.to_bits()));
        for (site, firma, lokalny) in kandydaci.into_iter().take(3) {
            // Klucz rzutu niesie **numer przetargu**, nie sam zakład: bez tego
            // ten sam zakład składał w dziesięciu przetargach tej samej doby
            // dziesięć identycznych ofert — ta sama cena, jakość i termin.
            let klucz = site.0.index() ^ id.0.wrapping_mul(0x9E37_79B9);
            let mut r = rng(seed, StreamId::TenderScoring, klucz, t);
            let szum = i64::from(r.next_u32() % u32::from(szum_bp(city)));
            let cena = Money(budzet.get() * (10_000 - szum) / 10_000);
            let jakosc = Q::new(u8::try_from(50 + r.next_u32() % 45).unwrap_or(60));
            let dni = u16::try_from(5 + r.next_u32() % 40).unwrap_or(20);
            let zlozona = city.tenders.submit_bid(
                id,
                Bid {
                    bidder: firma,
                    site,
                    price: cena,
                    quality_promise: jakosc,
                    delivery_days: dni,
                    local: lokalny,
                    kickback: None,
                },
                t,
            );
            if zlozona {
                powody.push((
                    site,
                    DecisionReason::TenderPublished {
                        subject: subject.kind,
                        subject_id: subject.id,
                        budget: cena,
                    },
                ));
            }
        }
    }
    let _ = market;
}

/// Rozrzut oferty wokół budżetu przetargu, w punktach bazowych. Z danych,
/// a nie ze stałej — `bid_noise_bp` w `data/city/government.ron` istnieje po to,
/// żeby balansator mógł nim ruszyć bez rekompilacji.
fn szum_bp(city: &City) -> u16 {
    city.gov.tuning.as_ref().map_or(1_500, |t| {
        u16::try_from(t.bid_noise_bp.max(1)).unwrap_or(1_500)
    })
}

/// Zapłata za obowiązujące umowy. Pieniądz idzie z konta miasta na konto zakładu,
/// który usługę wykonuje — przez `Books::transfer`, jak każdy inny przelew.
fn zaplac_umowy(
    city: &mut City,
    market: &Market,
    world: &mut World,
    t: Tick,
) -> Vec<(SiteId, DecisionReason)> {
    let naleznosci = crate::tender::due_this_month(&city.tenders, t);
    if naleznosci.is_empty() {
        return Vec::new();
    }
    let konto_miasta = city.budget.account;
    let mut out = Vec::new();
    let mut zerwane: Vec<SiteId> = Vec::new();
    for (site, kwota) in naleznosci {
        let Some(konto) = market.account_of(site) else {
            continue;
        };
        let memo = magnat_economy::TxMemo::new(
            magnat_economy::TxKind::PublicSpend {
                program: magnat_economy::ProgramId(
                    magnat_core::SpendCategory::Waste.as_index() as u16
                ),
            },
            DecisionReason::PublicSpend {
                category: magnat_core::SpendCategory::Waste,
                amount: kwota,
            },
        );
        let ok = world
            .get_resource_mut::<magnat_economy::Books>()
            .map(|b| b.transfer(konto_miasta, konto, kwota, memo, t).is_ok())
            .unwrap_or(false);
        if ok {
            let i = SpendCategory::Waste.as_index();
            city.budget.spend_ytd[i] = Money(city.budget.spend_ytd[i].get() + kwota.get());
            out.push((
                site,
                DecisionReason::PublicSpend {
                    category: SpendCategory::Waste,
                    amount: kwota,
                },
            ));
        } else {
            // Miasto nie zapłaciło — umowa się kończy. Cicha darmowa usługa
            // byłaby gorsza niż zerwana umowa: wykonawca pracowałby za nic,
            // a raport pokazywałby kontrakt, którego nikt nie realizuje.
            zerwane.push(site);
        }
    }
    for site in zerwane {
        city.tenders.terminate(site, t);
        out.push((
            site,
            DecisionReason::PublicSpend {
                category: SpendCategory::Waste,
                amount: Money::ZERO,
            },
        ));
    }
    out
}

/// Ziarno świata. Jedno miejsce, bo wybory i przetargi mają losować z tego samego.
pub(crate) fn seed_of(world: &World) -> u64 {
    world.seed
}
