//! Schemat pliku metryk przebiegu (§7.4 dokumentu fazy M5, kontrakt `BalansatorMetrics`).
//!
//! **Kontraktem jest schemat — nazwy pól i to, że rosną addytywnie — a nie składnia.**
//! Kontrakt §6 dokumentu fazy mówi „schemat JSON"; plik zapisujemy w RON i jest to
//! decyzja świadoma, nie przeoczenie:
//!
//! 1. `ron` jest już zależnością workspace'u, a całe `data/` w repozytorium jest w RON —
//!    drugi format serializacji byłby drugą konwencją bez drugiego powodu,
//! 2. jedynym konsumentem pliku jest podpolecenie `gate` w tym samym binarzu,
//! 3. `serde` sprawia, że zamiana na JSON to jedna linia w [`crate::run`] i [`crate::gates`],
//!    gdyby M12 dołożył analizę w Pythonie.
//!
//! Faza następna **dopisuje pola**, nie zmienia istniejących (kontrakt §6): stary plik
//! ma się dalej czytać, a [`SCHEMA_VERSION`] rośnie dopiero, gdy znaczenie pola się zmieni.

use serde::{Deserialize, Serialize};

/// Wersja schematu. Rośnie przy **zmianie znaczenia** pola, nie przy dopisaniu nowego.
pub const SCHEMA_VERSION: u32 = 1;

/// Jeden przebieg: jeden scenariusz, jedno ziarno, `days` dób.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct RunFile {
    pub schema_version: u32,
    pub scenario: String,
    pub seed: u64,
    pub days: u16,
    pub citizens: u32,
    pub shops: u32,
    /// Ciąg hashy stanu świata co dobę — wejście bramki G8.
    pub hashes: Vec<(u64, String)>,
    pub days_data: Vec<DayMetrics>,
    /// Towar, który dostał szok podaży — `None` w scenariuszach bez szoku.
    ///
    /// Pole **dopisane** do schematu §7.4 (kontrakt jest addytywny) i wymuszone
    /// przez pomiar: bez niego bramka G4 musiałaby zgadywać szokowany towar po
    /// tym, który najbardziej podrożał, a przy kilku ofertach na towar mediana
    /// skacze o 50 % z samego szumu i zgadywanie trafia w szum, nie w szok.
    #[serde(default)]
    pub shock_good: Option<u16>,
    /// Niezmiennik P1 w księgach na koniec przebiegu (bramka G7).
    pub conservation_ok: bool,
    /// Ile decyzji bez `DecisionReason` znalazł przebieg (bramka G9).
    pub decisions_without_reason: u64,
    pub decisions_sampled: u64,
    /// Trajektoria makro tego samego świata, próbkowana co dobę — wejście
    /// bramki G12 (M10f WP10.17, `E-13`).
    ///
    /// Pusta w plikach sprzed M10f i to jest cała wsteczna zgodność: bramka
    /// bez próbek **nie udaje, że zmierzyła**, tak samo jak G11 bez ciasnego
    /// rynku pracy.
    #[serde(default)]
    pub lod: Vec<LodSample>,
}

/// Jedna doba w obu trybach naraz — mezo z tykającego świata, makro z modelu
/// zdjętego w dobie zero i krokowanego obok (bramka G12).
///
/// Dwie liczby, a nie dwanaście, i to jest decyzja: kontrakt `K3` z §7.3
/// dokumentu M10 pyta o **dryf**, a nie o kompletność. Pieniądz gospodarstw
/// i bezrobocie wystarczą, bo pierwszy jest sumą wszystkich przepływów modelu,
/// a drugie wyjściem rynku pracy — jeśli któryś z nich dryfuje, dryfuje model,
/// a nie pojedyncza faza kroku.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct LodSample {
    pub day: u32,
    /// Gotówka plus depozyty gospodarstw, mezo.
    pub mezo_hh_money_gr: i64,
    /// To samo w makro: suma po komórkach.
    pub macro_hh_money_gr: i64,
    pub mezo_unemployment_permille: u16,
    pub macro_unemployment_permille: u16,
}

