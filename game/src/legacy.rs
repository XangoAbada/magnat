//! Porażka i dziedziczenie (M9e WP12, §5.11, PRD §13.4).
//!
//! # Bankructwo nie kończy gry
//!
//! To jest cała treść §13.4: `GameState` zostaje `Playing`, a gracz wraca na etat
//! z długiem i popsutą reputacją. Nie ma ekranu porażki — jest wpis w kronice
//! o wysokiej ważności i inna sytuacja startowa. Gra, która kończy się przy
//! pierwszym błędzie finansowym, uczy ostrożności zamiast przedsiębiorczości.
//!
//! # Śmierć przychodzi z demografii, nie stąd
//!
//! Postać gracza jest zwykłym mieszkańcem i umiera tak samo jak każdy inny — M3
//! decyduje kiedy, a my sprawdzamy tylko, czy jeszcze żyje. Osobnej śmierci dla
//! gracza nie ma i mieć nie będzie.
//!
//! # Co dziedzic dostaje, a czego nie
//!
//! Przechodzi **własność firm i zobowiązania**, bo `Owner::Player` nie wskazuje
//! mieszkańca — jest rolą, a nie osobą. Nie przechodzą **relacje i umiejętności**,
//! bo należą do komponentu nowego mieszkańca i nikt ich nie przepisuje. To jest
//! prawdziwy koszt śmierci: majątek zostaje, sieć znajomości zaczyna się od zera.
//!
//! **Podatku spadkowego nie ma** i to nie jest przeoczenie: kodeks podatkowy M8 zna
//! siedem danin (`K-55`) i żadna z nich nie jest spadkową. Decyzja otwarta nr 8
//! dokumentu fazy zostaje otwarta — naliczanie stawki, której nie ma w `data/city/`,
//! byłoby wymyśleniem prawa po stronie gracza.

use magnat_core::CitizenId;

use crate::Session;

/// Co się właśnie stało postaci gracza.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LifeEvent {
    /// Postać nie żyje — czas na sukcesję.
    Died,
    /// Gospodarstwo ma ujemne saldo i żadnego zakładu z dodatnim kapitałem.
    Insolvent,
}

/// Sprawdza, czy z postacią gracza stało się coś, co zmienia stan gry.
///
/// Wołane raz na dobę z pętli gry. `None` jest normalną odpowiedzią i będzie nią
/// przez większość gry — dlatego sprawdzenie jest tanie: żywotność to jeden bit
/// w `Identity`, a saldo jedno pole gospodarstwa.
#[must_use]
pub fn check(session: &Session) -> Option<LifeEvent> {
    let p = session.player()?;
    let zyje = session
        .app
        .world
        .get::<magnat_agents::Identity>(p.citizen.entity())
        .is_some_and(magnat_agents::Identity::is_alive);
    if !zyje {
        return Some(LifeEvent::Died);
    }
    let saldo = crate::metrics::gotowka_gracza(session).get();
    if saldo >= 0 {
        return None;
    }
    let ma_z_czego = session.market.as_ref().is_some_and(|m| {
        crate::career::Holdings::of(session)
            .sites
            .iter()
            .any(|s| m.equity_of(*s).get() > 0)
    });
    (!ma_z_czego).then_some(LifeEvent::Insolvent)
}

/// Kto dziedziczy: **dorosły domownik o najniższym indeksie encji**.
///
/// `ponytail:` gospodarstwo jest jedyną więzią, o którą da się dziś zapytać tanio.
/// Rozróżnienie „dziecko → małżonek → rodzeństwo" wymaga stopnia pokrewieństwa,
/// a `Household` niesie listę członków, nie drzewo rodziny — pytanie o role
/// należałoby zadać `sim/agents`, a ono go dziś nie ma. Sufit jest widoczny
/// w zachowaniu: w gospodarstwie dwojga dorosłych dziedziczy małżonek, w gospodarstwie
/// z dorosłym dzieckiem wygrywa ten, kto urodził się wcześniej w świecie. Ścieżka
/// wyjścia: `RelationKind` po stronie M3 albo M10 (rody i kroniki dynastii).
///
/// Kolejność jest **deterministyczna i bez losowania** — indeks encji nie zależy
/// od niczego poza kolejnością powstania mieszkańca.
///
/// `None` znaczy „nie ma komu" i prowadzi do ekranu spuścizny, a nie do końca gry.
///
/// # Dlaczego gospodarstwo bierze się z dwóch miejsc
///
/// Zgon w `sim/agents` **despawnuje** mieszkańca w tej samej dobie, w której go
/// wykrywa (`demography::day::smierc` kończy się `cmd.despawn`), więc w chwili,
/// gdy gra pyta o dziedzica, `Identity` zmarłego już nie istnieje. Pytanie o nie
/// dawało wtedy `None` **zawsze**, czyli sukcesja po prawdziwej śmierci nigdy nie
/// miała kandydata, a `Succeed` i `SetHeir` były komendami bez drogi wejścia.
/// Dlatego indeks gospodarstwa czyta się z komponentu, gdy jeszcze jest, a z postaci
/// gracza, gdy już go nie ma — to jest to samo gospodarstwo, tylko zapamiętane
/// przy wyborze postaci.
#[must_use]
pub fn heir_of(session: &Session, of: CitizenId) -> Option<CitizenId> {
    let w = &session.app.world;
    let hh_index = w
        .get::<magnat_agents::Identity>(of.entity())
        .map(|i| i.household)
        .or_else(|| {
            session
                .player()
                .filter(|p| p.citizen == of)
                .map(|p| p.household.entity().index())
        })?;
    let hh_e = magnat_agents::household_by_index(w, hh_index)?;
    let gd = *w.get::<magnat_agents::Household>(hh_e)?;
    let overflow = w.resource::<magnat_agents::HouseholdOverflow>();
    let dzis = i32::try_from(session.tick().get() / magnat_core::time::MINUTES_PER_DAY)
        .unwrap_or(i32::MAX);
    let mut kandydaci: Vec<CitizenId> = magnat_agents::members_of(hh_index, &gd, overflow)
        .iter()
        .filter_map(|i| magnat_agents::citizen_by_index(w, *i).map(CitizenId))
        .filter(|c| *c != of)
        .filter(|c| {
            w.get::<magnat_agents::Identity>(c.entity())
                .is_some_and(|x| x.is_alive() && x.age_years(dzis) >= WIEK_DOROSLOSCI)
        })
        .collect();
    kandydaci.sort_unstable_by_key(|c| c.entity().index());
    kandydaci.first().copied()
}

/// Od ilu lat mieszkaniec dziedziczy. Ta sama granica, od której wolno pracować
/// bez zgody — dziedziczyć może ten, kto może prowadzić firmę.
const WIEK_DOROSLOSCI: i32 = 18;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn granica_dorslosci_jest_jedna() {
        assert_eq!(WIEK_DOROSLOSCI, 18, "wiek dziedziczenia zmienił się bez wpisu");
    }
}
