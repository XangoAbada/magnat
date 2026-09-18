//! Sklep: zaplecze, półka, asortyment (M5b §5.3, PRD §7.1, §7.3).
//!
//! **Zaplecze i półka to dwa różne stany.** `Offer.available` odzwierciedla wyłącznie
//! półkę — towar w zapleczu nie jest na sprzedaż. Przy wyczerpaniu półki oferta
//! zostaje (z `available == 0`), bo sklep ma pozostać widoczny w inspekcji jako
//! „znany, ale bez towaru": to jest odpowiedź na pytanie z PRD §14.1, a nie
//! przeoczenie w sprzątaniu ofert.

use std::collections::BTreeMap;

use magnat_agents::{SocialClass, SOCIAL_CLASS_COUNT};
use magnat_core::{
    FirmId, GoodId, HashState, Money, Qty, RejectCause, SiteId, StateHasher, Tick, UtilityKind,
    REJECT_CAUSE_COUNT, UTILITY_KIND_COUNT,
};

use crate::ledger::Ledger;
use crate::pricing::{CompetitorSnapshot, FirmPricing, PriceController};

/// Polityka zapasu: poniżej `point` zamawiamy do `target`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ReorderPolicy {
    pub point: Qty,
    pub target: Qty,
    pub lead_time_days: u8,
}

/// Kto decyduje o asortymencie.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum AssortmentPolicy {
    /// Gracz wybiera towary z listy.
    Manual { goods: Vec<GoodId> },
    /// AI: tyle linii, ile mieści półka, w kolejności rangi substytutu kategorii.
    ///
    /// Plan §5.3 mówi „top-N wg marża × popyt w dzielnicy". `ponytail:` sufit nazwany:
    /// popytu w dzielnicy w M5b jeszcze nie ma — mierzy go dopiero balansator (M5e),
    /// a marża jest dziś stałą z danych, więc ranking po marży × popyt byłby
    /// rankingiem po jednej znanej liczbie. Ścieżka wyjścia: ranking wchodzi razem
    /// z `ObservedElasticity` w M5c, kiedy będzie z czego go policzyć.
    Auto { max_lines: u16, min_margin_bp: i32 },
}

/// Linia na półce — **ekspozycja, nie zapas** (WP11).
///
/// Do M6c linia niosła ilość, koszt nabycia i datę ważności, bo nie było gdzie indziej
/// ich trzymać. Od WP11 towar leży w slocie magazynowym o roli
/// [`WarehouseRole::Shelf`](magnat_supply::WarehouseRole::Shelf) i to on odpowiada na
/// pytanie „ile i za ile" — razem z jakością, marką i pochodzeniem, których linia
/// nigdy nie znała. Zostaje to, czego magazyn nie wie: **ile miejsca zajmuje na półce**
/// i **pod jaką ofertą stoi**.
///
/// `offer` jest uchwytem do areny ofert — cena czyta się z areny na żywo, więc przecena
/// nie rusza ani półki, ani indeksu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ShelfLine {
    pub good: GoodId,
    /// Ile miejsc na półce; limit ekspozycji, czyli sufit wyłożenia.
    pub facings: u16,
    pub offer: crate::offer::OfferId,
}

/// Półka sklepu. `lines` jest **posortowane po `GoodId`** — to ona ustala kolejność
/// iteracji, więc żadna pętla sklepu nie zależy od kolejności wstawiania.
#[derive(Clone, Default)]
pub struct Shelf {
    pub slots: u16,
    pub lines: Vec<ShelfLine>,
}

impl Shelf {
    #[must_use]
    pub fn line(&self, good: GoodId) -> Option<&ShelfLine> {
        self.lines
            .binary_search_by_key(&good, |l| l.good)
            .ok()
            .map(|i| &self.lines[i])
    }

    pub fn line_mut(&mut self, good: GoodId) -> Option<&mut ShelfLine> {
        self.lines
            .binary_search_by_key(&good, |l| l.good)
            .ok()
            .map(|i| &mut self.lines[i])
    }

    /// Wstawia linię, zachowując porządek po `GoodId`. Zwraca `false`, gdy półka pełna.
    pub fn insert(&mut self, line: ShelfLine) -> bool {
        match self.lines.binary_search_by_key(&line.good, |l| l.good) {
            Ok(_) => false,
            Err(i) => {
                if self.lines.len() >= usize::from(self.slots) {
                    return false;
                }
                self.lines.insert(i, line);
                true
            }
        }
    }
}

