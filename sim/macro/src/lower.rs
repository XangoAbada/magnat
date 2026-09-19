//! `lower()` — rozwinięcie agregatu z powrotem do jednostek (M10a §5.8, WP10.1).
//!
//! To jest druga strona [`crate::lift`] i sedno techniczne podfazy. `lift()`
//! sumuje, `lower()` dzieli — i dzieli tak, żeby suma wróciła **co do grosza**.
//! Nie „w przybliżeniu", nie „z tolerancją": dokładnie. Gwarancję daje mechanizm,
//! nie staranność — każda kwota rozchodzi się przez [`magnat_core::split_proportional`],
//! które oddaje resztę pierwszej stronie wg posortowanego klucza (00 §2).
//!
//! # Podział na rdzeń i sterownik
//!
//! [`lower_cell`] jest **funkcją czystą**: bierze komórkę, ziarno i tick, oddaje
//! listę stanów indywidualnych. Nie widzi `World`, nie ma stanu ukrytego i nie
//! zależy od kolejności wywołań. To jest wymaganie M12 (`K4` w M10 §7.3): tam,
//! gdzie replay i zapis mogą się rozjechać, musi stać funkcja, którą da się
//! wywołać dwa razy i porównać bitowo.
//!
//! [`lower`] jest **sterownikiem**: woła rdzeń per komórka i nanosi wynik na świat.
//! Cała wiedza o ECS jest tutaj, cała arytmetyka podziału — tam.
//!
//! # Czego `lower()` nie odtwarza i dlaczego to nie jest brak
//!
//! Umiejętności, potrzeby i wiek pojedynczego mieszkańca **nie wracają z makra**,
//! bo makro ich nie zmienia: żadna faza kroku nie rusza `skill_sum` ani `need_sat`
//! per osobę, więc zapis tych liczb z powrotem byłby przepisaniem tego, co `lift()`
//! przed chwilą przeczytał. Wraca to, co model faktycznie przeliczył: **pieniądz**
//! gospodarstw i firm. Kiedy R&D (M10c) zacznie ruszać umiejętnościami, wrócą
//! razem z nim — i wtedy będzie to zapis czegoś, co się zmieniło.
//!
//! Zatrudnienie też nie wraca, i to jest cena **jawna**: makro prowadzi liczbę
//! zatrudnionych w komórce, a nie przypisanie człowieka do zakładu. Odtworzenie
//! tego drugiego z pierwszego wymagałoby rynku pracy, czyli `sim/economy::labor` —
//! a ten stoi **pod** `sim/macro` i nie ma go czym zawołać (`K-54`).

use std::collections::BTreeMap;

use magnat_agents::{Household, Identity, Population};
use magnat_core::{split_proportional, DecisionReason, DistrictId, Entity, Money, StreamId, Tick};
use magnat_economy::{AccountId, Books, Market, TxKind, TxMemo};
use magnat_ecs::World;

use crate::state::{MacroCell, MacroState};
use crate::types::ClassId;

/// Stan pieniężny jednego mieszkańca po rozwinięciu komórki.
///
/// Trzy liczby i tożsamość — nic więcej, bo nic więcej makro nie policzyło.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PersonState {
    pub birth_index: u32,
    pub cash: Money,
    pub deposits: Money,
    pub debt: Money,
}

/// Wynik rozwinięcia jednej komórki. Suma każdej kolumny jest równa agregatowi
/// komórki **co do grosza** — i to jest cała umowa tego typu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellExpansion {
    pub key: (DistrictId, ClassId),
    pub people: Vec<PersonState>,
}

impl CellExpansion {
    /// Suma gotówki — lewa strona testu własnościowego `lift(lower(s)) == s`.
    #[must_use]
    pub fn cash(&self) -> Money {
        Money(
            self.people
                .iter()
                .map(|p| p.cash.get())
                .fold(0i64, i64::saturating_add),
        )
    }

    #[must_use]
    pub fn deposits(&self) -> Money {
        Money(
            self.people
                .iter()
                .map(|p| p.deposits.get())
                .fold(0i64, i64::saturating_add),
        )
    }

