//! Szyldy po stronie klienta (M11c §5.7, WP9).
//!
//! Atlas kafli i prostokąty napisów mieszkają w `engine/render`; tutaj jest to, czego
//! renderer nie może zrobić sam — **nadanie kafli nazwom firm**. Nazwy są w `CityData`,
//! a renderer miasta nie widzi (§6.3 pkt 1).

impl crate::app::App {
    /// Rozdaje kafle atlasu szyldów firmom miasta (WP9).
    ///
    /// Kafel jest **per firma, nie per zakład**: sieć z trzema sklepami ma na wszystkich
    /// ten sam szyld, bo to ta sama nazwa. Dzięki temu atlas mieści miasto, a nie tylko
    /// jego wycinek.
    ///
    /// `ponytail:` sufit nazwany — po wyczerpaniu atlasu (255 nazw) kolejne firmy
    /// zostają **bez szyldu**, a nie z cudzym. W mieście 4 km firm jest około dwustu;
    /// metropolia przekroczy ten próg i wtedy kafle trzeba będzie nadawać leniwie,
    /// tym firmom, które są w kadrze. Ścieżka wyjścia: LRU na atlasie w M11e.
    pub(crate) fn zwiaz_szyldy(&mut self, city: &magnat_world::CityData) {
        if self.szyldy_seed == Some(city.plan.seed) {
            return;
        }
        self.szyldy_seed = Some(city.plan.seed);
        self.szyldy = magnat_render::SignAtlas::new();
        let mut per_firma: Vec<magnat_render::SignId> = Vec::with_capacity(city.sites.firms.len());
        for f in &city.sites.firms {
            per_firma.push(self.szyldy.intern(&f.name));
        }
        let mut mapa: Vec<(magnat_core::SiteId, u16)> = city
            .sites
            .sites
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let kafel = per_firma
                    .get(s.firm.0.index() as usize)
                    .copied()
                    .unwrap_or_default();
                (
                    magnat_world::city::sites::site_id(i as u32),
                    kafel.0,
                )
            })
            .collect();
        mapa.sort_unstable_by_key(|(s, _)| *s);
        self.filler.set_signs(mapa);
    }

    /// Składa prostokąty napisów z rekordów zakładów tej klatki.
    ///
    /// Barwa liter jest jedna dla całego miasta i to jest świadome: marka firmy (M10)
    /// dopiero powstanie, a dobieranie koloru z palety dzielnicy dałoby szyldy zlewające
    /// się ze ścianą, na której wiszą.
    pub(crate) fn zloz_szyldy(&mut self) {
        self.szyldy_kadr.clear();
        for s in self.snapshot.front().sites.as_slice() {
            if s.sign_id == 0 {
                continue;
            }
            self.szyldy_kadr.push(magnat_render::SignQuad {
                center: [
                    s.pos[0] as f32 / 1000.0,
                    s.pos[1] as f32 / 1000.0,
                    s.pos[2] as f32 / 1000.0,
                ],
                // `ponytail:` wszystkie szyldy patrzą w `+X`. Sufit nazwany: kierunek
                // wejścia jest w `Entrance.seg`, czyli w osi ulicy, i wejdzie razem
                // z orientowaniem propów wnętrz przy elewacji.
                yaw: 0,
                tile: magnat_render::SignId(s.sign_id),
                color: [255, 238, 200, 255],
            });
        }
    }
}
