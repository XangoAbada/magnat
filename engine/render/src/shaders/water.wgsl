// Tafla wody — przebieg naprzód (M1 §5.8, WP-R5).
//
// Geometria jest ta sama co terenu (te same wierzchołki chunka, tylko inny zakres indeksów),
// więc shader wierzchołka powtarza rozpakowanie z `voxel.wgsl`. Powtórzenie jest tu tańsze
// niż wspólny moduł: oba shadery pakują ten sam format, ale liczą inne rzeczy we fragmencie,
// a `voxel.wgsl` jest kompilowany także jako pass głębi i cienia.
//
// Cztery rzeczy odróżniają wodę od nieprzezroczystego voxela i każda jest tu z powodu:
//   1. **Fresnel** — pod małym kątem woda odbija niebo, pod dużym widać dno. Bez tego tafla
//      wygląda jak niebieski beton niezależnie od tego, skąd się patrzy.
//   2. **Mgła głębinowa** — barwa zależy od tego, ile wody jest **pod** pikselem, czyli od
//      różnicy głębi z terenem. Stąd płycizny są jaśniejsze, a środek jeziora ciemny.
//   3. **Miękki brzeg** — ta sama różnica głębi wygasza krycie przy linii brzegowej, więc
//      styk wody z lądem nie jest ostrą kreską.
//   4. **Animowane normalne** — dwie fale w przeciwnych kierunkach; bez ruchu tafla wygląda
//      jak szkło, a nie jak woda.

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

struct ChunkData {
    origin_scale: vec4<f32>,
}

@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<storage, read> chunks: array<ChunkData>;
@group(0) @binding(2) var<storage, read> materials: array<vec4<f32>>;
// Głębia sceny **po** przebiegu nieprzezroczystym — stąd bierze się grubość słupa wody.
@group(1) @binding(0) var scene_depth: texture_depth_2d;
// Czas w sekundach do animacji fal; osobny uniform, bo to jedyna wielkość w rendererze,
// która ma prawo płynąć niezależnie od zegara symulacji.
@group(1) @binding(1) var<uniform> czas: vec4<f32>;

const VOXEL_HEIGHT_M: f32 = 0.5;

struct VertexOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) world_pos: vec3<f32>,
    @location(2) view_dist: f32,
}

@vertex
fn vs_main(
    @location(0) packed: vec2<u32>,
    @builtin(instance_index) chunk_index: u32,
) -> VertexOut {
    let lo = packed.x;
    let material = packed.y & 0xFFFFu;
    let chunk = chunks[chunk_index];
    let scale = chunk.origin_scale.w;
    let local = vec3<f32>(
        f32(lo & 0x3Fu) * scale,
        f32((lo >> 6u) & 0x3Fu) * scale,
        f32((lo >> 12u) & 0x3Fu) * scale * VOXEL_HEIGHT_M,
    );
    let pos = chunk.origin_scale.xyz + local;

    var out: VertexOut;
    out.clip_pos = frame.view_proj * vec4<f32>(pos, 1.0);
    out.color = materials[material].xyz;
    out.world_pos = pos;
    out.view_dist = length(pos);
    return out;
}

/// Zaburzenie normalnej dwiema falami. Nie szum i nie tekstura: dwie sinusoidy o różnych
/// kierunkach i okresach wystarczają, żeby odbicie się ruszało, a kosztują cztery działania.
fn normalna_fali(p: vec2<f32>, t: f32) -> vec3<f32> {
    let a = sin(p.x * 0.35 + t * 0.9) * 0.03;
    let b = sin((p.y + p.x * 0.4) * 0.23 - t * 0.6) * 0.025;
    return normalize(vec3<f32>(a, b, 1.0));
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    if (frame.clip.y > 0.5 && (in.world_pos.z + frame.clip.z) > frame.clip.x) {
        discard;
    }

    let n = normalna_fali(in.world_pos.xy, czas.x);
    let do_oka = normalize(-in.world_pos);

    // Grubość słupa wody: różnica między głębią dna (zapisaną w przebiegu nieprzezroczystym)
    // a głębią tafli. Obie są nieliniowe, więc porównujemy je po odległości od kamery —
    // odwrotność `w` z przestrzeni obcinania jest właśnie tą odległością.
    let piksel = vec2<i32>(i32(in.clip_pos.x), i32(in.clip_pos.y));
    let ndc_dna = textureLoad(scene_depth, piksel, 0);
    // Odległości w metrach, nie wartości z bufora: rozkład głębi jest nieliniowy, więc
    // różnica surowych `z` znaczy co innego przy brzegu niż na środku jeziora — i tafla
    // rozpadała się na jasne i ciemne kwadraty zależnie od odległości.
    let d_dna = frame.screen.z / (ndc_dna + frame.screen.w);
    let d_tafli = 1.0 / in.clip_pos.w;
    // Sześć metrów wody to pełne nasycenie barwy toni; głębiej różnicy i tak nie widać.
    let grubosc = clamp(max(0.0, d_dna - d_tafli) / 6.0, 0.0, 1.0);

    // Fresnel Schlicka dla współczynnika załamania wody (F0 ≈ 0,02).
    let cos_theta = clamp(dot(n, do_oka), 0.0, 1.0);
    let fresnel = 0.02 + 0.98 * pow(1.0 - cos_theta, 5.0);

    let ndotl = max(dot(n, frame.sun_dir.xyz), 0.0);
    let blask = pow(max(dot(reflect(-frame.sun_dir.xyz, n), do_oka), 0.0), 64.0);

    // Barwa: od jasnej płycizny do ciemnej toni, plus odbite niebo według Fresnela.
    let plytko = in.color * 1.6;
    let gleboko = in.color * 0.45;
    let woda = mix(plytko, gleboko, grubosc);
    var color = mix(woda * (0.35 + 0.65 * ndotl), frame.sky_color.xyz, fresnel);
    color = color + frame.sun_color.xyz * blask * 0.6;

    let fog = 1.0 - exp(-in.view_dist * frame.fog.w);
    color = mix(color, frame.fog.xyz, clamp(fog, 0.0, 1.0));


    // Krycie rośnie z grubością: przy brzegu tafla jest niemal przezroczysta, więc styk
    // z lądem nie jest kreską. Fresnel podbija krycie pod małym kątem, bo tam i tak
    // widać odbicie, a nie dno.
    let krycie = clamp(0.25 + 0.75 * grubosc + fresnel * 0.4, 0.0, 1.0);
    return vec4<f32>(color, krycie);
}
