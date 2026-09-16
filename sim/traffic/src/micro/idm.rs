//! Parametry IDM/MOBIL i fizyka car-followingu (M4d §5.4).
//!
//! Wydzielone z `micro.rs` w R-WP10 bez zmiany zachowania. `przyspieszenie`
//! i `pozadana` zostają metodami `VehicleBuffer` — czytają `self.vehs` i `self.paths`,
//! więc wyprowadzenie ich do funkcji wolnych byłoby zmianą sygnatury, nie przeniesieniem.

use super::*;

// ── parametry IDM/MOBIL ─────────────────────────────────────────────────────────

/// Parametry car-followingu i zmiany pasa z `data/roads/idm.ron`.
///
/// **To są parametry animacji.** Nie wchodzą do żadnego wzoru pieniężnego ani czasowego;
/// ich jedyne sprzężenie z ekonomią jest offline, przez `balansator calibrate-vdf`.
#[derive(Clone, Copy, PartialEq, Debug, Deserialize)]
pub struct IdmParams {
    pub schema_version: u32,
    pub s0_cm: u32,
    pub headway_ds: u32,
    pub accel_cms2: u32,
    pub decel_cms2: u32,
    pub delta: u32,
    pub politeness_permille: u32,
    pub threshold_cms2: u32,
    pub safe_decel_cms2: u32,
    pub servo_min_permille: u32,
    pub servo_max_permille: u32,
    pub substep_ms: u32,
    pub max_substeps: u32,
}

pub const IDM_SCHEMA_VERSION: u32 = 1;

impl Default for IdmParams {
    /// Wartości z pliku danych powielone w kodzie **wyłącznie** dla testów jednostkowych,
    /// które nie mają katalogu `data/`. Ścieżka produkcyjna zawsze ładuje plik.
    fn default() -> IdmParams {
        IdmParams {
            schema_version: IDM_SCHEMA_VERSION,
            s0_cm: 120,
            headway_ds: 5,
            accel_cms2: 130,
            decel_cms2: 200,
            delta: 4,
            politeness_permille: 250,
            threshold_cms2: 20,
            safe_decel_cms2: 400,
            servo_min_permille: 850,
            servo_max_permille: 1150,
            substep_ms: 500,
            max_substeps: 120,
        }
    }
}

impl IdmParams {
    pub fn load_default() -> Result<IdmParams, DataError> {
        IdmParams::load(&data_path("roads/idm.ron"))
    }

    pub fn load(path: &Path) -> Result<IdmParams, DataError> {
        let txt = std::fs::read_to_string(path)?;
        let p: IdmParams = ron::from_str(&txt).map_err(|e| DataError::Ron(e.to_string()))?;
        if p.schema_version != IDM_SCHEMA_VERSION {
            return Err(DataError::Schema {
                found: p.schema_version,
                want: IDM_SCHEMA_VERSION,
            });
        }
        // `delta` inne niż 4 byłoby cichym kłamstwem: kod liczy człon prędkości jako
        // kwadrat kwadratu, bo `powf` jest zakazane w kodzie symulacji (`K-6`).
        if p.delta != 4 {
            return Err(DataError::Empty("roads/idm.ron: delta musi być 4"));
        }
        if p.accel_cms2 == 0 || p.decel_cms2 == 0 || p.max_substeps == 0 {
            return Err(DataError::Empty("roads/idm.ron: parametr zerowy"));
        }
        Ok(p)
    }
}

impl VehicleBuffer {
    /// IDM. Bez poprzednika rolę przeszkody gra **linia wyjazdowa**: pojazd, który nie
    /// dostał jeszcze swojej minuty wyjazdu, hamuje do niej i staje — i to jest cały
    /// mechanizm kolejki przed światłem. Sygnalizacji nie liczymy tu drugi raz;
    /// czekanie wynika z `exit_cs`, które policzył `settle_node`.
    pub(super) fn przyspieszenie(
        &self,
        i: usize,
        lider: Option<usize>,
        now_cs: u64,
        p: &IdmParams,
    ) -> f32 {
        let me = &self.vehs[i];
        let dlugosc = self.paths.len_cm(me.path) as f32;
        let a = p.accel_cms2 as f32;
        let b = p.decel_cms2 as f32;
        let v = me.speed_cms.max(0.0);

        let (luka, dv) = match lider {
            Some(j) => {
                let l = &self.vehs[j];
                (l.pos_cm - f32::from(l.len_cm) - me.pos_cm, v - l.speed_cms)
            }
            None => {
                // Za linią wyjazdową jest przeszkoda tylko dopóki pojazd nie ma prawa
                // wyjechać. Po `exit_cs` mezo już go zdjęło — bryła toczy się swobodnie
                // do najbliższego zasilenia i wtedy trafia na następną krawędź.
                if now_cs >= me.exit_cs {
                    (f32::MAX / 4.0, 0.0)
                } else {
                    (dlugosc - me.pos_cm, v)
                }
            }
        };
        let s = luka.max(10.0);

        // Serwo (§5.4): pożądana prędkość to ta, przy której dystans schodzi do zera
        // dokładnie w zaksięgowanej chwili, przycięta do widełek z danych.
        let v0 = self.pozadana(me, now_cs, p);
        let ratio = (v / v0).min(4.0);
        let r2 = ratio * ratio;
        let r4 = r2 * r2; // delta == 4, więc bez `powf` (K-6)
        let s_star = p.s0_cm as f32
            + (v * (p.headway_ds as f32 / 10.0) + v * dv / (2.0 * (a * b).sqrt())).max(0.0);
        let z = s_star / s;
        (a * (1.0 - r4 - z * z)).clamp(-3.0 * b, a)
    }

