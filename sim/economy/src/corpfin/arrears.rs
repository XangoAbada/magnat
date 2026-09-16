//! Zaległości: jedna pozycja widziana z dwóch stron (M7d §5.12–5.13).
//!
//! Do M7d nieudany przelew firmy nie zostawiał po sobie **niczego**: `close_month`
//! robił `continue` i to była cała „upadłość" fazy M5. Dług, którego nie ma, nie
//! może wpędzić firmy w bankructwo, nie może zostać roszczeniem w postępowaniu
//! i nie może zostać sprzedany faktorowi — więc trzy mechanizmy M7d stały na
//! pustym miejscu.
//!
//! Poprawka jest jedna i jest tutaj: **każdy nieudany przelew firmy tworzy
//! [`Arrear`]** — pozycję, która dla dłużnika jest zobowiązaniem, a dla wierzyciela
//! należnością. To jest **jedna struktura, nie dwie**, bo to jest jedna kwota:
//! dwie tabele rozjechałyby się przy pierwszej zapłacie częściowej, a wtedy nie
//! dałoby się powiedzieć, która kłamie.
//!
//! Stronami są [`AccountOwner`], a nie własny enum wierzyciela z planu §5.13.
//! Powód jest ten sam, dla którego `K-8` wypycha słowniki do `core`: typ, który
//! odpowiada na pytanie „kto ma konto", już istnieje, pokrywa pracownika, bank,
//! dostawcę, miasto i resztę świata, i ma porządek liniowy — a drugi enum o tym
//! samym znaczeniu rozjechałby się przy pierwszym nowym uczestniku.

use magnat_core::{ClaimPriority, Money, StateHasher, Tick};

use crate::books::{AccountOwner, LoanId};
use crate::corpfin::instruments::{BondId, LeaseId};

/// Skąd wziął się dług. Odpowiada na pytanie karty inspekcji „za co to jest",
/// a przy okazji wyznacza priorytet w upadłości — bo to jest to samo pytanie.
///
/// Kolejność wariantów jest kontraktem: `as_index()` indeksuje histogram struktury
/// zadłużenia w panelu firmy.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum ClaimOrigin {
    /// Niewypłacone wynagrodzenie z listy płac.
    Wages,
    /// Naliczona, niewypłacona odprawa.
    Severance,
    /// Rata kredytu — kapitał albo odsetki.
    Loan(LoanId),
    /// Rata leasingowa. Rzecz do masy nie wchodzi, ale zaległa rata jest długiem.
    Lease(LeaseId),
    /// Kupon albo wykup obligacji.
    Bond(BondId),
    /// Faktura od dostawcy (rynek B2B, M6).
    Trade,
    /// Czynsz za lokal.
    Rent,
    /// Rachunek za media.
    Utility,
    /// Danina publiczna — podatek, składka, opłata. Zgłasza M8 (`K-10`).
    Tax,
    /// Kara umowna albo administracyjna.
    Penalty,
}

impl ClaimOrigin {
    #[must_use]
    pub const fn as_index(self) -> usize {
        match self {
            ClaimOrigin::Wages => 0,
            ClaimOrigin::Severance => 1,
            ClaimOrigin::Loan(_) => 2,
            ClaimOrigin::Lease(_) => 3,
            ClaimOrigin::Bond(_) => 4,
            ClaimOrigin::Trade => 5,
            ClaimOrigin::Rent => 6,
            ClaimOrigin::Utility => 7,
            ClaimOrigin::Tax => 8,
            ClaimOrigin::Penalty => 9,
        }
    }

    /// Priorytet zaspokojenia, gdy ten dług trafi do postępowania upadłościowego.
    ///
    /// `secured` mówi, czy roszczenie ma zabezpieczenie rzeczowe — pyta o to
    /// wyłącznie kredyt, bo tylko on je w tym świecie miewa. Zabezpieczony kredyt
    /// staje w `Secured` **do wartości zabezpieczenia**; nadwyżkę ponad nią rozbija
    /// [`super::bankruptcy`] na osobne roszczenie `Unsecured`, jawnie i nie po cichu.
    #[must_use]
    pub const fn priority(self, secured: bool) -> ClaimPriority {
        match self {
            ClaimOrigin::Wages => ClaimPriority::Wages,
            ClaimOrigin::Severance => ClaimPriority::Severance,
            ClaimOrigin::Loan(_) if secured => ClaimPriority::Secured,
            ClaimOrigin::Tax => ClaimPriority::Public,
            ClaimOrigin::Loan(_)
            | ClaimOrigin::Lease(_)
            | ClaimOrigin::Bond(_)
            | ClaimOrigin::Trade
            | ClaimOrigin::Rent
            | ClaimOrigin::Utility
            | ClaimOrigin::Penalty => ClaimPriority::Unsecured,
        }
    }

