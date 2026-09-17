//! Urząd jako kolejka: czas wydania pozwolenia jest wynikiem, nie parametrem
//! (M8d WP7, §5.2 dokumentu M8e).
//!
//! # Dlaczego to jest tutaj, skoro `Permit` opisuje §5.2
//!
//! Bo pakietem, który to obiecuje, jest **WP7**, a WP7 należy do M8d. Podział §5
//! poszedł za tematem („polityka, pozwolenia, przetargi, władza"), a podział pakietów
//! za zależnościami — i w tym jednym miejscu się rozjechały. Uchwały, przetargi
//! i wybory zostają w M8e; kolejka, obsada i czas oczekiwania są tutaj, bo to one
//! są kryterium ukończenia WP7.
//!
//! # Czas jest kosztem
//!
//! Przerób urzędu to `obsada × jednostki na urzędnika na dobę`, pomnożone przez zero
//! w dni wolne (`K-15`). Kolejka jest FIFO, a wniosek kosztuje tyle jednostek, ile
//! mówi jego rodzaj — ocena środowiskowa pięć razy tyle co przebudowa, więc stoi
//! w kolejce pięć razy dłużej. Skutek uboczny jest zamierzony: wniosek złożony
//! w piątek leży dwa dni, bo urząd ma weekend.
//!
//! Podwojenie obsady podwaja przerób i skraca medianę oczekiwania — to jest
//! kryterium ukończenia WP7 i jedyna własność, której ten moduł musi dowieść.

use magnat_core::{
    DayOfWeek, DecisionReason, DistrictId, HashState, Money, PermitKind, SiteId, StateHasher, Tick,
};

use crate::tuning::CityTuning;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct PermitId(pub u32);

/// Kto składa wniosek.
///
/// Dwa warianty, bo dwóch wnioskodawców istnieje: zakład otwierany przez firmę
/// i gracz. Mieszkaniec budujący dom przyjdzie razem z rynkiem nieruchomości
/// (M10), a wariant, którego nic nie ustawia, przechodzi każdy test (`R2`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Applicant {
    Site(SiteId),
    Player,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PermitStatus {
    Queued,
    UnderReview,
    Approved {
        at: Tick,
    },
    /// Wniosek leżał dłużej niż rok gry i przestał być sprawą w toku.
    Expired,
}

impl PermitStatus {
    #[must_use]
    pub fn is_open(self) -> bool {
        matches!(self, PermitStatus::Queued | PermitStatus::UnderReview)
    }

