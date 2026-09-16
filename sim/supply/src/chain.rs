//! Łańcuch dostaw jako jeden zasób i jeden zegar (M6d, WP11).
//!
//! **Dlaczego jeden zasób, a nie cztery** (`AL-3`). `AJ-1` zapowiadało `Store`, `Plant`,
//! `Transport` i `B2b` jako cztery osobne zasoby świata, każdy wpięty przez
//! `register_resource_hash`. To jest ten sam błąd, który `AD-7` naprawił dla partii
//! i slotów, a `AG-2` dla linii i rampy, tylko o piętro wyżej: `World::resource_mut`
//! pożycza **cały** świat, a każda operacja łańcucha dotyka co najmniej dwóch z tych
//! czterech naraz — rozstrzygnięcie zapytania podpisuje kontrakt (`B2b`), wystawia
//! zlecenie (`Transport`) i ładuje partię (`Store`) w jednym kroku. Cztery zasoby
//! znaczyłyby cztery bufory komend i rozbicie tego kroku na cztery ticki.
//!
//! `K-29` dopuszcza to wprost i nie wymaga nowego rozstrzygnięcia. Sekcja hasha jest
//! jedna (`Chain`), a kolejność jej składników jest kontraktem.
//!
//! **Zegar.** Trzy funkcje kadencji ([`Chain::step_minute`], [`Chain::step_hour`],
//! [`Chain::step_day`]) robią to, co §5.11 opisuje jako osiem systemów ECS. Tu są
//! zwykłymi funkcjami, bo `sim/supply` nie zależy od `magnat-ecs` i zależeć nie musi:
//! M6e opakuje każdą z nich w system i doda rozproszenie po indeksie encji, zamiast
//! pisać tę pętlę drugi raz. Kolejność wewnątrz kadencji **jest** kontraktem z `AJ-3`:
//! kaskada przed rynkiem, przybycia przed eksportem.

use std::sync::{Arc, Mutex, MutexGuard};

use magnat_core::{FirmId, HashState, Money, SimMinute, SiteId, StateHasher};

use crate::b2b::{Settlement, B2b};
use crate::catalog::Catalog;
use crate::inventory::{InventoryRule, PreferredSource};
use crate::mining::{Deposits, NoDeposits};
use crate::plant::{advance_production, Plant, ProductionCtx};
use crate::shortage::{self, ShortageAction};
use crate::transport::{Carrier, FreightOracle, Transport, TransportOrderState};
use crate::tuning::Tuning;
use crate::Store;

/// Wszystko, co M6 trzyma o łańcuchu dostaw miasta.
pub struct Chain {
    pub store: Store,
    pub plant: Plant,
    pub transport: Transport,
    pub b2b: B2b,
    /// Reguły zapasu per zakład. Polityka („ile i kiedy") jest własnością tego, kto
    /// zakład prowadzi — dla sklepu jest nią M5 i to on te liczby odświeża; M6
    /// wyłącznie je wykonuje.
    pub rules: Vec<(SiteId, Vec<InventoryRule>)>,
}

impl Chain {
    #[must_use]
    pub fn new(goods: usize, b2b: B2b) -> Chain {
        Chain {
            store: Store::new(goods),
            plant: Plant::new(),
            transport: Transport::new(),
            b2b,
            rules: Vec::new(),
        }
    }

    /// Ustawia reguły zapasu zakładu, zachowując porządek po `SiteId` (00 §3.2).
    pub fn set_rules(&mut self, site: SiteId, rules: Vec<InventoryRule>) {
        match self.rules.binary_search_by_key(&site, |(s, _)| *s) {
            Ok(i) => self.rules[i].1 = rules,
            Err(i) => self.rules.insert(i, (site, rules)),
        }
    }

