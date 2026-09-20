//! Wycena firmy: co wiadomo, kiedy się to wie i jak bardzo się mylimy (M10d §5.5).
//!
//! # Skąd bierze się cena
//!
//! PRD §6.5 żąda wyceny **z transakcji, nigdy z formuły wpisanej do stanu**. Ten
//! moduł nie ustala więc kursu — ustala, ile każdy inwestor **uważa**, że firma jest
//! warta, a kurs powstaje z przecięcia tych przekonań na fixingu ([`super::book`]).
//!
//! Trzy warstwy, każda z własnym zaburzeniem:
//!
//! 1. **`fundamental`** — z opublikowanych wyników. Nie z prawdziwych: firma publikuje
//!    wynik miesiąca 45 dób po jego zamknięciu i publikuje go z szumem
//!    ([`Published`]). Inwestor nie zna prawdziwego zysku i nie ma jak go poznać.
//! 2. **`belief`** — `fundamental` przemnożony przez szum inwestora. Bez tego szumu
//!    arkusz zleceń byłby pusty: gdyby wszyscy liczyli tę samą liczbę, nikt nie
//!    miałby powodu sprzedać temu, kto kupuje.
//! 3. **plotka** — korekta z ujawnionych pakietów, gasnąca w ciągu tygodnia.
//!    To jest cała mechanika „ktoś wiedział wcześniej": kurs rusza się przed
//!    publikacją **tylko** wtedy, gdy ktoś rzucił pakiet na rynek.
//!
//! Rok gry ma 360 dób (`K-1`), więc „T+45 dni" to półtora miesiąca i wypada
//! piętnastego dnia drugiego miesiąca po zamkniętym — bez konwencji dziennej
//! i bez ani jednego `365` w kodzie.

use magnat_core::{rng, Money, StreamId, Tick};

/// Ile dób po zamknięciu miesiąca firma publikuje jego wynik (PRD §6.5, plan §5.5).
pub const PUBLISH_LAG_DAYS: u32 = 45;

/// Opublikowany obraz firmy — to, co inwestor ma prawo wiedzieć.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Published {
    /// Ostatni miesiąc objęty publikacją.
    pub month: u32,
    /// Zysk z dwunastu miesięcy kończących się na `month`, **z szumem**.
    pub profit_12m: Money,
    /// Wartość księgowa: kapitał własny zakładów plus saldo firmy.
    pub book: Money,
}

/// Wskaźnik cena/zysk wyprowadzony ze stopy bazowej.
///
/// Odwrotność stopy, przycięta do widełek: przy stopie 5 % rocznie złotówka zysku
/// jest warta dwadzieścia złotych, przy 20 % — pięć. Widełki są konieczne w obie
/// strony: stopa bliska zeru dałaby mnożnik nieskończony, a stopa 100 % — firmę
/// wartą rocznego zysku, czyli mniej niż jej własny magazyn.
#[must_use]
pub fn price_earnings(base_rate_bp: u32, min: u16, max: u16) -> u32 {
    let r = base_rate_bp.max(1);
    (10_000 / r).clamp(u32::from(min), u32::from(max))
}

/// Wartość fundamentalna **całej firmy** w groszach.
///
/// `max(wartość księgowa, zysk × mnożnik)` — bo firma przynosząca stratę nadal ma
/// majątek, a firma bez majątku, ale z zyskiem, nadal ma wartość. Zero znaczy
/// „nie ma z czego liczyć", a nie „nic nie warta", i wtedy notowanie zostaje przy
/// wczorajszym kursie.
#[must_use]
pub fn fundamental(p: &Published, pe: u32) -> Money {
    let z_zysku = p.profit_12m.get().saturating_mul(i64::from(pe)).max(0);
    Money(p.book.get().max(0).max(z_zysku))
}

/// Ile ten inwestor uważa, że warta jest **jedna bp** udziału.
///
/// `noise_bp` jest szerokością widełek w punktach bazowych: 2 000 znaczy ±20 %.
/// Losowanie idzie strumieniem [`StreamId::InvestorNoise`] z kluczem
/// `(indeks inwestora, tick doby)`, więc ten sam inwestor tego samego dnia ma jedno
/// zdanie o firmie — a nie tyle zdań, ile razy go zapytamy.
#[must_use]
pub fn belief(
    fundamental: Money,
    noise_bp: u16,
    rumor_bp: i16,
    seed: u64,
    investor_index: u32,
    firm_key: u64,
    t: Tick,
) -> Money {
    if fundamental.get() <= 0 {
        return Money::ZERO;
    }
    // Zero znaczy **brak losowania**, a nie „losuj z zerowym rozrzutem": dzięki temu
    // test mierzący sam kanał (publikacja albo plotka) nie musi odsiewać szumu
    // z wyniku, a świat bez różnicy zdań jest opisywalny jednym zerem w danych.
    let odchyl = if noise_bp == 0 {
        0
    } else {
        // Klucz miesza firmę z inwestorem: bez tego ten sam inwestor miałby w danej
        // dobie identyczne odchylenie na każdej spółce, więc jego portfel byłby
        // jedną decyzją.
        let klucz = investor_index ^ (magnat_core::mix64(firm_key) as u32);
        let mut r = rng(seed, StreamId::InvestorNoise, klucz, t);
        let rozrzut = i64::from(noise_bp);
        i64::from(r.gen_range_u32((2 * rozrzut + 1) as u32)) - rozrzut
    };
    let mnoznik = 10_000 + odchyl + i64::from(rumor_bp);
    let calosc = fundamental.get().saturating_mul(mnoznik.max(100)) / 10_000;
    // Cena jednej bp. Dzielenie całkowite w dół jest tu właściwe: przekonanie
    // zaokrąglone w górę kazałoby inwestorowi licytować grosz więcej, niż uważa.
    Money(calosc / i64::from(super::WHOLE_BP))
}

