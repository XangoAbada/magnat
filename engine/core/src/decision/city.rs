//! Powody decyzji miasta jako aktora (`K-58`, `R2-WP20`).
//!
//! Miasto nalicza daniny, wydaje pieniądz publiczny, prowadzi usługi, urzędy
//! i wybory oraz odcina prąd, kiedy sieć nie domyka bilansu.
//!
//! **Numery są wieczne i nie zmieniły się przy podziale** — wchodzą do hasha stanu
//! i do kronik, więc przenumerowanie przepisałoby cudzą historię. Trzy enumy dzielą
//! jedną przestrzeń numerów, a nie każdy własną; pilnuje tego test
//! `dyskryminanty_sa_wieczne`.
//!
//! **Numer stoi w [`discriminant`](CityReason::discriminant), a nie przy wariancie**,
//! i to jest cena podziału zapłacona świadomie. Jawna dyskryminanta przy wariancie
//! wymaga `#[repr(u16)]`, a `#[repr(u16)]` **wyłącza optymalizację niszy** — suma
//! `DecisionReason` urosła przez to z 24 B na 32 B, czyli o jedną trzecią na każdy
//! powód zapisany w dzienniku przecen, w kronice i w pierścieniu decyzji. Numery
//! nie zniknęły: przeniosły się o czterdzieści linii niżej, do jednego miejsca,
//! które ma test strażniczy — a przedtem były w **dwóch**, bo `discriminant()`
//! i tak wypisywał je po raz drugi.
//!
//! Reguła `K-12` obowiązuje tutaj **osobno**: bez `#[non_exhaustive]`, bez ramienia
//! `_` w renderze. To jest cała treść podziału — dopisanie powodu przez fazę
//! dotykającą firm nie powiększa pliku, który obsługuje mieszkańców.

use super::*;

