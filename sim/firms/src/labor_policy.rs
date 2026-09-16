//! Reguły **firmy** na rynku pracy: kogo chce zatrudnić i ile gotowa jest zapłacić
//! (M7b WP4/WP5, M7 §5.5, PRD §6.6).
//!
//! ## Gdzie przebiega granica (decyzja otwarta `D19` fazy)
//!
//! Rynek pracy — oferty, indeks, dopasowanie, statystyki — mieszka w `sim/economy::labor`
//! (`D2`), bo rynek pracy jest rynkiem i ma korzystać z tej samej maszynerii co rynek
//! towarowy. Tutaj zostaje wyłącznie to, co jest **decyzją firmy**: scoring kandydata,
//! krok licytacji i sufit, powyżej którego firma przestaje licytować.
//!
//! ## Dlaczego te funkcje biorą gołe fakty, a nie `&Application` i `&JobOffer`
//!
//! §5.5 dokumentu fazy podaje sygnaturę `score_application(a: &Application, o: &JobOffer, …)`.
//! Ta sygnatura **nie może istnieć**: `Application` i `JobOffer` żyją w `sim/economy`,
//! a zależność idzie `economy → firms` i odwrócić się nie da (byłby cykl, którego Cargo
//! nie zbuduje). Wstrzyknięcie działa więc w drugą stronę, dokładnie tak, jak zapowiada
//! `D19`: rynek buduje z własnych typów [`CandidateFacts`] i [`OpeningFacts`] i woła
//! regułę firmy. Treść granicy jest ta sama, zmienia się kierunek podania danych.
//!
//! ## Czego tu nie ma
//!
//! Wagi z osobowości dyrektora ustawia M7e — [`HiringPolicy::NEUTRAL`] jest wartością,
//! z którą pracuje całe miasto do tego czasu. Pamięć firmy o kandydacie (`FirmMemory`)
//! i siła polecenia z grafu relacji (`ReferralStrength`) też należą do M7e i M10;
//! pole bez pisarza byłoby kosztem razy dziesięć tysięcy firm i zerem wartości (`AR-6`).

use magnat_core::{Money, WageCause, Q};

use crate::hr::tuning::WageTuning;

/// Tyle, ile firma wie o kandydacie w chwili wyboru.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CandidateFacts {
    pub skill: Q,
    /// Poziom wykształcenia (`Vitals::edu_level`).
    pub education: u8,
    /// Płaca progowa kandydata — poniżej niej oferty nie przyjmie.
    pub wage_expectation: Money,
}

/// Tyle, ile o wakacie wie scoring.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct OpeningFacts {
    pub wage_month: Money,
    pub min_skill: Q,
    pub min_edu: u8,
}

/// Wagi wyboru wywiedzione z osobowości firmy (M7e). Obie w punktach procentowych
/// **dodawanych** do wagi bazowej, więc zero znaczy „bez preferencji", a nie „zero wagi".
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct HiringPolicy {
    pub quality_focus: u8,
    pub price_focus: u8,
}

impl HiringPolicy {
    pub const NEUTRAL: HiringPolicy = HiringPolicy {
        quality_focus: 0,
        price_focus: 0,
    };
}

/// Nadwyżka umiejętności nad wymaganiem, przy której kandydat jest **idealny**.
///
/// Nie zero i nie „im więcej, tym lepiej": człowiek dokładnie na progu nie ma zapasu
/// na gorszy dzień, a człowiek dwa razy za dobry odejdzie przy pierwszej lepszej ofercie.
const IDEAL_MARGIN: i32 = 10;

/// Od tej nadwyżki zaczyna się kara za przekwalifikowanie (M7 §5.5).
const OVERQUALIFIED: i32 = 30;

/// Oszczędność na stawce, powyżej której tania oferta przestaje być okazją,
/// a zaczyna być sygnałem ryzyka (kandydat, który godzi się na połowę stawki,
/// zwykle wie o sobie coś, czego firma nie wie).
const SUSPICIOUS_SAVING_PCT: i32 = 25;

