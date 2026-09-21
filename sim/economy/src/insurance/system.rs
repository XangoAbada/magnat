//! Doba ubezpieczeń: szkoda, wypłata, składka, nowa polisa (M10d WP10.12).
//!
//! Kolejność kroków w dobie jest wymuszona i nie jest kwestią gustu:
//!
//! 1. **Szkoda** — zgłoszenia z `sim/events` realizują się jako odpis zapasu.
//! 2. **Wypłata** — polisa czynna w chwili szkody płaci, po udziale własnym.
//! 3. **Statystyka** — szkoda wchodzi do szkodowości dzielnicy i miasta.
//! 4. **Miesiąc** — składki i nowe polisy, już po zaktualizowanej statystyce.
//!
//! Gdyby składki szły przed szkodą, dzielnica po powodzi płaciłaby jeszcze przez
//! miesiąc starą stawkę i test „po dwóch zdarzeniach składka jest dwa razy wyższa"
//! mierzyłby opóźnienie, a nie wycenę.

use magnat_core::{
    Cadence, CoverId, DecisionReason, DistrictId, FirmReason, Money, PerilKind, SimMinute, SiteId,
    Tick,
};
use magnat_ecs::{System, SystemCtx, SystemDesc, SystemId, World};
use magnat_firms::{firm_id, FirmKey, FirmStatus, Firms};

use crate::books::{AccountId, Books, TxKind, TxMemo};
use crate::market::Market;

use super::{premium, Cover, Insurers, PerilOccurrence};

/// Klucz typu zakładu, który sprzedaje polisy. Wiązanie idzie po **kluczu
/// tekstowym**, tak samo jak rejestr tytułów medialnych w M10b: `SiteTypeId` jest
/// indeksem w katalogu i zmienia się razem z nim, a klucz nie zmienia się nigdy.
pub const INSURER_SITE_TYPE: &str = "insurance_office";

/// Kapitał otwarcia zakładu ubezpieczeń, w groszach.
///
/// Tyle samo co `KAPITAL_ZAKLADU` zakładu produkcyjnego i z tego samego powodu:
/// firma, która startuje z pustym kontem, nie wypłaci pierwszego odszkodowania
/// i pójdzie ścieżką niewypłacalności w pierwszym miesiącu.
pub const KAPITAL_UBEZPIECZYCIELA: i64 = 120_000_000;

/// Wpina rejestr ubezpieczeń do świata i do funkcji haszującej stan (00 §3.6).
pub fn register_insurers(world: &mut World, ins: Insurers) {
    world.insert_resource(ins);
    world.register_resource_hash::<Insurers>();
}

/// Otwiera rachunki zakładom ubezpieczeń postawionym przez generator.
///
/// **Bez tego kroku cały mechanizm jest martwy, a wygląda na działający** — i to jest
/// znalezisko recenzji M10d, nie przewidywanie. Konta otwierają dziś dwie drogi:
/// sklepom (`world::retail`) i zakładom z linią produkcyjną (`world::plants`).
/// Biuro nie jest ani jednym, ani drugim, więc `Market::account_of_firm` zwracało
/// `None`, `skladki` robiło `continue`, a `wyplac` zero — polisy powstawały, składki
/// nie szły, ekspozycja zostawała zerem i stawka na zawsze równała się priorowi.
/// Kryterium WP10.12 przechodziłoby wyłącznie w testach z ręcznie dopiętym kontem.
///
/// Wołać **raz, przy stawianiu świata**, po `register_firms` i po `register_insurers`.
/// Zwraca liczbę zakładów, którym otwarto rachunek.
pub fn stand_up_insurers(world: &mut World) -> u32 {
    let Some(market) = world.get_resource::<Market>().cloned() else {
        return 0;
    };
    let cele: Vec<(SiteId, FirmKey)> = {
        let (Some(firms), Some(katalog)) = (
            world.get_resource::<Firms>(),
            world.get_resource::<magnat_firms::SiteTypeCatalog>(),
        ) else {
            return 0;
        };
        firms
            .sites()
            .filter(|(_, s)| katalog.get(s.site_type).key == INSURER_SITE_TYPE)
            .map(|(id, s)| (id, s.firm))
            .collect()
    };
    let rest = market.rest_of_world();
    let mut ile = 0;
    for (site, key) in cele {
        if market.account_of_firm(firm_id(key)).is_some() {
            continue;
        }
        let Some(books) = world.get_resource_mut::<Books>() else {
            continue;
        };
        let konto = books.open_account(
            crate::books::AccountOwner::Firm(firm_id(key)),
            crate::books::AccountKind::Current,
            None,
            Money::ZERO,
        );
        if books
            .transfer(
                rest,
                konto,
                Money(KAPITAL_UBEZPIECZYCIELA),
                TxMemo::new(TxKind::Endowment, DecisionReason::Unspecified),
                Tick(0),
            )
            .is_err()
        {
            continue;
        }
        market.register_plant(site, firm_id(key), konto);
        ile += 1;
    }
    ile
}

