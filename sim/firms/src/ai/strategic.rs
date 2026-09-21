//! Tier strategiczny — kwartał firmy (M7f WP13, M7 §5.9, PRD §12.3).
//!
//! # Firma nie widzi prognozy, widzi ranking
//!
//! To jest cała treść tego modułu i jego jedyna trudna decyzja. Model makro
//! (`sim/macro`) deklaruje własny błąd 3–12 % i ten błąd jest nieusuwalny.
//! Gdyby tier strategiczny dostał wynik jako liczbę, prędzej czy później ktoś
//! porównałby ją z progiem — a reguła progowa na wielkości obarczonej 12 % błędu
//! daje decyzje, które gracz odczyta jako „AI zachowuje się losowo" (`R14`).
//!
//! Dlatego do [`crate::view::FirmView`] wchodzi [`Outlook`]: lista wariantów,
//! **indeks** zwycięzcy, kierunek i szerokość marginesu. Żadnej kwoty. I nie jest
//! to samodyscyplina, tylko graf crate'ów: `sim/firms` nie widzi `sim/macro`,
//! więc `MacroOutcome` jest tu **niewyrażalny** — tak samo jak cudzy koszt (§5.8).
//!
//! # Co tier strategiczny umie zrobić
//!
//! Cztery akcje, każda z wykonawcą. Planu fazy jest osiem, ale `Restructure`
//! i `SwitchStrategy` mają już wykonawcę w tierze taktycznym (`TacAction`), a enum
//! z wariantami, których nikt nie wykonuje, byłby abstrakcją bez drugiego
//! konsumenta — to samo rozstrzygnięcie, które M7e zapisał jako `BC-6`.

use magnat_core::{DecisionReason, DistrictId, FirmReason, Money, SiteId, Tick, Trend};
use magnat_policy::Decided;
use smallvec::SmallVec;

use crate::view::FirmView;

/// Ile wariantów firma porównuje naraz. Pięć, bo tyle mieści `SmallVec` po stronie
/// modelu i tyle wystarcza: `KeepCourse` plus cztery ruchy.
pub const MAX_VARIANTS: usize = 5;

/// Co tier strategiczny może zrobić w ciągu kwartału.
///
/// Wariant **jest** akcją: firma pyta model o to, co zamierza zrobić, a nie
/// o abstrakcyjny „scenariusz", który potem trzeba by na akcję tłumaczyć. Jedno
/// tłumaczenie mniej to jedno miejsce mniej, w którym ranking mógłby wskazać co
/// innego, niż firma wykona.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StrAction {
    /// Nic nie rób. **Zawsze w zestawie** i wygrywa, gdy model nie rozstrzyga:
    /// niepewność modelu ma się przekładać na bezwładność firmy (§5.10).
    KeepCourse,
    /// Otwórz zakład w dzielnicy.
    OpenSite {
        district: DistrictId,
        slots: u32,
        capex: Money,
    },
    /// Zamknij zakład — ten sam skutek co w tierze taktycznym, inna przyczyna:
    /// tam ciąg strat, tu wariant „bez niego" wypadający lepiej niż „z nim".
    CloseSite { site: SiteId, slots: u32 },
    /// Zwiń firmę dobrowolnie, zanim zrobi to syndyk (PRD §12.4).
    RequestVoluntaryClosure,
}

/// Wynik porównania wariantów, przepisany do postaci, którą firma wolno zobaczyć.
///
/// Powstaje w `sim/macro` i **nie da się go zbudować z liczby pieniężnej**, bo
/// żadnej nie niesie. To jest ta sama gwarancja, którą `FirmView` daje wobec
/// kosztów rywala, wystawiona przez ten sam mechanizm — kształt typu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outlook {
    /// Warianty w kolejności, w jakiej firma je zgłosiła. `variants[0]` to zawsze
    /// [`StrAction::KeepCourse`].
    pub variants: SmallVec<[StrAction; MAX_VARIANTS]>,
    /// Indeks zwycięzcy albo `None`, gdy przewaga nie przekroczyła marginesu.
    pub winner: Option<u8>,
    /// Kierunek wyniku zwycięzcy względem `KeepCourse` — bez wielkości.
    pub trend: Trend,
    /// Margines błędu porównania w punktach bazowych, **deklarowany przez model**.
    /// Firma go nie zgaduje i nie nadpisuje (M10 §6).
    pub error_margin_bp: u32,
    /// Kiedy powstał. Starszy niż kwartał znaczy „nie wiem", a nie „bez zmian".
    pub at: Tick,
}

