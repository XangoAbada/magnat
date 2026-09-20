//! Giełda, przejęcia, dywidendy i emisje (M10d, WP10.10 i WP10.11, PRD §6.5, §7.8, §7.9).
//!
//! # Dlaczego w `sim/economy`, a nie w nowym crate'cie
//!
//! Ten sam powód, dla którego [`crate::corpfin`] trzyma leasing, obligację
//! i upadłość: **obrót udziałami rusza pieniądz, a pieniądz mieszka w [`crate::Books`]**.
//! Giełda po stronie `sim/firms` byłaby zbiorem nazw pól — rynkiem, na którym nikt
//! nikomu nic nie płaci. Ubezpieczenia tej samej podfazy potrzebowały czegoś więcej
//! niż ksiąg (historii zdarzeń) i dlatego mają własną drogę wejścia ([`crate::insurance`]);
//! giełda nie potrzebuje niczego, czego ten crate nie ma.
//!
//! # „Akcja" to punkt bazowy udziału (`GD-1`)
//!
//! Plan §5.5 zapowiadał `Share { total_shares, float }` i `Holding { holder, firm,
//! shares }`. Tego tu nie ma i **nie jest to cięcie zakresu, tylko usunięcie drugiej
//! prawdy**: własność firmy liczy od M7a `Firm.owners` w punktach bazowych, suma
//! wynosi dokładnie 10 000 i pilnuje tego `Firm::owners_sum_ok`. Druga tablica
//! akcjonariatu rozjechałaby się z pierwszą dokładnie tak, jak rozjechały się dwa
//! `DepositId` (`K-39`) i trzy definicje wieku produkcyjnego (`K-60`) — a rozjazd
//! widać dopiero jako inną liczbę w cudzym panelu.
//!
//! Konsekwencje, wszystkie na plus:
//! - kryterium WP10.10 „suma akcji w obiegu == `total_shares`, 0 akcji" **jest**
//!   istniejącym niezmiennikiem `Σ bp == 10 000` i ma test od M7a;
//! - próg ujawnienia to 500 bp, próg kontroli 5 000 bp — liczby, nie ułamki;
//! - emisja rozwadnia przez przeskalowanie dotychczasowych udziałów, więc
//!   proporcjonalność jest arytmetyką, a nie osobną regułą.
//!
//! Cena jest ceną **jednego punktu bazowego**: razy 10 000 daje wycenę całej firmy.
//!
//! # Czego tu nie ma
//!
//! **Funduszy inwestycyjnych jako osobnych encji.** Plan §5.5 wymieniał „5–15 encji"
//! obok mieszkańców i gracza. Inwestorami są tu zamożne gospodarstwa i firmy, które
//! kupują konkurenta — i to jest pełna lista, bo fundusz bez zakładu, bez załogi
//! i bez księgi byłby trzecim rodzajem właściciela w `Owner`, którego nikt poza tym
//! modułem by nie widział. `ponytail:` sufit nazwany: gdy powstanie panel giełdy
//! (M10f) i okaże się, że płynność jest za mała, fundusz dopisuje się jako wariant
//! `Owner` razem ze swoim kontem w `Books`, a nie jako wyjątek tutaj.

pub mod book;
pub mod cap;
pub mod corp;
pub mod invest;
pub mod month;
pub mod pay;
pub mod system;
pub mod value;

use std::collections::BTreeMap;

use magnat_core::{HashState, Money, SimMinute, StateHasher, Subject, Tick};
use magnat_firms::{FirmKey, Owner};

pub use book::{fixing, Fix, Side, StockOrder, StockOrderId};
pub use cap::{
    dilute, fixing_reason, move_stake, owner_key, owner_subject, split_dividend, stake_of,
};
pub use value::{belief, fundamental, Published};

/// Cała firma w punktach bazowych. Ta sama stała co `Firm::SHARES_TOTAL` — nie druga,
/// tylko jej nazwa w kontekście giełdy.
pub const WHOLE_BP: u32 = 10_000;

/// Próg ujawnienia pakietu (PRD §6.5): 5 %.
pub const DISCLOSURE_BP: u32 = 500;

/// Próg kontroli: ponad połowa udziałów.
pub const CONTROL_BP: u32 = 5_000;

/// Ile dób plotka z ujawnienia podnosi przekonanie inwestorów.
pub const RUMOR_DAYS: u8 = 7;

