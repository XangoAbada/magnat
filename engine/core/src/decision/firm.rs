//! Powody decyzji firmy i jej zakładów (`K-58`, `R2-WP20`).
//!
//! Firma decyduje o cenie, zapasie, zatrudnieniu, produkcji i o tym, czy
//! w ogóle istnieje. Numer wariantu pochodzi z fazy, która go wniosła,
//! a aktor — z tego, o kim ten wariant mówi: `Repricing` ma numer z bloku
//! detalu M5, a mimo to jest decyzją sprzedawcy, nie kupującego.
//!
//! **Numery są wieczne i nie zmieniły się przy podziale** — wchodzą do hasha stanu
//! i do kronik, więc przenumerowanie przepisałoby cudzą historię. Trzy enumy dzielą
//! jedną przestrzeń numerów, a nie każdy własną; pilnuje tego test
//! `dyskryminanty_sa_wieczne`.
//!
//! **Numer stoi w [`discriminant`](FirmReason::discriminant), a nie przy wariancie**,
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

/// Powód decyzji firmy i jej zakładów.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum FirmReason {
    /// Sklep zmienił cenę oferty (M5c §5.6). `driver` mówi, który człon korekty
    /// przeważył, `delta_bp` — o ile zmieniła się cena względem poprzedniej.
    /// To jest odpowiedź na pytanie gracza „czemu u konkurenta potaniało".
    Repricing {
        site: SiteId,
        good: GoodId,
        driver: PriceDriver,
        delta_bp: i16,
    },
    // 307–399 zarezerwowane dla M5.

    // ── M6: 400..=499 ────────────────────────────────────────────────────────────
    /// Zakład wszedł na kolejny stopień kaskady niedoboru (M6b §5.7, PRD §8.4).
    /// `coverage_minutes` to pokrycie zapasu w minutach w chwili przejścia — liczba,
    /// którą gracz widzi w panelu łańcucha jako „zostało ci 3 h mąki".
    /// Zapisywane na **każdym** przejściu, także w dół: powrót do `Ok` też jest
    /// odpowiedzią na pytanie „co się stało z moją piekarnią".
    Shortage {
        good: GoodId,
        from: ShortageStageKind,
        to: ShortageStageKind,
        coverage_minutes: u32,
    },
    /// Linia stanęła (M6b §5.5). `line` to indeks linii w zakładzie, nie `Entity` —
    /// linia nie jest encją ECS, a karta inspekcji i tak pokazuje ją jako „linia 2".
    ProductionHalted {
        site: SiteId,
        line: u16,
        cause: LineStopCause,
    },
    /// Zakład użył substytutu zamiast brakującego wejścia (M6b §5.7, PRD §8.4).
    /// `quality_loss` to spadek jakości wyjścia w punktach skali `Q` — cena substytucji,
    /// przez którą stoi ona **przedostatnia** w kaskadzie, tuż przed postojem.
    SubstituteUsed {
        good: GoodId,
        alt: GoodId,
        quality_loss: u8,
    },
    /// Rozstrzygnięcie zapytania ofertowego na rynku spot (M6c §5.8).
    /// `saving_bp` to przewaga zwycięzcy nad drugą ofertą w punktach bazowych funkcji
    /// celu — zero znaczy „jedyna oferta", a nie „remis". Gracz pytający „dlaczego
    /// kupiłeś u nich" dostaje odpowiedź w postaci, w której da się ją sprawdzić:
    /// ilu było chętnych i o ile ten był lepszy.
    SupplierChosen {
        good: GoodId,
        seller: FirmId,
        quotes: u16,
        saving_bp: u16,
    },
    /// Podpisanie kontraktu terminowego (M6c §5.8). `months` to okres obowiązywania
    /// w miesiącach 30-dniowych (`K-1`), `indexed` odróżnia cenę stałą od takiej,
    /// która chodzi za indeksem — bo to jest różnica, o którą gracz pyta najpierw.
    ContractSigned {
        good: GoodId,
        seller: FirmId,
        months: u16,
        indexed: bool,
    },
    /// Producent wybrał eksport zamiast sprzedaży lokalnej (M6c §5.9).
    /// `premium_bp` to przewaga ceny eksportowej **po odjęciu transportu do węzła**
    /// nad najlepszą ceną lokalną. Drenaż podaży jest emergentny, więc powód musi
    /// nieść liczbę, z której wynikł — inaczej wzrost cen w mieście wygląda na błąd.
    ExportChosen {
        good: GoodId,
        premium_bp: u16,
        mass_kg: u32,
    },
    // 406–499 zarezerwowane dla M6.

    // ── M7: 500..=599 ────────────────────────────────────────────────────────────
    /// Firma wybrała kandydata (M7b §5.5, PRD §6.6). `score` to wynik scoringu
    /// zatrudnionego, `runner_up` — drugiego w kolejce; `i32::MIN` znaczy „nie było
    /// drugiego", a nie „drugi był fatalny". Dwie liczby zamiast jednej, bo pytanie
    /// gracza brzmi „dlaczego **on**", a nie „czy był dobry": różnica między pierwszym
    /// a drugim jest całą odpowiedzią i bez niej powód byłby oceną bez skali.
    Hired {
        role: JobRoleId,
        score: i32,
        runner_up: i32,
    },
    /// Firma ruszyła stawkę w wiszącej ofercie (M7b §5.5, PRD §6.6).
    /// `delta_bp` to przyrost wobec stawki poprzedniej w punktach bazowych,
    /// `days_open` — ile dni oferta wisiała bez akceptowalnego kandydata.
    ///
    /// **`delta_bp == 0` przy `cause: Ceiling` jest wpisem pełnoprawnym**: znaczy
    /// „dalej nie licytuję, bo przy wyższej stawce ten etat przestaje się opłacać".
    /// Nieobsadzony wakat jest poprawnym wynikiem (M7 §7.1 pkt 4) i musi mieć zdanie,
    /// którym da się go graczowi wytłumaczyć.
    WageRaise {
        role: JobRoleId,
        delta_bp: u16,
        days_open: u16,
        cause: WageCause,
    },
    /// Pracownik przestał pracować w tym zakładzie (M7b WP6). Powód jest zapisywany
    /// **po stronie odchodzącego** — także wtedy, gdy odejście jest zwolnieniem.
    /// `tenure_days` to staż w dobach: rotacja tygodniowa i rotacja po pięciu latach
    /// to dwie różne diagnozy tego samego zdarzenia.
    JobLeft {
        role: JobRoleId,
        cause: LeaveCause,
        tenure_days: u16,
    },
    /// Reguła polityki wyzwoliła się i firma wykonała jej akcję (M7c WP6b, `K-11`).
    ///
    /// **Ten sam wariant dla gracza i dla AI** — to jest cała treść `sim/policy`:
    /// reguła z edytora M9 i reguła wygenerowana przez tier taktyczny M7e wykonują się
    /// tym samym kodem i zapisują ten sam powód. Gdyby były dwa warianty, różnica
    /// wróciłaby tylnymi drzwiami w karcie inspekcji.
    ///
    /// `rule` to indeks reguły w polityce (0..=7 — twardy limit ośmiu reguł z M9d §5.6),
    /// `action` — rodzaj wykonanej akcji. Pełnych wejść reguły tu nie ma i być nie może:
    /// `Metric` mieszka w `sim/policy`, a `core` od niego nie zależy. Odtwarza je na
    /// żądanie `magnat_policy::inputs` przy otwarciu karty — dlatego ta funkcja istnieje.
    ///
    /// **Dwa pola dokłada M9d WP9 i one wejść nie zastępują.** `lag_days` to wiek obrazu
    /// konkurencji, na którym menedżer pracował, `deviation_bp` — o ile punktów bazowych
    /// spudłował wobec wartości, którą reguła wyliczyła. Jedno i drugie da się poznać
    /// **wyłącznie w chwili wykonania**: dzień później obraz konkurencji jest już inny,
    /// a rzut menedżera nie zostawia po sobie śladu nigdzie indziej. Bez nich zdanie
    /// „cel 6,38 zł, menedżer ustawił 6,44 zł" z §5.6 nie miałoby z czego powstać.
    /// Zero w obu znaczy „wykonano dokładnie i na świeżych danych".
    PolicyApplied {
        policy: PolicyId,
        rule: u8,
        action: ActionKind,
        lag_days: u8,
        deviation_bp: i16,
    },
    /// Zakład dostał menedżera albo go stracił (M7c WP7, PRD §7.5).
    ///
    /// `prev` to jakość zarządzania **sprzed** zmiany, `skill_mgmt` — umiejętność
    /// przychodzącego menedżera. Odejście menedżera zapisuje się tym samym wariantem
    /// z `skill_mgmt` zastępstwa, bo dla gracza „przyszedł nowy" i „został po nim
    /// zastępca" to odpowiedź na to samo pytanie: dlaczego zakład nagle produkuje inaczej.
    ManagerAssigned {
        site: SiteId,
        skill_mgmt: Q,
        prev: u8,
    },
    /// Firma uruchomiła instrument dłużny (M7d WP8, PRD §7.8).
    ///
    /// Jeden wariant na kredyt obrotowy i inwestycyjny, bo `LoanKind` już je rozróżnia
    /// — drugi wariant powielałby słownik, który po to powstał. `rate_bp` to stopa
    /// roczna z decyzji banku, `term_months` — długość harmonogramu; razem odpowiadają
    /// na pytanie „ile mnie to kosztuje i jak długo", czyli na to, które gracz zadaje.
    LoanTaken {
        kind: LoanKind,
        rate_bp: i16,
        term_months: u16,
    },
    /// Firma podpisała leasing (M7d WP8). `months` to okres do wykupu.
    ///
    /// Osobno od [`DecisionReason::LoanTaken`], bo leasing **nie tworzy pieniądza**
    /// i nie daje aktywa: rzecz należy do leasingodawcy aż do wykupu i w upadłości
    /// do masy nie wchodzi. To jest różnica, którą karta inspekcji ma pokazać, zanim
    /// gracz policzy na nią majątek firmy.
    LeaseSigned { site: SiteId, months: u16 },
    /// Firma sprzedała należności z dyskontem (M7d WP8, faktoring).
    ///
    /// `count` to liczba sprzedanych pozycji, `discount_bp` — marża faktora.
    /// Dla obserwatora jest to typowy sygnał kłopotów z płynnością i dlatego ma
    /// własny wariant: „wzięli kredyt" i „sprzedali należności" to dwie różne
    /// diagnozy tej samej firmy.
    ReceivablesFactored { count: u16, discount_bp: u16 },
    /// Firma wypuściła obligacje (M7d WP8). `coupon_bp` to kupon roczny,
    /// `months` — czas do wykupu. Nabywcami są mieszkańcy z oszczędnościami
    /// i inne firmy; rynek wtórny należy do M10.
    BondIssued { coupon_bp: u16, months: u16 },
    /// Otwarto postępowanie upadłościowe (M7d WP9, `K-10`).
    ///
    /// `days` ma znaczenie tylko przy `BankruptcyTrigger::Illiquid` (ile dób firma
    /// nie płaciła wymagalnych zobowiązań) i przy pozostałych wyzwalaczach jest zerem
    /// — liczba stoi obok słownika, a nie w nim, żeby histogram przyczyn upadłości
    /// liczył przyczyny, a nie pary (przyczyna, długość).
    BankruptcyOpened {
        trigger: BankruptcyTrigger,
        days: u16,
    },
    /// Syndyk zaspokoił roszczenia jednego priorytetu (M7d WP9).
    ///
    /// `ratio_bp` to stopień zaspokojenia w punktach bazowych — 10 000 znaczy
    /// „w całości", 0 „nie starczyło na nic". To jest liczba, której szuka
    /// i pracownik, i bank, i dostawca, więc powód niesie ją zamiast kwoty:
    /// kwota jest indywidualna, stopień zaspokojenia dotyczy całego priorytetu.
    ClaimSettled {
        priority: ClaimPriority,
        ratio_bp: u16,
    },
    /// Tier operacyjny ustawił cel marży na towarze (M7e WP11, PRD §12.3).
    ///
    /// Nie mylić z `Repricing` (M5c): tamten mówi, **która korekta przeważyła**
    /// przy składaniu dzisiejszej ceny, ten — że firma zmieniła cel, wokół którego
    /// cena się składa. Pierwsze zdarza się codziennie, drugie raz na kilka tygodni,
    /// i gracz pyta o nie osobno („czemu dziś taniej" vs. „czemu on zszedł z marży").
    MarginTargetSet {
        good: GoodId,
        margin_bp: i32,
        prev_bp: i32,
    },
    /// Tier operacyjny ustawił cel zapasu na towarze, w dobach sprzedaży (M7e WP11).
    ///
    /// `days` to pokrycie, do którego firma chce zamawiać; `prev` — poprzednie.
    /// Zapas jest drugą dźwignią tieru operacyjnego obok ceny i musi mieć własny
    /// powód, bo „stoi pusty" i „stoi pełny" to dwa różne błędy tej samej firmy.
    RestockTargetSet { good: GoodId, days: u16, prev: u16 },
    /// Tier taktyczny zamknął zakład (M7e WP12, PRD §12.3).
    ///
    /// `months` to długość nieprzerwanej straty, `margin_bp` — marża ostatniego
    /// miesiąca (ujemna). Dwie liczby zamiast jednej z tego samego powodu co przy
    /// `Hired`: pytanie gracza brzmi „dlaczego **ten**", a odpowiedź „bo od trzech
    /// miesięcy traci 8%" niesie i skalę, i czas.
    SiteClosed { months: u8, margin_bp: i32 },
    /// Tier taktyczny zmienił kurs firmy i przypiął do niego preset polityki
    /// (M7e WP12, PRD §12.1).
    ///
    /// **To jest źródło polityk firm AI.** Do M7e menedżer dostawał delegację
    /// z polityką pustą, bo zestaw reguł miał generować tier taktyczny, a tieru
    /// nie było (`AZ-1`). Reguła gracza i reguła stąd wykonują się tym samym
    /// ewaluatorem — różnica jest w tym, kto ją napisał (`K-11`).
    StrategySet {
        strategy: FirmStrategy,
        prev: FirmStrategy,
    },
    /// Firma odpowiedziała na utratę udziału w rynku (M7e WP14, PRD §12.2).
    ///
    /// `target` to **zakład** rywala, nie jego firma — także wtedy, gdy rywalem jest
    /// gracz. Zakład, bo to on jest widoczny z ulicy i to jego cena stoi w tablicy
    /// publicznej; firmę znajdzie się z niego jednym odczytem rejestru, a w drugą
    /// stronę nie da się wcale (`K-46`: `FirmId` sklepu pochodzi z generatora miasta,
    /// `FirmKey` z rejestru firm, i nie są tą samą liczbą).
    ///
    /// `depth_bp` znaczy co innego w każdym wariancie `kind` i to jest zamierzone:
    /// przy wojnie cenowej to zejście z ceny, przy wyłączności premia dla dostawcy,
    /// przy przeciąganiu ludzi — nadpłata ponad stawkę rywala. Jedna liczba, bo we
    /// wszystkich trzech odpowiada na to samo pytanie gracza: „ile go to kosztuje".
    CompetitiveResponse {
        kind: ReactionKind,
        target: SiteId,
        depth_bp: u16,
    },
    /// Tier strategiczny otwiera zakład (M7f WP13, PRD §12.3).
    ///
    /// **Nie ma tu kwoty i nie będzie.** Decyzja stoi na porównaniu wariantów
    /// w modelu makro, a ten deklaruje własny błąd 3–12 % — kwota z niego byłaby
    /// fałszywą precyzją, w którą gracz uwierzy i na której zbuduje plan (`R15`).
    /// Dlatego powód niesie **to, z czego konkurent wybierał**: ile wariantów
    /// porównał, jak szeroki był margines i w którą stronę szedł wynik. Gracz widzi,
    /// że rywal wybrał A nad B i o ile pewnie, a nie że „wyliczył 240 tys.".
    SiteOpened {
        district: DistrictId,
        variants: u8,
        margin_bp: u16,
        trend: Trend,
    },
    /// Właściciel zamyka firmę dobrowolnie (M7f WP15, PRD §12.4).
    ///
    /// Osobny powód od `BankruptcyOpened`: to nie jest upadłość, tylko wyjście
    /// przed nią. Firma wyprzedaje majątek bez syndyka, spłaca zobowiązania i wraca
    /// na rynek pracy — tańsze dla niej i dla symulacji. `months` mówi, jak długo
    /// trwała strata, `cash` — ile zostało w kasie, kiedy właściciel się poddał.
    VoluntaryClosure { months: u8, cash: Money },
    /// Mieszkaniec zakłada firmę (M7f WP15, PRD §5.6).
    ///
    /// `score` to wynik `founding_score` w setnych, `capital` — kapitał, który
    /// wniósł. Obie liczby są **z jego własnych oszczędności i zdolności kredytowej**,
    /// a nie z prognozy: nisza jest wykryta w okolicy, którą zna (§5.7), a nie
    /// przez wyrocznię globalną.
    FirmFounded { score: u16, capital: Money },
    /// Sieć zewnętrzna wchodzi do miasta (M7f WP15, PRD §12.4).
    ///
    /// `capital` jest **zarejestrowanym punktem emisji pieniądza** — przechodzi
    /// przez `Books::inject_external_capital`, inaczej globalny test zachowania
    /// pieniądza pękłby i nikt nie wiedziałby dlaczego (`D10`).
    ChainEntered { capital: Money, sites: u8 },
    /// Ktoś dołożył się do kampanii kandydata (M8e WP10, PRD §10.2).
    ///
    /// `illegal` rozstrzyga, czy to darowizna, czy łapówka — i to jedno pole niesie
    /// całe ryzyko: wsparcie nielegalne podnosi sondę hazardu skandalu, a ujawnienie
    /// uderza w kandydata **i** we wspierającego. Fundator jedzie osobno, bo jest
    /// podmiotem, a nie słownikiem (`K-62`).
    CampaignBacked {
        candidate: u8,
        amount: Money,
        illegal: bool,
    },
    /// Firma ruszyła kampanię reklamową (M10b WP10.6, PRD §7.6).
    ///
    /// `claim` to deklarowana jakość — liczba, którą kampania wpisuje odbiorcom
    /// w `expected_quality`. Obietnica ponad stan jest samokarząca i to pole
    /// jest jej zapisem.
    AdCampaignStarted {
        brand: BrandId,
        channel: AdChannelKind,
        budget: Money,
        claim: Q,
    },
    /// Redakcja opublikowała tekst o zdarzeniu (M10b WP10.7, PRD §7.2, §11.2).
    ///
    /// `outlet` jest marką tytułu, nie firmą: wiarygodność jest per-para
    /// (tytuł ↔ czytelnik) i mieszka w tym samym slocie, co marka sklepu.
    /// `reach_bp` to udział mieszkańców miasta, do których tekst dotarł.
    StoryPublished {
        outlet: BrandId,
        event: EventId,
        bias: EditorialBias,
        reach_bp: u16,
    },
    /// Firma wybrała węzeł drzewa technologii i zaczęła go badać (M10c WP10.8, PRD §7.7).
    ///
    /// `months_est` to prognoza z **bieżącego** tempa, nie obietnica: zwolnienie
    /// badaczy albo cięcie budżetu materiałowego wydłuża projekt, a przełom go skraca.
    /// Gracz ma z tego pola dowiedzieć się, czego firma się spodziewała w chwili
    /// decyzji — i porównać z tym, co wyszło w [`DecisionReason::TechDiscovered`].
    ResearchStarted {
        tech: TechId,
        cost_rp: u32,
        months_est: u16,
    },
    /// Firma odkryła technologię (M10c WP10.8, PRD §7.7, §11.3).
    ///
    /// `patented` rozstrzyga, czy odkrycie wyprzedziło rok „światowy": przed nim
    /// daje patent i wyłączność na dwadzieścia lat gry, po nim jest już wiedzą
    /// w obiegu i jedyną nagrodą jest obniżony koszt. To jest cała różnica między
    /// byciem pierwszym a byciem na czas.
    TechDiscovered {
        tech: TechId,
        patented: bool,
        rp_spent: u32,
        months: u16,
    },
    /// Firma kupiła licencję na cudzy patent (M10c WP10.8, PRD §7.7).
    ///
    /// Licencja **nie jest nowym mechanizmem umowy**: niesie `ContractId` z M6,
    /// tak samo jak franczyza w M10e. Tutaj są dwie liczby, których umowa dostawy
    /// nie ma czym wyrazić — stawka royalty i to, komu się ją płaci.
    LicenseSigned {
        tech: TechId,
        licensor: FirmId,
        royalty_bp: u16,
    },
    /// Technologia wypuściła na rynek towar, którego wcześniej nie było
    /// (M10c WP10.9, PRD §11.3).
    ///
    /// To jest powód, który widać w mieście: przed nim towaru nie było na żadnej
    /// półce i nie było go w koszyku żadnej potrzeby, po nim jest. `shops` mówi,
    /// ile sklepów wstawiło go na półkę tego samego dnia.
    ProductLaunched {
        good: GoodId,
        tech: TechId,
        shops: u16,
    },
    /// Firma weszła na giełdę (M10d WP10.10, PRD §6.5).
    ///
    /// `price` jest ceną odniesienia pierwszego fixingu, a nie kursem: kurs powstaje
    /// dopiero z transakcji (PRD §6.5 — „wycena z transakcji, nigdy z formuły").
    StockListed { firm: FirmId, price: Money },
    /// Dobowy fixing wyznaczył kurs (M10d WP10.10).
    ///
    /// Cena jest **za jeden punkt bazowy udziału**, więc pomnożona przez 10 000
    /// daje wycenę całej firmy. Wolumenu w ładunku nie ma (sufit 16 B, patrz
    /// `StakeDisclosed`) — notowanie trzyma go u siebie.
    StockFixing { firm: FirmId, price: Money },
    /// Ktoś przekroczył próg 5 % w akcjonariacie i musiał to ujawnić (M10d WP10.11).
    ///
    /// **Firmy w ładunku nie ma i to jest decyzja, nie przeoczenie.** Powód zapisuje
    /// się w dzienniku decyzji firmy, której akcjonariat się zmienił, więc firma
    /// jest kontekstem — ta sama droga, którą `LicenseSigned` z M10c nazywa
    /// licencjodawcę, a licencjobiorcę zostawia kontekstowi. Alternatywa (oba pola)
    /// to 20 B ładunku przy suficie 16 B z zasady 5 w nagłówku modułu, a `Subject`
    /// zjada z niego 12, bo pakiet trzyma i mieszkaniec, i firma.
    ///
    /// Wielkości pakietu też nie ma: próg jest treścią wariantu, a stan bieżący
    /// odtwarza się z `Firm.owners` przy otwarciu karty — tak samo jak `K-70`
    /// odtwarza wejścia reguły.
    StakeDisclosed { holder: Subject },
    /// Ktoś przekroczył 50 % i przejął kontrolę (M10d WP10.11, PRD §7.9).
    ///
    /// To jest druga — obok upadłości — droga wyjścia firmy z rynku (PRD §12.4):
    /// firma nie znika, tylko zaczyna działać cechami przejmującego.
    ControlAcquired { holder: Subject },
    /// Wypłacono dywidendę (M10d WP10.11, PRD §7.8).
    DividendPaid { firm: FirmId, total: Money },
    /// Emisja nowych udziałów (M10d WP10.11, PRD §7.8).
    ///
    /// `bp` jest wielkością emisji **po rozwodnieniu**: tyle udziału mają nowe
    /// akcje w firmie, która właśnie urosła. Firma jest kontekstem, jak przy
    /// [`DecisionReason::StakeDisclosed`].
    SharesIssued { bp: u16, price: Money },
    /// Zdarzenie wyrządziło szkodę majątkową (M10d WP10.12, PRD §11.2).
    ///
    /// Pierwszy powód w tej grze, który mówi „coś przepadło": do M10d zdarzenie
    /// zmieniało wyłącznie parametry, a nakładka przywracała je po wygaśnięciu.
    /// Szkoda jest nieodwracalna i dlatego **nie jest efektem** (`GD-4`).
    PerilStruck {
        peril: PerilKind,
        district: DistrictId,
        loss: Money,
    },
    /// Ubezpieczyciel wystawił polisę (M10d WP10.12, PRD §6.5).
    ///
    /// `rate_bp` jest stawką roczną od sumy ubezpieczenia i to ona niesie całą
    /// wiedzę zakładu o ryzyku — **nie ma jej w żadnym pliku danych**: wychodzi
    /// z historii szkód w dzielnicy zmieszanej z priorem miejskim.
    Underwritten {
        peril: PerilKind,
        rate_bp: u16,
        premium: Money,
    },
    /// Ubezpieczyciel wypłacił odszkodowanie (M10d WP10.12).
    ClaimPaid { insurer: FirmId, paid: Money },
    /// Kupujący wybrał droższą ofertę, bo jest od stałego dostawcy
    /// (M10e WP10.13, PRD §7.9).
    ///
    /// Powód powstaje **tylko wtedy, gdy rabat rozstrzygnął** — czyli gdy bez
    /// niego wygrałby ktoś inny. Zapisywanie go przy każdym zakupie od znajomego
    /// dostawcy zamieniłoby go w szum: gracz pyta „dlaczego przepłaciłem", a nie
    /// „od kogo kupiłem". `discount_bp` jest preferencją w funkcji celu, a nie
    /// obniżką ceny: firma **płaci pełną kwotę** i to jest treść tego wariantu.
    TrustedSupplier {
        supplier: FirmId,
        trust: Q,
        discount_bp: u16,
    },
    /// Firmy uzgodniły cenę minimalną towaru (M10e WP10.13, PRD §7.9).
    ///
    /// Zmowa jest **nielegalna od pierwszej minuty**, ale nikt o niej nie wie:
    /// do kroniki trafia dopiero wykrycie (§5.9), bo wpis w chwili zawiązania
    /// zdradzałby graczowi tajemnicę, której uczestnicy pilnują.
    CartelFormed {
        good: GoodId,
        members: u8,
        floor: Money,
    },
    /// Regulator wykrył zmowę (M10e WP10.13, PRD §7.9, `K-10` — sprawę prowadzi M8).
    ///
    /// `months` mówi, ile kartel przetrwał. To jest liczba, o którą gra się toczy:
    /// zmowa umiarkowana żyje latami, chciwa — kwartał.
    CartelDetected {
        good: GoodId,
        members: u8,
        months: u16,
    },
    /// Marka oberwała od skandalu — zmowy, strajku albo tekstu w prasie
    /// (M10e WP10.13/WP10.14, PRD §7.6).
    ///
    /// Osobny wariant od [`DecisionReason::BrandExperience`] z rozmysłu: tamten
    /// mówi „kupiłem i się rozczarowałem", ten — „usłyszałem i przestałem lubić".
    /// Dla gracza to dwa różne pytania i dwie różne naprawy.
    BrandScandal { brand: BrandId, drop: u8 },
    /// Załoga zakładu zawiązała związek zawodowy (M10e WP10.14, PRD §6.6, `K-9`).
    ///
    /// Zakład jest kontekstem wpisu (dziennik decyzji firmy), więc w ładunku są
    /// dwie liczby, które odpowiadają na „dlaczego akurat tu": gęstość poparcia
    /// i poziom żalu w chwili zawiązania.
    UnionFormed { density: Q, grievance: Q },
    /// Związek przedstawił żądanie płacowe (M10e WP10.14, PRD §6.6).
    ///
    /// `anchor` jest **płacą znaną załodze z grafu relacji i plotki**, a nie
    /// prawdziwą medianą miejską (§5.9): związek może żądać za dużo albo za mało,
    /// bo ma niepełną informację, i to jest realizm z systemu, nie z parametru.
    WageDemandMade { raise_bp: u16, anchor: Money },
    /// Negocjacje padły i zakład stanął (M10e WP10.14, PRD §6.6).
    ///
    /// `participation_bp` jest odsetkiem załogi, która przystąpiła do strajku —
    /// ta sama liczba jedzie jako siła zdarzenia `social/strike`, więc produkcja
    /// spada dokładnie o tyle, ilu ludzi wyszło.
    StrikeStarted { participation_bp: u16, round: u8 },
    /// Strajk się skończył (M10e WP10.14, PRD §6.6).
    ///
    /// `raise_bp` zero znaczy **kapitulację związku**: fundusz się wyczerpał
    /// i ludzie wrócili bez podwyżki. To jest jedyny wariant, w którym zero
    /// jest odpowiedzią, a nie brakiem pomiaru.
    StrikeEnded { days: u16, raise_bp: u16 },
}

