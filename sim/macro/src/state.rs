//! Stan modelu makro (M7f WP13, §5.10; kontrakt M10 §6, M10a §5.6).
//!
//! Komórka to **dzielnica × klasa**, firma to **firma** — firm się nie agreguje
//! (`D21`). Bez tożsamości konkurenta cały tier strategiczny nie miałby o kim
//! myśleć: „sektor piekarniczy" nie otwiera zakładu obok gracza, robi to konkretna
//! piekarnia.

use magnat_core::{
    DistrictId, FirmId, GoodId, HashState, Money, Qty, StateHasher, Tick, NEED_COUNT, Q,
};
use smallvec::SmallVec;

use crate::types::{
    BranchId, CitizenSeed, ClassGrain, ClassId, CommuteMatrix, EpochState, MacroLedger, MacroStock,
    SparseVec, TechLevel,
};

/// Liczba potrzeb — ta sama tablica, którą indeksuje `Needs.level` (`K-20`).
pub const N_NEEDS: usize = NEED_COUNT;

/// Komórka = (dzielnica × klasa). Metropolia: 40 dzielnic × 6 klas = 240 komórek.
///
/// `labor` i `skill_sum` są wektorami, a nie tablicami `[u32; N_ROLES]` z planu:
/// liczba ról pochodzi z `data/jobs/roles.ron` (`K-43`) i jest daną, nie stałą
/// kompilacji. Katalog urósł w M7a z 4 ról do 46 i urośnie dalej; tablica o stałym
/// rozmiarze byłaby drugą deklaracją tej samej liczby, a dwie takie deklaracje
/// rozjeżdżają się przy pierwszej zmianie danych.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MacroCell {
    pub key: (DistrictId, ClassId),
    /// Tożsamość mieszkańców komórki (8 B/os.) — patrz [`CitizenSeed`].
    pub citizens: Vec<CitizenSeed>,
    /// Kohorty 5-letnie: 0–4, 5–9, … 85+.
    pub age_hist: [u32; 18],
    pub cash: Money,
    pub deposits: Money,
    pub debt: Money,
    /// min, q1, q3, max — do rozwinięcia rozkładu majątku przy `lower()` (M10).
    pub wealth_q: [Money; 4],
    /// Liczba osób zdolnych do roli.
    pub labor: Vec<u32>,
    /// Suma umiejętności w roli (średnia = `skill_sum / labor`).
    pub skill_sum: Vec<u32>,
    pub employed: u32,
    pub unemployed: u32,
    pub need_sat: [Q; N_NEEDS],
    /// Popyt zrealizowany w ostatnim kroku.
    pub demand: MacroStock,
}

impl MacroCell {
    #[must_use]
    pub fn new(key: (DistrictId, ClassId), roles: usize) -> MacroCell {
        MacroCell {
            key,
            citizens: Vec::new(),
            age_hist: [0; 18],
            cash: Money(0),
            deposits: Money(0),
            debt: Money(0),
            wealth_q: [Money(0); 4],
            labor: vec![0; roles],
            skill_sum: vec![0; roles],
            employed: 0,
            unemployed: 0,
            need_sat: [Q::MIN; N_NEEDS],
            demand: MacroStock::new(),
        }
    }

    /// Ilu ludzi jest w komórce. Mianownik każdego agregatu — poniżej
    /// [`crate::types::MIN_CELL_POP`] wynik przestaje mieścić się w deklarowanym
    /// marginesie i test uczciwości marginesu to widzi.
    #[must_use]
    pub fn population(&self) -> u32 {
        self.citizens.len() as u32
    }

    /// Siła robocza komórki: zatrudnieni + bezrobotni.
    #[must_use]
    pub fn labour_force(&self) -> u32 {
        self.employed.saturating_add(self.unemployed)
    }

    /// Średnia umiejętność w roli, 0..=100. Rola bez ludzi zwraca 0 — nie „nie wiem",
    /// bo faza 2 mnoży przez tę liczbę i `None` musiałoby i tak zejść do zera.
    #[must_use]
    pub fn avg_skill(&self, role: usize) -> u32 {
        match self.labor.get(role).copied().unwrap_or(0) {
            0 => 0,
            n => self.skill_sum.get(role).copied().unwrap_or(0) / n,
        }
    }
}

impl HashState for MacroCell {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u16(self.key.0 .0);
        h.write_u8(self.key.1 .0);
        h.write_u32(self.citizens.len() as u32);
        for c in &self.citizens {
            h.write_u32(c.birth_index);
            h.write_u16(c.cell);
        }
        for a in self.age_hist {
            h.write_u32(a);
        }
        h.write_i64(self.cash.get());
        h.write_i64(self.deposits.get());
        h.write_i64(self.debt.get());
        for w in self.wealth_q {
            h.write_i64(w.get());
        }
        h.write_u32(self.labor.len() as u32);
        for (l, s) in self.labor.iter().zip(&self.skill_sum) {
            h.write_u32(*l);
            h.write_u32(*s);
        }
        h.write_u32(self.employed);
        h.write_u32(self.unemployed);
        for n in self.need_sat {
            h.write_u8(n.get());
        }
        self.demand.hash_state(h);
    }
}

