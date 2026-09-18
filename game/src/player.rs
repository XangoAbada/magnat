//! Postać gracza (M9c §5.3, WP4).
//!
//! # Gracz nie jest osobnym bytem
//!
//! Gracz to `CitizenId` — ten sam, którego obsługują systemy M3: potrzeby, planer dnia,
//! relacje, pamięć, zdrowie i demografia. [`PlayerCharacter`] jest **cienkim komponentem
//! sterującym** nad zwykłym mieszkańcem, a nie drugim modelem człowieka. Dzięki temu dom,
//! rodzina, praca i znajomi przychodzą za darmo z generatora M2/M3 — nie tworzymy
//! syntetycznego mieszkańca, bo syntetyczny nie miałby ani sąsiadów, ani historii.
//!
//! # Wariant startu to predykat wyboru plus łatka
//!
//! [`StartVariant`] nie daje graczowi mnożników. Daje **kogo wolno wybrać** ([`accepts`])
//! i **co się zmienia w chwili wyboru** ([`take_role`]): kapitał startowy, a dla inwestora
//! z zewnątrz — wyzerowane relacje. Reszta jest emergentna: nikt o nim nie wie, więc
//! pierwsza rekrutacja i pierwsi klienci są trudni (PRD §5.7).
//!
//! [`accepts`]: StartVariant::accepts

use magnat_agents::{Employment, Household, Identity};
use magnat_core::{CitizenId, HouseholdId, Money, SiteId, Tick};
use magnat_economy::{Market, TxKind, TxMemo};
use magnat_ecs::World;
use serde::{Deserialize, Serialize};

/// Ile pozycji pokazuje ekran wyboru postaci. Lista kandydatów jest **pierwszym
/// ekranem gry**, a nie spisem ludności: dwanaście nazwisk da się przeczytać,
/// dwadzieścia osiem tysięcy nie.
pub const CANDIDATES_SHOWN: usize = 12;

/// Wariant startu (PRD §13.1).
///
/// Wartość jedzie w kopercie `StartGame` od M9a, więc **kolejność wariantów jest
/// kontraktem dziennika wejść** — dopisywać wolno wyłącznie na końcu.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum StartVariant {
    /// Absolwent bez kapitału; pieniądze są pożyczką od rodziny.
    Graduate,
    /// Doświadczony pracownik z oszczędnościami.
    #[default]
    Worker,
    /// Spadkobierca małej firmy: zakład jest, gotówki mało.
    Heir,
    /// Inwestor z zewnątrz: kapitał jest, sieci relacji brak.
    Investor,
    /// Piaskownica: kapitał bez ograniczeń.
    Sandbox,
}

