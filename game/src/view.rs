//! Wypełnianie snapshotu renderu (M11a §5.2, WP2).
//!
//! ### Dlaczego to mieszka tutaj, a nie w `sim-snapshot`
//!
//! §6.1 dokumentu fazy stawiało `fill_render_snapshot(&World, &ViewQuery, &mut RenderSnapshot)`
//! w crate'cie `sim-snapshot`. Tam się nie da i nie chodzi o wygodę: §6.3 pkt 2 wymaga, żeby
//! ten crate zależał **wyłącznie** od `engine-core`, a rekordy trzeba złożyć z danych
//! trzech różnych crate'ów — pozycje z `sim/traffic`, zawód i gospodarstwo z `sim/agents`,
//! rodzaj dzielnicy z `sim/world`. Jedna funkcja nad `&World` musiałaby zależeć od
//! wszystkich i wciągnąć je do drzewa zależności renderu, czyli unieważnić to, czego
//! `sim-snapshot` pilnuje.
//!
//! `magnat-game` jest jedynym miejscem, które widzi je wszystkie naraz — i jest tym samym
//! miejscem, w którym `K-68` postawił stawianie świata. Kontrakt zostaje bez zmian:
//! **wejściem jest `&World`, nie `&mut World`**, więc kompilator gwarantuje, że publikacja
//! snapshotu nie zmienia stanu symulacji (§7.1, `snapshot_fill_is_pure`).
//!
//! ### Czego M11a jeszcze nie wypełnia i dlaczego
//!
//! `sites`, `crowd`, `weather`, `power` i `player` zostają wyzerowane. To nie jest brak,
//! tylko kolejność: pole wypełnione, którego nikt nie czyta, wygląda w danych dokładnie
//! tak samo jak działające, a pierwszy czytelnik każdego z nich powstaje w M11c
//! (postać gracza, szyldy) i M11d (pogoda, światła, dym, łoża dźwiękowe). Typy są
//! zdefiniowane i to jest treść kontraktu WP2; napełni je ta podfaza, która je narysuje.

use magnat_agents::components::{Employment, Identity, Residence};
use magnat_agents::household::Household;
use magnat_agents::social::StatusDistribution;
use magnat_agents::{demography, AgentSources};
use magnat_core::{Entity, Money};
use magnat_ecs::World;
use magnat_sim_snapshot::{
    dist2_mm, select_top_k, AgeBand, Appearance, Candidate, CitizenRenderRec, OutfitTier,
    PedestrianRecord, RenderSnapshot, VehicleRecord, VehicleRenderRec, ViewQuery,
    CITIZEN_FLAG_PLAYER_OWNED, MAX_DISTRICTS,
};
use magnat_traffic::{TrafficServices, VehicleOwner};
use magnat_voxel::PaletteLibrary;
use magnat_world::CityData;

/// Bufory robocze wypełniacza.
///
/// Trzymane między publikacjami, bo publikacja zdarza się 30 razy na sekundę i ma
/// **nie alokować** — to jest ta sama zasada, dla której snapshot ma stałą pojemność.
pub struct SnapshotFiller {
    peds: Vec<PedestrianRecord>,
    vehs: Vec<VehicleRecord>,
    cands: Vec<Candidate>,
    /// `DistrictPaletteId` per `DistrictId`, wyliczona raz przy związaniu z miastem.
    palette_by_district: [u16; MAX_DISTRICTS],
    /// Ziarno miasta, dla którego tablica wyżej została policzona. Nowa gra ma inne
    /// ziarno, więc związanie odnawia się samo — bez flagi, którą ktoś musiałby zerować.
    bound_seed: Option<u64>,
}

impl Default for SnapshotFiller {
    fn default() -> Self {
        SnapshotFiller {
            peds: Vec::new(),
            vehs: Vec::new(),
            cands: Vec::new(),
            palette_by_district: [0; MAX_DISTRICTS],
            bound_seed: None,
        }
    }
}

impl SnapshotFiller {
    #[must_use]
    pub fn new() -> SnapshotFiller {
        SnapshotFiller::default()
    }

