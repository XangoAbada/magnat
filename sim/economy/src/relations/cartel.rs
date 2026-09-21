//! Zmowa cenowa: cena minimalna, hazard wykrycia i skutki ujawnienia
//! (M10e WP10.13, PRD §7.9).
//!
//! ## Kartel nie ma własnego cennika
//!
//! Zmowa nie trzyma drugiej ceny obok ceny sklepu — **podnosi tę, która jest**,
//! przez `Market::set_price`, czyli przez jedyne wejście, którym cena w tej grze
//! się zmienia. Gdyby kartel miał własną tablicę cen, panel sklepu pokazywałby
//! jedną liczbę, decyzja zakupowa drugą, a gracz zobaczyłby cenę, której nikt
//! nie płaci. Spadek wolumenu bierze się z tego sam: droższa oferta wypada
//! z funkcji użyteczności mieszkańca i nikt jej nie kupuje.
//!
//! ## Kartel ginie od własnej chciwości, nie od kostki
//!
//! Hazard wykrycia ma cztery człony i **każdy jest funkcją zachowania zmowy**:
//! liczebności, odchylenia ceny od benchmarku, liczby niezadowolonych
//! wtajemniczonych i aktywności regulatora. Zmowa podnosząca cenę o pięć procent
//! żyje latami, zmowa podnosząca o połowę — kwartał. Bez tego mechanizm byłby
//! albo zawsze opłacalny, albo nigdy (ryzyko `R8` fazy).
//!
//! ## Podział z `sim/city`
//!
//! Wykrycie zapada tutaj, **sprawę prowadzi M8** (`K-10`): model zostawia fakt
//! w skrzynce [`Cartels::take_detected`], a `sim/city::law` zamienia go na sprawę
//! urzędu antymonopolowego z prawdziwymi dowodami, karą i terminem. W drugą stronę
//! idzie jedna liczba — aktywność regulatora ([`Cartels::set_enforcement_bp`]) —
//! bo obsada urzędów jest polityką miasta, a `sim/economy` jej nie widzi.

use std::collections::BTreeMap;

use magnat_core::{
    DecisionReason, DistrictId, FirmReason, GoodId, HashState, Money, StateHasher, Q,
};
use magnat_firms::FirmKey;
use smallvec::SmallVec;

use super::data::CartelParams;
use super::union::DetectedCartel;

/// Ceny minimalne ułożone po parze (towar, dzielnica): próg i skład zmowy.
pub type FloorIndex = BTreeMap<(u16, u16), (Money, SmallVec<[FirmKey; 8]>)>;

/// Numer zmowy — monotoniczny w obrębie gry i nigdy nie wracający.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct CartelId(pub u32);

/// Zmowa cenowa na jeden towar w jednej dzielnicy.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Cartel {
    pub id: CartelId,
    pub members: SmallVec<[FirmKey; 8]>,
    pub good: GoodId,
    pub district: DistrictId,
    /// Cena, poniżej której żaden członek nie schodzi.
    pub floor_price: Money,
    /// Mediana ceny **przed** zmową. To wobec niej mierzy się chciwość —
    /// nie wobec ceny bieżącej, bo tę kartel właśnie ustawił sam.
    pub benchmark: Money,
    pub formed_day: u32,
    pub secrecy: Q,
}

impl Cartel {
    /// O ile procent cena zmowy odstaje od benchmarku. To jest ta liczba,
    /// z której rośnie hazard.
    #[must_use]
    pub fn deviation_pct(&self) -> u32 {
        if self.benchmark.get() <= 0 {
            return 0;
        }
        let d = (self.floor_price.get() - self.benchmark.get()).max(0);
        u32::try_from(d.saturating_mul(100) / self.benchmark.get()).unwrap_or(u32::MAX)
    }

    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.id.0);
        h.write_u32(self.members.len() as u32);
        for m in &self.members {
            m.hash_state(h);
        }
        h.write_u16(self.good.0);
        self.district.hash_state(h);
        self.floor_price.hash_state(h);
        self.benchmark.hash_state(h);
        h.write_u32(self.formed_day);
        h.write_u8(self.secrecy.get());
    }
}