/// Zaplecze sklepu (§7.3: pojemność wynika z budynku).
///
/// Od WP11 **towaru tu nie ma** — jest w slocie
/// [`WarehouseRole::Backroom`](magnat_supply::WarehouseRole::Backroom) magazynu M6.
/// Zostaje pojemność (bo to parametr budynku) i polityka zamawiania (bo to decyzja
/// sklepu, którą M6 wyłącznie wykonuje).
#[derive(Clone, Default)]
pub struct ShopInventory {
    pub capacity_m3: i64,
    pub reorder: BTreeMap<GoodId, ReorderPolicy>,
}

// ── utracone sprzedaże (kontrakt uzgodniony z M9) ────────────────────────────────

/// Trójstopniowe śledzenie: pełny bufor na wszystkich sklepach AI byłby kosztem
/// bez odbiorcy (§5.4).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LostSaleTracking {
    #[default]
    None,
    Histogram,
    Full,
}

impl LostSaleTracking {
    /// Nazwa wariantu — klucz tekstu w `data/locale/`. Taka sama konwencja co
    /// w słownikach `vocab_enum!` z `core`, żeby interfejs nie musiał trzymać
    /// własnego `match` na coś, co nie jest wyborem projektowym, tylko poziomem.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            LostSaleTracking::None => "None",
            LostSaleTracking::Histogram => "Histogram",
            LostSaleTracking::Full => "Full",
        }
    }
}

/// Jedna utracona sprzedaż — 24 B, pierścień 256 wpisów na sklep.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LostSale {
    pub citizen: magnat_core::CitizenId,
    pub good: GoodId,
    pub when: Tick,
    pub cause: RejectCause,
    pub went_to: Option<SiteId>,
}

pub const LOST_SALE_RING: usize = 256;

/// Histogram doby: ile razy który powód, bez pamiętania kto.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct LostSaleHistogram {
    pub day: u32,
    pub by_cause: [u32; REJECT_CAUSE_COUNT],
}

/// Zapis utraconych sprzedaży jednego sklepu.
#[derive(Clone, Default)]
pub struct ShopLostSales {
    pub today: LostSaleHistogram,
    /// Siedem domkniętych dób, slot indeksowany numerem doby modulo 7. Kryterium
    /// WP12 pyta o **ostatnie 7 dni**, a nie o dzisiaj: „dlaczego Anna nie kupiła
    /// u mnie" musi mieć odpowiedź także wtedy, gdy Anna była wczoraj.
    week: [LostSaleHistogram; 7],
    ring: Vec<LostSale>,
    head: usize,
}

impl ShopLostSales {
    /// Sprawdzenie poziomu jest jednym odczytem bitu, więc rynek obsadzony wyłącznie
    /// przez AI nie płaci za ten mechanizm nic (§5.4).
    pub fn record(&mut self, level: LostSaleTracking, day: u32, sale: LostSale) {
        if level == LostSaleTracking::None {
            return;
        }
        if self.today.day != day {
            // Domknięta doba idzie do okna tygodnia, zanim zostanie wyzerowana.
            self.week[(self.today.day % 7) as usize] = self.today;
            self.today = LostSaleHistogram {
                day,
                by_cause: [0; REJECT_CAUSE_COUNT],
            };
        }
        self.today.by_cause[sale.cause.as_index()] += 1;
        if level != LostSaleTracking::Full {
            return;
        }
        if self.ring.len() < LOST_SALE_RING {
            self.ring.push(sale);
        } else {
            self.ring[self.head] = sale;
            self.head = (self.head + 1) % LOST_SALE_RING;
        }
    }

    /// Histogram siedmiu ostatnich dób łącznie z bieżącą — to jest okno, o które
    /// pyta kryterium WP12. Sloty starsze niż tydzień odpadają po polu `day`,
    /// więc sklep, który nie tracił sprzedaży od miesiąca, pokazuje zera,
    /// a nie zeszłomiesięczny osad.
    #[must_use]
    pub fn window(&self, day: u32) -> LostSaleHistogram {
        let mut out = LostSaleHistogram {
            day,
            by_cause: [0; REJECT_CAUSE_COUNT],
        };
        for h in std::iter::once(&self.today).chain(self.week.iter()) {
            if h.day > day || day - h.day >= 7 {
                continue;
            }
            for (o, v) in out.by_cause.iter_mut().zip(h.by_cause) {
                *o += v;
            }
        }
        out
    }

