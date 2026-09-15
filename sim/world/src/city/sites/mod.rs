//! Etap 7 — gospodarka bazowa: firmy jako obiekty danych (M2 §5.8, WP13 i WP14).
//!
//! W M2 firma **nie ma zachowań**: nie produkuje, nie zatrudnia, nie wycenia. Jest
//! rekordem, który mówi „w tym budynku będzie huta o skali 1,4, z 320 stanowiskami
//! tych ról". Zachowania dokładają M6 (produkcja) i M7 (pełna `Firm`).
//!
//! ## Co się tu dzieje, w kolejności
//!
//! 1. **Normatywy ludnościowe** (`per_pop`) — handel, usługi publiczne i zieleń.
//!    Rozmieszczenie zachłanne po pokryciu ludności, nie losowe.
//! 2. **Wypełniacze stref nieprodukcyjnych** — handel, biura, urzędy, logistyka.
//!    Muszą stanąć przed bilansem, bo ich `consumes` jest częścią popytu (def(3) z §5.8).
//! 3. **Zakłady produkcyjne** — liczba wyprowadzona z popytu, rozmieszczenie
//!    po klastrach przemysłowych wg szablonów łańcuchów z `data/chains/`.
//! 4. **Wypełniacze stref przemysłowych** — reszta działek.
//! 5. **Domknięcie łańcuchów** (`supply_closure_check`) i bilans przepustowości.
//!
//! ## Korekta wobec §5.8: KROK 5 liczy się konstrukcyjnie, nie korekcyjnie
//!
//! Plan opisywał KROK 5 jako korektę po fakcie: policz `ratio`, przeskaluj `capacity_scale`
//! w zakresie 0,4–2,5, w razie czego dostaw zakład. To działa tylko wtedy, kiedy liczba
//! zakładów jest z grubsza właściwa **zanim** zacznie się korekta — a przy obsadzie
//! „każda działka rolna to gospodarstwo" nie jest: metropolia ma ok. 2 400 działek
//! rolnych wobec zapotrzebowania rzędu stu gospodarstw, więc nadwyżka jest kilkunastokrotna,
//! a dolny limit skali (0,4) nie ma jej jak zjeść. §5.8 nigdzie nie przewiduje **usuwania**
//! zakładu, bo milcząco zakłada, że obsada była od początku świadoma popytu.
//!
//! Robimy więc to, co plan zakłada, jawnie: liczbę zakładów każdego archetypu wyprowadzamy
//! z popytu (propagacja wstecz po hipergrafie receptur), a działki, których popyt nie
//! potrzebuje, dostają **wypełniacz** — warsztat, skład, punkt usługowy. Korekta z KROKU 5
//! zostaje i pilnuje zaokrągleń. Zapisane jako korekta I-2 w dokumencie podfazy.

mod closure;
mod data;
mod place;

use super::blocks::BlockSet;
use super::build::{self, BuildInput, BuildingSet, UnitIdx, UnitOccupant, Workplace};
use super::catalog::Catalog;
use super::districts::DistrictSet;
use super::gates::GateKind;
use super::grammar::{GrammarId, GrammarSet};
use super::parcels::{ParcelOwner, ParcelSet, ParcelStatus};
use super::poly;
use super::road::PolyArena;
use super::zoning::{ZoneKind, ZoneResult};
use super::CityPlan;
use magnat_core::{rng, Entity, FirmId, GoodId, RecipeId, ResourceKind, SiteId, StreamId, Tick};
use magnat_spatial::Vec2;
use magnat_voxel::EditQueue;
use serde::Deserialize;
use smallvec::SmallVec;
use std::num::NonZeroU32;
use std::ops::Range;
use std::path::Path;

pub use closure::{supply_closure_check, ClosureReport};
pub use data::{
    Archetype, ArchetypeSpec, ChainTemplate, FirmNames, SiteArchetypeId, SiteCatalog,
    SiteDataError, ARCHETYPE_SCHEMA_VERSION, CHAIN_SCHEMA_VERSION, FIRM_NAMES_SCHEMA_VERSION,
    RATIO_MAX, RATIO_MIN, SCALE_BASE, SCALE_MAX, SCALE_MIN,
};
pub use place::populate;

/// Sektor gospodarki. Grupuje archetypy do wypełniania stref i do nazw firm;
/// pełna klasyfikacja PKD to nie jest zadanie M2.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Deserialize)]
pub enum SectorId {
    Industry,
    Logistics,
    Retail,
    Services,
    Office,
    Public,
    Agriculture,
    Extraction,
    Green,
}

