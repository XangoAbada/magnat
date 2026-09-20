//! Relacje z dostawcami: zaufanie, rabat preferencyjny i priorytet w niedoborze
//! (M10e WP10.13, PRD §7.9).
//!
//! ## Dlaczego to mieszka w `sim/supply`, a nie w `sim/macro`
//!
//! Plan M10 §6 zapisywał `SupplierRelation` jako typ M10 („M6 jawnie odmówił
//! własności"). Adres okazał się inny niż autorstwo — i to jest ta sama korekta,
//! którą `K-50` zrobił jądru ekonomii: **reguła jest M10, implementacja mieszka
//! tam, gdzie są dane**. Wszystkie trzy wejścia relacji są tutaj i nigdzie indziej:
//!
//! - historia terminowości to [`crate::b2b::SupplyContract::late_deliveries`]
//!   i `missed_mass`,
//! - obrót skumulowany to masa, która przeszła przez [`crate::b2b::Settlement`],
//! - jedyny czytelnik rabatu i priorytetu to rozstrzygnięcie przetargu
//!   (`rfq::score`, `B2b::resolve_due`).
//!
//! Trzymanie tabeli piętro wyżej znaczyłoby port z `Arc<dyn …>` (jak `Deposits`)
//! wyłącznie po to, żeby wrócić po liczbę, którą ten crate właśnie sam policzył.
//! Relacja jest przy tym **strukturalnie tym samym** co [`crate::b2b::Exclusives`]:
//! mapa par (kupujący, dostawca), czytana w przetargu, haszowana razem z `B2b`.
//! Dwa sąsiadujące mechanizmy o jednym kształcie i dwóch adresach rozjechałyby się
//! przy pierwszej zmianie.
//!
//! ## Co relacja zmienia, a czego nie zmienia
//!
//! `discount_bp` jest **preferencją w funkcji celu**, a nie obniżką ceny. Kupujący
//! płaci dostawcy pełną kwotę z oferty; rabat mówi tylko tyle, że oferta stałego
//! dostawcy wygrywa, mimo że jest o te procenty droższa. Kryterium WP10.13 brzmi
//! „firma płaci +3 % stałemu dostawcy zamiast szukać taniej" — czyli faktycznie
//! **przepłaca**, a nie dostaje rabat. Gdyby rabat schodził z ceny, sprzedawca
//! płaciłby za własną rzetelność.

use std::collections::BTreeMap;

use magnat_core::{FirmId, GoodId, HashState, Mass, SimMinute, StateHasher, Q};
use serde::Deserialize;

/// Kalibracja relacji dostawczych. Właścicielem pliku `data/tuning/relations.ron`
/// jest M10; ten crate dostaje gotową strukturę setterem, bo ładowarka nie może
/// mieszkać pod `sim/economy` w crate'cie, który o `data/tuning/` nic nie wie.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
pub struct RelationTuning {
    /// Zaufanie nowej relacji. Nie zero: pierwszy dostawca nie jest podejrzany,
    /// tylko nieznany, a zero znaczyłoby „zawiódł", zanim cokolwiek dostarczył.
    pub trust_start: u8,
    /// Okno wygładzania zaufania w dostawach. Im dłuższe, tym trudniej odbudować
    /// i trudniej zepsuć — to jest jedyny parametr bezwładności tej mechaniki.
    pub trust_window: u8,
    /// Ile punktów zaufania kosztuje dostawa spóźniona **ponad tolerancję**
    /// kontraktu (`Penalty::grace_minutes`). Spóźnienie w tolerancji nie kosztuje
    /// nic — inaczej `grace_minutes` byłoby polem bez znaczenia.
    pub late_penalty: u8,
    /// Poziom zaufania, od którego zaczyna się preferencja. Poniżej niego relacja
    /// istnieje, ale nie waży w przetargu.
    pub trust_neutral: u8,
    /// Największa preferencja, w punktach bazowych funkcji celu, przy zaufaniu 100.
    pub max_discount_bp: u16,
    /// Ile dób bez dostawy kasuje relację. Bez tego mapa rośnie o każdą parę,
    /// która kiedykolwiek raz coś kupiła, przez całą stuletnią sesję.
    pub forget_days: u16,
}

