//! Zakład jako model fizyczny (M6b §5.5, WP4).
//!
//! Podział pliku: tutaj **stan zakładu** — linie, harmonogram, liczniki, rampa i ślad
//! decyzji; [`line`] trzyma samą linię i jej harmonogram, [`utility`] licznik mediów,
//! [`dock`] kolejkę rampy, [`produce`] jedyną funkcję, która to wszystko rusza.
//!
//! **Zakłady mieszkają w zasobie, nie w komponentach ECS** — i to jest korekta wobec
//! §6.1 dokumentu fazy, który zapowiadał `ProductionLine` i `Dock` jako komponenty.
//! Powód jest ten sam, który `AD-7` wpisał dla partii: `World::resource_mut` pożycza
//! **cały** świat, a `advance_production` w każdej minucie dotyka i linii, i areny
//! partii w `Store`. Komponent plus zasób nie dają się zmutować naraz, więc albo zakład
//! idzie do zasobu, albo produkcja przestaje być jedną funkcją — a jedna funkcja dla
//! wszystkich trzech poziomów LOD jest wymaganiem, nie wygodą (§7.6, `R8`).
//! `K-29` dopuszcza to wprost: zasób haszuje się przez `register_resource_hash`.

use magnat_core::{
    DecisionReason, GoodId, HashState, Mass, Money, SimMinute, SiteId, StateHasher, Volume, Q,
};
use std::collections::BTreeMap;

pub mod dock;
pub mod line;
pub mod produce;
pub mod utility;

pub use dock::{Dock, DockEntry, SiteDwellResponse, VehicleArrivedAtSite, MAX_BAYS};
pub use line::{
    BreakCause, Charge, LineState, PlannedRun, ProductionLine, ProductionSchedule, Shift,
};
pub use produce::{advance_production, ProductionCtx, ProductionReport};
pub use utility::UtilityMeter;

use crate::batch::SlotId;
use crate::store::WarehouseRole;

/// Ile powodów decyzji zakład pamięta. Karta inspekcji pokazuje ostatnie kilka —
/// pierścień jest po to, żeby pamięć zakładu nie rosła przez sto lat gry.
pub const REASON_RING: usize = 8;

/// Emisje zakładu, kumulowane od startu świata. M8 je konsumuje, M6 nie liczy skutków.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EmissionTotals {
    pub pm_g: i64,
    pub co2_g: i64,
    pub wastewater: Volume,
    /// Największy hałas, jaki zakład wydał — do nakładki hałasu M8, nie do sumy.
    pub peak_noise_db: u8,
    /// Ile gramów pyłu poszło w ostatniej minucie — to z tego liczy się skala
    /// `emission` w snapshocie renderu (M6 §6.4.3).
    pub pm_g_last_minute: i64,
}

impl HashState for EmissionTotals {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_i64(self.pm_g);
        h.write_i64(self.co2_g);
        self.wastewater.hash_state(h);
        h.write_u8(self.peak_noise_db);
        h.write_i64(self.pm_g_last_minute);
    }
}

/// Faktura za medium: zakład, dostawca, **rodzaj medium** i kwota.
///
/// Struktura, a nie trójka z §6.1 (`AP-5`): księgujący potrzebuje rodzaju, bo
/// `TxKind::Utility` go niesie, a bez niego rachunek zakładu pokazywałby prąd i wodę
/// jako jedną pozycję „media" — czyli dokładnie tę informację, po którą gracz
/// otwiera kartę zakładu, gdy rachunek urósł.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct UtilityBill {
    pub site: SiteId,
    pub supplier: magnat_core::FirmId,
    pub kind: magnat_core::UtilityService,
    pub amount: Money,
    /// Rozliczone zużycie w **tysięcznych jednostki rozliczeniowej** (kWh dla
    /// energii, m³ dla cieczy). Osobno od kwoty, bo **akcyza od energii jest
    /// kwotowa** (grosze za kWh) i liczy się od zużycia, a nie od rachunku (M8b).
    /// W tysięcznych, żeby obcięcie ułamka nie zaniżało podstawy przy każdej
    /// fakturze w tę samą stronę.
    pub units_milli: i64,
}

