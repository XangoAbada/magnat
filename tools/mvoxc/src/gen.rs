//! Modele zastępcze generowane proceduralnie (M11a, WP1).
//!
//! `data/models/` musi coś zawierać, zanim powstanie pierwszy plik artysty — inaczej
//! ścieżka „snapshot → instancja → draw call" nie ma czego narysować i nie da się jej
//! sprawdzić. Te dwa modele są **zastępcze i nazwane po imieniu**: bryły z pudełek,
//! z pełną hierarchią części, pivotami i trzema poziomami detalu, czyli ze wszystkim,
//! czego dotyczy kontrakt. Podmiana na `.vox` artysty jest wywołaniem `mvoxc import`
//! i nie rusza ani jednej linii kodu.
//!
//! ponytail: sufit jest jawny — postać ma cztery role palety zamiast dwunastu, bo przy
//! siedmiu voxelach wzrostu (1,75 m) nie ma miejsca na osobny voxel włosów nad głową,
//! a nakładanie części na siebie dałoby migotanie głębi. Włosy, nakrycie głowy i niesiony
//! przedmiot wchodzą razem z modelem artysty i z klipami M11b.

use magnat_voxel::{ModelFlags, ModelKind, PaletteSlot, Part, PartName, SlotRole, VoxModel};

/// Slot 1 — odsłonięta skóra.
const SKIN: u8 = 1;
/// Slot 2 — ubranie zasadnicze.
const OUTFIT: u8 = 2;
/// Slot 3 — wykończenie (buty, pas).
const TRIM: u8 = 3;
/// Slot 4 — lakier pojazdu.
const PAINT: u8 = 4;
/// Slot 5 — szyby.
const GLASS: u8 = 5;
/// Slot 6 — opony.
const RUBBER: u8 = 6;
/// Slot 7 — światła.
const EMISSIVE: u8 = 7;

fn pudlo(
    name: PartName,
    parent: u8,
    lods: u8,
    pivot: [i16; 3],
    dims: [u8; 3],
    slot: u8,
) -> Part {
    let n = dims[0] as usize * dims[1] as usize * dims[2] as usize;
    Part {
        name,
        parent,
        lod_mask: lods,
        pivot,
        dims,
        voxels: vec![slot; n],
    }
}

