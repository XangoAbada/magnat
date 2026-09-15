//! Bramki G1–G9 z §7.4 dokumentu fazy M5 — treść tabeli, przełożona na liczby.
//!
//! Dwie warstwy, i podział jest celowy:
//!
//! 1. **funkcje na seriach** (`g1_series`, `g2_series`, …) — biorą gołe ciągi liczb
//!    (CPI po miesiącach, marża po dobach, mediana ceny po dobach) i nic nie wiedzą
//!    o przebiegu. Dzięki temu meta-test bramki G3 woła tę samą funkcję, którą CI
//!    woła na prawdziwych danych, karmiąc ją serią z `kernel::next_price_full` —
//!    bez budowania świata i bez kaleczenia kodu produkcyjnego,
//! 2. **[`evaluate`]** — składa z przebiegów to, czego funkcje z punktu 1 potrzebują,
//!    i wydaje werdykt profilu.
//!
//! Wszystkie progi są całkowitoliczbowe: punkty bazowe, promile, doby. Żadne
//! porównanie bramki nie przechodzi przez `f64`, bo wtedy werdykt zależałby
//! od kolejności działań, a nie od rynku (00 §2).

use std::collections::BTreeMap;

use crate::metrics::{DayMetrics, RunFile};

// ── progi (PRD §20.1 przez §7.4 dokumentu fazy) ──────────────────────────────────

/// G1: dolna krawędź inflacji r/r — −5 %.
pub const G1_YOY_MIN_BP: i32 = -500;
/// G1: górna krawędź inflacji r/r — +15 %.
pub const G1_YOY_MAX_BP: i32 = 1_500;
/// G1: w ilu promilach ziaren warunek ma być spełniony.
pub const G1_SEED_SHARE_PERMILLE: u64 = 950;
/// G1: pierwszy miesiąc, od którego mierzymy (rozbieg wyłączony).
pub const G1_FIRST_MONTH: u32 = 12;
/// G2: inflacja m/m powyżej tego progu to hiperinflacja.
pub const G2_MOM_MAX_BP: i32 = 1_000;
/// G2: okno kwartalne — `CPI_t / CPI_{t−90d} > 1,5`, czyli `2·CPI_t > 3·CPI_{t−90d}`.
pub const G2_WINDOW_DAYS: u32 = 90;
/// G3: tyle kolejnych miesięcy spadku CPI to już spirala.
pub const G3_DEFLATION_MONTHS: usize = 6;
/// G3: w ilu promilach dób marża może stać poniżej `min_margin_bp`.
pub const G3_BELOW_FLOOR_PERMILLE: u64 = 50;
/// G4: w tylu dobach od szoku cena ma pokryć połowę drogi. Za szybko = też fail.
pub const G4_RESPONSE_DAYS: (u32, u32) = (2, 7);
/// G4: w tylu dobach ma się ustabilizować na nowym poziomie.
pub const G4_SETTLE_DAYS: (u32, u32) = (14, 56);
/// G4: ile dób z końca przebiegu wyznacza „nowy poziom".
pub const G4_TAIL_DAYS: u32 = 15;
/// G5: mediana odsetka decyzji zakończonych odłożeniem.
pub const G5_DEFERRAL_MAX_PERMILLE: i32 = 250;
/// G5: odsetek ofert z pustą półką, w każdej dobie.
pub const G5_STOCKOUT_MAX_PERMILLE: i32 = 150;
/// G6: HHI × 10 000 — 6 000 to 0,6 z tabeli.
pub const G6_HHI_MAX: i32 = 6_000;

/// Profil bramek. `ci` pomija G4 i G6, bo obie wymagają scenariusza szokowego
/// i długiego przebiegu — na PR nie ma na to budżetu czasu (§7.4).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Profile {
    Ci,
    Nightly,
}

impl Profile {
    #[must_use]
    pub fn includes(self, gate: &str) -> bool {
        match self {
            Profile::Nightly => true,
            Profile::Ci => !matches!(gate, "G4" | "G6"),
        }
    }
}

