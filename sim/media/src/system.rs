//! System reklamy i mediów (M10b §5.2, §5.3).
//!
//! Jeden system, bo osiem kanałów to osiem sposobów wybrania odbiorcy, a nie osiem
//! mechanizmów — dzielą pamięć marki, budżet i księgę. Wyłączny (`K-21`), bo zapisuje
//! sloty marek, rejestr kampanii, tytuły, księgi zakładów i podsłuch krawędzi w ruchu:
//! deklaracja dostępów wypisałaby połowę świata i byłaby fałszywa przy pierwszym
//! przeoczeniu.
//!
//! **Rytm jest godzinowy, a nie dobowy**, i decyduje o tym billboard: podsłuch ruchu
//! (`EdgeWatch`) zbiera przejazdy z każdej minuty i ma sufit bufora. Reszta kanałów,
//! redakcja i rozliczenie idą raz na dobę, na pierwszej godzinie.

use crate::campaign::{AdCampaign, AdChannel, Campaigns};
use crate::outlet::{Outlets, Story, STORY_SPREAD_DAYS};
use crate::reach::{expose, expose_media, sample_stride, DistrictRoster};
use magnat_core::{
    rng, AdChannelKind, Cadence, DecisionReason, DistrictId, EditorialBias, Money, SimMinute,
    StreamId, Tick, Q,
};
use magnat_ecs::{System, SystemCtx, SystemDesc, World};

/// Ile ekspozycji ma jedna „tysiączna" rozliczenia kosztu (CPM).
const PER_MILLE: i64 = 1_000;

/// Jedna kampania gotowa do rozliczenia doby: numer, płatnik, kwota, kanał, odbiorca
/// pieniądza, marka i obietnica. Krotka, nie struktura — powstaje i umiera w jednej
/// pętli, a nazwanie jej typem nie kupiłoby ani jednego czytelnika.
type DoRozliczenia = (
    magnat_core::CampaignId,
    magnat_core::SiteId,
    Money,
    AdChannelKind,
    Option<magnat_core::SiteId>,
    magnat_core::BrandId,
    Q,
);

pub struct MediaSystem {
    desc: SystemDesc,
}

/// Co system zrobił w ostatnim przebiegu — wejście panelu i testów.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct MediaReport {
    pub exposures: u32,
    pub first_contacts: u32,
    pub billboard_passes: u32,
    pub stories_published: u32,
    pub spent: Money,
}

impl MediaSystem {
    #[must_use]
    pub fn new() -> MediaSystem {
        MediaSystem {
            desc: SystemDesc::new("media.Ads", Cadence::EveryHour).exclusive(),
        }
    }
}

impl Default for MediaSystem {
    fn default() -> MediaSystem {
        MediaSystem::new()
    }
}

impl System for MediaSystem {
    fn desc(&self) -> &SystemDesc {
        &self.desc
    }

    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let t = ctx.tick;
        let world = ctx.world_mut();
        // Raport z kroku czytają testy i scenariusze przez `step`; system go nie
        // przechowuje, bo stan reklamy jest w `Campaigns` i w `Outlets`, a nie
        // w liczniku ostatniej godziny.
        let _ = step(world, t);
    }
}

/// Jedna godzina reklamy i mediów — **cała treść systemu**, wołalna bez schedulera.
///
/// Wydzielona z `run`, bo krok jest funkcją nad światem i testuje się go światem,
/// a nie harmonogramem (to samo, co `society::step_day` robi dla rytmu doby).
pub fn step(world: &mut World, t: Tick) -> MediaReport {
    let mut raport = MediaReport::default();
    sync_watch(world, t);
    billboardy(world, t, &mut raport);
    if t.0 % 1_440 < 60 {
        doba(world, t, &mut raport);
    }
    raport
}

/// Ustawia ruchowi listę krawędzi, które ma podsłuchiwać.
///
/// Kampania nie sięga do ruchu sama: mówi, gdzie stoi jej tablica, a ruch odpowiada,
/// kto tamtędy przejechał. Świat bez billboardów dostaje pusty zbiór i nie płaci nic.
fn sync_watch(world: &mut World, t: Tick) {
    let now = SimMinute(t.0);
    let edges = world.resource::<Campaigns>().billboard_edges(now);
    if let Some(net) = world.get_resource_mut::<magnat_traffic::TrafficNetwork>() {
        if net.watch().edges() != edges.as_slice() {
            net.watch_mut().set_edges(edges);
        }
    }
}

