//! Nakładki debug na terenie (M1 §1 pkt 2, WP-R6).
//!
//! Osiem podglądów z §1 idzie przez **jeden** mechanizm `TerrainOverlay`: pole skalarne na
//! siatce roboczej plus paleta. To nie jest oszczędność kodu, tylko warunek z §6.1 — M2
//! podepnie pod ten sam mechanizm wartość dzielnicy, dostępność i hałas, i ma zastać
//! gotowy kontrakt, a nie osiem wariantów rysowania mapy.
//!
//! Wartości są **kwantowane do bajtu**, bo nakładka służy do oglądania, nie do liczenia.
//! Kto potrzebuje dokładnej liczby, klika w teren i czyta ją w inspektorze (§1 pkt 5).

use magnat_world::{CityData, Terrain, WaterClass};

/// Który podgląd jest włączony. `Brak` to normalny render.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Nakladka {
    #[default]
    Brak,
    Wysokosc,
    Splyw,
    KlasaWody,
    Biom,
    TemperaturaStycznia,
    TemperaturaLipca,
    Opady,
    Geologia,
    Zloza,
    /// M2e, WP16 — jedyna nakładka **danych miejskich**, a nie terenu. Paleta i progi
    /// z `data/ui/overlays.ron`, żeby podgląd headless i klient pokazywały to samo.
    WartoscGruntu,
    /// M4d, WP11 — nakładki **ruchu**. W odróżnieniu od wszystkich powyżej nie liczą
    /// się z danych generatora, tylko z **bieżącej minuty symulacji**, więc budują się
    /// z podwójnie buforowanego zrzutu (`TrafficOverlay`), a nie z `Terrain`.
    Natezenie,
    Korki,
    Izochrona,
    Parkingi,
    ObciazenieLinii,
    /// M9c, WP7 — pozostałe **nakładki danych** z PRD §14.2. Liczy je `game::overlays`,
    /// bo pytają o mieszkańców, rynek i zakłady naraz; klient je tylko przełącza
    /// i rysuje legendę.
    DochodGD,
    ZasiegSklepu,
    CenaTowaru,
    Bezrobocie,
    Zdrowie,
    Zanieczyszczenie,
    PrzeplywTowaru,
}

impl Nakladka {
    /// Kolejność przełączania klawiszem `F3`.
    const KOLEJNOSC: [Nakladka; 23] = [
        Nakladka::Brak,
        Nakladka::Wysokosc,
        Nakladka::Splyw,
        Nakladka::KlasaWody,
        Nakladka::Biom,
        Nakladka::TemperaturaStycznia,
        Nakladka::TemperaturaLipca,
        Nakladka::Opady,
        Nakladka::Geologia,
        Nakladka::Zloza,
        Nakladka::WartoscGruntu,
        Nakladka::Natezenie,
        Nakladka::Korki,
        Nakladka::Izochrona,
        Nakladka::Parkingi,
        Nakladka::ObciazenieLinii,
        Nakladka::DochodGD,
        Nakladka::ZasiegSklepu,
        Nakladka::CenaTowaru,
        Nakladka::Bezrobocie,
        Nakladka::Zdrowie,
        Nakladka::Zanieczyszczenie,
        Nakladka::PrzeplywTowaru,
    ];

    #[must_use]
    pub fn nastepna(self) -> Nakladka {
        let i = Self::KOLEJNOSC.iter().position(|n| *n == self).unwrap_or(0);
        Self::KOLEJNOSC[(i + 1) % Self::KOLEJNOSC.len()]
    }

