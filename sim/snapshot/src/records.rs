//! Rekordy encji w snapshocie (M11a §5.2).
//!
//! Wszystkie są `#[repr(C)]`, `Copy` i bez wskaźników — mają iść do bufora GPU bez
//! przepakowywania. Rozmiary są **kontraktem** i pilnuje ich test na końcu pliku:
//! rachunek pasma z §5.2 (1,30 MB na bufor, 37 MB/s przy 30 Hz) jest funkcją tych liczb,
//! a nie deklaracją.
//!
//! ### Kolejność pól nie jest kolejnością z dokumentu i to jest poprawka, nie swoboda
//!
//! §5.2 wypisuje pola w kolejności czytelnej dla człowieka, a ta zostawia w strukturze
//! dziury wyrównania: `VehicleRenderRec` wychodzi z niej 44 B zamiast obiecanych 40,
//! a `SiteRenderRec` 28 zamiast 24. Kolejność jest tu więc ułożona **od najszerszego
//! pola**, a rozmiar wychodzi dokładnie taki, jaki obiecuje rachunek pasma. Znaczenie
//! ani jedno pole nie zmieniło.

/// Ile dzielnic mieści tablica zasilania. 64 — tyle, ile `PowerRec` w §5.2.
pub const MAX_DISTRICTS: usize = 64;

// ── Mieszkaniec ─────────────────────────────────────────────────────────────────

/// Mieszkaniec siedzi w pojeździe — bryła pieszego się nie rysuje, sylwetka jedzie
/// w kabinie (`VehicleRenderRec::occupants`).
pub const CITIZEN_FLAG_IN_VEHICLE: u8 = 1 << 0;
/// Mieszkaniec jest wewnątrz budynku — widoczny dopiero po cięciu poziomami (M11c).
pub const CITIZEN_FLAG_IN_BUILDING: u8 = 1 << 1;
/// Podświetlony przez filtr encji albo zaznaczenie gracza (§14.2).
pub const CITIZEN_FLAG_HIGHLIGHTED: u8 = 1 << 2;
/// Pracownik albo klient zakładu gracza.
pub const CITIZEN_FLAG_PLAYER_OWNED: u8 = 1 << 3;

/// Mieszkaniec w kadrze — 32 B.
///
/// Pozycja jest w **milimetrach świata** jako `i32` (zasięg ±2147 km), a nie w metrach
/// jako `f32`: przeliczenie na układ kamery robi render, odejmując origin w `f64`
/// **przed** rzutowaniem na `f32` (§5.3). Stała działka całkowita nie ma tu nic wspólnego
/// z zakazem floatów z 00 §2 — snapshot nie jest księgowością; chodzi o to, żeby pieszy
/// przy współrzędnej 8 km nie skakał o pół metra między klatkami.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct CitizenRenderRec {
    /// Pozycja w milimetrach świata. Z jest osią pionową.
    pub pos: [i32; 3],
    /// Dolne 32 bity `Entity` — picking i stabilność instancji między klatkami.
    pub entity_lo: u32,
    /// Wariant wyglądu — rozkład bitów w [`crate::Appearance`].
    pub appearance: u32,
    /// Obrót wokół osi pionowej w 1/65536 obrotu.
    pub yaw: u16,
    /// `DistrictId` — paleta dzielnicy i przynależność do `AmbientZone`.
    pub district: u16,
    /// `ClipId` — co postać robi (M11b §5.4).
    pub anim_state: u8,
    /// Faza 0..255 w klipie.
    pub anim_phase: u8,
    /// Co niesie — [`crate::Carry`].
    pub carry: u8,
    /// `CITIZEN_FLAG_*`.
    pub flags: u8,
    pub _pad: [u8; 4],
}

// ── Pojazd ──────────────────────────────────────────────────────────────────────

