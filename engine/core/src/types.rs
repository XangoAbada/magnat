//! Typy bazowe z `00-konwencje-i-kontrakty.md` §2 — kontrakt nienegocjowalny.
//!
//! Wszystkie wielkości są całkowitoliczbowe. Zakaz `f32`/`f64` w pieniądzu,
//! księgowości, stanach magazynowych i podatkach obowiązuje przez konstrukcję:
//! te typy po prostu nie mają wariantu zmiennoprzecinkowego.

use serde::{Deserialize, Serialize};

/// Makro definiujące newtype nad liczbą całkowitą wraz ze standardowym zestawem cech.
macro_rules! scalar_newtype {
    ($(#[$meta:meta])* $name:ident($inner:ty)) => {
        $(#[$meta])*
        #[derive(
            Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default,
            Serialize, Deserialize,
        )]
        #[repr(transparent)]
        pub struct $name(pub $inner);

        impl $name {
            pub const ZERO: $name = $name(0);

            #[inline]
            #[must_use]
            pub const fn get(self) -> $inner {
                self.0
            }
        }
    };
}

scalar_newtype! {
    /// Pieniądz: zawsze `i64` w groszach, nigdy float (00 §2).
    /// Arytmetyka wyłącznie przez `core::money` — `checked_*`, `mul_ratio`,
    /// `div_round_half_up`, `split_proportional`.
    Money(i64)
}

scalar_newtype! {
    /// Minuty od startu świata. Tick ekonomiczny = 1 minuta gry.
    SimMinute(u64)
}

scalar_newtype! {
    /// Milisekundy czasu gry. Tick ruchu mikro = 100 ms (`time::MICRO_TICK_MS`).
    SimInstant(u64)
}

scalar_newtype! {
    /// Licznik ticków ekonomicznych, monotoniczny. Rośnie wyłącznie w `App::tick`.
    Tick(u64)
}

scalar_newtype! {
    /// Masa w gramach.
    Mass(i64)
}

scalar_newtype! {
    /// Objętość w mililitrach.
    Volume(i64)
}

scalar_newtype! {
    /// Energia w watogodzinach.
    Energy(i64)
}

scalar_newtype! {
    /// Ilość w milisztukach (sztuki × 1000).
    Qty(i64)
}

scalar_newtype! {
    /// Gęsty indeks dzielnicy.
    DistrictId(u16)
}

scalar_newtype! {
    /// Indeks do katalogu towarów, stabilny w obrębie wersji danych (00 §5).
    GoodId(u16)
}

scalar_newtype! {
    /// Indeks do katalogu receptur.
    RecipeId(u16)
}

scalar_newtype! {
    /// Indeks do katalogu kategorii potrzeb (`data/needs/categories.ron`), stabilny
    /// w obrębie wersji danych tak samo jak [`GoodId`].
    ///
    /// Kategoria jest **drobna** — „pieczywo", „paliwo silnikowe", „części maszyn" — i to
    /// ona odpowiada na pytanie „czym to podmienić", kiedy zabraknie towaru (PRD §8.4).
    /// Grubszy podział gospodarstwa domowego niesie [`crate::StockCat`] i to on indeksuje
    /// `Household.stock`; jedna kategoria potrzeby wpada w co najwyżej jeden `StockCat`,
    /// a odwzorowanie mieszka w danych, nie w kodzie.
    NeedCategoryId(u16)
}

scalar_newtype! {
    /// Indeks do katalogu ról zawodowych.
    JobRoleId(u16)
}

scalar_newtype! {
    /// Polityka w katalogu reguł — indeks nadawany przy ładowaniu `data/policies/`
    /// albo przez edytor gracza (M9d §5.6).
    ///
    /// Mieszka w `core`, bo jest **ładunkiem centralnego enuma**: `DecisionReason::Firm(FirmReason::PolicyApplied)`
    /// niesie go do karty inspekcji, a ładunek nie może pochodzić z crate'u, który od `core`
    /// zależy — ta sama reguła, która wypchnęła tu `PriceDriver` (`K-30`) i `WageCause` (`K-45`).
    /// Konsumenci: M7 (polityki firm AI), M9 (edytor reguł i dry-run), M12 (modding).
    PolicyId(u16)
}

