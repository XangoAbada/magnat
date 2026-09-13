// Daleki teren — siatka wysokościowa poza zasięgiem chunków (M1 §5.8, WP-R5).
//
// Pierścienie LOD sięgają kilku kilometrów; dalej mapa ma jeszcze kilkanaście kilometrów,
// których nie ma sensu materializować na voxele — z takiej odległości jeden voxel to
// ułamek piksela. Zamiast tego rysujemy jedną siatkę o stałym oczku, **przyciągniętą do
// swojej siatki** (żeby nie pełzała przy ruchu kamery) i próbkującą tę samą mapę wysokości
// 4 m, z której powstał teren voxelowy. Spójność w miejscu przejścia bierze się właśnie
// stąd: to jedno źródło prawdy o wysokości (K-13), a nie druga jego kopia.
//
// Siatka jest **obniżona** o pół metra względem terenu voxelowego. To nie jest hack na
// z-fighting, tylko konsekwencja tego, czym jest voxel: powierzchnia voxelowa leży na
// górnej ścianie kostki, a interpolowana siatka przechodzi przez jej środek.

struct Frame {
    view_proj: mat4x4<f32>,
    light_view_proj: array<mat4x4<f32>, 4>,
    cascade_far: vec4<f32>,
    cascade_texel: vec4<f32>,
    sun_dir: vec4<f32>,
    sun_color: vec4<f32>,
    sky_color: vec4<f32>,
    ground_color: vec4<f32>,
    fog: vec4<f32>,
    clip: vec4<f32>,
    screen: vec4<f32>,
    eye: vec4<f32>,
    overlay: vec4<f32>,
}

/// Opis siatki dalekiego terenu.
struct FarCfg {
    // x = bok komórki mapy w metrach, y = wymiar mapy w komórkach,
    // z = oczko siatki w metrach, w = liczba quadów na bok
    grid: vec4<f32>,
    // xy = środek siatki w świecie, **zatrzaśnięty** do oczka; z = promień wycięcia
    snap: vec4<f32>,
    // xyz = pozycja kamery w świecie
    eye: vec4<f32>,
}

@group(0) @binding(0) var<uniform> frame: Frame;
@group(1) @binding(0) var<uniform> cfg: FarCfg;
@group(1) @binding(1) var height_map: texture_2d<i32>;
@group(1) @binding(2) var albedo_map: texture_2d<f32>;
@group(2) @binding(0) var overlay_field: texture_2d<f32>;
@group(2) @binding(1) var overlay_palette: texture_2d<f32>;

struct VertexOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) view_dist: f32,
    @location(3) swiat_xy: vec2<f32>,
}

/// Komórka mapy dla punktu świata, zaciśnięta do jej granic: świat kończy się krawędzią,
/// a nie dziurą.
fn komorka(swiat: vec2<f32>) -> vec2<i32> {
    let dim = i32(cfg.grid.y);
    return clamp(vec2<i32>(swiat / cfg.grid.x), vec2<i32>(0), vec2<i32>(dim - 1));
}