/// Ile ujawnień czeka na odbiorcę, zanim najstarsze zaczną wypadać.
///
/// Sufit jawny, ta sama zasada co przy `EdgeWatch` (`K-81`): skrzynka, której nikt
/// nie opróżnia, rośnie bez końca, a wchodzi do hasha stanu. Świat bez mediów
/// i bez kroniki gubi więc najstarsze ujawnienia — i to jest lepsze niż rejestr,
/// który po stu latach gry zajmuje więcej niż wszystkie notowania razem.
pub const DISCLOSURE_CAP: usize = 64;

/// Notowanie firmy.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Listing {
    pub firm: FirmKey,
    pub since: SimMinute,
    /// Kurs jednego punktu bazowego z ostatniej sesji z wolumenem.
    pub last_fixing: Money,
    /// Kurs sprzed niej — rozstrzyga remisy przy wyborze ceny.
    pub prev_fixing: Money,
    /// Ile bp trafiło do wolnego obrotu przy wejściu na giełdę.
    pub float_bp: u16,
    pub last_volume_bp: u16,
    /// Korekta przekonania z plotek, w punktach bazowych, ze znakiem.
    /// Gaśnie o jedną `RUMOR_DAYS`-tą na dobę — bez tego jedno ujawnienie
    /// podnosiłoby kurs na zawsze.
    pub rumor_bp: i16,
    pub rumor_days: u8,
}

/// Ujawnienie do odebrania przez media (M10b) i kronikę (M10f).
///
/// Skrzynka, a nie wołanie: `sim/media` stoi **nad** `sim/economy`, więc to ono
/// sięga tutaj — ten sam kierunek, którym `PayrollOutbox` oddaje listę płac.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Disclosure {
    pub firm: FirmKey,
    pub holder: Subject,
    pub bp: u16,
    /// Czy pakiet przekroczył próg kontroli, a nie tylko próg ujawnienia.
    pub control: bool,
    pub day: u32,
}

/// Parametry giełdy. Stroi je balansator przez `data/tuning/insurance.ron`
/// (jeden plik na całą podfazę — patrz [`crate::insurance::InsuranceData`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EquityParams {
    /// Minimalny majątek gospodarstwa, od którego w ogóle inwestuje.
    pub investor_wealth_min: Money,
    /// Jaką część nadwyżki majątku nad progiem gospodarstwo skłonne jest wyłożyć.
    pub investor_stake_bp: u16,
    /// Rozrzut przekonania: gospodarstwo i firma, w punktach bazowych.
    pub noise_household_bp: u16,
    pub noise_firm_bp: u16,
    /// Szum publikowanego wyniku, w punktach bazowych.
    pub earnings_noise_bp: u16,
    /// Mnożnik zysku: widełki wskaźnika cena/zysk po przeliczeniu ze stopy bazowej.
    pub pe_min: u16,
    pub pe_max: u16,
    /// Premia w wezwaniu przy wrogim przejęciu, w punktach bazowych.
    pub tender_premium_bp: u16,
    /// Jaką część zysku miesiąca firma wypłaca w dywidendzie.
    pub payout_bp: u16,
}

impl Default for EquityParams {
    fn default() -> EquityParams {
        EquityParams {
            investor_wealth_min: Money(5_000_000),
            investor_stake_bp: 2_000,
            noise_household_bp: 2_000,
            noise_firm_bp: 500,
            earnings_noise_bp: 400,
            pe_min: 5,
            pe_max: 25,
            tender_premium_bp: 2_500,
            payout_bp: 3_000,
        }
    }
}

/// Giełda miasta: notowania, arkusze zleceń i to, co z nich wyszło.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Equity {
    params: EquityParams,
    /// Klucz to `FirmKey.0` — `BTreeMap`, bo iteracja po notowaniach wyznacza
    /// kolejność sesji w dobie (00 §3.2).
    listings: BTreeMap<u64, Listing>,
    books: BTreeMap<u64, Vec<StockOrder>>,
    published: BTreeMap<u64, Published>,
    next_order: u32,
    disclosures: Vec<Disclosure>,
    /// Ile bp udziału ma dziś ten sam posiadacz — zdjęte przy ostatnim fixingu.
    /// Służy wyłącznie do wykrycia **przekroczenia** progu: bez niego ujawnienie
    /// powtarzałoby się każdej doby, w której pakiet stoi powyżej 5 %.
    known_stakes: BTreeMap<(u64, u32), u16>,
}

impl Equity {
    #[must_use]
    pub fn new(params: EquityParams) -> Equity {
        Equity {
            params,
            ..Equity::default()
        }
    }

