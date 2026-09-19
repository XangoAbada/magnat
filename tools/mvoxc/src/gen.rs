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
#[must_use]
pub fn citizen() -> VoxModel {
    // Układ pionowy w voxelach: golenie 0–2, uda 2–4, tors 4–6, głowa 6–7.
    let parts = vec![
        // 0: tors — korzeń, metr nad gruntem.
        pudlo(PartName::TORSO, Part::NO_PARENT, 0b011, [0, 0, 16], [2, 1, 2], OUTFIT),
        // 1: głowa.
        pudlo(PartName::HEAD, 0, 0b011, [0, 0, 8], [2, 1, 1], SKIN),
        // 2–3: lewe ramię i przedramię (L0).
        pudlo(PartName::ARM_L, 0, 0b001, [-4, 0, 4], [1, 1, 1], OUTFIT),
        pudlo(PartName::FOREARM_L, 2, 0b001, [0, 0, -4], [1, 1, 1], SKIN),
        // 4–5: prawe.
        pudlo(PartName::ARM_R, 0, 0b001, [8, 0, 4], [1, 1, 1], OUTFIT),
        pudlo(PartName::FOREARM_R, 4, 0b001, [0, 0, -4], [1, 1, 1], SKIN),
        // 6–7: lewa noga.
        pudlo(PartName::THIGH_L, 0, 0b001, [0, 0, -8], [1, 1, 2], OUTFIT),
        pudlo(PartName::SHIN_L, 6, 0b001, [0, 0, -8], [1, 1, 2], TRIM),
        // 8–9: prawa noga.
        pudlo(PartName::THIGH_R, 0, 0b001, [4, 0, -8], [1, 1, 2], OUTFIT),
        pudlo(PartName::SHIN_R, 8, 0b001, [0, 0, -8], [1, 1, 2], TRIM),
        // 10–11: uproszczenia L1 — jedna bryła na ramiona, jedna na nogi.
        pudlo(PartName::ARMS, 0, 0b010, [-4, 0, 0], [4, 1, 2], OUTFIT),
        pudlo(PartName::LEGS, 0, 0b010, [0, 0, -16], [2, 1, 4], OUTFIT),
    ];
    VoxModel {
        key: "citizen".into(),
        kind: ModelKind::Character,
        flags: ModelFlags::default(),
        bbox: [4, 1, 7],
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
        pudlo(PartName::BODY, Part::NO_PARENT, 0b011, [0, 0, 4], [16, 7, 3], PAINT),
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

/// Modele zastępcze pod kluczami, którymi nazywają się pliki w `data/models/`.
#[must_use]
pub fn all() -> Vec<VoxModel> {
    vec![citizen(), car()]
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
    #[test]
    fn l1_jest_tanszy_od_l0_i_niepusty() {
        for m in all() {
            let l0 = build_model_mesh(&m, 0);
            let l1 = build_model_mesh(&m, 1);
            assert!(!l1.is_empty(), "{}: L1 pusty", m.key);
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
