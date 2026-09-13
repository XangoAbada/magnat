//! `magnat-sim-snapshot` — jedyny kanał z symulacji do renderu (00 §1, M1 §6.1).
//!
//! **Właścicielem schematu ładunku jest M11, nie M1.** Ten crate powstaje tutaj w postaci
//! minimalnej z powodu technicznego: `engine/render` nie skompiluje się bez niego, a reguła
//! uzgodniona z M11 mówi, że render **nie może mieć w drzewie zależności żadnego `sim/*`
//! poza tym jednym crate'em** (pilnuje tego `cargo tree` w CI, M1 §7.3). Cztery pola poniżej
//! to potrzeba M1, a nie propozycja kształtu całości — M11 dokłada rekordy encji i zmienia,
//! co uzna za stosowne.
//!
//! Zawartość jest **wyłącznie POD**: zero zależności od `sim/*`, zero `Arc`, zero `Vec`
//! w ładunku publikowanym co klatkę. Snapshot publikuje się do 30 razy na sekundę, więc
//! `Vec` w ładunku znaczy alokację na publikację — czyli dokładnie to, czemu podwójne
//! buforowanie ma zapobiegać.

#![forbid(unsafe_code)]

use magnat_core::{SimMinute, Tick};

/// Globalny limit świateł w klatce (M1 §5.8). Cap jest **stały**, nie dynamiczny:
/// przepełnienie obsługuje producent snapshotu, obcinając po malejącym dystansie,
/// a nie pass, który musiałby wtedy alokować w środku klatki.
pub const MAX_LIGHTS: usize = 4096;

/// Światło punktowe w klatce. 20 B na rekord, 80 KB na pełny cap.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
#[repr(C)]
pub struct LightRecord {
    /// Pozycja w metrach, względem początku świata.
    pub pos: [f32; 3],
    /// Zasięg w metrach.
    pub range: f32,
    /// Barwa i natężenie spakowane w RGBE.
    pub color_rgbe: u32,
}

/// Tablica o stałej pojemności — odpowiednik `SoaSlice` z planu M1 §6.1.
///
/// Osobny typ zamiast `[T; N]` z licznikiem obok, bo licznik trzymany osobno rozjeżdża się
/// z zawartością dokładnie wtedy, gdy ktoś doda drugie miejsce zapisu.
#[derive(Clone, Copy, Debug)]
pub struct CappedSlice<T: Copy, const N: usize> {
    items: [T; N],
    len: u32,
}

impl<T: Copy + Default, const N: usize> Default for CappedSlice<T, N> {
    fn default() -> Self {
        CappedSlice {
            items: [T::default(); N],
            len: 0,
        }
    }
}

impl<T: Copy, const N: usize> CappedSlice<T, N> {
    /// Dopisuje element. Zwraca `false` przy przepełnieniu — **bez paniki i bez cichego
    /// gubienia**: producent ma wtedy obciąć listę po priorytecie, co jest jego decyzją,
    /// nie naszą (M1 §5.8).
    pub fn push(&mut self, v: T) -> bool {
        if (self.len as usize) >= N {
            return false;
        }
        self.items[self.len as usize] = v;
        self.len += 1;
        true
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.items[..self.len as usize]
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.len as usize
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[must_use]
    pub const fn capacity(&self) -> usize {
        N
    }
}

/// Ładunek publikowany z symulacji do renderu.
#[derive(Clone, Copy, Debug)]
pub struct RenderSnapshot {
    pub tick: Tick,
    /// Pozycja słońca i cykl dobowy wyprowadzają się z tej jednej liczby.
    pub sim_minute: SimMinute,
    /// Cel śledzenia kamery; `None` = kamera swobodna.
    pub camera_hint: Option<[f64; 3]>,
    /// Rośnie przy każdej edycji voxeli — unieważnia cache chunków po stronie renderu.
    pub terrain_revision: u32,
    pub lights: CappedSlice<LightRecord, MAX_LIGHTS>,
}

impl Default for RenderSnapshot {
    fn default() -> Self {
        RenderSnapshot {
            tick: Tick(0),
            sim_minute: SimMinute(0),
            camera_hint: None,
            terrain_revision: 0,
            lights: CappedSlice::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_swiatel_nie_panikuje_tylko_odmawia() {
        let mut s: CappedSlice<LightRecord, 4> = CappedSlice::default();
        for _ in 0..4 {
            assert!(s.push(LightRecord::default()));
        }
        assert!(!s.push(LightRecord::default()), "przepełnienie przeszło");
        assert_eq!(s.len(), 4);
        assert_eq!(s.capacity(), 4);
        s.clear();
        assert!(s.is_empty());
    }

    #[test]
    fn snapshot_nie_alokuje() {
        // Cały sens stałego capa: struktura jest `Copy` i mieści się na stosie.
        // Gdyby ktoś dołożył `Vec`, ten test przestanie się kompilować — i o to chodzi.
        fn wymaga_copy<T: Copy>(_: &T) {}
        let s = RenderSnapshot::default();
        wymaga_copy(&s);
        assert_eq!(s.lights.capacity(), MAX_LIGHTS);
    }
}
