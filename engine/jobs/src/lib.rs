//! `magnat-jobs` — pula wątków i **deterministyczny** fork-join (00 §3.3, M0 §5.4).
//!
//! Determinizm nie pochodzi od puli — pochodzi od dyscypliny redukcji. Work stealing
//! wolno mieć dowolny, bo wynik składa się po **indeksie chunka**, nigdy po kolejności
//! zakończenia zadań. Dlatego `engine/jobs` to cienka warstwa nad jawną instancją
//! `rayon::ThreadPool`, a nie własny scheduler Chase-Lev (D-1).
//!
//! Sufit tego skrótu: brak kontroli nad priorytetami zadań i nad przypięciem do rdzeni.
//! Wymiana na własną pulę dopiero, gdy profilowanie w M12 wskaże narzut rayon —
//! API `JobPool` jest tak dobrane, żeby wymiana nie dotknęła wywołujących.
//!
//! **Zobowiązanie faz M1+:** `rayon::iter` i globalna pula rayon są zakazane poza tym
//! crate'em (lint `disallowed-types`). Kod symulacji widzi wyłącznie trzy funkcje:
//! [`JobPool::scope`], [`map_reduce_indexed`] i [`for_each_chunk_mut`].

#![forbid(unsafe_code)]

use rayon::prelude::*;

/// Zakres zadań zagnieżdżonych. Alias na typ rayon — opakowywanie go dodałoby
/// warstwę bez zachowania.
pub type Scope<'s> = rayon::Scope<'s>;

/// Jawna, **nie-globalna** pula wątków. Globalna pula rayon nie jest używana nigdzie
/// w projekcie: liczba wątków jest parametrem uruchomienia (`--threads`), a testy
/// determinizmu porównują przebiegi przy 1 i 16 wątkach.
pub struct JobPool {
    inner: rayon::ThreadPool,
}

impl JobPool {
    /// `threads == 0` → liczba dostępnych rdzeni.
    ///
    /// Panika, jeśli puli nie da się utworzyć — bez puli nie ma symulacji,
    /// więc ciche zejście do trybu jednowątkowego byłoby gorsze niż zatrzymanie.
    #[must_use]
    pub fn new(threads: usize) -> JobPool {
        let inner = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|i| format!("magnat-worker-{i}"))
            .build()
            .expect("JobPool: nie udało się utworzyć puli wątków");
        JobPool { inner }
    }

    #[must_use]
    pub fn thread_count(&self) -> usize {
        self.inner.current_num_threads()
    }

    /// Zadania zagnieżdżone. Panika workera propaguje się do wywołującego,
    /// a pula pozostaje użyteczna.
    pub fn scope<'s, R: Send>(&self, f: impl FnOnce(&Scope<'s>) -> R + Send) -> R {
        self.inner.scope(f)
    }

    /// Wykonuje domknięcie wewnątrz puli — punkt wejścia dla funkcji modułu.
    fn install<R: Send>(&self, f: impl FnOnce() -> R + Send) -> R {
        self.inner.install(f)
    }
}

/// **Jedyny dopuszczony sposób równoległej redukcji w kodzie symulacji** (00 §3.3).
///
/// Wyniki cząstkowe lądują w wektorze indeksowanym numerem chunka i są składane
/// w kolejności indeksu na wątku wywołującym — nigdy w kolejności zakończenia zadań.
/// Dzięki temu `combine` nie musi być łączne ani przemienne, a wynik nie zależy
/// od liczby wątków ani od tego, który worker skończył pierwszy.
pub fn map_reduce_indexed<C, A, R>(
    pool: &JobPool,
    chunks: &[C],
    map: impl Fn(usize, &C) -> A + Sync,
    combine: impl Fn(R, A) -> R,
    init: R,
) -> R
where
    C: Sync,
    A: Send,
    R: Send,
{
    // `collect::<Vec<_>>` po `par_iter().enumerate()` zachowuje kolejność indeksów —
    // to jest ta jedna właściwość rayon, na której stoi determinizm redukcji.
    let partial: Vec<A> = pool.install(|| {
        chunks
            .par_iter()
            .enumerate()
            .map(|(i, c)| map(i, c))
            .collect()
    });
    partial.into_iter().fold(init, combine)
}

