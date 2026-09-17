//! Osobowość i kurs firmy (M7e WP11/WP12, M7 §5.7, PRD §12.1).
//!
//! **Osobowość firmy jest wywiedziona z dyrektora-mieszkańca**, a nie wylosowana
//! firmie. To jest cała treść §12.1: zmiana dyrektora — śmierć, dziedziczenie,
//! sprzedaż firmy — zmienia zachowanie firmy, bo [`personality_from_director`]
//! jest funkcją czystą jego cech. Nie ma tu stanu, który mógłby się rozjechać
//! z mieszkańcem.
//!
//! Firma **bez** dyrektora (sieć zewnętrzna, zarząd tymczasowy po śmierci bez
//! spadkobiercy — `D15`) dostaje cechy z [`FirmPersonality::draw`]: losowanie
//! jednorazowe, ze strumienia `StreamId::FirmPersonality` i z klucza firmy. Nie jest
//! to obejście reguły, tylko jej druga połowa — miasto stawiane przez generator ma
//! dziś same firmy zewnętrzne (`Owner::External`), a dwa zakłady różniące się
//! wyłącznie osobowością mają się różnić tym, kogo zatrudnią i za ile, **już teraz**,
//! a nie dopiero wtedy, gdy M7f dołoży właścicieli-mieszkańców.
//!
//! ## Czego tu nie ma
//!
//! Osobowości **cenowej** — `magnat_economy::FirmPricing` istnieje od M5c, losuje się
//! z tego samego ziarna i opisuje widełki marży, czułość na zapas i czujność wobec
//! konkurencji. Drugiego opisu tego samego nie robimy; tier operacyjny M7e przesuwa
//! **cel** marży w ramach tamtych widełek, a nie zastępuje ich własnymi.

use magnat_agents::Personality;
use magnat_core::hash::{HashState, StateHasher};
use magnat_core::rng::{rng, StreamId};
use magnat_core::{FirmStrategy, Tick, TraitId, Q};

use crate::key::FirmKey;

/// Siedem cech firmy, wszystkie 0..=100 (M7 §5.7).
///
/// `50` jest punktem neutralnym każdej z nich i to nie jest przypadek: cecha bez
/// punktu neutralnego nie jest cechą, tylko cichym przesunięciem skali — ta sama
/// nauka, którą M7a zapisał przy `tech_mult` w `hr::productivity`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FirmPersonality {
    /// Ile firma dokłada w licytacji o pracownika i jak głęboko schodzi w wojnie cenowej.
    pub aggression: u8,
    /// Skłonność do dźwigni i do otwierania zakładu przy niepewnym popycie.
    pub risk_tolerance: u8,
    /// Ile firma dopłaca za lepszego kandydata i za wyższą marżę jakościową.
    pub quality_focus: u8,
    /// Ile firma oszczędza na stawce i jak chętnie schodzi z ceną.
    pub price_focus: u8,
    pub innovation: u8,
    /// Ile miesięcy straty firma zniesie, zanim zamknie zakład.
    pub patience: u8,
    /// Opór przed zwolnieniami i premia za staż.
    pub staff_loyalty: u8,
}

impl FirmPersonality {
    pub const NEUTRAL: FirmPersonality = FirmPersonality {
        aggression: 50,
        risk_tolerance: 50,
        quality_focus: 50,
        price_focus: 50,
        innovation: 50,
        patience: 50,
        staff_loyalty: 50,
    };

