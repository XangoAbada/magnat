//! Łańcuch dostaw w harmonogramie systemów (§5.11, wykonanie `AO-2`).
//!
//! **Jeden system, nie osiem — i to jest korekta planu, nie skrót** (`AP-4`).
//! §5.11 rozpisuje łańcuch na piętnaście systemów z własnymi częstotliwościami.
//! Rozpisanie go tak dzisiaj dałoby piętnaście systemów **wyłącznych** (`K-21`)
//! nad jednym `Mutex`em, bo cały stan łańcucha siedzi w jednym zasobie `Chain`
//! (`AO-1`, `AD-7`, `AG-2`, `AI-2`) — a system wyłączny stoi sam na swoim poziomie.
//! Piętnaście poziomów zamiast jednego, zero krawędzi DAG do wykorzystania, zero
//! zrównoleglenia: ceremonia, nie harmonogram.
//!
//! **Co z §5.11 zostaje w mocy i jest tu wykonane:**
//! 1. **częstotliwości** — minuta, godzina, doba i miesiąc mają w [`Chain::step`]
//!    osobne gałęzie i osobne warunki brzegowe;
//! 2. **rozpraszanie po indeksie encji** — zakład `i` przegląda się w minucie `i % 60`,
//!    slot `i` scala się w minucie `i % 1440`. To jest ta część §5.11, która naprawdę
//!    decyduje o budżecie §7.4, i ona jest zrobiona;
//! 3. **kolejności, które są kontraktem** — psucie przed detalem (`D12`), kaskada
//!    przed rynkiem (`AJ-3`), przybycia przed eksportem (`AJ-3`). Pierwsza jest teraz
//!    **krawędzią w DAG** (`supply.Chain` przed `economy.Market`), a nie komentarzem
//!    w środku cudzego systemu; dwie pozostałe są kolejnością wewnątrz [`Chain::step`].
//!
//! **Co się przez to zmienia w własności.** Do M6d kadencję łańcucha wołał
//! `MarketSystem` z `sim/economy` — czyli M5 prowadził zegar M6. Teraz łańcuch ma
//! własny system, własny `SystemId` i własną krawędź, a rynek detaliczny **odbiera**
//! od niego fakty przez [`ChainHandle::take_tick`], zamiast je produkować.

use magnat_core::{Cadence, SimMinute};
use magnat_ecs::{System, SystemCtx, SystemDesc, SystemId};

use crate::ChainHandle;

/// Minuta łańcucha dostaw: produkcja, przewozy, psucie, przeglądy, rynek B2B, doba.
pub struct ChainSystem {
    desc: SystemDesc,
}

impl ChainSystem {
    #[must_use]
    pub fn new() -> ChainSystem {
        ChainSystem {
            // Wyłączny (`K-21`), bo krok łańcucha jest gotową funkcją nad całym jego
            // stanem i nie da się go opisać zbiorem komponentów — a rozpisanie dostępu
            // dałoby **fałszywą** deklarację, gdyby ktoś czegoś nie wypisał.
            //
            // `before(economy.Market)` jest kontraktem `D12`: psucie musi zdjąć towar
            // po dacie, **zanim** półka go sprzeda. Na tym stoi `prop_no_expired_on_shelf`
            // i to jest jedyny powód, dla którego ta krawędź istnieje.
            desc: SystemDesc::new("supply.Chain", Cadence::EveryMinute)
                .exclusive()
                .before(SystemId::from_name("economy.Market")),
        }
    }
}

impl Default for ChainSystem {
    fn default() -> ChainSystem {
        ChainSystem::new()
    }
}

impl System for ChainSystem {
    fn desc(&self) -> &SystemDesc {
        &self.desc
    }

    fn run(&mut self, ctx: &mut SystemCtx<'_>) {
        let Some(chain) = ctx.world().get_resource::<ChainHandle>().cloned() else {
            return;
        };
        let tick = ctx.tick;
        let wynik = {
            let mut ch = chain.lock();
            ch.step(
                &chain.cat,
                &chain.tuning,
                chain.oracle.as_ref(),
                chain.deposits.as_ref(),
                ctx.seed,
                SimMinute(tick.get()),
            )
        };
        chain.post_tick(wynik);
    }
}