pub const VEHICLE_FLAG_LIGHTS: u8 = 1 << 0;
pub const VEHICLE_FLAG_BLINKER: u8 = 1 << 1;
pub const VEHICLE_FLAG_ENGINE_ON: u8 = 1 << 2;
pub const VEHICLE_FLAG_PLAYER_OWNED: u8 = 1 << 3;
/// Podświetlony przez filtr widoku albo zaznaczenie gracza (§14.2).
///
/// **Inny bit niż u mieszkańca i to nie jest przeoczenie:** oba zestawy flag są
/// rozłączne, bit 2 znaczy u pojazdu „silnik pracuje", a u mieszkańca „podświetlony".
/// Jedna funkcja licząca podświetlenie dla obu zestawów malowałaby na żółto każde auto
/// z pracującym silnikiem — i nikt by nie wiedział, dlaczego.
pub const VEHICLE_FLAG_HIGHLIGHTED: u8 = 1 << 4;

/// Pojazd w kadrze — 40 B.
///
/// `paint` rozwiązuje wprost wymaganie PRD §16.3: *„auto należy do konkretnego GD i ma
/// swój kolor"*. Wypełniacz liczy go z indeksu gospodarstwa, render podstawia barwę pod
/// slot `Paint`. Zero duplikacji modeli: 48 modeli × 64 lakiery = 3072 wizualnie różnych
/// aut z 48 siatek.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct VehicleRenderRec {
    /// Pozycja w milimetrach świata.
    pub pos: [i32; 3],
    pub entity_lo: u32,
    pub yaw: u16,
    /// Nachylenie (wzniesienie, rampa) w 1/65536 obrotu ze znakiem.
    pub pitch: i16,
    /// `VehicleModelId` — marka i klasa (§15.1).
    pub model: u16,
    /// Indeks lakieru należący do gospodarstwa właściciela.
    pub paint: u16,
    /// Oklejenie firmowe → `SignAtlas`; 0 = brak.
    pub livery: u16,
    pub district: u16,
    /// Jedzie | Parkuje | Stoi | Tankuje | Rozładunek | Awaria.
    pub anim_state: u8,
    pub anim_phase: u8,
    pub wheel_phase: u8,
    /// Wypełnienie 0..255 → widoczny ładunek na pace.
    pub load: u8,
    /// `VEHICLE_FLAG_*`.
    pub flags: u8,
    /// Ile sylwetek w kabinie.
    pub occupants: u8,
    pub _pad: [u8; 6],
}

// ── Zakład ──────────────────────────────────────────────────────────────────────

/// Awaria, nie bezczynność. Oba stany dają `activity == 0`, ale zakład bez zmiany jest
/// w porządku, a zakład zepsuty kosztuje pieniądze co minutę (§5.2).
pub const SITE_FAULT: u8 = 1 << 0;

/// Zakład albo budynek aktywny — 24 B.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct SiteRenderRec {
    pub entity_lo: u32,
    /// Punkt referencyjny w milimetrach: komin, rampa albo wejście.
    pub pos: [i32; 3],
    /// Kafel w `SignAtlas` — nazwa firmy gracza (M11c/WP9).
    pub sign_id: u16,
    /// 0..255. **0 = linia stoi** (cisza i bezruch maszyn), nie „brak danych".
    pub activity: u8,
    /// 0..255 — gęstość dymu z komina, skala absolutna (§15.3).
    pub emission: u8,
    /// 0..255 — jasność okien; 0 przy blackoucie.
    pub lights: u8,
    /// Rodzaj łoża dźwiękowego emitera (M11d §5.9).
    pub ambient_kind: u8,
    /// 0..255 — wypełnienie regałów dla `InteriorKit` (M11c §5.7).
    pub stock_fill: u8,
    /// Bit 0: [`SITE_FAULT`]; bity 1–2: rodzaj pióropusza (`PlumeKind`).
    pub flags: u8,
}

