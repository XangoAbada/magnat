//! Miniatura mapy cieplnej (`M9b` §5.8, WP6).
//!
//! Pole skalarne nakładki jako tekstura [`THUMB_PX`] × [`THUMB_PX`] w rogu panelu.
//! Paleta pochodzi z `data/ui/overlays.ron` (`K-19`) — tej samej tabeli, którą czyta
//! podgląd `headless`, żeby klient i przebieg bezgłowy rysowały **tę samą** mapę.
//!
//! Tekstura powstaje przy [`HeatmapThumb::update`] i żyje między klatkami; kadencja
//! odświeżania należy do wołającego (`EveryHour` z §7). Rysowanie kosztuje jeden
//! prostokąt z teksturą, niezależnie od rozmiaru pola.

/// Bok miniatury w teksturze.
pub const THUMB_PX: usize = 256;

pub struct HeatmapThumb {
    nazwa: String,
    tex: Option<egui::TextureHandle>,
}

impl HeatmapThumb {
    #[must_use]
    pub fn new(nazwa: impl Into<String>) -> HeatmapThumb {
        HeatmapThumb {
            nazwa: nazwa.into(),
            tex: None,
        }
    }

    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.tex.is_some()
    }

    /// Przelicza pole na teksturę.
    ///
    /// `values` to **indeksy palety** o boku `dim` (tak, jak wychodzą z
    /// `OverlaySpec::index_of`), a nie wartości surowe: progi i barwy są w danych,
    /// a nie w kodzie widgetu. Indeks 0 znaczy „brak wartości" i jest przezroczysty —
    /// inaczej pusta komórka udawałaby najniższy próg.
    pub fn update(
        &mut self,
        ctx: &egui::Context,
        dim: u32,
        values: &[u8],
        palette: &[[u8; 4]],
    ) -> bool {
        if dim == 0 || values.len() < (dim * dim) as usize {
            return false;
        }
        let mut px = Vec::with_capacity(THUMB_PX * THUMB_PX);
        for y in 0..THUMB_PX {
            // Najbliższy sąsiad, nie filtrowanie: progi palety są skokowe, więc
            // uśrednianie wyprodukowałoby kolor, którego w legendzie nie ma.
            let sy = (y * dim as usize / THUMB_PX).min(dim as usize - 1);
            for x in 0..THUMB_PX {
                let sx = (x * dim as usize / THUMB_PX).min(dim as usize - 1);
                let i = values[sy * dim as usize + sx] as usize;
                let c = if i == 0 {
                    [0, 0, 0, 0]
                } else {
                    palette.get(i).copied().unwrap_or([0, 0, 0, 0])
                };
                px.push(egui::Color32::from_rgba_unmultiplied(
                    c[0], c[1], c[2], c[3],
                ));
            }
        }
        let obraz = egui::ColorImage {
            size: [THUMB_PX, THUMB_PX],
            pixels: px,
            source_size: egui::vec2(THUMB_PX as f32, THUMB_PX as f32),
        };
        match &mut self.tex {
            Some(t) => t.set(obraz, egui::TextureOptions::NEAREST),
            None => {
                self.tex =
                    Some(ctx.load_texture(&self.nazwa, obraz, egui::TextureOptions::NEAREST));
            }
        }
        true
    }

    /// Rysuje miniaturę o zadanym boku. Bez tekstury nie rysuje nic i mówi o tym
    /// zwracając `false` — pusty prostokąt wyglądałby jak pole bez danych.
    pub fn show(&self, ui: &mut egui::Ui, bok: f32) -> bool {
        let Some(t) = &self.tex else {
            return false;
        };
        ui.add(egui::Image::new((t.id(), egui::vec2(bok, bok))));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn miniatura_powstaje_z_pola_i_rysuje_sie_bez_gpu() {
        let ctx = egui::Context::default();
        let dim = 64u32;
        let pole: Vec<u8> = (0..dim * dim).map(|i| (i % 4) as u8).collect();
        let paleta = [
            [0, 0, 0, 0],
            [10, 20, 30, 255],
            [40, 50, 60, 255],
            [70, 80, 90, 255],
        ];
        let mut m = HeatmapThumb::new("test");
        assert!(!m.is_ready());
        assert!(m.update(&ctx, dim, &pole, &paleta));
        assert!(m.is_ready());

        let mut narysowano = false;
        let _ = crate::testing::draw_in(&ctx, crate::testing::input(crate::testing::EKRAN), |ui| {
            narysowano = m.show(ui, 128.0);
        });
        assert!(narysowano);
    }

    #[test]
    fn pole_krotsze_niz_deklarowany_bok_nie_panikuje() {
        let ctx = egui::Context::default();
        let mut m = HeatmapThumb::new("test");
        assert!(!m.update(&ctx, 64, &[0u8; 10], &[[0, 0, 0, 0]]));
        assert!(!m.update(&ctx, 0, &[], &[]));
        assert!(!m.is_ready());
    }
}
