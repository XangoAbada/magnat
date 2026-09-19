// Impostory encji: dwa trójkąty zamiast modelu (M11b §5.6, WP4).
//
// Od progu L2 postać ma na ekranie kilkanaście pikseli. Zamiast czterystu wierzchołków
// rysuje się wtedy **kafel z atlasu sylwetek** — billboard obrócony ku kamerze, wybrany
// z ośmiu azymutów i czterech faz chodu (`magnat_voxel::impostor`).
//
// Kafel niesie **rolę slotu palety**, nie kolor: barwę wybiera ten sam `pick_color`,
// którym liczy ją `instance.wgsl` dla pełnej geometrii. Dlatego tłum na trzystu metrach
// ma nadal zróżnicowane ubrania, a przejście L1 → L2 nie zmienia koloru ani jednej
// postaci (kryterium `variant_survives_lod`).
//
// Dwa punkty wejścia fragmentu i jeden wierzchołka — ta sama zasada co w `instance.wgsl`:
// bufor identyfikatorów rysuje się z porównaniem głębi `Equal`, więc geometria musi być
// bit w bit ta sama.
//
// **Kafel nie zależy od klipu, tylko od azymutu i fazy chodu.** To jest jawny sufit:
// mieszkaniec siedzący albo pracujący dostaje od 120 m sylwetkę idącego, bo drugi zestaw
// kafli na klip mnożyłby atlas przez dwadzieścia cztery po to, żeby rozróżnić pozy,
// których z tej odległości nie widać. Wyjście, gdyby kiedyś było widać: wpis atlasu
// niesie już `base_layer` per model, więc wystarczy drugi wymiar w indeksie.

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
    weather: vec4<f32>,   // x = pokrywa śnieżna, y = wilgoć, z = pora roku, w = zachmurzenie
}

// Wpis atlasu: gdzie leżą kafle modelu i jak duży jest jego billboard w metrach.
struct Impostor {
    base_layer: u32,
    // `kierunki | kafle << 8 | przesunięcie fazy << 16`
    dirs_frames: u32,
    size: vec2<f32>,
}

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<storage, read> palette_colors: array<u32>;
@group(0) @binding(2) var<storage, read> palette_ramps: array<vec2<u32>>;
@group(0) @binding(4) var<storage, read> impostors: array<Impostor>;
@group(0) @binding(5) var impostor_tiles: texture_2d_array<u32>;

const PALETTE_MIX: u32 = 0x9E3779B9u;
const PALETTE_ROLE_SALT: u32 = 0x85EBCA6Bu;
const PALETTE_ROLE_COUNT: u32 = 12u;

const TAU: f32 = 6.2831853;

struct Instance {
    @location(2) pos: vec3<f32>,
    @location(3) yaw_pitch: u32,
    @location(4) pal_model: u32,
    @location(5) anim: u32,
    @location(6) tint: u32,
    @location(7) variant: u32,
    @location(8) pick: u32,
}

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) @interpolate(flat) layer: u32,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) variant: u32,
    @location(3) @interpolate(flat) tint: vec4<f32>,
    @location(4) @interpolate(flat) pick: u32,
    @location(5) @interpolate(flat) paleta: u32,
    @location(6) dist: f32,
    @location(7) @interpolate(flat) yaw: f32,
}

fn pick_color(variant: u32, role: u32, ramp: vec2<u32>) -> u32 {
    if ramp.y == 0u {
        return 0xFFFF00FFu;
    }
    let v = (variant ^ (role * PALETTE_ROLE_SALT)) * PALETTE_MIX;
    return palette_colors[ramp.x + ((v >> 13u) % ramp.y)];
}

fn unpack_srgb(c: u32) -> vec3<f32> {
    let s = vec3<f32>(
        f32(c & 0xFFu),
        f32((c >> 8u) & 0xFFu),
        f32((c >> 16u) & 0xFFu),
    ) / 255.0;
    return pow(s, vec3<f32>(2.2));
}

fn unpack_rgba(c: u32) -> vec4<f32> {
    return vec4<f32>(
        f32(c & 0xFFu),
        f32((c >> 8u) & 0xFFu),
        f32((c >> 16u) & 0xFFu),
        f32((c >> 24u) & 0xFFu),
    ) / 255.0;
}