    pub(crate) fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(self.as_index() as u8);
        match self {
            ClaimOrigin::Loan(id) => h.write_u32(id.0),
            ClaimOrigin::Lease(id) => h.write_u32(id.0),
            ClaimOrigin::Bond(id) => h.write_u32(id.0),
            _ => h.write_u32(0),
        }
    }
}

/// Uchwyt zaległości. Indeks w [`Arrears`], nigdy nie zwalniany — spłacona pozycja
/// zostaje w rejestrze, bo historia płatnicza jest wejściem oceny kredytowej.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ArrearId(pub u32);

/// Jedna niezapłacona kwota.
///
/// `creditor` zmienia się przy faktoringu — i to jest cała mechanika faktoringu:
/// ktoś inny staje się wierzycielem, kwota zostaje ta sama, a dłużnik nawet
/// nie musi o tym wiedzieć.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Arrear {
    pub id: ArrearId,
    pub debtor: AccountOwner,
    pub creditor: AccountOwner,
    /// Kwota **pozostała** do zapłaty. Zapłata częściowa ją zmniejsza.
    pub amount: Money,
    /// Kiedy zobowiązanie stało się wymagalne. Stąd liczy się wiek należności
    /// i licznik dób niewypłacalności.
    pub since: Tick,
    pub origin: ClaimOrigin,
    /// Czy należność została odsprzedana faktorowi. Nie zmienia kwoty ani dłużnika
    /// — jest etykietą dla panelu i dla oceny kredytowej faktora.
    pub factored: bool,
    /// Zamknięta: zapłacona w całości albo umorzona w postępowaniu upadłościowym.
    pub settled: bool,
}

impl Arrear {
    /// Wiek zaległości w dobach na dany tick.
    #[must_use]
    pub fn age_days(&self, now: Tick) -> u16 {
        let minut = now.get().saturating_sub(self.since.get());
        u16::try_from(minut / magnat_core::time::MINUTES_PER_DAY).unwrap_or(u16::MAX)
    }

    pub(crate) fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.id.0);
        hash_owner(&self.debtor, h);
        hash_owner(&self.creditor, h);
        h.write_i64(self.amount.get());
        h.write_u64(self.since.get());
        self.origin.hash_state(h);
        h.write_u8(u8::from(self.factored));
        h.write_u8(u8::from(self.settled));
    }
}

/// Hasz właściciela konta. Wariant plus indeks encji — `AccountId` celowo **nie**
/// wchodzi, bo jest uchwytem nadanym przy otwarciu konta, a nie tożsamością.
pub(crate) fn hash_owner(o: &AccountOwner, h: &mut StateHasher) {
    match o {
        AccountOwner::Household(x) => {
            h.write_u8(0);
            h.write_u32(x.entity().index());
        }
        AccountOwner::Citizen(x) => {
            h.write_u8(1);
            h.write_u32(x.entity().index());
        }
        AccountOwner::Firm(x) => {
            h.write_u8(2);
            h.write_u32(x.entity().index());
        }
        AccountOwner::Bank(x) => {
            h.write_u8(3);
            h.write_u32(x.entity().index());
        }
        AccountOwner::City => {
            h.write_u8(4);
            h.write_u32(0);
        }
        AccountOwner::RestOfWorld => {
            h.write_u8(5);
            h.write_u32(0);
        }
        AccountOwner::CentralBank => {
            h.write_u8(6);
            h.write_u32(0);
        }
    }
}

/// Rejestr zaległości całego miasta.
///
/// Jedna tablica, dwa indeksy pochodne: po dłużniku (czyta upadłość i licznik
/// niewypłacalności) i po wierzycielu (czyta faktoring i panel należności).
/// Indeksy są `Vec` posortowanych par, a nie mapą mieszającą — iteracja po nich
/// wchodzi do ścieżki, której wynik zapisuje się w stanie (00 §3.2).
#[derive(Default, Clone, PartialEq, Eq, Debug)]
pub struct Arrears {
    items: Vec<Arrear>,
}

impl Arrears {
    #[must_use]
    pub fn new() -> Arrears {
        Arrears::default()
    }

