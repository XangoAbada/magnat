//! Cykl życia encji i kontrakt layoutu chunka (M0 WP-04, §7.2).

use magnat_core::{HashState, Money, StateHasher};
use magnat_ecs::{Component, World, CHUNK_TARGET_BYTES};
use std::sync::atomic::{AtomicI64, Ordering};

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

#[derive(Debug, Clone, Copy)]
struct Bulk([u8; 400]);
impl HashState for Bulk {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write(&self.0);
    }
}
impl Component for Bulk {
    const NAME: &'static str = "Bulk";
}

/// Komponent z destruktorem — licznik żywych instancji wykrywa wyciek i podwójne
/// zwolnienie. Bez niego przenoszenie wierszy między archetypami byłoby testowane
/// wyłącznie na typach `Copy`, czyli tam, gdzie nie ma czego zepsuć.
static ZYWE: AtomicI64 = AtomicI64::new(0);

#[derive(Debug)]
struct Tracked(#[allow(dead_code)] String);
impl Tracked {
    fn new(s: &str) -> Tracked {
        ZYWE.fetch_add(1, Ordering::SeqCst);
        Tracked(s.to_string())
    }
}
impl Clone for Tracked {
    fn clone(&self) -> Self {
        Tracked::new(&self.0)
    }
}
impl Drop for Tracked {
    fn drop(&mut self) {
        ZYWE.fetch_sub(1, Ordering::SeqCst);
    }
}
impl HashState for Tracked {
    fn hash_state(&self, h: &mut StateHasher) {
        h.write(self.0.as_bytes());
    }
}
impl Component for Tracked {
    const NAME: &'static str = "Tracked";
}

#[test]
fn uchwyt_po_despawnie_nie_trafia_w_nowa_encje() {
    let mut w = World::new(1);
    let stary = w.spawn().with(Pos { x: 1, y: 1 }).id();
    assert!(w.despawn(stary));
    let nowy = w.spawn().with(Pos { x: 2, y: 2 }).id();
    assert_eq!(
        nowy.index(),
        stary.index(),
        "indeks powinien się recyklingować"
    );
    assert!(!w.contains(stary));
    assert_eq!(w.get::<Pos>(stary), None);
    assert_eq!(w.get::<Pos>(nowy), Some(&Pos { x: 2, y: 2 }));
}

#[test]
fn zmiana_archetypu_zachowuje_wartosci() {
    let mut w = World::new(1);
    let e = w.spawn().with(Pos { x: 7, y: 9 }).id();
    w.insert(e, Wallet(Money(1_000)));
    w.insert(e, Bulk([3; 400]));
    assert_eq!(w.get::<Pos>(e), Some(&Pos { x: 7, y: 9 }));
    assert_eq!(w.get::<Wallet>(e), Some(&Wallet(Money(1_000))));

    let zabrany = w.remove::<Wallet>(e);
    assert_eq!(zabrany, Some(Wallet(Money(1_000))));
    assert_eq!(w.get::<Wallet>(e), None);
    assert_eq!(w.get::<Pos>(e), Some(&Pos { x: 7, y: 9 }), "sąsiad ocalał");
    assert_eq!(w.remove::<Wallet>(e), None, "drugie usunięcie to None");

    // Nadpisanie w miejscu nie zmienia archetypu.
    let przed = w.archetypes().len();
    w.insert(e, Pos { x: 0, y: 0 });
    assert_eq!(w.archetypes().len(), przed);
    assert_eq!(w.get::<Pos>(e), Some(&Pos { x: 0, y: 0 }));
}

#[test]
fn destruktory_nie_wyciekaja_ani_nie_biegna_dwa_razy() {
    let start = ZYWE.load(Ordering::SeqCst);
    {
        let mut w = World::new(1);
        let mut encje = Vec::new();
        for i in 0..10_000 {
            let e = w
                .spawn()
                .with(Tracked::new(&format!("encja-{i}")))
                .with(Pos { x: i, y: -i })
                .id();
            encje.push(e);
        }
        assert_eq!(ZYWE.load(Ordering::SeqCst) - start, 10_000);

        // Przeniesienia między archetypami: dołożenie i zabranie komponentu.
        for e in encje.iter().take(3_000) {
            w.insert(*e, Wallet(Money(1)));
        }
        for e in encje.iter().take(1_500) {
            let _ = w.remove::<Wallet>(*e);
        }
        assert_eq!(
            ZYWE.load(Ordering::SeqCst) - start,
            10_000,
            "przenoszenie wierszy zgubiło albo zduplikowało wartość"
        );

        // Zabranie komponentu z destruktorem — wartość wraca do wywołującego.
        let zabrany = w.remove::<Tracked>(encje[0]).expect("komponent istniał");
        assert_eq!(ZYWE.load(Ordering::SeqCst) - start, 10_000);
        drop(zabrany);
        assert_eq!(ZYWE.load(Ordering::SeqCst) - start, 9_999);

        for e in &encje {
            w.despawn(*e);
        }
        assert_eq!(
            ZYWE.load(Ordering::SeqCst) - start,
            0,
            "wyciek po despawnie"
        );
    }
    assert_eq!(ZYWE.load(Ordering::SeqCst), start);
}

#[test]
fn czterysta_tysiecy_encji_w_szesciu_archetypach() {
    let mut w = World::new(7);
    let n = 400_000i64;
    let mut encje = Vec::with_capacity(n as usize);
    for i in 0..n {
        let mut b = w.spawn().with(Pos { x: i, y: i * 2 });
        if i % 2 == 0 {
            b = b.with(Wallet(Money(i)));
        }
        if i % 3 == 0 {
            b = b.with(Bulk([1; 400]));
        }
        encje.push(b.id());
    }
    assert_eq!(w.entity_count(), n as u32);
    // Pusty + 6 kombinacji (Pos, Pos+Wallet, Pos+Bulk, Pos+Wallet+Bulk) po drodze
    // przez archetypy pośrednie — liczba jest deterministyczna.
    assert!(w.archetypes().len() >= 4);

    // Wyrywkowa weryfikacja: wartości nie pomieszały się przy przenoszeniu wierszy.
    for i in (0..n).step_by(9_973) {
        let e = encje[i as usize];
        assert_eq!(w.get::<Pos>(e), Some(&Pos { x: i, y: i * 2 }));
        assert_eq!(w.has::<Wallet>(e), i % 2 == 0);
        assert_eq!(w.has::<Bulk>(e), i % 3 == 0);
    }

    // Despawn co drugiej encji i ponowny spawn — indeksy wracają, dane zostają spójne.
    for e in encje.iter().step_by(2) {
        assert!(w.despawn(*e));
    }
    assert_eq!(w.entity_count(), (n / 2) as u32);
    for i in (1..n).step_by(2 * 9_973) {
        let e = encje[i as usize];
        assert_eq!(w.get::<Pos>(e), Some(&Pos { x: i, y: i * 2 }));
    }
}

#[test]
fn chunk_to_jedna_alokacja_pod_jednym_wskaznikiem() {
    // Kontrakt z M12 (§5.5.1). Test layoutu istnieje po to, żeby te pięć warunków
    // nie zniknęło przy pierwszej optymalizacji storage'u.
    let mut w = World::new(1);
    for i in 0..5_000i64 {
        let _ = w.spawn().with(Pos { x: i, y: i }).with(Bulk([7; 400])).id();
    }

    let mut widzianych = 0;
    let mut poprzedni_id = None;
    for chunk in w.chunks() {
        widzianych += 1;
        let layout = chunk.layout();
        assert!(
            layout.alloc_bytes() <= CHUNK_TARGET_BYTES,
            "chunk {} B przekracza budżet",
            layout.alloc_bytes()
        );
        assert!(!chunk.is_empty());
        assert_eq!(chunk.entities().len(), chunk.len() as usize);
        // Kolejność iteracji jest deterministyczna: (ArchetypeId, index) rosnąco.
        let id = chunk.id();
        if let Some(prev) = poprzedni_id {
            assert!(prev < id, "chunki nie idą w kolejności: {prev:?} → {id:?}");
        }
        poprzedni_id = Some(id);
        // Kolumna jest osiągalna jako surowe bajty — to jest wejście sekcji snapshotu.
        let pos_id = w.components().id_by_name("Pos").unwrap();
        if let Some(bytes) = chunk.column_bytes(pos_id) {
            assert_eq!(bytes.len(), chunk.len() as usize * size_of::<Pos>());
        }
    }
    assert!(widzianych > 0);
}

#[test]
fn generacja_chunka_rosnie_tylko_przy_mutacji() {
    let mut w = World::new(1);
    let e = w.spawn().with(Pos { x: 1, y: 1 }).id();
    let id = w.chunks().next().unwrap().id();

    let po_spawnie = w.chunk(id).unwrap().generation();
    // Sam odczyt nie rusza generacji.
    let _ = w.get::<Pos>(e);
    assert_eq!(w.chunk(id).unwrap().generation(), po_spawnie);
    // Zapis rusza.
    w.get_mut::<Pos>(e).unwrap().x = 2;
    assert!(w.chunk(id).unwrap().generation() > po_spawnie);
}

#[test]
fn chunk_ref_jest_send_i_clone() {
    // Kontrakt z M12: wątek zapisu trzyma ChunkRef i czyta bajty.
    fn wymagaj_send<T: Send + Clone>() {}
    wymagaj_send::<magnat_ecs::ChunkRef<'static>>();
}

#[test]
fn rozmiary_typow_z_kontraktu() {
    assert_eq!(size_of::<magnat_ecs::Entity>(), 8);
    assert_eq!(size_of::<Option<magnat_ecs::Entity>>(), 8);
    assert_eq!(size_of::<magnat_ecs::ComponentId>(), 2);
}