/// Czy kandydat w ogóle wchodzi w grę: umiejętność, wykształcenie i to, czy stawka
/// w ofercie mieści jego oczekiwanie.
///
/// Osobno od scoringu, bo to nie jest ocena, tylko warunek brzegowy — i pyta o niego
/// także kandydat, zanim złoży aplikację.
#[must_use]
pub fn meets_requirements(c: &CandidateFacts, o: &OpeningFacts) -> bool {
    c.skill.get() >= o.min_skill.get()
        && c.education >= o.min_edu
        && c.wage_expectation.get() <= o.wage_month.get()
}

/// Wynik kandydata. Scoring, nie argmax po jednej cesze (M7 §5.5).
///
/// Skala jest umowna i **porównywalna tylko w obrębie jednej oferty** — powód decyzji
/// niesie wynik zwycięzcy i drugiego w kolejce właśnie dlatego, że sama liczba nic
/// nie znaczy, a różnica znaczy wszystko.
#[must_use]
pub fn score_application(c: &CandidateFacts, o: &OpeningFacts, p: &HiringPolicy) -> i32 {
    let nadwyzka = i32::from(c.skill.get()) - i32::from(o.min_skill.get());
    let mut skill_fit = 100 - (nadwyzka - IDEAL_MARGIN).abs();
    if nadwyzka > OVERQUALIFIED {
        // Kara rośnie **poza** spadkiem dopasowania: przekwalifikowany kandydat nie jest
        // gorszym pracownikiem, tylko krótszym (M7 §8, ryzyko `R11`).
        skill_fit -= nadwyzka - OVERQUALIFIED;
    }
    let skill_fit = skill_fit.clamp(-100, 100);

    let oferta = o.wage_month.get().max(1);
    let oszczednosc = ((oferta - c.wage_expectation.get()) * 100 / oferta) as i32;
    let wage_fit = if oszczednosc > SUSPICIOUS_SAVING_PCT {
        // Odbicie, nie obcięcie: 40 % oszczędności ma być gorsze niż 25 %, a nie równe.
        SUSPICIOUS_SAVING_PCT - (oszczednosc - SUSPICIOUS_SAVING_PCT)
    } else {
        oszczednosc
    }
    .clamp(-100, 100);

    skill_fit * (100 + i32::from(p.quality_focus)) / 100
        + wage_fit * 2 * (100 + i32::from(p.price_focus)) / 100
}

/// Sufit licytacji: stawka, powyżej której ten etat przestaje się zakładowi opłacać.
///
/// **To jest jedyny hamulec spirali płacowej** (M7 §8, ryzyko `R1`), więc musi być
/// twardy i liczony z czegoś, co firma wie o sobie, a nie z tego, co robi rynek.
///
/// Podstawą jest górny kraniec widełek roli z `data/jobs/roles.ron` — a te są już
/// przeliczone przez epokę i zamożność dzielnicy, czyli mówią, ile ta praca w tym
/// miejscu może kosztować. `margin_headroom_bp` przesuwa sufit o zapas marży zakładu:
/// zakład zarabiający dobrze może przepłacić, zakład na granicy nie może.
//
// ponytail: dziś headroom jest hakiem zerowym, bo `SitePnlMonth` ma koszt pracy
// i koszt stały, ale **nie ma przychodu** — pierwszym jego pisarzem jest M7e
// (`AR-7`). Do tego czasu sufitem jest sam kraniec widełek i to wystarcza, żeby
// licytacja się zatrzymała. Gdy przychód zacznie istnieć, zmienia się tu jedna
// liczba przekazywana z wywołania, a nie kształt reguły.
#[must_use]
pub fn wage_ceiling(band: (Money, Money), margin_headroom_bp: i32) -> Money {
    let zapas = 10_000 + margin_headroom_bp.clamp(-5_000, 5_000);
    Money(band.1.get().saturating_mul(i64::from(zapas)) / 10_000)
}

