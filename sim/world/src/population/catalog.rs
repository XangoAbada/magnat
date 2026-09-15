//! Most miasto → agenci: katalog miejsc, pula lokali i etatów, fakty o mieście.
//!
//! Wydzielone z `population/mod.rs` w R-WP4 bez zmiany zachowania.

use super::*;

// ── krok 0: most między miastem a agentami ──────────────────────────────────────

/// Środek bryły budynku w centymetrach.
fn srodek(city: &CityData, building: u32) -> WorldCoord {
    let b = &city.buildings.buildings[building as usize];
    WorldCoord::new(
        ((b.aabb.min.x + b.aabb.max.x) * 50.0) as i32,
        ((b.aabb.min.y + b.aabb.max.y) * 50.0) as i32,
        (b.aabb.min.z * 100.0) as i32,
    )
}

fn dzielnica_budynku(city: &CityData, building: u32) -> u16 {
    let b = &city.buildings.buildings[building as usize];
    city.parcels
        .parcels
        .get(b.parcel.0.index() as usize)
        .map_or(0, |p| p.district.0)
}

/// Katalog miejsc (korekta E-1).
///
/// Budynek z choćby jednym mieszkaniem jest `Home`; zakład dostaje rodzaj z archetypu
/// (`data/buildings/`, pole `place_kind`), a gdy go nie ma — `Workplace`. Jeden zakład
/// ma jeden rodzaj: szkoła jest `Education` **i** miejscem pracy, ale w indeksie
/// kategorii siedzi raz, bo do pracy dociera się przez `PlaceRef`, a nie przez
/// wyszukiwanie po rodzaju.
pub(super) fn katalog_miejsc(city: &CityData) -> Vec<PlaceEntry> {
    let mut out = Vec::with_capacity(city.buildings.buildings.len() + city.sites.sites.len());
    for (i, b) in city.buildings.buildings.iter().enumerate() {
        let mieszkalny = city.buildings.units[b.units.start as usize..b.units.end as usize]
            .iter()
            .any(|u| matches!(u.kind, UnitKind::Dwelling { .. }));
        if mieszkalny {
            out.push(PlaceEntry {
                place: PlaceRef::Building(BuildingId(encja(i as u32))),
                kind: PlaceKind::Home,
                at: srodek(city, i as u32),
            });
        }
    }
    for (i, s) in city.sites.sites.iter().enumerate() {
        let kind = city
            .site_catalog
            .get(s.archetype)
            .spec
            .place_kind
            .unwrap_or(PlaceKind::Workplace);
        out.push(PlaceEntry {
            place: PlaceRef::Site(SiteId(encja(SITE_KEY_BASE + i as u32))),
            kind,
            at: srodek(city, s.building.0.index()),
        });
    }
    out
}

/// Pustostany. Wartość lokalu wyprowadzona z podpowiedzi czynszowej M2 — `rent_hint`
/// jest już funkcją powierzchni i wartości gruntu, więc mnożnik nie wnosi nowej wiedzy,
/// tylko zmienia jednostkę na „cenę lokalu". Krok 6 porównuje **decyle**, więc każde
/// przekształcenie rosnące daje ten sam wynik.
pub(super) fn pustostany(city: &CityData) -> Vec<HomeSlot> {
    /// Ile miesięcy czynszu składa się na wartość lokalu — 20 lat.
    const CZYNSZOW: i64 = 240;
    let mut out = Vec::new();
    for (bi, b) in city.buildings.buildings.iter().enumerate() {
        let d = dzielnica_budynku(city, bi as u32);
        for (k, u) in city.buildings.units[b.units.start as usize..b.units.end as usize]
            .iter()
            .enumerate()
        {
            if !matches!(u.kind, UnitKind::Dwelling { .. }) {
                continue;
            }
            out.push(HomeSlot {
                building: bi as u32,
                unit: k as u16,
                district: d,
                value: Money(u.rent_hint.get().saturating_mul(CZYNSZOW)),
            });
        }
    }
    out
}

