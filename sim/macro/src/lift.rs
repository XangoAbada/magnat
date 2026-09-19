//! `lift()` — zdjęcie świata w rozdzielczości makro (M7f WP13, kontrakt M10 §6).
//!
//! To jest **jedyne** miejsce w tym crate'cie, które widzi `World`. Reszta modelu
//! dostaje `MacroState` i o istnieniu ECS-a nie wie — dlatego „co jeśli" da się
//! zrobić na klonie, bez dotykania świata i bez ryzyka, że prognoza coś w nim
//! zmieni.
//!
//! # Druga strona mostu
//!
//! [`crate::lower`] nanosi stan makro z powrotem na świat — powstało w M10a i to
//! ono jest pierwszym czytelnikiem `wealth_q` oraz `citizens`, które M7f wypełniał
//! bez czytelnika. „Co jeśli" nadal niczego nie rozwija: czyta agregaty i tyle
//! (M10 §6, tabela podzbioru); rozwinięcia potrzebuje historia „na sucho"
//! i tryb 50×.

use std::collections::BTreeMap;

use magnat_agents::{
    Household, Identity, Needs, Population, Residence, Skills, SocialClass, Vitals,
};
use magnat_core::{DistrictId, Entity, HashState, Money, Qty, Q};
use magnat_economy::{Books, Market};
use magnat_ecs::World;
use magnat_firms::{Firms, SiteTypeCatalog};

use crate::state::{MacroCell, MacroFirm, MacroState, N_NEEDS};
use crate::types::{
    cell_grain, BranchId, CitizenSeed, ClassId, CommuteMatrix, EpochState, MacroAccount,
    MacroLedger, MacroStock, SparseVec, TechLevel,
};

/// Ile minut kosztuje przeciętny dojazd między dzielnicami, dopóki `CommuteMatrix`
/// jest płaska. Liczba pochodzi z rozkładu czasów dojazdu M4 (mediana dojazdu
/// w mieście 4 km) i jest **stałą odniesienia**, nie kalibracją: zmienia ją
/// wypełnienie macierzy przez M4, a nie przebieg balansatora.
const FLAT_COMMUTE_MIN: u16 = 22;

/// Marża odniesienia, z której zdjęcie odtwarza koszt własny półki, w bp.
/// Środek widełek z `data/economy/shop.ron` — patrz `MacroFirm::cost`.
/// macro-guard: kalibracja — marża odniesienia, stroi ją balansator
const REF_MARGIN_BP: i32 = 1_800;

/// Zdjęcie świata. Woła się **raz na kwartał** i wynik jest współdzielony przez
/// wszystkie firmy klas S2/S3 (§5.10, budżet) — bez tego byłoby 10 tys. zdjęć.
#[must_use]
pub fn lift(world: &World) -> MacroState {
    let ludzie = zbierz_ludzi(world);
    let firmy_reg = world.get_resource::<Firms>();
    let role = liczba_rol(&ludzie, firmy_reg);
    let dzielnice = ludzie
        .iter()
        .map(|c| c.district)
        .chain(
            firmy_reg
                .into_iter()
                .flat_map(|f| f.sites().map(|(_, s)| s.district.0)),
        )
        .max()
        .map_or(1u16, |d| d.saturating_add(1));
    let populacja = ludzie.len() as u32;
    let grain = cell_grain(populacja, dzielnice);

    let mut st = MacroState::empty(grain, role, dzielnice);
    st.commute = CommuteMatrix::flat(dzielnice, FLAT_COMMUTE_MIN);
    st.day = doba(world);
    st.tick = magnat_core::Tick(u64::from(st.day) * magnat_core::time::MINUTES_PER_DAY);
    st.epoch = EpochState::default();

    zbuduj_komorki(&mut st, &ludzie, role);
    rozlej_gospodarstwa(&mut st, world, &ludzie);
    zbuduj_firmy(&mut st, world);
    zbuduj_ksiege(&mut st, world);
    st
}

/// Mieszkaniec sprowadzony do dziesięciu liczb. Struktura pośrednia, żeby nie
/// przechodzić przez ECS dwa razy — raz po komórki, raz po pieniądze.
struct Osoba {
    entity: Entity,
    /// Indeks encji gospodarstwa — po nim rozlewa się pieniądz na komórki.
    household: u32,
    district: u16,
    class: u8,
    age_years: u16,
    employed: bool,
    in_labour_force: bool,
    skills: [(u16, u8); 4],
    needs: [u8; N_NEEDS],
}