/// Próbka z granicy doby. Doba 0 to stan **przed** pierwszym tickiem — bramka G5
/// bierze z niej liczbę żywych kategorii jako punkt odniesienia.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct DayMetrics {
    pub day: u32,
    /// Rozkład cen per towar — z `BalanceSample.prices`.
    pub prices: Vec<PriceRow>,
    pub cpi_index_bp: i32,
    /// `None` = „nie ma jeszcze historii", a nie „inflacja zero" (`AA-7`).
    pub cpi_mom_bp: Option<i32>,
    pub cpi_yoy_bp: Option<i32>,
    pub base_rate_bp: i32,
    pub margin_median_bp: i32,
    pub shops: u32,
    pub insolvent: u32,
    pub hhi_median: i32,
    pub hhi_pairs: u32,
    pub live_categories: u32,
    /// Udział **ofert z pustą półką** wśród wszystkich ofert, w promilach.
    ///
    /// Metryka raportowa, nie bramkowa, i to rozróżnienie ma powód: od M5e
    /// dojrzały sklep **przestaje zamawiać towar, którego u niego nikt nie kupuje**,
    /// a jego oferta zostaje widoczna z `available == 0` („znam, nie ma" — §5.3).
    /// Taka linia jest pustą półką na zawsze i mówi o **asortymencie**, a nie
    /// o zdrowiu rynku. Zdrowie rynku mierzy `stockout_refusal_permille`.
    pub stockout_permille: i32,
    /// Udział prób zakupu zakończonych „brak towaru", w promilach — to jest
    /// `stockout_rate` z PRD §20.1 i to on stoi pod bramką G5.
    #[serde(default)]
    pub stockout_refusal_permille: i32,
    /// Udział decyzji zakończonych odłożeniem, w promilach — z przyrostu `MarketStats`.
    pub deferral_permille: i32,
    /// Przyrost doby.
    pub purchases: u64,
    /// Przyrost doby.
    pub revenue_gr: i64,
    pub reprices: u64,
    pub write_offs: u64,
    /// Korekta ★ po M5c: odpisy to **pierwszy** objaw złej kalibracji zamówień,
    /// więc mają własną kolumnę, a nie miejsce w rozkładzie marż.
    pub write_off_gr: i64,
    pub expired_qty: i64,
    pub loans: u32,
    pub credit_outstanding_gr: i64,
    pub money_supply_gr: i64,
    // ── warstwa firm (M7f WP17) ───────────────────────────────────────────────
    //
    // `serde(default)` we wszystkich pięciu, bo pliki przebiegów sprzed M7f mają
    // zostać czytelne — `SCHEMA_VERSION` rośnie dopiero wtedy, gdy zmienia się
    // **znaczenie** pola, a nie wtedy, gdy dochodzi nowe.
    /// Ile firm stoi w mieście tej doby — mianownik pasma z §7.10.
    #[serde(default)]
    pub firms: u32,
    /// Ile zakładów mają te firmy razem.
    #[serde(default)]
    pub firm_sites: u32,
    /// Bezrobocie w promilach siły roboczej.
    #[serde(default)]
    pub unemployment_permille: u16,
    /// Siła robocza: zatrudnieni plus bezrobotni.
    #[serde(default)]
    pub labour_force: u32,
    /// Nieobsadzone etaty w mieście.
    ///
    /// Mianownik bramki G11 i **powód, dla którego ona istnieje osobno od samego
    /// bezrobocia**: miasto, w którym etatów jest więcej niż ludzi, nie może mieć
    /// trzyprocentowego bezrobocia i pomiar niczego by o gospodarce nie powiedział.
    #[serde(default)]
    pub vacancies: u32,
    /// Firmy założone przez mieszkańców **od początku przebiegu**.
    #[serde(default)]
    pub firms_founded: u32,
    /// Firmy, które zniknęły — zwinięte dobrowolnie plus postępowania upadłościowe.
    #[serde(default)]
    pub firms_gone: u32,
}

/// Rozkład ceny jednego towaru po wszystkich ofertach miasta w danej dobie.
/// Percentyle liczy `Market::balance_sample` — tu są tylko przepisane.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct PriceRow {
    pub good: u16,
    pub min: i64,
    pub p10: i64,
    pub p50: i64,
    pub p90: i64,
    pub max: i64,
    pub offers: u32,
}

impl DayMetrics {
    /// Mediana ceny towaru w tej dobie; `None`, gdy towaru nie było w ofercie.
    #[must_use]
    pub fn p50_of(&self, good: u16) -> Option<i64> {
        self.prices.iter().find(|r| r.good == good).map(|r| r.p50)
    }
}
