//! Pola robocze generatora — stan żyjący tylko na czas generacji.
//!
//! Nie wchodzą do zapisu gry ani do hasha: wszystkie są pochodną seeda, więc zapisywanie
//! ich kosztowałoby setki megabajtów za coś, co odtwarza się w kilka sekund. Rozmiar dla
//! mapy 16 km jest mimo to istotny — 16,8 mln komórek razy 4 B to 67 MB na pole.
//!
//! Wysokość robocza jest w `f32` **w metrach**, nie w decymetrach `i16` jak stan trwały.
//! Powód: erozja to 80 iteracji odejmowania małych wartości od dużych — kwantyzacja do
//! decymetra po każdym kroku zjadłaby cały sygnał. Kwantyzacja następuje raz, na końcu P6.
//! Determinizm `f32` jest tu utrzymany dyscypliną z M1 §5.6 (stała kolejność iteracji,
//! tylko `+ − × ÷` i `sqrt`, brak `mul_add`), a nie typem — patrz decyzja D10.

use crate::grid::Grid2;

/// Indeks komórki bez odbiornika (morze, krawędź mapy). Wartość, nie `Option`,
/// bo pole odbiorników ma 16,8 mln pozycji i `Option<u32>` kosztowałby 8 B zamiast 4.
pub const NO_RECEIVER: u32 = u32::MAX;

pub struct WorkFields {
    /// Maska lądu z P1. `true` = ląd.
    pub land: Grid2<bool>,
    /// Wysokość robocza w metrach n.p.m. Źródło prawdy od P2 do końca P6.
    pub height_m: Grid2<f32>,
    /// Wysokość po wypełnieniu zagłębień (P4). Różnica względem `height_m` wyznacza misy jezior.
    pub filled_m: Grid2<f32>,
    /// Tempo wypiętrzenia `U` w m/rok (P3).
    pub uplift: Grid2<f32>,
    /// Podatność na erozję `K` (P3). Niska = twarda intruzja, czyli przyszły ostaniec.
    pub erodibility: Grid2<f32>,
    /// Indeks komórki odbiorczej D8 (P5) albo [`NO_RECEIVER`].
    pub receiver: Grid2<u32>,
    /// Porządek topologiczny od ujść w górę zlewni (P5). Przejście po nim w przód liczy
    /// akumulację, w tył — erozję. Jeden wektor zamiast rekurencji: zlewnia potrafi mieć
    /// milion komórek, a stos wątku nie.
    pub stack: Vec<u32>,
    /// Pole zlewni w m² (P5).
    pub flow_acc: Grid2<f32>,
    /// Identyfikator zlewni każdej komórki (P5) — podstawa równoległości erozji po zlewniach.
    pub basin: Grid2<u32>,
    /// Indeksy komórek będących ujściami, w kolejności rosnących indeksów.
    pub outlets: Vec<u32>,
    /// Zakres `[start, end)` w `stack` dla każdej zlewni — podstawa równoległości erozji.
    pub basin_ranges: Vec<(u32, u32)>,
    /// Listy donorów w postaci CSR: `donors[donor_start[c]..donor_start[c + 1]]`.
    /// Jedna alokacja zamiast 16,8 mln małych wektorów.
    pub donor_start: Vec<u32>,
    pub donors: Vec<u32>,
    /// Ile komórek zmieniło ujście, do którego spływają, przy przetrasowaniach w trakcie
    /// erozji (P6). To jest miara przechwyceń rzecznych — przy `reroutes = 0`
    /// z definicji zero, bo topologia odwodnienia jest wtedy ustalana raz, przed pętlą.
    pub basin_captures: u64,
}

impl WorkFields {
    #[must_use]
    pub fn new(dim: usize) -> WorkFields {
        WorkFields {
            land: Grid2::filled(dim, true),
            height_m: Grid2::filled(dim, 0.0),
            filled_m: Grid2::filled(dim, 0.0),
            uplift: Grid2::filled(dim, 0.0),
            erodibility: Grid2::filled(dim, 0.0),
            receiver: Grid2::filled(dim, NO_RECEIVER),
            stack: Vec::new(),
            flow_acc: Grid2::filled(dim, 0.0),
            basin: Grid2::filled(dim, 0),
            outlets: Vec::new(),
            basin_ranges: Vec::new(),
            donor_start: Vec::new(),
            donors: Vec::new(),
            basin_captures: 0,
        }
    }

    /// Pamięć robocza w bajtach — pozycja do raportu, bo dla mapy 16 km to setki megabajtów
    /// i warto, żeby było to widoczne, a nie zaskakujące.
    #[must_use]
    pub fn bytes(&self) -> usize {
        use std::mem::size_of;
        self.land.len()
            + self.height_m.len() * size_of::<f32>() * 4
            + self.receiver.len() * size_of::<u32>() * 2
            + self.stack.capacity() * size_of::<u32>()
            + self.outlets.capacity() * size_of::<u32>()
    }
}
