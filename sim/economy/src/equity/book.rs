//! Arkusz zleceń i fixing (M10d §5.5, WP10.10, PRD §6.5).
//!
//! # Dlaczego fixing, a nie notowanie ciągłe
//!
//! Ciągła podwójna aukcja to mutacja stanu na każde zlecenie, czyli tyle punktów
//! synchronizacji, ile zleceń — a determinizm wymaga, żeby żaden z nich nie zależał
//! od kolejności ukończenia jobów (00 §3.3). Dla miasta z rzędu trzech tysięcy firm
//! i kilku tysięcy inwestorów efekt gospodarczy jest ten sam, a koszt jednej
//! deterministycznej redukcji na dobę — pomijalny.
//!
//! # Co tu jest funkcją czystą, a co nie
//!
//! **Cały ten moduł.** [`fixing`] dostaje dwie listy zleceń i wczorajszy kurs,
//! a oddaje cenę, wolumen i przydziały — nie widzi świata, nie widzi ksiąg i niczego
//! nie zapisuje. Pieniądz przesuwa dopiero [`super::system`], i to jest ta sama
//! granica, którą `lower_cell` postawił w `sim/macro`: tam, gdzie replay i zapis mogą
//! się rozjechać, stoi funkcja czysta z własnym testem.

use magnat_core::{Money, SimMinute};
use magnat_firms::Owner;

/// Uchwyt zlecenia giełdowego.
///
/// **Nie `OrderId`** — ta nazwa jest zajęta od M5 przez zamówienie zakupowe
/// (`crate::supply::OrderId`), a dwa typy o jednej nazwie w jednym crate'cie nie mogą
/// istnieć. Numer jest monotoniczny w obrębie gry i rozstrzyga remisy przy
/// racjonowaniu, więc jego kolejność jest kontraktem determinizmu.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct StockOrderId(pub u32);

/// Strona zlecenia.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Side {
    Buy,
    Sell,
}

/// Zlecenie z limitem ceny.
///
/// `bp` jest wielkością pakietu w **punktach bazowych udziału w firmie**: 10 000
/// to cała firma, 500 to próg ujawnienia. Cena jest ceną jednego punktu bazowego —
/// pomnożona przez 10 000 daje wycenę całości. Udziału nie liczymy w sztukach akcji,
/// bo `Firm.owners` liczy go w bp od M7a i druga tablica własności byłaby drugą
/// prawdą o tej samej liczbie (`GD-1`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct StockOrder {
    pub id: StockOrderId,
    pub holder: Owner,
    pub side: Side,
    /// Cena graniczna za jeden punkt bazowy. Kupujący płaci **nie więcej**,
    /// sprzedający dostaje **nie mniej**.
    pub limit: Money,
    pub bp: u16,
    pub expires: SimMinute,
}

/// Wynik fixingu: jedna cena dla wszystkich i rozpiska przydziałów.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Fix {
    pub price: Money,
    pub volume_bp: u32,
    /// `(zlecenie, przydzielone bp)` — obie strony, posortowane po identyfikatorze.
    /// Suma po stronie kupna równa się sumie po stronie sprzedaży równa się
    /// [`Fix::volume_bp`]; pilnuje tego test własnościowy na 10 tys. losowych ksiąg.
    pub fills: Vec<(StockOrderId, u16)>,
}

/// Wyznacza kurs i przydziały jednej sesji.
///
/// Reguła wyboru ceny jest z planu i ma trzy szczeble, bo dwa nie wystarczają:
/// **maksymalny wolumen**, przy remisie **cena najbliższa poprzedniemu fixingowi**,
/// przy dalszym remisie **niższa**. Bez trzeciego szczebla dwie ceny równo odległe
/// od wczorajszej rozstrzygałyby się kolejnością w wektorze, czyli niczym.
///
/// `None` znaczy „nie było przecięcia" — najlepsze kupno poniżej najlepszej sprzedaży.
/// To jest normalny stan sesji, nie błąd: kurs zostaje wtedy wczorajszy.
#[must_use]
pub fn fixing(buys: &[StockOrder], sells: &[StockOrder], prev: Money) -> Option<Fix> {
    if buys.is_empty() || sells.is_empty() {
        return None;
    }
    // Kandydatami są wyłącznie limity ze zleceń: między nimi funkcja wolumenu jest
    // stała, więc przeszukiwanie gęstsze nie znalazłoby ani jednej lepszej ceny.
    let mut ceny: Vec<i64> = buys
        .iter()
        .chain(sells.iter())
        .map(|o| o.limit.get())
        .collect();
    ceny.sort_unstable();
    ceny.dedup();

    let mut najlepsza: Option<(i64, u32)> = None;
    for p in ceny {
        let popyt: u32 = buys
            .iter()
            .filter(|o| o.limit.get() >= p)
            .map(|o| u32::from(o.bp))
            .sum();
        let podaz: u32 = sells
            .iter()
            .filter(|o| o.limit.get() <= p)
            .map(|o| u32::from(o.bp))
            .sum();
        let vol = popyt.min(podaz);
        if vol == 0 {
            continue;
        }
        najlepsza = Some(match najlepsza {
            None => (p, vol),
            Some((bp_cena, bv)) => {
                if lepsza(p, vol, bp_cena, bv, prev.get()) {
                    (p, vol)
                } else {
                    (bp_cena, bv)
                }
            }
        });
    }
    let (cena, vol) = najlepsza?;

    let mut fills = przydziel(buys, Side::Buy, cena, vol);
    fills.extend(przydziel(sells, Side::Sell, cena, vol));
    fills.sort_unstable_by_key(|(id, _)| id.0);
    Some(Fix {
        price: Money(cena),
        volume_bp: vol,
        fills,
    })
}

