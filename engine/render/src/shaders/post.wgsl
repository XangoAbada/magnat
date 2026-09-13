// Post-processing: ekspozycja, tonemap ACES, FXAA (M1 §5.8, WP-R6).
//
// Scena rysuje się do bufora **HDR** (RGBA16F) w liniowej przestrzeni barw, a dopiero ten
// przebieg sprowadza ją do ekranu. Kolejność nie jest dowolna:
//
//   1. **Ekspozycja** działa na wartościach liniowych — po tonemapie jest już za późno,
//      bo krzywa zdążyła ścisnąć jasne końce i rozjaśnianie tylko wyszarza obraz.
//   2. **Tonemap** (ACES w przybliżeniu Narkowicza) sprowadza HDR do 0…1.
//   3. **FXAA** działa na obrazie **po** tonemapie, na percepcyjnej luminancji. Antyaliasing
//      na wartościach HDR wygładza liczby, których nikt nie zobaczy, a zostawia schodki tam,
//      gdzie kontrast powstaje dopiero przy kompresji zakresu.
//
// FXAA, a nie TAA: TAA potrzebuje wektorów ruchu i historii klatek, czyli dwóch rzeczy,
// których M1 nie ma. FXAA kosztuje jeden przebieg i nie wnosi smużenia.

@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var scene_sampler: sampler;
struct PostCfg {
    // x = ekspozycja, y = siła FXAA (0 = wyłączone), zw = rozmiar piksela w UV
    params: vec4<f32>,
}
@group(0) @binding(2) var<uniform> cfg: PostCfg;

struct VertexOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VertexOut {
    // Jeden trójkąt przykrywający ekran, nie dwa: brak szwu po przekątnej i o jeden
    // wierzchołek mniej do rasteryzacji na krawędzi.
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    var out: VertexOut;
    out.uv = vec2<f32>(x, 1.0 - y);
    out.clip_pos = vec4<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.0, 1.0);
    return out;
}

fn tonemap(hdr: vec3<f32>) -> vec3<f32> {
    let c = hdr * cfg.params.x;
    let m = (c * (2.51 * c + 0.03)) / (c * (2.43 * c + 0.59) + 0.14);
    return clamp(m, vec3<f32>(0.0), vec3<f32>(1.0));
}

/// Luminancja percepcyjna — wagi z BT.709. FXAA porównuje właśnie ją, a nie jasność
/// kanałów: krawędź między czerwienią a zielenią o tej samej jasności nie jest krawędzią,
/// którą widać jako schodki.
fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn probka(uv: vec2<f32>) -> vec3<f32> {
    return tonemap(textureSampleLevel(scene, scene_sampler, uv, 0.0).rgb);
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let px = cfg.params.zw;
    let srodek = probka(in.uv);
    if (cfg.params.y <= 0.0) {
        return vec4<f32>(srodek, 1.0);
    }

    // FXAA w wersji krawędziowej: kontrast lokalny wyznacza kierunek krawędzi, a próbka
    // pobrana **w poprzek** niej wygładza schodek. Pełny FXAA 3.11 z przeszukiwaniem wzdłuż
    // krawędzi kosztuje kilkanaście próbek więcej i przy voxelowej geometrii — samych
    // prostych krawędziach pod kątami 0°, 45° i 90° — nie widać różnicy.
    let l_c = luma(srodek);
    let l_n = luma(probka(in.uv + vec2<f32>(0.0, -px.y)));
    let l_s = luma(probka(in.uv + vec2<f32>(0.0, px.y)));
    let l_w = luma(probka(in.uv + vec2<f32>(-px.x, 0.0)));
    let l_e = luma(probka(in.uv + vec2<f32>(px.x, 0.0)));

    let lo = min(l_c, min(min(l_n, l_s), min(l_w, l_e)));
    let hi = max(l_c, max(max(l_n, l_s), max(l_w, l_e)));
    let kontrast = hi - lo;
    // Próg względny **i** bezwzględny: bez względnego wygładzałby się szum w cieniu,
    // bez bezwzględnego — gradienty na jasnym niebie.
    if (kontrast < max(0.0312, hi * 0.125)) {
        return vec4<f32>(srodek, 1.0);
    }

    let poziomo = abs(l_w + l_e - 2.0 * l_c) >= abs(l_n + l_s - 2.0 * l_c);
    var kierunek = vec2<f32>(0.0, px.y);
    if (poziomo) {
        kierunek = vec2<f32>(px.x, 0.0);
    }
    // Przesunięcie w stronę ciemniejszej strony krawędzi — tam leży schodek.
    let a = probka(in.uv - kierunek);
    let b = probka(in.uv + kierunek);
    let mieszane = (srodek * 2.0 + a + b) * 0.25;
    return vec4<f32>(mix(srodek, mieszane, cfg.params.y), 1.0);
}
