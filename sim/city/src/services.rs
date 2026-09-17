//! Usługi publiczne: placówka, jakość i obwód (M8d WP7, PRD §10.3).
//!
//! # Czym placówka **nie** jest
//!
//! Nie jest firmą. Nie ma utargu, nie wystawia oferty i nie upada. Jest za to
//! zakładem w tym samym sensie, w jakim jest nim sklep — stoi na parceli, ma
//! `SiteId` i ma **prawdziwych ludzi na etatach**. Obsada bierze się z komponentu
//! `Employment` mieszkańców i nie jest liczbą wpisaną w dane: nauczyciel, który
//! umarł albo przeszedł na emeryturę, znika z obsady szkoły tak samo, jak znika
//! z obsady piekarni, a wakat wraca do puli, z której bierze migracja (`M3c`).
//!
//! # Jakość jest wynikiem, a nie polem
//!
//! `quality` liczy się co miesiąc z czterech liczb, których żadna nie jest
//! „jakością": pieniędzy na obsługiwaną osobę, obsady wobec etatów, stanu budynku
//! i obłożenia. Dlatego `DecisionReason::ServiceQuality` niesie wszystkie cztery —
//! szkoła niedofinansowana i szkoła przepełniona mają tę samą jakość z dwóch
//! różnych powodów, a naprawia się je dwiema różnymi decyzjami.
//!
//! # Obwód
//!
//! Pokrycie publikuje się do [`ServiceCoverage`] w `engine/core` — zasobu, który
//! czytają `sim/agents` (nauka dziecka, długość choroby) i `sim/economy` (ubytki
//! inwentaryzacyjne). Kierunek jest wymuszony grafem zależności: te crate'y stoją
//! **pod** `sim/city`, więc to piszący sięga do pola czytelnika, a nie odwrotnie —
//! ten sam wzorzec, którym `sim/events` nakłada parametry (`CE-4`).

use magnat_core::{
    DecisionReason, DistrictId, HashState, Money, ServiceCoverage, ServiceKind, SiteId,
    SpendCategory, StateHasher, Q, SERVICE_KIND_COUNT,
};

use crate::budget::{BudgetPolicy, CityBudget};
use crate::tuning::CityTuning;

/// Placówka publiczna (M8d §5.3).
#[derive(Clone, Debug)]
pub struct PublicService {
    pub kind: ServiceKind,
    /// Zwykły zakład na parceli — ten sam klucz, którym posługują się sklep M5
    /// i komponent `Employment` mieszkańca (`K-46`). Dzięki temu szkoła otwiera się
    /// tą samą kartą inspekcji co sklep (`K-62`).
    pub site: SiteId,
    pub district: DistrictId,
    /// Ilu ludzi placówka obsługuje bez przeciążenia: uczniów, łóżek, rewirów.
    pub capacity: u32,
    /// Ile etatów przewiduje normatyw — z liczby stanowisk pracy budynku (M2).
    pub staff_target: u32,
    /// Ile etatów jest obsadzonych. Liczone z ECS co miesiąc, nie trzymane na zapas.
    pub staff: u32,
    pub funding_per_month: Money,
    /// Stan budynku. Degraduje bez finansowania i wraca przy pełnym — jedyna
    /// wielkość w tej strukturze, która pamięta poprzedni miesiąc.
    pub condition: Q,
    pub quality: Q,
    /// Obłożenie: obsługiwani wobec pojemności, w punktach bazowych.
    pub utilization_bps: u32,
    /// Ilu ludzi tej placówce przypadło w ostatnim przeliczeniu — podstawa
    /// finansowania na osobę i obłożenia.
    pub served: u32,
}

