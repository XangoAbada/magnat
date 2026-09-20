//! Prawo i egzekucja: pięć urzędów, sprawy, dowody i środki zaradcze
//! (M8d WP8, PRD §10.4).
//!
//! # Sprawa nie jest karą
//!
//! Urząd otwiera sprawę, kiedy stan zakładu przekroczy próg — i to jest wszystko,
//! co się wtedy dzieje. Dowody rosną z obsadą inspektorów i rozłożone są na doby,
//! więc zakład ma przez ten czas szansę przestać: sprawa, która straciła podstawę,
//! umarza się sama. Dolegliwość przychodzi dopiero na końcu i jest jedna na sprawę.
//!
//! # Skąd urząd wie
//!
//! Cztery urzędy z pięciu patrzą na **stan świata**, nie na rzut kostką: sanepid
//! na masę odpisaną z powodu terminu, inspekcja pracy na obsadę wobec etatów,
//! ochrona środowiska na pył z komina, antymonopol na udział w obrocie miasta.
//! Piąty — skarbowy — **jest zdarzeniem** (`CF-1`): hazard liczy `sim/events`
//! z sondy `FirmUnreportedBps`, a miasto dostaje stąd fakt, że kontrola przyszła.
//! Drugiego losowania nie budujemy, bo pierwsze stoi publiczne od M8c.
//!
//! # Gdzie kończy się miasto
//!
//! Kara pieniężna i domiar wchodzą do budżetu **tą samą drogą co każda danina**:
//! `ChargeRegistry::accrue` (`CB-3`). Nie ma drugiego rejestru wpływów i nie będzie,
//! bo domknięcie `Σ Assessed = Σ Settled + Σ Overdue + Σ Abated` z testu T1 jest
//! niezmiennikiem struktury, a nie wnioskiem — kara zaksięgowana obok niego byłaby
//! pieniądzem miasta spoza jego księgi.

use magnat_core::{
    AgencyKind, CaseId, DecisionReason, FirmId, HashState, Mass, Money, RemedyKind, SiteId,
    StateHasher, TaxKind, Tick, AGENCY_KIND_COUNT, Q,
};
use magnat_economy::Market;
use magnat_firms::Firms;

use crate::charge::TaxPayer;
use crate::city::{due_on_day, City};
use crate::tuning::CityTuning;

/// Środek zaradczy. Wariant z ładunkiem; słownik bez ładunku to `RemedyKind`
/// w `engine/core` (histogram kar ma liczyć kary, nie pary).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Remedy {
    Fine(Money),
    /// Zawieszenie działalności do ticku. Zakład wraca sam — sanepid zamykający
    /// restaurację na zawsze byłby karą śmierci za brudną lodówkę.
    Closure {
        until: Tick,
    },
    /// Przymusowy podział: firma schodzi poniżej progu, oddając zakład.
    ForcedDivestiture {
        share_bps: u32,
    },
    /// Domiar z odsetkami — należność jak każda inna (`CB-3`).
    BackTax {
        amount: Money,
        interest: Money,
    },
}

impl Remedy {
    #[must_use]
    pub fn kind(self) -> RemedyKind {
        match self {
            Remedy::Fine(_) => RemedyKind::Fine,
            Remedy::Closure { .. } => RemedyKind::Closure,
            Remedy::ForcedDivestiture { .. } => RemedyKind::ForcedDivestiture,
            Remedy::BackTax { .. } => RemedyKind::BackTax,
        }
    }

    /// Kwota tam, gdzie środek ma kwotę; zero tam, gdzie dolegliwością jest czas
    /// albo majątek. Udawanie kwoty zafałszowałoby histogram kar.
    #[must_use]
    pub fn amount(self) -> Money {
        match self {
            Remedy::Fine(m) => m,
            Remedy::BackTax { amount, interest } => Money(amount.get() + interest.get()),
            _ => Money::ZERO,
        }
    }
}

