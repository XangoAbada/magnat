//! Atlas szyldów: nazwa firmy gracza na tablicy nad wejściem (M11c §5.7, WP9).
//!
//! ### Dlaczego własny font, a nie krój z `engine/ui`
//!
//! `engine/ui` nie ma swojego kroju — jest warstwą nad `egui`, a atlas `egui` żyje
//! wewnątrz `egui_wgpu::Renderer` i wychodzi z niego wyłącznie jako `TexturesDelta`.
//! Sięganie po niego znaczyłoby wiązanie cyklu życia szyldów w świecie z cyklem życia
//! atlasu interfejsu — dwie rzeczy, które nie mają ze sobą nic wspólnego poza tym,
//! że obie rysują litery.
//!
//! `ponytail:` krój jest **bitmapowy 5 × 7**, wpisany w kod. Sufit nazwany: nie ma
//! kerningu, nie ma rozmiarów i nie ma znaków spoza zapisanej tablicy — te ostatnie
//! rysują się jako spacja. Przy szyldzie oglądanym z kilkunastu metrów, w grze, w której
//! wszystko jest sześcianem o boku ćwierć metra, krój wektorowy byłby niewidoczny.
//! Ścieżka wyjścia: prawdziwy krój przez atlas `egui`, kiedy M12 doda lokalizację
//! i pojawi się drugi alfabet.

mod gpu;
pub use gpu::{SignGeometry, SignQuad, SignRenderer, SignVertex, MAX_SIGNS, SIGN_H_M, SIGN_W_M};

/// Kafel atlasu. `0` znaczy „bez szyldu" i jest zarezerwowane, bo tak samo znaczy
/// `SiteRenderRec::sign_id`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub struct SignId(pub u16);

/// Szerokość kafla w pikselach (§5.7: tile 256 × 64).
pub const TILE_W: usize = 256;
/// Wysokość kafla w pikselach.
pub const TILE_H: usize = 64;
/// Ile kafli mieści atlas. 256 firm gracza to o dwa rzędy wielkości więcej, niż jeden
/// gracz zdąży założyć, a atlas waży wtedy 4 MB w R8.
pub const MAX_TILES: usize = 256;

/// Szerokość glifu w pikselach kroju.
const GLYPH_W: usize = 5;
/// Wysokość glifu w pikselach kroju.
const GLYPH_H: usize = 7;

/// Atlas szyldów: jedna tekstura R8, jeden kafel na nazwę.
///
/// Kafle nadaje się **przez internowanie**: ta sama nazwa dostaje ten sam numer, więc
/// zmiana nazwy firmy w interfejsie zmienia numer kafla i szyld w świecie aktualizuje
/// się w klatce, w której snapshot poniesie nowy `sign_id` (kryterium WP9).
#[derive(Clone, Debug)]
pub struct SignAtlas {
    /// Piksele wszystkich kafli, R8, wiersz po wierszu w obrębie kafla.
    pixels: Vec<u8>,
    /// Nazwy w kolejności nadawania numerów; indeks + 1 jest `SignId`.
    keys: Vec<String>,
    /// Czy od ostatniego wgrania doszedł kafel.
    dirty: bool,
}

impl Default for SignAtlas {
    fn default() -> SignAtlas {
        SignAtlas::new()
    }
}

impl SignAtlas {
    #[must_use]
    pub fn new() -> SignAtlas {
        SignAtlas {
            // Kafel zerowy zostaje pusty: `sign_id == 0` znaczy „bez szyldu", więc
            // musi mieć swoje miejsce w teksturze i musi być przezroczysty.
            pixels: vec![0; TILE_W * TILE_H],
            keys: Vec::new(),
            dirty: true,
        }
    }