impl PublicService {
    /// Kierunek wydatku publicznego, z którego ta placówka jest finansowana.
    ///
    /// Odwzorowanie jest **tutaj**, a nie w danych, bo to nie jest kalibracja:
    /// przychodnia finansowana z budżetu policji nie byłaby innym balansem, tylko
    /// innym modelem.
    #[must_use]
    pub fn spend_category(kind: ServiceKind) -> SpendCategory {
        match kind {
            ServiceKind::School => SpendCategory::Education,
            ServiceKind::Clinic | ServiceKind::Hospital => SpendCategory::Health,
            ServiceKind::Police => SpendCategory::Police,
            ServiceKind::Fire => SpendCategory::Fire,
            ServiceKind::Waste => SpendCategory::Waste,
            ServiceKind::Park => SpendCategory::Parks,
            ServiceKind::Office => SpendCategory::Administration,
        }
    }

    /// Obsada wobec etatów, w punktach bazowych. Placówka bez etatów w normatywie
    /// jest obsadzona w pełni — nie jest placówką stojącą, tylko placówką bezobsługową.
    #[must_use]
    pub fn staff_bp(&self) -> u32 {
        if self.staff_target == 0 {
            return 10_000;
        }
        (self.staff * 10_000 / self.staff_target).min(10_000)
    }
}

/// Wszystkie placówki miasta plus indeksy, którymi się je znajduje (M8d WP7).
///
/// Indeks `SiteId → placówka` jest wymaganiem wstecznym z decyzji właściciela
/// produktu: gracz klika w budynek szkoły i oczekuje karty, a kartę otwiera się
/// po `SiteId`. Sama lista by nie wystarczyła.
#[derive(Clone, Default, Debug)]
pub struct PublicServices {
    services: Vec<PublicService>,
    /// `SiteId` → indeks, po kluczu rosnąco (00 §3.2).
    by_site: std::collections::BTreeMap<u64, u32>,
    coverage: ServiceCoverage,
}

impl PublicServices {
    #[must_use]
    pub fn new(services: Vec<PublicService>, districts: usize) -> PublicServices {
        let mut by_site = std::collections::BTreeMap::new();
        for (i, s) in services.iter().enumerate() {
            by_site.insert(s.site.0.to_bits(), i as u32);
        }
        PublicServices {
            services,
            by_site,
            coverage: ServiceCoverage::new(districts),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.services.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.services.is_empty()
    }

    #[must_use]
    pub fn all(&self) -> &[PublicService] {
        &self.services
    }

    pub fn all_mut(&mut self) -> &mut [PublicService] {
        &mut self.services
    }

    /// Placówka stojąca na tym zakładzie — wejście karty inspekcji (`K-62`).
    #[must_use]
    pub fn by_site(&self, site: SiteId) -> Option<&PublicService> {
        self.by_site
            .get(&site.0.to_bits())
            .map(|i| &self.services[*i as usize])
    }

    #[must_use]
    pub fn coverage(&self) -> &ServiceCoverage {
        &self.coverage
    }

    /// Ile placówek tego rodzaju stoi w mieście — do raportu i do podziału budżetu.
    #[must_use]
    pub fn count_of(&self, kind: ServiceKind) -> u32 {
        self.services.iter().filter(|s| s.kind == kind).count() as u32
    }
}

impl HashState for PublicServices {
    /// Wchodzi wszystko, co się w trakcie zmienia. Rodzaj, zakład, dzielnica
    /// i pojemność są danymi wejściowymi mostu — takie same w obu przebiegach.
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.services.len() as u64);
        for s in &self.services {
            h.write_u64(s.site.0.to_bits());
            h.write_u32(s.staff);
            h.write_i64(s.funding_per_month.get());
            h.write_u8(s.condition.get());
            h.write_u8(s.quality.get());
            h.write_u32(s.utilization_bps);
            h.write_u32(s.served);
        }
        self.coverage.hash_state(h);
    }
}

/// Ludność obsługiwana przez dzielnicę — wejście miesięcznego przeliczenia.
///
/// Osobny typ zamiast `&[u32]`, żeby nie dało się pomylić kolejności z listą
/// placówek: obie są wektorami `u32` i obie indeksują się czym innym.
#[derive(Clone, Default, Debug)]
pub struct DistrictPopulation(pub Vec<u32>);