/// Mieszkaniec: 1,75 m, siedem voxeli wzrostu.
///
/// Poziomy: L0 ma dziesięć części (staw łokciowy i kolanowy osobno), L1 zbija ramiona
/// i nogi w dwie bryły — to jest dokładnie podział z §5.1, tyle że pudełkami.
///
/// **Postać patrzy w `+X`, tak samo jak auto jedzie w `+X`.** Oś barków i rozstaw nóg
/// idą wzdłuż `Y`, więc wymach kończyny w marszu jest obrotem wokół `Y` — i to jest
/// jedyna orientacja, przy której `yaw` z warstwy ruchu ustawia postać twarzą do celu.
/// Do M11b model stał bokiem i nikt tego nie widział, bo `yaw` mieszkańca był zerem
/// (`F-5`); pierwszy obrócony pieszy pokazałby to natychmiast.
///
/// **Bryła jest wyśrodkowana na origin w poziomie**, a stoi na nim w pionie. To nie jest
/// kosmetyka: shader obraca model o `yaw` **wokół origin**, więc model przesunięty
/// względem niego zatacza łuk zamiast się obrócić, a impostor wypalony z tej samej bryły
/// wychodzi poza kafel. Pivot korzenia niesie to przesunięcie i nic poza nim nie drgnęło.
#[must_use]
pub fn citizen() -> VoxModel {
    // Układ pionowy w voxelach: golenie 0–2, uda 2–4, tors 4–6, głowa 6–7.
    let parts = vec![
        // 0: tors — korzeń, metr nad gruntem.
        pudlo(PartName::TORSO, Part::NO_PARENT, 0b011, [-2, -4, 16], [1, 2, 2], OUTFIT),
        // 1: głowa.
        pudlo(PartName::HEAD, 0, 0b011, [0, 0, 8], [1, 2, 1], SKIN),
        // 2–3: lewe ramię i przedramię (L0).
        pudlo(PartName::ARM_L, 0, 0b001, [0, -4, 4], [1, 1, 1], OUTFIT),
        pudlo(PartName::FOREARM_L, 2, 0b001, [0, 0, -4], [1, 1, 1], SKIN),
        // 4–5: prawe.
        pudlo(PartName::ARM_R, 0, 0b001, [0, 8, 4], [1, 1, 1], OUTFIT),
        pudlo(PartName::FOREARM_R, 4, 0b001, [0, 0, -4], [1, 1, 1], SKIN),
        // 6–7: lewa noga.
        pudlo(PartName::THIGH_L, 0, 0b001, [0, 0, -8], [1, 1, 2], OUTFIT),
        pudlo(PartName::SHIN_L, 6, 0b001, [0, 0, -8], [1, 1, 2], TRIM),
        // 8–9: prawa noga.
        pudlo(PartName::THIGH_R, 0, 0b001, [0, 4, -8], [1, 1, 2], OUTFIT),
        pudlo(PartName::SHIN_R, 8, 0b001, [0, 0, -8], [1, 1, 2], TRIM),
        // 10–11: uproszczenia L1 — jedna bryła na ramiona, jedna na nogi.
        pudlo(PartName::ARMS, 0, 0b010, [0, -4, 0], [1, 4, 2], OUTFIT),
        pudlo(PartName::LEGS, 0, 0b010, [0, 0, -16], [1, 2, 4], OUTFIT),
    ];
    VoxModel {
        key: "citizen".into(),
        kind: ModelKind::Character,
        flags: ModelFlags::default(),
        bbox: [1, 4, 7],
        parts,
        slots: vec![
            PaletteSlot { slot: SKIN, role: SlotRole::Skin },
            PaletteSlot { slot: OUTFIT, role: SlotRole::OutfitMain },
            PaletteSlot { slot: TRIM, role: SlotRole::OutfitTrim },
        ],
    }
}

/// Samochód osobowy: 4 × 1,75 × 1,5 m, czyli 16 × 7 × 6 voxeli.
///
/// L1 traci kabinę i światła i zostaje przy bryle z czterema kołami — §5.1.
#[must_use]
pub fn car() -> VoxModel {
    let parts = vec![
        pudlo(PartName::BODY, Part::NO_PARENT, 0b011, [-32, -14, 4], [16, 7, 3], PAINT),
        pudlo(PartName::CAB, 0, 0b001, [16, 4, 12], [8, 5, 2], GLASS),
        // Koła **w obrysie** nadwozia, nie poza nim: auto ma mieć 1,75 m szerokości
        // razem z nimi, a nie 2,25 m.
        pudlo(PartName::WHEEL_FL, 0, 0b011, [8, 0, -4], [2, 1, 2], RUBBER),
        pudlo(PartName::WHEEL_FR, 0, 0b011, [8, 24, -4], [2, 1, 2], RUBBER),
        pudlo(PartName::WHEEL_RL, 0, 0b011, [44, 0, -4], [2, 1, 2], RUBBER),
        pudlo(PartName::WHEEL_RR, 0, 0b011, [44, 24, -4], [2, 1, 2], RUBBER),
        pudlo(PartName::LAMPS, 0, 0b001, [60, 4, 4], [1, 5, 1], EMISSIVE),
    ];
    VoxModel {
        key: "car".into(),
        kind: ModelKind::Vehicle,
        flags: ModelFlags::HAS_WHEELS.union(ModelFlags::EMISSIVE),
        bbox: [16, 7, 6],
        parts,
        slots: vec![
            PaletteSlot { slot: PAINT, role: SlotRole::Paint },
            PaletteSlot { slot: GLASS, role: SlotRole::Glass },
            PaletteSlot { slot: RUBBER, role: SlotRole::Rubber },
            PaletteSlot { slot: EMISSIVE, role: SlotRole::Emissive },
        ],
    }
}

/// Slot 8 — powierzchnia szyldu; barwę bierze z marki firmy (`SlotRole::Sign`).
const SIGN: u8 = 8;
/// Slot 9 — korpus mebla albo maszyny.
const BODY_SLOT: u8 = 9;