    /// Numer kafla dla tej nazwy; wypala go, jeśli jeszcze nie istnieje.
    ///
    /// Zwraca `SignId(0)` po przepełnieniu atlasu — czyli **brak szyldu**, a nie cudzy
    /// szyld. To jest ta sama zasada co przy przepełnieniu bufora instancji: wolimy
    /// stracić obiekt niż pokazać zły.
    pub fn intern(&mut self, name: &str) -> SignId {
        if let Some(i) = self.keys.iter().position(|k| k == name) {
            return SignId(i as u16 + 1);
        }
        if self.keys.len() + 1 >= MAX_TILES {
            return SignId(0);
        }
        let id = SignId(self.keys.len() as u16 + 1);
        self.keys.push(name.to_string());
        self.pixels
            .resize((usize::from(id.0) + 1) * TILE_W * TILE_H, 0);
        let start = usize::from(id.0) * TILE_W * TILE_H;
        rysuj_napis(&mut self.pixels[start..start + TILE_W * TILE_H], name);
        self.dirty = true;
        id
    }

    #[must_use]
    pub fn tiles(&self) -> usize {
        self.keys.len() + 1
    }

    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Kafel jako wycinek pikseli — do testu bez GPU.
    #[must_use]
    pub fn tile(&self, id: SignId) -> &[u8] {
        let start = usize::from(id.0) * TILE_W * TILE_H;
        &self.pixels[start..start + TILE_W * TILE_H]
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    #[must_use]
    pub fn bytes(&self) -> usize {
        self.pixels.len()
    }
}

/// Wypala napis na środku kafla, skalując glify tak, żeby nazwa się zmieściła.
///
/// Skala jest całkowita — półpiksel w krojach bitmapowych daje rozmycie, a rozmyty
/// szyld czyta się gorzej niż mniejszy ostry.
fn rysuj_napis(kafel: &mut [u8], tekst: &str) {
    let znaki: Vec<char> = tekst.chars().take(40).collect();
    if znaki.is_empty() {
        return;
    }
    // Odstęp międzyliterowy to jeden piksel kroju, więc litera zajmuje sześć.
    let szerokosc_1x = znaki.len() * (GLYPH_W + 1);
    let skala = (TILE_W / szerokosc_1x.max(1)).clamp(1, TILE_H / GLYPH_H);
    let w = szerokosc_1x * skala;
    let x0 = TILE_W.saturating_sub(w) / 2;
    let y0 = TILE_H.saturating_sub(GLYPH_H * skala) / 2;
    for (i, c) in znaki.iter().enumerate() {
        let g = glif(*c);
        for (row, bits) in g.iter().enumerate() {
            for col in 0..GLYPH_W {
                if bits & (1 << (GLYPH_W - 1 - col)) == 0 {
                    continue;
                }
                for sy in 0..skala {
                    for sx in 0..skala {
                        let x = x0 + (i * (GLYPH_W + 1) + col) * skala + sx;
                        let y = y0 + row * skala + sy;
                        if x < TILE_W && y < TILE_H {
                            kafel[y * TILE_W + x] = 255;
                        }
                    }
                }
            }
        }
    }
}

/// Glif 5 × 7 jako siedem wierszy po pięć bitów (bit 4 = lewa kolumna).
///
/// Polskie znaki diakrytyczne mają **własne glify**, a nie podmianę na literę bazową:
/// szyld „Piekarnia »Łąka«" ma być tym napisem, a nie „Piekarnia »Laka«". Litery,
/// których tu nie ma, rysują się jako spacja — i to jest widoczne, a nie ciche.
fn glif(c: char) -> [u8; GLYPH_H] {
    // Wielkie i małe litery dzielą glif: krój ma jeden rozmiar, a szyld i tak czyta się
    // wersalikami. Znak spoza tablicy jest spacją.
    let c = c.to_uppercase().next().unwrap_or(' ');
    match c {
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'Ą' => [
            0b01110, 0b10001, 0b11111, 0b10001, 0b10001, 0b00100, 0b00010,
        ],
        'B' => [
            0b11110, 0b10001, 0b11110, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110,
        ],
        'Ć' => [
            0b00010, 0b01110, 0b10001, 0b10000, 0b10000, 0b10001, 0b01110,
        ],
        'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'E' => [
            0b11111, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'Ę' => [
            0b11111, 0b10000, 0b11110, 0b10000, 0b11111, 0b00100, 0b00010,
        ],
        'F' => [
            0b11111, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111,
        ],
        'H' => [
            0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        'J' => [
            0b00111, 0b00010, 0b00010, 0b00010, 0b10010, 0b10010, 0b01100,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'Ł' => [
            0b10000, 0b10000, 0b10100, 0b11000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'Ń' => [
            0b00010, 0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'Ó' => [
            0b00010, 0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'Ś' => [
            0b00010, 0b01111, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001,
        ],
        'X' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        'Ź' => [
            0b00010, 0b11111, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        'Ż' => [
            0b00100, 0b11111, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00110, 0b01000, 0b10000, 0b11111,
        ],
        '3' => [
            0b11111, 0b00010, 0b00100, 0b00110, 0b00001, 0b10001, 0b01110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
        ],
        '6' => [
            0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
        ],
        '.' => [0, 0, 0, 0, 0, 0b01100, 0b01100],
        ',' => [0, 0, 0, 0, 0b01100, 0b01100, 0b01000],
        '-' => [0, 0, 0, 0b11111, 0, 0, 0],
        '\'' | '"' | '„' | '”' | '«' | '»' => [0b01010, 0b01010, 0, 0, 0, 0, 0],
        '&' => [
            0b01100, 0b10010, 0b10100, 0b01000, 0b10101, 0b10010, 0b01101,
        ],
        _ => [0; GLYPH_H],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zapalone(a: &SignAtlas, id: SignId) -> usize {
        a.tile(id).iter().filter(|p| **p > 0).count()
    }

    /// Ta sama nazwa daje ten sam kafel, inna — inny. Na tym stoi kryterium WP9:
    /// zmiana nazwy w interfejsie zmienia `sign_id` w snapshocie, a szyld w świecie
    /// zmienia się w tej samej klatce, w której snapshot ten numer poniesie.
    #[test]
    fn internowanie_jest_stabilne() {
        let mut a = SignAtlas::new();
        let x = a.intern("Piekarnia Wola");
        assert_eq!(x, a.intern("Piekarnia Wola"));
        let y = a.intern("Piekarnia Żoliborz");
        assert_ne!(x, y);
        assert_eq!(a.tiles(), 3, "kafel zerowy plus dwie nazwy");
        assert_ne!(x, SignId(0), "nazwa dostała numer „bez szyldu”");
    }

    /// Kafel zerowy jest **pusty** i zostaje pusty: `sign_id == 0` znaczy „bez szyldu".
    #[test]
    fn kafel_zerowy_jest_pusty() {
        let mut a = SignAtlas::new();
        a.intern("Cokolwiek");
        assert_eq!(zapalone(&a, SignId(0)), 0);
    }

    /// Napis faktycznie się rysuje, a polskie znaki mają własne glify — „Łąka" ma
    /// zapalać więcej pikseli niż „Laka", bo ogonek i kreska to dodatkowe piksele.
    #[test]
    fn polskie_znaki_maja_wlasne_glify() {
        let mut a = SignAtlas::new();
        let z = a.intern("ŁĄKA");
        let b = a.intern("LAKA");
        assert!(zapalone(&a, z) > 0 && zapalone(&a, b) > 0);
        assert_ne!(
            a.tile(z),
            a.tile(b),
            "polska litera narysowała się jak jej odpowiednik bez znaku"
        );
    }

    /// Długa nazwa ma się **zmieścić**, a nie wyjść poza kafel.
    #[test]
    fn dluga_nazwa_miesci_sie_w_kaflu() {
        let mut a = SignAtlas::new();
        let id = a.intern("Przedsiębiorstwo Wielobranżowe Śródmieście");
        assert!(zapalone(&a, id) > 0);
        assert_eq!(a.tile(id).len(), TILE_W * TILE_H);
    }

    /// Przepełniony atlas oddaje **brak szyldu**, a nie cudzy kafel.
    #[test]
    fn przepelniony_atlas_nie_oddaje_cudzego_kafla() {
        let mut a = SignAtlas::new();
        for i in 0..MAX_TILES + 8 {
            a.intern(&format!("Firma {i}"));
        }
        assert!(a.tiles() <= MAX_TILES);
        assert_eq!(a.intern("Jeszcze jedna"), SignId(0));
    }
}
