//! Kryteria WP10.6 i WP10.7 fazy M10 — zasięg kanałów i rozejście się tekstu.
//!
//! Świat jest tu **stawiany ręcznie**, a nie generatorem: kryteria mówią o liczbach
//! („8000 przejazdów × 12 % → 960 ± 30"), a liczby da się sprawdzić tylko wtedy, gdy
//! wejście jest znane co do sztuki. Przebieg w mieście z generatora jest osobnym
//! zadaniem i należy do balansatora (M10f).

use magnat_agents::{
    register, AgentState, BrandsRef, Employment, Identity, KnowledgeRef, Lifecycle, NeedTable,
    Needs, Personality, PlanRef, RelationsRef, Residence, SkillSlot, Skills, Vitals, Wealth,
};
use magnat_core::{BrandId, CampaignId, DistrictId, Money, SimMinute, SiteId, Tick, Q};
use magnat_ecs::{Entity, World};
use magnat_media::{AdCampaign, AdChannel, CampaignMetrics, Campaigns, Outlets};
use magnat_nav::EdgeId;

const SEED: u64 = 4242;

/// Miasto bez gospodarki: sami mieszkańcy z pamięcią marek i przypisaną dzielnicą.
fn miasto(n: u32, dzielnic: u16) -> World {
    let mut w = World::new(SEED);
    register(
        &mut w,
        NeedTable::load_default().expect("data/needs/needs.ron"),
    );
    magnat_agents::register_society(
        &mut w,
        magnat_agents::DemographyTable::load_default().expect("data/demography"),
    );
    for i in 0..n {
        let e = w
            .spawn()
            .with(Identity {
                birth_day: -(360 * 30),
                flags: Identity::FLAG_ALIVE,
                household: i,
                ..Identity::default()
            })
            .with(Personality([50; 8]))
            .with(Vitals {
                health: 90,
                energy: 80,
                ..Vitals::default()
            })
            .with(Needs::default())
            .with(Skills([SkillSlot::default(); 4]))
            .with(Wealth::default())
            .with(Employment::default())
            .with(Residence {
                building: i,
                unit: 0,
                district: (i % u32::from(dzielnic)) as u16,
            })
            .with(PlanRef::default())
            .with(AgentState::default())
            .with(KnowledgeRef::default())
            .with(RelationsRef::default())
            .with(BrandsRef::default())
            .with(Lifecycle::default())
            .id();
        w.resource_mut::<magnat_agents::Population>().add_citizen(e);
    }
    magnat_media::register_media(&mut w);
    w
}

fn kampania_billboard(w: &mut World, edge: u32, rate: u16) -> CampaignId {
    w.resource_mut::<Campaigns>().open(|id| AdCampaign {
        id,
        site: SiteId(Entity::new(1, std::num::NonZeroU32::MIN)),
        brand: BrandId(7),
        channel: AdChannel::Billboard {
            edge: EdgeId(edge),
            notice_rate_bps: rate,
        },
        claim: Q::new(80),
        budget: Money(10_000_000),
        spent: Money::ZERO,
        window: (SimMinute(0), SimMinute(100 * 1_440)),
        metrics: CampaignMetrics::default(),
    })
}

/// Podaje sieci ruchu `n` przejazdów po krawędzi — to samo, co robi `dispatch`
/// przy wpuszczaniu podróży, tylko bez stawiania grafu drogowego.
fn przejazdy(w: &mut World, edge: u32, kto: &[u32]) {
    let net = w
        .get_resource_mut::<magnat_traffic::TrafficNetwork>()
        .expect("sieć ruchu");
    for k in kto {
        net.watch_mut().note(*k, &[EdgeId(edge)]);
    }
}

#[test]
fn billboard_dostarcza_zapowiedziana_liczbe_ekspozycji() {
    // Kryterium WP10.6: krawędź o dobowym potoku 8000 przejazdów i `notice_rate = 12 %`
    // daje 960 ± 30 ekspozycji, powtarzalnie dla ziarna.
    let mut w = miasto(8_000, 4);
    w.insert_resource(magnat_traffic::TrafficNetwork::default());
    let id = kampania_billboard(&mut w, 11, 1_200);

    // Pierwszy przebieg ustawia podsłuch krawędzi; przejazdy podajemy po nim.
    let _ = magnat_media::step(&mut w, Tick(60));
    let kto: Vec<u32> = (0..8_000).collect();
    przejazdy(&mut w, 11, &kto);
    let r = magnat_media::step(&mut w, Tick(120));

    assert_eq!(r.billboard_passes, 8_000);
    println!("billboard: {} ekspozycji z 8000 przejazdów", r.exposures);
    assert!(
        (930..=990).contains(&r.exposures),
        "billboard dostarczył {} ekspozycji, oczekiwane 960 ± 30",
        r.exposures
    );
    assert_eq!(
        w.resource::<Campaigns>()
            .get(id)
            .expect("kampania")
            .metrics
            .first_contacts,
        u64::from(r.exposures),
        "każda pierwsza ekspozycja jest pierwszym kontaktem z tą marką"
    );
}

