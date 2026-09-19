// Pipeline voxelowy (M1 §5.3, §5.8, WP-R2).
//
// Wierzchołek ma 8 bajtów i jest w całości spakowany — rozpakowanie dzieje się tutaj,
// bo przesyłanie rozpakowanych pozycji i normalnych kosztowałoby 32 B zamiast 8.
// Pozycja jest **lokalna dla chunka** (0..32); przesunięcie chunka względem kamery
// leży w per-chunk SSBO. Powód jest w `camera.rs`: mapa 16 km przekracza precyzję f32,
// więc do shadera nie trafia ani jedna współrzędna absolutna.

struct Frame {
    view_proj: mat4x4<f32>,
    light_view_proj: array<mat4x4<f32>, 4>,  // macierze kaskad cieni
    cascade_far: vec4<f32>,                  // koniec zakresu kaskad w metrach
    cascade_texel: vec4<f32>,                // rozmiar texela kaskady w metrach
    sun_dir: vec4<f32>,       // xyz = kierunek do słońca, w = ekspozycja
    sun_color: vec4<f32>,     // xyz = barwa, w = wysokość słońca w stopniach
    sky_color: vec4<f32>,     // xyz = ambient z nieba, w = nieużywane
    ground_color: vec4<f32>,  // xyz = ambient odbity od gruntu, w = nieużywane
    fog: vec4<f32>,           // xyz = barwa mgły, w = gęstość
    clip: vec4<f32>,          // x = poziom cięcia w metrach, y = czy aktywne, z = wysokość kamery
    screen: vec4<f32>,        // xy = rozmiar okna w pikselach, zw = odwrócenie bufora głębi
    eye: vec4<f32>,           // xyz = pozycja kamery w świecie
    overlay: vec4<f32>,       // x = bok komórki, y = wymiar, z = siła, w = czy aktywna
    weather: vec4<f32>,   // x = pokrywa śnieżna, y = wilgoć, z = pora roku, w = zachmurzenie
}

struct ChunkData {
    // xyz = przesunięcie chunka względem kamery w metrach, w = skala voxela (2^lod)
    origin_scale: vec4<f32>,
}

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<storage, read> chunks: array<ChunkData>;
@group(0) @binding(2) var<storage, read> materials: array<vec4<f32>>;
@group(0) @binding(3) var shadow_map: texture_depth_2d_array;
@group(0) @binding(4) var shadow_sampler: sampler_comparison;

struct Light {
    pos_range: vec4<f32>,
    color: vec4<f32>,
}

struct Clusters {
    frustum: vec4<f32>,
    dims: vec4<u32>,
    light_count: vec4<u32>,
}

@group(0) @binding(5) var<storage, read> lights: array<Light>;
@group(0) @binding(6) var<storage, read> cluster_counts: array<u32>;
@group(0) @binding(7) var<storage, read> cluster_indices: array<u32>;
@group(0) @binding(8) var<uniform> clusters: Clusters;

struct Cascade { index: u32 }
@group(1) @binding(0) var<uniform> cascade: Cascade;

// Nakładka terenowa (§6.1). W passie nieprzezroczystym grupa 1 niesie nakładkę, w passie
// cienia — numer kaskady. To nie kolizja: oba pipeline'y mają własny układ wiązań i własny
// punkt wejścia, a wspólny jest tylko moduł.
@group(1) @binding(0) var overlay_field: texture_2d<f32>;
@group(1) @binding(1) var overlay_palette: texture_2d<f32>;

struct VertexOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) ao: f32,
    @location(3) view_dist: f32,
    @location(4) world_z: f32,
    @location(5) world_pos: vec3<f32>,
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

/// Pozycja voxela względem kamery — wspólna dla kadru i dla mapy cienia. Rozjazd tych
/// dwóch przeliczeń byłby widoczny jako cień przesunięty względem bryły, która go rzuca.
fn pozycja(packed: vec2<u32>, chunk_index: u32) -> vec3<f32> {
    let lo = packed.x;
    let chunk = chunks[chunk_index];
    let scale = chunk.origin_scale.w;
    let local = vec3<f32>(
        f32(lo & 0x3Fu) * scale,
        f32((lo >> 6u) & 0x3Fu) * scale,
        f32((lo >> 12u) & 0x3Fu) * scale * VOXEL_HEIGHT_M,
    );
    return chunk.origin_scale.xyz + local;
}

