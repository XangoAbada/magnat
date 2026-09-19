//! Wypełnianie snapshotu renderu (M11a §5.2, WP2).
//!
//! ### Dlaczego to mieszka tutaj, a nie w `sim-snapshot`
//!
//! §6.1 dokumentu fazy stawiało `fill_render_snapshot(&World, &ViewQuery, &mut RenderSnapshot)`
//! w crate'cie `sim-snapshot`. Tam się nie da i nie chodzi o wygodę: §6.3 pkt 2 wymaga, żeby
//! ten crate zależał **wyłącznie** od `engine-core`, a rekordy trzeba złożyć z danych
//! trzech różnych crate'ów — pozycje z `sim/traffic`, zawód i gospodarstwo z `sim/agents`,
//! rodzaj dzielnicy z `sim/world`. Jedna funkcja nad `&World` musiałaby zależeć od
//! wszystkich i wciągnąć je do drzewa zależności renderu, czyli unieważnić to, czego
//! `sim-snapshot` pilnuje.
//!
//! `magnat-game` jest jedynym miejscem, które widzi je wszystkie naraz — i jest tym samym
//! miejscem, w którym `K-68` postawił stawianie świata. Kontrakt zostaje bez zmian:
//! **wejściem jest `&World`, nie `&mut World`**, więc kompilator gwarantuje, że publikacja
//! snapshotu nie zmienia stanu symulacji (§7.1, `snapshot_fill_is_pure`).
//!
//! ### Co tu jest, a czego nie ma
//!
//! Tutaj są **encje**: mieszkańcy, pojazdy, zakłady i postać gracza. Pogoda, zasilanie
//! dzielnic i lista świateł mieszkają obok, w [`crate::ambience`] — to jest inny temat
//! i inny zestaw źródeł (stan `sim/events`, sieci `sim/traffic`, geometria latarni
//! z `CityData`), a nie złączenie warstwy Mikro z komponentami.
//!
//! `crowd` zostaje wyzerowane i to jest jedyna sekcja bez wypełniacza. Warstwy L3
//! (tłum agregatowy) nie ma — `W-2` zdjęło ją z fazy, bo impostor z czterystu metrów
//! zajmuje te 4×6 px sam z siebie.

use crate::ambience::Ambience;
use magnat_agents::components::{AgentState, Employment, Identity, Residence};
use magnat_agents::household::Household;
use magnat_agents::social::StatusDistribution;
use magnat_agents::{demography, AgentSources};
use magnat_core::{CitizenId, Entity, Money, SiteId};
use magnat_ecs::World;
use magnat_sim_snapshot::{
    dist2_mm, select_top_k, AgeBand, Appearance, Candidate, CitizenRenderRec, OutfitTier,
    PedestrianRecord, RenderSnapshot, SiteRenderRec, VehicleRecord, VehicleRenderRec, ViewQuery,
    CITIZEN_FLAG_PLAYER_OWNED, MAX_DISTRICTS,
};
use magnat_traffic::{TrafficServices, VehicleOwner};
use magnat_voxel::{anim, ClipKind, ClipLibrary, PaletteLibrary, WorkStyle};
use magnat_world::CityData;

/// Wysokość oka nad gruntem w metrach. Ta sama liczba, którą klient podaje kamerze
/// w trybie pierwszoosobowym — jedno miejsce, bo dwa rozjechałyby się przy pierwszej
/// zmianie wzrostu postaci.
pub const EYE_HEIGHT_M: f32 = 1.65;