    #[must_use]
    pub fn params(&self) -> &EquityParams {
        &self.params
    }

    #[must_use]
    pub fn listing(&self, firm: FirmKey) -> Option<&Listing> {
        self.listings.get(&firm.0)
    }

    #[must_use]
    pub fn listed_count(&self) -> usize {
        self.listings.len()
    }

    /// Notowania rosnąco po kluczu firmy.
    pub fn listings(&self) -> impl Iterator<Item = &Listing> {
        self.listings.values()
    }

    #[must_use]
    pub fn published(&self, firm: FirmKey) -> Option<&Published> {
        self.published.get(&firm.0)
    }

    pub(crate) fn set_published(&mut self, firm: FirmKey, p: Published) {
        self.published.insert(firm.0, p);
    }

    /// Wprowadza firmę na giełdę: `float_bp` udziału idzie do wolnego obrotu,
    /// a `price` jest ceną odniesienia pierwszej sesji.
    ///
    /// Zwraca `false`, gdy firma już jest notowana albo `float_bp` nie mieści się
    /// w widełkach — debiut bez wolnego obrotu byłby notowaniem, na którym nie da
    /// się kupić ani jednej bp.
    pub fn list(&mut self, firm: FirmKey, float_bp: u16, price: Money, t: SimMinute) -> bool {
        if self.listings.contains_key(&firm.0) || float_bp == 0 || price.get() <= 0 {
            return false;
        }
        if u32::from(float_bp) >= WHOLE_BP {
            return false;
        }
        self.listings.insert(
            firm.0,
            Listing {
                firm,
                since: t,
                last_fixing: price,
                prev_fixing: price,
                float_bp,
                last_volume_bp: 0,
                rumor_bp: 0,
                rumor_days: 0,
            },
        );
        true
    }

    /// Składa zlecenie. Zwraca `None`, gdy firma nie jest notowana.
    ///
    /// Jedyne wejście do arkusza — woła je i AI inwestora, i komenda gracza (M10f).
    /// Ten sam podział, który `CorpFinance` zastosował do leasingu: kanał jest tu,
    /// decyzja gdzie indziej.
    pub fn place_order(
        &mut self,
        firm: FirmKey,
        holder: Owner,
        side: Side,
        limit: Money,
        bp: u16,
        expires: SimMinute,
    ) -> Option<StockOrderId> {
        if !self.listings.contains_key(&firm.0) || bp == 0 || limit.get() <= 0 {
            return None;
        }
        self.next_order += 1;
        let id = StockOrderId(self.next_order);
        self.books.entry(firm.0).or_default().push(StockOrder {
            id,
            holder,
            side,
            limit,
            bp,
            expires,
        });
        Some(id)
    }

    /// Zlecenia w arkuszu firmy — do panelu i do testów.
    #[must_use]
    pub fn orders(&self, firm: FirmKey) -> &[StockOrder] {
        self.books.get(&firm.0).map_or(&[], Vec::as_slice)
    }

    pub(crate) fn take_orders(&mut self, firm: FirmKey) -> Vec<StockOrder> {
        self.books.remove(&firm.0).unwrap_or_default()
    }

    /// Zwraca do arkusza to, co z sesji zostało.
    ///
    /// Bez tego zlecenie niezrealizowane **przepadałoby po jednej sesji**, a termin
    /// ważności byłby polem bez znaczenia. Ma znaczenie dokładnie tam, gdzie miał je
    /// mieć: pakiet wystawiony przy debiucie leży, dopóki nie znajdzie kupca albo nie
    /// wygaśnie — a gospodarstwa podejmują decyzję raz na trzydzieści dób, więc na
    /// większości sesji po drugiej stronie nie ma nikogo.
    pub(crate) fn restore_orders(&mut self, firm: FirmKey, reszta: Vec<StockOrder>) {
        if reszta.is_empty() {
            return;
        }
        self.books.entry(firm.0).or_default().extend(reszta);
    }

    pub(crate) fn drop_expired(&mut self, t: SimMinute) {
        for b in self.books.values_mut() {
            b.retain(|o| o.expires.0 > t.0);
        }
    }

    pub(crate) fn listing_mut(&mut self, firm: FirmKey) -> Option<&mut Listing> {
        self.listings.get_mut(&firm.0)
    }