scalar_newtype! {
    /// Złoże surowca pierwotnego — indeks w tablicy złóż świata (M1).
    ///
    /// Mieszka w `core`, bo ma **dwóch** konsumentów znanych z nazwy i numeru fazy
    /// (`K-8`): M1 generuje złoża i prowadzi ich bilans masy (`Deposit::extract`),
    /// a M6 przenosi identyfikator przez cały łańcuch w `BatchOrigin::deposit`, żeby
    /// panel „od pola do półki" mógł dojść do konkretnej żyły. Do M6d istniały **dwa**
    /// typy o tej nazwie — `sim/world::DepositId(u32)` i `sim/supply::DepositId(u16)` —
    /// bez konwersji między nimi, czyli ślad partii i tak nie prowadził do złoża,
    /// tylko do liczby, która przypadkiem wyglądała podobnie.
    DepositId(u32)
}

scalar_newtype! {
    /// Klasa taryfowa towaru w obrocie zagranicznym — indeks do `data/trade/tariffs.ron`.
    ///
    /// Mieszka w `core`, bo ma **dwóch** konsumentów znanych z nazwy i numeru fazy
    /// (`K-8`): M6 nadaje ją w `TradeGood` i przekazuje dalej, M8 odczytuje ją przy
    /// naliczaniu cła w `ChargeRegistry`. Sama klasa jest **etykietą**, nie stawką —
    /// stawka jest polityką miasta i epoki, a ta należy do M8. Dlatego cło **nie
    /// modyfikuje ceny w ofercie** (`K-7`): `ImportQuote` niesie cenę netto i osobno
    /// rozpisane obciążenia, nigdy kwotę „z cłem w środku".
    TariffClassId(u16)
}

scalar_newtype! {
    /// Uchwyt do zdarzenia świata w rejestrze `sim/events` (M8c §5.5).
    ///
    /// Mieszka w `core`, bo jest **ładunkiem** `DecisionReason::City(CityReason::EventStarted)` — tak samo
    /// jak `PriceDriver` (`K-30`) i z tego samego powodu: ładunek centralnego enuma
    /// nie może pochodzić z crate'u, który od `core` zależy. Drugim konsumentem znanym
    /// z nazwy i numeru fazy jest M9: `Subject::Event(EventId)` (`K-62`) robi z wpisu
    /// w kronice odnośnik do karty zdarzenia, a nie napis.
    ///
    /// Numer jest **monotoniczny w obrębie gry** i nigdy nie wraca: zdarzenie, które się
    /// skończyło, zostaje w kronice pod swoim numerem na zawsze.
    EventId(u32)
}

scalar_newtype! {
    /// Przetarg ogłoszony przez radę miasta (M8e §5.7).
    ///
    /// Mieszka w `core` z tego samego powodu co `EventId`: jest adresem karty
    /// inspekcji (`Subject::Tender`, `K-62`), a `core` nie może zależeć od `sim/city`.
    /// Numer jest monotoniczny w obrębie gry — rozstrzygnięty przetarg zostaje
    /// w kronice pod swoim numerem.
    TenderId(u32)
}

scalar_newtype! {
    /// Sprawa urzędowa prowadzona przez jeden z pięciu urzędów kontrolnych (M8d §5.6).
    CaseId(u32)
}

scalar_newtype! {
    /// Wniosek o pozwolenie złożony w urzędzie miasta (M8d §5.4).
    PermitId(u32)
}

scalar_newtype! {
    /// Marka — to, pod czym gracz i firma AI są znani mieszkańcom (PRD §7.6).
    ///
    /// Przeprowadzka z `sim/supply::batch` przy starcie M10b, wykonanie `K-8`.
    /// Do M10b typ mieszkał u partii, bo tylko ona go przenosiła; od M10b marka jest
    /// **wpisem w pamięci mieszkańca** (`magnat_agents::brand`), a `sim/agents` nie
    /// zależy i nie będzie zależał od `sim/supply` — zależność idzie w drugą stronę.
    /// Czterech konsumentów znanych z nazwy i numeru fazy: M6 (partia), M5 (półka
    /// i decyzja zakupowa), M3/M10 (sloty marek u mieszkańca), M10 (kampanie i media).
    ///
    /// **Marka jest firmą**: wartość to `FirmKey` rzutowany na `u16`
    /// (`magnat_firms::brand_of`). Osobny rejestr marek nie powstaje, dopóki nie ma
    /// drugiego konsumenta — linie produktowe należą do M10c.
    BrandId(u16)
}

scalar_newtype! {
    /// Kampania reklamowa w rejestrze `magnat_media` (M10b §5.2).
    ///
    /// Numer jest **monotoniczny w obrębie gry i nigdy nie wraca** — ta sama reguła
    /// co przy `EventId`: zakończona kampania zostaje w metrykach pod swoim numerem.
    CampaignId(u32)
}

