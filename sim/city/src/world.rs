//! Rozwiązanie kluczy tekstowych kodeksu na identyfikatory świata (M8a).
//!
//! Jedna funkcja, bo druga — budowa katastru z parceli generatora — stoi po
//! stronie mostu w `tools/headless`. Granica jest tu, gdzie powinna: `sim/city`
//! **nie zależy od `sim/world`**, bo miasto jako aktor fiskalny nie ma powodu znać
//! urbanistyki. Kataster wiąże parcelę z zakładem, czyli dwie strony, których żadna
//! nie widzi drugiej — i robi to ten, kto widzi obie.

use std::collections::BTreeMap;

use magnat_core::Money;

use crate::code::TaxCode;

/// Opłaty koncesyjne rozwiązane z kluczy tekstowych na `SiteTypeId`.
///
/// Klucz, którego nie ma w katalogu rodzajów zakładu, jest **pomijany po cichu**
/// i to jest jedyne miejsce w tej fazie, gdzie milczenie jest właściwą odpowiedzią:
/// katalog rodzajów zależy od epoki, a kodeks podatkowy od niej nie zależy. Miasto
/// epoki, w której nie ma jeszcze rafinerii, nie powinno odmawiać startu z powodu
/// wiersza o koncesji rafineryjnej.
#[must_use]
pub fn licenses_from_catalog(
    code: &TaxCode,
    catalog: &magnat_firms::SiteTypeCatalog,
) -> BTreeMap<u16, Money> {
    let mut out = BTreeMap::new();
    for l in &code.licenses {
        if let Some(id) = catalog.id(&l.site_type) {
            out.insert(id.0, Money(l.fee_per_year));
        }
    }
    out
}
