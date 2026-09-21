//! Wybory: kto staje, kto idzie głosować i na kogo (M8e WP10, PRD §10.2).
//!
//! Trzy rzeczy, które ten moduł ma zrobić dobrze, bo z nich wynika test T6:
//!
//! 1. **Ten sam seed daje ten sam wynik co do głosu.** Iteracja po indeksie
//!    mieszkańca rosnąco, rzut z czystej funkcji `rng(seed, Election, idx, tick)`,
//!    zero `HashMap`, zero floatów. Wybór kandydata idzie **całkowitoliczbowym
//!    wyborem temperowanym**, a nie softmaxem na `f64` — kształt jest ten sam
//!    (więcej użyteczności = większa szansa), a determinizm trywialny.
//! 2. **Poparcie zależy od dzielnicy.** Jedyny składnik użyteczności, który różni
//!    się między obwodami, to ocena usług w **tej** dzielnicy — reszta jest wspólna
//!    dla miasta. Bez tego test wrażliwości („pogorszenie usług w jednej dzielnicy
//!    o 30 % obniża tam poparcie inkumbenta") mierzyłby szum.
//! 3. **Głos ma powód.** Wyjaśnialność (`00` §7) dotyczy wyborcy tak samo jak firmy:
//!    [`VoteDriver`] niesie największy składnik użyteczności, a nie całą rozpiskę.
//!
//! Czego tu nie ma i dlaczego: **głosu per mandat okręgowy**. Mandaty dzielą się
//! metodą największych reszt z sumy głosów w całym mieście, bo okręgi jednomandatowe
//! wymagałyby podziału miasta na okręgi, a dzielnic jest 10–40 i zmieniają się
//! z rozmiaru mapy. Wynik per dzielnica zostaje w [`DistrictTally`] i to on jest
//! treścią zdania z §1 dokumentu fazy („przegrana w tym obwodzie").

use magnat_core::{
    rng, CitizenReason, DecisionReason, DistrictId, FirmReason, Money, StreamId, TaxKind, Tick,
    VoteDriver, TAX_KIND_COUNT,
};
use magnat_firms::FirmKey;

use crate::gov::{GovTuning, Government, Preference};

/// Ile głosów trzyma pierścień wyjaśnień. Ta sama liczba i ten sam powód
/// co przy `Market::budget_log` (M5): pełny log 400 tys. wyborców na kadencję
/// to kilkanaście megabajtów na wybory i nikt tego nie czyta w całości.
/// Rozkład motywów nie ginie — jest w histogramie per dzielnica.
pub const VOTE_LOG_RING: usize = 256;

/// Kto wspiera kampanię.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Backer {
    Firm(FirmKey),
    Citizen(u32),
    Player,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Legality {
    Legal,
    Illegal,
}

/// Kandydat na burmistrza.
#[derive(Clone, Debug)]
pub struct Candidate {
    /// Indeks encji mieszkańca.
    pub citizen: u32,
    pub district: DistrictId,
    pub platform: Preference,
    /// Deklarowana zmiana stawki w punktach bazowych. Wyborca czyta ją wprost:
    /// obniżka VAT-u jest warta więcej biedniejszemu, obniżka CIT-u bogatszemu.
    pub tax_stance: Vec<(TaxKind, i32)>,
    pub funding: Money,
    /// Część kampanii z pieniędzy poza rejestrem wpłat.
    pub illegal_funding: Money,
    pub backers: Vec<(Backer, Money, Legality)>,
    pub incumbent: bool,
}

impl Candidate {
    #[must_use]
    pub fn new(citizen: u32, district: DistrictId, platform: Preference) -> Candidate {
        Candidate {
            citizen,
            district,
            platform: platform.normalized(),
            tax_stance: Vec::new(),
            funding: Money::ZERO,
            illegal_funding: Money::ZERO,
            backers: Vec::new(),
            incumbent: false,
        }
    }

