//! Postępowanie upadłościowe (M7d WP9, §5.13, PRD §7.8).
//!
//! **Właścicielem całej upadłości jest M7** (`K-10`). Inne fazy są wyłącznie
//! wierzycielami zgłaszającymi roszczenie przez [`Bankruptcy::file_claim`] —
//! w szczególności M8 (urząd skarbowy) nie ma i nie będzie miał własnej ścieżki
//! egzekucji. Dlatego postępowanie jest od początku otwarte na wierzycieli
//! z zewnątrz, choć w M7d nikt taki jeszcze nie istnieje.
//!
//! ## Co jest tu czyste, a co nie
//!
//! Podział masy — [`Bankruptcy::plan_distribution`] — jest **funkcją czystą** nad
//! stanem postępowania i nie dotyka ani `Books`, ani ECS. To nie jest estetyka:
//! niezmienniki 5, 6 i 7 z M7 §7.2 (kolejność priorytetów, reszta co do grosza,
//! determinizm) mówią o **planie wypłat**, a nie o przelewach, więc property-test
//! ma je sprawdzać na dziesięciu tysiącach przypadków bez budowania świata.
//! Wykonanie planu na kontach robi [`super::CorpFinance`], bo ono ma `Books`.

use magnat_core::{
    split_proportional, BankruptcyTrigger, ClaimPriority, DecisionReason, FirmId, Money, SimMinute,
    StateHasher, Tick, CLAIM_PRIORITY_COUNT,
};

use crate::books::{AccountId, AccountOwner};
use crate::corpfin::arrears::{hash_owner, ArrearId, ClaimOrigin};
use crate::corpfin::instruments::AssetRef;

/// Etap postępowania. Kolejność jest jednokierunkowa i nie ma z niej powrotu —
/// postępowanie ma być skończone, bo inaczej firma w upadłości zostaje w świecie
/// na zawsze i blokuje swoje zakłady.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BankruptcyStage {
    /// Zgłoszenie: załoga rozwiązana, produkcja wstrzymana, leasingi oddane.
    Filed,
    /// Wycena lotów masy.
    Valuation,
    /// Wyprzedaż w rundach z rosnącym dyskontem.
    Auction {
        round: u8,
    },
    /// Podział wpływów wg [`ClaimPriority`].
    Distribution,
    Closed,
}

impl BankruptcyStage {
    #[must_use]
    pub const fn as_index(self) -> usize {
        match self {
            BankruptcyStage::Filed => 0,
            BankruptcyStage::Valuation => 1,
            BankruptcyStage::Auction { .. } => 2,
            BankruptcyStage::Distribution => 3,
            BankruptcyStage::Closed => 4,
        }
    }
}

/// Co stało się z lotem. Każdy lot kończy w **dokładnie jednym** z tych stanów
/// i to jest niezmiennik 2 z M7 §7.2 — suma liczności stanów równa się liczbie
/// lotów, a przecięcia są puste.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LotFate {
    /// Jeszcze w masie, czeka na rundę.
    InEstate,
    Sold {
        to: AccountOwner,
        price: Money,
    },
    /// Rzecz leasingowana wróciła do właściciela i do masy nie weszła.
    ReturnedToLessor,
    /// Nikt nie kupił nawet po wartości złomu. **Jawny odpis**, nie zniknięcie.
    WrittenOff,
}

/// Lot masy upadłościowej.
///
/// Plan §5.13 zapowiadał wystawianie lotów jako zwykłych [`crate::Offer`] na rynku
/// M5. To się nie da i nie powinno: `Offer` jest ceną **półkową towaru** dla
/// gospodarstwa domowego, indeksuje się po `CategoryId::Stock(StockCat)`, a tokarka
/// nie ma `StockCat` i mieszkaniec jej nie kupi. To jest dokładnie ten sam argument,
/// którym `K-36` odrzucił `Quote` jako widok na `Offer`. Lot jest więc własną
/// pozycją z ceną rundy, a `bid_lot` jest jednym wejściem zakupu — dla AI firm
/// (M7e) i dla gracza (M9) tym samym.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AssetLot {
    pub asset: AssetRef,
    /// Wycena z etapu `Valuation` — punkt wyjścia dla rund, nie cena.
    pub valuation: Money,
    pub fate: LotFate,
}

impl AssetLot {
    /// Cena w danej rundzie: wycena minus dyskonto rundy.
    #[must_use]
    pub fn price_in_round(&self, discount_bp: i64) -> Money {
        self.valuation
            .mul_ratio(10_000 - discount_bp.clamp(0, 10_000), 10_000)
    }

