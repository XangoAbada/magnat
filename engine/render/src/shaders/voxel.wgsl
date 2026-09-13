// Pipeline voxelowy (M1 §5.3, §5.8, WP-R2).
//
// Wierzchołek ma 8 bajtów i jest w całości spakowany — rozpakowanie dzieje się tutaj,
// bo przesyłanie rozpakowanych pozycji i normalnych kosztowałoby 32 B zamiast 8.
// Pozycja jest **lokalna dla chunka** (0..32); przesunięcie chunka względem kamery
// leży w per-chunk SSBO. Powód jest w `camera.rs`: mapa 16 km przekracza precyzję f32,
// więc do shadera nie trafia ani jedna współrzędna absolutna.

struct Frame {
    view_proj: mat4x4<f32>,
    sun_dir: vec4<f32>,       // xyz = kierunek do słońca, w = ekspozycja
    sun_color: vec4<f32>,     // xyz = barwa, w = wysokość słońca w stopniach
    sky_color: vec4<f32>,     // xyz = ambient z nieba, w = nieużywane
    ground_color: vec4<f32>,  // xyz = ambient odbity od gruntu, w = nieużywane
    fog: vec4<f32>,           // xyz = barwa mgły, w = gęstość
    clip: vec4<f32>,          // x = poziom cięcia w metrach, y = czy aktywne
}

struct ChunkData {
    // xyz = przesunięcie chunka względem kamery w metrach, w = skala voxela (2^lod)
    origin_scale: vec4<f32>,
}

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<storage, read> chunks: array<ChunkData>;
@group(0) @binding(2) var<storage, read> materials: array<vec4<f32>>;

struct VertexOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) ao: f32,
    @location(3) view_dist: f32,
    @location(4) world_z: f32,
}

// Sześć kierunków ścian — ta sama kolejność co `NORMALS` w `engine/voxel`.
// Rozjazd tej tablicy z tamtą oznaczałby oświetlenie policzone dla złej ściany,
// co wygląda jak błąd cieniowania, a jest błędem indeksu.
const NORMALS = array<vec3<f32>, 6>(
    vec3<f32>( 1.0,  0.0,  0.0),
    vec3<f32>(-1.0,  0.0,  0.0),
    vec3<f32>( 0.0,  1.0,  0.0),
    vec3<f32>( 0.0, -1.0,  0.0),
    vec3<f32>( 0.0,  0.0,  1.0),
    vec3<f32>( 0.0,  0.0, -1.0),
);

// Wysokość voxela to 0,5 m, a bok 1 m — chunk jest niesymetryczny w metrach (M1 §5.1).
const VOXEL_HEIGHT_M: f32 = 0.5;

@vertex
fn vs_main(
    @location(0) packed: vec2<u32>,
    @builtin(instance_index) chunk_index: u32,
) -> VertexOut {
    let lo = packed.x;
    let hi = packed.y;

    let lx = f32(lo & 0x3Fu);
    let ly = f32((lo >> 6u) & 0x3Fu);
    let lz = f32((lo >> 12u) & 0x3Fu);
    let ao = f32((lo >> 18u) & 0x3u) / 3.0;
    let n = (lo >> 20u) & 0x7u;

    let material = hi & 0xFFFFu;
    let sun = f32((hi >> 16u) & 0xFu) / 15.0;

    let chunk = chunks[chunk_index];
    let scale = chunk.origin_scale.w;
    // Pozycja względem kamery: przesunięcie chunka plus lokalna pozycja w metrach.
    let local = vec3<f32>(lx * scale, ly * scale, lz * scale * VOXEL_HEIGHT_M);
    let pos = chunk.origin_scale.xyz + local;

    var out: VertexOut;
    out.clip_pos = frame.view_proj * vec4<f32>(pos, 1.0);
    out.normal = NORMALS[min(n, 5u)];
    out.ao = mix(0.35, 1.0, ao) * mix(0.25, 1.0, sun);
    out.color = materials[material].xyz;
    out.view_dist = length(pos);
    // Wysokość bezwzględna potrzebna do cięcia poziomem — chunk niesie ją w przesunięciu,
    // więc odtwarzamy ją z pozycji kamery zapisanej w `clip.z`.
    out.world_z = pos.z + frame.clip.z;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // Cięcie poziomem (§15.2): wszystko powyżej zadanej rzędnej znika. Cap domykający
    // ściany dokłada M11 jako osobny pass — tutaj jest sam uniform i odrzucenie fragmentu.
    if (frame.clip.y > 0.5 && in.world_z > frame.clip.x) {
        discard;
    }

    let n = normalize(in.normal);
    let ndotl = max(dot(n, frame.sun_dir.xyz), 0.0);

    // Ambient dwustrefowy: niebo z góry, odbicie od gruntu z dołu, mieszane po normalnej.
    // Tanie i wystarczające — pełne GI jest poza zakresem M1 (§5.8, ryzyko R9).
    let up = n.z * 0.5 + 0.5;
    let ambient = mix(frame.ground_color.xyz, frame.sky_color.xyz, up);

    let light = frame.sun_color.xyz * ndotl + ambient;
    var color = in.color * light * in.ao;

    // Mgła atmosferyczna — wykładnicza po odległości, barwa z nieba przy horyzoncie.
    let fog = 1.0 - exp(-in.view_dist * frame.fog.w);
    color = mix(color, frame.fog.xyz, clamp(fog, 0.0, 1.0));

    // Ekspozycja i tonemap ACES (przybliżenie Narkowicza) — jeden wzór zamiast krzywej
    // w teksturze, bo i tak jest strojony jednym parametrem ekspozycji.
    color = color * frame.sun_dir.w;
    color = (color * (2.51 * color + 0.03)) / (color * (2.43 * color + 0.59) + 0.14);
    return vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