    /// Zasięg medialny kampanii: złotówki na jednego wyborcę, przemnożone przez
    /// przelicznik z danych. Pieniądz poza rejestrem działa mocniej i **to jest
    /// cała jego atrakcyjność** — razem ze sprawą, którą otwiera u wpłacającego.
    ///
    /// To jest cały model mediów w tej fazie (`D7`): M10 podmienia **ciało tej
    /// jednej metody** na zasięg realnych tytułów, bez zmiany sygnatury i bez
    /// dotykania funkcji użyteczności wyborcy.
    #[must_use]
    pub fn reach_bp(&self, voters: u32, t: &GovTuning) -> i64 {
        if voters == 0 {
            return 0;
        }
        let legalne = self.funding.get() - self.illegal_funding.get();
        let wazone =
            legalne + self.illegal_funding.get() * i64::from(t.illegal_reach_mul_bp) / 10_000;
        // **Mnożenie przed dzieleniem.** Odwrotna kolejność zerowała zasięg skokowo
        // przy dzielnicy większej niż kampania w złotówkach — a to jest każda
        // dzielnica poza najmniejszą. Grosze × przelicznik mieszczą się w `i64`
        // z dużym zapasem (kampania rzędu 10⁹ groszy × 60 to 6·10¹⁰).
        wazone * i64::from(t.media_reach_bp_per_zl) / 100 / i64::from(voters).max(1)
    }
}

/// Wynik w jednej dzielnicy.
#[derive(Clone, Debug)]
pub struct DistrictTally {
    pub district: DistrictId,
    pub eligible: u32,
    pub voted: u32,
    /// Głosy na kandydatów, w kolejności listy.
    pub votes: Vec<u32>,
    /// Histogram motywów — [`VoteDriver::as_index`].
    pub drivers: [u32; 6],
}

impl DistrictTally {
    #[must_use]
    pub fn turnout_bp(&self) -> u32 {
        if self.eligible == 0 {
            return 0;
        }
        u32::try_from(u64::from(self.voted) * 10_000 / u64::from(self.eligible)).unwrap_or(0)
    }

    /// Udział kandydata w głosach dzielnicy, w punktach bazowych.
    #[must_use]
    pub fn share_bp(&self, candidate: usize) -> u32 {
        let suma: u32 = self.votes.iter().sum();
        if suma == 0 {
            return 0;
        }
        let g = self.votes.get(candidate).copied().unwrap_or(0);
        u32::try_from(u64::from(g) * 10_000 / u64::from(suma)).unwrap_or(0)
    }
}

#[derive(Clone, Debug)]
pub struct ElectionResult {
    pub turnout_bp: u32,
    pub per_district: Vec<DistrictTally>,
    /// Mandaty: (indeks kandydata, liczba mandatów).
    pub council: Vec<(u8, u8)>,
    /// Indeks zwycięskiego kandydata.
    pub mayor: u8,
    pub winner_bp: u32,
    pub incumbent_won: bool,
}

#[derive(Clone, Debug)]
pub struct Election {
    pub scheduled_at: Tick,
    pub term_ticks: u64,
    pub candidates: Vec<Candidate>,
    pub result: Option<ElectionResult>,
    /// Ostatnie [`VOTE_LOG_RING`] wyjaśnień głosu — treść zakładki „Wybory".
    pub vote_log: Vec<(u32, DecisionReason)>,
}

impl Election {
    #[must_use]
    pub fn new(scheduled_at: Tick, term_ticks: u64, candidates: Vec<Candidate>) -> Election {
        Election {
            scheduled_at,
            term_ticks,
            candidates,
            result: None,
            vote_log: Vec::new(),
        }
    }

    /// Wsparcie kampanii. Zwraca powód do dziennika; **pieniądz przelewa wołający**,
    /// bo `sim/city` nie sięga do konta firmy bez `&mut Books`.
    ///
    /// To jest wejście, którym wchodzi gracz (M9) i firma AI. Jedno, nie dwa —
    /// `K-11` mówi, co sądzimy o dwóch ścieżkach do jednej mechaniki.
    pub fn back_candidate(
        &mut self,
        candidate: usize,
        backer: Backer,
        amount: Money,
        legality: Legality,
    ) -> Option<DecisionReason> {
        if amount.get() <= 0 {
            return None;
        }
        let c = self.candidates.get_mut(candidate)?;
        c.funding = Money(c.funding.get() + amount.get());
        if legality == Legality::Illegal {
            c.illegal_funding = Money(c.illegal_funding.get() + amount.get());
        }
        c.backers.push((backer, amount, legality));
        Some(DecisionReason::Firm(FirmReason::CampaignBacked {
            candidate: u8::try_from(candidate).unwrap_or(u8::MAX),
            amount,
            illegal: legality == Legality::Illegal,
        }))
    }