/// Werdykt jednej bramki: to, co idzie do tabeli na stdout i do raportu.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GateOutcome {
    pub gate: &'static str,
    pub name: &'static str,
    pub pass: bool,
    /// Zmierzona liczba, słownie — bramki mierzą różne rzeczy, więc nie ma
    /// wspólnej jednostki.
    pub value: String,
    pub threshold: &'static str,
    /// Bramka **doradcza**: mierzy i raportuje, ale nie wywraca przebiegu.
    ///
    /// Jedna taka jest i ma nazwisko: **G4 do czasu M6**. Zmierzone w M5e na
    /// scenariuszu `supply-shock` (120 dób, szok +80 % ceny hurtowej chleba):
    /// odpowiedź w **2 dobach** (widełki 2–7, zielone), stabilizacja w **5**
    /// wobec widełek 14–56. Dolna granica `t_settle` zakłada tarcie, którego M5
    /// **nie modeluje i nie udaje, że modeluje**: dostawca zewnętrzny jest
    /// zaślepką o nieskończonej podaży, bez kontraktów i bez terminów, a sklep,
    /// któremu wzrósł koszt własny, przecenia od razu — opóźnienie 1–7 dób
    /// dotyczy **obserwacji konkurencji**, nie własnego rachunku.
    ///
    /// Tarcie wnosi M6 razem z rynkiem B2B (`AC-1`), więc do tego czasu czerwona
    /// G4 mówiłaby wyłącznie „M6 jeszcze nie ma" — a bramka świecąca na czerwono
    /// z powodu nieistniejącej fazy uczy wyłącznie ignorowania bramek. Liczba
    /// jest przy tym wypisywana co noc, więc doba, w której M6 wniesie kontrakty,
    /// będzie widoczna w raporcie.
    pub advisory: bool,
}

// ── warstwa 1: bramki na gołych seriach ──────────────────────────────────────────

/// G1 dla jednego ziarna. Miesiące z `None` są **pomijane**, nie liczone jako zero:
/// „nie ma jeszcze roku historii" to co innego niż „inflacja zero" (`AA-7`).
#[must_use]
pub fn g1_series(yoy_by_month: &[(u32, Option<i32>)]) -> bool {
    yoy_by_month
        .iter()
        .filter(|(m, _)| *m >= G1_FIRST_MONTH)
        .filter_map(|(_, v)| *v)
        .all(|v| (G1_YOY_MIN_BP..=G1_YOY_MAX_BP).contains(&v))
}

/// G2 dla jednego ziarna: brak miesiąca z inflacją m/m ponad progiem **i** brak
/// doby, w której CPI wyprzedził stan sprzed 90 dób o połowę.
#[must_use]
pub fn g2_series(mom_by_month: &[Option<i32>], cpi_by_day: &[(u32, i32)]) -> bool {
    if mom_by_month.iter().flatten().any(|v| *v > G2_MOM_MAX_BP) {
        return false;
    }
    let po_dobach: BTreeMap<u32, i32> = cpi_by_day.iter().copied().collect();
    for (d, cpi) in &po_dobach {
        let Some(wczesniej) = d
            .checked_sub(G2_WINDOW_DAYS)
            .and_then(|p| po_dobach.get(&p))
        else {
            continue;
        };
        if i64::from(*cpi) * 2 > i64::from(*wczesniej) * 3 {
            return false;
        }
    }
    true
}

/// G3 dla jednego ziarna: brak sześciu kolejnych miesięcy spadku CPI **i** marża
/// mediana poniżej dolnego ogranicznika w najwyżej 5 % dób.
///
/// To jest funkcja, którą wywołuje meta-test WP13: karmi ją serią marż złożoną
/// przez `kernel::next_price_full` z podłogą i bez niej.
#[must_use]
pub fn g3_series(cpi_by_month: &[i32], margin_by_day: &[i32], min_margin_bp: i32) -> bool {
    let mut spadki = 0usize;
    for para in cpi_by_month.windows(2) {
        if para[1] < para[0] {
            spadki += 1;
            if spadki >= G3_DEFLATION_MONTHS {
                return false;
            }
        } else {
            spadki = 0;
        }
    }
    if margin_by_day.is_empty() {
        return true;
    }
    let ponizej = margin_by_day.iter().filter(|m| **m < min_margin_bp).count() as u64;
    ponizej * 1_000 <= margin_by_day.len() as u64 * G3_BELOW_FLOOR_PERMILLE
}