    /// Zapisuje nową zaległość albo **powiększa istniejącą** o tej samej trójce
    /// (dłużnik, wierzyciel, źródło).
    ///
    /// Scalanie jest tu istotne, a nie wygodne: bez niego firma, która nie płaci
    /// czynszu przez rok, miałaby dwanaście pozycji wobec tego samego wierzyciela
    /// i dwanaście roszczeń w upadłości, a `since` najstarszej z nich — jedyna data,
    /// z której liczy się licznik dób niewypłacalności — zgubiłaby się wśród nowszych.
    pub fn accrue(
        &mut self,
        debtor: AccountOwner,
        creditor: AccountOwner,
        amount: Money,
        origin: ClaimOrigin,
        t: Tick,
    ) -> Option<ArrearId> {
        if amount.get() <= 0 {
            return None;
        }
        if let Some(a) = self.items.iter_mut().find(|a| {
            !a.settled && a.debtor == debtor && a.creditor == creditor && a.origin == origin
        }) {
            a.amount = Money(a.amount.get().saturating_add(amount.get()));
            return Some(a.id);
        }
        let id = ArrearId(u32::try_from(self.items.len()).unwrap_or(u32::MAX));
        self.items.push(Arrear {
            id,
            debtor,
            creditor,
            amount,
            since: t,
            origin,
            factored: false,
            settled: false,
        });
        Some(id)
    }

    /// Zmniejsza zaległość o zapłaconą kwotę; zero zamyka pozycję.
    /// Zwraca kwotę, która faktycznie zeszła z długu.
    pub fn settle(&mut self, id: ArrearId, amount: Money) -> Money {
        let Some(a) = self.items.get_mut(id.0 as usize) else {
            return Money::ZERO;
        };
        if a.settled {
            return Money::ZERO;
        }
        let zeszlo = Money(a.amount.get().min(amount.get().max(0)));
        a.amount = Money(a.amount.get() - zeszlo.get());
        if a.amount.get() == 0 {
            a.settled = true;
        }
        zeszlo
    }

    /// Umarza pozycję bez zapłaty — wyłącznie przy domknięciu postępowania
    /// upadłościowego, kiedy na dany priorytet nie starczyło masy.
    pub fn write_off(&mut self, id: ArrearId) {
        if let Some(a) = self.items.get_mut(id.0 as usize) {
            a.settled = true;
            a.amount = Money::ZERO;
        }
    }

    /// Przepisuje należność na nowego wierzyciela (faktoring).
    pub fn assign(&mut self, id: ArrearId, to: AccountOwner) -> bool {
        match self.items.get_mut(id.0 as usize) {
            Some(a) if !a.settled => {
                a.creditor = to;
                a.factored = true;
                true
            }
            _ => false,
        }
    }

    #[must_use]
    pub fn get(&self, id: ArrearId) -> Option<&Arrear> {
        self.items.get(id.0 as usize)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Otwarte zaległości danego dłużnika, w kolejności powstania.
    pub fn of_debtor(&self, debtor: AccountOwner) -> impl Iterator<Item = &Arrear> {
        self.items
            .iter()
            .filter(move |a| !a.settled && a.debtor == debtor)
    }

    /// Otwarte należności danego wierzyciela, w kolejności powstania.
    pub fn of_creditor(&self, creditor: AccountOwner) -> impl Iterator<Item = &Arrear> {
        self.items
            .iter()
            .filter(move |a| !a.settled && a.creditor == creditor)
    }

    /// Suma otwartych zobowiązań dłużnika.
    #[must_use]
    pub fn debt_of(&self, debtor: AccountOwner) -> Money {
        Money(self.of_debtor(debtor).map(|a| a.amount.get()).sum())
    }

    /// Suma otwartych należności wierzyciela — `Metric::Receivables` języka reguł.
    #[must_use]
    pub fn receivables_of(&self, creditor: AccountOwner) -> Money {
        Money(self.of_creditor(creditor).map(|a| a.amount.get()).sum())
    }

    /// Wiek najstarszej otwartej zaległości dłużnika, licząc tylko te powyżej progu.
    ///
    /// Próg jest tu, a nie u wołającego, i to jest istota licznika niewypłacalności:
    /// firma nie ma upadać przez trzy grosze niedopłaty na rachunku za prąd, które
    /// wiszą od roku, bo nikt ich nie zauważył.
    #[must_use]
    pub fn oldest_overdue_days(&self, debtor: AccountOwner, min: Money, now: Tick) -> u16 {
        self.of_debtor(debtor)
            .filter(|a| a.amount >= min)
            .map(|a| a.age_days(now))
            .max()
            .unwrap_or(0)
    }

    pub(crate) fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.items.len() as u64);
        for a in &self.items {
            a.hash_state(h);
        }
    }
}