    /// Ile pieniędzy poza rejestrem stoi za kandydatem, w punktach bazowych
    /// jego kampanii — miara afery, gdy ta wyjdzie na jaw.
    ///
    /// Ujawnienie idzie **sprawą urzędu antymonopolowego**, a nie zdarzeniem
    /// (`CI-9`): wpłata spoza rejestru otwiera postępowanie u wpłacającego,
    /// dowody rosną, kara przychodzi na końcu, a kandydat traci zasięg kupiony
    /// za te pieniądze. Druga ścieżka do tego samego skutku — sonda i definicja
    /// zdarzenia — byłaby tym, przed czym bronią `K-11` i `K-13`.
    #[must_use]
    pub fn illegal_share_bp(&self, candidate: usize) -> u32 {
        let Some(c) = self.candidates.get(candidate) else {
            return 0;
        };
        if c.funding.get() <= 0 {
            return 0;
        }
        u32::try_from(c.illegal_funding.get() * 10_000 / c.funding.get()).unwrap_or(0)
    }

    pub fn hash_state(&self, h: &mut magnat_core::StateHasher) {
        h.write_u64(self.scheduled_at.0);
        h.write_u64(self.term_ticks);
        h.write_u64(self.candidates.len() as u64);
        for c in &self.candidates {
            h.write_u32(c.citizen);
            h.write_u16(c.district.0);
            h.write_u32(c.platform.growth_bps);
            h.write_u32(c.platform.social_bps);
            h.write_u32(c.platform.green_bps);
            h.write_u32(c.platform.populist_bps);
            h.write_i64(c.funding.get());
            h.write_i64(c.illegal_funding.get());
            h.write_u8(u8::from(c.incumbent));
        }
        match &self.result {
            None => h.write_u8(0),
            Some(r) => {
                h.write_u8(1);
                h.write_u32(r.turnout_bp);
                h.write_u8(r.mayor);
                h.write_u32(r.winner_bp);
                for d in &r.per_district {
                    h.write_u16(d.district.0);
                    h.write_u32(d.voted);
                    for v in &d.votes {
                        h.write_u32(*v);
                    }
                }
                for (i, m) in &r.council {
                    h.write_u8(*i);
                    h.write_u8(*m);
                }
            }
        }
    }
}

/// Wyborca tak, jak widzi go komisja. Struktura, a nie zapytanie do ECS, bo krok
/// miasta ma zasób wyjęty ze świata i nie może trzymać `&World` obok `&mut City`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct VoterView {
    pub citizen: u32,
    pub district: DistrictId,
    pub age_years: u32,
    /// `Vitals.status`, 0..=100.
    pub status: u8,
    /// `Vitals.mood`, −100..=100.
    pub mood: i8,
    pub employed: bool,
}

/// Frekwencja jednego wyborcy w punktach bazowych.
///
/// Wyższy status i starszy rocznik głosują częściej — to jest kryterium
/// akceptacyjne z T6, więc stoi w jednej, sprawdzalnej funkcji, a nie rozlane
/// po pętli. Nastrój działa w obie strony: człowiek wściekły też idzie głosować.
#[must_use]
pub fn turnout_bp(v: &VoterView, t: &GovTuning) -> u32 {
    let lata = i64::from(v.age_years.saturating_sub(18).min(60));
    let mut bp = i64::from(t.turnout_base_bp);
    bp += i64::from(v.status) * i64::from(t.turnout_status_bp);
    bp += lata * i64::from(t.turnout_age_bp);
    // Nastrój liczy się **modułem**: zadowolony broni władzy, wściekły ją zmienia,
    // a obojętny zostaje w domu. Frekwencja rosnąca wyłącznie z zadowolenia
    // dawałaby miasto, w którym kryzys obniża zainteresowanie wyborami.
    bp += i64::from(v.mood.unsigned_abs()) * i64::from(t.turnout_mood_bp);
    u32::try_from(bp.clamp(
        i64::from(t.turnout_min_bp),
        i64::from(t.turnout_max_bp.max(t.turnout_min_bp)),
    ))
    .unwrap_or(t.turnout_base_bp)
}

