//! Animacja części sztywnych (M11b §5.4, WP3).
//!
//! Model voxelowy nie ma się jak deformować — ma się obracać w stawach (§5.1). Cała
//! animacja sprowadza się więc do jednej liczby na część: obrotu wokół pivota, złożonego
//! wzdłuż łańcucha rodziców. Te obroty są **wypalane przy starcie** do [`PoseAtlas`] —
//! statycznej tablicy, którą vertex shader czyta indeksem `(model, klip, klatka, część)`.
//!
//! ### Zero bajtów stanu animacji
//!
//! Instancja niesie `(clip, phase)` i nic więcej (`GpuInstance::anim`). Fazę liczy
//! [`anim_phase`] — funkcja czysta od chwili i indeksu encji — a liczy ją **wypełniacz
//! snapshotu**, nie symulacja (decyzja 9.5 fazy M11, zamknięta). W ECS nie ma ani bajta
//! stanu animacji, więc animacja nie wchodzi do hasha stanu i nie ma jak zmienić wyniku
//! ekonomicznego. Składnik `entity_index` rozsuwa fazy między agentami — tłum nie
//! maszeruje w takt.
//!
//! ### Dlaczego poza jest złożona po stronie procesora
//!
//! Atlas trzyma **obrót globalny w przestrzeni modelu**, a nie lokalny w stawie: shader
//! dostaje jeden quaternion i jedną translację na część i nie ma po czym iterować.
//! Alternatywa — hierarchia w shaderze — znaczyłaby pętlę po rodzicach w każdym
//! wierzchołku, czyli cztery pobrania z pamięci tam, gdzie wystarcza jedno.
//!
//! Translacja jest przy tym **skorygowana o pozę spoczynkową**: wierzchołek siatki niesie
//! pozycję z wliczonym łańcuchem pivotów ([`crate::model_mesh`]), więc shader liczy
//! `p' = R·p + t`, gdzie `t = t_global − R·rest`. Stąd `part_rest_qv` — żeby nie było
//! drugiego miejsca, które wyprowadza te przesunięcia (`F-2`).

use crate::model::{ModelId, ModelLibrary, PartName, VoxModel};
use crate::model_mesh::rest_offsets;
use glam::{Quat, Vec3};

/// Klatek na sekundę każdego klipu — świadomy „stop-motion", spójny ze stylem voxelowym.
///
/// ponytail: jedna stała zamiast pola `fps` w klipie z §5.4. Pole miałoby sens dopiero
/// wtedy, gdyby fazę liczył shader z własnego zegara; przy fazie liczonej raz na encję
/// (§5.4, „faza jest funkcją czystą od chwili") różne `fps` w jednym snapshocie znaczyłyby
/// tyle, że jedna liczba musi opisać dwa zegary naraz. Podniesienie do 24 dla wszystkich
/// klipów (ryzyko `R9`) jest zmianą tej stałej i przeliczeniem atlasu.
pub const ANIM_FPS: u64 = 12;

/// Sufit liczby klipów. Indeks `(model, klip)` w [`PoseAtlas`] jest płaski, więc ta stała
/// mnoży się przez liczbę modeli — 64 klipy × 64 modele to 32 KB tablicy wejść.
/// `ClipId` jest bajtem, więc twardy sufit i tak stoi na 256 (`F-3`).
pub const MAX_CLIPS: usize = 64;

/// Skala translacji w atlasie: jednostka to 1/8 ćwiartki voxela, czyli 7,8 mm.
///
/// Ćwiartka voxela (6,25 cm) jest za gruba na **złożoną** translację: przy obrocie uda
/// o 30° błąd zaokrąglenia przenosi stopę o centymetry i noga zaczyna wibrować między
/// klatkami. Zakres `i16` przy tej skali to ±16 m — dwa razy więcej niż największy model.
pub const POSE_TRANS_SCALE: f32 = 8.0;

/// Klip, którego nie ma: encja rysuje się w pozie spoczynkowej.
///
/// Wartość spoza [`MAX_CLIPS`], więc shader rozpoznaje ją jednym porównaniem i nie
/// sięga do tablicy wejść. Istnieje dla pomiaru z kryterium WP3 („dodatkowy czas GPU
/// **względem pozy bazowej**") — bez niej tej samej sceny nie da się zmierzyć dwa razy.
pub const NO_CLIP: ClipId = ClipId(255);

/// Indeks klipu w [`ClipLibrary`]. Jeden bajt — mieści się w `GpuInstance::anim` (`F-3`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct ClipId(pub u8);

/// Klasa animacji pracy. **Wiele ról zawodowych na jeden klip** — to jest mitygacja
/// ryzyka `R4`: kasjer, recepcjonista i urzędnik dzielą „pracę przy ladzie", więc
/// 46 ról z `data/jobs/roles.ron` schodzi do sześciu klipów, a nie rośnie z katalogiem.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub enum WorkStyle {
    /// Lada, kasa, okienko — ruch przedramion, reszta stoi.
    Counter,
    /// Magazyn — podnoszenie i odkładanie całymi ramionami.
    Warehouse,
    /// Linia produkcyjna — krótki, rytmiczny ruch.
    Line,
    /// Biurko. Najmniej ruchu ze wszystkich.
    #[default]
    Office,
    /// Budowa i warsztat — zamach narzędziem.
    Construction,
    /// Za kierownicą.
    Driving,
}