/// Billboardy: kto przejechał obok tablicy i czy ją zauważył.
fn billboardy(world: &mut World, t: Tick, raport: &mut MediaReport) {
    let Some(net) = world.get_resource_mut::<magnat_traffic::TrafficNetwork>() else {
        return;
    };
    if net.watch().pending() == 0 {
        return;
    }
    let passes = net.watch_mut().take_passes();
    raport.billboard_passes = passes.len() as u32;
    let now = SimMinute(t.0);
    let doba = t.0 / 1_440;
    let seed = world.seed;

    // Krawędź → kampanie, które przy niej stoją. Lista jest krótka (jedna tablica
    // na krawędź w typowym mieście), więc wektor par bije mapę.
    let tablice: Vec<(magnat_nav::EdgeId, magnat_core::CampaignId, u16)> = world
        .resource::<Campaigns>()
        .live(now)
        .filter_map(|c| match c.channel {
            AdChannel::Billboard {
                edge,
                notice_rate_bps,
            } => Some((edge, c.id, notice_rate_bps)),
            _ => None,
        })
        .collect();
    if tablice.is_empty() {
        return;
    }

    for (podroznik, edge) in passes {
        let Some(e) = magnat_agents::citizen_by_index(world, podroznik) else {
            continue;
        };
        for (kraw, id, rate) in &tablice {
            if *kraw != edge {
                continue;
            }
            // Klucz jest **mieszkańcem**, a nie kampanią: gdyby losowała kampania,
            // wszyscy przejeżdżający w tej samej minucie dostaliby ten sam rzut.
            let mut r = rng(
                seed,
                StreamId::AdNotice,
                podroznik,
                Tick(t.0 + u64::from(id.0)),
            );
            if r.gen_range_u32(10_000) >= u32::from(*rate) {
                continue;
            }
            let Some(c) = world.resource::<Campaigns>().get(*id).copied() else {
                continue;
            };
            let pierwszy = expose(world, e, c.brand, AdChannelKind::Billboard, c.claim, doba);
            zapisz_ekspozycje(world, *id, e, pierwszy, raport);
        }
    }
}

/// Doliczenie ekspozycji do metryk kampanii.
///
/// `kto` jest potrzebne wyłącznie po to, żeby doliczyć ekspozycję do **dzielnicy
/// zamieszkania** odbiorcy — to jest liczba, po którą gracz otwiera panel marketingu
/// (kryterium WP10.19: „ilu mieszkańców i z jakich dzielnic"). Mieszkaniec bez
/// przypisanego miejsca zamieszkania podnosi sumę, ale nie podnosi żadnej dzielnicy.
fn zapisz_ekspozycje(
    world: &mut World,
    id: magnat_core::CampaignId,
    kto: magnat_ecs::Entity,
    pierwszy: bool,
    raport: &mut MediaReport,
) {
    let dzielnica = world
        .get::<magnat_agents::Residence>(kto)
        .map(|r| usize::from(r.district))
        .filter(|d| *d < crate::campaign::METRIC_DISTRICTS);
    if let Some(c) = world.resource_mut::<Campaigns>().get_mut(id) {
        c.metrics.exposures_today = c.metrics.exposures_today.saturating_add(1);
        c.metrics.exposures_total = c.metrics.exposures_total.saturating_add(1);
        if pierwszy {
            c.metrics.first_contacts = c.metrics.first_contacts.saturating_add(1);
        }
        if let Some(d) = dzielnica {
            c.metrics.by_district[d] = c.metrics.by_district[d].saturating_add(1);
        }
    }
    raport.exposures = raport.exposures.saturating_add(1);
    if pierwszy {
        raport.first_contacts = raport.first_contacts.saturating_add(1);
    }
}

/// Raz na dobę: pozostałe kanały, redakcja, rozchodzenie się tekstów, rozliczenie.
fn doba(world: &mut World, t: Tick, raport: &mut MediaReport) {
    let spis = DistrictRoster::build(world);
    let now = SimMinute(t.0);
    let doba = t.0 / 1_440;

    let zywe: Vec<AdCampaign> = world.resource::<Campaigns>().live(now).copied().collect();
    // Dwa kanały pytają **każdego mieszkańca z osobna** i dlatego nie idą w tej pętli:
    // promocja pyta magazyn wiedzy („czy tu bywasz"), PR pyta pamięć marki („czy ją
    // lubisz"). Wołane raz na kampanię dają koszt „liczba kampanii × liczba
    // mieszkańców" i to jest cała treść pomiaru `GF-1`: przy 250 kampaniach i 20 tys.
    // mieszkańców było to 6079 ms i 6479 ms na dobę gry wobec 225 ms budżetu na
    // wszystkie osiem kanałów razem. Zbieramy je więc do planu i przechodzimy po
    // mieście **raz na dobę dla wszystkich naraz**.
    let mut plan_promocji: Vec<Promocja> = Vec::new();
    let mut plan_pr: Vec<magnat_core::BrandId> = Vec::new();
    for c in &zywe {
        match c.channel {
            AdChannel::Billboard { .. } => {}
            AdChannel::Press { outlet }
            | AdChannel::Radio { outlet }
            | AdChannel::Tv { outlet } => {
                kanal_medialny(world, &spis, c, outlet, t, raport);
            }
            AdChannel::Leaflet { origin, radius_m } => {
                ulotki(world, &spis, c, origin, radius_m, t, raport);
            }
            AdChannel::InStorePromo { site } => {
                if let Some(klucz) = magnat_agents::knowledge_key(magnat_core::PlaceRef::Site(site))
                {
                    plan_promocji.push((klucz, c.id, c.brand, c.claim));
                }
            }
            AdChannel::Sponsorship { event } => {
                sponsoring(world, &spis, c, event, t, raport);
            }
            AdChannel::Pr => plan_pr.push(c.brand),
        }
    }
    promocje(world, &spis, &mut plan_promocji, t, raport);
    plan_pr.sort_unstable();
    plan_pr.dedup();
    public_relations(world, &spis, &plan_pr, t);

    // Raz na miesiąc: firmy AI decydują, czy kupić kampanię (§5.2). Bez tego cały
    // mechanizm czekałby na pierwszy panel gracza, czyli na M10f.
    if magnat_agents::is_month_start(t.0 / 1_440) {
        let _ = crate::ai::monthly(world, t);
    }

    redakcja(world, &spis, t, raport);
    rozejdz(world, &spis, doba as u32);
    rozlicz(world, t, raport);

    world.resource_mut::<Outlets>().retire((t.0 / 1_440) as u32);
    let zamkniete = world.resource_mut::<Campaigns>().close_finished(now);
    let _ = zamkniete;
}