impl Default for RelationTuning {
    fn default() -> RelationTuning {
        RelationTuning {
            trust_start: 50,
            trust_window: 8,
            late_penalty: 25,
            trust_neutral: 50,
            max_discount_bp: 300,
            forget_days: 360,
        }
    }
}

/// Jedna relacja kupujący → dostawca dla jednego towaru.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SupplierRelation {
    pub buyer: FirmId,
    pub supplier: FirmId,
    pub good: GoodId,
    pub since: SimMinute,
    /// Masa, która faktycznie dojechała. Plan fazy pisał `Qty` (milisztuki);
    /// cały łańcuch M6 liczy w gramach i drugiej jednostki tu nie ma.
    pub volume_cum: Mass,
    pub trust: Q,
    /// Ostatnia doba z jakąkolwiek dostawą — z niej liczy się zapomnienie.
    pub last_day: u32,
    pub deliveries: u32,
    pub failures: u32,
}

impl SupplierRelation {
    /// Preferencja w funkcji celu przetargu, w punktach bazowych.
    ///
    /// Liniowo od progu obojętności do setki: zaufanie 50 daje zero, 100 daje
    /// `max_discount_bp`. Krzywej tu nie ma z rozmysłu — próg i nachylenie
    /// wystarczają, żeby bramka WP10.13 była mierzalna, a każdy dodatkowy kształt
    /// byłby parametrem bez pytania, na które odpowiada.
    #[must_use]
    pub fn discount_bp(&self, t: &RelationTuning) -> u16 {
        let ponad = i32::from(self.trust.get()) - i32::from(t.trust_neutral);
        if ponad <= 0 {
            return 0;
        }
        let zakres = i32::from(100u8.saturating_sub(t.trust_neutral)).max(1);
        u16::try_from(i32::from(t.max_discount_bp) * ponad / zakres).unwrap_or(0)
    }

    /// Priorytet w niedoborze: 0..=4. Kupujący o wyższym priorytecie jest obsłużony
    /// wcześniej, gdy w tej samej minucie dojrzewa kilka zapytań do jednej wystawki.
    #[must_use]
    pub fn priority(&self) -> u8 {
        self.trust.get() / 25
    }
}

/// Wynik jednej dostawy widziany przez relację.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DeliveryOutcome {
    pub buyer: FirmId,
    pub supplier: FirmId,
    pub good: GoodId,
    pub delivered: Mass,
    pub missed: Mass,
    /// Spóźnienie **ponad tolerancję** kontraktu. Rynek spot nie ma terminu,
    /// więc zawsze `false`.
    pub late: bool,
}

/// Rejestr relacji. `BTreeMap`, bo po tej kolejności idzie hash stanu (00 §3.2).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Relations {
    rel: BTreeMap<(u32, u32, u16), SupplierRelation>,
    tuning: RelationTuning,
}

impl Relations {
    #[must_use]
    pub fn new(tuning: RelationTuning) -> Relations {
        Relations {
            rel: BTreeMap::new(),
            tuning,
        }
    }

    pub fn set_tuning(&mut self, t: RelationTuning) {
        self.tuning = t;
    }

    #[must_use]
    pub fn tuning(&self) -> &RelationTuning {
        &self.tuning
    }

    #[must_use]
    pub fn get(&self, buyer: FirmId, supplier: FirmId, good: GoodId) -> Option<&SupplierRelation> {
        self.rel
            .get(&(buyer.entity().index(), supplier.entity().index(), good.0))
    }

    /// Preferencja tej pary w przetargu. Brak relacji znaczy zero — degradacja
    /// jest łagodna i świat bez historii dostaw liczy przetarg dokładnie tak,
    /// jak liczył go M6c.
    #[must_use]
    pub fn discount_bp(&self, buyer: FirmId, supplier: FirmId, good: GoodId) -> u16 {
        self.get(buyer, supplier, good)
            .map_or(0, |r| r.discount_bp(&self.tuning))
    }