fn face_normal(i: u32) -> vec3<f32> {
    switch i {
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
    let model = inst.pal_model >> 18u;
    let e = impostors[model];
    // Pozycja instancji jest **względem oka** (`instancing::build_instances`), więc
    // kierunek do kamery to po prostu przeciwieństwo pozycji w płaszczyźnie poziomej.
    // Billboard obraca się wokół pionu, a nie ku kamerze w pełni: postać stojąca
    // na ziemi ma zostać pionowa także wtedy, gdy kamera patrzy z góry.
    let plaska = vec2<f32>(-inst.pos.x, -inst.pos.y);
    let dlugosc = max(length(plaska), 1.0e-4);
    let do_oka = plaska / dlugosc;
    let prawo = vec3<f32>(-do_oka.y, do_oka.x, 0.0);

    var rogi = array<vec2<f32>, 6>(
        vec2<f32>(-0.5, 0.0),
        vec2<f32>(0.5, 0.0),
        vec2<f32>(0.5, 1.0),
        vec2<f32>(-0.5, 0.0),
        vec2<f32>(0.5, 1.0),
        vec2<f32>(-0.5, 1.0),
    );
    let r = rogi[vi];
    let world = inst.pos + prawo * (r.x * e.size.x) + vec3<f32>(0.0, 0.0, r.y * e.size.y);

    // Który kafel: kąt, pod jakim kamera patrzy na encję, **w układzie modelu**.
    // `atan2(-do_oka)` to kierunek od kamery do encji, czyli dokładnie ten azymut,
    // dla którego wypalano kafel (`impostor::wypal`).
    let yaw = f32(inst.yaw_pitch & 0xFFFFu) / 65536.0 * TAU;
    let dirs = e.dirs_frames & 0xFFu;
    let frames = max((e.dirs_frames >> 8u) & 0xFFu, 1u);
    let azymut = atan2(-do_oka.y, -do_oka.x) - yaw;
    let kroki = i32(dirs);
    var d = 0;
    if (dirs > 0u) {
        d = ((i32(round(azymut / TAU * f32(dirs))) % kroki) + kroki) % kroki;
    }
    // Kafle są wypalane z co n-tej klatki klipu, więc faza z rekordu tyka n razy
    // szybciej, niż zmienia się sylwetka — stąd przesunięcie. Bez niego tłum
    // na impostorach przebiera nogami cztery razy szybciej niż metr bliżej.
    let shift = (e.dirs_frames >> 16u) & 0xFFu;
    let faza = (((inst.anim >> 8u) & 0xFFu) >> shift) % frames;

    var out: VsOut;
    out.clip_pos = frame.view_proj * vec4<f32>(world, 1.0);
    out.layer = e.base_layer + u32(d) * frames + faza;
    out.uv = vec2<f32>(r.x + 0.5, 1.0 - r.y);
    out.variant = inst.variant;
    out.tint = unpack_rgba(inst.tint);
    out.pick = inst.pick;
    out.paleta = inst.pal_model & 0xFFFFu;
    out.dist = length(world);
    out.yaw = yaw;
    return out;
}

// Kafel czyta się `textureLoad`, a nie samplerem: kanał `R` niesie **numer roli**,
// a interpolacja liniowa zrobiłaby z roli 2 i 4 rolę 3, czyli cudzą barwę.
fn kafel(in: VsOut) -> vec4<u32> {
    let rozmiar = textureDimensions(impostor_tiles);
    let px = vec2<i32>(
        clamp(i32(in.uv.x * f32(rozmiar.x)), 0, i32(rozmiar.x) - 1),
        clamp(i32(in.uv.y * f32(rozmiar.y)), 0, i32(rozmiar.y) - 1),
    );
    return textureLoad(impostor_tiles, px, in.layer, 0);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let t = kafel(in);
    if (t.w == 0u) {
        discard;
    }
    let role = t.x - 1u;
    let ramp = palette_ramps[in.paleta * PALETTE_ROLE_COUNT + min(role, PALETTE_ROLE_COUNT - 1u)];
    let albedo = unpack_srgb(pick_color(in.variant, role, ramp));

    // Normalna z numeru ściany, obrócona tak samo jak model w świecie — dzięki temu
    // sylwetka dostaje **to samo światło** co pełna geometria i przejście progu nie
    // zmienia jasności.
    let n = face_normal(t.y - 1u);
    let cs = cos(in.yaw);
    let sn = sin(in.yaw);
    let n_obr = normalize(vec3<f32>(n.x * cs - n.y * sn, n.x * sn + n.y * cs, n.z));
    let ndl = max(dot(n_obr, frame.sun_dir.xyz), 0.0);
    let up = n_obr.z * 0.5 + 0.5;
    let ambient = mix(frame.ground_color.rgb, frame.sky_color.rgb, up);
    var color = albedo * (frame.sun_color.rgb * ndl + ambient);
    color = mix(color, in.tint.rgb, in.tint.a);
    let fog = 1.0 - exp(-in.dist * frame.fog.w);
    color = mix(color, frame.fog.rgb, clamp(fog, 0.0, 1.0));
    return vec4<f32>(color, 1.0);
}

@fragment
fn fs_id(in: VsOut) -> @location(0) u32 {
    let t = kafel(in);
    if (t.w == 0u) {
        discard;
    }
    return in.pick;
}
