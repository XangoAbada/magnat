//! Determinizm generatora świata — bramka 2 fazy M1 (00 §3.6, M1 §7.1).
//!
//! To są testy kontraktowe, nie jednostkowe: sprawdzają, że ten sam seed daje ten sam świat
//! niezależnie od liczby wątków i od przebiegu, bo na tym stoi wszystko dalsze — M2 stawia
//! parcele na tym terenie, a rozjazd o decymetr przesuwa granicę działki w zapisie gry.

use magnat_jobs::JobPool;
use magnat_world::{
    generate, Difficulty, EconomyProfile, Epoch, Region, WorldGenParams, WorldSize,
};

fn params(seed: u64, region: Region, size: WorldSize) -> WorldGenParams {
    WorldGenParams {
        seed,
        size,
        region,
        epoch: Epoch::Y1990,
        profile: EconomyProfile::Mixed,
        difficulty: Difficulty::Normal,
    }
}

#[test]
fn terrain_hash_stable() {
    let pool = JobPool::new(4);
    let p = params(0x00C0_FFEE, Region::River, WorldSize::Small4km);
    let (_, a) = generate(p, &pool).unwrap();
    let (_, b) = generate(p, &pool).unwrap();
    assert_eq!(a.terrain_hash, b.terrain_hash, "dwa przebiegi, dwa światy");
    assert_eq!(
        a.stats, b.stats,
        "statystyki się rozjechały mimo zgodnego hasha"
    );
}

#[test]
fn terrain_thread_invariant() {
    // Ryzyko R1 fazy M1: niedeterminizm `f32` w erozji. Gdyby redukcja składała się
    // po kolejności zakończenia zadań, ten test padłby natychmiast.
    let p = params(31_337, Region::Mountain, WorldSize::Small4km);
    let mut hashes = Vec::new();
    for threads in [1usize, 2, 8, 16] {
        let pool = JobPool::new(threads);
        let (_, r) = generate(p, &pool).unwrap();
        hashes.push((threads, r.terrain_hash));
    }
    let wzorzec = hashes[0].1;
    for (t, h) in &hashes {
        assert_eq!(*h, wzorzec, "{t} wątków dało inny teren");
    }
}

#[test]
fn seed_i_region_zmieniaja_swiat() {
    // Test odwrotny do powyższych: gdyby hash był stały niezależnie od wejścia, poprzednie
    // testy przechodziłyby, a generator nic by nie robił.
    let pool = JobPool::new(2);
    let a = generate(params(1, Region::Lowland, WorldSize::Small4km), &pool)
        .unwrap()
        .1
        .terrain_hash;
    let b = generate(params(2, Region::Lowland, WorldSize::Small4km), &pool)
        .unwrap()
        .1
        .terrain_hash;
    let c = generate(params(1, Region::Mountain, WorldSize::Small4km), &pool)
        .unwrap()
        .1
        .terrain_hash;
    assert_ne!(a, b, "zmiana ziarna nie zmieniła świata");
    assert_ne!(a, c, "zmiana regionu nie zmieniła świata");
}