/// Prasa, radio, telewizja: odbiorcy z czytelnictwa dzielnic.
fn kanal_medialny(
    world: &mut World,
    spis: &DistrictRoster,
    c: &AdCampaign,
    outlet: magnat_core::SiteId,
    t: Tick,
    raport: &mut MediaReport,
) {
    let Some(o) = world.resource::<Outlets>().get(outlet).cloned() else {
        return;
    };
    // Tytuł bez wolnego slotu nie emituje — powierzchnia reklamowa jest skończona
    // i to ona jest towarem, którym media handlują (§5.3).
    if o.inventory_left == 0 {
        return;
    }
    if let Some(m) = world.resource_mut::<Outlets>().get_mut(outlet) {
        m.inventory_left = m.inventory_left.saturating_sub(1);
    }
    let kanal = c.channel.kind();
    let doba = t.0 / 1_440;
    let seed = world.seed;
    for d in spis.districts().collect::<Vec<_>>() {
        let p = o.readership_permille(d);
        if p == 0 {
            continue;
        }
        let ludzie = spis.in_district(d);
        let ilu = (ludzie.len() as u64 * u64::from(p) / 1_000) as u32;
        for e in sample_stride(
            ludzie,
            ilu,
            seed,
            StreamId::AdMediaPick,
            c.id.0 ^ (u32::from(d.0) << 16),
            t,
        ) {
            let pierwszy = expose(world, e, c.brand, kanal, c.claim, doba);
            zapisz_ekspozycje(world, c.id, e, pierwszy, raport);
        }
    }
}

/// Skąd wychodzi roznoszący — trzy próby, bo pierwsza bywa pusta (`GF-2`).
///
/// **Katalog miejsc powstaje raz, przy zaludnianiu miasta**, z zakładów generatora
/// (`sim/world::population::katalog_miejsc`). Zakład założony **w trakcie gry** —
/// przez firmę AI albo przez gracza — nigdy do niego nie trafia, więc
/// `coord_of(Site)` odpowiada dla niego `None`. Do `M10g` kampania ulotkowa takiego
/// zakładu kończyła się w tym miejscu i nie dostarczała ani jednej ekspozycji;
/// nie było tego widać, bo raport pokazywał sumę po wszystkich kanałach.
///
/// Trzy próby, od najdokładniejszej: sam zakład, jego budynek (kamienica ze sklepem
/// na parterze **jest** w katalogu, bo ma mieszkania), a na końcu środek ciężkości
/// domów dzielnicy. Ostatnia zawsze coś daje w dzielnicy, w której ktoś mieszka,
/// i jest uczciwa: roznoszący zaczyna od środka osiedla.
///
/// `ponytail:` prawdziwy sufit jest gdzie indziej i jest nazwany — **katalog miejsc
/// nie odbudowuje się po założeniu zakładu**. Dotyczy to każdego pytania o położenie
/// nowego zakładu, nie tylko ulotek. Droga wyjścia: przebudowa `PlaceTable`
/// w `sim/world` przy zmianie rejestru zakładów, razem z indeksem, który z niej
/// korzysta — to jest własna naprawa z własnym przebiegiem, nie dopisek do reklamy.
fn punkt_nadania(
    miejsca: &magnat_agents::PlaceTable,
    spis: &DistrictRoster,
    world: &World,
    origin: magnat_core::SiteId,
    budynek: magnat_core::BuildingId,
    dzielnica: DistrictId,
) -> Option<magnat_core::WorldCoord> {
    if let Some(at) = miejsca.coord_of(magnat_core::PlaceRef::Site(origin)) {
        return Some(at);
    }
    if let Some(at) = miejsca.coord_of(magnat_core::PlaceRef::Building(budynek)) {
        return Some(at);
    }
    let (mut sx, mut sy, mut sz, mut n) = (0i64, 0i64, 0i64, 0i64);
    for e in spis.in_district(dzielnica) {
        let Some(at) = world
            .get::<magnat_agents::Residence>(*e)
            .and_then(magnat_agents::home_of)
            .and_then(|dom| miejsca.coord_of(dom))
        else {
            continue;
        };
        sx += i64::from(at.x);
        sy += i64::from(at.y);
        sz += i64::from(at.z);
        n += 1;
    }
    (n > 0).then(|| magnat_core::WorldCoord::new((sx / n) as i32, (sy / n) as i32, (sz / n) as i32))
}

