//! RNG ze strumieniami (00 §3.1, PRD §18.2).
//!
//! Brak globalnego stanu. Generator jest wyprowadzany **czystą funkcją czterech
//! argumentów** i żyje tylko w obrębie jednego wywołania systemu dla jednej encji.
//! Dzięki temu wynik nie zależy od kolejności wywołań ani od liczby wątków — dołożenie
//! systemu w M7 nie przesuwa sekwencji widzianej przez system z M5.

use crate::types::{Tick, Q};
use serde::{Deserialize, Serialize};

/// Strumień RNG. Wartości liczbowe są **wieczne**: faza dopisuje warianty w swoim
/// zakresie, nigdy nie zmienia i nie usuwa istniejących (00 §3.1, §K-4).
/// Zmiana numeru strumienia zmienia każdy świat wygenerowany wcześniej z tego samego seeda.
///
/// Siatka zakresów (00 §K-4): M0 0–19, M1 100–119, M2 120–139, M3 140–159, M4 160–179,
/// M5 180–199, M6 200–219, M7 220–239, M8 240–259, M9 260–279, M10 280–299, M11 300–319,
/// M12 320–339. Fazie, której 20 wartości nie wystarcza, przysługuje blok `baza + 1000`.
/// Zakresy 20–99 i 340–999 pozostają wolne.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[repr(u16)]
pub enum StreamId {
    // ── M0: 0..=19 ───────────────────────────────────────────────────────────────
    // 0 zarezerwowane — brak strumienia; użycie to błąd, nie wartość domyślna.
    /// Testy silnika i świat syntetyczny `tools/headless`. Poza silnikiem nieużywany.
    EngineSelfTest = 1,

    // ── M1: 100..=119 ── generator świata; jeden strumień na przebieg potoku,
    // żeby dopisanie przebiegu nie przesunęło losowań pozostałych (M1 §5.6).
    /// P1 — maska lądu i poziom morza.
    WorldLandmask = 100,
    /// P2 — baza wysokości (fBm / ridged multifractal).
    WorldHeightBase = 101,
    /// P2 — pole zniekształcenia dziedziny (domain warping).
    WorldDomainWarp = 102,
    /// P3 — pole wypiętrzenia `U` dla stream-power.
    WorldUplift = 103,
    /// P3 — pole podatności na erozję `K` (twarde intruzje → ostańce).
    WorldErodibility = 104,
    /// P8 — modulacja miąższości warstw geologicznych.
    WorldGeology = 105,
    /// P9 — rozmieszczenie, typ i kształt złóż.
    WorldDeposits = 106,
    /// P10 — temperatura bazowa i jej lokalna zmienność.
    WorldClimate = 107,
    /// P11 — wiatr dominujący i adwekcja wilgoci.
    WorldWind = 108,
    /// P12 — klasyfikacja biomu i żyzność gleby.
    WorldBiome = 109,
    /// P13 — szum detalu przy materializacji 1 m.
    WorldDetail = 110,
    // 111–119 zarezerwowane dla M1.

    // ── M2: 120..=139 ── generator miasta; wartości imiennie z M2 §6 i niezmienne.
    /// Wybór miejsca bramy wśród trzech najlepszych kandydatów (M2 §5.2).
    Gates = 120,
    /// L-system arterii — klucz `seq` propozycji (M2 §5.2).
    RoadsL = 121,
    /// Scalanie i rozjazdy kolei towarowej (M2 §5.2, WP5b).
    RoadRail = 122,
    /// Podział kwartału OBB — punkt cięcia (M2 §5.4).
    Blocks = 123,
    /// Szum punktacji i remisy przydziału kwotowego stref (M2 §5.3).
    Zoning = 124,
    /// Rozrost footprintu per epoka (M2 §5.3).
    EpochRings = 125,
    /// Próbkowanie Poissona zalążków dzielnic (M2 §5.5).
    Districts = 126,
    /// Nazwy dzielnic z szablonów (M2 §5.5).
    Naming = 127,
    /// Szerokości frontów w podziale pasowym (M2 §5.4).
    Parcels = 128,
    /// Wybór gramatyki dla parceli (M2 §5.6, M2d).
    BuildingPick = 129,
    /// Derywacja gramatyki — klucz `(building_index, node_index)` (M2 §5.6, M2d).
    BuildingGrammar = 130,
    /// Podział kondygnacji na lokale, liczba pokoi (M2 §5.6, M2d).
    Interiors = 131,
    /// Wybór szablonu łańcucha i nazwy firm (M2 §5.8, M2e).
    FirmSeed = 132,
    /// Remisy przy sadzeniu zakładów i placówek handlu (M2 §5.8, M2e).
    SitePlacement = 133,
    // 134–139 zarezerwowane dla M2.

