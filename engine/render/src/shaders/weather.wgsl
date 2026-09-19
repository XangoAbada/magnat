// Cząstki pogody i dymu (M11d §5.8, WP7).
//
// Ani jedna cząstka nie ma stanu. Pozycja kropli jest funkcją numeru instancji i czasu:
// numer rozsiewa ją po pudle wokół kamery, czas przesuwa w dół modulo wysokość pudła.
// Dzięki temu deszcz to jedno wywołanie rysowania bez bufora, bez kroku compute
// i bez podwójnego buforowania — a „wrapping" jest darmowy, bo jest resztą z dzielenia.
//
// Cały układ współrzędnych jest **względem kamery** (`camera.rs`), więc pudło cząstek
// ma środek w zerze i nie potrzebuje pozycji oka.

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

struct Weather {
    // x = natężenie opadu 0..1, y = 0 deszcz / 1 śnieg, z = czas renderu w s,
    // w = pokrywa śnieżna 0..1
    params: vec4<f32>,
    // xy = wiatr w m/s, z = bok pudła w poziomie, w = światło dzienne 0..1
    wind: vec4<f32>,
}

struct Plume {
    pos_density: vec4<f32>,   // xyz względem kamery, w = gęstość 0..1
    tint_rise: vec4<f32>,     // xyz = barwa, w = prędkość wznoszenia m/s
}

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<uniform> weather: Weather;
@group(0) @binding(2) var<storage, read> plumes: array<Plume>;

// Wysokość pudła cząstek. Musi się zgadzać z `PRECIP_BOX_Z_M` po stronie Rusta —
// pilnuje tego test `stale_pogody_zgadzaja_sie_z_kodem` w `tests/shaders.rs`.
const BOX_Z_M: f32 = 40.0;
// Prędkość opadania deszczu i śniegu w m/s. Śnieg leci wolniej i o to chodzi.
const RAIN_FALL_MS: f32 = 9.0;
const SNOW_FALL_MS: f32 = 1.2;
// Ile cząstek przypada na jeden komin — `PARTICLES_PER_PLUME` po stronie Rusta.
const PARTICLES_PER_PLUME: u32 = 64u;
// Ile sekund żyje cząstka dymu.
const PLUME_LIFE_S: f32 = 6.0;
// Poniżej tylu metrów od oka cząstka gaśnie. Bez tego kropla przelatująca dwadzieścia
// centymetrów przed kamerą zajmuje pół ekranu — jest fizycznie na miejscu i wygląda
// jak usterka. Prawdziwe oko też jej nie widzi, bo nie ma jej jak zogniskować.
const NEAR_FADE_M: f32 = 4.0;

// Rozsiewacz: ten sam numer daje zawsze ten sam punkt, więc kropla nie przeskakuje
// między klatkami. Trzy niezależne mieszania, bo trzy współrzędne skorelowane ze sobą
// dałyby deszcz padający po przekątnej siatki.
fn hash3(i: u32) -> vec3<f32> {
    var h = i * 747796405u + 2891336453u;
    h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
    let a = (h ^ (h >> 22u)) & 0xFFFFFFu;
    h = h * 1664525u + 1013904223u;
    let b = (h ^ (h >> 15u)) & 0xFFFFFFu;
    h = h * 1664525u + 1013904223u;
    let c = (h ^ (h >> 15u)) & 0xFFFFFFu;
    return vec3<f32>(f32(a), f32(b), f32(c)) / 16777216.0;
}

// Prawa i górna oś kamery w świecie, odzyskane z macierzy widoku i rzutowania.
//
// `clip.x = dot(wiersz0(VP), świat)`, a wiersz 0 macierzy rzutowania perspektywicznego
// to skalowany wiersz 0 macierzy widoku, czyli wektor „w prawo" kamery. Normalizacja
// zdejmuje skalę. Dzięki temu billboard nie potrzebuje osobnego uniformu z bazą kamery.
fn camera_right() -> vec3<f32> {
    return normalize(vec3<f32>(
        frame.view_proj[0].x,
        frame.view_proj[1].x,
        frame.view_proj[2].x,
    ));
}

fn camera_up() -> vec3<f32> {
    return normalize(vec3<f32>(
        frame.view_proj[0].y,
        frame.view_proj[1].y,
        frame.view_proj[2].y,
    ));
}

// Sześć wierzchołków quada z numeru wierzchołka: dwa trójkąty, rogi w [-1, 1].
fn corner(vi: u32) -> vec2<f32> {
    let x = f32((vi == 1u) || (vi == 2u) || (vi == 4u)) * 2.0 - 1.0;
    let y = f32((vi == 2u) || (vi == 4u) || (vi == 5u)) * 2.0 - 1.0;
    return vec2<f32>(x, y);
}

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    // Odległość kółka od środka w [-1, 1]² — fragment wycina z quada owal.
    @location(1) uv: vec2<f32>,
    // 0 = kreska deszczu (nie wycinamy), 1 = płatek/kłąb (wycinamy).
    @location(2) round: f32,
}

