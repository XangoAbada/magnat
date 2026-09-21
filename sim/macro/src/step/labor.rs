//! Faza 2 — rynek pracy (M10a §5.7, co krok).
//!
//! Podaż roli w komórce zestawiona z popytem firm, stawka z `kernel::wage_bid`,
//! zmiana przycięta do ±2 %/dobę. Firma zatrudnia z **puli swojej dzielnicy**:
//! # Rekrutacja poza dzielnicą kosztuje dojazd (M10e, decyzja `D10`)
//!
//! Do M10d firma zatrudniała **wyłącznie** z puli swojej dzielnicy, bo
//! `CommuteMatrix` była płaska i granica dzielnicy była jedyną rzeczą, która
//! w ogóle odróżniała dzielnicę od dzielnicy. Kosztowało to jedną trzecią firm:
//! miasto ma dzielnice przemysłowe bez mieszkań i mieszkaniowe bez zakładów, więc
//! **29 % firm nie dostawało ani jednego pracownika przez trzydzieści lat** historii
//! „na sucho". Te firmy nic nie produkowały, rynek zjadał zapas z Etapu 7, a bramka 1
//! Etapu 10 (nierównowaga) wychodziła 1000 ‰.
//!
//! Otwarcie puli na całe miasto **zostało spróbowane w M10a i cofnięte**: bezrobocie
//! schodziło do zera, firmy o niższym `FirmId` zabierały całą pulę, a odsetek firm
//! bez obsady rósł z 291 ‰ do 342 ‰. Problemem nie była granica dzielnicy, tylko
//! **brak kosztu dojazdu**: bez niego rekrutacja jest albo zakazana, albo darmowa,
//! a żadne z tych dwojga nie jest rynkiem pracy.
//!
//! Od M10e macierz jest wypełniona geometrią miasta (`crate::commute`), a pula
//! jest **ważona gotowością do dojazdu**: z dzielnicy za rogiem przychodzą prawie
//! wszyscy, z drugiego końca miasta prawie nikt. Dzielnice obchodzi się w kolejności
//! rosnącego czasu dojazdu, więc „najpierw swoi" zostaje — ale jako preferencja,
//! a nie zakaz.
//!
//! # Skąd w ogóle bierze się bezrobocie (`GF-7`)
//!
//! Otwarcie puli miało cenę, której nie zapisano: **granica dzielnicy była jedynym
//! tarciem, jakie ten model miał**. Kiedy zniknęła, firmy zatrudniły wszystkich
//! i bramka 4 Etapu 10 pokazała **0 ‰ bezrobocia wobec pasma 30–150**, a `G12`
//! zobaczyła to samo jako 863 ‰ odchylenia od mezo.
//!
//! Brakującym źródłem są **odejścia dobrowolne**. W mezo prowadzi je
//! `hr::turnover` na liczbie `hr.quit_base_per_10k` z `data/tuning/labor.ron`
//! (2 na dziesięć tysięcy obsady na dobę, czyli ~7 % rotacji rocznej) i to ona
//! utrzymuje bezrobocie na dodatnim poziomie równowagi. Makro liczy je **tą samą
//! liczbą** (`K-50`: jedna reguła, jedno źródło) i przed rekrutacją, nie po —
//! odchodzący wracają do puli tej samej doby, więc pieniądz ani ludzie nie znikają.
//!
//! Zaokrąglenie jest **losowe**, a nie w dół: firma o stu etatach traci
//! 100 × 2 / 10 000 = 0,02 etatu na dobę, a obcięcie do zera znaczyłoby, że
//! w mieście małych firm nie odchodzi nikt. Rzut idzie strumieniem
//! `StreamId::MacroStep` z kluczem `(indeks firmy, tick)` — numer zarezerwowany
//! w `K-78` właśnie na „losowość wewnątrz kroku makro" i tu dostaje pierwszego
//! użytkownika.

use magnat_core::{rng, Money, StreamId};
use magnat_economy::kernel;
use magnat_firms::hr::productivity::FULL_TIME;

