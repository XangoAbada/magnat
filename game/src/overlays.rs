//! Nakładki danych i filtry encji (M9c §5.10, WP7; PRD §14.2).
//!
//! # Podział pracy
//!
//! `game/` **wybiera i liczy pole**, `engine/render` je rysuje, a paleta i progi są
//! w `data/ui/overlays.ron` (`K-19`) — ta sama tabela dla klienta graficznego i dla
//! podglądu bezgłowego. Dzięki temu zmiana progu jest widoczna w diffie, a nie
//! w rekompilacji, i obie drogi rysują tę samą mapę.
//!
//! # Skąd bierze się liczba
//!
//! Trzy kształty, bo trzy rodzaje danych. **Po działkach** (`parcel_raster` z M2) idzie
//! wszystko, co opisuje miejsce: wartość gruntu, dochód gospodarstw, bezrobocie, zdrowie,
//! cena towaru, zasięg sklepu. **Po punktach** — emisje zakładów. **Po odcinkach** —
//! przepływ towaru i ruch drogowy.
//!
//! Agregaty dzielnicowe liczą się **jednym przejściem po mieszkańcach** przy każdej
//! przebudowie nakładki, a nie w kroku symulacji: nakładka jest widokiem i nie ma prawa
//! zostawić po sobie stanu (00 §4). Koszt jest liniowy od liczby mieszkańców i płaci go
//! wyłącznie ten, kto nakładkę otworzył.

use crate::Session;
use magnat_core::{DistrictId, GoodId, SiteId, WorldCoord};
use magnat_world::{OverlaySpec, OverlayTable};

/// Pole nakładki gotowe dla renderera: indeksy palety na siatce kwadratowej.
pub struct OverlayField2d {
    pub dim: u32,
    pub cell_m: f32,
    /// Jeden bajt na komórkę — **indeks palety**, nie wartość surowa (`K-19`).
    pub values: Vec<u8>,
    pub palette: [[u8; 4]; 256],
    /// Podziałki legendy: indeks palety i wartość progu w jednostce nakładki.
    pub legend: Vec<(u8, i64)>,
    /// Jednostka, w której gracz czyta legendę.
    pub unit: String,
}

/// Co pokazuje nakładka (PRD §14.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OverlayField {
    LandValue,
    HouseholdIncome,
    ShopCatchment {
        site: SiteId,
    },
    Traffic,
    ProductPrice {
        good: GoodId,
    },
    Unemployment,
    Health,
    Pollution,
    /// Przepływ towaru: masa w drodze między zakładami.
    GoodFlow {
        good: GoodId,
    },
    /// Zarezerwowane dla M10 — marki jeszcze nie ma, więc nakładka nie ma czego
    /// pokazać i mówi to wprost, zamiast rysować zera (`R2`, decyzja §9 pkt 11).
    BrandAwareness,
}

impl OverlayField {
    /// Klucz w `data/ui/overlays.ron` i człon klucza `ui.overlay.<key>` w `data/locale/`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            OverlayField::LandValue => "land_value",
            OverlayField::HouseholdIncome => "household_income",
            OverlayField::ShopCatchment { .. } => "shop_catchment",
            OverlayField::Traffic => "traffic_flow",
            OverlayField::ProductPrice { .. } => "product_price",
            OverlayField::Unemployment => "unemployment",
            OverlayField::Health => "health",
            OverlayField::Pollution => "pollution",
            OverlayField::GoodFlow { .. } => "good_flow",
            OverlayField::BrandAwareness => "brand_awareness",
        }
    }

    /// Czy pole czeka na fazę, która je zasili.
    #[must_use]
    pub const fn is_reserved(self) -> bool {
        matches!(self, OverlayField::BrandAwareness)
    }

    /// Dziewięć nakładek z §14.2 — bez rezerwacji dla M10.
    #[must_use]
    pub fn all(site: SiteId, good: GoodId) -> [OverlayField; 9] {
        [
            OverlayField::LandValue,
            OverlayField::HouseholdIncome,
            OverlayField::ShopCatchment { site },
            OverlayField::Traffic,
            OverlayField::ProductPrice { good },
            OverlayField::Unemployment,
            OverlayField::Health,
            OverlayField::Pollution,
            OverlayField::GoodFlow { good },
        ]
    }
}

