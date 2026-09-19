//! `mvoxc` — kompilator i walidator modeli voxelowych (M11a, WP1).
//!
//! Trzy polecenia i każde odpowiada na inne pytanie:
//!
//! - `import` — „mam plik z MagicaVoxela, zrób z niego model gry". Kształty bierze
//!   z `.vox`, wszystko, co jest modelem **gry** (hierarchia, stawy, poziomy detalu,
//!   role palety) — ze specyfikacji `<model>.ron` leżącej obok.
//! - `check` — „czy ten `.mvox` jest poprawny i co w nim jest". Uruchamia walidator
//!   i wypisuje części, poziomy detalu i liczby trójkątów. To jest narzędzie do
//!   odpowiadania na pytanie „dlaczego tego nie widać".
//! - `gen` — „nie mam jeszcze pliku artysty". Zapisuje modele zastępcze z `gen.rs`.
//!
//! Kod wyjścia: 0 = w porządku, 1 = błąd danych, 2 = błąd wywołania.

mod gen;
mod spec;
mod vox;

use clap::{Parser, Subcommand};
use magnat_voxel::{build_model_mesh, ModelLibrary, Part, VoxModel, LOD_COUNT};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(name = "mvoxc", about = "Kompilator modeli voxelowych .mvox (M11).")]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// MagicaVoxel `.vox` + specyfikacja `.ron` → `.mvox`.
    Import {
        /// Plik `.vox`. Specyfikacji szuka obok, pod tą samą nazwą z `.ron`.
        vox: PathBuf,
        /// Plik wyjściowy; domyślnie `data/models/<nazwa>.mvox`.
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Inna ścieżka specyfikacji niż domyślna.
        #[arg(long)]
        spec: Option<PathBuf>,
    },
    /// Sprawdza `.mvox` albo cały katalog i wypisuje, co w nim jest.
    Check {
        /// Plik albo katalog; domyślnie `data/models/`.
        path: Option<PathBuf>,
    },
    /// Zapisuje modele zastępcze do katalogu (domyślnie `data/models/`).
    Gen {
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    let args = Args::parse();
    let wynik = match args.cmd {
        Cmd::Import { vox, out, spec } => import(&vox, out.as_deref(), spec.as_deref()),
        Cmd::Check { path } => check(path.unwrap_or_else(models_dir).as_path()),
        Cmd::Gen { out } => generate(out.unwrap_or_else(models_dir).as_path()),
    };
    match wynik {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mvoxc: {e}");
            ExitCode::FAILURE
        }
    }
}

fn models_dir() -> PathBuf {
    magnat_core::assets::data_path("models")
}