/// Tabela hashy dla siatki seed × region. Zmiana którejkolwiek wartości oznacza, że każdy
/// świat wygenerowany wcześniej z tego samego ziarna wygląda teraz inaczej — a więc zapisy
/// gry przestały być wczytywalne w sensie, o który chodzi (M1 §7.1 `terrain_hash_matrix`).
///
/// **Zatwierdzanie zmiany:** `cargo test -p magnat-world --release -- --ignored --nocapture
/// wypisz_macierz_hashy` i przeniesienie wyniku tutaj, razem z wpisem w dzienniku.
#[test]
#[ignore = "160 generacji (~95 s w release) — CI uruchamia w jobie determinizmu"]
fn terrain_hash_matrix() {
    // 32 seedy × 5 regionów przy rozmiarze 4 km (§7.1). **Korekta wobec planu:** plan
    // wymienia także cztery rozmiary, czyli 640 generacji — to dwie godziny na przebieg
    // i tabela, której nikt nie zatwierdzi świadomie. Wpływ rozmiaru na hash pilnuje
    // osobno `seed_i_region_zmieniaja_swiat`, a sam mechanizm generacji jest wspólny
    // dla wszystkich rozmiarów.
    const OCZEKIWANE: &[(u64, Region, u128)] = &[
        (1, Region::Coastal, 0x299A3EDE761BF79A085D26154FBEE747),
        (1, Region::Mountain, 0x56C9E75C3C8569C52233532034C52ABF),
        (1, Region::Lowland, 0xA07FFF883F7D37C67C386CF08392FB04),
        (1, Region::River, 0x51048F60DE5410C14A1B25AD0734920C),
        (1, Region::Desert, 0x21E289D37C08015EC8630F48EAFA60CA),
        (2, Region::Coastal, 0x40CD8BE992891230A7B37482EC521D24),
        (2, Region::Mountain, 0x6BD0536C2FA60341ABCEAF5C56320272),
        (2, Region::Lowland, 0xF3D0C6BCEE86CDC5A5C0A16578B7C488),
        (2, Region::River, 0x307FF9B1A334C66283EEC82478D03E03),
        (2, Region::Desert, 0x13776B26FDBF0596E599CA2787C305F1),
        (3, Region::Coastal, 0x21BC4104530C23E64B471F2FE093F502),
        (3, Region::Mountain, 0x456B56AFDA58ACFB8AFE35280751CBAA),
        (3, Region::Lowland, 0xB5FE6F9268666E49ED149528AC625AE9),
        (3, Region::River, 0x5BE296D9889BEF03A2ADFC1129E2EFA5),
        (3, Region::Desert, 0x5172311980960EBAD8E20371FE424B2F),
        (7, Region::Coastal, 0x8C49533A77F60BC65391E430F2758FE6),
        (7, Region::Mountain, 0x963BB156D585F6C09A0C636E866DD63B),
        (7, Region::Lowland, 0xB073305B8783241AE3E8BD650F1A4F8B),
        (7, Region::River, 0x3B3365EEC49353968C2DA7A8284D6DD0),
        (7, Region::Desert, 0x98E6E02FE451ED706678A63B1D3D2C2A),
        (42, Region::Coastal, 0x3B53447E1DD84C8E9C11766DF8FA6965),
        (42, Region::Mountain, 0x45E070D79D910AC3260361D0CA068AE6),
        (42, Region::Lowland, 0x2C08E9B80C1A93E0540B8ADEB6C9B652),
        (42, Region::River, 0x615F0ABF5CD9516D53C4F4515121E064),
        (42, Region::Desert, 0x58C435984A3B2188F9A91F7497770973),
        (99, Region::Coastal, 0x5D20DF4ED72889822088A81F3A0184B9),
        (99, Region::Mountain, 0xB1E452555914352EEAFA2DAC9B782935),
        (99, Region::Lowland, 0x9BB353EF295528EFAA658C4B097A1816),
        (99, Region::River, 0x8C71E3C721029794702F8E2B582DE230),
        (99, Region::Desert, 0xAC53F233AD40DEAFA42F267E1C6521D8),
        (255, Region::Coastal, 0x4E4053A1FD5BB34BE50F8A4D716811DA),
        (255, Region::Mountain, 0xF978E521757B8B23822EB1A5C8F9A8AA),
        (255, Region::Lowland, 0xD57E9F6E9C2DA3603F8BB2849665BC59),
        (255, Region::River, 0x5FF22990A4EAD456E33F89ACACEB7C11),
        (255, Region::Desert, 0x3A202F501B9BAE76D5010301CD01E148),
        (256, Region::Coastal, 0xB94BDEBE2833F03D335D3475A4165855),
        (256, Region::Mountain, 0xC40F8DC044639DBD4AF1FB326EB2B450),
        (256, Region::Lowland, 0x435CB158F9542EF660C7C3F6F513F563),
        (256, Region::River, 0x2380339C61DA48BE5D59EFFF3276F44E),
        (256, Region::Desert, 0x1030CFA7BA410496B0BE3F1C88275698),
        (1000, Region::Coastal, 0xD3E622562BE2B335DF17D6400F826B74),
        (1000, Region::Mountain, 0x1FB5162E4D237A8AD68A2D52DE564D8F),
        (1000, Region::Lowland, 0xB8569F3519A9A27876914C7B678DDDE9),
        (1000, Region::River, 0x2FFFCC550B2E16CA544A6BD2B013AF81),
        (1000, Region::Desert, 0x54B1D1AF82BBC0E736A3FBD7919C0146),
        (4096, Region::Coastal, 0x398EB13E0D8C1F1D4E7018478B698947),
        (4096, Region::Mountain, 0x131335F3155AC6E59F0355C467428C9D),
        (4096, Region::Lowland, 0xBC271977B8340CFAD81AD6A4CAB9146A),
        (4096, Region::River, 0x58D8E64239F6BB5C05ABFE4B789DEB2C),
        (4096, Region::Desert, 0x5680F290E8D2779207C205BA8B445A67),
        (31337, Region::Coastal, 0xE85D2080A859ED25CB692465EAAAC26B),
        (31337, Region::Mountain, 0x2FC74D511D1744C403936896879CCA43),
        (31337, Region::Lowland, 0x9664FDF08D9A07542D3E30C96D1276E4),
        (31337, Region::River, 0x29EE1DA72B357089C48E0A78CFC7DCC9),
        (31337, Region::Desert, 0xDEE8E9A2E72D5B9A5A75AF5FD265A09C),
        (65535, Region::Coastal, 0xF784C1AAE00B06047529520F3BBF677C),
        (65535, Region::Mountain, 0xCDEFF04188B7E4E41C0CA3DC19B3B29F),
        (65535, Region::Lowland, 0x15F8802FB9A76C0BE57A3058601CC4A9),
        (65535, Region::River, 0xEDFD97CF7421E599B4D66E11209041D7),
        (65535, Region::Desert, 0x819968B72693999672A99FD3345FDFA5),
        (65536, Region::Coastal, 0x922AF67BF24DF19E76B8A2B5557130C1),
        (65536, Region::Mountain, 0x95E5ECB0FCF5A2CAF9C204561A0E3025),
        (65536, Region::Lowland, 0x88AFEDD261E1CB9AE00FDED281E4E1C6),
        (65536, Region::River, 0x64E38AC42164F754D404E6D05165B688),
        (65536, Region::Desert, 0xB564ED19335296F2D67301F1D2FB5504),
        (123456, Region::Coastal, 0xBE34A4AF163320801398DDCEBA077A95),
        (123456, Region::Mountain, 0x22DC2E361A0B967FB33E85FD09733DE7),
        (123456, Region::Lowland, 0x9864ADFB151D8C990E7381FB3C1733CC),
        (123456, Region::River, 0x63F18409D6AB1760D651D9EAAC57D620),
        (123456, Region::Desert, 0x5A1FC0476F0E2B43DB5B19D01851DBD7),
        (999983, Region::Coastal, 0x60089838B83F3EB23ECF4DF3F692C631),
        (999983, Region::Mountain, 0x5DB62A64D019070F09E46D7724752095),
        (999983, Region::Lowland, 0x895C98D6916D2645B1A7418786A0F7FF),
        (999983, Region::River, 0x21F31C6A4B9F3B0F147766C4610F276F),
        (999983, Region::Desert, 0x17F70DA3A15DADFC618CDD0BAB5A7309),
        (
            12648430,
            Region::Coastal,
            0xF4EA206A986553F06B41A63FC0D1CE87,
        ),
        (
            12648430,
            Region::Mountain,
            0x40E964F0FB8F1853828A264FC8CC5FC0,
        ),
        (
            12648430,
            Region::Lowland,
            0xC1C75E76A82772D3CF2B4BCB75F82BE7,
        ),
        (12648430, Region::River, 0x8FF34C7BD9678E3E8A386CA5DDCBE492),
        (12648430, Region::Desert, 0x9D9AAB06C8C91C290C8F466630C69983),
        (
            233811181,
            Region::Coastal,
            0x745C5A4A47B31CBA5570B559BA80ECED,
        ),
        (
            233811181,
            Region::Mountain,
            0x1A1D1FF0046B7EC5F53B48791B198A56,
        ),
        (
            233811181,
            Region::Lowland,
            0xBFC84D9B0A7E8839E1F7CF505C483A06,
        ),
        (233811181, Region::River, 0xDAA98FD838C3A5422D6A890D399D5C1F),
        (
            233811181,
            Region::Desert,
            0x2E7127ADDABC52A727028357458A0ACB,
        ),
        (
            305419896,
            Region::Coastal,
            0x8F4ED9B37BDFD07A085CAB014145CD5E,
        ),
        (
            305419896,
            Region::Mountain,
            0xB12B40600EF965928FFE76D0A21CB647,
        ),
        (
            305419896,
            Region::Lowland,
            0x552DD253128E72C5780B15718089DED8,
        ),
        (305419896, Region::River, 0xA083B0001338EBC17EAF2C2DDBEF476B),
        (
            305419896,
            Region::Desert,
            0x4D774229A7E783B29F2B63F9173B3C67,
        ),
        (
            2147483648,
            Region::Coastal,
            0xFFB43181EB262FF9827EC986EF8A6C54,
        ),
        (
            2147483648,
            Region::Mountain,
            0x595393F4C8FA08066D7B3EDDF148CE76,
        ),
        (
            2147483648,
            Region::Lowland,
            0x2F955970B0FD7161EC2C889555F48B80,
        ),
        (
            2147483648,
            Region::River,
            0xBFE22285C69902177FA60021DEF9BD45,
        ),
        (
            2147483648,
            Region::Desert,
            0xEF7ACFFC36D02C1C704B5CC95D9DD334,
        ),
        (
            3735928559,
            Region::Coastal,
            0xFC928203312EF3626A66BAAC6E5F3358,
        ),
        (
            3735928559,
            Region::Mountain,
            0xE9A25B337A203A56E6BFB1CF345B1094,
        ),
        (
            3735928559,
            Region::Lowland,
            0x93A7832E47C5604179313EFCC92C0262,
        ),
        (
            3735928559,
            Region::River,
            0xCBEA1D15A8AE0BBFE51288BBA6EC1936,
        ),
        (
            3735928559,
            Region::Desert,
            0xC1BC079F5288E952F32E55E85AC01738,
        ),
        (
            4294967295,
            Region::Coastal,
            0xDA8E31D50D6522FDCE2A11A899879962,
        ),
        (
            4294967295,
            Region::Mountain,
            0x09B4EEC111E4E0D8CD65B0369DCF9D8C,
        ),
        (
            4294967295,
            Region::Lowland,
            0x309C2BE18D01B0B4773DDC8D3151331D,
        ),
        (
            4294967295,
            Region::River,
            0xC958F236C340B89C3233B6F0EAF0BDDB,
        ),
        (
            4294967295,
            Region::Desert,
            0xA38FEEDD3BF32ABF3643DA089CC1B1A2,
        ),
        (
            4294967296,
            Region::Coastal,
            0x48E1FF87478C6219CA742640E1686606,
        ),
        (
            4294967296,
            Region::Mountain,
            0x6B123B4742534296AEFF27EB28A8F042,
        ),
        (
            4294967296,
            Region::Lowland,
            0x8693CB3B77E570506BD325EE2852D92F,
        ),
        (
            4294967296,
            Region::River,
            0x9F9C36777A521973A277E7B0C6F17CF8,
        ),
        (
            4294967296,
            Region::Desert,
            0x253F705021AE3344FC2FBA1545E9CCF2,
        ),
        (
            93824992236885,
            Region::Coastal,
            0x2701A0D5A039B239F8D215C28284EF9B,
        ),
        (
            93824992236885,
            Region::Mountain,
            0xEE4897DA02499CFDB4C92F8B6580ABCD,
        ),
        (
            93824992236885,
            Region::Lowland,
            0x363863ACDE80B1B04CFF709E437E5B10,
        ),
        (
            93824992236885,
            Region::River,
            0xAB6FE2748C0F0D77020084DB9A84BDF1,
        ),
        (
            93824992236885,
            Region::Desert,
            0x6573F89F391E9013C2BFBFED00FEC110,
        ),
        (
            187649984473770,
            Region::Coastal,
            0x69FD8C3C0777C6F767D9D0F3D741A2D1,
        ),
        (
            187649984473770,
            Region::Mountain,
            0xD5C5C91CEE0B0CFDF3313ED1F1051E7A,
        ),
        (
            187649984473770,
            Region::Lowland,
            0x007BFD507965E1F9006E8A824076CDE8,
        ),
        (
            187649984473770,
            Region::River,
            0xDF136A6D07841A19FC16855144D19919,
        ),
        (
            187649984473770,
            Region::Desert,
            0x9076BE23573E9EABACC637585C82C348,
        ),
        (
            81985529216486895,
            Region::Coastal,
            0xF955D5551BB9B2B782C674A1ABB22E9E,
        ),
        (
            81985529216486895,
            Region::Mountain,
            0x886DD2786E8DCD06972464904BE7722D,
        ),
        (
            81985529216486895,
            Region::Lowland,
            0xDEF86E469EFCC2E4DDDF4A9FD91AC3C6,
        ),
        (
            81985529216486895,
            Region::River,
            0x8D95CECC4F1DEB845271F6594423C402,
        ),
        (
            81985529216486895,
            Region::Desert,
            0xDCFEC35FFA5BC711B8FA8BFD4B17EB7D,
        ),
        (
            9223372036854775807,
            Region::Coastal,
            0xED75668E7A59F3CB3B04AFB65E205E4F,
        ),
        (
            9223372036854775807,
            Region::Mountain,
            0x9C97111B66ECD0F9BB319E16489520E8,
        ),
        (
            9223372036854775807,
            Region::Lowland,
            0xB2860C6A047A63D789D9964BE4602FD9,
        ),
        (
            9223372036854775807,
            Region::River,
            0xDED676A9D72699E04F25A8132C5F4772,
        ),
        (
            9223372036854775807,
            Region::Desert,
            0xEA43D380DF2E6EB6CB9C1AD78753F7F1,
        ),
        (
            9223372036854775808,
            Region::Coastal,
            0xEA2AC28754585DFAC30BBBDE9C76296B,
        ),
        (
            9223372036854775808,
            Region::Mountain,
            0x984EECE344F715DFC5A16C2AC9918482,
        ),
        (
            9223372036854775808,
            Region::Lowland,
            0xDD84D8D9A06BB6E4D2449D3D64729876,
        ),
        (
            9223372036854775808,
            Region::River,
            0x6225D7A78288CF238757D00A1B5F5CE7,
        ),
        (
            9223372036854775808,
            Region::Desert,
            0x128DC3E2EDD459B7F363952BA3486D46,
        ),
        (
            18446744073709551615,
            Region::Coastal,
            0xC6E76B1411D404A148B7D08D4733E136,
        ),
        (
            18446744073709551615,
            Region::Mountain,
            0x3DF33A701B64C00F12F479D1904FB925,
        ),
        (
            18446744073709551615,
            Region::Lowland,
            0xE42B4797F572A85097D2025DF3F904CD,
        ),
        (
            18446744073709551615,
            Region::River,
            0x429181DAB0E908A3D99884E3C4D0607E,
        ),
        (
            18446744073709551615,
            Region::Desert,
            0x59A899EC6797327C464BFA19E99391EE,
        ),
        (
            11111111,
            Region::Coastal,
            0x3555ED74B820F2786426727C08D97C53,
        ),
        (
            11111111,
            Region::Mountain,
            0x12EEBDBF61BFBD052A35C85478BA3163,
        ),
        (
            11111111,
            Region::Lowland,
            0xA929AD1E801943C5BF3E8AD4CAB62C63,
        ),
        (11111111, Region::River, 0xA1DA9CD2D7B17DC74E035DBEB7A373CC),
        (11111111, Region::Desert, 0xA71D0F3DB0FE76B86000E8435EAE8B36),
        (
            22222222,
            Region::Coastal,
            0x80A7F95319C566106867A403496C5A3E,
        ),
        (
            22222222,
            Region::Mountain,
            0xA1C89C3AC8BE0BBD87B7490D49DA50D6,
        ),
        (
            22222222,
            Region::Lowland,
            0x0593D7CA61B4C22D6C3AF2EA6705741D,
        ),
        (22222222, Region::River, 0x5B46A265A3554CB92C98A9EF6882EAE9),
        (22222222, Region::Desert, 0xF36A88FCA6022AD8608A3F5F1BB4D6B9),
        (
            314159265,
            Region::Coastal,
            0x0F7596D250CEA1D80ED42C1D0D027946,
        ),
        (
            314159265,
            Region::Mountain,
            0x08BBEF3B09F494A3FEB26ADBBB4A6BB8,
        ),
        (
            314159265,
            Region::Lowland,
            0xB39FCFB9A8A21633BE686F20B600D7B5,
        ),
        (314159265, Region::River, 0x0CC61C2271CF36E3EE3BC764A9B17DA4),
        (
            314159265,
            Region::Desert,
            0x1A6741B9CB028B72F4CEEA7EAAA3F6BD,
        ),
        (
            271828182,
            Region::Coastal,
            0xECCE08D044B2EA0A2D903F340B91339F,
        ),
        (
            271828182,
            Region::Mountain,
            0x6C983B65430EBABA240EE791E67505B2,
        ),
        (
            271828182,
            Region::Lowland,
            0xE4DF249773A1C7914E808EFA79E72848,
        ),
        (271828182, Region::River, 0x0C599BCB69A83A4EAE4CD6431BE83595),
        (
            271828182,
            Region::Desert,
            0x0A52497F1DEF233A02C9329519B5F400,
        ),
    ];
    let pool = JobPool::new(2);
    for (seed, region, want) in OCZEKIWANE {
        let (_, r) = generate(params(*seed, *region, WorldSize::Small4km), &pool).unwrap();
        assert_eq!(
            r.terrain_hash.0,
            *want,
            "seed {seed}, region {}",
            region.key()
        );
    }
}