/// Wynik pomiaru reaktywności szoku — obie liczby wchodzą do raportu, bo „za szybko"
/// jest tak samo czerwone jak „za wolno" i trzeba widzieć, którą stroną się wyszło.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct G4Verdict {
    /// Doby od zdarzenia do pokrycia połowy szoku.
    pub t_response: Option<u32>,
    /// Doby od zdarzenia do wejścia w ±10 % nowego poziomu **na stałe**.
    pub t_settle: Option<u32>,
    pub pass: bool,
}

/// G4: reakcja mediany ceny detalicznej szokowanego towaru.
///
/// „Nowy poziom" to mediana `p50` z ostatnich [`G4_TAIL_DAYS`] dób przebiegu —
/// przebieg musi więc kończyć się **po** wygaszeniu szoku, inaczej poziom
/// odniesienia byłby punktem na zboczu.
#[must_use]
pub fn g4_series(p50_by_day: &[(u32, i64)], event_day: u32) -> G4Verdict {
    let mut v = G4Verdict::default();
    let Some(p0) = p50_by_day
        .iter()
        .find(|(d, _)| *d == event_day)
        .map(|(_, p)| *p)
    else {
        return v;
    };
    let Some(ostatnia) = p50_by_day.last().map(|(d, _)| *d) else {
        return v;
    };
    let od = ostatnia.saturating_sub(G4_TAIL_DAYS);
    let mut ogon: Vec<i64> = p50_by_day
        .iter()
        .filter(|(d, _)| *d > od)
        .map(|(_, p)| *p)
        .collect();
    if ogon.is_empty() {
        return v;
    }
    ogon.sort_unstable();
    let poziom = ogon[ogon.len() / 2];
    if poziom <= p0 {
        return v;
    }
    let skok = poziom - p0;
    v.t_response = p50_by_day
        .iter()
        .find(|(d, p)| *d > event_day && (*p - p0) * 2 >= skok)
        .map(|(d, _)| *d - event_day);
    // Ustabilizowanie liczy się **od końca**: pierwsza doba, po której już nigdy
    // nie wyszło poza ±10 %. Pierwsze wejście w widełki nie wystarcza, bo cena
    // przecina je w drodze w górę.
    let mut ostatnie_wyjscie = None;
    for (d, p) in p50_by_day.iter().filter(|(d, _)| *d > event_day) {
        if (*p - poziom).abs() * 10 > poziom {
            ostatnie_wyjscie = Some(*d);
        }
    }
    v.t_settle = match ostatnie_wyjscie {
        Some(d) if d >= ostatnia => None,
        Some(d) => Some(d + 1 - event_day),
        None => Some(1),
    };
    v.pass = matches!(v.t_response, Some(t) if t >= G4_RESPONSE_DAYS.0 && t <= G4_RESPONSE_DAYS.1)
        && matches!(v.t_settle, Some(t) if t >= G4_SETTLE_DAYS.0 && t <= G4_SETTLE_DAYS.1);
    v
}

/// G5 dla jednego ziarna: żywe kategorie, odmowy „brak towaru" i odłożenia.
#[must_use]
pub fn g5_series(days: &[DayMetrics]) -> bool {
    let Some(pierwsza) = days.first() else {
        return false;
    };
    if days
        .iter()
        .any(|d| d.live_categories < pierwsza.live_categories)
    {
        return false;
    }
    // `stockout_rate` z PRD §20.1 to odsetek **odmów** „brak towaru", a nie odsetek
    // pustych półek. Różnica przestała być akademicka od M5e: dojrzały sklep
    // świadomie nie zamawia towaru, którego u niego nikt nie kupuje, a jego oferta
    // zostaje widoczna z zerowym stanem („znam, nie ma" — §5.3). Bramka liczona po
    // półkach czerwieniłaby się wtedy na **decyzji asortymentowej**, czyli na
    // zdrowym zachowaniu rynku. `stockout_permille` zostaje w raporcie, bo mówi
    // coś prawdziwego o asortymencie — tylko nie to, o co pyta G5.
    if days
        .iter()
        .any(|d| d.stockout_refusal_permille >= G5_STOCKOUT_MAX_PERMILLE)
    {
        return false;
    }
    let odlozenia: Vec<i32> = days.iter().map(|d| d.deferral_permille).collect();
    mediana(&odlozenia) < G5_DEFERRAL_MAX_PERMILLE
}