    /// Cechy firmy bez dyrektora — funkcja czysta ziarna świata i klucza firmy.
    ///
    /// Tick zero w kluczu losowania jest celowy: osobowość ma być **stała przez życie
    /// firmy**, więc nie wolno jej wiązać z chwilą, w której ktoś o nią zapytał.
    #[must_use]
    pub fn draw(world_seed: u64, key: FirmKey) -> FirmPersonality {
        let mut r = rng(world_seed, StreamId::FirmPersonality, key.0 as u32, Tick(0));
        // Rozkład trójkątny wokół 50 zamiast płaskiego: firm skrajnych ma być mało,
        // inaczej miasto składa się z samych ekstremistów i różnica przestaje być
        // czytelna. Dwa losowania po 0..=50 sumują się do 0..=100 z wierzchołkiem w 50.
        let mut cecha = || (r.gen_range_u32(51) + r.gen_range_u32(51)) as u8;
        FirmPersonality {
            aggression: cecha(),
            risk_tolerance: cecha(),
            quality_focus: cecha(),
            price_focus: cecha(),
            innovation: cecha(),
            patience: cecha(),
            staff_loyalty: cecha(),
        }
    }

    /// Kurs, na którym firma stoi domyślnie — z cech, nie z losowania (M7 §5.7).
    ///
    /// Kolejność sprawdzeń jest kolejnością **wyrazistości**: firma, która jest
    /// jednocześnie tania i innowacyjna, jest przede wszystkim tania, bo to widzi
    /// klient. Pierwsze pasujące wygrywa — ta sama reguła co w ewaluatorze polityk.
    #[must_use]
    pub fn strategy(&self) -> FirmStrategy {
        let p = i32::from(self.price_focus);
        let q = i32::from(self.quality_focus);
        if p - q >= 20 {
            return FirmStrategy::Discount;
        }
        if q - p >= 20 {
            return FirmStrategy::NicheQuality;
        }
        if i32::from(self.aggression) + i32::from(self.risk_tolerance) >= 140 {
            return FirmStrategy::AggressiveExpansion;
        }
        if self.innovation >= 70 {
            return FirmStrategy::Innovative;
        }
        if self.patience >= 70 && self.aggression <= 40 {
            return FirmStrategy::Consolidator;
        }
        FirmStrategy::Cautious
    }

    /// Wagi wyboru kandydata (M7b `AV-6`). Obie w punktach procentowych **dodawanych**
    /// do wagi bazowej, więc firma neutralna daje `HiringPolicy::NEUTRAL`.
    ///
    /// Skala jest połową odchylenia od 50: cecha 100 daje +25 punktów procentowych
    /// do wagi, cecha 0 nie odejmuje nic. Ujemna waga odwracałaby znak scoringu —
    /// firma szukałaby najgorszego kandydata, a to nie jest osobowość, tylko błąd.
    #[must_use]
    pub fn hiring(&self) -> crate::labor_policy::HiringPolicy {
        crate::labor_policy::HiringPolicy {
            quality_focus: nadwyzka(self.quality_focus),
            price_focus: nadwyzka(self.price_focus),
        }
    }

    /// Agresja licytacyjna zakładu bez menedżera (M7b `AV-6`).
    ///
    /// Do M7e zakład bez menedżera licytował z agresją zero — czyli krokiem bazowym,
    /// tym samym w całym mieście. Od M7e ma agresję swojej firmy, a menedżer,
    /// jeśli jest, nadal ją nadpisuje: to on prowadzi ten zakład.
    #[must_use]
    pub const fn aggression_bp(&self) -> u8 {
        self.aggression
    }

    /// Ile miesięcy nieprzerwanej straty firma zniesie, zanim zamknie zakład (M7e WP12).
    ///
    /// **Dwie wartości, nie skala, i sufit przy trzech miesiącach — to jest kryterium
    /// ukończenia WP12, a nie kalibracja.** „Firma z trwale nierentownym zakładem
    /// zamyka go w ≤ 3 miesiące gry" przestaje być prawdą, gdy tylko cierpliwość
    /// zacznie rozciągać ten próg dalej; rozpiętość osobowości ma się objawiać w cenie
    /// i w płacy, a nie w obchodzeniu kryterium fazy.
    ///
    /// Dolna granica to dwa miesiące, bo jeden zły miesiąc nie jest trwałą stratą —
    /// jest sezonem, awarią linii albo dostawą, która przyszła dzień po terminie.
    #[must_use]
    pub const fn loss_patience_months(&self) -> u8 {
        if self.patience >= 50 {
            3
        } else {
            2
        }
    }
}

