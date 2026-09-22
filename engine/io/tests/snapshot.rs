//! Hash stanu i snapshot (M0 WP-09, WP-10; testy T-D4, T-D5, T-D13).

use magnat_core::{HashState, Money, StateHasher};
use magnat_ecs::{Component, ComponentRegistry, Entity, World};
use magnat_io::{
    load_world, read_directory, read_section, rewrite_sections, save_world, state_hash_parts,
    world_state_hash, SectionKind,
};

#[derive(Debug, Clone, Copy, PartialEq)]
struct Pos {
    x: i64,
    y: i64,
}
impl HashState for Pos {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write_i64(self.x);
        h.write_i64(self.y);
    }
}
impl Component for Pos {
    const NAME: &'static str = "Pos";
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Wallet(Money);
impl HashState for Wallet {
    fn hash_state(&self, h: &mut StateHasher) {
        self.0.hash_state(h);
    }
}
impl Component for Wallet {
    const NAME: &'static str = "Wallet";
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Bulk([u8; 128]);
impl HashState for Bulk {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write(&self.0);
    }
}
impl Component for Bulk {
    const NAME: &'static str = "Bulk";
}

fn rejestr() -> ComponentRegistry {
    let mut w = World::new(0);
    w.register_component::<Pos>();
    w.register_component::<Wallet>();
    w.register_component::<Bulk>();
    w.components().clone()
}

fn swiat(n: i64) -> World {
    let mut w = World::with_registry(7, rejestr());
    for i in 0..n {
        let mut e = w.spawn().with(Pos { x: i, y: -i });
        if i % 2 == 0 {
            e = e.with(Wallet(Money(i * 100)));
        }
        if i % 7 == 0 {
            e = e.with(Bulk([(i % 256) as u8; 128]));
        }
        let _ = e.id();
    }
    w
}

fn katalog_tymczasowy() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("magnat-io-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("katalog tymczasowy");
    dir
}

#[test]
fn hash_nie_zalezy_od_historii_spawnow() {
    // T-D5: ten sam stan logiczny osiągnięty dwiema drogami ma dać ten sam hash.
    let mut a = World::with_registry(1, rejestr());
    let e1 = a.spawn().with(Pos { x: 1, y: 1 }).id();
    let e2 = a.spawn().with(Pos { x: 2, y: 2 }).id();
    let _ = (e1, e2);

    let mut b = World::with_registry(1, rejestr());
    let smieci = b.spawn().with(Pos { x: 9, y: 9 }).id();
    let x1 = b.spawn().with(Pos { x: 1, y: 1 }).id();
    b.despawn(smieci);
    // Indeks 0 wraca do obiegu i dostaje wartość, którą w świecie `a` miała encja 0.
    let x2 = b.spawn().with(Pos { x: 2, y: 2 }).id();
    assert_ne!(x1.index(), x2.index());

    // Uporządkowanie po indeksie encji: w `b` encja o indeksie 0 ma Pos(2,2),
    // a o indeksie 1 — Pos(1,1). Żeby stany były logicznie identyczne, muszą się
    // zgadzać po indeksach.
    let mut c = World::with_registry(1, rejestr());
    let _ = c.spawn().with(Pos { x: 2, y: 2 }).id();
    let _ = c.spawn().with(Pos { x: 1, y: 1 }).id();

    assert_eq!(
        world_state_hash(&b),
        world_state_hash(&c),
        "hash zależy od historii, a nie od stanu"
    );
    assert_ne!(world_state_hash(&a), world_state_hash(&b));
}

#[test]
fn zmiana_jednego_bajtu_zmienia_hash() {
    let mut w = swiat(100);
    let przed = world_state_hash(&w);
    let e = w.query::<(Entity, &Pos), ()>().iter().next().unwrap().0;
    w.get_mut::<Pos>(e).unwrap().x += 1;
    assert_ne!(przed, world_state_hash(&w));
}

#[test]
fn roundtrip_zachowuje_hash_stanu() {
    let dir = katalog_tymczasowy();
    let sciezka = dir.join("roundtrip.mgs");
    let w = swiat(20_000);
    let przed = world_state_hash(&w);

    let raport = save_world(&w, &sciezka).expect("zapis");
    assert_eq!(raport.hash, przed);
    assert!(raport.sections > 3);

    let wczytany = load_world(&sciezka, &rejestr()).expect("odczyt");
    assert_eq!(world_state_hash(&wczytany), przed);
    assert_eq!(wczytany.entity_count(), w.entity_count());
    assert_eq!(wczytany.tick, w.tick);
    assert_eq!(wczytany.seed, w.seed);

    // Wartości, nie tylko hash.
    for e in [7i64, 1_000, 19_999] {
        let encja = wczytany
            .query_ref_pos(e)
            .unwrap_or_else(|| panic!("brak encji o Pos.x = {e}"));
        assert_eq!(encja.x, e);
    }
    std::fs::remove_file(&sciezka).ok();
}

/// Pomocnik testowy: znajdź komponent po wartości (świat z `swiat()` ma unikalne `x`).
trait ZnajdzPos {
    fn query_ref_pos(&self, x: i64) -> Option<Pos>;
}
impl ZnajdzPos for World {
    fn query_ref_pos(&self, x: i64) -> Option<Pos> {
        for arch in self.archetypes().iter() {
            let cid = self.components().id_by_name("Pos")?;
            if !arch.contains(cid) {
                continue;
            }
            for chunk_index in 0..arch.chunk_count() {
                let bytes = arch.column_bytes(chunk_index, cid)?;
                let rows = arch.chunk(chunk_index).len() as usize;
                for r in 0..rows {
                    let from = r * size_of::<Pos>();
                    let mut buf = [0u8; 16];
                    buf.copy_from_slice(&bytes[from..from + 16]);
                    let px = i64::from_ne_bytes(buf[0..8].try_into().unwrap());
                    let py = i64::from_ne_bytes(buf[8..16].try_into().unwrap());
                    if px == x {
                        return Some(Pos { x: px, y: py });
                    }
                }
            }
        }
        None
    }
}

#[test]
fn wznowienie_z_zapisu_jest_nieodroznialne_od_przebiegu_ciaglego() {
    // T-D4 w wersji bez schedulera: ten sam ciąg operacji, raz ciągły, raz przerwany
    // zapisem i wczytaniem. Kluczowy element to lista wolnych indeksów — bez niej
    // pierwszy spawn po wczytaniu dostałby inny numer.
    let dir = katalog_tymczasowy();
    let sciezka = dir.join("wznowienie.mgs");

    let operacje = |w: &mut World, od: i64, do_: i64| {
        let mut encje: Vec<Entity> = Vec::new();
        for i in od..do_ {
            encje.push(w.spawn().with(Pos { x: i, y: i }).id());
        }
        for e in encje.iter().step_by(3) {
            w.despawn(*e);
        }
    };

    let mut ciagly = World::with_registry(11, rejestr());
    operacje(&mut ciagly, 0, 500);
    operacje(&mut ciagly, 500, 1_000);

    let mut przerwany = World::with_registry(11, rejestr());
    operacje(&mut przerwany, 0, 500);
    save_world(&przerwany, &sciezka).expect("zapis");
    let mut wznowiony = load_world(&sciezka, &rejestr()).expect("odczyt");
    operacje(&mut wznowiony, 500, 1_000);

    assert_eq!(
        world_state_hash(&ciagly),
        world_state_hash(&wznowiony),
        "wznowienie rozjechało się z przebiegiem ciągłym"
    );
    std::fs::remove_file(&sciezka).ok();
}

#[test]
fn katalog_czyta_sie_bez_dekompresji_ladunku() {
    // T-D13: nagłówek i lista sekcji są nieskompresowane i adresowalne.
    let dir = katalog_tymczasowy();
    let sciezka = dir.join("katalog.mgs");
    let w = swiat(5_000);
    save_world(&w, &sciezka).expect("zapis");

    let start = std::time::Instant::now();
    let snapshot = read_directory(&sciezka).expect("katalog");
    let czas = start.elapsed();

    assert!(czas.as_millis() < 50, "katalog czytany {czas:?}");
    assert_eq!(snapshot.header.world_seed, 7);
    assert!(snapshot.sections.len() > 3);
    assert!(snapshot
        .sections
        .iter()
        .any(|s| matches!(s.kind, SectionKind::ComponentColumn { .. })));
    // Sekcje nie zachodzą na siebie i idą po kolei.
    for para in snapshot.sections.windows(2) {
        assert_eq!(para[0].byte_range.end, para[1].byte_range.start);
    }
    // Pojedyncza sekcja da się odczytać bez dotykania pozostałych.
    let sekcja = snapshot
        .sections
        .iter()
        .find(|s| s.kind == SectionKind::EntityTable)
        .unwrap();
    let bajty = read_section(&sciezka, sekcja).expect("sekcja");
    assert_eq!(bajty.len() as u64, sekcja.uncompressed_len);
    std::fs::remove_file(&sciezka).ok();
}

#[test]
fn przepisanie_pliku_nie_rusza_pozostalych_sekcji() {
    // T-D13 część druga: podmiana jednej sekcji zostawia resztę bajtowo nietkniętą.
    // To jest fundament migracji schematu w M12.
    let dir = katalog_tymczasowy();
    let zrodlo = dir.join("przed.mgs");
    let cel = dir.join("po.mgs");
    let w = swiat(1_000);
    save_world(&w, &zrodlo).expect("zapis");

    let przed = read_directory(&zrodlo).expect("katalog");
    let indeks = przed
        .sections
        .iter()
        .position(|s| matches!(s.kind, SectionKind::ComponentColumn { .. }))
        .expect("sekcja kolumny");
    let oryginal = read_section(&zrodlo, &przed.sections[indeks]).expect("ładunek");
    let mut zmieniony = oryginal.clone();
    zmieniony[0] ^= 0xFF;

    rewrite_sections(&zrodlo, &cel, &[(indeks, zmieniony.clone())]).expect("przepisanie");

    let po = read_directory(&cel).expect("katalog po");
    assert_eq!(po.sections.len(), przed.sections.len());
    for (i, sekcja) in po.sections.iter().enumerate() {
        let dane = read_section(&cel, sekcja).expect("ładunek po");
        let wzorzec = if i == indeks {
            zmieniony.clone()
        } else {
            read_section(&zrodlo, &przed.sections[i]).expect("ładunek przed")
        };
        assert_eq!(dane, wzorzec, "sekcja {i} zmieniła się wbrew oczekiwaniu");
    }
    std::fs::remove_file(&zrodlo).ok();
    std::fs::remove_file(&cel).ok();
}

#[test]
fn niezgodna_wersja_formatu_to_czytelny_blad() {
    let dir = katalog_tymczasowy();
    let sciezka = dir.join("wersja.mgs");
    let w = swiat(10);
    save_world(&w, &sciezka).expect("zapis");

    // Podmiana numeru wersji w nieskompresowanym nagłówku.
    let mut bajty = std::fs::read(&sciezka).expect("odczyt pliku");
    // magic(8) + dlugosc katalogu(8) + magic w nagłówku(8) → wersja formatu.
    bajty[24] = 99;
    std::fs::write(&sciezka, &bajty).expect("zapis pliku");

    match read_directory(&sciezka) {
        Err(magnat_io::IoError::UnsupportedVersion { found, expected }) => {
            assert_eq!(found, 99);
            assert_eq!(expected, magnat_io::FORMAT_VERSION);
        }
        inne => panic!("oczekiwano UnsupportedVersion, dostano {inne:?}"),
    }
    std::fs::remove_file(&sciezka).ok();
}

#[test]
fn przerwany_zapis_nie_niszczy_poprzedniego_pliku() {
    let dir = katalog_tymczasowy();
    let sciezka = dir.join("atomowy.mgs");
    let stary = swiat(100);
    save_world(&stary, &sciezka).expect("zapis");
    let hash_stary = world_state_hash(&stary);

    // Plik tymczasowy z poprzedniej, przerwanej próby nie może wpłynąć na odczyt.
    std::fs::write(sciezka.with_extension("tmp"), b"smieci").expect("tmp");
    let wczytany = load_world(&sciezka, &rejestr()).expect("odczyt");
    assert_eq!(world_state_hash(&wczytany), hash_stary);

    std::fs::remove_file(&sciezka).ok();
    std::fs::remove_file(sciezka.with_extension("tmp")).ok();
}

/// Raport rozbieżności ma wskazać **archetyp**, nie tylko tick (N1.11, `M0#8`).
/// Zmiana jednej encji zmienia hash dokładnie jednej części, a hash całości
/// liczony obok części jest tym samym co `world_state_hash` — części go nie zastępują.
#[test]
fn czesci_hasha_wskazuja_archetyp_rozjazdu() {
    let a = swiat(20);
    let mut b = swiat(20);
    let czesci_a = state_hash_parts(&a);
    assert_eq!(
        czesci_a,
        state_hash_parts(&b),
        "ten sam stan, te same części"
    );
    assert!(
        czesci_a.iter().any(|(n, _)| n == "Pos+Wallet"),
        "archetyp nazwany składem komponentów: {czesci_a:?}"
    );

    let encja = b
        .query::<(Entity, &Pos), ()>()
        .iter()
        .find(|(_, p)| p.x == 2)
        .expect("encja z Pos.x = 2")
        .0;
    b.get_mut::<Wallet>(encja).expect("encja 2 ma portfel").0 = Money(1);
    let rozne: Vec<String> = czesci_a
        .iter()
        .zip(state_hash_parts(&b))
        .filter(|((_, ha), (_, hb))| ha != hb)
        .map(|((n, _), _)| n.clone())
        .collect();
    assert_eq!(rozne, vec!["Pos+Wallet".to_string()]);
    assert_ne!(world_state_hash(&a), world_state_hash(&b));
}