/// Granice wieku produkcyjnego — ten sam mianownik, którego używa statystyka
/// rynku pracy w `sim/economy::labor` (dziecko bez pracy nie jest bezrobotne),
/// bo obie strony czytają **to samo pole danych** (`ages.labour_force`, `K-60`).
/// Świat bez tabeli demograficznej nie ma siły roboczej wcale (`None`) — i to jest
/// uczciwsze niż liczba wzięta ze stałej, której nikt nie widzi.
fn wiek_produkcyjny(world: &World) -> Option<(u16, u16)> {
    world
        .get_resource::<magnat_agents::DemographyTable>()
        .map(|t| {
            let lf = t.ages().labour_force;
            (u16::from(lf.min), u16::from(lf.max))
        })
}

/// Doba świata. Zegar niesie rynek (`Market::tick`) — `World` sam z siebie nie wie,
/// która jest godzina, bo `SimClock` jest przelicznikiem prezentacji i do symulacji
/// nie wchodzi (`K-22`). Świat bez rynku stoi na dobie zero i to jest poprawne:
/// zdjęcie gospodarki, w której gospodarki nie ma, nie ma czego datować.
fn doba(world: &World) -> u32 {
    world.get_resource::<Market>().map_or(0, |m| {
        u32::try_from(m.tick().get() / magnat_core::time::MINUTES_PER_DAY).unwrap_or(0)
    })
}

fn zbierz_ludzi(world: &World) -> Vec<Osoba> {
    let Some(pop) = world.get_resource::<Population>() else {
        return Vec::new();
    };
    let dzis = i32::try_from(doba(world)).unwrap_or(0);
    let wiek = wiek_produkcyjny(world);
    let mut out = Vec::with_capacity(pop.citizens().len());
    // Spis jest posortowany po indeksie encji, więc kolejność zdjęcia nie zależy
    // od układu archetypów w ECS (00 §3.2).
    for e in pop.citizens() {
        let Some(id) = world.get::<Identity>(*e) else {
            continue;
        };
        if !id.is_alive() {
            continue;
        }
        let v = world.get::<Vitals>(*e).copied().unwrap_or_default();
        let district = world.get::<Residence>(*e).map_or(0, |r| r.district);
        let emp = world.get::<magnat_agents::Employment>(*e).copied();
        let lata = u16::try_from((dzis - id.birth_day).max(0) / 360).unwrap_or(0);
        let mut skills = [(Skills::ROLE_NONE, 0u8); 4];
        if let Some(s) = world.get::<Skills>(*e) {
            for (i, slot) in s.0.iter().enumerate() {
                skills[i] = (slot.role, slot.level);
            }
        }
        let needs = world.get::<Needs>(*e).map_or([100u8; N_NEEDS], |n| n.level);
        out.push(Osoba {
            entity: *e,
            household: id.household,
            district,
            class: SocialClass::of(Q::new(v.status)).as_index() as u8,
            age_years: lata,
            employed: emp.is_some_and(|e| e.is_employed()),
            in_labour_force: wiek.is_some_and(|(lo, hi)| lata >= lo && lata <= hi),
            skills,
            needs,
        });
    }
    out
}

/// Ile ról ma tablica. Nie stała kompilacji (`K-43`) — liczbę podaje najwyższy
/// `JobRoleId`, jaki w tym świecie w ogóle występuje.
fn liczba_rol(ludzie: &[Osoba], firms: Option<&Firms>) -> usize {
    let z_ludzi = ludzie
        .iter()
        .flat_map(|o| o.skills.iter())
        .filter(|(r, _)| *r != Skills::ROLE_NONE)
        .map(|(r, _)| usize::from(*r))
        .max();
    let z_firm = firms.and_then(|f| {
        f.sites()
            .flat_map(|(_, s)| s.positions.iter())
            .map(|p| usize::from(p.role.0))
            .max()
    });
    z_ludzi.max(z_firm).map_or(1, |m| m + 1)
}

