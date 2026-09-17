//! Domknięcie podatkowe i arytmetyka danin (M8a, test T1 na poziomie rejestru).
//!
//! T1 z dokumentu fazy ma dwie części i **ta pierwsza nie potrzebuje miasta**:
//! „dla każdej pary (danina, okres) `Σ Assessed = Σ Settled + Σ Overdue + Σ Abated`"
//! jest własnością rejestru należności, nie własnością gospodarki. Sprawdzamy ją
//! tu, na losowych ciągach naliczeń i przejść — bo miasto stawiane pod test
//! sprawdziłoby przy okazji sto innych rzeczy i przy czerwonym wyniku nie dałoby się
//! powiedzieć, która z nich pękła.
//!
//! Część druga (`Σ Settled == Δ CityBudget.revenue_ytd` w prawdziwym mieście)
//! siedzi w scenariuszu `m8miasto` i w `tools/headless/tests/city_tax.rs`.

use magnat_city::{
    cit_due, late_interest, pit_annual, pit_withheld, property_tax_month, property_tax_year,
    vat_add_to_net, vat_from_gross, ChargeRegistry, FiscalPeriod, TaxCode, TaxPayer,
};
use magnat_core::{AbateReason, Entity, Mass, Money, SiteId, TaxKind, Tick, TAX_KIND_COUNT};
use proptest::prelude::*;

fn site(i: u32) -> TaxPayer {
    TaxPayer::Site(SiteId(Entity::new(i, std::num::NonZeroU32::MIN)))
}

fn kodeks() -> TaxCode {
    TaxCode::load_default().expect("data/city/tax.ron")
}

proptest! {
    /// Rejestr domyka się po **dowolnym** ciągu naliczeń i przejść.
    ///
    /// Losowany jest nie tylko rozkład kwot, ale i to, co się z należnością dzieje:
    /// zapłata, zaległość, zapłata po zaległości, umorzenie i podwójne wywołanie
    /// każdego z nich. Ostatnie jest tu najważniejsze — to właśnie podwójne
    /// rozliczenie tej samej należności umiałoby dopisać kwotę dwa razy i przejść
    /// niezauważone w przebiegu, w którym sumy i tak rosną.
    #[test]
    fn rejestr_domyka_sie_po_dowolnym_ciagu(
        kroki in prop::collection::vec((0u32..40, 0u8..7, 1i64..5_000_000, 0u8..6), 1..300)
    ) {
        let mut r = ChargeRegistry::new();
        let mut ids = Vec::new();
        for (platnik, danina, kwota, co) in kroki {
            let kind = TaxKind::from_index(usize::from(danina)).expect("danina z zakresu");
            let okres = FiscalPeriod::Month(1, 1 + u8::try_from(platnik % 12).unwrap_or(0));
            if let Some(id) = r.accrue(
                site(platnik),
                kind,
                okres,
                Money(kwota * 4),
                Mass(0),
                1000,
                Money(kwota),
                Tick(100),
                Tick(200),
            ) {
                ids.push(id);
            }
            let Some(id) = ids.get(platnik as usize % ids.len().max(1)).copied() else {
                continue;
            };
            match co {
                0 => r.mark_settled(id, Tick(300)),
                1 => {
                    r.mark_overdue(id, Tick(300), Money(7));
                }
                2 => {
                    r.mark_overdue(id, Tick(300), Money(7));
                    r.mark_settled(id, Tick(400));
                }
                3 => r.abate(id, Tick(300), AbateReason::Bankruptcy),
                4 => {
                    r.mark_settled(id, Tick(300));
                    r.mark_settled(id, Tick(400));
                }
                _ => {
                    r.abate(id, Tick(300), AbateReason::Council);
                    r.mark_settled(id, Tick(400));
                }
            }
            prop_assert!(r.check_closure().is_ok(), "{:?}", r.check_closure());
        }
        // Druga strona równania: suma zapłaconych po daninach zgadza się z sumą
        // po okresach. Dwa liczniki tej samej liczby są tu z rozmysłu — budżet
        // czyta pierwszy, a test T1 grupuje po drugim.
        let mut po_daninach = 0i64;
        for k in TaxKind::ALL {
            po_daninach += r.settled_of(*k).get();
        }
        let po_okresach: i64 = r.periods().map(|(_, _, t)| t.settled.get()).sum();
        prop_assert_eq!(po_daninach, po_okresach);
    }

    /// VAT wyłuskany z brutto nigdy nie przekracza kwoty przelewu i nigdy nie jest
    /// ujemny. To nie jest estetyka: `Books::transfer` odrzuca `tax > amount`,
    /// więc złamanie tego zatrzymałoby **sprzedaż**, a nie tylko podatek.
    #[test]
    fn vat_miesci_sie_w_kwocie(brutto in 0i64..1_000_000_000i64, bp in 0u32..10_000u32) {
        let v = vat_from_gross(Money(brutto), bp);
        prop_assert!(v.get() >= 0);
        prop_assert!(v.get() <= brutto);
        // Droga powrotna różni się najwyżej o grosz zaokrąglenia.
        let netto = Money(brutto - v.get());
        let z_powrotem = vat_add_to_net(netto, bp);
        prop_assert!((z_powrotem.get() - brutto).abs() <= 1);
    }

    /// Dwanaście rat podatku od nieruchomości sumuje się **dokładnie** do kwoty
    /// rocznej — dla każdej wartości katastralnej i każdej stawki. To jest ryzyko
    /// `R5` sprawdzone wprost: dryf zaokrągleń nie ma gdzie powstać, bo rata jest
    /// różnicą sum, a nie dwunastym ułamkiem.
    #[test]
    fn raty_nieruchomosci_sumuja_sie_do_roku(
        wartosc in 0i64..100_000_000_000i64,
        bp in 0u32..2_000u32,
    ) {
        let roczny = property_tax_year(Money(wartosc), bp);
        let suma: i64 = (1..=12).map(|m| property_tax_month(Money(wartosc), bp, m).get()).sum();
        prop_assert_eq!(suma, roczny.get());
    }

    /// Suma dwunastu zaliczek PIT równa się podatkowi rocznemu co do grosza —
    /// niezależnie od tego, jak w ciągu roku skakała pensja.
    #[test]
    fn zaliczki_pit_sumuja_sie_do_podatku_rocznego(
        pensje in prop::collection::vec(0i64..5_000_000i64, 12..=12)
    ) {
        let c = kodeks();
        let mut ytd_brutto = Money::ZERO;
        let mut ytd_pobrane = Money::ZERO;
        for (i, p) in pensje.iter().enumerate() {
            let z = pit_withheld(
                Money(*p),
                ytd_brutto,
                ytd_pobrane,
                u8::try_from(i + 1).unwrap_or(12),
                &c,
            );
            ytd_brutto = Money(ytd_brutto.get() + p);
            ytd_pobrane = Money(ytd_pobrane.get() + z.get());
        }
        let roczny = pit_annual(ytd_brutto, Money(c.pit_free_allowance), &c.pit_brackets);
        prop_assert_eq!(ytd_pobrane.get(), roczny.get());
    }

    /// CIT nie gubi ani nie tworzy straty: to, co odliczono, zeszło z zapasu.
    #[test]
    fn cit_nie_gubi_straty(
        wynik in -10_000_000i64..10_000_000i64,
        strata in 0i64..10_000_000i64,
        bp in 0u32..5_000u32,
    ) {
        let (podatek, reszta) = cit_due(Money(wynik), Money(strata), bp);
        prop_assert!(podatek.get() >= 0);
        prop_assert!(reszta.get() >= 0);
        if wynik > 0 {
            let odliczone = strata.min(wynik);
            prop_assert_eq!(reszta.get(), strata - odliczone);
            // Podatek liczy się od podstawy po odliczeniu, nigdy od wyniku brutto.
            prop_assert!(podatek.get() <= Money(wynik - odliczone).mul_ratio(i64::from(bp), 10_000).get());
        } else {
            prop_assert_eq!(podatek.get(), 0);
            prop_assert_eq!(reszta.get(), strata - wynik);
        }
    }
}

