//! Miesiąc giełdy: dywidenda i emisja obronna (M10d WP10.11).
//!
//! Osobny plik od [`super::system`], bo to jest **inna kadencja i inny temat**: tam
//! dzieje się doba (publikacja, zlecenia, fixing, rozliczenie), tutaj granica miesiąca.
//! Obie rzeczy tutaj są też jedynymi w tym module, które **ruszają akcjonariat poza
//! sesją** — i dlatego obie muszą odświeżyć pamięć progów, czego sesja robić nie musi.

use magnat_core::{CitizenId, DecisionReason, FirmReason, Money, Tick};
use magnat_ecs::World;
use magnat_firms::{firm_id, FirmKey, Firms, Owner};

use crate::books::{Books, TxKind, TxMemo};
use crate::market::Market;

use super::corp;
use super::pay::{self, dodaj_do_gospodarstwa, player_citizen, saldo, zaplac};
use super::{Equity, CONTROL_BP};

/// Wypłata dywidendy na granicy miesiąca.
pub(crate) fn dywidendy(world: &mut World, market: &Market, eq: &mut Equity, t: Tick) -> Money {
    let payout = eq.params().payout_bp;
    let klucze: Vec<FirmKey> = eq.listings().map(|l| l.firm).collect();
    let mut razem = Money::ZERO;
    for key in klucze {
        let Some(p) = eq.published(key).copied() else {
            continue;
        };
        let Some(konto) = market.account_of_firm(firm_id(key)) else {
            continue;
        };
        let gotowka = saldo(world, Some(konto));
        // Miesięczny zysk z rocznego: dzielimy, bo dywidendę uchwala się co miesiąc,
        // a opublikowany jest wynik dwunastu miesięcy.
        let kwota = corp::dividend_amount(Money(p.profit_12m.get() / 12), gotowka, payout);
        if kwota.get() <= 0 {
            continue;
        }
        let podzial = {
            let Some(firms) = world.get_resource::<Firms>() else {
                continue;
            };
            let Some(firm) = firms.get(key) else { continue };
            super::split_dividend(firm, kwota)
        };
        let memo = TxMemo::new(
            TxKind::Dividend { firm: firm_id(key) },
            DecisionReason::Firm(FirmReason::DividendPaid {
                firm: firm_id(key),
                total: kwota,
            }),
        );
        let mut wyplacone = Money::ZERO;
        for (o, m) in podzial {
            if m.get() <= 0 {
                continue;
            }
            match o {
                Owner::Firm(k) => {
                    let Some(cel) = market.account_of_firm(firm_id(k)) else {
                        continue;
                    };
                    if world
                        .get_resource_mut::<Books>()
                        .is_some_and(|b| b.transfer(konto, cel, m, memo, t).is_ok())
                    {
                        wyplacone = Money(wyplacone.get() + m.get());
                    }
                }
                Owner::Citizen(c) => {
                    wyplacone =
                        Money(wyplacone.get() + dywidenda_do_gd(world, c, konto, m, memo, t).get());
                }
                Owner::Player => {
                    if let Some(c) = player_citizen(world) {
                        wyplacone = Money(
                            wyplacone.get() + dywidenda_do_gd(world, c, konto, m, memo, t).get(),
                        );
                    }
                }
                // Miasto i sieć zewnętrzna nie mają w tej grze konta udziałowca,
                // więc ich część zostaje w firmie. Sufit nazwany, nie zgubiony grosz:
                // pieniądz nie wychodzi z ksiąg.
                Owner::City | Owner::External => {}
            }
        }
        if wyplacone.get() > 0 {
            if let Some(firms) = world.get_resource_mut::<Firms>() {
                firms.log(
                    key,
                    t,
                    DecisionReason::Firm(FirmReason::DividendPaid {
                        firm: firm_id(key),
                        total: wyplacone,
                    }),
                );
            }
            razem = Money(razem.get() + wyplacone.get());
        }
    }
    razem
}

/// Wypłata dywidendy mieszkańcowi przez granicę sektora gospodarstw.
///
/// **Gospodarstwo sprawdza się przed przelewem**, a nie po nim: `household_receive`
/// zdejmuje kwotę z ksiąg i podnosi `household_sector_out`, więc wołanie go dla
/// mieszkańca bez gospodarstwa wypuszczałoby pieniądz z ksiąg donikąd. Znalezisko
/// recenzji M10d — to ten sam błąd, co przy rozliczeniu sesji.
fn dywidenda_do_gd(
    world: &mut World,
    c: CitizenId,
    konto: crate::books::AccountId,
    kwota: Money,
    memo: TxMemo,
    t: Tick,
) -> Money {
    if super::pay::gospodarstwo(world, c).is_none() {
        return Money::ZERO;
    }
    let ok = world
        .get_resource_mut::<Books>()
        .is_some_and(|b| b.household_receive(konto, kwota, memo, t).is_ok());
    if !ok {
        return Money::ZERO;
    }
    dodaj_do_gospodarstwa(world, c, kwota);
    kwota
}

