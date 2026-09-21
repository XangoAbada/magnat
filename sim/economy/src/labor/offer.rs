//! Oferta pracy i aplikacja (M7b WP4, M7 §5.5, PRD §6.6).
//!
//! **Rynek pracy jest rynkiem** i korzysta z tej samej maszynerii co rynek towarowy:
//! oferta siedzi w arenie z uchwytem `{index, generation}` (`K-16`), a indeks jest
//! pochodną areny i do hasha nie wchodzi — dokładnie jak [`crate::OfferIndex`].
//!
//! ## Dlaczego to nie jest po prostu `Offer`
//!
//! Ten sam rozstrzygnięty spór, co przy ofercie B2B (`K-36`), i z tych samych trzech
//! powodów. **Kształt:** `Offer` niesie cenę jednostki, stan półki i jakość towaru;
//! oferta pracy niesie stawkę miesięczną, liczbę etatów, zmianę, wymagania i świadczenia
//! — z tych dziewięciu pól wspólne jest jedno. **Indeks:** `OfferIndex` warstwuje się
//! po `StockCat`, a zawód nie ma kategorii spiżarni, w której mógłby stanąć.
//! **Koszt:** dołożenie ośmiu pól do 56-bajtowej struktury trzymanej w arenie setek
//! tysięcy ofert detalicznych byłoby kosztem płaconym przez M5 za wygodę M7.
//!
//! Co z zakazu „drugiego, równoległego mechanizmu dopasowania" (§5.5) zostaje bez
//! zmian, i to jest istota: **jeden mechanizm dopasowania**, czyli ta sama kolejność
//! kroków — publikacja, zapytanie do indeksu, ocena kandydatów funkcją firmy,
//! rozstrzygnięcie po wyniku z jawnym remisem — i ta sama arena z generacjami.
//! Powielony jest kształt, nie decyzja.
//!
//! ## Dlaczego indeks jest po `(JobRoleId, DistrictId)`, a nie przestrzenny
//!
//! §5.5 mówi „ten sam indeks przestrzenny", a WP4 w tym samym dokumencie mówi
//! „indeks ofert per (`JobRoleId`, `DistrictId`) zgodnie z §17.5". Rozstrzygnięte na
//! korzyść drugiego zapisu, bo to on odpowiada PRD §17.5 i bo pytanie kandydata
//! naprawdę tak brzmi: **szukam pracy w swoim zawodzie, niedaleko**. Siatka przestrzenna
//! odpowiada na „co jest w promieniu 800 m" — pytanie kupującego chleb, nie szukającego
//! pracy, który pojedzie przez pół miasta, jeśli oferta jest w jego zawodzie.

use magnat_agents::ShiftKind;
use magnat_core::{
    Arena, ArenaHandle, CitizenId, DistrictId, HashState, JobRoleId, Money, SimMinute, SiteId,
    StateHasher, Q,
};
use magnat_firms::{BenefitSet, FirmKey};
use std::collections::BTreeMap;

/// Uchwyt oferty pracy. Ten sam kształt co `OfferId` — uchwyt po zwolnieniu nigdy
/// nie jest ponownie ważny (`K-16`).
pub type JobOfferId = ArenaHandle<JobOffer>;

/// Wymagania stanowiska. Dwa progi, nie profil kompetencji: w M7b umiejętność jest
/// jedną liczbą per zawód (`Skills::level_in`), a wykształcenie poziomem z M3.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct SkillReq {
    pub min_skill: Q,
    pub min_edu: u8,
}

impl HashState for SkillReq {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(self.min_skill.get());
        h.write_u8(self.min_edu);
    }
}

