//! Trasa ładunku — wtyczka M4 do portu `magnat_supply::FreightOracle` (M6d, `AG-1`).
//!
//! Do M6c ładunek jeździł atrapą `FlatRateFreight`: jedna odległość dla całego miasta
//! i stawka tonokilometrowa. To wystarczało do przetestowania **maszyny stanów zlecenia**
//! i nie wystarcza do niczego więcej — przy stałej odległości porównanie „centrum
//! dystrybucyjne kontra dostawa bezpośrednia" rozstrzyga się arytmetyką zamiast
//! geografią, czyli spełnia się tożsamościowo.
//!
//! Wzorzec jest ten sam co `TravelOracle` (`Z-1`) i `Deposits` (§5.10): port i atrapa
//! stoją w crate'cie, który pyta (`sim/supply`), implementacja produkcyjna w tym, który
//! umie odpowiedzieć. Kierunek zależności `traffic → supply` jest bezpieczny, bo
//! `sim/supply` zależy wyłącznie od `engine/core`.
//!
//! **Co ta wtyczka wnosi ponad atrapę:**
//! 1. prawdziwe kilometry — z grafu drogowego, przez `NavGraphs::route_length_cm`;
//! 2. prawdziwą odmowę — profil `HeavyDay`/`HeavyNight` wyłącza mosty o za małym
//!    tonażu i krawędzie z zakazem ruchu ciężkiego, więc `None` znaczy „tą ciężarówką
//!    tam nie dojedziesz", a nie „nie chce mi się liczyć";
//! 3. pojazd o skończonej ładowności — ładunek większy niż skrzynia wymaga kilku kursów
//!    i **za każdy się płaci**, bo stawka jest za kilometr pojazdu, nie za tonokilometr.
//!    To jest cały mechanizm, dla którego konsolidacja dostaw ma sens.

use std::collections::BTreeMap;
use std::sync::Arc;

use magnat_core::{Mass, Money, SiteId, WorldCoord};
use magnat_nav::RouteQuery;
use magnat_supply::{FreightOracle, FreightQuote, VehicleRequirements};

use crate::oracle::TrafficOracle;
use crate::spec::{VehicleCatalog, VehicleClassId};

/// Godzina, o której planuje się dostawy, gdy nikt nie powie inaczej.
///
/// `ponytail:` jedna godzina zamiast pory wyjazdu zlecenia. Sufit nazwany: `TransportOrder`
/// zna `ready_at`, ale `FreightOracle::quote` go nie dostaje — wycena jest robiona,
/// zanim zlecenie w ogóle powstanie (rynek B2B liczy nią oferty). Droga wyjścia to
/// dołożenie pory do sygnatury portu, kiedy pierwszy konsument naprawdę jej potrzebuje.
const GODZINA_DOSTAW: u8 = 8;

/// Rampa zakładu w sieci drogowej.
pub struct RoadFreight {
    oracle: Arc<TrafficOracle>,
    catalog: Arc<VehicleCatalog>,
    /// Pozycje ramp. `BTreeMap`, nie `HashMap` — 00 §3.2; po tej mapie się nie iteruje,
    /// ale zakaz obowiązuje bez wyjątków dla „na razie nie iteruję".
    sites: BTreeMap<SiteId, WorldCoord>,
    /// Stała opłata za podstawienie pojazdu, w groszach. Pokrywa to, czego kilometry
    /// nie widzą: podjazd, dokumenty, czas kierowcy przy załadunku.
    call_out_gr: i64,
}

impl RoadFreight {
    #[must_use]
    pub fn new(
        oracle: Arc<TrafficOracle>,
        catalog: Arc<VehicleCatalog>,
        sites: BTreeMap<SiteId, WorldCoord>,
        call_out_gr: i64,
    ) -> RoadFreight {
        RoadFreight {
            oracle,
            catalog,
            sites,
            call_out_gr,
        }
    }