/// Wysokość w metrach. Dane są w decymetrach na siatce 4 m — tej samej, z której powstał
/// teren voxelowy (K-13: jedno źródło prawdy o terenie).
fn wysokosc_m(swiat: vec2<f32>) -> f32 {
    return f32(textureLoad(height_map, komorka(swiat), 0).x) * 0.1;
}

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VertexOut {
    // Siatka bez bufora wierzchołków: numer wierzchołka rozkłada się na pozycję w kracie.
    // Dwa trójkąty na quad, sześć wierzchołków — bufor byłby tu czystym kosztem transferu,
    // bo wszystkie dane są regularne.
    let na_bok = u32(cfg.grid.w);
    let quad = i / 6u;
    let rog = i % 6u;
    let qx = f32(quad % na_bok);
    let qy = f32(quad / na_bok);
    var dx = 0.0;
    var dy = 0.0;
    if (rog == 1u || rog == 2u || rog == 4u) { dx = 1.0; }
    if (rog == 2u || rog == 4u || rog == 5u) { dy = 1.0; }

    let oczko = cfg.grid.z;
    let lokalnie = vec2<f32>(
        (qx + dx - cfg.grid.w * 0.5) * oczko,
        (qy + dy - cfg.grid.w * 0.5) * oczko,
    );
    // Środek siatki jest **wycięty**: tam stoją chunki i to one mają rysować teren.
    // Bez wycięcia dwie powierzchnie leżą w tej samej objętości tuż pod kamerą i walczą
    // o bufor głębi. Promień bierze się z zasięgu pierścieni LOD — przy `LOD_RADII_M`
    // kończących się na 4 km, 800 m jest zawsze wewnątrz obszaru pokrytego chunkami.
    let od_srodka = max(abs(lokalnie.x), abs(lokalnie.y));
    if (od_srodka < cfg.snap.z) {
        // Wierzchołek wypchnięty **za** daleką płaszczyznę: trójkąt wypada przy obcinaniu.
        // Zerowe `w` byłoby tu pułapką — dzielenie przez zero daje wartości nieokreślone,
        // a nie zniknięcie, i cały pas ekranu wychodził czarny.
        var pusty: VertexOut;
        pusty.clip_pos = vec4<f32>(0.0, 0.0, 2.0, 1.0);
        pusty.color = vec3<f32>(0.0);
        pusty.normal = vec3<f32>(0.0, 0.0, 1.0);
        pusty.view_dist = 0.0;
        pusty.swiat_xy = vec2<f32>(0.0);
        return pusty;
    }

    let swiat = cfg.snap.xy + lokalnie;
    let h = wysokosc_m(swiat);

    // Normalna z różnic skończonych na oczku siatki — tania i wystarczająca, bo daleki
    // teren i tak nie ma detalu poniżej oczka.
    let hx = wysokosc_m(swiat + vec2<f32>(oczko, 0.0)) - h;
    let hy = wysokosc_m(swiat + vec2<f32>(0.0, oczko)) - h;

    // Zejście pod teren voxelowy. Pół metra to grubość voxela (siatka przechodzi środkiem
    // kostki, nie po jej górnej ścianie), reszta rośnie z nachyleniem: na zboczu siatka
    // o oczku 32 m ścina wierzchołki i bez tego zapasu przebijałaby przez teren jasnymi
    // trójkątami — widać to było jako białe łaty u podnóża gór.
    let nachylenie = length(vec2<f32>(hx, hy)) / oczko;
    let zejscie = 0.5 + 24.0 * nachylenie;

    // Do macierzy trafia pozycja **względem kamery** — `f32` nie uniesie dziesiątek
    // tysięcy metrów (patrz `camera.rs`).
    let pos = vec3<f32>(swiat - cfg.eye.xy, h - cfg.eye.z - zejscie);

    var out: VertexOut;
    out.clip_pos = frame.view_proj * vec4<f32>(pos, 1.0);
    out.color = textureLoad(albedo_map, komorka(swiat), 0).xyz;
    out.normal = normalize(vec3<f32>(-hx, -hy, oczko));
    out.view_dist = length(pos);
    out.swiat_xy = swiat;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let ndotl = max(dot(n, frame.sun_dir.xyz), 0.0);
    let up = n.z * 0.5 + 0.5;
    let ambient = mix(frame.ground_color.xyz, frame.sky_color.xyz, up);
    var color = in.color * (frame.sun_color.xyz * ndotl + ambient);
    // Ta sama nakładka co na terenie bliskim — inaczej podgląd urywałby się na granicy
    // pierścieni LOD, czyli dokładnie tam, gdzie najczęściej się patrzy.
    if (frame.overlay.w >= 0.5) {
        let dim = i32(frame.overlay.y);
        let k = clamp(vec2<i32>(in.swiat_xy / frame.overlay.x), vec2<i32>(0), vec2<i32>(dim - 1));
        let wartosc = textureLoad(overlay_field, k, 0).r;
        let barwa = textureLoad(overlay_palette, vec2<i32>(i32(wartosc * 255.0 + 0.5), 0), 0);
        color = mix(color, barwa.rgb, frame.overlay.z * barwa.a);
    }

    let fog = 1.0 - exp(-in.view_dist * frame.fog.w);
    color = mix(color, frame.fog.xyz, clamp(fog, 0.0, 1.0));

    return vec4<f32>(color, 1.0);
}
