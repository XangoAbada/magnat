//! Lista płac obciąża pracodawcę (`R2-WP30`, pozycja 58 wykazu `R2`).
//!
//! `sim/firms` nie może sięgnąć po `Books` — zależność idzie `economy → firms`, nie
//! odwrotnie — więc `FirmSystem::run` odkłada wypłaty w [`PayrollOutbox`], a księguje
//! je ten, kto ma czym. Ten sam wzorzec działa dla rozliczeń B2B (`absorb_settlements`)
//! i dla decyzji AI (`DecisionOutbox`).
//!
//! **Skrzynka nie miała konsumenta od M7b.** `PayrollOutbox::take()` nie miało w całym
//! repozytorium ani jednego wołającego, więc lista płac rosła w nieskończoność,
//! a gospodarstwa dostawały pieniądze zupełnie inną drogą: `pay_incomes` przelewało
//! `Household.income_monthly` z konta `rest_of_world`. Skutki były trzy i wszystkie
//! zapisane wcześniej: rachunek wyniku zakładu nie znał kosztu pracy, miasto nie mogło
//! być pracodawcą, a pieniądz wchodził do gospodarstw z konta bez pokrycia
//! w gospodarce — czego niezmiennik `P1` nie łapie, bo `rest_of_world` jest kontem
//! emisyjnym i ma prawo schodzić poniżej zera.
//!
//! **Potrącenie PIT zostaje tam, gdzie było** (`market.withhold`, hak M8): zmienia się
//! płatnik, nie mechanizm. `K-57` wpiął zaliczkę w punkt, w którym gospodarstwo dostaje
//! netto — i R2 nie ma prawa go przestawić. Potrącony pieniądz zostaje u pracodawcy,
//! a miasto zabiera go systemem `city.Tax` razem z deklaracją miesięczną.

use magnat_agents::{Household, Identity};
use magnat_core::{DecisionReason, Money, Tick};
use magnat_ecs::World;

use crate::books::{Books, TxKind, TxMemo};
use crate::market::Market;

/// Co zrobiła doba listy płac — do raportu scenariusza i do testów.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct PayrollDay {
    /// Ile wypłat doszło do gospodarstwa.
    pub paid: u32,
    /// Ile wypłat przepadło, bo pracodawca nie miał konta albo pieniędzy.
    pub failed: u32,
    /// Suma brutto wypłat, które doszły.
    pub gross: Money,
    /// Suma netto, czyli to, co faktycznie weszło do gospodarstw.
    pub net: Money,
    /// Suma zaliczek PIT potrąconych u źródła.
    pub withheld: Money,
}

/// Konsument [`PayrollOutbox`] — jedyna droga, którą płaca wychodzi od pracodawcy.
///
/// Stoi obok `absorb_settlements` i z tego samego powodu: skrzynka pochodzi
/// z `sim/firms`, a księgowanie należy do `sim/economy`. Kadencja jest minutowa,
/// ale `run_payroll` produkuje wyłącznie o północy, więc prawie zawsze kończy się
/// na sprawdzeniu, czy skrzynka jest pusta.
pub fn absorb_payroll(world: &mut World, market: &Market, t: Tick) -> PayrollDay {
    let mut dzien = PayrollDay::default();
    let Some(run) = world
        .get_resource_mut::<magnat_firms::PayrollOutbox>()
        .map(magnat_firms::PayrollOutbox::take)
    else {
        return dzien;
    };
    if run.is_empty() {
        return dzien;
    }
    // Kolejność wpisów jest ustalona po `(FirmKey, SiteId, CitizenId)` po stronie
    // `sim/firms`, więc kolejność przelewów nie zależy od archetypów ECS (00 §3.2).
    for item in &run.items {
        // Konto **zakładu**, a nie firmy: `FirmKey` jest licznikiem świata, a nie
        // encją, więc nie da się z niego zrobić `FirmId`. Zakład bez konta w rynku
        // (placówka miejska — patrz nagłówek) nie wypłaca i liczy się jako nieudana
        // wypłata, żeby brak był widoczny w raporcie, a nie cichy.
        let Some(konto) = market.account_of(item.site) else {
            dzien.failed += 1;
            continue;
        };
        let Some(gospodarstwo) = world
            .get::<Identity>(item.citizen.0)
            .map(|id| id.household)
            .and_then(|idx| magnat_agents::demography::household_by_index(world, idx))
        else {
            dzien.failed += 1;
            continue;
        };
        let brutto = item.gross;
        if brutto.get() <= 0 {
            continue;
        }
        // Zaliczka liczy się od **brutto** i tym samym hakiem, którym liczyło ją
        // `pay_incomes`: gospodarstwo dostaje netto, bo z tego, co dostanie, zaraz
        // planuje koperty.
        let zaliczka = market.withhold(gospodarstwo.index(), brutto);
        let netto = Money(brutto.get() - zaliczka.get() - item.deductions.get());
        if netto.get() <= 0 {
            continue;
        }
        let memo = TxMemo::new(
            TxKind::Wage { site: item.site },
            DecisionReason::Unspecified,
        );
        let ok = world
            .get_resource_mut::<Books>()
            .map(|b| b.household_receive(konto, netto, memo, t).is_ok())
            .unwrap_or(false);
        if !ok {
            // Pracodawca bez pokrycia nie wypłaca — i to jest poprawny stan świata,
            // a nie błąd. Zobowiązanie wobec pracownika obsługuje M7 (`WagePayable`),
            // a nie skrzynka: kwota, której nie ma, nie może wejść do gospodarstwa.
            dzien.failed += 1;
            continue;
        }
        if let Some(h) = world.get_mut::<Household>(gospodarstwo) {
            h.bank = Money(h.bank.get().saturating_add(netto.get()));
        }
        // Licznik miesięczny brutto — `pay_incomes` dopłaca tylko to, czego żaden
        // pracodawca nie pokrył (emerytury, świadczenia, świat bez rejestru firm).
        market.record_wage_paid(gospodarstwo.index(), brutto);
        dzien.paid += 1;
        dzien.gross = Money(dzien.gross.get() + brutto.get());
        dzien.net = Money(dzien.net.get() + netto.get());
        dzien.withheld = Money(dzien.withheld.get() + zaliczka.get());
    }
    dzien
}