    /// Od najstarszej do najnowszej.
    pub fn iter(&self) -> impl Iterator<Item = &LostSale> {
        self.ring[self.head..]
            .iter()
            .chain(self.ring[..self.head].iter())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.ring.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }
}

/// Kto kupił w tym zakładzie: skąd, z jakiej klasy i co przeważyło w jego wyborze
/// (§5.12, trzy pytania karty „Klienci").
///
/// Zbierane **wyłącznie dla zakładów śledzonych** i **poza hashem stanu** — z tego
/// samego powodu co pierścień utraconych sprzedaży (`U-22`): kliknięcie „śledź"
/// nie ma prawa zmieniać świata.
///
/// `ponytail:` rozkłady są kumulatywne od włączenia śledzenia, a nie w oknie
/// siedmiu dób — okno prowadzi wyłącznie `daily`, czyli ta jedna liczba, na której
/// widać skutek podwyżki („po 3 dniach spadek liczby klientów" z §1 dokumentu fazy).
/// Sufit: rozkład dzielnic po pół roku śledzenia pokazuje średnią, nie bieżący
/// zasięg. Wyjście: ten sam pierścień siedmiodobowy co w `ShopLostSales`, kiedy
/// nakładka „zasięg sklepu" zacznie kłamać.
#[derive(Clone, Default)]
pub struct ShopCustomers {
    /// Dzielnica zamieszkania kupującego → liczba zakupów.
    pub by_district: BTreeMap<u16, u32>,
    /// Klasa społeczna kupującego, indeks z `SocialClass::as_index()`.
    pub by_class: [u32; SOCIAL_CLASS_COUNT],
    /// Człon użyteczności, który przeważył w wyborze tego sklepu.
    pub by_driver: [u32; UTILITY_KIND_COUNT],
    /// Liczba zakupów w każdej z siedmiu ostatnich dób, slot indeksowany dobą modulo 7.
    pub daily: [u32; 7],
    /// Doba ostatniego zapisu — po niej zeruje się slot, do którego wchodzi nowa doba.
    day: u32,
    /// Łączna liczba zakupów od włączenia śledzenia.
    pub total: u32,
    /// Ostatni kupujący — pierścień [`RECENT_BUYERS`] wpisów, prowadzony wyłącznie
    /// przez zakłady śledzone, tak samo jak pierścień utraconych sprzedaży.
    ///
    /// Po co osobno od `by_district`: filtr „pokaż tylko klientów mojego sklepu"
    /// (PRD §14.2) pyta o **osobę**, a rozkład po dzielnicach odpowiada o zbiorze.
    /// Koszt przy graczu z 200 sklepami to 200 × 256 × 4 B ≈ 200 kB.
    buyers: Vec<magnat_core::CitizenId>,
    buyers_head: usize,
}

/// Ilu ostatnich kupujących pamięta zakład śledzony.
pub const RECENT_BUYERS: usize = 256;

impl ShopCustomers {
    /// Czy ten mieszkaniec jest wśród ostatnich kupujących.
    #[must_use]
    pub fn bought(&self, citizen: magnat_core::CitizenId) -> bool {
        self.buyers.contains(&citizen)
    }

    /// Zapis jednego zakupu. Wołane tylko dla zakładów śledzonych.
    pub fn record(
        &mut self,
        day: u32,
        district: u16,
        class: SocialClass,
        driver: UtilityKind,
        buyer: magnat_core::CitizenId,
    ) {
        if self.day != day {
            // Doby pominięte (sklep bez klientów) też muszą się wyzerować, inaczej
            // tydzień temu zostałby w oknie jako „dzisiaj".
            let ile = day.saturating_sub(self.day).min(7);
            for i in 1..=ile {
                self.daily[((self.day + i) % 7) as usize] = 0;
            }
            self.day = day;
        }
        *self.by_district.entry(district).or_insert(0) += 1;
        self.by_class[class.as_index()] += 1;
        self.by_driver[driver.as_index()] += 1;
        self.daily[(day % 7) as usize] += 1;
        self.total += 1;
        if self.buyers.len() < RECENT_BUYERS {
            self.buyers.push(buyer);
        } else {
            self.buyers[self.buyers_head] = buyer;
            self.buyers_head = (self.buyers_head + 1) % RECENT_BUYERS;
        }
    }
}

// ── sklep ────────────────────────────────────────────────────────────────────────

