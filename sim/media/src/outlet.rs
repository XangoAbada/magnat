//! Media jako firmy (M10b §5.3).
//!
//! Tytuł jest **zwykłym zakładem z rejestru M7** plus jedną strukturą: czytelnictwem,
//! wiarygodnością i linią redakcyjną. Sprzedaż powierzchni reklamowej nie ma własnego
//! rynku — idzie kanałem [`crate::AdChannel::Press`] i płaci na konto zakładu, tak samo
//! jak każdy inny zakup usługi.
//!
//! **Wiarygodność jest per para, nie per tytuł**, i mieszka w tym samym slocie co marka:
//! tytuł ma `BrandId` jak każda firma, więc „ile wierzę tej gazecie" to dokładnie
//! `BrandAffinity.affinity` w pamięci czytelnika. Drugi mechanizm o tym samym znaczeniu
//! rozjechałby się z pierwszym.

use magnat_core::{
    DecisionReason, DistrictId, EditorialBias, EventCategory, EventId, HashState, MediaKind, Money,
    SiteId, StateHasher, Tick, Q,
};

/// Tytuł medialny — nakładka na zakład.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaOutlet {
    pub site: SiteId,
    pub kind: MediaKind,
    /// Udział czytelnictwa per dzielnica, w promilach. Posortowane po dzielnicy —
    /// kolejność wchodzi do hasha (00 §3.2).
    pub readership: Vec<(DistrictId, u16)>,
    /// Wiarygodność **startowa** tytułu; różnicę per czytelnik niesie jego slot marki.
    pub credibility: Q,
    pub bias: EditorialBias,
    /// Ile slotów reklamowych sprzedaje na dobę.
    pub inventory_daily: u16,
    /// Ile z nich zostało na dziś.
    pub inventory_left: u16,
    pub slot_price: Money,
}

impl MediaOutlet {
    /// Czytelnictwo w dzielnicy, w promilach. Zero, gdy tytuł tam nie dociera.
    #[must_use]
    pub fn readership_permille(&self, d: DistrictId) -> u16 {
        self.readership
            .binary_search_by_key(&d, |(k, _)| *k)
            .map(|i| self.readership[i].1)
            .unwrap_or(0)
    }
}

impl HashState for MediaOutlet {
    fn hash_state(&self, h: &mut StateHasher) {
        self.site.entity().hash_state(h);
        h.write_u8(self.kind.as_index() as u8);
        h.write_u64(self.readership.len() as u64);
        for (d, r) in &self.readership {
            h.write_u16(d.0);
            h.write_u16(*r);
        }
        h.write_u8(self.credibility.get());
        h.write_u8(self.bias.as_index() as u8);
        h.write_u16(self.inventory_daily);
        h.write_u16(self.inventory_left);
        h.write_u64(self.slot_price.get() as u64);
    }
}

/// Opublikowany tekst.
///
/// **Zasięg jest licznikiem per dzielnica, a nie zbiorem czytelników**, i to jest
/// decyzja, nie skrót. Zbiór czytelników to 400-tysięczna tablica bitów na tekst,
/// czyli 1,2 MB na trzy doby publikacji — za odpowiedź na jedyne zadawane pytanie
/// („jaka część dzielnicy o tym wie"). Wiedza o **zdarzeniu** nie jest przy tym wiedzą
/// o miejscu: magazyn `Knowledge` z M3 trzyma miejsca i ma twardy limit 32 wpisów,
/// więc wpuszczenie tam zdarzeń wypchnęłoby mieszkańcowi sklepy, do których chodzi.
///
/// `ponytail:` sufit nazwany — dopóki nikt nie pyta „czy **ten** mieszkaniec wie
/// o strajku", licznik wystarcza. Droga wyjścia: osobny, krótki magazyn zdarzeń
/// obok magazynu miejsc, kiedy M10f zbuduje kronikę per mieszkaniec.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Story {
    pub outlet: SiteId,
    pub event: EventId,
    pub category: EventCategory,
    pub bias: EditorialBias,
    pub published_day: u32,
    /// Ilu mieszkańców dzielnicy wie o zdarzeniu. Posortowane po dzielnicy.
    pub known: Vec<(DistrictId, u32)>,
}

impl Story {
    /// Ilu wie w tej dzielnicy.
    #[must_use]
    pub fn known_in(&self, d: DistrictId) -> u32 {
        self.known
            .binary_search_by_key(&d, |(k, _)| *k)
            .map(|i| self.known[i].1)
            .unwrap_or(0)
    }

    pub fn add_known(&mut self, d: DistrictId, n: u32) {
        match self.known.binary_search_by_key(&d, |(k, _)| *k) {
            Ok(i) => self.known[i].1 = self.known[i].1.saturating_add(n),
            Err(i) => self.known.insert(i, (d, n)),
        }
    }

    /// Łączny zasięg w promilach populacji miasta.
    #[must_use]
    pub fn reach_permille(&self, population: u32) -> u16 {
        if population == 0 {
            return 0;
        }
        let suma: u64 = self.known.iter().map(|(_, n)| u64::from(*n)).sum();
        (suma * 1000 / u64::from(population)).min(1000) as u16
    }
}