    /// Wylicza tablicę „dzielnica → paleta" dla postawionego miasta.
    ///
    /// Raz, a nie co klatkę: rodzaj dzielnicy i epoka założenia miasta nie zmieniają się
    /// w trakcie gry, a odpytywanie biblioteki palet po kluczach tekstowych dla każdej
    /// z dwudziestu czterech tysięcy encji byłoby porównywaniem łańcuchów w pętli klatki.
    ///
    /// Dzielnica, dla której paleta nie istnieje, dostaje zero i **jest to widoczne**:
    /// zerowa paleta to paleta pierwszej dzielnicy w pierwszej epoce, więc rozjazd kluczy
    /// między `data/palettes/` a `DistrictKind` objawia się jednolitą tonacją całego
    /// miasta, a nie pustym ekranem. Pokrycia pilnuje test w `magnat_voxel::palette`.
    pub fn bind_city(&mut self, city: &CityData, palettes: &PaletteLibrary) {
        self.bound_seed = Some(city.plan.seed);
        let epoka = city.plan.epoch.key();
        self.palette_by_district = [0; MAX_DISTRICTS];
        for (i, d) in city.districts.districts.iter().enumerate() {
            if i >= MAX_DISTRICTS {
                break;
            }
            self.palette_by_district[i] = palettes
                .id_of(d.kind.key(), epoka)
                .map_or(0, |p| p.0);
        }
    }

    /// Wiąże z miastem, jeśli to inne miasto niż poprzednio. Wołane z pętli klatki:
    /// porównanie ziarna kosztuje jedno `u64`, a przeliczenie tablicy zdarza się raz
    /// na wczytany świat.
    pub fn ensure_city(&mut self, city: &CityData, palettes: &PaletteLibrary) {
        if self.bound_seed != Some(city.plan.seed) {
            self.bind_city(city, palettes);
        }
    }

    /// Wypełnia snapshot encjami widocznymi w kadrze.
    ///
    /// Krok po kroku: warstwa Mikro oddaje pozycje, `aabb` odsiewa to, co poza kadrem,
    /// `select_top_k` zostawia najbliższe encje do wysokości capu (remis po indeksie
    /// encji, więc wynik jest deterministyczny), a złączenie z komponentami dokłada
    /// wygląd. Skan jest **lokalny**: warstwa Mikro ma własne okno wokół kamery i nie
    /// zwraca całego miasta.
    pub fn fill(&mut self, world: &World, view: &ViewQuery, out: &mut RenderSnapshot) {
        self.fill_filtered(world, view, &|_| true, out);
    }

    /// To samo z filtrem encji (§14.2): mieszkaniec, dla którego predykat zwróci `false`,
    /// **nie trafia do snapshotu**.
    ///
    /// Filtr jest preferencją widoku i dlatego stoi tutaj, a nie w warstwie Mikro:
    /// warstwa jest wspólna dla renderu i dla śledzenia, więc ukrycie kogoś na ekranie
    /// nie ma prawa zmienić tego, kogo symulacja liczy. Filtr **nie dotyczy pojazdów** —
    /// „moi klienci" i „moi pracownicy" są pytaniami o ludzi.
    pub fn fill_filtered(
        &mut self,
        world: &World,
        view: &ViewQuery,
        keep: &dyn Fn(u32) -> bool,
        out: &mut RenderSnapshot,
    ) {
        out.clear();
        out.tick = world.tick;
        out.sim_minute = magnat_core::SimMinute(world.tick.get());
        out.district_palette = self.palette_by_district;

        let doba = (world.tick.get() / 1440) as i32;
        let seed = world.seed;

        self.fill_citizens(world, view, doba, seed, keep, out);
        self.fill_vehicles(world, view, out);
    }

    fn fill_citizens(
        &mut self,
        world: &World,
        view: &ViewQuery,
        doba: i32,
        seed: u64,
        keep: &dyn Fn(u32) -> bool,
        out: &mut RenderSnapshot,
    ) {
        self.peds.clear();
        if let Some(z) = world
            .get_resource::<AgentSources>()
            .and_then(AgentSources::get)
        {
            z.travel.micro_snapshot(&mut self.peds);
        }
        self.cands.clear();
        for (i, p) in self.peds.iter().enumerate() {
            let pos = to_mm(p.pos);
            if !view.aabb.contains(pos) || !keep(p.entity) {
                continue;
            }
            self.cands.push(Candidate {
                dist2: dist2_mm(pos, view.eye),
                entity: p.entity,
                src: i as u32,
            });
        }
        select_top_k(&mut self.cands, view.caps.citizens as usize);

        let status = world.get_resource::<StatusDistribution>();
        for c in &self.cands {
            let p = self.peds[c.src as usize];
            out.citizens.push(citizen_rec(world, &p, doba, seed, status));
        }
    }

    fn fill_vehicles(&mut self, world: &World, view: &ViewQuery, out: &mut RenderSnapshot) {
        self.vehs.clear();
        if let Some(t) = world.get_resource::<TrafficServices>() {
            t.oracle.micro().vehicle_snapshot(&mut self.vehs);
        }
        self.cands.clear();
        for (i, v) in self.vehs.iter().enumerate() {
            let pos = to_mm(v.pos);
            if !view.aabb.contains(pos) {
                continue;
            }
            self.cands.push(Candidate {
                dist2: dist2_mm(pos, view.eye),
                entity: v.entity,
                src: i as u32,
            });
        }
        select_top_k(&mut self.cands, view.caps.vehicles as usize);

        for c in &self.cands {
            let v = self.vehs[c.src as usize];
            out.vehicles.push(vehicle_rec(world, &v));
        }
    }
}

