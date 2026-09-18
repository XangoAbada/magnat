//! Selekcja encji w widoku 3D (M3d §5.11, decyzja 9.3) i stos kart inspekcji
//! (M9c §5.7, `Z-3`).
//!
//! **Model selekcji jest po stronie UI, mechanizm wskazywania po stronie renderu.**
//! `engine/ui` nie wie, jak powstaje odpowiedź na pytanie „co jest pod kursorem" —
//! dostaje ją przez [`Picker`] i zamienia na [`Selection`]. Dzięki temu wymiana
//! bufora ID na raycast po CPU (albo odwrotnie) nie dotyka ani karty inspekcji,
//! ani sterowania czasem.
//!
//! **Zaznaczeniem jest `Option<Subject>`, a nie własny enum** — wykonanie `K-62` pkt (1).
//! Do M9b istniały dwa równoległe słowniki „co da się zaznaczyć": trzywariantowy
//! `Selection` tutaj i szesnastowariantowy `Subject` w `engine/core`. Rozjechałyby się
//! przy pierwszej nowej encji, a rozjazd wyglądałby jak brak karty, a nie jak błąd.
//!
//! Fallback z ryzyka R8 jest wbudowany, a nie dopisany: zaznaczenie da się ustawić
//! wprost z listy mieszkańców ([`ListPicker::by_index`]), więc karta inspekcji działa
//! także wtedy, gdy klikanie w świat jeszcze nie działa.

use magnat_core::{CitizenId, Entity, Subject};

/// Co jest zaznaczone. `None` = nic — i to jest normalny stan, nie błąd.
pub type Selection = Option<Subject>;

/// Zaznaczony mieszkaniec, jeśli zaznaczenie jest mieszkańcem.
#[must_use]
pub const fn selected_citizen(s: Selection) -> Option<CitizenId> {
    match s {
        Some(Subject::Citizen(c)) => Some(c),
        _ => None,
    }
}

/// Ile kart pamięta stos nawigacji w każdą stronę. Najstarsza wypada.
pub const NAV_DEPTH: usize = 32;

/// Stos „wstecz/dalej" karty inspekcji (M9c §5.7, `Z-3`).
///
/// Bez niego karta, w której **każda nazwa jest odnośnikiem**, jest pułapką: trzy
/// skoki i gracz nie ma jak wrócić do pytania, od którego zaczął. Stos jest stanem
/// widoku — nie wchodzi do zapisu świata ani do hasha (00 §3.6).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct InspectionNav {
    current: Selection,
    back: Vec<Subject>,
    forward: Vec<Subject>,
}

impl InspectionNav {
    #[must_use]
    pub fn new() -> InspectionNav {
        InspectionNav::default()
    }

    #[must_use]
    pub const fn current(&self) -> Selection {
        self.current
    }

    /// Skok do nowego podmiotu: poprzedni ląduje na stosie wstecz, a gałąź „dalej"
    /// przestaje istnieć — tak samo jak w przeglądarce, bo nawyk jest ten sam.
    pub fn go(&mut self, to: Subject) {
        if self.current == Some(to) {
            return;
        }
        if let Some(prev) = self.current {
            if self.back.len() == NAV_DEPTH {
                self.back.remove(0);
            }
            self.back.push(prev);
        }
        self.forward.clear();
        self.current = Some(to);
    }

    /// Czyści zaznaczenie i całą historię — klik w pustkę, wyjście ze świata.
    pub fn clear(&mut self) {
        self.current = None;
        self.back.clear();
        self.forward.clear();
    }

    /// Wraca do poprzedniej karty. `false` = nie było dokąd.
    pub fn back(&mut self) -> bool {
        let Some(prev) = self.back.pop() else {
            return false;
        };
        if let Some(cur) = self.current {
            self.forward.push(cur);
        }
        self.current = Some(prev);
        true
    }

    /// Idzie w przód po wcześniejszym cofnięciu. `false` = nie było dokąd.
    pub fn forward(&mut self) -> bool {
        let Some(next) = self.forward.pop() else {
            return false;
        };
        if let Some(cur) = self.current {
            self.back.push(cur);
        }
        self.current = Some(next);
        true
    }

    #[must_use]
    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    #[must_use]
    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }
}

/// Źródło odpowiedzi „co jest pod kursorem".
///
/// Implementuje je klient graficzny (bufor ID renderowany razem z geometrią,
/// decyzja 9.3). `engine/ui` widzi wyłącznie ten interfejs — i dlatego karta
/// inspekcji jest testowalna bez GPU.
pub trait Picker {
    /// Encja pod punktem ekranu albo `None`. Współrzędne w pikselach, od lewego
    /// górnego rogu.
    fn pick(&self, x: u32, y: u32) -> Option<PickResult>;
}