impl WorkStyle {
    pub const ALL: [WorkStyle; 6] = [
        WorkStyle::Counter,
        WorkStyle::Warehouse,
        WorkStyle::Line,
        WorkStyle::Office,
        WorkStyle::Construction,
        WorkStyle::Driving,
    ];

    /// Styl pracy z klucza roli zawodowej (`data/jobs/roles.ron`).
    ///
    /// Odwzorowanie stoi tutaj, a nie w danych, bo dotyczy **animacji**, a nie zawodu:
    /// dopisanie roli do katalogu nie ma wymuszać decyzji, którą machnięciem ręki ma się
    /// ona poruszać. Nieznany klucz dostaje [`WorkStyle::Office`] — postać, która stoi
    /// i lekko się rusza, wygląda poprawnie w każdym wnętrzu, a błędny zamach młotem
    /// w banku widać z drugiego końca ulicy.
    #[must_use]
    pub fn from_role_key(key: &str) -> WorkStyle {
        const COUNTER: [&str; 10] = [
            "cashier",
            "sales_assistant",
            "waiter",
            "bank_teller",
            "pharmacist",
            "hotel_attendant",
            "retail_clerk",
            "loan_officer",
            "leasing_agent",
            "dispatcher",
        ];
        const WAREHOUSE: [&str; 5] = [
            "warehouse_operator",
            "stocker",
            "forklift_operator",
            "logger",
            "miner",
        ];
        const LINE: [&str; 10] = [
            "production_worker",
            "machine_operator",
            "process_operator",
            "furnace_operator",
            "food_processor",
            "baker",
            "assembler",
            "printer_operator",
            "cook",
            "quality_inspector",
        ];
        const CONSTRUCTION: [&str; 8] = [
            "welder",
            "mechanic",
            "electrician",
            "service_technician",
            "drill_operator",
            "field_worker",
            "livestock_keeper",
            "cleaner",
        ];
        if key == "truck_driver" {
            return WorkStyle::Driving;
        }
        if COUNTER.contains(&key) {
            return WorkStyle::Counter;
        }
        if WAREHOUSE.contains(&key) {
            return WorkStyle::Warehouse;
        }
        if LINE.contains(&key) {
            return WorkStyle::Line;
        }
        if CONSTRUCTION.contains(&key) {
            return WorkStyle::Construction;
        }
        WorkStyle::Office
    }
}

/// **Projekcja `core::ActivityKind` na animację, nie drugi słownik czynności (`K-8`).**
///
/// To, *co* agent robi, jest własnością `engine/core`; `ClipKind` mówi tylko, *jak to
/// narysować*. Odwzorowanie jest wiele-do-jednego i żyje w [`clip_for`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub enum ClipKind {
    #[default]
    Idle,
    Walk,
    WalkCarry,
    Run,
    Sit,
    Stand,
    EnterVehicle,
    ExitVehicle,
    Board,
    Work(WorkStyle),
    Shop,
    Queue,
    Drive,
    Park,
    Refuel,
    Unload,
    Forklift,
    Crane,
    RampLoad,
}

/// Jedyne miejsce, w którym czynność domenowa staje się animacją.
///
/// Funkcja czysta, bez stanu. `match` jest **wyczerpujący bez ramienia `_`** celowo:
/// nowa czynność w `core` ma łamać kompilację tutaj, a nie po cichu rysować postać
/// stojącą bez ruchu (`K-12` stosuje tę samą zasadę do `DecisionReason`).
#[must_use]
pub fn clip_for(activity: magnat_core::ActivityKind, work: WorkStyle) -> ClipKind {
    use magnat_core::ActivityKind as A;
    match activity {
        A::Work => ClipKind::Work(work),
        A::Commute | A::Errand => ClipKind::Walk,
        A::Shop => ClipKind::Shop,
        // Nauka i posiłek są czynnościami „na siedząco"; na ulicy ich nie widać,
        // bo mieszkaniec jest wtedy we wnętrzu (M11c odsłania je cięciem poziomami).
        A::School | A::Eat => ClipKind::Sit,
        A::Sleep | A::Leisure | A::Social | A::Idle => ClipKind::Idle,
    }
}

