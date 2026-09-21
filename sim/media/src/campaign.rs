//! Kampania reklamowa: kanał, budżet, pomiar (M10b §5.2).
//!
//! **Zasięg jest planem i pomiarem, nie zbiorem odbiorców.** Kampania nie trzyma listy
//! ludzi, do których dotarła — trzyma liczbę. Lista byłaby 400-tysięczną tablicą bitów
//! na kampanię, a jedyne pytanie, które ktokolwiek jej zadaje, brzmi „ilu i z jakich
//! dzielnic" (panel marketingu M9, M10 §6 pkt 1).

use magnat_core::{
    AdChannelKind, BrandId, CampaignId, DistrictId, EventId, HashState, Money, SimMinute, SiteId,
    StateHasher, AD_CHANNEL_KIND_COUNT, Q,
};
use magnat_nav::EdgeId;

/// Kanał, którym kampania dociera do mieszkańca — **z parametrami**.
///
/// Sam słownik bez parametrów mieszka w `core` jako [`AdChannelKind`] (`K-79`),
/// bo jest ładunkiem `DecisionReason`. Tutaj zostaje to, co niesie uchwyty:
/// krawędź billboardu, zakład redakcji, promień ulotkowania. To ta sama korekta,
/// którą `K-48` zrobił przy `BankruptcyTrigger`, a `K-64` przy `RemedyKind`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AdChannel {
    /// Tablica przy krawędzi grafu drogowego. `notice_rate_bps` to szansa, że
    /// przejeżdżający w ogóle ją zauważy (M10b §5.2: domyślnie 12 %).
    Billboard { edge: EdgeId, notice_rate_bps: u16 },
    /// Ogłoszenie w tytule prasowym — zasięg z czytelnictwa dzielnic.
    Press { outlet: SiteId },
    /// Spot radiowy.
    Radio { outlet: SiteId },
    /// Spot telewizyjny.
    Tv { outlet: SiteId },
    /// Ulotki roznoszone w promieniu od zakładu.
    Leaflet { origin: SiteId, radius_m: u16 },
    /// Ekspozycja w sklepie — dociera do tych, którzy i tak tam bywają, i dlatego
    /// jest najtańsza z ośmiu.
    InStorePromo { site: SiteId },
    /// Sponsoring zdarzenia — zasięg z jego zakresu.
    Sponsorship { event: EventId },
    /// Działania PR: **nie tworzą ekspozycji**, tylko wstrzykują opinię w graf relacji
    /// (§5.2). Dlatego nie mają parametru: PR nie ma gdzie stanąć ani kogo wykupić.
    Pr,
}

impl AdChannel {
    /// Rodzaj kanału jako słownik z `core`.
    #[must_use]
    pub fn kind(&self) -> AdChannelKind {
        match self {
            AdChannel::Billboard { .. } => AdChannelKind::Billboard,
            AdChannel::Press { .. } => AdChannelKind::Press,
            AdChannel::Radio { .. } => AdChannelKind::Radio,
            AdChannel::Tv { .. } => AdChannelKind::Tv,
            AdChannel::Leaflet { .. } => AdChannelKind::Leaflet,
            AdChannel::InStorePromo { .. } => AdChannelKind::InStorePromo,
            AdChannel::Sponsorship { .. } => AdChannelKind::Sponsorship,
            AdChannel::Pr => AdChannelKind::Pr,
        }
    }

    /// Zakład, który inkasuje pieniądze za ten kanał.
    ///
    /// `None` znaczy, że odbiorcą jest reszta świata, i jest to **jawny sufit**:
    /// właściciela tablicy reklamowej, drukarni ulotek ani organizatora imprezy
    /// nie modelujemy, więc udawanie, że pieniądz trafia do konkretnej firmy,
    /// byłoby zmyśleniem drugiej strony przelewu.
    #[must_use]
    pub fn payee(&self) -> Option<SiteId> {
        match self {
            AdChannel::Press { outlet }
            | AdChannel::Radio { outlet }
            | AdChannel::Tv { outlet } => Some(*outlet),
            _ => None,
        }
    }

    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u8(self.kind().as_index() as u8);
        match self {
            AdChannel::Billboard {
                edge,
                notice_rate_bps,
            } => {
                h.write_u32(edge.0);
                h.write_u16(*notice_rate_bps);
            }
            AdChannel::Press { outlet }
            | AdChannel::Radio { outlet }
            | AdChannel::Tv { outlet } => {
                outlet.entity().hash_state(h);
            }
            AdChannel::Leaflet { origin, radius_m } => {
                origin.entity().hash_state(h);
                h.write_u16(*radius_m);
            }
            AdChannel::InStorePromo { site } => site.entity().hash_state(h),
            AdChannel::Sponsorship { event } => h.write_u32(event.0),
            AdChannel::Pr => {}
        }
    }
}

