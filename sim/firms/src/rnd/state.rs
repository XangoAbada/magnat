//! Stan R&D: co firma wie, co bada i co opatentowała (M10c §5.4).
//!
//! **Katalog jest wejściem, stan jest stanem.** `TechTree` i `RndTuning` nie zmieniają
//! się w przebiegu i nie wchodzą do hasha — tak samo jak `Catalog` w `ChainHandle`
//! (`AP-1`). Do hasha wchodzi wszystko, co niżej: projekty, wiedza, patenty i licencje,
//! bo każde z nich zmienia to, co świat zrobi w następnej dobie.
//!
//! **Czego tu nie ma: listy towarów w obiegu.** Kto jest na półce, wie `Market`,
//! bo to on ma półki, oferty i indeks kategorii — druga tablica po tej stronie
//! byłaby drugą prawdą o tej samej rzeczy. R&D **zgłasza** odblokowanie
//! (`RndOutcome::unlocked`), a wykonuje je `sim/economy`.

use magnat_core::hash::{HashState, StateHasher};
use magnat_core::{ContractId, Money, SimMinute, TechId};
use std::collections::BTreeMap;

use crate::key::FirmKey;

/// Projekt badawczy w toku.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Project {
    pub tech: TechId,
    /// Koszt do zebrania, w **milipunktach** badawczych. Zamrożony w chwili startu:
    /// węzeł, którego rok światowy minie w trakcie badań, nie tanieje w środku —
    /// firma podjęła decyzję na liczbie, którą wtedy widziała.
    pub cost_mrp: u64,
    pub done_mrp: u64,
    pub started: SimMinute,
    /// Udział opłaconego budżetu materiałowego w docelowym, w promilach.
    /// Przepisuje go `sim/economy` po rozliczeniu obciążenia z poprzedniego miesiąca.
    pub budget_permille: u16,
    /// Ile razy w tym projekcie wypadł przełom — do kroniki i do karty firmy.
    pub breakthroughs: u8,
}

impl Project {
    /// Nowy projekt: koszt w **punktach**, przeliczony na milipunkty tu, w jednym
    /// miejscu. Pierwszy miesiąc idzie pełnym tempem — obciążenie wychodzi dopiero
    /// na najbliższej granicy miesiąca i dopiero ono może tempo przyciąć.
    ///
    /// Wejście dla obu stron: wybiera projekt firma AI (`rnd::progress`) albo gracz
    /// komendą `StartResearch` (M10g). Jeden konstruktor, bo inaczej dwie ścieżki
    /// rozjechałyby się przy pierwszej zmianie pola.
    #[must_use]
    pub fn new(tech: TechId, cost_rp: u32, started: SimMinute) -> Project {
        Project {
            tech,
            cost_mrp: u64::from(cost_rp) * crate::rnd::progress::MRP,
            done_mrp: 0,
            started,
            budget_permille: 1000,
            breakthroughs: 0,
        }
    }

    #[must_use]
    pub fn remaining_mrp(&self) -> u64 {
        self.cost_mrp.saturating_sub(self.done_mrp)
    }

    /// Ile jeszcze miesięcy przy tym tempie. Zero tempa znaczy „nie wiadomo" i wraca
    /// jako sufit `u16`, a nie jako nieskończoność w typie bez niej.
    #[must_use]
    pub fn months_left(&self, mrp_per_day: u64) -> u16 {
        if mrp_per_day == 0 {
            return u16::MAX;
        }
        let dob = self.remaining_mrp().div_ceil(mrp_per_day);
        u16::try_from(dob.div_ceil(30)).unwrap_or(u16::MAX)
    }
}

impl HashState for Project {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u16(self.tech.0);
        h.write_u64(self.cost_mrp);
        h.write_u64(self.done_mrp);
        self.started.hash_state(h);
        h.write_u16(self.budget_permille);
        h.write_u8(self.breakthroughs);
    }
}

/// Patent: wyłączność na technologię odkrytą przed światem.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Patent {
    pub tech: TechId,
    pub owner: FirmKey,
    pub granted: SimMinute,
    pub expires: SimMinute,
}

impl Patent {
    #[must_use]
    pub fn active(&self, now: SimMinute) -> bool {
        now.0 < self.expires.0
    }

    /// Ile życia patentu zostało, w promilach jego pełnej długości. To z tej liczby
    /// wychodzi stawka royalty — patent wygasający za rok jest wart mniej niż świeży.
    #[must_use]
    pub fn life_left_permille(&self, now: SimMinute) -> u32 {
        let cala = self.expires.0.saturating_sub(self.granted.0);
        if cala == 0 {
            return 0;
        }
        let zostalo = self.expires.0.saturating_sub(now.0).min(cala);
        u32::try_from(zostalo * 1000 / cala).unwrap_or(1000)
    }
}

impl HashState for Patent {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u16(self.tech.0);
        self.owner.hash_state(h);
        self.granted.hash_state(h);
        self.expires.hash_state(h);
    }
}

/// Licencja patentowa. **Jest `ContractId` z M6, nie nowym typem umowy** (§5.4 pkt 3):
/// numer pochodzi z tego samego licznika co umowy dostawy
/// ([`magnat_supply::B2b::mint_contract_id`]), więc `Subject::Contract` prowadzi
/// dokładnie do jednej rzeczy.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct License {
    pub contract: ContractId,
    pub tech: TechId,
    pub licensor: FirmKey,
    pub licensee: FirmKey,
    /// Stawka od przychodu zakładów licencjobiorcy, w punktach bazowych.
    pub royalty_bp: u16,
    pub signed: SimMinute,
    /// Licencja gaśnie razem z patentem — po wygaśnięciu technologia jest darmowa.
    pub expires: SimMinute,
}