    /// Minuta łańcucha: produkcja, przybycia, psucie.
    ///
    /// Kolejność jest kontraktem: `SpoilageSystem` **przed** sprzedażą (`D12`), więc
    /// psucie zamyka minutę, a nie zaczyna ją. Detal woła sprzedaż po tej funkcji.
    pub fn step_minute(
        &mut self,
        cat: &Catalog,
        tuning: &Tuning,
        oracle: &dyn FreightOracle,
        deposits: &dyn Deposits,
        world_seed: u64,
        now: SimMinute,
    ) -> Vec<crate::store::Spoiled> {
        let ctx = ProductionCtx {
            cat,
            tuning,
            world_seed,
            deposits,
        };
        let sites: Vec<SiteId> = self.plant.sites().collect();
        for s in &sites {
            advance_production(&ctx, &mut self.store, &mut self.plant, *s, now, 1);
        }
        self.wyslij_gotowe(oracle, now);
        self.odbierz_przybyle(cat, now);
        self.store.spoil(now)
    }

    /// Godzina łańcucha: kaskada niedoboru, przegląd zapasów, rynek.
    ///
    /// `AJ-3`: kaskada **przed** rynkiem, bo rynek konsumuje akcje wyprodukowane przez
    /// kaskadę w tej samej godzinie. Odwrócona kolejność nie wywala się — daje
    /// o godzinę starsze dane, czyli `SpotSearch` z uchwytem sprzed godziny.
    pub fn step_hour(
        &mut self,
        cat: &Catalog,
        tuning: &Tuning,
        oracle: &dyn FreightOracle,
        now: SimMinute,
    ) -> Vec<Settlement> {
        let sites: Vec<SiteId> = self.plant.sites().collect();
        let mut akcje: Vec<ShortageAction> = Vec::new();
        for s in &sites {
            if let Some(p) = self.plant.get_mut(*s) {
                let mut wlasne = shortage::review(cat, &self.store, p, now, &tuning.shortage);
                akcje.append(&mut wlasne);
            }
        }
        akcje.extend(self.przeglad_zapasow(cat, now));
        self.b2b.serve(
            &akcje,
            cat,
            &self.store,
            &mut self.plant,
            oracle,
            tuning,
            now,
        );

        let mut rozliczenia =
            self.b2b
                .resolve_due(cat, &mut self.store, &mut self.transport, oracle, tuning, now);
        rozliczenia.append(&mut self.b2b.run_contracts(
            cat,
            &mut self.store,
            &mut self.transport,
            oracle,
            now,
        ));
        rozliczenia.append(&mut self.b2b.poll_imports(
            cat,
            &mut self.store,
            &mut self.transport,
            oracle,
            now,
        ));
        rozliczenia
    }

    /// Doba łańcucha: indeks dostawców, okno cen spot, wchłonięcie eksportu,
    /// scalanie partii i sprzątanie zamkniętych zleceń.
    pub fn step_day(&mut self, cat: &Catalog, tuning: &Tuning) {
        self.b2b.reindex(cat, &self.plant);
        self.b2b.roll_day(tuning);
        self.b2b.absorb_exports(&mut self.store);
        self.transport.prune();
    }

    /// Faktury za media — wychodzą z łańcucha **listą faktów**, tym samym wzorcem
    /// co `Settlement` (`AI-1`): `sim/supply` księgi nie widzi i widzieć nie może.
    pub fn bill_utilities(&mut self, until: SimMinute) -> Vec<(SiteId, FirmId, Money)> {
        self.plant.bill_utilities(until)
    }

    /// Przegląd zapasów wszystkich zakładów zamieniony na akcje rynku.
    ///
    /// `ReplenishRequest` i `ShortageAction` odpowiadają na dwa różne pytania —
    /// „ile brakuje do celu" i „co robić, bo już brakuje" — ale rynek obsługuje
    /// jedną listę, więc przegląd wchodzi do niej jako zwykłe zapytanie ofertowe.
    /// Preferencja zakładu rozstrzyga, czy idzie do rynku lokalnego, czy od razu
    /// do importu; `Any` znaczy „najpierw szukaj u siebie".
    fn przeglad_zapasow(&self, cat: &Catalog, now: SimMinute) -> Vec<ShortageAction> {
        let mut akcje = Vec::new();
        for (site, rules) in &self.rules {
            let Some(p) = self.plant.get(*site) else {
                continue;
            };
            for r in crate::inventory::review(cat, &self.store, &self.transport, p, rules, now) {
                // `Any` znaczy „najpierw szukaj u siebie" — i to „najpierw" trzeba
                // rozstrzygnąć **tutaj**, bo zapytanie ofertowe bez ani jednego
                // dostawcy nie kończy się importem, tylko odmową. Miasto, w którym
                // nikt jeszcze nie produkuje mąki, sprowadza ją zza granicy; kiedy
                // stanie pierwszy młyn, wygra ceną, bo transport zza granicy kosztuje.
                let lokalnie = !self.b2b.sellers_of(r.good).is_empty();
                let import = matches!(r.preferred, PreferredSource::Import)
                    || (!lokalnie && !matches!(r.preferred, PreferredSource::Spot));
                akcje.push(if import {
                    ShortageAction::Import {
                        site: r.site,
                        good: r.good,
                        mass: r.mass,
                    }
                } else {
                    ShortageAction::OpenRfq {
                        site: r.site,
                        good: r.good,
                        mass: r.mass,
                    }
                });
            }
        }
        akcje
    }

