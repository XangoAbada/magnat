# M7d — Finanse i upadłość

Podfaza 4 z 6 fazy **M7 — Firmy AI i rynek pracy** (`M7-firmy-ai-i-rynek-pracy.md`).
Dokument nadrzędny: `00-konwencje-i-kontrakty.md`.
Zakres fazy (§2), kontrakty międzyfazowe (§6), ryzyka (§8) i decyzje otwarte (§9)
zostają w dokumencie fazy — tu jest wyłącznie to, co robisz w tej porcji.

| | |
|---|---|
| **Wejście** | M7a (model firmy), M5 (`BaseRate`, księga). |
| **Pakiety robocze** | WP8, WP9 |
| **Projekt techniczny** | §5.12, §5.13 |
| **Wynik do pokazania** | Firma bierze kredyt, przestaje go obsługiwać, bankrutuje, a wierzyciele są zaspokajani w udokumentowanej kolejności. |
| **Kryterium zamknięcia** | Kryteria WP8 i WP9. **Decyzja właściciela produktu** (`00-postep.md`): kolejność zaspokajania rozstrzygnięta przed startem WP9 — **pracownicy przed wierzycielem zabezpieczonym**, wariant domyślny `D7`. Układ siedzi w `ClaimPriority` (`engine/core`) i jest tam regułą podziału, nie tylko indeksem. |
| **Poprzednia / następna** | `M7c-polityki-i-menedzerowie.md` · `M7e-ai-firm.md` |

Kredyt, leasing, faktoring i obligacje oraz postępowanie upadłościowe z syndykiem, kolejnością zaspokojenia i wyprzedażą majątku.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| WP8 ✅ | Finanse firmy: kredyt, leasing, faktoring, obligacje | WP1, M5 (`BaseRate`, księga) | M |
| WP9 ✅ | Bankructwo: syndyk, kolejność zaspokojenia, wyprzedaż | WP8, WP3 | M |

### WP8 — Finanse firmy

Kredyt obrotowy (linia odnawialna) i inwestycyjny (harmonogram rat), leasing (aktywo należy do
leasingodawcy do wykupu — kluczowe dla WP9), faktoring (sprzedaż należności z dyskontem),
obligacje (prosty zapis: nabywcy to mieszkańcy z oszczędnościami i inne firmy; rynek wtórny — M10).
Bank jest zwykłą firmą z polityką kredytową; kreację pieniądza księguje `sim/economy` (M5).

*Kryterium ukończenia:* test zachowania pieniądza przechodzi z włączonymi wszystkimi czterema
instrumentami; harmonogram rat sumuje się dokładnie do kwoty kredytu + odsetek (i64, reszta do
pierwszej raty wg reguły z dokumentu 00 §2).
**Spełnione:** `cztery_instrumenty_zachowuja_pieniadz` (60 miesięcy gry, wszystkie cztery naraz
i na przemian) oraz `harmonogram_kredytu_inwestycyjnego_sumuje_sie_co_do_grosza` — oba
w `sim/economy/tests/corpfin.rs`. Resztę zmiata **ostatnia** rata, nie pierwsza (`AU`-owa
własność P6 z M5d); reguła z 00 §2 dotyczy podziału kwoty między strony i jest spełniona
w `split_proportional`, którym idzie podział masy upadłościowej.

### WP9 — Bankructwo i syndyk

Maszyna stanów `Filed → Valuation → Auction(rundy) → Distribution → Closed`.
Wyprzedaż majątku to **zwykłe oferty** na istniejącym rynku (M5/M6) z harmonogramem obniżek
i terminem — okazja dla gracza. Niesprzedane po ostatniej rundzie → wartość złomu.
Aktywa leasingowane wracają do leasingodawcy i nie wchodzą do masy.
Kolejność zaspokojenia: wynagrodzenia i odprawy → wierzyciele zabezpieczeni (do wartości
zabezpieczenia) → **zobowiązania publiczne: miasto jako wierzyciel podatkowy (M8 zgłasza
roszczenie, M7 je zaspokaja — K-10)** → wierzyciele niezabezpieczeni proporcjonalnie → właściciele.
Postępowanie przyjmuje wierzycieli z zewnątrz przez `file_claim` — M7 jest właścicielem
całej upadłości, żadna faza nie ma własnej ścieżki egzekucji.