@vertex
fn vs_precip(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> Out {
    let snieg = weather.params.y > 0.5;
    let bok = weather.wind.z;
    let h = hash3(ii);

    // Rozsianie po pudle wokół kamery. Pudło jedzie z kamerą, więc środek jest w zerze.
    var p = vec3<f32>((h.x - 0.5) * bok, (h.y - 0.5) * bok, 0.0);

    // Opadanie modulo wysokość pudła — to jest cały „wrapping". Śnieg dodatkowo
    // kołysze się bokiem, bo płatek spadający pionowo wygląda jak deszcz na biało.
    var predkosc = RAIN_FALL_MS;
    if (snieg) { predkosc = SNOW_FALL_MS; }
    let faza = fract(h.z + weather.params.z * predkosc / BOX_Z_M);
    p.z = BOX_Z_M * (0.5 - faza);
    // Wiatr znosi tor opadania; przy śniegu dochodzi kołysanie o okresie zależnym
    // od numeru cząstki, żeby cały opad nie falował jednym taktem.
    let czas_lotu = faza * BOX_Z_M / predkosc;
    p.x += weather.wind.x * czas_lotu;
    p.y += weather.wind.y * czas_lotu;
    if (snieg) {
        p.x += sin(weather.params.z * 1.7 + h.x * 40.0) * 0.4;
        p.y += cos(weather.params.z * 1.4 + h.y * 40.0) * 0.4;
    }

    let c = corner(vi);
    var o: Out;
    var os_x: vec3<f32>;
    var os_y: vec3<f32>;
    var barwa: vec4<f32>;
    if (snieg) {
        // Płatek: mały kwadrat zwrócony do kamery, wycinany do kółka we fragmencie.
        os_x = camera_right() * 0.022;
        os_y = camera_up() * 0.022;
        barwa = vec4<f32>(1.0, 1.0, 1.0, 0.85);
        o.round = 1.0;
    } else {
        // Kreska deszczu wzdłuż wektora prędkości. Bok bierzemy z prawej osi kamery
        // odjętej wzdłuż kreski (Gram-Schmidt) — kreska jest wtedy najszersza na ekranie.
        let v = normalize(vec3<f32>(weather.wind.x, weather.wind.y, -RAIN_FALL_MS));
        let r = camera_right();
        var bok_w = r - v * dot(r, v);
        if (length(bok_w) < 1e-4) { bok_w = camera_up(); }
        os_x = normalize(bok_w) * 0.008;
        os_y = v * 0.26;
        barwa = vec4<f32>(0.72, 0.80, 0.92, 0.42);
        o.round = 0.0;
    }

    // Deszcz jest jaśniejszy w dzień, bo odbija niebo; w nocy widać go głównie
    // w stożkach latarni, a tych nie próbkujemy — stąd przyciemnienie mnożnikiem.
    let swiatlo = 0.25 + 0.75 * weather.wind.w;
    let blisko = smoothstep(0.0, NEAR_FADE_M, length(p));
    barwa = vec4<f32>(barwa.rgb * swiatlo, barwa.a * weather.params.x * blisko);

    let swiat = p + os_x * c.x + os_y * c.y;
    o.pos = frame.view_proj * vec4<f32>(swiat, 1.0);
    o.color = barwa;
    o.uv = c;
    return o;
}

@vertex
fn vs_smoke(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> Out {
    let e = ii / PARTICLES_PER_PLUME;
    let k = ii % PARTICLES_PER_PLUME;
    let pl = plumes[e];
    let h = hash3(ii * 2654435761u);

    // Wiek cząstki rozsunięty po kominie: 64 kłęby w równych odstępach plus rozrzut,
    // żeby pióropusz nie wyglądał jak sznur korali.
    let faza = fract(
        weather.params.z / PLUME_LIFE_S + f32(k) / f32(PARTICLES_PER_PLUME) + h.x * 0.12
    );
    let wiek = faza * PLUME_LIFE_S;

    var p = pl.pos_density.xyz;
    p.z += pl.tint_rise.w * wiek;
    p.x += weather.wind.x * wiek * 0.6 + (h.y - 0.5) * wiek * 1.1;
    p.y += weather.wind.y * wiek * 0.6 + (h.z - 0.5) * wiek * 1.1;

    // Kłąb rośnie z wiekiem i blednie. Gęstość zakładu skaluje krycie, nie rozmiar:
    // słabo dymiący komin ma dawać rzadszy dym, a nie mniejszy.
    let promien = 1.2 + wiek * 1.1;
    let c = corner(vi);
    let swiatlo = 0.35 + 0.65 * weather.wind.w;

    var o: Out;
    let swiat = p + camera_right() * c.x * promien + camera_up() * c.y * promien;
    o.pos = frame.view_proj * vec4<f32>(swiat, 1.0);
    o.color = vec4<f32>(
        pl.tint_rise.xyz * swiatlo,
        pl.pos_density.w * (1.0 - faza) * 0.18 * smoothstep(0.0, NEAR_FADE_M, length(p)),
    );
    o.uv = c;
    o.round = 1.0;
    return o;
}

@fragment
fn fs_main(in: Out) -> @location(0) vec4<f32> {
    var a = in.color.a;
    if (in.round > 0.5) {
        // Miękkie kółko z kwadratu: bez tego dym jest chmurą sześcianów.
        let d = length(in.uv);
        if (d > 1.0) { discard; }
        a *= 1.0 - d * d;
    }
    if (a <= 0.002) { discard; }
    return vec4<f32>(in.color.rgb, a);
}
