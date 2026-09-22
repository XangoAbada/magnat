# E5 — Makro i spójność LOD

**Po co.** Makro liczy ceny za sztukę, a są to ceny za 1000 jednostek — komórka kupuje
1000× mniej towaru niż mezo. `lower()` gubi dług firm. Bramka G12 ma próg 10× luźniejszy
niż kontrakt K2, a bramka 6 Etapu 10 nie może się nie udać. To prawdopodobnie brakująca
przyczyna czerwonych bramek Etapu 10 (`FF-29`, `GG-3`).

**Wejście.** E1 i E2 zamknięte (`N1.5` — `macro-kernel` zielony; `N2.1` — przyrząd długu).

**Pomiar zamknięcia.** Ponowny pomiar bramek Etapu 10 (1, 3, 4, 5) i K2 na tych samych
ziarnach co w M10f, wynik w dokumencie etapu obok starego. Dopiero ten wynik mówi, czy
`FF-29` i `GG-3` są nadal potrzebne — to rozstrzygnięcie idzie do `N8.4`.

## Punkty

- [ ] **N5.1** jednostka ceny w makro — `M10#1`
  - `sim/macro/src/step/goods.rs:103-108`, `step/produce.rs:83-84`, `dosyp_zapasy`
    w `dryrun.rs`: `ShelfLine.price_net` jest ceną za `PRICE_UNIT = 1000`; mezo dzieli przez
    1000 (`supply.rs:21,30`, `fulfil.rs:581`), makro nie.
  - *Naprawa:* makro woła tę samą funkcję przeliczenia co mezo (z `kernel`, nie kopia).
  - *Test:* ta sama komórka z tym samym budżetem kupuje w makro i mezo tę samą masę
    (z tolerancją K2).

- [ ] **N5.2** `lower()` przenosi dług — `M10#5`
  - `sim/macro/src/lower.rs:383-395` ustawia saldo na `capital`, pomija `debt`;
    `sumy_pieniezne` (`macro_lod.rs:80`) nie sumuje długu.
  - *Test:* przyrząd długu z `N2.1` na przejściu `lift → lower`.

- [ ] **N5.3** G12 = K2 — `M10#11`
  - `tools/balansator/src/gates.rs:61`: 50 ‰ = 5 %, K2 wymaga 0,5 %; komentarz mówi „0,5 %".

- [ ] **N5.4** bramka 6 Etapu 10 może się nie udać — `M10#14`
  - `sim/macro/src/verify.rs` liczy `capital < 0`, a `lift` przycina do ≥ 0;
    `wyplacalna` (`labor.rs:484`) zawsze `true`.
  - *Naprawa:* wypłacalność liczona przed przycięciem; przycięcie jest zdarzeniem zliczanym.
  - *Test:* komórka z ujemnym kapitałem po kroku → bramka pada.

- [ ] **N5.5** pieniądz bez `f64` — `M10#16`, `M7#11`
  - `goods.rs:99`: budżet dzielony przez `f64`. Deterministyczne, ale łamie 00 §2.
    Rachunek na `i128` albo `Fx`.

- [ ] **N5.6** `macro_error_margin_is_honest` — `M10` WP10.4
  - Kontrakt §6 powołuje się na ten test, a go nie ma. Margines błędu `what_if()`
    niezmierzony. Napisać: prognoza makro vs przebieg mezo na N ziarnach, błąd w deklarowanym
    marginesie.

- [ ] **N5.7** `MacroLodPolicy` wie, czego nie wolno przerwać — `M10` WP10.1
  - `may_transition` (`sim/macro/src/lod.rs:77`) dostaje `&MacroState`, który nie wie
    o fixingu, negocjacjach ani strajkach — przejście LOD w trakcie jest możliwe.
  - *Test:* trwający strajk → przejście odrzucone.

- [ ] **N5.8** proptest `lift(lower(s)) == s` z firmami — `M10` WP10.1
  - Dziś tylko sumy komórek, bez firm i długu.

## Znalezione po drodze