fn zbuduj_komorki(st: &mut MacroState, ludzie: &[Osoba], role: usize) {
    let grain = st.grain;
    let mut klucze: Vec<(u16, u8)> = ludzie
        .iter()
        .map(|o| (o.district, grain.class_of(o.class).0))
        .collect();
    klucze.sort_unstable();
    klucze.dedup();
    st.cells = klucze
        .iter()
        .map(|(d, c)| MacroCell::new((DistrictId(*d), ClassId(*c)), role))
        .collect();

    // Sumy potrzeb liczy się osobno, bo `need_sat` jest średnią, a średniej nie
    // da się dodawać w miejscu bez mianownika.
    let mut sumy: Vec<[u64; N_NEEDS]> = vec![[0; N_NEEDS]; st.cells.len()];
    for o in ludzie {
        let klucz = (o.district, grain.class_of(o.class).0);
        let Ok(i) = klucze.binary_search(&klucz) else {
            continue;
        };
        let cell = &mut st.cells[i];
        cell.citizens.push(CitizenSeed {
            birth_index: o.entity.index(),
            cell: u16::try_from(i).unwrap_or(u16::MAX),
        });
        let kohorta = usize::from(o.age_years / 5).min(17);
        cell.age_hist[kohorta] = cell.age_hist[kohorta].saturating_add(1);
        if o.employed {
            cell.employed = cell.employed.saturating_add(1);
        } else if o.in_labour_force {
            cell.unemployed = cell.unemployed.saturating_add(1);
        }
        for (r, lvl) in o.skills {
            if r == Skills::ROLE_NONE {
                continue;
            }
            let idx = usize::from(r);
            if idx < role {
                cell.labor[idx] = cell.labor[idx].saturating_add(1);
                cell.skill_sum[idx] = cell.skill_sum[idx].saturating_add(u32::from(lvl));
            }
        }
        for (n, v) in o.needs.iter().enumerate() {
            sumy[i][n] += u64::from(*v);
        }
    }
    for (i, cell) in st.cells.iter_mut().enumerate() {
        let n = u64::from(cell.population()).max(1);
        for (k, suma) in sumy[i].iter().enumerate() {
            cell.need_sat[k] = Q::new(u8::try_from(suma / n).unwrap_or(100));
        }
    }
}

/// Pieniądz gospodarstw rozkłada się na komórki **po członkach**, a reszta z dzielenia
/// trafia do pierwszej z nich w kolejności indeksów (00 §2: podział kwoty między
/// N stron musi sumować się do oryginału). Bez tego suma pieniądza w makro różniłaby
/// się od sumy w świecie o kilkadziesiąt groszy na każde gospodarstwo — a to jest
/// dokładnie ta różnica, której §4 dokumentu 00 zabrania z tolerancją zero.
fn rozlej_gospodarstwa(st: &mut MacroState, world: &World, ludzie: &[Osoba]) {
    let grain = st.grain;
    let klucze: Vec<(u16, u8)> = st.cells.iter().map(|c| (c.key.0 .0, c.key.1 .0)).collect();
    let Some(pop) = world.get_resource::<Population>() else {
        return;
    };
    // Członkowie gospodarstwa czytają się z `Identity.household` mieszkańca, a nie
    // z tablicy `Household.members`: ta ostatnia mieści tylko członków „w linii"
    // (`HH_INLINE_MEMBERS`), a przelew ma się rozejść na **wszystkich**, inaczej
    // grosze z gospodarstwa wieloosobowego wypadałyby z sumy.
    let mut czlonkowie: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for o in ludzie {
        if let Ok(i) = klucze.binary_search(&(o.district, grain.class_of(o.class).0)) {
            czlonkowie.entry(o.household).or_default().push(i);
        }
    }
    // Majątek na głowę, komórka po komórce — materiał na kwantyle. Zbierany przy
    // okazji rozlewania, bo drugi przebieg po gospodarstwach kosztowałby tyle samo,
    // a dawałby drugie miejsce, w którym wolno pomylić przynależność do komórki.
    let mut majatki: Vec<Vec<i64>> = vec![Vec::new(); st.cells.len()];
    for e in pop.households() {
        let Some(h) = world.get::<Household>(*e) else {
            continue;
        };
        let mut cele: Vec<usize> = czlonkowie.get(&e.index()).cloned().unwrap_or_default();
        if cele.is_empty() {
            continue;
        }
        cele.sort_unstable();
        let na_glowe = (h.cash.get() + h.bank.get() + h.savings.get()) / cele.len() as i64;
        for i in &cele {
            majatki[*i].push(na_glowe);
        }
        podziel(&mut st.cells, &cele, h.cash.get(), |c, v| {
            c.cash = Money(c.cash.get().saturating_add(v));
        });
        podziel(
            &mut st.cells,
            &cele,
            h.bank.get().saturating_add(h.savings.get()),
            |c, v| c.deposits = Money(c.deposits.get().saturating_add(v)),
        );
        podziel(&mut st.cells, &cele, h.debt.get(), |c, v| {
            c.debt = Money(c.debt.get().saturating_add(v));
        });
    }

    // Kwartyle majątku komórki. Do M7f stała tu jawna zaślepka — cztery razy ta
    // sama średnia — bo nikt rozkładu nie czytał. Od M10a czyta go `lower()`
    // i zaślepka przestała być nieszkodliwa: profil płaski rozdałby całej komórce
    // po równo, więc świat po rozwinięciu byłby egalitarny co do grosza, a bramka
    // Etapu 10 na współczynnik Giniego mierzyłaby wtedy własną zaślepkę.
    for (i, c) in st.cells.iter_mut().enumerate() {
        c.wealth_q = kwantyle(&mut majatki[i]);
    }
}