/// Odsetki za rok są dokładnie stawką roczną — kalendarz 360-dniowy (`K-1`) nie
/// zostawia tu reszty i to jest jeden z powodów, dla których jest 12 × 30.
#[test]
fn odsetki_za_pelny_rok_to_stawka_roczna() {
    for kwota in [100i64, 1_000_000, 99_999_999] {
        let r = late_interest(Money(kwota), 1450, 360);
        assert_eq!(r, Money(kwota).mul_ratio(1450, 10_000));
    }
}

/// Kodeks z repozytorium jest poprawny i pokrywa **wszystkie** domeny katalogu.
///
/// Test ładujący prawdziwy plik, a nie wymyśloną tabelę: literówka w `data/city/tax.ron`
/// ma pękać w CI, a nie u gracza w trzecim miesiącu gry.
#[test]
fn kodeks_z_repozytorium_pokrywa_caly_katalog() {
    let c = kodeks();
    let katalog = magnat_supply::catalog::load_default("contemporary").expect("data/goods/");
    let stawki = c.resolve(&katalog).expect("każda domena ma klasę VAT");
    assert!(
        stawki.vat_goods() > 0,
        "kodeks nie obkłada VAT-em ani jednego towaru"
    );
    assert!(
        stawki.excise_goods() > 0,
        "kodeks nie obkłada akcyzą ani jednego towaru"
    );
    // Siedem danin i ani jednej więcej — tablica wpływów budżetu jest tej długości.
    assert_eq!(TAX_KIND_COUNT, 7);
    assert!(c.cit_bp > 0 && c.property_bp_per_year > 0);
    assert!(!c.pit_brackets.is_empty() && !c.licenses.is_empty());
}