impl StartVariant {
    pub const ALL: [StartVariant; 5] = [
        StartVariant::Graduate,
        StartVariant::Worker,
        StartVariant::Heir,
        StartVariant::Investor,
        StartVariant::Sandbox,
    ];

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            StartVariant::Graduate => "graduate",
            StartVariant::Worker => "worker",
            StartVariant::Heir => "heir",
            StartVariant::Investor => "investor",
            StartVariant::Sandbox => "sandbox",
        }
    }

    /// Kapitał startowy wariantu.
    ///
    /// `ponytail:` pięć liczb w kodzie, a nie w `data/`. Sufit nazwany: format
    /// scenariusza opisanego danymi projektuje `M9e` (`data/scenarios/`), a do tego
    /// czasu osobny plik na pięć kwot byłby katalogiem z jednym wierszem. Kwoty są
    /// w groszach (00 §2) i celowo różnią się rzędem wielkości, bo to one, a nie
    /// modyfikatory, są całą różnicą między wariantami.
    #[must_use]
    pub const fn capital(self) -> Money {
        match self {
            StartVariant::Graduate => Money(300_000),
            StartVariant::Worker => Money(1_200_000),
            StartVariant::Heir => Money(500_000),
            StartVariant::Investor => Money(8_000_000),
            StartVariant::Sandbox => Money(100_000_000),
        }
    }

    /// Czy ten mieszkaniec może być tą postacią.
    ///
    /// To jest `candidate_filter` z §5.3. **Nie jest to `ConditionExpr`** i to jest
    /// świadoma korekta planu: język reguł (`sim/policy`) opisuje politykę firmy —
    /// metryki, na których stoi, mówią o cenach, zapasach i kadrach, a nie o wieku
    /// mieszkańca. Predykat nad trzema liczbami nie potrzebuje drzewa składniowego,
    /// a drzewo bez metryk demograficznych i tak nie umiałoby tego wyrazić.
    #[must_use]
    pub fn accepts(self, age_years: u32, employed: bool, savings: Money) -> bool {
        match self {
            StartVariant::Graduate => (20..=26).contains(&age_years) && !employed,
            StartVariant::Worker => {
                (30..=55).contains(&age_years) && employed && savings.get() >= 500_000
            }
            StartVariant::Heir => (25..=60).contains(&age_years),
            StartVariant::Investor => (30..=60).contains(&age_years),
            StartVariant::Sandbox => age_years >= 18,
        }
    }

    /// Czy wariant kasuje sieć relacji wybranego mieszkańca.
    #[must_use]
    pub const fn wipes_relations(self) -> bool {
        matches!(self, StartVariant::Investor)
    }
}

/// Które decyzje mieszkańca przejmuje gracz, a które dalej robi autopilot M3 (§5.3).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct PlayerAutonomy {
    pub job: Control,
    pub shopping: Control,
    pub housing: Control,
    pub vehicle: Control,
    /// Domyślnie `Auto` — inaczej gra jest nudna.
    pub leisure: Control,
    pub schedule: Control,
}

impl Default for PlayerAutonomy {
    fn default() -> PlayerAutonomy {
        PlayerAutonomy {
            job: Control::Manual,
            shopping: Control::Auto,
            housing: Control::Manual,
            vehicle: Control::Manual,
            leisure: Control::Auto,
            schedule: Control::Manual,
        }
    }
}

impl PlayerAutonomy {
    #[must_use]
    pub const fn get(&self, f: AutonomyField) -> Control {
        match f {
            AutonomyField::Job => self.job,
            AutonomyField::Shopping => self.shopping,
            AutonomyField::Housing => self.housing,
            AutonomyField::Vehicle => self.vehicle,
            AutonomyField::Leisure => self.leisure,
            AutonomyField::Schedule => self.schedule,
        }
    }

    pub fn set(&mut self, f: AutonomyField, c: Control) {
        match f {
            AutonomyField::Job => self.job = c,
            AutonomyField::Shopping => self.shopping = c,
            AutonomyField::Housing => self.housing = c,
            AutonomyField::Vehicle => self.vehicle = c,
            AutonomyField::Leisure => self.leisure = c,
            AutonomyField::Schedule => self.schedule = c,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Control {
    Manual,
    Auto,
}

/// Pole autonomii. Kolejność jest kontraktem dziennika wejść — niesie ją
/// `PlayerCommand::SetAutonomy`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum AutonomyField {
    Job,
    Shopping,
    Housing,
    Vehicle,
    Leisure,
    Schedule,
}

impl AutonomyField {
    pub const ALL: [AutonomyField; 6] = [
        AutonomyField::Job,
        AutonomyField::Shopping,
        AutonomyField::Housing,
        AutonomyField::Vehicle,
        AutonomyField::Leisure,
        AutonomyField::Schedule,
    ];

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            AutonomyField::Job => "job",
            AutonomyField::Shopping => "shopping",
            AutonomyField::Housing => "housing",
            AutonomyField::Vehicle => "vehicle",
            AutonomyField::Leisure => "leisure",
            AutonomyField::Schedule => "schedule",
        }
    }
}