    #[must_use]
    pub fn debt(&self) -> Money {
        Money(
            self.people
                .iter()
                .map(|p| p.debt.get())
                .fold(0i64, i64::saturating_add),
        )
    }
}

/// Numer wielkości w kluczu porządku rangowego. §5.8 wymaga, żeby porządek był
/// funkcją `(ziarno, strumień, tożsamość, wielkość)` — inaczej gotówka, depozyt
/// i dług rozkładałyby się na tych samych ludziach i majątek wychodziłby
/// skrajnie skoncentrowany.
#[derive(Clone, Copy)]
enum Wielkosc {
    Cash = 0,
    Deposits = 1,
    Debt = 2,
}

/// Ile wielkości rozdziela `lower_cell` — mnożnik klucza ticku, żeby porządki
/// rangowe różnych wielkości nie zlewały się w jeden.
const WIELKOSCI: u64 = 3;

/// Rozwinięcie komórki do jednostek. **Funkcja czysta** (M10 §7.3, `K4`).
///
/// Przebieg jest ten sam dla każdej wielkości i ma cztery kroki z §5.8:
/// 1. porządek rangowy z `hash(seed, MacroLower, birth_index, wielkość)`,
/// 2. wagi z profilu kwantylowego komórki dopasowane do pozycji w rankingu,
/// 3. podział kwoty proporcjonalnie do wag,
/// 4. reszta do pierwszego wg posortowanego klucza — w `split_proportional`.
///
/// **Zero arytmetyki zmiennoprzecinkowej.** Plan mówił o odwrotnej dystrybuancie
/// log-normalnej; tu jest profil kwantylowy w liczbach całkowitych i to jest
/// świadoma zamiana, nie uproszczenie z lenistwa: kwantyle są tym, co komórka
/// **zna** (`wealth_q`), a dopasowanie rozkładu dwuparametrowego do czterech
/// kwantyli i tak sprowadza się do interpolacji między nimi. Float w drodze do
/// kwoty pieniężnej jest zabroniony (00 §2) nawet przejściowo, a tutaj nie jest
/// do niczego potrzebny.
#[must_use]
pub fn lower_cell(cell: &MacroCell, seed: u64, tick: Tick) -> CellExpansion {
    let n = cell.citizens.len();
    let mut people: Vec<PersonState> = cell
        .citizens
        .iter()
        .map(|c| PersonState {
            birth_index: c.birth_index,
            cash: Money::ZERO,
            deposits: Money::ZERO,
            debt: Money::ZERO,
        })
        .collect();
    if n == 0 {
        return CellExpansion {
            key: cell.key,
            people,
        };
    }

    let profil = profil_kwantylowy(cell);
    for (w, kwota) in [
        (Wielkosc::Cash, cell.cash),
        (Wielkosc::Deposits, cell.deposits),
        (Wielkosc::Debt, cell.debt),
    ] {
        let porzadek = porzadek_rangowy(&people, seed, tick, w);
        let wagi: Vec<u64> = (0..n).map(|ranga| profil.waga(ranga, n)).collect();
        for (ranga, udzial) in split_proportional(kwota, &wagi).into_iter().enumerate() {
            let osoba = &mut people[porzadek[ranga]];
            match w {
                Wielkosc::Cash => osoba.cash = udzial,
                Wielkosc::Deposits => osoba.deposits = udzial,
                Wielkosc::Debt => osoba.debt = udzial,
            }
        }
    }

    CellExpansion {
        key: cell.key,
        people,
    }
}

/// Indeksy osób w kolejności rangowej: rosnąco po kluczu mieszającym, a przy
/// równym kluczu po `birth_index`.
///
/// Klucz jest funkcją tożsamości, **nie** pozycji w wektorze — dzięki temu
/// kolejność zapełniania komórki nie zmienia wyniku, a `lower()` wykonane
/// dwukrotnie daje ten sam świat (00 §4 pkt 4).
fn porzadek_rangowy(people: &[PersonState], seed: u64, tick: Tick, w: Wielkosc) -> Vec<usize> {
    let klucz_ticku = Tick(tick.get().wrapping_mul(WIELKOSCI).wrapping_add(w as u64));
    let mut klucze: Vec<(u64, u32, usize)> = people
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let mut r = magnat_core::rng(seed, StreamId::MacroLower, p.birth_index, klucz_ticku);
            (r.next_u64(), p.birth_index, i)
        })
        .collect();
    klucze.sort_unstable();
    klucze.into_iter().map(|(_, _, i)| i).collect()
}