/// Metry `f32` → milimetry `i32`, z nasyceniem zamiast zawijania.
fn to_mm(p: [f32; 3]) -> [i32; 3] {
    let m = |v: f32| (f64::from(v) * 1000.0).round().clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32;
    [m(p[0]), m(p[1]), m(p[2])]
}

/// Kąt w radianach → 1/65536 obrotu.
fn yaw_u16(rad: f32) -> u16 {
    const TAU: f32 = std::f32::consts::TAU;
    let t = rad.rem_euclid(TAU) / TAU;
    (t * 65536.0) as u32 as u16
}

fn citizen_rec(
    world: &World,
    p: &PedestrianRecord,
    doba: i32,
    seed: u64,
    status: Option<&StatusDistribution>,
) -> CitizenRenderRec {
    let e = demography::citizen_by_index(world, p.entity);
    let id = e.and_then(|e| world.get::<Identity>(e)).copied();
    let emp = e.and_then(|e| world.get::<Employment>(e)).copied();
    let res = e.and_then(|e| world.get::<Residence>(e)).copied();

    // Klasa ubrania z roli zawodowej. Katalog ról rośnie (46 po M7a), a pole ma pięć
    // bitów, więc reszta z dzielenia jest jedyną odpowiedzią, która nie wywraca renderu
    // przy dopisaniu roli — kasjer i recepcjonista i tak dzielą ubranie (ryzyko R4 fazy).
    let outfit_class = emp
        .filter(|e| e.site != Employment::NO_SITE)
        .map_or(0, |e| (e.role % 32) as u8);
    let tier = wealth_tier(world, e, status);
    let age = id.map_or(AgeBand::Adult, |i| {
        AgeBand::from_years(i.age_years(doba).clamp(0, 255) as u8)
    });
    let flags = if id.is_some_and(|i| i.flags & Identity::FLAG_PLAYER != 0) {
        CITIZEN_FLAG_PLAYER_OWNED
    } else {
        0
    };

    CitizenRenderRec {
        pos: to_mm(p.pos),
        entity_lo: p.entity,
        appearance: Appearance::derive(seed, p.entity, outfit_class, tier as u8, age as u8, 0).0,
        // Kierunek marszu wnosi M11b razem z klipami lokomocji: rekord ruchu go nie
        // niesie, a zgadywanie z różnicy pozycji między publikacjami dałoby obrót
        // zależny od częstotliwości publikacji, czyli od klatki.
        yaw: 0,
        district: res.map_or(0, |r| r.district),
        anim_state: 0,
        // Decyzja 9.3 przyjęta wg propozycji domyślnej: fazę liczy symulacja, funkcją
        // czystą od `(chwila, indeks encji)`, i **nie ma jej w ECS**. Rozsunięcie po
        // indeksie jest po to, żeby tłum nie maszerował w jednym takcie.
        anim_phase: anim_phase(world.tick.get(), p.entity),
        carry: 0,
        flags,
        _pad: [0; 4],
    }
}

fn vehicle_rec(world: &World, v: &VehicleRecord) -> VehicleRenderRec {
    let owner = vehicle_entity(world, v.entity).and_then(|e| world.get::<VehicleOwner>(e));
    let hh = owner
        .and_then(|o| demography::household_by_index(world, o.owner))
        .and_then(|h| world.get::<Household>(h));

    VehicleRenderRec {
        pos: to_mm(v.pos),
        entity_lo: v.entity,
        yaw: yaw_u16(v.heading),
        pitch: 0,
        // Jeden model bryły na razie — katalog modeli pojazdów wnosi M11b razem
        // z poziomami detalu. `class` z rekordu ruchu jedzie w `model` bez zmiany,
        // bo to jego docelowe znaczenie.
        model: u16::from(v.class),
        // Lakier należy do **gospodarstwa**, nie do pojazdu: ten sam numer właściciela
        // daje zawsze tę samą barwę, a wybór z zestawu robi paleta epoki (PRD §16.3).
        paint: owner.map_or(0, |o| (o.owner & 0xFFFF) as u16),
        livery: 0,
        district: hh.map_or(0, |h| h.district),
        anim_state: 0,
        anim_phase: anim_phase(world.tick.get(), v.entity),
        wheel_phase: anim_phase(world.tick.get(), v.entity.wrapping_add(7)),
        load: 0,
        flags: 0,
        occupants: 0,
        _pad: [0; 6],
    }
}