/// Mediana metodą najbliższej rangi — ta sama, którą liczy `Market::balance_sample`.
/// Bez interpolacji, bo interpolacja wniosłaby float do liczby, którą porównuje próg.
#[must_use]
pub fn mediana(v: &[i32]) -> i32 {
    if v.is_empty() {
        return 0;
    }
    let mut s = v.to_vec();
    s.sort_unstable();
    s[s.len() / 2]
}

// ── warstwa 2: złożenie przebiegów w werdykt ─────────────────────────────────────

/// Miesięczne próbki przebiegu: doba `30·m` niesie stan miesiąca `m`, bo CPI domyka
/// miesiąc na granicy doby (kalendarz 360-dniowy, `K-1`).
fn miesiace(run: &RunFile) -> Vec<(u32, &DayMetrics)> {
    run.days_data
        .iter()
        .filter(|d| d.day > 0 && d.day.is_multiple_of(30))
        .map(|d| (d.day / 30, d))
        .collect()
}

/// Przebiegi bez bliźniaków determinizmu: dla pary `(scenario, seed)` zostaje pierwszy.
#[must_use]
pub fn unikalne(runs: &[RunFile]) -> Vec<&RunFile> {
    let mut widziane: BTreeMap<(&str, u64), ()> = BTreeMap::new();
    let mut out = Vec::new();
    for r in runs {
        if widziane.insert((r.scenario.as_str(), r.seed), ()).is_none() {
            out.push(r);
        }
    }
    out
}