/// Firma w makro — **nie agregat**, ta sama firma co w mezo (`D21`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MacroFirm {
    pub id: FirmId,
    pub district: DistrictId,
    pub branch: BranchId,
    pub capital: Money,
    pub debt: Money,
    /// Zapas w jednostce natywnej towaru.
    pub stock: MacroStock,
    pub capacity_daily: Qty,
    pub utilization_bps: u16,
    pub employees: u32,
    pub wage_bill: Money,
    pub price: SparseVec<GoodId, Money>,
    /// Koszt jednostkowy netto per towar.
    ///
    /// **Dopisane do kształtu z kontraktu M10 §6** i to jest świadoma poprawka:
    /// faza 5 woła `kernel::next_price`, a ta bierze `unit_cost` jako pierwsze
    /// wejście. Bez tego pola cenę trzeba by wyprowadzać z niej samej przez marżę
    /// docelową — czyli zgadywać koszt z ceny, którą ten koszt ma dopiero ustalić.
    /// Wyprowadzenie ceny z ceny nie jest modelem, tylko pętlą.
    pub cost: SparseVec<GoodId, Money>,
    /// Skalarny **surogat** marki: wartość oczekiwana agregatu afinitetów.
    /// M7 go czyta i nie zmienia — marka jest §7.6, czyli M10.
    pub brand_stock: i32,
    /// Poziom, nie zbiór węzłów. M7 go czyta i nie zmienia — R&D to §7.7, czyli M10.
    pub tech: TechLevel,
    /// Stali dostawcy wybrani w fazie 4.
    pub suppliers: SmallVec<[(GoodId, FirmId); 8]>,
}

impl MacroFirm {
    /// Wartość, po której porządkuje się warianty: kapitał minus dług.
    ///
    /// Nie „prognozowany zysk" — ta liczba nigdy nie wychodzi z modelu na zewnątrz
    /// jako kwota (§5.10, `R15`), a służy wyłącznie do porównania dwóch przebiegów
    /// tego samego świata.
    #[must_use]
    pub fn equity(&self) -> Money {
        Money(self.capital.get().saturating_sub(self.debt.get()))
    }
}

impl HashState for MacroFirm {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.id.0.to_bits());
        h.write_u16(self.district.0);
        h.write_u16(self.branch.0);
        h.write_i64(self.capital.get());
        h.write_i64(self.debt.get());
        self.stock.hash_state(h);
        h.write_i64(self.capacity_daily.get());
        h.write_u16(self.utilization_bps);
        h.write_u32(self.employees);
        h.write_i64(self.wage_bill.get());
        h.write_u32(self.price.len() as u32);
        for (g, p) in self.price.iter() {
            h.write_u16(g.0);
            h.write_i64(p.get());
        }
        h.write_u32(self.cost.len() as u32);
        for (g, c) in self.cost.iter() {
            h.write_u16(g.0);
            h.write_i64(c.get());
        }
        h.write_u32(self.brand_stock as u32);
        h.write_u8(self.tech.0);
        h.write_u32(self.suppliers.len() as u32);
        for (g, f) in &self.suppliers {
            h.write_u16(g.0);
            h.write_u64(f.0.to_bits());
        }
    }
}

/// Zdjęcie świata w rozdzielczości makro.
///
/// Klonowalne z rozmysłu: „co jeśli" robi się **na klonie**, a `lift()` woła się
/// raz na kwartał dla całego miasta (§5.10, budżet). Metropolia ≤ 8 MB, więc
/// dziesięć równoległych scenariuszy to 80 MB.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MacroState {
    pub tick: Tick,
    pub day: u32,
    /// Posortowane po kluczu — determinizm iteracji.
    pub cells: Vec<MacroCell>,
    /// Posortowane po `FirmId`.
    pub firms: Vec<MacroFirm>,
    pub commute: CommuteMatrix,
    pub epoch: EpochState,
    pub ledger: MacroLedger,
    /// Ziarno, którym scalono klasy — potrzebne, żeby odczytać `key.1`.
    pub grain: ClassGrain,
    /// Liczba ról w `labor`/`skill_sum` każdej komórki.
    pub roles: usize,
    /// Wstrząs w skali miasta, o ile akurat trwa (faza 8, M10a).
    ///
    /// **Jeden, nie lista** — uzasadnienie w `step::events`: dwa wstrząsy naraz
    /// wymagałyby składania mnożników popytu, czyli modelu cyklu koniunkturalnego,
    /// a nie zdarzenia.
    pub shock: Option<MacroShock>,
}

/// Rodzaj wstrząsu w skali miasta. Trzy warianty, bo trzy mają sens w agregacie
/// dzielnica × klasa; kolejność jest kontraktem, bo indeksuje klucz tekstu
/// we wpisie kronikarskim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShockKind {
    /// Popyt gospodarstw spada.
    Recession,
    /// Popyt gospodarstw rośnie.
    Boom,
    /// Przerób zakładów spada — wsad drożeje albo go nie ma.
    SupplyShock,
}

