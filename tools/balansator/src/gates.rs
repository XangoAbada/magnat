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

use crate::metrics::{DayMetrics, LodSample, RunFile};

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
/// G10: o ile promili liczba firm może odejść od wartości startowej (§7.10: ±40 %).
///
/// To jest **kryterium ukończenia WP15** wyrażone liczbą: „liczba firm nie eksploduje
/// ani nie wymiera". Pasmo jest szerokie z rozmysłu — miasto ma prawo się przebudować
/// przez rok, nie ma prawa zniknąć ani spuchnąć dwukrotnie.
pub const G10_FIRM_BAND_PERMILLE: i64 = 400;
/// G11: pasmo bezrobocia w promilach siły roboczej (§7.10: 3–12 %).
pub const G11_UNEMPLOYMENT: (u16, u16) = (30, 120);
/// G12: mediana bezwzględnego odchylenia makro od mezo, w promilach (M10 §7.3 `K2`:
/// 0,5 % dla agregatów o n ≥ 500; miasto balansatora ma ich kilka tysięcy).
pub const G12_MEDIAN_DEV_MAX_PERMILLE: i64 = 50;
/// G12: dopuszczalne nachylenie regresji odchylenia, w **dziesiątych promila
/// na rok gry** (M10 §7.3 `K3`: ≤ 0,02 pp/rok, czyli 0,2 ‰/rok).
///
/// To jest **ważniejszy próg niż poprzedni** i po to ta bramka istnieje:
/// odchylenie 4 ‰ w losową stronę jest nieszkodliwe, a 4 ‰ co miesiąc w tę samą
/// stronę to po stu latach świat, który się rozpadł.
pub const G12_SLOPE_MAX_TENTHS_PERMILLE: i64 = 2;
/// G12: autokorelacja znaku odchylenia (lag 1), w setnych. Powyżej tego progu
/// odchylenie jest systematyczne, nawet gdy mieści się w medianie.
pub const G12_SIGN_AUTOCORR_MAX: i64 = 30;
/// G12: ile próbek miesięcznych musi mieć przebieg, żeby regresja cokolwiek
/// znaczyła. Poniżej tego bramka jest **doradcza**, a nie zielona.
pub const G12_MIN_MONTHS: usize = 6;
/// G12: o ile promili wolno się różnić w **dobie zero**, zanim uznamy, że obie
/// strony liczą co innego.
///
/// `lift()` kopiuje świat, więc w dobie zero odchylenie ma być zerem — pięć
/// promili to zapas na zaokrąglenia przy dzieleniu na komórki. Większe znaczy,
/// że pomiar porównuje dwie różne wielkości, a nie dwa modele, i wtedy bramka
/// **nie sądzi**: świeci doradczo i mówi, co zobaczyła. Ta reguła powstała
/// z własnego błędu — pierwsza wersja brała po stronie mezo `total_money`
/// i świeciła na czerwono z powodu własnej definicji, nie z powodu modelu.
pub const G12_DAY0_TOLERANCE_PERMILLE: i64 = 5;
/// G11: ile ostatnich dób przebiegu wyznacza stopę bezrobocia.
///
/// **Ogon, a nie całość po rozbiegu** — i to jest wniosek z pomiaru, nie ostrożność.
/// Siła robocza w przebiegu **rośnie**: miasto startuje z obsadą z generacji, a ludzi
/// dokłada migracja, więc przez pierwsze miesiące prawie każdy, kto już wszedł na
/// rynek, pracuje. Zmierzone na czterech ziarnach: mediana od 60. doby daje
/// 2,0–3,3 %, mediana ostatnich trzydziestu dób z tych samych przebiegów — 6,4–10,9 %.
/// Pierwsza liczba mówi, jak szybko miasto się zaludnia; druga o gospodarce.
///
/// To jest ten sam wybór, którym G4 wyznacza „nowy poziom" ceny po szoku
/// (`G4_TAIL_DAYS`) i z tego samego powodu.
pub const G11_TAIL_DAYS: usize = 30;
/// G11: krótszy przebieg nie ma ogona, który cokolwiek znaczy.
pub const G11_MIN_DAYS: u16 = 90;

/// Profil bramek. `ci` pomija G4 i G6, bo obie wymagają scenariusza szokowego
/// i długiego przebiegu — na PR nie ma na to budżetu czasu (§7.4).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Profile {
    Ci,
    Nightly,
}