/// Ulotki: mieszkańcy, których dom leży w promieniu od punktu nadania.
///
/// Bez nowego indeksu (kryterium WP10.6): przechodzimy po dzielnicach objętych
/// promieniem i mierzymy odległość domu od nadania współrzędnymi z katalogu miejsc.
fn ulotki(
    world: &mut World,
    spis: &DistrictRoster,
    c: &AdCampaign,
    origin: magnat_core::SiteId,
    radius_m: u16,
    t: Tick,
    raport: &mut MediaReport,
) {
    let hit = world
        .resource::<magnat_agents::BrandData>()
        .channels
        .leaflet_hit_bps;
    let doba = t.0 / 1_440;
    let seed = world.seed;
    let promien_cm = i64::from(radius_m) * 100;
    let kwadrat = promien_cm * promien_cm;

    // Katalog miejsc **raz na kampanię**, nie raz na mieszkańca: `resource::<T>()`
    // w filtrze kosztowałby tyle wyszukań, ilu jest ludzi w mieście, razy liczba
    // kampanii ulotkowych, razy doba.
    let katalog = world.resource::<magnat_agents::PlaceCatalog>().clone();
    let Some(miejsca) = katalog.get() else {
        return;
    };
    // **Ulotka nie przekracza granicy dzielnicy** i to jest jawny sufit, nie model:
    // roznoszący chodzi po okolicy zakładu, a okolica to jego dzielnica. Cena
    // alternatywy jest zmierzona — przejście po całym mieście na każdą kampanię
    // ulotkową i na każdą dobę to koszt rosnący z liczbą kampanii razy liczba
    // mieszkańców, przy budżecie 1,5 % ticku dla dwóch tysięcy kampanii (§7.5).
    //
    // `ponytail:` droga wyjścia, gdyby promień miał naprawdę przecinać dzielnice:
    // sąsiedztwo dzielnic z `CityData`, którego dziś `magnat_media` nie widzi.
    let Some((moja, budynek)) = world
        .get_resource::<magnat_firms::Firms>()
        .and_then(|f| f.site(origin))
        .map(|s| (s.district, s.building))
    else {
        return;
    };
    let Some(srodek) = punkt_nadania(miejsca, spis, world, origin, budynek, moja) else {
        return;
    };
    let kandydaci: Vec<magnat_ecs::Entity> = spis
        .in_district(moja)
        .iter()
        .copied()
        .filter(|e| {
            let Some(r) = world.get::<magnat_agents::Residence>(*e) else {
                return false;
            };
            let Some(dom) = magnat_agents::home_of(r) else {
                return false;
            };
            let Some(at) = miejsca.coord_of(dom) else {
                return false;
            };
            let dx = i64::from(at.x) - i64::from(srodek.x);
            let dy = i64::from(at.y) - i64::from(srodek.y);
            dx * dx + dy * dy <= kwadrat
        })
        .collect();

    let ilu = (kandydaci.len() as u64 * u64::from(hit) / 10_000) as u32;
    for e in sample_stride(&kandydaci, ilu, seed, StreamId::AdLeaflet, c.id.0, t) {
        let pierwszy = expose(world, e, c.brand, AdChannelKind::Leaflet, c.claim, doba);
        zapisz_ekspozycje(world, c.id, e, pierwszy, raport);
    }
}

/// Jedna kampania promocyjna w planie doby: klucz miejsca, numer, marka, obietnica.
type Promocja = (u32, magnat_core::CampaignId, magnat_core::BrandId, Q);

/// Wszyscy mieszkańcy miasta w kolejności dzielnic — jedna lista na dobę.
fn mieszkancy(spis: &DistrictRoster) -> Vec<magnat_ecs::Entity> {
    let dzielnice: Vec<DistrictId> = spis.districts().collect();
    dzielnice
        .iter()
        .flat_map(|d| spis.in_district(*d).iter().copied())
        .collect()
}