    /// Nakładka po kluczu z wiersza poleceń. Klucze są angielskie (klucze `data/` i CLI
    /// są po angielsku — CLAUDE.md), nazwy wyświetlane po polsku.
    #[must_use]
    pub fn z_klucza(k: &str) -> Option<Nakladka> {
        Some(match k {
            "none" => Nakladka::Brak,
            "height" => Nakladka::Wysokosc,
            "flow" => Nakladka::Splyw,
            "water" => Nakladka::KlasaWody,
            "biome" => Nakladka::Biom,
            "temp-jan" => Nakladka::TemperaturaStycznia,
            "temp-jul" => Nakladka::TemperaturaLipca,
            "precip" => Nakladka::Opady,
            "geology" => Nakladka::Geologia,
            "deposits" => Nakladka::Zloza,
            "land-value" => Nakladka::WartoscGruntu,
            "traffic-flow" => Nakladka::Natezenie,
            "congestion" => Nakladka::Korki,
            "isochrone" => Nakladka::Izochrona,
            "parking" => Nakladka::Parkingi,
            "transit-load" => Nakladka::ObciazenieLinii,
            "household-income" => Nakladka::DochodGD,
            "shop-catchment" => Nakladka::ZasiegSklepu,
            "product-price" => Nakladka::CenaTowaru,
            "unemployment" => Nakladka::Bezrobocie,
            "health" => Nakladka::Zdrowie,
            "pollution" => Nakladka::Zanieczyszczenie,
            "good-flow" => Nakladka::PrzeplywTowaru,
            _ => return None,
        })
    }

    #[must_use]
    pub fn nazwa(self) -> &'static str {
        match self {
            Nakladka::Brak => "brak",
            Nakladka::Wysokosc => "wysokość",
            Nakladka::Splyw => "akumulacja spływu",
            Nakladka::KlasaWody => "klasa wody",
            Nakladka::Biom => "biom",
            Nakladka::TemperaturaStycznia => "temperatura stycznia",
            Nakladka::TemperaturaLipca => "temperatura lipca",
            Nakladka::Opady => "opady",
            Nakladka::Geologia => "warstwa geologiczna 4 m pod powierzchnią",
            Nakladka::Zloza => "złoża",
            // ponytail: nazwa z tabeli w kodzie, bo `engine/ui` i `data/locale/` powstają
            // w M3 — plik `data/ui/overlays.ron` niesie już `loc_key`, pod który podłączy
            // się tamta faza (CLAUDE.md, „Język i lokalizacja").
            Nakladka::WartoscGruntu => "wartość gruntu",
            Nakladka::Natezenie => "natężenie ruchu",
            Nakladka::Korki => "korki",
            Nakladka::Izochrona => "czas dojazdu",
            Nakladka::Parkingi => "obłożenie parkingów",
            Nakladka::ObciazenieLinii => "obciążenie linii",
            // Dziewięć nakładek danych z §14.2 ma nazwy w `data/locale/`
            // (`ui.overlay.<klucz>`) i to one trafiają do legendy; ta tabela zostaje
            // dla konsoli i dla nakładek terenu, których gracz nie widzi w HUD-zie.
            Nakladka::DochodGD => "dochód gospodarstw",
            Nakladka::ZasiegSklepu => "zasięg sklepu",
            Nakladka::CenaTowaru => "cena produktu",
            Nakladka::Bezrobocie => "bezrobocie",
            Nakladka::Zdrowie => "zdrowie",
            Nakladka::Zanieczyszczenie => "zanieczyszczenie",
            Nakladka::PrzeplywTowaru => "przepływ towaru",
        }
    }

    /// Czy to jest jedna z dziewięciu nakładek danych z §14.2.
    ///
    /// Osobno od [`Nakladka::pole_danych`], bo tamta zwraca `None` także wtedy, gdy
    /// nakładka jest danymi, ale brakuje jej przedmiotu (sklepu do zasięgu). Bez tego
    /// rozróżnienia zasięg bez wybranego sklepu wpadałby w gałąź nakładek terenu.
    #[must_use]
    pub const fn jest_danymi(self) -> bool {
        matches!(
            self,
            Nakladka::WartoscGruntu
                | Nakladka::Natezenie
                | Nakladka::DochodGD
                | Nakladka::ZasiegSklepu
                | Nakladka::CenaTowaru
                | Nakladka::Bezrobocie
                | Nakladka::Zdrowie
                | Nakladka::Zanieczyszczenie
                | Nakladka::PrzeplywTowaru
        )
    }

    /// Pole danych z PRD §14.2, jeśli ta nakładka nim jest.
    ///
    /// Dziewięć pozycji listy liczy `game::overlays`, bo pytają naraz o mieszkańców,
    /// rynek, zakłady i miasto. `site` i `good` są parametrami dwóch z nich: zasięg
    /// dotyczy **konkretnego** sklepu, a cena i przepływ **konkretnego** towaru —
    /// nakładka bez wskazanego przedmiotu nie ma o co zapytać.
    #[must_use]
    pub fn pole_danych(
        self,
        site: Option<magnat_core::SiteId>,
        good: magnat_core::GoodId,
    ) -> Option<magnat_game::OverlayField> {
        use magnat_game::OverlayField as F;
        Some(match self {
            Nakladka::WartoscGruntu => F::LandValue,
            Nakladka::Natezenie => F::Traffic,
            Nakladka::DochodGD => F::HouseholdIncome,
            Nakladka::ZasiegSklepu => F::ShopCatchment { site: site? },
            Nakladka::CenaTowaru => F::ProductPrice { good },
            Nakladka::Bezrobocie => F::Unemployment,
            Nakladka::Zdrowie => F::Health,
            Nakladka::Zanieczyszczenie => F::Pollution,
            Nakladka::PrzeplywTowaru => F::GoodFlow { good },
            _ => return None,
        })
    }

    /// Pole ruchu, które ta nakładka rysuje — `None` dla nakładek terenu i miasta.
    ///
    /// Nakładki ruchu przechodzą osobną ścieżką, bo ich źródłem jest **symulacja
    /// w tej minucie**, a nie dane generatora: `zbuduj` dostaje `Terrain` i `CityData`,
    /// a te o korku nie wiedzą nic.
    #[must_use]
    pub fn pole_ruchu(self) -> Option<magnat_traffic::TrafficField> {
        use magnat_traffic::TrafficField as F;
        Some(match self {
            Nakladka::Natezenie => F::Flow,
            Nakladka::Korki => F::Congestion,
            Nakladka::Izochrona => F::Isochrone,
            Nakladka::Parkingi => F::Parking,
            Nakladka::ObciazenieLinii => F::TransitLoad,
            _ => return None,
        })
    }

    /// Czy nakładka jest **kategoryczna** (paleta to zbiór barw, nie gradient).
    fn kategoryczna(self) -> bool {
        matches!(
            self,
            Nakladka::KlasaWody | Nakladka::Biom | Nakladka::Geologia | Nakladka::Zloza
        )
    }
}