/// Faza klipu: numer klatki modulo 256, rozsunięty indeksem encji.
///
/// `now_ms` to **czas animacji**, a nie czas świata: rośnie, gdy gra idzie, i staje przy
/// pauzie. Nie jest minutą symulacji, bo minuta trwa sekundę realną przy prędkości ×1
/// i klip przesuwałby się o jedną klatkę na sekundę.
///
/// Wynik jest brany modulo 256, a nie modulo długości klipu, bo długości klipu tu nie
/// znamy — dzieli ją shader. Stąd wymaganie, żeby [`AnimationClip::frames`] było potęgą
/// dwójki: inaczej przy zawinięciu licznika (co 21 s) klip przeskakiwałby o kilka klatek.
#[must_use]
pub fn anim_phase(now_ms: u64, entity_index: u32) -> u8 {
    let klatka = now_ms / (1000 / ANIM_FPS);
    (klatka.wrapping_add(u64::from(entity_index)) % 256) as u8
}

/// Stan animacji instancji. **Nie istnieje jako komponent ECS** — jest wyprowadzany
/// przy wypełnianiu snapshotu i mieści się w dwóch bajtach rekordu.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct AnimationState {
    pub clip: ClipId,
    pub phase: u8,
}

/// Ruch jednej części w klipie.
///
/// ponytail: ruch jest sinusoidalny albo żaden, i to jest cała gramatyka klipu. Krzywe
/// z klatkami kluczowymi znaczyłyby format danych, edytor i walidator — a chód figurki
/// o siedmiu voxelach wzrostu to wahnięcie czterech kończyn w przeciwfazie. Ścieżka
/// wyjścia, jeśli animator dostanie narzędzia: `Channel` staje się wektorem klatek,
/// [`PoseAtlas::bake`] przestaje liczyć sinus i zaczyna czytać plik.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Motion {
    /// Bez ruchu — zostaje samo pochylenie i przesunięcie kanału.
    #[default]
    Hold,
    /// Wahnięcie tam i z powrotem wokół osi (0 = X, 1 = Y, 2 = Z).
    Swing { axis: u8, amp_deg: i16, phase: u8 },
    /// Pełny obrót na cykl klipu — koła i wysięgniki.
    Spin { axis: u8 },
}

/// Co robi jedna część w klipie: stałe pochylenie, stałe przesunięcie i ruch.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Channel {
    pub part: PartName,
    /// Pochylenie stałe w stopniach wokół osi X, Y, Z — składane w tej kolejności.
    pub tilt_deg: [i16; 3],
    /// Stałe przesunięcie pivota w ćwiartkach voxela (przykucnięcie, podniesienie masztu).
    pub offset_qv: [i16; 3],
    pub motion: Motion,
}

impl Channel {
    #[must_use]
    pub fn new(part: PartName, motion: Motion) -> Channel {
        Channel {
            part,
            motion,
            ..Default::default()
        }
    }

    #[must_use]
    pub fn tilt(mut self, tilt_deg: [i16; 3]) -> Channel {
        self.tilt_deg = tilt_deg;
        self
    }

    #[must_use]
    pub fn offset(mut self, offset_qv: [i16; 3]) -> Channel {
        self.offset_qv = offset_qv;
        self
    }

    /// Obrót lokalny części w chwili `t` (0..1 cyklu).
    fn rotation(&self, t: f32) -> Quat {
        let base = Quat::from_rotation_z(radiany(self.tilt_deg[2]))
            * Quat::from_rotation_y(radiany(self.tilt_deg[1]))
            * Quat::from_rotation_x(radiany(self.tilt_deg[0]));
        match self.motion {
            Motion::Hold => base,
            Motion::Swing {
                axis,
                amp_deg,
                phase,
            } => {
                let f = t + f32::from(phase) / 256.0;
                let kat = radiany(amp_deg) * (f * std::f32::consts::TAU).sin();
                base * os(axis, kat)
            }
            Motion::Spin { axis } => base * os(axis, t * std::f32::consts::TAU),
        }
    }
}

fn radiany(stopnie: i16) -> f32 {
    f32::from(stopnie) * std::f32::consts::PI / 180.0
}

fn os(axis: u8, kat: f32) -> Quat {
    match axis {
        0 => Quat::from_rotation_x(kat),
        1 => Quat::from_rotation_y(kat),
        _ => Quat::from_rotation_z(kat),
    }
}

/// Klip animacji: ile klatek i co w nich robią części.
///
/// Pola `id`, `fps`, `loop_mode`, `pose_offset` i `parts_mask` z §5.4 tutaj nie stoją
/// i tabela zmian tego dokumentu mówi dlaczego: `id` nadaje [`ClipLibrary`] (pozycja
/// w katalogu), `fps` jest stałą ([`ANIM_FPS`]), `pose_offset` należy do atlasu, a nie
/// do klipu, `parts_mask` wynika z listy kanałów, a `loop_mode` nie ma czego opisywać —
/// ruch sinusoidalny jest zapętlony z definicji, a klip jednorazowy potrzebowałby chwili
/// startu zdarzenia, której rekord snapshotu nie niesie.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct AnimationClip {
    pub kind: ClipKind,
    /// Liczba klatek — **potęga dwójki** (8, 16, 32). Pilnuje tego walidator biblioteki.
    pub frames: u8,
    pub channels: Vec<Channel>,
}

