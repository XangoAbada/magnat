//! System miasta i wpięcie go w świat (M8a §5.1).
//!
//! **Jeden system, kadencja dobowa, wyłączny** (`K-21`). Wyłączny, bo krok miasta
//! jest funkcją nad całym światem naraz: potrzebuje ksiąg, rynku, rejestru firm
//! i własnego rejestru należności. Rozpisanie go na deklaracje dostępu nie kupiłoby
//! ani jednej krawędzi DAG, dałoby za to fałszywą deklarację, gdyby ktoś czegoś
//! nie wypisał.
//!
//! **Kolejność wobec rynku jest kontraktem** (`K-42`, `K-51`): miasto stoi **po**
//! `economy.Market`, bo to tamten system wypłaca dochody, domyka miesiąc zakładów
//! i odbiera minutę łańcucha dostaw. Miasto naliczające przed nim liczyłoby VAT
//! od sprzedaży, której jeszcze nie było, i CIT z miesiąca, który jeszcze się nie
//! domknął. Ograniczenie jest warunkowe, bo scenariusz stawiający samo miasto
//! bez detalu jest dopuszczalny — a wtedy nie ma na co czekać.

use magnat_core::{Cadence, Money, SiteId, Tick};
use magnat_economy::{Books, Market};
use magnat_ecs::{System, SystemCtx, SystemDesc, SystemId, World};

use crate::assess;
use crate::budget;
use crate::city::City;
use crate::settle;

/// Wpina miasto w świat razem z jego hakiem podatkowym.
///
/// Silnik podatkowy idzie do rynku **tutaj**, a nie przy budowie rynku: `Market`
/// powstaje w M5 i nie ma prawa wiedzieć, że istnieje `sim/city`. Zależność idzie
/// w jedną stronę i to jest jedyna poprawna strona tej relacji.
pub fn register_city(world: &mut World, city: City) {
    if let Some(market) = world.get_resource::<Market>() {
        market.set_tax_engine(Box::new(city.tax_engine()));
    }
    world.insert_resource(city);
    world.register_resource_hash::<City>();
}

/// Dobowy krok miasta.
pub struct CitySystem {
    desc: SystemDesc,
    /// Bufor zakładów do naliczeń rocznych — raz w roku, ale dla wszystkich naraz,
    /// więc alokacja per rok byłaby alokacją na kilka tysięcy wierszy.
    sites: Vec<SiteId>,
    licensed: Vec<(SiteId, u16)>,
}

impl CitySystem {
    #[must_use]
    pub fn new() -> CitySystem {
        CitySystem {
            desc: SystemDesc::new("city.Tax", Cadence::EveryDay)
                .exclusive()
                .after_if_present(SystemId::from_name("economy.Market")),
            sites: Vec::new(),
            licensed: Vec::new(),
        }
    }
}

impl Default for CitySystem {
    fn default() -> CitySystem {
        CitySystem::new()
    }
}

impl System for CitySystem {
    fn desc(&self) -> &SystemDesc {
        &self.desc
    }

    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let t = ctx.tick;
        let Some(market) = ctx.world().get_resource::<Market>().cloned() else {
            return;
        };
        // Miasto wychodzi ze świata na czas kroku, tak samo jak finanse firm
        // w `economy.Market`: dwóch pożyczek `&mut World` naraz nie ma.
        let Some(mut city) = ctx
            .world_mut()
            .get_resource_mut::<City>()
            .map(std::mem::take)
        else {
            return;
        };
        if city.is_active() {
            self.krok(&mut city, &market, ctx, t);
        }
        if let Some(slot) = ctx.world_mut().get_resource_mut::<City>() {
            *slot = city;
        }
    }
}

impl CitySystem {
    fn krok(&mut self, city: &mut City, market: &Market, ctx: &mut SystemCtx<'_>, t: Tick) {
        let rest = market.rest_of_world();

        // 1. Daniny hurtu — codziennie, bo cło płaci się na granicy, a nie
        //    z deklaracji, a akcyza nalicza się w chwili, gdy wyrób zmienia
        //    właściciela. Termin ma za to miesięczny, tak jak VAT.
        assess::clo_i_akcyza(city, market, t);

        if settle::is_month_start(t) {
            // 2. Deklaracja miesięczna: VAT i akcyza z zakładów, zaliczki PIT
            //    od pracodawców, rata podatku od nieruchomości.
            //
            //    Kolejność nie ma znaczenia dla kwot, bo każda z tych danin ma
            //    własne źródło — ma za to znaczenie dla czytelności dziennika,
            //    więc idzie tak, jak w tabeli kontraktu księgowania (§5.1).
            assess::vat_i_akcyza(city, market, t);
            assess::pit(city, t);
            assess::nieruchomosci(city, market, t);

            if settle::is_year_start(t) {
                self.rok(city, market, ctx, t);
            }
        }

        // 3. Rozliczenie — jedyne miejsce, w którym pieniądz przechodzi do miasta.
        if let Some(books) = ctx.world_mut().get_resource_mut::<Books>() {
            settle::settle_due(city, market, books, rest, t);
        }
        // 4. Starzenie zaległości: odsetki i przedawnienie.
        settle::age_overdue(city, t);

        // 5. Budżet. **Po** rozliczeniu, bo miasto wydaje to, co wpłynęło —
        //    odwrotna kolejność planowałaby wydatki z zeszłomiesięcznego salda.
        if settle::is_month_start(t) {
            let wplywy = std::mem::replace(&mut city.month_revenue, Money::ZERO);
            let polityka = city.policy.clone();
            if let Some(books) = ctx.world_mut().get_resource_mut::<Books>() {
                budget::close_month(&mut city.budget, &polityka, books, rest, wplywy, t);
            }
            if settle::is_year_start(t) {
                budget::close_year(&mut city.budget);
            }
        }
    }

    /// Naliczenia roczne: CIT za rok miniony i koncesje na rok rozpoczynany.
    ///
    /// Tu też zeruje się licznik zaliczek PIT — bez tego drugi rok liczyłby
    /// zaliczki od dochodu dwuletniego i każdy mieszkaniec wpadłby w drugi próg.
    fn rok(&mut self, city: &mut City, market: &Market, ctx: &mut SystemCtx<'_>, t: Tick) {
        let rok = u16::try_from(magnat_core::SimCalendar::new(t).year()).unwrap_or(0);
        if city.last_year_assessed == rok {
            return;
        }
        city.last_year_assessed = rok;

        self.sites.clear();
        self.licensed.clear();
        if let Some(firms) = ctx.world().get_resource::<magnat_firms::Firms>() {
            for (site, s) in firms.sites() {
                self.sites.push(site);
                self.licensed.push((site, s.site_type.0));
            }
        } else {
            self.sites.extend(market.sites());
        }

        assess::cit(city, market, &self.sites, t);
        assess::koncesje(city, market, &self.licensed, t);
        city.withholding.reset_year();
    }
}
