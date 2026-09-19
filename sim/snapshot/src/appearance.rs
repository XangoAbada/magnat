//! Wygląd mieszkańca spakowany w 32 bity (M11a §5.2, WP11).
//!
//! ### Dlaczego to nie jest komponent ECS (`E-4`)
//!
//! Plan fazy pisał: „losowanie deterministyczne … **raz, przy generacji encji**, wynik
//! ląduje w polu `appearance` i jest niezmienny". Pole w ECS znaczy jednak stan: wchodzi
//! do hasha (00 §3.6), do zapisu gry i do migracji schematu — a nie niesie **ani jednej**
//! informacji, której świat już nie ma. Bity losowe (sylwetka, karnacja, fryzura, odcień)
//! są funkcją czystą `(world_seed, entity_index)`; bity stanowe (klasa ubrania, zamożność,
//! wiek) są funkcją zawodu i gospodarstwa, czyli komponentów, które już istnieją.
//!
//! Zapisany komponent dałby **drugie źródło prawdy o tym, gdzie mieszkaniec pracuje**,
//! i rozjechałby się przy pierwszej zmianie pracy — chyba że ktoś pamiętałby o jego
//! przeliczeniu w każdym miejscu, które rusza etatem. Stąd: [`Appearance::derive`] liczy
//! się przy wypełnianiu snapshotu, a obietnica niezmienności jest **mocniejsza** niż
//! przy polu, bo nie da się jej złamać zapomnianym zapisem.
//!
//! Konsekwencja dla DoD fazy (§7.5 pkt 4, „nowe komponenty po stronie sim dopisane do
//! funkcji haszującej"): M11a nie wnosi ani jednego komponentu, więc warunek jest
//! spełniony zbiorem pustym, a hash stanu nie drga.

use magnat_core::{rng, StreamId, Tick};

/// Spakowany wariant wyglądu. Rozkład bitów jest **kontraktem** — czyta go shader
/// instancji i rozwiązywanie palety (`engine/voxel::palette`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Appearance(pub u32);

/// Pola wyglądu przed spakowaniem. Wartości powyżej zakresu pola są **obcinane maską**,
/// a nie odrzucane: klasa ubrania pochodzi z `JobRoleId`, którego katalog rośnie (46 ról
/// po M7a), a pole ma pięć bitów — zawijanie jest wtedy jedyną odpowiedzią, która nie
/// wywraca renderu przy dopisaniu roli.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct AppearanceFields {
    /// Sylwetka i wzrost, 0..7.
    pub body: u8,
    /// Karnacja i rysy, 0..15.
    pub head: u8,
    /// Fryzura i jej kolor, 0..15.
    pub hair: u8,
    /// Klasa ubrania wyprowadzona z `JobRoleId`, 0..31.
    pub outfit_class: u8,
    /// Zamożność gospodarstwa, 0..3 — ten sam fartuch, lepszy materiał i odcień.
    pub outfit_tier: u8,
    /// Losowanie odcienia **w obrębie** palety dzielnicy i epoki, 0..63.
    pub palette_seed: u8,
    /// Przedział wieku, 0..7.
    pub age_band: u8,
    /// Czapka, parasol (przy deszczu!), teczka, wózek, 0..31.
    pub accessory: u8,
}

const BODY: (u32, u32) = (0, 0b111);
const HEAD: (u32, u32) = (3, 0b1111);
const HAIR: (u32, u32) = (7, 0b1111);
const OUTFIT_CLASS: (u32, u32) = (11, 0b1_1111);
const OUTFIT_TIER: (u32, u32) = (16, 0b11);
const PALETTE_SEED: (u32, u32) = (18, 0b11_1111);
const AGE_BAND: (u32, u32) = (24, 0b111);
const ACCESSORY: (u32, u32) = (27, 0b1_1111);

#[inline]
const fn wstaw(v: u32, pole: (u32, u32), x: u8) -> u32 {
    v | ((x as u32 & pole.1) << pole.0)
}

#[inline]
const fn wytnij(v: u32, pole: (u32, u32)) -> u8 {
    ((v >> pole.0) & pole.1) as u8
}

impl Appearance {
    #[must_use]
    pub const fn pack(f: AppearanceFields) -> Appearance {
        let mut v = 0u32;
        v = wstaw(v, BODY, f.body);
        v = wstaw(v, HEAD, f.head);
        v = wstaw(v, HAIR, f.hair);
        v = wstaw(v, OUTFIT_CLASS, f.outfit_class);
        v = wstaw(v, OUTFIT_TIER, f.outfit_tier);
        v = wstaw(v, PALETTE_SEED, f.palette_seed);
        v = wstaw(v, AGE_BAND, f.age_band);
        v = wstaw(v, ACCESSORY, f.accessory);
        Appearance(v)
    }