impl Outlook {
    /// Zwycięski wariant, jeśli model rozstrzygnął.
    #[must_use]
    pub fn winning(&self) -> Option<StrAction> {
        self.variants.get(usize::from(self.winner?)).copied()
    }
}

/// Kwartał firmy: wykonaj wariant, który wygrał ponad margines.
///
/// `None` znaczy `KeepCourse` i **to jest najczęstsza odpowiedź** — tak ma być.
/// Firma rusza się wtedy, gdy różnica między wariantami jest realna, a nie wtedy,
/// gdy model akurat wskazał inny.
#[must_use]
pub fn decide_strategic(v: &FirmView) -> Option<Decided<StrAction>> {
    let outlook = v.outlook.as_ref()?;
    let akcja = outlook.winning()?;
    match akcja {
        StrAction::KeepCourse => None,
        StrAction::OpenSite {
            district,
            slots,
            capex,
        } => Some(Decided::new(
            StrAction::OpenSite {
                district,
                slots,
                capex,
            },
            DecisionReason::Firm(FirmReason::SiteOpened {
                district,
                variants: u8::try_from(outlook.variants.len()).unwrap_or(u8::MAX),
                margin_bp: u16::try_from(outlook.error_margin_bp).unwrap_or(u16::MAX),
                trend: outlook.trend,
            }),
        )),
        StrAction::CloseSite { site, slots } => Some(Decided::new(
            StrAction::CloseSite { site, slots },
            DecisionReason::Firm(FirmReason::SiteClosed {
                // Tier strategiczny zamyka zakład **nie** z powodu ciągu strat,
                // więc liczba miesięcy jest zerem i to jest prawda, a nie brak
                // danych: powodem jest porównanie wariantów, nie historia.
                months: 0,
                margin_bp: v.site(site).and_then(|s| s.last_margin_bp).unwrap_or(0),
            }),
        )),
        StrAction::RequestVoluntaryClosure => Some(Decided::new(
            StrAction::RequestVoluntaryClosure,
            DecisionReason::Firm(FirmReason::VoluntaryClosure {
                months: v.sites.iter().map(|s| s.months_in_loss).max().unwrap_or(0),
                cash: v.cash,
            }),
        )),
    }
}

/// Warianty, o które firma pyta model.
///
/// Pytania biorą się z **jej własnych ksiąg**, nie z prognozy: zakład w ciągu strat
/// rodzi pytanie „zamknąć?", pełna obsada i gotówka w kasie — „otworzyć drugi?",
/// a trwała strata przy pustej kasie — „zwinąć?". Model odpowiada tylko na to,
/// o co się go zapyta, i nigdy nie podpowiada pytania.
#[must_use]
pub fn propose_variants(v: &FirmView) -> SmallVec<[StrAction; MAX_VARIANTS]> {
    let mut out: SmallVec<[StrAction; MAX_VARIANTS]> = SmallVec::new();
    out.push(StrAction::KeepCourse);

    // Najgorszy zakład — kandydat do zamknięcia.
    if let Some(s) = v
        .sites
        .iter()
        .filter(|s| s.months_in_loss > 0)
        .max_by_key(|s| s.months_in_loss)
    {
        out.push(StrAction::CloseSite {
            site: s.site,
            slots: s.headcount.saturating_add(s.vacancies),
        });
    }

    // Ekspansja: firma z gotówką i bez wakatów pyta o drugi zakład w dzielnicy,
    // w której już jest. Rozmiar bierze z zakładu, który już prowadzi — makro nie
    // zna katalogu typów, a pytanie „ile etatów" ma mieć odpowiedź z jej świata.
    let wakaty: u32 = v.sites.iter().map(|s| s.vacancies).sum();
    if wakaty == 0 {
        if let Some(s) = v.sites.iter().max_by_key(|s| s.headcount) {
            let etaty = s.headcount.max(1);
            let capex = Money(i64::from(etaty) * CAPEX_PER_SLOT);
            if v.cash.get() > capex.get() {
                out.push(StrAction::OpenSite {
                    district: s.district,
                    slots: etaty,
                    capex,
                });
            }
        }
    }

    // Wyjście: wszystkie zakłady w trwałej stracie i kasa na dnie.
    let wszystkie_w_stracie =
        !v.sites.is_empty() && v.sites.iter().all(|s| s.months_in_loss >= EXIT_MONTHS);
    if wszystkie_w_stracie && v.cash.get() <= 0 {
        out.push(StrAction::RequestVoluntaryClosure);
    }
    out
}

