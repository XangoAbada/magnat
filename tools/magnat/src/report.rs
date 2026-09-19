//! Raporty diagnostyczne klienta (`--screenshot`, `--bench`, `--pick`).
//!
//! Wydzielone z `app.rs` w M11d bez zmiany zachowania: pętla okna i wypisywanie stanu
//! na standardowe wyjście błędu to dwa tematy, a plik przekroczył próg strukturalny
//! dopiero wtedy, gdy doszedł do niego raport pogody i dźwięku (CLAUDE.md, „przegląd
//! strukturalny po zamkniętym pakiecie").
//!
//! Te trzy funkcje są jedynym sposobem, w jaki render i dźwięk odpowiadają na pytania
//! bez patrzenia na ekran — a przy pustym albo cichym kadrze to jest pierwsze pytanie.

use crate::app::App;
use crate::bench;
use crate::preview::czasy_passow;
use magnat_game::GameState;

impl App {
    /// Raport z przelotu: jeden wiersz na etap plus podsumowanie całości.
    ///
    /// Percentyle, nie sama średnia: 60 FPS średnio przy jednym zacięciu 200 ms to gorsze
    /// wrażenie niż równe 50 FPS, a średnia obu nie odróżnia.
    pub(crate) fn raport_bench(&mut self, s: magnat_render::FrameStats) {
        eprintln!("── raport wydajności (§7.4) ──");
        for (i, etap) in bench::ETAPY.iter().enumerate() {
            let Some(p) = self.bench_etapy.get(i) else {
                continue;
            };
            eprintln!("{}", bench::wiersz(etap, p));
        }

        let wszystkie: usize = self.bench_etapy.iter().map(|p| p.ms.len()).sum();
        let zaciecia: usize = self.bench_etapy.iter().map(bench::Pomiar::zaciecia).sum();
        eprintln!(
            "razem {wszystkie} klatek w {:.0} s, zacięć > 33 ms: {zaciecia} (próg §7.4: ≤ 2 na 60 s)",
            self.bench_czas_s
        );

        let gpu = magnat_render::PASS_NAMES
            .iter()
            .enumerate()
            .map(|(i, nazwa)| {
                let mut v: Vec<f32> = self.bench_pass_ms.iter().map(|p| p[i]).collect();
                v.sort_by(f32::total_cmp);
                if v.is_empty() {
                    return format!("{nazwa} brak pomiaru");
                }
                format!(
                    "{nazwa} {:.2} ms (p95 {:.2})",
                    v[v.len() / 2],
                    v[(v.len() - 1) * 95 / 100]
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        if !self.swiatla.is_empty() {
            eprintln!("światła: {} · {}", self.swiatla.len(), self.occupancy_opis);
        }
        eprintln!(
            "GPU {gpu}; arena {:.1} MB z {:.1} MB",
            (s.vertex_bytes + s.index_bytes) as f64 / (1024.0 * 1024.0),
            s.arena_capacity_bytes as f64 / (1024.0 * 1024.0),
        );
    }

    /// Cztery zdania stanu obok zrzutu: render, snapshot, kamera i pierwszy pieszy.
    ///
    /// Razem odpowiadają na pytanie, które przy pustym kadrze zadaje się najpierw:
    /// czy symulacja **nic nie oddała**, czy render **tego nie narysował**, czy może
    /// kamera patrzy gdzie indziej. Trzy różne przyczyny dają ten sam obraz, więc bez
    /// tych liczb szuka się ich po kolei i po omacku — tak właśnie wyszło, że piesi
    /// w M11b chodzili jedenaście metrów pod terenem.
    pub(crate) fn raport_zrzutu(
        &self,
        s: &magnat_render::FrameStats,
        kamera: &magnat_render::CameraState,
    ) {
        eprintln!(
            "stan renderu: {} chunków rezydentnych, {} rysowanych, {} trójkątów, \
             {} encji w {} wsadach, {:.1} MB geometrii; GPU {}",
            s.chunks_resident,
            s.chunks_drawn,
            s.triangles,
            s.instances,
            s.instance_batches,
            (s.vertex_bytes + s.index_bytes) as f64 / (1024.0 * 1024.0),
            czasy_passow(s),
        );
        let snap = self.snapshot.front();
        // Warstwa Mikro obok snapshotu, bo to **dwie różne bramki** i przy pustym
        // kadrze trzeba wiedzieć, która zamknęła: okno warstwy (promień wokół celu
        // kamery) czy wycinek snapshotu (`ViewQuery.aabb` plus cap).
        let mikro = match &self.game {
            GameState::Playing(s) => s
                .app
                .world
                .resource::<magnat_agents::AgentSources>()
                .get()
                .map_or(0, |z| z.travel.micro_len()),
            _ => 0,
        };
        eprintln!(
            "snapshot: {} mieszkańców, {} pojazdów, {} zakładów (warstwa Mikro: {mikro} encji)",
            snap.citizens.len(),
            snap.vehicles.len(),
            snap.sites.len(),
        );
        let w = snap.weather;
        eprintln!(
            "pogoda: opad {} ({}), mgła {}, chmury {}, śnieg {}, sezon {}, dzień {} ·              wiatr {:?} m/s · {} °C",
            w.precipitation,
            if w.kind == 0 { "deszcz" } else { "śnieg" },
            w.fog_density,
            w.cloud,
            w.snow_cover,
            w.season,
            w.daylight,
            w.wind,
            w.temp_c,
        );
        let ciemnych = snap
            .power
            .iter()
            .filter(|p| p.supply_ratio < magnat_game::ambience::BLACKOUT_THRESHOLD)
            .count();
        eprintln!(
            "światła: {} w liście, {} zakładów pracuje, {} z pióropuszem, {} kominów dymi,              {} cząstek pogody, {ciemnych} dzielnic bez prądu · remeshing chunków: {}",
            snap.lights.len(),
            snap.sites.as_slice().iter().filter(|s| s.activity > 0).count(),
            snap.sites.as_slice().iter().filter(|s| s.plume_kind() != 0).count(),
            snap.sites
                .as_slice()
                .iter()
                .filter(|s| magnat_render::plume_density(s) > 0)
                .count(),
            self.renderer
                .as_ref()
                .map_or(0, magnat_render::Renderer::weather_particles),
            self.streamer
                .as_ref()
                .map_or(0, crate::stream::Streamer::remesh_count),
        );
        if let Some(a) = self.audio.as_ref() {
            let st = a.stats();
            eprintln!(
                "dźwięk: {} głosów, {} odrzuconych, nastrój {:?}, takt {}",
                st.voices_active,
                st.voices_dropped,
                a.mood(),
                st.bar
            );
        } else {
            eprintln!("dźwięk: wyłączony");
        }
        eprintln!(
            "szyldy: {} kafli w atlasie, {} napisów w kadrze, {} zakładów z kaflem",
            self.szyldy.tiles(),
            self.szyldy_kadr.len(),
            snap.sites
                .as_slice()
                .iter()
                .filter(|s| s.sign_id != 0)
                .count(),
        );
        eprintln!(
            "przekrój: {:?} na {:.1} m · czapek {} wierzchołków · propów wnętrz {}",
            self.przekroj.mode,
            self.przekroj.world_y,
            self.wnetrza.cuts.len() * 6,
            self.wnetrza.props.len(),
        );
        eprintln!(
            "kamera: oko {:?}, cel {:?}",
            kamera.eye().to_array(),
            kamera.target().to_array()
        );
        if let Some(c) = snap.citizens.as_slice().first() {
            eprintln!(
                "pierwszy pieszy: {:?} m, klip {}, faza {}, yaw {}",
                [
                    c.pos[0] as f32 / 1000.0,
                    c.pos[1] as f32 / 1000.0,
                    c.pos[2] as f32 / 1000.0
                ],
                c.anim_state,
                c.anim_phase,
                c.yaw
            );
        }
    }

    /// `--pick`: kto jest pod zadanym pikselem. Diagnostyka bufora identyfikatorów
    /// bez myszy — kryterium WP11 fazy M3.
    pub(crate) fn pick_diagnostyczny(&mut self) {
        let Some(renderer) = self.renderer.as_ref() else {
            return;
        };
        let trafiony = renderer.pick();
        // Snapshot razem z buforem: „nic pod kursorem" ma dwie zupełnie różne
        // przyczyny — pusty kadr i pusty snapshot — a bez tych liczb wyglądają tak samo.
        let snap = self.snapshot.front();
        let oko = self.camera.eye();
        let najblizszy = snap
            .citizens
            .as_slice()
            .iter()
            .map(|c| {
                let d = glam::DVec3::new(
                    f64::from(c.pos[0]) / 1000.0,
                    f64::from(c.pos[1]) / 1000.0,
                    f64::from(c.pos[2]) / 1000.0,
                ) - oko;
                d.length()
            })
            .fold(f64::INFINITY, f64::min);
        eprintln!(
            "bufor ID {}×{}, kamera nad ({:.0}, {:.0}) z {:.0} m; instancji rysowanych {}; snapshot: {} mieszkańców, {} pojazdów, najbliższy {:.0} m (odcięcie {:.0} m)",
            renderer.gpu.config.width,
            renderer.gpu.config.height,
            oko.x,
            oko.y,
            oko.z,
            renderer.instances.drawn(),
            snap.citizens.len(),
            snap.vehicles.len(),
            najblizszy,
            magnat_render::instancing::draw_distance_m(
                magnat_render::instancing::ENTITY_RADIUS_NOMINAL_M,
                self.camera.fov_deg,
                renderer.viewport_height_px(),
            ),
        );
        let kursor = self.kursor;
        match trafiony {
            Some(hit) => {
                let i = hit.entity;
                println!("bufor ID: piksel {kursor:?} → {:?} {i}", hit.kind);
                let mut karta = None;
                if let (Some(c), GameState::Playing(s)) = (self.citizens.as_mut(), &mut self.game) {
                    if c.select(s, i) {
                        karta = Some(c.card_text(s));
                    }
                }
                match karta {
                    Some(t) => println!("{t}"),
                    None => println!("encja {i} nie jest mieszkańcem z listy populacji"),
                }
            }
            None => println!("bufor ID: piksel {kursor:?} → nic"),
        }
        self.tryb_pick = false;
        if self.zrzut.is_none() {
            self.koniec = true;
        }
    }
}
