// Napis na szyldzie (M11c §5.7, WP9).
//
// Tablica szyldu jest **encją instancjonowaną** (model `sign.mvox`, slot `SlotRole::Sign`)
// i jedzie tą samą ścieżką co postacie i pojazdy — razem z identyfikatorem do bufora ID,
// więc kliknięcie w szyld otwiera kartę zakładu. Ten pass dokłada do niej samą **literę**:
// prostokąt tuż przed licem tablicy, próbkujący kafel atlasu.
//
// Rozdzielenie jest tańsze niż wspólny potok: napis potrzebuje współrzędnej tekstury,
// a `ModelVertex` jej nie niesie i nie będzie — miałby wtedy osiem bajtów na wierzchołek
// więcej dla wszystkich modeli po to, żeby użył ich jeden.

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
@group(0) @binding(1) var sign_tiles: texture_2d_array<f32>;
@group(0) @binding(2) var sign_sampler: sampler;

struct VertexOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) layer: u32,
    @location(2) color: vec3<f32>,
    @location(3) view_dist: f32,
}

@vertex
fn vs_main(
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) layer: u32,
    @location(3) color: u32,
) -> VertexOut {
    var out: VertexOut;
    out.clip_pos = frame.view_proj * vec4<f32>(pos, 1.0);
    out.uv = uv;
    out.layer = layer;
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
    // Kafel jest maską pokrycia w kanale R: 0 = tło tablicy, 1 = litera.
    let pokrycie = textureSample(sign_tiles, sign_sampler, in.uv, in.layer).r;
    if (pokrycie < 0.35) {
        discard;
    }
    // Litera **świeci własnym światłem**, a nie odbija słońca: szyld ma być czytelny
    // także w nocy i w cieniu kamienicy, a to jest przecież neon albo podświetlona
    // kaseta, nie malowana blacha.
    var color = in.color;
    let fog = 1.0 - exp(-in.view_dist * frame.fog.w);
    color = mix(color, frame.fog.xyz, clamp(fog, 0.0, 1.0));
    return vec4<f32>(color, 1.0);
}