/// Filtr encji rysowanych w świecie (PRD §14.2: „pokaż tylko klientów mojego sklepu").
///
/// **Nie jest to `ConditionExpr`** i to jest korekta planu (§5.10 pisało
/// `type EntityFilter = ConditionExpr`). Powód jest ten sam co przy predykacie wyboru
/// postaci: metryki języka reguł opisują cenę, zapas i kadry firmy, a nie „czy ten
/// pieszy kupił u mnie". Trzy warianty z PRD to trzy pytania i każde ma odpowiedź
/// w danych, które już są — drzewo składniowe bez metryk i tak nie umiałoby ich zadać.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EntityFilter {
    /// Tylko mieszkańcy, którzy kupili w zakładzie gracza (pierścień klientów M5e).
    MyCustomers { site: SiteId },
    /// Tylko pracownicy zakładu gracza.
    MyEmployees { site: SiteId },
}

impl EntityFilter {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            EntityFilter::MyCustomers { .. } => "my_customers",
            EntityFilter::MyEmployees { .. } => "my_employees",
        }
    }

    /// Czy tego mieszkańca wolno pokazać.
    #[must_use]
    pub fn accepts(self, session: &Session, citizen: u32) -> bool {
        match self {
            EntityFilter::MyCustomers { site } => {
                // Klientem jest ten, kto **kupił**: pierścień utraconych sprzedaży
                // odpowiada na pytanie odwrotne, więc tu go nie ma.
                let Some(e) = encja_mieszkanca(session, citizen) else {
                    return false;
                };
                session
                    .market
                    .as_ref()
                    .is_some_and(|m| m.is_customer(site, magnat_core::CitizenId(e)))
            }
            EntityFilter::MyEmployees { site } => encja_mieszkanca(session, citizen)
                .and_then(|e| session.app.world.get::<magnat_agents::Employment>(e))
                .is_some_and(|e| {
                    matches!(
                        magnat_agents::places::place_from_key(e.site),
                        Some(magnat_core::PlaceRef::Site(s)) if s == site
                    )
                }),
        }
    }
}

/// Encja mieszkańca po indeksie — bufor identyfikatorów renderu niesie indeks,
/// a komponenty adresuje się encją z generacją.
fn encja_mieszkanca(session: &Session, citizen: u32) -> Option<magnat_core::Entity> {
    magnat_agents::citizen_by_index(&session.app.world, citizen)
}

/// Buduje pole nakładki. `None` = nakładka nie ma z czego powstać: pole zarezerwowane,
/// brak gospodarki albo brak danych w tym świecie.
///
/// # Panics
/// Nie panikuje: brak `data/ui/overlays.ron` daje `None`, tak samo jak brak danych.
#[must_use]
pub fn build(session: &Session, field: OverlayField) -> Option<OverlayField2d> {
    if field.is_reserved() {
        return None;
    }
    let tab = OverlayTable::load().ok()?;
    let spec = tab.get(field.key()).ok()?.clone();
    let city = &session.built.city;

    let (dim, cell_m, values) = match field {
        OverlayField::LandValue => {
            magnat_world::parcel_raster(city, &spec, |p| p.land_value_per_m2.0)
        }
        OverlayField::HouseholdIncome => {
            let per = dochod_dzielnic(session);
            po_dzielnicach(session, &spec, &per)
        }
        OverlayField::Unemployment => {
            let per = bezrobocie_dzielnic(session);
            po_dzielnicach(session, &spec, &per)
        }
        OverlayField::Health => {
            let per = zdrowie_dzielnic(session);
            po_dzielnicach(session, &spec, &per)
        }
        OverlayField::ProductPrice { good } => {
            let per = ceny_dzielnic(session, good);
            po_dzielnicach(session, &spec, &per)
        }
        OverlayField::ShopCatchment { site } => {
            let per = zasieg_sklepu(session, site)?;
            po_dzielnicach(session, &spec, &per)
        }
        OverlayField::Pollution => punkty(session, &spec, &emisje(session)),
        OverlayField::GoodFlow { good } => odcinki(session, &spec, &przeplyw(session, good)),
        OverlayField::Traffic => ruch(session, &spec)?,
        OverlayField::BrandAwareness => return None,
    };

    Some(OverlayField2d {
        dim,
        cell_m,
        values,
        palette: spec.palette(),
        legend: spec.legend(),
        unit: spec.unit.clone(),
    })
}