#[test]
fn ten_sam_seed_daje_te_same_ekspozycje() {
    let mut a = miasto(8_000, 4);
    a.insert_resource(magnat_traffic::TrafficNetwork::default());
    kampania_billboard(&mut a, 11, 1_200);
    let _ = magnat_media::step(&mut a, Tick(60));
    przejazdy(&mut a, 11, &(0..8_000).collect::<Vec<_>>());
    let ra = magnat_media::step(&mut a, Tick(120));

    let mut b = miasto(8_000, 4);
    b.insert_resource(magnat_traffic::TrafficNetwork::default());
    kampania_billboard(&mut b, 11, 1_200);
    let _ = magnat_media::step(&mut b, Tick(60));
    przejazdy(&mut b, 11, &(0..8_000).collect::<Vec<_>>());
    let rb = magnat_media::step(&mut b, Tick(120));

    assert_eq!(ra.exposures, rb.exposures);
    // Liczba ekspozycji to za mało: dwa przebiegi mogą trafić w tylu samo **innych**
    // ludzi. Porównujemy hash slabu marek, czyli cały stan pamięci miasta.
    assert_eq!(hash_marek(&a), hash_marek(&b));
}

/// Hash pamięci marek całego miasta — wejście kryterium determinizmu (00 §3.6).
fn hash_marek(w: &World) -> magnat_core::StateHash {
    let mut h = magnat_core::StateHasher::new();
    magnat_core::HashState::hash_state(w.resource::<magnat_agents::BrandSlab>(), &mut h);
    h.finish()
}

#[test]
fn ekspozycje_trafiaja_do_tych_ktorzy_tamtedy_jezdza() {
    // Kryterium WP10.6: przeniesienie billboardu na krawędź w innej dzielnicy zmienia
    // rozkład dzielnic odbiorców. Mieszkańcy o parzystym indeksie mieszkają w dzielnicy
    // 0, nieparzystym w 1 — i tylko jedni z nich jeżdżą daną krawędzią.
    let mut w = miasto(8_000, 2);
    w.insert_resource(magnat_traffic::TrafficNetwork::default());
    kampania_billboard(&mut w, 11, 5_000);
    let _ = magnat_media::step(&mut w, Tick(60));

    let parzysci: Vec<u32> = (0..8_000).filter(|i| i % 2 == 0).collect();
    przejazdy(&mut w, 11, &parzysci);
    let _ = magnat_media::step(&mut w, Tick(120));

    let rozklad = rozklad_dzielnic(&w, BrandId(7));
    assert!(
        rozklad[0] > 0,
        "nikt z dzielnicy jeżdżącej tą krawędzią nie zobaczył tablicy"
    );
    assert_eq!(
        rozklad[1], 0,
        "tablicę zobaczył ktoś, kto tamtędy nie jechał — zasięg nie jest zasięgiem"
    );
}

/// Ilu mieszkańców każdej dzielnicy zna tę markę.
fn rozklad_dzielnic(w: &World, brand: BrandId) -> Vec<u32> {
    let mut out = vec![0u32; 8];
    let spis = w
        .resource::<magnat_agents::Population>()
        .citizens()
        .to_vec();
    for e in spis {
        let Some(r) = w.get::<Residence>(e) else {
            continue;
        };
        let d = r.district as usize;
        if magnat_agents::slots_of(w, e, 0)
            .as_slice()
            .iter()
            .any(|s| s.brand == brand)
        {
            out[d.min(7)] += 1;
        }
    }
    out
}