use crate::state::MacroState;

use super::{MacroParams, WAGE_STEP_CAP_BP};

pub fn phase(st: &mut MacroState, p: &MacroParams) {
    let dzielnice = st.commute.districts();
    // Pula bezrobotnych per dzielnica — liczona raz, zużywana po kolei przez firmy
    // w kolejności `FirmId`. Kolejność jest deterministyczna i to ona rozstrzyga,
    // kto pierwszy dostanie ludzi przy niedoborze.
    let mut pula: Vec<u32> = vec![0; usize::from(dzielnice)];
    for c in &st.cells {
        if let Some(slot) = pula.get_mut(usize::from(c.key.0 .0)) {
            *slot = slot.saturating_add(c.unemployed);
        }
    }

    // Odejścia dobrowolne — jedyne źródło bezrobocia poza redukcją etatów.
    odejscia(st, p, &mut pula);

    for f in &mut st.firms {
        // Firma bez kapitału nie licytuje i nie zatrudnia. To nie jest upadłość —
        // tę prowadzi mezo (`K-10`) — tylko zatrzymanie ekspansji, które w horyzoncie
        // kwartału odróżnia wariant wykonalny od niewykonalnego.
        let wyplacalna = f.capital.get() >= 0;
        let etaty = u32::try_from((f.capacity_daily.get() / FULL_TIME).max(0)).unwrap_or(0);
        if etaty == 0 {
            continue;
        }
        let wakaty = etaty.saturating_sub(f.employees);
        let d = usize::from(f.district.0);

        // Stawka jednostkowa: dzisiejszy rachunek podzielony przez obsadę.
        let stawka = if f.employees == 0 {
            p.default_wage_month
        } else {
            Money(f.wage_bill.get() / i64::from(f.employees))
        };

        if wakaty > 0 && wyplacalna {
            // Indeks niedoboru w promilach zamówionych etatów — to samo znaczenie
            // co `shortage_index` rynku pracy mezo: „ile z tego, czego chcę, stoi puste".
            let niedobor =
                u16::try_from(u64::from(wakaty) * 1_000 / u64::from(etaty)).unwrap_or(1_000);
            let mozliwe =
                u32::from(p.hire_speed_permille) * u32::from(p.days_per_step.max(1)) * etaty
                    / 1_000;
            let chciane = wakaty.min(mozliwe.max(1));
            let mut brakuje = chciane;
            // Dzielnice w kolejności rosnącego czasu dojazdu; przy remisie niższy
            // numer, bo kolejność rekrutacji wchodzi do stanu (00 §3.2).
            let mut zrodla: Vec<(u16, usize)> = (0..dzielnice)
                .map(|e| (st.commute.minutes(e, f.district.0), usize::from(e)))
                .collect();
            zrodla.sort_unstable();
            for (minuty, e) in zrodla {
                if brakuje == 0 {
                    break;
                }
                let gotowi = u32::from(gotowosc_bp(minuty)) * pula[e] / 10_000;
                let chetni = brakuje.min(gotowi);
                if chetni > 0 {
                    pula[e] -= chetni;
                    f.employees = f.employees.saturating_add(chetni);
                    brakuje -= chetni;
                }
            }

            // **Stawkę rusza dopiero nieudana rekrutacja, a widełki kotwiczą
            // w płacy odniesienia** — dwie połowy jednej naprawy (`GG-2`).
            //
            // Obie odtwarzają regułę mezo: tam firma podnosi ofertę, kiedy nikt
            // nie przyszedł (`search.escalate_after_days`), a przedział bierze
            // z widełek roli w `data/jobs/roles.ron`, czyli z liczby, która nie
            // zależy od tego, ile firma płaci dziś. Tutaj licytacja szła
            // w **każdym** kroku z wakatem, `wage_escalation_step` nigdy nie
            // zwraca zera, a sufit `stawka × 3` przesuwał się razem ze stawką —
            // czyli nie był sufitem. Bez odejść dobrowolnych nie było tego widać,
            // bo każdy wakat kiedyś się zamykał; z odejściami wakat jest wieczny
            // i płace rosły o dwa procent na dobę przez trzydzieści lat, aż firmy
            // pożyczały na listę płac stokrotność obrotu miasta.
            let nowa = if brakuje > 0 {
                let baza = p.default_wage_month.get().max(1);
                let bid = kernel::wage_bid(
                    stawka,
                    (Money(baza / 2), Money(baza * 3)),
                    niedobor,
                    p.aggression,
                    // Zapas marży: makro nie zna rachunku wyniku zakładu, więc podaje
                    // widełki z danych. To jest ten sam hak zerowy, którym M7b stał
                    // do M7e — i tu zostaje, bo `SitePnlMonth` jest wielkością mezo.
                    p.margin_bp.1,
                    &p.wage,
                );
                let cap = kernel::apply_bp(stawka, WAGE_STEP_CAP_BP).get();
                bid.wage.get().min(stawka.get().saturating_add(cap.max(1)))
            } else {
                stawka.get()
            };
            f.wage_bill = Money(nowa.saturating_mul(i64::from(f.employees)));
        } else if f.employees > etaty {
            // Redukcja do zamówionych etatów. Zwolnieni wracają do puli **tej samej
            // doby** — inaczej zamknięcie zakładu w wariancie „co jeśli" wyglądałoby
            // jak zniknięcie ludzi z miasta.
            let nadmiar = f.employees - etaty;
            f.employees = etaty;
            pula[d] = pula[d].saturating_add(nadmiar);
            f.wage_bill = Money(stawka.get().saturating_mul(i64::from(f.employees)));
        }
    }

    rozlej_zatrudnienie(st, &pula);
}

