//! Naliczanie danin: kto, ile, za jaki okres i do kiedy (M8a WP2).
//!
//! Każda z siedmiu danin ma tutaj **jedno** miejsce naliczenia i jedno tylko.
//! Kwotę liczy czysta funkcja z [`crate::calc`], a ta funkcja nie widzi świata —
//! dzięki temu „dlaczego tyle" da się odpowiedzieć bez odtwarzania stanu z chwili
//! naliczenia, a test własnościowy sprawdza arytmetykę bez stawiania miasta.
//!
//! **Moment naliczenia jest kontraktem** (M8a §5.1, tabela „Kontrakt księgowania"):
//! VAT i akcyza schodzą z deklaracji miesięcznej, PIT z listy płac, podatek od
//! nieruchomości pierwszego dnia miesiąca, CIT z domknięcia roku obrotowego,
//! koncesja z początkiem roku licencyjnego, cło przy odprawie.

use magnat_core::{DecisionReason, Mass, Money, SimCalendar, SiteId, TaxKind, Tick};
use magnat_economy::Market;

use crate::calc::{cit_due, property_tax_month};
use crate::charge::{FiscalPeriod, TaxPayer};
use crate::city::{due_on_day, City};

/// Okres miesięczny, który właśnie się skończył. Wołane na granicy miesiąca,
/// czyli w pierwszej minucie miesiąca następnego.
#[must_use]
pub fn poprzedni_miesiac(t: Tick) -> FiscalPeriod {
    let cal = SimCalendar::new(t);
    let (rok, mies) = (cal.year(), cal.month_of_year());
    if mies <= 1 {
        FiscalPeriod::Month(u16::try_from(rok.saturating_sub(1)).unwrap_or(0), 12)
    } else {
        FiscalPeriod::Month(u16::try_from(rok).unwrap_or(0), mies - 1)
    }
}

/// VAT i akcyza z deklaracji miesięcznej.
///
/// Sklep naliczał je transakcja po transakcji (`Market::take_tax_accrued`),
/// ale **należnością stają się raz w miesiącu** — inaczej gracz dostałby cztery
/// tysiące wierszy zamiast dwóch (`R8`), a rejestr rósłby jak dziennik transakcji.
pub fn vat_i_akcyza(city: &mut City, market: &Market, t: Tick) -> (Money, Money) {
    let okres = poprzedni_miesiac(t);
    let termin_vat = due_on_day(t, city.code.vat_due_day);
    let termin_akcyza = due_on_day(t, city.code.excise_due_day);
    let mut razem_vat = Money::ZERO;
    let mut razem_akcyza = Money::ZERO;
    for (site, a) in market.take_tax_accrued() {
        if a.vat.get() != 0 {
            // Stawka w migawce jest stawką **ważoną obrotem**, a nie stawką jednego
            // towaru: deklaracja obejmuje cały asortyment, a w nim stoi obok siebie
            // chleb po 5 % i papierosy po 23 %. Karta inspekcji ma pokazać tę, którą
            // sklep faktycznie zapłacił, a nie którąś z tabeli.
            let stawka = stawka_z_kwoty(a.vat, a.vat_base);
            city.charges.accrue(
                TaxPayer::Site(site),
                TaxKind::Vat,
                okres,
                a.vat_base,
                Mass(0),
                stawka,
                a.vat,
                t,
                termin_vat,
            );
            razem_vat = Money(razem_vat.get() + a.vat.get());
        }
        if a.excise.get() != 0 {
            city.charges.accrue(
                TaxPayer::Site(site),
                TaxKind::Excise,
                okres,
                a.excise,
                a.excise_mass,
                0,
                a.excise,
                t,
                termin_akcyza,
            );
            razem_akcyza = Money(razem_akcyza.get() + a.excise.get());
        }
    }
    (razem_vat, razem_akcyza)
}

fn stawka_z_kwoty(kwota: Money, podstawa: Money) -> u32 {
    if podstawa.get() <= 0 {
        return 0;
    }
    u32::try_from(kwota.get().saturating_mul(10_000) / podstawa.get()).unwrap_or(u32::MAX)
}

