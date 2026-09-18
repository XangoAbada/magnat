//! Ścieżka kariery bez sztucznych blokad (M9e §5.4, PRD §13.2).
//!
//! # Tier jest etykietą, nie bramą
//!
//! [`CareerTier`] **wyprowadza się** z obserwowalnych faktów i nigdy nie jest
//! ustawiany. Żadna komenda go nie sprawdza — jedynymi ograniczeniami gracza są
//! kapitał, ludzie, informacja i czas. Test akceptacyjny sprawdza to wprost:
//! w wariancie `Sandbox` gracz zakłada firmę w pierwszej dobie i komenda przechodzi.
//!
//! Do czego tier służy: do tytułu w kronice, do statystyk i do **domyślnego układu
//! paneli** ([`crate::panels::PanelDesc::min_tier`]). To ostatnie jest jedynym
//! miejscem, gdzie tier w ogóle na coś wpływa, i wpływa wyłącznie na to, co jest
//! przypięte nowemu graczowi — nie na to, co wolno otworzyć.
//!
//! # Skąd biorą się liczby
//!
//! [`Holdings`] to jedno przejście po rejestrze firm: zakłady gracza, załoga,
//! menedżerowie, branże i integracja pionowa. Udział w rynku liczy się **dopiero
//! wtedy**, gdy reszta warunków dobiła do [`CareerTier::Group`] — inaczej każdy
//! pracownik bez firmy płaciłby za pętlę po wszystkich sklepach miasta.

use magnat_core::SiteId;
use magnat_firms::{FirmKey, Firms, Owner};

use crate::Session;

/// Etap kariery gracza. Kolejność wariantów jest kolejnością rosnącą — `min_tier`
/// panelu porównuje się nią wprost.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub enum CareerTier {
    #[default]
    Employee,
    FirstBusiness,
    Company,
    Group,
    Magnate,
}

impl CareerTier {
    pub const ALL: [CareerTier; 5] = [
        CareerTier::Employee,
        CareerTier::FirstBusiness,
        CareerTier::Company,
        CareerTier::Group,
        CareerTier::Magnate,
    ];

    /// Klucz tekstu: `ui.career.<key>`.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            CareerTier::Employee => "employee",
            CareerTier::FirstBusiness => "first_business",
            CareerTier::Company => "company",
            CareerTier::Group => "group",
            CareerTier::Magnate => "magnate",
        }
    }

    /// Wyprowadza etap z bieżącego stanu świata.
    ///
    /// Gra bez postaci (tryb przeglądu) jest `Employee` — nie dlatego, że gracz
    /// gdzieś pracuje, tylko dlatego, że niczego nie prowadzi, a to jest ta sama
    /// odpowiedź.
    #[must_use]
    pub fn derive(session: &Session) -> CareerTier {
        Holdings::of(session).tier(session)
    }
}

/// Co gracz prowadzi — jedno przejście po rejestrze firm.
///
/// Osobno od [`CareerTier`], bo tych samych liczb potrzebuje pulpit firmy: „ile
/// zakładów, ilu ludzi, ilu menedżerów" jest pytaniem gracza, a nie tylko wejściem
/// do etykiety.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Holdings {
    /// Zakłady gracza, posortowane po kluczu — kolejność jest deterministyczna,
    /// bo idzie po `Firms::sites()`, czyli po `BTreeMap`.
    pub sites: Vec<SiteId>,
    pub firms: Vec<FirmKey>,
    /// Obsadzone etaty we wszystkich zakładach gracza.
    pub employees: u32,
    /// Zakłady z przypisanym menedżerem-mieszkańcem.
    pub managers: u32,
    /// Ile różnych rodzajów zakładu gracz prowadzi — to jest „branża" z §5.4.
    pub kinds: u32,
    /// Czy któryś zakład gracza dostarcza innemu jego zakładowi (integracja pionowa).
    pub vertical: bool,
}