impl SiteRenderRec {
    /// Rodzaj pióropusza: 0 = brak, 1 = para, 2 = sadza, 3 = chemiczny (§5.2).
    #[must_use]
    pub const fn plume_kind(&self) -> u8 {
        (self.flags >> 1) & 0b11
    }

    #[must_use]
    pub const fn is_faulted(&self) -> bool {
        self.flags & SITE_FAULT != 0
    }
}

// ── Tłum, pogoda, zasilanie, gracz ──────────────────────────────────────────────

/// Gęstość tłumu na krawędzi grafu pieszego — 8 B. Tu trafiają mieszkańcy, którzy
/// wypadli poza cap: **nie znikają ze świata**, tylko przestają mieć własną bryłę.
///
/// `edge` jest `u32`, a nie `u16` jak w §5.2. Graf pieszy metropolii ma dziesiątki
/// tysięcy krawędzi i zapas w `u16` kończy się cicho — tłum przypisany do złej ulicy
/// wygląda dokładnie tak samo jak tłum przypisany do właściwej. Cena: 8 KB.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct CrowdDensityRec {
    pub edge: u32,
    pub count: u16,
    /// Rodzaj tłumu — pieszy, kolejka, zgromadzenie.
    pub kind: u8,
    pub _pad: u8,
}

/// Pogoda widoczna — 12 B, jeden rekord na świat. Model mieszka w `sim/events` (M8),
/// tutaj jest wyłącznie to, co widać i słychać.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct WeatherState {
    /// Natężenie opadu 0..255.
    pub precipitation: u8,
    /// Deszcz | śnieg | grad.
    pub kind: u8,
    pub temp_c: i8,
    /// Wiatr w m/s, składowe X i Y.
    pub wind: [i8; 2],
    pub fog_density: u8,
    pub cloud: u8,
    /// `Season` z `engine/core` — rok 360 dób, sezon to równe 90 (`K-1`, `K-63`).
    pub season: u8,
    pub snow_cover: u8,
    /// Ułamek światła dziennego 0..255 — napędza paletę i zapalanie okien.
    pub daylight: u8,
    pub _pad: [u8; 2],
}

/// Zasilanie dzielnicy — 1 B. `supply_ratio < 128` to blackout (M8b).
///
/// §5.2 dawał tu jeszcze pole `district: u16` i liczył rekord na 3 B. Pole jest
/// **redundantne i niewykonalne naraz**: tablica jest indeksowana dzielnicą, więc
/// `power[i].district` może być wyłącznie `i`, a przy `u16` obok `u8` wyrównanie robi
/// z rekordu 4 B i z tablicy 256 B zamiast obiecanych 192. Zostaje jedna liczba, indeks
/// jest tożsamością, a tablica waży 64 B.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct PowerRec {
    pub supply_ratio: u8,
}

/// Postać gracza i stan jego finansów — 24 B.
///
/// Finanse są tu, bo `MusicDirector` (M11d §5.9) reaguje na nie, a `engine/audio` nie ma
/// prawa widzieć księgi. Dwie liczby zamiast salda z rozmysłu: muzyka ma reagować na
/// **sytuację**, a nie na kwotę, a kwota i tak nie przeszłaby przez `u8`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[repr(C)]
pub struct PlayerViewRec {
    /// `entity_lo` postaci gracza — kotwica trybu pierwszoosobowego (M11c §5.7).
    pub citizen: u32,
    /// Pozycja oka w milimetrach świata.
    pub eye: [i32; 3],
    pub yaw: u16,
    pub pitch: i16,
    /// Płynność / 30-dniowe koszty stałe, przycięte do 0..255.
    pub liquidity_ratio: u8,
    /// Kierunek zysku, -128..127.
    pub profit_trend: i8,
    pub _pad: [u8; 2],
}

// ── Rekordy warstwy Mikro (M3d, M4d) ────────────────────────────────────────────

