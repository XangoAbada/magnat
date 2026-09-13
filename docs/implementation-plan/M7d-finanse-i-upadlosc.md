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
| **Kryterium zamknięcia** | Kryteria WP8 i WP9. **Decyzja właściciela produktu** (`00-postep.md`): kolejność zaspokajania — pracownicy vs wierzyciel zabezpieczony — musi być rozstrzygnięta przed startem WP9. |
| **Poprzednia / następna** | `M7c-polityki-i-menedzerowie.md` · `M7e-ai-firm.md` |

Kredyt, leasing, faktoring i obligacje oraz postępowanie upadłościowe z syndykiem, kolejnością zaspokojenia i wyprzedażą majątku.

---

## Pakiety robocze

| WP | Nazwa | Zależy od | Rozmiar |
|---|---|---|---|
| WP8 | Finanse firmy: kredyt, leasing, faktoring, obligacje | WP1, M5 (`BaseRate`, księga) | M |
| WP9 | Bankructwo: syndyk, kolejność zaspokojenia, wyprzedaż | WP8, WP3 | M |

### WP8 — Finanse firmy

Kredyt obrotowy (linia odnawialna) i inwestycyjny (harmonogram rat), leasing (aktywo należy do
leasingodawcy do wykupu — kluczowe dla WP9), faktoring (sprzedaż należności z dyskontem),
obligacje (prosty zapis: nabywcy to mieszkańcy z oszczędnościami i inne firmy; rynek wtórny — M10).
Bank jest zwykłą firmą z polityką kredytową; kreację pieniądza księguje `sim/economy` (M5).

*Kryterium ukończenia:* test zachowania pieniądza przechodzi z włączonymi wszystkimi czterema
instrumentami; harmonogram rat sumuje się dokładnie do kwoty kredytu + odsetek (i64, reszta do
pierwszej raty wg reguły z dokumentu 00 §2).

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
10 000 losowych konfiguracji.

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