/// Co się w tej dobie stało.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct InsuranceDay {
    pub covers: u32,
    pub written: u32,
    pub premiums: Money,
    pub damages: u32,
    pub loss: Money,
    pub paid: Money,
}

/// Dobowy krok ubezpieczeń.
pub struct InsuranceSystem {
    desc: SystemDesc,
    last: InsuranceDay,
}

impl InsuranceSystem {
    #[must_use]
    pub fn new() -> InsuranceSystem {
        InsuranceSystem {
            desc: SystemDesc::new("economy.Insurance", Cadence::EveryDay)
                .exclusive()
                // Po zdarzeniach, bo to one zgłaszają szkodę. Kolejność wobec
                // giełdy deklaruje **giełda** (`economy.Equity`, `after_if_present`),
                // a nie ten system: `before` wymaga, żeby cel stał w tym samym
                // harmonogramie (`K-51`), więc scenariusz z samymi ubezpieczeniami
                // nie zbudowałby się — a taki scenariusz jest normalny.
                .after_if_present(SystemId::from_name("events.Event")),
            last: InsuranceDay::default(),
        }
    }

    #[must_use]
    pub fn last(&self) -> InsuranceDay {
        self.last
    }
}

impl Default for InsuranceSystem {
    fn default() -> InsuranceSystem {
        InsuranceSystem::new()
    }
}

impl System for InsuranceSystem {
    fn desc(&self) -> &SystemDesc {
        &self.desc
    }

    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let t = ctx.tick;
        self.last = step_day(ctx.world_mut(), t);
    }
}

/// Jedna doba ubezpieczeń. Wolna funkcja z tego samego powodu co przy giełdzie.
pub fn step_day(world: &mut World, t: Tick) -> InsuranceDay {
    let Some(market) = world.get_resource::<Market>().cloned() else {
        return InsuranceDay::default();
    };
    let Some(mut ins) = world.get_resource_mut::<Insurers>().map(std::mem::take) else {
        return InsuranceDay::default();
    };
    let mut raport = InsuranceDay::default();

    szkody(world, &market, &mut ins, t, &mut raport);
    if t.0.is_multiple_of(60 * 24 * 30) {
        // Postarzenie **przed** składkami: ekspozycja tego miesiąca ma ważyć pełną
        // jednostkę, a nie być od razu przyciętą razem z historią.
        ins.age_one_month();
        skladki(world, &market, &mut ins, t, &mut raport);
        nowe_polisy(world, &market, &mut ins, t, &mut raport);
    }
    raport.covers = ins.cover_count() as u32;

    *world.resource_mut::<Insurers>() = ins;
    raport
}

// ── 1–3. szkoda, wypłata, statystyka ─────────────────────────────────────────────

fn szkody(
    world: &mut World,
    market: &Market,
    ins: &mut Insurers,
    t: Tick,
    raport: &mut InsuranceDay,
) {
    let zgloszenia = ins.take_pending();
    if zgloszenia.is_empty() {
        return;
    }
    let skala = u32::from(ins.params().damage_at_full_bp);
    for o in zgloszenia {
        let cele = cele(market, &o);
        if cele.is_empty() {
            continue;
        }
        // Siła zdarzenia skaluje szkodę liniowo: zdarzenie o połowie siły niszczy
        // połowę tego, co zdarzenie pełne. Bez tego wszystkie pożary byłyby takie same.
        let bp = skala * u32::from(o.severity_bps) / 10_000;
        // Powód jedzie **do księgi zakładu**, a nie tylko do dziennika firmy: karta
        // zakładu przy pożarze ma mówić „pożar", a nie „nieokreślone". Dzielnica
        // w powodzie jest ta ze zgłoszenia, bo zdarzenie zna swój zakres.
        let powod = DecisionReason::Firm(FirmReason::PerilStruck {
            peril: o.peril,
            district: o.district.unwrap_or(DistrictId(0)),
            loss: Money::ZERO,
        });
        for (site, odpis) in market.peril_damage(&cele, bp, powod, t) {
            let district = market.district_of(site).unwrap_or(DistrictId(0));
            ins.note_loss(o.peril, district, odpis);
            raport.damages += 1;
            raport.loss = Money(raport.loss.get() + odpis.get());
            let wyplata = wyplac(world, market, ins, site, o.peril, odpis, t);
            raport.paid = Money(raport.paid.get() + wyplata.get());
            if let Some(firms) = world.get_resource_mut::<Firms>() {
                if let Some(key) = firms.site(site).map(|s| s.firm) {
                    firms.log(
                        key,
                        t,
                        DecisionReason::Firm(FirmReason::PerilStruck {
                            peril: o.peril,
                            district,
                            loss: odpis,
                        }),
                    );
                }
            }
        }
    }
}