/// Emisja obronna: notowana firma, nad którą ktoś obcy zbliżył się do kontroli,
/// dosypuje udziału największemu właścicielowi — jeśli ten ma czym zapłacić.
///
/// To jest odpowiedź na „wrogie przejęcie da się **obronić**" z wyniku podfazy
/// i zarazem jedyny wołający emisji w symulacji. Pieniądz idzie **do firmy**
/// i to jest cała różnica wobec debiutu, gdzie trafia do sprzedających.
pub(crate) fn emisje(world: &mut World, market: &Market, eq: &mut Equity, t: Tick) -> u32 {
    let klucze: Vec<FirmKey> = eq.listings().map(|l| l.firm).collect();
    let mut ile = 0;
    for key in klucze {
        let Some(l) = eq.listing(key).copied() else {
            continue;
        };
        let Some((broniacy, broniacy_bp)) = zagrozenie(world, key) else {
            continue;
        };
        let bp = corp::defence_issue_bp(broniacy_bp);
        if bp == 0 {
            continue;
        }
        let cena = corp::issue_price(bp, l.last_fixing);
        let Some(konto) = market.account_of_firm(firm_id(key)) else {
            continue;
        };
        let memo = TxMemo::new(
            TxKind::ShareIssue {
                firm: firm_id(key),
                bp,
            },
            DecisionReason::Firm(FirmReason::SharesIssued {
                bp,
                price: l.last_fixing,
            }),
        );
        let row = market.rest_of_world();
        let rozliczenie = pay::Rozliczenie {
            market,
            row,
            firm: key,
            t,
        };
        // **Emisja najpierw, przelew potem.** Odwrotna kolejność zostawiałaby
        // broniącego bez pieniędzy i bez udziału, gdyby `issue` odmówił — a odmawia
        // wtedy, gdy firmy nie ma w rejestrze albo wielkość emisji wypadła z widełek.
        // Ten sam porządek, którym `magnat_media` księguje koszt przed przelewem.
        let wydane = world
            .get_resource_mut::<Firms>()
            .is_some_and(|firms| corp::issue(firms, key, broniacy, bp, l.last_fixing, t));
        if !wydane {
            continue;
        }
        if !zaplac(world, rozliczenie, broniacy, cena, bp) {
            continue;
        }
        let ok = world
            .get_resource_mut::<Books>()
            .is_some_and(|b| b.transfer(row, konto, cena, memo, t).is_ok());
        if !ok {
            continue;
        }
        // Emisja przestawiła bp **wszystkim** właścicielom, a nie tylko dwóm stronom
        // sesji, więc pamięć progów trzeba odświeżyć w całości. Bez tego `przed`
        // napastnika zostaje trwale zawyżony i próg 5 %/50 % nie odpali się dla niego
        // już nigdy — znalezisko recenzji M10d.
        odswiez_progi(world, eq, key);
        ile += 1;
    }
    ile
}

/// Przepisuje pamięć progów akcjonariatu firmy od nowa.
///
/// Wołać zawsze, gdy udział zmienił się **poza sesją** — czyli po emisji. Sesja
/// aktualizuje pamięć sama, ale tylko dla stron transakcji.
fn odswiez_progi(world: &World, eq: &mut Equity, key: FirmKey) {
    let Some(firms) = world.get_resource::<Firms>() else {
        return;
    };
    let Some(firm) = firms.get(key) else {
        return;
    };
    let udzialy: Vec<(Owner, u16)> = firm.owners.iter().map(|s| (s.owner, s.bp)).collect();
    for (o, bp) in udzialy {
        eq.remember_stake(key, o, bp);
    }
}

/// Kto broni i ile ma — `None`, gdy nikt nie zagraża.
///
/// Zagrożeniem jest **cudzy** pakiet powyżej połowy progu kontroli, czyli 25 %:
/// niżej przejęcie jest jeszcze odległe, a emisja obronna byłaby rozwadnianiem
/// własnych właścicieli bez powodu.
fn zagrozenie(world: &World, key: FirmKey) -> Option<(Owner, u16)> {
    let firms = world.get_resource::<Firms>()?;
    let firm = firms.get(key)?;
    let broniacy = corp::preemptive_holder(firm)?;
    let napastnik = firm
        .owners
        .iter()
        .filter(|s| s.owner != broniacy)
        .max_by_key(|s| s.bp)?;
    if u32::from(napastnik.bp) * 2 <= CONTROL_BP {
        return None;
    }
    let moje = super::stake_of(firm, broniacy);
    if u32::from(moje) > CONTROL_BP {
        return None;
    }
    Some((broniacy, moje))
}