    fn hash_state(&self, h: &mut StateHasher) {
        self.asset.hash_state(h);
        h.write_i64(self.valuation.get());
        match self.fate {
            LotFate::InEstate => h.write_u8(0),
            LotFate::Sold { to, price } => {
                h.write_u8(1);
                hash_owner(&to, h);
                h.write_i64(price.get());
            }
            LotFate::ReturnedToLessor => h.write_u8(2),
            LotFate::WrittenOff => h.write_u8(3),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ClaimId(pub u32);

/// Roszczenie zgłoszone do masy.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Claim {
    pub id: ClaimId,
    pub creditor: AccountOwner,
    pub priority: ClaimPriority,
    pub amount: Money,
    /// Ile z tego wypłacono. Do `Distribution` zawsze zero.
    pub paid: Money,
    /// Zabezpieczenie rzeczowe. `Some` znaczy `Secured` **do wartości
    /// zabezpieczenia**; nadwyżkę [`Bankruptcy::file_claim`] rozbija na drugie,
    /// jawne roszczenie `Unsecured` — nie chowa jej po cichu w pierwszym.
    pub collateral: Option<AssetRef>,
    pub filed: SimMinute,
    pub origin: ClaimOrigin,
    /// Zaległość, z której to roszczenie powstało — żeby domknięcie postępowania
    /// zamknęło też rejestr zaległości i dług nie żył dalej po śmierci firmy.
    pub arrear: Option<ArrearId>,
}

impl Claim {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.id.0);
        hash_owner(&self.creditor, h);
        h.write_u8(self.priority.as_index() as u8);
        h.write_i64(self.amount.get());
        h.write_i64(self.paid.get());
        h.write_u8(u8::from(self.collateral.is_some()));
        if let Some(a) = self.collateral {
            a.hash_state(h);
        }
        h.write_u64(self.filed.0);
        self.origin.hash_state(h);
    }
}

/// Dlaczego roszczenie nie weszło do masy.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClaimRejected {
    /// Postępowanie jest już na etapie podziału albo zamknięte — termin jest twardy,
    /// bo inaczej podział nigdy by się nie skończył (i nie byłby deterministyczny).
    TooLate,
    /// Kwota niedodatnia.
    NonPositive,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct BankruptcyId(pub u32);

/// Postępowanie upadłościowe jednej firmy.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Bankruptcy {
    pub id: BankruptcyId,
    pub firm: FirmId,
    /// Rachunek upadłego — z niego idą wypłaty i na niego wpływają ceny lotów.
    pub account: AccountId,
    pub opened: SimMinute,
    pub trigger: BankruptcyTrigger,
    /// Ile dób firma nie płaciła, zanim sąd otworzył postępowanie. Ma znaczenie
    /// tylko przy `BankruptcyTrigger::Illiquid` — przy pozostałych jest zerem,
    /// bo tamte nie mierzą się czasem. Ładunek [`DecisionReason::BankruptcyOpened`].
    pub trigger_days: u16,
    pub stage: BankruptcyStage,
    /// Sortowane po `(kind, site)` przy wejściu w `Valuation` — determinizm (00 §3.2).
    pub estate: Vec<AssetLot>,
    /// Sortowane po `(priority, creditor, id)` przy wejściu w `Distribution`.
    pub claims: Vec<Claim>,
    /// Wpływy ze sprzedaży lotów. Gotówka, którą firma miała przed zgłoszeniem,
    /// tu **nie wchodzi** — ona po prostu leży na jej koncie.
    pub proceeds: Money,
    pub trustee_fee_bp: u16,
    /// Kiedy postępowanie weszło w bieżący etap; stąd liczy się długość rundy.
    pub stage_since: Tick,
}

impl Bankruptcy {
    #[must_use]
    pub fn new(
        id: BankruptcyId,
        firm: FirmId,
        account: AccountId,
        trigger: BankruptcyTrigger,
        trustee_fee_bp: u16,
        t: Tick,
    ) -> Bankruptcy {
        Bankruptcy {
            id,
            firm,
            account,
            opened: SimMinute(t.get()),
            trigger,
            trigger_days: 0,
            stage: BankruptcyStage::Filed,
            estate: Vec::new(),
            claims: Vec::new(),
            proceeds: Money::ZERO,
            trustee_fee_bp,
            stage_since: t,
        }
    }