*Kryterium ukończenia:* property-test `bankruptcy_conserves_money_and_assets` (§7) zielony na
10 000 losowych konfiguracji. **Spełnione** — `sim/economy/tests/corpfin.rs`. Test stoi na
**czystej** funkcji `Bankruptcy::plan_distribution`, bo niezmienniki 5, 6 i 7 mówią o planie
wypłat, a nie o przelewach; niezmienniki 1, 2 i 3 (pieniądz, loty, leasingi) wymagają realnych
kont i sprawdza je `postepowanie_nie_gubi_pieniedzy_ani_lotow` na pełnym przebiegu. Niezmiennik 4
(każdy `Employment` zakończony dokładnie raz) jest w `sim/economy/tests/labor.rs`, bo tam stoi
rusztowanie rynku pracy — `upadlosc_konczy_kazda_umowe_dokladnie_raz`.

---

## Projekt techniczny

Numeracja sekcji jest ta sama co w pierwotnym dokumencie fazy — odesłania
w tekście („patrz §5.4") nadal wskazują tę samą treść.

### 5.12 Finanse firmy (§7.8)

```rust
pub struct Loan {
    pub id: LoanId, pub lender: FirmId,         // bank jest zwykłą firmą
    pub kind: LoanKind,                         // Working (linia odnawialna) | Investment
    pub principal: Money, pub outstanding: Money,
    pub rate_bp: u16,                           // marża + BaseRate z sim/economy (M5)
    pub schedule: SmallVec<[Installment; 12]>,  // suma rat == kapitał + odsetki, co do grosza
    pub collateral: Option<AssetRef>,           // zabezpieczenie -> priorytet w bankructwie
    pub missed: u8,
}
pub struct Lease {                              // aktywo NALEŻY do leasingodawcy do wykupu
    pub lessor: FirmId, pub asset: AssetRef,
    pub monthly: Money, pub months_left: u16, pub buyout: Money,
}
pub struct Bond { pub face: Money, pub coupon_bp: u16, pub maturity: SimMinute,
                  pub holders: Vec<(Owner, Money)> }   // rynek wtórny: M10
```

Faktoring: sprzedaż `Receivable` (z §6.9) za `wartość × (1 − discount_bp)` — natychmiastowa gotówka
kosztem marży. Typowe wyjście z chwilowej utraty płynności i typowy sygnał kłopotów dla obserwatora.

Ocena zdolności kredytowej przez bank: historia spłat, przepływy z `equity_history`, zabezpieczenie,
`BaseRate` i polityka kredytowa banku. Bank może odmówić (`CreditDenial`) — i to jest częsty
mechanizm wpychający firmę w bankructwo, a nie awaryjny wyjątek.

### 5.13 Bankructwo z syndykiem (§7.8) — postępowanie prowadzi M7 (K-10)

**Własność (K-10):** `Bankruptcy` należy w całości do M7. Inne fazy są wyłącznie **wierzycielami
zgłaszającymi roszczenie** do postępowania, które prowadzi M7 — w szczególności M8 (urząd
skarbowy z zaległościami podatkowymi) nie ma własnej ścieżki egzekucji.
Dlatego postępowanie jest zaprojektowane jako **otwarte na wierzycieli z zewnątrz**.

```rust
pub struct Bankruptcy {
    pub firm: FirmId,
    pub opened: SimMinute,
    pub trigger: BankruptcyTrigger,  // IlliquidDays{n} | NegativeEquityAndDefault | CourtOrder
    pub stage: BankruptcyStage,      // Filed | Valuation | Auction{round} | Distribution | Closed
    pub estate: Vec<AssetLot>,       // sortowane po (kind, id) — determinizm
    pub claims: Vec<Claim>,          // sortowane po (priority, creditor) — determinizm
    pub proceeds: Money,
    pub trustee_fee_bp: u16,
}

pub struct Claim {
    pub creditor: Creditor,
    pub priority: ClaimPriority,
    pub amount: Money,
    pub collateral: Option<AssetRef>,   // Some => Secured do wartości zabezpieczenia,
                                        // nadwyżka spada do Unsecured (jawnie, nie po cichu)
    pub filed: SimMinute,
    pub origin: ClaimOrigin,            // do karty inspekcji: skąd wziął się dług
}

/// Wierzyciele z ZEWNĄTRZ postępowania — lista jest zamknięta w kodzie, ale rozszerzalna per faza.
pub enum Creditor {
    Employee(CitizenId),      // zaległe pensje i odprawy — M7
    Bank(FirmId),             // kredyty, leasing — M7
    Supplier(FirmId),         // niezapłacone faktury B2B — M6/M7
    BondHolder(Owner),        // obligacje — M7
    City(ClaimKind),          // ZALEGŁOŚCI PODATKOWE, opłaty, kary — M8 zgłasza, M7 zaspokaja
    Landlord(Owner),          // zaległy czynsz za parcelę/budynek — M10
    Other(FirmId),            // kary umowne z kontraktów (§6.2)
}

pub enum ClaimPriority {
    Wages     = 0,  // zaległe wynagrodzenia
    Severance = 1,  // odprawy
    Secured   = 2,  // wierzyciele zabezpieczeni, do wartości zabezpieczenia
    Public    = 3,  // MIASTO: podatki, składki, opłaty, kary administracyjne (M8)
    Unsecured = 4,  // dostawcy, obligatariusze, kary umowne, nadwyżki ponad zabezpieczenie
    Owners    = 5,  // właściciele — to, co zostanie (zwykle nic)
}

/// JEDYNA droga zgłoszenia roszczenia. Woła ją M8 (podatki), M6 (dostawcy), M10 (czynsz).
/// Po przejściu w stage Distribution zgłoszenia są odrzucane (ClaimTooLate) — termin jest twardy,
/// żeby postępowanie było deterministyczne i skończone.
pub fn file_claim(b: &mut Bankruptcy, c: Claim) -> Result<ClaimId, ClaimRejected>;
```

Kolejność zaspokojenia jest **jedną stałą tablicą** `ClaimPriority` — zmiana układu (np. D7)
nie wymaga zmiany logiki podziału. W obrębie priorytetu podział proporcjonalny z regułą reszty
z dokumentu 00 §2, przy remisach sortowanie po `(Creditor, ClaimId)`.

Do czasu M8 miasto po prostu nie zgłasza roszczeń (`Creditor::City` istnieje, nikt go nie używa) —
wpięcie M8 nie zmienia ani struktury, ani testu §7.2, tylko dokłada wpisy do `claims`.

Przebieg:

1. **Filed** — natychmiast: wszystkie `Employment` firmy kończą się
   (`QuitCause::EmployerBankrupt`), naliczane są odprawy jako `Claim { Severance }`;
   zakłady wstrzymują produkcję (M6). **Aktywa leasingowane wracają do leasingodawcy**
   i nie wchodzą do masy.
2. **Valuation** — wycena lotów: budynki i maszyny wg wartości księgowej po amortyzacji,
   zapasy wg kosztu, należności wg dyskonta jakości dłużnika.
3. **Auction** — loty trafiają **na zwykły rynek ofert M5/M6** jako `Offer` z ceną
   `wycena × (1 − dyskont_rundy)`; 3 rundy po 15 dni gry (pół miesiąca w siatce 30-dniowej),
   dyskont 0% / 25% / 50%; całe postępowanie mieści się w 1,5 miesiąca.
   Gracz widzi je w panelu Rynek jak każdą inną ofertę. To jest cała „okazja dla gracza" —
   bez osobnego podsystemu aukcyjnego, bez osobnego UI, bez drugiego mechanizmu wyceny.
4. **Distribution** — zaspokojenie wg `ClaimPriority`; w obrębie priorytetu proporcjonalnie,
   z regułą reszty z dokumentu 00 §2 (reszta do pierwszego wg ustalonego porządku).
5. **Closed** — loty niesprzedane po rundzie 3 idą po wartości złomu; jeśli i to zawiedzie,
   są **jawnie spisywane** rekordem `AssetWriteOff` — nie znikają po cichu (test §7.2).
   Firma → `FirmStatus::Closed`; dyrektor wraca na rynek pracy z piętnem w pamięci firm.

Skutki w dzielnicy (§7.8: „skutki w dzielnicy") są emergentne: fala bezrobocia → spadek popytu
w okolicznych sklepach → spadek wartości gruntu. Żadnych modyfikatorów „na sztywno".

---

## Zmiany wpisane po M7d

Korekty **tej** podfazy naniesione w trakcie jej wykonania (`K-18`). Poprawki dokumentu
fazy są w tabeli „Zmiany wpisane po M7d" w `M7-firmy-ai-i-rynek-pracy.md`.
Gwiazdka = zmiana zakresu albo kryterium.

| # | Co | Dlaczego |
|---|---|---|
| `BA-1`* | **Finanse firmy i upadłość mieszkają w `sim/economy::corpfin`, a nie w `sim/firms`.** §5.12 i §5.13 wypisywały `Loan`, `Lease`, `Bond`, `Bankruptcy` i `Claim` jako typy `sim/firms` | `sim/firms` **nie zna `Books`** i znać nie może: zależność idzie `economy → firms → policy` i odwrócenie zamyka cykl, którego Cargo nie zbuduje. Finanse bez ksiąg to same nazwy pól — kredyt, który niczego nie przelewa, i syndyk, który niczego nie wypłaca. To ten sam podział, który `D2` narzucił rynkowi pracy, a `AY-3` wykonawcy polityki cenowej: mechanizm jest tam, gdzie dane |
| `BA-2`* | **Wierzycielem jest `AccountOwner`, a nie własny enum `Creditor` z §5.13.** `Creditor::City(ClaimKind)` zastępuje para `AccountOwner::City` + `ClaimOrigin::Tax`; M8 zgłasza roszczenie dokładnie tak samo, przez `file_claim` | Typ odpowiadający na pytanie „kto ma konto" już istnieje, pokrywa pracownika, bank, dostawcę, miasto i resztę świata, i ma porządek liniowy potrzebny do determinizmu podziału. Drugi enum o tym samym znaczeniu rozjechałby się przy pierwszym nowym uczestniku — ta sama reguła, którą `K-8` stosuje do słowników |
| `BA-3`* | **Loty upadłościowe nie są `Offer` z rynku M5.** §5.13 obiecywało wystawianie ich „jako zwykłych ofert"; są własną pozycją z ceną rundy, a jedynym wejściem zakupu jest `CorpFinance::bid_lot` — to samo dla AI firm (M7e) i dla gracza (M9) | `Offer` jest ceną **półkową towaru** dla gospodarstwa domowego i indeksuje się po `CategoryId::Stock(StockCat)`. Tokarka nie ma `StockCat` i mieszkaniec jej nie kupi, więc lot nie miałby warstwy, w której mógłby stanąć. To jest dokładnie argument, którym `K-36` odrzucił `Quote` jako widok na `Offer`. Obietnica „bez osobnego podsystemu aukcyjnego" zostaje spełniona inaczej: nie ma licytacji, remisów ani drugiego mechanizmu wyceny — jest cena rundy i jedno wejście |
| `BA-4`* | **`AssetRef` ma dwa rodzaje aktywa, nie trzy: wyposażenie i zapas.** §5.12 zakładało maszyny jako osobne aktywa | M6 nie prowadzi **egzemplarzy** maszyn, tylko klasy w recepturze (`MachineClassId`), więc nie ma czego wskazać — maszyna jest częścią wyposażenia zakładu. Przypadek (2) z `K-18`: typ z przykładu nie istnieje w tej szerokości. Trzeci wariant powstanie razem z egzemplarzami, nie wcześniej |
| `BA-5` | **`Lease` rozróżnia „umowa skończona" (`ended`) od „rzecz jest firmy" (`bought_out`).** §5.12 miało jedno pole | Upadłość kończy **wszystkie** umowy firmy, więc gdyby „skończona" znaczyło „moja", każda rzecz leasingowana wchodziłaby do masy dokładnie w tej chwili, w której najbardziej nie powinna. Złapał to test niezmiennika 3 z §7.2 przy pierwszym uruchomieniu — nie przegląd kodu |
| `BA-6` | **Wynagrodzenie syndyka jest przycięte do gotówki w masie.** §5.13 podawało je jako `proceeds × trustee_fee_bp` bez ograniczenia | Postępowanie, w którym loty sprzedano, a gotówka zdążyła zejść, wypłacałoby syndykowi kwotę, której nie ma — i podział przestawałby się sumować do masy. Złamanie niezmiennika 6 z §7.2 **niewidoczne dla żadnego pojedynczego przelewu**; znalazł je property-test na 10 000 przypadków, przy gotówce zero i niezerowych wpływach |
| `BA-7`* | **Progi upadłości są w `data/tuning/insolvency.ron`, nie w `data/economy/`.** Produkty finansowe (kredyt inwestycyjny, leasing, faktoring, obligacja) dochodzą do `data/economy/bank.ron`, którego `schema_version` idzie 1 → 2 razem z całym katalogiem | Wykonanie `K-35`: w `data/tuning/` siedzą liczby, **które wolno przestawić bez zmiany znaczenia modelu** — po ilu dobach niepłacenia sąd otwiera postępowanie, ile bierze syndyk, o ile tanieje lot. Produkt kredytowy zmienia kształt rozwiązania i zostaje przy `consumer` i `working_capital`, bo rozdzielenie ich na dwa pliki kazałoby czytać oba, żeby zrozumieć jeden. `data/economy/` jest własnością M5 i nie ma być workiem na cudze liczby |
| `BA-8`* | **Konsumenta `PayrollOutbox` w M7d nie ma — adres przesuwa się na M7e.** Zamiast tego M7d domyka **stronę kosztową**: niezapłacona pozycja (czynsz, media, płace, rata) zostaje zaległością i zobowiązaniem w księdze, zamiast znikać bez śladu | Notatka po M7b uzasadniała odłożenie tym, że „firma bez przychodu i bez kredytu nie ma z czego zapłacić". Powód nie zniknął: `SitePnlMonth` nie ma przychodu do M7e (`AV-2`), a zakłady produkcyjne nie mają księgi wcale. Wypłata realnej listy płac z konta, na które nic nie wpływa, wywróciłaby saldo każdej firmy w pierwszym miesiącu — czyli dokładnie to, przed czym tamta notatka ostrzegała, tylko o jedną podfazę później. **Co M7d z tego domyka mimo wszystko:** odprawy przy upadłości są naliczane imiennie (`AccountOwner::Citizen`) i wypłacane z masy, więc niezmiennik 4 z §7.2 ma pisarza i test |
| `BA-9` | **Dwa nowe słowniki w `core` i sześć powodów decyzji (505–510).** `BankruptcyTrigger` i `ClaimPriority` — patrz `K-48` | Oba są ładunkami `DecisionReason`, a ładunek centralnego enuma nie może pochodzić z crate'u, który od `core` zależy — ta sama reguła, która wypchnęła tam `WageCause` (`K-45`) i `PriceDriver` (`K-30`) |
| `BA-10` | **`CorpFinance` rozbity na cztery pliki przy domknięciu pakietu**: `params.rs` (kalibracja z RON), `ops.rs` (leasing, obligacja, faktoring), `proceedings.rs` (postępowanie), `mod.rs` (rejestr i hasz) | Przegląd strukturalny (CLAUDE.md): jeden plik miał 1216 linii i jeden blok `impl` 921. Podział był mechaniczny — przeniesienie bloków bez zmiany zachowania — więc wszedł w tym samym commicie, zgodnie z odpowiedzią nr 1 reguły |

---

## Zmiany wpisane po M7b

Poprawki wpisane przez podfazę **M7b** (`K-18`).

| # | Zmiana | Dlaczego |
|---|---|---|
| ★ | **`PayrollOutbox` czeka na konsumenta i to jest zadanie M7d.** Skrzynka niesie dwie listy: `PayrollRun::items` (brutto per umowa, potrącenia zerowe — `D6`) i dołożone w M7b `PayrollRun::hr_costs` (koszt kadrowy per zakład: premie, odprawy, świadczenia, szkolenia). Dziś nikt jej nie opróżnia, a dochód gospodarstwa nadal płynie z konta „reszta świata" | Zamknięcie obiegu **wymaga finansów**: firma bez przychodu i bez kredytu nie ma z czego zapłacić, więc księgowanie listy płac bez M7d wywróciłoby saldo każdej firmy w mieście w pierwszym miesiącu. Kolejność jest więc właściwa, ale dług trzeba widzieć: do M7d pieniądz za pracę **nie przechodzi** przez konto firmy |
| ★ | **`Household.income_monthly` jest od M7b prowadzone przez rynek pracy** — zatrudnienie je podnosi, odejście obniża, a wartość nigdy nie schodzi poniżej zera. To jest **denormalizacja**, nie drugie źródło prawdy: umowa po stronie firmy zostaje jedyną prawdą o płacy (`D1`) | Gdy M7d zacznie księgować wypłaty z konta firmy, ta liczba przestaje być kanałem dochodu i zostaje wyłącznie tym, czym jest w M5: prognozą, z której gospodarstwo planuje budżet. Przejście trzeba zrobić **jednym ruchem**, inaczej gospodarstwo dostanie pensję dwa razy |
| ★ | **Bankructwo zastanie na liście płac ludzi, nie tylko liczby.** Rejestr firm po M7b prowadzi obsadę w czasie: `Position::filled` zmienia się codziennie, a każde wyjście z etatu idzie przez `hr::odejdz` — jedyną drogę, która zwalnia etat i po stronie firmy, i po stronie mieszkańca, i w puli wakatów miasta | Niezmiennik 4 z §7.2 („każdy `Employment` zakończony dokładnie raz, dokładnie jedna odprawa naliczona") ma po M7b gotowy mechanizm: syndyk woła tę samą funkcję z `LeaveCause::Redundancy`. Wariant `Redundancy` istnieje w `core` od M7b **właśnie po to** i do M7d nie ma pisarza |