    /// Najwyższy priorytet, jaki ten kupujący ma u **któregokolwiek** dostawcy
    /// tego towaru. To jest liczba, która ustawia kolejkę zapytań w niedoborze:
    /// pytamy „czy ten kupujący jest dla kogoś stałym klientem", a nie „czy jest
    /// stałym klientem tego, kto akurat wygra".
    #[must_use]
    pub fn buyer_priority(&self, buyer: FirmId, good: GoodId) -> u8 {
        let b = buyer.entity().index();
        self.rel
            .range((b, 0, good.0)..=(b, u32::MAX, good.0))
            .filter(|((_, _, g), _)| *g == good.0)
            .map(|(_, r)| r.priority())
            .max()
            .unwrap_or(0)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.rel.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rel.is_empty()
    }

    /// Iteracja po relacjach w kolejności klucza — dla panelu i dla testów.
    pub fn iter(&self) -> impl Iterator<Item = &SupplierRelation> {
        self.rel.values()
    }

    /// Dostawa doszła (albo nie doszła) — zaufanie się przesuwa.
    pub fn record(&mut self, o: DeliveryOutcome, now: SimMinute) {
        let t = self.tuning;
        let dzis = u32::try_from(now.0 / 1_440).unwrap_or(u32::MAX);
        let klucz = (
            o.buyer.entity().index(),
            o.supplier.entity().index(),
            o.good.0,
        );
        let r = self.rel.entry(klucz).or_insert(SupplierRelation {
            buyer: o.buyer,
            supplier: o.supplier,
            good: o.good,
            since: now,
            volume_cum: Mass::ZERO,
            trust: Q::new(t.trust_start),
            last_day: dzis,
            deliveries: 0,
            failures: 0,
        });
        r.last_day = dzis;
        r.volume_cum = Mass(r.volume_cum.0.saturating_add(o.delivered.0));
        if o.delivered.0 > 0 {
            r.deliveries = r.deliveries.saturating_add(1);
        }
        if o.missed.0 > 0 || o.late {
            r.failures = r.failures.saturating_add(1);
        }

        // Ocena tej dostawy: ile z obiecanego dojechało, minus kara za spóźnienie
        // ponad tolerancję. Arytmetyka całkowita, bo zaufanie wchodzi do hasha.
        let obiecane = o.delivered.0.saturating_add(o.missed.0);
        let udzial = if obiecane <= 0 {
            0
        } else {
            (i128::from(o.delivered.0) * 100 / i128::from(obiecane)) as i32
        };
        let kara = if o.late { i32::from(t.late_penalty) } else { 0 };
        let ocena = (udzial - kara).clamp(0, 100);

        // Wygładzanie wykładnicze w oknie `trust_window`: jedna zawalona dostawa
        // nie kasuje pięciu lat współpracy, a jedna udana nie odbudowuje zaufania
        // po serii wpadek.
        let k = i32::from(t.trust_window.max(1));
        let stare = i32::from(r.trust.get());
        let mut nowe = (stare * (k - 1) + ocena) / k;
        // Krok o co najmniej jeden punkt w stronę oceny. Bez tego dzielenie
        // całkowite zatrzymuje wygładzanie przed celem — przy oknie ośmiu dostaw
        // zaufanie stawało na 93 i **nigdy** nie dochodziło do setki, więc sufit
        // preferencji z kryterium WP10.13 byłby nieosiągalny z powodu zaokrąglenia.
        if nowe == stare && ocena != stare {
            nowe += if ocena > stare { 1 } else { -1 };
        }
        r.trust = Q::new(u8::try_from(nowe.clamp(0, 100)).unwrap_or(0));
    }