    /// Zdejmuje firmę z notowań razem z jej arkuszem.
    ///
    /// Firma po upadłości zostawała notowana na zawsze: dalej publikowała wyniki,
    /// wypłacała dywidendę z konta, które właśnie przejął syndyk, i emitowała nowe
    /// udziały. Znalezisko recenzji M10d. Historia kursu ginie razem z notowaniem —
    /// tak samo jak zakład znika z rejestru przy zamknięciu (`Firms::close_site`).
    pub fn delist(&mut self, firm: FirmKey) -> bool {
        self.books.remove(&firm.0);
        self.published.remove(&firm.0);
        self.known_stakes.retain(|(f, _), _| *f != firm.0);
        self.listings.remove(&firm.0).is_some()
    }

    /// Odbiera ujawnienia. Skrzynka opróżnia się przy odbiorze — nieodebrane
    /// wchodzą do hasha stanu, bo są tym, co świat ma do przekazania w następnej
    /// minucie (ta sama zasada co przy `EdgeWatch`, `K-81`).
    pub fn take_disclosures(&mut self) -> Vec<Disclosure> {
        std::mem::take(&mut self.disclosures)
    }

    /// Podgląd skrzynki bez jej opróżniania — dla raportu scenariusza i panelu.
    #[must_use]
    pub fn disclosures(&self) -> &[Disclosure] {
        &self.disclosures
    }

    pub(crate) fn push_disclosure(&mut self, d: Disclosure) {
        // Wypada **najstarsze**, nie najnowsze: kronika ma pokazywać, co się stało
        // ostatnio, a nie co się stało najdawniej.
        if self.disclosures.len() >= DISCLOSURE_CAP {
            self.disclosures.remove(0);
        }
        self.disclosures.push(d);
    }

    pub(crate) fn known_stake(&self, firm: FirmKey, holder: Owner) -> u16 {
        self.known_stakes
            .get(&(firm.0, owner_key(holder)))
            .copied()
            .unwrap_or(0)
    }

    pub(crate) fn remember_stake(&mut self, firm: FirmKey, holder: Owner, bp: u16) {
        if bp == 0 {
            self.known_stakes.remove(&(firm.0, owner_key(holder)));
        } else {
            self.known_stakes.insert((firm.0, owner_key(holder)), bp);
        }
    }
}

impl HashState for Equity {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.listings.len() as u64);
        for (k, l) in &self.listings {
            h.write_u64(*k);
            h.write_u64(l.since.0);
            l.last_fixing.hash_state(h);
            l.prev_fixing.hash_state(h);
            h.write_u16(l.float_bp);
            h.write_u16(l.last_volume_bp);
            h.write_u16(l.rumor_bp as u16);
            h.write_u8(l.rumor_days);
        }
        h.write_u64(self.books.len() as u64);
        for (k, b) in &self.books {
            h.write_u64(*k);
            h.write_u64(b.len() as u64);
            for o in b {
                h.write_u32(o.id.0);
                h.write_u32(owner_key(o.holder));
                h.write_u8(u8::from(o.side == Side::Sell));
                o.limit.hash_state(h);
                h.write_u16(o.bp);
                h.write_u64(o.expires.0);
            }
        }
        h.write_u64(self.published.len() as u64);
        for (k, p) in &self.published {
            h.write_u64(*k);
            h.write_u32(p.month);
            p.profit_12m.hash_state(h);
            p.book.hash_state(h);
        }
        h.write_u32(self.next_order);
        h.write_u64(self.disclosures.len() as u64);
        for d in &self.disclosures {
            h.write_u64(d.firm.0);
            // Posiadacz **wchodzi do hasha**: bez niego dwa światy różniące się tym,
            // kto ujawnił pakiet, dawały ten sam odcisk (znalezisko recenzji M10d).
            h.write_u8(d.holder.kind() as u8);
            h.write_u32(d.holder.entity().map_or(u32::MAX, |e| e.index()));
            h.write_u16(d.bp);
            h.write_u8(u8::from(d.control));
            h.write_u32(d.day);
        }
        h.write_u64(self.known_stakes.len() as u64);
        for ((f, o), bp) in &self.known_stakes {
            h.write_u64(*f);
            h.write_u32(*o);
            h.write_u16(*bp);
        }
    }
}

/// Numer miesiąca od startu świata dla ticku. Jedno miejsce, bo miesiąc ma 30 dób
/// (`K-1`) i przeliczenie „na oko" w trzech modułach rozjechałoby się o jeden.
#[must_use]
pub fn month_of(t: Tick) -> u32 {
    (t.0 / (60 * 24 * 30)) as u32
}

/// Numer doby od startu świata.
#[must_use]
pub fn day_of(t: Tick) -> u32 {
    (t.0 / (60 * 24)) as u32
}