/// Publikowany zysk: prawdziwy plus szum zależny od jakości raportowania.
///
/// Szum jest **symetryczny** i to jest decyzja: firma, która zawsze zaniża,
/// byłaby firmą o innej wycenie, a nie firmą gorzej raportującą — a takiej różnicy
/// gracz nie ma jak odróżnić od zwykłej słabszej spółki.
#[must_use]
pub fn reported(profit: Money, noise_bp: u16, seed: u64, site_index: u32, t: Tick) -> Money {
    if noise_bp == 0 {
        return profit;
    }
    let mut r = rng(seed, StreamId::EarningsNoise, site_index, t);
    let rozrzut = i64::from(noise_bp);
    let odchyl = i64::from(r.gen_range_u32((2 * rozrzut + 1) as u32)) - rozrzut;
    Money(profit.get().saturating_mul(10_000 + odchyl) / 10_000)
}

/// Czy w tej dobie wypada publikacja wyniku miesiąca `month`.
///
/// Miesiąc `m` kończy się w dobie `(m + 1) × 30`, a publikacja wypada 45 dób
/// później. Funkcja czysta od numeru doby — nie ma tu kalendarza ani stanu.
///
/// Prywatna z rozmysłu: symulacja pyta [`published_month`] („co dziś publikujemy"),
/// a ta strona równania służy wyłącznie testowi, który sprawdza, że obie mówią
/// to samo. Publiczna byłaby drugim wejściem do tej samej wiedzy.
#[cfg(test)]
#[must_use]
fn publishes_on(day: u32, month: u32) -> bool {
    day == (month + 1) * 30 + PUBLISH_LAG_DAYS
}

/// Który miesiąc publikuje się w tej dobie, jeśli w ogóle.
#[must_use]
pub fn published_month(day: u32) -> Option<u32> {
    let po = day.checked_sub(PUBLISH_LAG_DAYS)?;
    if po % 30 != 0 || po < 30 {
        return None;
    }
    Some(po / 30 - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publikacja_wypada_czterdziesci_piec_dob_po_miesiacu() {
        // Miesiąc 0 kończy się w dobie 30, publikacja w dobie 75.
        assert!(publishes_on(75, 0));
        assert_eq!(published_month(75), Some(0));
        assert_eq!(published_month(105), Some(1));
        assert_eq!(published_month(74), None);
        assert_eq!(published_month(76), None);
        for d in 0..400u32 {
            match published_month(d) {
                Some(m) => assert!(publishes_on(d, m), "doba {d}"),
                None => {
                    for m in 0..13u32 {
                        assert!(!publishes_on(d, m), "doba {d}, miesiąc {m}");
                    }
                }
            }
        }
    }

    #[test]
    fn mnoznik_zysku_jest_odwrotnoscia_stopy_w_widelkach() {
        assert_eq!(price_earnings(500, 5, 25), 20);
        assert_eq!(price_earnings(2_000, 5, 25), 5);
        assert_eq!(
            price_earnings(1, 5, 25),
            25,
            "stopa bliska zeru nie daje nieskończoności"
        );
        assert_eq!(price_earnings(100_000, 5, 25), 5);
    }

    #[test]
    fn przekonanie_jest_funkcja_doby_a_nie_kolejnosci_pytania() {
        let p = Published {
            month: 3,
            profit_12m: Money(12_000_000),
            book: Money(4_000_000),
        };
        let f = fundamental(&p, 20);
        assert_eq!(f, Money(240_000_000));
        let a = belief(f, 2_000, 0, 7, 11, 5, Tick(1_000));
        let b = belief(f, 2_000, 0, 7, 11, 5, Tick(1_000));
        assert_eq!(
            a, b,
            "to samo pytanie w tej samej dobie ma tę samą odpowiedź"
        );
        let c = belief(f, 2_000, 0, 7, 12, 5, Tick(1_000));
        assert_ne!(
            a, c,
            "dwóch inwestorów nie może mieć zawsze tego samego zdania"
        );
    }

    #[test]
    fn plotka_przesuwa_przekonanie_w_swoja_strone() {
        let p = Published {
            month: 3,
            profit_12m: Money(12_000_000),
            book: Money(0),
        };
        let f = fundamental(&p, 20);
        // Bez szumu inwestora widać sam skutek plotki.
        let bez = belief(f, 1, 0, 7, 11, 5, Tick(1_000));
        let z = belief(f, 1, 1_000, 7, 11, 5, Tick(1_000));
        assert!(z > bez, "{z:?} vs {bez:?}");
    }

    #[test]
    fn wycena_bierze_wieksza_z_dwoch_liczb() {
        let strata = Published {
            month: 1,
            profit_12m: Money(-5_000_000),
            book: Money(9_000_000),
        };
        assert_eq!(fundamental(&strata, 20), Money(9_000_000));
        let bez_majatku = Published {
            month: 1,
            profit_12m: Money(1_000_000),
            book: Money(0),
        };
        assert_eq!(fundamental(&bez_majatku, 20), Money(20_000_000));
    }
}