impl DistrictPopulation {
    #[must_use]
    pub fn at(&self, d: DistrictId) -> u32 {
        self.0.get(d.0 as usize).copied().unwrap_or(0)
    }

    #[must_use]
    pub fn total(&self) -> u32 {
        self.0.iter().sum()
    }
}

/// Miesięczne przeliczenie jakości placówek (§5.3, `sys_update_service_quality`).
///
/// Zwraca powody do dziennika — po jednym na placówkę, z rozbiciem na cztery
/// czynniki. Powód bez rozbicia byłby liczbą bez odpowiedzi na „dlaczego tyle".
pub fn update_quality(
    services: &mut PublicServices,
    budget: &CityBudget,
    policy: &BudgetPolicy,
    pop: &DistrictPopulation,
    tuning: &CityTuning,
    month_of_year: u8,
) -> Vec<(SiteId, DecisionReason)> {
    let plan = budget.plan_base_month();
    // Ile pieniędzy przypada na rodzaj usługi w tym miesiącu i jak dzieli się to
    // między placówki: **proporcjonalnie do pojemności**, bo szpital na tysiąc łóżek
    // kosztuje więcej niż przychodnia na dwadzieścia. Podział po równo zagłodziłby
    // duże placówki i przelałby małe.
    let mut pojemnosc = [0u64; SERVICE_KIND_COUNT];
    for s in services.all() {
        pojemnosc[s.kind.as_index()] += u64::from(s.capacity.max(1));
    }
    let udzialy = policy.shares();
    let mut pula = [0i64; SERVICE_KIND_COUNT];
    for k in ServiceKind::ALL {
        // Udział kierunku wydatku dzieli się jeszcze między rodzaje usług,
        // które z niego żyją — zdrowie niesie i przychodnie, i szpitale.
        let bp = udzialy[PublicService::spend_category(*k).as_index()];
        pula[k.as_index()] = plan.get() * i64::from(bp) / 10_000;
    }
    // Zdrowie płaci za dwa rodzaje z jednej koperty, więc dzieli się ją między nie
    // w tej samej proporcji co resztę: po pojemności.
    let zdrowie = pula[ServiceKind::Clinic.as_index()];
    let suma_zdrowia =
        pojemnosc[ServiceKind::Clinic.as_index()] + pojemnosc[ServiceKind::Hospital.as_index()];
    if suma_zdrowia > 0 {
        pula[ServiceKind::Clinic.as_index()] =
            zdrowie * pojemnosc[ServiceKind::Clinic.as_index()] as i64 / suma_zdrowia as i64;
        pula[ServiceKind::Hospital.as_index()] =
            zdrowie * pojemnosc[ServiceKind::Hospital.as_index()] as i64 / suma_zdrowia as i64;
    }

    // Ilu ludzi przypada na placówkę: ludność jej dzielnicy podzielona przez liczbę
    // placówek tego rodzaju w tej dzielnicy, a jeśli w dzielnicy nie ma żadnej —
    // ludność miasta podzielona przez wszystkie placówki tego rodzaju.
    let mut w_dzielnicy: std::collections::BTreeMap<(u8, u16), u32> =
        std::collections::BTreeMap::new();
    for s in services.all() {
        *w_dzielnicy
            .entry((s.kind.as_index() as u8, s.district.0))
            .or_insert(0) += 1;
    }
    // Ludność dzielnic, w których **nie stoi** ani jedna placówka tego rodzaju.
    // Ci ludzie też są obsługiwani — tyle że nie przez sąsiada, a przez miasto.
    // Bez tego kroku szpital liczył sobie wyłącznie mieszkańców swojej dzielnicy,
    // a dziewięć pozostałych nie było obsługiwane przez nikogo: obłożenie wychodziło
    // dziesięciokrotnie za niskie, a finansowanie na osobę dziesięciokrotnie za wysokie.
    let mut poza: [u32; SERVICE_KIND_COUNT] = [0; SERVICE_KIND_COUNT];
    let mut wszystkich: [u32; SERVICE_KIND_COUNT] = [0; SERVICE_KIND_COUNT];
    for s in services.all() {
        wszystkich[s.kind.as_index()] += 1;
    }
    for k in ServiceKind::ALL {
        if wszystkich[k.as_index()] == 0 {
            continue;
        }
        for d in 0..pop.0.len() {
            if !w_dzielnicy.contains_key(&(k.as_index() as u8, d as u16)) {
                poza[k.as_index()] += pop.at(DistrictId(d as u16));
            }
        }
    }

    let q = &tuning.quality;
    let mut powody = Vec::with_capacity(services.len());
    for s in services.all_mut() {
        let ile_tu = w_dzielnicy
            .get(&(s.kind.as_index() as u8, s.district.0))
            .copied()
            .unwrap_or(1)
            .max(1);
        let wszystkie = wszystkich[s.kind.as_index()].max(1);
        s.served = pop.at(s.district) / ile_tu + poza[s.kind.as_index()] / wszystkie;
        s.utilization_bps = u32::try_from(
            u64::from(s.served)
                .saturating_mul(10_000)
                .checked_div(u64::from(s.capacity))
                .unwrap_or(10_000),
        )
        .unwrap_or(u32::MAX);

        // Finansowanie: udział placówki w puli rodzaju, wobec normy na osobę.
        let udzial = if pojemnosc[s.kind.as_index()] == 0 {
            0
        } else {
            pula[s.kind.as_index()].saturating_mul(i64::from(s.capacity.max(1)))
                / pojemnosc[s.kind.as_index()] as i64
        };
        s.funding_per_month = Money(udzial);
        let norma = tuning
            .funding_ref(s.kind)
            .saturating_mul(i64::from(s.served.max(1)));
        let funding_bp = if norma <= 0 {
            10_000
        } else {
            (udzial * 10_000 / norma).clamp(0, 10_000) as u32
        };

        // Stan budynku rusza się raz na rok gry, w styczniu: ubytek zawsze, naprawa
        // proporcjonalnie do finansowania. Przy pełnym finansowaniu wychodzi zero,
        // przy połowicznym budynek schodzi — i to jest różnica, która narasta.
        if month_of_year == 1 {
            let naprawa = u32::from(q.condition_repair_per_year) * funding_bp / 10_000;
            let nowy = i32::from(s.condition.get()) - i32::from(q.condition_decay_per_year)
                + naprawa as i32;
            s.condition = Q::new(nowy.clamp(0, 100) as u8);
        }

        let staff_bp = s.staff_bp();
        // Obłożenie: pełne przy pojemności, zero przy `load_zero_bp`. Placówka
        // niedociążona nie dostaje premii — pusta szkoła nie uczy lepiej.
        let load_bp = if s.utilization_bps <= 10_000 {
            10_000
        } else {
            let ponad = s.utilization_bps - 10_000;
            let zakres = q.load_zero_bp.saturating_sub(10_000).max(1);
            10_000u32.saturating_sub(ponad.min(zakres) * 10_000 / zakres)
        };

        let mieszanka = u64::from(funding_bp) * u64::from(q.w_funding)
            + u64::from(staff_bp) * u64::from(q.w_staff)
            + u64::from(s.condition.get()) * 100 * u64::from(q.w_condition)
            + u64::from(load_bp) * u64::from(q.w_load);
        // Wagi sumują się do 100 (walidator), więc mianownikiem jest 100 × 100 bp.
        s.quality = Q::new((mieszanka / 100 / 100) as u8);

        powody.push((
            s.site,
            DecisionReason::ServiceQuality {
                kind: s.kind,
                district: s.district,
                quality: s.quality,
                funding_bp: u16::try_from(funding_bp).unwrap_or(u16::MAX),
                staff_bp: u16::try_from(staff_bp).unwrap_or(u16::MAX),
                load_bp: u16::try_from(s.utilization_bps.min(u32::from(u16::MAX)))
                    .unwrap_or(u16::MAX),
            },
        ));
    }
    powody
}