/// Zaliczki PIT pobrane u źródła przez pracodawców w minionym miesiącu.
///
/// Płatnikiem jest `TaxPayer::External`, i to jest **stan przejściowy nazwany
/// wprost**: do czasu, aż lista płac zacznie ruszać pieniądz z konta zakładu
/// (`BF-7`), pracodawcą jest abstrakcyjna „reszta świata" i to z jej kanału
/// miasto zabiera zaliczkę. Kwota i moment są już dziś prawdziwe; zmieni się
/// wyłącznie konto, z którego przelew wychodzi.
pub fn pit(city: &mut City, t: Tick) -> Money {
    let (podstawa, podatek) = city.withholding.take_pending();
    if podatek.get() == 0 {
        return Money::ZERO;
    }
    let okres = poprzedni_miesiac(t);
    let termin = due_on_day(t, city.code.pit_due_day);
    city.charges.accrue(
        TaxPayer::External,
        TaxKind::Pit,
        okres,
        podstawa,
        Mass(0),
        stawka_z_kwoty(podatek, podstawa),
        podatek,
        t,
        termin,
    );
    podatek
}

/// Rata podatku od nieruchomości. Metoda różnicy skumulowanej, więc dwanaście rat
/// sumuje się dokładnie do kwoty rocznej — żadna nie jest „resztą po zaokrągleniu".
pub fn nieruchomosci(city: &mut City, market: &Market, t: Tick) -> Money {
    if city.cadastre.is_empty() || city.code.property_bp_per_year == 0 {
        return Money::ZERO;
    }
    let cal = SimCalendar::new(t);
    let okres = FiscalPeriod::Month(
        u16::try_from(cal.year()).unwrap_or(0),
        cal.month_of_year().max(1),
    );
    let termin = due_on_day(t, city.code.property_due_day);
    let bp = city.code.property_bp_per_year;
    let miesiac = cal.month_of_year().max(1);
    let mut razem = Money::ZERO;
    let wpisy: Vec<(SiteId, Money)> = city.cadastre.iter().map(|c| (c.site, c.value)).collect();
    for (site, wartosc) in wpisy {
        let rata = property_tax_month(wartosc, bp, miesiac);
        if rata.get() == 0 {
            continue;
        }
        city.charges.accrue(
            TaxPayer::Site(site),
            TaxKind::Property,
            okres,
            wartosc,
            Mass(0),
            bp,
            rata,
            t,
            termin,
        );
        market.post_tax_accrual(site, rata, powod(TaxKind::Property, bp, rata));
        razem = Money(razem.get() + rata.get());
    }
    razem
}

/// CIT od wyniku minionego roku obrotowego, ze stratą rozliczaną w przód.
///
/// Podstawą jest suma **domkniętych** miesięcy roku, a nie bieżący stan księgi:
/// dziennik zakładu jest pierścieniem i po roku nie pamięta stycznia, a migawki
/// domknięć pamiętają.
pub fn cit(city: &mut City, market: &Market, sites: &[SiteId], t: Tick) -> Money {
    if city.code.cit_bp == 0 {
        return Money::ZERO;
    }
    let cal = SimCalendar::new(t);
    let rok_podatkowy = cal.year().saturating_sub(1);
    let okres = FiscalPeriod::Year(u16::try_from(rok_podatkowy).unwrap_or(0));
    let od = Tick(u64::from(rok_podatkowy) * 518_400);
    let do_ = Tick(od.0 + 518_400);
    let termin = Tick(
        u64::from(cal.year()) * 518_400
            + u64::from(city.code.cit_due_month.clamp(1, 12) - 1) * 43_200,
    );
    let mut razem = Money::ZERO;
    for site in sites {
        let Some(wynik) = market.closed_result(*site, od, do_) else {
            continue;
        };
        let strata = city.loss_of(*site);
        let (podatek, reszta_straty) = cit_due(wynik, strata, city.code.cit_bp);
        city.set_loss(*site, reszta_straty);
        if podatek.get() == 0 {
            continue;
        }
        city.charges.accrue(
            TaxPayer::Site(*site),
            TaxKind::Cit,
            okres,
            wynik,
            Mass(0),
            city.code.cit_bp,
            podatek,
            t,
            termin,
        );
        market.post_tax_accrual(
            *site,
            podatek,
            powod(TaxKind::Cit, city.code.cit_bp, podatek),
        );
        razem = Money(razem.get() + podatek.get());
    }
    razem
}