impl HashState for Story {
    fn hash_state(&self, h: &mut StateHasher) {
        self.outlet.entity().hash_state(h);
        h.write_u32(self.event.0);
        h.write_u8(self.category.as_index() as u8);
        h.write_u8(self.bias.as_index() as u8);
        h.write_u32(self.published_day);
        h.write_u64(self.known.len() as u64);
        for (d, n) in &self.known {
            h.write_u16(d.0);
            h.write_u32(*n);
        }
    }
}

/// Ile dób tekst jeszcze rozchodzi się plotką po publikacji (§5.3).
///
/// Trzy, bo tyle mierzy kryterium WP10.7 — i tyle wystarcza: po trzech dobach przyrost
/// z plotki jest mniejszy od szumu, a tekst przestaje być nowiną.
pub const STORY_SPREAD_DAYS: u32 = 3;

/// Ile ostatnich publikacji pamięta dziennik redakcji.
///
/// Pierścień, a nie pełna historia: kronika gracza zbiera dzienniki `sim/*` raz na
/// dobę (`DK-1`), więc wystarczy, żeby wpis przeżył dobę. Pełną historię prowadzi
/// `game::chronicle`, bo to ona ma filtry, ważność i wyszukiwarkę.
pub const LOG_RING: usize = 128;

/// Rejestr tytułów i żywych tekstów — zasób świata, wchodzi do hasha.
#[derive(Clone, Debug, Default)]
pub struct Outlets {
    map: std::collections::BTreeMap<SiteId, MediaOutlet>,
    stories: Vec<Story>,
    log: Vec<(Tick, DecisionReason)>,
    /// Ile tekstów uderzyło w czyjąś markę, od początku gry.
    ///
    /// Licznik, a nie historia: pytanie brzmi „ile marek rocznie obrywa od prasy"
    /// (`FF-25`) i odpowiada na nie jedna liczba podzielona przez lata przebiegu.
    /// Liczony **per tekst**, nie per czytelnik — inaczej mierzyłby zasięg tytułu,
    /// a nie liczbę skandali. Wchodzi do hasha, bo jest faktem o świecie, który
    /// powstał deterministycznie: dwa przebiegi tego samego ziarna mają mieć tyle
    /// samo skandali, a rozjazd w tej liczbie jest rozjazdem w redakcji.
    scandal_stories: u32,
}

impl Outlets {
    #[must_use]
    pub fn new() -> Outlets {
        Outlets::default()
    }

    pub fn insert(&mut self, o: MediaOutlet) {
        self.map.insert(o.site, o);
    }

    /// Ile tekstów uderzyło w czyjąś markę od początku gry (`FF-25`).
    #[must_use]
    pub fn scandal_stories(&self) -> u32 {
        self.scandal_stories
    }

    /// Notuje, że publikowany właśnie tekst zabrał komuś sympatię do marki.
    pub fn note_scandal(&mut self) {
        self.scandal_stories = self.scandal_stories.saturating_add(1);
    }

    #[must_use]
    pub fn get(&self, site: SiteId) -> Option<&MediaOutlet> {
        self.map.get(&site)
    }

    pub fn get_mut(&mut self, site: SiteId) -> Option<&mut MediaOutlet> {
        self.map.get_mut(&site)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&SiteId, &MediaOutlet)> {
        self.map.iter()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Publikuje tekst i zapisuje powód do dziennika redakcji.
    ///
    /// Powód zapisuje się **u siebie**, a nie w kronice gracza: `game/` stoi nad
    /// wszystkimi `sim/*`, więc zgłoszenie w drugą stronę zamknęłoby cykl (`DK-1`).
    /// `Chronicle::harvest` czyta ten dziennik tak samo, jak czyta dziennik zdarzeń.
    pub fn publish(&mut self, s: Story, at: Tick, reason: DecisionReason) {
        self.stories.push(s);
        self.log.push((at, reason));
        if self.log.len() > LOG_RING {
            let nadmiar = self.log.len() - LOG_RING;
            self.log.drain(..nadmiar);
        }
    }

    /// Powody ostatnich publikacji — wejście kroniki gracza i karty tytułu.
    #[must_use]
    pub fn reasons(&self) -> &[(Tick, DecisionReason)] {
        &self.log
    }

    #[must_use]
    pub fn stories(&self) -> &[Story] {
        &self.stories
    }

    pub fn stories_mut(&mut self) -> &mut Vec<Story> {
        &mut self.stories
    }

    /// Zdejmuje teksty, które przestały być nowiną.
    pub fn retire(&mut self, today: u32) {
        self.stories
            .retain(|s| today.saturating_sub(s.published_day) <= STORY_SPREAD_DAYS);
    }
}

impl HashState for Outlets {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u64(self.map.len() as u64);
        for (site, o) in &self.map {
            site.entity().hash_state(h);
            o.hash_state(h);
        }
        h.write_u64(self.stories.len() as u64);
        for s in &self.stories {
            s.hash_state(h);
        }
        h.write_u64(self.log.len() as u64);
        for (t, r) in &self.log {
            h.write_u64(t.0);
            h.write_u16(r.discriminant());
        }
        h.write_u32(self.scandal_stories);
    }
}