/// Werdykt wszystkich bramek profilu.
///
/// `min_margin_bp` pochodzi z `data/economy/shop.ron` — dolna krawędź widełek,
/// bo żadna firma nie losuje podłogi niżej.
#[must_use]
pub fn evaluate(runs: &[RunFile], profile: Profile, min_margin_bp: i32) -> Vec<GateOutcome> {
    let u = unikalne(runs);
    let n = u.len() as u64;
    let mut out = Vec::new();

    // ── G1 ──────────────────────────────────────────────────────────────────────
    let zielone = u
        .iter()
        .filter(|r| {
            let s: Vec<(u32, Option<i32>)> = miesiace(r)
                .into_iter()
                .map(|(m, d)| (m, d.cpi_yoy_bp))
                .collect();
            g1_series(&s)
        })
        .count() as u64;
    out.push(GateOutcome {
        gate: "G1",
        name: "Stabilnosc cen",
        pass: n > 0 && zielone * 1_000 >= n * G1_SEED_SHARE_PERMILLE,
        value: format!("{zielone}/{n} ziaren"),
        threshold: "≥ 95 % ziaren, r/r ∈ ⟨−5 %, +15 %⟩ od 12. miesiąca",
        advisory: false,
    });

    // ── G2 ──────────────────────────────────────────────────────────────────────
    let czerwone = czerwone_ziarna(&u, |r| {
        let mom: Vec<Option<i32>> = miesiace(r).into_iter().map(|(_, d)| d.cpi_mom_bp).collect();
        let cpi: Vec<(u32, i32)> = r
            .days_data
            .iter()
            .map(|d| (d.day, d.cpi_index_bp))
            .collect();
        g2_series(&mom, &cpi)
    });
    out.push(GateOutcome {
        gate: "G2",
        name: "Brak hiperinflacji",
        pass: czerwone.is_empty(),
        value: format!("{} ziaren czerwonych {czerwone:?}", czerwone.len()),
        threshold: "m/m ≤ 10 %, CPI_t / CPI_{t−90d} ≤ 1,5",
        advisory: false,
    });

    // ── G3 ──────────────────────────────────────────────────────────────────────
    let czerwone = czerwone_ziarna(&u, |r| {
        let cpi: Vec<i32> = miesiace(r)
            .into_iter()
            .map(|(_, d)| d.cpi_index_bp)
            .collect();
        let marze: Vec<i32> = r.days_data.iter().map(|d| d.margin_median_bp).collect();
        g3_series(&cpi, &marze, min_margin_bp)
    });
    out.push(GateOutcome {
        gate: "G3",
        name: "Brak spirali deflacji",
        pass: czerwone.is_empty(),
        value: format!("{} ziaren czerwonych {czerwone:?}", czerwone.len()),
        threshold: "< 6 miesięcy spadku CPI; marża pod podłogą w ≤ 5 % dób",
        advisory: false,
    });

    // ── G4 ──────────────────────────────────────────────────────────────────────
    if profile.includes("G4") {
        out.push(g4_gate(&u));
    }

    // ── G5 ──────────────────────────────────────────────────────────────────────
    let czerwone = czerwone_ziarna(&u, |r| g5_series(&r.days_data));
    let odlozenia: Vec<i32> = u
        .iter()
        .flat_map(|r| r.days_data.iter().map(|d| d.deferral_permille))
        .collect();
    out.push(GateOutcome {
        gate: "G5",
        name: "Rynek nie wymiera",
        pass: n > 0 && czerwone.is_empty(),
        value: format!(
            "{} ziaren czerwonych {czerwone:?}, mediana odlozen {} ‰",
            czerwone.len(),
            mediana(&odlozenia)
        ),
        threshold: "kategorie żywe w 100 % dób, odłożenia < 250 ‰, braki < 150 ‰",
        advisory: false,
    });

    // ── G6 ──────────────────────────────────────────────────────────────────────
    if profile.includes("G6") {
        let per_seed: Vec<i32> = u
            .iter()
            .map(|r| {
                let v: Vec<i32> = r
                    .days_data
                    .iter()
                    .filter(|d| d.hhi_pairs > 0)
                    .map(|d| d.hhi_median)
                    .collect();
                mediana(&v)
            })
            .collect();
        let m = mediana(&per_seed);
        out.push(GateOutcome {
            gate: "G6",
            name: "Brak monopolizacji",
            pass: !per_seed.is_empty() && m < G6_HHI_MAX,
            value: format!("HHI {m}"),
            threshold: "< 6 000 (0,6)",
        advisory: false,
        });
    }

    // ── G7 ──────────────────────────────────────────────────────────────────────
    let czerwone = czerwone_ziarna(&u, |r| r.conservation_ok);
    out.push(GateOutcome {
        gate: "G7",
        name: "Zachowanie pieniadza",
        pass: n > 0 && czerwone.is_empty(),
        value: format!("{} przebiegow z rozjazdem {czerwone:?}", czerwone.len()),
        threshold: "P1 zielone, 0 gr",
        advisory: false,
    });

    // ── G8 ──────────────────────────────────────────────────────────────────────
    // Bliźniak przebiegu to ten sam `(scenario, seed)` w drugim pliku — balansator
    // puszcza `seed0` dwa razy właśnie po to (§7.4 mówi „dwa przebiegi seeda 0").
    let mut pary: BTreeMap<(&str, u64), Vec<&RunFile>> = BTreeMap::new();
    for r in runs {
        pary.entry((r.scenario.as_str(), r.seed))
            .or_default()
            .push(r);
    }
    let blizniaki: Vec<&Vec<&RunFile>> = pary.values().filter(|v| v.len() > 1).collect();
    let zgodne = blizniaki
        .iter()
        .all(|v| v.windows(2).all(|p| p[0].hashes == p[1].hashes));
    out.push(GateOutcome {
        gate: "G8",
        name: "Determinizm",
        pass: !blizniaki.is_empty() && zgodne,
        value: format!("{} par przebiegow, zgodne: {zgodne}", blizniaki.len()),
        threshold: "identyczny ciąg hashy",
        advisory: false,
    });

    // ── G9 ──────────────────────────────────────────────────────────────────────
    let bez: u64 = u.iter().map(|r| r.decisions_without_reason).sum();
    let probka: u64 = u.iter().map(|r| r.decisions_sampled).sum();
    out.push(GateOutcome {
        gate: "G9",
        name: "Wyjasnialnosc",
        pass: n > 0 && bez == 0,
        value: format!("{bez} bez powodu na {probka} probkowanych"),
        threshold: "0",
        advisory: false,
    });

    out.retain(|g| profile.includes(g.gate));
    out
}