/// Użyteczność kandydata dla wyborcy, z rozbiciem na motywy.
///
/// Wszystko w punktach bazowych i wszystko całkowitoliczbowe.
fn utility(
    v: &VoterView,
    c: &Candidate,
    idx: usize,
    approval_bp: u32,
    voters_in_district: u32,
    t: &GovTuning,
) -> (i64, VoteDriver) {
    let mut skladniki: [i64; 6] = [0; 6];

    // Podatki: obniżka jest warta tyle, ile wyborca na niej zyskuje. Biedniejszy
    // płaci głównie VAT-em (kupuje za cały dochód), zamożniejszy odczuwa CIT i PIT.
    let biedny = 100 - i64::from(v.status);
    for (kind, delta) in &c.tax_stance {
        let waga = match kind {
            TaxKind::Vat => biedny,
            TaxKind::Pit => 60 + i64::from(v.status) / 4,
            TaxKind::Cit | TaxKind::Property => i64::from(v.status),
            _ => 20,
        };
        skladniki[VoteDriver::Taxes.as_index()] -= i64::from(*delta) * waga / 100;
    }

    // Usługi: inkumbent odpowiada za to, co wyborca ma **u siebie**. Wyzwanie
    // działa odwrotnie — im gorzej w dzielnicy, tym bardziej opłaca się zmiana.
    let odchylka = i64::from(approval_bp) - 5_000;
    skladniki[VoteDriver::Services.as_index()] = if c.incumbent { odchylka } else { -odchylka / 2 };

    // Nastrój: zły nastrój jest głosem przeciwko władzy, dobry — za nią.
    skladniki[VoteDriver::Mood.as_index()] = i64::from(v.mood) * if c.incumbent { 20 } else { -12 };
    // Bezrobotny liczy na zmianę mocniej niż pracujący.
    if !v.employed && !c.incumbent {
        skladniki[VoteDriver::Mood.as_index()] += 600;
    }

    skladniki[VoteDriver::Media.as_index()] = c.reach_bp(voters_in_district, t);

    // Więź: kandydat z tej samej dzielnicy jest „nasz". To jest cały model
    // więzi osobistych w tej fazie — graf relacji M3 wchodzi w M10 razem
    // ze związkami i rodami, bo tam ma drugiego konsumenta.
    if c.district == v.district {
        skladniki[VoteDriver::Ties.as_index()] = 900;
    }

    // Przyzwyczajenie: urzędujący burmistrz startuje z przewagi status quo,
    // a pierwszy na liście z tego, że jest pierwszy. Obie przewagi są małe
    // i obie są prawdziwe.
    skladniki[VoteDriver::Habit.as_index()] = if c.incumbent {
        500
    } else {
        200 - 40 * idx as i64
    };

    let suma: i64 = skladniki.iter().sum();
    // Motyw = największy **dodatni** składnik. Gdy wszystkie są ujemne, głos padł
    // z przyzwyczajenia i tak też się go zapisuje — to jest prawdziwa odpowiedź.
    let mut motyw = VoteDriver::Habit;
    let mut naj = 0i64;
    for (i, s) in skladniki.iter().enumerate() {
        if *s > naj {
            naj = *s;
            motyw = VoteDriver::from_index(i).unwrap_or(VoteDriver::Habit);
        }
    }
    (suma, motyw)
}