@vertex
fn vs_shadow(
    @location(0) packed: vec2<u32>,
    @builtin(instance_index) chunk_index: u32,
) -> @builtin(position) vec4<f32> {
    return frame.light_view_proj[cascade.index] * vec4<f32>(pozycja(packed, chunk_index), 1.0);
}

/// Ile światła dociera do punktu: 1 = pełne słońce, 0 = pełny cień.
///
/// Kaskadę wybiera odległość od kamery, a nie głębia w NDC — progi są w metrach i mają
/// znaczyć to samo niezależnie od rzutowania. Ostatnia kaskada kończy cienie miękkim
/// zanikiem zamiast twardej krawędzi, bo krawędź na 1200 m widać jako linię na terenie.
fn oswietlenie(pos: vec3<f32>, dist_m: f32, ndotl: f32) -> f32 {
    if (ndotl <= 0.0) {
        return 0.0;
    }
    var k = 3;
    for (var i = 0; i < 4; i = i + 1) {
        if (dist_m < frame.cascade_far[i]) {
            k = i;
            break;
        }
    }
    if (dist_m > frame.cascade_far[3]) {
        return 1.0;
    }

    let p = frame.light_view_proj[k] * vec4<f32>(pos, 1.0);
    let ndc = p.xyz / p.w;
    if (any(abs(ndc.xy) > vec2<f32>(1.0)) || ndc.z > 1.0) {
        return 1.0;
    }
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);

    // Bias zależny od nachylenia: przy świetle padającym płasko jeden texel cienia
    // obejmuje większy przedział głębi, więc stały bias zostawiałby akne.
    let nachylenie = clamp(1.0 - ndotl, 0.0, 1.0);
    let bias = (0.00008 + 0.0009 * nachylenie) * (frame.cascade_texel[k] + 1.0);

    // PCF 3×3 na sprzętowym porównaniu — każda próbka jest już dwuliniowa, więc efektywnie
    // jest to filtr 4×4 za dziewięć pobrań.
    let krok = 1.0 / f32(textureDimensions(shadow_map).x);
    var suma = 0.0;
    for (var dy = -1; dy <= 1; dy = dy + 1) {
        for (var dx = -1; dx <= 1; dx = dx + 1) {
            let o = vec2<f32>(f32(dx), f32(dy)) * krok;
            suma = suma + textureSampleCompareLevel(
                shadow_map, shadow_sampler, uv + o, k, ndc.z - bias
            );
        }
    }
    let swiatlo = suma / 9.0;

    // Zanik na końcu ostatniej kaskady.
    let zanik = smoothstep(frame.cascade_far[3] * 0.85, frame.cascade_far[3], dist_m);
    return mix(swiatlo, 1.0, zanik);
}

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
    out.world_pos = pos;
    // Wysokość bezwzględna potrzebna do cięcia poziomem — chunk niesie ją w przesunięciu,
    // więc odtwarzamy ją z pozycji kamery zapisanej w `clip.z`.
    out.world_z = pos.z + frame.clip.z;
    return out;
}