/// Zakład: linie, harmonogram, magazyny, liczniki i rampa.
#[derive(Clone, Debug)]
pub struct PlantSite {
    pub site: SiteId,
    /// Właściciel — trafia do `Batch::producer`, czyli do śladu „kto to zrobił".
    /// Decyzje właściciela (ceny, inwestycje, zmiany) należą do M7; M6 zna go z nazwy.
    pub owner: magnat_core::FirmId,
    pub lines: Vec<ProductionLine>,
    pub schedule: ProductionSchedule,
    pub meters: Vec<UtilityMeter>,
    pub dock: Dock,
    /// Sloty magazynu wejściowego i wyjściowego w [`Store`](crate::Store).
    /// Uchwyty, nie partie — `AE-1`: zakład sięga do towaru **przez magazyn**.
    pub inputs: Vec<SlotId>,
    pub outputs: Vec<SlotId>,
    /// Poziom technologii zakładu — wejście [`QualityModel`](crate::QualityModel).
    /// Do M10 stałe; M10 podmieni je drzewem technologii, nie zmieniając kształtu.
    pub tech: Q,
    pub emissions: EmissionTotals,
    /// Straty per kategoria — histogram do panelu zakładu, indeksowany
    /// `LossKind::as_index()`.
    pub losses: [i64; magnat_core::LOSS_KIND_COUNT],
    /// Kaskada niedoboru per wejście (§5.7). Mieszka **w zakładzie**, a nie w osobnej
    /// mapie `(SiteId, GoodId)`, bo `advance_production` pyta o obniżenie produkcji
    /// w każdej minucie i osobna mapa byłaby drugim wyszukiwaniem po tym samym kluczu.
    pub shortage: Vec<crate::shortage::ShortageState>,
    /// Złoże, na którym zakład stoi (M6d §5.10). `None` dla wszystkiego, co przetwarza
    /// — czyli dla większości miasta. Receptura `Extraction` w zakładzie bez tego pola
    /// nie ma skąd wziąć masy i stoi na `Starved`; to jest właściwa odpowiedź, a nie
    /// błąd danych, bo wypełniacz strefy przemysłowej naprawdę nie ma czego kopać.
    pub mining: Option<crate::mining::MiningSite>,
    /// Pokrycie etatowe zakładu w promilach: 1000 = obsada kompletna i w formie.
    ///
    /// **Pisarzem jest M7** (`magnat_firms::FirmSystem`), czytelnikiem `sprobuj_start`.
    /// Do M7a pole stało na 1000 i było neutralne — zakład produkował tyle, ile miał
    /// wsadu, niezależnie od tego, czy ktokolwiek w nim pracował. Teraz linia bez ludzi
    /// stoi, a linia z połową załogi robi połowę szarży; jakość obsady to osobna sprawa
    /// i liczy ją `Shift::skill` (M7b).
    ///
    /// Promile, a nie procenty: przy dziesięciu tysiącach firm różnica między 995
    /// a 1000 to jest różnica, której nie chcemy zgubić na zaokrągleniu w każdej
    /// minucie każdej linii.
    pub labor_pct: u16,
    /// Mnożnik zdarzeniowy zdolności produkcyjnej, w punktach bazowych
    /// (10 000 = bez zmian). Pisze go **wyłącznie** `sim/events` (M8c §5.5),
    /// czytelnik jest jeden: `sprobuj_start`.
    ///
    /// Osobne pole od `labor_pct`, choć w kernelu mnoży się tak samo — bo
    /// `labor_pct` przepisuje **co dobę** rynek pracy M7 z faktycznej obsady,
    /// więc strajk wpisany tam zniknąłby przy najbliższym przeliczeniu.
    /// Znaczenie jest też inne: „nie ma komu" i „nie ma czym" to dla gracza
    /// dwa różne zdania o tym samym zakładzie, a powód zdarzenia mówi które.
    pub event_output_bps: u16,
    reasons: Vec<(SimMinute, DecisionReason)>,
}