/// Zakłady objęte zgłoszeniem.
fn cele(market: &Market, o: &PerilOccurrence) -> Vec<SiteId> {
    if let Some(s) = o.site {
        return vec![s];
    }
    match o.district {
        Some(d) => market.shops_in_district(d),
        None => market.sites(),
    }
}

/// Wypłata z polisy, jeśli jest.
fn wyplac(
    world: &mut World,
    market: &Market,
    ins: &mut Insurers,
    site: SiteId,
    peril: PerilKind,
    loss: Money,
    t: Tick,
) -> Money {
    // `ponytail:` przegląd liniowy po wszystkich polisach, także wygasłych — tak
    // samo jak `CorpFinance` trzyma zakończone leasingi i wykupione obligacje,
    // bo historia firmy o nie pyta. Sufit: przy 80 zakładach, dwóch ryzykach
    // i polisie rocznej to 160 wpisów na rok gry. Droga wyjścia, gdy zacznie boleć:
    // indeks `(zakład, ryzyko) → ostatnia polisa`, a nie kasowanie historii.
    let Some(c) = ins.cover_of(site, peril, SimMinute(t.0)).copied() else {
        return Money::ZERO;
    };
    // Odszkodowanie: szkoda minus udział własny, nie więcej niż suma ubezpieczenia.
    let kwota = Money(
        (loss.get() - c.deductible.get())
            .max(0)
            .min(c.sum_insured.get()),
    );
    if kwota.get() <= 0 {
        return Money::ZERO;
    }
    let (Some(od), Some(do_)) = (
        market.account_of_firm(firm_id(c.insurer)),
        market.account_of(site),
    ) else {
        return Money::ZERO;
    };
    let memo = TxMemo::new(
        TxKind::InsuranceClaim { cover: c.id },
        DecisionReason::Firm(FirmReason::ClaimPaid {
            insurer: firm_id(c.insurer),
            paid: kwota,
        }),
    );
    // Ubezpieczyciel płaci tyle, ile ma: zabraknie mu — idzie ścieżką
    // niewypłacalności M7d jak każda inna firma, a nie cedują ryzyka.
    let mozliwe = Money(kwota.get().min(saldo(world, Some(od)).get().max(0)));
    if mozliwe.get() <= 0 {
        return Money::ZERO;
    }
    let ok = world
        .get_resource_mut::<Books>()
        .is_some_and(|b| b.transfer(od, do_, mozliwe, memo, t).is_ok());
    if !ok {
        return Money::ZERO;
    }
    market.post_insurance_claim(site, mozliwe, t);
    if let Some(firms) = world.get_resource_mut::<Firms>() {
        firms.log(
            c.insurer,
            t,
            DecisionReason::Firm(FirmReason::ClaimPaid {
                insurer: firm_id(c.insurer),
                paid: mozliwe,
            }),
        );
    }
    mozliwe
}

// ── 4. miesiąc: składki i nowe polisy ────────────────────────────────────────────

