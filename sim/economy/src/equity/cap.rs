//! Akcjonariat: kto ile ma i jak to się zmienia (M10d WP10.10 i WP10.11).
//!
//! Osobny plik od [`super`], bo to jest **drugi temat w tym samym module**: tamten
//! trzyma rejestr giełdy, a ten arytmetykę udziałów. Wszystkie funkcje są czyste
//! wobec `Firm` — nie znają ksiąg, nie znają świata i nie wiedzą, kto komu płaci.
//!
//! Udział liczy się w **punktach bazowych**, a suma wynosi dokładnie 10 000
//! (`Firm::SHARES_TOTAL`). Drugiej tablicy akcjonariatu nie ma i nie będzie
//! (`GD-1`) — byłaby drugą prawdą o tej samej liczbie.

use magnat_core::{DecisionReason, FirmReason, Money, Subject};
use magnat_firms::{Firm, Owner};

use super::WHOLE_BP;

/// Liczbowa tożsamość właściciela — klucz mapy i porządek sortowania.
///
/// Warianty bez identyfikatora (`Player`, `City`, `External`) dostają numery
/// z górnego krańca `u32`, bo są jednostkowe i nigdy nie zderzą się z indeksem
/// encji: encji o indeksie `u32::MAX - 2` nie da się utworzyć w świecie, który
/// mieści się w pamięci.
#[must_use]
pub fn owner_key(o: Owner) -> u32 {
    match o {
        Owner::Citizen(c) => c.0.index(),
        Owner::Firm(k) => (k.0 as u32) ^ 0x8000_0000,
        Owner::Player => u32::MAX,
        Owner::City => u32::MAX - 1,
        Owner::External => u32::MAX - 2,
    }
}

/// Właściciel jako podmiot karty inspekcji (`K-62`).
///
/// `Player` jedzie jako `Subject::Government`? Nie — gracz **jest** mieszkańcem
/// i jego kartą jest karta mieszkańca, ale `Owner::Player` nie niesie jego encji.
/// Dlatego `None`: powód bez podmiotu jest lepszy od powodu wskazującego nie na tego.
#[must_use]
pub fn owner_subject(o: Owner) -> Option<Subject> {
    match o {
        Owner::Citizen(c) => Some(Subject::Citizen(c)),
        Owner::Firm(k) => Some(Subject::Firm(magnat_firms::firm_id(k))),
        Owner::Player | Owner::City | Owner::External => None,
    }
}

/// Ile bp ma ten właściciel w firmie.
#[must_use]
pub fn stake_of(firm: &Firm, holder: Owner) -> u16 {
    firm.owners
        .iter()
        .find(|s| s.owner == holder)
        .map_or(0, |s| s.bp)
}

/// Przenosi `bp` udziału od sprzedającego do kupującego.
///
/// Zwraca ile faktycznie przeszło — mniej, gdy sprzedający nie ma tyle. Suma
/// udziałów nie zmienia się o ani jedną bp, więc `Firm::owners_sum_ok` zostaje
/// prawdą; to jest cały test własnościowy „suma akcji w obiegu" z WP10.10.
pub fn move_stake(firm: &mut Firm, from: Owner, to: Owner, bp: u16) -> u16 {
    let ma = stake_of(firm, from);
    let ile = bp.min(ma);
    if ile == 0 {
        return 0;
    }
    if let Some(s) = firm.owners.iter_mut().find(|s| s.owner == from) {
        s.bp -= ile;
    }
    match firm.owners.iter_mut().find(|s| s.owner == to) {
        Some(s) => s.bp += ile,
        None => firm
            .owners
            .push(magnat_firms::OwnerShare { owner: to, bp: ile }),
    }
    firm.owners.retain(|s| s.bp > 0);
    // Kolejność właścicieli jest kolejnością wypłaty dywidendy i wchodzi do hasha
    // stanu, więc nie może zależeć od tego, kto kiedy kupił.
    firm.owners.sort_by_key(|s| owner_key(s.owner));
    ile
}

/// Emisja: `bp` nowych udziałów obejmuje `to`, a dotychczasowi właściciele
/// rozwadniają się **proporcjonalnie**.
///
/// Zwraca `false`, gdy emisja byłaby całą firmą albo większa. Reszta z dzielenia
/// trafia do właściciela o najniższym kluczu (00 §2), więc suma zostaje 10 000 —
/// bez tego emisja gubiłaby po jednej bp na każdym rozwodnionym właścicielu.
pub fn dilute(firm: &mut Firm, to: Owner, bp: u16) -> bool {
    let nowe = u32::from(bp);
    if nowe == 0 || nowe >= WHOLE_BP {
        return false;
    }
    let zostaje = WHOLE_BP - nowe;
    let mut suma = 0u32;
    for s in firm.owners.iter_mut() {
        let po = (u64::from(s.bp) * u64::from(zostaje) / u64::from(WHOLE_BP)) as u32;
        s.bp = po as u16;
        suma += po;
    }
    let mut reszta = zostaje - suma;
    firm.owners.sort_by_key(|s| owner_key(s.owner));
    for s in firm.owners.iter_mut() {
        if reszta == 0 {
            break;
        }
        s.bp += 1;
        reszta -= 1;
    }
    match firm.owners.iter_mut().find(|s| s.owner == to) {
        Some(s) => s.bp += bp,
        None => firm.owners.push(magnat_firms::OwnerShare { owner: to, bp }),
    }
    firm.owners.retain(|s| s.bp > 0);
    firm.owners.sort_by_key(|s| owner_key(s.owner));
    true
}