/// Miesięczny hazard wykrycia w częściach na milion (§5.9).
///
/// Cztery człony dodają się, a aktywność regulatora **mnoży** całość — bo urząd
/// bez inspektorów nie znajdzie nawet zmowy oczywistej, a urząd rozbudowany
/// znajdzie i tę dyskretną.
#[must_use]
pub fn detection_ppm(
    members: u32,
    deviation_pct: u32,
    unhappy_insiders: u32,
    enforcement_bp: u32,
    p: &CartelParams,
) -> u32 {
    let ponad = members.saturating_sub(u32::from(p.min_members));
    let suma = p
        .base_hazard_ppm
        .saturating_add(p.member_hazard_ppm.saturating_mul(ponad))
        .saturating_add(p.deviation_hazard_ppm.saturating_mul(deviation_pct) / 10)
        .saturating_add(p.insider_hazard_ppm.saturating_mul(unhappy_insiders));
    u32::try_from(u64::from(suma) * u64::from(enforcement_bp) / 10_000).unwrap_or(u32::MAX)
}

/// Zmowy miasta — zasób świata.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Cartels {
    list: BTreeMap<u32, Cartel>,
    /// Karencja per (towar, dzielnica): doba, od której wolno się zmówić znowu.
    cooldown: BTreeMap<(u16, u16), u32>,
    next_id: u32,
    /// Aktywność urzędu antymonopolowego w punktach bazowych; 10 000 = nominalna.
    /// Pisze `sim/city`, bo obsada urzędów jest polityką miasta.
    enforcement_bp: u32,
    /// Wykryte, czekające na urząd. Opróżnia je `sim/city::law`.
    detected: Vec<DetectedCartel>,
    /// Marki, które mają oberwać od skandalu — opróżnia je system tej podfazy,
    /// bo tylko on ma `&mut World` i widzi pamięć mieszkańców.
    scandals: Vec<(magnat_core::BrandId, u8)>,
}

impl Default for Cartels {
    fn default() -> Cartels {
        Cartels {
            list: BTreeMap::new(),
            cooldown: BTreeMap::new(),
            next_id: 1,
            enforcement_bp: 10_000,
            detected: Vec::new(),
            scandals: Vec::new(),
        }
    }
}