/// Przeprowadzenie wyborów (`EveryDay` z bramką „dzień wyborów", §5.7).
///
/// `approval` to poparcie inkumbenta per dzielnica z [`Government`]. Iteracja idzie
/// po `voters` w kolejności, w jakiej je podano — wołający ma je podać po indeksie
/// mieszkańca rosnąco, i to jest jedyny wymóg determinizmu po jego stronie.
pub fn run_election(
    e: &mut Election,
    voters: &[VoterView],
    approval: &[u32],
    t: &GovTuning,
    seed: u64,
    tick: Tick,
) -> Option<ElectionResult> {
    if e.candidates.is_empty() || voters.is_empty() {
        return None;
    }
    let n = e.candidates.len();
    let dzielnic = approval.len().max(1);

    // Ilu wyborców ma każda dzielnica — potrzebne do zasięgu kampanii na osobę.
    // Dzielnica spoza zakresu liczy się do ostatniej — tak samo jak przy oddawaniu
    // głosu niżej. Dwa różne przycięcia dałyby frekwencję powyżej stu procent.
    let mut uprawnieni = vec![0u32; dzielnic];
    for v in voters {
        let d = usize::from(v.district.0).min(dzielnic - 1);
        uprawnieni[d] += 1;
    }

    let mut tally: Vec<DistrictTally> = (0..dzielnic)
        .map(|d| DistrictTally {
            district: DistrictId(d as u16),
            eligible: uprawnieni[d],
            voted: 0,
            votes: vec![0; n],
            drivers: [0; 6],
        })
        .collect();

    let mut log: Vec<(u32, DecisionReason)> = Vec::new();
    let mut uzytecznosci: Vec<i64> = vec![0; n];
    let mut wagi: Vec<u64> = vec![0; n];

    for v in voters {
        let d = usize::from(v.district.0).min(dzielnic - 1);
        let mut r = rng(seed, StreamId::Election, v.citizen, tick);
        if r.next_u32() % 10_000 >= turnout_bp(v, t) {
            continue;
        }
        let poparcie = approval.get(d).copied().unwrap_or(5_000);
        let mut motywy = [VoteDriver::Habit; 8];
        for (i, c) in e.candidates.iter().enumerate() {
            let (u, m) = utility(v, c, i, poparcie, uprawnieni[d], t);
            uzytecznosci[i] = u;
            if i < motywy.len() {
                motywy[i] = m;
            }
        }
        // Wybór temperowany: waga kandydata maleje liniowo z odległością od
        // najlepszego i nigdy nie schodzi poniżej jedynki. Kształt jak w softmaksie,
        // arytmetyka całkowita — a przy okazji bez `exp`, którego `K-6` i tak
        // zabrania brać ze std.
        let top = *uzytecznosci.iter().max().unwrap_or(&0);
        let temp = i64::from(t.vote_temperature_bp.max(1));
        let mut suma = 0u64;
        for (i, u) in uzytecznosci.iter().enumerate() {
            let w = (temp - (top - *u)).max(1) as u64;
            wagi[i] = w;
            suma += w;
        }
        let los = u64::from(r.next_u32()) % suma.max(1);
        let mut akumulator = 0u64;
        let mut wybrany = 0usize;
        for (i, w) in wagi.iter().enumerate() {
            akumulator += *w;
            if los < akumulator {
                wybrany = i;
                break;
            }
        }
        // Przewaga nad drugim w rankingu **tego** wyborcy — nie nad drugim w mieście.
        let mut drugi = i64::MIN;
        for (i, u) in uzytecznosci.iter().enumerate() {
            if i != wybrany && *u > drugi {
                drugi = *u;
            }
        }
        let przewaga = if drugi == i64::MIN {
            0
        } else {
            (uzytecznosci[wybrany] - drugi).clamp(0, 65_535) as u16
        };

        let tw = &mut tally[d];
        tw.voted += 1;
        tw.votes[wybrany] += 1;
        let motyw = motywy[wybrany.min(motywy.len() - 1)];
        tw.drivers[motyw.as_index()] += 1;
        if log.len() < VOTE_LOG_RING {
            log.push((
                v.citizen,
                DecisionReason::Citizen(CitizenReason::VoteCast {
                    candidate: u8::try_from(wybrany).unwrap_or(u8::MAX),
                    driver: motyw,
                    margin_bp: przewaga,
                }),
            ));
        }
    }

    let glosy: Vec<u32> = (0..n)
        .map(|i| tally.iter().map(|t| t.votes[i]).sum())
        .collect();
    let wszystkie: u32 = glosy.iter().sum();
    if wszystkie == 0 {
        return None;
    }
    let poszli: u32 = tally.iter().map(|t| t.voted).sum();
    let uprawnionych: u32 = tally.iter().map(|t| t.eligible).sum();

    // Zwycięzca: najwięcej głosów, remis rozstrzyga indeks na liście.
    let mayor = glosy
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(&a.0)))
        .map_or(0, |(i, _)| i);

    let wynik = ElectionResult {
        turnout_bp: u32::try_from(u64::from(poszli) * 10_000 / u64::from(uprawnionych.max(1)))
            .unwrap_or(0),
        council: mandaty(&glosy, t.council_seats),
        mayor: u8::try_from(mayor).unwrap_or(0),
        winner_bp: u32::try_from(u64::from(glosy[mayor]) * 10_000 / u64::from(wszystkie))
            .unwrap_or(0),
        incumbent_won: e.candidates[mayor].incumbent,
        per_district: tally,
    };
    e.vote_log = log;
    e.result = Some(wynik.clone());
    Some(wynik)
}