impl HashState for License {
    fn hash_state(&self, h: &mut StateHasher) {
        self.contract.entity().hash_state(h);
        h.write_u16(self.tech.0);
        self.licensor.hash_state(h);
        self.licensee.hash_state(h);
        h.write_u16(self.royalty_bp);
        self.signed.hash_state(h);
        self.expires.hash_state(h);
    }
}

/// Obciążenie, które R&D zostawia do zaksięgowania przez `sim/economy`.
///
/// Skrzynka, a nie wywołanie — ten sam wzorzec co [`crate::PayrollOutbox`]
/// i z tego samego powodu: `sim/firms` nie widzi `Books` i widzieć nie może,
/// bo zależność idzie `economy → firms`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RndCharge {
    pub firm: FirmKey,
    /// Zakład, którego księga przyjmie koszt. Badania księgują się tam, gdzie stoi
    /// laboratorium, a nie „w firmie" — firma nie ma księgi, mają ją zakłady.
    pub site: magnat_core::SiteId,
    pub amount: Money,
    /// Czego dotyczy wydatek: budżetu materiałowego projektu albo opłaty licencyjnej.
    pub kind: ChargeKind,
}

/// Rodzaj wydatku badawczego.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChargeKind {
    /// Budżet materiałowy miesiąca. `tech` mówi, na czym firma pracuje.
    Materials { tech: TechId },
    /// Opłata wstępna licencji.
    LicenseUpfront { tech: TechId, licensor: FirmKey },
    /// Royalty za miniony miesiąc.
    Royalty { tech: TechId, licensor: FirmKey },
}

/// Wiedza, projekty, patenty i licencje świata.
#[derive(Clone, Debug, Default)]
pub struct RndState {
    /// Co firma umie, rosnąco po `TechId`. `Vec`, nie zbiór bitowy: mediana firmy
    /// zna zero technologii, a węzłów jest kilkanaście.
    pub known: BTreeMap<FirmKey, Vec<TechId>>,
    pub projects: BTreeMap<FirmKey, Project>,
    /// Patenty per technologia. Jeden patent na węzeł — pierwszy, kto zdąży.
    pub patents: BTreeMap<TechId, Patent>,
    /// Licencje kluczowane indeksem `ContractId`, bo to on jest numerem umowy.
    pub licenses: BTreeMap<u32, License>,
}

impl RndState {
    #[must_use]
    pub fn knows(&self, f: FirmKey, t: TechId) -> bool {
        self.known
            .get(&f)
            .is_some_and(|v| v.binary_search(&t).is_ok())
    }

    pub fn learn(&mut self, f: FirmKey, t: TechId) {
        let v = self.known.entry(f).or_default();
        if let Err(i) = v.binary_search(&t) {
            v.insert(i, t);
        }
    }

    /// Kto blokuje firmie dostęp do technologii. `None` znaczy „droga wolna":
    /// patentu nie ma, wygasł, należy do tej firmy albo firma ma licencję.
    #[must_use]
    pub fn blocked_by(&self, f: FirmKey, t: TechId, now: SimMinute) -> Option<FirmKey> {
        let p = self.patents.get(&t)?;
        if !p.active(now) || p.owner == f {
            return None;
        }
        let ma_licencje = self
            .licenses
            .values()
            .any(|l| l.tech == t && l.licensee == f && now.0 < l.expires.0);
        if ma_licencje {
            None
        } else {
            Some(p.owner)
        }
    }
}

impl HashState for RndState {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.known.len() as u32);
        for (f, v) in &self.known {
            f.hash_state(h);
            h.write_u32(v.len() as u32);
            for t in v {
                h.write_u16(t.0);
            }
        }
        h.write_u32(self.projects.len() as u32);
        for (f, p) in &self.projects {
            f.hash_state(h);
            p.hash_state(h);
        }
        h.write_u32(self.patents.len() as u32);
        for (t, p) in &self.patents {
            h.write_u16(t.0);
            p.hash_state(h);
        }
        h.write_u32(self.licenses.len() as u32);
        for (i, l) in &self.licenses {
            h.write_u32(*i);
            l.hash_state(h);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patent_wlasciciela_nie_blokuje_wlasciciela() {
        let mut s = RndState::default();
        s.patents.insert(
            TechId(1),
            Patent {
                tech: TechId(1),
                owner: FirmKey(7),
                granted: SimMinute(0),
                expires: SimMinute(1_000),
            },
        );
        assert_eq!(s.blocked_by(FirmKey(7), TechId(1), SimMinute(10)), None);
        assert_eq!(
            s.blocked_by(FirmKey(9), TechId(1), SimMinute(10)),
            Some(FirmKey(7))
        );
        // Po wygaśnięciu technologia jest darmowa dla wszystkich.
        assert_eq!(s.blocked_by(FirmKey(9), TechId(1), SimMinute(2_000)), None);
    }

    #[test]
    fn stawka_royalty_maleje_z_zyciem_patentu() {
        let p = Patent {
            tech: TechId(0),
            owner: FirmKey(1),
            granted: SimMinute(0),
            expires: SimMinute(1_000),
        };
        assert_eq!(p.life_left_permille(SimMinute(0)), 1000);
        assert_eq!(p.life_left_permille(SimMinute(500)), 500);
        assert_eq!(p.life_left_permille(SimMinute(1_000)), 0);
    }
}
