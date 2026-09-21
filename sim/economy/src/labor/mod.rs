//! Rynek pracy (M7b, WP4–WP6; M7 §5.5, PRD §6.6).
//!
//! # Gdzie to mieszka i dlaczego akurat tu (`D2`)
//!
//! W `sim/economy`, czyli w crate'cie M5, choć autorem jest M7. Powód jest jeden
//! i wystarcza: **rynek pracy jest rynkiem**. Publikuje oferty, indeksuje je, zbiera
//! zgłoszenia, rozstrzyga po ocenie i zapisuje powód — tak samo jak rynek towarowy
//! obok. Drugi, równoległy mechanizm dopasowania jest jawnie zakazany.
//!
//! W `sim/firms` zostaje wyłącznie to, co jest **decyzją firmy**: kogo chce
//! ([`magnat_firms::score_application`]) i ile gotowa jest zapłacić
//! ([`magnat_firms::next_bid`]). Granica `D19` biegnie dokładnie tędy.
//!
//! # Czego tu nie ma: tabeli płac
//!
//! Nie istnieje zmienna „pensja spawacza". Jest widełka roli z `data/jobs/roles.ron`
//! (przedział, w którym **wolno licytować**), jest stawka w konkretnej ofercie i jest
//! mediana zawartych umów w [`LaborMarketStats`] — agregat po tym, co rynek zrobił.
//! Pensja rośnie wtedy i tylko wtedy, gdy oferta wisi bez kandydata.
//!
//! # Port do mieszkańców
//!
//! Rynek nie dotyka ECS-a sam: robi to przez [`Workforce`]. Dwie implementacje —
//! ta produkcyjna nad `&mut World` ([`system::WorldWorkforce`]) i ta testowa, dzięki
//! której §7.1 fazy da się sprawdzić na dwudziestu spawaczach zamiast na mieście.
//! To jest ten sam wzorzec, którym M3 oddaje miejsca przez `PlaceProvider`.

pub mod bidding;
pub mod hr;
pub mod market;
pub mod matching;
pub mod offer;
pub mod stats;
pub mod system;

use magnat_agents::ShiftKind;
use magnat_core::{
    CitizenId, DistrictId, HashState, JobRoleId, Money, NeedKind, SimMinute, SiteId, StateHasher, Q,
};

pub use market::LaborMarket;
pub use offer::{Application, JobIndex, JobOffer, JobOfferId, SkillReq};
pub use stats::{shortage, LaborMarketStats, RoleStats};
pub use system::{register_labor, LaborHandle, LaborSystem};

/// Tyle rynek pracy wie o mieszkaniu i o człowieku. `None` z [`Workforce::facts`]
/// znaczy, że tego mieszkańca już nie ma — wyprowadził się albo umarł.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PersonFacts {
    /// Forma: zdrowie, energia, nastrój, stres, wykształcenie. Jeden komponent zamiast
    /// czterech pól — tej samej struktury wymaga `Site::effective_labor`, więc port,
    /// który by ją rozebrał, kazałby ją potem składać z powrotem.
    pub vitals: magnat_agents::Vitals,
    /// Dzielnica zamieszkania — w niej kandydat szuka pracy najpierw.
    pub district: DistrictId,
    pub ambition: Q,
    pub loyalty: Q,
    /// Obecny etat: zakład i zawód. `None` = bez pracy. Stawki tu nie ma i być
    /// nie może — komponent mieszkańca nie niesie ani grosza (`D1`), a jedynym
    /// źródłem prawdy o płacy jest umowa po stronie firmy.
    pub job: Option<(SiteId, JobRoleId)>,
    /// Zawód, w którym kandydat jest najlepszy — w nim szuka pracy najpierw.
    /// `None` znaczy „nie umie jeszcze nic", czyli absolwent.
    pub best_role: Option<JobRoleId>,
    /// Czy mieszkaniec jest **dziś na zwolnieniu** (M8d WP7, `Lifecycle::FLAG_ILL`).
    ///
    /// Do M8d chorobę widziało wyłącznie `Vitals.health`, czyli forma — a chory
    /// pracownik dalej stał przy maszynie, tylko słabiej. Absencja jest czym innym
    /// niż osłabienie i to ona jest kanałem skutku szpitala z §5.3: przychodnia
    /// skraca zwolnienie, a nie leczy formę.
    pub on_sick_leave: bool,
}

impl PersonFacts {
    #[must_use]
    pub fn employed(&self) -> bool {
        self.job.is_some()
    }

    #[must_use]
    pub fn education(&self) -> u8 {
        self.vitals.edu_level
    }
}

/// Port rynku pracy do mieszkańców.
///
/// Wąski z rozmysłu (`I` z SOLID): rynek pyta o to, co wpływa na decyzję, i zapisuje
/// to, co z decyzji wynika. Nie widzi planu dnia, tras ani portfela.
pub trait Workforce {
    fn facts(&self, c: CitizenId) -> Option<PersonFacts>;