/// Powód decyzji miasta jako aktora.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum CityReason {
    // 520–599 zarezerwowane dla M7.

    // ── M8: miasto jako aktor (600–699) ──────────────────────────────────────────
    /// Miasto naliczyło daninę (M8a WP2, PRD §6.8).
    ///
    /// Powód niesie **stawkę użytą w chwili naliczenia**, a nie aktualną: stawka
    /// zmienia się uchwałą i nigdy wstecz (`effective_from`), więc karta inspekcji
    /// pokazana pół roku później ma tłumaczyć kwotę, która wtedy powstała. Bez tego
    /// pola „dlaczego tyle" nie da się odpowiedzieć inaczej niż przeliczeniem, które
    /// da inny wynik.
    TaxAssessed {
        kind: TaxKind,
        rate_bp: u16,
        amount: Money,
    },
    /// Należność zapłacona — pieniądz przeszedł od płatnika do budżetu miasta.
    TaxSettled { kind: TaxKind, amount: Money },
    /// Termin minął, a należność stoi. `days` liczy doby od terminu płatności,
    /// `amount` to kwota główna bez odsetek — odsetki rosną co dobę i mają własny
    /// wiersz w rejestrze, więc powtarzanie ich tutaj dałoby dwie prawdy o jednej
    /// liczbie.
    TaxOverdue {
        kind: TaxKind,
        days: u16,
        amount: Money,
    },
    /// Należność umorzona: przestała być wymagalna, choć nikt jej nie zapłacił.
    /// To jest czwarty stan cyklu życia i wchodzi do domknięcia `Assessed =
    /// Settled + Overdue + Abated` (test T1) — pominięcie go znaczyłoby, że
    /// upadłość firmy gubi budżetowi pieniądze bez śladu.
    TaxAbated {
        kind: TaxKind,
        why: AbateReason,
        amount: Money,
    },
    /// Miasto wydało pieniądze (M8a WP1).
    PublicSpend {
        category: SpendCategory,
        amount: Money,
    },
    /// Miasto wyemitowało obligację, bo deficytu nie dało się zamknąć cięciem.
    MunicipalBondIssued { coupon_bp: u16, principal: Money },
    /// Domknięcie deficytu cięciem wydatków: `gap` to luka, `cut_bp` — o ile
    /// promili przycięto plan wydatków bieżących.
    BudgetDeficitClosed { gap: Money, cut_bp: u16 },
    /// Zrzut obciążenia w sieci przesyłowej (M8b §5.4 krok 3): wyspa nie domykała
    /// bilansu, więc odbiorcy od najniższego priorytetu poszli w ciemność.
    ///
    /// `priority` to **ostatni odłączony** próg, a nie każdy po kolei: gracz pyta
    /// „dokąd sięgnęło", a nie „ilu było". `shortfall_w` niesie moc, której
    /// zabrakło **przed** zrzutem — po zrzucie jest z definicji zero, więc powód
    /// zapisany po fakcie mówiłby, że nic się nie stało.
    LoadShed {
        service: UtilityService,
        priority: u8,
        shortfall_w: u32,
    },
    /// Zabezpieczenie krawędzi zadziałało: przepływ przekroczył przepustowość
    /// i linia wypadła z sieci (M8b §5.4 krok 5). To jest wejście do kaskady —
    /// wypadnięcie linii zmienia topologię, a zmiana topologii jest następną rundą.
    ///
    /// `repair_minutes` jest **wylosowanym** czasem brygady (`StreamId::GridFault`),
    /// a nie stałą: to jedyna rzecz, którą sieć w tej fazie losuje.
    GridTripped {
        service: UtilityService,
        repair_minutes: u16,
    },
    /// Zdarzenie świata zaczęło się (M8c §5.5, PRD §11.1).
    ///
    /// Powodem **nie jest** losowanie: rzut rozstrzygnął tylko „czy dziś", a szansę
    /// wyliczyły sondy stanu świata. Dlatego karta zdarzenia pokazuje obok tego powodu
    /// rozbicie hazardu na czynniki (`HazardFactor`) — „awaria, bo blok ma 32 lata
    /// i 90 dób zaległej konserwacji", a nie „awaria, bo wypadła szóstka".
    ///
    /// `severity_bps` jest siłą **wylosowaną w widełkach definicji** i to ona skaluje
    /// każdy efekt zdarzenia: to samo zdarzenie o sile 2000 i 9000 bps zmienia parametr
    /// inaczej, bo susza bywa dokuczliwa i bywa katastrofą.
    EventStarted {
        event: EventId,
        category: EventCategory,
        severity_bps: u16,
    },
    /// Zdarzenie świata się skończyło (M8c §5.5).
    ///
    /// `days` to długość, która faktycznie wyszła, a nie ta zapowiedziana: zdarzenie
    /// `UntilResolved` kończy się wtedy, gdy stan świata przestaje je podtrzymywać,
    /// więc „ile trwało" jest wynikiem symulacji, nie parametrem definicji.
    EventEnded {
        event: EventId,
        category: EventCategory,
        days: u16,
    },
    /// Jakość placówki publicznej po miesięcznym przeliczeniu (M8d WP7, PRD §10.3).
    ///
    /// Trzy liczby obok wyniku, bo „szkoła ma 41 punktów" nie jest odpowiedzią na
    /// pytanie gracza „dlaczego moje dziecko nie umie czytać". `funding_bp` mówi,
    /// ile miasto daje na ucznia wobec normy, `staff_bp` — jaka część etatów jest
    /// obsadzona, `load_bp` — ilu uczniów przypada na miejsce. Placówka niedofinansowana
    /// i placówka przepełniona schodzą do tej samej jakości z dwóch różnych powodów,
    /// a naprawia się je dwoma różnymi decyzjami.
    ServiceQuality {
        kind: ServiceKind,
        district: DistrictId,
        quality: Q,
        funding_bp: u16,
        staff_bp: u16,
        load_bp: u16,
    },
    /// Urząd wydał pozwolenie (M8d WP7, M8e §5.2).
    ///
    /// `waited_days` jest **wynikiem**, a nie parametrem: czas oczekiwania bierze się
    /// z obsady urzędu, długości kolejki i dni wolnych (`K-15`). To jest cała treść
    /// tego powodu — pozwolenie wydane w trzy doby i w sześćdziesiąt jest tą samą
    /// decyzją urzędu i różni się wyłącznie tym, ile kosztowało czasu.
    PermitIssued { kind: PermitKind, waited_days: u16 },
    /// Urząd otworzył sprawę przeciwko zakładowi (M8d WP8, PRD §10.4).
    ///
    /// `evidence` to materiał dowodowy w chwili otwarcia, nie w chwili rozstrzygnięcia:
    /// sprawa rośnie w czasie i to jest jej istota. Otwarcie sprawy samo w sobie nie
    /// jest karą i nie musi się nią skończyć.
    CaseOpened { agency: AgencyKind, evidence: Q },
    /// Urząd nałożył środek zaradczy (M8d WP8).
    ///
    /// `amount` jest kwotą tam, gdzie środek ma kwotę (grzywna, domiar), i zerem tam,
    /// gdzie jej nie ma (zamknięcie, cofnięcie koncesji, przymusowy podział) — bo
    /// wtedy dolegliwością jest czas albo majątek, a nie pieniądz, i udawanie kwoty
    /// zafałszowałoby histogram kar.
    RemedyImposed {
        agency: AgencyKind,
        remedy: RemedyKind,
        amount: Money,
    },
    /// Zakład zmienił udział obrotu poza deklaracją (M8d WP8, PRD §10.4).
    ///
    /// Szara strefa nie jest cechą charakteru, tylko **odpowiedzią na przyciśnięcie**:
    /// zakład pod kreską ukrywa więcej, zakład z marżą wraca do deklarowania. Dlatego
    /// powód niesie obie liczby — nowy udział i wynik miesiąca, który go wywołał.
    ShadowShareSet { share_bp: u16, last_result: Money },
    /// Rada uchwaliła regulację (M8e WP9, PRD §10.1).
    ///
    /// `for_bp` to poparcie w radzie w punktach bazowych, a `delay_days` — vacatio
    /// legis, czyli ile dób minie od uchwalenia do wejścia w życie. Obie liczby są
    /// treścią, a nie ozdobą: uchwała przegłosowana 5100 do 4900 i uchwała
    /// jednomyślna to dwie różne sytuacje polityczne, a regulacja wchodząca
    /// jutro i za kwartał to dwie różne sytuacje gospodarcze.
    PolicyEnacted {
        kind: PolicyKind,
        for_bp: u16,
        delay_days: u16,
    },
    /// Burmistrz ruszył stawkę daniny (M8e WP9, PRD §10.1, §6.8).
    ///
    /// Osobny powód od [`DecisionReason::PolicyEnacted`], mimo że stawka jest
    /// uchwałą jak każda inna, bo niesie **kierunek i odchylenie od celu**, czyli
    /// to, czego pilnuje test T5. `gap_bp` jest odchyleniem salda budżetu od celu
    /// w chwili decyzji — z dodatnim znakiem, gdy miasto ma nadwyżkę.
    TaxRateChanged {
        kind: TaxKind,
        from_bp: u16,
        to_bp: u16,
        gap_bp: i16,
    },
    /// Miasto ogłosiło przetarg (M8e WP9, PRD §10.3).
    ///
    /// `subject_id` niesie dzielnicę albo linię — słownik jest płaski, a identyfikator
    /// idzie osobnym polem, ta sama korekta co przy `RemedyKind` (`K-64`).
    TenderPublished {
        subject: TenderKind,
        subject_id: u16,
        budget: Money,
    },
    /// Przetarg rozstrzygnięty (M8e WP9).
    ///
    /// `score_bp` jest punktacją zwycięzcy, a `runner_up_bp` — drugiego w kolejności.
    /// Przetarg wygrany o włos i wygrany bezkonkurencyjnie to dwie różne odpowiedzi
    /// na pytanie „dlaczego nie ja", a to jest pytanie, które gracz zada (PRD §14.1).
    /// `bids` równe zero znaczy przetarg nierozstrzygnięty — miasto robi wtedy usługę
    /// samo i płaci za nią plan, a nie ofertę.
    TenderAwarded {
        subject: TenderKind,
        price: Money,
        score_bp: u16,
        runner_up_bp: u16,
        bids: u8,
    },
    /// Wybory rozstrzygnięte (M8e WP10, PRD §10.2).
    ///
    /// `turnout_bp` to frekwencja, `winner_bp` — wynik zwycięzcy, `incumbent` mówi,
    /// czy wygrał urzędujący burmistrz. Trzecia liczba jest tu dlatego, że cały
    /// mechanizm z §1 dokumentu fazy („spadek poparcia → przegrana w tym obwodzie")
    /// jest nieczytelny bez odpowiedzi, czy władza się w ogóle zmieniła.
    ElectionHeld {
        turnout_bp: u16,
        winner_bp: u16,
        incumbent: bool,
    },
}