/// Równoległa mutacja rozłącznych chunków bez redukcji (typowa iteracja ECS).
/// Brak wartości zwracanej ⇒ brak problemu kolejności; kolejność efektów ubocznych
/// jest kontraktem wywołującego — ma ich nie mieć poza swoim chunkiem.
pub fn for_each_chunk_mut<C: Send>(
    pool: &JobPool,
    chunks: &mut [C],
    f: impl Fn(usize, &mut C) + Sync,
) {
    pool.install(|| {
        chunks.par_iter_mut().enumerate().for_each(|(i, c)| f(i, c));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Redukcja niełączna i nieprzemienna: gdyby składanie szło po kolejności
    /// zakończenia, wynik rozjechałby się natychmiast.
    fn splot(acc: u64, v: u64) -> u64 {
        acc.wrapping_mul(31).wrapping_add(v)
    }

    #[test]
    fn redukcja_nie_zalezy_od_liczby_watkow() {
        let chunks: Vec<u64> = (0..10_000).collect();
        let mut wyniki = Vec::new();
        for threads in [1, 2, 8, 16] {
            let pool = JobPool::new(threads);
            wyniki.push(map_reduce_indexed(
                &pool,
                &chunks,
                |i, c| {
                    // Celowo nierówna praca: worker kończy w losowej kolejności.
                    let mut x = *c;
                    for _ in 0..(i % 37) {
                        x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                    }
                    x
                },
                splot,
                0,
            ));
        }
        assert!(
            wyniki.windows(2).all(|w| w[0] == w[1]),
            "wyniki różne przy różnej liczbie wątków: {wyniki:?}"
        );
    }

    #[test]
    fn redukcja_powtarzalna_w_wielu_przebiegach() {
        let pool = JobPool::new(16);
        let chunks: Vec<u64> = (0..1_000).collect();
        let wzorzec = map_reduce_indexed(&pool, &chunks, |_, c| *c, splot, 0);
        for _ in 0..1_000 {
            assert_eq!(
                map_reduce_indexed(&pool, &chunks, |_, c| *c, splot, 0),
                wzorzec
            );
        }
    }

    #[test]
    fn mutacja_dotyka_kazdego_chunka_dokladnie_raz() {
        let pool = JobPool::new(8);
        let mut chunks: Vec<u32> = vec![0; 5_000];
        let licznik = AtomicU32::new(0);
        for_each_chunk_mut(&pool, &mut chunks, |i, c| {
            *c = i as u32 + 1;
            licznik.fetch_add(1, Ordering::Relaxed);
        });
        assert_eq!(licznik.load(Ordering::Relaxed), 5_000);
        assert!(chunks.iter().enumerate().all(|(i, c)| *c == i as u32 + 1));
    }

    #[test]
    fn panika_workera_wraca_do_wywolujacego_a_pula_zyje() {
        let pool = JobPool::new(4);
        let chunks: Vec<u32> = (0..100).collect();
        let wynik = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            map_reduce_indexed(
                &pool,
                &chunks,
                |_, c| {
                    assert!(*c != 50, "celowa panika w workerze");
                    *c
                },
                |a: u32, b| a + b,
                0,
            )
        }));
        assert!(wynik.is_err(), "panika nie dotarła do wywołującego");

        // Pula ma być nadal użyteczna — zatruty zamek byłby tu widoczny.
        let suma = map_reduce_indexed(&pool, &chunks, |_, c| *c, |a: u32, b| a + b, 0);
        assert_eq!(suma, (0..100).sum::<u32>());
    }

    #[test]
    fn zero_watkow_znaczy_liczba_rdzeni() {
        let pool = JobPool::new(0);
        assert!(pool.thread_count() >= 1);
        assert_eq!(JobPool::new(3).thread_count(), 3);
    }

    #[test]
    fn scope_wykonuje_zadania_zagniezdzone() {
        let pool = JobPool::new(4);
        let licznik = AtomicU32::new(0);
        pool.scope(|s| {
            for _ in 0..100 {
                s.spawn(|_| {
                    licznik.fetch_add(1, Ordering::Relaxed);
                });
            }
        });
        assert_eq!(licznik.load(Ordering::Relaxed), 100);
    }
}