/// Pole i paleta gotowe do wysłania na GPU.
pub struct Pole {
    pub dim: u32,
    pub cell_m: f32,
    pub values: Vec<u8>,
    pub palette: [[u8; 4]; 256],
}

/// Buduje pole dla wybranej nakładki z danych świata.
///
/// Wszystko liczy się z **już policzonych** danych generatora (`WorldData`), a nie z zapytań
/// punktowych: pole ma milion komórek, a zapytania liczą szum i interpolację na każdą z nich.
#[must_use]
pub fn zbuduj(terrain: &Terrain, city: Option<&CityData>, co: Nakladka) -> Option<Pole> {
    if co == Nakladka::Brak {
        return None;
    }
    if co == Nakladka::WartoscGruntu {
        return wartosc_gruntu(city?);
    }
    // Ruch ma własne źródło (zrzut symulacji), więc tutaj kończy się cicho: wołający
    // buduje go przez `Citizens::pole_ruchu`, bo tylko on trzyma świat. Tak samo
    // nakładki danych z §14.2 — te liczy `game::overlays`.
    if co.pole_ruchu().is_some() || co.jest_danymi() {
        return None;
    }
    let dane = terrain.data();
    let dim = dane.height.dim();
    let cdim = dane.climate.dim();
    let na_klimat = (magnat_world::CLIMATE_CELL_M / magnat_world::WORK_CELL_M) as usize;
    let mut values = vec![0u8; dim * dim];

    // Skala wysokości i opadów wyprowadzana z faktycznego zakresu mapy, nie ze stałej:
    // nakładka ma pokazywać kontrast tego świata, a nie tego, który ktoś miał na myśli.
    let (h_min, h_max) = dane
        .height
        .as_slice()
        .iter()
        .fold((i16::MAX, i16::MIN), |(a, b), h| (a.min(*h), b.max(*h)));
    let rozpietosc = f32::from(h_max - h_min).max(1.0);

    for gy in 0..dim {
        for gx in 0..dim {
            let i = gy * dim + gx;
            let c = (gy / na_klimat).min(cdim - 1) * cdim + (gx / na_klimat).min(cdim - 1);
            values[i] = match co {
                Nakladka::Brak => 0,
                Nakladka::Wysokosc => {
                    (f32::from(dane.height[i] - h_min) / rozpietosc * 255.0) as u8
                }
                // **Korekta wobec §1:** sama akumulacja spływu nie jest stanem trwałym —
                // cztery bajty na komórkę to 64 MB na mapie 16 km, czyli więcej niż cały
                // budżet stanu z §5.9. Nakładka pokazuje więc jej pochodną, którą generator
                // zachowuje: rząd Strahlera koryta. Ten sam podział zlewni, inna jednostka.
                Nakladka::Splyw => dane
                    .river_cell(i)
                    .map_or(0, |r| (u16::from(r.strahler) * 28).min(255) as u8),
                Nakladka::KlasaWody => match dane.water[i].class() {
                    WaterClass::Dry => 0,
                    WaterClass::River => 1,
                    WaterClass::Lake => 2,
                    WaterClass::Sea => 3,
                },
                Nakladka::Biom => dane.climate[c].biome as u8,
                Nakladka::TemperaturaStycznia => {
                    skala_temperatury(dane.climate[c].temp_monthly_dc[0])
                }
                Nakladka::TemperaturaLipca => skala_temperatury(dane.climate[c].temp_monthly_dc[6]),
                Nakladka::Opady => {
                    let rocznie: u32 = dane.climate[c]
                        .precip_monthly_mm
                        .iter()
                        .map(|m| u32::from(*m))
                        .sum();
                    (rocznie * 255 / 2500).min(255) as u8
                }
                Nakladka::Geologia => {
                    let (x, y) = (
                        (gx * magnat_world::WORK_CELL_M as usize) as i32,
                        (gy * magnat_world::WORK_CELL_M as usize) as i32,
                    );
                    warstwa_pod_powierzchnia(terrain, x, y)
                }
                // Nakładki miejskie i ruchu mają własne ścieżki i nie dochodzą tutaj;
                // wyliczamy je mimo to, bo `K-12` żąda wyczerpującego `match` — brak
                // ramienia ma łamać kompilację, a nie rysować pustą mapę.
                Nakladka::Zloza
                | Nakladka::WartoscGruntu
                | Nakladka::Natezenie
                | Nakladka::Korki
                | Nakladka::Izochrona
                | Nakladka::Parkingi
                | Nakladka::ObciazenieLinii
                | Nakladka::DochodGD
                | Nakladka::ZasiegSklepu
                | Nakladka::CenaTowaru
                | Nakladka::Bezrobocie
                | Nakladka::Zdrowie
                | Nakladka::Zanieczyszczenie
                | Nakladka::PrzeplywTowaru => 0,
            };
        }
    }

    if co == Nakladka::Zloza {
        zaznacz_zloza(terrain, dim, &mut values);
    }

    Some(Pole {
        dim: dim as u32,
        cell_m: magnat_world::WORK_CELL_M as f32,
        values,
        palette: paleta(co),
    })
}