    // ── M3: 140..=159 ── ludzie i dzień; wartości imiennie z M3 §6.1 i niezmienne.
    /// Planer dnia i przeplanowania — klucz `(citizen_idx, day * 1440 + minute)` (M3b §5.4).
    DayPlan = 140,
    /// Hazardy demograficzne: narodziny, choroba, zgon, dobór partnera (M3c §5.6).
    Demography = 141,
    /// Propagacja plotki po grafie relacji (M3c §5.8).
    Gossip = 142,
    /// Generacja populacji — Etap 8 (M3d §5.9).
    PopGen = 143,
    /// Losowanie cech osobowości przy tworzeniu mieszkańca (M3d §5.9).
    PersonalityGen = 144,
    /// Napływ i odpływ gospodarstw domowych (M3c §5.7).
    Migration = 145,
    /// Zawiązywanie i wygasanie relacji (M3c §5.8).
    Relations = 146,
    // 147–159 zarezerwowane dla M3; rezerwa dalsza 1140–1159 (K-4).

    // ── M4: 160..=179 ── ruch; wartości imiennie z M4 §6 i niezmienne.
    /// Wybór środka transportu — klucz `(citizen_idx, minuta odjazdu)` (M4 §5.3).
    /// M4b używa go do remisów w regule zastępczej, M4c do pełnego `evaluate_modes`.
    ModeChoice = 160,
    /// Wybór stacji paliw spośród kandydatów w korytarzu trasy (M4b §5.7).
    FuelStationChoice = 161,
    /// Obsadzenie gospodarstw pojazdami przy generacji świata (M4b, WP5).
    VehicleSeed = 162,
    /// Wybór miejsca parkingowego spośród kandydatów w promieniu dojścia (M4c §5.5).
    ParkingSearch = 163,
    /// Rozrzut postoju na przystanku wokół wartości z rozkładu (M4c §5.6).
    TransitDwell = 164,
    /// Awaria pojazdu w trasie (M4d).
    VehicleBreakdown = 165,
    /// Pogoda doby — **zaślepka `D4`**, właścicielem docelowym jest M8. Numer zostaje
    /// przy M4, bo to M4 wnosi mechanizm i to jego strumień wchodzi do zapisów
    /// wygenerowanych przed M8; podmiana ciała `weather_at` numeru nie ruszy.
    WeatherStub = 166,
    // 167–179 zarezerwowane dla M4; rezerwa dalsza 1160–1179 (K-4).

    // ── M5: 180..=199 — gospodarka detaliczna ───────────────────────────────────
    /// Szum funkcji użyteczności zakupu, stały dla trójki (kupujący, oferta, tick)
    /// (M5b §5.4). Bez niego dwie identyczne oferty dawałyby zawsze ten sam wybór
    /// i rynek degenerowałby się do monopolu przy pierwszym remisie.
    PurchaseNoise = 180,
    /// Losowanie oferty z rozkładu softmax (M5b §5.4) — **nie** argmax, bo argmax
    /// produkuje monopole (PRD §6.4, bramka G6 na HHI).
    PurchaseChoice = 181,
    /// Eksperyment cenowy sklepu AI (M5c §5.6).
    PriceExperiment = 182,
    /// Opóźnienie obserwacji konkurenta, 1–7 dni (M5c §5.6).
    CompetitorDelay = 183,
    /// Dryf ceny dostawcy zewnętrznego, kluczem jest doba (M5b §5.7).
    ExternalPriceDrift = 184,
    /// Rozrzut scoringu kredytowego (M5d §5.10).
    CreditScoringJitter = 185,
    /// Osobowość cenowa firmy handlowej — czułość na zapas i na konkurencję, widełki
    /// marży, skłonność do eksperymentu (M5c §5.6). Klucz: indeks firmy, tick 0, więc
    /// parametry są stałe przez całe życie firmy. W M7 zastąpi to pełna osobowość
    /// właściciela; numer strumienia zostaje, bo niesie go każdy zapis sprzed M7.
    FirmPricing = 186,
    // 187–199 zarezerwowane dla M5 (m.in. rynek pracy od M7); rezerwa dalsza 1180–1199.
}

/// Encja zastępcza dla losowania bez encji (zdarzenie globalne, generator świata).
/// System, który jej potrzebuje, podaje właśnie tę wartość — nie wymyśla własnego
/// mechanizmu (M0 §5.2).
pub const NO_ENTITY: u32 = u32::MAX;

/// xoshiro256++ — stan 256-bitowy, okres 2^256−1.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Rng {
    s: [u64; 4],
}