    /// Pożądana prędkość po korekcie serwa.
    pub(super) fn pozadana(&self, me: &MicroVehicle, now_cs: u64, p: &IdmParams) -> f32 {
        let dlugosc = self.paths.len_cm(me.path) as f32;
        let zostalo_cm = (dlugosc - me.pos_cm).max(0.0);
        let zostalo_s = (me.exit_cs.saturating_sub(now_cs)) as f32 / 100.0;
        if zostalo_s <= 0.01 || zostalo_cm <= 1.0 {
            return me.v_free_cms;
        }
        let wymagana = zostalo_cm / zostalo_s;
        let lo = p.servo_min_permille as f32 / 1000.0;
        let hi = p.servo_max_permille as f32 / 1000.0;
        let korekta = (wymagana / me.v_free_cms).clamp(lo, hi);
        (me.v_free_cms * korekta).max(1.0)
    }
}

// ── kalibracja: IDM ↔ diagram podstawowy ────────────────────────────────────────

/// Prędkość równowagi IDM przy zadanej luce do poprzednika — **postać zamknięta z §5.4**:
/// `s_e(v) = s0 + v·T / sqrt(1 − (v/v0)^4)`, rozwiązana względem `v`.
///
/// Lewa strona rośnie monotonicznie na `[0, v0)`, więc bisekcja zbiega bez warunków
/// brzegowych i bez `powf` (`K-6`): wykładnik 4 to kwadrat kwadratu, a `sqrt` wolno.
///
/// To jest **rdzeń `calibrate-vdf`** (WP9) i powód, dla którego kalibrator nie potrzebuje
/// jednorodnego pierścienia z §5.4: pierścień służyłby wyłącznie zmierzeniu tej samej
/// równowagi, którą tu widać wprost, a bufor Mikro jest liniowy z konstrukcji — krawędź
/// ma początek i koniec, więc zawijanie pozycji byłoby kodem istniejącym tylko dla testu.
#[must_use]
pub fn equilibrium_speed_cms(gap_cm: f32, v_free_cms: f32, p: &IdmParams) -> f32 {
    let v0 = v_free_cms.max(1.0);
    let s0 = p.s0_cm as f32;
    let t = p.headway_ds as f32 / 10.0;
    if gap_cm <= s0 {
        return 0.0;
    }
    let odstep = |v: f32| {
        let r = (v / v0).min(0.999_9);
        let r2 = r * r;
        s0 + v * t / (1.0 - r2 * r2).max(1e-6).sqrt()
    };
    let (mut lo, mut hi) = (0.0f32, v0 * 0.999);
    if odstep(hi) <= gap_cm {
        return hi;
    }
    for _ in 0..48 {
        let mid = 0.5 * (lo + hi);
        if odstep(mid) <= gap_cm {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Prędkość wyprowadzona z IDM dla danego zapełnienia krawędzi, w decykilometrach
/// na godzinę — czyli w jednostce, w której `data/roads/vdf.ron` trzyma swoją tabelę.
///
/// Przy zapełnieniu `d` promili na jeden pojazd przypada `jam_spacing / d` przestrzeni,
/// z czego jego własna długość jest zajęta; reszta to luka do poprzednika.
#[must_use]
pub fn idm_speed_dkmh(
    occupancy_permille: u32,
    jam_spacing_cm: u32,
    vehicle_len_cm: u32,
    free_dkmh: u16,
    p: &IdmParams,
) -> u16 {
    let d = occupancy_permille.clamp(1, 1000) as f32;
    let przestrzen = jam_spacing_cm as f32 * 1000.0 / d;
    let luka = przestrzen - vehicle_len_cm as f32;
    // Decykilometr na godzinę to 100 000 cm / 3 600 s / 10 = 2,778 cm/s.
    let v_free = f32::from(free_dkmh) * 100.0 / 36.0;
    let v = equilibrium_speed_cms(luka, v_free, p);
    (v * 36.0 / 100.0).round().clamp(0.0, f32::from(u16::MAX)) as u16
}
