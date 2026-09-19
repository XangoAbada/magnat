//! Faza 3 — produkcja i zatowarowanie (M10a §5.7, co krok).
//!
//! Firma uzupełnia zapas każdego towaru, który sprzedaje, do docelowego pokrycia
//! w dobach — ograniczona przez `kernel::throughput`, czyli przez to, ilu ma ludzi.
//! Płaci za to z kapitału na konto reszty świata; pieniądz nie znika, tylko
//! przechodzi przez granicę modelu.
//!
//! # Jednostka, w której liczy `throughput`
//!
//! `kernel::throughput` bierze i zwraca `Mass` (gramy), bo taka jest jednostka
//! linii produkcyjnej w M6. Tutaj karmimy go **jednostkami natywnymi towaru**
//! (`MacroStock`, `K-34`/M6a) i odczytujemy tak samo. To jest legalne, bo funkcja
//! jest czystą arytmetyką `nominał × czas × dławienie × obsada` i nie zagląda
//! do jednostki — a `ponytail:` sufit jest nazwany: gdyby kiedyś `throughput`
//! zaczął sięgać po gęstość albo po katalog, makro musiałoby przeliczać na granicy,
//! czyli zaokrąglać, czyli łamać zachowanie masy (§5.10).

use magnat_core::{Mass, Money};
use magnat_economy::kernel;
use magnat_firms::hr::productivity::FULL_TIME;

use crate::state::MacroState;
use crate::types::MacroAccount;

use super::MacroParams;

/// Doba ma 1440 minut — `throughput` liczy szarżę z czasu jej trwania.
const MINUTES_PER_DAY: u32 = 1_440;

pub fn phase(st: &mut MacroState, p: &MacroParams) {
    // Szok podażowy dławi wsad wszystkim zakładom naraz — to jest cała treść
    // wstrząsu w skali miasta (faza 8). Punkt bazowy schodzi tu na procent, bo
    // `throughput` liczy dławienie w procentach: to jest zmiana jednostki, a nie
    // stosowanie stawki do kwoty.
    let dlawienie = u8::try_from((100 + st.supply_shift_bp() / 100).clamp(0, 100)).unwrap_or(100);
    // Czas trwania szarży rośnie z długością kroku — `throughput` liczy przerób
    // z minut, więc krok sześciodobowy to sześć razy więcej minut pracy.
    let minut = MINUTES_PER_DAY.saturating_mul(u32::from(p.days_per_step.max(1)));
    for f in &mut st.firms {
        let etaty = (f.capacity_daily.get() / FULL_TIME).max(0);
        if etaty == 0 || f.price.is_empty() {
            f.utilization_bps = 0;
            continue;
        }
        // Pokrycie etatowe w promilach — ta sama liczba i to samo znaczenie,
        // co `Site::labor_pct` w mezo (`K-44`).
        let labor_pct = u16::try_from(
            (i64::from(f.employees) * 1_000 / etaty)
                .clamp(0, i64::from(magnat_supply::PlantSite::FULL_LABOR)),
        )
        .unwrap_or(0);
        if f.capital.get() < 0 {
            f.utilization_bps = 0;
            continue;
        }

        let zdolnosc = kernel::throughput(
            Mass(p.nominal_per_slot_hour.saturating_mul(etaty)),
            minut,
            dlawienie,
            labor_pct,
        )
        .0;
        if zdolnosc <= 0 {
            f.utilization_bps = 0;
            continue;
        }

        // Ile brakuje do docelowego pokrycia. Sprzedaż wczorajszej doby jest
        // zapisana w `demand` komórek — firma jej nie zna, więc bierze własny
        // ubytek: różnicę między zapasem docelowym a bieżącym.
        let towary: Vec<magnat_core::GoodId> = f.price.iter().map(|(g, _)| g).collect();
        let na_towar = zdolnosc / towary.len().max(1) as i64;
        let mut wydatek: i64 = 0;
        let mut wykorzystanie: i64 = 0;
        for g in towary {
            let cel = na_towar.saturating_mul(i64::from(p.target_cover_days));
            let brak = (cel - f.stock.get(g)).max(0).min(na_towar);
            if brak == 0 {
                continue;
            }
            let koszt = f.cost.get(g).unwrap_or(Money::ZERO).get();
            let kwota = koszt.saturating_mul(brak);
            // Nie kupuje na kredyt: to jest różnica między firmą, która zwalnia
            // tempo, a firmą, która dopisuje sobie pieniądz.
            if kwota > f.capital.get().saturating_sub(wydatek) {
                continue;
            }
            f.stock.add(g, brak);
            wydatek = wydatek.saturating_add(kwota);
            wykorzystanie = wykorzystanie.saturating_add(brak);
        }
        f.capital = Money(f.capital.get().saturating_sub(wydatek));
        f.utilization_bps =
            u16::try_from((wykorzystanie.saturating_mul(10_000) / zdolnosc).clamp(0, 10_000))
                .unwrap_or(0);
        if wydatek > 0 {
            st.ledger.transfer(
                MacroAccount::Firms,
                MacroAccount::RestOfWorld,
                Money(wydatek),
            );
        }
    }
}