/// Cztery punkty rozkładu majątku na głowę: min, q1, q3, max.
///
/// Sortowanie w miejscu, bez interpolacji między sąsiadami — kwantyl jest tu
/// **elementem próby**, a nie jej modelem. Komórka pusta daje cztery zera i to
/// jest poprawna odpowiedź: rozkład zbioru pustego nie ma kształtu.
fn kwantyle(majatki: &mut [i64]) -> [Money; 4] {
    if majatki.is_empty() {
        return [Money::ZERO; 4];
    }
    majatki.sort_unstable();
    let n = majatki.len();
    let idx = |licznik: usize, mianownik: usize| majatki[(n - 1) * licznik / mianownik];
    [
        Money(majatki[0]),
        Money(idx(1, 4)),
        Money(idx(3, 4)),
        Money(majatki[n - 1]),
    ]
}

/// Dzieli kwotę równo między komórki, resztę oddając pierwszej w kolejności.
fn podziel(
    cells: &mut [MacroCell],
    cele: &[usize],
    kwota: i64,
    mut apply: impl FnMut(&mut MacroCell, i64),
) {
    if cele.is_empty() {
        return;
    }
    let n = cele.len() as i64;
    let rata = kwota / n;
    let reszta = kwota - rata * n;
    for (k, i) in cele.iter().enumerate() {
        let v = if k == 0 { rata + reszta } else { rata };
        apply(&mut cells[*i], v);
    }
}

fn zbuduj_firmy(st: &mut MacroState, world: &World) {
    let Some(firms) = world.get_resource::<Firms>() else {
        return;
    };
    let market = world.get_resource::<Market>();
    let books = world.get_resource::<Books>();
    let katalog = world.get_resource::<SiteTypeCatalog>();
    let polki = market.map(Market::shelf_snapshot).unwrap_or_default();

    for (key, firma) in firms.iter() {
        // `firm_id` jest jedyną drogą z klucza rejestru na identyfikator w przestrzeni
        // gospodarki (`K-46`): dwie numeracje istnieją i tylko ta funkcja je łączy.
        let id = magnat_firms::firm_id(key);
        let mut stock = MacroStock::new();
        let mut price: SparseVec<magnat_core::GoodId, Money> = SparseVec::new();
        let mut cost: SparseVec<magnat_core::GoodId, Money> = SparseVec::new();
        for s in polki.iter().filter(|s| s.firm == id) {
            stock.add(s.good, s.qty.get());
            price.set(s.good, s.price_net);
            // Koszt własny jest w księdze zakładu, a nie na półce, i `shelf_snapshot`
            // go nie niesie. Zdjęcie odtwarza go z **marży odniesienia** i zapisuje
            // raz, na starcie — dalej prowadzi go faza 3, płacąc za zatowarowanie.
            // To jest przybliżenie startowe, nie model: po pierwszej dobie kroku
            // koszt pochodzi z faktycznie zapłaconej ceny.
            cost.set(
                s.good,
                magnat_economy::kernel::cost_from_price(s.price_net, REF_MARGIN_BP),
            );
        }
        let mut employees = 0u32;
        let mut wage_bill = 0i64;
        let mut sloty = 0i64;
        let mut tech = 0u8;
        let mut branch = BranchId(0);
        for site_id in &firma.sites {
            let Some(site) = firms.site(*site_id) else {
                continue;
            };
            employees = employees.saturating_add(site.headcount() as u32);
            wage_bill = wage_bill.saturating_add(site.labor_cost_month().get());
            sloty = sloty.saturating_add(i64::from(site.required_slots()));
            tech = tech.max(site.tech.get());
            if let Some(cat) = katalog {
                branch = BranchId(cat.get(site.site_type).category.as_index() as u16);
            }
        }
        let capital = market
            .and_then(|m| m.account_of_firm(id))
            .and_then(|a| books.and_then(|b| b.balance(a)))
            .unwrap_or(Money::ZERO);
        // Zobowiązania firmy w modelu to **saldo ujemne rachunku**, a nie osobny
        // rejestr: `LoanBook` należy do rynku i prowadzi kredyt gospodarstw oraz firm
        // w jednej strukturze, do której makro nie ma odczytu per firma. Ujemne saldo
        // jest tym, co firma realnie jest winna bankowi dzisiaj — a odsetki od niego
        // liczy faza 6 tym samym `kernel::monthly_interest`, którym liczy je mezo.
        let debt = Money((-capital.get()).max(0));
        st.firms.push(MacroFirm {
            id,
            district: firma.hq_district,
            branch,
            capital: Money(capital.get().max(0)),
            debt,
            stock,
            // Przepustowość dobowa w **milietatach**: tyle pracy zakład zamówił
            // stanowiskami. To nie jest wydajność maszyny — tę zna M6 i makro jej
            // nie powtarza (`K-50`: reguła mieszka tam, gdzie dane).
            capacity_daily: Qty(sloty * magnat_firms::hr::productivity::FULL_TIME),
            utilization_bps: 0,
            employees,
            wage_bill: Money(wage_bill),
            price,
            cost,
            brand_stock: 0,
            tech: TechLevel(tech),
            suppliers: smallvec::SmallVec::new(),
        });
    }
    st.firms.sort_unstable_by_key(|f| f.id.0.to_bits());
}