impl Holdings {
    /// Stan posiadania gracza. Pusty, gdy gra nie ma postaci.
    #[must_use]
    pub fn of(session: &Session) -> Holdings {
        let mut h = Holdings::default();
        let Some(firms) = session
            .app
            .world
            .get_resource::<Firms>()
            .filter(|_| session.player().is_some())
        else {
            return h;
        };
        let mut rodzaje: Vec<u16> = Vec::new();
        for (key, f) in firms.iter() {
            if !f.owners.iter().any(|o| o.owner == Owner::Player) {
                continue;
            }
            h.firms.push(key);
            for id in &f.sites {
                let Some(s) = firms.site(*id) else {
                    continue;
                };
                h.sites.push(*id);
                h.employees += s
                    .positions
                    .iter()
                    .map(|p| u32::try_from(p.filled.len()).unwrap_or(u32::MAX))
                    .sum::<u32>();
                if s.delegation.as_ref().is_some_and(|d| d.manager.is_some()) {
                    h.managers += 1;
                }
                if !rodzaje.contains(&s.site_type.0) {
                    rodzaje.push(s.site_type.0);
                }
            }
        }
        // Zakład z listy postaci, którego rejestr firm nie zna (wariant `Heir` bierze
        // pierwszy sklep miasta, zanim firma gracza w ogóle powstanie), też się liczy —
        // inaczej spadkobierca byłby pracownikiem z własnym sklepem.
        if let Some(p) = session.player() {
            for s in &p.owned_sites {
                if !h.sites.contains(s) {
                    h.sites.push(*s);
                }
            }
        }
        h.sites.sort_unstable_by_key(|s| s.0.to_bits());
        h.kinds = u32::try_from(rodzaje.len()).unwrap_or(0);
        h.vertical = wewnetrzny_dostawca(session, &h.sites);
        h
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sites.is_empty()
    }

    /// Etap kariery z tego stanu posiadania.
    #[must_use]
    pub fn tier(&self, session: &Session) -> CareerTier {
        if self.sites.is_empty() {
            return CareerTier::Employee;
        }
        let podstawa = if self.kinds >= 2 || self.vertical {
            CareerTier::Group
        } else if self.sites.len() >= 2 || self.managers >= 1 {
            CareerTier::Company
        } else if self.employees <= 3 {
            CareerTier::FirstBusiness
        } else {
            CareerTier::Company
        };
        // Udział w rynku kosztuje przejście po wszystkich sklepach miasta, więc
        // pyta o niego wyłącznie ten, kto mógłby na nie odpowiedzieć „tak".
        if podstawa < CareerTier::Group {
            return podstawa;
        }
        if self.employment_bp(session) >= 500 || self.best_share_bp(session) >= 2500 {
            CareerTier::Magnate
        } else {
            podstawa
        }
    }

    /// Udział gracza w zatrudnieniu miasta, w punktach bazowych.
    #[must_use]
    pub fn employment_bp(&self, session: &Session) -> u16 {
        let Some(firms) = session.app.world.get_resource::<Firms>() else {
            return 0;
        };
        let wszyscy = i64::try_from(firms.headcount()).unwrap_or(i64::MAX);
        if wszyscy <= 0 {
            return 0;
        }
        let bp = i64::from(self.employees).saturating_mul(10_000) / wszyscy;
        u16::try_from(bp.clamp(0, 10_000)).unwrap_or(0)
    }

    /// Najwyższy udział gracza w obrocie pojedynczym towarem, w punktach bazowych.
    ///
    /// Zero znaczy „w niczym nie ma udziału" **albo** „nie ma gospodarki" — dla
    /// etykiety kariery jedno i drugie znaczy to samo, więc rozróżnienia tu nie ma.
    #[must_use]
    pub fn best_share_bp(&self, session: &Session) -> u16 {
        let Some(m) = session.market.as_ref() else {
            return 0;
        };
        let mut towary: Vec<magnat_core::GoodId> = Vec::new();
        for s in &self.sites {
            for g in m.goods_of(*s) {
                if !towary.contains(&g) {
                    towary.push(g);
                }
            }
        }
        towary
            .into_iter()
            .filter_map(|g| m.turnover_share_bp(g, None, &self.sites))
            .max()
            .unwrap_or(0)
    }
}

/// Czy któryś zakład gracza dostarcza innemu jego zakładowi.
///
/// Umowa B2B jest jedynym miejscem, w którym „dostawca wewnętrzny" z §5.4 ma
/// obserwowalny ślad: obie strony są zakładami z nazwy, a nie domysłem z branży.
fn wewnetrzny_dostawca(session: &Session, sites: &[SiteId]) -> bool {
    if sites.len() < 2 {
        return false;
    }
    let Some(m) = session.market.as_ref() else {
        return false;
    };
    let chain = m.chain();
    let c = chain.lock();
    let jest = c
        .b2b
        .contracts()
        .any(|k| sites.contains(&k.deliver_from) && sites.contains(&k.deliver_to));
    jest
}
