//! Palety per rodzaj dzielnicy × epoka startowa (M11a §5.1, WP11). Katalog `data/palettes/`.
//!
//! **Osią jest epoka startowa gry** (`Epoch::key()` — 1950…2020), a nie pierścień wieku
//! zabudowy z `data/epochs/epochs.ron`. Słowo „epoka" znaczy w planie dwie różne rzeczy
//! i to jest wybór między nimi: ta paleta ubiera **ludzi i pojazdy**, a ci wyglądają tak,
//! jak się wtedy chodziło i jeździło, niezależnie od tego, ile lat ma kamienica obok.
//! Pierścień wieku rządzi materiałem budynku i jest osobną sprawą.
//!
//! Paleta **ogranicza, a nie definiuje**: slot `OutfitMain` w pasie przemysłowym losuje
//! z sześciu barw roboczych, w śródmieściu roku 2010 z szesnastu. Dzięki temu tonacja
//! dzielnicy jest własnością danych, nie kodu, i widać ją w diffie.
//!
//! ### Dlaczego nie ma tablicy wariantów (`E-7`)
//!
//! §5.1 zapisywał `PaletteTable { entries: Vec<[[u8;4];16]> }` z 4096 wpisami i
//! `VariantKey` rozwiązywanym do indeksu w tej tablicy. To się nie domyka arytmetycznie:
//! wpis jest per **wygląd**, a 24 576 mieszkańców w kadrze ma rzędu dwudziestu tysięcy
//! różnych wyglądów — pięciokrotnie ponad cap. Tablica unieważniałaby się co klatkę,
//! a mieszkańcy migotaliby barwą ubrania.
//!
//! Zamiast tego na GPU idą **zestawy barw**: płaska tablica kolorów plus deskryptor
//! `(offset, długość)` per `(paleta dzielnicy, rola slotu)`. Barwę wybiera [`pick`] —
//! funkcja czysta od `(wariant, rola)` — i liczy ją shader dokładnie tym samym wzorem,
//! którym liczy ją CPU w testach. Liczba wyglądów przestaje mieć jakikolwiek cap,
//! a całość waży kilka kilobajtów zamiast 256.

use crate::model::SlotRole;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

pub const PALETTES_SCHEMA_VERSION: u32 = 1;

/// Indeks pary (rodzaj dzielnicy × epoka) w [`PaletteLibrary`]. To jest pole
/// `district_palette` klucza wariantu i to ono jedzie w instancji GPU.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct DistrictPaletteId(pub u16);

/// Wybór barwy z zestawu: funkcja czysta od wariantu i roli.
///
/// Mieszanie jest **bijekcją na `u32`** (xor stałą roli, potem mnożenie przez liczbę
/// nieparzystą), więc dwa różne warianty nie sklejają się w jeden przed resztą z dzielenia.
/// Stąd bierze się obietnica „auto należy do konkretnego gospodarstwa i ma swój kolor":
/// ten sam numer lakieru daje zawsze tę samą barwę, a sąsiednie numery nie dają barw
/// sąsiadujących w zestawie.
///
/// Ta sama arytmetyka jest w `instance.wgsl`. Rozjazd łapie test
/// `wybor_barwy_zgadza_sie_z_shaderem`, który parsuje stałe wprost z pliku shadera.
#[must_use]
pub fn pick(variant: u32, role: SlotRole, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let r = role.as_index() as u32;
    let v = (variant ^ r.wrapping_mul(ROLE_SALT)).wrapping_mul(MIX);
    ((v >> 13) as usize) % len
}

/// Stała mieszająca — liczba nieparzysta, więc mnożenie przez nią jest bijekcją mod 2³².
pub const MIX: u32 = 0x9E37_79B9;
/// Rozsunięcie ról, żeby ten sam wariant nie dawał tej samej pozycji w każdym zestawie.
pub const ROLE_SALT: u32 = 0x85EB_CA6B;

/// Zestaw barw dla jednej roli: wycinek płaskiej tablicy kolorów.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct RampRef {
    pub offset: u32,
    pub len: u32,
}

/// Wczytana biblioteka palet.
///
/// Układ pamięci jest **tym samym**, który idzie na GPU: `colors` to bufor barw,
/// `ramps` to `palety × 12 ról` deskryptorów. Druga postać „do rysowania" rozjechałaby
/// się z pierwszą przy pierwszej zmianie danych.
#[derive(Clone, Debug, Default)]
pub struct PaletteLibrary {
    /// RGBA8 spakowane w `u32` (R w najmłodszym bajcie) — gotowe do bufora storage.
    colors: Vec<u32>,
    /// `palette_index * 12 + role_index`.
    ramps: Vec<RampRef>,
    /// Klucze dzielnic w kolejności nadawania identyfikatorów.
    districts: Vec<String>,
    /// Klucze epok w kolejności nadawania identyfikatorów.
    epochs: Vec<String>,
}