/// Prop jednobryłowy: regał, skrzynia, biurko, maszyna (M11c §5.7, WP5).
///
/// Jedna część, jeden slot, trzy poziomy detalu — wnętrze widać wyłącznie przy aktywnym
/// przekroju albo w trybie pierwszoosobowym, czyli z bliska i w małej liczbie sztuk,
/// więc uproszczenie L1 nic by tu nie kupiło. `dims` są w voxelach 0,25 m.
fn prop(key: &str, dims: [u8; 3], slot: u8, rola: SlotRole) -> VoxModel {
    // Pivot w poziomie na środku bryły, w pionie na jej spodzie: prop stoi na podłodze
    // i obraca się wokół własnej osi, tak samo jak postać i pojazd.
    let pivot = [
        -(i16::from(dims[0]) * 4) / 2,
        -(i16::from(dims[1]) * 4) / 2,
        0,
    ];
    VoxModel {
        key: key.into(),
        kind: ModelKind::Prop,
        flags: ModelFlags::default(),
        bbox: dims,
        parts: vec![pudlo(PartName::BODY, Part::NO_PARENT, 0b111, pivot, dims, slot)],
        slots: vec![PaletteSlot { slot, role: rola }],
    }
}

/// Szyld firmy: tablica 2,5 × 0,25 × 0,75 m (M11c §5.7, WP9).
///
/// Lico ma **własny slot** (`SlotRole::Sign`), bo barwę bierze z marki firmy, a nie
/// z palety dzielnicy — dwa sklepy na tej samej ulicy mają się różnić szyldem, a nie
/// tylko nazwą. Napis nakłada atlas szyldów; model niesie samą powierzchnię.
///
/// Wspornika nie ma i to jest decyzja, nie przeoczenie: szyld wisi na ścianie, więc
/// gracz go nie widzi, a każda część poniżej origin łamie regułę „model stoi na origin",
/// na której stoi obrót o `yaw` i wypalanie sylwetek.
#[must_use]
pub fn sign() -> VoxModel {
    VoxModel {
        key: "sign".into(),
        kind: ModelKind::Sign,
        flags: ModelFlags::TWO_SIDED,
        bbox: [1, 10, 3],
        parts: vec![pudlo(
            PartName::BODY,
            Part::NO_PARENT,
            0b111,
            [-2, -20, 0],
            [1, 10, 3],
            SIGN,
        )],
        slots: vec![PaletteSlot { slot: SIGN, role: SlotRole::Sign }],
    }
}