/// Profil kwantylowy komórki sprowadzony do wag podziału.
///
/// `wealth_q` niesie cztery punkty (min, q1, q3, max). Waga rangi bierze się
/// z interpolacji liniowej między nimi po pozycji w rankingu — jedna ćwiartka
/// na odcinek min→q1 i q3→max, połowa na q1→q3. Wagi są **przesunięte o jeden
/// w górę**, żeby komórka o zerowym majątku nie dostała samych zer: wtedy
/// `split_proportional` wrzuciłoby całą kwotę pierwszej osobie, czyli zrobiło
/// z egalitarnej komórki jednego milionera.
struct Profil {
    punkty: [i64; 4],
}

impl Profil {
    fn waga(&self, ranga: usize, n: usize) -> u64 {
        if n <= 1 {
            return 1;
        }
        // Pozycja w rankingu w promilach, 0..=1000.
        let p = (ranga as u64 * 1_000) / (n as u64 - 1);
        let [min, q1, q3, max] = self.punkty;
        let v = match p {
            0..=250 => interpoluj(min, q1, p, 0, 250),
            251..=750 => interpoluj(q1, q3, p, 250, 750),
            _ => interpoluj(q3, max, p, 750, 1_000),
        };
        u64::try_from(v.max(0)).unwrap_or(0).saturating_add(1)
    }
}

/// Interpolacja liniowa w liczbach całkowitych między `a` (przy `p == lo`)
/// i `b` (przy `p == hi`).
fn interpoluj(a: i64, b: i64, p: u64, lo: u64, hi: u64) -> i64 {
    let rozpietosc = i128::from(hi - lo).max(1);
    let krok = i128::from(p - lo);
    let wynik = i128::from(a) + (i128::from(b) - i128::from(a)) * krok / rozpietosc;
    i64::try_from(wynik).unwrap_or(i64::MAX)
}

fn profil_kwantylowy(cell: &MacroCell) -> Profil {
    let mut punkty = [
        cell.wealth_q[0].get(),
        cell.wealth_q[1].get(),
        cell.wealth_q[2].get(),
        cell.wealth_q[3].get(),
    ];
    // Kwantyle muszą być niemalejące. Zdjęcie z M7 wpisuje w nie cztery razy tę samą
    // liczbę (jawna zaślepka w `lift`), a stan zbudowany ręcznie w teście może mieć
    // dowolne; profil ma być monotoniczny niezależnie od tego, co dostanie.
    for i in 1..punkty.len() {
        punkty[i] = punkty[i].max(punkty[i - 1]);
    }
    Profil { punkty }
}

// ── sterownik: nanoszenie na świat ───────────────────────────────────────────────

/// Co `lower()` naniósł i czego nie. Nie jest to lista błędów programu, tylko
/// **opis rozjazdu między makrem a światem** — dlatego jest raportem, a nie paniką.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LowerReport {
    /// Ile gospodarstw dostało nowy stan pieniężny.
    pub households: u32,
    /// Ile firm dostało nowe saldo rachunku.
    pub firms: u32,
    /// Ilu mieszkańców z makra nie ma w świecie. Po M10a jest to zero — populacja
    /// jest zamknięta (`step::demography`) — i dlatego liczba stoi w raporcie:
    /// gdy przestanie być zerem, będzie to znaczyło, że ktoś dołożył przyrost
    /// naturalny bez dołożenia tworzenia encji.
    pub missing_citizens: u32,
    /// Ile gospodarstw ma członków w **więcej niż jednej** komórce. Dla nich
    /// równość per komórka nie zachodzi (patrz [`lower`]), choć suma nadal tak.
    pub straddling_households: u32,
    /// Kwota, której nie dało się przesunąć — rachunek reszty świata odmówił
    /// przelewu albo firma nie ma konta. Zero znaczy „naniesione w całości".
    pub unsettled: Money,
}