    /// Wysyła zlecenia, które czekają w `Draft` i mają już towar w magazynie nadawcy.
    fn wyslij_gotowe(&mut self, oracle: &dyn FreightOracle, now: SimMinute) {
        let gotowe: Vec<_> = self
            .transport
            .iter()
            .filter(|o| o.state == TransportOrderState::Draft && o.ready_at.0 <= now.0)
            .map(|o| o.id)
            .collect();
        for id in gotowe {
            let _ = self
                .transport
                .dispatch(oracle, &mut self.store, id, Carrier::Unassigned, now);
        }
    }

    /// Rozładowuje to, co dojechało.
    fn odbierz_przybyle(&mut self, cat: &Catalog, now: SimMinute) {
        let przybyle: Vec<_> = self
            .transport
            .iter()
            .filter(|o| matches!(o.state, TransportOrderState::EnRoute { eta } if eta.0 <= now.0))
            .map(|o| o.id)
            .collect();
        for id in przybyle {
            let _ = self.transport.deliver(cat, &mut self.store, id, now);
        }
    }
}

impl HashState for Chain {
    fn hash_state(&self, h: &mut StateHasher) {
        self.store.hash_state(h);
        self.plant.hash_state(h);
        self.transport.hash_state(h);
        self.b2b.hash_state(h);
        h.write_u32(self.rules.len() as u32);
        for (s, r) in &self.rules {
            s.entity().hash_state(h);
            h.write_u32(r.len() as u32);
            for x in r {
                x.hash_state(h);
            }
        }
    }
}

/// Uchwyt do łańcucha, współdzielony z detalem.
///
/// Ten sam wzorzec co `Market` w M5 i `TrafficOracle` w M4, i z tego samego powodu
/// (`K-29`): sprzedaż zdejmuje towar z półki, a półka jest slotem magazynu — więc
/// kod detalu, który dostaje `&self`, musi umieć sięgnąć do `&mut Store`.
///
/// **Kolejność zamków jest kontraktem:** najpierw `Market`, potem `Chain`. Łańcuch
/// nigdy nie sięga do rynku detalicznego, więc odwrotna kolejność nie ma prawa się
/// zdarzyć, a przy jednej kolejności zakleszczenie jest niemożliwe.
#[derive(Clone)]
pub struct ChainHandle {
    inner: Arc<Mutex<Chain>>,
    /// Katalog, strojenie i trasa są **wejściem**, nie stanem: nie zmieniają się
    /// w przebiegu i nie wchodzą do hasha, więc stoją poza zamkiem.
    pub cat: Arc<Catalog>,
    pub tuning: Arc<Tuning>,
    pub oracle: Arc<dyn FreightOracle>,
    pub deposits: Arc<dyn Deposits + Send + Sync>,
}

impl ChainHandle {
    #[must_use]
    pub fn new(
        chain: Chain,
        cat: Arc<Catalog>,
        tuning: Arc<Tuning>,
        oracle: Arc<dyn FreightOracle>,
    ) -> ChainHandle {
        ChainHandle {
            inner: Arc::new(Mutex::new(chain)),
            cat,
            tuning,
            oracle,
            deposits: Arc::new(NoDeposits),
        }
    }