/// Nakładka wartości gruntu (M2e, WP16). Raster i paleta pochodzą z `sim/world`, więc
/// klient nie zna ani progów, ani jednostki — zna je plik danych.
fn wartosc_gruntu(city: &CityData) -> Option<Pole> {
    let tab = match magnat_world::OverlayTable::load() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("nakładka wartości gruntu: {e}");
            return None;
        }
    };
    let spec = tab.get("land_value").ok()?;
    let (dim, cell_m, values) = magnat_world::city::overlay::land_value_raster(city, spec);
    // Legenda na konsolę — barwa bez jednostki nie jest informacją (kryterium WP16).
    eprintln!(
        "legenda wartości gruntu ({}): {}",
        spec.unit,
        spec.legend()
            .iter()
            .map(|(_, v)| format!("{:.0}", *v as f64 / 100.0))
            .collect::<Vec<_>>()
            .join(" · ")
    );
    Some(Pole {
        dim,
        cell_m,
        values,
        palette: spec.palette(),
    })
}

/// Temperatura na skali −30…+40 °C — zakres, w którym mieszczą się wszystkie regiony.
/// Wejście w dziesiątych stopnia, bo tak trzyma ją `ClimateCell` (00 §2: bez floatów w stanie).
fn skala_temperatury(t_dc: i16) -> u8 {
    (((i32::from(t_dc) + 300) * 255 / 700).clamp(0, 255)) as u8
}