scalar_newtype! {
    /// Węzeł drzewa technologii — indeks do `data/tech/tech.ron` (M10c §5.4, `K-82`).
    ///
    /// Indeks nadawany przy ładowaniu, w **stabilnej kolejności alfabetycznej klucza
    /// tekstowego**, tak samo jak [`GoodId`] i [`RecipeId`] (00 §5). W zapisie gry
    /// trzyma się klucz, nie indeks, więc dopisanie technologii nie psuje starych
    /// zapisów — to jest różnica wobec `JobRoleId` i klas pojazdów, gdzie indeks
    /// jest kontraktem.
    ///
    /// Mieszka w `core`, bo jest **ładunkiem centralnego enuma**: `DecisionReason::
    /// {ResearchStarted, TechDiscovered, LicenseSigned, ProductLaunched}` niosą go do
    /// karty inspekcji, a ładunek nie może pochodzić z crate'u, który od `core` zależy —
    /// ta sama reguła, która wypchnęła tu `PriceDriver` (`K-30`) i `WageCause` (`K-45`).
    TechId(u16)
}

scalar_newtype! {
    /// Polisa ubezpieczeniowa w rejestrze `magnat_economy::Insurers` (M10d §5.5).
    ///
    /// **Nazwa jest inna niż w planie fazy i to jest korekta, nie kaprys:** plan pisał
    /// `PolicyId`, a ten typ jest zajęty od `K-47` przez politykę silnika reguł
    /// i znaczy coś zupełnie innego. Dwa typy o jednej nazwie w jednym crate'cie nie
    /// mogą istnieć, a przemianowanie tamtego przepisałoby edytor reguł M9.
    ///
    /// Mieszka w `core`, bo jest **ładunkiem centralnego enuma**: `TxKind::
    /// {InsurancePremium, InsuranceClaim}` niosą go do dziennika transakcji, a dziennik
    /// wchodzi do zapisu gry. Numer jest monotoniczny w obrębie gry i nigdy nie wraca —
    /// wygasła polisa zostaje w historii szkodowości pod swoim numerem.
    CoverId(u32)
}

/// Skala 0..=100: jakość, zaspokojenie potrzeby, poziom umiejętności.
/// Konstruktor przycina do zakresu — wartość spoza skali nigdy nie powstaje.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize,
)]
#[repr(transparent)]
pub struct Q(u8);

impl Q {
    pub const MIN: Q = Q(0);
    pub const MAX: Q = Q(100);

    /// Przycina do 0..=100. Świadomie bez wariantu panikującego: dane wejściowe
    /// spoza skali są błędem kalibracji, nie błędem programu.
    #[inline]
    #[must_use]
    pub const fn new(v: u8) -> Q {
        Q(if v > 100 { 100 } else { v })
    }

    #[inline]
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Nasycone dodawanie w obrębie skali.
    #[inline]
    #[must_use]
    pub const fn saturating_add(self, rhs: u8) -> Q {
        Q::new(self.0.saturating_add(rhs))
    }

    #[inline]
    #[must_use]
    pub const fn saturating_sub(self, rhs: u8) -> Q {
        Q(self.0.saturating_sub(rhs))
    }
}

/// Nastrój w skali -100..=100.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize,
)]
#[repr(transparent)]
pub struct Mood(i8);

impl Mood {
    pub const MIN: Mood = Mood(-100);
    pub const NEUTRAL: Mood = Mood(0);
    pub const MAX: Mood = Mood(100);

    #[inline]
    #[must_use]
    pub const fn new(v: i8) -> Mood {
        Mood(if v > 100 {
            100
        } else if v < -100 {
            -100
        } else {
            v
        })
    }

    #[inline]
    #[must_use]
    pub const fn get(self) -> i8 {
        self.0
    }

    #[inline]
    #[must_use]
    pub const fn saturating_add(self, rhs: i8) -> Mood {
        Mood::new(self.0.saturating_add(rhs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn q_przycina_do_skali() {
        assert_eq!(Q::new(200), Q::MAX);
        assert_eq!(Q::new(0), Q::MIN);
        assert_eq!(Q::new(50).saturating_add(200), Q::MAX);
        assert_eq!(Q::new(3).saturating_sub(10), Q::MIN);
    }

    #[test]
    fn mood_przycina_obustronnie() {
        assert_eq!(Mood::new(-128), Mood::MIN);
        assert_eq!(Mood::new(127), Mood::MAX);
        assert_eq!(Mood::new(-100).saturating_add(-50), Mood::MIN);
    }

    #[test]
    fn rozmiary_typow_bazowych() {
        assert_eq!(size_of::<Money>(), 8);
        assert_eq!(size_of::<Q>(), 1);
        assert_eq!(size_of::<Mood>(), 1);
        assert_eq!(size_of::<GoodId>(), 2);
    }
}