// ── agregaty dzielnicowe ─────────────────────────────────────────────────────────

fn ile_dzielnic(session: &Session) -> usize {
    session.built.city.districts.districts.len().max(1)
}

/// Średni miesięczny dochód gospodarstwa w dzielnicy, w groszach.
fn dochod_dzielnic(session: &Session) -> Vec<i64> {
    let world = &session.app.world;
    let n = ile_dzielnic(session);
    let (mut suma, mut ile) = (vec![0i64; n], vec![0u32; n]);
    let Some(p) = world.get_resource::<magnat_agents::Population>() else {
        return suma;
    };
    for e in p.households() {
        let Some(h) = world.get::<magnat_agents::Household>(*e) else {
            continue;
        };
        if h.flags & magnat_agents::Household::FLAG_ACTIVE == 0 {
            continue;
        }
        let d = usize::from(h.district).min(n - 1);
        suma[d] += h.income_monthly.get();
        ile[d] += 1;
    }
    srednia(&suma, &ile)
}

/// Bezrobocie w promilach siły roboczej. Granice wieku produkcyjnego czyta się
/// z `data/demography/demography.ron` (`K-60`), a nie ze stałej w kodzie.
fn bezrobocie_dzielnic(session: &Session) -> Vec<i64> {
    let world = &session.app.world;
    let n = ile_dzielnic(session);
    let (mut bez, mut sila) = (vec![0i64; n], vec![0u32; n]);
    let (Some(p), Some(tab)) = (
        world.get_resource::<magnat_agents::Population>(),
        world.get_resource::<magnat_agents::DemographyTable>(),
    ) else {
        return bez;
    };
    let lf = tab.ages().labour_force;
    let doba = session.tick().get() / 1440;
    for e in p.citizens() {
        let (Some(id), Some(emp), Some(res)) = (
            world.get::<magnat_agents::Identity>(*e),
            world.get::<magnat_agents::Employment>(*e),
            world.get::<magnat_agents::Residence>(*e),
        ) else {
            continue;
        };
        if !id.is_alive() {
            continue;
        }
        let wiek = id.age_years(doba as i32);
        if wiek < i32::from(lf.min) || wiek > i32::from(lf.max) {
            continue;
        }
        let d = usize::from(res.district).min(n - 1);
        sila[d] += 1;
        if !emp.is_employed() {
            bez[d] += 1;
        }
    }
    bez.iter()
        .zip(&sila)
        .map(|(b, s)| if *s == 0 { 0 } else { b * 1000 / i64::from(*s) })
        .collect()
}

/// Średnie zdrowie mieszkańca w dzielnicy, w skali `Q`.
fn zdrowie_dzielnic(session: &Session) -> Vec<i64> {
    let world = &session.app.world;
    let n = ile_dzielnic(session);
    let (mut suma, mut ile) = (vec![0i64; n], vec![0u32; n]);
    let Some(p) = world.get_resource::<magnat_agents::Population>() else {
        return suma;
    };
    for e in p.citizens() {
        let (Some(id), Some(v), Some(res)) = (
            world.get::<magnat_agents::Identity>(*e),
            world.get::<magnat_agents::Vitals>(*e),
            world.get::<magnat_agents::Residence>(*e),
        ) else {
            continue;
        };
        if !id.is_alive() {
            continue;
        }
        let d = usize::from(res.district).min(n - 1);
        suma[d] += i64::from(v.health_q().get());
        ile[d] += 1;
    }
    srednia(&suma, &ile)
}