impl Default for FirmPersonality {
    fn default() -> FirmPersonality {
        FirmPersonality::NEUTRAL
    }
}

impl HashState for FirmPersonality {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(self.aggression);
        h.write_u8(self.risk_tolerance);
        h.write_u8(self.quality_focus);
        h.write_u8(self.price_focus);
        h.write_u8(self.innovation);
        h.write_u8(self.patience);
        h.write_u8(self.staff_loyalty);
    }
}

/// Połowa nadwyżki cechy ponad punkt neutralny, 0..=25.
const fn nadwyzka(v: u8) -> u8 {
    if v <= 50 {
        0
    } else {
        (v - 50) / 2
    }
}

/// **Osobowość firmy JEST wywiedziona z dyrektora-mieszkańca** (M7 §5.7, PRD §12.1).
///
/// Funkcja czysta jego cech i pozycji społecznej — nie ma tu ani losowania, ani stanu,
/// więc zmiana dyrektora zmienia firmę natychmiast i w całości.
///
/// Odwzorowanie cech idzie wprost za §5.7:
/// ambicja i skłonność do ryzyka → agresja i tolerancja ryzyka;
/// oszczędność → nacisk na cenę;
/// sumienność → nacisk na jakość i cierpliwość;
/// otwartość → innowacyjność;
/// lojalność → lojalność wobec załogi.
///
/// `status` podnosi wyłącznie tolerancję ryzyka i podnosi ją słabo: dyrektor z pozycją
/// ma poduszkę, na którą może upaść, ale pozycja nie czyni go ani tańszym, ani bardziej
/// pomysłowym. Gdyby wchodziła do każdej cechy, byłaby drugą skalą zamożności ukrytą
/// w osobowości.
#[must_use]
pub fn personality_from_director(p: &Personality, status: Q) -> FirmPersonality {
    let ambicja = i32::from(p.get(TraitId::Ambition).get());
    let ryzyko = i32::from(p.get(TraitId::Risk).get());
    let oszczednosc = i32::from(p.get(TraitId::Thrift).get());
    let sumiennosc = i32::from(p.get(TraitId::Conscientiousness).get());
    let otwartosc = i32::from(p.get(TraitId::Openness).get());
    let lojalnosc = i32::from(p.get(TraitId::Loyalty).get());
    let poduszka = (i32::from(status.get()) - 50) / 5;
    FirmPersonality {
        aggression: skala((ambicja * 2 + ryzyko) / 3),
        risk_tolerance: skala(ryzyko + poduszka),
        quality_focus: skala(sumiennosc),
        price_focus: skala(oszczednosc),
        innovation: skala(otwartosc),
        patience: skala((sumiennosc + (100 - ryzyko)) / 2),
        staff_loyalty: skala(lojalnosc),
    }
}

