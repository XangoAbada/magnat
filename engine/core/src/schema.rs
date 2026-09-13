//! `ComponentSchemaId` — tożsamość schematu komponentu (M0 §5.1a).
//!
//! Potrzebna M12 do migracji zapisów, ale musi istnieć od M0: inaczej pierwsze zapisy
//! nie mają czego wersjonować.
//!
//! Reguła dla faz: **zmiana układu pól komponentu to podbicie `Component::SCHEMA_VERSION`**,
//! nie zmiana jego nazwy. Nazwa identyfikuje komponent przez całe życie projektu, wersja
//! identyfikuje jego kształt — dopiero ta para pozwala M12 napisać migrację
//! „`Wallet@1` → `Wallet@2`" zamiast zgadywać.

use serde::{Deserialize, Serialize};
use xxhash_rust::const_xxh3::xxh3_64_with_seed;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[repr(transparent)]
pub struct ComponentSchemaId(pub u64);

impl ComponentSchemaId {
    /// Liczona w czasie kompilacji z pary (`Component::NAME`, `Component::SCHEMA_VERSION`).
    /// Wersja wchodzi jako ziarno, więc `Wallet@1` i `Wallet@2` to dwie różne tożsamości
    /// tej samej nazwy — i M12 widzi to bez dodatkowego pola w pliku.
    #[inline]
    #[must_use]
    pub const fn new(name: &str, version: u16) -> ComponentSchemaId {
        ComponentSchemaId(xxh3_64_with_seed(name.as_bytes(), version as u64))
    }
}

impl std::fmt::Display for ComponentSchemaId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "schema:{:016x}", self.0)
    }
}

// `name_hint()` z planu §5.1a NIE mieszka tutaj: wymagałby globalnego rejestru
// w `core`, który nie ma czego rejestrować. Nazwę do komunikatów błędów zna
// `ComponentRegistry` w `engine/ecs` (`registry.name_of_schema(id)`) — ten sam efekt
// bez stanu globalnego. Odnotowane w dzienniku 00-postep.md.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wersja_zmienia_tozsamosc() {
        let v1 = ComponentSchemaId::new("Wallet", 1);
        let v2 = ComponentSchemaId::new("Wallet", 2);
        assert_ne!(v1, v2);
        assert_eq!(v1, ComponentSchemaId::new("Wallet", 1));
    }

    #[test]
    fn nazwa_zmienia_tozsamosc() {
        assert_ne!(
            ComponentSchemaId::new("Wallet", 1),
            ComponentSchemaId::new("Walet", 1)
        );
    }

    #[test]
    fn liczona_w_czasie_kompilacji() {
        const ID: ComponentSchemaId = ComponentSchemaId::new("Position", 1);
        assert_eq!(ID, ComponentSchemaId::new("Position", 1));
    }
}