/// Trzy konta i jedna suma.
///
/// `Households` **musi** być sumą gotówki komórek, a nie sumą pieniądza gospodarstw
/// w świecie — i to jest różnica warta zapisania. Świat trzyma pieniądz także
/// w komponencie `Wealth` mieszkańca, w spadkach bezdziedzicznych i u emigrantów;
/// komórka niesie wyłącznie kasę gospodarstwa. Gdyby konto startowało od szerszej
/// liczby, każda wypłata w kroku rozjeżdżałaby konto z komórkami o tę różnicę,
/// a test zachowania pieniądza pękałby **poprawnie**, w miejscu odległym od przyczyny.
///
/// Reszta pieniądza świata siedzi więc na `RestOfWorld` i to jest uczciwy opis:
/// z punktu widzenia kroku makro jest poza modelem.
fn zbuduj_ksiege(st: &mut MacroState, world: &World) {
    let mut l = MacroLedger::default();
    // **Gotówka i depozyt razem.** Do M7f konto brało samą gotówkę, bo stan
    // budowany ręcznie w teście trzymał wszystko w gotówce i różnicy nie było
    // widać. Na prawdziwym świecie gospodarstwo trzyma pieniądz na rachunku
    // (`Household.bank`), a `cash` bywa zerem — konto liczone z samej gotówki
    // startowało wtedy od zera, komórka nie miała za co kupować i cały rynek dóbr
    // stał. Zapasy nie schodziły z półek, więc bramki 1 i 2 Etapu 10 nie miały
    // czego zmierzyć: model **wyglądał** na działający, bo nic w nim nie pękało.
    let gospodarstwa: i64 = st
        .cells
        .iter()
        .map(|c| c.cash.get().saturating_add(c.deposits.get()))
        .fold(0i64, i64::saturating_add);
    let firmy: i64 = st
        .firms
        .iter()
        .map(|f| f.capital.get())
        .fold(0i64, i64::saturating_add);
    let swiat = i64::from(world.get_resource::<Population>().is_some())
        * magnat_agents::total_money(world)
        + world
            .get_resource::<Books>()
            .map_or(0, |b| b.total_balance().get());
    l.set(MacroAccount::Households, Money(gospodarstwa));
    l.set(MacroAccount::Firms, Money(firmy));
    l.set(
        MacroAccount::RestOfWorld,
        Money(swiat.saturating_sub(gospodarstwa).saturating_sub(firmy)),
    );
    st.ledger = l;
}

/// Hash zdjęcia — wejście testu determinizmu (§7.5): dwa `lift()` z tego samego
/// świata muszą dać identyczną liczbę.
#[must_use]
pub fn state_hash(st: &MacroState) -> magnat_core::StateHash {
    let mut h = magnat_core::StateHasher::new();
    st.hash_state(&mut h);
    h.finish()
}