    #[must_use]
    pub const fn is_open(&self) -> bool {
        !matches!(self.stage, BankruptcyStage::Closed)
    }

    /// Powód otwarcia postępowania — do dziennika decyzji firmy i karty inspekcji.
    #[must_use]
    pub const fn reason(&self) -> DecisionReason {
        DecisionReason::BankruptcyOpened {
            trigger: self.trigger,
            days: self.trigger_days,
        }
    }

    /// JEDYNA droga zgłoszenia roszczenia. Woła ją M7 (płace, kredyty, leasingi),
    /// M6 (dostawcy), M8 (podatki, `K-10`), M10 (czynsz).
    ///
    /// Roszczenie zabezpieczone ponad wartość zabezpieczenia **rozpada się na dwa**:
    /// `Secured` do wartości zabezpieczenia i `Unsecured` na nadwyżkę. Bank nie ma
    /// dostawać z pierwszego priorytetu więcej, niż wart jest jego zastaw — a bez
    /// tego rozbicia dostawałby, i nikt by tego nie zobaczył.
    pub fn file_claim(
        &mut self,
        creditor: AccountOwner,
        amount: Money,
        origin: ClaimOrigin,
        collateral: Option<(AssetRef, Money)>,
        arrear: Option<ArrearId>,
        t: Tick,
    ) -> Result<ClaimId, ClaimRejected> {
        if matches!(
            self.stage,
            BankruptcyStage::Distribution | BankruptcyStage::Closed
        ) {
            return Err(ClaimRejected::TooLate);
        }
        if amount.get() <= 0 {
            return Err(ClaimRejected::NonPositive);
        }
        let filed = SimMinute(t.get());
        match collateral {
            Some((asset, wartosc)) if wartosc.get() > 0 => {
                let zabezpieczone = Money(amount.get().min(wartosc.get()));
                let id = self.push_claim(Claim {
                    id: ClaimId(0),
                    creditor,
                    priority: origin.priority(true),
                    amount: zabezpieczone,
                    paid: Money::ZERO,
                    collateral: Some(asset),
                    filed,
                    origin,
                    arrear,
                });
                let nadwyzka = Money(amount.get() - zabezpieczone.get());
                if nadwyzka.get() > 0 {
                    self.push_claim(Claim {
                        id: ClaimId(0),
                        creditor,
                        priority: origin.priority(false),
                        amount: nadwyzka,
                        paid: Money::ZERO,
                        collateral: None,
                        filed,
                        origin,
                        arrear,
                    });
                }
                Ok(id)
            }
            _ => Ok(self.push_claim(Claim {
                id: ClaimId(0),
                creditor,
                priority: origin.priority(false),
                amount,
                paid: Money::ZERO,
                collateral: None,
                filed,
                origin,
                arrear,
            })),
        }
    }

    fn push_claim(&mut self, mut c: Claim) -> ClaimId {
        let id = ClaimId(u32::try_from(self.claims.len()).unwrap_or(u32::MAX));
        c.id = id;
        self.claims.push(c);
        id
    }

    /// Gotówka do podziału: to, co firma miała na koncie, plus wpływy z wyprzedaży.
    #[must_use]
    pub fn estate_cash(&self, cash_on_account: Money) -> Money {
        Money(cash_on_account.get().max(0))
    }

    /// Wynagrodzenie syndyka — liczone **od wpływów z wyprzedaży**, nie od całej
    /// masy. Syndyk dostaje za to, że coś sprzedał, a nie za to, że firma miała
    /// gotówkę na koncie.
    ///
    /// Przycięte do tego, co w masie faktycznie jest. Bez tego przycięcia
    /// postępowanie, w którym loty sprzedano, a gotówka zdążyła zejść na wypłaty,
    /// wypłacałoby syndykowi kwotę, której nie ma — i podział przestawałby się
    /// sumować do masy, czyli łamał niezmiennik 6 z M7 §7.2 w sposób niewidoczny
    /// dla żadnego pojedynczego przelewu.
    #[must_use]
    pub fn trustee_fee_of(&self, cash_on_account: Money) -> Money {
        let pelne = self
            .proceeds
            .mul_ratio(i64::from(self.trustee_fee_bp), 10_000);
        Money(pelne.get().min(cash_on_account.get().max(0)))
    }