    fn tag(self) -> u8 {
        match self {
            PermitStatus::Queued => 0,
            PermitStatus::UnderReview => 1,
            PermitStatus::Approved { .. } => 2,
            PermitStatus::Expired => 3,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Permit {
    pub id: PermitId,
    pub applicant: Applicant,
    pub kind: PermitKind,
    pub filed_at: Tick,
    pub status: PermitStatus,
    pub fee: Money,
    pub office: u32,
    /// Ile jednostek przerobu zostało do wydania decyzji.
    pub units_left: u32,
}

impl Permit {
    /// Ile dób wniosek czekał. Dla wniosku otwartego — ile czeka do teraz.
    #[must_use]
    pub fn waited_days(&self, now: Tick) -> u16 {
        let koniec = match self.status {
            PermitStatus::Approved { at } => at.get(),
            _ => now.get(),
        };
        u16::try_from(koniec.saturating_sub(self.filed_at.get()) / 1_440).unwrap_or(u16::MAX)
    }
}

/// Urząd wydający pozwolenia — zwykły zakład publiczny z obsadą (M8d WP7).
#[derive(Clone, Debug)]
pub struct PermitOffice {
    pub site: SiteId,
    pub district: DistrictId,
    /// Obsada. Przepisuje ją co miesiąc ten sam krok, który liczy obsadę placówek —
    /// urząd jest placówką `ServiceKind::Office` i nie ma drugiej listy etatów.
    pub staff: u32,
    /// Dni robocze jako maska tygodnia (`K-15`). Bit `n` = `DayOfWeek::from_index(n)`.
    pub open_days: u8,
    /// Kolejka wniosków w kolejności złożenia.
    pub queue: Vec<PermitId>,
}

impl PermitOffice {
    /// Poniedziałek–piątek (`K-15`): bity 0..4 maski tygodnia.
    pub const WORKDAYS: u8 = 0b0001_1111;

    #[must_use]
    pub fn new(site: SiteId, district: DistrictId) -> PermitOffice {
        PermitOffice {
            site,
            district,
            staff: 0,
            open_days: PermitOffice::WORKDAYS,
            queue: Vec::new(),
        }
    }

    #[must_use]
    pub fn open_on(&self, day: DayOfWeek) -> bool {
        self.open_days & (1 << day as u8) != 0
    }
}

/// Wnioski i urzędy, które je przerabiają.
#[derive(Clone, Default, Debug)]
pub struct PermitRegistry {
    permits: Vec<Permit>,
    offices: Vec<PermitOffice>,
    next: u32,
    /// Czasy oczekiwania wydanych pozwoleń, w dobach — pierścień, bo to jest
    /// metryka, a nie historia. Mediana z niego jest kryterium WP7.
    waits: Vec<u16>,
    issued: u32,
    expired: u32,
}

/// Ile ostatnich czasów oczekiwania pamięta rejestr.
const WAIT_RING: usize = 1_024;

impl PermitRegistry {
    #[must_use]
    pub fn new(offices: Vec<PermitOffice>) -> PermitRegistry {
        PermitRegistry {
            permits: Vec::new(),
            offices,
            next: 1,
            waits: Vec::new(),
            issued: 0,
            expired: 0,
        }
    }

    #[must_use]
    pub fn offices(&self) -> &[PermitOffice] {
        &self.offices
    }

    pub fn offices_mut(&mut self) -> &mut [PermitOffice] {
        &mut self.offices
    }

    #[must_use]
    pub fn all(&self) -> &[Permit] {
        &self.permits
    }

    #[must_use]
    pub fn get(&self, id: PermitId) -> Option<&Permit> {
        self.permits.get((id.0 as usize).checked_sub(1)?)
    }

    #[must_use]
    pub fn issued(&self) -> u32 {
        self.issued
    }

    #[must_use]
    pub fn expired(&self) -> u32 {
        self.expired
    }

    #[must_use]
    pub fn open_count(&self) -> usize {
        self.permits.iter().filter(|p| p.status.is_open()).count()
    }

    /// Mediana czasu oczekiwania wydanych pozwoleń, w dobach. `None`, gdy urząd
    /// nie wydał jeszcze ani jednego — zero byłoby wtedy kłamstwem.
    #[must_use]
    pub fn median_wait_days(&self) -> Option<u16> {
        if self.waits.is_empty() {
            return None;
        }
        let mut v = self.waits.clone();
        v.sort_unstable();
        Some(v[v.len() / 2])
    }

    /// Czy ten zakład ma już złożony albo wydany wniosek — strażnik przed
    /// dwukrotnym złożeniem tego samego wniosku przez most.
    ///
    /// Wniosek **wygasły się nie liczy**: wygasa wniosek, a nie zakład. Liczenie go
    /// znaczyłoby, że zakład, któremu urząd nie wyrobił się w ciągu roku, nie może
    /// złożyć już nigdy — czyli że `Expired` jest stanem końcowym zakładu.
    #[must_use]
    pub fn has_permit_for(&self, site: SiteId) -> bool {
        self.permits
            .iter()
            .any(|p| p.applicant == Applicant::Site(site) && p.status != PermitStatus::Expired)
    }

    /// Składa wniosek. Zwraca `None`, gdy miasto nie ma ani jednego urzędu —
    /// wtedy nie ma komu go rozpatrzyć i udawanie, że wniosek istnieje, byłoby
    /// gorsze od odmowy.
    pub fn file(
        &mut self,
        applicant: Applicant,
        kind: PermitKind,
        tuning: &CityTuning,
        t: Tick,
    ) -> Option<PermitId> {
        if self.offices.is_empty() {
            return None;
        }
        // Do najkrótszej kolejki, a przy remisie do urzędu o niższym indeksie —
        // deterministycznie, bo kolejność wchodzi do stanu świata (00 §3.2).
        let (biuro, _) = self
            .offices
            .iter()
            .enumerate()
            .map(|(i, o)| (i, o.queue.len()))
            .min_by_key(|(i, n)| (*n, *i))?;
        let id = PermitId(self.next);
        self.next += 1;
        self.permits.push(Permit {
            id,
            applicant,
            kind,
            filed_at: t,
            status: PermitStatus::Queued,
            fee: tuning.permits.fee(kind),
            office: biuro as u32,
            units_left: tuning.permits.cost(kind).max(1),
        });
        self.offices[biuro].queue.push(id);
        Some(id)
    }
}

/// Doba urzędu: przerób kolejki i wygaśnięcia (§5.2, `sys_process_permit_queue`).
///
/// Zwraca powody wydanych pozwoleń — po jednym na decyzję, z czasem oczekiwania,
/// bo to on jest treścią tej decyzji.
pub fn process_queue(
    reg: &mut PermitRegistry,
    tuning: &CityTuning,
    day: DayOfWeek,
    t: Tick,
) -> Vec<(Applicant, DecisionReason)> {
    let mut powody = Vec::new();
    for biuro in 0..reg.offices.len() {
        if !reg.offices[biuro].open_on(day) {
            continue;
        }
        let mut budzet = reg.offices[biuro].staff * tuning.permits.units_per_clerk_day;
        if budzet == 0 {
            continue;
        }
        let mut wydane: Vec<PermitId> = Vec::new();
        for id in reg.offices[biuro].queue.clone() {
            if budzet == 0 {
                break;
            }
            let i = id.0 as usize - 1;
            let zrobione = budzet.min(reg.permits[i].units_left);
            reg.permits[i].units_left -= zrobione;
            budzet -= zrobione;
            if reg.permits[i].units_left == 0 {
                reg.permits[i].status = PermitStatus::Approved { at: t };
                let czekal = reg.permits[i].waited_days(t);
                if reg.waits.len() >= WAIT_RING {
                    reg.waits.remove(0);
                }
                reg.waits.push(czekal);
                reg.issued += 1;
                wydane.push(id);
                powody.push((
                    reg.permits[i].applicant,
                    DecisionReason::PermitIssued {
                        kind: reg.permits[i].kind,
                        waited_days: czekal,
                    },
                ));
            } else {
                reg.permits[i].status = PermitStatus::UnderReview;
            }
        }
        reg.offices[biuro].queue.retain(|id| !wydane.contains(id));
    }

    // Wygaśnięcia: wniosek, który leży dłużej niż rok gry, jest historią urzędu.
    let limit = u64::from(tuning.permits.expire_days) * 1_440;
    let mut przeterminowane: Vec<PermitId> = Vec::new();
    for p in &mut reg.permits {
        if p.status.is_open() && t.get().saturating_sub(p.filed_at.get()) > limit {
            p.status = PermitStatus::Expired;
            przeterminowane.push(p.id);
        }
    }
    if !przeterminowane.is_empty() {
        reg.expired += przeterminowane.len() as u32;
        for o in &mut reg.offices {
            o.queue.retain(|id| !przeterminowane.contains(id));
        }
    }
    powody
}

impl HashState for PermitRegistry {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.next);
        h.write_u32(self.issued);
        h.write_u32(self.expired);
        h.write_u64(self.permits.len() as u64);
        for p in &self.permits {
            h.write_u32(p.id.0);
            match p.applicant {
                Applicant::Site(s) => h.write_u64(s.0.to_bits()),
                Applicant::Player => h.write_u64(u64::MAX),
            }
            h.write_u32(p.office);
            h.write_u8(p.kind.as_index() as u8);
            h.write_u64(p.filed_at.get());
            h.write_u8(p.status.tag());
            h.write_u32(p.units_left);
            h.write_i64(p.fee.get());
        }
        h.write_u64(self.offices.len() as u64);
        for o in &self.offices {
            h.write_u64(o.site.0.to_bits());
            h.write_u32(o.staff);
            h.write_u8(o.open_days);
            h.write_u64(o.queue.len() as u64);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::Entity;
    use std::num::NonZeroU32;

    fn site(i: u32) -> SiteId {
        SiteId(Entity::new(i, NonZeroU32::new(1).unwrap()))
    }

    /// Przerób jednego urzędu przez `dni` dób, zaczynając od doby 0.
    fn przebieg(obsada: u32, wnioskow: u32, dni: u64) -> Option<u16> {
        let t = CityTuning::load_default().expect("tuning");
        let mut biuro = PermitOffice::new(site(1), DistrictId(0));
        biuro.staff = obsada;
        let mut reg = PermitRegistry::new(vec![biuro]);
        for i in 0..wnioskow {
            reg.file(
                Applicant::Site(site(100 + i)),
                PermitKind::Build,
                &t,
                Tick(0),
            );
        }
        for d in 0..dni {
            let tick = Tick(d * 1_440);
            process_queue(&mut reg, &t, DayOfWeek::from_day_index(d), tick);
        }
        reg.median_wait_days()
    }

    #[test]
    fn podwojenie_obsady_skraca_mediane_oczekiwania_o_ponad_40_procent() {
        // Kryterium ukończenia WP7. Sto wniosków złożonych naraz, dwa urzędy
        // różniące się wyłącznie obsadą.
        let wolno = przebieg(2, 100, 120).expect("urząd wydał pozwolenia");
        let szybko = przebieg(4, 100, 120).expect("urząd wydał pozwolenia");
        assert!(
            u32::from(szybko) * 100 <= u32::from(wolno) * 60,
            "mediana {wolno} → {szybko} dób, a miała spaść o ≥ 40 %"
        );
    }

    #[test]
    fn urzad_bez_obsady_nie_wydaje_nic() {
        assert_eq!(przebieg(0, 10, 60), None);
    }

    #[test]
    fn dni_wolne_wydluzaja_kolejke() {
        // Ten sam przerób tygodniowy, inny rozkład: urząd czynny siedem dni wydaje
        // szybciej niż czynny pięć. To jest `K-15` widziany od strony gracza.
        let t = CityTuning::load_default().expect("tuning");
        let mut mediany = Vec::new();
        for dni_robocze in [PermitOffice::WORKDAYS, 0b0111_1111u8] {
            let mut biuro = PermitOffice::new(site(1), DistrictId(0));
            biuro.staff = 2;
            biuro.open_days = dni_robocze;
            let mut reg = PermitRegistry::new(vec![biuro]);
            for i in 0..60 {
                reg.file(
                    Applicant::Site(site(200 + i)),
                    PermitKind::Build,
                    &t,
                    Tick(0),
                );
            }
            for d in 0..90 {
                process_queue(&mut reg, &t, DayOfWeek::from_day_index(d), Tick(d * 1_440));
            }
            mediany.push(reg.median_wait_days().expect("wydane"));
        }
        assert!(
            mediany[1] < mediany[0],
            "pięć dni {} vs siedem dni {}",
            mediany[0],
            mediany[1]
        );
    }
}