/// Z czego wzięła się sprawa. Nie jest to słownik dla `core` (`K-8`): ma jednego
/// właściciela i jednego czytelnika — ten plik.
///
/// Istnieje, bo urząd antymonopolowy prowadzi od M10e **dwie różne sprawy**:
/// za dominację (przymusowy podział) i za zmowę cenową (kara pieniężna). Gdyby
/// obie kończyły się tym samym środkiem, wykrycie kartelu zamykałoby sklepy
/// wszystkich członków naraz — czyli karałoby miasto mocniej niż zmowę.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CaseOrigin {
    /// Próg na stanie świata: udział w obrocie, masa odpisów, emisja, płaca.
    Threshold,
    /// Zmowa cenowa wykryta przez model (M10e WP10.13, `K-10`).
    Cartel,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Case {
    pub id: CaseId,
    /// Płatnikiem jest **zakład**, nie firma (`CA-4`) — księga i konto są per zakład,
    /// a domiar musi trafić w to samo miejsce co reszta należności.
    pub subject: SiteId,
    pub firm: FirmId,
    pub agency: AgencyKind,
    pub origin: CaseOrigin,
    pub opened_at: Tick,
    pub evidence: Q,
    pub remedy: Option<Remedy>,
    pub closed_at: Option<Tick>,
}

impl Case {
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.closed_at.is_none()
    }
}

/// Urząd kontrolny: budżet, inspektorzy i licznik spraw.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Agency {
    pub kind: AgencyKind,
    /// Ilu inspektorów ma urząd. **To jest cały jego budżet** i jedyne, co go
    /// ogranicza: liczba spraw prowadzonych naraz równa się tej liczbie.
    ///
    /// Pola `budget: Money` tu nie ma i nie będzie, dopóki ktoś nie rozbije
    /// `SpendCategory::Administration` na pięć urzędów — a to jest decyzja
    /// burmistrza, czyli M8e (`CH-6`). Pole, którego nikt nie wypełnia, wchodziłoby
    /// do hasha stanu jako stałe zero i wyglądało na działający mechanizm.
    pub inspectors: u32,
    pub opened: u32,
    pub closed: u32,
}

/// Wszystkie urzędy i wszystkie sprawy miasta.
#[derive(Clone, Debug)]
pub struct Enforcement {
    agencies: [Agency; AGENCY_KIND_COUNT],
    cases: Vec<Case>,
    next: u32,
    /// Emisja pyłu, powyżej której ochrona środowiska otwiera sprawę, w gramach
    /// na minutę. Liczona raz, przy stawianiu miasta, z odniesienia w
    /// `data/tuning/supply.ron` — żeby próg nie miał drugiego źródła (`K-35`).
    emission_limit_g_per_min: i64,
    /// Płaca minimalna z uchwały rady (M8e, `CH-5`). `None` = nieuchwalona.
    min_wage: Option<Money>,
}

impl Default for Enforcement {
    fn default() -> Enforcement {
        Enforcement::new(0, 0)
    }
}

impl Enforcement {
    /// Urzędy z równą obsadą inspektorów. Liczba jest wejściem mostu, bo bierze się
    /// z obsady urzędów miejskich, a tę zna świat, nie kodeks.
    #[must_use]
    pub fn new(inspectors_each: u32, emission_limit_g_per_min: i64) -> Enforcement {
        let mut agencies = [Agency {
            kind: AgencyKind::Antitrust,
            inspectors: inspectors_each,
            opened: 0,
            closed: 0,
        }; AGENCY_KIND_COUNT];
        for k in AgencyKind::ALL {
            agencies[k.as_index()].kind = *k;
        }
        Enforcement {
            agencies,
            cases: Vec::new(),
            next: 1,
            emission_limit_g_per_min,
            min_wage: None,
        }
    }

    #[must_use]
    pub fn agencies(&self) -> &[Agency; AGENCY_KIND_COUNT] {
        &self.agencies
    }

    /// Próg emisji pyłu, powyżej którego ochrona środowiska otwiera sprawę.
    #[must_use]
    pub fn emission_limit_g_per_min(&self) -> i64 {
        self.emission_limit_g_per_min
    }

    /// Zaostrzenie albo poluzowanie progu emisji uchwałą rady (M8e).
    ///
    /// Uchwała **zastępuje** próg policzony przy stawianiu miasta, a nie dokłada
    /// się do niego: dwa progi emisji naraz znaczyłyby, że nie wiadomo, który
    /// obowiązuje, a limit jest liczbą, którą gracz ma znać.
    pub fn set_emission_limit(&mut self, g_per_min: i64) {
        self.emission_limit_g_per_min = g_per_min.max(0);
    }