impl PaletteLibrary {
    /// Identyfikator palety dla pary kluczy. `None` = para spoza katalogu, co jest
    /// błędem danych, a nie stanem świata — walidator sprawdza pokrycie przy ładowaniu.
    #[must_use]
    pub fn id_of(&self, district: &str, epoch: &str) -> Option<DistrictPaletteId> {
        let d = self.districts.iter().position(|k| k == district)?;
        let e = self.epochs.iter().position(|k| k == epoch)?;
        Some(DistrictPaletteId((d * self.epochs.len() + e) as u16))
    }

    #[must_use]
    pub fn ramp(&self, p: DistrictPaletteId, role: SlotRole) -> RampRef {
        let i = p.0 as usize * SlotRole::ALL.len() + role.as_index();
        self.ramps.get(i).copied().unwrap_or_default()
    }

    /// Barwa dla wariantu i roli — dokładnie to, co policzy shader.
    #[must_use]
    pub fn color(&self, p: DistrictPaletteId, role: SlotRole, variant: u32) -> [u8; 4] {
        let r = self.ramp(p, role);
        if r.len == 0 {
            return [255, 0, 255, 255];
        }
        let i = r.offset as usize + pick(variant, role, r.len as usize);
        let c = self.colors[i];
        [c as u8, (c >> 8) as u8, (c >> 16) as u8, (c >> 24) as u8]
    }

    #[must_use]
    pub fn colors(&self) -> &[u32] {
        &self.colors
    }

    #[must_use]
    pub fn ramps(&self) -> &[RampRef] {
        &self.ramps
    }

    #[must_use]
    pub fn palette_count(&self) -> usize {
        self.districts.len() * self.epochs.len()
    }

    /// Wczytuje `data/palettes/palettes.ron`.
    pub fn load(dir: &Path) -> Result<PaletteLibrary, PaletteError> {
        let path = dir.join("palettes.ron");
        let text = std::fs::read_to_string(&path).map_err(PaletteError::Io)?;
        PaletteLibrary::parse(&text)
    }

    pub fn parse(text: &str) -> Result<PaletteLibrary, PaletteError> {
        let f: PaletteFile = ron::from_str(text).map_err(|e| PaletteError::Parse(e.to_string()))?;
        if f.schema_version != PALETTES_SCHEMA_VERSION {
            return Err(PaletteError::SchemaVersion(f.schema_version));
        }
        PaletteLibrary::build(f)
    }