/// Grafik zmiany i maska dni tygodnia wg profilu branży (§5.9 krok 5, K-15).
///
/// Maska jest **własnością obsady, nie kalendarza**: tydzień dryfuje względem miesiąca,
/// więc liczba dni roboczych w miesiącu waha się między 20 a 23 i żaden wiersz tej
/// funkcji nie ma prawa tego zakładać.
fn grafik(sector: SectorId, base: ShiftId, i: u32) -> (ShiftKind, u8) {
    const PN_PT: u8 = 0b001_1111;
    const WT_SB: u8 = 0b011_1110;
    /// Środa plus weekend — obsada, dla której sobota i niedziela są dniami pracy.
    const SR_WEEKEND: u8 = 0b110_0100;
    /// Ruch ciągły: cztery brygady po pięć dni, wolne w innej parze dni każda.
    const BRYGADY: [u8; 4] = [0b001_1111, 0b011_1110, 0b111_1001, 0b110_0111];

    match sector {
        // Handel, usługi i zieleń mają obsadę weekendową — inaczej sobota wyglądałaby
        // jak miasto zamknięte na klucz (PRD §5.5, test `prop_weekly_rhythm`).
        SectorId::Retail | SectorId::Services | SectorId::Green => match i % 6 {
            0 => (ShiftKind::Early, PN_PT),
            1 => (ShiftKind::Day, PN_PT),
            2 => (ShiftKind::Afternoon, PN_PT),
            3 => (ShiftKind::Early, WT_SB),
            4 => (ShiftKind::Afternoon, WT_SB),
            _ => (ShiftKind::Weekend, SR_WEEKEND),
        },
        // Ruch ciągły tam, gdzie dane mówią o trzeciej zmianie.
        SectorId::Industry | SectorId::Extraction if base == ShiftId::III => {
            let b = (i % 4) as usize;
            (
                [
                    ShiftKind::Early,
                    ShiftKind::Afternoon,
                    ShiftKind::Night,
                    ShiftKind::Day,
                ][b],
                BRYGADY[b],
            )
        }
        SectorId::Office | SectorId::Public => (ShiftKind::Day, PN_PT),
        _ => match i % 4 {
            0 => (ShiftKind::Early, PN_PT),
            3 => (ShiftKind::Afternoon, PN_PT),
            _ => (ShiftKind::Day, PN_PT),
        },
    }
}

/// Wakaty. Zakład wchodzi do puli z kluczem przesuniętym o [`SITE_KEY_BASE`].
pub(super) fn wakaty(city: &CityData) -> Vec<JobSlot> {
    let mut out = Vec::with_capacity(city.buildings.workplaces.len());
    for (si, s) in city.sites.sites.iter().enumerate() {
        let sector = city.site_catalog.get(s.archetype).sector();
        let d = dzielnica_budynku(city, s.building.0.index());
        for (k, wi) in (s.workplaces.start..s.workplaces.end).enumerate() {
            let Some(w) = city.buildings.workplaces.get(wi as usize) else {
                continue;
            };
            let (shift, days) = grafik(sector, w.shift, k as u32);
            out.push(JobSlot {
                site: SITE_KEY_BASE + si as u32,
                role: w.role.0,
                shift: shift as u8,
                work_days: days,
                district: d,
                wage_monthly: w.wage_band.median,
            });
        }
    }
    out
}

/// Fakty o mieście dla funkcji statusu i sąsiedztwa (korekta E-14).
pub(super) fn fakty_miasta(city: &CityData, jobs: &crate::city::build::JobTable) -> CityFacts {
    let prestiz: Vec<u8> = jobs.roles.iter().map(|r| r.prestige).collect();

    // Pozycja dzielnicy to **percentyl** średniej wartości gruntu, a nie sama wartość:
    // status jest pozycją w społeczeństwie, więc i adres musi być pozycją wśród adresów.
    let mut wartosci: Vec<(u16, i64)> = city
        .districts
        .districts
        .iter()
        .enumerate()
        .map(|(i, d)| (i as u16, d.avg_land_value.get()))
        .collect();
    wartosci.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    let n = wartosci.len().max(1);
    let mut score = vec![50u8; city.districts.districts.len()];
    for (rank, (d, _)) in wartosci.iter().enumerate() {
        score[*d as usize] = ((rank * 100) / n) as u8;
    }

    let block_of: Vec<u32> = city
        .buildings
        .buildings
        .iter()
        .enumerate()
        .map(|(i, b)| {
            city.parcels
                .parcels
                .get(b.parcel.0.index() as usize)
                .map_or(i as u32, |p| p.block.0)
        })
        .collect();

    CityFacts {
        job_prestige: prestiz,
        district_score: score,
        block_of,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grafik_daje_obsade_weekendowa_w_handlu_i_pelny_tydzien_w_ruchu_ciaglym() {
        let weekendowa = (0..6)
            .map(|i| grafik(SectorId::Retail, ShiftId::II, i).1)
            .any(|m| m & 0b110_0000 != 0);
        assert!(weekendowa, "handel bez obsady weekendowej");
        let pokrycie = (0..4)
            .map(|i| grafik(SectorId::Industry, ShiftId::III, i).1)
            .fold(0u8, |a, b| a | b);
        assert_eq!(pokrycie, 0b111_1111, "ruch ciągły nie pokrywa tygodnia");
        for i in 0..4 {
            assert_eq!(
                grafik(SectorId::Industry, ShiftId::III, i).1.count_ones(),
                5,
                "brygada {i} pracuje inną liczbę dni niż pięć"
            );
        }
        assert_eq!(
            grafik(SectorId::Office, ShiftId::I, 3),
            (ShiftKind::Day, 0b001_1111)
        );
    }
}
