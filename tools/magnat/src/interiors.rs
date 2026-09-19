//! Przekrój i wnętrza po stronie klienta (M11c §5.7, WP5).
//!
//! Tu mieszka to, czego `engine/render` nie może zrobić samo: **złożenie opisu budynku
//! z danych miasta**. Renderer nie widzi `CityData` i widzieć nie może (§6.3 pkt 1),
//! więc obrysy do domknięcia przekroju i kondygnacje do generatora wnętrz składa klient,
//! który widzi naraz miasto i kamerę.
//!
//! Zakres pracy jest ograniczony **promieniem wokół celu kamery**, a nie liczbą budynków
//! w mieście: wnętrze niewidoczne nie jest generowane (§5.5), a przekrój bez cięcia
//! nie kosztuje ani jednego wielokąta.

use magnat_render::{BuildingCut, CutMode, CutPlane, FloorUse, InteriorSpec, PropPlacement};
use magnat_world::city::build::UnitKind;
use magnat_world::CityData;

/// Promień, w którym składamy przekrój i wnętrza, w metrach.
///
/// Sto pięćdziesiąt, bo tyle widać przez przecięty kwartał przy orbicie na poziomie
/// dzielnicy, a propy mają własny próg rysowania 60 m. Większy promień kosztowałby
/// generowanie wnętrz, których renderer i tak nie narysuje.
const PROMIEN_M: f32 = 150.0;

/// Ile budynków obsługujemy w jednej klatce.
///
/// `ponytail:` sufit nazwany — przy gęstej zabudowie część przeciętych budynków zostaje
/// bez czapki i bez wnętrza, i będą to te dalsze, bo lista idzie od najbliższych.
/// Ścieżka wyjścia: cache wnętrz z §5.7 (LRU 64) w M11e, kiedy będzie budżet klatki,
/// wobec którego dałoby się go zmierzyć.
const BUDYNKOW_MAX: usize = 96;

/// Rzędna cięcia dla trybu `Level(n)`: strop `n`-tej kondygnacji **budynku, na który
/// patrzy gracz**.
///
/// Płaszczyzna jest jedna dla całego kadru, więc musi się skądś wziąć — a jedyna
/// sensowna odpowiedź to budynek pod celem kamery. Cięcie „co trzy metry" przechodziłoby
/// przez środek witryny każdej kamienicy, bo parter usługowy jest wyższy od piętra
/// (§5.7). Brak budynku pod celem (park, jezdnia) zostawia cięcie wyłączone.
pub(crate) fn poziom_ciecia(city: &CityData, cel: glam::DVec3, n: u8) -> CutPlane {
    let c = glam::Vec2::new(cel.x as f32, cel.y as f32);
    let mut kand = Vec::new();
    city.buildings.index.query_rect(
        magnat_spatial::Aabb2 {
            min: c - glam::Vec2::splat(60.0),
            max: c + glam::Vec2::splat(60.0),
        },
        &mut kand,
    );
    let Some(b) = kand.iter().min_by_key(|b| {
        let a = city.buildings.buildings[b.0.index() as usize].aabb;
        let d = glam::Vec2::new((a.min.x + a.max.x) * 0.5, (a.min.y + a.max.y) * 0.5) - c;
        (d.length_squared() * 100.0) as i64
    }) else {
        return CutPlane::off();
    };
    let bud = &city.buildings.buildings[b.0.index() as usize];
    CutPlane::level(bud.aabb.min.z, &bud.floor_heights_dm, n)
}

/// Bufory składania — trzymane między klatkami, bo klatka nie alokuje.
#[derive(Default)]
pub(crate) struct Wnetrza {
    pub(crate) cuts: Vec<BuildingCut>,
    pub(crate) props: Vec<PropPlacement>,
    kandydaci: Vec<magnat_core::BuildingId>,
}

