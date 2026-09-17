//! Funkcje naliczające daniny (M8a §5.1).
//!
//! Wszystkie są **czyste**: biorą liczby, oddają liczbę, nie widzą świata i nie
//! zapisują niczego. Dzięki temu test własnościowy sprawdza je bez stawiania miasta,
//! a `ChargeRegistry` nie musi powtarzać arytmetyki, żeby wyjaśnić kwotę.
//!
//! Zero floatów i zero `det_math` (M8a §5.0). Zaokrąglenie jest jawne i jedno:
//! `Money::mul_ratio` liczy w `i128` i zaokrągla pół w górę co do wartości
//! bezwzględnej. Dryf, przed którym broni ryzyko `R5`, bierze się nie z samego
//! zaokrąglenia, tylko z **powtarzania** go na dwunastu ratach — dlatego PIT
//! i podatek od nieruchomości liczą się narastająco, jako różnica sum, a nie jako
//! dwanaście niezależnych działań.

use magnat_core::{Mass, Money};

use crate::code::{PitBracket, TaxCode};

/// VAT wyłuskany z ceny brutto (`K-7`: cena detaliczna jest tym, co płaci kupujący).
///
/// `vat = round(gross * bp / (10 000 + bp))`. Mianownik jest większy od licznika,
/// więc wynik nigdy nie przekroczy kwoty — `Books::transfer` odrzuciłby `tax > amount`.
#[must_use]
pub fn vat_from_gross(gross: Money, bp: u32) -> Money {
    if bp == 0 || gross.get() == 0 {
        return Money::ZERO;
    }
    gross.mul_ratio(i64::from(bp), 10_000 + i64::from(bp))
}

/// Cena brutto z netto. Odwrotność [`vat_from_gross`] z dokładnością do grosza
/// zaokrąglenia — i to jest kierunek, w którym liczy sklep (`K-7`).
#[must_use]
pub fn vat_add_to_net(net: Money, bp: u32) -> Money {
    if bp == 0 {
        return net;
    }
    Money(net.get() + net.mul_ratio(i64::from(bp), 10_000).get())
}

/// CIT od dochodu rocznego, ze stratą z lat ubiegłych rozliczaną w przód.
///
/// Zwraca `(należność, strata pozostała do rozliczenia)`. Strata jest **dodatnia**
/// po obu stronach: „mam 30 tys. straty do odliczenia", nie „mam −30 tys. zysku".
#[must_use]
pub fn cit_due(taxable_profit: Money, loss_carry: Money, bp: u32) -> (Money, Money) {
    let zysk = taxable_profit.get();
    let strata = loss_carry.get().max(0);
    if zysk <= 0 {
        // Rok na minusie powiększa stratę do rozliczenia; podatku nie ma.
        return (Money::ZERO, Money(strata.saturating_sub(zysk)));
    }
    let odliczone = strata.min(zysk);
    let podstawa = Money(zysk - odliczone);
    (
        podstawa.mul_ratio(i64::from(bp), 10_000),
        Money(strata - odliczone),
    )
}

/// Podatek roczny ze skali progresywnej, po odjęciu kwoty wolnej.
#[must_use]
pub fn pit_annual(annual_gross: Money, free_allowance: Money, brackets: &[PitBracket]) -> Money {
    let podstawa = annual_gross.get() - free_allowance.get();
    if podstawa <= 0 {
        return Money::ZERO;
    }
    let mut razem: i64 = 0;
    let mut dol: i64 = 0;
    for b in brackets {
        let gora = if b.upper > 0 { b.upper } else { i64::MAX };
        let w_progu = podstawa.min(gora) - dol;
        if w_progu <= 0 {
            break;
        }
        razem += Money(w_progu).mul_ratio(i64::from(b.bp), 10_000).get();
        dol = gora;
        if podstawa <= gora {
            break;
        }
    }
    Money(razem)
}