    #[must_use]
    pub const fn unpack(self) -> AppearanceFields {
        AppearanceFields {
            body: wytnij(self.0, BODY),
            head: wytnij(self.0, HEAD),
            hair: wytnij(self.0, HAIR),
            outfit_class: wytnij(self.0, OUTFIT_CLASS),
            outfit_tier: wytnij(self.0, OUTFIT_TIER),
            palette_seed: wytnij(self.0, PALETTE_SEED),
            age_band: wytnij(self.0, AGE_BAND),
            accessory: wytnij(self.0, ACCESSORY),
        }
    }

    // Osobnych akcesorów na pojedyncze pola **nie ma**: `unpack` oddaje wszystkie osiem
    // naraz, a shader czyta spakowane `u32` bez rozbierania go na części. Osiem funkcji
    // bez czytelnika wygląda w API tak samo jak osiem używanych.

    /// Wygląd mieszkańca: bity losowe z `(world_seed, entity_index)`, bity stanowe
    /// z argumentów.
    ///
    /// Jedno losowanie na wywołanie i **ten sam wynik przy każdym** — `rng` jest funkcją
    /// czystą czterech argumentów (00 §3.1), więc odtworzenie wyglądu przy kolejnej
    /// publikacji snapshotu nie jest ponownym losowaniem, tylko ponownym policzeniem.
    /// `tick` jest zerem z rozmysłu: człowiek, który co minutę zmienia twarz, nie jest
    /// człowiekiem.
    #[must_use]
    pub fn derive(
        world_seed: u64,
        entity_index: u32,
        outfit_class: u8,
        outfit_tier: u8,
        age_band: u8,
        accessory: u8,
    ) -> Appearance {
        let mut r = rng(world_seed, StreamId::Appearance, entity_index, Tick(0));
        let los = r.next_u32();
        Appearance::pack(AppearanceFields {
            body: (los & 0b111) as u8,
            head: ((los >> 3) & 0b1111) as u8,
            hair: ((los >> 7) & 0b1111) as u8,
            outfit_class,
            outfit_tier,
            palette_seed: ((los >> 11) & 0b11_1111) as u8,
            age_band,
            accessory,
        })
    }
}

/// Przedział wieku — bity 24–26. Kolejność jest kontraktem, bo indeksuje dobór modelu
/// (dziecko jest niższe) i wagę kroku w animacji.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
#[repr(u8)]
pub enum AgeBand {
    Child = 0,
    Teen = 1,
    #[default]
    Adult = 2,
    Senior = 3,
}

impl AgeBand {
    /// Przedział z wieku w latach. Granice idą za `data/demography/demography.ron`
    /// w duchu, nie w liczbach: wygląd nie ma prawa być drugim źródłem prawdy o wieku
    /// szkolnym (`K-60`), więc mówi wyłącznie o tym, jak ktoś wygląda.
    #[must_use]
    pub const fn from_years(lata: u8) -> AgeBand {
        match lata {
            0..=12 => AgeBand::Child,
            13..=19 => AgeBand::Teen,
            20..=64 => AgeBand::Adult,
            _ => AgeBand::Senior,
        }
    }
}

/// Zamożność gospodarstwa w skali wyglądu — bity 16–17.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
#[repr(u8)]
pub enum OutfitTier {
    #[default]
    Poor = 0,
    Modest = 1,
    Comfortable = 2,
    Affluent = 3,
}

impl OutfitTier {
    /// Z percentyla zamożności 0..100 (`sim/agents::social::wealth_percentile`).
    /// Progi ćwiartkowe: wygląd ma **rozróżniać**, a nie mierzyć — do mierzenia
    /// jest karta gospodarstwa.
    #[must_use]
    pub const fn from_percentile(p: u8) -> OutfitTier {
        match p {
            0..=24 => OutfitTier::Poor,
            25..=59 => OutfitTier::Modest,
            60..=89 => OutfitTier::Comfortable,
            _ => OutfitTier::Affluent,
        }
    }
}