/// Podział mandatów metodą największych reszt.
///
/// Reszta idzie do kandydata o **największej** części ułamkowej, a remis rozstrzyga
/// kolejność na liście — ta sama reguła co przy podziale kwoty między N stron
/// (`00` §2). Suma mandatów równa się liczbie miejsc w radzie, zawsze.
#[must_use]
pub fn mandaty(glosy: &[u32], miejsca: u8) -> Vec<(u8, u8)> {
    let suma: u64 = glosy.iter().map(|g| u64::from(*g)).sum();
    if suma == 0 || miejsca == 0 {
        return Vec::new();
    }
    let m = u64::from(miejsca);
    let mut pelne: Vec<u64> = glosy.iter().map(|g| u64::from(*g) * m / suma).collect();
    let rozdane: u64 = pelne.iter().sum();
    let mut reszty: Vec<(u64, usize)> = glosy
        .iter()
        .enumerate()
        .map(|(i, g)| (u64::from(*g) * m % suma, i))
        .collect();
    // Malejąco po reszcie, rosnąco po indeksie — remis jest rozstrzygnięty jawnie.
    reszty.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    for (_, i) in reszty.iter().take((m - rozdane) as usize) {
        pelne[*i] += 1;
    }
    pelne
        .iter()
        .enumerate()
        .filter(|(_, v)| **v > 0)
        .map(|(i, v)| {
            (
                u8::try_from(i).unwrap_or(u8::MAX),
                u8::try_from(*v).unwrap_or(u8::MAX),
            )
        })
        .collect()
}

/// Skład rady po wyborach: mandat dostaje program kandydata, który go zdobył.
#[must_use]
pub fn council_from(result: &ElectionResult, candidates: &[Candidate]) -> Vec<(u32, Preference)> {
    let mut out = Vec::new();
    for (i, ile) in &result.council {
        let Some(c) = candidates.get(usize::from(*i)) else {
            continue;
        };
        for _ in 0..*ile {
            out.push((c.citizen, c.platform));
        }
    }
    out
}

/// Deklaracja podatkowa kandydata wyprowadzona z jego programu.
///
/// Prorozwojowy obniża CIT, socjalny podnosi go i obniża VAT, ekologiczny sięga
/// po akcyzę, populista obiecuje obniżkę wszystkiego — i to jest jedyna obietnica,
/// której miasto potem nie da rady spełnić, bo widełki `rate_bands` mają podłogę.
#[must_use]
pub fn stance_from(platform: &Preference, step_bp: u32) -> Vec<(TaxKind, i32)> {
    let s = i32::try_from(step_bp).unwrap_or(100);
    let mut out = Vec::new();
    let os = platform.dominant();
    match os {
        crate::gov::Axis::Growth => out.push((TaxKind::Cit, -s)),
        crate::gov::Axis::Social => {
            out.push((TaxKind::Cit, s));
            out.push((TaxKind::Vat, -s));
        }
        crate::gov::Axis::Green => out.push((TaxKind::Excise, s)),
        crate::gov::Axis::Populist => {
            out.push((TaxKind::Vat, -s));
            out.push((TaxKind::Pit, -s));
        }
    }
    out
}

/// Stawki, jakie kandydat wprowadziłby po objęciu urzędu — obietnica wyborcza
/// przyłożona do obowiązującego kodeksu i przycięta do widełek.
#[must_use]
pub fn promised_rates(
    c: &Candidate,
    rates: &[u32; TAX_KIND_COUNT],
    t: &GovTuning,
) -> [u32; TAX_KIND_COUNT] {
    let mut out = *rates;
    for (kind, delta) in &c.tax_stance {
        let i = kind.as_index();
        let (min_bp, max_bp) = t.band(*kind);
        let nowa = i64::from(out[i]) + i64::from(*delta);
        out[i] = u32::try_from(nowa.clamp(i64::from(min_bp), i64::from(max_bp))).unwrap_or(out[i]);
    }
    out
}