fn import(vox_path: &Path, out: Option<&Path>, spec_path: Option<&Path>) -> Result<(), String> {
    let key = vox_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .ok_or_else(|| format!("{} nie ma nazwy", vox_path.display()))?;
    let spec_path = spec_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| vox_path.with_extension("ron"));

    let bajty = std::fs::read(vox_path).map_err(|e| format!("{}: {e}", vox_path.display()))?;
    let plik = vox::read(&bajty).map_err(|e| format!("{}: {e}", vox_path.display()))?;
    let tekst = std::fs::read_to_string(&spec_path)
        .map_err(|e| format!("{}: {e}", spec_path.display()))?;
    let s = spec::ImportSpec::parse(&tekst).map_err(|e| format!("{}: {e}", spec_path.display()))?;

    if s.parts.len() != plik.shapes.len() {
        return Err(format!(
            "{}: {} modeli w .vox, {} części w specyfikacji — mają odpowiadać sobie po kolei",
            vox_path.display(),
            plik.shapes.len(),
            s.parts.len()
        ));
    }

    let mut parts = Vec::with_capacity(s.parts.len());
    let mut bbox = [0u8; 3];
    for (i, ksztalt) in plik.shapes.iter().enumerate() {
        let dims = [
            ksztalt.dims[0] as u8,
            ksztalt.dims[1] as u8,
            ksztalt.dims[2] as u8,
        ];
        let mut voxels = vec![0u8; dims[0] as usize * dims[1] as usize * dims[2] as usize];
        for v in &ksztalt.voxels {
            let slot = *s.colors.get(&v[3]).ok_or_else(|| {
                let c = plik.palette[v[3] as usize];
                format!(
                    "część `{}`: barwa nr {} (RGB {},{},{}) nie ma przypisanego slotu — \
                     dopisz ją do `colors` w specyfikacji",
                    s.parts[i].name, v[3], c[0], c[1], c[2]
                )
            })?;
            // Współrzędna poza zadeklarowanym pudełkiem jest **błędem pliku**, a nie
            // paniką: reszta czytnika konsekwentnie zwraca `VoxError`, a plik z cudzego
            // edytora ma prawo być uszkodzony.
            if v[0] >= dims[0] || v[1] >= dims[1] || v[2] >= dims[2] {
                return Err(format!(
                    "część `{}`: voxel ({}, {}, {}) leży poza pudełkiem {:?}",
                    s.parts[i].name, v[0], v[1], v[2], dims
                ));
            }
            let idx = usize::from(v[0])
                + dims[0] as usize * (usize::from(v[1]) + dims[1] as usize * usize::from(v[2]));
            voxels[idx] = slot;
        }
        for k in 0..3 {
            bbox[k] = bbox[k].max(dims[k]);
        }
        parts.push(Part {
            name: s.part_name(i)?,
            parent: s.parent_index(i)?,
            lod_mask: s.lod_mask(i)?,
            pivot: [s.parts[i].pivot.0, s.parts[i].pivot.1, s.parts[i].pivot.2],
            dims,
            voxels,
        });
    }

    let model = VoxModel {
        key: key.clone(),
        kind: s.kind()?,
        flags: s.flags()?,
        bbox,
        parts,
        slots: s.slots()?,
    };
    model.validate().map_err(|e| e.to_string())?;

    let out = out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| models_dir().join(format!("{key}.mvox")));
    zapisz(&out, &model)?;
    println!(
        "{} → {} ({} części, {} B)",
        vox_path.display(),
        out.display(),
        model.parts.len(),
        model.write().len()
    );
    opisz(&model);
    Ok(())
}

fn check(path: &Path) -> Result<(), String> {
    let modele: Vec<VoxModel> = if path.is_dir() {
        let lib = ModelLibrary::load_dir(path).map_err(|e| e.to_string())?;
        if lib.is_empty() {
            return Err(format!("{}: ani jednego pliku .mvox", path.display()));
        }
        lib.iter().map(|(_, m)| m.clone()).collect()
    } else {
        let key = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let buf = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        vec![VoxModel::read(&key, &buf).map_err(|e| e.to_string())?]
    };
    for m in &modele {
        m.validate().map_err(|e| format!("{}: {e}", m.key))?;
        opisz(m);
    }
    println!("{} model(e) w porządku", modele.len());
    Ok(())
}

fn generate(out: &Path) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    for m in gen::all() {
        m.validate().map_err(|e| format!("{}: {e}", m.key))?;
        let p = out.join(format!("{}.mvox", m.key));
        zapisz(&p, &m)?;
        println!("{} ({} części)", p.display(), m.parts.len());
        opisz(&m);
    }
    Ok(())
}

fn zapisz(path: &Path, m: &VoxModel) -> Result<(), String> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    std::fs::write(path, m.write()).map_err(|e| format!("{}: {e}", path.display()))
}