/// Podział dywidendy między właścicieli — **co do grosza**.
///
/// Zwraca listę `(właściciel, kwota)` posortowaną po kluczu właściciela. Suma równa
/// się `total` dokładnie: reszta z dzielenia trafia do pierwszego wg posortowanego
/// klucza (00 §2), bo inaczej co miesiąc gubiłoby się kilka groszy na firmę,
/// a test własnościowy pieniądza nie wybacza ani jednego.
#[must_use]
pub fn split_dividend(firm: &Firm, total: Money) -> Vec<(Owner, Money)> {
    if total.get() <= 0 || firm.owners.is_empty() {
        return Vec::new();
    }
    let mut udzialy: Vec<(Owner, u32)> = firm
        .owners
        .iter()
        .map(|s| (s.owner, u32::from(s.bp)))
        .collect();
    udzialy.sort_by_key(|(o, _)| owner_key(*o));
    let suma: u32 = udzialy.iter().map(|(_, bp)| *bp).sum();
    if suma == 0 {
        return Vec::new();
    }
    let mut out: Vec<(Owner, Money)> = Vec::with_capacity(udzialy.len());
    let mut rozdane: i64 = 0;
    for (o, bp) in &udzialy {
        let kwota = total.get() as i128 * i128::from(*bp) / i128::from(suma);
        let kwota = kwota as i64;
        rozdane += kwota;
        out.push((*o, Money(kwota)));
    }
    let reszta = total.get() - rozdane;
    if reszta != 0 {
        if let Some(pierwszy) = out.first_mut() {
            pierwszy.1 = Money(pierwszy.1.get() + reszta);
        }
    }
    out
}

/// Powód do dziennika decyzji firmy po fixingu.
#[must_use]
pub fn fixing_reason(firm: magnat_core::FirmId, price: Money) -> DecisionReason {
    DecisionReason::Firm(FirmReason::StockFixing { firm, price })
}

#[cfg(test)]
mod tests {
    use super::*;
    use magnat_core::{CitizenId, Entity, SimMinute};
    use magnat_firms::FirmKey;
    use smallvec::smallvec;

    fn obywatel(i: u32) -> Owner {
        Owner::Citizen(CitizenId(Entity::new(i, std::num::NonZeroU32::MIN)))
    }

    fn firma() -> Firm {
        Firm::sole_owner(
            FirmKey(1),
            "Test".to_string(),
            SimMinute(0),
            magnat_core::DistrictId(0),
            obywatel(1),
        )
    }

    #[test]
    fn przeniesienie_udzialu_nie_rusza_sumy() {
        let mut f = firma();
        assert_eq!(move_stake(&mut f, obywatel(1), obywatel(2), 600), 600);
        assert_eq!(stake_of(&f, obywatel(1)), 9_400);
        assert_eq!(stake_of(&f, obywatel(2)), 600);
        assert!(f.owners_sum_ok());
    }

    #[test]
    fn nie_da_sie_sprzedac_wiecej_niz_sie_ma() {
        let mut f = firma();
        assert_eq!(move_stake(&mut f, obywatel(2), obywatel(3), 100), 0);
        assert!(f.owners_sum_ok());
    }

    #[test]
    fn emisja_rozwadnia_proporcjonalnie_i_domyka_sume() {
        let mut f = firma();
        f.owners = smallvec![
            magnat_firms::OwnerShare {
                owner: obywatel(1),
                bp: 3_333
            },
            magnat_firms::OwnerShare {
                owner: obywatel(2),
                bp: 3_333
            },
            magnat_firms::OwnerShare {
                owner: obywatel(3),
                bp: 3_334
            },
        ];
        assert!(dilute(&mut f, obywatel(9), 1_000));
        assert!(f.owners_sum_ok(), "suma po emisji: {:?}", f.owners);
        assert_eq!(stake_of(&f, obywatel(9)), 1_000);
        // Proporcje między starymi właścicielami zostają: 3333/3333/3334 → 3000/3000/3000
        // plus jedna bp reszty do pierwszego wg klucza.
        let a = stake_of(&f, obywatel(1));
        let b = stake_of(&f, obywatel(2));
        let c = stake_of(&f, obywatel(3));
        assert_eq!(a + b + c, 9_000);
        assert!(a.abs_diff(c) <= 1, "{a} vs {c}");
    }

    #[test]
    fn dywidenda_dzieli_sie_bez_utraty_grosza() {
        let mut f = firma();
        f.owners = smallvec![
            magnat_firms::OwnerShare {
                owner: obywatel(1),
                bp: 3_333
            },
            magnat_firms::OwnerShare {
                owner: obywatel(2),
                bp: 3_333
            },
            magnat_firms::OwnerShare {
                owner: obywatel(3),
                bp: 3_334
            },
        ];
        for kwota in [1i64, 7, 100, 999_999, 1_234_567] {
            let podzial = split_dividend(&f, Money(kwota));
            let suma: i64 = podzial.iter().map(|(_, m)| m.get()).sum();
            assert_eq!(suma, kwota, "kwota {kwota}");
        }
    }
}