impl Profile {
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Profile::Ci => "ci",
            Profile::Nightly => "nightly",
        }
    }

    #[must_use]
    pub fn includes(self, gate: &str) -> bool {
        match self {
            Profile::Nightly => true,
            // G11 wymaga przebiegu dłuższego niż ten z PR (`G11_MIN_DAYS`), bo
            // krótszy nie ma ogona, w którym miasto jest już zaludnione — tak samo
            // jak G4 i G6 mierzyłyby szum przy krótkim przebiegu.
            // G12 wymaga przebiegu z próbkami miesięcznymi: regresja na trzech
            // punktach mierzy szum, a nie dryf.
            Profile::Ci => !matches!(gate, "G4" | "G6" | "G11" | "G12"),
        }
    }

    /// Czy bramka może w tym profilu wyjść **bez danych** i nie wywrócić przebiegu.
    ///
    /// Lista jest z nazwy, a nie „każda pominięta" (N1.6): do E1 profil `ci`
    /// przepuszczał pominięcie dowolnej bramki, a G1 i G3 nie były nawet pomijane,
    /// tylko zielone na pustym zbiorze. W profilu `ci` bez danych mogą wyjść bramki,
    /// których profil nie bierze, oraz G1 (r/r od 12. miesiąca) i G3 (sześć miesięcy
    /// deflacji) — 120 dób to cztery miesiące. W biegu nocnym żadna (`D-N17`).
    #[must_use]
    pub fn dopuszcza_brak_danych(self, gate: &str) -> bool {
        match self {
            Profile::Nightly => false,
            Profile::Ci => !self.includes(gate) || matches!(gate, "G1" | "G3"),
        }
    }
}

/// Werdykt bramki. Cztery stany, bo trzy nie wystarczały (`R2-WP24`).
///
/// Do R2f bramka miała dwa pola typu `bool` — `pass` i `advisory` — i brakowało
/// w nich miejsca na jedyny stan, który naprawdę zdarzał się co noc: **bramka
/// nie została policzona**. Wyrażało się to wtedy jako `pass: false,
/// advisory: true`, czyli „czerwona, ale nie wywraca przebiegu", i to była
/// nieprawda o dwóch bramkach naraz: G11 nie była czerwona, tylko odfiltrowana,
/// a G4 nie była odfiltrowana, tylko czerwona z nazwanym powodem.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verdict {
    /// Zmierzona i w widełkach.
    Green,
    /// Zmierzona, poza widełkami — wywraca przebieg.
    Red,
    /// Zmierzona, poza widełkami, **nie** wywraca przebiegu. Powód jest wtedy
    /// zapisany przy bramce i ma nazwisko fazy, która go zdejmie.
    Advisory,
    /// **Nie została policzona.** Powód siedzi w `GateOutcome::value` i nigdy
    /// nie jest domysłem: albo profil jej nie bierze, albo dane nie mają tego,
    /// czego potrzebuje.
    Skipped,
}

impl Verdict {
    /// Słowo do tabeli na stdout i do raportu.
    #[must_use]
    pub fn slowo(self) -> &'static str {
        match self {
            Verdict::Green => "ZIELONE",
            Verdict::Red => "CZERWONE",
            Verdict::Advisory => "DORADCZE",
            Verdict::Skipped => "POMINIETE",
        }
    }
}

/// Werdykt jednej bramki: to, co idzie do tabeli na stdout i do raportu.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GateOutcome {
    pub gate: &'static str,
    pub name: &'static str,
    pub verdict: Verdict,
    /// Zmierzona liczba, słownie — bramki mierzą różne rzeczy, więc nie ma
    /// wspólnej jednostki. Przy `Verdict::Skipped` stoi tu **powód pominięcia**,
    /// bo bramka, która zniknęła z raportu bez słowa, jest bramką wyłączoną.
    pub value: String,
    pub threshold: &'static str,
}

impl GateOutcome {
    /// Zielona albo czerwona, zależnie od warunku — skrót dla ośmiu bramek,
    /// które nie mają trzeciego stanu.
    fn nowa(
        gate: &'static str,
        name: &'static str,
        ok: bool,
        value: String,
        threshold: &'static str,
    ) -> Self {
        Self {
            gate,
            name,
            verdict: if ok { Verdict::Green } else { Verdict::Red },
            value,
            threshold,
        }
    }

    /// Bramka, której nie było czym policzyć. Powód jest argumentem, a nie
    /// domysłem czytelnika.
    fn pominieta(
        gate: &'static str,
        name: &'static str,
        powod: String,
        threshold: &'static str,
    ) -> Self {
        Self {
            gate,
            name,
            verdict: Verdict::Skipped,
            value: powod,
            threshold,
        }
    }

    /// Czy ta bramka wywraca przebieg w danym profilu.
    ///
    /// Pominięcie wywraca **bieg nocny** i to jest wykonanie decyzji `D-N17`:
    /// filtr, który wyklucza bramkę zawsze, jest wyłączeniem bramki napisanym
    /// okrężnie. Jeśli przy pełnej macierzy nocnej bramka nie ma czego zmierzyć,
    /// to nie zmierzy nigdy i czekanie dziesięciu nocy niczego do tej wiedzy nie
    /// doda. W profilu `ci` pominięcie przechodzi tylko dla bramek, które profil
    /// wymienia z nazwy ([`Profile::dopuszcza_brak_danych`]).
    #[must_use]
    pub fn blokuje(&self, profil: Profile) -> bool {
        match self.verdict {
            Verdict::Red => true,
            Verdict::Skipped => !profil.dopuszcza_brak_danych(self.gate),
            Verdict::Green | Verdict::Advisory => false,
        }
    }