impl PlantSite {
    /// Obsada kompletna — wartość neutralna dla przepustowości.
    pub const FULL_LABOR: u16 = 1000;

    /// Brak zdarzenia dotykającego zakładu — wartość neutralna mnożnika M8c.
    pub const NO_EVENT: u16 = 10_000;

    /// Pokrycie etatowe po uwzględnieniu zdarzeń świata.
    ///
    /// Mnożenie, a nie minimum, z tego samego powodu co przy wsadzie i obsadzie
    /// w `sprobuj_start`: zakład, w którym połowa załogi strajkuje, a druga połowa
    /// pracuje na zepsutej linii, robi ćwierć szarży, bo brakuje mu obu rzeczy.
    #[must_use]
    pub fn effective_labor_pct(&self) -> u16 {
        let v = u32::from(self.labor_pct) * u32::from(self.event_output_bps) / 10_000;
        u16::try_from(v).unwrap_or(u16::MAX)
    }

    #[must_use]
    pub fn new(site: SiteId, owner: magnat_core::FirmId, dock: Dock) -> PlantSite {
        PlantSite {
            site,
            owner,
            lines: Vec::new(),
            schedule: ProductionSchedule::default(),
            meters: Vec::new(),
            dock,
            inputs: Vec::new(),
            outputs: Vec::new(),
            tech: Q::new(50),
            emissions: EmissionTotals::default(),
            losses: [0; magnat_core::LOSS_KIND_COUNT],
            shortage: Vec::new(),
            mining: None,
            // Zakład bez przypisanej firmy pracuje pełną parą — inaczej każdy test M6
            // musiałby zakładać firmę, żeby cokolwiek wyprodukować.
            labor_pct: PlantSite::FULL_LABOR,
            event_output_bps: PlantSite::NO_EVENT,
            reasons: Vec::new(),
        }
    }

    /// Licznik danego medium, jeśli zakład go ma.
    #[must_use]
    pub fn meter(&self, kind: magnat_core::UtilityService) -> Option<&UtilityMeter> {
        self.meters.iter().find(|m| m.kind == kind)
    }

    pub fn meter_mut(&mut self, kind: magnat_core::UtilityService) -> Option<&mut UtilityMeter> {
        self.meters.iter_mut().find(|m| m.kind == kind)
    }

    /// Czy medium jest dostępne: brak licznika znaczy „zakład go nie potrzebuje",
    /// a licznik odcięty — „potrzebuje i nie ma". Te dwie rzeczy nie mogą znaczyć
    /// tego samego, bo pierwsza jest normą, a druga zatrzymuje produkcję.
    #[must_use]
    pub fn has_utility(&self, kind: magnat_core::UtilityService) -> bool {
        self.meter(kind).is_none_or(|m| !m.cut_off)
    }

    /// Do ilu procent zeszła produkcja z powodu niedoboru tego wejścia. 100 = pełna.
    #[must_use]
    pub fn throttle_pct(&self, good: GoodId) -> u8 {
        self.shortage
            .iter()
            .find(|s| s.good == good)
            .map_or(100, |s| s.throttle_pct)
    }

    /// Stopień kaskady dla wejścia — `shortage_stage` z §6.1 dokumentu fazy.
    #[must_use]
    pub fn stage(&self, good: GoodId) -> crate::shortage::ShortageStage {
        self.shortage
            .iter()
            .find(|s| s.good == good)
            .map_or(crate::shortage::ShortageStage::Ok, |s| s.stage)
    }

    /// Wstawia rozstrzygnięty ładunek szczebla kaskady (M6c).
    ///
    /// Kaskada buduje `SpotSearch { rfq }` i `Importing { eta }` z **zaślepkami** —
    /// `RfqId::default()` i czasem, którego nie ma z czego wyprowadzić, dopóki nie ma
    /// rynku. Prawdziwe wartości zna dopiero rynek i to on je tu wpisuje, w tej samej
    /// minucie, w której kaskada je zażądała. Zmiana **nie** zapisuje `DecisionReason`:
    /// powód przejścia między szczeblami zapisał już `shortage::review`, a drugi wpis
    /// o tym samym przejściu byłby duplikatem w karcie inspekcji.
    ///
    /// Brak wpisu dla tego towaru znaczy, że kaskada go nie zgłaszała — wtedy nic się
    /// nie dzieje, zamiast powstawać szczebel bez historii.
    pub fn set_stage(&mut self, good: GoodId, stage: crate::shortage::ShortageStage) {
        if let Some(s) = self.shortage.iter_mut().find(|s| s.good == good) {
            s.stage = stage;
        }
    }

