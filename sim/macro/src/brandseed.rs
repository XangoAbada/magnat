//! Marka na moście makro↔mezo: agregat przy `lift()`, zasiew przy `lower()`.
//!
//! Wykonanie decyzji `D7` fazy M10, przeniesionej z M10a do M10b z imieniem:
//! „`lower()` zasiewa 3–5 wpisów doświadczeń i 2–4 sloty marek per mieszkaniec,
//! wybierając sklepy z jego dzielnicy proporcjonalnie do ich udziału rynkowego".
//!
//! **Dlaczego to musiało poczekać na markę.** Sklepy, którymi się zasiewa pamięć,
//! wskazuje udział rynkowy **marki**, a marki do M10b nie było: `MacroFirm.brand_stock`
//! stał w modelu od M7f i był zerem w każdym przebiegu — polem, którego nikt nie
//! wypełniał i nikt nie czytał. Ten moduł zamyka obie strony naraz.
//!
//! **Zasiew nie rusza pieniądza** i dlatego nie ma go w `LowerReport.unsettled`:
//! pamięć nie jest bilansem, a mieszkaniec, który „już tam kupował", nie ma z tego
//! tytułu ani grosza mniej.

use crate::state::MacroState;
use magnat_agents::{Population, Touch};
use magnat_core::{rng, BrandId, DistrictId, StreamId, Tick, Q};
use magnat_ecs::World;

/// Ile marek dostaje mieszkaniec w zasiewie — dolna i górna granica (`D7`).
pub const SEED_MIN: u32 = 2;
pub const SEED_MAX: u32 = 4;

/// Marki już zasiane temu mieszkańcowi — najwyżej [`SEED_MAX`], więc bez alokacji.
type Wziete = magnat_agents::arrayvec::ArrayVec<u16, { SEED_MAX as usize }>;

/// Ile razy zasiew powtarza doświadczenie z jedną marką.
///
/// Dwa, bo jedno doświadczenie daje znajomość na poziomie „raz tam byłem", a `D7`
/// mówi o świecie **zużytym**: mieszkaniec po osiemdziesięciu latach historii ma
/// swoje sklepy, a nie pierwsze wrażenie.
const POWTORZEN: u32 = 2;

/// Wypełnia `MacroFirm.brand_stock` z pamięci mieszkańców. Woła `lift()`.
///
/// Jeden przebieg po populacji na całą tablicę firm, a nie jeden na firmę: sloty
/// marek są u mieszkańca, więc tanio jest zapytać każdego raz, o co ma w pamięci.
pub fn fill_brand_stock(state: &mut MacroState, world: &World, today: u64) {
    let spis = world.resource::<Population>().citizens().to_vec();
    if spis.is_empty() {
        return;
    }
    // `(marka, suma afinitetów, ilu zna)` posortowane po marce — bez `HashMap` (00 §3.2).
    let mut agregat: Vec<(u16, i64, u32)> = Vec::new();
    for e in &spis {
        for slot in magnat_agents::slots_of(world, *e, today).as_slice() {
            let klucz = slot.brand.0;
            match agregat.binary_search_by_key(&klucz, |(b, _, _)| *b) {
                Ok(i) => {
                    agregat[i].1 += i64::from(slot.affinity);
                    agregat[i].2 += 1;
                }
                Err(i) => agregat.insert(i, (klucz, i64::from(slot.affinity), 1)),
            }
        }
    }
    let populacja = spis.len() as i64;
    for f in &mut state.firms {
        let Some(marka) = magnat_supply::brand_of(f.id) else {
            f.brand_stock = 0;
            continue;
        };
        // `suma / populacja`, a nie `(suma / ilu) * ilu / populacja`: drugi zapis
        // obcina dwa razy i przy `|suma| < ilu` daje zero **zawsze**, czyli marka
        // znana słabo przez wielu nie istniałaby w modelu wcale.
        f.brand_stock = agregat
            .binary_search_by_key(&marka.0, |(b, _, _)| *b)
            .map(|i| (agregat[i].1 / populacja) as i32)
            .unwrap_or(0);
    }
}