    /// Czy bramka jest zielona. Pominięta nie jest — i to jest cała różnica
    /// wobec stanu sprzed `R2-WP24`.
    #[must_use]
    pub fn pass(&self) -> bool {
        self.verdict == Verdict::Green
    }
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

/// Ile zamkniętych miesięcy ma najdłuższy przebieg.
fn najwiecej_miesiecy(u: &[&RunFile]) -> usize {
    u.iter().map(|r| miesiace(r).len()).max().unwrap_or(0)
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
    let mut out = Vec::new();
    detal(&u, min_margin_bp, &mut out);
    rzetelnosc(runs, &u, &mut out);
    firmy(&u, &mut out);
    out.push(lod(&u));
    // **Bramka spoza profilu zostaje w raporcie jako pominięta.** Do R2f znikała
    // z listy, więc profil `ci` wypisywał osiem wierszy i nikt nie wiedział, że
    // czterech brakuje — a bramka, o której raport milczy, jest bramką wyłączoną.
    for g in &mut out {
        if !profile.includes(g.gate) {
            g.verdict = Verdict::Skipped;
            g.value = format!("profil {} nie bierze tej bramki", profile.key());
        }
    }
    out
}

/// G1–G6: czy gospodarka detaliczna trzyma się w ryzach.
///
/// Sześć pytań o **ceny i rynek**: czy stoją, czy nie uciekają w górę, czy nie
/// spadają spiralą, czy reagują na szok, czy rynek żyje i czy nie zrósł się
/// w monopol.
fn detal(u: &[&RunFile], min_margin_bp: i32, out: &mut Vec<GateOutcome>) {
    let n = u.len() as u64;
    out.push(g1_gate(u));

    // ── G2 ──────────────────────────────────────────────────────────────────────
    let czerwone = czerwone_ziarna(u, |r| {
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
        verdict: if czerwone.is_empty() {
            Verdict::Green
        } else {
            Verdict::Red
        },
        value: format!("{} ziaren czerwonych {czerwone:?}", czerwone.len()),
        threshold: "m/m ≤ 10 %, CPI_t / CPI_{t−90d} ≤ 1,5",
    });

    out.push(g3_gate(u, min_margin_bp));
    // ── G4 ──────────────────────────────────────────────────────────────────────
    out.push(g4_gate(u));
    detal_rynek(u, n, out);
}

/// G1: inflacja r/r w widełkach od 12. miesiąca w ≥ 95 % ziaren.
fn g1_gate(u: &[&RunFile]) -> GateOutcome {
    let n = u.len() as u64;
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
    // Ziarno bez ani jednego miesiąca r/r od 12. nie ma czego zmierzyć, a `all()` na
    // pustym zbiorze dawało dla niego zieleń (N1.6). Jeśli takie są wszystkie ziarna,
    // bramka mówi „bez danych", zamiast udawać pomiar.
    let prog_g1 = "≥ 95 % ziaren, r/r ∈ ⟨−5 %, +15 %⟩ od 12. miesiąca";
    let zmierzone = u
        .iter()
        .filter(|r| {
            miesiace(r)
                .iter()
                .any(|(m, d)| *m >= G1_FIRST_MONTH && d.cpi_yoy_bp.is_some())
        })
        .count();
    if zmierzone == 0 {
        return GateOutcome::pominieta(
            "G1",
            "Stabilnosc cen",
            format!(
                "brak danych: najdłuższy przebieg ma {} mies., r/r liczy się od {G1_FIRST_MONTH}.",
                najwiecej_miesiecy(u)
            ),
            prog_g1,
        );
    }
    GateOutcome {
        gate: "G1",
        name: "Stabilnosc cen",
        verdict: if zielone * 1_000 >= n * G1_SEED_SHARE_PERMILLE {
            Verdict::Green
        } else {
            Verdict::Red
        },
        value: format!("{zielone}/{n} ziaren"),
        threshold: prog_g1,
    }
}

/// G3: brak sześciu miesięcy deflacji z rzędu i marża nad podłogą.
fn g3_gate(u: &[&RunFile], min_margin_bp: i32) -> GateOutcome {
    let czerwone = czerwone_ziarna(u, |r| {
        let cpi: Vec<i32> = miesiace(r)
            .into_iter()
            .map(|(_, d)| d.cpi_index_bp)
            .collect();
        let marze: Vec<i32> = r.days_data.iter().map(|d| d.margin_median_bp).collect();
        g3_series(&cpi, &marze, min_margin_bp)
    });
    // Połowa deflacyjna potrzebuje `G3_DEFLATION_MONTHS + 1` próbek miesięcznych. Przy
    // krótszym przebiegu nie może się zaczerwienić, więc zieleń nic by nie znaczyła
    // (N1.6). Czerwona marża zostaje czerwona — tę połowę da się zmierzyć zawsze.
    let prog_g3 = "< 6 miesięcy spadku CPI; marża pod podłogą w ≤ 5 % dób";
    let miesiecy = najwiecej_miesiecy(u);
    if czerwone.is_empty() && miesiecy <= G3_DEFLATION_MONTHS {
        return GateOutcome::pominieta(
            "G3",
            "Brak spirali deflacji",
            format!(
                "brak danych o deflacji: {miesiecy} mies. < {} (marża w normie)",
                G3_DEFLATION_MONTHS + 1
            ),
            prog_g3,
        );
    }
    GateOutcome {
        gate: "G3",
        name: "Brak spirali deflacji",
        verdict: if czerwone.is_empty() {
            Verdict::Green
        } else {
            Verdict::Red
        },
        value: format!("{} ziaren czerwonych {czerwone:?}", czerwone.len()),
        threshold: prog_g3,
    }
}

/// G5–G6: czy rynek żyje i czy nie zrósł się w monopol.
fn detal_rynek(u: &[&RunFile], n: u64, out: &mut Vec<GateOutcome>) {
    // ── G5 ──────────────────────────────────────────────────────────────────────
    let czerwone = czerwone_ziarna(u, |r| g5_series(&r.days_data));
    let odlozenia: Vec<i32> = u
        .iter()
        .flat_map(|r| r.days_data.iter().map(|d| d.deferral_permille))
        .collect();
    out.push(GateOutcome {
        gate: "G5",
        name: "Rynek nie wymiera",
        verdict: if n > 0 && czerwone.is_empty() {
            Verdict::Green
        } else {
            Verdict::Red
        },
        value: format!(
            "{} ziaren czerwonych {czerwone:?}, mediana odlozen {} ‰",
            czerwone.len(),
            mediana(&odlozenia)
        ),
        threshold: "kategorie żywe w 100 % dób, odłożenia < 250 ‰, braki < 150 ‰",
    });

    // ── G6 ──────────────────────────────────────────────────────────────────────
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
    if per_seed.is_empty() {
        out.push(GateOutcome::pominieta(
            "G6",
            "Brak monopolizacji",
            "brak przebiegu z parami do policzenia HHI".to_string(),
            "< 6 000 (0,6)",
        ));
    } else {
        let m = mediana(&per_seed);
        out.push(GateOutcome::nowa(
            "G6",
            "Brak monopolizacji",
            m < G6_HHI_MAX,
            format!("HHI {m}"),
            "< 6 000 (0,6)",
        ));
    }
}

/// G7–G9: czy przebieg w ogóle wolno czytać.
///
/// Trzy pytania nie o gospodarkę, tylko o **rzetelność pomiaru**: czy pieniądz się
/// zgadza, czy dwa przebiegi tego samego ziarna są identyczne i czy każda decyzja
/// ma powód. Czerwień którejkolwiek z nich unieważnia pozostałe bramki — dlatego
/// stoją osobno, a nie dlatego, że plik był długi.
///
/// `runs` obok `u`, bo G8 porównuje **bliźniaki**, czyli dokładnie te przebiegi,
/// które `unikalne` odsiewa.
fn rzetelnosc(runs: &[RunFile], u: &[&RunFile], out: &mut Vec<GateOutcome>) {
    let n = u.len() as u64;
    // ── G7 ──────────────────────────────────────────────────────────────────────
    let czerwone = czerwone_ziarna(u, |r| r.conservation_ok);
    out.push(GateOutcome {
        gate: "G7",
        name: "Zachowanie pieniadza",
        verdict: if n > 0 && czerwone.is_empty() {
            Verdict::Green
        } else {
            Verdict::Red
        },
        value: format!("{} przebiegow z rozjazdem {czerwone:?}", czerwone.len()),
        threshold: "P1 zielone, 0 gr",
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
        verdict: if !blizniaki.is_empty() && zgodne {
            Verdict::Green
        } else {
            Verdict::Red
        },
        value: format!("{} par przebiegow, zgodne: {zgodne}", blizniaki.len()),
        threshold: "identyczny ciąg hashy",
    });

    // ── G9 ──────────────────────────────────────────────────────────────────────
    let bez: u64 = u.iter().map(|r| r.decisions_without_reason).sum();
    let probka: u64 = u.iter().map(|r| r.decisions_sampled).sum();
    out.push(GateOutcome {
        gate: "G9",
        name: "Wyjasnialnosc",
        verdict: if n > 0 && bez == 0 {
            Verdict::Green
        } else {
            Verdict::Red
        },
        value: format!("{bez} bez powodu na {probka} probkowanych"),
        threshold: "0",
    });
}

/// G10–G11: czy warstwa firm nie wywraca miasta (M7f WP17).
///
/// Dwa pytania o **populację firm i rynek pracy**: czy firm nie przybywa ani nie
/// ubywa bez opamiętania i czy bezrobocie stoi w paśmie z §7.10.
fn firmy(u: &[&RunFile], out: &mut Vec<GateOutcome>) {
    // ── G10 ─────────────────────────────────────────────────────────────────────
    //
    // Populacja firm. Mierzona wobec **pierwszej doby przebiegu**, a nie wobec
    // liczby z planu: ile firm postawi generator, zależy od wielkości miasta,
    // a bramka ma mówić o gospodarce, nie o rozmiarze mapy.
    let czerwone = czerwone_ziarna(u, |r| {
        let Some(start) = r.days_data.first().map(|d| i64::from(d.firms)) else {
            return true;
        };
        if start == 0 {
            // Przebieg bez rejestru firm — bramka nie ma czego mierzyć i **nie
            // udaje, że zmierzyła**. Zielona, bo czerwień znaczyłaby „gospodarka
            // się zawaliła", a tu nie ma gospodarki firm w ogóle.
            return true;
        }
        r.days_data.iter().all(|d| {
            let teraz = i64::from(d.firms);
            (teraz - start).abs() * 1_000 <= start * G10_FIRM_BAND_PERMILLE
        })
    });
    out.push(GateOutcome {
        gate: "G10",
        name: "Populacja firm w pasmie",
        verdict: if czerwone.is_empty() {
            Verdict::Green
        } else {
            Verdict::Red
        },
        value: format!(
            "{} ziaren czerwonych {czerwone:?}; start/koniec: {}",
            czerwone.len(),
            u.iter()
                .map(|r| format!(
                    "{}→{}",
                    r.days_data.first().map_or(0, |d| d.firms),
                    r.days_data.last().map_or(0, |d| d.firms)
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        threshold: "liczba firm w ±40 % wartości startowej",
    });

    // ── G11 ─────────────────────────────────────────────────────────────────────
    //
    // **Do R2f ta bramka nie mogła zaświecić na czerwono i nikt tego nie sprawdził.**
    // Filtr „wakatów nie więcej niż ludzi w sile roboczej" wyłączał ją w każdym
    // przebiegu, w którym generator postawił więcej etatów niż mieszkańców — czyli
    // w każdym (poz. 6 wykazu `R2`: bezrobocie 0,2 % przy 12 032 pustych etatach).
    // Bramka nie zwracała wtedy „czerwone", tylko nie zwracała nic, a raport
    // profilu `ci` gubił ją całkiem, bo profil filtrował ją drugi raz.
    //
    // Gęstość etatów **zostaje w opisie**, bo jest prawdziwym wyjaśnieniem czerwieni
    // i adresatem naprawy (`R2-WP18` → R3, `D-N20`). Przestaje natomiast być
    // powodem, dla którego pomiaru nie ma: bezrobocie 0,2 % jest poza pasmem 3–12 %
    // niezależnie od tego, czyja to wina, a bramka ma mówić, co zmierzyła, a nie
    // kogo za to winić.
    //
    // Jedyne, co zostaje pominięciem, to **przebieg za krótki** (`G11_MIN_DAYS`):
    // tam nie ma ogona, z którego liczy się medianę, więc nie ma czego zmierzyć.
    let mierzalne: Vec<&&RunFile> = u.iter().filter(|r| r.days >= G11_MIN_DAYS).collect();
    let wakaty = |lista: &[&&RunFile]| {
        lista
            .iter()
            .map(|r| {
                let d = r.days_data.last();
                format!(
                    "{}/{}",
                    d.map_or(0, |x| x.vacancies),
                    d.map_or(0, |x| x.labour_force)
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    if mierzalne.is_empty() {
        out.push(GateOutcome::pominieta(
            "G11",
            "Bezrobocie w pasmie",
            format!(
                "przebieg krótszy niż {G11_MIN_DAYS} dób — ogon {G11_TAIL_DAYS} dób nie istnieje (dlugosci: {})",
                u.iter()
                    .map(|r| r.days.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            "mediana bezrobocia z ostatnich 30 dób ∈ ⟨3 %, 12 %⟩",
        ));
    } else {
        let czerwone: Vec<u64> = mierzalne
            .iter()
            .filter(|r| !g11_seed(r))
            .map(|r| r.seed)
            .collect();
        out.push(GateOutcome::nowa(
            "G11",
            "Bezrobocie w pasmie",
            czerwone.is_empty(),
            format!(
                "{} ziaren czerwonych {czerwone:?}; mediany: {}; wakaty/siła robocza: {}",
                czerwone.len(),
                mierzalne
                    .iter()
                    .map(|r| {
                        let m = mediana_u16(&ogon(r));
                        format!("{},{}%", m / 10, m % 10)
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
                wakaty(&mierzalne)
            ),
            "mediana bezrobocia z ostatnich 30 dób ∈ ⟨3 %, 12 %⟩",
        ));
    }
}

/// Bezrobocie z ogona przebiegu — patrz [`G11_TAIL_DAYS`].
fn ogon(r: &RunFile) -> Vec<u16> {
    let v: Vec<u16> = r
        .days_data
        .iter()
        .map(|d| d.unemployment_permille)
        .collect();
    v.iter()
        .skip(v.len().saturating_sub(G11_TAIL_DAYS))
        .copied()
        .collect()
}

fn g11_seed(r: &RunFile) -> bool {
    let v = ogon(r);
    if v.is_empty() {
        return true;
    }
    let m = mediana_u16(&v);
    m >= G11_UNEMPLOYMENT.0 && m <= G11_UNEMPLOYMENT.1
}

/// Mediana ciągu promili. Pusty ciąg daje zero — i to jest właściwa odpowiedź,
/// bo wołający sprawdza pustkę przed wywołaniem.
fn mediana_u16(v: &[u16]) -> u16 {
    if v.is_empty() {
        return 0;
    }
    let mut s = v.to_vec();
    s.sort_unstable();
    s[s.len() / 2]
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
    if szokowe.is_empty() {
        return GateOutcome::pominieta(
            "G4",
            "Reaktywnosc szoku",
            "brak przebiegow supply-shock — nie ma szoku, którego reakcję dałoby się zmierzyć"
                .to_string(),
            "t_response 2–7 d, t_settle 14–56 d",
        );
    }
    GateOutcome::nowa(
        "G4",
        "Reaktywnosc szoku",
        zdane == szokowe.len() as u64,
        format!("{zdane}/{} · {}", szokowe.len(), opis.join("; ")),
        "t_response 2–7 d, t_settle 14–56 d",
    )
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

// ── G12: czy makro i mezo to jeden model (M10f WP10.17, `E-13`) ──────────────────

/// Odchylenie względne makro od mezo w promilach dla jednej pary liczb.
///
/// Mianownikiem jest **mezo**, bo to ono jest źródłem prawdy (00 §4). Zero
/// w mianowniku znaczy „nie ma czego porównać", nie „odchylenie nieskończone".
fn odchylenie_permille(makro: i64, mezo: i64) -> Option<i64> {
    if mezo == 0 {
        return None;
    }
    Some((makro - mezo).saturating_mul(1_000) / mezo)
}

/// Seria miesięczna odchyleń: po jednej próbce na trzydzieści dób.
///
/// Miesięcznie, a nie dobowo, bo kontrakt `K2` mówi o **skali miesiąca gry**,
/// a próbka dobowa niosłaby rytm dobowy mezo, którego makro z definicji nie ma.
fn seria_miesieczna(lod: &[LodSample], wielkosc: Wielkosc) -> Vec<i64> {
    lod.iter()
        .filter(|s| s.day > 0 && s.day.is_multiple_of(30))
        .filter_map(|s| {
            let (makro, mezo) = wielkosc(s);
            odchylenie_permille(makro, mezo)
        })
        .collect()
}

/// Ile próbek miesięcznych ma przebieg — mianownik pytania „czy jest co mierzyć".
fn probki_miesieczne(lod: &[LodSample]) -> usize {
    lod.iter()
        .filter(|s| s.day > 0 && s.day.is_multiple_of(30))
        .count()
}

/// Nachylenie regresji liniowej serii, w **dziesiątych promila na rok gry**.
///
/// Próbki są miesięczne i równo odległe, więc `x` jest numerem miesiąca;
/// dwanaście miesięcy to rok gry (`K-1`). Liczone w `i128` i bez ani jednego
/// dzielenia pośredniego — werdykt bramki nie ma prawa zależeć od kolejności
/// działań (00 §2).
#[must_use]
pub fn nachylenie_na_rok(seria: &[i64]) -> i64 {
    let n = seria.len() as i128;
    if n < 2 {
        return 0;
    }
    let sx: i128 = (0..n).sum();
    let sy: i128 = seria.iter().map(|v| i128::from(*v)).sum();
    let sxy: i128 = seria
        .iter()
        .enumerate()
        .map(|(i, v)| i as i128 * i128::from(*v))
        .sum();
    let sxx: i128 = (0..n).map(|i| i * i).sum();
    let mianownik = n * sxx - sx * sx;
    if mianownik == 0 {
        return 0;
    }
    // slope [‰/miesiąc] × 12 [mies./rok] × 10 [dziesiąte]
    let licznik = (n * sxy - sx * sy) * 120;
    i64::try_from(licznik / mianownik).unwrap_or(i64::MAX)
}

/// Autokorelacja znaku serii przy opóźnieniu 1, w setnych (−100..=100).
///
/// Łapie „makro zawsze odrobinę na plus" natychmiast, także wtedy, gdy mediana
/// odchylenia mieści się w progu — a sam próg na nachylenie da się przypadkiem
/// przejść na krótkiej próbce (M10 §7.3 `K3`).
#[must_use]
pub fn autokorelacja_znaku(seria: &[i64]) -> i64 {
    let znaki: Vec<i64> = seria.iter().map(|v| v.signum()).collect();
    if znaki.len() < 2 {
        return 0;
    }
    let par = (znaki.len() - 1) as i64;
    let zgodne: i64 = znaki.windows(2).map(|w| w[0] * w[1]).sum();
    zgodne * 100 / par
}

fn mediana_abs(seria: &[i64]) -> i64 {
    let mut v: Vec<i64> = seria.iter().map(|x| x.abs()).collect();
    v.sort_unstable();
    v.get(v.len() / 2).copied().unwrap_or(0)
}

/// Werdykt jednej wielkości: mediana, nachylenie i autokorelacja znaku.
fn g12_wielkosc(seria: &[i64]) -> (bool, String) {
    let med = mediana_abs(seria);
    let nach = nachylenie_na_rok(seria);
    let auto = autokorelacja_znaku(seria);
    let ok = med <= G12_MEDIAN_DEV_MAX_PERMILLE
        && nach.abs() <= G12_SLOPE_MAX_TENTHS_PERMILLE
        && auto.abs() <= G12_SIGN_AUTOCORR_MAX;
    (
        ok,
        format!("med {med} ‰, nachylenie {nach}/10 ‰ na rok, autokor. znaku {auto}/100"),
    )
}

/// G12: czy `sim/macro` i mezo to **jeden model**, a nie dwa podobne.
///
/// Nie mierzy jakości przybliżenia — mierzy, czy ktoś nie dopisał logiki
/// ekonomicznej do `sim/macro` zamiast zawołać jądro. To jest ryzyko `R1` fazy
/// M10 i cała jej architektura stoi wokół niego (`WP10.2` przed `WP10.1`).
///
/// **Przestała być doradcza w `R2-WP24`.** Powód doradczości był nazwany
/// i miał warunek wygaśnięcia: dochód gospodarstwa był egzogeniczny, bo
/// `PayrollOutbox` nie miała konsumenta (`FF-29`), więc makro nie miało jak
/// odtworzyć kanału, którego mezo nie przechodziło. `R2-WP30` dała jej
/// konsumenta (`K-93`) — płaca schodzi z konta zakładu — więc warunek się
/// spełnił i komentarz zniknął razem z `advisory: true`. Bramka bez werdyktu
/// jest wykresem.
///
/// Przebieg bez próbek miesięcznych jest **pominięciem z powodem**, a nie
/// zielenią: bramka, która nie miała czego zmierzyć, nie ma prawa świecić.
fn lod(u: &[&RunFile]) -> GateOutcome {
    let mierzalne: Vec<&&RunFile> = u
        .iter()
        .filter(|r| probki_miesieczne(&r.lod) >= G12_MIN_MONTHS)
        .collect();
    if mierzalne.is_empty() {
        return GateOutcome::pominieta(
            "G12",
            "Makro nie odjezdza od mezo",
            format!(
                "brak przebiegu z {G12_MIN_MONTHS} probkami miesiecznymi (dlugosci: {})",
                u.iter()
                    .map(|r| r.lod.len().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            "mediana ≤ 5 ‰, nachylenie ≤ 0,2 ‰/rok, autokorelacja znaku ≤ 0,30",
        );
    }

    let mut czerwone: Vec<u64> = Vec::new();
    // Seria nieporównywalna w dobie zero znaczy, że ktoś zmienił jedną ze stron
    // pomiaru i bramka mierzy własną definicję — to jest **osobna** informacja
    // od czerwieni i dlatego ma własną listę w opisie.
    let mut nieporownywalne: Vec<&'static str> = Vec::new();
    let mut opisy: Vec<String> = Vec::new();
    for r in &mierzalne {
        for (nazwa, wielkosc) in WIELKOSCI {
            let (op, werdykt) = g12_seria(&r.lod, *wielkosc);
            match werdykt {
                Werdykt::Nieporownywalna => nieporownywalne.push(nazwa),
                Werdykt::Czerwona => czerwone.push(r.seed),
                Werdykt::Zielona => {}
            }
            opisy.push(format!("ziarno {}: {nazwa} {op}", r.seed));
        }
    }
    czerwone.dedup();

    GateOutcome::nowa(
        "G12",
        "Makro nie odjezdza od mezo",
        czerwone.is_empty(),
        format!(
            "{} ziaren czerwonych {czerwone:?}{}; {}",
            czerwone.len(),
            if nieporownywalne.is_empty() {
                String::new()
            } else {
                format!("; nieporównywalne w dobie zero: {nieporownywalne:?}")
            },
            opisy.join(" | ")
        ),
        "mediana ≤ 5 ‰, nachylenie ≤ 0,2 ‰/rok, autokorelacja znaku ≤ 0,30",
    )
}

/// Co porównujemy. Pieniądz jest wielkością **nazwaną w kontrakcie** (M10 §7.3,
/// tabela `K2`: „gotówka + depozyty GD per dzielnica"); bezrobocie dołożone,
/// bo jest wyjściem rynku pracy, czyli fazy, w której makro i mezo najłatwiej
/// się rozjeżdżają.
const WIELKOSCI: &[(&str, Wielkosc)] = &[
    ("pieniądz GD", |s| {
        (s.macro_hh_money_gr, s.mezo_hh_money_gr)
    }),
    ("bezrobocie", |s| {
        (
            i64::from(s.macro_unemployment_permille),
            i64::from(s.mezo_unemployment_permille),
        )
    }),
];

/// Wyciąg pary „makro, mezo" z jednej próbki doby.
type Wielkosc = fn(&LodSample) -> (i64, i64);

/// Werdykt jednej wielkości w jednym przebiegu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Werdykt {
    Zielona,
    Czerwona,
    /// Doba zero już się nie zgadza — obie strony liczą co innego.
    Nieporownywalna,
}

/// Seria jednej wielkości z werdyktem i opisem.
///
/// **Najpierw doba zero, potem dryf** — i ta kolejność jest istotą poprawki:
/// `lift()` kopiuje świat, więc jeśli w dobie zero liczby się nie zgadzają,
/// to nie jest dryf modelu, tylko dwie różne definicje. Sądzenie takiej serii
/// dałoby bramkę czerwoną na zawsze i nic nieznaczącą.
fn g12_seria(lod: &[LodSample], wielkosc: Wielkosc) -> (String, Werdykt) {
    let start = lod
        .iter()
        .find(|s| s.day == 0)
        .map(wielkosc)
        .and_then(|(m, z)| odchylenie_permille(m, z));
    if let Some(d0) = start {
        if d0.abs() > G12_DAY0_TOLERANCE_PERMILLE {
            return (
                format!("doba zero różni się o {d0} ‰ — nieporównywalne"),
                Werdykt::Nieporownywalna,
            );
        }
    }
    let seria = seria_miesieczna(lod, wielkosc);
    let (ok, opis) = g12_wielkosc(&seria);
    (
        opis,
        if ok {
            Werdykt::Zielona
        } else {
            Werdykt::Czerwona
        },
    )
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
            // Warstwa firm zerami: testy G1–G9 budują szereg cen i CPI, a bramki
            // firmowe mają własne dane i własne testy. Zero w `labour_force` znaczy
            // przy okazji „nie ma czego mierzyć" dla G11 — czyli dokładnie to, czym
            // ten wzorzec jest.
            firms: 0,
            firm_sites: 0,
            unemployment_permille: 0,
            labour_force: 0,
            vacancies: 0,
            firms_founded: 0,
            firms_gone: 0,
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

    // ── G12 ─────────────────────────────────────────────────────────────────

    /// Odchylenie skaczące w obie strony ma nachylenie zero — i to jest cała
    /// różnica między szumem a dryfem.
    #[test]
    fn szum_nie_ma_nachylenia() {
        // Palindrom: ta sama seria czytana od końca, więc regresja jest płaska
        // z konstrukcji. Znaki układają się w pary, więc autokorelacja też siada.
        let seria = vec![3, -3, -3, 3, 3, -3, -3, 3, 3, -3, -3, 3];
        assert_eq!(nachylenie_na_rok(&seria), 0);
        assert_eq!(mediana_abs(&seria), 3);
        let (ok, opis) = g12_wielkosc(&seria);
        assert!(ok, "szum w paśmie ma przechodzić ({opis})");
    }

    /// Odchylenie rosnące o promil na miesiąc to dwanaście promili na rok,
    /// czyli sto dwadzieścia dziesiątych — sześćdziesiąt razy ponad próg.
    #[test]
    fn dryf_wylapuje_nachylenie() {
        let seria: Vec<i64> = (0..12).collect();
        assert_eq!(nachylenie_na_rok(&seria), 120);
        let (ok, opis) = g12_wielkosc(&seria);
        assert!(!ok, "dryf o promil na miesiąc ma być czerwony ({opis})");
    }

    /// Odchylenie małe, ale **zawsze w tę samą stronę**: mediana i nachylenie
    /// przechodzą, a bramka i tak jest czerwona. To jest ten wiersz `K3`,
    /// dla którego autokorelacja znaku w ogóle tam stoi.
    #[test]
    fn stale_odchylenie_w_jedna_strone_jest_czerwone() {
        let seria = vec![4, 4, 3, 4, 3, 4, 4, 3, 4, 3, 4, 4];
        assert!(mediana_abs(&seria) <= G12_MEDIAN_DEV_MAX_PERMILLE);
        assert!(nachylenie_na_rok(&seria).abs() <= G12_SLOPE_MAX_TENTHS_PERMILLE);
        assert_eq!(autokorelacja_znaku(&seria), 100);
        let (ok, _) = g12_wielkosc(&seria);
        assert!(!ok, "makro zawsze odrobinę na plus to nie jest szum");
    }

    /// Mezo równe zero nie daje odchylenia nieskończonego — nie daje żadnego.
    #[test]
    fn zerowy_mianownik_nie_jest_odchyleniem() {
        assert_eq!(odchylenie_permille(100, 0), None);
        assert_eq!(odchylenie_permille(105, 100), Some(50));
    }

    /// Przebieg bez próbek nie ma czego zmierzyć i mówi to o sobie.
    ///
    /// Do `R2-WP24` mówił to jako „zielona, doradcza" — czyli tak samo jak
    /// przebieg zmierzony i zdany. Od R2f mówi to jako `Skipped` z powodem,
    /// a w biegu nocnym takie pominięcie wywraca przebieg (`D-N17`).
    #[test]
    fn brak_probek_pomija_bramke_z_powodem() {
        let r = przebieg_pusty();
        let g = lod(&[&r]);
        assert_eq!(g.verdict, Verdict::Skipped);
        assert!(
            !g.pass(),
            "bramka bez pomiaru nie ma prawa świecić na zielono"
        );
        assert!(g.blokuje(Profile::Nightly), "bieg nocny ma to złapać");
        assert!(!g.blokuje(Profile::Ci), "G12 jest na liście profilu ci");
    }

    fn przebieg_pusty() -> RunFile {
        RunFile {
            schema_version: crate::metrics::SCHEMA_VERSION,
            scenario: "base".to_string(),
            seed: 1,
            days: 10,
            citizens: 100,
            shops: 1,
            hashes: Vec::new(),
            days_data: Vec::new(),
            shock_good: None,
            conservation_ok: true,
            decisions_without_reason: 0,
            decisions_sampled: 0,
            lod: Vec::new(),
        }
    }
}