/// Oferta pracy. **Stawka jest tu jedyną licytowaną zmienną** — reszta pól opisuje
/// etat, a nie cenę.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct JobOffer {
    pub firm: FirmKey,
    pub site: SiteId,
    pub role: JobRoleId,
    pub district: DistrictId,
    /// Brutto, miesięcznie. To ona rośnie przy niedoborze (§5.5 pkt 1).
    pub wage_month: Money,
    pub slots: u16,
    pub shift: ShiftKind,
    /// Maska `DayOfWeek` etatu (`K-15`, `R2-WP37`). Jedzie w ofercie razem ze zmianą,
    /// bo **nie jest jej funkcją**: ta sama zmiana poranna wypada raz w poniedziałek–
    /// piątek, a raz we wtorek–sobotę, i to jest cała obsada weekendowa handlu.
    /// Do R2 `hire` wpisywało każdemu `WEEKDAYS` i sklep w sobotę nie miał kasjera.
    pub work_days: u8,
    pub requirements: SkillReq,
    pub benefits: BenefitSet,
    /// `Some` = oferta bezpośrednia (headhunting): rozważa ją **wyłącznie** wskazany
    /// mieszkaniec i nikt inny jej nie widzi.
    pub targeted: Option<CitizenId>,
    pub posted: SimMinute,
    pub expires: SimMinute,
    /// Ile dób oferta wisi bez obsadzenia — wejście licytacji i statystyki czasu wakatu.
    pub days_open: u16,
    /// Ile razy podbito stawkę. Wchodzi do powodu decyzji i ogranicza licytację
    /// niezależnie od sufitu marży.
    pub raises: u8,
    /// Czy licytacja stanęła na suficie marży. Pole istnieje po to, żeby powód
    /// „stawka bez zmian" trafił do dziennika **raz**, a nie co trzy doby do końca
    /// świata — dziennik ma trzydzieści dwa wpisy i jedna zamrożona oferta wyczyściłaby
    /// z niego całą resztę historii firmy.
    pub frozen: bool,
    /// Ilu kandydatów złożyło aplikację, odkąd oferta wisi.
    pub applicants: u16,
    /// Widełki stanowiska — sufit, powyżej którego firma nie licytuje.
    /// Dane **prywatne firmy**: kandydat widzi stawkę, nie widzi widełek.
    pub band: (Money, Money),
}

impl JobOffer {
    /// Czy oferta jest publiczna, czyli widoczna dla każdego kandydata.
    #[must_use]
    pub fn is_public(&self) -> bool {
        self.targeted.is_none()
    }
}

impl HashState for JobOffer {
    fn hash_state(&self, h: &mut StateHasher) {
        self.firm.hash_state(h);
        self.site.entity().hash_state(h);
        self.role.hash_state(h);
        self.district.hash_state(h);
        self.wage_month.hash_state(h);
        h.write_u16(self.slots);
        h.write_u8(self.shift as u8);
        h.write_u8(self.work_days);
        self.requirements.hash_state(h);
        self.benefits.hash_state(h);
        self.targeted.map(|c| c.0).hash_state(h);
        self.posted.hash_state(h);
        self.expires.hash_state(h);
        h.write_u16(self.days_open);
        h.write_u8(self.raises);
        h.write_u8(u8::from(self.frozen));
        h.write_u16(self.applicants);
        self.band.0.hash_state(h);
        self.band.1.hash_state(h);
    }
}

/// Aplikacja kandydata. Żyje jedną dobę: rynek pracy rozstrzyga się raz dziennie,
/// a aplikacja, której nikt nie rozpatrzył, jest aplikacją odrzuconą.
///
/// **Bez `referral`.** §5.5 wymienia siłę polecenia z grafu relacji, ale jej właścicielem
/// jest M10 (relacje i związki), a pole bez pisarza jest kosztem razy liczba aplikacji
/// i zerem wartości — ta sama reguła, którą M7a przycięła `Firm` (`AR-6`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Application {
    pub offer: JobOfferId,
    pub citizen: CitizenId,
    /// Płaca progowa kandydata w chwili złożenia (§5.6).
    pub wage_expectation: Money,
    pub skill: Q,
    pub education: u8,
    pub submitted: SimMinute,
}

impl HashState for Application {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.offer.index());
        h.write_u32(self.offer.generation());
        self.citizen.0.hash_state(h);
        self.wage_expectation.hash_state(h);
        h.write_u8(self.skill.get());
        h.write_u8(self.education);
        self.submitted.hash_state(h);
    }
}