/// Pogoda na materiale: śnieg, wilgoć i pora roku — **bez dotykania geometrii**.
///
/// Remeshing całego miasta cztery razy w roku gry za zmianę koloru liści byłby kosztem
/// bez pokrycia (§5.8), więc sezon jest przesunięciem palety i tyle. Ta sama zasada
/// obejmuje śnieg: nie sypiemy voxeli, tylko bielimy powierzchnie zwrócone w górę.
///
/// ponytail: roślinność rozpoznajemy po **przewadze zieleni w albedo**, bo tabela
/// materiałów nie niesie klasy pogodowej. Sufit: zielona ściana zmieniałaby barwę
/// z porą roku. Ścieżka wyjścia, gdy to zacznie przeszkadzać: jeden bajt klasy
/// w czwartej składowej wpisu tabeli materiałów obok chropowatości.
fn pogoda_na_materiale(color: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    var albedo = color;

    // Pora roku dotyka wyłącznie zieleni. Zima jest już załatwiona śniegiem, więc
    // sezony przestawiają tylko odcień: wiosna soczysta, lato neutralne, jesień żółto-ruda.
    let zielen = f32(albedo.g > albedo.r * 1.15 && albedo.g > albedo.b * 1.15);
    if (zielen > 0.0) {
        let sezon = i32(frame.weather.z + 0.5);
        var szorstka = vec3<f32>(1.0);
        if (sezon == 0) { szorstka = vec3<f32>(0.85, 0.82, 0.78); }   // zima: wyblakła
        if (sezon == 1) { szorstka = vec3<f32>(0.92, 1.12, 0.80); }   // wiosna: soczysta
        if (sezon == 3) { szorstka = vec3<f32>(1.45, 1.00, 0.45); }   // jesień: ruda
        albedo = albedo * szorstka;
    }

    // Wilgoć: mokra powierzchnia jest ciemniejsza i mniej nasycona. Kałuż jako kształtów
    // nie ma i nie będzie — różnica między mokrym a suchym asfaltem jest w połysku.
    let poziomo = clamp(n.z, 0.0, 1.0);
    let wilgoc = frame.weather.y * poziomo;
    let szarosc = dot(albedo, vec3<f32>(0.299, 0.587, 0.114));
    albedo = mix(albedo, mix(albedo, vec3<f32>(szarosc), 0.25) * 0.62, wilgoc);

    // Śnieg leży na tym, co zwrócone w górę, i tym mocniej, im bardziej płasko.
    // Pion zostaje odsłonięty — inaczej miasto zimą wygląda jak polany lukrem.
    let przyczepnosc = smoothstep(0.35, 0.85, n.z);
    return mix(albedo, vec3<f32>(0.90, 0.93, 0.98), frame.weather.x * przyczepnosc);
}

/// Numer klastra dla piksela: kafel z pozycji na ekranie, warstwa z odległości.
///
/// Podział po Z jest logarytmiczny i **musi** być tą samą funkcją co w `clusters.wgsl` —
/// rozjazd o jedną warstwę znaczy oświetlenie czytane z sąsiedniego froxela, czyli światła
/// znikające przy ruchu kamery.
fn numer_klastra(frag_xy: vec2<f32>, glebokosc_m: f32) -> u32 {
    let n = clusters.frustum.z;
    let f = clusters.frustum.w;
    let z = clamp(glebokosc_m, n, f);
    let warstwa = u32(clamp(
        log(z / n) / log(f / n) * f32(clusters.dims.z),
        0.0,
        f32(clusters.dims.z - 1u),
    ));
    // Rozmiar ekranu w pikselach leży w `frame.screen`; kafel to po prostu jego podział.
    // Oś Y ekranu rośnie w dół, a siatka klastrów jest liczona w NDC, gdzie rośnie w górę.
    // Bez tego odbicia fragment czyta listę z kafla odbitego względem środka ekranu —
    // widać to jako poziome pasy oświetlenia przesunięte względem świateł.
    let kx = u32(clamp(frag_xy.x / frame.screen.x * f32(clusters.dims.x), 0.0, f32(clusters.dims.x - 1u)));
    let ky_gora = u32(clamp(frag_xy.y / frame.screen.y * f32(clusters.dims.y), 0.0, f32(clusters.dims.y - 1u)));
    let kafel = vec2<u32>(kx, clusters.dims.y - 1u - ky_gora);
    return warstwa * clusters.dims.x * clusters.dims.y + kafel.y * clusters.dims.x + kafel.x;
}

/// Wkład świateł punktowych przypisanych do klastra tego piksela.
fn swiatla_punktowe(klaster: u32, pos: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    let pojemnosc = clusters.dims.w;
    let ile = min(cluster_counts[klaster], pojemnosc);
    var suma = vec3<f32>(0.0);
    for (var i = 0u; i < ile; i = i + 1u) {
        let l = lights[cluster_indices[klaster * pojemnosc + i]];
        let do_swiatla = l.pos_range.xyz - pos;
        let d = length(do_swiatla);
        if (d > l.pos_range.w) {
            continue;
        }
        // Tłumienie odwrotnie kwadratowe z odcięciem na zasięgu — bez odcięcia światło
        // ma nieskończony ogon i klaster przestaje cokolwiek odsiewać.
        let zanik = clamp(1.0 - d / l.pos_range.w, 0.0, 1.0);
        let ndotl = max(dot(n, do_swiatla / max(d, 0.0001)), 0.0);
        suma = suma + l.color.xyz * ndotl * zanik * zanik;
    }
    return suma;
}

