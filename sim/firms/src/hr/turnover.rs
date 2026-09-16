//! Rotacja: kto odchodzi sam, kogo firma zwalnia i ile to kosztuje (M7b WP6, PRD §7.5).
//!
//! Rotacja jest w tej fazie ryzykiem, nie ozdobą (M7 §8, `R11`): pracownicy krążący
//! między firmami co tydzień zabijają i wydajność, i realizm. Dlatego ciśnienie na
//! odejście liczy się z **czterech niezależnych powodów** i każdy z nich ma własną
//! skalę, a losowanie rozstrzyga wyłącznie „czy dziś" — nie „czy w ogóle".

use magnat_core::Money;

use super::productivity::ManagementQuality;
use super::tuning::{HrTuning, ManagerTuning};

/// Dlaczego pracownik chce odejść i jak bardzo — w **punktach bazowych na dobę**.
///
/// Punkty bazowe, nie promile, i to nie jest kosmetyka. §7.10 fazy stawia rotację
/// roczną w paśmie 8–25 %, czyli dobowe ryzyko rzędu 2–8 bp. W promilach najmniejszą
/// wyrażalną wartością jest 1 ‰ na dobę, czyli **30 % rocznie** — sama jednostka
/// wpychała więc rotację ponad górny kraniec pasma, zanim ktokolwiek cokolwiek
/// skalibrował. Zmierzone: w mieście 4 km promile dawały 9 tys. odejść na 90 dób
/// przy 14 tys. zatrudnionych.
///
/// Zwracamy **parę**, a nie samą liczbę: powód jest wymagany przez kryterium WP6
/// (odejście zawsze ma `DecisionReason` po stronie odchodzącego), a wyprowadzenie go
/// z powrotem z liczby byłoby zgadywaniem.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct QuitPressure {
    pub per_10k: u16,
    pub cause: magnat_core::LeaveCause,
}

/// Staż, poniżej którego świeżo zatrudniony nie odchodzi sam.
///
/// Nie dlatego, że nie chce — dlatego, że koszt zmiany pracy zapłacił tydzień temu.
/// Bez tego progu para „zatrudnij–odejdź" potrafi się zapętlić w obrębie jednej doby.
const MIN_TENURE_DAYS: u32 = 30;

/// Ciśnienie na odejście dobrowolne.
///
/// `wage_gap_bp` to różnica obecnej stawki wobec mediany zawodu w dzielnicy
/// (ujemna = firma płaci poniżej rynku). `mood` i `stress` są z `Vitals`.
#[must_use]
pub fn quit_pressure(
    mood: i8,
    stress: u8,
    tenure_days: u32,
    wage_gap_bp: i32,
    mgmt: ManagementQuality,
    t: &HrTuning,
    mt: &ManagerTuning,
) -> QuitPressure {
    use magnat_core::LeaveCause;
    if tenure_days < MIN_TENURE_DAYS {
        return QuitPressure {
            per_10k: 0,
            cause: LeaveCause::BetterOffer,
        };
    }
    // Trzy niezależne kanały. Wygrywa najsilniejszy i to on nadaje powód —
    // suma dałaby liczbę bez zdania, a zdanie jest tu wymaganiem.
    let z_nastroju = if mood < 0 {
        u32::from(mood.unsigned_abs()) / 4
    } else {
        0
    };
    let ze_stresu = u32::from(stress.saturating_sub(60)) / 2;
    let z_placy = if wage_gap_bp < 0 {
        (-wage_gap_bp / 500) as u32
    } else {
        0
    };
    let baza = u32::from(t.quit_base_per_10k);
    let (najwiekszy, cause) = if z_placy >= z_nastroju && z_placy >= ze_stresu {
        (z_placy, LeaveCause::BetterOffer)
    } else if ze_stresu >= z_nastroju {
        (ze_stresu, LeaveCause::Stress)
    } else {
        (z_nastroju, LeaveCause::Mood)
    };
    // Zarządzanie mnoży **całe** ciśnienie, a nie dokłada osobny kanał: powodem
    // odejścia jest dalej nastrój, stres albo płaca, a menedżer decyduje o tym,
    // jak mocno one uwierają. Odwrotnie byłoby trudniej wytłumaczyć graczowi
    // („odszedł, bo menedżer" nie jest zdaniem, na którym da się coś zmienić).
    let skala = turnover_mult(mgmt, mt).max(0) as u32;
    let po_mnozeniu = (baza + najwiekszy).saturating_mul(skala) / 1_000;
    QuitPressure {
        per_10k: po_mnozeniu.min(10_000) as u16,
        cause,
    }
}

