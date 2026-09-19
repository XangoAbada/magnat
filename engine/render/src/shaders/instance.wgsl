// Encje dynamiczne rysowane instancjonowanie (M11a §5.3, WP2).
//
// Jedna siatka na (model, poziom detalu), wiele instancji. Model **nie niesie koloru** —
// niesie rolę slotu; barwę wybiera ten shader z zestawu palety dzielnicy, funkcją
// `(wariant, rola)`. Stąd „auto należy do konkretnego gospodarstwa i ma swój kolor"
// bez duplikowania siatek (PRD §16.3).
//
// **Pozycja instancji jest względem kamery**, nie absolutna (`camera.rs`): przeliczenie
// robi `instancing::build_instances` w `f64`, bo przy współrzędnej 8 km krok `f32`
// to pół metra i pieszy zaczyna drgać (ryzyko R11 fazy).
//
// Ten sam moduł ma dwa punkty wejścia fragmentu: `fs_main` maluje encję w scenie HDR,
// `fs_id` zapisuje identyfikator encji do bufora ID. Jeden moduł, bo geometria musi być
// **bit w bit ta sama** — bufor ID jest rysowany z porównaniem głębi `Equal`.

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
// Płaska tablica barw RGBA8 spakowanych w u32 (R w najmłodszym bajcie).
@group(0) @binding(1) var<storage, read> palette_colors: array<u32>;
// Zestaw barw per (paleta dzielnicy, rola): x = przesunięcie, y = długość.
@group(0) @binding(2) var<storage, read> palette_ramps: array<vec2<u32>>;
// Atlas póz w jednym buforze: najpierw tablica wejść (`model * POSE_MAX_CLIPS + klip`,
// `x` = absolutny indeks pierwszego teksela, `y` = `klatki | części << 8`), zaraz za nią
// same pozy — po jednej na (klatka, część), 16 B, dwa `i16` w słowie, w kolejności
// `rot.xy | rot.zw | trans.xy | trans.z`. `klatki == 0` znaczy „ten model nie zna tego
// klipu" i jest poprawnym stanem: rysuje się wtedy poza spoczynkowa.
//
// Jeden bufor, bo `downlevel_defaults` daje cztery bufory storage na etap shadera,
// a render potrzebuje palety (dwa), póz i sylwetek.
@group(0) @binding(3) var<storage, read> pose_atlas: array<vec4<u32>>;

// Te trzy liczby muszą się zgadzać z `magnat_voxel::palette`. Rozjazd nie daje błędu,
// tylko inne barwy na ekranie niż w testach — dlatego pilnuje go test
// `shadery.rs::stale_palety_zgadzaja_sie_z_kodem`, który je stąd parsuje.
const PALETTE_MIX: u32 = 0x9E3779B9u;
const PALETTE_ROLE_SALT: u32 = 0x85EBCA6Bu;
const PALETTE_ROLE_COUNT: u32 = 12u;

// Ćwiartka voxela modelu: 0,25 m / 4.
const QUARTER_VOXEL_M: f32 = 0.0625;

// Te dwie muszą się zgadzać z `magnat_voxel::anim`; pilnuje tego ten sam test,
// który pilnuje stałych palety.
const POSE_MAX_CLIPS: u32 = 64u;
const POSE_TRANS_SCALE: f32 = 8.0;

const TAU: f32 = 6.2831853;

struct Vertex {
    // Pozycja w ćwiartkach voxela; `w` jest wypełniaczem układu, nie współrzędną.
    @location(0) pos_qv: vec4<i32>,
    // x: numer ściany, y: slot, z: numer części, w: rola slotu.
    @location(1) attrs: vec4<u32>,
}

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
    @location(0) normal: vec3<f32>,
    @location(1) @interpolate(flat) albedo: vec3<f32>,
    @location(2) @interpolate(flat) tint: vec4<f32>,
    @location(3) @interpolate(flat) pick: u32,
    @location(4) dist: f32,
    @location(5) @interpolate(flat) emissive: f32,
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

// Ta sama arytmetyka co `magnat_voxel::palette::pick`: xor stałą roli i mnożenie przez
// liczbę nieparzystą to bijekcja na u32, więc dwa różne warianty nie sklejają się
// w jeden przed resztą z dzielenia.
fn pick_color(variant: u32, role: u32, ramp: vec2<u32>) -> u32 {
    if ramp.y == 0u {
        return 0xFFFF00FFu;
    }
    let v = (variant ^ (role * PALETTE_ROLE_SALT)) * PALETTE_MIX;
    return palette_colors[ramp.x + ((v >> 13u) % ramp.y)];
}

// RGBA8 → barwa liniowa. Paleta jest pisana w sRGB, bo tak ją widzi człowiek
// w pliku danych, a bufor sceny jest liniowy (HDR + tonemap w post-processingu).
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

// Dolny i górny `i16` ze słowa, ze znakiem. WGSL ma arytmetyczne przesunięcie
// w prawo dla `i32`, więc `(w << 16) >> 16` rozszerza znak bez gałęzi.
fn i16_lo(w: u32) -> f32 {
    return f32(i32(w << 16u) >> 16u);
}

fn i16_hi(w: u32) -> f32 {
    return f32(i32(w) >> 16u);
}

// Obrót wektora quaternionem — postać Rodriguesa, dwa iloczyny wektorowe.
fn q_rot(q: vec4<f32>, v: vec3<f32>) -> vec3<f32> {
    let t = 2.0 * cross(q.xyz, v);
    return v + q.w * t + cross(q.xyz, t);
}