impl ShockKind {
    #[must_use]
    pub const fn as_index(self) -> usize {
        match self {
            ShockKind::Recession => 0,
            ShockKind::Boom => 1,
            ShockKind::SupplyShock => 2,
        }
    }

    /// Klucz tekstowy do kroniki i do raportu dry-runu.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            ShockKind::Recession => "recession",
            ShockKind::Boom => "boom",
            ShockKind::SupplyShock => "supply_shock",
        }
    }
}

/// Trwający wstrząs: co, jak mocno i do której doby.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MacroShock {
    pub kind: ShockKind,
    /// Korekta w punktach bazowych — ujemna dla recesji i szoku podażowego.
    pub magnitude_bp: i32,
    pub started_day: u32,
    pub ends_day: u32,
}

impl MacroState {
    #[must_use]
    pub fn empty(grain: ClassGrain, roles: usize, districts: u16) -> MacroState {
        MacroState {
            tick: Tick(0),
            day: 0,
            cells: Vec::new(),
            firms: Vec::new(),
            commute: CommuteMatrix::flat(districts, 0),
            epoch: EpochState::default(),
            ledger: MacroLedger::default(),
            grain,
            roles,
            shock: None,
        }
    }

    /// Korekta popytu z trwającego wstrząsu, w punktach bazowych. Zero, gdy nic
    /// się nie dzieje — a wtedy faza 4 liczy dokładnie to, co liczyła przed M10a.
    #[must_use]
    pub fn demand_shift_bp(&self) -> i32 {
        match self.shock {
            Some(s) if s.kind != ShockKind::SupplyShock => s.magnitude_bp,
            _ => 0,
        }
    }

    /// Korekta przerobu z trwającego wstrząsu, w punktach bazowych.
    #[must_use]
    pub fn supply_shift_bp(&self) -> i32 {
        match self.shock {
            Some(s) if s.kind == ShockKind::SupplyShock => s.magnitude_bp,
            _ => 0,
        }
    }

    /// Komórka po kluczu. Wyszukiwanie binarne — `cells` jest posortowane.
    #[must_use]
    pub fn cell(&self, key: (DistrictId, ClassId)) -> Option<&MacroCell> {
        self.cells
            .binary_search_by_key(&(key.0 .0, key.1 .0), |c| (c.key.0 .0, c.key.1 .0))
            .ok()
            .map(|i| &self.cells[i])
    }

    /// Indeks firmy po `FirmId` — `firms` jest posortowane po tym kluczu.
    #[must_use]
    pub fn firm_index(&self, id: FirmId) -> Option<usize> {
        self.firms
            .binary_search_by_key(&id.0.to_bits(), |f| f.id.0.to_bits())
            .ok()
    }

    #[must_use]
    pub fn population(&self) -> u32 {
        self.cells
            .iter()
            .map(MacroCell::population)
            .fold(0u32, u32::saturating_add)
    }

    #[must_use]
    pub fn employed(&self) -> u32 {
        self.cells
            .iter()
            .map(|c| c.employed)
            .fold(0u32, u32::saturating_add)
    }

    #[must_use]
    pub fn unemployed(&self) -> u32 {
        self.cells
            .iter()
            .map(|c| c.unemployed)
            .fold(0u32, u32::saturating_add)
    }

    /// Bezrobocie w promilach siły roboczej.
    #[must_use]
    pub fn unemployment_permille(&self) -> u16 {
        let sila = self.employed().saturating_add(self.unemployed());
        if sila == 0 {
            return 0;
        }
        ((u64::from(self.unemployed()) * 1000) / u64::from(sila)) as u16
    }

    /// Suma pieniądza w modelu — liczba, która przez cały krok nie ma prawa drgnąć.
    #[must_use]
    pub fn money(&self) -> Money {
        self.ledger.total()
    }

    /// Suma masy wszystkich zapasów firm — druga strona tego samego testu.
    #[must_use]
    pub fn mass(&self) -> i64 {
        self.firms
            .iter()
            .map(|f| f.stock.total())
            .fold(0i64, i64::saturating_add)
    }
}

impl HashState for MacroState {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.tick.0);
        h.write_u32(self.day);
        h.write_u32(self.cells.len() as u32);
        for c in &self.cells {
            c.hash_state(h);
        }
        h.write_u32(self.firms.len() as u32);
        for f in &self.firms {
            f.hash_state(h);
        }
        self.commute.hash_state(h);
        h.write_u16(self.epoch.key);
        self.ledger.hash_state(h);
        h.write_u8(self.grain.classes());
        h.write_u32(self.roles as u32);
        // Wstrząs wchodzi do hasha, bo zmienia popyt i przerób — dwa przebiegi
        // tego samego ziarna, w których wypadł w różnych dobach, **mają** się
        // różnić hashem, a nie tylko wynikiem.
        match self.shock {
            None => h.write_u8(0),
            Some(s) => {
                h.write_u8(1 + s.kind.as_index() as u8);
                h.write_u32(s.magnitude_bp as u32);
                h.write_u32(s.started_day);
                h.write_u32(s.ends_day);
            }
        }
    }
}