/// Zakład handlowy: budynek, magazyn, półka, oferta.
pub struct Shop {
    pub site: SiteId,
    pub firm: FirmId,
    /// Rachunek bieżący sklepu w [`crate::Books`].
    pub account: crate::books::AccountId,
    /// Pozycja w metrach — z budynku, w którym stoi zakład.
    pub pos: magnat_spatial::Vec2,
    /// Dzielnica zakładu — wyłącznie do metryk koncentracji (bramka G6).
    pub district: u16,
    pub kind: magnat_core::PlaceKind,
    pub hours: magnat_agents::OpenHours,
    pub inventory: ShopInventory,
    pub shelf: Shelf,
    /// Slot zaplecza w magazynie M6 — tu ląduje dostawa i stąd idzie wyłożenie.
    pub backroom: magnat_supply::SlotId,
    /// Slot półki. To z niego zdejmuje sprzedaż i to on wchodzi do
    /// `prop_no_expired_on_shelf`.
    pub shelf_slot: magnat_supply::SlotId,
    pub assortment: AssortmentPolicy,
    pub tracking: LostSaleTracking,
    pub lost: ShopLostSales,
    /// Kto u mnie kupuje (§5.12). Jak `lost`: wyłącznie dla zakładów śledzonych
    /// i poza hashem stanu.
    pub customers: ShopCustomers,
    /// Licznik sprzedanych sztuk od początku świata — wejście do metryk balansatora.
    pub sold_qty: i64,
    /// Szybki podgląd obrotu dla scenariusza. **Liczbą w panelu jest `Revenue`
    /// z [`Ledger`]**, nie to pole — księga jest źródłem prawdy o wyniku (§5.8).
    pub revenue: Money,
    /// Daniny naliczone od sprzedaży i jeszcze nieodprowadzone do miasta (M8a WP2).
    pub accrued: TaxAccrual,
    // ── M5c ──
    /// Osobowość cenowa firmy: czułości i widełki marży (§5.6).
    pub pricing: FirmPricing,
    /// Sterownik ceny per towar. `BTreeMap`, więc iteracja idzie po `GoodId`
    /// i nie zależy od kolejności wstawiania (00 §3.2).
    pub controllers: BTreeMap<GoodId, PriceController>,
    /// Obraz cen konkurencji z opóźnieniem 1–7 dni (§6.3).
    pub observed: CompetitorSnapshot,
    /// Księga zakładu (§5.8).
    pub ledger: Ledger,
    /// Powody ostatnich przecen — wyjaśnialność §7 dla zakładów śledzonych.
    /// Nie wchodzi do hasha z tego samego powodu co pierścień utraconych sprzedaży:
    /// prowadzi go wyłącznie zakład oznaczony, a oznaczenie nie jest stanem świata.
    pub reprice_log: Vec<magnat_core::DecisionReason>,
    /// Ślad ostatnich dób dla dry-runu polityk (M9d WP8). Jak `reprice_log`:
    /// prowadzi go wyłącznie zakład oznaczony i nie wchodzi do hasha stanu.
    pub trace: crate::policy_run::PolicyTrace,
    /// Miesięczny odpis amortyzacyjny wyposażenia, liniowy.
    pub depreciation_monthly: Money,
    // ── M5d ──
    /// Czynny kredyt obrotowy, jeśli sklep go wziął (§5.10). Jeden naraz: drugi
    /// kredyt pod ten sam zapas to refinansowanie, a to jest mechanika M7.
    pub loan: Option<crate::books::LoanId>,
    /// Tick otwarcia zakładu — staż działalności w ocenie zdolności kredytowej.
    pub opened: magnat_core::Tick,
    /// Zakład zamknięty decyzją taktyczną albo upadłością (M7e WP12).
    ///
    /// Rekord **zostaje** razem z księgą: historia wyniku jest tym, z czego panel M9
    /// tłumaczy graczowi, dlaczego zakład padł, a usunięcie sklepu z wektora
    /// przesunęłoby indeksy wszystkich pozostałych. Zamknięty sklep nie ma półki,
    /// nie zamawia i nie płaci czynszu — bo go nie wynajmuje.
    pub closed: bool,
    // ── M8d ──
    /// Udział obrotu **poza deklaracją**, w punktach bazowych (M8d WP8, PRD §10.4).
    ///
    /// Szara strefa obniża **fakt, a nie naliczenie**: ta część utargu nie wchodzi
    /// do [`TaxAccrual`] i nie staje się przychodem w księdze — idzie prosto do
    /// kapitału właściciela. Inaczej powstałyby dwie prawdy o obrocie, jedna
    /// w księdze zakładu i druga w rejestrze miasta, a to jest dokładnie ta para
    /// liczb, którą porównuje test T1.
    ///
    /// Zero znaczy „deklaruję wszystko" i jest stanem domyślnym — zakład zaczyna
    /// uczciwie i schodzi w szarą strefę dopiero pod presją wyniku.
    pub unreported_bps: u16,
    /// Do kiedy zakład jest zamknięty decyzją urzędu (`Remedy::Closure`).
    ///
    /// Osobne pole od `closed`, bo to są dwie różne rzeczy: `closed` jest końcem
    /// zakładu, a to jest przerwą w jego działaniu. Zakład otwiera się z powrotem
    /// sam, gdy tick minie — bez tego sanepid zamykałby restauracje na zawsze.
    pub suspended_until: magnat_core::Tick,
    /// Masa odpisana z powodu przekroczonego terminu, narastająco (M8d WP8).
    ///
    /// Licznik, nie stan: to jest materiał dowodowy sprawy sanitarnej. Sklep, który
    /// przez rok wyrzucił tonę zepsutego nabiału, ma inną kartotekę niż sklep, który
    /// wyrzucił kilogram — i bez tej liczby inspekcja nie miałaby czego zobaczyć,
    /// bo magazyn liczy straty per **towar**, a nie per slot.
    pub expired_mass: magnat_core::Mass,
}