fn skladki(
    world: &mut World,
    market: &Market,
    ins: &mut Insurers,
    t: Tick,
    raport: &mut InsuranceDay,
) {
    let teraz = SimMinute(t.0);
    let czynne: Vec<Cover> = ins
        .covers()
        .filter(|c| c.is_active(teraz))
        .copied()
        .collect();
    for c in czynne {
        let (Some(od), Some(do_)) = (
            market.account_of(c.site),
            market.account_of_firm(firm_id(c.insurer)),
        ) else {
            continue;
        };
        let memo = TxMemo::new(
            TxKind::InsurancePremium { cover: c.id },
            DecisionReason::Firm(FirmReason::Underwritten {
                peril: c.peril,
                rate_bp: c.rate_bp,
                premium: c.premium_monthly,
            }),
        );
        let ok = world
            .get_resource_mut::<Books>()
            .is_some_and(|b| b.transfer(od, do_, c.premium_monthly, memo, t).is_ok());
        if !ok {
            // Niezapłacona składka kończy ochronę. Bez tego zakład bez grosza byłby
            // ubezpieczony za darmo, a to jest dokładnie ta firma, która najbardziej
            // potrzebuje wypłaty.
            ins.end_cover(c.id);
            continue;
        }
        market.post_insurance_premium(c.site, c.premium_monthly, t);
        raport.premiums = Money(raport.premiums.get() + c.premium_monthly.get());
        // Polisomiesiąc wchodzi do statystyki **po opłaceniu**: ekspozycja bez składki
        // nie jest ekspozycją, tylko wnioskiem.
        ins.note_exposure(c.peril, c.district, c.sum_insured);
    }
}

/// Wystawia polisy zakładom, które ich nie mają.
fn nowe_polisy(
    world: &mut World,
    market: &Market,
    ins: &mut Insurers,
    t: Tick,
    raport: &mut InsuranceDay,
) {
    let ubezpieczyciele = ubezpieczyciele(world);
    if ubezpieczyciele.is_empty() {
        return;
    }
    let params = *ins.params();
    let teraz = SimMinute(t.0);
    let do_konca = SimMinute(t.0 + u64::from(params.term_months) * 30 * 24 * 60);
    let mut i = 0usize;
    for site in market.sites() {
        let Some(district) = market.district_of(site) else {
            continue;
        };
        let suma = market.inventory_value(site);
        if suma < params.min_sum_insured() {
            continue;
        }
        for peril in PerilKind::ALL {
            if ins.cover_of(site, *peril, teraz).is_some() {
                continue;
            }
            // Ubezpieczyciel wybiera się cyklicznie, nie losowo: przy dwóch zakładach
            // ubezpieczeń losowanie dałoby ten sam rozkład, a numer strumienia
            // zapisałby na wieczność mechanizm, którego ta podfaza nie ma.
            let insurer = ubezpieczyciele[i % ubezpieczyciele.len()];
            i += 1;
            let rate = ins.rate_bp(*peril, district);
            let skladka = premium(suma, rate, params.loading_bp, params.policy_fee());
            if skladka.get() <= 0 {
                continue;
            }
            let id = ins.push_cover(Cover {
                id: CoverId(0),
                insurer,
                site,
                district,
                peril: *peril,
                sum_insured: suma,
                deductible: params.deductible(suma),
                premium_monthly: skladka,
                rate_bp: rate.min(u32::from(u16::MAX)) as u16,
                from: teraz,
                until: do_konca,
                ended: false,
            });
            let _ = id;
            raport.written += 1;
            if let Some(firms) = world.get_resource_mut::<Firms>() {
                if let Some(key) = firms.site(site).map(|s| s.firm) {
                    firms.log(
                        key,
                        t,
                        DecisionReason::Firm(FirmReason::Underwritten {
                            peril: *peril,
                            rate_bp: rate.min(u32::from(u16::MAX)) as u16,
                            premium: skladka,
                        }),
                    );
                }
            }
        }
    }
}

/// Firmy prowadzące zakład ubezpieczeń, rosnąco po kluczu.
fn ubezpieczyciele(world: &World) -> Vec<FirmKey> {
    let (Some(firms), Some(katalog)) = (
        world.get_resource::<Firms>(),
        world.get_resource::<magnat_firms::SiteTypeCatalog>(),
    ) else {
        return Vec::new();
    };
    let mut out: Vec<FirmKey> = firms
        .sites()
        .filter(|(_, s)| katalog.get(s.site_type).key == INSURER_SITE_TYPE)
        .filter_map(|(_, s)| firms.get(s.firm).map(|f| (s.firm, f.status)))
        .filter(|(_, st)| *st == FirmStatus::Active)
        .map(|(k, _)| k)
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

fn saldo(world: &World, konto: Option<AccountId>) -> Money {
    let (Some(books), Some(a)) = (world.get_resource::<Books>(), konto) else {
        return Money::ZERO;
    };
    books.account(a).map_or(Money::ZERO, |acc| acc.balance())
}