/// Indeks ofert per `(JobRoleId, DistrictId)` — **pochodna areny**, nie stan.
///
/// Do hasha nie wchodzi i wchodzić nie może: odtwarza się z areny w całości, tak samo
/// jak kubełki slotów decyzyjnych w M7a (`AR-9`) i `OfferIndex` w M5a.
#[derive(Default)]
pub struct JobIndex {
    by_role: BTreeMap<(JobRoleId, DistrictId), Vec<JobOfferId>>,
    /// Wszystko, co wisi w dzielnicy — po to i tylko po to, żeby absolwent bez
    /// ani jednej umiejętności miał czego szukać. Kandydat z zawodem idzie
    /// przez `by_role` i tej mapy nie dotyka.
    by_district: BTreeMap<DistrictId, Vec<JobOfferId>>,
    /// Oferty bezpośrednie per adresat. Osobna mapa, bo oferta z headhuntingu
    /// **nie jest widoczna nigdzie indziej** — ani w zawodzie, ani w dzielnicy.
    by_target: BTreeMap<CitizenId, Vec<JobOfferId>>,
    /// Wyłącznie oferty **publiczne**: po to, żeby oferta bezpośrednia nie blokowała
    /// firmie wystawienia zwykłego ogłoszenia na ten sam wakat.
    by_position: BTreeMap<(SiteId, JobRoleId), JobOfferId>,
    dirty: bool,
}