/// Ostatnia obserwowana cena towaru w dzielnicy (tablica ogłoszeń M5).
fn ceny_dzielnic(session: &Session, good: GoodId) -> Vec<i64> {
    let n = ile_dzielnic(session);
    let world = &session.app.world;
    let Some(b) = world.get_resource::<magnat_economy::PublicMarketBoard>() else {
        return vec![0; n];
    };
    (0..n)
        .map(|d| {
            b.latest(DistrictId(d as u16), good)
                .map_or(0, |o| o.median_net.get())
        })
        .collect()
}

/// Zasięg sklepu: ilu klientów z której dzielnicy w ostatnim tygodniu.
///
/// `None`, gdy zakład nie jest śledzony — rozkład klientów prowadzą wyłącznie zakłady
/// gracza (`Z-3` z M5e), więc nakładka dla cudzego sklepu nie kłamie zerami, tylko
/// się nie otwiera.
fn zasieg_sklepu(session: &Session, site: SiteId) -> Option<Vec<i64>> {
    let n = ile_dzielnic(session);
    let m = session.market.as_ref()?;
    let t = session.tick();
    let od = magnat_core::Tick(t.get() - t.get() % magnat_core::time::MINUTES_PER_MONTH);
    let snap = m.shop_panel(site, od, t)?;
    let mut out = vec![0i64; n];
    for (d, ile) in &snap.customers.by_district {
        if let Some(v) = out.get_mut(usize::from(d.get())) {
            *v = i64::from(*ile);
        }
    }
    Some(out)
}

fn srednia(suma: &[i64], ile: &[u32]) -> Vec<i64> {
    suma.iter()
        .zip(ile)
        .map(|(s, n)| if *n == 0 { 0 } else { s / i64::from(*n) })
        .collect()
}

/// Stempluje wartość dzielnicy na jej działkach — dzielnica jest zbiorem kwartałów,
/// a kwartał zbiorem działek, więc granica wychodzi tam, gdzie naprawdę jest.
fn po_dzielnicach(session: &Session, spec: &OverlaySpec, per: &[i64]) -> (u32, f32, Vec<u8>) {
    magnat_world::parcel_raster(&session.built.city, spec, |p| {
        per.get(usize::from(p.district.get())).copied().unwrap_or(0)
    })
}

// ── punkty i odcinki ─────────────────────────────────────────────────────────────

/// Emisje zakładów: pozycja i gramy pyłu na minutę.
fn emisje(session: &Session) -> Vec<(WorldCoord, i64)> {
    let world = &session.app.world;
    let Some(city) = world.get_resource::<magnat_city::City>() else {
        return Vec::new();
    };
    city.emissions
        .iter()
        .filter_map(|(site, _, g)| pozycja_zakladu(session, *site).map(|p| (p, *g)))
        .collect()
}

/// Masa towaru w drodze: odcinek od nadawcy do odbiorcy.
///
/// `ponytail:` odcinek jest **prostą między zakładami**, a nie trasą po drogach. Sufit
/// nazwany: trasa wymagałaby routingu każdego zlecenia przy każdym przerysowaniu,
/// a animowane strumienie z §14.2 i tak należą do prezentacji (M11). Kierunku ten
/// kształt nie niesie i nie udaje, że niesie.
fn przeplyw(session: &Session, good: GoodId) -> Vec<(WorldCoord, WorldCoord, i64)> {
    let world = &session.app.world;
    let Some(t) = world.get_resource::<magnat_supply::Transport>() else {
        return Vec::new();
    };
    t.iter()
        .filter(|o| o.good == good && w_drodze(o.state))
        .filter_map(|o| {
            let a = pozycja_zakladu(session, o.from)?;
            let b = pozycja_zakladu(session, o.to)?;
            Some((a, b, o.mass.get() / 1000))
        })
        .collect()
}