/// Bufory robocze wypełniacza.
///
/// Trzymane między publikacjami, bo publikacja zdarza się 30 razy na sekundę i ma
/// **nie alokować** — to jest ta sama zasada, dla której snapshot ma stałą pojemność.
pub struct SnapshotFiller {
    peds: Vec<PedestrianRecord>,
    vehs: Vec<VehicleRecord>,
    cands: Vec<Candidate>,
    /// `DistrictPaletteId` per `DistrictId`, wyliczona raz przy związaniu z miastem.
    palette_by_district: [u16; MAX_DISTRICTS],
    /// Ziarno miasta, dla którego tablica wyżej została policzona. Nowa gra ma inne
    /// ziarno, więc związanie odnawia się samo — bez flagi, którą ktoś musiałby zerować.
    bound_seed: Option<u64>,
    /// Katalog klipów — ten sam, który renderer wypala do atlasu póz.
    clips: ClipLibrary,
    /// `WorkStyle` per `JobRoleId`, wyliczony raz z katalogu ról.
    ///
    /// `None` znaczy „jeszcze nie próbowano"; pusty wektor — „scenariusz nie ma rynku
    /// pracy", bo `m5shop` i `m3day` stawiają wycinek świata bez katalogu ról. Wtedy
    /// każdy pracujący dostaje [`WorkStyle::Office`] i to jest poprawna odpowiedź:
    /// animacja ma wyglądać neutralnie, a nie zgadywać zawód z numeru.
    style_by_role: Option<Vec<WorkStyle>>,
    /// Czy encje dostają klip, czy pozę spoczynkową. Wyłącza to `--no-anim` i służy
    /// wyłącznie do pomiaru z kryterium WP3 — w grze zawsze jest włączone.
    animations: bool,
    /// Zakłady miasta: `(SiteId, pozycja w mm, dzielnica)`, wyliczone raz przy związaniu.
    ///
    /// Pozycja jest w `CityData`, a nie w ECS, więc `fill` po samym `&World` by jej nie
    /// dosięgnął. Tablica jest sortowana po `SiteId`, bo tak samo indeksuje ją `Plant`.
    /// Dzielnica dołączyła w M11d — do tej pory docstring ją obiecywał, a krotka miała
    /// dwa pola (`I-4`); bez niej okno nie wie, czy jego dzielnica ma prąd.
    ///
    /// **Klucz jest przesunięty** (`SITE_KEY_BASE + indeks`, `K-46`), bo przechodzi
    /// granicę crate'u: pod nim stoi zakład w `magnat_supply::Plant`, pod nim jedzie
    /// `entity_lo` do bufora identyfikatorów i pod nim `Subject::Site` otwiera kartę.
    /// Do M11d stała tu numeracja generatora i `plant.get` **nigdy nie trafiał** —
    /// każdy zakład dostawał aktywność 128 i zapas 255, czyli wartości domyślne
    /// nie do odróżnienia od prawdziwych (`I-5`).
    sites: Vec<(SiteId, [i32; 3], u16)>,
    /// Postać gracza, jeśli już jest. `PlayerCharacter` mieszka w sesji, a nie w świecie,
    /// więc wypełniacz dostaje ją setterem — tak samo jak `animations`.
    player: Option<CitizenId>,
    /// Kafel atlasu szyldów per `SiteId`, nadany przez klienta (WP9). Pusty wektor
    /// znaczy „nikt jeszcze nie rozdał numerów", a nie „żadna firma nie ma szyldu".
    signs: Vec<(SiteId, u16)>,
    /// Pogoda, zasilanie i światła — patrz [`crate::ambience`].
    ambience: Ambience,
}

impl Default for SnapshotFiller {
    fn default() -> Self {
        SnapshotFiller {
            peds: Vec::new(),
            vehs: Vec::new(),
            cands: Vec::new(),
            palette_by_district: [0; MAX_DISTRICTS],
            bound_seed: None,
            clips: ClipLibrary::builtin(),
            style_by_role: None,
            animations: true,
            sites: Vec::new(),
            player: None,
            signs: Vec::new(),
            ambience: Ambience::default(),
        }
    }
}

impl SnapshotFiller {
    #[must_use]
    pub fn new() -> SnapshotFiller {
        SnapshotFiller::default()
    }

