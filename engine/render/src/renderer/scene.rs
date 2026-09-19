//! Wejścia sceny dokładane przez M11c: przekrój, wnętrza i szyldy.
//!
//! Osobny blok `impl Renderer`, a nie kolejne metody w `renderer.rs`: tamten plik trzyma
//! **cykl życia urządzenia i zasobów GPU**, a te trzy rzeczy są wejściem klatki, które
//! klient podstawia przed rysowaniem. Rejestr długu strukturalnego `R1` przewidywał
//! dokładnie ten szew („M11 dokłada tu animacje, wnętrza i budżet klatki — wtedy szew
//! pokaże się sam").

use super::Renderer;

impl Renderer {
    /// Ustawia przekrój tej klatki i buduje czapki domykające (M11c §5.7, WP5).
    ///
    /// Sam clip jest uniformem M1 (`CameraState.clip_plane_z`) i to klient go ustawia —
    /// tutaj przychodzi **to, czego M1 nie ma**: rzędna wyliczona ze stropów oraz obrysy
    /// budynków, które płaszczyzna przecięła. Obrysy składa klient, bo `engine/render`
    /// nie widzi miasta i widzieć nie może (§6.3 pkt 1).
    ///
    /// Wołanie z `CutMode::Off` zeruje geometrię — czapka z poprzedniej klatki nie ma
    /// prawa zostać w kadrze po wyłączeniu cięcia.
    pub fn set_cut(
        &mut self,
        cut: crate::interiors::CutPlane,
        cuts: &[crate::interiors::BuildingCut],
        eye: glam::DVec3,
    ) {
        self.cut = cut;
        self.cap_geom.build(cuts, &cut, eye);
        self.cap.upload(&self.gpu.queue, &self.cap_geom.vertices);
    }

    /// Podstawia wyposażenie wnętrz tej klatki (M11c §5.7, WP5).
    ///
    /// Propy wchodzą do **tego samego bufora instancji** co mieszkańcy i pojazdy, więc
    /// nie kosztują ani jednego dodatkowego wiązania — i sortują się razem z nimi
    /// po `(model, poziom detalu)`, czyli mieszczą się w tych samych wsadach.
    pub fn set_interiors(&mut self, props: &[crate::interiors::PropPlacement]) {
        self.props.clear();
        self.props.extend_from_slice(props);
    }

    /// Podstawia napisy na szyldach tej klatki (M11c §5.7, WP9).
    ///
    /// Atlas wgrywa się **tylko wtedy, gdy przybyło kafli** — a przybywa ich wtedy, gdy
    /// gracz nazwie nową firmę. Stąd bierze się kryterium „zmiana nazwy aktualizuje
    /// szyld w jednej klatce": nazwa idzie do atlasu, numer kafla do snapshotu, a napis
    /// zmienia się w tej samej klatce, w której snapshot ten numer poniesie.
    pub fn set_signs(
        &mut self,
        atlas: &mut crate::signs::SignAtlas,
        signs: &[crate::signs::SignQuad],
        eye: glam::DVec3,
    ) {
        if atlas.is_dirty() {
            self.signs
                .upload_atlas(&self.gpu.device, &self.gpu.queue, atlas);
            atlas.clear_dirty();
        }
        self.sign_geom.build(signs, eye);
        self.signs.upload(&self.gpu.queue, &self.sign_geom.vertices);
    }

    /// Modele wyposażenia i szyldu — klient potrzebuje ich, żeby zawołać generator wnętrz.
    #[must_use]
    pub fn prop_models(&self) -> crate::interiors::PropModels {
        self.instances.models().props()
    }

    /// Przekrój obowiązujący w tej klatce.
    #[must_use]
    pub fn cut(&self) -> crate::interiors::CutPlane {
        self.cut
    }

    /// Wysokość kadru w pikselach. Czyta ją klient, bo próg rysowania encji wynika
    /// z jej rozmiaru ekranowego (`J-3`), a wycinek snapshotu składa się przed renderem.
    #[must_use]
    pub fn viewport_height_px(&self) -> f32 {
        self.gpu.config.height.max(1) as f32
    }
}