    /// Kasuje relacje bez dostawy od `forget_days`. Woła się raz na dobę, razem
    /// z wygaszaniem wyłączności — z tego samego powodu i w tym samym miejscu.
    pub fn forget(&mut self, now: SimMinute) {
        let dzis = u32::try_from(now.0 / 1_440).unwrap_or(u32::MAX);
        let prog = u32::from(self.tuning.forget_days);
        self.rel
            .retain(|_, r| dzis.saturating_sub(r.last_day) < prog);
    }
}

impl HashState for Relations {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.rel.len() as u32);
        for ((b, s, g), r) in &self.rel {
            h.write_u32(*b);
            h.write_u32(*s);
            h.write_u16(*g);
            h.write_u64(r.since.0);
            h.write_i64(r.volume_cum.0);
            h.write_u8(r.trust.get());
            h.write_u32(r.last_day);
            h.write_u32(r.deliveries);
            h.write_u32(r.failures);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::Entity;
    use std::num::NonZeroU32;

    fn f(i: u32) -> FirmId {
        FirmId(Entity::new(i, NonZeroU32::MIN))
    }

    fn dostawa(rel: &mut Relations, ile: i64, brak: i64, late: bool, doba: u64) {
        rel.record(
            DeliveryOutcome {
                buyer: f(1),
                supplier: f(2),
                good: GoodId(7),
                delivered: Mass(ile),
                missed: Mass(brak),
                late,
            },
            SimMinute(doba * 1_440),
        );
    }

    #[test]
    fn zaufanie_rosnie_z_terminowych_dostaw_i_daje_preferencje() {
        let mut rel = Relations::new(RelationTuning::default());
        assert_eq!(
            rel.discount_bp(f(1), f(2), GoodId(7)),
            0,
            "nieznany dostawca"
        );
        for d in 0..60 {
            dostawa(&mut rel, 1_000_000, 0, false, d);
        }
        let r = rel.get(f(1), f(2), GoodId(7)).expect("relacja");
        assert!(
            r.trust.get() >= 95,
            "zaufanie po 60 dostawach: {}",
            r.trust.get()
        );
        assert!(
            rel.discount_bp(f(1), f(2), GoodId(7)) >= 270,
            "preferencja bliska sufitowi 300 bp"
        );
        assert_eq!(r.priority(), 4, "priorytet z zaufania");
    }

    #[test]
    fn spoznienie_ponad_tolerancje_zbija_zaufanie_a_jedna_wpadka_go_nie_kasuje() {
        let mut rel = Relations::new(RelationTuning::default());
        for d in 0..60 {
            dostawa(&mut rel, 1_000_000, 0, false, d);
        }
        let przed = rel.get(f(1), f(2), GoodId(7)).expect("relacja").trust.get();
        dostawa(&mut rel, 1_000_000, 0, true, 60);
        let po = rel.get(f(1), f(2), GoodId(7)).expect("relacja").trust.get();
        assert!(po < przed, "spóźnienie kosztuje: {przed} → {po}");
        assert!(po > 80, "jedna wpadka nie kasuje historii: {po}");
    }

    #[test]
    fn niedostarczona_masa_liczy_sie_mocniej_niz_spoznienie() {
        let mut rel = Relations::new(RelationTuning::default());
        for d in 0..60 {
            dostawa(&mut rel, 1_000_000, 0, false, d);
        }
        let start = rel.get(f(1), f(2), GoodId(7)).expect("relacja").trust.get();
        dostawa(&mut rel, 0, 1_000_000, false, 60);
        let po = rel.get(f(1), f(2), GoodId(7)).expect("relacja").trust.get();
        assert!(
            start - po >= 10,
            "całkiem niedostarczona dostawa boli: {start} → {po}"
        );
    }

    #[test]
    fn relacja_bez_dostaw_znika() {
        let mut rel = Relations::new(RelationTuning::default());
        dostawa(&mut rel, 1_000, 0, false, 1);
        assert_eq!(rel.len(), 1);
        rel.forget(SimMinute(200 * 1_440));
        assert_eq!(rel.len(), 1, "pół roku to za mało");
        rel.forget(SimMinute(400 * 1_440));
        assert!(rel.is_empty(), "rok bez dostawy kasuje relację");
    }
}
