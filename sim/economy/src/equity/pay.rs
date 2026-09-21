//! Kto komu płaci na sesji giełdowej (M10d WP10.10).
//!
//! Osobny plik od [`super::system`], bo to jest **drugi temat w tym samym kroku**:
//! tamten mówi, co się w dobie dzieje, a ten — jak pieniądz przechodzi granicę między
//! księgami a komponentami. Ta granica ma dwie strony i cztery przypadki, a pomylenie
//! którejkolwiek połowy znaczy pieniądz zgubiony między księgami a światem.
//!
//! # Konto przelotowe
//!
//! Wszystkie przelewy sesji idą **przez konto reszty świata** i to jest świadome:
//! aukcja ma jedną cenę, więc suma wpłat równa się sumie wypłat co do grosza, a konto
//! przelotowe domyka się w tej samej dobie. Alternatywa — przelew wprost od kupującego
//! do sprzedającego — wymagałaby parowania zleceń, którego fixing z definicji nie robi,
//! a dla pary „gospodarstwo → gospodarstwo" nie zostawiłaby w dzienniku ani jednego
//! zapisu, bo gospodarstwo **nie ma konta w księgach** (M5b).

use magnat_agents::{Household, Identity, Population};
use magnat_core::{CitizenId, DecisionReason, Entity, FirmReason, Money, Tick};
use magnat_ecs::World;
use magnat_firms::{firm_id, FirmKey, Owner};

use crate::books::{AccountId, Books, TxKind, TxMemo};
use crate::market::Market;

/// Saldo konta, jeśli jest.
#[must_use]
pub fn saldo(world: &World, konto: Option<AccountId>) -> Money {
    let (Some(books), Some(a)) = (world.get_resource::<Books>(), konto) else {
        return Money::ZERO;
    };
    books.account(a).map_or(Money::ZERO, |acc| acc.balance())
}

/// Opis zapisu obrotu udziałami.
///
/// Powód niesie cenę zerową z rozmysłu: cena sesji jest w [`super::Listing`]
/// i w powodzie `StockFixing` dopisanym do dziennika firmy, a dublowanie jej
/// w każdym z kilkuset przelewów doby byłoby drugą prawdą o tej samej liczbie.
#[must_use]
pub fn memo(key: FirmKey, bp: u16) -> TxMemo {
    TxMemo::new(
        TxKind::ShareTrade {
            firm: firm_id(key),
            bp,
        },
        DecisionReason::Firm(FirmReason::StockFixing {
            firm: firm_id(key),
            price: Money::ZERO,
        }),
    )
}

/// Strona przelewu sesyjnego: wszystko poza tym, kto i ile.
///
/// Struktura, a nie osiem argumentów — bo cztery z nich (rynek, konto przelotowe,
/// spółka, tick) są **takie same dla całej sesji**, a tylko dwa zmieniają się
/// z przydziału na przydział.
#[derive(Clone, Copy)]
pub struct Rozliczenie<'a> {
    pub market: &'a Market,
    pub row: AccountId,
    pub firm: FirmKey,
    pub t: Tick,
}

/// Kupujący płaci na konto przelotowe.
///
/// Zwraca `false`, gdy nie miał z czego — wtedy jego przydział przepada i udziału
/// nie dostaje. Gospodarstwu, któremu zabrakło, zwraca się to, co zdążyło zapłacić:
/// pakietu o zadanej wielkości nie da się kupić w połowie.
pub fn zaplac(world: &mut World, r: Rozliczenie<'_>, kto: Owner, kwota: Money, bp: u16) -> bool {
    let (market, row, key, t) = (r.market, r.row, r.firm, r.t);
    if kwota.get() <= 0 {
        return false;
    }
    match kto {
        Owner::Firm(k) => {
            let Some(konto) = market.account_of_firm(firm_id(k)) else {
                return false;
            };
            let Some(books) = world.get_resource_mut::<Books>() else {
                return false;
            };
            books.transfer(konto, row, kwota, memo(key, bp), t).is_ok()
        }
        Owner::Citizen(c) => {
            let zdjete = zdejmij_z_gospodarstwa(world, c, kwota);
            if zdjete.pobrane().get() < kwota.get() {
                oddaj_gospodarstwu(world, c, zdjete);
                return false;
            }
            let ok = world
                .get_resource_mut::<Books>()
                .is_some_and(|b| b.household_pay(row, kwota, memo(key, bp), t).is_ok());
            if !ok {
                // **Zwrot, a nie milczenie.** Zdjęcie z komponentu już się stało,
                // więc odmowa po stronie ksiąg bez zwrotu niszczyłaby pieniądz
                // poza księgami — a `Books::check_conservation` tego nie widzi,
                // bo suma sald się zgadza. To jest znalezisko recenzji M10d.
                oddaj_gospodarstwu(world, c, zdjete);
            }
            ok
        }
        Owner::Player => {
            player_citizen(world).is_some_and(|c| zaplac(world, r, Owner::Citizen(c), kwota, bp))
        }
        // Miasto i sieć zewnętrzna nie mają w tej grze konta udziałowca. Sufit
        // nazwany: ich zlecenia nie powstają, więc ta gałąź nie jest martwa —
        // jest odmową dla wołającego, który by ją pominął.
        Owner::City | Owner::External => false,
    }
}