/// Wypisuje, co w modelu jest. To jest odpowiedź na „dlaczego tego nie widać":
/// pusty poziom detalu i część bez voxeli widać tu od razu.
fn opisz(m: &VoxModel) {
    let role: Vec<&str> = m.roles_used().iter().map(|r| r.key()).collect();
    println!(
        "  {} [{:?}] bbox {:?} · role: {}",
        m.key,
        m.kind,
        m.bbox,
        role.join(", ")
    );
    for lod in 0..LOD_COUNT {
        let siatka = build_model_mesh(m, lod);
        let czesci: Vec<String> = m.parts_in_lod(lod).map(|(_, p)| p.name.key()).collect();
        println!(
            "  L{lod}: {} trójkątów, {} części [{}]",
            siatka.indices.len() / 3,
            czesci.len(),
            czesci.join(" ")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pełna droga WP1: `.vox` + specyfikacja → `.mvox` → z powrotem model.
    /// Test robi to w pamięci, bo chodzi o przekład, a nie o system plików.
    #[test]
    fn import_z_voxa_daje_model_z_hierarchia_i_slotami() {
        // Dwie części: pudełko 2×1×2 i pudełko 2×1×1 nad nim.
        let tors: Vec<[u8; 4]> = (0..2)
            .flat_map(|z| (0..2u8).map(move |x| [x, 0, z, 1]))
            .collect();
        let glowa: Vec<[u8; 4]> = (0..2u8).map(|x| [x, 0, 0, 2]).collect();

        let mut bajty = crate::vox::tests::maly_vox([2, 1, 2], &tors);
        // Doklejamy drugą parę SIZE/XYZI przez sklejenie dwóch plików: bierzemy
        // dzieci drugiego i dopisujemy do pierwszego, poprawiając rozmiar MAIN.
        let drugi = crate::vox::tests::maly_vox([2, 1, 1], &glowa);
        let ogon = &drugi[20..];
        let stary = u32::from_le_bytes([bajty[16], bajty[17], bajty[18], bajty[19]]);
        let nowy = stary + ogon.len() as u32;
        bajty[16..20].copy_from_slice(&nowy.to_le_bytes());
        bajty.extend_from_slice(ogon);

        let plik = crate::vox::read(&bajty).expect("vox");
        assert_eq!(plik.shapes.len(), 2);

        let s = spec::ImportSpec::parse(
            r#"(
                schema_version: 1,
                kind: "character",
                slots: {1: "outfit_main", 2: "skin"},
                colors: {1: 1, 2: 2},
                parts: [
                    (name: "torso", pivot: (0,0,16), lods: [0,1]),
                    (name: "head", parent: "torso", pivot: (0,0,8), lods: [0,1]),
                ],
            )"#,
        )
        .expect("spec");

        let mut parts = Vec::new();
        for (i, k) in plik.shapes.iter().enumerate() {
            let dims = [k.dims[0] as u8, k.dims[1] as u8, k.dims[2] as u8];
            let mut voxels = vec![0u8; dims[0] as usize * dims[1] as usize * dims[2] as usize];
            for v in &k.voxels {
                let idx = usize::from(v[0])
                    + dims[0] as usize * (usize::from(v[1]) + dims[1] as usize * usize::from(v[2]));
                voxels[idx] = s.colors[&v[3]];
            }
            parts.push(Part {
                name: s.part_name(i).unwrap(),
                parent: s.parent_index(i).unwrap(),
                lod_mask: s.lod_mask(i).unwrap(),
                pivot: [s.parts[i].pivot.0, s.parts[i].pivot.1, s.parts[i].pivot.2],
                dims,
                voxels,
            });
        }
        let m = VoxModel {
            key: "t".into(),
            kind: s.kind().unwrap(),
            flags: s.flags().unwrap(),
            bbox: [2, 1, 3],
            parts,
            slots: s.slots().unwrap(),
        };
        m.validate().expect("model z importu ma być poprawny");
        // Odczyt z powrotem: format przeżywa podróż w obie strony.
        assert_eq!(VoxModel::read("t", &m.write()).unwrap(), m);
        // Głowa siedzi nad torsem, bo pivot się zsumował.
        let off = magnat_voxel::rest_offsets(&m);
        assert_eq!(off[1][2], 24);
    }

    /// Barwa bez przypisanego slotu ma **zatrzymać** import, a nie zniknąć z modelu.
    #[test]
    fn barwa_bez_slotu_zatrzymuje_import() {
        let s = spec::ImportSpec::parse(
            r#"(schema_version: 1, kind: "prop", slots: {1: "metal"},
                colors: {1: 1}, parts: [(name: "body", pivot: (0,0,0), lods: [0,1,2])])"#,
        )
        .expect("spec");
        assert!(!s.colors.contains_key(&9), "barwa 9 nie ma slotu");
    }
}