    /// Umiejętność w zawodzie, 0..=100.
    fn skill_in(&self, c: CitizenId, role: JobRoleId) -> Q;

    /// Kto dziś szuka pracy: wszyscy bez etatu plus dobowa część zatrudnionych,
    /// którzy się rozglądają. Kolejność **musi** być deterministyczna.
    fn job_seekers(&mut self, day: u32, on_the_job_every: u16, out: &mut Vec<CitizenId>);

    /// Zapis umowy po stronie mieszkańca.
    fn hire(
        &mut self,
        c: CitizenId,
        site: SiteId,
        role: JobRoleId,
        shift: ShiftKind,
        work_days: u8,
        wage: Money,
    );

    /// Wyjście z etatu. **Jedyna legalna droga** — etat, który nie wraca do puli,
    /// znika z miasta na zawsze (M3c, korekta G-7).
    fn release(&mut self, c: CitizenId, wage: Money);

    /// Zmiana stawki bez zmiany pracodawcy (kontroferta, podwyżka).
    fn raise_wage(&mut self, c: CitizenId, from: Money, to: Money);

    /// Świadczenie pozapłacowe podnosi potrzebę — realnie, nie przez nastrój.
    fn add_need(&mut self, c: CitizenId, need: NeedKind, points: u8);

    /// Szkolenie podnosi umiejętność w zawodzie.
    fn set_skill(&mut self, c: CitizenId, role: JobRoleId, level: Q);

    /// `(siła robocza, bez pracy)` — mianownik i licznik stopy bezrobocia.
    /// `day` jest potrzebny do wieku: dziecko bez pracy nie jest bezrobotne.
    fn labour_force(&mut self, day: u32) -> (u32, u32);
}

/// Szukający pracy: odkąd szuka i ile zarabiał ostatnio.
///
/// Dwa pola, bo tyle wystarcza na płacę progową: **oczekiwanie schodzi z czasem
/// bezrobocia** i to jest cała degresja przy nadmiarze podaży pracy (§5.5 pkt 3).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Seeker {
    pub since_day: u32,
    pub last_wage: Money,
}

impl HashState for Seeker {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.since_day);
        self.last_wage.hash_state(h);
    }
}

/// Podsumowanie doby rynku pracy — metryki balansatora i panelu.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct LaborDay {
    pub posted: u32,
    /// Ile ofert wygasło. Liczy **wyłącznie oferty bezpośrednie**: ogłoszenie
    /// publiczne na nadal otwarty wakat firma odnawia, a nie porzuca (`AU-7`),
    /// a takie, którego wakat się zamknął, znika razem z wakatem.
    pub expired: u32,
    pub applications: u32,
    pub hires: u32,
    pub quits: u32,
    pub dismissals: u32,
    pub left_force: u32,
    pub raises: u32,
    /// Ile ofert zatrzymało się na suficie marży zamiast podbić stawkę.
    pub frozen: u32,
    pub headhunts: u32,
    pub trained: u32,
    pub vacancies: u32,
    pub employed: u32,
    pub labour_force: u32,
    pub unemployed: u32,
    pub bonus: Money,
    /// Koszt świadczeń pozapłacowych — osobno od premii, bo to dwie różne decyzje
    /// firmy i balansator stroi je osobno.
    pub benefits: Money,
    pub severance: Money,
    pub training_cost: Money,
}

impl LaborDay {
    /// Stopa bezrobocia w promilach. Promile, nie procenty: pasmo 3–9 % ma być
    /// mierzalne z jedną cyfrą po przecinku, a nie zaokrąglane do liczby całkowitej.
    #[must_use]
    pub fn unemployment_permille(&self) -> u16 {
        if self.labour_force == 0 {
            return 0;
        }
        ((u64::from(self.unemployed) * 1_000) / u64::from(self.labour_force)).min(1_000) as u16
    }
}

/// Wejście `sim/agents` do rynku pracy: publikacja oferty przez gracza (M9).
pub fn post_offer(m: &mut LaborMarket, draft: JobOffer) -> JobOfferId {
    m.post_offer(draft)
}

/// Wejście mieszkańca: złożenie aplikacji (M3, M9).
pub fn apply_for(
    m: &mut LaborMarket,
    offer: JobOfferId,
    citizen: CitizenId,
    expectation: Money,
    skill: Q,
    education: u8,
    now: SimMinute,
) -> bool {
    m.apply_for(offer, citizen, expectation, skill, education, now)
}

/// Indeks niedoboru — kontrakt do M8 (polityka miejska) i M10 (związki, `K-9`).
#[must_use]
pub fn shortage_index(m: &LaborMarket, role: JobRoleId, district: DistrictId) -> u16 {
    m.shortage_index(role, district)
}