    fn build(f: PaletteFile) -> Result<PaletteLibrary, PaletteError> {
        // Zestawy barw idą do płaskiej tablicy w kolejności **posortowanych kluczy**,
        // tak samo jak identyfikatory modeli i materiałów (00 §5).
        let mut nazwy: Vec<&String> = f.ramps.keys().collect();
        nazwy.sort();
        let mut colors: Vec<u32> = Vec::new();
        let mut ramp_by_key: BTreeMap<&str, RampRef> = BTreeMap::new();
        for n in nazwy {
            let barwy = &f.ramps[n];
            if barwy.is_empty() {
                return Err(PaletteError::EmptyRamp(n.clone()));
            }
            let r = RampRef {
                offset: colors.len() as u32,
                len: barwy.len() as u32,
            };
            for (r8, g8, b8) in barwy {
                colors.push(
                    u32::from(*r8)
                        | (u32::from(*g8) << 8)
                        | (u32::from(*b8) << 16)
                        | (0xFFu32 << 24),
                );
            }
            ramp_by_key.insert(n.as_str(), r);
        }

        // Domyślne przypisanie roli musi pokrywać **wszystkie dwanaście**: to jest
        // kryterium WP11 („walidator odrzuca paletę bez pokrycia wszystkich ról").
        // Sprawdzamy je raz, na domyślnych, bo nadpisania mogą je tylko zastąpić.
        let mut domyslne = [RampRef::default(); 12];
        for rola in SlotRole::ALL {
            let klucz = f
                .defaults
                .get(rola.key())
                .ok_or_else(|| PaletteError::RoleNotCovered {
                    palette: "defaults".into(),
                    role: rola.key().into(),
                })?;
            domyslne[rola.as_index()] =
                *ramp_by_key
                    .get(klucz.as_str())
                    .ok_or_else(|| PaletteError::UnknownRamp(klucz.clone()))?;
        }

        let districts = f.districts.clone();
        let epochs = f.epochs.clone();
        if districts.is_empty() || epochs.is_empty() {
            return Err(PaletteError::NoAxes);
        }

        // Nadpisania: najpierw wpisy z gwiazdką (obowiązują całą epokę), potem imienne.
        // Kolejność jest regułą rozstrzygania, a nie kolejnością w pliku — dzięki temu
        // przestawienie wierszy w danych niczego nie zmienia.
        let mut ramps = vec![RampRef::default(); districts.len() * epochs.len() * 12];
        for (di, d) in districts.iter().enumerate() {
            for (ei, e) in epochs.iter().enumerate() {
                let baza = (di * epochs.len() + ei) * 12;
                ramps[baza..baza + 12].copy_from_slice(&domyslne);
                for wzorzec in [
                    ("*", "*"),
                    ("*", e.as_str()),
                    (d.as_str(), "*"),
                    (d.as_str(), e.as_str()),
                ] {
                    let Some(p) = f
                        .palettes
                        .iter()
                        .find(|p| p.district == wzorzec.0 && p.epoch == wzorzec.1)
                    else {
                        continue;
                    };
                    for (rola_klucz, ramp_klucz) in &p.ramps {
                        let rola = SlotRole::from_key(rola_klucz)
                            .ok_or_else(|| PaletteError::UnknownRole(rola_klucz.clone()))?;
                        let r = *ramp_by_key
                            .get(ramp_klucz.as_str())
                            .ok_or_else(|| PaletteError::UnknownRamp(ramp_klucz.clone()))?;
                        ramps[baza + rola.as_index()] = r;
                    }
                }
            }
        }

        // Wpis palety wskazujący nieistniejącą dzielnicę albo epokę jest martwy i wygląda
        // dokładnie tak samo jak działający — stąd błąd, a nie ostrzeżenie. Z tego samego
        // powodu błędem jest **druga** paleta o tej samej parze kluczy: rozstrzyganie bierze
        // pierwszą, więc druga nie robi nic i nikt się o tym nie dowie.
        let mut widziane: BTreeMap<(&str, &str), ()> = BTreeMap::new();
        for p in &f.palettes {
            if p.district != "*" && !districts.contains(&p.district) {
                return Err(PaletteError::UnknownDistrict(p.district.clone()));
            }
            if p.epoch != "*" && !epochs.contains(&p.epoch) {
                return Err(PaletteError::UnknownEpoch(p.epoch.clone()));
            }
            if widziane
                .insert((p.district.as_str(), p.epoch.as_str()), ())
                .is_some()
            {
                return Err(PaletteError::DuplicatePalette {
                    district: p.district.clone(),
                    epoch: p.epoch.clone(),
                });
            }
        }

        let lib = PaletteLibrary {
            colors,
            ramps,
            districts,
            epochs,
        };
        lib.validate()?;
        Ok(lib)
    }

    /// Każda para dzielnica × epoka ma niepusty zestaw dla **każdej** z dwunastu ról.
    pub fn validate(&self) -> Result<(), PaletteError> {
        for (di, d) in self.districts.iter().enumerate() {
            for (ei, e) in self.epochs.iter().enumerate() {
                let id = DistrictPaletteId((di * self.epochs.len() + ei) as u16);
                for rola in SlotRole::ALL {
                    let r = self.ramp(id, rola);
                    if r.len == 0 {
                        return Err(PaletteError::RoleNotCovered {
                            palette: format!("{d}/{e}"),
                            role: rola.key().into(),
                        });
                    }
                    if (r.offset + r.len) as usize > self.colors.len() {
                        return Err(PaletteError::RampOutOfRange {
                            palette: format!("{d}/{e}"),
                            role: rola.key().into(),
                        });
                    }
                }
            }
        }
        Ok(())
    }
}

// ── Postać w pliku ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize)]
struct PaletteFile {
    schema_version: u32,
    /// Kolejność jest kontraktem: indeks dzielnicy wchodzi do `DistrictPaletteId`,
    /// a ten jedzie w instancji GPU.
    districts: Vec<String>,
    epochs: Vec<String>,
    ramps: BTreeMap<String, Vec<(u8, u8, u8)>>,
    defaults: BTreeMap<String, String>,
    #[serde(default)]
    palettes: Vec<PaletteDef>,
}

#[derive(Clone, Debug, Deserialize)]
struct PaletteDef {
    /// Klucz dzielnicy albo `"*"` — wtedy wpis obowiązuje wszystkie dzielnice tej epoki.
    district: String,
    epoch: String,
    ramps: BTreeMap<String, String>,
}

