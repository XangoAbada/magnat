//! Instrumenty finansowe firmy poza kredytem: leasing i obligacja (M7d §5.12, PRD §7.8).
//!
//! Kredyt ma własny moduł od M5d ([`crate::credit`]) i M7d go **nie przepisuje** —
//! dokłada mu wyłącznie trzeci produkt (`LoanKind::Investment`). Tutaj mieszka to,
//! czego kredytem nazwać nie można, bo różni się tym, co zostaje po drugiej stronie:
//!
//! - **leasing** nie daje firmie rzeczy, tylko jej używanie. Rzecz należy do
//!   leasingodawcy aż do wykupu i w upadłości **do masy nie wchodzi** — to jest cała
//!   różnica między leasingiem a kredytem pod zastaw i jedyny powód, dla którego
//!   `sim/economy` w ogóle musi wiedzieć, czym jest aktywo;
//! - **obligacja** rozbija dług na wielu wierzycieli, z których każdy jest osobnym
//!   roszczeniem w upadłości i osobnym kontem przy wypłacie kuponu.
//!
//! Żaden z nich **nie tworzy pieniądza**. Kreacja jest wyłącznie po stronie kredytu
//! bankowego ([`crate::Books::create_credit`]) — leasing i obligacja przesuwają
//! pieniądz, który już jest, i dlatego idą zwykłym [`crate::Books::transfer`].

use magnat_core::{FirmId, HouseholdId, Money, SiteId, StateHasher, Tick};

use crate::books::AccountId;

/// Rodzaj aktywa firmy. Dwa warianty, bo tyle firma w tym świecie ma.
///
/// Plan §5.12 zapowiadał także maszyny jako osobne aktywa; M6 nie prowadzi
/// egzemplarzy maszyn, tylko klasy w recepturze (`MachineClassId`), więc maszyna
/// jest częścią wyposażenia zakładu i nie ma czego wskazać. Trzeci wariant powstanie
/// wtedy, gdy powstaną egzemplarze — nie wcześniej.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum AssetKind {
    /// Wyposażenie zakładu: nakład inwestycyjny minus umorzenie.
    Equipment,
    /// Zapas towaru wyceniony po koszcie nabycia.
    Inventory,
}

impl AssetKind {
    #[must_use]
    pub const fn as_index(self) -> usize {
        match self {
            AssetKind::Equipment => 0,
            AssetKind::Inventory => 1,
        }
    }
}

/// Wskazanie aktywa. Zakład plus rodzaj — bo aktywo w tym świecie zawsze stoi
/// w zakładzie i nie da się go wskazać inaczej niż przez niego.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct AssetRef {
    pub site: SiteId,
    pub kind: AssetKind,
}

impl AssetRef {
    #[must_use]
    pub const fn new(site: SiteId, kind: AssetKind) -> AssetRef {
        AssetRef { site, kind }
    }

    pub(crate) fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.site.entity().index());
        h.write_u8(self.kind.as_index() as u8);
    }
}

/// Uchwyt umowy leasingowej. Indeks w [`super::CorpFinance`] i nigdy nie jest
/// zwalniany — umowa zakończona zostaje w rejestrze, bo historia firmy o nią pyta.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct LeaseId(pub u32);

/// Umowa leasingowa (M7d §5.12).
///
/// `asset` **należy do leasingodawcy**, nie do leasingobiorcy, i to jest jedyna
/// rzecz, o którą w tej strukturze naprawdę chodzi. Wszystko inne jest ratą.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Lease {
    pub id: LeaseId,
    /// Kto używa rzeczy i płaci raty.
    pub lessee: FirmId,
    /// Kto jest jej właścicielem do czasu wykupu.
    pub lessor: FirmId,
    /// Rachunek leasingodawcy — na niego idą raty.
    pub lessor_account: AccountId,
    pub asset: AssetRef,
    pub monthly: Money,
    pub months_left: u16,
    /// Cena wykupu po ostatniej racie. Do wykupu rzecz nie jest majątkiem firmy.
    pub buyout: Money,
    /// Ile rat z rzędu zostało bez zapłaty. Próg z danych kończy umowę.
    pub missed: u8,
    /// Umowa zakończona: wykupem, odbiorem rzeczy albo upadłością leasingobiorcy.
    pub ended: bool,
    /// Rzecz została **wykupiona** i od tej chwili należy do leasingobiorcy.
    ///
    /// Osobno od `ended`, bo to są dwa różne zdania o tej samej umowie i mylenie ich
    /// kosztuje niezmiennik 3 z M7 §7.2: upadłość kończy wszystkie umowy firmy,
    /// więc gdyby „skończona" znaczyło „moja", każda rzecz leasingowana wchodziłaby
    /// do masy dokładnie w tej chwili, w której najbardziej nie powinna.
    pub bought_out: bool,
}