/// Modele zastępcze pod kluczami, którymi nazywają się pliki w `data/models/`.
#[must_use]
pub fn all() -> Vec<VoxModel> {
    vec![
        citizen(),
        car(),
        // Wyposażenie wnętrz — klucze są kontraktem z `PropModels` w `engine/render`.
        // Regał 1,0 × 0,5 × 2,0 m, skrzynia 0,75 m sześcienna, biurko 1,5 × 0,75 × 0,75 m,
        // maszyna 2,0 × 1,25 × 1,75 m.
        prop("shelf", [4, 2, 8], BODY_SLOT, SlotRole::OutfitMain),
        prop("crate", [3, 3, 3], BODY_SLOT, SlotRole::OutfitTrim),
        prop("desk", [6, 3, 3], BODY_SLOT, SlotRole::Accent),
        prop("machine", [8, 5, 7], BODY_SLOT, SlotRole::Metal),
        sign(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_voxel::build_model_mesh;

    #[test]
    fn modele_zastepcze_przechodza_walidator() {
        for m in all() {
            m.validate().unwrap_or_else(|e| panic!("{}: {e}", m.key));
        }
    }

    /// L1 ma być **tańszy** od L0 i nadal widoczny — to jest cała treść poziomu detalu.
    ///
    /// Model jednobryłowy (regał, skrzynia, szyld) nie ma czego uprościć i jego L1 jest
    /// równy L0. To nie jest wyjątek od reguły, tylko jej granica: upraszcza się
    /// **hierarchię części**, a jedna część hierarchii nie tworzy. Wymaganie zostaje
    /// tam, gdzie ma sens — i tam właśnie jest sprawdzane.
    #[test]
    fn l1_jest_tanszy_od_l0_i_niepusty() {
        for m in all() {
            let l0 = build_model_mesh(&m, 0);
            let l1 = build_model_mesh(&m, 1);
            assert!(!l1.is_empty(), "{}: L1 pusty", m.key);
            if m.parts.len() == 1 {
                assert_eq!(
                    l1.indices.len(),
                    l0.indices.len(),
                    "{}: jedna część, a L1 różni się od L0",
                    m.key
                );
                continue;
            }
            assert!(
                l1.indices.len() < l0.indices.len(),
                "{}: L1 ma {} indeksów, L0 {}",
                m.key,
                l1.indices.len(),
                l0.indices.len()
            );
        }
    }

    /// Postać ma mieć 1,75 m. Wysokość liczy się z siatki, a nie z deklaracji `bbox`,
    /// bo to siatkę widzi gracz.
    #[test]
    fn mieszkaniec_ma_wzrost_czlowieka() {
        let m = build_model_mesh(&citizen(), 0);
        let (min, max) = m.bounds_qv;
        // Ćwiartka voxela to 0,0625 m.
        let wysokosc = f32::from(max[2] - min[2]) * 0.0625;
        assert!(
            (1.6..=1.9).contains(&wysokosc),
            "wzrost {wysokosc} m poza zakresem"
        );
        assert_eq!(min[2], 0, "postać nie stoi na gruncie");
    }

    /// Bryła ma być wyśrodkowana na origin w poziomie — inaczej `yaw` obraca model
    /// wokół punktu poza nim (auto jedzie dwa metry obok jezdni i zatacza łuk przy
    /// skręcie), a impostor wypalony z tej bryły wychodzi poza kafel.
    #[test]
    fn bryla_jest_wysrodkowana_w_poziomie() {
        for m in all() {
            let mesh = build_model_mesh(&m, 0);
            let (min, max) = mesh.bounds_qv;
            for os in 0..2 {
                let srodek = i32::from(min[os]) + i32::from(max[os]);
                assert!(
                    srodek.abs() <= 2,
                    "{}: oś {os} ma środek w {} ćwiartkach (min {}, max {})",
                    m.key,
                    srodek / 2,
                    min[os],
                    max[os]
                );
            }
            assert_eq!(min[2], 0, "{}: model nie stoi na origin", m.key);
        }
    }

    /// Postać i auto patrzą w tę samą stronę: są węższe wzdłuż `X` niż wzdłuż `Y`
    /// dla postaci (bark szerszy od klatki) i dłuższe wzdłuż `X` dla auta. Gdyby postać
    /// stała bokiem, pierwszy `yaw` z warstwy ruchu ustawiłby ją ramieniem do przodu.
    #[test]
    fn postac_patrzy_wzdluz_osi_x() {
        let m = build_model_mesh(&citizen(), 0);
        let (min, max) = m.bounds_qv;
        let glebokosc = max[0] - min[0];
        let szerokosc = max[1] - min[1];
        assert!(
            glebokosc < szerokosc,
            "postać ma {glebokosc} ćwiartek głębokości i {szerokosc} szerokości"
        );
        let a = build_model_mesh(&car(), 0);
        let (amin, amax) = a.bounds_qv;
        assert!(amax[0] - amin[0] > amax[1] - amin[1], "auto stoi w poprzek");
    }

    /// Auto ma być 4 m długie i mieć koła pod nadwoziem, a nie w nim.
    #[test]
    fn auto_ma_wymiary_auta() {
        let m = build_model_mesh(&car(), 0);
        let (min, max) = m.bounds_qv;
        let dlugosc = f32::from(max[0] - min[0]) * 0.0625;
        let szerokosc = f32::from(max[1] - min[1]) * 0.0625;
        let wysokosc = f32::from(max[2] - min[2]) * 0.0625;
        assert!((3.8..=4.2).contains(&dlugosc), "długość {dlugosc} m");
        assert!((1.6..=2.2).contains(&szerokosc), "szerokość {szerokosc} m");
        assert!((1.4..=1.7).contains(&wysokosc), "wysokość {wysokosc} m");
        assert_eq!(min[2], 0, "koła nie dotykają gruntu");
    }
}
