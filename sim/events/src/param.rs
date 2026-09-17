//! Parametry symulacji i nakładka, która je składa (M8c §5.5, PRD §11.1).
//!
//! **Zasada nienegocjowalna: zdarzenie zmienia parametry, nie ceny.** Zdarzenie nie
//! zna słowa „cena" w mieście — zna plon, dostępność mocy, tempo spadku potrzeby
//! i cenę **importu**, bo ta ostatnia powstaje poza miastem i nie jest decyzją
//! żadnej tutejszej firmy. Zakaz jest wpisany w typ [`Effect`]: nie ma wariantu
//! ustawiającego cenę oferty, marżę, wolumen sprzedaży ani popyt w sztukach,
//! i test T7 sprawdza to wyliczeniem wariantów, a nie czytaniem komentarza.
//!
//! **Jak patch dociera do celu.** Nie przez odczyt: gdyby konsumenci mieli czytać
//! nakładkę, każdy z nich musiałby zależeć od tego crate'u, a `sim/economy` zależy
//! od `sim/supply`, więc `economy → events → economy` nie zbudowałoby się. Dlatego
//! nakładka **zapisuje** wartość do pola właściciela i pamięta, co tam zastała.
//! Zdarzenie gaśnie → wartość wraca. To jest cała treść [`ParamOverlay`].

use crate::catalog::EventScope;
use crate::probe::SiteFilter;
use magnat_core::{GoodId, NeedKind, SiteId, TariffClassId, UtilityService};
use serde::Deserialize;
use std::collections::BTreeMap;

/// Neutralna wartość mnożnika: 10 000 punktów bazowych.
pub const NEUTRAL: i64 = 10_000;

/// Co zdarzenie **deklaruje** w danych. Skala jest podana dla siły 10 000 bps;
/// przy sile mniejszej efekt interpoluje się liniowo od wartości neutralnej.
///
/// Wariantów jest sześć i **każdy ma dziś czytelnika**, którego widać w kodzie
/// (`crate::apply`). Wariant bez czytelnika przeszedłby każdy test i wyglądałby
/// w katalogu tak samo jak działający — ta sama reguła, która w M8b wyrzuciła
/// `EdgeState::UnderMaintenance` (`CC-4`).
#[derive(Clone, PartialEq, Eq, Debug, Deserialize)]
pub enum Effect {
    /// Zdolność produkcyjna zakładu. Cel wynika z zakresu: `Site` → ten zakład,
    /// `Firm` → zakłady tej firmy, `District` → zakłady dzielnicy przechodzące
    /// filtr, `World` → wszystkie przechodzące filtr.
    ///
    /// To jest **jeden** parametr na trzy rzeczy z §5.5 — plon suszy, przestój
    /// maszyny i brak ludzi przy strajku. Nie dlatego, że są tym samym, tylko
    /// dlatego, że jądro przepustowości M6 ma na nie jedną liczbę: szarża zależy
    /// od wsadu i od obsady, a każda z tych trzech rzeczy zabiera zakładowi
    /// zdolność wyprodukowania partii. Trzy pola o jednym miejscu zastosowania
    /// rozjechałyby się przy pierwszej zmianie (`DRY` dotyczy wiedzy).
    /// Czym się różnią, mówi powód zdarzenia — a ten gracz widzi w karcie.
    SiteOutput { mul_bps: u32, filter: SiteFilter },
    /// Źródło sieci gaśnie. Wyłącznie zakres `Network`.
    SourceOnline,
    /// Źródło sieci traci część mocy — awaria **jednego bloku**, nie zakładu.
    /// Wyłącznie zakres `Network`.
    SourceCapacity { mul_bps: u32 },
    /// Cena towaru **u dostawcy zewnętrznego**. Wyłącznie zakres `World`.
    ///
    /// Tu wolno dotknąć ceny i tylko tu: świat zewnętrzny nie jest miastem,
    /// a embargo albo wojna „w świecie" to jedyny sposób, w jaki może się odezwać.
    /// Cena w mieście zmienia się potem sama, w decyzji cenowej firmy (M7).
    ExternalPrice { good: String, mul_bps: u32 },
    /// Stawka celna klasy taryfowej, w punktach bazowych. Wyłącznie zakres `World`.
    /// Wartość jest **bezwzględna**, nie mnożnikiem: cło ustala się na poziomie,
    /// a nie „o tyle procent więcej niż poprzednio".
    Duty { class: String, bp: i64 },
    /// Tempo spadku potrzeby mieszkańców. Wyłącznie zakres `World`.
    NeedDecay { need: NeedKind, mul_bps: u32 },
}

