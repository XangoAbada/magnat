// Domknięcie przekroju budynku (M11c §5.7, WP5).
//
// Cięcie poziomami w `voxel.wgsl` odrzuca fragmenty powyżej rzędnej — i to wystarczy,
// żeby zdjąć dach, ale **nie** żeby zrobić z tego przekrój: ściana przecięta w połowie
// jest pustą skorupą i widać przez nią wnętrze geometrii, czyli dziurę. Ten pass kładzie
// na rzędnej cięcia płaski wielokąt obrysu budynku i tym domyka sylwetkę.
//
// Płytka jest **pozioma z definicji**, więc normalnej nie ma w wierzchołku: liczy się ją
// w shaderze jako `+Z`. Oświetlenie jest tym samym modelem co teren — słońce plus ambient
// dwustrefowy — żeby przekrój nie świecił inaczej niż reszta kadru.

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

@group(0) @binding(0) var<uniform> frame: Frame;

struct VertexOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) view_dist: f32,
}

@vertex
fn vs_main(@location(0) pos: vec3<f32>, @location(1) color: u32) -> VertexOut {
    var out: VertexOut;
    out.clip_pos = frame.view_proj * vec4<f32>(pos, 1.0);
    out.color = vec3<f32>(
        f32(color & 0xffu) / 255.0,
        f32((color >> 8u) & 0xffu) / 255.0,
        f32((color >> 16u) & 0xffu) / 255.0,
    );
    out.view_dist = length(pos);
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // Normalna pionowa: płytka leży na rzędnej cięcia i patrzy w niebo.
    let n = vec3<f32>(0.0, 0.0, 1.0);
    let ndotl = max(dot(n, frame.sun_dir.xyz), 0.0);
    let ambient = frame.sky_color.xyz;
    var color = in.color * (frame.sun_color.xyz * ndotl + ambient);
    let fog = 1.0 - exp(-in.view_dist * frame.fog.w);
    color = mix(color, frame.fog.xyz, clamp(fog, 0.0, 1.0));
    return vec4<f32>(color, 1.0);
}