/// Postać gracza — cienki komponent sterujący nad zwykłym mieszkańcem.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PlayerCharacter {
    pub citizen: CitizenId,
    pub household: HouseholdId,
    pub variant: StartVariant,
    pub autonomy: PlayerAutonomy,
    /// Zakłady, które gracz prowadzi.
    ///
    /// `ponytail:` własność jest tu **listą**, a nie udziałem w firmie. Sufit nazwany:
    /// `FoundFirm`, `OpenSite` i przeniesienie udziałów to `M9e` razem z panelami
    /// biznesowymi. Do tego czasu lista wystarcza do jedynej rzeczy, do której jest
    /// potrzebna: oznaczenia zakładów gracza jako śledzonych (`Z-3` z M5e).
    pub owned_sites: Vec<SiteId>,
    /// Kapitał, który postać dostała na starcie — wiersz slotu zapisu i karta.
    pub capital: Money,
    /// Kogo gracz wskazał na dziedzica. `None` = wybierze go `legacy::heir_of`
    /// po śmierci, deterministycznie i bez losowania.
    pub heir: Option<CitizenId>,
    /// Ile razy gracz ogłosił upadłość osobistą. Liczba do kroniki i do tytułu —
    /// **nie** do oceny kredytowej: tę bank wystawia z zaległości, które po
    /// upadłości zostają w księgach.
    pub bankruptcies: u8,
}

/// Jeden wiersz listy wyboru postaci.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Candidate {
    pub citizen: CitizenId,
    /// Imię i nazwisko z `data/names/` — nazwa własna, nie lokalizacja UI.
    pub name: String,
    pub age_years: u32,
    pub employed: bool,
    pub savings: Money,
    pub household_size: u8,
    pub district: u16,
}

/// Kandydaci na postać gracza: pierwsi [`CANDIDATES_SHOWN`] mieszkańcy spełniający
/// predykat wariantu, w kolejności indeksów encji.
///
/// Kolejność jest **deterministyczna i nielosowa** z rozmysłu: lista ma być ta sama
/// przy każdym wejściu do tego świata, bo gracz, który cofnął się z ekranu wyboru,
/// ma zobaczyć tych samych ludzi. Losowanie dokłada [`pick_random`] i robi to
/// z ziarna świata, a nie z zegara.
#[must_use]
pub fn candidates(world: &World, variant: StartVariant, day: u64) -> Vec<Candidate> {
    let Some(p) = world.get_resource::<magnat_agents::Population>() else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(CANDIDATES_SHOWN);
    for e in p.citizens() {
        if out.len() == CANDIDATES_SHOWN {
            break;
        }
        let Some(id) = world.get::<Identity>(*e) else {
            continue;
        };
        if !id.is_alive() {
            continue;
        }
        let wiek = id.age_years(day as i32).max(0) as u32;
        let praca = world
            .get::<Employment>(*e)
            .is_some_and(magnat_agents::Employment::is_employed);
        let hh = magnat_agents::household_by_index(world, id.household)
            .and_then(|h| world.get::<Household>(h).copied());
        let oszczednosci = hh.map_or(Money::ZERO, |h| {
            Money(h.cash.get() + h.bank.get() + h.savings.get())
        });
        if !variant.accepts(wiek, praca, oszczednosci) {
            continue;
        }
        out.push(Candidate {
            citizen: CitizenId(*e),
            name: magnat_ui::full_name(id),
            age_years: wiek,
            employed: praca,
            savings: oszczednosci,
            household_size: hh.map_or(0, |h| h.size),
            district: hh.map_or(0, |h| h.district),
        });
    }
    out
}

/// Losuje kandydata z ziarna świata — „wylosuj postać" z §1 dokumentu fazy.
///
/// Czysta funkcja `(ziarno, lista)`, więc ten sam świat daje tego samego kandydata
/// przy każdym kliknięciu: to nie jest kostka, tylko podpowiedź.
#[must_use]
pub fn pick_random(cands: &[Candidate], seed: u64) -> Option<CitizenId> {
    if cands.is_empty() {
        return None;
    }
    let i = (magnat_core::mix64(seed) % cands.len() as u64) as usize;
    Some(cands[i].citizen)
}