/// Zaliczka pobierana u źródła przy wypłacie, liczona **narastająco i w skali roku**.
///
/// `months` to numer tej wypłaty w roku podatkowym, 1..=12.
///
/// Krok jest trzyczęściowy i każda część ma powód:
/// 1. dochód od początku roku **skaluje się do roku** (`× 12 / months`) — bez tego
///    kwota wolna zjadałaby całą podstawę przez pierwsze pół roku i pracownik
///    płaciłby zero do lipca, a od sierpnia potrójnie;
/// 2. od dochodu rocznego liczy się podatek ze skali;
/// 3. należna część roku minus to, co już pobrano.
///
/// Przy dwunastej wypłacie skalowanie jest tożsamością, więc **suma dwunastu
/// zaliczek równa się podatkowi rocznemu co do grosza** — niezależnie od tego, jak
/// w ciągu roku skakała pensja. To jest ta sama własność co przy podatku od
/// nieruchomości i z tego samego powodu: dwanaście rat to jedna kwota podzielona,
/// a nie dwanaście niezależnych zaokrągleń.
///
/// Wynik może być **ujemny**, gdy dochód spadł i pobrano za dużo; wołający ma
/// wtedy oddać, a nie przyciąć do zera. Przycięcie znaczyłoby, że suma zaliczek
/// przekracza podatek roczny i test T1 nie domknąłby się do zera.
#[must_use]
pub fn pit_withheld(
    gross_wage: Money,
    ytd_gross: Money,
    ytd_withheld: Money,
    months: u8,
    code: &TaxCode,
) -> Money {
    let m = i64::from(months.clamp(1, 12));
    let narastajaco = Money(ytd_gross.get() + gross_wage.get());
    let w_skali_roku = narastajaco.mul_ratio(12, m);
    let roczny = pit_annual(
        w_skali_roku,
        Money(code.pit_free_allowance),
        &code.pit_brackets,
    );
    let nalezne_do_teraz = roczny.mul_ratio(m, 12);
    Money(nalezne_do_teraz.get() - ytd_withheld.get())
}

/// Podatek od nieruchomości za `month` (1..=12) roku podatkowego.
///
/// Metoda różnicy skumulowanej: `annual*m/12 − annual*(m−1)/12`. Dwanaście rat
/// sumuje się dokładnie do kwoty rocznej, a reszta z dzielenia ląduje w ostatnich
/// miesiącach — nie ma dryfu, bo nie ma dwunastu niezależnych zaokrągleń.
#[must_use]
pub fn property_tax_month(cadastral_value: Money, bps_per_year: u32, month: u8) -> Money {
    if month == 0 || month > 12 {
        return Money::ZERO;
    }
    let roczny = cadastral_value
        .mul_ratio(i64::from(bps_per_year), 10_000)
        .get();
    let do_teraz = roczny * i64::from(month) / 12;
    let do_poprzedniego = roczny * i64::from(month - 1) / 12;
    Money(do_teraz - do_poprzedniego)
}

/// Podatek od nieruchomości za cały rok — punkt odniesienia dla testu własnościowego
/// „suma dwunastu rat równa się kwocie rocznej".
#[must_use]
pub fn property_tax_year(cadastral_value: Money, bps_per_year: u32) -> Money {
    cadastral_value.mul_ratio(i64::from(bps_per_year), 10_000)
}

/// Akcyza od partii wyrobu. Stawka jest **kwotowa** — grosze za kilogram.
#[must_use]
pub fn excise_due(mass: Mass, per_kg: Money) -> Money {
    if per_kg.get() == 0 || mass.0 <= 0 {
        return Money::ZERO;
    }
    // Masa jest w gramach, stawka za kilogram — dzielenie przez 1000 z zaokrągleniem
    // pół w górę, jak wszędzie indziej w pieniądzu.
    Money(per_kg.get()).mul_ratio(mass.0, 1_000)
}