/// Promocje w sklepach: docierają do tych, którzy już tam byli.
///
/// Darmowe w sensie zasięgu — mieszkaniec i tak tam jest — i dlatego najtańsze
/// w tabeli CPM. „Byłem tam" czytamy z magazynu wiedzy M3, a nie z drugiego licznika.
///
/// **Jeden przebieg po mieście dla wszystkich kampanii** (`GF-1`): plan jest
/// posortowany po kluczu miejsca, a każdy mieszkaniec przechodzi swój magazyn wiedzy
/// raz i sam sprawdza, w którą kampanię trafia. Poprzednia postać pytała
/// `knows_place` raz na kampanię i raz na mieszkańca, czyli przechodziła ten sam
/// magazyn tyle razy, ile w mieście stało promocji.
///
/// Jedna kampania na miejsce: `ai::monthly` pomija zakład z żywą kampanią, a komenda
/// gracza sprawdza to samo. Duplikat w planie znaczyłby dwie promocje w jednym sklepie
/// tego samego dnia, więc wygrywa pierwsza — i to jest ta sama reguła, nie wyjątek.
fn promocje(
    world: &mut World,
    spis: &DistrictRoster,
    plan: &mut Vec<Promocja>,
    t: Tick,
    raport: &mut MediaReport,
) {
    plan.sort_unstable_by_key(|p| p.0);
    plan.dedup_by_key(|p| p.0);
    if plan.is_empty() {
        return;
    }
    let doba = t.0 / 1_440;
    // Trafienia najpierw, ekspozycje potem: `expose` bierze `&mut World`, a przejście
    // po magazynie wiedzy pożycza go niemutowalnie.
    let mut trafienia: Vec<(magnat_ecs::Entity, usize)> = Vec::new();
    for e in mieszkancy(spis) {
        magnat_agents::for_each_known_place(world, e, |target| {
            if let Ok(i) = plan.binary_search_by_key(&target, |p| p.0) {
                trafienia.push((e, i));
            }
        });
    }
    for (e, i) in trafienia {
        let (_, id, brand, claim) = plan[i];
        let pierwszy = expose(world, e, brand, AdChannelKind::InStorePromo, claim, doba);
        zapisz_ekspozycje(world, id, e, pierwszy, raport);
    }
}

/// Sponsoring: uczestnicy zdarzenia.
///
/// Zdarzenie nie ma listy uczestników — ma **zakres** (`ScopeInstance`), więc zasięg
/// sponsoringu jest zasięgiem zdarzenia: dzielnica, miasto albo dzielnica zakładu.
/// To jest cała informacja, którą M8 o zdarzeniu trzyma, i zmyślanie listy uczestników
/// byłoby dopisaniem danych, których nikt nie ma.
fn sponsoring(
    world: &mut World,
    spis: &DistrictRoster,
    c: &AdCampaign,
    event: magnat_core::EventId,
    t: Tick,
    raport: &mut MediaReport,
) {
    let Some(zakres) = world
        .get_resource::<magnat_events::Events>()
        .and_then(|ev| ev.active().iter().find(|e| e.id == event).map(|e| e.scope))
    else {
        return;
    };
    let dzielnice: Vec<DistrictId> = match zakres {
        magnat_events::ScopeInstance::District(d) => vec![DistrictId(d)],
        magnat_events::ScopeInstance::World => spis.districts().collect(),
        _ => return,
    };
    let doba = t.0 / 1_440;
    let seed = world.seed;
    for d in dzielnice {
        let ludzie = spis.in_district(d);
        // Impreza dociera do co dziesiątego mieszkańca dzielnicy — tyle, ile mieści
        // się na niej fizycznie. `ponytail:` sufit nazwany: pojemność imprezy jest
        // daną, której `sim/events` nie trzyma, więc jest tu jedną stałą.
        let ilu = (ludzie.len() / 10) as u32;
        // Dzielnica wchodzi do klucza tak samo jak w kanale medialnym — bez tego
        // wszystkie dzielnice startowały od tego samego punktu w liście.
        let klucz = c.id.0 ^ (u32::from(d.0) << 16) ^ 0x5350_4f4e;
        for e in sample_stride(ludzie, ilu, seed, StreamId::AdMediaPick, klucz, t) {
            let pierwszy = expose(world, e, c.brand, AdChannelKind::Sponsorship, c.claim, doba);
            zapisz_ekspozycje(world, c.id, e, pierwszy, raport);
        }
    }
}