/// Miesza barwę terenu z barwą nakładki. Nakładka jest **podglądem danych**, nie materiałem:
/// dlatego miesza się z gotowym oświetleniem, a nie z albedo — wtedy odczyt wartości nie
/// zależy od tego, czy zbocze akurat jest w cieniu.
fn z_nakladka(color: vec3<f32>, swiat_xy: vec2<f32>) -> vec3<f32> {
    if (frame.overlay.w < 0.5) {
        return color;
    }
    let dim = i32(frame.overlay.y);
    let k = clamp(vec2<i32>(swiat_xy / frame.overlay.x), vec2<i32>(0), vec2<i32>(dim - 1));
    let wartosc = textureLoad(overlay_field, k, 0).r;
    let barwa = textureLoad(overlay_palette, vec2<i32>(i32(wartosc * 255.0 + 0.5), 0), 0);
    return mix(color, barwa.rgb, frame.overlay.z * barwa.a);
}

// Prepass głębi **z cięciem** (M11c §5.7). Prepass bez shadera fragmentu zapisuje głębię
// całej bryły, także tej, którą `fs_main` zaraz odrzuci — a wtedy przekrój zostaje czarną
// dziurą: niebo przegrywa z zapisaną głębią dachu, a czapka domykająca przegrywa z nią
// porównaniem. Ten punkt wejścia robi **tylko** odrzucenie i istnieje wyłącznie po to,
// żeby prepass widział to samo co pass nieprzezroczysty.
@fragment
fn fs_depth(in: VertexOut) {
    if (frame.clip.y > 0.5 && in.world_z > frame.clip.x) {
        discard;
    }
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

    let cien = oswietlenie(in.world_pos, in.view_dist, ndotl);
    // Warstwa klastra idzie po **głębokości widoku**, nie po odległości od oka: compute
    // wycina froxele płaszczyznami prostopadłymi do osi patrzenia. Na brzegu kadru te dwie
    // wielkości różnią się o kilkanaście procent, a objawem jest oświetlenie w kwadratowych
    // łatach, bo fragment czyta listę z sąsiedniej warstwy. `position.w` we fragmencie
    // to odwrotność `w` z przestrzeni obcinania, czyli dokładnie ta głębokość.
    let glebokosc = 1.0 / in.clip_pos.w;
    // Poza zasięgiem siatki klastrów świateł po prostu nie ma. Zaciskanie głębokości do
    // ostatniej warstwy dawałoby tam listę z granicy zasięgu — czyli prostokątne łaty
    // cudzego światła na odległym terenie.
    var punktowe = vec3<f32>(0.0);
    if (glebokosc <= clusters.frustum.w) {
        punktowe = swiatla_punktowe(numer_klastra(in.clip_pos.xy, glebokosc), in.world_pos, n);
    }
    let light = frame.sun_color.xyz * ndotl * cien + ambient + punktowe;
    var color = pogoda_na_materiale(in.color, n) * light * in.ao;
    color = z_nakladka(color, in.world_pos.xy + frame.eye.xy);

    // Mgła atmosferyczna — wykładnicza po odległości, barwa z nieba przy horyzoncie.
    let fog = 1.0 - exp(-in.view_dist * frame.fog.w);
    color = mix(color, frame.fog.xyz, clamp(fog, 0.0, 1.0));

    // Kolor wychodzi **liniowy i bez ograniczenia z góry** — ekspozycja i tonemap są
    // w przebiegu post-processingu (`post.wgsl`), bo inaczej każdy shader sceny miałby
    // własną kopię krzywej i pierwsza zmiana rozjechałaby wodę z terenem.
    return vec4<f32>(color, 1.0);
}