/// Wybór postaci: łatka wariantu i przypięcie do warstwy Mikro.
///
/// Zwraca `None`, gdy wskazany mieszkaniec nie istnieje albo nie żyje — komenda jest
/// wtedy odrzucona i tak samo odrzuci ją replay.
///
/// Pieniądz **wchodzi kanałem emisji** (`TxKind::Endowment`), tym samym, którym generator
/// stawia kapitał firm: łatka dzieje się w ticku 0 i jest częścią inicjalizacji świata,
/// a nie dochodem z niczego. Bez tego test własnościowy „suma pieniądza = emisja −
/// destrukcja" pokazałby brak pokrycia dokładnie o kapitał gracza.
pub fn take_role(
    world: &mut World,
    market: Option<&Market>,
    variant: StartVariant,
    citizen: CitizenId,
    t: Tick,
) -> Option<PlayerCharacter> {
    let id = *world.get::<Identity>(citizen.entity())?;
    if !id.is_alive() {
        return None;
    }
    let hh_e = magnat_agents::household_by_index(world, id.household)?;

    // Znacznik postaci gracza siedzi w `Identity` od M3 (bit 2) i czeka tam na tę fazę.
    if let Some(x) = world.get_mut::<Identity>(citizen.entity()) {
        x.flags |= Identity::FLAG_PLAYER;
    }

    // Inwestor z zewnątrz: kapitał jest, sieci relacji brak.
    //
    // `ponytail:` zerujemy relacje **po stronie gracza**. Sufit nazwany: druga strona
    // dalej go pamięta, bo symetryczne czyszczenie wymaga przejścia po wszystkich
    // slabach relacji miasta. Dla zachowania, o które chodzi (planer dnia i wybory
    // gracza czytają **jego** relacje), różnicy nie ma.
    if variant.wipes_relations() {
        if let Some(r) = world.get_mut::<magnat_agents::RelationsRef>(citizen.entity()) {
            *r = magnat_agents::RelationsRef::default();
        }
    }

    let kapital = variant.capital();
    if let Some(m) = market {
        let rest = m.rest_of_world();
        let memo = TxMemo::new(TxKind::Endowment, magnat_core::DecisionReason::Unspecified);
        let ok = world
            .get_resource_mut::<magnat_economy::Books>()
            .is_some_and(|b| b.household_receive(rest, kapital, memo, t).is_ok());
        if ok {
            if let Some(h) = world.get_mut::<Household>(hh_e) {
                h.bank = Money(h.bank.get().saturating_add(kapital.get()));
            }
        }
    }

    // Spadkobierca małej firmy dostaje pierwszy zakład miasta w kolejności klucza.
    // Kolejność jest deterministyczna, bo `Market::sites` idzie po `BTreeMap`.
    let owned_sites = if variant == StartVariant::Heir {
        market
            .map(|m| m.sites())
            .and_then(|v| v.first().copied())
            .into_iter()
            .collect()
    } else {
        Vec::new()
    };

    // Spadkobierca dziedziczy **firmę**, a nie wskaźnik na zakład. Bez przepisania
    // własności w rejestrze `precheck` odmawiałby mu każdej komendy dotyczącej
    // własnego sklepu — „to nie jest twój zakład" — a gracz miałby rację, czując
    // się oszukanym. Wykryte testem WP10 (`DI-1`).
    for site in &owned_sites {
        let firma = world
            .get_resource::<magnat_firms::Firms>()
            .and_then(|f| f.site(*site))
            .map(|z| z.firm);
        if let (Some(key), Some(firms)) = (
            firma,
            world.get_resource_mut::<magnat_firms::Firms>(),
        ) {
            if let Some(f) = firms.get_mut(key) {
                f.owners.clear();
                f.owners.push(magnat_firms::OwnerShare {
                    owner: magnat_firms::Owner::Player,
                    bp: magnat_firms::Firm::SHARES_TOTAL,
                });
            }
        }
    }

    // Zakład gracza jest zakładem śledzonym: to jest ta flaga, którą ustawia `game/`,
    // a `sim/economy` ją tylko czyta (`Z-3` z M5e). Bez niej karta „dlaczego Anna
    // nie kupiła u mnie" nie ma z czego powstać.
    if let Some(m) = market {
        for s in &owned_sites {
            m.set_tracking(*s, magnat_economy::LostSaleTracking::Full);
        }
    }

    pin_micro(world, &[citizen]);

    Some(PlayerCharacter {
        citizen,
        household: HouseholdId(hh_e),
        variant,
        autonomy: PlayerAutonomy::default(),
        owned_sites,
        capital: kapital,
        heir: None,
        bankruptcies: 0,
    })
}