    /// Wylicza tablicę „dzielnica → paleta" dla postawionego miasta.
    ///
    /// Raz, a nie co klatkę: rodzaj dzielnicy i epoka założenia miasta nie zmieniają się
    /// w trakcie gry, a odpytywanie biblioteki palet po kluczach tekstowych dla każdej
    /// z dwudziestu czterech tysięcy encji byłoby porównywaniem łańcuchów w pętli klatki.
    ///
    /// Dzielnica, dla której paleta nie istnieje, dostaje zero i **jest to widoczne**:
    /// zerowa paleta to paleta pierwszej dzielnicy w pierwszej epoce, więc rozjazd kluczy
    /// między `data/palettes/` a `DistrictKind` objawia się jednolitą tonacją całego
    /// miasta, a nie pustym ekranem. Pokrycia pilnuje test w `magnat_voxel::palette`.
    pub fn bind_city(&mut self, city: &CityData, palettes: &PaletteLibrary) {
        self.bound_seed = Some(city.plan.seed);
        let epoka = city.plan.epoch.key();
        self.palette_by_district = [0; MAX_DISTRICTS];
        for (i, d) in city.districts.districts.iter().enumerate() {
            if i >= MAX_DISTRICTS {
                break;
            }
            self.palette_by_district[i] = palettes.id_of(d.kind.key(), epoka).map_or(0, |p| p.0);
        }
        // Pozycja zakładu to **wejście budynku**, a nie środek bryły: szyld wisi nad
        // drzwiami, a nie nad dachem, i tam samo gracz podchodzi. Wysokość to nadproże
        // parteru, czyli tyle, ile trzeba, żeby napis był nad głowami i pod oknem piętra.
        self.sites.clear();
        for (i, s) in city.sites.sites.iter().enumerate() {
            let b = &city.buildings.buildings[s.building.0.index() as usize];
            let z = b.aabb.min.z
                + (f32::from(b.floor_heights_dm.first().copied().unwrap_or(30)) * 0.1 - 0.7)
                    .max(2.5);
            let p = b.entrances.first().map_or(
                [
                    (b.aabb.min.x + b.aabb.max.x) * 0.5,
                    (b.aabb.min.y + b.aabb.max.y) * 0.5,
                    z,
                ],
                |e| [e.pos.x, e.pos.y, z],
            );
            let dzielnica = city
                .parcels
                .parcels
                .get(s.parcel.0.index() as usize)
                .map_or(0, |p| p.district.0);
            self.sites
                .push((crate::world::plants::site_id(i), to_mm(p), dzielnica));
        }
        self.sites.sort_unstable_by_key(|(s, _, _)| *s);
        self.ambience.bind(city);
    }

    /// Wiąże z miastem, jeśli to inne miasto niż poprzednio. Wołane z pętli klatki:
    /// porównanie ziarna kosztuje jedno `u64`, a przeliczenie tablicy zdarza się raz
    /// na wczytany świat.
    pub fn ensure_city(&mut self, city: &CityData, palettes: &PaletteLibrary) {
        if self.bound_seed != Some(city.plan.seed) {
            self.bind_city(city, palettes);
        }
    }

    /// Włącza albo wyłącza klipy animacji (scena pomiarowa `--no-anim`).
    pub fn set_animations(&mut self, on: bool) {
        self.animations = on;
    }

    /// Wskazuje postać gracza — kotwicę trybu pierwszoosobowego (M11c §5.7).
    ///
    /// Setter, a nie argument `fill`, bo `PlayerCharacter` mieszka w [`crate::Session`],
    /// a nie w świecie ECS: gracz jest bytem sesji, a `fill` widzi wyłącznie świat.
    pub fn set_player(&mut self, citizen: Option<CitizenId>) {
        self.player = citizen;
    }

    /// Rozdaje kafle atlasu szyldów (WP9). Wejściem jest lista `(zakład, kafel)`,
    /// posortowana po zakładzie; `0` znaczy „bez szyldu".
    pub fn set_signs(&mut self, signs: Vec<(SiteId, u16)>) {
        self.signs = signs;
    }