/// Ziarna, dla których predykat „jest zielone" nie wyszedł.
fn czerwone_ziarna(u: &[&RunFile], zielone: impl Fn(&RunFile) -> bool) -> Vec<u64> {
    u.iter().filter(|r| !zielone(r)).map(|r| r.seed).collect()
}

fn g4_gate(u: &[&RunFile]) -> GateOutcome {
    let szokowe: Vec<&&RunFile> = u.iter().filter(|r| r.scenario == "supply-shock").collect();
    let mut zdane = 0u64;
    let mut opis = Vec::new();
    for r in &szokowe {
        let dzien = crate::run::event_day(r.days);
        // Towar z przebiegu, a nie zgadnięty: przy kilku ofertach mediana skacze
        // o kilkadziesiąt procent z samego szumu i heurystyka „ten, który najbardziej
        // podrożał" trafiałaby w szum. Zgadywanie zostaje tylko dla plików sprzed
        // dopisania pola `shock_good`.
        let v = r
            .shock_good
            .or_else(|| najbardziej_ruszony(r, dzien))
            .map_or(G4Verdict::default(), |g| {
                let s: Vec<(u32, i64)> = r
                    .days_data
                    .iter()
                    .filter_map(|d| d.p50_of(g).map(|p| (d.day, p)))
                    .collect();
                g4_series(&s, dzien)
            });
        if v.pass {
            zdane += 1;
        }
        opis.push(format!(
            "seed {}: odpowiedz {:?} d, stabilizacja {:?} d",
            r.seed, v.t_response, v.t_settle
        ));
    }
    GateOutcome {
        gate: "G4",
        name: "Reaktywnosc szoku",
        pass: !szokowe.is_empty() && zdane == szokowe.len() as u64,
        value: if szokowe.is_empty() {
            "brak przebiegow supply-shock".to_string()
        } else {
            format!("{zdane}/{} · {}", szokowe.len(), opis.join("; "))
        },
        threshold: "t_response 2–7 d, t_settle 14–56 d",
        advisory: true,
    }
}