/// Krok licytacji wraz z powodem — wynik decyzji „podbijam czy nie".
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Bid {
    /// Nowa stawka w ofercie. Równa poprzedniej znaczy „dalej nie licytuję".
    pub wage: Money,
    /// Przyrost wobec poprzedniej stawki w punktach bazowych; 0 przy zamrożeniu.
    pub delta_bp: u16,
    pub cause: WageCause,
}

/// Ile firma dokłada do wiszącej oferty (M7 §5.5 pkt 1).
///
/// `step = base_step × (1 + agresja) × (1 + niedobór)`, przycięte twardo do sufitu.
/// Przycięcie do zera jest **poprawnym wynikiem** i ma własny powód: nieobsadzony
/// wakat jest właściwą odpowiedzią firmy, która nie chce produkować poniżej progu
/// rentowności (M7 §7.1 pkt 4).
#[must_use]
pub fn wage_escalation_step(
    current: Money,
    shortage_index: u16,
    aggression: u8,
    t: &WageTuning,
) -> Money {
    let mut step = current.get().saturating_mul(i64::from(t.base_step_bp)) / 10_000;
    step = step * (100 + i64::from(aggression)) / 100;
    step = step * (1_000 + i64::from(shortage_index.min(1_000))) / 1_000;
    // Krok zerowy zatrzymałby licytację przy stawkach groszowych i wyglądałby
    // jak sufit, którym nie jest.
    Money(step.max(1))
}

/// Pełna decyzja o stawce: krok, sufit i powód (M7 §5.5 pkt 1).
#[must_use]
pub fn next_bid(
    current: Money,
    band: (Money, Money),
    shortage_index: u16,
    aggression: u8,
    margin_headroom_bp: i32,
    t: &WageTuning,
) -> Bid {
    let sufit = wage_ceiling(band, margin_headroom_bp)
        .get()
        .max(current.get());
    let krok = wage_escalation_step(current, shortage_index, aggression, t).get();
    let docelowa = current.get().saturating_add(krok).min(sufit);
    let delta = docelowa - current.get();
    if delta <= 0 {
        return Bid {
            wage: current,
            delta_bp: 0,
            cause: WageCause::Ceiling,
        };
    }
    let delta_bp =
        (delta.saturating_mul(10_000) / current.get().max(1)).clamp(0, i64::from(u16::MAX));
    Bid {
        wage: Money(docelowa),
        delta_bp: delta_bp as u16,
        cause: if shortage_index >= t.headhunt_shortage {
            WageCause::Shortage
        } else {
            WageCause::NoCandidates
        },
    }
}