/// Światło punktowe w klatce. 20 B na rekord, 80 KB na pełny cap.
///
/// Kształt jest **M1 i taki zostaje**: ma działającego konsumenta (`clusters::to_gpu`)
/// i mieści się w tych samych 20 B, co wersja z §5.2. `kind` i `flags` z tamtej wersji
/// dołoży M11d razem z pierwszym czytelnikiem — pole bez czytelnika wygląda w danych
/// tak samo jak działające.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
#[repr(C)]
pub struct LightRecord {
    /// Pozycja w metrach, względem początku świata.
    pub pos: [f32; 3],
    /// Zasięg w metrach.
    pub range: f32,
    /// Barwa i natężenie spakowane w RGBE.
    pub color_rgbe: u32,
}

/// Pieszy w warstwie Mikro — rekord **ruchu**, nie wyglądu (M3d, `Z-1`).
///
/// Zna pozycję i tożsamość, nie zna zawodu ani ubrania. Złączenie go z wyglądem
/// w [`CitizenRenderRec`] robi wypełniacz w `magnat_game::view`, bo tylko on widzi
/// naraz ruch i komponenty mieszkańca.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
#[repr(C)]
pub struct PedestrianRecord {
    /// Pozycja w metrach, względem początku świata. Z jest osią pionową.
    pub pos: [f32; 3],
    /// Indeks encji mieszkańca — to samo, co czyta bufor ID przy kliknięciu.
    pub entity: u32,
}

/// Jeden pojazd w LOD Mikro (M4d/WP8) — ten sam kanał i ta sama zasada co pieszy.
///
/// `heading` jest tu, a nie liczony w rendererze, bo kierunek wynika z osi krawędzi,
/// którą zna wyłącznie warstwa ruchu. `class` to `VehicleClassId` — model bryły dobierze
/// M11, tu jest tylko indeks katalogu.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
#[repr(C)]
pub struct VehicleRecord {
    /// Pozycja w metrach, Z pionowo — jak u pieszego.
    pub pos: [f32; 3],
    /// Kurs w radianach, 0 = oś +X.
    pub heading: f32,
    /// Indeks encji pojazdu. Kurs komunikacji, który nie ma encji pojazdu, niesie
    /// tu indeks swojego taboru — bufor ID i tak wskazuje wtedy na linię, nie na osobę.
    pub entity: u32,
    /// `VehicleClassId` z `data/vehicles/classes.ron`.
    pub class: u8,
    /// Pas liczony od prawej krawędzi jezdni.
    pub lane: u8,
    pub _pad: [u8; 2],
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;

    /// Rachunek pasma z §5.2 jest funkcją tych liczb. Zmiana rozmiaru rekordu zmienia
    /// budżet 1,30 MB i 37 MB/s, więc nie jest szczegółem implementacji.
    #[test]
    fn rozmiary_rekordow_sa_kontraktem() {
        assert_eq!(size_of::<CitizenRenderRec>(), 32);
        assert_eq!(size_of::<VehicleRenderRec>(), 40);
        assert_eq!(size_of::<SiteRenderRec>(), 24);
        assert_eq!(size_of::<CrowdDensityRec>(), 8);
        assert_eq!(size_of::<WeatherState>(), 12);
        assert_eq!(size_of::<PowerRec>(), 1);
        assert_eq!(size_of::<PlayerViewRec>(), 24);
        assert_eq!(size_of::<LightRecord>(), 20);
    }

    #[test]
    fn ponumerowanie_pioropusza_i_awarii_nie_koliduje() {
        let s = SiteRenderRec {
            flags: SITE_FAULT | (3 << 1),
            ..Default::default()
        };
        assert!(s.is_faulted());
        assert_eq!(s.plume_kind(), 3);
        let czysty = SiteRenderRec {
            flags: 2 << 1,
            ..Default::default()
        };
        assert!(!czysty.is_faulted());
        assert_eq!(czysty.plume_kind(), 2);
    }
}