/// Nakład na jedno stanowisko przy otwarciu zakładu, w groszach.
///
/// `ponytail:` jedna liczba zamiast `SiteType::capex(floor_m2)` z katalogu. Sufit
/// nazwany: firma pyta o „drugi taki sam zakład", a nie o konkretny typ z `data/
/// site_types/`, więc wybór typu i powierzchni nie ma tu jeszcze decydenta.
/// Ścieżka wyjścia: wykonawca w `sim/economy` zna katalog i przy otwarciu i tak
/// liczy nakład z niego — ta stała służy wyłącznie do **odsiania wariantu,
/// na który firmy nie stać**, i dlatego jest z grubsza, a nie co do grosza.
const CAPEX_PER_SLOT: i64 = 3_000_000;

/// Ile miesięcy strat wszystkich zakładów czyni pytanie o zwinięcie firmy sensownym.
const EXIT_MONTHS: u8 = 6;

/// Uporządkowania wariantów, jedno na firmę — zasób świata pisany przez model makro.
///
/// # Dlaczego ten typ mieszka tutaj, a nie w `sim/macro`
///
/// Bo czytają go **dwa** crate'y stojące po dwóch stronach modelu: `sim/macro` pisze,
/// `sim/economy::ai_run` czyta przy składaniu `FirmView`. `sim/economy` nie widzi
/// `sim/macro` i widzieć nie może — makro stoi w grafie **nad** gospodarką, bo z niej
/// bierze jądro (`K-50`). Wspólnym przodkiem obu jest `sim/firms` i to jest cały
/// powód adresu; ta sama reguła, która trzyma słowniki w `engine/core` (`K-8`).
///
/// Wchodzi do hasha stanu: uporządkowanie steruje decyzjami firm, więc jego rozjazd
/// byłby rozjazdem gospodarki.
#[derive(Clone, Debug, Default)]
pub struct StrategicOutlooks {
    map: std::collections::BTreeMap<crate::FirmKey, Outlook>,
}

impl StrategicOutlooks {
    #[must_use]
    pub fn new() -> StrategicOutlooks {
        StrategicOutlooks::default()
    }

    pub fn set(&mut self, key: crate::FirmKey, outlook: Outlook) {
        self.map.insert(key, outlook);
    }

    #[must_use]
    pub fn get(&self, key: crate::FirmKey) -> Option<&Outlook> {
        self.map.get(&key)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Zdejmuje uporządkowania starsze niż kwartał. Prognoza sprzed pół roku nie
    /// jest „trochę mniej pewna" — jest o innym świecie.
    pub fn expire(&mut self, now: Tick, max_age_minutes: u64) {
        self.map
            .retain(|_, o| now.get().saturating_sub(o.at.get()) <= max_age_minutes);
    }
}

impl magnat_core::HashState for StrategicOutlooks {
    fn hash_state(&self, h: &mut magnat_core::StateHasher) {
        h.write_u32(self.map.len() as u32);
        for (k, o) in &self.map {
            h.write_u64(k.0);
            h.write_u32(o.variants.len() as u32);
            for a in &o.variants {
                hash_action(a, h);
            }
            h.write_u8(o.winner.unwrap_or(u8::MAX));
            h.write_u8(o.trend.as_index() as u8);
            h.write_u32(o.error_margin_bp);
            h.write_u64(o.at.get());
        }
    }
}

fn hash_action(a: &StrAction, h: &mut magnat_core::StateHasher) {
    match a {
        StrAction::KeepCourse => h.write_u8(0),
        StrAction::OpenSite {
            district,
            slots,
            capex,
        } => {
            h.write_u8(1);
            h.write_u16(district.0);
            h.write_u32(*slots);
            h.write_i64(capex.get());
        }
        StrAction::CloseSite { site, slots } => {
            h.write_u8(2);
            h.write_u64(site.0.to_bits());
            h.write_u32(*slots);
        }
        StrAction::RequestVoluntaryClosure => h.write_u8(3),
    }
}