/// Kolejka danin zakładu do najbliższej deklaracji (M8a WP2).
///
/// **Kolejka, nie saldo.** Zeruje ją `Market::take_tax_accrued` przy miesięcznej
/// deklaracji. Saldem jest konto `TaxPayable` w księdze zakładu — tam siedzą
/// **wszystkie** daniny razem, więc deklaracja VAT nie ma jak wyczytać z niego
/// swojej części i musi mieć własny licznik.
///
/// Do M8 wszystkie trzy liczby są zerami, bo `NoTax` nie nalicza niczego.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct TaxAccrual {
    /// VAT wyłuskany z ceny brutto sprzedaży detalicznej.
    pub vat: Money,
    /// Obrót netto objęty tym podatkiem — podstawa, nie kwota. Bez niej deklaracja
    /// miesięczna niosłaby kwotę bez stawki, a karta inspekcji nie miałaby czym
    /// odpowiedzieć na „dlaczego tyle".
    pub vat_base: Money,
    /// Akcyza od wyrobów obłożonych, naliczona kwotowo od masy.
    pub excise: Money,
    /// Masa wyrobów obłożonych akcyzą — podstawa, nie kwota. Bez niej karta
    /// inspekcji pokazałaby kwotę, której nie da się wytłumaczyć.
    pub excise_mass: magnat_core::Mass,
}

impl HashState for TaxAccrual {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_i64(self.vat.get());
        h.write_i64(self.vat_base.get());
        h.write_i64(self.excise.get());
        h.write_i64(self.excise_mass.0);
    }
}

/// Daniny naliczone na rozliczeniu hurtowym jednego zakładu (M8a WP2).
///
/// Osobna struktura od [`TaxAccrual`], bo w hurcie **nie ma VAT-u**: cena B2B jest
/// netto (`K-7`), a podatek od wartości dodanej rozlicza się dopiero na końcu
/// łańcucha. Wspólna struktura z dwoma martwymi polami po każdej stronie byłaby
/// gorsza od dwóch — a przy okazji sugerowałaby, że hurt VAT-u nie płaci przez
/// przeoczenie, a nie z definicji.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct B2bTax {
    /// Wartość celna towaru wprowadzonego na obszar miasta.
    pub customs_value: Money,
    /// Cło już zapłacone przy odprawie — liczy je `sim/supply` (`K-36`).
    pub duty: Money,
    /// Akcyza od wyrobu obłożonego, naliczona kwotowo od masy.
    pub excise: Money,
    pub excise_mass: magnat_core::Mass,
}

impl HashState for B2bTax {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_i64(self.customs_value.get());
        h.write_i64(self.duty.get());
        h.write_i64(self.excise.get());
        h.write_i64(self.excise_mass.0);
    }
}

