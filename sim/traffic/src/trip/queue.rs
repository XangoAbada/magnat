//! Kolejka oczekujących na wjazd na krawędź: spillback i rozplątywanie
//! zakleszczeń (M4b/WP4, ryzyko R4).
//!
//! Wydzielone z `trip.rs` w R-WP6 bez zmiany zachowania. To jest mechanizm
//! krawędzi, nie podróży: lista jednokierunkowa `wait_next` wpięta w `ActiveTrip`,
//! głowa i ogon w `MezoState`, a `unjam` jest jedynym miejscem, w którym obłożenie
//! krawędzi wolno przekroczyć pojemność.

use super::*;

impl TrafficNetwork {
    /// Wstawia podroz na koniec kolejki oczekujacych na wjazd na krawedz.
    pub(super) fn enqueue_wait(&mut self, edge: EdgeId, slot: u32) {
        let i = edge.0 as usize;
        self.trips[slot as usize]
            .as_mut()
            .expect("podroz w toku")
            .wait_next = crate::mezo::NO_WAIT;
        let ogon = self.mezo.wait_tail[i];
        if ogon == crate::mezo::NO_WAIT {
            self.mezo.wait_head[i] = slot;
            self.waiting_edges.push(edge.0);
        } else {
            self.trips[ogon as usize]
                .as_mut()
                .expect("oczekujacy")
                .wait_next = slot;
        }
        self.mezo.wait_tail[i] = slot;
    }

    /// Budzi pierwszego oczekujacego na krawedz, ktora wlasnie sie zwolnila.
    /// Godzina przebudzenia to chwila zwolnienia miejsca plus jeden odstep --
    /// tyle, ile trwa ruszenie z kolejki.
    pub(super) fn wake_one(&mut self, edge: EdgeId, at_cs: u64) {
        let i = edge.0 as usize;
        let slot = self.mezo.wait_head[i];
        if slot == crate::mezo::NO_WAIT {
            return;
        }
        let czekal;
        let (nastepny, veh, cs) = {
            let t = self.trips[slot as usize].as_mut().expect("oczekujacy");
            let przed = t.exit_cs;
            t.exit_cs = at_cs.max(t.exit_cs) + u64::from(crate::mezo::HEADWAY_CS);
            czekal = t.exit_cs - przed;
            t.pending_blocked_cs = t
                .pending_blocked_cs
                .saturating_add(czekal.min(u64::from(u32::MAX)) as u32);
            let n = t.wait_next;
            t.wait_next = crate::mezo::NO_WAIT;
            (n, t.vehicle, t.exit_cs)
        };
        self.mezo.wait_head[i] = nastepny;
        if nastepny == crate::mezo::NO_WAIT {
            self.mezo.wait_tail[i] = crate::mezo::NO_WAIT;
        }
        self.stats.blocked_cs_total += czekal;
        self.heap.push(Reverse((cs, veh, slot)));
    }

    /// Rozplatanie zakleszczenia (ryzyko R4): krawedz pelna i nieruchoma od
    /// [`GRIDLOCK_RELEASE_MIN`] minut wypuszcza pierwszego oczekujacego mimo braku
    /// miejsca. To jedyne miejsce, w ktorym oblozenie krawedzi przekracza pojemnosc.
    pub(super) fn unjam(&mut self, now: u32) {
        let at_cs = u64::from(now) * CS_PER_MINUTE;
        let mut edges = std::mem::take(&mut self.waiting_edges);
        edges.sort_unstable();
        edges.dedup();
        edges.retain(|e| {
            let i = *e as usize;
            if self.mezo.wait_head[i] == crate::mezo::NO_WAIT {
                return false;
            }
            if self.mezo.queues[i].stuck_minutes >= GRIDLOCK_RELEASE_MIN as u16 {
                // Krawędź pełna i nieruchoma od kilku minut to **zakleszczenie**,
                // a nie kolejka: jej pojazdy czekają na coś, co czeka na nią.
                // Budzenie jednego oczekującego rozwiązywałoby taki cykl w tempie
                // jednego pojazdu na kilka minut, czyli wcale. Wypuszczamy cały
                // ogonek naraz — nadmiar rozejdzie się w następnych minutach.
                while self.mezo.wait_head[i] != crate::mezo::NO_WAIT {
                    self.stats.gridlock_releases += 1;
                    self.wake_one(EdgeId(*e), at_cs);
                }
                self.mezo.queues[i].stuck_minutes = 0;
            }
            self.mezo.wait_head[i] != crate::mezo::NO_WAIT
        });
        self.waiting_edges = edges;
    }
}