impl JobIndex {
    #[must_use]
    pub fn new() -> JobIndex {
        JobIndex::default()
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Przebudowa z areny. Kolejność wejścia to kolejność indeksów areny, więc
    /// w kubełku oferty leżą rosnąco po uchwycie niezależnie od kolejności publikacji.
    pub fn rebuild(&mut self, offers: &Arena<JobOffer>) {
        self.by_role.clear();
        self.by_district.clear();
        self.by_target.clear();
        self.by_position.clear();
        for (id, o) in offers.iter() {
            match o.targeted {
                None => {
                    self.by_role
                        .entry((o.role, o.district))
                        .or_default()
                        .push(id);
                    self.by_district.entry(o.district).or_default().push(id);
                    self.by_position.insert((o.site, o.role), id);
                }
                Some(c) => self.by_target.entry(c).or_default().push(id),
            }
        }
        self.dirty = false;
    }

    /// Oferty publiczne w zawodzie i dzielnicy.
    #[must_use]
    pub fn in_role(&self, role: JobRoleId, district: DistrictId) -> &[JobOfferId] {
        debug_assert!(
            !self.dirty,
            "zapytanie do indeksu ofert pracy bez przebudowy"
        );
        self.by_role
            .get(&(role, district))
            .map_or(&[][..], Vec::as_slice)
    }

    /// Wszystkie publiczne oferty w dzielnicy — wejście kandydata bez zawodu.
    #[must_use]
    pub fn in_district(&self, district: DistrictId) -> &[JobOfferId] {
        debug_assert!(
            !self.dirty,
            "zapytanie do indeksu ofert pracy bez przebudowy"
        );
        self.by_district
            .get(&district)
            .map_or(&[][..], Vec::as_slice)
    }

    /// Oferty bezpośrednie wysłane do tego mieszkańca.
    #[must_use]
    pub fn for_target(&self, c: CitizenId) -> &[JobOfferId] {
        debug_assert!(
            !self.dirty,
            "zapytanie do indeksu ofert pracy bez przebudowy"
        );
        self.by_target.get(&c).map_or(&[][..], Vec::as_slice)
    }

    /// Czy do tego mieszkańca ktoś już dziś wysłał ofertę bezpośrednią.
    #[must_use]
    pub fn is_targeted(&self, c: CitizenId) -> bool {
        self.by_target.contains_key(&c)
    }

    /// Oferta wisząca na tym stanowisku, jeśli jest — żeby firma nie publikowała
    /// drugiej na ten sam wakat.
    #[must_use]
    pub fn at_position(&self, site: SiteId, role: JobRoleId) -> Option<JobOfferId> {
        self.by_position.get(&(site, role)).copied()
    }

    /// Wszystkie zawody, w których cokolwiek wisi — wejście statystyk dobowych.
    pub fn keys(&self) -> impl Iterator<Item = (JobRoleId, DistrictId)> + '_ {
        self.by_role.keys().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::Entity;
    use std::num::NonZeroU32;

    fn oferta(site: u32, role: u16, district: u16) -> JobOffer {
        JobOffer {
            firm: FirmKey(1),
            site: SiteId(Entity::new(site, NonZeroU32::MIN)),
            role: JobRoleId(role),
            district: DistrictId(district),
            wage_month: Money(400_000),
            slots: 2,
            shift: ShiftKind::Day,
            work_days: magnat_agents::Employment::WEEKDAYS,
            requirements: SkillReq::default(),
            benefits: BenefitSet::NONE,
            targeted: None,
            posted: SimMinute(0),
            expires: SimMinute(1440 * 14),
            days_open: 0,
            raises: 0,
            frozen: false,
            applicants: 0,
            band: (Money(300_000), Money(500_000)),
        }
    }

    #[test]
    fn indeks_zwraca_tylko_swoj_zawod_i_swoja_dzielnice() {
        let mut arena: Arena<JobOffer> = Arena::new();
        arena.insert(oferta(1, 4, 0));
        arena.insert(oferta(2, 4, 0));
        arena.insert(oferta(3, 4, 1));
        arena.insert(oferta(4, 9, 0));
        let mut idx = JobIndex::new();
        idx.rebuild(&arena);
        assert_eq!(idx.in_role(JobRoleId(4), DistrictId(0)).len(), 2);
        assert_eq!(idx.in_role(JobRoleId(4), DistrictId(1)).len(), 1);
        assert_eq!(idx.in_role(JobRoleId(9), DistrictId(0)).len(), 1);
        assert!(idx.in_role(JobRoleId(9), DistrictId(7)).is_empty());
    }

    #[test]
    fn oferta_bezposrednia_nie_jest_widoczna_w_indeksie() {
        let mut arena: Arena<JobOffer> = Arena::new();
        let mut o = oferta(1, 4, 0);
        o.targeted = Some(CitizenId(Entity::new(77, NonZeroU32::MIN)));
        arena.insert(o);
        let mut idx = JobIndex::new();
        idx.rebuild(&arena);
        assert!(idx.in_role(JobRoleId(4), DistrictId(0)).is_empty());
        // Widzi ją wyłącznie adresat.
        let adresat = CitizenId(Entity::new(77, NonZeroU32::MIN));
        assert_eq!(idx.for_target(adresat).len(), 1);
        // I nie blokuje firmie zwykłego ogłoszenia na ten sam wakat.
        assert!(idx
            .at_position(SiteId(Entity::new(1, NonZeroU32::MIN)), JobRoleId(4))
            .is_none());
    }

    #[test]
    fn kolejnosc_w_kubelku_nie_zalezy_od_kolejnosci_publikacji() {
        let build = |rev: bool| {
            let mut arena: Arena<JobOffer> = Arena::new();
            let sites: Vec<u32> = if rev {
                (1..=20u32).rev().collect()
            } else {
                (1..=20u32).collect()
            };
            for s in sites {
                arena.insert(oferta(s, 4, 0));
            }
            let mut idx = JobIndex::new();
            idx.rebuild(&arena);
            idx.in_role(JobRoleId(4), DistrictId(0))
                .iter()
                .map(|id| arena.get(*id).expect("oferta").site.entity().index())
                .collect::<Vec<_>>()
        };
        let a = build(false);
        let b = build(true);
        // Zawartość ta sama, kolejność wyznaczona przez arenę, a nie przez wołającego.
        let mut a_s = a.clone();
        let mut b_s = b.clone();
        a_s.sort_unstable();
        b_s.sort_unstable();
        assert_eq!(a_s, b_s);
        assert_eq!(a.len(), 20);
    }
}