    /// Klasa pojazdu dla tego zlecenia: **najmniejsza, która uwiezie ładunek za jednym
    /// razem**, a gdy takiej nie ma — największa dostępna, bo wtedy kursów będzie kilka
    /// i chodzi o to, żeby było ich jak najmniej.
    ///
    /// Nadwozie jest warunkiem twardym: mleko przewiezione wywrotką nie dojeżdża wcale.
    /// Ładowność i objętość są warunkiem miękkim — ładunek większy od skrzyni jedzie
    /// kilkoma kursami, a nie wcale.
    ///
    /// Reguła „najmniejsza, która się mieści", a nie „najmniejsza w ogóle": dwunastotonówka
    /// do trzech palet jest droższa w kilometrach niż furgonetka, ale furgonetka do
    /// dwunastu ton jest droższa **dziesięciokrotnie**, bo jedzie dziesięć razy.
    #[must_use]
    fn klasa_dla(&self, mass: Mass, req: &VehicleRequirements) -> Option<VehicleClassId> {
        // Ładunek zlecenia bywa większy niż deklaracja wymagań: przy trasie objazdowej
        // pierwszy odcinek wiezie towar wszystkich przystanków, a `min_payload` opisuje
        // tylko ten jeden. Bierzemy większą z dwóch liczb.
        let potrzeba = mass.0.max(req.min_payload.0);
        let mut mieszczace: Option<(i64, u16)> = None;
        let mut najwieksza: Option<(i64, u16)> = None;
        for i in 0..self.catalog.len() {
            let id = VehicleClassId(i as u16);
            let s = self.catalog.spec(id);
            if s.payload_g <= 0 || s.body != req.body {
                continue;
            }
            if najwieksza.is_none_or(|(p, _)| s.payload_g > p) {
                najwieksza = Some((s.payload_g, id.0));
            }
            if s.payload_g >= potrzeba
                && s.cargo_ml >= req.min_volume.0
                && mieszczace.is_none_or(|(p, _)| s.payload_g < p)
            {
                mieszczace = Some((s.payload_g, id.0));
            }
        }
        mieszczace.or(najwieksza).map(|(_, i)| VehicleClassId(i))
    }
}

impl FreightOracle for RoadFreight {
    fn quote(
        &self,
        from: SiteId,
        to: SiteId,
        mass: Mass,
        req: &VehicleRequirements,
    ) -> Option<FreightQuote> {
        let a = *self.sites.get(&from)?;
        let b = *self.sites.get(&to)?;
        let klasa = self.klasa_dla(mass, req)?;
        let spec = self.catalog.spec(klasa);

        // Ile kursów. Objętość liczy się razem z masą, bo skrzynki z chipsami
        // wypełniają naczepę przy śmiesznej masie — wąskim gardłem jest ten
        // z dwóch limitów, który akurat gryzie (to samo rozstrzygnięcie co
        // w `stock_fill`, M6 §6.4.3).
        let po_masie = dzielenie_w_gore(mass.0.max(0), spec.payload_g);
        let po_objetosci = dzielenie_w_gore(req.min_volume.0.max(0), spec.cargo_ml.max(1));
        let kursy = po_masie.max(po_objetosci).max(1);

        let ladunek_kursu = Mass(mass.0.max(0) / kursy);
        let brutto = Mass(spec.kerb_mass_g + ladunek_kursu.0);

        let (minuty, dlugosc_cm) = if a == b {
            (0u16, 0u64)
        } else {
            let od = self.oracle.nearest_node(a)?;
            let do_ = self.oracle.nearest_node(b)?;
            if od == do_ {
                // Dwa zakłady przy tej samej krawędzi. Trasy nie ma, ale przewóz jest —
                // odległość w linii prostej jest tu bliższa prawdy niż zero.
                (0u16, u64::from(odleglosc_cm(a, b)))
            } else {
                let noc = false;
                let q = RouteQuery::freight(od, do_, brutto, GODZINA_DOSTAW, noc);
                let r = self.oracle.route_freight(&q)?;
                let cm = self.oracle.route_length_cm(&r);
                (r.planned_minutes, cm)
            }
        };

        // Koszt liczony z centymetrów, nie z pełnych kilometrów: przy dzieleniu na
        // kilometry **przed** mnożeniem dostawa na 900 m kosztuje tyle co dostawa na
        // 100 m, czyli dokładnie ta różnica, o którą pyta porównanie milk-runu
        // z dostawą bezpośrednią, znika w zaokrągleniu.
        let koszt_kursu = i128::from(dlugosc_cm) * i128::from(spec.haul_gr_per_km) / 100_000
            + i128::from(self.call_out_gr);
        let koszt = koszt_kursu * i128::from(kursy);
        Some(FreightQuote {
            minutes: u32::from(minuty) * (kursy.min(i64::from(u32::MAX)) as u32),
            cost: Money(koszt.min(i128::from(i64::MAX)) as i64),
            distance_m: (dlugosc_cm / 100).min(u64::from(u32::MAX)) as u32,
            call_out: Money(self.call_out_gr),
        })
    }
}

fn dzielenie_w_gore(a: i64, b: i64) -> i64 {
    if b <= 0 {
        return 1;
    }
    (a + b - 1) / b
}

fn odleglosc_cm(a: WorldCoord, b: WorldCoord) -> u32 {
    let dx = i64::from(a.x) - i64::from(b.x);
    let dy = i64::from(a.y) - i64::from(b.y);
    ((dx.abs() + dy.abs()).max(0)).min(i64::from(u32::MAX)) as u32
}