    /// Zapisuje powód decyzji w pierścieniu zakładu — 00 §7 i bramka 5 fazy.
    pub fn note(&mut self, at: SimMinute, r: DecisionReason) {
        if self.reasons.len() == REASON_RING {
            self.reasons.remove(0);
        }
        self.reasons.push((at, r));
    }

    /// Ostatnie powody, od najstarszego. To jest treść karty inspekcji zakładu.
    #[must_use]
    pub fn reasons(&self) -> &[(SimMinute, DecisionReason)] {
        &self.reasons
    }

    /// Obłożenie zakładu, 0..=255 (M6 §6.4.3). Normalizowane do **własnej** nominalnej
    /// wydajności: `activity == 0` znaczy „nic się nie rusza", a nie „brak danych".
    #[must_use]
    pub fn activity(&self) -> u8 {
        let mianownik: i128 = self
            .lines
            .iter()
            .map(|l| i128::from(l.nominal_throughput.0))
            .sum();
        if mianownik == 0 {
            return 0;
        }
        let licznik: i128 = self
            .lines
            .iter()
            .map(|l| i128::from(l.nominal_throughput.0) * i128::from(l.state.activity_permille()))
            .sum();
        (255 * licznik / (mianownik * 1000)).clamp(0, 255) as u8
    }

    /// Bit `SITE_FAULT` w snapshocie: awaria linii **albo** odcięty licznik. Zakład
    /// w awarii jest cichy, ale ma się dać odróżnić od takiego, który nie ma zmiany.
    #[must_use]
    pub fn is_fault(&self) -> bool {
        self.lines.iter().any(|l| l.state.is_fault()) || self.meters.iter().any(|m| m.cut_off)
    }
}

impl HashState for PlantSite {
    fn hash_state(&self, h: &mut StateHasher) {
        self.site.entity().hash_state(h);
        self.owner.entity().hash_state(h);
        h.write_u32(self.lines.len() as u32);
        for l in &self.lines {
            l.hash_state(h);
        }
        self.schedule.hash_state(h);
        h.write_u32(self.meters.len() as u32);
        for m in &self.meters {
            m.hash_state(h);
        }
        self.dock.hash_state(h);
        h.write_u32(self.inputs.len() as u32);
        for s in &self.inputs {
            h.write_u32(s.0);
        }
        h.write_u32(self.outputs.len() as u32);
        for s in &self.outputs {
            h.write_u32(s.0);
        }
        self.tech.hash_state(h);
        self.emissions.hash_state(h);
        for l in &self.losses {
            h.write_i64(*l);
        }
        h.write_u32(self.shortage.len() as u32);
        for s in &self.shortage {
            s.hash_state(h);
        }
        match self.mining {
            Some(m) => {
                h.write_u32(m.deposit.0);
                h.write_u8(m.concentration_pct);
                h.write_u16(m.depth_m);
            }
            None => h.write_u32(u32::MAX),
        }
        // Pokrycie etatowe jest stanem: zmienia to, ile zakład wyprodukuje.
        h.write_u16(self.labor_pct);
        h.write_u16(self.event_output_bps);
        h.write_u32(self.reasons.len() as u32);
        for (at, r) in &self.reasons {
            at.hash_state(h);
            h.write_u16(r.discriminant());
        }
    }
}

/// Wszystkie zakłady miasta.
///
/// `BTreeMap`, nie `HashMap` — 00 §3.2 zabrania iterowania po `HashMap` w kodzie
/// symulacji, a po zakładach iteruje się w każdym ticku.
#[derive(Default)]
pub struct Plant {
    sites: BTreeMap<u32, PlantSite>,
}