/// PR: **nie tworzy ekspozycji** — bierze tych, którzy markę już lubią, i każe im
/// o niej opowiedzieć (§5.2).
///
/// Dlatego PR nie ma czym zadziałać na markę, której nikt nie zna: nie ma kogo
/// zacytować. To jest zamierzone i jest jedyną różnicą między PR a tanią reklamą.
/// **Jeden przebieg po mieście dla wszystkich kampanii PR naraz** (`GF-1`): marki są
/// posortowane, a każdy mieszkaniec czyta swoje szesnaście slotów raz i sam sprawdza,
/// o której z nich ma co powiedzieć. Poprzednia postać wołała `affinity_of` raz na
/// kampanię i raz na mieszkańca — przy 250 kampaniach PR i 20 tys. mieszkańców
/// kosztowało to 6479 ms na dobę gry wobec 225 ms budżetu na wszystkie kanały.
fn public_relations(
    world: &mut World,
    spis: &DistrictRoster,
    marki: &[magnat_core::BrandId],
    t: Tick,
) {
    if marki.is_empty() {
        return;
    }
    let doba = t.0 / 1_440;
    let seed = world.seed;
    // Nadawcy: ci, którzy markę znają i lubią.
    let mut nadawcy: Vec<(magnat_ecs::Entity, magnat_agents::BrandAffinity)> = Vec::new();
    for e in mieszkancy(spis) {
        for a in magnat_agents::slots_of(world, e, doba).as_slice() {
            if a.affinity > 0 && marki.binary_search(&a.brand).is_ok() {
                nadawcy.push((e, *a));
            }
        }
    }
    for (nadawca, opinia) in nadawcy {
        let sluchacze = magnat_agents::relations_of(world, nadawca);
        for (sluchacz, waga) in sluchacze {
            let mut r = rng(seed, StreamId::BrandRumor, nadawca.index(), t);
            if r.gen_range_u32(100) >= u32::from(waga) {
                continue;
            }
            let _ = magnat_agents::touch(
                world,
                sluchacz,
                opinia.brand,
                magnat_agents::Touch::Rumor {
                    from: opinia,
                    weight: waga,
                    credibility: 100,
                },
                doba,
            );
        }
    }
}

/// O ile najwyżej spada sympatia do marki po najgorszym możliwym tekście.
///
/// `ponytail:` stała w kodzie, nie w `data/tuning/brand.ron`. Sufit jest nazwany:
/// dopóki nie ma przebiegu balansatora mierzącego, ile marek rocznie obrywa od
/// prasy, liczba w danych byłaby parametrem bez pytania, na które odpowiada.
/// Ścieżka wyjścia: pole `scandal_drop` obok `k_up`/`k_down` w sekcji `memory`.
/// Dla porównania zmowa cenowa zabiera 20 punktów (`data/tuning/relations.ron`)
/// i ma to być cios mocniejszy niż jeden zły artykuł.
const SCANDAL_MAX_DROP: u8 = 12;

/// Redakcja: co dobę każdy tytuł wybiera teksty ze strumienia zdarzeń M8.
fn redakcja(world: &mut World, spis: &DistrictRoster, t: Tick, raport: &mut MediaReport) {
    // **Nowina jest nowa.** Bez tego warunku redakcja opisywałaby ten sam strajk
    // od nowa co cztery doby: tekst schodzi z obiegu po `STORY_SPREAD_DAYS`, więc
    // deduplikacja po żywych tekstach przestawała go widzieć, a wpis w kronice
    // zdarzeń zostawał. Okno jest dobowe — to samo, którym kronika gracza zbiera
    // dzienniki `sim/*`.
    let okno = t.0.saturating_sub(1_440);
    let Some(kandydaci) = world.get_resource::<magnat_events::Events>().map(|ev| {
        ev.chronicle()
            .iter()
            .filter(|c| c.opened && c.at.0 >= okno)
            .map(|c| (c.event, c.category, c.severity_bps, c.scope))
            .collect::<Vec<_>>()
    }) else {
        return;
    };
    if kandydaci.is_empty() {
        return;
    }
    let dane = world.resource::<magnat_agents::BrandData>().outlets.clone();
    let doba = (t.0 / 1_440) as u32;
    let seed = world.seed;
    let tytuly: Vec<(magnat_core::SiteId, EditorialBias, Q, u8)> = world
        .resource::<Outlets>()
        .iter()
        .map(|(s, o)| (*s, o.bias, o.credibility, o.kind.as_index() as u8))
        .collect();

    for (site, bias, wiarygodnosc, _) in tytuly {
        // Wartość informacyjna: skala zdarzenia przez wagę linii redakcyjnej.
        let mut ranking: Vec<(
            u32,
            magnat_core::EventId,
            magnat_core::EventCategory,
            u16,
            magnat_events::ScopeInstance,
        )> = kandydaci
            .iter()
            .map(|(id, cat, sev, scope)| {
                (dane.news_value(bias, *cat, *sev), *id, *cat, *sev, *scope)
            })
            .collect();
        ranking.sort_unstable_by_key(|(w, id, ..)| (std::cmp::Reverse(*w), id.0));
        let mut r = rng(seed, StreamId::MediaEditorial, site.entity().index(), t);
        let ile = u32::from(dane.stories_min)
            + r.gen_range_u32(u32::from(dane.stories_max.saturating_sub(dane.stories_min)) + 1);
        for (_, event, category, severity, scope) in ranking.into_iter().take(ile as usize) {
            if world
                .resource::<Outlets>()
                .stories()
                .iter()
                .any(|s| s.outlet == site && s.event == event)
            {
                continue;
            }
            let mut story = Story {
                outlet: site,
                event,
                category,
                bias,
                published_day: doba,
                known: Vec::new(),
            };
            // Publikacja: czytelnicy dowiadują się od razu.
            let czytelnictwo: Vec<(DistrictId, u16)> = world
                .resource::<Outlets>()
                .get(site)
                .map(|o| o.readership.clone())
                .unwrap_or_default();
            for (d, p) in czytelnictwo {
                let n = spis.population_of(d) as u64 * u64::from(p) / 1_000;
                story.add_known(d, n as u32);
            }
            let zasieg = story.reach_permille(spis.population());

            // **Tekst uderza w markę tego, o kim jest** (M10e, `FE-3`). Do M10d
            // publikacja budowała wyłącznie znajomość tytułu, więc obietnica
            // z §1 dokumentu fazy — „gazeta pisze o strajku, marka gracza traci
            // afinitet" — nie miała ostatniego ogniwa. Skandal dociera **tylko
            // do tych, którzy markę znają**: kto o firmie nie słyszał, ten po
            // przeczytaniu o cudzym strajku nadal o niej nie słyszał.
            let bohater = podmiot_tekstu(world, scope);
            // Zła nowina dotyczy zdarzeń społecznych i firmowych; pogoda i awaria
            // sieci nie są niczyją winą i marki nie dotykają.
            let uderzenie = matches!(
                category,
                magnat_core::EventCategory::Social | magnat_core::EventCategory::Firm
            )
            .then(|| {
                u8::try_from(u32::from(SCANDAL_MAX_DROP) * u32::from(severity) / 10_000)
                    .unwrap_or(SCANDAL_MAX_DROP)
            })
            .filter(|d| *d > 0);

            // Tytuł, który pisze, jest przy okazji poznawany — wiarygodność jest
            // per para i mieszka w tym samym slocie co marka (§5.3).
            let marka = world
                .resource::<magnat_economy::Market>()
                .firm_of(site)
                .and_then(magnat_supply::brand_of);
            if let Some(marka) = marka {
                let d0: Vec<DistrictId> = spis.districts().collect();
                for d in d0 {
                    let p = world
                        .resource::<Outlets>()
                        .get(site)
                        .map(|o| o.readership_permille(d))
                        .unwrap_or(0);
                    if p == 0 {
                        continue;
                    }
                    let ludzie = spis.in_district(d);
                    let ilu = (ludzie.len() as u64 * u64::from(p) / 1_000) as u32;
                    for e in sample_stride(
                        ludzie,
                        ilu,
                        seed,
                        StreamId::MediaEditorial,
                        event.0 ^ (u32::from(d.0) << 16),
                        t,
                    ) {
                        expose_media(world, e, marka, Q::new(60), wiarygodnosc, u64::from(doba));
                        if let (Some(b), Some(drop)) = (bohater, uderzenie) {
                            magnat_agents::touch(
                                world,
                                e,
                                b,
                                magnat_agents::brand::Touch::Scandal { drop },
                                u64::from(doba),
                            );
                        }
                    }
                }
            }

            if bohater.is_some() && uderzenie.is_some() {
                world.resource_mut::<Outlets>().note_scandal();
            }
            world.resource_mut::<Outlets>().publish(
                story,
                t,
                DecisionReason::StoryPublished {
                    outlet: marka.unwrap_or(magnat_core::BrandId(0)),
                    event,
                    bias,
                    reach_bp: zasieg * 10,
                },
            );
            raport.stories_published = raport.stories_published.saturating_add(1);
        }
    }
}

