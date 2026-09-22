//! Macierz hashy miasta — dowód, że rozcięcie `lsystem.rs` niczego nie zmieniło.
//!
//! `R2-WP21` jest jedynym pakietem R2, w którym obowiązuje reguła R1, a nie R2:
//! **żaden hash nie zmienia się o bit**. Powód jest w kontrakcie samego generatora —
//! kolejność odsiewu ograniczeń i kolejność wpychania propozycji do kopca wchodzą
//! w `seq`, czyli w klucz porządku totalnego, czyli w każde miasto z każdego ziarna.
//! Podział, który je przestawia, jest zmianą świata przebraną za refaktor.
//!
//! Test wypisuje macierz i porównuje ją z **wpisaną tablicą wartości**, a nie sam
//! ze sobą: porównanie dwóch przebiegów po podziale przeszłoby także wtedy, gdy
//! podział przestawił wszystko, byle powtarzalnie. Tablica powstała z przebiegu
//! **przed** rozcięciem i to ona jest tu dowodem.

use magnat_jobs::JobPool;
use magnat_voxel::MaterialRegistry;
use magnat_world::{
    generate, generate_city, CityPlan, Difficulty, EconomyProfile, Epoch, Region, Terrain,
    WorldGenParams, WorldSize,
};
use std::sync::Arc;

/// Hashe miasta dla ziaren 1..=8 × cztery profile, zmierzone **przed** podziałem
/// `lsystem.rs` (R2e, 2026-09-21). Kolejność: ziarno rośnie wolniej niż profil.
const OCZEKIWANE: [&str; 32] = [
    "4be427e67b2fd741c25cd30cea293afa",
    "ec32bdde63d62da65e0fc9c2849fb07f",
    "9651066ffc78041825993395757f0606",
    "656469ce1a299d278fbab02312b789a0",
    "5ef061ec412d0f1e5aa9c3f54f4501f8",
    "0d61fd241d9f5afe40c11ee06a559365",
    "f787157e0a659b240268606e3d41ac4b",
    "06a5e0cb0e790a8042865c79e6731f34",
    "4506b811f8fbbcd8557ba86e9f50dff7",
    "e069479dadf6f7e105f23f548bcf07ed",
    "d2ebe0e1c4407f4f8ffcf9b90191ba9d",
    "d9cd9910b6a6462fce949e00921b4d55",
    "741e024a33e16563862775f4891642d7",
    "5e7e25e6b2743a1606a34e1afcb2ff55",
    "a772221c261c3eb15444b8b82f225a03",
    "3b15be28bc2aa1347c68da2818390318",
    "f3d1bc7253dfa6599bfc1c58b339a690",
    "6264dc1d0ec89d77bbdeee67ed95c40a",
    "3265a4e9dfff311f52b7c12acb267673",
    "4886d8d51aaaa7fde3f00a8c381b88b0",
    "e4af552e406027fb0a1f737783590306",
    "1fd9ec75107940ec2777e92a83a2fc77",
    "18b72889908e4ab957793e6e1d3fd65a",
    "be19649e6191ce350006312012929ce5",
    "ddd89904a8cf07b8f749df6825c08846",
    "8a154d8d16befad181c360a3b720c09a",
    "e25219ab6b542213e03a0eb28b0601fd",
    "5dd1cf81d25f8adfe4137640a314a978",
    "74737639e61726b92aa01f64ab3ee127",
    "b08bf6c2c25b33abd282a59f88a79542",
    "230f76007813fbbb5191b85f2045315f",
    "db7dbb3b0ca2a135ae620f88d87f8a36",
];

const PROFILE: [(EconomyProfile, Region); 4] = [
    (EconomyProfile::Mixed, Region::Lowland),
    (EconomyProfile::Industrial, Region::River),
    (EconomyProfile::Agricultural, Region::Lowland),
    (EconomyProfile::University, Region::River),
];

fn hash(seed: u64, region: Region, profile: EconomyProfile) -> String {
    let pool = JobPool::new(0);
    let params = WorldGenParams {
        seed,
        size: WorldSize::Small4km,
        region,
        epoch: Epoch::Y1990,
        profile,
        difficulty: Difficulty::Normal,
    };
    let (data, _) = generate(params, &pool).expect("generacja świata");
    let reg = Arc::new(MaterialRegistry::load_dir(&magnat_world::data_path("materials")).unwrap());
    let t = Terrain::new(data, reg);
    let plan = CityPlan::from_world(&params);
    let c = generate_city(&plan, &t, t.materials(), &pool).expect("generacja miasta");
    format!("{:032x}", c.report.city_hash.0)
}

#[test]
#[ignore = "N1.2: 32 generacje świata — uruchamiać jawnie przez --include-ignored"]
fn macierz_hashy_miasta_nie_drgnela_po_podziale() {
    let mut mam: Vec<String> = Vec::new();
    for seed in 1..=8u64 {
        for (profil, region) in PROFILE {
            mam.push(hash(seed, region, profil));
        }
    }
    let rozne: Vec<String> = mam
        .iter()
        .zip(OCZEKIWANE)
        .enumerate()
        .filter(|(_, (a, b))| a.as_str() != *b)
        .map(|(i, (a, b))| format!("#{i}: {a} zamiast {b}"))
        .collect();
    assert!(
        rozne.is_empty(),
        "rozcięcie lsystem.rs zmieniło {} z 32 hashy miasta:\n  {}",
        rozne.len(),
        rozne.join("\n  ")
    );
}