/// Faza animacji: funkcja czysta od chwili i indeksu encji (decyzja 9.3).
fn anim_phase(tick: u64, entity: u32) -> u8 {
    (tick as u32)
        .wrapping_mul(11)
        .wrapping_add(entity.wrapping_mul(97)) as u8
}

fn wealth_tier(
    world: &World,
    e: Option<Entity>,
    status: Option<&StatusDistribution>,
) -> OutfitTier {
    let (Some(e), Some(status)) = (e, status) else {
        return OutfitTier::Modest;
    };
    let Some(hh) = world
        .get::<Identity>(e)
        .and_then(|id| demography::household_by_index(world, id.household))
        .and_then(|h| world.get::<Household>(h))
    else {
        return OutfitTier::Modest;
    };
    let majatek = Money(
        hh.cash
            .get()
            .saturating_add(hh.bank.get())
            .saturating_add(hh.savings.get()),
    );
    OutfitTier::from_percentile(status.wealth_percentile(majatek))
}

/// Encja pojazdu z samego indeksu. Pojazdy nie są w spisie ludności, więc generacja
/// pochodzi wprost z magazynu encji — inaczej niż u mieszkańca, który ma `Population`.
fn vehicle_entity(world: &World, index: u32) -> Option<Entity> {
    let gen = world.entities().generation_at(index)?;
    Some(Entity::new(index, std::num::NonZeroU32::new(gen)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_sim_snapshot::{Aabb, SnapshotCaps};

    fn zapytanie() -> ViewQuery {
        ViewQuery {
            aabb: Aabb::around([0, 0, 0], 2_000_000, 2_000_000),
            eye: [0, 0, 0],
            caps: SnapshotCaps::DEFAULT,
        }
    }

    /// `snapshot_fill_is_pure` z §7.1: publikacja snapshotu nie zmienia stanu symulacji.
    /// Sygnatura `&World` gwarantuje to na poziomie kompilatora, ale hash jest dowodem,
    /// że nie ma obejścia przez wnętrze komórki ani przez zasób pod muteksem.
    #[test]
    fn wypelnianie_nie_zmienia_swiata() {
        let world = magnat_ecs::World::new(7);
        let przed = magnat_io::world_state_hash(&world);
        let mut f = SnapshotFiller::new();
        let mut snap = RenderSnapshot::default();
        for _ in 0..1_000 {
            f.fill(&world, &zapytanie(), &mut snap);
        }
        assert_eq!(
            magnat_io::world_state_hash(&world),
            przed,
            "wypełnianie ruszyło stan"
        );
    }

    /// Świat bez warstwy Mikro i bez ruchu ma dawać pusty snapshot, a nie panikę:
    /// scenariusz bezgłowy stawia wycinek symulacji i wypełniacz ma to przeżyć.
    #[test]
    fn swiat_bez_ruchu_daje_pusty_snapshot() {
        let world = magnat_ecs::World::new(1);
        let mut f = SnapshotFiller::new();
        let mut snap = RenderSnapshot::default();
        f.fill(&world, &zapytanie(), &mut snap);
        assert!(snap.citizens.is_empty());
        assert!(snap.vehicles.is_empty());
        assert_eq!(snap.resident_bytes(), RenderSnapshot::default().resident_bytes());
    }

    #[test]
    fn metry_na_milimetry_nie_zawijaja_sie_na_krancach() {
        assert_eq!(to_mm([1.5, -2.25, 0.0]), [1500, -2250, 0]);
        let ogromne = to_mm([1.0e12, -1.0e12, 0.0]);
        assert_eq!(ogromne[0], i32::MAX);
        assert_eq!(ogromne[1], i32::MIN);
    }

    #[test]
    fn kat_zawija_sie_do_pelnego_obrotu() {
        let tau = std::f32::consts::TAU;
        assert_eq!(yaw_u16(0.0), 0);
        assert_eq!(yaw_u16(tau), 0, "pełny obrót to zero, nie przepełnienie");
        assert!((i32::from(yaw_u16(tau / 2.0)) - 32_768).abs() <= 1);
        assert_eq!(yaw_u16(-tau / 4.0), yaw_u16(3.0 * tau / 4.0));
    }

    /// Faza rozsuwa się po indeksie encji — inaczej cały tłum maszerowałby w jednym takcie.
    #[test]
    fn faza_animacji_rozsuwa_tlum() {
        let fazy: std::collections::BTreeSet<u8> =
            (0..64u32).map(|i| anim_phase(100, i)).collect();
        assert!(fazy.len() > 30, "tylko {} różnych faz", fazy.len());
        assert_eq!(anim_phase(100, 5), anim_phase(100, 5), "faza nie jest czysta");
    }
}