impl Effect {
    /// Czy efekt pasuje do zakresu definicji. Zwraca powód niezgodności albo `None`.
    #[must_use]
    pub fn scope_mismatch(&self, scope: EventScope) -> Option<&'static str> {
        match self {
            Effect::SiteOutput { .. } => None,
            Effect::SourceOnline | Effect::SourceCapacity { .. } => (scope != EventScope::Network)
                .then_some("efekt sieciowy w zakresie innym niż Network"),
            Effect::ExternalPrice { .. } | Effect::Duty { .. } | Effect::NeedDecay { .. } => {
                (scope != EventScope::World).then_some("efekt globalny w zakresie innym niż World")
            }
        }
    }

    /// Wartość efektu przy zadanej sile, interpolowana od wartości neutralnej.
    ///
    /// Susza o sile 2000 bps zabiera piątą część tego, co susza o sile 10 000 —
    /// bo „zdarzenie bywa dokuczliwe i bywa katastrofą" jest treścią widełek siły,
    /// a nie ozdobą w karcie.
    #[must_use]
    pub fn lerp(target_bps: i64, severity_bps: u32) -> i64 {
        NEUTRAL + (target_bps - NEUTRAL) * i64::from(severity_bps) / NEUTRAL
    }
}

/// Klucz parametru: **co** zmieniamy i **czemu**. Porządek jest liniowy i całkowity,
/// bo po nim idzie składanie nakładki (00 §3.2 — nigdy po `HashMap`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum SimParam {
    /// Mnożnik zdolności produkcyjnej zakładu, klucz: indeks encji zakładu.
    SiteOutputMulBps(u32),
    /// Czy źródło sieci jest czynne. Klucz: `(medium, numer węzła)`.
    SourceOnline(u8, u32),
    /// Mnożnik mocy źródła. Klucz jak wyżej.
    SourceCapacityMulBps(u8, u32),
    /// Mnożnik ceny zewnętrznej towaru.
    ExternalPriceMulBps(u16),
    /// Stawka celna klasy taryfowej, w punktach bazowych (wartość bezwzględna).
    DutyBp(u16),
    /// Mnożnik tempa spadku potrzeby. Klucz: indeks `NeedKind`.
    NeedDecayMulBps(u8),
}

impl SimParam {
    /// Czy parametr składa się mnożeniem (`true`) czy ostatnim przypisaniem.
    #[must_use]
    pub const fn is_multiplicative(self) -> bool {
        !matches!(self, SimParam::SourceOnline(..) | SimParam::DutyBp(_))
    }
}

/// Jeden patch: parametr, wartość, i zdarzenie, które go wystawiło.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ParamPatch {
    pub param: SimParam,
    pub value: i64,
}

/// Nakładka: wartość efektywna każdego parametru dotkniętego przez aktywne zdarzenia.
///
/// Przeliczana **przy zmianie zbioru aktywnych zdarzeń**, nie co tick. Składanie idzie
/// po `(SimParam, EventId)` — czyli po posortowanym kluczu, nigdy po kolejności
/// wstawiania. Dwa zdarzenia na jeden parametr mnożą się (susza i szkodniki zabierają
/// plon **razem**), poza parametrami przypisywanymi, gdzie wygrywa zdarzenie
/// o wyższym numerze, czyli późniejsze.
#[derive(Clone, Debug, Default)]
pub struct ParamOverlay {
    /// Wartość wynikowa parametru — to, co ma stać w polu właściciela.
    efektywne: BTreeMap<SimParam, i64>,
    /// Wartość sprzed pierwszego zdarzenia. Nakładka **musi** ją pamiętać, bo
    /// właściciel pola nie wie, że ktoś mu je podmienił, i nie ma do czego wrócić.
    bazowe: BTreeMap<SimParam, i64>,
}