// Poza części w tej klatce klipu. Wierzchołek niesie pozycję **spoczynkową**
// z wliczonym łańcuchem pivotów, a atlas trzyma translację już o nią skorygowaną
// (`t = t_global − R·rest`), więc całość to `p' = R·p + t` — bez tablicy przesunięć
// po stronie shadera i bez iterowania po rodzicach (`F-2`, M11b §5.4).
fn apply_pose(model: u32, clip: u32, phase: u32, part: u32, p: ptr<function, vec3<f32>>, n: ptr<function, vec3<f32>>) {
    // Klip spoza katalogu (`magnat_voxel::NO_CLIP`) znaczy „poza spoczynkowa" i jest
    // sprawdzany **przed** sięgnięciem do tablicy, bo indeks poza zakresem w WGSL
    // nie jest błędem, tylko cudzym wpisem.
    if (clip >= POSE_MAX_CLIPS) {
        return;
    }
    let e = pose_atlas[model * POSE_MAX_CLIPS + clip];
    let frames = e.y & 0xFFu;
    if (frames == 0u) {
        return;
    }
    let parts = (e.y >> 8u) & 0xFFu;
    // Liczba klatek jest potęgą dwójki (pilnuje tego test biblioteki klipów), więc
    // maska zastępuje dzielenie i faza zawija się bez przeskoku.
    let frame = phase & (frames - 1u);
    let w = pose_atlas[e.x + frame * parts + min(part, parts - 1u)];
    let q = vec4<f32>(i16_lo(w.x), i16_hi(w.x), i16_lo(w.y), i16_hi(w.y)) / 32767.0;
    let t = vec3<f32>(i16_lo(w.z), i16_hi(w.z), i16_lo(w.w)) * (QUARTER_VOXEL_M / POSE_TRANS_SCALE);
    *p = q_rot(q, *p) + t;
    *n = q_rot(q, *n);
}

@vertex
fn vs_main(v: Vertex, inst: Instance) -> VsOut {
    var local = vec3<f32>(f32(v.pos_qv.x), f32(v.pos_qv.y), f32(v.pos_qv.z)) * QUARTER_VOXEL_M;
    var n = face_normal(v.attrs.x);
    // Animacja jest **przed** obrotem instancji: klip działa w przestrzeni modelu,
    // a `yaw` ustawia gotową postać w świecie.
    apply_pose(
        (inst.pal_model >> 18u),
        inst.anim & 0xFFu,
        (inst.anim >> 8u) & 0xFFu,
        v.attrs.z,
        &local,
        &n,
    );
    let yaw = f32(inst.yaw_pitch & 0xFFFFu) / 65536.0 * TAU;
    let cs = cos(yaw);
    let sn = sin(yaw);
    // Obrót wokół pionu (Z). Nachylenie z `yaw_pitch >> 16` wchodzi razem z rampami
    // i pojazdami na wzniesieniu w M11b — tu byłoby polem bez czytelnika.
    let obrocone = vec3<f32>(
        local.x * cs - local.y * sn,
        local.x * sn + local.y * cs,
        local.z,
    );
    let n_obr = vec3<f32>(n.x * cs - n.y * sn, n.x * sn + n.y * cs, n.z);
    let world = inst.pos + obrocone;

    let role = min(v.attrs.w, PALETTE_ROLE_COUNT - 1u);
    let paleta = inst.pal_model & 0xFFFFu;
    let ramp = palette_ramps[paleta * PALETTE_ROLE_COUNT + role];

    var out: VsOut;
    out.clip_pos = frame.view_proj * vec4<f32>(world, 1.0);
    out.normal = n_obr;
    out.albedo = unpack_srgb(pick_color(inst.variant, role, ramp));
    out.tint = unpack_rgba(inst.tint);
    out.pick = inst.pick;
    out.dist = length(world);
    // Rola `Emissive` ma numer 11 — okna, reflektory, palniki świecą własnym światłem.
    out.emissive = select(0.0, 1.0, role == 11u);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let ndl = max(dot(n, frame.sun_dir.xyz), 0.0);
    // Ambient **ten sam co teren**: mieszanka barwy gruntu i nieba wg pochylenia ściany,
    // a nie stały ułamek nieba. Bez tego encja stojąca na trawie ma inne światło niż
    // trawa, na której stoi, i wygląda jak wklejona.
    let up = n.z * 0.5 + 0.5;
    let ambient = mix(frame.ground_color.rgb, frame.sky_color.rgb, up);
    var color = in.albedo * (frame.sun_color.rgb * ndl + ambient);
    // Element świecący nie gaśnie w cieniu — to jest cała różnica między lampą a blachą.
    color = mix(color, in.albedo * 3.0, in.emissive);
    // Podświetlenie filtra encji (§14.2): alfa niesie siłę, więc zero nic nie zmienia.
    color = mix(color, in.tint.rgb, in.tint.a);
    // Ta sama mgła co teren — inaczej encje w oddali wisiałyby nad panoramą.
    let fog = 1.0 - exp(-in.dist * frame.fog.w);
    color = mix(color, frame.fog.rgb, clamp(fog, 0.0, 1.0));
    // Kolor wychodzi **liniowy i bez ekspozycji**: ekspozycja i tonemap są w passie
    // post-processingu, tak jak w `voxel.wgsl`. `pedestrian.wgsl` mnożył tu przez
    // `sun_dir.w` i przez to piesi dostawali ekspozycję dwa razy — stąd brali się biali
    // ludzie na kolorowym mieście przy każdej palecie, jaką się im dało.
    return vec4<f32>(color, 1.0);
}

@fragment
fn fs_id(in: VsOut) -> @location(0) u32 {
    // Wartość jest już spakowana po stronie CPU (`instancing::pick_id`): rodzaj encji
    // w czterech starszych bitach, indeks przesunięty o jeden w dwudziestu ośmiu.
    // Zero znaczy „nic", bo tekstura jest czyszczona zerem.
    return in.pick;
}
