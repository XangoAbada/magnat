// Jeden pipeline, jeden shader — rusztowanie pod M1 (M0 §5.12).
// Oświetlenie jest zahardkodowane i celowo prymitywne: to nie jest render,
// to dowód, że stos graficzny startuje.

struct Camera {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec4<f32>,
    @location(2) palette_index: u32,
};

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

// Cztery materiały — tyle, ile ma zahardkodowany chunk.
const PALETTE = array<vec3<f32>, 4>(
    vec3<f32>(0.42, 0.32, 0.22),  // ziemia
    vec3<f32>(0.30, 0.52, 0.24),  // trawa
    vec3<f32>(0.55, 0.55, 0.58),  // kamień
    vec3<f32>(0.75, 0.70, 0.45),  // piasek
);

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);

    let light_dir = normalize(vec3<f32>(0.4, 0.7, 0.55));
    let ambient = 0.35;
    let diffuse = max(dot(normalize(in.normal.xyz), light_dir), 0.0) * 0.65;
    let base = PALETTE[in.palette_index % 4u];
    out.color = base * (ambient + diffuse);
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