/// **Trzeci kanał wpływu menedżera** (M7c §5.4): mnożnik ciśnienia na odejście,
/// w tysięcznych.
///
/// Kierunek jest odwrotny niż przy produktywności i to jest cała treść tej funkcji:
/// zły menedżer **podnosi** rotację, dobry ją obniża. Rozpiętość jest wymaganiem,
/// nie kalibracją (M7 §8, `R6`) — przy 1 400 wobec 700 zakład źle prowadzony traci
/// dwa razy więcej ludzi niż ten sam zakład prowadzony dobrze, i to widać w panelu.
///
/// Do M7c ciśnienie na odejście nie zależało od zarządzania w żaden sposób, choć
/// tabela z §5.4 ten kanał obiecywała.
#[must_use]
pub fn turnover_mult(mgmt: ManagementQuality, t: &ManagerTuning) -> i32 {
    let zakres = t.turnover_mult_worst - t.turnover_mult_best;
    t.turnover_mult_worst - i32::from(mgmt.0) * zakres / 100
}

/// Czy firma zwalnia za wynik. Dwa warunki naraz, nie jeden: sama słaba ocena to
/// powód do rozmowy, a nie do wypowiedzenia.
#[must_use]
pub fn should_dismiss(perf_ema: u16, warnings: u8, t: &HrTuning) -> bool {
    perf_ema < t.dismiss_perf && warnings >= t.dismiss_warnings
}

/// Odprawa: dni stawki za każdy pełny rok stażu, z sufitem.
#[must_use]
pub fn severance(wage_month: Money, tenure_days: u32, t: &HrTuning) -> Money {
    let lata = tenure_days / 360;
    let dni = (lata * u32::from(t.severance_days_per_year)).min(u32::from(t.severance_days_max));
    // Miesiąc ma 30 dób (`K-1`), więc dniówka to stawka przez 30.
    Money(wage_month.get().saturating_mul(i64::from(dni)) / 30)
}