impl SectorId {
    pub const ALL: [SectorId; 9] = [
        SectorId::Industry,
        SectorId::Logistics,
        SectorId::Retail,
        SectorId::Services,
        SectorId::Office,
        SectorId::Public,
        SectorId::Agriculture,
        SectorId::Extraction,
        SectorId::Green,
    ];

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            SectorId::Industry => "industry",
            SectorId::Logistics => "logistics",
            SectorId::Retail => "retail",
            SectorId::Services => "services",
            SectorId::Office => "office",
            SectorId::Public => "public",
            SectorId::Agriculture => "agriculture",
            SectorId::Extraction => "extraction",
            SectorId::Green => "green",
        }
    }

    /// Czy zakład należy do miasta (decyzja 9.2/5). Budżet i polityka usługi — M8.
    #[must_use]
    pub const fn is_municipal(self) -> bool {
        matches!(self, SectorId::Public | SectorId::Green)
    }
}

// ── Wyjście Etapu 7 (kontrakt M2 §6) ─────────────────────────────────────────────────

#[derive(Clone, PartialEq, Debug)]
pub struct FirmSeed {
    /// Nazwa proceduralna — **nie jest** lokalizacją UI (CLAUDE.md).
    pub name: String,
    pub sector: SectorId,
    pub sites: SmallVec<[SiteId; 4]>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct SiteSeed {
    pub firm: FirmId,
    pub building: magnat_core::BuildingId,
    pub units: Range<u32>,
    pub archetype: SiteArchetypeId,
    /// Receptury z archetypu; puste dla handlu, usług i administracji.
    pub recipes: Vec<RecipeId>,
    /// 1000 = skala bazowa archetypu.
    pub capacity_scale: u16,
    pub workplaces: Range<u32>,
    /// Parcela zakładu — do karty inspekcji i do testów spójności.
    pub parcel: magnat_core::ParcelId,
}

#[must_use]
pub fn site_id(i: u32) -> SiteId {
    SiteId(Entity::new(i, NonZeroU32::new(1).expect("1 != 0")))
}

#[must_use]
pub fn firm_id(i: u32) -> FirmId {
    FirmId(Entity::new(i, NonZeroU32::new(1).expect("1 != 0")))
}

#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct SiteReport {
    pub firms: u32,
    pub sites: u32,
    /// Zakłady per sektor, w kolejności `SectorId::ALL`.
    pub by_sector: [u32; 9],
    /// Budynki dostawione przez Etap 7 (zieleń, wydobycie, naprawa domknięcia).
    pub buildings_added: u32,
    /// Działki, dla których Etap 7 chciał budynek, ale nic się nie zmieściło.
    pub buildings_failed: u32,
    /// Zabudowane parcele niemieszkalne bez zakładu — kryterium WP13 mówi „zero".
    pub parcels_without_site: u32,
    /// Normatywy ludnościowe: ile placówek oczekiwano i ile udało się postawić.
    pub norm_target: u32,
    pub norm_placed: u32,
    /// Zakłady postawione z szablonu łańcucha (a nie pojedynczo).
    pub from_chain: u32,
    /// Archetypy, których bilans potrzebował, a dla których zabrakło działki — i ile.
    /// Bez tej listy „podaż/popyt 0,00" nie mówi, czy zabrakło popytu, czy miejsca.
    pub unplaced: Vec<(String, u32)>,
    /// Zakłady postawione po **rozluźnieniu wymagania strefy** (KROK 4c po korekcie I-3).
    pub relaxed_zone: u32,
}

#[derive(Clone, Debug)]
pub struct SiteSet {
    pub firms: Vec<FirmSeed>,
    pub sites: Vec<SiteSeed>,
    /// Zakład stojący w budynku, po indeksie budynku. `None` = budynek bez zakładu
    /// (mieszkalny albo pusty).
    pub by_building: Vec<Option<u32>>,
    pub report: SiteReport,
    pub closure: ClosureReport,
}

impl SiteSet {
    #[must_use]
    pub fn site_of_building(&self, b: magnat_core::BuildingId) -> Option<&SiteSeed> {
        self.by_building
            .get(b.0.index() as usize)
            .and_then(|x| *x)
            .map(|i| &self.sites[i as usize])
    }
}

/// Strefy, w których stoi zakład. Mieszkaniówka odpada: sklep na parterze kamienicy jest
/// lokalem, nie zakładem z własnym budynkiem, i przypisanie mu `SiteSeed` obejmującego
/// całą kamienicę mówiłoby nieprawdę o tym, kto jest właścicielem mieszkań.
#[must_use]
pub const fn niemieszkalna(z: ZoneKind) -> bool {
    matches!(
        z,
        ZoneKind::Commercial
            | ZoneKind::Office
            | ZoneKind::IndustryLight
            | ZoneKind::IndustryHeavy
            | ZoneKind::Logistics
            | ZoneKind::Institutional
            | ZoneKind::Agriculture
            | ZoneKind::Green
            | ZoneKind::Extraction
    )
}
