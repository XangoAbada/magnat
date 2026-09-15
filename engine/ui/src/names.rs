//! Formatowanie nazwy mieszkańca (M4d §5.10 pkt 4, WP13).
//!
//! Formater stoi **po stronie prezentacji**, nie w `sim/agents`: nazwa jest tekstem
//! na ekranie, a to, co siedzi w komponencie, to dwa indeksy. Pulę ładuje `sim/agents`,
//! bo to ona losuje i potrzebuje długości pul jako zakresu losowania — tutaj jest
//! tylko odczyt.
//!
//! **To nie jest lokalizacja UI** (CLAUDE.md): „Anna Kowalska" brzmi tak samo
//! w polskiej i angielskiej wersji językowej, więc nazwa nie ma `LocKey` i nie wchodzi
//! do `data/locale/`. Lokalizowalna jest kolejność „imię nazwisko" — i pozostaje
//! zachodnia, bo taka jest w obu obsługiwanych językach; regionu, który ma odwrotną,
//! w `data/names/regions.ron` dziś nie ma.

use magnat_agents::components::Identity;

/// „Anna Kowalska" — imię z puli i nazwisko w formie zgodnej z płcią.
///
/// Forma żeńska bierze się z **pary form w danych**, nie z reguły sufiksowej:
/// „Kowalski → Kowalska", ale „Nowak" i „Schmidt" mają jedną formę dla obu płci.
#[must_use]
pub fn full_name(id: &Identity) -> String {
    let c = magnat_agents::name_catalog();
    let m = id.is_male();
    format!(
        "{} {}",
        c.first_name(id.first_name),
        c.surname(id.last_name, m)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn osoba(first: u16, last: u16, male: bool) -> Identity {
        Identity {
            first_name: first,
            last_name: last,
            flags: Identity::FLAG_ALIVE | if male { Identity::FLAG_MALE } else { 0 },
            ..Identity::default()
        }
    }

    /// §7.1 `nazwisko_dziedziczone_w_formie_wlasnej_plci`: rodzina dzieli **indeks**
    /// nazwiska, a forma dobiera się przy wypisywaniu. Nazwisko nieodmienne jest
    /// identyczne w obu formach i to też musi być prawdą.
    #[test]
    fn nazwisko_dziedziczone_w_formie_wlasnej_plci() {
        let c = magnat_agents::name_catalog();
        let mut odmienne = None;
        let mut nieodmienne = None;
        for i in 0..c.surname_len() as u16 {
            if c.surname(i, true) == c.surname(i, false) {
                nieodmienne.get_or_insert(i);
            } else {
                odmienne.get_or_insert(i);
            }
        }
        let od = odmienne.expect("pula bez nazwisk odmiennych");
        let nie = nieodmienne.expect("pula bez nazwisk nieodmiennych");

        // Ojciec i córka: ten sam indeks nazwiska, dwie formy.
        let ojciec = full_name(&osoba(0, od, true));
        let corka = full_name(&osoba(0, od, false));
        assert_ne!(
            ojciec.split_once(' ').unwrap().1,
            corka.split_once(' ').unwrap().1,
            "nazwisko odmienne nie odmieniło się: {ojciec} / {corka}"
        );

        let syn = full_name(&osoba(0, nie, true));
        let siostra = full_name(&osoba(0, nie, false));
        assert_eq!(
            syn.split_once(' ').unwrap().1,
            siostra.split_once(' ').unwrap().1,
            "nazwisko nieodmienne zostało odmienione: {syn} / {siostra}"
        );
    }

    #[test]
    fn karta_pokazuje_osobe_a_nie_numer() {
        let c = magnat_agents::name_catalog();
        let imie_zenskie = (0..c.first_len() as u16)
            .find(|i| !c.first_is_male(*i))
            .expect("pula bez imion żeńskich");
        let n = full_name(&osoba(imie_zenskie, 0, false));
        assert!(!n.contains('#'), "nadal numer: {n}");
        assert!(!n.contains('?'), "indeks bez wpisu w puli: {n}");
        assert_eq!(n.split(' ').count(), 2, "nazwa to imię i nazwisko: {n}");
    }
}