impl AnimationClip {
    fn new(kind: ClipKind, frames: u8, channels: Vec<Channel>) -> AnimationClip {
        AnimationClip {
            kind,
            frames,
            channels,
        }
    }

    /// Kanał dotyczący danej części, jeśli klip ją rusza.
    #[must_use]
    pub fn channel(&self, part: PartName) -> Option<&Channel> {
        self.channels.iter().find(|c| c.part == part)
    }
}

mod clips;

/// Katalog klipów. Kolejność jest tożsamością: [`ClipId`] to pozycja w tej liście.
///
/// Klipy są **w kodzie, nie w `data/`**, i to jest decyzja: katalog danych na animacje
/// wymagałby formatu, walidatora i wpisu w 00 §5, a treścią byłoby dwadzieścia kilka
/// list po kilka kanałów. Kiedy animator dostanie narzędzia (modding, M12), ten katalog
/// staje się ładowarką pliku — reszta ścieżki nie drgnie, bo [`PoseAtlas::bake`] bierze
/// bibliotekę, a nie stałą.
#[derive(Clone, Debug)]
pub struct ClipLibrary {
    clips: Vec<AnimationClip>,
}

impl Default for ClipLibrary {
    fn default() -> Self {
        ClipLibrary::builtin()
    }
}

impl ClipLibrary {
    /// Wbudowany zestaw klipów.
    #[must_use]
    pub fn builtin() -> ClipLibrary {
        ClipLibrary {
            clips: clips::builtin_clips(),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.clips.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.clips.is_empty()
    }

    #[must_use]
    pub fn get(&self, id: ClipId) -> Option<&AnimationClip> {
        self.clips.get(id.0 as usize)
    }

    /// Identyfikator klipu dla rodzaju czynności.
    ///
    /// Rodzaj bez klipu dostaje zero (`Idle`) zamiast paniki — biblioteka jest danymi
    /// i brakujący klip ma znaczyć „postać stoi", a nie „gra się nie uruchamia".
    /// Że wbudowany zestaw pokrywa wszystkie rodzaje, pilnuje test.
    #[must_use]
    pub fn id_of(&self, kind: ClipKind) -> ClipId {
        self.clips
            .iter()
            .position(|c| c.kind == kind)
            .map_or(ClipId(0), |i| ClipId(i as u8))
    }

    pub fn iter(&self) -> impl Iterator<Item = (ClipId, &AnimationClip)> {
        self.clips
            .iter()
            .enumerate()
            .map(|(i, c)| (ClipId(i as u8), c))
    }
}

/// Pojedyncza poza jednej części — 16 B, tak jak liczy rachunek z §5.4.
///
/// Obrót jest **globalny w przestrzeni modelu** (złożony po hierarchii), a translacja
/// skorygowana o pozę spoczynkową, więc shader liczy `p' = R·p + t` i nie iteruje
/// po rodzicach.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct PoseTexel {
    /// Quaternion `(x, y, z, w)` w ustalonym punkcie: wartość × 32767.
    pub rot: [i16; 4],
    /// Translacja w jednostkach [`POSE_TRANS_SCALE`] ćwiartki voxela.
    pub trans: [i16; 3],
    pub _pad: i16,
}

/// Gdzie w atlasie leży klip danego modelu.
///
/// `frames == 0` znaczy „ten model nie zna tego klipu" i jest **poprawnym stanem**:
/// auto nie chodzi, postać nie kręci kołami. Shader rysuje wtedy pozę spoczynkową,
/// czyli dokładnie to, co rysował przed M11b.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct ClipEntry {
    pub offset: u32,
    /// `frames | parts << 8` — obie liczby mieszczą się w bajcie (`Part` jest `u8`).
    pub frames_parts: u32,
}

impl ClipEntry {
    #[must_use]
    pub const fn frames(&self) -> u8 {
        (self.frames_parts & 0xFF) as u8
    }

    #[must_use]
    pub const fn parts(&self) -> u8 {
        ((self.frames_parts >> 8) & 0xFF) as u8
    }
}

/// Statyczny atlas póz — wypalany raz przy starcie, nigdy nie zmieniany.
///
/// Indeks jest płaski: `model * MAX_CLIPS + clip` wskazuje [`ClipEntry`], a teksele
/// klipu leżą w kolejności `klatka × część`. Ta sama arytmetyka jest w `instance.wgsl`
/// i pilnuje jej test w `engine/render`.
#[derive(Clone, Debug, Default)]
pub struct PoseAtlas {
    texels: Vec<PoseTexel>,
    index: Vec<ClipEntry>,
    models: usize,
}