impl CityReason {
    /// Dyskryminanta w **globalnej** przestrzeni numerów `DecisionReason`.
    ///
    /// Jawny `match`, a nie `self as u16`: numery nie zaczynają się od zera i nie są
    /// ciągłe, bo należą do bloków faz (`K-12`). Dopisanie wariantu bez wpisu tutaj
    /// łamie kompilację — ten sam mechanizm, który wymusza ramię w karcie inspekcji.
    #[inline]
    #[must_use]
    pub const fn discriminant(self) -> u16 {
        match self {
            CityReason::TaxAssessed { .. } => 600,
            CityReason::TaxSettled { .. } => 601,
            CityReason::TaxOverdue { .. } => 602,
            CityReason::TaxAbated { .. } => 603,
            CityReason::PublicSpend { .. } => 604,
            CityReason::MunicipalBondIssued { .. } => 605,
            CityReason::BudgetDeficitClosed { .. } => 606,
            CityReason::LoadShed { .. } => 607,
            CityReason::GridTripped { .. } => 608,
            CityReason::EventStarted { .. } => 609,
            CityReason::EventEnded { .. } => 610,
            CityReason::ServiceQuality { .. } => 611,
            CityReason::PermitIssued { .. } => 612,
            CityReason::CaseOpened { .. } => 613,
            CityReason::RemedyImposed { .. } => 614,
            CityReason::ShadowShareSet { .. } => 615,
            CityReason::PolicyEnacted { .. } => 616,
            CityReason::TaxRateChanged { .. } => 617,
            CityReason::TenderPublished { .. } => 618,
            CityReason::TenderAwarded { .. } => 619,
            CityReason::ElectionHeld { .. } => 620,
        }
    }
}

impl From<CityReason> for DecisionReason {
    fn from(r: CityReason) -> DecisionReason {
        DecisionReason::City(r)
    }
}
