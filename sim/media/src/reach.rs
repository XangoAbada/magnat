//! Dostarczanie ekspozycji: osiem kanałów, jeden zapis (M10b §5.2).
//!
//! **Ekspozycja zawsze kończy się w [`magnat_agents::touch`]** — jedno wejście do pamięci
//! marki, tak samo jak `learn_place` jest jedynym wejściem do wiedzy o miejscach (M3a).
//! Kanały różnią się wyłącznie tym, **kogo** wybierają; co się dzieje z wybranym,
//! wie `sim/agents` i nikt poza nim.

use magnat_agents::{Residence, Touch};
use magnat_core::{rng, AdChannelKind, BrandId, DistrictId, StreamId, Tick, Q};
use magnat_ecs::{Entity, World};

/// Mieszkańcy pogrupowani dzielnicami — budowany raz na dobę, nie raz na kampanię.
///
/// Bez tego każda kampania prasowa przechodziłaby po całej populacji, a przy dwóch
/// tysiącach kampanii (§7.5) byłoby to 800 mln odczytów na dobę. Z indeksem koszt
/// kampanii jest proporcjonalny do **liczby ekspozycji**, czyli do pracy, która
/// i tak musi się wykonać.
///
/// To nie jest indeks przestrzenny (kryterium WP10.6: „zero nowych indeksów
/// przestrzennych") — to pogrupowanie listy, którą `Population` już trzyma.
#[derive(Clone, Debug, Default)]
pub struct DistrictRoster {
    /// `(dzielnica, mieszkańcy)` posortowane po dzielnicy; w grupie po indeksie encji.
    groups: Vec<(DistrictId, Vec<Entity>)>,
}

impl DistrictRoster {
    /// Przechodzi po populacji raz i grupuje ją dzielnicami zamieszkania.
    #[must_use]
    pub fn build(world: &World) -> DistrictRoster {
        let spis = world.resource::<magnat_agents::Population>().citizens();
        let mut mapa: std::collections::BTreeMap<DistrictId, Vec<Entity>> =
            std::collections::BTreeMap::new();
        for e in spis {
            let Some(r) = world.get::<Residence>(*e) else {
                continue;
            };
            mapa.entry(DistrictId(r.district)).or_default().push(*e);
        }
        DistrictRoster {
            groups: mapa.into_iter().collect(),
        }
    }

    #[must_use]
    pub fn in_district(&self, d: DistrictId) -> &[Entity] {
        self.groups
            .binary_search_by_key(&d, |(k, _)| *k)
            .map(|i| self.groups[i].1.as_slice())
            .unwrap_or(&[])
    }

    pub fn districts(&self) -> impl Iterator<Item = DistrictId> + '_ {
        self.groups.iter().map(|(d, _)| *d)
    }

    #[must_use]
    pub fn population(&self) -> u32 {
        self.groups.iter().map(|(_, v)| v.len() as u32).sum()
    }

    #[must_use]
    pub fn population_of(&self, d: DistrictId) -> u32 {
        self.in_district(d).len() as u32
    }
}

/// Wybiera z listy `n` mieszkańców krokiem o losowym punkcie startu.
///
/// Krok zamiast losowania z powtórzeniami: przy 35 % czytelnictwa losowanie
/// z powtórzeniami dałoby tę samą osobę kilka razy i policzyłoby ją jako kilku
/// czytelników. Punkt startu jest z ziarna, więc dwie gazety o tym samym
/// czytelnictwie **nie trafiają w tych samych ludzi**.
pub fn sample_stride(
    spis: &[Entity],
    n: u32,
    seed: u64,
    stream: StreamId,
    key: u32,
    t: Tick,
) -> Vec<Entity> {
    if spis.is_empty() || n == 0 {
        return Vec::new();
    }
    let n = n.min(spis.len() as u32) as usize;
    let mut r = rng(seed, stream, key, t);
    let start = r.gen_range_u32(spis.len() as u32) as usize;
    let krok = spis.len() / n;
    let krok = krok.max(1);
    let mut out = Vec::with_capacity(n);
    let mut i = start;
    for _ in 0..n {
        out.push(spis[i % spis.len()]);
        i += krok;
    }
    // Sortowanie zostaje — to ono ustala kolejność zapisów do pamięci mieszkańców,
    // czyli kolejność, w której slab przydziela bloki (00 §3.2). `dedup` **nie** —
    // największe przesunięcie to `(n-1) * (len/n) < len`, więc powtórzeń nie ma
    // z konstrukcji i była to praca za każdym razem daremna.
    out.sort_unstable_by_key(|e| e.index());
    out
}

/// Jedna ekspozycja: mieszkaniec zetknął się z marką przez kampanię.
///
/// Zwraca `true`, gdy to był **pierwszy** kontakt tego człowieka z tą marką —
/// stąd bierze się lejek „znajomość → próba → afinitet" z M10 §6 pkt 1.
pub fn expose(
    world: &mut World,
    citizen: Entity,
    brand: BrandId,
    channel: AdChannelKind,
    claim: Q,
    day: u64,
) -> bool {
    // Pytanie „czy już zna" nie potrzebuje zaniku — pyta o **istnienie** slotu,
    // a zanik zmienia wartości, nie zbiór. Odczyt bez kopiowania kalibracji.
    let znal = magnat_agents::slots_of(world, citizen, day)
        .as_slice()
        .iter()
        .any(|s| s.brand == brand);
    let _ = magnat_agents::touch(world, citizen, brand, Touch::Ad { channel, claim }, day);
    !znal
}

/// Ekspozycja medialna: siła aktualizacji zależy od wiarygodności tytułu.
pub fn expose_media(
    world: &mut World,
    citizen: Entity,
    brand: BrandId,
    claim: Q,
    credibility: Q,
    day: u64,
) {
    let _ = magnat_agents::touch(
        world,
        citizen,
        brand,
        Touch::Media {
            claim,
            credibility: credibility.get(),
        },
        day,
    );
}