impl PoseAtlas {
    /// Wypala pozy wszystkich par (model, klip), które mają cokolwiek wspólnego.
    ///
    /// Para bez wspólnej części dostaje wpis pusty i nie zajmuje ani jednego teksela —
    /// stąd bierze się to, że dwadzieścia kilka klipów postaci i pięć klipów pojazdu
    /// nie mnożą się przez siebie.
    #[must_use]
    pub fn bake(models: &ModelLibrary, clips: &ClipLibrary) -> PoseAtlas {
        let mut atlas = PoseAtlas {
            texels: Vec::new(),
            index: vec![ClipEntry::default(); models.len().max(1) * MAX_CLIPS],
            models: models.len().max(1),
        };
        for (id, model) in models.iter() {
            for (cid, clip) in clips.iter() {
                if cid.0 as usize >= MAX_CLIPS || clip.frames == 0 {
                    continue;
                }
                if !clip.channels.iter().any(|c| ma_czesc(model, c.part)) {
                    continue;
                }
                let offset = atlas.texels.len() as u32;
                let parts = model.parts.len();
                bake_clip(model, clip, &mut atlas.texels);
                atlas.index[id.0 as usize * MAX_CLIPS + cid.0 as usize] = ClipEntry {
                    offset,
                    frames_parts: u32::from(clip.frames) | ((parts as u32) << 8),
                };
            }
        }
        atlas
    }

    #[must_use]
    pub fn texels(&self) -> &[PoseTexel] {
        &self.texels
    }

    #[must_use]
    pub fn index(&self) -> &[ClipEntry] {
        &self.index
    }

    #[must_use]
    pub fn models(&self) -> usize {
        self.models
    }

    /// Atlas w jednym buforze, w układzie, którego oczekuje shader: najpierw tablica
    /// wejść (`model × MAX_CLIPS`), zaraz za nią teksele póz.
    ///
    /// Jeden bufor zamiast dwóch, bo `downlevel_defaults` `wgpu` daje **cztery** bufory
    /// storage na etap shadera, a render potrzebuje ich pięciu: dwa na paletę, dwa tutaj
    /// i jeden na atlas sylwetek. Podniesienie limitu odcięłoby starsze karty — a to jest
    /// jedna tablica rozbita na nagłówek i dane, więc scalenie niczego nie miesza.
    /// `ClipEntry::offset` jest po tej stronie **absolutnym indeksem w buforze**, więc
    /// shader nie musi wiedzieć, jak długi jest nagłówek.
    #[must_use]
    pub fn to_gpu(&self) -> Vec<[u32; 4]> {
        let naglowek = self.index.len();
        let mut out: Vec<[u32; 4]> = Vec::with_capacity(naglowek + self.texels.len());
        for e in &self.index {
            let offset = if e.frames() == 0 {
                0
            } else {
                e.offset + naglowek as u32
            };
            out.push([offset, e.frames_parts, 0, 0]);
        }
        for t in &self.texels {
            out.push([
                spakuj(t.rot[0], t.rot[1]),
                spakuj(t.rot[2], t.rot[3]),
                spakuj(t.trans[0], t.trans[1]),
                spakuj(t.trans[2], 0),
            ]);
        }
        if out.is_empty() {
            out.push([0; 4]);
        }
        out
    }

    /// Rozmiar atlasu w bajtach — razem z tablicą wejść. Kryterium WP3: ≤ 512 KB.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.texels.len() * core::mem::size_of::<PoseTexel>()
            + self.index.len() * core::mem::size_of::<ClipEntry>()
    }

    #[must_use]
    pub fn entry(&self, model: ModelId, clip: ClipId) -> ClipEntry {
        // Klip spoza katalogu (`NO_CLIP`) **musi** odpaść tutaj, a nie na granicy wektora:
        // indeks `model * MAX_CLIPS + 255` mieści się w tablicy i trafia w wiersz innego
        // modelu, czyli zwróciłby cudzy klip zamiast pustego wpisu. Shader ma to samo
        // porównanie (`instance.wgsl`) i z tego samego powodu.
        if clip.0 as usize >= MAX_CLIPS {
            return ClipEntry::default();
        }
        self.index
            .get(model.0 as usize * MAX_CLIPS + clip.0 as usize)
            .copied()
            .unwrap_or_default()
    }

    /// Poza jednej części — ta sama liczba, którą przeczyta shader.
    #[must_use]
    pub fn pose(&self, model: ModelId, clip: ClipId, frame: u8, part: u8) -> Option<PoseTexel> {
        let e = self.entry(model, clip);
        if e.frames() == 0 || part >= e.parts() {
            return None;
        }
        let f = u32::from(frame) & (u32::from(e.frames()) - 1);
        let i = e.offset + f * u32::from(e.parts()) + u32::from(part);
        self.texels.get(i as usize).copied()
    }
}

fn ma_czesc(model: &VoxModel, part: PartName) -> bool {
    model.parts.iter().any(|p| p.name == part)
}