/// Wyłonienie kandydatów (`EveryDay` w dniu ogłoszenia wyborów).
///
/// Staje **inkumbent i trzech mieszkańców o najwyższym statusie z różnych dzielnic**.
/// Nie jest to model partii i nie udaje nim być: kandydat bierze się z tego, kogo
/// w mieście widać, a program losuje się z jego indeksu — czysta funkcja seeda
/// i tożsamości, więc ten sam świat daje tę samą listę.
#[must_use]
pub fn nominate(
    gov: &Government,
    voters: &[VoterView],
    t: &GovTuning,
    seed: u64,
    tick: Tick,
) -> Vec<Candidate> {
    let mut lista: Vec<Candidate> = Vec::new();
    if gov.mayor != Government::NO_MAYOR {
        let d = voters
            .iter()
            .find(|v| v.citizen == gov.mayor)
            .map_or(DistrictId(0), |v| v.district);
        let mut c = Candidate::new(gov.mayor, d, gov.mayor_pref);
        c.incumbent = true;
        c.tax_stance = stance_from(&gov.mayor_pref, t.tax_step_bp);
        lista.push(c);
    }
    // Najwyższy status, po jednym na dzielnicę, żeby lista nie była jedną ulicą.
    let mut wg_statusu: Vec<&VoterView> = voters
        .iter()
        .filter(|v| v.age_years >= 25 && v.citizen != gov.mayor)
        .collect();
    wg_statusu.sort_unstable_by(|a, b| b.status.cmp(&a.status).then(a.citizen.cmp(&b.citizen)));
    let mut zajete: Vec<u16> = Vec::new();
    for v in wg_statusu {
        if lista.len() >= usize::from(t.candidates.max(2)) {
            break;
        }
        if zajete.contains(&v.district.0) {
            continue;
        }
        zajete.push(v.district.0);
        let mut r = rng(seed, StreamId::Election, v.citizen, tick);
        let p = Preference {
            growth_bps: r.next_u32() % 4_000 + 500,
            social_bps: r.next_u32() % 4_000 + 500,
            green_bps: r.next_u32() % 4_000 + 500,
            populist_bps: r.next_u32() % 4_000 + 500,
        }
        .normalized();
        let mut c = Candidate::new(v.citizen, v.district, p);
        c.tax_stance = stance_from(&p, t.tax_step_bp);
        lista.push(c);
    }
    lista
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tuning() -> GovTuning {
        GovTuning::load_default().expect("data/city/government.ron")
    }

    fn wyborcy(n: u32, dzielnic: u16) -> Vec<VoterView> {
        (0..n)
            .map(|i| VoterView {
                citizen: i,
                district: DistrictId((i % u32::from(dzielnic)) as u16),
                age_years: 20 + i % 55,
                status: u8::try_from(i % 101).unwrap_or(50),
                mood: i8::try_from(i % 61).unwrap_or(0) - 30,
                employed: i % 7 != 0,
            })
            .collect()
    }

    fn kandydaci() -> Vec<Candidate> {
        let mut inku = Candidate::new(
            0,
            DistrictId(0),
            Preference {
                growth_bps: 4_000,
                social_bps: 3_000,
                green_bps: 2_000,
                populist_bps: 1_000,
            },
        );
        inku.incumbent = true;
        let wyzwanie = Candidate::new(
            1,
            DistrictId(1),
            Preference {
                growth_bps: 1_000,
                social_bps: 4_000,
                green_bps: 3_000,
                populist_bps: 2_000,
            },
        );
        vec![inku, wyzwanie]
    }

    /// T6, pierwsza połowa: ten sam seed → ten sam wynik co do głosu.
    #[test]
    fn wybory_sa_deterministyczne_co_do_glosu() {
        let t = tuning();
        let v = wyborcy(3_000, 6);
        let popr = vec![5_000u32; 6];
        let mut a = Election::new(Tick(0), 0, kandydaci());
        let mut b = Election::new(Tick(0), 0, kandydaci());
        let ra = run_election(&mut a, &v, &popr, &t, 7, Tick(1_000)).expect("wynik");
        let rb = run_election(&mut b, &v, &popr, &t, 7, Tick(1_000)).expect("wynik");
        assert_eq!(ra.turnout_bp, rb.turnout_bp);
        assert_eq!(ra.mayor, rb.mayor);
        for (x, y) in ra.per_district.iter().zip(rb.per_district.iter()) {
            assert_eq!(x.votes, y.votes, "dzielnica {}", x.district.0);
        }
        assert_eq!(a.vote_log.len(), b.vote_log.len());
    }

    /// T6, druga połowa: pogorszenie usług w jednej dzielnicy o 30 % obniża tam
    /// poparcie inkumbenta o co najmniej 5 punktów procentowych.
    #[test]
    fn spadek_uslug_w_dzielnicy_zbija_poparcie_inkumbenta() {
        let t = tuning();
        let v = wyborcy(20_000, 6);
        let kontrola = vec![7_000u32; 6];
        let mut gorzej = kontrola.clone();
        gorzej[2] = 7_000 * 70 / 100;

        let mut a = Election::new(Tick(0), 0, kandydaci());
        let mut b = Election::new(Tick(0), 0, kandydaci());
        let ra = run_election(&mut a, &v, &kontrola, &t, 11, Tick(5_000)).expect("wynik");
        let rb = run_election(&mut b, &v, &gorzej, &t, 11, Tick(5_000)).expect("wynik");

        let przed = ra.per_district[2].share_bp(0);
        let po = rb.per_district[2].share_bp(0);
        assert!(
            przed >= po + 500,
            "poparcie w dzielnicy 2: {przed} bp → {po} bp, spadek {} bp",
            przed.saturating_sub(po)
        );
        // Dzielnica nietknięta ma zostać nietknięta — inaczej test mierzyłby szum.
        let inna_przed = ra.per_district[4].share_bp(0);
        let inna_po = rb.per_district[4].share_bp(0);
        assert!(
            inna_przed.abs_diff(inna_po) < 300,
            "{inna_przed} vs {inna_po}"
        );
    }

    /// T6, trzecia część: frekwencja w widełkach i rosnąca ze statusem i wiekiem.
    #[test]
    fn frekwencja_miesci_sie_w_widelkach_i_rosnie_ze_statusem() {
        let t = tuning();
        let v = wyborcy(20_000, 6);
        let mut e = Election::new(Tick(0), 0, kandydaci());
        let r = run_election(&mut e, &v, &[5_000; 6], &t, 3, Tick(99)).expect("wynik");
        assert!(
            (3_500..=7_500).contains(&r.turnout_bp),
            "frekwencja {} bp",
            r.turnout_bp
        );
        let mlody = VoterView {
            citizen: 1,
            district: DistrictId(0),
            age_years: 20,
            status: 10,
            mood: 0,
            employed: true,
        };
        let stary = VoterView {
            age_years: 70,
            status: 90,
            ..mlody
        };
        assert!(turnout_bp(&stary, &t) > turnout_bp(&mlody, &t));
    }

    #[test]
    fn mandaty_sumuja_sie_do_liczby_miejsc() {
        for glosy in [
            vec![100u32, 100, 100],
            vec![1, 0, 0],
            vec![7, 5, 3, 1],
            vec![1_000_000, 3, 2],
        ] {
            let m = mandaty(&glosy, 15);
            let suma: u32 = m.iter().map(|(_, v)| u32::from(*v)).sum();
            assert_eq!(suma, 15, "{glosy:?} → {m:?}");
        }
    }

    #[test]
    fn lapowka_dziala_mocniej_niz_darowizna() {
        let t = tuning();
        let mut e = Election::new(Tick(0), 0, kandydaci());
        e.back_candidate(0, Backer::Player, Money(100_000), Legality::Legal);
        let legalny = e.candidates[0].reach_bp(1_000, &t);
        let mut f = Election::new(Tick(0), 0, kandydaci());
        f.back_candidate(0, Backer::Player, Money(100_000), Legality::Illegal);
        let nielegalny = f.candidates[0].reach_bp(1_000, &t);
        assert!(nielegalny > legalny, "{nielegalny} vs {legalny}");
        assert_eq!(f.illegal_share_bp(0), 10_000);
        assert_eq!(e.illegal_share_bp(0), 0);
    }
}