impl Wnetrza {
    /// Składa przekrój i wyposażenie widocznych budynków.
    ///
    /// `sites` to zakłady ze snapshotu — stąd bierze się `stock_fill`, czyli wypełnienie
    /// regałów. Budynek bez zakładu dostaje wnętrze neutralne, a nie żadne: kamienica
    /// też ma klatkę schodową i mieszkania, tyle że prywatne (`FloorUse::Dwelling`).
    pub(crate) fn zloz(
        &mut self,
        city: &CityData,
        sites: &[magnat_sim_snapshot::SiteRenderRec],
        cel: glam::DVec3,
        cut: CutPlane,
        modele: magnat_render::PropModels,
        seed: u64,
    ) {
        self.cuts.clear();
        self.props.clear();
        if cut.mode == CutMode::Off {
            return;
        }
        let c = glam::Vec2::new(cel.x as f32, cel.y as f32);
        city.buildings.index.query_rect(
            magnat_spatial::Aabb2 {
                min: c - glam::Vec2::splat(PROMIEN_M),
                max: c + glam::Vec2::splat(PROMIEN_M),
            },
            &mut self.kandydaci,
        );
        // Od najbliższych: sufit `BUDYNKOW_MAX` ma obcinać to, co dalej, a nie to,
        // co akurat wypadło w indeksie jako ostatnie.
        self.kandydaci.sort_unstable_by_key(|b| {
            let a = city.buildings.buildings[b.0.index() as usize].aabb;
            let d = glam::Vec2::new((a.min.x + a.max.x) * 0.5, (a.min.y + a.max.y) * 0.5) - c;
            (d.length_squared() * 100.0) as i64
        });
        self.kandydaci.truncate(BUDYNKOW_MAX);

        for b in &self.kandydaci {
            let bud = &city.buildings.buildings[b.0.index() as usize];
            let obrys: Vec<[f32; 2]> = city
                .roads
                .geom
                .get(bud.footprint)
                .iter()
                .map(|p| [p.x, p.y])
                .collect();
            self.cuts.push(BuildingCut {
                footprint: obrys,
                base_z_m: bud.aabb.min.z,
                height_m: bud.aabb.max.z - bud.aabb.min.z,
                // Barwa przekroju: jasny beton. Materiał ściany zna pass chunków,
                // a nie ta ścieżka — a przekrój i tak jest płytą stropową, nie ścianą.
                color: [186, 180, 170, 255],
            });

            // Wnętrze tylko poniżej cięcia i tylko tam, gdzie kondygnacja się otwiera.
            if cut.world_y <= bud.aabb.min.z {
                continue;
            }
            let zaklad = city
                .sites
                .site_of_building(*b)
                .and_then(|_| {
                    let id = city.sites.by_building[b.0.index() as usize]?;
                    let sid = magnat_game::world::plants::site_id(id as usize);
                    sites.iter().find(|s| s.entity_lo == sid.0.index())
                })
                .copied()
                .unwrap_or(magnat_sim_snapshot::SiteRenderRec {
                    activity: 128,
                    stock_fill: 255,
                    ..Default::default()
                });
            let spec = spec_budynku(city, bud, cut.world_y);
            let kit = magnat_render::generate_interior(
                &spec,
                &zaklad,
                &modele,
                seed ^ u64::from(b.0.index()),
            );
            self.props.extend(kit.props);
        }
    }
}

/// Opis budynku dla generatora wnętrz: obrys jako prostokąt, kondygnacje i ich użycie.
///
/// Sposób użytkowania bierze się z **lokali** M2 (`Unit.kind`), a nie z gramatyki:
/// gramatyka mówi, jak budynek wygląda, a lokale — co się w nim dzieje. Kondygnacja
/// bez lokalu jest mieszkalna, bo taka jest większość pięter w mieście.
fn spec_budynku(
    city: &CityData,
    bud: &magnat_world::city::build::Building,
    world_y: f32,
) -> InteriorSpec {
    let mut uzycie = vec![FloorUse::Dwelling; bud.floor_heights_dm.len().max(1)];
    for u in &city.buildings.units[bud.units.start as usize..bud.units.end as usize] {
        let Ok(p) = usize::try_from(u.floor.max(0)) else {
            continue;
        };
        if p >= uzycie.len() {
            continue;
        }
        // Pierwszy lokal kondygnacji nadaje jej charakter. Piętro z biurem i magazynem
        // obok dostanie biurka — i to jest właściwa odpowiedź, bo biuro widać z ulicy.
        let nowe = match u.kind {
            UnitKind::Retail => FloorUse::Retail,
            UnitKind::Office => FloorUse::Office,
            UnitKind::Workshop => FloorUse::Workshop,
            UnitKind::Storage => FloorUse::Storage,
            UnitKind::Dwelling { .. } | UnitKind::Common => continue,
        };
        if uzycie[p] == FloorUse::Dwelling {
            uzycie[p] = nowe;
        }
    }
    // Kondygnacje powyżej cięcia odpadają: propów, których nie widać, nie generujemy.
    let mut z = bud.aabb.min.z;
    let mut wysokosci: Vec<u16> = Vec::new();
    for h in &bud.floor_heights_dm {
        if z >= world_y {
            break;
        }
        wysokosci.push(*h);
        z += f32::from(*h) * 0.1;
    }
    InteriorSpec {
        bounds: [
            bud.aabb.min.x,
            bud.aabb.min.y,
            bud.aabb.max.x,
            bud.aabb.max.y,
        ],
        base_z_m: bud.aabb.min.z,
        floor_heights_dm: wysokosci,
        floor_use: uzycie,
        // Klatka i korytarz: jedna piąta powierzchni brutto. Gramatyka M2 niesie tę
        // liczbę per budynek, ale w `Building` jej nie ma — jest w `BuildingGrammar`,
        // a ten jest katalogiem, nie własnością bryły.
        circulation_share: 0.2,
    }
}