#[test]
fn tekst_rozchodzi_sie_zgodnie_z_czytelnictwem() {
    // Kryterium WP10.7: tytuł o czytelnictwie 35 % w jednej dzielnicy i 8 % w drugiej
    // publikuje tekst; po trzech dobach znajomość zdarzenia przekracza 40 % tam
    // i zostaje poniżej 15 % tu, a rozkład trzyma się czytelnictwa ± 5 pkt.
    let mut w = miasto(4_000, 2);
    let site = SiteId(Entity::new(2, std::num::NonZeroU32::MIN));
    w.resource_mut::<Outlets>()
        .insert(magnat_media::MediaOutlet {
            site,
            kind: magnat_core::MediaKind::Newspaper,
            readership: vec![(DistrictId(0), 350), (DistrictId(1), 80)],
            credibility: Q::new(70),
            bias: magnat_core::EditorialBias::Local,
            inventory_daily: 12,
            inventory_left: 12,
            slot_price: Money(240_000),
        });

    let mut story = magnat_media::Story {
        outlet: site,
        event: magnat_core::EventId(1),
        category: magnat_core::EventCategory::Social,
        bias: magnat_core::EditorialBias::Local,
        published_day: 0,
        known: Vec::new(),
    };
    // Publikacja: czytelnicy dowiadują się od razu, reszta dzielnicy z plotki.
    for (d, p) in [(DistrictId(0), 350u16), (DistrictId(1), 80u16)] {
        let ludzie = 2_000u64;
        story.add_known(d, (ludzie * u64::from(p) / 1_000) as u32);
    }
    w.resource_mut::<Outlets>().publish(
        story,
        Tick(0),
        magnat_core::DecisionReason::StoryPublished {
            outlet: BrandId(9),
            event: magnat_core::EventId(1),
            bias: magnat_core::EditorialBias::Local,
            reach_bp: 2_150,
        },
    );

    for doba in 1..=magnat_media::STORY_SPREAD_DAYS {
        let _ = magnat_media::step(&mut w, Tick(u64::from(doba) * 1_440));
    }

    let s = &w.resource::<Outlets>().stories()[0];
    let a = f64::from(s.known_in(DistrictId(0))) / 2_000.0 * 100.0;
    let b = f64::from(s.known_in(DistrictId(1))) / 2_000.0 * 100.0;
    println!("znajomość zdarzenia: dzielnica 0 = {a:.1} %, dzielnica 1 = {b:.1} %");
    assert!(a > 40.0, "dzielnica o czytelnictwie 35 % wie w {a:.1} %");
    assert!(b < 15.0, "dzielnica o czytelnictwie 8 % wie w {b:.1} %");
    // „Rozkład zgodny z czytelnictwem ± 5 pkt" da się utrzymać **tylko dla dzielnicy,
    // która nie czyta**: pierwsza część kryterium żąda przekroczenia 40 % przy
    // czytelnictwie 35 %, czyli sama wymusza odchylenie większe niż 5 punktów.
    // Dla dzielnicy czytającej sprawdzamy więc, że plotka nie ucieka: między
    // czytelnictwem a sufitem kryterium.
    assert!(
        (b - 8.0).abs() <= 5.0,
        "dzielnica bez czytelnictwa odjechała od swoich 8 %: {b:.1} %"
    );
    assert!(
        (35.0..=48.0).contains(&a),
        "plotka uciekła poza pasmo między czytelnictwem a sufitem: {a:.1} %"
    );
}

// ── FF-8: koszt systemów reklamowych przy 2000 kampanii ─────────────────────────

/// Ile kampanii ma stać naraz — liczba wprost z kryterium §7.5 dokumentu M10.
const KAMPANII: u32 = 2_000;

/// Budżet z §7.5: „systemy reklamowe ≤ 1,5 % budżetu ticku". Doba `m7miasto`
/// przy 28,5 tys. mieszkańców kosztuje ~15 s (zmierzone w M10b, `F-27`), więc
/// półtora procenta to **225 ms na dobę gry**.
const BUDZET_MS: u128 = 225;

/// Kampania na kanale wskazanym numerem — po jednej z każdych ośmiu.
fn kampania(w: &mut World, i: u32) -> CampaignId {
    let site = SiteId(Entity::new(1 + i % 16, std::num::NonZeroU32::MIN));
    let channel = match i % 8 {
        0 => AdChannel::Billboard {
            edge: EdgeId(i % 64),
            notice_rate_bps: 1_200,
        },
        1 => AdChannel::Press { outlet: site },
        2 => AdChannel::Radio { outlet: site },
        3 => AdChannel::Tv { outlet: site },
        4 => AdChannel::Leaflet {
            origin: site,
            radius_m: 400,
        },
        5 => AdChannel::InStorePromo { site },
        6 => AdChannel::Sponsorship {
            event: magnat_core::EventId(i),
        },
        _ => AdChannel::Pr,
    };
    w.resource_mut::<Campaigns>().open(|id| AdCampaign {
        id,
        site,
        brand: BrandId(1 + (i % 64) as u16),
        channel,
        claim: Q::new(80),
        budget: Money(10_000_000),
        spent: Money::ZERO,
        window: (SimMinute(0), SimMinute(100 * 1_440)),
        metrics: CampaignMetrics::default(),
    })
}

/// Jedna doba gry systemów mediów, w milisekundach.
fn doba_mediow_ms(w: &mut World, od_minuty: u64) -> u128 {
    let t0 = std::time::Instant::now();
    for m in 1..=1_440u64 {
        let _ = magnat_media::step(w, Tick(od_minuty + m));
    }
    t0.elapsed().as_millis()
}