/// Materiał cztery metry pod powierzchnią — głębokość fundamentu, czyli to, co interesuje
/// M2 przy stawianiu budynków.
fn warstwa_pod_powierzchnia(terrain: &Terrain, x: i32, y: i32) -> u8 {
    use magnat_world::TerrainQuery;
    let c = terrain.column_at(x, y);
    // 8 = cztery metry w jednostkach 0,5 m.
    let m = c
        .material_at(c.surface_z - 8)
        .unwrap_or(magnat_voxel::MaterialId::AIR);
    // Indeks materiału jako indeks palety: materiałów jest kilkanaście, paleta ma 256 miejsc.
    (m.0 & 0xFF) as u8
}

/// Zaznacza komórki, nad którymi leży złoże.
///
/// Najpierw prostokąt otaczający kształt, dopiero w nim test punktowy. Bez tego przejścia
/// nakładka kosztuje `liczba złóż × cała siatka` — przy sześciuset złożach i mapie 8 km
/// to miliardy testów i kilkadziesiąt sekund zawieszenia okna. Sam prostokąt nie wystarcza,
/// bo pokłady są polilinią, a warstwy wodonośne wielokątem: prostokąt pokazywałby złoże
/// wszędzie tam, gdzie go nie ma, a to jest mapa do szukania.
fn zaznacz_zloza(terrain: &Terrain, dim: usize, values: &mut [u8]) {
    use magnat_core::IVec3;
    use magnat_world::{DepositShape, TerrainQuery};
    let cell = magnat_world::WORK_CELL_M as i32;

    for d in &terrain.data().deposits {
        let (min, max) = match &d.shape {
            DepositShape::Ellipsoid { center, radii, .. }
            | DepositShape::Trap { center, radii, .. } => (
                (center.x - radii.x, center.y - radii.y),
                (center.x + radii.x, center.y + radii.y),
            ),
            DepositShape::Seam { polyline, .. } => obwiednia(polyline),
            DepositShape::Aquifer { poly, .. } => obwiednia(poly),
        };
        let (gx0, gx1) = (
            (min.0 / cell).max(0) as usize,
            ((max.0 / cell) as usize).min(dim - 1),
        );
        let (gy0, gy1) = (
            (min.1 / cell).max(0) as usize,
            ((max.1 / cell) as usize).min(dim - 1),
        );
        for gy in gy0..=gy1 {
            for gx in gx0..=gx1 {
                let (x, y) = ((gx as i32) * cell, (gy as i32) * cell);
                // Sprawdzamy strop złoża: pod nim jest już tylko więcej tego samego.
                let z = terrain.height_at(x, y) * 5 - i32::from(d.depth_top_m) * 10 - 1;
                if d.shape.contains(IVec3::new(x, y, z)) {
                    // Rodzaj surowca jako indeks palety, przesunięty o jeden — zero zostaje
                    // dla „bez złoża".
                    values[gy * dim + gx] = d.resource as u8 + 1;
                }
            }
        }
    }
}