/// O czyjej marce jest ten tekst. `None` znaczy „o nikim konkretnym" — pogoda,
/// protest miejski, awaria sieci.
fn podmiot_tekstu(
    world: &World,
    scope: magnat_events::ScopeInstance,
) -> Option<magnat_core::BrandId> {
    let firma = match scope {
        magnat_events::ScopeInstance::Site(s) => {
            world.get_resource::<magnat_economy::Market>()?.firm_of(s)?
        }
        magnat_events::ScopeInstance::Firm(k) => magnat_firms::firm_id(k),
        _ => return None,
    };
    magnat_supply::brand_of(firma)
}

/// Tekst rozchodzi się plotką przez [`STORY_SPREAD_DAYS`] dób po publikacji.
///
/// Przyrost logistyczny na agregacie dzielnicy, w liczbach całkowitych: ilu jeszcze
/// nie wie, razy ilu już wie, przez populację. Bez tego zasięg tekstu równałby się
/// czytelnictwu co do promila, a kryterium WP10.7 („po trzech dobach powyżej 40 %
/// przy czytelnictwie 35 %") nie miałoby jak się spełnić.
fn rozejdz(world: &mut World, spis: &DistrictRoster, doba: u32) {
    let dzielnice: Vec<(DistrictId, u32)> = spis
        .districts()
        .map(|d| (d, spis.population_of(d)))
        .collect();
    let teksty = world.resource_mut::<Outlets>().stories_mut();
    for s in teksty.iter_mut() {
        if doba.saturating_sub(s.published_day) > STORY_SPREAD_DAYS {
            continue;
        }
        for (d, n) in &dzielnice {
            if *n == 0 {
                continue;
            }
            let wie = s.known_in(*d);
            if wie == 0 {
                continue;
            }
            let nie_wie = n.saturating_sub(wie);
            // Współczynnik 1/8 na dobę: trzy doby podnoszą 35 % do ~41 %, a 8 % do ~11 %.
            // Dobrany tak, żeby zmieścić się w obu granicach kryterium WP10.7 naraz —
            // powyżej 40 % w dzielnicy czytającej i poniżej 15 % w tej, która nie czyta.
            let przyrost = (u64::from(wie) * u64::from(nie_wie) / u64::from(*n) / 8) as u32;
            s.add_known(*d, przyrost.min(nie_wie));
        }
    }
}