/// Przypina mieszkańców do warstwy Mikro (`LodPin`, §9 pkt 2 dokumentu fazy).
///
/// Sufit to `magnat_traffic::MAX_PINNED`; lista dłuższa jest przycinana, bo przypięcie
/// kosztuje symulację mikro poza kadrem i ma być decyzją, a nie nawykiem.
pub fn pin_micro(world: &World, citizens: &[CitizenId]) {
    let Some(z) = world
        .get_resource::<magnat_agents::AgentSources>()
        .and_then(magnat_agents::AgentSources::get)
    else {
        return;
    };
    let idx: Vec<u32> = citizens
        .iter()
        .take(magnat_traffic::MAX_PINNED)
        .map(|c| c.entity().index())
        .collect();
    z.travel.set_micro_pins(&idx);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warianty_startu_wybieraja_rozne_osoby() {
        // Dwudziestodwulatek bez pracy jest absolwentem i nie jest doświadczonym
        // pracownikiem — to jest cała treść predykatu i to ona ma się nie rozjechać.
        assert!(StartVariant::Graduate.accepts(22, false, Money::ZERO));
        assert!(!StartVariant::Graduate.accepts(22, true, Money::ZERO));
        assert!(!StartVariant::Worker.accepts(22, true, Money(900_000)));
        assert!(StartVariant::Worker.accepts(40, true, Money(900_000)));
        assert!(!StartVariant::Worker.accepts(40, true, Money(1000)));
        assert!(StartVariant::Sandbox.accepts(18, false, Money::ZERO));
        assert!(!StartVariant::Sandbox.accepts(17, false, Money::ZERO));
    }

    #[test]
    fn kazdy_wariant_ma_wlasny_klucz_i_kapital() {
        let mut klucze: Vec<&str> = StartVariant::ALL.iter().map(|v| v.key()).collect();
        klucze.sort_unstable();
        let ile = klucze.len();
        klucze.dedup();
        assert_eq!(klucze.len(), ile, "dwa warianty o tym samym kluczu");
        for v in StartVariant::ALL {
            assert!(v.capital().get() > 0, "{v:?} startuje bez grosza");
        }
    }

    #[test]
    fn losowanie_kandydata_jest_funkcja_ziarna() {
        let c = |i: u32| Candidate {
            citizen: CitizenId(magnat_core::Entity::new(i, std::num::NonZeroU32::MIN)),
            name: String::new(),
            age_years: 30,
            employed: true,
            savings: Money::ZERO,
            household_size: 1,
            district: 0,
        };
        let lista = vec![c(1), c(2), c(3)];
        assert_eq!(pick_random(&lista, 42), pick_random(&lista, 42));
        assert!(pick_random(&[], 42).is_none());
    }

    #[test]
    fn autonomia_domyslnie_zostawia_czas_wolny_autopilotowi() {
        let a = PlayerAutonomy::default();
        assert_eq!(a.get(AutonomyField::Leisure), Control::Auto);
        assert_eq!(a.get(AutonomyField::Job), Control::Manual);
        let mut b = a;
        b.set(AutonomyField::Leisure, Control::Manual);
        assert_eq!(b.get(AutonomyField::Leisure), Control::Manual);
    }
}