/// Odejścia dobrowolne: ludzie wracają do puli swojej **dzielnicy pracy**.
///
/// Dzielnicy zakładu, a nie zamieszkania, i to jest świadome uproszczenie: makro nie
/// wie, skąd dojeżdżał ten konkretny człowiek, bo pula jest liczbą, a nie listą.
/// `ponytail:` sufit nazwany — ścieżka wyjścia to macierz „kto skąd dojeżdża"
/// per firma, czyli stan rzędu dzielnic × firm, którego nikt dziś nie potrzebuje.
fn odejscia(st: &mut MacroState, p: &MacroParams, pula: &mut [u32]) {
    if p.quit_per_10k_day == 0 {
        return;
    }
    let dni = u64::from(p.days_per_step.max(1));
    let seed = p.seed;
    let tick = st.tick;
    for (i, f) in st.firms.iter_mut().enumerate() {
        if f.employees == 0 {
            continue;
        }
        let x = u64::from(f.employees) * u64::from(p.quit_per_10k_day) * dni;
        let mut r = rng(
            seed,
            StreamId::MacroStep,
            u32::try_from(i).unwrap_or(0),
            tick,
        );
        // Zaokrąglenie losowe: reszta jest szansą na jeszcze jedno odejście.
        let ilu = u32::try_from(x / 10_000).unwrap_or(0)
            + u32::from(r.gen_range_u32(10_000) < u32::try_from(x % 10_000).unwrap_or(0));
        let ilu = ilu.min(f.employees);
        if ilu == 0 {
            continue;
        }
        // **Rachunek płac schodzi razem z obsadą.** Bez tego `stawka` liczona niżej
        // jako `wage_bill / employees` rośnie przy każdym odejściu, licytacja stawia
        // na zawyżonej podstawie i lista płac pęcznieje wykładniczo — a suma
        // pieniądza w mieście przestaje się zgadzać (K1, tolerancja 0 groszy).
        // Odejście pracownika zmienia **liczbę** pensji, nie ich wysokość.
        let stawka = f.wage_bill.get() / i64::from(f.employees);
        f.employees -= ilu;
        f.wage_bill = Money(stawka.saturating_mul(i64::from(f.employees)));
        if let Some(slot) = pula.get_mut(usize::from(f.district.0)) {
            *slot = slot.saturating_add(ilu);
        }
    }
}