    /// Płaca minimalna uchwalona przez radę, w groszach miesięcznie (`CH-5`).
    ///
    /// `None` znaczy „nie uchwalono" i wtedy inspekcja pracy stoi na dolnych
    /// widełkach roli z `data/jobs/roles.ron`, tak jak od M8d. Uchwała **podmienia
    /// próg, nie mechanizm**: sprawa, dowody, kara i wpis do budżetu są na miejscu.
    #[must_use]
    pub fn min_wage(&self) -> Option<Money> {
        self.min_wage
    }

    pub fn set_min_wage(&mut self, m: Money) {
        self.min_wage = if m.get() > 0 { Some(m) } else { None };
    }

    /// Urząd tego rodzaju — odczyt dla panelu i dla kalibracji hazardu zmów.
    #[must_use]
    pub fn agency(&self, kind: AgencyKind) -> &Agency {
        &self.agencies[kind.as_index()]
    }

    pub fn set_inspectors(&mut self, kind: AgencyKind, n: u32) {
        self.agencies[kind.as_index()].inspectors = n;
    }

    #[must_use]
    pub fn cases(&self) -> &[Case] {
        &self.cases
    }

    #[must_use]
    pub fn get(&self, id: CaseId) -> Option<&Case> {
        self.cases.get((id.0 as usize).checked_sub(1)?)
    }

    /// Sprawy dotyczące tego zakładu — treść karty inspekcji (`K-62`).
    pub fn of_site(&self, site: SiteId) -> impl Iterator<Item = &Case> {
        self.cases.iter().filter(move |c| c.subject == site)
    }

    #[must_use]
    pub fn open_count(&self) -> usize {
        self.cases.iter().filter(|c| c.is_open()).count()
    }

    fn ma_otwarta(&self, agency: AgencyKind, site: SiteId) -> bool {
        self.cases
            .iter()
            .any(|c| c.is_open() && c.agency == agency && c.subject == site)
    }

    /// Ile spraw urząd prowadzi naraz. Inspektor prowadzi jedną — i to jest cała
    /// treść „budżetu urzędu": urząd bez ludzi nie otwiera niczego, a urząd
    /// z pięcioma inspektorami ma pięć spraw i szósta czeka.
    ///
    /// Bez tego limitu sanepid otwierał w pierwszym tygodniu sprawę przeciwko
    /// co drugiemu sklepowi w mieście, dowody dzieliły się na dwieście spraw
    /// i **żadna nigdy się nie kończyła** — czyli mechanizm wyglądał na działający
    /// i nie robił nic.
    fn ma_miejsce(&self, agency: AgencyKind) -> bool {
        let otwarte = self
            .cases
            .iter()
            .filter(|c| c.is_open() && c.agency == agency)
            .count();
        otwarte < self.agencies[agency.as_index()].inspectors as usize
    }

    /// Otwarcie sprawy. Publiczne od M8e, bo drugim wołającym jest wpłata poza
    /// rejestrem wpłat kampanijnych (`rule::finansuj_kampanie`) — a `K-11` mówi,
    /// co sądzimy o drugiej ścieżce do tego samego skutku. Limit spraw na urząd,
    /// odsiewanie powtórek i dowody działają tak samo dla obu wołających.
    pub fn otworz(
        &mut self,
        agency: AgencyKind,
        site: SiteId,
        firm: FirmId,
        evidence: Q,
        t: Tick,
    ) -> Option<DecisionReason> {
        self.otworz_z(agency, CaseOrigin::Threshold, site, firm, evidence, t)
    }

    /// Otwarcie sprawy o znanym pochodzeniu. Druga nazwa, a nie druga ścieżka:
    /// [`Enforcement::otworz`] jest jej opakowaniem, więc limit spraw, odsiewanie
    /// powtórek i dowody działają tak samo dla każdego wołającego (`K-11`).
    pub fn otworz_z(
        &mut self,
        agency: AgencyKind,
        origin: CaseOrigin,
        site: SiteId,
        firm: FirmId,
        evidence: Q,
        t: Tick,
    ) -> Option<DecisionReason> {
        if self.ma_otwarta(agency, site) || !self.ma_miejsce(agency) {
            return None;
        }
        let id = CaseId(self.next);
        self.next += 1;
        self.cases.push(Case {
            id,
            subject: site,
            firm,
            agency,
            origin,
            opened_at: t,
            evidence,
            remedy: None,
            closed_at: None,
        });
        self.agencies[agency.as_index()].opened += 1;
        Some(DecisionReason::CaseOpened { agency, evidence })
    }
}