/// Ile dzielnic mieści rozbicie ekspozycji.
///
/// `ponytail:` sufit nazwany: tyle samo, ile mieści migawka renderu
/// (`sim-snapshot::MAX_DISTRICTS`), ale **stała jest tutaj**, bo `sim/media` nie
/// zależy od prezentacji i zależeć nie ma. Miasto z większą liczbą dzielnic policzy
/// ekspozycje tych pierwszych sześćdziesięciu czterech; ścieżka wyjścia to wspólna
/// stała w `engine/core`, kiedy pojawi się trzeci czytelnik.
pub const METRIC_DISTRICTS: usize = 64;

/// Pomiar kampanii — to, co widzi gracz w panelu marketingu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CampaignMetrics {
    /// Ekspozycje dostarczone w bieżącej dobie.
    pub exposures_today: u32,
    /// Ekspozycje dostarczone od początku kampanii.
    pub exposures_total: u64,
    /// Ile z nich trafiło do kogoś, kto marki **nie znał** — lejek „znajomość" z §6.
    pub first_contacts: u64,
    /// Ekspozycje od początku kampanii w rozbiciu na dzielnicę **zamieszkania**
    /// odbiorcy (`GF-1` nie rusza tej liczby; wymaga jej kryterium WP10.19).
    ///
    /// Dzielnica zamieszkania, a nie miejsca kontaktu, i to jest rozstrzygnięcie:
    /// billboard przy trasie dojazdowej dociera do ludzi, **którzy mieszkają gdzie
    /// indziej**, a to jest właśnie ta informacja, po którą gracz otwiera panel.
    /// Suma tej tablicy bywa mniejsza niż `exposures_total`, bo odbiorca bez
    /// przypisanego miejsca zamieszkania nie ma dzielnicy — i tak jest uczciwiej
    /// niż wrzucać go do dzielnicy zerowej.
    pub by_district: [u32; METRIC_DISTRICTS],
}

impl Default for CampaignMetrics {
    fn default() -> CampaignMetrics {
        CampaignMetrics {
            exposures_today: 0,
            exposures_total: 0,
            first_contacts: 0,
            by_district: [0; METRIC_DISTRICTS],
        }
    }
}

impl CampaignMetrics {
    /// Dzielnice, które kampania dosięgła, od największej liczby ekspozycji.
    ///
    /// Do panelu, nie do symulacji: kolejność jest malejąca po liczbie, a remis
    /// rozstrzyga niższy numer dzielnicy, więc wynik jest deterministyczny.
    #[must_use]
    pub fn top_districts(&self) -> Vec<(DistrictId, u32)> {
        let mut out: Vec<(DistrictId, u32)> = self
            .by_district
            .iter()
            .enumerate()
            .filter(|(_, n)| **n > 0)
            .map(|(i, n)| (DistrictId(i as u16), *n))
            .collect();
        out.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0 .0.cmp(&b.0 .0)));
        out
    }
}

impl HashState for CampaignMetrics {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.exposures_today);
        h.write_u64(self.exposures_total);
        h.write_u64(self.first_contacts);
        for n in &self.by_district {
            h.write_u32(*n);
        }
    }
}

/// Jedna kampania reklamowa.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AdCampaign {
    pub id: CampaignId,
    /// Zakład, który ją kupił — **on płaci i on ma księgę**. Firma wyprowadza się
    /// z zakładu, a nie odwrotnie: ogłoszenie kupuje sklep, nie centrala.
    pub site: SiteId,
    /// Marka, którą kampania promuje (`magnat_supply::brand_of` reklamodawcy).
    pub brand: BrandId,
    pub channel: AdChannel,
    /// Deklarowana jakość — liczba, którą kampania wpisuje odbiorcom w oczekiwania.
    /// Obietnica ponad stan jest samokarząca (§5.1).
    pub claim: Q,
    pub budget: Money,
    pub spent: Money,
    /// Okno emisji: od kiedy do kiedy. Poza nim kampania nie dostarcza ekspozycji.
    pub window: (SimMinute, SimMinute),
    pub metrics: CampaignMetrics,
}

impl AdCampaign {
    /// Czy kampania emituje w tej minucie.
    #[must_use]
    pub fn is_live(&self, now: SimMinute) -> bool {
        now.get() >= self.window.0.get()
            && now.get() < self.window.1.get()
            && self.spent.get() < self.budget.get()
    }

    /// Ile jeszcze wolno wydać.
    #[must_use]
    pub fn remaining(&self) -> Money {
        Money((self.budget.get() - self.spent.get()).max(0))
    }
}

impl HashState for AdCampaign {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.id.0);
        self.site.entity().hash_state(h);
        h.write_u16(self.brand.0);
        self.channel.hash_state(h);
        h.write_u8(self.claim.get());
        h.write_u64(self.budget.get() as u64);
        h.write_u64(self.spent.get() as u64);
        h.write_u64(self.window.0.get());
        h.write_u64(self.window.1.get());
        self.metrics.hash_state(h);
    }
}

