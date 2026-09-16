//! Firma jako osoba prawna (M7a WP1, M7 §5.2, PRD §7.1).
//!
//! **Czym firma nie jest:** encją ECS. Cały jej stan siedzi w rejestrze [`crate::Firms`],
//! bo do firmy dociera się zawsze przez klucz albo przez zakład, a nigdy przekrojowo
//! po archetypach — ten sam argument, który wypchnął partie i oferty do aren (`K-16`).
//! Tu jednak rotacja jest niska (firma żyje latami), więc arena z generacjami nie
//! zarabia na siebie i rejestr jest zwykłą `BTreeMap`, tak jak `Plant` w M6 i `Market` w M5.
//!
//! **Czego tu nie ma, i gdzie to jest:** pieniądz firmy. Konto prowadzi `Books` z M5
//! pod `AccountOwner::Firm(FirmId)` i to jest jedyne saldo — druga kopia gotówki
//! w `sim/firms` byłaby drugą prawdą o tej samej liczbie. Kredyty, leasingi i obligacje
//! dokłada M7d, osobowość i strategia M7e, polityka M7c.

use magnat_core::hash::{HashState, StateHasher};
use magnat_core::{CitizenId, DecisionReason, DistrictId, GoodId, SimMinute, SiteId, Tick};
use smallvec::SmallVec;

use crate::key::FirmKey;
use crate::ring::Ring;

/// Kto jest właścicielem udziału. `Player` i `City` są wariantami bez identyfikatora,
/// bo gracz jest jeden, a miasto jest jedno.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Owner {
    Citizen(CitizenId),
    /// Spółka córka — udziałowcem jest inna firma.
    Firm(FirmKey),
    Player,
    City,
    /// Sieć zewnętrzna wchodząca do miasta (M7f). Identyfikator sieci nadaje M7f;
    /// do tego czasu wariant niesie sam fakt zewnętrzności.
    External,
}

impl HashState for Owner {
    fn hash_state(&self, h: &mut StateHasher) {
        match self {
            Owner::Citizen(c) => {
                h.write_u8(0);
                c.0.hash_state(h);
            }
            Owner::Firm(k) => {
                h.write_u8(1);
                k.hash_state(h);
            }
            Owner::Player => h.write_u8(2),
            Owner::City => h.write_u8(3),
            Owner::External => h.write_u8(4),
        }
    }
}

/// Udział we własności w punktach bazowych. Suma udziałów firmy **musi** wynosić
/// 10 000 — pilnuje tego [`Firm::owners_sum_ok`] i test własnościowy.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct OwnerShare {
    pub owner: Owner,
    pub bp: u16,
}

impl HashState for OwnerShare {
    fn hash_state(&self, h: &mut StateHasher) {
        self.owner.hash_state(h);
        h.write_u16(self.bp);
    }
}

/// Stan firmy. Warianty upadłościowe dokłada M7d — tu są dwa, które M7a rozróżnia,
/// bo zamknięty zakład nie dostaje wypłaty.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(u8)]
pub enum FirmStatus {
    #[default]
    Active = 0,
    Closed = 1,
}

impl HashState for FirmStatus {
    #[inline]
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(*self as u8);
    }
}

/// Wpis dziennika decyzji: co i kiedy. Powód jest strukturalny (`DecisionReason`,
/// dokument 00 §7) — tekst powstaje dopiero w warstwie prezentacji, z lokalizacją.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LoggedDecision {
    pub tick: Tick,
    pub reason: DecisionReason,
}

impl HashState for LoggedDecision {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.tick.0);
        self.reason.hash_state(h);
    }
}

/// Dziennik ostatnich 32 decyzji firmy (M7 §5.11). Pierwszym pisarzem jest M7b —
/// M7a buduje mechanizm i wpina go do hasha, bo dopisanie go później znaczyłoby
/// zmianę hasha stanu w środku fazy.
pub type DecisionLog = Ring<LoggedDecision, 32>;