impl HashState for Enforcement {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.next);
        for a in &self.agencies {
            h.write_u32(a.inspectors);
            h.write_u32(a.opened);
            h.write_u32(a.closed);
        }
        h.write_u64(self.cases.len() as u64);
        for c in &self.cases {
            h.write_u32(c.id.0);
            h.write_u64(c.subject.0.to_bits());
            h.write_u8(c.agency.as_index() as u8);
            h.write_u8(u8::from(c.origin == CaseOrigin::Cartel));
            h.write_u64(c.opened_at.get());
            h.write_u8(c.evidence.get());
            h.write_u8(c.remedy.map_or(255, |r| r.kind().as_index() as u8));
            h.write_i64(c.remedy.map_or(0, |r| r.amount().get()));
            h.write_u64(c.closed_at.map_or(0, magnat_core::Tick::get));
        }
    }
}

/// Doba urzędów: otwieranie spraw, dowody, rozstrzygnięcia (M8d WP8).
///
/// `audited` to zakłady, których firma dostała w tej dobie zdarzenie kontroli
/// skarbowej — system miasta czyta je z rejestru zdarzeń, bo hazard należy do
/// `sim/events` (`CF-1`). Reszta urzędów patrzy na stan świata sama.
///
/// Zwraca powody do dziennika: otwarcia spraw i nałożone środki.
pub fn step_day(
    city: &mut City,
    market: &Market,
    firms: Option<&Firms>,
    audited: &[SiteId],
    tuning: &CityTuning,
    t: Tick,
) -> Vec<(SiteId, DecisionReason)> {
    let p = tuning.agencies;
    let mut powody: Vec<(SiteId, DecisionReason)> = Vec::new();
    let widok = market.enforcement_view(t);

    // 1. Kontrola skarbowa — z rejestru zdarzeń, nie z progu.
    for site in audited {
        let Some(r) = widok.iter().find(|r| r.site == *site) else {
            continue;
        };
        if r.closed || r.unreported_bps == 0 {
            continue;
        }
        if let Some(powod) =
            city.enforcement
                .otworz(AgencyKind::TaxOffice, r.site, r.firm, Q::new(20), t)
        {
            powody.push((r.site, powod));
        }
    }

    // 2. Cztery urzędy patrzące na stan świata. Kolejność po `SiteId` rosnąco,
    //    bo kolejność otwierania spraw wchodzi do stanu (00 §3.2).
    let obrot_miasta: i64 = widok
        .iter()
        .filter(|r| !r.closed)
        .map(|r| r.declared_revenue.get())
        .sum();
    for r in &widok {
        // Zakład zamknięty nie ma czego naruszać, a zawieszony właśnie odbywa karę.
        if r.closed || r.suspended {
            continue;
        }
        // Sanepid: ile ten zakład wyrzucił z powodu terminu.
        if r.expired_mass.0 >= p.sanitary_expired_g {
            if let Some(powod) =
                city.enforcement
                    .otworz(AgencyKind::Sanitary, r.site, r.firm, Q::new(30), t)
            {
                powody.push((r.site, powod));
            }
        }
        // Antymonopol: udział zakładu w obrocie zadeklarowanym miasta. Zakład,
        // a nie firma — bo podmiotem sprawy jest zakład (`CA-4`), a firma
        // jednozakładowa i tak jest swoim zakładem.
        if obrot_miasta > 0 {
            let udzial = r.declared_revenue.get() * 10_000 / obrot_miasta;
            if udzial >= i64::from(p.antitrust_share_bp) {
                if let Some(powod) =
                    city.enforcement
                        .otworz(AgencyKind::Antitrust, r.site, r.firm, Q::new(40), t)
                {
                    powody.push((r.site, powod));
                }
            }
        }
    }
    if let Some(f) = firms {
        powody.extend(inspekcja_pracy(city, f, &p, t));
    }
    powody.extend(ochrona_srodowiska(city, t));

    // 3. Dowody i rozstrzygnięcia.
    powody.extend(prowadz_sprawy(city, market, &widok, tuning, t));
    powody
}