    /// Łańcuch z **jedną bramą towarową**, przez którą da się sprowadzić wszystko,
    /// co ma cenę zewnętrzną i wolno wieźć drogą.
    ///
    /// Istnieje, bo miasto bez ani jednego węzła granicznego nie ma skąd wziąć towaru
    /// w chwili zero: pierwszy młyn zaczyna mleć w minucie zero, a pierwszy klient
    /// przychodzi w tej samej minucie. Brama jest przy tym **prawdziwym** źródłem,
    /// a nie zaślepką: ma przepustowość dobową, zaległość wydłużającą kolejkę, cenę
    /// rosnącą z wolumenem i cło osobno od ceny (§5.9).
    ///
    /// Jedno przeliczenie, które trzeba tu mieć na oku: `external_base_price` jest
    /// **za 1000 jednostek natywnych** (za kilogram towaru masowego), a cena węzła
    /// jest **za tonę**.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn with_import_gate(
        cat: Arc<Catalog>,
        tuning: Arc<Tuning>,
        oracle: Arc<dyn FreightOracle>,
        tariffs: crate::b2b::TariffTable,
        seed: u64,
        gate: SiteId,
        capacity_per_day: magnat_core::Mass,
        lead_minutes: u32,
    ) -> ChainHandle {
        let mut store = Store::new(cat.goods.len());
        let slot = store.add_slot(
            gate,
            crate::store::WarehouseRole::Distribution,
            crate::catalog::StorageClass::Yard,
            magnat_core::Mass(capacity_per_day.0.saturating_mul(4)),
            magnat_core::Volume(i64::MAX / 4),
            // Brama przyjmuje wszystko, także towary niebezpieczne: plac celny ma
            // na to zgodę z definicji, inaczej paliwo nie przekroczyłoby granicy.
            0xFF,
        );
        let mut b2b = B2b::new(cat.goods.len(), tariffs, seed);
        let mut goods = Vec::new();
        for g in &cat.goods {
            let (Some(cena), Some(klasa)) = (g.external_base_price, b2b.tariffs().class_of(&g.key))
            else {
                continue;
            };
            if !g.gates().contains(&magnat_core::GateKind::Highway) {
                continue;
            }
            goods.push(crate::b2b::TradeGood {
                good: g.id,
                base_price: magnat_core::Money(cena.get().saturating_mul(1_000)),
                elasticity_permille: tuning.trade.elasticity_permille,
                window_reference: capacity_per_day,
                bought_window: magnat_core::Mass::ZERO,
                export_spread_pct: tuning.trade.export_spread_pct,
                tariff_class: klasa,
            });
        }
        b2b.add_node(crate::b2b::TradeNode {
            id: crate::b2b::TradeNodeId(0),
            kind: magnat_core::GateKind::Highway,
            site: gate,
            slot,
            capacity_per_day,
            used_today: magnat_core::Mass::ZERO,
            backlog: magnat_core::Mass::ZERO,
            base_lead_minutes: lead_minutes,
            goods,
        });
        ChainHandle::new(
            Chain {
                store,
                plant: Plant::new(),
                transport: Transport::new(),
                b2b,
                rules: Vec::new(),
            },
            cat,
            tuning,
            oracle,
        )
    }

    /// Podmienia bilans złóż. Osobno od konstruktora, bo złoża wnosi M1 i wołający
    /// zwykle składa łańcuch wcześniej, niż ma do nich dostęp.
    #[must_use]
    pub fn with_deposits(mut self, d: Arc<dyn Deposits + Send + Sync>) -> ChainHandle {
        self.deposits = d;
        self
    }

    /// # Panics
    /// Gdy zamek jest zatruty — czyli gdy inny wątek spanikował w środku operacji
    /// magazynowej. Kontynuowanie na połowicznie zmienionym magazynie byłoby gorsze
    /// niż panika, bo bilans masy przestałby się domykać bez śladu przyczyny.
    pub fn lock(&self) -> MutexGuard<'_, Chain> {
        self.inner.lock().expect("łańcuch dostaw")
    }
}

impl HashState for ChainHandle {
    fn hash_state(&self, h: &mut StateHasher) {
        self.lock().hash_state(h);
    }
}
