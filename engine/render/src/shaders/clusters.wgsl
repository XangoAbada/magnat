// Przypisanie świateł do froxeli (M1 §5.8, WP-R4).
//
// Jeden wątek na froxel, pętla po wszystkich światłach. Odwrotnie — wątek na światło —
// wymagałoby atomowego dopisywania do list sąsiadów i synchronizacji między grupami;
// przy 3456 froxelach i 4096 światłach pętla jest krótsza niż koszt tej synchronizacji.
//
// Froxel jest prostopadłościanem w **przestrzeni widoku**, nie ostrosłupem ściętym.
// To przybliżenie od zewnątrz: klaster dostaje czasem światło, które go nie dotyka
// (fragment i tak policzy wtedy zerowy wkład), ale nigdy nie gubi światła, które dotyka.

struct Clusters {
    // x = tan(fov/2) * aspect, y = tan(fov/2), z = near klastrów, w = far klastrów
    frustum: vec4<f32>,
    // x,y,z = wymiary siatki, w = pojemność jednego klastra
    dims: vec4<u32>,
    // liczba świateł w buforze
    light_count: vec4<u32>,
}

struct Light {
    // xyz = pozycja względem kamery w metrach, w = zasięg
    pos_range: vec4<f32>,
    // xyz = barwa × natężenie, w = nieużywane
    color: vec4<f32>,
}

@group(0) @binding(0) var<uniform> cfg: Clusters;
@group(0) @binding(1) var<storage, read> lights: array<Light>;
@group(0) @binding(2) var<storage, read_write> counts: array<u32>;
@group(0) @binding(3) var<storage, read_write> indices: array<u32>;
// Macierz widoku: pozycje świateł są w przestrzeni świata względem kamery, a froxele
// w przestrzeni widoku — bez tej macierzy jedno z dwóch byłoby w złym układzie.
@group(0) @binding(4) var<uniform> view: mat4x4<f32>;

/// Granica slice'u w metrach. Podział logarytmiczny, bo froxele mają mieć zbliżone
/// proporcje w całym zakresie: przy równym podziale pierwsze są cienkie jak kartka,
/// a ostatnie długie na sto metrów.
fn slice_z(i: f32) -> f32 {
    let n = cfg.frustum.z;
    let f = cfg.frustum.w;
    return n * pow(f / n, i / f32(cfg.dims.z));
}

// Grupa robocza to **jedna warstwa siatki** (16 × 9 froxeli). Światła wczytuje się wtedy
// raz do pamięci dzielonej i sprawdza przez wszystkie 144 wątki, zamiast czytać każdą
// pozycję 144 razy z pamięci globalnej. Przy 4096 światłach to różnica między 0,7 ms
// a budżetem 0,4 ms z §4.
const W: u32 = 16u;
const H: u32 = 9u;
const PORCJA: u32 = W * H;
var<workgroup> porcja: array<Light, PORCJA>;

@compute @workgroup_size(16, 9, 1)
fn cs_main(
    @builtin(global_invocation_id) id: vec3<u32>,
    @builtin(local_invocation_index) lokalny: u32,
) {
    if (id.x >= cfg.dims.x || id.y >= cfg.dims.y || id.z >= cfg.dims.z) {
        return;
    }
    let klaster = id.z * cfg.dims.x * cfg.dims.y + id.y * cfg.dims.x + id.x;

    let z0 = slice_z(f32(id.z));
    let z1 = slice_z(f32(id.z + 1u));

    // Kafel na ekranie w znormalizowanych współrzędnych [-1, 1].
    let sx0 = f32(id.x) / f32(cfg.dims.x) * 2.0 - 1.0;
    let sx1 = f32(id.x + 1u) / f32(cfg.dims.x) * 2.0 - 1.0;
    let sy0 = f32(id.y) / f32(cfg.dims.y) * 2.0 - 1.0;
    let sy1 = f32(id.y + 1u) / f32(cfg.dims.y) * 2.0 - 1.0;

    // Prostopadłościan otaczający froxel: bok kafla rośnie z odległością, więc bierzemy
    // szerszy koniec (z1) dla obu ścian.
    let hx = cfg.frustum.x * z1;
    let hy = cfg.frustum.y * z1;
    let lo = vec3<f32>(min(sx0, sx1) * hx, min(sy0, sy1) * hy, -z1);
    let hi = vec3<f32>(max(sx0, sx1) * hx, max(sy0, sy1) * hy, -z0);

    var n = 0u;
    let pojemnosc = cfg.dims.w;
    let ile = cfg.light_count.x;
    var baza = 0u;
    loop {
        if (baza >= ile) {
            break;
        }
        // Wczytanie porcji: każdy wątek przynosi jedno światło.
        let i = baza + lokalny;
        if (i < ile) {
            let l = lights[i];
            porcja[lokalny] = Light(
                vec4<f32>((view * vec4<f32>(l.pos_range.xyz, 1.0)).xyz, l.pos_range.w),
                l.color,
            );
        }
        workgroupBarrier();

        let w_porcji = min(PORCJA, ile - baza);
        for (var k = 0u; k < w_porcji; k = k + 1u) {
            let l = porcja[k];
            // Odległość punktu od prostopadłościanu — klasyczny test sfera/AABB.
            let d = max(lo - l.pos_range.xyz, max(vec3<f32>(0.0), l.pos_range.xyz - hi));
            if (dot(d, d) <= l.pos_range.w * l.pos_range.w) {
                if (n < pojemnosc) {
                    indices[klaster * pojemnosc + n] = baza + k;
                }
                // Licznik rośnie **także po przepełnieniu**: `ClusterOccupancy` ma pokazać,
                // ile świateł naprawdę trafiło do klastra, a nie ile się w nim zmieściło.
                n = n + 1u;
            }
        }
        workgroupBarrier();
        baza = baza + PORCJA;
    }
    counts[klaster] = n;
}