/// Inspekcja pracy: zakład płacący poniżej dolnych widełek roli.
///
/// **Próg stoi na płacy, a nie na obsadzie, i to jest korekta wobec pierwszej
/// wersji.** Obsada wobec etatów wyglądała na naturalną miarę, ale mierzy dziś
/// co innego, niż nazywa: po M7f w mieście stoi kilkanaście tysięcy nieobsadzonych
/// etatów, więc „zakład pracujący obsadą, której nie ma" to w tym świecie **każdy**
/// zakład. Sonda mierząca znaną fikcję jest tym samym błędem co stopa bezrobocia
/// 6 ‰ w wykazie `R2` — i tak samo nie jest kwestią progu.
///
/// Widełki roli są za to prawdziwe i pochodzą z `data/jobs/roles.ron`: umowa
/// poniżej dolnej granicy przedziału, w którym firmy licytują, jest naruszeniem,
/// które da się nazwać bez uchwały rady o płacy minimalnej (ta jest w M8e).
fn inspekcja_pracy(
    city: &mut City,
    firms: &Firms,
    p: &crate::tuning::AgencyParams,
    t: Tick,
) -> Vec<(SiteId, DecisionReason)> {
    // Uchwała rady o płacy minimalnej **podmienia próg, nie mechanizm** (`CH-5`):
    // sprawa, dowody, kara i wpis do budżetu są na miejscu od M8d. Bez uchwały
    // progiem zostaje ułamek dolnych widełek roli, tak jak było.
    let ustawowa = city.enforcement.min_wage();
    let mut out = Vec::new();
    for (id, site) in firms.sites() {
        let mut ponizej = 0u32;
        let mut umow = 0u32;
        for poz in &site.positions {
            let prog = ustawowa.map_or_else(
                || poz.wage_band.0.get() * i64::from(p.wage_floor_bp) / 10_000,
                |m| m.get(),
            );
            for e in &poz.filled {
                umow += 1;
                if e.wage_month.get() < prog {
                    ponizej += 1;
                }
            }
        }
        if umow == 0 || ponizej * 2 < umow {
            continue;
        }
        if let Some(powod) = city.enforcement.otworz(
            AgencyKind::LaborInspection,
            id,
            firms.id_of(site.firm),
            Q::new(25),
            t,
        ) {
            out.push((id, powod));
        }
    }
    out
}

/// Ochrona środowiska: pył z komina ponad normę.
fn ochrona_srodowiska(city: &mut City, t: Tick) -> Vec<(SiteId, DecisionReason)> {
    let limit = city.enforcement.emission_limit_g_per_min();
    if limit <= 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let emisje: Vec<(SiteId, FirmId, i64)> = city
        .emissions
        .iter()
        .map(|(s, f, g)| (*s, *f, *g))
        .collect();
    for (site, firm, g) in emisje {
        if g < limit {
            continue;
        }
        if let Some(powod) =
            city.enforcement
                .otworz(AgencyKind::Environment, site, firm, Q::new(35), t)
        {
            out.push((site, powod));
        }
    }
    out
}