/// Co mieszkaniec niesie — pole `carry` rekordu, nie część `appearance`.
///
/// Osobno od `accessory` z rozmysłu: akcesorium jest stałe (człowiek nosi czapkę), a to,
/// co się niesie, zmienia się w ciągu doby — siatka pojawia się dopiero przy wyjściu
/// ze sklepu. Kolejność wariantów jest kontraktem, bo indeksuje prop w `.mvox`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
#[repr(u8)]
pub enum Carry {
    #[default]
    None = 0,
    Bag = 1,
    ShoppingNet = 2,
    Crate = 3,
    Tool = 4,
    Briefcase = 5,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pakowanie_jest_odwracalne_na_calym_zakresie() {
        let f = AppearanceFields {
            body: 7,
            head: 15,
            hair: 15,
            outfit_class: 31,
            outfit_tier: 3,
            palette_seed: 63,
            age_band: 7,
            accessory: 31,
        };
        assert_eq!(Appearance::pack(f).unpack(), f);
        assert_eq!(Appearance::pack(f).0, u32::MAX, "pola nie pokrywają 32 bitów");
        assert_eq!(
            Appearance::pack(AppearanceFields::default()).0,
            0,
            "zero nie pakuje się na zero"
        );
    }

    /// Pola nie mogą na siebie nachodzić: ustawienie jednego na maksimum nie ma prawa
    /// zmienić żadnego innego.
    #[test]
    fn pola_sie_nie_nakladaja() {
        let ustaw: [fn(&mut AppearanceFields); 8] = [
            |f| f.body = 7,
            |f| f.head = 15,
            |f| f.hair = 15,
            |f| f.outfit_class = 31,
            |f| f.outfit_tier = 3,
            |f| f.palette_seed = 63,
            |f| f.age_band = 7,
            |f| f.accessory = 31,
        ];
        let mut suma = 0u32;
        for s in ustaw {
            let mut f = AppearanceFields::default();
            s(&mut f);
            let bity = Appearance::pack(f).0;
            assert_eq!(suma & bity, 0, "pole nachodzi na poprzednie");
            suma |= bity;
        }
        assert_eq!(suma, u32::MAX);
    }

    /// Wygląd jest funkcją czystą: ten sam mieszkaniec w tym samym świecie wygląda
    /// tak samo przy każdym wywołaniu, a różni mieszkańcy wyglądają różnie.
    #[test]
    fn wyglad_jest_stabilny_i_rozny_miedzy_encjami() {
        let a = Appearance::derive(0xDEAD_BEEF, 42, 3, 1, 2, 0);
        let b = Appearance::derive(0xDEAD_BEEF, 42, 3, 1, 2, 0);
        assert_eq!(a, b, "dwa wywołania dały inny wygląd");

        let inny_swiat = Appearance::derive(1, 42, 3, 1, 2, 0);
        assert_ne!(a, inny_swiat, "seed nie zmienia wyglądu");

        let rozne: std::collections::BTreeSet<u32> = (0..256)
            .map(|i| Appearance::derive(7, i, 3, 1, 2, 0).0)
            .collect();
        assert!(
            rozne.len() > 200,
            "256 mieszkańców dało tylko {} wyglądów",
            rozne.len()
        );
    }

    /// Zmiana pracy zmienia **wyłącznie** klasę ubrania. Reszta wyglądu jest ta sama
    /// osoba — to jest cała treść obietnicy „appearance jest niezmienne poza bitami
    /// 11–17" z §5.2.
    #[test]
    fn zmiana_pracy_nie_zmienia_twarzy() {
        let przed = Appearance::derive(7, 100, 3, 1, 2, 0).unpack();
        let po = Appearance::derive(7, 100, 19, 1, 2, 0).unpack();
        assert_eq!(po.outfit_class, 19);
        assert_eq!((przed.body, przed.head, przed.hair), (po.body, po.head, po.hair));
        assert_eq!(przed.palette_seed, po.palette_seed);
    }

    #[test]
    fn progi_wieku_i_zamoznosci() {
        assert_eq!(AgeBand::from_years(7), AgeBand::Child);
        assert_eq!(AgeBand::from_years(16), AgeBand::Teen);
        assert_eq!(AgeBand::from_years(40), AgeBand::Adult);
        assert_eq!(AgeBand::from_years(70), AgeBand::Senior);
        assert_eq!(OutfitTier::from_percentile(0), OutfitTier::Poor);
        assert_eq!(OutfitTier::from_percentile(50), OutfitTier::Modest);
        assert_eq!(OutfitTier::from_percentile(75), OutfitTier::Comfortable);
        assert_eq!(OutfitTier::from_percentile(100), OutfitTier::Affluent);
    }
}