/// Nowa ocena wyniku po dobie pracy — wygładzanie wykładnicze na liczbach całkowitych.
///
/// `labor_milli` to produktywność pracownika w milietatach: 1000 znaczy pełny etat
/// o sprawności odniesienia, więc ocena jest po prostu przeskalowaną wydajnością.
/// Waga 1/16 daje stałą czasową rzędu dwóch tygodni — jeden gorszy dzień nie kosztuje
/// premii, dwa gorsze tygodnie kosztują.
#[must_use]
pub fn update_perf(perf_ema: u16, labor_milli: i64) -> u16 {
    let cel = labor_milli.clamp(0, 1_000) as i32;
    let stary = i32::from(perf_ema);
    (stary + (cel - stary) / 16).clamp(0, 1_000) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hr::tuning::LaborTuning;
    use magnat_core::LeaveCause;

    fn tuning() -> HrTuning {
        LaborTuning::load_default()
            .expect("data/tuning/labor.ron")
            .hr
    }

    fn mt() -> ManagerTuning {
        LaborTuning::load_default()
            .expect("data/tuning/labor.ron")
            .manager
    }

    /// Zarządzanie neutralne — punkt odniesienia wszystkich testów sprzed M7c.
    fn n() -> ManagementQuality {
        ManagementQuality::NEUTRAL
    }

    #[test]
    fn swiezo_zatrudniony_nie_odchodzi_tego_samego_tygodnia() {
        let t = tuning();
        assert_eq!(
            quit_pressure(-100, 100, 3, -5_000, n(), &t, &mt()).per_10k,
            0
        );
    }

    #[test]
    fn kazdy_kanal_ma_wlasny_powod() {
        let t = tuning();
        assert_eq!(
            quit_pressure(0, 0, 400, -6_000, n(), &t, &mt()).cause,
            LeaveCause::BetterOffer
        );
        assert_eq!(
            quit_pressure(0, 100, 400, 0, n(), &t, &mt()).cause,
            LeaveCause::Stress
        );
        assert_eq!(
            quit_pressure(-90, 0, 400, 0, n(), &t, &mt()).cause,
            LeaveCause::Mood
        );
    }

    #[test]
    fn zadowolony_i_dobrze_placony_prawie_nie_odchodzi() {
        let t = tuning();
        let p = quit_pressure(80, 10, 400, 1_500, n(), &t, &mt());
        assert!(p.per_10k <= t.quit_base_per_10k, "{}", p.per_10k);
        // Górny kraniec: najgorsza możliwa praca daje rotację rzędu 60 % rocznie,
        // a nie 100 % — nawet z fatalnej pracy ludzie odchodzą stopniowo.
        let fatalna = quit_pressure(-100, 100, 400, -10_000, n(), &t, &mt());
        assert!((20..=40).contains(&fatalna.per_10k), "{}", fatalna.per_10k);
    }

    #[test]
    fn menedzer_jest_trzecim_kanalem_i_dziala_monotonicznie() {
        // §5.4: mnożnik rotacji 1,40 przy zarządzaniu zerowym, 0,70 przy doskonałym.
        let (t, mt) = (tuning(), mt());
        let mut poprzedni = u16::MAX;
        for m in 0..=100u8 {
            let p = quit_pressure(-60, 80, 400, -3_000, ManagementQuality(m), &t, &mt);
            assert!(p.per_10k <= poprzedni, "rotacja wzrosła przy mgmt = {m}");
            poprzedni = p.per_10k;
        }
        assert_eq!(
            turnover_mult(ManagementQuality(0), &mt),
            mt.turnover_mult_worst
        );
        assert_eq!(
            turnover_mult(ManagementQuality(100), &mt),
            mt.turnover_mult_best
        );
        assert_eq!(turnover_mult(ManagementQuality::NEUTRAL, &mt), 1_050);
        // Rozpiętość ≥ ±30 % — menedżer ma być czuć także w rotacji (`R6`).
        let zly = quit_pressure(-60, 80, 400, -3_000, ManagementQuality(0), &t, &mt);
        let dobry = quit_pressure(-60, 80, 400, -3_000, ManagementQuality(100), &t, &mt);
        assert!(
            u32::from(zly.per_10k) * 10 >= u32::from(dobry.per_10k) * 18,
            "zły {} vs dobry {}",
            zly.per_10k,
            dobry.per_10k
        );
    }

    #[test]
    fn zwolnienie_wymaga_i_slabego_wyniku_i_upomnien() {
        let t = tuning();
        assert!(!should_dismiss(100, 0, &t));
        assert!(!should_dismiss(900, 9, &t));
        assert!(should_dismiss(100, t.dismiss_warnings, &t));
    }

    #[test]
    fn odprawa_rosnie_ze_stazem_i_ma_sufit() {
        let t = tuning();
        assert_eq!(
            severance(Money(300_000), 100, &t),
            Money::ZERO,
            "poniżej roku"
        );
        let rok = severance(Money(300_000), 400, &t);
        let dekada = severance(Money(300_000), 3_600, &t);
        assert!(rok.get() > 0 && dekada.get() > rok.get());
        assert_eq!(
            dekada,
            severance(Money(300_000), 36_000, &t),
            "sufit odprawy nie działa"
        );
    }

    #[test]
    fn ocena_wygladza_a_nie_skacze() {
        // Jeden fatalny dzień nie zbija oceny z 500 do zera.
        let po_jednym = update_perf(500, 0);
        assert!(po_jednym > 450, "{po_jednym}");
        // Ale miesiąc pełnej wydajności podnosi ją wyraźnie i nigdy ponad skalę.
        let mut p = 500;
        for _ in 0..30 {
            p = update_perf(p, 1_000);
        }
        assert!(p > 700 && p <= 1_000, "{p}");
    }
}