/// Dowody rosną, sprawy się kończą. Jedna doba.
fn prowadz_sprawy(
    city: &mut City,
    market: &Market,
    widok: &[magnat_economy::SiteEnforcementRow],
    tuning: &CityTuning,
    t: Tick,
) -> Vec<(SiteId, DecisionReason)> {
    let p = tuning.agencies;
    // Inspektorzy dzielą się między sprawy swojego urzędu: dziesięć spraw naraz
    // znaczy, że każda idzie dziesięć razy wolniej. To jest cała treść budżetu
    // urzędu — bez tego jeden inspektor prowadziłby tysiąc spraw w tym samym tempie.
    let mut spraw = [0u32; AGENCY_KIND_COUNT];
    for c in city.enforcement.cases.iter().filter(|c| c.is_open()) {
        spraw[c.agency.as_index()] += 1;
    }
    let mut przyrost = [0u8; AGENCY_KIND_COUNT];
    for k in AgencyKind::ALL {
        let i = k.as_index();
        let inspektorzy = city.enforcement.agencies[i].inspectors;
        przyrost[i] = if spraw[i] == 0 {
            0
        } else {
            u8::try_from(u32::from(p.evidence_per_inspector_day) * inspektorzy / spraw[i].max(1))
                .unwrap_or(u8::MAX)
        };
    }

    // Czy sprawa **nadal ma podstawę**. To jest obietnica z nagłówka modułu i bez
    // tego kroku była pustym słowem: zakład, który przestał ukrywać obrót albo
    // wyrzucać przeterminowany towar, i tak dostawałby karę, bo dowody rosną same.
    // Urzędy stojące na stanie trwałym (inspekcja pracy, ochrona środowiska,
    // antymonopol) podstawy nie tracą w ciągu jednej doby i nie są tu sprawdzane.
    let ma_podstawe = |agency: AgencyKind, site: SiteId| -> bool {
        let Some(r) = widok.iter().find(|r| r.site == site) else {
            return false;
        };
        match agency {
            AgencyKind::TaxOffice => r.unreported_bps > 0,
            AgencyKind::Sanitary => r.expired_mass.0 >= p.sanitary_expired_g,
            _ => true,
        }
    };

    let mut do_rozstrzygniecia: Vec<(usize, AgencyKind, CaseOrigin, SiteId, FirmId)> = Vec::new();
    let mut umorzone: Vec<usize> = Vec::new();
    let limit = u64::from(p.case_expire_days) * 1_440;
    for (i, c) in city.enforcement.cases.iter_mut().enumerate() {
        if !c.is_open() {
            continue;
        }
        if !ma_podstawe(c.agency, c.subject) {
            umorzone.push(i);
            continue;
        }
        c.evidence = c.evidence.saturating_add(przyrost[c.agency.as_index()]);
        if c.evidence.get() >= p.evidence_to_close {
            do_rozstrzygniecia.push((i, c.agency, c.origin, c.subject, c.firm));
        } else if t.get().saturating_sub(c.opened_at.get()) > limit {
            umorzone.push(i);
        }
    }
    for i in umorzone {
        city.enforcement.cases[i].closed_at = Some(t);
        let k = city.enforcement.cases[i].agency.as_index();
        city.enforcement.agencies[k].closed += 1;
    }

    let mut out = Vec::new();
    for (i, agency, origin, site, _firm) in do_rozstrzygniecia {
        let srodek = naloz_srodek(city, market, tuning, agency, origin, site, t);
        city.enforcement.cases[i].remedy = Some(srodek);
        city.enforcement.cases[i].closed_at = Some(t);
        city.enforcement.agencies[agency.as_index()].closed += 1;
        out.push((
            site,
            DecisionReason::RemedyImposed {
                agency,
                remedy: srodek.kind(),
                amount: srodek.amount(),
            },
        ));
    }
    out
}

/// Najniższa kara za zmowę, w groszach. Zmowa firmy bez zmierzonego obrotu
/// (zakład produkcyjny, sklep świeżo otwarty) i tak ma kosztować: kara zero
/// znaczyłaby, że opłaca się zmawiać przed pierwszym rachunkiem wyniku.
const CARTEL_MIN_FINE: i64 = 100_000;