/// Czy zlecenie jest w drodze: między załadunkiem a rozładunkiem.
///
/// Zlecenie w szkicu i w przetargu **nie jest przepływem** — nikt jeszcze niczego
/// nie wiezie, a nakładka ma pokazywać masę, która naprawdę jedzie.
const fn w_drodze(s: magnat_supply::TransportOrderState) -> bool {
    use magnat_supply::TransportOrderState as S;
    matches!(
        s,
        S::Assigned { .. }
            | S::LoadingQueue
            | S::Loading { .. }
            | S::EnRoute { .. }
            | S::UnloadingQueue
            | S::Unloading { .. }
    )
}

/// Pozycja zakładu: z rynku, jeśli to sklep, inaczej ze środka budynku, w którym stoi.
fn pozycja_zakladu(session: &Session, site: SiteId) -> Option<WorldCoord> {
    if let Some(p) = session.market.as_ref().and_then(|m| m.shop_pos(site)) {
        return Some(WorldCoord {
            x: (p.x * 100.0) as i32,
            y: (p.y * 100.0) as i32,
            z: 0,
        });
    }
    let firms = session.app.world.get_resource::<magnat_firms::Firms>()?;
    let b = firms.site(site)?.building;
    let bud = session
        .built
        .city
        .buildings
        .buildings
        .get(b.entity().index() as usize)?;
    let c = (bud.aabb.min + bud.aabb.max) * 0.5;
    Some(WorldCoord {
        x: (c.x * 100.0) as i32,
        y: (c.y * 100.0) as i32,
        z: 0,
    })
}

fn wymiary(session: &Session) -> (usize, f32) {
    let bok = f32::from(magnat_world::OVERLAY_CELL_M);
    let dim = ((session.built.city.plan.map_size_m() as f32 / bok).ceil() as usize).max(1);
    (dim, bok)
}

/// Stempel punktowy z liniowym zanikiem — komin brudzi wokół siebie, nie w jednej komórce.
fn punkty(session: &Session, spec: &OverlaySpec, pts: &[(WorldCoord, i64)]) -> (u32, f32, Vec<u8>) {
    const PROMIEN_M: f32 = 400.0;
    let (dim, bok) = wymiary(session);
    let mut suma = vec![0i64; dim * dim];
    let r = (PROMIEN_M / bok).ceil() as i64;
    for (p, w) in pts {
        let (cx, cy) = (
            (p.x as f32 / 100.0 / bok) as i64,
            (p.y as f32 / 100.0 / bok) as i64,
        );
        for dy in -r..=r {
            for dx in -r..=r {
                let (x, y) = (cx + dx, cy + dy);
                if x < 0 || y < 0 || x >= dim as i64 || y >= dim as i64 {
                    continue;
                }
                let d = ((dx * dx + dy * dy) as f64).sqrt();
                if d > r as f64 {
                    continue;
                }
                let zanik = 1.0 - d / r as f64;
                suma[y as usize * dim + x as usize] += (*w as f64 * zanik) as i64;
            }
        }
    }
    (dim as u32, bok, na_indeksy(spec, &suma))
}