/// Dlaczego rozwinięcie w ogóle nie ruszyło.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LowerError {
    /// Świat bez spisu ludności — nie ma na co nanosić.
    NoPopulation,
    /// Świat bez rynku albo bez ksiąg: pieniądz firm nie ma gdzie wylądować,
    /// a naniesienie samych gospodarstw zmieniłoby sumę pieniądza w świecie.
    NoBooks,
}

/// Nanosi stan makro na świat, **nie zmieniając sumy pieniądza w świecie**.
///
/// Mechanizm zachowania jest jeden i cały mieści się w dwóch zdaniach. Pieniądz
/// firmy siedzi na rachunku w `Books`, więc zmiana idzie przelewem z rachunku
/// **reszty świata** — suma sald się nie rusza. Pieniądz gospodarstwa siedzi
/// w komponencie, czyli poza księgami, więc jego zmiana idzie kanałem sektora
/// gospodarstw (`Books::household_pay` / `household_receive`), który jest w `Books`
/// policzony po to właśnie: żeby `check_conservation` widziało obie strony.
/// Bez tego drugiego kroku makro **stworzyłoby pieniądz** dokładnie o różnicę
/// stanu gospodarstw — a 00 §4 pkt 1 zabrania tego z tolerancją zero.
///
/// # Równość per komórka i jej jeden warunek
///
/// `lift()` dzieli pieniądz gospodarstwa **równo między komórki jego członków**.
/// Gospodarstwo, którego członkowie należą do dwóch komórek (ten sam dom, różne
/// klasy społeczne), wraca więc do makra inaczej, niż z niego wyszło — suma jest
/// ta sama, podział między dwie komórki inny. Takich gospodarstw jest w praktyce
/// garść i raport je **liczy**, zamiast milczeć: `straddling_households`.
pub fn lower(state: &MacroState, world: &mut World, seed: u64) -> Result<LowerReport, LowerError> {
    let (encje, gospodarstwa) = {
        let pop = world
            .get_resource::<Population>()
            .ok_or(LowerError::NoPopulation)?;
        let encje: BTreeMap<u32, Entity> = pop.citizens().iter().map(|e| (e.index(), *e)).collect();
        let gd: BTreeMap<u32, Entity> = pop.households().iter().map(|e| (e.index(), *e)).collect();
        (encje, gd)
    };
    let (rest, konta_firm) = {
        let market = world.get_resource::<Market>().ok_or(LowerError::NoBooks)?;
        let konta: Vec<(usize, AccountId)> = state
            .firms
            .iter()
            .enumerate()
            .filter_map(|(i, f)| market.account_of_firm(f.id).map(|a| (i, a)))
            .collect();
        (market.rest_of_world(), konta)
    };
    if world.get_resource::<Books>().is_none() {
        return Err(LowerError::NoBooks);
    }

    let mut report = LowerReport::default();
    let tick = state.tick;

    // ── 1. rozwinięcie komórek i złożenie w gospodarstwa ─────────────────────
    // Rozwinięcie liczy się per osoba, a zapisuje per gospodarstwo: pieniądz
    // w świecie ma gospodarstwo, nie osoba (M3c §5.2). Kolejność sumowania jest
    // kolejnością komórek i rang, czyli deterministyczna (00 §2).
    let mut na_gd: BTreeMap<u32, (i64, i64, i64)> = BTreeMap::new();
    let mut komorki_gd: BTreeMap<u32, (usize, bool)> = BTreeMap::new();

    for (ci, cell) in state.cells.iter().enumerate() {
        let rozwiniecie = lower_cell(cell, seed, tick);
        for osoba in &rozwiniecie.people {
            let Some(entity) = encje.get(&osoba.birth_index) else {
                report.missing_citizens = report.missing_citizens.saturating_add(1);
                continue;
            };
            let Some(id) = world.get::<Identity>(*entity) else {
                report.missing_citizens = report.missing_citizens.saturating_add(1);
                continue;
            };
            let gd = id.household;
            let wpis = na_gd.entry(gd).or_insert((0, 0, 0));
            wpis.0 = wpis.0.saturating_add(osoba.cash.get());
            wpis.1 = wpis.1.saturating_add(osoba.deposits.get());
            wpis.2 = wpis.2.saturating_add(osoba.debt.get());
            let slad = komorki_gd.entry(gd).or_insert((ci, false));
            if slad.0 != ci {
                slad.1 = true;
            }
        }
    }
    report.straddling_households = komorki_gd
        .values()
        .filter(|(_, rozkraczone)| *rozkraczone)
        .count()
        .try_into()
        .unwrap_or(u32::MAX);

    // ── 2. zapis gospodarstw i zebranie różnicy ──────────────────────────────
    let mut delta_gd: i64 = 0;
    for (indeks, (cash, bank, debt)) in &na_gd {
        let Some(entity) = gospodarstwa.get(indeks) else {
            continue;
        };
        let Some(h) = world.get_mut::<Household>(*entity) else {
            continue;
        };
        let przed = h.cash.get() + h.bank.get() + h.savings.get();
        h.cash = Money(*cash);
        h.bank = Money(*bank);
        // Oszczędności wchodzą do depozytu: `lift()` czyta `bank + savings` jako
        // jedną liczbę, więc rozbicie ich z powrotem byłoby zgadywaniem proporcji,
        // której model nie zna.
        h.savings = Money::ZERO;
        h.debt = Money(*debt);
        delta_gd = delta_gd.saturating_add(cash.saturating_add(*bank).saturating_sub(przed));
        report.households = report.households.saturating_add(1);
    }

    // ── 3. firmy ─────────────────────────────────────────────────────────────
    for (i, konto) in &konta_firm {
        let cel = state.firms[*i].capital.get();
        let saldo = world
            .get_resource::<Books>()
            .and_then(|b| b.balance(*konto))
            .unwrap_or(Money::ZERO)
            .get();
        let roznica = cel.saturating_sub(saldo);
        if roznica == 0 {
            continue;
        }
        if przesun(world, rest, *konto, roznica, tick) {
            report.firms = report.firms.saturating_add(1);
        } else {
            report.unsettled = Money(report.unsettled.get().saturating_add(roznica.abs()));
        }
    }

    // ── 4. domknięcie sektora gospodarstw ────────────────────────────────────
    // Komponent gospodarstwa jest poza księgami, więc jego zmiana musi przejść
    // przez kanał sektora GD — jedyny, który `MoneySupplyLedger` o niej informuje.
    if delta_gd != 0 && !rozlicz_sektor_gd(world, rest, delta_gd, tick) {
        report.unsettled = Money(report.unsettled.get().saturating_add(delta_gd.abs()));
    }

    Ok(report)
}