/// Trafienie: co i jak daleko od kamery.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct PickResult {
    pub selection: Selection,
    /// Odległość od kamery w metrach — do rozstrzygania, co jest bliżej.
    pub distance_m: f32,
}

/// Picker, który nie widzi niczego — stan przed wpięciem bufora ID i zarazem
/// atrapa w testach.
pub struct NoPicker;

impl Picker for NoPicker {
    fn pick(&self, _x: u32, _y: u32) -> Option<PickResult> {
        None
    }
}

/// Picker po liście mieszkańców — fallback z ryzyka R8 (§8 dokumentu fazy).
///
/// Wyszukiwarka zamiast klikania: karta inspekcji otwiera się po indeksie encji,
/// więc złoty test i przegląd fazy nie czekają na bufor ID w renderze.
pub struct ListPicker {
    citizens: Vec<Entity>,
}

impl ListPicker {
    #[must_use]
    pub fn new(citizens: Vec<Entity>) -> ListPicker {
        ListPicker { citizens }
    }

    /// `n`-ty mieszkaniec z listy.
    #[must_use]
    pub fn by_index(&self, n: usize) -> Selection {
        self.citizens
            .get(n)
            .map(|e| Subject::Citizen(CitizenId(*e)))
    }

    /// Mieszkaniec o danym indeksie encji.
    #[must_use]
    pub fn by_entity_index(&self, index: u32) -> Selection {
        self.citizens
            .binary_search_by_key(&index, |e| e.index())
            .ok()
            .map(|i| Subject::Citizen(CitizenId(self.citizens[i])))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.citizens.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.citizens.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::{FirmId, SiteId};
    use std::num::NonZeroU32;

    fn encja(i: u32) -> Entity {
        Entity::new(i, NonZeroU32::new(1).unwrap())
    }

    #[test]
    fn wybor_po_indeksie_daje_citizen_id() {
        let p = ListPicker::new(vec![encja(3), encja(7), encja(11)]);
        assert_eq!(selected_citizen(p.by_index(1)), Some(CitizenId(encja(7))));
        assert_eq!(
            selected_citizen(p.by_entity_index(11)),
            Some(CitizenId(encja(11)))
        );
        assert!(p.by_entity_index(4).is_none());
        assert!(p.by_index(99).is_none());
    }

    #[test]
    fn brak_pickera_nie_zaznacza_niczego() {
        assert!(NoPicker.pick(10, 10).is_none());
        assert!(Selection::default().is_none());
    }

    #[test]
    fn stos_wraca_tam_skad_gracz_przyszedl() {
        let anna = Subject::Citizen(CitizenId(encja(1)));
        let sklep = Subject::Site(SiteId(encja(2)));
        let firma = Subject::Firm(FirmId(encja(3)));

        let mut nav = InspectionNav::new();
        assert!(!nav.back(), "pusty stos nie ma dokąd wracać");
        nav.go(anna);
        nav.go(sklep);
        nav.go(firma);

        assert!(nav.back());
        assert_eq!(nav.current(), Some(sklep));
        assert!(nav.back());
        assert_eq!(nav.current(), Some(anna), "trzy skoki i powrót do pytania");
        assert!(!nav.back());

        assert!(nav.forward());
        assert_eq!(nav.current(), Some(sklep));
        // Nowy skok ucina gałąź „dalej" — tak samo jak w przeglądarce.
        nav.go(anna);
        assert!(!nav.can_go_forward());
    }

    #[test]
    fn skok_w_to_samo_miejsce_nie_rosnie_stosu() {
        let anna = Subject::Citizen(CitizenId(encja(1)));
        let mut nav = InspectionNav::new();
        nav.go(anna);
        nav.go(anna);
        assert!(
            !nav.can_go_back(),
            "ten sam podmiot dwa razy to jedna karta"
        );
    }

    #[test]
    fn stos_ma_sufit_i_gubi_najstarsze() {
        let mut nav = InspectionNav::new();
        for i in 0..(NAV_DEPTH as u32 + 10) {
            nav.go(Subject::Citizen(CitizenId(encja(i))));
        }
        let mut ile = 0;
        while nav.back() {
            ile += 1;
        }
        assert_eq!(ile, NAV_DEPTH, "stos przestał mieć sufit");
    }
}