/// Czy zatrudniony przyjmie ofertę z zewnątrz (M7 §5.5 pkt 2).
///
/// Próg jest w punktach bazowych przewagi nad obecną stawką; **ambicja go obniża,
/// lojalność podwyższa**. To jest cały koszt zmiany pracy po stronie mieszkańca
/// i jedyne, co powstrzymuje rotację przed staniem się młynkiem (`R11`).
#[must_use]
pub fn switch_threshold_bp(ambition: Q, loyalty: Q, t: &WageTuning) -> i32 {
    let a = i32::from(ambition.get()) - 50;
    let l = i32::from(loyalty.get()) - 50;
    (t.switch_threshold_bp - a * 10 + l * 14).max(100)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wage_tuning() -> WageTuning {
        crate::hr::tuning::LaborTuning::load_default()
            .expect("data/tuning/labor.ron")
            .wage
    }

    fn oferta(wage: i64, min_skill: u8) -> OpeningFacts {
        OpeningFacts {
            wage_month: Money(wage),
            min_skill: Q::new(min_skill),
            min_edu: 0,
        }
    }

    fn kandydat(skill: u8, oczekiwanie: i64) -> CandidateFacts {
        CandidateFacts {
            skill: Q::new(skill),
            education: 2,
            wage_expectation: Money(oczekiwanie),
        }
    }

    #[test]
    fn przekwalifikowany_przegrywa_z_dopasowanym() {
        let o = oferta(400_000, 40);
        let p = HiringPolicy::NEUTRAL;
        let dopasowany = score_application(&kandydat(50, 380_000), &o, &p);
        let przekwalifikowany = score_application(&kandydat(95, 380_000), &o, &p);
        assert!(
            dopasowany > przekwalifikowany,
            "dopasowany {dopasowany}, przekwalifikowany {przekwalifikowany}"
        );
    }

    #[test]
    fn zbyt_tani_kandydat_jest_sygnalem_ryzyka_a_nie_okazja() {
        let o = oferta(400_000, 40);
        let p = HiringPolicy::NEUTRAL;
        let rozsadny = score_application(&kandydat(50, 320_000), &o, &p); // −20 %
        let podejrzany = score_application(&kandydat(50, 160_000), &o, &p); // −60 %
        assert!(
            rozsadny > podejrzany,
            "rozsądny {rozsadny}, podejrzany {podejrzany}"
        );
    }

    #[test]
    fn wymagania_odsiewaja_zanim_dojdzie_do_oceny() {
        let o = oferta(400_000, 60);
        assert!(
            !meets_requirements(&kandydat(50, 300_000), &o),
            "za niska umiejętność"
        );
        assert!(
            !meets_requirements(&kandydat(70, 420_000), &o),
            "oczekiwanie ponad stawkę"
        );
        assert!(meets_requirements(&kandydat(70, 300_000), &o));
    }

    #[test]
    fn niedobor_podnosi_krok_licytacji() {
        let t = wage_tuning();
        let spokojnie = wage_escalation_step(Money(400_000), 0, 0, &t);
        let niedobor = wage_escalation_step(Money(400_000), 800, 0, &t);
        assert!(niedobor.get() > spokojnie.get());
        // Agresja mnoży się z niedoborem, a nie zastępuje go.
        let agresywnie = wage_escalation_step(Money(400_000), 800, 100, &t);
        assert!(agresywnie.get() > niedobor.get());
    }

    #[test]
    fn licytacja_zatrzymuje_sie_na_sufit_i_mowi_dlaczego() {
        let t = wage_tuning();
        let band = (Money(300_000), Money(420_000));
        // Stawka tuż pod sufitem: krok zostaje przycięty, ale jeszcze jest.
        let b = next_bid(Money(415_000), band, 1_000, 100, 0, &t);
        assert_eq!(b.wage, Money(420_000));
        assert!(b.delta_bp > 0);
        // Stawka na suficie: firma przestaje licytować i zapisuje powód.
        let b = next_bid(Money(420_000), band, 1_000, 100, 0, &t);
        assert_eq!(b.wage, Money(420_000));
        assert_eq!(b.delta_bp, 0);
        assert_eq!(b.cause, WageCause::Ceiling);
    }

    #[test]
    fn spirala_jest_niemozliwa_bo_sufit_jest_twardy() {
        let t = wage_tuning();
        let band = (Money(300_000), Money(420_000));
        let mut w = Money(300_000);
        for _ in 0..1_000 {
            w = next_bid(w, band, 1_000, 100, 0, &t).wage;
        }
        assert_eq!(w, Money(420_000), "licytacja przebiła sufit widełek");
    }

    #[test]
    fn ambicja_obniza_prog_zmiany_pracy_a_lojalnosc_go_podnosi() {
        let t = wage_tuning();
        let obojetny = switch_threshold_bp(Q::new(50), Q::new(50), &t);
        let ambitny = switch_threshold_bp(Q::new(90), Q::new(50), &t);
        let lojalny = switch_threshold_bp(Q::new(50), Q::new(90), &t);
        assert!(ambitny < obojetny);
        assert!(lojalny > obojetny);
        assert!(
            switch_threshold_bp(Q::new(100), Q::new(0), &t) > 0,
            "próg nigdy nie znika"
        );
    }
}