/// Zasiewa pamięć marek po rozwinięciu makra do świata. Woła `lower()`.
///
/// Zwraca liczbę zasianych slotów. Zero znaczy, że w mieście nie ma firm z marką —
/// i jest to poprawny stan świata, a nie błąd.
pub fn seed_memory(state: &MacroState, world: &mut World, seed: u64, tick: Tick) -> u32 {
    // Firmy dzielnicy z wagą udziału rynkowego: przepustowość razy wykorzystanie.
    // Zakład, który nic nie produkuje, nie ma czym zbudować sobie pamięci u klienta.
    let mut wg_dzielnicy: Vec<(DistrictId, Vec<(BrandId, u64)>)> = Vec::new();
    for f in &state.firms {
        let Some(marka) = magnat_supply::brand_of(f.id) else {
            continue;
        };
        let waga = (f.capacity_daily.get().max(0) as u64) * u64::from(f.utilization_bps.max(1));
        if waga == 0 {
            continue;
        }
        match wg_dzielnicy.binary_search_by_key(&f.district, |(d, _)| *d) {
            Ok(i) => wg_dzielnicy[i].1.push((marka, waga)),
            Err(i) => wg_dzielnicy.insert(i, (f.district, vec![(marka, waga)])),
        }
    }
    if wg_dzielnicy.is_empty() {
        return 0;
    }
    for (_, lista) in &mut wg_dzielnicy {
        lista.sort_unstable_by_key(|(b, _)| b.0);
    }

    let dzis = tick.0 / 1_440;
    let spis = world.resource::<Population>().citizens().to_vec();
    let mut zasiane = 0u32;
    for e in spis {
        let Some(r) = world.get::<magnat_agents::Residence>(e).copied() else {
            continue;
        };
        let Ok(i) = wg_dzielnicy.binary_search_by_key(&DistrictId(r.district), |(d, _)| *d) else {
            continue;
        };
        let lista = wg_dzielnicy[i].1.clone();
        if lista.is_empty() {
            continue;
        }
        let mut rnd = rng(seed, StreamId::MacroSeedMemory, e.index(), tick);
        let ile = SEED_MIN + rnd.gen_range_u32(SEED_MAX - SEED_MIN + 1);
        // Losowanie **bez powtórzeń**: dwa trafienia w tę samą markę dałyby jeden
        // slot, więc mieszkaniec dostałby ich mniej, niż `D7` obiecuje. Dzielnica
        // z dwoma sklepami daje dwa sloty i to jest poprawna odpowiedź — nie da się
        // zapamiętać czterech sklepów, jeśli są dwa.
        let mut wziete: crate::brandseed::Wziete = Default::default();
        for _ in 0..ile.min(lista.len() as u32) {
            // Wybór proporcjonalny do udziału: dystrybuanta po posortowanej liście,
            // więc kolejność losowań nie zależy od kolejności wstawień do wektora.
            let wolne: Vec<(BrandId, u64)> = lista
                .iter()
                .filter(|(b, _)| !wziete.contains(&b.0))
                .copied()
                .collect();
            let reszta: u64 = wolne.iter().map(|(_, w)| *w).sum();
            if reszta == 0 {
                break;
            }
            let los = (u64::from(rnd.next_u32()) * reszta) >> 32;
            let mut skumulowana = 0u64;
            let mut wybrana = wolne[0].0;
            for (b, w) in &wolne {
                skumulowana += *w;
                if los < skumulowana {
                    wybrana = *b;
                    break;
                }
            }
            wziete.push(wybrana.0);
            // Jakość, której mieszkaniec doświadczył: przeciętna plus renoma marki.
            let renoma = state
                .firms
                .iter()
                .find(|f| magnat_supply::brand_of(f.id) == Some(wybrana))
                .map_or(0, |f| f.brand_stock);
            let jakosc = Q::new((50 + renoma).clamp(0, 100) as u8);
            for _ in 0..POWTORZEN {
                let _ = magnat_agents::touch(
                    world,
                    e,
                    wybrana,
                    Touch::Experience { actual: jakosc },
                    dzis,
                );
            }
            zasiane += 1;
        }
    }
    zasiane
}