    /// Odwzorowanie „rola zawodowa → styl animacji pracy", liczone raz na sesję.
    ///
    /// Klucz roli jest łańcuchem, a mieszkańców w kadrze jest do 24 576 — porównywanie
    /// łańcuchów w pętli klatki byłoby dokładnie tym kosztem, którego cała ścieżka
    /// instancingu unika.
    fn ensure_styles(&mut self, world: &World) {
        // Pustej tablicy **nie zapamiętujemy**: scenariusz bez rynku pracy wygląda
        // wtedy tak samo jak świat, w którym katalog ról jeszcze się nie załadował,
        // a pierwszy przypadek kosztuje jedno odpytanie zasobu na klatkę, drugi —
        // wszystkich pracujących na zawsze w jednej animacji.
        if self.style_by_role.as_ref().is_some_and(|t| !t.is_empty()) {
            return;
        }
        let tabela = world
            .get_resource::<magnat_economy::labor::LaborHandle>()
            .and_then(magnat_economy::labor::LaborHandle::get)
            .map(|m| {
                let r = m.roles();
                (0..r.len())
                    .map(|i| WorkStyle::from_role_key(r.key(magnat_core::JobRoleId(i as u16))))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        self.style_by_role = Some(tabela);
    }

    /// Wypełnia snapshot encjami widocznymi w kadrze.
    ///
    /// Krok po kroku: warstwa Mikro oddaje pozycje, `aabb` odsiewa to, co poza kadrem,
    /// `select_top_k` zostawia najbliższe encje do wysokości capu (remis po indeksie
    /// encji, więc wynik jest deterministyczny), a złączenie z komponentami dokłada
    /// wygląd. Skan jest **lokalny**: warstwa Mikro ma własne okno wokół kamery i nie
    /// zwraca całego miasta.
    pub fn fill(&mut self, world: &World, view: &ViewQuery, out: &mut RenderSnapshot) {
        self.fill_filtered(world, view, &|_| true, out);
    }

    /// To samo z filtrem encji (§14.2): mieszkaniec, dla którego predykat zwróci `false`,
    /// **nie trafia do snapshotu**.
    ///
    /// Filtr jest preferencją widoku i dlatego stoi tutaj, a nie w warstwie Mikro:
    /// warstwa jest wspólna dla renderu i dla śledzenia, więc ukrycie kogoś na ekranie
    /// nie ma prawa zmienić tego, kogo symulacja liczy. Filtr **nie dotyczy pojazdów** —
    /// „moi klienci" i „moi pracownicy" są pytaniami o ludzi.
    pub fn fill_filtered(
        &mut self,
        world: &World,
        view: &ViewQuery,
        keep: &dyn Fn(u32) -> bool,
        out: &mut RenderSnapshot,
    ) {
        out.clear();
        out.tick = world.tick;
        out.sim_minute = magnat_core::SimMinute(world.tick.get());
        out.district_palette = self.palette_by_district;

        let doba = (world.tick.get() / 1440) as i32;
        let seed = world.seed;

        // Atmosfera **przed** encjami: jasność okna zakładu zależy od tego, czy
        // dzielnica ma prąd, a lista świateł — od tego, które zakłady i pojazdy
        // w ogóle weszły do kadru. Stąd stan na początku, światła na końcu.
        self.ambience.fill_state(world, view, out);
        self.fill_citizens(world, view, doba, seed, keep, out);
        self.fill_vehicles(world, view, out);
        self.fill_sites(world, view, out);
        self.fill_player(world, out);
        self.ambience.fill_lights(view, out);
    }

    /// Atmosfera kadru — pogoda, zasilanie, światła. Klient ustawia przez nią
    /// światło dzienne i sprzężenie zwrotne zajętości klastrów.
    pub fn ambience_mut(&mut self) -> &mut Ambience {
        &mut self.ambience
    }

    /// Zakłady widoczne w kadrze (M11c §5.7, WP5 i WP9).
    ///
    /// `activity`, `stock_fill`, emisja i awaria pochodzą z `magnat_supply`, więc
    /// **zakład handlowy ich nie ma** — sklep nie jest linią produkcyjną. Dostaje wtedy
    /// zapas pełny i aktywność neutralną: regały w sklepie mają stać pełne, dopóki
    /// stan półki nie dojdzie własną drogą (pozycja w `R3`).
    ///
    /// M11d dokłada trzy pola, których M11c nie ruszało: `emission` i rodzaj pióropusza
    /// (dym z komina, §5.8), `lights` (okna po zmroku, gaszone blackoutem) oraz
    /// `ambient_kind` (łoże dźwiękowe emitera, §5.9).
    fn fill_sites(&mut self, world: &World, view: &ViewQuery, out: &mut RenderSnapshot) {
        if self.sites.is_empty() {
            return;
        }
        let chain = world.get_resource::<magnat_supply::ChainHandle>();
        self.cands.clear();
        for (i, (_, pos, _)) in self.sites.iter().enumerate() {
            if !view.aabb.contains(*pos) {
                continue;
            }
            self.cands.push(Candidate {
                dist2: dist2_mm(*pos, view.eye),
                entity: i as u32,
                src: i as u32,
            });
        }
        select_top_k(&mut self.cands, view.caps.sites as usize);
        for c in &self.cands {
            let (id, pos, dzielnica) = self.sites[c.src as usize];
            let mut rec = SiteRenderRec {
                entity_lo: id.0.index(),
                pos,
                sign_id: self
                    .signs
                    .binary_search_by_key(&id, |(s, _)| *s)
                    .map_or(0, |i| self.signs[i].1),
                activity: 128,
                stock_fill: 255,
                ambient_kind: self.ambience.bed_of_site(id),
                ..Default::default()
            };
            if let Some(ch) = chain {
                let c = ch.lock();
                if let Some(zaklad) = c.plant.get(id) {
                    rec.activity = zaklad.activity();
                    rec.stock_fill = magnat_supply::plant::stock_fill(&c.store, zaklad);
                    if zaklad.is_fault() {
                        rec.flags |= magnat_sim_snapshot::SITE_FAULT;
                    }
                    rec.emission = self.ambience.plant_emission(zaklad);
                    // Pióropusz bierze się z **pracującej** receptury, a nie z archetypu:
                    // linia przezbrojona na inny wyrób dymi inaczej, a stojąca nie dymi
                    // wcale — i to jest ta sama informacja, którą niesie `activity`.
                    if let Some(r) = zaklad.lines.iter().find_map(|l| match l.state {
                        magnat_supply::plant::LineState::Running { recipe, .. } => Some(recipe),
                        _ => None,
                    }) {
                        rec.flags |= (self.ambience.plume_of_recipe(r) & 0b11) << 1;
                    }
                }
            }
            rec.lights = self.ambience.window_brightness(dzielnica, rec.activity);
            out.sites.push(rec);
        }
    }

    /// Postać gracza: pozycja oka dla trybu pierwszoosobowego (M11c §5.7, `G-3`).
    ///
    /// Oko bierze się z **warstwy Mikro**, a nie z komponentu: pozycja mieszkańca istnieje
    /// wyłącznie wtedy, gdy jest on w kadrze albo przypięty (`player::pin_micro`), i to
    /// jest właściwe źródło — ta sama liczba, którą widzi renderer dla każdego innego
    /// pieszego. `citizen == 0` znaczy „gracz jeszcze nie wybrał postaci".
    fn fill_player(&mut self, world: &World, out: &mut RenderSnapshot) {
        let Some(gracz) = self.player else { return };
        let idx = gracz.0.index();
        out.player.citizen = idx;
        if let Some(p) = self.peds.iter().find(|p| p.entity == idx) {
            out.player.eye = to_mm([p.pos[0], p.pos[1], p.pos[2] + EYE_HEIGHT_M]);
            out.player.yaw = yaw_u16(p.heading);
        }
        let _ = world;
    }

    fn fill_citizens(
        &mut self,
        world: &World,
        view: &ViewQuery,
        doba: i32,
        seed: u64,
        keep: &dyn Fn(u32) -> bool,
        out: &mut RenderSnapshot,
    ) {
        self.peds.clear();
        if let Some(z) = world
            .get_resource::<AgentSources>()
            .and_then(AgentSources::get)
        {
            z.travel.micro_snapshot(&mut self.peds);
        }
        self.cands.clear();
        for (i, p) in self.peds.iter().enumerate() {
            let pos = to_mm(p.pos);
            if !view.aabb.contains(pos) || !keep(p.entity) {
                continue;
            }
            self.cands.push(Candidate {
                dist2: dist2_mm(pos, view.eye),
                entity: p.entity,
                src: i as u32,
            });
        }
        select_top_k(&mut self.cands, view.caps.citizens as usize);

        let status = world.get_resource::<StatusDistribution>();
        self.ensure_styles(world);
        let style = self.style_by_role.as_deref().unwrap_or(&[]);
        for c in &self.cands {
            let p = self.peds[c.src as usize];
            out.citizens.push(citizen_rec(
                world,
                &p,
                doba,
                seed,
                status,
                &self.clips,
                style,
                view.anim_ms,
                self.animations,
            ));
        }
    }

    fn fill_vehicles(&mut self, world: &World, view: &ViewQuery, out: &mut RenderSnapshot) {
        self.vehs.clear();
        if let Some(t) = world.get_resource::<TrafficServices>() {
            t.oracle.micro().vehicle_snapshot(&mut self.vehs);
        }
        self.cands.clear();
        for (i, v) in self.vehs.iter().enumerate() {
            let pos = to_mm(v.pos);
            if !view.aabb.contains(pos) {
                continue;
            }
            self.cands.push(Candidate {
                dist2: dist2_mm(pos, view.eye),
                entity: v.entity,
                src: i as u32,
            });
        }
        select_top_k(&mut self.cands, view.caps.vehicles as usize);

        let swiatla = self.ambience.headlights_on();
        for c in &self.cands {
            let v = self.vehs[c.src as usize];
            out.vehicles.push(vehicle_rec(
                world,
                &v,
                &self.clips,
                view.anim_ms,
                self.animations,
                swiatla,
            ));
        }
    }
}

/// Metry `f32` → milimetry `i32`, z nasyceniem zamiast zawijania.
fn to_mm(p: [f32; 3]) -> [i32; 3] {
    let m = |v: f32| {
        (f64::from(v) * 1000.0)
            .round()
            .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
    };
    [m(p[0]), m(p[1]), m(p[2])]
}

/// Kąt w radianach → 1/65536 obrotu.
fn yaw_u16(rad: f32) -> u16 {
    const TAU: f32 = std::f32::consts::TAU;
    let t = rad.rem_euclid(TAU) / TAU;
    (t * 65536.0) as u32 as u16
}

#[allow(clippy::too_many_arguments)]
fn citizen_rec(
    world: &World,
    p: &PedestrianRecord,
    doba: i32,
    seed: u64,
    status: Option<&StatusDistribution>,
    clips: &ClipLibrary,
    style: &[WorkStyle],
    anim_ms: u64,
    animations: bool,
) -> CitizenRenderRec {
    let e = demography::citizen_by_index(world, p.entity);
    let id = e.and_then(|e| world.get::<Identity>(e)).copied();
    let emp = e.and_then(|e| world.get::<Employment>(e)).copied();
    let res = e.and_then(|e| world.get::<Residence>(e)).copied();
    let stan = e.and_then(|e| world.get::<AgentState>(e)).copied();

    // Klasa ubrania z roli zawodowej. Katalog ról rośnie (46 po M7a), a pole ma pięć
    // bitów, więc reszta z dzielenia jest jedyną odpowiedzią, która nie wywraca renderu
    // przy dopisaniu roli — kasjer i recepcjonista i tak dzielą ubranie (ryzyko R4 fazy).
    let outfit_class = emp
        .filter(|e| e.site != Employment::NO_SITE)
        .map_or(0, |e| (e.role % 32) as u8);
    let tier = wealth_tier(world, e, status);
    let age = id.map_or(AgeBand::Adult, |i| {
        AgeBand::from_years(i.age_years(doba).clamp(0, 255) as u8)
    });
    let flags = if id.is_some_and(|i| i.flags & Identity::FLAG_PLAYER != 0) {
        CITIZEN_FLAG_PLAYER_OWNED
    } else {
        0
    };

    // Czynność agenta → klip. Odwzorowanie jest w `engine/voxel`, bo to ono wie,
    // *jak narysować* czynność; `core::ActivityKind` wie, *co to za czynność* (`K-8`).
    // Mieszkaniec bez komponentu stanu (scenariusz bez pętli doby) dostaje `Idle`.
    let styl = emp
        .filter(|e| e.site != Employment::NO_SITE)
        .and_then(|e| style.get(e.role as usize).copied())
        .unwrap_or_default();
    let klip = stan.map_or(ClipKind::Idle, |s| {
        magnat_voxel::clip_for(s.activity_kind(), styl)
    });

    CitizenRenderRec {
        pos: to_mm(p.pos),
        entity_lo: p.entity,
        appearance: Appearance::derive(seed, p.entity, outfit_class, tier as u8, age as u8, 0).0,
        // Kurs liczy warstwa ruchu z osi odcinka trasy (`F-5`); renderer dostaje go
        // gotowego, bo różnica pozycji między publikacjami zależałaby od klatki.
        yaw: yaw_u16(p.heading),
        district: res.map_or(0, |r| r.district),
        anim_state: if animations {
            clips.id_of(klip).0
        } else {
            magnat_voxel::NO_CLIP.0
        },
        // Decyzja 9.3 przyjęta wg propozycji domyślnej: fazę liczy wypełniacz, funkcją
        // czystą od `(chwila, indeks encji)`, i **nie ma jej w ECS**. Rozsunięcie po
        // indeksie jest po to, żeby tłum nie maszerował w jednym takcie.
        anim_phase: anim::anim_phase(anim_ms, p.entity),
        // ponytail: niesiony przedmiot zostaje zerem, dopóki model postaci nie ma części
        // `carried` — klip `WalkCarry` czeka gotowy w katalogu. Wchodzi razem z modelem
        // artysty albo z propami wnętrz (M11c).
        carry: 0,
        flags,
        _pad: [0; 4],
    }
}

fn vehicle_rec(
    world: &World,
    v: &VehicleRecord,
    clips: &ClipLibrary,
    anim_ms: u64,
    animations: bool,
    headlights: bool,
) -> VehicleRenderRec {
    let owner = vehicle_entity(world, v.entity).and_then(|e| world.get::<VehicleOwner>(e));
    let hh = owner
        .and_then(|o| demography::household_by_index(world, o.owner))
        .and_then(|h| world.get::<Household>(h));

    VehicleRenderRec {
        pos: to_mm(v.pos),
        entity_lo: v.entity,
        yaw: yaw_u16(v.heading),
        pitch: 0,
        // Jeden model bryły na razie — katalog modeli pojazdów wnosi M11b razem
        // z poziomami detalu. `class` z rekordu ruchu jedzie w `model` bez zmiany,
        // bo to jego docelowe znaczenie.
        model: u16::from(v.class),
        // Lakier należy do **gospodarstwa**, nie do pojazdu: ten sam numer właściciela
        // daje zawsze tę samą barwę, a wybór z zestawu robi paleta epoki (PRD §16.3).
        paint: owner.map_or(0, |o| (o.owner & 0xFFFF) as u16),
        livery: 0,
        district: hh.map_or(0, |h| h.district),
        // Pojazd w warstwie Mikro jest w podróży — warstwa trzyma tych, którzy jadą.
        // ponytail: postój, tankowanie i rozładunek mają swoje klipy w katalogu i czekają
        // na stan pojazdu w rekordzie; dziś każdy widoczny pojazd jedzie.
        anim_state: if animations {
            clips.id_of(ClipKind::Drive).0
        } else {
            magnat_voxel::NO_CLIP.0
        },
        anim_phase: anim::anim_phase(anim_ms, v.entity),
        wheel_phase: anim::anim_phase(anim_ms, v.entity.wrapping_add(7)),
        load: 0,
        // **Silnik pracuje w każdym widocznym pojeździe** i to nie jest uproszczenie:
        // warstwa Mikro trzyma wyłącznie tych, którzy jadą (komentarz przy `anim_state`
        // wyżej). Do M11d flagi były twardym zerem, więc reflektory z WP6 i dźwięk
        // silnika z WP8 nie miały na czym stanąć — pole bez pisarza wygląda w danych
        // tak samo jak pole wyzerowane z rozmysłu (`I-18`).
        flags: magnat_sim_snapshot::VEHICLE_FLAG_ENGINE_ON
            | if headlights {
                magnat_sim_snapshot::VEHICLE_FLAG_LIGHTS
            } else {
                0
            },
        occupants: 0,
        _pad: [0; 6],
    }
}

fn wealth_tier(
    world: &World,
    e: Option<Entity>,
    status: Option<&StatusDistribution>,
) -> OutfitTier {
    let (Some(e), Some(status)) = (e, status) else {
        return OutfitTier::Modest;
    };
    let Some(hh) = world
        .get::<Identity>(e)
        .and_then(|id| demography::household_by_index(world, id.household))
        .and_then(|h| world.get::<Household>(h))
    else {
        return OutfitTier::Modest;
    };
    let majatek = Money(
        hh.cash
            .get()
            .saturating_add(hh.bank.get())
            .saturating_add(hh.savings.get()),
    );
    OutfitTier::from_percentile(status.wealth_percentile(majatek))
}

/// Encja pojazdu z samego indeksu. Pojazdy nie są w spisie ludności, więc generacja
/// pochodzi wprost z magazynu encji — inaczej niż u mieszkańca, który ma `Population`.
fn vehicle_entity(world: &World, index: u32) -> Option<Entity> {
    let gen = world.entities().generation_at(index)?;
    Some(Entity::new(index, std::num::NonZeroU32::new(gen)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_sim_snapshot::{Aabb, SnapshotCaps};

    fn zapytanie() -> ViewQuery {
        ViewQuery {
            aabb: Aabb::around([0, 0, 0], 2_000_000, 2_000_000),
            eye: [0, 0, 0],
            caps: SnapshotCaps::DEFAULT,
            anim_ms: 0,
        }
    }

    /// `snapshot_fill_is_pure` z §7.1: publikacja snapshotu nie zmienia stanu symulacji.
    /// Sygnatura `&World` gwarantuje to na poziomie kompilatora, ale hash jest dowodem,
    /// że nie ma obejścia przez wnętrze komórki ani przez zasób pod muteksem.
    #[test]
    fn wypelnianie_nie_zmienia_swiata() {
        let world = magnat_ecs::World::new(7);
        let przed = magnat_io::world_state_hash(&world);
        let mut f = SnapshotFiller::new();
        let mut snap = RenderSnapshot::default();
        for _ in 0..1_000 {
            f.fill(&world, &zapytanie(), &mut snap);
        }
        assert_eq!(
            magnat_io::world_state_hash(&world),
            przed,
            "wypełnianie ruszyło stan"
        );
    }

    /// Świat bez warstwy Mikro i bez ruchu ma dawać pusty snapshot, a nie panikę:
    /// scenariusz bezgłowy stawia wycinek symulacji i wypełniacz ma to przeżyć.
    #[test]
    fn swiat_bez_ruchu_daje_pusty_snapshot() {
        let world = magnat_ecs::World::new(1);
        let mut f = SnapshotFiller::new();
        let mut snap = RenderSnapshot::default();
        f.fill(&world, &zapytanie(), &mut snap);
        assert!(snap.citizens.is_empty());
        assert!(snap.vehicles.is_empty());
        assert_eq!(
            snap.resident_bytes(),
            RenderSnapshot::default().resident_bytes()
        );
    }

    #[test]
    fn metry_na_milimetry_nie_zawijaja_sie_na_krancach() {
        assert_eq!(to_mm([1.5, -2.25, 0.0]), [1500, -2250, 0]);
        let ogromne = to_mm([1.0e12, -1.0e12, 0.0]);
        assert_eq!(ogromne[0], i32::MAX);
        assert_eq!(ogromne[1], i32::MIN);
    }

    #[test]
    fn kat_zawija_sie_do_pelnego_obrotu() {
        let tau = std::f32::consts::TAU;
        assert_eq!(yaw_u16(0.0), 0);
        assert_eq!(yaw_u16(tau), 0, "pełny obrót to zero, nie przepełnienie");
        assert!((i32::from(yaw_u16(tau / 2.0)) - 32_768).abs() <= 1);
        assert_eq!(yaw_u16(-tau / 4.0), yaw_u16(3.0 * tau / 4.0));
    }

    /// Faza rozsuwa się po indeksie encji — inaczej cały tłum maszerowałby w jednym takcie.
    #[test]
    fn faza_animacji_rozsuwa_tlum() {
        let fazy: std::collections::BTreeSet<u8> =
            (0..64u32).map(|i| anim::anim_phase(100, i)).collect();
        assert!(fazy.len() > 30, "tylko {} różnych faz", fazy.len());
        assert_eq!(
            anim::anim_phase(100, 5),
            anim::anim_phase(100, 5),
            "faza nie jest czysta"
        );
    }

    /// Klatka animacji zależy od **czasu animacji**, a nie od ticku symulacji: przy
    /// prędkości ×1 tick zmienia się raz na sekundę realną i klip stałby.
    #[test]
    fn faza_plynie_miedzy_tickami() {
        let a = anim::anim_phase(0, 3);
        let b = anim::anim_phase(250, 3);
        assert_ne!(a, b, "faza nie ruszyła się przez ćwierć sekundy");
    }
}