/// Firma.
pub struct Firm {
    pub key: FirmKey,
    /// Nazwa proceduralna z `data/names/` — **nie jest** lokalizacją UI (CLAUDE.md),
    /// więc stoi tu tekstem, a nie kluczem.
    pub name: String,
    pub founded: SimMinute,
    /// Dyrektor-mieszkaniec. `None` znaczy zarząd tymczasowy albo firmę zewnętrzną
    /// (decyzja otwarta `D15` fazy, wariant domyślny).
    pub director: Option<CitizenId>,
    pub owners: SmallVec<[OwnerShare; 4]>,
    pub hq_district: DistrictId,
    pub sites: SmallVec<[SiteId; 8]>,
    /// Portfel produktów — wyjścia receptur zakładów firmy. Zmienia go dopiero
    /// decyzja asortymentowa M7e; tu jest wynikiem tego, co firma odziedziczyła.
    pub products: SmallVec<[GoodId; 16]>,
    pub status: FirmStatus,
    pub log: DecisionLog,
}

impl Firm {
    #[must_use]
    pub fn new(
        key: FirmKey,
        name: String,
        founded: SimMinute,
        hq_district: DistrictId,
        owners: SmallVec<[OwnerShare; 4]>,
    ) -> Firm {
        Firm {
            key,
            name,
            founded,
            director: None,
            owners,
            hq_district,
            sites: SmallVec::new(),
            products: SmallVec::new(),
            status: FirmStatus::Active,
            log: DecisionLog::new(),
        }
    }

    /// Jedyny właściciel — skrót dla firmy jednoosobowej, czyli dla większości.
    #[must_use]
    pub fn sole_owner(
        key: FirmKey,
        name: String,
        founded: SimMinute,
        hq_district: DistrictId,
        owner: Owner,
    ) -> Firm {
        let mut o = SmallVec::new();
        o.push(OwnerShare {
            owner,
            bp: Self::SHARES_TOTAL,
        });
        Firm::new(key, name, founded, hq_district, o)
    }

    /// Suma udziałów właścicielskich w punktach bazowych.
    pub const SHARES_TOTAL: u16 = 10_000;

    /// Niezmiennik własności. Osobna funkcja, bo pyta o nią test własnościowy
    /// i pytać będzie M7d przy podziale masy upadłościowej.
    #[must_use]
    pub fn owners_sum_ok(&self) -> bool {
        self.owners.iter().map(|s| u32::from(s.bp)).sum::<u32>() == u32::from(Firm::SHARES_TOTAL)
    }

    pub fn log_decision(&mut self, tick: Tick, reason: DecisionReason) {
        self.log.push(LoggedDecision { tick, reason });
    }
}

impl HashState for Firm {
    fn hash_state(&self, h: &mut StateHasher) {
        self.key.hash_state(h);
        h.write_u32(self.name.len() as u32);
        h.write(self.name.as_bytes());
        self.founded.hash_state(h);
        self.director.map(|c| c.0).hash_state(h);
        h.write_u32(self.owners.len() as u32);
        for o in &self.owners {
            o.hash_state(h);
        }
        self.hq_district.hash_state(h);
        h.write_u32(self.sites.len() as u32);
        for s in &self.sites {
            s.0.hash_state(h);
        }
        h.write_u32(self.products.len() as u32);
        for g in &self.products {
            g.hash_state(h);
        }
        self.status.hash_state(h);
        self.log.hash_state(h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::Entity;
    use std::num::NonZeroU32;

    fn firma() -> Firm {
        Firm::sole_owner(
            FirmKey(1),
            "Młyn Nowak".to_owned(),
            SimMinute(0),
            DistrictId(3),
            Owner::Citizen(CitizenId(Entity::new(5, NonZeroU32::MIN))),
        )
    }

    #[test]
    fn udzialy_sumuja_sie_do_calosci() {
        assert!(firma().owners_sum_ok());
    }

    #[test]
    fn niepelne_udzialy_sa_wykrywane() {
        let mut f = firma();
        f.owners[0].bp = 9_999;
        assert!(!f.owners_sum_ok());
    }

    #[test]
    fn dziennik_trzyma_ostatnie_trzydziesci_dwie_decyzje() {
        let mut f = firma();
        for i in 0..40u64 {
            f.log_decision(Tick(i), DecisionReason::Unspecified);
        }
        assert_eq!(f.log.len(), 32);
        assert_eq!(f.log.iter().next().expect("niepusty").tick, Tick(8));
    }
}