impl Cartels {
    #[must_use]
    pub fn new() -> Cartels {
        Cartels::default()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Cartel> {
        self.list.values()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.list.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Aktywność regulatora — wejście od `sim/city` (obsada urzędu antymonopolowego).
    pub fn set_enforcement_bp(&mut self, bp: u32) {
        self.enforcement_bp = bp.clamp(5_000, 20_000);
    }

    #[must_use]
    pub fn enforcement_bp(&self) -> u32 {
        self.enforcement_bp
    }

    /// Czy na tę parę wolno się dziś zmówić.
    #[must_use]
    pub fn allowed(&self, good: GoodId, district: DistrictId, day: u32) -> bool {
        self.cooldown
            .get(&(good.0, district.0))
            .is_none_or(|d| day >= *d)
    }

    /// Cena minimalna obowiązująca ten zakład, jeśli jego firma jest w zmowie.
    ///
    /// Przegląd liniowy — wołane raz na zakładanie zmowy, czyli rzadko. Dobowe
    /// pilnowanie ceny idzie przez [`Cartels::floors`], bo tam pytanie zadaje się
    /// raz na **każdą pozycję na każdej półce w mieście**.
    #[must_use]
    pub fn floor_for(&self, firm: FirmKey, good: GoodId, district: DistrictId) -> Option<Money> {
        self.list
            .values()
            .find(|c| c.good == good && c.district == district && c.members.contains(&firm))
            .map(|c| c.floor_price)
    }

    /// Ceny minimalne ułożone po parze (towar, dzielnica) — jedna zmowa na parę,
    /// bo przy zawiązywaniu odsiewa się firmy już zmówione na ten towar.
    ///
    /// Liczy się raz na dobę i zdejmuje z pilnowania cen mnożenie „półki razy
    /// zmowy": bez tej mapy każda pozycja na każdej półce przeglądałaby całą
    /// listę zmów, a obie rosną z wielkością miasta.
    #[must_use]
    pub fn floors(&self) -> FloorIndex {
        self.list
            .values()
            .map(|c| ((c.good.0, c.district.0), (c.floor_price, c.members.clone())))
            .collect()
    }

    /// Zawiązuje zmowę. Zwraca powód do dziennika decyzji każdego z członków.
    pub fn form(
        &mut self,
        members: SmallVec<[FirmKey; 8]>,
        good: GoodId,
        district: DistrictId,
        benchmark: Money,
        day: u32,
        p: &CartelParams,
    ) -> (CartelId, DecisionReason) {
        let id = CartelId(self.next_id);
        self.next_id += 1;
        let floor =
            Money(benchmark.get() * i64::from(10_000 + u32::from(p.floor_markup_bp)) / 10_000);
        let liczba = u8::try_from(members.len()).unwrap_or(u8::MAX);
        self.list.insert(
            id.0,
            Cartel {
                id,
                members,
                good,
                district,
                floor_price: floor,
                benchmark,
                formed_day: day,
                // Tajemnica jest tym słabsza, im więcej ust ją zna.
                secrecy: Q::new(100u8.saturating_sub(liczba.saturating_mul(5))),
            },
        );
        (
            id,
            DecisionReason::Firm(FirmReason::CartelFormed {
                good,
                members: liczba,
                floor,
            }),
        )
    }

    /// Ujawnia zmowę: rozwiązuje ją, nakłada karencję i zostawia ślad dla urzędu
    /// oraz dla pamięci marek. Zwraca powód do dziennika i listę członków.
    pub fn bust(
        &mut self,
        id: CartelId,
        day: u32,
        p: &CartelParams,
    ) -> Option<(DecisionReason, SmallVec<[FirmKey; 8]>)> {
        let c = self.list.remove(&id.0)?;
        let miesiecy = u16::try_from((day.saturating_sub(c.formed_day)) / 30).unwrap_or(u16::MAX);
        self.cooldown.insert(
            (c.good.0, c.district.0),
            day + u32::from(p.cooldown_months) * 30,
        );
        Some((
            DecisionReason::Firm(FirmReason::CartelDetected {
                good: c.good,
                members: u8::try_from(c.members.len()).unwrap_or(u8::MAX),
                months: miesiecy,
            }),
            c.members,
        ))
    }

    /// Dopisuje sprawę do przekazania urzędowi (`K-10`).
    pub fn report(&mut self, d: DetectedCartel) {
        self.detected.push(d);
    }

    /// Odbiera sprawy. Opróżnia skrzynkę — odebranie dwa razy otworzyłoby
    /// dwie sprawy o jedną zmowę.
    pub fn take_detected(&mut self) -> Vec<DetectedCartel> {
        std::mem::take(&mut self.detected)
    }

    pub fn note_scandal(&mut self, brand: magnat_core::BrandId, drop: u8) {
        self.scandals.push((brand, drop));
    }

    pub fn take_scandals(&mut self) -> Vec<(magnat_core::BrandId, u8)> {
        std::mem::take(&mut self.scandals)
    }

    /// Kasuje z zmowy firmę, która przestała istnieć, i rozwiązuje zmowę, w której
    /// zostało mniej niż dwóch — zmowa jednego to nie jest zmowa.
    pub fn drop_member(&mut self, firm: FirmKey) {
        for c in self.list.values_mut() {
            c.members.retain(|m| *m != firm);
        }
        self.list.retain(|_, c| c.members.len() >= 2);
    }
}

impl HashState for Cartels {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.list.len() as u32);
        for (k, c) in &self.list {
            h.write_u32(*k);
            c.hash_state(h);
        }
        h.write_u32(self.cooldown.len() as u32);
        for ((g, d), until) in &self.cooldown {
            h.write_u16(*g);
            h.write_u16(*d);
            h.write_u32(*until);
        }
        h.write_u32(self.next_id);
        h.write_u32(self.enforcement_bp);
        // Nieodebrane zgłoszenia i skandale są tym, co świat ma do przekazania
        // w następnym kroku — ta sama zasada, co przy wezwaniach do strajku.
        h.write_u32(self.detected.len() as u32);
        for d in &self.detected {
            d.site.entity().hash_state(h);
            h.write_u8(d.evidence.get());
        }
        h.write_u32(self.scandals.len() as u32);
        for (b, drop) in &self.scandals {
            h.write_u16(b.0);
            h.write_u8(*drop);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p() -> CartelParams {
        super::super::data::RelationsTuning::default().cartel
    }

    #[test]
    fn hazard_rosnie_z_liczba_czlonkow_i_z_chciwoscia() {
        let p = p();
        let maly = detection_ppm(3, 5, 0, 10_000, &p);
        let wiekszy = detection_ppm(6, 5, 0, 10_000, &p);
        let chciwy = detection_ppm(3, 50, 0, 10_000, &p);
        assert!(wiekszy > maly, "każdy członek to dodatkowe usta");
        assert!(chciwy > maly, "im więcej kradniesz, tym bardziej widać");
        assert!(
            detection_ppm(3, 5, 2, 10_000, &p) > maly,
            "niezadowolony wtajemniczony podnosi ryzyko"
        );
        assert!(
            detection_ppm(3, 5, 0, 20_000, &p) > maly,
            "silniejszy regulator znajduje częściej"
        );
    }

    #[test]
    fn hazard_jest_monotoniczny_po_odchyleniu() {
        let p = p();
        let mut poprzedni = 0;
        for dev in 0..60 {
            let h = detection_ppm(4, dev, 1, 10_000, &p);
            assert!(h >= poprzedni, "hazard spadł przy odchyleniu {dev}");
            poprzedni = h;
        }
    }

    #[test]
    fn zmowa_podnosi_cene_i_pamieta_punkt_wyjscia() {
        let mut c = Cartels::new();
        let czlonkowie: SmallVec<[FirmKey; 8]> =
            SmallVec::from_slice(&[FirmKey(1), FirmKey(2), FirmKey(3)]);
        let (id, powod) = c.form(czlonkowie, GoodId(4), DistrictId(0), Money(1_000), 10, &p());
        assert!(matches!(
            powod,
            DecisionReason::Firm(FirmReason::CartelFormed { members: 3, .. })
        ));
        let k = c.iter().next().expect("zmowa");
        assert_eq!(k.floor_price, Money(1_150), "cena o 15 % ponad medianę");
        assert_eq!(k.deviation_pct(), 15);
        assert_eq!(
            c.floor_for(FirmKey(2), GoodId(4), DistrictId(0)),
            Some(Money(1_150))
        );
        assert_eq!(c.floor_for(FirmKey(9), GoodId(4), DistrictId(0)), None);

        // Ujawnienie rozwiązuje zmowę i zamyka rynek na dwa lata.
        let (powod, czlonkowie) = c.bust(id, 10 + 30 * 7, &p()).expect("wykryta");
        assert!(matches!(
            powod,
            DecisionReason::Firm(FirmReason::CartelDetected { months: 7, .. })
        ));
        assert_eq!(czlonkowie.len(), 3);
        assert!(c.is_empty());
        assert!(!c.allowed(GoodId(4), DistrictId(0), 10 + 30 * 7));
        assert!(c.allowed(GoodId(4), DistrictId(0), 10 + 30 * 31));
    }

    #[test]
    fn zmowa_dwoch_przestaje_byc_zmowa_po_odejsciu_trzeciego() {
        let mut c = Cartels::new();
        c.form(
            SmallVec::from_slice(&[FirmKey(1), FirmKey(2), FirmKey(3)]),
            GoodId(4),
            DistrictId(0),
            Money(1_000),
            0,
            &p(),
        );
        c.drop_member(FirmKey(3));
        assert_eq!(c.len(), 1, "dwóch to nadal zmowa");
        c.drop_member(FirmKey(2));
        assert!(c.is_empty(), "jeden nie zmawia się z nikim");
    }
}
