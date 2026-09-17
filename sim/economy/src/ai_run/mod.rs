//! Wykonawca decyzji AI firm (M7e WP10–WP12, WP14).
//!
//! # Podział: decyzja u firmy, wykonanie tutaj
//!
//! `sim/firms::ai` mówi **co zrobić** i nie wie, czym jest sklep — nie widzi
//! `sim/economy`, bo zależność idzie `economy → firms` i odwrócić się nie da.
//! Ten moduł robi trzy rzeczy, których tamten zrobić nie może: **zbiera fakty**
//! z półek, ksiąg i tablicy publicznej, **woła** regułę firmy i **wykonuje** jej wynik
//! na sterowniku ceny, regule zapasu i rejestrze firm.
//!
//! To ten sam podział, którym `D2` rozciął rynek pracy, a `AY-3` wykonawcę polityk:
//! mechanizm jest tam, gdzie dane, a decyzja tam, gdzie firma.
//!
//! # Trzy poziomy, trzy kadencje, jedna kolejka
//!
//! Kto dziś decyduje, mówi scheduler z M7a (`Firms::schedule`): kubełek minuty doby
//! dla tieru operacyjnego, dnia miesiąca dla taktycznego, dnia kwartału dla reakcji.
//! Budżet jest liczony **w firmach na tick**, a nadmiar czeka w kolejce FIFO, która
//! wchodzi do hasha stanu (00 §3.5, `R4`). Tutaj nie ma ani jednego pomiaru czasu.
//!
//! # Czego ten moduł nie robi
//!
//! Nie dotyka `&World`. Zamknięcie zakładu wymaga rozwiązania umów, a te siedzą
//! w komponentach mieszkańców — więc `run_firm_ai` **zwraca listę zakładów do
//! zamknięcia**, a domyka ją wołający, który świat widzi. Ta sama granica, którą
//! `absorb_settlements` trzyma wobec ksiąg: fakty wychodzą listą, wykonuje ten,
//! kto ma czym (`AI-1`).

use std::collections::BTreeMap;

use magnat_core::{Money, SiteId, Tick};
use magnat_ecs::World;
use magnat_firms::view::{CityFacts, FirmView, GoodFacts, SiteFacts};
use magnat_firms::{
    decide_operational, decide_reaction, decide_strategic, decide_tactical, FirmKey, Firms,
    StrategicOutlooks,
};
use magnat_policy::PolicyCatalog;

use crate::board::PublicMarketBoard;
use crate::market::Market;

pub mod apply;
pub mod facts;

/// Pokrycie zapasu powyżej tej liczby dób czyta się jako „nieskończone".
/// Ta sama stała co w wykonawcy polityk — dwie różne granice tego samego pojęcia
/// znaczyłyby, że reguła gracza i reguła AI widzą inny zapas.
pub use crate::policy_run::MAX_COVER;

/// Co doba AI firm zrobiła — licznik dla panelu, balansatora i testów.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FirmAiDay {
    pub firms_ops: u32,
    pub firms_tactical: u32,
    pub firms_strategic: u32,
    pub margin_moves: u32,
    pub restock_moves: u32,
    pub strategy_changes: u32,
    pub policies_adopted: u32,
    pub campaigns_started: u32,
    /// Zakłady, które tier taktyczny albo strategiczny postanowił zamknąć. Domyka
    /// je wołający — rozwiązanie umów dotyka komponentów mieszkańców, a rynek ich
    /// nie widzi.
    pub to_close: Vec<SiteId>,
    /// Zakłady, które tier strategiczny postanowił **otworzyć** (M7f WP13).
    /// Ta sama granica co przy `to_close` i z tego samego powodu: otwarcie stawia
    /// budynek i obsadza stanowiska, a rynek nie widzi ani parceli, ani mieszkańców.
    pub to_open: Vec<(FirmKey, magnat_core::DistrictId, u32, Money)>,
    /// Firmy, których właściciel postanowił zwinąć interes dobrowolnie (M7f WP15).
    pub to_wind_down: Vec<FirmKey>,
}