impl ParamOverlay {
    /// Składa patche w wartości efektywne. Wejście musi być posortowane po
    /// `(SimParam, EventId)` — woła to [`crate::registry::Events::rebuild_overlay`].
    #[must_use]
    pub fn compose(patches: &[ParamPatch]) -> BTreeMap<SimParam, i64> {
        let mut out: BTreeMap<SimParam, i64> = BTreeMap::new();
        for p in patches {
            let e = out
                .entry(p.param)
                .or_insert(if p.param.is_multiplicative() {
                    NEUTRAL
                } else {
                    p.value
                });
            if p.param.is_multiplicative() {
                *e = *e * p.value / NEUTRAL;
            } else {
                *e = p.value;
            }
        }
        out
    }

    /// Zamienia stary zestaw na nowy i mówi, co trzeba zapisać i co przywrócić.
    ///
    /// Zwraca `(do_zapisu, do_przywrocenia)`. Pierwsza lista to parametry, których
    /// wartość efektywna się zmieniła; druga — te, które przestały być dotknięte
    /// i wracają do wartości zastanej.
    pub fn diff(&mut self, nowe: BTreeMap<SimParam, i64>) -> (Vec<(SimParam, i64)>, Vec<SimParam>) {
        let mut zapis = Vec::new();
        for (k, v) in &nowe {
            if self.efektywne.get(k) != Some(v) {
                zapis.push((*k, *v));
            }
        }
        let mut powrot: Vec<SimParam> = self
            .efektywne
            .keys()
            .filter(|k| !nowe.contains_key(k))
            .copied()
            .collect();
        powrot.sort_unstable();
        self.efektywne = nowe;
        (zapis, powrot)
    }

    /// Zapamiętuje wartość zastaną, jeśli jeszcze jej nie ma.
    pub fn remember_base(&mut self, p: SimParam, base: i64) {
        self.bazowe.entry(p).or_insert(base);
    }

    /// Wartość zastana i zdjęcie jej z rejestru — wołane przy przywracaniu.
    pub fn take_base(&mut self, p: SimParam) -> Option<i64> {
        self.bazowe.remove(&p)
    }

    #[must_use]
    pub fn base(&self, p: SimParam) -> Option<i64> {
        self.bazowe.get(&p).copied()
    }

    #[must_use]
    pub fn active(&self) -> &BTreeMap<SimParam, i64> {
        &self.efektywne
    }

    /// Ile parametrów jest dziś dotkniętych — do inspektora i do raportu.
    #[must_use]
    pub fn len(&self) -> usize {
        self.efektywne.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.efektywne.is_empty()
    }
}

/// Cel efektu rozwinięty na konkretne klucze parametrów.
///
/// Rozwinięcie dzieje się **raz, przy powstaniu zdarzenia**, a nie przy każdym
/// przeliczeniu nakładki: lista zakładów dzielnicy nie zmienia się w trakcie suszy,
/// a gdyby się zmieniła, zakład postawiony w środku suszy nie powinien dostać jej
/// skutku wstecz.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ResolvedTarget {
    pub param: SimParam,
    pub value: i64,
}

/// Pomocnicze klucze — jedno miejsce, w którym typowany identyfikator zamienia się
/// w liczbę klucza nakładki.
#[must_use]
pub fn site_key(s: SiteId) -> u32 {
    s.0.index()
}

#[must_use]
pub fn service_key(s: UtilityService) -> u8 {
    s.as_index() as u8
}

#[must_use]
pub fn good_key(g: GoodId) -> u16 {
    g.0
}

#[must_use]
pub fn tariff_key(c: TariffClassId) -> u16 {
    c.0
}