/// Nakłada środek zaradczy i wykonuje go. Jedno miejsce, bo wykonanie i zapis
/// muszą iść razem: środek zapisany bez skutku byłby napisem w kartotece.
#[allow(clippy::too_many_arguments)]
fn naloz_srodek(
    city: &mut City,
    market: &Market,
    tuning: &CityTuning,
    agency: AgencyKind,
    origin: CaseOrigin,
    site: SiteId,
    t: Tick,
) -> Remedy {
    let p = tuning.agencies;
    match agency {
        AgencyKind::TaxOffice => {
            let ukryte_bp = i64::from(market.unreported_bps_of(site));
            // Podstawa: dwanaście domkniętych miesięcy zadeklarowanego utargu,
            // przeliczone na to, czego w nich nie było. Fiskus przyjmuje stawkę
            // podstawową — nie ma jak odtworzyć, które towary schowano.
            let zadeklarowane = market.declared_revenue_recent(site, 12).get();
            // Mianownik przycina się do jedynki, a nie do gałęzi: `set_unreported_bps`
            // jest publiczne i przepuszcza 9 999, więc warunek „>= 10 000" chroniłby
            // wyłącznie przed wartością, której dziś nikt nie ustawia.
            let podstawa = zadeklarowane
                .saturating_mul(ukryte_bp)
                .saturating_div((10_000 - ukryte_bp).max(1));
            let stawka = city.code.standard_vat_bp();
            let vat = podstawa.saturating_mul(i64::from(stawka)) / 10_000;
            let sankcja = vat.saturating_mul(i64::from(p.back_tax_penalty_bp)) / 10_000;
            // Odsetki od **średniego okresu zaległości w oknie dwunastu miesięcy**,
            // czyli pół roku gry. Liczenie ich od początku świata dawało po roku gry
            // ten sam, przycięty do sufitu nalicz dla każdego domiaru — niezależnie
            // od tego, jak długo zakład naprawdę ukrywał obrót.
            const SREDNI_OKRES_DOB: u32 = 180;
            let odsetki = crate::calc::late_interest(
                Money(vat),
                city.code.late_interest_bp_per_year,
                SREDNI_OKRES_DOB,
            );
            let kwota = Money(vat + sankcja + odsetki.get());
            if kwota.get() > 0 {
                let okres = crate::assess::poprzedni_miesiac(t);
                city.charges.accrue(
                    TaxPayer::Site(site),
                    TaxKind::Vat,
                    okres,
                    Money(podstawa),
                    Mass::ZERO,
                    stawka,
                    kwota,
                    t,
                    due_on_day(t, city.code.vat_due_day),
                );
            }
            // Złapany przestaje ukrywać. Nie z przekonania — z arytmetyki: domiar
            // z sankcją i odsetkami jest droższy niż to, co się na tym zarobiło.
            market.set_unreported_bps(site, 0);
            Remedy::BackTax {
                amount: Money(vat + sankcja),
                interest: odsetki,
            }
        }
        AgencyKind::Sanitary => {
            let until = Tick(t.get() + u64::from(p.sanitary_closure_days) * 1_440);
            market.suspend_site(site, until);
            // Kartoteka zeruje się razem z zamknięciem sprawy. Bez tego licznik
            // odpisów jest narastający od otwarcia zakładu i sklep, który raz
            // przekroczył próg, przekracza go już zawsze — kara zamieniłaby się
            // w stan, a sanepid w podatek.
            market.reset_expired_mass(site);
            Remedy::Closure { until }
        }
        // Zmowa cenowa kończy się **karą pieniężną**, a nie podziałem: zmówiło się
        // kilku, a zamknięcie każdego z nich zabrałoby dzielnicy cały handel tym
        // towarem naraz — kara spadłaby na klientów, nie na winnych (M10e WP10.13).
        AgencyKind::Antitrust if origin == CaseOrigin::Cartel => {
            let podstawa = market.declared_revenue_recent(site, 12);
            let kwota =
                Money((podstawa.get() * i64::from(p.fine_bp) / 10_000).max(CARTEL_MIN_FINE));
            city.charges.accrue(
                TaxPayer::Site(site),
                TaxKind::License,
                crate::assess::poprzedni_miesiac(t),
                podstawa,
                Mass::ZERO,
                p.fine_bp,
                kwota,
                t,
                due_on_day(t, city.code.vat_due_day),
            );
            Remedy::Fine(kwota)
        }
        AgencyKind::Antitrust => {
            // ponytail: przymusowy podział wykonuje się dziś **zamknięciem** zakładu,
            // bo nie ma komu go sprzedać — rynek kontroli nad firmą (giełda, przejęcia)
            // należy do M10. Skutek dla udziału rynkowego jest ten sam, koszt dla
            // gospodarki większy, i to jest różnica, którą M10 usunie.
            market.close_shop(site);
            Remedy::ForcedDivestiture {
                share_bps: p.antitrust_share_bp,
            }
        }
        AgencyKind::LaborInspection | AgencyKind::Environment => {
            let podstawa = market.declared_revenue_recent(site, 12);
            let kwota =
                Money((podstawa.get().saturating_mul(i64::from(p.fine_bp)) / 10_000).max(50_000));
            city.charges.accrue(
                TaxPayer::Site(site),
                TaxKind::License,
                crate::assess::poprzedni_miesiac(t),
                podstawa,
                Mass::ZERO,
                p.fine_bp,
                kwota,
                t,
                due_on_day(t, city.code.vat_due_day),
            );
            Remedy::Fine(kwota)
        }
    }
}