impl HashState for Shop {
    fn hash_state(&self, h: &mut StateHasher) {
        self.site.entity().hash_state(h);
        self.firm.entity().hash_state(h);
        // Zapas **nie wchodzi tutaj**: od WP11 leży w magazynie M6 i haszuje go zasób
        // `Chain` (kolejność indeksów slotów i partii, `K-16`). Haszowanie go drugi raz
        // po tej stronie liczyłoby ten sam gram dwa razy i przy pierwszej rozbieżności
        // nie dałoby się powiedzieć, która z dwóch kopii kłamie. Zostaje ekspozycja:
        // który towar stoi na półce, w ilu miejscach i pod jaką ofertą.
        h.write_u32(self.backroom.0);
        h.write_u32(self.shelf_slot.0);
        h.write_u32(self.shelf.lines.len() as u32);
        for l in &self.shelf.lines {
            l.good.hash_state(h);
            h.write_u16(l.facings);
        }
        h.write_i64(self.sold_qty);
        self.revenue.hash_state(h);
        self.accrued.hash_state(h);
        // M5c: stan cenowy i księgowy jest **stanem trwałym**, więc wchodzi do hasha
        // (00 §3.6). Poziom śledzenia i pierścień dziennika — nie (`U-22`).
        h.write_u32(self.controllers.len() as u32);
        for (g, pc) in &self.controllers {
            g.hash_state(h);
            pc.hash_state(h);
        }
        self.observed.hash_state(h);
        self.ledger.hash_state(h);
        self.depreciation_monthly.hash_state(h);
        // M5d: kredyt obrotowy jest stanem zakładu — rata wchodzi do wyniku miesiąca.
        h.write_u32(self.loan.map_or(u32::MAX, |l| l.0));
        h.write_u64(self.opened.get());
        h.write_u8(u8::from(self.closed));
        // M8d: oba pola zmieniają wynik zakładu i wpływy miasta, więc są stanem.
        h.write_u16(self.unreported_bps);
        h.write_u64(self.suspended_until.get());
        h.write_i64(self.expired_mass.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Testy linii zapasu (`zdjecie_calej_linii_zmiata_reszte_groszy`,
    // `dostawa_bierze_wczesniejsza_date_waznosci`) odeszły razem z `StockLine`
    // w WP11: zapas sklepu jest od tej chwili partią w magazynie M6, a obie
    // własności, których pilnowały, mają tam własne testy — zmiatanie reszty
    // groszy w `tysiac_podzialow_nie_gubi_grama_ani_grosza`, a datę przydatności
    // w `Store::spoil`, gdzie jest **własnością partii**, a nie linii.

    #[test]
    fn polka_trzyma_porzadek_po_good_id_i_pilnuje_slotow() {
        let mut s = Shelf {
            slots: 2,
            lines: Vec::new(),
        };
        let l = |g: u16| ShelfLine {
            good: GoodId(g),
            facings: 1,
            offer: crate::offer::OfferId::from_bits(1 << 32).unwrap(),
        };
        assert!(s.insert(l(7)));
        assert!(s.insert(l(3)));
        assert!(!s.insert(l(5)), "półka pełna");
        assert!(!s.insert(l(3)), "towar już jest");
        assert_eq!(
            s.lines.iter().map(|x| x.good.get()).collect::<Vec<_>>(),
            vec![3, 7]
        );
    }

    #[test]
    fn pierscien_utraconych_sprzedazy_nie_rosnie_w_nieskonczonosc() {
        let mut l = ShopLostSales::default();
        let sale = |i: u64| LostSale {
            citizen: magnat_core::CitizenId(magnat_core::Entity::new(
                i as u32,
                std::num::NonZeroU32::MIN,
            )),
            good: GoodId(1),
            when: Tick(i),
            cause: RejectCause::OutOfStock,
            went_to: None,
        };
        for i in 0..(LOST_SALE_RING as u64 + 10) {
            l.record(LostSaleTracking::Full, 0, sale(i));
        }
        assert_eq!(l.len(), LOST_SALE_RING);
        assert_eq!(
            l.today.by_cause[RejectCause::OutOfStock.as_index()],
            LOST_SALE_RING as u32 + 10
        );
        // Najstarszy w pierścieniu to ten, który nie został jeszcze nadpisany.
        assert_eq!(l.iter().next().unwrap().when, Tick(10));
        // Poziom `None` nie kosztuje nic i nic nie zapisuje.
        let mut cichy = ShopLostSales::default();
        cichy.record(LostSaleTracking::None, 0, sale(0));
        assert!(cichy.is_empty());
        assert_eq!(cichy.today.by_cause[RejectCause::OutOfStock.as_index()], 0);
    }
}