/// Cło ad valorem od wartości celnej. Stawki niesie `data/trade/tariffs.ron`
/// (`K-37`, właścicielem wartości jest od M8 miasto).
#[must_use]
pub fn duty_due(customs_value: Money, bp: u32) -> Money {
    if bp == 0 {
        return Money::ZERO;
    }
    customs_value.mul_ratio(i64::from(bp), 10_000)
}

/// Odsetki za zwłokę, naliczane dobowo od kwoty głównej.
///
/// Rok kalendarza gry ma 360 dób (`K-1`), więc dzielnik jest dokładny i nie ma
/// tu reszty kalendarzowej — to jest jeden z powodów, dla których kalendarz ma
/// 12 × 30, a nie gregoriański.
#[must_use]
pub fn late_interest(principal: Money, bp_per_year: u32, days: u32) -> Money {
    if bp_per_year == 0 || days == 0 || principal.get() <= 0 {
        return Money::ZERO;
    }
    principal.mul_ratio(i64::from(bp_per_year) * i64::from(days), 10_000 * 360)
}

/// Opłata koncesyjna: kwota z kodeksu, bez proporcji do części roku.
///
/// Świadomie bez proporcji — koncesja jest prawem do prowadzenia działalności
/// w danym roku, a nie usługą rozliczaną za dobę. Zakład otwarty w grudniu płaci
/// tyle samo co otwarty w styczniu i to jest w grze widoczna decyzja, kiedy wejść.
#[must_use]
pub fn license_fee(fee_per_year: Money) -> Money {
    fee_per_year
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kodeks() -> TaxCode {
        TaxCode {
            schema_version: crate::code::TAX_SCHEMA_VERSION,
            cit_bp: 1900,
            loss_carry_years: 5,
            pit_free_allowance: 3_000_000,
            pit_brackets: vec![
                PitBracket {
                    upper: 12_000_000,
                    bp: 1200,
                },
                PitBracket { upper: 0, bp: 3200 },
            ],
            vat_classes: Vec::new(),
            property_bp_per_year: 100,
            excise: Vec::new(),
            excise_energy: Vec::new(),
            licenses: Vec::new(),
            vat_due_day: 20,
            pit_due_day: 20,
            property_due_day: 15,
            excise_due_day: 25,
            cit_due_month: 3,
            late_interest_bp_per_year: 1450,
            time_bar_days: 1800,
            effective_from: magnat_core::Tick(0),
            excise_scale_bp: 10_000,
        }
    }

    #[test]
    fn vat_z_brutto_i_z_netto_to_ta_sama_transakcja() {
        // 123 zł brutto przy 23 % to 23 zł podatku i 100 zł netto.
        let brutto = Money(12_300);
        let vat = vat_from_gross(brutto, 2300);
        assert_eq!(vat, Money(2_300));
        assert_eq!(
            vat_add_to_net(Money(brutto.get() - vat.get()), 2300),
            brutto
        );
    }

    #[test]
    fn vat_nigdy_nie_przekracza_kwoty_przelewu() {
        // `Books::transfer` odrzuca `tax > amount`, więc to nie jest estetyka.
        for bp in [0u32, 500, 2300, 9999] {
            for g in [1i64, 7, 99, 100_000, i64::from(u32::MAX)] {
                let v = vat_from_gross(Money(g), bp);
                assert!(v.get() >= 0 && v.get() <= g, "bp={bp} gross={g} vat={v:?}");
            }
        }
    }

    #[test]
    fn cit_odlicza_strate_i_nie_gubi_reszty() {
        // Zysk 100 tys., strata 40 tys. → podstawa 60 tys., strata zjedzona.
        let (podatek, strata) = cit_due(Money(10_000_000), Money(4_000_000), 1900);
        assert_eq!(podatek, Money(1_140_000));
        assert_eq!(strata, Money::ZERO);
        // Rok na minusie: podatku nie ma, strata rośnie o wynik.
        let (podatek, strata) = cit_due(Money(-2_000_000), Money(1_000_000), 1900);
        assert_eq!(podatek, Money::ZERO);
        assert_eq!(strata, Money(3_000_000));
        // Strata większa od zysku: podatku nie ma, reszta straty zostaje.
        let (podatek, strata) = cit_due(Money(1_000_000), Money(4_000_000), 1900);
        assert_eq!(podatek, Money::ZERO);
        assert_eq!(strata, Money(3_000_000));
    }

    #[test]
    fn pit_jest_progresywny_i_ma_kwote_wolna() {
        let c = kodeks();
        // Dochód poniżej kwoty wolnej — zero.
        assert_eq!(
            pit_annual(
                Money(2_500_000),
                Money(c.pit_free_allowance),
                &c.pit_brackets
            ),
            Money::ZERO
        );
        // 90 tys. zł rocznie: podstawa 60 tys. zł, cała w pierwszym progu.
        assert_eq!(
            pit_annual(
                Money(9_000_000),
                Money(c.pit_free_allowance),
                &c.pit_brackets
            ),
            Money(720_000)
        );
        // 200 tys. zł: podstawa 170 tys. zł — 120 tys. po 12 %, 50 tys. po 32 %.
        assert_eq!(
            pit_annual(
                Money(20_000_000),
                Money(c.pit_free_allowance),
                &c.pit_brackets
            ),
            Money(1_440_000 + 1_600_000)
        );
    }

    #[test]
    fn suma_zaliczek_pit_rowna_sie_podatkowi_rocznemu() {
        // Sedno `R5`: pensja skacze w środku roku, a domknięcie ma być co do grosza.
        let c = kodeks();
        let pensje = [
            700_000, 700_000, 700_000, 1_900_000, 1_900_000, 1_900_000, 1_900_000, 1_900_000,
            300_000, 300_000, 300_000, 300_000,
        ];
        let mut ytd_brutto = Money::ZERO;
        let mut ytd_pobrane = Money::ZERO;
        for (i, p) in pensje.into_iter().enumerate() {
            let z = pit_withheld(Money(p), ytd_brutto, ytd_pobrane, (i + 1) as u8, &c);
            ytd_brutto = Money(ytd_brutto.get() + p);
            ytd_pobrane = Money(ytd_pobrane.get() + z.get());
        }
        assert_eq!(
            ytd_pobrane,
            pit_annual(ytd_brutto, Money(c.pit_free_allowance), &c.pit_brackets)
        );
    }

    #[test]
    fn dwanascie_rat_nieruchomosci_sumuje_sie_do_roku() {
        // Wartość dobrana tak, żeby roczny podatek NIE dzielił się przez 12.
        let wartosc = Money(23_456_789);
        let roczny = property_tax_year(wartosc, 100);
        let suma: i64 = (1..=12)
            .map(|m| property_tax_month(wartosc, 100, m).get())
            .sum();
        assert_eq!(suma, roczny.get());
        assert_eq!(property_tax_month(wartosc, 100, 0), Money::ZERO);
        assert_eq!(property_tax_month(wartosc, 100, 13), Money::ZERO);
    }

    #[test]
    fn akcyza_jest_kwotowa() {
        // 40 ton paliwa po 2,10 zł/kg = 84 000 zł.
        assert_eq!(excise_due(Mass(40_000_000), Money(210)), Money(8_400_000));
        assert_eq!(excise_due(Mass(0), Money(210)), Money::ZERO);
        assert_eq!(excise_due(Mass(40_000_000), Money(0)), Money::ZERO);
    }

    #[test]
    fn odsetki_licza_sie_na_roku_360_dobowym() {
        // 100 000 zł, 14,5 % rocznie, 360 dób = dokładnie 14 500 zł.
        assert_eq!(
            late_interest(Money(10_000_000), 1450, 360),
            Money(1_450_000)
        );
        assert_eq!(late_interest(Money(10_000_000), 1450, 0), Money::ZERO);
    }
}
