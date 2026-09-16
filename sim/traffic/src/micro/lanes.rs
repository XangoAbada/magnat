//! Zmiana pasa (MOBIL) i sąsiedztwo pojazdów na krawędzi.
//!
//! Wydzielone z `micro.rs` w R-WP10 bez zmiany zachowania. `przyspieszenie_w`
//! jest wariantem IDM liczonym „co by było, gdyby" — dlatego stoi przy MOBIL-u,
//! a nie przy `przyspieszenie` w `idm.rs`.

use super::*;

impl VehicleBuffer {
    /// MOBIL: zmiana pasa, gdy zysk własny przewyższa próg powiększony o uprzejmość
    /// wobec nadjeżdżającego z tyłu, a ten nie musi przy tym hamować ponad `safe_decel`.
    pub(super) fn zmien_pasy(&mut self, now_cs: u64, p: &IdmParams) {
        let n = self.vehs.len();
        let uprzejmosc = p.politeness_permille as f32 / 1000.0;
        let prog = p.threshold_cms2 as f32;
        let bezpieczne = -(p.safe_decel_cms2 as f32);

        // Zgłoszenia w porządku z `sortuj`, czyli totalnym — remisy są niemożliwe.
        let mut zgloszenia: Vec<(usize, u8, f32)> = Vec::new();
        for k in 0..n {
            let i = self.order[k] as usize;
            let me = self.vehs[i];
            if !me.car_following || me.lanes < 2 || me.edge == NO_EDGE {
                continue;
            }
            let teraz = self.przyspieszenie(i, self.lider(k), now_cs, p);
            for d in [-1i16, 1] {
                let cel = i16::from(me.lane) + d;
                if cel < 0 || cel >= i16::from(me.lanes) {
                    continue;
                }
                let cel = cel as u8;
                let (przed, za) = self.sasiedzi(i, cel);
                let po = self.przyspieszenie_w(i, przed, now_cs, p);
                // Nadjeżdżający z tyłu nie może dostać hamowania ponad próg.
                let strata_za = match za {
                    Some(j) => {
                        let bez = self.przyspieszenie_w(j, self.przed_w(j), now_cs, p);
                        let z = self.przyspieszenie_w(j, Some(i), now_cs, p);
                        if z < bezpieczne {
                            continue;
                        }
                        bez - z
                    }
                    None => 0.0,
                };
                let zysk = po - teraz - uprzejmosc * strata_za;
                if zysk > prog {
                    zgloszenia.push((i, cel, zysk));
                }
            }
        }
        // Przy dwóch zgłoszeniach tego samego pojazdu wygrywa większy zysk, a przy
        // remisie mniejszy numer pasa — klucz pozostaje totalny.
        zgloszenia
            .sort_unstable_by(|a, b| a.0.cmp(&b.0).then(b.2.total_cmp(&a.2)).then(a.1.cmp(&b.1)));
        let mut poprzedni = usize::MAX;
        for (i, cel, _) in zgloszenia {
            if i == poprzedni {
                continue;
            }
            poprzedni = i;
            self.vehs[i].lane = cel;
        }
    }

    /// Zakres w `order` zajmowany przez jeden pas jednej krawędzi.
    ///
    /// Szukanie połówkowe, a nie przegląd bufora: `order` jest już posortowane kluczem
    /// `(krawędź, pas, pozycja, encja)`, więc pas jest w nim **ciągłym** przedziałem.
    /// Zmierzone: przegląd liniowy w MOBIL kosztował 459 ms na minutę świata przy
    /// 3 tys. pojazdów na trzech pasach wobec 11 ms na jednym — bo dopiero drugi pas
    /// włącza szukanie sąsiadów, a ono było kwadratowe względem całego bufora.
    fn zakres(&self, edge: u32, lane: u8) -> std::ops::Range<usize> {
        let klucz = |k: &u32| {
            let v = &self.vehs[*k as usize];
            (v.edge, v.lane)
        };
        let od = self.order.partition_point(|k| klucz(k) < (edge, lane));
        let do_ = self.order.partition_point(|k| klucz(k) <= (edge, lane));
        od..do_
    }

    /// Poprzednik i następca pojazdu `i`, gdyby stanął na pasie `lane`.
    fn sasiedzi(&self, i: usize, lane: u8) -> (Option<usize>, Option<usize>) {
        let me = &self.vehs[i];
        let r = self.zakres(me.edge, lane);
        let pierwszy_przed =
            self.order[r.clone()].partition_point(|k| self.vehs[*k as usize].pos_cm < me.pos_cm);
        let przed = self.order[r.clone()]
            .get(pierwszy_przed)
            .map(|k| *k as usize)
            .filter(|j| *j != i);
        let za = pierwszy_przed
            .checked_sub(1)
            .and_then(|p| self.order[r].get(p))
            .map(|k| *k as usize)
            .filter(|j| *j != i);
        (przed, za)
    }

    /// Poprzednik pojazdu `i` na jego własnym pasie.
    fn przed_w(&self, i: usize) -> Option<usize> {
        let me = &self.vehs[i];
        let r = self.zakres(me.edge, me.lane);
        let p =
            self.order[r.clone()].partition_point(|k| self.vehs[*k as usize].pos_cm <= me.pos_cm);
        self.order[r].get(p).map(|k| *k as usize)
    }

    fn przyspieszenie_w(&self, i: usize, lider: Option<usize>, now_cs: u64, p: &IdmParams) -> f32 {
        let me = &self.vehs[i];
        let dlugosc = self.paths.len_cm(me.path) as f32;
        let a = p.accel_cms2 as f32;
        let b = p.decel_cms2 as f32;
        let v = me.speed_cms.max(0.0);
        let (luka, dv) = match lider {
            Some(j) => {
                let l = &self.vehs[j];
                (l.pos_cm - f32::from(l.len_cm) - me.pos_cm, v - l.speed_cms)
            }
            None => {
                if now_cs >= me.exit_cs {
                    (f32::MAX / 4.0, 0.0)
                } else {
                    (dlugosc - me.pos_cm, v)
                }
            }
        };
        let s = luka.max(10.0);
        let v0 = self.pozadana(me, now_cs, p);
        let ratio = (v / v0).min(4.0);
        let r2 = ratio * ratio;
        let s_star = p.s0_cm as f32
            + (v * (p.headway_ds as f32 / 10.0) + v * dv / (2.0 * (a * b).sqrt())).max(0.0);
        let z = s_star / s;
        (a * (1.0 - r2 * r2 - z * z)).clamp(-3.0 * b, a)
    }
}