/// Mieszalnik SplitMix64 — rozprasza słabe ziarna (np. `world_seed = 1`), zanim trafią
/// do xoshiro. Bez tego kolejne encje z sąsiednimi indeksami dają skorelowane sekwencje.
///
/// **Publiczny od M5b.** Losowanie związane z *parą* (kupujący, oferta) nie da się
/// wyrazić przez `rng(seed, stream, entity, tick)`, bo ta funkcja przyjmuje jedną
/// encję — a szum funkcji użyteczności musi być stały dla pary i **niezależny
/// od kolejności, w jakiej kandydaci zostali policzeni** (M5b §5.4). Zamiast
/// drugiego mieszalnika w `sim/economy` udostępniamy ten sam, który już rozprasza
/// ziarna: jeden mieszalnik w grze, tak samo jak jeden generator.
#[inline]
#[must_use]
pub const fn mix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Jedyny sposób utworzenia generatora. Czysta funkcja czterech argumentów (00 §3.1).
#[must_use]
pub fn rng(world_seed: u64, stream: StreamId, entity_index: u32, tick: Tick) -> Rng {
    let mut acc = world_seed;
    acc = mix64(acc ^ (stream as u64).wrapping_mul(0x2545_F491_4F6C_DD1D));
    acc = mix64(acc ^ u64::from(entity_index));
    acc = mix64(acc ^ tick.0);

    let mut s = [0u64; 4];
    for slot in &mut s {
        acc = acc.wrapping_add(0x9E37_79B9_7F4A_7C15);
        *slot = mix64(acc);
    }
    // Stan zerowy jest jedynym punktem stałym xoshiro. Praktycznie nieosiągalny,
    // ale „praktycznie" nie jest gwarancją, a koszt zabezpieczenia to jedna gałąź.
    if s == [0; 4] {
        s[0] = 1;
    }
    Rng { s }
}