/// Czy kandydat `(p, v)` bije dotychczasowego zwycięzcę.
fn lepsza(p: i64, v: u32, best_p: i64, best_v: u32, prev: i64) -> bool {
    if v != best_v {
        return v > best_v;
    }
    let d = (p - prev).abs();
    let bd = (best_p - prev).abs();
    if d != bd {
        return d < bd;
    }
    p < best_p
}

/// Przydział wolumenu jednej stronie.
///
/// Pierwszeństwo ma cena: kto chce płacić więcej (albo brać mniej), dostaje
/// w całości. Racjonowanie dotyka **wyłącznie zleceń po cenie granicznej** i idzie
/// proporcjonalnie, a reszta z dzielenia trafia do najniższego identyfikatora
/// (00 §2). Zlecenia spoza widełek nie dostają nic i nie ma ich na liście —
/// wpis z zerem znaczyłby „próbował i nie wyszło", a to jest to samo co „nie
/// próbował" dopiero po stronie pieniądza, nie po stronie gracza.
fn przydziel(zlecenia: &[StockOrder], side: Side, cena: i64, vol: u32) -> Vec<(StockOrderId, u16)> {
    let w_widelkach = |o: &StockOrder| match side {
        Side::Buy => o.limit.get() >= cena,
        Side::Sell => o.limit.get() <= cena,
    };
    let lepszy_od_granicy = |o: &StockOrder| match side {
        Side::Buy => o.limit.get() > cena,
        Side::Sell => o.limit.get() < cena,
    };

    let mut pewne: Vec<&StockOrder> = zlecenia
        .iter()
        .filter(|o| o.side == side && lepszy_od_granicy(o))
        .collect();
    pewne.sort_unstable_by_key(|o| o.id.0);
    let mut graniczne: Vec<&StockOrder> = zlecenia
        .iter()
        .filter(|o| o.side == side && w_widelkach(o) && !lepszy_od_granicy(o))
        .collect();
    graniczne.sort_unstable_by_key(|o| o.id.0);

    let mut out: Vec<(StockOrderId, u16)> = Vec::with_capacity(pewne.len() + graniczne.len());
    let mut zostalo = vol;
    for o in pewne {
        let ile = zostalo.min(u32::from(o.bp));
        if ile == 0 {
            break;
        }
        out.push((o.id, ile as u16));
        zostalo -= ile;
    }
    if zostalo == 0 || graniczne.is_empty() {
        return out;
    }
    let razem: u32 = graniczne.iter().map(|o| u32::from(o.bp)).sum();
    if razem <= zostalo {
        for o in graniczne {
            out.push((o.id, o.bp));
        }
        return out;
    }
    // Pro rata z resztą do pierwszego wg identyfikatora — ten sam mechanizm, którym
    // `Money::split_proportional` dzieli kwotę między strony (00 §2).
    let mut rozdane = 0u32;
    let mut czesci: Vec<u32> = graniczne
        .iter()
        .map(|o| {
            let ile = (u64::from(zostalo) * u64::from(o.bp) / u64::from(razem)) as u32;
            rozdane += ile;
            ile
        })
        .collect();
    let mut reszta = zostalo - rozdane;
    for c in czesci.iter_mut() {
        if reszta == 0 {
            break;
        }
        *c += 1;
        reszta -= 1;
    }
    for (o, ile) in graniczne.iter().zip(czesci) {
        if ile > 0 {
            out.push((o.id, ile as u16));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::{rng, Entity, FirmId, StreamId, Tick};

    fn zl(id: u32, side: Side, limit: i64, bp: u16) -> StockOrder {
        StockOrder {
            id: StockOrderId(id),
            holder: Owner::Firm(magnat_firms::FirmKey(u64::from(id) + 1)),
            side,
            limit: Money(limit),
            bp,
            expires: SimMinute(u64::MAX),
        }
    }

    fn rozdziel(zlecenia: &[StockOrder]) -> (Vec<StockOrder>, Vec<StockOrder>) {
        (
            zlecenia
                .iter()
                .copied()
                .filter(|o| o.side == Side::Buy)
                .collect(),
            zlecenia
                .iter()
                .copied()
                .filter(|o| o.side == Side::Sell)
                .collect(),
        )
    }

    #[test]
    fn kurs_maksymalizuje_wolumen() {
        let z = [
            zl(1, Side::Buy, 100, 50),
            zl(2, Side::Buy, 90, 50),
            zl(3, Side::Sell, 80, 50),
            zl(4, Side::Sell, 95, 50),
        ];
        let (b, s) = rozdziel(&z);
        let f = fixing(&b, &s, Money(90)).expect("przecięcie jest");
        // Przy 95: popyt 50 (tylko zlecenie 1), podaż 100 → wolumen 50.
        // Przy 90: popyt 100, podaż 50 → wolumen 50. Przy 80: popyt 100, podaż 50 → 50.
        // Remis na wolumenie rozstrzyga odległość od wczorajszego kursu 90.
        assert_eq!(f.price, Money(90));
        assert_eq!(f.volume_bp, 50);
    }

    #[test]
    fn remis_wybiera_nizsza_cene() {
        // Dwie ceny równo odległe od wczorajszej i o tym samym wolumenie.
        let z = [zl(1, Side::Buy, 110, 30), zl(2, Side::Sell, 90, 30)];
        let (b, s) = rozdziel(&z);
        let f = fixing(&b, &s, Money(100)).expect("przecięcie jest");
        assert_eq!(f.price, Money(90), "przy remisie bierze się niższą");
    }

    #[test]
    fn brak_przeciecia_nie_daje_kursu() {
        let z = [zl(1, Side::Buy, 80, 10), zl(2, Side::Sell, 120, 10)];
        let (b, s) = rozdziel(&z);
        assert_eq!(fixing(&b, &s, Money(100)), None);
    }

    #[test]
    fn racjonowanie_dzieli_proporcjonalnie_a_reszte_daje_pierwszemu() {
        // Podaż 10 bp, popyt 30 bp po tej samej cenie granicznej w trzech zleceniach
        // o wielkościach 10/10/10 → po 3 i jedna bp reszty do najniższego id.
        let z = [
            zl(1, Side::Buy, 100, 10),
            zl(2, Side::Buy, 100, 10),
            zl(3, Side::Buy, 100, 10),
            zl(9, Side::Sell, 100, 10),
        ];
        let (b, s) = rozdziel(&z);
        let f = fixing(&b, &s, Money(100)).expect("przecięcie jest");
        assert_eq!(f.volume_bp, 10);
        let kupno: Vec<(u32, u16)> = f
            .fills
            .iter()
            .filter(|(id, _)| id.0 < 9)
            .map(|(id, bp)| (id.0, *bp))
            .collect();
        assert_eq!(kupno, vec![(1, 4), (2, 3), (3, 3)]);
    }

    /// Kryterium WP10.10: suma przydzielonych akcji == wolumen, **zawsze**.
    #[test]
    fn suma_przydzialow_rowna_sie_wolumenowi_na_dziesieciu_tysiacach_ksiag() {
        for ziarno in 0..10_000u64 {
            let mut r = rng(ziarno, StreamId::InvestorNoise, 0, Tick(ziarno));
            let n = 1 + (r.next_u32() % 12) as usize;
            let mut zlecenia = Vec::with_capacity(n);
            for i in 0..n {
                let side = if r.next_u32().is_multiple_of(2) {
                    Side::Buy
                } else {
                    Side::Sell
                };
                let limit = 50 + i64::from(r.next_u32() % 200);
                let bp = 1 + (r.next_u32() % 400) as u16;
                zlecenia.push(zl(i as u32 + 1, side, limit, bp));
            }
            let (b, s) = rozdziel(&zlecenia);
            let Some(f) = fixing(&b, &s, Money(150)) else {
                continue;
            };
            let po_stronie = |side: Side| -> u32 {
                f.fills
                    .iter()
                    .filter(|(id, _)| zlecenia.iter().any(|o| o.id == *id && o.side == side))
                    .map(|(_, bp)| u32::from(*bp))
                    .sum()
            };
            assert_eq!(po_stronie(Side::Buy), f.volume_bp, "ziarno {ziarno}");
            assert_eq!(po_stronie(Side::Sell), f.volume_bp, "ziarno {ziarno}");
            // Nikt nie dostaje więcej, niż zlecił, i nikt nie płaci gorzej niż limit.
            for (id, bp) in &f.fills {
                let o = zlecenia.iter().find(|o| o.id == *id).expect("zlecenie");
                assert!(*bp <= o.bp);
                match o.side {
                    Side::Buy => assert!(o.limit >= f.price),
                    Side::Sell => assert!(o.limit <= f.price),
                }
            }
        }
    }

    // Import trzymany przy teście: `FirmId` i `Entity` służą wyłącznie do budowy
    // właściciela w danych testowych.
    #[allow(dead_code)]
    fn _typy(_: FirmId, _: Entity) {}
}