/// Opłata koncesyjna za rozpoczynający się rok licencyjny.
///
/// Płatna z góry i w całości — koncesja jest prawem do prowadzenia działalności
/// w danym roku, a nie usługą rozliczaną za dobę. Zakład otwarty w grudniu płaci
/// tyle samo co otwarty w styczniu i to jest widoczna w grze decyzja, kiedy wejść.
pub fn koncesje(city: &mut City, market: &Market, sites: &[(SiteId, u16)], t: Tick) -> Money {
    if city.licenses.is_empty() {
        return Money::ZERO;
    }
    let cal = SimCalendar::new(t);
    let okres = FiscalPeriod::Year(u16::try_from(cal.year()).unwrap_or(0));
    let mut razem = Money::ZERO;
    for (site, typ) in sites {
        let Some(oplata) = city.licenses.get(typ).copied() else {
            continue;
        };
        if oplata.get() <= 0 {
            continue;
        }
        city.charges.accrue(
            TaxPayer::Site(*site),
            TaxKind::License,
            okres,
            oplata,
            Mass(0),
            0,
            oplata,
            t,
            t,
        );
        market.post_tax_accrual(*site, oplata, powod(TaxKind::License, 0, oplata));
        razem = Money(razem.get() + oplata.get());
    }
    razem
}

/// Daniny z rozliczeń hurtowych minionej doby: cło i akcyza.
///
/// **Cła nie liczy miasto** — `sim/supply` dolicza je przy wycenie importu i wsadza
/// w koszt nabycia partii, bo tam jest jego miejsce (`K-36`). Tutaj zapisuje się
/// fakt i termin, a terminem jest „natychmiast": cło płaci się na granicy, a nie
/// z deklaracji.
///
/// **Akcyzę liczy miasto** i staje się ona zobowiązaniem kupującego do najbliższej
/// deklaracji miesięcznej. Naliczenie idzie od masy wyrobu zmieniającego właściciela
/// w hurcie — tam, gdzie w rzeczywistości wyrób opuszcza skład podatkowy.
pub fn clo_i_akcyza(city: &mut City, market: &Market, t: Tick) -> (Money, Money) {
    let cal = SimCalendar::new(t);
    let doba = FiscalPeriod::Day(u32::try_from(cal.day_index()).unwrap_or(u32::MAX));
    let miesiac = FiscalPeriod::Month(
        u16::try_from(cal.year()).unwrap_or(0),
        cal.month_of_year().max(1),
    );
    let termin_akcyza = due_on_day(t, city.code.excise_due_day);
    let mut razem_clo = Money::ZERO;
    let mut razem_akcyza = Money::ZERO;
    for (site, b) in market.take_b2b_tax() {
        if b.duty.get() != 0 {
            city.charges.accrue(
                TaxPayer::Site(site),
                TaxKind::Duty,
                doba,
                b.customs_value,
                Mass(0),
                stawka_z_kwoty(b.duty, b.customs_value),
                b.duty,
                t,
                t,
            );
            razem_clo = Money(razem_clo.get() + b.duty.get());
        }
        if b.excise.get() != 0 {
            city.charges.accrue(
                TaxPayer::Site(site),
                TaxKind::Excise,
                miesiac,
                b.excise,
                b.excise_mass,
                0,
                b.excise,
                t,
                termin_akcyza,
            );
            market.post_tax_accrual(site, b.excise, powod(TaxKind::Excise, 0, b.excise));
            razem_akcyza = Money(razem_akcyza.get() + b.excise.get());
        }
    }
    (razem_clo, razem_akcyza)
}

/// Powód naliczenia w postaci strukturalnej — to, co zobaczy karta inspekcji.
#[must_use]
pub fn powod(kind: TaxKind, rate_bp: u32, amount: Money) -> DecisionReason {
    DecisionReason::TaxAssessed {
        kind,
        rate_bp: u16::try_from(rate_bp).unwrap_or(u16::MAX),
        amount,
    }
}