const fn skala(v: i32) -> u8 {
    if v < 0 {
        0
    } else if v > 100 {
        100
    } else {
        v as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dyrektor(ambicja: u8, ryzyko: u8, oszczednosc: u8, sumiennosc: u8) -> Personality {
        let mut p = Personality::default();
        p.set(TraitId::Ambition, Q::new(ambicja));
        p.set(TraitId::Risk, Q::new(ryzyko));
        p.set(TraitId::Thrift, Q::new(oszczednosc));
        p.set(TraitId::Conscientiousness, Q::new(sumiennosc));
        p
    }

    #[test]
    fn osobowosc_firmy_jest_funkcja_czysta_dyrektora() {
        let d = dyrektor(80, 70, 30, 60);
        let a = personality_from_director(&d, Q::new(50));
        let b = personality_from_director(&d, Q::new(50));
        assert_eq!(a, b);
        // Ambicja waży w agresji podwójnie wobec ryzyka — dyrektor ambitny
        // i ostrożny nadal licytuje ostro.
        assert!(a.aggression > 70, "agresja {}", a.aggression);
    }

    #[test]
    fn zmiana_dyrektora_zmienia_firme() {
        let ostrozny = personality_from_director(&dyrektor(20, 10, 80, 80), Q::new(50));
        let ryzykant = personality_from_director(&dyrektor(90, 90, 20, 30), Q::new(50));
        assert!(ryzykant.aggression > ostrozny.aggression);
        assert!(ryzykant.risk_tolerance > ostrozny.risk_tolerance);
        assert!(ostrozny.price_focus > ryzykant.price_focus);
        assert!(ostrozny.patience > ryzykant.patience);
    }

    #[test]
    fn kurs_wychodzi_z_cech_a_nie_z_losowania() {
        let mut tani = FirmPersonality::NEUTRAL;
        tani.price_focus = 90;
        tani.quality_focus = 30;
        assert_eq!(tani.strategy(), FirmStrategy::Discount);

        let mut niszowy = FirmPersonality::NEUTRAL;
        niszowy.quality_focus = 90;
        niszowy.price_focus = 20;
        assert_eq!(niszowy.strategy(), FirmStrategy::NicheQuality);

        let mut zdobywca = FirmPersonality::NEUTRAL;
        zdobywca.aggression = 80;
        zdobywca.risk_tolerance = 80;
        assert_eq!(zdobywca.strategy(), FirmStrategy::AggressiveExpansion);

        assert_eq!(FirmPersonality::NEUTRAL.strategy(), FirmStrategy::Cautious);
    }

    #[test]
    fn losowanie_jest_stale_i_rozne_miedzy_firmami() {
        let a = FirmPersonality::draw(7, FirmKey(1));
        assert_eq!(a, FirmPersonality::draw(7, FirmKey(1)), "nie jest stałe");
        assert_ne!(a, FirmPersonality::draw(7, FirmKey(2)), "wszyscy tacy sami");
        // Inne ziarno świata to inne miasto — i inne firmy w nim.
        assert_ne!(a, FirmPersonality::draw(8, FirmKey(1)));
    }

    #[test]
    fn rozklad_losowania_ma_wierzcholek_w_srodku() {
        // Trójkątny, nie płaski: skrajnych firm ma być mało. Sprawdzamy to tam,
        // gdzie różnica jest widoczna — w liczbie firm bardzo agresywnych.
        let mut skrajne = 0u32;
        let mut srodek = 0u32;
        for i in 1..=10_000u64 {
            let a = FirmPersonality::draw(1, FirmKey(i)).aggression;
            if a >= 90 {
                skrajne += 1;
            }
            if (40..=60).contains(&a) {
                srodek += 1;
            }
        }
        assert!(srodek > skrajne * 4, "środek {srodek}, skrajne {skrajne}");
    }

    #[test]
    fn wagi_wyboru_kandydata_nigdy_nie_odwracaja_scoringu() {
        for v in 0..=100u8 {
            let mut p = FirmPersonality::NEUTRAL;
            p.quality_focus = v;
            p.price_focus = v;
            let h = p.hiring();
            assert!(h.quality_focus <= 25 && h.price_focus <= 25);
        }
        assert_eq!(
            FirmPersonality::NEUTRAL.hiring(),
            crate::labor_policy::HiringPolicy::NEUTRAL
        );
    }

    #[test]
    fn cierpliwosc_miesci_sie_w_kryterium_wp12() {
        // Kryterium: zakład trwale nierentowny zamyka się w ≤ 3 miesiące od chwili,
        // w której strata stała się trwała. Sufit cierpliwości nie może go przebić.
        for v in 0..=100u8 {
            let mut p = FirmPersonality::NEUTRAL;
            p.patience = v;
            let m = p.loss_patience_months();
            assert!((2..=3).contains(&m), "cierpliwość {v} → {m} miesięcy");
        }
    }
}