/// Rozliczenie doby: koszt ekspozycji, przelew i zapis w księdze.
///
/// Pieniądz rusza się **przez księgi** (`Books::transfer`), a wynik siada w księdze
/// zakładu przez `ledger_post` — kryterium WP10.6 mówi o tym wprost. Kampania, na którą
/// nie starcza budżetu, dostarcza mniej ekspozycji, a nie ujemne saldo.
fn rozlicz(world: &mut World, t: Tick, raport: &mut MediaReport) {
    let cennik = world
        .resource::<magnat_agents::BrandData>()
        .channels
        .clone();
    let rozliczenia: Vec<DoRozliczenia> = world
        .resource::<Campaigns>()
        .iter()
        .filter(|(_, c)| c.metrics.exposures_today > 0)
        .map(|(id, c)| {
            let kanal = c.channel.kind();
            let koszt =
                Money(cennik.cpm(kanal).get() * i64::from(c.metrics.exposures_today) / PER_MILLE)
                    .get()
                    .min(c.remaining().get());
            (
                *id,
                c.site,
                Money(koszt.max(0)),
                kanal,
                c.channel.payee(),
                c.brand,
                c.claim,
            )
        })
        .collect();

    let Some(market) = world.get_resource::<magnat_economy::Market>().cloned() else {
        return;
    };
    for (id, site, koszt, kanal, payee, brand, claim) in rozliczenia {
        // Kampania zakładu bez konta nie ma czym zapłacić — **i dlatego się kończy**,
        // zamiast emitować za darmo. Darmowa emisja nie tworzy pieniądza, ale tworzy
        // zasięg bez kosztu, czyli dokładnie to, czego balansator nie ma jak zobaczyć.
        if market.account_of(site).is_none() {
            if let Some(c) = world.resource_mut::<Campaigns>().get_mut(id) {
                c.spent = c.budget;
                c.metrics.exposures_today = 0;
            }
            continue;
        }
        if koszt.get() > 0 {
            let z = market.account_of(site);
            let do_kogo = payee
                .and_then(|p| market.account_of(p))
                .unwrap_or_else(|| market.rest_of_world());
            let powod = DecisionReason::AdCampaignStarted {
                brand,
                channel: kanal,
                budget: koszt,
                claim,
            };
            // **Księga przed przelewem.** Gdyby szło odwrotnie, odrzucony zapis
            // zostawiłby pieniądz przesunięty i koszt niezaksięgowany — a to jest
            // dokładnie ten rozjazd, którego `check_conservation` nie widzi, bo
            // suma sald się zgadza. Zapis w księdze niczego nie przesuwa, więc
            // jego odmowa jest tania: nie płacimy i zamykamy kampanię.
            let zaksiegowano = market.post_marketing_expense(site, koszt, powod);
            let mut ok = false;
            if zaksiegowano {
                if let (Some(z), Some(books)) =
                    (z, world.get_resource_mut::<magnat_economy::Books>())
                {
                    let memo = magnat_economy::TxMemo::new(
                        magnat_economy::TxKind::AdSpend {
                            campaign: id,
                            channel: kanal,
                        },
                        powod,
                    );
                    ok = books.transfer(z, do_kogo, koszt, memo, t).is_ok();
                }
            }
            if ok {
                raport.spent = Money(raport.spent.get() + koszt.get());
                if let Some(c) = world.resource_mut::<Campaigns>().get_mut(id) {
                    c.spent = Money(c.spent.get() + koszt.get());
                    c.metrics.exposures_today = 0;
                }
                continue;
            }
            // Nie zapłacił — **kampania się kończy**, zamiast emitować dalej za darmo.
            // Darmowa emisja nie tworzy pieniądza, ale tworzy zasięg bez kosztu,
            // czyli dokładnie to, czego balansator nie ma jak zobaczyć.
            if let Some(c) = world.resource_mut::<Campaigns>().get_mut(id) {
                c.spent = c.budget;
                c.metrics.exposures_today = 0;
            }
            continue;
        }
        if let Some(c) = world.resource_mut::<Campaigns>().get_mut(id) {
            c.metrics.exposures_today = 0;
        }
    }

    // Powierzchnia reklamowa odnawia się co dobę.
    let tytuly: Vec<magnat_core::SiteId> = world
        .resource::<Outlets>()
        .iter()
        .map(|(s, _)| *s)
        .collect();
    for s in tytuly {
        if let Some(o) = world.resource_mut::<Outlets>().get_mut(s) {
            o.inventory_left = o.inventory_daily;
        }
    }
}