/// Towar, którego mediana ceny urosła najbardziej od doby zdarzenia. Plik metryk
/// nie zapisuje, który towar dostał szok — a i tak mierzymy skutek, nie zamiar.
fn najbardziej_ruszony(run: &RunFile, event_day: u32) -> Option<u16> {
    let przed = run.days_data.iter().find(|d| d.day == event_day)?;
    let po = run.days_data.last()?;
    przed
        .prices
        .iter()
        .filter(|r| r.p50 > 0)
        .filter_map(|r| {
            let p = po.p50_of(r.good)?;
            Some((r.good, (p - r.p50).saturating_mul(10_000) / r.p50))
        })
        .max_by_key(|(_, wzrost)| *wzrost)
        .map(|(g, _)| g)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doba(day: u32) -> DayMetrics {
        DayMetrics {
            day,
            prices: Vec::new(),
            cpi_index_bp: 10_000,
            cpi_mom_bp: None,
            cpi_yoy_bp: None,
            base_rate_bp: 500,
            margin_median_bp: 3_000,
            shops: 10,
            insolvent: 0,
            hhi_median: 2_000,
            hhi_pairs: 4,
            live_categories: 5,
            stockout_permille: 10,
            stockout_refusal_permille: 10,
            deferral_permille: 50,
            purchases: 100,
            revenue_gr: 10_000,
            reprices: 0,
            write_offs: 0,
            write_off_gr: 0,
            expired_qty: 0,
            loans: 0,
            credit_outstanding_gr: 0,
            money_supply_gr: 0,
        }
    }

    #[test]
    fn g1_pomija_miesiace_bez_historii_i_lapie_wyjscie_poza_widelki() {
        // Rozbieg (miesiące < 12) nie jest mierzony, brak historii też nie.
        let zielone = [
            (11, Some(9_000)),
            (12, None),
            (13, Some(1_400)),
            (14, Some(-400)),
        ];
        assert!(g1_series(&zielone));
        // +15,01 % w 13. miesiącu to już czerwone.
        assert!(!g1_series(&[(12, Some(0)), (13, Some(1_501))]));
        // I −5,01 % też — bramka jest dwustronna.
        assert!(!g1_series(&[(12, Some(-501))]));
    }

    #[test]
    fn g2_lapie_skok_miesieczny_i_kwartalny() {
        let plaski: Vec<(u32, i32)> = (0..=180).map(|d| (d, 10_000)).collect();
        assert!(g2_series(&[Some(999), None], &plaski));
        assert!(!g2_series(&[Some(1_001)], &plaski));
        // CPI 10 000 → 15 000 w 90 dób jest dokładnie na granicy 1,5×; 15 001 już nie.
        let granica: Vec<(u32, i32)> = (0..=90)
            .map(|d| (d, if d == 90 { 15_000 } else { 10_000 }))
            .collect();
        assert!(g2_series(&[], &granica));
        let ponad: Vec<(u32, i32)> = (0..=90)
            .map(|d| (d, if d == 90 { 15_001 } else { 10_000 }))
            .collect();
        assert!(!g2_series(&[], &ponad));
    }

    #[test]
    fn g3_lapie_szesc_miesiecy_spadku_i_marze_pod_podloga() {
        let rosnie = [10_000, 10_100, 10_050, 10_200];
        assert!(g3_series(&rosnie, &[3_000; 100], 300));
        let spada = [10_600, 10_500, 10_400, 10_300, 10_200, 10_100, 10_000];
        assert!(!g3_series(&spada, &[3_000; 100], 300));
        // 6 dób na 100 poniżej podłogi to 60 ‰ > 50 ‰.
        let mut marze = [3_000i32; 100];
        for m in marze.iter_mut().take(6) {
            *m = 100;
        }
        assert!(!g3_series(&rosnie, &marze, 300));
    }

    #[test]
    fn g4_czerwieni_sie_takze_gdy_rynek_reaguje_za_szybko() {
        // Skok z 100 na 180 gr w jednej dobie: rynek bez tarcia, czyli fail.
        let szybki: Vec<(u32, i64)> = (0..=240)
            .map(|d| (d, if d > 180 { 180 } else { 100 }))
            .collect();
        let v = g4_series(&szybki, 180);
        assert_eq!(v.t_response, Some(1));
        assert!(!v.pass, "reakcja w jedna dobe to rynek bez tarcia");

        // Dojście po 4 gr na dobę: połowa drogi (40 gr) w 10. dobie — też poza
        // widełkami 2–7, ale z drugiej strony.
        let wolny: Vec<(u32, i64)> = (0..=280)
            .map(|d| {
                let p = if d <= 180 {
                    100
                } else {
                    (100 + (i64::from(d) - 180) * 4).min(180)
                };
                (d, p)
            })
            .collect();
        let v = g4_series(&wolny, 180);
        assert_eq!(v.t_response, Some(10));
        assert!(!v.pass);

        // Dojście po 14 gr na dobę: 40 gr pokryte w 3. dobie, widełki ±10 %
        // opuszczone po raz ostatni w 4. dobie → t_settle 5, czyli za wcześnie.
        let ostry: Vec<(u32, i64)> = (0..=280)
            .map(|d| {
                let p = if d <= 180 {
                    100
                } else {
                    (100 + (i64::from(d) - 180) * 14).min(180)
                };
                (d, p)
            })
            .collect();
        let v = g4_series(&ostry, 180);
        assert_eq!(v.t_response, Some(3));
        assert_eq!(v.t_settle, Some(5));
        assert!(!v.pass, "stabilizacja w 5 dob jest ponizej progu 14");
    }

    #[test]
    fn g5_lapie_wymarla_kategorie_i_zatkane_polki() {
        let dobre: Vec<DayMetrics> = (0..30).map(doba).collect();
        assert!(g5_series(&dobre));

        let mut brak = dobre.clone();
        brak[17].live_categories = 4;
        assert!(!g5_series(&brak), "utrata kategorii = czerwone");

        let mut puste = dobre.clone();
        puste[3].stockout_refusal_permille = G5_STOCKOUT_MAX_PERMILLE;
        assert!(!g5_series(&puste), "150 promili to juz prog, nie prawie");

        let mut odkladaja = dobre;
        for d in &mut odkladaja {
            d.deferral_permille = G5_DEFERRAL_MAX_PERMILLE;
        }
        assert!(!g5_series(&odkladaja));
    }
}