impl Lease {
    /// Czy umowa jeszcze biegnie — czyli czy w tym miesiącu wypada rata.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        !self.ended && self.months_left > 0
    }

    /// Czy rzecz nadal należy do leasingodawcy.
    ///
    /// To, a nie [`Lease::is_active`], rozstrzyga o wejściu do masy upadłościowej:
    /// umowa zerwana za niepłacenie jest skończona, a rzecz i tak nie jest firmy.
    #[must_use]
    pub const fn lessor_owns(&self) -> bool {
        !self.bought_out
    }

    pub(crate) fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.id.0);
        h.write_u32(self.lessee.entity().index());
        h.write_u32(self.lessor.entity().index());
        self.asset.hash_state(h);
        h.write_i64(self.monthly.get());
        h.write_u16(self.months_left);
        h.write_i64(self.buyout.get());
        h.write_u8(self.missed);
        h.write_u8(u8::from(self.ended));
        h.write_u8(u8::from(self.bought_out));
    }
}

/// Uchwyt emisji obligacji. Jak [`LeaseId`]: indeks, nigdy nie zwalniany.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct BondId(pub u32);

/// Kto objął pakiet obligacji.
///
/// Gospodarstwo domowe **nie ma konta w `Books`** (saldo siedzi w komponencie
/// `Household`, M5b), więc jego pakiet rozlicza się kanałem sektora gospodarstw,
/// a nie przelewem. Rozróżnienie musi być w typie, bo od niego zależy, która
/// funkcja `Books` obsłuży kupon.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum BondHolder {
    Firm(FirmId, AccountId),
    Household(HouseholdId),
}

impl BondHolder {
    pub(crate) fn hash_state(&self, h: &mut StateHasher) {
        match self {
            BondHolder::Firm(f, _) => {
                h.write_u8(0);
                h.write_u32(f.entity().index());
            }
            BondHolder::Household(hh) => {
                h.write_u8(1);
                h.write_u32(hh.entity().index());
            }
        }
    }
}

/// Emisja obligacji firmy (M7d §5.12).
///
/// Rynek wtórny należy do M10 — tutaj pakiet zostaje u pierwszego nabywcy aż do
/// wykupu. `holders` jest posortowane przy zamknięciu emisji i od tej chwili się
/// nie przestawia, bo kolejność wypłat kuponu wchodzi do hasha stanu (00 §3.2).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Bond {
    pub id: BondId,
    pub issuer: FirmId,
    /// Rachunek emitenta — stąd wychodzą kupony i wykup.
    pub issuer_account: AccountId,
    /// Wartość nominalna całej emisji.
    pub face: Money,
    pub coupon_bp: u16,
    /// Kiedy przypada wykup.
    pub matures: Tick,
    /// Kto ile objął. Suma kwot równa się `face` **co do grosza** — pilnuje tego
    /// [`super::CorpFinance::issue_bond`], bo inaczej wykup nie domknąłby emisji.
    pub holders: Vec<(BondHolder, Money)>,
    pub redeemed: bool,
}

impl Bond {
    /// Kupon miesięczny całej emisji. Rok ma dwanaście miesięcy po 30 dób (`K-1`),
    /// więc dzielenie przez 12 jest dokładne i nie ma konwencji dziennej.
    #[must_use]
    pub fn monthly_coupon(&self) -> Money {
        self.face.mul_ratio(i64::from(self.coupon_bp), 10_000 * 12)
    }

    pub(crate) fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.id.0);
        h.write_u32(self.issuer.entity().index());
        h.write_i64(self.face.get());
        h.write_u16(self.coupon_bp);
        h.write_u64(self.matures.get());
        h.write_u64(self.holders.len() as u64);
        for (who, kwota) in &self.holders {
            who.hash_state(h);
            h.write_i64(kwota.get());
        }
        h.write_u8(u8::from(self.redeemed));
    }
}
