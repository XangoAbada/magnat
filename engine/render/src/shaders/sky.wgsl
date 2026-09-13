// Niebo jako trójkąt pełnoekranowy (M1 WP-R4).
//
// Trójkąt, nie prostokąt: jeden trójkąt pokrywający cały ekran ma o jeden wierzchołek mniej
// i — co ważniejsze — nie ma przekątnej przez środek kadru, na której rasteryzator liczy
// fragmenty dwa razy.

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

struct SkyOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) ndc: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> SkyOut {
    // Trójkąt pokrywający [-1, 1]²: (-1,-1), (3,-1), (-1,3).
    let x = f32(i32(i & 1u) * 4 - 1);
    let y = f32(i32((i >> 1u) & 1u) * 4 - 1);
    var out: SkyOut;
    // Głębokość 1.0 — niebo jest zawsze najdalej i przegrywa z każdą geometrią.
    out.clip_pos = vec4<f32>(x, y, 1.0, 1.0);
    out.ndc = vec2<f32>(x, y);
    return out;
}

@fragment
fn fs_main(in: SkyOut) -> @location(0) vec4<f32> {
    // Kierunek promienia odtworzony z odwrotności macierzy nie jest potrzebny: do gradientu
    // wystarcza wysokość na ekranie, bo kamera nie patrzy w zenit ani w nadir (clamp
    // pochylenia w `camera.rs`). Tańsze o jedną odwrotność macierzy na klatkę.
    let t = clamp(in.ndc.y * 0.5 + 0.5, 0.0, 1.0);
    var color = mix(frame.fog.xyz, frame.sky_color.xyz, pow(t, 0.7));

    // Tarcza słońca: jasna plama tam, gdzie kierunek patrzenia zbiega się ze słońcem.
    // Rzut kierunku słońca na ekran robimy macierzą widoku — bez niego słońce wisiałoby
    // w tym samym miejscu kadru niezależnie od obrotu kamery.
    let sun_clip = frame.view_proj * vec4<f32>(frame.sun_dir.xyz * 4000.0, 1.0);
    if (sun_clip.w > 0.0) {
        let sun_ndc = sun_clip.xy / sun_clip.w;
        let d = length(in.ndc - sun_ndc);
        let disc = exp(-d * d * 900.0);
        let glow = exp(-d * d * 12.0) * 0.35;
        color += frame.sun_color.xyz * (disc + glow);
    }

    return vec4<f32>(color, 1.0);
}