/// Seedy macierzy. Trzydzieści dwa, jak w §7.1 — wartości „okrągłe" (potęgi dwójki,
/// powtarzalne cyfry) obok przypadkowych, bo generator dzieli seed na strumienie i słaby
/// podział objawiłby się właśnie na wartościach o regularnych bitach.
const SEEDY: [u64; 32] = [
    1,
    2,
    3,
    7,
    42,
    99,
    255,
    256,
    1_000,
    4_096,
    31_337,
    65_535,
    65_536,
    123_456,
    999_983,
    0x00C0_FFEE,
    0x0DEF_ACED,
    0x1234_5678,
    0x8000_0000,
    0xDEAD_BEEF,
    0xFFFF_FFFF,
    0x1_0000_0000,
    0x5555_5555_5555,
    0xAAAA_AAAA_AAAA,
    0x0123_4567_89AB_CDEF,
    0x7FFF_FFFF_FFFF_FFFF,
    0x8000_0000_0000_0000,
    0xFFFF_FFFF_FFFF_FFFF,
    11_111_111,
    22_222_222,
    314_159_265,
    271_828_182,
];

#[test]
#[ignore = "narzędzie, nie test: wypisuje macierz hashy do zatwierdzenia"]
fn wypisz_macierz_hashy() {
    let pool = JobPool::new(4);
    for seed in SEEDY {
        for region in Region::ALL {
            let (_, r) = generate(params(seed, *region, WorldSize::Small4km), &pool).unwrap();
            println!(
                "    ({seed}, Region::{:?}, 0x{:032X}),",
                region, r.terrain_hash.0
            );
        }
    }
}