/// Wypala jeden klip dla jednego modelu: `frames × parts` tekseli.
fn bake_clip(model: &VoxModel, clip: &AnimationClip, out: &mut Vec<PoseTexel>) {
    let rest = rest_offsets(model);
    let mut rot = vec![Quat::IDENTITY; model.parts.len()];
    let mut trans = vec![Vec3::ZERO; model.parts.len()];
    for frame in 0..clip.frames {
        let t = f32::from(frame) / f32::from(clip.frames);
        for (i, p) in model.parts.iter().enumerate() {
            let kanal = clip
                .channel(p.name)
                .copied()
                .unwrap_or_else(|| Channel::new(p.name, Motion::Hold));
            let lokalny = kanal.rotation(t);
            let pivot = Vec3::new(
                f32::from(p.pivot[0]) + f32::from(kanal.offset_qv[0]),
                f32::from(p.pivot[1]) + f32::from(kanal.offset_qv[1]),
                f32::from(p.pivot[2]) + f32::from(kanal.offset_qv[2]),
            );
            // Hierarchia jest uporządkowana od korzenia (`VoxModel::validate`), więc
            // rodzic ma policzoną pozę, zanim dojdziemy do dziecka — jeden przebieg
            // w przód, bez stosu i bez wykrywania cykli.
            let (rp, tp) = if p.parent == crate::model::Part::NO_PARENT {
                (Quat::IDENTITY, Vec3::ZERO)
            } else {
                (rot[p.parent as usize], trans[p.parent as usize])
            };
            rot[i] = rp * lokalny;
            trans[i] = tp + rp * pivot;
            let r = rot[i].normalize();
            let spoczynkowa = Vec3::new(
                f32::from(rest[i][0]),
                f32::from(rest[i][1]),
                f32::from(rest[i][2]),
            );
            // `t = t_global − R·rest`, żeby shader liczył `p' = R·p + t` na wierzchołku,
            // który już niesie pozę spoczynkową (`F-2`).
            let korekta = trans[i] - r * spoczynkowa;
            out.push(PoseTexel {
                rot: [
                    kwantyzuj(r.x),
                    kwantyzuj(r.y),
                    kwantyzuj(r.z),
                    kwantyzuj(r.w),
                ],
                trans: [
                    kwantyzuj_trans(korekta.x),
                    kwantyzuj_trans(korekta.y),
                    kwantyzuj_trans(korekta.z),
                ],
                _pad: 0,
            });
        }
    }
}

/// Dwa `i16` w jednym słowie — dokładnie tak, jak leżą w [`PoseTexel`].
fn spakuj(lo: i16, hi: i16) -> u32 {
    u32::from(lo as u16) | (u32::from(hi as u16) << 16)
}