    /// Plan wypłat. **Funkcja czysta** — nie dotyka kont i nie zmienia postępowania.
    ///
    /// Reguła jest jedna i wynika wprost z kolejności wariantów [`ClaimPriority`]:
    /// idziemy priorytetami w górę, a niższy nie dostaje **ani grosza**, dopóki
    /// wyższy nie jest zaspokojony w całości. Gdy na dany priorytet nie starcza,
    /// dzieli się proporcjonalnie przez [`split_proportional`], który sumuje się
    /// do kwoty dzielonej co do grosza — reszta idzie do pierwszego wierzyciela
    /// w ustalonym porządku (00 §2).
    #[must_use]
    pub fn plan_distribution(&self, cash_on_account: Money) -> Distribution {
        let oplata = self.trustee_fee_of(cash_on_account);
        let mut zostalo = Money(
            self.estate_cash(cash_on_account)
                .get()
                .saturating_sub(oplata.get())
                .max(0),
        );
        let mut wyplaty = vec![Money::ZERO; self.claims.len()];
        let mut ratio = [0u16; CLAIM_PRIORITY_COUNT];

        for p in ClaimPriority::ALL {
            // Kolejność w obrębie priorytetu jest kolejnością `claims`, a ta jest
            // ustalona przy wejściu w `Distribution` (sort po creditor, id) — więc
            // reszta z dzielenia trafia zawsze do tego samego wierzyciela.
            let idx: Vec<usize> = self
                .claims
                .iter()
                .enumerate()
                .filter(|(_, c)| c.priority == *p && c.amount.get() > 0)
                .map(|(i, _)| i)
                .collect();
            if idx.is_empty() {
                continue;
            }
            let suma: i64 = idx.iter().map(|i| self.claims[*i].amount.get()).sum();
            if suma <= 0 {
                continue;
            }
            if zostalo.get() >= suma {
                for i in &idx {
                    wyplaty[*i] = self.claims[*i].amount;
                }
                zostalo = Money(zostalo.get() - suma);
                ratio[p.as_index()] = 10_000;
            } else {
                let wagi: Vec<u64> = idx
                    .iter()
                    .map(|i| u64::try_from(self.claims[*i].amount.get()).unwrap_or(0))
                    .collect();
                let podzial = split_proportional(zostalo, &wagi);
                for (k, i) in idx.iter().enumerate() {
                    wyplaty[*i] = podzial.get(k).copied().unwrap_or(Money::ZERO);
                }
                ratio[p.as_index()] =
                    u16::try_from(zostalo.get().saturating_mul(10_000) / suma).unwrap_or(10_000);
                zostalo = Money::ZERO;
            }
        }

        Distribution {
            payouts: wyplaty,
            trustee_fee: oplata,
            residual: zostalo,
            ratio_bp: ratio,
        }
    }

    pub(crate) fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.id.0);
        h.write_u32(self.firm.entity().index());
        h.write_u64(self.opened.0);
        h.write_u8(self.trigger.as_index() as u8);
        h.write_u16(self.trigger_days);
        h.write_u8(self.stage.as_index() as u8);
        if let BankruptcyStage::Auction { round } = self.stage {
            h.write_u8(round);
        }
        h.write_u64(self.estate.len() as u64);
        for l in &self.estate {
            l.hash_state(h);
        }
        h.write_u64(self.claims.len() as u64);
        for c in &self.claims {
            c.hash_state(h);
        }
        h.write_i64(self.proceeds.get());
        h.write_u16(self.trustee_fee_bp);
        h.write_u64(self.stage_since.get());
    }
}

/// Wynik [`Bankruptcy::plan_distribution`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Distribution {
    /// Wypłata na każde roszczenie, w kolejności `Bankruptcy::claims`.
    pub payouts: Vec<Money>,
    pub trustee_fee: Money,
    /// Co zostało po zaspokojeniu wszystkich priorytetów — trafia do właścicieli
    /// i w praktyce jest zerem.
    pub residual: Money,
    /// Stopień zaspokojenia per priorytet w punktach bazowych — ładunek powodu
    /// [`DecisionReason::ClaimSettled`].
    pub ratio_bp: [u16; CLAIM_PRIORITY_COUNT],
}

impl Distribution {
    #[must_use]
    pub fn total_paid(&self) -> Money {
        Money(self.payouts.iter().map(|m| m.get()).sum())
    }

    /// Powód wypłaty dla danego priorytetu — do dziennika decyzji firmy.
    #[must_use]
    pub fn reason(&self, p: ClaimPriority) -> DecisionReason {
        DecisionReason::ClaimSettled {
            priority: p,
            ratio_bp: self.ratio_bp[p.as_index()],
        }
    }
}
