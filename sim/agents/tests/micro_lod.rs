//! `micro_writes_nothing` — deklaracja dostępu warstwy Mikro (M4d §5.4 i §7.2, WP9).
//!
//! Ten test jest **tańszy i mocniejszy niż jakikolwiek test przebiegowy**: przebieg
//! dowodzi, że mikro niczego nie zmieniło w tym scenariuszu, a deklaracja dowodzi,
//! że nie ma jak zmienić czegokolwiek w żadnym. Dopisanie komponentu ekonomicznego
//! do zapisów systemu Mikro wywala go natychmiast, zanim rozejdzie się po saldach.
//!
//! Patrzy na **system Mikro i tylko na niego** (`N-3`). Krok minutowy warstwy mezo
//! jest wyłączny (`K-21`) i deklaruje dostęp do całego świata — deklaracja przez
//! komponenty nic by tam nie kupiła, bo krok dotyka zbiornika, stanu technicznego,
//! położenia pojazdu, gotówki i kolejki zdarzeń naraz, a dałaby **fałszywą**
//! deklarację, gdyby ktoś czegoś nie wypisał.

use magnat_agents::{register, register_day, NeedTable, TravelMicroSystem};
use magnat_core::Cadence;
use magnat_ecs::{System, World};

#[test]
fn micro_writes_nothing() {
    let mut w = World::new(1);
    register(&mut w, NeedTable::load_default().expect("data/needs/"));
    register_day(&mut w);

    let system = TravelMicroSystem::new(&w);
    let desc = system.desc();

    assert!(
        desc.access.is_read_only(),
        "system Mikro `{}` zadeklarował zapis — warstwa wizualna dostała prawo \
         zmiany stanu symulacji i kamera gracza zaczyna zmieniać salda (00 §4)",
        desc.name
    );
    assert!(
        !desc.access.is_exclusive(),
        "system Mikro wziął świat na wyłączność — wizualizator nie ma po co blokować \
         reszty symulacji"
    );
    assert_eq!(
        desc.cadence,
        Cadence::EveryMicroTick,
        "kadencja warstwy Mikro przestała być tickiem mikro"
    );
}