// ── Krok klatki ──────────────────────────────────────────────────────────────────────

impl crate::app::App {
    /// `entity_lo` postaci gracza albo `None`, gdy gracz jeszcze jej nie wybrał.
    pub(crate) fn postac_gracza(&self) -> Option<u32> {
        let magnat_game::GameState::Playing(s) = &self.game else {
            return None;
        };
        s.player().map(|p| p.citizen.0.index())
    }

    /// Składa przekrój i wnętrza tej klatki, i ustawia uniform cięcia (M11c §5.7, WP5).
    ///
    /// Idzie **po** publikacji snapshotu, bo wypełnienie regałów bierze się ze stanu
    /// magazynu, czyli z rekordów zakładów — a te powstają dopiero w publikacji.
    ///
    /// W trybie pierwszoosobowym cięcie jest zawsze włączone na wysokości nad głową:
    /// bez tego strop nad graczem zasłania wnętrze, po którym gracz właśnie chodzi
    /// (§5.7, `CutPlane::Box` wokół kamery — tu w wersji płaszczyznowej).
    pub(crate) fn zloz_przekroj(&mut self) {
        let magnat_game::GameState::Playing(s) = &self.game else {
            self.przekroj = magnat_render::CutPlane::off();
            self.wnetrza.cuts.clear();
            self.wnetrza.props.clear();
            self.camera.clip_plane_z = None;
            return;
        };
        // Kamera przypięta do postaci gracza idzie za **rekordem snapshotu**, a nie za
        // klawiszami: postać jest zwykłym agentem, a tryb pierwszoosobowy tylko ją
        // ogląda (decyzja 9.7). Pozycja jest z poprzedniej publikacji i to jest
        // w porządku — klatka renderu i tak interpoluje między publikacjami.
        if self.camera.anchor().is_some() {
            let p = self.snapshot.front().player;
            if p.citizen != 0 && p.eye != [0; 3] {
                self.camera.set_eye(glam::DVec3::new(
                    f64::from(p.eye[0]) / 1000.0,
                    f64::from(p.eye[1]) / 1000.0,
                    // Rekord niesie **oko**, a kamera dokłada wzrost sama — odejmujemy go
                    // z powrotem, żeby nie podnieść gracza dwa razy.
                    f64::from(p.eye[2]) / 1000.0 - f64::from(crate::WZROST_OCZU_M),
                ));
            }
        }
        let cel = self.camera.target();
        let fpp = matches!(
            self.camera.mode,
            magnat_render::CameraMode::FirstPerson { .. }
        );
        self.przekroj = if fpp {
            magnat_render::CutPlane::above(self.camera.eye().z as f32 + 1.0)
        } else if self.ciecie == 0 {
            magnat_render::CutPlane::off()
        } else {
            crate::interiors::poziom_ciecia(&s.built.city, cel, self.ciecie)
        };
        self.camera.clip_plane_z = self.przekroj.clip_plane_z();

        let modele = self
            .renderer
            .as_ref()
            .map(magnat_render::Renderer::prop_models)
            .unwrap_or_default();
        let ziarno = self.params.seed ^ u64::from(magnat_core::StreamId::Interior as u32);
        // Pożyczka miasta kończy się przed `zloz`, bo ten bierze `&mut self.wnetrza`.
        let city = s.built.city.clone();
        let sites = self.snapshot.front().sites.as_slice().to_vec();
        self.wnetrza
            .zloz(&city, &sites, cel, self.przekroj, modele, ziarno);
    }
}