fn kwantyzuj(v: f32) -> i16 {
    (v.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

fn kwantyzuj_trans(v_qv: f32) -> i16 {
    (v_qv * POSE_TRANS_SCALE)
        .round()
        .clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::tests_support;

    fn biblioteka() -> (ModelLibrary, ClipLibrary) {
        let modele = ModelLibrary::from_models(vec![tests_support::dwie_czesci()]).expect("katalog");
        (modele, ClipLibrary::builtin())
    }

    /// Każdy rodzaj czynności ma klip — inaczej `id_of` po cichu podmieniłby animację
    /// na `Idle` i nikt by się nie dowiedział, że kasjer stoi bez ruchu.
    #[test]
    fn kazdy_rodzaj_klipu_jest_w_katalogu() {
        let lib = ClipLibrary::builtin();
        let mut rodzaje: Vec<ClipKind> = vec![
            ClipKind::Idle,
            ClipKind::Walk,
            ClipKind::WalkCarry,
            ClipKind::Run,
            ClipKind::Sit,
            ClipKind::Stand,
            ClipKind::EnterVehicle,
            ClipKind::ExitVehicle,
            ClipKind::Board,
            ClipKind::Shop,
            ClipKind::Queue,
            ClipKind::Drive,
            ClipKind::Park,
            ClipKind::Refuel,
            ClipKind::Unload,
            ClipKind::Forklift,
            ClipKind::Crane,
            ClipKind::RampLoad,
        ];
        rodzaje.extend(WorkStyle::ALL.map(ClipKind::Work));
        for k in rodzaje {
            let id = lib.id_of(k);
            assert_eq!(lib.get(id).expect("klip").kind, k, "brak klipu dla {k:?}");
        }
        assert!(lib.len() <= MAX_CLIPS, "{} klipów, sufit {MAX_CLIPS}", lib.len());
    }

    /// Faza jest brana modulo 256, a klatkę wybiera shader maską `frames - 1`.
    /// Liczba klatek spoza potęg dwójki przeskakiwałaby przy zawinięciu licznika.
    #[test]
    fn liczba_klatek_jest_potega_dwojki() {
        for (id, c) in ClipLibrary::builtin().iter() {
            assert!(
                c.frames.is_power_of_two(),
                "klip {id:?} ma {} klatek",
                c.frames
            );
            assert!((8..=32).contains(&c.frames), "klip {id:?}: {} klatek", c.frames);
        }
    }

    /// Kanał bez ruchu i bez pochylenia daje pozę spoczynkową co do bitu: obrót
    /// jednostkowy i zerową translację. To jest warunek, przy którym animacja nie
    /// psuje modelu, którego nie dotyczy.
    #[test]
    fn poza_spoczynkowa_nie_rusza_wierzcholka() {
        let model = tests_support::dwie_czesci();
        let klip = AnimationClip::new(
            ClipKind::Idle,
            8,
            vec![Channel::new(model.parts[0].name, Motion::Hold)],
        );
        let modele = ModelLibrary::from_models(vec![model]).expect("katalog");
        let lib = ClipLibrary {
            clips: vec![klip],
        };
        let atlas = PoseAtlas::bake(&modele, &lib);
        let p = atlas.pose(ModelId(0), ClipId(0), 0, 0).expect("poza");
        assert_eq!(p.rot, [0, 0, 0, 32767], "obrót nie jest jednostkowy");
        assert_eq!(p.trans, [0, 0, 0], "translacja nie jest zerowa");
    }

    /// Obrót rodzica przenosi dziecko. Bez składania hierarchii goleń zostałaby
    /// w miejscu, a udo obróciłoby się samo — i to jest dokładnie ta klasa błędu,
    /// której statyczny zrzut ekranu nie pokazuje.
    #[test]
    fn obrot_rodzica_przenosi_dziecko() {
        let model = tests_support::dwie_czesci();
        let rodzic = model.parts[0].name;
        let klip = AnimationClip::new(
            ClipKind::Idle,
            8,
            vec![Channel::new(rodzic, Motion::Hold).tilt([0, 90, 0])],
        );
        assert_eq!(model.parts[1].parent, 0, "test zakłada hierarchię");
        let rest = crate::model_mesh::rest_offsets(&model);
        let modele = ModelLibrary::from_models(vec![model]).expect("katalog");
        let atlas = PoseAtlas::bake(&modele, &ClipLibrary { clips: vec![klip] });
        let p = atlas.pose(ModelId(0), ClipId(0), 0, 1).expect("poza dziecka");
        // Dziecko dziedziczy obrót rodzica, więc jego quaternion nie jest jednostkowy.
        assert_ne!(p.rot, [0, 0, 0, 32767], "dziecko nie dostało obrotu rodzica");
        // I przesuwa się: jego pozycja spoczynkowa leży nad pivotem rodzica.
        assert_ne!(rest[1], rest[0], "test zakłada przesunięcie dziecka");
        assert_ne!(p.trans, [0, 0, 0], "dziecko zostało w miejscu");
    }

    /// Model, którego klip nie dotyczy, nie zajmuje w atlasie ani jednego teksela —
    /// inaczej dwadzieścia klipów postaci mnożyłoby się przez katalog pojazdów.
    #[test]
    fn klip_bez_wspolnej_czesci_nie_zajmuje_miejsca() {
        let (modele, klipy) = biblioteka();
        let atlas = PoseAtlas::bake(&modele, &klipy);
        let jazda = klipy.id_of(ClipKind::Drive);
        assert_eq!(
            atlas.entry(ModelId(0), jazda).frames(),
            0,
            "postać dostała klip jazdy"
        );
    }

    /// Kryterium WP3: atlas mieści się w 512 KB. Liczy się prawdziwy katalog modeli,
    /// bo to on rośnie — klipów jest dwadzieścia kilka i nie przybędzie ich setka.
    #[test]
    fn atlas_miesci_sie_w_budzecie() {
        let dir = magnat_core::assets::data_path("models");
        let modele = match ModelLibrary::load_dir(&dir) {
            Ok(m) => m,
            // Katalog danych bywa nieosiągalny z katalogu roboczego testu; wtedy
            // budżetu pilnuje test w `engine/render`, który ładuje te same pliki.
            Err(_) => return,
        };
        let atlas = PoseAtlas::bake(&modele, &ClipLibrary::builtin());
        assert!(
            atlas.bytes() <= 512 * 1024,
            "atlas póz waży {} B, budżet 512 KB",
            atlas.bytes()
        );
        assert!(atlas.bytes() > 0, "atlas jest pusty — nic się nie wypaliło");
    }

    /// Klucz roli spoza katalogu nie ma prawa wywrócić doboru animacji.
    #[test]
    fn nieznana_rola_dostaje_biuro() {
        assert_eq!(WorkStyle::from_role_key("cashier"), WorkStyle::Counter);
        assert_eq!(WorkStyle::from_role_key("truck_driver"), WorkStyle::Driving);
        assert_eq!(WorkStyle::from_role_key("stocker"), WorkStyle::Warehouse);
        assert_eq!(WorkStyle::from_role_key("baker"), WorkStyle::Line);
        assert_eq!(WorkStyle::from_role_key("welder"), WorkStyle::Construction);
        assert_eq!(WorkStyle::from_role_key("astronauta"), WorkStyle::Office);
    }

    /// Faza rozsuwa się indeksem encji i przesuwa się w czasie. Bez pierwszego
    /// cały tłum maszerowałby w takt, bez drugiego stałby w miejscu.
    #[test]
    fn faza_plynie_i_rozsuwa_tlum() {
        assert_eq!(anim_phase(0, 0), 0);
        // Jedna klatka klipu to 1/12 sekundy.
        assert_eq!(anim_phase(1000 / ANIM_FPS, 0), 1);
        assert_ne!(anim_phase(0, 0), anim_phase(0, 1));
        // Modulo 256 zawija się bez paniki na dowolnie długiej sesji.
        assert_eq!(anim_phase(u64::MAX / 2, u32::MAX) as u32 % 256, {
            let k = (u64::MAX / 2) / (1000 / ANIM_FPS);
            (k.wrapping_add(u64::from(u32::MAX)) % 256) as u32
        });
    }

    /// Czynność bez ramienia w `clip_for` ma łamać kompilację, a nie rysować postać
    /// bez ruchu. Test jest tu po to, żeby odwzorowanie miało jednego czytelnika
    /// także wtedy, gdy wypełniacz snapshotu jeszcze go nie woła.
    #[test]
    fn czynnosc_odwzorowuje_sie_na_klip() {
        use magnat_core::ActivityKind as A;
        assert_eq!(
            clip_for(A::Work, WorkStyle::Counter),
            ClipKind::Work(WorkStyle::Counter)
        );
        assert_eq!(clip_for(A::Commute, WorkStyle::Office), ClipKind::Walk);
        assert_eq!(clip_for(A::Shop, WorkStyle::Office), ClipKind::Shop);
        assert_eq!(clip_for(A::Eat, WorkStyle::Office), ClipKind::Sit);
        for a in A::ALL {
            // Każda czynność ma dostać klip, który jest w katalogu.
            let kind = clip_for(*a, WorkStyle::Office);
            let lib = ClipLibrary::builtin();
            assert_eq!(lib.get(lib.id_of(kind)).expect("klip").kind, kind);
        }
    }
}

#[cfg(test)]
mod testy_bufora {
    use super::*;

    /// Bufor dla shadera i czytnik dla testów muszą pokazywać **tę samą** pozę.
    /// `to_gpu` przesuwa offsety o długość nagłówka i pakuje `i16` po dwa w słowie;
    /// pomyłka w którymkolwiek z tych dwóch kroków daje postać złożoną z części
    /// cudzej klatki — czyli objaw, którego nie widać w żadnej liczbie.
    #[test]
    fn bufor_dla_shadera_zgadza_sie_z_czytnikiem() {
        let modele = ModelLibrary::from_models(vec![crate::model::tests_support::dwie_czesci()])
            .expect("katalog");
        let klipy = ClipLibrary::builtin();
        let atlas = PoseAtlas::bake(&modele, &klipy);
        let bufor = atlas.to_gpu();
        let naglowek = atlas.index().len();
        assert_eq!(bufor.len(), naglowek + atlas.texels().len());

        let mut sprawdzone = 0usize;
        for (cid, _) in klipy.iter() {
            let e = atlas.entry(ModelId(0), cid);
            if e.frames() == 0 {
                continue;
            }
            let wpis = bufor[cid.0 as usize];
            assert_eq!(wpis[1], e.frames_parts, "nagłówek klipu {cid:?}");
            for frame in 0..e.frames() {
                for part in 0..e.parts() {
                    let p = atlas.pose(ModelId(0), cid, frame, part).expect("poza");
                    let i = wpis[0] as usize
                        + usize::from(frame) * usize::from(e.parts())
                        + usize::from(part);
                    let w = bufor[i];
                    assert_eq!(w[0] & 0xFFFF, u32::from(p.rot[0] as u16), "rot.x");
                    assert_eq!(w[0] >> 16, u32::from(p.rot[1] as u16), "rot.y");
                    assert_eq!(w[1] & 0xFFFF, u32::from(p.rot[2] as u16), "rot.z");
                    assert_eq!(w[1] >> 16, u32::from(p.rot[3] as u16), "rot.w");
                    assert_eq!(w[2] & 0xFFFF, u32::from(p.trans[0] as u16), "trans.x");
                    assert_eq!(w[2] >> 16, u32::from(p.trans[1] as u16), "trans.y");
                    assert_eq!(w[3] & 0xFFFF, u32::from(p.trans[2] as u16), "trans.z");
                    sprawdzone += 1;
                }
            }
        }
        assert!(sprawdzone > 0, "żaden klip nie trafił w ten model");
    }

    /// Klip, którego model nie zna, musi mieć w buforze **zerowe klatki** — shader
    /// sprawdza tę jedną liczbę, zanim sięgnie po cokolwiek innego.
    #[test]
    fn brak_klipu_jest_zerem_w_naglowku() {
        let modele = ModelLibrary::from_models(vec![crate::model::tests_support::dwie_czesci()])
            .expect("katalog");
        let klipy = ClipLibrary::builtin();
        let atlas = PoseAtlas::bake(&modele, &klipy);
        let bufor = atlas.to_gpu();
        let jazda = klipy.id_of(ClipKind::Drive);
        assert_eq!(bufor[jazda.0 as usize][1] & 0xFF, 0);
        assert_eq!(bufor[jazda.0 as usize][0], 0);
    }
}