/// Prostokąt otaczający polilinię albo wielokąt, w metrach.
fn obwiednia(punkty: &[magnat_core::IVec2]) -> ((i32, i32), (i32, i32)) {
    punkty.iter().fold(
        ((i32::MAX, i32::MAX), (i32::MIN, i32::MIN)),
        |((x0, y0), (x1, y1)), p| ((x0.min(p.x), y0.min(p.y)), (x1.max(p.x), y1.max(p.y))),
    )
}

/// Paleta dla nakładki: gradient dla wielkości ciągłych, zestaw barw dla kategorii.
fn paleta(co: Nakladka) -> [[u8; 4]; 256] {
    let mut p = [[0u8, 0, 0, 255]; 256];
    if co.kategoryczna() {
        // Barwy kategorii: pierwsza jest przezroczysta, bo „brak" ma nie zamalowywać terenu.
        const BARWY: [[u8; 4]; 12] = [
            [0, 0, 0, 0],
            [60, 120, 220, 255],
            [40, 80, 200, 255],
            [20, 40, 160, 255],
            [220, 200, 60, 255],
            [220, 120, 40, 255],
            [180, 60, 60, 255],
            [140, 60, 180, 255],
            [60, 180, 120, 255],
            [200, 200, 200, 255],
            [120, 80, 40, 255],
            [240, 120, 200, 255],
        ];
        for (i, v) in p.iter_mut().enumerate() {
            *v = BARWY[i % BARWY.len()];
        }
        // Biom wodny i „brak" mają zostać przezroczyste tylko dla indeksu zero.
        p[0] = [0, 0, 0, 0];
        return p;
    }
    // Gradient „turbo-podobny": granat → turkus → zieleń → żółć → czerwień. Czytelny
    // także dla osób z deuteranopią, w przeciwieństwie do klasycznego zielono-czerwonego.
    for (i, v) in p.iter_mut().enumerate() {
        let t = i as f32 / 255.0;
        let r = (255.0 * (1.5 * t - 0.35).clamp(0.0, 1.0)) as u8;
        let g = (255.0 * (1.0 - (t - 0.5).abs() * 1.8).clamp(0.0, 1.0)) as u8;
        let b = (255.0 * (1.0 - 1.8 * t).clamp(0.0, 1.0)) as u8;
        *v = [r, g, b, 255];
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f3_obchodzi_wszystkie_nakladki_i_wraca_do_braku() {
        let mut n = Nakladka::Brak;
        let mut widziane = Vec::new();
        for _ in 0..Nakladka::KOLEJNOSC.len() {
            n = n.nastepna();
            widziane.push(n);
        }
        assert_eq!(n, Nakladka::Brak, "cykl nie domyka się na „braku”");
        assert_eq!(
            widziane.len(),
            Nakladka::KOLEJNOSC.len(),
            "cykl pomija nakładki"
        );
        // Wszystkie osiem podglądów z §1 musi być w cyklu — to jest treść kryterium R6.
        for wymagana in [
            Nakladka::Wysokosc,
            Nakladka::Splyw,
            Nakladka::KlasaWody,
            Nakladka::Biom,
            Nakladka::TemperaturaStycznia,
            Nakladka::TemperaturaLipca,
            Nakladka::Opady,
            Nakladka::Geologia,
            Nakladka::Zloza,
            Nakladka::WartoscGruntu,
        ] {
            assert!(widziane.contains(&wymagana), "brak nakładki {wymagana:?}");
        }
    }

    #[test]
    fn paleta_kategoryczna_zostawia_zero_przezroczyste() {
        // Indeks zero znaczy „nic tu nie ma" — zamalowanie go barwą zasłoniłoby teren
        // wszędzie tam, gdzie nakładka nie ma nic do powiedzenia.
        assert_eq!(paleta(Nakladka::KlasaWody)[0][3], 0);
        assert!(paleta(Nakladka::KlasaWody)[1][3] > 0);
        // Gradient jest nieprzezroczysty na całej długości.
        assert!(paleta(Nakladka::Wysokosc).iter().all(|b| b[3] == 255));
    }
}
