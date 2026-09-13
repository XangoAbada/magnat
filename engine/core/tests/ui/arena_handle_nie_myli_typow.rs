//! K-16: uchwyt areny jest typowany zawartością, a encja nie jest uchwytem.
use magnat_core::{Arena, ArenaHandle, Entity};

struct Batch;
struct Offer;

fn wez_oferte(_h: ArenaHandle<Offer>) {}

fn main() {
    let mut partie: Arena<Batch> = Arena::new();
    let uchwyt_partii = partie.insert(Batch);

    // Uchwyt partii w miejscu uchwytu oferty.
    wez_oferte(uchwyt_partii);

    // Encja w miejscu uchwytu areny.
    let e: Entity = Entity::from_bits(0x0000_0001_0000_0000).unwrap();
    wez_oferte(e);

    // Uchwyt oferty w arenie partii.
    let oferty: Arena<Offer> = Arena::new();
    let _ = oferty.get(uchwyt_partii);
}