/// Stempel odcinkowy: próbkuje prostą co pół komórki, więc nie gubi kratek na skosie.
fn odcinki(
    session: &Session,
    spec: &OverlaySpec,
    seg: &[(WorldCoord, WorldCoord, i64)],
) -> (u32, f32, Vec<u8>) {
    let (dim, bok) = wymiary(session);
    let mut suma = vec![0i64; dim * dim];
    for (a, b, w) in seg {
        let (ax, ay) = (a.x as f32 / 100.0 / bok, a.y as f32 / 100.0 / bok);
        let (bx, by) = (b.x as f32 / 100.0 / bok, b.y as f32 / 100.0 / bok);
        let kroki = ((bx - ax).abs().max((by - ay).abs()) * 2.0).ceil().max(1.0) as i32;
        for i in 0..=kroki {
            let t = f64::from(i) / f64::from(kroki);
            let x = f64::from(ax) + (f64::from(bx) - f64::from(ax)) * t;
            let y = f64::from(ay) + (f64::from(by) - f64::from(ay)) * t;
            if x < 0.0 || y < 0.0 || x >= dim as f64 || y >= dim as f64 {
                continue;
            }
            suma[y as usize * dim + x as usize] += *w;
        }
    }
    (dim as u32, bok, na_indeksy(spec, &suma))
}

fn na_indeksy(spec: &OverlaySpec, v: &[i64]) -> Vec<u8> {
    v.iter()
        .map(|x| if *x == 0 { 0 } else { spec.index_of(*x) })
        .collect()
}

/// Nakładka ruchu — z przedniego bufora zrzutu, czyli bez czekania na symulację.
fn ruch(session: &Session, spec: &OverlaySpec) -> Option<(u32, f32, Vec<u8>)> {
    let world = &session.app.world;
    let (dim, bok) = wymiary(session);
    let oracle = world
        .get_resource::<magnat_traffic::TrafficServices>()?
        .oracle
        .clone();
    let pole = magnat_traffic::TrafficField::Flow;
    let surowe = world
        .get_resource::<magnat_traffic::TrafficOverlay>()?
        .with_front(|snap| {
            oracle.with_road(|road| {
                let v: Vec<u16> = (0..road.edge_count())
                    .map(|i| snap.edge_value(pole, i).clamp(0, i64::from(u16::MAX)) as u16)
                    .collect();
                magnat_traffic::rasterize_edges(road, &v, dim as u32, bok as u32, 1)
            })
        });
    let idx = surowe
        .iter()
        .map(|x| {
            if *x == 0 {
                0
            } else {
                spec.index_of(i64::from(*x))
            }
        })
        .collect();
    Some((dim as u32, bok, idx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kazda_nakladka_ma_wpis_w_danych() {
        let t = OverlayTable::load().expect("data/ui/overlays.ron");
        let site = SiteId(magnat_core::Entity::new(1, std::num::NonZeroU32::MIN));
        for f in OverlayField::all(site, GoodId(0)) {
            assert!(
                t.get(f.key()).is_ok(),
                "nakładka {} nie ma palety w data/ui/overlays.ron",
                f.key()
            );
        }
        // Rezerwacja M10 celowo **nie ma** wpisu: paleta bez danych rysowałaby zera.
        assert!(OverlayField::BrandAwareness.is_reserved());
        assert!(t.get(OverlayField::BrandAwareness.key()).is_err());
    }

    #[test]
    fn kazdy_filtr_ma_wlasny_klucz() {
        let site = SiteId(magnat_core::Entity::new(1, std::num::NonZeroU32::MIN));
        let f = [
            EntityFilter::MyCustomers { site },
            EntityFilter::MyEmployees { site },
        ];
        let mut k: Vec<&str> = f.iter().map(|x| x.key()).collect();
        k.sort_unstable();
        let ile = k.len();
        k.dedup();
        assert_eq!(k.len(), ile, "dwa filtry o tym samym kluczu");
    }

    #[test]
    fn kazda_nakladka_ma_wlasny_klucz() {
        let site = SiteId(magnat_core::Entity::new(1, std::num::NonZeroU32::MIN));
        let mut k: Vec<&str> = OverlayField::all(site, GoodId(0))
            .iter()
            .map(|f| f.key())
            .collect();
        k.sort_unstable();
        let ile = k.len();
        k.dedup();
        assert_eq!(k.len(), ile, "dwie nakładki o tym samym kluczu");
        assert_eq!(ile, 9, "§14.2 wymienia dziewięć nakładek bez marki");
    }
}
