//! **T7 — zdarzenia nie zadają cen** (M8 §7, PRD §11.1).
//!
//! Test jest strukturalny i to jest jego istota: nie sprawdza, czy dzisiejszy
//! katalog przypadkiem nie rusza ceny, tylko czy **da się** taki wariant dopisać
//! bez łamania budowy. `match` bez ramienia `_` pęka przy każdym nowym wariancie
//! `SimParam` i `Effect`, więc dopisanie „ustaw cenę oferty" wymaga świadomego
//! przejścia przez ten plik — a nie da się przez niego przejść, nie czytając,
//! po co on jest.

use magnat_events::{Effect, SimParam};

/// Czy parametr wolno zmieniać zdarzeniu. Lista jest **zamknięta** i to ona jest
/// treścią zasady z §11.1.
fn dozwolony(p: SimParam) -> &'static str {
    match p {
        // Zdolność produkcyjna zakładu: plon, maszyna, obsada. Nie cena wyrobu.
        SimParam::SiteOutputMulBps(_) => "zdolność produkcyjna",
        // Dostępność i moc źródła sieci. Nie taryfa.
        SimParam::SourceOnline(..) => "dostępność źródła",
        SimParam::SourceCapacityMulBps(..) => "moc źródła",
        // Cena **u dostawcy spoza miasta**. Jedyny wariant dotykający pieniądza
        // i jedyny, który może — świat zewnętrzny nie jest niczyją firmą w tym mieście.
        SimParam::ExternalPriceMulBps(_) => "cena importu",
        // Danina. Polityka, nie cena: cło dolicza się osobno i nigdy nie wchodzi
        // do środka ceny w ofercie (`K-7`, `K-36`).
        SimParam::DutyBp(_) => "stawka celna",
        // Tempo spadku potrzeby. Zmienia, **kiedy** mieszkaniec pójdzie kupić,
        // a nie ile zapłaci ani ile kupi.
        SimParam::NeedDecayMulBps(_) => "tempo potrzeby",
    }
}

#[test]
fn zaden_parametr_nie_dotyka_ceny_w_miescie() {
    let wszystkie = [
        SimParam::SiteOutputMulBps(0),
        SimParam::SourceOnline(0, 0),
        SimParam::SourceCapacityMulBps(0, 0),
        SimParam::ExternalPriceMulBps(0),
        SimParam::DutyBp(0),
        SimParam::NeedDecayMulBps(0),
    ];
    for p in wszystkie {
        let opis = dozwolony(p);
        assert!(
            !opis.contains("marża") && !opis.contains("oferta") && !opis.contains("sprzedaż"),
            "parametr opisany jako `{opis}` dotyka ceny w mieście"
        );
    }
}

#[test]
fn katalog_uzywa_wylacznie_dozwolonych_efektow() {
    let c = magnat_events::EventCatalog::load_default().expect("data/events");
    for d in &c.defs {
        for e in &d.effects {
            match e {
                Effect::SiteOutput { mul_bps, .. } | Effect::SourceCapacity { mul_bps } => {
                    assert!(*mul_bps <= 30_000, "`{}`: mnożnik {mul_bps} bps", d.key);
                }
                Effect::SourceOnline | Effect::NeedDecay { .. } | Effect::Duty { .. } => {}
                Effect::ExternalPrice { good, .. } => {
                    // Cena zewnętrzna wolno ruszać wyłącznie towarom, które miasto
                    // **importuje**, a nie wytwarza u siebie. Klucz surowca albo
                    // paliwa to jedyne domeny, w których to twierdzenie jest prawdziwe.
                    let domena = good.split_once('_').map_or("", |(d, _)| d);
                    assert!(
                        matches!(domena, "raw" | "fuel" | "agri"),
                        "`{}` rusza cenę zewnętrzną towaru `{good}` spoza domen surowcowych",
                        d.key
                    );
                }
            }
        }
    }
}

#[test]
fn zrodla_nie_wspominaja_o_cenie_w_miescie() {
    // Strażnik na tekście, nie na typie — bo najprostszą drogą do złamania zasady
    // nie jest nowy wariant `SimParam`, tylko sięgnięcie do `Market` z modułu,
    // który ma prawo pisać do świata.
    for plik in ["src/param.rs", "src/apply.rs", "src/hazard.rs"] {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(plik);
        let tekst = std::fs::read_to_string(&p).expect("źródło");
        let kod: String = tekst
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for zakazane in [
            "unit_price",
            "Offer",
            "margin_bp",
            "demand_qty",
            "set_price",
        ] {
            assert!(
                !kod.contains(zakazane),
                "{plik} sięga po `{zakazane}` — zdarzenie nie zna ceny w mieście"
            );
        }
    }
}

#[test]
fn ani_jeden_float_na_sciezce_hazardu() {
    // T3: „w module hazardu i losowania zdarzeń nie występuje żaden typ
    // zmiennoprzecinkowy". Sprawdzane na źródle, bo typ, którego nie ma,
    // nie zostawia po sobie symbolu do odpytania.
    for plik in [
        "src/hazard.rs",
        "src/param.rs",
        "src/catalog.rs",
        "src/probe.rs",
    ] {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(plik);
        let tekst = std::fs::read_to_string(&p).expect("źródło");
        let kod: String = tekst
            .lines()
            .filter(|l| !l.trim_start().starts_with("//") && !l.trim_start().starts_with("///"))
            .collect::<Vec<_>>()
            .join("\n");
        for zakazane in ["f32", "f64", "as f6", "0.0"] {
            assert!(
                !kod.contains(zakazane),
                "{plik} zawiera `{zakazane}` — ścieżka hazardu jest całkowitoliczbowa"
            );
        }
    }
}