#[derive(Debug)]
pub enum PaletteError {
    Io(std::io::Error),
    Parse(String),
    SchemaVersion(u32),
    NoAxes,
    EmptyRamp(String),
    UnknownRamp(String),
    UnknownRole(String),
    UnknownDistrict(String),
    UnknownEpoch(String),
    RoleNotCovered { palette: String, role: String },
    RampOutOfRange { palette: String, role: String },
    DuplicatePalette { district: String, epoch: String },
}

impl fmt::Display for PaletteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PaletteError::Io(e) => write!(f, "odczyt palet: {e}"),
            PaletteError::Parse(m) => write!(f, "palettes.ron: {m}"),
            PaletteError::SchemaVersion(g) => write!(
                f,
                "palettes.ron: schema_version {g}, oczekiwano {PALETTES_SCHEMA_VERSION}"
            ),
            PaletteError::NoAxes => write!(f, "palettes.ron: pusta lista dzielnic albo epok"),
            PaletteError::EmptyRamp(k) => write!(f, "zestaw barw `{k}` jest pusty"),
            PaletteError::UnknownRamp(k) => write!(f, "nieznany zestaw barw `{k}`"),
            PaletteError::UnknownRole(k) => write!(f, "nieznana rola slotu `{k}`"),
            PaletteError::UnknownDistrict(k) => {
                write!(f, "paleta wskazuje nieistniejącą dzielnicę `{k}`")
            }
            PaletteError::UnknownEpoch(k) => write!(f, "paleta wskazuje nieistniejącą epokę `{k}`"),
            PaletteError::RoleNotCovered { palette, role } => {
                write!(f, "paleta `{palette}` nie pokrywa roli `{role}`")
            }
            PaletteError::RampOutOfRange { palette, role } => {
                write!(f, "paleta `{palette}`, rola `{role}`: zestaw poza tablicą barw")
            }
            PaletteError::DuplicatePalette { district, epoch } => write!(
                f,
                "paleta `{district}`/`{epoch}` zdefiniowana dwukrotnie — druga nic nie robi"
            ),
        }
    }
}

impl std::error::Error for PaletteError {}

#[cfg(test)]
mod tests {
    use super::*;