/// Rejestr kampanii — zasób świata, wchodzi do hasha stanu.
///
/// `BTreeMap`, bo kolejność iteracji jest kolejnością dostarczania ekspozycji,
/// a ta wchodzi do stanu świata (00 §3.2).
#[derive(Clone, Debug, Default)]
pub struct Campaigns {
    map: std::collections::BTreeMap<CampaignId, AdCampaign>,
    next: u32,
    /// Ekspozycje dostarczone **od początku gry**, w rozbiciu na kanał.
    ///
    /// Licznik dożywotni, bo rejestr kampanii nim nie jest: kampania żyje
    /// trzydzieści dób, a `close_finished` usuwa ją razem z jej pomiarem. Histogram
    /// liczony z żywych kampanii pokazuje więc **zero na każdej granicy miesiąca** —
    /// poprzednie właśnie wygasły, a nowe otwierają się już po dobowym rozdaniu
    /// ekspozycji (`ai::monthly` stoi w `doba()` **za** pętlą kanałów). Tak powstał
    /// fałszywy wniosek `GF-2` („kanał ulotkowy nie dociera do nikogo") z przebiegu
    /// `--days 300`: 300 jest wielokrotnością trzydziestu.
    ///
    /// Wchodzi do hasha stanu jak reszta rejestru — jest sumą tego, co świat zrobił.
    lifetime_by_channel: [u64; AD_CHANNEL_KIND_COUNT],
}

impl Campaigns {
    /// Ekspozycje dostarczone od początku gry tym kanałem.
    #[must_use]
    pub fn lifetime(&self, k: AdChannelKind) -> u64 {
        self.lifetime_by_channel[k.as_index()]
    }

    /// Cały histogram dożywotni — wejście raportu scenariusza.
    #[must_use]
    pub fn lifetime_by_channel(&self) -> &[u64; AD_CHANNEL_KIND_COUNT] {
        &self.lifetime_by_channel
    }

    /// Dolicza ekspozycję do licznika dożywotniego kanału.
    pub fn note_exposure(&mut self, k: AdChannelKind) {
        let slot = &mut self.lifetime_by_channel[k.as_index()];
        *slot = slot.saturating_add(1);
    }

    #[must_use]
    pub fn new() -> Campaigns {
        Campaigns::default()
    }

    /// Zakłada kampanię i zwraca jej numer. Numer jest monotoniczny i nigdy nie wraca.
    pub fn open(&mut self, build: impl FnOnce(CampaignId) -> AdCampaign) -> CampaignId {
        let id = CampaignId(self.next);
        self.next = self.next.wrapping_add(1);
        self.map.insert(id, build(id));
        id
    }

    #[must_use]
    pub fn get(&self, id: CampaignId) -> Option<&AdCampaign> {
        self.map.get(&id)
    }

    pub fn get_mut(&mut self, id: CampaignId) -> Option<&mut AdCampaign> {
        self.map.get_mut(&id)
    }

    /// Wszystkie kampanie w kolejności numerów.
    pub fn iter(&self) -> impl Iterator<Item = (&CampaignId, &AdCampaign)> {
        self.map.iter()
    }

    /// Kampanie emitujące w tej minucie, w kolejności numerów.
    pub fn live(&self, now: SimMinute) -> impl Iterator<Item = &AdCampaign> {
        self.map.values().filter(move |c| c.is_live(now))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Zdejmuje kampanie, które skończyły okno albo wydały budżet.
    ///
    /// Zamknięta kampania **znika**, a jej numer nie wraca: metryki, które ktoś chciałby
    /// oglądać później, należą do kroniki, a nie do rejestru żywych kampanii.
    pub fn close_finished(&mut self, now: SimMinute) -> usize {
        let przed = self.map.len();
        self.map
            .retain(|_, c| now.get() < c.window.1.get() && c.spent.get() < c.budget.get());
        przed - self.map.len()
    }

    /// Krawędzie z aktywnym billboardem — wejście podsłuchu ruchu (`EdgeWatch`).
    #[must_use]
    pub fn billboard_edges(&self, now: SimMinute) -> Vec<EdgeId> {
        let mut out: Vec<EdgeId> = self
            .live(now)
            .filter_map(|c| match c.channel {
                AdChannel::Billboard { edge, .. } => Some(edge),
                _ => None,
            })
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }
}

impl HashState for Campaigns {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_u32(self.next);
        h.write_u64(self.map.len() as u64);
        for (id, c) in &self.map {
            h.write_u32(id.0);
            c.hash_state(h);
        }
        for n in &self.lifetime_by_channel {
            h.write_u64(*n);
        }
    }
}