impl Rng {
    /// Generator o zadanym stanie — wyłącznie do wektorów testowych i odtwarzania
    /// sekwencji w narzędziach. Kod symulacji tworzy generator przez `rng()`.
    #[must_use]
    pub const fn from_state(s: [u64; 4]) -> Rng {
        Rng { s }
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[0]
            .wrapping_add(self.s[3])
            .rotate_left(23)
            .wrapping_add(self.s[0]);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// Starsze 32 bity — u xoshiro++ są lepszej jakości niż młodsze.
    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Losowa liczba z `0..end_exclusive`, **bez obciążenia modulo**
    /// (metoda Lemire'a z odrzucaniem). Panika przy `end_exclusive == 0`.
    pub fn gen_range_u32(&mut self, end_exclusive: u32) -> u32 {
        assert!(end_exclusive > 0, "gen_range_u32: pusty zakres");
        let bound = u64::from(end_exclusive);
        let mut m = u64::from(self.next_u32()) * bound;
        let mut low = m as u32;
        if low < end_exclusive {
            // 2^32 mod bound, liczone bez 64-bitowego dzielenia.
            let threshold = end_exclusive.wrapping_neg() % end_exclusive;
            while low < threshold {
                m = u64::from(self.next_u32()) * bound;
                low = m as u32;
            }
        }
        (m >> 32) as u32
    }

    /// Wartość w skali 0..=100.
    #[inline]
    pub fn gen_q(&mut self) -> Q {
        Q::new(self.gen_range_u32(101) as u8)
    }

    /// Prawda z prawdopodobieństwem `p/1000`. Promile, bo procent bywa za gruby
    /// dla zdarzeń rzadkich, a float w symulacji nie wchodzi w grę.
    #[inline]
    pub fn gen_bool_permille(&mut self, p: u16) -> bool {
        u32::from(p) > self.gen_range_u32(1000)
    }

    /// Tasowanie Fishera–Yatesa — jedyny dopuszczony sposób losowej permutacji.
    /// Kolejność wejścia jest kontraktem wywołującego: wynik zależy od niej,
    /// więc tasowanie listy zbudowanej z iteracji po `HashMap` nadal jest błędem.
    pub fn shuffle<T>(&mut self, slice: &mut [T]) {
        for i in (1..slice.len()).rev() {
            let j = self.gen_range_u32(i as u32 + 1) as usize;
            slice.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wektor_referencyjny_xoshiro256pp() {
        // Wartości z referencyjnej implementacji xoshiro256++ dla stanu [1,2,3,4].
        let mut r = Rng::from_state([1, 2, 3, 4]);
        let oczekiwane: [u64; 6] = [
            0x0280_0001,
            0x0380_0067,
            0x000C_C000_0380_0067,
            0x000C_C201_9944_00B2,
            0x8012_A201_9AC4_33CD,
            0x8A69_978A_CDEE_33BA,
        ];
        for (i, want) in oczekiwane.iter().enumerate() {
            assert_eq!(r.next_u64(), *want, "wyraz {i}");
        }
    }

    #[test]
    fn wektor_referencyjny_mix64() {
        assert_eq!(mix64(0), 0xE220_A839_7B1D_CDAF);
        assert_eq!(mix64(1), 0x910A_2DEC_8902_5CC1);
    }

    #[test]
    fn strumien_jest_czysta_funkcja() {
        let a = rng(42, StreamId::EngineSelfTest, 7, Tick(100));
        let b = rng(42, StreamId::EngineSelfTest, 7, Tick(100));
        assert_eq!(a, b, "ten sam zestaw argumentów = ten sam generator");

        // Zmiana któregokolwiek argumentu zmienia strumień.
        assert_ne!(a, rng(43, StreamId::EngineSelfTest, 7, Tick(100)));
        assert_ne!(a, rng(42, StreamId::EngineSelfTest, 8, Tick(100)));
        assert_ne!(a, rng(42, StreamId::EngineSelfTest, 7, Tick(101)));
    }

    #[test]
    fn kolejnosc_wywolan_nie_ma_znaczenia() {
        // Ten sam zbiór encji odpytany w dwóch różnych kolejnościach daje te same
        // wartości per encja — to jest warunek niezależności od liczby wątków.
        let w_przod: Vec<u64> = (0..100)
            .map(|e| rng(1, StreamId::EngineSelfTest, e, Tick(5)).next_u64())
            .collect();
        let mut w_tyl: Vec<u64> = (0..100)
            .rev()
            .map(|e| rng(1, StreamId::EngineSelfTest, e, Tick(5)).next_u64())
            .collect();
        w_tyl.reverse();
        assert_eq!(w_przod, w_tyl);
    }

    #[test]
    fn zakres_bez_obciazenia_modulo() {
        let mut r = rng(7, StreamId::EngineSelfTest, 0, Tick(0));
        let mut liczniki = [0u32; 3];
        for _ in 0..30_000 {
            let v = r.gen_range_u32(3);
            assert!(v < 3);
            liczniki[v as usize] += 1;
        }
        // Odchylenie od 10 000 na koszyk większe niż 5 % oznaczałoby obciążenie.
        for c in liczniki {
            assert!((9_500..10_500).contains(&c), "koszyki: {liczniki:?}");
        }
    }

    #[test]
    fn promile_trafiaja_w_zadeklarowana_czestosc() {
        let mut r = rng(9, StreamId::EngineSelfTest, 0, Tick(0));
        let trafienia = (0..100_000).filter(|_| r.gen_bool_permille(250)).count();
        assert!(
            (24_000..26_000).contains(&trafienia),
            "trafienia: {trafienia}"
        );
        // Skraje są absolutne, nie „prawie".
        assert!(!r.gen_bool_permille(0));
        assert!(r.gen_bool_permille(1000));
    }

    #[test]
    fn tasowanie_zachowuje_zawartosc() {
        let mut r = rng(3, StreamId::EngineSelfTest, 0, Tick(0));
        let mut v: Vec<u32> = (0..1000).collect();
        r.shuffle(&mut v);
        assert_ne!(v, (0..1000).collect::<Vec<_>>(), "nic się nie przetasowało");
        v.sort_unstable();
        assert_eq!(v, (0..1000).collect::<Vec<_>>());
    }

    #[test]
    fn wartosci_stream_id_sa_wieczne() {
        // Test strażniczy: dopisanie wariantu nie może zmienić istniejących wartości.
        assert_eq!(StreamId::EngineSelfTest as u16, 1);
        assert_eq!(StreamId::WorldLandmask as u16, 100);
        assert_eq!(StreamId::WorldDetail as u16, 110);
        assert_eq!(StreamId::Gates as u16, 120);
        assert_eq!(StreamId::RoadsL as u16, 121);
        assert_eq!(StreamId::Parcels as u16, 128);
        assert_eq!(StreamId::DayPlan as u16, 140);
        assert_eq!(StreamId::Relations as u16, 146);
        assert_eq!(StreamId::ModeChoice as u16, 160);
        assert_eq!(StreamId::WeatherStub as u16, 166);
        assert_eq!(StreamId::PurchaseNoise as u16, 180);
        assert_eq!(StreamId::PurchaseChoice as u16, 181);
        assert_eq!(StreamId::CreditScoringJitter as u16, 185);
    }
}
