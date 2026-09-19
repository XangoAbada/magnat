//! `MacroLodPolicy` — kiedy wolno przełączyć poziom szczegółowości (WP10.1 pkt 4).
//!
//! Podział ról jest w kontrakcie M10 §6 i jest ostry: **M12 dostarcza wywołanie
//! z pętli gry i `SpeedGovernor`, M10 daje matematykę.** Tutaj nie ma więc ani
//! jednego odczytu zegara, ani jednej decyzji o prędkości gry — jest funkcja
//! stanu, którą M12 zawoła i której odpowiedź wykona.
//!
//! # Dlaczego histereza, a nie próg
//!
//! Próg pojedynczy znaczy, że obciążenie oscylujące wokół niego przełącza poziom
//! co klatkę — a każde przełączenie to `lift()` albo `lower()`, czyli przejście
//! po całym mieście. Dwa progi (wyżej na wejście w makro, niżej na wyjście) robią
//! z tego jedno przełączenie zamiast setki, a różnica między nimi jest jedyną
//! liczbą, którą ta polityka naprawdę stroi.
//!
//! # Dlaczego blokada
//!
//! Przejście w makro **w środku** fixingu giełdowego, negocjacji ze związkiem albo
//! strajku zgubiłoby stan, którego makro nie niesie — a zgubiłoby go cicho, bo
//! agregat się zgadza. Dlatego `may_transition` odpowiada powodem odmowy,
//! a nie wartością logiczną: wołający ma napisać w dzienniku, **na co** czeka.

use crate::state::MacroState;

/// Poziom szczegółowości symulacji miasta.
///
/// Nie jest tym samym co `magnat_agents::Lod` (LOD pojedynczej encji): tamten
/// mówi, jak dokładnie liczy się **jeden** mieszkaniec, a ten — czy miasto
/// w ogóle liczy się po mieszkańcach, czy po komórkach.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CityLod {
    /// Pełne mezo: encje, transakcje, trasy.
    Mezo,
    /// Agregat dzielnica × klasa — `MacroState` i [`crate::step`].
    Makro,
}

/// Obciążenie zmierzone przez wołającego. Wszystko w jednostkach, które M12
/// i tak liczy dla `SpeedGovernor` — ta struktura niczego nie mierzy sama.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoadStats {
    /// Ile minut gry przypada na sekundę realną (mnożnik prędkości).
    pub speed_multiplier: u16,
    /// Czas ostatniego ticku symulacji w mikrosekundach.
    pub tick_us: u32,
    /// Budżet ticku w mikrosekundach — ile wolno.
    pub budget_us: u32,
}

impl LoadStats {
    /// Wykorzystanie budżetu w promilach. Budżet zerowy znaczy „nie mierzono"
    /// i daje zero, a nie nieskończoność — brak pomiaru nie jest przeciążeniem.
    #[must_use]
    pub fn load_permille(&self) -> u32 {
        if self.budget_us == 0 {
            return 0;
        }
        u32::try_from(u64::from(self.tick_us) * 1_000 / u64::from(self.budget_us))
            .unwrap_or(u32::MAX)
    }
}

/// Dlaczego nie teraz.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockReason {
    /// Trwa wstrząs w skali miasta — przełączenie zgubiłoby jego pozostały czas.
    ShockInProgress,
    /// Świat nie ma jeszcze ani jednej komórki: nie ma czego podnieść.
    NothingToLift,
    /// Komórka poniżej progu liczebności (`MIN_CELL_POP`) — agregat nie mieści się
    /// w deklarowanym błędzie, więc przejście w makro byłoby obietnicą bez pokrycia.
    CellTooSmall,
}

/// Progi przełączania i warunki blokady (M10 §6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MacroLodPolicy {
    /// Od jakiego mnożnika prędkości gry makro w ogóle wchodzi w grę.
    pub speed_enter: u16,
    /// Poniżej jakiego mnożnika wraca mezo. **Musi być mniejszy** od `speed_enter` —
    /// to jest cała histereza.
    pub speed_exit: u16,
    /// Wykorzystanie budżetu ticku, przy którym makro wchodzi mimo niskiej prędkości.
    pub load_enter_permille: u32,
    /// Wykorzystanie, poniżej którego mezo wraca.
    pub load_exit_permille: u32,
}

impl Default for MacroLodPolicy {
    // macro-guard: kalibracja przełączania, stroi ją M12 wraz z trybem 50×
    fn default() -> MacroLodPolicy {
        MacroLodPolicy {
            // Prędkości gry to `Paused`, `X1`, `X3`, `X10` i `X50` od M12 (`K-22`).
            // Makro wchodzi od pięćdziesiątki, a wychodzi poniżej dziesiątki —
            // między nimi jest pas, w którym poziom się nie zmienia.
            speed_enter: 50,
            speed_exit: 10,
            load_enter_permille: 900,
            load_exit_permille: 500,
        }
    }
}