/// Publikacja pokrycia obwodowego (§5.3, `sys_publish_service_coverage`).
///
/// Dzielnica z placówką bierze **najlepszą** z tych, które w niej stoją; dzielnica
/// bez placówki — najlepszą w mieście, przyciętą o `spillover_bp`. To jest zanik
/// dwustopniowy i jego sufit nazywa `data/tuning/city.ron`: pełny zanik po czasie
/// przejazdu wymaga macierzy odległości między dzielnicami, a `sim/city` nie widzi
/// `TravelOracle` (należy do M4 i mieszka po stronie mostu).
pub fn publish_coverage(services: &mut PublicServices, tuning: &CityTuning) {
    let mut lokalne: std::collections::BTreeMap<(u8, u16), u8> = std::collections::BTreeMap::new();
    let mut najlepsze = [0u8; SERVICE_KIND_COUNT];
    for s in services.all() {
        let klucz = (s.kind.as_index() as u8, s.district.0);
        let e = lokalne.entry(klucz).or_insert(0);
        *e = (*e).max(s.quality.get());
        najlepsze[s.kind.as_index()] = najlepsze[s.kind.as_index()].max(s.quality.get());
    }
    let dzielnic = services.coverage.districts();
    services.coverage.clear();
    for d in 0..dzielnic {
        for k in ServiceKind::ALL {
            let klucz = (k.as_index() as u8, d as u16);
            let q = match lokalne.get(&klucz) {
                Some(v) => *v,
                None => {
                    let bp = tuning.spillover(*k);
                    (u32::from(najlepsze[k.as_index()]) * bp / 10_000) as u8
                }
            };
            services
                .coverage
                .set(DistrictId(d as u16), *k, Q::new(q));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::Entity;
    use std::num::NonZeroU32;

    fn placowka(kind: ServiceKind, d: u16, idx: u32) -> PublicService {
        PublicService {
            kind,
            site: SiteId(Entity::new(idx, NonZeroU32::new(1).unwrap())),
            district: DistrictId(d),
            capacity: 1000,
            staff_target: 20,
            staff: 20,
            funding_per_month: Money::ZERO,
            condition: Q::new(80),
            quality: Q::new(0),
            utilization_bps: 0,
            served: 0,
        }
    }

    #[test]
    fn pokrycie_dzielnicy_bez_placowki_jest_przyciete() {
        let t = CityTuning::load_default().expect("tuning");
        let mut s = PublicServices::new(vec![placowka(ServiceKind::School, 0, 1)], 3);
        s.all_mut()[0].quality = Q::new(80);
        publish_coverage(&mut s, &t);
        assert_eq!(s.coverage().at(DistrictId(0), ServiceKind::School).get(), 80);
        let obok = s.coverage().at(DistrictId(1), ServiceKind::School).get();
        assert!(obok > 0 && obok < 80, "zanik poza dzielnicą: {obok}");
    }

    #[test]
    fn brak_obsady_scina_jakosc() {
        let t = CityTuning::load_default().expect("tuning");
        let mut s = PublicServices::new(
            vec![placowka(ServiceKind::School, 0, 1), placowka(ServiceKind::School, 1, 2)],
            2,
        );
        s.all_mut()[1].staff = 0;
        let budzet = CityBudget::new(magnat_economy::AccountId(0));
        let pop = DistrictPopulation(vec![1000, 1000]);
        update_quality(&mut s, &budzet, &BudgetPolicy::default(), &pop, &t, 2);
        assert!(
            s.all()[0].quality.get() > s.all()[1].quality.get(),
            "obsadzona {} vs pusta {}",
            s.all()[0].quality.get(),
            s.all()[1].quality.get()
        );
    }
}