/// Ilu z dziesięciu tysięcy jest gotowych dojeżdżać tyle minut.
///
/// Prosta liniowa krzywa od pełnej gotowości przy dojeździe wewnątrzdzielnicowym
/// do zera przy [`MAX_COMMUTE_MIN`]. Kształtu bardziej wyszukanego tu nie ma
/// z rozmysłu: o wyborze środka transportu i o wartości czasu rozstrzyga M4 na
/// poziomie mezo (`data/roads/mode_choice.ron`), a makro potrzebuje wyłącznie
/// **monotonicznego kosztu odległości**. Druga krzywa nad tą samą rzeczą
/// rozjechałaby się z pierwszą (`K-50`).
///
/// `ponytail:` liniowo, bez wartości czasu i bez różnicy między zawodami. Sufit
/// jest nazwany: kierownik dojedzie dalej niż kasjer, a makro tego nie odróżnia.
/// Ścieżka wyjścia: mnożnik per `ClassId`, kiedy bramka Etapu 10 zacznie na to
/// reagować.
#[must_use]
pub fn gotowosc_bp(minuty: u16) -> u16 {
    if minuty >= MAX_COMMUTE_MIN {
        return 0;
    }
    let zostalo = u32::from(MAX_COMMUTE_MIN - minuty);
    u16::try_from(zostalo * 10_000 / u32::from(MAX_COMMUTE_MIN)).unwrap_or(10_000)
}

/// Dojazd, przy którym nikt już nie przychodzi. Godzina w jedną stronę —
/// dwie godziny dziennie są granicą, powyżej której ludzie się przeprowadzają
/// zamiast dojeżdżać, a przeprowadzki prowadzi faza 1 kroku.
pub const MAX_COMMUTE_MIN: u16 = 60;

/// Przepisuje pulę z powrotem na komórki, w kolejności klucza komórki, i domyka
/// różnicę na `employed`. Bez tego bezrobocie liczyłoby się z liczby, która
/// zmieniła się tylko po stronie firm.
fn rozlej_zatrudnienie(st: &mut MacroState, pula: &[u32]) {
    for (d, wolni) in pula.iter().enumerate() {
        let mut zostalo = *wolni;
        let indeksy: Vec<usize> = st
            .cells
            .iter()
            .enumerate()
            .filter(|(_, c)| usize::from(c.key.0 .0) == d)
            .map(|(i, _)| i)
            .collect();
        if indeksy.is_empty() {
            continue;
        }
        // Siła robocza dzielnicy się nie zmienia — zmienia się jej podział.
        let sila: u32 = indeksy
            .iter()
            .map(|i| st.cells[*i].labour_force())
            .fold(0, u32::saturating_add);
        for i in &indeksy {
            let c = &mut st.cells[*i];
            // Siłę roboczą komórki trzeba **zapamiętać przed zapisem**: to ona jest
            // mianownikiem i ona jest tym, co się nie zmienia. Odczyt po zmianie
            // `unemployed` liczyłby udział z liczby, którą właśnie się nadpisało —
            // i miasto gubiłoby ludzi po kilku na dobę.
            let moja = c.labour_force();
            let udzial = if sila == 0 {
                0
            } else {
                (u64::from(zostalo) * u64::from(moja) / u64::from(sila)) as u32
            };
            let bez = udzial.min(moja);
            c.unemployed = bez;
            c.employed = moja.saturating_sub(bez);
            zostalo = zostalo.saturating_sub(bez);
        }
        // Reszta z dzielenia trafia do ostatniej komórki dzielnicy — tak samo jak
        // reszta z podziału kwoty trafia do pierwszej strony (00 §2). Liczba ludzi
        // ma się zgadzać co do osoby, nie „w przybliżeniu".
        if zostalo > 0 {
            if let Some(i) = indeksy.last() {
                let c = &mut st.cells[*i];
                let dodaj = zostalo.min(c.employed);
                c.unemployed = c.unemployed.saturating_add(dodaj);
                c.employed -= dodaj;
            }
        }
    }
}
