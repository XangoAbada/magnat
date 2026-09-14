// Piesi w kadrze (M3 §5.11, decyzja 9.3 i 9.17).
//
// Jedna bryła na mieszkańca, rysowana instancjonowanie bez bufora wierzchołków:
// 36 indeksów rozwija się w sześcian w samym shaderze. Powód jest ten sam co przy
// voxelach — przy 80 tys. pieszych na klatkę liczy się to, ile bajtów przechodzi
// przez PCIe, a nie to, ile linii ma shader. Instancja to 16 B: pozycja i identyfikator.
//
// **Pozycja instancji jest względem kamery**, nie absolutna (`camera.rs`): przeliczenie
// robi `Renderer::set_pedestrians`, bo tylko renderer zna oko.
//
// Ten sam moduł ma dwa punkty wejścia fragmentu: `fs_main` maluje pieszego w scenie HDR,
// `fs_id` zapisuje identyfikator encji do bufora ID. Jeden moduł, bo geometria musi być
// **bit w bit ta sama** — bufor ID jest rysowany z porównaniem głębi `Equal`, więc
// najmniejsza różnica w wierzchołku zerowałaby trafienia.

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

@group(0) @binding(0) var<uniform> frame: Frame;

/// Pół szerokości bryły i jej wysokość w metrach. Sylwetka, nie model — M11 podmienia
/// to na siatkę z `PoseAtlas`, a do klikania i do „widać, że miasto żyje" wystarcza.
const HALF_W: f32 = 0.25;
const HEIGHT: f32 = 1.75;

struct Instance {
    @location(0) pos: vec3<f32>,
    @location(1) entity: u32,
}

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) @interpolate(flat) entity: u32,
    @location(2) dist: f32,
}

// Sześcian z 36 wierzchołków. Tablica trzyma róg (0/1 na każdej osi) i normalną ściany;
// rozwinięcie jest stałe, więc kompilator umieszcza je w pamięci stałej.
fn corner(i: u32) -> vec3<f32> {
    // Kolejność ścian: -X, +X, -Y, +Y, -Z, +Z; po dwa trójkąty na ścianę.
    let face = i / 6u;
    let v = i % 6u;
    // Dwa trójkąty kwadratu: 0,1,2 i 0,2,3.
    var q = 0u;
    switch v {
        case 0u: { q = 0u; }
        case 1u: { q = 1u; }
        case 2u: { q = 2u; }
        case 3u: { q = 0u; }
        case 4u: { q = 2u; }
        default: { q = 3u; }
    }
    // Współrzędne rogu kwadratu w układzie ściany.
    let a = f32(q == 1u || q == 2u);
    let b = f32(q == 2u || q == 3u);
    var p = vec3<f32>(0.0);
    switch face {
        case 0u: { p = vec3<f32>(0.0, a, b); }        // -X
        case 1u: { p = vec3<f32>(1.0, b, a); }        // +X
        case 2u: { p = vec3<f32>(b, 0.0, a); }        // -Y
        case 3u: { p = vec3<f32>(a, 1.0, b); }        // +Y
        case 4u: { p = vec3<f32>(a, b, 0.0); }        // -Z
        default: { p = vec3<f32>(b, a, 1.0); }        // +Z
    }
    return p;
}

fn face_normal(i: u32) -> vec3<f32> {
    switch i / 6u {
        case 0u: { return vec3<f32>(-1.0, 0.0, 0.0); }
        case 1u: { return vec3<f32>(1.0, 0.0, 0.0); }
        case 2u: { return vec3<f32>(0.0, -1.0, 0.0); }
        case 3u: { return vec3<f32>(0.0, 1.0, 0.0); }
        case 4u: { return vec3<f32>(0.0, 0.0, -1.0); }
        default: { return vec3<f32>(0.0, 0.0, 1.0); }
    }
}

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Instance) -> VsOut {
    let c = corner(vi);
    // Pozycja instancji to punkt na gruncie; bryła rośnie w górę (Z jest pionem).
    let local = vec3<f32>(
        (c.x - 0.5) * 2.0 * HALF_W,
        (c.y - 0.5) * 2.0 * HALF_W,
        c.z * HEIGHT,
    );
    let world = inst.pos + local;
    var out: VsOut;
    out.clip_pos = frame.view_proj * vec4<f32>(world, 1.0);
    out.normal = face_normal(vi);
    out.entity = inst.entity;
    out.dist = length(world);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Oświetlenie jest celowo ubogie: kierunkowe słońce plus ambient z nieba, bez cieni
    // i bez świateł punktowych. Pieszy w M3 ma być widoczny i klikalny, a nie ładny —
    // materiał i cienie wnosi M11 razem z animacją.
    let n = normalize(in.normal);
    let ndl = max(dot(n, normalize(frame.sun_dir.xyz)), 0.0);
    let albedo = vec3<f32>(0.62, 0.45, 0.38);
    var color = albedo * (frame.sun_color.rgb * ndl + frame.sky_color.rgb * 0.6);
    // Ta sama mgła co teren — inaczej piesi w oddali wisieliby nad panoramą.
    let fog = 1.0 - exp(-in.dist * frame.fog.w);
    color = mix(color, frame.fog.rgb, clamp(fog, 0.0, 1.0));
    return vec4<f32>(color * frame.sun_dir.w, 1.0);
}

@fragment
fn fs_id(in: VsOut) -> @location(0) u32 {
    // 0 znaczy „nic" (tekstura jest czyszczona zerem), więc identyfikator jest
    // przesunięty o jeden. Odwrotność siedzi w `Renderer::pick`.
    return in.entity + 1u;
}