impl Plant {
    #[must_use]
    pub fn new() -> Plant {
        Plant::default()
    }

    pub fn insert(&mut self, s: PlantSite) -> SiteId {
        let id = s.site;
        self.sites.insert(id.entity().index(), s);
        id
    }

    #[must_use]
    pub fn get(&self, site: SiteId) -> Option<&PlantSite> {
        self.sites.get(&site.entity().index())
    }

    pub fn get_mut(&mut self, site: SiteId) -> Option<&mut PlantSite> {
        self.sites.get_mut(&site.entity().index())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.sites.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sites.is_empty()
    }

    /// Zakłady w kolejności indeksu encji — jedyny dozwolony porządek iteracji.
    pub fn iter(&self) -> impl Iterator<Item = (SiteId, &PlantSite)> {
        self.sites.values().map(|s| (s.site, s))
    }

    pub fn sites(&self) -> impl Iterator<Item = SiteId> + '_ {
        self.sites.values().map(|s| s.site)
    }

    /// Faktury za media, wystawiane raz na miesiąc. Zwraca listę
    /// `(zakład, dostawca, kwota)` — księguje je wołający, bo `sim/supply` nie zależy
    /// od `sim/economy` i zależeć nie może (kierunek jest odwrotny od M6a).
    pub fn bill_utilities(&mut self, until: SimMinute) -> Vec<UtilityBill> {
        let mut faktury = Vec::new();
        for s in self.sites.values_mut() {
            for m in &mut s.meters {
                let (amount, units_milli) = m.bill(until);
                if amount.0 != 0 {
                    faktury.push(UtilityBill {
                        site: s.site,
                        supplier: m.supplier,
                        kind: m.kind,
                        amount,
                        units_milli,
                    });
                }
            }
        }
        faktury
    }
}

impl HashState for Plant {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.sites.len() as u32);
        for (k, s) in &self.sites {
            h.write_u32(*k);
            s.hash_state(h);
        }
    }
}

/// Zapełnienie magazynów zakładu, 0..=255 (M6 §6.4.3): **wąskie gardło pojemności**,
/// czyli `max` ze stosunku masy i objętości, z wyłączeniem półki.
///
/// `max`, a nie sam wolumen, bo wąskie gardło jest różne dla różnych towarów: skrzynki
/// z chipsami wypełniają objętość przy śmiesznej masie, a silos zboża odwrotnie.
/// Półka jest wyłączona, bo ma własny widok w panelu sklepu — wliczenie jej dałoby
/// regały wyglądające na pełne dokładnie wtedy, gdy sklepowi kończy się towar.
#[must_use]
pub fn stock_fill(store: &crate::Store, site: &PlantSite) -> u8 {
    let mut najgorszy = 0i128;
    for slot in site.inputs.iter().chain(site.outputs.iter()) {
        let Some(sl) = store.slot(*slot) else {
            continue;
        };
        if sl.role == WarehouseRole::Shelf {
            continue;
        }
        let po_masie = udzial(sl.used_mass(), sl.cap_mass);
        let po_objetosci = udzial_v(sl.used_volume(), sl.cap_volume);
        najgorszy = najgorszy.max(po_masie).max(po_objetosci);
    }
    najgorszy.clamp(0, 255) as u8
}

fn udzial(uzyte: Mass, cap: Mass) -> i128 {
    if cap.0 <= 0 {
        return 0;
    }
    255 * i128::from(uzyte.0) / i128::from(cap.0)
}

fn udzial_v(uzyte: Volume, cap: Volume) -> i128 {
    if cap.0 <= 0 {
        return 0;
    }
    255 * i128::from(uzyte.0) / i128::from(cap.0)
}

/// Masa towaru we wszystkich magazynach wejściowych zakładu — pytanie kaskady niedoboru
/// i polityki zapasów.
#[must_use]
pub fn input_stock(store: &crate::Store, site: &PlantSite, good: GoodId) -> Mass {
    Mass(site.inputs.iter().map(|s| store.stock_of(*s, good).0).sum())
}