impl MacroLodPolicy {
    /// Poziom, do którego należy dążyć przy tym obciążeniu i tej prędkości.
    ///
    /// Histereza jest po stronie **obecnego** poziomu: pytanie brzmi „czy zmienić",
    /// a nie „jaki poziom pasuje". Bez `from` funkcja nie miałaby jak odróżnić
    /// wejścia od wyjścia i dwa progi nie znaczyłyby nic.
    #[must_use]
    pub fn target_lod(&self, from: CityLod, load: &LoadStats) -> CityLod {
        let obciazenie = load.load_permille();
        match from {
            CityLod::Mezo => {
                if load.speed_multiplier >= self.speed_enter
                    || obciazenie >= self.load_enter_permille
                {
                    CityLod::Makro
                } else {
                    CityLod::Mezo
                }
            }
            CityLod::Makro => {
                if load.speed_multiplier < self.speed_exit && obciazenie < self.load_exit_permille {
                    CityLod::Mezo
                } else {
                    CityLod::Makro
                }
            }
        }
    }

    /// Czy wolno teraz przejść z `from` na `to`.
    ///
    /// `None` znaczy „wolno". Kontrakt M10 §6 wymienia fixing, negocjacje i strajk;
    /// po M10a istnieje z nich **wstrząs w skali miasta**, a giełda i związki
    /// dokładają swoje warunki w M10d i M10e — każdy jako wariant [`BlockReason`],
    /// bo `match` bez ramienia `_` nie skompiluje się wtedy po stronie M12.
    #[must_use]
    pub fn may_transition(
        &self,
        from: CityLod,
        to: CityLod,
        state: &MacroState,
    ) -> Option<BlockReason> {
        if from == to {
            return None;
        }
        if state.cells.is_empty() {
            return Some(BlockReason::NothingToLift);
        }
        if state.shock.is_some() {
            return Some(BlockReason::ShockInProgress);
        }
        if to == CityLod::Makro
            && state
                .cells
                .iter()
                .any(|c| c.population() < crate::types::MIN_CELL_POP)
        {
            return Some(BlockReason::CellTooSmall);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ClassGrain;

    fn obciazenie(speed: u16, permille: u32) -> LoadStats {
        LoadStats {
            speed_multiplier: speed,
            tick_us: permille,
            budget_us: 1_000,
        }
    }

    #[test]
    fn histereza_nie_przelacza_w_pasie_miedzy_progami() {
        let p = MacroLodPolicy::default();
        // Prędkość 30 leży między progiem wyjścia (10) i wejścia (50): poziom
        // zostaje taki, jaki był — i to jest cała umowa histerezy.
        assert_eq!(
            p.target_lod(CityLod::Mezo, &obciazenie(30, 0)),
            CityLod::Mezo
        );
        assert_eq!(
            p.target_lod(CityLod::Makro, &obciazenie(30, 0)),
            CityLod::Makro
        );
    }

    #[test]
    fn przeciazenie_wciaga_w_makro_nawet_przy_wolnej_grze() {
        let p = MacroLodPolicy::default();
        assert_eq!(
            p.target_lod(CityLod::Mezo, &obciazenie(1, 950)),
            CityLod::Makro
        );
    }

    #[test]
    fn brak_pomiaru_nie_jest_przeciazeniem() {
        let p = MacroLodPolicy::default();
        let bez_budzetu = LoadStats {
            speed_multiplier: 1,
            tick_us: 999_999,
            budget_us: 0,
        };
        assert_eq!(p.target_lod(CityLod::Mezo, &bez_budzetu), CityLod::Mezo);
    }

    #[test]
    fn wstrzas_blokuje_przejscie_w_obie_strony() {
        let mut st = MacroState::empty(ClassGrain::Classes6, 1, 1);
        st.cells.push(crate::state::MacroCell::new(
            (magnat_core::DistrictId(0), crate::types::ClassId(0)),
            1,
        ));
        let p = MacroLodPolicy::default();
        st.shock = Some(crate::state::MacroShock {
            kind: crate::state::ShockKind::Recession,
            magnitude_bp: -1_000,
            started_day: 1,
            ends_day: 100,
        });
        assert_eq!(
            p.may_transition(CityLod::Mezo, CityLod::Makro, &st),
            Some(BlockReason::ShockInProgress)
        );
        assert_eq!(
            p.may_transition(CityLod::Makro, CityLod::Mezo, &st),
            Some(BlockReason::ShockInProgress)
        );
        assert_eq!(p.may_transition(CityLod::Mezo, CityLod::Mezo, &st), None);
    }

    #[test]
    fn za_mala_komorka_blokuje_wejscie_w_makro_ale_nie_wyjscie() {
        let mut st = MacroState::empty(ClassGrain::Classes6, 1, 1);
        let mut c =
            crate::state::MacroCell::new((magnat_core::DistrictId(0), crate::types::ClassId(0)), 1);
        c.citizens.push(crate::types::CitizenSeed {
            birth_index: 1,
            cell: 0,
        });
        st.cells.push(c);
        let p = MacroLodPolicy::default();
        assert_eq!(
            p.may_transition(CityLod::Mezo, CityLod::Makro, &st),
            Some(BlockReason::CellTooSmall)
        );
        assert_eq!(p.may_transition(CityLod::Makro, CityLod::Mezo, &st), None);
    }
}