    const MINI: &str = r#"(
        schema_version: 1,
        districts: ["old_town", "suburb"],
        epochs: ["medieval", "contemporary"],
        ramps: {
            "neutral": [(128,128,128)],
            "outfit_old": [(90,70,50),(70,60,40),(110,90,70)],
            "outfit_new": [(20,60,140),(200,40,40),(240,240,240),(30,30,30)],
        },
        defaults: {
            "skin": "neutral", "hair": "neutral", "outfit_main": "outfit_old",
            "outfit_trim": "neutral", "accent": "neutral", "metal": "neutral",
            "glass": "neutral", "paint": "neutral", "livery": "neutral",
            "sign": "neutral", "rubber": "neutral", "emissive": "neutral",
        },
        palettes: [
            (district: "*", epoch: "contemporary", ramps: {"outfit_main": "outfit_new"}),
        ],
    )"#;

    #[test]
    fn gwiazdka_obowiazuje_cala_epoke_a_domyslne_reszte() {
        let lib = PaletteLibrary::parse(MINI).expect("parse");
        assert_eq!(lib.palette_count(), 4);
        let stare = lib.id_of("old_town", "medieval").unwrap();
        let nowe = lib.id_of("suburb", "contemporary").unwrap();
        assert_eq!(lib.ramp(stare, SlotRole::OutfitMain).len, 3);
        assert_eq!(lib.ramp(nowe, SlotRole::OutfitMain).len, 4);
        assert_eq!(lib.ramp(nowe, SlotRole::Skin).len, 1);
    }

    /// Kryterium WP11: walidator odrzuca paletę bez pokrycia wszystkich ról slotów.
    #[test]
    fn brak_roli_w_domyslnych_jest_bledem() {
        let bez_wlosow = MINI.replace(r#""hair": "neutral", "#, "");
        assert!(matches!(
            PaletteLibrary::parse(&bez_wlosow),
            Err(PaletteError::RoleNotCovered { .. })
        ));
    }

    #[test]
    fn druga_paleta_o_tej_samej_parze_jest_bledem() {
        let dubel = MINI.replace(
            r#"        ],
    )"#,
            r#"            (district: "*", epoch: "contemporary", ramps: {"skin": "neutral"}),
        ],
    )"#,
        );
        assert!(matches!(
            PaletteLibrary::parse(&dubel),
            Err(PaletteError::DuplicatePalette { .. })
        ));
    }

    /// Wpis z gwiazdką w **obu** osiach obowiązuje cały katalog. Bez tego byłby daną,
    /// która nic nie robi i wygląda tak samo jak działająca.
    #[test]
    fn gwiazdka_w_obu_osiach_obowiazuje_wszystko() {
        let wszedzie = MINI.replace(
            r#"(district: "*", epoch: "contemporary", ramps: {"outfit_main": "outfit_new"}),"#,
            r#"(district: "*", epoch: "*", ramps: {"skin": "outfit_new"}),"#,
        );
        let lib = PaletteLibrary::parse(&wszedzie).expect("parse");
        for d in ["old_town", "suburb"] {
            for e in ["medieval", "contemporary"] {
                let id = lib.id_of(d, e).unwrap();
                assert_eq!(lib.ramp(id, SlotRole::Skin).len, 4, "{d}/{e}");
            }
        }
    }

    #[test]
    fn wpis_do_nieistniejacej_dzielnicy_jest_bledem() {
        let zly = MINI.replace(r#"district: "*""#, r#"district: "atlantis""#);
        assert!(matches!(
            PaletteLibrary::parse(&zly),
            Err(PaletteError::UnknownDistrict(_))
        ));
    }

    /// Cała treść WP11: ta sama dzielnica w dwóch epokach ma **wyraźnie inną tonację**.
    /// Mierzymy to liczbą, bo zrzut ekranu nie przechodzi w CI — średnia barwa ubrania
    /// ma się różnić, a nie „wyglądać inaczej".
    #[test]
    fn ta_sama_dzielnica_w_dwoch_epokach_ma_inna_tonacje() {
        let lib = PaletteLibrary::parse(MINI).expect("parse");
        let srednia = |epoka: &str| -> [u32; 3] {
            let id = lib.id_of("old_town", epoka).unwrap();
            let mut s = [0u32; 3];
            for v in 0..256u32 {
                let c = lib.color(id, SlotRole::OutfitMain, v);
                for i in 0..3 {
                    s[i] += u32::from(c[i]);
                }
            }
            [s[0] / 256, s[1] / 256, s[2] / 256]
        };
        let a = srednia("medieval");
        let b = srednia("contemporary");
        let roznica: u32 = (0..3).map(|i| a[i].abs_diff(b[i])).sum();
        assert!(roznica > 60, "tonacje różnią się o {roznica}, za mało");
    }

    /// Wybór barwy jest funkcją czystą i rozkłada wariant po całym zestawie —
    /// inaczej cała dzielnica chodziłaby w jednym kolorze.
    #[test]
    fn wybor_barwy_jest_stabilny_i_rozrzucony() {
        let lib = PaletteLibrary::parse(MINI).expect("parse");
        let id = lib.id_of("suburb", "contemporary").unwrap();
        assert_eq!(
            lib.color(id, SlotRole::OutfitMain, 77),
            lib.color(id, SlotRole::OutfitMain, 77)
        );
        let uzyte: std::collections::BTreeSet<usize> =
            (0..64).map(|v| pick(v, SlotRole::OutfitMain, 4)).collect();
        assert_eq!(uzyte.len(), 4, "wariant nie pokrywa całego zestawu");
    }

    /// Katalog w `data/` musi się ładować i przechodzić walidator — inaczej gra staje
    /// przy pierwszym mieszkańcu, a test przechodziłby na danych z pliku źródłowego.
    #[test]
    fn katalog_produkcyjny_laduje_sie_i_pokrywa_wszystko() {
        let dir = magnat_core::assets::data_path("palettes");
        let lib = PaletteLibrary::load(&dir).expect("data/palettes/palettes.ron");
        assert_eq!(lib.palette_count(), 9 * 5, "9 rodzajów dzielnic × 5 epok startowych");
        lib.validate().expect("pokrycie ról");
        // Lakier ma mieć zapas na 64 gospodarstwa (§5.2: 48 modeli × 64 lakiery).
        let id = lib.id_of("suburb", "2020").unwrap();
        assert!(lib.ramp(id, SlotRole::Paint).len >= 64);
        // Ta sama dzielnica w 1950 i 2020 ma mieć inne ubranie — inaczej oś epoki
        // jest w danych, ale nie działa.
        let stare = lib.id_of("suburb", "1950").unwrap();
        assert_ne!(
            lib.color(stare, SlotRole::OutfitMain, 11),
            lib.color(id, SlotRole::OutfitMain, 11),
            "epoka nie zmienia ubrania"
        );
    }
}