/// Przesuwa `kwota` z `rest` na `konto` (albo odwrotnie, gdy ujemna).
/// Zwraca `false`, gdy przelew się nie udał — wtedy wołający odkłada kwotę
/// do raportu zamiast udawać, że pieniądz się przesunął.
fn przesun(world: &mut World, rest: AccountId, konto: AccountId, kwota: i64, tick: Tick) -> bool {
    if kwota == 0 || rest == konto {
        return kwota == 0;
    }
    let memo = TxMemo::new(TxKind::Endowment, DecisionReason::Unspecified);
    let Some(books) = world.get_resource_mut::<Books>() else {
        return false;
    };
    let wynik = if kwota > 0 {
        books.transfer(rest, konto, Money(kwota), memo, tick)
    } else {
        books.transfer(konto, rest, Money(-kwota), memo, tick)
    };
    wynik.is_ok()
}

/// Domyka różnicę stanu gospodarstw po stronie ksiąg.
///
/// `delta > 0` znaczy, że gospodarstwa mają teraz więcej niż przed rozwinięciem —
/// pieniądz musiał więc wyjść z ksiąg (`household_receive`). `delta < 0` — odwrotnie.
fn rozlicz_sektor_gd(world: &mut World, rest: AccountId, delta: i64, tick: Tick) -> bool {
    let memo = TxMemo::new(TxKind::Endowment, DecisionReason::Unspecified);
    let Some(books) = world.get_resource_mut::<Books>() else {
        return false;
    };
    let wynik = if delta > 0 {
        books.household_receive(rest, Money(delta), memo, tick)
    } else {
        books.household_pay(rest, Money(-delta), memo, tick)
    };
    wynik.is_ok()
}
