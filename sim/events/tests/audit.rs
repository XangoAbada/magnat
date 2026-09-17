//! Kontrola skarbowa jako zdarzenie: kryterium ukończenia WP8 fazy M8d.
//!
//! „Firma ukrywa 30 % obrotu i kończy się to kontrolą w medianie < 3 lat gry"
//! jest zdaniem o **hazardzie**, a nie o urzędzie — kiedy kontrola przyjdzie,
//! rozstrzyga `sim/events`, a `sim/city` tylko zamienia jej przyjście w sprawę.
//! Dlatego mierzy się to tutaj, na samym rzucie, bez stawiania gospodarki:
//! trzy lata gry pełnego miasta to godziny przebiegu, a mierzyłyby dokładnie
//! tę samą liczbę z większym szumem.
//!
//! Drugi kierunek jest równie ważny i ma własny test: firma, która deklaruje
//! wszystko, ma nie być nękana kontrolami. Bez tego „hazard rośnie z udziałem"
//! byłoby prawdą także wtedy, gdyby rósł z 40 000 ppm do 45 000.

use magnat_core::Tick;
use magnat_events::{hazard_ppm, roll, EventCatalog, EventDef, Probe};

const KLUCZ: &str = "political/tax_audit";
/// Ile dób gry mierzymy. Pięć lat — żeby mediana trzech lat miała gdzie wypaść,
/// a przebieg bez ani jednej kontroli dał się odróżnić od przebiegu z kontrolą
/// w ostatniej dobie.
const DOB: u64 = 1_800;
/// Ile firm w próbce. Sto wystarcza na medianę i kosztuje sto tysięcy rzutów.
const FIRM: u64 = 100;

fn definicja(cat: &EventCatalog) -> (u16, &EventDef) {
    let i = cat
        .defs
        .iter()
        .position(|d| d.key == KLUCZ)
        .expect("`political/tax_audit` w data/events/political.ron");
    (u16::try_from(i).unwrap(), &cat.defs[i])
}

/// Mediana doby pierwszej kontroli przy zadanym udziale obrotu poza deklaracją.
///
/// `None` dla firmy, której kontrola nie odwiedziła przez cały przebieg — i to
/// jest informacja, a nie brak: mediana licząca takie firmy jako „doba 1800"
/// kłamałaby w dół tym mocniej, im rzadsze są kontrole.
fn mediana_pierwszej_kontroli(ukryte_bp: i64) -> (Option<u64>, usize) {
    let cat = EventCatalog::load_default().expect("data/events/");
    let (idx, def) = definicja(&cat);
    let ppm = hazard_ppm(
        def,
        |p| match p {
            Probe::FirmUnreportedBps => ukryte_bp,
            _ => 0,
        },
        None,
    );
    let mut dni: Vec<u64> = Vec::new();
    let mut bez = 0usize;
    for firma in 0..FIRM {
        let mut kiedy = None;
        for d in 0..DOB {
            if roll(42, idx, firma, Tick(d * 1_440), ppm) {
                kiedy = Some(d);
                break;
            }
        }
        match kiedy {
            Some(d) => dni.push(d),
            None => bez += 1,
        }
    }
    dni.sort_unstable();
    // Mediana całej próbki: firmy bez kontroli są w niej jako „później niż DOB",
    // więc jeśli jest ich więcej niż połowa, mediany nie ma.
    if dni.len() * 2 <= FIRM as usize {
        return (None, bez);
    }
    (Some(dni[FIRM as usize / 2]), bez)
}

#[test]
fn firma_ukrywajaca_30_procent_obrotu_dostaje_kontrole_w_medianie_ponizej_trzech_lat() {
    // Kryterium ukończenia WP8. Trzy lata gry to 1080 dób (`K-1`: rok = 360 dób).
    let (mediana, bez) = mediana_pierwszej_kontroli(3_000);
    let m = mediana.expect("ponad połowa firm ukrywających 30 % nie została skontrolowana");
    assert!(
        m < 1_080,
        "mediana pierwszej kontroli {m} dób ({} lat gry), a miała być < 3 lat; \
         {bez} firm ze stu nie skontrolowano przez pięć lat",
        m / 360
    );
}

#[test]
fn firma_deklarujaca_wszystko_nie_jest_nekana() {
    // Druga strona tego samego zdania. Przy zerowym udziale hazard schodzi do
    // dziesiątej części stawki bazowej i kontrola jest rzadkim wydarzeniem,
    // a nie corocznym rytuałem.
    let (mediana, bez) = mediana_pierwszej_kontroli(0);
    assert!(
        bez > 0,
        "przez pięć lat skontrolowano **każdą** ze stu uczciwych firm — \
         krzywa nie różnicuje"
    );
    if let Some(m) = mediana {
        assert!(
            m > 1_080,
            "uczciwa firma dostaje kontrolę w medianie {m} dób — częściej niż raz na trzy lata"
        );
    }
}

#[test]
fn hazard_rosnie_z_udzialem_i_nie_nasyca_sie_przedwczesnie() {
    // Krzywa ma **różnicować**, a nie tylko rosnąć: sonda, której trzy różne
    // wartości dają ten sam mnożnik, jest ozdobą (`R2` zastosowane do krzywej).
    let cat = EventCatalog::load_default().expect("data/events/");
    let (_, def) = definicja(&cat);
    let ppm = |bp: i64| {
        hazard_ppm(
            def,
            |p| match p {
                Probe::FirmUnreportedBps => bp,
                _ => 0,
            },
            None,
        )
    };
    let (zero, maly, sredni, duzy) = (ppm(0), ppm(500), ppm(1_500), ppm(3_000));
    assert!(zero < maly && maly < sredni && sredni < duzy);
    assert!(
        duzy >= zero * 20,
        "trzydzieści procent poza deklaracją podnosi hazard z {zero} do {duzy} ppm — za mało"
    );
}