impl FirmReason {
    /// Dyskryminanta w **globalnej** przestrzeni numerów `DecisionReason`.
    ///
    /// Jawny `match`, a nie `self as u16`: numery nie zaczynają się od zera i nie są
    /// ciągłe, bo należą do bloków faz (`K-12`). Dopisanie wariantu bez wpisu tutaj
    /// łamie kompilację — ten sam mechanizm, który wymusza ramię w karcie inspekcji.
    #[inline]
    #[must_use]
    pub const fn discriminant(self) -> u16 {
        match self {
            FirmReason::Repricing { .. } => 303,
            FirmReason::Shortage { .. } => 400,
            FirmReason::ProductionHalted { .. } => 401,
            FirmReason::SubstituteUsed { .. } => 402,
            FirmReason::SupplierChosen { .. } => 403,
            FirmReason::ContractSigned { .. } => 404,
            FirmReason::ExportChosen { .. } => 405,
            FirmReason::Hired { .. } => 500,
            FirmReason::WageRaise { .. } => 501,
            FirmReason::JobLeft { .. } => 502,
            FirmReason::PolicyApplied { .. } => 503,
            FirmReason::ManagerAssigned { .. } => 504,
            FirmReason::LoanTaken { .. } => 505,
            FirmReason::LeaseSigned { .. } => 506,
            FirmReason::ReceivablesFactored { .. } => 507,
            FirmReason::BondIssued { .. } => 508,
            FirmReason::BankruptcyOpened { .. } => 509,
            FirmReason::ClaimSettled { .. } => 510,
            FirmReason::MarginTargetSet { .. } => 511,
            FirmReason::RestockTargetSet { .. } => 512,
            FirmReason::SiteClosed { .. } => 513,
            FirmReason::StrategySet { .. } => 514,
            FirmReason::CompetitiveResponse { .. } => 515,
            FirmReason::SiteOpened { .. } => 516,
            FirmReason::VoluntaryClosure { .. } => 517,
            FirmReason::FirmFounded { .. } => 518,
            FirmReason::ChainEntered { .. } => 519,
            FirmReason::CampaignBacked { .. } => 622,
            FirmReason::AdCampaignStarted { .. } => 802,
            FirmReason::StoryPublished { .. } => 803,
            FirmReason::ResearchStarted { .. } => 804,
            FirmReason::TechDiscovered { .. } => 805,
            FirmReason::LicenseSigned { .. } => 806,
            FirmReason::ProductLaunched { .. } => 807,
            FirmReason::StockListed { .. } => 808,
            FirmReason::StockFixing { .. } => 809,
            FirmReason::StakeDisclosed { .. } => 810,
            FirmReason::ControlAcquired { .. } => 811,
            FirmReason::DividendPaid { .. } => 812,
            FirmReason::SharesIssued { .. } => 813,
            FirmReason::PerilStruck { .. } => 814,
            FirmReason::Underwritten { .. } => 815,
            FirmReason::ClaimPaid { .. } => 816,
            FirmReason::TrustedSupplier { .. } => 817,
            FirmReason::CartelFormed { .. } => 818,
            FirmReason::CartelDetected { .. } => 819,
            FirmReason::BrandScandal { .. } => 820,
            FirmReason::UnionFormed { .. } => 821,
            FirmReason::WageDemandMade { .. } => 822,
            FirmReason::StrikeStarted { .. } => 823,
            FirmReason::StrikeEnded { .. } => 824,
        }
    }
}

impl From<FirmReason> for DecisionReason {
    fn from(r: FirmReason) -> DecisionReason {
        DecisionReason::Firm(r)
    }
}