/// Sprzedający odbiera z konta przelotowego.
pub fn odbierz(world: &mut World, r: Rozliczenie<'_>, kto: Owner, kwota: Money, bp: u16) {
    let (market, row, key, t) = (r.market, r.row, r.firm, r.t);
    if kwota.get() <= 0 {
        return;
    }
    match kto {
        Owner::Firm(k) => {
            let Some(konto) = market.account_of_firm(firm_id(k)) else {
                return;
            };
            if let Some(books) = world.get_resource_mut::<Books>() {
                let _ = books.transfer(row, konto, kwota, memo(key, bp), t);
            }
        }
        Owner::Citizen(c) => {
            // **Najpierw sprawdzamy, czy jest komu oddać.** `household_receive`
            // zdejmuje kwotę z ksiąg i podnosi `household_sector_out`, więc wołanie
            // go dla mieszkańca bez gospodarstwa (zmarły z pakietem) wypuszczałoby
            // pieniądz z ksiąg donikąd. Znalezisko recenzji M10d.
            if gospodarstwo(world, c).is_none() {
                return;
            }
            let ok = world
                .get_resource_mut::<Books>()
                .is_some_and(|b| b.household_receive(row, kwota, memo(key, bp), t).is_ok());
            if ok {
                dodaj_do_gospodarstwa(world, c, kwota);
            }
        }
        Owner::Player => {
            if let Some(c) = player_citizen(world) {
                odbierz(world, r, Owner::Citizen(c), kwota, bp);
            }
        }
        // Sprzedaż ze strony miasta i sieci zewnętrznej: pieniądz zostaje na koncie
        // przelotowym i **nie znika**, bo `RestOfWorld` jest kontem w księgach.
        // Tą drogą wychodzi na rynek pakiet założycielski przy debiucie.
        Owner::City | Owner::External => {}
    }
}

/// Mieszkaniec-gracz.
///
/// `Owner::Player` nie niesie encji, a pieniądz gracza leży w jego gospodarstwie
/// jak każdy inny — bez tego wyszukania gracz kupowałby udziały za nic.
#[must_use]
pub fn player_citizen(world: &World) -> Option<CitizenId> {
    let pop = world.get_resource::<Population>()?;
    pop.citizens()
        .iter()
        .find(|e| {
            world
                .get::<Identity>(**e)
                .is_some_and(|i| i.flags & Identity::FLAG_PLAYER != 0)
        })
        .map(|e| CitizenId(*e))
}

/// Skąd wzięto pieniądz gospodarstwa — żeby zwrot trafił tam, skąd wyszedł.
///
/// Jedna liczba by nie wystarczyła: zwrot w całości do oszczędności teleportowałby
/// pieniądz z rachunku na lokatę przy każdej nieudanej próbie zakupu.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Zrodlo {
    pub z_oszczednosci: Money,
    pub z_banku: Money,
}

impl Zrodlo {
    #[must_use]
    pub fn pobrane(self) -> Money {
        Money(self.z_oszczednosci.get() + self.z_banku.get())
    }
}

/// Zdejmuje kwotę z gospodarstwa: najpierw z oszczędności, potem z rachunku.
pub fn zdejmij_z_gospodarstwa(world: &mut World, c: CitizenId, kwota: Money) -> Zrodlo {
    let Some(h) = gospodarstwo(world, c) else {
        return Zrodlo::default();
    };
    let Some(gd) = world.get_mut::<Household>(h) else {
        return Zrodlo::default();
    };
    let mut zostalo = kwota.get();
    let z_oszczednosci = zostalo.min(gd.savings.get().max(0));
    gd.savings = Money(gd.savings.get() - z_oszczednosci);
    zostalo -= z_oszczednosci;
    let z_banku = zostalo.min(gd.bank.get().max(0));
    gd.bank = Money(gd.bank.get() - z_banku);
    Zrodlo {
        z_oszczednosci: Money(z_oszczednosci),
        z_banku: Money(z_banku),
    }
}

/// Oddaje gospodarstwu dokładnie to, co i skąd zdjęto.
pub fn oddaj_gospodarstwu(world: &mut World, c: CitizenId, z: Zrodlo) {
    let Some(h) = gospodarstwo(world, c) else {
        return;
    };
    if let Some(gd) = world.get_mut::<Household>(h) {
        gd.savings = Money(gd.savings.get() + z.z_oszczednosci.get());
        gd.bank = Money(gd.bank.get() + z.z_banku.get());
    }
}

/// Dopisuje kwotę do oszczędności gospodarstwa.
pub fn dodaj_do_gospodarstwa(world: &mut World, c: CitizenId, kwota: Money) {
    let Some(h) = gospodarstwo(world, c) else {
        return;
    };
    if let Some(gd) = world.get_mut::<Household>(h) {
        gd.savings = Money(gd.savings.get() + kwota.get());
    }
}

/// Encja gospodarstwa mieszkańca.
#[must_use]
pub fn gospodarstwo(world: &World, c: CitizenId) -> Option<Entity> {
    let id = world.get::<Identity>(c.0)?;
    magnat_agents::demography::household_by_index(world, id.household)
}