/// `FF-8`: dwa tysiące kampanii mieszczą się w budżecie ticku.
///
/// # Dlaczego test, a nie przebieg miasta
///
/// Bo w przebiegu **nigdy nie powstanie dwa tysiące kampanii**: firmy AI otwierają
/// je raz na miesiąc i w mieście 4 km było ich trzydzieści pięć (M10b). Czekanie
/// na tę liczbę w naturalnym przebiegu to czekanie na coś, co nie zajdzie —
/// kampanie trzeba **postawić**, i to jest cała treść tego pomiaru.
///
/// Mierzymy **różnicę**, a nie czas bezwzględny: ten sam świat bez ani jednej
/// kampanii jest odniesieniem, więc wynik nie zawiera kosztu mieszkańców, pamięci
/// marek ani redakcji. To jest ta sama ablacja, którą M10b zrobiło na `m7miasto`,
/// tylko przy liczbie kampanii z kryterium, a nie z przypadku.
///
/// `#[ignore]`, bo mierzy zegarem: na obciążonej maszynie liczba skacze, a bramka,
/// która świeci na czerwono od cudzego kompilatora w tle, uczy ignorowania bramek.
#[test]
#[ignore = "pomiar czasu"]
fn dwa_tysiace_kampanii_miesci_sie_w_budzecie_ticku() {
    // Dwadzieścia tysięcy mieszkańców w ośmiu dzielnicach: tyle, żeby kanały
    // skalujące się z ludnością (ulotka, prasa) miały do kogo docierać.
    let mut bez = miasto(20_000, 8);
    bez.insert_resource(magnat_traffic::TrafficNetwork::default());
    let odniesienie = doba_mediow_ms(&mut bez, 0);

    let mut z = miasto(20_000, 8);
    z.insert_resource(magnat_traffic::TrafficNetwork::default());
    for i in 0..KAMPANII {
        kampania(&mut z, i);
    }
    // Pierwsza doba ustawia podsłuch krawędzi i zasiewa liczniki; mierzymy drugą,
    // bo to ona jest dobą typową.
    let _ = doba_mediow_ms(&mut z, 0);
    let z_kampaniami = doba_mediow_ms(&mut z, 1_440);

    let koszt = z_kampaniami.saturating_sub(odniesienie);
    println!(
        "media: doba bez kampanii {odniesienie} ms, z {KAMPANII} kampaniami \
         {z_kampaniami} ms → koszt kampanii {koszt} ms (budżet {BUDZET_MS} ms)"
    );
    assert!(
        koszt <= BUDZET_MS,
        "{KAMPANII} kampanii kosztuje {koszt} ms na dobę gry wobec budżetu \
         {BUDZET_MS} ms (1,5 % doby `m7miasto`, §7.5)"
    );
}

/// Rozbicie kosztu na kanały — odpowiedź na pytanie „który z ośmiu".
///
/// Sam czerwony pomiar mówi „za drogo" i nie mówi, gdzie szukać. Osiem osobnych
/// przebiegów po dwieście pięćdziesiąt kampanii **jednego** kanału mówi to,
/// czego potrzebuje adres naprawy: czy koszt rozkłada się równo, czy siedzi
/// w jednym miejscu. M10b nazwało kandydata (`F-27`: ulotka skalowała się
/// z liczbą mieszkańców razy liczba kampanii) — ten pomiar to sprawdza.
#[test]
#[ignore = "pomiar czasu"]
fn koszt_kampanii_w_rozbiciu_na_kanaly() {
    const NA_KANAL: u32 = 250;
    let nazwy = [
        "Billboard",
        "Press",
        "Radio",
        "Tv",
        "Leaflet",
        "InStorePromo",
        "Sponsorship",
        "Pr",
    ];
    let mut bez = miasto(20_000, 8);
    bez.insert_resource(magnat_traffic::TrafficNetwork::default());
    let odniesienie = doba_mediow_ms(&mut bez, 0);
    println!("odniesienie (0 kampanii): {odniesienie} ms/dobę");

    for (k, nazwa) in nazwy.iter().enumerate() {
        let mut w = miasto(20_000, 8);
        w.insert_resource(magnat_traffic::TrafficNetwork::default());
        for i in 0..NA_KANAL {
            // `i * 8 + k` trafia zawsze w ten sam kanał, a zmienia zakład i markę.
            kampania(&mut w, i * 8 + k as u32);
        }
        let _ = doba_mediow_ms(&mut w, 0);
        let z = doba_mediow_ms(&mut w, 1_440);
        let koszt = z.saturating_sub(odniesienie);
        println!(
            "{nazwa:<14} {NA_KANAL} kampanii → {koszt} ms/dobę \
             ({} µs na kampanię)",
            koszt * 1_000 / u128::from(NA_KANAL)
        );
    }
}