/// Wszystko, co firma AI czyta i czego nie zmienia.
///
/// Struktura, a nie cztery argumenty: po dołożeniu uporządkowań wariantów (M7f WP13)
/// lista wejść przekroczyła próg czytelności, a każde z nich jest **odczytem**
/// o tym samym czasie życia — więc jedna nazwa zamiast czterech pozycji, których
/// kolejność trzeba pamiętać.
pub struct AiInputs<'a> {
    /// Tablica publiczna cen z opóźnieniem (§5.8).
    pub board: &'a PublicMarketBoard,
    /// Presety polityk z `data/policies/`.
    pub catalog: &'a PolicyCatalog,
    /// Uporządkowania wariantów z modelu makro (M7f WP13); puste, gdy makra nie ma.
    pub outlooks: &'a StrategicOutlooks,
    /// Salda rachunków zakładów — `Books` i rejestr firm nie dają się pożyczyć naraz.
    pub cash: &'a BTreeMap<SiteId, Money>,
}

/// Wpina tablicę publiczną i katalog presetów do świata (M7e WP10).
///
/// Tablica **wchodzi do hasha stanu**: jest pamięcią miasta o cenach i steruje
/// decyzjami, więc jej rozjazd byłby rozjazdem gospodarki. Katalog presetów nie
/// wchodzi — jest danymi wczytanymi z `data/policies/`, tak samo jak tabela ról.
pub fn register_firm_ai(world: &mut World, board: PublicMarketBoard, catalog: PolicyCatalog) {
    world.insert_resource(board);
    world.register_resource_hash::<PublicMarketBoard>();
    world.insert_resource(catalog);
}

impl Market {
    /// Doba i miesiąc firm AI (M7e WP11, WP12, WP14).
    ///
    /// `due` to wynik `Firms::schedule` z tej minuty — jedyne wejście do decyzji firm.
    /// `cash` to salda rachunków zakładów, bo `Books` i rejestr firm nie dają się
    /// pożyczyć ze świata naraz (ten sam powód co przy `run_policies`).
    pub fn run_firm_ai(
        &self,
        firms: &mut Firms,
        inputs: &AiInputs<'_>,
        due: &[(magnat_firms::Tier, Vec<FirmKey>)],
        t: Tick,
    ) -> FirmAiDay {
        let AiInputs {
            board,
            catalog,
            outlooks,
            cash,
        } = inputs;
        let mut d = FirmAiDay::default();
        let mut sites: Vec<SiteFacts> = Vec::new();
        let mut goods: Vec<GoodFacts> = Vec::new();
        for (tier, klucze) in due {
            for key in klucze {
                let Some(gotowka) =
                    self.zbierz(firms, board, cash, *key, t, &mut sites, &mut goods)
                else {
                    continue;
                };
                let Some(firma) = firms.get(*key) else {
                    continue;
                };
                let v = FirmView {
                    key: *key,
                    cash: gotowka,
                    personality: firma.personality,
                    strategy: firma.strategy,
                    margin_floor_bp: self.widelki(firma.sites.first().copied()).0,
                    margin_ceiling_bp: self.widelki(firma.sites.first().copied()).1,
                    lag_days: FirmView::lag_for(firma.sites.len(), &firma.personality),
                    tick: t,
                    sites: &sites,
                    goods: &goods,
                    city: CityFacts::default(),
                    outlook: outlooks.get(*key),
                };
                match tier {
                    magnat_firms::Tier::Operational => {
                        d.firms_ops += 1;
                        for akcja in decide_operational(&v) {
                            self.wykonaj_ops(firms, *key, &akcja, t, &mut d);
                        }
                    }
                    magnat_firms::Tier::Tactical => {
                        d.firms_tactical += 1;
                        for akcja in decide_tactical(&v) {
                            self.wykonaj_tac(firms, catalog, *key, &akcja, t, &mut d);
                        }
                    }
                    magnat_firms::Tier::Strategic => {
                        d.firms_strategic += 1;
                        let trwa = firma.campaign;
                        // Kwartał niesie dwie niezależne decyzje i obie mogą paść
                        // w tej samej dobie: reakcja odpowiada na **zmierzoną**
                        // utratę udziału, wybór wariantu — na porównanie prognoz.
                        // Rozdzielenie ich na dwa tiery znaczyłoby, że firma
                        // odpowiada rywalowi w innym kwartale, niż planuje rozwój.
                        if let Some(akcja) = decide_reaction(&v, trwa) {
                            self.wykonaj_reakcje(firms, *key, &akcja, t, &mut d);
                        }
                        if let Some(akcja) = decide_strategic(&v) {
                            self.wykonaj_str(firms, *key, &akcja, t, &mut d);
                        }
                    }
                }
            }
        }
        d
    }
}
